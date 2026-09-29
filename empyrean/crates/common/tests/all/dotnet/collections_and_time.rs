//! Vectors: tests/fixtures/dotnet_misc.tsv
//! DotNetDict/HashSet enumeration order with free-list reuse, DateTime/TimeSpan formatting and
//! arithmetic, seeded System.Random equal .NET 8 output (dotnet_misc.tsv).
//! Fixture: tests/fixtures/dotnet_misc.tsv and locally constructed edge cases.

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict, DotNetHashSet, TimeSpan};
use empyrean_common::random::DotNetRandom;

const FIXTURE: &str = include_str!("../../fixtures/dotnet_misc.tsv");

fn rows(tag: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    FIXTURE
        .lines()
        .filter(move |l| l.split('\t').next() == Some(tag))
        .map(|l| l.split('\t').collect())
}

fn d(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s.strip_prefix("d:").expect("d:"), 16).expect("hex"))
}

fn keys<V>(dict: &DotNetDict<i32, V>) -> String {
    dict.keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn dictionary_enumerates_in_dotnet_order_with_free_list_reuse() {
    // The same operation sequence as dotnet_oracle_misc.cs.
    let mut d: DotNetDict<i32, &str> = DotNetDict::new();
    let mut log = Vec::new();
    for (k, v) in [(1, "a"), (2, "b"), (3, "c"), (4, "d")] {
        d.insert(k, v);
    }
    log.push(keys(&d));
    d.remove(&2);
    log.push(keys(&d));
    d.insert(5, "e");
    log.push(keys(&d));
    d.remove(&1);
    d.remove(&4);
    log.push(keys(&d));
    d.insert(6, "f");
    log.push(keys(&d));
    d.insert(7, "g");
    log.push(keys(&d));
    d.insert(8, "h");
    log.push(keys(&d));
    assert_eq!(d.insert(3, "c2"), Some("c"), "overwrite keeps the slot");
    log.push(keys(&d));
    assert_eq!(d.remove(&99), None);
    log.push(keys(&d));
    for k in [5, 6, 7, 8, 3] {
        d.remove(&k);
    }
    log.push(keys(&d));
    d.insert(9, "i");
    d.insert(10, "j");
    log.push(keys(&d));
    d.clear();
    d.insert(11, "k");
    d.insert(12, "l");
    log.push(keys(&d));
    for i in 20..40 {
        d.insert(i, "x");
    }
    for i in (20..40).step_by(3) {
        d.remove(&i);
    }
    log.push(keys(&d));
    for i in 100..110 {
        d.insert(i, "y");
    }
    log.push(keys(&d));
    d.remove(&11);
    d.remove(&100);
    d.remove(&12);
    for i in 200..204 {
        d.insert(i, "z");
    }
    log.push(keys(&d));

    let expected = rows("dict").next().expect("dict row")[1];
    assert_eq!(log.join("|"), expected);
    assert_eq!(d.len(), 26);
    assert_eq!(d.get(&3), None);
    assert_eq!(d.get(&200), Some(&"z"));
}

#[test]
fn dictionary_api_members() {
    let mut d: DotNetDict<String, i32> = DotNetDict::new();
    assert!(d.is_empty());
    assert!(d.try_add("a".into(), 1));
    assert!(!d.try_add("a".into(), 2), "TryAdd never overwrites");
    d.add("b".into(), 2);
    *d.get_mut("a").expect("a") += 10;
    *d.get_or_insert_with("c".into(), || 3) += 100;
    *d.get_or_insert_with("a".into(), || 999) += 1;
    assert!(d.contains_key("c"));
    assert_eq!(
        d.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>(),
        ["a=12", "b=2", "c=103"]
    );
    for (_, v) in d.iter_mut() {
        *v *= 2;
    }
    for v in d.values_mut() {
        *v += 1;
    }
    assert_eq!(d.values().copied().collect::<Vec<_>>(), [25, 5, 207]);
    // A removed slot is reused by the next insert, most recently freed first.
    d.remove("a");
    d.remove("c");
    d.insert("d".into(), 4);
    d.insert("e".into(), 5);
    assert_eq!(d.keys().cloned().collect::<Vec<_>>(), ["e", "b", "d"]);
    let collected: DotNetDict<i32, i32> = [(3, 0), (1, 0), (2, 0)].into_iter().collect();
    assert_eq!(collected.keys().copied().collect::<Vec<_>>(), [3, 1, 2]);
}

#[test]
#[should_panic(expected = "same key")]
fn dictionary_add_of_a_present_key_throws() {
    let mut d = DotNetDict::new();
    d.add(1, 1);
    d.add(1, 2);
}

#[test]
fn hash_set_enumerates_in_dotnet_order() {
    let mut h: DotNetHashSet<u32> = DotNetHashSet::new();
    let dump = |h: &DotNetHashSet<u32>| {
        h.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut log = Vec::new();
    assert!(h.insert(10));
    assert!(h.insert(20));
    assert!(h.insert(30));
    assert!(!h.insert(10));
    log.push(dump(&h));
    assert!(h.remove(&20));
    assert!(h.remove(&10));
    log.push(dump(&h));
    h.insert(40);
    log.push(dump(&h));
    h.insert(50);
    h.insert(60);
    log.push(dump(&h));
    h.clear();
    h.insert(1);
    log.push(dump(&h));
    assert_eq!(log.join("|"), rows("hashset").next().expect("row")[1]);
    assert!(h.contains(&1) && !h.contains(&2) && h.len() == 1 && !h.is_empty());
    let s: DotNetHashSet<u32> = [5, 3, 5, 1].into_iter().collect();
    assert_eq!(s.iter().copied().collect::<Vec<_>>(), [5, 3, 1]);
}

#[test]
fn date_time_formats_match_dotnet_en_us() {
    let mut n = 0;
    for r in rows("dt") {
        let dt = DotNetDateTime::from_ticks(r[1].parse().expect("ticks"));
        assert_eq!(dt.format(r[2]), r[3], "ticks {} format {:?}", r[1], r[2]);
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    let dt = DotNetDateTime::new_hms(2024, 1, 5, 13, 7, 9);
    assert_eq!(dt.to_string(), "1/5/2024 1:07:09 PM");
    assert_eq!(dt.format("D"), "Friday, January 5, 2024");
    assert_eq!(dt.format("t"), "1:07 PM");
    assert_eq!(dt.format("T"), "1:07:09 PM");
    assert_eq!(dt.format("g"), "1/5/2024 1:07 PM");
    assert_eq!(dt.format("s"), "2024-01-05T13:07:09");
    assert_eq!(dt.format("u"), "2024-01-05 13:07:09Z");
    assert_eq!(dt.format("h 't' \\t"), "1 t t");
    assert_eq!(
        dt.format("HH.FFF"),
        "13",
        "an all-zero F fraction drops the preceding point"
    );
}

#[test]
fn date_time_components_and_constructors() {
    let dt = DotNetDateTime::new_hms_ms(2000, 2, 29, 23, 59, 59, 999);
    assert_eq!(dt.ticks(), 630_874_655_999_990_000);
    assert_eq!((dt.year(), dt.month(), dt.day()), (2000, 2, 29));
    assert_eq!(
        (dt.hour(), dt.minute(), dt.second(), dt.millisecond()),
        (23, 59, 59, 999)
    );
    assert_eq!(dt.day_of_week(), 2, "a Tuesday");
    assert_eq!(dt.date(), DotNetDateTime::new(2000, 2, 29));
    assert_eq!(DotNetDateTime::UNIX_EPOCH, DotNetDateTime::new(1970, 1, 1));
    assert_eq!(DotNetDateTime::MAX_VALUE.ticks(), 3_155_378_975_999_999_999);
    assert_eq!(DotNetDateTime::MIN_VALUE.ticks(), 0);
    assert_eq!(
        (
            DotNetDateTime::MAX_VALUE.year(),
            DotNetDateTime::MAX_VALUE.month(),
            DotNetDateTime::MAX_VALUE.day()
        ),
        (9999, 12, 31)
    );
    let a = DotNetDateTime::new(2024, 3, 1);
    assert_eq!(
        a - DotNetDateTime::new(2024, 2, 28),
        TimeSpan::from_days(2.0)
    );
    assert_eq!(
        a + TimeSpan::from_hours(1.5) - TimeSpan::from_minutes(30.0),
        DotNetDateTime::new_hms(2024, 3, 1, 1, 0, 0)
    );
    assert_eq!(a.add_days(-1.0), DotNetDateTime::new(2024, 2, 29));
    assert_eq!(a.add_minutes(1.5).ticks() - a.ticks(), 900_000_000);
    assert_eq!(
        a.add_milliseconds(0.00015).ticks() - a.ticks(),
        1,
        "1.5 ticks truncate to 1"
    );
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn date_time_rejects_february_30() {
    let _ = DotNetDateTime::new(2023, 2, 29);
}

#[test]
fn add_seconds_truncates_integral_and_fractional_parts_separately() {
    let epoch = DotNetDateTime::UNIX_EPOCH;
    let mut n = 0;
    for r in rows("addsec") {
        let t = epoch.add_seconds(d(r[1]));
        assert_eq!(
            t.ticks(),
            r[2].parse::<i64>().expect("ticks"),
            "AddSeconds({:e})",
            d(r[1])
        );
        assert_eq!(
            (t - epoch).total_seconds().to_bits(),
            d(r[3]).to_bits(),
            "TotalSeconds after AddSeconds({:e})",
            d(r[1])
        );
        n += 1;
    }
    for r in rows("addhours") {
        assert_eq!(
            epoch.add_hours(d(r[1])).ticks(),
            r[2].parse::<i64>().expect("ticks")
        );
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
}

#[test]
fn time_span_matches_dotnet() {
    let mut n = 0;
    for r in rows("fromsec") {
        assert_eq!(
            TimeSpan::from_seconds(d(r[1])).ticks(),
            r[2].parse::<i64>().expect("ticks")
        );
        n += 1;
    }
    for r in rows("frommin") {
        assert_eq!(
            TimeSpan::from_minutes(d(r[1])).ticks(),
            r[2].parse::<i64>().expect("ticks")
        );
        n += 1;
    }
    for r in rows("ts") {
        let ts = TimeSpan::from_ticks(r[1].parse().expect("ticks"));
        assert_eq!(ts.to_string(), r[2]);
        assert_eq!(
            [
                ts.format("%d"),
                ts.format("%h"),
                ts.format("%m"),
                ts.format("%s")
            ],
            [r[3], r[4], r[5], r[6]]
        );
        let totals = [
            ts.total_seconds(),
            ts.total_minutes(),
            ts.total_hours(),
            ts.total_days(),
            ts.total_milliseconds(),
        ];
        for (got, want) in totals.iter().zip(&r[7..12]) {
            assert_eq!(got.to_bits(), d(want).to_bits(), "ticks {} totals", r[1]);
        }
        let parts = format!(
            "{},{},{},{},{}",
            ts.days(),
            ts.hours(),
            ts.minutes(),
            ts.seconds(),
            ts.milliseconds()
        );
        assert_eq!(parts, r[12]);
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    let ts = TimeSpan::from_ticks(1_234_567_891_234);
    assert_eq!(ts.format("dd\\.hh\\:mm\\:ss\\.fff"), "01.10:17:36.789");
    assert_eq!(ts.format("'x'FFFFFFF"), "x7891234");
    assert_eq!((-ts).duration(), ts);
    assert_eq!(ts + ts - ts, ts);
    assert_eq!(TimeSpan::from_milliseconds(1.5).ticks(), 15_000);
    assert_eq!(
        TimeSpan::MAX_VALUE.total_milliseconds(),
        922_337_203_685_477.0,
        "clamped"
    );
    assert_eq!(
        TimeSpan::MIN_VALUE.total_milliseconds(),
        -922_337_203_685_477.0,
        "clamped"
    );
    assert_eq!(TimeSpan::ZERO, TimeSpan::default());
}

#[test]
#[should_panic(expected = "Not-a-Number")]
fn time_span_rejects_nan() {
    let _ = TimeSpan::from_seconds(f64::NAN);
}

fn ints(s: &str) -> Vec<i32> {
    s.split(',').map(|v| v.parse().expect("int")).collect()
}

#[test]
fn seeded_random_matches_dotnet_draw_for_draw() {
    // The first draws of new Random(seed) for nine seeds, including int.MinValue (folded to
    // int.MaxValue) and int.MaxValue, through each overload.
    let mut n = 0;
    for r in FIXTURE
        .lines()
        .filter(|l| l.starts_with("rnd_"))
        .map(|l| l.split('\t').collect::<Vec<_>>())
    {
        let seed: i32 = r[1].parse().expect("seed");
        let mut rng = DotNetRandom::new(seed);
        let got: Vec<String> = match r[0] {
            "rnd_next" => (0..8).map(|_| rng.next().to_string()).collect(),
            "rnd_double" => (0..8)
                .map(|_| format!("d:{:016X}", rng.next_double().to_bits()))
                .collect(),
            "rnd_0_11" => (0..12).map(|_| rng.next_range(0, 11).to_string()).collect(),
            "rnd_large" => (0..8)
                .map(|_| rng.next_range(i32::MIN, i32::MAX).to_string())
                .collect(),
            "rnd_m5_6" => (0..8).map(|_| rng.next_range(-5, 6).to_string()).collect(),
            "rnd_max100" => (0..8).map(|_| rng.next_max(100).to_string()).collect(),
            "rnd_float_0.5_2.5" => (0..8)
                .map(|_| {
                    format!(
                        "d:{:016X}",
                        (rng.next_double() * f64::from(2.5f32 - 0.5f32) + 0.5).to_bits()
                    )
                })
                .collect(),
            "rnd_7_7" => (0..8).map(|_| rng.next_range(7, 7).to_string()).collect(),
            "rnd_next_10000" => {
                vec![(0..10000)
                    .map(|_| rng.next())
                    .last()
                    .expect("draw")
                    .to_string()]
            }
            other => panic!("unknown row {other}"),
        };
        assert_eq!(got.join(","), r[2], "{} seed {seed}", r[0]);
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    assert_eq!(
        ints(
            &(0..3)
                .map(|_| DotNetRandom::new(0).next().to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        [1_559_595_546; 3]
    );
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn random_next_range_rejects_min_above_max() {
    let _ = DotNetRandom::new(1).next_range(2, 1);
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn random_next_max_rejects_negative() {
    let _ = DotNetRandom::new(1).next_max(-1);
}
