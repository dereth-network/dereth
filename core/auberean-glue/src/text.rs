//! Strings, building strings, string references, the log and the clock.
//!
//! Strings are wide (UTF-16 units). Numbers are written as the client's C library writes them:
//! integers in decimal (`%ld`, `%lu`, 64-bit `%I64u`), floats with six decimals (`%f`), hex as
//! `0x` and eight upper-case digits, instance ids as four groups of four upper-case hex digits.
//! Reading a number from a string follows the C library too: leading white space, a sign, and the
//! base from the prefix (`0x` hex, `0` octal, else decimal), stopping at the first character that
//! cannot continue the number.
//!
//! A string reference names a string table entry (a string id and a table's record id) or holds a
//! literal, and carries variables by id; setting the table by enumeration value or by name asks
//! the host, and keeps only a record id of the string table type.

use auberean_assets::script::WordFilter;
use auberean_dat::{type_of_id, RecordType};
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{NativeObject, TextInfo, Variable, VariableValue};
use auberean_script::value::{join, split, tag, NULL};

use auberean_text as fmt;

use crate::machine::{new_text, ret, text_of};

const TEXT_INFO: u32 = 0x6A;
const TIMER: u32 = 0x6C;
const BUILD: u32 = 0x74;
const LOG: u32 = 0x75;
const TEXT: u32 = 0x76;
const TOKENIZE: u32 = 0x6D;
const STRING_UTILS: u32 = 0x1ACE;
/// The text utilities.
const PS_UTILS: u32 = 0x5D;
/// The case-insensitive string.
const CI_TEXT: u32 = 0x1ACF;

/// The word filter's package, and where the data files say its record is.
const WORD_FILTER: u32 = 0x6B;
const WORD_FILTER_VALUE: u32 = 0x4000_0005;
const WORD_FILTER_MAPPING: u32 = 0x4000_0002;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (WORD_FILTER, 3, format_for_audience),
    (BUILD, 1, concat_text),
    (BUILD, 2, concat_int),
    (BUILD, 3, concat_uint),
    (BUILD, 4, concat_float),
    (BUILD, 5, concat_long),
    (BUILD, 6, concat_ulong),
    (BUILD, 7, concat_double),
    (BUILD, 8, to_hex),
    (BUILD, 0xA, to_instance_id),
    (BUILD, 0xB, to_hex),
    (TEXT, 2, is_empty),
    (TEXT, 3, length),
    (TEXT, 4, clear),
    (TEXT, 5, assign),
    (TEXT, 6, append_text),
    (TEXT, 7, append_int),
    (TEXT, 8, append_int),
    (TEXT, 9, append_hex),
    (TEXT, 0xA, append_uint),
    (TEXT, 0xB, append_hex),
    (TEXT, 0xC, append_float),
    (TEXT, 0xD, find),
    (TEXT, 0xE, substring),
    (TEXT, 0x11, equal),
    (TEXT, 0x12, not_equal),
    (TEXT, 0x17, parse_int),
    (TEXT, 0x18, parse_uint),
    (TEXT, 0x19, parse_long),
    (TEXT, 0x1A, parse_ulong),
    (TEXT, 0x1B, parse_float),
    (TEXT, 0x1E, upper),
    (TEXT, 0x1F, lower),
    (TEXT, 1, hash),
    (TEXT, 0xF, trim),
    (TEXT, 0x10, compare),
    (TEXT, 0x13, less),
    (TEXT, 0x14, greater),
    (TEXT, 0x15, less_or_equal),
    (TEXT, 0x16, greater_or_equal),
    (TEXT, 0x1C, encrypt),
    (TEXT, 0x1D, decrypt),
    (TEXT, 0x20, append_long),
    (TEXT, 0x21, append_double),
    (TEXT, 0x22, replace),
    // The case-insensitive string's natives: the string's, the client binding both to one function.
    (CI_TEXT, 0x01, hash),
    (CI_TEXT, 0x02, is_empty),
    (CI_TEXT, 0x03, length),
    (CI_TEXT, 0x04, clear),
    (CI_TEXT, 0x05, assign),
    (CI_TEXT, 0x06, append_text),
    (CI_TEXT, 0x07, append_int),
    (CI_TEXT, 0x08, append_long),
    (CI_TEXT, 0x09, append_int),
    (CI_TEXT, 0x0A, append_hex),
    (CI_TEXT, 0x0B, append_uint),
    (CI_TEXT, 0x0C, append_hex),
    (CI_TEXT, 0x0D, append_float),
    (CI_TEXT, 0x0E, find),
    (CI_TEXT, 0x0F, substring),
    (CI_TEXT, 0x10, trim),
    (CI_TEXT, 0x11, compare),
    (CI_TEXT, 0x12, equal),
    (CI_TEXT, 0x13, not_equal),
    (CI_TEXT, 0x14, less),
    (CI_TEXT, 0x15, greater),
    (CI_TEXT, 0x16, less_or_equal),
    (CI_TEXT, 0x17, greater_or_equal),
    (CI_TEXT, 0x18, replace),
    (CI_TEXT, 0x19, parse_int),
    (CI_TEXT, 0x1A, parse_uint),
    (CI_TEXT, 0x1B, parse_long),
    (CI_TEXT, 0x1C, parse_ulong),
    (CI_TEXT, 0x1D, parse_float),
    (CI_TEXT, 0x1E, encrypt),
    (CI_TEXT, 0x1F, decrypt),
    (CI_TEXT, 0x20, upper),
    (CI_TEXT, 0x21, lower),
    (STRING_UTILS, 0x01, is_word_in_string),
    (TEXT_INFO, 2, add_text_variable),
    (TEXT_INFO, 3, add_int_variable),
    (TEXT_INFO, 5, add_uint_variable),
    (TEXT_INFO, 9, add_info_variable),
    (TEXT_INFO, 0xB, set_table_by_enum),
    (TEXT_INFO, 0xC, set_id_and_table_by_enum),
    (TEXT_INFO, 0xD, set_table_by_name),
    (TEXT_INFO, 0xE, set_id_and_table_by_name),
    (TEXT_INFO, 0xF, set_id_and_table),
    (TEXT_INFO, 0x12, set_literal),
    (TEXT_INFO, 0x15, has_table),
    (TEXT_INFO, 0x16, has_string_and_table),
    (TEXT_INFO, 0x17, is_valid),
    (TEXT_INFO, 0x19, reset),
    (TEXT_INFO, 0x1A, info_equal),
    (TEXT_INFO, 0x1B, info_not_equal),
    (TEXT_INFO, 0x23, add_ulong_variable),
    (TEXT_INFO, 1, add_string_id_variable),
    (TEXT_INFO, 4, add_formatted_int_variable),
    (TEXT_INFO, 6, add_formatted_uint_variable),
    (TEXT_INFO, 7, add_float_variable),
    (TEXT_INFO, 8, add_formatted_float_variable),
    (TEXT_INFO, 0x21, add_long_variable),
    (TEXT_INFO, 0x22, add_formatted_long_variable),
    (TEXT_INFO, 0x24, add_formatted_ulong_variable),
    (TEXT_INFO, 0x10, get_string),
    (TEXT_INFO, 0x11, inq_string),
    (TEXT_INFO, 0x13, is_literal),
    (TEXT_INFO, 0x18, get_log_string),
    (TEXT_INFO, 0x1D, get_string_with_flags),
    (TEXT_INFO, 0x1F, get_string_id),
    (TEXT_INFO, 0x20, get_table_id),
    (TOKENIZE, 1, tokenize),
    (PS_UTILS, 0x01, current_folder),
    (PS_UTILS, 0x02, folder_name_cleaned),
    (PS_UTILS, 0x03, is_int32),
    (PS_UTILS, 0x04, is_uint32),
    (PS_UTILS, 0x05, is_int64),
    (PS_UTILS, 0x06, is_uint64),
    (PS_UTILS, 0x07, is_double),
    (PS_UTILS, 0x08, is_double),
    (PS_UTILS, 0x09, remove_extension),
    (PS_UTILS, 0x0A, strip_directory),
    (TIMER, 1, time),
    (TIMER, 2, time),
    (TIMER, 3, real_time),
    (LOG, 1, not_logging),
    (LOG, 2, not_logging),
    (LOG, 9, no_log),
    (LOG, 0xA, no_log),
    (LOG, 0xB, log_message),
    (LOG, 0xC, log_message),
    (LOG, 0xD, log_message),
    (LOG, 0xE, log_message),
    (LOG, 0xF, log_message),
    (LOG, 0x10, log_message),
    (LOG, 0x11, log_message),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

