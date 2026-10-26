//! The part of the Luau VM's C runtime written in Rust, for `wasm32-unknown-unknown` only: the
//! allocator (on Rust's own), `strtod` (on Rust's correctly rounded parser) and the floating-point
//! digits of `vsnprintf`. The rest of that runtime is C and C++ built by this crate's build script.
//!
//! Every function here is called by the C and C++ code with the C library's contract; each checks
//! what that contract lets it check and trusts the rest, as a C library does.

use std::alloc::{alloc, dealloc, realloc as ralloc, Layout};
use std::ffi::{c_char, c_int, c_void};
use std::sync::Once;

extern "C" {
    /// Runs the C++ static constructors (the runtime's own, in `rt/libc.c`).
    fn __luaurt_init();
}

/// Run the C++ static constructors, once, before the first VM.
pub fn init() {
    static ONCE: Once = Once::new();
    // SAFETY: the constructors take no arguments and run once, before any C++ code is entered.
    ONCE.call_once(|| unsafe { __luaurt_init() });
}

/// Every block is aligned for any C type.
const ALIGN: usize = 16;
/// The size is kept in front of each block, in a header as large as the alignment.
const HEADER: usize = 16;

fn layout(size: usize) -> Option<Layout> {
    Layout::from_size_align(size.checked_add(HEADER)?, ALIGN).ok()
}

/// `malloc`.
///
/// # Safety
/// The C contract: the block is released with [`free`] or [`realloc`] only.
#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let Some(l) = layout(size) else {
        return std::ptr::null_mut();
    };
    // SAFETY: the layout has a non-zero size (the header).
    let p = unsafe { alloc(l) };
    if p.is_null() {
        return p.cast();
    }
    // SAFETY: the block is at least HEADER bytes and aligned for a usize.
    unsafe {
        p.cast::<usize>().write(size);
        p.add(HEADER).cast()
    }
}

/// `calloc`.
///
/// # Safety
/// As [`malloc`].
#[no_mangle]
pub unsafe extern "C" fn calloc(n: usize, size: usize) -> *mut c_void {
    let Some(total) = n.checked_mul(size) else {
        return std::ptr::null_mut();
    };
    // SAFETY: the C contract, passed on.
    let p = unsafe { malloc(total) };
    if !p.is_null() {
        // SAFETY: the block just allocated holds `total` bytes.
        unsafe { std::ptr::write_bytes(p.cast::<u8>(), 0, total) };
    }
    p
}

/// `free`.
///
/// # Safety
/// `p` is null or a block from [`malloc`], [`calloc`] or [`realloc`] not yet released.
#[no_mangle]
pub unsafe extern "C" fn free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    // SAFETY: `p` came from `malloc`, so its header, holding the size, is just in front of it.
    unsafe {
        let base = p.cast::<u8>().sub(HEADER);
        let size = base.cast::<usize>().read();
        if let Some(l) = layout(size) {
            dealloc(base, l);
        }
    }
}

/// `realloc`.
///
/// # Safety
/// As [`free`] for `p`.
#[no_mangle]
pub unsafe extern "C" fn realloc(p: *mut c_void, size: usize) -> *mut c_void {
    if p.is_null() {
        // SAFETY: the C contract, passed on.
        return unsafe { malloc(size) };
    }
    let Some(new_total) = size.checked_add(HEADER) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `p` came from `malloc`; its header holds the size it was allocated with.
    unsafe {
        let base = p.cast::<u8>().sub(HEADER);
        let old = base.cast::<usize>().read();
        let Some(l) = layout(old) else {
            return std::ptr::null_mut();
        };
        let n = ralloc(base, l, new_total);
        if n.is_null() {
            return n.cast();
        }
        n.cast::<usize>().write(size);
        n.add(HEADER).cast()
    }
}

/// `aligned_alloc`, for alignments up to the allocator's own.
///
/// # Safety
/// As [`malloc`].
#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut c_void {
    if align > ALIGN {
        return std::ptr::null_mut();
    }
    // SAFETY: the C contract, passed on.
    unsafe { malloc(size) }
}

