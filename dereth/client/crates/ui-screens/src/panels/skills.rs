//! `SkillsPanel` — the skills page's four groups and its skill rows.
//!
//! Sources: the skill panel's list rebuild and sorted insertion, the stat-management
//! panel's list flush, the skill info region's construction and update, and the retail toolbar
//! and panel behaviour.
//!
//! # What the client actually does
//!
//! ```text
//! skill list rebuild:
//!   flush the list                                        (every row deleted)
//!   look up the SkillTable, 0x0E000004
//!   append header templates 1..4                          "Specialized", "Trained",
//!                                                         "Untrained", the fourth group
//!   for each (skill, base) in the table                   (the TABLE, not the player's skills)
//!       read the player's advancement class for it
//!         specialized -> sorted insert between header 1 and header 2
//!         trained     -> sorted insert between header 2 and header 3
//!         untrained   -> min level > 1 ? after header 4 : between header 3 and header 4
//!         otherwise   -> after header 4
//!   clear the selected index (-1)
//! ```
//!
//! Three things in that are easy to get wrong and each changes what a player sees:
//!
//! * **The walk is over the `SkillTable`, not over the player's skills.** A skill the server
//!   never mentioned still gets a row — in the fourth group, because its advancement class reads
//!   as `UNDEF`. In the shipped `0x0E000004` the two sets happen to coincide — the table has 38
//!   skills and a recorded login's `0x0013` sends all 38 — so this build cannot
//!   *demonstrate* the difference; it is transcribed because the direction of the join decides
//!   what a server that omits a skill would show.
//! * **The min level splits the untrained skills in two.** An untrained skill whose `SkillBase`
//!   min level is above 1 goes into the *fourth* group with the undefined ones, not into
//!   "Untrained". That is the shipped data's way of saying "you cannot train this yet".
//! * **The four groups are four rows of the same list box.** They are not four list boxes and not
//!   four containers: the sorted insert finds the header's index in the list's items and inserts
//!   between it and the next header, so the whole page is one flat `ListBox`.
//!
//! # The sort inside a group
//!
//! The sorted skill insert reads the skill's name — which is the `SkillTable` entry's name and
//! **not** a string-table lookup — walks from the
//! header's index to the next header's, reads each existing row's label back off it, and stops at
//! the first `wcscmp(existing, new) < 0`. That is an insertion sort by name, ascending, inside the
//! group. Sorting the group once is the same list.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::listbox::ListBoxWidget;
use super::statmgmt::{self, button_state, child, row_state, state, Footer, FooterContent};
use crate::view::{GameView, SkillAdvancement, SkillEntry, UiRequest};

/// `SkillsPanel` — the skills sub-panel of the character page `0x1000018E`, type `0x1000002B`.
///
/// **The search has to start here.** `AttributesPanel` (`0x1000022B`, type `0x1000002A`) is the
/// sibling sub-panel of the same page and it carries the *same* stat-management child ids,
/// `0x1000023D` included — both sub-panels inherit the shared stat-management layout.
/// A recursive lookup of `0x1000023D` from the page or from the screen root finds the attribute
/// page's list box first and fills that instead. [verified against the live element
/// tree: `0x1000023D` appears twice under `0x1000018E`, once under each sub-panel]
pub const PANEL: ElementId = ElementId(0x1000_022C);

/// The list box — the one list every group and every row lives in.
pub const LIST_BOX: ElementId = ElementId(0x1000_023D);

/// The template-list index of each of the four group headers — the rebuild's four header
/// appends, in order.
///
/// Template **0** is the skill row itself: the sorted insert always passes 0.
/// The shipped `0x1000023D` carries five entries, all in layout `0x21000045`
/// (`0x10000248`…`0x1000024C`), which is exactly one row template and four headers. [verified
/// against the live element tree]
pub const HEADER_TEMPLATES: [usize; 4] = [1, 2, 3, 4];
/// The template-list index of a skill row.
pub const ROW_TEMPLATE: usize = 0;

/// The three children the client looks up on the row it just created.
pub mod row {
    /// The row's icon — set to the icon (draw mode 3) when the DataID is not `INVALID_DID`.
    pub const ICON: u32 = 0x1000_0129;
    /// The label text — the row's construction writes the skill name into it.
    pub const LABEL: u32 = 0x1000_012A;
    /// The value text — the row's update writes the skill value into it.
    pub const VALUE: u32 = 0x1000_012B;
}

/// `SkillAdvancementClass`, as the rebuild switches on it: the raw values of
/// [`dereth_rules::skills::Sac`], so the panel and the rules cannot drift apart.
pub mod sac {
    use dereth_rules::skills::Sac;
    pub const UNDEF: u32 = Sac::Undef as u32;
    pub const UNTRAINED: u32 = Sac::Untrained as u32;
    pub const TRAINED: u32 = Sac::Trained as u32;
    pub const SPECIALIZED: u32 = Sac::Specialized as u32;
}

/// Which of the four groups a skill lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillGroup {
    /// Header template 1.
    Specialized,
    /// Header template 2.
    Trained,
    /// Header template 3 — untrained **and** min level `<= 1`.
    Untrained,
    /// Header template 4 — everything else: `UNDEF`, and untrained with min level `> 1`.
    Unusable,
}

impl SkillGroup {
    /// The four, in the rebuild's header order.
    pub const ALL: [Self; 4] = [
        Self::Specialized,
        Self::Trained,
        Self::Untrained,
        Self::Unusable,
    ];

