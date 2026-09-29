//! `Toolbar` — the panel buttons, the shortcut bar and the stack splitter.

pub mod shortcuts;
pub mod splitter;

use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::bind::attr;
use crate::panels::catalogue::TOOLBAR_PANEL_BUTTONS;

/// The state puts a panel button in when
/// its page is up, and its partner when the page goes down.
///
/// The retail handler walks its button array in order (an empty array answers nothing), skips a
/// null button, stops at the first entry whose panel id matches, and sets that button to state 6
/// when the panel is visible and state 1 when it is not. \[verified\]
///
/// The main chat window's panel-visibility notice uses the same literal states 6 and 1,
/// [`crate::chat::mainchat::STATE_LAMP_LIT`] / [`crate::chat::mainchat::STATE_ENABLED`] — the two
/// classes light their button the same way.
///
/// **Why 6 and 1 rather than "pressed" and "normal".** All seven shipped panel buttons carry
/// attribute `0x0B UICore_Button_toggleButton = true` and declare exactly states `1`, `3` and `6`
/// (measured on `classic_gameplay`), so the retail state setter's toggle arm turns state 6 into
/// `0x0E = true` and state 1 into `0x0E = false`, and the state update lands on 6 or 1
/// accordingly.
pub const STATE_PANEL_OPEN: StateId = StateId(6);
/// See [`STATE_PANEL_OPEN`].
pub const STATE_PANEL_CLOSED: StateId = StateId(1);

/// One toolbar panel button and the panel id its attribute `0x10000029` carries.
///
/// A (button, panel id) pair, as retail keeps them. Which button opens which page is layout
/// data, so the id is read at run time and never hard-coded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelButtonInfo {
    pub element: ElementId,
    pub handle: ElemHandle,
    pub panel_id: u32,
}

/// The toolbar panel's button array, filled at set-up.
#[derive(Debug, Default)]
pub struct Toolbar {
    pub buttons: Vec<PanelButtonInfo>,
    /// The inventory button's drag overlay — child `0x1000046c`, looked up at set-up.
    pub inventory_drag_overlay: Option<ElemHandle>,
}

impl Toolbar {
    /// Read the seven panel buttons and their panel ids off the live tree.
    pub fn setup_buttons(&mut self, ui: &UiSystem, root: ElemHandle) {
        self.buttons.clear();
        for id in TOOLBAR_PANEL_BUTTONS {
            let id = ElementId(id);
            let Some(h) = ui.get_child_recursive(root, id) else {
                continue;
            };
            let panel_id = crate::bind::attr_enum(ui, h, attr::PANEL_ID)
                .or_else(|| crate::bind::attr_int(ui, h, attr::PANEL_ID).map(|v| v as u32))
                .unwrap_or(0);
            self.buttons.push(PanelButtonInfo {
                element: id,
                handle: h,
                panel_id,
            });
        }
        self.inventory_drag_overlay = ui.get_child_recursive(root, INVENTORY_DRAG_OVERLAY);
    }

    /// The client's **`0x3E` arm**, which is keyed on
    /// element `0x100001B1` and nothing else: dragging an item over the large backpack icon shows
    /// a green-arrow overlay.
    ///
    /// All four calls set a state, not visibility — the overlay is never shown or hidden; it is
    /// put in one of two **states**. On message `0x3E` from `0x100001B1`,
    /// leaving (or no drag element) puts it down; otherwise the drag-over handling reads the
    /// dragged proxy's (item, spell, flags) and does nothing when the item id is 0 or the flags
    /// mark an alias (the alias mask `0xE`).
    ///
    /// So a spell, shortcut or salvage drag leaves the overlay exactly as it was, and only a real
    /// item raises it. Returns true when the message was the inventory button's `0x3E`.
    pub fn on_drag_cursor_over(&self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER || m.source_id != INVENTORY_BUTTON
        {
            return false;
        }
        let Some(overlay) = self.inventory_drag_overlay else {
            return true;
        };
        let proxy = ui.drag_state().element.filter(|_| m.p1 != 0);
        let Some(proxy) = proxy else {
            ui.set_state(overlay, inventory_drag_overlay::DOWN);
            return true;
        };
        let info = crate::items::widget::inq_drop_icon_info(ui, proxy);
        if info.item.is_some() && info.is_inventory_move() {
            ui.set_state(overlay, inventory_drag_overlay::UP);
        }
        true
    }

    /// The same function's **`0x15` arm** on the same button — the client:
    ///  runs. It is what takes the arrow down when the item is
    /// dropped on the icon rather than carried off it. Returns true when the message was the
    /// inventory button's `0x15`.
    pub fn on_drop_release(&self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if m.id != dereth_ui::msg::element::id::DROP_FAILED || m.source_id != INVENTORY_BUTTON {
            return false;
        }
        if let Some(overlay) = self.inventory_drag_overlay {
            ui.set_state(overlay, inventory_drag_overlay::DOWN);
        }
        true
    }

