//! The read-only seam onto the game model, and the request channel back out.
//!
//! This is the presentation-only boundary for UI screens.
//!
//! This crate does not depend on `dereth-client-model`: game state reaches
//! a panel as `&dyn GameView` and as `Notice`s, and requests leave as [`UiRequest`] values the
//! binary routes. That is the whole reason the panels can be tested without a world.
//!
//! **The client predicts nothing.** `GameView` is `&dyn` and has no `&mut`
//! anywhere: a panel physically cannot write the value it is about to ask the server for, so an
//! inventory move ghosts the icon and waits for `Item_ServerSaysMoveItem`.

use crate::ids::UiMode;
use dereth_primitives::{DataId, ObjectId};

mod request;
pub use request::*;
mod social;
pub use social::*;
mod game_view;
pub use game_view::*;
mod appraisal;
pub use appraisal::*;

/// The three secondary attributes the vitals bar and the toolbar read-out show.
///
/// The vitals bar reads a *current* and a *max* secondary attribute per bar; the numeric ids are
/// the `Attribute2ndType` values established for the in-game HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Vital {
    Health,
    Stamina,
    Mana,
}

impl Vital {
    /// `(current stat, max stat)`, using the HUD's vital-stat ids.
    #[must_use]
    pub const fn stats(self) -> (u32, u32) {
        match self {
            Self::Health => (2, 1),
            Self::Stamina => (4, 3),
            Self::Mana => (6, 5),
        }
    }

    /// The three bars in binding order.
    pub const ALL: [Self; 3] = [Self::Health, Self::Stamina, Self::Mana];
}

/// One radar row, reduced to what the drawing code reads.
///
/// The original client stores an object reference, color and blip shape together.
/// Color and shape are derived from the object by `crate::mapradar::radar`, so this
/// seam carries those object facts plus the computed player-space position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RadarEntry {
    pub id: ObjectId,
    /// The object's player-relative, already-heading-up position.
    pub player_space: (f32, f32, f32),
    /// The object's radar blip-color override. 0 means "not overridden".
    pub blip_color: u8,
    /// The object's public-description bitfield.
    pub bitfield: u32,
    /// The `RadarEnum` the server sent, or `0`
    /// (undefined) when it sent none.
    ///
    /// This field controls the radar's visibility filter: an object is listed only when its
    /// `_radar_enum` is `ShowMovement`(2), `ShowAttacking`(3) or `ShowAlways`(4). Absent means
    /// `Undef`(0) and `Undef` is not shown — which is why the retail radar plots a handful of
    /// blips in a scene holding hundreds of objects. See
    /// `crate::mapradar::radar::inq_showable_on_radar`.
    pub radar_enum: u8,
    /// Whether the object is a player: `_bitfield & 0x08` (`Player`).
    ///
    /// True of **any** player, ours included — it is the flag the blip colour branches on to reach
    /// the admin / PK / fellowship colours. "Is this our own character" is [`Self::is_self`], a
    /// different question with a different answer.
    pub is_player: bool,
    /// Whether this id is the local player id — our own character, and only ours.
    ///
    /// The radar's add-object entry point opens by comparing the incoming object's id
    /// against the local player id and returning if they match, so the player never
    /// gets a radar entry and never a blip; what marks his position is the green cross
    /// the radar's own draw pass lays on the centre point
    /// (`crate::mapradar::radar::center_marker_fills`). The blip-shape query asks the same
    /// question again for the PK-threat shapes, which compare *another* object's PK state against
    /// ours. Our seam carries one flat list where the client has two structures, so the entry stays
    /// in the list with this flag set and `crate::mapradar::radar::draw_objects` skips it.
    pub is_self: bool,
    /// Whether the object's type has the creature bit: **`_type & 0x10`**, not a `_bitfield` bit.
    ///
    /// The source is the creature type, whatever the name says. The radar tests the creature type
    /// here.
    /// The `_bitfield & 0x10` half of the same test is the separate
    /// `Attackable` flag (`crate::mapradar::radar::bitfield::ATTACKABLE`); the two are different
    /// words that happen to share a mask. The field keeps its name so no existing assertion moves.
    pub is_attackable: bool,
    pub is_pk: bool,
    pub is_pk_lite: bool,
    pub is_allegiance_member: bool,
    pub is_fellow: bool,
    pub is_fellowship_leader: bool,
    /// False when the object has no cell or `position.objcell_id == 0`; the radar draw skips it.
    pub in_world: bool,
}

/// One row of the skills panel; each value comes from the panel's skill-table loop.
///
/// The function walks the **skill table** selected by group 4, type 2, and enum `0x10000004`, not the
/// player's own skill hash, and asks about each key; a skill the player
/// has no entry for still gets a row, in the `UNDEF` group. So one entry here is
/// `(SkillTable key, SkillBase, skill-lookup answer)` already joined, which is the only shape that
/// keeps `dereth-ui-screens` free of `dereth-client-model` and `dereth-assets`' table types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    /// The skill-table key shown by the skill-info region.
    pub id: u32,
    /// The skill definition's name, which is a `SkillTable`
    /// lookup and not a string-table one.
    pub name: String,
    /// The skill-table icon used for this row.
    pub icon: Option<DataId>,
    /// The skill definition's minimum level. The skill-list rebuild puts an **untrained** skill
    /// whose minimum level is above 1 in the fourth group rather than the third.
    pub min_level: u32,
    /// 0 undef, 1 untrained, 2 trained, 3 specialised.
    pub sac: u32,
    /// The unenchanted skill value used as the baseline for the font comparison.
    pub level: i32,
    /// The enchanted skill value used for display.
    /// Font comparison uses `effective - vitae` against `level`.
    pub effective: i32,

    /// The vitae modifier for this skill — zero, or **negative**
    /// under a vitae penalty.
    ///
    /// `Enchant(skill(raw = 1)) - skill(raw = 1)` with **only** the vitae enchantment
    /// applied, rounded half up. It is not `effective - level`, which also carries the spells.
    ///
    /// **Why the colour needs it.** Vitae is an enchantment, so
    /// [`Self::effective`] already has the penalty folded in; comparing that against
    /// [`Self::level`] with no correction marks a vitae-carrying character's **whole skill list
    /// debuffed red**, which retail does not do. The comparands are
    /// `level` and `effective - vitae`, and on a character with vitae and no spells those are
    /// equal, so every row draws plain.
    pub vitae: i32,
}

/// Everything the skill footer needs about one skill, already joined.
///
/// The four numbers are the client's own: the "raise 1" and "raise 10" costs
/// for the two buttons, plus the trained footer's two progress-meter values. They are computed by
/// the host, for the same reason [`SkillEntry`]'s name and icon are: this crate must not depend on
/// `dereth-client-model`'s experience table or on `dereth-assets`' `SkillTable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillAdvancement {
    /// The skill's advancement class, repeated so the footer can choose its variant without the row list.
    pub sac: u32,
    /// Experience already sunk into this skill.
    pub pp: u32,
    /// The trained level derived from advancement points.
    pub level_from_pp: u32,
    /// The "raise 1" cost. Below `TRAINED`, this is the skill definition's training cost
    /// in **skill credits**, not experience: one field carries two currencies, which
    /// is why the untrained footer says "Skill Credits To Train".
    pub cost_to_raise: u32,
    /// The "raise 10" cost. Zero for a skill below `TRAINED`.
    pub cost_to_raise_10: u32,
    /// The skill-XP table's total for `sac` at the level earned from `pp` — the meter's floor.
    pub xp_at_level: u32,
    /// The same total one level higher — the meter's ceiling.
    pub xp_at_next_level: u32,
}

impl SkillAdvancement {
    /// The trained footer's meter fraction, transcribed:
    ///
    /// ```text
    /// lo = skill_xp_total(sac, level);
    /// hi = skill_xp_total(sac, level + 1);
    /// span = hi - lo;
    /// fill = span == 0 ? 0.0f : (float)(pp - lo) / (float)span;
    /// ```
    ///
    /// Both operands go through **`float`**, and both are converted with the unsigned fix-up
    /// (`+ 4.2949673e9` when the `int` reading is negative) that the compiler emits for a `ulong`,
    /// so a `pp` below `lo` does not produce a negative fill.
    #[must_use]
    pub fn meter_fill(&self) -> f32 {
        let span = self.xp_at_next_level.wrapping_sub(self.xp_at_level);
        if span == 0 {
            return 0.0;
        }
        let num = self.pp.wrapping_sub(self.xp_at_level);
        #[allow(clippy::cast_precision_loss)]
        {
            num as f32 / span as f32
        }
    }
}

/// Everything the attribute footer needs about one of its nine rows, already joined; the
/// counterpart of [`SkillAdvancement`] for the skill footer.
///
/// The two costs are computed by
/// `dereth_rules::advancement::{attribute_cost_to_raise, attribute_cost_to_raise_10}`,
/// which are verified against recorded traffic.
///
/// `value` and `effective` are the raw and enchanted attribute or vital values used
/// to select the title font. Each footer queries both forms of its stat. See
/// [`SkillEntry::effective`] for why these comparands differ from a trained skill level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeAdvancement {
    /// The primary or secondary attribute level derived from advancement points.
    pub level_from_cp: u32,
    /// Advancement points spent, the operand both costs subtract.
    pub cp_spent: u32,
    /// The "raise 1" cost. 0 at the table's cap, which is the footer's
    /// "Infinity!" and a disabled button.
    pub cost_to_raise: u32,
    /// The "raise 10" cost.
    pub cost_to_raise_10: u32,
    /// The attribute lookup with `raw = 1` -- the **unenchanted** number. It is not what the
    /// footer's title shows; it is the operand the buff/debuff delta is measured from.
    pub value: i32,
    /// The attribute lookup with `raw = 0` -- the enchanted number, and the one the title shows.
    pub effective: i32,
    /// A vital only: the vital lookup for `stat + 1` with `raw = 0`, the **current** half of the
    /// pair the title renders as `current/maximum`. Equal to [`Self::effective`] for a primary,
    /// which has no such half.
    pub current: i32,
    /// A vital only: `Enchant(raw) - raw` under an active vitae penalty, therefore
    /// **negative or zero**. It is appended to the title in parentheses with font 3
    /// and subtracted from the raw side of the buff comparison. Primary attributes
    /// always contribute zero.
    pub vitae: i32,
}

