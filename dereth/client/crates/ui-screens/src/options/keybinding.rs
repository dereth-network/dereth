//! The action-key-map option and keyboard page — key-binding rows and the capture flow.
//!
//! Action-key-map type `0x10000034` gets its base widget in [`super::controls`] and its back end
//! from [`dereth_input::binding`]; this module builds the rows and joins the two through the three
//! calls a page needs.
//!
//! # What the shipped layout actually carries, measured
//!
//! There is exactly **one** element description of type `0x10000034` in all 101 shipped layouts:
//! `0x1000002F` of `0x21000009` (`classic_keyboard`), which is **template 1** of every one of the
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
//! `0x1000002A`, so a shipped row has three key buttons and no clear button. \[verified\]
//!
//! The action id and input-map id come from row initialization,
//! called with the list box, action, input map, name, tooltip and default keys — not from any
//! attribute. Every field initialization must fill is
//! named by its call site and by the client's initialiser list.
//!
//! # How the row was disambiguated
//!
//! The row's accesses resolve consistently by behavior:
//!
//! * the optional clear-button handle is tested before clearing;
//! * the key-button array has separate storage and count values;
//! * the binding list checked by the message handler is the **current** list, not the defaults;
//! * its bounds check uses the selected key-button slot before erasing.
//!
//! These four roles also agree with row initialization and refresh.
//! The current-list bounds check is the decisive distinction.
//!
//! That last role matters: the message-`0x19` guard is *"the slot is within the current list"*, not
//! *"within the defaults"*; the other reading would have made a user-added binding
//! un-eraseable. \[verified\]
//!
//! # One inference, declared
//!
//! When the row is full (as many current bindings as key buttons), the binding write takes the
//! head of the current list, unbinds it, rebinds it to "do nothing" unless the new control
//! conflicts with it, then binds the new control to the row's action and appends it to the list.
//!
//! **No removal of the head is observed.** Leaving the head in place would let the current list
//! grow past the key-button count for ever while `ActionKeyMapRow::refresh` went on drawing,
//! at slot 0, a key that had just been unbound. [`ActionKeyMapRow::set_binding`]
//! therefore **removes the head**, which is what "recycle" means. Marked `[inferred]`, and the
//! alternative is stated here so it can be checked against retail.

use dereth_input::binding::{Capture, Conflict, DO_NOTHING};
use dereth_input::presentation::{self, Interface};
use dereth_input::spec::{activation, ControlChord};
use dereth_input::{ActionId, DeviceType, InputManager, InputMapId};
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::panels::listbox::ListBoxWidget;

// ---------------------------------------------------------------------------------------------
// The transcribed constants
// ---------------------------------------------------------------------------------------------

/// The client's two attributes, and the two
/// writes.
pub mod attr {
    /// Enum attribute `0x1000002A` — the element id of the row's clear button.
    ///
    /// **Not** "the child that displays the binding": the *display* children are the array below.
    /// The shipped template carries no `0x1000002A` at all.
    pub const CLEAR_BUTTON: u32 = 0x1000_002A;
    /// The **array** of key-button element ids that becomes the row's key buttons.
    pub const KEY_BUTTONS: u32 = 0x1000_002B;
    /// The id every member of that array carries.
    pub const KEY_BUTTON_MEMBER: u32 = 0x1000_002C;
    /// String-info attribute `0x49` on a key button — the binding text the row's refresh writes.
    pub const BINDING_TEXT: u32 = 0x49;
    /// Bool attribute `0x0D` on the clear button, set while the current binding list is empty —
    /// the Clear button greys itself when there is nothing to clear.
    pub const CLEAR_DISABLED: u32 = 0x0D;
}

/// The keyboard panel's post-init's eight attributes.
///
/// **The first is not one of eight list boxes.** `0x10000018` is the id of the list-box child
/// *inside each tab page*, looked up once per page by a recursive child search for the page and
/// then, inside it, for `0x10000018`'s value; the six pages are hard-coded ids in the panel's
/// post-init. `0x10000019`..`0x1000001F` are seven single children — the
/// keymap load/save controls and the filename label. In the shipped `classic_keyboard` tree the
/// values are `0x10000025` (the list box) and `0x1000002C, 0x1000002D, 0x1000002A, 0x1000002B,
/// 0x10000028, 0x10000027, 0x10000029`. [verified — measured off the built tree]
pub mod page_attr {
    /// The list-box child id, read once and applied inside each of the six tab pages.
    pub const LIST_BOX_CHILD: u32 = 0x1000_0018;
    /// The keyboard OK button — the *Apply* half of the element-message handler.
    pub const OK_BUTTON: u32 = 0x1000_0019;
    /// The keyboard Cancel button.
    pub const CANCEL_BUTTON: u32 = 0x1000_001A;
    /// The keyboard Reset-to-Defaults button.
    pub const RESET_TO_DEFAULTS_BUTTON: u32 = 0x1000_001B;
    /// The keyboard Revert-to-Saved button.
    pub const REVERT_TO_SAVED_BUTTON: u32 = 0x1000_001C;
    /// The keymap filename label update's label.
    pub const KEYMAP_FILENAME_LABEL: u32 = 0x1000_001D;
    /// The Load Keymap button.
    pub const LOAD_KEYMAP_BUTTON: u32 = 0x1000_001E;
    /// The Save Keymap button.
    pub const SAVE_KEYMAP_BUTTON: u32 = 0x1000_001F;
    /// All eight, in order, for a test that wants to state the range rather than re-derive it.
    pub const ALL: [u32; 8] = [
        0x1000_0018,
        0x1000_0019,
        0x1000_001A,
        0x1000_001B,
        0x1000_001C,
        0x1000_001D,
        0x1000_001E,
        0x1000_001F,
    ];
}

/// The keyboard-page element in the shipped `classic_gameplay` tree — type `0x1000000E`.
///
/// Measured off the built tree, and pinned as a literal by the key-binding tests. \[verified\]
pub const KEYBOARD_UI: ElementId = ElementId(0x1000_0020);

/// The six tab pages looks a list box up inside, in the order
/// it looks them up. Each is keyed in the panel's list-box table by one **action class**.
pub const KEYBOARD_TAB_PAGES: [ElementId; 6] = [
    ElementId(0x1000_049D),
    ElementId(0x1000_049F),
    ElementId(0x1000_04A1),
    ElementId(0x1000_04A3),
    ElementId(0x1000_0211),
    ElementId(0x1000_04A5),
];

/// An action's class — the key of the panel's list-box table, and the reason there
/// are six tab pages.
///
/// Class 0 is "internal" and is exactly what refuses, so
/// the six non-zero classes and the six pages are the same six. The names are
/// `dereth_input::actionmap::action_class`'s doc comment.
///
/// **In page order, not class order**: entry `n` is the class of [`KEYBOARD_TAB_PAGES`]`[n]`.
/// The shipped tabs read Movement, Camera, Combat, UI, Character Settings, Emotes, and the
/// client registers each page's list box under the class its tab names — 1, 2, **4, 3, 7, 5**.
/// Pairing the pages with the classes in numeric order put the UI bindings under the Combat
/// tab, Combat under UI, Emotes under Character Settings and Character Settings under Emotes.
pub const ACTION_CLASSES: [(u32, &str); 6] = [
    (1, "Movement"),
    (2, "Camera"),
    (4, "Combat"),
    (3, "UI"),
    (7, "CharacterOption"),
    (5, "Emote"),
];

/// The dialog **queue** every dialog this control raises shares, and the one the key-button
/// handler gates both of its arms on with an is-dialog-open test for `0x10000001`.
///
/// It is not the default queue 2 (`dereth_ui::dialog::factory::DEFAULT_QUEUE`): a key-binding dialog
/// must not queue behind an unrelated confirmation, and a row must not start a capture while one
/// is up.
pub const DIALOG_QUEUE: u64 = 0x1000_0001;

/// The shared shipped `Dialog` layout (`0x2100003C` in this DAT build), whose Wait root is
/// selected by [`dialog_kind`].
pub const DIALOG_LAYOUT: dereth_ui::LayoutEnum = dereth_ui::LayoutEnum(2);

/// The string table enum every caption in this control comes from — `0x10000004`, **not** the
/// `0x10000003` the rest of the option pages use (`super::preferences::STRING_TABLE_ENUM`).
///
/// The page refresh and the binding initiation both use it to select the string table for
/// their captions.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0004;

/// The row's string table in retail — the table the client's two ids
/// resolve in, and therefore where a row's action name and tooltip come from.
pub const ACTION_STRING_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0005);

/// The four string tokens this control names.
pub mod token {
    /// The wrapper a **bound** key button's caption is written as, with
    /// variable `LABEL` set to the control's name.
    pub const BUTTON_LABEL: &str = "ID_ActionKeyMap_ButtonLabel";
    /// The tooltip on a bound key button, with variable `VALUE` set to the caption above.
    pub const TT_EXISTING_BINDING: &str = "ID_ActionKeyMap_TT_ExistingBinding";
    /// The tooltip on an **empty** key button — the ones past the end of the current list, which
    /// `ActionKeyMapRow::refresh` first clears.
    pub const TT_NEW_BINDING: &str = "ID_ActionKeyMap_TT_NewBinding";
    /// The map-warn dialog's prompt, with variable `ACTION` set to
    /// the row's own text.
    pub const MAP_INSTRUCTIONS: &str = "ID_ActionKeyMap_MapInstructions";
    /// The open overwrite binding dialog's **one-conflict** prompt, with
    /// variables `KEY` (the key just captured) and `ACTION` (the action it is already bound to).
    ///
    /// Shipped row, table `0x23000004` \[measured\]:
    /// `["'", "' is currently bound to '", "'. Do you wish to erase that binding?"]`,
    /// `variables = [KEY, ACTION]`.
    pub const OVERWRITE_EXISTING_BINDING: &str = "ID_ActionKeyMap_OverwriteExistingBinding";
    /// The same dialog's **many-conflict** prompt, with variables `KEY` and
    /// `BINDINGS` — the per-conflict list built out of [`BINDING`], one line each.
    ///
    /// Shipped row \[measured\]:
    /// `["'", "' conflicts with the following bindings:\n", "\nDo you wish to erase those
    /// bindings?"]`, `variables = [KEY, BINDINGS]`.
    pub const OVERWRITE_EXISTING_BINDINGS: &str = "ID_ActionKeyMap_OverwriteExistingBindings";
    /// One line of that list, with variables `ACTION` and `KEY` — **in that order**,
    /// which is the reverse of the row above.
    ///
    /// Shipped row \[measured\]: `["'", "' ('", "')"]`, `variables = [ACTION, KEY]`.
    pub const BINDING: &str = "ID_ActionKeyMap_Binding";
    /// The client's prompt, with variable `KEY` alone.
    ///
    /// Shipped row \[measured\]: `["'", "' is currently bound to a non user-bindable action. Please
    /// select a different binding."]`, `variables = [KEY]`.
    pub const NON_USER_BINDABLE_BINDING: &str = "ID_ActionKeyMap_NonUserBindableBinding";
    /// The load keymap dialog build's prompt.
    pub const LOAD_KEYMAP_LABEL: &str = "ID_KeyMapLoadKeymap_Label";
    /// The save keymap dialog build's prompt.
    pub const SAVE_KEYMAP_LABEL: &str = "ID_KeyMapSaveKeymap_Label";
    /// The cant overwrite read only keymap dialog build's prompt.
    pub const CANT_OVERWRITE_READ_ONLY_KEYMAP_LABEL: &str =
        "ID_KeyMapCantOverwriteReadOnlyKeymap_Label";
    /// The overwrite keymap dialog build's prompt.
    pub const OVERWRITE_KEYMAP_LABEL: &str = "ID_KeyMapOverwriteKeymap_Label";
    ///  joins a control's modifier names and its
    /// key name with this, out of string table enum **3**.
    pub const KEY_DESC_DELIMITER: &str = "ID_KeyDescDelimiter";
    /// The client's sub-control wrapper, out of enum **3** — variables
    /// `KEY` and `SUBCONTROL`. Measured against the shipped table: the three literal pieces are
    /// `["", " ", ""]`, i.e. `"{KEY} {SUBCONTROL}"`.
    pub const KEY_NAME_WITH_SUB_CONTROL: &str = "ID_KeyNameWithSubControl";
    /// The client's six rows, indexed by
    /// [`dereth_input::spec::SubControlIndex`], all out of enum **3**. Index 0 (`None`) has no row —
    /// the function returns `false` for it.
    pub const SUB_CONTROL: [&str; 6] = [
        "ID_sci_PositiveAxis",
        "ID_sci_NegativeAxis",
        "ID_sci_POVUp",
        "ID_sci_POVRight",
        "ID_sci_POVDown",
        "ID_sci_POVLeft",
    ];
}

