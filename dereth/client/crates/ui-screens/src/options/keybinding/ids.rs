//! The page's element, attribute, string-table and state ids.

use super::*;

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
/// keymap load/save controls and the filename label. In the shipped `0x21000009` tree the
/// values are `0x10000025` (the list box) and `0x1000002C, 0x1000002D, 0x1000002A, 0x1000002B,
/// 0x10000028, 0x10000027, 0x10000029`.
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

/// The keyboard-page element in the shipped `0x21000005` tree — type `0x1000000E`.
///
/// Measured off the built tree, and pinned as a literal by the key-binding tests.
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
/// `0x10000003` the rest of the option pages use (`super::super::preferences::STRING_TABLE_ENUM`).
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
    pub const SUB_CONTROL: [&str; 6] = dereth_input::labels::SUB_CONTROL_TOKENS;
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
    /// A bound key button's caption wrapper, [`super::super::token::BUTTON_LABEL`].
    pub const LABEL: &str = "LABEL";
    /// The nested caption inside [`super::super::token::TT_EXISTING_BINDING`].
    pub const VALUE: &str = "VALUE";
    /// The keymap file name in the four `ID_KeyMap*_Label` dialog prompts.
    pub const KEYMAP: &str = "KEYMAP";
}

/// The `DidMapper` **group** every string-table enum lookup uses —
/// `super::super::preferences::STRING_TABLE_GROUP`, repeated here because this control resolves four
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
/// Fallback labels are used when a host has no asset source.
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
/// The same shape `super::super::preferences::table` has, and for the same reason.
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
