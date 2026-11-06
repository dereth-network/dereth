//! The world's clock as the scripts ask it, its natives' frames, and with no clock.

use auberean_assets::region::{DayPart, GameTime, Season};
use auberean_script::native_object::{native_type, NativeObject, TextInfo, VariableValue};
use auberean_script::value::tag;
use auberean_script::Status;
use auberean_world::calendar::GameClock;

use crate::common::{Bench, ITEM_REF};

const PACKAGE: u32 = 0x2C;
/// Two years and 100 and a half days past the calendar's zero.
const NOW: f64 = 720_000.0 + 100_500.0;

fn bench() -> Bench {
    let mut b = Bench::new();
    let part = |begin, night, name| DayPart { begin, night, name };
    let mut g = GameClock::new(GameTime {
        zero_year: 100,
        day_length: 1000.0,
        days_per_year: 360,
        year_name: 7,
        day_parts: vec![part(0.0, 1, 10), part(0.25, 0, 11)],
        seasons: vec![Season { name: 20, begin: 0 }],
        ..GameTime::default()
    });
    g.use_time(NOW);
    b.host.game_clock = Some(g);
    b.host.clock = NOW;
    b
}

/// The calendar's natives answer the clock's values, below the receiver; a null receiver crashes
/// each but the night's, and with no clock each answers 0.
///
/// Behaviour: script.natives.the-world-clock-answers-the-calendars-values
#[test]
fn the_clocks_natives_answer_its_values() {
    let mut b = bench();
    assert_eq!(b.answer(PACKAGE, 0x01, ITEM_REF, &[]), 0, "the day part");
    assert_eq!(b.answer(PACKAGE, 0x03, ITEM_REF, &[]), 1000f32.to_bits());
    assert_eq!(b.answer(PACKAGE, 0x04, ITEM_REF, &[]), 100);
    assert_eq!(b.answer(PACKAGE, 0x05, ITEM_REF, &[]), 360);
    assert_eq!(b.answer(PACKAGE, 0x06, ITEM_REF, &[]), 102);
    let (status, slots) = b.call(PACKAGE, 0x02, ITEM_REF, &[], 2);
    assert_eq!(status, Status::Success);
    let bits = u64::from(slots[0].0) << 32 | u64::from(slots[1].0);
    assert_eq!(f64::from_bits(bits), 0.5);
    assert_eq!(slots[1].1, tag::LONG | tag::MIXED);
    let null = auberean_script::value::NULL;
    assert_eq!(b.call(PACKAGE, 0x06, null, &[], 1).0, Status::Crash);
    assert_eq!(b.call(PACKAGE, 0x01, null, &[], 1).0, Status::Success);
    b.host.game_clock = None;
    assert_eq!(b.answer(PACKAGE, 0x06, ITEM_REF, &[]), 0);
    assert_eq!(b.answer(PACKAGE, 0x01, ITEM_REF, &[]), 0);
    assert_eq!(b.call(PACKAGE, 0x06, null, &[], 1).0, Status::Crash);
}

/// The date and time are written into two string references with the calendar's strings' ids and
/// their variables; with no clock nothing is written and the answer is 0.
///
/// Behaviour: script.natives.the-world-clock-writes-the-date-and-time-as-string-references
#[test]
fn the_clock_writes_the_date_and_time_as_string_references() {
    let mut b = bench();
    let info = |b: &mut Bench| {
        b.vm.heap.add_native(
            native_type::STRING_INFO,
            NativeObject::TextInfo(TextInfo::default()),
        )
    };
    let (date, time) = (info(&mut b), info(&mut b));
    let args = [(date, tag::REFERENCE), (time, tag::REFERENCE)];
    assert_eq!(b.answer(PACKAGE, 0x08, ITEM_REF, &args), 1);
    let token = auberean_assets::strings::token;
    let var = |b: &Bench, r: u32, name: &str| {
        let NativeObject::TextInfo(t) = b.vm.heap.native(r).unwrap() else {
            panic!("a string reference");
        };
        t.variables
            .iter()
            .find(|v| v.id == token(name))
            .map(|v| v.value.clone())
            .expect("the variable")
    };
    assert_eq!(var(&b, date, "YEAR"), VariableValue::Word(102));
    assert_eq!(var(&b, date, "DAY"), VariableValue::Word(101));
    let nested = |v| match v {
        VariableValue::Nested(n) => n.string_id,
        other => panic!("{other:?}"),
    };
    assert_eq!(nested(var(&b, date, "YEARSPEC")), 7);
    assert_eq!(nested(var(&b, date, "SEASON")), 20);
    assert_eq!(nested(var(&b, time, "TIME")), 11);
    let NativeObject::TextInfo(t) = b.vm.heap.native(time).unwrap() else {
        panic!("a string reference");
    };
    assert_eq!(t.string_id, token("ID_TimeString"));
    // At a stamp: the stamp a little later than the day's start in the present day's terms.
    let stamp = 100_100f64.to_bits();
    let (lo, hi) = auberean_script::value::split(stamp);
    let at = [
        (date, tag::REFERENCE),
        (time, tag::REFERENCE),
        (lo, tag::LONG | tag::MIXED),
        (hi, tag::LONG | tag::MIXED),
    ];
    assert_eq!(b.answer(PACKAGE, 0x07, ITEM_REF, &at), 1);
    assert_eq!(nested(var(&b, time, "TIME")), 10);
    b.host.game_clock = None;
    assert_eq!(b.answer(PACKAGE, 0x08, ITEM_REF, &args), 0);
}

/// An admin's commands act on the clock and answer whether they could; a rate too slow is refused.
///
/// Behaviour: script.natives.the-world-clock-takes-an-admins-commands
#[test]
fn an_admins_commands_move_the_clock_and_answer_whether_they_could() {
    let mut b = bench();
    let arg = |x: f32| [(x.to_bits(), tag::FLOAT)];
    assert_eq!(b.answer(PACKAGE, 0x09, ITEM_REF, &arg(0.75)), 1);
    let g = b.host.game_clock.as_ref().unwrap();
    assert!((g.time_of_day() - 0.75).abs() < 1e-6);
    assert!(g.needs_light());
    assert_eq!(
        b.answer(PACKAGE, 0x0A, ITEM_REF, &arg(0.0001)),
        0,
        "too slow"
    );
    assert_eq!(b.answer(PACKAGE, 0x0A, ITEM_REF, &arg(3.0)), 1);
    assert_eq!(b.host.game_clock.as_ref().unwrap().dilation(), 3.0);
    b.host.game_clock = None;
    assert_eq!(b.answer(PACKAGE, 0x09, ITEM_REF, &arg(0.75)), 0);
    assert_eq!(b.answer(PACKAGE, 0x0A, ITEM_REF, &arg(2.0)), 0);
}
