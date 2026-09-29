//! The world clock follows the shard, not this process: a time-sync header in a datagram sets the
//! game clock, so the map's date and time and the sky's day/night cycle read the shard's time
//! rather than a process-local epoch.
//!
//! Absolute game time is timer time + clock offset + the region's time-zero delta; the offset is
//! zero, and the only thing that sets the timer time is the network time-sync handler.
//!
//! Fixture: a headless App on the retail dats in gameplay, with a real `WorldScene` holding a
//! `GameClock` over the shipped region; the date is read out of the live map text element, and
//! the inbound side is driven from real datagram bytes built with `dereth_transport::OutPacket`
//! and `CryptoSystem` and fed through the attached session transport's `feed` method.
//!
//! # The arithmetic, so no assertion here is fitted to the implementation
//!
//! The shipped region (`client_portal.dat` object `0x13000000`) has `zero_time_of_year = 3600.0`,
//! `zero_year = 10`, `day_length = 7620.0`, `days_per_year = 360`, `year_spec = "P.Y."`; seasons
//! begin at day `0, 30, 60, ...`; the 16 times of day begin at `i/16`. The day-boundary
//! calculation uses `y = t + 3600`, `year = floor(y / 2743200) + 10`, and
//! `day = floor((y mod 2743200) / 7620)`; time-of-day selection picks the last `i`
//! with `i/16 <= fraction`.
//!
//! | server time | y = t + 3600 | year | day | fraction | time of day |
//! |---|---|---|---|---|---|
//! | `107080` | `110680` | `0 + 10` | `14` (`110680 - 14*7620 = 4000`) | `4000/7620 = 0.5249` | `8` Midsong |
//! | `114700` | `118300` | `0 + 10` | `15` (`118300 - 15*7620 = 4000`) | `0.5249` | `8` Midsong |
//! | `2850280` | `2853880` | `1 + 10` | `14` (`2853880 - 2743200 = 110680`) | `0.5249` | `8` Midsong |
//!
//! These integer server times and day remainders are exact in `f32` as well as `f64`, so the
//! day-boundary calculation's two narrowings
//! cannot move a boundary under the test. `4000 s` sits in the middle of Midsong's band
//! `[0.5000, 0.5625)`, not on either edge.
//!
//! Unsynchronised timer time is local elapsed seconds -- under `--headless`, `n/30` after
//! `n` frames. `y = 3600 + n/30` stays inside day `0` and inside Morntide-and-Half
//! (`[0.4375, 0.5000)` is `3334.5 s .. 3810 s`) for the first 6,300 frames, so the unsynchronised
//! date is exactly `"Date: Morningthaw 0, 10 P.Y."` / `"Time: Morntide-and-Half"`: the
//! process-local epoch a client without time sync shows.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_primitives::LocalTime;
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// =================================================================================================
// Harness
// =================================================================================================

/// Missing dats fail the test; they never skip it.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

const PEER: &str = "127.0.0.1:19000";

/// The server end of one connection: the ISAAC stream whose output seeds the checksums the client
/// will check, and the sequence counter the datagrams carry.
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
}

impl Peer {
    /// A socket-free `ClientNetwork` with one already-established connection and no fragments:
    /// recipient `0xB`, out seed `0xDEADBEEF`, in seed `0x12345678`.
    fn new() -> (Self, dereth_client::net::ClientNetwork) {
        let mut net =
            dereth_client::net::ClientNetwork::new(PEER, 7304, "f71-station", "unused", 0)
                .expect("a net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some(PEER.parse().expect("addr")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
            },
            net,
        )
    }

    /// **The real datagram.** The time-sync header is an *optional header*, not a fragment: flag
    /// `0x01000000` (`PacketHeaderFlags.TimeSync`), an 8-byte section holding one little-endian
    /// `f64`, sitting in ascending mask order right after the 20-byte `ProtoHeader`. The handler
    /// passes that decoded server-time value to the clock setter.
    ///
    /// Fed through the attached session transport's `feed` method, the receive-parser seam, so the bytes go
    /// through the shipping parser and the shipping checksum, exactly as a shard's would.
    fn send_time_sync(&mut self, app: &mut App, server_time: f64) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::PacketFlags::TIME_SYNC,
                server_time.to_le_bytes().to_vec(),
            )
            .expect("one TimeSync section");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a datagram");
        assert_eq!(
            u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) & 0x0100_0000,
            0x0100_0000,
            "the TimeSync bit is actually set in the header this test built"
        );
        assert_eq!(
            f64::from_le_bytes([
                raw[20], raw[21], raw[22], raw[23], raw[24], raw[25], raw[26], raw[27]
            ]),
            server_time,
            "and the f64 is the only optional section, at datagram offset 20"
        );
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, Some(PEER.parse().expect("addr")), LocalTime(0.0))
            .expect("the shipping parser accepts this datagram");
    }
}

