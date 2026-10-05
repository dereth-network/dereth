//! The stat-management **footer** — the only place a player can spend experience.
//!
//! The footer handles skill and attribute selection, cost displays and experience spending.
//!
//! # The footer is three stacked containers, and the panel's own state picks one
//!
//! The eight getters are **one function eight times**, differing only in the child id they look
//! up. Each picks a container from the sub-panel's state — `0x10000012` → `0x10000241`,
//! `0x10000013` → `0x10000247`, anything else → `0x10000240` — and then looks up its own child
//! inside that container.
//!
//! So the *selection* is expressed as a **state on the sub-panel element**, and the three
//! containers all sit at the same rectangle `(0, 282, 299, 336)` on top of one another; the state
//! cascades to them and each one's own state description decides whether it draws. The elements `0x10000240`, `0x10000241` and
//! `0x10000247` are three siblings of `SkillsPanel` with identical boxes, and each declares exactly
//! the three states `0x10000011`, `0x10000012`, `0x10000013`.
//!
//! Trained skills select state `0x10000013`, whose container `0x10000247` has the progress
//! meter (attribute `0x69`). Untrained skills use the footer without a meter.
//!
//! # Two magic numbers that are *not* magic
//!
//! * **A row's selected state is `6` and its unselected state is `1`.** The selection update's loop
//!   sets state `6` for the selected row and state `1` for the rest. The live row template `0x10000248` declares exactly the two
//!   states `0x1` and `0x6` and nothing else, which is the cross-check.
//! * **A button's disabled state is `0x0D` and its enabled state is `1`.** Both footer buttons
//!   (`0x10000246`, `0x100005EB`) declare exactly `0x1` and `0xD`, and they ship **in `0xD`** —
//!   the raise buttons are disabled until something selects a skill.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

/// `StringInfo`'s table enum `0x10000001`, resolved.
///
/// Every footer label is a `StringInfo` in this table. The enum resolves to `0x23000001`.
pub const STRING_TABLE: DataId = DataId(0x2300_0001);

/// The state values the selection update writes onto the sub-panel.
pub mod state {
    /// No selection — the footer getters read container `0x10000240`.
    pub const DEFAULT: u32 = 0x1000_0011;
    /// A selection with **no** progress meter — container `0x10000241`.
    pub const SELECTION: u32 = 0x1000_0012;
    /// A selection **with** the progress meter — container `0x10000247`.
    pub const SELECTION_METER: u32 = 0x1000_0013;
}

/// The three footer containers, in the order the footer getters' switch names them.
pub mod container {
    pub const DEFAULT: u32 = 0x1000_0240;
    pub const SELECTION: u32 = 0x1000_0241;
    pub const SELECTION_METER: u32 = 0x1000_0247;
}

/// The eight children each getter resolves inside the chosen container.
pub mod child {
    /// The footer title label read.
    pub const TITLE: u32 = 0x1000_024E;
    /// The footer line one label read.
    pub const LINE_ONE_LABEL: u32 = 0x1000_0242;
    /// The footer line one value read.
    pub const LINE_ONE_VALUE: u32 = 0x1000_0243;
    /// Only container `0x10000247` has one.
    pub const METER: u32 = 0x1000_0247;
    /// The footer line two label read.
    pub const LINE_TWO_LABEL: u32 = 0x1000_0244;
    /// The footer line two value read.
    pub const LINE_TWO_VALUE: u32 = 0x1000_0245;
    /// The footer button read — "raise 1" / "train".
    pub const BUTTON: u32 = 0x1000_0246;
    /// The footer10 button read — "raise 10".
    pub const BUTTON_10: u32 = 0x1000_05EB;
}

/// `Meter`'s fill attribute, a float.
pub const METER_ATTR: u32 = 0x69;

/// A list row's two states, from the selection update's loop. See the module header.
pub mod row_state {
    pub const UNSELECTED: u32 = 1;
    pub const SELECTED: u32 = 6;
}

/// A footer button's two states.
pub mod button_state {
    pub const ENABLED: u32 = 1;
    pub const DISABLED: u32 = 0x0D;
}

