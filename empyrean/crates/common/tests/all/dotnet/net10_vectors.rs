//! Vectors: empyrean/fixtures/vectors/dotnet and random
//! Dotnet/random/ThreadSafeRandom/float-extension shims replay every net10 golden vector
//! (Math.Round, casts, dict/hashset order, number formats, random) and brand rules each hit a
//! vector.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use empyrean_common::dotnet::TimeSpan;
use empyrean_common::dotnet::{
    self, math, CsCast, DotNetDict, DotNetHashSet, MidpointRounding, Num,
};
use empyrean_common::extensions::{float_extensions, time_span_extensions};
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{
    self, f32_of, f64_of, same_f32, same_f64, throws, Case, VectorFile,
};
use serde_json::Value;

// ---------------------------------------------------------------------------------- harness

thread_local!(static QUIET: Cell<bool> = const { Cell::new(false) });
static HOOK: Once = Once::new();

/// Runs `f`, turning a panic (a .NET exception in the shims) into `Err`, without printing it.
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                previous(info);
            }
        }));
    });
    QUIET.with(|q| q.set(true));
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|_| ())
}

/// Collects mismatches over many cases and fails once, listing them.
struct Report {
    name: String,
    total: usize,
    failures: Vec<String>,
}

impl Report {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            total: 0,
            failures: Vec::new(),
        }
    }

    fn check(&mut self, case: &Case, ok: bool, got: impl std::fmt::Debug) {
        self.total += 1;
        if !ok {
            self.failures.push(format!(
                "in {} expected {} got {got:?}",
                case.input, case.output
            ));
        }
    }

    fn finish(self) {
        assert!(self.total > 0, "{}: no cases ran", self.name);
        if !self.failures.is_empty() {
            let shown: Vec<_> = self.failures.iter().take(25).cloned().collect();
            panic!(
                "{}: {} of {} cases differ from net10:\n  {}",
                self.name,
                self.failures.len(),
                self.total,
                shown.join("\n  ")
            );
        }
    }
}

fn load(area: &str, name: &str) -> VectorFile {
    vectors::load_named(area, name)
}

fn field<'a>(case: &'a Case, key: &str) -> &'a Value {
    case.input
        .get(key)
        .unwrap_or_else(|| panic!("case {} has no {key}", case.input))
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("{v} is not an i64"))
}

fn i32_of(v: &Value) -> i32 {
    i32::try_from(int(v)).unwrap_or_else(|_| panic!("{v} is not an i32"))
}

fn dbl(v: &Value) -> f64 {
    f64_of(v).unwrap_or_else(|| panic!("{v} is not a double"))
}

fn flt(v: &Value) -> f32 {
    f32_of(v).unwrap_or_else(|| panic!("{v} is not a float"))
}

fn mode(v: &Value) -> MidpointRounding {
    match v.as_str() {
        Some("ToEven") => MidpointRounding::ToEven,
        Some("AwayFromZero") => MidpointRounding::AwayFromZero,
        Some("ToZero") => MidpointRounding::ToZero,
        Some("ToNegativeInfinity") => MidpointRounding::ToNegativeInfinity,
        Some("ToPositiveInfinity") => MidpointRounding::ToPositiveInfinity,
        _ => panic!("unknown MidpointRounding {v}"),
    }
}

/// Compares a double result (or a panic) with the case's `out`.
fn check_f64(report: &mut Report, case: &Case, got: Result<f64, ()>) {
    let ok = match (throws(&case.output), got) {
        (Some(_), Err(())) => true,
        (None, Ok(g)) => f64_of(&case.output).is_some_and(|e| same_f64(e, g)),
        _ => false,
    };
    report.check(case, ok, got);
}

/// Compares an integer result (or a panic) with the case's `out`.
fn check_int(report: &mut Report, case: &Case, got: Result<i128, ()>) {
    let ok = match (throws(&case.output), got) {
        (Some(_), Err(())) => true,
        (None, Ok(g)) => {
            case.output
                .as_i64()
                .map(i128::from)
                .or_else(|| case.output.as_u64().map(i128::from))
                == Some(g)
        }
        _ => false,
    };
    report.check(case, ok, got);
}

fn check_str(report: &mut Report, case: &Case, got: Result<String, ()>) {
    let ok = match (throws(&case.output), &got) {
        (Some(_), Err(())) => true,
        (None, Ok(g)) => case.output.as_str() == Some(g.as_str()),
        _ => false,
    };
    report.check(case, ok, got);
}

