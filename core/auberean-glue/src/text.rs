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

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
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
    (TIMER, 2, time),
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

/// An unsigned 64-bit number.
fn to_u64(text: &[u16]) -> u64 {
    let (neg, v, overflow) = leading(text);
    if overflow {
        return u64::MAX;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// A signed 64-bit number.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn to_i64(text: &[u16]) -> u64 {
    let (neg, v, overflow) = leading(text);
    if neg {
        if overflow || v > 1 << 63 {
            i64::MIN as u64
        } else {
            (v as i64).wrapping_neg() as u64
        }
    } else if overflow || v > i64::MAX as u64 {
        i64::MAX as u64
    } else {
        v
    }
}

/// A float as the C library reads one (the longest leading part that is a number).
fn to_float(text: &[u16]) -> f64 {
    let s: String = char::decode_utf16(text.iter().copied())
        .map(|r| r.unwrap_or('\u{fffd}'))
        .collect();
    let t = s.trim_start();
    let mut best = 0.0;
    for (end, _) in t.char_indices().chain(core::iter::once((t.len(), ' '))) {
        if let Ok(v) = t[..end].parse::<f64>() {
            best = v;
        }
    }
    best
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

fn give_text(c: &mut Call<'_>, slot: i64, units: Vec<u16>) -> Result<Status, Stop> {
    let id = new_text(c, units);
    c.write(slot, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Two strings joined.
// D2-09591
fn concat_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let mut a = text_arg(c, -2)?;
    a.extend(text_arg(c, -3)?);
    give_text(c, -5, a)
}

/// A string and a signed integer joined.
// D2-09588
fn concat_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    #[allow(clippy::cast_possible_wrap)]
    let v = c.cx.at(-3)? as i32;
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &v.to_string(), order))
}

/// A string and an unsigned integer joined.
// D2-09592
fn concat_uint(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = c.cx.at(-3)?;
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &v.to_string(), order))
}

/// A string and a float joined.
// D2-09587
fn concat_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = f64::from(f32::from_bits(c.cx.at(-3)?));
    let order = c.cx.at(-4)?;
    give_text(c, -5, joined(s, &fixed(v), order))
}

/// A string and a signed 64-bit integer joined.
// D2-09590
fn concat_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    #[allow(clippy::cast_possible_wrap)]
    let v = join(c.cx.at(-4)?, c.cx.at(-3)?) as i64;
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &v.to_string(), order))
}

/// A string and an unsigned 64-bit integer joined.
// D2-09593
fn concat_ulong(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = join(c.cx.at(-4)?, c.cx.at(-3)?);
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &v.to_string(), order))
}

/// A string and a 64-bit float joined.
// D2-09589
fn concat_double(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = text_arg(c, -2)?;
    let v = f64::from_bits(join(c.cx.at(-4)?, c.cx.at(-3)?));
    let order = c.cx.at(-5)?;
    give_text(c, -6, joined(s, &fixed(v), order))
}

/// A number as `0x` and eight upper-case hex digits.
// D2-09584
fn to_hex(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    give_text(c, -3, units(&format!("0x{v:08X}")))
}

/// A 64-bit instance id as four groups of four hex digits, the high word first.
// D2-09585
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
// D2-09610
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

fn same_text(c: &mut Call<'_>) -> Result<Option<bool>, Stop> {
    let other = text_arg(c, -2)?;
    let ignore_case = c.cx.at(-3)? & 0xFF != 0;
    let Some(this) = this_text(c)? else {
        return Ok(None);
    };
    Ok(Some(if ignore_case {
        this.len() == other.len() && this.iter().zip(&other).all(|(a, b)| fold(*a) == fold(*b))
    } else {
        *this == other
    }))
}

/// Whether two strings are equal, ignoring case when asked.
// D2-09242
fn equal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(eq) = same_text(c)? else {
        return Ok(Status::Crash);
    };
    ret(c, 2, u32::from(eq), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether two strings differ, ignoring case when asked.
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

/// Upper-case the string's ASCII letters, in place.
// D2-09602
fn upper(c: &mut Call<'_>) -> Result<Status, Stop> {
    for u in text_or_crash!(c).iter_mut() {
        if (u16::from(b'a')..=u16::from(b'z')).contains(u) {
            *u -= 32;
        }
    }
    Ok(Status::Success)
}

/// Lower-case the string's ASCII letters, in place.
// D2-09601
fn lower(c: &mut Call<'_>) -> Result<Status, Stop> {
    for u in text_or_crash!(c).iter_mut() {
        *u = fold(*u);
    }
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
const TABLE_ENUM_MAPPING: u32 = 0x4000_0004;

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
pub(crate) fn table_or_none(did: u32) -> u32 {
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
// The clock and the log
// ---------------------------------------------------------------------------------------------

/// The game's clock, a 64-bit float.
// D2-09545
fn time(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (lo, hi) = split(c.host.time().to_bits());
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// Whether a log of a level is being written: none is.
// D2-09045, D2-09046
fn not_logging(c: &mut Call<'_>) -> Result<Status, Stop> {
    ret(c, 1, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// The log level being written: none.
// D2-09043, D2-09044
fn no_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    ret(c, 0, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// A log message: written nowhere.
// D2-09042
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