/// The `StringInfo` ids the footer resolves, by their symbolic names.
///
/// The names are hashed with exactly as `credits.rs` does, so
/// nothing here writes a hash down.
pub mod string {
    pub const DEFAULT_SKILL_TITLE: &str = "ID_StatManagement_Footer_DefaultSkillTitle";
    pub const DEFAULT_ATTRIBUTE_TITLE: &str = "ID_StatManagement_Footer_DefaultAttributeTitle";
    pub const SKILL_CREDITS_LABEL: &str = "ID_StatManagement_Footer_SkillCreditsLabel";
    pub const EXPERIENCE_LABEL: &str = "ID_StatManagement_Footer_ExperienceLabel";
    pub const SKILL_CREDITS_TO_RAISE_LABEL: &str =
        "ID_StatManagement_Footer_SkillCreditsToRaiseLabel";
    pub const XP_TO_RAISE_LABEL: &str = "ID_StatManagement_Footer_XPToRaiseLabel";
    pub const INFINITY: &str = "ID_StatManagement_Header_XPToLevelMeterInfinity";
}

/// Resolve one `ID_*` token out of [`STRING_TABLE`], or fall back to the token itself.
///
/// The fall-back matters for the headless tests, which run without a string service; a footer that
/// silently drew nothing would be indistinguishable from a footer that was never written.
#[must_use]
pub fn label(ui: &UiSystem, token: &str) -> String {
    ui.resolve_string(
        STRING_TABLE,
        dereth_primitives::num::hash::str_hash(token.as_bytes()),
    )
    .unwrap_or_else(|| token.to_owned())
}

/// One integer string-info variable, rendered the way the client renders it.
///
/// Every number in this header and in both panels'
/// footers is an integer string-info variable in retail, and the string-info renderer
/// puts each one through the long-integer conversion ->
/// the number-to-string helper, which groups the digits with
/// the active language's thousands separator. A plain `.to_string()` at any of those sites would
/// drop the separator.
///
/// See [`super::numfmt`] for the two independent pins on the separator. It is a one-line
/// forward so that the call sites read as the client's own idiom rather than as a formatting
/// choice made here.
#[must_use]
pub fn num(v: impl Into<i128>) -> String {
    dereth_presentation::numfmt::language_number(v)
}

/// What one footer render put on screen. The panel keeps the last one so a test can read the
/// numbers back without going through glyph lists.
///
/// `title` is the *value* half only for a selection footer; `title_font` is the 0/1/2 index
/// the set/append-text-with-font call was handed for it.
///
/// **The field's name is this crate's and it is a misnomer.** The
/// 0/1/2 is the append-with-font call's **third** argument, which the text element uses to
/// index attribute `0x1B` — the **font-colour** array. The *font* argument is the second one,
/// which indexes attribute `0x1A`, and every call site in the two stat panels passes **0** for
/// it. See [`set_text_with_font`]. The name is kept
/// because tests read it back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FooterContent {
    pub title: String,
    /// The append-with-font call's **colour** index for the appended value, 0/1/2 —
    /// an index into attribute `0x1B`, not into the font array. See the struct's own note.
    pub title_font: u32,
    pub line_one_label: String,
    pub line_one_value: String,
    pub line_two_label: String,
    pub line_two_value: String,
    /// `None` when the chosen container has no meter.
    pub meter: Option<f32>,
    /// The footer button's state — [`button_state`].
    pub button: u32,
    /// The footer ten-raise button's state, and whether it is shown at all.
    pub button_10: u32,
    pub button_10_visible: bool,
}

/// One bound stat-management footer, addressed the way the eight getters address it.
#[derive(Debug, Clone, Copy)]
pub struct Footer {
    /// The sub-panel element (`SkillsPanel` `0x1000022C` or `AttributesPanel` `0x1000022B`), whose
    /// state picks the container.
    pub panel: ElemHandle,
    /// The state last written, i.e. what the getters will resolve against.
    pub state: u32,
}

impl Footer {
    #[must_use]
    pub const fn new(panel: ElemHandle) -> Self {
        Self {
            panel,
            state: state::DEFAULT,
        }
    }

    /// The footer getters' switch on the sub-panel's state.
    #[must_use]
    pub const fn container_for(state: u32) -> u32 {
        match state {
            state::SELECTION => container::SELECTION,
            state::SELECTION_METER => container::SELECTION_METER,
            _ => container::DEFAULT,
        }
    }