// ---------------------------------------------------------------------------------- brand rule

/// Every string value inside a JSON value.
fn strings_in<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|v| strings_in(v, out)),
        Value::Object(map) => map.values().for_each(|v| strings_in(v, out)),
        _ => {}
    }
}

/// Each brand substitution occurs in some case's output of some vector file, so a re-recorded ACE
/// whose wording changed fails here rather than silently leaving our text untested.
#[test]
fn brand_rules_each_hit_a_vector() {
    let mut hits = vec![0usize; vectors::BRAND_RULES.len()];
    for path in vectors::all_files().unwrap() {
        let file = vectors::load(&path).unwrap();
        let mut texts = Vec::new();
        for case in &file.cases {
            strings_in(&case.output, &mut texts);
        }
        for text in texts {
            for (i, (ace, _)) in vectors::BRAND_RULES.iter().enumerate() {
                hits[i] += usize::from(text.contains(ace));
            }
        }
    }
    let missing: Vec<&str> = vectors::BRAND_RULES
        .iter()
        .zip(&hits)
        .filter(|(_, n)| **n == 0)
        .map(|((ace, _), _)| *ace)
        .collect();
    assert!(
        missing.is_empty(),
        "brand rules no vector output contains: {missing:?}"
    );
    assert_eq!(
        vectors::brand_ruled("x type acehelp < command >."),
        "x type emphelp < command >."
    );
}

// ---------------------------------------------------------------------------------- Math.Round

#[test]
fn math_round_matches_net10() {
    let mut r = Report::new("dotnet/math_round");
    for c in &load("dotnet", "math_round").cases {
        let x = dbl(field(c, "x"));
        check_f64(&mut r, c, catch(|| math::round(x)));
    }
    r.finish();

    let mut r = Report::new("dotnet/math_round_mode");
    for c in &load("dotnet", "math_round_mode").cases {
        let (x, m) = (dbl(field(c, "x")), mode(field(c, "mode")));
        check_f64(&mut r, c, catch(|| math::round_mode(x, m)));
    }
    r.finish();

    let mut r = Report::new("dotnet/math_round_digits");
    for c in &load("dotnet", "math_round_digits").cases {
        let (x, d) = (dbl(field(c, "x")), i32_of(field(c, "digits")));
        check_f64(&mut r, c, catch(|| math::round_digits(x, d)));
    }
    r.finish();

    let mut r = Report::new("dotnet/math_round_digits_mode");
    for c in &load("dotnet", "math_round_digits_mode").cases {
        let (x, d, m) = (
            dbl(field(c, "x")),
            i32_of(field(c, "digits")),
            mode(field(c, "mode")),
        );
        check_f64(&mut r, c, catch(|| math::round_digits_mode(x, d, m)));
    }
    r.finish();
}

// ---------------------------------------------------------------------------------- casts

macro_rules! cast_file {
    ($report:ident, $file:expr, $from:ident, $to:ty) => {{
        let mut $report = Report::new(concat!("dotnet/", $file));
        for c in &load("dotnet", $file).cases {
            let x = $from(field(c, "x"));
            let got: $to = x.cs_cast();
            check_int(&mut $report, c, Ok(i128::from(got)));
        }
        $report.finish();
    }};
}

#[test]
fn float_to_integer_casts_match_net10() {
    cast_file!(r, "cast_double_to_sbyte", dbl, i8);
    cast_file!(r, "cast_double_to_byte", dbl, u8);
    cast_file!(r, "cast_double_to_short", dbl, i16);
    cast_file!(r, "cast_double_to_ushort", dbl, u16);
    cast_file!(r, "cast_double_to_int", dbl, i32);
    cast_file!(r, "cast_double_to_uint", dbl, u32);
    cast_file!(r, "cast_double_to_long", dbl, i64);
    cast_file!(r, "cast_double_to_ulong", dbl, u64);
    cast_file!(r, "cast_float_to_sbyte", flt, i8);
    cast_file!(r, "cast_float_to_byte", flt, u8);
    cast_file!(r, "cast_float_to_short", flt, i16);
    cast_file!(r, "cast_float_to_ushort", flt, u16);
    cast_file!(r, "cast_float_to_int", flt, i32);
    cast_file!(r, "cast_float_to_uint", flt, u32);
    cast_file!(r, "cast_float_to_long", flt, i64);
    cast_file!(r, "cast_float_to_ulong", flt, u64);

    let mut r = Report::new("dotnet/cast_double_to_float");
    for c in &load("dotnet", "cast_double_to_float").cases {
        let got: f32 = dbl(field(c, "x")).cs_cast();
        let ok = f32_of(&c.output).is_some_and(|e| same_f32(e, got));
        r.check(c, ok, got);
    }
    r.finish();
}

