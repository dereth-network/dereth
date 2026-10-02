//! The classic interface's own settings: the thin block of options only that interface has.
//!
//! Every other setting either interface shows is one shared preference, edited by both
//! interfaces' pages, or a bit of the character's option words on the server. These are the
//! classic interface's alone. The early clients kept the first three in the character's option
//! word (bits 4, 14 and 21); the later client gave those bits other meanings, so the word keeps
//! the later meanings and these live in the profile, under `[UI.Classic]`, beside the shared
//! options. The other three are this client's: which of the social window's pages the classic
//! interface shows besides Allegiance and Fellowship.

use crate::view::PrefValue;

/// `[UI.Classic] InvertMouseLook`: mouse look turns the camera up when the mouse moves down.
/// Only the vertical motion is inverted, unlike `Input.InvertMouseLookYAxis`, which inverts both.
pub const INVERT_MOUSE_LOOK: &str = "UI.Classic.InvertMouseLook";

/// `[UI.Classic] RightClickMouseLook`: holding the right mouse button looks around.
pub const RIGHT_CLICK_MOUSE_LOOK: &str = "UI.Classic.RightClickMouseLook";

/// `[UI.Classic] StretchUI`: the interface is stretched to the window's height.
pub const STRETCH_UI: &str = "UI.Classic.StretchUI";

/// `[UI.Classic] ShowTradeTab`: the social window has its Secure Trade page, on a world with
/// trade. Off, the page is left out and the other tabs share its room.
pub const SHOW_TRADE_TAB: &str = "UI.Classic.ShowTradeTab";

/// `[UI.Classic] ShowFriendsTab`: the social window has its Friends page.
pub const SHOW_FRIENDS_TAB: &str = "UI.Classic.ShowFriendsTab";

/// `[UI.Classic] ShowSquelchTab`: the social window has its Squelch page.
pub const SHOW_SQUELCH_TAB: &str = "UI.Classic.ShowSquelchTab";

/// The block's names.
pub const NAMES: [&str; 6] = [
    INVERT_MOUSE_LOOK,
    RIGHT_CLICK_MOUSE_LOOK,
    STRETCH_UI,
    SHOW_TRADE_TAB,
    SHOW_FRIENDS_TAB,
    SHOW_SQUELCH_TAB,
];

/// The option's value before anything sets it: right-click mouse look and the Friends and
/// Squelch pages on, the rest off.
#[must_use]
pub fn default_on(name: &str) -> bool {
    matches!(
        name,
        RIGHT_CLICK_MOUSE_LOOK | SHOW_FRIENDS_TAB | SHOW_SQUELCH_TAB
    )
}

/// Register the block in the option value store, each option at its default.
pub fn register() -> usize {
    NAMES
        .iter()
        .map(|n| {
            usize::from(super::store::register_preference(
                n,
                PrefValue::Bool(default_on(n)),
                super::store::DataType::Bool,
            ))
        })
        .sum()
}

/// Whether the store holds `name` on.
#[must_use]
pub fn on(name: &str) -> bool {
    matches!(super::store::inq_value(name), Some(PrefValue::Bool(true)))
}

/// Whether `name` is on: as the store holds it, or at its default where the store has no
/// value for it.
#[must_use]
pub fn shown(name: &str) -> bool {
    match super::store::inq_value(name) {
        Some(PrefValue::Bool(b)) => b,
        _ => default_on(name),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an option block's own registration; the interface that reads it is tested
    //! where it does).
    use super::*;

    #[test]
    fn the_block_registers_at_its_defaults_and_holds_what_is_set() {
        super::super::store::init();
        assert!(NAMES.iter().all(|n| on(n)
            == matches!(
                *n,
                RIGHT_CLICK_MOUSE_LOOK | SHOW_FRIENDS_TAB | SHOW_SQUELCH_TAB
            )));
        assert!(super::super::store::set_value(
            STRETCH_UI,
            PrefValue::Bool(true)
        ));
        assert!(on(STRETCH_UI));
        assert!(!on(INVERT_MOUSE_LOOK));
        assert!(super::super::store::set_value(
            SHOW_FRIENDS_TAB,
            PrefValue::Bool(false)
        ));
        assert!(!on(SHOW_FRIENDS_TAB));
    }
}
