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
    /// The non-raw skill query for `id` — the **enchanted** value, which is `Update`'s
    /// comparison operand and what picks the buffed / debuffed font.
    ///
    /// It is **not** the skill base-level query's answer, which is the
    /// *attribute-formula contribution alone* (the base-level query is a
    /// formula over one or two attributes and nothing else). Comparing that
    /// against the skill lookup marks **every skill with any experience in it** as buffed: measured
    /// against a recorded first-login capture, exactly the character's 14 trained and
    /// specialised skills came out font 1 and the 24 untrained ones font 0, which is not a buff, it
    /// is a restatement of `_sac`.
    ///
    /// The right operand pair is the skill lookup with `raw = 1` — the number that is
    /// **displayed** — against the same lookup with `raw = 0`. The attribute row settles it: it
    /// computes the identical 0/1/2 index from **only** those two calls, with no base-level term in
    /// scope at all, and the secondary-attribute row does the same with the vitae modifier
    /// subtracted. The skill row does call the base-level query as well, and which values its
    /// comparison uses is **not established** — but the sibling function is unambiguous and the
    /// capture shows what the other reading produces. [inferred, cross-checked against the capture]
    ///
    /// `\[verified\]`, and it is also the number that gets *displayed*. The skill row reads
    /// the skill lookup with `raw=1` and with `raw=0` separately, and the
    /// comparison is the raw one against the enchanted one **less the vitae modifier**. The
    /// **string** the row prints is the *enchanted* value, not [`Self::level`]. See
    /// `crate::panels::skills::value_font`.
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
    /// [`crate::panels::inforegion::vitae_modifier`] computes it.
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
/// `dereth_client_model::advancement::{attribute_cost_to_raise, attribute_cost_to_raise_10}`,
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
    /// and in this crate's [`crate::panels::inforegion`].
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
    /// The spell's power level and its bitfield, which the spell icon is composed from (the
    /// level's background, the reversed wash and the fellowship or self badge), as the
    /// spellbook composes it.
    pub level: u32,
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
/// `dereth_client_model::advancement::vitae_cp_pool_threshold`, which lives in
/// `dereth-client-model`, and
/// `dereth-ui-screens` may not depend on that crate. The **level fall-back is part of it**:
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
    /// [`super::panels::characterinfo::melee_mastery_name`] names it.
    pub melee_mastery: i32,
    /// `InqInt(355 MissileMastery)`, named by
    /// [`super::panels::characterinfo::ranged_mastery_name`].
    pub ranged_mastery: i32,
    /// `InqInt(362 SummoningMastery)`, named by
    /// [`super::panels::characterinfo::summoning_mastery_name`].
    pub summoning_mastery: i32,
    /// Every **other** int inquires, by property id.
    ///
    /// A map rather than fifty named fields because the function is fifty copies of one shape:
    /// `v = InqInt(id)` (defaulted to 0 before the call, and the return ignored), `if v > 0`,
    /// emit one row with `v` as `%NumAugmentations`. The order on screen is
    /// [`super::panels::characterinfo::LUMINANCE`] then
    /// [`super::panels::characterinfo::AUGMENTATIONS`], which is where the ids live; an absent
    /// key and a `0` are the same answer, exactly as they are to the client.
    pub aug_ints: std::collections::BTreeMap<u32, i32>,
    /// What `localtime` would add to [`Self::created`] before the sheet's `strftime("%c")`.
    /// See [`GameView::utc_offset_secs`]. `0` renders the born line in UTC.
    pub utc_offset_secs: crate::ctime::UtcOffsetSecs,
}

/// The target-cursor mode.
///
/// The original client keeps this in shared UI state. The host stores it in
/// `dereth_client::interaction::Interaction`; the toolbar reads it through this seam
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
    /// An element of the paper doll, identified by its element id: one of the
    /// twenty-four equipment slots, or the drag mask `0x100001D6` over the figure itself.
    ///
    /// **The id, not a location, is what the client carries here too.** The paper doll's drop
    /// handler hands the same element id to both of its arms and lets
    /// the location lookup on the far side decide which runs —
    /// the slot accept for a slot, the body accept for the
    /// mask. The producer is
    /// `crate::panels::inventory::InventoryPanels::drop_target`; the mask's consumer arm is
    /// `dereth_client::interaction`'s `accept_drag_object`, which answers `false` for it and
    /// un-ghosts.
    EquipSlot(crate::ids::ElementId),
    /// The world viewport, which forwards element message `0x15`.
    World,
    /// A container object.
    Container(ObjectId),
    /// An inventory item-list slot, **resolved** into the four facts its drag accept
    /// reads from the element tree. The panel that filled the list has already
    /// translated element ids into object ids.
    ///
    /// [`Self::ItemList`] carries a list element and a slot number, which
    /// `dereth_client::interaction` cannot map to anything. Converting it to a bare
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
    /// `crate::panels::friends::APPEAR_OFFLINE_OPTION_BOX`.
    AppearOffline,
}

impl PlayerOption {
    /// Every option, in declaration order.
    ///
    /// It exists so [`GameSnapshot`](crate::GameSnapshot) can copy
    /// [`GameView::player_option`] and [`GameView::player_option_default`] for *all* of them:
    /// the two methods are keyed by this enum and an owned snapshot has no other way to
    /// enumerate the keys. The order is the declaration order and carries no wire meaning --
    /// the wire ordinal is `dereth_client::hud::option_ordinal`, which is a different number.
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

/// What the toolbar's selection-changed handler asks the game about the selected object
/// before it decides which vital query to send.
///
/// For a selection with fewer than two items in its stack, a **player**, a **pet**, or an
/// attackable object prompts a health query. A player or pet takes that branch without consulting
/// the attackability predicate. Other objects prompt a mana query only when the local player owns
/// them. All four facts remain separate because each affects this decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectionQueryFacts {
    /// The is-player test — `_bitfield` bit 3.
    pub is_player: bool,
    /// Whether the object has a nonzero pet owner.
    pub has_pet_owner: bool,
    /// The combat system's is-attackable test.
    pub attackable: bool,
    /// The is-owned-by-the-player test.
    pub owned_by_player: bool,
}

/// A `UserPreferences` value, using one of the four registered data types.
#[derive(Debug, Clone, PartialEq)]
pub enum PrefValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    Text(String),
}

/// Which of the allegiance panel's three confirmed actions a
/// [`UiRequest::AllegianceConfirmation`] asks about.
///
/// Break and kick both send `0x001E Allegiance_BreakAllegiance`, targeting the
/// patron and selected vassal respectively. They remain separate because their
/// prompts, enable rules and target lifetimes differ: break re-reads the patron
/// when the dialog closes, while kick latches the possible victim when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllegianceAction {
    /// Child `0x10000263` sends action `0x001D Allegiance_SwearAllegiance` for the selected object.
    Swear,
    /// `0x10000264` → `0x001E Allegiance_BreakAllegiance` on the player's patron.
    Break,
    /// `0x10000265` → `0x001E Allegiance_BreakAllegiance` on the selected vassal.
    Kick,
}

/// The two title representations accepted by floaty chat. A command supplies a
/// literal override; authored/default `PlayerModule` rows retain both table-reference
/// ids. A literal resembling `ID_*` must therefore remain literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatWindowTitle {
    Literal(String),
    Table { string_id: u32, table_id: u32 },
}