/// The **variable names** the client files its values under, which is the only handle a row has on
/// them.
///
/// The string-info string-variable insert keys its variables by
/// the client's string hash of the name, and the string-table read looks each
/// of the **row's** variable ids up in that table. The client builds these three names
/// (`ACTION`, `BINDINGS`, `KEY`) once for the key-binding control.
///
/// Passing them to [`UiSystem::resolve_string_named`] is what makes the two orders the shipped
/// table really uses — `ID_ActionKeyMap_OverwriteExistingBinding` is `KEY, ACTION` and
/// `ID_ActionKeyMap_Binding` is `ACTION, KEY` — a property of the *dat* rather than of this file.
pub mod var {
    /// The action's descriptive name, in the two conflict prompts and the map-warn dialog.
    pub const ACTION: &str = "ACTION";
    /// The per-conflict list, in the many-conflict prompt.
    pub const BINDINGS: &str = "BINDINGS";
    /// A control's name — the captured key in the prompts, the base key in the sub-control
    /// wrapper.
    pub const KEY: &str = "KEY";
    /// The sub-control decoration in `ID_KeyNameWithSubControl`.
    pub const SUB_CONTROL: &str = "SUBCONTROL";
    /// A bound key button's caption wrapper, [`super::token::BUTTON_LABEL`].
    pub const LABEL: &str = "LABEL";
    /// The nested caption inside [`super::token::TT_EXISTING_BINDING`].
    pub const VALUE: &str = "VALUE";
    /// The keymap file name in the four `ID_KeyMap*_Label` dialog prompts.
    pub const KEYMAP: &str = "KEYMAP";
}

/// The `DidMapper` **group** every string-table enum lookup uses —
/// `super::preferences::STRING_TABLE_GROUP`, repeated here because this control resolves four
/// different string tables and none of them is the preferences one.
pub const STRING_TABLE_GROUP: u32 = 4;

/// The four `stringTableEnum` values this control selects string tables with,
/// and what each resolves to against the shipped `client_local_English.dat`.
///
/// | enum | `DidMapper` group 4 | what lives there |
/// |---|---|---|
/// | 3 | `0x23000007` | `ID_KeyDescDelimiter` (`"+"`), `ID_KeyNameWithSubControl`, the six `ID_sci_*` |
/// | 4 | `0x2300000A` | the **key-name overrides**: `DIK_LCONTROL` → `"Left Ctrl"`, `DIK_LMENU` → `"Left Alt"` — and nothing else |
/// | 5 | `0x2300000B` | the **meta-key overrides**: `DIK_LWIN`, `DIK_RWIN` → `"Windows"` |
/// | 7 | `0x23000005` | the eighteen `ID_InputMap_*` section titles |
///
/// All four are measured, not inferred; the fallbacks below are only what a headless host with no
/// asset source can answer.
pub mod table_enum {
    /// Key-name lookup and its internal helper use this table for the delimiter and the
    /// sub-control wrapper.
    pub const KEY_DESC: u32 = 3;
    /// Ordinary key names pass table enum 4 to the internal lookup.
    pub const KEY_NAME: u32 = 4;
    /// Meta-key names pass table enum 5 to the internal lookup.
    pub const META_KEY_NAME: u32 = 5;
    /// Input-map captions use table enum 7 in all nineteen switch arms.
    pub const INPUT_MAP: u32 = 7;
}

/// The string table `value` names in `DidMapper` group 4, through the installed environment, with the
/// measured answer as the fallback so a headless test still names the right table.
///
/// The same shape `super::preferences::table` has, and for the same reason.
#[must_use]
pub fn string_table(ui: &UiSystem, enum_value: u32) -> dereth_primitives::DataId {
    let measured = match enum_value {
        table_enum::KEY_DESC => 0x2300_0007,
        table_enum::KEY_NAME => 0x2300_000A,
        table_enum::META_KEY_NAME => 0x2300_000B,
        table_enum::INPUT_MAP => 0x2300_0005,
        _ => 0x2300_0004,
    };
    ui.env()
        .cloned()
        .and_then(|e| e.did_by_enum(STRING_TABLE_GROUP, enum_value))
        .unwrap_or(dereth_primitives::DataId(measured))
}

/// The string whose id is `token`'s hash (`dereth_primitives::num::hash::str_hash`), looked up in the enum's
/// table — the one shape every lookup in this control has.
///
/// `None` is the string-invalid branch used by both ordinary and meta-key lookup.
/// An **empty** row answers `None` too: the client's
/// validity test is a length test, and every caller here treats a zero-length row as absent.
#[must_use]
pub fn resolve_token(ui: &UiSystem, enum_value: u32, token: &str) -> Option<String> {
    let id = dereth_primitives::num::hash::str_hash(token.as_bytes());
    ui.resolve_string(string_table(ui, enum_value), id)
        .filter(|s| !s.is_empty())
}

/// The delimiter used when [`token::KEY_DESC_DELIMITER`] cannot be resolved.
///
/// The client reads it from string table enum 3 at every call; a headless host
/// with no resolver installed would otherwise join `SHIFT` and `W` with nothing at all, which is
/// worse than a wrong separator because it is unreadable. **Measured**: the row in `0x23000007`
/// is `"+"`, so the fallback and the client's value agree.
pub const DEFAULT_KEY_DESC_DELIMITER: &str = "+";

/// The state a key button is in when it may **not** be right-click-erased —
/// the client's `state == 0x0D` guard.
pub const DISABLED_STATE: StateId = StateId(0x0D);

/// The enabled state written when [`ActionKeyMapRow::changed`] is true.
pub const ENABLED_STATE: StateId = StateId(1);

/// Template-list entry 0 is the section header row.
/// The page inserts it, resolves the input-map caption,
/// and writes that caption onto the header.
pub const TEMPLATE_HEADER: usize = 0;
/// Template-list entry 1 — the action-key-map row, the index
/// the keyboard panel's insert passes.
pub const TEMPLATE_ROW: usize = 1;

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
    let device = m
        .keymap
        .device_type_of(qc.control)
        .unwrap_or(DeviceType::Keyboard);
    let key = control_name(ui, device, qc.control, false);
    // An unnamed control is the empty string and leaves the cell
    // blank rather than drawing a modifier list with no key.
    if key.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut bit = 1u32;
    loop {
        if qc.meta_mode & bit != 0 {
            if let Some(cs) = m.keymap.key_from_meta_mode(bit) {
                let d = m.keymap.device_type_of(cs).unwrap_or(DeviceType::Keyboard);
                let n = control_name(ui, d, cs, true);
                if !n.is_empty() {
                    parts.push(n);
                }
            }
        }
        let Some(next) = bit.checked_mul(2) else {
            break;
        };
        if next == 0 {
            break;
        }
        bit = next;
    }
    parts.push(key);
    parts.join(delimiter)
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
    use dereth_input::spec::ControlNames;

    let enum_value = if meta {
        table_enum::META_KEY_NAME
    } else {
        table_enum::KEY_NAME
    };
    let name = ControlNames::name_by_semantic(device, cs.offset())
        .and_then(|dik| resolve_token(ui, enum_value, dik))
        .or_else(|| {
            dereth_input::objname::device_object_name(device, cs.offset()).map(str::to_owned)
        })
        .unwrap_or_default();
    if name.is_empty() {
        return name;
    }
    // Add the sub-control decoration. `SubControlIndex::None` has no row, so that branch leaves
    // the bare key name alone.
    let Some(tok) = sub_control_token(cs.sub_control()) else {
        return name;
    };
    let Some(sub) = resolve_token(ui, table_enum::KEY_DESC, tok) else {
        return name;
    };
    // The shipped row is `["", " ", ""]` — the literal pieces around `KEY` and `SUBCONTROL`, in
    // that order. The values go through the string read's meta-language arm rather than being
    // interleaved here. For this row the interleave and the render agree on the characters; the
    // render adds excess-space trimming, so a key name that arrives with a doubled space draws
    // one.
    let Some(out) = ui.resolve_string_named(
        string_table(ui, table_enum::KEY_DESC),
        dereth_primitives::num::hash::str_hash(token::KEY_NAME_WITH_SUB_CONTROL.as_bytes()),
        &[(var::KEY, name.as_str()), (var::SUB_CONTROL, sub.as_str())],
    ) else {
        return name;
    };
    if out.is_empty() {
        name
    } else {
        out
    }
}

/// The client's sub-control-to-token mapping, index by index.
#[must_use]
pub fn sub_control_token(sub: dereth_input::spec::SubControlIndex) -> Option<&'static str> {
    use dereth_input::spec::SubControlIndex as S;
    Some(match sub {
        S::PositiveAxis => token::SUB_CONTROL[0],
        S::NegativeAxis => token::SUB_CONTROL[1],
        S::PovUp => token::SUB_CONTROL[2],
        S::PovRight => token::SUB_CONTROL[3],
        S::PovDown => token::SUB_CONTROL[4],
        S::PovLeft => token::SUB_CONTROL[5],
        S::None | S::Other(_) => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// One row
// ---------------------------------------------------------------------------------------------

/// Which of the row's three dialogs a context belongs to —
/// dispatches on exactly this, comparing the closing context against its three fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDialog {
    /// The open map warn dialog / the close map warn dialog — "press a key".
    MapWarn,
    /// "That key is already used by X; replace?".
    Overwrite,
    /// The existing binding is not user-bindable.
    CantOverwrite,
}

/// What [`ActionKeyMapRow::on_element_message`] did, so a caller can assert on an arm rather than
/// on a side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowEvent {
    /// Message 1 on the clear button — every binding freed to [`DO_NOTHING`].
    ClearedAll(usize),
    /// Message 1 on key button `slot` — capture entered.
    CaptureStarted { slot: usize },
    /// Message 1 on a key button while a dialog is open on [`DIALOG_QUEUE`]: refused, silently, by
    /// the client's own is-dialog-open guard.
    Refused,
    /// Message `0x19` with `p1 == 8` (secondary click) on key button `slot`.
    Erased { slot: usize },
}

