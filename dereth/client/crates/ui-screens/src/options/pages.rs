//! The other three option pages and the seven option control types.

use dereth_ui::{ElementId, ElementType};

use crate::element_types::ty;
use crate::view::PlayerOption;
use dereth_client_contract::options::sheet::Act;

// -------------------------------------------------------------------------------------------
// The seven control types
// -------------------------------------------------------------------------------------------

/// What an option control is bound to.
///
/// The preference and player-option binding setters are reached through the same call, so which
/// one a control uses is known only for controls created through the two toggle insertion paths.
/// Use those assignments and, for any check box created elsewhere, read the binding from the
/// layout property and **fail loudly** if neither is present — which is what [`OptionBinding::Unbound`] is
/// for: it is a state a page must never leave a control in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionBinding {
    /// A named `UserPreferences` entry.
    Preference(&'static str),
    /// A `PlayerOption` bit in the server-side blob.
    Player(PlayerOption),
    /// A gameplay-option property key, with its `BasePropertyType`.
    GameplayProperty { key: u32, chat: bool },
    /// Neither was recoverable. A control in this state is a bug, not a default.
    Unbound,
}

/// One option control type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionControl {
    pub class: &'static str,
    pub ty: ElementType,
    /// The child the control reads and writes, where there is one.
    ///
    /// **`SliderOption`'s pair is easy to swap.** The player-option page's slider insertion finds descendant `0x1000021C` and checks its slider type,
    /// while the label write starts at the parent, finds descendant `0x1000021B`,
    /// and updates its text — so
    /// `0x1000021C` is the control and `0x1000021B` is the row's caption. The shipped
    /// `0x2100002B` layout agrees: under template root `0x1000021A`, `0x1000021B` is type
    /// `0x0C` (text) and `0x1000021C` is type `0x10000037` (`SliderOption`).
    /// `CheckboxSliderOption`'s two children are `0x10000219` and `0x1000021C`.
    ///
    /// **`MenuOption`'s is the same shape** one row down: the player-option page's menu-option
    /// insert does
    /// a descendant lookup of `0x10000224` and a control type check, while its label write
    /// starts at the parent and finds descendant `0x10000223` before
    /// updating its text. The shipped template `0x10000222` has `0x10000223` at type
    /// `0x0C` and `0x10000224` at type `0x10000038`. Every row here is checked against the
    /// shipped layout by a dat-tier test.
    pub child: Option<ElementId>,
    /// The attribute it writes on that child.
    pub attribute: Option<u32>,
    /// The element message that means "the user changed me".
    pub message: Option<u32>,
}

const fn oc(
    class: &'static str,
    ty: ElementType,
    child: Option<u32>,
    attribute: Option<u32>,
    message: Option<u32>,
) -> OptionControl {
    OptionControl {
        class,
        ty,
        child: match child {
            Some(c) => Some(ElementId(c)),
            None => None,
        },
        attribute,
        message,
    }
}