    /// The rebuild's switch on the advancement class, including the min-level split: an
    /// untrained skill goes between headers 3 and 4 unless its min level is above 1, in which
    /// case it goes after header 4 with the undefined ones.
    #[must_use]
    pub const fn of(sac: u32, min_level: u32) -> Self {
        match sac {
            sac::SPECIALIZED => Self::Specialized,
            sac::TRAINED => Self::Trained,
            sac::UNTRAINED if min_level <= 1 => Self::Untrained,
            _ => Self::Unusable,
        }
    }

    /// The header's template-list index.
    #[must_use]
    pub const fn header_template(self) -> usize {
        match self {
            Self::Specialized => 1,
            Self::Trained => 2,
            Self::Untrained => 3,
            Self::Unusable => 4,
        }
    }
}

/// One built row, kept so a later update can rewrite the value without rebuilding the list.
#[derive(Debug, Clone)]
pub struct SkillRow {
    pub skill: u32,
    /// The skill name, kept so the footer's title does not need the
    /// `SkillTable` again.
    pub name: String,
    pub group: SkillGroup,
    pub element: ElemHandle,
    /// What the update last wrote into the value text — the **enchanted** total, not the raw
    /// level; see `write_row`.
    pub value: i32,
    /// `0` plain, `1` buffed, `2` debuffed — the colour index the update writes the value with.
    pub font: u32,
}

/// `SkillsPanel` — the live skills list.
#[derive(Debug, Default)]
pub struct SkillsPanel {
    /// The list box.
    pub list: Option<ListBoxWidget>,
    /// The four group headers, in [`SkillGroup::ALL`] order, as far as the templates provided.
    pub headers: Vec<(SkillGroup, ElemHandle)>,
    /// One skill row per list row, in list order.
    pub rows: Vec<SkillRow>,
    /// The selected index, which the rebuild resets to -1 on every rebuild.
    pub selected_index: i32,
    /// The selected skill — 0 is "nothing selected", which is why skill id 0 could never
    /// be selectable and why the selection change uses 0 as its deselect sentinel.
    pub selected_skill: u32,
    /// The shared stat-management footer, bound off the sub-panel.
    pub footer: Option<Footer>,
    /// What the last footer render put on screen, kept so a test can read the numbers back.
    pub footer_content: FooterContent,
    /// The same for the header above the list — the shared name, level, and experience
    /// block.
    pub header_content: statmgmt::HeaderContent,
    /// The awaiting-raise latch — set when a `Train_*` goes out and cleared when the
    /// server answers. **This is the double-spend latch**: the
    /// client never touches its local skill copy on a raise, it waits for the changed quality.
    pub awaiting_raise: bool,
    /// The **whole** `SkillEntry` list the panel was last built for, so an unchanged frame
    /// rebuilds nothing.
    ///
    /// *An early-out is safe exactly when the compared value is a **superset** of the rebuild's
    /// inputs, and unsafe the moment the snapshot is a **projection** the fill loop reaches past.*
    /// [`rebuild`](SkillsPanel::rebuild) reads all eight of a [`SkillEntry`]'s fields: `sac` and `min_level` pick
    /// the group, `name` and `id` order it, and [`write_row`] draws `icon`, `name`, `effective`
    /// and `value_font(effective, level, vitae)`. Some of those move on their own in ordinary
    /// play — `effective` alone when an enchantment lands or expires, `vitae` alone on death and
    /// recovery — so a guard over only `(id, sac, level)` would keep a 72->77 buff off the screen.
    ///
    /// **The guard is the whole value and not a wider tuple, deliberately.** A tuple carrying
    /// `effective` and `vitae` would still omit `name`, `icon` and `min_level`, which is the same
    /// mistake one field narrower. Comparing `&[SkillEntry]` against the stored `Vec` also needs
    /// no per-frame allocation: the clone happens only on a frame that actually rebuilds.
    ///
    /// Retail's rebuild has no guard of any kind to be faithful to — it is
    /// notice-driven (player description received, skill advancement class changed) and
    /// recomputes unconditionally — so the only correctness question here is whether the guard can
    /// see everything the client's notice would have redrawn.
    last: Option<Vec<SkillEntry>>,
}

