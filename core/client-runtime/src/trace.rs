//! Live diagnostics: the trace targets. **Instruments, not client features.**
//!
//! Each is a `tracing` target at `debug` level, off unless the log filter asks for it (the
//! client's `--log`, or `[Log] Level=`), and free when it is off: a site checks
//! `tracing::enabled!` before it builds anything. `--log info,dereth::trace=debug` turns all four
//! on; one of them alone is `--log info,dereth::trace::camera=debug`.
//!
//! - [`RAISE`](crate::trace::RAISE), `dereth::trace::raise` -- the `+10` enable inputs on the
//!   **live** login path, which headless tests cannot see: every `Int64` quality as it arrives
//!   (`0x0013`'s table, the `0x02CF` / `0x02D0` updates), every stat-panel row select, and a watch
//!   on `0x100005EB` (the stat panel's `+10` footer button) that prints whenever its state, its
//!   attribute `0x0D`, the costs or the available experience change -- so a later refresh that
//!   overwrites the enable is visible as a second line, not as silence.
//! - [`NOTICE`](crate::trace::NOTICE), `dereth::trace::notice` -- every inbound text-bearing
//!   notice (`0x02EB`, `0x028A`, `0x028B`, `0xF7E0`), every `0x01C7 UseDone` code and every
//!   `0xF750 Sound`, and which receivers took each chat line: the spew box at arrival, and the
//!   chat windows -- with their filters -- at render.
//! - [`CAMERA`](crate::trace::CAMERA), `dereth::trace::camera` -- the body's position, the camera
//!   fields that decide the eye and the render frame, every 30th frame
//!   (`camera::trace_live_camera`).
//! - [`NET`](crate::trace::NET), `dereth::trace::net` -- every received datagram's header.
//!
//! The targets are not module paths because the UI crate emits `raise` and `notice` lines too, and
//! one filter has to reach both halves.

/// The `+10` raise trace's target.
pub const RAISE: &str = "dereth::trace::raise";
/// The notice trace's target.
pub const NOTICE: &str = "dereth::trace::notice";
/// The live camera trace's target.
pub const CAMERA: &str = "dereth::trace::camera";
/// The received-datagram trace's target.
pub const NET: &str = "dereth::trace::net";

/// Whether the log filter has [`RAISE`] on.
#[must_use]
pub fn raise() -> bool {
    tracing::enabled!(target: "dereth::trace::raise", tracing::Level::DEBUG)
}

/// Whether the log filter has [`NOTICE`] on.
#[must_use]
pub fn notice() -> bool {
    tracing::enabled!(target: "dereth::trace::notice", tracing::Level::DEBUG)
}

/// Whether the log filter has [`CAMERA`] on.
#[must_use]
pub fn camera() -> bool {
    tracing::enabled!(target: "dereth::trace::camera", tracing::Level::DEBUG)
}

/// Whether the log filter has [`NET`] on.
#[must_use]
pub fn net() -> bool {
    tracing::enabled!(target: "dereth::trace::net", tracing::Level::DEBUG)
}

/// The first `max` bytes as lower-case hex, with a `..` when truncated.
#[must_use]
pub fn hex(bytes: &[u8], max: usize) -> String {
    let mut s: String = bytes.iter().take(max).map(|b| format!("{b:02x}")).collect();
    if bytes.len() > max {
        s.push_str("..");
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_four_targets_share_the_prefix_one_filter_turns_on() {
        for t in [super::RAISE, super::NOTICE, super::CAMERA, super::NET] {
            assert!(t.starts_with("dereth::trace::"), "{t}");
        }
    }
}