/// What a screen asks for. The binary routes these onward; nothing in this crate ever
/// sends a network message or mutates a world.
#[derive(Debug, Clone, PartialEq)]
pub enum UiRequest {
    /// Selects the named object.
    Select(ObjectId),
    /// Use the selected object with both optional arguments zero — the toolbar's Use button **with a selection**.
    Use(ObjectId),
    /// A targeted-use left click on an item-list row. The host reads its live mode and
    /// retained targeting source; this is deliberately not Use(selected, clicked).
    ExecuteTargetItem(ObjectId),
    /// Close the external container after use, then unregister its range handler.
    CloseExternalContainer(ObjectId),
    /// Hiding the slumlord panel retires every range registration
    /// owned by this panel, including a manual close before the one-shot exit fires.
    UnregisterSlumlordRange,
    /// Hiding the book panel retires every range registration owned
    /// by the reader, including a manual close before the one-shot exit fires.
    UnregisterBookRange,
    /// Examine the selected object — the toolbar's Examine button **with a selection**.
    Examine(ObjectId),
    /// Send `0x00C8 Item_Appraise` with a **zero** object id, the
    /// appraisal **cancel**.
    ///
    /// A variant of its own rather than `Examine(ObjectId(0))`, because in retail these are two
    /// different functions and the zero means the opposite thing in each:
    ///
    /// * The UI system's examine entry point sends an examine notice for a nonzero id and otherwise
    ///   arms the examine target mode. A zero there never reaches the appraisal sender, so it
    ///   sends nothing and arms the examine **cursor** — which is what [`Self::Examine`] means.
    /// * The examination panel's spell examine sends the appraisal event **directly**, with a
    ///   literal zero, bypassing
    ///   the examine entry point. It is the only producer of a zero-id appraise in retail.
    ///
    /// Raised by `crate::panels::examination::ExaminationPanel::examine_spell`, and only when
    /// one of the panel's two appraisal ids is set — the entry jumps straight to the
    /// fill when both are already zero, so a spell right-click with nothing in flight puts nothing
    /// on the wire.
    CancelAppraisal,
    /// A click on the paper doll's **body**, carrying the `INVENTORY_LOC` mask
    /// the paper doll's hit test resolved the pixel under the cursor to.
    ///
    /// The mask, not an object: the closing upper-inventory-object walk goes over the player's
    /// inventory-placement list with the placement priority rule and
    /// falls back to the player's own id, and the panel holds neither the list nor the player.
    ///
    /// `secondary` is action `8` rather than `7` — the two arms
    ///  the paper doll's element-message handler has for message `0x1C` on element
    /// `0x100001D6`, and the only two. A middle click, or any other action, does nothing at all.
    PaperDollRegion {
        mask: u32,
        secondary: bool,
    },
    /// Set the target mode — the toolbar's Use and Examine buttons with
    /// **nothing selected**, which is what arms the "use on…" cursor.
    ///
    /// The toolbar's element-message handler is explicit that the two are alternatives,
    /// not a pair: on `0x1000019d`, a selection is used and the handler returns; with none it sets
    /// the use target mode — and the same shape for `0x100001a5` with examine /
    /// the examine target mode. The mode is consumed by the **next** click in the
    /// world, through the target-mode executor.
    SetTargetMode(TargetMode),
    /// A completed drag. The panel does **not** move the item; it waits for
    /// `Item_ServerSaysMoveItem`: the client predicts nothing.
    DragDrop {
        item: ObjectId,
        target: DropTarget,
    },
    /// Remove the item shortcut and notify the server, raised by
    /// the begin-drag notice when a drag is picked up **out of** a shortcut
    /// slot.
    ///
    /// It is a request of its own and not a [`DropTarget`], because in the client it has no drop:
    /// The client's closing notice fires the instant the icon leaves the
    /// tile, and a release that lands nowhere the toolbar recognises leaves the shortcut removed.
    /// That is how dragging an item off the shortcut bar removes it; there is no code on retail's
    /// drop path that would do it.
    RemoveShortcut(ObjectId),
    /// Create a shortcut to the object in the first empty slot of the bar — the make-shortcut
    /// key (`0x1000010D`) on the selected object. The host runs the whole create: the
    /// eligibility and ownership gates, the sweep for an existing shortcut or a full bar (each
    /// refused with its message), then the add with server notification.
    CreateShortcut(ObjectId),
    /// Submit `text` from chat window `window` as a chat command.
    ChatLine {
        text: String,
        window: u32,
    },
    /// Sends `0x01E9 Character_RequestPing`.
    ///
    /// The link-status panel's update is the **only** thing in the client that sends it, and
    /// it sends it when the panel is opened and every 120 s while it stays open. Without it the
    /// `0x01EA` answer never arrives.
    RequestPing,
    /// Set `objectId`'s inscription to `text` with event `0x00BF`, the identify window's
    /// inscription box committing what was typed into it.
    ///
    /// Raised only by the inscription box when it loses keyboard focus. The two guards in front of
    /// it live in the panel — see
    /// `crate::panels::examination::ExaminationPanel::handle_inscription_losing_focus`.
    SetInscription {
        object: ObjectId,
        text: String,
    },
    /// Sends `0x0140`, the target name, literal status `1`, and complaint. The abuse panel's report
    /// button is its sole producer.
    AbuseLog {
        target: String,
        complaint: String,
    },
    /// Broadcast `text` on `channel` with communication event `0x0147`.
    ///
    /// Added for the urgent-assistance window. Its Send button is the original
    /// client's only producer on Help channel `0x400`: ordinary chat excludes that
    /// value, so `@help` is a command rather than a channel send. See
    /// `crate::panels::urgent_assistance::HELP_CHANNEL`.
    ///
    /// The variant carries a general channel/text pair because allegiance chat rows
    /// and talk-focus destinations use the same broadcast operation.
    ChannelBroadcast {
        channel: u32,
        text: String,
    },
    /// `0x00AC`, one book id.
    BookAddPage {
        book: ObjectId,
    },
    /// `0x00AB`, book, page, text.
    BookModifyPage {
        book: ObjectId,
        page: u32,
        text: String,
    },
    /// `0x00AD`, book and page.
    BookDeletePage {
        book: ObjectId,
        page: u32,
    },
    /// `0x00AE`, book and page.
    BookPageData {
        book: ObjectId,
        page: u32,
    },
    /// `0x00AA`, one book id.
    BookData {
        book: ObjectId,
    },
    /// The authored barber Apply button after native appearance generation. The host sends the
    /// exact `0x0311` body and waits for an authoritative appearance update.
    BarberFinish(BarberAppearance),
    /// Heritage-specific local player transformation script run immediately before FinishBarber.
    /// It changes particles only; the server's later object-description update owns appearance.
    BarberLocalEffect(DataId),
    /// Empyrean Earthbound/floating motion-table selection applied immediately before
    /// FinishBarber, without rebuilding the local player's setup.
    BarberLocalMotionTable(DataId),
    /// Set talk focus to row `n` — the thirteen-row talk-focus menu
    /// picked a new destination for the next line.
    ///
    /// `n` is the menu id, which is also the value switches on:
    /// 1 = public chat, 2 = a direct tell to the last speakable target, and 3..6 =
    /// a channel broadcast with channel `0x800`, `0x2000`, `0x4000`, `0x1000` respectively.
    /// 7..13 are the Turbine chat rooms. [verified against retail's chat-command talk-focus switch]
    ///
    /// It is a **local** setting, not a message: nothing goes to the shard when the menu changes,
    /// only when the next line is typed.
    SetTalkFocus {
        focus: u32,
    },
    /// The main chat window set its "Tell to &lt;name&gt;" target, the last speakable target:
    /// the object a line typed with talk focus 2 is told to, and the one its squelch row toggles.
    /// `ObjectId(0)` is none. The window's once-a-second sweep and its start-up reset are the only
    /// writers; a new selection is adopted only by the sweep, so the target is not simply the
    /// selection.
    SetLastSpeakableTarget {
        object: ObjectId,
    },
    /// Enable or disable talk-focus row `n` — a panel on this side of
    /// the seam decided that talk-focus row `n` is (or is not) a destination the player can pick.
    ///
    /// The three producers in the allegiance panel enable focus 4 for a logged-in patron, focus 5
    /// for a logged-in monarch other than the player, and focus 6 when any vassal is logged in. The mask
    /// itself lives in `dereth_client_model::chat::ChatState`, which this crate may not depend on,
    /// so the call crosses as a request and the host applies it; the menu row then follows
    /// through the ordinary `TalkFocusNotice` path, exactly as the chat system's own enabler
    /// broadcasts the enable-chat-target-selection notice after writing the flag.
    SetTalkFocusEnabled {
        focus: u32,
        enabled: bool,
    },
    /// Add or remove the character squelch for the given object, account, and message type —
    /// the talk-focus menu's **row 0**, the one with no `0x1000000B`, matched by pointer in
    /// the main chat window's element-message handler and answered by its
    /// squelch toggle for the current speakable target.
    ///
    /// `add` is the **negation** of the character's current squelch state for message type 1, which is
    /// what makes the row a toggle; `account` is the empty string and `message_type` is 1, i.e.
    /// "this character, all message types". Unlike [`UiRequest::SetTalkFocus`] this one *does* go
    /// to the shard: it is `0x0058 Communication_ModifyCharacterSquelch`.
    ModifyCharacterSquelch {
        object: ObjectId,
        add: bool,
        account: String,
        message_type: u32,
    },
    /// Add or remove an account squelch for `name` with event `0x0059`. This is the
    /// Squelch Account button's only message.
    ///
    /// Both producers are in `panels::squelch`: element `0x1000054C` adds the typed name;
    /// Remove element `0x10000547` removes the selected row's text when its text element
    /// has state `0x10000057`. The wire contains only the opcode, `add` dword and one
    /// narrow string. It has no object id or message type, so this variant has two
    /// fields while [`UiRequest::ModifyCharacterSquelch`] has four.
    ModifyAccountSquelch {
        add: bool,
        name: String,
    },
    /// The friends panel's add request -- the Add button, and the `@friends add`
    /// chat command's one hop.
    ///
    /// The **name**, and only the name: event `0x0018` has no id field, as the captures confirm. The
    /// 100-friend cap is *not* applied here -- it lives with the list, in
    /// `dereth_client_model::friends::MAX_FRIENDS`, and the panel uses the same number only to grey the
    /// button.
    AddFriend {
        name: String,
    },
    /// The client's `0x10000515` arm -- the Remove button, which reads
    /// `0x10000085` off the **selected row** and sends that id
    /// (`0x0017`).
    ///
    /// An id and not a name, which is the opposite of [`UiRequest::AddFriend`] and is retail's
    /// asymmetry rather than this build's.
    RemoveFriend {
        target: ObjectId,
    },
    /// The start-tell notice is broadcast to every
    /// notice handler and overridden by exactly one class, the main chat
    /// window, whose handler is a bare jump to the chat interface's start-tell.
    ///
    /// Three raisers in retail: the friends panel's element-message handler on its
    /// `0x10000516` arm (the Send Tell button, the text of the selected row's `0x1000051A`),
    /// the chat interface's action handler on its `0x10000119` arm (the selected object, when it
    /// is a player) and the main chat window's id-string-click notice (a click on a
    /// `<Tell:IIDString>` name in the log, when the entry is not already focused -- that one
    /// starts the tell directly rather than raising the notice).
    ///
    /// A notice, not a send: nothing goes to the shard. The receiver lives on the screen, so the
    /// host routes it in `UiShell::handle_request` the way `SetLockUi` travels.
    StartTell {
        name: String,
    },
    /// The contracts panel's element-message handler on its `0x100005DC` arm -- the **Abandon**
    /// button, which reads the selected contract id and sends the abandon-contract event
    /// (`0x0316`).
    ///
    /// The same shape as [`UiRequest::RemoveFriend`]: an id, and one send.
    ///
    /// **Two guards are the client's, and both are here rather than at the send site.**
    /// The client refuses a selected index of `-1` and refuses `contract_id == 0`, so
    /// nothing is emitted at all unless a row is selected and carries a real id.
    AbandonContract {
        contract_id: u32,
    },
    /// Apply one checkbox's new player-option value.
    ///
    /// The host changes that single bit in the received `PlayerModule` and re-packs
    /// the same module. Constructing fresh option words from this
    /// build's checkboxes would erase unmodeled bits: the second option word has bit 25
    /// set in all five recorded `0x01A1` bodies, although the original getter stops
    /// at bit 24 and no option-set mask exceeds `0x01000000`.
    ///
    /// `crate::options::character::CharacterSettingsPage::apply` emits it and
    /// `dereth_client::interaction` consumes it.
    SetPlayerOption(PlayerOption, bool),
    /// Query object health with event `0x01BF`.
    ///
    /// The toolbar's selection-changed handler is the only thing in the client that
    /// sends it, twice: a health query for `0` in the edge block, to stop the shard sending
    /// updates about the object just deselected, and a health query on the selected object
    /// inside the **not-a-stack** arm of the stack-size-below-2 branch. The answer, `0x01C0`, is
    /// what fills the selected object's health meter — without this request the meter draws a
    /// number nobody ever supplies.
    QueryHealth(ObjectId),
    /// Query the selected item's mana with event `0x0263` — the same handler
    /// and the same two sites, on the other side of that arm's own branch.
    QueryItemMana(ObjectId),
    /// The player option page's save-current-values **first line**:
    /// save the current player module with the immediate flag false.
    ///
    /// The page overrides its save-current-values step for exactly this, and its visibility change
    /// calls that step on **show**. So the whole
    /// module goes out when the page comes up carrying un-flushed changes, and not once per tick:
    /// a tick updates the option, and the module's changed hook only
    /// marks the module dirty unless the option is one of the twenty-one auto-save options.
    ///
    /// This is a request because the module, dirty flag and wire sender belong to the
    /// host, not the option page.
    SavePlayerOptions,
    /// Save the current keymap file without prompting, from the Key Bindings page's
    /// *OK* arm in its element-message handler when the page reports itself changed.
    ///
    /// The client's body sets the keymap file name, saves the keymap in its directory, updates the
    /// filename label, and saves preferences. With its prompt flag false and the current file name,
    /// the file-name assignment and dialogs are no-ops, so what is left for the host is the write —
    /// `dereth_client::input::InputShell::save_keymap`, the same writer the client's exit
    /// clean-up uses.
    ///
    /// This is a request because the host owns the input manager and keymap writer;
    /// the screen has no handle to them, just as [`Self::SavePlayerOptions`] has none
    /// to the `PlayerModule`.
    SaveKeyMap,
    SetPreference(&'static str, PrefValue),
    /// The sound portion of one media-playback step.
    ///
    /// The fields retain the step's file id and sound type. Type 0 (invalid)
    /// selects direct playback of a `0x0Axxxxxx` wave; other values select a row of a
    /// `0x20xxxxxx` sound table.
    ///
    /// UI-layout data determines which UI events play sounds. Of the twelve observed
    /// original sound-play calls, buttons can reach only the media machine's two
    /// paths. The producer is therefore `dereth_ui`'s media machine, handed across this
    /// seam by the shell; screens in this crate do not emit it.
    PlaySound {
        file: DataId,
        sound_type: u32,
    },
    /// Queue a UI mode change.
    QueueMode(UiMode),
    /// Set a panel's visibility — the *only* channel between a
    /// toolbar button and a panel page.
    SetPanelVisibility {
        panel: u32,
        visible: bool,
    },
    /// The radar's lock button state.
    SetLockUi(bool),
    /// Set one chat-window property — the per-window position,
    /// filter and opacity blob.
    SetChatWindowOption {
        window: u32,
        property: u32,
        value: i32,
    },
    /// The floaty-chat title property `0x1000008D`, retaining its literal or table-reference
    /// form. Separate from the numeric variant so a command title survives a rebuild.
    SetChatWindowTitle {
        window: u32,
        title: ChatWindowTitle,
    },
    /// Set chat-window property `0x1000007F` with a `Bitfield64` —
    /// the Chat Options page's per-window text-type filter.
    ///
    /// Separate from [`Self::SetChatWindowOption`] because that variant's payload is an `i32` and
    /// the filter is 64 bits wide (`ID_ChatOption_TextFilter_Society` alone is `0x100000000`), and
    /// because the receiver has to write a `BasePropertyValue::Bitfield64` rather than an
    /// `Integer` — `dereth_protocol::property::property_type(0x1000007F)` is `Bitfield64` and a wrong
    /// type is a silent no-op in the struct-element setter.
    ///
    /// **No datagram.** The setter's tail is the player module's changed hook,
    /// which raises the gameplay-option-changed notice locally and sets the deferred-save dirty flag;
    /// there is no `0x0005` for a chat option (the player-option-changed event is only reached
    /// from the changed hook's *player option* overload).
    SetChatWindowFilter {
        window: u32,
        mask: u64,
    },
    /// Set a floating-point gameplay option by property id —
    /// `0x10000080 Option_DefaultOpacity` or `0x10000081 Option_ActiveOpacity`, the Chat Options
    /// page's two general-section sliders.
    ///
    /// The client's named-property arm writes the **top-level**
    /// gameplay-option collection, not the per-window array, which is why one pair of sliders
    /// moves all five chat windows. Same deferred-save path as
    /// [`Self::SetChatWindowFilter`]; nothing goes on the wire here either.
    SetChatOpacity {
        property: u32,
        value: f32,
    },
    /// End the character session, optionally asking first — the indicator strip's log-out button and the
    /// Game/Support page.
    EndCharacterSession {
        ask: bool,
    },
    /// Notify the UI that the stack-slider size or maximum changed.
    StackSliderChanged {
        split: u32,
        max: u32,
    },
    /// The UI item's failed-owner drag completion.
    /// Clear only; it neither changes contents nor releases a request.
    ClearItemWaiting(ObjectId),
    /// The **pick-up** ghost, and the other half of [`Self::ClearItemWaiting`].
    ///
    /// The item list's begin-drag ghosts the icon the
    /// moment it leaves the slot, past the three list-kind refusals — a vendor list, a salvage
    /// list and a shortcut list each refuse it — and then sets the waiting state to 1.
    ///
    /// Vendor, salvage, and shortcut lists refuse this path. Every other item list sets the
    /// object's waiting state to true after the drag begins.
    ///
    /// The waiting-state setter writes the flag **onto the object**, not onto the element. It
    /// changes the object only when the requested flag differs, while the element keeps a mirror
    /// that its per-frame update re-derives and applies on every edge.
    ///
    /// **That is why the flag has to cross this seam.** The list flush clears each slot's local
    /// identity, geometry, and waiting mirror before every refill. In retail a refill therefore
    /// *cannot* lose a ghost: the authoritative flag was never on the slot.
    /// Here `ItemSlot::clear` does put `waiting` back to `false`, so a pick-up ghost that lived
    /// only on the widget would be wiped by the next `InventoryPanels::update` -- and that pass
    /// re-applies ghosts from `GameView::item_waiting`, i.e. from the object, which is where this
    /// request puts it.
    ///
    /// This crate has no access to the object table, so the write goes out as a request; it is
    /// applied inside the frame's own dispatch hook (`dispatch_ui_owner_requests`), before
    /// `Hud::drive` refills anything.
    SetItemWaiting(ObjectId),
    /// `ShellExecuteA("open", url)` from the Game/Support page.
    OpenUrl(&'static str),
    /// Shut down the device — the epilogue screen's only job.
    DeviceDone,
    /// The player opened (`true`) or closed (`false`) character creation: the one game phase a UI
    /// enters by asking (`crate::pregame::GamePhase::CharacterCreation`).
    CharacterCreation(bool),
    /// Leave the game: end the character session and then shut down, what the epilogue screen's
    /// [`Self::EndCharacterSession`] and [`Self::DeviceDone`] do between them, with no screen in
    /// between. Without the character session's end the account stays logged in on the server
    /// until its own timeout.
    Quit,
    /// A player-session operation the character-management screen asks for (log on, delete,
    /// restore). The screen has no session; the UI shell's request drain routes it to the host's
    /// character-action queue, in emission order.
    CharacterAction(crate::pregame::CharacterAction),
    /// What the character-generation wizard owes the session (the creation result, the
    /// post-creation log-on). Routed by the shell's drain like [`Self::CharacterAction`].
    CharGenAction(crate::pregame::CharGenAction),
    /// The character-management screen's selected character, mirrored into the UI flow's
    /// persistent selected avatar by the shell's drain before the frame's mode switch.
    SelectedAvatar(ObjectId),
    /// The character-generation slot, mirrored into the UI flow's persistent data by the shell's
    /// drain: the selected character's index in the character set (server order) when the
    /// character screen selects a row, `-1` when its selection is reset, and `-1` when the server
    /// refuses a creation. The creation request carries it.
    CharGenSlot(i32),
    /// Train a skill with `(skill, xp)` — `0x0046 Train_TrainSkill`.
    ///
    /// Both the skill panel's "raise" and its "raise 10" send **this**
    /// message; they differ only in the amount, which is the raise-1 cost or
    /// the raise-10 cost. There is no separate "raise ten" opcode.
    TrainSkill {
        skill: u32,
        xp: u32,
    },
    /// Train a skill's advancement class with `(skill, credits)` — `0x0047`.
    ///
    /// The *train* half: the raise path falls through to the panel's train call when
    /// the skill's `_sac` is below `TRAINED`, which raises a confirmation dialog whose callback
    /// is what sends this. This build has no dialog, so the
    /// request is emitted directly and the confirmation is missing.
    TrainSkillAdvancementClass {
        skill: u32,
        credits: u32,
    },
    /// Raise an attribute with `(attribute, xp)` -- `0x0045 Train_TrainAttribute`.
    ///
    /// The attribute panel's "raise" and "raise 10" both send
    /// **this** message for a row whose token reports stat type `8`; as on the skills page the
    /// two differ only in the amount, which is the raise-1 cost or the raise-10
    /// cost. There is no "raise ten" opcode.
    TrainAttribute {
        attribute: u32,
        xp: u32,
    },
    /// Send `0x0044 Train_TrainAttribute2nd` for a vital, from the same raise-one
    /// and raise-ten paths used for primary attributes.
    ///
    /// The answer is `Qualities_PrivateUpdateAttribute2nd` (`0x02E7`), carrying the
    /// whole `SecondaryAttribute` record. It shares `PropertySequenceGate` tag 9
    /// with regeneration ticks. `dereth_client_model::qualities::update::answers_a_raise_update`
    /// distinguishes the answer that releases the raise latch.
    TrainAttribute2nd {
        vital: u32,
        xp: u32,
    },
    /// Set the spellbook filter mask with event `0x0286`.
    ///
    /// The spellbook's filter update writes the new mask into the player module **and** sends it,
    /// and only when it actually changed.
    SetSpellbookFilter {
        mask: u32,
    },
    /// The spell bar's add-to-player-module — the request that makes
    /// magic combat reachable.
    ///
    /// The client updates the player module first and then sends event `0x01E3`. Model **and**
    /// wire move in that order from one call, which is why this is a single request
    /// and not a "move the icon" plus a "tell the shard". A spell bar is persistent server state:
    /// a client that inserted the row and sent nothing would look correct until the relog.
    ///
    /// `index` is the spell bar's own, already adjusted: a drop names the
    /// row it landed on, an append names the spell count **after** its own increment (one past the
    /// last row, which the packable list resolves to a tail push), and a move
    /// that came from earlier in the same tab has had one subtracted.
    AddSpellFavorite {
        spell_id: u32,
        index: i32,
        tab: usize,
    },
    /// The spell bar removes the favorite from the local model, then sends event `0x01E4`.
    ///
    /// Raised by the bar's remove-from-menu, which is reached two ways: the bar's own
    /// begin-drag notice (dragging a spell *off* the bar) and
    /// the first half of every add-favorite — so re-dropping a spell already on the tab sends
    /// this and then [`UiRequest::AddSpellFavorite`], which is how retail spells "move".
    RemoveSpellFavorite {
        spell_id: u32,
        tab: usize,
    },
    /// Remove the spell with event `0x01A8` — the spellbook's DELETE button.
    ///
    /// The spellbook's delete button does not remove anything: it raises a confirmation
    /// dialog carrying the selected spell id under property `0x1000003F`, and
    /// the dialog's callback sends this when `0x92` came back true. The row
    /// leaves the list when the shard echoes the removal, not when the button is pressed.
    RemoveSpell {
        spell_id: u32,
    },
    /// Notify the UI that an item list now has a different parent container.
    ///
    /// The setter broadcasts only when the parent actually changed: it checks the old
    /// id before writing and uses that result again afterward. The listener stores
    /// the open-container id, which the pickup destination prefers over the player.
    ///
    /// This is a request because the panel is inside element-message dispatch and
    /// cannot borrow the mutable world directly.
    NewParentContainer(ObjectId),

    // ---------------------------------------------------------------------------------------
    // The vendor window's button handler and its eleven `case`s, as the four
    // distinct things they do. The element ids are the cases themselves and live in
    // [`crate::panels::vendor`]; a request carries what the button decided, not which button.
    // ---------------------------------------------------------------------------------------
    /// Buy a single item — cases `0x100000C2` ("Buy") and `0x100000C9`
    /// ("Buy Item"). **This sends `0x005F` on its own**, with a one-entry list; it does not fill
    /// the basket. `split` is the current stack-slider answer.
    VendorBuySingle {
        item: ObjectId,
        split: i32,
    },
    /// case `0x100000C3` ("Add to List"). Local only.
    VendorAddToBuyList {
        item: ObjectId,
        split: i32,
    },
    /// Set the vendor row's object stack size -- and it is not a
    /// UI call at all.
    ///
    /// It divides the description value by the old stack size (using 1 when that size is zero),
    /// multiplies by `size`, then stores the scaled value and new stack size.
    ///
    /// This changes the local copy of a vendor stock object. Its stack size limits
    /// how much one Add to List action can cover; rescaling value by the same factor
    /// keeps the per-unit price unchanged. The Add to List handler uses the split
    /// slider amount when stack size is at least 2, otherwise 1. Without this write,
    /// a stackable row always adds one item regardless of the slider.
    ///
    /// `crate::panels::vendor::VendorPanel::update_items_list` emits this once for
    /// each listed row whose maximum stack size exceeds 1. The host owns and changes
    /// the description.
    VendorSetObjectStackSize {
        item: ObjectId,
        size: i32,
    },
    /// A buy shop event over the whole basket — case `0x100000CA` ("Buy All").
    VendorBuyAll,
    /// Sell a single item — case `0x100000D2` ("Sell Item"), which also
    /// sends on its own.
    VendorSellSingle {
        item: ObjectId,
    },
    /// Sell the complete sell list to the current vendor — case `0x100000D3` ("Sell All"). This is
    /// the shape the corpus's one recorded `0x0060` has: **two rows in one message.**
    VendorSellAll,
    /// Remove one basket item for `0x100000CB` / `0x100000D4` (Clear Item), or the
    /// whole list for `0x100000CC` / `0x100000D5` (Clear List). `item` is `None`
    /// for the whole-list form.
    VendorClearList {
        sell: bool,
        item: Option<ObjectId>,
    },
    /// Close the vendor window — case `0x100000D6`, the close button.
    VendorClose,
    /// The sell list's drag accept — **an item dragged onto the sell list**, which
    /// is the one thing this window takes that is not a button.
    ///
    /// The vendor window's drop release is the same "dropped inside the sell list" + drop-icon
    /// info + drop-item alias shape
    /// the secure-trade window's drop release has, and this is its
    /// [`UiRequest::TradeAddItem`]. The refusals, the marks and the basket are
    /// owned by the game world's add-to-sell operation, because every one of them needs the object table:
    /// ownership by the player, the contained-item count and the vendor profile's acceptability
    /// test.
    ///
    /// **Local only.** Nothing goes on the wire until "Sell Item" or "Sell All"; the client's own
    /// drop-acceptance check sends nothing either.
    VendorAddToSell {
        item: ObjectId,
    },
    /// The sell list's drop with only part of a stack dialled in. The host asks for the split
    /// and puts the source on the list as the row's placeholder; the object the server makes
    /// takes that row when it arrives. `split` and `max` are the splitter as the drop left it.
    VendorSplitToSell {
        item: ObjectId,
        split: u32,
        max: u32,
    },

    /// The client's message `0x2F` arm — the player
    /// committed a new number in a component row's edit field `0x1000046B`.
    ///
    /// The host stores the desired component level and sends event `0x0224`. Out of range
    /// (`level < 0 || level >= 0x1389`) is not a
    /// send and the panel reverts the field, so the request is raised only for a value the panel
    /// has already bounds-checked -- the host checks again, because the bound is enforced three
    /// times in the client and this crate is not the authority on it.
    SetDesiredComponentLevel {
        wcid: u32,
        level: i32,
    },
    /// Fill the component list for a category and maximum price -- the "buy the components I
    /// am short of" helper. `category` is `None` for the client's
    /// undefined spell-component category, which means every category; `max_price` of 0 is no
    /// limit.
    VendorFillComponents {
        category: Option<u32>,
        max_price: i32,
    },

    /// Cast `spell_id` with the from-UI flag set — and
    /// the request that makes a player able to cast a spell at all.
    ///
    /// Raised by `crate::panels::spellcasting::SpellcastingPanel::cast`, which is
    /// the spellcasting panel's cast — the **only** caller `CastSpell` has in the shipped
    /// binary. Everything the client decides after that point (the component check, the
    /// self-targeted branch, the target compatibility test and which of `0x0048` /
    /// `0x004A` goes on the wire) belongs to the game world's spell-casting operation and is deliberately
    /// not duplicated here: this crate carries the click, not the rules.
    CastSpell {
        spell_id: u32,
    },

    /// The spell research page's Test: the formula's components (component weenie class ids, in
    /// the order they were laid) tried on the selected target. See [`crate::research`].
    TestSpellFormula {
        components: Vec<u32>,
    },

    /// Put `item`, one of the player's own, on the ground: the drop-selection key and a drag out
    /// of a pack onto the world.
    PutInWorld(ObjectId),
    /// Enter a combat mode (`1` peace, `2` melee, `4` missile, `8` magic), or with `0` the mode
    /// the wielded weapon calls for; refused, with the world's own words, when the player cannot
    /// change mode now.
    SetCombatMode(u32),
    /// Wear `item` where it goes, as a double click on armour or clothing does.
    AutoWear(ObjectId),
    /// Wield `item` in its slot; `side` picks a hand for a one-handed item (`0` either, `1` left,
    /// `2` right).
    AutoWield {
        item: ObjectId,
        side: u32,
    },
    /// The privileged player's map teleport: to the middle of the outdoor block at (`x`, `y`) in
    /// the 2040-block world grid.
    MapTeleport {
        x: u32,
        y: u32,
    },
    /// Ask the server for the house the player owns.
    QueryHouse,
    /// Open trade negotiations with `partner`; refused outside peace mode.
    OpenTrade(ObjectId),
    /// Switch the abuse log on `target` on or off, with the complaint.
    AbuseLogStatus {
        target: String,
        enabled: bool,
        complaint: String,
    },
    /// Set both character option words at once (the classic interface's Character page), and
    /// the chat timestamp format; `save` sends them to the server.
    SetOptionWords {
        options: u32,
        options2: u32,
        timestamp_format: Option<String>,
        save: bool,
    },

    /// The combat system's set-requested-attack-height — the arm
    /// that makes the combat window's three attack-height buttons do anything.
    ///
    /// Raised by `crate::hud::combat_window::CombatWindow::on_element_message`'s `0x1C`
    /// (mouse-press) arm and by `dereth_client::Interaction::on_actions`' `CombatLow/Medium/High
    /// Attack` and `CombatAimLow/Medium/High` keys, which are the same act: the combat action
    /// handler and the combat window's element-message handler call the one function.
    ///
    /// `height` is `ATTACK_HEIGHT` (`HIGH` 1, `MEDIUM` 2, `LOW` 3).
    CombatSetAttackHeight {
        height: u32,
    },

    /// End the attack at `height` with the power override set to `-1.0` — the
    /// release half. `-1.0` is "use what the bar says", which is why the button is a *held*
    /// control: press charges, release swings.
    CombatEndAttack {
        height: u32,
    },

    /// The combat window's element-message handler on its `0x0A` arm — the
    /// power/recklessness gauge. `position` is the scrollbar position in thousandths, read back
    /// unsigned exactly as the client's integer-to-float conversion does; the clamp to `[0, 1]`
    /// belongs to `dereth_client_model::CombatState::set_ui_requested_power_from_scrollbar`.
    CombatSetDesiredPower {
        position: u32,
    },

    // -----------------------------------------------------------------------------------------
    // Secure trade. The panel carries the clicks; the rules beneath
    // them belong to `dereth_client_model::trade`.
    // -----------------------------------------------------------------------------------------
    /// Add an item — an item dropped on the table.
    ///
    /// `position` is the row's index in the window's **own** self list and becomes the second dword
    /// of `0x01F8 Trade_AddToTrade`. It
    /// is the panel's number because the list is the panel's.
    TradeAddItem {
        item: ObjectId,
        position: u32,
    },
    /// The partial-stack arm. The source is not inserted; these splitter values
    /// are used to request an authoritative result.
    TradeSplitItem {
        item: ObjectId,
        split: u32,
        max: u32,
    },
    /// The client's unequal-stack arm. Retail asks the
    /// inventory holder to split this many beside the source; the resulting object is selected by
    /// the ordinary authoritative create path and is not inserted into the payment list until the
    /// player drops it a second time.
    HouseSplitItem {
        item: ObjectId,
        split: u32,
        max: u32,
    },
    /// Accept the trade — the trade button pressed at state **6**.
    ///
    /// The two counts are what the window is showing. They are carried rather than re-derived
    /// because the comparison against `dereth_client_model`'s mirror **is** the client's whole share of the
    /// anti-scam protocol, and a comparison of a value with itself would assert nothing.
    TradeAccept {
        displayed_self: usize,
        displayed_partner: usize,
    },
    /// Decline the trade — the same button pressed at state **1**.
    TradeDecline,
    /// Reset the trade — the "Clear All Items" button, `0x1000008A`.
    TradeReset,
    /// The client's hide arm — the close button
    /// `0x1000008B` hides the window, and hiding it is what sends
    /// `0x01F7 Trade_CloseTradeNegotiations`.
    TradeClose,

    /// Enable or disable allegiance updates with `0x001F Allegiance_UpdateRequest`, the
    /// subscribe toggle the shard answers with `0x0020 Allegiance_AllegianceUpdate`. Without it
    /// the allegiance panel stays empty.
    ///
    /// The allegiance panel has **five** call sites for it — its post-init, its
    /// player-description notice, its quality-changed handler and both arms of its
    /// visibility change — and this build emits the visibility pair. The shard is
    /// the only source of a roster: it answers this request, or pushes on a change to an
    /// allegiance it has already been asked about. A client that never sends it sees nothing, for
    /// ever, and cannot tell that apart from belonging to no allegiance.
    ///
    /// In the three recorded fellowship sessions, where the client half is **retail's**, there
    /// are 31 `0x001F`, 28 with `on = 1` and exactly one `on = 0` per session, every one of the 31
    /// answered within milliseconds by `0x0020` and then `0x01C8`.
    AllegianceUpdateRequest {
        on: bool,
    },
    /// A window started (`raised`) or stopped waiting on the server's answer to something it
    /// asked for: the busy count goes up or down by one, and the pointer is the hourglass while
    /// it is not zero. The allegiance panel's busy latch is the one window that raises it.
    Busy {
        raised: bool,
    },

    /// The allegiance panel's three confirmation dialogs: the question, before any
    /// send.
    ///
    /// The panel's element-message handler on message `1` does nothing but raise a dialog:
    ///
    /// ```text
    /// 0x10000263 -> the swear confirmation dialog
    /// 0x10000264 -> the break confirmation dialog
    /// 0x10000265 -> the kick confirmation dialog
    /// ```
    ///
    /// and the game action goes out of the *close* callback of the break and the kick dialogs
    /// rather than out of the button.
    /// So this request carries no opcode: it asks the host to put a question on screen, and the
    /// host sends `0x001D` / `0x001E` if and only if the player answers yes. A build that sent
    /// straight from the button would swear or break on one click, which retail never does.
    ///
    /// `prompt` is resolved here rather than by the host because the three
    /// `ID_Allegiance_*Confirmation` strings live in this crate's string table
    /// (`crate::panels::allegiance::STRING_TABLE`) and the host has no reader for it.
    AllegianceConfirmation {
        action: AllegianceAction,
        target: ObjectId,
        prompt: String,
    },

    /// Set the displayed character title to `id` with event `0x002C`, four bytes of
    /// body — the Titles tab's only outbound message.
    ///
    /// The titles panel's element-message handler is the **only** caller in the
    /// image, and it is reached by one gesture: a click on the *"Set as Display Title"* button
    /// `0x10000535`, which reads `0x1000008E` off the row the list box has selected. Selecting a
    /// row sends nothing.
    ///
    /// Nothing local moves when it goes out. The worn title is server state: the answer is
    /// `0x002B Social_AddOrSetCharacterTitle` with `set_as_display_title` set, or a fresh
    /// `0x0029`, and that is what moves the header and re-enables the button.
    SetDisplayCharacterTitle {
        title_id: u32,
    },

    // -----------------------------------------------------------------------------------------
    // Fellowship UI. Every one of the seven is a fellowship event the
    // panel calls directly in the client; the request queue is this build's stand-in for that
    // call, and `dereth_client_model::World`'s own methods hold the guards.
    //
    // The reference for all of them is the client half of the three recorded fellowship
    // sessions, which is **retail's client** and carries 27 of these.
    // -----------------------------------------------------------------------------------------
    /// Enable or disable fellowship updates with event `0x00A6`, and the whole of the fellowship
    /// panel's visibility change.
    ///
    /// The subscribe toggle for the **live vitals feed**: `0x02C0 Fellowship_UpdateFellow`
    /// arrives while it is on and stops when it is off. In the recorded three-vassal session the
    /// `on` goes out at t=259.371, the `off` at t=365.392, and all sixteen `0x02C0` of that session
    /// fall between the two.
    FellowshipUpdateRequest {
        on: bool,
    },
    /// Create the named fellowship with the requested XP-sharing state — event `0x00A2`, the Create button `0x10000274`.
    ///
    /// `share_xp` is the state of the check box on this same tab, read at
    /// the moment of the click, not a property of the fellowship being made.
    FellowshipCreate {
        name: String,
        share_xp: bool,
    },
    /// Quit or disband the fellowship with event `0x00A3`. **Both** leave buttons send it: the Quit
    /// button `0x1000027C` with `disband = false`, the Disband button `0x10000280` with `true`.
    FellowshipQuit {
        disband: bool,
    },
    /// Dismiss member `id` with event `0x00A4`, the Dismiss button `0x1000027F` on the
    /// **selected row**.
    FellowshipDismiss {
        target: ObjectId,
    },
    /// Recruit target `id` with event `0x00A5`, the Recruit button `0x1000027E` on
    /// i.e. whoever is selected **in the world**, not in the list.
    FellowshipRecruit {
        target: ObjectId,
    },
    /// Assign target `id` as leader with event `0x0290`, the Leader button `0x1000027B` on
    /// the selected row, and also the first half of a *leader's* Quit.
    FellowshipAssignNewLeader {
        target: ObjectId,
    },
    /// The change-fellowship-openness request — `0x0291`, the Open/Close button
    /// `0x1000027D`. The client flips its own `_open_fellow` first and sends the result, so the
    /// caption changes on the click; the host does the flip because the flag is `dereth_client_model`'s.
    FellowshipToggleOpenness,
    /// The salvage window's one send: process the selected salvage operation.
    /// (`0x027D`).
    ///
    /// `items` is the window's list **reversed**: the collecting loop walks
    /// the last row down to row 0 and appends each, so the last row dropped is the
    /// first id on the wire. Carried in the request rather than re-derived at the send site,
    /// because the order is a property of *this* gesture and of the window that owns the list.
    ///
    /// Both of the client's guards are the panel's own -- a non-empty list and a
    /// non-zero tool id -- and the game world's tinkering-tool operation repeats
    /// them because a chat command could reach it without passing through the panel.
    SalvageItems {
        tool: ObjectId,
        items: Vec<ObjectId>,
    },
    /// The mini-game window's element-message handler, message-id `1` arm:
    /// one of `0x10000175` Resign, `0x10000176` Pass, `0x10000177` Stalemate.
    ///
    /// The id travels rather than a decoded intent because retail's own three arms are a
    /// subtract-and-decrement ladder on the id and each has its own guard; the decision is
    /// `dereth_client_model::minigame::MiniGame::on_button`'s, which is the transcription of that ladder.
    MiniGameButton(u32),
    /// The game board's mouse-press handler — a left press
    /// (action `7`) on one of the board's 64 cells, as the list-box index answers it.
    /// The index→square map is the *model's*, because it needs the team.
    MiniGameBoardPress(usize),
    /// The mini-game quit dialog's callback answer —
    /// the quit-game notice's `bool`, whose Yes is the only `0x026A` in retail.
    MiniGameQuitAnswer(bool),
    /// Display `text` on `channel` -- a line straight into the scroll,
    /// which `dereth_client_model::scroll::Scroll::on_display_string_info` already consumes.
    ///
    /// Used by the salvage panel's two channel-`0x1A` lines: the heading printed
    /// before walking a dropped pack, and "You can only salvage items that you own!"
    /// This differs from [`UiRequest::ChatLine`], which is player input parsed for
    /// slash commands before it reaches the scroll.
    DisplayChatText {
        channel: u32,
        text: String,
    },
    /// The slumlord window's payment request (`0x021C`).
    /// and the rent request (`0x0221`), which of the two chosen by the current house operation.
    ///
    /// `items` is the **current** item list read top to bottom (row `0` to the last
    /// row), each row's object id, skipping a zero — the
    /// opposite order from [`UiRequest::SalvageItems`], which walks its list backwards. A recorded
    /// refused house purchase shows the drop order preserved on the wire at `t = 144.458`:
    /// `3af0da79 03000000 c2150080 c1150080 92150080`.
    ///
    /// Both of the client's guards are the panel's own: an op that is `Buy` or `Rent` and a
    /// non-empty list, plus a non-zero owner id.
    HousePayment {
        slumlord: ObjectId,
        rent: bool,
        items: Vec<ObjectId>,
    },
    /// The slumlord window's buy confirmation dialog, or its
    /// rent-by-proxy confirmation dialog. The panel keeps the payment rows;
    /// the gameplay dialog owner supplies the shipped confirmation dialog and routes its answer
    /// back through [`UiRequest::HousePaymentConfirmationAnswer`].
    HousePaymentConfirmation {
        rent: bool,
    },
    /// The matching completion. Property `0x8E` identifies
    /// the dialog kind; the Boolean answer itself is property `0x92`. `None` means creation or
    /// framework teardown supplied no answer and only releases the panel's context guard.
    HousePaymentConfirmationAnswer {
        rent: bool,
        confirmed: Option<bool>,
    },
    /// Query `target`'s landlord with event `0x0258`, and it is a **retry**, not an opener.
    ///
    /// The slumlord window's failed-transaction notice is retail's only sender:
    /// when it holds a house profile, a query-lord request on the owner. A failed `0x021C`/`0x0221` arrives as
    /// `0x0226`/`0x0259`, and the window re-asks the lord so its prices and paid counts come back
    /// in step with the shard's.
    HouseQueryLord {
        slumlord: ObjectId,
    },
}

/// One fellowship member projected for its panel row.
///
/// The panel never sees a `Fellowship`: the vitals are drawn from the six words directly and the
/// experience share is a **percentage the host computed**, because the two functions behind it —
/// the fellowship's experience-proportion sum and the per-member experience proportion — need
/// the `XpTable`, which is a dat object this crate must not know about. This is the same declared
/// crossing as [`SkillEntry`]'s name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FellowEntry {
    /// The `PackableHashTable` key, and the value of row attribute `0x1000000D`.
    pub id: ObjectId,
    /// The name variable of `ID_Fellowship_FellowName`.
    pub name: String,
    /// The level variable of `ID_Fellowship_FellowStats`.
    pub level: u32,
    /// The experience variable of `ID_Fellowship_FellowStats`, as a **whole percent**.
    ///
    /// The fellowship panel's stat update computes a float and then multiplies by `100.0f` and
    /// truncates, so the number in the box is truncated toward zero.
    /// The float is:
    ///
    /// * `0.0` when the share-XP flag is clear;
    /// * the even-split share for the current member count when `_even_xp_split` is set;
    /// * this member's level-based XP proportion divided by the sum of all members'
    ///   proportions otherwise.
    ///
    /// \[measured\]
    pub xp_percent: i32,
    pub current_health: u32,
    pub max_health: u32,
    pub current_stamina: u32,
    pub max_stamina: u32,
    pub current_mana: u32,
    pub max_mana: u32,
}

/// What the fellowship panel shows when a fellowship exists.
///
/// `None` from [`GameView::fellowship`] means that no current fellowship exists, which
/// is the whole of the client's first branch: the *Not In A Fellowship*
/// frame is shown, the *In A Fellowship* frame is hidden, and the panel registers for global
/// message 3 so it keeps polling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FellowshipView {
    /// The fellowship name, written into the fellowship-name text (`0x10000276`) as a **literal**
    /// rather than a string-table token.
    pub name: String,
    /// The leader id. The panel compares it against the player's own id
    /// and that comparison decides five of the six buttons.
    pub leader: ObjectId,
    pub share_xp: bool,
    pub even_xp_split: bool,
    /// `_open_fellow` — which caption the Open/Close button carries, and whether a non-leader may
    /// recruit at all.
    pub open_fellow: bool,
    pub locked: bool,
    /// The members, in the order the panel walks them. Retail walks a `PackableHashTable` in
    /// bucket order; the mirror behind this is a `BTreeMap`, so this is **id order** and the rows
    /// are stably sorted rather than arbitrarily so.
    pub members: Vec<FellowEntry>,
}

