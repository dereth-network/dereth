//! The C interface: what the Decal plugin calls.
//!
//! Every function takes the host [`dereth_host_new`] made; text crosses as UTF-8 bytes with a
//! length, never as a C string. Each runs on the caller's thread, the game's, and never panics
//! back across the boundary: a fault inside stops the host, which then does nothing more and says
//! so once, as a chat line among its ops.
//!
//! | function | what |
//! |---|---|
//! | `dereth_host_abi` | the interface's version, [`ABI`] |
//! | `dereth_host_new(settings, len)` | a host from its settings' JSON ([`crate::Settings::from_json`]), or null |
//! | `dereth_host_free(host)` | the host gone, its stores written out |
//! | `dereth_host_server_message(host, data, len, now)` | a message the client received, whole |
//! | `dereth_host_client_message(host, data, len, now)` | a message the client sent, whole |
//! | `dereth_host_place(host, landcell, x, y, z, heading)` | the player's place, as Decal reads it |
//! | `dereth_host_selected(host, id)` | the object the player selected |
//! | `dereth_host_player_moved(host)` | the player pressed a key in the game |
//! | `dereth_host_game_state(host, focused, chat_open)` | whether the game's window is in front and its chat line open (each 0 or 1), before each frame |
//! | `dereth_host_bindings(host, json, len)` | the player's key bindings, as Decal reads them from the game's key map ([`crate::RetailHost::set_bindings`]) |
//! | `dereth_host_typed(host, line, len)` | a line the player typed: 1 when a plugin claims it (the client must not send it) |
//! | `dereth_host_view_event(host, json, len)` | an event on a plugin window's view: `{ key, control, event, value }` |
//! | `dereth_host_flush(host)` | the messages handed over so far, applied as one batch (a host that applies them only when asked) |
//! | `dereth_host_frame(host, now)` | one frame |
//! | `dereth_host_ops(host, len)` | what to do since the last call: a JSON array of ops, valid until the next call |
//! | `dereth_host_report(host, len)` | the host's state as JSON ([`crate::RetailHost::report`]), valid until the next call |
//!
//! `now` is seconds on the caller's own steady clock.

#![allow(unsafe_code)]

use std::panic::{catch_unwind, AssertUnwindSafe};

use dereth_client_sdk::plugin_api::Value;

use crate::{RetailHost, Settings};

/// The interface's version: what a caller checks before it uses the rest.
pub const ABI: u32 = 2;

/// A host behind the C interface.
#[derive(Debug)]
pub struct DerethHost {
    host: RetailHost,
    /// The text last handed out, kept until the next call.
    out: Vec<u8>,
    /// A fault stopped the host.
    broken: bool,
    /// The fault has been told.
    told: bool,
}

impl DerethHost {
    /// Run `f` on the host unless a fault has stopped it; a fault now stops it.
    fn run(&mut self, f: impl FnOnce(&mut RetailHost)) {
        if self.broken {
            return;
        }
        if catch_unwind(AssertUnwindSafe(|| f(&mut self.host))).is_err() {
            self.broken = true;
        }
    }
}

/// The bytes at `data`, `len` long.
///
/// # Safety
/// `data` is null with `len` 0, or points at `len` readable bytes.
unsafe fn bytes<'a>(data: *const u8, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: the caller hands `len` readable bytes at `data`, as this function's contract says.
    unsafe { std::slice::from_raw_parts(data, len) }
}

/// The host at `host`.
///
/// # Safety
/// `host` is null, or a host [`dereth_host_new`] made and [`dereth_host_free`] has not freed.
unsafe fn host<'a>(host: *mut DerethHost) -> Option<&'a mut DerethHost> {
    // SAFETY: as this function's contract says, a non-null pointer is a live host this module made.
    unsafe { host.as_mut() }
}

/// The interface's version.
#[no_mangle]
pub extern "C" fn dereth_host_abi() -> u32 {
    ABI
}

/// A host from its settings' JSON; null when they do not read.
///
/// # Safety
/// `settings` points at `len` readable bytes (or is null with `len` 0).
#[no_mangle]
pub unsafe extern "C" fn dereth_host_new(settings: *const u8, len: usize) -> *mut DerethHost {
    // SAFETY: the caller's bytes, as this function's contract says.
    let text = unsafe { bytes(settings, len) };
    let made = catch_unwind(|| {
        let text = std::str::from_utf8(text).ok()?;
        let settings = if text.trim().is_empty() {
            Settings::default()
        } else {
            Settings::from_json(text).ok()?
        };
        Some(RetailHost::new(&settings))
    });
    match made {
        Ok(Some(host)) => Box::into_raw(Box::new(DerethHost {
            host,
            out: Vec::new(),
            broken: false,
            told: false,
        })),
        _ => std::ptr::null_mut(),
    }
}

/// The host gone; its stores are written out first.
///
/// # Safety
/// `host` is null, or a host [`dereth_host_new`] made, freed once.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_free(host: *mut DerethHost) {
    if host.is_null() {
        return;
    }
    // SAFETY: a live host this module made, given back once, as this function's contract says.
    let mut h = unsafe { Box::from_raw(host) };
    h.run(|r| r.plugins_mut().flush());
    drop(h);
}

/// A message the client received.
///
/// # Safety
/// `host` as [`dereth_host_free`] says; `data` points at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_server_message(
    host: *mut DerethHost,
    data: *const u8,
    len: usize,
    now: f64,
) {
    // SAFETY: the caller's host and bytes, as this function's contract says.
    let (Some(h), blob) = (unsafe { self::host(host) }, unsafe { bytes(data, len) }) else {
        return;
    };
    h.run(|r| r.server_message(blob, now));
}

