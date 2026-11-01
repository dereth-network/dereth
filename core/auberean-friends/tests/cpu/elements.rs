//! The elements against the vectors: each one written and read back, the friends service's own
//! elements read in the forms its traffic showed, what a reader skips, what it refuses, and the
//! service's result codes and friend states.

use auberean_friends::element::{MAX_SOFTWARE, MAX_TELL, MAX_TICKET};
use auberean_friends::frame::MAX_FRAME;
use auberean_friends::stream::{
    BURST, HELLO_DEADLINE, PING, RATE, RECONNECT_WAITS, SILENCE, STOOD, TICKET_DEADLINE,
};
use auberean_friends::{Element, Hr, Mode};

use crate::common::{fields, list, text, vectors};

/// Behaviour: friends.element.round-trip
#[test]
fn every_element_reads_as_its_fields_and_writes_back_the_same() {
    for v in list("elements") {
        let xml = text(v, "xml");
        let e = Element::parse(xml).unwrap_or_else(|err| panic!("{xml}: {err}"));
        assert_eq!(fields(&e), v["fields"], "{xml}");
        assert_eq!(e.to_xml(), xml);
        assert_eq!(e.out_of_bounds(), None, "{xml}");
    }
}

/// Behaviour: friends.element.read-as-captured
#[test]
fn the_friends_services_own_elements_read_as_its_traffic_wrote_them() {
    for v in list("captured") {
        let xml = text(v, "xml");
        let e = Element::parse(xml).unwrap_or_else(|err| panic!("{xml}: {err}"));
        assert_eq!(fields(&e), v["fields"], "{}", text(v, "about"));
        assert_eq!(e.to_xml(), text(v, "canonical"), "{}", text(v, "about"));
    }
}

/// Behaviour: friends.element.unknown-skipped
#[test]
fn what_a_reader_does_not_know_it_skips_or_reads_leniently() {
    for v in list("skipped") {
        let xml = text(v, "xml");
        let e = Element::parse(xml).unwrap_or_else(|err| panic!("{xml}: {err}"));
        assert_eq!(fields(&e), v["fields"], "{}", text(v, "about"));
    }
}

/// Behaviour: friends.element.malformed-refused
#[test]
fn text_that_is_no_element_or_breaks_an_elements_bounds_is_refused() {
    for v in list("malformed") {
        let xml = text(v, "xml");
        assert!(Element::parse(xml).is_err(), "{}: {xml}", text(v, "about"));
    }
}

/// Behaviour: friends.element.codes-and-modes
#[test]
fn the_result_codes_and_friend_states_are_the_friends_services_own() {
    let codes = &vectors()["codes"];
    for (name, hr) in [
        ("ok", Hr::OK),
        ("ticket_refused", Hr::TICKET_REFUSED),
        ("banned", Hr::BANNED),
        ("not_ticketed", Hr::NOT_TICKETED),
        ("already_ticketed", Hr::ALREADY_TICKETED),
        ("not_connected", Hr::NOT_CONNECTED),
        ("no_such_user", Hr::NO_SUCH_USER),
        ("not_online", Hr::NOT_ONLINE),
        ("timed_out", Hr::TIMED_OUT),
        ("denied", Hr::DENIED),
        ("not_implemented", Hr::NOT_IMPLEMENTED),
    ] {
        assert_eq!(hr.to_string(), text(codes, name), "{name}");
        assert_eq!(Hr::parse(text(codes, name)), Some(hr), "{name}");
    }
    let modes = &vectors()["modes"];
    for (name, mode) in [
        ("offline", Mode::OFFLINE),
        ("online", Mode::ONLINE),
        ("away", Mode::AWAY),
        ("not_disturbed", Mode::NOT_DISTURBED),
    ] {
        assert_eq!(modes[name].as_u64(), Some(u64::from(mode.0)), "{name}");
    }
    let away = Mode::ONLINE.with(Mode::AWAY);
    assert_eq!(away, Mode(3));
    assert!(away.is_online() && away.has(Mode::AWAY) && !away.has(Mode::NOT_DISTURBED));
    assert!(!Mode::OFFLINE.is_online());
    assert_eq!(Mode::ONLINE.with(Mode::NOT_DISTURBED), Mode(5));
}

/// Behaviour: friends.stream.limits
#[test]
fn the_crates_timings_and_bounds_are_the_specifications() {
    let l = &vectors()["limits"];
    let n = |name: &str| l[name].as_u64().unwrap_or_else(|| panic!("{name}"));
    assert_eq!(n("max_frame"), MAX_FRAME as u64);
    assert_eq!(n("ping_secs"), PING.as_secs());
    assert_eq!(n("silence_secs"), SILENCE.as_secs());
    assert_eq!(n("hello_deadline_secs"), HELLO_DEADLINE.as_secs());
    assert_eq!(n("ticket_deadline_secs"), TICKET_DEADLINE.as_secs());
    assert_eq!(n("stood_secs"), STOOD.as_secs());
    assert_eq!(n("burst"), u64::from(BURST));
    assert_eq!(n("rate_per_sec"), u64::from(RATE));
    assert_eq!(n("max_software"), MAX_SOFTWARE as u64);
    assert_eq!(n("max_ticket"), MAX_TICKET as u64);
    assert_eq!(n("max_tell"), MAX_TELL as u64);
    let waits: Vec<u64> = RECONNECT_WAITS.iter().map(|w| w.as_secs()).collect();
    let listed: Vec<u64> = l["reconnect_waits_secs"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(serde_json::Value::as_u64)
        .collect();
    assert_eq!(waits, listed);
}
