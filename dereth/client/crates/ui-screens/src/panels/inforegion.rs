//! The info regions' formatting, re-exported.
//!
//! Every item lives in [`dereth_client_contract::panels::inforegion`]: the module is
//! `dereth_primitives::num` and arithmetic, it names no `dereth_ui` type, and `dereth_client::hud`
//! calls `apply_vitae` and `vitae_modifier` directly.
//!
//! Every `dereth_ui_screens::panels::inforegion::…` path resolves through the glob below.
pub use dereth_client_contract::panels::inforegion::*;
