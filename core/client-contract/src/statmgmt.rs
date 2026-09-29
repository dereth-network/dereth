//! The stat-management panel's inputs.
//!
//! `GameView::experience_header` returns [`XpHeader`], so the type is contract: it is what the
//! host computed (`dereth_client_model::advancement::experience_header`) and handed the panel, not anything
//! the panel draws. Its one inherent method is the meter fraction, which is arithmetic on its own
//! fields, so it comes with the type. The panel's `HeaderContent`, `HeaderInputs`, `xp_to_string`
//! and element ids stay in `dereth-ui-screens`, which `pub use`s this type so
//! `dereth_ui_screens::panels::statmgmt::XpHeader` still resolves.

/// The client's inputs, already joined by the host.
///
/// `dereth_client_model::advancement::experience_header` computes all of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct XpHeader {
    /// Integer64 quality 1, `TotalExperience`, already saturated at 2³²−1 for a
    /// non-Throne-of-Destiny account.
    pub total: u64,
    /// `total - xp_to_reach(level)`, where `xp_to_reach` is the experience table's total for a
    /// level.
    pub into_level: u64,
    /// `xp_to_reach(level + 1) - xp_to_reach(level)`; 0 makes the meter 0.
    pub level_span: u64,
    /// `xp_to_reach(level + 1) - total`, forced to 0 at the non-ToD level cap.
    pub to_level: u64,
    /// Integer quality `0x19`, the character level.
    pub level: i32,
    /// True when the level cap has replaced the number with "Infinity!".
    pub at_cap: bool,
}

impl XpHeader {
    /// The experience meter's fraction: `into_level / level_span`, 0 on a zero
    /// span. Both operands go through `float`.
    #[must_use]
    pub fn meter_fill(&self) -> f32 {
        if self.level_span == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.into_level as f32 / self.level_span as f32
        }
    }
}
