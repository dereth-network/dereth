//! The outbound request queue, re-exported from [`dereth_client_contract::requests`].
//!
//! The queue is the request half of the contract *between* whoever owns the world and whoever
//! draws it: `dereth_client::{hud, interaction}` read `is_local_module_write` and
//! `take_placement_updates` from it, so it cannot live in a crate that draws. It names no
//! `dereth_ui` type -- its whole production surface is `std::cell::RefCell` and
//! [`crate::view::UiRequest`] (`dereth_client_contract::view::UiRequest`).
//!
//! Every `dereth_ui_screens::requests::…` path in this crate and every other resolves through the
//! glob below, deprecation attributes included.
pub use dereth_client_contract::requests::*;