impl SkillsPanel {
    /// The client's one load-bearing binding for this panel — the list box `0x1000023D` —
    /// resolved from [`PANEL`] and not from the
    /// page. `page` is the character page `0x1000018E`.
    pub fn post_init(&mut self, ui: &mut UiSystem, page: ElemHandle) {
        let panel = ui.get_child_recursive(page, PANEL).unwrap_or(page);
        self.list = ui
            .get_child_recursive(panel, LIST_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        // The stat-management footer hangs off the **sub-panel**, whose state
        // picks which of the three stacked containers the eight getters read.
        self.footer = Some(Footer::new(panel));
        self.headers.clear();
        self.rows.clear();
        self.selected_index = -1;
        self.selected_skill = 0;
        self.footer_content = FooterContent::default();
        self.header_content = statmgmt::HeaderContent::default();
        self.awaiting_raise = false;
        self.last = None;
    }

    /// How many elements the list has created since `post_init` — zero is the defect.
    ///
    /// This counts **every** successful template append, so a full rebuild of `n` skills
    /// leaves it at `HEADER_TEMPLATES.len() + n`; it is cumulative across rebuilds because
    /// `flush` empties the list's items without unbinding the widget, and only `post_init` re-binds.
    ///
    /// **This is not redundant against [`Self::create_failures`], and the difference
    /// is the whole reason it exists.** `ListBoxWidget::add_from_template` has *three* outcomes,
    /// not two: it can create a row, it can fail to create one (`create_failures += 1`), or it can
    /// find no template at that index at all — in which case it returns `None` and **increments
    /// neither counter**. So a skills page whose `0x1000023D` came up with no template list reads
    /// `create_failures == 0`, exactly like a page that built all forty-two of its elements. Only
    /// a positive `rows_created` separates the two: the instrument has a third state — yes, no,
    /// and **not asked**.
    #[must_use]
    pub fn rows_created(&self) -> u32 {
        self.list.as_ref().map_or(0, |w| w.created)
    }

    /// Template appends that resolved a template and still produced no element —
    /// `ListBoxWidget::create_failures`, surfaced so the skills panel can be asserted the way
    /// `drag.rs`, `panels.rs` and `quickbar.rs` already assert the lists they own.
    ///
    /// Read this **with** [`Self::rows_created`] and never instead of it; see that method for why
    /// a zero here is not evidence the list built anything.
    #[must_use]
    pub fn create_failures(&self) -> u32 {
        self.list.as_ref().map_or(0, |w| w.create_failures)
    }

    /// The skill panel's skill-list rebuild, run against the current world.
    ///
    /// Returns true on a frame that actually rebuilt the list. The guard is the panel's own
    /// snapshot: the client's version is notice-driven (player description received and skill
    /// advancement class changed), not per-frame.
    ///
    /// **The compared value is the entire `&[SkillEntry]` the rebuild is a pure function of** —
    /// see [`SkillsPanel::last`] for why it is not a projection.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let skills = view.skills();
        if self.last.as_deref() == Some(skills) {
            return false;
        }
        self.rebuild(ui, skills);
        self.last = Some(skills.to_vec());
        true
    }

    /// The stat management panel's character info update + the experience update.
    ///
    /// Driven every frame the header's inputs change, which is how the
    /// player-description-received and quality-changed notices reach it in the client.
    pub fn update_header(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(footer) = self.footer else {
            return false;
        };
        // The eight fields are gathered once, by `HeaderInputs::gather`, because both
        // stat-management sub-panels draw the same eight fields from their own copies.
        let c = footer.update_header(ui, &statmgmt::HeaderInputs::gather(view));
        if self.header_content == c {
            return false;
        }
        self.header_content = c;
        true
    }

    /// The body of [`Self::update`], separated so a test can drive it with a list it names.
    pub fn rebuild(&mut self, ui: &mut UiSystem, skills: &[SkillEntry]) {
        let Some(list) = self.list.as_mut() else {
            return;
        };
        // The stat management panel's list flush.
        list.flush(ui);
        self.headers.clear();
        self.rows.clear();
        // The rebuild's tail clears the selected index. The selected *skill* survives a
        // rebuild in the client (it is a skill id, not an index) and is re-found by
        // the selection update's loop, so only the index is cleared here.
        self.selected_index = -1;
        // The rebuild's player-description guard: the four header appends and the whole walk
        // sit **inside** it, so before `0x0013`
        // the client leaves the list flushed rather than drawing four empty group headers.
        if skills.is_empty() {
            return;
        }

        // The four headers, appended in order — templates 1..4.
        for g in SkillGroup::ALL {
            if let Some(h) = list.add_from_template(ui, g.header_template(), None) {
                self.headers.push((g, h));
            }
        }

        // The sorted insert's per-group alphabetical order, done as one sort per group.
        let mut grouped: Vec<(SkillGroup, Vec<&SkillEntry>)> =
            SkillGroup::ALL.iter().map(|g| (*g, Vec::new())).collect();
        for s in skills {
            let g = SkillGroup::of(s.sac, s.min_level);
            if let Some(slot) = grouped.iter_mut().find(|(k, _)| *k == g) {
                slot.1.push(s);
            }
        }
        for (_, v) in &mut grouped {
            v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        }

        // Insert each group's rows immediately after its header, in order. Walking the groups
        // back to front keeps the earlier headers' indices valid without recomputing them, which
        // is what the client gets for free by looking the header up again on every call.
        for (g, entries) in grouped.iter().rev() {
            let Some((_, header)) = self.headers.iter().find(|(k, _)| k == g) else {
                continue;
            };
            let Some(at) = list.index_of(*header) else {
                continue;
            };
            for (i, s) in entries.iter().enumerate() {
                let Some(h) = list.add_from_template(ui, ROW_TEMPLATE, Some(at + 1 + i)) else {
                    continue;
                };
                write_row(ui, h, s);
                self.rows.push(SkillRow {
                    skill: s.id,
                    name: s.name.clone(),
                    group: *g,
                    element: h,
                    value: s.effective,
                    font: value_font(s.effective, s.level, s.vitae),
                });
            }
        }
        // The rows were pushed back-to-front; put them in list order so `rows` reads like the
        // screen does.
        self.rows
            .sort_by_key(|r| list.index_of(r.element).unwrap_or(usize::MAX));
        list.update_layout(ui);
    }

    /// The skill ids on screen, in list order — what a test asserts against.
    #[must_use]
    pub fn shown(&self) -> Vec<u32> {
        self.rows.iter().map(|r| r.skill).collect()
    }

    // ---- selection, the footer, and the two raise buttons -----------------------------------

