//! Vectors: tests/fixtures/dotnet_format.tsv
//! Numeric ToString(format) en-US, every Math.Round overload and Math.Max/Min equal .NET 8
//! (dotnet_format.tsv).
//! Fixture: tests/fixtures/dotnet_format.tsv and locally constructed edge cases.

use empyrean_common::dotnet::math::{self, MidpointRounding};
use empyrean_common::dotnet::{format, Num};

const FIXTURE: &str = include_str!("../../fixtures/dotnet_format.tsv");

fn parse_num(s: &str) -> Num {
    let (kind, v) = s.split_once(':').expect("typed value");
    match kind {
        "d" => Num::F64(f64::from_bits(u64::from_str_radix(v, 16).expect("hex"))),
        "f" => Num::F32(f32::from_bits(u32::from_str_radix(v, 16).expect("hex"))),
        "i64" => v.parse::<i64>().expect("i64").into(),
        "i32" => v.parse::<i32>().expect("i32").into(),
        "u32" => v.parse::<u32>().expect("u32").into(),
        "u64" => v.parse::<u64>().expect("u64").into(),
        "i16" => v.parse::<i16>().expect("i16").into(),
        "u16" => v.parse::<u16>().expect("u16").into(),
        "u8" => v.parse::<u8>().expect("u8").into(),
        "i8" => v.parse::<i8>().expect("i8").into(),
        other => panic!("unknown value kind {other}"),
    }
}

fn d(s: &str) -> f64 {
    let v = s.strip_prefix("d:").unwrap_or(s);
    f64::from_bits(u64::from_str_radix(v, 16).expect("hex"))
}

fn f(s: &str) -> f32 {
    let v = s.strip_prefix("f:").expect("f:");
    f32::from_bits(u32::from_str_radix(v, 16).expect("hex"))
}

fn unescape(s: &str) -> String {
    s.replace("\\t", "\t")
        .replace("\\n", "\n")
        .replace("\\\\", "\\")
}

fn same_f64(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn same_f32(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn mode(m: &str) -> Option<MidpointRounding> {
    Some(match m {
        "-" => return None,
        "0" => MidpointRounding::ToEven,
        "1" => MidpointRounding::AwayFromZero,
        "2" => MidpointRounding::ToZero,
        "3" => MidpointRounding::ToNegativeInfinity,
        "4" => MidpointRounding::ToPositiveInfinity,
        other => panic!("mode {other}"),
    })
}

#[test]
fn numeric_to_string_matches_dotnet() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for line in FIXTURE.lines().filter(|l| l.starts_with("fmt\t")) {
        let cols: Vec<&str> = line.split('\t').collect();
        let (value, fmt, expected) = (parse_num(cols[1]), cols[2], unescape(cols[3]));
        let got = format(value, fmt);
        checked += 1;
        if got != expected {
            failures.push(format!(
                "{} {fmt:?}: got {got:?}, .NET {expected:?}",
                cols[1]
            ));
        }
    }
    assert!(checked > 0, "fixture has {checked} format lines");
    assert!(
        failures.is_empty(),
        "{} of {checked} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(40)].join("\n")
    );
}

#[test]
fn math_round_matches_dotnet_in_every_overload() {
    let header = FIXTURE
        .lines()
        .find(|l| l.starts_with("#round-columns\t"))
        .expect("header");
    let columns: Vec<(&str, &str)> = header
        .split('\t')
        .nth(1)
        .expect("columns")
        .split(',')
        .map(|c| c.split_once('/').expect("digits/mode"))
        .collect();
    let mut checked = 0;
    let mut failures = Vec::new();
    for line in FIXTURE.lines().filter(|l| l.starts_with("round\t")) {
        let cols: Vec<&str> = line.split('\t').collect();
        let x = d(cols[1]);
        for ((digits, m), expected) in columns.iter().zip(cols[2].split(',')) {
            let expected = d(expected);
            let got = match (*digits, mode(m)) {
                ("-", None) => math::round(x),
                ("-", Some(m)) => math::round_mode(x, m),
                (n, None) => math::round_digits(x, n.parse().expect("digits")),
                (n, Some(m)) => math::round_digits_mode(x, n.parse().expect("digits"), m),
            };
            checked += 1;
            if !same_f64(got, expected) {
                failures.push(format!(
                    "Round({x:e}, {digits}, {m}) = {got:e}, .NET {expected:e}"
                ));
            }
        }
    }
    assert!(checked > 0, "fixture has {checked} round results");
    assert!(
        failures.is_empty(),
        "{} of {checked} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(40)].join("\n")
    );
}

#[test]
fn math_max_min_match_dotnet() {
    let mut checked = 0;
    for line in FIXTURE.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols[0] {
            "max" => assert!(
                same_f64(math::max(d(cols[1]), d(cols[2])), d(cols[3])),
                "{line}"
            ),
            "min" => assert!(
                same_f64(math::min(d(cols[1]), d(cols[2])), d(cols[3])),
                "{line}"
            ),
            "maxf" => {
                assert!(
                    same_f32(math::max_f32(f(cols[1]), f(cols[2])), f(cols[3])),
                    "{line}"
                )
            }
            "minf" => {
                assert!(
                    same_f32(math::min_f32(f(cols[1]), f(cols[2])), f(cols[3])),
                    "{line}"
                )
            }
            _ => continue,
        }
        checked += 1;
    }
    assert!(checked > 0, "recorded min/max comparisons are present");
    // Signed zeros are distinguished: Max(-0, +0) is +0 and Min(+0, -0) is -0.
    assert!(math::max(-0.0, 0.0).is_sign_positive());
    assert!(math::min(0.0, -0.0).is_sign_negative());
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn math_round_rejects_more_than_15_digits() {
    let _ = math::round_digits(1.0, 16);
}