impl AttributeAdvancement {
    /// The attribute footer's font index for the value appended to the title:
    /// `0` plain, `1` buffed, `2` debuffed.
    ///
    /// It is a three-way compare of the two `Inq*` answers: a positive difference uses the
    /// `"+"` prefix and index 1, a negative difference uses index 2, and
    /// equal falls through with the index still 0. The identical shape is in the skill row
    /// and used by the information-row presentation.
    #[must_use]
    pub const fn title_font(&self) -> u32 {
        let base = self.value - self.vitae;
        if base < self.effective {
            1
        } else if self.effective < base {
            2
        } else {
            0
        }
    }

    /// `enchanted - raw` -- the number the title's parenthesised suffix carries, and 0 when there
    /// is no suffix at all. The attribute footer computes it as one subtraction and skips the
    /// whole append when it is zero.
    #[must_use]
    pub const fn title_delta(&self) -> i32 {
        self.effective - self.value
    }
}

/// The three outcomes of the stat panel's PK-status line.
///
/// The player object's PK and PK-lite predicates select one of three localized
/// strings. The corresponding Rust predicates are
/// `dereth_client_model::weenie::Weenie::{is_pk, is_pk_lite}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PkStatus {
    /// `ID_StatManagement_Header_PKStatus_NPK`, also used when the player object is missing.
    #[default]
    Npk,
    /// `ID_StatManagement_Header_PKStatus_PK` -- the is-PK predicate.
    Pk,
    /// `ID_StatManagement_Header_PKStatus_PKL` -- the is-PK-lite predicate, and only
    /// when is-PK said no.
    PkLite,
}

impl PkStatus {
    /// The PK-status line's label, as retail chooses it (then looked up in enum `0x10000001`):
    /// `PK` when is-PK is true, `PKL` when is-PK is false and is-PK-lite is true, and
    /// `NPK` otherwise -- including for no weenie at all.
    #[must_use]
    pub const fn of(is_pk: bool, is_pk_lite: bool) -> Self {
        if is_pk {
            Self::Pk
        } else if is_pk_lite {
            Self::PkLite
        } else {
            Self::Npk
        }
    }

    /// The localization token resolved in string table `0x10000001`.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Npk => "ID_StatManagement_Header_PKStatus_NPK",
            Self::Pk => "ID_StatManagement_Header_PKStatus_PK",
            Self::PkLite => "ID_StatManagement_Header_PKStatus_PKL",
        }
    }
}

/// One page of an open book, projected from the received page data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookPageView {
    /// The page author's object id; the editor compares it with the local player.
    pub author_id: ObjectId,
    /// The author name used as the page-row label.
    pub author_name: String,
    /// The author's account, appended in angle brackets when the viewer has PSR privileges.
    pub author_account: String,
    /// The page text, or `None` when the message carried no text -- the state that makes the
    /// page-setter request the page data instead of drawing.
    pub text: Option<String>,
    /// Whether to ignore the author, kept raw and signed. Its consumers deliberately disagree: the menu update
    /// blanks the author only for exactly `1`, while the page display treats every nonzero value
    /// as editable. Keep the dword intact until each consumer applies its own native comparison.
    pub ignore_author: i32,
}

/// The book opened by the `0x00B4 Writing_BookOpen` message, as the panel receives it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookView {
    pub book_id: ObjectId,
    /// The object's context-appropriate name, interpolated into the owned-blank refusal.
    /// Kept separate from [`Self::title`]: a signed
    /// book displays its inscription but the refusal still names the object.
    pub object_name: String,
    /// The local player's identity, compared with each page's author before enabling editing.
    pub player_id: ObjectId,
    /// The viewer's PSR flag: Boolean quality `0x2C`, `0x2D`, or
    /// `0x61`. The book menu reads this before choosing its author label format.
    pub viewer_is_psr: bool,
    /// The message's **second dword**, the maximum page count. It is the range the page setter clamps
    /// against and the count the menu update pads the strip out to.
    pub max_num_pages: u32,
    pub pages: Vec<BookPageView>,
    pub inscription: String,
    pub scribe_id: ObjectId,
    pub scribe_name: String,
    /// What opening a book puts in its title text (`0x1000010F`): the book's own name when
    /// the scribe id is 0, otherwise the inscription. Composed on
    /// the far side of the seam because the object name is the world's and this crate cannot see
    /// it.
    pub title: String,
    /// `dereth_client_model::book::BookState::opening` -- **the panel's edge**. A second `0x00B4` for the
    /// same book is a real re-open in the client, so id equality cannot stand in for it.
    pub opening: u64,
    /// `dereth_client_model::book::BookState::page_data_applied`, so a `0x00B8` that filled the page the
    /// panel is showing redraws it without a re-open.
    pub page_data_applied: u64,
    /// `dereth_client_model::book::BookState::add_page_responses` — the third edge, for `0x00B6`.
    pub add_page_responses: u64,
    /// The `0x00B6` behind that count, including the author used to build the new page.
    ///
    /// The client reads the author inside the handler — the player's id and
    /// the name string quality (property 1) on the local player description — and neither is visible from this
    /// crate, so both are composed on the far side of the seam exactly as [`Self::title`] is.
    pub add_page: Option<AddedPageView>,
}

/// The sixteen values delivered by `0x0075 Character_StartBarber`, plus the two
/// player qualities read while the barber panel initializes.
///
/// This copy is the read-only UI seam rather than a protocol type: `dereth-ui-screens` deliberately
/// has no dependency on `dereth-protocol`, and the host performs the field-for-field projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BarberView {
    /// The notice edge. A repeated `0x0075` is a new modal even when all sixteen values agree.
    pub generation: u64,
    pub base_palette: u32,
    pub head_object: u32,
    pub head_texture: u32,
    pub default_head_texture: u32,
    pub eyes_texture: u32,
    pub default_eyes_texture: u32,
    pub nose_texture: u32,
    pub default_nose_texture: u32,
    pub mouth_texture: u32,
    pub default_mouth_texture: u32,
    pub skin_palette: u32,
    pub hair_palette: u32,
    pub eyes_palette: u32,
    pub setup_id: u32,
    pub option1: i32,
    pub option2: i32,
    /// `PropertyInt 0xBC HeritageGroup` on the local `PlayerDesc`.
    pub heritage: u32,
    /// `PropertyInt 0x71 Gender` on the local `PlayerDesc`.
    pub gender: u32,
}

/// The exact sixteen fields written after the outbound `0x0311` subtype.
/// Kept distinct from [`BarberView`]: this request carries neither the inbound
/// notice generation nor player qualities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BarberAppearance {
    pub base_palette: u32,
    pub head_object: u32,
    pub head_texture: u32,
    pub default_head_texture: u32,
    pub eyes_texture: u32,
    pub default_eyes_texture: u32,
    pub nose_texture: u32,
    pub default_nose_texture: u32,
    pub mouth_texture: u32,
    pub default_mouth_texture: u32,
    pub skin_palette: u32,
    pub hair_palette: u32,
    pub eyes_palette: u32,
    pub setup_id: u32,
    pub option1: i32,
    pub option2: i32,
}

/// One `0x00B6 Writing_BookAddPageResponse` projected for the book panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddedPageView {
    pub page: u32,
    pub success: bool,
    /// The local player's ID, stored as the new page's author and compared by its editability test.
    pub author_id: ObjectId,
    /// The name string quality (property 1) on the local player description — the label the book menu
    /// puts on the new row.
    pub author_name: String,
}

/// One spellbook row, reduced to the fields the panel uses.
///
/// The player's known-spell ids are joined to the `SpellTable`.
/// `level` is a rough heuristic, not a stored field; `display_order` is the spell
/// definition's ordering value compared during sorted insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellEntry {
    pub id: u32,
    /// The spell name drawn in the row's text element.
    pub name: String,
    /// The spell's icon id returned by the magic system.
    pub icon: Option<DataId>,
    /// The spell school: 1 War, 2 Life, 3 Item, 4 Creature, 5 Void.
    pub school: u32,
    /// The spell level by retail's rough heuristic — 1…9, or 0 when the power component is unrecognised.
    pub level: u32,
    /// The first component's power, before the display-level collapse, used by icon composition.
    pub icon_power: u32,
    /// The spell's display order.
    pub display_order: i32,
    /// The spell's bitfield.
    ///
    /// The icon compositor reads three of its bits and nothing else
    /// in this crate reads any of them: `Reversed (0x10)` picks the `0x10000007 UISpellOverlays`
    /// row the white texels are replaced from, and `FellowshipSpell (0x2000)` / `SelfTargeted
    /// (0x8)` pick the badge blitted last. Decoded by `dereth_assets::SpellBase`.
    pub bitfield: u32,
}