/// `posix_memalign`, for alignments up to the allocator's own.
///
/// # Safety
/// `out` is valid for a write.
#[no_mangle]
pub unsafe extern "C" fn posix_memalign(out: *mut *mut c_void, align: usize, size: usize) -> c_int {
    const ENOMEM: c_int = 12;
    if align > ALIGN {
        return ENOMEM;
    }
    // SAFETY: the C contract, passed on; `out` is valid for a write.
    unsafe {
        let p = malloc(size);
        out.write(p);
        if p.is_null() {
            ENOMEM
        } else {
            0
        }
    }
}

/// `strtod`: optional white space, a sign, then a decimal or hexadecimal number, `inf`,
/// `infinity` or `nan`; `end` gets the first character not used.
///
/// # Safety
/// `s` is a NUL-terminated string; `end` is null or valid for a write.
#[no_mangle]
pub unsafe extern "C" fn strtod(s: *const c_char, end: *mut *mut c_char) -> f64 {
    // SAFETY: `s` is NUL-terminated.
    let bytes = unsafe { std::ffi::CStr::from_ptr(s) }.to_bytes();
    let (v, used) = parse_f64(bytes);
    if !end.is_null() {
        // SAFETY: `used` is within the string; `end` is valid for a write.
        unsafe { end.write(s.add(used).cast_mut()) };
    }
    v
}

/// A number at the start of `b` as `strtod` reads it, and the bytes it used (0 for none).
fn parse_f64(b: &[u8]) -> (f64, usize) {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t'..=b'\r') {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let lower = |j: usize| b.get(j).map(u8::to_ascii_lowercase);
    let word = |j: usize, w: &[u8]| w.iter().enumerate().all(|(k, c)| lower(j + k) == Some(*c));
    let sign = |v: f64| if neg { -v } else { v };
    if word(i, b"infinity") {
        return (sign(f64::INFINITY), i + 8);
    }
    if word(i, b"inf") {
        return (sign(f64::INFINITY), i + 3);
    }
    if word(i, b"nan") {
        return (f64::NAN, i + 3);
    }
    if b.get(i) == Some(&b'0') && lower(i + 1) == Some(b'x') {
        return parse_hex(b, i, neg);
    }
    let mut j = i;
    let digits_start = j;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    let mut ndig = j - digits_start;
    if j < b.len() && b[j] == b'.' {
        let k = j + 1;
        let mut m = k;
        while m < b.len() && b[m].is_ascii_digit() {
            m += 1;
        }
        ndig += m - k;
        if ndig > 0 {
            j = m;
        }
    }
    if ndig == 0 {
        return (0.0, 0);
    }
    if j < b.len() && (b[j] | 32) == b'e' {
        let mut k = j + 1;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            k += 1;
        }
        let ds = k;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
        }
        if k > ds {
            j = k;
        }
    }
    let text = std::str::from_utf8(&b[i..j]).unwrap_or("0");
    (sign(text.parse::<f64>().unwrap_or(0.0)), j)
}

