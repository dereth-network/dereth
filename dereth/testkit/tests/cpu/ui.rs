//! The interface layer with no game model behind it: the clipboard, which surface takes a line
//! the client writes for itself, and the shape and zone a date is drawn in.
//!
//! The clipboard's drive is a `UiSystem` and the client's clipboard bridge rather than a
//! `HeadlessClient` step: the selection is made with real mouse messages into the element tree,
//! and the bridge is what a frame runs over it. The behaviour is asserted at the end through a
//! model client, which is how a scenario whose subject is not the game model still books its
//! claim.

use dereth_client::clipboard::{ClipboardBridge, FakeClipboard};
use dereth_primitives::{AssetError, AssetSource, DataId, DataType, LocalTime};
use dereth_testkit::HeadlessClient;
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::{action, InputEvent};
use dereth_ui::{ElemHandle, ElementId, ElementType, InputPump, UiSystem};

/// No dats: every element below is built from a description written here.
#[derive(Debug)]
struct NoAssets;

impl AssetSource for NoAssets {
    fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError> {
        Err(AssetError::NotFound(id))
    }
    fn exists(&self, _: DataId) -> bool {
        false
    }
    fn iter_type(&self, _: DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

#[derive(Debug, Default)]
struct Pump;

impl InputPump for Pump {
    fn use_time(&mut self, _now: LocalTime) {}
    fn shift_key_down(&self) -> bool {
        false
    }
}

const CONTAINER: u32 = 0x1000_0010;
const LOG: u32 = 0x1000_0011;

fn desc(id: u32, ty: ElementType, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x: 0,
            y: 0,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

/// The shipped chat log: selectable, not editable.
fn chat_log(ui: &mut UiSystem, text: &str) -> ElemHandle {
    let mut container = desc(CONTAINER, ty::FIELD, 400, 200);
    let mut t = desc(LOG, ty::TEXT, 200, 40);
    t.base.properties.set(
        dereth_ui::props::attr::TEXT_SELECTABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    container.children.insert(ElementId(LOG), t);
    let layout = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = layout
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("the root is there");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &d)
        .expect("nothing is inherited")
        .expect("the element registers");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    ui.initialize_tree(c);
    let h = ui
        .get_child(c, ElementId(LOG))
        .expect("the child was built");
    ui.text_element_mut(h).expect("a text half").set_text(text);
    h
}

const fn at(i: i32) -> i32 {
    i * 8
}

/// Sweep the mouse from glyph `a` to glyph `b`.
fn sweep(ui: &mut UiSystem, a: i32, b: i32) {
    ui.mouse_down(action::PRIMARY_CLICK, at(a), 8);
    ui.mouse_move(LocalTime(1.0), at(b), 8);
    ui.mouse_up(action::PRIMARY_CLICK, at(b), 8, false);
}

fn a_chat_window() -> (UiSystem, ElemHandle) {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let mut pump = Pump;
    ui.use_time(LocalTime(0.0), &mut pump);
    let log = chat_log(&mut ui, "abcdefgh");
    (ui, log)
}

fn copy(ui: &mut UiSystem, log: ElemHandle) {
    ui.dispatch_action(
        log,
        &InputEvent {
            action: action::COPY,
            start: true,
            x: 0,
            y: 0,
        },
    );
}

/// A copy hands the selection to the desktop once, and a copy with nothing selected leaves the
/// desktop alone.
pub fn a_copy_reaches_the_host_clipboard_once() {
    // End to end: sweep four glyphs, press copy, run the bridge.
    let (mut ui, log) = a_chat_window();
    let mut host = FakeClipboard::default();
    let mut bridge = ClipboardBridge::default();
    sweep(&mut ui, 1, 5);
    copy(&mut ui, log);
    // Ten frames, because the pending write is a **take**: a bridge that re-sent it would take
    // the desktop's clipboard away from every other application sixty times a second.
    for _ in 0..10 {
        bridge.sync(&mut ui, &mut host);
    }
    let sent_once = host.sent == vec!["bcde".to_owned()] && bridge.sends == 1;

    // A copy with nothing selected must not empty what the player copied elsewhere. Getting this
    // wrong silently destroys their data.
    let (mut ui, log) = a_chat_window();
    let mut host = FakeClipboard {
        contents: Some("something the player copied elsewhere".to_owned()),
        seq: Some(7),
        ..FakeClipboard::default()
    };
    let mut bridge = ClipboardBridge::default();
    copy(&mut ui, log);
    bridge.sync(&mut ui, &mut host);
    let left_alone = host.sent.is_empty()
        && host.contents.as_deref() == Some("something the player copied elsewhere");

    // Text copied in another application becomes available to paste, and the desktop is opened
    // only when its contents actually changed.
    let (mut ui, _log) = a_chat_window();
    let mut host = FakeClipboard {
        contents: Some("from a browser".to_owned()),
        seq: Some(1),
        ..FakeClipboard::default()
    };
    let mut bridge = ClipboardBridge::default();
    for _ in 0..30 {
        bridge.sync(&mut ui, &mut host);
    }
    let mirrored = ui.clipboard == "from a browser" && host.reads == 1 && bridge.refreshes == 1;
    host.contents = Some("new".to_owned());
    host.seq = Some(5);
    bridge.sync(&mut ui, &mut host);
    let noticed = host.reads == 2 && ui.clipboard == "new";

    // …and a format the client cannot read does not wipe what it already had.
    let (mut ui, log) = a_chat_window();
    let mut host = FakeClipboard::default();
    let mut bridge = ClipboardBridge::default();
    sweep(&mut ui, 1, 5);
    copy(&mut ui, log);
    bridge.sync(&mut ui, &mut host);
    let ours = ui.clipboard == "bcde";
    host.contents = None;
    host.seq = Some(host.seq.unwrap_or(0) + 1);
    bridge.sync(&mut ui, &mut host);
    let survived = ui.clipboard == "bcde";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "clipboard.copy.hands-the-selection-to-the-host-once",
        move |_| sent_once && left_alone && mirrored && noticed && ours && survived,
    );
}

// -------------------------------------------------------------------------------------------

dereth_testkit::scenarios! {
    scenario_a_copy_reaches_the_host_clipboard_once => a_copy_reaches_the_host_clipboard_once ["clipboard.copy.hands-the-selection-to-the-host-once"],
    scenario_the_strip_takes_the_clients_own_channel_and_the_windows_take_the_rest => the_strip_takes_the_clients_own_channel_and_the_windows_take_the_rest ["notice.the-strip-takes-the-clients-own-channel-and-the-chat-windows-take-the-rest"],
    scenario_a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows => a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows ["notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows"],
    scenario_the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in => the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in ["dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in"],
    scenario_a_pane_draws_in_whatever_zone_it_is_handed_row_by_row => a_pane_draws_in_whatever_zone_it_is_handed_row_by_row ["dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row"],
}

// =============================================================================================
// notice.* -- where a line the client writes for itself goes
//
// A refusal the client composes has to be drawn somewhere; a client can compute, raise and count
// one and still drop it. The row `notice.refusal.is-a-bubble-and-not-a-chat-line` is booked in the
// `dat` tier, read off the drawn elements rather than the model, with one more notice claim beside
// it; a sweep of the whole recorded corpus for refusals is not here, because it needs a replay of
// the raw recordings on disk, which the harness does not read.
//
// Neither of these needs a data file: what they are about is which surface takes which channel,
// and that is the client's own routing.
// =============================================================================================

use dereth_ui_screens::chat::interface::{window, ChatInterface, Routed};
use dereth_ui_screens::hud::speech_bubbles::BUBBLE_CHAT_TYPE;

// =============================================================================================
// notice.the-strip-takes-the-clients-own-channel-and-the-chat-windows-take-the-rest
// =============================================================================================

/// Both directions, because a client that simply put every line in the strip would pass a
/// one-directional test.
pub fn the_strip_takes_the_clients_own_channel_and_the_windows_take_the_rest() {
    let mut c = HeadlessClient::model();
    // One line on an ordinary broadcast channel and one on the channel the client talks to
    // itself on, in that order.
    c.world_mut()
        .scroll
        .add_text_to_scroll("a broadcast", 0, true, 0);
    c.world_mut()
        .scroll
        .add_text_to_scroll("a refusal", u32::from(BUBBLE_CHAT_TYPE), true, 0);
    c.tick(1);

    let both_reached_the_fan_out = c.view().hud().stats.scroll_lines == 2;
    let the_strip_took_one = c.view().hud().stats.spew_lines == 1
        && c.view().hud().panels.spew.model.pending == vec!["a refusal".to_owned()];

    // The chat half: both lines are offered to the windows, and each window's own filter decides.
    let lines: Vec<dereth_ui_screens::chat::interface::ChatMessage> = c.chat_lines().to_vec();
    let both_offered = lines.len() == 2 && lines[1].ty == BUBBLE_CHAT_TYPE;

    let mut main = ChatInterface::new(window::MAIN);
    let the_window_takes_the_broadcast =
        main.recv_display_final_string_info(&lines[0], true) == Routed::Accepted;
    let and_drops_the_clients_own =
        main.recv_display_final_string_info(&lines[1], true) == Routed::FilteredOut;
    let no_floaty_carries_it = [
        window::FLOATY_1,
        window::FLOATY_2,
        window::FLOATY_3,
        window::FLOATY_4,
    ]
    .into_iter()
    .all(|w| ChatInterface::new(w).route(&lines[1]) == Routed::FilteredOut);

    // ...and a player who asks for that kind of line gets it in the window as well.
    let mut asked = ChatInterface::new(window::MAIN);
    asked.filter |= 0x0400_0000;
    let asking_for_it_brings_it_back =
        asked.recv_display_final_string_info(&lines[1], true) == Routed::Accepted;

    c.assert_behaviour(
        "notice.the-strip-takes-the-clients-own-channel-and-the-chat-windows-take-the-rest",
        move |_| {
            both_reached_the_fan_out
                && the_strip_took_one
                && both_offered
                && the_window_takes_the_broadcast
                && and_drops_the_clients_own
                && no_floaty_carries_it
                && asking_for_it_brings_it_back
        },
    );
}

// =============================================================================================
// notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows
// =============================================================================================

/// The line is drained and *then* discarded, which is the client's own order -- so the counter and
/// the queue disagree here on purpose.
pub fn a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows() {
    let mut c = HeadlessClient::model();
    c.world_mut()
        .scroll
        .add_text_to_scroll("a refusal", u32::from(BUBBLE_CHAT_TYPE), true, 0);
    let one_is_waiting = c.view().world().scroll.pending().len() == 1;

    c.when(dereth_testkit::Inbound::event(
        dereth_client_net::client_session::SessionEvent::LoggedOff,
    ));

    let the_queue_is_empty = c.view().world().scroll.pending().is_empty();
    let the_strip_is_empty = c.view().hud().panels.spew.model.pending.is_empty();
    // It was counted on the way past: the drain runs before the log-off arm.
    let it_was_counted = c.chat_lines().len() == 1;

    c.assert_behaviour(
        "notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows",
        move |_| one_is_waiting && the_queue_is_empty && the_strip_is_empty && it_was_counted,
    );
}

// =============================================================================================
// dates.* -- the shape a date is drawn in, and the zone it is drawn for
//
// The same house at the same instant can read seven hours apart and in two different shapes.
// These two claims are arithmetic over the client's own formatter and over the view a pane is
// handed, and need no data file; the surfaces that draw a date are in the `dat` tier.
//
// **No assertion here reads the machine's own zone.** Every zone below is a number this file
// writes out, so these say the same thing wherever they are run.
// =============================================================================================

use dereth_ui_screens::ctime::{asctime, strftime_c};
use dereth_ui_screens::panels::house;

/// The instant a pair of retail screenshots shows.
const THE_INSTANT: i64 = 1_789_404_178;
/// The zone that machine was in, written out rather than read.
const WEST: i32 = -7 * 3600;
/// A zone on the far side of the date line, so a sign error cannot cancel out.
const EAST: i32 = 9 * 3600;
/// What the shipped runtime writes for that instant in that zone, and what this client used to
/// write instead.
const WHAT_IT_SHOULD_SAY: &str = "9/14/2026 9:42:58 AM";
const WHAT_IT_USED_TO_SAY: &str = "Mon Sep 14 16:42:58 2026";

// =============================================================================================
// dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in
// =============================================================================================

/// The values are the shipped runtime's own answers, taken from it once and written down here, so
/// this is a claim about the client and not a restatement of it.
pub fn the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in() {
    // The pair from the screenshot, on one line each.
    let the_pair = strftime_c(THE_INSTANT, WEST) == WHAT_IT_SHOULD_SAY
        && asctime(THE_INSTANT, 0) == WHAT_IT_USED_TO_SAY
        && strftime_c(THE_INSTANT, WEST) != asctime(THE_INSTANT, WEST);

    // Eight instants, including both ends of the twelve-hour clock, a leap day and the far end of
    // a thirty-two-bit clock.
    let the_shape = [
        (0_i64, "1/1/1970 12:00:00 AM"),
        (43_200, "1/1/1970 12:00:00 PM"),
        (46_800, "1/1/1970 1:00:00 PM"),
        (86_399, "1/1/1970 11:59:59 PM"),
        (951_782_400, "2/29/2000 12:00:00 AM"),
        (1_789_404_178, "9/14/2026 4:42:58 PM"),
        (1_788_565_581, "9/4/2026 11:46:21 PM"),
        (2_147_483_647, "1/19/2038 3:14:07 AM"),
    ]
    .into_iter()
    .all(|(t, want)| strftime_c(t, 0) == want);

    // The zone is a shift and not a label: east and west, across midnight in both directions.
    let a_real_shift = strftime_c(0, -3600) == "12/31/1969 11:00:00 PM"
        && strftime_c(0, EAST) == "1/1/1970 9:00:00 AM"
        && strftime_c(THE_INSTANT, EAST) == "9/15/2026 1:42:58 AM";

    // The other shape pads the day with a zero. The one character that separates the runtime the
    // client ships with from the one this workspace is built on.
    let the_other_shape =
        asctime(0, 0) == "Thu Jan 01 00:00:00 1970" && !asctime(0, 0).contains("Jan  1");

    // ...and no date in this shape ever carries a weekday or a month's name, at any zone and any
    // instant, which is what catches a surface reaching for the other one.
    let mut never_the_other = true;
    for off in [0, WEST, EAST] {
        for t in [0_i64, THE_INSTANT, 1_788_565_581, 2_147_483_647] {
            let c = strftime_c(t, off);
            never_the_other &= ![
                "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun", "Jan", "Sep", "Dec",
            ]
            .iter()
            .any(|w| c.contains(w))
                && (c.ends_with(" AM") || c.ends_with(" PM"));
        }
    }

    // The two answers that are not a date at all: nothing recorded, and nothing to say.
    let the_two_non_dates =
        house::convert_time(0, WEST) == "N/A" && house::convert_time(-1, WEST).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in",
        move |_| {
            the_pair
                && the_shape
                && a_real_shift
                && the_other_shape
                && never_the_other
                && the_two_non_dates
        },
    );
}

// =============================================================================================
// dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row
// =============================================================================================

/// Driven at zones this machine is not in, which is the point: a host on the far side of the
/// world gets that side's sheet.
pub fn a_pane_draws_in_whatever_zone_it_is_handed_row_by_row() {
    use dereth_ui_screens::view::{HouseDataView, HousePurchaseView};

    let mut every_row_follows = true;
    for (off, want) in [
        (0, "9/14/2026 4:42:58 PM"),
        (WEST, WHAT_IT_SHOULD_SAY),
        (EAST, "9/15/2026 1:42:58 AM"),
    ] {
        let d = HouseDataView {
            buy_time: THE_INSTANT,
            maintenance_period_end: THE_INSTANT,
            maintenance_next_due: THE_INSTANT,
            utc_offset_secs: [off; 3],
            ..HouseDataView::default()
        };
        every_row_follows &= house::convert_time(d.buy_time, d.utc_offset_secs[0]) == want
            && house::convert_time(d.maintenance_period_end, d.utc_offset_secs[1]) == want
            && house::convert_time(d.maintenance_next_due, d.utc_offset_secs[2]) == want;
        let p = HousePurchaseView {
            utc_offset_secs: off,
            ..HousePurchaseView::default()
        };
        every_row_follows &= strftime_c(THE_INSTANT, p.utc_offset_secs) == want;
    }

    // Three rows, three zones at once -- which is why the zone is a row's and not a pane's: each
    // row is worked out on its own.
    let d = HouseDataView {
        buy_time: THE_INSTANT,
        maintenance_period_end: THE_INSTANT,
        maintenance_next_due: THE_INSTANT,
        utc_offset_secs: [0, WEST, EAST],
        ..HouseDataView::default()
    };
    let three: Vec<String> = [0_usize, 1, 2]
        .map(|i| {
            house::convert_time(
                [d.buy_time, d.maintenance_period_end, d.maintenance_next_due][i],
                d.utc_offset_secs[i],
            )
        })
        .to_vec();
    let three_at_once = three
        == vec![
            "9/14/2026 4:42:58 PM".to_owned(),
            WHAT_IT_SHOULD_SAY.to_owned(),
            "9/15/2026 1:42:58 AM".to_owned(),
        ];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row",
        move |_| every_row_follows && three_at_once,
    );
}
