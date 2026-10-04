//! UI fixtures and scenarios for dates.

use super::*;
// =============================================================================================
// dates.* (dat) -- the five surfaces a date is drawn on
//
// The formatter itself and the zone seam are in the `cpu` tier's file beside this one, and the
// reasoning is there.
//
// **No assertion here reads a zone this scenario made up.** These are the wiring half: each
// surface is compared against what the machine's own offset would give for that very instant, so
// they say the same thing in every zone and go red only where a surface is left on a fixed one.
// =============================================================================================

/// The instant the date scenarios draw.
const DATE_INSTANT: i64 = 1_789_404_178;
/// A villa's own rent period; the arithmetic for it is another scenario's claim.
const DATE_RENT_PERIOD: i64 = 30 * 86_400;
/// When this house was bought, as the quality carries it.
const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;
/// The cell the recorded house sits in.
const HOUSE_CELL: u32 = 0xA9B4_0025;

/// The date the machine itself would draw for an instant -- the offset for **that** instant,
/// because which offset it is depends on when it is.
fn the_machines_own(t: i64) -> String {
    strftime_c(t, local_utc_offset_secs(t))
}

/// A surface must not be drawing in the shard's time, and must be drawing in the machine's.
///
/// On a machine that *is* at that offset the two are one string and there is nothing to see; that
/// is said out loud rather than left as a silently vacuous assertion, and the shape half still
/// bites everywhere.
fn drawn_in_the_machines_time(text: &str, t: i64) -> bool {
    let off = local_utc_offset_secs(t);
    if off == 0 {
        return text.contains(&strftime_c(t, 0));
    }
    !text.contains(&strftime_c(t, 0)) && text.contains(&strftime_c(t, off))
}

/// No date in the shipped runtime's shape carries a weekday, and every one of them ends in a
/// morning or afternoon mark.
fn not_the_other_shape(text: &str) -> bool {
    !["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .any(|w| text.contains(w))
        && (text.contains(" AM") || text.contains(" PM"))
}

fn the_clock_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a clock after 1970")
}

// ---------------------------------------------------------------------------------------------
// dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape
// ---------------------------------------------------------------------------------------------

