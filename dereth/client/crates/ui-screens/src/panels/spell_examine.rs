//! [`SpellExamineUi`] — the fourth pane of `<EXAM>`, and what a **right-click on a spell** fills.
//!
//! # The gesture, end to end
//!
//! Right-clicking a spell in the spellbook or on the cast bar examines it and lists its
//! components. The client's path has four links:
//!
//! | link | client |
//! |---|---|
//! | the press | a row sends message `0x1C` with `p1 == 8` (not a *select*) |
//! | the call | a row with an item id examines the object; otherwise a row with a spell id examines the spell |
//! | the notice | spell examination is routed from the central examine handler through the examination panel to this pane |
//! | the pane | this module, behind `ExaminationPanel`'s `spell_base` and `ExamineSubUi::Spell` |
//!
//! **It is a purely local lookup.** Starting a spell examination cancels any
//! appraisal in flight (the appraise request `0x00C8` with a zero id) and then
//! fills the pane from `client_portal.dat`'s `SpellTable` (`0x0E00000E`) and `SpellComponentTable`
//! (`0x0E00000F`). Nothing is requested and nothing is awaited, which is why the panel is shown
//! immediately rather than when a reply lands.
//!
//! # The bindings, and that they are in the shipped layout
//!
//! The pane looks up `0x10000153` off the window and then seven children off it. All seven are in
//! `classic_gameplay`, read off the live tree rather than assumed. The focused probe dumps the
//! subtree and asserts each one, and it also
//! asserts that the formula list carries the row template `(0x2100001C, 0x1000032E)` that
//! the pane instantiates by id. \[measured\]
//!
//! # The text
//!
//! The client writes six things, in this order:
//!
//! ```text
//!   spell name                  -> the window's displayed-name text   (no stack suffix)
//!   school name                 -> school text   = "School: " + name
//!   base mana, mana mod         -> mana text     = "Mana: " + n   [+ " + %d per target"]
//!   duration                    -> duration text, only when != -1 and > 0
//!                                  "Duration: %u sec."  (< 60)
//!                                  "Duration: %u min."  (>= 60, times 1/60)
//!   spell range                 -> range text, cleared when 0
//!                                  "Range: %.1f yds."   (divided by 0.9144)
//!   description                 -> display text
//!   "\nCOMPONENTS:"             -> display text, only when the spell has components
//!   "     " + component name    -> display text, one line per surviving component
//! ```
//!
//! The duration conversion multiplies by the stored `1/60` constant; range divides by
//! `0.9144` (metres per yard) and clamps to `75.0`.
//!
//! **The two numeric literals matter.** The number is `"0"` when the mana is `< 1` and `"???"`
//! when conversion fails; the per-target mana mod appends `" + %d per target"`, and only when it
//! is `> 0`.
//!
//! # Two components can be skipped, and the skip takes the text line with it
//!
//! The per-component loop is not "one row per formula slot". The client skips the slot when
//! the component table has no entry for it, and skips it when the entry's icon id is
//! `INVALID_DID` — and **both skip the text append too**, so a component with no
//! icon contributes neither a picture nor a line. That is why
//! [`crate::view::SpellExamineView::components`] carries one `Option` per slot rather than a
//! pre-filtered list: the guard belongs where a test can drive it.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{GameView, SpellExamineComponent, SpellExamineView};

use super::listbox::ListBoxWidget;