    /// Set the sub-panel's state — the one line that swaps the whole footer.
    pub fn set_state(&mut self, ui: &mut UiSystem, s: u32) {
        self.state = s;
        ui.set_state(self.panel, dereth_ui::StateId(s));
    }

    /// Is `h` inside **this** sub-panel's subtree?
    ///
    /// **This is load-bearing.** The original skills and attributes controllers each
    /// receive messages from their own element subtree side by side under the same page, and every id in this
    /// module -- the list box `0x1000023D`, the three containers, all eight children, both
    /// buttons -- appears **twice** in the live tree, once under each. In the client that is
    /// harmless: each original controller receives only its own subtree's messages,
    /// so a click on `0x10000246` is delivered to exactly one of them. This crate's panels are
    /// plain structs and the screen fans one message out to all of them, so without this test the
    /// first panel offered a `BUTTON_CLICKED` on `0x10000246` would answer for the other one's
    /// button.
    ///
    /// [`crate::panels::skills::SkillsPanel::item_under`] already scopes the *selection* arm this
    /// way, because a row handle is only a member of its own list. The buttons have no such
    /// natural scope, so they are scoped by ancestry.
    #[must_use]
    pub fn owns(&self, ui: &UiSystem, mut h: ElemHandle) -> bool {
        for _ in 0..16 {
            if h == self.panel {
                return true;
            }
            match ui.parent(h) {
                Some(p) => h = p,
                None => return false,
            }
        }
        false
    }

    /// The common half of all eight getters.
    #[must_use]
    pub fn child(&self, ui: &UiSystem, id: u32) -> Option<ElemHandle> {
        let c = ui.get_child_recursive(self.panel, ElementId(Self::container_for(self.state)))?;
        ui.get_child_recursive(c, ElementId(id))
    }

    /// Set the text of one footer child.
    pub fn set_text(&self, ui: &mut UiSystem, id: u32, s: &str) -> bool {
        let Some(h) = self.child(ui, id) else {
            return false;
        };
        match ui.text_element_mut(h) {
            Some(t) => {
                t.set_text(s);
                true
            }
            None => false,
        }
    }

    /// Set text with a colour and a font index — the third argument this crate has
    /// had no way to express, which is why a buffed skill drew in the plain font.
    ///
    /// `TextElement::set_text` hard-codes font index 0 into add_text; the glyph list
    /// itself has carried a per-glyph font index all along, so this is the missing call and not a
    /// missing capability.
    pub fn set_text_with_font(
        &self,
        ui: &mut UiSystem,
        id: u32,
        s: &str,
        font: u32,
        color: u32,
    ) -> bool {
        let Some(h) = self.child(ui, id) else {
            return false;
        };
        set_text_with_font(ui, h, s, font, color)
    }

    /// Write the footer meter's fill (attribute `0x69`).
    ///
    /// Returns false when the current container has no meter, which is the *default* and
    /// *untrained* footers — both of which really do lack one in the shipped layout.
    pub fn set_meter(&self, ui: &mut UiSystem, fill: f32) -> bool {
        let Some(h) = self.child(ui, child::METER) else {
            return false;
        };
        crate::bind::set_attr_float(ui, h, METER_ATTR, fill);
        true
    }

    /// The button element's state write on the footer button / the ten-raise button.
    ///
    /// **Writing the attribute is what that override does**, and it is not a shortcut round
    /// `UiSystem::set_state`: the button's state write reads its disabled attribute `0x0D`; if
    /// "the new state is `0x0D`" differs from it, it writes the attribute and returns; otherwise it
    /// sets the state.
    ///
    /// Going through `UiSystem::set_state` instead reaches the same override — but from *inside*
    /// the behaviour lift, where the attribute handler's subclass half is skipped by construction
    /// (see `UiSystem::queue_set_state`'s note). That half is the only thing that updates the
    /// state from the attribute, so the attribute would change and the state would not,
    /// and the button would stay in `0x0D` while claiming to be enabled. That was measured, not
    /// guessed: the first version of this method used `set_state` and the raise button read back
    /// `0x0D` with `cost = 23` against `AvailableExperience = 23500`.
    pub fn set_button_state(&self, ui: &mut UiSystem, id: u32, s: u32) -> bool {
        let Some(h) = self.child(ui, id) else {
            return false;
        };
        set_button_state_at(ui, h, s);
        true
    }

