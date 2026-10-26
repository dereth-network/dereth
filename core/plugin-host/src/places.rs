//! Places as plugins name them, and the arithmetic between them.
//!
//! A plugin names a place one of two ways:
//!
//! - by **map coordinates**, `{ ns = 42.2, ew = 33.8 }` (south and west negative), the numbers the
//!   radar shows and players trade; these name a place outdoors;
//! - by **cell and offset**, `{ cell = 0x01D90108, x = 10, y = 20, z = 0 }`, which names any place,
//!   indoors or out.
//!
//! Positions the host reports carry both (map coordinates only outdoors), and a heading in degrees
//! clockwise from north.
//!
//! One map unit is 240 metres (ten 24-metre cells), and the map's origin sits 101.9 units west and
//! south of the corner of the world, so a cell whose corner is at landscape cell `(lx, ly)` shows as
//! `(ly − 1024) × 0.1 + 0.5` north and `(lx − 1024) × 0.1 + 0.5` east, as the radar shows it.

use dereth_plugin_api::{CallError, Value};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

/// Metres along one side of a landblock.
pub const BLOCK: f64 = 192.0;
/// Metres per map unit.
pub const UNIT: f64 = 240.0;
/// Map units from the world's corner to the map's origin.
pub const ORIGIN: f64 = 101.9;

/// How far above the ground each end of a line of sight runs, metres: about a body's middle.
pub const SIGHT_HEIGHT: f64 = 1.0;

/// `p` raised `h` metres.
#[must_use]
pub fn raised(p: &Position, h: f64) -> Position {
    let mut out = *p;
    #[allow(clippy::cast_possible_truncation)]
    {
        out.frame.origin.z += h as f32;
    }
    out
}

/// Whether a cell is outdoors (a landscape cell rather than a dungeon or building cell).
#[must_use]
pub fn outdoors(cell: CellId) -> bool {
    (cell.0 & 0xFFFF) < 0x100
}

/// The position's place in metres from the world's corner: its landblock's corner plus its offset.
/// Inside one landblock this is exact for every cell; across landblocks it is exact outdoors.
#[must_use]
pub fn global(p: &Position) -> (f64, f64, f64) {
    let bx = f64::from(p.cell.0 >> 24);
    let by = f64::from((p.cell.0 >> 16) & 0xFF);
    (
        bx * BLOCK + f64::from(p.frame.origin.x),
        by * BLOCK + f64::from(p.frame.origin.y),
        f64::from(p.frame.origin.z),
    )
}

/// The map coordinates of an outdoor position: `(ns, ew)`.
#[must_use]
pub fn coords(p: &Position) -> Option<(f64, f64)> {
    outdoors(p.cell).then(|| {
        let (gx, gy, _) = global(p);
        (gy / UNIT - ORIGIN, gx / UNIT - ORIGIN)
    })
}

/// The outdoor position at map coordinates `(ns, ew)`, at height `z`.
#[must_use]
pub fn from_coords(ns: f64, ew: f64, z: f64) -> Option<Position> {
    let gx = (ew + ORIGIN) * UNIT;
    let gy = (ns + ORIGIN) * UNIT;
    if !(0.0..BLOCK * 255.0).contains(&gx) || !(0.0..BLOCK * 255.0).contains(&gy) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (bx, by) = ((gx / BLOCK) as u32, (gy / BLOCK) as u32);
    let (ox, oy) = (gx - f64::from(bx) * BLOCK, gy - f64::from(by) * BLOCK);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (cx, cy) = ((ox / 24.0) as u32, (oy / 24.0) as u32);
    let cell = (bx << 24) | (by << 16) | (cx * 8 + cy + 1);
    #[allow(clippy::cast_possible_truncation)]
    Some(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(ox as f32, oy as f32, z as f32), Quat::IDENTITY),
    ))
}

/// The distance between two positions in metres (3-D).
#[must_use]
pub fn distance(a: &Position, b: &Position) -> f64 {
    let (ax, ay, az) = global(a);
    let (bx, by, bz) = global(b);
    ((bx - ax).powi(2) + (by - ay).powi(2) + (bz - az).powi(2)).sqrt()
}

