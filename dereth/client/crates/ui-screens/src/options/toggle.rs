//! Panel-bound check boxes for character options — the five boxes that are
//! not on an option page.
//!
//! The four controls the Fellowship panel binds during its initialization are the
//! character-options mechanism, not panel state, and the allegiance tab has a fifth one of the
//! same shape. None of the five is on an option page, so the option pages do not cover them;
//! without this module they draw no label, do nothing when clicked and all show unticked.
//!
//! # The five, and where they come from
//!
//! Player-option binding is used by the Character Options page (`super::character`),
//! three controls in combat-panel initialization ([`crate::hud::combat_window`]), one
//! in friends-panel initialization and **five** across the two social panels. This
//! module owns the five social controls. Their caption targets were verified from the complete
//! binding sequence because an intermediate representation obscured the destination field.
//!
//! Allegiance initialization finds child `0x10000262`, verifies the option-checkbox
//! type `0x10000035`, binds option 1 and installs the caption token
//! `ID_PlayerOption_IgnoreAllegianceRequests` plus its `_Help` tooltip token.
//! Fellowship initialization repeats that sequence for children `0x10000270` through
//! `0x10000273`, bound respectively to options `0x02`, `0x12`, `0x0F`, and `0x11`:
//! ignore requests, auto-accept requests, share experience and share loot.
//!
//! **The caption is not the preference registry's.** The preference-binding path
//! — the leg `super::preferences` fills — does not run
//! once a player option is bound, and these five never had a preference name. Their caption is the
//! literal token pair installed during panel initialization, spelled mechanically from the option's enum
//! name, exactly as the Character Options page spells its 50 labels.
//! So `super::character::label_token` is the right producer and this module reuses it rather
//! than carrying a second spelling of the same 5 strings.
//!
//! # The three ends this module joins
//!
//! | end | the machinery | what this module adds |
//! |---|---|---|
//! | the caption | `super::page::set_string_info` | names these five elements |
//! | the value | [`crate::view::GameView::player_option`] | reads it for these five |
//! | the click | `dereth_ui::widgets::button`'s `0x0B`/`0x0E` toggle | the arm keyed on these five sources |
//!
//! The elements themselves need nothing: all five are element type `0x10000035` in the shipped
//! `classic_gameplay` tree and all five carry attribute `0x0B`, so the button element's mouse-up
//! handler flips `0x0E` on every press. The press goes out as element message 1, which is what
//! this module handles.
//!
//! # The default state is not "all off"
//!
//! The check box reads **the character's own bound option word**,
//! never a page default. The default character-option word is `0x50C4A54A`,
//! which sets `IgnoreFellowshipRequests` (`0x08`) and `FellowshipShareXP` (`0x040000`) and leaves
//! `FellowshipShareLoot` (`0x100000`) and `FellowshipAutoAcceptRequests` (`0x20000000`) clear. So
//! a shipped character opens the Create screen with **two of the four ticked**, not all off.
//! `IgnoreAllegianceRequests` (`0x04`) is clear in that
//! word, so the allegiance box does open unticked — correctly, and for a reason rather than by
//! accident.
//!
//! # Declared deviations
//!
//! 1. **The values live here, not on the element.** Same convention `super::page` states: in the
//!    client each option check box *is* the element and keeps its current value in that object;
//!    here the element carries only button behaviour (`super::controls`) and the
//!    holder owns the values.
//! 2. **The value is polled, not noticed.** The reference control listens for option reload,
//!    option-panel refresh and player-option-change notices; all three read the bound value and
//!    refresh the control. This
//!    build has no notice bus, so `PanelOptionBoxes::refresh` is driven once a frame and writes
//!    only when the bit moved — the same treatment [`crate::hud::combat_window`] gives the combat
//!    window's three, and one frame late in the same way.
//! 3. **The two panels' tables live here rather than in `panels::allegiance` and
//!    `panels::fellowship`.** The element binding stands alone; the binder runs from
//!    [`crate::panels::remaining::RemainingPanels::post_init`], which is already the one place
//!    that holds the screen root, a `GameView` and the element messages at once.
//! 4. **The friends panel's `AppearOffline` box is not here.** It is the sixth player-option bind
//!    site, and `AppearOffline` is not in `PlayerOption` at all. The table below takes a slice,
//!    so a sixth row is one const away.
//!
//!    The friends-panel initialization performs **three** actions and not five: recursively find child `0x1000052C`, verify type
//!    `0x10000035`, bind player option `0x27` — and then it **stops**. It neither installs
//!    a string token nor a tooltip, which the complete operation sequence confirms:
//!    That initialization has **no string-table reference at all** and performs only
//!    base initialization, recursive child lookup, player-option binding and
//!    global-event-handler lookup. The caption *"Appear Offline"* is authored in the
//!    shipped layout and the box draws it today; `ID_PlayerOption_AppearOffline` is not in string
//!    table `0x10000003` at all, so reusing `super::character::label_token` for this one would
//!    hash a token that resolves to nothing and `ui.set_tooltip(_, None)` would wipe whatever
//!    tooltip the layout carries. That is what `Caption` is for. The sixth row lives in
//!    [`crate::panels::friends`], beside its element id.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{GameView, PlayerOption, UiRequest};

