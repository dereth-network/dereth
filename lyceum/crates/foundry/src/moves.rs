//! The checks of a retail building the packs moved, beside the world's other checks
//! ([`crate::validate`]): each is a warning, which stops nothing.
//!
//! - **What it now stands on:** a static, or another building, inside the moved building. Every
//!   static of the landblock is looked at: one that stood inside the building before the move and
//!   still stands where it was is named as left behind.
//! - **Its doors:** an opening that met the ground before the move (within [`DOOR_HEIGHT`]) and
//!   now stands over or under it by more, so the door is no longer at walking height. Openings
//!   that never met the ground (windows, upper floors) are not doors.
//! - **A house's plot:** the land cells its restriction table gives the house. Retail's every house
//!   (3,490) stands on a land cell of its plot; a house moved off its plot keeps a yard where it
//!   no longer is.
//!
//! The texts start with the statics record's id and the building's place in it, as
//! `0xA9B4FFFE: building 8: ...`.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{CellLandblock, EnvCell, LandblockInfo};
use dereth_assets::Region;
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::{DataId, Vec3};

use crate::dats::DatSet;
use crate::footprint::{Environments, Footprints};

/// How far over or under the ground a door may stand and still be at walking height, in meters.
pub const DOOR_HEIGHT: f32 = 0.5;

/// What a landblock holds, as one set of files reads it.
struct Block {
    info: LandblockInfo,
    cells: BTreeMap<u16, EnvCell>,
    land: Option<CellLandblock>,
}

fn block(set: &DatSet, lb: u16) -> Option<Block> {
    let first = u32::from(lb) << 16;
    let read = |low: u32| set.cell.read(DataId(first | low)).ok();
    let info = set
        .decode::<LandblockInfo>(DataId(first | 0xFFFE), &read(0xFFFE)?)
        .ok()?;
    let mut cells = BTreeMap::new();
    for n in 0x0100..0x0100 + info.num_cells {
        let id = DataId(first | n);
        if let Some(c) = read(n).and_then(|b| set.decode::<EnvCell>(id, &b).ok()) {
            if let Ok(n) = u16::try_from(n) {
                cells.insert(n, c);
            }
        }
    }
    let land =
        read(0xFFFF).and_then(|b| set.decode::<CellLandblock>(DataId(first | 0xFFFF), &b).ok());
    Some(Block { info, cells, land })
}

/// The cells of building `index`: those its doorways lead into and every cell joined to them.
fn rooms(b: &Block, index: usize) -> BTreeSet<u16> {
    let doorways: Vec<u16> = b
        .info
        .buildings
        .get(index)
        .map(|x| x.portals.iter().map(|p| p.other_cell_id).collect())
        .unwrap_or_default();
    crate::cells::building_cells(&doorways, &b.cells)
}

/// Points well inside each of `cells`: each corner pulled halfway to its cell's middle.
fn inner_points(
    b: &Block,
    cells: &BTreeSet<u16>,
    envs: &mut Environments,
    set: &DatSet,
) -> Vec<Vec3> {
    let mut out = Vec::new();
    for c in cells {
        let Some(cell) = b.cells.get(c) else { continue };
        let Some(e) = envs.get(set, cell.environment) else {
            continue;
        };
        let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
            continue;
        };
        let pts: Vec<Vec3> = cs
            .vertex_array
            .vertices
            .iter()
            .map(|v| localtoglobal(&cell.frame, v.position))
            .collect();
        if pts.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let k = pts.len() as f32;
        let m = pts.iter().fold(Vec3::ZERO, |s, p| {
            Vec3::new(s.x + p.x / k, s.y + p.y / k, s.z + p.z / k)
        });
        out.push(m);
        out.extend(
            pts.iter()
                .map(|p| Vec3::new((p.x + m.x) / 2.0, (p.y + m.y) / 2.0, (p.z + m.z) / 2.0)),
        );
    }
    out
}

/// Each opening of building `index` to the outdoors: the cell it leads into, its lowest point,
/// and the middle of the opening (landblock meters).
fn openings(
    b: &Block,
    index: usize,
    envs: &mut Environments,
    set: &DatSet,
) -> Vec<(u16, f32, Vec3)> {
    let Some(building) = b.info.buildings.get(index) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in &building.portals {
        let Some(cell) = b.cells.get(&p.other_cell_id) else {
            continue;
        };
        let Some(cp) = usize::try_from(p.other_portal_id)
            .ok()
            .and_then(|i| cell.portals.get(i))
        else {
            continue;
        };
        let Some(e) = envs.get(set, cell.environment) else {
            continue;
        };
        let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
            continue;
        };
        let Some(poly) = cs.polygons.get(usize::from(cp.polygon_id)) else {
            continue;
        };
        let pts: Vec<Vec3> = poly
            .vertex_ids
            .iter()
            .filter_map(|v| cs.vertex_array.vertices.get(usize::from(*v)))
            .map(|v| localtoglobal(&cell.frame, v.position))
            .collect();
        if pts.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let k = pts.len() as f32;
        let m = pts.iter().fold(Vec3::ZERO, |s, q| {
            Vec3::new(s.x + q.x / k, s.y + q.y / k, s.z + q.z / k)
        });
        let low = pts.iter().map(|q| q.z).fold(f32::MAX, f32::min);
        out.push((p.other_cell_id, low, m));
    }
    out
}

