//! A landblock's indoor cells as the packs make them: new cells placed from environment pieces,
//! and buildings of the base moved or turned with every cell inside them.
//!
//! **New cells** are numbered after the base's, in the order the packs place them, so cell ids
//! stay dense from `0x0100` and the statics record's cell count stays exact. Each cell's portals
//! follow its piece's portal list: a portal joined to another new cell names that cell and its
//! portal there, and the two must name each other; a portal to the outdoors is written as the
//! files write theirs; a shut one is left out. Two joined portals whose polygons coincide are an
//! exact match, as nearly every retail portal is.
//!
//! **A reshaped cell** is one of the landblock's own with another piece in its place: it keeps
//! its number, its statics and every way it had, each met by one of the new piece's portals
//! (`keep`); the piece's other portals are shut, free to build new cells on, or join new cells.
//! The cells its ways lead to name its portals by their new places.
//!
//! **A cell taken away** is one of the landblock's own a pack removes: it keeps its number, closed
//! and empty far under the landblock, and every way into it is shut, so the cells around it keep
//! theirs and the server still finds every cell by its number.
//!
//! **A moved building** takes the cells its doorways lead into, and every cell joined to those,
//! with it: each cell's frame and each of its statics get the same move, about the building's
//! origin. Portals, the doorways and the cells' ids stay as they are.
//!
//! Every cell made or moved gets its stab list again ([`crate::stab`]) and its seen-outside flag.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{CellPortal, EnvCell, LandblockInfo, ObjectEntry};
use dereth_assets::Environment;
use dereth_primitives::frame::{combine, globaltolocalvec, l2g, localtoglobal, V3};
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::compile::quat_of;
use crate::error::Error;
use crate::source::{AddCell, Link, Located, MoveBuilding, ReplaceCell};
use crate::stab;

/// The first indoor cell's index.
pub const FIRST_CELL: u16 = 0x0100;

/// How far apart two portal corners may be and still coincide, in meters.
pub const COINCIDE: f32 = 0.01;

/// What a landblock's cells become: the records made or changed, by cell index, and the cell
/// each new cell's symbol was given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placed {
    pub cells: BTreeMap<u16, EnvCell>,
    pub names: BTreeMap<String, u16>,
    /// The cell each dropped building's room was given, by the room's name.
    pub rooms: BTreeMap<String, u16>,
    /// Each dropped building's name and its place in the building list.
    pub dropped: Vec<(String, usize)>,
    /// One line for each building moved and for the cells placed.
    pub summary: Vec<String>,
    /// Doorways removed by replacements.
    pub warnings: Vec<String>,
}

/// The inverse of a frame: what takes its space back to the one it was placed in.
#[must_use]
pub fn inverse(f: &Frame) -> Frame {
    let q = Quat::new(f.rotation.w, -f.rotation.x, -f.rotation.y, -f.rotation.z);
    let o = globaltolocalvec(l2g(f.rotation), f.origin);
    Frame::new(Vec3::new(-o.x, -o.y, -o.z), q)
}

/// `f` moved as a thing placed in `from` moves when `from` becomes `to`.
#[must_use]
pub fn carried(f: &Frame, from: &Frame, to: &Frame) -> Frame {
    combine(to, &combine(&inverse(from), f))
}

/// The cells of the building whose doorways lead into `doorways`: those cells and every cell
/// joined to them by a cell portal.
#[must_use]
pub fn building_cells(doorways: &[u16], cells: &BTreeMap<u16, EnvCell>) -> BTreeSet<u16> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<u16> = doorways.to_vec();
    while let Some(c) = todo.pop() {
        if !out.insert(c) {
            continue;
        }
        if let Some(cell) = cells.get(&c) {
            for p in &cell.portals {
                if p.flags & 4 == 0 {
                    #[allow(clippy::cast_possible_truncation)]
                    todo.push(p.other_cell_id as u16);
                }
            }
        }
    }
    out
}

/// A portal polygon's corners in landblock space: piece `piece` of `env`, polygon `polygon`,
/// placed by `frame`.
fn corners(env: &Environment, piece: u16, polygon: u16, frame: &Frame) -> Option<Vec<Vec3>> {
    let cs = env.cells.get(usize::from(piece))?;
    let poly = cs.polygons.get(usize::from(polygon))?;
    poly.vertex_ids
        .iter()
        .map(|i| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| localtoglobal(frame, v.position))
        })
        .collect()
}

/// Whether two polygons have the same corners, in any order and either winding.
fn coincide(a: &[Vec3], b: &[Vec3]) -> bool {
    meet(a, b, COINCIDE)
}