/// A hexadecimal number (`0x…`, with an optional binary exponent `p…`) starting at `i`.
fn parse_hex(b: &[u8], i: usize, neg: bool) -> (f64, usize) {
    let lower = |j: usize| b.get(j).map(u8::to_ascii_lowercase);
    let mut j = i + 2;
    let mut mant: u64 = 0;
    let mut exp: i64 = 0;
    let mut any = false;
    let mut seen_dot = false;
    while j < b.len() {
        let c = b[j];
        if c == b'.' && !seen_dot {
            seen_dot = true;
        } else if let Some(d) = char::from(c).to_digit(16) {
            any = true;
            if mant >> 60 == 0 {
                mant = mant * 16 + u64::from(d);
                if seen_dot {
                    exp -= 4;
                }
            } else if !seen_dot {
                exp += 4;
            }
        } else {
            break;
        }
        j += 1;
    }
    let sign = |v: f64| if neg { -v } else { v };
    if !any {
        return (sign(0.0), i + 1);
    }
    if lower(j) == Some(b'p') {
        let mut k = j + 1;
        let mut eneg = false;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            eneg = b[k] == b'-';
            k += 1;
        }
        let ds = k;
        let mut e: i64 = 0;
        while k < b.len() && b[k].is_ascii_digit() {
            e = (e * 10 + i64::from(b[k] - b'0')).min(100_000);
            k += 1;
        }
        if k > ds {
            exp += if eneg { -e } else { e };
            j = k;
        }
    }
    let exp = i32::try_from(exp.clamp(-2000, 2000)).unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a mantissa of at most 64 bits, rounded to a double as strtod rounds it.
    let m = mant as f64;
    (sign(m * 2f64.powi(exp)), j)
}

/// The digits of `v` (not negative) for a `printf` floating-point conversion (`f`, `e`, `g`, `a`
/// and their capitals), at most `cap` bytes into `buf`; answers the bytes written.
///
/// # Safety
/// `buf` is valid for `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn __luaurt_fmt_f64(
    buf: *mut c_char,
    cap: usize,
    v: f64,
    conv: c_int,
    prec: c_int,
    alt: c_int,
) -> usize {
    let s = format_f64(
        v,
        u8::try_from(conv).unwrap_or(b'g'),
        usize::try_from(prec).unwrap_or(0),
        alt != 0,
    );
    let n = s.len().min(cap);
    // SAFETY: `buf` is valid for `cap` bytes and `n` is at most `cap`.
    unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), buf.cast::<u8>(), n) };
    n
}

fn format_f64(v: f64, conv: u8, prec: usize, alt: bool) -> String {
    let upper = conv.is_ascii_uppercase();
    let s = if v.is_nan() {
        "nan".to_owned()
    } else if v.is_infinite() {
        "inf".to_owned()
    } else {
        match conv.to_ascii_lowercase() {
            b'f' => {
                let mut s = format!("{v:.prec$}");
                if alt && prec == 0 {
                    s.push('.');
                }
                s
            }
            b'e' | b'a' => fmt_e(v, prec, alt),
            _ => fmt_g(v, prec, alt),
        }
    };
    if upper {
        s.to_ascii_uppercase()
    } else {
        s
    }
}

/// `%g`: `%e` or `%f`, whichever C picks for the exponent, trailing zeros dropped unless `#`.
fn fmt_g(v: f64, prec: usize, alt: bool) -> String {
    let p = prec.max(1);
    let e = fmt_e(v, p - 1, false);
    let x: i64 = e
        .find('e')
        .and_then(|k| e[k + 1..].parse().ok())
        .unwrap_or(0);
    let p_i = i64::try_from(p).unwrap_or(i64::MAX);
    let mut s = if p_i > x && x >= -4 {
        let digits = usize::try_from(p_i - 1 - x).unwrap_or(0);
        format!("{v:.digits$}")
    } else {
        e
    };
    if !alt {
        let (m, tail) = match s.find('e') {
            Some(k) => (s[..k].to_owned(), s[k..].to_owned()),
            None => (s.clone(), String::new()),
        };
        let m = if m.contains('.') {
            m.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            m
        };
        s = m + &tail;
    }
    s
}

/// `%e`: one digit, the fraction, and an exponent of at least two digits.
fn fmt_e(v: f64, prec: usize, alt: bool) -> String {
    let s = format!("{v:.prec$e}");
    let k = s.find('e').unwrap_or(s.len());
    let (m, e) = (&s[..k], s.get(k + 1..).unwrap_or("0"));
    let (esign, edig) = e.strip_prefix('-').map_or(('+', e), |d| ('-', d));
    let mut m = m.to_owned();
    if alt && prec == 0 {
        m.push('.');
    }
    format!("{m}e{esign}{edig:0>2}")
}