    // **There is no panel-button click arm here, deliberately.** A click on a panel button does
    // not broadcast a set-panel-visibility notice `(panel id, !visible)`: the retail toolbar
    // never sends that notice, and its message-1 `switch` has no panel-button case at all.
    //
    // What a panel button really does is what every other button does: the button element's
    // element-message handler -> the button click handling fires the input action in attribute
    // `0x12`, raises `0x31` on the page registered for it under `0x57`, and the page's
    // `0x58 = 1` **toggles**. All seven shipped panel buttons carry a live `0x12`, so with the arm
    // on `dereth_ui`'s widget the click returns stop-processing and no ancestor — this toolbar
    // included — ever sees the message. The toolbar-button tests assert the `0x12` and the stop
    // for each of the seven.
    //
    // [`Self::on_set_panel_visibility`] is the real `Toolbar` half.

    /// The toolbar panel's set-panel-visibility notice (inherited unchanged by the floating
    /// toolbar).
    ///
    /// The toolbar's buttons are toggle buttons: the click flips `0x0E`, so a button lights
    /// itself and **nothing on the click path puts it out**. Without this, opening a second panel
    /// closes the first page correctly and leaves the first button lit.
    ///
    /// The client's own answer is this function: every set-panel-visibility notice
    /// reaches it, including the one `PanelStack` re-sends for the page it just covered, so the
    /// button that goes out is driven by the notice rather than by the click.
    ///
    /// Two details taken from retail rather than from the shape of the problem:
    ///
    /// - the walk **stops at the first matching entry**, and
    /// - there is **no "ignore panel id 0" guard** here, unlike
    ///   [`crate::panels::panel_stack::PanelStack::recv_set_panel_visibility`] step 1. A null
    ///   *button* is skipped; a zero *panel id* is matched like any other. It cannot fire on the
    ///   shipped tree — all seven buttons carry a non-zero `0x10000029` (7, 10, 11, 12, 13, 16,
    ///   25) and `PanelStack` never sends a notice for panel 0 — and it is transcribed rather than
    ///   improved.
    ///
    /// Returns the button it restated, so a caller can assert that the notice landed rather than
    /// assert that it did not crash.
    pub fn on_set_panel_visibility(
        &self,
        ui: &mut UiSystem,
        panel_id: u32,
        visible: bool,
    ) -> Option<ElementId> {
        let b = *self.buttons.iter().find(|b| b.panel_id == panel_id)?;
        ui.set_state(
            b.handle,
            if visible {
                STATE_PANEL_OPEN
            } else {
                STATE_PANEL_CLOSED
            },
        );
        Some(b.element)
    }
}

/// The Use and Examine buttons, which also set the client UI's target mode.
pub mod target_mode {
    use dereth_ui::ElementId;
    /// The Use button; sets the use target mode.
    pub const USE_BUTTON: ElementId = ElementId(0x1000_019D);
    /// The Examine button; sets the examine target mode.
    pub const EXAMINE_BUTTON: ElementId = ElementId(0x1000_01A5);
}

/// The four combat-mode icons.
///
/// The toolbar carries **four** buttons stacked on the same 55 × 58 rectangle at the left of the
/// strip, one per `COMBAT_MODE`, each with its own picture. The combat-mode notice handler ends
/// by looking up `0x10000192`, `0x10000193`, `0x10000194` and `0x10000195` in that order and,
/// for each one found, setting its visibility.
///
/// **The shipped layout does not mark all four visible.** `0x10000192` carries no `0x3B` at all
/// and the other three carry `0x3B = true` — and `0x3B` is the master property
/// `UICore_Element_hide`, not "visible", so the layout comes up showing the dove alone, which is
/// right for the mode a character enters the world in. Reading `0x3B` the other way up shows the
/// magic icon in peace mode; the hide-polarity tests hold it the right way. The handler below is
/// correct either way, and the `hide` reading pins the first row of the table independently.
///
/// **The pairing is `\[verified\]`.** Each of the four visibility calls passes `mode == N`, where
/// `mode` is the `COMBAT_MODE` argument:
///
/// ```text
///   mode == 1   ->  0x10000192   NONCOMBAT
///   mode == 2   ->  0x10000193   MELEE
///   mode == 4   ->  0x10000194   MISSILE
///   mode == 8   ->  0x10000195   MAGIC
/// ```
///
/// Two indirect facts agree with it — the ids running in `COMBAT_MODE` order (`NONCOMBAT` 1,
/// `MELEE` 2, `MISSILE` 4, `MAGIC` 8) and `0x10000192`'s picture `0x06004CEC` being the peace-mode
/// dove in the recorded retail frame — but corroboration is not verification; the reading is.
pub mod combat_mode {
    /// The five `COMBAT_MODE` ids — the client's combat-mode enum.
    ///
    /// Defined in [`dereth_client_contract::combat_mode`], because `GameView::combat_mode`'s
    /// default body is `NONCOMBAT` and the contract crate may not depend on this one; re-exported
    /// here.
    pub use dereth_client_contract::combat_mode::{MAGIC, MELEE, MISSILE, NONCOMBAT, UNDEF};