/// Whether two polygons have the same corners to within `tolerance`, in any order and either
/// winding.
fn meet(a: &[Vec3], b: &[Vec3], tolerance: f32) -> bool {
    a.len() == b.len()
        && a.iter().all(|p| {
            b.iter().any(|q| {
                (p.x - q.x).abs() <= tolerance
                    && (p.y - q.y).abs() <= tolerance
                    && (p.z - q.z).abs() <= tolerance
            })
        })
}

/// How far apart a reshaped cell's portal and the way it keeps may be, in meters: a piece snapped
/// in place by the editor, its corners written to the millimeter.
pub const KEEP: f32 = 0.05;

/// How many portal records a new cell's link writes: none when shut, one for each cell it joins.
fn records(l: &Link) -> usize {
    match l {
        Link::Shut => 0,
        Link::Many(v) => v.len(),
        _ => 1,
    }
}

/// Whether link `l` names `target`: is it, or is one of the cells it joins.
fn names(l: Option<&Link>, target: &Link) -> bool {
    match l {
        Some(Link::Many(v)) => v.contains(target),
        Some(l) => l == target,
        None => false,
    }
}

/// The place, among a new cell's written portal records, of the one its portal `slot` (of links
/// `links`) writes for `to`.
fn written_to(links: &[Link], slot: usize, to: &Link) -> Option<i16> {
    let before: usize = links.get(..slot)?.iter().map(records).sum();
    let k = match links.get(slot)? {
        Link::Shut => return None,
        Link::Many(v) => v.iter().position(|l| l == to)?,
        _ => 0,
    };
    i16::try_from(before + k).ok()
}

/// Whether a cell is on the front of its portal polygon (the side its corners' winding faces),
/// which the portal record's bit 1 stores.
fn portal_side(env: &Environment, piece: u16, polygon: u16) -> Option<bool> {
    let cs = env.cells.get(usize::from(piece))?;
    let poly = cs.polygons.get(usize::from(polygon))?;
    let pts: Vec<Vec3> = poly
        .vertex_ids
        .iter()
        .filter_map(|i| {
            cs.vertex_array
                .vertices
                .get(usize::from(*i))
                .map(|v| v.position)
        })
        .collect();
    if pts.len() < 3 || cs.vertex_array.vertices.is_empty() {
        return None;
    }
    let mut n = Vec3::ZERO;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    #[allow(clippy::cast_precision_loss)]
    let inside = {
        let k = cs.vertex_array.vertices.len() as f32;
        let s = cs
            .vertex_array
            .vertices
            .iter()
            .fold(Vec3::ZERO, |s, v| s.add(v.position));
        Vec3::new(s.x / k, s.y / k, s.z / k)
    };
    Some(n.dot(inside.sub(pts[0])) > 0.0)
}

