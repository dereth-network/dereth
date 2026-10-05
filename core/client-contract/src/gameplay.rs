//! The gameplay screen's 3D viewport.
//!
//! One element id shared with `dereth_ui_screens::screens::gameplay`, which keeps the screen. The
//! smart box is where the *world* is drawn, so `dereth_client_runtime::interaction` names it when it
//! decides whether a click landed in the world or on a panel, and both sides have to mean the same
//! element.

use crate::ids::ElementId;

/// `<SBOX>` — the 3D viewport.
pub const SMART_BOX: ElementId = ElementId(0x1000_049A);
