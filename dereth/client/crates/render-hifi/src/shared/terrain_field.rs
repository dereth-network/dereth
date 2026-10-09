//! The landscape field: heights, smooth normals and surface classes over the resident window,
//! one texel per landscape vertex and seamless across block edges.
//!
//! It is built from the snapshot's landscape blocks -- their drawn vertex heights, cuts and
//! terrain words -- and rebuilt only when a block's contents or the window change. Its heights
//! are the drawn landscape's: at a full-detail block every texel is a drawn vertex, and on a
//! coarser block each texel is the height of the coarse triangle drawn over it. So
//! [`TerrainField::plane_height`] and its shader twin `plane_height` give the height of the drawn
//! ground at any point, and the field's normals are a smooth surface through those heights with
//! no seam at block edges.
//!
//! The landscape cell's diagonal is a hash of its global cell coordinates. The shader's
//! `split_diagonal` is the same arithmetic, in 32-bit wrapping unsigned integers.

use glam::{Vec2, Vec3};

use crate::snapshot::HifiTerrainBlock;

/// Landscape vertices along one side of a block.
pub const BLOCK_VERTICES: usize = 9;
/// Landscape cells along one side of a block.
pub const BLOCK_CELLS: usize = BLOCK_VERTICES - 1;
/// The side of one landscape cell, metres.
pub const CELL_METRES: f32 = 24.0;
/// The side of one block, metres.
pub const BLOCK_METRES: f32 = CELL_METRES * BLOCK_CELLS as f32;

/// The diagonal hash's constants, in the order the expression uses them:
/// `((x * A + B) * y + x * C + D)`, wrapping, whose top bit says the cut runs south-west to
/// north-east.
pub const SPLIT_HASH: [u32; 4] = [0x0CCA_C033, 0x6C1A_C587, 0xBDE4_1C43, 0xAE64_70DB];

/// What kind of ground a landscape vertex is, as the class texture stores it.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroundClass {
    /// No block covers it.
    None = 0,
    /// Grass and lush ground.
    Grass = 1,
    /// Dirt, mud and marsh.
    Dirt = 2,
    /// Sand.
    Sand = 3,
    /// Bare rock and stone.
    Rock = 4,
    /// Snow and ice.
    Snow = 5,
    /// Water.
    Water = 6,
    /// A road.
    Road = 7,
}

impl GroundClass {
    /// The class of a vertex with terrain word `word`: its road bits first, then its terrain type.
    #[must_use]
    pub const fn of_word(word: u16) -> Self {
        if word & 0x3 != 0 {
            return Self::Road;
        }
        Self::of_type(((word >> 2) & 0x1F) as u8)
    }

    /// The class of terrain type `t`.
    #[must_use]
    pub const fn of_type(t: u8) -> Self {
        match t {
            // Barren rock, obsidian plain, sedimentary rock, semi-barren rock.
            0 | 6 | 13 | 14 => Self::Rock,
            // Grassland, lush grass, patchy grassland.
            1 | 3 | 9 => Self::Grass,
            // Ice, snow.
            2 | 15 => Self::Snow,
            // Marsh, rich dirt, packed dirt, patchy dirt.
            4 | 5 | 7 | 8 => Self::Dirt,
            // Yellow, grey and rock-strewn sand.
            10..=12 => Self::Sand,
            // Running, fresh, shallow sea, still sea and deep sea water.
            16..=20 => Self::Water,
            // The road type.
            31 => Self::Road,
            _ => Self::Rock,
        }
    }
}

/// Whether landscape cell `(x, y)`, in global cell coordinates, is cut from its south-west corner
/// to its north-east one.
#[must_use]
pub const fn split_diagonal(x: i32, y: i32) -> bool {
    let [a, b, c, d] = SPLIT_HASH;
    let (x, y) = (x as u32, y as u32);
    let v = x
        .wrapping_mul(a)
        .wrapping_add(b)
        .wrapping_mul(y)
        .wrapping_add(x.wrapping_mul(c))
        .wrapping_add(d);
    v >> 31 != 0
}

