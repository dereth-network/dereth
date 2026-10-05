//! Neutral panel facts and the synchronous HUD delivery seam.
//!
//! Property identifiers, view kinds and content profiles are shared with producers.
//! Display formatting and display-order tables live in `dereth-presentation`.

/// The character page's property identifiers.
pub mod characterinfo;
/// The contracts panel's sort criterion.
pub mod contracts;
/// The identify window's property identifiers.
pub mod examination;
/// The external-container panel's two subscribed notices.
pub mod external_container;
/// The housing system's thirty-day purchase wait.
pub mod house;
/// The information row kinds carried by panel views.
pub mod inforegion;
/// The paper doll's drag mask, its refusal line and the heritage property.
pub mod inventory;
/// The map's world-profile locations and rectangles.
pub mod map;
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
pub trait HudPanels: std::fmt::Debug {
    /// The speech-bubble strip's final-string receiver. Whether it took the line.
    fn spew_offer(&mut self, ty: u8, body: &str, _feedback: crate::feedback::Feedback) -> bool;
    /// For the notice trace: whether the strip's list is bound, how many lines are pending, and
    /// how many it has drawn.
    fn spew_trace(&self) -> (bool, usize, u64);
    /// Drop the strip's pending lines (log-off takes the windows down with the screen).
    fn spew_clear_pending(&mut self);
    /// The abuse panel's answer to a failure event.
    fn abuse_response(&mut self, code: u32);
}
