//! `ExaminationPanel` — the identify panel.
//!
//! It is the floating examination window (`<EXAM>`, element `0x100005F7`, type `0x1000004C`).
//!
//! # The chain an identify goes through
//!
//! Identifying an object is a **chain of three**, and only the middle link is a panel arm:
//!
//! | link | where it lives |
//! |---|---|
//! | the request — `Item_Appraise` `0x00C8` | `on_target_mode_button`'s `EXAMINE_BUTTON` arm emits `UiRequest::Examine`, and `interaction.rs` calls attempt_appraise, which sends it |
//! | the subject — the awaited appraisal id | written by the examine-object notice ([`ExaminationPanel::examine_object`]) |
//! | the reply — `0x00C9 Item_SetAppraiseInfo` | decoded and stored by the model, raising `Notice::AppraisalReady` |
//! | the panel — `ExaminationPanel` | this module; `<EXAM>` is in `HUD_START_VISIBILITY` as a window the examine call shows |
//!
//! The use button's whole job is one request; the identify button's request is only the start,
//! and everything else here happens when the answer comes back.
//!
//! # The one arm that decides whether the panel opens
//!
//! The examination panel's appraise-info write returns at once for id zero, or for an id that is
//! neither the awaited appraisal id nor the current one. A reply for the awaited id is *new*: the
//! client clears the awaited id, drops its busy count and makes this the current id. It then picks
//! the pane — a creature profile carrying string `5` or int `0x105` is the character pane, any
//! other creature the creature pane, anything else the item pane — and notes whether the object
//! differs from the one that pane last showed. With no live client object for the id it returns.
//! The title is string `0x34`, or else the object's name, with its stack size; the pane is
//! initialized with the object and given the profile; it becomes the active pane if it was not;
//! and only a *new* reply shows the window.
//!
//! Two things fall out of it that a plainer reading would have got wrong:
//!
//! * **The panel opens only for the appraisal it was *waiting* for.** A reply that matches
//!   the current appraisal id — which is what the client's 0.75-second combat re-poll
//!   produces — refills the panel and does **not** re-show it, so a panel the player closed stays
//!   closed while the poll runs.
//! * **A reply for an object with no live client object does nothing at all**, not
//!   even the title: the live-object check is above every write.
//!
//! # The first two blocks
//!
//! The item-examine panel's appraise-info write is **twenty-five** appraisal blocks, each
//! with its own formatting table (see the formatting-tables section below). The first two are the
//! two it reaches before any table:
//!
//! * The value block reads int property `0x13` and emits `"Value: " + commas` or `"Value: ???"`.
//! * The burden block reads int property `5` and emits `"Burden: " + commas` or
//!   `"Burden: Unknown"`.
//!
//! **Both of those run only because the shipped layout has no separate value or burden element.**
//! The item pane's constructor looks for `0x10000138` (a value element) and `0x10000139` (a
//! burden element) under `0x1000012E`, and **neither is in `classic_gameplay`** (measured: the
//! base field's children are `0x10000135`, `0x10000137`, `0x1000013C`, `0x100005F8`,
//! `0x1000013E`, `0x1000013F` and `0x1000013D`). The appraise-info write's
//! value-element-or-description fork therefore takes the *second* branch in retail too, and the
//! value and burden go into the description block.

use dereth_primitives::num::math;
use dereth_rules::advancement;
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::view::{AppraisalView, GameView};

/// `<EXAM>` — `FloatingExamination`, the window the appraise-info write's last line shows.
pub const WINDOW: ElementId = ElementId(0x1000_05F7);
/// The window's close control.
///
/// The floaty examination panel hides its window when message 1 comes from element
/// `0x100005F3`, then forwards to the base examination panel's element-message handler.
/// Other messages forward without hiding the window.
///
/// Message `1` is [`dereth_ui::msg::element::id::BUTTON_CLICKED`], which is what a
/// `Button` raises on release — and the shipped `classic_gameplay` gives `0x100005F3`
/// element type **1**, a 13x13 box at (284, 8)-(297, 21), the X in the frame's top-right corner.
/// Read out of the live tree, not assumed. \[verified\]
///
/// The hide is on the **window**, not on the button's parent chain: it is the floating
/// examination window itself.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_05F3);
/// The displayed-name text — the title line.
pub const DISPLAYED_NAME_TEXT: ElementId = ElementId(0x1000_012D);
/// The item pane's base field, from its constructor.
pub const ITEM_BASE: ElementId = ElementId(0x1000_012E);
/// The item description text, the block every appraisal block appends to.
pub const ITEM_DISPLAY_TEXT: ElementId = ElementId(0x1000_013C);
/// Shared by the creature
/// and character sub-UIs, which differ only in the sub-panel each shows.
pub const CREATURE_BASE: ElementId = ElementId(0x1000_0140);
/// The creature info sub-panel.
pub const CREATURE_SUB_PANEL: ElementId = ElementId(0x1000_014D);
/// The **3D portrait of the appraised
/// creature**, a `Viewport` (engine class `0x0D`).
///
/// Two things about this binding are unlike every other element in this panel, and both are in
/// the constructor:
///
/// * it is looked up off the pane's **parent element** — the `<EXAM>` window — not off the creature
///   base field that `0x1000014C`, `0x1000014D`, `0x1000014F`, `0x10000149` and `0x10000335` use;
/// * the result is cast to viewport type `0xD`, so a layout whose `0x10000148` were some other
///   class would leave the paper doll unbound and the panel would simply draw no portrait.
///
/// In the shipped layout it is a child of `CREATURE_BASE` with the **same box as
/// [`CREATURE_STAT_LIST`]** — retail draws the model behind the numbers, not beside them.
pub const CREATURE_PAPER_DOLL: ElementId = ElementId(0x1000_0148);
/// The `<Inscribe here>` edit box.
pub const ITEM_INSCRIPTION_TEXT: ElementId = ElementId(0x1000_013E);
/// The inscription signature text — the `--<scribe>` line under it.
pub const ITEM_INSCRIPTION_SIGNATURE_TEXT: ElementId = ElementId(0x1000_013F);
/// The inscription background — the panel the inscription-editable-state write
/// shows and hides with the box.
pub const ITEM_INSCRIPTION_BACKGROUND: ElementId = ElementId(0x1000_0137);
/// **The state that centres the placeholder, and it is authored in `client_portal.dat`.**
///
/// The inscription write first puts the inscription box into this state, and the "this item
/// already has an inscription" leg takes it straight off again with state `1`. The inscription
/// gaining-focus handling also sets 1, and the losing-focus handling puts `0x10000050`
/// back when the box is left empty.
///
/// Element `0x1000013E` ships `0x14 = Enum(2)` / `0x15 = Enum(4)` — near on both axes — on its
/// base state, and state `0x10000050` carries exactly two properties, `0x14 = Enum(1)` and
/// `0x15 = Enum(1)`, which [`dereth_ui::text::glyph::calc_justification`] reads as **centre**. So
/// the whole of "the placeholder is centred and a real inscription is not" is one state change
/// against data the layout already had. \[measured\]
///
/// State **1** is not authored on this element at all; a state change to a state with no
/// description records 0, which restores the base's near/near.
pub const INSCRIPTION_PLACEHOLDER_STATE: StateId = StateId(0x1000_0050);
/// The state the client asks for whenever the box is showing something other than the invitation.
pub const INSCRIPTION_NORMAL_STATE: StateId = StateId(1);
/// The level value text. Left unwritten, retail's *Character Level 1* would read as the words
/// *Character Level* and a gap.
pub const LEVEL_VALUE_TEXT: ElementId = ElementId(0x1000_014C);
/// The creature display name — the creature pane's first line, e.g. `Golem`.
pub const CREATURE_DISPLAY_NAME: ElementId = ElementId(0x1000_014E);
/// The list box receives nine information regions: six attributes followed by three vitals.
pub const CREATURE_STAT_LIST: ElementId = ElementId(0x1000_0149);

/// The pose, the light and the camera of the identify
/// window's 3D portrait.
///
/// Every constant here is the client's own initialization value, including the camera
/// argument that is not a look-at target.
pub mod portrait {
    use dereth_primitives::Vec3;

    /// The light's direction, `(0.3, 1.9, 0.65)`, set by initialization. The same direction the paper doll uses. \[verified\]
    pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, 1.9, 0.65);

    /// The light's intensity, `2.0`.
    /// \[verified\]
    pub const LIGHT_INTENSITY: f32 = 2.0;

    /// The light's type: `1`, which is `DISTANT_LIGHT`. \[verified\]
    pub const LIGHT_TYPE: u32 = 1;

    /// The heading the portrait clone is set to, `191.3679` degrees.
    /// The same three-quarter pose the paper doll takes, and the reason the creature is not seen
    /// edge-on. \[verified\]
    pub const HEADING_DEGREES: f32 = 191.3679;

    /// The camera-fit factor, `1.2071068`. \[verified\]
    ///
    /// It is `cot(22.5 deg) / 2`, and the viewport's field of view is
    /// `0.7853982` radians (45 degrees). So [`camera_position`] is the textbook "back off
    /// far enough to fit this extent in a 45-degree vertical field of view".
    pub const CAMERA_FIT: f32 = 1.2071068;

    /// The camera write's **second** argument, explicitly zeroed before the call.
    ///
    /// It is not a look-at target. The viewport's camera write hands it to
    /// the creature mode's camera-direction write, which is `euler_set_rotate(0,0,0)` followed
    /// by `rotate(v)` — so `(0,0,0)` leaves the camera's orientation at **identity**,
    /// looking straight down `+Y`. A rebuild that read it as a look-at point would get the same
    /// picture here only by accident.
    pub const CAMERA_DIRECTION: (f32, f32, f32) = (0.0, 0.0, 0.0);

    /// Initialization's camera placement, given the object's bounding box and the
    /// **viewport element's own** width and height: with the box extents `dx`, `dy`, `dz`, when
    /// `dz/dx < h/w` then `dz` becomes `dx * (h/w)`, and the camera sits at the box centre moved
    /// back along `-Y` by `dz * 1.2071068 + dy * 0.5`.
    ///
    /// The clamp is the piece worth naming: when the element is taller relative to its width than
    /// the object is deep relative to its width, the *fitted* extent becomes the width scaled by
    /// the element's aspect, so a wide flat creature in a tall box is pushed back far enough for
    /// its width to fit rather than its depth. The comparison is strict — the multiply runs
    /// only when `dz/dx < h/w` — and a
    /// degenerate box that makes `dz/dx` NaN takes the *unordered* branch and keeps `dz`, which is
    /// what Rust's `NaN < aspect == false` does here.
    #[must_use]
    pub fn camera_position(min: Vec3, max: Vec3, width: i32, height: i32) -> Vec3 {
        let dx = max.x - min.x;
        let dy = max.y - min.y;
        let mut dz = max.z - min.z;
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: element boxes are a few hundred pixels; the client's own int-to-float load is
        // the same conversion.
        let aspect = height as f32 / width as f32;
        if dz / dx < aspect {
            dz = dx * aspect;
        }
        // The midpoint, component by component.
        let c = Vec3::new(
            (max.x + min.x) * 0.5,
            (max.y + min.y) * 0.5,
            (max.z + min.z) * 0.5,
        );
        Vec3::new(c.x, c.y - (dz * CAMERA_FIT + dy * 0.5), c.z)
    }
}
/// The client's three row children, shared with `AttributesPanel`'s rows
/// (`panels::attributes`' `row::LABEL` / `row::VALUE`): `0x10000129` icon, `0x1000012A` label,
/// `0x1000012B` value.
pub const ROW_LABEL: ElementId = ElementId(0x1000_012A);
/// See [`ROW_LABEL`].
pub const ROW_VALUE: ElementId = ElementId(0x1000_012B);
/// The character info sub-panel.
pub const CHARACTER_SUB_PANEL: ElementId = ElementId(0x1000_014F);
/// The extra-info list, bound from the creature base as a list box (type `5`): the list
/// box every row lands in.
///
/// The shipped layout has it at `(0, 278)-(291, 364)`, a `ListBox` whose template row 0
/// carries the same [`ROW_LABEL`] / [`ROW_VALUE`] pair the stat list uses.
pub const MISC_LIST: ElementId = ElementId(0x1000_0335);
/// The heritage text.
pub const CHAR_HERITAGE_TEXT: ElementId = ElementId(0x1000_0150);
/// The profession text.
pub const CHAR_PROFESSION_TEXT: ElementId = ElementId(0x1000_0151);
/// The PK-status text.
pub const CHAR_PK_STATUS_TEXT: ElementId = ElementId(0x1000_0152);
/// The allegiance-name text; the **creature**
/// pane clears the same element by looking it up afresh.
pub const ALLEGIANCE_NAME_TEXT: ElementId = ElementId(0x1000_053A);
/// The spell pane's base field — the third sibling of `<EXAM>`'s three panes.
pub const SPELL_BASE: ElementId = ElementId(0x1000_0153);
/// The spell pane's component list — the element the examination panel's element-message
/// handler compares against as a literal (`0x10000137 + 7 + 0x1EF`).
///
/// This panel does not bind it; the spell pane does. It is named here because
/// [`ExaminationPanel::select_spell_component`] is that arm's body and a literal pinned once is
/// what makes a wrong id falsifiable.
pub const SPELL_COMPONENT_LIST: ElementId = ElementId(0x1000_032D);
/// The message that arm answers: `ListBox`'s selection-changed, **4**.
pub const SPELL_COMPONENT_LIST_MESSAGE: u32 = 4;

/// Which examination pane the appraisal result selects: item, creature, character or spell.
/// The original selector stores four pane pointers; this enum carries the same choice without
/// claiming their native offsets as a Rust layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExamineSubUi {
    Item,
    Creature,
    Char,
    Spell,
}

/// Thousands separators, right to left.
///
/// The client calls it on a string it has just built from the integer, and falls back to
/// `"???"` when it answers false. Written here for a value that is already an integer, so the
/// failure branch is unreachable and is expressed by the caller taking the `Option` route instead.
#[must_use]
pub fn insert_commas(v: i32) -> String {
    let neg = v < 0;
    let digits = v.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// The examination panel's title text write.
///
/// `stackSize < 2` is the name alone; otherwise `convert(stackSize) + " " + name`, with `"???"`
/// standing in for a count that would not convert. Corroborated by
/// the UI item element's tooltip update, whose wide literal is `L"%d %s"` — *count then
/// name* (`items::widget::ItemSlot::update_tooltip`).
#[must_use]
pub fn title_text(name: &str, stack_size: u32) -> String {
    if stack_size < 2 {
        name.to_string()
    } else {
        format!("{stack_size} {name}")
    }
}

/// The item-examine panel's appraisal show value info.
#[must_use]
pub fn value_line(value: Option<i32>) -> String {
    match value {
        None => "Value: ???".to_string(),
        Some(v) => format!("Value: {}", insert_commas(v)),
    }
}

/// The item-examine panel's appraisal show burden info.
///
/// Note the two absent-value strings differ — `"Value: ???"` and `"Burden: Unknown"` — and both are
/// the client's own literals rather than one shared word.
#[must_use]
pub fn burden_line(burden: Option<i32>) -> String {
    match burden {
        None => "Burden: Unknown".to_string(),
        Some(v) => format!("Burden: {}", insert_commas(v)),
    }
}

/// Append one block to the description text.
///
/// When the description already holds more than one glyph a separator goes first — one newline
/// for a same-line block, two otherwise — and then the text, in the block's colour.
///
/// The `> 1` test is on the *glyph list*, so an empty block adds no separator — which is what makes
/// the first line of a fresh panel start at the top rather than after a blank.
#[must_use]
pub fn add_item_info(so_far: &str, text: &str, same_line: bool) -> String {
    if so_far.is_empty() {
        return text.to_string();
    }
    let sep = if same_line { "\n" } else { "\n\n" };
    format!("{so_far}{sep}{text}")
}

// =================================================================================================
// The enchantment highlighting
// =================================================================================================
//
// **The add-item-info step's second argument is a colour index, not a font index.** The text
// append receives the width-converted
// string as argument 1, literal zero as argument 2, and this colour index as argument 3.
// Argument 2 selects the font from property `0x1A`; argument 3 selects the colour from `0x1B`.
//
// The authored data settles it beyond argument. The description pane `0x1000013C`, read off the
// live tree, carries **one** font at `0x1A`
// (`0x40000001`) and **three colours** at `0x1B`:
//
// | index | colour | this file's name |
// |---|---|---|
// | 0 | `0xFFFFFFFF` white | the plain font |
// | 1 | `0xFF00FF00` green | `MOD_HIGH_FONT`, initialized to 1 |
// | 2 | `0xFFFF0000` red | `MOD_LOW_FONT`, initialized to 2 |
//
// So a raised stat is **green** and a lowered one **red**, out of the pane's own three-colour
// array, and a per-run *font* would have had nothing to select: the array at `0x1A` has one entry.
// `TextElement` got the per-run font anyway (it is additive and the client really does have
// it), but it is not what this block uses.

/// The raised-stat colour index, set to **1** by the constructor.
///
/// An index into the description pane's `0x1B` colour array, whose element 1 the shipped layout
/// authors as `0xFF00FF00`: a stat the enchantment **raised**.
pub const MOD_HIGH_FONT: u8 = 1;
/// The lowered-stat colour index, set to **2** by the constructor -- element 2 of
/// the same array, `0xFFFF0000`: a stat the enchantment **lowered**.
pub const MOD_LOW_FONT: u8 = 2;
/// The colour a **failed assess** puts on the creature pane's nine value cells.
///
/// Not a field of anything: the attribute info region's update uses the literal colour index `3`
/// when the profile's success flag is zero, and the secondary-attribute info region's update does
/// the same.
///
/// **It is yellow, and that is authored data rather than a name.** The stat row's value cell
/// `0x1000012B` carries **four** colours at `0x1B` where the item description pane `0x1000013C`
/// carries three, read off the live element:
///
/// | index | colour | who selects it |
/// |---|---|---|
/// | 0 | `0xFFFFFFFF` white | the plain arm, and every plain text write |
/// | 1 | `0xFF00FF00` green | [`MOD_HIGH_FONT`] |
/// | 2 | `0xFFFF0000` red | [`MOD_LOW_FONT`] |
/// | 3 | `0xFFFFFF00` **yellow** | this, and nothing else in the client |
///
/// So retail's yellow is the fourth entry of an array the item pane cannot even index.
pub const UNKNOWN_FONT: u8 = 3;

/// The attribute info region's update and the attribute2nd info region's update —
/// the colour one value cell is drawn in.
///
/// The ladder both functions spell out, in this order: a failed assess is colour `3`; otherwise a
/// stat with an enchantment modifier is colour `(raised == 0) + 1`; otherwise `0`.
///
/// `(raised == 0) + 1` is [`MOD_LOW_FONT`] when the
/// enchantment did not raise the stat and [`MOD_HIGH_FONT`] when it did — the same two indices
/// the item pane uses, out of the same kind of array.
///
/// **The failure colour does not depend on the text.** `???` is chosen separately, by
/// the attribute read answering zero; a failed assess that still carried a number would
/// colour the number. And it is the **value** cell only: the value text is the
/// only element either function touches, so the labels stay in the element's own colour, and
/// the level cell's plain text write, which carries no colour, leaves it alone as well.
#[must_use]
pub fn info_region_color(success: bool, enchanted: Option<bool>) -> u8 {
    if !success {
        return UNKNOWN_FONT;
    }
    match enchanted {
        Some(true) => MOD_HIGH_FONT,
        Some(false) => MOD_LOW_FONT,
        None => 0,
    }
}

/// Every property the two highlighting blocks ask the enchantment-modifier reads about, with
/// whether the question goes to the **float** table. `(key, is_float)`.
///
/// It lives in [`dereth_client_contract::panels::examination`], because `dereth_client::hud`
/// walks it to decide which resolved values to send.
pub use dereth_client_contract::panels::examination::HIGHLIGHTED_PROPERTIES;

/// The add-item-info colour argument for one line, from the profile's resolved enchantment bits.
///
/// The ladder every call site in both blocks spells out: not enchanted is `0`, enchanted but not
/// raised is [`MOD_LOW_FONT`], raised is [`MOD_HIGH_FONT`].
///
/// `keys` is a list because the damage line asks **twice**: the int modifier `0x2C` and, only if
/// that answers 0, the float modifier `0x16`. Every other line asks once.
#[must_use]
pub fn mod_color(p: &AppraisalView, keys: &[u32]) -> u8 {
    for k in keys {
        if let Some(raised) = p.enchantment_mods.get(k) {
            return if *raised { MOD_HIGH_FONT } else { MOD_LOW_FONT };
        }
    }
    0
}

/// One add-item-info call: the block's text, its `same_line` separator flag, and the
/// **colour index** it passes as the second argument.
///
/// The pane is built from runs rather than one flat `String`, because a flat string cannot carry
/// a colour per block and the enchantment highlighting would be lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    pub text: String,
    pub same_line: bool,
    pub color: u8,
}

fn push_item_info(out: &mut Vec<ItemInfo>, text: String, same_line: bool, color: u8) {
    out.push(ItemInfo {
        text,
        same_line,
        color,
    });
}

/// The runs as one plain string, through [`add_item_info`]'s own separator rule -- what
/// `ExaminationPanel::item_text` reports.
#[must_use]
pub fn flatten_item_info(runs: &[ItemInfo]) -> String {
    let mut text = String::new();
    for r in runs {
        text = add_item_info(&text, &r.text, r.same_line);
    }
    text
}

// =================================================================================================
// The formatting tables
// =================================================================================================
//
// The tables are the **client's**, taken from retail's own strings for each appraisal block.
// Nothing here is invented and nothing is taken from a community page.
//
// The client calls its twenty-five blocks in one fixed order, and this build implements the ones
// the two
// reported subjects and the recorded corpus reach. What is still absent is listed in
// [`ITEM_BLOCKS_NOT_IMPLEMENTED`], by name, so "not drawn" is a list and not a silence.

/// The appraisal blocks that this build does **not**
/// implement, in the client's call order. A denominator for the item pane.
///
/// The eight this build does implement are value, burden, weapon-and-armour data, armour mods,
/// usage, lock appraisal and description, plus the inscription write, which is not a block but
/// the edit box beside them.
///
/// **The count is load-bearing and is asserted** — see
/// `the_unimplemented_block_list_is_the_calls_this_build_does_not_make`.
pub const ITEM_BLOCKS_NOT_IMPLEMENTED: &[&str] = &[
    // **This list is empty.** Every block is implemented, including tinkering
    // [`tinkering_lines`], sets [`set_lines`], ratings [`ratings_lines`], defense mods
    // [`defense_mod_lines`], level limit [`level_limit_lines`], wield requirements
    // [`wield_requirement_lines`], usage limit [`usage_limit_lines`], item level
    // [`item_level_lines`], activation requirements [`activation_requirement_lines`], caster data
    // [`caster_data_lines`], boost value [`boost_value_lines`], heal kit values
    // [`heal_kit_lines`], capacity [`capacity_lines`], mana stone [`mana_stone_lines`], remaining
    // uses [`remaining_uses_lines`], craftsman [`craftsman_lines`], the bool `0x45` line
    // [`cannot_be_sold_lines`], rare info [`rare_info_lines`], the lock block, the special
    // properties and both spell blocks, short and full magic info.
    //
    // **An empty list is a claim, not an absence**, and it is asserted empty by
    // `the_unimplemented_block_list_is_the_calls_this_build_does_not_make`. What is still partial
    // lives one level down, in [`SPECIAL_PROPERTIES_NOT_IMPLEMENTED`] and
    // [`CREATURE_MISC_NOT_IMPLEMENTED`].
];

/// The **creature and character** panes' extra-info list.
///
/// The row helper puts one two-cell row into the list box
/// [`MISC_LIST`] (the extra-info list, bound by the constructor), whose cells are the
/// same `0x1000012A` label / `0x1000012B` value pair [`ROW_LABEL`] and [`ROW_VALUE`] name. It has
/// exactly **two** callers, and this build now draws both:
///
/// * The creature examine panel's appraise info write — [`creature_misc_rows`];
/// * The character pane uses [`char_misc_rows`], plus the four text
///   elements bound beside the list ([`CHAR_HERITAGE_TEXT`],
///   [`CHAR_PROFESSION_TEXT`], [`CHAR_PK_STATUS_TEXT`], [`ALLEGIANCE_NAME_TEXT`]) and the title
///   bar that the allegiance data's full name read overwrites.
///
/// The list box itself is bound by [`ExaminationPanel::post_init`] and its rows are counted by
/// [`ExaminationPanel::misc_rows_drawn`], so "the pane drew nothing" and "the layout has no row
/// template" are different answers.
///
/// **An empty list is a claim, not an absence**, and it is asserted empty by
/// the creature-appraisal tests. Nothing was left out for want of data: every key the two
/// functions read is an assessment property, so the server sends all of them on appraisal, and
/// the only value in either function that is read and never drawn is
/// int `0x143` `HealingBoostRating` — dead in retail too (see [`CHARACTER_RATING_ROWS`]).
pub const CREATURE_MISC_NOT_IMPLEMENTED: &[&str] = &[];

/// The parts of the special-properties block that [`special_properties_lines`]
/// does **not** draw.
///
/// A block that is *partly* implemented is worse than one that is not, unless the missing part is
/// named — the same rule [`ITEM_BLOCKS_NOT_IMPLEMENTED`] exists for, one level down. **The count
/// is asserted**, by `the_special_properties_block_names_what_it_does_not_draw`.
///
/// The pair is one feature: float `0xA7` is the cooldown and int `0x118` is the shared
/// cooldown group, and between them they draw the wide lines `"Cooldown When Used: "` and
/// `"Cooldown Remaining: "` through
/// the delta-time formatter and a player-description query for the group's own remaining
/// time, which connect to the journal's formatter and the player registry query.
pub const SPECIAL_PROPERTIES_NOT_IMPLEMENTED: &[&str] = &[];

/// `AMMO_TYPE` — the ammunition type. The same four values as
/// `dereth_client_model::appraisal_model::ammo_type`; this crate has no edge to `dereth-client-model` and the enum is
/// three comparisons in the weapon-and-armour block's two ammunition arms.
pub mod ammo_type {
    pub const NONE: u16 = 0;
    pub const ARROW: u16 = 1;
    pub const BOLT: u16 = 2;
    pub const ATLATL: u16 = 3;
}

/// The `EquipMask` bits the weapon-and-armour block tests `_valid_locations` against. Mirrors
/// `dereth_client_model::appraisal_model::equip` for the same reason as [`ammo_type`].
pub mod equip {
    pub const MELEE_WEAPON: u32 = 0x0010_0000;
    pub const SHIELD: u32 = 0x0020_0000;
    pub const MISSILE_WEAPON: u32 = 0x0040_0000;
    pub const AMMUNITION: u32 = 0x0080_0000;
    pub const WAND: u32 = 0x0200_0000;
    /// Mask `0x3F00000` — any weapon-ish slot at all.
    pub const ANY_WEAPON: u32 = 0x03F0_0000;
    /// Mask `0x2500000` — the slots that show Speed (and, with `MISSILE_WEAPON`, Range).
    pub const SPEED_SHOWN: u32 = MELEE_WEAPON | MISSILE_WEAPON | WAND;
    /// Mask `0x8007FFF` — armour and clothing, the clothing-priority name fallback.
    pub const CLOTHING: u32 = 0x0800_7FFF;
}

/// The skill system's attribute name read and the attribute2nd name read -- six
/// wide literals each.
///
/// It lives in [`dereth_client_contract::panels::examination`]: `dereth_client::hud` names
/// both when it composes an enchantment line.
pub use dereth_client_contract::panels::examination::{attribute_name, vital_name};

/// The creature pane's six attribute rows, **in drawn order**.
///
/// The original creature pane adds six attribute regions to list box `0x10000149`, in this order:
/// `1, 2, 4, 3, 5, 6`. Coordination is constructed
/// **before** quickness, which is why retail's panel reads *Strength Endurance Coordination
/// Quickness Focus Self* and the attribute ids do not run 1..6 down the column.
pub const CREATURE_ATTRIBUTE_ROWS: [u32; 6] = [1, 2, 4, 3, 5, 6];

/// The creature pane's three vital rows, in drawn order, with the show-percentage flag.
///
/// The same constructor's last three regions: `(2, true)`, `(4, false)`, `(6, false)` — Health with
/// a percentage, Stamina and Mana without. Each region's maximum id is its current id minus 1,
/// which is the maximum beside each current.
pub const CREATURE_VITAL_ROWS: [(u32, bool); 3] = [(2, true), (4, false), (6, false)];

/// One attribute's value cell.
///
/// The attribute read answering zero is `"???"`; otherwise the wide literal, `L"%d"`.
#[must_use]
pub fn attribute_value(v: Option<u32>) -> String {
    v.map_or_else(|| "???".to_string(), |v| v.to_string())
}

/// One vital's value cell, and **four** formats.
///
/// All wide literals:
///
/// | condition | literal |
/// |---|---|
/// | both vitals known, assess succeeded, show-percent | `L"%d/%d (%d %%)"`  |
/// | both known, succeeded, not percent | `L"%d/%d"`  |
/// | both known, assess **failed**, percent | `L"%d %%"`  |
/// | either unknown, or failed without percent | `L"???"`  |
///
/// The percentage is `MulDiv(100, current, max)` — integer, rounded to nearest, which is what
/// makes 31 of 31 read `100 %` and 12 of 31 read `39 %` rather than 38.
#[must_use]
pub fn vital_value(
    current: Option<u32>,
    max: Option<u32>,
    show_percent: bool,
    success: bool,
) -> String {
    let (Some(c), Some(m)) = (current, max) else {
        return "???".to_string();
    };
    let pct = mul_div(100, c, m);
    match (success, show_percent) {
        (true, true) => format!("{c}/{m} ({pct} %)"),
        (true, false) => format!("{c}/{m}"),
        (false, true) => format!("{pct} %"),
        (false, false) => "???".to_string(),
    }
}

