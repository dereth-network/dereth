//! The power bar's mode: which activity the charge bar is showing.
//!
//! The combat state decides the mode and the power-bar widgets guard on it, so the jump bar and the
//! attack bar never fight over the one meter. Both read it here: `dereth_client_model::combat` and
//! `dereth_ui_screens::hud::powerbar` re-export it.

/// What the power bar is charging for. The numbers are the ones the begin, set-level and finish
/// notices carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum PowerBarMode {
    #[default]
    Undef = 0,
    Combat = 1,
    AdvancedCombat = 2,
    Jump = 3,
    /// Dat-patch progress, which reuses the same widget.
    Ddd = 4,
}
