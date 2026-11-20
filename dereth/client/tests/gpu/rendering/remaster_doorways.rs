//! Behaviour: none (experimental)
//!
//! The remaster on the oldest data: from a room of a dungeon, the doorway ahead shows the rooms
//! beyond it, at the depth Retail Look draws them, before the body walks through; and from
//! outdoors, a building's doorway shows its rooms. The October 1999 rooms and building shells
//! carry an opaque untextured surface in their openings, which neither draws.
//!
//! Fixture: the `portal.dat` and `cell.dat` of the October 1999 capture (the `1999-10-09`
//! folder under `DERETH_TEST_DAT_CAPTURES_DIR`) with the retail dats (`DERETH_TEST_DAT_DIR`)
//! beside them, and a hardware Vulkan 1.3 device.

#![cfg(all(gpu, feature = "remaster"))]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::Vec3;

use crate::instruments::remaster_frames::{run, Draw, Frame};
use crate::instruments::stations::{Clock, Place, Station};

/// The October 1999 world beside the retail dats, as the client opens an older world.
fn october_1999() -> Arc<RetailDatStore> {
    let (_, _, cell) = dereth_dat::testing::dat_captures_or_fail()
        .into_iter()
        .find(|(folder, file, _)| folder == "1999-10-09" && file == "cell.dat")
        .expect("the 1999-10-09 capture holds a cell.dat");
    let dir = cell.parent().expect("the capture's folder").to_path_buf();
    Arc::new(
        RetailDatStore::open_classic_with_modern(&dir, &dereth_dat::testing::dat_dir())
            .expect("the October 1999 files open beside the retail ones"),
    )
}

/// A body standing in a room of the dungeon under block `0x017E`, facing the doorway into the
/// corridor and the rooms beyond it.
fn facing_the_doorway() -> Station {
    Station {
        name: "dungeon-doorway-1999",
        why: "a doorway between two rooms of the October 1999 world",
        place: Place::Body {
            cell: 0x017E_0166,
            origin: Vec3::new(30.466_879, -77.079_13, -4.667_748),
            yaw: -1.875,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    }
}

/// Of the pixels Retail Look draws beyond the doorway (farther than the doorway's own plane),
/// the share the remaster draws nearer than Retail Look does, and how many there are.
fn hidden_beyond(retail: &Frame, remaster: &Frame) -> (f64, usize) {
    // The doorway's plane is at about 0.974 in this frame, the rooms beyond at 0.99 and over.
    const BEYOND: f32 = 0.985;
    let mut beyond = 0usize;
    let mut hidden = 0usize;
    for (r, m) in retail.depth.iter().zip(&remaster.remaster_depth) {
        if *r > BEYOND && *r < 1.0 {
            beyond += 1;
            if *m < r - 0.002 {
                hidden += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts of one frame
    let share = hidden as f64 / beyond.max(1) as f64;
    (share, beyond)
}

/// From a room whose doorway leads on into other rooms, the remaster draws the rooms beyond
/// where Retail Look draws them, with the body still on this side of the doorway: under 1% of
/// the pixels Retail Look draws beyond the doorway are covered by something nearer.
#[test]
fn the_rooms_beyond_a_doorway_draw_before_the_body_walks_through() {
    let _gpu = crate::common::gpu_lock();
    let store = october_1999();
    let station = facing_the_doorway();
    let size = (960, 540);
    let retail = run(&store, &station, Draw::Retail, size);
    let remaster = run(&store, &station, Draw::Pixel, size);
    let (share, beyond) = hidden_beyond(&retail, &remaster);
    assert!(
        beyond > 10_000,
        "Retail Look draws the rooms beyond the doorway over {beyond} pixels"
    );
    assert!(
        share < 0.01,
        "the remaster hides {:.1}% of the {beyond} pixels beyond the doorway",
        share * 100.0
    );
}

/// The camera outdoors before the training hall's doorway (block `0xC888`), looking south through
/// the doorway into the hall, where a player's report put it.
fn before_the_training_hall() -> Station {
    Station {
        name: "training-hall-doorway-1999",
        why: "a building's doorway of the October 1999 world, seen from outdoors",
        place: Place::Free {
            block: 0xC888,
            x: 60.05,
            y: 143.82,
            eye: 1.9,
            yaw: -3.161_148_5,
            pitch: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    }
}

/// From outdoors, the training hall's doorway shows the hall's rooms where Retail Look draws them:
/// the shell's own opening, an untextured pane, is not drawn over them. Under 1% of the pixels
/// Retail Look draws are covered by something the remaster draws nearer.
#[test]
fn a_buildings_doorway_shows_its_rooms_from_outdoors() {
    let _gpu = crate::common::gpu_lock();
    let store = october_1999();
    let station = before_the_training_hall();
    let size = (960, 540);
    let retail = run(&store, &station, Draw::Retail, size);
    let remaster = run(&store, &station, Draw::Pixel, size);
    let mut drawn = 0usize;
    let mut covered = 0usize;
    for (r, m) in retail.depth.iter().zip(&remaster.remaster_depth) {
        if *r < 1.0 {
            drawn += 1;
            if *m < r - 0.002 {
                covered += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts of one frame
    let share = covered as f64 / drawn.max(1) as f64;
    assert!(
        share < 0.01,
        "the remaster covers {covered} of the {drawn} pixels Retail Look draws ({:.1}%)",
        share * 100.0
    );
}
