//! The chat window scrolls, draws each line in its channel's colour, and can be typed into and
//! sent from.
//!
//! The log (`0x10000011`, 368 x 73) and the entry (`0x10000016`, 306 x 17) are different text
//! elements: the log is selectable (`0x27`) and scrollable, the entry is editable (`0x16`), one
//! line (`0x20`) and holds 255 characters (`0x1E`), as shipped. Each chat type is drawn in its
//! colour-table slot and the speaker prefix in the grey slot `0x0C` whatever the type; a line is
//! appended at the bottom and the log follows it only when it was already at its end (otherwise
//! the "new text below" arrow comes up); the scrollbar's position and the log's offset are exact
//! inverses; and Send (element `0x10000019`, message 1) hands the typed line to the chat-command
//! interpreter, which speaks plain text and forwards an unknown `@` command verbatim.
//!
//! | claim | oracle |
//! |---|---|
//! | which colour a chat type is drawn in | `chat::colors`, the client's 34-slot colour table |
//! | the prefix is grey whatever the type is | the display-notice path appends the prefix with colour index `0x0C` |
//! | the newest line is at the bottom | the same path scrolls to the glyph count only when the log was already at its end |
//! | the scroll offset from a bar position | `offset = ftol(position * (content - view))`, inverse to `position = offset / (content - view)` |
//! | an arrow's step in pixels | the element's configured scroll delta |
//! | the thumb's proportion | `min(view / content, 1.0)` |
//! | Send | element `0x10000019`, message 1, processes the entry command |
//!
//! Fixture: headless Apps with no shard link over the shipped layout; pointer and key messages
//! are built through [`dereth_client::pump::Pump`] and pushed through the application's own
//! window procedure, and every fixture path is an `expect`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::pump::{Pump, Win32Message};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::interface::ChatMessage;
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

fn require_dats() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
}

fn app_in_gameplay(frames: u32) -> App {
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
            .expect("the gameplay screen"),
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

// ---------------------------------------------------------------------------------------------
// The pointer and keyboard, driven as Windows drives them by this file's local `Hand` helper,
// following the mouse-test pattern.
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

    fn move_to(&mut self, app: &mut App, x: i32, y: i32) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(x), f64::from(y), self.time_ms);
        self.send(app, m);
    }

    fn button(&mut self, app: &mut App, pressed: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_button_message(MouseButton::Left, pressed, self.time_ms)
            .expect("the left button is in the 0x201 block");
        self.send(app, m);
    }

    fn click(&mut self, app: &mut App, at: (i32, i32)) {
        self.move_to(app, at.0, at.1);
        self.button(app, true);
        self.button(app, false);
        app.frame();
    }

    /// `WM_CHAR`, which is what `TranslateMessage` produces for a printable key press.
    fn type_text(&mut self, app: &mut App, s: &str) {
        for b in s.bytes() {
            self.time_ms += 10;
            let m = Win32Message::new(
                dereth_input::win32::msg::WM_CHAR,
                b as usize,
                0,
                self.time_ms,
            );
            self.send(app, m);
        }
        app.frame();
    }
}

// ---------------------------------------------------------------------------------------------
// The elements, as `chat::window` names them
// ---------------------------------------------------------------------------------------------

use dereth_ui_screens::chat::window::{ENTRY, LOG, NEW_TEXT_BELOW, SCROLLBAR, SEND};

/// The arrow that walks the log back is `0x10000072`, not `0x10000071`.
///
/// The scrollbar's layout moves the increment button (attribute `0x77`, here `0x10000072`) to
/// `(0, 0)` -- the top of a vertical bar -- and the decrement button (`0x78`, `0x10000071`) to
/// `(right, bottom)`. The shipped layout authors them the other way round; the runtime move
/// decides, and the layout-recovery arm performs the same two moves independently.
///
/// The increment arrow raises `0x0D`, and messages `0x0D` and `0x0F` negate the configured scroll
/// delta, so the top arrow moves the offset down toward zero, i.e. back through the log.
const ARROW_UP: ElementId = ElementId(0x1000_0072);

/// One line in the shape delivered by the broadcast-chat notice.
fn line(ty: u8, body: &str) -> ChatMessage {
    ChatMessage {
        ty,
        body: body.to_owned(),
        prefix: None,
        window: 0,
    }
}

