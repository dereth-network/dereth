//! The runtime terrain-texture compositor and its merge cache.
//!
//! The cell rotation, the terrain selection, the terrain and road-code lookups, the terrain and
//! road alpha searches, the temporary texture fill, the copy-and-tile step and the three image
//! operations under it.
//!
//! Palette and surface records are described in `docs/formats/13-palette-and-surfaces.md`.
//!
//! One rule governs this whole file: **the blend is integer arithmetic and alpha 0 means the overlay
//! wins.** `lerp(src, dst, a/255)` differs by up to one unit per channel across the entire
//! landscape because of the `a > 128 → a + 1` correction and the `>> 8`.

// The merge cache is keyed by MergeKey and only ever looked up; the surface *index* comes from a
// separate insertion-ordered Vec (`order`), which is what allocates and
// what a caller can observe.
// ORDER-OK: nothing in this crate iterates a HashMap for output.
use std::collections::HashMap;
use std::sync::Arc;

use dereth_assets::region::{Region, TerrainDesc, TexMerge};
use dereth_assets::world::CellLandblock;
use dereth_primitives::{DataId, RenderBackend, TextureData, TextureFormat, TextureHandle};

use crate::consts::{IMAGE_SHIFT, MIN_TEX_SIZE, ROAD_TERRAIN_TYPE, SIDE_VERTEX_COUNT};
use crate::land::mesh::Rotation;
use crate::narrow::{u16_of, u32_of, u8_of_u32, usize_of_i64};

/// `TexMerge`'s 32-bit surface key.
///
/// Bits 0-19 are the four terrain types, five bits each, in the order SW, SE, NE, NW from the top
/// of that field; bits 20-27 are the four 2-bit road values in the same corner order; bits 28+ are
/// the LOD/detail code (`palLod`), from which surface restoration re-derives the texture size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MergeKey(pub u32);

impl MergeKey {
    /// `key(a, b, c, d)` uses corner order SW, SE, NE, NW.
    #[must_use]
    pub const fn new(types: [u16; 4], roads: [u16; 4], pal_lod: u32) -> Self {
        let r = (roads[0] as u32) * 64
            + (roads[1] as u32) * 16
            + (roads[2] as u32) * 4
            + roads[3] as u32;
        Self(
            (r << 20)
                | ((types[0] as u32) << 15)
                | ((types[1] as u32) << 10)
                | ((types[2] as u32) << 5)
                | (types[3] as u32)
                | (pal_lod << 28),
        )
    }

    /// The four corner terrain types, as re-extracts them:
    /// `t[0] = (key>>15)&0x1F` (SW), `t[1] = (key>>10)&0x1F` (SE), `t[2] = (key>>5)&0x1F` (NE),
    /// `t[3] = key&0x1F` (NW).
    #[must_use]
    pub const fn corner_types(self) -> [u32; 4] {
        [
            (self.0 >> 15) & 0x1F,
            (self.0 >> 10) & 0x1F,
            (self.0 >> 5) & 0x1F,
            self.0 & 0x1F,
        ]
    }

    /// `key >> 28` — the palette-LOD code the surface restore reads the size back out of.
    #[must_use]
    pub const fn pal_lod(self) -> u32 {
        self.0 >> 28
    }
}

/// The cell rotation followed by the terrain selection.
///
/// Returns `(key, rotation, uniform)`.
///
/// The client computes **four** keys, one per rotation of the corner pattern, so that a single
/// cached surface can serve four orientations of the same terrain arrangement. Only the
/// palette-shifting path uses more than the first: the texture-merging path takes `keys[0]`
/// and `ROT_0` unconditionally (no cell rotation). The region from Throne of Destiny on texture
/// merges; the region before it palette shifts, and [`crate::land::palshift::select`] chooses its
/// texture and rotation. [`cell_rotation_keys`] exposes the full set for the rotation test.
#[must_use]
pub fn cell_rotation(
    lb: &CellLandblock,
    region: &Region,
    side_cell_count: usize,
    i: usize,
    j: usize,
) -> (MergeKey, Rotation, bool) {
    let (keys, uniform) = cell_rotation_keys(lb, region, side_cell_count, i, j);
    match &region.land_surf.pal_shift {
        // Palette shifting chooses a texture and rotation per cell, at its global cell position.
        Some(ps) => {
            let choice = crate::land::palshift::select(ps, &keys, cell_x(lb, i), cell_y(lb, j));
            (choice.key, choice.rotation, uniform)
        }
        None => (keys[0], Rotation::Rot0, uniform),
    }
}

/// The global cell column of cell row `i` of a landblock: the block's x times eight, plus `i`.
#[must_use]
pub fn cell_x(lb: &CellLandblock, i: usize) -> i32 {
    i32::try_from((lb.id.raw() >> 24) * 8).unwrap_or(0) + i32::try_from(i).unwrap_or(0)
}

/// The global cell row of cell column `j` of a landblock: the block's y times eight, plus `j`.
#[must_use]
pub fn cell_y(lb: &CellLandblock, j: usize) -> i32 {
    i32::try_from(((lb.id.raw() >> 16) & 0xFF) * 8).unwrap_or(0) + i32::try_from(j).unwrap_or(0)
}

/// The full four-key set from `cell_rotation`, in `Rotation` order, plus the `uniform` flag.
///
/// ```text
/// keys[ROT_0]   = key(SW, SE, NE, NW)
/// keys[ROT_90]  = key(SE, NE, NW, SW)
/// keys[ROT_180] = key(NE, NW, SW, SE)
/// keys[ROT_270] = key(NW, SW, SE, NE)
/// uniform = (all four road values equal) && (all four types equal) && road_SW == 0
/// ```
#[must_use]
pub fn cell_rotation_keys(
    lb: &CellLandblock,
    region: &Region,
    side_cell_count: usize,
    i: usize,
    j: usize,
) -> ([MergeKey; 4], bool) {
    let step = crate::consts::BLOCK_SIDE / side_cell_count;
    // Palette shifting (type 1) is the land surface of the software region of a dat set from before
    // Throne of Destiny.
    let pal_shifted = is_pal_shifted(region);
    let pal_lod = if pal_shifted || step == 1 { 1 } else { 4 };

    // The 9x9 terrain grid, sampled at the LOD step. The client writes the index as
    // `(a*9 + b) * step`, which is the same thing as `(a*step)*9 + b*step`.
    let word = |a: usize, b: usize| lb.terrain[(a * SIDE_VERTEX_COUNT + b) * step];
    let corners = [
        word(i, j),
        word(i + 1, j),
        word(i + 1, j + 1),
        word(i, j + 1),
    ];
    let roads = corners.map(|w| w & 3);
    let types = corners.map(|w| (w >> 2) & 0x1F);

    let rot = |k: usize| {
        MergeKey::new(
            [
                types[k % 4],
                types[(k + 1) % 4],
                types[(k + 2) % 4],
                types[(k + 3) % 4],
            ],
            [
                roads[k % 4],
                roads[(k + 1) % 4],
                roads[(k + 2) % 4],
                roads[(k + 3) % 4],
            ],
            pal_lod,
        )
    };
    let keys = [rot(0), rot(1), rot(2), rot(3)];
    let uniform = roads.iter().all(|&r| r == roads[0])
        && types.iter().all(|&t| t == types[0])
        && roads[0] == 0;
    (keys, uniform)
}

/// True when the region's land surface is palette shifting (`LandSurf.type != 0`): the region
/// before Throne of Destiny. From Throne of Destiny on the region texture merges.
#[must_use]
pub fn is_pal_shifted(region: &Region) -> bool {
    region.land_surf.surf_type != 0
}