    /// The state a footer button is actually in — what a test asserts against.
    #[must_use]
    pub fn button_state(&self, ui: &UiSystem, id: u32) -> Option<u32> {
        self.child(ui, id)
            .and_then(|h| ui.node(h))
            .map(|n| n.state.0)
    }

    pub fn set_button_visible(&self, ui: &mut UiSystem, id: u32, on: bool) -> bool {
        let Some(h) = self.child(ui, id) else {
            return false;
        };
        ui.set_visible(h, on);
        true
    }

    /// The selection footers' enable test, both times it appears: state `0xD` when
    /// `cost == 0 || available < cost`, else state `1`.
    #[must_use]
    pub const fn enable_for(cost: u64, available: u64) -> u32 {
        if !dereth_presentation::stats::can_raise(cost, available) {
            button_state::DISABLED
        } else {
            button_state::ENABLED
        }
    }
}

/// `UICore_Button_disabled` — attribute `0x0D`, the one the button's state write sets.
pub const ATTR_DISABLED: u32 = 0x0D;
/// `UICore_Button_toggled` — attribute `0x0E`, the toggle half of the same override.
pub const ATTR_TOGGLED: u32 = 0x0E;

/// The button state write on a handle. See [`Footer::set_button_state`].
pub fn set_button_state_at(ui: &mut UiSystem, h: ElemHandle, s: u32) {
    crate::bind::set_attr_bool(ui, h, ATTR_DISABLED, s == button_state::DISABLED);
}

/// The toggle arm of the same override — state `6` on an untoggled button sets attribute `0x0E`.
///
/// The client sets the state of thirteen toggle buttons and
/// this is where each of those lands.
pub fn set_toggle_button_state(ui: &mut UiSystem, h: ElemHandle, on: bool) {
    crate::bind::set_attr_bool(ui, h, ATTR_TOGGLED, on);
}

/// Set text with a colour and a font index on an arbitrary text element.
///
/// Kept as a free function as well as a [`Footer`] method because the skill *rows* need it too:
/// The client writes its value with a font index.
pub fn set_text_with_font(
    ui: &mut UiSystem,
    h: ElemHandle,
    s: &str,
    font: u32,
    color: u32,
) -> bool {
    let (font, color) = resolve_font_and_color(ui, h, font, color);
    let Some(t) = ui.text_element_mut(h) else {
        return false;
    };
    let metrics = std::sync::Arc::clone(&t.metrics);
    let tag = t.current_tag.clone();
    t.glyphs.flush();
    t.glyphs
        .add_text(0, s, metrics.as_ref(), color, font, tag.as_ref());
    t.cursor = t.glyphs.len();
    t.selection = dereth_ui::text::Selection::default();
    true
}

/// The second half of a selection footer's
/// title, which is how the value beside a skill's name gets its own colour. Same two indices as
/// [`set_text_with_font`].
pub fn append_text_with_font(
    ui: &mut UiSystem,
    h: ElemHandle,
    s: &str,
    font: u32,
    color: u32,
) -> bool {
    let (font, color) = resolve_font_and_color(ui, h, font, color);
    let Some(t) = ui.text_element_mut(h) else {
        return false;
    };
    let metrics = std::sync::Arc::clone(&t.metrics);
    let tag = t.current_tag.clone();
    let at = t.glyphs.len();
    t.glyphs
        .add_text(at, s, metrics.as_ref(), color, font, tag.as_ref());
    t.cursor = t.glyphs.len();
    true
}

/// `UICore_Text_font` — attribute `0x1A`, the **array** of font `DataFile`s the font index
/// selects from.
pub const ATTR_FONT: u32 = 0x1A;
/// `UICore_Text_fontColor` — attribute `0x1B`, the **array** of `Color`s the colour index
/// selects from.
///
/// `dereth_ui` reads element **0** of it into `TextElement::font_color` and keeps no array, so the
/// other entries are read back off the element here.
pub const ATTR_FONT_COLOR: u32 = 0x1B;