/// Every glyph of the log, as `(char, colour)`.
fn log_glyphs(ui: &mut UiSystem, log: ElemHandle) -> Vec<(char, u32)> {
    ui.text_element_mut(log).map_or_else(Vec::new, |t| {
        t.glyphs
            .glyphs
            .iter()
            .map(|g| (char::from_u32(u32::from(g.data)).unwrap_or('?'), g.color))
            .collect()
    })
}

/// The colour every glyph of one substring is drawn in. Panics when the substring is not there,
/// which is the assertion that the line arrived at all.
fn colour_of(ui: &mut UiSystem, log: ElemHandle, needle: &str) -> u32 {
    let gs = log_glyphs(ui, log);
    let text: String = gs.iter().map(|(c, _)| *c).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the chat log; the log holds {text:?}"));
    let n = needle.chars().count();
    let colours: Vec<u32> = gs[at..at + n].iter().map(|(_, c)| *c).collect();
    assert!(
        colours.windows(2).all(|w| w[0] == w[1]),
        "{needle:?} is drawn in more than one colour: {colours:#010X?}"
    );
    colours[0]
}

fn scroll_y_of(t: &dereth_ui::text::TextElement) -> i32 {
    t.scroll.y
}

fn scroll_y(ui: &mut UiSystem, log: ElemHandle) -> i32 {
    ui.text_element_mut(log).map_or(0, |t| t.scroll.y)
}

fn scroll_extent(ui: &mut UiSystem, log: ElemHandle) -> (i32, i32) {
    ui.text_element_mut(log)
        .map_or((0, 0), |t| (t.scroll.width, t.scroll.height))
}

fn bar_float(ui: &UiSystem, bar: ElemHandle, id: u32) -> f32 {
    ui.node(bar)
        .and_then(|n| n.merged_properties().get_float(id))
        .unwrap_or(0.0)
}

/// Enough traffic to overflow the 73-pixel log, in the shape a retail session shows: the two
/// channel-join lines ACE sends first, then its three-line welcome.
fn fill_the_log(app: &mut App) {
    let lines = [
        "You have entered the Trade channel.",
        "You have entered the LFG channel.",
        "Welcome to Asheron's Call\n  powered by ACEmulator",
        "For more information on commands supported by this server, type @acehelp",
        "Ipsum tells you, one",
        "Ipsum tells you, two",
        "Ipsum tells you, three",
        "Ipsum tells you, four",
    ];
    for l in lines {
        let m = line(0, l);
        let (ui, screen) = gameplay(app);
        screen.recv_display_final_string_info(ui, &m);
    }
    app.frame();
}

// ---------------------------------------------------------------------------------------------
// 1. Which element is the entry, and is it editable as shipped?
// ---------------------------------------------------------------------------------------------