/// One component row of the spell-examine formula list — a `SpellComponentBase` reduced to what
/// the examine loop reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpellExamineComponent {
    /// The formula slot's **SCID**, used to look up the component and stored under row property
    /// `0x10000010`.
    pub scid: u32,
    /// The de-obfuscated component name.
    pub name: String,
    /// The component icon. When it is `None`, the client skips the entire component row,
    /// including its text.
    pub icon: Option<DataId>,
}

/// The spell-examine inputs, gathered on the host side.
///
/// The join follows the client's sequence: the spell record supplies the base fields, the
/// appropriate-formula lookup supplies the formula, and the component table supplies each slot.
/// It is done host-side
/// because `dereth-ui-screens` may not depend on `dereth-client-model` or on the two dat tables,
/// exactly as [`EffectEntry`] and [`SpellEntry`] are.
///
/// **Nothing here is a server round trip.** The examine entry point cancels any
/// pending appraisal by appraising object id zero and then fills the pane from the two dat
/// tables; no `0x00C8` is sent and no `0x00C9` is awaited.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpellExamineView {
    /// The spell name used as the window title.
    pub name: String,
    /// The spell description used as the first block of display text.
    pub description: String,
    /// The spell school (1 War, 2 Life, 3 Item, 4 Creature, 5 Void), for the client's
    /// school-name lookup.
    pub school: u32,
    /// The spell's base mana, passed as the mana text's first argument.
    pub base_mana: i32,
    /// The per-target mana modifier, passed as the `" + %d per target"` half.
    pub mana_mod: i32,
    /// The spell's duration, seconds. **`-1.0` is "no duration"**, which is the
    /// literal the client returns for a meta-spell that has none, and the value its `!= -1.0`
    /// guard tests.
    pub duration: f64,
    /// The client's answer, in **metres**, already clamped to
    /// 75. `0.0` is the client's "no range", on which it clears the element.
    pub range: f32,
    /// The spell icon id returned by the magic system.
    pub icon: Option<DataId>,
    /// The spell's collapsed display level, as in the spellbook.
    pub level: u32,
    /// The first component's raw power used by the icon background.
    pub icon_power: u32,
    pub bitfield: u32,
    /// One entry per slot counted, **in formula
    /// order**; `None` where the spell-component lookup missed, which is the client's first skip.
    pub components: Vec<Option<SpellExamineComponent>>,
}

/// One row of the effects list — what the effect row draws.
///
/// The list rebuild walks the enchantment registry, retains entries accepted by
/// the UI-type filter, and builds one row per survivor. The host joins the spell's
/// name, icon, beneficial flag and remaining time, because `dereth-ui-screens` may
/// not depend on `dereth-client-model` or the `SpellTable`. The original
/// client performs this join in the subsystem that builds the rows as well.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectEntry {
    /// The enchantment id's low 16 bits, used as the effect region's spell id.
    pub spell: u32,
    /// The effect row's label.
    pub name: String,
    /// The spell description used as the second half of the effects panel's
    /// selected-spell info line.
    pub description: String,
    /// The spell icon returned for the row's `0x10000129` child.
    pub icon: Option<DataId>,
    /// Bit 4 of the spell bitfield. **This bit is the whole of the buff/debuff split**:
    /// The effect-type match accepts a spell when the panel's effects type is 1 and the bit is set,
    /// or `== 2` and it is clear.
    pub beneficial: bool,
    /// The enchantment's start time plus its duration, less the current frame time, in seconds —
    /// **already rebased on receipt**.
    /// Do not recompute it from a server timestamp.
    pub remaining: f64,
    /// Whether the enchantment duration is negative.
    pub permanent: bool,
    /// The enchantment's spell category.
    pub category: u16,
    /// The enchantment's power level.
    pub power_level: i32,
}

/// The vitae panel's inputs, gathered on the host side.
///
/// The threshold is computed here rather than in the panel because it is
/// `dereth_rules::advancement::vitae_cp_pool_threshold`, evaluated by the runtime's HUD view. The **level fall-back is part of it**:
/// the panel update reads `DeathLevel` (139) and uses `Level` (25) only when that is zero, and
/// doing it here keeps the panel from needing two int qualities it would otherwise have to name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VitaeDisplay {
    /// The vitae multiplier. **1.0 when there is none**, and
    /// the update's `100 - (int)(v * 100)` then gives 0, which is the full-strength arm.
    pub multiplier: f32,
    /// Integer quality `0x81`, `VitaeCpPool`, the experience already earned back.
    pub cp_pool: i32,
    /// `VitaeCPPoolThreshold(multiplier, death level, or level when that is zero)` — the pool
    /// needed to burn off one percentage point.
    pub threshold: i32,
}

/// The character sheet's six sections' inputs, gathered on the host side.
///
/// Fields come from the quality inquiries used by those six section updates;
/// [`super::panels::characterinfo`] documents them. `capacity` is the one derived
/// value, computed by the host because its encumbrance formula lives in `dereth-client-model`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterInfo {
    /// The attribute lookup's innate (initial) level, for ids `1, 2, 4, 3, 5, 6` — i.e. in
    /// `ID_CharacterInfo_Innates`' **display** order: Strength, Endurance, Coordination,
    /// Quickness, Focus, Self.
    pub innate: [i32; 6],
    /// `InqInt(181 ChessRank)`. The client's local default is **1400** (`0x578`), not zero.
    pub chess_rank: i32,
    /// `InqInt(192 FakeFishingSkill)`, default 0.
    pub fishing_skill: i32,
    /// `InqInt(43 NumDeaths)`.
    pub num_deaths: i32,
    /// Attribute 1 Strength with `raw = 1` — the **raw** level, which is what
    /// the endurance line asks for and is not the same read the load line makes.
    pub strength: i32,
    /// Attribute 2 Endurance with `raw = 1`.
    pub endurance: i32,
    /// The player's carried load.
    pub load: f32,
    /// `InqInt(5 EncumbranceVal)`.
    pub encumbrance: i32,
    /// `EncumbranceCapacity(strength(raw = 0, default 10), augmentations)`.
    pub capacity: i32,
    /// `InqInt(230 AugmentationIncreasedCarryingCapacity)`.
    pub augmentations: i32,
    /// `InqInt(98 CreationTimestamp)` -- the birth/age/deaths section's **first**
    /// arm, gated on `InqInt`'s **return** and not on the value, so `None` is the arm that
    /// writes no birth line at all.
    pub created: Option<i32>,
    /// `InqInt(125 Age)` -- the second arm, gated the same way. This is also the **one** quality
    ///  the sheet registers a quality-changed handler for
    /// (int quality `0x7D` on the player).
    pub age: Option<i32>,
    /// `InqInt(390 Enlightenment)` -- the line after the deaths line, drawn only when the
    /// property is present **and** above zero (the examine pane's line has no such floor).
    pub enlightenment: Option<i32>,
    /// `InqInt(354 WeaponMastery)` -- the augmentations section's first line, and a
    /// **weapon-group id**, not a skill level. `> 0` gates the line;
    /// presentation maps it to the displayed mastery name.
    pub melee_mastery: i32,
    /// `InqInt(355 MissileMastery)`, the ranged mastery group.
    pub ranged_mastery: i32,
    /// `InqInt(362 SummoningMastery)`, the summoning mastery group.
    pub summoning_mastery: i32,
    /// Every **other** int inquires, by property id.
    ///
    /// A map rather than fifty named fields because the function is fifty copies of one shape:
    /// `v = InqInt(id)` (defaulted to 0 before the call, and the return ignored), `if v > 0`,
    /// emit one row with `v` as `%NumAugmentations`. The order on screen is
    /// the luminance rows followed by the augmentation rows; an absent
    /// key and a `0` are the same answer, exactly as they are to the client.
    pub aug_ints: std::collections::BTreeMap<u32, i32>,
    /// What `localtime` would add to [`Self::created`] before the sheet's `strftime("%c")`.
    /// See [`GameView::utc_offset_secs`]. `0` renders the born line in UTC.
    pub utc_offset_secs: crate::ctime::UtcOffsetSecs,
}

/// The target-cursor mode.
///
/// The original client keeps this in shared UI state. The host stores it in
/// `dereth_client_runtime::interaction::Interaction`; the toolbar reads it through this seam
/// without needing to know that storage location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    /// No target mode.
    #[default]
    None,
    /// The use target mode.
    Use,
    /// The examine target mode.
    Examine,
}

/// Compact drag feedback shared by equipment slots and the figure canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipmentHover {
    /// Accepted slot masks after the wield legality check; none leaves the hint unchanged.
    pub slot_mask: Option<u32>,
    /// Whether the canvas accepts; none leaves the hint unchanged.
    pub canvas: Option<bool>,
}

impl Default for EquipmentHover {
    fn default() -> Self {
        Self {
            slot_mask: None,
            canvas: Some(false),
        }
    }
}

impl EquipmentHover {
    #[must_use]
    pub fn at_location(self, mask: u32) -> Option<bool> {
        self.slot_mask.map(|accepted| accepted & mask != 0)
    }
}