#[test]
fn integer_casts_match_net10() {
    let mut r = Report::new("dotnet/cast_integer");
    for c in &load("dotnet", "cast_integer").cases {
        let from = field(c, "from").as_str().unwrap_or_default();
        let to = field(c, "to").as_str().unwrap_or_default();
        let x = field(c, "x");
        let v = i128::from(int(x));
        let got: i128 = match (from, to) {
            ("int", "uint") => i128::from(CsCast::<u32>::cs_cast(i32_of(x))),
            ("int", "byte") => i128::from(CsCast::<u8>::cs_cast(i32_of(x))),
            ("int", "sbyte") => i128::from(CsCast::<i8>::cs_cast(i32_of(x))),
            ("int", "short") => i128::from(CsCast::<i16>::cs_cast(i32_of(x))),
            ("int", "ushort") => i128::from(CsCast::<u16>::cs_cast(i32_of(x))),
            ("uint", "int") => i128::from(CsCast::<i32>::cs_cast(u32::try_from(v).expect("uint"))),
            ("long", "int") => i128::from(CsCast::<i32>::cs_cast(int(x))),
            ("long", "uint") => i128::from(CsCast::<u32>::cs_cast(int(x))),
            ("long", "ulong") => i128::from(CsCast::<u64>::cs_cast(int(x))),
            _ => panic!("unknown cast {from} -> {to}"),
        };
        check_int(&mut r, c, Ok(got));
    }
    r.finish();
}

// ---------------------------------------------------------------------------------- collections

fn ints(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap_or_else(|| panic!("{v} is not an array"))
        .iter()
        .map(int)
        .collect()
}

#[test]
fn dictionary_enumeration_order_matches_net10() {
    let mut r = Report::new("dotnet/dictionary_int_int_order");
    for c in &load("dotnet", "dictionary_int_int_order").cases {
        let mut d = DotNetDict::<i32, i32>::new();
        let mut results = Vec::new();
        let mut trace = Vec::new();
        for op in field(c, "script").as_array().expect("script") {
            let op = op.as_array().expect("op");
            match op[0].as_str().expect("op name") {
                "set" => {
                    d.insert(i32_of(&op[1]), i32_of(&op[2]));
                    results.push(Value::Null);
                }
                "try_add" => results.push(Value::Bool(d.try_add(i32_of(&op[1]), i32_of(&op[2])))),
                "remove" => results.push(Value::Bool(d.remove(&i32_of(&op[1])).is_some())),
                "clear" => {
                    d.clear();
                    results.push(Value::Null);
                }
                other => panic!("unknown op {other}"),
            }
            trace.push(d.keys().map(|&k| i64::from(k)).collect::<Vec<_>>());
        }
        let keys: Vec<i64> = d.keys().map(|&k| i64::from(k)).collect();
        let values: Vec<i64> = d.values().map(|&v| i64::from(v)).collect();
        let out = &c.output;
        let expected_trace: Vec<Vec<i64>> = out["trace"]
            .as_array()
            .expect("trace")
            .iter()
            .map(ints)
            .collect();
        let ok = ints(&out["keys"]) == keys
            && ints(&out["values"]) == values
            && out["results"].as_array().expect("results") == &results
            && expected_trace == trace;
        r.check(c, ok, (keys, values));
    }
    r.finish();
}

#[test]
fn hashset_enumeration_order_matches_net10() {
    let mut r = Report::new("dotnet/hashset_int_order");
    for c in &load("dotnet", "hashset_int_order").cases {
        let mut h = DotNetHashSet::<i32>::new();
        let mut results = Vec::new();
        let mut trace = Vec::new();
        for op in field(c, "script").as_array().expect("script") {
            let op = op.as_array().expect("op");
            match op[0].as_str().expect("op name") {
                "set" | "try_add" => results.push(Value::Bool(h.insert(i32_of(&op[1])))),
                "remove" => results.push(Value::Bool(h.remove(&i32_of(&op[1])))),
                "clear" => {
                    h.clear();
                    results.push(Value::Null);
                }
                other => panic!("unknown op {other}"),
            }
            trace.push(h.iter().map(|&k| i64::from(k)).collect::<Vec<_>>());
        }
        let items: Vec<i64> = h.iter().map(|&k| i64::from(k)).collect();
        let out = &c.output;
        let expected_trace: Vec<Vec<i64>> = out["trace"]
            .as_array()
            .expect("trace")
            .iter()
            .map(ints)
            .collect();
        let ok = ints(&out["items"]) == items
            && out["results"].as_array().expect("results") == &results
            && expected_trace == trace;
        r.check(c, ok, items);
    }
    r.finish();
}