/// `MulDiv` — `(a * b + c / 2) / c`, rounded half away from zero, which is the Win32 contract and
/// what the vital info region's update calls.
#[must_use]
pub fn mul_div(a: u32, b: u32, c: u32) -> u32 {
    if c == 0 {
        return 0;
    }
    let (a, b, c) = (u64::from(a), u64::from(b), u64::from(c));
    u32::try_from((a * b + c / 2) / c).unwrap_or(u32::MAX)
}

/// The modifier to string — `sprintf("%c%d%%", sign, magnitude)`.
///
/// Compare `x` with 1.0 to choose '+' for `x >= 1.0` and '-' otherwise. Compute the
/// magnitude as `trunc(abs(1.0 - x) * 100.0 + 0.5)`, preserving that operation order.
///
/// So a `damage_mod` of exactly 1.0 prints `"+0%"` — the Training Shortbow's line, and the reason
/// a zero modifier is not blank.
#[must_use]
// The client truncates toward zero, which is what the `+ 0.5` above is compensating for; the
// cast **is** that truncation, so the lint is answered rather than avoided.
#[allow(clippy::cast_possible_truncation)]
pub fn modifier_to_string(x: f64) -> String {
    let sign = if x < 1.0 { '-' } else { '+' };
    let magnitude = ((1.0 - x).abs() * 100.0 + 0.5) as i64;
    format!("{sign}{magnitude}%")
}

/// `Sprintf("%c%.1lf%%", sign, |1 - x| * 100)`, one decimal
/// and **no** half-up bias.
#[must_use]
pub fn small_modifier_to_string(x: f64) -> String {
    let sign = if x < 1.0 { '-' } else { '+' };
    format!("{sign}{:.1}%", (1.0 - x).abs() * 100.0)
}

/// The appraisal system's weapon time to string, also implemented as
/// `dereth_client_model::combat::weapon_time_to_string` and repeated here because this crate does not depend
/// on `dereth-client-model`. The five bands are `<11 Very Fast`, `<31 Fast`, `<=49 Average`, `<80 Slow`,
/// else `Very Slow` — so the Training Shortbow's 40 is `Average`.
#[must_use]
pub fn weapon_time_to_string(t: i32) -> &'static str {
    if t < 11 {
        "Very Fast"
    } else if t < 31 {
        "Fast"
    } else if t <= 49 {
        "Average"
    } else if t < 80 {
        "Slow"
    } else {
        "Very Slow"
    }
}

/// The client's nine names, joined with `/`. The same table
/// as `dereth_client_model::combat::damage_type_to_string`.
#[must_use]
pub fn damage_type_to_string(mask: u32) -> String {
    const NAMES: [(u32, &str); 9] = [
        (0x01, "Slashing"),
        (0x02, "Piercing"),
        (0x04, "Bludgeoning"),
        (0x08, "Cold"),
        (0x10, "Fire"),
        (0x20, "Acid"),
        (0x40, "Electrical"),
        (0x400, "Nether"),
        (0x800, "Prismatic"),
    ];
    NAMES
        .iter()
        .filter(|(m, _)| mask & m != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("/")
}

/// The skill name read -- the 54-arm switch, in the client's order.
///
/// It lives in [`dereth_client_contract::panels::examination`], because `dereth_client::hud`
/// is what resolves a spell's or an item's skill id to the identify panel's label. All 54 names
/// are in the client's order.
pub use dereth_client_contract::panels::examination::skill_to_string;

/// The weapon-and-armour block's switch on int `0x161` — the parenthesised
/// weapon family appended to the skill name. Ten literals, each with its **leading
/// space**, which is why the line reads `Skill: Missile Weapons (Bow)` and not `...Weapons(Bow)`.
#[must_use]
pub fn weapon_type_suffix(t: i32) -> Option<&'static str> {
    Some(match t {
        1 => " (Unarmed Weapon)",
        2 => " (Sword)",
        3 => " (Axe)",
        4 => " (Mace)",
        5 => " (Spear)",
        6 => " (Dagger)",
        7 => " (Staff)",
        8 => " (Bow)",
        9 => " (Crossbow)",
        10 => " (Thrown)",
        _ => return None,
    })
}

/// The eight labels, by `DAMAGE_TYPE`.
#[must_use]
pub fn resistance_label(damage_type: u32) -> Option<&'static str> {
    Some(match damage_type {
        0x01 => "Slashing: ",
        0x02 => "Piercing: ",
        0x04 => "Bludgeoning: ",
        0x08 => "Cold: ",
        0x10 => "Fire: ",
        0x20 => "Acid: ",
        0x40 => "Electric: ",
        0x400 => "Nether: ",
        _ => return None,
    })
}

/// The same function's adjective ladder uses bands of 0.4, open at the top.
#[must_use]
pub fn resistance_adjective(m: f32) -> &'static str {
    if m.abs() <= 0.0002 {
        "No"
    } else if m <= 0.4 {
        "Poor"
    } else if m <= 0.8 {
        "Below Average"
    } else if m < 1.2 {
        "Average"
    } else if m < 1.6 {
        "Above Average"
    } else if m < 2.0 {
        "Excellent"
    } else {
        "Unparalleled"
    }
}

/// The same function's **negative** arm: a modifier below `-0.0002` replaces the
/// whole line with one of eight sentences rather than decorating the label.
#[must_use]
pub fn resistance_curse(damage_type: u32) -> Option<&'static str> {
    Some(match damage_type {
        0x01 => "Your armor will rend and slash you if hit.",
        0x02 => "Your armor will cave in and pierce you if hit.",
        0x04 => "Your armor will shatter and bruise you if hit.",
        0x08 => "Your armor is unnaturally cold.",
        0x10 => "Your armor is flammable.",
        0x20 => "Your armor itches and burns your skin.",
        0x40 => "Your armor is extremely conductive.",
        0x400 => "Your armor is infused with shadow.",
        _ => return None,
    })
}

/// One resistance row: label, adjective, and the effective level in brackets.
///
/// The suffix is `" (%.0f)"` (**two** spaces) over `armor_level * modifier`, appended
/// after the adjective — so a level-20 piece with a 1.0 modifier reads
/// `Slashing: Average  (20)`.
#[must_use]
pub fn resistance_line(damage_type: u32, armor_level: i32, m: f32) -> Option<String> {
    let label = resistance_label(damage_type)?;
    if m < -0.0002 {
        return resistance_curse(damage_type).map(str::to_string);
    }
    let effective = (f64::from(armor_level) * f64::from(m)).round();
    Some(format!(
        "{label}{}  ({effective:.0})",
        resistance_adjective(m)
    ))
}

/// The client's **drawn** order for the eight resistances, with the
/// `ArmorProfile` wire slot each reads.
///
/// The order is the client's eight calls and is **not** the wire order: fire is drawn
/// fourth and cold fifth, while the block packs cold fourth and fire fifth. Corroborated by the
/// float enchantment-modifier key beside each call — `0x0D 0x0E 0x0F 0x11 0x10 0x12 0x13 0xA5`,
/// which is the **third** member of each row, and it is what colours the row.
pub const ARMOR_RESISTANCE_ROWS: [(u32, usize, u32); 8] = [
    (0x01, 0, 0x0D),  // Slashing  <- mod_vs_slash
    (0x02, 1, 0x0E),  // Piercing  <- mod_vs_pierce
    (0x04, 2, 0x0F),  // Bludgeoning <- mod_vs_bludgeon
    (0x10, 4, 0x11),  // Fire      <- mod_vs_fire
    (0x08, 3, 0x10),  // Cold      <- mod_vs_cold
    (0x20, 5, 0x12),  // Acid      <- mod_vs_acid
    (0x40, 7, 0x13),  // Electric  <- mod_vs_electric
    (0x400, 6, 0xA5), // Nether   <- mod_vs_nether
];

/// `MISSILE_RANGE_CAP` and the two roundings of the weapon-and-armour block's range block.
///
/// The range is `max_velocity^2 * (1/9.8)` metres, times `1.094` for yards, capped at `85.0`
/// (`MISSILE_RANGE_CAP`); at `10.0` or more it truncates and drops to a multiple of 5, below that
/// it rounds up.
#[must_use]
// Both casts truncate (the second after `ceil`); truncation is the client's own rounding.
#[allow(clippy::cast_possible_truncation)]
pub fn missile_range(max_velocity: f64) -> i32 {
    let v = (max_velocity * max_velocity * 0.102_040_816_326_530_6 * 1.094).min(85.0);
    if v >= 10.0 {
        // LINT-OK: the client's own truncation, not a rounding choice.
        let t = v as i32;
        t - t % 5
    } else {
        // LINT-OK: `ceil` then truncation, exactly as the client rounds it.
        v.ceil() as i32
    }
}

/// `printf`'s `%.<n>g`, for the damage range's `"%s%.3g - %d%s"` / `"%s%.4g - %d%s"` pair.
///
/// `n` significant digits, trailing zeros and a trailing point removed. The values it is asked
/// about are small positive damages, so the exponent branch is unreachable in practice and is
/// written as the plain fixed form.
#[must_use]
// `log10().floor()` of a printable magnitude is a small integer.
#[allow(clippy::cast_possible_truncation)]
pub fn format_g(v: f64, sig: i32) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    // LINT-OK: `log10().floor()` of a printable damage magnitude is a small integer.
    let exp = math::log10(v.abs()).floor() as i32;
    let decimals = usize::try_from((sig - 1 - exp).max(0)).unwrap_or(0);
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// `L"<Inscribe here>"` — the wide literal pushed by the inscription write
/// and again by the inscription losing-focus handling.
pub const PLACEHOLDER: &str = "<Inscribe here>";

/// What the inscription edit box shows.
///
/// **It is a four-way fork, not a three-way one.** The two `""` tests do *not*
/// lead to the same place: only the scribe's pair reaches the invitation, and the inscription's
/// pair reaches a leg that writes **nothing at all**, leaving the box as it was cleared at the top
/// of the function.
///
/// 1. The box is cleared first, on every leg.
/// 2. Not inscribable: the box is hidden, and that is all.
/// 3. No scribe name (string `8`), or an empty one: the box shows `<Inscribe here>`.
/// 4. No inscription (string `7`), or an empty one: **no text is set** — a blank box.
/// 5. Otherwise: state 1, the inscription, and `--` + scribe on the signature line.
///
/// Legs 3 to 5 all end by showing the box.
///
/// So **an item somebody has signed with no text shows an empty box, not the invitation** — and
/// because the string read stores the scribe name itself (see
/// `ExaminationPanel::set_inscription`), the panel is still holding that scribe when the
/// editable-state write asks whose it is, so the player sees no inscription and still cannot
/// edit it.
///
/// `None` is the not-inscribable leg, where the box is not shown at all; `Some("")` is the blank
/// one.
#[must_use]
pub fn inscription_text(
    inscribable: bool,
    scribe_name: Option<&str>,
    inscription: Option<&str>,
) -> Option<String> {
    if !inscribable {
        return None;
    }
    // These are the only two routes to the invitation.
    let scribe = scribe_name.unwrap_or("");
    if scribe.is_empty() {
        return Some(PLACEHOLDER.to_string());
    }
    // The non-placeholder route only makes the box visible when the inscription is blank.
    let text = inscription.unwrap_or("");
    if text.is_empty() {
        return Some(String::new());
    }
    Some(text.to_string())
}

/// The signature line under the inscription is `"--"` followed by the scribe's name. It is
/// written only for an actual inscription, so it is empty both when the box shows
/// `<Inscribe here>` and when it shows a blank inscription: the
/// clear at the top is what stands on the other three.
#[must_use]
pub fn inscription_signature(
    inscribable: bool,
    scribe_name: Option<&str>,
    inscription: Option<&str>,
) -> String {
    if !inscribable {
        return String::new();
    }
    let scribe = scribe_name.unwrap_or("");
    if scribe.is_empty() || inscription.unwrap_or("").is_empty() {
        return String::new();
    }
    format!("--{scribe}")
}

/// The item-examine panel's weapon-and-armour data block, as the lines it appends and the
/// **colour index** each one carries.
///
/// Every line is one same-line add-item-info call with its colour, so the block is a run of
/// single newlines, and the second argument is the index into the pane's `0x1B` colour
/// array that [`mod_color`] computes (not a *font* index: the append passes a literal `0` for
/// the font). The arms, in the function's own order:
///
/// 1. **not a weapon slot at all** (`!(loc & 0x3F00000)`) — the clothing-priority fallback, which
///    this build does not implement;
/// 2. the shield block (`loc & 0x200000`), likewise not implemented;
/// 3. `Skill: ` + the skill name of `weapon_skill` + the int `0x161` family;
/// 4. `Damage: ` or **`Damage Bonus: `** — the fork is
///    `(loc & 0x400000) && ammo_type != NONE`, i.e. *this is a launcher with an ammunition
///    type*. A launcher also suppresses the `", <damage type>"` suffix;
/// 5. `Damage Modifier: ` — launcher only, [`modifier_to_string`] of `damage_mod`, or `Unknown` on
///    a failed assess;
/// 6. `Speed: ` and, for a launcher, `Range: ` — `loc & 0x2500000`;
/// 7. `Bonus to Attack Skill: ` — when `weapon_offense != 1.0` and this is *not* a launcher;
/// 8. the ammunition sentence.
#[must_use]
// The one cast is the `Bonus to Attack Skill` truncation; see [`modifier_to_string`].
#[allow(clippy::cast_possible_truncation)]
pub fn weapon_and_armor_lines(p: &AppraisalView) -> Vec<(String, u8)> {
    let mut out = Vec::new();
    let loc = p.valid_locations;
    if loc & equip::ANY_WEAPON == 0 {
        // Arm 1: the clothing-priority name — see `ITEM_BLOCKS_NOT_IMPLEMENTED`'s note.
        return out;
    }
    let Some(w) = p.weapon else {
        push_ammunition_line(&mut out, loc, p.ammo_type);
        return out;
    };
    // "Is a launcher that takes ammunition".
    let launcher = loc & equip::MISSILE_WEAPON != 0 && p.ammo_type != ammo_type::NONE;

    // 3. The skill line. An unknown skill name skips the whole line.
    // Colour 0, same line: plain, whatever the weapon skill is enchanted by.
    if let Some(skill) = skill_to_string(w.weapon_skill) {
        let suffix = p.weapon_type.and_then(weapon_type_suffix).unwrap_or("");
        out.push((format!("Skill: {skill}{suffix}"), 0));
    }

    // 4. The damage line. **Two** enchantment questions, `0x2C` then `0x16` — see
    // [`mod_color`] — and the `Unknown` arm uses colour 0 outright.
    let label = if launcher {
        "Damage Bonus: "
    } else {
        "Damage: "
    };
    if w.weapon_damage < 0 {
        out.push((format!("{label}Unknown"), 0));
    } else {
        let suffix = if launcher {
            String::new()
        } else if w.damage_type != 0 {
            format!(", {}", damage_type_to_string(w.damage_type))
        } else {
            ", unknown type".to_string()
        };
        let damage = f64::from(w.weapon_damage);
        let low = (1.0 - w.damage_variance) * damage;
        let color = mod_color(p, &[0x2C, 0x16]);
        if damage - low > 0.000_199_999_994_947_575_03 {
            let sig = if low >= 10.0 { 4 } else { 3 };
            out.push((
                format!(
                    "{label}{} - {}{suffix}",
                    format_g(low, sig),
                    w.weapon_damage
                ),
                color,
            ));
        } else {
            out.push((format!("{label}{}{suffix}", w.weapon_damage), color));
        }
    }

    // 5. The damage modifier, launcher only. `0x3F`, and the failed-assess arm sets the colour to
    //    a literal `0` before it picks its string.
    if launcher {
        if p.success {
            out.push((
                format!("Damage Modifier: {}.", modifier_to_string(w.damage_mod)),
                mod_color(p, &[0x3F]),
            ));
        } else {
            out.push(("Damage Modifier: Unknown".to_string(), 0));
        }
    }

    // 6. Speed, and range for a launcher. Both `Unknown` literals have **two** spaces, unlike
    // the known branches. `Speed` takes `0x31`; both `Unknown` arms and the whole `Range` line
    // use colour 0.
    if loc & equip::SPEED_SHOWN != 0 {
        if w.weapon_time < 0 {
            out.push(("Speed:  Unknown".to_string(), 0));
            if loc & equip::MISSILE_WEAPON != 0 {
                out.push(("Range:  Unknown".to_string(), 0));
            }
        } else {
            out.push((
                format!(
                    "Speed: {} ({})",
                    weapon_time_to_string(w.weapon_time),
                    w.weapon_time
                ),
                mod_color(p, &[0x31]),
            ));
            if loc & equip::MISSILE_WEAPON != 0 {
                let based = if w.max_velocity_estimated != 0 {
                    " (based on STRENGTH 100)"
                } else {
                    ""
                };
                out.push((
                    format!("Range: {} yds.{based}", missile_range(w.max_velocity)),
                    0,
                ));
            }
        }
    }

    // 7. The attack-skill bonus, non-launchers only. `0x3E`.
    if (w.weapon_offense - 1.0).abs() > f64::EPSILON && !launcher {
        let sign = if w.weapon_offense < 1.0 { '-' } else { '+' };
        let magnitude = ((1.0 - w.weapon_offense).abs() * 100.0 + 0.5) as i64;
        out.push((
            format!("Bonus to Attack Skill: {sign}{magnitude}%."),
            mod_color(p, &[0x3E]),
        ));
    }

    // 8. The ammunition sentence.
    push_ammunition_line(&mut out, loc, p.ammo_type);
    out
}

/// The ammunition arm produces six sentences over two slots and three ammunition types.
fn push_ammunition_line(out: &mut Vec<(String, u8)>, loc: u32, ammo: u16) {
    let line = if loc & equip::MISSILE_WEAPON != 0 {
        match ammo {
            ammo_type::ARROW => "Uses arrows as ammunition.",
            ammo_type::BOLT => "Uses quarrels as ammunition.",
            ammo_type::ATLATL => "Uses atlatl darts as ammunition.",
            _ => return,
        }
    } else if loc & equip::AMMUNITION != 0 {
        match ammo {
            ammo_type::ARROW => "Used as ammunition by bows.",
            ammo_type::BOLT => "Used as ammunition by crossbows.",
            ammo_type::ATLATL => "Used as ammunition by atlatls.",
            _ => return,
        }
    } else {
        return;
    };
    // Colour 0, same line — never highlighted.
    out.push((line.to_string(), 0));
}

/// The item-examine panel's armour-mods block, as its lines and their colour indices.
///
/// **This is the block the enchantment highlighting is mostly about** — nine lines, each with its
/// own enchantment-modifier key.
///
/// The whole block is gated on an armour profile **and** int `0x1C` `> 0`: an armour profile with
/// no positive armour level draws nothing at all, not even the resistances. The first line's
/// literal is `"\nArmor Level: "` — with a **leading newline inside the string**, which the
/// add-item-info separator then adds to, so the block starts on a blank line.
#[must_use]
pub fn armor_mod_lines(p: &AppraisalView) -> Vec<(String, u8)> {
    let mut out = Vec::new();
    let (Some(mods), Some(level)) = (p.armor_mods, p.armor_level) else {
        return out;
    };
    if level <= 0 {
        return out;
    }
    // Int modifier `0x1C`, the armour level's own key.
    out.push((format!("\nArmor Level: {level}"), mod_color(p, &[0x1C])));
    for (damage_type, slot, key) in ARMOR_RESISTANCE_ROWS {
        if let Some(line) = resistance_line(damage_type, level, mods[slot]) {
            out.push((line, mod_color(p, &[key])));
        }
    }
    out
}

/// The nine adjectives, and the
/// one input that draws no line at all.
///
/// The test is a plain chain. A negative percent answers nothing, and the caller then draws no
/// line (it formats only when this answers). `0` is *impossible*; below 5 *ridiculously
/// difficult*; below 15 *extremely difficult*; below 35 *quite difficult*; below 50 *difficult*;
/// below 70 *challenging*; below 85 *mildly challenging*; below 95 *easy*; otherwise *trivial*.
///
/// Every literal is the client's own ASCII string.
#[must_use]
pub fn lockpick_success_percent_to_string(percent: i32) -> Option<&'static str> {
    if percent < 0 {
        return None;
    }
    Some(match percent {
        0 => "impossible",
        1..=4 => "ridiculously difficult",
        5..=14 => "extremely difficult",
        15..=34 => "quite difficult",
        35..=49 => "difficult",
        50..=69 => "challenging",
        70..=84 => "mildly challenging",
        85..=94 => "easy",
        _ => "trivial",
    })
}

/// The item-examine panel's lock-appraisal block — what identifying a locked chest says about
/// its lock.
///
/// The whole body: an object that is a hook draws nothing (see below). With the `Locked` bool
/// (`3`) present, `0` draws *Unlocked* and stops; otherwise *Locked*, then — with no
/// `ResistLockpick` (int `0x26`) — *You can't tell how hard the lock is to pick.* and stops; with
/// it, and with an `AppraisalLockpickSuccessPercent` (int `0xAD`) that names an adjective,
/// *The lock looks %s to pick (Resistance %d).* With no `Locked` bool, an int `0x26` below zero
/// draws *Bonus to Lockpick Skill: %d* and one above zero *Bonus to Lockpick Skill: +%d*.
///
/// Four things in it are easy to get wrong and each is pinned here:
///
/// * **A missing bool `3` is not the same as `Locked = 0`.** A profile with no bool table takes
///   the `else` and can still print a lockpick *bonus*; a profile that says
///   `Locked = 0` prints `"Unlocked"` and returns, bonus or no bonus. The corpus has both shapes
///   on the same object.
/// * **`ResistLockpick` supplies `%d`; the percent determines the adjective for `%s`.**
///   Int `0x26` is the third `sprintf` argument and the adjective string is the second.
///   Int `0xAD` never reaches the format directly.
/// * **`bonus == 0` draws nothing**: the separate negative and positive branches leave zero
///   with no line, so the `+` sign is not a formatting choice but a third case.
/// * **Every line is added with colour 0 and `same_line` 0** — a nonzero `same_line` chooses
///   one newline, zero two. Unlike Value and Burden this starts a **paragraph**.
#[must_use]
pub fn lock_appraise_lines(p: &AppraisalView) -> Vec<String> {
    let mut out = Vec::new();
    // The client asks whether the object is a hook, not a creature; see
    // `AppraisalView::weenie_is_hook`. The two agree for
    // everything the corpus carries (no recorded object is either), so no asserted line moves —
    // but the lock block is now skipped for a *hook*, which is what retail does, instead of for a
    // creature, which it does not.
    if p.weenie_is_hook {
        return out;
    }
    match p.locked {
        Some(true) => {
            out.push("Locked".to_string());
            let Some(resist) = p.resist_lockpick else {
                out.push("You can't tell how hard the lock is to pick.".to_string());
                return out;
            };
            if let Some(percent) = p.lockpick_success_percent {
                if let Some(word) = lockpick_success_percent_to_string(percent) {
                    out.push(format!(
                        "The lock looks {word} to pick (Resistance {resist})."
                    ));
                }
            }
        }
        Some(false) => out.push("Unlocked".to_string()),
        None => match p.resist_lockpick {
            Some(bonus) if bonus < 0 => out.push(format!("Bonus to Lockpick Skill: {bonus}")),
            Some(bonus) if bonus > 0 => out.push(format!("Bonus to Lockpick Skill: +{bonus}")),
            _ => {}
        },
    }
    out
}

/// Three lines, and only one string: *"Attuned"* for `1` and `2`, and nothing otherwise.
///
/// So the attuned status (1) **and** the sticky status (2) both read *"Attuned"*, and
/// `Normal` (0) and anything negative draw no entry at all. The two enumerators stay distinct on
/// the wire and are deliberately not distinguished here, because the client does not.
#[must_use]
pub fn attuned_status_to_string(v: i32) -> Option<&'static str> {
    (0 < v && v < 3).then_some("Attuned")
}

/// The appraisal system's bonded status to string.
///
/// The bonded status is `Normal 0, Bonded 1, Sticky 2, Destroy 0xFFFFFFFE, Slippery 0xFFFFFFFF`,
/// and the function's three arms are `Destroy`, `Slippery` and `Bonded` in that order — `Normal`
/// and `Sticky` fall through and draw nothing.
#[must_use]
pub fn bonded_status_to_string(v: i32) -> Option<&'static str> {
    match v {
        -2 => Some("Destroyed on Death"),
        -1 => Some("Dropped on Death"),
        1 => Some("Bonded"),
        _ => None,
    }
}

/// The fifteen bits of the or-ed `ImbuedEffect` mask, in the order
/// the special-properties block tests them — which is **not** ascending: `Nether Rending`
/// (`0x4000`) is drawn between `Acid Rending` (`0x40`) and `Cold Rending` (`0x80`). That is the
/// client's own order.
pub const IMBUE_NAMES: &[(u32, &str)] = &[
    (0x0000_0001, "Critical Strike"),
    (0x0000_0002, "Crippling Blow"),
    (0x0000_0004, "Armor Rending"),
    (0x0000_0008, "Slash Rending"),
    (0x0000_0010, "Pierce Rending"),
    (0x0000_0020, "Bludgeon Rending"),
    (0x0000_0040, "Acid Rending"),
    (0x0000_4000, "Nether Rending"),
    (0x0000_0080, "Cold Rending"),
    (0x0000_0100, "Lightning Rending"),
    (0x0000_0200, "Fire Rending"),
    (0x0000_0400, "+1 Melee Defense"),
    (0x0000_0800, "+1 Missile Defense"),
    (0x0000_1000, "+1 Magic Defense"),
    (0x8000_0000, "Phantasmal"),
];

/// Append `text` to the list, with `", "` before it unless the list is still empty.
///
/// So the first entry gets no separator and every later one gets `", "`.
fn append_helper(list: &mut String, text: &str) {
    if !list.is_empty() {
        list.push_str(", ");
    }
    list.push_str(text);
}

/// The **`Properties:` line** — what tells an attuned item from any other.
///
/// The block is a list built by `append_helper` and drawn as one line, with three
/// stand-alone sentences around it. Returned as `(text, same_line)` pairs in the client's own
/// order.
///
/// 1. A blank line, unconditionally.
/// 2. With int `0x117`: a blank, then *You can only carry N of these items.* (N with commas).
/// 3. The cooldown pair (`0xA7` / `0x118`), including the group's trailing blank.
/// 4. With int `0x124` above 1: *Cleave: N enemies in front arc.* and a blank.
/// 5. The list, as *Properties: …* when it is not empty, and *This item cannot be further
///    imbued.* for an imbued item; either one counts as having drawn.
/// 6. With bool `0x82` true, *This item is tethered to the left side.*; otherwise, when nothing
///    was drawn, stop.
/// 7. A trailing blank.
///
/// Two things in it are easy to get wrong and both are pinned:
///
/// * **the leading blank line is unconditional**, before the first property read. Every
///   appraised item gets it, which is
///   why adding this block moves every other item pane's text by one newline.
/// * **the trailing blank is not**: the client skips it for an item with
///   nothing to say, and `drew` is set by the `Properties:` line and by the imbue sentence but
///   **not** by the tether one.
///
/// `same_line` is `1` for every line in this function, so nothing here starts a
/// paragraph.
#[must_use]
pub fn special_properties_lines(p: &AppraisalView) -> Vec<(String, bool)> {
    let s = &p.special;
    let mut out: Vec<(String, bool)> = vec![(String::new(), true)];

    // The comma insertion cannot fail on a string this function itself built out of an
    // `int32`, so the `"???"` is unreachable for any profile and is not drawn here.
    if let Some(n) = s.unique_limit {
        out.push((String::new(), true));
        out.push((
            format!("You can only carry {} of these items.", insert_commas(n)),
            true,
        ));
    }
    // The client skips the entire pair when the duration is absent. Remaining is nested under
    // group presence, and the group's blank is appended even when no active cooldown was found.
    if let Some(duration) = s.cooldown_duration {
        out.push((
            format!("Cooldown When Used: {}", appraisal_cooldown_text(duration)),
            true,
        ));
        if s.cooldown_group.is_some() {
            if let Some(remaining) = s.cooldown_remaining {
                out.push((
                    format!("Cooldown Remaining: {}", appraisal_cooldown_text(remaining)),
                    true,
                ));
            }
            out.push((String::new(), true));
        }
    }
    // The guard is `n > 1`: one enemy is not a cleave.
    if let Some(n) = s.cleave.filter(|n| *n > 1) {
        out.push((format!("Cleave: {n} enemies in front arc."), true));
        out.push((String::new(), true));
    }

    let mut list = String::new();
    // Append the slayer name and the client's `" slayer"` suffix. `0x1F` is the one creature
    // type with a name of its own.
    if let Some((id, name)) = s.slayer.as_ref() {
        if *id == 0x1F {
            append_helper(&mut list, "Bael'Zharon's Hate");
        } else {
            append_helper(&mut list, &format!("{name} slayer"));
        }
    }
    // Draw the weapon-skill property only when int `0x2F` is present with `v & 0x79E0` set.
    if s.weapon_skill.is_some_and(|v| v as u32 & 0x79E0 != 0) {
        append_helper(&mut list, "Multi-Strike");
    }
    // OR the five integers, then perform fifteen bit tests.
    if let Some(mask) = s.imbued {
        for (bit, name) in IMBUE_NAMES {
            if mask & bit != 0 {
                append_helper(&mut list, name);
            }
        }
    }
    if s.absorb_magic_damage {
        append_helper(&mut list, "Magic Absorbing");
    }
    // The guard is `> 0x270E`: **above** 9998, not at or above.
    if s.item_spellcraft.is_some_and(|v| v > 0x270E) {
        append_helper(&mut list, "Unenchantable");
    }
    if let Some(word) = s.attuned.and_then(attuned_status_to_string) {
        append_helper(&mut list, word);
    }
    if let Some(word) = s.bonded.and_then(bonded_status_to_string) {
        append_helper(&mut list, word);
    }
    if s.retained == Some(true) {
        append_helper(&mut list, "Retained");
    }
    if s.critical_multiplier {
        append_helper(&mut list, "Crushing Blow");
    }
    if s.critical_frequency {
        append_helper(&mut list, "Biting Strike");
    }
    if s.ignore_armor {
        append_helper(&mut list, "Armor Cleaving");
    }
    if let Some(dt) = s.resistance_cleaving {
        append_helper(
            &mut list,
            &format!("Resistance Cleaving: {}", damage_type_to_string(dt)),
        );
    }
    if s.proc_spell {
        append_helper(&mut list, "Cast on Strike");
    }
    if s.ivoryable == Some(true) {
        append_helper(&mut list, "Ivoryable");
    }
    if s.dyeable == Some(true) {
        append_helper(&mut list, "Dyeable");
    }

    let mut drew = false;
    if !list.is_empty() {
        out.push((format!("Properties: {list}"), true));
        drew = true;
    }
    if s.imbued.is_some_and(|m| m != 0) {
        out.push(("This item cannot be further imbued.".to_string(), true));
        drew = true;
    }
    if s.tethered_left == Some(true) {
        out.push(("This item is tethered to the left side.".to_string(), true));
    } else if !drew {
        return out;
    }
    out.push((String::new(), true));
    out
}

