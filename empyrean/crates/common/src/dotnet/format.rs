//! .NET numeric `ToString(format)` for the `en-US` culture, as ACE's player-visible and log text
//! uses it (ACE forces `en-US` in `Program.Main`).
//!
//! Source: dotnet/runtime `src/libraries/System.Private.CoreLib/src/System/Number.Formatting.cs`
//! (`FormatDouble`, `NumberToString`, `NumberToStringFormat`, `RoundNumber`, `FormatFixed`,
//! `FormatGeneral`) and `Number.Dragon4.cs`. Every rule below was checked against the .NET 8.0.22
//! runtime by `tests/fixtures/dotnet_oracle.cs`, whose output (`tests/fixtures/dotnet_format.tsv`)
//! the tests replay line by line.
//!
//! # Supported formats
//! The set ACE uses (`grep` of `ToString("...")` and `{x:...}` in ACE.Server/ACE.Common):
//! * standard: `""` (default), `R`, `G`/`Gn`, `N`/`Nn`, `F`/`Fn`, `P`/`Pn`, `D`/`Dn`, `X`/`Xn`
//!   (integers only for `D` and `X`), any precision;
//! * custom: patterns built from `0`, `#`, `.`, `,` (grouping and scaling), `%`, quoted literals,
//!   `\` escapes and other literal characters, one section. This covers `0`, `000`, `00000`,
//!   `0.0`, `0.00`, `#.00`, `#,###0`, `####` and friends.
//!
//! An unsupported format (`E`, `C`, multi-section `;` patterns, exponent patterns) panics with the
//! format string, because ACE passes only literal format strings and a test will catch it.
//!
//! # The rules that matter
//! * `double`/`float` standard formats (`F`, `N`, `P`, `G` with a precision) round the **exact**
//!   binary value, half to even (Dragon4 with a cut-off): `0.125.ToString("F2") == "0.12"`,
//!   `2.675.ToString("F2") == "2.67"`.
//! * The default format and `R` print the shortest round-trippable digits, switching to `E+XX`
//!   notation when the decimal exponent exceeds `max(digits, 17)` (`float`: 9) or is below -4.
//! * Custom formats first round to 15 significant digits (`float`: 7), half to even, and then
//!   round the digit string half **up**: `0.125.ToString("0.00") == "0.13"`.
//! * Negative zero keeps its sign (`"-0"`), except that a custom format that produces no output
//!   at all (`"####"` of zero) produces `""`.
//! * NaN is `NaN`, the infinities are `∞` and `-∞`.

use std::fmt::Write as _;

/// A number to format, carrying the C# type it came from (the width matters to `X`, the digit
/// generation to floating-point formats).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    /// An integer of `bits` width (8, 16, 32 or 64), signed or unsigned.
    Int {
        /// The value.
        value: i128,
        /// The C# type's width in bits.
        bits: u32,
        /// Whether the C# type is signed.
        signed: bool,
    },
    /// A `double`.
    F64(f64),
    /// A `float`.
    F32(f32),
}

macro_rules! num_from_int {
    ($($t:ty => $bits:expr, $signed:expr);* $(;)?) => {$(
        impl From<$t> for Num {
            fn from(v: $t) -> Self {
                Num::Int { value: i128::from(v), bits: $bits, signed: $signed }
            }
        }
    )*};
}

num_from_int! {
    i8 => 8, true; u8 => 8, false; i16 => 16, true; u16 => 16, false;
    i32 => 32, true; u32 => 32, false; i64 => 64, true; u64 => 64, false;
}

/// A Rust `usize` formats as the C# `int` it stands for (`Count`, `Length`).
impl From<usize> for Num {
    fn from(v: usize) -> Self {
        Num::Int {
            value: i128::try_from(v).unwrap_or(i128::MAX),
            bits: 32,
            signed: true,
        }
    }
}

impl From<f64> for Num {
    fn from(v: f64) -> Self {
        Num::F64(v)
    }
}

impl From<f32> for Num {
    fn from(v: f32) -> Self {
        Num::F32(v)
    }
}

/// `value.ToString()` in `en-US`.
pub fn to_string(value: impl Into<Num>) -> String {
    format(value, "")
}

