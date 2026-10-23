//! Building footprints: whether a static stands inside a building.
//!
//! A building's footprint is the floor plan of its interior cells: each cell is the space its
//! cell tree bounds (the planes along the tree's positive chain), and its footprint is that space
//! seen from above, between its lowest point (less [`FOOTPRINT_DROP`], for the ground under a
//! raised floor) and its highest. A static's origin inside one of a building's cells, deeper than
//! [`FOOTPRINT_MARGIN`] behind every wall, stands inside the building: drawn in its walls, and in
//! the way of whoever walks in. A lamp or a sign set against a wall from outside, or a chimney on
//! the roof, stays clear.
//!
//! The cells of a building are the ones its portals lead to and every cell reachable from them
//! through the cells' own portals, within the landblock's cells.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::Environment;
use dereth_primitives::frame::globaltolocal;
use dereth_primitives::shape::Plane;
use dereth_primitives::{DataId, Frame, Vec3};

use crate::dats::DatSet;

/// How far behind every wall of a cell a point must be to stand inside it, in meters.
pub const FOOTPRINT_MARGIN: f32 = 0.25;

/// How far below a cell's lowest point its footprint still reaches, in meters: a static stood on
/// the ground under a floor raised above it is inside the building too.
pub const FOOTPRINT_DROP: f32 = 1.0;

/// How many cells one building may hold before the walk stops: a building is a few rooms, and a
/// walk that runs on has found a dungeon under it.
const MAX_CELLS: usize = 64;

/// A plane whose normal leans less than this from level is a wall.
const WALL: f32 = 0.5;

/// One cell of a building, as a footprint.
#[derive(Clone, Debug)]
struct CellShape {
    frame: Frame,
    walls: Vec<Plane>,
    low: f32,
    high: f32,
}

impl CellShape {
    fn contains(&self, p: Vec3) -> bool {
        if self.walls.is_empty() {
            return false;
        }
        let q = globaltolocal(&self.frame, p);
        q.z >= self.low - FOOTPRINT_DROP
            && q.z <= self.high
            && self
                .walls
                .iter()
                .all(|w| w.normal.dot(q) + w.d >= FOOTPRINT_MARGIN)
    }
}

/// The environments read so far, by id, so a world's checks decode each once.
#[derive(Debug, Default)]
pub struct Environments {
    read: BTreeMap<u32, Option<Environment>>,
}

impl Environments {
    pub(crate) fn get(&mut self, set: &DatSet, id: DataId) -> Option<&Environment> {
        self.read
            .entry(id.raw())
            .or_insert_with(|| {
                let bytes = set.read_portal(id)?;
                set.decode::<Environment>(id, &bytes).ok()
            })
            .as_ref()
    }
}

/// The footprints of one landblock's buildings, in the landblock's meters.
#[derive(Clone, Debug, Default)]
pub struct Footprints {
    /// Each building's cells, in the statics record's building order.
    buildings: Vec<Vec<CellShape>>,
}

/// The walls along a cell tree's positive chain.
fn walls(tree: &dereth_assets::BspTree) -> Vec<Plane> {
    let mut out = Vec::new();
    let mut at = (!tree.nodes.is_empty()).then_some(0u32);
    let mut steps = 0;
    while let Some(i) = at {
        let Some(node) = tree.nodes.get(i as usize) else {
            break;
        };
        if let Some(p) = node.plane {
            if p.normal.z.abs() < WALL {
                out.push(p);
            }
        }
        at = node.pos_child;
        steps += 1;
        if steps > tree.nodes.len() {
            break;
        }
    }
    out
}

