//! Slowing down in the background: whether the client draws fewer frames while its window is not
//! the active one.
//!
//! The original client spaced its frames at least 99 milliseconds apart, about ten a second,
//! whenever its window was not the active one, and drew as fast as it could otherwise. This client
//! does the same while [`SLOW_DOWN_IN_BACKGROUND`] is on, which it is at first; off, the window
//! keeps its full frame rate in the background. It is this client's own option, not a retail one,
//! and belongs to no interface: every interface's Client Options page shows it, and it is saved in
//! the profile like any other. The frame pace reads it every frame, so a change applies at once.

use crate::view::PrefValue;

/// `[Display] SlowDownInBackground`: whether frames slow down while the window is in the
/// background.
pub const SLOW_DOWN_IN_BACKGROUND: &str = crate::options::names::DISPLAY_SLOW_DOWN_IN_BACKGROUND;

/// The option's caption, literal text: no string table has one.
pub const CAPTION: &str = "Slow Down in the Background";

/// What the option does, where a page explains it. A browser slows a page in a background tab
/// itself, so in a browser the option changes nothing.
pub const NOTE: &str = "Draws about ten frames a second while the window is not the active one, \
                        as the original did. A browser slows a background tab by itself, so there \
                        this changes nothing.";

/// Register the option in the option value store, on.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        SLOW_DOWN_IN_BACKGROUND,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    ))
}

/// Whether frames slow down while the window is in the background: yes unless the store holds
/// the option off, so a store that has no value for it paces as the original did.
#[must_use]
pub fn slows_down() -> bool {
    !matches!(
        super::store::inq_value(SLOW_DOWN_IN_BACKGROUND),
        Some(PrefValue::Bool(false))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::preferences::UserPreferences;

    /// The option is on at first and in a store that does not hold it; turned off, it is saved in
    /// the profile's `[Display]` section and a profile read back turns it off again.
    ///
    /// Behaviour: frame.pace.slowing-down-in-the-background-is-an-option-on-at-first
    #[test]
    fn the_option_is_on_at_first_and_a_saved_profile_brings_it_back_off() {
        super::super::store::clear();
        assert!(
            slows_down(),
            "a store without the option paces as the original did"
        );
        super::super::store::init();
        assert_eq!(
            super::super::store::inq_value(SLOW_DOWN_IN_BACKGROUND),
            Some(PrefValue::Bool(true))
        );
        assert!(slows_down());

        assert!(super::super::store::set_value(
            SLOW_DOWN_IN_BACKGROUND,
            PrefValue::Bool(false)
        ));
        assert!(!slows_down());
        let text = super::super::store::save().to_text();
        let display = text
            .split("[Display]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Display] section: {text}"));
        assert!(
            display.contains("SlowDownInBackground=False\r\n"),
            "{display}"
        );

        // A fresh start is on again, until the saved profile is read.
        super::super::store::init();
        assert!(slows_down());
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        super::super::store::load(&ini);
        assert!(!slows_down());
        super::super::store::clear();
    }
}