/// The height of a cell's drawn triangles at fraction `f` across it (east, north, each 0..1),
/// from its corner heights `h` (south-west, south-east, north-east, north-west), cut south-west
/// to north-east when `cut`.
#[must_use]
pub fn cell_plane_height(h: [f32; 4], f: Vec2, cut: bool) -> f32 {
    let [sw, se, ne, nw] = h;
    if cut {
        if f.x >= f.y {
            sw + f.x * (se - sw) + f.y * (ne - se)
        } else {
            sw + f.x * (ne - nw) + f.y * (nw - sw)
        }
    } else if f.x + f.y <= 1.0 {
        sw + f.x * (se - sw) + f.y * (nw - sw)
    } else {
        ne + (1.0 - f.x) * (nw - ne) + (1.0 - f.y) * (se - ne)
    }
}

/// The shader half: `split_diagonal`, `cell_plane_height`, and the field's own lookups over its
/// textures, which a pass binds as `field_height` (`r32float`), `field_normal` (`rg16float`) and
/// `field_class` (`r8uint`) with a `field: TerrainFieldParams` uniform.
#[must_use]
pub fn wgsl() -> String {
    let [a, b, c, d] = SPLIT_HASH;
    format!(
        "
// ---- the landscape field ----------------------------------------------------------------

struct TerrainFieldParams {{
    // xy: the render-space position of texel (0, 0); zw: texels across, texels up.
    origin_size: vec4<f32>,
    // xy: the global cell coordinates of texel (0, 0)'s cell.
    first_cell: vec4<i32>,
}};

// Whether global cell `cell` is cut from its south-west corner to its north-east one.
fn split_diagonal(cell: vec2<i32>) -> bool {{
    let x = bitcast<u32>(cell.x);
    let y = bitcast<u32>(cell.y);
    let v = (x * {a:#010x}u + {b:#010x}u) * y + x * {c:#010x}u + {d:#010x}u;
    return (v >> 31u) != 0u;
}}

// The height of a cell's drawn triangles at fraction `f` across it, from its corner heights
// h = (south-west, south-east, north-east, north-west).
fn cell_plane_height(h: vec4<f32>, f: vec2<f32>, cut: bool) -> f32 {{
    if (cut) {{
        if (f.x >= f.y) {{
            return h.x + f.x * (h.y - h.x) + f.y * (h.z - h.y);
        }}
        return h.x + f.x * (h.z - h.w) + f.y * (h.w - h.x);
    }}
    if (f.x + f.y <= 1.0) {{
        return h.x + f.x * (h.y - h.x) + f.y * (h.w - h.x);
    }}
    return h.z + (1.0 - f.x) * (h.w - h.z) + (1.0 - f.y) * (h.y - h.z);
}}

fn field_texel(t: vec2<i32>) -> f32 {{
    let size = vec2<i32>(field.origin_size.zw);
    return textureLoad(field_height, clamp(t, vec2<i32>(0), size - vec2<i32>(1)), 0).r;
}}

// The drawn landscape's height at render-space `p`.
fn plane_height(p: vec2<f32>) -> f32 {{
    let t = (p - field.origin_size.xy) / {CELL_METRES:.1};
    let size = field.origin_size.zw - vec2<f32>(1.0);
    let c = clamp(floor(t), vec2<f32>(0.0), max(size - vec2<f32>(1.0), vec2<f32>(0.0)));
    let f = clamp(t - c, vec2<f32>(0.0), vec2<f32>(1.0));
    let i = vec2<i32>(c);
    let h = vec4<f32>(
        field_texel(i),
        field_texel(i + vec2<i32>(1, 0)),
        field_texel(i + vec2<i32>(1, 1)),
        field_texel(i + vec2<i32>(0, 1)),
    );
    return cell_plane_height(h, f, split_diagonal(field.first_cell.xy + i));
}}

fn field_vertex_normal(t: vec2<i32>) -> vec3<f32> {{
    let size = vec2<i32>(field.origin_size.zw);
    let n = textureLoad(field_normal, clamp(t, vec2<i32>(0), size - vec2<i32>(1)), 0).rg;
    return vec3<f32>(n, sqrt(max(1.0 - dot(n, n), 0.0)));
}}

// The field's smooth normal at render-space `p`, z up: the normals of the four vertices around
// it, blended by distance, so it has no step at a cell's edge.
fn field_normal_at(p: vec2<f32>) -> vec3<f32> {{
    let t = (p - field.origin_size.xy) / {CELL_METRES:.1};
    let size = field.origin_size.zw - vec2<f32>(1.0);
    let c = clamp(floor(t), vec2<f32>(0.0), max(size - vec2<f32>(1.0), vec2<f32>(0.0)));
    let f = clamp(t - c, vec2<f32>(0.0), vec2<f32>(1.0));
    let i = vec2<i32>(c);
    let south = mix(field_vertex_normal(i), field_vertex_normal(i + vec2<i32>(1, 0)), f.x);
    let north = mix(
        field_vertex_normal(i + vec2<i32>(0, 1)),
        field_vertex_normal(i + vec2<i32>(1, 1)),
        f.x,
    );
    return normalize(mix(south, north, f.y));
}}

// The ground class of the vertex nearest render-space `p`.
fn field_class_at(p: vec2<f32>) -> u32 {{
    let size = vec2<i32>(field.origin_size.zw);
    let t = (p - field.origin_size.xy) / {CELL_METRES:.1};
    let i = clamp(vec2<i32>(round(t)), vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(field_class, i, 0).r;
}}
"
    )
}

/// What the field was built from: each block's id, generation, ring and render-space origin (as
/// bits), so a rebuilt block, a block changing detail and the window moving all change it.
pub type FieldSignature = Vec<(u16, u32, u8, [u32; 2])>;

/// The field on the CPU, ready to upload.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainField {
    /// The block coordinates of the field's south-west block.
    pub first_block: (u8, u8),
    /// The render-space position of texel (0, 0).
    pub origin: Vec2,
    /// Texels across (east) and up (north).
    pub size: (u32, u32),
    /// The height of every texel, metres, row by row from the south: `y * width + x`.
    pub heights: Vec<f32>,
    /// The smooth normal of every texel, its east and north parts (the up part is positive).
    pub normals: Vec<[f32; 2]>,
    /// The ground class of every texel.
    pub classes: Vec<GroundClass>,
    /// The terrain type of every texel, its road bits left out: what the ground beside a road
    /// is painted with.
    pub types: Vec<u8>,
}

impl TerrainField {
    /// The field over `blocks`, or `None` when there are none.
    #[must_use]
    pub fn build(blocks: &[HifiTerrainBlock]) -> Option<Self> {
        let usable: Vec<&HifiTerrainBlock> = blocks
            .iter()
            .filter(|b| b.side >= 2 && b.heights.len() == usize::from(b.side).pow(2))
            .collect();
        let coords = |b: &HifiTerrainBlock| ((b.block >> 8) as u8, (b.block & 0xFF) as u8);
        let x0 = usable.iter().map(|b| coords(b).0).min()?;
        let y0 = usable.iter().map(|b| coords(b).1).min()?;
        let x1 = usable.iter().map(|b| coords(b).0).max()?;
        let y1 = usable.iter().map(|b| coords(b).1).max()?;
        let width = (usize::from(x1 - x0) + 1) * BLOCK_CELLS + 1;
        let height = (usize::from(y1 - y0) + 1) * BLOCK_CELLS + 1;
        let first = usable
            .iter()
            .find(|b| coords(b) == (x0, y0))
            .map(|b| b.origin)
            .or_else(|| {
                // No block at the south-west corner: place it from any block's origin.
                usable.first().map(|b| {
                    let (bx, by) = coords(b);
                    b.origin
                        - Vec3::new(
                            f32::from(bx - x0) * BLOCK_METRES,
                            f32::from(by - y0) * BLOCK_METRES,
                            0.0,
                        )
                })
            })?;
        let mut heights = vec![0.0f32; width * height];
        let mut classes = vec![GroundClass::None; width * height];
        let mut types = vec![u8::MAX; width * height];
        let mut best = vec![u8::MAX; width * height];
        for b in &usable {
            let (bx, by) = coords(b);
            let (ox, oy) = (
                usize::from(bx - x0) * BLOCK_CELLS,
                usize::from(by - y0) * BLOCK_CELLS,
            );
            for e in 0..BLOCK_VERTICES {
                for n in 0..BLOCK_VERTICES {
                    let at = (oy + n) * width + ox + e;
                    // A finer block's vertex wins where two blocks share an edge.
                    if b.ring > best[at] {
                        continue;
                    }
                    // A full-detail block's vertices are its drawn ones; a coarser block's are
                    // read off the triangles it draws.
                    #[allow(clippy::cast_precision_loss)] // vertex indices
                    let h = if usize::from(b.side) == BLOCK_VERTICES {
                        b.heights[e * BLOCK_VERTICES + n]
                    } else {
                        block_height(b, Vec2::new(e as f32, n as f32) * CELL_METRES)
                    };
                    heights[at] = h;
                    best[at] = b.ring;
                    let word = b.words[e * BLOCK_VERTICES + n];
                    classes[at] = GroundClass::of_word(word);
                    #[allow(clippy::cast_possible_truncation)] // five bits
                    {
                        types[at] = ((word >> 2) & 0x1F) as u8;
                    }
                }
            }
        }
        let normals = smooth_normals(&heights, width, height);
        Some(Self {
            first_block: (x0, y0),
            origin: Vec2::new(first.x, first.y),
            size: (u32::try_from(width).ok()?, u32::try_from(height).ok()?),
            heights,
            normals,
            classes,
            types,
        })
    }

    /// What `blocks` would build: each block's id, generation, ring and origin, in order.
    #[must_use]
    pub fn signature(blocks: &[HifiTerrainBlock]) -> FieldSignature {
        let mut s: FieldSignature = blocks
            .iter()
            .map(|b| {
                (
                    b.block,
                    b.generation,
                    b.ring,
                    [b.origin.x.to_bits(), b.origin.y.to_bits()],
                )
            })
            .collect();
        s.sort_unstable();
        s
    }

    /// The global cell coordinates of texel (0, 0)'s cell.
    #[must_use]
    pub fn first_cell(&self) -> (i32, i32) {
        (
            i32::from(self.first_block.0) * 8,
            i32::from(self.first_block.1) * 8,
        )
    }

    fn texel(&self, x: i64, y: i64) -> f32 {
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        self.heights[(y * w + x) as usize]
    }

    /// The drawn landscape's height at render-space `(x, y)`: the shader's `plane_height`.
    #[must_use]
    pub fn plane_height(&self, x: f32, y: f32) -> f32 {
        let t = (Vec2::new(x, y) - self.origin) / CELL_METRES;
        #[allow(clippy::cast_precision_loss)] // texel counts
        let size = Vec2::new(self.size.0 as f32, self.size.1 as f32) - Vec2::ONE;
        let c = t
            .floor()
            .clamp(Vec2::ZERO, (size - Vec2::ONE).max(Vec2::ZERO));
        let f = (t - c).clamp(Vec2::ZERO, Vec2::ONE);
        #[allow(clippy::cast_possible_truncation)] // clamped to the field
        let (i, j) = (c.x as i64, c.y as i64);
        let h = [
            self.texel(i, j),
            self.texel(i + 1, j),
            self.texel(i + 1, j + 1),
            self.texel(i, j + 1),
        ];
        let (cx, cy) = self.first_cell();
        #[allow(clippy::cast_possible_truncation)] // a cell index inside the field
        let cut = split_diagonal(cx + i as i32, cy + j as i32);
        cell_plane_height(h, f, cut)
    }

    /// The bytes of the height texture, `r32float`.
    #[must_use]
    pub fn height_bytes(&self) -> Vec<u8> {
        self.heights.iter().flat_map(|h| h.to_le_bytes()).collect()
    }

    /// The bytes of the normal texture, `rg16float`.
    #[must_use]
    pub fn normal_bytes(&self) -> Vec<u8> {
        self.normals
            .iter()
            .flat_map(|n| [f16_bits(n[0]), f16_bits(n[1])])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    /// The bytes of a texture of the terrain types, `r8uint` (255 where no block covers it).
    #[must_use]
    pub fn type_bytes(&self) -> Vec<u8> {
        self.types.clone()
    }

    /// The bytes of the class texture, `r8uint`.
    #[must_use]
    pub fn class_bytes(&self) -> Vec<u8> {
        self.classes.iter().map(|c| *c as u8).collect()
    }
}

/// The height of block `b`'s drawn landscape at block-local `p`, metres.
fn block_height(b: &HifiTerrainBlock, p: Vec2) -> f32 {
    let side = usize::from(b.side);
    let cells = side - 1;
    #[allow(clippy::cast_precision_loss)] // 1..=8 cells
    let scale = BLOCK_METRES / cells as f32;
    let t = p / scale;
    #[allow(clippy::cast_precision_loss)]
    let top = (cells - 1) as f32;
    let c = t.floor().clamp(Vec2::ZERO, Vec2::splat(top));
    let f = (t - c).clamp(Vec2::ZERO, Vec2::ONE);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
    let (i, j) = (c.x as usize, c.y as usize);
    let v = |a: usize, n: usize| b.heights[a * side + n];
    let cut = b.cuts.get(i * cells + j).copied().unwrap_or(true);
    cell_plane_height([v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)], f, cut)
}

/// A smooth normal at every texel, from central differences of the heights (one-sided at the
/// field's edge), as its east and north parts.
fn smooth_normals(heights: &[f32], width: usize, height: usize) -> Vec<[f32; 2]> {
    let at = |x: usize, y: usize| heights[y * width + x];
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let (xl, xr) = (x.saturating_sub(1), (x + 1).min(width - 1));
            let (yd, yu) = (y.saturating_sub(1), (y + 1).min(height - 1));
            #[allow(clippy::cast_precision_loss)] // one or two texels
            let dx = (at(xr, y) - at(xl, y)) / ((xr - xl).max(1) as f32 * CELL_METRES);
            #[allow(clippy::cast_precision_loss)]
            let dy = (at(x, yu) - at(x, yd)) / ((yu - yd).max(1) as f32 * CELL_METRES);
            let n = Vec3::new(-dx, -dy, 1.0).normalize();
            out.push([n.x, n.y]);
        }
    }
    out
}

