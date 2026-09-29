//! The read-only view onto game state, and the request channel back out.
//!
//! Every item lives in [`dereth_client_contract`]; this module is the `pub use` that keeps
//! `dereth_ui_screens::view::…` resolving.
//!
//! `GameView`, `UiRequest` and the ~45 `…View` value structs are the contract *between* whoever
//! owns the world and whoever draws it. Kept in the crate that draws, they would force
//! `dereth-client`, which owns the world, to depend **upward** on this crate to name a read or a
//! command. Both sides depend on `dereth-client-contract` instead, and
//! `cargo tree -e normal -p dereth-client-contract` is the gate that keeps it that way.
//!
//! This crate does not depend on `dereth-client-model`: game state reaches a panel as
//! `&dyn GameView` and as `Notice`s, and requests leave as `UiRequest` values the binary routes.
//!
//! **The client predicts nothing.** `GameView` is `&dyn` and has no `&mut` anywhere, so a panel
//! physically cannot write the value it is about to ask the server for.

pub use dereth_client_contract::*;