/// `value.ToString(format)` in `en-US`. See the module documentation for the supported set.
///
/// # Panics
/// On a format outside the supported set, or `D`/`X` applied to a floating-point value (where .NET
/// throws `FormatException`).
pub fn format(value: impl Into<Num>, format: &str) -> String {
    let value = value.into();
    let (kind, precision) = parse_standard(format);
    match value {
        Num::Int {
            value,
            bits,
            signed,
        } => format_int(value, bits, signed, kind, precision, format),
        Num::F64(v) => format_float(v, false, kind, precision, format),
        Num::F32(v) => format_float(f64::from(v), true, kind, precision, format),
    }
}

/// Composite-format alignment `{0,width}`: pads on the left for a positive width and on the right
/// for a negative one; never truncates.
#[must_use]
pub fn align(text: &str, width: i32) -> String {
    let len = text.chars().count();
    let w = usize::try_from(width.unsigned_abs()).unwrap_or(usize::MAX);
    if len >= w {
        return text.to_owned();
    }
    let pad = " ".repeat(w - len);
    if width > 0 {
        pad + text
    } else {
        text.to_owned() + &pad
    }
}

/// `{value,width:format}`.
pub fn format_aligned(value: impl Into<Num>, width: i32, fmt: &str) -> String {
    align(&format(value, fmt), width)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Default,
    Standard(char),
    Custom,
}

/// `ParseFormatSpecifier`: a letter followed by at most nine digits is a standard format.
fn parse_standard(format: &str) -> (Kind, Option<i32>) {
    if format.is_empty() {
        return (Kind::Default, None);
    }
    let bytes = format.as_bytes();
    if bytes[0].is_ascii_alphabetic()
        && bytes.len() <= 10
        && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        let precision = if bytes.len() > 1 {
            format[1..].parse::<i32>().ok()
        } else {
            None
        };
        // Hex is the one standard format whose letter case reaches the output (`x` vs `X`).
        let letter = if bytes[0] == b'x' {
            'x'
        } else {
            char::from(bytes[0].to_ascii_uppercase())
        };
        return (Kind::Standard(letter), precision);
    }
    (Kind::Custom, None)
}

/// The `NumberBuffer`: `0.d1d2d3... * 10^scale`, ASCII digits, no trailing zeros.
#[derive(Debug, Clone)]
struct Digits {
    digits: Vec<u8>,
    scale: i32,
    negative: bool,
}

impl Digits {
    fn trim(&mut self) {
        while self.digits.last() == Some(&b'0') {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.scale = 0;
        }
    }

    fn len(&self) -> i32 {
        i32::try_from(self.digits.len()).unwrap_or(i32::MAX)
    }

    /// Keeps `pos` digits. `ties_even` rounds an exact buffer half to even (Dragon4's cut-off);
    /// otherwise rounds half up on the digit string (`RoundNumber` for custom formats and
    /// integers).
    fn round(&mut self, pos: i32, ties_even: bool) {
        if pos >= self.len() {
            return;
        }
        let Ok(p) = usize::try_from(pos) else {
            // Every digit is below the cut-off by more than half a unit.
            self.digits.clear();
            self.scale = 0;
            return;
        };
        let d = self.digits[p];
        let up = if ties_even {
            d > b'5'
                || (d == b'5'
                    && (self.digits.len() > p + 1
                        || (p > 0 && (self.digits[p - 1] - b'0') % 2 == 1)))
        } else {
            d >= b'5'
        };
        self.digits.truncate(p);
        if up {
            loop {
                match self.digits.last_mut() {
                    None => {
                        self.digits.push(b'1');
                        self.scale += 1;
                        break;
                    }
                    Some(last) if *last == b'9' => {
                        self.digits.pop();
                    }
                    Some(last) => {
                        *last += 1;
                        break;
                    }
                }
            }
        }
        self.trim();
    }
}