// ---------------------------------------------------------------------------------- ToString

/// The lowercase hex format `x8` (ACE never uses it) is split out into
/// [`lowercase_hex_format_matches_net10`], a V.1 finding.
fn is_lowercase_hex(fmt: &str) -> bool {
    fmt.starts_with('x')
}

fn format_file(name: &str, to_num: impl Fn(&Value) -> Num) {
    format_cases(name, &to_num, |fmt| !is_lowercase_hex(fmt));
}

fn format_cases(name: &str, to_num: &impl Fn(&Value) -> Num, keep: impl Fn(&str) -> bool) {
    let mut r = Report::new(&format!("dotnet/{name}"));
    for c in &load("dotnet", name).cases {
        let fmt = field(c, "format").as_str().expect("format").to_owned();
        if !keep(&fmt) {
            continue;
        }
        let n = to_num(field(c, "x"));
        check_str(&mut r, c, catch(|| dotnet::format(n, &fmt)));
    }
    r.finish();
}

#[test]
fn double_formats_match_net10() {
    format_file("format_double", |v| Num::F64(dbl(v)));
}

#[test]
fn float_formats_match_net10() {
    format_file("format_float", |v| Num::F32(flt(v)));
}

#[test]
fn int_formats_match_net10() {
    format_file("format_int", |v| Num::from(i32_of(v)));
}

#[test]
fn uint_formats_match_net10() {
    format_file("format_uint", |v| {
        Num::from(u32::try_from(int(v)).expect("uint"))
    });
}

#[test]
fn long_formats_match_net10() {
    format_file("format_long", |v| Num::from(int(v)));
}

#[test]
fn ulong_formats_match_net10() {
    format_file("format_ulong", |v| Num::from(v.as_u64().expect("ulong")));
}

#[test]
fn lowercase_hex_format_matches_net10() {
    format_cases(
        "format_int",
        &|v: &Value| Num::from(i32_of(v)),
        is_lowercase_hex,
    );
    format_cases(
        "format_uint",
        &|v: &Value| Num::from(u32::try_from(int(v)).expect("uint")),
        is_lowercase_hex,
    );
    format_cases(
        "format_long",
        &|v: &Value| Num::from(int(v)),
        is_lowercase_hex,
    );
    format_cases(
        "format_ulong",
        &|v: &Value| Num::from(v.as_u64().expect("ulong")),
        is_lowercase_hex,
    );
}

// ---------------------------------------------------------------------------------- random

fn calls(c: &Case) -> &Vec<Value> {
    field(c, "calls").as_array().expect("calls")
}

#[test]
fn system_random_matches_net10() {
    let mut r = Report::new("random/system_random");
    for c in &load("random", "system_random").cases {
        let mut rng = DotNetRandom::new(i32_of(field(c, "seed")));
        let expected = c.output.as_array().expect("out");
        let mut ok = true;
        let mut got = Vec::new();
        for (call, want) in calls(c).iter().zip(expected) {
            let call = call.as_array().expect("call");
            let arg = |i: usize| i32_of(&call[i]);
            let value: Result<Value, ()> = match call[0].as_str().expect("name") {
                "next" => catch(|| Value::from(rng.next())),
                "next_double" => catch(|| Value::from(rng.next_double())),
                "next_max" => catch(|| Value::from(rng.next_max(arg(1)))),
                "next_min_max" => catch(|| Value::from(rng.next_range(arg(1), arg(2)))),
                other => panic!("unknown call {other}"),
            };
            ok &= match (&value, throws(want)) {
                (Err(()), Some(_)) => true,
                (Ok(v), None) => match (v.as_f64(), f64_of(want)) {
                    (Some(g), Some(e)) if v.is_f64() => same_f64(g, e),
                    _ => v.as_i64() == want.as_i64(),
                },
                _ => false,
            };
            got.push(value.map_or_else(|()| "panic".to_owned(), |v| v.to_string()));
        }
        ok &= calls(c).len() == expected.len();
        r.check(c, ok, got);
    }
    r.finish();
}