// ---------------------------------------------------------------------------------------------
// Formatting and parsing numbers
// ---------------------------------------------------------------------------------------------

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// A float as `%f` writes it: six decimals, rounded as the C library rounds.
pub(crate) fn fixed(x: f64) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() {
            "-1.#IND00".into()
        } else {
            "1.#QNAN0".into()
        };
    }
    if x.is_infinite() {
        return if x < 0.0 {
            "-1.#INF00".into()
        } else {
            "1.#INF00".into()
        };
    }
    fmt::crt::fixed(x, 6)
}

/// The leading number of a string as the C library reads it: its digits, sign and base.
fn leading(text: &[u16]) -> (bool, u64, bool) {
    let s: Vec<char> = char::decode_utf16(text.iter().copied())
        .map(|r| r.unwrap_or('\u{fffd}'))
        .collect();
    let mut i = 0;
    while i < s.len() && matches!(s[i], ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}') {
        i += 1;
    }
    let mut negative = false;
    if i < s.len() && (s[i] == '+' || s[i] == '-') {
        negative = s[i] == '-';
        i += 1;
    }
    let mut base = 10;
    if i + 1 < s.len()
        && s[i] == '0'
        && (s[i + 1] == 'x' || s[i + 1] == 'X')
        && s.get(i + 2).is_some_and(char::is_ascii_hexdigit)
    {
        base = 16;
        i += 2;
    } else if i < s.len() && s[i] == '0' {
        base = 8;
    }
    let mut v: u64 = 0;
    let mut overflow = false;
    while i < s.len() {
        let Some(d) = s[i].to_digit(base) else { break };
        match v
            .checked_mul(u64::from(base))
            .and_then(|x| x.checked_add(u64::from(d)))
        {
            Some(x) => v = x,
            None => overflow = true,
        }
        i += 1;
    }
    (negative, v, overflow)
}

/// A signed 32-bit number, saturating as the C library does.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn to_long(text: &[u16]) -> u32 {
    let (neg, v, overflow) = leading(text);
    let v = if overflow { u64::MAX } else { v };
    if neg {
        if v > 1 << 31 {
            i32::MIN as u32
        } else {
            (v as i64).wrapping_neg() as i32 as u32
        }
    } else if v > i32::MAX as u64 {
        i32::MAX as u32
    } else {
        v as u32
    }
}

/// An unsigned 32-bit number: out of range saturates, a minus sign negates.
#[allow(clippy::cast_possible_truncation)]
fn to_ulong(text: &[u16]) -> u32 {
    let (neg, v, overflow) = leading(text);
    if overflow || v > u64::from(u32::MAX) {
        return u32::MAX;
    }
    let v = v as u32;
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// An unsigned 64-bit number, in base 0.
fn to_u64(text: &[u16]) -> u64 {
    fmt::wide::to_uint64(text, 0).0
}

/// A signed 64-bit number, in base 0.
fn to_i64(text: &[u16]) -> u64 {
    fmt::wide::to_int64(text, 0).0.cast_unsigned()
}

/// A float as the C library reads one: white space passed over, then the longest leading part
/// that is a number (see [`reads_as_whole_double`]). There is no infinity or not-a-number to
/// read, and an exponent may be written with `d`.
fn to_float(text: &[u16]) -> f64 {
    let at = |i: usize| {
        text.get(i)
            .copied()
            .filter(|&u| u != 0)
            .and_then(number_char)
    };
    let mut i = 0;
    while text.get(i).is_some_and(|&u| crt_space(u)) {
        i += 1;
    }
    let mut number = String::new();
    if let Some(sign @ (b'+' | b'-')) = at(i) {
        number.push(char::from(sign));
        i += 1;
    }
    let (mut point, mut any) = (false, false);
    loop {
        match at(i) {
            Some(b'.') if !point => {
                point = true;
                number.push('.');
            }
            Some(d) if d.is_ascii_digit() => {
                any = true;
                number.push(char::from(d));
            }
            _ => break,
        }
        i += 1;
    }
    if !any {
        return 0.0;
    }
    if matches!(at(i), Some(b'e' | b'E' | b'd' | b'D')) {
        let mut j = i + 1;
        let mut exponent = String::from("e");
        if let Some(sign @ (b'+' | b'-')) = at(j) {
            exponent.push(char::from(sign));
            j += 1;
        }
        let digits = exponent.len();
        while let Some(d) = at(j).filter(u8::is_ascii_digit) {
            exponent.push(char::from(d));
            j += 1;
        }
        if exponent.len() > digits {
            number.push_str(&exponent);
        }
    }
    number.parse().unwrap_or(0.0)
}

// ---------------------------------------------------------------------------------------------
// Building strings
// ---------------------------------------------------------------------------------------------

fn text_arg(c: &Call<'_>, k: i64) -> Result<Vec<u16>, Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(c.fail("a string argument is the null reference"));
    }
    text_of(c, r)
}

/// `text` and `number` joined, the number first when `order` is not zero.
fn joined(text: Vec<u16>, number: &str, order: u32) -> Vec<u16> {
    let n = units(number);
    if order == 0 {
        text.into_iter().chain(n).collect()
    } else {
        n.into_iter().chain(text).collect()
    }
}

/// A name's written form, given the audience (`fp[-2]`, unused), the name (`fp[-3]`) and an out
/// reference (`fp[-4]`) to the string it goes into: answers (`fp[-5]`) whether the form differs from
/// what was typed. Data files with no word filter leave the string alone and answer 0.
// D2-09005, D2-14301
fn format_for_audience(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = text_arg(c, -3)?;
    let out = c.cx.at(-4)?;
    let has_filter = c.host.database().is_some_and(|db| {
        db.get_by_enum::<WordFilter>(WORD_FILTER_VALUE, WORD_FILTER_MAPPING)
            .is_some()
    });
    if !has_filter {
        c.write(-5, 0, tag::MIXED)?;
        return Ok(Status::Success);
    }
    let (form, changed) = fmt::names::format_for_audience(&name);
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = form,
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-5, u32::from(changed), tag::MIXED)?;
    Ok(Status::Success)
}

fn give_text(c: &mut Call<'_>, slot: i64, units: Vec<u16>) -> Result<Status, Stop> {
    let id = new_text(c, units);
    c.write(slot, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Two strings joined.
// D2-09591, D2-16500
fn concat_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let mut a = text_arg(c, -2)?;
    a.extend(text_arg(c, -3)?);
    give_text(c, -5, a)
}

/// A string and a signed integer joined.
// D2-09588, D2-16504
fn concat_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    #[allow(clippy::cast_possible_wrap)]
    let v = c.cx.at(-3)? as i32;
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &v.to_string(), order))
}

/// A string and an unsigned integer joined.
// D2-09592, D2-16505
fn concat_uint(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = c.cx.at(-3)?;
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &v.to_string(), order))
}

/// A string and a float joined.
// D2-09587, D2-16506
fn concat_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = f64::from(f32::from_bits(c.cx.at(-3)?));
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &fixed(v), order))
}

/// A string and a signed 64-bit integer joined.
// D2-09590, D2-16507
fn concat_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    #[allow(clippy::cast_possible_wrap)]
    let v = join(c.cx.at(-4)?, c.cx.at(-3)?) as i64;
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &v.to_string(), order))
}

/// A string and an unsigned 64-bit integer joined.
// D2-09593, D2-16508
fn concat_ulong(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = join(c.cx.at(-4)?, c.cx.at(-3)?);
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &v.to_string(), order))
}

/// A string and a 64-bit float joined.
// D2-09589, D2-16509
fn concat_double(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = f64::from_bits(join(c.cx.at(-4)?, c.cx.at(-3)?));
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &fixed(v), order))
}

/// A number as `0x` and eight upper-case hex digits.
// D2-09584, D2-16512, D2-16510
fn to_hex(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    give_text(c, -3, units(&format!("0x{v:08X}")))
}

/// A 64-bit instance id as four groups of four hex digits, the high word first.
// D2-09585, D2-16501
fn to_instance_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = join(c.cx.at(-3)?, c.cx.at(-2)?);
    let g = |s: u32| (v >> s) & 0xFFFF;
    give_text(
        c,
        -4,
        units(&format!(
            "{:04X}:{:04X}:{:04X}:{:04X}",
            g(48),
            g(32),
            g(16),
            g(0)
        )),
    )
}

// ---------------------------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------------------------

fn this_text<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut Vec<u16>>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Text(t) => Ok(Some(t)),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a string".into(),
        }),
    }
}

macro_rules! text_or_crash {
    ($c:ident) => {
        match this_text($c)? {
            Some(t) => t,
            None => return Ok(Status::Crash),
        }
    };
}