/// Parses Rust's `{:e}` output ("d.ddde-7") into a digit buffer.
fn from_exp_string(s: &str, negative: bool) -> Digits {
    let (mantissa, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut d = Digits {
        digits,
        scale: exp + 1,
        negative,
    };
    d.trim();
    d
}

/// The exact decimal expansion of a finite double (at most 767 significant digits).
fn exact_digits(v: f64) -> Digits {
    if v == 0.0 {
        return Digits {
            digits: Vec::new(),
            scale: 0,
            negative: v.is_sign_negative(),
        };
    }
    from_exp_string(&format!("{:.800e}", v.abs()), v.is_sign_negative())
}

/// The shortest round-trippable digits, of a double or of a float.
fn shortest_digits(v: f64, single: bool) -> Digits {
    if v == 0.0 {
        return Digits {
            digits: Vec::new(),
            scale: 0,
            negative: v.is_sign_negative(),
        };
    }
    #[allow(clippy::cast_possible_truncation)]
    let s = if single {
        format!("{:e}", (v as f32).abs())
    } else {
        format!("{:e}", v.abs())
    };
    from_exp_string(&s, v.is_sign_negative())
}

fn int_digits(value: i128) -> Digits {
    let mut d = Digits {
        digits: value.unsigned_abs().to_string().into_bytes(),
        scale: 0,
        negative: value < 0,
    };
    d.scale = d.len();
    if value == 0 {
        d.digits.clear();
        d.scale = 0;
    }
    d.trim_keep_scale();
    d
}

impl Digits {
    /// Trims trailing zeros without touching the scale (integer digits).
    fn trim_keep_scale(&mut self) {
        while self.digits.last() == Some(&b'0') {
            self.digits.pop();
        }
    }
}

const GROUP: char = ',';

fn format_float(v: f64, single: bool, kind: Kind, precision: Option<i32>, fmt: &str) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "∞".to_owned()
        } else {
            "-∞".to_owned()
        };
    }
    let round_trip = if single { 9 } else { 17 };
    match kind {
        Kind::Default | Kind::Standard('R') => {
            let d = shortest_digits(v, single);
            let max = d.len().max(round_trip);
            general(&d, max, 'E')
        }
        Kind::Standard('G') => match precision {
            None | Some(0) => {
                let d = shortest_digits(v, single);
                let max = d.len().max(round_trip);
                general(&d, max, 'E')
            }
            Some(p) => {
                let mut d = exact_digits(v);
                d.round(p, true);
                general(&d, p, 'E')
            }
        },
        Kind::Standard(c @ ('F' | 'N' | 'P')) => {
            let p = precision.unwrap_or(2);
            let mut d = exact_digits(v);
            if c == 'P' && !d.digits.is_empty() {
                d.scale += 2;
            }
            d.round(d.scale + p, true);
            fixed_with_affixes(&d, p, c)
        }
        Kind::Standard(c) => {
            panic!("FormatException: format {fmt:?} ({c}) is not supported for floating point")
        }
        Kind::Custom => {
            let mut d = exact_digits(v);
            d.round(if single { 7 } else { 15 }, true);
            custom(d, fmt, true)
        }
    }
}

fn format_int(
    value: i128,
    bits: u32,
    signed: bool,
    kind: Kind,
    precision: Option<i32>,
    fmt: &str,
) -> String {
    match kind {
        Kind::Default => value.to_string(),
        Kind::Standard('D') => {
            let digits = value.unsigned_abs().to_string();
            let width = usize::try_from(precision.unwrap_or(0)).unwrap_or(0);
            let sign = if value < 0 { "-" } else { "" };
            format!("{sign}{digits:0>width$}")
        }
        Kind::Standard(hex @ ('X' | 'x')) => {
            let mask: u128 = if bits >= 128 {
                u128::MAX
            } else {
                (1u128 << bits) - 1
            };
            // Two's complement in the C# type's width. `signed` only matters for the sign
            // extension, which the mask already accounts for.
            let _ = signed;
            #[allow(clippy::cast_sign_loss)]
            let raw = (value as u128) & mask;
            let width = usize::try_from(precision.unwrap_or(0)).unwrap_or(0);
            if hex == 'x' {
                format!("{raw:0>width$x}")
            } else {
                format!("{raw:0>width$X}")
            }
        }
        Kind::Standard('G') => match precision {
            None | Some(0) => value.to_string(),
            Some(p) => {
                let mut d = int_digits(value);
                d.round(p, false);
                if d.digits.is_empty() {
                    d.negative = false;
                }
                general(&d, p, 'E')
            }
        },
        Kind::Standard('R') => value.to_string(),
        Kind::Standard(c @ ('F' | 'N' | 'P')) => {
            let p = precision.unwrap_or(2);
            let mut d = int_digits(value);
            if c == 'P' && !d.digits.is_empty() {
                d.scale += 2;
            }
            fixed_with_affixes(&d, p, c)
        }
        Kind::Standard(c) => panic!("FormatException: format {fmt:?} ({c}) is not supported"),
        Kind::Custom => custom(int_digits(value), fmt, false),
    }
}