/// The spell pane's base field — recursive child `0x10000153` of the window.
/// The same literal [`super::examination::SPELL_BASE`] already carried.
pub const BASE: ElementId = ElementId(0x1000_0153);
/// The magic school text.
pub const MAGIC_SCHOOL_TEXT: ElementId = ElementId(0x1000_015E);
/// A plain region (element type 3 in the shipped layout), which is
/// why it is not cast to a text element.
pub const SPELL_ICON: ElementId = ElementId(0x1000_015F);
/// The spell mana text.
pub const MANA_TEXT: ElementId = ElementId(0x1000_0160);
/// The spell duration text.
pub const DURATION_TEXT: ElementId = ElementId(0x1000_0161);
/// The spell range text.
pub const RANGE_TEXT: ElementId = ElementId(0x1000_0162);
/// The display text the description block appends to.
pub const DISPLAY_TEXT: ElementId = ElementId(0x1000_0163);
/// The same literal [`super::examination::SPELL_COMPONENT_LIST`]
/// carries, bound here because this is the constructor that binds it.
pub const FORMULA_LIST: ElementId = ElementId(0x1000_032D);
/// The row template the pane instantiates in the formula list. In `classic_gameplay` the list's one template is `(0x2100001C, 0x1000032E)`.
pub const FORMULA_ROW_TEMPLATE: ElementId = ElementId(0x1000_032E);
/// Property `0x10000010` — the property each row carries its component **SCID** under, and what
/// the click arm reads back.
pub const ROW_COMPONENT_PROPERTY: u32 = 0x1000_0010;
/// Recursive child `0x10000330` of a row — the **"you do not have this one"** mark,
/// the one child of the formula row template.
///
/// In the shipped `0x2100001C` it is a type-3 region filling the whole 32×32 row
/// (`0,0..31,31`, the same box as the template) with media `Image { 0x06004CC2, draw_mode 3 }`
/// and attribute `0x3B HIDE = false` — so the layout authors it **visible**, drawn over the
/// component icon, and [`SpellExamineUi::update_components`] is what hides it again for every
/// component the player is actually carrying.
pub const COMPONENT_MISSING_MARK: ElementId = ElementId(0x1000_0330);

/// The component list's x and width defaults, used
/// when the layout has no `0x1000032D` to measure.
pub const DEFAULT_COMPONENT_CELL: i32 = 0x20;

/// Metres per yard — the divisor of the range line.
pub const METRES_PER_YARD: f64 = 0.9144;
/// The seconds/minutes fork.
pub const MINUTE_SECONDS: f64 = 60.0;
/// The multiplier used by the minutes-formatting arm, with the client's precision.
pub const RECIPROCAL_MINUTE: f64 = 0.016_666_666_666_666_666;

/// The school names, in school-number order.
///
/// The default arm is `"None"` for anything outside `1..=5`,
/// **not** an empty string.
#[must_use]
pub fn school_name(school: u32) -> &'static str {
    match school {
        1 => "War Magic",
        2 => "Life Magic",
        3 => "Item Enchantment",
        4 => "Creature Enchantment",
        5 => "Void Magic",
        _ => "None",
    }
}

/// The spell's skill lookup, the five fallback skills, and
/// the spell-range determination's arithmetic.
///
/// All three live in [`dereth_client_contract::panels::spell_examine`], because
/// `dereth_client::hud` evaluates the range against the player's own skills.
pub use dereth_client_contract::panels::spell_examine::{
    skill_for_spell, spell_range, MAGIC_SKILLS,
};

/// The mana text write, whole: `"Mana: "` + (`"0"` when the mana is below 1, else its digits,
/// or `"???"` if conversion fails), then `" + %d per target"` when the per-target mod is positive.
///
/// Conversion cannot fail for a value that is already an integer here, so the `"???"` arm is
/// unreachable from this build and is not written; the `"0"` arm is reachable and is.
#[must_use]
pub fn mana_text(base_mana: i32, mana_mod: i32) -> String {
    let n = if base_mana < 1 {
        "0".to_owned()
    } else {
        base_mana.to_string()
    };
    let mut s = format!("Mana: {n}");
    if mana_mod > 0 {
        s.push_str(&format!(" + {mana_mod} per target"));
    }
    s
}

/// The spell examination's duration arm.
///
/// `None` is the client leaving the duration text as the clear left it: a duration of exactly
/// `-1.0` (the duration read's no-meta-spell answer) or `<= 0`. The conversion truncates toward
/// zero, so `59.9 s` reads `59` and `119 s` reads `1 min.`.
#[must_use]
pub fn duration_text(duration: f64) -> Option<String> {
    if duration == -1.0 || duration <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    if duration >= MINUTE_SECONDS {
        Some(format!(
            "Duration: {} min.",
            (duration * RECIPROCAL_MINUTE) as u32
        ))
    } else {
        Some(format!("Duration: {} sec.", duration as u32))
    }
}

