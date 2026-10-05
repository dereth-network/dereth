//! The combat panel — **the combat window's own controls**.
//!
//! [`super::combat_notice`] is the chain that *opens* `<COMB>` (`0x100006B5`); this module wires
//! the window's contents.
//!
//! # What the window is, measured from the shipped tree
//!
//! The combat panel (`0x1000005C`, element type `0x1000000C`) is a page of `<COMB>`. It is built
//! from `0x21000005` by `GamePlayScreen::create`; its children are:
//!
//! | element | type | what it is |
//! |---|---|---|
//! | `0x1000004F` | `0x0B` scrollbar | the **desired-power** slider, and the `0x85` notch |
//! | `0x10000050` | `0x07` meter | the **actual** power fill, attribute `0x69` |
//! | `0x100005EF` | `0x03` field | Recklessness indicator, inside the meter |
//! | `0x10000053/54/55` | `0x10000035` option check box | `AutoRepeatAttack`, `AutoTarget`, `ViewCombatTarget` |
//! | `0x10000056` | `0x11` group box | the attack-height radio group |
//! | `0x10000057/58/59` | `0x01` button | high / medium / low |
//!
//! The three check boxes are type `0x10000035`, which **is** the option check box
//! (`element_types.rs`'s own table, and `ty::OPTION_CHECKBOX`), so the cast succeeds and the
//! bindings are reproducible. [`CombatWindow::post_init`] makes the cast rather than assuming it,
//! and counts the failures.
//!
//! # The element-message handler's three arms
//!
//! The message-`0x0A` arm has no element filter and carries the recklessness re-check inside
//! it — both of which are true, surprising as they look:
//!
//! * Mouse press (`0x1C`) on `0x10000057/58/59` sets requested attack height to
//!   high/medium/low respectively, encoded as 1/2/3.
//! * Button click (1) on the same buttons ends the attack at that height with power
//!   override `-1.0f`.
//! * Scrollbar position (`0x0A`) has no element filter. It rechecks Recklessness
//!   advancement (`0x32`), multiplies the position by `0.001`, clamps to `[0, 1]`,
//!   and stores the requested attack power.
//!
//! So the press sets the height — which turns into a
//! attack request, i.e. the power bar starts charging — and the completed click releases it
//! at `-1.0`, "use what the bar says". **The button is a held control exactly like the key.**
//! The combat action handler reaches the same two functions from `CombatLow/Medium/HighAttack`
//! and `CombatAimLow/Medium/High`, which is why this module and `Interaction::on_actions` raise the
//! same requests.
//!
//! # Scope, and what is deliberately not here
//!
//! Two of the three check boxes have engine side effects that live elsewhere and that this module
//! does **not** half-implement:
//!
//! * `AutoTarget` -> combat auto-targeting, which needs the player's select-next behavior.
//! * `ViewCombatTarget` -> target tracking and its update, counted in
//!   `option_side_effects_unapplied` for exactly this.
//!
//! Both check boxes still *set their bit and send it*, because that half is
//! the option setter, which belongs to this window's own control and is here. What is deferred is
//! the engine call the option then drives.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{GameView, PlayerOption, UiRequest};

/// The combat panel itself — element type `0x1000000C`, and the page the set-combat-mode notice
/// shows in melee and missile.
pub const COMBAT_UI: ElementId = ElementId(0x1000_005C);

/// The recklessness field — the client finds `0x100005EF` by recursive child search, then
/// caches and **hides** it.
pub const RECKLESSNESS_FIELD: ElementId = ElementId(0x1000_05EF);

/// The desired-power **scrollbar**: the element
/// writes attribute `0x85` on, and the element whose own `0x0A` sets the value.
pub const DESIRED_POWER: ElementId = ElementId(0x1000_004F);

/// The actual-power meter: writes attribute `0x69` here,
/// and **only** for `PBM_COMBAT`.
pub const ACTUAL_POWER: ElementId = ElementId(0x1000_0050);

