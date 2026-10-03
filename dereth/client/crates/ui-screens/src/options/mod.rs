//! The four option pages, the seven option-control types and the key-binding page.
//!
//! They follow the client's options screens.

pub mod character;
/// The Chat Options page.
pub mod chat;
pub mod config;
pub mod controls;
/// The Game / Support page's seven buttons.
pub mod gameplay;
pub mod keybinding;
pub mod page;
pub mod pages;
pub mod preferences;
/// The options both interfaces show, which these pages draw.
pub use dereth_client_contract::options::sheet;
pub mod store;
/// The option checkboxes a *panel* binds, which are not on any option page.
pub mod toggle;