impl Footprints {
    /// The footprints of the buildings of `info`, the statics record of landblock `landblock`,
    /// from the cells `set` holds.
    #[must_use]
    pub fn of(
        set: &DatSet,
        landblock: u16,
        info: &LandblockInfo,
        environments: &mut Environments,
    ) -> Self {
        let base = u32::from(landblock) << 16;
        let last = 0x0100 + info.num_cells;
        // Each cell read, with the cells its portals lead to.
        let mut cells: BTreeMap<u16, (Option<CellShape>, Vec<u16>)> = BTreeMap::new();
        let mut read = |index: u16| -> (Option<CellShape>, Vec<u16>) {
            let id = DataId(base | u32::from(index));
            let Some(cell) = set
                .cell
                .read(id)
                .ok()
                .and_then(|b| set.decode::<EnvCell>(id, &b).ok())
            else {
                return (None, Vec::new());
            };
            let next = cell
                .portals
                .iter()
                .filter_map(|p| u16::try_from(p.other_cell_id).ok())
                .collect();
            (cell_shape(set, &cell, environments), next)
        };
        let mut buildings = Vec::with_capacity(info.buildings.len());
        for b in &info.buildings {
            let mut seen: BTreeSet<u16> = BTreeSet::new();
            let mut todo: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
            let mut shapes = Vec::new();
            while let Some(index) = todo.pop() {
                if index < 0x0100 || u32::from(index) >= last || !seen.insert(index) {
                    continue;
                }
                if seen.len() > MAX_CELLS {
                    break;
                }
                let (s, next) = cells.entry(index).or_insert_with(|| read(index));
                todo.extend(next.iter().copied());
                if let Some(s) = s {
                    shapes.push(s.clone());
                }
            }
            buildings.push(shapes);
        }
        Self { buildings }
    }
}

/// One interior cell as a footprint: its placement, its walls and its height range.
fn cell_shape(set: &DatSet, cell: &EnvCell, environments: &mut Environments) -> Option<CellShape> {
    let env = environments.get(set, cell.environment)?;
    let cs = env
        .cells
        .iter()
        .find(|c| c.cellstruct_id == u32::from(cell.cell_struct))
        .or_else(|| env.cells.get(usize::from(cell.cell_struct)))?;
    let zs = cs.vertex_array.vertices.iter().map(|v| v.position.z);
    let low = zs.clone().fold(f32::INFINITY, f32::min);
    let high = zs.fold(f32::NEG_INFINITY, f32::max);
    Some(CellShape {
        frame: cell.frame,
        walls: walls(&cs.cell_bsp),
        low,
        high,
    })
}

impl Footprints {
    /// The building `p` (landblock meters) stands inside, by its place in the statics record.
    #[must_use]
    pub fn building_at(&self, p: Vec3) -> Option<usize> {
        self.buildings
            .iter()
            .position(|cells| cells.iter().any(|c| c.contains(p)))
    }

    /// How many buildings have a footprint (any cell read).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buildings.iter().filter(|b| !b.is_empty()).count()
    }

    /// Whether no building has a footprint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Every static of `info` that stands inside one of its buildings: `(static, building)`, by their
/// places in the record.
#[must_use]
pub fn statics_inside(info: &LandblockInfo, footprints: &Footprints) -> Vec<(usize, usize)> {
    info.objects
        .iter()
        .enumerate()
        .filter_map(|(i, o)| footprints.building_at(o.frame.origin).map(|b| (i, b)))
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a building footprint's geometry)
    use super::*;
    use dereth_primitives::Quat;

    /// A room 10 m square from (100, 100) to (110, 110) and 4 m high, its floor at 50, its walls
    /// facing in.
    fn room() -> CellShape {
        let wall = |x: f32, y: f32, d: f32| Plane::new(Vec3::new(x, y, 0.0), d);
        CellShape {
            frame: Frame::new(Vec3::new(100.0, 100.0, 50.0), Quat::IDENTITY),
            walls: vec![
                wall(1.0, 0.0, 0.0),
                wall(-1.0, 0.0, 10.0),
                wall(0.0, 1.0, 0.0),
                wall(0.0, -1.0, 10.0),
            ],
            low: 0.0,
            high: 4.0,
        }
    }

    #[test]
    fn a_point_deeper_than_the_margin_behind_every_wall_is_inside() {
        let r = room();
        assert!(r.contains(Vec3::new(105.0, 105.0, 50.0)));
        assert!(r.contains(Vec3::new(100.0 + FOOTPRINT_MARGIN, 105.0, 50.0)));
        // Set against a wall from inside or out, or past it, it is not.
        assert!(!r.contains(Vec3::new(100.1, 105.0, 50.0)));
        assert!(!r.contains(Vec3::new(99.0, 105.0, 50.0)));
    }

    #[test]
    fn the_footprint_reaches_a_meter_under_the_floor_and_not_over_the_roof() {
        let r = room();
        assert!(r.contains(Vec3::new(105.0, 105.0, 50.0 - FOOTPRINT_DROP)));
        assert!(!r.contains(Vec3::new(105.0, 105.0, 50.0 - FOOTPRINT_DROP - 0.1)));
        assert!(r.contains(Vec3::new(105.0, 105.0, 54.0)));
        assert!(!r.contains(Vec3::new(105.0, 105.0, 54.1)));
    }
}
