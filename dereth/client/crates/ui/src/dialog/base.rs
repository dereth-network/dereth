//! The `Dialog` base and the seven kinds.
//!
//! Shared dialog state, queues, and answer handling.
//!
//! The original dialog combines field and shared element behavior. Its whole behavior is to read two properties out of the
//! caller's collection (modal and timeout), tick a countdown on global message 3, re-centre itself
//! on element message 0x24, and write the pressed button's element id back into **the same
//! `PropertyCollection` the caller supplied**, under property 0x92.

use crate::{ElementId, ElementType};

/// The seven dialog kinds, keyed by property **0x8E** (1..=7) during dialog creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// 0x8E = 1, element type 0x13. Yes/no confirmation with two buttons.
    Confirmation,
    /// 0x8E = 2, element type 0x19. Modal "please wait" with no buttons; also registers for global
    /// message 1 so `Esc` can dismiss it.
    Wait,
    /// 0x8E = 3, element type 0x17. One-button message box.
    Message,
    /// 0x8E = 4, element type 0x18. Prompt for a string.
    TextInput,
    /// 0x8E = 5, element type 0x15. Confirmation plus a text field.
    ConfirmationTextInput,
    /// 0x8E = 6, element type 0x16. Pick one item from a list.
    Menu,
    /// 0x8E = 7, element type 0x14. Confirmation plus a drop-down of choices.
    ConfirmationMenu,
}

