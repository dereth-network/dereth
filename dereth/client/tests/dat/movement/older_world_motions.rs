//! On a world of older data files the client plays every motion as those files key it: the
//! server numbers its commands as the client of the files' day did, the client reads them in that
//! numbering and plays them from the files' own motion tables, and its own motion state goes back
//! out in the same numbering. The February 2005 logout plays its departure; every emote key
//! animates that world's human body; the end-of-retail world is unchanged.
//! Fixture: the production `Character` and object stream over the February 2005 dats and over
//! the retail dats; no device.

use std::sync::Arc;

use dereth_animation::command::CommandNumbering;
use dereth_animation::data::AnimAssets;
use dereth_animation::MotionCommand;
use dereth_client_runtime::actions::emote::INPUT_ACTION_COMMANDS;
use dereth_client_runtime::character::Character;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime};
use dereth_protocol::movement::{InterpretedMotionState, MotionAction};
use {dereth_world_data::landblock::load_region, dereth_world_data::landblock::DEFAULT_LANDBLOCK};

/// The human motion table, which every human uses.
const HUMAN_MOTION_TABLE: DataId = DataId(0x0900_0001);
/// The departure the February 2005 files play for `LogOut`, and the end of retail's.
const DEPARTURE_2005: u32 = 0x0300_07BE;
const DEPARTURE_END_OF_RETAIL: u32 = 0x0300_0C22;

fn older() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_classic_store_or_fail())
}

fn later() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// A body standing in Holtburg over `store`, settled on the ground.
fn body(store: &Arc<RetailDatStore>) -> Character {
    let region = load_region(store).expect("the region");
    let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, (96.0, 96.0))
        .expect("a body on the terrain");
    for f in 1..=60 {
        c.update(LocalTime(f64::from(f) / 30.0));
    }
    c
}

/// The animations the body's sequence holds now.
fn playing(c: &Character) -> Vec<u32> {
    c.driver()
        .sequence
        .nodes()
        .iter()
        .map(|n| n.anim_id.0)
        .collect()
}

/// The server's logout: `NonCombat`, and the logout queued as an action, with the indices the
/// world's numbering gives them.
fn logout_on_the_wire(n: CommandNumbering) -> InterpretedMotionState {
    InterpretedMotionState {
        current_style: n.command_to_wire(MotionCommand::NON_COMBAT),
        actions: vec![MotionAction {
            command_index: n
                .command_to_wire(MotionCommand::LOG_OUT)
                .expect("every era logs out"),
            stamp_and_autonomy: 1,
            speed: 1.0,
        }],
        ..InterpretedMotionState::default()
    }
}

/// Apply the server's logout through the stream's numbering, and return what the body plays
/// over the next second.
fn logout_plays(store: &Arc<RetailDatStore>) -> (CommandNumbering, u16, Vec<u32>) {
    let stream = ObjectStream::with_store(Arc::clone(store));
    let n = stream.command_numbering();
    let wire = logout_on_the_wire(n);
    let state = dereth_client_runtime::movement::interpreted_state_in(&wire, n);
    assert_eq!(state.actions.len(), 1);
    assert_eq!(state.actions[0].action, MotionCommand::LOG_OUT, "{n:?}");
    let mut c = body(store);
    c.move_to_interpreted_state(&state);
    let mut seen = Vec::new();
    for f in 61..=90 {
        c.update(LocalTime(f64::from(f) / 30.0));
        seen.extend(playing(&c));
    }
    (n, wire.actions[0].command_index, seen)
}

/// Behaviour: movement.era.an-older-worlds-logout-plays-its-own-departure
#[test]
fn on_an_older_world_the_servers_logout_plays_the_departure_its_files_give_it() {
    let (n, index, seen) = logout_plays(&older());
    assert_eq!(n, CommandNumbering::Before2015);
    assert_eq!(index, 0x11B, "the February 2005 client's LogOut");
    assert!(
        seen.contains(&DEPARTURE_2005),
        "the departure plays: {seen:08X?}"
    );

    let (n, index, seen) = logout_plays(&later());
    assert_eq!(n, CommandNumbering::Final);
    assert_eq!(index, 0x11E, "the end-of-retail LogOut, as before");
    assert!(
        seen.contains(&DEPARTURE_END_OF_RETAIL),
        "the end-of-retail departure plays: {seen:08X?}"
    );
}

/// Behaviour: movement.era.every-emote-key-animates-an-older-worlds-human-body
#[test]
fn every_emote_key_animates_the_february_2005_human_body() {
    let store = older();
    let assets = dereth_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store));
    let mt = assets
        .motion_table(HUMAN_MOTION_TABLE)
        .expect("the human table");
    let style = (MotionCommand::NON_COMBAT.0 & 0xFFFF) << 16;
    let linked: std::collections::BTreeSet<u32> = mt
        .links
        .values()
        .flat_map(|inner| inner.keys().copied())
        .collect();
    let mut silent = Vec::new();
    let mut plays_count = 0;
    for &(_, cmd) in INPUT_ACTION_COMMANDS {
        let c = MotionCommand(cmd);
        let plays = if c.is_substate() {
            mt.cycles.contains_key(&(style | c.ordinal()))
        } else {
            linked.contains(&cmd)
        };
        if plays {
            plays_count += 1;
        } else {
            silent.push(c.name().unwrap_or("?"));
        }
    }
    // Every one plays, those numbered above the final client's insertion (sitting, pointing,
    // the snow angel) included.
    assert!(plays_count > 60, "{plays_count} emote keys play");
    assert!(silent.is_empty(), "silent emote keys: {silent:?}");
    for state in [
        MotionCommand::SIT_STATE,
        MotionCommand::SNOW_ANGEL_STATE,
        MotionCommand::POINT_LEFT_STATE,
        MotionCommand::AT_EASE_STATE,
    ] {
        assert!(
            mt.cycles.contains_key(&(style | state.ordinal())),
            "{state:?}"
        );
    }
}

/// Behaviour: movement.era.an-older-worlds-client-sends-its-motion-in-its-files-numbering
#[test]
fn on_an_older_world_the_client_sends_its_motion_state_in_its_files_numbering() {
    let raw = dereth_animation::motion::RawMotionState {
        current_style: MotionCommand::ATLATL_COMBAT,
        forward_command: MotionCommand::SIT_STATE,
        ..Default::default()
    };
    let then =
        dereth_client_runtime::app::raw_motion_state_to_wire_in(&raw, CommandNumbering::Before2015);
    assert_eq!(
        then.current_style,
        Some(0x8000_0138),
        "the older AtlatlCombat"
    );
    assert_eq!(
        then.forward_command,
        Some(0x4300_013A),
        "the older SitState"
    );
    let now =
        dereth_client_runtime::app::raw_motion_state_to_wire_in(&raw, CommandNumbering::Final);
    assert_eq!(
        now,
        dereth_client_runtime::app::raw_motion_state_to_wire(&raw)
    );
    assert_eq!(now.current_style, Some(MotionCommand::ATLATL_COMBAT.0));
    assert_eq!(now.forward_command, Some(MotionCommand::SIT_STATE.0));
}