/// One bindable action's key-map row.
#[derive(Debug, Clone, Default)]
pub struct ActionKeyMapRow {
    /// The row element itself carries the action's name as text, which
    /// is what the binding initiation reads back.
    pub element: Option<ElemHandle>,
    /// The clear button, from attribute [`attr::CLEAR_BUTTON`]. `None` in the shipped layout.
    pub clear_button: Option<ElemHandle>,
    /// The key buttons, from attribute [`attr::KEY_BUTTONS`]. Three in the shipped template.
    pub key_buttons: Vec<ElemHandle>,
    /// The action this row binds.
    pub action: ActionId,
    /// The input map the action lives in.
    pub input_map: InputMapId,
    /// The default keys — the keys the action had when [`Self::init`] ran.
    pub defaults: Vec<ControlChord>,
    /// The saved keys, what Cancel reverts to.
    pub saved: Vec<ControlChord>,
    /// The current keys.
    pub current: Vec<ControlChord>,
    /// The chord being bound — `None` is the client's invalid key `0xFFFFFFFF`.
    pub binding_being_changed: Option<ControlChord>,
    /// The slot being bound — `None` is the client's `-1`.
    pub slot_being_changed: Option<usize>,
    /// Skip the overwrite confirmation; set only when restoring mouse-turning defaults.
    pub skip_confirmation: bool,
    /// The contexts of the row's three dialogs: map-warn, overwrite, can't-overwrite.
    pub dialogs: Vec<(RowDialog, u64)>,
    /// The action's name, as resolved it.
    pub label: String,
    /// Its description, the same way — the client's id.
    pub tooltip: String,
    /// What [`Self::refresh`] last wrote onto each key button, in key-button order. Empty
    /// where the button is past the end of the current list. Recorded because a caption that landed
    /// and a caption that did not are otherwise indistinguishable headless.
    pub button_labels: Vec<String>,
    /// The value [`Self::refresh`] last wrote to the clear button's [`attr::CLEAR_DISABLED`], or
    /// `None` when there is no clear button — which is the shipped case, and is a fact about the
    /// data rather than a failure.
    pub clear_disabled: Option<bool>,
    /// The arguments of the calls
    /// this row has made and [`KeyBindingPage::drain_refresh_notices`] has not delivered yet.
    ///
    /// Retail sends that notice from exactly **two** places — erasing and setting a binding —
    /// and in both it is the last thing done before returning. The notice is how a rebind reaches
    /// the *other* rows: [`Self::set_binding`] displaces every conflicting control out of every map that can be
    /// registered beside this one, and the row those controls belonged to is only told this way.
    ///
    /// Each action-key-map row listens on the global event handler, so the
    /// client sends straight from the row; here the rows are owned by [`KeyBindingPage`], so the
    /// row records the call and the page performs the broadcast in the same gesture.
    pub pending_refresh_notices: Vec<ControlChord>,
}

impl ActionKeyMapRow {
    /// Bind the clear button and the key buttons.
    ///
    /// The client bails out of the whole of the row's post-init when `0x1000002A` is present and
    /// its child does not resolve, which is why the attribute is read first and
    /// why a row whose layout names a clear button that is not there registers no notice. Absent
    /// attribute is **not** that case: the enum read answers false and the function carries
    /// on to the array.
    #[must_use]
    pub fn post_init(ui: &UiSystem, element: ElemHandle) -> Option<Self> {
        let mut row = Self {
            element: Some(element),
            ..Self::default()
        };
        if let Some(id) = crate::bind::attr_enum(ui, element, attr::CLEAR_BUTTON) {
            let h = ui.get_child_recursive(element, ElementId(id))?;
            row.clear_button = Some(h);
        }
        for id in key_button_ids(ui, element) {
            if let Some(h) = ui.get_child_recursive(element, id) {
                row.key_buttons.push(h);
            }
        }
        Some(row)
    }

    /// The action key map option control's initialisation, as the keyboard panel's action key
    /// map insert calls it.
    ///
    /// `keys` is `InputManager::find_keys_for_action`'s answer on the **merged** master map, which is
    /// what makes the default, saved and current lists all start equal.
    /// `keys` is the current list, returned by `find_keys_for_action` on the **merged** map;
    /// `defaults` is the default list, passed as the fifth argument and copied from the
    /// keys of the **shipped** default maps. They are a different map from `keys`; using one for
    /// both would make *Restore Defaults* restore whatever the page was opened with.
    ///
    /// The saved list is **not** written by the client's initialisation: it is left empty and the
    /// closing `save_current_values` of `init_options` fills it. Seeded from `keys` anyway, because
    /// a row built outside `init_options` would otherwise read as changed the moment it
    /// appeared.
    pub fn init(
        &mut self,
        action: ActionId,
        input_map: InputMapId,
        label: String,
        tooltip: String,
        keys: Vec<ControlChord>,
        defaults: Vec<ControlChord>,
    ) {
        self.action = action;
        self.input_map = input_map;
        self.label = label;
        self.tooltip = tooltip;
        self.defaults = defaults;
        self.saved.clone_from(&keys);
        self.current = keys;
    }

    /// The action key map option control's refresh.
    ///
    /// Two loops and a tail, in the client's order:
    ///
    /// 1. walk the current list alongside the key buttons, stopping at whichever runs out first,
    ///    and for each write the control's name onto the button and a tooltip beside it;
    /// 2. every remaining button has its text cleared and gets the *new binding* tooltip;
    /// 3. the clear button's bool attribute `0x0D` is set to "the current list is empty".
    ///
    /// Resolve the caption's `LABEL` and the bound tooltip's nested `VALUE` through the existing
    /// StringTable renderer. The resulting literals keep the shipped decoration and meta-language;
    /// no tooltip prose or key names are supplied by this control.
    pub fn refresh(&mut self, ui: &mut UiSystem, m: &InputManager, delimiter: &str) {
        self.button_labels.clear();
        self.button_labels
            .resize(self.key_buttons.len(), String::new());
        for (i, h) in self.key_buttons.clone().into_iter().enumerate() {
            let text = match self.current.get(i) {
                Some(qc) => binding_label(ui, m, qc, delimiter),
                None => String::new(),
            };
            let table = string_table(ui, STRING_TABLE_ENUM);
            let (caption, help) = if text.is_empty() {
                (
                    text,
                    resolve_token(ui, STRING_TABLE_ENUM, token::TT_NEW_BINDING),
                )
            } else {
                let caption = ui
                    .resolve_string_named(
                        table,
                        dereth_primitives::num::hash::str_hash(token::BUTTON_LABEL.as_bytes()),
                        &[(var::LABEL, text.as_str())],
                    )
                    .unwrap_or(text);
                let help = ui.resolve_string_named(
                    table,
                    dereth_primitives::num::hash::str_hash(token::TT_EXISTING_BINDING.as_bytes()),
                    &[(var::VALUE, caption.as_str())],
                );
                (caption, help)
            };
            set_literal(ui, h, &caption);
            ui.set_tooltip(h, help);
            ui.set_tooltip_on(h, true);
            self.button_labels[i] = caption;
        }
        self.clear_disabled = self.clear_button.map(|h| {
            let v = self.current.is_empty();
            crate::bind::set_attr_bool(ui, h, attr::CLEAR_DISABLED, v);
            v
        });
    }

