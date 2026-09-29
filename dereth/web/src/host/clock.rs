//! The clocks and the local time zone. The clocks are the runtime's; the zone is the browser's.

pub use dereth_client_runtime::platform::clock::{
    system_unix_time, Clock, FixedStepClock, Pacer, SystemClock, Timer, EXTERNAL_TIME_EPSILON,
};

/// The local zone's shift from UTC at `unix_secs`, in seconds, as the browser's date arithmetic
/// applies it (with that instant's daylight rule). `0` off the web, where there is no browser to
/// ask.
#[must_use]
pub fn local_utc_offset_secs(unix_secs: i64) -> i32 {
    #[cfg(target_arch = "wasm32")]
    {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: a Unix instant in milliseconds fits
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(unix_secs as f64 * 1000.0));
        // Minutes west of UTC: the opposite sign.
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: at most a day of minutes
        {
            -(date.get_timezone_offset() as i32) * 60
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = unix_secs;
        0
    }
}

/// A pacer that never sleeps: the page's animation frames pace the client, and a browser cannot
/// block.
#[derive(Debug, Default)]
pub struct FramePaced;

impl Pacer for FramePaced {
    fn frame_sleep(&mut self, _is_active_app: bool) -> u32 {
        0
    }
}