/// The bits of `v` as a half-precision float, rounded to nearest.
#[must_use]
pub fn f16_bits(v: f32) -> u16 {
    // A single-precision float's 23 stored mantissa bits, and the leading one they leave out.
    const MANTISSA: u32 = (1 << 23) - 1;
    const LEADING: u32 = 1 << 23;
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & MANTISSA;
    if exp == 0xFF {
        return sign | 0x7C00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | LEADING) >> (1 - e);
        let half = (m + 0x1000) >> 13;
        #[allow(clippy::cast_possible_truncation)] // under 0x400 plus a carry
        return sign | half as u16;
    }
    let m = mant + 0x1000;
    let (e, m) = if m & LEADING != 0 { (e + 1, 0) } else { (e, m) };
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 5 and 10 bits
    let out = sign | ((e as u16) << 10) | ((m >> 13) as u16);
    out
}

/// One uploaded texture of the field: the texture, its view and the bytes it takes.
pub type Uploaded = (wgpu::Texture, wgpu::TextureView, u64);

impl TerrainField {
    /// The parameters a shader reads the field through, as `TerrainFieldParams` lays them out.
    #[must_use]
    pub fn params(&self) -> [u8; 32] {
        let (w, h) = self.size;
        let mut params = [0u8; 32];
        #[allow(clippy::cast_precision_loss)] // texel counts
        let floats = [self.origin.x, self.origin.y, w as f32, h as f32];
        for (i, f) in floats.iter().enumerate() {
            params[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
        }
        let (cx, cy) = self.first_cell();
        params[16..20].copy_from_slice(&cx.to_le_bytes());
        params[20..24].copy_from_slice(&cy.to_le_bytes());
        params
    }

    /// The bytes the three textures take on the device.
    #[must_use]
    pub fn device_bytes(&self) -> u64 {
        u64::from(self.size.0) * u64::from(self.size.1) * (4 + 4 + 1)
    }

    /// Upload the field as its height (`r32float`), normal (`rg16float`) and class (`r8uint`)
    /// textures, in that order.
    #[must_use]
    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> [Uploaded; 3] {
        [
            self.upload_one(
                device,
                queue,
                wgpu::TextureFormat::R32Float,
                4,
                &self.height_bytes(),
            ),
            self.upload_one(
                device,
                queue,
                wgpu::TextureFormat::Rg16Float,
                4,
                &self.normal_bytes(),
            ),
            self.upload_one(
                device,
                queue,
                wgpu::TextureFormat::R8Uint,
                1,
                &self.class_bytes(),
            ),
        ]
    }

    /// Upload the terrain types (`r8uint`).
    #[must_use]
    pub fn upload_types(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Uploaded {
        self.upload_one(
            device,
            queue,
            wgpu::TextureFormat::R8Uint,
            1,
            &self.type_bytes(),
        )
    }

    fn upload_one(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        texel: u32,
        data: &[u8],
    ) -> Uploaded {
        let (w, h) = self.size;
        let make = |format: wgpu::TextureFormat, texel: u32, data: &[u8]| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("high-fidelity landscape field"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * texel),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (
                texture,
                view,
                u64::from(w) * u64::from(h) * u64::from(texel),
            )
        };
        make(format, texel, data)
    }
}