/// The seven control types.
pub const OPTION_CONTROLS: [OptionControl; 7] = [
    // attribute 0x0E (checked) on the check-box child; element message 1 toggles.
    oc(
        "CheckboxOption",
        ty::OPTION_CHECKBOX,
        None,
        Some(0x0E),
        Some(1),
    ),
    // The control **is** the scrollbar: `SliderOption` derives from `Scrollbar` and
    // reads attribute `0x86` off itself. `0x1000021C` is that element; `0x1000021B` is the row's
    // name label. See the note on `OptionControl::child`.
    oc(
        "SliderOption",
        ty::OPTION_SLIDER,
        Some(0x1000_021C),
        Some(0x86),
        Some(0x0A),
    ),
    // children 0x10000219 (check box) and 0x1000021C (slider); the check box gates the slider.
    oc(
        "CheckboxSliderOption",
        ty::OPTION_CHECKBOX_SLIDER,
        Some(0x1000_0219),
        Some(0x0E),
        Some(1),
    ),
    // The control is `0x10000224`; `0x10000223` is the row's **name label**.
    // See the note on `OptionControl::child`.
    oc(
        "MenuOption",
        ty::OPTION_MENU,
        Some(0x1000_0224),
        Some(0x1000_0025),
        Some(7),
    ),
    oc(
        "BitfieldCheckboxOption",
        ty::OPTION_CB_BITFIELD,
        None,
        Some(0x0E),
        Some(1),
    ),
    // child 0x10000219; writes attribute 0x10000084 (the bit index) and 0x0E.
    oc(
        "WideBitfieldCheckboxOption",
        ty::OPTION_CB_BITFIELD64,
        Some(0x1000_0219),
        Some(0x1000_0084),
        Some(1),
    ),
    // element message 0x19 starts a binding, message 1 is a button on the row.
    oc(
        "ActionKeyMapOption",
        ty::OPTION_ACTION_KEY_MAP,
        None,
        Some(0x49),
        Some(0x19),
    ),
];

/// The property the dialog-close notice handler reads for the yes/no answer.
pub const DIALOG_ANSWER_PROPERTY: u32 = 0x92;

/// `PlayerOptionPage`'s row templates. The index is the template index the list box's
/// add-from-template-list takes.
///
/// **The two slider rows are easy to swap.** The slider option insert's template index is `(-wide & 3) + 3` with `wide` an unsigned 0
/// or 1, and `-1 == 0xFFFFFFFF`, so `wide` gives **6** and narrow gives **3**. The shipped layout
/// agrees independently: template 6 (`0x1000021D`) is the only slider row carrying the two end
/// captions the slider-label write fills, and the wide rows are exactly the ones this
/// page gives end captions to.
///
/// These are re-exported from [`super::page::template`], which is the copy the option build uses; the
/// two are asserted equal so the table cannot drift from the builder that reads it.
pub mod template {
    /// `add_header` (a string id).
    pub const HEADER: u32 = 0;
    /// `add_separator`.
    pub const SEPARATOR: u32 = 1;
    /// Both overloads of `add_toggle_option`.
    pub const TOGGLE: u32 = 2;
    /// `add_slider_option` with `wide = false` — `(-0 & 3) + 3`.
    pub const SLIDER_NARROW: u32 = 3;
    /// `add_menu_option`.
    pub const MENU: u32 = 4;
    /// `add_toggle_with_slider_option`.
    pub const TOGGLE_WITH_SLIDER: u32 = 5;
    /// `add_slider_option` with `wide = true` — `(-1 & 3) + 3`.
    pub const SLIDER_WIDE: u32 = 6;
}

// -------------------------------------------------------------------------------------------
// Chat Options — ChatOptionsPanel
// -------------------------------------------------------------------------------------------

/// The property the `WideBitfieldCheckboxOption` filter control edits, with the chat-type
/// enumeration.
pub const CHAT_FILTER_PROPERTY: u32 = 0x1000_007F;
/// The child every chat-options slider sits on.
pub const CHAT_OPTIONS_SLIDER_CHILD: ElementId = ElementId(0x1000_021C);

// -------------------------------------------------------------------------------------------
// Game / Support page — GameplayOptionsPanel
// -------------------------------------------------------------------------------------------

/// The title of the `MessageBoxA` shown when `ShellExecute` fails (result ≤ 32).
pub use dereth_client_contract::options::SHELL_EXECUTE_ERROR_TITLE;

/// The meaning of a Game / Support control. Keyboard and support actions are delivered by
/// the button's input action; the page answers the remaining actions itself. Help stays hidden.
#[must_use]
pub fn gameplay_option_action(element: ElementId) -> Option<Act> {
    Some(match element.0 {
        0x1000_0203 => Act::ExitToCharacterSelection,
        0x1000_0617 => Act::ExitGame,
        0x1000_0204 => Act::ConfigureKeyboard,
        0x1000_05CC => Act::MouseTurningSettings,
        0x1000_0206 => Act::UrgentAssistance,
        0x1000_0207 => Act::ReportAbuse,
        _ => return None,
    })
}

