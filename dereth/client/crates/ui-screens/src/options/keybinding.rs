//! The action-key-map option and keyboard page — key-binding rows and the capture flow.
//!
//! Action-key-map type `0x10000034` gets its base widget in [`super::controls`] and its back end
//! from [`dereth_input::binding`]; this module builds the rows and joins the two through the three
//! calls a page needs.
//!
//! # What the shipped layout actually carries, measured
//!
//! There is exactly **one** element description of type `0x10000034` in all 101 shipped layouts:
//! `0x1000002F` of `0x21000009`, which is **template 1** of every one of the
//! six key-binding list boxes. It carries
//!
//! ```text
//! 0x1000002B = Array([ {id 0x1000002C, Enum 0x10000030},
//!                      {id 0x1000002C, Enum 0x10000031},
//!                      {id 0x1000002C, Enum 0x10000032} ])
//! ```
//!
//! and its three children are exactly `0x10000030`, `0x10000031`, `0x10000032`.
//!
//! **So `0x1000002B` is the key-button array, not the action id** (see
//! [`super::pages::action_key_map`]). The client reads it with the **array** accessor, one call after reading
//! `0x1000002A` as an enum and resolving the result as a button — the clear button, which
//! refresh greys (state `0x0D`) while the current binding list is empty and which the message-1
//! arm compares against before clearing every binding. The shipped template declares **no**
//! `0x1000002A`, so a shipped row has three key buttons and no clear button.
//!
//! The action id and input-map id come from row initialization,
//! called with the list box, action, input map, name, tooltip and default keys — not from any
//! attribute. Every field initialization must fill is
//! named by its call site and by the client's initialiser list.
//!
//! # Row controls and full lists
//!
//! The optional clear button clears the current bindings. The message-`0x19` guard checks
//! the selected slot against the current list, so user-added bindings can be erased.
//!
//! A full row recycles its head: it unbinds that control, binds it to DoNothing unless
//! it conflicts with the new control, removes it from the current list, and appends the
//! new control. Removing the head keeps the list within the number of key buttons;
//! this implementation does not retain an unbound key as a visible row entry.

use dereth_input::binding::{Capture, Conflict, DO_NOTHING};
use dereth_input::presentation::{self, Interface};
use dereth_input::spec::{activation, ControlChord};
use dereth_input::{ActionId, DeviceType, InputManager, InputMapId};
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

mod ids;
pub use ids::*;
mod labels;
pub use labels::*;
mod row;
pub use row::*;
mod prompts;
pub use prompts::*;
mod page;
pub use page::*;

use crate::panels::listbox::ListBoxWidget;

#[cfg(test)]
mod tests;
