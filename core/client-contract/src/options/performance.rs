//! The performance panel: frame rate and frame time over the game, whatever the interface.
//!
//! This is this client's own option, not a retail one, and it belongs to no interface:
//! [`PERFORMANCE_PANEL`] is a boolean in the option value store ([`crate::options::store::init`]),
//! saved in the profile like any other, and the
//! [`crate::actions::dereth::TOGGLE_PERFORMANCE_PANEL`] action flips it.

use crate::view::PrefValue;

/// `[Debug] PerformancePanel`: whether the performance panel shows.
pub const PERFORMANCE_PANEL: &str = crate::options::names::PERFORMANCE_PANEL;

/// The option's caption, literal text: no string table has one.
pub const CAPTION: &str = "Performance Panel";

/// Register the option in the option value store, off.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        PERFORMANCE_PANEL,
        PrefValue::Bool(false),
        super::store::DataType::Bool,
    ))
}

/// Whether the store says the panel shows.
#[must_use]
pub fn shown() -> bool {
    matches!(
        super::store::inq_value(PERFORMANCE_PANEL),
        Some(PrefValue::Bool(true))
    )
}

/// Flip the option in the store, and return the value it now holds.
pub fn toggle() -> PrefValue {
    let v = PrefValue::Bool(!shown());
    let _ = super::store::set_value(PERFORMANCE_PANEL, v.clone());
    v
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an option's own value; the panel it drives is tested where it is drawn).
    use super::*;

    #[test]
    fn the_option_is_off_until_toggled_and_toggles_back() {
        super::super::store::init();
        assert!(!shown());
        assert_eq!(toggle(), PrefValue::Bool(true));
        assert!(shown());
        assert_eq!(toggle(), PrefValue::Bool(false));
        assert!(!shown());
    }
}