/// The option check box's value attribute: refresh writes it, and
/// the element-message-1 handler reads it back.
pub const ATTR_CHECKED: u32 = 0x0E;

/// The allegiance panel's one recursive-child lookup, type check and player-option bind:
/// child `0x10000262`, option 1.
pub const ALLEGIANCE_OPTION_BOXES: [(ElementId, PlayerOption); 1] = [(
    ElementId(0x1000_0262),
    PlayerOption::IgnoreAllegianceRequests,
)];

/// The client's four, in the order it binds them. All four sit inside
/// the **not-in-fellowship** frame `0x1000026B`, which is the fellowship *Create* screen.
///
/// The player-option binding arguments are `2`, `0x12`, `0x0F`,
/// `0x11`.
pub const FELLOWSHIP_OPTION_BOXES: [(ElementId, PlayerOption); 4] = [
    (
        ElementId(0x1000_0270),
        PlayerOption::IgnoreFellowshipRequests,
    ),
    (
        ElementId(0x1000_0271),
        PlayerOption::FellowshipAutoAcceptRequests,
    ),
    (ElementId(0x1000_0272), PlayerOption::FellowshipShareXP),
    (ElementId(0x1000_0273), PlayerOption::FellowshipShareLoot),
];

/// Where a panel-bound check box's caption comes from — **measured per call site, not assumed.**
///
/// The two social panels and the friends panel differ here, which is why the
/// sixth box could not simply be appended to a table:
///
/// ```text
/// allegiance initialization  find child / verify type / bind option / set caption / set tooltip
/// fellowship initialization  ... the same five actions, four times over
/// friends initialization     find child / verify type / bind option   <- and stops
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caption {
    ///  with `ID_PlayerOption_<Name>`, then
    ///  with `..._Help` — the five social-panel boxes.
    FromToken,
    /// Neither call occurs during panel initialization, so the caption is the layout's authored text
    /// and this holder only *reads* it. Writing a token here would be worse than leaving it alone:
    /// `ID_PlayerOption_AppearOffline` is not in the string table, and `set_tooltip(h, None)` would
    /// clear a tooltip the layout may carry.
    FromLayout,
}

/// One panel-bound check box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelOptionBox {
    /// The player option read and written by this box.
    pub option: PlayerOption,
    /// The option-check-box element itself, which is also the caption target and
    /// carries the inherited button and text-element behavior.
    pub element: ElemHandle,
    /// The last value observed from the game view.
    ///
    /// **`Option`, and the outer layer is load-bearing.** `None` is *"no `GameView` has answered
    /// for this box yet"*, which is not the same as *"the character has it off"* — the second is
    /// the all-off state and the two must not be one state. It is also what makes
    /// the first `PanelOptionBoxes::refresh` always write the attribute, rather than skipping a
    /// character whose option happens to be `false`.
    pub current: Option<bool>,
    /// What is actually drawn on the box: the string-table result for a
    /// [`Caption::FromToken`] box, or the layout's own text for a [`Caption::FromLayout`] one.
    /// `None` when there is no string table, or no caption at all.
    pub label: Option<String>,
    /// The token the caption id is hashed from — recorded whether or not it resolved, so a headless run can
    /// tell *"no string table"* from *"no caption"*. **`None` for a [`Caption::FromLayout`] box**,
    /// because its initialization hashes nothing: the absence is the measurement, and an empty `String`
    /// here would read as "hashed the empty token".
    pub label_token: Option<String>,
    /// The `_Help` token is given, `None` on the same terms as
    /// [`Self::label_token`].
    pub help_token: Option<String>,
    /// Which initialization path bound this box.
    pub caption: Caption,
}

/// The check boxes bound during one panel's initialization.
#[derive(Debug, Default, Clone)]
pub struct PanelOptionBoxes {
    /// In the panel's own bind order.
    pub boxes: Vec<PanelOptionBox>,
    /// Recursive-child lookup or type-check misses — the denominator that separates *"the panel is
    /// not on this layout"* from *"the boxes bound and did nothing"*.
    pub failures: usize,
    /// How many of [`Self::boxes`] resolved a caption; zero means the boxes draw without labels.
    pub captions: usize,
    /// How many boxes a `GameView` has ever answered for. **0 with a host that has no
    /// `PlayerModule` is the right answer**, and it is the number that separates that host from a
    /// character whose options are genuinely all off.
    pub values_seen: usize,
}