/// The worn title and every earned title, with names already resolved for the
/// titles panel.
///
/// The join is the host's for the reason [`SkillEntry`]'s name is: it maps the title id through
/// enum `0x10000006`, then hashes the resulting token into string table `0x10000007`, i.e. two dat
/// objects this crate must not know about. Both are loaded at startup already — they are the same
/// pair `display_title()` uses.
///
/// A title whose lookup fails carries the **empty** name rather than being dropped, so that
/// `TitlesPanel::unresolved` can count it: an id the shipped tables do not carry and an id that
/// was never sent look identical once the row is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterTitles {
    /// The displayed title. `0` is "no title", which the title-name lookup refuses outright.
    pub display: u32,
    /// The earned titles, in **arrival** order — the panel does its own alphabetical insert.
    pub titles: Vec<(u32, String)>,
}

/// One friends-list row.
///
/// The received friend record has six fields, but the panel needs only three: name
/// for the text child, id for attribute `0x10000085`, and online state for the text
/// element state. The appear-offline flag and the two friendship-id lists remain
/// in `dereth_client_model::player::Friend`; the panel does not read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FriendEntry {
    /// The value of row attribute `0x10000085`, which is what the Remove button reads back.
    pub id: ObjectId,
    /// The name written into the row's `0x1000051A` text child.
    pub name: String,
    /// Whether the friend's online flag is nonzero. Decides the row's text state -- `0x10000054` when true,
    /// `0x10000055` when false -- which is in turn what the Tell button and the `@friends online`
    /// listing both read back off the row.
    pub online: bool,
}