// -------------------------------------------------------------------------------------------
// get_terrain / get_road_code
// -------------------------------------------------------------------------------------------

/// What decides: the base terrain type plus up to three overlay
/// terrain types with their corner masks.
///
/// Corner bit numbering is **bit0 = SW, bit1 = SE, bit2 = NE, bit3 = NW**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerrainChoice {
    /// `tex[0..4]` as terrain types; `tex[0]` is the base. `None` past the last overlay.
    pub tex: [Option<u32>; 4],
    /// `code[0..3]`; zero terminates. Each is a corner mask, or a two-corner "side" mask.
    pub code: [u32; 3],
}

/// The terrain lookup.
///
/// Note the asymmetry the transcription makes explicit: the adjacent pair (NW, SW) = mask 9 is
/// never produced, because the merge only fires for `i` immediately after the recorded corner.
/// That configuration falls through to two separate corner overlays (masks 1 and 8), and so do two
/// opposite corners.
#[must_use]
pub fn get_terrain(key: MergeKey) -> TerrainChoice {
    let t = key.corner_types();
    let mut out = TerrainChoice::default();

    // find the smallest i such that t[i] == t[j] for some j > i
    let base = (0..4).find(|&i| t.iter().skip(i + 1).any(|&o| o == t[i]));

    let Some(bi) = base else {
        // All four corners distinct: the SW corner is the base and the other three are overlaid
        // with their own corner masks.
        out.tex = [Some(t[0]), Some(t[1]), Some(t[2]), Some(t[3])];
        out.code = [2, 4, 8];
        return out;
    };

    let base = t[bi];
    out.tex[0] = Some(base);
    let mut second: Option<u32> = None;
    for (i, &ti) in t.iter().enumerate() {
        if ti == base {
            continue;
        }
        if out.code[0] == 0 {
            out.code[0] = 1 << i;
            out.tex[1] = Some(ti);
            second = Some(ti);
            continue;
        }
        if second == Some(ti)
            && ((i == 1 && out.code[0] == 1)
                || (i == 2 && out.code[0] == 2)
                || (i == 3 && out.code[0] == 4))
        {
            out.code[0] += 1 << i; // an adjacent-pair "side" mask: 3, 6 or 12
            return out;
        }
        out.tex[2] = Some(ti);
        out.code[1] = 1 << i;
        return out;
    }
    out
}

/// What the road-code lookup decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoadChoice {
    /// The whole cell is road; the base texture is replaced rather than overlaid.
    pub all_road: bool,
    /// Up to two road masks; zero terminates. Three-corner roads are drawn as two overlapping side
    /// masks.
    pub codes: [u32; 2],
}

/// The road-code lookup.
#[must_use]
pub fn get_road_code(key: MergeKey) -> RoadChoice {
    let k = key.0;
    let rc = u32::from(k & 0x0C00_0000 != 0)          // SW road, bits 26-27
        | (u32::from(k & 0x0300_0000 != 0) << 1)       // SE road, bits 24-25
        | (u32::from(k & 0x00C0_0000 != 0) << 2)       // NE road, bits 22-23
        | (u32::from(k & 0x0030_0000 != 0) << 3); // NW road, bits 20-21
    match rc {
        0 => RoadChoice::default(),
        15 => RoadChoice {
            all_road: true,
            codes: [0, 0],
        },
        7 => RoadChoice {
            all_road: false,
            codes: [3, 6],
        }, // SW, SE, NE
        11 => RoadChoice {
            all_road: false,
            codes: [9, 3],
        }, // SW, SE, NW
        13 => RoadChoice {
            all_road: false,
            codes: [9, 12],
        }, // SW, NE, NW
        14 => RoadChoice {
            all_road: false,
            codes: [6, 12],
        }, // SE, NE, NW
        _ => RoadChoice {
            all_road: false,
            codes: [rc, 0],
        },
    }
}

// -------------------------------------------------------------------------------------------
// The alpha-map lookup
// -------------------------------------------------------------------------------------------

/// The pseudo-random alpha-map index both lookups derive from the surface key:
///
/// ```text
/// h = (int32)(key * 0x523AA99E - 0x51C9E74A)
/// idx = (int)floor(((uint32)h * 2^-32) * count)
/// if (float)(uint)idx >= (float)count: idx = 0
/// ```
#[must_use]
pub fn alpha_map_index(key: MergeKey, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let h = key.0.wrapping_mul(0x523A_A99E).wrapping_sub(0x51C9_E74A);
    #[allow(clippy::cast_precision_loss)] // exactly what the client's float path does
    let f = (h as f32) * 2.328_306_4e-10 * (count as f32);
    let idx = dereth_primitives::num::floor_to_i32(f);
    // The client's guard is `(float)(uint)idx >= (float)count`, i.e. it reinterprets a negative
    // index as a huge unsigned one and therefore catches it too.
    let idx = idx as u32 as usize;
    if idx >= count {
        0
    } else {
        idx
    }
}

/// `c*2` with `-15` on overflow — a 4-bit cyclic rotate of the corner mask
/// (1→2→4→8→1, 3→6→12→9→3).
#[must_use]
pub const fn rotate_code(c: u32) -> u32 {
    let c = c * 2;
    if c > 15 {
        c - 15
    } else {
        c
    }
}

/// One alpha map: which of the region's `CodeTexture` lists it came from, and how far it must be
/// rotated to present the wanted mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaMap {
    pub tex_gid: DataId,
    pub rotation: Rotation,
}

/// The terrain alpha search.
///
/// Uses `corner_terrain_maps` when the code is 1, 2, 4 or 8 and `side_terrain_maps` otherwise, then
/// takes **exactly** `maps[idx]` and rotates its `tcode` until it equals the wanted code. If four
/// rotations do not reach it, the lookup fails — the client returns 0 and the overlay is skipped.
#[must_use]
pub fn find_terrain_alpha(tm: &TexMerge, code: u32, key: MergeKey) -> Option<AlphaMap> {
    let maps = if matches!(code, 1 | 2 | 4 | 8) {
        &tm.corner_terrain_maps
    } else {
        &tm.side_terrain_maps
    };
    if maps.is_empty() {
        return None;
    }
    let m = &maps[alpha_map_index(key, maps.len())];
    let mut c = m.code;
    let mut rot = Rotation::Rot0;
    while c != code {
        c = rotate_code(c);
        rot = rot.next()?;
    }
    Some(AlphaMap {
        tex_gid: m.tex_gid,
        rotation: rot,
    })
}

/// The road alpha search.
///
/// Differs from [`find_terrain_alpha`]: it **scans all** `road_maps` starting at `idx` modulo the
/// count until one rotates to the wanted code.
#[must_use]
pub fn find_road_alpha(tm: &TexMerge, code: u32, key: MergeKey) -> Option<AlphaMap> {
    let maps = &tm.road_maps;
    if maps.is_empty() {
        return None;
    }
    let start = alpha_map_index(key, maps.len());
    for n in 0..maps.len() {
        let m = &maps[(start + n) % maps.len()];
        let mut c = m.code;
        let mut rot = Some(Rotation::Rot0);
        while let Some(r) = rot {
            if c == code {
                return Some(AlphaMap {
                    tex_gid: m.tex_gid,
                    rotation: r,
                });
            }
            c = rotate_code(c);
            rot = r.next();
        }
    }
    None
}

/// Select `terrain_desc[k]` for the entry whose `terrain_type == t`, falling
/// back to `terrain_desc[0]` when the type is absent.
#[must_use]
pub fn get_terrain_tex(tm: &TexMerge, t: u32) -> Option<&TerrainDesc> {
    tm.terrain_desc
        .iter()
        .find(|d| d.terrain_type == t)
        .or_else(|| tm.terrain_desc.first())
}

