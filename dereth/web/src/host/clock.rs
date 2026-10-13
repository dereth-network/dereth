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
/// block. The Slow Down in the Background option changes nothing here, since the browser slows a
/// background tab's animation frames itself.
#[derive(Debug, Default)]
pub struct FramePaced;

impl Pacer for FramePaced {
    fn frame_sleep(&mut self, _is_active_app: bool) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a browser no frame waits, in the background or not, with Slow Down in the Background
    /// on or off: the browser slows a background tab itself.
    ///
    /// Behaviour: frame.pace.slowing-down-in-the-background-is-an-option-on-at-first
    #[test]
    fn a_browser_frame_never_waits_whatever_slow_down_in_the_background_says() {
        use dereth_client_contract::options::{background, store};
        use dereth_client_contract::PrefValue;
        use dereth_client_runtime::platform::clock::pace_frame;
        store::init();
        for on in [true, false] {
            assert!(store::set_value(
                background::SLOW_DOWN_IN_BACKGROUND,
                PrefValue::Bool(on)
            ));
            assert_eq!(pace_frame(&mut FramePaced, false), 0, "on: {on}");
            assert_eq!(pace_frame(&mut FramePaced, true), 0, "on: {on}");
        }
        store::clear();
    }
}