/// `F`, `N` or `P` output with the `en-US` sign and percent patterns (`-n`, `n%`, `-n%`).
fn fixed_with_affixes(d: &Digits, precision: i32, kind: char) -> String {
    let mut s = String::new();
    if d.negative {
        s.push('-');
    }
    fixed(&mut s, d, precision, kind != 'F');
    if kind == 'P' {
        s.push('%');
    }
    s
}

/// `FormatFixed`.
fn fixed(out: &mut String, d: &Digits, precision: i32, grouping: bool) {
    let mut di = 0usize;
    if d.scale > 0 {
        let int_len = usize::try_from(d.scale).unwrap_or(0);
        for i in 0..int_len {
            let c = d.digits.get(di).copied().map_or('0', char::from);
            di += 1;
            out.push(c);
            let remaining = int_len - i - 1;
            if grouping && remaining > 0 && remaining % 3 == 0 {
                out.push(GROUP);
            }
        }
    } else {
        out.push('0');
    }
    if precision > 0 {
        out.push('.');
        let mut pos = d.scale.min(0);
        for _ in 0..precision {
            if pos < 0 {
                out.push('0');
                pos += 1;
            } else {
                out.push(d.digits.get(di).copied().map_or('0', char::from));
                di += 1;
            }
        }
    }
}

/// `FormatGeneral` plus the sign: scientific when the exponent exceeds `max_digits` or is below
/// -4.
fn general(d: &Digits, max_digits: i32, exp_char: char) -> String {
    let mut out = String::new();
    if d.negative {
        out.push('-');
    }
    let mut dig_pos = d.scale;
    let mut scientific = false;
    if dig_pos > max_digits || dig_pos < -3 {
        dig_pos = 1;
        scientific = true;
    }
    let mut di = 0usize;
    if dig_pos > 0 {
        while dig_pos > 0 {
            out.push(d.digits.get(di).copied().map_or('0', char::from));
            if di < d.digits.len() {
                di += 1;
            }
            dig_pos -= 1;
        }
    } else {
        out.push('0');
    }
    if di < d.digits.len() || dig_pos < 0 {
        out.push('.');
        while dig_pos < 0 {
            out.push('0');
            dig_pos += 1;
        }
        while di < d.digits.len() {
            out.push(char::from(d.digits[di]));
            di += 1;
        }
    }
    if scientific {
        let e = if d.digits.is_empty() { 0 } else { d.scale - 1 };
        let _ = write!(
            out,
            "{exp_char}{}{:02}",
            if e < 0 { '-' } else { '+' },
            e.unsigned_abs()
        );
    }
    out
}