/// The ground's height at `(x, y)` of landblock `lb`, as `land` and `region` make it; `None` off
/// the landblock.
fn ground(land: &CellLandblock, region: &Region, lb: u16, x: f32, y: f32) -> Option<f32> {
    ((0.0..192.0).contains(&x) && (0.0..192.0).contains(&y))
        .then(|| crate::compile::ground(land, region, lb, x, y))
}

/// The checks of the buildings `moved` (by their places in the statics record) of landblock `lb`:
/// `before` holds the landblock as it was, `now` as the packs leave it.
#[must_use]
pub fn check(before: &DatSet, now: &DatSet, lb: u16, moved: &BTreeSet<usize>) -> Vec<String> {
    let (Some(was), Some(is)) = (block(before, lb), block(now, lb)) else {
        return Vec::new();
    };
    let region = now
        .read_portal(crate::compile::REGION)
        .and_then(|b| now.decode::<Region>(crate::compile::REGION, &b).ok());
    let mut envs = Environments::default();
    let fp_was = Footprints::of(before, lb, &was.info, &mut envs);
    let fp_is = Footprints::of(now, lb, &is.info, &mut envs);
    let head = |b: usize| format!("0x{lb:04X}FFFE: building {b}");
    let mut out = Vec::new();
    for &b in moved {
        if b >= is.info.buildings.len() {
            continue;
        }
        // Statics inside it now, and those it left behind.
        for (s, o) in is.info.objects.iter().enumerate() {
            let p = o.frame.origin;
            let inside_now = fp_is.building_at(p) == Some(b);
            let inside_before = fp_was.building_at(p) == Some(b);
            if inside_now && !inside_before {
                out.push(format!(
                    "{}: static {s} ({:#010X} at {:.1}, {:.1}, {:.1}) stands inside it",
                    head(b),
                    o.id.raw(),
                    p.x,
                    p.y,
                    p.z
                ));
            } else if inside_before && !inside_now {
                out.push(format!(
                    "{}: static {s} ({:#010X}) stood inside it and stays where it was",
                    head(b),
                    o.id.raw()
                ));
            }
        }
        // Other buildings: a point well inside one in the other, where none was before.
        let mut points = |blk: &Block, i: usize, set: &DatSet| {
            let r = rooms(blk, i);
            inner_points(blk, &r, &mut envs, set)
        };
        let (mine_was, mine_is) = (points(&was, b, before), points(&is, b, now));
        for other in 0..is.info.buildings.len() {
            if other == b {
                continue;
            }
            let (theirs_was, theirs_is) = (points(&was, other, before), points(&is, other, now));
            let meets = |mine: &[Vec3], theirs: &[Vec3], fp: &Footprints| {
                mine.iter().any(|p| fp.building_at(*p) == Some(other))
                    || theirs.iter().any(|p| fp.building_at(*p) == Some(b))
            };
            if meets(&mine_is, &theirs_is, &fp_is) && !meets(&mine_was, &theirs_was, &fp_was) {
                out.push(format!(
                    "{}: it overlaps building {other} ({:#010X})",
                    head(b),
                    is.info.buildings[other].id.raw()
                ));
            }
        }
        // Its doors: those at the ground before, at the ground still.
        if let (Some(region), Some(land_was), Some(land_is)) =
            (region.as_ref(), was.land.as_ref(), is.land.as_ref())
        {
            let doors_was: BTreeMap<u16, f32> = openings(&was, b, &mut envs, before)
                .into_iter()
                .filter_map(|(c, low, m)| {
                    let g = ground(land_was, region, lb, m.x, m.y)?;
                    ((low - g).abs() <= DOOR_HEIGHT).then_some((c, low - g))
                })
                .collect();
            for (c, low, m) in openings(&is, b, &mut envs, now) {
                if !doors_was.contains_key(&c) {
                    continue;
                }
                let Some(g) = ground(land_is, region, lb, m.x, m.y) else {
                    out.push(format!(
                        "{}: its door into cell {c:04X} stands off the landblock",
                        head(b)
                    ));
                    continue;
                };
                let gap = low - g;
                if gap.abs() > DOOR_HEIGHT {
                    out.push(format!(
                        "{}: its door into cell {c:04X} stands {:.1} m {} the ground",
                        head(b),
                        gap.abs(),
                        if gap > 0.0 { "over" } else { "under" }
                    ));
                }
            }
        }
        // A house: its plot under it.
        let owner = is.info.buildings[b]
            .portals
            .iter()
            .filter_map(|p| is.cells.get(&p.other_cell_id))
            .find_map(|c| c.restriction_obj);
        if let Some(owner) = owner {
            let plot: BTreeSet<u32> = is
                .info
                .restrictions
                .as_ref()
                .map(|r| {
                    r.entries
                        .iter()
                        .filter(|(c, o)| *o == owner && c & 0xFFFF < 0x0100)
                        .map(|(c, _)| *c)
                        .collect()
                })
                .unwrap_or_default();
            let o = is.info.buildings[b].frame.origin;
            let under = (u32::from(lb) << 16) | crate::edit::outdoor_cell(o.x, o.y);
            if !plot.is_empty() && !plot.contains(&under) {
                out.push(format!(
                    "{}: it is a house moved off its plot: the plot's {} land cell(s) stay where \
                     they were, and the land cell under it ({under:#010X}) is not one",
                    head(b),
                    plot.len()
                ));
            }
        }
    }
    out
}