/// The special-properties block truncates the seconds to a signed 32-bit word and converts that
/// back to a float for the delta-time formatter, which truncates again and divides the word as
/// unsigned. Retain the low 32 bits before reusing the existing duration decomposition, including
/// negative words.
#[must_use]
pub fn appraisal_cooldown_text(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation)]
    // the low 32 bits, as the client's integer store keeps them
    let word = dereth_primitives::num::to_i64_f64(seconds) as u32;
    crate::panels::journal::delta_time_to_string(i64::from(word))
}

/// The **`Spells:` line** — what an enchanted item is carrying.
///
/// What the client does:
///
/// ```text
/// if the appraisal failed, append "Spells: unknown." as its own paragraph
/// otherwise start text with "Spells: "
/// for each spell id:
///     skip ids whose high bit is set
///     mask the id with 0x7FFFFFFF and look up its name
///     decide whether to append ", " before testing whether the name resolved
///     append the name only when lookup succeeds; an unknown name contributes no text
/// if no unmasked id was collected, suppress the paragraph
/// otherwise append the collected spell list as its own paragraph
/// ```
///
/// Three things in it are easy to get wrong and all three are pinned:
///
/// * **the ids with `0x80000000` set are not in this list.** the client skips them outright, so
///   an item whose only spells are enchantments gets no `Spells:` line at all — they appear in
///   [`magic_info_lines`]' *"Enchantments:"* paragraph instead;
/// * **the separator is decided before the name is tested.** The separator flag is set by
///   the first id that reaches the lookup, and the empty-name guard that drops an unresolved name
///   comes after it — so an id the table does not know still costs a `", "`;
/// * **it is a paragraph, not a line.** Both lines pass `0` for `same_line`, unlike
///   every line of the block before it.
#[must_use]
pub fn short_magic_info_lines(p: &AppraisalView) -> Vec<(String, bool)> {
    // A profile with no `0x0010` block has no short magic information.
    let Some(spells) = p.magic.spells.as_ref() else {
        return Vec::new();
    };
    // This is reached with an **empty** list too: the client tests the flag
    // before it looks at the array.
    if !p.success {
        return vec![("Spells: unknown.".to_string(), false)];
    }
    let mut list = String::new();
    let mut any = false;
    for s in spells.iter().filter(|s| !s.enchantment) {
        if any {
            list.push_str(", ");
        }
        any = true;
        list.push_str(&s.name);
    }
    if !any {
        return Vec::new();
    }
    vec![(format!("Spells: {list}"), false)]
}

/// The **`Spell Descriptions:`** and
/// **`Enchantments:`** paragraphs and the four lines above them.
///
/// This is the spells active on an identified item at full length; [`short_magic_info_lines`] is
/// its one-line summary, drawn twenty blocks earlier in the same appraise-info write.
///
/// The function builds **two** accumulators and sorts each spell into one of them by the top bit
/// of its id, then draws whichever were filled:
///
/// ```text
/// Initialize buffers with "Spell Descriptions:" and "Enchantments:\n".
/// For each spell, save its high bit and mask the id with 0x7FFFFFFF.
/// Resolve name and description; concatenate "\n~ " + name + ": " + description.
/// Append to Enchantments when the high bit is set, otherwise to Spell Descriptions.
/// If Spell Descriptions has entries:
///     int 0x6A -> "Spellcraft: %d."                             (0, 1)
///     int 0x6B && int 0x6C -> "Mana: %d / %d."                  (0, 1)
///     float 5 -> "Mana Cost: 1 point per %d seconds."           (0, 1)
///     else int 0x75 -> "Mana Cost: %d."[ + footnote]            (0, 0)
///     Append the Spell Descriptions buffer with arguments (0, 0).
/// If Enchantments has entries, append its buffer with arguments (0, 0).
/// ```
///
/// Points worth stating:
///
/// * **the four mana lines hang off the `Spell Descriptions` flag, not the spell book.** An item
///   carrying nothing but enchantments shows its `Enchantments:` paragraph and **no**
///   `Spellcraft:` / `Mana:` line, however present those properties are;
/// * **the five format strings**, exactly as the client has them:
///   `"Spellcraft: %d."`, `"Mana: %d / %d."`,
///   `"Mana Cost: 1 point per %d seconds."`, `"Mana Cost: %d."` and
///   `"Mana Cost: %d.\n(Can be reduced by the Mana Conversion skill)"`;
/// * **the two `Mana Cost` arms disagree about the separator.** The rate arm passes `same_line =
///   1` and the flat one passes `0` — one line, one paragraph, for what
///   reads as the same sentence;
/// * **a failed assess says `Spells: unknown.` here as well**, so a profile with a spell block
///   and `success_flag == 0` shows that sentence **twice**. This matches the client;
/// * **the two headers disagree about their own newline.** `"Enchantments:\n"` includes the
///   newline, while `"Spell Descriptions:"` does not. Since
///   every entry appended to either begins with `"\n~ "`, the enchantment list gets a
///   blank line under its header and the description list does not. Kept, because it is visible
///   in the pane and "tidying" it would be an approximation.
#[must_use]
pub fn magic_info_lines(p: &AppraisalView) -> Vec<(String, bool)> {
    let m = &p.magic;
    // Use the same no-spell-book gate as the short block.
    let Some(spells) = m.spells.as_ref() else {
        return Vec::new();
    };
    // A failed assessment reports an unknown spell list.
    if !p.success {
        return vec![("Spells: unknown.".to_string(), false)];
    }

    let mut descriptions = "Spell Descriptions:".to_string();
    let mut enchantments = "Enchantments:\n".to_string();
    let (mut any_description, mut any_enchantment) = (false, false);
    for s in spells {
        // Emit one string per spell, whichever bucket it lands in.
        let entry = format!("\n~ {}: {}", s.name, s.description);
        if s.enchantment {
            enchantments.push_str(&entry);
            any_enchantment = true;
        } else {
            descriptions.push_str(&entry);
            any_description = true;
        }
    }

    let mut out: Vec<(String, bool)> = Vec::new();
    if any_description {
        if let Some(n) = m.spellcraft {
            out.push((format!("Spellcraft: {n}."), true));
        }
        // One `&&` requires both keys, or no line is emitted.
        if let (Some(cur), Some(max)) = (m.cur_mana, m.max_mana) {
            out.push((format!("Mana: {cur} / {max}."), true));
        }
        if let Some(rate) = m.mana_rate {
            // The client: `|1.0 / rate| + 0.5`, truncated.
            #[allow(clippy::cast_possible_truncation)] // a whole number of seconds
            let seconds = ((1.0f64 / rate).abs() + 0.5) as i64;
            out.push((format!("Mana Cost: 1 point per {seconds} seconds."), true));
        } else if let Some(cost) = m.mana_cost {
            // A cost of zero or less gets the bare sentence.
            let text = if cost > 0 {
                format!("Mana Cost: {cost}.\n(Can be reduced by the Mana Conversion skill)")
            } else {
                format!("Mana Cost: {cost}.")
            };
            out.push((text, false));
        }
        out.push((descriptions, false));
    }
    if any_enchantment {
        out.push((enchantments, false));
    }
    out
}

// =================================================================================================
// The remaining appraisal blocks
// =================================================================================================
//
// Every one of these is an appraisal block in the client, every literal below is the string that
// block uses, and every property key is the one the block queries. Three things that are easy to
// get wrong, each noted again at its block:
//
// * **The tested predicate is *is a hook*, not *is a creature*** -- see
//   `AppraisalView::weenie_is_hook`. Five blocks branch on it.
// * **The mana stone block scales both of its percentages by 100**; without that it would
//   print `Efficiency: 0%` for every mana stone.
// * **The timer sentence's two `%d` values are both the literal `3`**, not a property.

/// The appraisal system's workmanship adjective read -- the workmanship strings, eleven narrow
/// strings, plus a suffix for the low five.
///
/// The index is `min(v, 10)`; below 5, `"cut"` or `"crafted"` is appended.
///
/// The tinkering and description blocks both ask for the crafted form, so the suffix is
/// always `crafted`. Note string 0 is
/// the **empty** string and elements 1..4 end in a space or a hyphen, which is what makes
/// `"Poorly "` + `"crafted"` read as one word pair and `0` read as bare `"crafted"`.
#[must_use]
pub fn workmanship_adjective(v: i32) -> String {
    // The bound check is unsigned, so negative property words select index 10 too.
    let idx = (v as u32).min(10);
    let base = match idx {
        1 => "Poorly ",
        2 => "Well-",
        3 => "Finely ",
        4 => "Exquisitely ",
        5 => "Magnificent",
        6 => "Nearly flawless",
        7 => "Flawless",
        8 => "Utterly flawless",
        9 => "Incomparable",
        10 => "Priceless",
        _ => "",
    };
    if idx < 5 {
        format!("{base}crafted")
    } else {
        base.to_string()
    }
}

/// The tinkering block -- four lines and an unconditional blank.
///
/// | line | key |
/// |---|---|
/// | *"This item has been tinkered %d time%s."* | int `0xAB` |
/// | *"Last tinkered by %s."* | string `0x27` |
/// | *"Imbued by %s."* | string `0x28` |
/// | *"Workmanship: %s (%d)"* | int `0x69` alone |
/// | the salvage form | `0x69` **and** int `0xAA` |
///
/// The plural needs a count above 1, so exactly one tinker reads *"1 time."*.
///
/// **The trailing blank is unconditional** -- it is reached both by falling out of
/// the workmanship arm and by the branch that skips it, so *every* item pane gets one blank
/// paragraph here whether or not anything above drew.
///
/// The salvage arm's two numbers come from one division: int `0x69` over int `0xAA` gives the
/// `%.2f` value, and the same quotient `+ 0.5`, truncated, is the adjective's index.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn tinkering_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if let Some(n) = p.num_times_tinkered {
        let s = if n > 1 { "s" } else { "" };
        push_item_info(
            &mut out,
            format!("This item has been tinkered {n} time{s}."),
            true,
            0,
        );
    }
    if let Some(name) = p.tinker_name.as_deref() {
        push_item_info(&mut out, format!("Last tinkered by {name}."), true, 0);
    }
    if let Some(name) = p.imbuer_name.as_deref() {
        push_item_info(&mut out, format!("Imbued by {name}."), true, 0);
    }
    if let Some(w) = p.workmanship {
        let text = if let Some(items) = p.num_items_in_material {
            let q = (w as f32) / (items as f32);
            let adj = workmanship_adjective(dereth_primitives::num::to_i32_f64(f64::from(q) + 0.5));
            format!("Workmanship: {adj} ({q:.2})\n\nSalvaged from {items} items.")
        } else {
            format!("Workmanship: {} ({w})", workmanship_adjective(w))
        };
        push_item_info(&mut out, text, true, 0);
    }
    // The client: reached from both arms.
    push_item_info(&mut out, String::new(), true, 0);
    out
}

/// The set name -- int `0x109` `EquipmentSetId` through a 119-arm switch.
///
/// Every name is the client's literal for the matching `case`; the ids with no arm (`0x22`,
/// `0x2A`..`0x30`, `0x58`) fall to `default`, which zeroes the id and draws nothing. Five ids
/// share *"Weave of Light Weapons"* and three share *"Weave of Missile Weapons"*, which is the
/// switch's own arm sharing and not a transcription slip. `52 CloakAssessPerson` and
/// `83 CloakAssessCreature` are the pair most easily transposed.
#[must_use]
pub fn equipment_set_name(id: i32) -> Option<&'static str> {
    Some(match id {
        0x04 => "Carraida's Benediction",
        0x05 => "Noble Relic",
        0x06 => "Ancient Relic",
        0x07 => "Alduressa Relic",
        0x08 => "Shou-jen",
        0x09 => "Empyrean Rings",
        0x0A => "Arm, Mind, Heart",
        0x0B => "Coat of Perfect Light",
        0x0C => "Leggings of Perfect Light",
        0x0D => "Soldier's",
        0x0E => "Adept's",
        0x0F => "Archer's",
        0x10 => "Defender's",
        0x11 => "Tinker's",
        0x12 => "Crafter's",
        0x13 => "Hearty",
        0x14 => "Dexterous",
        0x15 => "Wise",
        0x16 => "Swift",
        0x17 => "Hardened",
        0x18 => "Reinforced",
        0x19 => "Interlocking",
        0x1A => "Flame Proof",
        0x1B => "Acid Proof",
        0x1C => "Cold Proof",
        0x1D => "Lightning Proof",
        0x1E => "Dedication",
        0x1F => "Gladiatorial Clothing",
        0x20 => "Ceremonial Clothing",
        0x21 => "Protective Clothing",
        0x23 => "Sigil of Defense",
        0x24 => "Sigil of Destruction",
        0x25 => "Sigil of Fury",
        0x26 => "Sigil of Growth",
        0x27 => "Sigil of Vigor",
        0x28 => "Heroic Protector",
        0x29 => "Heroic Destroyer",
        0x31 => "Weave of Alchemy",
        0x32 => "Weave of Arcane Lore",
        0x33 => "Weave of Armor Tinkering",
        0x34 => "Weave of Assess Person",
        0x35 => "Weave of Light Weapons",
        0x36 => "Weave of Missile Weapons",
        0x37 => "Weave of Cooking",
        0x38 => "Weave of Creature Enchantment",
        0x39 => "Weave of Missile Weapons",
        0x3A => "Weave of Finesse Weapons",
        0x3B => "Weave of Deception",
        0x3C => "Weave of Fletching",
        0x3D => "Weave of Healing",
        0x3E => "Weave of Item Enchantment",
        0x3F => "Weave of Item Tinkering",
        0x40 => "Weave of Leadership",
        0x41 => "Weave of Life Magic",
        0x42 => "Weave of Loyalty",
        0x43 => "Weave of Light Weapons",
        0x44 => "Weave of Magic Defense",
        0x45 => "Weave of Magic Item Tinkering",
        0x46 => "Weave of Mana Conversion",
        0x47 => "Weave of Melee Defense",
        0x48 => "Weave of Missile Defense",
        0x49 => "Weave of Salvaging",
        0x4A => "Weave of Light Weapons",
        0x4B => "Weave of Light Weapons",
        0x4C => "Weave of Heavy Weapons",
        0x4D => "Weave of Missile Weapons",
        0x4E => "Weave of Two Handed Combat",
        0x4F => "Weave of Light Weapons",
        0x50 => "Weave of Void Magic",
        0x51 => "Weave of War Magic",
        0x52 => "Weave of Weapon Tinkering",
        0x53 => "Weave of Assess Creature",
        0x54 => "Weave of Dirty Fighting",
        0x55 => "Weave of Dual Wield",
        0x56 => "Weave of Recklessness",
        0x57 => "Weave of Shield",
        0x58 => "Weave of Sneak Attack",
        0x59 => "Shou-jen Shozoku",
        0x5A => "Weave of Summoning",
        0x5B => "Shrouded Soul",
        0x5C => "Darkened Mind",
        0x5D => "Clouded Spirit",
        0x5E => "Minor Stinging Shrouded Soul",
        0x5F => "Minor Sparking Shrouded Soul",
        0x60 => "Minor Smoldering Shrouded Soul",
        0x61 => "Minor Shivering Shrouded Soul",
        0x62 => "Minor Stinging Darkened Mind",
        0x63 => "Minor Sparking Darkened Mind",
        0x64 => "Minor Smoldering Darkened Mind",
        0x65 => "Minor Shivering Darkened Mind",
        0x66 => "Minor Stinging Clouded Spirit",
        0x67 => "Minor Sparking Clouded Spirit",
        0x68 => "Minor Smoldering Clouded Spirit",
        0x69 => "Minor Shivering Clouded Spirit",
        0x6A => "Major Stinging Shrouded Soul",
        0x6B => "Major Sparking Shrouded Soul",
        0x6C => "Major Smoldering Shrouded Soul",
        0x6D => "Major Shivering Shrouded Soul",
        0x6E => "Major Stinging Darkened Mind",
        0x6F => "Major Sparking Darkened Mind",
        0x70 => "Major Smoldering Darkened Mind",
        0x71 => "Major Shivering Darkened Mind",
        0x72 => "Major Stinging Clouded Spirit",
        0x73 => "Major Sparking Clouded Spirit",
        0x74 => "Major Smoldering Clouded Spirit",
        0x75 => "Major Shivering Clouded Spirit",
        0x76 => "Blackfire Stinging Shrouded Soul",
        0x77 => "Blackfire Sparking Shrouded Soul",
        0x78 => "Blackfire Smoldering Shrouded Soul",
        0x79 => "Blackfire Shivering Shrouded Soul",
        0x7A => "Blackfire Stinging Darkened Mind",
        0x7B => "Blackfire Sparking Darkened Mind",
        0x7C => "Blackfire Smoldering Darkened Mind",
        0x7D => "Blackfire Shivering Darkened Mind",
        0x7E => "Blackfire Stinging Clouded Spirit",
        0x7F => "Blackfire Sparking Clouded Spirit",
        0x80 => "Blackfire Smoldering Clouded Spirit",
        0x81 => "Blackfire Shivering Clouded Spirit",
        0x82 => "Shimmering Shadows",
        _ => return None,
    })
}

/// The appraisal show set's one line.
///
/// *"Set: "*  prepended to the name, colour 0, same line. The block returns
/// non-zero when it drew, and the appraise-info write uses that with the ratings block's answer to
/// decide a shared blank line -- see [`item_description_runs`].
#[must_use]
pub fn set_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    // The id must be positive -- a non-positive id draws nothing even with an arm.
    if let Some(id) = p.equipment_set_id {
        if id > 0 {
            if let Some(name) = equipment_set_name(id) {
                push_item_info(&mut out, format!("Set: {name}"), true, 0);
            }
        }
    }
    out
}

/// The thirteen gear-rating terms, in drawn order.
///
/// It lives in [`dereth_client_contract::panels::examination`]: `dereth_client::hud` walks
/// the table to read the thirteen properties off the qualities.
pub use dereth_client_contract::panels::examination::GEAR_RATING_ROWS;

/// The appraisal show ratings.
///
/// *"Ratings: "* + the positive terms joined with *", "* + *"."*, then
/// *"This item adds %d Vitality."* for `0x17B`, then a blank if either drew. That blank is
/// the block's own, on top of the one
/// the appraise-info write adds when this block or the set block returned non-zero.
#[must_use]
pub fn ratings_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    let mut terms: Vec<String> = Vec::new();
    for (i, (_, label)) in GEAR_RATING_ROWS.iter().enumerate() {
        if let Some(v) = p.gear_ratings[i] {
            if v > 0 {
                terms.push(format!("{label} {v}"));
            }
        }
    }
    let vitality = p.gear_ratings[GEAR_RATING_ROWS.len()].filter(|v| *v > 0);
    if !terms.is_empty() {
        push_item_info(&mut out, format!("Ratings: {}.", terms.join(", ")), true, 0);
    }
    if let Some(v) = vitality {
        push_item_info(&mut out, format!("This item adds {v} Vitality."), true, 0);
    }
    if !terms.is_empty() || vitality.is_some() {
        push_item_info(&mut out, String::new(), true, 0);
    }
    out
}

/// The defense-mod block -- three float lines.
///
/// | key | literal | colour |
/// |---|---|---|
/// | `0x1D` `WeaponDefense` | *"Bonus to Melee Defense: %s."* | the float modifier `0x1D` |
/// | `0x95` `WeaponMissileDefense` | *"Bonus to Missile Defense: %s."* | always `0` |
/// | `0x96` `WeaponMagicDefense` | *"Bonus to Magic Defense: %s."* | always `0` |
///
/// **Only the first asks about enchantment.** The other two pass a literal `0` as the
/// colour, which is the client's own asymmetry and not an omission here.
///
/// Every line is guarded by `present && value != 1.0`, so an unmodified weapon draws none.
#[must_use]
pub fn defense_mod_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    const LABELS: [&str; 3] = [
        "Bonus to Melee Defense: ",
        "Bonus to Missile Defense: ",
        "Bonus to Magic Defense: ",
    ];
    let mut out = Vec::new();
    for (i, label) in LABELS.iter().enumerate() {
        let Some(v) = p.defense_mods[i] else { continue };
        if (v - 1.0).abs() < f64::EPSILON {
            continue;
        }
        let color = if i == 0 { mod_color(p, &[0x1D]) } else { 0 };
        push_item_info(
            &mut out,
            format!("{label}{}.", small_modifier_to_string(v)),
            true,
            color,
        );
    }
    out
}

/// The elemental bonus against players, shared with the server (`dereth_rules::combat`).
pub use dereth_rules::combat::elemental_mod_pk_modifier;

/// The caster-data block -- the wand block.
///
/// ```text
/// float 0x90 -> "Bonus to Mana Conversion: %s."   modifier_to_string(v + 1.0)     same_line 0
/// float 0x98 && int 0x2D ->
///     "Damage bonus for %s spells:"   damage_type_to_string(int 0x2D)              same_line 0
///     " vs. Monsters: %s."            small_modifier_to_string(v)                  same_line 1
///     " vs. Players: %s."             small_modifier_to_string(pk(v))              same_line 1
/// ```
///
/// The first line adds `1.0`; the other three take the value raw. **Which of the two numbers goes
/// on which line matters:** the *Monsters* line reads the raw float `0x98` value, while the
/// *Players* one reads the [`elemental_mod_pk_modifier`] result (`pk` above).
///
/// All three elemental lines take the **same** colour, from the float modifier `0x98`.
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub fn caster_data_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if let Some(m) = p.mana_conversion_mod {
        push_item_info(
            &mut out,
            format!("Bonus to Mana Conversion: {}.", modifier_to_string(m + 1.0)),
            false,
            mod_color(p, &[0x90]),
        );
    }
    if let (Some(m), Some(dt)) = (p.elemental_damage_mod, p.caster_damage_type) {
        let color = mod_color(p, &[0x98]);
        let damage = damage_type_to_string(dt as u32);
        push_item_info(
            &mut out,
            format!("Damage bonus for {damage} spells:"),
            false,
            color,
        );
        push_item_info(
            &mut out,
            format!(" vs. Monsters: {}.", small_modifier_to_string(m)),
            true,
            color,
        );
        push_item_info(
            &mut out,
            format!(
                " vs. Players: {}.",
                small_modifier_to_string(elemental_mod_pk_modifier(m))
            ),
            true,
            color,
        );
    }
    out
}

/// The level-limit block -- the level band and the portal destination.
///
/// Both int reads start at `-1`, so "absent" and "zero" take the same arm. The destination is
/// *"Destination: "*  with string `0x26` appended. Both lines pass
/// `same_line = 0`, so each starts a paragraph.
#[must_use]
pub fn level_limit_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    let min = p.level_limits.0.unwrap_or(-1);
    let max = p.level_limits.1.unwrap_or(-1);
    let text = if min < 1 {
        if max < 1 {
            None
        } else {
            Some(format!("Restricted to characters of Level {max} or below."))
        }
    } else if max < 1 {
        Some(format!(
            "Restricted to characters of Level {min} or greater."
        ))
    } else if min == max {
        Some(format!("Restricted to characters of Level {min}."))
    } else {
        Some(format!(
            "Restricted to characters of Levels {min} to {max}."
        ))
    };
    if let Some(t) = text {
        push_item_info(&mut out, t, false, 0);
    }
    if let Some(d) = p.portal_destination.as_deref() {
        push_item_info(&mut out, format!("Destination: {d}"), false, 0);
    }
    out
}

/// The appraisal show wield requirements.
///
/// ```text
/// bool 0x55 == 1   -> "Wield requires " + string 0x19, else "the original owner"
/// int 0x1A != 0     -> "" , or "Use requires Throne of Destiny." when it is 1
/// int 0x144         -> "Wield requires " + the heritage group's display name
/// three triples      -> the switch below
/// ```
///
/// The switch is for the subject
/// plus five format arms:
///
/// | requirement | literal | reads |
/// |---|---|---|
/// | `1..=7`, `9`, `10` | *"Wield requires %s %d"* | subject, difficulty |
/// | `8` | *"Wield requires %s %s"* | *"trained"* unless difficulty is 3, then *"specialized"* |
/// | `11` | *"Wield requires %s type"* | subject |
/// | `12` | *"Wield requires %s race"* | subject |
/// | anything else | -- | **returns from the whole block**, so the later triples are dropped too |
#[must_use]
pub fn wield_requirement_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.has_allowed_wielder {
        let who = p.craftsman_name.as_deref().unwrap_or("the original owner");
        push_item_info(&mut out, format!("Wield requires {who}"), true, 0);
    }
    if let Some(v) = p.account_requirements {
        if v != 0 {
            let t = if v == 1 {
                "Use requires Throne of Destiny."
            } else {
                ""
            };
            push_item_info(&mut out, t.to_string(), true, 0);
        }
    }
    if let Some(h) = p.heritage_specific_armor.as_deref() {
        push_item_info(&mut out, format!("Wield requires {h}"), true, 0);
    }
    for r in &p.wield_requirements {
        let subject = r.subject.clone().unwrap_or_default();
        let text = match r.requirement {
            1..=7 | 9 | 10 => format!("Wield requires {subject} {}", r.difficulty),
            8 => {
                let word = if r.difficulty == 3 {
                    "specialized"
                } else {
                    "trained"
                };
                format!("Wield requires {word} {subject}")
            }
            11 => format!("Wield requires {subject} type"),
            12 => format!("Wield requires {subject} race"),
            // The switch default returns from the function, not merely from this arm.
            _ => break,
        };
        push_item_info(&mut out, text, true, 0);
    }
    out
}

/// The appraisal show usage limit info.
///
/// Three lines, each preceded by a blank **only the first time**:
///
/// | key | literal |
/// |---|---|
/// | int `0x171` above 0 | *"Use requires level %d."* |
/// | int `0x16E` non-zero **and** int `0x16F` non-zero | *"Use requires %s of at least %d."* |
/// | int `0x170` non-zero | *"Use requires specialized %s."* |
///
/// A skill id the `SkillTable` has no row for becomes *"Unknown Skill"*  and the line
/// still draws -- the skill-name read's false answer still sets the text; it is not a skip.
#[must_use]
pub fn usage_limit_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    let mut drew = false;
    fn blank(out: &mut Vec<ItemInfo>, drew: &mut bool) {
        if !*drew {
            push_item_info(out, String::new(), true, 0);
            *drew = true;
        }
    }
    if let Some(l) = p.use_requires_level {
        if l > 0 {
            blank(&mut out, &mut drew);
            push_item_info(&mut out, format!("Use requires level {l}."), true, 0);
        }
    }
    if let Some((name, level)) = p.use_requires_skill.as_ref() {
        let name = name.as_deref().unwrap_or("Unknown Skill");
        blank(&mut out, &mut drew);
        push_item_info(
            &mut out,
            format!("Use requires {name} of at least {level}."),
            true,
            0,
        );
    }
    if let Some(name) = p.use_requires_skill_spec.as_ref() {
        let name = name.as_deref().unwrap_or("Unknown Skill");
        blank(&mut out, &mut drew);
        push_item_info(
            &mut out,
            format!("Use requires specialized {name}."),
            true,
            0,
        );
    }
    out
}