// -------------------------------------------------------------------------------------------
// Key bindings — KeyboardPanel
// -------------------------------------------------------------------------------------------

/// One input map and the section caption gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMapCaption {
    pub map_id: u32,
    pub caption: &'static str,
}

const fn imc(map_id: u32, caption: &'static str) -> InputMapCaption {
    InputMapCaption { map_id, caption }
}

/// The input maps the keybinding panel groups its rows by, and their captions.
///
/// Two binding details:
///
/// * the captions come from string table enum **7** (`0x23000005`), not `0x10000001`. Every one
///   of the nineteen input-map cases uses table enum `7`.
/// * `ID_InputMap_Combat` is **not** a fallback. Map `0x10000002` gets Combat only on an exact
///   match, like every other case. An index with
///   no case returns a **default-constructed `StringInfo`**. The option build then fails its
///   validity check and sets no text at all. So an unknown map id gets a blank header, not "Combat",
///   and `0x10000002` is an exact match like every other row.
pub const INPUT_MAP_CAPTIONS: [InputMapCaption; 19] = [
    imc(4, "ID_InputMap_MovementCommands"),
    imc(5, "ID_InputMap_CameraControls"),
    imc(6, "ID_InputMap_CameraAlternateControls"),
    imc(9, "ID_InputMap_DialogBoxes"),
    imc(0x0B, "ID_InputMap_DebugConsole"),
    imc(0x0C, "ID_InputMap_ProfilerUI"),
    imc(0x0D, "ID_InputMap_UIDebugger"),
    imc(0x0E, "ID_InputMap_DebugCommands"),
    imc(0x1000_0002, "ID_InputMap_Combat"),
    imc(0x1000_0003, "ID_InputMap_MeleeCombat"),
    imc(0x1000_0004, "ID_InputMap_MissileCombat"),
    imc(0x1000_0005, "ID_InputMap_MagicCombat"),
    imc(0x1000_0006, "ID_InputMap_Emotes"),
    imc(0x1000_0007, "ID_InputMap_ItemSelectionCommands"),
    imc(0x1000_0008, "ID_InputMap_CharacterOptionCommands"),
    imc(0x1000_0009, "ID_InputMap_UICommands"),
    imc(0x1000_000A, "ID_InputMap_ChatCommands"),
    imc(0x1000_000C, "ID_InputMap_QuickslotCommands"),
    imc(0x1000_000D, "ID_InputMap_ToggleChatEntry"),
];

/// The token, `None` when the switch has no arm.
///
/// The caller resolves it through string table enum 7; see
/// `super::keybinding::table_enum::INPUT_MAP`.
#[must_use]
pub fn input_map_caption(map_id: u32) -> Option<&'static str> {
    INPUT_MAP_CAPTIONS
        .iter()
        .find(|r| r.map_id == map_id)
        .map(|r| r.caption)
}