/// The spell examination's range arm. `None` is the exact-zero arm, on which the client clears
/// the text rather than writing `"Range: 0.0 yds."`.
#[must_use]
pub fn range_text(range: f32) -> Option<String> {
    if range == 0.0 {
        return None;
    }
    Some(format!(
        "Range: {:.1} yds.",
        f64::from(range) / METRES_PER_YARD
    ))
}

/// The spell pane's text append, which is **not**
/// [`super::examination::add_item_info`]: it appends a single `"\n"`, never `"\n\n"`, and it has no
/// font or colour argument.
///
/// The guard is that the text already holds more than one glyph, so a block appended after a **one-glyph** block
/// runs straight on. That is the client's own off-by-one and it is reproduced rather than tidied.
pub fn add_item_info(acc: &mut String, text: &str) {
    if acc.encode_utf16().count() > 1 {
        acc.push('\n');
    }
    acc.push_str(text);
}

/// The caption the client pushes, **including its leading newline**.
pub const COMPONENTS_CAPTION: &str = "\nCOMPONENTS:";
/// The indent the client seeds each component line with — five spaces.
pub const COMPONENT_INDENT: &str = "     ";

/// The whole of the display text for one spell: the description, then the caption when any
/// component survived, then one indented line per surviving component.
///
/// `names` is already the surviving list — see [`surviving_components`], which applies the two
/// skips the client applies.
#[must_use]
pub fn display_text(description: &str, names: &[String]) -> String {
    let mut out = String::new();
    add_item_info(&mut out, description);
    if !names.is_empty() {
        add_item_info(&mut out, COMPONENTS_CAPTION);
    }
    for n in names {
        add_item_info(&mut out, &format!("{COMPONENT_INDENT}{n}"));
    }
    out
}

/// The components the loop actually draws, in formula order, omit a slot the component table
/// has no entry for and one whose icon id is `INVALID_DID`.
/// **Both skips pass over the text append, so the text line goes too.**
#[must_use]
pub fn surviving_components(
    components: &[Option<SpellExamineComponent>],
) -> Vec<(SpellExamineComponent, DataId)> {
    components
        .iter()
        .filter_map(|c| c.as_ref())
        .filter_map(|c| c.icon.map(|i| (c.clone(), i)))
        .collect()
}

/// The examination window's spell subpanel.
#[derive(Debug, Default)]
pub struct SpellExamineUi {
    /// The base field.
    pub base: Option<ElemHandle>,
    school: Option<ElemHandle>,
    icon: Option<ElemHandle>,
    mana: Option<ElemHandle>,
    duration: Option<ElemHandle>,
    range: Option<ElemHandle>,
    display: Option<ElemHandle>,
    /// The formula list box.
    list: Option<ListBoxWidget>,
    /// The list box's design x, kept because the spell examination re-centres the list on it
    /// for every row count.
    pub component_list_x: i32,
    /// The list box's design width.
    pub component_list_w: i32,

    /// The spell currently shown.
    pub spell: u32,
    /// What went into the school text, so a test can read the pane back without the glyphs.
    pub school_text: Option<String>,
    /// The mana text's content.
    pub mana_text: Option<String>,
    /// The duration text's content; `None` is the arm that left it cleared.
    pub duration_text: Option<String>,
    /// The range text's content; `None` is the clearing arm.
    pub range_text: Option<String>,
    /// The display text's content — description, caption and component lines.
    pub display_text: String,
    /// The component names drawn, in drawn order.
    pub component_names: Vec<String>,
    /// The SCID each drawn row carries under [`ROW_COMPONENT_PROPERTY`], in the same order.
    pub component_scids: Vec<u32>,
    /// How many of those reached a row **element** of the formula list. A denominator: "the pane
    /// computed four components" and "four icons are on screen" are different facts, and a layout
    /// whose list box carries no row template would otherwise fail silently.
    pub rows_drawn: u32,
    /// How many spells this pane has been filled with — a denominator, so "the pane is empty" and
    /// "nothing ever asked it" are different answers.
    pub filled: u32,
    /// How many times [`Self::update_components`] has run — the component update's
    /// denominator, so "no row is marked" and "nothing re-marked them" are different
    /// answers.
    pub components_marked: u32,
    /// How many rows the last [`Self::update_components`] left marked as missing.
    pub rows_marked_missing: usize,
}

