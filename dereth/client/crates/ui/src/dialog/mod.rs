//! Dialogs, tooltips and `InfoRegion`.
//!
//! Dialog, tooltip, and context-menu behavior live here.
//!
//! Dialogs are ordinary elements (types 0x13–0x19) created by a process-wide dialog controller that keeps
//! **queues** of pending dialogs keyed by a context id, so a burst of server confirmation requests
//! appears one at a time rather than stacked.
//!
//! The client's generic context-menu framework is **dead code**: its manager is only ever
//! null, so no context menu of that kind is ever constructed, and it is omitted here. What AC
//! actually does is element message **0x27** from an
//! element with property 0x37, handled by the owning panel — see
//! [`crate::msg::element::id::CONTEXT_MENU`].

pub mod base;
pub mod factory;
pub mod inforegion;
pub mod tooltip;
pub mod types;

pub use base::{AnswerRole, Dialog, DialogChild, DialogKind};
pub use factory::{DialogController, DialogInfo};
pub use inforegion::{InfoRegion, InfoRegionKind};
pub use tooltip::TooltipState;

/// The *gameplay* reason a confirmation is on screen. It is stored on the gameplay screen, **not**
/// on the dialog, which is why it is a plain value rather than dialog state; the contract owns it.
pub use dereth_client_contract::confirmation::ConfirmationType;

pub mod resolution;