impl DialogKind {
    /// Property 0x8E → kind.
    #[must_use]
    pub const fn from_property(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Confirmation,
            2 => Self::Wait,
            3 => Self::Message,
            4 => Self::TextInput,
            5 => Self::ConfirmationTextInput,
            6 => Self::Menu,
            7 => Self::ConfirmationMenu,
            _ => return None,
        })
    }

    /// Property **0x8E**'s value for this kind — the inverse of [`Self::from_property`], and what
    /// each dialog-making call stores as an enum property.
    #[must_use]
    pub const fn property(self) -> i32 {
        match self {
            Self::Confirmation => 1,
            Self::Wait => 2,
            Self::Message => 3,
            Self::TextInput => 4,
            Self::ConfirmationTextInput => 5,
            Self::Menu => 6,
            Self::ConfirmationMenu => 7,
        }
    }

    // The element type instantiated for this dialog kind.

    #[must_use]
    pub const fn element_type(self) -> ElementType {
        ElementType(match self {
            Self::Confirmation => 0x13,
            Self::ConfirmationMenu => 0x14,
            Self::ConfirmationTextInput => 0x15,
            Self::Menu => 0x16,
            Self::Message => 0x17,
            Self::TextInput => 0x18,
            Self::Wait => 0x19,
        })
    }

    /// The **element id inside the `Dialog` layout** that `create_and_add_root_element` is asked
    /// for.
    ///
    /// Earlier notes called these "layout enums" but could not resolve them to DataIDs.
    /// They are **not** layout enums: they
    /// are root element ids inside layout enum 2 (`Dialog` = `0x2100003C`). Dumping that layout
    /// shows six roots whose ids and element types are exactly
    ///
    /// | root id | type | kind |
    /// |---:|---:|---|
    /// | 0x15 | 0x13 | `Confirmation` |
    /// | 0x1B | 0x16 | `Menu` |
    /// | 0x1F | 0x14 | `ConfirmationMenu` |
    /// | 0x24 | 0x17 | `Message` |
    /// | 0x2C | 0x15 | `ConfirmationTextInput` |
    /// | 0x31 | 0x19 | `Wait` |
    ///
    /// — six of the seven documented pairs, each matching its type. The seventh, **0x28**
    /// (`TextInput`, 0x18), has no root in any of the 101 shipped layouts, so a `TextInput`
    /// dialog cannot be created in this build because no shipped layout supplies its root.
    #[must_use]
    pub const fn root_element_id(self) -> ElementId {
        ElementId(match self {
            Self::Confirmation => 0x15,
            Self::Menu => 0x1B,
            Self::ConfirmationMenu => 0x1F,
            Self::Message => 0x24,
            Self::TextInput => 0x28,
            Self::ConfirmationTextInput => 0x2C,
            Self::Wait => 0x31,
        })
    }

    /// The two children whose element message **1** this subclass treats as an answer, as an
    /// ordered **(accept, cancel)** pair. `None` where the subclass has no such button.
    ///
    /// **Each kind has its own pair.** Only the confirmation dialog sends `0x17`/`0x19`; a
    /// `ConfirmationTextInput` (the char-select DELETE warning) watching those would never record
    /// an answer at all. Each row below is the source id that subclass's own
    /// element-message handler answers to:
    ///
    /// | kind | accept | cancel | note |
    /// |---|---:|---:|---|
    /// | `Confirmation` | 0x17 | 0x19 | its own handler |
    /// | `ConfirmationMenu` | 0x22 | 0x23 | its own handler |
    /// | `ConfirmationTextInput` | 0x2E | 0x2F | its own handler |
    /// | `Menu` | 0x1E | — | its own handler |
    /// | `Message` | 0x26 | — | its own handler |
    /// | `TextInput` | 0x2A | — | its own handler |
    /// | `Wait` | — | — | defers entirely to the base dialog's handler |
    ///
    /// The order matters and is not cosmetic: *accept* and *cancel* run different arms in every
    /// two-button subclass — `0x2E` harvests the box's text into `0x9C` while `0x2F` writes an
    /// empty one, and `0x22` reads the menu selection while `0x23` leaves it at `-1`. Transposing
    /// a row would delete a character on *Cancel*. \[verified\]
    #[must_use]
    pub const fn answer_children(self) -> (Option<ElementId>, Option<ElementId>) {
        match self {
            Self::Confirmation => (Some(child::BUTTON1), Some(child::BUTTON2)),
            Self::ConfirmationMenu => (
                Some(child::CONFIRM_MENU_ACCEPT),
                Some(child::CONFIRM_MENU_CANCEL),
            ),
            Self::ConfirmationTextInput => (
                Some(child::CONFIRM_TEXT_INPUT_ACCEPT),
                Some(child::CONFIRM_TEXT_INPUT_CANCEL),
            ),
            Self::Menu => (Some(child::MENU_BUTTON), None),
            Self::Message => (Some(child::MESSAGE_BUTTON), None),
            Self::TextInput => (Some(child::TEXT_INPUT_BUTTON), None),
            Self::Wait => (None, None),
        }
    }

    /// Which child id, if any, is this kind's *accept* or *cancel*.
    #[must_use]
    pub fn answer_role(self, id: ElementId) -> Option<AnswerRole> {
        let (accept, cancel) = self.answer_children();
        if accept == Some(id) {
            Some(AnswerRole::Accept)
        } else if cancel == Some(id) {
            Some(AnswerRole::Cancel)
        } else {
            None
        }
    }

    /// The property each subclass writes its answer into, in the caller's own collection.
    ///
    /// It is **not** `0x92` for anything but `Confirmation`: a caller reading `0x92` off a
    /// `ConfirmationTextInput` reads nothing.
    ///
    /// | kind | property | what it holds |
    /// |---|---:|---|
    /// | `Confirmation` | 0x92 | Boolean: button0x17=true, button0x19=false |
    /// | `ConfirmationMenu` | 0xAB | the menu index, or `-1` on cancel |
    /// | `ConfirmationTextInput` | 0x9C | the typed string, empty on cancel |
    /// | `Menu` | 0xA4 | the menu index |
    /// | `TextInput` | 0x98 | the typed string, empty on cancel |
    /// | `Message`, `Wait` | — | neither writes anything; they only close the dialog |
    ///
    /// [verified — each subclass writes the property before closing or cancelling]
    #[must_use]
    pub const fn answer_property(self) -> Option<u32> {
        use crate::props::attr;
        Some(match self {
            Self::Confirmation => attr::DIALOG_ANSWER,
            Self::ConfirmationMenu => attr::DIALOG_CONFIRM_MENU_ANSWER,
            Self::ConfirmationTextInput => attr::DIALOG_TEXT_INPUT_ANSWER_TEXT,
            Self::Menu => attr::DIALOG_MENU_ANSWER,
            Self::TextInput => attr::DIALOG_TEXT_INPUT_ANSWER,
            Self::Message | Self::Wait => return None,
        })
    }

    /// The menu child from which each menu-bearing dialog reads its answer. The original two
    /// dialog kinds store that child in their own fields; this enum records child IDs.
    ///
    /// | kind | child | note |
    /// |---|---:|---|
    /// | `Menu` | 0x1D | its `set_data` fetches the child by that id |
    /// | `ConfirmationMenu` | 0x21 | the same, with its own id |
    ///
    /// Both original lookup paths require element type **6**, the menu type,
    /// and both resolve, in the shipped `Dialog` layout `0x2100003C`, to a child whose base
    /// element is `0x1000035B` of layout `0x21000043`, a real type-6 menu 150 × 18. So the
    /// answer's holder is not missing from the *data*. Until the menu has rows,
    /// [`crate::UiSystem::menu_selected_index`] on a dialog raised
    /// from the shipped layout answers **-1** — which is the selected-index query's own
    /// answer for a menu with no list box, not a stand-in for one. \[verified\]
    #[must_use]
    pub const fn menu_child(self) -> Option<ElementId> {
        Some(match self {
            Self::Menu => child::MENU_MENU,
            Self::ConfirmationMenu => child::CONFIRM_MENU_MENU,
            _ => return None,
        })
    }

    // The edit box whose contents the *accept* arm harvests, for the two kinds that have one.

    ///
    /// The confirmation-text-input dialog's handler looks up descendant `0x2C` as a text element
    /// (type `0xC`) and the text-input dialog does the same with `0x2B`. A `None` here is a kind with no text to harvest. \[verified\]
    #[must_use]
    pub const fn text_child(self) -> Option<ElementId> {
        Some(match self {
            Self::ConfirmationTextInput => child::CONFIRM_TEXT_INPUT_BOX,
            Self::TextInput => child::TEXT_INPUT_BOX,
            _ => return None,
        })
    }

    /// The **(accept, cancel)** button *caption* properties, paired with
    /// [`Self::answer_children`]'s ids — each subclass's `set_data` reads these out of the caller's
    /// collection and sets them as its own buttons' text.
    ///
    /// | kind | accept caption | cancel caption |
    /// |---|---:|---:|
    /// | `Confirmation` | 0x90 | 0x91 |
    /// | `ConfirmationMenu` | 0xA8 | 0xA9 |
    /// | `ConfirmationTextInput` | 0x9A | 0x9B |
    /// | `Menu` | 0xA2 | — |
    /// | `Message` | 0x95 | — |
    /// | `TextInput` | 0x97 | — |
    /// | `Wait` | — | — | its `set_data` reads only `0x9E` and it has no buttons |
    ///
    /// Without this every dialog's buttons would carry whatever caption the shipped layout gave
    /// them. [verified — the property lookup before each child lookup]
    #[must_use]
    pub const fn caption_properties(self) -> (Option<u32>, Option<u32>) {
        use crate::props::attr;
        match self {
            Self::Confirmation => (Some(attr::DIALOG_BUTTON1), Some(attr::DIALOG_BUTTON2)),
            Self::ConfirmationMenu => (
                Some(attr::DIALOG_CONFIRM_MENU_ACCEPT_CAPTION),
                Some(attr::DIALOG_CONFIRM_MENU_CANCEL_CAPTION),
            ),
            Self::ConfirmationTextInput => (
                Some(attr::DIALOG_TEXT_INPUT_ACCEPT_CAPTION),
                Some(attr::DIALOG_TEXT_INPUT_CANCEL_CAPTION),
            ),
            Self::Menu => (Some(attr::DIALOG_MENU_BUTTON), None),
            Self::Message => (Some(attr::DIALOG_MESSAGE_BUTTON), None),
            Self::TextInput => (Some(attr::DIALOG_TEXT_INPUT_BUTTON), None),
            Self::Wait => (None, None),
        }
    }
}

