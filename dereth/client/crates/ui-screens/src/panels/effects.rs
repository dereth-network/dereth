//! `EffectsPanel` — the **buff** and **debuff** panels the two magic lamps open.
//!
//! Element type `0x1000001B`; the shipped `classic_gameplay` tree holds **two** instances,
//! `0x10000184` and `0x10000185`, and the only thing that tells them apart is the layout attribute
//! `0x1000000C`, the effects UI type — `1` helpful, `2` harmful. Reading it once and applying it to
//! both would light the debuff panel with buffs.
//!
//! A lamp click reaches the visibility-toggle action and shows the panel; this file is what
//! fills it.
//!
//! # The client's own chain, and where each half already lived
//!
//! * The panel's element-message handler, on its own visibility-changed message (`0x18`, shown),
//!   subscribes to global message 3 (the per-frame tick) and runs the update.
//! * The update does nothing while the panel is hidden; otherwise it rebuilds the list when a
//!   rebuild is pending and refreshes the durations when not.
//! * The rebuild clears the list and reads the enchantments in effect. For each one whose spell
//!   (the enchantment id `& 0xFFFF`) has a spell base and matches the panel's polarity, it inserts a row at the
//!   alphabetical position by spell name, renumbering the rows after it, and gives the row
//!   `(duration + start time) - now` as its duration. A selected spell that is no longer
//!   enchanted is dropped, and the selection display is refreshed.
//!
//! **Every input exists elsewhere; this module joins them.** `dereth_client_model::enchant` stores
//! the enchantment id, category, power level, start time and duration, and
//! `EnchantmentRegistry::enchantments_in_effect` is
//! the quality table's enchantments-in-effect read; the duration formatter
//! [`super::inforegion::format_duration`] is the client's
//! `"%d:%02d"` / `"%d:%02d:%02d"`; the row
//! template and its three children are the same info-region shape `super::skills` already builds.
//! This module is presentation and a filter, and that is why it is the cheapest of the five.
//!
//! # Two facts measured off the shipped tree rather than taken from the catalogue
//!
//! Building `classic_gameplay` and reading both instances back:
//!
//! * The client binds `0x10000126` as the panel's **info text** (a `TextElement`, type `0x0C`) and
//!   `0x10000123` as its **list box** (type `0x05`). The second child is the info line under the
//!   list, not a row template.
//! * The list box carries exactly **one** template, `(0x2100001B, 0x10000128)` — so
//!   template 0 is the only row shape, which is what the info region's constructor always
//!   passes.
//!
//! # What is here and what is not
//!
//! The list, the filter, the sort, the duration column and the two info-line strings are here.
//! A click on a row selects it — the row's state changes `1` to `6`, exactly as the client's loop
//! does — and the info line becomes the spell name + two newlines + the spell description, both of
//! which
//! [`EffectEntry`] carries from the spell base.
//!
//! **What is not here**: nothing writes the client's next-duration-update one-second throttle. The
//! client redraws the duration column once a second off global message 3; this rebuilds it on the
//! frame the value actually changed, which is the same picture at a finer grain and costs a string
//! compare per row.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{EffectEntry, GameView};

/// The effects UI type — the layout attribute that splits the two instances apart.
///
/// The same id as `EffectsIndicator`'s own `0x1000000C`, and for the same reason: the
/// lamp and the panel are told which polarity they are by the layout, not by their class.
pub const EFFECTS_UI_TYPE: u32 = 0x1000_000C;

/// The two values the effects panel's polarity filter accepts.
pub mod kind {
    /// UI type `1` — keeps a spell whose bitfield `& 4` is **set**.
    pub const HELPFUL: u32 = 1;
    /// UI type `2` — keeps a spell whose bitfield `& 4` is **clear**.
    pub const HARMFUL: u32 = 2;
}