    /// The stat-management panel's element-message handler, the two arms that matter here.
    ///
    /// On `0x1C` (`MOUSE_PRESS`), with a list box, the item under the mouse is selected when `p1`
    /// is 7, 8, 10 or 11. On `1` (`BUTTON_CLICKED`), unless a raise is awaited, `0x10000246`
    /// raises the selection and `0x100005EB` raises it by ten.
    ///
    /// **The selection arm is `0x1C MOUSE_PRESS`, not `0x19 MOUSE_CLICK`.** The gameplay screen
    /// listens on `MOUSE_CLICK` and routes it to the inventory alone, so a `MOUSE_CLICK` arm here
    /// would never select a row.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::MOUSE_PRESS {
            if !SELECT_ACTIONS.contains(&m.p1) {
                return false;
            }
            let Some(item) = self.item_under(ui, m) else {
                return false;
            };
            self.set_selection(ui, Some(item), view);
            return true;
        }
        if m.id == msg::BUTTON_CLICKED {
            // Both footer button ids exist twice in the live tree, once under
            // `SkillsPanel` and once under `AttributesPanel`; without this the two panels answer for
            // each other's buttons. See [`Footer::owns`].
            if !self.footer.is_some_and(|f| f.owns(ui, m.source)) {
                return false;
            }
            if self.awaiting_raise {
                // The awaiting-raise latch gates **both** buttons, and it is the whole of the
                // double-spend guard: a second click before the server answers sends nothing.
                return false;
            }
            if m.source_id.0 == child::BUTTON {
                return self.raise_selection(ui, view);
            }
            if m.source_id.0 == child::BUTTON_10 {
                return self.raise_10_selection(ui, view);
            }
        }
        false
    }

    /// The row **under the pointer**, from the
    /// list box's own geometry.
    ///
    /// **Not by walking `m.source` upwards** looking for one of the list's items, on the
    /// assumption that a press lands on a row and bubbles. It does not, and it cannot: a row
    /// created from the template list is a `Field` (type 3) whose
    /// "should be mouse visible" is false (no context menu, no tooltip) and which no
    /// listener registers `0x1C` for, so the mouse-down event never hits
    /// it. What it hits is the **list box** `0x1000023D`, which is type 5 and folds
    /// its own "should be mouse visible" to true. Measured on the live tree: a hit
    /// test at a skill row's centre returns `0x1000023D`, the row and all three of its children
    /// read as not mouse-visible, so an ancestor walk would return `None` for every real click
    /// on either panel.
    ///
    /// The client never looks at the source. The stat-management panel's element-message
    /// handler's `0x1C` arm asks the list box for the item under the mouse, and that is
    /// pure geometry — see [`ListBoxWidget::item_under_mouse`].
    ///
    /// Tests press through `UiSystem::mouse_down` rather than broadcasting the `0x1C` on the row
    /// element by hand, which would stand in for the missing producer and hide exactly this.
    #[must_use]
    pub fn item_under(&self, ui: &UiSystem, m: &dereth_ui::ElementMessage) -> Option<ElemHandle> {
        let list = self.list.as_ref()?;
        // The element-message broadcast's bubble, which this crate fans out by hand — see
        // [`ListBoxWidget::owns`]. Both stat sub-panels' list boxes share one rectangle, so
        // geometry alone is ambiguous between them.
        if !list.owns(ui, m.source) {
            return None;
        }
        let (x, y) = m.point.window;
        list.item_under_mouse(ui, x, y)
    }

    /// The skill panel's selection change: take the pressed row's skill (0 for no row), treat
    /// the already-selected skill as 0 — clicking the selected row deselects it — store it, and
    /// update the selection.
    ///
    /// The **toggle** is the load-bearing line: a second press on the same row clears the
    /// selection and puts the default footer back.
    pub fn set_selection(
        &mut self,
        ui: &mut UiSystem,
        item: Option<ElemHandle>,
        view: &dyn GameView,
    ) {
        let mut skill = 0u32;
        if let Some(h) = item {
            // The client keeps a parallel array of row regions; `rows` is this crate's, and it
            // is indexed by the same element.
            skill = self
                .rows
                .iter()
                .find(|r| r.element == h)
                .map_or(0, |r| r.skill);
        }
        if self.selected_skill == skill {
            skill = 0;
        }
        self.selected_skill = skill;
        self.update_selection(ui, view);
    }

    /// Repaint every row's state, then the footer.
    ///
    /// For the `sac >= TRAINED` arm, see [`super::statmgmt`]'s header for what the shipped layout
    /// says about it.
    pub fn update_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        self.selected_index = -1;
        for (i, r) in self.rows.iter().enumerate() {
            let selected = r.skill == self.selected_skill && self.selected_skill != 0;
            if selected {
                self.selected_index = i32::try_from(i).unwrap_or(-1);
            }
            let s = if selected {
                row_state::SELECTED
            } else {
                row_state::UNSELECTED
            };
            ui.set_state(r.element, dereth_ui::StateId(s));
        }
        let Some(mut footer) = self.footer else {
            return;
        };
        if self.selected_skill == 0 {
            footer.set_state(ui, state::DEFAULT);
            self.footer = Some(footer);
            self.footer_content = self.display_default_footer(ui, view);
            return;
        }
        let sac = view
            .skill_advancement(self.selected_skill)
            .map_or(0, |a| a.sac);
        if sac >= sac::TRAINED {
            footer.set_state(ui, state::SELECTION_METER);
            self.footer = Some(footer);
            self.footer_content = self.display_selection_footer_trained(ui, view);
        } else {
            footer.set_state(ui, state::SELECTION);
            self.footer = Some(footer);
            self.footer_content = self.display_selection_footer_untrained(ui, view);
        }
    }

    /// The skill panel's display default footer.
    ///
    /// | slot | `StringInfo` | value |
    /// |---|---|---|
    /// | title | `ID_StatManagement_Footer_DefaultSkillTitle` | — |
    /// | line one | `..._SkillCreditsLabel` / `..._SkillCreditsValue` | int property `0x18` |
    /// | line two | `..._ExperienceLabel` / `..._ExperienceValue` | int64 property `2` |
    ///
    /// Both `*Value` rows resolve to the **empty** string with two variants — the row is one bare
    /// substitution with no literal around it, so the value cell is just the number. [verified
    /// against string table `0x23000001`]
    fn display_default_footer(&self, ui: &mut UiSystem, view: &dyn GameView) -> FooterContent {
        let Some(footer) = self.footer else {
            return FooterContent::default();
        };
        let c = FooterContent {
            title: statmgmt::label(ui, statmgmt::string::DEFAULT_SKILL_TITLE),
            title_font: 0,
            line_one_label: statmgmt::label(ui, statmgmt::string::SKILL_CREDITS_LABEL),
            line_one_value: statmgmt::num(view.skill_credits()),
            line_two_label: statmgmt::label(ui, statmgmt::string::EXPERIENCE_LABEL),
            line_two_value: statmgmt::num(view.available_experience()),
            meter: None,
            // The default container carries no buttons at all, so neither is touched.
            button: button_state::DISABLED,
            button_10: button_state::DISABLED,
            button_10_visible: false,
        };
        write_footer(ui, &footer, &c);
        c
    }

    /// The skill panel's display selection footer untrained — the **train** footer.
    ///
    /// Line one is the *skill credits to train* (the `SkillBase` trained cost), line two the
    /// credits the character has. The raise-10 button is hidden outright (first thing),
    /// and the raise button is enabled only when the cost is non-zero and affordable.
    fn display_selection_footer_untrained(
        &self,
        ui: &mut UiSystem,
        view: &dyn GameView,
    ) -> FooterContent {
        let Some(footer) = self.footer else {
            return FooterContent::default();
        };
        let adv = view
            .skill_advancement(self.selected_skill)
            .unwrap_or_default();
        let credits = u64::try_from(view.skill_credits()).unwrap_or(0);
        let c = FooterContent {
            title: self.selected_name(),
            title_font: 0,
            line_one_label: statmgmt::label(ui, statmgmt::string::SKILL_CREDITS_TO_RAISE_LABEL),
            line_one_value: statmgmt::num(adv.cost_to_raise),
            line_two_label: statmgmt::label(ui, statmgmt::string::SKILL_CREDITS_LABEL),
            line_two_value: statmgmt::num(view.skill_credits()),
            meter: None,
            button: Footer::enable_for(u64::from(adv.cost_to_raise), credits),
            button_10: button_state::DISABLED,
            button_10_visible: false,
        };
        write_footer(ui, &footer, &c);
        c
    }

    /// The skill panel's display selection footer trained — the **raise** footer.
    ///
    /// Line one is the experience to raise one level (the cost to raise), or the string
    /// `ID_StatManagement_Header_XPToLevelMeterInfinity` when that is 0 — which is what a skill at
    /// its cap shows. The meter is the fraction of the current level's XP band already paid, and
    /// both buttons are enabled against **unassigned experience** rather than credits.
    ///
    /// **The title.** Retail writes `"%s: %d"` (name, enchanted value) in colour 0;
    /// then, only when vitae is negative, appends `" (%d)"` (the vitae) in colour 3; then takes
    /// `delta = (enchanted - vitae) - raw` and, when it is non-zero, appends `" (%s%d)"` with the
    /// prefix `"+"` and colour 1 when `raw < enchanted - vitae`, and no prefix and colour 2
    /// otherwise.
    ///
    /// So it is **three runs**: the base in colour 0, an optional vitae segment in colour 3 and
    /// the buff/debuff delta in colour 1 or 2 -- the same shape `AttributesPanel`'s two selection
    /// footers have (`attributes.rs::write_selection_footer`).
    ///
    /// The comparison is [`value_font`]'s -- `raw` against `enchanted - vitae` -- and the three
    /// numbers are the [`SkillEntry`]'s `level`, `effective` and `vitae`, which are the same two
    /// skill reads and the same vitae modifier the row used.
    fn display_selection_footer_trained(
        &self,
        ui: &mut UiSystem,
        view: &dyn GameView,
    ) -> FooterContent {
        let Some(footer) = self.footer else {
            return FooterContent::default();
        };
        let adv = view
            .skill_advancement(self.selected_skill)
            .unwrap_or_default();
        let xp = u64::try_from(view.available_experience()).unwrap_or(0);
        let value = if adv.cost_to_raise == 0 {
            statmgmt::label(ui, statmgmt::string::INFINITY)
        } else {
            statmgmt::num(adv.cost_to_raise)
        };
        let entry = view.skills().iter().find(|s| s.id == self.selected_skill);
        let (level, effective, vitae) =
            entry.map_or((0, 0, 0), |s| (s.level, s.effective, s.vitae));
        let base = format!("{}: {effective}", self.selected_name());
        // `" (%d)"` in colour 3, only under a vitae penalty.
        let vitae_segment = if vitae < 0 {
            format!(" ({vitae})")
        } else {
            String::new()
        };
        let delta = effective.saturating_sub(vitae).saturating_sub(level);
        let font = value_font(effective, level, vitae);
        // `" (%s%d)"`: the `+` is a separate prefix on the buff arm; the debuff arm
        // has no prefix because `%d` prints the sign.
        let suffix = if delta == 0 {
            String::new()
        } else if delta > 0 {
            format!(" (+{delta})")
        } else {
            format!(" ({delta})")
        };
        let c = FooterContent {
            title: format!("{base}{vitae_segment}{suffix}"),
            title_font: font,
            line_one_label: statmgmt::label(ui, statmgmt::string::XP_TO_RAISE_LABEL),
            line_one_value: value,
            line_two_label: statmgmt::label(ui, statmgmt::string::EXPERIENCE_LABEL),
            line_two_value: statmgmt::num(view.available_experience()),
            meter: Some(adv.meter_fill()),
            button: Footer::enable_for(u64::from(adv.cost_to_raise), xp),
            button_10: Footer::enable_for(u64::from(adv.cost_to_raise_10), xp),
            button_10_visible: true,
        };
        // One set-with-colour then up to two appends-with-colour, which is what lets each
        // segment carry its own colour; everything else is a plain set.
        footer.set_text_with_font(ui, child::TITLE, &base, 0, 0);
        if let Some(h) = footer.child(ui, child::TITLE) {
            if !vitae_segment.is_empty() {
                statmgmt::append_text_with_font(ui, h, &vitae_segment, 0, 3);
            }
            if !suffix.is_empty() {
                statmgmt::append_text_with_font(ui, h, &suffix, 0, font);
            }
        }
        write_footer_lines(ui, &footer, &c);
        c
    }

    /// The selected skill's name, off the row the list already built.
    #[must_use]
    fn selected_name(&self) -> String {
        self.rows
            .iter()
            .find(|r| r.skill == self.selected_skill)
            .map_or_else(String::new, |r| r.name.clone())
    }

    /// The skill panel's selection raise. With no selected skill it refuses. For a skill above
    /// untrained it sets the awaiting-raise latch, sends the train-skill request with the cost
    /// to raise, and disables the footer button (`0xD`); otherwise it goes to the train-skill
    /// confirmation.
    pub fn raise_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if self.selected_skill == 0 {
            return false;
        }
        let adv = view
            .skill_advancement(self.selected_skill)
            .unwrap_or_default();
        if adv.sac > sac::UNTRAINED {
            self.awaiting_raise = true;
            ui.requests.emit(UiRequest::TrainSkill {
                skill: self.selected_skill,
                xp: adv.cost_to_raise,
            });
            if let Some(f) = self.footer {
                f.set_button_state(ui, child::BUTTON, button_state::DISABLED);
            }
            self.footer_content.button = button_state::DISABLED;
        } else {
            self.train_skill(&mut ui.requests, adv);
        }
        true
    }

    /// The same message with the cost to raise ten levels, and **no** fall-through to the
    /// training dialog.
    pub fn raise_10_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if self.selected_skill == 0 {
            return false;
        }
        let adv = view
            .skill_advancement(self.selected_skill)
            .unwrap_or_default();
        if adv.sac <= sac::UNTRAINED {
            return true;
        }
        self.awaiting_raise = true;
        ui.requests.emit(UiRequest::TrainSkill {
            skill: self.selected_skill,
            xp: adv.cost_to_raise_10,
        });
        if let Some(f) = self.footer {
            f.set_button_state(ui, child::BUTTON_10, button_state::DISABLED);
        }
        self.footer_content.button_10 = button_state::DISABLED;
        true
    }

    /// The skill panel's train skill → the train skill dialog callback.
    ///
    /// The client puts up a confirmation dialog first and only its callback sends the
    /// train-skill-advancement-class request. **This build has no dialog
    /// system**, so the request is emitted straight away; the missing confirmation step is named
    /// here rather than faked, because inserting a dialog that does not exist would be
    /// a worse lie than sending without one.
    fn train_skill(&mut self, requests_out: &mut crate::requests::Outbox, adv: SkillAdvancement) {
        requests_out.emit(UiRequest::TrainSkillAdvancementClass {
            skill: self.selected_skill,
            credits: adv.cost_to_raise,
        });
        self.awaiting_raise = true;
    }

    /// The element-message handler's `0x10000004` arm — the notice that clears the
    /// awaiting-raise latch. The caller raises it when a quality update arrives.
    pub fn clear_awaiting_raise(&mut self) {
        self.awaiting_raise = false;
    }
}

