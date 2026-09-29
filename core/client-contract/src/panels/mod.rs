//! The panel seam.
//!
//! `dereth-ui-screens` draws the panels; `dereth-client` owns the world they show. Everything both
//! sides have to agree about — a property key, a table of ids, a notice the world queues and a
//! panel drains, a formula the world has to evaluate for itself — lives here, at the same module
//! path as under `dereth_ui_screens::panels`, and that crate re-exports each item.
//!
//! Nothing here draws, binds an element or names a `dereth_ui` type. What stays in the UI is the
//! drawing: every `…Panel` struct, its element ids, its layout and its `update`.

/// The character page's ids and its two rating tables.
pub mod characterinfo;
/// The contracts panel's sort criterion.
pub mod contracts;
/// The identify window's property keys, name switches and gear-rating rows.
pub mod examination;
/// The external-container panel's two subscribed notices.
pub mod external_container;
/// The housing system's thirty-day purchase wait.
pub mod house;
/// The info regions' vitae arithmetic and their number formats. Moved whole.
pub mod inforegion;
/// The paper doll's drag mask, its refusal line and the heritage property.
pub mod inventory;
/// The map panel's date-and-time formatter, shared with the calendar clock.
pub mod map;
/// Digit grouping, shared by the HUD's view and the panels.
pub mod numfmt;
/// The salvage panel's three object-carrying notices.
pub mod salvage;
/// The house payment window's operation: which payment list is in play.
pub mod slumlord;
/// Spell-range calculation's skill choice and arithmetic.
pub mod spell_examine;
/// The endowment equipment location.
pub mod spellcasting;
/// The spell-component row's icon.
pub mod spellcomponent;

/// What the HUD model needs of the panel set that sits beside it (the gameplay screen's panels,
/// which the application owns and the model's crate cannot name): the two receivers the event
/// application feeds directly as the events land.
///
/// The speech-bubble strip is the final-string display notice's second receiver, offered every
/// line as it arrives; the abuse panel takes the failure-event answer. Both are synchronous in the
/// client, so the model calls them in place rather than queueing. `dereth_ui_screens` implements it
/// for its panel set.
pub trait HudPanels: std::fmt::Debug + Default {
    /// The speech-bubble strip's final-string receiver. Whether it took the line.
    fn spew_offer(&mut self, ty: u8, body: &str) -> bool;
    /// For the notice trace: whether the strip's list is bound, how many lines are pending, and
    /// how many it has drawn.
    fn spew_trace(&self) -> (bool, usize, u64);
    /// Drop the strip's pending lines (log-off takes the windows down with the screen).
    fn spew_clear_pending(&mut self);
    /// The abuse panel's answer to a failure event.
    fn abuse_response(&mut self, code: u32);
}
