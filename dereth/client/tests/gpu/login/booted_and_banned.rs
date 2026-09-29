//! A booted or banned account reaches the disconnected screen with its own sentence, and the process
//! stays up. `Login_AccountBooted 0xF7DC` and `Login_AccountBanned 0xF7C1` become
//! `SessionEvent::AccountBooted` / `AccountBanned`, which `App::recv_disconnect_notice` turns into
//! the disconnected screen. The sentences are literal formats, not string-table tokens:
//! `You have been booted from Asheron's Call%s.` (an absent reason is ` for Code of Conduct
//! Violations`), `You have been banned until %s%s. For ban appeals, please visit
//! support.turbine.com` (expiry > 0: current Unix time rounded up to the minute plus the duration),
//! and `You have been banned from Asheron's Call%s.` (expiry <= 0). A clean exit still reaches the
//! epilogue. Fixture: the `login-account-booted` recording replayed through `ClientNetwork`;
//! constructed ban events; a headless linked App aimed at an unused loopback port.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::{App, AppState};
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::LocalTime;
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::disconnected::{self, DisconnectedScreen};
use dereth_ui_screens::screens::epilogue::EpilogueScreen;
use dereth_ui_screens::screens::gameplay::logout;

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// Enable the link path using loopback port 9000, intended to have no listener. This makes
/// App::link present and selects build_host_state's link arm; no remote host is configured.
fn connected_config() -> Config {
    Config {
        connect: true,
        account: "o100".into(),
        host: "127.0.0.1".into(),
        port: 9000,
        ..base_config()
    }
}

fn app_on(cfg: Config, want: dereth_ui::UiMode) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} (set DERETH_TEST_DAT_DIR)",
        client_dir().display()
    );
    let mut app = App::new(cfg).expect("the application comes up headless");
    app.start_shell().expect("the UI shell comes up");
    app.load_first_pixel_scene()
        .expect("the first-pixel scene loads");
    if want != mode::DATA_PATCH {
        app.queue_ui_mode(want);
    }
    step_to(&mut app, want, 30);
    app
}

fn step_to(app: &mut App, want: dereth_ui::UiMode, limit: u32) {
    for _ in 0..limit {
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(want) {
            return;
        }
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::INTRO) {
            let root = app
                .ui()
                .and_then(|u| u.flow.current())
                .and_then(|s| s.roots().first().copied());
            if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                shell.ui.broadcast_element_message(
                    root,
                    dereth_ui_screens::screens::intro::MSG_SKIP,
                    0,
                    0,
                );
            }
        }
        app.frame();
    }
    panic!(
        "the flow did not reach {want:?}: {:?}",
        app.ui().and_then(|u| u.flow.current_mode())
    );
}

/// A linked application in gameplay mode, without loading the world or character.
fn app_in_game() -> App {
    app_on(connected_config(), mode::GAME_PLAY)
}

fn screen<T: 'static>(app: &mut App) -> &mut T {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<T>().expect("the expected screen")
}

fn current_mode(app: &App) -> Option<dereth_ui::UiMode> {
    app.ui().and_then(|u| u.flow.current_mode())
}

/// Deliver through the application's session-event consumer instead of writing HostState.
fn deliver(app: &mut App, e: SessionEvent) {
    app.process_logon_event_queue(vec![e]);
}

/// Broadcast the button-release message 1 on an element found in the live tree.
fn click(app: &mut App, id: dereth_ui::ElementId) {
    let shell = app.ui_mut().expect("shell");
    let h = shell
        .ui
        .get_element(id)
        .unwrap_or_else(|| panic!("{:#X} is in the shipped layout", id.0));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
}

/// The shipped English row for a string id in table enum `0x10000002`, or None on a miss.
/// Resolve through the shell's two-level layout-to-data-id mapping, not a hardcoded data id.
///
/// Returning `Option` rather than panicking lets the tests prove a **miss** as well as a hit.
fn shipped_row(app: &App, id: &str) -> Option<String> {
    use dereth_ui::framework::LayoutEnumResolver as _;
    let table = dereth_ui::framework::DidMapperResolver::load_group(
        app.assets(),
        dereth_ui::framework::STRING_TABLE_GROUP,
    )
    .expect("the string-table group resolves")
    .resolve(dereth_ui::framework::LayoutEnum(
        dereth_client::ui::PATCH_STRING_TABLE_ENUM,
    ))
    .expect("string-table enum 0x10000002 resolves to a shipped table");
    app.ui()
        .expect("the shell is up")
        .ui
        .resolve_string(table, dereth_primitives::num::hash::str_hash(id.as_bytes()))
}

