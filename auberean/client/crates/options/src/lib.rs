//! The client's own preferences below any window: each preference's value, the bank that keeps one
//! of each and tells who uses it, and what an options page asks of them, so every interface edits
//! the same preferences.
//!
//! The values are [`preference`]'s, the bank [`bank`], what a page asks and is listed [`page`].
//!
//! **Depends on** `auberean-dat` and `auberean-text` (how a number reads from the preferences file
//! and is written there) alone. **Used by**
//! the shell, which holds the client's one set of preferences for whichever interface is shown,
//! and by the interfaces' options pages, which list and edit them.
//!
//! **Must never** name an interface's elements, the client's session or the platform: the
//! preferences are the same whatever shows them.

pub mod bank;
pub mod page;
pub mod preference;