/// A headless `App` on the gameplay screen over a real static scene, with a replay endpoint attached.
///
/// The scene matters: loading it builds `GameClock` from the region, and
/// `WorldScene::use_time_sky` feeds it `cur_time` once per frame, as the world update does.
fn app_in_world() -> (App, Peer) {
    have_dats();
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir().join("dere-f71-time-sync-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("a headless application over the retail dats");
    app.start_shell().expect("the UI shell comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..6 {
        assert!(app.frame());
    }
    let (peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");
    (app, peer)
}

/// **What the player reads.** The live text element behind `0x100001EB`, through
/// `glyphs.inq_text` -- never a model query and never the formatter called directly.
fn rendered_date(app: &mut App) -> String {
    let shell = app.ui_mut().expect("the shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    let g = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    let h = g.map.date_time_text;
    match h.and_then(|h| ui.text_element_mut(h)) {
        Some(t) => t.glyphs.inq_text(false),
        None => String::new(),
    }
}

/// The shared game-clock state read by the sky and landscape lighting:
/// `(current_year, current_day, present_time_of_day)`.
fn sky_state(app: &App) -> (u32, u32, f32) {
    app.world_state().expect("a world scene").game_time()
}

fn pump(app: &mut App, frames: u32) {
    for _ in 0..frames {
        assert!(app.frame());
    }
}

/// Frames to wait after a sync for the live date element to catch up. Two production behaviors
/// account for the delay:
///
/// * The map panel refreshes at most once every five seconds of timer time.
/// * In the current frame, network synchronization precedes `ui_use_time`, but the HUD's calendar
///   snapshot is read there before `WorldScene::update` refreshes the game clock. Thus a panel
///   refresh on the sync frame can still read the prior calendar and wait for its next refresh.
///
/// `5.0 / (1/30) = 150` frames, plus a margin for frame ordering. On a live shard this is simply
/// "the next five seconds"; ACE resends `TimeSync` every 20 s (`NetworkSession.timeBetweenTimeSync`).
const REDRAW_FRAMES: u32 = 154;

/// With no sync, timer time is this process's elapsed seconds and the date uses that epoch.
const EPOCH_DATE: &str = "Date: Morningthaw 0, 10 P.Y.\nTime: Morntide-and-Half";

/// Shard time `107080` -- see the table in the module doc.
const DAY_14: f64 = 107_080.0;
const DAY_14_DATE: &str = "Date: Morningthaw 14, 10 P.Y.\nTime: Midsong";
/// `DAY_14 + 7620`, one Derethian day later.
const DAY_15: f64 = 114_700.0;
const DAY_15_DATE: &str = "Date: Morningthaw 15, 10 P.Y.\nTime: Midsong";
/// `DAY_14 + 2_743_200`, one Derethian year later.
const YEAR_11: f64 = 2_850_280.0;
const YEAR_11_DATE: &str = "Date: Morningthaw 14, 11 P.Y.\nTime: Midsong";

/// The `REDRAW_FRAMES` of `cur_time` this adds are `154/30 = 5.13 s` out of a `7620 s` day, i.e.
/// `0.00067` of the day -- far inside one of the sixteen `476.25 s` bands, so every date in the
/// table above survives the wait.
fn sync_and_redraw(app: &mut App, peer: &mut Peer, server_time: f64) {
    peer.send_time_sync(app, server_time);
    pump(app, REDRAW_FRAMES);
}

// =================================================================================================
// 0. The premise: unsynchronised, the date is a process-local epoch
// =================================================================================================

/// The premise of the tests below: without a sync the date is the process-local epoch, so their
/// failure text names that rather than just a mismatch.
#[test]
fn with_no_time_sync_the_date_and_the_sky_run_on_this_process_s_epoch() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = app_in_world();
    pump(&mut app, 4);

    assert_eq!(
        rendered_date(&mut app),
        EPOCH_DATE,
        "the unsynchronised date"
    );
    assert_eq!(
        app.clock().external_offset(),
        0.0,
        "the external offset starts at zero and changes only through time synchronization"
    );
    let (year, day, fraction) = sky_state(&app);
    assert_eq!((year, day), (10, 0), "zero_year + 0 years, day 0");
    assert!(
        (0.4375..0.5).contains(&fraction),
        "present_time_of_day {fraction} is Morntide-and-Half's band, i.e. 3600 s into day zero"
    );
}

// =================================================================================================
// 1. One real TimeSync datagram moves the date the player reads
// =================================================================================================

/// Behaviour: presentation.clock.a-time-sync-puts-the-date-and-sky-on-the-shards-clock
#[test]
fn one_time_sync_datagram_puts_the_map_s_date_on_the_shard_s_clock() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);
    let before = rendered_date(&mut app);
    assert_eq!(
        before, EPOCH_DATE,
        "the clock starts on this process's epoch"
    );

    sync_and_redraw(&mut app, &mut peer, DAY_14);

    let after = rendered_date(&mut app);
    assert_eq!(after, DAY_14_DATE, "the date element after one TimeSync");
    assert_ne!(after, before, "and it is not what it was");
}

