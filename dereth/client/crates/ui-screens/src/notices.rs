//! The inbound notice queue, re-exported from [`dereth_client_contract::notices`].
//!
//! It lives beside [`crate::requests`] for the same reason: it is the other direction of the same
//! contract, its whole production surface is `std::cell::RefCell` and [`crate::view::MagicNotice`],
//! and `dereth_client::interaction` is one of its producers.
//!
//! Every `dereth_ui_screens::notices::…` path resolves through the glob below.
pub use dereth_client_contract::notices::*;