/// The helpful instance — the `0x57` listener for input action `0x10000006`
/// *"Show/Hide Positive Magic Panel"*, which the lamp at `0x100000F5` fires.
pub const HELPFUL_PANEL: ElementId = ElementId(0x1000_0184);
/// The harmful instance — action `0x10000007`, lamp `0x100000F6`.
pub const HARMFUL_PANEL: ElementId = ElementId(0x1000_0185);

/// The panel's list box, bound at initialization — a `ListBox`.
pub const LIST_BOX: ElementId = ElementId(0x1000_0123);
/// The panel's info text, bound at initialization — a `TextElement`. **Not a row template**;
/// see the module header.
pub const INFO_TEXT: ElementId = ElementId(0x1000_0126);

/// The one entry of the list box's template list — the info region's constructor always passes
/// template index 0.
pub const ROW_TEMPLATE: usize = 0;

/// The three children the client looks up on a fresh row. The same
/// three ids `super::skills::row` names, because it is the same base class.
pub mod row {
    /// Set from the spell's icon.
    pub const ICON: u32 = 0x1000_0129;
    /// The label text — the spell name.
    pub const LABEL: u32 = 0x1000_012A;
    /// The value text — the duration, through `super::inforegion::format_duration`.
    pub const VALUE: u32 = 0x1000_012B;
}

/// The row-id attribute — the token index a row carries, which the client reads back as an
/// integer attribute.
pub const ATTR_ROW_INDEX: u32 = 0x1000_003A;

/// The client's two row states: `1` for an unselected row and `6` for the
/// selected one.
pub mod row_state {
    pub const UNSELECTED: u32 = 1;
    pub const SELECTED: u32 = 6;
}

/// The two strings the selection display puts on the info text when there is nothing to
/// describe. Table enum `0x10000001`, which resolves to `0x23000001`.
pub mod string {
    /// *"NO SPELLS"* — no rows at all.
    pub const NO_SPELLS: &str = "ID_Effects_Info_NoSpells";
    /// *"SELECT A SPELL"* — rows, but none selected.
    pub const SELECT_A_SPELL: &str = "ID_Effects_Info_SelectASpell";
}

/// One row on screen, read back without walking the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    pub spell: u32,
    pub name: String,
    /// The spell description — the info line's second half.
    pub description: String,
    /// What the value text reads — [`super::inforegion::format_duration`]'s output, or the empty
    /// string when the row is permanent or the **Spell Duration** character option is off.
    pub duration: String,
    pub element: ElemHandle,
}

/// One `EffectsPanel` instance.
#[derive(Debug, Default)]
pub struct EffectsPanel {
    /// The panel element. `None` when the layout has no such sub-panel.
    pub panel: Option<ElemHandle>,
    /// The effects UI type, read out of the layout at initialization. `0` when the element is
    /// unbound, which matches no spell and is what keeps an unbound panel empty rather than full.
    pub ui_type: u32,
    list: Option<ListBoxWidget>,
    info_text: Option<ElemHandle>,
    /// The selected spell id, `0` for none.
    pub selected_spell: u32,
    /// The rows (tokens), in list order — the pane read back.
    pub rows: Vec<EffectRow>,
    /// What the info text last read.
    pub info: String,
    /// How many times [`Self::rebuild`] has run.
    pub rebuilds: u32,
    /// The snapshot the last rebuild was made from, so an invisible panel and an
    /// unchanged registry are both no-ops.
    seen: Option<Vec<(u32, bool)>>,
    /// Whether the panel was visible on the previous frame, so that the `0x18` edge
    /// the client's element-message handler sees can be reconstructed from a poll. See
    /// [`Self::update`].
    was_visible: bool,
}

