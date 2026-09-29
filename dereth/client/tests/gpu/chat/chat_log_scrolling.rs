//! The chat log follows the conversation and its scrollbar, track, "new text below" indicator
//! and maximize button behave. The log stays at its newest glyph as lines arrive (a decoded body's
//! boundary newlines are trimmed at delivery, so no empty bottom row opens); the indicator
//! (`0x1000048C`) is hidden at the end and comes up when a line arrives below a scrolled-back
//! view; the thumb travels the track as the log is walked; a click on the track pages the log by
//! the view less a line; and maximizing grows the main chat window by half its parent's height
//! upward (property `0x3E` is the restore floor, not the grown height) with the log re-anchored.
//!
//! | claim | oracle |
//! |---|---|
//! | chat text/order | the committed corpus's richest chat recording replayed through ClientNetwork, ObjectStream and HUD; the trim test builds a decoded welcome and follow-up lines |
//! | end position | offset equals content height minus view height when overflowing, plus last-glyph visibility |
//! | thumb position | track-relative pixel range, including the active-travel-minus-one rule |
//! | track click | page increment from the text element's scroll-delta function: the view minus a line |
//! | indicator states | shipped records 1/0x0D carry hide property 0x3B=false/true |
//! | maximize geometry | grown y = y - parent height/2 and grown height = current height + parent height/2 |
//!
//! Fixture: headless Apps with no shard link; replay feeds `ClientNetwork` without a socket peer
//! and discards outgoing datagrams. Pointer messages are built by `Pump`, dispatched to it and
//! forwarded to the input shell. Every fixture path is an `expect`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;
use crate::common::workspace_root;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::pump::{Pump, Win32Message};
use dereth_client_net::client_session::testing::capture::{shared_session, Datagram};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::LocalTime;
use dereth_ui::framework::mode;
use dereth_ui::{Box2D, ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::interface::ChatMessage;
use dereth_ui_screens::chat::window::{LOG, NEW_TEXT_BELOW, SCROLLBAR};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use winit::event::MouseButton;

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn app_in_gameplay(frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        ui: true,
        ..base_config()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("gameplay screen"),
    )
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn centre(app: &App, h: ElemHandle) -> (i32, i32) {
    let b = app.ui().expect("shell").ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// The arrow that walks the log back is `0x10000072`, not `0x10000071`.
///
/// The scrollbar's layout places increment button 0x77 (element 0x10000072) at (0,0), the top
/// of a vertical bar, and decrement button 0x78 (0x10000071) at the far corner. A recovery path
/// makes the same moves independently; the runtime positions override the opposite DAT order.
///
/// Increment raises message 0x0D. Scroll handling negates the delta for 0x0D or 0x0F, so the top
/// button moves toward offset zero and older text.
const ARROW_UP: ElementId = ElementId(0x1000_0072);
/// The other one: the decrement arrow, `0x78`, at the bottom of the bar, raising `0x0E` and
/// walking the log forward toward its newest line. See [`ARROW_UP`].
const ARROW_DOWN: ElementId = ElementId(0x1000_0071);
/// Main chat window whose height the maximize action changes.
const MAIN_CHAT_WINDOW: ElementId = ElementId(0x1000_0601);

// ---------------------------------------------------------------------------------------------
// Pointer messages delivered through Pump and InputShell
// ---------------------------------------------------------------------------------------------

struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 100_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn click(&mut self, app: &mut App, at: (i32, i32)) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .mouse_button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is in the 0x201 block");
            self.send(app, m);
        }
        app.frame();
    }
}

// ---------------------------------------------------------------------------------------------
// The corpus's own recorded wire
// ---------------------------------------------------------------------------------------------

/// The recording [`a_login_made_long_after_start_up_keeps_its_connection`] replays: a short
/// login that enters the world, carries chat and whose replayed link ends `Connected`. (The two
/// `unclean-logout-*` recordings cannot serve: their replayed links end `Disconnected` on their
/// own.)
const LATE_LOGIN_RECORDING: &str = "first-login-walk-jump";

/// How far after process start that replay's login is placed: late enough that a receiver
/// clock started at zero would already count the connection as silent.
const LATE_LOGIN_START: f64 = 1995.0;