/// The `p1` values the element-message handler accepts on `0x1C` — the four
/// input actions that count as a press on a list row.
pub const SELECT_ACTIONS: [u32; 4] = [7, 8, 10, 11];

/// Push one [`FooterContent`] into the eight footer elements.
fn write_footer(ui: &mut UiSystem, f: &Footer, c: &FooterContent) {
    // The title is set with a colour and everything else is a plain set, which is exactly what
    // the three footer functions do. The trained footer's title is three runs and is
    // written by its own function; it shares the other seven through [`write_footer_lines`].
    f.set_text_with_font(ui, child::TITLE, &c.title, 0, c.title_font);
    write_footer_lines(ui, f, c);
}

/// Everything in [`write_footer`] but the title.
fn write_footer_lines(ui: &mut UiSystem, f: &Footer, c: &FooterContent) {
    f.set_text(ui, child::LINE_ONE_LABEL, &c.line_one_label);
    f.set_text(ui, child::LINE_ONE_VALUE, &c.line_one_value);
    f.set_text(ui, child::LINE_TWO_LABEL, &c.line_two_label);
    f.set_text(ui, child::LINE_TWO_VALUE, &c.line_two_value);
    if let Some(m) = c.meter {
        f.set_meter(ui, m);
    }
    f.set_button_state(ui, child::BUTTON, c.button);
    f.set_button_visible(ui, child::BUTTON_10, c.button_10_visible);
    f.set_button_state(ui, child::BUTTON_10, c.button_10);
}

