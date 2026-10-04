//! `AttributesPanel` — the character page's **default** tab: six attributes and three vitals.
//!
//! Sources: the attribute panel's post-init,
//! The attribute info region's constructor / the update,
//! The attribute2nd info region's constructor / the update,
//! The skill system's attribute name read and the attribute2nd name read.
//!
//! # Why this shares the skills page's machinery
//!
//! It is the **first thing a player sees** when they open the character panel — `AttributesPanel` is
//! the tab that comes up, not `SkillsPanel` — and it is nine rows of the same list box, the same row
//! template and the same `InfoRegion` children the skills page already uses. Everything it needs
//! was built for [`super::skills`]; only the nine rows and their two formatters are new.
//!
//! # Post-init builds the rows once, and that is the whole difference from the skills page
//!
//! The attribute panel's post-init binds the list box `0x1000023D` and, only when it has no rows
//! yet and the list box exists, adds nine rows in this order, each preceded by its own icon
//! lookup by enum:
//!
//! | row | stat | kind | icon enum (value, group) |
//! |---|---:|---|---|
//! | Strength | 1 | primary | 1, `0x10000002` |
//! | Endurance | 2 | primary | 2, `0x10000002` |
//! | Coordination | 4 | primary | 4, `0x10000002` |
//! | Quickness | 3 | primary | 3, `0x10000002` |
//! | Focus | 5 | primary | 5, `0x10000002` |
//! | Self | 6 | primary | 6, `0x10000002` |
//! | Health | 2 | secondary | 2, `0x10000003` |
//! | Stamina | 4 | secondary | 4, `0x10000003` |
//! | Mana | 6 | secondary | 6, `0x10000003` |
//!
//! **Nine icons, not one.** There are **nine**
//! lookups, one before each row, and the enum value is the **stat id**: the primaries take group
//! `0x10000002` and the secondaries group `0x10000003`. The lookup takes three arguments, and the
//! group is the easily-missed third. Verified for all nine rows.
//!
//! Every row's last constructor argument is **`true`**, not `false`. It is the row's
//! register-quality-handler-for-the-player flag, which this build drives from
//! `RemainingPanels::update` instead, so nothing here depends on it; it is recorded because a
//! wrong description is how wrong code gets written.
//!
//! Unlike the skill list's rebuild, nothing here is rebuilt when the description arrives —
//! the rows are structural and only their **values** are refreshed, by `Update`. So `post_init`
//! creates and labels, and [`AttributesPanel::update`] writes numbers into rows that already
//! exist.
//!
//! **The attribute order is 1, 2, 4, 3, 5, 6 and that is not a transcription slip.** The
//! `ATTRIBUTE` enum is 1 Strength, 2 Endurance, 3 Quickness, 4 Coordination, 5 Focus, 6 Self, and
//! the client lists **Coordination before Quickness**.
//!
//! # The names are string literals in the client, not a string table
//!
//! The attribute-name and second-attribute-name reads are both plain
//! `switch`es over `L"Strength"`, `L"Endurance"` … and `L"Health"`, `L"Stamina"`, `L"Mana"`. There
//! is no `StringInfo` and no table id to resolve, so the labels are transcribed here — which is
//! also why they are English-only in the client.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::listbox::ListBoxWidget;
use super::skills::row;
use super::statmgmt::{self, button_state, child, row_state, state, Footer, FooterContent};
use crate::view::{AttributeAdvancement, GameView, UiRequest, Vital};

/// `AttributesPanel` — the character page's first sub-panel, type `0x1000002A`.
///
/// The same trap [`super::skills::PANEL`] documents, from the other side: `SkillsPanel`
/// (`0x1000022C`) carries the same `0x1000023D`, so the search starts at the sub-panel.
pub const PANEL: ElementId = ElementId(0x1000_022B);

/// The list box, shared with the skills page's layout.
pub const LIST_BOX: ElementId = super::skills::LIST_BOX;

/// The icon lookup's **group** (`0x10000002`, enum value the attribute id) — the enum group each of
/// the six primary rows' icons is looked up in.
///
/// The enum *value* is the attribute id, so there are six distinct icons and not one. Never a
/// `DataID` at a call site: `crate::env::did_by_enum` resolves it through the
/// master mapper, so a DDD patch that moves the art still draws.
pub const ICON_GROUP: u32 = 0x1000_0002;

