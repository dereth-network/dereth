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
/// block. The frame rate limits change nothing here, since the browser paces a page's animation
/// frames, and slows a background tab's, itself.
#[derive(Debug, Default)]
pub struct FramePaced;

impl Pacer for FramePaced {
    fn frame_sleep(&mut self, _spacing_us: u64) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a browser no frame waits, in the background or not, whatever the frame rate limits
    /// say: the browser paces the page itself.
    ///
    /// Behaviour: frame.pace.the-active-window-and-the-background-each-have-a-frame-rate-limit
    #[test]
    fn a_browser_frame_never_waits_whatever_the_frame_rate_limits_say() {
        use dereth_client_contract::options::{frame_rate, store};
        use dereth_client_contract::PrefValue;
        use dereth_client_runtime::platform::clock::pace_frame;
        store::init();
        for fps in frame_rate::CHOICES {
            let fps = PrefValue::Int(i32::try_from(fps).unwrap());
            assert!(store::set_value(frame_rate::MAX_FRAME_RATE, fps.clone()));
            assert!(store::set_value(
                frame_rate::MAX_BACKGROUND_FRAME_RATE,
                fps.clone()
            ));
            assert_eq!(pace_frame(&mut FramePaced, false, false), 0, "{fps:?}");
            assert_eq!(pace_frame(&mut FramePaced, true, false), 0, "{fps:?}");
        }
        store::clear();
    }
}