/// How many recorded lines the maximize station replays: enough to overflow the grown log.
const MAXIMIZE_LINES: usize = 30;

/// Every recording in the flat `fixtures/packet-captures/` corpus, by slug, sorted. Missing or
/// unreadable data is fatal; the corpus is an input oracle, not a generated fixture.
fn corpus_recordings() -> Vec<(String, &'static [Datagram])> {
    let dir = workspace_root().join("fixtures/packet-captures");
    let mut slugs: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("the capture corpus is this test's oracle; {dir:?}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .expect("a slug")
                .to_owned()
        })
        .collect();
    slugs.sort();
    assert!(
        !slugs.is_empty(),
        "the capture corpus at {dir:?} holds no recording"
    );
    slugs
        .into_iter()
        .map(|slug| {
            let records = shared_session(&slug);
            (slug, records)
        })
        .collect()
}

/// One recorded session through the real transport, session and object stream.
fn replay(records: &[Datagram]) -> Vec<SessionEvent> {
    replay_link(records, 0.0).0
}

/// [`replay`], plus what the link itself ended up as: the status it reached and how many recorded
/// server datagrams [`ClientNetwork::feed`] refused. A station that decodes nothing wants to say *where*
/// it stopped, not only that it stopped. `offset` is added to every recorded time, placing the
/// login that many seconds after the process (and so the receiver's clock) started.
fn replay_link(
    records: &[Datagram],
    offset: f64,
) -> (Vec<SessionEvent>, dereth_client::net::LinkStatus, u32) {
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("this recording has a LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t + offset);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    (events, net.status(), net.rejected())
}

/// Choose the recording yielding the most decoded HUD chat messages, rather than the longest
/// duration or a fixed name. Require at least nine messages for the welcome tests, and
/// [`MAXIMIZE_LINES`] for the maximize station.
fn shard_chat_lines() -> Vec<ChatMessage> {
    let mut best: Vec<ChatMessage> = Vec::new();
    for (_, records) in corpus_recordings() {
        let events = replay(records);
        let mut hud = dereth_client::hud::Hud::default();
        let mut world = dereth_client_model::World::new();
        let mut lines = Vec::new();
        for e in &events {
            lines.extend(hud.apply_events(std::slice::from_ref(e), &mut world));
        }
        if lines.len() > best.len() {
            best = lines;
        }
    }
    assert!(
        best.len() >= MAXIMIZE_LINES,
        "the corpus carries the shard's welcome burst; the chosen recording has {} line(s)",
        best.len()
    );
    best
}

/// A connection created long after process start begins with a current last-data stamp.
/// The receiver's initialization sets the heartbeat and state-change times; packet processing
/// handles connection-creating optional headers first, then stamps the newly available receiver
/// before processing fragment data, so the creating ConnectRequest itself starts the silence
/// clock.
///
/// `ClientNetwork::feed` handles that request before there is a transport connection, bypassing
/// the ordinary packet-feed stamp. A receiver left at time 0 makes `Net::process_connections` see
/// more than 140 seconds of silence on its next pass whenever login happens more than 140 seconds
/// after startup, and the link disconnects after three datagrams, before a world blob.
///
/// [`LATE_LOGIN_RECORDING`] is replayed with its clock offset to that late login time. Connected
/// status, zero rejections and nonempty chat below each rule out the premature timeout.
#[test]
fn a_login_made_long_after_start_up_keeps_its_connection() {
    let (_, seg) = corpus_recordings()
        .into_iter()
        .find(|(slug, _)| slug == LATE_LOGIN_RECORDING)
        .expect("the late-login recording is in the corpus");
    assert!(
        seg[0].t + LATE_LOGIN_START > 140.0,
        "this replay is the oracle precisely because it begins after the 140 s silence timeout, \
         and it begins at t = {} s",
        seg[0].t + LATE_LOGIN_START
    );

    let (events, status, rejected) = replay_link(&seg, LATE_LOGIN_START);
    assert_eq!(
        status,
        dereth_client::net::LinkStatus::Connected,
        "the recorded link was alive for the whole segment; this build ended it"
    );
    assert_eq!(
        rejected, 0,
        "every recorded server datagram belongs to the recorded connection"
    );

    let mut hud = dereth_client::hud::Hud::default();
    let mut world = dereth_client_model::World::new();
    let mut lines = Vec::new();
    for e in &events {
        lines.extend(hud.apply_events(std::slice::from_ref(e), &mut world));
    }
    assert!(
        !lines.is_empty(),
        "the shard's welcome burst is in this segment; found {} session event(s) and 0 chat line(s)",
        events.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Small readers
// ---------------------------------------------------------------------------------------------

fn scroll(ui: &mut UiSystem, log: ElemHandle) -> (i32, i32) {
    ui.text_element_mut(log)
        .map_or((0, 0), |t| (t.scroll.y, t.scroll.height))
}

fn at_end(ui: &mut UiSystem, log: ElemHandle) -> bool {
    let screen = ui.screen_box(log);
    ui.text_element_mut(log)
        .is_none_or(|t| t.is_at_vertical_end(screen))
}

fn thumb(ui: &UiSystem, bar: ElemHandle) -> Box2D {
    let t = ui
        .get_child(bar, ElementId(1))
        .expect("the thumb is child id 1");
    ui.screen_box(t)
}

/// The band the thumb's own box may occupy: the bar less the two arrow buttons.
fn track(app: &App) -> Box2D {
    let ui = &app.ui().expect("shell").ui;
    let bar = ui.screen_box(find(app, SCROLLBAR));
    let up = ui.screen_box(find(app, ARROW_UP));
    let down = ui.screen_box(find(app, ARROW_DOWN));
    Box2D::new(bar.x0, bar.y0 + up.height(), bar.x1, bar.y1 - down.height())
}

fn drawn(app: &App, h: ElemHandle) -> bool {
    app.ui()
        .expect("shell")
        .ui
        .draw_trace()
        .iter()
        .any(|e| e.who == h)
}

// ---------------------------------------------------------------------------------------------
// 1. The log follows the conversation
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.log.the-bottom-row-is-the-newest-line-and-never-an-empty-one
///
/// A delivered chat body does not open an empty final line. The scroll submission trims leading
/// and trailing LF before constructing its display body, and the chat consumer applies
/// `add_text_to_scroll_trim`, so the welcome's trailing LF never reaches the log.
///
/// The general glyph-tail rule (empty, ordinary and trailing-newline text) is the unit test
/// `glyph::tests::the_final_tail_distinguishes_empty_ordinary_and_trailing_newline_text`.
///
/// This test builds the recorded welcome's decoded `ChatMessage` body without replaying the
/// unrelated login preconditions. The real `GamePlayScreen` consumer appends to element
/// 0x10000011, updates the scroll extent and follows eight synthetic ordinary lines. The
/// replay-backed test below separately covers recorded chat messages.
#[test]
fn a_trailing_newline_is_trimmed_at_the_chat_log_and_the_log_still_follows() {
    let _gpu = gpu_lock();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    let welcome = ChatMessage {
        ty: 0,
        body: "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n".into(),
        prefix: None,
        window: 0,
    };
    {
        let (ui, screen) = gameplay(&mut app);
        assert!(
            !screen
                .recv_display_final_string_info(ui, &welcome)
                .is_empty(),
            "the decoded welcome reaches a shipped chat window",
        );
        let screen = ui.screen_box(log);
        let t = ui
            .text_element_mut(log)
            .expect("the shipped chat log is a text element");
        let content = t.content_box(screen);
        let wrapped =
            dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
        let count = t.glyphs.len();
        assert!(
            !t.glyphs
                .glyphs
                .last()
                .expect("the routed welcome has glyphs")
                .is_new_line(),
            "chat delivery trimmed the welcome's trailing newline before appending",
        );
        let bottom = wrapped.last().copied().expect("the log has wrapped lines");
        assert!(
            bottom.end > bottom.start && bottom.width > 0,
            "so the log's bottom row carries glyphs; got {bottom:?} over {count} glyph(s)",
        );
        assert_eq!(
            t.glyphs.inq_text(false),
            welcome.body.trim_matches('\n'),
            "the log holds the trimmed body and nothing else",
        );
        let (up, down, _, _) = t.margins;
        assert_eq!(
            t.scroll.height,
            wrapped.iter().map(|line| line.height).sum::<i32>() + up + down,
            "wrapped line heights and margins determine the scrollable extent",
        );
    }

    for i in 0..8 {
        let m = ChatMessage {
            ty: 0,
            body: format!("ordinary follow-up line {i}"),
            prefix: None,
            window: 0,
        };
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, &m);
    }
    let (ui, _) = gameplay(&mut app);
    let view = ui.screen_box(log).height();
    let (y, content) = scroll(ui, log);
    assert!(content > view, "the real log has scroll travel");
    assert_eq!(y, content - view, "later lines still follow to the bottom");
    assert!(
        at_end(ui, log),
        "the end-of-log predicate agrees after the follow-up lines"
    );
    app.shutdown();
}

/// Behaviour: chat.log.the-log-follows-new-lines-while-at-its-end
///
/// The first nine messages from the selected recorded chat segment keep the log at its newest
/// glyph without a player gesture. The decoded welcome ends in LF, trimmed at delivery.
///
/// The checks require offset=content-view whenever overflowing and at-end after every message,
/// then require the newest glyph in view and the oldest above it.
#[test]
fn the_log_follows_the_shards_own_welcome_burst() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();
    assert!(
        lines[0].body.ends_with('\n'),
        "the shard's welcome ends in a newline -- the premise of this test; got {:?}",
        lines[0].body
    );

    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    let view = app.ui().expect("shell").ui.screen_box(log).height();

    let mut seen = 0;
    for (n, m) in lines.iter().take(9).enumerate() {
        let (ui, screen) = gameplay(&mut app);
        let before = at_end(ui, log);
        assert!(before, "line {n} arrived with the log at its end");
        let took = screen.recv_display_final_string_info(ui, m);
        assert!(!took.is_empty(), "line {n} was routed to a window");
        seen += 1;
        let (y, content) = scroll(ui, log);
        if content > view {
            assert_eq!(
                y,
                content - view,
                "after line {n} the log sits on its last line (content {content}, view {view})"
            );
        }
        assert!(
            at_end(ui, log),
            "and the end-of-log predicate agrees after line {n}"
        );
    }
    assert_eq!(seen, 9, "nine of the shard's lines were replayed");

    // What a player sees, not merely what the offset says: the last glyph is inside the pane and
    // the first is above it.
    let (ui, _) = gameplay(&mut app);
    let screen = ui.screen_box(log);
    let t = ui.text_element_mut(log).expect("a text element");
    let last = t.glyphs.len() - 1;
    assert!(
        t.is_position_in_view(screen, last),
        "the newest glyph is in the pane"
    );
    assert!(
        !t.is_position_in_view(screen, 0),
        "and the oldest has scrolled off the top"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 2. The "new text below" indicator
// ---------------------------------------------------------------------------------------------

/// The 0x1000048C indicator is hidden at the end and shown after text arrives below a
/// scrolled-back view. Layout state 1 carries hide 0x3B=false; state 0x0D carries true; state 0 is
/// absent. Only the indicator's click arm hides it once shown; this client also applies the
/// hidden state 0x0D in the chat window's post_init, so a full log opens with no arrow.
///
/// Removing the initial hiding exposes the arrow immediately; removing the off-end arrival arm
/// prevents it appearing. The test also proves scrolling alone leaves it hidden, a new line
/// raises it without moving the view, and clicking it reaches the end and hides it again.
#[test]
fn the_new_text_below_indicator_follows_is_at_vertical_end() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();
    let mut app = app_in_gameplay(4);
    let (log, arrow) = (find(&app, LOG), find(&app, NEW_TEXT_BELOW));

    // The two state records, off the shipped layout.
    {
        let ui = &app.ui().expect("shell").ui;
        let n = ui.node(arrow).expect("the arrow");
        assert_eq!(n.desc.ty.0, 0x01, "0x1000048C is a button element");
        let hidden = |s: u32| {
            n.desc
                .access_state(dereth_ui::StateId(s))
                .and_then(|d| d.properties.get(dereth_ui::props::attr::HIDE))
                .cloned()
        };
        assert_eq!(
            hidden(1),
            Some(dereth_ui::PropertyValue::Bool(false)),
            "state 1 shows it"
        );
        assert_eq!(
            hidden(0x0D),
            Some(dereth_ui::PropertyValue::Bool(true)),
            "state 0x0D hides it"
        );
        assert!(
            n.desc.access_state(dereth_ui::StateId(0)).is_none(),
            "there is no state 0"
        );
    }

    // At the end -- and it stays down through the whole of the shard's burst.
    assert!(!drawn(&app, arrow), "the arrow is down on an empty log");
    for m in lines.iter().take(9) {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, m);
    }
    app.frame();
    assert!(
        !drawn(&app, arrow),
        "and down through the shard's whole welcome burst"
    );

    // Scroll off the end by hand, then take a line.
    let up = centre(&app, find(&app, ARROW_UP));
    let mut hand = Hand::new();
    for _ in 0..3 {
        hand.click(&mut app, up);
    }
    {
        let (ui, _) = gameplay(&mut app);
        assert!(!at_end(ui, log), "three up-arrows took the log off its end");
    }
    assert!(
        !drawn(&app, arrow),
        "the arrow is still down until something arrives below the fold"
    );

    let before = {
        let (ui, _) = gameplay(&mut app);
        scroll(ui, log).0
    };
    {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, &lines[1]);
    }
    app.frame();
    assert!(drawn(&app, arrow), "a line below the fold raises the arrow");
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(scroll(ui, log).0, before, "and does not move the view");
    }

    // Clicking element 0x1000048C scrolls to the end and applies hidden state 0x0D.
    let at = centre(&app, arrow);
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert!(at_end(ui, log), "the click jumped the log to its end");
    }
    assert!(!drawn(&app, arrow), "and took the arrow down");
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 3. The thumb tracks the offset
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track
///
/// Walking from newest to oldest text moves the thumb from the bottom to the top of its track.
/// `Scrollable::update_scrollbar_position` refreshes the bar's layout explicitly; an attribute
/// write alone does not, because arrow dispatch temporarily detaches the bar's behaviour from its
/// arena slot, so `UiSystem::on_set_attribute` cannot reach that subclass.
///
/// Removing `update_layout_of` leaves the position attribute moving but the thumb stationary.
/// The test requires exact endpoints, unchanged thumb size, at least four distinct positions,
/// monotonic upward travel and an exact return to the original bottom box.
#[test]
fn the_thumb_travels_the_track_as_the_log_is_walked() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();
    let mut app = app_in_gameplay(4);
    let (log, bar) = (find(&app, LOG), find(&app, SCROLLBAR));
    for m in lines.iter().take(9) {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, m);
    }
    app.frame();

    let band = track(&app);
    let (at_bottom, content, view) = {
        let (ui, _) = gameplay(&mut app);
        let (y, content) = scroll(ui, log);
        (thumb(ui, bar), content, ui.screen_box(log).height() + y - y)
    };
    assert!(
        content > view,
        "the burst overflows the pane ({content} > {view})"
    );
    // Thumb y = track.y0 + trunc(position * (active - 1)), where active is the track height
    // minus the thumb height. At position 1 the travel ends one pixel short; keep that exact
    // endpoint rather than tolerating or smoothing the difference.
    let active = band.height() - at_bottom.height();
    assert!(
        active > 0,
        "the thumb is shorter than the track ({at_bottom:?} in {band:?})"
    );
    assert_eq!(
        at_bottom.y0,
        band.y0 + active - 1,
        "at the end of the log the thumb is at the far end of its travel ({at_bottom:?} in {band:?})"
    );

    // Walk the whole log back with the up arrow, one line at a time, and watch the thumb.
    let up = centre(&app, find(&app, ARROW_UP));
    let mut hand = Hand::new();
    let mut seen: Vec<i32> = vec![at_bottom.y0];
    for _ in 0..64 {
        hand.click(&mut app, up);
        let (ui, _) = gameplay(&mut app);
        let t = thumb(ui, bar);
        if seen.last() != Some(&t.y0) {
            seen.push(t.y0);
        }
        if scroll(ui, log).0 == 0 {
            break;
        }
    }
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            scroll(ui, log).0,
            0,
            "the arrows walked the log to its first line"
        );
    }
    let at_top = {
        let (ui, _) = gameplay(&mut app);
        thumb(ui, bar)
    };
    assert_eq!(
        at_top.y0, band.y0,
        "and the thumb's top is now the track's top"
    );
    assert_eq!(
        at_top.height(),
        at_bottom.height(),
        "the thumb kept its size; only its position moved"
    );
    assert!(
        seen.len() >= 4,
        "the thumb moved through the track rather than jumping between two ends: {seen:?}"
    );
    assert!(
        seen.windows(2).all(|w| w[1] <= w[0]),
        "and it moved monotonically upward as the log did: {seen:?}"
    );

    // ...and back down again, to the pixel it started on.
    let down = centre(&app, find(&app, ARROW_DOWN));
    for _ in 0..64 {
        hand.click(&mut app, down);
        let (ui, _) = gameplay(&mut app);
        if at_end(ui, log) {
            break;
        }
    }
    let (ui, _) = gameplay(&mut app);
    assert_eq!(
        thumb(ui, bar),
        at_bottom,
        "the return traverse puts the thumb back where it was"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 4. A track click pages the log
// ---------------------------------------------------------------------------------------------

/// A click below the thumb pages forward by the view minus a line; the expected delta is derived
/// from the text element's own scroll function. It requires exact forward addition and a
/// backward move for a click above the new thumb.
///
/// A hot-click message 0x02 raised inside the bar's own dispatch cannot be delivered to its
/// temporarily detached behaviour (an arrow is a separate element and still works), so the
/// scrollbar's MOUSE_PRESS handling recognizes a self-sourced hot click directly. Removing that
/// arm leaves the arrows working but stops track paging.
#[test]
fn a_click_on_the_track_pages_the_log() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();
    let mut app = app_in_gameplay(4);
    let (log, bar) = (find(&app, LOG), find(&app, SCROLLBAR));
    for m in lines.iter().take(9) {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, m);
    }
    app.frame();

    // Start at offset zero so there is empty track below the thumb for the forward click.
    {
        let (ui, _) = gameplay(&mut app);
        let mut s = ui.text_element_mut(log).expect("a text element").scroll;
        s.set_scrollable_xy(ui, log, 0, 0, true);
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = s;
        }
        s.update_scrollbar_position(ui, log, false);
    }
    app.frame();

    let band = track(&app);
    let (page, tb) = {
        let (ui, _) = gameplay(&mut app);
        let screen = ui.screen_box(log);
        let t = ui.text_element_mut(log).expect("a text element");
        let page = t.inq_scroll_delta(screen, false, false, true);
        (page, thumb(ui, bar))
    };
    assert!(page > 0, "a page is more than nothing");

    let before = {
        let (ui, _) = gameplay(&mut app);
        scroll(ui, log).0
    };
    let mut hand = Hand::new();
    let below = ((band.x0 + band.x1) / 2, (tb.y1 + band.y1) / 2);
    assert!(
        below.1 > tb.y1,
        "the click is on the track below the thumb, not on the thumb"
    );
    hand.click(&mut app, below);
    let after_down = {
        let (ui, _) = gameplay(&mut app);
        scroll(ui, log).0
    };
    assert_eq!(
        after_down,
        before + page,
        "one track click below the thumb pages the log forward"
    );

    // And the other half of the track pages it back.
    let tb2 = {
        let (ui, _) = gameplay(&mut app);
        thumb(ui, bar)
    };
    assert!(
        tb2.y0 > band.y0,
        "the thumb moved down the track, leaving room above it"
    );
    let above = ((band.x0 + band.x1) / 2, (band.y0 + tb2.y0) / 2);
    hand.click(&mut app, above);
    let after_up = {
        let (ui, _) = gameplay(&mut app);
        scroll(ui, log).0
    };
    assert!(
        after_up < after_down,
        "a click above the thumb pages it back ({after_down} -> {after_up})"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 5. The maximize button
// ---------------------------------------------------------------------------------------------

/// Maximizing grows the window and keeps the log anchored: grown y = y - parentH/2 and grown
/// height = myH + parentH/2. Property 0x3E is the restore floor; its shipped value 100 equals the
/// window height and therefore cannot grow it.
///
/// This station replays [`MAXIMIZE_LINES`] recorded lines and asserts the overflow it depends on.
///
/// The fixture's 0x10000601 window is 410x100 at y 500..599 within a 600-high parent: grown y=200
/// and height=400, with the bottom edge unchanged. Restore returns 500/100. The test invokes the
/// maximize consumer directly, not its pointer hit path. Using 0x3E as the grown height fails the
/// geometry; replacing the chat-aware resize_to with bare UI resizing loses scroll re-anchoring.
#[test]
fn the_maximize_button_grows_the_window_and_keeps_the_log_at_its_end() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    let win = find(&app, MAIN_CHAT_WINDOW);
    for m in lines.iter().take(MAXIMIZE_LINES) {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, m);
    }
    app.frame();

    let (before, parent_h) = {
        let ui = &app.ui().expect("shell").ui;
        (
            ui.screen_box(win),
            ui.parent(win).map_or(0, |p| ui.screen_box(p).height()),
        )
    };
    assert_eq!(parent_h, 600, "the gameplay root is 600 tall at 800x600");
    assert_eq!(
        (before.x0, before.y0, before.width(), before.height()),
        (0, 500, 410, 100),
        "the shipped main chat window"
    );
    // The attribute that was being used as the grown height, and why it cannot be.
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(win)
            .and_then(|n| n.merged_properties().get_int(0x3E)),
        Some(100),
        "attribute 0x3E equals the window's own height, so growing to it is a no-op"
    );

    let got = {
        let (ui, screen) = gameplay(&mut app);
        screen.chat_on_maximize_button(ui)
    };
    app.frame();
    assert_eq!(
        got,
        Some((200, 400)),
        "grownY = y - parentH/2, grownH = myH + parentH/2"
    );

    let after = app.ui().expect("shell").ui.screen_box(win);
    assert_eq!(
        (after.x0, after.y0, after.width(), after.height()),
        (0, 200, 410, 400),
        "the window grew from 100 to 400 rather than moving with its height unchanged"
    );
    assert_eq!(after.y1, before.y1, "and its bottom edge did not move");
    assert!(
        after.height() > before.height(),
        "this is a maximize, not a move"
    );

    // Chat-aware resizing grows the log and re-anchors it to its newest line.
    let log_after = app.ui().expect("shell").ui.screen_box(log);
    assert!(
        log_after.height() > 300,
        "the log grew with the window ({})",
        log_after.height()
    );
    {
        // Assert offset=content-view as well as at_end. Bare UI resizing leaves offset 391, the
        // former end of a 73-pixel view, after the view grows to 373 pixels: 300 beyond the new
        // end, which the end predicate alone accepts. The exact offset assertion closes the gap.
        let (ui, _) = gameplay(&mut app);
        let (y, content) = scroll(ui, log);
        let view = ui.screen_box(log).height();
        assert!(
            content > view,
            "the replayed lines overflow the grown log ({content} of {view})"
        );
        assert_eq!(
            y,
            content - view,
            "the log re-anchored to its last line across the resize (content {content}, view {view})"
        );
        assert!(
            at_end(ui, log),
            "and the newest line is on the pane's bottom edge"
        );
    }

    // Restoring puts it back where it was.
    let got = {
        let (ui, screen) = gameplay(&mut app);
        screen.chat_on_maximize_button(ui)
    };
    app.frame();
    assert_eq!(
        got,
        Some((500, 100)),
        "restore returns the saved y and height"
    );
    let back = app.ui().expect("shell").ui.screen_box(win);
    assert_eq!(back, before, "and the window is exactly where it started");
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 6. The same four gestures in rendered pixels
// ---------------------------------------------------------------------------------------------

