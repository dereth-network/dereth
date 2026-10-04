//! Leaving the game, and being put out of it. A server-side disconnect lands on the disconnect
//! screen showing the server's own reason (the shipped string table behind enum `0x10000002`,
//! resolved through the same two-level mapper lookup the shell uses); the character-error codes 0,
//! 2, 7 and 22 queue no mode; a server death shows `ID_NetErr_ConnectionLost`; the reason survives
//! the frame that raised it with a link up; and the process stays up until the player presses OK
//! (`0x10000418`, message 1), which goes to the epilogue and ends the loop on the following check.
//! From inside the world, Exit to Character Selection raises a modal confirmation, No leaves the
//! player in the world, Yes asks to log off without quitting, and Exit Game quits without asking.
//! Fixture: the retail dats, a headless `App` with a link pointed at a port nobody listens on, and
//! clicks broadcast through the live UI tree to elements of the shipped layout.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::{App, AppState};
use dereth_client::config::Config;
use dereth_client_net::client_session::{DisconnectReason, SessionEvent, SessionState};
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::disconnected::{self, DisconnectedScreen};
use dereth_ui_screens::screens::epilogue::EpilogueScreen;

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

/// `--connect`, pointed at a host with **nothing listening**.
///
/// The socket binds to `0.0.0.0` on an ephemeral port and the `LoginRequest` goes nowhere; no
/// server is contacted. `App::link` is `Some`, so `build_host_state` takes its link arm and
/// `process_logon_event_queue` runs its event loop instead of returning at the guard. A linkless
/// `App` exercises neither.
fn connected_config() -> Config {
    Config {
        connect: true,
        account: "disconnect".into(),
        host: "127.0.0.1".into(),
        port: 9000,
        ..base_config()
    }
}

/// Bring an `App` up on `want`, dismissing the intro the way a player does.
///
/// Every step is an `expect`: a missing dat or a device that will not come up is a **failure**
/// here, not a silent pass.
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
        // The data-patch screen hands over on its own only when `0xF658` has arrived, which needs a
        // server; the player's own route to any later screen is what `queue_ui_mode` is.
        app.queue_ui_mode(want);
    }
    step_to(&mut app, want, 30);
    app
}

/// Frame until `want` is up, skipping the intro through its own skip message.
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

/// An application standing in the world with a link, which is where a disconnect finds a player.
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

/// The English text behind a reason token in table enum `0x10000002`, resolved **independently of
/// the shell** through the same two-level mapper lookup: string-table group `0x2500000E`, then the
/// enum. The table id is never hard-coded because a data patch can move a string table just as it
/// can move a layout.
fn shipped_reason(app: &App, id: &str) -> String {
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
        .unwrap_or_else(|| panic!("{id} is a row of the shipped table"))
}

/// Raise a disconnect through **the application's own login-event queue**, rather than by writing
/// `HostState` directly.
fn deliver(app: &mut App, e: SessionEvent) {
    app.process_logon_event_queue(vec![e]);
}

// ---------------------------------------------------------------------------------------------
// 1. A disconnect lands on the screen, with the server's own reason
// ---------------------------------------------------------------------------------------------

/// Oracle: the character-error notice queues disconnected mode with string-table enum
/// `0x10000002`; error row 1 names `ID_CHAR_ERROR_LOGON`, the retail string *"Cannot have two
/// accounts logged on at the same time."*
///
/// **Full rigour on which reason is shown**, because showing the wrong one is invisible and
/// misleading: the text on the screen is compared against the shipped table's row for the token
/// the code names, *and* against the row for a different code, so a build that shows a constant
/// string cannot pass.
#[test]
fn a_character_error_shows_the_servers_own_reason_and_not_another_codes() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    let logon = shipped_reason(&app, "ID_CHAR_ERROR_LOGON");
    let crash = shipped_reason(&app, "ID_CHAR_ERROR_SERVER_CRASH");
    assert_ne!(
        logon, crash,
        "the oracle itself must distinguish the two codes"
    );
    assert!(logon.starts_with("Cannot have two accounts"), "{logon:?}");

    deliver(&mut app, SessionEvent::CharacterError(1));
    // One frame carries the notice into the flow and performs the switch; the second applies the
    // pending error text and draws.
    for _ in 0..3 {
        assert!(app.frame(), "the loop must not end on a disconnect");
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));

    let shown = screen::<DisconnectedScreen>(&mut app).shown_text.clone();
    assert_eq!(
        shown.as_deref(),
        Some(logon.as_str()),
        "the screen shows character error 1's own row"
    );
    assert_ne!(shown.as_deref(), Some(crash.as_str()));
}