/// The sky, not just the string: sky rendering and lighting read the shared day fraction, and
/// `4000/7620` falls in Midsong. Unlike the date element, the game-clock update is unthrottled
/// and runs every frame.
#[test]
fn the_same_datagram_moves_the_day_night_cycle() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);
    let (_, _, before) = sky_state(&app);

    peer.send_time_sync(&mut app, DAY_14);
    pump(&mut app, 2);

    let (year, day, fraction) = sky_state(&app);
    assert_eq!(
        (year, day),
        (10, 14),
        "day-boundary calculation over the synchronised absolute time"
    );
    assert!(
        (fraction - 4000.0 / 7620.0).abs() < 1e-4,
        "present_time_of_day is {fraction}, want 4000/7620 = 0.524934"
    );
    assert!(
        (before - fraction).abs() > 0.05,
        "and it moved: {before} -> {fraction}"
    );
}

/// Behaviour: map.page.the-date-and-the-time-are-drawn-on-the-page
///
/// A day and then a year, to show the date tracks the shard rather than latching once.
#[test]
fn successive_syncs_roll_the_day_and_then_the_year() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);

    sync_and_redraw(&mut app, &mut peer, DAY_14);
    assert_eq!(rendered_date(&mut app), DAY_14_DATE);

    sync_and_redraw(&mut app, &mut peer, DAY_15);
    assert_eq!(
        rendered_date(&mut app),
        DAY_15_DATE,
        "one Derethian day later"
    );

    sync_and_redraw(&mut app, &mut peer, YEAR_11);
    assert_eq!(
        rendered_date(&mut app),
        YEAR_11_DATE,
        "one Derethian year later"
    );
}

// =================================================================================================
// 2. The clock synchronization contract
// =================================================================================================

