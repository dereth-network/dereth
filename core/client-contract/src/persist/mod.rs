//! Persistence: what survives a mode switch, a session and a re-install.
//!
//! Four stores keep UI state and each has a different lifetime:
//!
//! | Store | Lives in | Survives | Here |
//! |---|---|---|---|
//! | persistent UI state | memory, owned by the UI flow | a UI **mode switch** | state module |
//! | The screen-layout file | `<prefs dir>\UI-<char>-<world>-<h>-<w>.txt` | a session and a resolution change | [`screen_layout`] |
//! | `UserPreferences.ini` | `<install dir>` | everything | [`preferences`] |
//! | `PlayerModule` | the **server** | account-wide | not in this crate |
//!
//! ## Where this lives, and why
//!
//! It cannot live in `dereth-client`: eleven call sites in `dereth-ui-screens`' library and one in
//! `dereth-ui`'s own `framework.rs` consume these types, and all of them sit *above* `dereth-ui`
//! and *below* `dereth-client`, so a move into the application crate would make two dependency
//! cycles. `dereth-client-contract` is the home below them all. `dereth_ui::persist` is a
//! re-export of this module.
//!
//! **The file I/O did not come with the types.** `ScreenLayout`'s `fopen` pair and the path
//! builder its caller reaches through are `dereth_client::persist`, because opening a file is the
//! application's job and a contract crate that touches `std::fs` is not a contract any more. What
//! is here is the shape of the data, the text format, and the parse/serialise pair either side of
//! it — all of which a server, a tool or a test can use with no disk at all.

pub mod persistent_data;
pub mod preferences;
pub mod screen_layout;

pub use persistent_data::{CharacterIdentity, CharacterSet, UiPersistentData};
pub use preferences::{
    PreferenceDataType, UiPreferenceItem, UiPreferences, UserPreferences, FILE_NAME,
};
pub use screen_layout::{SavedWindow, ScreenLayout, WindowSlot, WINDOWS};

/// Everything the persistence parsers here can refuse to do.
///
/// At every `dereth-ui` call site this *is* `dereth_ui::UiError::Persist(String)`: `dereth-ui`
/// keeps the variant and converts (`impl From<PersistError> for UiError`), so a `?` there yields a
/// `UiError::Persist`. The split is forced rather than
/// chosen — `UiError`'s other variants carry `dereth-ui`'s own `ElementType` and `LayoutEnum`, which
/// a crate whose one dependency is `dereth-primitives` cannot name.
///
/// The `Display` text is `UiError::Persist`'s, `"persistence: {0}"`, character for character, so a
/// message that reaches a log or a chat line reads the same whichever type carried it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistError(pub String);

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "persistence: {}", self.0)
    }
}

impl std::error::Error for PersistError {}