/// XP to string -- `sprintf("%I64d")` then `GetNumberFormatA` with
/// `NumDigits 0`, `Grouping 3`, `lpThousandSep ","`.
///
/// That is [`insert_commas`] widened to 64 bits; the client's own is a
/// different function reached from the value line, and the two agree on everything an item's XP
/// can be.
#[must_use]
pub fn xp_to_string(v: u64) -> String {
    let digits = v.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The item-level block -- the aetheria/cloak level pair and the cloak
/// sentence.
///
/// The four-term guard is int64 `5` non-zero, int `0x13F` above 0 and int `0x140` above 0, with
/// int64 `4` read unguarded afterwards. Then:
///
/// ```text
/// level = advancement::item_total_xp_to_level(total, base, maxLevel, style)
/// next  = advancement::item_level_to_total_xp(min(level + 1, maxLevel), base, maxLevel, style)
/// "Item Level: %d / %d"   level, maxLevel                               same_line 1
/// "Item XP: %s / %s"      xp_to_string(total), xp_to_string(next)        same_line 1
/// ""                                                                      same_line 1
/// ```
///
/// The cloak sentence's `%d` is the literal `0xC8` = **200**, not a property.
#[must_use]
pub fn item_level_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if let Some(l) = p.item_level {
        if l.base_xp != 0 && l.max_level > 0 && l.xp_style > 0 {
            let level =
                advancement::item_total_xp_to_level(l.total_xp, l.base_xp, l.max_level, l.xp_style);
            // The client adds one in 32 bits, wrapping: a level of `i32::MAX` (reachable only with a
            // `max_level` of `i32::MAX`) becomes negative and its next-level total 0.
            let next = advancement::item_level_to_total_xp(
                level.wrapping_add(1).min(l.max_level),
                l.base_xp,
                l.max_level,
                l.xp_style,
            );
            push_item_info(
                &mut out,
                format!("Item Level: {level} / {}", l.max_level),
                true,
                0,
            );
            push_item_info(
                &mut out,
                format!(
                    "Item XP: {} / {}",
                    xp_to_string(l.total_xp),
                    xp_to_string(next)
                ),
                true,
                0,
            );
            push_item_info(&mut out, String::new(), true, 0);
        }
    }
    if p.cloak_weave_proc == Some(2) {
        push_item_info(
            &mut out,
            "This cloak has a chance to reduce an incoming attack by 200 damage.".to_string(),
            true,
            0,
        );
        push_item_info(&mut out, String::new(), true, 0);
    }
    out
}

/// The activation-requirements block -- one joined line, plus the owner sentence.
///
/// The whole block is behind a successful assess, which is the only one of the
/// twenty-five that is.
///
/// | term | keys |
/// |---|---|
/// | *"Arcane Lore: %d"* | int `0x6D` above 0 |
/// | *"Allegiance Rank: %d"* | int `0x6E` above 0 |
/// | the heritage name | int `0xBC` non-zero |
/// | *"%s: %d"* skill | int `0x73` above 0 **and** int `0xB0` |
/// | *"%s: %d"* attribute | int `0x102` above 0 **and** int `0x101` |
/// | *"%s: %d"* vital | int `0x104` above 0 **and** int `0x103` |
///
/// Joined with *", "*, prefixed with *"Activation requires "* and drawn
/// only when the join is non-empty. The owner sentence is
/// *"This item can only be activated by "* + string `0x19` defaulting to
/// *"the original owner"* + *"."*, behind bool `0x5E` being 1.
#[must_use]
pub fn activation_requirement_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if !p.success {
        return out;
    }
    let mut terms: Vec<String> = Vec::new();
    if let Some(v) = p.item_difficulty {
        if v > 0 {
            terms.push(format!("Arcane Lore: {v}"));
        }
    }
    if let Some(v) = p.allegiance_rank_limit {
        if v > 0 {
            terms.push(format!("Allegiance Rank: {v}"));
        }
    }
    if let Some(h) = p.heritage_group.as_deref() {
        terms.push(h.to_string());
    }
    for (name, level) in [
        &p.activation_skill,
        &p.activation_attribute,
        &p.activation_attribute_2nd,
    ]
    .into_iter()
    .flatten()
    {
        terms.push(format!("{name}: {level}"));
    }
    if !terms.is_empty() {
        push_item_info(
            &mut out,
            format!("Activation requires {}", terms.join(", ")),
            true,
            0,
        );
    }
    if p.has_allowed_activator {
        let who = p.craftsman_name.as_deref().unwrap_or("the original owner");
        push_item_info(
            &mut out,
            format!("This item can only be activated by {who}."),
            true,
            0,
        );
    }
    out
}

/// The boost-value block -- the food-and-potion line.
///
/// Two guards run before anything is read: the object's description bitfield `& 0x10000` ("is a
/// healing kit") returns outright, and a **hook** showing a hooked healer does too. Then int `0x5A`
/// `BoostValue` and int `0x59` `BoosterEnum`, both required.
///
/// | `BoosterEnum` | positive | negative |
/// |---|---|---|
/// | 2 | *"Restores %d Health when used."* | *"Depletes ..."* |
/// | 4 | *"Restores %d Stamina when consumed."* | *"Depletes ..."* |
/// | 6 | *"Restores %d Mana when used."* | *"Depletes ..."* |
///
/// A negative value is negated before the `%d`, so the sentence never shows a sign.
///
/// **The client has a bug here and this build reproduces its visible half.** Any other
/// `BoosterEnum` adds the text of a 2,024-byte stack buffer that was never
/// written -- so retail prints whatever was on the stack. There is nothing faithful to emit for
/// that, and an empty run is the closest observable: the separator still appears, the garbage does
/// not.
#[must_use]
pub fn boost_value_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.weenie_is_healer || (p.weenie_is_hook && p.hooked_item_healer) {
        return out;
    }
    let (Some(v), Some(kind)) = (p.boost_value, p.booster_enum) else {
        return out;
    };
    let n = v.unsigned_abs();
    let text = match (kind, v < 0) {
        (2, false) => format!("Restores {n} Health when used."),
        (2, true) => format!("Depletes {n} Health when used."),
        (4, false) => format!("Restores {n} Stamina when consumed."),
        (4, true) => format!("Depletes {n} Stamina when consumed."),
        (6, false) => format!("Restores {n} Mana when used."),
        (6, true) => format!("Depletes {n} Mana when used."),
        // This branch leaves the output buffer uninitialized.
        _ => String::new(),
    };
    push_item_info(&mut out, text, false, 0);
    out
}

/// The heal-kit block -- the mirror image of [`boost_value_lines`]'s guard.
///
/// It draws **only** when the object is a healing kit (bitfield `& 0x10000`) or is a hook whose
/// hooked item is one, which is exactly the condition that makes the boost-value block return. So
/// the two blocks share int `0x5A` and never both draw it.
///
/// *"Bonus to Healing Skill: %d"* (the client, `same_line = 0`) from int `0x5A`, then
/// *"Restoration Bonus: %d%%"* (the client, `same_line = 1`) from float `0x64` **times 100**,
/// truncated. The float read's default is `1.0`, which is why a heal kit with no `HealkitMod` shows
/// nothing rather than `0%`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn heal_kit_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if !(p.weenie_is_healer || (p.weenie_is_hook && p.hooked_item_healer)) {
        return out;
    }
    if let Some(v) = p.boost_value {
        push_item_info(&mut out, format!("Bonus to Healing Skill: {v}"), false, 0);
    }
    if let Some(m) = p.healkit_mod {
        push_item_info(
            &mut out,
            format!(
                "Restoration Bonus: {}%",
                dereth_primitives::num::to_i32_f64(m * 100.0)
            ),
            true,
            0,
        );
    }
    out
}

/// The capacity block -- the pack line and the book line.
///
/// **The capacity numbers are not on the profile.** The client reads the items capacity and
/// containers capacity from the current object's description -- the
/// same two fields the inventory slot's capacity bar uses. No recorded `0x00C9` carries them and
/// none ever will; they arrive with the object description.
///
/// | arm | literal |
/// |---|---|
/// | both above 0 | *"Can hold up to %d items and %d containers."* |
/// | items only | *"Can hold up to %d items."* |
/// | containers only | *"Can hold up to %d containers."* |
///
/// Then *"%d of %d pages full."*  from int `0xAE` `AppraisalPages` and
/// int `0xAF` `AppraisalMaxPages`, in that printed order -- pages (`0xAE`) is the first `%d` and
/// the maximum (`0xAF`) the second. Both lines pass `same_line = 0`.
///
/// The whole block is skipped for a hook that is showing a hooked item.
#[must_use]
pub fn capacity_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.weenie_is_hook && p.hooked_item {
        return out;
    }
    let (i, c) = (p.items_capacity, p.containers_capacity);
    let text = if i > 0 {
        if c > 0 {
            Some(format!("Can hold up to {i} items and {c} containers."))
        } else {
            Some(format!("Can hold up to {i} items."))
        }
    } else if c > 0 {
        Some(format!("Can hold up to {c} containers."))
    } else {
        None
    };
    if let Some(t) = text {
        push_item_info(&mut out, t, false, 0);
    }
    if let Some((pages, max)) = p.pages {
        push_item_info(&mut out, format!("{pages} of {max} pages full."), false, 0);
    }
    out
}

/// The mana-stone block -- three lines, all `same_line = 1`.
///
/// Gated on the profile carrying **no** spell book, so
/// a mana stone that has absorbed spells draws nothing here and gets the magic-info block instead.
///
/// | key | literal | scaling |
/// |---|---|---|
/// | int `0x6B` `ItemCurMana` | *"Stored Mana: %d"* | none |
/// | float `0x57` `ItemEfficiency` | *"Efficiency: %d%%"* | **x 100**, truncated |
/// | float `0x89` `ManaStoneDestroyChance` | *"Chance of Destruction: %d%%"* | **x 100**, truncated |
///
/// **Both values are multiplied by 100.0 before truncation.** Leaving that out would print
/// `Efficiency: 0%` for every stone in the game.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn mana_stone_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.magic.spells.is_some() {
        return out;
    }
    if let Some(v) = p.stored_mana {
        push_item_info(&mut out, format!("Stored Mana: {v}"), true, 0);
    }
    if let Some(e) = p.item_efficiency {
        push_item_info(
            &mut out,
            format!(
                "Efficiency: {}%",
                dereth_primitives::num::to_i32_f64(e * 100.0)
            ),
            true,
            0,
        );
    }
    if let Some(d) = p.destroy_chance {
        push_item_info(
            &mut out,
            format!(
                "Chance of Destruction: {}%",
                dereth_primitives::num::to_i32_f64(d * 100.0)
            ),
            true,
            0,
        );
    }
    out
}

/// The appraisal show remaining uses.
///
/// ```text
/// int 0xC1      -> "Contains %d key."  when it is 1, else "...keys."
/// bool 0x3F     -> "Number of uses remaining:  Unlimited"             and stop
/// int 0x5C      -> "Number of uses remaining: %d"                     and stop
/// failed assess && (bitfield & 0x30000 || (hook && (hooked healer || hooked lockpick)))
///               -> "Number of uses remaining:  Unknown"              same_line 0
/// ```
///
/// **Two spaces** in both the `Unlimited` and `Unknown` literals; the `%d`
/// line has one. The `Unknown` arm is the only one that passes `same_line = 0`.
#[must_use]
pub fn remaining_uses_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if let Some(n) = p.num_keys {
        let word = if n == 1 { "key" } else { "keys" };
        push_item_info(&mut out, format!("Contains {n} {word}."), true, 0);
    }
    if p.unlimited_use {
        push_item_info(
            &mut out,
            "Number of uses remaining:  Unlimited".to_string(),
            true,
            0,
        );
        return out;
    }
    if let Some(s) = p.structure {
        push_item_info(&mut out, format!("Number of uses remaining: {s}"), true, 0);
        return out;
    }
    if p.success {
        return out;
    }
    let kit = p.weenie_is_healer || p.weenie_is_lockpick;
    let hooked = p.weenie_is_hook && (p.hooked_item_healer || p.hooked_item_lockpick);
    if kit || hooked {
        push_item_info(
            &mut out,
            "Number of uses remaining:  Unknown".to_string(),
            false,
            0,
        );
    }
    out
}

/// The craftsman block -- one line, and two reasons not to draw it.
///
/// *"Created by %s."*  from string `0x19` `CraftsmanName`, but **only** when
/// neither bool `0x55` `AppraisalHasAllowedWielder` nor bool `0x5E`
/// `AppraisalHasAllowedActivator` answered with a non-zero value -- because those two already
/// spent the same string on *"Wield requires %s"* and *"This item can only be activated by %s."*.
#[must_use]
pub fn craftsman_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.has_allowed_wielder || p.has_allowed_activator {
        return out;
    }
    if let Some(n) = p.craftsman_name.as_deref() {
        push_item_info(&mut out, format!("Created by {n}."), true, 0);
    }
    out
}

/// The bool `0x45` line the appraise-info write draws itself.
///
/// It is not a block: the appraise-info write asks bool `0x45` `IsSellable` between
/// the craftsman and rare-info blocks and draws *"This item cannot be sold."*  when
/// the read **succeeds and the value is zero**. An absent
/// key draws nothing, which is why almost no item shows it.
#[must_use]
pub fn cannot_be_sold_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.cannot_be_sold {
        push_item_info(&mut out, "This item cannot be sold.".to_string(), true, 0);
    }
    out
}

/// The rare-info block -- two lines, both `same_line = 0`.
///
/// **Both `%d` values in the timer sentence are the literal 3**: the client passes `3` twice,
/// and the format string takes exactly two.
///
/// *"Rare #%d"*  from int `0x11` `RareId`.
#[must_use]
pub fn rare_info_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    if p.rare_uses_timer {
        push_item_info(
            &mut out,
            "This rare item has a timer restriction of 3 minutes. You will not be able to use \
             another rare item with a timer within 3 minutes of using this one."
                .to_string(),
            false,
            0,
        );
    }
    if let Some(id) = p.rare_id {
        push_item_info(&mut out, format!("Rare #{id}"), false, 0);
    }
    out
}

// =================================================================================================
// The portal block.
// =================================================================================================
//
// A portal's appraisal carries its restriction lines. There is no `"cannot be tied to"` literal
// anywhere in the retail client (its strings contain `tied` in two endurance help paragraphs and
// nowhere else). Retail says the restrictions in as few sentences as bits are set,
// and both of them come out of the same five-armed block: it reads int `0x6F`
// (`PortalBitmask`), skips the whole block when that is absent, appends one literal per set bit,
// and ends with two unconditional lines — a blank, then the accumulated text.
//
// Note the order: **`0x20` is tested before `0x10`**, which is the one thing in this block a
// transcription could silently permute.
//
// Each arm *appends* its literal, trailing newline and all, to one string; the two
// lines at the end run whether or not any arm fired, so **every** object carrying a
// `PortalBitmask` gains a blank paragraph, and an `Unrestricted` portal gains a blank paragraph
// and an empty line. Both pass colour `0` and `same_line = 1`.
//
// In the world data all twenty-nine `Destroyed … Portal` weenies (1013, 1014, 1016 … 30384)
// carry `PropertyInt 111 = 49 = Unrestricted | NoSummon | NoRecall`, so identifying one draws
// arms 4 and 5 and neither of the first three.

/// `PropertyInt.PortalBitmask` -- `0x6F`, decimal 111.
///
/// It lives in [`dereth_client_contract::panels::examination`]; `dereth_client::hud` reads
/// the property.
pub use dereth_client_contract::panels::examination::PORTAL_BITMASK;

/// The five portal restrictions, in the client's test order, and the literal each appends.
///
/// | bit | `PortalBitmask` |
/// |---|---|
/// | `0x02` | `NoPk` |
/// | `0x04` | `NoPKLite` |
/// | `0x08` | `NoNPK` |
/// | `0x20` | `NoRecall` |
/// | `0x10` | `NoSummon` |
///
/// The trailing newline is **in each of the five literals**.
pub const PORTAL_RESTRICTION_LINES: [(i32, &str); 5] = [
    (0x02, "Player Killers may not use this portal.\n"),
    (0x04, "Lite Player Killers may not use this portal.\n"),
    (0x08, "Non-Player Killers may not use this portal.\n"),
    (0x20, "This portal cannot be recalled nor linked to.\n"),
    (0x10, "This portal cannot be summoned.\n"),
];

/// The client's portal block, whole.
///
/// Two runs or none: the blank and the accumulated paragraph, both
/// colour 0, same line. None at all when int `0x6F` misses, which is every object that
/// is not a portal.
#[must_use]
pub fn portal_restriction_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let Some(mask) = p.portal_bitmask else {
        return Vec::new();
    };
    let mut text = String::new();
    for (bit, literal) in PORTAL_RESTRICTION_LINES {
        // Only the low eight bits are ever asked about, and all five of these live there, so
        // the plain mask test is the client's own.
        if mask & bit != 0 {
            text.push_str(literal);
        }
    }
    let mut out = Vec::new();
    push_item_info(&mut out, String::new(), true, 0);
    push_item_info(&mut out, text, true, 0);
    out
}

/// The three presence-gated int keys at the head of
/// the item-examine panel's description block, and its fallback string key.
///
/// It lives in [`dereth_client_contract::panels::examination`]; `dereth_client::hud` reads
/// all four off the qualities.
pub use dereth_client_contract::panels::examination::{
    CREATION_TIMESTAMP, LIFESPAN, REMAINING_LIFESPAN, SHORT_DESC,
};

/// The client's lifespan block, whole.
///
/// The first three int reads are only presence gates. The client never uses the values
/// returned for `Lifespan` or `CreationTimestamp`, never read a clock and never calculate a new
/// remainder: the client immediately loads `RemainingLifespan` after all three calls succeed.
///
/// Each divisor comparison is strict (`>`), so an exact minute remains `60 seconds`, an exact hour
/// is `60 minutes, 0 seconds`, and so on. The five format strings are always plural.
#[must_use]
pub fn lifespan_lines(p: &AppraisalView) -> Vec<ItemInfo> {
    let Some(mut remaining) = p.remaining_lifespan else {
        return Vec::new();
    };
    let text = if remaining < 0 {
        "This item is in the act of disintegrating.".to_string()
    } else {
        let mut text = "This item expires in ".to_string();
        for (divisor, unit) in [
            (0x1E13380, "years"),
            (0x15180, "days"),
            (0xE10, "hours"),
            (0x3C, "minutes"),
        ] {
            if remaining > divisor {
                let amount = remaining / divisor;
                remaining %= divisor;
                text.push_str(&format!("{amount} {unit}, "));
            }
        }
        text.push_str(&format!("{remaining} seconds."));
        text
    };
    vec![ItemInfo {
        text,
        same_line: true,
        color: 0,
    }]
}

/// The remaining description blocks and which ones this build implements.
///
/// The function has four blocks in this order:
///
/// 1. The **lifespan** block is implemented by [`lifespan_lines`].
/// 2. The **description** block reads string `0x10` `LongDesc`, with
///    string `0x0F` `ShortDesc` as its fallback, and, when
///    int `0xAC` `AppraisalLongDescDecoration` is present, a rewrite that prefixes
///    the workmanship adjective (`& 1`, int `0x69`), substitutes the material
///    name (int `0x83`) and appends *", set with "* plus a gem count and name
///    (`& 4`, int `0xB1`/int `0xB2`). **The plain `LongDesc` half
///    and `ShortDesc` fallback are present; decoration is in [`decorated_description`].**
/// 3. The **portal** block is implemented by [`portal_restriction_lines`].
/// 4. The **augmentation cost** line reads int64 `3` `AugmentationCost` into a `StringInfo`
///    with `ID_Examine_Item_AugmentationCost` and
///    table `0x10000001`, which is the only line of this function that does not go through
///    the add-item-info step at all. **Drawn in `set_appraise_info` after the ordinary runs.**
///
/// The property keys are read in this order: `0x10B`, `0x62`, `0x10C`, `0x10`, `0x34`, `0xAC`,
/// `0x69`, `0x83`, `0xB1`, `0xB2`, `0x6F`, `0x0F`, `3`.
pub const DESCRIPTION_BLOCKS_NOT_IMPLEMENTED: &[&str] = &[];

/// The appraisal system's pluralized gem name read.
///
/// It lives in [`dereth_client_contract::panels::examination`], because the host supplies
/// the singular name and `dereth_client::hud` is the host.
pub use dereth_client_contract::panels::examination::pluralized_gem_name;

/// The description block's LongDesc-only rewrite. In particular,
/// GearPlatingName is an override in this arm, not a replacement for an absent LongDesc.
#[must_use]
pub fn decorated_description(p: &AppraisalView) -> Option<String> {
    let Some(long) = p.long_desc.as_ref() else {
        return p.short_desc.clone();
    };
    let mut text = p.gear_plating_name.as_ref().unwrap_or(long).clone();
    let Some(flags) = p.long_desc_decoration else {
        return Some(text);
    };
    let mut prefix = String::new();
    if flags & 1 != 0 {
        if let Some(workmanship) = p.workmanship {
            prefix.push_str(&workmanship_adjective(workmanship));
            prefix.push(' ');
        }
    }
    // There is no bit-2 test: int `0x83` present and positive are the only material gates.
    if let Some(material) = &p.description_material {
        prefix.push_str(material);
        prefix.push(' ');
        if !material.is_empty() && text.contains(material) {
            text = text.replace(material, "").trim().to_owned();
        }
    }
    prefix.push_str(&text);
    if flags & 4 != 0 {
        if let Some((count, name)) = &p.description_gems {
            // The count is formatted with `%ld` and then given thousands separators.
            prefix.push_str(&format!(", set with {} {name}", insert_commas(*count)));
        }
    }
    Some(prefix)
}

/// The item pane's whole description block, in the client's
/// order, minus the blocks named in
/// [`ITEM_BLOCKS_NOT_IMPLEMENTED`].
///
/// Value and burden are `same_line = TRUE`, as is every line of the weapon and armour blocks. The
/// usage block and the description-text arm pass **0**, the blank-line separator — so the `Use`
/// sentence and flavour text each start a paragraph. The lifespan prefix and portal suffix inside
/// the description block instead pass **1**.
#[must_use]
pub fn item_description(p: &AppraisalView) -> String {
    flatten_item_info(&item_description_runs(p))
}

/// The same blocks in the same order, as the **runs** the pane is given --
/// each with the colour index its call passes as the second argument.
///
/// Only the two mod blocks ever pass a non-zero one: every line of the value, burden, short magic
/// info, special properties, usage, lock appraisal, magic info and description blocks passes a
/// literal `0`.
#[must_use]
pub fn item_description_runs(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = Vec::new();
    push_item_info(&mut out, value_line(p.value), true, 0);
    push_item_info(&mut out, burden_line(p.burden), true, 0);
    // The tinkering block is the first block
    // after burden; its own unconditional blank is inside [`tinkering_lines`].
    out.extend(tinkering_lines(p));
    // The set block then the ratings block, and the appraise-info write adds a blank
    // of its own when **either** returned non-zero. The set block answers "I drew" and the
    // ratings block answers "I drew a term or a Vitality line", which is exactly "the vector is
    // non-empty".
    let set = set_lines(p);
    let ratings = ratings_lines(p);
    let shared_blank = !set.is_empty() || !ratings.is_empty();
    out.extend(set);
    out.extend(ratings);
    if shared_blank {
        push_item_info(&mut out, String::new(), true, 0);
    }
    for (line, color) in weapon_and_armor_lines(p) {
        push_item_info(&mut out, line, true, color);
    }
    // The defense-mod block sits between the weapon-and-armour block and the
    // armour-mods block.
    out.extend(defense_mod_lines(p));
    for (line, color) in armor_mod_lines(p) {
        push_item_info(&mut out, line, true, color);
    }
    // The short magic info block comes after the armour mods and **before** the
    // special properties.
    for (line, same) in short_magic_info_lines(p) {
        push_item_info(&mut out, line, same, 0);
    }
    // The special properties come after the short magic info and **before** the
    // usage block. Every one of its own lines passes `same_line = 1`.
    for (line, same) in special_properties_lines(p) {
        push_item_info(&mut out, line, same, 0);
    }
    if let Some(u) = p.use_text.as_deref() {
        push_item_info(&mut out, u.to_string(), false, 0);
    }
    // The nine blocks the appraise-info write calls between the usage block
    // and the lock block, in its own order:
    // The order is level limit, wield, usage limit, item level, activation, caster, boost, heal
    // kit, and capacity.
    out.extend(level_limit_lines(p));
    out.extend(wield_requirement_lines(p));
    out.extend(usage_limit_lines(p));
    out.extend(item_level_lines(p));
    out.extend(activation_requirement_lines(p));
    out.extend(caster_data_lines(p));
    out.extend(boost_value_lines(p));
    out.extend(heal_kit_lines(p));
    out.extend(capacity_lines(p));
    // The lock block runs after the usage and capacity blocks, before the
    // description. Its own lines pass `same_line = 0`, so each line is a paragraph.
    for line in lock_appraise_lines(p) {
        push_item_info(&mut out, line, false, 0);
    }
    // Mana stone, remaining uses, craftsman, then the appraise-info write's own
    // bool `0x45` line and the rare info -- everything between the lock block and the magic info.
    out.extend(mana_stone_lines(p));
    out.extend(remaining_uses_lines(p));
    out.extend(craftsman_lines(p));
    out.extend(cannot_be_sold_lines(p));
    out.extend(rare_info_lines(p));
    // The magic info is the **last** block before the flavour text, after the rare
    // info and before the description. Its own lines carry their separators.
    for (line, same) in magic_info_lines(p) {
        push_item_info(&mut out, line, same, 0);
    }
    // The description block's first part, before either description arm
    // and the later portal block.
    out.extend(lifespan_lines(p));
    // The client tests whether the string read succeeded, not the string's length. The decoration
    // helper preserves that distinction: `Some("")` suppresses the ShortDesc fallback.
    if let Some(d) = decorated_description(p) {
        push_item_info(&mut out, d, false, 0);
    }
    // The portal restrictions are inside the *same* description block
    // the flavour text comes from, and they come **after** it: the description arm
    // falls through to the int `0x6F` the block is guarded on. Nothing
    // in the appraise-info write's own block order separates them, so this is one more `extend` and
    // not a twenty-sixth block.
    out.extend(portal_restriction_lines(p));
    out
}

/// The creature and character panes' rows —
/// the nine info regions its constructor added to `0x10000149`.
///
/// `(label, value)`, in drawn order: the six attributes then the three vitals. The level is not
/// here — it has its own element, [`LEVEL_VALUE_TEXT`] `0x1000014C`, and its own setter.
#[must_use]
pub fn creature_rows(p: &AppraisalView) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(9);
    for a in CREATURE_ATTRIBUTE_ROWS {
        let name = attribute_name(a).unwrap_or_default();
        out.push((
            name.to_string(),
            attribute_value(p.attributes[(a - 1) as usize]),
        ));
    }
    for (v, percent) in CREATURE_VITAL_ROWS {
        let name = vital_name(v).unwrap_or_default();
        let current = p.vitals[(v - 1) as usize];
        let max = p.vitals[(v - 2) as usize];
        out.push((
            name.to_string(),
            vital_value(current, max, percent, p.success),
        ));
    }
    out
}

/// The colour each of [`creature_rows`]' nine **value** cells is drawn in, in the same drawn
/// order.
///
/// One [`info_region_color`] per token: the first six rows query the enchantment modifier for
/// their attribute ids, and the final three query it for the **maximum**, not the current, vital
/// ids. [`crate::view::AppraisalView::vital_enchanted`] is indexed like `vitals`, so the maximum
/// of vital `v` is at `v - 2`, the same index [`creature_rows`] reads the maximum from.
///
/// On a failed assess all nine are [`UNKNOWN_FONT`] and none of the enchantment-modifier reads is
/// made at all — the failure branch skips straight past them.
#[must_use]
pub fn creature_row_colors(p: &AppraisalView) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    for a in CREATURE_ATTRIBUTE_ROWS {
        out.push(info_region_color(
            p.success,
            p.attribute_enchanted[(a - 1) as usize],
        ));
    }
    for (v, _) in CREATURE_VITAL_ROWS {
        out.push(info_region_color(
            p.success,
            p.vital_enchanted[(v - 2) as usize],
        ));
    }
    out
}

/// The basic creature examine panel's level value text write — `L"???"` for anything below 1,
/// otherwise the number.
///
/// The guard is `level < 1`, not `== 0`: the profile carries the level as a signed int and an
/// absent int `0x19` leaves the local at zero.
#[must_use]
pub fn level_value(level: Option<i32>) -> String {
    match level {
        Some(v) if v >= 1 => v.to_string(),
        _ => "???".to_string(),
    }
}

// =================================================================================================
// The extra-info list.
// =================================================================================================
//
// The misc-info row helper, whole: with no list it does nothing at all; otherwise it adds a row
// from template 0 and writes the label into the row's `0x1000012A` text cell and the value into
// its `0x1000012B` text cell, both in the given colour.
//
// So the fourth argument is the **colour** index — the same colour argument of the coloured text
// write the item pane's [`ItemInfo::color`] uses, not a font — and it is applied to **both** cells
// of the row. Only one caller ever passes anything but `0`: the society row, which colours itself
// by whether the *viewer* shares the examined player's society.
//
// Two functions fill this list:
//
// * The creature examine panel's appraise info write — [`creature_misc_rows`];
// * The character pane uses [`char_misc_rows`], the larger by an order of
//   magnitude.

/// One row-helper call: the `0x1000012A` label cell, the `0x1000012B` value
/// cell, and the colour index both are drawn in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MiscRow {
    pub label: String,
    pub value: String,
    /// `0` plain, [`MOD_HIGH_FONT`] and [`MOD_LOW_FONT`] being the same two entries of the pane's
    /// colour array the item blocks use.
    pub color: u8,
}

fn misc(label: &str, value: impl Into<String>, color: u8) -> MiscRow {
    MiscRow {
        label: label.to_string(),
        value: value.into(),
        color,
    }
}

/// A row with two empty cells and colour 0 — the separator rows the rating block puts between its
/// groups and the armour block puts above itself. Both cells are the client's empty wide
/// literal.
fn misc_blank() -> MiscRow {
    MiscRow::default()
}