/// The same for the three secondary-attribute info-region rows — group `0x10000003`, enum value the
/// **even** (current) stat id `2`, `4`, `6`.
pub const ICON_GROUP_2ND: u32 = 0x1000_0003;

/// The six primary-attribute rows, in post-init's order — **1, 2, 4, 3, 5, 6**.
///
/// The skill system's attribute name read's literals.
pub const ATTRIBUTE_ROWS: [(u32, &str); 6] = [
    (1, "Strength"),
    (2, "Endurance"),
    (4, "Coordination"),
    (3, "Quickness"),
    (5, "Focus"),
    (6, "Self"),
];

/// The three secondary-attribute rows, in post-init's order — stats **2, 4, 6**, the
/// *current* halves of the three pairs.
///
/// The client's literals: the odd ids are "Maximum Health" and
/// friends and the even ones are the bare names, and post-init passes the even ones.
pub const SECONDARY_ROWS: [(u32, &str, Vital); 3] = [
    (2, "Health", Vital::Health),
    (4, "Stamina", Vital::Stamina),
    (6, "Mana", Vital::Mana),
];

/// One built row.
#[derive(Debug, Clone)]
pub struct AttributeRow {
    /// The `StatType` the row stands for -- the constructor argument, so `1, 2, 4, 3, 5, 6` for
    /// the primaries and `2, 4, 6` for the secondaries.
    pub stat: u32,
    /// True for the three secondary-attribute rows.
    pub secondary: bool,
    pub element: ElemHandle,
    /// What was last written into the row's value text.
    pub value: String,
    /// The row's label, kept so the footer's title does not have to look the name up again.
    pub name: String,
    /// The colour index the value was last written with: `0` plain, `1` buffed, `2` debuffed --
    /// The attribute info region's update's / the attribute2nd info region's update's
    /// fourth `set_text_with_font` argument.
    pub font: u32,
}

impl AttributeRow {
    /// The row token's stat -- **the id that goes on the wire**, and it is not always
    /// [`Self::stat`].
    ///
    /// For a primary row the client returns the constructor's own id.
    /// For a secondary row the getter returns **the maximum id**, which the constructor sets to
    /// the passed id minus one, while the current id is the passed id. Post-init passes the
    /// *even*, current ids `2, 4, 6`, so the three vital rows
    /// report `1, 3, 5` -- `MaxHealth`, `MaxStamina`, `MaxMana`.
    ///
    /// That is what the cost-to-raise and cost-to-raise-10 reads and
    /// [`UiRequest::TrainAttribute2nd`] are all handed, so a vital raise names the **odd** id
    /// on the wire.
    #[must_use]
    pub const fn wire_stat(&self) -> u32 {
        if self.secondary {
            self.stat - 1
        } else {
            self.stat
        }
    }
}

/// `AttributesPanel` — the live attributes list.
#[derive(Debug)]
pub struct AttributesPanel {
    pub list: Option<ListBoxWidget>,
    /// The row tokens, in post-init order.
    pub rows: Vec<AttributeRow>,
    /// The shared stat-management footer and header, bound off this sub-panel. `AttributesPanel`
    /// carries its **own** copy of all eight header elements and
    /// all three footer containers — they are siblings of this panel's `0x1000023D`, not shared
    /// with `SkillsPanel`'s.
    pub footer: Option<super::statmgmt::Footer>,
    /// What the last header render put on screen.
    pub header_content: super::statmgmt::HeaderContent,
    /// The selected index -- an index into [`Self::rows`], `-1` for no selection.
    ///
    /// **The attribute page keys its selection on the index and the skills page on the skill
    /// id**, and that is the client's own difference, not a simplification here:
    /// the skill panel's selection change toggles the selected skill while this panel's
    /// toggles the selected index against `-1`.
    pub selected_index: i32,
    /// What the last footer render put on screen.
    pub footer_content: FooterContent,
    /// The awaiting-raise flag -- the double-spend latch, shared with the skills
    /// page's copy in shape but not in storage: each subclass has its own.
    pub awaiting_raise: bool,
    /// The last set of `(value, colour index)` pairs written, so an unchanged frame is a no-op
    /// The colour is part of the key: a buff that lands on a value the vitae
    /// penalty had already moved changes the colour without changing the number.
    last: Option<Vec<(String, u32)>>,
}