/// The attack-height group box: writes the selected
/// child id (`0xB1`) here; its set-attribute hook switches the children's states to 1 / 6.
pub const ATTACK_HEIGHT_GROUP: ElementId = ElementId(0x1000_0056);

/// `ATTACK_HEIGHT`, retail's own enum values — **not** the 0/1/2 index
/// [`crate::hud::powerbar::combat::AttackHeight`] uses to walk `ATTACK_HEIGHT_STATES`.
pub mod height {
    pub const UNDEF: u32 = 0;
    pub const HIGH: u32 = 1;
    pub const MEDIUM: u32 = 2;
    pub const LOW: u32 = 3;
}

/// The three attack-height buttons, paired with the `ATTACK_HEIGHT` each one means. The element id
/// is also the selected-child id the attack-height-changed notice writes for that height, so
/// one table serves both directions.
pub const ATTACK_HEIGHT_BUTTONS: [(ElementId, u32); 3] = [
    (ElementId(0x1000_0057), height::HIGH),
    (ElementId(0x1000_0058), height::MEDIUM),
    (ElementId(0x1000_0059), height::LOW),
];

/// The client's three check-box bindings (find the child, cast it to an option check box, set
/// its player option), in the order it makes them.
pub const OPTION_CHECKBOXES: [(ElementId, PlayerOption); 3] = [
    (ElementId(0x1000_0053), PlayerOption::AutoRepeatAttack),
    (ElementId(0x1000_0054), PlayerOption::AutoTarget),
    (ElementId(0x1000_0055), PlayerOption::ViewCombatTarget),
];

/// Combat-specific captions, not `ID_PlayerOption_*` captions from Character Options.
/// Bound by the combat panel's post-init using three string tokens.
/// All three use string table enum 0x10000003.
pub const OPTION_CAPTIONS: [&str; 3] = [
    "ID_CombatPanelOption_AutoRepeatAttack",
    "ID_CombatPanelOption_AutoTarget",
    "ID_CombatPanelOption_ViewCombatTarget",
];

/// The same post-init's three tooltip writes, in binding order.
pub const OPTION_HELP: [&str; 3] = [
    "ID_PlayerOption_AutoRepeatAttack_Help",
    "ID_PlayerOption_AutoTarget_Help",
    "ID_PlayerOption_ViewCombatTarget_Help",
];

pub const RECKLESSNESS_TRAINED: u32 = 2;

/// The option check box's value attribute: refresh writes it,
/// and the element-message handler reads it back.
pub const ATTR_CHECKED: u32 = 0x0E;

/// One bound check box, the same shape `options::character::CharacterOptionRow` has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionBox {
    /// The player option the check box was bound to.
    pub option: PlayerOption,
    /// The option-check-box element.
    pub element: ElemHandle,
    /// The check box's current value.
    pub current: bool,
}

/// The combat panel state needed to answer a gesture.
#[derive(Debug, Default)]
pub struct CombatWindow {
    /// The page itself, `0x1000005C`.
    pub page: Option<ElemHandle>,
    /// The recklessness field.
    pub recklessness_field: Option<ElemHandle>,
    /// `0x1000004F`.
    pub desired_power: Option<ElemHandle>,
    /// `0x10000050`.
    pub actual_power: Option<ElemHandle>,
    /// `0x10000056`.
    pub height_group: Option<ElemHandle>,
    /// The three buttons, in [`ATTACK_HEIGHT_BUTTONS`] order.
    pub height_buttons: Vec<(ElemHandle, u32)>,
    /// The three check boxes, in post-init's order.
    pub options: Vec<OptionBox>,
    /// Child-search or cast failures — the denominator that separates "no check
    /// box bound" from "the window is not on this layout".
    pub failures: usize,
    /// The last `ATTACK_HEIGHT` written to the group box, so the write is edge-driven.
    pub shown_height: Option<u32>,
    /// The last desired power written to the notch.
    pub shown_desired_power: Option<f32>,
    /// The last actual power written to the meter.
    pub shown_actual_power: Option<f32>,
}