impl EffectsPanel {
    /// The effects panel's initialization, minus the two notice registrations: this build
    /// has no notice bus for the magic notices, and the two handlers' bodies are
    /// [`Self::recv_enchantments_changed`].
    ///
    /// `panel` is the instance to bind — [`HELPFUL_PANEL`] or [`HARMFUL_PANEL`]. Bound off the
    /// screen root, like [`super::house::HousePanel::post_init`], because both are pages of
    /// `<PANS>` and the child lookup is recursive.
    ///
    /// **The UI type is read from the element, not passed in.** The client's initialization reads
    /// enum attribute `0x1000000C` off the panel and so does this; a caller that supplied
    /// the polarity itself could disagree with the layout, which lights the debuff panel with
    /// buffs.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle, panel: ElementId) {
        self.panel = ui.get_child_recursive(root, panel);
        let Some(p) = self.panel else { return };
        self.ui_type = ui
            .node(p)
            .and_then(|n| n.merged_properties().get_enum(EFFECTS_UI_TYPE))
            .unwrap_or(0);
        self.info_text = ui.get_child_recursive(p, INFO_TEXT);
        self.list = ui
            .get_child_recursive(p, LIST_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.was_visible = self.visible(ui);
    }

    /// Whether `post_init` found the panel element.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether both children initialization binds are in the shipped layout too.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.list.is_some() && self.info_text.is_some()
    }

    /// The panel's visible flag — the inner update's first line.
    #[must_use]
    pub fn visible(&self, ui: &UiSystem) -> bool {
        self.panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// The spell ids on screen, in list order — what a test asserts against.
    #[must_use]
    pub fn shown(&self) -> Vec<u32> {
        self.rows.iter().map(|r| r.spell).collect()
    }

    /// The effects panel's polarity filter: no spell base, no match; otherwise UI type `1` keeps a
    /// spell whose bitfield `& 4` is set and UI type `2` one whose bit is clear.
    ///
    /// A UI type of anything else — including the `0` an unbound panel carries — matches
    /// **nothing**, which is the client's behaviour and not a guard added here.
    #[must_use]
    pub fn matches_ui_type(&self, beneficial: bool) -> bool {
        (self.ui_type == kind::HELPFUL && beneficial)
            || (self.ui_type == kind::HARMFUL && !beneficial)
    }

    /// The effects panel's two enchantments-changed notice handlers. Both mark a rebuild pending
    /// and run the update.
    ///
    /// Here it drops the snapshot, so the next [`Self::update`] rebuilds.
    pub fn recv_enchantments_changed(&mut self) {
        self.seen = None;
    }

    /// One frame's drive — the visibility edge, then the update.
    ///
    /// Returns true on a frame that rewrote the pane.
    ///
    /// **The visibility edge is polled rather than delivered.** In the client
    /// the element-message handler's `0x18` arm is what runs the update when the panel is shown,
    /// and the element's visibility setter raises it whenever the element's own visible flag
    /// changes (on hiding as well as showing); this crate's panels are plain structs that
    /// receive no `0x18` of their own, so the edge is reconstructed from [`Self::visible`]. The
    /// observable is the same one the client has — a panel that has just become visible rebuilds —
    /// and it is why a click on the lamp fills the list on the very next frame rather than on the
    /// next enchantment change.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if !self.fully_bound() {
            return false;
        }
        let visible = self.visible(ui);
        let shown_now = visible && !self.was_visible;
        self.was_visible = visible;
        // The inner update's first line. An invisible panel does no work at all, which is also
        // why the client unregisters global message 3 on the way down.
        if !visible {
            return false;
        }
        if shown_now {
            // The `0x18` arm's rebuild: the pane may have been built against a registry that has
            // moved since it was last up.
            self.seen = None;
        }
        let effects = view.active_effects();
        let snapshot: Vec<(u32, bool)> = effects.iter().map(|e| (e.spell, e.beneficial)).collect();
        if self.seen.as_ref() == Some(&snapshot) {
            // No rebuild pending — the client still refreshes the durations once a second
            // off global message 3. That is [`Self::update_durations`].
            return self.update_durations(ui, &effects, view);
        }
        self.seen = Some(snapshot);
        self.rebuild(ui, &effects, view);
        true
    }

    /// The effects panel's list rebuild, with the list already gathered.
    ///
    /// Separated so a test can drive it with a list it names, the way
    /// [`super::skills::SkillsPanel::rebuild`] is.
    pub fn rebuild(&mut self, ui: &mut UiSystem, effects: &[EffectEntry], view: &dyn GameView) {
        if self.list.is_none() {
            return;
        }
        // The filter — the polarity test per entry, and nothing else decides membership.
        let ui_type = self.ui_type;
        let mut keep: Vec<&EffectEntry> = effects
            .iter()
            .filter(|e| {
                (ui_type == kind::HELPFUL && e.beneficial)
                    || (ui_type == kind::HARMFUL && !e.beneficial)
            })
            .collect();
        // The sorted-insertion-place lookup: each new row is inserted before the first existing
        // row whose spell name does not compare *after* it, i.e. the list ends up **ascending by
        // spell name**. The insertion walk is that sort done one element at a time; done as one
        // sort here, with the spell id as the tie-break so the order is total.
        keep.sort_by(|a, b| a.name.cmp(&b.name).then(a.spell.cmp(&b.spell)));

        // SpellDuration — the effect info region's update returns
        // **before** writing the value cell when the option is off, so the duration column is
        // blank rather than zero. Read once per rebuild, as the client reads it once per row.
        let show_duration = view.player_option(crate::view::PlayerOption::SpellDuration);

        let Some(list) = self.list.as_mut() else {
            return;
        };
        // The effects panel's list flush.
        list.flush(ui);
        self.rows.clear();
        for (i, e) in keep.iter().enumerate() {
            let Some(h) = list.add_from_template(ui, ROW_TEMPLATE, Some(i)) else {
                continue;
            };
            // The row id — the token index, which the client reads back off the row.
            ui.set_attribute_int(h, ATTR_ROW_INDEX, i32::try_from(i).unwrap_or(i32::MAX));
            super::skills::set_row_icon(ui, h, e.icon, dereth_ui::ImageSource::World);
            if let Some(t) = ui
                .get_child_recursive(h, ElementId(row::LABEL))
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&e.name);
            }
            let duration = duration_cell(e, show_duration);
            if let Some(t) = ui
                .get_child_recursive(h, ElementId(row::VALUE))
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&duration);
            }
            self.rows.push(EffectRow {
                spell: e.spell,
                name: e.name.clone(),
                description: e.description.clone(),
                duration,
                element: h,
            });
        }
        // The rebuild's tail: a selection that is no longer enchanted is dropped.
        if self.selected_spell != 0 && !self.rows.iter().any(|r| r.spell == self.selected_spell) {
            self.selected_spell = 0;
        }
        self.update_selection(ui);
        if let Some(list) = self.list.as_mut() {
            list.update_layout(ui);
        }
        self.rebuilds += 1;
    }

    /// The effects panel's duration update — every row's info-region update, and then the next
    /// update is scheduled one second on.
    ///
    /// Returns true when any cell changed, so the caller's frame count means "something moved".
    pub fn update_durations(
        &mut self,
        ui: &mut UiSystem,
        effects: &[EffectEntry],
        view: &dyn GameView,
    ) -> bool {
        let show_duration = view.player_option(crate::view::PlayerOption::SpellDuration);
        let mut moved = false;
        for r in &mut self.rows {
            let Some(e) = effects.iter().find(|e| e.spell == r.spell) else {
                continue;
            };
            let d = duration_cell(e, show_duration);
            if d == r.duration {
                continue;
            }
            r.duration.clone_from(&d);
            if let Some(t) = ui
                .get_child_recursive(r.element, ElementId(row::VALUE))
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&d);
            }
            moved = true;
        }
        moved
    }

    /// Every row's state, then the info line.
    ///
    /// The selected arm is the spell name + two newlines + the spell description, both of which
    /// the row carries.
    pub fn update_selection(&mut self, ui: &mut UiSystem) {
        for r in &self.rows {
            let s = if r.spell == self.selected_spell && self.selected_spell != 0 {
                row_state::SELECTED
            } else {
                row_state::UNSELECTED
            };
            ui.set_state(r.element, dereth_ui::StateId(s));
        }
        let text = if self.rows.is_empty() {
            super::statmgmt::label(ui, string::NO_SPELLS)
        } else if self.selected_spell == 0 {
            super::statmgmt::label(ui, string::SELECT_A_SPELL)
        } else {
            // Name, `"\n\n"`, description — the two newlines are the client's own literal.
            self.rows
                .iter()
                .find(|r| r.spell == self.selected_spell)
                .map(|r| format!("{}\n\n{}", r.name, r.description))
                .unwrap_or_default()
        };
        self.info.clone_from(&text);
        if let Some(t) = self.info_text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&text);
        }
        // **The spell description's scrollbar.** Nothing is missing from the layout and nothing is
        // missing from the binding. The shipped tree carries both halves [measured on the shipped
        // layout]:
        // The info text `0x10000126` is a 280x78 `TextElement` declaring attribute
        // **`0x72 = 0x10000127`**, and `0x10000127` is a real `Scrollbar` (type `0x0B`)
        // in the 15 px column to its right shipping `0x76 disabled = true`, `0x79
        // hide-when-disabled = true` and `0x82 proportional`. Both `EffectsPanel` instances carry
        // the identical pair.
        //
        // If the glyph-list recalculation never ran after the `set_text` above, the pane would
        // report a content extent of **0 x 0** however many lines the description held — so the
        // scrollbar sizing would see `content <= view`, set the bar disabled, and
        // `hide-when-disabled` would take it off the screen.
        //
        // No hand call is needed here. Setting the text raises a needs-recalculation flag (`0x100`) and stops, exactly as
        // it does in retail, and the consumer is now the draw: the client recalculates the glyph
        // list of every visible region each frame, and `UiSystem::draw` runs the same sweep from
        // the root, so every text pane (this one, the appraisal pane, the burden sheet, the
        // character wizard) is covered by the one pass.
        //
        // The offset is still not reset: the recalculation ends by re-applying the old scroll
        // offset, forced, and the scrollbar sizing only zeroes it when the new content fits.
        // Selecting a second, shorter spell therefore keeps the scroll position exactly as retail
        // does.
    }

    /// The effects panel's element-message handler's `0x1C` arm — a press (`p1 == 7`) on the list
    /// box `0x10000123` selects the row under the mouse through the selected-spell write.
    ///
    /// # The row is resolved **by point**, never by the message's source.
    ///
    /// Walking `m.source` upwards looking for one of [`Self::rows`] would answer `None` for
    /// every press a player can make: **a list row is not mouse-visible**, in this build or in
    /// retail, so the hit test resolves a press over a row to the **list
    /// box** and the message names `0x10000123`. The client's own answer is to
    /// ask the list for the item under the mouse, tested against the **list's** rectangle and the
    /// global cursor, never against the message's element.
    /// [`super::listbox::ListBoxWidget::item_under_mouse`] is that function; the two stat panels
    /// use it for the same reason.
    ///
    /// `m.point.window` is the global cursor position the client reads; the message was stamped
    /// from the same source.
    ///
    /// **A press on the empty part of the list clears the selection**, because
    /// the selected-spell write treats no row as spell `0`. That is
    /// the client's behaviour, and declining the message instead would leave a selection standing
    /// that retail drops.
    ///
    /// Returns true when the message was this panel's.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if m.id.0 != 0x1C || m.p1 != 7 {
            return false;
        }
        let Some(list) = self.list.as_ref() else {
            return false;
        };
        // The message must name the list box `0x10000123` — and then `list.owns`, because the two
        // instances share every child id and only the subtree tells them apart.
        if !list.owns(ui, m.source) {
            return false;
        }
        // The list box's item-under-mouse read, then the selected-spell write reads the row's
        // `0x1000003A` attribute as the token index. The token index is the row's
        // position in the list, which is what [`Self::rows`] is ordered by.
        let (wx, wy) = m.point.window;
        let spell = list
            .item_under_mouse(ui, wx, wy)
            .and_then(|h| self.rows.iter().find(|r| r.element == h))
            .map_or(0, |r| r.spell);
        // Pressing the already-selected spell selects `0` — the press on the selected row is a
        // toggle, and the press on nothing is a deselect.
        self.selected_spell = if self.selected_spell == spell {
            0
        } else {
            spell
        };
        self.update_selection(ui);
        true
    }
}

