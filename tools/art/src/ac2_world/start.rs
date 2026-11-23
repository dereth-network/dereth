//! The converted world's starting area: one, at [`start_point`], written into AC1's
//! character-creation table (`0x0E000002`) as the world's only starter area.
//!
//! The point is the dry land cell nearest world meters (17,840 east, 27,120 north), searched ring
//! by ring outward over AC1's 24 m land cells, the character half a meter above the ground at the
//! cell's middle, the ground and the water told as Empyrean's physics tells them on the converted
//! land ([`dry_ground`]). The area is named for the nearest of AC2's towns AC2's map names
//! ([`nearest_town`]). Every heritage offers it, and nothing else.

use dereth_assets::tables::{CharGen, StarterArea};
use dereth_assets::CellLandblock;
use dereth_physics::globals::{LAND_HEIGHT_TABLE_LEN, WATER_DEPTH_PARTIAL_WATER_VERTEX};
use dereth_physics::{PlaneExt as _, WaterType};
use dereth_primitives::land_grid::LandGrid;
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};

use crate::ac2_world::names::{self, Kind, Label};

/// The start point as the tool carries it: `ac2-world/start.toml`.
pub const START_TOML: &str = include_str!("../../ac2-world/start.toml");

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StartFile {
    version: u32,
    east: f64,
    north: f64,
}

/// Where new characters start, world meters east and north, from its file's text.
///
/// # Errors
/// The text does not read, or is not version 1.
pub fn parse_start(text: &str) -> Result<(f64, f64), String> {
    let f: StartFile = toml::from_str(text).map_err(|e| format!("the start point: {e}"))?;
    if f.version != 1 {
        return Err(format!("the start point is version {}, not 1", f.version));
    }
    Ok((f.east, f.north))
}

/// Where new characters start, world meters east and north, as the tool carries it.
///
/// # Panics
/// If the carried file does not read, which its test rules out.
#[must_use]
pub fn start_point() -> (f64, f64) {
    parse_start(START_TOML).expect("the carried start point reads")
}
/// How far above the ground a new character stands, meters.
pub const ABOVE_GROUND: f32 = 0.5;
/// How many rings out the search goes before it gives up.
const RINGS: i32 = 200;
/// AC1's land cell, meters, and the land cells along the world's side.
const CELL: f64 = 24.0;
const CELLS: i32 = 255 * 8;

/// The ground height at block-local point `local` of outdoor cell `cell` of `block`, when there
/// is dry land there: terrain under it, and neither a wholly water square nor water at the
/// nearest vertex. Empyrean's own test for a dry start.
#[must_use]
pub fn dry_ground(
    block: &dereth_physics::LandblockCollision,
    cell: CellId,
    local: Vec3,
) -> Option<f32> {
    let poly = block.find_terrain_poly(cell.index(), local)?;
    let wet = match block.water_type_at(cell.index(), local)? {
        WaterType::NotWater => false,
        WaterType::EntirelyWater => true,
        WaterType::PartiallyWater => {
            block.calc_water_depth(cell, local) == WATER_DEPTH_PARTIAL_WATER_VERTEX
        }
    };
    let mut at = local;
    (!wet && poly.plane.set_height(&mut at)).then_some(at.z)
}

/// One converted landblock record's collision, as Empyrean builds it from the overlay: the
/// world's grid of `steps` vertex steps a cell, its road width and its height table.
#[must_use]
pub fn collision(
    lb: &CellLandblock,
    steps: usize,
    road_width: f32,
    table: &[f32; LAND_HEIGHT_TABLE_LEN],
) -> Option<dereth_physics::LandblockCollision> {
    let raw = lb.id.raw() >> 16;
    let id = LandblockId(u16::try_from(raw).ok()?);
    let grid = LandGrid::with_steps(steps, road_width);
    dereth_world_data::land_source::land_collision(&grid, id, lb, table)
}

/// The land cell `(gx, gy)` (24 m cells from the world's south-west corner) as its cell id and
/// its middle, block-local; `None` outside the world.
#[must_use]
pub fn cell_middle(gx: i32, gy: i32) -> Option<(CellId, Vec3)> {
    if !(0..CELLS).contains(&gx) || !(0..CELLS).contains(&gy) {
        return None;
    }
    let (bx, by, ix, iy) = (gx / 8, gy / 8, gx % 8, gy % 8);
    let id = (u32::try_from(bx).ok()? << 24)
        | (u32::try_from(by).ok()? << 16)
        | u32::try_from(ix * 8 + iy + 1).ok()?;
    #[allow(clippy::cast_precision_loss)]
    let local = Vec3::new(ix as f32 * 24.0 + 12.0, iy as f32 * 24.0 + 12.0, 0.0);
    Some((CellId(id), local))
}