// -------------------------------------------------------------------------------------------
// The pixels
// -------------------------------------------------------------------------------------------

/// A decoded BGRA8 image. `dst[0] = B`, `[1] = G`, `[2] = R`, `[3] = A`, which is the byte order
/// used by the client's image textures and by the texture merge operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bgra8 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Bgra8 {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0; 4]; (width * height) as usize],
        }
    }

    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }

    /// The buffer laid out for upload, `TextureFormat::Bgra8`.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.pixels.into_iter().flatten().collect()
    }
}

/// per destination texel.
///
/// `a` is the alpha-map texel. `a == 255` leaves `dst` alone; **`a == 0` means the overlay wins**.
/// The alpha channel of `dst` is untouched.
#[inline]
pub fn integer_blend(dst: &mut [u8; 4], src: [u8; 4], a: u8) {
    if a == 255 {
        return;
    }
    let ap = if a > 128 {
        u32::from(a) + 1
    } else {
        u32::from(a)
    };
    let ia = 256 - ap;
    for (d, &s) in dst.iter_mut().zip(src.iter()).take(3) {
        // LINT-OK: the sum is at most 255*256, so the >> 8 lands inside u8 by construction.
        *d = u8_of_u32((u32::from(s) * ia + u32::from(*d) * ap) >> 8);
    }
}

/// The alpha map's walk for each rotation:
///
/// | rotation | start offset | column step | row step |
/// |---|---|---|---|
/// | ROT_0 | 0 | +1 | +w |
/// | ROT_90 | `w - 1` | +w | -1 |
/// | ROT_180 | `w·h - 1` | -1 | -w |
/// | ROT_270 | `(h-1)·w` | -w | +1 |
#[must_use]
pub fn rotated_offset(rot: Rotation, w: u32, h: u32, col: u32, row: u32) -> usize {
    let (w, h, col, row) = (w as i64, h as i64, col as i64, row as i64);
    let idx = match rot {
        Rotation::Rot0 => col + row * w,
        Rotation::Rot90 => (w - 1) + col * w - row,
        Rotation::Rot180 => (w * h - 1) - col - row * w,
        Rotation::Rot270 => (h - 1) * w - col * w + row,
    };
    usize_of_i64(idx)
}

/// Fill a `size x size` buffer with `tex`
/// tiled `tiling` times in each direction.
///
/// With a null texture the client fills every pixel with the bytes `00 FF 00 00` — an opaque-less
/// green debug colour — and that is reproduced, because a missing terrain texture is visible.
///
/// the retail resampling uses integer replication or skipping
/// (a step of `size/alpha_width` or `alpha_width/size`) rather than filtering, but the exact index arithmetic
/// inside the inner loop remains unconfirmed. This implementation preserves that nearest-neighbour
/// behavior.
#[must_use]
pub fn copy_and_tile(size: u32, tex: Option<&Bgra8>, tiling: u32) -> Bgra8 {
    let mut out = Bgra8::new(size, size);
    let Some(tex) = tex else {
        out.pixels.fill([0x00, 0xFF, 0x00, 0x00]);
        return out;
    };
    let tiling = tiling.max(1);
    for y in 0..size {
        for x in 0..size {
            let sx = (x * tiling * tex.width / size) % tex.width;
            let sy = (y * tiling * tex.height / size) % tex.height;
            out.pixels[(y * size + x) as usize] = tex.get(sx, sy);
        }
    }
    out
}

/// Blend `src` (tiled by its own `tiling`) into `dst` through
/// `alpha` (walked at `rot`), one destination texel at a time through [`integer_blend`].
///
/// The alpha map's red channel is the mask: image-texture alpha maps are 8-bit images loaded as BGRA,
/// so all three colour channels carry the same value.
///
/// UNVERIFIED, as [`copy_and_tile`].
pub fn merge_overlay(
    dst: &mut Bgra8,
    alpha: &Bgra8,
    rot: Rotation,
    src: Option<&Bgra8>,
    tiling: u32,
) {
    let size = dst.width;
    let tiling = tiling.max(1);
    for y in 0..size {
        for x in 0..size {
            let ax = x * alpha.width / size;
            let ay = y * alpha.height / size;
            // Index 3 is A. The alpha map is a `PFID_CUSTOM_LSCAPE_ALPHA` surface, which decodes
            // alpha-only: the mask is in the alpha channel and B/G/R are left at zero. Reading a
            // colour channel here yields a constant 0, which `integer_blend` reads as "the overlay
            // wins outright" for every texel -- so every overlay hard-replaces the base, the last
            // one wins the whole cell, and the landscape shows flat per-cell patches with no
            // blending at all — which is visibly wrong from the air.
            let a = alpha.pixels[rotated_offset(rot, alpha.width, alpha.height, ax, ay)][3];
            let s = match src {
                Some(t) => {
                    let sx = (x * tiling * t.width / size) % t.width;
                    let sy = (y * tiling * t.height / size) % t.height;
                    t.get(sx, sy)
                }
                None => [0x00, 0xFF, 0x00, 0x00],
            };
            integer_blend(&mut dst.pixels[(y * size + x) as usize], s, a);
        }
    }
}

/// Where the compositor gets its decoded pixels. Texture decode (DXT, JPEG, palette expansion) is
/// the renderer's and the asset decoder's work, so it arrives here already expanded to BGRA8.
///
/// Shared rather than owned: the same few terrain tiles and alpha maps feed every merge in the
/// window, so a source that memoises them hands out the one decoded copy instead of a fresh
/// megabyte per request.
pub trait TerrainTextureSource {
    /// The decoded pixels of one `SurfaceTexture`/`RenderSurface` id, or `None` when it is missing,
    /// which the client renders as the `00 FF 00 00` debug colour rather than skipping.
    fn image(&self, id: DataId) -> Option<Arc<Bgra8>>;
}

impl<F: Fn(DataId) -> Option<Bgra8>> TerrainTextureSource for F {
    fn image(&self, id: DataId) -> Option<Arc<Bgra8>> {
        self(id).map(Arc::new)
    }
}

/// Land-texture scale derived from the landscape texture-detail render preference:
/// `pref == 0 ? FULL_RES : pref - 1`. Note the shift table is `{0,1,2,4,8}`, so pref 4 shifts by
/// **4**, not 3.
#[must_use]
pub fn land_texture_scale_shift(pref: u32) -> u32 {
    let scale = if pref == 0 { 0 } else { (pref - 1) as usize };
    IMAGE_SHIFT[scale.min(IMAGE_SHIFT.len() - 1)]
}

/// The temporary texture fill's size rule:
/// `max(MIN_TEX_SIZE, (base_tex_size >> IMAGE_SHIFT[landscape texture scale]) / pal_lod)`.
#[must_use]
pub fn merged_texture_size(base_tex_size: u32, shift: u32, pal_lod: u32) -> u32 {
    let shifted = base_tex_size.checked_shr(shift).unwrap_or(0);
    MIN_TEX_SIZE.max(shifted / pal_lod.max(1))
}

/// One source image as the composite at `size` texels per cell carries it, for drawing the
/// landscape by blending the sources in the pixel shader rather than through composites.
///
/// A composite of `size` texels tiles a texture `tiling` times, so each repeat holds
/// `size / tiling` texels, picked by the same stride the composite takes; an alpha map
/// (`tiling` 1) holds `size`. An image already at or under that is returned unchanged, so at the
/// highest texture detail the shader samples the full source. Lower detail settings shrink `size`
/// ([`merged_texture_size`]) and the sources with it, which is what the setting does to the
/// composites.
#[must_use]
pub fn source_at_scale(img: &Bgra8, size: u32, tiling: u32) -> Option<Bgra8> {
    let per_repeat = (size / tiling.max(1)).max(1);
    let w = img.width.min(per_repeat);
    let h = img.height.min(per_repeat);
    if (w, h) == (img.width, img.height) {
        return None;
    }
    let mut out = Bgra8::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.pixels[(y * w + x) as usize] = img.get(x * img.width / w, y * img.height / h);
        }
    }
    Some(out)
}

