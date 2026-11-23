//! AC2's art in AC1's formats, for the portal overlay: AC2's ground pictures as the land slots'
//! terrain textures, and AC2's road masks as the region's road alpha maps; and the ids, pictures
//! and records every part of the converter shares.
//!
//! **Pictures** keep AC2's own blocks: a block-compressed picture becomes an AC1 image in the
//! same format, so nothing is lost or re-encoded; one image and one image texture per AC2 picture
//! list and size, shared by every ground and model that draws it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ac2_look::source::Ac2;
use dereth_assets::region::TerrainDesc;
use dereth_assets::{Region, RenderSurface, SurfaceTexture};

/// The greatest side of an AC2 ground picture taken, texels.
pub const GROUND_PICTURE_LIMIT: u32 = 1024;
/// The image category AC1's terrain pictures carry.
const TERRAIN_IMAGE_CATEGORY: u32 = 2;
pub use dereth_ac2_convert::records::{Ids, Pictures, Records};

/// AC2's ground as AC1's terrain textures: the texture and tiling per terrain type, and a table
/// of what was taken.
pub struct Ground {
    pub set: BTreeMap<u32, (u32, u32)>,
    /// An entry for each variant on the land that is not its slot's own kind, after AC1's.
    pub extra: Vec<TerrainDesc>,
    pub tsv: String,
}