// ---------------------------------------------------------------------------------------------
// The capture oracle — the corpus's one real `0xF7DC`.
// ---------------------------------------------------------------------------------------------

/// Replay one recording through the application's own `ClientNetwork` → `ObjectStream::pump` path and
/// return every `SessionEvent` it produced, in order.
fn replay_events(session: &str) -> Vec<SessionEvent> {
    let records = shared_session(session);
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn)
        .expect("a replay client network");
    let mut objects = ObjectStream::new();
    let mut out = Vec::new();
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(r.t);
        net.feed(&r.raw, r.peer(), now);
        net.tick(now);
        let _ = net.take_outgoing();
        out.extend(objects.pump(&mut net, now));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// 1. The boot, against a real recording
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.disconnect.a-boot-shows-the-servers-own-reason
///
/// The recording supplies the reason; the client's format supplies the sentence.
/// `login-account-booted` carries one boot event rejecting a password. Replay through ClientNetwork and
/// ObjectStream::pump produces SessionEvent::AccountBooted, delivered to
/// App::process_logon_event_queue. The formatting result and displayed sentence must agree.
/// This is explicit event injection after independent replay, not a live server or App::frame
/// feeding those recorded datagrams itself.
///
/// Showing the Code-of-Conduct default here fails.
#[test]
fn the_corpus_0xf7dc_reaches_the_screen_with_the_servers_own_reason() {
    let _gpu = gpu_lock();

    let events = replay_events("login-account-booted");
    let booted: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::AccountBooted(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !booted.is_empty(),
        "login-account-booted carries a 0xF7DC and the replay surfaced none"
    );
    assert!(
        booted.windows(2).all(|w| w[0] == w[1]),
        "every recorded 0xF7DC carries the same reason: {booted:?}"
    );
    let reason = booted[0]
        .clone()
        .expect("the recorded 0xF7DC carries a reason string");
    assert_eq!(
        reason,
        " because the password entered for this account was not correct"
    );

    let expected = disconnection_sentence(&reason);
    assert_eq!(
        expected,
        "You have been booted from Asheron's Call because the password entered for this account \
         was not correct."
    );
    assert_ne!(
        expected,
        disconnected::account_booted_message(None),
        "the recorded reason is not the Code-of-Conduct default"
    );

    let mut app = app_in_game();
    deliver(&mut app, SessionEvent::AccountBooted(Some(reason)));
    for _ in 0..3 {
        assert!(app.frame(), "the loop must not end on a boot");
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    assert_eq!(
        screen::<DisconnectedScreen>(&mut app).shown_text.as_deref(),
        Some(expected.as_str()),
        "the screen shows the server's own reason inside the client's own sentence"
    );
}

fn disconnection_sentence(reason: &str) -> String {
    disconnected::account_booted_message(Some(reason))
}

/// Boot formatting uses the Code-of-Conduct suffix when no reason or an empty reason
/// is supplied. This integration case exercises the absent-reason event; the formatting unit
/// tests cover the empty-string branch. The recorded nonempty reason is tested above.
#[test]
fn a_boot_with_no_reason_shows_the_code_of_conduct_default() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    deliver(&mut app, SessionEvent::AccountBooted(None));
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    assert_eq!(
        screen::<DisconnectedScreen>(&mut app).shown_text.as_deref(),
        Some("You have been booted from Asheron's Call for Code of Conduct Violations.")
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The ban, and the discrimination that a "did a screen appear" test cannot see
// ---------------------------------------------------------------------------------------------

/// Four conditions reach this one screen and three of them would look
/// identical to a test that only asked whether it came up. Each is driven into its own `App` and
/// the sentence is compared with the other three, so a build that shows one message for all of
/// them — or the generic connection-lost text where retail shows the ban text — fails.
#[test]
fn each_condition_shows_its_own_sentence_and_not_another_conditions() {
    let _gpu = gpu_lock();

    let shown = |e: SessionEvent| -> String {
        let mut app = app_in_game();
        deliver(&mut app, e);
        for _ in 0..3 {
            assert!(app.frame());
        }
        assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
        screen::<DisconnectedScreen>(&mut app)
            .shown_text
            .clone()
            .expect("a sentence")
    };

    let boot = shown(SessionEvent::AccountBooted(None));
    let permanent = shown(SessionEvent::AccountBanned {
        expiry: 0,
        reason: " - griefing".to_string(),
    });
    let timed = shown(SessionEvent::AccountBanned {
        expiry: 3_600,
        reason: " - griefing".to_string(),
    });
    let char_error = shown(SessionEvent::CharacterError(1));

    // Each is the sentence its own handler builds.
    assert_eq!(
        boot,
        "You have been booted from Asheron's Call for Code of Conduct Violations."
    );
    assert_eq!(
        permanent,
        "You have been banned from Asheron's Call - griefing."
    );
    assert!(timed.starts_with("You have been banned until "), "{timed}");
    assert!(
        timed.ends_with(" - griefing. For ban appeals, please visit support.turbine.com"),
        "{timed}"
    );
    assert!(
        char_error.starts_with("Cannot have two accounts"),
        "{char_error}"
    );

    // And no two of the four are the same: handling one and dropping three fails.
    let all = [&boot, &permanent, &timed, &char_error];
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            if i != j {
                assert_ne!(
                    a, b,
                    "condition {i} and condition {j} show the same sentence"
                );
            }
        }
    }
}