/// Where a dragged item was dropped: the item-list drag handler's destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    /// The client's exact `0x100001B1` catcher: the large
    /// backpack button. It is not an item list and its destination is chosen by
    /// the client's place-in-backpack logic, so it cannot be represented by a
    /// [`Self::Container`] id on this side of the object-table boundary.
    BackpackButton,
    /// An item-list slot, identified by the list element id and slot index.
    ItemList {
        list: crate::ids::ElementId,
        slot: u32,
    },
    /// One of the eighteen shortcut slots, reached by a drag whose proxy carries
    /// **no** drop-item alias bit, i.e. a real item off some other list.
    /// The toolbar's drop handler takes this on its first arm.
    ShortcutSlot(u32),
    /// One of the eighteen shortcut slots, reached by a drag that **is itself a shortcut** —
    /// the toolbar's drop handler on its `else if` arm for the drop-item shortcut flag.
    /// This is what moves a shortcut from one slot to another.
    ///
    /// It is a separate variant rather than a flag on [`Self::ShortcutSlot`] because the two arms
    /// call different functions: this one adds the shortcut directly, not
    /// the create-shortcut-to-item path — the object is already the player's and already had its
    /// shortcut removed at pick-up, so re-running that path's
    /// owned-by-player / place-in-backpack fork would be a second gate on a decision the client
    /// has already made.
    ShortcutAlias {
        /// The slot the drop landed on — the index of the slot sweep that found the drop element.
        slot: u32,
        /// The last shortcut slot dragged: the slot the drag **left**, written at pick-up by
        /// the begin-drag notice. `-1` when no shortcut drag is
        /// in flight, which the slot-available test's `-1 < slot` refuses.
        from: i32,
    },
    /// An equipment slot, resolved by the interface into its accepted mask and paired side.
    EquipLocation { mask: u32, side: u32 },
    /// The figure itself: wearing chooses from the item's whole location mask.
    EquipCanvas,
    /// The world viewport, which forwards element message `0x15`.
    World,
    /// A container object.
    Container(ObjectId),
    /// An inventory item-list slot, **resolved** into the four facts its drag accept
    /// reads from the element tree. The panel that filled the list has already
    /// translated element ids into object ids.
    ///
    /// [`Self::ItemList`] carries a list element and a slot number, which
    /// `dereth_client_runtime::interaction` cannot map to anything. Converting it to a bare
    /// [`Self::Container`] would **lose the slot index altogether**: every drag into a pack would
    /// be sent as a put-item-in-container of `(item, container, 0)` — the head of the list,
    /// whatever slot the player aimed at — and a drop onto a plain item as a put-into-*that-item*,
    /// which no server can answer.
    ///
    /// The remaining decisions all need a weenie (its item and container capacities, whether it
    /// is a container, the item's place in the list, its trade state) and belong to
    /// `dereth_client_model`.
    /// This variant is the seam between the two halves and carries
    /// nothing this crate had to ask the game for.
    ItemListSlot {
        /// The list's parent container id.
        container: ObjectId,
        /// The client's slot's `itemID`; `None` for an empty slot.
        under: Option<ObjectId>,
        /// The index the list box reports for the row under the cursor.
        index: u32,
        /// The list's item count — the clamp on `index`.
        num_ui_items: u32,
        /// The drag proxy's drop-item container bit.
        dragged_is_container: bool,
        /// The list's container-list flag — whether the list the drop landed on is the side-pack strip
        /// rather than an item grid. The host needs it to name the list retail's
        /// pending item would have gone into; the drop-acceptance check itself never reads it, because
        /// retail already holds the element.
        container_list: bool,
    },
}

/// The combat UI's three read-back notices.
///
/// The client sends them one at a time as they change: the attack-height notice, the
/// power-bar-level notice and
/// the desired-attack-power notice. Height and desired power are polled here;
/// actual power now uses the host's ordered notice delivery, because a final snapshot loses
/// the hide-power-bar notice's zero/Finish and same-frame restart edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombatBar {
    /// The requested attack height (`HIGH` 1,
    /// `MEDIUM` 2, `LOW` 3). `UNDEF` (0) writes nothing, as the handler's `if/else if/else if`
    /// does.
    pub requested_attack_height: u32,
    /// `PowerBarMode`. The meter takes a level only for
    /// `PBM_COMBAT` (1), which is what keeps the jump bar out of it.
    pub power_bar_mode: u32,
    /// The last requested power-bar level — the argument of the last
    /// set-power-bar-level notice, **not** a fresh read of the live power bar. On three arms the
    /// two differ (the minimum pin, the not-ready zero, and the commence-attack handler's
    /// requested attack power with the build stopped), and
    /// this remains the retail cache for inspection. Hide/Finish does not change it, and the
    /// actual UI display comes from notices rather than this possibly stale cached value.
    pub level: f32,
    /// The UI's requested power — the notch, attribute `0x85`.
    pub desired_power: f32,
}

impl Default for CombatBar {
    /// The client's own resets: medium attack height, `PBM_UNDEF`,
    /// no charge, and a **0.5** cap.
    fn default() -> Self {
        Self {
            requested_attack_height: 2,
            power_bar_mode: 0,
            level: 0.0,
            desired_power: 0.5,
        }
    }
}

/// The ten notices produced by the combat system's magic-action handler.
///
/// Eighteen bound keys map to these actions. Only the spellcasting panel consumes
/// them. The type belongs here, like [`PlayerOption`], because it is a UI message
/// and `dereth-ui-screens` deliberately does not depend on `dereth-client-model`. The producer
/// lives in `dereth-client`, above both crates.
///
/// The actions cast the selected spell, move to the previous/next/first/last spell
/// selection, move to the previous/next/first/last spell tab, or cast a numbered
/// quickslot spell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicNotice {
    /// `0x10000060` `CombatCastCurrentSpell`.
    CastCurrentSpell,
    /// `0x10000061` `CombatPrevSpell`.
    PrevSpellSelection,
    /// `0x10000062` `CombatNextSpell`.
    NextSpellSelection,
    /// `0x10000102` `CombatFirstSpell`.
    FirstSpellSelection,
    /// `0x10000103` `CombatLastSpell`.
    LastSpellSelection,
    /// `0x10000063` `CombatPrevSpellTab`.
    PrevSpellTab,
    /// `0x10000064` `CombatNextSpellTab`.
    NextSpellTab,
    /// `0x10000104` `CombatFirstSpellTab`.
    FirstSpellTab,
    /// `0x10000105` `CombatLastSpellTab`.
    LastSpellTab,
    /// `0x10000065`…`0x10000070` `UseSpellSlot_1`…`_12`, zero-based: the client's own argument is
    /// `action - 0x10000065`.
    CastQuickslotSpell { slot: usize },
}

/// A `PlayerOption` bit. The numeric values live in `PlayerModule` and are the game model's
/// responsibility: this enum names them, in the page order
/// the character-settings page adds them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum PlayerOption {
    // ID_CharacterOption_UIBehavior_Section
    ViewCombatTarget,
    SalvageMultiple,
    MainPackPreferred,
    // ID_CharacterOption_UIDisplay_Section
    VividTargetingIndicator,
    ShowTooltips,
    CoordinatesOnRadar,
    SideBySideVitals,
    SpellDuration,
    DisableMostWeatherEffects,
    DisableDistanceFog,
    PersistentAtDay,
    DisableHouseRestrictionEffects,
    UseCraftSuccessDialog,
    ConfirmVolatileRareUse,
    DisplayTimeStamps,
    FilterLanguage,
    ShowHelm,
    ShowCloak,
    // ID_CharacterOption_Grouping_Section
    IgnoreAllegianceRequests,
    IgnoreFellowshipRequests,
    DisplayAllegianceLogonNotifications,
    FellowshipShareXP,
    FellowshipShareLoot,
    FellowshipAutoAcceptRequests,
    // ID_CharacterOption_OtherPlayers_Section
    AcceptLootPermits,
    UseDeception,
    AllowGive,
    IgnoreTradeRequests,
    DragItemOnPlayerOpensSecureTrade,
    DisplayDateOfBirth,
    DisplayAge,
    DisplayChessRank,
    DisplayFishingSkill,
    DisplayNumberDeaths,
    DisplayNumberCharacterTitles,
    // ID_CharacterOption_CharacterBehavior_Section
    ToggleRun,
    AdvancedCombatUI,
    AutoTarget,
    AutoRepeatAttack,
    UseChargeAttack,
    LeadMissileTargets,
    UseFastMissiles,
    // ID_CharacterOption_Chat_Section
    StayInChatMode,
    HearAllegianceChat,
    HearGeneralChat,
    HearTradeChat,
    HearLFGChat,
    HearRoleplayChat,
    HearSocietyChat,
    /// Whether player-killer death broadcasts are shown; the chat section's last row.
    HearPKDeaths,
    /// Not on the character-options page: it is set from the radar's lock button
    /// and read by every locked-status update.
    LockUI,
    /// Not on the character-options page either: option **39** is controlled only by checkbox
    /// `0x1000052C` on the friends panel.
    ///
    /// It is the **player's own** appear-offline setting, not a friend's
    /// — the two are different words on different sides of the wire. See
    /// `dereth_ui_screens::panels::friends::APPEAR_OFFLINE_OPTION_BOX`.
    AppearOffline,
}