/// `NumberToStringFormat` for a single section.
fn custom(mut d: Digits, fmt: &str, floating: bool) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    assert!(
        !chars.contains(&';'),
        "FormatException: multi-section custom format {fmt:?} is not supported"
    );

    // Pass 1: shape of the format.
    let mut digit_count = 0i32;
    let mut decimal_pos = -1i32;
    let mut first_digit = i32::MAX;
    let mut last_digit = 0i32;
    let mut scale_adjust = 0i32;
    let mut thousand_pos = -1i32;
    let mut thousand_count = 0i32;
    let mut thousand_seps = false;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        i += 1;
        match ch {
            '#' => digit_count += 1,
            '0' => {
                if first_digit == i32::MAX {
                    first_digit = digit_count;
                }
                digit_count += 1;
                last_digit = digit_count;
            }
            '.' => {
                if decimal_pos < 0 {
                    decimal_pos = digit_count;
                }
            }
            ',' => {
                if digit_count > 0 && decimal_pos < 0 {
                    if thousand_pos >= 0 {
                        if thousand_pos == digit_count {
                            thousand_count += 1;
                            continue;
                        }
                        thousand_seps = true;
                    }
                    thousand_pos = digit_count;
                    thousand_count = 1;
                }
            }
            '%' => scale_adjust += 2,
            '\u{2030}' => scale_adjust += 3,
            '\'' | '"' => {
                while i < chars.len() && chars[i] != ch {
                    i += 1;
                }
                i += 1;
            }
            '\\' => i += 1,
            'E' | 'e' => panic!("FormatException: exponent custom format {fmt:?} is not supported"),
            _ => {}
        }
    }
    if decimal_pos < 0 {
        decimal_pos = digit_count;
    }
    if thousand_pos >= 0 {
        if thousand_pos == decimal_pos {
            scale_adjust -= 3 * thousand_count;
        } else {
            thousand_seps = true;
        }
    }

    if d.digits.is_empty() {
        if !floating {
            d.negative = false;
        }
        d.scale = 0;
    } else {
        d.scale += scale_adjust;
        let pos = d.scale + digit_count - decimal_pos;
        d.round(pos, false);
        if d.digits.is_empty() && !floating {
            d.negative = false;
        }
    }

    let first_digit = if first_digit < decimal_pos {
        decimal_pos - first_digit
    } else {
        0
    };
    let last_digit = if last_digit > decimal_pos {
        decimal_pos - last_digit
    } else {
        0
    };
    let mut dig_pos = d.scale.max(decimal_pos);
    let mut adjust = d.scale - decimal_pos;

    // Positions (counted in integer digits from the decimal point) after which a group separator
    // is written, as `thousandsSepPos` precomputes them; consumed from the back.
    let mut sep_positions: Vec<i32> = Vec::new();
    if thousand_seps {
        let total_digits = dig_pos + adjust.min(0);
        let num_digits = first_digit.max(total_digits);
        let mut group_total = 3;
        while num_digits > group_total {
            sep_positions.push(group_total);
            group_total += 3;
        }
    }

    let mut out = String::new();
    if d.negative && d.scale != 0 {
        out.push('-');
    }
    let mut di = 0usize;
    let mut decimal_written = false;
    let next_digit = |di: &mut usize| -> Option<char> {
        let c = d.digits.get(*di).copied().map(char::from);
        if c.is_some() {
            *di += 1;
        }
        c
    };
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        i += 1;
        if adjust > 0 && matches!(ch, '#' | '0' | '.') {
            while adjust > 0 {
                out.push(next_digit(&mut di).unwrap_or('0'));
                if thousand_seps
                    && dig_pos > 1
                    && sep_positions.last().is_some_and(|&p| dig_pos == p + 1)
                {
                    out.push(GROUP);
                    sep_positions.pop();
                }
                dig_pos -= 1;
                adjust -= 1;
            }
        }
        match ch {
            '#' | '0' => {
                let c = if adjust < 0 {
                    adjust += 1;
                    if dig_pos <= first_digit {
                        Some('0')
                    } else {
                        None
                    }
                } else {
                    match next_digit(&mut di) {
                        Some(c) => Some(c),
                        None if dig_pos > last_digit => Some('0'),
                        None => None,
                    }
                };
                if let Some(c) = c {
                    out.push(c);
                    if thousand_seps
                        && dig_pos > 1
                        && sep_positions.last().is_some_and(|&p| dig_pos == p + 1)
                    {
                        out.push(GROUP);
                        sep_positions.pop();
                    }
                }
                dig_pos -= 1;
            }
            '.' => {
                if dig_pos != 0 || decimal_written {
                    continue;
                }
                if last_digit < 0 || (decimal_pos < digit_count && di < d.digits.len()) {
                    out.push('.');
                    decimal_written = true;
                }
            }
            '%' => out.push('%'),
            '\u{2030}' => out.push('\u{2030}'),
            ',' => {}
            '\'' | '"' => {
                while i < chars.len() && chars[i] != ch {
                    out.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '\\' => {
                if i < chars.len() {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            other => out.push(other),
        }
    }
    if d.negative && d.scale == 0 && !out.is_empty() {
        out.insert(0, '-');
    }
    out
}