impl Default for AttributesPanel {
    /// AttributesPanel — everything zero **except** the selected index, which the
    /// constructor sets to `-1`. A derived `Default` would make it 0, i.e. "row zero is
    /// selected", which is the one value in this struct that is not the zero one.
    fn default() -> Self {
        Self {
            list: None,
            rows: Vec::new(),
            footer: None,
            header_content: super::statmgmt::HeaderContent::default(),
            selected_index: -1,
            footer_content: FooterContent::default(),
            awaiting_raise: false,
            last: None,
        }
    }
}

impl AttributesPanel {
    /// Bind the list box and create the nine rows, each
    /// with its label already written. `page` is the character page `0x1000018E`.
    ///
    /// The "no rows yet" guard is the client's own "only once"; here re-running
    /// `post_init` is what a screen rebuild means, so the list is flushed first.
    pub fn post_init(&mut self, ui: &mut UiSystem, page: ElemHandle) {
        let panel = ui.get_child_recursive(page, PANEL).unwrap_or(page);
        self.list = ui
            .get_child_recursive(panel, LIST_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.footer = Some(super::statmgmt::Footer::new(panel));
        self.header_content = super::statmgmt::HeaderContent::default();
        self.footer_content = FooterContent::default();
        self.selected_index = -1;
        self.awaiting_raise = false;
        self.rows.clear();
        self.last = None;
        let Some(list) = self.list.as_mut() else {
            return;
        };
        list.flush(ui);
        for (stat, name) in ATTRIBUTE_ROWS {
            if let Some(h) = list.add_from_template(ui, super::skills::ROW_TEMPLATE, None) {
                super::skills::set_row_icon(
                    ui,
                    h,
                    ui.env()
                        .cloned()
                        .and_then(|e| e.did_by_enum(ICON_GROUP, stat)),
                    dereth_ui::ImageSource::Interface,
                );
                set_label(ui, h, name);
                self.rows.push(AttributeRow {
                    stat,
                    secondary: false,
                    element: h,
                    value: String::new(),
                    name: name.to_owned(),
                    font: 0,
                });
            }
        }
        for (stat, name, _) in SECONDARY_ROWS {
            if let Some(h) = list.add_from_template(ui, super::skills::ROW_TEMPLATE, None) {
                super::skills::set_row_icon(
                    ui,
                    h,
                    ui.env()
                        .cloned()
                        .and_then(|e| e.did_by_enum(ICON_GROUP_2ND, stat)),
                    dereth_ui::ImageSource::Interface,
                );
                set_label(ui, h, name);
                self.rows.push(AttributeRow {
                    stat,
                    secondary: true,
                    element: h,
                    value: String::new(),
                    name: name.to_owned(),
                    font: 0,
                });
            }
        }
        list.update_layout(ui);
    }

    /// The attribute info region's update and the attribute2nd info region's update,
    /// run against the current world.
    ///
    /// The primaries render `"%d"` or `"???"`; the secondaries render `"%d/%d"` — the third
    /// secondary-attribute info-region constructor argument is `false` for all three of this panel's
    /// rows, which is the current-and-maximum shape and not the percentage one. Both formats are
    /// [`dereth_client_contract::panels::inforegion`]'s, already recovered and tested there.
    ///
    /// # The value is one `set_text_with_font`, and its colour is the buff/debuff ladder
    ///
    /// Not `TextElement::set_text`, which is colour 0 whatever the enchantments say. Retail
    /// writes it once, with the colour
    /// index chosen as follows:
    ///
    /// * **Primary row.** It reads the attribute's raw and enchanted values, formats the
    ///   **enchanted** number as `"%d"`, and picks colour 1 when raw < enchanted, 2 when
    ///   enchanted < raw, else 0; one run in font 0.
    /// * **Secondary row.** It reads the maximum's raw and enchanted values and the current
    ///   value, formats `"%d/%d"` as (current, enchanted maximum), subtracts the vitae modifier
    ///   (zero or negative) from the enchanted maximum, and applies the same 1 / 0 / 2 ladder to
    ///   the raw maximum against that; one run in font 0.
    ///
    /// So a primary compares `raw` against `eff` with **no** vitae term (the base
    /// vitae modifier is 0 and the client never asks for it), and a vital
    /// compares the **maximum** half's raw against its enchanted value less the vitae modifier —
    /// the current half is displayed and never compared. Both are [`super::skills::value_font`]'s
    /// `raw` against `enchanted - vitae`, and the operands are exactly the ones
    /// [`AttributeAdvancement`] already carries for the selection footer, which is why the two
    /// agree by construction.
    ///
    /// Returns true on a frame that actually rewrote a value or a colour.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        use dereth_client_contract::panels::inforegion::{format_attribute, format_attribute_2nd};
        if self.rows.is_empty() {
            return false;
        }
        let player = view.player();
        let mut values: Vec<(String, u32)> = Vec::with_capacity(self.rows.len());
        for r in &self.rows {
            let text = if r.secondary {
                let which = SECONDARY_ROWS
                    .iter()
                    .find(|(s, _, _)| *s == r.stat)
                    .map(|(_, _, v)| *v);
                let pair = match (player, which) {
                    (Some(p), Some(v)) => view.vital(p, v),
                    _ => None,
                };
                match pair {
                    Some((cur, max)) => {
                        format_attribute_2nd(i32::try_from(cur).ok(), i32::try_from(max).ok(), None)
                    }
                    None => format_attribute(None),
                }
            } else {
                format_attribute(view.attribute(r.stat))
            };
            // `raw` against `enchanted - vitae` on the row's own `wire_stat()` — the maximum id
            // for a vital, the attribute id for a primary (where `vitae` is 0).
            let font = view
                .attribute_advancement(r.wire_stat(), r.secondary)
                .map_or(0, |a| {
                    super::skills::value_font(a.effective, a.value, a.vitae)
                });
            values.push((text, font));
        }
        if self.last.as_ref() == Some(&values) {
            return false;
        }
        for (r, (v, font)) in self.rows.iter_mut().zip(values.iter()) {
            if let Some(c) = ui.get_child_recursive(r.element, ElementId(row::VALUE)) {
                // Set the value text with font index 0 and colour index
                // `0x1B[colour]` — white / green / red on the shipped row template.
                statmgmt::set_text_with_font(ui, c, v, 0, *font);
            }
            r.value.clone_from(v);
            r.font = *font;
        }
        self.last = Some(values);
        true
    }