/// The skill info region's constructor + the skill info region's update, applied
/// to one freshly created row element.
///
/// The construction sets the icon (skipped when the `SkillBase` icon id is `INVALID_DID`) and
/// the label; the update sets the value.
fn write_row(ui: &mut UiSystem, h: ElemHandle, s: &SkillEntry) {
    set_row_icon(ui, h, s.icon);
    if let Some(t) = ui
        .get_child_recursive(h, ElementId(row::LABEL))
        .and_then(|c| ui.text_element_mut(c))
    {
        t.set_text(&s.name);
    }
    // The skill info region's update writes the value **with a colour index**, and that index
    // is what turns a buffed skill green and a debuffed one red.
    // `statmgmt::set_text_with_font` carries the number and its colour together; a plain
    // `TextElement` text write would drop the colour.
    if let Some(c) = ui.get_child_recursive(h, ElementId(row::VALUE)) {
        // **Font index 0**, colour index [`value_font`].
        //
        // The number is the **enchanted total**, formatted with `"%d"`.
        // Not `SkillEntry::level`, the raw level, which is the same number on an unenchanted
        // character and is the wrong one on any other.
        statmgmt::set_text_with_font(
            ui,
            c,
            &s.effective.to_string(),
            0,
            value_font(s.effective, s.level, s.vitae),
        );
    }
}

