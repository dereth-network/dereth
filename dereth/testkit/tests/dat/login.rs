//! The `dat` tier's login scenarios: the whole client running with no device and no window,
//! entering the world from the wizard as well as from character select, and the rule that two
//! bodies never share one object id (fixture: the `first-login-walk-jump` recording replayed
//! through a socket-free endpoint, on the retail dats).
//!
//! Every scenario here builds a whole client -- `App::with_platform(cfg, NullPresentation,
//! Platform::headless(w, h))` -- and **this binary must run serially**, because two headless
//! clients in one process share the UI request globals.
//!
//! Each scenario is a `pub fn` with a `#[test]` beside it that runs it and checks its declaration.
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.

use dereth_client::frame::FrameStep;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::present::{NullPresentation, NullPresentationCounts};
use dereth_client_net::client_session::testing::capture::Datagram;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_testkit::{ClientSpec, HeadlessClient, ScenarioView};

/// The recording the identity scenario replays, by its content slug.
const FIRST_LOGIN: &str = "first-login-walk-jump";

/// The recording, and the endpoint it replays into, both from `dereth_testkit::replay`.
use dereth_testkit::replay::records as recording;

/// A socket-free endpoint named after the recording's own connection sequence number.
fn endpoint(records: &[Datagram], account: &str) -> ClientNetwork {
    dereth_testkit::replay::recorded_endpoint_as(records, account, "unused")
}

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "a_whole_client_with_no_device_or_window",
        &["headless.a-whole-client-with-no-device-or-window"],
        a_whole_client_with_no_device_or_window,
    ),
    (
        "entering_the_world_reaches_the_hud_from_the_wizard",
        &["login.enter-world.reaches-the-hud-from-the-wizard-as-well-as-from-character-select"],
        entering_the_world_reaches_the_hud_from_the_wizard,
    ),
    (
        "two_bodies_cannot_share_one_id",
        &["login.player-identity.two-bodies-cannot-share-one-id"],
        two_bodies_cannot_share_one_id,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

/// What the null presentation was asked to do.
fn counts(v: &ScenarioView<'_>) -> NullPresentationCounts {
    v.expect_app()
        .presentation()
        .as_any()
        .downcast_ref::<NullPresentation>()
        .expect("the presentation this client was built with")
        .counts()
}

// -------------------------------------------------------------------------------------------
// 12. headless.a-whole-client-with-no-device-or-window
// -------------------------------------------------------------------------------------------

/// The whole client runs with no device and no window, on a clock that is exactly its own steps.
pub fn a_whole_client_with_no_device_or_window() {
    use dereth_client::app::HEADLESS_STEP;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let before = counts(&c.view());
    c.tick(2);

    c.assert_behaviour(
        "headless.a-whole-client-with-no-device-or-window",
        move |v| {
            let app = v.expect_app();
            let after = counts(v);
            let per_frame = |f: fn(&NullPresentationCounts) -> u64| f(&after) - f(&before) == 2;
            let drew = per_frame(|c| c.prepare_graphics_device)
                && per_frame(|c| c.start_frame)
                && per_frame(|c| c.draw_scene)
                && per_frame(|c| c.draw_ui)
                && per_frame(|c| c.end_frame);
            // Two frames of the fixed-step clock are two steps, which no wall clock can be.
            let simulated = (app.clock().cur_time - 2.0 * HEADLESS_STEP).abs() < 1e-12
                && app.clock().external_offset() == 0.0;
            // The seed rectangle is the default 800x600 window centred on the NullWindow's
            // 1920x1080 virtual desktop: the client starts windowed and full screen follows
            // gameplay, so this is not the whole desktop.
            let no_window = app.presentation().size() == (800, 600)
                && app
                    .window_rect()
                    .is_some_and(|r| (r.left, r.top, r.right, r.bottom) == (560, 240, 1360, 840));
            drew && simulated && no_window && app.last_frame_steps() == FrameStep::ORDER
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_whole_client_with_no_device_or_window() {
    scenario("a_whole_client_with_no_device_or_window");
}

// -------------------------------------------------------------------------------------------
// login.enter-world.reaches-the-hud-from-the-wizard-as-well-as-from-character-select
// -------------------------------------------------------------------------------------------

/// A character who has just been created walks into the world the same way a chosen one does.
///
/// **The calibration is the point.** A predicate that fires on two screens has to be shown *not*
/// firing on the others, or "the wizard switched" and "everything switches" are the same result;
/// and the edge has to be an edge, or a wizard that is merely open falls into the world.
///
/// The shell alone is what this needs -- the data files and nothing else, no device and no
/// socket -- so it is stood up directly rather than through a whole client.
pub fn entering_the_world_reaches_the_hud_from_the_wizard() {
    use dereth_client::ui::{HostState, UiShell};
    use dereth_primitives::LocalTime;
    use dereth_ui::framework::mode;
    use dereth_ui::{NullInputPump, UiMode};

    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail data files are this scenario's oracle and there are none at {} -- set \
         DERETH_TEST_DAT_DIR",
        dir.display()
    );
    let store = std::sync::Arc::new(
        dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail data files open"),
    );

    /// Stand a shell up and settle it on `want`, with none of the intro flow.
    fn settled(store: &std::sync::Arc<dereth_dat::RetailDatStore>, want: UiMode) -> UiShell {
        let mut shell =
            UiShell::new(store, (800, 600)).expect("the shell comes up on the retail data files");
        let host = HostState::default();
        shell.queue(want);
        for i in 0..8 {
            shell.frame(LocalTime(f64::from(i)), &host, &mut NullInputPump);
            if shell.flow.current_mode() == Some(want) {
                return shell;
            }
        }
        panic!("the shell never reached {want:?}")
    }

    /// Drive the edge once and answer with the screen the shell settled on.
    fn on_entering(shell: &mut UiShell) -> Option<UiMode> {
        let host = HostState {
            in_world: true,
            ..HostState::default()
        };
        for i in 100..110 {
            shell.frame(LocalTime(f64::from(i)), &host, &mut NullInputPump);
        }
        shell.flow.current_mode()
    }

    // The claim: the wizard's last page gives way to the heads-up display.
    let from_the_wizard =
        on_entering(&mut settled(&store, mode::CHAR_GEN)) == Some(mode::GAME_PLAY);
    // The positive control: the screen that already worked. Without it the line above could be
    // measuring a shell that switches unconditionally.
    let from_character_select =
        on_entering(&mut settled(&store, mode::CHARACTER_MANAGEMENT)) == Some(mode::GAME_PLAY);
    // The calibration: no other screen answers that notice at all.
    let mut nothing_else_switches = true;
    for m in [
        mode::DATA_PATCH,
        mode::INTRO,
        mode::DISCONNECTED,
        mode::CREDITS,
        mode::EPILOGUE,
    ] {
        nothing_else_switches &= on_entering(&mut settled(&store, m)) == Some(m);
    }
    // And it is an edge: a wizard that is merely open does not fall into the world.
    let mut wizard = settled(&store, mode::CHAR_GEN);
    let host = HostState::default();
    for i in 100..110 {
        wizard.frame(LocalTime(f64::from(i)), &host, &mut NullInputPump);
    }
    let it_stays_put = wizard.flow.current_mode() == Some(mode::CHAR_GEN);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.enter-world.reaches-the-hud-from-the-wizard-as-well-as-from-character-select",
        move |_| from_the_wizard && from_character_select && nothing_else_switches && it_stays_put,
    );
}

#[test]
fn scenario_entering_the_world_reaches_the_hud_from_the_wizard() {
    scenario("entering_the_world_reaches_the_hud_from_the_wizard");
}

// -------------------------------------------------------------------------------------------
// login.player-identity.two-bodies-cannot-share-one-id
// -------------------------------------------------------------------------------------------

/// Replay a recording through the client's own three lines and hand back the object stream at its
/// fullest, with the identity the shard gave the player.
///
/// **Stopped before the log-off**, because every recording ends with one and the client empties
/// the world when a character session ends.
fn replay_to_the_world(session: &str) -> (ObjectStream, ObjectId) {
    let records = recording(session);
    let run = |limit: usize| -> (ObjectStream, usize) {
        let mut net = endpoint(&records, "dereth-testkit");
        let mut objects = ObjectStream::new();
        let mut entered = false;
        let mut fullest = 0usize;
        for (index, r) in records.iter().enumerate() {
            if index >= limit {
                break;
            }
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, r.peer(), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(ch) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(ch.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
            if !objects.is_empty() {
                fullest = index;
            }
        }
        (objects, fullest)
    };
    let (_, fullest) = run(usize::MAX);
    let (stream, _) = run(fullest + 1);
    let player = stream
        .player()
        .unwrap_or_else(|| panic!("{session} never put a character in the world"));
    (stream, player)
}

/// Two bodies under one id leave the first out of the sweep, which is why a re-key onto an
/// occupied id is refused.
pub fn two_bodies_cannot_share_one_id() {
    use dereth_client::character::{Character, PLAYER_OBJECT_ID};
    use dereth_physics::source::StaticLandSource;
    use dereth_physics::{PhysicsWorld, SetupGeometry, Sphere};
    use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

    /// The id this client's own body carries before a shard has named it, and the one a shard
    /// allocates first -- which is what made the collision reachable at all.
    const FIRST_SHARD_ID: ObjectId = ObjectId(0x5000_0001);

    fn a_sphere() -> std::sync::Arc<SetupGeometry> {
        std::sync::Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
            radius: 0.5,
            height: 1.0,
            ..SetupGeometry::default()
        })
    }

    // 1. The mechanism, stated as a measurement: the id table replaces rather than rejects, and
    //    the body that loses its key is simply never stepped again.
    let block = LandblockId(0xA9B4);
    let mut land = StaticLandSource::linear();
    land.add_flat_block(block, 10);
    let mut w = PhysicsWorld::new(std::sync::Arc::new(land));
    let place = |w: &mut PhysicsWorld, id: ObjectId| {
        let h = w.create(id, a_sphere(), true);
        let cell = block.cell(1);
        w.enter_cell(h, cell);
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(96.0, 96.0, 100.0), Quat::IDENTITY),
        );
        o.transient_state.set_active_bit(true);
        o.update_time = 0.0;
        h
    };
    let first = place(&mut w, FIRST_SHARD_ID);
    let second = place(&mut w, FIRST_SHARD_ID);
    let replaced = w.by_object_id(FIRST_SHARD_ID) == Some(second);
    let swept = w.use_time(LocalTime(1.0), false);
    let first_is_out_of_the_sweep = w
        .get(first)
        .expect("still alive, merely unreachable")
        .update_time
        == 0.0;
    let second_is_in_it = w.get(second).expect("live").update_time > 0.0;

    // 2. And that is why a re-key onto an id another body already holds changes nothing and is
    //    counted -- otherwise the fix would reintroduce the eviction from the other direction.
    let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail data files are this scenario's oracle and there are none under {} -- set \
             DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    }));
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut body = Character::new(&store, &region, block.0, (96.0, 96.0)).expect("the body builds");
    let occupied = ObjectId(0x5000_0009);
    let sitting = body.world.create(occupied, a_sphere(), true);
    let refused = !body.adopt_server_id(Some(occupied))
        && body.object_id() == PLAYER_OBJECT_ID
        && body.stats.id_adoptions == 0
        && body.stats.id_adoptions_refused == 1
        && body.world.by_object_id(PLAYER_OBJECT_ID) == Some(body.handle)
        && body.world.by_object_id(occupied) == Some(sitting);

    // **What is deliberately not here, and why.** One more arm belongs to this claim: the same
    // recording with a second character standing on the recorded player's own spot, run twice --
    // once with the local body still holding the placeholder and once with it holding the
    // shard's id -- to show that the second character is refused a body in the first case and is
    // an ordinary solid one in the second. Its fixture does not build that yet: a presence that
    // fills only the body's achieved pose and not the last pose the wire named creates no body at
    // all, so the arm is not written here until the fixture fills both.

    // 4. And the whole recording made solid around it leaves the local body answering to its id
    //    and still in the sweep.
    let (mut stream, player) = replay_to_the_world(FIRST_LOGIN);
    let pos = stream
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player is placed");
    let block = pos.cell.landblock();
    let mut c = Character::new(&store, &region, block.0, (96.0, 96.0)).expect("the body builds");
    c.land().load_block_cells(block);
    c.teleport(pos);
    c.adopt_server_id(Some(player));
    stream.sync_physics(&store, &mut c.world);
    let st = stream.physics.stats;
    let the_corpus_was_made_solid = st.created > 0 && st.id_collision == 0;
    let before = c.world.get(c.handle).expect("live").update_time;
    let swept_after = c.world.use_time(LocalTime(before + 1.0), false)
        && c.world.get(c.handle).expect("live").update_time > before;
    let the_local_body_survives = c.world.by_object_id(player) == Some(c.handle) && swept_after;

    for (what, ok) in [
        ("the second insert replaced the first", replaced),
        ("the sweep ran", swept),
        (
            "the first body was never stepped",
            first_is_out_of_the_sweep,
        ),
        ("the second was", second_is_in_it),
        ("a re-key onto an occupied id is refused", refused),
        ("the corpus was made solid", the_corpus_was_made_solid),
        ("the local body survives", the_local_body_survives),
    ] {
        println!("identity: {what}: {ok}");
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.player-identity.two-bodies-cannot-share-one-id",
        move |_| {
            replaced
                && swept
                && first_is_out_of_the_sweep
                && second_is_in_it
                && refused
                && the_corpus_was_made_solid
                && the_local_body_survives
        },
    );
}

#[test]
fn scenario_two_bodies_cannot_share_one_id() {
    scenario("two_bodies_cannot_share_one_id");
}