impl SpellExamineUi {
    /// The client's eight child lookups, off the `<EXAM>` window.
    ///
    /// The two cell measurements take the constructor's `0x20` defaults when the list box is
    /// absent, which is the client's own initialisation order (defaults before the lookup).
    pub fn post_init(&mut self, ui: &mut UiSystem, window: ElemHandle) {
        self.component_list_x = DEFAULT_COMPONENT_CELL;
        self.component_list_w = DEFAULT_COMPONENT_CELL;
        self.base = ui.get_child_recursive(window, BASE);
        let Some(b) = self.base else { return };
        self.school = ui.get_child_recursive(b, MAGIC_SCHOOL_TEXT);
        self.icon = ui.get_child_recursive(b, SPELL_ICON);
        self.mana = ui.get_child_recursive(b, MANA_TEXT);
        self.duration = ui.get_child_recursive(b, DURATION_TEXT);
        self.range = ui.get_child_recursive(b, RANGE_TEXT);
        self.display = ui.get_child_recursive(b, DISPLAY_TEXT);
        if let Some(h) = ui.get_child_recursive(b, FORMULA_LIST) {
            let measured = ui
                .node(h)
                .map_or((0, 0), |n| (n.region.box_.x0, n.region.box_.width()));
            self.component_list_x = measured.0;
            self.component_list_w = measured.1;
            self.list = Some(ListBoxWidget::bind(ui, h));
        }
    }