/// The chat entry is editable as shipped and the log is not, read off the shipped layout.
///
/// The two elements are both text elements and they are not interchangeable:
///
/// * `0x10000011` -- 368 x 73, `0x27` only, and `0x72 = 0x10000012`: a scrollable log.
/// * `0x10000016` -- 306 x 17, `0x16` and `0x20` and `0x27`, `0x1E = 255`: an entry.
///
/// Falsified by pointing either constant at the other id.
#[test]
fn the_shipped_chat_entry_is_editable_and_the_log_is_not() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let (entry, log, bar) = (find(&app, ENTRY), find(&app, LOG), find(&app, SCROLLBAR));

    let eb = app.ui().expect("shell").ui.screen_box(entry);
    let lb = app.ui().expect("shell").ui.screen_box(log);
    assert_eq!(
        (eb.width(), eb.height()),
        (306, 17),
        "the entry is the input line's box"
    );
    assert_eq!(
        (lb.width(), lb.height()),
        (368, 73),
        "the log is the scrollback's box"
    );

    let (ui, _) = gameplay(&mut app);
    let e = ui
        .text_element_mut(entry)
        .expect("the entry is a text element")
        .bits;
    assert!(
        e.editable(),
        "the shipped chat entry carries Editable (0x16)"
    );
    assert!(e.one_line(), "and 0x20, one line");
    assert!(e.selectable(), "and 0x27");
    let l = ui
        .text_element_mut(log)
        .expect("the log is a text element")
        .bits;
    assert!(l.selectable(), "the log is sweep-selectable");
    assert!(!l.editable(), "and is NOT editable");

    // Attribute `0x72` names the vertical scrollbar, the binding that makes the bar's messages
    // reachable at all because the log and bar are siblings.
    let v = ui
        .text_element_mut(log)
        .expect("a text element")
        .scroll
        .v_scrollbar;
    assert_eq!(
        v,
        Some(SCROLLBAR),
        "the log names its vertical scrollbar with attribute 0x72"
    );
    assert_eq!(
        ui.node(bar).expect("the bar").desc.ty.0,
        0x0B,
        "and it is a scrollbar element"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 2. Colour: a derived value a screenshot cannot check
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.log.two-recorded-kinds-of-line-draw-in-two-different-colours
///
/// The colour mapping, channel by channel, against the client's colour table.
///
/// A chat line in the wrong colour looks entirely plausible and misleads a player about who is
/// talking to them, so every claim here is a type -> RGB pair taken from the 34-slot colour
/// lookup rather than from a screenshot.
///
/// The ACE welcome is a `Broadcast` (type 0) line and is drawn green,
/// which is this table's default fill, `0x80FF7F`, not the log element's `font_color`
/// `0xFFC6C6CC`.
///
/// Falsified by deleting the chat window's `build_chat_color_lookup_table` call: with no table on
/// the element `TextElement::chat_color` falls back to `font_color` and every assertion below
/// fails at once.
#[test]
fn a_line_is_drawn_in_its_own_channels_colour_and_a_prefix_is_always_grey() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);

    // Every type whose colour differs from the default, plus the default itself. The right-hand
    // column is the `chat::colors` table, which its module asserts slot by slot.
    let cases: [(u8, &str, u32); 8] = [
        (0, "broadcast", 0xFF80_FF7F), // green — the fill, and ACE's welcome line
        (2, "speech", 0xFFFF_FFFF),    // white
        (3, "tell", 0xFFFF_FF3F),      // yellow
        (5, "system", 0xFFFF_7FFF),    // bright purple
        (6, "combat", 0xFFFF_3F3F),    // dark red
        (7, "magic", 0xFF3F_BFFF),     // light blue
        (0x12, "allegiance", 0xFFEE_921E), // orange
        (0x0D, "advancement", 0xFF3F_DCDC), // cyan
    ];
    for (ty, body, want) in cases {
        let m = ChatMessage {
            ty,
            body: body.to_owned(),
            prefix: None,
            window: 0,
        };
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, &m);
        let got = colour_of(ui, log, body);
        assert_eq!(got, want, "chat type {ty:#04X} ({body})");
    }

    // The display-notice path appends the speaker prefix with colour index 12 (grey), whatever
    // the message's own type is. This is asserted against a `Speech` (white body) so the two
    // cannot be confused.
    let m = ChatMessage {
        ty: 2,
        body: "hello there".to_owned(),
        prefix: Some("Tarinell".to_owned()),
        window: 0,
    };
    {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, &m);
        assert_eq!(
            colour_of(ui, log, "Tarinell"),
            0xFFD2_D2C8,
            "the prefix uses the grey table entry"
        );
        assert_eq!(
            colour_of(ui, log, "hello there"),
            0xFFFF_FFFF,
            "and the body is its own type"
        );
    }
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 3. Anchoring
// ---------------------------------------------------------------------------------------------