    /// The stat management panel's character info update + the experience update on
    /// **this** sub-panel's copy of the header. See
    /// [`super::statmgmt::Footer::update_header`] for the three fields it does not write.
    pub fn update_header(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(footer) = self.footer else {
            return false;
        };
        let c = footer.update_header(ui, &super::statmgmt::HeaderInputs::gather(view));
        if self.header_content == c {
            return false;
        }
        self.header_content = c;
        true
    }

    /// The `(stat, value)` pairs on screen, in list order — what a test reads back.
    #[must_use]
    pub fn shown(&self) -> Vec<(u32, String)> {
        self.rows
            .iter()
            .map(|r| (r.stat, r.value.clone()))
            .collect()
    }

    // ---- selection, the two footers, and the two raise buttons ------------------------------

    /// The stat-management panel's element-message handler, the two arms that matter.
    ///
    /// Exactly the same handler [`super::skills::SkillsPanel::on_element_message`] answers,
    /// because it **is** the same behaviour: `AttributesPanel` shares it and only the three
    /// operations it dispatches to (set selection, raise selection, raise-by-ten selection) are
    /// its own.
    ///
    /// The selection arm is `0x1C MOUSE_PRESS` with the action id in the first parameter, not `0x19
    /// MOUSE_CLICK`; see [`super::skills::SELECT_ACTIONS`].
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::MOUSE_PRESS {
            if !super::skills::SELECT_ACTIONS.contains(&m.p1) {
                return false;
            }
            let Some(item) = self.item_under(ui, m) else {
                return false;
            };
            self.set_selection(ui, Some(item), view);
            return true;
        }
        if m.id == msg::BUTTON_CLICKED {
            // Both footer button ids exist twice in the live tree; see
            // [`super::statmgmt::Footer::owns`].
            if !self.footer.is_some_and(|f| f.owns(ui, m.source)) {
                return false;
            }
            if self.awaiting_raise {
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

    /// The list box element's item under mouse read.
    ///
    /// Both stat panels call the one transcription in [`ListBoxWidget::item_under_mouse`] (see
    /// [`super::skills::SkillsPanel::item_under`]), so they cannot drift apart.
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

    /// The attribute panel's selection change.
    ///
    /// Two things here are not guessable from `SkillsPanel`. The index comes from the **row's
    /// info region**, not from the list box's own index lookup — the row element is asked for the
    /// `InfoRegion` interface `0x1000003A` and that object reports its own index. And the toggle
    /// is on the *index* against `-1`, so pressing the selected row a second time clears the
    /// selection and puts the default footer back, exactly as on the skills page.
    ///
    pub fn set_selection(
        &mut self,
        ui: &mut UiSystem,
        item: Option<ElemHandle>,
        view: &dyn GameView,
    ) {
        let mut index = -1i32;
        if let Some(h) = item {
            if let Some(i) = self.rows.iter().position(|r| r.element == h) {
                index = i32::try_from(i).unwrap_or(-1);
            }
        }
        if self.selected_index == index {
            index = -1;
        }
        self.selected_index = index;
        self.update_selection(ui, view);
    }

    /// Repaint every row's state, then the footer.
    ///
    /// Each row gets state 6 when it is the selected index and 1 otherwise. With no selection the
    /// panel takes state `0x10000011` and shows the default footer; otherwise it takes state
    /// `0x10000012` and shows the attribute selection footer for a stat of type 8 and the vital
    /// one for anything else.
    ///
    /// **The attribute page never reaches state `0x10000013`.** State `0x10000012` is written
    /// unconditionally before the branch and neither footer touches the footer meter, so the
    /// meter container is dead on this sub-panel — which is the visible difference from
    /// `SkillsPanel`, whose trained footer is the one function in the pair that writes a meter.
    pub fn update_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        for (i, r) in self.rows.iter().enumerate() {
            let selected = i32::try_from(i).unwrap_or(-1) == self.selected_index;
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
        let Some(row) = self.selected().cloned() else {
            footer.set_state(ui, state::DEFAULT);
            self.footer = Some(footer);
            self.footer_content = self.display_default_footer(ui, view);
            return;
        };
        footer.set_state(ui, state::SELECTION);
        self.footer = Some(footer);
        self.footer_content = if row.secondary {
            self.display_selection_footer_vital(ui, view, &row)
        } else {
            self.display_selection_footer_attribute(ui, view, &row)
        };
    }

    /// The selected row, or `None`. The row at the selected index, with the `-1` guard.
    #[must_use]
    pub fn selected(&self) -> Option<&AttributeRow> {
        usize::try_from(self.selected_index)
            .ok()
            .and_then(|i| self.rows.get(i))
    }

    /// The attribute panel's display default footer.
    ///
    /// | slot | `StringInfo` | value |
    /// |---|---|---|
    /// | title | `..._DefaultAttributeTitle` | — |
    /// | line one | `..._SkillCreditsLabel` / `..._SkillCreditsValue` | int quality `0x18` |
    /// | line two | `..._ExperienceLabel` / `..._ExperienceValue` | int64 quality `2` |
    ///
    /// **The default footer of the *attributes* page really does show skill credits.** Only the
    /// title id differs; the two value lines
    /// are the same two quality reads in the same order.
    fn display_default_footer(&self, ui: &mut UiSystem, view: &dyn GameView) -> FooterContent {
        let Some(footer) = self.footer else {
            return FooterContent::default();
        };
        let c = FooterContent {
            title: statmgmt::label(ui, statmgmt::string::DEFAULT_ATTRIBUTE_TITLE),
            title_font: 0,
            line_one_label: statmgmt::label(ui, statmgmt::string::SKILL_CREDITS_LABEL),
            line_one_value: statmgmt::num(view.skill_credits()),
            line_two_label: statmgmt::label(ui, statmgmt::string::EXPERIENCE_LABEL),
            line_two_value: statmgmt::num(view.available_experience()),
            meter: None,
            // Container `0x10000240` carries no buttons at all, so neither is touched.
            button: button_state::DISABLED,
            button_10: button_state::DISABLED,
            button_10_visible: false,
        };
        write_footer(ui, &footer, &c);
        c
    }

    /// The attribute panel's display selection footer attribute.
    ///
    /// The title is two writes, and the *second* one carries the colour. First
    /// `"%s: %d"` (name, enchanted value) in font 0, colour 0. Then, only when enchanted and raw
    /// differ, `" (%s%d)"` (prefix, enchanted − raw) is appended in font 0 with colour 1 and prefix
    /// `"+"` when raw < enchanted, or colour 2 and an empty prefix when enchanted < raw.
    ///
    ///
    /// Line one is the experience to raise one level, or `ID_StatManagement_Header_
    /// XPToLevelMeterInfinity` when that is 0; line two is unassigned experience; and **both**
    /// buttons are enabled against unassigned experience. There is no meter.
    fn display_selection_footer_attribute(
        &self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        row: &AttributeRow,
    ) -> FooterContent {
        let adv = view
            .attribute_advancement(row.wire_stat(), row.secondary)
            .unwrap_or_default();
        self.write_selection_footer(
            ui,
            view,
            &adv,
            dereth_presentation::stats::stat_title(
                &row.name,
                adv.effective,
                false,
                dereth_presentation::DisplayVariant::Modern,
            ),
        )
    }

    /// The same footer with a
    /// three-part title.
    ///
    /// It reads the maximum's raw and enchanted values (stat `wire_stat()`) and the current value
    /// (stat `wire_stat() + 1`), writes `"%s: %d/%d"` (name, current, enchanted maximum) in font 0,
    /// colour 0, then, when the vitae modifier `v` is negative, appends `" (%d)"` of `v` in
    /// colour 3, then the same buff/debuff append as the attribute footer with `raw - v` as its
    /// raw side.
    ///
    fn display_selection_footer_vital(
        &self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        row: &AttributeRow,
    ) -> FooterContent {
        let adv = view
            .attribute_advancement(row.wire_stat(), row.secondary)
            .unwrap_or_default();
        let mut title = format!("{}: {}/{}", row.name, adv.current, adv.effective);
        if adv.vitae < 0 {
            // `append_text_with_font` with font 0, colour 3. `FooterContent` carries one font index, so what is
            // recorded is the *last* append's -- the buff/debuff one, which is the colour a test
            // can distinguish. The vitae segment's own font 3 is written to screen by
            // `write_selection_footer` and is not separately readable back, the same compromise
            // `SkillsPanel`'s title makes.
            title.push_str(&format!(" ({})", adv.vitae));
        }
        self.write_selection_footer(ui, view, &adv, title)
    }

    /// The half both selection footers share: the two lines, the two buttons and the title's
    /// buff/debuff suffix.
    fn write_selection_footer(
        &self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        adv: &AttributeAdvancement,
        base_title: String,
    ) -> FooterContent {
        let Some(footer) = self.footer else {
            return FooterContent::default();
        };
        let xp = u64::try_from(view.available_experience()).unwrap_or(0);
        let value = if adv.cost_to_raise == 0 {
            statmgmt::label(ui, statmgmt::string::INFINITY)
        } else {
            statmgmt::num(adv.cost_to_raise)
        };
        let delta = adv.title_delta();
        let font = adv.title_font();
        let suffix = dereth_presentation::stats::signed_suffix(delta);
        let c = FooterContent {
            title: format!("{base_title}{suffix}"),
            title_font: font,
            line_one_label: statmgmt::label(ui, statmgmt::string::XP_TO_RAISE_LABEL),
            line_one_value: value,
            line_two_label: statmgmt::label(ui, statmgmt::string::EXPERIENCE_LABEL),
            line_two_value: statmgmt::num(view.available_experience()),
            // Neither footer writes the footer meter, and container `0x10000241` has none.
            meter: None,
            button: Footer::enable_for(u64::from(adv.cost_to_raise), xp),
            button_10: Footer::enable_for(u64::from(adv.cost_to_raise_10), xp),
            button_10_visible: true,
        };
        // The title is `set_text_with_font` then `append_text_with_font`, which is what lets the
        // suffix carry its own colour; everything else is a plain `SetText`.
        footer.set_text_with_font(ui, child::TITLE, &base_title, 0, 0);
        if !suffix.is_empty() {
            if let Some(h) = footer.child(ui, child::TITLE) {
                statmgmt::append_text_with_font(ui, h, &suffix, 0, font);
            }
        }
        footer.set_text(ui, child::LINE_ONE_LABEL, &c.line_one_label);
        footer.set_text(ui, child::LINE_ONE_VALUE, &c.line_one_value);
        footer.set_text(ui, child::LINE_TWO_LABEL, &c.line_two_label);
        footer.set_text(ui, child::LINE_TWO_VALUE, &c.line_two_value);
        footer.set_button_state(ui, child::BUTTON, c.button);
        footer.set_button_visible(ui, child::BUTTON_10, c.button_10_visible);
        footer.set_button_state(ui, child::BUTTON_10, c.button_10);
        c
    }

    /// The attribute panel's selection raise.
    ///
    /// With no selection, or no player description, it returns false. Otherwise it sets the
    /// awaiting-raise latch, sends [`UiRequest::TrainAttribute`] for a stat of type 8 or
    /// [`UiRequest::TrainAttribute2nd`] otherwise (the row's stat and its cost to raise), and
    /// puts the footer button in state `0x0D`.
    ///
    /// **There is no gate.** The skills page re-reads the skill's advancement class and diverts a
    /// below-trained skill into the training dialog; this one sets the latch before it has even
    /// looked at the stat type and sends unconditionally, because an attribute cannot be
    /// untrained. The only thing that stops a click is the awaiting-raise latch, which
    /// the element-message handler tests before it dispatches.
    ///
    /// So **the training-confirmation dialog does not apply here**: `AttributesPanel` has no
    /// `TrainSkill` fall-through and raises no dialog. That difference is `SkillsPanel`'s alone.
    pub fn raise_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(row) = self.selected().cloned() else {
            return false;
        };
        let adv = view
            .attribute_advancement(row.wire_stat(), row.secondary)
            .unwrap_or_default();
        self.awaiting_raise = true;
        ui.requests.emit(request_for(&row, adv.cost_to_raise));
        if let Some(f) = self.footer {
            f.set_button_state(ui, child::BUTTON, button_state::DISABLED);
        }
        self.footer_content.button = button_state::DISABLED;
        true
    }

    /// The same two messages with
    /// The client's amount, and the footer's raise-by-10 button is the one disabled.
    pub fn raise_10_selection(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(row) = self.selected().cloned() else {
            return false;
        };
        let adv = view
            .attribute_advancement(row.wire_stat(), row.secondary)
            .unwrap_or_default();
        self.awaiting_raise = true;
        ui.requests.emit(request_for(&row, adv.cost_to_raise_10));
        if let Some(f) = self.footer {
            f.set_button_state(ui, child::BUTTON_10, button_state::DISABLED);
        }
        self.footer_content.button_10 = button_state::DISABLED;
        true
    }

    /// The element-message handler's `0x10000004` arm — the notice that clears
    /// the awaiting-raise latch. The caller raises it when a quality update arrives.
    pub fn clear_awaiting_raise(&mut self) {
        self.awaiting_raise = false;
    }
}

/// The raise's stat-type-8 test — which of the two train messages a row
/// sends.
///
/// The info region's constructor is handed the stat type by its subclass constructor:
/// the attribute region passes the attribute stat type (8) and the secondary-attribute region
/// passes the secondary-attribute stat type (9), so the test is "is this one of the six primaries".
#[must_use]
pub fn request_for(row: &AttributeRow, xp: u32) -> UiRequest {
    if row.secondary {
        UiRequest::TrainAttribute2nd {
            vital: row.wire_stat(),
            xp,
        }
    } else {
        UiRequest::TrainAttribute {
            attribute: row.wire_stat(),
            xp,
        }
    }
}

/// Push one [`FooterContent`] into the footer elements, for the **default** footer.
///
/// The selection footers write their own title in two calls (see
/// [`AttributesPanel::write_selection_footer`]), so this is only the one-call form.
fn write_footer(ui: &mut UiSystem, f: &Footer, c: &FooterContent) {
    f.set_text_with_font(ui, child::TITLE, &c.title, 0, c.title_font);
    f.set_text(ui, child::LINE_ONE_LABEL, &c.line_one_label);
    f.set_text(ui, child::LINE_ONE_VALUE, &c.line_one_value);
    f.set_text(ui, child::LINE_TWO_LABEL, &c.line_two_label);
    f.set_text(ui, child::LINE_TWO_VALUE, &c.line_two_value);
    f.set_button_state(ui, child::BUTTON, c.button);
    f.set_button_visible(ui, child::BUTTON_10, c.button_10_visible);
    f.set_button_state(ui, child::BUTTON_10, c.button_10);
}

/// The info region's label text (`0x1000012A`).
fn set_label(ui: &mut UiSystem, h: ElemHandle, name: &str) {
    if let Some(t) = ui
        .get_child_recursive(h, ElementId(row::LABEL))
        .and_then(|c| ui.text_element_mut(c))
    {
        t.set_text(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's nine constructor calls, in order, and
    /// The skill system's attribute name read / the attribute2nd name read's literals.
    ///
    /// The order is the part worth a test: **Coordination (4) comes before Quickness (3)**, which
    /// reads like a transcription slip and is not one.
    #[test]
    fn the_nine_rows_are_the_documented_stats_in_post_inits_order() {
        assert_eq!(
            ATTRIBUTE_ROWS.map(|(s, _)| s),
            [1, 2, 4, 3, 5, 6],
            "Coordination (4) is listed before Quickness (3)"
        );
        assert_eq!(
            ATTRIBUTE_ROWS.map(|(_, n)| n),
            [
                "Strength",
                "Endurance",
                "Coordination",
                "Quickness",
                "Focus",
                "Self"
            ]
        );
        assert_eq!(
            SECONDARY_ROWS.map(|(s, _, _)| s),
            [2, 4, 6],
            "the even, current-value ids"
        );
        assert_eq!(
            SECONDARY_ROWS.map(|(_, n, _)| n),
            ["Health", "Stamina", "Mana"]
        );
        // The three secondaries name the same stat pairs the vitals bar reads, which is
        // what lets this panel reuse `GameView::vital` instead of a second accessor.
        for (stat, _, v) in SECONDARY_ROWS {
            assert_eq!(v.stats().0, stat, "{v:?}'s current stat id");
        }
        // All six primaries, exactly once each.
        let mut prim: Vec<u32> = ATTRIBUTE_ROWS.iter().map(|(s, _)| *s).collect();
        prim.sort_unstable();
        assert_eq!(prim, vec![1, 2, 3, 4, 5, 6]);
    }

    /// Oracle: the live `classic_gameplay` tree — `AttributesPanel` is `0x1000022B` and shares
    /// `0x1000023D` with `SkillsPanel`, which is why both panels resolve it from their own
    /// sub-panel and never from the page.
    #[test]
    fn the_panel_id_is_the_attribute_sub_panel_and_shares_the_skill_pages_list_box() {
        assert_eq!(PANEL, ElementId(0x1000_022B));
        assert_ne!(PANEL, super::super::skills::PANEL);
        assert_eq!(LIST_BOX, super::super::skills::LIST_BOX);
        assert_eq!(
            crate::panels::catalogue::spec("AttributesPanel").map(|s| s.ty),
            Some(crate::element_types::ty::ATTRIBUTE)
        );
        let spec = crate::panels::catalogue::spec("AttributesPanel").unwrap();
        assert!(spec.children.iter().any(|c| c.id == LIST_BOX));
    }
}