/// The client's icon arm, which both stat panels' rows use: unless the icon id is
/// `INVALID_DID`, find child `0x10000129` and set its image to the icon (draw mode 3).
///
/// `None` is `INVALID_DID` and writes nothing, which is what leaves an untrained skill with no
/// art rather than a blank rectangle. `AttributesPanel`'s nine rows use it too.
pub fn set_row_icon(ui: &mut UiSystem, h: ElemHandle, icon: Option<dereth_primitives::DataId>) {
    let Some(icon) = icon else { return };
    let Some(c) = ui.get_child_recursive(h, ElementId(row::ICON)) else {
        return;
    };
    if let Some(n) = ui.node_mut(c) {
        n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(icon, 0, 0));
    }
}

/// The skill info region's update's 0/1/2 — the **colour** index the value cell is written
/// with.
///
/// Retail reads the skill's base level (and never uses it), its raw level, its enchanted value
/// and the vitae modifier (`<= 0`). The colour is 1 (buffed) when `raw < eff - v`, 2 (debuffed)
/// when `eff - v < raw`, and 0 otherwise; the text is `"%d"` of the enchanted value.
///
/// The comparison is the enchanted value minus the vitae modifier against the raw value — so
/// **the base level is never read**, and raw-against-enchanted alone would drop the **vitae
/// term**. \[verified\]
///
/// # Why the vitae term is not cosmetic
///
/// The vitae modifier is `enchant(raw) - raw` with **only** the vitae enchantment applied, so it
/// is `0` with no vitae and **negative** with one; `eff - v` therefore *adds the penalty back*.
/// It has to, because vitae is itself an enchantment and the enchanted value already carries it
/// (the enchantment pass applies vitae before it culls either spell list). Without the term, a character with 5 % vitae and no spells at all
/// has `eff < raw` on every skill and **the whole list draws red**. With it, `eff - v == raw`
/// and every row draws plain — which is what a player sees in retail.
///
/// The three arms, with `m` the vitae multiplier and no spells:
///
/// | state | `raw` | `eff` | `v` | `eff - v` | colour |
/// |---|---:|---:|---:|---:|---|
/// | plain | 100 | 100 | 0 | 100 | 0 white |
/// | vitae 0.95 | 100 | 95 | −5 | 100 | **0 white** |
/// | vitae 0.95 + a `+3` spell | 100 | 98 | −5 | 103 | **1 green** |
/// | vitae 0.95 + a `−20` spell | 100 | 75 | −5 | 80 | 2 red |
///
/// The third row is the discriminating one: without the term it reads `98 < 100` and draws
/// **red** for a skill the player has just buffed.
///
/// The index itself is an index into attribute **`0x1B`**, the font-**colour** array — see
/// [`super::statmgmt::set_text_with_font`]. The shipped row template's value element
/// `0x1000012B` declares exactly three of them, white / green / red, and exactly one font.
#[must_use]
pub const fn value_font(effective: i32, raw: i32, vitae: i32) -> u32 {
    // `eff - v`, saturating rather than wrapping: the two skill values are 32-bit in the
    // client and the subtraction wraps there, so this is only reachable with a corrupt
    // modifier and there is no retail behaviour to be faithful to at the boundary.
    let adjusted = effective.saturating_sub(vitae);
    if raw < adjusted {
        1
    } else if adjusted < raw {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: u32, name: &str, sac: u32, min_level: u32, level: i32) -> SkillEntry {
        SkillEntry {
            id,
            name: name.to_owned(),
            icon: None,
            min_level,
            sac,
            level,
            effective: level,
            vitae: 0,
        }
    }

    /// Oracle: the client's switch on the advancement class, including the min-level > 1 arm.
    #[test]
    fn an_untrained_skill_above_min_level_one_falls_into_the_fourth_group() {
        assert_eq!(SkillGroup::of(sac::SPECIALIZED, 0), SkillGroup::Specialized);
        assert_eq!(SkillGroup::of(sac::TRAINED, 0), SkillGroup::Trained);
        assert_eq!(SkillGroup::of(sac::UNTRAINED, 0), SkillGroup::Untrained);
        assert_eq!(SkillGroup::of(sac::UNTRAINED, 1), SkillGroup::Untrained);
        // The split: min_level 2 puts an untrained skill with the undefined ones.
        assert_eq!(SkillGroup::of(sac::UNTRAINED, 2), SkillGroup::Unusable);
        assert_eq!(SkillGroup::of(sac::UNDEF, 0), SkillGroup::Unusable);
        // A specialised or trained skill is never split, whatever its min_level.
        assert_eq!(SkillGroup::of(sac::SPECIALIZED, 5), SkillGroup::Specialized);
        assert_eq!(SkillGroup::of(sac::TRAINED, 5), SkillGroup::Trained);
        // And the header order is the order the four header appends run in.
        assert_eq!(
            SkillGroup::ALL.map(SkillGroup::header_template),
            [1, 2, 3, 4],
        );
        assert_eq!(HEADER_TEMPLATES, [1, 2, 3, 4]);
        assert_ne!(
            ROW_TEMPLATE, HEADER_TEMPLATES[0],
            "the row template is 0, not a header"
        );
    }

    /// Oracle: the client's two comparisons. **Four stations, not
    /// one** — plain, buffed, debuffed and vitae-carrying — because the vitae term is invisible
    /// in the first three.
    #[test]
    fn the_value_font_compares_the_raw_level_against_the_enchanted_total_less_vitae() {
        // `value_font(effective, raw, vitae)`.
        assert_eq!(
            value_font(100, 100, 0),
            0,
            "plain: no enchantment, no vitae"
        );
        assert_eq!(
            value_font(120, 100, 0),
            1,
            "buffed — the enchanted value is the larger"
        );
        assert_eq!(value_font(80, 100, 0), 2, "debuffed");

        // **The vitae station.** The enchanted value already carries the penalty, so without the
        // term this reads `95 < 100` and draws every row red.
        assert_eq!(value_font(95, 100, -5), 0, "vitae alone is not a debuff");
        // And the case that separates the two readings in the *other* direction: a small buff
        // under vitae is still a buff, though the enchanted total is below the raw level.
        assert_eq!(
            value_font(98, 100, -5),
            1,
            "vitae 0.95 plus a +3 spell is green, not red"
        );
        // A real debuff under vitae stays red.
        assert_eq!(value_font(75, 100, -5), 2, "vitae 0.95 plus a -20 spell");

        // The term is signed, and a *positive* modifier (which vitae never produces, because
        // the vitae modifier is 0 unless the multiplier is below 1.0) would push the
        // comparison the other way. Pinned so the sign convention cannot silently flip.
        assert_eq!(value_font(105, 100, 5), 0, "eff - v = 100");
    }

    /// Oracle: retail's sorted insert — it scans from the group's header
    /// to the next header and stops at the first `wcscmp(existing, new) < 0`, i.e. the group is
    /// kept in ascending name order and a skill never crosses a header.
    ///
    /// This exercises the grouping and ordering without a layout: `rebuild` needs a live list box
    /// and there is none here, so the same two steps are applied directly.
    #[test]
    fn each_group_is_sorted_by_name_and_no_skill_crosses_a_header() {
        let skills = [
            e(1, "War Magic", sac::TRAINED, 0, 5),
            e(2, "Alchemy", sac::TRAINED, 0, 5),
            e(3, "Melee Defense", sac::SPECIALIZED, 0, 10),
            e(4, "Void Magic", sac::UNTRAINED, 2, 0),
            e(5, "Arcane Lore", sac::UNTRAINED, 0, 0),
            e(6, "Salvaging", sac::UNDEF, 0, 0),
        ];
        let mut out: Vec<(SkillGroup, Vec<&str>)> = Vec::new();
        for g in SkillGroup::ALL {
            let mut v: Vec<&str> = skills
                .iter()
                .filter(|s| SkillGroup::of(s.sac, s.min_level) == g)
                .map(|s| s.name.as_str())
                .collect();
            v.sort_unstable();
            out.push((g, v));
        }
        assert_eq!(
            out,
            vec![
                (SkillGroup::Specialized, vec!["Melee Defense"]),
                (SkillGroup::Trained, vec!["Alchemy", "War Magic"]),
                (SkillGroup::Untrained, vec!["Arcane Lore"]),
                // Void Magic is untrained but min level 2, so it joins the undefined one.
                (SkillGroup::Unusable, vec!["Salvaging", "Void Magic"]),
            ]
        );
    }

    /// Oracle: the live `classic_gameplay` tree — `0x1000023D` is a child of **both**
    /// `AttributesPanel` (`0x1000022B`) and `SkillsPanel` (`0x1000022C`), so the panel id this module
    /// starts its search from has to be the skill one. Cheap guard against a transposition.
    #[test]
    fn the_panel_id_is_the_skill_sub_panel_and_not_the_attribute_one() {
        assert_eq!(PANEL, ElementId(0x1000_022C));
        assert_eq!(
            crate::panels::catalogue::spec("SkillsPanel").map(|s| s.ty),
            Some(crate::element_types::ty::SKILL)
        );
        assert_eq!(crate::element_types::ty::SKILL.0, 0x1000_002B);
        assert_eq!(crate::element_types::ty::ATTRIBUTE.0, 0x1000_002A);
        // The list box is one of the shared stat-management bindings.
        let spec = crate::panels::catalogue::spec("SkillsPanel").unwrap();
        assert!(spec.children.iter().any(|c| c.id == LIST_BOX));
    }
}