    /// The action key map option control's element-message handler.
    ///
    /// Both arms do nothing while a dialog is open on queue `0x10000001`. Message 1 on the clear
    /// button clears every binding; message 1 on key button `i` starts a capture for slot `i`.
    /// Message `0x19` with first parameter 8 on key button `i` erases binding `i`, but only when
    /// the button is not in state `0x0D` and `i` is inside the current list.
    ///
    /// Parameter 8 is `dereth_ui::focus::action::SECONDARY_CLICK`: **right-clicking a key button
    /// frees it**, and the state `0x0D` and in-range guards are why right-clicking an empty
    /// or greyed slot does nothing.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
        delimiter: &str,
    ) -> Option<RowEvent> {
        use dereth_ui::msg::element::id;
        let blocked = ui.dialogs.is_dialog_open(DIALOG_QUEUE);
        if msg.id == id::BUTTON_CLICKED {
            if blocked {
                return self
                    .key_buttons
                    .contains(&msg.source)
                    .then_some(RowEvent::Refused);
            }
            if Some(msg.source) == self.clear_button {
                let n = self.clear_all_bindings(ui, m, delimiter);
                return Some(RowEvent::ClearedAll(n));
            }
            let slot = self.key_buttons.iter().position(|b| *b == msg.source)?;
            self.initiate_binding(ui, slot);
            return Some(RowEvent::CaptureStarted { slot });
        }
        if msg.id == id::MOUSE_CLICK && msg.p1 == dereth_ui::focus::action::SECONDARY_CLICK {
            if blocked {
                return self
                    .key_buttons
                    .contains(&msg.source)
                    .then_some(RowEvent::Refused);
            }
            let slot = self.key_buttons.iter().position(|b| {
                *b == msg.source && ui.node(*b).is_some_and(|n| n.state != DISABLED_STATE)
            })?;
            if slot >= self.current.len() {
                return None;
            }
            self.erase_binding(ui, m, slot, delimiter);
            return Some(RowEvent::Erased { slot });
        }
        None
    }

    /// The action-key-map option control's binding initialization: record the slot, open the
    /// map-warn dialog with `ID_ActionKeyMap_MapInstructions` (`ACTION` = the row's text), and on
    /// success register the row as the input handler.
    ///
    /// Returns whether the dialog was raised, which is the client's own return and the flag it
    /// gates the input-handler registration on.
    ///
    /// **The dialog's text is the instruction, not the row's bare action label** — a modal saying
    /// only *"Move Forward"* reads as "clicking a cell does nothing". The client sequence is
    /// the string `ID_ActionKeyMap_MapInstructions` from table enum `0x10000004`, with variable
    /// `ACTION` set to the row's own label.
    pub fn initiate_binding(&mut self, ui: &mut UiSystem, slot: usize) -> bool {
        self.slot_being_changed = Some(slot);
        let text = map_instructions(ui, &self.label);
        self.open_dialog(ui, RowDialog::MapWarn, &text)
    }

    /// The capture ended, however it ended.
    pub fn close_map_warn_dialog(&mut self, ui: &mut UiSystem) {
        self.close_dialog_context(ui, RowDialog::MapWarn);
    }

    /// Whether a capture is in flight — the client's *"is the key-hit handler registered"*, which
    /// is "the map-warn dialog context is set".
    #[must_use]
    pub fn capturing(&self) -> bool {
        self.dialog_context(RowDialog::MapWarn).is_some()
    }

    /// The capture, end to end.
    ///
    /// The **policy** is [`InputManager::capture_key_hit`], transcribed from this same function;
    /// what lives here is everything around it that is the *row's*: unregistering
    /// the handler, closing the map-warn dialog, opening one of the two overwrite dialogs, and
    /// calling [`Self::set_binding`] when there is nothing to ask about.
    ///
    /// `control` is the **event**, so its activation is `Down` or `Up`; step 4 of the policy is
    /// what turns it into a `Click` binding. Handing it a binding-shaped control would trip
    /// `activation & 0x81` and be [`Capture::Ignored`], which is why the client passes the raw
    /// event and so does this.
    pub fn key_hit(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        control: ControlChord,
        delimiter: &str,
    ) -> Capture {
        let verdict =
            m.capture_key_hit(self.input_map, self.action, control, self.skip_confirmation);
        match &verdict {
            // The handler stays registered and the dialog stays up: these are not answers.
            Capture::Ignored | Capture::Rejected => return verdict,
            _ => {}
        }
        // Unregister the input handler, then close the map-warn dialog — the client's own order,
        // and it happens before every remaining arm including `Cancelled`.
        self.close_map_warn_dialog(ui);
        match verdict.clone() {
            Capture::Cancelled | Capture::Unchanged => {
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            Capture::Refused(_) => {
                // The chord being bound is written **before** the conflict scan — step 4's
                // key the player just pressed. [`Capture::Refused`] does not carry it, so the
                // same normalisation is repeated here rather than changing that enum.
                let pending =
                    ControlChord::new(control.control, control.meta_mode, activation::CLICK);
                let text = non_user_bindable_prompt(ui, m, &pending, delimiter);
                self.open_dialog(ui, RowDialog::CantOverwrite, &text);
            }
            Capture::NeedsConfirmation { control, conflicts } => {
                self.binding_being_changed = Some(control);
                let text = overwrite_prompt(ui, m, &control, &conflicts, delimiter);
                self.open_dialog(ui, RowDialog::Overwrite, &text);
            }
            Capture::Ready { control, .. } => {
                let slot = self.slot_being_changed;
                self.set_binding(m, control, slot);
                self.binding_being_changed = None;
                self.slot_being_changed = None;
                self.refresh(ui, m, delimiter);
            }
            Capture::Ignored | Capture::Rejected => unreachable!("handled above"),
        }
        verdict
    }

    /// The action key map option control's close dialog notice.
    ///
    /// The overwrite dialog reads property `0x92` (`dereth_ui::props::attr::DIALOG_ANSWER`) out of
    /// the returned collection and, **only on yes**, calls
    /// [`Self::set_binding`] with the pending chord and slot. Both the yes and the no path
    /// then clear the pending control and slot; the can't-overwrite dialog clears them and does
    /// nothing else. A context that is neither is ignored, which is what stops one row answering
    /// another's dialog.
    ///
    /// Returns whether a binding was made.
    pub fn close_dialog(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        context: u64,
        answer: bool,
        delimiter: &str,
    ) -> bool {
        let Some(which) = self
            .dialogs
            .iter()
            .find(|(_, c)| *c == context)
            .map(|(w, _)| *w)
        else {
            return false;
        };
        self.close_dialog_context(ui, which);
        let mut bound = false;
        match which {
            RowDialog::Overwrite => {
                if answer {
                    if let Some(qc) = self.binding_being_changed {
                        bound = self.set_binding(m, qc, self.slot_being_changed);
                        self.refresh(ui, m, delimiter);
                    }
                }
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            RowDialog::CantOverwrite => {
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            RowDialog::MapWarn => {}
        }
        bound
    }

    /// The page's half of it.
    ///
    /// [`InputManager::set_binding`] is the map surgery, deliberately without the row's two
    /// pieces of state: *how many key buttons this row has* and *what to do when they are all
    /// full*. Both are here.
    ///
    /// * `slot` inside the current list — a **replacement**: the old control stops firing the action
    ///   and is bound to [`DO_NOTHING`] so a save-and-reload does not hand it back (see that
    ///   constant; this is the whole reason a "clear" is not a delete).
    /// * `slot` outside it, with a button free — an **addition**: the shipped rows carry several
    ///   keys per action and this is how the second one is made.
    /// * `slot` outside it, with **no** button free — a **recycle**: the head of the current list is
    ///   unbound, bound to `DoNothing` unless the incoming control conflicts with it, and dropped.
    ///   See the module docs for what is verified here and what is inferred.
    pub fn set_binding(
        &mut self,
        m: &mut InputManager,
        control: ControlChord,
        slot: Option<usize>,
    ) -> bool {
        use dereth_input::spec::ControlCode;
        if control.control == ControlCode::INVALID || control.activation == 0 {
            return false;
        }
        let replacing = slot.filter(|i| *i < self.current.len());
        if replacing.is_none() && self.current.len() >= self.key_buttons.len() {
            // As many current bindings as key buttons — the row is full.
            let Some(head) = self.current.first().copied() else {
                return false;
            };
            m.unbind_by_key(&head, self.input_map);
            if !control.is_conflicting(&head) {
                m.bind_action(head, DO_NOTHING, self.input_map);
            }
            self.current.remove(0);
        }
        let ok = m.set_binding(self.input_map, self.action, replacing, control);
        if !ok {
            return false;
        }
        // The current list is the client's own mirror of what `find_keys_for_action` would answer; the
        // client keeps it by hand (an append, or an in-place overwrite of slot `i`) rather than
        // re-reading, and so does this, so that a row with a control the map cannot name still
        // shows what the player put there.
        match replacing {
            Some(i) => self.current[i] = control,
            None => self.current.push(control),
        }
        // Send the refresh-action-key-mapping notice — with the control
        // that was *just bound*, not with any of the controls it displaced. That is what makes the
        // one notice reach both sides: the row losing the key still lists that same control.
        self.pending_refresh_notices.push(control);
        true
    }

    /// The action key map option control's erase binding: unbind the chord, bind it to action 1,
    /// send the refresh-action-key-mapping notice, then tell the option-change handler.
    ///
    /// **Action 1 is `DoNothing`, and the binding is written rather than deleted.** That is the
    /// whole of why "clear" is not "remove": loading adds what is *absent* and merges the user
    /// file **before** the shipped default map, so a control the user file does not mention gets
    /// its shipped default back on the next run — and the player would find both keys firing.
    ///
    /// Note what it does **not** do: it does not remove the entry from the current list. The client
    /// does not either — it erases every index and *then* flushes the list, which would
    /// double-erase if each erase shortened it.
    pub fn erase_binding(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        i: usize,
        delimiter: &str,
    ) -> bool {
        let Some(qc) = self.current.get(i).copied() else {
            return false;
        };
        m.unbind_by_key(&qc, self.input_map);
        m.bind_action(qc, DO_NOTHING, self.input_map);
        self.current.remove(i);
        // Send the refresh-action-key-mapping notice.
        self.pending_refresh_notices.push(qc);
        self.refresh(ui, m, delimiter);
        true
    }

    /// [`Self::erase_binding`] over every index, then empty the current list and
    /// [`Self::refresh`].
    ///
    /// Returns how many bindings were freed.
    pub fn clear_all_bindings(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) -> usize {
        let n = self.current.len();
        for qc in std::mem::take(&mut self.current) {
            m.unbind_by_key(&qc, self.input_map);
            m.bind_action(qc, DO_NOTHING, self.input_map);
            // Retail's clear-all reaches every index through its single-binding erase, so it raises one
            // notice per freed control rather than one for the row.
            self.pending_refresh_notices.push(qc);
        }
        self.refresh(ui, m, delimiter);
        n
    }

    /// The action key map option control's refresh mappings and
    /// The refresh-action-key-mapping notice, sent when a binding is set or erased — re-read the
    /// map and redraw.
    ///
    /// This is what makes *another* row's rebind show up on this one: setting a binding displaces
    /// conflicting controls in every map that can be registered at the same time, so a row whose
    /// key was taken must be told.
    pub fn refresh_mappings(&mut self, ui: &mut UiSystem, m: &InputManager, delimiter: &str) {
        self.current = m.find_keys_for_action(self.action, self.input_map);
        self.refresh(ui, m, delimiter);
    }

    /// The action key map option control's refresh action key mapping notice — the *keyed* arm,
    /// which is the one both senders of the notice actually use.
    ///
    /// A control with the invalid key or a zero activation is ignored; otherwise the row looks the
    /// control up in its own current list and refreshes its mappings only when it is there.
    ///
    /// So a row redraws **only when the notified control is one of the controls it is currently
    /// showing** — the row that just lost the key, and the row that just took it. Every other row
    /// on the page is left alone, which is why this is not a page rebuild.
    ///
    /// Returns whether this row redrew.
    pub fn on_refresh_action_key_mapping(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        control: &ControlChord,
        delimiter: &str,
    ) -> bool {
        use dereth_input::spec::ControlCode;
        if control.control == ControlCode::INVALID || control.activation == 0 {
            return false;
        }
        if !self.current.iter().any(|c| c == control) {
            return false;
        }
        self.refresh_mappings(ui, m, delimiter);
        true
    }

    /// Same length, and exactly equal pairwise.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.saved.len() != self.current.len()
            || self
                .saved
                .iter()
                .zip(&self.current)
                .any(|(a, b)| !a.is_exactly_equal(b))
    }

    /// The save current value.
    pub fn save_current_value(&mut self) {
        self.saved.clone_from(&self.current);
    }

    /// Rebind the action to exactly the saved list.
    pub fn restore_saved_value(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) {
        self.apply_list(ui, m, self.saved.clone(), delimiter);
    }

    /// The restore default value.
    pub fn restore_default_value(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) {
        self.apply_list(ui, m, self.defaults.clone(), delimiter);
    }

    fn apply_list(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        want: Vec<ControlChord>,
        delimiter: &str,
    ) {
        for qc in self.current.clone() {
            if !want.iter().any(|w| w.is_exactly_equal(&qc)) {
                m.unbind_by_key(&qc, self.input_map);
                m.bind_action(qc, DO_NOTHING, self.input_map);
            }
        }
        m.unbind_all_by_action(self.action, self.input_map);
        for qc in &want {
            m.bind_action(*qc, self.action, self.input_map);
        }
        self.current = want;
        self.refresh(ui, m, delimiter);
    }

    /// The action key map option control's mouse turning defaults write, reached from the
    /// global-message handler's global message `0x0C`.
    ///
    /// Two rows and only two: **action `0x33` in map 5** takes `{key 0x00080101, meta 0,
    /// activation 2}` and **action `0x34` in map 5** takes `{key 0x00080201, meta 0, activation
    /// 2}` — the mouse wheel up and down, offered to the row's own key-hit handler with
    /// skip-confirmation set so the overwrite dialog is skipped. Every other row does nothing at
    /// all.
    ///
    /// Returns the chat lines the client prints, which are
    /// [`super::config::MOUSE_TURNING_KEY_MESSAGES`]; this is their producer.
    pub fn set_mouse_turning_defaults(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.input_map != InputMapId(5) {
            return out;
        }
        let row = match self.action.0 {
            0x33 => Some((dereth_input::spec::ControlCode(0x0008_0101), 0usize)),
            0x34 => Some((dereth_input::spec::ControlCode(0x0008_0201), 1usize)),
            _ => None,
        };
        let Some((cs, line)) = row else { return out };
        self.skip_confirmation = true;
        let qc = ControlChord::new(cs, 0, activation::UP);
        self.key_hit(ui, m, qc, delimiter);
        self.binding_being_changed = None;
        self.slot_being_changed = None;
        self.skip_confirmation = false;
        out.push(super::config::MOUSE_TURNING_KEY_MESSAGES[line]);
        out
    }

    /// The open context for one of this row's three dialogs.
    #[must_use]
    pub fn dialog_context(&self, which: RowDialog) -> Option<u64> {
        self.dialogs
            .iter()
            .find(|(w, _)| *w == which)
            .map(|(_, c)| *c)
    }

    /// The `Make*Dialog` body every one of the three shares: refuse when this row already has one
    /// of that kind open, otherwise fill a `PropertyCollection` and hand
    /// it to the dialog factory.
    ///
    /// `0xC3` is [`DIALOG_QUEUE`], which is the property set
    /// second and the one the element-message handler's open-dialog check reads.
    fn open_dialog(&mut self, ui: &mut UiSystem, which: RowDialog, text: &str) -> bool {
        if self.dialog_context(which).is_some() {
            return false;
        }
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(dialog_kind(which).property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_QUEUE_ID,
            dereth_assets::ui::PropertyValue::Integer(
                i32::try_from(DIALOG_QUEUE).unwrap_or(i32::MAX),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            dereth_assets::ui::PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(text.to_string()),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        self.dialogs.push((which, context));
        true
    }

    fn close_dialog_context(&mut self, ui: &mut UiSystem, which: RowDialog) {
        let Some(i) = self.dialogs.iter().position(|(w, _)| *w == which) else {
            return;
        };
        let (_, context) = self.dialogs.remove(i);
        if let Some(h) = ui.dialogs.close_dialog(context, ui.now.0) {
            ui.remove_and_delete_root(h);
        }
    }
}

/// The kind each of the three dialogs is.
///
/// The map warn is a `Wait`: it has no buttons, it is modal, and the way out of it is a key press
/// or Escape, which is the key-hit handler's `Cancelled` arm. The overwritable-conflict dialog is a
/// `Confirmation`; it reads its answer from property `0x92`.
/// The can't-overwrite notice is a one-button `Message` and carries no answer property.
///
/// Each dialog's property collection carries four properties, in this order: `0x8E` (the kind),
/// `0xC3` (the queue), `0xAC` (modal) and `0xC5` (the text).
///
/// So `MapWarn` is `Wait`, and the four properties this build sets are exactly the
/// four the client sets. The overwritable-conflict dialog uses kind `1`,
/// while the cannot-overwrite dialog uses kind `3`.
#[must_use]
pub const fn dialog_kind(which: RowDialog) -> dereth_ui::dialog::DialogKind {
    match which {
        RowDialog::MapWarn => dereth_ui::dialog::DialogKind::Wait,
        RowDialog::Overwrite => dereth_ui::dialog::DialogKind::Confirmation,
        RowDialog::CantOverwrite => dereth_ui::dialog::DialogKind::Message,
    }
}

/// The client's **first** id, resolved — one conflicting action's
/// own name, which is the value every `ACTION` variable in this control carries.
///
/// `descrip_values` returns the two *string ids* the client hands the option build; the row's
/// label comes from resolving the first against the row's string table, never from printing
/// the id.
///
/// The client abandons the whole dialog when it cannot resolve the key's description; both
/// failure branches return without making the dialog. Here an unresolvable
/// name becomes the empty string instead, which is what
/// writes for a variable the caller never named — a dialog with a gap in it is closer to the
/// player's situation than no dialog at all, and is stated rather than hidden.
#[must_use]
fn conflict_action_name(ui: &UiSystem, m: &InputManager, c: &Conflict) -> String {
    let table = dereth_primitives::DataId(m.action_map.string_table);
    let (name_id, _) = m.action_map.descrip_values(c.input_map, c.action);
    ui.resolve_string(table, name_id).unwrap_or_default()
}

/// The client's prompt — the shipped row, with its variables.
///
/// The function has **two** arms, both built from the shipped rows rather than any prose of this
/// file's own.
///
/// With one conflict it is `ID_ActionKeyMap_OverwriteExistingBinding` (table enum `0x10000004`)
/// with `KEY` = the captured key's name and `ACTION` = the conflicting action's name. With more,
/// each conflict becomes one `ID_ActionKeyMap_Binding` line (`ACTION` = its action's name, `KEY`
/// = its own key's name) followed by `"\n"`, and the prompt is
/// `ID_ActionKeyMap_OverwriteExistingBindings` with `KEY` = the captured key's name and
/// `BINDINGS` = the joined lines.
///
/// `control` is the chord being bound — the key the player **just pressed**, named
/// exactly as a cell is, and *not* one of the conflicts' keys. The conflicts' own keys
/// appear only inside the `BINDINGS` list.
///
/// Every value goes in **by name** through [`UiSystem::resolve_string_named`] — the string's
/// meta-language arm, matching each value to the row's variables by
/// string hash (`dereth_primitives::num::hash::str_hash`) exactly as the client does. One `(ACTION, KEY)`
/// supply therefore serves both
/// `ID_ActionKeyMap_Binding` (which lists `ACTION, KEY`) and
/// `ID_ActionKeyMap_OverwriteExistingBinding` (which lists them the other way round), and a
/// localised dat that reorders either row still renders correctly; the order is the dat's, not
/// an assumption recorded on the [`token`] constants.
///
/// The client appends `"\n"` after **every** line, including the last, and
/// it is what puts the blank line before *"Do you wish to erase those bindings?"*.
#[must_use]
pub fn overwrite_prompt(
    ui: &UiSystem,
    m: &InputManager,
    control: &ControlChord,
    conflicts: &[Conflict],
    delimiter: &str,
) -> String {
    let table = string_table(ui, STRING_TABLE_ENUM);
    let key = binding_label(ui, m, control, delimiter);
    if conflicts.len() == 1 {
        let action = conflict_action_name(ui, m, &conflicts[0]);
        return ui
            .resolve_string_named(
                table,
                dereth_primitives::num::hash::str_hash(
                    token::OVERWRITE_EXISTING_BINDING.as_bytes(),
                ),
                &[(var::KEY, key.as_str()), (var::ACTION, action.as_str())],
            )
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_binding_line(&action, &key));
    }
    let mut bindings = String::new();
    for c in conflicts {
        let action = conflict_action_name(ui, m, c);
        let ckey = binding_label(ui, m, &c.control, delimiter);
        let line = ui
            .resolve_string_named(
                table,
                dereth_primitives::num::hash::str_hash(token::BINDING.as_bytes()),
                &[(var::ACTION, action.as_str()), (var::KEY, ckey.as_str())],
            )
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_binding_line(&action, &ckey));
        bindings.push_str(&line);
        bindings.push('\n');
    }
    ui.resolve_string_named(
        table,
        dereth_primitives::num::hash::str_hash(token::OVERWRITE_EXISTING_BINDINGS.as_bytes()),
        &[(var::KEY, key.as_str()), (var::BINDINGS, bindings.as_str())],
    )
    .filter(|s| !s.is_empty())
    .unwrap_or(bindings)
}

/// The client's prompt — [`token::NON_USER_BINDABLE_BINDING`]
/// with variable `KEY` set to the captured key's name, and **nothing else**: the refusal never
/// names the action it protects.
///
/// This build passed the *row's own label* here, so the notice read `"Move Forward"`.
#[must_use]
pub fn non_user_bindable_prompt(
    ui: &UiSystem,
    m: &InputManager,
    control: &ControlChord,
    delimiter: &str,
) -> String {
    let key = binding_label(ui, m, control, delimiter);
    ui.resolve_string_named(
        string_table(ui, STRING_TABLE_ENUM),
        dereth_primitives::num::hash::str_hash(token::NON_USER_BINDABLE_BINDING.as_bytes()),
        &[(var::KEY, key.as_str())],
    )
    .filter(|s| !s.is_empty())
    .unwrap_or(key)
}

/// What a host with **no** string table can say about one conflict: the two names and nothing
/// invented around them. It is not retail prose and is only reachable when the row is missing —
/// with the shipped `client_local_English.dat` installed, every call above resolves.
#[must_use]
fn fallback_binding_line(action: &str, key: &str) -> String {
    if action.is_empty() {
        return key.to_owned();
    }
    if key.is_empty() {
        return action.to_owned();
    }
    format!("{action} ({key})")
}

/// The client's prompt — `ID_ActionKeyMap_MapInstructions` out of table enum
/// [`STRING_TABLE_ENUM`] with variable `ACTION` set to the row's label.
///
/// The shipped row (`0x23000004`, measured) is two literal pieces around the one variable:
/// *"The next key you press or mouse button that you click will be mapped to the '"*, then
/// *"' action.  You may combine keys with SHIFT, CTRL or ALT … Press the ESC key to cancel."*
///
/// The value goes through the string read's meta-language arm
/// ([`UiSystem::resolve_string_named`]) instead of interleaving the pieces here, and **this line
/// shows why**: the shipped row's *"action.&nbsp;&nbsp;You may"* carries a double space, and the
/// client's `flags & 1` tail (trim excess spaces) collapses it. Retail draws one space;
/// interleaving would draw two.
///
/// Falls back to the bare action name when the table is absent, which is what a headless host with
/// no asset source can say.
#[must_use]
pub fn map_instructions(ui: &UiSystem, action_label: &str) -> String {
    let id = dereth_primitives::num::hash::str_hash(token::MAP_INSTRUCTIONS.as_bytes());
    ui.resolve_string_named(
        string_table(ui, STRING_TABLE_ENUM),
        id,
        &[(var::ACTION, action_label)],
    )
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| action_label.to_string())
}

