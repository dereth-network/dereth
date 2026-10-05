//! The prompt and instruction texts the rows and the page show.

use super::*;

/// The same action caption used by rows and overwrite prompts.
pub(super) fn action_description(
    ui: &UiSystem,
    m: &InputManager,
    map: InputMapId,
    action: ActionId,
) -> (String, String) {
    if map == dereth_input::dereth::INPUT_MAP {
        if let Some(name) = dereth_input::dereth::name(action) {
            return (name.to_owned(), String::new());
        }
    }
    if let Some(caption) = presentation::modern_caption(map, action) {
        return (caption.to_owned(), String::new());
    }
    let table = dereth_primitives::DataId(m.action_map.string_table);
    let (name, tip) = m.action_map.descrip_values(map, action);
    (
        ui.resolve_string(table, name).unwrap_or_default(),
        ui.resolve_string(table, tip).unwrap_or_default(),
    )
}

#[must_use]
fn conflict_action_name(ui: &UiSystem, m: &InputManager, c: &Conflict) -> String {
    action_description(ui, m, c.input_map, c.action).0
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
            .set(super::super::page::ATTR_STRING_INFO, v.clone());
    }
    ui.on_set_attribute(h, super::super::page::ATTR_STRING_INFO, Some(&v));
}