#[test]
fn thread_safe_random_matches_net10() {
    let mut r = Report::new("random/thread_safe_random");
    for c in &load("random", "thread_safe_random").cases {
        // `seed` takes the low 32 bits as System.Random's int seed.
        ThreadSafeRandom::seed(u64::from(i32_of(field(c, "seed")).cast_unsigned()));
        let expected = c.output.as_array().expect("out");
        let mut ok = calls(c).len() == expected.len();
        let mut got = Vec::new();
        for (call, want) in calls(c).iter().zip(expected) {
            let call = call.as_array().expect("call");
            let value: Result<Value, ()> = match call[0].as_str().expect("name") {
                "next_int" => {
                    let (a, b) = (i32_of(&call[1]), i32_of(&call[2]));
                    catch(|| Value::from(ThreadSafeRandom::next(a, b)))
                }
                "next_float" => {
                    let (a, b) = (flt(&call[1]), flt(&call[2]));
                    catch(|| Value::from(ThreadSafeRandom::next_float(a, b)))
                }
                "next_interval" => {
                    let q = flt(&call[1]);
                    catch(|| Value::from(ThreadSafeRandom::next_interval(q)))
                }
                "next_interval_max" => {
                    let q = flt(&call[1]);
                    catch(|| Value::from(ThreadSafeRandom::next_interval_max(q)))
                }
                other => panic!("unknown call {other}"),
            };
            ok &= match (&value, throws(want)) {
                (Err(()), Some(_)) => true,
                (Ok(v), None) if v.is_f64() => f64_of(want)
                    .zip(v.as_f64())
                    .is_some_and(|(e, g)| same_f64(e, g)),
                (Ok(v), None) => v.as_i64() == want.as_i64(),
                _ => false,
            };
            got.push(value.map_or_else(|()| "panic".to_owned(), |v| v.to_string()));
        }
        r.check(c, ok, got);
    }
    r.finish();
}

// ---------------------------------------------------------------------------------- ACE.Common

#[test]
fn float_extensions_match_ace_on_net10() {
    let mut r = Report::new("common/float_extensions_round_double");
    for c in &load("common", "float_extensions_round_double").cases {
        let (x, p) = (dbl(field(c, "num")), i32_of(field(c, "decimal_places")));
        check_int(
            &mut r,
            c,
            catch(|| i128::from(float_extensions::round_f64(x, p))),
        );
    }
    r.finish();

    let mut r = Report::new("common/float_extensions_round_float");
    for c in &load("common", "float_extensions_round_float").cases {
        let (x, p) = (flt(field(c, "num")), i32_of(field(c, "decimal_places")));
        check_int(
            &mut r,
            c,
            catch(|| i128::from(float_extensions::round(x, p))),
        );
    }
    r.finish();

    let mut r = Report::new("common/float_extensions_truncate");
    for c in &load("common", "float_extensions_truncate").cases {
        let (x, p) = (flt(field(c, "num")), i32_of(field(c, "decimal_places")));
        let got = float_extensions::truncate(x, p);
        let ok = f32_of(&c.output).is_some_and(|e| same_f32(e, got));
        r.check(c, ok, got);
    }
    r.finish();
}

#[test]
fn time_span_extensions_match_ace_on_net10() {
    let mut r = Report::new("common/time_span_extensions");
    for c in &load("common", "time_span_extensions").cases {
        let span = TimeSpan::from_ticks(int(field(c, "ticks")));
        let out = &c.output;
        let friendly = catch(|| time_span_extensions::get_friendly_string(span));
        let friendly_long = catch(|| time_span_extensions::get_friendly_long_string(span));
        let months = catch(|| time_span_extensions::get_months(span));
        let years = catch(|| time_span_extensions::get_years(span));
        let ok = friendly.as_deref().ok() == out["friendly"].as_str()
            && friendly_long.as_deref().ok() == out["friendly_long"].as_str()
            && months.ok().map(u64::from) == out["months"].as_u64()
            && years.ok().map(u64::from) == out["years"].as_u64();
        r.check(c, ok, (friendly, friendly_long, months, years));
    }
    r.finish();
}