/// The client's whole body: element `index` of attribute `0x1B`, or **no
/// change** when the index is past the end of the array.
///
/// Returns `None` for "leave the colour alone", which is what the client does.
#[must_use]
pub fn font_color_at(ui: &UiSystem, h: ElemHandle, index: u32) -> Option<u32> {
    use dereth_assets::ui::PropertyValue;
    let node = ui.node(h)?;
    let merged = node.merged_properties();
    let PropertyValue::Array(a) = merged.get(ATTR_FONT_COLOR)? else {
        return None;
    };
    match a.get(usize::try_from(index).ok()?).map(|e| &e.value) {
        Some(PropertyValue::Color(c)) => Some(*c),
        _ => None,
    }
}

/// How many fonts attribute `0x1A` declares — the bound the client checks before it swaps the
/// current font.
#[must_use]
pub fn font_count(ui: &UiSystem, h: ElemHandle) -> usize {
    use dereth_assets::ui::PropertyValue;
    let Some(node) = ui.node(h) else { return 0 };
    match node.merged_properties().get(ATTR_FONT) {
        Some(PropertyValue::Array(a)) => a.len(),
        Some(PropertyValue::DataFile(_)) => 1,
        _ => 0,
    }
}

/// The font and colour the pair computes before it appends, with both of the
/// client's out-of-range rules applied.
///
/// Retail selects the current font from attribute `0x1A` by the font index, and the current
/// colour and tag colour from attributes `0x1B` and `0x1D` by the colour index. Each selection
/// **leaves its field unchanged** when the index is past the end of the array, so an index the layout does not declare is a no-op there and must
/// be one here. In this crate the font travels as a **per-glyph index** into
/// `TextElement::fonts`, and `dereth_ui::text::compose::place` resolves an index past the end to
/// `DataId(0)` — which rasterises **nothing at all**. Clamping here is therefore not cosmetic:
/// it is the difference between the client's "keep the last font" and a blank cell.
fn resolve_font_and_color(ui: &UiSystem, h: ElemHandle, font: u32, color: u32) -> (u32, u32) {
    let n = font_count(ui, h);
    let font = if usize::try_from(font).is_ok_and(|f| f < n) {
        font
    } else {
        0
    };
    // "Unchanged" is element **0** of the same array — what last set and
    // what `dereth_ui` put in `TextElement::font_color` when the layout was applied. `0xFFFFFFFF`
    // (opaque white) only when the element declares no `0x1B` at all, which is `TextElement`'s
    // own default.
    let current = font_color_at(ui, h, 0).unwrap_or(0xFFFF_FFFF);
    (font, font_color_at(ui, h, color).unwrap_or(current))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All eight footer getters begin with the same client sequence, which is one switch on the
    /// sub-panel's state repeated eight times.
    #[test]
    fn the_panel_state_picks_which_of_the_three_footer_containers_the_getters_read() {
        assert_eq!(Footer::container_for(state::DEFAULT), container::DEFAULT);
        assert_eq!(
            Footer::container_for(state::SELECTION),
            container::SELECTION
        );
        assert_eq!(
            Footer::container_for(state::SELECTION_METER),
            container::SELECTION_METER
        );
        // Anything else is the `default:` arm — not a panic and not the selection footer.
        assert_eq!(Footer::container_for(0), container::DEFAULT);
        assert_eq!(Footer::container_for(0x1000_0014), container::DEFAULT);
        // And the meter's id collides with the third container's id. That is real, not a typo:
        // `0x10000247` names both the container and the `Meter` inside it.
        assert_eq!(child::METER, container::SELECTION_METER);
    }

    /// Oracle: both selection footers' enable test. `cost == 0 || available < cost` takes the
    /// `0xD` arm, and only the other one takes `1`.
    #[test]
    fn a_raise_button_is_disabled_when_the_cost_is_zero_or_unaffordable() {
        assert_eq!(
            Footer::enable_for(0, 1_000_000),
            button_state::DISABLED,
            "cost 0 == at cap"
        );
        assert_eq!(
            Footer::enable_for(500, 499),
            button_state::DISABLED,
            "one short"
        );
        assert_eq!(
            Footer::enable_for(500, 500),
            button_state::ENABLED,
            "exactly enough"
        );
        assert_eq!(Footer::enable_for(500, 501), button_state::ENABLED);
        assert_eq!(Footer::enable_for(0, 0), button_state::DISABLED);
        // The two states are the ones the live buttons declare, and nothing else.
        assert_eq!(button_state::ENABLED, 1);
        assert_eq!(button_state::DISABLED, 0x0D);
    }

    /// Oracle: the client's row loop, whose two state literals are 6 and 1, cross-checked against
    /// the live row template `0x10000248`, which declares exactly those two states.
    #[test]
    fn the_two_row_states_are_the_ones_the_row_template_declares() {
        assert_eq!(row_state::SELECTED, 6);
        assert_eq!(row_state::UNSELECTED, 1);
        assert_ne!(row_state::SELECTED, row_state::UNSELECTED);
    }
}