/// Whether the string is empty.
// D2-09241
fn is_empty(c: &mut Call<'_>) -> Result<Status, Stop> {
    let empty = text_or_crash!(c).is_empty();
    ret(c, 0, u32::from(empty), tag::MIXED)?;
    Ok(Status::Success)
}

/// The string's stored length, which counts its terminating zero: one more than its units.
// D2-09248
fn length(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_truncation)]
    let n = text_or_crash!(c).len() as u32 + 1;
    ret(c, 0, n, tag::MIXED)?;
    Ok(Status::Success)
}

/// Empty the string.
// D2-09594
fn clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    text_or_crash!(c).clear();
    Ok(Status::Success)
}

/// Copy another string into this one.
// D2-09595
fn assign(c: &mut Call<'_>) -> Result<Status, Stop> {
    let other = text_arg(c, -2)?;
    *text_or_crash!(c) = other;
    Ok(Status::Success)
}

/// Append another string.
// D2-09608
fn append_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let other = text_arg(c, -2)?;
    text_or_crash!(c).extend(other);
    Ok(Status::Success)
}

/// Append a signed integer (a truth value as 0 or 1).
// D2-09606, D2-09611
fn append_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let v = c.cx.at(-2)? as i32;
    text_or_crash!(c).extend(units(&v.to_string()));
    Ok(Status::Success)
}

/// Append an unsigned integer.
// D2-09609
fn append_uint(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    text_or_crash!(c).extend(units(&v.to_string()));
    Ok(Status::Success)
}

/// Append a number in hex.
// D2-09610, D2-09253
fn append_hex(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    text_or_crash!(c).extend(units(&format!("0x{v:08X}")));
    Ok(Status::Success)
}

/// Append a float.
// D2-09605
fn append_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f64::from(f32::from_bits(c.cx.at(-2)?));
    text_or_crash!(c).extend(units(&fixed(v)));
    Ok(Status::Success)
}

/// Where another string first occurs, -1 when it does not.
// D2-09243
fn find(c: &mut Call<'_>) -> Result<Status, Stop> {
    let needle = text_arg(c, -2)?;
    let hay = text_or_crash!(c).clone();
    let at = if needle.is_empty() {
        Some(0)
    } else {
        hay.windows(needle.len())
            .position(|w| w == needle.as_slice())
    };
    #[allow(clippy::cast_possible_truncation)]
    ret(c, 1, at.map_or(NULL, |i| i as u32), tag::MIXED)?;
    Ok(Status::Success)
}

/// A new string of the units from `start` to `end` inclusive (`end` past the last: the last);
/// empty when `start` is past the last.
// D2-09612
fn substring(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (start, end) = (c.cx.at(-2)? as usize, c.cx.at(-3)? as usize);
    let s = text_or_crash!(c).clone();
    let out = if start < s.len() {
        let end = end.min(s.len() - 1);
        if end >= start {
            s[start..=end].to_vec()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    give_text(c, -4, out)
}

fn fold(u: u16) -> u16 {
    if (u16::from(b'A')..=u16::from(b'Z')).contains(&u) {
        u + 32
    } else {
        u
    }
}

/// Whether the receiver equals the string at `fp[-2]` as C strings, case kept when the low byte
/// of `fp[-3]` is set; `None` for a null receiver.
fn same_text(c: &mut Call<'_>) -> Result<Option<bool>, Stop> {
    let other = text_arg(c, -2)?;
    let keep_case = c.cx.at(-3)? & 0xFF != 0;
    let Some(this) = this_text(c)? else {
        return Ok(None);
    };
    Ok(Some(fmt::wide::equal(this, &other, keep_case)))
}

/// Whether two strings are equal, case kept when asked.
// D2-09242
fn equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(eq) = same_text(c)? else {
        return Ok(Status::Crash);
    };
    ret(c, 2, u32::from(eq), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether two strings differ, case kept when asked.
// D2-09250
fn not_equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(eq) = same_text(c)? else {
        return Ok(Status::Crash);
    };
    ret(c, 2, u32::from(!eq), tag::MIXED)?;
    Ok(Status::Success)
}

/// The string's leading signed integer.
// D2-09597
fn parse_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = to_long(text_or_crash!(c));
    ret(c, 0, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The string's leading unsigned integer.
// D2-09599
fn parse_uint(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = to_ulong(text_or_crash!(c));
    ret(c, 0, v, tag::MIXED)?;
    Ok(Status::Success)
}

fn long_result(c: &mut Call<'_>, v: u64) -> Result<Status, Stop> {
    let (lo, hi) = split(v);
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// The string's leading signed 64-bit integer.
// D2-09598
fn parse_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = to_i64(text_or_crash!(c));
    long_result(c, v)
}

/// The string's leading unsigned 64-bit integer.
// D2-09600
fn parse_ulong(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = to_u64(text_or_crash!(c));
    long_result(c, v)
}

/// The string's leading float.
// D2-09596
fn parse_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_truncation)]
    let v = to_float(text_or_crash!(c)) as f32;
    ret(c, 0, v.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// Upper-case the string's letters in place, as the C library does in the English locale, up
/// to its first zero.
// D2-09602
fn upper(c: &mut Call<'_>) -> Result<Status, Stop> {
    for u in text_or_crash!(c).iter_mut().take_while(|u| **u != 0) {
        *u = fmt::crt::to_upper(*u);
    }
    Ok(Status::Success)
}

/// Lower-case the string's letters in place, as the C library does in the English locale, up
/// to its first zero.
// D2-09601
fn lower(c: &mut Call<'_>) -> Result<Status, Stop> {
    for u in text_or_crash!(c).iter_mut().take_while(|u| **u != 0) {
        *u = fmt::crt::to_lower(*u);
    }
    Ok(Status::Success)
}

/// The units of a string up to its first zero unit, as the C library's wide string functions
/// read it (only an encrypted string holds a zero unit).
fn c_string(t: &[u16]) -> &[u16] {
    t.iter().position(|u| *u == 0).map_or(t, |n| &t[..n])
}

/// Two strings compared as the C library compares them: case kept, -1, 0 or 1; case ignored
/// (each unit lowered as `towlower` lowers it), the difference of the first two units that differ.
fn order(a: &[u16], b: &[u16], keep_case: bool) -> i32 {
    let (a, b) = (c_string(a), c_string(b));
    let mut k = 0;
    loop {
        let x = a.get(k).copied().unwrap_or(0);
        let y = b.get(k).copied().unwrap_or(0);
        let (x, y) = if keep_case {
            (x, y)
        } else {
            (fmt::crt::to_lower(x), fmt::crt::to_lower(y))
        };
        if x != y || x == 0 {
            let d = i32::from(x) - i32::from(y);
            return if keep_case { d.signum() } else { d };
        }
        k += 1;
    }
}

/// The receiver compared with the string at `fp[-2]`, case kept when the low byte of `fp[-3]` is
/// set; `None` for a null receiver.
fn compared(c: &mut Call<'_>) -> Result<Option<i32>, Stop> {
    let other = text_arg(c, -2)?;
    let keep_case = c.cx.at(-3)? & 0xFF != 0;
    let Some(this) = this_text(c)? else {
        return Ok(None);
    };
    Ok(Some(order(this, &other, keep_case)))
}

/// How the string orders against another (`fp[-2]`), case kept when asked (`fp[-3]`), answered
/// at `fp[-4]`.
// D2-09240
fn compare(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(d) = compared(c)? else {
        return Ok(Status::Crash);
    };
    #[allow(clippy::cast_sign_loss)]
    ret(c, 2, d as u32, tag::MIXED)?;
    Ok(Status::Success)
}

/// One of the four orderings of the string against another, as a truth value at `fp[-4]` (its
/// low byte: the client writes a byte there).
fn ordering(c: &mut Call<'_>, holds: fn(i32) -> bool) -> Result<Status, Stop> {
    let Some(d) = compared(c)? else {
        return Ok(Status::Crash);
    };
    let i = c.cx.frame_index(-4)?;
    c.cx.data[i] = (c.cx.data[i] & !0xFF) | u32::from(holds(d));
    c.cx.tags[i] = tag::MIXED;
    Ok(Status::Success)
}

/// Whether the string orders before another.
// D2-09249, D2-09238
fn less(c: &mut Call<'_>) -> Result<Status, Stop> {
    ordering(c, |d| d < 0)
}

/// Whether the string orders after another.
// D2-09245, D2-09236
fn greater(c: &mut Call<'_>) -> Result<Status, Stop> {
    ordering(c, |d| d > 0)
}

/// Whether the string orders before another or equals it.
// D2-09247, D2-09237
fn less_or_equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    ordering(c, |d| d <= 0)
}

/// Whether the string orders after another or equals it.
// D2-09244, D2-09235
fn greater_or_equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    ordering(c, |d| d >= 0)
}