/// The start: the first land cell, ring by ring out from the one holding `point` (world
/// meters), at whose middle `ground` (a cell and its block-local middle) finds dry land, as a
/// position [`ABOVE_GROUND`] over it, facing north; `None` with no dry land within the search.
pub fn nearest_dry(
    point: (f64, f64),
    mut ground: impl FnMut(CellId, Vec3) -> Option<f32>,
) -> Option<Position> {
    #[allow(clippy::cast_possible_truncation)]
    let (cx, cy) = (
        // LINT-OK: a land cell of the world, a few thousand at most.
        (point.0 / CELL).floor() as i32,
        (point.1 / CELL).floor() as i32,
    );
    for r in 0..RINGS {
        for dx in -r..=r {
            for dy in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let Some((cell, local)) = cell_middle(cx + dx, cy + dy) else {
                    continue;
                };
                if let Some(z) = ground(cell, local) {
                    let at = Vec3::new(local.x, local.y, z + ABOVE_GROUND);
                    return Some(Position::new(cell, Frame::new(at, Quat::IDENTITY)));
                }
            }
        }
    }
    None
}

/// The name of the town among `labels` nearest world point `point` (meters), and how far it is;
/// `None` when AC2's map names no town.
#[must_use]
pub fn nearest_town(labels: &[Label], point: (f64, f64)) -> Option<(String, f64)> {
    labels
        .iter()
        .filter(|l| names::kind_of(l.icon) == Some(Kind::Town))
        .map(|l| {
            let (x, y) = l.at();
            (
                names::name_of(&l.words, Kind::Town),
                dereth_primitives::num::math::hypot(x - point.0, y - point.1),
            )
        })
        .filter(|(name, _)| !name.is_empty())
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// AC1's character-creation table with the world's one starter area, `name` at `at`, in place of
/// AC1's: every heritage starts there and offers nothing else.
#[must_use]
pub fn creation_table(ac1: &CharGen, name: &str, at: Position) -> CharGen {
    let mut cg = ac1.clone();
    cg.starter_areas = vec![StarterArea {
        name: name.to_owned(),
        locations: vec![at],
    }];
    for h in cg.heritage_groups.values_mut() {
        h.primary_start_areas = vec![0];
        h.secondary_start_areas = Vec::new();
    }
    cg
}

/// The world point (meters east, north) of an outdoor position.
#[must_use]
pub fn world_point(p: &Position) -> (f64, f64) {
    let (bx, by) = (
        f64::from(p.cell.0 >> 24),
        f64::from((p.cell.0 >> 16) & 0xFF),
    );
    (
        bx * 192.0 + f64::from(p.frame.origin.x),
        by * 192.0 + f64::from(p.frame.origin.y),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's starting area)

    /// The start point the tool carries reads, and is where new characters have always started.
    #[test]
    fn the_carried_start_point_reads_and_is_by_cragstone() {
        assert_eq!(parse_start(START_TOML), Ok((17_840.0, 27_120.0)));
        assert!(parse_start(
            "version = 2
east = 1.0
north = 2.0
"
        )
        .is_err());
    }
    use super::*;

    /// The search takes the start point's own cell when it is dry, as the server's stand-in did.
    #[test]
    fn the_start_point_s_own_cell_is_taken_when_it_is_dry() {
        let p = nearest_dry(start_point(), |_, _| Some(14.48)).unwrap();
        assert_eq!(p.cell, CellId(0x5C8D_003B));
        assert_eq!(p.frame.origin, Vec3::new(180.0, 60.0, 14.98));
    }

    /// Off a wet cell the search goes out a ring at a time, in the server's order: west to east
    /// across a ring, south to north in each column, the first dry cell of the nearest ring.
    #[test]
    fn the_search_goes_out_ring_by_ring_in_the_servers_order() {
        let (cx, cy) = (743, 1130);
        let mut seen = Vec::new();
        let dry = |gx: i32, gy: i32| (gx, gy) == (cx + 1, cy) || (gx, gy) == (cx - 1, cy + 1);
        let p = nearest_dry(start_point(), |cell, local| {
            let (bx, by) = (cell.0 >> 24, (cell.0 >> 16) & 0xFF);
            let ix = (cell.0 & 0xFFFF) - 1;
            let gx = i32::try_from(bx * 8 + ix / 8).unwrap();
            let gy = i32::try_from(by * 8 + ix % 8).unwrap();
            assert_eq!(
                (local.x, local.y),
                ((ix / 8) as f32 * 24.0 + 12.0, (ix % 8) as f32 * 24.0 + 12.0)
            );
            seen.push((gx - cx, gy - cy));
            dry(gx, gy).then_some(1.0)
        })
        .unwrap();
        assert_eq!(&seen[..4], &[(0, 0), (-1, -1), (-1, 0), (-1, 1)]);
        let (gx, gy) = (cx - 1, cy + 1);
        let (cell, local) = cell_middle(gx, gy).unwrap();
        assert_eq!(
            (p.cell, p.frame.origin),
            (cell, Vec3::new(local.x, local.y, 1.5))
        );
        assert!(nearest_dry(start_point(), |_, _| None).is_none());
    }

    /// The table keeps AC1's own everything but its starter areas, which are the world's one,
    /// and every heritage starts there.
    #[test]
    fn the_table_offers_the_one_area_to_every_heritage() {
        use dereth_assets::tables::HeritageGroup;
        let heritage = |primary: Vec<u32>, secondary: Vec<u32>| HeritageGroup {
            description: None,
            name: "Aluvian".into(),
            icon: 1,
            setup: dereth_primitives::DataId(2),
            environment_setup: dereth_primitives::DataId(3),
            attribute_credits: 330,
            skill_credits: 50,
            primary_start_areas: primary,
            secondary_start_areas: secondary,
            skills: vec![(6, 4, 8)],
            templates: Vec::new(),
            sex_order: Vec::new(),
            template_presentations: std::collections::BTreeMap::new(),
            sex_table_marker: 1,
            sexes: std::collections::BTreeMap::new(),
        };
        let ac1 = CharGen {
            help_strings: Vec::new(),
            id: dereth_primitives::DataId(0x0E00_0002),
            second_data_id: dereth_primitives::DataId(0),
            starter_areas: (0..5)
                .map(|i| StarterArea {
                    name: format!("Town {i}"),
                    locations: Vec::new(),
                })
                .collect(),
            hg_table_marker: 1,
            heritage_order: vec![1, 12],
            heritage_groups: std::collections::BTreeMap::from([
                (1, heritage(vec![0], vec![1, 2, 3])),
                (12, heritage(vec![4], Vec::new())),
            ]),
        };
        let at = Position::new(
            CellId(0x5C8D_003B),
            Frame::new(Vec3::new(180.0, 60.0, 14.98), Quat::IDENTITY),
        );
        let cg = creation_table(&ac1, "Cragstone", at);
        assert_eq!(
            cg.starter_areas,
            [StarterArea {
                name: "Cragstone".into(),
                locations: vec![at]
            }]
        );
        for h in cg.heritage_groups.values() {
            assert_eq!(
                (&h.primary_start_areas[..], &h.secondary_start_areas[..]),
                (&[0][..], &[][..])
            );
        }
        let mut rest = cg.clone();
        rest.starter_areas.clone_from(&ac1.starter_areas);
        for (k, h) in &mut rest.heritage_groups {
            h.primary_start_areas
                .clone_from(&ac1.heritage_groups[k].primary_start_areas);
            h.secondary_start_areas
                .clone_from(&ac1.heritage_groups[k].secondary_start_areas);
        }
        assert_eq!(rest, ac1, "nothing else changes");
        assert_eq!(world_point(&at), (17_844.0, 27_132.0));
    }

    /// The nearest of the towns names the area; landmarks and nameless labels do not.
    #[test]
    fn the_nearest_town_names_the_area() {
        let label = |icon: u32, words: &str, cell: (u32, u32)| Label {
            icon,
            words: words.into(),
            cell,
        };
        let labels = [
            label(0x4100_405A, "Old Cragstone Excavation", (892, 1356)),
            label(0x4100_405D, "Town of Cragstone (@@)", (898, 1355)),
            label(0x4100_405D, "Millstone", (819, 1403)),
            label(0x4100_405D, "", (892, 1356)),
        ];
        let (name, d) = nearest_town(&labels, start_point()).unwrap();
        assert_eq!(name, "Cragstone");
        assert!(
            (d - dereth_primitives::num::math::hypot(17_970.0 - 17_840.0, 27_110.0 - 27_120.0))
                .abs()
                < 1e-9
        );
        assert!(nearest_town(&labels[..1], start_point()).is_none());
    }
}