/// The newest line is at the bottom of the pane, not sliced off it.
///
/// The display-notice tail checks whether the view is at its vertical end before appending and,
/// when it was, scrolls to the new glyph count afterward.
///
/// Falsified by deleting the `if was_at_end { scroll_to_end }` arm: `scroll.y` stays 0 and the
/// last glyph is out of view.
#[test]
fn the_newest_line_is_at_the_bottom_and_the_log_stays_at_the_end() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    let view = app.ui().expect("shell").ui.screen_box(log).height();

    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            scroll_y(ui, log),
            0,
            "an empty log is at the top and the bottom at once"
        );
    }
    fill_the_log(&mut app);

    let (ui, _) = gameplay(&mut app);
    let (_, content) = scroll_extent(ui, log);
    assert!(
        content > view,
        "the four lines overflow the 73-pixel pane ({content} > {view})"
    );
    // The offset is the whole travel: the last line sits on the pane's bottom edge.
    assert_eq!(
        scroll_y(ui, log),
        content - view,
        "the log is scrolled to its end, which is what pins the newest line to the bottom"
    );
    let screen = ui.screen_box(log);
    let t = ui.text_element_mut(log).expect("a text element");
    assert!(
        t.is_at_vertical_end(screen),
        "the end-of-content predicate agrees"
    );
    let last = t.glyphs.len() - 1;
    assert!(
        t.is_position_in_view(screen, last),
        "and the final glyph is inside the pane"
    );
    // …and the *first* glyph is not, which is the half a top-pinned log got right by accident.
    assert!(
        !t.is_position_in_view(screen, 0),
        "the oldest line has scrolled off the top"
    );

    // What a player sees, not merely what the offset says: the composed draw list. The client
    // scrolls by moving the glyphs, so the topmost placed glyph must be drawn above the content
    // box's own top edge. With no offset applied to the placement it would sit exactly on it,
    // which is a top-pinned log.
    let content = t.content_box(screen);
    let placed = t.compose(screen);
    assert!(!placed.is_empty(), "the log composes glyphs");
    let top = placed.iter().map(|g| g.y).min().expect("a glyph");
    let bottom = placed.iter().map(|g| g.y).max().expect("a glyph");
    assert!(
        top < content.y0,
        "the oldest line is drawn above the pane ({top} vs {})",
        content.y0
    );
    assert_eq!(
        content.y0 - top,
        scroll_y_of(t),
        "and by exactly the scroll offset"
    );
    assert!(bottom < content.y1, "the newest line is drawn inside it");
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 4. The scrollbar: the arithmetic
// ---------------------------------------------------------------------------------------------

/// The bar position and the scroll offset are exact inverses.
///
/// The scrollbar update writes `position = offset / (content - view)` and message `0x0A` reads
/// `offset = ftol(position * (content - view))`, the second pinned by being the first formula's
/// inverse. Both ends of the range and the middle are asserted.
///
/// Falsified by changing either divisor: the round trip stops closing.
#[test]
fn the_bar_position_and_the_scroll_offset_round_trip() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let (log, bar) = (find(&app, LOG), find(&app, SCROLLBAR));
    let view = app.ui().expect("shell").ui.screen_box(log).height();
    fill_the_log(&mut app);

    let (ui, _) = gameplay(&mut app);
    let (_, content) = scroll_extent(ui, log);
    let travel = content - view;
    assert!(travel > 0, "there is something to scroll");

    for offset in [0, travel / 3, travel / 2, travel] {
        // Offset to position, as the scrollbar update writes it.
        let mut s = ui.text_element_mut(log).expect("a text element").scroll;
        s.set_scrollable_xy(ui, log, 0, offset, true);
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = s;
        }
        let pos = bar_float(ui, bar, dereth_ui::widgets::scrollbar::attr::POSITION);
        #[allow(clippy::cast_precision_loss)]
        let want = offset as f32 / travel as f32;
        assert!(
            (pos - want).abs() < 1e-4,
            "offset {offset} -> position {pos}, wanted {want}"
        );

        // Position to offset, as message `0x0A` reads it
        // back. The offset is parked away from the answer first, so a stale value cannot pass.
        let mut s = ui.text_element_mut(log).expect("a text element").scroll;
        s.y = if offset == travel { 0 } else { travel };
        ui.set_attribute_float(bar, dereth_ui::widgets::scrollbar::attr::POSITION, pos);
        s.handle_scrollbar_message(
            ui,
            log,
            false,
            dereth_ui::msg::element::id::SCROLL_POSITION,
            0,
        );
        // The float-to-int conversion truncates towards zero, so the round trip is exact at both
        // ends and can lose one pixel in between: `7/23 = 0.3043478`, and `ftol(0.3043478 * 23)`
        // is 6, not 7. That is the client's own arithmetic, asserted as the formula rather than
        // papered over with a tolerance.
        #[allow(clippy::cast_precision_loss)]
        let want_back = dereth_primitives::num::to_i32(pos * travel as f32);
        assert_eq!(s.y, want_back, "position {pos} -> ftol(pos x travel)");
        assert!(
            (s.y - offset).abs() <= 1,
            "and it lands within one pixel of the offset it came from ({} vs {offset})",
            s.y
        );
        if offset == 0 || offset == travel {
            assert_eq!(s.y, offset, "the two ends round-trip exactly");
        }
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = s;
        }
    }

    // The clamp, past both ends. Attribute `0x73` is read into a local initialized to 1, so an
    // absent attribute still enables clamping.
    let mut s = ui.text_element_mut(log).expect("a text element").scroll;
    s.set_scrollable_xy(ui, log, 0, travel + 1000, false);
    assert_eq!(s.y, travel, "past the end clamps to the travel");
    s.set_scrollable_xy(ui, log, 0, -1000, false);
    assert_eq!(s.y, 0, "and before the start clamps to 0");
    app.shutdown();
}