/// A string's hash: each unit added to the hash shifted left four bits, the top four bits folded
/// back into the low byte's high nibble and cleared; all ones becomes all ones but the lowest bit.
// D2-01377
#[must_use]
pub fn string_hash(t: &[u16]) -> u32 {
    let mut h: u32 = 0;
    for u in c_string(t) {
        h = (h << 4).wrapping_add(u32::from(*u));
        let top = h & 0xF000_0000;
        if top != 0 {
            h = ((top >> 24) ^ h) & 0x0FFF_FFFF;
        }
    }
    if h == u32::MAX {
        0xFFFF_FFFE
    } else {
        h
    }
}

/// The string's hash, answered below the receiver.
// D2-09246
fn hash(c: &mut Call<'_>) -> Result<Status, Stop> {
    let h = string_hash(text_or_crash!(c));
    ret(c, 0, h, tag::MIXED)?;
    Ok(Status::Success)
}

/// The white space a string is trimmed of: space, line feed, carriage return, tab, form feed.
const WHITE_SPACE: [u16; 5] = [0x20, 0x0A, 0x0D, 0x09, 0x0C];

/// The string trimmed of white space at both ends: leading units up to the first zero, trailing
/// ones (a zero unit counting as white space there, as the C library finds a string's own end).
// D2-09256, D2-02273
fn trim(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = text_or_crash!(c);
    let start = t
        .iter()
        .position(|u| *u == 0 || !WHITE_SPACE.contains(u))
        .unwrap_or(t.len());
    let mut end = t.len();
    while end > start && (t[end - 1] == 0 || WHITE_SPACE.contains(&t[end - 1])) {
        end -= 1;
    }
    *t = t[start..end].to_vec();
    Ok(Status::Success)
}

/// The key the string's 32-bit words are scrambled with, a word each in turn.
const SCRAMBLE: [u32; 16] = [
    0xE584_9238,
    0xEB2D_767D,
    0xFC0F_DD03,
    0xCB45_A6A6,
    0x7F21_E6BB,
    0xF5A6_CDC8,
    0x22D9_AD12,
    0xB2A3_D4DE,
    0x3B83_4DB7,
    0xC406_C91C,
    0x8392_305E,
    0x0A9F_0099,
    0x0943_B90C,
    0xA66D_953C,
    0x0D92_9E1F,
    0x0E2E_9AD4,
];

/// Each whole 32-bit word of the units (two units, the first low) exclusive-ored with the key; a
/// last odd unit is left.
// D2-00376
fn scramble_words(t: &mut [u16]) {
    for (k, pair) in t.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let w = (u32::from(pair[0]) | u32::from(pair[1]) << 16) ^ SCRAMBLE[k & 0xF];
        #[allow(clippy::cast_possible_truncation)]
        {
            pair[0] = w as u16;
            pair[1] = (w >> 16) as u16;
        }
    }
}

/// Each byte of the units with its two halves swapped.
fn swap_nibbles(t: &mut [u16]) {
    for u in t.iter_mut() {
        let [lo, hi] = u.to_le_bytes();
        *u = u16::from_le_bytes([lo.rotate_left(4), hi.rotate_left(4)]);
    }
}

/// The string scrambled: each byte's halves swapped, then its words keyed.
// D2-09252, D2-04339
fn encrypt(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = text_or_crash!(c);
    swap_nibbles(t);
    scramble_words(t);
    Ok(Status::Success)
}

/// The string unscrambled: its words keyed, then each byte's halves swapped.
// D2-09251, D2-04338
fn decrypt(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = text_or_crash!(c);
    scramble_words(t);
    swap_nibbles(t);
    Ok(Status::Success)
}

/// Append a signed 64-bit integer (`fp[-3]` low, `fp[-2]` high), in decimal.
// D2-09607, D2-09254
fn append_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let v = long_arg(c, -3)? as i64;
    text_or_crash!(c).extend(units(&v.to_string()));
    Ok(Status::Success)
}

/// Append a 64-bit float (`fp[-3]` low, `fp[-2]` high), with six decimals.
// D2-09604, D2-09603
fn append_double(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f64::from_bits(long_arg(c, -3)?);
    text_or_crash!(c).extend(units(&fixed(v)));
    Ok(Status::Success)
}

/// Every occurrence of a string (`fp[-2]`) replaced by another (`fp[-3]`), searching on after
/// each; how many, answered at `fp[-4]`. Searching for the empty string never ends in the client;
/// the run stops there.
// D2-09257, D2-02279
fn replace(c: &mut Call<'_>) -> Result<Status, Stop> {
    let find = text_arg(c, -2)?;
    let with = text_arg(c, -3)?;
    let find = c_string(&find).to_vec();
    if find.is_empty() {
        return Err(c.fail("replacing the empty string never ends"));
    }
    let Some(t) = this_text(c)? else {
        return Ok(Status::Crash);
    };
    let hay = c_string(t).to_vec();
    let mut out = Vec::with_capacity(hay.len());
    let (mut at, mut n) = (0, 0u32);
    while let Some(k) = hay[at..]
        .windows(find.len())
        .position(|w| w == find.as_slice())
    {
        out.extend_from_slice(&hay[at..at + k]);
        out.extend_from_slice(c_string(&with));
        at += k + find.len();
        n += 1;
    }
    if n > 0 {
        out.extend_from_slice(&hay[at..]);
        *t = out;
    }
    ret(c, 2, n, tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether a word (`fp[-2]`) is one of a text's words (`fp[-3]`), answered below them
/// (`fp[-4]`): the text split at spaces, commas and semicolons, the empty pieces dropped, each
/// piece compared with the word with no regard to the case of ASCII letters.
// D2-09542, D2-09541, D2-03104
fn is_word_in_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let word: Vec<u16> = text_arg(c, -2)?.into_iter().map(fold).collect();
    let text = text_arg(c, -3)?;
    let found = text
        .split(|u| matches!(*u, 0x20 | 0x2C | 0x3B))
        .filter(|piece| !piece.is_empty())
        .any(|piece| piece.iter().map(|u| fold(*u)).eq(word.iter().copied()));
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// String references
// ---------------------------------------------------------------------------------------------

/// The string table record type.
const STRING_TABLE: u32 = RecordType::STRING_TABLE.0;

/// The mapping a table is looked up by enumeration value through.
pub const TABLE_ENUM_MAPPING: u32 = 0x4000_0004;

fn info<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut TextInfo>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::TextInfo(t) => Ok(Some(t)),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a string reference".into(),
        }),
    }
}

macro_rules! info_or_crash {
    ($c:ident) => {
        match info($c)? {
            Some(t) => t,
            None => return Ok(Status::Crash),
        }
    };
}

/// A table id when it is a string table's, else none.
pub fn table_or_none(did: u32) -> u32 {
    if type_of_id(did).is_some_and(|t| t.0 == STRING_TABLE) {
        did
    } else {
        0
    }
}

/// Set the table by enumeration value.
// D2-08988, D2-14119
fn set_table_by_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let e = c.cx.at(-2)?;
    let did = table_or_none(c.host.data_id_by_enum(e, TABLE_ENUM_MAPPING));
    info_or_crash!(c).table = did;
    Ok(Status::Success)
}

/// Set the string id and the table by enumeration value.
// D2-08985, D2-14120
fn set_id_and_table_by_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (id, e) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let did = table_or_none(c.host.data_id_by_enum(e, TABLE_ENUM_MAPPING));
    let t = info_or_crash!(c);
    t.string_id = id;
    t.table = did;
    Ok(Status::Success)
}

/// Set the table by its record's name; an unknown name leaves the table as it was.
fn table_by_name(c: &mut Call<'_>, k: i64) -> Result<Option<u32>, Stop> {
    let name: String = char::decode_utf16(text_arg(c, k)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let did = c.host.data_id_by_name(&name, STRING_TABLE);
    Ok((did != 0).then(|| table_or_none(did)))
}

/// Set the table by name.
// D2-08989, D2-14124
fn set_table_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let did = table_by_name(c, -2)?;
    let t = info_or_crash!(c);
    if let Some(d) = did {
        t.table = d;
    }
    Ok(Status::Success)
}