/// Oracle: the server-death notice has exactly one string, `ID_NetErr_ConnectionLost`, whose retail
/// text is *"The connection to the server has been lost!"*. The session decides the server died
/// itself after either the 110-second world-entry timeout or 40-second heartbeat gap, so this
/// arrives as a state change rather than a character error.
#[test]
fn a_server_died_disconnect_shows_the_connection_lost_string() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    let lost = shipped_reason(&app, disconnected::SERVER_DIED_STRING_ID);
    assert_eq!(lost, "The connection to the server has been lost!");

    deliver(
        &mut app,
        SessionEvent::StateChanged(SessionState::Disconnected(DisconnectReason::ServerDied)),
    );
    for _ in 0..3 {
        assert!(app.frame(), "the loop must not end on a disconnect");
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    assert_eq!(
        screen::<DisconnectedScreen>(&mut app).shown_text.as_deref(),
        Some(lost.as_str()),
        "the reason is the server-died string, not a character-error row"
    );
}

/// Oracle: the same character-error table — the four codes with no token (0 `UNDEF`, 2 `LOGGED_ON`,
/// 7 `NO_PREMADE`, 22 `CHARACTER_IS_BOOTED`) leave the reason string untouched and **never queue a
/// mode**; the client ignores them.
///
/// The other direction of the reason test: routing every code would put a **blank** disconnect
/// screen in front of a player who should have seen nothing at all.
#[test]
fn the_four_codes_with_no_token_do_not_raise_the_screen_at_all() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    for code in [0_u32, 2, 7, 22] {
        assert_eq!(
            disconnected::character_error_string_id(code),
            None,
            "{code} has no token"
        );
        deliver(&mut app, SessionEvent::CharacterError(code));
        for _ in 0..2 {
            assert!(app.frame());
        }
        assert_eq!(
            current_mode(&app),
            Some(mode::GAME_PLAY),
            "character error {code} must not queue a mode"
        );
        assert!(
            app.host_state().error.is_none(),
            "character error {code} left no reason string"
        );
    }

    // …and a code that *does* carry one still works afterwards, so the loop above is not passing
    // because the whole path is dead.
    deliver(&mut app, SessionEvent::CharacterError(21));
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
}

// ---------------------------------------------------------------------------------------------
// 2. The process does not exit
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.disconnect.shows-the-servers-reason-and-the-process-stays-until-quit
///
/// **The claim that matters, asserted explicitly**: a test that only checked "the screen was
/// requested" would pass on a build that requests it and then dies.
///
/// Oracle: the quit path goes gameplay → epilogue → device done, never straight to process exit;
/// the disconnected screen has exactly that one exit path.
/// Both directions: the loop stays alive for as long as nobody presses the button, and it ends
/// when somebody does.
#[test]
fn the_process_stays_up_on_a_disconnect_and_ends_only_when_the_player_asks() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();
    assert!(app.frame());

    deliver(&mut app, SessionEvent::CharacterError(1));

    // Thirty frames: a loop that ended on the disconnect would have returned false from `frame()`
    // long before this.
    for i in 0..30 {
        assert!(
            app.frame(),
            "frame {i}: the loop ended on its own after a disconnect"
        );
        assert_ne!(app.state(), AppState::ShuttingDown, "frame {i}");
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    assert!(
        !app.ui().expect("shell").stats.device_done,
        "nothing has asked to quit yet"
    );

    // The player presses the screen's one button — `0x10000418`, message 1. Mouse release
    // broadcasts 1, while the auto-repeat hot click uses 2.
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
        "the click queues the epilogue; the loop is still alive"
    );
    assert_eq!(current_mode(&app), Some(mode::EPILOGUE));
    assert!(screen::<EpilogueScreen>(&mut app).log_off_requested);

    // The epilogue's own data message `0x10000002` marks the device done. The frame that handles
    // it still finishes; the next event-loop check sees the done flag and ends the loop.
    assert!(
        app.frame(),
        "the frame that marks the device done still finishes"
    );
    assert!(app.ui().expect("shell").stats.device_done);
    assert!(
        !app.frame(),
        "the following event-loop check sees the done flag and ends the loop"
    );
    assert_eq!(app.state(), AppState::ShuttingDown);
}

// ---------------------------------------------------------------------------------------------
// 3. The frame that erased the reason
// ---------------------------------------------------------------------------------------------

