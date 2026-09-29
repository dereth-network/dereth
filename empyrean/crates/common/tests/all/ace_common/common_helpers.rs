//! Vectors: tests/fixtures/ace_common.tsv
//! ISAAC, CryptoSystem search, Hash32, DerethDateTime, string/float/list/TimeSpan extensions and
//! ThreadSafeRandom equal ACE's C# run on .NET 8 (ace_common.tsv), with two net10 saturating-cast
//! rows.
//! Fixture: tests/fixtures/ace_common.tsv and locally constructed edge cases.

use std::panic::{catch_unwind, AssertUnwindSafe};

use empyrean_common::cryptography::crypto_system::CryptoSystem;
use empyrean_common::cryptography::hash32;
use empyrean_common::cryptography::isaac::Isaac;
use empyrean_common::dereth_date_time::{DerethDateTime, Hours, Months, Seasons};
use empyrean_common::dotnet::{format, DotNetDateTime, TimeSpan};
use empyrean_common::extensions::{
    character_name_extensions, date_time_extensions, double_extensions, enum_helper,
    exception_extensions, float_extensions, list_extensions, string_extensions,
    time_span_extensions,
};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::{Time, VirtualClock};

const FIXTURE: &str = include_str!("../../fixtures/ace_common.tsv");

fn rows(tag: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    FIXTURE
        .lines()
        .filter(move |l| l.split('\t').next() == Some(tag))
        .map(|l| l.split('\t').collect())
}

fn d(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s.strip_prefix("d:").expect("d:"), 16).expect("hex"))
}

fn f(s: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(s.strip_prefix("f:").expect("f:"), 16).expect("hex"))
}