impl CombatWindow {
    /// The bindings, the four notice registrations aside.
    ///
    /// The client's first act after post-init is to register for four attack-height notices;
    /// this build has no notice bus (see [`super::combat_notice`]), so those
    /// four arrive as the [`Self::on_attack_height_changed`],
    /// [`Self::on_set_powerbar_level`],
    /// [`Self::on_desired_attack_power_changed`] and
    /// `GamePlayScreen::on_set_combat_mode` calls the frame drives.
    ///
    /// The recklessness field is bound **and hidden**, which is why a character with
    /// no Recklessness never sees it even before the first mode change.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.page = ui.get_child_recursive(root, COMBAT_UI);
        self.recklessness_field = None;
        self.desired_power = None;
        self.actual_power = None;
        self.height_group = None;
        self.height_buttons.clear();
        self.options.clear();
        self.failures = 0;
        self.shown_height = None;
        self.shown_desired_power = None;
        self.shown_actual_power = None;
        let Some(page) = self.page else { return };
        self.recklessness_field = ui.get_child_recursive(page, RECKLESSNESS_FIELD);
        if let Some(h) = self.recklessness_field {
            ui.set_visible(h, false);
        }
        self.desired_power = ui.get_child_recursive(page, DESIRED_POWER);
        self.actual_power = ui.get_child_recursive(page, ACTUAL_POWER);
        self.height_group = ui.get_child_recursive(page, ATTACK_HEIGHT_GROUP);
        for (id, h) in ATTACK_HEIGHT_BUTTONS {
            if let Some(elem) = ui.get_child_recursive(page, id) {
                self.height_buttons.push((elem, h));
            } else {
                self.failures += 1;
            }
        }
        for (i, (id, option)) in OPTION_CHECKBOXES.into_iter().enumerate() {
            let Some(element) = ui.get_child_recursive(page, id) else {
                self.failures += 1;
                continue;
            };
            // Casting to an option check box — the client registers nothing when it fails. On the
            // shipped tree it succeeds.
            if ui.node(element).map(dereth_ui::ElementNode::ty)
                != Some(crate::element_types::ty::OPTION_CHECKBOX)
            {
                self.failures += 1;
                continue;
            }
            let table = crate::options::character::table(ui);
            crate::options::page::set_string_info(
                ui,
                element,
                table,
                dereth_primitives::num::hash::str_hash(OPTION_CAPTIONS[i].as_bytes()),
            );
            let help = ui.resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(OPTION_HELP[i].as_bytes()),
            );
            ui.set_tooltip(element, help);
            self.options.push(OptionBox {
                option,
                element,
                current: false,
            });
        }
    }

    /// True once [`Self::post_init`] found the page.
    #[must_use]
    pub const fn bound(&self) -> bool {
        self.page.is_some()
    }

    ///  for all three, then.
    ///
    /// The client reads the value at bind time and again whenever the option page saves; here the
    /// window is re-read once a frame and written only when it moved, which is the same polling
    /// the indicator lamps use for the same reason (no notice bus). Returns how many moved.
    pub fn refresh(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> u32 {
        let mut moved = 0;
        for i in 0..self.options.len() {
            let v = view.player_option(self.options[i].option);
            if self.options[i].current == v {
                continue;
            }
            self.options[i].current = v;
            ui.set_attribute_bool(self.options[i].element, ATTR_CHECKED, v);
            moved += 1;
        }
        moved
    }

    /// Height and desired-power read-back, once per frame. Actual power arrives separately as
    /// ordered set-power-bar-level notices; a final snapshot loses the zero that hiding the power
    /// bar writes.
    ///
    /// In the client each is raised by the combat system the moment its value changes
    /// (setting the requested attack height raises the attack-height-changed notice, and the
    /// combat action handler's gauge arm raises the desired-attack-power-changed notice). These two
    /// remain polled and each writes only when it moved. **Declared deviation: one frame late**,
    /// like every other polled handler here.
    ///
    /// Returns how many of the two wrote, which is the denominator that separates "nothing
    /// changed" from "this never ran".
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> u32 {
        let bar = view.combat_bar();
        u32::from(self.on_attack_height_changed(ui, bar.requested_attack_height))
            + u32::from(self.on_desired_attack_power_changed(ui, bar.desired_power))
    }

    /// The combat panel's attack height changed notice — set `0x10000056`'s enum attribute
    /// `0xB1` to the state, where the state is the element id of the button for that height.
    ///
    /// The undefined attack height writes nothing: the client's `if / else if / else if` has no
    /// fourth arm. Returns whether it wrote.
    pub fn on_attack_height_changed(&mut self, ui: &mut UiSystem, h: u32) -> bool {
        let Some(group) = self.height_group else {
            return false;
        };
        let Some((state, _)) = ATTACK_HEIGHT_BUTTONS.iter().find(|(_, hh)| *hh == h) else {
            return false;
        };
        if self.shown_height == Some(h) {
            return false;
        }
        self.shown_height = Some(h);
        ui.set_attribute_enum(
            group,
            dereth_ui::widgets::groupbox::SELECTED_BUTTON,
            state.0,
        );
        true
    }

    /// **Only** `PBM_COMBAT`, which is what
    /// keeps the jump bar and the advanced-combat bar out of this meter.
    pub fn on_set_powerbar_level(
        &mut self,
        ui: &mut UiSystem,
        mode: crate::hud::powerbar::PowerBarMode,
        level: f32,
    ) -> bool {
        if mode != crate::hud::powerbar::PowerBarMode::Combat {
            return false;
        }
        let Some(h) = self.actual_power else {
            return false;
        };
        if self.shown_actual_power == Some(level) {
            return false;
        }
        self.shown_actual_power = Some(level);
        ui.set_attribute_float(h, crate::bind::attr::METER_LEVEL, level);
        true
    }

    /// The combat panel's desired attack power changed notice — attribute `0x85`, the
    /// notch, on the scrollbar itself.
    pub fn on_desired_attack_power_changed(&mut self, ui: &mut UiSystem, power: f32) -> bool {
        let Some(h) = self.desired_power else {
            return false;
        };
        if self.shown_desired_power == Some(power) {
            return false;
        }
        self.shown_desired_power = Some(power);
        ui.set_attribute_float(h, crate::bind::attr::MARKER, power);
        true
    }

    /// The combat panel's element-message handler's three arms.
    ///
    /// Returns the request it raised. Everything that reaches the combat system is a request
    /// because this crate has no `World`: the arms set requested attack height,
    /// end the attack at `(height, -1.0)`, or clamp `p1 * 0.001` as requested power; then
    /// `dereth_client_runtime::interaction::Interaction::run_ui_requests` is where each becomes the call.
    ///
    /// The `0x0A` arm's recklessness re-check is applied here rather than deferred, because it is
    /// a pure element write the window owns.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> Option<UiRequest> {
        use dereth_ui::msg::element::id;
        if m.id == id::SCROLL_POSITION {
            // The client filters on nothing at all here, because the combat panel receives only its
            // own subtree's messages. This build fans every element message out to the screen, so
            // the page is used as the filter — otherwise the chat log's scrollbar would move the
            // gauge.
            let inside = self
                .page
                .is_some_and(|p| p == m.source || ui.is_ancestor_of(p, m.source));
            if !inside {
                return None;
            }
            self.apply_recklessness_visibility(ui, view);
            // The message's first parameter is read back as an unsigned scrollbar position; see
            // `dereth_client_model::CombatState::set_ui_requested_power_from_scrollbar` for the
            // comparison that makes that matter.
            return Some(UiRequest::CombatSetDesiredPower { position: m.p1 });
        }
        let height = ATTACK_HEIGHT_BUTTONS
            .iter()
            .find(|(e, _)| *e == m.source_id)
            .map(|(_, h)| *h)?;
        if m.id == id::MOUSE_PRESS {
            return Some(UiRequest::CombatSetAttackHeight { height });
        }
        if m.id == id::BUTTON_CLICKED {
            return Some(UiRequest::CombatEndAttack { height });
        }
        None
    }

    /// The half of the `0x0A` arm that is not the gauge: read the advancement class of skill
    /// `0x32` and show the recklessness field when it is at least trained (2).
    ///
    /// The set-combat-mode notice makes the same test at a different
    /// moment — see [`crate::hud::combat_notice::combat_ui_arm`], which owns the mode-change half.
    pub fn apply_recklessness_visibility(&self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(h) = self.recklessness_field else {
            return false;
        };
        let visible = view.recklessness_advancement_class() >= RECKLESSNESS_TRAINED;
        ui.set_visible(h, visible);
        visible
    }

    /// The client's message-1 arm for the three boxes
    /// this window owns: read attribute `0x0E` back and apply it.
    ///
    /// Kept separate from [`Self::on_element_message`] because the check boxes and the attack
    /// buttons both raise message **1** and the client's two handlers are different objects — the
    /// check box's handler and the combat panel's. Folding them into one `match`
    /// on the id is what would let a check-box click be read as an attack.
    pub fn on_checkbox_message(
        &mut self,
        ui: &UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> Option<UiRequest> {
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return None;
        }
        let i = self.options.iter().position(|o| o.element == m.source)?;
        let v = crate::bind::attr_bool(ui, self.options[i].element, ATTR_CHECKED).unwrap_or(false);
        self.options[i].current = v;
        Some(UiRequest::SetPlayerOption(self.options[i].option, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the stated testability rule: *"pin a transcribed constant as a literal somewhere"*. Every id here is
    /// spelled out once against retail in this module's own documentation, so a test
    /// that reads them back through the same symbols cannot hide a wrong number.
    #[test]
    fn the_combat_window_ids_match_the_three_attack_heights() {
        assert_eq!(COMBAT_UI.0, 0x1000_005C);
        assert_eq!(DESIRED_POWER.0, 0x1000_004F);
        assert_eq!(ACTUAL_POWER.0, 0x1000_0050);
        assert_eq!(ATTACK_HEIGHT_GROUP.0, 0x1000_0056);
        assert_eq!(RECKLESSNESS_FIELD.0, 0x1000_05EF);
        // The attack-height values are HIGH 1, MEDIUM 2, LOW 3 — **not** the 0/1/2 the media-state
        // array is indexed by.
        assert_eq!(ATTACK_HEIGHT_BUTTONS[0], (ElementId(0x1000_0057), 1));
        assert_eq!(ATTACK_HEIGHT_BUTTONS[1], (ElementId(0x1000_0058), 2));
        assert_eq!(ATTACK_HEIGHT_BUTTONS[2], (ElementId(0x1000_0059), 3));
        assert_eq!(height::UNDEF, 0);
        assert_eq!((height::HIGH, height::MEDIUM, height::LOW), (1, 2, 3));
        // Preserve the player-option binding order from post-init.
        assert_eq!(OPTION_CHECKBOXES[0].0 .0, 0x1000_0053);
        assert_eq!(OPTION_CHECKBOXES[1].0 .0, 0x1000_0054);
        assert_eq!(OPTION_CHECKBOXES[2].0 .0, 0x1000_0055);
        assert_eq!(ATTR_CHECKED, 0x0E);
        assert_eq!(RECKLESSNESS_TRAINED, 2);
        // The media state written for a height is the button's own element id, which is what
        // makes `ATTACK_HEIGHT_STATES` and these three the same three numbers.
        assert_eq!(
            crate::hud::powerbar::combat::ATTACK_HEIGHT_STATES,
            [
                ATTACK_HEIGHT_BUTTONS[0].0 .0,
                ATTACK_HEIGHT_BUTTONS[1].0 .0,
                ATTACK_HEIGHT_BUTTONS[2].0 .0
            ]
        );
    }
}