impl PanelOptionBoxes {
    /// One panel's sequence: recursively find a child, verify type `0x10000035`, bind its option,
    /// then install its caption and tooltip.
    ///
    /// `root` is the screen root rather than the panel element, for the reason
    /// `AllegiancePanel::post_init` gives: every id in both tables is unique in the shipped tree,
    /// and the social page the two panels sit on is built lazily by the panel stack.
    ///
    /// The type check is reproduced and is not a formality — the client binds nothing when it
    /// fails, and reproducing that is what stops a layout change from silently making these five
    /// plain buttons that toggle and send.
    ///
    /// Returns how many boxes bound.
    pub fn post_init(
        &mut self,
        ui: &mut UiSystem,
        root: ElemHandle,
        table: &[(ElementId, PlayerOption)],
    ) -> usize {
        self.post_init_with_caption(ui, root, table, Caption::FromToken)
    }

    /// [`Self::post_init`] with the caption leg named.
    ///
    /// The five social-panel boxes take [`Caption::FromToken`], which is what [`Self::post_init`]
    /// passes; the friends panel's sixth takes [`Caption::FromLayout`] because its initialization stops
    /// after binding the player option. See `Caption`.
    pub fn post_init_with_caption(
        &mut self,
        ui: &mut UiSystem,
        root: ElemHandle,
        table: &[(ElementId, PlayerOption)],
        caption: Caption,
    ) -> usize {
        self.boxes.clear();
        self.failures = 0;
        self.captions = 0;
        self.values_seen = 0;
        let strings = super::character::table(ui);
        for (id, option) in table.iter().copied() {
            let Some(element) = ui.get_child_recursive(root, id) else {
                self.failures += 1;
                continue;
            };
            if ui.node(element).map(dereth_ui::ElementNode::ty)
                != Some(crate::element_types::ty::OPTION_CHECKBOX)
            {
                self.failures += 1;
                continue;
            }
            // Install the panel's literal caption and tooltip token pair. The caption
            // goes on the check box itself; the tooltip uses the UI tooltip path.
            //
            // A `FromLayout` box runs **neither** line, because its initialization does not: it only
            // reads back the caption the layout already placed, so `captions` stays the number of
            // boxes a player can read a label on either way.
            let (label_token, help_token, label) = match caption {
                Caption::FromToken => {
                    let label_token = super::character::label_token(option);
                    let help_token = super::character::help_token(option);
                    let label = super::page::set_string_info(
                        ui,
                        element,
                        strings,
                        dereth_primitives::num::hash::str_hash(label_token.as_bytes()),
                    );
                    let help = ui.resolve_string(
                        strings,
                        dereth_primitives::num::hash::str_hash(help_token.as_bytes()),
                    );
                    ui.set_tooltip(element, help);
                    (Some(label_token), Some(help_token), label)
                }
                Caption::FromLayout => {
                    let text = ui
                        .text_element_mut(element)
                        .map(|t| t.glyphs.inq_text(false))
                        .filter(|s| !s.is_empty());
                    (None, None, text)
                }
            };
            self.captions += usize::from(label.is_some());
            self.boxes.push(PanelOptionBox {
                option,
                element,
                current: None,
                label,
                label_token,
                help_token,
                caption,
            });
        }
        self.boxes.len()
    }

    /// True once at least one box bound.
    #[must_use]
    pub fn bound(&self) -> bool {
        !self.boxes.is_empty()
    }