// ---------------------------------------------------------------------------------------------
// The header — the character info update / the experience update
// ---------------------------------------------------------------------------------------------

/// The header children binds, above the list box.
///
/// The three labels are `0x10000234` *"Total Experience (XP):"*, `0x10000237` *"XP for next
/// level:"* and `0x1000023A` *"Character Level"*, which the **layout** supplies; the value fields
/// beside them are this crate's to write, or the page shows three static labels with no values.
pub mod header {
    /// The name text — the character info update.
    pub const NAME: u32 = 0x1000_0231;
    /// The heritage text — the gender/heritage display string plus the display title.
    pub const HERITAGE: u32 = 0x1000_0232;
    /// The PK status text — the pk status update.
    pub const PK_STATUS: u32 = 0x1000_0233;
    /// The total-XP text — the experience update.
    pub const TOTAL_XP: u32 = 0x1000_0235;
    /// The XP-to-level meter — attribute `0x69`, the same one the footer's meter uses.
    pub const XP_METER: u32 = 0x1000_0236;
    /// The XP-to-level text.
    pub const XP_TO_LEVEL: u32 = 0x1000_0238;
    /// The level text.
    pub const LEVEL: u32 = 0x1000_023B;
    /// The luminance label.
    pub const LUMINANCE_LABEL: u32 = 0x1000_05C5;
    /// The luminance value.
    pub const LUMINANCE: u32 = 0x1000_05C6;
}

/// The client's inputs, already joined by the host.
///
/// Defined in [`dereth_client_contract::statmgmt`], because
/// `GameView::experience_header` returns it and the contract crate may not depend on this one.
pub use dereth_client_contract::statmgmt::XpHeader;

/// What one header render put on screen.
///
/// Includes `heritage`, `pk_status` and the luminance pair; tests read all of them back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeaderContent {
    pub name: String,
    pub level: String,
    pub total_xp: String,
    pub xp_to_level: String,
    pub meter: f32,
    /// The heritage text -- `"<Gender> <Heritage>"` plus, after one space, the display title.
    pub heritage: String,
    /// The PK status text -- one of the three `ID_StatManagement_Header_PKStatus_*` strings.
    pub pk_status: String,
    /// The luminance label -- `L"Luminance:"`, or empty below the level gate.
    pub luminance_label: String,
    /// The luminance value -- `"<available> / <maximum>"`, or empty.
    pub luminance: String,
}

/// The wide literal the experience update writes into the luminance label.
///
/// It is not a `StringInfo`: retail writes the wide literal `L"Luminance:"` directly, so it
/// does not come from the string table.
pub const LUMINANCE_LABEL: &str = "Luminance:";

/// The narrow format string receives the **available** luminance first and the **maximum** second.
///
pub const LUMINANCE_FORMAT: &str = "%s / %s";

/// The minimum level (integer property `0x19`). Below it both fields are cleared.
///
pub const LUMINANCE_MIN_LEVEL: i32 = 200;

/// The experience system's xp to string.
///
/// The client's is `sprintf("%I64d")` followed by `GetNumberFormatA` with an explicit
/// `NUMBERFMTA { NumDigits: 0, LeadingZero: 0, Grouping: 3, lpDecimalSep: ".",
/// lpThousandSep: ",", NegativeOrder: 1 }` -- so the locale cannot change the separators and the
/// answer is plain three-digit grouping with commas. `NegativeOrder 1` is "minus sign, then the
/// number", which is what a leading `-` gives.
#[must_use]
pub fn xp_to_string(v: i64) -> String {
    dereth_presentation::numfmt::exact_number(v)
}