/// One squelch-list row.
///
/// The wire `SquelchInfo` has three fields; this view carries only the name and
/// account flag that determine text child `0x10000542` and row state. The iterator
/// uses the 128-bit channel mask only to test emptiness before yielding a row.
/// The mask stays in `dereth_client_model::chat::SquelchEntry` for the chat router.
///
/// There is **no id**, deliberately. The row attribute `0x1000008F` is written with a literal
/// zero by the panel's one production caller and nothing ever reads it back; the Remove button
/// identifies a row by its text. See `crate::panels::squelch`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquelchEntry {
    /// The name written into the row's `0x10000542` text child.
    pub name: String,
    /// Whether the zone-squelch flag is nonzero — which ACE calls `Account` and which is what it is.
    /// Decides the row's text state — `0x10000057` when true, `0x10000056` when false — which is
    /// in turn the only thing the Remove button reads to choose between `0x0059` and `0x0058`.
    pub account: bool,
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
            dereth_primitives::ContainerEra::PreTod => dereth_primitives::EraId::Infiltration,
            dereth_primitives::ContainerEra::Tod => dereth_primitives::EraId::Eor,
        }
    }

    /// Whether the world is the one from before Throne of Destiny.
    #[must_use]
    pub fn before_throne_of_destiny(&self) -> bool {
        self.world_dats == dereth_primitives::ContainerEra::PreTod
    }

    /// Whether the world has skill `id`.
    #[must_use]
    pub fn has_skill(&self, id: u32) -> bool {
        self.skills.binary_search(&id).is_ok()
    }
}

pub trait GameView: std::fmt::Debug {
    /// What the world's era means for what can be shown ([`EraView`]); `None` from a view that
    /// has no world.
    fn era(&self) -> Option<&EraView> {
        None
    }

    /// The player object, once `LOGIN_COMPLETE` (global message `0x0B`) has fired.
    fn player(&self) -> Option<ObjectId> {
        None
    }
    fn name(&self, _id: ObjectId) -> Option<&str> {
        None
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        None
    }
    /// The requested integer quality.
    fn int_stat(&self, _id: ObjectId, _prop: u32) -> Option<i32> {
        None
    }
    /// The queried secondary attribute as `(current, max)`.
    fn vital(&self, _id: ObjectId, _which: Vital) -> Option<(u32, u32)> {
        None
    }
    fn container_contents(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// The contained-containers list — the **side packs**, which are a
    /// separate ordered list from [`Self::container_contents`] and are what a
    /// `UI_ItemList_IsContainer` list shows.
    fn contained_containers(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// The inventory-placement list — `(iid, loc)` per wielded or worn item.
    ///
    /// The paper doll's inventory rebuild walks exactly this list and places each `(iid, loc)`
    /// into its doll location, which is the only source the equipment doll has.
    fn equipment(&self, _id: ObjectId) -> &[(ObjectId, u32)] {
        &[]
    }
    /// The loose-item count used by the item list's container-size update
    /// writes into `UI_ItemList_FixedListSize` for a non-container list. `None` when the object is
    /// unknown; a negative value is the client's "unbounded" and selects the empty-slot update.
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        None
    }
    /// The subcontainer count used by a container-list update.
    fn containers_capacity(&self, _id: ObjectId) -> Option<i32> {
        None
    }
    /// An inventory request naming this object is outstanding, so its icon is ghosted.
    fn item_waiting(&self, _id: ObjectId) -> bool {
        false
    }
    /// The item-list insertion index for the list identified by
    /// its parent container id and container-list flag — the optimistic row a drop draws before the
    /// shard answers, together with its insertion index.
    ///
    /// Retail holds this on the list element, so the panel that owns the list also owns the row.
    /// This build rebuilds every list from the world on each changed frame, so the row travels the
    /// other way: it lives beside the object table and the panels splice it into the ids they
    /// fill from, at this index. `None` is the ordinary case — at most one list in the whole UI
    /// has a pending row at a time, because the player has one pointer.
    fn pending_row(
        &self,
        _container: Option<ObjectId>,
        _containers_list: bool,
    ) -> Option<(ObjectId, u32)> {
        None
    }
    /// The selected object's id.
    fn selection(&self) -> Option<ObjectId> {
        None
    }
    /// Radar objects already converted to player space by the smart box.
    fn radar_objects(&self) -> &[RadarEntry] {
        &[]
    }
    /// The burden indicator's only input.
    fn load(&self) -> Option<f32> {
        None
    }
    /// The helpful and harmful enchantment collections.
    fn enchantment_counts(&self) -> (u32, u32) {
        (0, 0)
    }
    /// Active enchantments joined to the `SpellTable`, in no particular order; the
    /// effects panel sorts them by name.
    ///
    /// The default is empty, matching the absent-player-description guard before
    /// `0x0013`: rebuilding leaves the list cleared rather than drawing an empty row.
    fn active_effects(&self) -> Vec<EffectEntry> {
        Vec::new()
    }
    /// The vitae multiplier, with `< 1.0` meaning a penalty is active.
    fn vitae(&self) -> Option<f32> {
        None
    }
    /// The three vitae-display inputs, or `None` when no player description is
    /// available; that case writes nothing.
    fn vitae_display(&self) -> Option<VitaeDisplay> {
        None
    }
    /// The character sheet's inputs, or `None` when no player description is available;
    /// that case writes nothing.
    fn character_info(&self) -> Option<CharacterInfo> {
        None
    }
    /// The frame clock needed by panels that own a timer.
    ///
    /// Carried on the view because `crate::panels::linkstatus::LinkStatusPanel`
    /// tracks its last ping request, next update and round trip, as the original panel
    /// does. The default `0.0` freezes those timers, which is what an unclocked host
    /// should get.
    fn now(&self) -> f64 {
        0.0
    }
    /// How many `0x01EA Character_ReturnPing` have arrived.
    ///
    /// A **count**, not a timestamp: the client computes
    /// the current time less the last ping request's time at notice-delivery time, and this build
    /// has no notice bus, so the panel does the subtraction on the frame it first sees the count
    /// move. The difference is at most one frame of the round trip and is stated rather than
    /// hidden.
    fn ping_returns(&self) -> u64 {
        0
    }
    /// The link-status holder's packet-loss figure.
    ///
    /// A **ratio**, not a percentage, in spite of the accessor's name and in spite of the shipped
    /// string putting a `%` after it: the average-packet-loss accessor is
    /// `2 * (NAKed + retransmitted) / (received + sent)` and nothing multiplies it by a hundred.
    ///
    /// The default is `1.0` because that is the packet-loss field's initial value
    /// (the client's constructor writes `1.0f` into that field), i.e. a client that
    /// has heard nothing from a server reports total loss. It is **not** an `Option`: in retail
    /// the link-status panel's update sets its packet-loss line's float variable unconditionally —
    /// a `????` belongs to the ping line alone.
    fn packet_loss_percent(&self) -> f32 {
        crate::linkstatus::INITIAL_PACKET_LOSS
    }
    /// The portal-storm level, as the portal-storm-level notice carries it.
    ///
    /// **A `f32`, not an integer.**
    /// The portal-storm-level notice takes an `M` (a `float`) and
    /// the storm indicator's update compares it with `0.0` as a float.
    /// The level on the wire
    /// is `0x02C9`/`0x02CA`'s `extent`, which ACE sends as a fraction — so truncating it to an
    /// integer would turn every storm below 1.0 into "no storm".
    fn portal_storm_level(&self) -> f32 {
        0.0
    }
    /// A character option bit read by the Character Options page.
    ///
    /// `dereth_client::hud::HudView` implements it. With the trait's `false` as the only answer,
    /// all 50 rows of the Character Options page would open unticked whatever the server had
    /// sent, which is why `crate::options::character::CharacterSettingsPage::values_seen`
    /// exists.
    ///
    /// The default is still `false` rather than an `Option`, because that is what the *page* can
    /// use; a host that wants to tell "off" from "not asked" apart asks
    /// `dereth_client::hud::Hud::character_option`, which answers `Option<bool>`.
    fn player_option(&self, _o: PlayerOption) -> bool {
        false
    }

    /// The player module's default option value — the value *Restore Defaults* writes into
    /// one character option, which the check box reads at bind
    /// time and the page's restore-defaults writes back.
    ///
    /// **`Option`, not `bool`, and that is the whole point.** The function is a membership test
    /// against a compiled-in true-list, so *every* option has an answer in retail; a host that
    /// cannot supply one has to say so rather than answer `false`, because a Defaults button that
    /// silently unticked 49 boxes would look like it worked.
    /// `crate::options::character::CharacterSettingsPage::restore_default_values` counts the
    /// rows that got an answer, so "Defaults is not available on this host" and "Defaults set
    /// every option off" are different numbers.
    ///
    /// The table is `dereth_client_model::player::options::DEFAULT_TRUE_ORDINALS`, and this crate has no
    /// edge to `dereth-client-model`; `dereth_client::hud::HudView` does, and is the implementor.
    fn player_option_default(&self, _o: PlayerOption) -> Option<bool> {
        None
    }

    /// Read a floating-point gameplay option by property id — the read
    /// half of the client's named-property arm.
    ///
    /// The only two callers are the Chat Options page's opacity sliders, `0x10000080` and
    /// `0x10000081`, and both are **top-level** properties of the gameplay options — no window
    /// index, because the general section is one pair of sliders for all five chat windows.
    ///
    /// `None` is a module that carries no value, which is not a module that carries `0.0`: the
    /// client's fallback is the option's default, which is `0.5` and `1.0` out of
    /// property collection `0x78000001`, and a zero would make the chat window invisible.
    fn gameplay_option_float(&self, _property: u32) -> Option<f32> {
        None
    }

    /// Read property `0x1000007F` for one chat window — that window's
    /// 64-bit text-type filter, the read half of the 64-bit check box's
    /// value getter.
    ///
    /// The chat-option structure lookup indexes the gameplay options' `0x1000008C` array by
    /// the window id less 1, so this is per window and a window with no row in that array answers
    /// `None` — which the page turns into the window's own default mask, exactly as
    /// the value getter's default-preloaded result does.
    fn chat_window_filter(&self, _window: u32) -> Option<u64> {
        None
    }

    /// The four facts the client's not-a-stack arm branches
    /// on before it sends a health query or an item-mana query.
    ///
    /// `None` is *"this host does not answer the question"* and is not the same as a selection
    /// that asks for neither query: the first must not read as the second, or a build with no
    /// producer looks exactly like a build whose selection genuinely wanted nothing. See
    /// `crate::screens::gameplay::ToolbarSelection::query`.
    fn selection_query_facts(&self, _id: ObjectId) -> Option<SelectionQueryFacts> {
        None
    }

    /// The toolbar's selected-object meters: `(health, mana)`, each 0.0–1.0 or `None`
    /// for no answer yet.
    ///
    /// These are the *only* place the retail client keeps either fraction:
    /// the object-health notice and the item-mana notice write
    /// `METER_LEVEL` straight from the `0x01C0` / `0x0264` reply and nothing stores it per object.
    /// `dereth_client_model::combat::SelectedMeters` holds them, and this is their reader.
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        (None, None)
    }
    /// the AC N/E coordinate pair.
    fn player_coords(&self) -> Option<(f32, f32)> {
        None
    }
    /// The game-time string off the client's current game time —
    /// `(date, time-of-day name)`, already formatted, for the map panel's first
    /// block.
    ///
    /// `dereth_client::sky::GameClock` drives the sky off that object, and this seam is how the
    /// map panel asks it: the element is one retail updates unconditionally every five seconds.
    ///
    /// `None` is retail's no-current-game-time arm, which is **not** a blank
    /// element: see `crate::mapradar::map::date_time_text`.
    ///
    /// This is *not* gated on being outdoors. The is-outside call sits below this
    /// block and guards only [`Self::player_coords`]' two consumers.
    fn game_date_time(&self) -> Option<(String, String)> {
        None
    }
    /// The is-player-outside test, which is
    /// also what picks the radar's 75-vs-25 range and the chat sweep's radius.
    ///
    /// `dereth_client::hud::HudView` implements it off the player's own `objcell_id`. With the
    /// trait's `true` as the only answer, the radar would draw at the outdoor 75 inside every
    /// building and every dungeon.
    ///
    /// The default stays `true` rather than becoming an `Option`: a host that cannot answer is
    /// outdoors as far as every screen is concerned, and the two arms are the only two the client
    /// has. What separates "outdoors" from "not asked" is whether the host implements it, which
    /// is a compile-time question and not a runtime one.
    fn player_outside(&self) -> bool {
        true
    }
    /// The player's heading, in degrees.
    fn player_heading(&self) -> f32 {
        0.0
    }
    /// the `/radar off` state, separate from window visibility.
    fn radar_blank(&self) -> bool {
        false
    }

    /// The link-status holder's connection status — the link lamp's only input.
    ///
    /// The connected state carries **seconds since the last datagram from the current
    /// server** and not a round-trip time. `None` is the not-connected case, where the client
    /// leaves the elapsed time at 0.0 and the flag clear — which
    /// `crate::hud::indicators::link_status::link_state` turns into the "lost" lamp.
    ///
    /// One further quirk is reproduced there rather than here: the connection-status query clamps its
    /// answer to 15.0 while the no-drop-kick flag is set.
    fn link_status(&self) -> Option<f64> {
        None
    }

    /// The object id held by quickbar slot `slot`.
    ///
    /// A player-description update walks all eighteen shortcut entries and adds every
    /// nonempty entry to its corresponding slot. These are `PlayerModule` state, carried
    /// by `0x0013 Login_PlayerDescription` and saved by the 480-second flush, rather
    /// than object-table state. A shortcut may name an object the client has never
    /// seen; that produces a delayed slot rather than an empty one.
    fn shortcut(&self, _slot: u32) -> Option<ObjectId> {
        None
    }

    /// The `SkillTable` joined to the player's quality state.
    ///
    /// One entry per `SkillTable` key, in the table's own key order; the panel does the grouping
    /// and the alphabetical sort rather than requiring the host to provide display order.
    fn skills(&self) -> &[SkillEntry] {
        &[]
    }

    /// The player's known spell IDs joined to the spell table.
    fn spellbook(&self) -> &[SpellEntry] {
        &[]
    }