/// Composite one merge key into a BGRA8 buffer.
///
/// ```text
/// size = max(MIN_TEX_SIZE (8), (base_tex_size >> IMAGE_SHIFT[landscape texture scale]) / pal_lod)
/// get_terrain(key) -> tex[4], code[3]
/// road_tex = get_terrain_tex(0x20)
/// get_road_code(key) -> all_road, road_codes
/// if all_road and road_tex: tex[0] = road_tex
/// copy_and_tile(size, tex[0])
/// if not all_road:
///     for k in 0..3: if code[k] == 0 break; find_terrain_alpha; merge_overlay(.., tex[k+1])
///     if road_tex:
///         for k in 0..2: if road_codes[k] == 0 break; find_road_alpha; merge_overlay(.., road_tex)
/// ```
#[must_use]
pub fn fill_temp_tex_buffer(
    tm: &TexMerge,
    key: MergeKey,
    shift: u32,
    src: &dyn TerrainTextureSource,
) -> Bgra8 {
    execute_merge_plan(&merge_plan(tm, key, shift), src)
}

/// One overlay pass of a [`MergePlan`]: `tex` tiled `tiling` times, blended in through the alpha
/// map `alpha` walked at `rotation`. `tex` is `None` when the terrain type names no texture, which
/// blends the debug colour, exactly as a missing image does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeOverlay {
    pub alpha: DataId,
    pub rotation: Rotation,
    pub tex: Option<DataId>,
    pub tiling: u32,
}

/// What [`fill_temp_tex_buffer`] composites for one merge key, by dat id, before any pixel is
/// read: a `size x size` base tiled from `base`, then each overlay in order. Splitting the choice
/// from the pixels is what lets the same composite run on the CPU ([`execute_merge_plan`]) or on
/// a device that can do the identical integer arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    pub size: u32,
    pub base: Option<DataId>,
    pub base_tiling: u32,
    pub overlays: Vec<MergeOverlay>,
}

/// The layers `key` composites, in the order the client applies them.
#[must_use]
pub fn merge_plan(tm: &TexMerge, key: MergeKey, shift: u32) -> MergePlan {
    let size = merged_texture_size(tm.base_tex_size, shift, key.pal_lod());
    let choice = get_terrain(key);
    let road = get_road_code(key);
    let road_tex = get_terrain_tex(tm, ROAD_TERRAIN_TYPE);

    let desc_of = |t: Option<u32>| t.and_then(|t| get_terrain_tex(tm, t));
    let base_desc = if road.all_road && road_tex.is_some() {
        road_tex
    } else {
        desc_of(choice.tex[0])
    };
    let mut plan = MergePlan {
        size,
        base: base_desc.map(|d| d.tex_gid),
        base_tiling: base_desc.map_or(1, |d| d.tex_tiling),
        overlays: Vec::new(),
    };

    if !road.all_road {
        for k in 0..3 {
            if choice.code[k] == 0 {
                break;
            }
            let Some(map) = find_terrain_alpha(tm, choice.code[k], key) else {
                continue;
            };
            let d = desc_of(choice.tex[k + 1]);
            plan.overlays.push(MergeOverlay {
                alpha: map.tex_gid,
                rotation: map.rotation,
                tex: d.map(|d| d.tex_gid),
                tiling: d.map_or(1, |d| d.tex_tiling),
            });
        }
        if let Some(rd) = road_tex {
            for k in 0..2 {
                if road.codes[k] == 0 {
                    break;
                }
                let Some(map) = find_road_alpha(tm, road.codes[k], key) else {
                    continue;
                };
                plan.overlays.push(MergeOverlay {
                    alpha: map.tex_gid,
                    rotation: map.rotation,
                    tex: Some(rd.tex_gid),
                    tiling: rd.tex_tiling,
                });
            }
        }
    }
    plan
}

/// Composite `plan` on the CPU. An overlay whose alpha map is missing is skipped, and a missing
/// texture blends the debug colour.
#[must_use]
pub fn execute_merge_plan(plan: &MergePlan, src: &dyn TerrainTextureSource) -> Bgra8 {
    let base_img = plan.base.and_then(|id| src.image(id));
    let mut dst = copy_and_tile(plan.size, base_img.as_deref(), plan.base_tiling);
    for o in &plan.overlays {
        let Some(alpha) = src.image(o.alpha) else {
            continue;
        };
        let img = o.tex.and_then(|id| src.image(id));
        merge_overlay(&mut dst, &alpha, o.rotation, img.as_deref(), o.tiling);
    }
    dst
}

/// `LandSurf`'s surface cache, keyed by the 32-bit merge key exactly as the client does. The
/// rotation is encoded in the key set, not in the texture, so one texture serves four orientations.
#[derive(Debug, Default)]
pub struct TerrainMergeCache {
    // The client's own container is a hash table; the observable surface
    // index comes from `order` below, not from this map's iteration.
    // ORDER-OK: keyed by MergeKey and only ever looked up, never iterated for output.
    surfaces: HashMap<MergeKey, (u16, TextureHandle)>,
    /// Surface-array slots allocated by this cache. `None` is
    /// retail's `NULL` slot: the surface removal nulls the surface-array slot when the last cell
    /// lets go, and the free-slot search returns **the first NULL index it finds** rather than the
    /// end of the array. So a freed index
    /// is reused, and this vector is bounded by the peak number of live surfaces rather than by
    /// the number ever created.
    order: Vec<Option<MergeKey>>,
    /// The surface record's cell count, incremented once per surface-selection call —
    /// on the cache-hit arm (count `+= 1`) as well as by
    /// the surface add, which sets it to 1 -- and decremented once per
    /// surface removal. Removing a destroyed block's surfaces calls that once
    /// per **cell** of a destroyed block, which is exactly the granularity
    /// at which [`Self::get_or_build`] operates.
    // ORDER-OK: refcounts are only ever looked up by key; nothing iterates them.
    refcounts: HashMap<MergeKey, u32>,
    /// The shift derived from the landscape texture-detail render preference.
    pub shift: u32,
    /// The extent of the surface [`Self::get_or_build`] last composited and uploaded, and the
    /// shift it was composited at. It is not a client field: it exists because
    /// `Render.LandscapeTextureDetail`'s whole observable effect is the **size** of the merged
    /// terrain texture, and a test that could only read [`Self::shift`] would be measuring the
    /// variable rather than the picture. `None` until the first surface is built.
    pub last_built: Option<(u32, u32, u32)>,
    /// [`Self::remove_surface`] naming a key this cache does not hold: a double release, or a
    /// block built against a different cache. Tolerated, counted, and asserted at zero — the same
    /// discipline `TextureTableStats::unknown_releases` gets, and the reason a green release path
    /// and a release path that never ran cannot print alike.
    pub unowned_removes: u32,
    /// Surfaces whose last cell let go, cumulative. The positive control for
    /// [`Self::unowned_removes`]: a walk that returned every link and a walk on which the edge was
    /// never called both leave `unowned_removes` at zero, and only this one separates them.
    pub surfaces_freed: u32,
    /// Cumulative build calls: surfaces actually composited and
    /// uploaded.
    ///
    /// **This is the counter "is the cache being hit" needs, and [`Self::len`] is not.** A cache
    /// that never released would have cumulative residency, and a residency that grew
    /// sub-linearly in the blocks walked would say the cache was hitting. With a release edge
    /// residency is a function of *where the window is* and says nothing about hit rate at all, so
    /// an assertion about hits has to be written against this counter.
    pub surfaces_built: u32,
    /// [`Self::get_or_build`] calls, cumulative — one per cell of every block ever meshed. The
    /// denominator for [`Self::surfaces_built`]: `requests - built` is the number of composites
    /// the cache saved, and a cache that missed every time would have the two equal.
    pub surface_requests: u64,
}