/// Which arm of a two-button dialog was pressed.
///
/// The distinction is behavioural, not decorative: the confirmation-text-input dialog's
/// [`AnswerRole::Accept`] harvests the box into property `0x9C` and its [`AnswerRole::Cancel`]
/// writes an **empty** one, which is the only thing separating *the player typed nothing* from
/// *the player changed their mind* — and, on the char-select DELETE dialog, the only thing
/// separating a deleted character from a live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerRole {
    Accept,
    Cancel,
}

/// Well-known child element ids inside a dialog layout.
///
/// **`BUTTON1`/`BUTTON2` belong to the confirmation dialog and to nothing else**, though they are
/// easy to read as though every dialog used them: no subclass but the confirmation dialog ever
/// sends either. Each subclass's own ids are below and each was verified against retail; the table
/// that
/// selects between them is [`DialogKind::answer_children`].
pub mod child {
    use crate::ElementId;
    /// Button 1 of a **confirmation dialog**; its caption comes from property 0x90, and its own
    /// message handler compares the source id against `0x17`.
    pub const BUTTON1: ElementId = ElementId(0x17);
    /// Button 2 of a **confirmation dialog**; caption from property 0x91.
    /// Same handler, comparing against `0x19`.
    pub const BUTTON2: ElementId = ElementId(0x19);