    /// `(mode, element)` in the combat-mode notice handler's own call order, and the
    /// toolbar-active flag's one comparison.
    ///
    /// Both are defined in [`dereth_client_contract::combat_mode`], beside the five ids, because
    /// `dereth_client::hud` ghosts the shortcut numerals with `toolbar_active` and
    /// `dereth_client::interaction` matches a pressed element against `BUTTONS`.
    pub use dereth_client_contract::combat_mode::{toolbar_active, BUTTONS};
}

/// The inventory button, which is also the drag-and-drop target: it puts
/// its drag overlay in state [`inventory_drag_overlay::UP`] on element message
/// `0x3E` (drag over) and back in [`inventory_drag_overlay::DOWN`] on leave and on the drop
/// message `0x15`. See [`Toolbar::on_drag_cursor_over`].
pub const INVENTORY_BUTTON: ElementId = ElementId(0x1000_01B1);
/// See [`INVENTORY_BUTTON`].
pub const INVENTORY_DRAG_OVERLAY: ElementId = ElementId(0x1000_046C);

/// The two states `Toolbar` puts the inventory drag overlay in. Pinned as literals so
/// a test can read them without going through the symbol that wrote them.
pub mod inventory_drag_overlay {
    use dereth_ui::StateId;
    /// `0x10000046` — the green arrow, while a real item is carried over the icon.
    pub const UP: StateId = StateId(0x1000_0046);
    /// `0x1000003f` — nothing showing.
    pub const DOWN: StateId = StateId(0x1000_003F);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered toolbar and panel behavior — the seven panel buttons in set-up order,
    /// with the inventory button last.
    #[test]
    fn the_seven_panel_buttons_are_read_in_the_documented_order() {
        assert_eq!(TOOLBAR_PANEL_BUTTONS.len(), 7);
        assert_eq!(TOOLBAR_PANEL_BUTTONS[0], 0x1000_0197);
        assert_eq!(TOOLBAR_PANEL_BUTTONS[6], INVENTORY_BUTTON.0);
        assert_eq!(INVENTORY_DRAG_OVERLAY, ElementId(0x1000_046C));
        assert_eq!(target_mode::USE_BUTTON, ElementId(0x1000_019D));
        assert_eq!(target_mode::EXAMINE_BUTTON, ElementId(0x1000_01A5));
    }

    /// Oracle: the toolbar panel's set combat mode notice's four child lookups, in the order
    /// retail makes them, and the recovered combat-mode enum.
    #[test]
    fn the_four_stance_buttons_are_the_four_combat_modes_in_id_order() {
        use combat_mode as cm;
        assert_eq!(cm::BUTTONS.len(), 4);
        assert_eq!(cm::BUTTONS[0], (cm::NONCOMBAT, ElementId(0x1000_0192)));
        assert_eq!(cm::BUTTONS[3], (cm::MAGIC, ElementId(0x1000_0195)));
        // The ids are contiguous and the modes are the enum's four valid bits.
        for (i, (m, e)) in cm::BUTTONS.iter().enumerate() {
            assert_eq!(e.0, 0x1000_0192 + u32::try_from(i).unwrap());
            assert_eq!(*m, 1 << i, "COMBAT_MODE is a bit mask: 1, 2, 4, 8");
        }
        assert_eq!(
            cm::NONCOMBAT | cm::MELEE | cm::MISSILE | cm::MAGIC,
            0xF,
            "VALID_COMBAT_MODES"
        );
        assert_eq!(
            cm::MELEE | cm::MISSILE | cm::MAGIC,
            0xE,
            "COMBAT_COMBAT_MODE"
        );
        // The toolbar is active unless the new mode is MAGIC — the handler's first step.
        assert!(cm::toolbar_active(cm::NONCOMBAT));
        assert!(cm::toolbar_active(cm::MELEE));
        assert!(cm::toolbar_active(cm::MISSILE));
        assert!(!cm::toolbar_active(cm::MAGIC));
        assert!(cm::toolbar_active(cm::UNDEF));
    }
}