    /// Look up `spell_id` in the spell table — **one row, whether
    /// or not the player knows the spell.**
    ///
    /// This is deliberately not `spellbook().iter().find(...)`, for the reason
    /// [`Self::is_spell_known`] gives in the other direction: `spellbook()` is the player's
    /// spell book joined to the table, and the two questions have different answers exactly
    /// where it matters here. The spellcasting panel's cast-button tooltip and
    /// its endowment icon both ask the spell table about
    /// **the endowment spell** — the spell on the wand in the player's hand — and a wand's spell
    /// is ordinarily not in the player's book at all. Answering from the book would leave the
    /// caption and the endowment icon blank for every caster in the game.
    ///
    /// The row is reduced to [`SpellEntry`] because that is already what a spell-bar row carries
    /// and what `crate::items::widget::spell_recipe` needs; `id` is echoed back so a caller can
    /// pass the entry on unchanged.
    ///
    /// The default is the join, which is all a host without the table can say.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.spellbook().iter().find(|s| s.id == spell_id).cloned()
    }

    /// Is `spell` a key of the player's spell book?
    ///
    /// This is *not* [`Self::spellbook`]`.contains`, and the difference is the whole reason it
    /// exists. `spellbook()` is the book **joined to the `SpellTable`**, so a spell the character
    /// knows whose id has no table row is absent from it. The spell-cast submenu update
    /// prunes a favourite that fails the known-spell test **and tells the shard**
    /// (`0x01E4`), so answering that question with the join would put an unrequested edit on the
    /// wire for a spell whose only problem is missing metadata.
    ///
    /// The default is the join, which is all a host without a `PlayerDesc` can say.
    fn is_spell_known(&self, spell_id: u32) -> bool {
        self.spellbook().iter().any(|s| s.id == spell_id)
    }

    /// The spell id joined to the component table — everything
    /// the spell-examine pane reads for one spell.
    ///
    /// `None` is the client's no-magic-system / unknown-spell arm: the pane is not
    /// filled and the window is not shown.
    fn spell_examine(&self, _spell_id: u32) -> Option<SpellExamineView> {
        None
    }

    /// The component-object lookup — which object in the player's own
    /// inventory a component row in the spell examine pane names.
    ///
    /// The argument is the row's `0x10000010` component SCID. The host maps it to a
    /// WCID and searches the seven component-tracker categories for the first carried
    /// object of that class. `None` means no tracker, no such class or no carried
    /// instance; the click then does nothing.
    fn component_object_id(&self, _scid: u32) -> Option<dereth_primitives::ObjectId> {
        None
    }

    /// The component tracker's is-owned test for a formula slot's SCID — whether the
    /// player holds any of that component.
    ///
    /// The default is `false`, which is deliberately the *marked* answer: a host with no
    /// component tracker takes the client's show-the-mark arm, so a view
    /// that cannot answer shows the "you do not have this" mark rather than hiding it.
    fn component_is_owned(&self, _scid: u32) -> bool {
        false
    }

    /// The update-spell-components notice, as a serial.
    ///
    /// The client is pushed that notice whenever a component object is offered to the tracker;
    /// this build pulls, so a reader that sees a different number does what the listener does —
    /// the same shape as [`Self::examine_request`]'s serial. `0` from a host that has no tracker
    /// at all, which never changes and therefore never refreshes.
    fn component_serial(&self) -> u64 {
        0
    }

    /// The non-raw attribute query for `id` — the number
    /// the attribute row writes into its value text, for one of the six
    /// primary attributes (1 Strength ... 6 Self). `None` renders as `"???"`.
    ///
    /// Not `raw`: the displayed value is the enchanted one, which is why a buffed character's
    /// attributes page reads higher than their creation profile.
    fn attribute(&self, _id: u32) -> Option<i32> {
        None
    }

    /// The skill footer's inputs for one skill; see [`SkillAdvancement`].
    fn skill_advancement(&self, _id: u32) -> Option<SkillAdvancement> {
        None
    }

    /// The attribute footer's inputs for one of nine rows; see
    /// [`AttributeAdvancement`].
    ///
    /// `secondary` distinguishes the three vital rows from the six primary attributes.
    /// The original row-kind query returns 8 for a primary and a different value for
    /// a vital. The flag cannot be derived from the stat id because the id spaces
    /// overlap: stat 2 is Endurance as a primary and Health as a secondary.
    fn attribute_advancement(&self, _id: u32, _secondary: bool) -> Option<AttributeAdvancement> {
        None
    }

    /// The local player character's name for the stat-panel header.
    ///
    /// Separate from [`Self::name`] because it specifically selects the local player.
    /// The host can answer from the received player description before the player's
    /// world object exists; the vitals panel uses the same early-availability seam.
    fn character_name(&self) -> Option<&str> {
        None
    }

    /// The journal path builder's three globals.
    ///
    /// The settings directory, current world name, and local player's singular object name — the same
    /// three `dereth_client::ui::UiShell::screen_layout_path` needs for
    /// the screen-layout path builder, which is the precedent this follows. None of them is
    /// reachable from this crate, and the journal is the one panel whose model outlives the
    /// process.
    ///
    /// `None` means "no journal file", and the panel then keeps its pages in memory only: a build
    /// with no preferences file, no world name or no character yet has nowhere to write and must
    /// not guess. See [`crate::journal::JournalIdentity`].
    fn journal_identity(&self) -> Option<crate::journal::JournalIdentity> {
        None
    }

    /// The client's inputs.
    fn experience_header(&self) -> Option<crate::statmgmt::XpHeader> {
        None
    }

    /// Resolve the display text from gender, heritage, and creature type for the
    /// player -- the heritage text's first half.
    ///
    /// The join is the host's for the same reason [`SkillEntry`]'s name is: the gender and
    /// heritage display-name lookups (enum `0x10000001` for gender, `0x10000002` for
    /// heritage group) against dat objects this crate must not know about. `None` is
    /// the gender/heritage display lookup returning 0, which leaves the field empty.
    fn gender_heritage_display(&self) -> Option<String> {
        None
    }

    /// The title-table lookup for the title id -- the title
    /// the stat panel's character-info update appends to the heritage text after a single space, and
    /// only when the lookup succeeds.
    fn display_title(&self) -> Option<String> {
        None
    }

    /// The titles panel's whole refresh input; see [`CharacterTitles`].
    ///
    /// Separate from [`Self::display_title`] because the header needs one resolved
    /// string while the Titles tab needs every id with its string. The tab also needs
    /// the current displayed title id to decide whether to offer the Display button.
    fn character_titles(&self) -> CharacterTitles {
        CharacterTitles::default()
    }

    /// The PK-status line -- see [`PkStatus`].
    fn pk_status(&self) -> PkStatus {
        PkStatus::default()
    }

    /// Integer qualities 6 and 7 -- `AvailableLuminance` and
    /// `MaximumLuminance`, the luminance pair's only inputs.
    ///
    /// The client reads both unconditionally and then decides whether to draw them; the level
    /// gate is the experience update's and lives in the panel, not here.
    fn luminance(&self) -> (i64, i64) {
        (0, 0)
    }

    /// Integer quality `0x18` — **available skill credits**, the number the skill panel's
    /// default footer and its untrained-selection footer both
    /// put on the footer's second and first lines respectively.
    fn skill_credits(&self) -> i64 {
        0
    }

    /// 64-bit integer quality 2 — **unassigned experience**, the other footer number and the
    /// one the raise buttons are affordable against.
    fn available_experience(&self) -> i64 {
        0
    }

    /// The fourteen filter bits
    /// the spellbook's filter test tests, **not** the button states.
    /// The player-module decoder defaults it to `0x3FFF` when section `0x0020` is absent, which is
    /// every school and every level.
    fn spell_filters(&self) -> u32 {
        crate::spellbook::DEFAULT_SPELL_FILTERS
    }

    /// The eight spell bars, which are the contents of the spell-casting submenu's item list.
    ///
    /// The spell bar's player-module refresh fills each tab's item list from this
    /// list, and the quick-cast notice indexes the *list*, so this is the
    /// order a quick-cast key resolves against. A tab outside `0..8` is empty.
    fn spell_tab(&self, _tab: usize) -> &[u32] {
        &[]
    }

    /// The client's three-condition test, answered by the
    /// host because the `ITEM_TYPE` constants live below this crate.
    ///
    /// `(item, spellID)` for the object wielded at equipment location `0x1000000` whose
    /// `ITEM_TYPE` carries `Caster (0x8000)` and whose public-description spell id is non-zero;
    /// `None` when any of the three fails, which is the arm that clears the endowment item and the
    /// endowment-selected flag.
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        None
    }

    /// The UI item's decoration fields for one object — everything
    /// the UI item's per-frame update reads off the weenie **after** the icon, and
    /// nothing else.
    ///
    /// One method instead of separate field accessors because all four decoration
    /// updates run together for every list slot. A single `PublicWeenieDesc` lookup
    /// avoids four hash probes.
    fn slot_decoration(&self, _id: ObjectId) -> Option<SlotDecoration> {
        None
    }

    /// The object's plural name, which item lists request for a stack.
    ///
    /// The client falls back to the object's name when the plural buffer is empty (length 1, i.e.
    /// the terminator alone), so `None` here means "use [`Self::name`]" and not "no name".
    fn plural_name(&self, _id: ObjectId) -> Option<&str> {
        None
    }

    /// The object's `AppraisalProfile` projected for the examination panel.
    ///
    /// `None` means no `0x00C9 Item_SetAppraiseInfo` has arrived for that object — which is what
    /// makes the pull in `crate::panels::examination::ExaminationPanel::update` equivalent to the
    /// client's push: the profile cannot be answered before the reply lands.
    ///
    /// It carries only the six fields the panel reads, for the reason [`SlotDecoration`] does the
    /// same for a slot: the profile is a hundred-odd properties, this crate cannot see
    /// `dereth_protocol`, and inventing a wider seam would only hide which of them anything uses.
    fn appraisal(&self, _id: ObjectId) -> Option<AppraisalView> {
        None
    }

    /// Live `PublicWeenieDesc` facts for the item-examine inscription mouse handler:
    /// the inscribable bit and current equipment location. `None` means the object
    /// lookup missed. Hook-appraisal facts are deliberately not substituted; this
    /// handler reads the live object itself.
    fn inscription_mouse_facts(&self, _id: ObjectId) -> Option<(bool, u32)> {
        None
    }

    /// Send the examine-object notice for `id`, whose one listener is the examination panel.
    ///
    /// `(id, serial)`: the object the player last asked about, and how many times anything has
    /// asked. **The serial is the whole of why this is a pair rather than an `Option<ObjectId>`** —
    /// examining the *same* object twice must re-arm the awaited appraisal id and clear the
    /// current appraisal id, which is what re-opens a panel the player closed, and an
    /// id-only pull cannot see that. It is the counterpart of [`AppraisalView::delivery`] in the
    /// other direction and is a seam artefact of the pull model, declared as that one is.
    ///
    /// Three of the four routes that examine something live in `dereth_client::interaction`, which
    /// has no screen to call: the examine **cursor**'s pick, the examine cursor's second click, and
    /// the `0x2B SELECTION_EXAMINE` key. The fourth, the toolbar's identify button, reaches the
    /// panel on the screen directly as well — the examine notice handler is idempotent for the same
    /// id, so arriving twice is harmless.
    fn examine_request(&self) -> Option<(ObjectId, u64)> {
        None
    }

    /// The open book's id, page list and inscription.
    ///
    /// `None` is `bookID == 0`: no `0x00B4 Writing_BookOpen` has arrived, or the last one was
    /// closed. The panel opens on a change of [`BookView::opening`] and on nothing else, which is
    /// the same pull-instead-of-push seam [`Self::appraisal`] carries.
    fn open_book(&self) -> Option<BookView> {
        None
    }

    /// The last `0x0075 Character_StartBarber` notice, projected with the local player's two
    /// appearance-table selectors.
    ///
    /// `None` means no barber modal is active. The generation is deliberately separate from the
    /// payload because retail opens a fresh modal for every notice, even when the values match.
    fn barber(&self) -> Option<BarberView> {
        None
    }

    /// The allegiance hierarchy, already walked.
    ///
    /// The default is an empty roster, which is what a character in no allegiance has — and what
    /// all twelve `0x0020 Allegiance_AllegianceUpdate` in the capture corpus produce, since every
    /// one of them carries `total_members = 0` and no member records.
    fn allegiance_roster(&self) -> AllegianceRoster {
        AllegianceRoster::default()
    }

    /// The game world's allegiance-abort counter — how many `0x0003
    /// Allegiance_AllegianceUpdateAborted` have arrived.
    ///
    /// The **panel's edge** for the one notice this build's per-frame pull cannot otherwise see:
    /// the allegiance panel's update-aborted notice updates the panel
    /// only when it is visible, and the update's first statement is the busy-latch clear that
    /// `crate::panels::allegiance::AllegiancePanel::awaiting_update` carries. Without this the
    /// latch, set on every allegiance update request, would stay set for ever on a request the server
    /// aborts — the panel stuck "busy" with no answer coming.
    fn allegiance_update_aborts(&self) -> u64 {
        0
    }

    /// The game world's allegiance-update counter — how many `0x0020
    /// Allegiance_AllegianceUpdate` have arrived.
    ///
    /// The panel's edge for an answer that leaves the roster as it was: every answer runs the
    /// panel's update, whose first statement clears the busy latch, so an unchanged roster must
    /// still be seen to have arrived.
    fn allegiance_updates(&self) -> u64 {
        0
    }

    /// The current fellowship, when one exists.
    ///
    /// The default is `None` — *no fellowship* — which is what every host that has never received
    /// a `0x02BE Fellowship_FullUpdate` answers, and it is a **different** state from a
    /// fellowship with one member: the panel branches on exactly this
    /// pointer to choose which of its two frames is on screen.
    fn fellowship(&self) -> Option<FellowshipView> {
        None
    }

    /// The friends list in model order.
    ///
    /// The default is **empty**, which is the same answer a host that has received a `0x0021`
    /// carrying no records gives -- and that is not a gap: all five recorded `0x0021` are
    /// exactly that, an empty list with type `Full`, so the empty case is the one the recorded
    /// corpus witnesses and the panel has to be able to say it drew it.
    /// `crate::panels::friends::FriendsPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel puts the
    /// online friends first and sorts each block by name, and that walk is the panel's.
    fn friends(&self) -> Vec<FriendEntry> {
        Vec::new()
    }

    /// The communication system's squelch iteration — the squelch DB's **character** hash,
    /// walked, with the
    /// empty entries and the account hash left out.
    ///
    /// It is the *communication system's* walk and not the panel's, which is why it crosses this
    /// seam already done rather than as a `SquelchDb`: the walk reaches into the communication
    /// system's own instance and asks each entry whether it is zoned and what name it carries,
    /// none of which this crate may know about.
    ///
    /// The default is **empty**, which is the same answer a host whose shard sent a `0x01F4` with
    /// an empty DB gives -- and that is the ordinary case, not a gap: every login clears the DB
    /// (the client clears it on login) and no recorded capture carries a populated one.
    /// `crate::panels::squelch::SquelchPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel's sorted insert sorts the
    /// rows by name as they are inserted, and that walk is the panel's.
    fn squelch_list(&self) -> Vec<SquelchEntry> {
        Vec::new()
    }

    /// Tracked contracts in tracker-table order, ascending by contract id.
    ///
    /// The default is empty, which is what every host that has received no `0x0314`/`0x0315`
    /// answers — and that is the overwhelmingly common case: neither opcode appears anywhere in
    /// the recorded corpus. So
    /// `crate::panels::contracts::ContractsPanel::rebuilds` is what separates "a character with
    /// no contracts" from "the panel never ran".
    ///
    /// This is not display order: the panel re-sorts by name or status on every rebuild.
    fn contracts(&self) -> Vec<ContractEntry> {
        Vec::new()
    }

    /// How many `0x0226 House_HouseStatus` notices this session has received.
    ///
    /// The house panel's failed-transaction notice is a bare jump to its
    /// update, and the update redraws the pane unconditionally — it reads nothing out of
    /// the notice, whose one `u32` all three registered receivers ignore. So what crosses this
    /// seam is not the message's *content* (there is none the client keeps) but the fact that one
    /// **arrived**: `crate::panels::house::HousePanel` redraws whenever this number moves, which
    /// is retail's "redraw on every notice" expressed in a pull.
    ///
    /// The default is `0`, which is what every host that has received no `0x0226` answers — and
    /// that is the case a build with no shard is in, where the pane falls back to
    /// `crate::panels::house::HousePanel::stand_in_for_the_login_query`.
    fn house_status_notices(&self) -> u64 {
        0
    }

    /// House data shaped for the six sections that read it.
    ///
    /// `None` is retail's null pointer, which is both "no house" and "the last thing that arrived
    /// was a `0x0226`" — the status handler deletes the object.
    fn house_data(&self) -> Option<HouseDataView> {
        None
    }

    /// How many `0x0225 House_HouseData` have arrived.
    ///
    /// The house-data redraw edge, alongside [`Self::house_status_notices`]. The
    /// house pane redraws on every notice, so it redraws whenever this count changes.
    fn house_data_notices(&self) -> u64 {
        0
    }

    /// The client's two host-side inputs.
    fn house_purchase(&self) -> HousePurchaseView {
        HousePurchaseView::default()
    }

    /// What the CRT's `localtime` would add to a Unix instant before any of this crate's date
    /// formatters sees it, in seconds.
    ///
    /// Six functions in the retail image format a date and **all six** run their instant through
    /// `localtime` first (the house panel's purchase-time line, the character sheet's
    /// birth/age/deaths section, the account-banned handler, the chat scroll's own timestamp,
    /// the load-file variable substitution and the house system's time conversion). MSVCR70's
    /// `localtime` reads `TZ` if it is set and
    /// otherwise calls `GetTimeZoneInformation`, so the number is the **operating system's**,
    /// daylight rule included, and nothing in the client's own configuration takes part.
    ///
    /// It is an accessor and not a constant because the answer depends on the instant: a house
    /// bought in January and one bought in July are an hour apart in the same zone. Hosts that
    /// cannot answer return `0` and render in UTC; `dereth_client::hud::HudView` implements it off
    /// `dereth_client::platform::local_utc_offset_secs`.
    ///
    /// The default is `0` rather than a panic because a screen must draw for a host that knows
    /// nothing — [`EmptyGameView`] is exactly that host.
    fn utc_offset_secs(&self) -> crate::ctime::UtcOffsetSecs {
        0
    }

    /// Vendor-window state. The default is a closed shop, which a host
    /// without an open vendor returns and the panel must draw as hidden.
    fn shop(&self) -> ShopView {
        ShopView::default()
    }

    /// Secure-trade state. The default is a closed window, as for a host
    /// without an open negotiation. The model is `dereth_client_model::trade`;
    /// `dereth_client::trade_view::trade` converts it.
    fn trade(&self) -> TradeView {
        TradeView::default()
    }

    /// Ask quietly whether `id` is acceptable to the vendor — **the drag cursor's
    /// question**, and the only thing the sell list's drag-over asks.
    ///
    /// It cannot be answered from [`Self::shop`]: a `ShopView` describes the three lists, and the
    /// object being dragged is in the *pack*. The decision needs ownership by the player,
    /// the contained-item count and the vendor profile's acceptability test against the open
    /// shop's own profile, which the game world's drag-acceptance query provides.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses —
    /// a host with no shop open shows the red circle, and a host with no vendor at all never raises
    /// the message, because the handler is bound to the sell list alone.
    ///
    /// It cannot be answered from [`Self::slot_decoration`]: the test reads the item's material
    /// type and bit `0x01000000` of `_bitfield`, neither of which is
    /// on that seam, **and** the `SalvageMultiple` character option, which is the player's and not
    /// the item's. The host answers it with
    /// `dereth_client_model::inventory::salvage::is_item_suitable`.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses:
    /// a host with no world refuses every drop rather than accepting every one.
    fn salvage_item_suitable(&self, _item: ObjectId, _panel_material: u32) -> bool {
        false
    }

    /// Whether the item is owned by the player, the client's first test and the one that produces
    /// the panel's only refusal string.
    fn item_owned_by_player(&self, _item: ObjectId) -> bool {
        false
    }

    /// `None` until a `0x021D House_HouseProfile` arrives.
    fn slumlord(&self) -> Option<SlumlordView> {
        None
    }

    /// Chess-panel state. `None` means the host has no chess model.
    fn minigame(&self) -> Option<MiniGameView> {
        None
    }

    /// How many `0x021D House_HouseProfile` have arrived — and the edge that
    /// **raises** the window: the slumlord window's house-profile notice ends in
    /// making it visible, through the element itself rather than by name.
    ///
    /// A **count**, not a flag, for the reason [`Self::house_data_notices`] is one: two profiles
    /// in one frame are two house updates in retail, and a second use of the same slumlord
    /// must re-raise a window the player closed.
    fn slumlord_notices(&self) -> u64 {
        0
    }

    /// The payment strings and paid-in-full result after this window's current drops.
    ///
    /// Each `drops` row carries `(wcid, amount, trade-note face value)` in drop order.
    /// The host replays them over a pristine profile copy. This matches the original
    /// window's accumulated payment state, which adds payment on insertion and removes
    /// it on removal.
    ///
    /// The arithmetic stays in `dereth-client-model` with `HousePaymentList`, rather than being
    /// duplicated here, as with [`Self::salvage_item_suitable`].
    fn slumlord_payment(&self, _rent: bool, _drops: &[(u32, i32, Option<i32>)]) -> SlumlordPayment {
        SlumlordPayment::default()
    }

    /// Check whether the selected rent or purchase payment still needs class `wcid` over the same
    /// replayed state — the client's last test, and the one that decides whether a drop is
    /// taken at all.
    fn slumlord_needs_more(
        &self,
        _rent: bool,
        _drops: &[(u32, i32, Option<i32>)],
        _wcid: u32,
        _trade_note_value: Option<i32>,
    ) -> bool {
        false
    }

    /// Apply `{wcid, num}` to the selected rent or purchase payment over the same replayed state —
    /// the boolean the slumlord window's add-payment gates its add-item on.
    ///
    /// It is **not** the same question as [`Self::slumlord_needs_more`], and the two are a pair
    /// because the client asks both: the drag-acceptance test asks whether more is needed about the *drag*, and
    /// the add then asks the payment step about the *insert*, so an item that passed the hover test can
    /// still be refused at the drop when the payment attempt finds the row already full.
    ///
    /// It cannot be inferred from [`Self::slumlord_payment`] either: the buy tab's text is
    /// `"<num> <name>"`, which carries **no paid count at all**, so a partial
    /// payment changes nothing visible and a panel that diffed the two strings would refuse every
    /// drop against the purchase price.
    fn slumlord_pay(
        &self,
        _rent: bool,
        _drops: &[(u32, i32, Option<i32>)],
        _wcid: u32,
        _amount: i32,
        _trade_note_value: Option<i32>,
    ) -> bool {
        false
    }

    /// The item's public weenie class id, used by the window's remaining-payment
    /// check and by each payment entry it adds.
    fn item_wcid(&self, _item: ObjectId) -> u32 {
        0
    }

    /// Reverse-lookup `wcid` in the dual enum map selected by group 10, enum `0x10000001`, and
    /// type `0x28`, yielding the trade-note value when present.
    fn item_trade_note_value(&self, _item: ObjectId) -> Option<i32> {
        None
    }

    /// The item's payment quantity: its stack size when nonzero, otherwise 1.
    /// This is the amount added to a house-payment entry.
    fn item_house_payment(&self, _item: ObjectId) -> i32 {
        1
    }

    /// The stack slider's top. The panel compares [`Self::split_size`] against it and takes the
    /// whole-stack arm when they are equal.
    fn max_split_size(&self) -> i32 {
        1
    }

    /// The object's material type, which latches into
    /// the window's material on its first row.
    ///
    /// `0` is the client's "no material", which is also the window material's empty value, so a miss
    /// leaves the window unlocked rather than locked to a material nothing can match.
    fn item_material_type(&self, _item: ObjectId) -> u32 {
        0
    }

    fn vendor_drag_item_accepted(&self, _item: ObjectId) -> bool {
        false
    }

    /// The secure-trade window's drag test, **ownership half**, asked with
    /// `quiet = 1` from its item-list drag-over.
    ///
    /// An unknown id is refused silently. An object the player does not own is refused, and
    /// unless `quiet` the client displays "You can only trade items you are carrying" as type
    /// `0x1A`. Otherwise the answer is whether the item is not already in the self items list.
    ///
    /// This is everything except the last step; the already-in-the-list half is the panel's own
    /// list and stays in `crate::panels::trade::TradePanel::drag_accept_state`. The host
    /// answers it with the game world's trade-item acceptance query, which the **drop** path
    /// also uses; this is the hover's.
    ///
    /// Default **false**: the client's own answer when the weenie lookup misses is
    /// a silent refusal, so a host with no world refuses every drag rather than accepting one.
    fn trade_drag_item_acceptable(&self, _item: ObjectId) -> bool {
        false
    }

    /// The external-container window's drag test, asked with `quiet = 1` from
    /// its item-list drag-over -- **the hover hint over an open chest, corpse, ground
    /// pack or hook.**
    ///
    /// The drag test accepts when the window has no ground-object id or when that
    /// container cannot be found. If the container exists but the dragged item does
    /// not, it refuses. Otherwise it performs the hook-status check below.
    ///
    /// `quiet` is never read: every refusal is silent, unlike vendor or trade refusals.
    ///
    /// The hook check first clears its output flag and refuses an absent item or
    /// container. A container with hook-type mask 0 **or** accepted-item-type mask 0
    /// is not a hook and accepts. A hook with owner id 0 sets the output flag to 1
    /// and refuses. It also refuses an item whose hook-type mask is 0 or has no bit
    /// in common with the hook's mask. Otherwise it accepts exactly when the item's
    /// type intersects the hook's accepted-item-type mask.
    ///
    /// **So every container that is not a hook answers `1`, and the hint over a chest or a corpse
    /// is the plain green `0x10000040` for every item you can carry** -- the window reads neither
    /// an is-corpse test nor `ITEM_TYPE` nor ownership. The red `0x10000041` exists, but only
    /// for a hook whose location mask or item-type mask the carried object misses, and for a hook
    /// in nobody's house.
    ///
    /// Default **true**, which is the client's own answer when the window has no ground object:
    /// a host with no world paints the same green every non-hook container paints, rather than a
    /// red the drop would then contradict.
    fn external_container_drag_item_acceptable(&self, _item: ObjectId, _ground: ObjectId) -> bool {
        true
    }

    /// The item's public valid-location mask — what the paper
    /// doll's item-list drag-over reads.
    ///
    /// `None` is the client's own answer when the weenie lookup misses: the handler
    /// returns without writing any state, which is not the same as refusing.
    fn item_valid_locations(&self, _item: ObjectId) -> Option<u32> {
        None
    }

    /// Quietly test whether the player may wield the item — the second test the
    /// doll's drag handler makes, after the slot mask. The host
    /// answers it with the game world's auto-wield legality query. Default false: a host with no
    /// world refuses every wield rather than accepting every one.
    fn auto_wield_is_legal(&self, _item: ObjectId) -> bool {
        false
    }

    /// Quietly test whether the player may wear the item and return the conflicting worn item — the **three**
    /// answers the paper doll's drag-over acts on.
    ///
    /// | this | the client | what the drag-over does |
    /// |---|---|---|
    /// | `Some(true)` | returned true | state `0x10000040` — accept |
    /// | `Some(false)` | false, worn flag `0` | state `0x10000041` — refuse |
    /// | `None` | false, worn flag `1` | **nothing at all** |
    ///
    /// The third row is a real branch and not a rounding of the second: the worn flag is written
    /// at exactly one site, reached only when the item's priority clashes with the
    /// player's clothing-priority mask **and** the item has a nonzero location on the player — i.e.
    /// the piece being carried is already on the figure. The drag-over then skips the
    /// refusal, so dragging a worn piece back over your own doll leaves the overlay untouched.
    /// Sibling of [`Self::auto_wield_is_legal`], which is the *slots*' predicate and a different
    /// function (wielding rather than wearing).
    ///
    /// Default `Some(false)`: a host with no world refuses rather than accepting, which is the
    /// same choice [`Self::auto_wield_is_legal`] makes.
    fn auto_wear_is_legal(&self, _item: ObjectId) -> Option<bool> {
        Some(false)
    }

    /// The spell's is-untargeted test — the first of the two tests the spellcasting panel's
    /// cast-button tooltip makes before it enables the Cast
    /// button.
    ///
    /// ```text
    /// f = the spell's formula
    /// t = the formula's targeting type
    /// return t == 0
    /// ```
    ///
    /// where an incomplete formula (any of its first five slots empty) has type 0, and otherwise the
    /// targeting type is read from one component: the walk goes up from slot **5** while the slot is
    /// filled and takes the last filled one (slot 4 when slot 5 is empty), and that component's
    /// targeting type is the answer —
    /// so this is a property of the *formula*, not a flag on the base. It is deliberately **not**
    /// the same question as `_bitfield & SelfTargeted (8)`, which
    /// [`SpellEntry::bitfield`](crate::view::SpellEntry::bitfield) already carries and which
    /// the tooltip tests separately: a spell can be either, both or neither, and each alone enables
    /// the button.
    ///
    /// Default false — a host with no spell table cannot say a spell needs no target, and the
    /// safe direction is the one that leaves the button greyed.
    fn spell_is_untargeted(&self, _spell_id: u32) -> bool {
        false
    }

    /// Whether the selected object is compatible with the spell, queried quietly.
    ///
    /// The function reads the spell base only to reach its target type and then delegates to
    /// the target-type compatibility test, which is the same predicate
    /// the game world's spell-casting operation runs before it sends. **The call here is quiet** where
    /// `CastSpell`'s is loud: this one runs to build a *tooltip*, and a
    /// chatty version would spam the feedback channel every time the selection moved.
    ///
    /// The object asked about is always [`Self::selected_object`], which is why it is not a parameter.
    ///
    /// Default false, for [`Self::spell_is_untargeted`]'s reason.
    fn spell_target_compatible(&self, _spell_id: u32) -> bool {
        false
    }

    /// Whether the item can be used with self as its target.
    ///
    /// The original client shifts the public useability word's high half down sixteen
    /// places with a sixteen-iteration loop, then tests bit 1. An endowed wand usable
    /// on the player needs no selection, the first and most common endowment-tooltip
    /// arm.
    ///
    /// Default false.
    fn item_useable_self_target(&self, _item: ObjectId) -> bool {
        false
    }

    /// Whether the selected target is compatible with the item, queried quietly.
    ///
    /// The argument order is **target first, source second**. The game-layer compatibility query
    /// uses the same `(target, source)` order.
    ///
    /// Quiet, for [`Self::spell_target_compatible`]'s reason. The target is always
    /// [`Self::selected_object`].
    ///
    /// Default false.
    fn item_target_compatible(&self, _item: ObjectId) -> bool {
        false
    }

    /// The selected object: the vendor button handler reads it once before
    /// dispatching, and eight of its eleven cases use it.
    fn selected_object(&self) -> Option<ObjectId> {
        None
    }

    /// Is `id` anywhere in the allegiance **hierarchy the shard sent**, at any depth?
    ///
    /// [`Self::allegiance_roster`] cannot answer this: it carries the monarch, the patron and the
    /// player's own vassals, which is what the panel *draws*, while `GetData` walks the whole
    /// tree. The allegiance panel needs the whole tree: the Swear
    /// button is offered only for a selected player who is **not already a member** — so this is
    /// its own question rather than a derivation of the roster.
    fn allegiance_has_member(&self, _id: ObjectId) -> bool {
        false
    }

    /// The player's `InstanceID` quality **26 (`Monarch`)**, which is what the allegiance
    /// panel's quality-changed handler watches.
    ///
    /// Its post-init registers two handlers on the player,
    /// for group 7, quality `0x19` and quality `0x1A` — group 7 is the
    /// `InstanceID` table and `0x19` / `0x1A` are `Patron` (25) and `Monarch` (26).
    ///
    /// The recorded captures confirm the direction: every `0x02DA Qualities_UpdateInstanceID` with
    /// key `0x1A` on the recorded character is followed within milliseconds by a `0x001F`.
    ///
    /// **Only the monarch half is available here**: `dereth_client_model::weenie::mirror_stat_update` has an
    /// arm for key 26 and none for key 25, so a patron change that does not also change the
    /// monarch is invisible to this build. That is a narrower trigger than retail's, not a
    /// different one, and it is stated rather than papered over.
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        None
    }

    /// The stack slider's current value; the default is 1, which is what a shop's own splitter
    /// seeds to (the toolbar's selection-changed handler, vendor arm).
    fn split_size(&self) -> i32 {
        1
    }

    /// The component tracker's seven category lists joined to the player's desired quantities.
    ///
    /// The spell-component panel walks all seven tracker categories in order, emitting
    /// a header per category and a row per component. Each tracker list is already
    /// sorted by object name on insertion. Neither the panel nor this seam sorts it.
    ///
    /// The default is **seven empty categories, not an empty vector**: a character with no
    /// components still sees seven headers, and a host that has not filled the tracker must not be
    /// indistinguishable from one whose player carries nothing.
    fn spell_components(&self) -> Vec<ComponentCategory> {
        (0..7)
            .map(|category| ComponentCategory {
                category,
                rows: Vec::new(),
            })
            .collect()
    }

    /// Ask whether `obj` is an owned component and return its class id — is this
    /// **object** one of the components the player is carrying, and if so which class is it?
    ///
    /// The spell-component panel's selection-changed notice is its only caller in the
    /// panel layer, and the question it asks cannot be answered from
    /// [`Self::spell_components`]: that snapshot carries one object per row
    /// while the tracker's object-id hash carries **every**
    /// object of every stack. Selecting the third of five piles of Lead Scarabs in the world
    /// still highlights the Lead Scarab row in retail, and matching on the row's first object
    /// would miss four of the five.
    ///
    /// `None` is the client's `FALSE` return: not a component, or not owned.
    fn object_is_owned_component(&self, _obj: ObjectId) -> Option<u32> {
        None
    }

    /// The enchantment registry's cooldown test on the **player's own** registry, for the
    /// cooldown id an item carries.
    ///
    /// ```text
    /// for each entry e in the cooldown list:
    ///     if ((e.id & 0xFFFF) == key) {
    ///         remaining = (e.duration + e.start_time) - now;
    ///         if (remaining <= 0) { remove e; return 0; }
    ///         return 1;
    ///     }
    /// return 0;
    /// ```
    ///
    /// `key` is what the caller passes, and the item slot's cooldown display passes
    /// **the cooldown id `+ 0x8000`** — the offset that puts an item cooldown above the spell-id
    /// range the registry's spell totals count. Adding it is the *host's* job here, exactly as
    /// it is the caller's in the client, so the widget hands over the raw cooldown id.
    ///
    /// `now` is the frame clock, which reaches an item slot as `UiSystem::now`. It is a
    /// parameter rather than something the host samples for itself because every other value on
    /// this seam is a fact about a frozen world and this one is not: it changes between two calls
    /// in the same frame, and a cached copy would make the wedge jitter.
    ///
    /// `None` is the client's `return 0` — no registry, no such cooldown, or one that has expired.
    fn cooldown_remaining(&self, _cooldown_id: u32, _now: f64) -> Option<f64> {
        None
    }

    /// The `COMBAT_MODE` the toolbar's stance icon shows.
    ///
    /// The default is `NONCOMBAT_COMBAT_MODE`, which is what a character enters the world in and
    /// what retail's in-game screen shows on entry; it is deliberately not `UNDEF_COMBAT_MODE`,
    /// because with `UNDEF` all four buttons are hidden and the toolbar has a hole in it.
    fn combat_mode(&self) -> u32 {
        crate::combat_mode::NONCOMBAT
    }

    /// The player module's advanced-combat-UI option — bit 12 of the first player-option word.
    ///
    /// The combat window's set-combat-mode notice calls it **itself**,
    /// rather than reading the copy the combat mode setter keeps of it, so this is
    /// the option and not the combat system's cached word. When it is on, the classic combat
    /// cluster stays down in every mode: the advanced combat interface is a different window.
    ///
    /// Default `false` — `PLAYER_OPTIONS`' bit 12 is clear in the client's own defaults.
    fn advanced_combat_ui(&self) -> bool {
        false
    }

    /// The combat window's three read-back notices, as one snapshot. See
    /// [`CombatBar`], and `crate::hud::combat_window::CombatWindow::update`, which applies it.
    ///
    /// The default is the client's reset state, so a host that does not implement this
    /// leaves the window showing a medium attack height and a half-full notch, which is what a
    /// character who has just logged in shows.
    fn combat_bar(&self) -> CombatBar {
        CombatBar::default()
    }

    /// The player's advancement class for Recklessness, skill `0x32`. This alone
    /// decides whether the combat cluster draws its recklessness meter in melee.
    ///
    /// The default is 0 (`Undef`), below 2, so the meter stays hidden before the player
    /// description arrives.
    fn recklessness_advancement_class(&self) -> u32 {
        0
    }
}