/// Each land slot drawn with AC2's ground, and each of its variants: the slot its own kind
/// ([`crate::ac2_world::textures::slot_kinds`]), each other variant on the land (`counts` by slot and
/// variant) its kind, under an entry of its own; each its flat ground's first picture, repeated as
/// often as fits its tile across a 24 m cell (`cell` meters); and the road with AC2's roads 1 to 3,
/// each its own. Water slots keep AC1's.
#[allow(clippy::too_many_arguments)]
pub fn ground_textures(
    ac2: &Ac2,
    region: &Region,
    counts: &BTreeMap<(u16, u16), usize>,
    steep: &[crate::ac2_world::steep::Steep],
    cell: f64,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Ground, String> {
    use crate::ac2_world::textures;
    let reg = ac2.region()?;
    let tm = region
        .land_surf
        .tex_merge
        .as_ref()
        .ok_or("AC1's region has no terrain textures")?;
    let own = textures::slot_kinds(counts);
    // (terrain type, what it is, the picture list and its tile).
    let mut picks: Vec<(u32, String, u32, f32)> = Vec::new();
    let kinds = own
        .iter()
        .map(|(&slot, &(_, kind))| (u32::from(slot), kind))
        .chain(
            textures::variant_kinds(counts, &own)
                .into_iter()
                .map(|(slot, variant, kind)| (textures::variant_type(slot, variant), kind)),
        );
    for (slot, kind) in kinds {
        let Some(k) = reg.terrain.terrains.iter().find(|k| k.index == kind) else {
            return Err(format!("AC2 has no kind {kind}"));
        };
        let base = reg
            .surface
            .surfaces
            .iter()
            .find(|s| s.index == k.surface)
            .and_then(|s| s.groups.first())
            .and_then(|g| g.base.first());
        let list = base.map_or(0, |b| ac2.ground_picture(b.material.0));
        let Some(base) = base.filter(|_| list != 0) else {
            eprintln!(
                "ground: slot {slot} ({}) draws no picture; AC1's stays",
                k.name
            );
            continue;
        };
        picks.push((slot, format!("{kind} {}", k.name), list, base.tile_width));
    }
    // Each steep variant on the land: its group's picture.
    for v in steep {
        if counts.get(&(v.slot, v.variant)).is_none_or(|&n| n == 0) {
            continue;
        }
        let list = ac2.ground_picture(v.material);
        if list == 0 {
            eprintln!(
                "ground: steep {:08X} draws no picture; its slot's stays",
                v.material
            );
            continue;
        }
        let kinds: Vec<String> = v.from.iter().map(|(k, gi)| format!("{k}/{gi}")).collect();
        picks.push((
            textures::variant_type(v.slot, v.variant),
            format!("steep {:08X} (kind/group {})", v.material, kinds.join(" ")),
            list,
            v.tile,
        ));
    }
    // Each of AC2's roads 1 to 3 (the road bits keep its value), road 1 as the road itself.
    for r in &reg.surface.roads {
        if !(1..=3).contains(&r.index) {
            continue;
        }
        let Some(base) = r.groups.first().and_then(|g| g.base.first()) else {
            continue;
        };
        let list = ac2.ground_picture(base.material.0);
        if list != 0 {
            picks.push((
                textures::road_type(r.index),
                format!("road {}", r.index),
                list,
                base.tile_width,
            ));
        }
    }
    let mut g = Ground {
        set: BTreeMap::new(),
        extra: Vec::new(),
        tsv: String::from(
            "ac1_type\tac2_kind\tpicture_list\tpicture\ttile_m\ttiling\tsurface_texture\n",
        ),
    };
    for (ty, what, list, tile) in picks {
        // A variant's entry is its slot's, and a road's the road's, with its own picture.
        let own_type = ty & 0xFF;
        let Some(own) = tm.terrain_desc.iter().find(|d| d.terrain_type == own_type) else {
            return Err(format!("AC1's region has no terrain texture {own_type}"));
        };
        let (tex, note) = pictures.texture(
            ac2,
            list,
            GROUND_PICTURE_LIMIT,
            TERRAIN_IMAGE_CATEGORY,
            ids,
            out,
        )?;
        let tiling = textures::tiling(tile, cell);
        if ty == own_type {
            g.set.insert(ty, (tex.raw(), tiling));
        } else {
            g.extra.push(TerrainDesc {
                terrain_type: ty,
                tex_gid: tex,
                tex_tiling: tiling,
                ..*own
            });
        }
        let _ = writeln!(
            g.tsv,
            "{ty}\t{what}\t{list:08X}\t{note}\t{tile}\t{tiling}\t{:08X}",
            tex.raw()
        );
    }
    Ok(g)
}

/// The side of the road alpha maps written, texels: AC1's own maps'.
const ROAD_MAP_SIDE: usize = 512;
/// AC1's terrain alpha maps' pixel format: one byte a texel.
const PFID_CUSTOM_LSCAPE_ALPHA: u32 = 0xF4;

/// AC2's road 1's first mask of each shape, as road shares (its alpha), checked to lie as
/// [`crate::ac2_world::roads::Mask`] says AC2 lays them.
fn ac2_road_masks(ac2: &Ac2) -> Result<[(u32, usize, Vec<f32>); 3], String> {
    let reg = ac2.region()?;
    let group = reg
        .surface
        .roads
        .iter()
        .find(|r| r.index == 1)
        .and_then(|r| r.groups.first())
        .ok_or("AC2 has no road 1")?;
    let take = |what: &str, list: &[auberean_assets::region::WeightedMaterial]| {
        let id = list
            .first()
            .map(|m| m.material.0)
            .ok_or_else(|| format!("AC2's road 1 has no {what} mask"))?;
        let p = ac2
            .picture(id, u32::try_from(ROAD_MAP_SIDE).unwrap_or(u32::MAX))
            .ok_or_else(|| format!("AC2's {what} mask {id:08X} will not read"))?;
        let share: Vec<f32> = p
            .texels
            .chunks(4)
            .map(|t| f32::from(t[3]) / 255.0)
            .collect();
        Ok::<_, String>((id, p.width as usize, share))
    };
    let masks = [
        take("corner", &group.corner_masks)?,
        take("side", &group.side_masks)?,
        take("diagonal", &group.diagonal_masks)?,
    ];
    // Where each must lay road and where not, (u, v) from the top left.
    type Points = &'static [(f64, f64)];
    let expect: [(Points, Points); 3] = [
        (&[(0.05, 0.05)], &[(0.5, 0.5), (0.95, 0.95)]),
        (
            &[(0.5, 0.05), (0.05, 0.05), (0.95, 0.05)],
            &[(0.5, 0.95), (0.5, 0.6)],
        ),
        (
            &[(0.05, 0.95), (0.5, 0.5), (0.95, 0.05)],
            &[(0.05, 0.05), (0.95, 0.95)],
        ),
    ];
    for ((id, w, share), (on, off)) in masks.iter().zip(expect) {
        let m = crate::ac2_world::roads::Mask {
            width: *w,
            height: share.len() / w,
            share,
        };
        if on.iter().any(|&(u, v)| m.at(u, v) < 0.5) || off.iter().any(|&(u, v)| m.at(u, v) >= 0.5)
        {
            return Err(format!("AC2's road mask {id:08X} does not lie as expected"));
        }
    }
    Ok(masks)
}

/// AC2's road masks as AC1's road alpha maps, one in place of each of `region`'s (by its code),
/// stretched by `stretch` (AC2's square over the converted one): the new texture of each code,
/// and a table of what was made.
pub fn road_maps(
    ac2: &Ac2,
    region: &Region,
    stretch: f64,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<(BTreeMap<u32, u32>, String), String> {
    use crate::ac2_world::roads::{alpha_map, Mask, Masks};
    let tm = region
        .land_surf
        .tex_merge
        .as_ref()
        .ok_or("AC1's region has no terrain textures")?;
    let [corner, side, diagonal] = ac2_road_masks(ac2)?;
    fn mask((_, w, s): &(u32, usize, Vec<f32>)) -> Mask<'_> {
        Mask {
            width: *w,
            height: s.len() / w,
            share: s.as_slice(),
        }
    }
    let masks = Masks {
        corner: mask(&corner),
        side: mask(&side),
        diagonal: mask(&diagonal),
    };
    let mut made = BTreeMap::new();
    let mut tsv = String::from("code\tac1_map\tac2_mask\tmap\n");
    for m in &tm.road_maps {
        let bytes = alpha_map(&masks, m.code, stretch, ROAD_MAP_SIDE).ok_or_else(|| {
            format!(
                "AC1's road map {:08X} has code {}, of no one shape",
                m.tex_gid.raw(),
                m.code
            )
        })?;
        let from = match crate::ac2_world::roads::shape_of(m.code).map(|s| s.0) {
            Some(crate::ac2_world::roads::Shape::Corner) => corner.0,
            Some(crate::ac2_world::roads::Shape::Side) => side.0,
            _ => diagonal.0,
        };
        let (rs_id, st_id) = (ids.next(0x06), ids.next(0x05));
        let side = u32::try_from(ROAD_MAP_SIDE).unwrap_or(512);
        let rs = RenderSurface {
            id: rs_id,
            data_category: TERRAIN_IMAGE_CATEGORY,
            width: side,
            height: side,
            format: PFID_CUSTOM_LSCAPE_ALPHA,
            image_size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            data_offset: 0x18,
            default_palette_id: None,
        };
        out.add(rs_id, rs.encode(&bytes));
        let st = SurfaceTexture {
            id: st_id,
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![rs_id],
        };
        out.add(st_id, st.encode());
        made.insert(m.code, st_id.raw());
        let _ = writeln!(
            tsv,
            "{}\t{:08X}\t{from:08X}\t{:08X}",
            m.code,
            m.tex_gid.raw(),
            st_id.raw()
        );
    }
    Ok((made, tsv))
}