impl PlayerOption {
    /// Every option, in declaration order.
    ///
    /// It exists so [`GameSnapshot`](crate::GameSnapshot) can copy
    /// [`GameView::player_option`] and [`GameView::player_option_default`] for *all* of them:
    /// the two methods are keyed by this enum and an owned snapshot has no other way to
    /// enumerate the keys. The order is the declaration order and carries no wire meaning --
    /// the wire ordinal is `dereth_client_runtime::hud::option_ordinal`, which is a different number.
    pub const ALL: [Self; 52] = [
        Self::ViewCombatTarget,
        Self::SalvageMultiple,
        Self::MainPackPreferred,
        Self::VividTargetingIndicator,
        Self::ShowTooltips,
        Self::CoordinatesOnRadar,
        Self::SideBySideVitals,
        Self::SpellDuration,
        Self::DisableMostWeatherEffects,
        Self::DisableDistanceFog,
        Self::PersistentAtDay,
        Self::DisableHouseRestrictionEffects,
        Self::UseCraftSuccessDialog,
        Self::ConfirmVolatileRareUse,
        Self::DisplayTimeStamps,
        Self::FilterLanguage,
        Self::ShowHelm,
        Self::ShowCloak,
        Self::IgnoreAllegianceRequests,
        Self::IgnoreFellowshipRequests,
        Self::DisplayAllegianceLogonNotifications,
        Self::FellowshipShareXP,
        Self::FellowshipShareLoot,
        Self::FellowshipAutoAcceptRequests,
        Self::AcceptLootPermits,
        Self::UseDeception,
        Self::AllowGive,
        Self::IgnoreTradeRequests,
        Self::DragItemOnPlayerOpensSecureTrade,
        Self::DisplayDateOfBirth,
        Self::DisplayAge,
        Self::DisplayChessRank,
        Self::DisplayFishingSkill,
        Self::DisplayNumberDeaths,
        Self::DisplayNumberCharacterTitles,
        Self::ToggleRun,
        Self::AdvancedCombatUI,
        Self::AutoTarget,
        Self::AutoRepeatAttack,
        Self::UseChargeAttack,
        Self::LeadMissileTargets,
        Self::UseFastMissiles,
        Self::StayInChatMode,
        Self::HearAllegianceChat,
        Self::HearGeneralChat,
        Self::HearTradeChat,
        Self::HearLFGChat,
        Self::HearRoleplayChat,
        Self::HearSocietyChat,
        Self::HearPKDeaths,
        Self::LockUI,
        Self::AppearOffline,
    ];
}

/// One tracked contract joined to its static dat definition for the contracts list.
///
/// The contracts panel's list rebuild walks the contract tracker and, for each
/// tracker, looks the **static** contract up in the contract table. The table is selected by group
/// `0x17`, type 2, and enum `0x10000010`, a dat object this crate must not know about. So the
/// join is the host's, exactly as [`SkillEntry`]'s name and [`CharacterTitles`]' are.
///
/// The **strings** cross this seam rather than the raw doubles because every one of them is
/// produced by a function in `dereth_client_model::quests` whose oracle is a retail literal
/// (the progress-string fill and the button update); [`Self::repeat_remaining`] and
/// [`Self::stage`] cross as numbers because they are what the contract sort comparator
/// compares, and a comparator that re-parsed its own rendered text would be a second transcription
/// of the same rule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContractEntry {
    /// The contract id sent by the Abandon button.
    pub contract_id: u32,
    /// The contract's name — the row's `0x100005D1`.
    pub name: String,
    /// The progress string — the row's `0x100005D2`.
    pub status: String,
    /// The contract stage, for the contact choice and for the status comparator's tie-break.
    pub stage: u32,
    /// The contract's description — the notes text `0x100005DE`.
    pub description: String,
    /// The start or end NPC's name, chosen by `quests::contact_is_the_end_npc` —
    /// the contact text `0x100005E0`.
    pub contact: String,
    /// The NPC's rendered position for element `0x100005E1`. `None` when the
    /// `Position` has `objcell_id == 0`; that case leaves the displayed field alone
    /// instead of writing text.
    pub contact_location: Option<String>,
    /// The quest area's location, same treatment — the area text `0x100005E2`.
    pub area_location: Option<String>,
    /// `quests::timed_string` — the timed text `0x100005E3`.
    pub timed: String,
    /// The repeat time less the time elapsed since the server's update, present exactly when
    /// the client's guard holds: the contract stage is 3 and the server-update time is above
    /// `0.0`. Two entries that both have it sort by this number
    /// **ascending**; anything else falls through to the status text.
    pub repeat_remaining: Option<f64>,
}

/// House data in the shape the six sections that read it need.
///
/// The two price lines cross as **strings** for the reason [`ContractEntry`] gives:
/// the host calls the payment-list text helpers with explicit values instead of
/// reimplementing their three-arm naming rule here. [`Self::rent_warning`] likewise
/// uses the rent-warning message builder.
///
/// [`Self::location`] is an already-resolved landscape coordinate pair. Position
/// validation, outside-cell lookup and landblock-to-landscape conversion belong to
/// the physics crate, and this crate does not depend on `dereth-physics`. `None` means draw no
/// location line: the position is invalid, or it is an apartment, whose successful
/// location lookup returns `(-1, -1)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HouseDataView {
    /// The purchase payment list composed as `", "`-joined text.
    pub buy_text: String,
    /// The rent payment list composed as `"<paid>/<num> <name>"`, `", "`-joined.
    pub rent_text: String,
    /// The buy time, a Unix instant; `0` formats as `"N/A"`.
    pub buy_time: i64,
    /// The rent period for the house type plus the rent time, as an integer.
    pub maintenance_period_end: i64,
    /// The same, with the period **doubled** when nothing more is owed.
    pub maintenance_next_due: i64,
    /// `gid_to_lcoord(get_outside_cell_id(position))`, or `None`. See the type's note.
    pub location: Option<(i32, i32)>,
    /// Not maintenance-free and the rent not paid in full — which of the warning-text display's
    /// two arms fires.
    pub rent_owed: bool,
    /// The rent warning message for the house type. Empty when [`Self::rent_owed`] is false.
    pub rent_warning: String,
    /// What `localtime` would add to each of the three instants above, **in that order** —
    /// [`Self::buy_time`], [`Self::maintenance_period_end`], [`Self::maintenance_next_due`]. See
    /// [`GameView::utc_offset_secs`].
    ///
    /// Three numbers and not one, because the time conversion is called once per row and
    /// calls `localtime` each time. The three instants are routinely on opposite
    /// sides of a daylight change — a house bought last winter, maintenance due next summer —
    /// and retail renders each in the zone *it* falls in. All zeroes renders in UTC.
    pub utc_offset_secs: [crate::ctime::UtcOffsetSecs; 3],
}

/// The house profile and owner id projected for the purchase and maintenance
/// window.
///
/// Unlike [`HouseDataView`], which represents the player's own house on the map
/// page, this view describes the dwelling offered by the slumlord just used.
/// The two views can describe different houses simultaneously.
///
/// Everything here is read straight off `0x021D House_HouseProfile`. The house profile's two
/// compose-text helpers and its paid-in-full test run over the window's **current** payment
/// state — i.e. the pristine profile with every dropped item paid into it, which is what
/// the slumlord window's add-item mutates. The host does that arithmetic because
/// `HousePaymentList` lives in `dereth-client-model`; see [`GameView::slumlord_payment`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlumlordView {
    /// The owner id — the slumlord object the profile answered for, and the target of every
    /// request this window makes.
    pub slumlord: ObjectId,
    /// The owner id. **Zero is "unowned"**, which is what the window's
    /// payment-allowed test splits the two tabs on.
    pub owner: ObjectId,
    /// ACE's `OwnerName`, empty for an unowned dwelling.
    /// The window's refresh prints `"Owner: " + (name.empty() ? "None" : name)`.
    pub owner_name: String,
    /// 1 cottage, 2 villa, 3 mansion, **4 apartment**, which is the one
    /// value the window's element-message handler branches on.
    pub house_type: u32,
    /// Whether the owner id equals the local player id. Answered by the host because the panel has
    /// no player id of its own.
    pub am_i_the_owner: bool,
}

/// One walk of `HouseProfile`'s payment arithmetic for one tab, after the window's own drops.
///
/// The five values needed by the purchase and maintenance tabs come from one walk of the same
/// list in the client, so they are one call here rather than five.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlumlordPayment {
    /// The buy-house composition for the buy tab and **the paid-count rent-house composition** for the rent
    /// tab. The asymmetry is retail's: the purchase panel shows
    /// `"<num> <name>"` and the maintenance panel `"<paid>/<num> <name>"`.
    pub requirements: String,
    /// Whether the buy-house requirements are paid in full — the button update's second gate on
    /// the Buy button.
    pub paid_in_full: bool,
}

/// The inputs of the house panel's purchase-time line that are **not** the house data.
///
/// The client resolves the player description and draws no line if either interface lookup fails.
/// It then queries the house-purchase timestamp into a pre-zeroed value, ignores the query result,
/// and tests whether the purchase wait period has expired.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HousePurchaseView {
    /// Whether the player description is available. False draws nothing, as before
    /// `LOGIN_COMPLETE`.
    pub have_player_desc: bool,
    /// Integer quality `0xC7`, `PropertyInt::HousePurchaseTimestamp`. The output is
    /// pre-zeroed and the return value is never tested, so a character without the quality reads
    /// `0`, which the purchase-wait check always calls expired.
    pub purchase_timestamp: i32,
    /// The purchase wait test — `real_time - t > 0x278D00`,
    /// strictly greater. Evaluated by the host because the clock is the host's.
    pub wait_expired: bool,
    /// What `localtime` would add before the panel's `strftime("%c")`; see
    /// [`GameView::utc_offset_secs`].
    pub utc_offset_secs: crate::ctime::UtcOffsetSecs,
}

