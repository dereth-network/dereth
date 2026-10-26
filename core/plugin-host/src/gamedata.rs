//! The game's own data, read only, for plugins that draw maps: a landblock's terrain and what
//! stands on it, and a dungeon cell's place, its neighbours and the outline of its floor.
//!
//! The host reads nothing itself: a product that has the game's data files hands it a
//! [`PluginGameData`], which answers a record's bytes by id, and the host decodes them with the
//! game-data decoders. A host without the files offers no `game.data`.

use dereth_assets::world::{CellLandblock, EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment, Region};
use dereth_plugin_api::Value;
use dereth_primitives::DataId;

/// The game's data, as a product reads it: a record's bytes by id, from the cell file (landblocks
/// and cells) or the portal file (everything else).
pub trait PluginGameData: std::fmt::Debug {
    fn read_cell(&self, id: u32) -> Option<Vec<u8>>;
    fn read_portal(&self, id: u32) -> Option<Vec<u8>>;
}

/// No game data.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoGameData;

impl PluginGameData for NoGameData {
    fn read_cell(&self, _id: u32) -> Option<Vec<u8>> {
        None
    }
    fn read_portal(&self, _id: u32) -> Option<Vec<u8>> {
        None
    }
}

/// The region's height table: a landblock vertex's height index to metres.
fn height_table(data: &dyn PluginGameData) -> Option<Vec<f32>> {
    let bytes = data.read_portal(0x1300_0000)?;
    Region::decode_payload(DataId(0x1300_0000), &bytes)
        .ok()
        .map(|r| r.land_defs.land_height_table)
}

fn round(v: f32) -> f64 {
    (f64::from(v) * 100.0).round() / 100.0
}

/// Landblock `lb` (`0xXXYY`, or any cell id within it): `{ id, heights, terrain, roads, cells,
/// buildings, objects }`, the 81 vertices (9 by 9, `x * 9 + y`, 24 metres apart) as heights in
/// metres, terrain types and road flags; `cells` its dungeon and building cells' ids. Nil when the
/// data has no such landblock.
#[must_use]
pub fn landblock(data: &dyn PluginGameData, lb: u32) -> Value {
    let base = if lb > 0xFFFF { lb >> 16 } else { lb } & 0xFFFF;
    let id = (base << 16) | 0xFFFF;
    let Some(bytes) = data.read_cell(id) else {
        return Value::Nil;
    };
    let Ok(land) = CellLandblock::decode_payload(DataId(id), &bytes) else {
        return Value::Nil;
    };
    let heights = height_table(data);
    let info = (land.lbi_exists != 0)
        .then(|| data.read_cell(id - 1))
        .flatten()
        .and_then(|b| LandblockInfo::decode_payload(DataId(id - 1), &b).ok());
    let cells: Vec<Value> = info.as_ref().map_or_else(Vec::new, |i| {
        (0..i.num_cells)
            .map(|n| Value::from((base << 16) | (0x0100 + n)))
            .collect()
    });
    Value::map()
        .with("id", base)
        .with(
            "heights",
            Value::List(
                land.height
                    .iter()
                    .map(|&h| match &heights {
                        Some(t) => t
                            .get(usize::from(h))
                            .map_or(Value::Nil, |m| Value::from(round(*m))),
                        None => Value::from(u32::from(h)),
                    })
                    .collect(),
            ),
        )
        .with(
            "terrain",
            Value::List(
                (0..81)
                    .map(|i| Value::from(u32::from(land.terrain_type(i / 9, i % 9))))
                    .collect(),
            ),
        )
        .with(
            "roads",
            Value::List(
                (0..81)
                    .map(|i| Value::from(land.road(i / 9, i % 9) != 0))
                    .collect(),
            ),
        )
        .with("cells", Value::List(cells))
        .with(
            "buildings",
            info.as_ref()
                .map_or(0, |i| u32::try_from(i.buildings.len()).unwrap_or(0)),
        )
        .with(
            "objects",
            info.as_ref()
                .map_or(0, |i| u32::try_from(i.objects.len()).unwrap_or(0)),
        )
}