/// The action key map option control's post-init's two attributes.
///
/// **They are easy to mislabel** as *"attribute `0x1000002A` (the id of the child that displays
/// the binding) … then attribute `0x1000002B` (the action id)"*. The shipped row template
/// settles it: `0x1000002F` of layout
/// `0x21000009` carries `0x1000002B` as an **Array** of three `0x1000002C` enums naming its three
/// children `0x10000030`/`0x10000031`/`0x10000032` — the key buttons — and carries no
/// `0x1000002A` at all. Initialization reads enum attribute `0x1000002A` and checks that the
/// referenced element is a button: this is the clear button, whose disabled attribute
/// `0x0D` is set when the current binding list is empty. The action id
/// is not an attribute at all: the keyboard panel's action-key-map insert passes it to
/// the action key map option control's initialization.
///
/// With the numbers right and the names wrong nothing would catch the mistake, so the names
/// matter. This module is the one copy — [`super::keybinding::attr`] re-states the same values
/// with the same names and the assertions live there.
pub mod action_key_map {
    /// Enum attribute `0x1000002A` names the clear button, **not** the
    /// display child.
    pub const CLEAR_BUTTON_ATTR: u32 = super::super::keybinding::attr::CLEAR_BUTTON;
    /// The **array** of key-button element ids, **not** the action id.
    pub const KEY_BUTTONS_ATTR: u32 = super::super::keybinding::attr::KEY_BUTTONS;
    /// The id every member of that array carries.
    pub const KEY_BUTTON_MEMBER_ATTR: u32 = super::super::keybinding::attr::KEY_BUTTON_MEMBER;
    /// The attribute writes the binding text into.
    pub const BINDING_TEXT_ATTR: u32 = super::super::keybinding::attr::BINDING_TEXT;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the type ids, the child
    /// each control reads and the attribute it writes.
    #[test]
    fn the_seven_option_controls_are_the_documented_types_and_children() {
        assert_eq!(OPTION_CONTROLS.len(), 7);
        let by = |c: &str| OPTION_CONTROLS.iter().find(|o| o.class == c).unwrap();
        assert_eq!(by("CheckboxOption").ty, ty::OPTION_CHECKBOX);
        assert_eq!(by("CheckboxOption").attribute, Some(0x0E));
        assert_eq!(by("SliderOption").child, Some(ElementId(0x1000_021C)));
        assert_eq!(by("SliderOption").attribute, Some(0x86));
        assert_eq!(by("SliderOption").message, Some(0x0A));
        assert_eq!(
            by("CheckboxSliderOption").child,
            Some(ElementId(0x1000_0219))
        );
        assert_eq!(by("MenuOption").child, Some(ElementId(0x1000_0224)));
        assert_eq!(by("MenuOption").attribute, Some(0x1000_0025));
        assert_eq!(by("MenuOption").message, Some(7));
        assert_eq!(
            by("WideBitfieldCheckboxOption").attribute,
            Some(0x1000_0084)
        );
        assert_eq!(by("ActionKeyMapOption").message, Some(0x19));
        // Every one is a registered game element type.
        for o in OPTION_CONTROLS {
            assert!(crate::element_types::REGISTRATION_ORDER
                .iter()
                .any(|r| r.ty == o.ty));
        }
    }