/// The chess panel's visible state.
///
/// This carries whether the window is shown, board orientation and the piece-image
/// slot in each visible cell. `draws` counts redraws and supplies the change edge,
/// just as `house_data_notices` does for the house panel.
///
/// Like the other values exposed by [`GameView`], this is a read-only seam: screens
/// emit [`UiRequest`] rather than mutating the world. The host implements the game
/// view; total defaults let a limited host supply only the inputs its panels need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MiniGameView {
    /// Visibility as the begin-game / end-game notices last left it.
    pub visible: bool,
    /// The team. `-1` until a `Game_JoinGameResponse` names one; it decides the cell map.
    pub team: i32,
    /// The current game — zero when no game is joined.
    pub game: ObjectId,
    /// The game board's selected square, already mapped through the draw's index arithmetic.
    pub selected_cell: Option<usize>,
    /// One array index per display cell. Empty squares are `None`.
    /// The DIDs themselves stay layout-owned and are resolved by the panel.
    pub piece_slots: [Option<u8>; 64],
    /// Draw calls so far. Moves whenever the board changed.
    pub draws: u32,
    /// The stalemate flag — the Stalemate button is a toggle and this is its latch.
    pub stalemate: bool,
}

impl Default for MiniGameView {
    fn default() -> Self {
        Self {
            visible: false,
            team: 0,
            game: ObjectId(0),
            selected_cell: None,
            piece_slots: [None; 64],
            draws: 0,
            stalemate: false,
        }
    }
}

/// What the world's era means for what a front end can show: which state exists. Every front end
/// reads the same answer, whatever it looks like.
///
/// It is read from what the client has: the data files the world is drawn from, their tables, and
/// the account's Throne of Destiny flag from the character list. A system the era lacks leaves
/// its state absent (no rating arrives, no skill outside [`Self::skills`] is ever trained), so a
/// front end hides what has nothing behind it rather than drawing it empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EraView {
    /// The data files the world is drawn from: those from before Throne of Destiny
    /// (`portal.dat` and `cell.dat`, February 2005) or the later ones. The older files have no
    /// later interface records of their own.
    pub world_dats: dereth_primitives::ContainerEra,
    /// The account's Throne of Destiny flag (`Login_LoginCharacterSet`, `0xF658`). Without it
    /// the level shown stops at 126 and only the original heritages are offered.
    pub account_has_throne_of_destiny: bool,
    /// The highest level the world's experience table lists: 126 in February 2005, 275 at the
    /// end of retail. 0 before the table is read.
    pub level_cap: u32,
    /// The skills the world's skill table has, ascending: the pre-2013 weapon skills (Axe through
    /// Unarmed Combat) in February 2005, the consolidated ones (Heavy, Light, Finesse, Missile
    /// Weapons, ...) at the end of retail. Empty before the table is read.
    pub skills: Vec<u32>,
    /// The era the world plays. The server's announcement wins (the launcher passes the era its
    /// world's status names); without one it is read from the data files: those from before
    /// Throne of Destiny are February 2005's, the later ones the end of retail's.
    pub era: dereth_primitives::EraId,
    /// Whether [`Self::era`] is the server's announcement rather than read from the data files.
    pub era_announced: bool,
    /// The systems the server announces for its world (the launcher passes them with the era).
    /// Each one announced wins over the era's table; empty: the era's table alone.
    pub announced_features: dereth_primitives::EraFeatureOverrides,
}

impl EraView {
    /// The systems the world has: the server's announcement over the era's table. A front end
    /// hides the panels of those it lacks (ratings, aetheria, luminance, trade, ...) before any
    /// state arrives.
    #[must_use]
    pub fn features(&self) -> dereth_primitives::EraFeatures {
        self.announced_features.apply(self.era.features())
    }

    /// The era a world drawn from `world_dats` plays when the server names none.
    #[must_use]
    pub fn era_of_dats(world_dats: dereth_primitives::ContainerEra) -> dereth_primitives::EraId {
        match world_dats {
            dereth_primitives::ContainerEra::Classic => dereth_primitives::EraId::Infiltration,
            dereth_primitives::ContainerEra::Modern => dereth_primitives::EraId::Eor,
        }
    }

    /// Whether the world is the one from before Throne of Destiny.
    #[must_use]
    pub fn before_throne_of_destiny(&self) -> bool {
        self.world_dats == dereth_primitives::ContainerEra::Classic
    }

    /// Whether the world has skill `id`.
    #[must_use]
    pub fn has_skill(&self, id: u32) -> bool {
        self.skills.binary_search(&id).is_ok()
    }
}

/// The `PublicWeenieDesc` fields an item slot decorates itself with.
///
/// Each decoration field belongs to one display operation:
///
/// | field | reader |
/// |---|---|
/// | `stack_size` | the tooltip update |
/// | `is_container`, `items_capacity`, `contained_items` | the capacity display |
/// | `structure`, `max_structure` | the structure display |
///
/// `is_container` is the item's per-frame update's own three-term test —
/// `(_bitfield & 0x800000) || items capacity || containers capacity` — computed by the host,
/// because the object description bitfield is not otherwise on this seam.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlotDecoration {
    /// The stack size. **Absent and 1 are the same thing here**: the tooltip update maps 0 to 1
    /// before it compares, so this field carries the mapped value.
    pub stack_size: u32,
    /// The item-tile update's is-container test.
    pub is_container: bool,
    /// The items capacity.
    pub items_capacity: i32,
    /// The contained-items count — the item list's id count, the loose items
    /// only. The side packs are the contained-containers count and the capacity bar does not count
    /// them.
    pub contained_items: i32,
    /// `_structure`.
    pub structure: u32,
    /// The maximum structure.
    pub max_structure: u32,

    // ---- the tile's state flags and cooldown -------------------------------------------------
    /// Whether the object is selected, compared with the tile's cached copy before showing or
    /// hiding the selected-icon element.
    ///
    /// `dereth_client_model::Weenie::selected` is written by world selection.
    pub selected: bool,
    /// Whether the object's sell-state field is nonzero. See
    /// `crate::items::widget::ItemSlot::set_sell_state`. The original vendor window
    /// is its only writer; outside a vendor transaction it is normally false.
    pub sell_state: bool,
    /// Whether the object's trade-state field is nonzero. The original client writes
    /// it from trade, salvage and slumlord panels. This rebuild wires secure-trade
    /// writes; salvage and slumlord state-write delivery remains a gap.
    pub trade_state: bool,
    /// The `BF_OPENABLE` bit (1) in the public-description flags — one of the two terms of the item
    /// update's is-openable test.
    pub openable: bool,
    /// Whether `itemID` is the local player's id — the other term. The player's own icon is
    /// openable whatever its flags say, which is what makes the main pack clickable.
    /// This is also the icon renderer's player guard for backpack/type substitution.
    pub is_player: bool,
    /// The object's container capacity for the is-container-holder test. Kept apart from
    /// [`Self::items_capacity`] because the capacity **bar** counts loose items only.
    pub containers_capacity: i32,
    /// The object's cooldown id, with `0` for none. Decoded by `dereth-protocol`.
    pub cooldown_id: u32,
    /// The object's cooldown duration, in seconds. Likewise.
    pub cooldown_duration: f64,

    // ---- the icon background -----------------------------------------------------------------
    /// The object's `ITEM_TYPE` bit mask. The icon renderer selects the slot's **background tile**
    /// from the lowest set item-type bit plus one.
    ///
    /// It is the raw mask and not the resolved index, because the lowest-set-bit search's `-1` for a zero
    /// mask is what selects the group's own default row and that decision belongs with the
    /// transcription of the icon renderer — see
    /// `crate::items::widget::icon_background::enum_index`.
    /// The local player's recipe first substitutes TYPE_CONTAINER, as retail does.
    pub obj_type: u32,

    // ---- the icon composite ------------------------------------------------------------------
    /// The object icon id: blit **1** of the composite and the one layer this build already drew.
    ///
    /// It is carried here as well as through [`GameView::icon`] because the composite is built in
    /// `ItemListWidget::decorate` and a recipe
    /// assembled from two different sources at two different moments is a recipe that can
    /// disagree with itself. `0` is the client's `INVALID_DID`.
    pub icon_id: u32,
    /// The object's `UiEffects` bit mask. The lowest set effects bit plus one selects an
    /// effect-icon surface for a colour replace on the drag icon's opaque-white contour pixels.
    ///
    /// In the original client,
    /// the item element caches this mask without rendering the ring itself; icon
    /// construction applies the effect one level below the element. The picture is
    /// the same, but the owner differs.
    pub effects: u32,
    /// The overlay id for blit **2**, an alpha blit over the icon inside
    /// the drag icon.
    ///
    /// Written by `dereth_client_model::weenie::mirror_stat_update`.
    pub icon_overlay_id: Option<DataId>,
    /// The underlay id for an alpha blit over the type tile, below the
    /// completed drag surface. The effects surface, not this field,
    /// supplies the colour replace inside the drag surface.
    ///
    /// Same provenance as [`Self::icon_overlay_id`], and **no capture in the corpus carries one**,
    /// so nothing replayed can exercise it.
    pub icon_underlay_id: Option<DataId>,

    // ---- the waiting flag ----------------------------------------------------------------------
    /// The object's waiting flag: **the busy / in-use overlay**,
    /// the ghosted-icon element.
    ///
    /// It belongs here, beside `selected`, `sell_state` and `trade_state`, because it is the
    /// fourth of the four object flags the item's per-frame update mirrors onto its tile, and it
    /// is mirrored by the same edge-triggered compare as the other three:
    ///
    /// On each update, the client compares the object's waiting flag with the tile's cached flag.
    /// It updates the cache and the tile only when the value changes.
    ///
    /// **One mirror, not two.** A separate `Vec<bool>` beside the snapshot in
    /// `dereth_ui_screens::panels::inventory::InventoryPanels`, gathered for `items` + `equipment` only,
    /// misses the side-pack strip (the container list) and the main-pack slot (the top
    /// container): a **backpack** picked up in the container list would take the ghost at the
    /// list's begin-drag and nothing on this side of the seam could take it off again. Retail has
    /// one mirror, in the item-tile update's tail, and it runs for every tile of every list.
    pub waiting: bool,

    // ---- the shortcut number -------------------------------------------------------------------
    /// The shortcut number — **which quickbar slot this object is
    /// assigned to**, or `None` for none.
    ///
    /// The client spells "none" `-1` and this seam spells it `None`, so that a
    /// [`Default`]-constructed decoration means *no shortcut* rather than *slot 0*;
    /// `crate::items::widget::ItemSlot::set_shortcut_num` takes the client's `i32` and the
    /// mapping happens at that one call.
    ///
    /// The registry belongs to the object, not the bar. Adding or removing a shortcut
    /// sets the object's slot (using -1 for removal) without notifying the server.
    /// When the slot or ghosted flag changes, an item-attributes-changed notice carries
    /// `(id, 0)`. Every item tile compares both values with its cached copies on each
    /// update and applies both when either changes.
    ///
    /// That is why retail draws the numeral on the icon **wherever the icon appears** — worn on
    /// the paper doll, in the main pack, in a side pack — and not only in the bar slot.
    pub shortcut_num: Option<u32>,
    /// The shortcut-ghosted flag — the *dimmed* numeral.
    ///
    /// Its only writer in the client is the toolbar's set-combat-mode notice, which
    /// sets toolbar-active to `mode != 8` and then walks its eighteen lists
    /// applying the slot index and inverse toolbar-active flag to each item
    /// — and the setter's tail writes it through to the object, so
    /// the dimming reaches every other tile showing that object on its next item-tile update.
    /// A per-player fact, therefore, not a per-tile one.
    pub shortcut_ghosted: bool,
}

