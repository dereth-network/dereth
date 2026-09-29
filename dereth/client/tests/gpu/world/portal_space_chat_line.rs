//! While in the portal tunnel the client prints one fixed line, "In Portal Space - Please Wait...",
//! on chat type `0x1A` with no prefix, on the tunnel's entry frame and at every camera re-aim. Each
//! re-aim draws exactly two random doubles, a duration from `[0.6, 1.8]` and then a target angle
//! from `[0.0, 360.0]`, and fires again once `start + duration <= now`; leaving the tunnel emits
//! nothing. Station: a headless `App` with the gameplay UI, taken through a login tunnel and a held
//! portal tunnel by a synthetic `PlayerCreated`, `Effects_PlayerTeleport 0xF751` (body `[0, 0]`)
//! and a `Movement_PositionEvent 0xF748` carrying only the player id, applied straight to
//! `Teleport::apply_events`. Each line's frame, type, text, prefix and window is compared with a
//! schedule rebuilt on the client's fixed seed 1, and the HUD scroll and spew counters are checked.
//! Fixture: the retail dats; application or GPU setup failures fail the test.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::teleport::{TeleportAnimState, PORTAL_SPACE_CHAT_TYPE, PORTAL_SPACE_MESSAGE};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::num::rng::CrtRand;
use dereth_primitives::ObjectId;
use dereth_protocol::Opcode;

// ---------------------------------------------------------------------------------------------
// The application station.
// ---------------------------------------------------------------------------------------------

/// Retail's random-double formula: `(b - a) * rand16 * 3.051850947599719e-05 + a`.
fn rand_double(r: &mut CrtRand, a: f64, b: f64) -> f64 {
    (b - a) * f64::from(r.next_u16()) * 3.051_850_947_599_719e-5 + a
}

/// Retail's re-aim schedule: the duration formula and draw order. It shares production `CrtRand`
/// and answers only whether this frame raises the line; it is not a separate random
/// implementation. Retail seeds its stream from wall-clock time; the headless client uses 1.
struct RetailRotation {
    rng: CrtRand,
    /// The tracked rotation start time.
    start: f64,
    /// The tracked rotation duration.
    duration: f64,
}

impl RetailRotation {
    /// The headless client's deterministic CRT seed, 1.
    fn new() -> Self {
        Self {
            rng: CrtRand::new(1),
            start: 0.0,
            duration: 0.0,
        }
    }

    /// Reset the two values this schedule tracks; the random stream continues.
    fn begin(&mut self) {
        self.start = 0.0;
        self.duration = 0.0;
    }

    /// Return true when the duration expires and this frame emits a line.
    fn use_time(&mut self, now: f64) -> bool {
        // Retail eases while start + duration > now.
        if self.start + self.duration > now {
            return false;
        }
        self.start = now;
        self.duration = rand_double(&mut self.rng, 0.6, 1.8);
        let _end_angle = rand_double(&mut self.rng, 0.0, 360.0);
        true
    }
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// Start the required headless application, load its static world, and enter gameplay UI. Every
/// setup failure is an assertion failure; this station has no application or GPU skip.
fn app_in_world() -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(base_config()).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        assert!(app.frame());
    }
    app
}

/// One queued chat line with the frame, type, text, prefix, and window observed by the model.
#[derive(Debug, Clone, PartialEq)]
struct Line {
    frame: usize,
    chat_type: u32,
    body: String,
    prefix: Option<String>,
    window: u32,
}

/// Run frames until `stop` answers true (or `max` frames), recording the transcribed schedule and
/// the application's lines. The schedule uses the application's current animation state to decide
/// whether each frame is in a tunnel; it independently predicts only the re-aim timing.
struct Station<'a> {
    app: &'a mut App,
    reference: RetailRotation,
    prev: TeleportAnimState,
    frame: usize,
    expected: Vec<Line>,
    actual: Vec<Line>,
    /// Current clock value for every frame that raised a line, for the report.
    times: Vec<f64>,
}

impl Station<'_> {
    /// Up to `max` frames, stopping early once `stop` answers true for the frame's end state.
    /// Returns whether it stopped early.
    fn run(&mut self, max: usize, stop: impl Fn(TeleportAnimState) -> bool) -> bool {
        for _ in 0..max {
            assert!(
                self.app.frame(),
                "a frame with nothing asking to quit continues the loop"
            );
            self.frame += 1;
            let now = self.app.clock().cur_time;
            let state = self.app.teleport().anim.state;
            // A transition into a tunnel resets the schedule's tracked start and duration.
            if state.is_tunnel() && !self.prev.is_tunnel() {
                self.reference.begin();
            }
            if state.is_tunnel() && self.reference.use_time(now) {
                self.expected.push(Line {
                    frame: self.frame,
                    chat_type: 0x1A,
                    body: "In Portal Space - Please Wait...".to_owned(),
                    prefix: None,
                    window: 0,
                });
                self.times.push(now);
            }
            // The HUD drains a line from the scroll at the top of the next frame.
            for l in self.app.objects().world.scroll.pending() {
                self.actual.push(Line {
                    frame: self.frame,
                    chat_type: l.chat_type,
                    body: l.body.clone(),
                    prefix: l.prefix.clone(),
                    window: l.window,
                });
            }
            self.prev = state;
            if stop(state) {
                return true;
            }
        }
        false
    }
}

