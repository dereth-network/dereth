//! The base widgets used by the seven option control types, distinct from their values.
//!
//! Every game element type in this crate is registered with the same
//! [`crate::register_all`] factory, which produces a `PlainElement` — a widget with no behaviour
//! at all. For most of the 84 that is right, because the interesting state lives on the screen
//! that owns the subtree. For the seven option types it is **not**, because each of them
//! *derives from* a real engine widget and inherits its whole behaviour:
//!
//! | type | local label | widget behavior |
//! |---|---|---|
//! | `0x10000035` | `CheckboxOption` | `Button` — delegates remaining element messages to the button |
//! | `0x10000043` | `BitfieldCheckboxOption` | `CheckboxOption`, so also `Button`; checked by the control type test |
//! | `0x10000037` | `SliderOption` | `Scrollbar` — delegates remaining element messages to the scrollbar |
//! | `0x10000036` | `CheckboxSliderOption` | plain `Element` — delegates remaining element messages to the base element |
//! | `0x10000038` | `MenuOption` | `Menu` |
//! | `0x10000044` | `WideBitfieldCheckboxOption` | `ListBox` |
//! | `0x10000034` | `ActionKeyMapOption` | `TextElement` |
//!
//! **This is what makes the volume slider draggable.** `0x1000021C` in the shipped
//! `classic_options` row templates is type `0x10000037`; as a `PlainElement` it has no thumb, no
//! track, no `MouseDown` and no `0x0A` — the element draws and nothing else. As a
//! [`dereth_ui::widgets::scrollbar::Scrollbar`] it is the full engine scrollbar, and a
//! press-drag-release on its thumb raises element message `0x0A`
//! with the position, which is exactly the message the option-page slider's element-message
//! handler acts on.
//!
//! The option-specific halves — the value, the preference binding, the greying of a paired
//! slider — live in [`super::page`], with the page that owns them, for the same reason every other
//! panel's state does.

use dereth_ui::UiSystem;

use crate::element_types::ty;

/// Give the seven option types their base widgets.
///
/// Called from [`crate::register_all`] **after** the 84-row loop, so it overwrites those seven
/// entries; is a hash insert and the last
/// writer wins there too.
pub fn register(ui: &mut UiSystem) {
    ui.register_element_class(ty::OPTION_CHECKBOX, dereth_ui::widgets::button::create);
    ui.register_element_class(ty::OPTION_CB_BITFIELD, dereth_ui::widgets::button::create);
    ui.register_element_class(ty::OPTION_SLIDER, dereth_ui::widgets::scrollbar::create);
    ui.register_element_class(ty::OPTION_MENU, dereth_ui::widgets::menu::create);
    ui.register_element_class(
        ty::OPTION_CB_BITFIELD64,
        dereth_ui::widgets::listbox::create,
    );
    ui.register_element_class(
        ty::OPTION_ACTION_KEY_MAP,
        dereth_ui::text::element_text::create,
    );
    // `CheckboxSliderOption` is a plain `Element`; the 84-row loop already left it as one and
    // it is named here so the table above is complete rather than six rows and a silence.
}

/// The base each type is registered with, for a test that wants to state the table rather than
/// re-derive it. `None` is "plain `Element`".
pub const BASES: [(dereth_ui::ElementType, &str); 7] = [
    (ty::OPTION_CHECKBOX, "Button"),
    (ty::OPTION_CB_BITFIELD, "Button"),
    (ty::OPTION_SLIDER, "Scrollbar"),
    (ty::OPTION_CHECKBOX_SLIDER, "Element"),
    (ty::OPTION_MENU, "Menu"),
    (ty::OPTION_CB_BITFIELD64, "ListBox"),
    (ty::OPTION_ACTION_KEY_MAP, "TextElement"),
];
