//! Behaviour: none (experimental)
//!
//! The remaster outdoors, looking into a building through its doorway: the rooms beyond the
//! doorway are drawn where Retail Look draws them, and the ground the building stands on, inside
//! its walls, does not cover them. The place is a cave whose entrance stands on the ground with
//! its stairs going down under it, seen from just outside the entrance, where a player's camera
//! stood when the ground covered the stairs.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(all(gpu, feature = "remaster"))]

use dereth_client_runtime::report::{ReportCamera, VisualReport};
use dereth_primitives::Vec3;

use crate::instruments::remaster_frames::{run_set, Draw, Frame, Setup};
use crate::instruments::stations::{report_station, Clock, Station};

/// The camera outside the cave entrance of block `0x2E11`, looking in and down through its
/// doorway at the stairs under the entrance's floor.
fn outside_the_cave() -> VisualReport {
    VisualReport {
        camera: Some(ReportCamera {
            block: 0x2E11,
            origin: Vec3::new(131.310_29, 54.091_656, 59.072_487),
            yaw: -0.080_129_14,
            pitch: -0.385_000_05,
            cell: Some(0x2E11_002B),
        }),
        fov: 89.8,
        ..VisualReport::default()
    }
}

fn station(report: &VisualReport) -> Station {
    let mut s = report_station(report, Some(2));
    s.clock = Clock::Sunny(0.5);
    s.weather = false;
    s
}

/// Of the pixels Retail Look draws, how many the remaster draws nearer than Retail Look does,
/// and how many Retail Look draws.
fn hidden(retail: &Frame, remaster: &Frame) -> (usize, usize) {
    let mut drawn = 0usize;
    let mut nearer = 0usize;
    for (r, m) in retail.depth.iter().zip(&remaster.remaster_depth) {
        if *r < 1.0 {
            drawn += 1;
            if *m < r - 0.002 {
                nearer += 1;
            }
        }
    }
    (nearer, drawn)
}

/// From outside a cave's entrance, the stairs under the entrance's floor are seen through its
/// doorway as Retail Look draws them: the ground inside the entrance's walls covers under 0.5% of
/// the pixels Retail Look draws.
#[test]
fn the_rooms_under_a_buildings_floor_show_through_its_doorway_from_outdoors() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = outside_the_cave();
    let station = station(&report);
    let size = (960, 540);
    let setup = Setup {
        report: Some(&report),
        ..Setup::default()
    };
    let retail = run_set(&store, &station, Draw::Retail, size, &setup);
    let remaster = run_set(&store, &station, Draw::Pixel, size, &setup);
    let (nearer, drawn) = hidden(&retail, &remaster);
    #[allow(clippy::cast_precision_loss)] // pixel counts of one frame
    let share = nearer as f64 / drawn.max(1) as f64;
    assert!(
        drawn > 100_000,
        "Retail Look draws the world over {drawn} pixels"
    );
    assert!(
        share < 0.005,
        "the remaster draws something nearer than Retail Look over {nearer} of the {drawn} \
         pixels Retail Look draws ({:.2}%)",
        share * 100.0
    );
}
