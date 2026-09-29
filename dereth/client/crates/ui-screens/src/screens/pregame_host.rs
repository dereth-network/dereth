//! The pre-game screens' host calls, through [`dereth_ui::framework::Screen::on_host_call`]: what the
//! host's development log-in driver and the creation wizard's preview read and do, in the screen
//! crate's own vocabulary, so the host names no concrete screen.

use std::rc::Rc;

use dereth_ui::ElemHandle;

use crate::screens::chargen::{Cg3dView, CharGenTables};
use crate::screens::chargen_state::CharGenState;

/// One pre-game host call.
#[allow(clippy::large_enum_variant)] // one call at a time, handed straight to its handler
#[derive(Debug)]
pub enum PregameCall {
    /// Character management: the row the driver double-clicks — the one named `wanted`
    /// (case-insensitively; an empty name matches none), else the first live one — or `None`
    /// when the list is empty or the row has no element.
    PickCharacterRow {
        wanted: String,
        out: Option<ElemHandle>,
    },
    /// The wizard: whether the unspent-credit warning is the open dialog.
    CreditWarningOpen(bool),
    /// The wizard: answer the open dialog, the dialog-close handler's call.
    CloseDialog(bool),
    /// The wizard: advance the preview turntable by `dt` seconds.
    TickPreview(f64),
    /// The wizard: its preview — view, state and tables.
    WizardPreview(Option<(Cg3dView, CharGenState, Option<Rc<CharGenTables>>)>),
}