/// The thumb is sized to the proportion the content actually overflows by, and the bar stops
/// being disabled once there is something to scroll.
///
/// `proportion = min(view / content, 1.0)`, attribute `0x88`, and `disabled` (`0x76`) is set
/// exactly when `content <= view`. The shipped layout starts the chat bar at `0x88 = 1.0` and
/// `0x76 = true`, which is right for an empty log: an empty scrollbar track.
///
/// Falsified by deleting the chat window's `update_scrollable_area` call: the content extent stays
/// 0, the proportion stays 1.0 and the bar stays disabled.
#[test]
fn the_thumb_is_sized_to_its_proportion_once_the_log_overflows() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let (log, bar) = (find(&app, LOG), find(&app, SCROLLBAR));
    let view = app.ui().expect("shell").ui.screen_box(log).height();
    let track = {
        let ui = &app.ui().expect("shell").ui;
        let b = ui.screen_box(bar);
        let up = ui.screen_box(find(&app, ARROW_UP));
        b.height() - 2 * up.height()
    };

    {
        let ui = &app.ui().expect("shell").ui;
        assert!(
            (bar_float(ui, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION) - 1.0).abs()
                < 1e-6,
            "an empty log fills its own bar"
        );
    }

    fill_the_log(&mut app);

    let (ui, _) = gameplay(&mut app);
    let (_, content) = scroll_extent(ui, log);
    let prop = bar_float(ui, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION);
    #[allow(clippy::cast_precision_loss)]
    let want = view as f32 / content as f32;
    assert!(
        (prop - want).abs() < 1e-4,
        "proportion {prop}, wanted view/content = {want}"
    );
    assert!(prop < 1.0, "and it is smaller than the whole bar");
    assert_eq!(
        ui.node(bar).and_then(|n| n
            .merged_properties()
            .get_bool(dereth_ui::widgets::scrollbar::attr::DISABLED)),
        Some(false),
        "a bar with travel is not disabled"
    );

    // The thumb the player sees is sized to `proportion * track`, floored at the configured minimum
    // widget size (`0x89 = 16` in this layout).
    let thumb = ui
        .get_child(bar, ElementId(1))
        .expect("the thumb is child id 1");
    let h = ui.screen_box(thumb).height();
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let expect = ((prop * track as f32) as i32).max(16);
    assert_eq!(
        h, expect,
        "the thumb is {expect} of the {track}-pixel track"
    );
    assert!(h < track, "and it no longer fills it");
    app.shutdown();
}

