//! The four option pages, the seven option-control types and the key-binding page.
//!
//! They follow the client's options screens.

pub mod character;
pub mod chat;
pub mod config;
pub mod controls;
pub mod gameplay;
pub mod keybinding;
pub mod page;
pub mod pages;
pub mod preferences;
/// The options both interfaces show, which these pages draw.
pub use dereth_client_contract::options::sheet;
pub mod store;
pub mod toggle;