/// A message the client sent.
///
/// # Safety
/// As [`dereth_host_server_message`].
#[no_mangle]
pub unsafe extern "C" fn dereth_host_client_message(
    host: *mut DerethHost,
    data: *const u8,
    len: usize,
    now: f64,
) {
    // SAFETY: the caller's host and bytes, as this function's contract says.
    let (Some(h), blob) = (unsafe { self::host(host) }, unsafe { bytes(data, len) }) else {
        return;
    };
    h.run(|r| r.client_message(blob, now));
}

/// The player's place: the cell, the place in its landblock in meters, the heading in degrees
/// clockwise from north.
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_place(
    host: *mut DerethHost,
    landcell: u32,
    x: f64,
    y: f64,
    z: f64,
    heading: f64,
) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(|r| r.set_place(landcell, x, y, z, heading));
    }
}

/// The object the player selected (0 for none).
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_selected(host: *mut DerethHost, id: u32) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(|r| r.set_selected(id));
    }
}

/// The player pressed a key in the game.
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_player_moved(host: *mut DerethHost) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(RetailHost::player_moved);
    }
}

/// Whether the game's window is in front, and whether its chat line is open (each 0 or 1).
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_game_state(
    host: *mut DerethHost,
    focused: i32,
    chat_open: i32,
) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(|r| r.set_game_state(focused != 0, chat_open != 0));
    }
}

/// The player's key bindings as JSON: `{ <the game's action name>: <virtual-key code> }`.
///
/// # Safety
/// `host` as [`dereth_host_free`] says; `json` points at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_bindings(host: *mut DerethHost, json: *const u8, len: usize) {
    // SAFETY: the caller's host and bytes, as this function's contract says.
    let (Some(h), text) = (unsafe { self::host(host) }, unsafe { bytes(json, len) }) else {
        return;
    };
    let Some(v) = std::str::from_utf8(text)
        .ok()
        .and_then(|t| Value::from_json(t).ok())
    else {
        return;
    };
    h.run(|r| r.set_bindings(&v));
}

/// A line the player typed: 1 when a plugin claims it.
///
/// # Safety
/// `host` as [`dereth_host_free`] says; `line` points at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_typed(
    host: *mut DerethHost,
    line: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller's host and bytes, as this function's contract says.
    let (Some(h), text) = (unsafe { self::host(host) }, unsafe { bytes(line, len) }) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return 0;
    };
    let mut claimed = false;
    h.run(|r| claimed = r.typed(text));
    i32::from(claimed)
}

/// An event on a plugin window's view: `{ key, control, event, value }`.
///
/// # Safety
/// `host` as [`dereth_host_free`] says; `json` points at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_view_event(
    host: *mut DerethHost,
    json: *const u8,
    len: usize,
) {
    // SAFETY: the caller's host and bytes, as this function's contract says.
    let (Some(h), text) = (unsafe { self::host(host) }, unsafe { bytes(json, len) }) else {
        return;
    };
    let Some(v) = std::str::from_utf8(text)
        .ok()
        .and_then(|t| Value::from_json(t).ok())
    else {
        return;
    };
    let s = |k: &str| v.get(k).as_str().unwrap_or_default().to_owned();
    let (key, control, event) = (s("key"), s("control"), s("event"));
    let value = v.get("value").clone();
    h.run(|r| r.view_event(&key, &control, &event, &value));
}

/// The messages handed over so far, applied as one batch: for a caller that knows where a
/// datagram ends, with a host made to apply them only when asked.
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_flush(host: *mut DerethHost) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(|r| {
            r.flush();
        });
    }
}

/// One frame.
///
/// # Safety
/// `host` as [`dereth_host_free`] says.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_frame(host: *mut DerethHost, now: f64) {
    // SAFETY: the caller's host, as this function's contract says.
    if let Some(h) = unsafe { self::host(host) } {
        h.run(|r| r.frame(now));
    }
}

/// What to do since the last call, as a JSON array; `len` gets its length. The bytes stay valid
/// until the next call into the host.
///
/// # Safety
/// `host` as [`dereth_host_free`] says; `len` is null or points at a writable `usize`.
#[no_mangle]
pub unsafe extern "C" fn dereth_host_ops(host: *mut DerethHost, len: *mut usize) -> *const u8 {
    // SAFETY: the caller's host, as this function's contract says.
    let Some(h) = (unsafe { self::host(host) }) else {
        return std::ptr::null();
    };
    let mut ops: Vec<Value> = Vec::new();
    h.run(|r| ops = r.take_ops().iter().map(crate::Op::to_value).collect());
    if h.broken && !h.told {
        h.told = true;
        ops.push(
            Value::map()
                .with("op", "print")
                .with(
                    "text",
                    "[Plugins] The plugins stopped on a fault inside the host.",
                )
                .with("color", 6),
        );
    }
    h.out = Value::List(ops).to_json().into_bytes();
    // SAFETY: `len` is null or writable, as this function's contract says.
    if let Some(l) = unsafe { len.as_mut() } {
        *l = h.out.len();
    }
    h.out.as_ptr()
}

/// The host's state as JSON; `len` gets its length. The bytes stay valid until the next call into
/// the host.
///
/// # Safety
/// As [`dereth_host_ops`].
#[no_mangle]
pub unsafe extern "C" fn dereth_host_report(host: *mut DerethHost, len: *mut usize) -> *const u8 {
    // SAFETY: the caller's host, as this function's contract says.
    let Some(h) = (unsafe { self::host(host) }) else {
        return std::ptr::null();
    };
    let mut report = Value::Nil;
    h.run(|r| report = r.report());
    h.out = report.with("broken", h.broken).to_json().into_bytes();
    // SAFETY: `len` is null or writable, as this function's contract says.
    if let Some(l) = unsafe { len.as_mut() } {
        *l = h.out.len();
    }
    h.out.as_ptr()
}