/// The effect info region's update and the duration read, as a string.
///
/// * The option off — SpellDuration false — writes **nothing**: the client returns
///   true before touching the value cell, so the column is blank rather than `0:00`.
/// * A permanent enchantment (duration `< 0`) has no meaningful remaining time; the duration read
///   would return a large negative and [`super::inforegion::format_duration`] clamps at zero, so
///   it is blanked here on the same test [`crate::view::EffectEntry::permanent`] carries.
/// * Everything else is `format_duration(remaining)`, the two shapes, with the seconds **already
///   rebased on receipt**: the server's times are converted to the local clock when they arrive.
#[must_use]
pub fn duration_cell(e: &EffectEntry, show_duration: bool) -> String {
    if !show_duration || e.permanent {
        return String::new();
    }
    #[allow(clippy::cast_possible_truncation)]
    super::inforegion::format_duration(e.remaining as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(spell: u32, name: &str, beneficial: bool, remaining: f64) -> EffectEntry {
        EffectEntry {
            spell,
            name: name.to_owned(),
            description: String::new(),
            icon: None,
            beneficial,
            remaining,
            permanent: false,
            category: 0,
            power_level: 1,
        }
    }

    /// Oracle: the polarity filter, both arms and the third case
    /// the client leaves falling through to `false`.
    #[test]
    fn the_polarity_filter_is_the_bitfield_bit_and_nothing_else() {
        let mut p = EffectsPanel {
            ui_type: kind::HELPFUL,
            ..EffectsPanel::default()
        };
        assert!(p.matches_ui_type(true));
        assert!(!p.matches_ui_type(false));
        p.ui_type = kind::HARMFUL;
        assert!(!p.matches_ui_type(true));
        assert!(p.matches_ui_type(false));
        // An unbound panel carries `0`, which is neither arm: it matches nothing, so a panel whose
        // element is missing draws empty rather than drawing everything.
        p.ui_type = 0;
        assert!(!p.matches_ui_type(true));
        assert!(!p.matches_ui_type(false));
    }

    /// Oracle: the effect info region's update's SpellDuration
    /// early return, and its two shapes through
    /// [`super::super::inforegion::format_duration`].
    #[test]
    fn the_duration_cell_is_blank_without_the_option_and_blank_when_permanent() {
        let a = e(1, "Strength Self VI", true, 75.0);
        assert_eq!(duration_cell(&a, true), "1:15");
        assert_eq!(
            duration_cell(&a, false),
            "",
            "SpellDuration off writes no value at all"
        );
        let mut perm = a.clone();
        perm.permanent = true;
        assert_eq!(
            duration_cell(&perm, true),
            "",
            "_duration < 0 has no remaining time"
        );
        let long = e(2, "x", true, 3661.0);
        assert_eq!(duration_cell(&long, true), "1:01:01");
    }

    /// Oracle: `crate::panels::catalogue`'s `EffectsPanel` row against the ids initialization
    /// binds.
    #[test]
    fn the_two_bound_children_are_the_catalogued_ones() {
        let spec =
            crate::panels::catalogue::spec("EffectsPanel").expect("EffectsPanel is catalogued");
        let ids: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
        assert!(ids.contains(&LIST_BOX.0), "the list box 0x10000123");
        assert!(ids.contains(&INFO_TEXT.0), "the info text 0x10000126");
    }
}