/// The assessed-object fields the examination window needs.
///
/// The initial six-field projection of `AppraisalProfile` was:
///
/// | field | property |
/// |---|---|
/// | `creature` | the `0x0100` creature block |
/// | `template` | string 5, `Template` |
/// | `character_title` | integer `0x105` (261), `CharacterTitleId` |
/// | `gear_plating_name` | string `0x34` (52), `GearPlatingName` |
/// | `value` | integer `0x13` (19), `Value` |
/// | `burden` | integer 5, `EncumbranceVal` |
///
/// `template` and `character_title` record presence, not value: either present
/// property sends a creature to the character pane. **This now carries the
/// whole readable profile.** The initial six served the frame and value/burden
/// blocks; the rest serve the creature pane and ordered item-examine blocks.
/// Skill names, creature-type names and attribute labels cross as resolved strings
/// because their dat sources (`SkillTable 0x0E000004`, `EnumMapper 0x2200000E`)
/// are unavailable to this crate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppraisalView {
    /// `dereth_client_model::AppraisalCache::delivery` — which `0x00C9` this profile came in on.
    ///
    /// It has no counterpart in the client and is not read from the profile: it is what lets a
    /// per-frame **pull** tell a new reply from the same one being offered again, which a client
    /// called *by* the reply never has to ask. See the field's own note in `dereth_client_model::appraisal`.
    pub delivery: u64,
    pub creature: bool,
    pub template: bool,
    pub character_title: bool,
    pub gear_plating_name: Option<String>,
    pub value: Option<i32>,
    pub burden: Option<i32>,

    // ---- the frame's own flag ------------------------------------------------------------------
    /// Whether appraisal succeeded. Every update and every modifier line branches on it:
    /// a failed assess draws its numbers in the "unknown" font and the vitals as a bare percentage.
    pub success: bool,

    // ---- the creature and character panes ------------------------------------------------------
    /// `InqInt(0x19)` — the creature pane's level text. `< 1` is `"???"`.
    pub level: Option<i32>,
    /// The creature-type label, resolved from integer quality 2 through enumeration table
    /// `0x2200000E` and displayed in the creature pane's first line (`0x1000014E`).
    pub creature_display_name: Option<String>,
    /// The six primary attributes for ids `1..=6`, in **attribute-id** order
    /// (`Strength Endurance Quickness Coordination Focus Self`); the pane's *drawn* order swaps
    /// the middle pair. `None` is the client's zero-is-absent.
    pub attributes: [Option<u32>; 6],
    /// The vital lookup for `1..=6`, in vital-id order
    /// (`MaxHealth Health MaxStamina Stamina MaxMana Mana`).
    pub vitals: [Option<u32>; 6],
    /// The attribute enchantment-mod query for `1..=6`; `Some(true)` beneficial, `Some(false)` harmful.
    pub attribute_enchanted: [Option<bool>; 6],
    /// The vital enchantment-mod query for the three **maxima**, indexed the same as `vitals`;
    /// the current vitals are always `None` because the client has no bit for them.
    pub vital_enchanted: [Option<bool>; 6],

    // ---- the item pane's blocks ----------------------------------------------------------------
    /// The object's valid locations, or the hooked-item override for a hooked item —
    /// the value every arm of the weapon-and-armour appraisal display branches on.
    pub valid_locations: u32,
    /// The object's ammunition type, or the hooked-item override for a hooked item.
    pub ammo_type: u16,
    /// The unpacked weapon block (`0x0020`).
    pub weapon: Option<WeaponView>,
    /// `InqInt(0x161)` `WeaponType`, the parenthesised family after the skill name.
    pub weapon_type: Option<i32>,
    /// `InqInt(0x1C)` `ArmorLevel`.
    pub armor_level: Option<i32>,
    /// The armor query — the `0x0080` block's eight floats, in **wire** order
    /// (`slash pierce bludgeon cold fire acid nether electric`).
    pub armor_mods: Option<[f32; 8]>,
    /// The profile's integer and float enchantment-mod queries, **already resolved**, for every
    /// property
    /// [`crate::panels::examination::HIGHLIGHTED_PROPERTIES`] names.
    ///
    /// Same encoding as [`Self::attribute_enchanted`]: a key is present only when the
    /// property is enchanted at all (the profile's low bit), and the value is whether the
    /// enchantment **raised** it (the same bit sixteen places up). Absent is plain.
    ///
    /// The two bitfield tables live in `dereth_client_model::appraisal` -- `int_highlight`,
    /// `float_highlight` and `highlight_state` -- and the join is made in `Hud::appraisal`, so the
    /// bit arithmetic has exactly one copy and this crate never sees a raw profile.
    pub enchantment_mods: std::collections::BTreeMap<u32, bool>,
    /// String property `0x0E` `Use` — the usage block.
    pub use_text: Option<String>,
    /// `InqInt(0x10C)` `RemainingLifespan`, present only when the preceding
    /// `InqInt(0x10B) Lifespan` and `InqInt(0x62) CreationTimestamp` calls also succeed.
    ///
    /// The other two values are not carried: the original uses them only as presence
    /// gates, then format this value directly without consulting a clock or computing a remainder.
    pub remaining_lifespan: Option<i32>,
    /// String property `0x10` `LongDesc` — the description block.
    pub long_desc: Option<String>,
    /// String property `0x0F` `ShortDesc` — the fallback read only when LongDesc is absent.
    /// Presence is retained independently because a present empty LongDesc suppresses this arm.
    pub short_desc: Option<String>,
    /// The augmentation cost from property 3. Presence alone gates the
    /// localized cost row, including zero and negative values.
    pub augmentation_cost: Option<i64>,
    /// `InqInt(0xAC)` gates the present-LongDesc decoration branch, including a zero mask.
    pub long_desc_decoration: Option<u32>,
    /// Positive `InqInt(0x83)` resolved through the material mapper; a lookup miss is `Some("")`.
    pub description_material: Option<String>,
    /// Present GemCount/GemType pair, with the singular/plural name resolved for that count.
    pub description_gems: Option<(i32, String)>,
    /// `InqInt(0x6F)` `PortalBitmask` — the guard *and* the operand of the description block's
    /// portal section.
    ///
    /// `None` is the client's `InqInt` answering zero, which skips the whole block — including
    /// its two unconditional info lines. ACE's `PropertyInt.PortalBitmask = 111` is an
    /// `[AssessmentProperty]`, so any portal the shard describes carries it;
    /// `ACE.Entity.Enum.PortalBitmask` names the bits
    /// (`Unrestricted 0x01, NoPk 0x02, NoPKLite 0x04, NoNPK 0x08, NoSummon 0x10, NoRecall 0x20`).
    pub portal_bitmask: Option<i32>,

    // ---- the lock-appraisal block -------------------------------------------------------------
    /// Whether the examined object is a hook. A hook skips the whole lock block.
    ///
    /// **The predicate is the hook test, not creature detection.** The independent
    /// appraise-info path confirms hook detection: a true result leads to a query
    /// for valid hook locations.
    ///
    /// Five blocks use the predicate: lock appraisal, item appraise-info, boost value,
    /// heal kit and remaining uses.
    pub weenie_is_hook: bool,
    /// Bit `0x10000` of the examined object's public-description flags — the first thing
    /// the boost-value block and the heal-kit block test.
    /// Set means "this is a healing kit", and it forks the two
    /// blocks apart: the boost-value block returns outright, the heal-kit block is the only arm
    /// that draws.
    pub weenie_is_healer: bool,
    /// The object's `0x20000` description bit, which means "lockpick". Together with the healer
    /// bit it decides whether an unassessed object gets
    /// *"Number of uses remaining:  Unknown"* at all.
    pub weenie_is_lockpick: bool,
    /// The item capacity is read off the **object**, not the profile. No `0x00C9` carries it.
    pub items_capacity: i32,
    /// The object's container capacity.
    pub containers_capacity: i32,
    /// Whether the appraisal profile describes a hooked item — a null test on its hook profile,
    /// so this
    /// is simply "the profile carries a `0x0040` hook block".
    /// The lock-appraisal block is its only caller.
    pub hooked_item: bool,
    /// Whether the hook profile's is-healer bit is set,
    /// which is the hook profile's bitfield `& 2`. The boost-value block
    /// and the heal-kit block ask it, and between them it decides which of
    /// the two blocks draws `InqInt(0x5A)`.
    pub hooked_item_healer: bool,
    /// Whether the hook profile's is-lockpick bit is set; it is
    /// `bitfield & 8`. The lock-appraisal block is its only caller here.
    pub hooked_item_lockpick: bool,

    // ---- the tinkering block ------------------------------------------------------------------
    /// `InqInt(0xAB)` `NumTimesTinkered` — *"This item has been tinkered %d time%s."*
    pub num_times_tinkered: Option<i32>,
    /// String property `0x27` `TinkerName` — *"Last tinkered by %s."*
    pub tinker_name: Option<String>,
    /// String property `0x28` `ImbuerName` — *"Imbued by %s."*
    pub imbuer_name: Option<String>,
    /// `InqInt(0x69)` `ItemWorkmanship` — the gate of the workmanship line.
    pub workmanship: Option<i32>,
    /// `InqInt(0xAA)` `NumItemsInMaterial` — present takes the salvage arm, absent the plain one.
    pub num_items_in_material: Option<i32>,

    // ---- the equipment-set block ---------------------------------------------------------------
    /// `InqInt(0x109)` `EquipmentSetId`, fed to the block's own 88-arm switch.
    pub equipment_set_id: Option<i32>,

    // ---- the gear-ratings block ----------------------------------------------------------------
    /// The thirteen `Gear*` rating terms in the block's **drawn** order — see
    /// [`crate::panels::examination::GEAR_RATING_ROWS`] — plus `GearMaxHealth` last.
    pub gear_ratings: [Option<i32>; 14],

    // ---- the defence-mod block -----------------------------------------------------------------
    /// Float property `0x1D` `WeaponDefense`, float property `0x95` `WeaponMissileDefense`,
    /// float property `0x96` `WeaponMagicDefense`. Each is drawn only when present **and not 1.0**.
    pub defense_mods: [Option<f64>; 3],

    // ---- the caster block ----------------------------------------------------------------------
    /// Float property `0x90` `ManaConversionMod`. The line prints `v + 1.0` as a modifier.
    pub mana_conversion_mod: Option<f64>,
    /// Float property `0x98` `ElementalDamageMod`, paired with [`Self::caster_damage_type`]; both
    /// are needed.
    pub elemental_damage_mod: Option<f64>,
    /// `InqInt(0x2D)` `DamageType` — the `%s` of *"Damage bonus for %s spells:"*.
    pub caster_damage_type: Option<i32>,

    // ---- the level-limit block -----------------------------------------------------------------
    /// `InqInt(0x56)` `MinLevel` and `InqInt(0x57)` `MaxLevel`, both defaulted to `-1` by the
    /// block itself so an absent key and a non-positive one take the same arm.
    pub level_limits: (Option<i32>, Option<i32>),
    /// String property `0x26` `AppraisalPortalDestination` — *"Destination: "* plus the text.
    pub portal_destination: Option<String>,

    // ---- the wield-requirements block ----------------------------------------------------------
    /// `InqBool(0x55)` `AppraisalHasAllowedWielder`, tested `== 1`.
    pub has_allowed_wielder: bool,
    /// String property `0x19` `CraftsmanName` — the `%s` of the allowed-wielder, allowed-activator
    /// and *"Created by %s."* lines alike. Its default in the first two is *"the original owner"*.
    pub craftsman_name: Option<String>,
    /// `InqInt(0x1A)` `AccountRequirements`, `== 1` being *"Use requires Throne of Destiny."*
    pub account_requirements: Option<i32>,
    /// `InqInt(0x144)` `HeritageSpecificArmor`, resolved through the appraisal system's
    /// heritage-group display name.
    pub heritage_specific_armor: Option<String>,
    /// The three `(requirement, skill-or-attribute id, difficulty)` triples —
    /// `0x9E/0x9F/0xA0`, `0x10E/0x10F/0x110`, `0x111/0x112/0x113`. All three of a triple must be
    /// present or the triple draws nothing.
    pub wield_requirements: Vec<WieldRequirementView>,

    // ---- the usage-limit block -----------------------------------------------------------------
    /// `InqInt(0x171)` `UseRequiresLevel`, above 0.
    pub use_requires_level: Option<i32>,
    /// `InqInt(0x16E)` `UseRequiresSkill` resolved to a name, with `InqInt(0x16F)`
    /// `UseRequiresSkillLevel`. `None` for the name is the block's *"Unknown Skill"*.
    pub use_requires_skill: Option<(Option<String>, i32)>,
    /// `InqInt(0x170)` `UseRequiresSkillSpec` resolved to a name — *"Use requires specialized %s."*
    pub use_requires_skill_spec: Option<Option<String>>,

    // ---- the item-level block ------------------------------------------------------------------
    /// Int64 property `5` `ItemBaseXp`, `InqInt(0x13F)` `ItemMaxLevel`, `InqInt(0x140)`
    /// `ItemXpStyle` and int64 property `4` `ItemTotalXp` — the four the level line needs.
    pub item_level: Option<ItemLevelView>,
    /// `InqInt(0x160)` `CloakWeaveProc`, `== 2` being the damage-reduction sentence.
    pub cloak_weave_proc: Option<i32>,

    // ---- the activation-requirements block -----------------------------------------------------
    /// `InqInt(0x6D)` `ItemDifficulty`, above 0 — *"Arcane Lore: %d"*.
    pub item_difficulty: Option<i32>,
    /// `InqInt(0x6E)` `ItemAllegianceRankLimit`, above 0 — *"Allegiance Rank: %d"*.
    pub allegiance_rank_limit: Option<i32>,
    /// `InqInt(0xBC)` `HeritageGroup` through.
    pub heritage_group: Option<String>,
    /// `InqInt(0x73)` `ItemSkillLevelLimit` above 0 with `InqInt(0xB0)` `AppraisalItemSkill`
    /// resolved to a name — *"%s: %d"*. A skill the table has no row for drops the whole term.
    pub activation_skill: Option<(String, i32)>,
    /// `InqInt(0x102)` `ItemAttributeLevelLimit` above 0 with `InqInt(0x101)`
    /// `ItemAttributeLimit` through the attribute-name lookup.
    pub activation_attribute: Option<(String, i32)>,
    /// `InqInt(0x104)` `ItemAttribute2ndLevelLimit` above 0 with `InqInt(0x103)`
    /// `ItemAttribute2ndLimit` through the secondary-attribute-name lookup.
    pub activation_attribute_2nd: Option<(String, i32)>,
    /// `InqBool(0x5E)` `AppraisalHasAllowedActivator`, tested `== 1`.
    pub has_allowed_activator: bool,

    // ---- the boost-value block / the heal-kit block --------------------------------------------
    /// `InqInt(0x5A)` `BoostValue` — the `%d` of both blocks' first line.
    pub boost_value: Option<i32>,
    /// `InqInt(0x59)` `BoosterEnum` — `2` Health, `4` Stamina, `6` Mana, and nothing else draws.
    pub booster_enum: Option<i32>,
    /// Float property `0x64` `HealkitMod` — *"Restoration Bonus: %d%%"*, the float times **100**.
    ///
    pub healkit_mod: Option<f64>,

    // ---- the capacity block --------------------------------------------------------------------
    /// `InqInt(0xAF)` `AppraisalMaxPages` and `InqInt(0xAE)` `AppraisalPages` — the
    /// *"%d of %d pages full."* pair, and both are needed.
    pub pages: Option<(i32, i32)>,

    // ---- the mana-stone block ------------------------------------------------------------------
    /// `InqInt(0x6B)` `ItemCurMana` — *"Stored Mana: %d"*. The whole block is gated on the
    /// profile carrying **no** spell book.
    pub stored_mana: Option<i32>,
    /// Float property `0x57` `ItemEfficiency` — *"Efficiency: %d%%"*, truncated, **not** scaled.
    pub item_efficiency: Option<f64>,
    /// Float property `0x89` `ManaStoneDestroyChance` — *"Chance of Destruction: %d%%"*, likewise.
    pub destroy_chance: Option<f64>,

    // ---- the remaining-uses block --------------------------------------------------------------
    /// `InqInt(0xC1)` `NumKeys` — *"Contains %d key."* / *"…keys."*
    pub num_keys: Option<i32>,
    /// `InqBool(0x3F)` `UnlimitedUse` — *"Number of uses remaining:  Unlimited"* (two spaces).
    pub unlimited_use: bool,
    /// `InqInt(0x5C)` `Structure` — *"Number of uses remaining: %d"*.
    pub structure: Option<i32>,

    // ---- the is-sellable line ------------------------------------------------------------------
    /// `IsSellable`, and the test is **present and false** — `InqBool` answering true with a
    /// zero value. An absent key draws nothing.
    pub cannot_be_sold: bool,

    // ---- the rare-info block -------------------------------------------------------------------
    /// `InqBool(0x6C)` `RareUsesTimer`, tested `== 1`. Both `%d` of its sentence are the literal
    /// `3` the block pushes, not a property.
    pub rare_uses_timer: bool,
    /// `InqInt(0x11)` `RareId` — *"Rare #%d"*.
    pub rare_id: Option<i32>,
    /// `InqBool(3)` `Locked`. `None` is the client's `InqBool` answering false, which takes the
    /// block's **other** arm — the `Bonus to Lockpick Skill` line — not a silent skip.
    pub locked: Option<bool>,
    /// `InqInt(0x26)` `ResistLockpick` — the `%d` of the last line, and the gate that decides
    /// between it and *"You can't tell how hard the lock is to pick."*
    pub resist_lockpick: Option<i32>,
    /// `InqInt(0xAD)` `AppraisalLockpickSuccessPercent`, fed to the appraisal system's
    /// lockpick-percent formatter.
    pub lockpick_success_percent: Option<i32>,

    // ---- the special-properties block ----------------------------------------------------------
    /// The block's own inputs; see [`SpecialPropertiesView`].
    pub special: SpecialPropertiesView,

    // ---- the short and long magic blocks -------------------------------------------------------
    /// The two spell blocks' inputs; see [`MagicInfoView`]. The spell book behind it is
    /// decoded by the appraisal profile and cached whole.
    pub magic: MagicInfoView,

    // ---- the inscription box -------------------------------------------------------------------
    /// Bit `0x2` of the public-description flags, or the hooked-item inscription rule — the
    /// one test the inscription check makes before it looks at the profile.
    pub inscribable: bool,
    /// The viewer's PSR flag: Boolean quality `0x2C`, `0x2D`, or
    /// `0x61`. The inscription's editable-state setter consults it only after the displayed
    /// object's live PWD inscribable-bit gate and an ordinary scribe/ownership check fail.
    pub viewer_is_psr: bool,
    /// String property `8` `ScribeName`.
    pub scribe_name: Option<String>,
    /// String property `7` `Inscription`.
    pub inscription: Option<String>,
    /// The is-owned-by-the-player walk — owned-by-object against the player id, which is
    /// "this object *is* the player, or its container or wielder is, or it is inside
    /// something that is".
    ///
    /// The one question the inscription's editable-state setter asks that is not on the profile:
    /// is this object the player's? The game world's ownership query performs the same walk and
    /// answers it at the seam.
    pub owned_by_player: bool,

    // ---- the character pane's own appraise-info setter ----------------------------------------
    //
    // The character pane's own half: the society, allegiance and armour blocks. Every key below is an
    // `[AssessmentProperty]` in `ACE.Entity/Enum/Properties`, so the shard sends all of them.
    /// The appraisal system's gender-and-heritage display of `InqInt(0x71)` `Gender`,
    /// `InqInt(0xBC)` `HeritageGroup` and — only when the heritage is **0** — `InqInt(2)`
    /// `CreatureType`. Written to the heritage text `0x10000150`.
    ///
    /// Resolved at the seam because two of its three terms are dat lookups (`EnumMapper`s
    /// `0x1000000C` and `0x10000002`), which this crate cannot read.
    pub gender_heritage_display: Option<String>,
    /// The profession text `0x10000151`, and it is a **two-source** line:
    /// the character-title table's lookup of
    /// `InqInt(0x105)` `CharacterTitleId` first, and string property `5` `Template` only if that
    /// `InqInt` missed **or** the title table had no row (a flag is set only inside the
    /// found arm, and testing it is what skips the fallback).
    pub profession: Option<String>,
    /// The is-PK predicate — `_bitfield & 0x20` — on the **examined**
    /// object, for the PK-status text `0x10000152`.
    pub weenie_is_pk: bool,
    /// The is-PK-lite predicate — `_bitfield & 0x2000000`, asked only when
    /// is-PK said no.
    pub weenie_is_pk_lite: bool,
    /// Resolve this object's allegiance title from rank, heritage, and gender —
    /// `dereth_client_model::allegiance::get_title`.
    ///
    /// The allegiance full name is `title + " " + name` when this is `Some` and
    /// the bare name when it is `None`, and that is written over the title bar the
    /// enclosing pane had already filled.
    pub allegiance_title: Option<String>,
    /// `InqInt(0x119)` `Faction1Bits` on the examined object — the society selector, tested
    /// `& 1` Celestial Hand, `& 2` Eldrytch Web, `& 4` Radiant Blood.
    /// Present-but-none-of-the-three draws *"???"*.
    pub faction_bits: Option<i32>,
    /// The **viewer's** own `Faction1Bits`, read off `PlayerDesc` through
    /// integer quality `0x119`, and used for nothing but the
    /// row's colour: same society `1`, a different one `2`, none `0`.
    pub viewer_faction_bits: i32,
    /// `InqInt(0x11F/0x120/0x121)` `SocietyRankCelhan/Eldweb/Radblo`, in that order. The client
    /// reads only the one its `Faction1Bits` selected and leaves the local at **0** when the key
    /// is absent, so an absent rank and a zero rank take the same arm — no suffix.
    pub society_ranks: [i32; 3],
    /// `InqInt(0x1E)` `AllegianceRank`. `< 1` skips the whole allegiance block.
    pub allegiance_rank: Option<i32>,
    /// String property `0x2F` `AllegianceName` — the allegiance-name text `0x1000053A`, which is
    /// cleared unconditionally first.
    pub allegiance_name: Option<String>,
    /// String property `0x15` `MonarchsTitle`. Absent takes the *"Alleg. Monarch:"* follower arm.
    pub monarch_title: Option<String>,
    /// String property `0x23` `PatronsTitle`. Equal to the monarch's is one *"Monarch/Patron:"*
    /// row; different is two rows; absent is *"Monarch:"* alone.
    pub patron_title: Option<String>,
    /// `InqInt(0x23)` `AllegianceFollowers` — the same key number in the **int** table, which is
    /// how one property id serves two rows. Absent or negative is 0.
    pub allegiance_followers: Option<i32>,
    /// The `0x4000` base-armour block: head, chest, groin, bicep, wrist, hand, thigh,
    /// shin and foot. A value `>= 9999` marks an unenchantable piece and draws `*`
    /// plus `v - 9999`. Public ACE's `ArmorLevel.GetArmorLevel` adds the same 9999
    /// for that purpose.
    pub base_armor: Option<[i32; 9]>,
    /// The thirteen rating properties both panes read:
    /// `0x133 DamageRating`, `0x134 DamageResistRating`, `0x139 CritRating`,
    /// `0x13A CritDamageRating`, `0x13B CritResistRating`, `0x13C CritDamageResistRating`,
    /// `0x143 HealingBoostRating`, `0x15E DotResistRating`, `0x15F LifeResistRating`, then
    /// `0x17D PKDamageRating`, `0x17E PKDamageResistRating`, `0x182 Overpower` and
    /// `0x183 OverpowerResist`.
    ///
    /// See `crate::panels::examination::CHARACTER_RATING_ROWS`; `0x143` is read and **never used**.
    pub ratings: [Option<i32>; 13],
    /// String property `0x0A` `Fellowship` — *"Fellowship:"*.
    pub fellowship: Option<String>,
    /// String property `0x2B` `DateOfBirth` — *"Arrived in Dereth:"*.
    pub date_of_birth: Option<String>,
    /// `InqInt(0x7D)` `Age`, through the client's elapsed-time formatter —
    /// *"Time in Dereth:"*. That formatter already exists as
    /// `crate::panels::journal::delta_time_to_string`.
    pub age: Option<i32>,
    /// `InqInt(0xB5)` `ChessRank` — *"Chess Rank:"*.
    pub chess_rank: Option<i32>,
    /// `InqInt(0xC0)` `FakeFishingSkill` — *"Fishing Skill:"*.
    pub fishing_skill: Option<i32>,
    /// `InqInt(0x2B)` `NumDeaths` — *"Deaths:"*, and `<= 0` is *"Has never died"*.
    /// The **same** key number as [`Self::date_of_birth`] in the string table.
    pub num_deaths: Option<i32>,
    /// `InqInt(0x106)` `NumCharacterTitles` — *"Titles Earned:"*.
    pub num_character_titles: Option<i32>,
    /// `InqInt(0x186)` `Enlightenment` — the character pane's *"Enlightenment:"* line, drawn
    /// whenever the property is present, zero included.
    pub enlightenment: Option<i32>,
}