/// Two clicks on the up-arrow reveal two earlier lines.
///
/// The chain starts with hot click `0x02` (attribute `0x0F`, carried by the shipped arrows). The
/// scrollbar turns it into a one-step move and raises element message `0x0D`; the scrollable text
/// element obtains the non-page delta and applies the new offset.
///
/// Falsified by deleting the scrollbar arm of `TextElement::listen_to_element_message`: the
/// message is still raised and the offset never moves.
#[test]
fn two_clicks_on_the_up_arrow_walk_the_log_two_lines_back() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    fill_the_log(&mut app);

    let at_end = {
        let (ui, _) = gameplay(&mut app);
        scroll_y(ui, log)
    };
    assert!(at_end > 0, "the log starts scrolled to its end");

    // One line's worth: the configured delta for a non-page step.
    let step = {
        let (ui, _) = gameplay(&mut app);
        let screen = ui.screen_box(log);
        let t = ui.text_element_mut(log).expect("a text element");
        t.inq_scroll_delta(screen, false, false, false)
    };
    assert!(step > 0, "a line has a height");

    let arrow = centre(&app, find(&app, ARROW_UP));
    let mut hand = Hand::new();
    hand.click(&mut app, arrow);
    let after_one = {
        let (ui, _) = gameplay(&mut app);
        scroll_y(ui, log)
    };
    assert_eq!(after_one, at_end - step, "one click walks one line back");

    hand.click(&mut app, arrow);
    let after_two = {
        let (ui, _) = gameplay(&mut app);
        scroll_y(ui, log)
    };
    assert_eq!(
        after_two,
        after_one - step,
        "and the second click walks another"
    );
    {
        let (ui, _) = gameplay(&mut app);
        let screen = ui.screen_box(log);
        let t = ui.text_element_mut(log).expect("a text element");
        assert!(
            !t.is_at_vertical_end(screen),
            "the log is no longer at its end"
        );
    }
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 5. Typing, and sending
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.window.scrolls-colours-each-line-and-sends-what-is-typed
///
/// Click in the box, type, press Send.
///
/// Nothing here sets `Editable`: the shipped `0x10000016` already carries it.
///
/// Falsified by pointing `chat::window::ENTRY` at `0x10000011`: the click still focuses (a log is
/// selectable) and the characters insert nothing.
#[test]
fn a_click_and_a_typed_line_and_the_send_button_emit_the_chat_line() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);

    let mut hand = Hand::new();
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }

    let _ = app.ui_mut().expect("the UI shell is up").ui.requests.take();
    hand.type_text(&mut app, "hello world");
    {
        let (ui, _) = gameplay(&mut app);
        let t = ui.text_element_mut(entry).expect("a text element");
        assert_eq!(
            t.glyphs.inq_text(false),
            "hello world",
            "the box holds what was typed"
        );
    }

    // The click itself, through the pump. `App::frame` drains `dereth_ui_screens::requests` into
    // the host every frame, so what the gesture left behind is asserted on the screen's own state,
    // and the request's *shape* is taken from the same arm the click ran, below.
    let send_at = centre(&app, find(&app, SEND));
    hand.click(&mut app, send_at);
    {
        let (ui, screen) = gameplay(&mut app);
        let t = ui.text_element_mut(entry).expect("a text element");
        assert_eq!(t.glyphs.len(), 0, "sending emptied the entry box");
        let main = screen
            .chat
            .iter()
            .find(|c| c.window_id == 8)
            .expect("the main window");
        assert_eq!(
            main.history,
            vec!["hello world".to_owned()],
            "and it went into the history"
        );
        assert_eq!(main.history_pos, None, "and the browse position was reset");
    }

    // The Send click takes focus off the entry. A button's mouse-down takes focus the way a text
    // element's does before doing anything of its own, and the Send arm for element `0x10000019`
    // processes the command and nothing else; it does not reactivate the chat entry. So after
    // pressing Send the player clicks the entry again, or invokes the chat-entry focus action,
    // before the next line can be typed. The focus move is asserted in both directions, and the
    // re-click a player has to make is spelled out.
    {
        let (ui, _) = gameplay(&mut app);
        assert_ne!(
            ui.focus_element(),
            Some(entry),
            "the Send button took the focus"
        );
    }
    let back = centre(&app, entry);
    hand.click(&mut app, back);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "and a second click gives it back"
        );
    }

    // The request that arm raises comes from the chat window's `listen_to_element_message` method, the same code
    // the click just ran, called directly so the frame cannot drain the queue underneath it.
    hand.type_text(&mut app, "second line");
    let (ui, screen) = gameplay(&mut app);
    let w = screen.chat_windows[0];
    let r = w
        .listen_to_element_message(
            ui,
            &mut screen.chat[0],
            dereth_ui_screens::chat::window::SEND,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
        )
        .expect("the Send arm raises a chat line");
    assert!(
        matches!(&r, dereth_ui_screens::view::UiRequest::ChatLine { text, window }
            if text == "second line" && *window == 8),
        "...carrying the typed text for the main window: {r:?}"
    );
    app.shutdown();
}