    /// Chat options have six sections with the expected window ids.
    #[test]
    fn the_chat_options_page_edits_the_five_windows_filters_in_order() {
        use dereth_client_contract::options::interface::Interface;
        use dereth_client_contract::options::sheet::{self, PageId, Value};
        let headings = sheet::headings_for(PageId::Chat, Interface::Modern).collect::<Vec<_>>();
        assert_eq!(headings.len(), 6);
        assert!(
            headings[0]
                .1
                .iter()
                .all(|r| !matches!(r.value, Value::Filter { .. })),
            "the general section edits opacity"
        );
        let ids = headings[1..]
            .iter()
            .map(|(_, rows)| {
                let Value::Filter { window, .. } = rows[0].value else {
                    panic!("filter section")
                };
                assert!(rows
                    .iter()
                    .all(|r| matches!(r.value, Value::Filter { window: w, .. } if w == window)));
                window
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![8, 2, 3, 4, 5], "main, then floaty 1..4");
        for (id, want) in ids.into_iter().zip([
            0xFBFF_FFFF_u64,
            0x0000_101C,
            0x0004_0C00,
            0x0008_0000,
            0x7800_0000,
        ]) {
            assert_eq!(crate::chat::interface::default_filter(id), want, "{id}");
        }
        let opacity = headings[0]
            .1
            .iter()
            .filter_map(|r| match r.value {
                Value::Opacity(p) => Some(p),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(opacity, [0x1000_0080, 0x1000_0081]);
        assert_eq!(CHAT_FILTER_PROPERTY, 0x1000_007F);
    }

    /// Every visible control has its shared meaning; hidden Help has none.
    #[test]
    fn the_game_support_page_maps_visible_buttons_to_their_shared_actions() {
        for (id, action) in [
            (0x1000_0203, Act::ExitToCharacterSelection),
            (0x1000_0617, Act::ExitGame),
            (0x1000_0204, Act::ConfigureKeyboard),
            (0x1000_05CC, Act::MouseTurningSettings),
            (0x1000_0206, Act::UrgentAssistance),
            (0x1000_0207, Act::ReportAbuse),
        ] {
            assert_eq!(gameplay_option_action(ElementId(id)), Some(action));
        }
        assert_eq!(gameplay_option_action(ElementId(0x1000_0205)), None);
        assert_eq!(gameplay_option_action(ElementId(0xDEAD)), None);
    }

    /// The key binding page captions every input map it knows.
    #[test]
    fn the_key_binding_page_captions_every_input_map_it_knows() {
        assert_eq!(INPUT_MAP_CAPTIONS.len(), 19);
        assert_eq!(input_map_caption(4), Some("ID_InputMap_MovementCommands"));
        assert_eq!(input_map_caption(0x0E), Some("ID_InputMap_DebugCommands"));
        assert_eq!(
            input_map_caption(0x1000_000D),
            Some("ID_InputMap_ToggleChatEntry")
        );
        // An exact match, the only id that reaches the Combat arm.
        assert_eq!(input_map_caption(0x1000_0002), Some("ID_InputMap_Combat"));
        // Past the end of the second switch table, so no string at all.
        assert_eq!(input_map_caption(0x1000_00FF), None);
        // Entry 8 is inside the dispatch table but resolves to its default arm.
        assert_eq!(input_map_caption(0x1000_000B), None);
        // 7, 8 and 0x0A occupy switch-table entries `[3]`, `[4]`, and `[6]`.
        assert_eq!(input_map_caption(7), None);
        assert_eq!(input_map_caption(8), None);
        assert_eq!(input_map_caption(0x0A), None);
    }

    /// The row templates are the indices the helpers pass.
    #[test]
    fn the_row_templates_are_the_indices_the_helpers_pass() {
        assert_eq!(template::HEADER, 0);
        assert_eq!(template::SEPARATOR, 1);
        assert_eq!(template::TOGGLE, 2);
        assert_eq!(template::SLIDER_NARROW, 3);
        assert_eq!(template::MENU, 4);
        assert_eq!(template::TOGGLE_WITH_SLIDER, 5);
        assert_eq!(template::SLIDER_WIDE, 6);
        // The builder's copy and this one are the same table.
        use crate::options::page::template as t;
        assert_eq!(
            [
                template::HEADER,
                template::SEPARATOR,
                template::TOGGLE,
                template::SLIDER_NARROW,
                template::MENU,
                template::TOGGLE_WITH_SLIDER,
                template::SLIDER_WIDE
            ]
            .map(|v| v as usize),
            [
                t::HEADER,
                t::SEPARATOR,
                t::TOGGLE,
                t::SLIDER_NARROW,
                t::MENU,
                t::TOGGLE_WITH_SLIDER,
                t::SLIDER_WIDE
            ]
        );
    }

    /// The action key map control reads the documented attributes.
    #[test]
    fn the_action_key_map_control_reads_the_documented_attributes() {
        assert_eq!(action_key_map::CLEAR_BUTTON_ATTR, 0x1000_002A);
        assert_eq!(action_key_map::KEY_BUTTONS_ATTR, 0x1000_002B);
        assert_eq!(action_key_map::KEY_BUTTON_MEMBER_ATTR, 0x1000_002C);
        assert_eq!(action_key_map::BINDING_TEXT_ATTR, 0x49);
    }
}