/// A string reference read from a text tree: each of the node's children by its keyword,
/// `STRINGTABLE` naming the table's record (an unknown name leaves the table as it was) and
/// `STRINGTOKEN` the string, given to `token` as written. Another keyword is warned of and passed
/// over. A keyword with no one text beneath it is an error on the tree and the end of the reading,
/// with what was read before it kept: whether the reading got through.
// D2-14136
pub fn text_from_node_keeping_token(
    info: &mut TextInfo,
    node: Option<&mut auberean_dat::tree::Node>,
    token: &mut Vec<u8>,
    table_by_name: &mut dyn FnMut(&str) -> u32,
) -> bool {
    let Some(node) = node else {
        return false;
    };
    let mut corrupt = false;
    let mut warnings = 0;
    for child in node.children_mut() {
        if child.is_keyword(b"STRINGTABLE") {
            let Some(name) = child.leaf_text() else {
                corrupt = true;
                break;
            };
            let did = table_by_name(&String::from_utf8_lossy(&name));
            if did != 0 {
                info.table = table_or_none(did);
            }
        } else if child.is_keyword(b"STRINGTOKEN") {
            let Some(name) = child.leaf_text() else {
                corrupt = true;
                break;
            };
            info.string_id = auberean_assets::strings::token(&String::from_utf8_lossy(&name));
            *token = name;
        } else {
            warnings += 1;
        }
    }
    for _ in 0..warnings {
        node.report_warning();
    }
    if corrupt {
        node.report_error();
    }
    !corrupt
}

/// [`text_from_node_keeping_token`], the token's text not asked for.
// D2-14137
pub fn text_from_node(
    info: &mut TextInfo,
    node: Option<&mut auberean_dat::tree::Node>,
    table_by_name: &mut dyn FnMut(&str) -> u32,
) -> bool {
    let mut token = Vec::new();
    text_from_node_keeping_token(info, node, &mut token, table_by_name)
}

/// Set the string id, and the table by name.
// D2-08987, D2-14125
fn set_id_and_table_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.cx.at(-2)?;
    let did = table_by_name(c, -3)?;
    let t = info_or_crash!(c);
    t.string_id = id;
    if let Some(d) = did {
        t.table = d;
    }
    Ok(Status::Success)
}

/// Set the string id and the table by record id.
// D2-08986, D2-14121
fn set_id_and_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (id, did) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let t = info_or_crash!(c);
    t.string_id = id;
    t.table = table_or_none(did);
    Ok(Status::Success)
}

/// Make the reference a literal string, its meta language marks escaped (the reference writes
/// the string back as it was given).
// D2-08984, D2-14131, D2-14272
fn set_literal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = fmt::meta::escape(&text_arg(c, -2)?);
    let t = info_or_crash!(c);
    t.literal = Some(s);
    t.has_literal = 1;
    Ok(Status::Success)
}

/// Whether a reference names a string there is: a literal always; else a string id and a
/// table, the table there and holding the string, and every reference among its variables
/// naming one too.
fn valid(c: &mut Call<'_>, t: &TextInfo) -> Result<bool, Stop> {
    if t.has_literal != 0 {
        return Ok(true);
    }
    if t.string_id == 0 || t.table == 0 || !c.host.string_exists(t.table, t.string_id)? {
        return Ok(false);
    }
    for v in &t.variables {
        if let VariableValue::Nested(n) = &v.value {
            if !valid(c, n)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Whether the reference names a table (below the receiver).
// D2-08992
fn has_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    let yes = info_or_crash!(c).table != 0;
    c.write(-2, u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the reference names both a string and a table (below the receiver).
// D2-08991
fn has_string_and_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = info_or_crash!(c);
    let yes = t.string_id != 0 && t.table != 0;
    c.write(-2, u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the reference names a string there is (below the receiver).
// D2-08993, D2-14138, D2-14094
fn is_valid(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = info_or_crash!(c).clone();
    let v = valid(c, &t)?;
    c.write(-2, u32::from(v), tag::MIXED)?;
    Ok(Status::Success)
}

/// Clear the reference: no string, no table, no literal, no variables.
// D2-08983, D2-14146
fn reset(c: &mut Call<'_>) -> Result<Status, Stop> {
    *info_or_crash!(c) = TextInfo::default();
    Ok(Status::Success)
}

fn set_var(c: &mut Call<'_>, kind: u16, value: VariableValue) -> Result<Status, Stop> {
    let id = c.cx.at(-2)?;
    info_or_crash!(c).set_variable(Variable { id, kind, value });
    Ok(Status::Success)
}

/// A signed integer variable.
// D2-08976, D2-14153
fn add_int_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-3)?;
    set_var(c, 2, VariableValue::Word(v))
}

/// An unsigned integer variable.
// D2-08980, D2-14155
fn add_uint_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-3)?;
    set_var(c, 6, VariableValue::Word(v))
}

/// An unsigned 64-bit variable.
// D2-08981, D2-14159
fn add_ulong_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = join(c.cx.at(-4)?, c.cx.at(-3)?);
    set_var(c, 12, VariableValue::Long(v))
}

/// A string variable: a nested reference holding the string as its literal, escaped.
// D2-08990, D2-14172
fn add_text_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = fmt::meta::escape(&text_arg(c, -3)?);
    let nested = TextInfo {
        has_literal: 1,
        literal: Some(s),
        ..TextInfo::default()
    };
    set_var(c, 9, VariableValue::Nested(Box::new(nested)))
}

/// A string reference variable: a copy of another reference.
// D2-08979, D2-14171
fn add_info_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.cx.at(-3)?;
    let other = match c.vm.heap.native(r)? {
        NativeObject::TextInfo(t) => t.clone(),
        _ => return Err(c.fail("a string reference argument is not one")),
    };
    set_var(c, 9, VariableValue::Nested(Box::new(other)))
}

fn same_info(c: &mut Call<'_>) -> Result<Option<bool>, Stop> {
    let r = c.cx.at(-2)?;
    let other = match c.vm.heap.native(r)? {
        NativeObject::TextInfo(t) => t.clone(),
        _ => return Err(c.fail("a string reference argument is not one")),
    };
    let Some(this) = info(c)? else {
        return Ok(None);
    };
    Ok(Some(
        this.string_id == other.string_id
            && this.table == other.table
            && this.literal == other.literal
            && this.variables == other.variables,
    ))
}

/// Whether two references are the same string with the same variables.
// D2-08994, D2-14147
fn info_equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(eq) = same_info(c)? else {
        return Ok(Status::Crash);
    };
    ret(c, 1, u32::from(eq), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether two references differ.
// D2-08995
fn info_not_equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(eq) = same_info(c)? else {
        return Ok(Status::Crash);
    };
    ret(c, 1, u32::from(!eq), tag::MIXED)?;
    Ok(Status::Success)
}

/// A 64-bit argument: its low word at `fp[k]`, its high word above it.
fn long_arg(c: &Call<'_>, k: i64) -> Result<u64, Stop> {
    Ok(join(c.cx.at(k)?, c.cx.at(k + 1)?))
}

/// A 16-bit argument.
fn half_arg(c: &Call<'_>, k: i64) -> Result<u16, Stop> {
    #[allow(clippy::cast_possible_truncation)]
    Ok(c.cx.at(k)? as u16)
}

/// A string reference variable: a reference to a token, its table by enumeration value.
// D2-08978, D2-14170
fn add_string_id_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (string_id, e) = (c.cx.at(-3)?, c.cx.at(-4)?);
    let table = table_or_none(c.host.data_id_by_enum(e, TABLE_ENUM_MAPPING));
    let nested = TextInfo {
        string_id,
        table,
        ..TextInfo::default()
    };
    set_var(c, 9, VariableValue::Nested(Box::new(nested)))
}

/// A signed integer variable with its grouping and base.
// D2-08972, D2-14154
fn add_formatted_int_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (v, g, b) = (c.cx.at(-3)?, c.cx.at(-4)?, half_arg(c, -5)?);
    set_var(c, 3, VariableValue::Words(v, g, b))
}

/// An unsigned integer variable with its grouping and base.
// D2-08974, D2-14156
fn add_formatted_uint_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (v, g, b) = (c.cx.at(-3)?, c.cx.at(-4)?, half_arg(c, -5)?);
    set_var(c, 7, VariableValue::Words(v, g, b))
}

/// A float variable (kept as a double) with its number of decimals.
// D2-08970, D2-14161
fn add_float_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f64::from(f32::from_bits(c.cx.at(-3)?));
    let d = half_arg(c, -4)?;
    set_var(c, 4, VariableValue::Double(v.to_bits(), d))
}

/// A double variable with its decimals, grouping and base.
// D2-08971, D2-14162
fn add_formatted_float_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = long_arg(c, -4)?;
    let (d, g, b) = (half_arg(c, -5)?, c.cx.at(-6)?, half_arg(c, -7)?);
    set_var(c, 5, VariableValue::DoubleWords(v, d, g, b))
}