/// One `(requirement, skill/attribute, difficulty)` triple of the wield-requirements block.
///
/// The block reads three of these — `0x9E/0x9F/0xA0`, `0x10E/0x10F/0x110`, `0x111/0x112/0x113` —
/// and runs the identical switch on each. `subject` is what the item-examine window's
/// requirement-string helper makes of `(requirement, skill)`,
/// resolved at the seam because two of its arms are dat lookups (`SkillTable 0x0E000004`,
/// `EnumMapper 0x10000002`); `None` is that function leaving the string as the `"base "`/`""`
/// prefix it starts with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WieldRequirementView {
    /// `InqInt(0x9E)` `WieldRequirements` and its two continuations — the switch selector.
    pub requirement: i32,
    /// `InqInt(0xA0)` `WieldDifficulty` and its two continuations — the `%d`, and for
    /// `requirement == 8` the trained/specialized fork (`!= 3` is *"trained"*).
    pub difficulty: i32,
    /// What the appraisal requirement-string lookup answers for `(requirement, skill)`.
    pub subject: Option<String>,
}

/// The four values the item-level block needs before it can draw a level.
///
/// All four must be present and the last two positive, which is the block's own four-term
/// guard.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ItemLevelView {
    /// Int64 property `4` `ItemTotalXp`.
    pub total_xp: u64,
    /// Int64 property `5` `ItemBaseXp` — and the guard is that it is **non-zero**, not positive.
    pub base_xp: u64,
    /// `InqInt(0x13F)` `ItemMaxLevel`, above 0.
    pub max_level: i32,
    /// `InqInt(0x140)` `ItemXpStyle`, above 0; `1`, `2` and `3` are the three curves the
    /// item-level inverse knows.
    pub xp_style: i32,
}