/// `App::build_host_state` is the only writer of `HostState::error`. With a link up it must keep a
/// character error raised by the session rather than replace it with the link's transport error
/// (which a character error never sets), so `apply_host_state`'s edge test sees it. A linkless
/// `App` does not take this arm.
///
/// The flow is left on the data-patch screen here — with nothing on the other end `0xF658` never
/// arrives and the patch screen never hands over, which is the screen a player watching a dead
/// server sits on. It is not the disconnected screen, so `apply_host_state`'s edge test is a real
/// one.
#[test]
fn a_character_error_survives_the_frame_that_raised_it_with_a_server_on_the_line() {
    let _gpu = gpu_lock();
    let mut app = app_on(connected_config(), mode::DATA_PATCH);
    assert!(app.config().will_connect(), "this run has a link");

    deliver(&mut app, SessionEvent::CharacterError(1));
    assert_eq!(
        app.host_state().error.as_deref(),
        Some("ID_CHAR_ERROR_LOGON"),
        "the notice raised the reason string"
    );

    // The first frame with the link arm running.
    assert!(app.frame(), "the loop must not end on a disconnect");
    assert_eq!(
        app.host_state().error.as_deref(),
        Some("ID_CHAR_ERROR_LOGON"),
        "the reason must survive build_host_state's link arm"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(current_mode(&app), Some(mode::DISCONNECTED));
    let logon = shipped_reason(&app, "ID_CHAR_ERROR_LOGON");
    assert_eq!(
        screen::<DisconnectedScreen>(&mut app).shown_text.as_deref(),
        Some(logon.as_str())
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Leaving the game from inside it — the logout confirmation
// ---------------------------------------------------------------------------------------------

use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen};

/// Click an element of the live tree by broadcasting **message 1**, the button-release message;
/// message 2 is the auto-repeat hot click.
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

/// The sentence the retail frame shows, from the shipped table behind enum `0x10000001`.
fn shipped_client_string(app: &App, id: &str) -> String {
    use dereth_ui::framework::LayoutEnumResolver as _;
    let table = dereth_ui::framework::DidMapperResolver::load_group(
        app.assets(),
        dereth_ui::framework::STRING_TABLE_GROUP,
    )
    .expect("the string-table group resolves")
    .resolve(dereth_ui::framework::LayoutEnum(logout::STRING_TABLE_ENUM))
    .expect("string-table enum 0x10000001 resolves to a shipped table");
    app.ui()
        .expect("the shell is up")
        .ui
        .resolve_string(table, dereth_primitives::num::hash::str_hash(id.as_bytes()))
        .unwrap_or_else(|| panic!("{id} is a row of the shipped table"))
}

/// Oracle: element `0x10000203` requests end-character-session with argument 1, which constructs
/// the shipped confirmation-dialog layout rooted at `0x15`.
///
/// **Full rigour on the sentence**, because the wrong prompt over the right buttons is exactly the
/// kind of thing a screenshot hides: the text on the dialog is compared with the shipped row for
/// `ID_Client_EndCharacterSessionConfirm` *and* with the row the other confirmation uses, so a
/// build that shows either constant cannot pass.
#[test]
fn exit_to_character_selection_raises_the_confirm_the_retail_frame_shows() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    let end_session = shipped_client_string(&app, logout::END_SESSION_CONFIRM);
    let logoff = shipped_client_string(&app, logout::LOGOFF_CONFIRM);
    assert_eq!(
        end_session, "This will exit your character from the game world.\n\nAre you sure?",
        "the paired retail frame's own sentence"
    );
    assert_ne!(
        end_session, logoff,
        "the oracle distinguishes the two confirmations"
    );

    assert!(
        screen::<GamePlayScreen>(&mut app).logout_dialog().is_none(),
        "nothing is up yet"
    );
    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    // One frame delivers the click and raises the dialog; the second is where the host resolves
    // the reason string and writes it in — `UiShell::apply_host_state` runs before the deliveries,
    // exactly as the disconnected screen's reason lands a frame after its mode switch.
    assert!(app.frame());
    assert!(app.frame());

    let dialog = screen::<GamePlayScreen>(&mut app)
        .logout_dialog()
        .expect("Exit to Character Selection raises the confirmation");
    assert_eq!(
        screen::<GamePlayScreen>(&mut app)
            .logout_prompt_text
            .as_deref(),
        Some(end_session.as_str()),
        "the dialog shows the client's own sentence"
    );

    let shell = app.ui_mut().expect("shell");
    // It is a confirmation dialog (element type 0x13) and it is modal: it blocks clicks behind it
    // and is brought to the front.
    let n = shell.ui.node(dialog).expect("the dialog element");
    assert_eq!(n.ty().0, 0x13, "the shipped confirmation dialog type");
    assert!(
        n.region.flags.block_clicks,
        "a modal confirmation blocks the clicks behind it"
    );
    // Yes on the left, No on the right, both real buttons of the shipped layout.
    for (id, x0) in [(logout::BUTTON_YES, 80), (logout::BUTTON_NO, 240)] {
        let b = shell
            .ui
            .get_child_recursive(dialog, id)
            .unwrap_or_else(|| panic!("{:#X} is a child of the dialog", id.0));
        assert_eq!(
            shell.ui.node(b).expect("button").ty().0,
            0x01,
            "a button element"
        );
        assert_eq!(shell.ui.node(b).expect("button").region.box_.x0, x0);
    }
    // Both resize and centering matter. The shipped panel is 400 x 95 with a one-line text box,
    // while the prompt is two lines: unresized it shows *"This will exit
    // your character from the game world."* and silently drops *"Are you sure?"*, and uncentred it
    // sits in the top-left corner instead of over the middle of the screen, which is where the
    // paired retail frame puts it.
    let (panel, root_box) = {
        let dialog = screen::<GamePlayScreen>(&mut app)
            .logout_dialog()
            .expect("the dialog");
        let shell = app.ui_mut().expect("shell");
        let p = shell
            .ui
            .get_child_recursive(dialog, dereth_ui::dialog::base::child::PANEL)
            .expect("the dialog's panel");
        (
            shell.ui.node(p).expect("panel").region.box_,
            shell.ui.node(dialog).expect("dialog").region.box_,
        )
    };
    assert!(
        panel.height() > 95,
        "the panel grew to fit two lines, not {}",
        panel.height()
    );
    assert_eq!(
        (panel.x0 + panel.x1 + 1) / 2,
        root_box.width() / 2,
        "centred horizontally in the dialog root"
    );
    assert_eq!(
        (panel.y0 + panel.y1 + 1) / 2,
        root_box.height() / 2,
        "and vertically at the center of the dialog root"
    );

    // And the confirmation is a *question*: nothing has left the world yet.
    assert_eq!(current_mode(&app), Some(mode::GAME_PLAY));
    assert!(!screen::<GamePlayScreen>(&mut app).logout_confirmed);
}

/// Oracle: the close-dialog logout arm sees property `0x92` as false, clears the ending-session
/// guard, and sets neither logout-confirmed nor do-end-session. The other direction of the claim,
/// and the one a one-sided test misses: *No* must leave the player exactly where they were.
#[test]
fn answering_no_dismisses_the_confirm_and_leaves_the_player_in_the_world() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    assert!(app.frame());
    assert!(screen::<GamePlayScreen>(&mut app).logout_dialog().is_some());

    click(&mut app, logout::BUTTON_NO);
    assert!(app.frame());
    assert!(
        screen::<GamePlayScreen>(&mut app).logout_dialog().is_none(),
        "the dialog is gone"
    );
    assert!(
        !screen::<GamePlayScreen>(&mut app).logout_confirmed,
        "No is not a yes"
    );
    for i in 0..20 {
        assert!(app.frame(), "frame {i}");
        assert_eq!(current_mode(&app), Some(mode::GAME_PLAY), "frame {i}");
    }

    // …and asking again works. Clearing the ending-session guard is essential: the refusal does
    // not unset do-end-session, so without the guard clear the per-frame session path would remain
    // latched for the rest of the session and a second *Yes* could never be acted on.
    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    assert!(app.frame());
    assert!(screen::<GamePlayScreen>(&mut app).logout_dialog().is_some());
    click(&mut app, logout::BUTTON_YES);
    assert!(app.frame());
    // The non-quit branch queues no mode; it asks the player system to log off with the quit flag
    // false and leaves the player standing in the world for the six seconds the shard takes to
    // answer. So the observable for "the second answer is acted on" is the pending log-off request,
    // not the screen.
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "no mode is queued by the client"
    );
    assert!(
        app.teleport().log_off_pending(),
        "the second answer is acted on -- it armed the pending logout request"
    );
}