/// The line is read off the element as well as off what the sheet remembers, because a claim about
/// what the player reads that is only ever taken from the model is a claim about the model.
pub(super) fn the_character_sheets_born_line_is_in_the_machines_own_time() {
    let (mut c, _peer) = a_described_character();
    press_the_strip(&mut c, BURDEN_LAMP);

    let stamp = i32::try_from(DATE_INSTANT).expect("this year fits the clock the quality carries");
    set_int_quality(&mut c, 11, characterinfo::prop::CREATION_TIMESTAMP, stamp);
    // Anything that forces the sheet to compose again; the sheet watches exactly one quality and
    // this is it.
    set_int_quality(&mut c, 12, characterinfo::prop::AGE, 15);
    c.tick(6);

    let born = c.view().expect_app().hud().panels.character_info.sections[0].clone();
    let want = the_machines_own(DATE_INSTANT);
    let it_says_so = born.contains(&want);
    let it_is_on_the_element = {
        let h = hud_find(&c, characterinfo::INFO_TEXT);
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell
            .ui
            .text_element_mut(h)
            .expect("the sheet's text is a text element")
            .glyphs
            .inq_text(false)
            .contains(&want)
    };
    let the_right_shape = not_the_other_shape(&born);
    let the_right_time = drawn_in_the_machines_time(&born, DATE_INSTANT);

    c.assert_behaviour(
        "dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape",
        move |_| it_says_so && it_is_on_the_element && the_right_shape && the_right_time,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The house, as the shard describes one
// ---------------------------------------------------------------------------------------------

/// One line of what was paid: how much, how much of it is paid, and what it was paid in.
fn a_payment(num: i32, paid: i32, wcid: u32, name: &str, plural: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&num.to_le_bytes());
    b.extend_from_slice(&paid.to_le_bytes());
    b.extend_from_slice(&wcid.to_le_bytes());
    for s in [name, plural] {
        b.extend_from_slice(&u16::try_from(s.len()).expect("a short name").to_le_bytes());
        b.extend_from_slice(s.as_bytes());
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

/// A villa that still owes rent, written the way the shard writes one.
fn a_villa(buy_time: i32, rent_time: i32) -> Vec<u8> {
    let (buy, rent) = (
        a_payment(30_000, 30_000, 273, "Pyreal", "Pyreals"),
        a_payment(30_000, 20_000, 273, "Pyreal", "Pyreals"),
    );
    let mut b = Vec::new();
    b.extend_from_slice(&buy_time.to_le_bytes());
    b.extend_from_slice(&rent_time.to_le_bytes());
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&0i32.to_le_bytes());
    for list in [&buy, &rent] {
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(list);
    }
    b.extend_from_slice(&HOUSE_CELL.to_le_bytes());
    for f in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// The house pane, filled from one description of a villa.
fn a_pane_for_a_villa(bought_at: i64) -> HeadlessClient {
    let (mut c, _peer) = a_described_character();
    let stamp = i32::try_from(bought_at).expect("this year fits");
    set_int_quality(&mut c, 21, HOUSE_PURCHASE_TIMESTAMP, stamp);
    let mut blob = dereth_protocol::Opcode::HOUSE_HOUSE_DATA
        .0
        .to_le_bytes()
        .to_vec();
    blob.extend_from_slice(&a_villa(stamp, stamp));
    c.when(dereth_testkit::Inbound::event(
        dereth_client_net::client_session::SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode::HOUSE_HOUSE_DATA,
            blob,
        },
    ));
    c.tick(6);
    c
}

fn pane_rows(c: &HeadlessClient) -> Vec<String> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .house
        .lines
        .iter()
        .map(|(s, _)| s.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape
// ---------------------------------------------------------------------------------------------

/// Three rows of the one pane, each with its own date worked out on its own.
pub(super) fn every_date_on_the_house_pane_is_in_the_machines_own_time() {
    let mut c = a_pane_for_a_villa(DATE_INSTANT);
    let rows = pane_rows(&c);
    let due = DATE_INSTANT + DATE_RENT_PERIOD;

    let bought = rows.get(2).cloned().expect("the pane has a bought row");
    let ends = rows.get(3).cloned().expect("the pane has a period-end row");
    let next = rows.get(4).cloned().expect("the pane has a next-due row");

    let the_bought_row = bought == format!("{}{}", house::BOUGHT, the_machines_own(DATE_INSTANT))
        && not_the_other_shape(&bought)
        && drawn_in_the_machines_time(&bought, DATE_INSTANT);
    let the_period_row = ends == format!("{}{}", house::PERIOD_ENDS, the_machines_own(due))
        && not_the_other_shape(&ends)
        && drawn_in_the_machines_time(&ends, due);
    let the_due_row = next.starts_with(house::NEXT_DUE)
        && next == format!("{}{}", house::NEXT_DUE, the_machines_own(due))
        && not_the_other_shape(&next)
        && drawn_in_the_machines_time(&next, due);

    c.assert_behaviour(
        "dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape",
        move |_| the_bought_row && the_period_row && the_due_row,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time
// ---------------------------------------------------------------------------------------------

/// The one date on this pane that is not worked out the same way as the other three. A house
/// bought moments ago has not served its wait, so the line takes its dated arm rather than saying
/// nothing.
pub(super) fn the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time() {
    let bought_at = the_clock_now() - 10;
    let mut c = a_pane_for_a_villa(bought_at);
    let rows = pane_rows(&c);
    let line = rows
        .last()
        .cloned()
        .expect("the last row of the pane is drawn");
    let at = bought_at + house::PURCHASE_WAIT_SECONDS;

    let it_says_so = line
        == format!(
            "{}{}{}",
            house::BUY_LANDSCAPE_AT,
            the_machines_own(at),
            house::APARTMENT_EXEMPTION
        );
    let the_right_shape = not_the_other_shape(&line);
    let the_right_time = drawn_in_the_machines_time(&line, at);

    c.assert_behaviour(
        "dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time",
        move |_| it_says_so && the_right_shape && the_right_time,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant
// ---------------------------------------------------------------------------------------------

/// The one surface whose shape was always right, because that shape says nothing about where in
/// the world it is. What is claimed here is the number behind it.
pub(super) fn the_chat_stamp_carries_the_machines_own_offset() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.tick(6);

    let (stamped_at, offset) = {
        let world = c.view().world();
        (world.scroll.now_unix, world.scroll.utc_offset_secs)
    };
    let the_clock_was_written = stamped_at > 1_700_000_000;
    let it_is_the_machines = offset == local_utc_offset_secs(stamped_at);

    let prefix = dereth_client_model::scroll::timestamp_prefix(stamped_at, offset);
    let hour = prefix.split(':').next().unwrap_or("");
    let the_stamp_is_shaped_right =
        prefix.ends_with(' ') && !hour.is_empty() && (hour.len() == 1 || !hour.starts_with('0'));
    // ...and it is not the shard's time, unless this machine keeps that time.
    let not_a_constant =
        offset == 0 || prefix != dereth_client_model::scroll::timestamp_prefix(stamped_at, 0);

    c.assert_behaviour(
        "dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant",
        move |_| {
            the_clock_was_written
                && it_is_the_machines
                && the_stamp_is_shaped_right
                && not_a_constant
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time
// ---------------------------------------------------------------------------------------------

/// The clock is read inside the arm that composes the sentence, so what is admissible is one
/// rendering per second the frames could have spanned -- built with the machine's own offset for
/// the instant the ban runs out.
pub(super) fn the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time() {
    use dereth_ui_screens::screens::disconnected::{self, DisconnectedScreen};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let before = the_clock_now();
    c.app_mut().process_logon_event_queue(vec![
        dereth_client_net::client_session::SessionEvent::AccountBanned {
            expiry: 7_200,
            reason: " - testing".to_string(),
        },
    ]);
    c.tick(3);
    let after = the_clock_now();

    let shown = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let s = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **s;
        any.downcast_mut::<DisconnectedScreen>()
            .expect("the client is off the world")
            .shown_text
            .clone()
            .expect("and it says why")
    };

    let admissible: Vec<String> = (before..=after)
        .map(|n| {
            let at = disconnected::ban_expiry_epoch(7_200, n);
            disconnected::account_banned_message(7_200, " - testing", n, local_utc_offset_secs(at))
        })
        .collect();
    let it_is_one_of_them = admissible.contains(&shown);
    // ...and it is none of the shard-time renderings, unless this machine keeps that time.
    let not_the_shards = local_utc_offset_secs(before) == 0
        || !(before..=after)
            .map(|n| disconnected::account_banned_message(7_200, " - testing", n, 0))
            .any(|u| u == shown);
    // The one date in the client drawn in the weekday-and-month-name shape keeps it.
    let it_keeps_its_shape = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .any(|w| shown.contains(w));

    c.assert_behaviour(
        "dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time",
        move |_| it_is_one_of_them && not_the_shards && it_keeps_its_shape,
    );
    c.shutdown();
}