/// Ban formatting distinguishes permanent expiry<=0 from a positive duration. The latter
/// rounds the current Unix time up to a minute before adding the duration.
///
/// This integration check enumerates the seconds between its before/after clock reads and uses
/// the current formatter for each admissible result. It checks propagation and timing, not an
/// independent reimplementation of that formatter's arithmetic.
#[test]
fn a_timed_ban_names_the_expiry_and_a_permanent_one_names_no_date() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    let before = unix_now();
    deliver(
        &mut app,
        SessionEvent::AccountBanned {
            expiry: 7_200,
            reason: " - testing".to_string(),
        },
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let after = unix_now();
    let shown = screen::<DisconnectedScreen>(&mut app)
        .shown_text
        .clone()
        .expect("a sentence");

    // The expiry is rendered in the host's UTC offset at the expiry instant. Build the admissible
    // set the same way. The disconnected-screen module's unit
    // tests separately exercise explicit offsets; this test reads the host's actual zone.
    let admissible: Vec<String> = (before..=after)
        .map(|now| {
            let at = disconnected::ban_expiry_epoch(7_200, now);
            let off = dereth_client::platform::local_utc_offset_secs(at);
            disconnected::account_banned_message(7_200, " - testing", now, off)
        })
        .collect();
    assert!(
        admissible.iter().any(|a| *a == shown),
        "{shown:?} is not one of {} admissible renderings for a clock in [{before}, {after}]",
        admissible.len()
    );
    assert!(
        shown.contains("For ban appeals, please visit support.turbine.com"),
        "{shown}"
    );

    // Permanent bans name no instant at all, which is the discrimination `expiry <= 0` makes.
    let mut app = app_in_game();
    deliver(
        &mut app,
        SessionEvent::AccountBanned {
            expiry: 0,
            reason: String::new(),
        },
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let permanent = screen::<DisconnectedScreen>(&mut app)
        .shown_text
        .clone()
        .expect("a sentence");
    assert_eq!(permanent, "You have been banned from Asheron's Call.");
    assert!(!permanent.contains("until"), "{permanent}");
    assert!(!permanent.contains("ban appeals"), "{permanent}");
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// **These sentences are literals, not tokens**, and this is the assertion that proves the
/// distinction is real rather than assumed.
///
/// The boot and ban handlers pass literal text. The shell hashes a candidate string and looks it up in
/// table enum `0x10000002`, passing it through unchanged on a miss. Check that these four sample
/// sentences have no row: a collision could substitute unrelated localized text. This does not
/// exhaust every possible server-provided reason.
///
/// Both directions, so the instrument cannot be reading *nothing*: a char-error token must still
/// resolve to its own row.
#[test]
fn the_booted_and_banned_sentences_are_not_rows_of_the_shipped_string_table() {
    let _gpu = gpu_lock();
    let app = app_in_game();

    // The instrument can see: a real token resolves.
    let logon = shipped_row(&app, "ID_CHAR_ERROR_LOGON").expect("a character-error token is a row");
    assert!(logon.starts_with("Cannot have two accounts"), "{logon}");

    for sentence in [
        disconnected::account_booted_message(None),
        disconnected::account_booted_message(Some(" because of something the server said")),
        disconnected::account_banned_message(0, " - griefing", 1_756_000_000, 0),
        disconnected::account_banned_message(3_600, " - griefing", 1_756_000_000, 0),
    ] {
        assert_eq!(
            shipped_row(&app, &sentence),
            None,
            "{sentence:?} collides with a row of table 0x10000002"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 3. The process does not exit, and a clean exit is untouched
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.disconnect.a-boot-keeps-the-process-up-until-the-player-asks
///
/// A boot opens a screen and keeps the process alive for thirty checked frames. Clicking its
/// OK element (`0x10000418`, message 1) then reaches the epilogue with the loop still running;
/// this test does not request or assert process termination.
#[test]
fn the_process_stays_up_on_a_boot_and_ends_only_when_the_player_asks() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();
    assert!(app.frame());
    let (w, h, before) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");

    deliver(&mut app, SessionEvent::AccountBooted(None));
    for i in 0..30 {
        assert!(
            app.frame(),
            "frame {i}: the loop ended on its own after a boot"
        );
        assert_ne!(app.state(), AppState::ShuttingDown, "frame {i}");
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    assert!(
        !app.ui().expect("shell").stats.device_done,
        "nothing has asked to quit yet"
    );

    // The disconnected screen has a full-screen root image. Compare before/after RGB pixels
    // and require more than half to change, proving visible output rather than only a flow node.
    // This is a whole-frame differential, not an isolated screen rectangle.
    let (w2, h2, after) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");
    assert_eq!((w, h), (w2, h2));
    let changed = (0..(w * h) as usize)
        .filter(|i| before[i * 4..i * 4 + 3] != after[i * 4..i * 4 + 3])
        .count();
    let total = (w * h) as usize;
    eprintln!("booted: {changed} of {total} pixels changed when the disconnect screen came up");
    assert!(
        changed * 2 > total,
        "only {changed} of {total} pixels changed — nothing was drawn"
    );

    let ok = app
        .ui_mut()
        .expect("shell")
        .ui
        .get_element(disconnected::OK_BUTTON)
        .expect("the OK button is in the shipped layout");
    app.ui_mut().expect("shell").ui.broadcast_element_message(
        ok,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    assert!(
        app.frame(),
        "the OK click switches modes; it does not exit the process"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::EPILOGUE),
        "OK goes to the epilogue, never to exit"
    );
    let _ = screen::<EpilogueScreen>(&mut app);
}

/// The direction a one-sided test misses: **a clean logout must not land here.**
///
/// Two clean-exit directions share this integration:
/// * Exit Game (`0x10000617`) reaches the epilogue without a disconnect error.
/// * The server's orderly `0xF653 Login_ExecuteLogOff` event leaves no error and must not put the
///   flow in the disconnected screen. That second arm does not assert an epilogue transition.
/// Treating every session end as a boot would incorrectly show the new error screen.
#[test]
fn a_clean_logout_reaches_the_epilogue_and_never_the_disconnect_screen() {
    let _gpu = gpu_lock();

    // (a) The player asks to quit.
    let mut app = app_in_game();
    click(&mut app, logout::EXIT_GAME);
    assert!(app.frame());
    assert_eq!(
        current_mode(&app),
        Some(mode::EPILOGUE),
        "Exit Game goes to the quit screen"
    );
    assert!(
        app.host_state().error.is_none(),
        "a clean exit leaves no disconnect StringInfo"
    );
    let _ = screen::<EpilogueScreen>(&mut app);

    // (b) The server acknowledges the log-off. `0xF653` is the orderly end and is **not** a boot
    // or ban; `recv_disconnect_notice` must fall through it.
    let mut app = app_in_game();
    deliver(&mut app, SessionEvent::LoggedOff);
    assert!(
        app.host_state().error.is_none(),
        "0xF653 raised no disconnect StringInfo"
    );
    assert_ne!(
        current_mode(&app),
        Some(mode::DISCONNECTED),
        "an orderly log-off is not a disconnect"
    );
}