impl TerrainMergeCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cached surface index for a key, if it has one.
    #[must_use]
    pub fn index_of(&self, k: MergeKey) -> Option<u16> {
        self.surfaces.get(&k).map(|&(i, _)| i)
    }

    /// How many distinct surfaces are live.
    #[must_use]
    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    /// Drop one reference; the surface stays until the last
    /// goes, and then it **leaves the cache**.
    ///
    /// With the decrement alone, a landblock that left the streaming window would return nothing:
    /// the merged surfaces of every block a walk had ever touched would stay resident, and their
    /// descriptor pairs with them. That growth is invisible in `BakeCache`'s own numbers — over a 24-station out-and-back walk `BakeCache`'s residency is flat at 345 pairs
    /// at every home station while the **device's** live count went 560 at the load to 689 on the
    /// way home. The 129 in between are these.
    ///
    /// The removal's zero arm is four statements and this is all four: the surface-array slot is
    /// nulled (the index goes back to `Self::order` for `next_free` to hand out again),
    /// the hash-table entry is deleted,
    /// Surface cleanup destroys the texture; here
    /// the handle is **returned** so the caller, which is the side that owns the device, can
    /// release it), and the unique-surface count drops by one.
    ///
    /// Returns the handle whose last reference this was, and `None` while any cell still holds it.
    pub fn remove_surface(&mut self, k: MergeKey) -> Option<TextureHandle> {
        let Some(rc) = self.refcounts.get_mut(&k) else {
            self.unowned_removes += 1;
            return None;
        };
        *rc = rc.saturating_sub(1);
        if *rc > 0 {
            return None;
        }
        self.refcounts.remove(&k);
        let (index, handle) = self.surfaces.remove(&k)?;
        if let Some(slot) = self.order.get_mut(index as usize) {
            *slot = None;
        }
        self.surfaces_freed += 1;
        Some(handle)
    }

    /// Empty every live surface during scene teardown, for the
    /// caller that owns the device.
    ///
    /// The counters are deliberately **not** reset: they are cumulative census numbers over the
    /// session, and a teardown that zeroed them would make a test that tears down between two
    /// passes unable to see anything at all.
    /// Every live surface's key and handle, in key order.
    #[must_use]
    pub fn surfaces(&self) -> Vec<(MergeKey, TextureHandle)> {
        let mut v: Vec<(MergeKey, TextureHandle)> =
            self.surfaces.iter().map(|(&k, &(_, h))| (k, h)).collect();
        v.sort_by_key(|&(k, _)| k);
        v
    }

    pub fn drain(&mut self) -> Vec<TextureHandle> {
        self.order.clear();
        self.refcounts.clear();
        let handles: Vec<TextureHandle> = self.surfaces.drain().map(|(_, (_, h))| h).collect();
        self.surfaces_freed += u32_of(handles.len());
        handles
    }

    /// Allocate the first `NULL` surface slot, or a fresh one past the
    /// end. Without the release edge there would be no `NULL` slots and this would always be
    /// `order.len()`.
    fn next_free(&mut self) -> u16 {
        match self.order.iter().position(Option::is_none) {
            Some(i) => u16_of(i),
            None => {
                self.order.push(None);
                u16_of(self.order.len() - 1)
            }
        }
    }

    /// Composite the terrain key if it is not cached, upload it, and return the handle. Repeated calls
    /// with the same key return the same handle and upload nothing — which is the whole point of
    /// keying on the merge key.
    pub fn get_or_build(
        &mut self,
        tm: &TexMerge,
        k: MergeKey,
        r: &mut dyn RenderBackend,
        src: &dyn TerrainTextureSource,
    ) -> TextureHandle {
        self.get_or_build_with(tm, k, &mut |plan| {
            let img = execute_merge_plan(plan, src);
            // The merge path uses full texture resolution, so the
            // merge result is not down-scaled a second time, and creates a single-level system
            // image. The runtime image-texture copy may autogenerate GPU sublevels.
            r.upload_texture(&TextureData {
                width: img.width,
                height: img.height,
                format: TextureFormat::Bgra8,
                levels: vec![img.into_bytes()],
            })
        })
    }

    /// [`Self::get_or_build`] with the composite and its upload left to `build`, which is handed
    /// the key's [`MergePlan`] on a miss and returns the texture it made from it. The cache's
    /// bookkeeping -- references, slots, counters -- is the same either way.
    pub fn get_or_build_with(
        &mut self,
        tm: &TexMerge,
        k: MergeKey,
        build: &mut dyn FnMut(&MergePlan) -> TextureHandle,
    ) -> TextureHandle {
        let shift = self.shift;
        self.get_or_build_keyed(k, &mut || {
            let plan = merge_plan(tm, k, shift);
            let handle = build(&plan);
            (handle, plan.size)
        })
    }

    /// The cache's bookkeeping around any surface builder: one reference per call, and on a miss
    /// `build` makes the texture and answers it with its extent. The palette-shift land surface
    /// builds through this with its own compositor; texture merging through
    /// [`Self::get_or_build_with`].
    pub fn get_or_build_keyed(
        &mut self,
        k: MergeKey,
        build: &mut dyn FnMut() -> (TextureHandle, u32),
    ) -> TextureHandle {
        self.surface_requests += 1;
        *self.refcounts.entry(k).or_insert(0) += 1;
        if let Some(&(_, h)) = self.surfaces.get(&k) {
            return h;
        }
        self.surfaces_built += 1;
        let (handle, size) = build();
        self.last_built = Some((size, size, self.shift));
        // LINT-OK: index arithmetic; the surface array is bounded by the window's cell count.
        let index = self.next_free();
        self.order[index as usize] = Some(k);
        self.surfaces.insert(k, (index, handle));
        handle
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use dereth_assets::region::CodeTexture;

    /// A one-colour tile.
    fn solid(w: u32, c: [u8; 4]) -> Bgra8 {
        Bgra8 {
            width: w,
            height: w,
            pixels: vec![c; (w * w) as usize],
        }
    }

    /// An alpha map that is 0 (overlay wins) on the SE half and 255 (keep the base) on the NW half,
    /// with `tcode` 8 so it matches the retail corner maps.
    ///
    /// **Alpha-only, like the real thing.** A retail map decodes to `B 0..0 G 0..0 R 0..0 A 0..255`;
    /// an earlier version of this fixture put the mask in B, G and R as well, which is exactly the
    /// misreading `merge_overlay` had, so the two agreed and the bug rendered a flat landscape.
    fn alpha_only(w: u32, f: impl Fn(u32, u32) -> u8) -> Bgra8 {
        let mut img = Bgra8::new(w, w);
        for y in 0..w {
            for x in 0..w {
                img.pixels[(y * w + x) as usize] = [0, 0, 0, f(x, y)];
            }
        }
        img
    }

    fn half_alpha(w: u32) -> Bgra8 {
        alpha_only(w, |x, y| if x + y >= w { 0 } else { 255 })
    }

    fn tex_merge(size: u32) -> TexMerge {
        let desc = |t: u32, gid: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: DataId(gid),
            tex_tiling: 1,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: DataId(0),
        };
        TexMerge {
            base_tex_size: size,
            // Four corner maps, all tcode 8, exactly as the retail region ships them.
            corner_terrain_maps: (0..4)
                .map(|i| CodeTexture {
                    code: 8,
                    tex_gid: DataId(0x0500_0100 + i),
                })
                .collect(),
            side_terrain_maps: vec![CodeTexture {
                code: 9,
                tex_gid: DataId(0x0500_0200),
            }],
            road_maps: vec![
                CodeTexture {
                    code: 9,
                    tex_gid: DataId(0x0500_0300),
                },
                CodeTexture {
                    code: 10,
                    tex_gid: DataId(0x0500_0301),
                },
                CodeTexture {
                    code: 8,
                    tex_gid: DataId(0x0500_0302),
                },
            ],
            terrain_desc: vec![
                desc(1, 0x0600_0001),
                desc(2, 0x0600_0002),
                desc(0x20, 0x0600_0020),
            ],
        }
    }

    /// The compositor lays the base down and blends the overlay through its alpha map.
    #[test]
    fn the_compositor_lays_the_base_down_and_blends_the_overlay_through_its_alpha_map() {
        let tm = tex_merge(16);
        let base = solid(16, [10, 20, 30, 255]);
        let overlay = solid(16, [200, 210, 220, 255]);
        let alpha = half_alpha(16);
        let src = |id: DataId| -> Option<Bgra8> {
            match id.0 {
                0x0600_0001 => Some(base.clone()),
                0x0600_0002 => Some(overlay.clone()),
                0x0500_0100..=0x0500_0103 => Some(alpha.clone()),
                _ => None,
            }
        };
        // Corner types SW=1, SE=1, NE=1, NW=2: base is type 1, one corner overlay of type 2 at
        // bit 3 (NW), mask 8 -- which is exactly the tcode the retail corner maps carry, so
        // find_terrain_alpha resolves at ROT_0.
        let key = MergeKey::new([1, 1, 1, 2], [0; 4], 1);
        let g = get_terrain(key);
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.code, [8, 0, 0]);
        let map = find_terrain_alpha(&tm, 8, key).expect("an alpha map for mask 8");
        assert_eq!(
            map.rotation,
            Rotation::Rot0,
            "tcode 8 == the wanted mask, so no rotation"
        );

        let out = fill_temp_tex_buffer(&tm, key, 0, &src);
        assert_eq!(out.width, 16, "base_tex_size 16 >> 0, / palLod 1");
        // Where the alpha map is 255 the base survives untouched.
        assert_eq!(out.get(0, 0), [10, 20, 30, 255], "alpha 255 keeps the base");
        // Where it is 0 the overlay wins outright, and the base's alpha channel is untouched.
        assert_eq!(
            out.get(15, 15),
            [200, 210, 220, 255],
            "alpha 0 means the overlay wins"
        );
    }

    /// Oracle: the blend arithmetic evaluated by hand at a mid-mask value, the case between the two
    /// extremes the test above pins, and the one that makes a landscape look blended rather than
    /// patched.
    ///
    /// With `a = 64`: `a' = 64` (not `> 128`), `ia = 192`, and each channel is
    /// `(src * 192 + dst * 64) >> 8`. Base `[10, 20, 30]`, overlay `[200, 210, 220]`:
    ///
    /// * B `(200*192 + 10*64) >> 8 = 39040 >> 8 = 152`
    /// * G `(210*192 + 20*64) >> 8 = 41600 >> 8 = 162`
    /// * R `(220*192 + 30*64) >> 8 = 44160 >> 8 = 172`
    #[test]
    fn a_mid_alpha_map_produces_a_true_blend_of_base_and_overlay() {
        let tm = tex_merge(16);
        let base = solid(16, [10, 20, 30, 255]);
        let overlay = solid(16, [200, 210, 220, 255]);
        let alpha = alpha_only(16, |_, _| 64);
        let src = |id: DataId| -> Option<Bgra8> {
            match id.0 {
                0x0600_0001 => Some(base.clone()),
                0x0600_0002 => Some(overlay.clone()),
                0x0500_0100..=0x0500_0103 => Some(alpha.clone()),
                _ => None,
            }
        };
        let out = fill_temp_tex_buffer(&tm, MergeKey::new([1, 1, 1, 2], [0; 4], 1), 0, &src);
        assert!(
            out.pixels.iter().all(|&p| p == [152, 162, 172, 255]),
            "every texel is the documented mix, not the base and not the overlay"
        );
    }

    /// Oracle: `fill_temp_tex_buffer`'s `if all_road and road_tex: tex[0] = road_tex` — a
    /// fully-roaded cell
    /// **replaces** the base texture rather than overlaying anything, and skips the whole overlay
    /// and road-mask loop.
    #[test]
    fn a_fully_roaded_cell_replaces_the_base_texture() {
        let tm = tex_merge(8);
        let src = |id: DataId| -> Option<Bgra8> {
            match id.0 {
                0x0600_0001 => Some(solid(8, [1, 1, 1, 255])),
                0x0600_0020 => Some(solid(8, [99, 98, 97, 255])),
                _ => Some(solid(8, [128, 128, 128, 255])),
            }
        };
        // Every corner carries a road value, so get_road_code reports all_road.
        let key = MergeKey::new([1, 1, 1, 1], [1, 1, 1, 1], 1);
        assert!(get_road_code(key).all_road);
        let out = fill_temp_tex_buffer(&tm, key, 0, &src);
        assert!(
            out.pixels.iter().all(|&p| p == [99, 98, 97, 255]),
            "the road texture alone"
        );
    }

    /// Oracle: with a null texture the compositor fills every pixel with the
    /// bytes `00 FF 00 00`. A missing terrain texture is visible in the original, so reproducing
    /// the debug colour is reproducing the bug report rather than hiding it.
    #[test]
    fn a_missing_texture_shows_the_debug_green_rather_than_being_skipped() {
        let out = copy_and_tile(4, None, 1);
        assert!(out.pixels.iter().all(|&p| p == [0x00, 0xFF, 0x00, 0x00]));
        let tm = tex_merge(8);
        let out = fill_temp_tex_buffer(&tm, MergeKey::new([1; 4], [0; 4], 1), 0, &|_| None);
        assert!(out.pixels.iter().all(|&p| p == [0x00, 0xFF, 0x00, 0x00]));
    }

    /// Oracle: the merge cache path and surface-slot allocation — a repeated key returns the same
    /// handle and uploads nothing, and the
    /// surface index is the insertion order.
    #[test]
    fn the_merge_cache_uploads_one_texture_per_distinct_key() {
        let tm = tex_merge(8);
        let src = |_: DataId| -> Option<Bgra8> { Some(solid(8, [1, 2, 3, 255])) };
        let mut cache = TerrainMergeCache::new();
        let mut r = crate::testing::Recorder::new();
        let a = MergeKey::new([1, 1, 1, 2], [0; 4], 1);
        let b = MergeKey::new([2, 2, 2, 1], [0; 4], 1);
        let ha = cache.get_or_build(&tm, a, &mut r, &src);
        let hb = cache.get_or_build(&tm, b, &mut r, &src);
        assert_ne!(ha, hb);
        assert_eq!(r.textures.len(), 2);
        assert_eq!(
            cache.get_or_build(&tm, a, &mut r, &src),
            ha,
            "the same key, the same handle"
        );
        assert_eq!(r.textures.len(), 2, "and no second upload");
        assert_eq!(
            cache.index_of(a),
            Some(0),
            "insertion order is the surface index"
        );
        assert_eq!(cache.index_of(b), Some(1));
        assert_eq!(cache.len(), 2);
        assert_eq!(
            (cache.surfaces_built, cache.surface_requests),
            (2, 3),
            "two builds, three asks"
        );
        // The uploaded texture is the composited size, single-level BGRA8.
        assert_eq!(r.textures[0].1, 8);
        assert_eq!(
            r.textures[0].3, 1,
            "the landscape texture has one level, not a mip chain"
        );
    }

    /// A surface leaves the cache when its last cell lets go and not before.
    #[test]
    fn a_surface_leaves_the_cache_when_its_last_cell_lets_go_and_not_before() {
        let tm = tex_merge(8);
        let src = |_: DataId| -> Option<Bgra8> { Some(solid(8, [1, 2, 3, 255])) };
        let mut cache = TerrainMergeCache::new();
        let mut r = crate::testing::Recorder::new();
        let a = MergeKey::new([1, 1, 1, 2], [0; 4], 1);
        let b = MergeKey::new([2, 2, 2, 1], [0; 4], 1);
        // Block 1: three cells on `a`. Block 2: one cell on `a`, two on `b`.
        let ha = cache.get_or_build(&tm, a, &mut r, &src);
        for _ in 0..2 {
            cache.get_or_build(&tm, a, &mut r, &src);
        }
        cache.get_or_build(&tm, a, &mut r, &src);
        let hb = cache.get_or_build(&tm, b, &mut r, &src);
        cache.get_or_build(&tm, b, &mut r, &src);
        assert_eq!(r.textures.len(), 2, "one upload per distinct key");
        assert_eq!(cache.len(), 2);

        // Block 2 leaves: one `a` and two `b`. `b`'s last cell goes with it, `a`'s does not.
        assert_eq!(
            cache.remove_surface(a),
            None,
            "block 1 still holds three cells on `a`"
        );
        assert_eq!(cache.remove_surface(b), None);
        assert_eq!(
            cache.remove_surface(b),
            Some(hb),
            "`b`'s last cell freed it"
        );
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.index_of(b), None, "and it left the surface array");
        assert_eq!(cache.surfaces_freed, 1);

        // Block 1 leaves.
        assert_eq!(cache.remove_surface(a), None);
        assert_eq!(cache.remove_surface(a), None);
        assert_eq!(
            cache.remove_surface(a),
            Some(ha),
            "`a`'s last cell freed it"
        );
        assert!(cache.is_empty(), "an empty window holds no surfaces");
        assert_eq!(cache.surfaces_freed, 2);
        assert_eq!(cache.unowned_removes, 0);
    }

    /// A freed surface index is handed out again.
    #[test]
    fn a_freed_surface_index_is_handed_out_again() {
        let tm = tex_merge(8);
        let src = |_: DataId| -> Option<Bgra8> { Some(solid(8, [1, 2, 3, 255])) };
        let mut cache = TerrainMergeCache::new();
        let mut r = crate::testing::Recorder::new();
        let a = MergeKey::new([1, 1, 1, 2], [0; 4], 1);
        let b = MergeKey::new([2, 2, 2, 1], [0; 4], 1);
        let c = MergeKey::new([3, 3, 3, 1], [0; 4], 1);
        cache.get_or_build(&tm, a, &mut r, &src);
        cache.get_or_build(&tm, b, &mut r, &src);
        assert_eq!((cache.index_of(a), cache.index_of(b)), (Some(0), Some(1)));
        assert!(cache.remove_surface(a).is_some());
        cache.get_or_build(&tm, c, &mut r, &src);
        assert_eq!(
            cache.index_of(c),
            Some(0),
            "index 0 went back into circulation"
        );
        assert_eq!(
            cache.order.len(),
            2,
            "and the array did not grow to hold a third"
        );
    }

    /// A removal of a key the cache never held is counted rather than silent.
    #[test]
    fn a_removal_of_a_key_the_cache_never_held_is_counted_rather_than_silent() {
        let mut cache = TerrainMergeCache::new();
        assert_eq!(cache.remove_surface(MergeKey::new([1; 4], [0; 4], 1)), None);
        assert_eq!(cache.unowned_removes, 1);
        assert_eq!(cache.surfaces_freed, 0, "and nothing was freed by it");
    }

    /// Oracle: the channel blend. Every assertion here is
    /// the formula evaluated by hand, not by this function.
    #[test]
    fn integer_blend_matches_the_transcribed_formula() {
        // a == 255: destination untouched.
        let mut d = [10, 20, 30, 40];
        integer_blend(&mut d, [200, 200, 200, 200], 255);
        assert_eq!(d, [10, 20, 30, 40]);

        // a == 0: the overlay wins outright. a' = 0, ia = 256, (src*256 + dst*0) >> 8 == src.
        let mut d = [10, 20, 30, 40];
        integer_blend(&mut d, [200, 201, 202, 203], 0);
        assert_eq!(
            d,
            [200, 201, 202, 40],
            "alpha 0 means the overlay wins; alpha channel untouched"
        );

        // a == 128: not > 128, so a' = 128, ia = 128. (200*128 + 0*128) >> 8 = 100.
        let mut d = [0, 0, 0, 99];
        integer_blend(&mut d, [200, 200, 200, 0], 128);
        assert_eq!(d, [100, 100, 100, 99]);

        // a == 129: > 128, so a' = 130, ia = 126. (200*126 + 100*130) >> 8 = (25200+13000)>>8 = 149.
        let mut d = [100, 100, 100, 1];
        integer_blend(&mut d, [200, 200, 200, 0], 129);
        assert_eq!(d, [149, 149, 149, 1]);
    }

    /// Oracle: "it differs from `lerp(src, dst, a/255)` by up to one unit per channel
    /// across the entire landscape". If a float lerp ever agreed everywhere, the integer rule would
    /// not matter; this test proves it does, and fails loudly if someone swaps the implementation.
    #[test]
    fn a_float_lerp_disagrees_with_the_integer_blend() {
        let mut disagreements = 0;
        let mut max_delta = 0i32;
        for a in 0..=255u8 {
            for &(s, d) in &[(0u8, 255u8), (255, 0), (200, 100), (17, 240), (128, 129)] {
                let mut dst = [d, d, d, 0];
                integer_blend(&mut dst, [s, s, s, 0], a);
                let lerp = f32::from(s) * (1.0 - f32::from(a) / 255.0)
                    + f32::from(d) * (f32::from(a) / 255.0);
                let lerp = dereth_primitives::num::to_i32(lerp + 0.5);
                let delta = i32::from(dst[0]) - lerp;
                if delta != 0 {
                    disagreements += 1;
                    max_delta = max_delta.max(delta.abs());
                }
            }
        }
        assert!(
            disagreements > 0,
            "the integer blend must not be a rounded float lerp"
        );
        assert!(
            max_delta <= 2,
            "the two forms differ by at most a unit or two, not wildly"
        );
    }

    /// Oracle: the merge-key bit layout, cross-read against `get_terrain`'s re-extraction
    /// (`t[0] = (key>>15)&0x1F` …) and `get_road_code`'s masks (SW is bits 26-27). If the two
    /// halves of the implementation disagreed, this is where it would show.
    #[test]
    fn key_layout_round_trips_through_both_readers() {
        let k = MergeKey::new([5, 9, 17, 30], [3, 0, 2, 1], 4);
        assert_eq!(k.corner_types(), [5, 9, 17, 30]);
        assert_eq!(k.pal_lod(), 4);
        // roads = 3*64 + 0*16 + 2*4 + 1 = 201 at bits 20-27.
        assert_eq!((k.0 >> 20) & 0xFF, 201);
        // get_road_code's masks must see SW and NE set, SE clear, NW set: rc = 1|4|8 = 13, which is
        // one of the three-corner cases and is drawn as the two overlapping side masks {9, 12}.
        let rc = get_road_code(k);
        assert_eq!(rc.codes, [9, 12]);
    }

    /// Oracle: [`get_terrain`], walked case by case. The
    /// asymmetry — mask 9 (NW, SW) is never produced — is the part a "tidier" implementation
    /// silently fixes.
    #[test]
    fn get_terrain_reproduces_every_documented_branch() {
        let k = |t: [u16; 4]| MergeKey::new(t, [0; 4], 1);

        // All four corners the same: base only, no overlays.
        let g = get_terrain(k([7, 7, 7, 7]));
        assert_eq!(g.tex[0], Some(7));
        assert_eq!(g.code, [0, 0, 0]);

        // All four distinct: SW is the base, the other three become corner overlays 2, 4, 8.
        let g = get_terrain(k([1, 2, 3, 4]));
        assert_eq!(g.tex, [Some(1), Some(2), Some(3), Some(4)]);
        assert_eq!(g.code, [2, 4, 8]);

        // SW and SE share; the NE corner is the first non-base, and NW matches it and is adjacent,
        // so the two merge into the side mask 4 + 8 = 12.
        let g = get_terrain(k([1, 1, 2, 2]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.tex[1], Some(2));
        assert_eq!(g.code, [12, 0, 0]);

        // The forbidden pair: NW and SW would be mask 9. SW is the base here (it repeats at NW),
        // so the non-base corners are SE and NE -- a different configuration entirely. Build the
        // real mask-9 candidate instead: base at SE/NE, overlay at SW and NW.
        let g = get_terrain(k([2, 1, 1, 2]));
        assert_eq!(g.tex[0], Some(2), "SW repeats at NW, so SW is the base");
        assert_eq!(
            g.code,
            [2 + 4, 0, 0],
            "SE and NE are adjacent and merge to side mask 6"
        );

        // Two opposite corners: masks 2 and 8 never merge, so they stay two overlays.
        let g = get_terrain(k([1, 2, 1, 2]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.code, [2, 8, 0], "opposite corners stay separate");
        assert!(g.code.iter().all(|&c| c != 9), "mask 9 is never produced");

        // A third distinct type stops the merge and takes the second overlay slot.
        let g = get_terrain(k([1, 2, 3, 1]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.tex[1], Some(2));
        assert_eq!(g.tex[2], Some(3));
        assert_eq!(g.code, [2, 4, 0]);
    }

    /// Oracle: the road-code switch over all sixteen inputs.
    #[test]
    fn road_codes_cover_all_sixteen_corner_combinations() {
        let key_for = |rc: u32| {
            let roads = [
                u16::from(rc & 1 != 0),
                u16::from(rc & 2 != 0),
                u16::from(rc & 4 != 0),
                u16::from(rc & 8 != 0),
            ];
            MergeKey::new([0; 4], roads, 1)
        };
        for rc in 0..16u32 {
            let g = get_road_code(key_for(rc));
            let expect = match rc {
                0 => (false, [0, 0]),
                15 => (true, [0, 0]),
                7 => (false, [3, 6]),
                11 => (false, [9, 3]),
                13 => (false, [9, 12]),
                14 => (false, [6, 12]),
                _ => (false, [rc, 0]),
            };
            assert_eq!((g.all_road, g.codes), expect, "rc={rc}");
        }
    }

    /// Oracle: `c*2`, with `-15` on overflow, is a 4-bit cyclic rotate of
    /// the corner mask.
    #[test]
    fn rotate_code_is_a_four_bit_cyclic_rotate() {
        assert_eq!([1, 2, 4, 8].map(rotate_code), [2, 4, 8, 1]);
        assert_eq!([3, 6, 12, 9].map(rotate_code), [6, 12, 9, 3]);
        // Four rotations return every mask to itself, which is what makes the search terminate.
        for c in 1..16u32 {
            assert_eq!(
                rotate_code(rotate_code(rotate_code(rotate_code(c)))),
                c,
                "c={c}"
            );
        }
    }

    /// Oracle: the blend rotation table, read as data. Each rotation must be a bijection of
    /// the map, and ROT_0 must be the identity.
    #[test]
    fn alpha_rotation_walks_are_bijections() {
        let (w, h) = (4u32, 4u32);
        for rot in Rotation::ALL {
            let mut seen = vec![false; (w * h) as usize];
            for row in 0..h {
                for col in 0..w {
                    let o = rotated_offset(rot, w, h, col, row);
                    assert!(!seen[o], "{rot:?} visits {o} twice");
                    seen[o] = true;
                }
            }
            assert!(seen.iter().all(|&b| b), "{rot:?} misses a texel");
        }
        for row in 0..h {
            for col in 0..w {
                assert_eq!(
                    rotated_offset(Rotation::Rot0, w, h, col, row),
                    (row * w + col) as usize
                );
            }
        }
    }

    /// A splat source at each texture detail level carries the texels the composite at that
    /// level carries for one repeat of the same texture: the highest detail leaves it whole, and
    /// each lower setting picks the same texels the composite's stride picks.
    #[test]
    fn a_splat_source_carries_the_texels_the_composite_carries_at_each_detail_level() {
        let mut tex = Bgra8::new(256, 256);
        for (i, p) in tex.pixels.iter_mut().enumerate() {
            *p = [(i % 251) as u8, (i / 256) as u8, (i % 256) as u8, 0xFF];
        }
        let tiling = 4;
        for pref in 0..=4 {
            let size = merged_texture_size(1024, land_texture_scale_shift(pref), 1);
            let composite = copy_and_tile(size, Some(&tex), tiling);
            let splat = source_at_scale(&tex, size, tiling);
            let per_repeat = size / tiling;
            match &splat {
                None => assert!(per_repeat >= tex.width, "detail {pref} keeps the source"),
                Some(s) => {
                    assert_eq!(
                        (s.width, s.height),
                        (per_repeat, per_repeat),
                        "detail {pref}"
                    );
                    for y in 0..per_repeat {
                        for x in 0..per_repeat {
                            assert_eq!(s.get(x, y), composite.get(x, y), "detail {pref} ({x},{y})");
                        }
                    }
                }
            }
        }
        assert!(
            source_at_scale(&tex, 1024, 4).is_none(),
            "Very High: the whole source"
        );
        let very_low = source_at_scale(&tex, merged_texture_size(1024, 4, 1), 4).expect("shrunk");
        assert_eq!(
            very_low.width, 16,
            "Very Low: 64 texels a cell, 16 a repeat"
        );
    }

    /// Oracle: the merged-pixel size rule and land-texture-scale derivation,
    /// including the trap that the image-shift table's entry 4 is 8 rather than 4.
    #[test]
    fn merged_size_uses_the_shift_table_not_the_preference() {
        assert_eq!(land_texture_scale_shift(0), 0);
        assert_eq!(land_texture_scale_shift(1), 0);
        assert_eq!(land_texture_scale_shift(2), 1);
        assert_eq!(land_texture_scale_shift(3), 2);
        assert_eq!(land_texture_scale_shift(4), 4, "the table is {{0,1,2,4,8}}");
        // base_tex_size 1024 (open question #79, resolved).
        assert_eq!(merged_texture_size(1024, 0, 1), 1024);
        assert_eq!(merged_texture_size(1024, 0, 4), 256);
        assert_eq!(merged_texture_size(1024, 4, 4), 16);
        assert_eq!(
            merged_texture_size(1024, 8, 1),
            MIN_TEX_SIZE,
            "the 8 floor bites"
        );
    }
}
