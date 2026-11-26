//! Divergence: V577
//! Outdoor encounters fire only on a live scene slot of their vertex and stand at the slot's
//! placed point; the row of an edge vertex fires once, in the landblock that holds the slot.
//! Fixture: fake dats with one slot scene and flat land (`FakeDats::with_encounter_slots_offset`);
//! in the real-content tier the retail dats and world.pack; with `launch-content`, the Launch fixture.

use std::sync::Arc;

use dereth_primitives::{LandblockId as PLandblockId, Vec3};
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_content::models::world::{Encounter, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{PropertyDataId, PropertyString, WeenieType};
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_testkit::{land, EmptyShard};
use empyrean_world::entity::landblock;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::physics::phys_ext;
use empyrean_world::World;

/// The landblock under test and its east neighbor.
const WEST: u16 = 0xA9B4;
const EAST: u16 = 0xAAB4;
const WCID: u32 = 2002;

fn creature() -> Weenie {
    Weenie::new(WCID, "encounter", WeenieType::Creature)
        .with_string(PropertyString::Name, "encounter")
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

fn row(id: u32, landblock: u16, cell_x: i32, cell_y: i32) -> Encounter {
    Encounter {
        id,
        landblock: i32::from(landblock),
        weenie_class_id: WCID,
        cell_x,
        cell_y,
        ..Encounter::default()
    }
}

/// A world on flat land over both landblocks, whose scenes put one slot `offset` from each vertex.
fn world(content: MemContent, offset: Vec3) -> (World, VirtualClock) {
    let dats = empyrean_testkit::dats::with_stat_tables(
        FakeDats::new().with_encounter_slots_offset(&[WEST, EAST], offset),
    )
    .build()
    .unwrap();
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let mut w = World::new(ClockSnapshot::take(&clock, timers.portal_year_ticks), dats);
    w.timers = timers;
    w.content = Arc::new(content.weenie(creature()));
    gm::initialize(&mut w, &mut EmptyShard);
    land::use_flat_land_with_test_setup(&mut w, &[WEST, EAST], 0);
    (w, clock)
}

fn fired(w: &World, landblock: u16) -> Vec<(u32, (f32, f32))> {
    landblock::encounter_placements(w, landblock)
        .iter()
        .map(|p| (p.encounter.id, (p.origin.x, p.origin.y)))
        .collect()
}

#[test]
fn an_encounter_stands_at_its_slots_placed_point_not_at_its_vertex() {
    let (mut w, clock) = world(
        MemContent::new().encounter(row(1, WEST, 2, 3)),
        Vec3::new(5.0, 3.0, 0.0),
    );
    let slot = phys_ext::scene_slots(&w, PLandblockId(WEST))
        .into_iter()
        .find(|s| s.vertex == 2 * 9 + 3)
        .expect("the vertex's slot");
    let (dx, dy) = (slot.frame.origin.x - 48.0, slot.frame.origin.y - 72.0);
    assert!(
        ((dx * dx + dy * dy).sqrt() - 34f32.sqrt()).abs() < 1e-4,
        "the slot stands its offset from the vertex: {:?}",
        slot.frame.origin
    );

    let id = LandblockId::new(u32::from(WEST) << 16 | 0xFFFF);
    lm::get_landblock(&mut w, id, false, false);
    w.now = ClockSnapshot::take(&clock, w.timers.portal_year_ticks);
    let pyt = w.timers.portal_year_ticks;
    lm::tick(&mut w, pyt);
    let placed: Vec<ObjectGuid> = w
        .landblock_manager
        .landblocks
        .expect(id)
        .world_object_guids()
        .copied()
        .collect();
    assert_eq!(placed.len(), 1);
    let at = w.objects.get(placed[0]).unwrap().location().unwrap();
    assert!(
        (at.position_x - slot.frame.origin.x).abs() < 1e-4
            && (at.position_y - slot.frame.origin.y).abs() < 1e-4,
        "{at:?} stands at the slot {:?}",
        slot.frame.origin
    );
    assert!(at.position_z.abs() < 1e-4, "on the ground: {at:?}");
}

#[test]
fn an_edge_vertex_row_fires_once_in_the_landblock_that_holds_its_slot() {
    // With no offset every slot stands on its vertex, and a point on the shared edge (x = 192
    // in the west block, 0 in the east one) is the east block's.
    let content = MemContent::new()
        // The same point in both blocks: the east block's own row fires.
        .encounter(row(1, WEST, 8, 4))
        .encounter(row(2, EAST, 0, 4))
        // Only the west block has a row: it fires in the east block, which holds the slot.
        .encounter(row(3, WEST, 8, 5))
        // Only the east block has a row.
        .encounter(row(4, EAST, 0, 6))
        // Inside the west block.
        .encounter(row(5, WEST, 7, 4));
    let (w, _) = world(content, Vec3::ZERO);
    assert_eq!(fired(&w, WEST), [(5, (168.0, 96.0))]);
    assert_eq!(
        fired(&w, EAST),
        [(2, (0.0, 96.0)), (4, (0.0, 144.0)), (3, (0.0, 120.0))],
        "the block's own rows in their order, then the neighbor's"
    );
}

#[test]
fn a_row_whose_vertex_has_no_slot_fires_nowhere() {
    // A row past the grid's last vertex has no slot, in any block.
    let content = MemContent::new()
        .encounter(row(1, WEST, 9, 0))
        .encounter(row(2, WEST, 3, 3));
    let (w, _) = world(content, Vec3::ZERO);
    assert_eq!(fired(&w, WEST), [(2, (72.0, 72.0))]);
    assert!(fired(&w, EAST).is_empty());
}

/// Every encounter row of a world's whole map, classed by where its vertex's slot is live, and
/// what the landblocks fire.
#[cfg(any(feature = "real-content", feature = "launch-content"))]
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    /// Every row.
    pub rows: usize,
    /// Rows whose vertex has a live slot in the row's own landblock.
    pub live_in_own_block: usize,
    /// Rows whose vertex's live slot is in a neighboring landblock.
    pub live_only_in_neighbor: usize,
    /// Rows whose vertex has no live slot anywhere.
    pub no_live_slot: usize,
    /// Encounters the landblocks fire, each on its own slot.
    pub fired: usize,
    /// Of those, the rows a neighboring landblock holds.
    pub fired_from_neighbor: usize,
}

/// [`Tally`] over every landblock of `w`'s map.
#[cfg(any(feature = "real-content", feature = "launch-content"))]
pub(crate) fn tally(w: &World) -> Tally {
    use std::collections::{BTreeMap, BTreeSet};

    let block = |x: i32, y: i32| -> Option<u16> {
        Some(u16::from_be_bytes([
            u8::try_from(x).ok().filter(|&v| v < 0xFF)?,
            u8::try_from(y).ok().filter(|&v| v < 0xFF)?,
        ]))
    };
    let mut with_rows = Vec::new();
    for x in 0..0xFF {
        for y in 0..0xFF {
            let b = block(x, y).unwrap();
            if !w.content.get_cached_encounters_by_landblock(b).is_empty() {
                with_rows.push((x, y));
            }
        }
    }
    let mut near = BTreeSet::new();
    for &(x, y) in &with_rows {
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(b) = block(x + dx, y + dy) {
                    near.insert(b);
                }
            }
        }
    }
    // Each landblock's vertices with a live slot.
    let live: BTreeMap<u16, BTreeSet<usize>> = near
        .iter()
        .map(|&b| {
            let slots = phys_ext::scene_slots(w, PLandblockId(b));
            (b, slots.iter().map(|s| s.vertex).collect())
        })
        .collect();
    let live_at = |b: Option<u16>, i: i32, j: i32| {
        (0..=8).contains(&i)
            && (0..=8).contains(&j)
            && b.and_then(|b| live.get(&b))
                .is_some_and(|v| v.contains(&(usize::try_from(i * 9 + j).unwrap())))
    };

    let mut t = Tally::default();
    for &(x, y) in &with_rows {
        for r in w
            .content
            .get_cached_encounters_by_landblock(block(x, y).unwrap())
            .iter()
        {
            t.rows += 1;
            let (i, j) = (r.cell_x, r.cell_y);
            if live_at(block(x, y), i, j) {
                t.live_in_own_block += 1;
                continue;
            }
            let edge = |c: i32| match c {
                0 => vec![0, -1],
                8 => vec![0, 1],
                _ => vec![0],
            };
            let in_neighbor = edge(i).iter().any(|&dx| {
                edge(j).iter().any(|&dy| {
                    (dx, dy) != (0, 0) && live_at(block(x + dx, y + dy), i - 8 * dx, j - 8 * dy)
                })
            });
            if in_neighbor {
                t.live_only_in_neighbor += 1;
            } else {
                t.no_live_slot += 1;
            }
        }
    }
    for &b in &near {
        for p in landblock::encounter_placements(w, b) {
            t.fired += 1;
            if p.encounter.landblock != i32::from(b) {
                t.fired_from_neighbor += 1;
            }
        }
    }
    t
}

/// The real-content tier: the retail dats under `DERETH_TEST_DAT_DIR` and `world.pack` under
/// `EMPYREAN_TEST_WORLD_PACK`.
#[cfg(feature = "real-content")]
mod real_content {
    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};

    use super::*;

    #[test]
    fn on_the_end_of_retail_dats_aces_rows_fire_only_where_a_slot_survives() {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let dats = DatManager::initialize(Arc::new(source)).expect("retail dats");
        let path = empyrean_common::test_paths::world_pack();
        let content = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        let clock = VirtualClock::default();
        let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats);
        w.content = Arc::new(content);
        // A fifth of the rows stand where no slot survives, and a tenth are live only in the
        // neighboring landblock, which fires them; the five points with a row in two landblocks
        // fire once.
        assert_eq!(
            tally(&w),
            Tally {
                rows: 165_465,
                live_in_own_block: 117_155,
                live_only_in_neighbor: 15_744,
                no_live_slot: 32_566,
                fired: 132_894,
                fired_from_neighbor: 15_794,
            }
        );
    }
}