/// One row of one of the vendor window's three item lists.
///
/// The price is computed on the **host's** side of the seam by the vendor sell/buy price helpers;
/// this crate may not depend on
/// `dereth-client-model`, which is why the row carries a number and not a profile. The same
/// reason `AllegianceRoster` carries joined names rather than an `AllegianceHierarchy`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShopRow {
    pub item: ObjectId,
    /// The object's name.
    pub name: String,
    /// The object's icon id.
    pub icon: Option<DataId>,
    /// **`-1` is a vendor's "unlimited stock"**, and a positive value is a
    /// finite one that the buy basket can exhaust.
    ///
    /// The recorded corpus carries **30** stock rows across **8** decoded `0x0062`; **29** are `-1`
    /// and the thirtieth is
    /// `0x80000997` "Sack", `ITEM_TYPE 0x200`, `amount == 1`. That single row is what makes
    /// the items list's basket subtraction reachable on
    /// recorded traffic at all.
    pub amount: i32,
    /// The row's `ITEM_TYPE` **bit mask**, which is the
    /// only thing the vendor items list tests a row on.
    ///
    /// The whole filter is one bitwise test of the tab's mask against the row's type, where the
    /// type comes from the object's type query and from nothing else. The
    /// "does this list contain this type" helper asks the same question of the same field the
    /// same way. So it is a **mask intersection, not an equality**,
    /// and a row is kept when *any* bit of the tab's mask is set on it — which is what makes
    /// `0x20004000` ("Keys, Tools") and `0x490` ("Miscellaneous") work at all.
    ///
    /// Zero means "no `ITEM_TYPE` at all", and it matches **no** tab: `0 & mask` is zero and
    /// the row is skipped by every one of the eighteen. That is not a neutral default, so a
    /// fixture that leaves it 0 is describing a stock row that cannot be bought.
    pub obj_type: u32,
    /// The object's maximum stack size, read twice by the vendor items list.
    ///
    /// The item list reads it on both finite- and unlimited-stock paths. Both paths skip the
    /// stack-size write when the maximum stack size is 0 **or** 1, so the client
    /// never touches the object's stack size at all. On the finite arm it is also the clamp:
    /// the amount is capped at the maximum stack size.
    ///
    /// Read off the same object [`Self::obj_type`] is read off, through the same
    /// `dereth_client_runtime::vendor_view` fork, because retail reads both out of the one
    /// object lookup of `profile.iid`. An absent maximum stack size on the wire is 0,
    /// which is the "no stack size to set" arm and not a neutral default.
    pub max_stack_size: u32,
    /// The live contained-item count.
    ///
    /// A stock row is omitted when either its contained-item count or its contained-
    /// container count is nonzero: nonempty containers are not listed.
    ///
    /// These counts cannot come from `ItemProfile` or the `PublicWeenieDesc` in
    /// `0x0062`. They come from the live inventory stream and therefore must be
    /// resolved by the host. An absent inventory, or an object the client does not
    /// hold, contributes zero.
    pub contained_items: u32,
    /// The contained-containers count -- see [`Self::contained_items`].
    ///
    /// A separate function over a separate list (the containers list), tested separately,
    /// so it is a separate field: a pack holding only sub-packs is skipped by this one alone.
    pub contained_containers: u32,
    /// The vendor's sell price for a stock or buy-basket row; its buy price for a sell-basket row.
    pub price: i32,
    /// The client's refusal message, or `None` when the vendor takes it. Only ever set on a row
    /// the player could try to sell.
    pub refusal: Option<&'static str>,
}

/// The vendor window's whole state as one snapshot.
///
/// [`Self::open`] records whether the client has a nonzero vendor id; a closed shop is
/// [`Self::default`], and the panel must render that correctly rather than merely render it
/// emptily — see `dereth_ui_screens::panels::vendor`.
///
/// # The money is **four** writers, two sub-UIs and three numbers
///
/// In the client a *total* is not a basket. There are four writers, and they split two ways
/// rather than one:
///
/// | writer | reads | writes the element |
/// |---|---|---|
/// | buy transaction update | sum of vendor sell prices over the buy basket, including stack sizes | `0x100000C7` |
/// | buy purse update | the player's purse | `0x100000C8` |
/// | sell transaction update | sum of vendor buy prices over the sell basket | `0x100000D0` |
/// | sell purse update | the same player purse | `0x100000D1` |
///
/// Both purse updates show the player's money, not a basket total. The window
/// queries integer quality `0x14` (`PropertyInt::CoinValue`, decimal 20) with
/// enchantments applied. A missing player description or failed query yields 0.
/// The window's quality-change handler refreshes that value.
///
/// So the two purse fields are **one** number rendered into two elements, and the two transaction
/// fields are two different numbers. A test on which the transaction and the total coincide
/// cannot tell the four writers apart.
///
/// # The four strings, as literals
///
/// Each writer ends in the text element's setter with a **different** wide format, so
/// the four are separable by what a player reads and not only by which element was touched:
///
/// | writer | pyreal literal | alternate currency |
/// |---|---|---|
/// | the buy tab's transaction update | `L"Buying %d %s worth %hsp"` | `"Buying %d %s worth %d %s."`, the trade item's name |
/// | the buy tab's total update | `L"You have %hsp"` | a `sprintf` with the trade name |
/// | the sell tab's transaction update | `L"Selling %d %s worth %hsp"` | **none — the function has no currency fork** |
/// | the sell tab's total update | `L"You have %hsp"` | **none — no fork either** |
///
/// `%hs` is the number formatted as an integer string; the trailing `p` is a
/// literal `p` for pyreals. `%d %s` is [`Self::buy_items`] / [`Self::sell_items`] and the
/// `L"item"` / `L"items"` pair, chosen on `count == 1`. The **asymmetry
/// is real and is transcribed**: only the buy sub-UI has an alternate-currency display, which is
/// why a `trade_id != 0` shop shows the sell side in pyreals regardless. Both recorded vendors are
/// pyreal shops (`trade_id == 0`), so the corpus cannot witness either currency fork, and this
/// build implements the pyreal arm only.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShopView {
    /// Selected category position, retained between shops by the client.
    pub filter: usize,
    /// Whether the client has a nonzero vendor id.
    pub open: bool,
    pub vendor: Option<ObjectId>,
    /// The shop mode is sell — the window was opened by dragging an item onto the
    /// vendor, so the window starts on the Selling tab.
    pub sell_mode: bool,
    /// The vendor stock list, element `0x100000BD`.
    pub stock: Vec<ShopRow>,
    /// The buy basket, element `0x100000C5`.
    pub buy_list: Vec<ShopRow>,
    /// The sell basket, element `0x100000CE`.
    pub sell_list: Vec<ShopRow>,
    /// What the **buy basket costs**, recomputed by the buy-tab transaction update and drawn
    /// into `0x100000C7`.
    pub buy_transaction: i32,
    /// The sell transaction value — what the vendor would **pay for
    /// the sell basket**, drawn into `0x100000D0`. Written by
    /// the sell tab's transaction update.
    ///
    /// Its whole body is guarded by the shop vendor's object existing, and
    /// the zeroing precedes that guard — so a shop whose *vendor object* is not in
    /// the object tables shows **0** and does not touch the text element at all.
    pub sell_transaction: i32,
    /// The `%d` of `L"Buying %d %s worth %hsp"` — how many **things** the buy basket holds, which
    /// is the sum of selected quantities over the buy list.
    ///
    /// It is a separate accumulator from the transaction value in the same loop, and it decides the
    /// `"item"` / `"items"` fork on `count == 1`, so a station that only compared the
    /// money could not tell the two apart.
    pub buy_items: i32,
    /// The sell basket's item count, summing each offered object's stack size (at least one).
    pub sell_items: i32,
    /// The vendor window's total value — the **player's purse**, `PropertyInt` 20
    /// `CoinValue`, drawn into **both** `0x100000C8` and `0x100000D1` by the two
    /// total-value updates. See the type's own doc.
    pub total_value: i32,
    /// The client's type-filter menu (`0x100000BF`), in its own order:
    /// the subset of `dereth_client_model::vendor::TYPE_FILTERS` whose masks match an item type in the
    /// displayed stock. The host resolves each live object's type before filtering.
    ///
    /// Each surviving filter becomes a menu row with its label and an item-type mask stored
    /// under property `0x10000039`. Opening the vendor checks the eighteen candidate filters
    /// in table order.
    ///
    /// **Name *and* mask**, because the type-filter add's second act is to write the mask onto the row
    /// it just made; a strip of bare labels would lose the filter.
    /// The masks are `dereth_client_model::vendor::TYPE_FILTERS`', supplied by the host because this crate
    /// may not depend on `dereth-client-model` — the same seam rule that puts the prices on the
    /// host's side.
    pub type_filters: Vec<(&'static str, u32)>,
}