/// **The forward-only gate, on the date the player reads.**
///
/// Synchronization accepts only a time strictly greater than the external-time watermark; a
/// lesser, equal or unordered comparison skips correction and reaches only the publishing stores.
/// The setter is not an unconditional hard-set; if it were, this date would fall back a year and
/// the sky with it.
///
/// **The clock is asserted as well as the string, deliberately.** An unchanged element could equally
/// mean "the redraw was throttled", so the stale sync is followed by the full `REDRAW_FRAMES` wait
/// and `App::clock` is read directly. Those are two different builds and the string alone cannot
/// separate them.
#[test]
fn a_stale_sync_is_rejected_and_the_date_does_not_go_backwards() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);

    sync_and_redraw(&mut app, &mut peer, YEAR_11);
    assert_eq!(rendered_date(&mut app), YEAR_11_DATE);
    let offset = app.clock().external_offset();

    // The shard's own 20-second resend arriving out of order, or a second server's clock.
    sync_and_redraw(&mut app, &mut peer, DAY_14);

    assert!(
        app.clock().cur_time >= YEAR_11,
        "cur_time is {}, behind the last accepted sync {YEAR_11}: the strictly-forward synchronization gate was not preserved",
        app.clock().cur_time
    );
    assert_eq!(
        app.clock().external_offset(),
        offset,
        "a rejected sync must not touch the external offset"
    );
    assert_eq!(
        rendered_date(&mut app),
        YEAR_11_DATE,
        "still the synchronised year, redrawn"
    );
}

/// **The `1e-09` dead band.** A sync that clears the
/// forward gate by less than a nanosecond is accepted and then corrects nothing: both of the two
/// branches are skipped and only the publishing stores run.
#[test]
fn a_sync_inside_the_nanosecond_dead_band_changes_no_offset() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);
    peer.send_time_sync(&mut app, DAY_14);
    pump(&mut app, 1);
    let offset = app.clock().external_offset();
    let cur = app.clock().cur_time;
    // Without this the whole test passes vacuously on a build that applies no sync at all: both
    // offsets would read 0.0 and be equal.
    assert!(
        offset > 1.0,
        "the first sync has to have taken for the dead band to mean anything"
    );

    // `cur + 1e-10`: above the external-time watermark so the gate lets it through, inside `1e-09` so neither
    // correction fires. The offset must come out bit-identical.
    peer.send_time_sync(&mut app, cur + 1e-10);
    pump(&mut app, 1);
    assert_eq!(
        app.clock().external_offset(),
        offset,
        "a sync 1e-10 ahead is inside the dead band and must leave the external offset alone"
    );
}

// =================================================================================================
// 3. The constraint: a monotonic local clock *and* a server-synchronised absolute time
// =================================================================================================

/// **How the client keeps both, and why a sync cannot break a fixed-step test.**
///
/// Forward correction writes the external offset and external time but **does not
/// alter local elapsed time**. External time = local elapsed time + external offset, and the
/// offset starts at `0.0`. So:
///
/// * with no sync, `cur_time == elapsed_time` and the fixed step is the whole clock;
/// * with a sync, the fixed step still drives the clock and the offset is an additive constant.
///
/// Between corrections, `cur_time` gains only that additive offset. With no sync the offset remains zero,
/// preserving the process-local clock used by offline headless capture and fixed-step suites.
#[test]
fn a_sync_shifts_the_absolute_time_by_a_constant_and_never_the_local_step() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);

    let local_before = app.clock().elapsed_time();
    peer.send_time_sync(&mut app, DAY_14);

    // The sync is drained after this frame's local timer update, as the client orders them,
    // so the arrival frame ends with `cur_time` exactly `DAY_14`.
    pump(&mut app, 1);
    assert_eq!(
        app.clock().cur_time,
        DAY_14,
        "the frame the sync lands on ends exactly on it"
    );

    const N: u32 = 9;
    pump(&mut app, N);
    let step = dereth_client::app::HEADLESS_STEP;

    assert!(
        (app.clock().elapsed_time() - (local_before + step * f64::from(N + 1))).abs() < 1e-9,
        "the local watermark advanced by exactly {} fixed steps and nothing else: {} vs {}",
        N + 1,
        app.clock().elapsed_time(),
        local_before + step * f64::from(N + 1)
    );
    assert!(
        (app.clock().cur_time - (DAY_14 + step * f64::from(N))).abs() < 1e-6,
        "the absolute time is the shard's plus the same {N} steps: {} vs {}",
        app.clock().cur_time,
        DAY_14 + step * f64::from(N)
    );
    assert!(
        (app.clock().external_offset() - (DAY_14 - (local_before + step))).abs() < 1e-9,
        "external offset = server time - local elapsed time, computed once, at the sync"
    );
}