/// Reshape cell `r.item.cell` of landblock `lb` in `all`: the new piece in its place, keeping
/// its number, its statics and matched doorways; neighboring return links are removed or renumbered,
/// and all of them in `changed`.
#[allow(clippy::too_many_arguments)]
fn reshape(
    lb: u16,
    r: &Located<ReplaceCell>,
    pieces: &BTreeMap<u32, Environment>,
    adds: &BTreeMap<&str, &Located<AddCell>>,
    names_of: &BTreeMap<String, u16>,
    env: &dyn Fn(DataId) -> Option<Environment>,
    all: &mut BTreeMap<u16, EnvCell>,
    changed: &mut BTreeSet<u16>,
    info: &mut LandblockInfo,
) -> Result<usize, Error> {
    let item = &r.item;
    let n = item.cell;
    let Some(old) = all.get(&n).cloned() else {
        return Err(Error::source(
            &r.at,
            format!("landblock {lb:04X} has no cell {n:04X}"),
        ));
    };
    let e = &pieces[&item.environment.raw()];
    let Some(cs) = e.cells.get(usize::from(item.piece)) else {
        return Err(Error::source(
            &r.at,
            format!(
                "environment {:#010X} has {} piece(s); there is no {}",
                item.environment.raw(),
                e.cells.len(),
                item.piece
            ),
        ));
    };
    if item.portals.len() != cs.portals.len() {
        return Err(Error::source(
            &r.at,
            format!(
                "the piece has {} portal(s); {} are given",
                cs.portals.len(),
                item.portals.len()
            ),
        ));
    }
    let Some(old_env) = env(old.environment) else {
        return Err(Error::source(
            &r.at,
            format!(
                "the files have no environment {:#010X}",
                old.environment.raw()
            ),
        ));
    };
    let ways: Vec<Option<Vec<Vec3>>> = old
        .portals
        .iter()
        .map(|p| corners(&old_env, old.cell_struct, p.polygon_id, &old.frame))
        .collect();
    let frame = Frame::new(
        Vec3::new(item.at[0], item.at[1], item.at[2]),
        quat_of(item.rotation),
    );
    let mut kept = vec![false; old.portals.len()];
    // Each way kept: its old place among the cell's portals, and its new.
    let mut renumbered: BTreeMap<i16, i16> = BTreeMap::new();
    let mut portals: Vec<CellPortal> = Vec::new();
    for (slot, link) in item.portals.iter().enumerate() {
        let polygon = cs.portals[slot];
        let front = portal_side(e, item.piece, polygon).unwrap_or(true);
        let mine = corners(e, item.piece, polygon, &frame);
        // A new cell this portal joins, as a portal record.
        let join = |cell: &str, portal: usize| -> Result<CellPortal, Error> {
            let Some(other) = adds.get(cell) else {
                return Err(Error::source(
                    &r.at,
                    format!("no cell {cell} is placed in landblock {lb:04X}"),
                ));
            };
            let me = Link::Base {
                cell: n,
                portal: slot,
            };
            if !names(other.item.portals.get(portal), &me) {
                return Err(Error::source(
                    &r.at,
                    format!(
                        "portal {slot} of cell {n:04X} leads to portal {portal} of {cell}, which does not lead back"
                    ),
                ));
            }
            let oe = &pieces[&other.item.environment.raw()];
            let opoly = oe.cells[usize::from(other.item.piece)].portals[portal];
            let oframe = Frame::new(
                Vec3::new(other.item.at[0], other.item.at[1], other.item.at[2]),
                quat_of(other.item.rotation),
            );
            let theirs = corners(oe, other.item.piece, opoly, &oframe);
            let exact = matches!((&mine, &theirs), (Some(m), Some(t)) if coincide(m, t));
            Ok(CellPortal {
                flags: flags(exact, front, false),
                polygon_id: polygon,
                other_cell_id: u32::from(names_of[cell]),
                other_portal_id: written_to(&other.item.portals, portal, &me).unwrap_or(-1),
            })
        };
        match link {
            Link::Shut => {}
            Link::Outside => portals.push(CellPortal {
                flags: flags(true, front, true),
                polygon_id: polygon,
                other_cell_id: 0xFFFF_FFFF,
                other_portal_id: -1,
            }),
            Link::Keep => {
                // Every way the opening had: one cell's, or several through parts of it.
                let ks: Vec<usize> = (0..old.portals.len())
                    .filter(|k| {
                        !kept[*k]
                            && matches!((&mine, &ways[*k]), (Some(m), Some(w)) if meet(m, w, KEEP))
                    })
                    .collect();
                if ks.is_empty() {
                    return Err(Error::source(
                        &r.at,
                        format!("portal {slot} of the piece meets none of cell {n:04X}'s ways"),
                    ));
                }
                for k in ks {
                    kept[k] = true;
                    let way = old.portals[k];
                    let outside = way.other_cell_id == 0xFFFF_FFFF;
                    if let (Ok(from), Ok(to)) = (i16::try_from(k), i16::try_from(portals.len())) {
                        renumbered.insert(from, to);
                    }
                    portals.push(CellPortal {
                        flags: flags(true, front, outside),
                        polygon_id: polygon,
                        other_cell_id: way.other_cell_id,
                        other_portal_id: way.other_portal_id,
                    });
                }
            }
            Link::Cell { cell, portal } => portals.push(join(cell, *portal)?),
            Link::Many(list) => {
                for l in list {
                    match l {
                        Link::Cell { cell, portal } => portals.push(join(cell, *portal)?),
                        _ => {
                            return Err(Error::source(
                                &r.at,
                                "a reshaped cell joins new cells by their names",
                            ));
                        }
                    }
                }
            }
            Link::Base { .. } => {
                return Err(Error::source(
                    &r.at,
                    "a reshaped cell joins new cells by their names",
                ));
            }
        }
    }
    let dropped = kept.iter().filter(|k| !**k).count();
    let mut cell = old;
    cell.surfaces.clone_from(&item.surfaces);
    cell.environment = item.environment;
    cell.cell_struct = item.piece;
    cell.frame = frame;
    // The ways it keeps lead where they led: the cells there name its portals anew.
    let neighbors: BTreeSet<u16> = cell
        .portals
        .iter()
        .filter(|p| p.other_cell_id != 0xFFFF_FFFF)
        .filter_map(|p| u16::try_from(p.other_cell_id).ok())
        .collect();
    cell.portals = portals;
    all.insert(n, cell);
    changed.insert(n);
    remap_building_doors(info, n, &renumbered);
    for o in neighbors {
        if let Some(c) = all.get_mut(&o) {
            let mut remap = BTreeMap::new();
            let mut retained = Vec::new();
            for (index, mut p) in c.portals.iter().copied().enumerate() {
                if p.other_cell_id == u32::from(n) {
                    let Some(to) = renumbered.get(&p.other_portal_id) else {
                        continue;
                    };
                    p.other_portal_id = *to;
                }
                if let (Ok(from), Ok(to)) = (i16::try_from(index), i16::try_from(retained.len())) {
                    remap.insert(from, to);
                }
                retained.push(p);
            }
            c.portals = retained;
            remap_building_doors(info, o, &remap);
            changed.insert(o);
            // Removing a neighbor's return doorway changes the indices used by its neighbors.
            for (other, cell) in all.iter_mut() {
                for p in &mut cell.portals {
                    if p.other_cell_id == u32::from(o) {
                        if let Some(to) = remap.get(&p.other_portal_id) {
                            if p.other_portal_id != *to {
                                p.other_portal_id = *to;
                                changed.insert(*other);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(dropped)
}

/// Keep building entrances consistent with a cell's surviving portal indices.
fn remap_building_doors(info: &mut LandblockInfo, cell: u16, remap: &BTreeMap<i16, i16>) {
    for building in &mut info.buildings {
        building.portals.retain_mut(|p| {
            if p.other_cell_id != cell {
                return true;
            }
            if let Some(to) = remap.get(&p.other_portal_id) {
                p.other_portal_id = *to;
                true
            } else {
                false
            }
        });
    }
}

/// How far under the landblock a closed cell (one standing for a cell taken away) is put, in
/// meters: below anything a point is ever looked for in.
pub const CLOSED_DEPTH: f32 = -2000.0;

/// The portal record's flags: bit 0 an exact match, bit 1 set when the cell is on the polygon's
/// back, bit 2 the outdoors.
fn flags(exact: bool, front: bool, outside: bool) -> u16 {
    u16::from(exact) | (u16::from(front) << 1) | (u16::from(outside) << 2)
}

/// What the packs do to a landblock's cells: the new cells, numbered as the lockfile numbers them
/// (one for each), the buildings of the base moved, its cells reshaped and taken away, and the
/// buildings dropped.
#[derive(Clone, Copy, Debug)]
pub struct Changes<'a> {
    pub adds: &'a [&'a Located<AddCell>],
    pub numbers: &'a [u16],
    pub moves: &'a [&'a Located<MoveBuilding>],
    pub replaces: &'a [&'a Located<ReplaceCell>],
    pub taken: &'a [u16],
    pub drops: &'a [crate::drops::Dropped],
}

/// A cell standing for one taken away: its number kept, closed and empty far under the landblock,
/// drawn with `piece` (surfaces, environment and part).
fn closed(lb: u16, n: u16, piece: (Vec<DataId>, DataId, u16)) -> EnvCell {
    let id = (u32::from(lb) << 16) | u32::from(n);
    EnvCell {
        id: DataId(id),
        flags: 0,
        cell_id_repeat: id,
        surfaces: piece.0,
        environment: piece.1,
        cell_struct: piece.2,
        frame: Frame::new(Vec3::new(96.0, 96.0, CLOSED_DEPTH), Quat::IDENTITY),
        portals: Vec::new(),
        visible_cells: Vec::new(),
        static_objects: Vec::new(),
        restriction_obj: None,
    }
}

/// Shut every way of `all`'s cells into cell `n`: each portal that leads there taken out, the
/// cells it joins naming the portals after it by their new places, and the buildings' doorways
/// into it gone; every cell changed in `changed`.
fn shut_ways_into(
    n: u16,
    all: &mut BTreeMap<u16, EnvCell>,
    changed: &mut BTreeSet<u16>,
    info: &mut LandblockInfo,
) {
    let around: Vec<u16> = all
        .iter()
        .filter(|(c, cell)| {
            **c != n
                && cell
                    .portals
                    .iter()
                    .any(|p| p.flags & 4 == 0 && p.other_cell_id == u32::from(n))
        })
        .map(|(c, _)| *c)
        .collect();
    for o in around {
        let Some(c) = all.get_mut(&o) else { continue };
        let mut remap = BTreeMap::new();
        let mut kept = Vec::new();
        for (index, p) in c.portals.iter().copied().enumerate() {
            if p.flags & 4 == 0 && p.other_cell_id == u32::from(n) {
                continue;
            }
            if let (Ok(from), Ok(to)) = (i16::try_from(index), i16::try_from(kept.len())) {
                remap.insert(from, to);
            }
            kept.push(p);
        }
        c.portals = kept;
        remap_building_doors(info, o, &remap);
        changed.insert(o);
        for (other, cell) in all.iter_mut() {
            for p in &mut cell.portals {
                if p.flags & 4 == 0 && p.other_cell_id == u32::from(o) {
                    if let Some(to) = remap.get(&p.other_portal_id) {
                        if p.other_portal_id != *to {
                            p.other_portal_id = *to;
                            changed.insert(*other);
                        }
                    }
                }
            }
        }
    }
    for b in &mut info.buildings {
        b.portals.retain(|p| p.other_cell_id != n);
    }
}

/// Place the packs' new cells (`changes.adds`, numbered `changes.numbers`, the lockfile's, one
/// for each), move the base's buildings, reshape and take away its cells, and add the dropped
/// buildings (after the moves: each refused where its origin is outside the landblock or in a land
/// cell a building stands in) in landblock `lb`, whose statics record is `info` (its cell count is
/// set) and whose base cells are `base`; `env` reads an environment.
///
/// A number below the highest that no cell holds (the lockfile keeps the number of a cell taken
/// away) stands as a closed cell with nothing in it, far under the landblock: the cells stay
/// numbered without a gap, which the server's search for the cell a point is in needs, and every
/// other cell keeps its number.
///
/// # Errors
/// A piece, environment or link that is not there, two portals that do not name each other or
/// do not meet, or a building that is not there.
#[allow(clippy::too_many_lines)]
pub fn place(
    lb: u16,
    info: &mut LandblockInfo,
    base: &BTreeMap<u16, EnvCell>,
    changes: &Changes<'_>,
    env: &dyn Fn(DataId) -> Option<Environment>,
) -> Result<Placed, Error> {
    let Changes {
        adds,
        numbers,
        moves,
        replaces,
        taken,
        drops,
    } = *changes;
    let mut out = Placed::default();
    let mut all: BTreeMap<u16, EnvCell> = base.clone();
    // The cells made (whose lists are made) and moved (whose lists stay: everything they see
    // moves with them).
    let mut changed: BTreeSet<u16> = BTreeSet::new();
    let mut moved: BTreeSet<u16> = BTreeSet::new();
    // Buildings first: their cells move with them.
    for m in moves {
        let Some(b) = info.buildings.get(m.item.index) else {
            let n = info.buildings.len();
            return Err(Error::source(
                &m.at,
                format!(
                    "landblock {lb:04X} has {n} building(s) of its own, so none numbered {}; a \
                     dropped building is moved by its add-building row",
                    m.item.index
                ),
            ));
        };
        let from = b.frame;
        let o = m
            .item
            .at
            .unwrap_or([from.origin.x, from.origin.y, from.origin.z]);
        let to = Frame::new(
            Vec3::new(o[0], o[1], o[2]),
            m.item.rotation.map_or(from.rotation, quat_of),
        );
        // Moved to its own place (as a source writes it, to a few decimals): nothing moves.
        let same = (to.origin.x - from.origin.x).abs() < 1e-3
            && (to.origin.y - from.origin.y).abs() < 1e-3
            && (to.origin.z - from.origin.z).abs() < 1e-3
            && (to.rotation.w - from.rotation.w).abs() < 1e-5
            && (to.rotation.x - from.rotation.x).abs() < 1e-5
            && (to.rotation.y - from.rotation.y).abs() < 1e-5
            && (to.rotation.z - from.rotation.z).abs() < 1e-5;
        if same {
            continue;
        }
        let doorways: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
        let inside = building_cells(&doorways, &all);
        for c in &inside {
            let Some(cell) = all.get_mut(c) else { continue };
            cell.frame = carried(&cell.frame, &from, &to);
            for s in &mut cell.static_objects {
                s.frame = carried(&s.frame, &from, &to);
            }
            moved.insert(*c);
        }
        info.buildings[m.item.index].frame = to;
        out.summary.push(format!(
            "{lb:04X}: building {} moved with its {} cell(s)",
            m.item.index,
            inside.len()
        ));
    }
    // Dropped buildings, their rooms numbered as the lockfile numbers them; their lists are the
    // templates', renumbered.
    for d in drops {
        crate::drops::check_sites(lb, &d.name, info, &d.buildings)
            .map_err(|why| Error::source(&d.at, why))?;
        for (n, cell) in &d.rooms {
            if all.contains_key(n) {
                return Err(Error::source(
                    &d.at,
                    format!(
                        "a room of building {} is numbered {n:04X}, which another cell of \
                         landblock {lb:04X} holds",
                        d.name
                    ),
                ));
            }
            all.insert(*n, cell.clone());
            out.cells.insert(*n, cell.clone());
        }
        out.rooms.extend(d.names.clone());
        for b in &d.buildings {
            out.dropped.push((d.name.clone(), info.buildings.len()));
            info.buildings.push(b.clone());
        }
        crate::drops::restrict(info, &d.restrictions);
        out.summary.push(crate::drops::summary(lb, d));
    }
    // New cells, numbered as the lockfile numbers them: after the base's.
    for (a, n) in adds.iter().zip(numbers) {
        if all.contains_key(n) || out.names.values().any(|m| m == n) {
            return Err(Error::source(
                &a.at,
                format!(
                    "cell {} is numbered {n:04X}, which another cell of landblock {lb:04X} holds",
                    a.item.name
                ),
            ));
        }
        out.names.insert(a.item.name.clone(), *n);
    }
    let mut pieces: BTreeMap<u32, Environment> = BTreeMap::new();
    let wanted = adds
        .iter()
        .map(|a| (&a.at, a.item.environment))
        .chain(replaces.iter().map(|r| (&r.at, r.item.environment)));
    for (at, e) in wanted {
        if let std::collections::btree_map::Entry::Vacant(v) = pieces.entry(e.raw()) {
            let Some(found) = env(e) else {
                return Err(Error::source(
                    at,
                    format!("the files have no environment {:#010X}", e.raw()),
                ));
            };
            v.insert(found);
        }
    }
    let by_number: BTreeMap<u16, &Located<ReplaceCell>> =
        replaces.iter().map(|r| (r.item.cell, *r)).collect();
    let add_by_name: BTreeMap<&str, &Located<AddCell>> =
        adds.iter().map(|a| (a.item.name.as_str(), *a)).collect();
    for r in replaces {
        let dropped = reshape(
            lb,
            r,
            &pieces,
            &add_by_name,
            &out.names,
            env,
            &mut all,
            &mut changed,
            info,
        )?;
        // A way into a cell taken away is shut on purpose.
        let into_taken = base.get(&r.item.cell).map_or(0, |c| {
            c.portals
                .iter()
                .filter(|p| {
                    p.flags & 4 == 0
                        && u16::try_from(p.other_cell_id).is_ok_and(|o| taken.contains(&o))
                })
                .count()
        });
        let dropped = dropped.saturating_sub(into_taken);
        if dropped > 0 {
            out.warnings.push(format!(
                "{}: cell {lb:04X}{:04X} replacement drops {dropped} doorway(s)",
                r.at, r.item.cell
            ));
        }
    }
    // The cells taken away: closed, every way into them shut, and every cell that saw them
    // seeing again.
    for n in taken {
        let Some(old) = all.get(n).cloned() else {
            return Err(Error::Refused(format!(
                "landblock {lb:04X} has no cell {n:04X} to take away"
            )));
        };
        shut_ways_into(*n, &mut all, &mut changed, info);
        all.insert(
            *n,
            closed(lb, *n, (old.surfaces, old.environment, old.cell_struct)),
        );
        out.cells.insert(*n, all[n].clone());
    }
    if !taken.is_empty() {
        let seeing: Vec<u16> = all
            .iter()
            .filter(|(c, cell)| {
                !taken.contains(c) && cell.visible_cells.iter().any(|v| taken.contains(v))
            })
            .map(|(c, _)| *c)
            .collect();
        changed.extend(seeing);
        out.summary
            .push(format!("{lb:04X}: {} cell(s) taken away", taken.len()));
    }
    if !replaces.is_empty() {
        out.summary
            .push(format!("{lb:04X}: {} cell(s) reshaped", replaces.len()));
    }
    let frame_of =
        |a: &AddCell| Frame::new(Vec3::new(a.at[0], a.at[1], a.at[2]), quat_of(a.rotation));
    let by_name: BTreeMap<&str, &Located<AddCell>> =
        adds.iter().map(|a| (a.item.name.as_str(), *a)).collect();
    for a in adds {
        let item = &a.item;
        let e = &pieces[&item.environment.raw()];
        let Some(cs) = e.cells.get(usize::from(item.piece)) else {
            return Err(Error::source(
                &a.at,
                format!(
                    "environment {:#010X} has {} piece(s); there is no {}",
                    item.environment.raw(),
                    e.cells.len(),
                    item.piece
                ),
            ));
        };
        if item.portals.len() != cs.portals.len() {
            return Err(Error::source(
                &a.at,
                format!(
                    "the piece has {} portal(s); {} are given",
                    cs.portals.len(),
                    item.portals.len()
                ),
            ));
        }
        let frame = frame_of(item);
        let id = out.names[&item.name];
        let mut portals = Vec::new();
        for (slot, link) in item.portals.iter().enumerate() {
            let polygon = cs.portals[slot];
            let front = portal_side(e, item.piece, polygon).unwrap_or(true);
            let mine = corners(e, item.piece, polygon, &frame);
            let me = Link::Cell {
                cell: item.name.clone(),
                portal: slot,
            };
            // One of the landblock's own cells this portal joins, reshaped, as a portal record.
            let to_base = |cell: u16, portal: usize| -> Result<CellPortal, Error> {
                let Some(r) = by_number.get(&cell) else {
                    return Err(Error::source(
                        &a.at,
                        format!(
                            "cell {cell:04X} of landblock {lb:04X} is not reshaped: a new cell joins one of the landblock's own only at a reshaped cell's portal"
                        ),
                    ));
                };
                if !names(r.item.portals.get(portal), &me) {
                    return Err(Error::source(
                        &a.at,
                        format!(
                            "portal {slot} of {} leads to portal {portal} of cell {cell:04X}, which does not lead back",
                            item.name
                        ),
                    ));
                }
                let oe = &pieces[&r.item.environment.raw()];
                let opoly = oe.cells[usize::from(r.item.piece)].portals[portal];
                let rframe = Frame::new(
                    Vec3::new(r.item.at[0], r.item.at[1], r.item.at[2]),
                    quat_of(r.item.rotation),
                );
                let theirs = corners(oe, r.item.piece, opoly, &rframe);
                let exact = matches!((&mine, &theirs), (Some(m), Some(t)) if coincide(m, t));
                // Its record of the way back, as its reshaping wrote it.
                let back = all.get(&cell).and_then(|c| {
                    c.portals
                        .iter()
                        .position(|p| p.polygon_id == opoly && p.other_cell_id == u32::from(id))
                });
                Ok(CellPortal {
                    flags: flags(exact, front, false),
                    polygon_id: polygon,
                    other_cell_id: u32::from(cell),
                    other_portal_id: back.and_then(|b| i16::try_from(b).ok()).unwrap_or(-1),
                })
            };
            // Another new cell this portal joins, as a portal record.
            let to_new = |cell: &str, portal: usize| -> Result<CellPortal, Error> {
                let Some(other) = by_name.get(cell) else {
                    return Err(Error::source(
                        &a.at,
                        format!("no cell {cell} is placed in landblock {lb:04X}"),
                    ));
                };
                if !names(other.item.portals.get(portal), &me) {
                    return Err(Error::source(
                        &a.at,
                        format!(
                            "portal {slot} of {} leads to portal {portal} of {cell}, which does not lead back",
                            item.name
                        ),
                    ));
                }
                let oe = &pieces[&other.item.environment.raw()];
                let opoly = oe.cells[usize::from(other.item.piece)].portals[portal];
                let theirs = corners(oe, other.item.piece, opoly, &frame_of(&other.item));
                let exact = matches!((&mine, &theirs), (Some(m), Some(t)) if coincide(m, t));
                Ok(CellPortal {
                    flags: flags(exact, front, false),
                    polygon_id: polygon,
                    other_cell_id: u32::from(out.names[cell]),
                    other_portal_id: written_to(&other.item.portals, portal, &me).unwrap_or(-1),
                })
            };
            match link {
                Link::Shut => {}
                Link::Outside => portals.push(CellPortal {
                    flags: flags(true, front, true),
                    polygon_id: polygon,
                    other_cell_id: 0xFFFF_FFFF,
                    other_portal_id: -1,
                }),
                Link::Keep => {
                    return Err(Error::source(
                        &a.at,
                        "a new cell's portal keeps nothing: keep is a reshaped cell's",
                    ));
                }
                Link::Base { cell, portal } => portals.push(to_base(*cell, *portal)?),
                Link::Cell { cell, portal } => portals.push(to_new(cell, *portal)?),
                Link::Many(list) => {
                    for l in list {
                        match l {
                            Link::Base { cell, portal } => portals.push(to_base(*cell, *portal)?),
                            Link::Cell { cell, portal } => portals.push(to_new(cell, *portal)?),
                            _ => {
                                return Err(Error::source(
                                    &a.at,
                                    "a portal that joins several cells names each by its cell",
                                ));
                            }
                        }
                    }
                }
            }
        }
        let static_objects: Vec<ObjectEntry> = item
            .statics
            .iter()
            .map(|s| ObjectEntry {
                id: s.model,
                frame: Frame::new(Vec3::new(s.at[0], s.at[1], s.at[2]), quat_of(s.rotation)),
            })
            .collect();
        let cell_id = (u32::from(lb) << 16) | u32::from(id);
        all.insert(
            id,
            EnvCell {
                id: DataId(cell_id),
                flags: if static_objects.is_empty() { 0 } else { 2 },
                cell_id_repeat: cell_id,
                surfaces: item.surfaces.clone(),
                environment: item.environment,
                cell_struct: item.piece,
                frame,
                portals,
                visible_cells: Vec::new(),
                static_objects,
                restriction_obj: None,
            },
        );
        changed.insert(id);
    }
    if !adds.is_empty() {
        out.summary.push(format!(
            "{lb:04X}: {} cell(s) placed, {:04X} to {:04X}",
            adds.len(),
            out.names.values().min().copied().unwrap_or(FIRST_CELL),
            out.names.values().max().copied().unwrap_or(FIRST_CELL)
        ));
    }
    // Their stab lists and seen-outside, over every cell of the landblock.
    let mut envs: BTreeMap<u32, Option<Environment>> =
        pieces.into_iter().map(|(k, v)| (k, Some(v))).collect();
    let mut shapes: BTreeMap<u16, stab::CellShape> = BTreeMap::new();
    for (c, cell) in &all {
        let e = envs
            .entry(cell.environment.raw())
            .or_insert_with(|| env(cell.environment));
        if let Some(s) = e.as_ref().and_then(|e| stab::shape(cell, e)) {
            shapes.insert(*c, s);
        }
    }
    let doorways: Vec<Vec<u16>> = info
        .buildings
        .iter()
        .map(|b| b.portals.iter().map(|p| p.other_cell_id).collect())
        .collect();
    let buildings = stab::buildings(&shapes, &doorways);
    let lists = stab::lists(&shapes, &buildings, &changed);
    for c in &changed {
        if taken.contains(c) {
            continue;
        }
        let Some(cell) = all.get_mut(c) else { continue };
        let (list, seen) = lists.get(c).cloned().unwrap_or_default();
        cell.flags = (cell.flags & !1) | u32::from(seen);
        cell.visible_cells = list;
        out.cells.insert(*c, cell.clone());
    }
    for c in &moved {
        if let Some(cell) = all.get(c) {
            out.cells.insert(*c, cell.clone());
        }
    }
    // The numbers of cells taken away, closed.
    let top = out.names.values().chain(out.rooms.values()).max().copied();
    // A closed cell is drawn with the lowest new cell's piece.
    let lowest = adds
        .iter()
        .map(|a| {
            (
                out.names[&a.item.name],
                (a.item.surfaces.clone(), a.item.environment, a.item.piece),
            )
        })
        .chain(drops.iter().flat_map(|d| {
            d.rooms
                .iter()
                .map(|(n, c)| (*n, (c.surfaces.clone(), c.environment, c.cell_struct)))
        }))
        .min_by_key(|(n, _)| *n)
        .map(|(_, piece)| piece);
    if let (Some(top), Some((surfaces, environment, piece))) = (top, lowest) {
        let mut kept = 0;
        for n in FIRST_CELL..top {
            if all.contains_key(&n) {
                continue;
            }
            let cell = closed(lb, n, (surfaces.clone(), environment, piece));
            all.insert(n, cell.clone());
            out.cells.insert(n, cell);
            kept += 1;
        }
        if kept > 0 {
            out.summary.push(format!(
                "{lb:04X}: {kept} number(s) of cells taken away kept, closed"
            ));
        }
    }
    if !adds.is_empty() || !drops.is_empty() {
        info.num_cells = u32::try_from(all.len()).unwrap_or(u32::MAX);
    }
    Ok(out)
}
