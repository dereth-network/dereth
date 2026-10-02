//! The eight main-framework screens.
//!
//! Every screen is the same shape: one or two root elements created from a layout **enum**, a
//! handful of children cached by id, a few notices, and a `queue_ui_mode` on the way out. The mode
//! switch is always deferred — `queue_ui_mode` only records the next mode, and the mode switch
//! runs inside the global-message-3 broadcast **after** everything else, so a screen that queues
//! a mode still finishes the rest of its frame.

pub mod chargen;
pub mod chargen_state;
pub mod charmgmt;
pub mod credits;
pub mod datapatch;
pub mod disconnected;
pub mod epilogue;
pub mod gameplay;
pub mod gameplay_host;
pub mod intro;
pub mod pregame_host;
pub mod screen_message;
/// `WorldView`'s teleport / portal animation.
pub mod teleport;

use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElementId, UiMode};

/// One screen's identity: its local behavior label, the mode that creates it, and the layout enum
/// and root element it builds from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSpec {
    /// The local behavior label used by [`spec`].
    pub class: &'static str,
    pub mode: UiMode,
    /// The layout enums the screen creates its root elements from, in call order.
    pub layouts: &'static [LayoutEnum],
    /// The root element ids, in the same order.
    pub roots: &'static [ElementId],
}

const fn ss(
    class: &'static str,
    mode: u32,
    layouts: &'static [LayoutEnum],
    roots: &'static [ElementId],
) -> ScreenSpec {
    ScreenSpec {
        class,
        mode: UiMode(mode),
        layouts,
        roots,
    }
}

const PATCH: &[LayoutEnum] = &[LayoutEnum(0x1000_0001)];
const INTRO: &[LayoutEnum] = &[LayoutEnum(0x1000_0002)];
const DISCONNECTED: &[LayoutEnum] = &[LayoutEnum(0x1000_0003)];
const CREDITS: &[LayoutEnum] = &[LayoutEnum(0x1000_0004), LayoutEnum(0x1000_0004)];
const CHARMGMT: &[LayoutEnum] = &[LayoutEnum(0x1000_0005)];
const GAMEPLAY: &[LayoutEnum] = &[LayoutEnum(0x1000_0006)];
const EPILOGUE: &[LayoutEnum] = &[LayoutEnum(0x1000_0037)];
const CHARGEN: &[LayoutEnum] = &[LayoutEnum(0x1000_0039)];

/// The eight screens, **in the client's UI-flow registration order**.
///
/// The gameplay screen names only its root element, `0x10000495`, not the layout enum it builds
/// from. That enum is `0x10000006` (`classic_gameplay` → layout `0x21000005`), the only shipped
/// layout whose root element is `0x10000495`, as verified against the shipped layout index and
/// enum map.
pub const SCREENS: [ScreenSpec; 8] = [
    ss(
        "DataPatchScreen",
        0x1000_0003,
        PATCH,
        &[ElementId(0x1000_041A)],
    ),
    ss("IntroScreen", 0x1000_0001, INTRO, &[ElementId(0x1000_0419)]),
    ss(
        "CharacterManagementScreen",
        0x1000_000A,
        CHARMGMT,
        &[ElementId(0x1000_039A)],
    ),
    ss(
        "GamePlayScreen",
        0x1000_0008,
        GAMEPLAY,
        &[ElementId(0x1000_0495)],
    ),
    ss(
        "EpilogueScreen",
        0x1000_0009,
        EPILOGUE,
        &[ElementId(0x1000_0399)],
    ),
    ss(
        "DisconnectedScreen",
        0x1000_0002,
        DISCONNECTED,
        &[ElementId(0x1000_0416)],
    ),
    ss(
        "CharGenScreen",
        0x1000_000B,
        CHARGEN,
        &[ElementId(0x1000_03CC)],
    ),
    // The credits screen builds *two* roots from the same layout enum; which element ids depends on
    // whether Ctrl+Alt+Shift are held. These are the normal pair.
    ss(
        "CreditsScreen",
        0x1000_0005,
        CREDITS,
        &[ElementId(0x1000_0413), ElementId(0x1000_0410)],
    ),
];

/// The three mode ids that are **not** registered in this build.
///
/// Queueing one leaves the flow with no current screen. Reproduce that or assert on it, but do not
/// invent screens for them.
pub const UNREGISTERED_MODES: [UiMode; 3] = [
    UiMode(0x1000_0004),
    UiMode(0x1000_0006),
    UiMode(0x1000_0007),
];

/// The mode `UiFlow`'s constructor queues immediately, before any frame has run.
pub const FIRST_MODE: UiMode = UiMode(0x1000_0003);

/// Look one screen up by its local behavior label.
#[must_use]
pub fn spec(class: &str) -> Option<&'static ScreenSpec> {
    SCREENS.iter().find(|s| s.class == class)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight screens are the documented modes in registration order.
    #[test]
    fn the_eight_screens_are_the_documented_modes_in_registration_order() {
        let j = dereth_ui::framework::mode::REGISTRATION_ORDER;
        let mine: Vec<UiMode> = SCREENS.iter().map(|s| s.mode).collect();
        assert_eq!(mine, j.to_vec());
        assert_eq!(SCREENS[0].class, "DataPatchScreen");
        assert_eq!(
            SCREENS[0].mode, FIRST_MODE,
            "UiFlow queues 0x10000003 first"
        );
        assert_eq!(SCREENS[7].class, "CreditsScreen");
        for m in UNREGISTERED_MODES {
            assert!(!mine.contains(&m), "{m:?} is not registered in this build");
        }
    }

    /// Oracle: §3–§10's per-screen "Root element:" lines, and §6's two-root credits screen.
    #[test]
    fn every_screen_names_one_root_per_layout_enum() {
        for s in SCREENS {
            assert_eq!(
                s.layouts.len(),
                s.roots.len(),
                "{}: one root-element creation per root",
                s.class
            );
            assert!(!s.roots.is_empty());
        }
        let credits = spec("CreditsScreen").unwrap();
        assert_eq!(
            credits.layouts.len(),
            2,
            "two roots, both from layout enum 0x10000004"
        );
        assert_eq!(credits.layouts[0], credits.layouts[1]);
    }
}