/// The other half of the same constraint: with no network, the clock is still exactly the step.
#[test]
fn with_nothing_on_the_wire_the_clock_is_still_the_fixed_step_alone() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = app_in_world();
    let base = app.clock().cur_time;
    const N: u32 = 12;
    pump(&mut app, N);

    assert!(
        (app.clock().cur_time - (base + dereth_client::app::HEADLESS_STEP * f64::from(N))).abs()
            < 1e-12,
        "cur_time is {} after {N} headless frames from {base}",
        app.clock().cur_time
    );
    assert_eq!(
        app.clock().external_offset(),
        0.0,
        "and no offset was invented"
    );
}

// =================================================================================================
// 4. A real shard value, out of a recorded session
// =================================================================================================

/// **The tests above use times from ~1e5 to ~3e6; a real shard sends ~3e8, a different
/// arithmetic regime.** `fixtures/packet-captures/first-login-walk-jump.jsonl` line 4 is an `s2c` datagram with
/// `header_ = 0x01000002` (`TimeSync | BlobFragments`) whose optional section holds
/// `0x41B2090440C193DC` = `302580800.756162` -- ACE's `Timers.PortalYearTicks`, seconds of Derethian
/// time (`NetworkSession.cs:969`), resent every 20 s. At `3e8` an `f32` has a quantum of **32**, and
/// the day-boundary calculation narrows twice: `(float)t` and the `float` `rem`. So
/// `time_of_day_begin` is quantised to 32-second steps, which is retail's own behaviour and must not
/// be "fixed"; this is the only test that exercises it.
///
/// Worked through by hand from the region constants, not read off this build:
/// `y = 302584400.756`, `years = floor(y / 2743200) = 110` so the year is `110 + 10 = 120`;
/// `rem = 832400.75` (exact in `f32`, being under 2^24), `day = floor(832400.75 / 7620) = 109`;
/// `seasons` puts day 109 in `Leafdawning` (begins 90, next begins 120);
/// `(float)302580800.756 = 302580800` and `302580800 - 1820.75` rounds to `302578976`, so
/// `fraction = 1824.756 / 7620 = 0.2395`, which is `Foredawn-and-Half`'s band `[0.1875, 0.25)`.
const SHARD_CAPTURE: f64 = 302_580_800.756_162;
const SHARD_CAPTURE_DATE: &str = "Date: Leafdawning 109, 120 P.Y.\nTime: Foredawn-and-Half";

#[test]
fn a_timesync_value_captured_from_the_owners_own_shard_renders_its_real_date() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = app_in_world();
    pump(&mut app, 2);

    sync_and_redraw(&mut app, &mut peer, SHARD_CAPTURE);

    // The year and the day come out of `rem`, which is exact in `f32`: these cannot be moved by the
    // 32-second quantisation and are the load-bearing half.
    let (year, day, _) = sky_state(&app);
    assert_eq!(
        (year, day),
        (120, 109),
        "a real shard date, not year 10 of a process epoch"
    );
    assert_eq!(rendered_date(&mut app), SHARD_CAPTURE_DATE);

    // One Derethian day on, from the same base: the day rolls by exactly one and stays in the month.
    sync_and_redraw(&mut app, &mut peer, SHARD_CAPTURE + 7620.0);
    let (year, day, _) = sky_state(&app);
    assert_eq!((year, day), (120, 110), "exactly one day later");
    assert_eq!(
        rendered_date(&mut app),
        "Date: Leafdawning 110, 120 P.Y.\nTime: Foredawn-and-Half"
    );
}