/// A vector rotated by a unit quaternion.
fn rotate(q: dereth_primitives::Quat, v: (f32, f32, f32)) -> (f32, f32, f32) {
    let (w, x, y, z) = (q.w, q.x, q.y, q.z);
    let (vx, vy, vz) = v;
    // t = 2 * cross(q.xyz, v); v' = v + w * t + cross(q.xyz, t)
    let (tx, ty, tz) = (
        2.0 * (y * vz - z * vy),
        2.0 * (z * vx - x * vz),
        2.0 * (x * vy - y * vx),
    );
    (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )
}

/// Dungeon or building cell `id` (`0xXXYY01nn`): `{ id, environment, position = { x, y, z },
/// portals = { { to, outside } }, visible, floor, bounds }`: its place in its landblock, the cells
/// its portals lead to, the cells seen from it, its floor as polygons of `{ x, y }` (metres, in
/// the landblock's frame) and its box `{ min = {x, y, z}, max = {x, y, z} }`. Nil when the data
/// has no such cell.
#[must_use]
pub fn cell(data: &dyn PluginGameData, id: u32) -> Value {
    let Some(bytes) = data.read_cell(id) else {
        return Value::Nil;
    };
    let Ok(c) = EnvCell::decode_payload(DataId(id), &bytes) else {
        return Value::Nil;
    };
    let base = id & 0xFFFF_0000;
    let o = c.frame.origin;
    let mut floor = Vec::new();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    if let Some(env) = data
        .read_portal(c.environment.0)
        .and_then(|b| Environment::decode_payload(c.environment, &b).ok())
    {
        if let Some(s) = env.cells.get(usize::from(c.cell_struct)) {
            let place = |i: u16| {
                s.vertex_array
                    .vertices
                    .iter()
                    .find(|v| v.id == i16::try_from(i).unwrap_or(-1))
                    .map(|v| {
                        let (x, y, z) =
                            rotate(c.frame.rotation, (v.position.x, v.position.y, v.position.z));
                        (x + o.x, y + o.y, z + o.z)
                    })
            };
            for v in &s.vertex_array.vertices {
                let (x, y, z) =
                    rotate(c.frame.rotation, (v.position.x, v.position.y, v.position.z));
                for (k, val) in [x + o.x, y + o.y, z + o.z].into_iter().enumerate() {
                    lo[k] = lo[k].min(val);
                    hi[k] = hi[k].max(val);
                }
            }
            for p in &s.polygons {
                let pts: Vec<(f32, f32, f32)> =
                    p.vertex_ids.iter().filter_map(|&i| place(i)).collect();
                if pts.len() < 3 {
                    continue;
                }
                // A floor: flat, and at the bottom of the cell.
                let zs = pts.iter().map(|p| p.2);
                let (zmin, zmax) = zs.fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)));
                if zmax - zmin < 0.05 && zmin - lo[2] < 0.5 {
                    floor.push(Value::List(
                        pts.iter()
                            .map(|p| Value::map().with("x", round(p.0)).with("y", round(p.1)))
                            .collect(),
                    ));
                }
            }
        }
    }
    let point = |v: [f32; 3]| {
        Value::map()
            .with("x", round(v[0]))
            .with("y", round(v[1]))
            .with("z", round(v[2]))
    };
    Value::map()
        .with("id", id)
        .with("environment", c.environment.0)
        .with(
            "position",
            Value::map()
                .with("x", round(o.x))
                .with("y", round(o.y))
                .with("z", round(o.z)),
        )
        .with(
            "portals",
            Value::List(
                c.portals
                    .iter()
                    .map(|p| {
                        let outside = p.flags & 4 != 0;
                        Value::map()
                            .with(
                                "to",
                                (!outside).then_some(base | (p.other_cell_id & 0xFFFF)),
                            )
                            .with("outside", outside)
                    })
                    .collect(),
            ),
        )
        .with(
            "visible",
            Value::List(
                c.visible_cells
                    .iter()
                    .map(|v| Value::from(base | u32::from(*v)))
                    .collect(),
            ),
        )
        .with("floor", Value::List(floor))
        .with(
            "bounds",
            if lo[0] <= hi[0] {
                Value::map().with("min", point(lo)).with("max", point(hi))
            } else {
                Value::Nil
            },
        )
}