/// The society bit, its name, and which of [`crate::view::AppraisalView::society_ranks`]
/// carries its rank — the client, tested in this order.
///
/// `Faction1Bits` is `PropertyInt 281` and an assessment property, so the server sends it on
/// appraisal.
pub const SOCIETY_BITS: [(i32, &str, usize); 3] = [
    (1, "Celestial Hand", 0),
    (2, "Eldrytch Web", 1),
    (4, "Radiant Blood", 2),
];

/// Five closed bands, with no suffix outside them.
///
/// The bands are `1..=100`, `101..=300`, `301..=600`, `601..=1000` and `1001..=1500`. A rank of 0 —
/// which is also what an **absent** `SocietyRank*` leaves in the zero-initialised local — falls out
/// of all five and the row is the bare society name.
#[must_use]
pub fn society_rank_suffix(rank: i32) -> &'static str {
    match rank {
        1..=100 => " ~ Initiate",
        101..=300 => " ~ Adept",
        301..=600 => " ~ Knight",
        601..=1000 => " ~ Lord",
        1001..=1500 => " ~ Master",
        _ => "",
    }
}

/// Build the *"Society:"* row.
///
/// The gate is that int `0x119` **succeeded**, not that it is non-zero: a player carrying
/// `Faction1Bits` with none of the low three bits set draws *"???"*.
///
/// The colour is the whole reason the client reads the **viewer's** `Faction1Bits` as well
/// (int `0x119` off the player description): the same society is `1`, one
/// of the other two is `2`, and no society at all leaves colour `0`.
#[must_use]
pub fn society_row(p: &AppraisalView) -> Option<MiscRow> {
    let bits = p.faction_bits?;
    let mine = p.viewer_faction_bits;
    for (bit, name, slot) in SOCIETY_BITS {
        if bits & bit == 0 {
            continue;
        }
        // The viewer's own bit first, then the other two bits of the low three.
        let color = if mine & bit != 0 {
            MOD_HIGH_FONT
        } else if mine & (7 & !bit) != 0 {
            MOD_LOW_FONT
        } else {
            0
        };
        let suffix = society_rank_suffix(p.society_ranks[slot]);
        return Some(misc("Society:", format!("{name}{suffix}"), color));
    }
    Some(misc("Society:", "???", 0))
}

/// Emit *"Monarch/Patron:"*, *"Monarch:"* + *"Patron:"*, or
/// *"Alleg. Monarch:"*.
///
/// Nothing at all when the allegiance rank (int `0x1E`) is below 1, which is also what an absent
/// rank gives because the local was zeroed.
///
/// The fork is on the two **strings**: string `0x15` `MonarchsTitle` present takes the
/// monarch/patron arm, absent takes the follower arm. Inside the first, string `0x23`
/// `PatronsTitle` present and **equal** to the monarch's is one row labelled
/// *"Monarch/Patron:"*; present and different is two rows; absent is *"Monarch:"* alone.
///
/// The server writes exactly that pair into the appraisal profile: a monarch gets
/// `AllegianceFollowers` and nobody else gets `MonarchsTitle`/`PatronsTitle`, so the two arms are
/// "you are looking at a monarch" and "you are looking at a vassal".
#[must_use]
pub fn allegiance_rows(p: &AppraisalView) -> Vec<MiscRow> {
    let mut out = Vec::new();
    if p.allegiance_rank.unwrap_or(0) < 1 {
        return out;
    }
    match (p.monarch_title.as_deref(), p.patron_title.as_deref()) {
        (Some(m), Some(patron)) if m == patron => out.push(misc("Monarch/Patron:", m, 0)),
        (Some(m), Some(patron)) => {
            out.push(misc("Monarch:", m, 0));
            out.push(misc("Patron:", patron, 0));
        }
        (Some(m), None) => out.push(misc("Monarch:", m, 0)),
        (None, _) => {
            // A missing or negative follower count becomes zero.
            let n = p.allegiance_followers.filter(|v| *v >= 0).unwrap_or(0);
            // Exactly 1 takes the singular literal; anything else the plural.
            let word = if n == 1 { "Follower" } else { "Followers" };
            out.push(misc("Alleg. Monarch:", format!("{n} {word}"), 0));
        }
    }
    out
}

/// The sentinel the server adds to a body part whose every layer is **unenchantable**, so that the
/// client can put a `*` in front of the number: it compares against `0x270F` and subtracts 9999.
///
/// The server does the mirror image: when all layers for a body part are unenchantable it sends
/// the total armour level plus 9999 for the client to display.
pub const UNENCHANTABLE: i32 = 9999;

/// The `%s` of *"AL: %s/%s/%s"* for one body part.
#[must_use]
pub fn armor_level_text(v: i32) -> String {
    if v >= UNENCHANTABLE {
        format!("*{}", v - UNENCHANTABLE)
    } else {
        v.to_string()
    }
}

/// The three armour rows, each with its label and the three `base_armor` slots it prints, in the
/// order the client read them.
pub const ARMOR_ROWS: [(&str, [usize; 3]); 3] = [
    ("Head/Chest/Groin", [0, 1, 2]),
    ("Bicep/Wrist/Hand", [3, 4, 5]),
    ("Thigh/Shin/Foot", [6, 7, 8]),
];

/// Build the blank row and the three *"AL: %s/%s/%s"* rows.
///
/// The guard is an OR of nine `> 0` tests: **any** body part above
/// zero draws all three rows. Those nine values are the `0x4000` block of the profile, not
/// properties, which is why the whole block vanishes for an object whose appraisal carries no
/// `BASE_ARMOR` — and the server attaches one only for a player or a non-attackable creature.
///
/// **These reads are on the current appraisal profile's** nine-value base-armour block, not on a
/// stale creature object; reading the latter would draw unrelated values.
#[must_use]
pub fn armor_rows(p: &AppraisalView) -> Vec<MiscRow> {
    let Some(a) = p.base_armor else {
        return Vec::new();
    };
    if !a.iter().any(|v| *v > 0) {
        return Vec::new();
    }
    let mut out = vec![misc_blank()];
    for (label, slots) in ARMOR_ROWS {
        let text = format!(
            "AL: {}/{}/{}",
            armor_level_text(a[slots[0]]),
            armor_level_text(a[slots[1]]),
            armor_level_text(a[slots[2]])
        );
        out.push(misc(label, text, 0));
    }
    out
}

/// How a rating group prints its pair.
///
/// **`Rating` and `Resist` are not transcription slips.** Their formats are `"%Rating: %d/%d"`
/// and `"%Resist: %d/%d"`, and `%R` is not a conversion: both go through
/// the CRT's `_vsnprintf`, which writes an unrecognised specifier's
/// *character* and drops the `%`. So the rendered text is *"Rating: 12/5"* and *"Resist: 12/5"* —
/// printing the literal `%` would be the bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingText {
    /// `"Rating: 12/5"`.
    Rating,
    /// `"Resist: 12/5"`.
    Resist,
    /// `"+12/-5"`, the overpower pair.
    Signed,
}

impl RatingText {
    fn format(self, left: i32, right: i32) -> String {
        match self {
            Self::Rating => format!("Rating: {left}/{right}"),
            Self::Resist => format!("Resist: {left}/{right}"),
            Self::Signed => format!("+{left}/-{right}"),
        }
    }
}

/// One rating group: `(label, text, guard slots, left, right)`, indexing
/// [`crate::view::AppraisalView::ratings`].
pub type RatingGroup = (&'static str, RatingText, &'static [usize], usize, usize);

/// The damage and critical-damage groups both panes open with.
///
/// The guard slots are wider than the printed pair: `CritRating 0x139` and
/// `CritResistRating 0x13B` can each open their group without appearing in it.
const DAMAGE_RATING: RatingGroup = ("Dmg/CritDmg", RatingText::Rating, &[0, 2, 3], 0, 3);
const DAMAGE_RESIST: RatingGroup = ("Dmg/CritDmg", RatingText::Resist, &[1, 4, 5], 1, 5);
/// `0x17D PKDamageRating` / `0x17E PKDamageResistRating`.
const PK_DAMAGE: RatingGroup = ("PK Dmg/Res", RatingText::Rating, &[9, 10], 9, 10);
/// `0x182 Overpower` / `0x183 OverpowerResist`.
const OVERPOWER: RatingGroup = ("Overpower %", RatingText::Signed, &[11, 12], 11, 12);
/// Always last in both panes.
const DOT_LIFE_RESIST: RatingGroup = ("DoT/Life:", RatingText::Resist, &[7, 8], 7, 8);

/// The character pane's five rating groups, in its order: the player-killer pair comes
/// **before** the overpower pair.
pub const CHARACTER_RATING_ROWS: [RatingGroup; 5] = [
    DAMAGE_RATING,
    DAMAGE_RESIST,
    PK_DAMAGE,
    OVERPOWER,
    DOT_LIFE_RESIST,
];

/// The creature pane's five rating groups, in its order: the overpower pair comes **before** the
/// player-killer pair, the other way round from the character pane.
pub const CREATURE_RATING_ROWS: [RatingGroup; 5] = [
    DAMAGE_RATING,
    DAMAGE_RESIST,
    OVERPOWER,
    PK_DAMAGE,
    DOT_LIFE_RESIST,
];

/// Build one pane's rating block from its group table.
///
/// A single blank row precedes the **first** group that draws, and a trailing blank follows when
/// any drew; a group whose guards are all `<= 0` contributes nothing.
///
/// int `0x143` `HealingBoostRating` is read and
/// **never used** — dead in retail, so dead here. `ratings[6]` exists to keep the slot
/// numbering honest and is deliberately unused.
#[must_use]
pub fn rating_rows(p: &AppraisalView, groups: &[RatingGroup]) -> Vec<MiscRow> {
    let mut out = Vec::new();
    // Every `%d` prints the zero-initialised local, so an absent key is `0` and not "missing".
    let v = |i: usize| p.ratings[i].unwrap_or(0);
    let mut any = false;
    for (label, text, guard, left, right) in groups.iter().copied() {
        if !guard.iter().any(|i| v(*i) > 0) {
            continue;
        }
        if !any {
            out.push(misc_blank());
            any = true;
        }
        out.push(misc(label, text.format(v(left), v(right)), 0));
    }
    if any {
        out.push(misc_blank());
    }
    out
}

/// The unconditional last row of the **character** pane, the client: label
/// `L"* = Unenchantable"`, value empty.
///
/// This is also where the creature-profile guard continues when the profile carries no creature
/// block, so it is the one row that draws whatever else did not.
pub const UNENCHANTABLE_FOOTNOTE: &str = "* = Unenchantable";

/// The character pane's misc-info rows, in the client's order.
///
/// | # | row | key |
/// |---|---|---|
/// | 1 | *"Society:"* | int `0x119` + int `0x11F`/`0x120`/`0x121` |
/// | 2 | the monarch/patron trio | int `0x1E`, string `0x15`, string `0x23`, int `0x23` |
/// | 3 | the blank above the armour | the nine `base_armor` dwords |
/// | 4 | the three *"AL:"* rows | the same |
/// | 5 | the rating groups | `0x133 0x134 0x139 0x13A 0x13B 0x13C 0x17D 0x17E 0x182 0x183 0x15E 0x15F` |
/// | 6 | *"Fellowship:"* | string `0x0A` |
/// | 7 | *"Arrived in Dereth:"* | string `0x2B` |
/// | 8 | *"Time in Dereth:"* | int `0x7D` through the delta-time formatter |
/// | 9 | *"Chess Rank:"* | int `0xB5` |
/// | 10 | *"Fishing Skill:"* | int `0xC0` |
/// | 11 | *"Deaths:"* | int `0x2B`, `<= 0` being *"Has never died"* |
/// | 12 | *"Titles Earned:"* | int `0x106` |
/// | 13 | *"Enlightenment:"* | int `0x186`, drawn whenever present |
/// | 14 | the unenchantable footnote | none — always |
#[must_use]
pub fn char_misc_rows(p: &AppraisalView) -> Vec<MiscRow> {
    let mut out = Vec::new();
    out.extend(society_row(p));
    out.extend(allegiance_rows(p));
    out.extend(armor_rows(p));
    out.extend(rating_rows(p, &CHARACTER_RATING_ROWS));
    if let Some(f) = p.fellowship.as_deref() {
        out.push(misc("Fellowship:", f, 0));
    }
    if let Some(d) = p.date_of_birth.as_deref() {
        out.push(misc("Arrived in Dereth:", d, 0));
    }
    if let Some(age) = p.age {
        // The delta-time formatter `panels::journal` already carries, whose only other caller is
        // the journal's timer.
        let text = crate::panels::journal::delta_time_to_string(i64::from(age));
        out.push(misc("Time in Dereth:", text, 0));
    }
    if let Some(v) = p.chess_rank {
        out.push(misc("Chess Rank:", v.to_string(), 0));
    }
    if let Some(v) = p.fishing_skill {
        out.push(misc("Fishing Skill:", v.to_string(), 0));
    }
    if let Some(v) = p.num_deaths {
        // The test is `< 1`, so zero deaths is the sentence and not the number.
        let text = if v < 1 {
            "Has never died".to_string()
        } else {
            v.to_string()
        };
        out.push(misc("Deaths:", text, 0));
    }
    if let Some(v) = p.num_character_titles {
        out.push(misc("Titles Earned:", v.to_string(), 0));
    }
    // Gated on the property being present, not on its value: a character stamped with zero
    // enlightenment still shows the line.
    if let Some(v) = p.enlightenment {
        out.push(misc("Enlightenment:", v.to_string(), 0));
    }
    out.push(misc(UNENCHANTABLE_FOOTNOTE, "", 0));
    out
}

/// The creature pane's share of the same list — [`rating_rows`] over [`CREATURE_RATING_ROWS`] and
/// nothing else.
///
/// The rest of that function is the creature display name (int `2`, then the appraisal
/// system's creature display name read, already
/// [`crate::view::AppraisalView::creature_display_name`]), the clearing of this list, and a
/// text clear on [`ALLEGIANCE_NAME_TEXT`]. It has **no** unenchantable footnote: a creature
/// has no per-part armour row to footnote.
#[must_use]
pub fn creature_misc_rows(p: &AppraisalView) -> Vec<MiscRow> {
    rating_rows(p, &CREATURE_RATING_ROWS)
}

/// The three literals the client writes into
/// the PK-status text [`CHAR_PK_STATUS_TEXT`] `0x10000152`.
///
/// These are wide literals used directly by this function, **not** the `StringInfo`
/// tokens resolved for the player's own stat panel. The two paths print the same three phrases
/// from different sources.
#[must_use]
pub fn pk_status_text(is_pk: bool, is_pk_lite: bool) -> &'static str {
    if is_pk {
        "Player Killer"
    } else if is_pk_lite {
        "Player Killer Lite"
    } else {
        "Non-Player Killer"
    }
}

/// `ExaminationPanel` — the panel, bound off the gameplay screen's root.
///
/// `Default` is the constructor, which is all zeroes
/// **except** the newly-selected-item flag, which starts `true` — so it is written by hand rather
/// than derived.
#[derive(Debug)]
pub struct ExaminationPanel {
    /// `<EXAM>` itself; the visibility writes go to this.
    pub window: Option<ElemHandle>,
    /// [`CLOSE_BUTTON`]. Bound so that a layout without the control is a `None` a test can see,
    /// rather than a click that silently matches nothing.
    close_button: Option<ElemHandle>,
    title: Option<ElemHandle>,
    item_base: Option<ElemHandle>,
    item_display_text: Option<ElemHandle>,
    creature_base: Option<ElemHandle>,
    creature_sub: Option<ElemHandle>,
    char_sub: Option<ElemHandle>,
    spell_base: Option<ElemHandle>,
    /// The spell pane — the fourth sub-UI. `spell_base` is bound by the constructor and
    /// [`Self::set_active_examine_ui`] shows it; this fills it. See
    /// [`crate::panels::spell_examine`].
    pub spell: crate::panels::spell_examine::SpellExamineUi,
    /// The inscription box, its signature line and the background
    /// panel behind them -- the item pane's constructor binds all three.
    inscription_text: Option<ElemHandle>,
    inscription_signature: Option<ElemHandle>,
    inscription_background: Option<ElemHandle>,
    /// The level value text and the creature display name.
    level_value: Option<ElemHandle>,
    creature_name: Option<ElemHandle>,
    /// The nine-row stat list the creature and character panes share.
    stat_list: Option<crate::panels::listbox::ListBoxWidget>,
    /// [`MISC_LIST`] — the extra-info list, the **second** list box of the
    /// creature base and the one every [`MiscRow`] lands in.
    misc_list: Option<crate::panels::listbox::ListBoxWidget>,
    /// The character pane's four text elements — [`CHAR_HERITAGE_TEXT`],
    /// [`CHAR_PROFESSION_TEXT`], [`CHAR_PK_STATUS_TEXT`] and [`ALLEGIANCE_NAME_TEXT`].
    char_heritage: Option<ElemHandle>,
    char_profession: Option<ElemHandle>,
    char_pk_status: Option<ElemHandle>,
    allegiance_name: Option<ElemHandle>,

    /// The awaited appraisal id — the object the
    /// examine request stored and has not yet been answered about.
    pub awaiting: Option<dereth_primitives::ObjectId>,
    /// The current appraisal id — the object the panel is currently showing.
    pub current: Option<dereth_primitives::ObjectId>,
    /// The active pane.
    pub active: Option<ExamineSubUi>,
    /// The object id of the **active** pane, which is what the appraise-info write's
    /// new-object flag compares against.
    sub_object: Option<dereth_primitives::ObjectId>,
    /// The paper doll — the viewport element that hosts the appraised
    /// creature's 3D portrait. `None` when the layout has no `0x10000148`, which is
    /// the client's own null check and makes the portrait absent rather than the panel broken.
    pub creature_viewport: Option<ElemHandle>,
    /// The title last written, so a test can read it back without the glyph list.
    pub title_text: Option<String>,
    /// The item description text's content, likewise.
    pub item_text: Option<String>,
    /// What the inscription write put in the edit box, or `None` for an object
    /// that is not inscribable at all -- in which case the client takes the box away.
    pub inscription: Option<String>,
    /// The `--<scribe>` signature line, empty while the box invites an inscription.
    pub inscription_signature_text: String,
    /// The scribe name — string `8`, or the player's own name once he
    /// has written on it and the box has lost focus.
    ///
    /// It is a **cache, not a view of the profile**: the gaining-focus handler
    /// tests it against `""` to decide whether the box is showing the invitation, the
    /// editable-state write tests it against `""` and against the player's
    /// name to decide who may write, and the losing-focus handler tests it to
    /// decide whether clearing the box is worth a message.
    pub scribe_name: String,
    /// The inscription — string `7`, replaced by whatever was typed
    /// once the box loses focus. The `!=` against this field is what
    /// stops a second focus loss re-sending the same inscription.
    pub inscription_value: String,
    /// The player's own name, refreshed by [`Self::update`].
    ///
    /// The client reads the player through a global; this panel is handed a [`GameView`] only on
    /// the per-frame pull, and the three inscription handlers run from an element message that has
    /// no view. So the name is latched on the pull, which is the same value at the same moment.
    pub player_name: String,
    /// The client's answer for the object now shown — whether the
    /// two `BoolProperty`s `0x16` and `0x27` went true.
    pub inscription_editable: bool,
    /// The live object facts the mouse-press handler reads after an
    /// unsigned locked inscription background is pressed: `(inscribable, location)`, or
    /// `None` when the object is not known.
    inscription_mouse_facts: Option<(bool, u32)>,
    /// How many `0xBF Writing_SetInscription` requests this panel has emitted. A denominator, so
    /// "the commit edge never fired" and "it fired and the guards refused" are different answers.
    pub inscriptions_sent: u32,
    /// The basic creature examine panel's level value text write's answer.
    pub level_text: Option<String>,
    /// The appraisal system's creature display name read's answer.
    pub creature_name_text: Option<String>,
    /// The nine `(label, value)` rows `creature_rows` produced, in drawn order, so a test can read
    /// the pane back without walking the list box's children.
    pub creature_row_text: Vec<(String, String)>,
    /// The colour index each of those rows' **value** cells was drawn in, in the same order —
    /// [`creature_row_colors`]' answer for the reply now showing.
    ///
    /// Kept beside [`Self::creature_row_text`] rather than folded into it because that field is
    /// read by name outside this crate; the two are always the same length.
    pub creature_row_colors: Vec<u8>,
    /// How many of those actually reached a row **element**. A denominator: "the pane computed
    /// nine rows" and "nine rows are on screen" are different facts, and a layout whose list box
    /// carries no row template would otherwise fail silently.
    pub creature_rows_drawn: u32,
    /// The [`MiscRow`]s [`char_misc_rows`] or [`creature_misc_rows`] produced for the reply now
    /// showing, in drawn order.
    pub misc_row_text: Vec<MiscRow>,
    /// How many of those reached a row **element** of [`MISC_LIST`] — the same denominator
    /// [`Self::creature_rows_drawn`] is, and for the same reason: "the pane computed the rows"
    /// and "the rows are on screen" are different facts.
    pub misc_rows_drawn: u32,
    /// The heritage text's content — the gender-and-heritage display read's answer, or `None` when
    /// the client's `if` left the element alone.
    pub heritage_text: Option<String>,
    /// The profession text's content — the character title, else the template.
    pub profession_text: Option<String>,
    /// The PK-status text's content, one of [`pk_status_text`]'s three literals. `None`
    /// only when the pane has never been a character pane, because the client's guard is on
    /// the current object being known rather than on the profile.
    pub pk_status_text: Option<String>,
    /// The allegiance-name text's content. The allegiance write runs on **every**
    /// character reply, so this is `None` for a player with no allegiance rank and not a stale
    /// name from the object before.
    pub allegiance_name_text: Option<String>,
    /// How many replies this panel has taken — a denominator (the stated testability rule, "assert the
    /// denominator"), so "the panel did not open" and "no reply reached it" are different answers.
    pub replies_applied: u32,
    /// How many of those were the *awaited* one, i.e. how many opened the window.
    pub opened: u32,
    /// `dereth_client_model::AppraisalCache::delivery` for the reply last applied, so the pull in
    /// [`Self::update`] can tell a fresh reply from the same one being offered again. The client
    /// has no such field because it is called *by* the reply; see that method.
    last_delivery: u64,
    /// `dereth_client_model::AppraisalCache::examine_serial` last seen, so the pull in [`Self::update`] can
    /// tell a **new** examine-object notice from the same one being re-offered every
    /// frame; see [`crate::view::GameView::examine_request`].
    last_examine_serial: u64,
    /// How many notices [`Self::update`]'s pull has turned into a
    /// examine-object notice — a denominator, so "the cursor route never asked" and "it asked
    /// and the panel refused" are different answers.
    pub examines_pulled: u32,
    /// How many examine-spell notices [`Self::update`]'s pull has taken.
    /// A denominator, so "the right-click never produced a notice" and "it did
    /// and the pane refused the spell" are different answers.
    pub spell_examines_pulled: u32,
    /// How many appraise-`0` cancels [`Self::examine_spell`] has raised.
    /// A denominator, so "the spell examine never cancelled" and "it cancelled
    /// when the client says it should have jumped over the call" are different answers.
    pub appraisals_cancelled: u32,
    /// How many times the window has been hidden — the close half's denominator, so "it never
    /// closed" and "it closed and something re-opened it" are different answers.
    pub closed: u32,
    /// The newly-selected-item flag, initialized to `true`. It gates the
    /// selection-change reaction while a spell component changes the selected object.
    ///
    /// It has exactly **one** clearer, immediately before the call it guards:
    ///
    /// Resolve the clicked component to an object id; zero does nothing. Otherwise clear
    /// the flag, select that object with the second argument zero, then restore the flag.
    ///
    /// So it is not a mode and never a latch: it is a **re-entrancy bracket** around one
    /// synchronous call. The selected object write broadcasts
    /// the selection-changed notice, *inside* the bracket, and
    /// the panel's selection-changed receiver reads the flag, which is this same byte. The two
    /// writes sit next to each other.
    ///
    /// **The bracket alone would not work here, and that is the one thing this build must do
    /// differently.** The client's reader is *pushed* inside the assignment; [`Self::update`]'s is
    /// a *pull* on the next frame, by which time an eager restore would have re-opened the gate.
    /// So [`Self::select_spell_component`] clears it and **[`Self::update`] performs the restore**,
    /// on the frame that absorbs the change the clear was hiding — the same one write,
    /// moved to the far side of the same broadcast.
    pub examine_newly_selected_item: bool,
    /// Selection changes [`Self::update`] absorbed with the guard down — i.e. the
    /// ones this panel made itself through [`Self::select_spell_component`], and which therefore
    /// did **not** produce a re-examine.
    ///
    /// A denominator: "the spell pane clicked a component and the panel did not re-examine" and
    /// "the spell pane never clicked anything" are otherwise the same observation.
    pub self_selections_absorbed: u32,

    /// The SCID of the component row the formula list last reported selected,
    /// waiting for [`Self::update`]'s pull to resolve it against the component tracker.
    ///
    /// The client resolves inside the message, because it reaches
    /// the component tracker through a global. Here the tracker is behind the
    /// [`GameView`] seam and `on_element_message` carries no view, so the row is latched and the
    /// lookup happens on the next pull — one frame later, exactly as the `examine_request`,
    /// `selection_changed` and examine-spell producers already are. See
    /// [`Self::spell_component_selected`].
    pending_component_scid: Option<u32>,
    /// How many component rows the formula list has reported selected — the denominator that
    /// separates "the click never arrived" from "it arrived and resolved to nothing".
    pub component_clicks: u32,
    /// How many of those resolved to an object and reached the selected-object write.
    pub component_selections: u32,
    /// The update-spell-components notice's serial as this panel last saw it.
    /// The pull half of the panel's update-spell-components receiver.
    /// See [`crate::view::GameView::component_serial`].
    last_component_serial: u64,
    /// How many update-spell-components notices [`Self::update`]'s pull has turned into a
    /// re-mark of the spell pane's formula rows.
    pub component_notices_pulled: u32,
    /// The world selection as this panel last saw it. The client is *pushed*
    /// the selection-changed notice; this build has no such notice, so [`Self::update`] pulls
    /// [`GameView::selected_object`] and raises the handler on a change. Same shape as the
    /// `examine_request` pull above, and the same one difference: the client's handler
    /// runs on the notice, this one on the frame that first sees a different id.
    last_selected: Option<Option<dereth_primitives::ObjectId>>,
}

fn inscription_editable_from_live_object(
    live_facts: Option<(bool, u32)>,
    ordinary_permission: bool,
    viewer_is_psr: bool,
) -> bool {
    live_facts.is_some_and(|(inscribable, _)| inscribable) && (ordinary_permission || viewer_is_psr)
}

impl Default for ExaminationPanel {
    fn default() -> Self {
        Self {
            window: None,
            close_button: None,
            title: None,
            item_base: None,
            item_display_text: None,
            creature_base: None,
            creature_viewport: None,
            creature_sub: None,
            char_sub: None,
            spell_base: None,
            spell: crate::panels::spell_examine::SpellExamineUi::default(),
            inscription_text: None,
            inscription_signature: None,
            inscription_background: None,
            level_value: None,
            creature_name: None,
            stat_list: None,
            misc_list: None,
            char_heritage: None,
            char_profession: None,
            char_pk_status: None,
            allegiance_name: None,
            misc_row_text: Vec::new(),
            misc_rows_drawn: 0,
            heritage_text: None,
            profession_text: None,
            pk_status_text: None,
            allegiance_name_text: None,
            awaiting: None,
            current: None,
            active: None,
            sub_object: None,
            title_text: None,
            item_text: None,
            inscription: None,
            inscription_signature_text: String::new(),
            // The item-examine panel initializes all three text fields as empty.
            scribe_name: String::new(),
            inscription_value: String::new(),
            player_name: String::new(),
            inscription_editable: false,
            inscription_mouse_facts: None,
            inscriptions_sent: 0,
            level_text: None,
            creature_name_text: None,
            creature_row_text: Vec::new(),
            creature_row_colors: Vec::new(),
            creature_rows_drawn: 0,
            replies_applied: 0,
            opened: 0,
            last_delivery: 0,
            last_examine_serial: 0,
            examines_pulled: 0,
            spell_examines_pulled: 0,
            appraisals_cancelled: 0,
            closed: 0,
            // The examination panel's constructor's last line.
            examine_newly_selected_item: true,
            self_selections_absorbed: 0,
            pending_component_scid: None,
            component_clicks: 0,
            component_selections: 0,
            last_component_serial: 0,
            component_notices_pulled: 0,
            last_selected: None,
        }
    }
}

