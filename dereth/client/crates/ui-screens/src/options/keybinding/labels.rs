//! What a key button shows: the names of controls and bindings.

use super::*;

// ---------------------------------------------------------------------------------------------
// Naming a control
// ---------------------------------------------------------------------------------------------

/// What a key button shows.
///
/// The name of every set meta-mode bit that maps to a key, in bit order, then the key's own name,
/// joined with `ID_KeyDescDelimiter` from table enum 3 — or the empty string when the key itself
/// has no name.
///
/// The empty-name early return is the client's and is kept: an unmapped control shows **nothing**
/// rather than a placeholder, which is what makes the refresh's leftover-button arm and a control
/// with no semantic name look the same on screen.
///
/// `delimiter` is [`token::KEY_DESC_DELIMITER`]'s resolved value, or
/// [`DEFAULT_KEY_DESC_DELIMITER`].
///
/// The `DIK_*` constant name that `name_by_semantic(...)` returns is the *key* into the string
/// table, never the caption; see [`control_name`].
#[must_use]
pub fn binding_label(
    ui: &UiSystem,
    m: &InputManager,
    qc: &ControlChord,
    delimiter: &str,
) -> String {
    dereth_input::labels::binding_label(&m.keymap, *qc, &Labels { ui, delimiter })
}

/// **One** control's display name.
///
/// An out-of-range device index yields an empty name. Otherwise hash the DIK name and look
/// it up in string-table enum 4 for a key or 5 for a meta key. If the row is absent, query
/// the input device's object information; a null device pointer uses the "Mouse-Look" literal.
/// Resolve the subcontrol byte through the six `ID_sci_*` rows in enum 3, then compose
/// `ID_KeyNameWithSubControl` in that same enum with string variable KEY and string-info
/// variable SUBCONTROL.
///
/// `meta` picks between the two tables: passes **4** and
///  passes **5**, and the tables really do differ — `DIK_LWIN`
/// is `"Windows"` in 5 and absent from 4.
///
/// The `GetObjectInfo` half is [`dereth_input::objname::device_object_name`]; the deviation it
/// declares (a transcribed US-English table instead of a live device query) is stated there.
#[must_use]
pub fn control_name(
    ui: &UiSystem,
    device: DeviceType,
    cs: dereth_input::spec::ControlCode,
    meta: bool,
) -> String {
    dereth_input::labels::control_name(
        device,
        cs,
        meta,
        &Labels {
            ui,
            delimiter: DEFAULT_KEY_DESC_DELIMITER,
        },
    )
}

/// The client's sub-control-to-token mapping, index by index.
#[must_use]
pub fn sub_control_token(sub: dereth_input::spec::SubControlIndex) -> Option<&'static str> {
    dereth_input::labels::sub_control_token(sub)
}

struct Labels<'a> {
    ui: &'a UiSystem,
    delimiter: &'a str,
}

impl dereth_input::labels::LabelProvider for Labels<'_> {
    fn resolve_token(&self, table: u32, token: &str) -> Option<String> {
        resolve_token(self.ui, table, token)
    }

    fn format_subcontrol(&self, key: &str, subcontrol: &str) -> Option<String> {
        self.ui.resolve_string_named(
            string_table(self.ui, table_enum::KEY_DESC),
            dereth_primitives::num::hash::str_hash(token::KEY_NAME_WITH_SUB_CONTROL.as_bytes()),
            &[(var::KEY, key), (var::SUB_CONTROL, subcontrol)],
        )
    }

    fn delimiter(&self) -> &str {
        self.delimiter
    }
}