/// Behaviour: world.portal-space.a-portal-says-in-portal-space-on-entry-and-every-re-aim
///
/// The fixed portal-space line appears on the entry frame and every predicted re-aim,
/// with exact type, text, prefix, and window. HUD counters and the first spew-model item confirm
/// model delivery; this station does not inspect rendered glyph pixels or strip placement.
#[test]
fn a_portal_says_in_portal_space_on_entry_and_at_every_re_aim_and_the_line_reaches_the_spew_box() {
    let _gpu = gpu_lock();
    let mut app = app_in_world();
    let player = ObjectId(0x5000_0001);
    let idle = app.teleport().anim.state;
    assert_eq!(idle, TeleportAnimState::Off);
    assert!(
        app.objects().world.scroll.pending().is_empty(),
        "nothing queued before the player"
    );

    let mut st = Station {
        app: &mut app,
        reference: RetailRotation::new(),
        prev: idle,
        frame: 0,
        expected: Vec::new(),
        actual: Vec::new(),
        times: Vec::new(),
    };

    // ---- Login tunnel: directly apply a synthetic PlayerCreated event.
    st.app
        .teleport_mut()
        .apply_events(&[SessionEvent::PlayerCreated(player)]);
    st.run(1, |_| true);
    assert_eq!(
        st.app.teleport().anim.state,
        TeleportAnimState::Tunnel,
        "the tunnel is up"
    );
    assert_eq!(
        st.expected.len(),
        1,
        "the schedule predicts the line on the entry frame"
    );
    assert_eq!(
        st.actual, st.expected,
        "the entry frame's line: expected {:?}, got {:?}",
        st.expected, st.actual
    );
    // The next frame drains it into the HUD counters and spew model.
    st.run(1, |_| true);
    assert_eq!(
        st.app.hud().stats.spew_lines,
        1,
        "the HUD spew counter records the first line"
    );
    assert_eq!(
        st.app
            .hud()
            .panels
            .spew
            .model
            .items
            .first()
            .map(String::as_str),
        Some(PORTAL_SPACE_MESSAGE),
        "the first spew-model item contains the line"
    );
    // Let the login settle. The loaded scene lets the current animation unwind through its
    // continue, fade-out, and world-fade-in states.
    assert!(
        st.run(600, |s| s == TeleportAnimState::Off),
        "the login tunnel ends on the world"
    );
    let after_login = st.expected.len();
    assert!(
        after_login >= 2,
        "the login tunnel contributes its entry line and at least one re-aim; got {after_login}"
    );
    assert_eq!(
        st.actual, st.expected,
        "the login tunnel's lines, frame for frame"
    );

    // ---- Portal seam: directly apply an `0xF751` event with the two-byte synthetic body. The
    // tunnel holds until the synthetic player-position event arrives.
    st.app
        .teleport_mut()
        .apply_events(&[SessionEvent::WorldObject {
            opcode: Opcode::EFFECTS_PLAYER_TELEPORT,
            body: vec![0, 0],
        }]);
    st.run(1, |_| true);
    assert_eq!(
        st.app.teleport().anim.state,
        TeleportAnimState::Tunnel,
        "portal space"
    );
    assert_eq!(
        st.expected.len(),
        after_login + 1,
        "the portal's entry frame says it again"
    );
    // Six seconds in portal space, waiting for the server.
    assert!(!st.run(180, |_| false));
    assert_eq!(
        st.app.teleport().anim.state,
        TeleportAnimState::Tunnel,
        "still waiting"
    );
    let held = st.expected.len() - after_login;
    assert!(held >= 4, "the held portal interval contributes its entry line and at least three re-aims; got {held}");
    // Apply `0xF748` with only the player id; the tunnel then unwinds to the world.
    st.app
        .teleport_mut()
        .apply_events(&[SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body: player.0.to_le_bytes().to_vec(),
        }]);
    assert!(
        st.run(600, |s| s == TeleportAnimState::Off),
        "the portal ends on the world"
    );
    assert_eq!(
        st.actual,
        st.expected,
        "every line, frame for frame, text and type -- the schedule from seed 1\n  re-aims at t = {:?}",
        st.times
    );
    let total = st.expected.len();
    assert!(
        total >= after_login + 5,
        "the portal tunnel contributes at least five scheduled lines; got {total}"
    );

    // One more frame checks that no additional line appears after return and drains the last queued
    // line. Counters cover all lines; each model line has the same fixed text and type 0x1A with no
    // prefix or window.
    st.run(1, |_| true);
    assert!(
        st.actual.len() == total,
        "no line appears in the first frame after world return"
    );
    let hud = st.app.hud();
    assert_eq!(
        hud.stats.scroll_lines, total as u64,
        "every line came off the scroll"
    );
    assert_eq!(
        hud.stats.spew_lines, total as u64,
        "and the HUD spew counter records every line"
    );
    assert!(st
        .actual
        .iter()
        .all(|l| l.chat_type == PORTAL_SPACE_CHAT_TYPE
            && l.body == PORTAL_SPACE_MESSAGE
            && l.prefix.is_none()
            && l.window == 0));
    assert_eq!(st.app.teleport().portal_messages, total as u64);
    eprintln!(
        "portal space: {total} portal-space lines over two tunnels; re-aims at t = {:?}",
        st.times
    );
    app.shutdown();
}