impl ExaminationPanel {
    /// The examination panel's initialization plus the three sub-UI constructors' bindings, off the
    /// gameplay screen root — `<EXAM>` is a window of that root, not a page of the panel stack.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.window = ui.get_child_recursive(root, WINDOW);
        let Some(w) = self.window else { return };
        self.close_button = ui.get_child_recursive(w, CLOSE_BUTTON);
        self.title = ui.get_child_recursive(w, DISPLAYED_NAME_TEXT);
        self.item_base = ui.get_child_recursive(w, ITEM_BASE);
        self.item_display_text = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_DISPLAY_TEXT));
        self.creature_base = ui.get_child_recursive(w, CREATURE_BASE);
        // The paper doll: off the window, recursively, exactly as the
        // client does -- the one binding in this constructor that does not start at the
        // creature base field.
        self.creature_viewport = ui.get_child_recursive(w, CREATURE_PAPER_DOLL);
        self.creature_sub = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_SUB_PANEL));
        self.char_sub = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHARACTER_SUB_PANEL));
        self.spell_base = ui.get_child_recursive(w, SPELL_BASE);
        // The fourth sub-UI's constructor, which
        // binds its own seven children off the same `0x10000153`.
        self.spell.post_init(ui, w);
        // The inscription trio hangs off the item base field, and the creature pane's
        // level cell, name line and stat list off the creature base -- the same two constructors
        // that bound the rest.
        self.inscription_text = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_TEXT));
        self.inscription_signature = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_SIGNATURE_TEXT));
        self.inscription_background = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_BACKGROUND));
        self.level_value = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, LEVEL_VALUE_TEXT));
        self.creature_name = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_DISPLAY_NAME));
        self.stat_list = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_STAT_LIST))
            .map(|h| crate::panels::listbox::ListBoxWidget::bind(ui, h));
        // The character pane's constructor binds five more children off the same creature-base
        // field as the stat list: the misc list and four text elements.
        self.misc_list = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, MISC_LIST))
            .map(|h| crate::panels::listbox::ListBoxWidget::bind(ui, h));
        self.char_heritage = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_HERITAGE_TEXT));
        self.char_profession = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_PROFESSION_TEXT));
        self.char_pk_status = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_PK_STATUS_TEXT));
        self.allegiance_name = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, ALLEGIANCE_NAME_TEXT));
        self.awaiting = None;
        self.current = None;
        self.active = None;
        self.sub_object = None;
    }

    /// Whether `post_init` found the window at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.window.is_some()
    }

    /// Whether `post_init` found the close control. A layout without it is a `None` here rather
    /// than a click that quietly matches nothing.
    #[must_use]
    pub fn close_control_bound(&self) -> bool {
        self.close_button.is_some()
    }

    /// Whether `<EXAM>` is on screen right now — on the window,
    /// which is what both close paths test first.
    #[must_use]
    pub fn is_open(&self, ui: &UiSystem) -> bool {
        self.window
            .and_then(|w| ui.node(w))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// The floaty examination panel's element-message handler — **the close control.**
    ///
    /// Message 1 from `0x100005F3` hides the window; every message then goes on to the base
    /// examination panel's handler. See [`CLOSE_BUTTON`]. Returns whether this call closed the
    /// window.
    ///
    /// **It does not touch `awaiting` or `current`**, and that is the point: the current appraisal
    /// id stays set, so the client's 0.75-second combat re-poll keeps arriving and keeps taking the
    /// appraise-info write's not-new path — refilling a hidden panel and **not** re-showing it.
    /// Clearing them here would make the next re-poll look like a fresh appraisal and pop the
    /// window straight back open, which is the defect this arm exists to avoid.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        // The inscription arms: focus-changed
        // (`0x2F`) on the edit box `0x1000013E` goes to the losing-focus handler when `p1 == 0`
        // and to the gaining-focus handler otherwise, and a press on the background `0x10000137`
        // goes to the mouse-press handler.
        if m.source_id == ITEM_INSCRIPTION_TEXT
            && m.id == dereth_ui::msg::element::id::FOCUS_CHANGED
        {
            if m.p1 == 0 {
                self.handle_inscription_losing_focus(ui);
            } else {
                self.handle_inscription_gaining_focus(ui);
            }
            return false;
        }
        if m.source_id == ITEM_INSCRIPTION_BACKGROUND
            && m.id == dereth_ui::msg::element::id::MOUSE_PRESS
            && m.p1 == dereth_ui::focus::action::PRIMARY_CLICK
        {
            self.handle_inscription_mouse_press(&mut ui.requests);
            return false;
        }
        // The third arm handles the formula list box reporting a
        // new selection. `SPELL_COMPONENT_LIST_MESSAGE` is `4`, which is
        // the list box's selected-item write broadcasting message 4 with the index and the
        // selected row — so the *gesture* is a **primary** press or a double-click on
        // a row (its mouse-down handler takes actions `7` and `0x0A`; the `0x1C` arm
        // takes `7`), gated on this list's `0x59 ClickSelect`, which `0x1000032D` declares in
        // `0x2100001C`. A secondary press reaches neither site and does nothing here.
        //
        // A cleared selection — index `-1`, handle 0 — falls straight out. The SCID comes
        // off that row's `0x10000010` property; see
        // [`crate::panels::spell_examine::row_component_scid`].
        if m.source_id == SPELL_COMPONENT_LIST && m.id.0 == SPELL_COMPONENT_LIST_MESSAGE {
            if m.p2 != 0 {
                if let Some(scid) = crate::panels::spell_examine::row_component_scid(
                    ui,
                    dereth_ui::ElemHandle::from_raw(m.p2),
                ) {
                    self.component_clicks += 1;
                    self.pending_component_scid = Some(scid);
                }
            }
            return false;
        }
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED || m.source_id != CLOSE_BUTTON {
            return false;
        }
        self.hide(ui)
    }

    /// The item-examine panel's inscription mouse presses handling's ordinary-player arms.
    ///
    /// The editable property is checked first and returns silently. A different scribe is named;
    /// a matching scribe on a locked item gets the inventory refusal. An unsigned object then
    /// distinguishes the not-inscribable bit, the missing/zero-location inventory refusal, and
    /// the non-zero-location silent return.
    ///
    /// The client's PSR override remains separate: once
    /// that privilege and its editability branch are projected, the editable first guard will
    /// suppress all three messages.
    fn handle_inscription_mouse_press(&self, requests_out: &mut crate::requests::Outbox) {
        if self.inscription_editable {
            return;
        }
        let text = if !self.scribe_name.is_empty() {
            if self.scribe_name.eq_ignore_ascii_case(&self.player_name) {
                "Item must be in your inventory to inscribe.".to_string()
            } else {
                format!("Only {} can change the inscription", self.scribe_name)
            }
        } else {
            match self.inscription_mouse_facts {
                Some((false, _)) => "This item is not inscribable.".to_string(),
                Some((true, location)) if location != 0 => return,
                Some((true, _)) | None => "Item must be in your inventory to inscribe.".to_string(),
            }
        };
        requests_out.emit(crate::view::UiRequest::DisplayChatText {
            channel: 0x1A,
            text,
        });
    }

    /// The inscription box gaining focus.
    ///
    /// With no scribe name the box goes to state 1 and is cleared. Either way the signature line is
    /// then set to `--` followed by the player's own name.
    ///
    /// So clicking in **empties the box** rather than dropping a caret into the literal
    /// `<Inscribe here>`, takes the centring off with it, and writes `--<your name>` underneath
    /// straight away — before anything has been sent — so the player can see whose signature he is
    /// about to leave. The signature is rewritten on **every** focus gain, not only the empty one.
    fn handle_inscription_gaining_focus(&mut self, ui: &mut UiSystem) {
        if self.scribe_name.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_NORMAL_STATE);
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text("");
                }
            }
            self.inscription = Some(String::new());
        }
        let signature = format!("--{}", self.player_name);
        if let Some(h) = self.inscription_signature {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&signature);
            }
        }
        self.inscription_signature_text = signature;
    }

    /// This is **the commit edge and the only client caller.**
    ///
    /// With no current object it returns. It reads the box's text and sends it as the inscription
    /// when it is worth saying — non-empty, or empty on an item that has a scribe — and differs
    /// from the cached inscription. Then an empty box goes back to the placeholder state and
    /// `<Inscribe here>`, the signature line is cleared and the scribe name forgotten; a non-empty
    /// one makes the player the scribe. Either way the cached inscription becomes the text.
    ///
    /// The two guards are worth naming because neither is obvious from the outside:
    ///
    /// * **an empty box on an unsigned item is not a message.** Opening the panel, clicking in
    ///   (which empties it) and clicking away again would otherwise send an empty inscription for
    ///   every inscribable thing the player ever looked at;
    /// * **the text has to have changed.** The cached inscription is the last thing the box
    ///   committed or was told, so losing focus twice with the same words sends once.
    ///
    /// The tail is what makes an inscription *look* persistent before the shard has answered: the
    /// scribe becomes the player, the cached inscription becomes the text, and the next
    /// inscription write for the same object — the 0.75 s combat re-poll included — draws them.
    fn handle_inscription_losing_focus(&mut self, ui: &mut UiSystem) {
        // The object is the *item* pane's current object; a creature or spell pane
        // has no inscription box at all.
        let Some(object) = self
            .sub_object
            .filter(|_| self.active == Some(ExamineSubUi::Item))
        else {
            return;
        };
        if object.0 == 0 {
            return;
        }
        let text = self
            .inscription_text
            .and_then(|h| ui.text_element_mut(h))
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default();
        let worth_saying = !(text.is_empty() && self.scribe_name.is_empty());
        // **The wart, and it is deliberate: it matches retail.** There is no
        // `text != PLACEHOLDER` here, and retail has none either: on the inscription write's
        // invitation leg the box holds `L"<Inscribe here>"` while the cached inscription is still
        // the client's `""`, so for an item that is inscribable, unsigned and **not** the player's,
        // the non-empty box passes the first test, `"<Inscribe here>" != ""` passes the second, and
        // the client sends the placeholder as an inscription for the new target, and the server
        // accepts it. Adding the missing test would change what goes on the wire.
        if worth_saying && text != self.inscription_value {
            ui.requests.emit(crate::view::UiRequest::SetInscription {
                object,
                text: text.clone(),
            });
            self.inscriptions_sent += 1;
        }
        if text.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_PLACEHOLDER_STATE);
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(PLACEHOLDER);
                }
            }
            if let Some(h) = self.inscription_signature {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text("");
                }
            }
            self.scribe_name.clear();
            self.inscription = Some(PLACEHOLDER.to_string());
            self.inscription_signature_text.clear();
        } else {
            self.scribe_name.clone_from(&self.player_name);
            self.inscription = Some(text.clone());
        }
        self.inscription_value = text;
    }

    /// The client's `0x1000002B` arm — **the third close path, and the
    /// one that makes the examine key a toggle.**
    ///
    /// Look up window `0x100005F7`. If it exists and is visible, hide it and return true
    /// without examining an object. A missing or hidden window takes the examine path.
    ///
    /// **It shares [`Self::hide`] with the close button and with the deselection path**, so all
    /// three increment `closed` and none of them touches `awaiting` or `current` — which is what
    /// keeps the combat re-poll from popping the window straight back open. See [`Self::hide`].
    ///
    /// The *decision* is not here: retail makes it in the action handler, before it reads
    /// the world selection, so this build makes it in
    /// `dereth_client::interaction`'s `0x1000002B` arm from a visibility answer pushed in by `App`.
    /// If it were made here, on the pull, the wire would be wrong — the examine-object path sends
    /// `0x00C8 Item_Appraise` on the way past, and retail's close-first leg sends nothing at all.
    ///
    /// Returns whether this call closed the window, i.e. whether it was open.
    pub fn close_from_action(&mut self, ui: &mut UiSystem) -> bool {
        self.hide(ui)
    }

    /// The hide all three close paths make, and the only place `closed` is counted.
    fn hide(&mut self, ui: &mut UiSystem) -> bool {
        let Some(w) = self.window else { return false };
        if !ui.node(w).is_some_and(|n| n.region.flags.visible) {
            return false;
        }
        ui.set_visible(w, false);
        self.closed += 1;
        true
    }

    /// The examination panel's element-message handler's `0x1000032D` arm — **the one
    /// producer of the newly-selected-item flag's clear.**
    ///
    /// The arm uses element `0x1000032D`, message **4**, and a non-null
    /// second parameter; the clicked row's `0x10000010` property is fetched and turned into a
    /// component number; the spell pane maps that number to an object id. If non-zero, clear the
    /// selection-reaction flag, select the object with second argument zero, and restore it.
    ///
    /// **Why the restore is not here.** Those three steps bracket a *synchronous*
    /// broadcast: selecting the object raises the selection-changed notice, and that notice's
    /// handler reads the flag while it is down. This build's reader is
    /// [`Self::update`]'s pull on the following frame, so restoring here would restore it before
    /// anything had looked and the panel would re-examine the component it had just selected —
    /// exactly the defect the bracket exists to prevent. `update` performs the restore, one
    /// statement after the reader, which is the same position relative to the broadcast.
    ///
    /// `0x1000032D` ([`SPELL_COMPONENT_LIST`]) is a real child of `<EXAM>` in the shipped layout,
    /// so the click is reachable. The id resolution — the spell-examine panel's
    /// component-object-id read, which turns the clicked row's `0x10000010` property into an
    /// object id — is the caller's; see `spell_component_selected`.
    ///
    /// Returns the `UiRequest::Select` to emit, or `None` for the arm's own zero-object guard.
    pub fn select_spell_component(
        &mut self,
        component: dereth_primitives::ObjectId,
    ) -> Option<crate::view::UiRequest> {
        if component.0 == 0 {
            return None;
        }
        self.examine_newly_selected_item = false;
        Some(crate::view::UiRequest::Select(component))
    }

    /// The complete client sequence around [`Self::select_spell_component`].
    ///
    /// A row with no SCID does nothing; an SCID the component tracker maps to no object does
    /// nothing at all; otherwise the flag is cleared, the object is selected (second argument
    /// zero), and the flag is restored.
    ///
    /// **Selecting the object is the whole of it.** There is no examine here, no
    /// use, no drag: the pane's rows are plain regions in a `ListBox`, not
    /// item tiles in an item list, so none of the client's arms is on this path. What
    /// the player sees is the component *selected* — and, because the panel is showing a spell
    /// rather than an item, the guard above is what stops the selection immediately re-examining
    /// it and replacing the spell in the window.
    ///
    /// Returns the request to emit, or `None` for either of the two early outs.
    fn spell_component_selected(
        &mut self,
        view: &dyn GameView,
        scid: u32,
    ) -> Option<crate::view::UiRequest> {
        let object = view.component_object_id(scid)?;
        let r = self.select_spell_component(object)?;
        self.component_selections += 1;
        Some(r)
    }

    /// **The other close path**, and
    /// the re-examine that shares its `if`.
    ///
    /// A hidden window or a cleared selection-reaction flag returns without action.
    /// Otherwise a nonzero selected object is examined; a zero selection hides the window.
    ///
    /// So: **the window closes when the selection is cleared, and re-examines when the selection
    /// moves to another object** — and neither happens while it is already hidden, which is the
    /// second reason a closed panel stays closed.
    ///
    /// The re-examine arm returns `UiRequest::Examine`, which is the examine-object call
    /// on this side of the seam: it reaches `examine_object` and comes back as the
    /// `examine_request` notice [`Self::update`] already pulls, so the *notice* half is not
    /// duplicated here. Returns `(closed, request)`.
    pub fn selection_changed(
        &mut self,
        ui: &mut UiSystem,
        selected: Option<dereth_primitives::ObjectId>,
    ) -> (bool, Option<crate::view::UiRequest>) {
        if !self.is_open(ui) || !self.examine_newly_selected_item {
            return (false, None);
        }
        match selected.filter(|id| id.0 != 0) {
            Some(id) => (false, Some(crate::view::UiRequest::Examine(id))),
            None => (self.hide(ui), None),
        }
    }

    /// The examination panel's examine-object notice.
    ///
    /// Id zero returns. Otherwise it raises the busy count if nothing was awaited, makes the id the
    /// awaited one, sends the appraise request and clears the current appraisal id.
    ///
    /// **It does not show the panel** — only the appraise-info write does, and only when the reply
    /// for this id arrives. The appraise-request half is the host's here: `UiRequest::Examine`
    /// carries it and reaches attempt_appraise, which is the same message.
    ///
    /// Clearing the current appraisal id is the line that makes the *second* examine of a different
    /// object re-open a closed panel: without it the old id would still match the guard and the
    /// reply would refill in place without showing.
    pub fn examine_object(&mut self, id: dereth_primitives::ObjectId) {
        if id.0 == 0 {
            return;
        }
        self.awaiting = Some(id);
        self.current = None;
    }

    /// The whole "right-click a spell" path and the operations it forwards to.
    ///
    /// An awaited appraisal is dropped, with its busy count. If there was an awaited or a current
    /// one, the current id is cleared and an appraise request for object `0` is sent — a
    /// **cancel**, not a request. Then the spell pane is filled, becomes the active pane if it was
    /// not, and the window is shown — **unconditionally**.
    ///
    /// Three things fall out of it that the appraisal path would have got wrong:
    ///
    /// * **The window is shown every time**, with no *new-reply* guard. A spell examine re-opens a
    ///   panel the player closed, because there is no reply to wait for — the pane is filled from
    ///   two dat tables before the window is shown, synchronously.
    /// * **Any appraisal in flight is cancelled**, and both id fields are cleared, so a `0x00C9`
    ///   that arrives afterwards fails the appraise-info write's guard and cannot overwrite the
    ///   spell. That cancel is the appraise request for object `0` — `0x00C8` with a **zero**
    ///   object id — and it is the only message this path can put on the wire.
    /// * **A spell the table does not know changes nothing at all.** The client's spell examine
    ///   returns 0 before its first write, and this build keeps that: the pane refuses, and the
    ///   window is not shown.
    ///
    /// Returns whether the window was shown by this call.
    pub fn examine_spell(&mut self, ui: &mut UiSystem, view: &dyn GameView, spell: u32) -> bool {
        // **The cancel, both halves.**
        //
        // With both ids zero the client goes straight to the fill and sends nothing.
        //
        // The **guard is the panel's**, which is why it is here and not in `dereth_client_model`: it asks
        // about `ExaminationPanel`'s own two ids, and a cancel sent unconditionally would put a
        // `0x00C8` on the wire for every spell right-click a player ever makes.
        //
        // `UiRequest::CancelAppraisal` and **not** `UiRequest::Examine(ObjectId(0))`: the zero
        // means the opposite thing in the other function. The client's examine-object
        // path reads a zero as *arm examine targeting* and never sends the request; the spell
        // examine calls the packer directly. Emitting the `Examine` spelling here would put the
        // pointer into examine mode on every spell right-click.
        //
        // Clearing both ids is the observable half and stays: it is what makes a `0x00C9` that
        // arrives after this call fail the appraise-info write's guard, so the spell stays on
        // screen instead of being overwritten by the item that was being appraised.
        if self.awaiting.is_some() || self.current.is_some() {
            ui.requests.emit(crate::view::UiRequest::CancelAppraisal);
            self.appraisals_cancelled += 1;
        }
        self.awaiting = None;
        self.current = None;
        let Some(v) = self.spell.examine_spell(ui, view, spell) else {
            return false;
        };
        // The parent's displayed-name text is written by the *sub*-UI here, not by
        // the title-text write, and there is **no stack-size suffix**: a spell is not an
        // object, so `title_text` here is the bare spell name.
        if let Some(h) = self.title {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&v.name);
            }
        }
        self.title_text = Some(v.name.clone());
        // The current object id belongs to the *item* pane and is not touched by this path;
        // clearing it here is what stops a later appraisal for the object that was showing before
        // from taking the appraise-info write's same-object path against a pane that has moved.
        self.sub_object = None;
        if self.active != Some(ExamineSubUi::Spell) {
            self.set_active_examine_ui(ui, ExamineSubUi::Spell);
        }
        let Some(w) = self.window else { return false };
        ui.set_visible(w, true);
        self.opened += 1;
        true
    }

    /// The appraise-info write; see the module header for what it does. Returns whether the window
    /// was shown by this call.
    ///
    /// `name` is the object's name and `stack_size` its stack size; `None` for the pair is the
    /// live-object check failing, which returns
    /// without writing anything.
    pub fn set_appraise_info(
        &mut self,
        ui: &mut UiSystem,
        id: dereth_primitives::ObjectId,
        p: &AppraisalView,
        weenie: Option<(&str, u32)>,
    ) -> bool {
        if id.0 == 0 || (self.awaiting != Some(id) && self.current != Some(id)) {
            return false;
        }
        let is_new = self.awaiting == Some(id);
        if is_new {
            self.awaiting = None;
            self.current = Some(id);
        }
        // The pane fork, as retail makes it: not a creature is the item pane; a creature carrying
        // either `Template` (PropertyString 5) or `CharacterTitleId` (PropertyInt 0x105) is a
        // *player* and takes the character pane; anything else is the creature pane.
        let sub = if p.creature {
            if p.template || p.character_title {
                ExamineSubUi::Char
            } else {
                ExamineSubUi::Creature
            }
        } else {
            ExamineSubUi::Item
        };
        let new_object = self.sub_object != Some(id);
        // The live-object check — above every write, so an appraisal for an
        // object this client does not have leaves the panel exactly as it was.
        let Some((name, stack_size)) = weenie else {
            return false;
        };
        self.replies_applied += 1;
        self.sub_object = Some(id);

        // The title is string `0x34` if present, else the name, with the stack size.
        // `GearPlatingName` overrides
        // the object's own name, which is how a plated weapon shows its plating.
        let display_name = p.gear_plating_name.as_deref().unwrap_or(name);
        let title = title_text(display_name, stack_size);
        if let Some(h) = self.title {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&title);
            }
        }
        self.title_text = Some(title);

        // The pane's own appraise-info write.
        //
        // The item-examine panel's appraise-info write runs the inscription write and then
        // twenty-five appraisal blocks in a fixed order; [`item_description`] is the six of them
        // this build draws, in that order, and [`ITEM_BLOCKS_NOT_IMPLEMENTED`] names the rest. The
        // creature and character panes use their own block lists plus the nine information regions
        // their constructors put into the list box.
        match sub {
            ExamineSubUi::Item => {
                self.set_inscription(ui, p);
                // **One per run, not one text write for the lot.** A single flat string would
                // draw every block in the element's plain colour, and the enchantment
                // highlighting would have nowhere to go. [`add_item_info`] has the separator rule;
                // the append passes font index `0`
                // and the block's **colour** index; see the `HIGHLIGHTED_PROPERTIES` section. The
                // glyph-count guard is the client's own and is why a fresh pane's first line starts
                // at the top.
                let runs = item_description_runs(p);
                // The description block ends with a direct text append of the augmentation
                // cost, not an add-item-info line: the shipped row owns its leading/trailing
                // newline. Its one variable is named "EXPERIENCE". Reuse the shared localized
                // integer/string services.
                let cost = p.augmentation_cost.map(|value| {
                    // The value is named. The row has exactly one variable and it is the hash of
                    // `"EXPERIENCE"` [measured], so the value goes in under that name rather than
                    // into whatever slot comes first.
                    let xp = super::numfmt::number(value);
                    super::characterinfo::compose_in(
                        ui,
                        super::statmgmt::STRING_TABLE,
                        "ID_Examine_Item_AugmentationCost",
                        &[("EXPERIENCE", xp.as_str())],
                    )
                });
                if let Some(h) = self.item_display_text {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text("");
                        for r in &runs {
                            if !t.glyphs.is_empty() {
                                t.append_text(if r.same_line { "\n" } else { "\n\n" });
                            }
                            t.append_text_with_font_and_color(&r.text, 0, r.color);
                        }
                        if let Some(cost) = &cost {
                            t.append_text(cost);
                        }
                    }
                }
                let mut text = flatten_item_info(&runs);
                if let Some(cost) = cost {
                    text.push_str(&cost);
                }
                self.item_text = Some(text);
            }
            ExamineSubUi::Creature | ExamineSubUi::Char => {
                self.set_creature_info(ui, p, sub, name);
            }
            ExamineSubUi::Spell => {}
        }
        let _ = new_object;

        if self.active != Some(sub) {
            self.set_active_examine_ui(ui, sub);
        }
        if is_new {
            if let Some(w) = self.window {
                ui.set_visible(w, true);
                self.opened += 1;
            }
        }
        is_new
    }

    /// The `<Inscribe here>` edit box.
    ///
    /// The client clears both text elements and the two cached strings first, then takes the box
    /// away entirely for an object that is not inscribable (it hides the *text element*, not the
    /// window). Only then does it read the profile. See [`inscription_text`] for the **four**-way
    /// fork.
    ///
    /// The caches the client keeps beside the two text
    /// elements (the scribe name and the inscription) are kept here too, because every
    /// later decision — may this player write, is the box showing the invitation, is what it now
    /// holds worth a message — is made against them and not against the profile.
    ///
    /// **How those two are written matters.** The string reads store them directly,
    /// so each is set the moment its `PropertyString` answers, and an
    /// item with a scribe and no inscription keeps the scribe. See the comment at the assignments.
    ///
    /// The **state** calls are the placeholder's centring: state `0x10000050` on the way in,
    /// state `1` on the
    /// leg that has a real inscription to show. See [`INSCRIPTION_PLACEHOLDER_STATE`].
    fn set_inscription(&mut self, ui: &mut UiSystem, p: &AppraisalView) {
        let text = inscription_text(
            p.inscribable,
            p.scribe_name.as_deref(),
            p.inscription.as_deref(),
        );
        let signature = inscription_signature(
            p.inscribable,
            p.scribe_name.as_deref(),
            p.inscription.as_deref(),
        );
        // The box goes to the placeholder state, and both text elements and both caches are
        // cleared.
        // Unconditional, and before the profile is looked at: the centred invitation is the
        // element's resting state and every other leg moves it off.
        if let Some(h) = self.inscription_text {
            ui.set_state(h, INSCRIPTION_PLACEHOLDER_STATE);
        }
        self.scribe_name.clear();
        self.inscription_value.clear();
        // A not-inscribable object hides the text and signature only. The editable-state write
        // keeps the background present and gives it the mouse on the locked leg, so the mouse-press
        // handler can explain the refusal.
        for h in [self.inscription_text, self.inscription_signature]
            .into_iter()
            .flatten()
        {
            ui.set_visible(h, text.is_some());
        }
        // The two caches are written by the string reads **directly**, one at a
        // time, and neither waits for the other: the scribe name (string `8`), then the
        // inscription (string `7`).
        //
        // Writing them as a pair on the "both present" leg would be wrong: an item with a scribe
        // and no text would leave the panel believing nobody had signed it, so the box would
        // invite an inscription the editable-state write would refuse to send. The
        // scribe's read runs first and unconditionally; the inscription's is reached only
        // when the scribe is non-empty, which is why the second is nested.
        if let Some(scribe) = p.scribe_name.as_deref() {
            self.scribe_name = scribe.to_string();
        }
        if !self.scribe_name.is_empty() {
            if let Some(written) = p.inscription.as_deref() {
                self.inscription_value = written.to_string();
            }
        }
        // On the "both strings present" leg, state `1` belongs to this path alone. A
        // signed-but-blank box skips it and keeps the centred state assigned earlier.
        if !self.scribe_name.is_empty() && !self.inscription_value.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_NORMAL_STATE);
            }
        }
        if let Some(h) = self.inscription_text {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text.as_deref().unwrap_or(""));
            }
        }
        if let Some(h) = self.inscription_signature {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&signature);
            }
        }
        self.inscription = text;
        self.inscription_signature_text = signature;
        // The editable-state write — the last line, on every
        // leg including the one that took the box away.
        self.set_inscription_editable_state(ui, p);
    }

    /// **Who is allowed to write on it.**
    ///
    /// The box's editable (`0x16`) and selectable (`0x27`) properties are written with one value.
    /// With no live object, or one whose inscribable bit (`0x2`) is clear, it is false. Otherwise the
    /// ordinary rule is that the box is editable when the item is unsigned or signed by this player
    /// (case-insensitive) **and** the player owns it; a PSR may edit regardless. The background is
    /// then made mouse-invisible on the editable leg and mouse-visible on the other.
    ///
    /// **The background takes the mouse only when the box does not**, which is what lets a click on
    /// `0x10000137` (element message `0x1C` with `p1 == 7`) be answered with *"Only %hs can change
    /// the inscription"*. That explanatory arm is implemented by
    /// [`Self::handle_inscription_mouse_press`]; its three literals include
    /// `"This item is not inscribable."`, all written to chat channel `0x1A`.
    ///
    /// The ordinary-player messages are implemented by
    /// [`Self::handle_inscription_mouse_press`]. The PSR test reads the same authoritative
    /// Boolean qualities as the client's, but only behind the live-object gate. The profile's
    /// `inscribable` field is deliberately not that gate: hook appraisal data may substitute it
    /// for display, while this function reads the current object's public-description bit 2.
    fn set_inscription_editable_state(&mut self, ui: &mut UiSystem, p: &AppraisalView) {
        // `stricmp` — the client's comparison on both legs is case-insensitive.
        let mine = !self.scribe_name.is_empty()
            && self.scribe_name.eq_ignore_ascii_case(&self.player_name);
        // Ask about the live current object and test its PWD bit 2
        // before either ownership or PSR. `None` is the missing-object lock; never fall back to
        // the hook-substituted appraisal field.
        let ordinary = (self.scribe_name.is_empty() || mine) && p.owned_by_player;
        let editable = inscription_editable_from_live_object(
            self.inscription_mouse_facts,
            ordinary,
            p.viewer_is_psr,
        );
        self.inscription_editable = editable;
        if let Some(h) = self.inscription_text {
            ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, editable);
            ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, editable);
            // The text element's attribute-set handler refreshes
            // the element's should-be-mouse-visible state after either setter; without this local
            // refresh a text element that just became locked continues to intercept the background
            // press.
            ui.update_mouse_visibility(h);
        }
        if let Some(h) = self.inscription_background {
            ui.set_mouse_visible(h, !editable);
        }
    }

    /// The basic creature examine panel's appraise-info write and
    /// the creature examine panel's appraise-info write's first block.
    ///
    /// With no creature block there is no level and no rows. Otherwise the level (int `0x19`) is
    /// written to the level value text, and each of the nine info regions updates from the profile.
    ///
    /// The nine tokens are six attribute info regions and three secondary-attribute info regions,
    /// built once by the constructor in the order [`CREATURE_ATTRIBUTE_ROWS`] then
    /// [`CREATURE_VITAL_ROWS`].
    ///
    /// **This build rebuilds the rows on every reply rather than updating nine live tokens.** The
    /// client's tokens are `static` — one set shared by every creature ever examined — and this
    /// panel has no equivalent lifetime; rebuilding produces the same nine rows with the same nine
    /// labels in the same order, which is what is observable. The *count* is asserted separately
    /// from the text ([`Self::creature_rows_drawn`]) so a list box with no row template is a
    /// number and not a blank pane.
    fn set_creature_info(
        &mut self,
        ui: &mut UiSystem,
        p: &AppraisalView,
        sub: ExamineSubUi,
        name: &str,
    ) {
        let level = level_value(p.level);
        if let Some(h) = self.level_value {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&level);
            }
        }
        self.level_text = Some(level);

        // The appraise-info write's int `2` -> the creature display name read. A miss
        // leaves the element alone, as in the client.
        if let Some(name) = p.creature_display_name.as_deref() {
            if let Some(h) = self.creature_name {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(name);
                }
            }
        }
        self.creature_name_text = p.creature_display_name.clone();

        let rows = creature_rows(p);
        // The info region's update does not write the value cell as plain text; it writes it
        // with font 0 and a colour, and the colour is `3` whenever the assess failed. Drawing
        // the value with `set_text` would leave a failed assess's `???` in the element's own
        // colour — white — where retail paints all nine.
        // The **label** cell keeps `set_text`: neither update touches the label text.
        let colors = creature_row_colors(p);
        self.creature_rows_drawn = 0;
        if let Some(mut list) = self.stat_list.take() {
            list.flush(ui);
            for ((label, value), color) in rows.iter().zip(colors.iter()) {
                let Some(h) = list.add_from_template(ui, 0, None) else {
                    continue;
                };
                if let Some(c) = ui.get_child_recursive(h, ROW_LABEL) {
                    if let Some(t) = ui.text_element_mut(c) {
                        t.set_text(label);
                    }
                }
                if let Some(c) = ui.get_child_recursive(h, ROW_VALUE) {
                    if let Some(t) = ui.text_element_mut(c) {
                        t.set_text("");
                        t.append_text_with_font_and_color(value, 0, *color);
                    }
                }
                self.creature_rows_drawn += 1;
            }
            list.update_layout(ui);
            self.stat_list = Some(list);
        }
        self.creature_row_text = rows;
        self.creature_row_colors = colors;

        // =========================================================================================
        // **The rest of both panes' appraise-info writes.**
        // =========================================================================================
        //
        // Everything above is the base creature-appraisal implementation, which both panes
        // call last. What follows is what the two overrides do **before** that call.

        // The text clear on the allegiance-name text —
        // unconditional in both panes, before either looks at the
        // profile. Without it the last player's allegiance would stay under the next creature.
        if let Some(h) = self.allegiance_name {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text("");
            }
        }
        self.allegiance_name_text = None;

        if sub == ExamineSubUi::Char {
            let write = |ui: &mut UiSystem, h: Option<ElemHandle>, text: &str| {
                if let Some(h) = h {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(text);
                    }
                }
            };
            // Write the element only when
            // the gender-and-heritage display read answered, so a miss leaves whatever was there —
            // which is why this is an `if let` and not a clear-then-set.
            if let Some(text) = p.gender_heritage_display.as_deref() {
                write(ui, self.char_heritage, text);
                self.heritage_text = Some(text.to_string());
            }
            // Use the title table first and the template only as its fallback.
            // `Hud::appraisal` resolves the fork because both halves are dat lookups.
            if let Some(text) = p.profession.as_deref() {
                write(ui, self.char_profession, text);
                self.profession_text = Some(text.to_string());
            }
            // Guarded on the current object being known, which is the same object
            // the live-object check above already required.
            let pk = pk_status_text(p.weenie_is_pk, p.weenie_is_pk_lite);
            write(ui, self.char_pk_status, pk);
            self.pk_status_text = Some(pk.to_string());
            // The allegiance full-name read over the title bar the examination panel's
            // appraise-info write had already filled: the allegiance title
            // for this rank/heritage/gender, a space, then the object's name — or the bare name
            // when the allegiance title read refuses. No stack count: a player is
            // never a stack.
            let full = match p.allegiance_title.as_deref() {
                Some(t) => format!("{t} {name}"),
                None => name.to_string(),
            };
            write(ui, self.title, &full);
            self.title_text = Some(full);
            // Set the allegiance name only when rank is at least one and the string is present.
            if p.allegiance_rank.unwrap_or(0) >= 1 {
                if let Some(text) = p.allegiance_name.as_deref() {
                    write(ui, self.allegiance_name, text);
                    self.allegiance_name_text = Some(text.to_string());
                }
            }
        }

        // The extra-info list runs first in both panes, followed by one row-helper call per
        // row.
        let misc = if sub == ExamineSubUi::Char {
            char_misc_rows(p)
        } else {
            creature_misc_rows(p)
        };
        self.misc_rows_drawn = 0;
        if let Some(mut list) = self.misc_list.take() {
            list.flush(ui);
            for row in &misc {
                let Some(h) = list.add_from_template(ui, 0, None) else {
                    continue;
                };
                for (id, text) in [(ROW_LABEL, &row.label), (ROW_VALUE, &row.value)] {
                    if let Some(c) = ui.get_child_recursive(h, id) {
                        if let Some(t) = ui.text_element_mut(c) {
                            // The coloured text write — font 0, the row's colour, and
                            // the **same** colour on both cells.
                            t.set_text("");
                            t.append_text_with_font_and_color(text, 0, row.color);
                        }
                    }
                }
                self.misc_rows_drawn += 1;
            }
            list.update_layout(ui);
            self.misc_list = Some(list);
        }
        self.misc_row_text = misc;
    }

    /// Hide the other three panes
    /// and show the chosen one, in that order.
    ///
    /// The creature and character examine panes share the base
    /// field `0x10000140` and differ only in the sub-panel, so showing one of them shows the base
    /// and its own sub-panel and hides the other's.
    fn set_active_examine_ui(&mut self, ui: &mut UiSystem, sub: ExamineSubUi) {
        self.active = Some(sub);
        let show = |ui: &mut UiSystem, h: Option<ElemHandle>, v: bool| {
            if let Some(h) = h {
                ui.set_visible(h, v);
            }
        };
        show(ui, self.item_base, sub == ExamineSubUi::Item);
        show(ui, self.spell_base, sub == ExamineSubUi::Spell);
        let creature_like = matches!(sub, ExamineSubUi::Creature | ExamineSubUi::Char);
        show(ui, self.creature_base, creature_like);
        show(ui, self.creature_sub, sub == ExamineSubUi::Creature);
        show(ui, self.char_sub, sub == ExamineSubUi::Char);
    }

    /// One frame's drive: if a reply has landed for the object this panel is waiting on, apply it.
    ///
    /// This is the pull half of a notice the client pushes. `set_appraise_info` raises
    /// `Notice::AppraisalReady`, which nothing consumed; rather than add a second notice bus, the
    /// panel asks the view for the profile of the id it is already waiting on, which it can only
    /// answer once `0x00C9` has landed. **The frame is the same one**: `Hud::drive` runs after
    /// `interaction::apply_events` in `App::frame`, so `set_appraise_info` happens on the frame the
    /// reply lands and not the frame after.
    ///
    /// Returns whether the panel changed.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let changed = self.pull(ui, view);
        // **The description pane's scrollbar.**
        //
        // The client re-measures, inside its own layout pass, every text element flagged as needing
        // it (`0x100`): it recalculates the glyphs, resizes the scrollable area to the text plus
        // its margins, and re-applies the scroll offset.
        //
        // Without it the item description text `0x1000013C` would report a content extent of
        // **0 x 0** however much text it held, so the scrollbar sizing would see
        // `content <= view`, set `0x1000013D` disabled, and its authored `0x79 hide-when-disabled`
        // would take the bar off the screen -- leaving the static plate `0x100005F8` behind it, a
        // darkened strip on the right side of the window instead of a scrollbar. The pane names
        // its bar (attribute `0x72 = 0x1000013D`) and binds it.
        //
        // There is no hand call here: setting text raises the `0x100` flag, and `UiSystem::draw`
        // consumes it from the root, which is where the client's per-frame region pass consumes
        // it too. Both panes in this subtree (`0x1000013C` and the inscription box `0x1000013E`)
        // are covered without either being named.
        changed
    }

    /// The pull itself. [`Self::update`] performs it and then re-measures the panes, matching
    /// the client's tail documented there.
    fn pull(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The examine-object notice first: the examine **cursor** and the
        // `0x2B` key raise it from `dereth_client::interaction`, which has no screen to call, so the
        // notice is pulled off the view here. In the client this is a push and this line does not
        // exist; the serial is what makes a repeat examine of the same object a new notice. See
        // [`crate::view::GameView::examine_request`].
        //
        // It runs *before* the reply is looked at, which is the client's order too: the notice sets
        // the awaited appraisal id and only then can the appraise-info write match it.
        // Selection changes are pulled the same way. This runs
        // **before** the examine notice for the client's own reason: its re-examine arm *is* what
        // raises that notice, so a selection change and the examine it produces must not be seen
        // in the other order. The first frame only records the selection -- a panel that has never
        // seen one has not had a *change*, and treating the initial `None` as a deselection would
        // close a window nothing had opened.
        // The player's own name —
        // read by the inscription gaining-focus handling, the losing-focus handling
        // and the inscription editable-state write, all three from a global the
        // client can reach from anywhere. Those three run from an element message that carries no
        // `GameView`, so the name is latched here, on the same pull that fills the panel.
        if let Some(n) = view.character_name() {
            if self.player_name != n {
                self.player_name = n.to_string();
            }
        }
        // The client's handler reads the current object id, which remains the displayed item
        // while a different appraisal is pending. Refresh that identity, not `awaiting`, so a
        // press during the wait cannot combine the old inscription with the new object's facts.
        if self.active == Some(ExamineSubUi::Item) {
            self.inscription_mouse_facts = self
                .sub_object
                .and_then(|id| view.inscription_mouse_facts(id));
        }
        let selected = view.selected_object();
        let mut changed = false;
        match self.last_selected {
            None => self.last_selected = Some(selected),
            Some(prev) if prev != selected => {
                self.last_selected = Some(selected);
                let (closed, request) = self.selection_changed(ui, selected);
                // Set the newly-selected-item flag unconditionally, immediately
                // after the selected-object write returns. In the client that is
                // after the broadcast has already run; here the broadcast is this pull, so the
                // restore is here rather than at the clear. Counted, so that "the guard suppressed
                // a re-examine" is a number and not a silence.
                if !self.examine_newly_selected_item {
                    self.examine_newly_selected_item = true;
                    self.self_selections_absorbed += 1;
                }
                changed |= closed;
                if let Some(r) = request {
                    ui.requests.emit(r);
                }
            }
            Some(_) => {}
        }
        // The formula list's click, resolved. It runs **after** the selection
        // reader above and not before, and the order is load-bearing: the clear it performs is
        // meant to be read by the *next* frame's reader, over the selection this request is about
        // to make. Resolving first would put the guard down in front of this frame's reader, which
        // is still looking at the old selection, and the restore would happen a change too early.
        if let Some(scid) = self.pending_component_scid.take() {
            if let Some(r) = self.spell_component_selected(view, scid) {
                ui.requests.emit(r);
            }
        }
        if let Some((id, serial)) = view.examine_request() {
            if serial != self.last_examine_serial {
                self.last_examine_serial = serial;
                self.examines_pulled += 1;
                self.examine_object(id);
            }
        }
        // Spell-examine notices are drained the same
        // way and for the same reason: its producer is the spellbook's or the spell bar's item
        // list, which live on `dereth_client::hud::Hud` and cannot reach this panel. Unlike the
        // object notice this one **fills and shows the panel immediately** — there is no reply to
        // wait for — so it runs after the object notice and before the appraisal is looked at, and
        // a spell examine on the same frame as an appraisal wins, which is the client's order too
        // (spell examination cancels the appraisal it arrives beside).
        for spell in ui.notice_inbox.take_examine_spell() {
            self.spell_examines_pulled += 1;
            changed |= self.examine_spell(ui, view, spell);
        }
        // The examination panel's update-spell-components notice:
        // it forwards straight to the spell pane's component update, with **no** visibility guard
        // and no test of the sub-UI in play, so a hidden `<EXAM>` re-marks its formula rows too.
        // The notice's component-tracker argument is not read by the listener, which is why
        // the pull side is a bare serial; see [`crate::view::GameView::component_serial`].
        //
        // It runs **after** the spell drain above so that a spell examined on the same frame as a
        // tracker change is marked once, over the rows it has just drawn, rather than over the
        // previous spell's.
        let serial = view.component_serial();
        if serial != self.last_component_serial {
            self.last_component_serial = serial;
            self.component_notices_pulled += 1;
            self.spell.update_components(ui, view);
        }
        // The client's guard is on the reply; here the pull has to ask about *some* id, and the two
        // it can be asked about are exactly the client's two.
        let Some(id) = self.awaiting.or(self.current) else {
            return changed;
        };
        let Some(p) = view.appraisal(id) else {
            return changed;
        };
        // A reply already applied is not a new reply. The client cannot ask this question because
        // it is called *by* the reply; this is the pull model's one difference, and it is a
        // **delivery serial** rather than a profile comparison precisely so that a combat re-poll
        // answering with a byte-identical profile still counts as a new reply — which is what the
        // client's appraise-info write sees and what refills the panel.
        if self.awaiting.is_none() && self.last_delivery == p.delivery {
            return changed;
        }
        let weenie = view.name(id).map(|n| {
            let s = view.slot_decoration(id).map_or(1, |d| d.stack_size);
            (n.to_string(), s)
        });
        // `set_appraise_info` changes the current object id only when the object is still present.
        // Carry the matching live snapshot across that same transition; the next pull refreshes it
        // again.
        if weenie.is_some() {
            self.inscription_mouse_facts = view.inscription_mouse_facts(id);
        }
        let applied =
            self.set_appraise_info(ui, id, &p, weenie.as_ref().map(|(n, s)| (n.as_str(), *s)));
        self.last_delivery = p.delivery;
        applied || changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inscription_privilege_still_requires_a_live_inscribable_object() {
        assert!(!inscription_editable_from_live_object(None, true, true));
        assert!(!inscription_editable_from_live_object(
            Some((false, 0)),
            true,
            true
        ));
        assert!(inscription_editable_from_live_object(
            Some((true, 0)),
            false,
            true
        ));
        assert!(inscription_editable_from_live_object(
            Some((true, 0)),
            true,
            false
        ));
        assert!(!inscription_editable_from_live_object(
            Some((true, 0)),
            false,
            false
        ));
    }

    #[test]
    fn decoration_presence_and_gem_plural_rules_match_the_native_branches() {
        assert_eq!(workmanship_adjective(-1), "Priceless", "unsigned clamp");
        assert_eq!(workmanship_adjective(0), "crafted");
        assert_eq!(pluralized_gem_name(0x26, "Ruby"), "Rubies");
        for material in [0x0B, 0x18, 0x1B, 0x1D, 0x20, 0x25, 0x28, 0x2E, 0x24, 0x2D] {
            assert_eq!(pluralized_gem_name(material, "gem"), "pieces of gem");
        }
        for material in [0x1A, 0x31] {
            assert_eq!(pluralized_gem_name(material, "gem"), "gemes");
        }
        assert_eq!(pluralized_gem_name(0x1C, "Lapis Lazuli"), "Lapis Lazuli");
        assert_eq!(pluralized_gem_name(0x21, "Opal"), "Opals");
        assert_eq!(
            pluralized_gem_name(u32::MAX, ""),
            "s",
            "lookup failure is ignored"
        );
        let mut p = AppraisalView {
            long_desc: Some("Steel sword Steel".into()),
            short_desc: Some("fallback".into()),
            description_material: Some("Steel".into()),
            ..Default::default()
        };
        assert_eq!(
            decorated_description(&p).as_deref(),
            Some("Steel sword Steel")
        );
        p.long_desc_decoration = Some(0);
        assert_eq!(decorated_description(&p).as_deref(), Some("Steel sword"));
        p.long_desc_decoration = Some(4);
        p.description_material = Some(String::new());
        p.description_gems = Some((0, "Rubies".into()));
        assert_eq!(
            decorated_description(&p).as_deref(),
            Some(" Steel sword Steel, set with 0 Rubies")
        );
        p.long_desc = None;
        assert_eq!(decorated_description(&p).as_deref(), Some("fallback"));
        p.long_desc = Some(String::new());
        p.long_desc_decoration = None;
        assert_eq!(
            decorated_description(&p).as_deref(),
            Some(""),
            "present-empty is not fallback"
        );
    }
    use dereth_primitives::ObjectId;

    /// The lockpick difficulty ladder is the client's own nine bands.
    #[test]
    fn the_lockpick_difficulty_ladder_is_the_clients_own_nine_bands() {
        // A negative answers **false**, and the caller formats only on an answer, so no line is
        // drawn at all.
        assert_eq!(lockpick_success_percent_to_string(-1), None);
        assert_eq!(lockpick_success_percent_to_string(i32::MIN), None);
        // Exactly zero, and nothing else, is "impossible".
        assert_eq!(lockpick_success_percent_to_string(0), Some("impossible"));
        for (lo, hi, word) in [
            (1, 4, "ridiculously difficult"), // below 5
            (5, 14, "extremely difficult"),   // below 15
            (15, 34, "quite difficult"),      // below 35
            (35, 49, "difficult"),            // below 50
            (50, 69, "challenging"),          // below 70
            (70, 84, "mildly challenging"),   // below 85
            (85, 94, "easy"),                 // below 95
        ] {
            assert_eq!(
                lockpick_success_percent_to_string(lo),
                Some(word),
                "{word} lower bound"
            );
            assert_eq!(
                lockpick_success_percent_to_string(hi),
                Some(word),
                "{word} upper bound"
            );
        }
        assert_eq!(lockpick_success_percent_to_string(95), Some("trivial"));
        assert_eq!(lockpick_success_percent_to_string(100), Some("trivial"));
    }

    /// The lock block takes the client's own four exits.
    #[test]
    fn the_lock_block_takes_the_clients_own_four_exits() {
        let with = |locked: Option<bool>, resist: Option<i32>, pct: Option<i32>| {
            lock_appraise_lines(&AppraisalView {
                locked,
                resist_lockpick: resist,
                lockpick_success_percent: pct,
                ..AppraisalView::default()
            })
        };
        // The corpus's own body: `long-solo-play`'s `0x00C9` for `0x77F03053`.
        assert_eq!(
            with(Some(true), Some(9999), Some(0)),
            vec![
                "Locked".to_string(),
                "The lock looks impossible to pick (Resistance 9999).".to_string(),
            ]
        );
        // `Unlocked` **returns** immediately, so a lockpick bonus beside it is not drawn.
        assert_eq!(
            with(Some(false), Some(25), Some(50)),
            vec!["Unlocked".to_string()]
        );
        // A locked item with no resistance draws only `Locked`.
        assert_eq!(
            with(Some(true), None, Some(50)),
            vec![
                "Locked".to_string(),
                "You can't tell how hard the lock is to pick.".to_string(),
            ]
        );
        // Locked with a resistance but no percent: `Locked` alone. The second line needs
        // int `0xAD`.
        assert_eq!(
            with(Some(true), Some(9999), None),
            vec!["Locked".to_string()]
        );
        // **No bool table at all** is not `Locked = 0`; it is the lockpick-bonus arm.
        assert_eq!(
            with(None, Some(12), None),
            vec!["Bonus to Lockpick Skill: +12".to_string()]
        );
        assert_eq!(
            with(None, Some(-12), None),
            vec!["Bonus to Lockpick Skill: -12".to_string()]
        );
        // The separate negative and positive branches leave zero with no line at all.
        assert!(with(None, Some(0), None).is_empty());
        assert!(with(None, None, None).is_empty());
        assert!(lock_appraise_lines(&AppraisalView {
            weenie_is_hook: true,
            locked: Some(true),
            resist_lockpick: Some(9999),
            lockpick_success_percent: Some(0),
            ..AppraisalView::default()
        })
        .is_empty());
    }

    /// Both appraisal block lists are empty when all their rows are implemented.
    #[test]
    fn the_unimplemented_block_list_is_the_calls_this_build_does_not_make() {
        assert_eq!(
            ITEM_BLOCKS_NOT_IMPLEMENTED.len(),
            0,
            "all item appraisal blocks are implemented"
        );
        assert_eq!(
            CREATURE_MISC_NOT_IMPLEMENTED.len(),
            0,
            "both creature appraisal callers populate the bound list box"
        );
    }

    /// The society row reads both factions.
    #[test]
    fn the_society_row_reads_both_factions() {
        let row = |bits: Option<i32>, mine: i32, ranks: [i32; 3]| {
            society_row(&AppraisalView {
                faction_bits: bits,
                viewer_faction_bits: mine,
                society_ranks: ranks,
                ..AppraisalView::default()
            })
        };
        assert_eq!(
            row(None, 0, [0; 3]),
            None,
            "int 0x119 missing draws no row at all"
        );
        // Present with none of the low three bits is the `???` arm.
        assert_eq!(row(Some(0x10), 0, [0; 3]).unwrap().value, "???");
        // The rank bands, on the Eldrytch Web slot.
        assert_eq!(row(Some(2), 0, [0, 0, 0]).unwrap().value, "Eldrytch Web");
        assert_eq!(
            row(Some(2), 0, [0, 1, 0]).unwrap().value,
            "Eldrytch Web ~ Initiate"
        );
        assert_eq!(
            row(Some(2), 0, [0, 100, 0]).unwrap().value,
            "Eldrytch Web ~ Initiate"
        );
        assert_eq!(
            row(Some(2), 0, [0, 101, 0]).unwrap().value,
            "Eldrytch Web ~ Adept"
        );
        assert_eq!(
            row(Some(2), 0, [0, 301, 0]).unwrap().value,
            "Eldrytch Web ~ Knight"
        );
        assert_eq!(
            row(Some(2), 0, [0, 601, 0]).unwrap().value,
            "Eldrytch Web ~ Lord"
        );
        assert_eq!(
            row(Some(2), 0, [0, 1500, 0]).unwrap().value,
            "Eldrytch Web ~ Master"
        );
        assert_eq!(
            row(Some(2), 0, [0, 1501, 0]).unwrap().value,
            "Eldrytch Web",
            "past the last band"
        );
        // The colour is the viewer's own faction against the subject's.
        assert_eq!(
            row(Some(1), 0, [0; 3]).unwrap().color,
            0,
            "a viewer with no society"
        );
        assert_eq!(
            row(Some(1), 1, [0; 3]).unwrap().color,
            MOD_HIGH_FONT,
            "the same society"
        );
        assert_eq!(
            row(Some(1), 2, [0; 3]).unwrap().color,
            MOD_LOW_FONT,
            "a rival"
        );
        assert_eq!(
            row(Some(1), 4, [0; 3]).unwrap().color,
            MOD_LOW_FONT,
            "the other rival"
        );
        assert_eq!(
            row(Some(4), 3, [0; 3]).unwrap().color,
            MOD_LOW_FONT,
            "the other two bits"
        );
    }

    /// The allegiance rows fork on the two titles.
    #[test]
    fn the_allegiance_rows_fork_on_the_two_titles() {
        let rows = |rank: Option<i32>, m: Option<&str>, p2: Option<&str>, f: Option<i32>| {
            allegiance_rows(&AppraisalView {
                allegiance_rank: rank,
                monarch_title: m.map(ToString::to_string),
                patron_title: p2.map(ToString::to_string),
                allegiance_followers: f,
                ..AppraisalView::default()
            })
        };
        assert!(
            rows(None, Some("Baron Bob"), None, None).is_empty(),
            "rank < 1 draws nothing"
        );
        assert!(rows(Some(0), Some("Baron Bob"), None, None).is_empty());
        let one = rows(Some(3), Some("Baron Bob"), Some("Baron Bob"), None);
        assert_eq!(one.len(), 1);
        assert_eq!(
            (one[0].label.as_str(), one[0].value.as_str()),
            ("Monarch/Patron:", "Baron Bob")
        );
        let two = rows(Some(3), Some("High King Al"), Some("Baron Bob"), None);
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].label, "Monarch:");
        assert_eq!(two[1].label, "Patron:");
        let alone = rows(Some(3), Some("High King Al"), None, None);
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].label, "Monarch:");
        // The monarch's own view: no titles, a follower count, singular and plural.
        assert_eq!(rows(Some(9), None, None, Some(1))[0].value, "1 Follower");
        assert_eq!(rows(Some(9), None, None, Some(2))[0].value, "2 Followers");
        assert_eq!(
            rows(Some(9), None, None, None)[0].value,
            "0 Followers",
            "absent is zero"
        );
        assert_eq!(
            rows(Some(9), None, None, Some(-4))[0].value,
            "0 Followers",
            "negative is zero"
        );
    }

    /// The armour rows carry the unenchantable star.
    #[test]
    fn the_armour_rows_carry_the_unenchantable_star() {
        assert!(
            armor_rows(&AppraisalView::default()).is_empty(),
            "no 0x4000 block, no rows"
        );
        assert!(
            armor_rows(&AppraisalView {
                base_armor: Some([0; 9]),
                ..AppraisalView::default()
            })
            .is_empty(),
            "the nine-part guard needs one part above zero"
        );
        let rows = armor_rows(&AppraisalView {
            base_armor: Some([170, 150, 150, 150, 150, 170, 150, 150, 10_170]),
            ..AppraisalView::default()
        });
        assert_eq!(rows.len(), 4, "a blank and three rows");
        assert_eq!(rows[0], MiscRow::default(), "the blank");
        assert_eq!(
            (rows[1].label.as_str(), rows[1].value.as_str()),
            ("Head/Chest/Groin", "AL: 170/150/150")
        );
        assert_eq!(
            (rows[2].label.as_str(), rows[2].value.as_str()),
            ("Bicep/Wrist/Hand", "AL: 150/150/170")
        );
        assert_eq!(
            (rows[3].label.as_str(), rows[3].value.as_str()),
            ("Thigh/Shin/Foot", "AL: 150/150/*171"),
            "10170 - 9999 = 171, drawn with a star"
        );
        assert_eq!(armor_level_text(9999), "*0", ">= 9999 and not > 9999");
        assert_eq!(armor_level_text(9998), "9998");
    }

    /// The rating rows print rating and resist without the percent.
    #[test]
    fn the_rating_rows_print_rating_and_resist_without_the_percent() {
        assert!(rating_rows(&AppraisalView::default(), &CHARACTER_RATING_ROWS).is_empty());
        // The first nine slots; the four newer ones stay absent.
        let rows = |r: [Option<i32>; 9]| {
            let mut ratings = [None; 13];
            ratings[..9].copy_from_slice(&r);
            rating_rows(
                &AppraisalView {
                    ratings,
                    ..AppraisalView::default()
                },
                &CHARACTER_RATING_ROWS,
            )
        };
        // Group 1 alone, opened by `CritRating 0x139` — which is a guard and is never printed.
        let g1 = rows([None, None, Some(7), None, None, None, None, None, None]);
        assert_eq!(g1.len(), 3, "a blank, the row, and the trailing blank");
        assert_eq!(g1[0], MiscRow::default());
        assert_eq!(
            (g1[1].label.as_str(), g1[1].value.as_str()),
            ("Dmg/CritDmg", "Rating: 0/0")
        );
        assert_eq!(g1[2], MiscRow::default());
        // Group 2 prints `%Resist` under the same label, from 0x134 and 0x13C.
        let g2 = rows([None, Some(4), None, None, None, Some(9), None, None, None]);
        assert_eq!(
            (g2[1].label.as_str(), g2[1].value.as_str()),
            ("Dmg/CritDmg", "Resist: 4/9")
        );
        // Group 3's label is the only one with its own colon.
        let g3 = rows([None, None, None, None, None, None, None, Some(2), Some(3)]);
        assert_eq!(
            (g3[1].label.as_str(), g3[1].value.as_str()),
            ("DoT/Life:", "Resist: 2/3")
        );
        // All three together: one leading blank, three rows, one trailing blank.
        let all = rows([
            Some(1),
            Some(2),
            None,
            Some(3),
            None,
            Some(4),
            Some(99),
            Some(5),
            Some(6),
        ]);
        assert_eq!(all.len(), 5);
        assert_eq!(all[1].value, "Rating: 1/3");
        assert_eq!(all[2].value, "Resist: 2/4");
        assert_eq!(all[3].value, "Resist: 5/6");
        // `HealingBoostRating 0x143` is read by both panes and drawn by neither.
        assert!(
            all.iter().all(|r| !r.value.contains("99")),
            "slot 6 is dead in retail"
        );
    }

    /// The player-killer and overpower groups: each opens on either of its two slots, the
    /// overpower pair prints signed, and the two panes put them in opposite orders, both ahead of
    /// DoT/Life.
    #[test]
    fn the_pk_and_overpower_groups_draw_in_each_panes_own_order() {
        let mut ratings = [None; 13];
        ratings[9] = Some(12); // 0x17D PKDamageRating
        ratings[10] = Some(5); // 0x17E PKDamageResistRating
        ratings[11] = Some(20); // 0x182 Overpower
        ratings[12] = Some(8); // 0x183 OverpowerResist
        ratings[7] = Some(2); // 0x15E DotResistRating
        let p = AppraisalView {
            ratings,
            ..AppraisalView::default()
        };
        let pairs = |rows: Vec<MiscRow>| -> Vec<(String, String)> {
            rows.into_iter().map(|r| (r.label, r.value)).collect()
        };
        let s = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(
            pairs(rating_rows(&p, &CHARACTER_RATING_ROWS)),
            vec![
                s("", ""),
                s("PK Dmg/Res", "Rating: 12/5"),
                s("Overpower %", "+20/-8"),
                s("DoT/Life:", "Resist: 2/0"),
                s("", ""),
            ]
        );
        assert_eq!(
            pairs(creature_misc_rows(&p)),
            vec![
                s("", ""),
                s("Overpower %", "+20/-8"),
                s("PK Dmg/Res", "Rating: 12/5"),
                s("DoT/Life:", "Resist: 2/0"),
                s("", ""),
            ]
        );
        // Either slot opens its group; the other prints as zero.
        let mut only = [None; 13];
        only[12] = Some(3);
        only[10] = Some(4);
        let p = AppraisalView {
            ratings: only,
            ..AppraisalView::default()
        };
        assert_eq!(
            pairs(rating_rows(&p, &CHARACTER_RATING_ROWS)),
            vec![
                s("", ""),
                s("PK Dmg/Res", "Rating: 0/4"),
                s("Overpower %", "+0/-3"),
                s("", ""),
            ]
        );
        // A creature with none of the thirteen draws no rating block at all.
        assert!(creature_misc_rows(&AppraisalView::default()).is_empty());
    }

    /// The character pane's enlightenment line: after the titles line, before the footnote, and
    /// drawn for any value the profile carries, zero included; absent draws nothing.
    #[test]
    fn the_enlightenment_line_draws_whenever_the_property_is_present() {
        let rows = |e: Option<i32>| -> Vec<(String, String)> {
            char_misc_rows(&AppraisalView {
                num_character_titles: Some(3),
                enlightenment: e,
                ..AppraisalView::default()
            })
            .into_iter()
            .map(|r| (r.label, r.value))
            .collect()
        };
        let s = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(
            rows(Some(2)),
            vec![
                s("Titles Earned:", "3"),
                s("Enlightenment:", "2"),
                s(UNENCHANTABLE_FOOTNOTE, ""),
            ]
        );
        assert_eq!(rows(Some(0))[1], s("Enlightenment:", "0"));
        assert_eq!(
            rows(None),
            vec![s("Titles Earned:", "3"), s(UNENCHANTABLE_FOOTNOTE, "")]
        );
    }

    /// The item ratings sentence with every term positive: thirteen terms, the overpower and
    /// player-killer four between "Crit Dam Resist" and "Heal Boost", then the vitality line.
    #[test]
    fn the_item_ratings_sentence_carries_all_thirteen_terms_in_order() {
        let mut gear_ratings = [None; 14];
        for (i, slot) in gear_ratings.iter_mut().enumerate() {
            *slot = Some(i32::try_from(i).unwrap() + 1);
        }
        let lines = ratings_lines(&AppraisalView {
            gear_ratings,
            ..AppraisalView::default()
        });
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            text,
            vec![
                "Ratings: Dam 1, Dam Resist 2, Crit 3, Crit Dam 4, Crit Resist 5, \
                 Crit Dam Resist 6, Overpower% 7, Overpower Reduction% 8, PK Dam 9, \
                 PK Dam Resist 10, Heal Boost 11, Nether Resist 12, Life Resist 13.",
                "This item adds 14 Vitality.",
                "",
            ]
        );
        // The four newest terms alone, and a zero or negative term is left out.
        let mut only = [None; 14];
        only[6] = Some(4); // Overpower%
        only[8] = Some(0); // PK Dam
        only[9] = Some(-2); // PK Dam Resist
        let lines = ratings_lines(&AppraisalView {
            gear_ratings: only,
            ..AppraisalView::default()
        });
        assert_eq!(lines[0].text, "Ratings: Overpower% 4.");
        assert_eq!(
            GEAR_RATING_ROWS.map(|(k, _)| k),
            [
                0x172, 0x173, 0x174, 0x176, 0x175, 0x177, 0x184, 0x185, 0x17F, 0x180, 0x178, 0x179,
                0x17A
            ]
        );
    }

    /// The character pane draws its rows in the documented order.
    #[test]
    fn the_character_pane_draws_its_rows_in_the_listings_order() {
        // `early-inventory-and-casting`'s recorded `0x50000003`: a `0x4000` block and nothing else
        // the pane reads.
        let recorded = AppraisalView {
            base_armor: Some([170, 150, 150, 150, 150, 170, 150, 150, 170]),
            ..AppraisalView::default()
        };
        let rows = char_misc_rows(&recorded);
        assert_eq!(rows.len(), 5, "a blank, three armour rows and the footnote");
        assert_eq!(rows[4].label, UNENCHANTABLE_FOOTNOTE);
        assert_eq!(rows[4].value, "", "the value cell is the empty literal");
        // The footnote draws even for a player with nothing at all.
        assert_eq!(char_misc_rows(&AppraisalView::default()).len(), 1);
        // And the tail, one line at a time.
        let full = char_misc_rows(&AppraisalView {
            fellowship: Some("The Lost Light".to_string()),
            date_of_birth: Some("4/13/2026 7:14:52 PM".to_string()),
            age: Some(3_723),
            chess_rank: Some(1_600),
            fishing_skill: Some(42),
            num_deaths: Some(0),
            num_character_titles: Some(17),
            ..AppraisalView::default()
        });
        let pairs: Vec<(&str, &str)> = full
            .iter()
            .map(|r| (r.label.as_str(), r.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("Fellowship:", "The Lost Light"),
                ("Arrived in Dereth:", "4/13/2026 7:14:52 PM"),
                ("Time in Dereth:", "1h 2m 3s"),
                ("Chess Rank:", "1600"),
                ("Fishing Skill:", "42"),
                ("Deaths:", "Has never died"),
                ("Titles Earned:", "17"),
                (UNENCHANTABLE_FOOTNOTE, ""),
            ]
        );
        assert_eq!(
            char_misc_rows(&AppraisalView {
                num_deaths: Some(4),
                ..AppraisalView::default()
            })[0]
                .value,
            "4"
        );
    }

    /// Every implemented appraisal block reaches the pane.
    #[test]
    fn every_landed_block_reaches_the_pane() {
        use crate::view::{ItemLevelView, WieldRequirementView};
        let p = AppraisalView {
            success: true,
            num_times_tinkered: Some(3),
            workmanship: Some(7),
            equipment_set_id: Some(0x0D),
            gear_ratings: [
                Some(5),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(20),
            ],
            defense_mods: [Some(1.08), None, None],
            mana_conversion_mod: Some(0.05),
            level_limits: (Some(20), Some(40)),
            has_allowed_wielder: true,
            craftsman_name: Some("Asheron".to_string()),
            wield_requirements: vec![WieldRequirementView {
                requirement: 7,
                difficulty: 150,
                subject: Some("level".to_string()),
            }],
            use_requires_level: Some(30),
            item_level: Some(ItemLevelView {
                total_xp: 3_000,
                base_xp: 1_000,
                max_level: 5,
                xp_style: 1,
            }),
            item_difficulty: Some(250),
            boost_value: Some(50),
            booster_enum: Some(2),
            items_capacity: 24,
            stored_mana: Some(600),
            num_keys: Some(1),
            rare_id: Some(12),
            ..AppraisalView::default()
        };
        let text = item_description(&p);
        for wanted in [
            "This item has been tinkered 3 times.",
            "Workmanship: Flawless (7)",
            "Set: Soldier's",
            "Ratings: Dam 5.",
            "This item adds 20 Vitality.",
            // One decimal and a sign.
            "Bonus to Melee Defense: +8.0%.",
            "Restricted to characters of Levels 20 to 40.",
            "Wield requires Asheron",
            "Wield requires level 150",
            "Use requires level 30.",
            "Item Level: 3 / 5",
            "Item XP: 3,000 / 4,000",
            "Activation requires Arcane Lore: 250",
            // `modifier_to_string` on `0.05 + 1.0`, the block's own addition.
            "Bonus to Mana Conversion: +5%.",
            "Restores 50 Health when used.",
            "Can hold up to 24 items.",
            "Stored Mana: 600",
            "Contains 1 key.",
            "Rare #12",
        ] {
            assert!(
                text.contains(wanted),
                "{wanted:?} is missing from the pane -- its block is transcribed and unwired\n\
                 {text}"
            );
        }
        // `craftsman_lines` is the one block whose *absence* is the assertion: the allowed-wielder
        // line already spent the name.
        assert!(
            !text.contains("Created by Asheron."),
            "the craftsman block returns when bool 0x55 answered non-zero"
        );
        // And the heal-kit block is the mirror of the boost block: neither draws for the other's
        // object, so a plain potion gets `Restores` and no `Bonus to Healing Skill`.
        assert!(
            !text.contains("Bonus to Healing Skill"),
            "not a healing kit"
        );
    }

    /// The special properties block names what it does not draw.
    #[test]
    fn the_special_properties_block_names_what_it_does_not_draw() {
        assert_eq!(
            SPECIAL_PROPERTIES_NOT_IMPLEMENTED.len(),
            0,
            "the duration and shared-cooldown pair now have production readers"
        );
    }

    #[test]
    fn cooldown_gates_and_unsigned_time_words_match_native() {
        assert_eq!(appraisal_cooldown_text(90.9), "1m 30s");
        assert_eq!(appraisal_cooldown_text(3600.9), "1h 0s");
        assert_eq!(appraisal_cooldown_text(0.0), "0s");
        assert_eq!(appraisal_cooldown_text(4_294_967_297.0), "1s");
        assert_eq!(appraisal_cooldown_text(-1.9), "1657mo 6h 28m 15s");
        assert_eq!(appraisal_cooldown_text(f64::NAN), "0s");
        let mut p = AppraisalView::default();
        let baseline = special_properties_lines(&p);
        p.special.cooldown_group = Some(0);
        p.special.cooldown_remaining = Some(5.9);
        assert_eq!(
            special_properties_lines(&p),
            baseline,
            "duration absence gates the pair"
        );
        p.special.cooldown_duration = Some(0.0);
        assert_eq!(
            special_properties_lines(&p),
            vec![
                (String::new(), true),
                ("Cooldown When Used: 0s".into(), true),
                ("Cooldown Remaining: 5s".into(), true),
                (String::new(), true),
            ]
        );
        p.special.cooldown_group = None;
        assert_eq!(
            special_properties_lines(&p).len(),
            2,
            "no group means no remaining/blank"
        );
    }

    /// The special properties block is the client's own order.
    #[test]
    fn the_special_properties_block_is_the_clients_own_order() {
        use crate::view::SpecialPropertiesView;
        let view = |s: SpecialPropertiesView| {
            special_properties_lines(&AppraisalView {
                special: s,
                ..AppraisalView::default()
            })
        };

        // An empty profile still gets the client's unconditional blank, and **only** that: `drew`
        // stays 0 so the client skips the trailing one.
        assert_eq!(
            view(SpecialPropertiesView::default()),
            vec![(String::new(), true)]
        );

        // The recorded `long-solo-play` idx 6683 shape: Attuned = 1, Bonded = 1.
        assert_eq!(
            view(SpecialPropertiesView {
                attuned: Some(1),
                bonded: Some(1),
                ..SpecialPropertiesView::default()
            }),
            vec![
                (String::new(), true),
                ("Properties: Attuned, Bonded".to_string(), true),
                (String::new(), true),
            ]
        );

        // The attuned-status string is `0 < v && v < 3`, so Sticky reads the same word
        // and Normal draws nothing; the bonded-status string has three arms and 0 and 2
        // are not among them.
        assert_eq!(attuned_status_to_string(0), None);
        assert_eq!(attuned_status_to_string(1), Some("Attuned"));
        assert_eq!(attuned_status_to_string(2), Some("Attuned"));
        assert_eq!(attuned_status_to_string(3), None);
        assert_eq!(bonded_status_to_string(-2), Some("Destroyed on Death"));
        assert_eq!(bonded_status_to_string(-1), Some("Dropped on Death"));
        assert_eq!(bonded_status_to_string(0), None);
        assert_eq!(bonded_status_to_string(1), Some("Bonded"));
        assert_eq!(bonded_status_to_string(2), None);

        // Every entry at once, which pins the order the fifteen imbue bits and the eleven other
        // tests are appended in.
        let all = view(SpecialPropertiesView {
            unique_limit: Some(12_345),
            cooldown_duration: None,
            cooldown_group: None,
            cooldown_remaining: None,
            cleave: Some(3),
            slayer: Some((7, "Undead".to_string())),
            weapon_skill: Some(0x2000),
            imbued: Some(0x8000_FFFF),
            absorb_magic_damage: true,
            item_spellcraft: Some(9999),
            attuned: Some(2),
            bonded: Some(-1),
            retained: Some(true),
            critical_multiplier: true,
            critical_frequency: true,
            ignore_armor: true,
            resistance_cleaving: Some(0x02),
            proc_spell: true,
            ivoryable: Some(true),
            dyeable: Some(true),
            tethered_left: Some(true),
        });
        assert_eq!(
            all,
            vec![
                (String::new(), true),
                (String::new(), true),
                (
                    "You can only carry 12,345 of these items.".to_string(),
                    true
                ),
                ("Cleave: 3 enemies in front arc.".to_string(), true),
                (String::new(), true),
                (
                    "Properties: Undead slayer, Multi-Strike, Critical Strike, Crippling Blow, \
                     Armor Rending, Slash Rending, Pierce Rending, Bludgeon Rending, \
                     Acid Rending, Nether Rending, Cold Rending, Lightning Rending, \
                     Fire Rending, +1 Melee Defense, +1 Missile Defense, +1 Magic Defense, \
                     Phantasmal, Magic Absorbing, Unenchantable, Attuned, Dropped on Death, \
                     Retained, Crushing Blow, Biting Strike, Armor Cleaving, \
                     Resistance Cleaving: Piercing, Cast on Strike, Ivoryable, Dyeable"
                        .to_string(),
                    true
                ),
                ("This item cannot be further imbued.".to_string(), true),
                ("This item is tethered to the left side.".to_string(), true),
                (String::new(), true),
            ]
        );

        // `0x1F` is the one slayer id with a name of its own, and it takes no suffix.
        let bz = view(SpecialPropertiesView {
            slayer: Some((0x1F, "Shadow".to_string())),
            ..SpecialPropertiesView::default()
        });
        assert_eq!(bz[1].0, "Properties: Bael'Zharon's Hate");

        // The three thresholds, on both sides.
        let one = |s: SpecialPropertiesView| view(s)[1].0.clone();
        assert_eq!(
            one(SpecialPropertiesView {
                item_spellcraft: Some(9999),
                ..Default::default()
            }),
            "Properties: Unenchantable"
        );
        assert_eq!(
            view(SpecialPropertiesView {
                item_spellcraft: Some(9998),
                ..Default::default()
            }),
            vec![(String::new(), true)],
            "0x270E itself is not above 0x270E"
        );
        assert_eq!(
            one(SpecialPropertiesView {
                weapon_skill: Some(0x2000),
                ..Default::default()
            }),
            "Properties: Multi-Strike"
        );
        assert_eq!(
            view(SpecialPropertiesView {
                weapon_skill: Some(0x0010),
                ..Default::default()
            }),
            vec![(String::new(), true)],
            "the mask is 0x79E0 and nothing outside it draws the line"
        );
        // `1 < n`, so a one-enemy cleave draws nothing at all.
        assert_eq!(
            view(SpecialPropertiesView {
                cleave: Some(1),
                ..Default::default()
            }),
            vec![(String::new(), true)]
        );
        // An imbue mask of **zero** is a present-but-empty set: no names, and no sentence.
        assert_eq!(
            view(SpecialPropertiesView {
                imbued: Some(0),
                ..Default::default()
            }),
            vec![(String::new(), true)]
        );
    }

    /// Oracle: the client's comma insertion, and the value and burden blocks' two absent
    /// strings, which are **different** literals.
    #[test]
    fn the_value_and_burden_lines_are_the_clients_own_two_strings() {
        assert_eq!(insert_commas(0), "0");
        assert_eq!(insert_commas(999), "999");
        assert_eq!(insert_commas(1000), "1,000");
        assert_eq!(insert_commas(1_234_567), "1,234,567");
        assert_eq!(insert_commas(-1000), "-1,000");
        assert_eq!(value_line(None), "Value: ???");
        assert_eq!(value_line(Some(12_500)), "Value: 12,500");
        assert_eq!(burden_line(None), "Burden: Unknown");
        assert_eq!(burden_line(Some(1200)), "Burden: 1,200");
    }

    /// Oracle: the client's `stackSize < 2` fork, and the client's
    /// `L"%d %s"` for the ordering.
    #[test]
    fn a_stack_puts_its_count_before_the_name_and_a_single_item_does_not() {
        assert_eq!(
            title_text("Arrow", 0),
            "Arrow",
            "0 and 1 are the same branch"
        );
        assert_eq!(title_text("Arrow", 1), "Arrow");
        assert_eq!(title_text("Arrow", 2), "2 Arrow");
        assert_eq!(title_text("Arrow", 100), "100 Arrow");
    }

    /// The first two lines are driven
    /// **directly**, because the per-frame pull cannot reach them.
    ///
    /// **This test exists because the mutation that drops that guard survived the driven suite.**
    /// [`ExaminationPanel::update`] only ever asks the view about `awaiting.or(current)`, so
    /// through the pull the inner guard is unfalsifiable: removing it changes nothing an
    /// application-level test can see. The guard is still the client's own and is kept (a direct
    /// caller — the notice route this build does not have — would need it), and this is what makes
    /// it falsifiable. The stated testability rule, the third reading of a survivor.
    #[test]
    fn a_reply_for_an_id_the_panel_is_not_waiting_on_writes_nothing() {
        let mut ui = UiSystem::new((800, 600));
        let p = AppraisalView {
            value: Some(5),
            burden: Some(10),
            ..AppraisalView::default()
        };
        let mut panel = ExaminationPanel::default();

        // Nothing awaited and nothing current: refused.
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0x1234), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 0);
        assert_eq!(panel.title_text, None);

        // Awaiting a *different* object: still refused.
        panel.examine_object(ObjectId(0x5678));
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0x1234), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 0);

        // The one it asked for: applied.
        assert!(panel.set_appraise_info(&mut ui, ObjectId(0x5678), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 1);
        assert_eq!(panel.title_text.as_deref(), Some("Sack"));
        assert_eq!(panel.current, Some(ObjectId(0x5678)));
        assert_eq!(panel.awaiting, None);

        // Id 0 is refused before anything else, even when it is the current one.
        panel.current = Some(ObjectId(0));
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 1);
    }

    /// Oracle: the pane fork — string `5` **or**
    /// int `0x105`, two separate tests leading to the same pane.
    ///
    /// **This test exists because the `||` -> `&&` mutation survived the corpus sweep**, and the
    /// reason is a fact about the corpus rather than about the code: its **one** character
    /// appraisal (`+Aldwyne`, `early-inventory-and-casting` `0x50000003`) carries **both**
    /// properties, so no recorded body can tell the two operators apart. Asserted here over the two
    /// one-sided cases, which is legitimate because the fork is a pure function of two booleans and
    /// nothing is being synthesised about the wire.
    #[test]
    fn either_a_template_or_a_character_title_sends_a_creature_to_the_character_pane() {
        let mut ui = UiSystem::new((800, 600));
        let mut pane = |creature: bool, template: bool, character_title: bool| {
            let p = AppraisalView {
                creature,
                template,
                character_title,
                ..AppraisalView::default()
            };
            let mut panel = ExaminationPanel::default();
            panel.examine_object(ObjectId(1));
            assert!(panel.set_appraise_info(&mut ui, ObjectId(1), &p, Some(("x", 1))));
            panel.active
        };
        assert_eq!(
            pane(false, false, false),
            Some(ExamineSubUi::Item),
            "not a creature"
        );
        assert_eq!(
            pane(false, true, true),
            Some(ExamineSubUi::Item),
            "still not a creature"
        );
        assert_eq!(pane(true, false, false), Some(ExamineSubUi::Creature));
        assert_eq!(
            pane(true, true, false),
            Some(ExamineSubUi::Char),
            "a template alone"
        );
        assert_eq!(
            pane(true, false, true),
            Some(ExamineSubUi::Char),
            "a title alone"
        );
        assert_eq!(
            pane(true, true, true),
            Some(ExamineSubUi::Char),
            "and both, which is all the corpus has"
        );
    }

    /// The two spell blocks sort on the top bit and nothing else.
    #[test]
    fn the_two_spell_blocks_sort_on_the_top_bit_and_nothing_else() {
        use crate::view::{AppraisalSpellView, MagicInfoView};
        let spell = |id: u32, name: &str| AppraisalSpellView {
            raw_id: id,
            enchantment: id & 0x8000_0000 != 0,
            name: name.to_string(),
            description: format!("{name} does something."),
        };
        let view = |m: MagicInfoView| AppraisalView {
            success: true,
            magic: m,
            ..Default::default()
        };

        // No `0x0010` block produces no magic lines.
        let none = view(MagicInfoView::default());
        assert!(short_magic_info_lines(&none).is_empty());
        assert!(magic_info_lines(&none).is_empty());

        // An **empty** list is past that gate and still draws nothing: the loops run zero times
        // and both emptiness guards fall through.
        let empty = view(MagicInfoView {
            spells: Some(Vec::new()),
            ..Default::default()
        });
        assert!(short_magic_info_lines(&empty).is_empty());
        assert!(magic_info_lines(&empty).is_empty());

        // One of each. The high-bit test keeps the enchantment out of the short list entirely.
        let both = view(MagicInfoView {
            spells: Some(vec![
                spell(1183, "Revitalize Other I"),
                spell(0x8000_0000 | 1, "Strength Other I"),
            ]),
            ..Default::default()
        });
        assert_eq!(
            short_magic_info_lines(&both),
            vec![("Spells: Revitalize Other I".to_string(), false)]
        );
        assert_eq!(
            magic_info_lines(&both),
            vec![
                (
                    "Spell Descriptions:\n~ Revitalize Other I: Revitalize Other I does something."
                        .to_string(),
                    false
                ),
                (
                    "Enchantments:\n\n~ Strength Other I: Strength Other I does something."
                        .to_string(),
                    false
                ),
            ],
            "the Enchantments header carries its own newline and Spell Descriptions does not"
        );

        // Two in the short list get the client's `", "`, in wire order.
        let two = view(MagicInfoView {
            spells: Some(vec![
                spell(1183, "Revitalize Other I"),
                spell(1, "Strength Other I"),
            ]),
            ..Default::default()
        });
        assert_eq!(
            short_magic_info_lines(&two),
            vec![(
                "Spells: Revitalize Other I, Strength Other I".to_string(),
                false
            )]
        );

        // An id the `SpellTable` has no row for still costs a separator (the client sets the flag
        // before the client tests the length) and still gets a `"\n~ : "` entry.
        let unknown = view(MagicInfoView {
            spells: Some(vec![
                spell(1183, "Revitalize Other I"),
                AppraisalSpellView {
                    raw_id: 999_999,
                    ..Default::default()
                },
            ]),
            ..Default::default()
        });
        assert_eq!(
            short_magic_info_lines(&unknown),
            vec![("Spells: Revitalize Other I, ".to_string(), false)]
        );
        assert_eq!(
            magic_info_lines(&unknown),
            vec![(
                "Spell Descriptions:\n~ Revitalize Other I: Revitalize Other I does something.\n~ : "
                    .to_string(),
                false
            )]
        );

        // Both blocks answer a failed assessment, both as a paragraph.
        let failed = AppraisalView {
            success: false,
            magic: MagicInfoView {
                spells: Some(vec![spell(1183, "Revitalize Other I")]),
                spellcraft: Some(250),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            short_magic_info_lines(&failed),
            vec![("Spells: unknown.".to_string(), false)]
        );
        assert_eq!(
            magic_info_lines(&failed),
            vec![("Spells: unknown.".to_string(), false)]
        );
    }

    /// The magic block's mana lines hang off the description bucket.
    #[test]
    fn the_magic_blocks_mana_lines_hang_off_the_description_bucket() {
        use crate::view::{AppraisalSpellView, MagicInfoView};
        let own = AppraisalSpellView {
            raw_id: 1183,
            enchantment: false,
            name: "S".to_string(),
            description: "D".to_string(),
        };
        let ench = AppraisalSpellView {
            raw_id: 0x8000_0001,
            enchantment: true,
            ..own.clone()
        };
        let all = MagicInfoView {
            spellcraft: Some(250),
            cur_mana: Some(700),
            max_mana: Some(800),
            mana_cost: Some(5),
            ..Default::default()
        };

        // With an own spell: `Spellcraft:`, `Mana:` and the flat `Mana Cost:` arm, in order, and
        // the last of them a **paragraph** (`same_line = 0`) where the first two are lines.
        let lines = magic_info_lines(&AppraisalView {
            success: true,
            magic: MagicInfoView {
                spells: Some(vec![own.clone()]),
                ..all.clone()
            },
            ..Default::default()
        });
        assert_eq!(
            lines,
            vec![
                ("Spellcraft: 250.".to_string(), true),
                ("Mana: 700 / 800.".to_string(), true),
                (
                    "Mana Cost: 5.\n(Can be reduced by the Mana Conversion skill)".to_string(),
                    false
                ),
                ("Spell Descriptions:\n~ S: D".to_string(), false),
            ]
        );

        // The **same** properties with only an enchantment: the client skips all four.
        assert_eq!(
            magic_info_lines(&AppraisalView {
                success: true,
                magic: MagicInfoView {
                    spells: Some(vec![ench]),
                    ..all.clone()
                },
                ..Default::default()
            }),
            vec![("Enchantments:\n\n~ S: D".to_string(), false)]
        );

        // float `5` wins over int `0x75`, and `-0.05` is twenty seconds:
        // `|1.0 / rate| + 0.5`, truncated.
        let rate = |r: f64| {
            magic_info_lines(&AppraisalView {
                success: true,
                magic: MagicInfoView {
                    spells: Some(vec![own.clone()]),
                    mana_rate: Some(r),
                    mana_cost: Some(5),
                    ..Default::default()
                },
                ..Default::default()
            })[0]
                .clone()
        };
        assert_eq!(
            rate(-0.05),
            ("Mana Cost: 1 point per 20 seconds.".to_string(), true)
        );
        assert_eq!(
            rate(0.05),
            ("Mana Cost: 1 point per 20 seconds.".to_string(), true)
        );
        // `+0.5` then truncation: 1/0.03 = 33.33, +0.5 = 33.83, floor 33.
        assert_eq!(
            rate(-0.03),
            ("Mana Cost: 1 point per 33 seconds.".to_string(), true)
        );

        // Zero or less takes the bare sentence, and it is still a paragraph.
        assert_eq!(
            magic_info_lines(&AppraisalView {
                success: true,
                magic: MagicInfoView {
                    spells: Some(vec![own.clone()]),
                    mana_cost: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            })[0],
            ("Mana Cost: 0.".to_string(), false)
        );

        // One `&&` means half the pair draws no line.
        assert_eq!(
            magic_info_lines(&AppraisalView {
                success: true,
                magic: MagicInfoView {
                    spells: Some(vec![own]),
                    cur_mana: Some(700),
                    ..Default::default()
                },
                ..Default::default()
            }),
            vec![("Spell Descriptions:\n~ S: D".to_string(), false)]
        );
    }

    /// Oracle: the client's more-than-one-glyph guard.
    #[test]
    fn the_first_block_gets_no_separator_and_later_ones_do() {
        let a = add_item_info("", "Value: 5", true);
        assert_eq!(a, "Value: 5");
        let b = add_item_info(&a, "Burden: 10", true);
        assert_eq!(b, "Value: 5\nBurden: 10");
        let c = add_item_info(&b, "Block", false);
        assert_eq!(c, "Value: 5\nBurden: 10\n\nBlock");
    }
    /// The portrait camera frames square and tall boxes using the element aspect ratio.
    /// Hand-computed distances cover both clamp branches, the equality boundary, and an
    /// off-origin centre. Equality cannot distinguish a strict comparison because both
    /// branches produce the same distance there.
    #[test]
    fn the_portrait_camera_frames_the_box_by_its_aspect_clamp() {
        use dereth_primitives::Vec3;

        let tall = portrait::camera_position(
            Vec3::new(-0.5, -0.5, -1.0),
            Vec3::new(0.5, 0.5, 1.0),
            300,
            300,
        );
        assert_eq!(
            (tall.x, tall.z),
            (0.0, 0.0),
            "the camera sits on the box's centre in x and z"
        );
        assert!(
            (tall.y - -2.914_213_6).abs() < 1e-4,
            "dz=2 unclamped: {}",
            tall.y
        );

        // The clamp does not fire on equality.
        let equal = portrait::camera_position(
            Vec3::new(-0.5, -0.5, -1.0),
            Vec3::new(0.5, 0.5, 1.0),
            150,
            300,
        );
        assert!(
            (equal.y - -2.914_213_6).abs() < 1e-4,
            "dz/dx == h/w keeps dz: {}",
            equal.y
        );

        // And it does fire when the object is shallow for the element's aspect.
        let wide = portrait::camera_position(
            Vec3::new(-1.0, -0.5, -0.5),
            Vec3::new(1.0, 0.5, 0.5),
            150,
            300,
        );
        assert!(
            (wide.y - -5.328_427).abs() < 1e-4,
            "dz clamped to dx*(h/w)=4: {}",
            wide.y
        );

        // The centre is carried through in x and z, so an off-origin creature is still framed.
        let off = portrait::camera_position(
            Vec3::new(9.5, -0.5, 4.0),
            Vec3::new(10.5, 0.5, 6.0),
            300,
            300,
        );
        assert!(
            (off.x - 10.0).abs() < 1e-4 && (off.z - 5.0).abs() < 1e-4,
            "centre in x and z"
        );
    }
}