    /// Whether the pane found its base field at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.base.is_some()
    }

    /// Whether the pane found the formula list — a separate answer, because a pane that draws the
    /// text and no icons is a different defect from one that draws nothing.
    #[must_use]
    pub fn formula_list_bound(&self) -> bool {
        self.list.is_some()
    }

    /// How many row templates the bound list carries. The row instantiation needs the one
    /// whose element id is [`FORMULA_ROW_TEMPLATE`].
    #[must_use]
    pub fn templates(&self) -> usize {
        self.list.as_ref().map_or(0, |l| l.templates.len())
    }

    /// Empty the formula list.
    pub fn clear_icons(&mut self, ui: &mut UiSystem) {
        if let Some(l) = self.list.as_mut() {
            l.flush(ui);
        }
        self.component_names.clear();
        self.component_scids.clear();
        self.rows_drawn = 0;
        self.rows_marked_missing = 0;
    }

    /// The spell-examine pane's spell examination. Returns the view it drew, which is the client's
    /// `1`/`0` return — `None` for a zero id, no magic system, or a spell the table does not
    /// carry, and in each of those cases the pane is left exactly as it was.
    pub fn examine_spell(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        spell: u32,
    ) -> Option<SpellExamineView> {
        if spell == 0 {
            return None;
        }
        // No magic system answers 0; then the spell's table entry is read. A spell the table
        // does not know leaves the current spell and every element untouched,
        // because every write below is past this point.
        let v = view.spell_examine(spell)?;
        // Clear the icons, the display text and the duration text, in that order; the duration
        // is the only line cleared up front.
        self.clear_icons(ui);
        set_text(ui, self.display, "");
        set_text(ui, self.duration, "");
        self.spell = spell;
        self.filled += 1;

        let school = format!("School: {}", school_name(v.school));
        set_text(ui, self.school, &school);
        self.school_text = Some(school);

        let mana = mana_text(v.base_mana, v.mana_mod);
        set_text(ui, self.mana, &mana);
        self.mana_text = Some(mana);

        self.duration_text = duration_text(v.duration);
        if let Some(d) = self.duration_text.clone() {
            set_text(ui, self.duration, &d);
        }

        self.range_text = range_text(v.range);
        match self.range_text.clone() {
            Some(r) => set_text(ui, self.range, &r),
            // Clearing the range text is the *else* of the fork, and it runs for a spell
            // with no range even though nothing cleared it above.
            None => set_text(ui, self.range, ""),
        }

        // Clear the image, then set alpha blit mode 3 and the image — the clear is
        // unconditional and the set is guarded on the spell having an icon.
        set_icon(ui, self.icon, v.icon);

        let surviving = surviving_components(&v.components);
        let names: Vec<String> = surviving.iter().map(|(c, _)| c.name.clone()).collect();
        let text = display_text(&v.description, &names);
        set_text(ui, self.display, &text);
        self.display_text = text;
        self.component_names = names;
        self.component_scids = surviving.iter().map(|(c, _)| c.scid).collect();

        // The row loop. The client re-positions and re-sizes the list before adding the rows:
        // x becomes `list_x - (list_w / 2) * (n - 1)` and the width `list_w * n` — the strip is centred on its design
        // x for whatever number of icons it is about to hold.
        let n = i32::try_from(surviving.len()).unwrap_or(0);
        if n > 0 {
            if let Some(h) = self.list.as_ref().map(|l| l.handle) {
                if let Some(node) = ui.node(h) {
                    let (y0, height) = (node.region.box_.y0, node.region.box_.height());
                    let x0 = self.component_list_x - (self.component_list_w / 2) * (n - 1);
                    let w = self.component_list_w * n;
                    ui.move_to(h, x0, y0);
                    ui.resize_to(h, w, height);
                }
            }
        }
        for (c, icon) in &surviving {
            let Some(l) = self.list.as_mut() else { break };
            let Some(index) = l
                .templates
                .iter()
                .position(|(_, e)| *e == FORMULA_ROW_TEMPLATE)
            else {
                break;
            };
            let Some(row) = l.add_from_template(ui, index, None) else {
                continue;
            };
            self.rows_drawn += 1;
            if let Some(node) = ui.node_mut(row) {
                node.region.blit_mode = dereth_ui::BlitMode::Alpha3;
                node.region.image = Some(dereth_ui::GraphicRef::world_surface(*icon, 0, 0));
            }
            set_row_component(ui, row, c.scid);
        }
        if let Some(l) = self.list.as_mut() {
            l.update_layout(ui);
        }
        // The row loop ends by updating components with literal argument zero.
        // The spell examination resolves the missing-component marks itself, so a pane that
        // opens before the next tracker notice is already correct.
        self.update_components(ui, view);
        Some(v)
    }

    /// The client's first half — the SCID the row at this
    /// index carries. The object-id half is the component **tracker**'s and is the caller's.
    #[must_use]
    pub fn component_scid_at(&self, index: usize) -> Option<u32> {
        self.component_scids.get(index).copied()
    }

    /// The row **element** at this index on the formula list box. The icon the player clicks,
    /// and what the arm's `p2` is.
    #[must_use]
    pub fn component_row(&self, index: usize) -> Option<ElemHandle> {
        self.list.as_ref()?.items.get(index).copied()
    }

    /// The spell-examine pane's component update, whole — **the mark on the components you
    /// are not carrying.**
    ///
    /// The loop re-reads the formula list box on every pass. With no list or no rows it returns.
    /// For each row it finds the mark child `0x10000330` (a row without it is skipped), reads the
    /// row's SCID from property `0x10000010`, and asks the player's component tracker: with no
    /// tracker the mark is shown; otherwise the SCID is mapped to a WCID and the mark is hidden
    /// when that component is owned and shown when it is not.
    ///
    /// Three things worth saying out loud:
    ///
    /// * **The tracker update the notice carries is never read.** The handler takes one argument
    ///   and returns without touching it: every row is re-resolved on every
    ///   notice, whatever changed. The notice is an edge, not a payload.
    /// * **There is no visibility or active-pane guard.** The update-spell-components notice
    ///   checks only that the spell pane exists and goes straight here, so a hidden `<EXAM>`
    ///   and an `<EXAM>` showing the *item* pane both still re-mark the spell pane's rows.
    /// * **The spell examination calls it itself.** The row loop ends by calling the component
    ///   update with argument zero, so the marks are resolved as the pane fills
    ///   and not only when the tracker next moves. Those are its only two callers.
    ///
    /// Returns how many rows were marked as missing — a denominator, so "nothing is marked" and
    /// "nothing was looked at" are different answers.
    pub fn update_components(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let rows: Vec<ElemHandle> = match self.list.as_ref() {
            Some(l) => l.items.clone(),
            None => return 0,
        };
        let mut missing = 0usize;
        for row in rows {
            let Some(mark) = ui.get_child_recursive(row, COMPONENT_MISSING_MARK) else {
                continue;
            };
            // The client zeroes the SCID before the read, so a row that carries no
            // `0x10000010` asks about component **0** — which no tracker owns, and the mark shows.
            let scid = row_component_scid(ui, row).unwrap_or(0);
            let owned = view.component_is_owned(scid);
            ui.set_visible(mark, !owned);
            if !owned {
                missing += 1;
            }
        }
        self.components_marked += 1;
        self.rows_marked_missing = missing;
        missing
    }

    /// The mark element under the row at `index`, for a caller that wants to read its visibility.
    #[must_use]
    pub fn component_mark(&self, ui: &UiSystem, index: usize) -> Option<ElemHandle> {
        ui.get_child_recursive(self.component_row(index)?, COMPONENT_MISSING_MARK)
    }

    /// The bound formula list box itself, for a caller that needs its box.
    #[must_use]
    pub fn formula_list(&self) -> Option<ElemHandle> {
        self.list.as_ref().map(|l| l.handle)
    }
}