/// One row of either secure-trade item list.
///
/// The lists are ordinary item lists — `0x10000088` for yours and `0x10000081` for the partner's —
/// so a row is an object id, its icon and its name, exactly as a pack slot is. It carries no
/// price: nothing in a secure trade is priced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TradeRow {
    pub item: ObjectId,
    /// The object's name.
    pub name: String,
    /// The object's icon id.
    pub icon: Option<DataId>,
}

/// Secure-trade state as one snapshot.
///
/// [`Self::open`] is raised by `0x01FD Trade_RegisterTrade`. The `0x01FE` open
/// notice reaches an inherited no-op; `0x01FF Trade_CloseTrade` resets the
/// negotiation but leaves an empty window visible. Only local window close and
/// session reset clear the open flag. The panel must render both that visible
/// empty state and the closed [`Self::default`] state correctly.
///
/// The two accepted flags are the mirror's, so they are what the **server** has confirmed.
/// The button's own state belongs to the panel and is not here: it is a transition over the
/// button's previous state, not a function of the model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TradeView {
    /// The window is up.
    pub open: bool,
    /// The trade partner's object id.
    pub partner: Option<ObjectId>,
    /// The name writes into `0x1000007E`. Empty when
    /// the partner is unknown, which is what the client's null-string default draws.
    pub partner_name: String,
    /// The local player's offered-object list as rows — what the **server** has confirmed.
    pub self_rows: Vec<TradeRow>,
    /// The partner's offered-object list.
    pub partner_rows: Vec<TradeRow>,
    /// Whether the local player accepted.
    pub accepted: bool,
    /// Whether the partner accepted, shown by status indicator `0x1000007F`.
    pub partner_accepted: bool,

    // ---- removals ------------------------------------------------------------------------------
    /// **The ids the server has taken off *your* side of the table during this negotiation** —
    /// this seam's carrier for the window's remove-added-item.
    ///
    /// This is additional UI history, not a field in the received trade record. The
    /// original client handles removal as an immediate notice and needs no memory
    /// across frames. This panel instead rebuilds from a snapshot, and the optimistic
    /// row to remove is `dereth_ui_screens::panels::trade::TradePanel::pending`. A refused id is
    /// not in [`Self::self_rows`] and never will be. Without refusal history, that
    /// optimistic row would remain displayed indefinitely.
    ///
    /// **Its two producers are the remove-added-item's own two producers**; there are exactly
    /// **two**:
    ///
    /// | site | passes |
    /// |---|---|
    /// | the remove-item-from-trade notice, its `side == 1` arm | `0x0201 Trade_RemoveFromTrade` |
    /// | the trade-failure notice | `0x0207 Trade_TradeFailure` |
    ///
    /// The same arm of the remove-item notice sends `side == 2` to the partner-removal path
    /// instead, and any other side does nothing at all.
    ///
    /// **The window's server-says-attempt-failed notice is NOT one of them.** It clears the
    /// pending stack split and touches neither list,
    /// neither button nor the light. See the note on
    /// `dereth_ui_screens::panels::trade::TradePanel`'s module header.
    ///
    /// An id leaves this set when a `0x0200 Trade_AddToTrade` names it on side 1, so the set and
    /// [`Self::self_rows`] are disjoint and the panel's retain needs no ordering rule. The whole
    /// set is cleared by the trade model's reset and by the end-of-session hook, which is what
    /// makes it a fact about a **frozen world**
    /// — it moves only when a datagram lands — and therefore something a guard may compare.
    pub self_removed: Vec<ObjectId>,

    // ---- acceptance ----------------------------------------------------------------------------
    /// **The window has darkened both acceptance lights since the last acceptance message** —
    /// `dereth_client_model::trade::TradeSystem::acceptance_darkened`, which marks all four window changes:
    /// add my item, add partner item, remove added item, and remove partner item.
    ///
    /// This is the *"any change resets both acceptances"* rule, and it is a **window** rule, not a
    /// protocol one: the shard clears its own `TradeAccepted` on an add and sends nothing about
    /// it, so [`Self::accepted`] and [`Self::partner_accepted`] both stay set in the mirror while
    /// retail's two lights are already out. Drawing the lights off the mirror alone shows a lit
    /// "partner accepted" beside an offer the partner has since changed.
    ///
    /// `dereth_ui_screens::panels::trade::TradePanel::update` takes it together with its own `removed_now`,
    /// which stays because it covers the one case a model flag cannot: an **optimistic** row the
    /// mirror never held.
    pub acceptance_darkened: bool,
}

/// The trade button's agreement state, independent of its artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum TradeButtonState {
    /// Neither displayed list has an item.
    #[default]
    Disabled = 0x0D,
    /// The offer can be accepted.
    Enabled = 1,
    /// The displayed offer has been accepted.
    Accepted = 6,
}

impl TradeButtonState {
    /// Empty lists disable the button; adding rows enables it without accepting.
    #[must_use]
    pub fn with_displayed_rows(self, displayed: usize) -> Self {
        match (self, displayed) {
            (Self::Accepted | Self::Enabled, 0) => Self::Disabled,
            (Self::Disabled, n) if n > 0 => Self::Enabled,
            (state, _) => state,
        }
    }

    /// Toggle agreement using the counts the player actually saw.
    pub fn press(&mut self, displayed_self: usize, displayed_partner: usize) -> Option<UiRequest> {
        let request = match self {
            Self::Enabled => {
                *self = Self::Accepted;
                Some(UiRequest::TradeAccept {
                    displayed_self,
                    displayed_partner,
                })
            }
            Self::Accepted => {
                *self = Self::Enabled;
                Some(UiRequest::TradeDecline)
            }
            Self::Disabled => None,
        };
        *self = self.with_displayed_rows(displayed_self + displayed_partner);
        request
    }
}

/// Agreement controls for one displayed trade offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TradeControls {
    pub button: TradeButtonState,
    pub partner_accepted: bool,
    pub displayed_self: usize,
    pub displayed_partner: usize,
}

impl TradeView {
    /// Resolve the lights from the latest offer and the panel's displayed rows.
    /// An optimistic row removal darkens the lights even before the mirror held it.
    #[must_use]
    pub fn controls(
        &self,
        displayed_self: usize,
        displayed_partner: usize,
        removed_now: bool,
    ) -> TradeControls {
        let darkened = removed_now || self.acceptance_darkened;
        let button = if self.open && self.accepted && !darkened {
            TradeButtonState::Accepted
        } else {
            TradeButtonState::Enabled
        };
        TradeControls {
            button: button.with_displayed_rows(displayed_self + displayed_partner),
            partner_accepted: self.open && self.partner_accepted && !darkened,
            displayed_self,
            displayed_partner,
        }
    }
}

/// One spell-component row, using element template `0x10000467`.
///
/// The `wcid` is the row's identity: the panel stores it as **DataID attribute `0x1000004C`** and
/// the panel's selection-changed notice finds the row by matching it, so a row without one
/// cannot be selected. The other four fields are the four children the row template carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComponentRow {
    /// The component class id written into the row as `0x1000004C`.
    pub wcid: u32,
    /// The component name shown in child `0x10000469`.
    pub name: String,
    /// The component icon drawn in child `0x10000468`.
    pub icon: Option<DataId>,
    /// The owned item count in child `0x1000046A`, written by the panel's
    /// component-region sync.
    pub owned: i64,
    /// The desired component level for `wcid` — child `0x1000046B`, the **editable**
    /// field, written by the panel's buy-rate update.
    pub desired: i32,
    /// The first object id for the component, used by the component region — the
    /// object the element-message handler's selection arm selects.
    pub object: Option<ObjectId>,
}

/// One of the seven spell-component category buckets, with its rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComponentCategory {
    /// The spell-component category: 0 Scarab, 1 Herb, 2 PowderedGem, 3 AlchemicalSubstance,
    /// 4 Talisman, 5 Taper, 6 Pea. `Undef` (8) is **not** one of these — the client's panel walks
    /// `< 7` and never draws it.
    pub category: u32,
    pub rows: Vec<ComponentRow>,
}

#[cfg(test)]
mod tests;