/// The `WeaponProfile` block (`0x0020`) of an `AppraisalProfile`, carried whole
/// because the weapon-and-armour display reads nine of its ten fields.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponView {
    pub damage_type: u32,
    pub weapon_time: i32,
    pub weapon_skill: u32,
    pub weapon_damage: i32,
    pub damage_variance: f64,
    pub damage_mod: f64,
    pub max_velocity: f64,
    pub weapon_offense: f64,
    /// `max_velocity_estimated` — non-zero appends `" (based on STRENGTH 100)"` to the range line.
    pub max_velocity_estimated: i32,
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
    /// `crate::panels::inventory::InventoryPanels`, gathered for `items` + `equipment` only,
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

/// One allegiance member projected for the panel.
///
/// The host walks the hierarchy's monarch, patron and direct vassals and supplies
/// the four fields the panel reads from each node. The rest of `AllegianceData`
/// stays behind the seam.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceEntry {
    pub id: ObjectId,
    /// The allegiance full name — the rank title, a space, then the name, or the
    /// bare name when the title lookup fails. **Already joined**, because the title tables live in
    /// `dereth-client-model` and this crate does not depend on it.
    pub full_name: String,
    /// The is-logged-in bit — `_bitfield & 1`. The vassals update
    /// reads it twice: once to decide the row's `0x100004AA` state and once, across the whole
    /// list, to enable talk focus 6 when any vassal is logged in.
    pub logged_in: bool,
    /// The number `ID_Allegiance_Rank` prints beside the title.
    pub rank: u16,
    /// The experience this vassal has passed up, the
    /// value variable of `ID_Allegiance_VassalExperiencePassedUp` on element `0x10000269`.
    pub cp_cached: u32,
}

/// The allegiance header, monarch, patron and direct vassals.
///
/// An allegiance the player is not in is [`Self::default`] — `total == 0`, no monarch, no patron,
/// no vassals — which is a state the panel must render *correctly* rather than merely render
/// emptily, and is the only state the capture corpus can witness (see
/// `crate::panels::allegiance`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceRoster {
    /// The allegiance name, present in version 8 and later and empty when unnamed.
    pub allegiance_name: String,
    /// The total-member count -- the size of the whole allegiance as the
    /// server counts it, monarch included. The monarch row shows **this minus one**
    /// as the monarch's Followers.
    /// It is not the count of records in the message, which is only the player's neighbourhood
    /// and which no panel update reads.
    pub total_members: u32,
    /// The player's total-vassal count for the whole follower subtree. The player row
    /// shows it unchanged.
    pub total_vassals: u32,
    /// The player's own tithed experience. Unlike a vassal row's
    /// [`AllegianceEntry::cp_cached`], this is the value the patron-is-monarch arm puts in child
    /// `0x10000492`.
    pub own_cp_tithed: u32,
    /// The player's own node, when the tree holds one.
    pub subject: Option<AllegianceEntry>,
    /// The player's allegiance-rank quality (int property 30) as the ordinary quality read answers
    /// it: the stored value passed through the int enchantments, since the quality filter lists this
    /// property; **0** when the player carries none, the value the panel seeds its read with.
    ///
    /// The rank line compares it with the tree's rank for [`Self::subject`]: equal (or `-1`)
    /// prints the plain rank, anything else the buffed form with the difference. A spell that
    /// raises the allegiance rank is the only way the two differ, because the shard sends the
    /// stored rank.
    pub player_rank_quality: i32,
    pub monarch: Option<AllegianceEntry>,
    pub patron: Option<AllegianceEntry>,
    /// Newest first: the client inserts at the head of the vassal list
    /// and its first-vassal query returns that head.
    pub vassals: Vec<AllegianceEntry>,
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
    /// `dereth_client::vendor_view` fork, because retail reads both out of the one
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

/// The special-properties block's inputs.
///
/// Each field is one `Inq*` in the client's own order; the key of every one is named and addressed
/// in `dereth_client_model::appraisal_model::property::special`. The whole block is a **list**, joined
/// with `", "` and drawn as one `"Properties: …"` line, plus three
/// stand-alone sentences around it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpecialPropertiesView {
    /// `InqInt(0x117)` — *"You can only carry N of these items."*
    pub unique_limit: Option<i32>,
    /// Present Float0xA7 gates both cooldown lines, including zero duration.
    pub cooldown_duration: Option<f64>,
    /// Int0x118 presence also owns the trailing blank, even when the group is not active.
    pub cooldown_group: Option<u32>,
    /// PlayerDesc registry query for group+0x8000 at the current display time, not item qualities.
    pub cooldown_remaining: Option<f64>,
    /// `InqInt(0x124)` — the `%d` of *"Cleave: %d enemies in front arc."*, drawn only above 1.
    pub cleave: Option<i32>,
    /// `InqInt(0xA6)` and its resolved slayer name. Both are carried because the **id** decides
    /// the `Bael'Zharon's Hate` special case
    /// (`0x1F`) and the *name* is what gets `" slayer"` appended.
    pub slayer: Option<(i32, String)>,
    /// `InqInt(0x2F)` `WeaponSkill`.
    pub weapon_skill: Option<i32>,
    /// The five `ImbuedEffect` ints `or`-ed together, and `None` when not one of them was present
    /// — which is not the same as `Some(0)`: the *"cannot be further imbued"* sentence is drawn on
    /// a non-zero mask.
    pub imbued: Option<u32>,
    /// Float property `0x9F`.
    pub absorb_magic_damage: bool,
    /// `InqInt(0x24)`.
    pub item_spellcraft: Option<i32>,
    /// `InqInt(0x72)` `Attuned`, as the raw attuned-status value.
    pub attuned: Option<i32>,
    /// `InqInt(0x21)` `Bonded`, as the raw bonded-status value — **signed**, because
    /// destroy-on-death is `-2` and slippery is `-1`.
    pub bonded: Option<i32>,
    /// `InqBool(0x5B)`.
    pub retained: Option<bool>,
    /// Float property `0x88`.
    pub critical_multiplier: bool,
    /// Float property `0x93`.
    pub critical_frequency: bool,
    /// Float property `0x9B`.
    pub ignore_armor: bool,
    /// `InqInt(0x107)`, and only when float property `0x9D` was present too — the pair is one `&&`.
    pub resistance_cleaving: Option<u32>,
    /// DataID property `0x37`.
    pub proc_spell: bool,
    /// `InqBool(0x63)`.
    pub ivoryable: Option<bool>,
    /// `InqBool(0x64)`.
    pub dyeable: Option<bool>,
    /// `InqBool(0x82)`.
    pub tethered_left: Option<bool>,
}

/// One entry in the appraisal spell book's `0x0010` block, with its two dat strings
/// already resolved.
///
/// The client asks the magic system for the spell's name and its description,
/// both of which come from the spell table selected by group 6, type 2, and enum `0x10000005` —
/// the portal dat's `SpellTable 0x0E00000E`. This crate does not read dats, so the
/// strings cross the seam already looked up, the same way `creature_display_name` does.
///
/// **A spell id the table does not know still produces an entry**, with empty strings: the short
/// list sets its flag before the name is tested, so an unknown id still contributes its `", "`
/// separator and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppraisalSpellView {
    /// The id **as it arrived**, high bit and all — kept because it is the only thing that
    /// distinguishes two entries with the same resolved name.
    pub raw_id: u32,
    /// `raw_id & 0x80000000`. The bit is masked off before the lookup,
    /// and it is the **only** thing that decides where the entry goes: set sends it
    /// to the *"Enchantments:"* paragraph, clear to *"Spell Descriptions:"* and to the
    /// `Spells: ` short list, which skips set ids outright.
    pub enchantment: bool,
    /// The resolved spell name, or `""` for an id the `SpellTable` has no row for.
    pub name: String,
    /// The resolved spell description, likewise empty for an unknown id.
    pub description: String,
}

/// The inputs of the short and long magic-info blocks that put an item's spells in the appraisal
/// pane.
///
/// Both blocks first test the spell book and return when it is null, so the `Option` is not
/// decoration: a profile with **no** spell block draws
/// nothing at all, while one carrying an empty list on a failed assess still draws
/// *"Spells: unknown."* — twice, once from each block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MagicInfoView {
    /// The resolved spell-book rows. `None` is the client's null spell book.
    pub spells: Option<Vec<AppraisalSpellView>>,
    /// `InqInt(0x6A)` — *"Spellcraft: %d."*
    pub spellcraft: Option<i32>,
    /// `InqInt(0x6B)` — the first `%d` of *"Mana: %d / %d."*
    pub cur_mana: Option<i32>,
    /// `InqInt(0x6C)` — the second.
    pub max_mana: Option<i32>,
    /// Float property `5` `ManaRate`. Present takes the *"1 point per %d seconds"* arm and
    /// [`Self::mana_cost`] is never asked for.
    pub mana_rate: Option<f64>,
    /// `InqInt(0x75)` — *"Mana Cost: %d."*, and above zero the Mana Conversion footnote.
    pub mana_cost: Option<i32>,
}

/// The vendor window's whole state as one snapshot.
///
/// [`Self::open`] records whether the client has a nonzero vendor id; a closed shop is
/// [`Self::default`], and the panel must render that correctly rather than merely render it
/// emptily — see `crate::panels::vendor`.
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
    /// is `Σ max(stack size, 1)` over the buy list and **not** the
    /// number of rows and **not** the money.
    ///
    /// It is a separate accumulator from the transaction value in the same loop, and it decides the
    /// `"item"` / `"items"` fork on `count == 1`, so a station that only compared the
    /// money could not tell the two apart.
    pub buy_items: i32,
    /// The same count for the sell basket — `L"Selling %d %s worth %hsp"`'s `%d`, accumulated the
    /// same way.
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
    /// row to remove is `crate::panels::trade::TradePanel::pending`. A refused id is
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
    /// `crate::panels::trade::TradePanel`'s module header.
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
    /// `crate::panels::trade::TradePanel::update` takes it together with its own `removed_now`,
    /// which stays because it covers the one case a model flag cannot: an **optimistic** row the
    /// mirror never held.
    pub acceptance_darkened: bool,
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

/// A `GameView` that knows nothing, for constructing a screen with no world attached.
///
/// This is what the acceptance gate builds every panel against before it feeds one the frozen
/// snapshot: a panel that cannot be constructed against an empty world has state it should not
/// have.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyGameView;
impl GameView for EmptyGameView {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the HUD's vital-stat table uses current id 2 and maximum id 1 for health.
    #[test]
    fn the_vital_stat_numbers_are_the_documented_attribute_2nd_types() {
        assert_eq!(Vital::Health.stats(), (2, 1));
        assert_eq!(Vital::Stamina.stats(), (4, 3));
        assert_eq!(Vital::Mana.stats(), (6, 5));
    }

    /// Oracle: the panel id is itself an element id, so a request
    /// carries the raw id and never a "which panel is this" enum. Trap 11.
    #[test]
    fn a_panel_visibility_request_carries_the_raw_panel_id() {
        let r = UiRequest::SetPanelVisibility {
            panel: 0x1000_018B,
            visible: true,
        };
        assert_eq!(
            r,
            UiRequest::SetPanelVisibility {
                panel: 0x1000_018B,
                visible: true
            }
        );
    }
}