    /// The menu dialog's single button — its own message handler compares the source id against
    /// `0x1E`. \[verified\]
    pub const MENU_BUTTON: ElementId = ElementId(0x1E);
    /// **The drop-down from which a menu dialog reads its answer.**
    ///
    /// The menu dialog's `set_data` fetches the child `0x1D`,
    /// then casts it to type **6**, which is `Menu`. The
    /// shipped `Dialog` layout `0x2100003C` agrees: root `0x1B`'s panel `0x3D` holds a button
    /// strip whose two children are `0x1D` (150 × 18, base element `0x1000035B` in layout
    /// `0x21000043`, which **is** a type-6 `Menu`) and `0x1E`, the button above.
    /// \[verified\]
    pub const MENU_MENU: ElementId = ElementId(0x1D);

    /// **The drop-down from which a confirmation-menu dialog reads its answer.**
    ///
    /// The confirmation-menu dialog's `set_data` fetches the child `0x21`
    /// and casts it to type 6. In layout `0x2100003C`, root
    /// `0x1F`'s strip holds `0x21` (150 × 18, base `0x1000035B`, a `Menu`), `0x22` and
    /// `0x23` — the menu and its two buttons. \[verified\]
    pub const CONFIRM_MENU_MENU: ElementId = ElementId(0x21);
    /// The confirmation-menu dialog's accept button — its handler compares against `0x22`.
    /// It is also the arm that reads the menu's selected index. \[verified\]
    pub const CONFIRM_MENU_ACCEPT: ElementId = ElementId(0x22);

    /// Its cancel button — the same handler, comparing against `0x23`. \[verified\]
    pub const CONFIRM_MENU_CANCEL: ElementId = ElementId(0x23);
    /// The message dialog's single button — its handler compares against `0x26`.
    /// \[verified\]
    pub const MESSAGE_BUTTON: ElementId = ElementId(0x26);
    /// The text-input dialog's single button — its handler compares against `0x2A`.
    /// \[verified\]
    pub const TEXT_INPUT_BUTTON: ElementId = ElementId(0x2A);
    /// The box the text-input dialog harvests — its handler fetches the child `0x2B`.
    /// \[verified\]
    pub const TEXT_INPUT_BOX: ElementId = ElementId(0x2B);
    /// The box the confirmation-text-input dialog harvests — its handler fetches the child
    /// `0x2C`.
    ///
    /// It shares its id with the confirmation-text-input dialog's **root** (`0x2C`, see
    /// `DialogKind::root_element_id`), which is not a mistake: `get_child_recursive` starts
    /// *below*
    /// the receiver, so the walk finds the descendant and never the receiver itself.
    /// \[verified\]
    pub const CONFIRM_TEXT_INPUT_BOX: ElementId = ElementId(0x2C);
    /// The confirmation-text-input dialog's *Done* — its handler compares against `0x2E`.
    /// This is the arm that harvests the box into property `0x9C`.
    /// \[verified\]
    pub const CONFIRM_TEXT_INPUT_ACCEPT: ElementId = ElementId(0x2E);
    /// Its *Cancel* — the same handler's other arm, which cancels the dialog, and so
    /// writes an **empty** `0x9C`. \[verified\]
    pub const CONFIRM_TEXT_INPUT_CANCEL: ElementId = ElementId(0x2F);

