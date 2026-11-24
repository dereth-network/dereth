//! The starting areas as the Starting Areas tab shows and changes them: each area of the
//! character-creation table as the world has it (its places, the heritages that offer it), and the
//! layer's own `[[starter-area]]` row of it ([`crate::tables`]), written in its pack's
//! `records/chargen.toml`.
//!
//! A place is set from where the camera is, and refused where the land would not hold a
//! character: indoors, a cell with no floor under the camera; outdoors, a spot off the land or
//! under its ground. Outdoors a place stands on the ground (`"ground"`), so it stays there when
//! the ground is edited.

use dereth_assets::tables::CharGen;
use dereth_primitives::{Position, Quat};
use toml::Value as T;

/// The records file the tab writes.
pub const FILE: &str = "chargen.toml";

/// The rows' kind.
pub const KIND: &str = "starter-area";

/// How far under the ground the camera may be and still stand on it, in meters.
const UNDER: f32 = 0.05;

/// One area as the world has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownArea {
    pub name: String,
    pub places: Vec<Position>,
    /// The heritages that offer it, by name, in the table's order.
    pub heritages: Vec<String>,
}

/// The areas of character-creation table `cg`, in its order.
#[must_use]
pub fn areas(cg: &CharGen) -> Vec<ShownArea> {
    cg.starter_areas
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            ShownArea {
                name: a.name.clone(),
                places: a.locations.clone(),
                heritages: cg
                    .heritage_order
                    .iter()
                    .filter_map(|k| cg.heritage_groups.get(k))
                    .filter(|h| {
                        h.primary_start_areas.contains(&i) || h.secondary_start_areas.contains(&i)
                    })
                    .map(|h| h.name.clone())
                    .collect(),
            }
        })
        .collect()
}

/// Every heritage of `cg`, by name, in its order.
#[must_use]
pub fn heritages(cg: &CharGen) -> Vec<String> {
    cg.heritage_order
        .iter()
        .filter_map(|k| cg.heritage_groups.get(k))
        .map(|h| h.name.clone())
        .collect()
}

/// A rotation's heading: degrees clockwise from north, from 0 to 360.
#[must_use]
pub fn heading(q: &Quat) -> f32 {
    let deg = -2.0 * dereth_primitives::num::math::atan2f(q.z, q.w).to_degrees();
    let h = deg.rem_euclid(360.0);
    // Adding zero writes no turn as 0, not -0.
    if (h - 360.0).abs() < 1e-3 {
        0.0
    } else {
        h + 0.0
    }
}

/// A place as a row writes it: `{ cell, at = [x, y, z], heading }`, `z` a height or `"ground"`.
#[must_use]
pub fn place_row(cell: u32, x: f32, y: f32, z: Option<f32>, heading: f32) -> T {
    let round = |v: f32| f64::from((v * 100.0).round() / 100.0);
    let mut t = toml::Table::new();
    t.insert("cell".into(), T::Integer(i64::from(cell)));
    t.insert(
        "at".into(),
        T::Array(vec![
            T::Float(round(x)),
            T::Float(round(y)),
            z.map_or_else(|| T::String("ground".into()), |z| T::Float(round(z))),
        ]),
    );
    t.insert("heading".into(), T::Float(round(heading)));
    T::Table(t)
}

/// A place as the table holds it, written as a row.
#[must_use]
pub fn row_of(p: &Position) -> T {
    let o = p.frame.origin;
    place_row(p.cell.0, o.x, o.y, Some(o.z), heading(&p.frame.rotation))
}

/// Why the camera's spot is no place to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// In a cell with no floor under the camera.
    NoFloor,
    /// Outdoors where there is no land.
    OffLand,
    /// Outdoors under the ground.
    UnderGround,
}

/// Where the camera stands, as the tab is told it: landblock `lb`, `x`, `y`, `z` in its meters,
/// and the indoor cell it is in with the floor under it there (`Some((index, floor))`), or the
/// ground under it outdoors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub lb: u16,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: f32,
    pub cell: Option<(u16, Option<f32>)>,
    pub ground: Option<f32>,
}

/// The place the camera's spot makes, or why it makes none.
///
/// # Errors
/// A cell with no floor under the camera, a spot off the land, or one under the ground.
pub fn place_here(c: &Camera) -> Result<T, Refused> {
    if let Some((index, floor)) = c.cell {
        let floor = floor.ok_or(Refused::NoFloor)?;
        return Ok(place_row(
            (u32::from(c.lb) << 16) | u32::from(index),
            c.x,
            c.y,
            Some(floor),
            c.heading,
        ));
    }
    let ground = c.ground.ok_or(Refused::OffLand)?;
    if c.z < ground - UNDER {
        return Err(Refused::UnderGround);
    }
    Ok(place_row(
        (u32::from(c.lb) << 16) | crate::edit::outdoor_cell(c.x, c.y),
        c.x,
        c.y,
        None,
        c.heading,
    ))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's starting areas)
    use super::*;

    fn camera() -> Camera {
        Camera {
            lb: 0xC6A9,
            x: 84.0,
            y: 141.0,
            z: 60.0,
            heading: 90.0,
            cell: None,
            ground: Some(58.0),
        }
    }

    /// Outdoors over the ground, a place stands on the ground in the outdoor cell under the camera;
    /// under the ground or off the land it is refused. Indoors, it stands on the cell's floor, and
    /// a cell with no floor under the camera is refused.
    #[test]
    fn a_place_from_the_camera_stands_on_land_or_a_floor_or_is_refused() {
        let p = place_here(&camera()).unwrap();
        assert_eq!(p["cell"], T::Integer(0xC6A9_001E));
        assert_eq!(p["at"].as_array().unwrap()[2], T::String("ground".into()));
        assert_eq!(p["heading"], T::Float(90.0));
        assert_eq!(
            place_here(&Camera {
                z: 50.0,
                ..camera()
            }),
            Err(Refused::UnderGround)
        );
        assert_eq!(
            place_here(&Camera {
                ground: None,
                ..camera()
            }),
            Err(Refused::OffLand)
        );
        let inside = place_here(&Camera {
            cell: Some((0x0105, Some(12.5))),
            ..camera()
        })
        .unwrap();
        assert_eq!(inside["cell"], T::Integer(0xC6A9_0105));
        assert_eq!(inside["at"].as_array().unwrap()[2], T::Float(12.5));
        assert_eq!(
            place_here(&Camera {
                cell: Some((0x0105, None)),
                ..camera()
            }),
            Err(Refused::NoFloor)
        );
    }

    /// A heading made a rotation reads back as itself.
    #[test]
    fn a_heading_reads_back_from_its_rotation() {
        assert!(heading(&crate::compile::heading_quat(0.0)).is_sign_positive());
        for h in [0.0_f32, 45.0, 90.0, 180.0, 270.0, 359.0] {
            let q = crate::compile::heading_quat(h);
            assert!((heading(&q) - h).abs() < 1e-3, "{h}: {}", heading(&q));
        }
    }
}