/// The key buttons' element ids, from attribute [`attr::KEY_BUTTONS`].
///
/// The array's members each carry id [`attr::KEY_BUTTON_MEMBER`]; a member with any other id is
/// skipped rather than trusted, which is what makes a layout that changed shape produce a **short**
/// row rather than a wrong one.
#[must_use]
pub fn key_button_ids(ui: &UiSystem, h: ElemHandle) -> Vec<ElementId> {
    let Some(n) = ui.node(h) else {
        return Vec::new();
    };
    let Some(dereth_assets::ui::PropertyValue::Array(items)) =
        n.merged_properties().get(attr::KEY_BUTTONS).cloned()
    else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter(|p| p.id == attr::KEY_BUTTON_MEMBER)
        .filter_map(|p| match p.value {
            dereth_assets::ui::PropertyValue::Enum(v) => Some(ElementId(v)),
            _ => None,
        })
        .collect()
}

/// A literal string value, then the string-info write — how the client writes a row's action
/// name, and how [`ActionKeyMapRow::refresh`] writes a key button's caption once it has resolved
/// it through the string table.
pub fn set_literal(ui: &mut UiSystem, h: ElemHandle, text: &str) {
    let si = dereth_assets::ui::StringInfo {
        override_flag: 0,
        literal: Some(text.to_string()),
        string_id: None,
        table_id: None,
        is_adder: 0,
        adder: None,
        variables: Vec::new(),
    };
    let v = dereth_assets::ui::PropertyValue::StringInfo(Box::new(si));
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties
            .set(super::page::ATTR_STRING_INFO, v.clone());
    }
    ui.on_set_attribute(h, super::page::ATTR_STRING_INFO, Some(&v));
}

// ---------------------------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------------------------

/// What [`KeyBindingPage::on_page_element_message`] did — one variant per arm of
/// the keyboard panel's element-message handler, so a caller can assert on the arm rather
/// than on a side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    /// The *OK* button — `save_current_values` over `rows` rows, having raised
    /// [`crate::UiRequest::SaveKeyMap`] iff `saved` (changed).
    Applied { rows: usize, saved: bool },
    /// *Cancel* or *Revert to Saved* — `restore_saved_values` wrote this many rows.
    RestoredSaved(usize),
    /// *Restore Defaults* — `restore_default_values` wrote this many rows.
    RestoredDefaults(usize),
    /// The load-keymap dialog. Not raised by this build; recorded so the arm is
    /// attributed rather than silent.
    LoadKeymapDialog,
    /// The save keymap dialog build. As above.
    SaveKeymapDialog,
    /// A non-cancel answer the close load keymap dialog handling.
    LoadKeymap(String),
    /// A non-empty answer the close save keymap dialog handling.
    SaveKeymap(String),
    /// Yes; No produces no host event.
    OverwriteKeymap(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDialogKind {
    Load,
    Save,
    Overwrite,
    ReadOnly,
}

#[derive(Debug, Clone)]
struct FileDialogState {
    context: u64,
    kind: FileDialogKind,
    files: Vec<String>,
    selected: i32,
    name: Option<String>,
    popup: Option<ElemHandle>,
}

/// State `0x0D` — the disabled-state guard applies to
/// *OK* and *Cancel* and to nothing else on this page.
#[must_use]
fn button_enabled(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).is_some_and(|n| n.state != DISABLED_STATE)
}

/// The keyboard page (type `0x1000000E`) — six list boxes of key-binding rows.
#[derive(Debug, Clone, Default)]
pub struct KeyBindingPage {
    /// The keyboard-page element.
    pub page: Option<ElemHandle>,
    /// The list-box table — `(action class, the list box on that class's tab page)`.
    pub list_boxes: Vec<(u32, ListBoxWidget)>,
    /// The option rows, one entry per bindable `(input map, action)`.
    pub rows: Vec<ActionKeyMapRow>,
    /// How many template-0 section headers were added — one per `(action class, input map)` pair.
    pub headers: usize,
    /// Template-list inserts that produced nothing, and rows whose post-init failed.
    pub failures: usize,
    /// The Load Keymap / Save Keymap buttons.
    pub load_button: Option<ElemHandle>,
    pub save_button: Option<ElemHandle>,
    /// The keymap filename label, attribute [`page_attr::KEYMAP_FILENAME_LABEL`].
    pub filename_label: Option<ElemHandle>,
    /// The Reset-to-Defaults button — attribute
    /// [`page_attr::RESET_TO_DEFAULTS_BUTTON`].
    pub reset_defaults_button: Option<ElemHandle>,
    /// The Revert-to-Saved button.
    pub revert_to_saved_button: Option<ElemHandle>,
    /// The OK button.
    pub ok_button: Option<ElemHandle>,
    /// The Cancel button.
    pub cancel_button: Option<ElemHandle>,
    /// The delimiter [`binding_label`] joins with, resolved once from
    /// [`token::KEY_DESC_DELIMITER`] or [`DEFAULT_KEY_DESC_DELIMITER`].
    pub delimiter: String,
    /// The text element inside each template-0 section header, in the order
    /// [`Self::init_options`] added them.
    ///
    /// Kept so the titles can be read **off the arena** rather than off a cached string: if the
    /// caption written there were the `ID_InputMap_*` token itself, a test that asserted the value
    /// this page had computed would still pass.
    pub header_elements: Vec<ElemHandle>,
    file_dialog: Option<FileDialogState>,
}