/// **The "new text below" arrow**: a line that lands while the player has scrolled up raises it
/// and does *not* move the view, and clicking it jumps to the end and takes it down.
///
/// The display-notice tail distinguishes whether the view was already at the end, and element
/// `0x1000048C` handles the jump-to-end click.
///
/// Falsified by removing the `was_at_end` test: the view follows every line and the arrow never
/// comes up.
#[test]
fn a_line_that_lands_while_scrolled_up_raises_the_arrow_and_does_not_move_the_view() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    fill_the_log(&mut app);

    let arrow = centre(&app, find(&app, ARROW_UP));
    let mut hand = Hand::new();
    hand.click(&mut app, arrow);
    hand.click(&mut app, arrow);
    let parked = {
        let (ui, _) = gameplay(&mut app);
        scroll_y(ui, log)
    };

    let m = line(0, "a line the player is not looking at");
    {
        let (ui, screen) = gameplay(&mut app);
        screen.recv_display_final_string_info(ui, &m);
        assert_eq!(
            scroll_y(ui, log),
            parked,
            "the view stayed where the player put it"
        );
        let main = screen
            .chat
            .iter()
            .find(|c| c.window_id == 8)
            .expect("the main window");
        assert!(main.new_non_visible_text, "the new-text-below flag is up");
    }
    app.frame();

    // The arrow itself scrolls to the final glyph and then hides.
    let arrow_at = centre(&app, find(&app, NEW_TEXT_BELOW));
    hand.click(&mut app, arrow_at);
    let (ui, screen) = gameplay(&mut app);
    let screen_box = ui.screen_box(log);
    let t = ui.text_element_mut(log).expect("a text element");
    assert!(
        t.is_at_vertical_end(screen_box),
        "clicking the arrow jumped the log to its end"
    );
    let main = screen
        .chat
        .iter()
        .find(|c| c.window_id == 8)
        .expect("the main window");
    assert!(!main.new_non_visible_text, "and took the flag down");
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 6. The wire — chat command dispatch's one external call
// ---------------------------------------------------------------------------------------------

/// A typed line has an owner and reaches the wire slot.
///
/// The Send arm hands the raw string and window id to the chat-command interpreter. This asserts
/// that the `UiRequest::ChatLine` request has an owner, the interpreter classified the line, and
/// a `dereth_client_model::Request` reached the wire slot.
///
/// The two classifications are opposite in an important way: plain text is speech, and an
/// unrecognised `@` command is forwarded to the server verbatim, `@` and all, with no allow-list,
/// which permits server-defined commands such as `@acehelp`. A locally handled verb is neither,
/// and must not be spoken.
///
/// Falsified by deleting the `UiRequest::ChatLine` arm from `run_ui_requests`: every line comes
/// back unowned and nothing reaches the wire slot.
#[test]
fn a_chat_line_is_classified_and_offered_to_the_wire() {
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats");
    for (line, wire) in [
        ("hello world", true), // Chat { Say } -> Communication_Talk
        ("@acehelp", true),    // ForwardVerbatim -> Communication_Talk, '@' and all
        ("@help", false),      // a registered handler: local, and never spoken
        ("", false),           // Empty
    ] {
        let mut inter = dereth_client::interaction::Interaction::new();
        let mut objects = dereth_client::objects::ObjectStream::new();
        inter.queue(
            Vec::new(),
            vec![dereth_ui_screens::view::UiRequest::ChatLine {
                text: line.to_owned(),
                window: 8,
            }],
        );
        let (unowned, _) = dereth_client::interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            dereth_primitives::LocalTime(1.0),
        );
        assert!(
            unowned.is_empty(),
            "{line:?} is nobody's request: {unowned:?}"
        );
        assert_eq!(
            inter.stats.chat_lines_sent, 1,
            "{line:?} went through chat-command classification"
        );
        // With no session the wire slot counts it undeliverable, which is the assertion that a
        // `dereth_client_model::Request` was built and offered at all.
        assert_eq!(
            inter.stats.requests_undeliverable,
            u64::from(wire),
            "{line:?}: {} request(s) reached the wire slot, wanted {}",
            inter.stats.requests_undeliverable,
            u64::from(wire)
        );
    }
}