/// Oracle: the same arm with the answer true sets logout-confirmed and do-end-session, then asks
/// the player system to log off with the quit flag false because end-character-session argument 1
/// leaves should-quit-on-logout **false**.
///
/// The client queues **no** mode on *Yes*: a later gameplay update reached from the server's
/// session-end event `0xF658` queues character-select mode `0x1000000A`, about six seconds later.
/// What *Yes* does immediately is ask to log off — and it must not quit.
#[test]
fn answering_yes_asks_to_log_off_and_does_not_quit() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    assert!(app.frame());
    click(&mut app, logout::BUTTON_YES);
    assert!(app.frame(), "answering Yes must not end the process");
    assert!(
        app.teleport().log_off_pending(),
        "player logoff with the quit flag clear must arm the logout request"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "log out to character select is not a mode change: the player stays in the world"
    );
    assert_ne!(
        current_mode(&app),
        Some(mode::EPILOGUE),
        "and it is not the quit path"
    );
    assert!(
        app.frame(),
        "and the client is still running, in the world, playing the departure"
    );
}

/// Oracle: element `0x10000617` broadcasts global message `(1, 0x10000027)`; its handler sets all
/// three quit flags at once.
///
/// **The asymmetry is the client's**: *Exit Game* does not ask. Reproduced rather than tidied, and
/// asserted, because "both buttons raise the dialog" is the plausible wrong answer.
#[test]
fn exit_game_quits_without_asking() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    click(&mut app, logout::EXIT_GAME);
    assert!(app.frame());
    // The mode switched in the **same frame the click was delivered**, which is only possible if
    // nothing asked: [`an_unanswered_confirm_never_ends_the_session_on_its_own`] holds the same
    // screen for thirty frames when a confirmation is up, and the screen is gone here — so there
    // is no `GamePlayScreen` left to hold a dialog.
    assert_eq!(
        current_mode(&app),
        Some(mode::EPILOGUE),
        "straight to the quit screen"
    );
    assert!(
        app.frame(),
        "and the quit screen is where the player is, not a dead process"
    );
}