/// A signed 64-bit variable.
// D2-08977, D2-14157
fn add_long_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = long_arg(c, -4)?;
    set_var(c, 10, VariableValue::Long(v))
}

/// A signed 64-bit variable with its grouping and base.
// D2-08973, D2-14158
fn add_formatted_long_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = long_arg(c, -4)?;
    let (g, b) = (c.cx.at(-5)?, half_arg(c, -6)?);
    set_var(c, 11, VariableValue::LongWords(v, g, b))
}

/// An unsigned 64-bit variable with its grouping and base.
// D2-08975, D2-14160
fn add_formatted_ulong_variable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = long_arg(c, -4)?;
    let (g, b) = (c.cx.at(-5)?, half_arg(c, -6)?);
    set_var(c, 13, VariableValue::LongWords(v, g, b))
}

// ---------------------------------------------------------------------------------------------
// Writing string references
// ---------------------------------------------------------------------------------------------

/// A script's string reference as the formatter takes it.
#[must_use]
pub fn to_formatter(t: &TextInfo) -> fmt::StringInfo {
    fmt::StringInfo {
        string_id: t.string_id,
        table: t.table,
        literal: (t.has_literal != 0).then(|| t.literal.clone().unwrap_or_default()),
        variables: t
            .variables
            .iter()
            .map(|v| fmt::Variable {
                id: v.id,
                value: value_of(v.kind, &v.value),
            })
            .collect(),
    }
}

/// A variable's value by its kind.
#[allow(clippy::cast_possible_wrap)]
fn value_of(kind: u16, v: &VariableValue) -> fmt::Value {
    match (kind, v) {
        (2, VariableValue::Word(a)) => fmt::Value::Int(*a as i32),
        (6, VariableValue::Word(a)) => fmt::Value::UInt(*a),
        (3, VariableValue::Words(a, g, b)) => fmt::Value::FormattedInt {
            value: *a as i32,
            grouping: g.cast_signed(),
            base: *b,
        },
        (7, VariableValue::Words(a, g, b)) => fmt::Value::FormattedUInt {
            value: *a,
            grouping: g.cast_signed(),
            base: *b,
        },
        (4, VariableValue::Double(v, d)) => fmt::Value::Float {
            value: f64::from_bits(*v),
            decimals: *d,
        },
        (5, VariableValue::DoubleWords(v, d, g, b)) => fmt::Value::FormattedFloat {
            value: f64::from_bits(*v),
            decimals: *d,
            grouping: g.cast_signed(),
            base: *b,
        },
        (9, VariableValue::Nested(n)) => fmt::Value::Text(Box::new(to_formatter(n))),
        (10, VariableValue::Long(v)) => fmt::Value::Long(*v as i64),
        (12, VariableValue::Long(v)) => fmt::Value::ULong(*v),
        (11, VariableValue::LongWords(v, g, b)) => fmt::Value::FormattedLong {
            value: *v as i64,
            grouping: g.cast_signed(),
            base: *b,
        },
        (13, VariableValue::LongWords(v, g, b)) => fmt::Value::FormattedULong {
            value: *v,
            grouping: g.cast_signed(),
            base: *b,
        },
        (k, _) => fmt::Value::Nothing(k),
    }
}

/// The tables nested references took while being written, back into the script's reference.
fn keep_tables(t: &mut TextInfo, s: &fmt::StringInfo) {
    t.table = s.table;
    for (a, b) in t.variables.iter_mut().zip(&s.variables) {
        if let (VariableValue::Nested(x), fmt::Value::Text(y)) = (&mut a.value, &b.value) {
            keep_tables(x, y);
        }
    }
}

/// No tables and no language: references to tables write as missing ones.
struct NoTables;

impl fmt::Source for NoTables {
    fn table(&mut self, _: u32) -> Option<std::sync::Arc<fmt::Table>> {
        None
    }
    fn language(&mut self) -> Option<std::sync::Arc<fmt::Language>> {
        None
    }
}

/// A reference's log line, flags kept, as a dump writes one it holds.
pub(crate) fn log_line(c: &mut Call<'_>, t: &TextInfo) -> String {
    let mut s = to_formatter(t);
    let text = match c.host.text() {
        Some(src) => s.log_string(src, true),
        None => s.log_string(&mut NoTables, true),
    };
    String::from_utf16_lossy(&text)
}

/// How a native writes its reference.
#[derive(Clone, Copy)]
enum Writing {
    /// Its text, flags kept or not.
    Text { keep_flags: bool },
    /// Its text without flags, and whether it rendered.
    Inquiry,
    /// Its log line, flags kept.
    Log,
}

/// Write the receiver, keeping the tables its nested references took, and tell the host; `None`
/// when the receiver is the null reference. Answers whether it rendered, and the text.
fn written(c: &mut Call<'_>, how: Writing) -> Result<Option<(bool, Vec<u16>)>, Stop> {
    let Some(t) = info(c)? else {
        return Ok(None);
    };
    let before = t.clone();
    let mut s = to_formatter(t);
    let mut write = |src: &mut dyn fmt::Source| match how {
        Writing::Text { keep_flags } => s.inq(src, keep_flags),
        Writing::Inquiry => s.inq(src, false),
        Writing::Log => (true, s.log_string(src, true)),
    };
    let (ok, text) = match c.host.text() {
        Some(src) => write(src),
        None => write(&mut NoTables),
    };
    if let Some(t) = info(c)? {
        keep_tables(t, &s);
    }
    c.host.wrote_text(&before, &text);
    Ok(Some((ok, text)))
}

/// The reference's text without its flags: a new string (below the receiver).
// D2-08997, D2-14126
fn get_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    match written(c, Writing::Text { keep_flags: false })? {
        Some((_, text)) => give_text(c, -2, text),
        None => Ok(Status::Crash),
    }
}

/// The reference's text with its flags: a new string (below the receiver).
// D2-08999, D2-14127
fn get_string_with_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    match written(c, Writing::Text { keep_flags: true })? {
        Some((_, text)) => give_text(c, -2, text),
        None => Ok(Status::Crash),
    }
}

/// The reference's text, written into the string an out reference holds; true when it rendered.
// D2-09002, D2-14128
fn inq_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let out = c.cx.at(-2)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Ok(Status::Crash);
    }
    let Some((ok, text)) = written(c, Writing::Inquiry)? else {
        return Ok(Status::Crash);
    };
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = text,
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The reference for a log: its text with flags in parentheses, or why it could not be written.
// D2-08996, D2-14135
fn get_log_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    match written(c, Writing::Log)? {
        Some((_, text)) => give_text(c, -2, text),
        None => Ok(Status::Crash),
    }
}