    /// The "N more messages waiting" banner.
    pub const PENDING_BANNER: ElementId = ElementId(0x33);
    /// Its text.
    pub const PENDING_TEXT: ElementId = ElementId(0x34);
    /// **The panel**, not the body text.
    ///
    /// `0x3D` is easy to take for "the main body text" and `0x3E` for "the
    /// countdown text". The shipped `Dialog` layout `0x2100003C` says otherwise, and so does
    /// the dialog's popup size-and-position update, which treats **`0x3E`** as a
    /// `TextElement` and treats `0x3D` as the region it resizes and centres. Dumped from the
    /// data, confirmation dialog child `0x3D` is a 400 x 95 plain element (type `0x3`) holding the eight
    /// frame images, the button strip `0x1000032F` and one `TextElement` (type `0xC`) — `0x3E`.
    /// The constant names are kept, with their actual roles stated here.
    pub const PANEL: ElementId = ElementId(0x3D);
    /// The one `TextElement` in a dialog: its prompt, and the countdown template (property 0xC5)
    /// substitutes into the same element. See [`PANEL`].
    pub const TEXT: ElementId = ElementId(0x3E);
    /// Deprecated spelling of [`PANEL`].
    pub const BODY_TEXT: ElementId = PANEL;
    /// Deprecated spelling of [`TEXT`].
    pub const COUNTDOWN_TEXT: ElementId = TEXT;
}

/// Resize and center a dialog around its text.
///
/// The original update finds text child `0x3E` and panel `0x3D`, recalculates the text layout, and
/// computes `needed = (text height - text box height) + panel height`. It grows the panel only when
/// its current height is below `needed`, preserving the never-shrink guard, then centers the panel
/// at half the dialog width and height. This function expresses the same behavior through current
/// element APIs.
///
/// Both halves are player-visible: the shipped confirmation dialog's panel is
/// 400 x 95 with a **one-line** text box at (15, 15), so an unresized dialog shows the first line
/// of its prompt and drops the rest, and an uncentred one sits in the top-left corner of the
/// screen instead of over the middle of it. The dialog creator calls this immediately after
/// it creates the element.
///
/// The root is the dialog itself, which the shipped layout sizes to the whole display — so
/// "centre in the root" is "centre on screen".
pub fn update_popup_size_and_position(ui: &mut crate::UiSystem, dialog: crate::ElemHandle) {
    let Some(panel) = ui.get_child_recursive(dialog, child::PANEL) else {
        return;
    };
    let text = ui.get_child_recursive(dialog, child::TEXT);
    let text_needed = text.and_then(|t| {
        let screen = ui.screen_box(t);
        ui.node(t)?.behaviour.as_ref()?.measured_text_height(screen)
    });
    let panel_h = ui.node(panel).map_or(0, |n| n.region.box_.height());
    if let (Some(needed), Some(t)) = (text_needed, text) {
        let text_h = ui.node(t).map_or(0, |n| n.region.box_.height());
        let grown = (needed - text_h) + panel_h;
        if panel_h < grown {
            let w = ui.node(panel).map_or(0, |n| n.region.box_.width());
            ui.resize_to(panel, w, grown);
        }
    }
    // Centre the panel on the dialog, as the element's own centring does.
    let Some(root) = ui.node(dialog).map(|n| n.region.box_) else {
        return;
    };
    let Some(b) = ui.node(panel).map(|n| n.region.box_) else {
        return;
    };
    ui.move_to(
        panel,
        root.width() / 2 - b.width() / 2,
        root.height() / 2 - b.height() / 2,
    );
}

/// An alias so [`child`]'s ids can be named as a type in signatures.
pub type DialogChild = ElementId;

/// The `Dialog` element's own state.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub kind: DialogKind,
    /// The dialog context — the id `make_dialog` returned.
    pub context: u64,
    /// The caller's collection, both input **and** output.
    pub data: crate::PropertyCollection,
    /// Absolute time at which the dialog auto-cancels; `None` = no timeout.
    pub expiration: Option<f64>,
    /// Whether property 0xAC asked for modality.
    pub modal: bool,
    /// How many dialogs are waiting behind this one, for the child-0x33 banner.
    pub pending_behind: usize,
}

/// Why a dialog was cancelled. Reason **3** is "timed out", the value the dialog's
/// global-message listener passes to the cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    Reset,
    Replaced,
    TimedOut,
}

impl CancelReason {
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Reset => 1,
            Self::Replaced => 2,
            Self::TimedOut => 3,
        }
    }
}