/// Oracle: the per-frame logout-confirmed guard keeps an *unconfirmed* end-of-session request
/// pending for as long as the question is on screen.
///
/// This is the guard that makes confirmation possible at all: without it the request flags would
/// issue a premature logout while the dialog was still pending. Character-select mode is queued
/// only later, after the server's session-end event, as the preceding test establishes.
#[test]
fn an_unanswered_confirm_never_ends_the_session_on_its_own() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();

    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    assert!(app.frame());
    assert!(
        screen::<GamePlayScreen>(&mut app).do_end_session,
        "the request armed end-of-session processing"
    );

    for i in 0..30 {
        assert!(app.frame(), "frame {i}");
        assert_eq!(
            current_mode(&app),
            Some(mode::GAME_PLAY),
            "frame {i}: nobody answered"
        );
        assert!(
            screen::<GamePlayScreen>(&mut app).logout_dialog().is_some(),
            "frame {i}"
        );
    }
}

/// The confirmation is not only in the tree — it is **on the screen**, and only where it should be.
///
/// Oracle: the frame itself — two captures of the same screen differing only in the thing under
/// test. Every pixel that changed between "before the click" and "after it"
/// must lie inside the dialog panel's own box (element `0x3D`, 400 x 95 of the shipped `Dialog`
/// layout), and there must be a great many of them: a dialog that is in the tree and draws
/// nothing, or one that repaints the whole HUD, both fail.
#[test]
fn the_confirmation_draws_and_only_inside_its_own_panel() {
    let _gpu = gpu_lock();
    let mut app = app_in_game();
    for _ in 0..2 {
        assert!(app.frame());
    }
    let (w, h, before) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");

    click(&mut app, logout::EXIT_TO_CHARACTER_SELECTION);
    for _ in 0..2 {
        assert!(app.frame());
    }
    let (w2, h2, after) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");
    assert_eq!((w, h), (w2, h2));

    // The panel the dialog actually paints, in screen coordinates.
    let panel = {
        let dialog = screen::<GamePlayScreen>(&mut app)
            .logout_dialog()
            .expect("the dialog");
        let shell = app.ui_mut().expect("shell");
        let p = shell
            .ui
            .get_child_recursive(dialog, dereth_ui::ElementId(0x3D))
            .expect("the dialog's panel");
        shell.ui.screen_box(p)
    };

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if before[i..i + 3] == after[i..i + 3] {
                continue;
            }
            let (xi, yi) = (x as i32, y as i32);
            if xi >= panel.x0 && xi <= panel.x1 && yi >= panel.y0 && yi <= panel.y1 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    eprintln!("logout confirm: panel {panel:?}, {inside} changed inside, {outside} outside");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the confirmation's own panel"
    );
    assert!(
        inside > 2_000,
        "only {inside} pixels changed inside it — the dialog did not draw"
    );
}