/// Whether the reference is a literal: its literal word, as stored.
// D2-01373
fn is_literal(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let v = i32::from(info_or_crash!(c).has_literal as i16) as u32;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The reference's token.
// D2-18959
fn get_string_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = info_or_crash!(c).string_id;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The reference's table.
// D2-08505
fn get_table_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = info_or_crash!(c).table;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// A string's token: each unit (up to the first zero) shifted to the top seven bits and folded in
/// at a place that moves seven bits a unit.
// D2-14276
#[must_use]
pub fn token_of(units: &[u16]) -> u32 {
    let mut h = 0u32;
    let mut s = 0u32;
    for &u in units.iter().take_while(|&&u| u != 0) {
        let d = u32::from(u) << 25;
        h ^= d >> s;
        if s > 25 {
            h ^= d << (32 - s);
        }
        s = (s + 7) & 31;
    }
    h
}

/// A string's token (below the string).
// D2-09006, D2-14277
fn tokenize(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.cx.at(-2)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let t = token_of(&text_of(c, r)?);
    c.write(-3, t, tag::MIXED)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Text utilities
// ---------------------------------------------------------------------------------------------

/// Whether the C library reads all of a text (to its first zero unit) as a whole number of
/// `bits` bits, signed or not, in base 0, without its range error: leading white space, a sign,
/// `0x` or `0X` for base 16, a leading `0` for base 8, else base 10, then digits and letters
/// below the base. A text with no digit is read as nothing, which counts only when the text is
/// empty. Too many for `bits` bits is a range error; a signed number past its range is one too,
/// while an unsigned one takes a minus sign as negation.
#[must_use]
pub fn reads_as_whole_number(t: &[u16], bits: u32, signed: bool) -> bool {
    let t = &t[..t.iter().position(|&u| u == 0).unwrap_or(t.len())];
    let at = |i: usize| t.get(i).copied().unwrap_or(0);
    let mut i = 0;
    while crt_space(at(i)) {
        i += 1;
    }
    let negative = at(i) == u16::from(b'-');
    if negative || at(i) == u16::from(b'+') {
        i += 1;
    }
    let x = |u: u16| u == u16::from(b'x') || u == u16::from(b'X');
    let base: u64 = if at(i) != u16::from(b'0') {
        10
    } else if x(at(i + 1)) {
        i += 2;
        16
    } else {
        8
    };
    let max = if bits == 64 {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let (mut n, mut read, mut over) = (0u64, false, false);
    loop {
        let u = at(i);
        let d = match u8::try_from(u) {
            Ok(b @ b'0'..=b'9') => u64::from(b - b'0'),
            Ok(b @ (b'a'..=b'z' | b'A'..=b'Z')) => u64::from(b.to_ascii_uppercase() - b'A' + 10),
            _ => break,
        };
        if d >= base {
            break;
        }
        read = true;
        match n.checked_mul(base).and_then(|m| m.checked_add(d)) {
            Some(m) if m <= max => n = m,
            _ => over = true,
        }
        i += 1;
    }
    if !read {
        return t.is_empty();
    }
    let half = 1u64 << (bits - 1);
    let range = over || (signed && if negative { n > half } else { n >= half });
    i == t.len() && !range
}

/// Whether the C library takes a unit for white space: its own table below 256 (tab, line feed,
/// vertical tab, form feed, carriage return, space and the no-break space; not the next-line
/// control 0x85), and the system's character types above (taken here as Unicode's white space).
fn crt_space(u: u16) -> bool {
    match u {
        0x09..=0x0D | 0x20 | 0xA0 => true,
        0..=0xFF => false,
        _ => char::from_u32(u32::from(u)).is_some_and(char::is_whitespace),
    }
}

/// The character of a number a unit becomes when the C library narrows it in the client's
/// Western code page, best fit allowed: the digits, signs, point and exponent letters themselves,
/// and the units the code page writes as one of them (full-width forms, sub- and superscript
/// digits, letters with marks, dashes and box lines, infinity as `8`). Any other unit ends a
/// number.
fn number_char(u: u16) -> Option<u8> {
    const FITS: &[(u16, u8)] = &[
        (0x010E, b'D'),
        (0x010F, b'd'),
        (0x0111, b'd'),
        (0x0112, b'E'),
        (0x0113, b'e'),
        (0x0114, b'E'),
        (0x0115, b'e'),
        (0x0116, b'E'),
        (0x0117, b'e'),
        (0x0118, b'E'),
        (0x0119, b'e'),
        (0x011A, b'E'),
        (0x011B, b'e'),
        (0x03B4, b'd'),
        (0x03B5, b'e'),
        (0x2010, b'-'),
        (0x2011, b'-'),
        (0x2074, b'4'),
        (0x2075, b'5'),
        (0x2076, b'6'),
        (0x2077, b'7'),
        (0x2078, b'8'),
        (0x2080, b'0'),
        (0x2081, b'1'),
        (0x2082, b'2'),
        (0x2083, b'3'),
        (0x2084, b'4'),
        (0x2085, b'5'),
        (0x2086, b'6'),
        (0x2087, b'7'),
        (0x2088, b'8'),
        (0x2089, b'9'),
        (0x2107, b'E'),
        (0x212E, b'e'),
        (0x212F, b'e'),
        (0x2130, b'E'),
        (0x2212, b'-'),
        (0x221E, b'8'),
        (0x2500, b'-'),
        (0x250C, b'+'),
        (0x2510, b'+'),
        (0x2514, b'+'),
        (0x2518, b'+'),
        (0x251C, b'+'),
        (0x252C, b'-'),
        (0x2534, b'-'),
        (0x253C, b'+'),
        (0x2550, b'-'),
        (0x2552, b'+'),
        (0x2553, b'+'),
        (0x2554, b'+'),
        (0x2555, b'+'),
        (0x2556, b'+'),
        (0x2557, b'+'),
        (0x2558, b'+'),
        (0x2559, b'+'),
        (0x255A, b'+'),
        (0x255B, b'+'),
        (0x255C, b'+'),
        (0x255D, b'+'),
        (0x2564, b'-'),
        (0x2565, b'-'),
        (0x2566, b'-'),
        (0x2567, b'-'),
        (0x2568, b'-'),
        (0x2569, b'-'),
        (0x256A, b'+'),
        (0x256B, b'+'),
        (0x256C, b'+'),
        (0xFF0B, b'+'),
        (0xFF0D, b'-'),
        (0xFF0E, b'.'),
        (0xFF10, b'0'),
        (0xFF11, b'1'),
        (0xFF12, b'2'),
        (0xFF13, b'3'),
        (0xFF14, b'4'),
        (0xFF15, b'5'),
        (0xFF16, b'6'),
        (0xFF17, b'7'),
        (0xFF18, b'8'),
        (0xFF19, b'9'),
        (0xFF24, b'D'),
        (0xFF25, b'E'),
        (0xFF44, b'd'),
        (0xFF45, b'e'),
    ];
    match u8::try_from(u) {
        Ok(b @ (b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E' | b'd' | b'D')) => Some(b),
        Ok(_) => None,
        Err(_) => FITS
            .binary_search_by_key(&u, |&(w, _)| w)
            .ok()
            .map(|k| FITS[k].1),
    }
}

/// The most significant digits the C library keeps of a number's digits; the rest are dropped.
const KEPT_DIGITS: usize = 21;

/// Whether the C library reads all of a text (to its first zero unit) as a floating-point number
/// without its range error. Leading white space is passed over; then, each unit narrowed: a sign,
/// digits with at most one point and at least one digit, and an exponent (`e`, `E`, `d` or `D`, a
/// sign, digits; without its digits the exponent is not read). A text with no number is read as
/// nothing, which counts only when the text is empty. The number is its first 21 significant
/// digits, the rest dropped; rounded to 53 bits, below the least normal double (`2^-1022`, so
/// every subnormal) or from `2^1024` on is a range error, and zero never is. The library works
/// the value out in 80-bit arithmetic, which can answer the other way for a number within about
/// one part in `2^64` of either limit.
#[must_use]
pub fn reads_as_whole_double(t: &[u16]) -> bool {
    let t = &t[..t.iter().position(|&u| u == 0).unwrap_or(t.len())];
    let mut i = 0;
    while i < t.len() && crt_space(t[i]) {
        i += 1;
    }
    let at = |i: usize| t.get(i).copied().and_then(number_char);
    let digit = |c: Option<u8>| c.filter(u8::is_ascii_digit).map(|b| b - b'0');
    if matches!(at(i), Some(b'+' | b'-')) {
        i += 1;
    }
    // The significant digits kept, and the power of ten they are scaled by.
    let (mut kept, mut scale, mut any) = (Vec::<u8>::new(), 0i64, false);
    let mut point = false;
    loop {
        match at(i) {
            Some(b'.') if !point => point = true,
            c => {
                let Some(d) = digit(c) else { break };
                any = true;
                if kept.is_empty() && d == 0 {
                    if point {
                        scale -= 1;
                    }
                } else if kept.len() < KEPT_DIGITS {
                    kept.push(d);
                    if point {
                        scale -= 1;
                    }
                } else if !point {
                    scale += 1;
                }
            }
        }
        i += 1;
    }
    if !any {
        return t.is_empty();
    }
    if matches!(at(i), Some(b'e' | b'E' | b'd' | b'D')) {
        let mut j = i + 1;
        let negative = at(j) == Some(b'-');
        if matches!(at(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if digit(at(j)).is_some() {
            let mut e = 0i64;
            while let Some(d) = digit(at(j)) {
                e = (e * 10 + i64::from(d)).min(1 << 40);
                j += 1;
            }
            scale += if negative { -e } else { e };
            i = j;
        }
    }
    i == t.len() && !out_of_range(&kept, scale)
}

/// Whether `digits` (most significant first, the first not zero) times ten to `scale` rounds, to
/// 53 bits, below `2^-1022` or to `2^1024` or more (halves round up). No digits is zero.
fn out_of_range(digits: &[u8], scale: i64) -> bool {
    if digits.is_empty() {
        return false;
    }
    // The number lies in [10^m, 10^(m+1)).
    let m = scale + digits.len() as i64 - 1;
    if m >= 309 || m <= -309 {
        return true;
    }
    if (-307..=306).contains(&m) {
        return false;
    }
    let d = digits
        .iter()
        .fold(Big::from(0), |n, &k| n.times(10).plus(u32::from(k)));
    // The two limits, each where rounding to 53 bits turns: 2^-1022 - 2^-1076 and 2^1024 - 2^970.
    let edge = Big::from((1 << 27) - 1).shifted(27).plus((1 << 27) - 1);
    if m < 0 {
        // Below when d / 10^n < edge * 2^-1076, i.e. d * 2^(1076 - n) < edge * 5^n.
        let n = u32::try_from(-scale).unwrap_or(u32::MAX);
        let left = d.shifted(1076 - n);
        let right = (0..n).fold(edge, |b, _| b.times(5));
        left < right
    } else {
        // At or above when d * 10^scale >= edge * 2^970.
        let n = u32::try_from(scale).unwrap_or(0);
        let left = (0..n).fold(d, |b, _| b.times(10));
        left >= edge.shifted(970)
    }
}

/// A whole number of any size, in 32-bit words, least significant first, no high zero words.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Big(Vec<u32>);

impl Big {
    fn from(n: u32) -> Self {
        Self(if n == 0 { Vec::new() } else { vec![n] })
    }

    fn times(mut self, k: u32) -> Self {
        let mut carry = 0u64;
        for w in &mut self.0 {
            let (lo, hi) = split(u64::from(*w) * u64::from(k) + carry);
            *w = lo;
            carry = u64::from(hi);
        }
        if carry != 0 {
            self.0.push(split(carry).0);
        }
        self.trim()
    }

    fn plus(mut self, k: u32) -> Self {
        let mut carry = u64::from(k);
        for w in &mut self.0 {
            if carry == 0 {
                break;
            }
            let (lo, hi) = split(u64::from(*w) + carry);
            *w = lo;
            carry = u64::from(hi);
        }
        if carry != 0 {
            self.0.push(split(carry).0);
        }
        self
    }

    fn shifted(self, bits: u32) -> Self {
        let (words, rest) = ((bits / 32) as usize, bits % 32);
        let mut out = vec![0u32; words];
        let mut carry = 0u32;
        for w in self.0 {
            out.push(if rest == 0 { w } else { (w << rest) | carry });
            carry = if rest == 0 { 0 } else { w >> (32 - rest) };
        }
        if carry != 0 {
            out.push(carry);
        }
        Self(out).trim()
    }

    fn trim(mut self) -> Self {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
        self
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}

/// Whether a string (`fp[-2]`) reads whole as a floating-point number; a byte below it (`fp[-3]`).
/// The script library's single and double precision ask the same.
// D2-08765, D2-08761
fn is_double(c: &mut Call<'_>) -> Result<Status, Stop> {
    let t = text_of(c, c.cx.at(-2)?)?;
    let yes = reads_as_whole_double(&t);
    let before = c.cx.at(-3)?;
    c.write(-3, (before & !0xFF) | u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether a string (`fp[-2]`) reads whole as a number of a kind; a byte below it (`fp[-3]`).
fn is_number(c: &mut Call<'_>, bits: u32, signed: bool) -> Result<Status, Stop> {
    let t = text_of(c, c.cx.at(-2)?)?;
    let yes = reads_as_whole_number(&t, bits, signed);
    let before = c.cx.at(-3)?;
    c.write(-3, (before & !0xFF) | u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08766, D2-08762
fn is_int32(c: &mut Call<'_>) -> Result<Status, Stop> {
    is_number(c, 32, true)
}

// D2-08768, D2-02247
fn is_uint32(c: &mut Call<'_>) -> Result<Status, Stop> {
    is_number(c, 32, false)
}

// D2-08767, D2-08763, D2-14313, D2-14312
fn is_int64(c: &mut Call<'_>) -> Result<Status, Stop> {
    is_number(c, 64, true)
}

// D2-08769, D2-08764, D2-14314
fn is_uint64(c: &mut Call<'_>) -> Result<Status, Stop> {
    is_number(c, 64, false)
}

/// Narrow text as the client narrows a wide string and widens it back: each unit past the narrow
/// range a `?`.
fn narrowed(t: &[u16]) -> Vec<u16> {
    t.iter()
        .take_while(|&&u| u != 0)
        .map(|&u| if u > 0xFF { u16::from(b'?') } else { u })
        .collect()
}

/// A path without its extension: cut at its last `.`, wherever that is (unchanged without one).
#[must_use]
pub fn without_extension(t: &[u16]) -> Vec<u16> {
    let t = narrowed(t);
    match t.iter().rposition(|&u| u == u16::from(b'.')) {
        Some(i) => t[..i].to_vec(),
        None => t,
    }
}

/// A path without its folders: what follows its last `\`, `/` or `:` (unchanged without one).
#[must_use]
pub fn without_directory(t: &[u16]) -> Vec<u16> {
    let t = narrowed(t);
    match t
        .iter()
        .rposition(|&u| b"\\/:".iter().any(|&b| u == u16::from(b)))
    {
        Some(i) => t[i + 1..].to_vec(),
        None => t,
    }
}

/// Change the string an out reference (`fp[-2]`) holds; a byte 1 below it (`fp[-3]`).
fn change_path(c: &mut Call<'_>, f: fn(&[u16]) -> Vec<u16>) -> Result<Status, Stop> {
    let out = c.cx.at(-2)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = f(t),
        _ => return Err(c.fail("the out reference holds no string")),
    }
    let before = c.cx.at(-3)?;
    c.write(-3, (before & !0xFF) | 1, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08772, D2-17805, D2-17785
fn remove_extension(c: &mut Call<'_>) -> Result<Status, Stop> {
    change_path(c, without_extension)
}

// D2-08773, D2-17806, D2-17796
fn strip_directory(c: &mut Call<'_>) -> Result<Status, Stop> {
    change_path(c, without_directory)
}

/// A folder's name made one way round: each `\` a `/` (`backslash` 0), or each `/` a `\`, then that
/// separator added after a name that does not end in it (an empty name stays empty).
#[must_use]
pub fn folder_cleaned(t: &[u16], backslash: bool) -> Vec<u16> {
    let (from, to) = if backslash {
        (u16::from(b'/'), u16::from(b'\\'))
    } else {
        (u16::from(b'\\'), u16::from(b'/'))
    };
    let mut out: Vec<u16> = narrowed(t)
        .into_iter()
        .map(|u| if u == from { to } else { u })
        .collect();
    if out.last().is_some_and(|&u| u != to) {
        out.push(to);
    }
    out
}

/// The folder files are named in, into the string the reference at `fp[-2]` reaches; answers 1
/// (`fp[-3]`). Files a script names stay in the folder the host keeps them in, so the folder is
/// the empty name a bare file name is built on (the client answered its working folder, each `\`
/// a `/`).
// D2-08771
fn current_folder(c: &mut Call<'_>) -> Result<Status, Stop> {
    let out = c.cx.at(-2)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => t.clear(),
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The folder name the reference at `fp[-2]` reaches, made one way round ([`folder_cleaned`]:
/// backslashes when the word at `fp[-3]` is not 0); answers 1 (`fp[-4]`).
// D2-08770, D2-17812, D2-17811, D2-17810
fn folder_name_cleaned(c: &mut Call<'_>) -> Result<Status, Stop> {
    let out = c.cx.at(-2)?;
    let backslash = c.cx.at(-3)? != 0;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = folder_cleaned(t, backslash),
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// The clock and the log
// ---------------------------------------------------------------------------------------------

/// The game's clock, a 64-bit float: the frame's (`get_time`), or read afresh (`compute_time`), which
/// is the same clock here: a call runs at one time.
// D2-09545, D2-09543
fn time(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (lo, hi) = split(c.host.time().to_bits());
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// The wall clock: seconds since 1970, -1 when the host has no clock (the C library's answer
/// when the time cannot be had).
// D2-09544, D2-17620
fn real_time(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let t = c.host.real_time().map_or(u32::MAX, |t| t as i32 as u32);
    c.write(-2, t, tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether a log of a level is being written, for a host that keeps no log masks: none is (the
/// client keeps them and answers as it does).
fn not_logging(c: &mut Call<'_>) -> Result<Status, Stop> {
    ret(c, 1, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// The log level being written, for a host that keeps no log masks: none.
fn no_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    ret(c, 0, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// A log message: written nowhere.
// D2-09042, D2-09041
fn log_message(_: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(Status::Success)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (number helpers)
    use super::*;

    #[test]
    fn numbers_read_from_strings_follow_their_prefix() {
        assert_eq!(to_long(&units("  -42xyz")), (-42i32) as u32);
        assert_eq!(to_long(&units("0x1F")), 31);
        assert_eq!(to_long(&units("017")), 15);
        assert_eq!(to_long(&units("99999999999")), i32::MAX as u32);
        assert_eq!(to_ulong(&units("-1")), u32::MAX);
        assert_eq!(to_u64(&units("18446744073709551615")), u64::MAX);
        assert!((to_float(&units(" 2.5e1abc")) - 25.0).abs() < 1e-9);
        assert_eq!(fixed(1.5), "1.500000");
    }
}