pub use dereth_presentation::stats::HeaderInputs;

impl Footer {
    /// The stat management panel's character info update and the experience update,
    /// as far as this build can source them.
    ///
    /// The header children hang off the sub-panel directly and **not** off one of the three
    /// state-selected footer containers, so they are resolved from [`Footer::panel`] rather than
    /// through [`Footer::child`].
    ///
    /// **All eight are written.** The heritage line's inputs
    /// are `PropertyInt` **`0xBC` `HeritageGroup`** and **`0x71` `Gender`**, not `0x71
    /// HeritageGroup` -- the char examine panel's creature info update reads property `0x71` as
    /// the gender and `0xBC` as the heritage group immediately before making the identical
    /// gender/heritage display call.
    ///
    /// Returns what was written, so a test can read the numbers back.
    ///
    /// The heritage line is
    /// the gender/heritage display string plus the display title, joined by the host and
    /// carried on [`HeaderInputs`]; the PK status is the client's three-way
    /// choice over the player's PK and PK-lite status; and the luminance pair is
    /// the experience update's own arm:
    ///
    /// If level (integer property `0x19`) is below 200 or maximum luminance (64-bit property 7)
    /// is zero, clear both label and value. Otherwise set the label to `L"Luminance:"`,
    /// format available luminance (64-bit property 6) and maximum luminance separately with
    /// the XP formatter, and join them as `"%s / %s"`, available first.
    ///
    pub fn update_header(&self, ui: &mut UiSystem, h: &HeaderInputs) -> HeaderContent {
        let xp = h.xp;
        let x = xp.unwrap_or_default();
        // The character info update shows `"???"` when the level property is absent.
        let level = if xp.is_some() {
            num(x.level)
        } else {
            "???".to_owned()
        };
        let xp_to_level = if x.at_cap || x.to_level == 0 {
            label(ui, string::INFINITY)
        } else {
            num(x.to_level)
        };
        let (luminance_label, luminance) = h.luminance_line();
        let c = HeaderContent {
            name: h.name.clone(),
            level,
            total_xp: num(x.total),
            xp_to_level,
            meter: x.meter_fill(),
            heritage: h.heritage_line(),
            pk_status: label(ui, h.pk.token()),
            luminance_label,
            luminance,
        };
        self.set_panel_text(ui, header::NAME, &c.name);
        self.set_panel_text(ui, header::LEVEL, &c.level);
        self.set_panel_text(ui, header::TOTAL_XP, &c.total_xp);
        self.set_panel_text(ui, header::XP_TO_LEVEL, &c.xp_to_level);
        self.set_panel_text(ui, header::HERITAGE, &c.heritage);
        self.set_panel_text(ui, header::PK_STATUS, &c.pk_status);
        self.set_panel_text(ui, header::LUMINANCE_LABEL, &c.luminance_label);
        self.set_panel_text(ui, header::LUMINANCE, &c.luminance);
        if let Some(h) = ui.get_child_recursive(self.panel, ElementId(header::XP_METER)) {
            crate::bind::set_attr_float(ui, h, METER_ATTR, c.meter);
        }
        c
    }

    /// Set the text of a child of the sub-panel itself.
    fn set_panel_text(&self, ui: &mut UiSystem, id: u32, s: &str) -> bool {
        let Some(h) = ui.get_child_recursive(self.panel, ElementId(id)) else {
            return false;
        };
        match ui.text_element_mut(h) {
            Some(t) => {
                t.set_text(s);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod number_tests {
    /// Behaviour: skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator
    #[test]
    fn exact_experience_preserves_signed_limits_while_language_values_stay_distinct() {
        assert_eq!(super::xp_to_string(i64::MIN), "-9,223,372,036,854,775,808");
        assert_eq!(super::xp_to_string(i64::MAX), "9,223,372,036,854,775,807");
        assert_ne!(
            super::num(9_007_199_254_740_993i64),
            super::xp_to_string(9_007_199_254_740_993)
        );
        assert_eq!(super::num(i128::MAX), super::num(i64::MAX));
        assert_eq!(super::num(i128::MIN), super::num(i64::MIN));
    }
}