impl KeyBindingPage {
    /// The mouse-turning settings' key half, offered to every row: the two camera-zoom rows take
    /// the mouse wheel and the rest do nothing. Returns the chat lines the two print.
    pub fn set_mouse_turning_defaults(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
    ) -> Vec<&'static str> {
        let delimiter = self.delimiter.clone();
        let mut lines = Vec::new();
        for row in &mut self.rows {
            lines.extend(row.set_mouse_turning_defaults(ui, m, &delimiter));
        }
        lines
    }

    /// Instantiate the pending dialog elements owned by this page's rows.
    ///
    /// `ActionKeyMapRow::open_dialog` creates the retail factory context and queue entry. The
    /// factory deliberately cannot create the element itself because it has no asset source;
    /// the element half belongs to the current framework, and this is it. Without it a click
    /// registers capture while putting nothing on screen.
    ///
    /// The same dialog-creation path serves the no-button map warning, the overwrite
    /// Confirmation and the can't-overwrite Message.
    pub(crate) fn service_dialog_elements(&mut self, ui: &mut UiSystem) -> Vec<ElemHandle> {
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        let mut contexts: Vec<u64> = self
            .rows
            .iter()
            .flat_map(|row| row.dialogs.iter().map(|(_, context)| *context))
            .filter(|context| owed.contains(context))
            .collect();
        if let Some(context) = self.file_dialog.as_ref().map(|dialog| dialog.context) {
            if owed.contains(&context) {
                contexts.push(context);
            }
        }
        let mut roots = Vec::new();
        for context in contexts {
            let Some(info) = ui.dialogs.info(context).cloned() else {
                continue;
            };
            let Ok(h) = ui.require_env().and_then(|e| {
                e.create_and_add_root_element(ui, DIALOG_LAYOUT, info.kind.root_element_id())
            }) else {
                continue;
            };
            ui.set_attribute_bool(
                h,
                dereth_ui::props::attr::DIALOG_MODAL,
                info.data
                    .get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                    .unwrap_or(false),
            );
            if let Some(dereth_assets::ui::PropertyValue::String(text)) =
                info.data.get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
            {
                if let Some(t) = ui
                    .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                    .and_then(|child| ui.text_element_mut(child))
                {
                    t.set_text(text);
                }
            }
            dereth_ui::dialog::types::set_dialog_data(ui, h, &info.data);
            if let Some(dialog) = self.file_dialog.as_mut().filter(|d| d.context == context) {
                if dialog.kind == FileDialogKind::Load {
                    let menu = ui
                        .get_child_recursive(h, dereth_ui::dialog::base::child::CONFIRM_MENU_MENU);
                    if let Some(menu) = menu {
                        let popup = ui.env().cloned().and_then(|e| {
                            e.with_assets(|assets| {
                                let popup = dereth_ui::widgets::menu::make_popup(ui, assets, menu)?;
                                dereth_ui::widgets::menu::initialize_popup(ui, menu)?;
                                for name in &dialog.files {
                                    dereth_ui::widgets::menu::add_text_item(
                                        ui, assets, menu, name,
                                    )?;
                                }
                                Some(popup)
                            })
                        });
                        if let Some(popup) = popup {
                            dereth_ui::widgets::menu::set_selected_index(ui, menu, dialog.selected);
                            dialog.popup = Some(popup);
                            roots.push(popup);
                        }
                    }
                }
            }
            dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
            if ui.bind_dialog_element(context, h) {
                roots.push(h);
            } else {
                ui.remove_and_delete_root(h);
            }
        }
        roots
    }

    /// The load-keymap dialog: kind 7, queue `0x10000001`, modal, with one menu row
    /// per `*.keymap` basename and the current file preselected.
    pub fn open_load_keymap_dialog(
        &mut self,
        ui: &mut UiSystem,
        files: Vec<String>,
        current: Option<&str>,
    ) -> bool {
        let selected = current
            .and_then(|name| {
                files
                    .iter()
                    .position(|file| file.eq_ignore_ascii_case(name))
            })
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1);
        self.open_file_dialog(ui, FileDialogKind::Load, files, selected, None)
    }

    /// The save-keymap dialog: kind 5 on the same modal queue.
    pub fn open_save_keymap_dialog(&mut self, ui: &mut UiSystem) -> bool {
        self.open_file_dialog(ui, FileDialogKind::Save, Vec::new(), -1, None)
    }

    /// The overwrite-keymap dialog: kind 1 Confirmation with `KEYMAP = name`.
    pub fn open_overwrite_keymap_dialog(&mut self, ui: &mut UiSystem, name: String) -> bool {
        self.open_file_dialog(ui, FileDialogKind::Overwrite, Vec::new(), -1, Some(name))
    }

    /// The cant overwrite read only keymap dialog build: kind 3 one-button Message.
    pub fn open_read_only_keymap_dialog(&mut self, ui: &mut UiSystem, name: String) -> bool {
        self.open_file_dialog(ui, FileDialogKind::ReadOnly, Vec::new(), -1, Some(name))
    }

    fn open_file_dialog(
        &mut self,
        ui: &mut UiSystem,
        kind: FileDialogKind,
        files: Vec<String>,
        selected: i32,
        name: Option<String>,
    ) -> bool {
        if self.file_dialog.is_some() || ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
            return false;
        }
        let (dialog_kind, prompt) = match kind {
            FileDialogKind::Load => (
                dereth_ui::dialog::DialogKind::ConfirmationMenu,
                token::LOAD_KEYMAP_LABEL,
            ),
            FileDialogKind::Save => (
                dereth_ui::dialog::DialogKind::ConfirmationTextInput,
                token::SAVE_KEYMAP_LABEL,
            ),
            FileDialogKind::Overwrite => (
                dereth_ui::dialog::DialogKind::Confirmation,
                token::OVERWRITE_KEYMAP_LABEL,
            ),
            FileDialogKind::ReadOnly => (
                dereth_ui::dialog::DialogKind::Message,
                token::CANT_OVERWRITE_READ_ONLY_KEYMAP_LABEL,
            ),
        };
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(dialog_kind.property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_QUEUE_ID,
            dereth_assets::ui::PropertyValue::Integer(
                i32::try_from(DIALOG_QUEUE).unwrap_or(i32::MAX),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            dereth_assets::ui::PropertyValue::Bool(true),
        );
        let prompt = match name.as_deref() {
            Some(name) => {
                let id = dereth_primitives::num::hash::str_hash(prompt.as_bytes());
                // The two prompts that take a file name — `ID_KeyMapOverwriteKeymap_Label` and
                // `ID_KeyMapCantOverwriteReadOnlyKeymap_Label` — both list `KEYMAP` [measured,
                // 0x23000004].
                ui.resolve_string_named(
                    string_table(ui, STRING_TABLE_ENUM),
                    id,
                    &[(var::KEYMAP, name)],
                )
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| name.to_owned())
            }
            None => {
                resolve_token(ui, STRING_TABLE_ENUM, prompt).unwrap_or_else(|| prompt.to_owned())
            }
        };
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(prompt),
        );
        if kind == FileDialogKind::Load {
            data.set(
                dereth_ui::props::attr::DIALOG_CONFIRM_MENU_ANSWER,
                dereth_assets::ui::PropertyValue::Integer(selected),
            );
        }
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        self.file_dialog = Some(FileDialogState {
            context,
            kind,
            files,
            selected,
            name,
            popup: None,
        });
        true
    }

    /// Harvest the client's kind-5/kind-7 answer arms.
    pub(crate) fn service_file_dialog_answer(&mut self, ui: &mut UiSystem) -> Option<PageEvent> {
        let state = self.file_dialog.as_ref()?;
        let root = ui
            .dialogs
            .info(state.context)
            .and_then(|info| info.element)?;
        let dialog = dereth_ui::dialog::types::dialog_element(ui, root)?;
        let answered = dialog.answer.is_some();
        let answer = dialog.answer_property();
        let event = match (state.kind, answer.map(|answer| answer.1)) {
            (FileDialogKind::Save, Some(dereth_assets::ui::PropertyValue::String(name)))
                if !name.is_empty() =>
            {
                Some(PageEvent::SaveKeymap(name))
            }
            (FileDialogKind::Load, Some(dereth_assets::ui::PropertyValue::Integer(index)))
                if index >= 0 =>
            {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| state.files.get(index).cloned())
                    .map(PageEvent::LoadKeymap)
            }
            (FileDialogKind::Overwrite, Some(dereth_assets::ui::PropertyValue::Bool(true))) => {
                state.name.clone().map(PageEvent::OverwriteKeymap)
            }
            _ => None,
        };
        if !answered {
            return None;
        }
        let state = self
            .file_dialog
            .take()
            .expect("the file-dialog state still exists");
        if let Some(root) = ui.dialogs.close_dialog(state.context, ui.now.0) {
            ui.remove_and_delete_root(root);
        }
        if let Some(popup) = state.popup {
            ui.remove_and_delete_root(popup);
        }
        event
    }

    /// Put the keymap file name on the page's filename label.
    pub fn refresh_keymap_file_name(&self, ui: &mut UiSystem, name: Option<&str>) {
        if let Some(label) = self.filename_label {
            set_literal(ui, label, name.unwrap_or_default());
        }
    }

    /// Deliver physical conflict-dialog answers to the client's row.
    ///
    /// The confirmation dialog records property `0x92`; the message dialog has no answer property and
    /// its one button is itself the completion. MapWarn is closed by the captured key path and is
    /// intentionally not harvested here.
    pub(crate) fn service_dialog_answers(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
    ) -> usize {
        let mut answers = Vec::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            for &(which, context) in &row.dialogs {
                let Some(root) = ui.dialogs.info(context).and_then(|info| info.element) else {
                    continue;
                };
                let Some(dialog) = dereth_ui::dialog::types::dialog_element(ui, root) else {
                    continue;
                };
                let answer = match which {
                    RowDialog::Overwrite => dialog.answer_property().and_then(|(_, value)| {
                        if let dereth_assets::ui::PropertyValue::Bool(answer) = value {
                            Some(answer)
                        } else {
                            None
                        }
                    }),
                    RowDialog::CantOverwrite => dialog.answer.is_some().then_some(false),
                    RowDialog::MapWarn => None,
                };
                if let Some(answer) = answer {
                    answers.push((row_index, context, which, answer));
                }
            }
        }
        let answered = answers.len();
        let delimiter = self.delimiter.clone();
        for (row_index, context, which, answer) in answers {
            if which == RowDialog::Overwrite {
                ui.dialogs.set_answer_property(
                    context,
                    dereth_ui::props::attr::DIALOG_ANSWER,
                    dereth_assets::ui::PropertyValue::Bool(answer),
                );
            }
            if self.rows[row_index].close_dialog(ui, m, context, answer, &delimiter) {
                self.on_option_changed(ui);
            }
        }
        // The client's Yes arm calls, whose last
        // act is the refresh notice. Without this the row that lost the key keeps drawing it.
        self.drain_refresh_notices(ui, m);
        answered
    }

    /// The keyboard panel's post-init.
    ///
    /// Reads [`page_attr::LIST_BOX_CHILD`] **once**, then walks the six tab pages of
    /// [`KEYBOARD_TAB_PAGES`] in the client's order, looking that child up inside each and
    /// registering it under one action class. Every page in the shipped tree carries the same
    /// child ids, which is exactly why the lookup is scoped to the page and not to the window:
    /// a recursive child search from the window would answer the first page six times.
    #[must_use]
    pub fn bind(ui: &UiSystem, page: ElemHandle) -> Self {
        let mut p = Self {
            page: Some(page),
            delimiter: DEFAULT_KEY_DESC_DELIMITER.to_string(),
            ..Self::default()
        };
        p.load_button = crate::bind::attr_enum(ui, page, page_attr::LOAD_KEYMAP_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.save_button = crate::bind::attr_enum(ui, page, page_attr::SAVE_KEYMAP_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.filename_label = crate::bind::attr_enum(ui, page, page_attr::KEYMAP_FILENAME_LABEL)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        // The four buttons the element-message handler compares against and that post-init
        // binds. Without them Apply, Cancel, *Restore Defaults* and *Revert to Saved* have nothing
        // to be compared against and the page cannot answer any of them.
        p.reset_defaults_button =
            crate::bind::attr_enum(ui, page, page_attr::RESET_TO_DEFAULTS_BUTTON)
                .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.revert_to_saved_button =
            crate::bind::attr_enum(ui, page, page_attr::REVERT_TO_SAVED_BUTTON)
                .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.ok_button = crate::bind::attr_enum(ui, page, page_attr::OK_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.cancel_button = crate::bind::attr_enum(ui, page, page_attr::CANCEL_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        // The client reads `ID_KeyDescDelimiter` out of string
        // table enum 3 at every call. Resolving it once here is the only deviation, and the value
        // it resolves to against the shipped dats is `"+"` — the same string
        // [`DEFAULT_KEY_DESC_DELIMITER`] falls back to, so a host with no asset source is not
        // silently joining `Shift` and `W` with nothing.
        if let Some(d) = resolve_token(ui, table_enum::KEY_DESC, token::KEY_DESC_DELIMITER) {
            p.delimiter = d;
        }
        let Some(child) = crate::bind::attr_enum(ui, page, page_attr::LIST_BOX_CHILD) else {
            return p;
        };
        for (tab, (class, _)) in KEYBOARD_TAB_PAGES.iter().zip(ACTION_CLASSES) {
            let Some(tab_h) = ui.get_child_recursive(page, *tab) else {
                continue;
            };
            let Some(lb) = ui.get_child_recursive(tab_h, ElementId(child)) else {
                continue;
            };
            p.list_boxes.push((class, ListBoxWidget::bind(ui, lb)));
        }
        p
    }

    /// The keyboard panel's option build.
    ///
    /// 1. Flush every list box and reset the option rows.
    /// 2. Walk the input maps, keeping only actions
    ///    accepts, and group them `action class -> input map -> [action]` in walk order.
    /// 3. For each group, add template 0 as the section header and set its text with
    ///    ([`super::pages::input_map_caption`]), then one
    ///    template-1 row per action through.
    ///
    /// The bindings each row starts with come from `InputManager::find_keys_for_action` on the
    /// **merged** master map — so the rows show what the
    /// player's keymap says and not what the shipped default says, and that is the acceptance's
    /// oracle.
    ///
    /// Returns how many rows were built.
    pub fn init_options(&mut self, ui: &mut UiSystem, m: &InputManager) -> usize {
        for (_, lb) in &mut self.list_boxes {
            lb.flush(ui);
        }
        self.rows.clear();
        self.headers = 0;
        self.failures = 0;
        self.header_elements.clear();
        // `action class -> [(input map, [action])]`, in input-map walk order.
        // `entries()` is the input maps' own walk order — map by map, action by action — which
        // is the order the option build fills its nested hash in and therefore the order the rows
        // appear in.
        //
        // The rows are the ones both interfaces' key pages list
        // ([`dereth_input::presentation`]): the bindable entries but the hidden quickslots and
        // the quest detail panel, with Disable Most Weather Effects among the character options.
        // A row this interface does nothing with is not listed.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        let mut groups: Vec<(u32, Vec<(InputMapId, Vec<ActionId>)>)> = Vec::new();
        for (map, action, v) in m.action_map.entries().collect::<Vec<_>>() {
            let Some(row) = presentation::find(map, action) else {
                continue;
            };
            let class = presentation::retail_class(map, action).unwrap_or(v.action_class);
            if row.not_used(Interface::Retail).is_some() {
                continue;
            }
            let ci = match groups.iter().position(|(c, _)| *c == class) {
                Some(i) => i,
                None => {
                    groups.push((class, Vec::new()));
                    groups.len() - 1
                }
            };
            let maps = &mut groups[ci].1;
            match maps.iter_mut().find(|(k, _)| *k == map) {
                Some((_, v)) => v.push(action),
                None => maps.push((map, vec![action])),
            }
        }
        for (class, maps) in groups {
            let Some(bi) = self.list_boxes.iter().position(|(c, _)| *c == class) else {
                continue;
            };
            for (map, actions) in maps {
                // Reading string info for an input-map id returns a string reference —
                // the hashed `ID_InputMap_…` token in table enum 7 — and the page's option
                // initialisation resolves it and sets it as the header text.
                // Writing the **token** onto the header instead would make a section title read
                // `ID_InputMap_MovementCommands`. An id with no switch arm leaves
                // the string reference empty, it is not valid and **no text is
                // set at all** — which is why the miss below writes nothing rather than a fallback.
                let caption = if map == dereth_input::dereth::INPUT_MAP {
                    // This client's own actions, under a heading of their own.
                    Some(dereth_input::dereth::SECTION_NAME.to_owned())
                } else {
                    super::pages::input_map_caption(map.0)
                        .and_then(|tok| resolve_token(ui, table_enum::INPUT_MAP, tok))
                };
                match self.list_boxes[bi]
                    .1
                    .add_from_template(ui, TEMPLATE_HEADER, None)
                {
                    Some(h) => {
                        if let Some(c) = caption {
                            set_literal(ui, h, &c);
                        }
                        self.header_elements.push(h);
                        self.headers += 1;
                    }
                    None => self.failures += 1,
                }
                for action in actions {
                    self.add_action_key_map(ui, m, bi, map, action);
                }
            }
            self.list_boxes[bi].1.update_layout(ui);
        }
        // Saving current values here is what makes `changed()` false on a freshly built page, and
        // therefore what makes *Revert to Saved* open greyed. Without it every row's saved list is
        // whatever initialisation left and the button's lit state is never computed at all.
        self.save_current_values();
        self.on_option_changed(ui);
        self.rows.len()
    }

    /// The **only** thing that lights or greys
    /// *Revert to Saved*.
    ///
    /// With no revert button nothing is written; otherwise the button gets state 1 when the page
    /// has changed and state `0x0D` when it has not.
    ///
    /// Two call sites reach this through the change handler installed during panel setup.
    /// This build owns that edge directly,
    /// so the page calls it after every gesture that
    /// can change a row instead; same edge, one indirection fewer. Returns the state written.
    pub fn on_option_changed(&mut self, ui: &mut UiSystem) -> Option<StateId> {
        let h = self.revert_to_saved_button?;
        let state = if self.changed() {
            ENABLED_STATE
        } else {
            DISABLED_STATE
        };
        ui.set_state(h, state);
        Some(state)
    }

    /// The keyboard panel's action key map insert — template 1, runtime type check,
    /// row initialisation, option registration.
    fn add_action_key_map(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        bi: usize,
        map: InputMapId,
        action: ActionId,
    ) {
        let Some(h) = self.list_boxes[bi]
            .1
            .add_from_template(ui, TEMPLATE_ROW, None)
        else {
            self.failures += 1;
            return;
        };
        // The runtime type check the insert performs before initialisation: a template whose root is not
        // an action-key-map option registers nothing, which is the loud form of "the layout
        // changed".
        if ui.node(h).map(|n| n.ty()) != Some(crate::element_types::ty::OPTION_ACTION_KEY_MAP) {
            self.failures += 1;
            return;
        }
        let Some(mut row) = ActionKeyMapRow::post_init(ui, h) else {
            self.failures += 1;
            return;
        };
        let (name_id, tip_id) = m.action_map.descrip_values(map, action);
        let table = dereth_primitives::DataId(m.action_map.string_table);
        // This client's own actions have no row in the string table: their names are its own;
        // so has Disable Most Weather Effects, which goes by its Character Options page's words.
        let (name, tip) = match (
            dereth_input::dereth::name(action),
            presentation::retail_caption(map, action),
        ) {
            (Some(own), _) if map == dereth_input::dereth::INPUT_MAP => {
                (own.to_owned(), String::new())
            }
            (_, Some(caption)) => (caption.to_owned(), String::new()),
            _ => (
                ui.resolve_string(table, name_id).unwrap_or_default(),
                ui.resolve_string(table, tip_id).unwrap_or_default(),
            ),
        };
        set_literal(ui, h, &name);
        // Initialization takes the defaults as its fifth argument and fills the current list
        // itself from `find_keys_for_action` on the *merged* map. The two lists are different
        // maps; using one for both would make *Restore Defaults* restore whatever the page was
        // opened with.
        let defaults = m.default_keys_for_action(action, map);
        row.init(
            action,
            map,
            name,
            tip,
            m.find_keys_for_action(action, map),
            defaults,
        );
        row.refresh(ui, m, &self.delimiter);
        self.rows.push(row);
    }

    /// Fan the element message out to the row that owns the element, which is
    /// the client's walk of the option rows.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
    ) -> Option<(usize, RowEvent)> {
        let d = self.delimiter.clone();
        for i in 0..self.rows.len() {
            if let Some(e) = self.rows[i].on_element_message(ui, m, msg, &d) {
                // The client's last two acts are the refresh-action-key-mapping notice for the
                // chord, then the change handler's option-changed call.
                self.drain_refresh_notices(ui, m);
                self.on_option_changed(ui);
                return Some((i, e));
            }
        }
        None
    }

    /// The refresh-action-key-mapping notice — every row re-reads the map.
    ///
    /// This is the **unkeyed** reload-options sweep. The notice the two binding call sites
    /// actually send carries a control and is
    /// handled by
    /// [`Self::send_notice_refresh_action_key_mapping`].
    pub fn on_refresh_action_key_mapping(&mut self, ui: &mut UiSystem, m: &InputManager) {
        let d = self.delimiter.clone();
        for r in &mut self.rows {
            r.refresh_mappings(ui, m, &d);
        }
    }

    /// The refresh-action-key-mapping notice, as far as this page is
    /// concerned: offer `control` to every row, and let
    /// [`ActionKeyMapRow::on_refresh_action_key_mapping`] decide which of them is
    /// showing it.
    ///
    /// The client walks the global notice-handler registry, skips engine handlers,
    /// and delivers the refresh to each option handler. Every live
    /// action-key-map row is one of those handlers, including the row that sent it.
    ///
    /// Returns how many rows redrew.
    pub fn send_notice_refresh_action_key_mapping(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        control: &ControlChord,
    ) -> usize {
        let d = self.delimiter.clone();
        let mut n = 0;
        for r in &mut self.rows {
            if r.on_refresh_action_key_mapping(ui, m, control, &d) {
                n += 1;
            }
        }
        n
    }

    /// Deliver every notice the rows raised during this gesture.
    ///
    /// Setting and erasing a binding send synchronously, before returning; the row
    /// cannot reach its siblings here, so it queues in
    /// [`ActionKeyMapRow::pending_refresh_notices`] and the page flushes that at the end of the
    /// same page-level call. The gesture that raised it and the redraw are one frame either way.
    ///
    /// Returns how many row redraws the notices caused.
    pub fn drain_refresh_notices(&mut self, ui: &mut UiSystem, m: &InputManager) -> usize {
        let mut pending: Vec<ControlChord> = Vec::new();
        for r in &mut self.rows {
            pending.append(&mut r.pending_refresh_notices);
        }
        let mut n = 0;
        for control in pending {
            n += self.send_notice_refresh_action_key_mapping(ui, m, &control);
        }
        n
    }

    /// The keyboard panel's restore-defaults operation — the
    /// *Restore Defaults* button.
    ///
    /// The override first clears the keymap and then performs two default-map additions, which
    /// rebuild the master input map from the shipped defaults; what reaches the rows is
    /// the option page's restore-defaults, an **unconditional** walk of
    /// the option array calling each option's restore-default operation. Every row,
    /// changed or not — the same shape `PlayerOptionPage`'s Defaults has.
    ///
    /// Returns how many rows were written, which is a denominator rather than a bare "it ran".
    ///
    /// The override's own two lines clear the keymap, then add maps `0x10000001` and `1`,
    /// i.e. [`InputManager::reload_defaults`]. Without them the walk below would write
    /// each row's default list back over the merged map and leave every binding the shipped maps
    /// do *not* mention exactly where the player had put it.
    pub fn restore_default_values(&mut self, ui: &mut UiSystem, m: &mut InputManager) -> usize {
        m.reload_defaults();
        let d = self.delimiter.clone();
        for r in &mut self.rows {
            r.restore_default_value(ui, m, &d);
        }
        self.rows.len()
    }

    /// The option page's save-current-values operation — the *OK* button's
    /// second half.
    ///
    /// A snapshot, not a write: each row copies its current list
    /// into its saved list, which is what gives Cancel something to revert to. Returns how many rows
    /// were snapshotted.
    pub fn save_current_values(&mut self) -> usize {
        for r in &mut self.rows {
            r.save_current_value();
        }
        self.rows.len()
    }

    /// The option page's restore-saved-values operation — *Cancel* and
    /// *Revert to Saved*, which invoke the same behavior from two different buttons.
    ///
    /// **Conditional, unlike Defaults**: the walk checks [`ActionKeyMapRow::changed`] first and
    /// only then restores the saved value, so a row that never moved is not rebound. Returns how
    /// many rows were written.
    pub fn restore_saved_values(&mut self, ui: &mut UiSystem, m: &mut InputManager) -> usize {
        let d = self.delimiter.clone();
        let mut n = 0;
        for r in &mut self.rows {
            if r.changed() {
                r.restore_saved_value(ui, m, &d);
                n += 1;
            }
        }
        n
    }

    /// *Any* row's [`ActionKeyMapRow::changed`], short-circuiting on the first.
    ///
    /// This is the flag the *OK* arm gates the keymap save on: an unchanged page is applied
    /// without rewriting the keymap file.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.rows.iter().any(ActionKeyMapRow::changed)
    }

    /// The **page's own** four buttons, as
    /// opposed to [`Self::on_element_message`]'s fan-out to the rows.
    ///
    /// Nothing happens while a dialog is open on queue `0x10000001`. Message 1 on Load Keymap or
    /// Save Keymap opens that dialog, on Reset to Defaults restores defaults, and on Revert to
    /// Saved restores the saved values. Message `0x19` with first parameter 7 on OK (not in state
    /// `0x0D`) saves the keymap file under its current name, without the overwrite prompt, if
    /// anything changed, then saves the current values; on Cancel (not in state `0x0D`) it
    /// restores the saved values.
    ///
    /// The asymmetry is the client's and is load-bearing: *Restore Defaults* and *Revert to Saved*
    /// answer message **1** (`BUTTON_CLICKED`), while *OK* and *Cancel* answer message **0x19**
    /// with first parameter 7 (`PRIMARY_CLICK`) and are additionally refused while their element is
    /// in state `0x0D`.
    ///
    /// The arm is raised as [`crate::UiRequest::SaveKeyMap`] rather than
    /// written here: the writer is the input manager's keymap save over the keymap file, which
    /// lives in the host (`dereth_client::input::InputShell::save_keymap`) and not in a `Screen`.
    /// The overwrite prompt the keymap save can raise belongs to the *Save Keymap As* path, which
    /// asks for it; this arm does not. The read-only refusal is not gated that way: retail checks
    /// that first on every save, so OK raises the can't-overwrite dialog too when the current
    /// keymap file exists and is not writable.
    pub fn on_page_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
    ) -> Option<PageEvent> {
        use dereth_ui::msg::element::id;
        if msg.id == id::BUTTON_CLICKED {
            if ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
                return None;
            }
            if Some(msg.source) == self.load_button {
                return Some(PageEvent::LoadKeymapDialog);
            }
            if Some(msg.source) == self.save_button {
                return Some(PageEvent::SaveKeymapDialog);
            }
            if Some(msg.source) == self.reset_defaults_button {
                let n = self.restore_default_values(ui, m);
                self.on_option_changed(ui);
                return Some(PageEvent::RestoredDefaults(n));
            }
            if Some(msg.source) == self.revert_to_saved_button {
                let n = self.restore_saved_values(ui, m);
                self.on_option_changed(ui);
                return Some(PageEvent::RestoredSaved(n));
            }
            return None;
        }
        if msg.id != dereth_ui::msg::element::id::MOUSE_CLICK
            || msg.p1 != dereth_ui::focus::action::PRIMARY_CLICK
        {
            return None;
        }
        if ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
            return None;
        }
        if Some(msg.source) == self.ok_button {
            if !button_enabled(ui, msg.source) {
                return None;
            }
            let changed = self.changed();
            if changed {
                ui.requests.emit(crate::UiRequest::SaveKeyMap);
            }
            let rows = self.save_current_values();
            self.on_option_changed(ui);
            return Some(PageEvent::Applied {
                rows,
                saved: changed,
            });
        }
        if Some(msg.source) == self.cancel_button {
            if !button_enabled(ui, msg.source) {
                return None;
            }
            let n = self.restore_saved_values(ui, m);
            self.on_option_changed(ui);
            return Some(PageEvent::RestoredSaved(n));
        }
        None
    }

    /// The input manager's key-hit handler call, routed to the row that asked for
    /// it, with the option-changed callback on the far side.
    ///
    /// The client reaches it through the row's registered key-hit handler, which is the *row's*
    /// own input handler; this build keeps one
    /// exclusive slot on the manager and finds the capturing row by asking, which is the same
    /// exclusivity. The option-changed notification half is here too, because the binding write
    /// calls it.
    pub fn key_hit(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        control: ControlChord,
    ) -> Option<Capture> {
        let i = self.rows.iter().position(ActionKeyMapRow::capturing)?;
        let d = self.delimiter.clone();
        let verdict = self.rows[i].key_hit(ui, m, control, &d);
        self.drain_refresh_notices(ui, m);
        self.on_option_changed(ui);
        Some(verdict)
    }

    /// The row that carries `element`, if any.
    #[must_use]
    pub fn row_for(&self, element: ElemHandle) -> Option<usize> {
        self.rows.iter().position(|r| r.element == Some(element))
    }

    /// The row bound to one `(input map, action)`.
    #[must_use]
    pub fn row_of(&self, map: InputMapId, action: ActionId) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.input_map == map && r.action == action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the shipped template `0x1000002F` of layout `0x21000009`, and
    /// The action key map option control's post-init / the refresh.
    ///
    /// **Literals, not symbols.** the stated testability rule: a test that reads a constant through the same
    /// symbol it writes it through cannot detect a wrong constant — and `0x1000002B` was labelled
    /// *the action id* in this crate and in the knowledge base until this unit read the layout.
    #[test]
    fn the_action_key_map_attributes_are_the_ones_the_client_reads() {
        assert_eq!(
            attr::CLEAR_BUTTON,
            0x1000_002A,
            "enum attribute -> the clear button"
        );
        assert_eq!(attr::KEY_BUTTONS, 0x1000_002B, "the key-button array");
        assert_eq!(
            attr::KEY_BUTTON_MEMBER,
            0x1000_002C,
            "each array member's property id"
        );
        assert_eq!(
            attr::BINDING_TEXT,
            0x49,
            "the string-info attribute Refresh writes"
        );
        assert_eq!(
            attr::CLEAR_DISABLED,
            0x0D,
            "the bool attribute on the clear button"
        );
        assert_eq!(
            DIALOG_QUEUE, 0x1000_0001,
            "the open-dialog check's queue, not the default 2"
        );
        assert_ne!(DIALOG_QUEUE, dereth_ui::dialog::factory::DEFAULT_QUEUE);
        assert_eq!(STRING_TABLE_ENUM, 0x1000_0004, "not 0x10000003");
        assert_eq!(DISABLED_STATE.0, 0x0D);
        assert_eq!(TEMPLATE_HEADER, 0);
        assert_eq!(TEMPLATE_ROW, 1);
        // The two message ids the row acts on, and the first parameter that makes the second one an
        // erase rather than a click.
        assert_eq!(dereth_ui::msg::element::id::MOUSE_CLICK.0, 0x19);
        assert_eq!(dereth_ui::focus::action::SECONDARY_CLICK, 8);
        assert_eq!(dereth_ui::msg::element::id::BUTTON_CLICKED.0, 1);
        // `DO_NOTHING` is the constant this whole unit turns on.
        assert_eq!(DO_NOTHING.0, 1, "action 1 is DoNothing");
    }

    /// Oracle: the client's six hard-coded page lookups, and
    /// the action classes.
    #[test]
    fn the_page_has_six_tab_pages_and_six_action_classes() {
        assert_eq!(KEYBOARD_TAB_PAGES.len(), ACTION_CLASSES.len());
        assert_eq!(
            KEYBOARD_TAB_PAGES.map(|e| e.0),
            [
                0x1000_049D,
                0x1000_049F,
                0x1000_04A1,
                0x1000_04A3,
                0x1000_0211,
                0x1000_04A5
            ]
        );
        // Page order: the client keys the six pages 1, 2, 4, 3, 7, 5.
        assert_eq!(ACTION_CLASSES.map(|(c, _)| c), [1, 2, 4, 3, 7, 5]);
        assert!(
            !ACTION_CLASSES.iter().any(|(c, _)| *c == 0),
            "class 0 is not user-bindable"
        );
        assert_eq!(page_attr::LIST_BOX_CHILD, 0x1000_0018);
        assert_eq!(page_attr::LOAD_KEYMAP_BUTTON, 0x1000_001E);
        assert_eq!(page_attr::SAVE_KEYMAP_BUTTON, 0x1000_001F);
        assert_eq!(page_attr::ALL.len(), 8);
        assert_eq!(page_attr::ALL[0], 0x1000_0018);
        assert_eq!(page_attr::ALL[7], 0x1000_001F);
    }

    /// The three dialogs' kinds, and the fact that the overwrite one is the only kind whose answer
    /// property is `0x92` — which is what reads.
    #[test]
    fn the_overwrite_dialog_is_the_kind_whose_answer_is_property_0x92() {
        use dereth_ui::dialog::DialogKind;
        assert_eq!(dialog_kind(RowDialog::Overwrite), DialogKind::Confirmation);
        assert_eq!(dialog_kind(RowDialog::CantOverwrite), DialogKind::Message);
        assert_eq!(dialog_kind(RowDialog::MapWarn), DialogKind::Wait);
        assert_eq!(DialogKind::Confirmation.answer_property(), Some(0x92));
        assert_eq!(DialogKind::Message.answer_property(), None);
        assert_eq!(DialogKind::Wait.answer_property(), None);
        assert_eq!(super::super::pages::DIALOG_ANSWER_PROPERTY, 0x92);
    }
}