/// The distance on the ground (ignoring height), in metres.
#[must_use]
pub fn ground_distance(a: &Position, b: &Position) -> f64 {
    let (ax, ay, _) = global(a);
    let (bx, by, _) = global(b);
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

/// The heading from `a` toward `b`, in degrees clockwise from north.
#[must_use]
pub fn bearing(a: &Position, b: &Position) -> f64 {
    let (ax, ay, _) = global(a);
    let (bx, by, _) = global(b);
    dereth_primitives::num::math::atan2(bx - ax, by - ay)
        .to_degrees()
        .rem_euclid(360.0)
}

/// A position's heading, in degrees clockwise from north.
#[must_use]
pub fn heading(p: &Position) -> f64 {
    f64::from(dereth_primitives::frame::get_heading(&p.frame)).rem_euclid(360.0)
}

/// The signed difference `to − from` between two headings, in −180..180.
#[must_use]
pub fn turn(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// A position as plugins read it: `{ cell, x, y, z, heading, ns, ew }`, the map coordinates only
/// outdoors.
#[must_use]
pub fn to_value(p: &Position) -> Value {
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut v = Value::map()
        .with("cell", p.cell.0)
        .with("x", round(f64::from(p.frame.origin.x)))
        .with("y", round(f64::from(p.frame.origin.y)))
        .with("z", round(f64::from(p.frame.origin.z)))
        .with("heading", round(heading(p)));
    if let Some((ns, ew)) = coords(p) {
        v = v.with("ns", round(ns)).with("ew", round(ew));
    }
    v
}

/// A place a plugin named (see the module documentation).
///
/// # Errors
/// Neither form, or numbers out of the world.
pub fn from_value(v: &Value) -> Result<Position, CallError> {
    let num = |k: &str| v.get(k).as_f64();
    if let (Some(ns), Some(ew)) = (num("ns"), num("ew")) {
        return from_coords(ns, ew, num("z").unwrap_or(0.0))
            .ok_or_else(|| CallError::BadArgs(format!("{ns}, {ew} is not on the map")));
    }
    let cell = v
        .get("cell")
        .as_i64()
        .and_then(|c| u32::try_from(c).ok())
        .ok_or_else(|| {
            CallError::BadArgs(
                "a place is { ns = …, ew = … } or { cell = …, x = …, y = …, z = … }".into(),
            )
        })?;
    #[allow(clippy::cast_possible_truncation)]
    let at = |k: &str| num(k).unwrap_or(0.0) as f32;
    Ok(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(at("x"), at("y"), at("z")), Quat::IDENTITY),
    ))
}

#[cfg(test)]
mod tests {
    use super::{bearing, coords, distance, from_coords, turn};

    #[test]
    fn map_coordinates_name_the_cell_the_radar_shows_them_for_and_read_back() {
        // Holtburg's landblock 0xA9B4: the radar shows 42.2N 33.8E in its first cell.
        let p = from_coords(42.2, 33.8, 0.0).expect("on the map");
        assert_eq!(p.cell.0 >> 16, 0xA9B4);
        let (ns, ew) = coords(&p).expect("outdoors");
        assert!((ns - 42.2).abs() < 1e-3 && (ew - 33.8).abs() < 1e-3);
        assert!(from_coords(200.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn distance_and_bearing_cross_landblocks() {
        let a = from_coords(0.0, 0.0, 0.0).expect("a");
        let north = from_coords(1.0, 0.0, 0.0).expect("north");
        let east = from_coords(0.0, 1.0, 0.0).expect("east");
        assert!((distance(&a, &north) - 240.0).abs() < 0.01);
        assert!(bearing(&a, &north).abs() < 0.01);
        assert!((bearing(&a, &east) - 90.0).abs() < 0.01);
        assert!((turn(350.0, 10.0) - 20.0).abs() < 1e-9);
        assert!((turn(10.0, 350.0) + 20.0).abs() < 1e-9);
    }
}
