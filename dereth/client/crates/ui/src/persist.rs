//! Persistence — UI persistence and keystone state, re-exported under this crate's path.
//!
//! The types live in [`dereth_client_contract::persist`]: call sites in `dereth-ui-screens` and
//! this crate's own `framework.rs` read them, so they cannot live *up* in `dereth-client`, where
//! the files they describe are read and written, without making dependency cycles.
//! `dereth-client-contract` sits below every one of those readers; the file reading and writing
//! is `dereth_client_shell::persist`.
//!
//! Everything in the contract module resolves through this one. One item lives here instead,
//! because it names something a crate whose only dependency is `dereth-primitives` cannot name:
//! [`preferences::token_of`] — `dereth_primitives::num::hash::str_hash`. An inherent associated
//! function cannot be added to a type from another crate, so this is a free function in the same
//! module and the call sites in `dereth-ui-screens` name it that way.

pub mod persistent_data {
    //! Re-exports the contract crate's persistent-data module under its historical path.
    pub use dereth_client_contract::persist::persistent_data::*;
}

pub mod preferences {
    //! [`dereth_client_contract::persist::preferences`], under the path it was written at, plus the one
    //! function that could not go with it.
    pub use dereth_client_contract::persist::preferences::*;

    /// The localisation token, the string hash of an `ID_*` name — `dereth_primitives::num::hash::str_hash`
    /// is that hash.
    ///
    /// `UiPreferenceItem` is `dereth-client-contract`'s, and that crate depends on
    /// `dereth-primitives` and nothing else, so the hash — which is `dereth_primitives::num`'s —
    /// is a free function on this side.
    #[must_use]
    pub fn token_of(id_name: &str) -> u32 {
        dereth_primitives::num::hash::str_hash(id_name.as_bytes())
    }
}

pub mod screen_layout {
    //! [`dereth_client_contract::persist::screen_layout`], under the path it was written at. The `fopen`
    //! pair and the `layout_path` branch that were here are `dereth_client_shell::persist`.
    pub use dereth_client_contract::persist::screen_layout::*;
}

pub use dereth_client_contract::persist::{
    CharacterIdentity, CharacterSet, PersistError, PreferenceDataType, SavedWindow, ScreenLayout,
    UiPersistentData, UiPreferenceItem, UiPreferences, UserPreferences, WindowSlot, FILE_NAME,
    WINDOWS,
};

#[cfg(test)]
mod tests {
    use super::preferences::token_of;
    use super::*;
    use preferences::{keys, UiPreferenceItem, UiPreferences, UI_STRING_TABLE};

    /// "`stringTableEnum` is always `0x10000003` (the UI string table);
    /// `tokenName`/`tokenTooltip` are the string hash of the localisation ids".
    #[test]
    fn a_preference_item_carries_the_ui_string_table_and_hashed_tokens() {
        let mut ui = UiPreferences::new();
        let item = UiPreferenceItem::new(
            keys::TOOLTIP_DELAY,
            3,
            UI_STRING_TABLE,
            token_of("ID_Misc_TooltipDelay"),
            token_of("ID_Misc_TooltipDelay_Tooltip"),
        )
        .unwrap();
        assert_eq!(item.string_table, 0x1000_0003);
        assert_ne!(item.token_name, item.token_tooltip);
        assert_eq!(
            item.token_name,
            dereth_primitives::num::hash::str_hash(b"ID_Misc_TooltipDelay")
        );
        ui.attach(item);
        assert_eq!(ui.len(), 1);
        assert!(ui.inq(keys::TOOLTIP_DELAY).is_some());
        ui.detach(keys::TOOLTIP_DELAY);
        assert!(ui.is_empty());
    }
}