fn hex(bytes: &str) -> Vec<u8> {
    (0..bytes.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&bytes[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The oracle's `DD()` rendering of a DerethDateTime.
fn dd(x: &DerethDateTime) -> String {
    let season = match x.season() {
        Seasons::Winter => "Winter",
        Seasons::Spring => "Spring",
        Seasons::Summer => "Summer",
        Seasons::Autumn => "Autumn",
    };
    let daytime = if x.is_daytime() { "True" } else { "False" };
    format!(
        "{}|{}|{}|{}|{}|{}|{season}|{daytime}",
        format(x.ticks(), "R"),
        x.year(),
        x.month(),
        x.day(),
        x.hour(),
        x
    )
}

/// `dd` of the result, or `EX:ArgumentOutOfRangeException` when the port panics with it.
fn dd_or_ex(f: impl FnOnce() -> DerethDateTime) -> String {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(x) => dd(&x),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            assert!(
                msg.starts_with("ArgumentOutOfRangeException"),
                "unexpected panic {msg}"
            );
            "EX:ArgumentOutOfRangeException".to_owned()
        }
    }
}

#[test]
fn isaac_key_stream_matches_ace() {
    let mut n = 0;
    for r in rows("isaac") {
        let seed = u32::from_str_radix(r[1], 16).expect("seed");
        let mut c = CryptoSystem::new(seed);
        let mut got = vec![format!("{:08X}", c.current_key)];
        for i in 0..600 {
            let v = c.next();
            if i < 12 || i % 97 == 0 || i > 590 {
                got.push(format!("{i}:{v:08X}"));
            }
        }
        assert_eq!(got.join(","), r[2], "seed {seed:08X}");
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    // The byte-array constructor reads the first four bytes little-endian.
    let mut a = Isaac::new(&[0x78, 0x56, 0x34, 0x12, 0xFF]);
    let b = CryptoSystem::new(0x1234_5678);
    assert_eq!(a.next(), b.current_key);
    assert_eq!(
        CryptoSystem::from_seed_bytes(&0x1234_5678u32.to_le_bytes()).current_key,
        0xFDD4_AFEB
    );
    a.release_resources();
    assert_eq!(a.next(), 0, "released generators return 0");
}

#[test]
fn crypto_system_search_matches_ace() {
    // The sequence of dotnet driver `crypto_search`.
    let mut keys = Vec::new();
    let mut r = CryptoSystem::new(0x1234_5678);
    keys.push(r.current_key);
    for _ in 0..400 {
        keys.push(r.next());
    }
    let mut c = CryptoSystem::new(0x1234_5678);
    let mut log = Vec::new();
    let s = |c: &mut CryptoSystem, x: u32| {
        let found = c.search(x);
        format!(
            "{}{}:{:08X}",
            if found { 'T' } else { 'F' },
            c.xors.as_ref().expect("xors").len(),
            c.current_key
        )
    };
    log.push(s(&mut c, keys[0]));
    c.consume_key(keys[0]);
    log.push(format!("{:08X}", c.current_key));
    log.push(s(&mut c, keys[3]));
    log.push(s(&mut c, keys[1]));
    c.consume_key(keys[3]);
    log.push(format!(
        "{}:{:08X}",
        c.xors.as_ref().expect("xors").len(),
        c.current_key
    ));
    c.consume_key(keys[1]);
    log.push(format!(
        "{}:{:08X}",
        c.xors.as_ref().expect("xors").len(),
        c.current_key
    ));
    log.push(s(&mut c, 0xABCD_EF01));
    log.push(s(&mut c, keys[300]));
    let first5: Vec<String> = c
        .xors
        .as_ref()
        .expect("xors")
        .iter()
        .take(5)
        .map(|x| format!("{x:08X}"))
        .collect();
    log.push(first5.join(","));
    assert_eq!(log.join("|"), rows("crypto_search").next().expect("row")[1]);
    c.release_resources();
    assert!(c.xors.is_none());
    assert_eq!(c.next(), 0);
}

#[test]
fn hash32_matches_ace() {
    let mut n = 0;
    for r in rows("hash32") {
        let data = hex(r[1]);
        assert_eq!(
            format!("{:08X}", hash32::calculate(&data, data.len())),
            r[2],
            "data {}",
            r[1]
        );
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    let rb = hex(rows("hash32").last().expect("row")[1]);
    assert_eq!(
        format!("{:08X}", hash32::calculate_at(&rb, 3, 47)),
        rows("hash32_off").next().expect("row")[2]
    );
}

#[test]
fn dereth_date_time_from_ticks_matches_ace() {
    let mut n = 0;
    for r in rows("ddt_ticks") {
        assert_eq!(
            dd(&DerethDateTime::from_ticks(d(r[1]))),
            r[2],
            "ticks {}",
            d(r[1])
        );
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    assert_eq!(DerethDateTime::MAX_VALUE, 1_073_741_824.0);
}

#[test]
fn dereth_date_time_from_components_matches_ace() {
    let mut n = 0;
    for r in rows("ddt_ymdh") {
        let c: Vec<i32> = r[1].split(',').map(|v| v.parse().expect("int")).collect();
        assert_eq!(
            dd_or_ex(|| DerethDateTime::from_ymdh(c[0], c[1], c[2], c[3])),
            r[2],
            "new DerethDateTime({})",
            r[1]
        );
        n += 1;
    }
    assert!(n > 0, "every recorded fixture row is compared");
    // The enum overload is the same constructor.
    let a = DerethDateTime::from_ymdh_named(11, Months::Wintersebb, 2, Hours::Midsong);
    assert_eq!(a, DerethDateTime::from_ymdh(11, 0, 2, 9));
    // ACE quirk kept: the Ticks setter at MinValue resets the fields, so hour 1 reads back as 8.
    assert_eq!(
        DerethDateTime::from_ymdh(10, 1, 1, 1).hour(),
        Hours::Morntide_and_Half as i32
    );
}

#[test]
fn dereth_date_time_arithmetic_matches_ace() {
    let base = DerethDateTime::from_ymdh(150, 9, 30, 16);
    let mut n = 0;
    for tag in [
        "ddt_addh", "ddt_addd", "ddt_addm", "ddt_addy", "ddt_addt", "ddt_subd", "ddt_subh",
        "ddt_subm",
    ] {
        for r in rows(tag) {
            let k: i32 = r[1].parse().expect("n");
            let got = dd_or_ex(|| match tag {
                "ddt_addh" => base.add_hours(k),
                "ddt_addd" => base.add_days(k),
                "ddt_addm" => base.add_months(k),
                "ddt_addy" => base.add_years(k),
                "ddt_addt" => base.add_ticks(f64::from(k) * 1000.5),
                "ddt_subd" => base.subtract_days(k),
                "ddt_subh" => base.subtract_hours(k),
                _ => base.subtract_months(k),
            });
            assert_eq!(got, r[2], "{tag}({k})");
            n += 1;
        }
    }
    assert!(n > 0, "every recorded fixture row is compared");
    assert_eq!(base.subtract_years(5).year(), 145);
    // ACE-BUG kept: SubtractTicks range-checks ticks + n but returns ticks - n.
    assert_eq!(base.subtract_ticks(1000.0).ticks(), base.ticks() - 1000.0);
    assert!(
        catch_unwind(|| DerethDateTime::new().subtract_ticks(-5.0)).is_err(),
        "check passes, the constructor then rejects"
    );
}

#[test]
fn dereth_date_time_real_world_conversions_match_ace() {
    let mut n = 0;
    for (tag, conv) in [("ddt_emu", 0), ("ddt_gdle", 1), ("ddt_lore", 2)] {
        for r in rows(tag) {
            let clock = VirtualClock::new(DotNetDateTime::from_ticks(r[1].parse().expect("ticks")));
            let got = dd_or_ex(|| match conv {
                0 => DerethDateTime::utc_now_to_emu_time(&clock),
                1 => DerethDateTime::utc_now_to_gdle_time(&clock),
                _ => DerethDateTime::utc_now_to_lore_time(&clock),
            });
            assert_eq!(got, r[2], "{tag} at ticks {}", r[1]);
            n += 1;
        }
    }
    assert!(n > 0, "every recorded fixture row is compared");
}

#[test]
fn dereth_date_time_names_and_predicates() {
    let x = DerethDateTime::from_ymdh(12, 0, 5, 12);
    assert_eq!(
        x.to_string(),
        "Date: Wintersebb 5, 12 P.Y.  Time: Warmtide-and-Half"
    );
    assert_eq!(x.date_to_string(), "Wintersebb 5, 12 P.Y.");
    assert_eq!(
        (x.month_to_string(), x.month_name_to_string()),
        ("Wintersebb".to_owned(), "Wintersebb".to_owned())
    );
    assert_eq!(
        (
            x.hour_to_string(),
            x.hour_name_to_string(),
            x.time_to_string()
        ),
        (
            "Warmtide-and-Half".into(),
            "Warmtide-and-Half".into(),
            "Warmtide-and-Half".into()
        )
    );
    assert_eq!(
        [x.py_to_string(), x.portal_year_string(), x.year_to_string()],
        ["12 P.Y.", "12 P.Y.", "12 P.Y."]
    );
    assert_eq!((x.py(), x.portal_year(), x.time()), (12, 12, 12));
    assert_eq!(
        (x.month_name(), x.hour_name(), x.time_name()),
        (
            Months::Wintersebb,
            Hours::Warmtide_and_Half,
            Hours::Warmtide_and_Half
        )
    );
    assert!(
        x.is_winter()
            && x.is_season(Seasons::Winter)
            && !x.is_spring()
            && !x.is_summer()
            && !x.is_autumn()
            && !x.is_fall()
    );
    assert!(x.is_daytime() && x.is_day() && !x.is_nighttime() && !x.is_night());
    let night = DerethDateTime::from_ymdh(12, 9, 5, 16);
    assert!(night.is_night() && night.is_fall());
    assert_eq!(DerethDateTime::default(), DerethDateTime::new());
    assert_eq!(
        DerethDateTime::new().to_string(),
        "Date: Morningthaw 1, 10 P.Y.  Time: Morntide-and-Half"
    );
    assert_eq!(Months::from_i32(10), None);
    assert_eq!(Hours::from_i32(0), None);
}

#[test]
fn double_and_float_extensions_match_ace() {
    for r in rows("formatchance") {
        assert_eq!(double_extensions::format_chance(d(r[1])), r[2]);
    }
    for r in rows("roundf") {
        let (x, dp) = (f(r[1]), r[2].parse().expect("dp"));
        // net10.0: (int) of 1e10 saturates to int.MaxValue and of NaN is 0; the .NET 8 oracle
        // printed int.MinValue for both.
        let expected: i32 = match r[3] {
            "-2147483648" if x.is_nan() => 0,
            "-2147483648" => i32::MAX,
            v => v.parse().expect("int"),
        };
        assert_eq!(float_extensions::round(x, dp), expected, "Round({x}, {dp})");
        let t = float_extensions::truncate(x, dp);
        assert!(
            t.to_bits() == f(r[4]).to_bits() || (t.is_nan() && f(r[4]).is_nan()),
            "Truncate({x}, {dp})"
        );
    }
    for r in rows("roundd") {
        let (x, dp) = (d(r[1]), r[2].parse().expect("dp"));
        let expected: i32 = match r[3] {
            "-2147483648" if x.is_nan() => 0,
            "-2147483648" => i32::MAX,
            v => v.parse().expect("int"),
        };
        assert_eq!(
            float_extensions::round_f64(x, dp),
            expected,
            "Round({x}, {dp})"
        );
    }
    let eps = rows("eps").next().expect("row");
    let got = [
        float_extensions::epsilon_equals(1.0, 1.00005),
        float_extensions::epsilon_equals(1.0, 1.0002),
        float_extensions::epsilon_equals(f32::NAN, f32::NAN),
    ];
    assert_eq!(
        got.map(|b| if b { "True" } else { "False" }),
        [eps[1], eps[2], eps[3]]
    );
}

#[test]
fn string_extensions_match_ace() {
    for r in rows("plural") {
        assert_eq!(string_extensions::pluralize(r[1]), r[2]);
    }
    for r in rows("vowel") {
        assert_eq!(
            string_extensions::starts_with_vowel(r[1]),
            r[2] == "True",
            "{:?}",
            r[1]
        );
    }
    let t = rows("trimstart").next().expect("row");
    assert_eq!(
        [
            string_extensions::trim_start("Hello World", "hello "),
            string_extensions::trim_start("Hello", "x"),
            string_extensions::trim_end("abcabc", "ABC"),
            string_extensions::trim_end("abc", "abcd"),
        ],
        [t[1], t[2], t[3], t[4]]
    );
    for r in rows("wildcard").filter(|r| !r[1].contains("\\t")) {
        assert_eq!(string_extensions::wild_card_to_regular(r[1]), r[2]);
    }
    // Regex.Escape writes a tab as the two characters `\t` (the oracle row cannot tell a raw tab
    // from that, so this case follows the documentation).
    assert_eq!(
        string_extensions::wild_card_to_regular("tab\there"),
        "^tab\\there$"
    );
    assert_eq!(
        string_extensions::trim_start("ÉCLAIR pie", "éclair "),
        "pie",
        "ordinal ignore-case beyond ASCII"
    );
    let c = rows("charname").next().expect("row");
    assert_eq!(
        character_name_extensions::string_array_to_character_name(&["cmd", "Some", "Name"], 1),
        c[1]
    );
    assert_eq!(
        character_name_extensions::string_array_to_character_name(&["cmd", "Name"], 1),
        c[2]
    );
    assert_eq!(
        character_name_extensions::string_array_to_character_name(&["A", "B", "C", "D"], 0),
        c[3]
    );
    assert_eq!(
        character_name_extensions::string_array_to_character_name(&["A", "B"], 0),
        c[4],
        "ACE-BUG kept"
    );
}

#[test]
fn enum_helper_and_list_extensions_match_ace() {
    for r in rows("flags") {
        let v: u32 = r[1].parse().expect("u32");
        assert_eq!(enum_helper::num_flags(v).to_string(), r[2]);
        assert_eq!(enum_helper::has_multiple(v), r[3] == "True");
    }
    ThreadSafeRandom::seed(42);
    let mut list: Vec<i32> = (0..10).collect();
    list_extensions::shuffle(&mut list);
    assert_eq!(
        list.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        rows("shuffle").next().expect("row")[2]
    );
    let p = rows("product").next().expect("row");
    assert_eq!(
        list_extensions::product(&[1.1, 2.2, 3.3]).to_bits(),
        f(p[1]).to_bits()
    );
    assert_eq!(list_extensions::product(&[]).to_bits(), f(p[2]).to_bits());
}

#[test]
fn thread_safe_random_matches_ace_draw_for_draw() {
    // ACE's ThreadSafeRandom over new Random(7): five Next(1, 6), three Next(0.25f, 0.75f),
    // three NextInterval(0.3f), three NextIntervalMax(0.9f).
    ThreadSafeRandom::seed(7);
    let mut got: Vec<String> = (0..5)
        .map(|_| ThreadSafeRandom::next(1, 6).to_string())
        .collect();
    let bits = |v: f64| format!("d:{:016X}", v.to_bits());
    got.extend((0..3).map(|_| bits(ThreadSafeRandom::next_float(0.25, 0.75))));
    got.extend((0..3).map(|_| bits(ThreadSafeRandom::next_interval(0.3))));
    got.extend((0..3).map(|_| bits(ThreadSafeRandom::next_interval_max(0.9))));
    assert_eq!(got.join(","), rows("tsr").next().expect("row")[2]);
}

#[test]
fn time_span_extensions_match_ace() {
    for r in rows("tsfriendly") {
        let ts = TimeSpan::from_ticks(r[1].parse().expect("ticks"));
        assert_eq!(time_span_extensions::get_friendly_string(ts), r[2]);
        assert_eq!(time_span_extensions::get_friendly_long_string(ts), r[3]);
        // net10.0: (uint) of a negative TotalSeconds saturates to 0; .NET 8 wrapped it (1657, 136).
        let (months, years) = if ts.ticks() < 0 {
            ("0", "0")
        } else {
            (r[4], r[5])
        };
        assert_eq!(time_span_extensions::get_months(ts).to_string(), months);
        assert_eq!(time_span_extensions::get_years(ts).to_string(), years);
    }
    assert_eq!(
        time_span_extensions::get_months(TimeSpan::from_days(65.0)),
        2
    );
    assert_eq!(
        time_span_extensions::get_years(TimeSpan::from_days(800.0)),
        2
    );
}

#[test]
#[allow(clippy::excessive_precision)] // the driver's C# literal, as written
fn time_and_date_time_extensions_match_ace() {
    let t = rows("time").next().expect("row");
    let dt = DotNetDateTime::new_hms_ms(2024, 1, 5, 0, 7, 9, 45);
    assert_eq!(Time::get_unix_time_at(dt).to_bits(), d(t[1]).to_bits());
    assert_eq!(
        Time::get_date_time_from_timestamp(1_700_000_000.123_456_789)
            .ticks()
            .to_string(),
        t[2]
    );
    assert_eq!(
        Time::get_date_time_from_timestamp(-1.5).ticks().to_string(),
        t[3]
    );
    assert_eq!(
        date_time_extensions::to_common_string(DotNetDateTime::new_hms(2024, 1, 5, 13, 7, 9)),
        rows("common").next().expect("row")[1]
    );
}

#[test]
fn exception_extensions_join_the_source_chain() {
    #[derive(Debug)]
    struct E(&'static str, Option<Box<E>>);
    impl std::fmt::Display for E {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for E {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|e| e as _)
        }
    }
    let e = E(
        "outer",
        Some(Box::new(E("middle", Some(Box::new(E("inner", None)))))),
    );
    assert_eq!(
        exception_extensions::get_full_message(&e),
        "outer --> middle --> inner"
    );
    assert_eq!(
        exception_extensions::get_full_message(&E("alone", None)),
        "alone"
    );
}

#[test]
fn sha2_hash_is_lowercase_hex_of_the_utf8_digest() {
    use empyrean_common::cryptography::sha2::{hash, Sha2Type};
    // FIPS 180-2 test vectors for "abc".
    assert_eq!(
        hash(Sha2Type::Sha256, "abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hash(Sha2Type::Sha512, "abc"),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
}

#[test]
fn enum_helper_get_flags_lists_every_member_whose_bits_are_set_in_get_values_order() {
    use empyrean_common::extensions::enum_helper::get_flags;
    // A [Flags] enum as .NET orders it: None = 0, A = 1, B = 2, AB = 3, C = 4.
    let members: [u64; 5] = [0, 1, 2, 3, 4];
    assert_eq!(get_flags(&members, |m| m, 3), vec![0, 1, 2, 3]);
    assert_eq!(get_flags(&members, |m| m, 0), vec![0]);
    assert_eq!(get_flags(&members, |m| m, 6), vec![0, 2, 4]);
}