    /// Read the current value and refresh the display for every checkbox.
    ///
    /// This is the whole of *"they default to all off"*: the value read uses
    /// the box's bound player option, so it opens at **the character's own bit**,
    /// and with nothing calling it every box opened at the attribute's zero. See the module docs
    /// for the two of four default character-option ticks.
    ///
    /// Returns how many boxes wrote, so a frame on which nothing changed is 0 and a frame on which
    /// the whole panel re-read is the box count — which is the difference between "polling and
    /// finding nothing" and "not polling".
    pub fn refresh(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> u32 {
        let mut moved = 0;
        // A caption naming luminance drops it on a world without luminance.
        let luminance = crate::panels::era::has_luminance(view);
        for b in &self.boxes {
            let Some(label) = b.label.as_deref() else {
                continue;
            };
            if !label.contains(crate::panels::era::LUMINANCE_WORDS) {
                continue;
            }
            let want = crate::panels::era::caption_for_era(label, luminance);
            if let Some(t) = ui.text_element_mut(b.element) {
                if t.glyphs.inq_text(false) != want {
                    t.set_text(&want);
                }
            }
        }
        for i in 0..self.boxes.len() {
            let v = view.player_option(self.boxes[i].option);
            if self.boxes[i].current.is_none() {
                self.values_seen += 1;
            } else if self.boxes[i].current == Some(v) {
                continue;
            }
            self.boxes[i].current = Some(v);
            ui.set_attribute_bool(self.boxes[i].element, ATTR_CHECKED, v);
            moved += 1;
        }
        moved
    }

    /// The client's message-1 arm — read attribute
    /// `0x0E` back off the element and apply it.
    ///
    /// ```text
    /// ```
    ///
    /// The attribute is read rather than assumed, because
    /// has already flipped it by the time message 1 goes out — so the value in hand is the one the
    /// player just produced, and a handler that negated its own copy would be right only until
    /// something else wrote `0x0E`.
    ///
    /// The corresponding client path writes the current value to the bound player option —
    /// **one option, never a word**; see `super::character`'s module docs on why composing the
    /// word from the boxes would silently clear bit 25 of the second option word.
    ///
    /// Returns the request the press raises, or `None` for any message this holder does not own.
    pub fn on_element_message(
        &mut self,
        ui: &UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> Option<UiRequest> {
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return None;
        }
        let i = self.boxes.iter().position(|b| b.element == m.source)?;
        let v = crate::bind::attr_bool(ui, self.boxes[i].element, ATTR_CHECKED).unwrap_or(false);
        self.boxes[i].current = Some(v);
        Some(UiRequest::SetPlayerOption(self.boxes[i].option, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the stated testability rule: pin a transcribed constant as a literal. Every number here is quoted
    /// against this module's own docs, so reading them back through the
    /// symbols cannot hide a wrong one.
    #[test]
    fn the_five_ids_and_ordinals_are_the_ones_the_two_post_inits_push() {
        assert_eq!(ALLEGIANCE_OPTION_BOXES[0].0, ElementId(0x1000_0262));
        assert_eq!(
            ALLEGIANCE_OPTION_BOXES[0].1,
            PlayerOption::IgnoreAllegianceRequests
        );
        assert_eq!(FELLOWSHIP_OPTION_BOXES[0].0, ElementId(0x1000_0270));
        assert_eq!(FELLOWSHIP_OPTION_BOXES[1].0, ElementId(0x1000_0271));
        assert_eq!(FELLOWSHIP_OPTION_BOXES[2].0, ElementId(0x1000_0272));
        assert_eq!(FELLOWSHIP_OPTION_BOXES[3].0, ElementId(0x1000_0273));
        assert_eq!(
            FELLOWSHIP_OPTION_BOXES[0].1,
            PlayerOption::IgnoreFellowshipRequests
        );
        assert_eq!(
            FELLOWSHIP_OPTION_BOXES[1].1,
            PlayerOption::FellowshipAutoAcceptRequests
        );
        assert_eq!(
            FELLOWSHIP_OPTION_BOXES[2].1,
            PlayerOption::FellowshipShareXP
        );
        assert_eq!(
            FELLOWSHIP_OPTION_BOXES[3].1,
            PlayerOption::FellowshipShareLoot
        );
        assert_eq!(ATTR_CHECKED, 0x0E);
    }

    /// The captions are the panel's literal tokens, and they are the *same producer* the Character
    /// Options page uses — including `FellowshipShareXP`'s capital `XP`, which is the one spelling
    /// a reader would get wrong.
    #[test]
    fn the_captions_are_the_id_playeroption_tokens_the_panels_hash() {
        let t: Vec<String> = FELLOWSHIP_OPTION_BOXES
            .iter()
            .map(|(_, o)| super::super::character::label_token(*o))
            .collect();
        assert_eq!(
            t,
            vec![
                "ID_PlayerOption_IgnoreFellowshipRequests",
                "ID_PlayerOption_FellowshipAutoAcceptRequests",
                "ID_PlayerOption_FellowshipShareXP",
                "ID_PlayerOption_FellowshipShareLoot",
            ]
        );
        assert_eq!(
            super::super::character::label_token(ALLEGIANCE_OPTION_BOXES[0].1),
            "ID_PlayerOption_IgnoreAllegianceRequests"
        );
        assert_eq!(
            super::super::character::help_token(ALLEGIANCE_OPTION_BOXES[0].1),
            "ID_PlayerOption_IgnoreAllegianceRequests_Help"
        );
    }
}