/// Set the text on an optional element; a layout without the element is the client's own null
/// check and writes nothing.
fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, text: &str) {
    if let Some(t) = h.and_then(|h| ui.text_element_mut(h)) {
        t.set_text(text);
    }
}

/// Clear the image, then, when there is one, set alpha blit mode 3 and the image.
fn set_icon(ui: &mut UiSystem, h: Option<ElemHandle>, icon: Option<DataId>) {
    let Some(node) = h.and_then(|h| ui.node_mut(h)) else {
        return;
    };
    node.region.image = None;
    if let Some(icon) = icon {
        node.region.blit_mode = dereth_ui::BlitMode::Alpha3;
        node.region.image = Some(dereth_ui::GraphicRef::world_surface(icon, 0, 0));
    }
}

/// Stamp the row's instance property `0x10000010` with the component number — what the click
/// arm reads back.
fn set_row_component(ui: &mut UiSystem, row: ElemHandle, scid: u32) {
    if let Some(node) = ui.node_mut(row) {
        node.instance_properties.set(
            ROW_COMPONENT_PROPERTY,
            dereth_assets::ui::PropertyValue::Integer(i32::try_from(scid).unwrap_or(0)),
        );
    }
}

/// The read-back of `set_row_component` — property `0x10000010` on the row the list box says
/// was selected, read as an unsigned value.
///
/// The client's third arm answers nothing when there is no row, when the row does not carry the
/// property, when the property has no value, and when the SCID is zero (a zero SCID is not a
/// component either).
///
/// Several separate ways to answer nothing, and all land on the same exit. They are one
/// `Option` here; the distinction is unobservable because the arm's only use of the value is the
/// lookup, which refuses `0` anyway.
#[must_use]
pub fn row_component_scid(ui: &UiSystem, row: ElemHandle) -> Option<u32> {
    let v = ui
        .node(row)?
        .instance_properties
        .get_int(ROW_COMPONENT_PROPERTY)?;
    u32::try_from(v).ok().filter(|s| *s != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the school-name table and the six literals it selects.
    #[test]
    fn the_five_schools_and_the_default_are_the_clients_six_literals() {
        assert_eq!(school_name(1), "War Magic");
        assert_eq!(school_name(2), "Life Magic");
        assert_eq!(school_name(3), "Item Enchantment");
        assert_eq!(school_name(4), "Creature Enchantment");
        assert_eq!(school_name(5), "Void Magic");
        assert_eq!(
            school_name(0),
            "None",
            "out-of-range school uses None, not an empty string"
        );
        assert_eq!(school_name(6), "None");
    }

    /// Oracle: the client's school-to-skill table, five entries.
    #[test]
    fn each_school_asks_for_its_own_magic_skill() {
        assert_eq!(skill_for_spell(1), 0x22);
        assert_eq!(skill_for_spell(2), 0x21);
        assert_eq!(skill_for_spell(3), 0x20);
        assert_eq!(skill_for_spell(4), 0x1F);
        assert_eq!(skill_for_spell(5), 0x2B);
        assert_eq!(
            skill_for_spell(7),
            0,
            "and the default falls back to the best magic skill"
        );
    }

    /// Oracle: the mana text write's two forks.
    #[test]
    fn the_mana_line_has_a_zero_arm_and_a_per_target_tail() {
        assert_eq!(mana_text(30, 0), "Mana: 30");
        assert_eq!(mana_text(0, 0), "Mana: 0");
        assert_eq!(mana_text(-5, 0), "Mana: 0", "everything below 1 reads 0");
        assert_eq!(mana_text(30, 7), "Mana: 30 + 7 per target");
        assert_eq!(
            mana_text(30, -1),
            "Mana: 30",
            "the tail is guarded on `> 0`"
        );
    }

    /// The client's duration arm, with its constants `-1.0` and `60.0`.
    #[test]
    fn the_duration_line_truncates_and_switches_at_sixty_seconds() {
        assert_eq!(duration_text(-1.0), None);
        assert_eq!(duration_text(0.0), None);
        assert_eq!(duration_text(-4.0), None);
        assert_eq!(duration_text(59.9).as_deref(), Some("Duration: 59 sec."));
        assert_eq!(duration_text(60.0).as_deref(), Some("Duration: 1 min."));
        assert_eq!(duration_text(119.0).as_deref(), Some("Duration: 1 min."));
        assert_eq!(duration_text(1800.0).as_deref(), Some("Duration: 30 min."));
    }

    /// Oracle: the client's exact-zero compare and its division by 0.9144.
    #[test]
    fn the_range_line_is_yards_and_zero_clears_it() {
        assert_eq!(range_text(0.0), None);
        assert_eq!(range_text(9.144).as_deref(), Some("Range: 10.0 yds."));
        assert_eq!(range_text(75.0).as_deref(), Some("Range: 82.0 yds."));
    }

    /// Oracle: the client's constant-plus-mod-times-skill formula and the `75.0` clamp.
    #[test]
    fn the_range_is_constant_plus_mod_times_skill_clamped_at_seventy_five() {
        assert!((spell_range(10.0, 0.1, 100) - 20.0).abs() < 1e-4);
        assert!((spell_range(10.0, 0.5, 400) - 75.0).abs() < 1e-4, "clamped");
        assert!(
            (spell_range(0.0, 0.0, 300) - 0.0).abs() < 1e-6,
            "and zero stays zero"
        );
    }

    /// Oracle: the client's more-than-one-glyph guard and the caption's own newline.
    #[test]
    fn the_display_block_is_description_then_caption_then_indented_components() {
        let names = vec!["Lead Scarab".to_owned(), "Prismatic Taper".to_owned()];
        assert_eq!(
            display_text("Hurls a bolt of flame.", &names),
            "Hurls a bolt of flame.\n\nCOMPONENTS:\n     Lead Scarab\n     Prismatic Taper"
        );
        assert_eq!(
            display_text("Hurls a bolt of flame.", &[]),
            "Hurls a bolt of flame.",
            "no components, no caption"
        );
        assert_eq!(display_text("", &[]), "");
    }

    /// Oracle: the client — the two skips, both of which pass over the text append.
    #[test]
    fn a_component_with_no_base_or_no_icon_contributes_neither_a_row_nor_a_line() {
        let comp = |scid: u32, name: &str, icon: Option<u32>| {
            Some(SpellExamineComponent {
                scid,
                name: name.to_owned(),
                icon: icon.map(DataId),
            })
        };
        let slots = vec![
            comp(1, "Lead Scarab", Some(0x0600_1234)),
            None,
            comp(3, "Missing Icon", None),
            comp(4, "Prismatic Taper", Some(0x0600_5678)),
        ];
        let kept = surviving_components(&slots);
        assert_eq!(
            kept.iter()
                .map(|(c, _)| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Lead Scarab", "Prismatic Taper"]
        );
        let names: Vec<String> = kept.iter().map(|(c, _)| c.name.clone()).collect();
        assert_eq!(
            display_text("D.", &names),
            "D.\n\nCOMPONENTS:\n     Lead Scarab\n     Prismatic Taper",
            "the skipped slots take their text lines with them"
        );
    }
}