/// The four gestures, measured on this client's own rendered frames.
///
/// A log rectangle is diffed before and after each gesture:
///
/// | gesture | what this asserts |
/// |---|---|
/// | down arrow | more than 5 % of the log changes |
/// | up arrow | more than 5 % of the log changes |
/// | track click | more than 5 % of the log changes |
/// | thumb over a full traverse | the bar's own rectangle changes |
///
/// The rectangles are the elements' own screen boxes rather than hand-chosen ones, and the
/// denominator is asserted, because a rate with no denominator is not a measurement.
#[test]
fn the_four_gestures_in_rendered_pixels() {
    let _gpu = gpu_lock();
    let lines = shard_chat_lines();

    let mut app = app_in_gameplay(4);
    let (log, bar) = (find(&app, LOG), find(&app, SCROLLBAR));
    // Enough of the shard's own traffic to overflow the pane several times over.
    for _ in 0..4 {
        for m in lines.iter().take(9) {
            let (ui, screen) = gameplay(&mut app);
            screen.recv_display_final_string_info(ui, m);
        }
    }
    app.frame();

    let log_box = app.ui().expect("shell").ui.screen_box(log);
    let bar_box = app.ui().expect("shell").ui.screen_box(bar);
    let band = track(&app);
    #[allow(clippy::cast_sign_loss)]
    let log_px = (log_box.width() * log_box.height()) as usize;
    assert!(
        log_px > 20_000,
        "the log rectangle is {log_px} px, expected more than 20,000"
    );

    let grab = |app: &mut App| -> (u32, u32, Vec<u8>) {
        app.renderer_mut()
            .capture_bgra()
            .expect("the frame reads back")
    };
    let changed = |a: &(u32, u32, Vec<u8>), b: &(u32, u32, Vec<u8>), r: Box2D| -> usize {
        assert_eq!((a.0, a.1), (b.0, b.1), "the two frames are the same size");
        let mut n = 0;
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                #[allow(clippy::cast_sign_loss)]
                let i = ((y as u32 * a.0 + x as u32) * 4) as usize;
                if a.2[i..i + 3] != b.2[i..i + 3] {
                    n += 1;
                }
            }
        }
        n
    };
    #[allow(clippy::cast_precision_loss)]
    let pct = |n: usize, d: usize| (n as f64) * 100.0 / (d as f64);

    let at_end_frame = grab(&mut app);

    let mut hand = Hand::new();
    let up = centre(&app, find(&app, ARROW_UP));
    let down = centre(&app, find(&app, ARROW_DOWN));

    hand.click(&mut app, up);
    app.frame();
    let after_up = grab(&mut app);
    let n_up = changed(&at_end_frame, &after_up, log_box);

    hand.click(&mut app, down);
    app.frame();
    let after_down = grab(&mut app);
    let n_down = changed(&after_up, &after_down, log_box);

    eprintln!(
        "log differential:up {n_up}/{log_px} = {:.2} %, down {n_down}/{log_px} = {:.2} %",
        pct(n_up, log_px),
        pct(n_down, log_px)
    );
    assert!(
        pct(n_up, log_px) > 5.0,
        "the up arrow redraws the log ({n_up} of {log_px})"
    );
    assert!(
        pct(n_down, log_px) > 5.0,
        "the down arrow redraws the log ({n_down} of {log_px})"
    );

    // The track click.
    let tb = {
        let (ui, _) = gameplay(&mut app);
        thumb(ui, bar)
    };
    assert!(
        tb.y0 > band.y0 + 2,
        "there is track above the thumb to click on"
    );
    let before_track = grab(&mut app);
    hand.click(&mut app, ((band.x0 + band.x1) / 2, (band.y0 + tb.y0) / 2));
    app.frame();
    let after_track = grab(&mut app);
    let n_track = changed(&before_track, &after_track, log_box);
    eprintln!(
        "track click: {n_track}/{log_px} = {:.2} %",
        pct(n_track, log_px)
    );
    assert!(
        pct(n_track, log_px) > 5.0,
        "a track click pages the log ({n_track} of {log_px})"
    );

    // The thumb, over a full traverse: the bar's rectangle changes.
    let bar_before = grab(&mut app);
    for _ in 0..96 {
        hand.click(&mut app, up);
        let (ui, _) = gameplay(&mut app);
        if scroll(ui, log).0 == 0 {
            break;
        }
    }
    app.frame();
    let bar_after = grab(&mut app);
    let n_bar = changed(&bar_before, &bar_after, bar_box);
    #[allow(clippy::cast_sign_loss)]
    let bar_px = (bar_box.width() * bar_box.height()) as usize;
    eprintln!("thumb travel: {n_bar}/{bar_px} px of the bar changed over a full traverse");
    assert!(
        n_bar > 0,
        "the thumb moved somewhere on the bar over a full traverse ({n_bar} of {bar_px})"
    );
    app.shutdown();
}
