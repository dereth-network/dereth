//! Divergence: V391
//! On the February 2005 dats the server reads the human motion table in the final command
//! numbering: the logout lasts the length of the departure those files give it, an emote state
//! numbered above the final client's insertion (sitting) has its cycle and its way in, and the
//! files' numbering is the one the wire uses. The end-of-retail logout is unchanged.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`) and the retail
//! dats.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dereth_world_data::command_numbering::CommandNumbering;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::{MotionCommand, MotionStance};
use empyrean_world::physics::motion_table as mt;
use empyrean_world::World;

/// The human motion table, which every player uses.
const HUMAN: u32 = 0x0900_0001;

fn open(dir: &std::path::Path) -> Arc<DatManager> {
    let real =
        RealDats::open(dir).unwrap_or_else(|e| panic!("the dats under {}: {e}", dir.display()));
    DatManager::initialize(Arc::new(real)).expect("the dats initialize")
}

fn world(dats: Arc<DatManager>) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, dats)
}

fn february_2005() -> World {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    let dats = DATS.get_or_init(|| {
        if let Some(msg) = dereth_dat::testing::classic_shortfall() {
            panic!("{msg}");
        }
        open(&dereth_dat::testing::classic_dat_dir().unwrap_or_default())
    });
    world(Arc::clone(dats))
}

fn end_of_retail() -> World {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    world(Arc::clone(
        DATS.get_or_init(|| open(&dereth_dat::testing::dat_dir())),
    ))
}

/// The length of the one animation the human table's way from `Ready` to `motion` plays.
fn link_length(w: &World, motion: MotionCommand) -> f32 {
    let link = mt::get_link_data(w, HUMAN, motion.0, None).expect("a way in");
    link.anims
        .iter()
        .map(|a| mt::get_animation_length_anim(w, a))
        .sum()
}

#[test]
fn on_the_february_2005_dats_the_logout_lasts_its_own_departure() {
    let w = february_2005();
    assert_eq!(
        w.dats.portal_dat().command_numbering(),
        CommandNumbering::Before2015
    );
    let length = mt::get_animation_length(
        &w,
        HUMAN,
        MotionStance::NonCombat,
        MotionCommand::LogOut,
        1.0,
    );
    assert!(length > 1.0, "the logout plays: {length}");
    assert!(
        (length - link_length(&w, MotionCommand::LogOut)).abs() < 1e-4,
        "{length} is the departure's own length"
    );
    let link = mt::get_link_data(&w, HUMAN, MotionCommand::LogOut.0, None).expect("a way in");
    assert_eq!(link.anims[0].anim_id.0, 0x0300_07BE, "the 2005 departure");

    let later = end_of_retail();
    assert_eq!(
        later.dats.portal_dat().command_numbering(),
        CommandNumbering::Final
    );
    let link = mt::get_link_data(&later, HUMAN, MotionCommand::LogOut.0, None).expect("a way in");
    assert_eq!(
        link.anims[0].anim_id.0, 0x0300_0C22,
        "the end-of-retail departure"
    );
    assert!(
        mt::get_animation_length(
            &later,
            HUMAN,
            MotionStance::NonCombat,
            MotionCommand::LogOut,
            1.0
        ) > 1.0
    );
}

#[test]
fn on_the_february_2005_dats_sitting_has_its_cycle_and_its_way_in() {
    let w = february_2005();
    let cycle =
        mt::get_motion_data(&w, HUMAN, MotionCommand::SitState.0, None).expect("the sitting cycle");
    assert!(!cycle.anims.is_empty());
    assert!(
        mt::get_cycle_length(
            &w,
            HUMAN,
            MotionStance::NonCombat,
            MotionCommand::SitState,
            1.0
        ) > 0.0
    );
    assert!(link_length(&w, MotionCommand::SitState) > 0.0);
    // Read in its own numbering the cycle would sit at the final HouseRecall's number.
    assert!(mt::get_motion_data(&w, HUMAN, MotionCommand::HouseRecall.0, None).is_none());
}