impl Dialog {
    /// Behavior: store the collection and read the two common properties:
    /// **0xAC** (modal → the dialog blocks clicks behind itself) and **0xC6** (timeout in
    /// seconds → `expiration` and a global-message-3 registration).
    #[must_use]
    pub fn new(kind: DialogKind, context: u64, data: crate::PropertyCollection, now: f64) -> Self {
        let modal = data
            .get_bool(crate::props::attr::DIALOG_MODAL)
            .unwrap_or(false);
        let expiration = data
            .get_float(crate::props::attr::DIALOG_TIMEOUT)
            .map(|s| now + f64::from(s));
        Self {
            kind,
            context,
            data,
            expiration,
            modal,
            pending_behind: 0,
        }
    }

    /// Handle the countdown on message 3: past the expiry, cancel with reason
    /// 3; otherwise rewrite the countdown text.
    #[must_use]
    pub fn tick(&self, now: f64) -> Option<CancelReason> {
        match self.expiration {
            Some(t) if now > t => Some(CancelReason::TimedOut),
            _ => None,
        }
    }

    /// The seconds still on the clock, for the child-0x3E countdown text.
    #[must_use]
    pub fn remaining(&self, now: f64) -> Option<f64> {
        self.expiration.map(|t| (t - now).max(0.0))
    }

    /// The answer travels back through the *same* collection the caller supplied, under property
    /// **0x92** — not through a callback argument.
    pub fn answer(&mut self, button: ElementId) {
        self.data.set(
            crate::props::attr::DIALOG_ANSWER,
            crate::PropertyValue::Bool(button == child::BUTTON1),
        );
    }

    #[must_use]
    pub fn answered_button(&self) -> Option<ElementId> {
        self.data
            .get_bool(crate::props::attr::DIALOG_ANSWER)
            .map(|v| if v { child::BUTTON1 } else { child::BUTTON2 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: property 0x8E selects the dialog's element type and layout from the fixed table.
    #[test]
    fn the_kind_property_maps_to_the_documented_type_and_root() {
        let rows = [
            (1_u32, DialogKind::Confirmation, 0x13_u32, 0x15_u32),
            (2, DialogKind::Wait, 0x19, 0x31),
            (3, DialogKind::Message, 0x17, 0x24),
            (4, DialogKind::TextInput, 0x18, 0x28),
            (5, DialogKind::ConfirmationTextInput, 0x15, 0x2C),
            (6, DialogKind::Menu, 0x16, 0x1B),
            (7, DialogKind::ConfirmationMenu, 0x14, 0x1F),
        ];
        assert_eq!(rows.len(), 7);
        for (p, kind, ty, root) in rows {
            assert_eq!(DialogKind::from_property(p), Some(kind));
            assert_eq!(kind.element_type().0, ty);
            assert_eq!(kind.root_element_id().0, root);
        }
        assert_eq!(DialogKind::from_property(0), None);
        assert_eq!(DialogKind::from_property(8), None);
    }

    /// The timeout properties leave the dialog open through its deadline and cancel it with reason
    /// 3 only after the deadline passes.
    #[test]
    fn a_timeout_cancels_with_reason_three_and_not_before() {
        let mut data = crate::PropertyCollection::new();
        data.set(
            crate::props::attr::DIALOG_MODAL,
            crate::PropertyValue::Bool(true),
        );
        data.set(
            crate::props::attr::DIALOG_TIMEOUT,
            crate::PropertyValue::Float(10.0),
        );
        let d = Dialog::new(DialogKind::Confirmation, 1, data, 100.0);
        assert!(d.modal);
        assert_eq!(d.expiration, Some(110.0));
        assert_eq!(d.tick(109.0), None);
        assert_eq!(d.tick(110.0), None, "the test is strictly greater than");
        assert_eq!(d.tick(110.5), Some(CancelReason::TimedOut));
        assert_eq!(CancelReason::TimedOut.code(), 3);
        assert_eq!(d.remaining(105.0), Some(5.0));
    }

    /// Retail supersedes the old knowledge claim: property 0x92 is Boolean; the raw button
    /// compatibility accessor is derived from that true/false value.
    #[test]
    fn the_answer_goes_back_through_the_callers_collection() {
        let mut d = Dialog::new(
            DialogKind::Confirmation,
            7,
            crate::PropertyCollection::new(),
            0.0,
        );
        assert_eq!(d.answered_button(), None);
        d.answer(child::BUTTON2);
        assert_eq!(d.answered_button(), Some(child::BUTTON2));
        assert_eq!(d.data.get_bool(0x92), Some(false));
        d.answer(child::BUTTON1);
        assert_eq!(d.data.get_bool(0x92), Some(true));
    }
}
