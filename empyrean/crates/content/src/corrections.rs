//! Not ACE: corrections to ACE's world data where retail shows the stored value is wrong.
//!
//! `world.pack` is ACE's world-database dump as it stands; it is never edited by hand. Each entry
//! here names one weenie property or spell field, the value the dump stores and the value retail
//! used, with the evidence and its DIVERGENCES row. The server applies them when it reads a weenie
//! ([`crate::WorldDatabaseBase::get_weenie`] and `get_all_weenies`) or a spell
//! (`get_cached_spell`, `cache_all_spells`), so every weenie the game creates and every spell it
//! casts carries the corrected value, whichever pack or test content it came from (a developer
//! export of a weenie writes the corrected value too). The pack itself keeps ACE's stored values;
//! [`crate::WorldDatabaseBase::get_stored_weenie`] reads a weenie as stored.
//!
//! An entry applies only while the row still holds the stored value it names: a row an overlay
//! edit changed, or a dump that has since been fixed, is left alone, and [`stale_corrections`]
//! and [`stale_spell_corrections`] report the entries that no longer match.
//!
//! Entries are small and individually evidenced: each names one object whose retail value was seen.
//! A difference that repeats across a family of weenies is treated as a rule to understand, not as
//! data to patch. Once it is understood, it is corrected by one evidenced rule, not by an entry per
//! weenie. There are two such rules: [`play_script_shift`] (the
//! default script, V337) and [`emote_motion_shift`] (emote motions, styles and substyles, V338).
//! [`play_script_shifts`] and [`emote_motion_shifts`] list what each changes on a given content.
//! Individual entries stay individually evidenced, and neither bulk entries nor a new rule are
//! added here without a deliberate, reviewed decision.
//!
//! The individual entries are scoped to the eras whose content they were evidenced on
//! ([`ENTRY_ERAS`]: the end of retail). On a pack built for another era ([`crate::pack::Pack::era`])
//! none applies, and none is reported stale or absent. The two rules correct how ACE's world data
//! numbers the client's enums, which every era's content shares and every era's client reads with
//! the end-of-retail enums, so they apply to every pack.
//!
//! Since the pack's content hash covers only what is stored, [`digest`] names this build's
//! corrections (every entry and each rule's full definition, [`canonical_listing`]); the server
//! logs it beside the content hash and reports both in `@empversion` and its status JSON.
//! [`report::CorrectionsReport`] lists what each entry and rule does to one content
//! (`empyrean-import --corrections`).

use empyrean_common::era::EraId;
use empyrean_entity::enums::{
    MotionCommand, PlayScript, PropertyBool, PropertyDataId, PropertyInt, WeenieType,
};

use crate::models::world::{
    Spell, Weenie, WeeniePropertiesBool, WeeniePropertiesDID, WeeniePropertiesEmoteAction,
    WeeniePropertiesInt,
};
use crate::pack::TableId;
use crate::world_database::WorldDatabaseBase;

pub mod report;

/// A property value in a correction. `None` is "no row".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Int(PropertyInt, Option<i32>),
    Bool(PropertyBool, Option<bool>),
    DataId(PropertyDataId, Option<u32>),
    /// The script (`PScript`) of one emote action; `None` is an action with no script.
    EmoteScript(EmoteAction, Option<i32>),
}

/// One action of a weenie's emote table: the emote set's row id and the action's order in it.
/// An action the table does not have holds no value, so its entry reports as stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmoteAction {
    pub emote_set: u32,
    pub order: u32,
}

/// One corrected weenie property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeenieCorrection {
    pub weenie_class_id: u32,
    /// The property and the value the dump stores.
    pub stored: Value,
    /// The same property and the value retail used (`None` removes the row).
    pub corrected: Value,
    /// The DIVERGENCES row.
    pub divergence: &'static str,
    /// Where the evidence is.
    pub evidence: &'static str,
}

/// The retail in-flight state of a ring, wave or bomb piece: ReportCollisions, Missile, AlignPath,
/// PathClipped, LightingOn, Inelastic.
const RING_PIECE_IN_FLIGHT: i32 = 0x0002_0B48;
/// What the dump stores for nine of them: the state after impact (Ethereal, IgnoreCollisions,
/// NoDraw, Inelastic, Cloaked).
const AFTER_IMPACT: i32 = 0x0012_0034;

const PROJECTILE_STATE: &str = "retail captures: projectile-physics-state";

const fn in_flight(weenie_class_id: u32) -> WeenieCorrection {
    WeenieCorrection {
        weenie_class_id,
        stored: Value::Int(PropertyInt::PhysicsState, Some(AFTER_IMPACT)),
        corrected: Value::Int(PropertyInt::PhysicsState, Some(RING_PIECE_IN_FLIGHT)),
        divergence: "V313",
        evidence: PROJECTILE_STATE,
    }
}

const CREATE_OBJECT_FIELDS: &str = "retail captures: createobject-fields";

const ITEM_TYPE_BITS: &str = "retail captures: itemtype-fletching-bits";

/// `Misc`.
const MISC: i32 = 0x80;
/// `CraftCookingBase`.
const CRAFT_COOKING_BASE: i32 = 0x0040_0000;
/// A bit no `ItemType` of the client's names (the six Brewmaster's Bible pieces in the dump).
const UNNAMED_ITEM_TYPE: i32 = 0x0200_0000;

const fn item_type(weenie_class_id: u32, stored: i32, corrected: i32) -> WeenieCorrection {
    WeenieCorrection {
        weenie_class_id,
        stored: Value::Int(PropertyInt::ItemType, Some(stored)),
        corrected: Value::Int(PropertyInt::ItemType, Some(corrected)),
        divergence: "V327",
        evidence: ITEM_TYPE_BITS,
    }
}

const fn bool_fix(
    weenie_class_id: u32,
    p: PropertyBool,
    stored: bool,
    corrected: bool,
) -> WeenieCorrection {
    WeenieCorrection {
        weenie_class_id,
        stored: Value::Bool(p, Some(stored)),
        corrected: Value::Bool(p, Some(corrected)),
        divergence: "V313",
        evidence: PROJECTILE_STATE,
    }
}

const ENUM_SHIFT_OTHER_FIELDS: &str = "retail captures: enum-shift-other-fields";

/// A Taunt set's first action (a script, order 0) that the dump stores as `AetheriaLevelUp` (161)
/// and retail played as `LevelUp` (138).
const fn taunt_script(weenie_class_id: u32, emote_set: u32) -> WeenieCorrection {
    let action = EmoteAction {
        emote_set,
        order: 0,
    };
    #[allow(clippy::cast_possible_wrap)]
    let (stored, corrected) = (
        PlayScript::AetheriaLevelUp.0 as i32,
        PlayScript::LevelUp.0 as i32,
    );
    WeenieCorrection {
        weenie_class_id,
        stored: Value::EmoteScript(action, Some(stored)),
        corrected: Value::EmoteScript(action, Some(corrected)),
        divergence: "V338",
        evidence: ENUM_SHIFT_OTHER_FIELDS,
    }
}

/// Every correction, in class id order.
pub const WEENIE_CORRECTIONS: &[WeenieCorrection] = &[
    // Items the dump types wrongly: retail created each of these with one ItemType throughout
    // the captures (a census of every CreateObject). The Brewmaster's Bible pieces carry a bit
    // no client ItemType names, and no object retail created carried it.
    item_type(29065, MISC, CRAFT_COOKING_BASE), // Healing Machine Base
    item_type(29204, MISC, CRAFT_COOKING_BASE), // Tusker Spit (3,756 CreateObjects)
    item_type(29205, UNNAMED_ITEM_TYPE, MISC),  // Brewmaster's Front Cover (57)
    item_type(29206, UNNAMED_ITEM_TYPE, MISC),  // Brewmaster's Back Cover (5)
    item_type(29207, UNNAMED_ITEM_TYPE, MISC),  // Brewmaster's Pages (6)
    item_type(29208, UNNAMED_ITEM_TYPE, CRAFT_COOKING_BASE), // Brewmaster's Spine (9)
    item_type(29209, UNNAMED_ITEM_TYPE, CRAFT_COOKING_BASE), // Incomplete Brewmaster's Bible (2)
    item_type(29210, UNNAMED_ITEM_TYPE, CRAFT_COOKING_BASE), // Nearly Complete Brewmaster's Bible (2)
    // Ring around the Rabbit: the dump's state is not a flight state (Ethereal, IgnoreCollisions,
    // LightingOn, Inelastic); retail flew it as 0x28B48 (7,952 CreateObjects), with
    // ScriptedCollision, which a ring piece keeps only when its stored state has it (V314).
    WeenieCorrection {
        weenie_class_id: 33040,
        stored: Value::Int(PropertyInt::PhysicsState, Some(0x0002_0814)),
        corrected: Value::Int(PropertyInt::PhysicsState, Some(0x0002_8B48)),
        divergence: "V313",
        evidence: PROJECTILE_STATE,
    },
    // Dark Vortex: its state has no LightingOn and retail sent none (0x20348, 2,028
    // CreateObjects), but its LightsStatus bool says on and takes precedence.
    bool_fix(33498, PropertyBool::LightsStatus, true, false),
    // The ring, wave and bomb pieces stored with the state they end in: retail created them
    // flying (0x20B48; Flame Wave 13,557 CreateObjects, Shock Waves 4,066, Frost Wave 195,
    // Lightning Wave 48, Force Bomb 12, Acid Bomb 19), then set the impact state on collision.
    in_flight(33727), // Heavy Blade Ring (not seen in the captures; the same stored value)
    in_flight(33845), // Acid Bomb
    in_flight(33846), // Blade Bomb (seen only after impact)
    in_flight(33848), // Force Bomb
    in_flight(33851), // Shock Bomb (seen only after impact)
    in_flight(33862), // Flame Wave
    in_flight(33864), // Frost Wave
    in_flight(33865), // Lightning Wave
    in_flight(33866), // Shock Waves
    // The nether projectiles: retail's 43231 flew with Gravity (0x28F48, 39,289 CreateObjects
    // over 423 sessions) and its 43232 without (0x28B48, 2,023): the reverse of the dump's
    // GravityStatus bools. 43231 is the arc's projectile on retail (`SPELL_CORRECTIONS`).
    bool_fix(43231, PropertyBool::GravityStatus, false, true),
    bool_fix(43232, PropertyBool::GravityStatus, true, false),
    // Nalicana: the dump has no icon, and retail sent 0x06006E2E on each of its 158 CreateObjects.
    WeenieCorrection {
        weenie_class_id: 43398,
        stored: Value::DataId(PropertyDataId::Icon, None),
        corrected: Value::DataId(PropertyDataId::Icon, Some(0x0600_6E2E)),
        divergence: "V335",
        evidence: CREATE_OBJECT_FIELDS,
    },
    // The Taunt script of the Zefir, Anekshen and Ruuk family: the dump stores AetheriaLevelUp
    // (161) and retail played LevelUp (138) on the monster as the taunt ran (never 161). Not the
    // old numbering, which would give 162; both are keys of these monsters' script tables, so
    // only the wire tells them apart. (The number of sends retail was seen making.)
    taunt_script(52575, 85961), // Zefir Thorn Reaver (8)
    taunt_script(52583, 33142), // Anekshen Storm Caller (6)
    taunt_script(52585, 33143), // Anekshen Storm Reaver (1)
    taunt_script(52587, 33144), // Anekshen Thorn Dancer (3)
    taunt_script(52589, 33145), // Anekshen Thorn Reaver (9)
    taunt_script(52620, 68870), // Poisonous Brier Wasp (5)
    // Deadly Lightning Volley: its state has Gravity and retail sent it (0x28F48), but its
    // GravityStatus bool says off and takes precedence.
    bool_fix(52621, PropertyBool::GravityStatus, false, true),
    // (The Taunt scripts continue.)
    taunt_script(52626, 38325), // Oaken Guardian (3)
    taunt_script(52627, 85916), // Guardian Wisp (6)
    taunt_script(52712, 35686), // Ruuk Ranger (17)
];

/// A spell field in a correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellValue {
    /// The projectile weenie (`wcid`).
    Wcid(Option<u32>),
}

/// One corrected spell field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellCorrection {
    pub spell_id: u32,
    /// The field and the value the dump stores.
    pub stored: SpellValue,
    /// The same field and the value retail used.
    pub corrected: SpellValue,
    /// The DIVERGENCES row.
    pub divergence: &'static str,
    /// Where the evidence is.
    pub evidence: &'static str,
}

const fn projectile(spell_id: u32, stored: u32, corrected: u32) -> SpellCorrection {
    SpellCorrection {
        spell_id,
        stored: SpellValue::Wcid(Some(stored)),
        corrected: SpellValue::Wcid(Some(corrected)),
        divergence: "V313",
        evidence: PROJECTILE_STATE,
    }
}

/// Nether Streak's projectile weenie (the dump's "netherstreak").
const NETHER_STREAK_WCID: u32 = 43231;
/// Nether Arc's projectile weenie (the dump's "netherarc").
const NETHER_ARC_WCID: u32 = 43232;

/// Every spell correction, in spell id order.
///
/// The Nether Streak and Nether Arc spells launch each other's projectile on retail. Retail's
/// 43231 flew with Gravity, like every other arc projectile, in 39,289 CreateObjects over 423
/// sessions; its 43232 flew without, like every other streak, in 2,023. The creatures that cast
/// these spells (Panumbris Shadows, Void Lords, Shadow Flyers, Spectral Voidmages, maggots: 45
/// weenies in the dump) all cast Nether Arc; two cast Nether Streak. So the common,
/// falling projectile is the arc's.
pub const SPELL_CORRECTIONS: &[SpellCorrection] = &[
    projectile(5332, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Bael'zharon's Nether Streak
    projectile(5333, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Bael'zharon's Nether Arc
    projectile(5345, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak V
    projectile(5346, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak VI
    projectile(5347, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak VII
    projectile(5348, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Incantation of Nether Streak
    projectile(5357, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak I
    projectile(5358, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak II
    projectile(5359, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak III
    projectile(5360, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Nether Streak IV
    projectile(5362, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc II
    projectile(5363, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc III
    projectile(5364, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc IV
    projectile(5365, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc V
    projectile(5366, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc VI
    projectile(5367, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc VII
    projectile(5368, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Incantation of Nether Arc
    projectile(5369, NETHER_ARC_WCID, NETHER_STREAK_WCID), // Nether Arc I
    projectile(5370, NETHER_STREAK_WCID, NETHER_ARC_WCID), // Incantation of Nether Streak
];

fn spell_holds(s: &Spell, v: SpellValue) -> bool {
    match v {
        SpellValue::Wcid(want) => s.wcid == want,
    }
}

fn spell_set(s: &mut Spell, v: SpellValue) {
    match v {
        SpellValue::Wcid(value) => s.wcid = value,
    }
}

fn spell_corrections_of(spell_id: u32) -> impl Iterator<Item = &'static SpellCorrection> {
    SPELL_CORRECTIONS
        .iter()
        .filter(move |c| c.spell_id == spell_id)
}

/// The eras whose content the weenie and spell entries apply to: every entry's evidence is the
/// end-of-retail world.
pub const ENTRY_ERAS: &[EraId] = &[EraId::Eor];

/// Whether the entries apply to content built for `era`.
#[must_use]
pub fn entries_apply_to(era: EraId) -> bool {
    ENTRY_ERAS.contains(&era)
}

/// [`apply_spell`] for content built for `era`: nothing applies outside [`ENTRY_ERAS`].
pub fn apply_spell_for(s: &mut Spell, era: EraId) -> usize {
    if entries_apply_to(era) {
        apply_spell(s)
    } else {
        0
    }
}

/// Applies the corrections for `s` whose stored value it still holds; returns how many applied.
pub fn apply_spell(s: &mut Spell) -> usize {
    let mut applied = 0;
    for c in spell_corrections_of(s.id) {
        if spell_holds(s, c.stored) {
            spell_set(s, c.corrected);
            applied += 1;
        }
    }
    applied
}

/// The corrections for `s` (as stored, before [`apply_spell`]) that no longer match it.
#[must_use]
pub fn stale_spell_corrections(s: &Spell) -> Vec<&'static SpellCorrection> {
    spell_corrections_of(s.id)
        .filter(|c| !spell_holds(s, c.stored))
        .collect()
}

fn int_row(w: &Weenie, p: PropertyInt) -> Option<i32> {
    w.weenie_properties_int
        .iter()
        .find(|r| r.r#type == p.0)
        .map(|r| r.value)
}

fn bool_row(w: &Weenie, p: PropertyBool) -> Option<bool> {
    w.weenie_properties_bool
        .iter()
        .find(|r| r.r#type == p.0)
        .map(|r| r.value)
}

fn did_row(w: &Weenie, p: PropertyDataId) -> Option<u32> {
    w.weenie_properties_did
        .iter()
        .find(|r| r.r#type == p.0)
        .map(|r| r.value)
}

fn emote_action(w: &Weenie, a: EmoteAction) -> Option<&WeeniePropertiesEmoteAction> {
    let set = w
        .weenie_properties_emote
        .iter()
        .find(|e| e.id == a.emote_set)?;
    set.weenie_properties_emote_action
        .iter()
        .find(|r| r.order == a.order)
}

fn emote_action_mut(w: &mut Weenie, a: EmoteAction) -> Option<&mut WeeniePropertiesEmoteAction> {
    let set = w
        .weenie_properties_emote
        .iter_mut()
        .find(|e| e.id == a.emote_set)?;
    set.weenie_properties_emote_action
        .iter_mut()
        .find(|r| r.order == a.order)
}

fn holds(w: &Weenie, v: Value) -> bool {
    match v {
        Value::Int(p, want) => int_row(w, p) == want,
        Value::Bool(p, want) => bool_row(w, p) == want,
        Value::DataId(p, want) => did_row(w, p) == want,
        Value::EmoteScript(a, want) => emote_action(w, a).is_some_and(|r| r.p_script == want),
    }
}

fn set(w: &mut Weenie, v: Value) {
    let object_id = w.class_id;
    match v {
        Value::Int(p, value) => {
            let rows = &mut w.weenie_properties_int;
            match (rows.iter_mut().find(|r| r.r#type == p.0), value) {
                (Some(r), Some(value)) => r.value = value,
                (None, Some(value)) => rows.push(WeeniePropertiesInt {
                    id: 0,
                    object_id,
                    r#type: p.0,
                    value,
                }),
                (_, None) => rows.retain(|r| r.r#type != p.0),
            }
            rows.sort_by_key(|r| r.r#type);
        }
        Value::Bool(p, value) => {
            let rows = &mut w.weenie_properties_bool;
            match (rows.iter_mut().find(|r| r.r#type == p.0), value) {
                (Some(r), Some(value)) => r.value = value,
                (None, Some(value)) => rows.push(WeeniePropertiesBool {
                    id: 0,
                    object_id,
                    r#type: p.0,
                    value,
                }),
                (_, None) => rows.retain(|r| r.r#type != p.0),
            }
            rows.sort_by_key(|r| r.r#type);
        }
        Value::DataId(p, value) => {
            let rows = &mut w.weenie_properties_did;
            match (rows.iter_mut().find(|r| r.r#type == p.0), value) {
                (Some(r), Some(value)) => r.value = value,
                (None, Some(value)) => rows.push(WeeniePropertiesDID {
                    id: 0,
                    object_id,
                    r#type: p.0,
                    value,
                }),
                (_, None) => rows.retain(|r| r.r#type != p.0),
            }
            rows.sort_by_key(|r| r.r#type);
        }
        Value::EmoteScript(a, value) => {
            if let Some(r) = emote_action_mut(w, a) {
                r.p_script = value;
            }
        }
    }
}

fn of(weenie_class_id: u32) -> impl Iterator<Item = &'static WeenieCorrection> {
    WEENIE_CORRECTIONS
        .iter()
        .filter(move |c| c.weenie_class_id == weenie_class_id)
}

/// Applies the corrections for `w` whose stored value it still holds, then the
/// [`play_script_shift`] and [`emote_motion_shift`] rules; returns how many values changed.
pub fn apply(w: &mut Weenie) -> usize {
    apply_for(w, EraId::Eor)
}

/// [`apply`] for content built for `era`: the entries only within [`ENTRY_ERAS`], the rules
/// always.
pub fn apply_for(w: &mut Weenie, era: EraId) -> usize {
    let mut applied = 0;
    if entries_apply_to(era) {
        for c in of(w.class_id) {
            if holds(w, c.stored) {
                set(w, c.corrected);
                applied += 1;
            }
        }
    }
    if let Some(corrected) = play_script_shift(w) {
        set(
            w,
            Value::DataId(PropertyDataId::PhysicsScript, Some(corrected)),
        );
        applied += 1;
    }
    applied + apply_emote_motion_shift(w)
}

/// Where the rule's evidence is.
pub const PLAY_SCRIPT_SHIFT_EVIDENCE: &str = "retail captures: physics-script-did30-diffs";

/// The rule's DIVERGENCES row.
pub const PLAY_SCRIPT_SHIFT_DIVERGENCE: &str = "V337";

/// The physics script table of the axes, bows, chests and orbs the rule's third case covers.
const WEAPON_SCRIPT_TABLE: u32 = 0x3400_002B;

/// The rule for the default script (`PhysicsScript`, data id 30) that ACE's world data stores one
/// below retail: the value `w` should read with, or `None` when the rule leaves it alone.
///
/// The old server data ACE's world data descends from numbers default scripts from `PortalExit`
/// (83) upward one lower than retail. ACE's importer can add one to a stored value in 83..=89, but
/// only when asked to, and 100 weenies in its data that retail was seen creating still hold the
/// old value, while 443 others hold the corrected one. The value alone cannot separate the two
/// groups (84 to 88 are valid in both), so the rule corrects only what the evidence isolates:
///
/// * 83 becomes 84: retail never sent 83 as a default script, and on both weenies that store it
///   83 is not a key of the object's script table while 84 is (Flaming Club, Mucky Moarsman).
/// * 89 becomes 90 on a projectile spell: retail never sent 89 either, and every weenie storing
///   it is a projectile spell. The condition keeps the rule to what was observed: 89 (`Destroy`)
///   is a defined key of other script tables, the weapons' among them, so on any other kind of
///   object it is not known to be the old numbering. It changes nothing on ACE's data today.
/// * 87 becomes 88 on the weapons' script table (data id 22 = 0x3400002B): that table has no 87,
///   and every one of the 84 wcids on it that retail created carried 88. On the monster tables 87
///   (`BreatheLightning`) is right and retail sent it, so the table decides.
///
/// On every ACE weenie with a default script that retail created, the rule corrects all 100 that
/// differ by the shift and changes none that retail matched. The spell projectiles' default script
/// is also cleared or replaced when they are launched, so the second case is mostly inert on the
/// wire; the first and third are what the client sees. The evidence is the
/// retail captures' physics-script finding ([`PLAY_SCRIPT_SHIFT_EVIDENCE`]), the record is
/// DIVERGENCES V337. Like an entry, the rule reads what the row holds when it is read:
/// an overlay edit to another value is left alone.
#[must_use]
pub fn play_script_shift(w: &Weenie) -> Option<u32> {
    let script = did_row(w, PropertyDataId::PhysicsScript)?;
    PLAY_SCRIPT_SHIFT_CASES
        .iter()
        .find(|c| c.stored.0 == script && c.when.holds(w))
        .map(|c| c.corrected.0)
}

/// Where a [`ScriptShiftCase`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptShiftWhen {
    /// On any weenie.
    Always,
    /// On a weenie of this type.
    WeenieType(WeenieType),
    /// On a weenie whose physics script table (data id 22) is this one.
    ScriptTable(u32),
}

impl ScriptShiftWhen {
    fn holds(self, w: &Weenie) -> bool {
        match self {
            Self::Always => true,
            Self::WeenieType(t) => w.r#type == weenie_type(t),
            Self::ScriptTable(table) => {
                did_row(w, PropertyDataId::PhysicsEffectTable) == Some(table)
            }
        }
    }
}

/// One case of the [`play_script_shift`] rule: a stored default script, the value it reads as,
/// and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptShiftCase {
    pub stored: PlayScript,
    pub corrected: PlayScript,
    pub when: ScriptShiftWhen,
}

/// The [`play_script_shift`] rule's cases; the first that matches decides.
pub const PLAY_SCRIPT_SHIFT_CASES: &[ScriptShiftCase] = &[
    ScriptShiftCase {
        stored: PlayScript::PortalExit,
        corrected: PlayScript::BreatheFlame,
        when: ScriptShiftWhen::Always,
    },
    ScriptShiftCase {
        stored: PlayScript::Destroy,
        corrected: PlayScript::ProjectileCollision,
        when: ScriptShiftWhen::WeenieType(WeenieType::ProjectileSpell),
    },
    ScriptShiftCase {
        stored: PlayScript::BreatheLightning,
        corrected: PlayScript::Create,
        when: ScriptShiftWhen::ScriptTable(WEAPON_SCRIPT_TABLE),
    },
];

#[allow(clippy::cast_possible_wrap)]
const fn weenie_type(t: WeenieType) -> i32 {
    t.0 as i32
}

/// One default script the [`play_script_shift`] rule changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayScriptShift {
    pub weenie_class_id: u32,
    /// The value the content stores.
    pub stored: u32,
    /// The value the server reads.
    pub corrected: u32,
}

/// Every weenie of `db` (as stored, overlay included) whose default script the
/// [`play_script_shift`] rule changes, in class id order.
#[must_use]
pub fn play_script_shifts(db: &WorldDatabaseBase) -> Vec<PlayScriptShift> {
    db.all::<Weenie>(TableId::WEENIE)
        .iter()
        .filter_map(|w| {
            let corrected = play_script_shift(w)?;
            let stored = did_row(w, PropertyDataId::PhysicsScript)?;
            Some(PlayScriptShift {
                weenie_class_id: w.class_id,
                stored,
                corrected,
            })
        })
        .collect()
}

/// Where the emote motion rule's evidence is.
pub const EMOTE_MOTION_SHIFT_EVIDENCE: &str = ENUM_SHIFT_OTHER_FIELDS;

/// The emote motion rule's DIVERGENCES row.
pub const EMOTE_MOTION_SHIFT_DIVERGENCE: &str = "V338";

fn is_motion_command(value: u32) -> bool {
    MotionCommand::ALL.iter().any(|c| c.0 == value)
}

/// The rule for an emote's motion command that ACE's world data stores three below retail: the
/// value a stored `value` should read with, or `None` when the rule leaves it alone. It covers an
/// emote action's `Motion` (whatever the action's type) and an emote set's `Style` and `Substyle`.
///
/// The old server data numbers the motion commands from index 0x115 upward three lower than
/// retail, which has no command at 0x115..=0x117 in any class. ACE's importer adds three to those
/// values only when asked to, and a few emote rows in its data were never corrected. The rule: a
/// value that is not a `MotionCommand`, when value + 3 is, becomes value + 3. Every other value,
/// every defined command among them, is left alone.
///
/// In ACE's emote data (50,843 motion, style and substyle values), exactly 4 values name no
/// command, and on all 4 retail sent value + 3 and never the stored value: the HeartBeat motion of
/// `guarddeepplaces` (25682, 0x13000116 → 0x13000119 WarmHands), the Vendor motion of Gilly
/// (24588, 0x43000117 → 0x4300011A CurtseyState), and the HeartBeat motion of
/// `tumerokleaderpeace-xp` (10980) with the Substyle of the heartbeat set that runs in that state
/// (0x43000119 → 0x4300011C MeditateState, which keeps the pair together). Every value retail
/// matched is a defined command, so the rule changes none of them. A stored value that happens to
/// name another command is beyond the rule; none was seen among the observed rows. The evidence
/// is the retail captures' enum-shift finding ([`EMOTE_MOTION_SHIFT_EVIDENCE`]), the record is
/// DIVERGENCES V338. Like an entry, the rule reads what the row holds when it is read.
#[must_use]
pub fn emote_motion_shift(value: u32) -> Option<u32> {
    let corrected = value.checked_add(3)?;
    (!is_motion_command(value) && is_motion_command(corrected)).then_some(corrected)
}

/// Which motion field of an emote set a [`EmoteMotionShift`] changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmoteMotionField {
    Style,
    Substyle,
    /// The `Motion` of the action with this order.
    Motion(u32),
}

/// One emote motion value the [`emote_motion_shift`] rule changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmoteMotionShift {
    pub weenie_class_id: u32,
    /// The emote set's row id.
    pub emote_set: u32,
    pub field: EmoteMotionField,
    /// The value the content stores.
    pub stored: u32,
    /// The value the server reads.
    pub corrected: u32,
}

/// The emote motion values of `w` that the [`emote_motion_shift`] rule changes, in table order.
#[must_use]
pub fn emote_motion_shifts_of(w: &Weenie) -> Vec<EmoteMotionShift> {
    let mut out = Vec::new();
    for e in &w.weenie_properties_emote {
        let fields = [
            (EmoteMotionField::Style, e.style),
            (EmoteMotionField::Substyle, e.substyle),
        ]
        .into_iter()
        .chain(
            e.weenie_properties_emote_action
                .iter()
                .map(|a| (EmoteMotionField::Motion(a.order), a.motion)),
        );
        for (field, value) in fields {
            if let Some(stored) = value {
                if let Some(corrected) = emote_motion_shift(stored) {
                    out.push(EmoteMotionShift {
                        weenie_class_id: w.class_id,
                        emote_set: e.id,
                        field,
                        stored,
                        corrected,
                    });
                }
            }
        }
    }
    out
}

fn apply_emote_motion_shift(w: &mut Weenie) -> usize {
    let mut applied = 0;
    let mut shift = |value: &mut Option<u32>| {
        if let Some(corrected) = value.and_then(emote_motion_shift) {
            *value = Some(corrected);
            applied += 1;
        }
    };
    for e in &mut w.weenie_properties_emote {
        shift(&mut e.style);
        shift(&mut e.substyle);
        for a in &mut e.weenie_properties_emote_action {
            shift(&mut a.motion);
        }
    }
    applied
}

/// Every emote motion value of `db` (as stored, overlay included) that the
/// [`emote_motion_shift`] rule changes, in class id order.
#[must_use]
pub fn emote_motion_shifts(db: &WorldDatabaseBase) -> Vec<EmoteMotionShift> {
    db.all::<Weenie>(TableId::WEENIE)
        .iter()
        .flat_map(emote_motion_shifts_of)
        .collect()
}

/// The corrections for `w` (as stored, before [`apply`]) that no longer match what it stores.
#[must_use]
pub fn stale_corrections(w: &Weenie) -> Vec<&'static WeenieCorrection> {
    of(w.class_id).filter(|c| !holds(w, c.stored)).collect()
}

/// The value `w` holds for the property `v` names, in `v`'s form (`None` inside it is "no row");
/// `None` for an emote action the weenie does not have.
#[must_use]
pub fn current(w: &Weenie, v: Value) -> Option<Value> {
    Some(match v {
        Value::Int(p, _) => Value::Int(p, int_row(w, p)),
        Value::Bool(p, _) => Value::Bool(p, bool_row(w, p)),
        Value::DataId(p, _) => Value::DataId(p, did_row(w, p)),
        Value::EmoteScript(a, _) => Value::EmoteScript(a, emote_action(w, a)?.p_script),
    })
}

/// The first line of the canonical listing [`digest`] hashes; its version changes when the
/// listing's layout does.
pub const DIGEST_FORMAT: &str = "empyrean corrections v2";

fn opt_text<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "none".to_owned(), |v| v.to_string())
}

fn canonical_value(v: Value) -> String {
    match v {
        Value::Int(p, x) => format!("int {} {}", p.0, opt_text(x)),
        Value::Bool(p, x) => format!("bool {} {}", p.0, opt_text(x)),
        Value::DataId(p, x) => format!("did {} {}", p.0, opt_text(x)),
        Value::EmoteScript(a, x) => {
            format!("emote_script {} {} {}", a.emote_set, a.order, opt_text(x))
        }
    }
}

fn canonical_spell_value(v: SpellValue) -> String {
    match v {
        SpellValue::Wcid(x) => format!("wcid {}", opt_text(x)),
    }
}

/// Every stored value the [`emote_motion_shift`] rule reads as another: `(stored, corrected)`,
/// ascending. The rule is exactly this mapping.
#[must_use]
pub fn emote_motion_shift_mapping() -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = MotionCommand::ALL
        .iter()
        .filter_map(|c| {
            let stored = c.0.checked_sub(3)?;
            emote_motion_shift(stored).map(|corrected| (stored, corrected))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The canonical listing of every correction this build applies: [`DIGEST_FORMAT`], the eras the
/// entries apply to, then one line per weenie and spell entry (with its DIVERGENCES row and
/// evidence), then each rule with its full definition (the default-script cases; the emote motion
/// mapping).
#[must_use]
pub fn canonical_listing() -> String {
    listing_of(
        WEENIE_CORRECTIONS,
        SPELL_CORRECTIONS,
        PLAY_SCRIPT_SHIFT_CASES,
        &emote_motion_shift_mapping(),
    )
}

/// [`canonical_listing`] over the given entries, cases and mapping.
#[must_use]
pub fn listing_of(
    weenies: &[WeenieCorrection],
    spells: &[SpellCorrection],
    script_cases: &[ScriptShiftCase],
    motion_mapping: &[(u32, u32)],
) -> String {
    let mut s = format!("{DIGEST_FORMAT}\n");
    let eras: Vec<&str> = ENTRY_ERAS.iter().map(|e| e.name()).collect();
    s += &format!("entries for eras {}\n", eras.join(" "));
    for c in weenies {
        s += &format!(
            "weenie {} {} -> {} {} {}\n",
            c.weenie_class_id,
            canonical_value(c.stored),
            canonical_value(c.corrected),
            c.divergence,
            c.evidence
        );
    }
    for c in spells {
        s += &format!(
            "spell {} {} -> {} {} {}\n",
            c.spell_id,
            canonical_spell_value(c.stored),
            canonical_spell_value(c.corrected),
            c.divergence,
            c.evidence
        );
    }
    s += &format!(
        "rule play_script_shift {PLAY_SCRIPT_SHIFT_DIVERGENCE} {PLAY_SCRIPT_SHIFT_EVIDENCE}\n"
    );
    for c in script_cases {
        let when = match c.when {
            ScriptShiftWhen::Always => "always".to_owned(),
            ScriptShiftWhen::WeenieType(t) => format!("weenie_type {}", t.0),
            ScriptShiftWhen::ScriptTable(table) => format!("script_table {table}"),
        };
        s += &format!("  {} -> {} when {when}\n", c.stored.0, c.corrected.0);
    }
    s += &format!(
        "rule emote_motion_shift {EMOTE_MOTION_SHIFT_DIVERGENCE} {EMOTE_MOTION_SHIFT_EVIDENCE}\n"
    );
    for (stored, corrected) in motion_mapping {
        s += &format!("  {stored} -> {corrected}\n");
    }
    s
}

/// The digest of a listing: `v1:` and the first 16 hex digits of its BLAKE3 hash.
#[must_use]
pub fn digest_of(listing: &str) -> String {
    let hash = blake3::hash(listing.as_bytes());
    format!("v1:{}", &hash.to_hex()[..16])
}

/// This build's corrections digest ([`digest_of`] the [`canonical_listing`]). `world.pack`'s
/// content hash covers the data as stored, not the corrections applied as it is read, so the two
/// together name the world the game sees.
#[must_use]
pub fn digest() -> &'static str {
    static DIGEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIGEST.get_or_init(|| digest_of(&canonical_listing()))
}

/// One line naming the world the game sees: the pack's content hash (`none` without a pack) and
/// this build's corrections [`digest`].
#[must_use]
pub fn world_data_line(content_hash: Option<&str>) -> String {
    format!(
        "World data: content hash {}, corrections {}\n",
        content_hash.unwrap_or("none"),
        digest()
    )
}

#[cfg(test)]
mod tests {
    use empyrean_entity::enums::WeenieType;

    use super::*;

    #[test]
    fn a_correction_applies_only_over_the_stored_value() {
        let stored = Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
            .with_int(PropertyInt::PhysicsState, AFTER_IMPACT);
        let mut w = stored.clone();
        assert_eq!(apply(&mut w), 1);
        assert_eq!(
            int_row(&w, PropertyInt::PhysicsState),
            Some(RING_PIECE_IN_FLIGHT)
        );
        assert!(stale_corrections(&stored).is_empty());

        let edited = Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
            .with_int(PropertyInt::PhysicsState, 0x408);
        let mut w = edited.clone();
        assert_eq!(apply(&mut w), 0, "an edited value is left alone");
        assert_eq!(w, edited);
        assert_eq!(stale_corrections(&edited).len(), 1);
    }

    /// Divergence: V388
    #[test]
    fn the_entries_apply_only_to_end_of_retail_content_and_the_rules_to_every_era() {
        let ring = || {
            Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
                .with_int(PropertyInt::PhysicsState, AFTER_IMPACT)
        };
        let shifted = || {
            Weenie::new(9000, "shifted", WeenieType::Generic)
                .with_did(PropertyDataId::PhysicsScript, 83)
        };
        let nether = || Spell {
            id: 5332,
            wcid: Some(NETHER_STREAK_WCID),
            ..Spell::default()
        };
        for (era, entries_apply) in [(EraId::Eor, true), (EraId::Infiltration, false)] {
            let content = crate::MemContent::new()
                .era(era)
                .weenie(ring())
                .weenie(shifted())
                .spell(nether());
            let db = content.db();
            let w = db.base().get_weenie(33862).expect("weenie");
            let expected = if entries_apply {
                RING_PIECE_IN_FLIGHT
            } else {
                AFTER_IMPACT
            };
            assert_eq!(
                int_row(&w, PropertyInt::PhysicsState),
                Some(expected),
                "{era}"
            );
            let w = db.base().get_weenie(9000).expect("weenie");
            assert_eq!(
                did_row(&w, PropertyDataId::PhysicsScript),
                Some(84),
                "{era}: the rule applies"
            );
            let s = db.get_cached_spell(5332).expect("spell");
            let expected = if entries_apply {
                NETHER_ARC_WCID
            } else {
                NETHER_STREAK_WCID
            };
            assert_eq!(s.wcid, Some(expected), "{era}");

            let report = report::CorrectionsReport::of(db.base());
            let s = report.summary();
            assert_eq!(report.era, era);
            if entries_apply {
                assert_eq!(s.other_era, 0);
                assert!(
                    s.stale + s.absent > 0,
                    "a sparse eor content lacks most entries"
                );
            } else {
                assert_eq!(
                    (s.other_era, s.applies, s.stale, s.absent),
                    (s.entries, 0, 0, 0)
                );
                assert!(report.render().contains("no entry is for this era"));
            }
            assert_eq!(s.play_script_shifts, 1, "{era}");
        }
    }

    #[test]
    fn a_bool_correction_replaces_the_row() {
        let mut w = Weenie::new(33498, "darkvortex", WeenieType::ProjectileSpell)
            .with_bool(PropertyBool::GravityStatus, false)
            .with_bool(PropertyBool::LightsStatus, true);
        assert_eq!(apply(&mut w), 1);
        assert_eq!(bool_row(&w, PropertyBool::LightsStatus), Some(false));
        assert_eq!(bool_row(&w, PropertyBool::GravityStatus), Some(false));
        assert_eq!(w.weenie_properties_bool.len(), 2);
    }

    #[test]
    fn a_spell_correction_swaps_the_nether_projectiles() {
        let mut arc = Spell {
            id: 5367,
            wcid: Some(NETHER_ARC_WCID),
            ..Spell::default()
        };
        assert_eq!(apply_spell(&mut arc), 1);
        assert_eq!(arc.wcid, Some(NETHER_STREAK_WCID));
        assert_eq!(apply_spell(&mut arc), 0, "applied once");
        assert_eq!(stale_spell_corrections(&arc).len(), 1);
        assert!(SPELL_CORRECTIONS
            .windows(2)
            .all(|p| p[0].spell_id < p[1].spell_id));
    }

    #[test]
    fn the_server_reads_corrected_weenies_and_spells() {
        use crate::{MemContent, WorldDatabase};

        let db = MemContent::new()
            .weenie(
                Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
                    .with_int(PropertyInt::PhysicsState, AFTER_IMPACT),
            )
            .spell(Spell {
                id: 5357,
                wcid: Some(NETHER_STREAK_WCID),
                ..Spell::default()
            });
        let cached = db.get_cached_weenie(33862).expect("the weenie");
        let state = cached
            .properties_int
            .as_ref()
            .and_then(|d| d.get(&PropertyInt::PhysicsState).copied());
        assert_eq!(state, Some(RING_PIECE_IN_FLIGHT));
        assert_eq!(
            db.get_all_weenies()[0].weenie_properties_int[0].value,
            RING_PIECE_IN_FLIGHT
        );
        assert_eq!(
            db.get_cached_spell(5357).and_then(|s| s.wcid),
            Some(NETHER_ARC_WCID)
        );
    }

    #[test]
    fn the_brewmasters_pieces_and_two_neighbours_read_with_retails_item_type() {
        use empyrean_entity::enums::ItemType;

        use crate::{MemContent, WorldDatabase};

        let cases = [
            (29065, MISC, ItemType::CraftCookingBase),
            (29204, MISC, ItemType::CraftCookingBase),
            (29205, UNNAMED_ITEM_TYPE, ItemType::Misc),
            (29206, UNNAMED_ITEM_TYPE, ItemType::Misc),
            (29207, UNNAMED_ITEM_TYPE, ItemType::Misc),
            (29208, UNNAMED_ITEM_TYPE, ItemType::CraftCookingBase),
            (29209, UNNAMED_ITEM_TYPE, ItemType::CraftCookingBase),
            (29210, UNNAMED_ITEM_TYPE, ItemType::CraftCookingBase),
        ];
        let mut db = MemContent::new();
        for (wcid, stored, _) in cases {
            db = db.weenie(
                Weenie::new(wcid, "piece", WeenieType::Generic)
                    .with_int(PropertyInt::ItemType, stored),
            );
        }
        for (wcid, stored, retail) in cases {
            let as_stored = Weenie::new(wcid, "piece", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, stored);
            assert!(stale_corrections(&as_stored).is_empty(), "{wcid}");
            let cached = db.get_cached_weenie(wcid).expect("the weenie");
            let item_type = cached
                .properties_int
                .as_ref()
                .and_then(|d| d.get(&PropertyInt::ItemType).copied());
            #[allow(clippy::cast_possible_wrap)]
            let retail = retail.0 as i32;
            assert_eq!(item_type, Some(retail), "{wcid}");
        }
        // A piece an overlay already retyped is left alone.
        let edited = Weenie::new(29205, "piece", WeenieType::Generic)
            .with_int(PropertyInt::ItemType, 0x2000);
        let mut w = edited.clone();
        assert_eq!(apply(&mut w), 0);
        assert_eq!(w, edited);
    }

    #[test]
    fn nalicana_reads_with_retails_icon() {
        use crate::{MemContent, WorldDatabase};

        let stored = Weenie::new(43398, "nalicana", WeenieType::Creature);
        assert!(stale_corrections(&stored).is_empty());
        let db = MemContent::new().weenie(stored);
        let cached = db.get_cached_weenie(43398).expect("the weenie");
        let icon = cached
            .properties_did
            .as_ref()
            .and_then(|d| d.get(&PropertyDataId::Icon).copied());
        assert_eq!(icon, Some(0x0600_6E2E));

        let with_icon = Weenie::new(43398, "nalicana", WeenieType::Creature)
            .with_did(PropertyDataId::Icon, 0x0600_1036);
        let mut w = with_icon.clone();
        assert_eq!(
            apply(&mut w),
            0,
            "an icon the row already has is left alone"
        );
        assert_eq!(w, with_icon);
    }

    /// The finding's cases: (wcid, type, data id 22, stored data id 30, the value the server reads).
    const PLAY_SCRIPT_CASES: [(u32, WeenieType, u32, u32, u32); 9] = [
        (301, WeenieType::MeleeWeapon, WEAPON_SCRIPT_TABLE, 87, 88), // Battle Axe
        (24557, WeenieType::MeleeWeapon, WEAPON_SCRIPT_TABLE, 87, 88), // Quadruple-bladed Axe
        (1499, WeenieType::ProjectileSpell, 0x3400_0005, 89, 90),    // Flame Bolt
        (3768, WeenieType::MeleeWeapon, 0x3400_0039, 83, 84),        // Flaming Club
        (15871, WeenieType::MeleeWeapon, WEAPON_SCRIPT_TABLE, 88, 88), // Bronze Battle Axe: already 88
        (49119, WeenieType::CombatPet, 0x3400_00B7, 87, 87),           // Moar: 87 on another table
        (7982, WeenieType::Creature, 0x3400_0084, 87, 87),             // Destroyer Grievver
        (1605, WeenieType::Creature, 0x3400_0016, 84, 84),
        (33527, WeenieType::ProjectileSpell, 0x3400_0005, 90, 90),
    ];

    fn scripted(wcid: u32, t: WeenieType, table: u32, script: u32) -> Weenie {
        Weenie::new(wcid, "scripted", t)
            .with_did(PropertyDataId::PhysicsEffectTable, table)
            .with_did(PropertyDataId::PhysicsScript, script)
    }

    #[test]
    fn the_play_script_shift_rule_reads_retails_default_script() {
        use crate::{MemContent, WorldDatabase};

        let mut db = MemContent::new();
        for (wcid, t, table, stored, _) in PLAY_SCRIPT_CASES {
            db = db.weenie(scripted(wcid, t, table, stored));
        }
        for (wcid, t, table, stored, retail) in PLAY_SCRIPT_CASES {
            let as_stored = scripted(wcid, t, table, stored);
            assert_eq!(
                play_script_shift(&as_stored),
                (stored != retail).then_some(retail),
                "{wcid}"
            );
            let cached = db.get_cached_weenie(wcid).expect("the weenie");
            let script = cached
                .properties_did
                .as_ref()
                .and_then(|d| d.get(&PropertyDataId::PhysicsScript).copied());
            assert_eq!(script, Some(retail), "{wcid}");
            let read = db.get_weenie(wcid).expect("the weenie");
            assert_eq!(
                did_row(&read, PropertyDataId::PhysicsScript),
                Some(retail),
                "{wcid}"
            );
            assert_eq!(play_script_shift(&read), None, "{wcid}: applied once");
            let kept = db.db().base().get_stored_weenie(wcid).expect("the weenie");
            assert_eq!(
                did_row(&kept, PropertyDataId::PhysicsScript),
                Some(stored),
                "{wcid}: the stored row is kept"
            );
        }
        for w in db.get_all_weenies() {
            let retail = PLAY_SCRIPT_CASES
                .iter()
                .find(|c| c.0 == w.class_id)
                .map(|c| c.4);
            assert_eq!(
                did_row(&w, PropertyDataId::PhysicsScript),
                retail,
                "{}",
                w.class_id
            );
        }
        let shifts = play_script_shifts(db.db().base());
        let mut want: Vec<PlayScriptShift> = PLAY_SCRIPT_CASES
            .iter()
            .filter(|c| c.3 != c.4)
            .map(
                |&(weenie_class_id, _, _, stored, corrected)| PlayScriptShift {
                    weenie_class_id,
                    stored,
                    corrected,
                },
            )
            .collect();
        want.sort_by_key(|c| c.weenie_class_id);
        assert_eq!(shifts, want);
    }

    #[test]
    fn the_play_script_shift_rule_keeps_to_what_was_observed() {
        // 89 (Destroy) on anything but a projectile spell, and 87 on a monster table, are left alone.
        for w in [
            scripted(9001, WeenieType::MeleeWeapon, WEAPON_SCRIPT_TABLE, 89),
            scripted(9002, WeenieType::Creature, 0x3400_0084, 89),
            scripted(9003, WeenieType::Creature, 0x3400_0084, 87),
            Weenie::new(9004, "untabled", WeenieType::MeleeWeapon)
                .with_did(PropertyDataId::PhysicsScript, 87),
            Weenie::new(9005, "unscripted", WeenieType::MeleeWeapon)
                .with_did(PropertyDataId::PhysicsEffectTable, WEAPON_SCRIPT_TABLE),
        ] {
            let mut read = w.clone();
            assert_eq!(apply(&mut read), 0, "{}", w.class_id);
            assert_eq!(read, w);
        }
        // With no table the 83 and projectile 89 cases still apply: the value decides there.
        let mut w = Weenie::new(9006, "portal", WeenieType::Generic)
            .with_did(PropertyDataId::PhysicsScript, 83);
        assert_eq!(apply(&mut w), 1);
        assert_eq!(did_row(&w, PropertyDataId::PhysicsScript), Some(84));
        let mut w = Weenie::new(9007, "bolt", WeenieType::ProjectileSpell)
            .with_did(PropertyDataId::PhysicsScript, 89);
        assert_eq!(apply(&mut w), 1);
        assert_eq!(did_row(&w, PropertyDataId::PhysicsScript), Some(90));
    }

    /// One emote action: (order, type, motion, script).
    type ActionRow = (u32, u32, Option<u32>, Option<i32>);

    /// A weenie with emote sets: (set id, category, substyle, actions).
    fn emoting(wcid: u32, sets: &[(u32, u32, Option<u32>, &[ActionRow])]) -> Weenie {
        use crate::models::world::WeeniePropertiesEmote;

        let mut w = Weenie::new(wcid, "emoting", WeenieType::Creature);
        for &(id, category, substyle, actions) in sets {
            w.weenie_properties_emote.push(WeeniePropertiesEmote {
                id,
                object_id: wcid,
                category,
                probability: 1.0,
                substyle,
                weenie_properties_emote_action: actions
                    .iter()
                    .map(
                        |&(order, r#type, motion, p_script)| WeeniePropertiesEmoteAction {
                            emote_id: id,
                            order,
                            r#type,
                            motion,
                            p_script,
                            ..WeeniePropertiesEmoteAction::default()
                        },
                    )
                    .collect(),
                ..WeeniePropertiesEmote::default()
            });
        }
        w
    }

    const HEART_BEAT: u32 = 5;
    const VENDOR: u32 = 2;
    const TAUNT: u32 = 14;
    const MOTION: u32 = 5;
    const FORCE_MOTION: u32 = 52;
    const PHYS_SCRIPT: u32 = 7;

    /// The finding's weenies, as the pack stores them: the four rows the motion rule changes and
    /// the rows it keeps (a few of 25715's QuestFailure set among them).
    fn motion_cases() -> Vec<Weenie> {
        vec![
            emoting(
                25682,
                &[(
                    49419,
                    HEART_BEAT,
                    None,
                    &[(0, MOTION, Some(0x1300_0116), None)],
                )],
            ),
            emoting(
                24588,
                &[(
                    89413,
                    VENDOR,
                    None,
                    &[
                        (0, 10, None, None),
                        (1, MOTION, Some(0x4300_0117), None),
                        (2, MOTION, Some(0x4100_0003), None),
                    ],
                )],
            ),
            emoting(
                10980,
                &[
                    (
                        72547,
                        HEART_BEAT,
                        None,
                        &[(0, MOTION, Some(0x4300_0119), None)],
                    ),
                    (
                        72548,
                        HEART_BEAT,
                        Some(0x4300_0119),
                        &[(0, MOTION, Some(0x4100_0003), None)],
                    ),
                ],
            ),
            emoting(
                25715,
                &[(
                    49450,
                    13,
                    None,
                    &[
                        (18, FORCE_MOTION, Some(0x4300_013D), None),
                        (19, MOTION, Some(0x1300_014A), None),
                    ],
                )],
            ),
            emoting(
                24578,
                &[(
                    729,
                    HEART_BEAT,
                    None,
                    &[(0, MOTION, Some(0x1300_0150), None)],
                )],
            ),
            emoting(
                8423,
                &[(
                    69877,
                    HEART_BEAT,
                    None,
                    &[(0, MOTION, Some(0x4300_011C), None)],
                )],
            ),
            emoting(
                5772,
                &[(
                    7956,
                    HEART_BEAT,
                    None,
                    &[(0, MOTION, Some(0x1300_0151), None)],
                )],
            ),
        ]
    }

    fn action(w: &Weenie, emote_set: u32, order: u32) -> &WeeniePropertiesEmoteAction {
        emote_action(w, EmoteAction { emote_set, order }).expect("the action")
    }

    fn substyle(w: &Weenie, emote_set: u32) -> Option<u32> {
        w.weenie_properties_emote
            .iter()
            .find(|e| e.id == emote_set)
            .and_then(|e| e.substyle)
    }

    #[test]
    fn the_emote_motion_rule_reads_retails_command() {
        use crate::{MemContent, WorldDatabase};

        assert_eq!(
            emote_motion_shift(0x1300_0116),
            Some(0x1300_0119),
            "WarmHands"
        );
        assert_eq!(
            emote_motion_shift(0x4300_0117),
            Some(0x4300_011A),
            "CurtseyState"
        );
        assert_eq!(
            emote_motion_shift(0x4300_0119),
            Some(0x4300_011C),
            "MeditateState"
        );
        for defined in [
            0x1300_0119,
            0x4300_011C,
            0x1300_014A,
            0x4300_013D,
            0x1300_0150,
            0x1300_0151,
            0x4100_0003,
            0,
        ] {
            assert_eq!(
                emote_motion_shift(defined),
                None,
                "{defined:#X} is a command"
            );
        }
        // Neither a command nor three below one.
        assert_eq!(emote_motion_shift(0x1300_0FFF), None);
        assert_eq!(emote_motion_shift(u32::MAX), None);

        let mut db = MemContent::new();
        for w in motion_cases() {
            db = db.weenie(w);
        }
        // (wcid, set, action order or `None` for the set's Substyle, stored, read)
        let rows: [(u32, u32, Option<u32>, u32, u32); 11] = [
            (25682, 49419, Some(0), 0x1300_0116, 0x1300_0119),
            (24588, 89413, Some(1), 0x4300_0117, 0x4300_011A),
            (24588, 89413, Some(2), 0x4100_0003, 0x4100_0003),
            (10980, 72547, Some(0), 0x4300_0119, 0x4300_011C),
            (10980, 72548, None, 0x4300_0119, 0x4300_011C),
            (10980, 72548, Some(0), 0x4100_0003, 0x4100_0003),
            (25715, 49450, Some(18), 0x4300_013D, 0x4300_013D),
            (25715, 49450, Some(19), 0x1300_014A, 0x1300_014A),
            (24578, 729, Some(0), 0x1300_0150, 0x1300_0150),
            (8423, 69877, Some(0), 0x4300_011C, 0x4300_011C),
            (5772, 7956, Some(0), 0x1300_0151, 0x1300_0151),
        ];
        let value = |w: &Weenie, set: u32, order: Option<u32>| match order {
            Some(order) => action(w, set, order).motion,
            None => substyle(w, set),
        };
        for (wcid, set, order, stored, retail) in rows {
            let read = db.get_weenie(wcid).expect("the weenie");
            assert_eq!(
                value(&read, set, order),
                Some(retail),
                "{wcid} set {set} {order:?}"
            );
            let kept = db.db().base().get_stored_weenie(wcid).expect("the weenie");
            assert_eq!(
                value(&kept, set, order),
                Some(stored),
                "{wcid}: the stored row is kept"
            );
            assert!(
                emote_motion_shifts_of(&read).is_empty(),
                "{wcid}: applied once"
            );
        }
        // The cached weenie (what the game creates from) carries the corrected commands.
        let cached = db.get_cached_weenie(10980).expect("the weenie");
        let sets = cached.properties_emote.as_ref().expect("an emote table");
        assert_eq!(
            sets[0].properties_emote_action[0].motion,
            Some(MotionCommand::MeditateState)
        );
        assert_eq!(sets[1].substyle, Some(MotionCommand::MeditateState));
        let cached = db.get_cached_weenie(25682).expect("the weenie");
        assert_eq!(
            cached.properties_emote.as_ref().unwrap()[0].properties_emote_action[0].motion,
            Some(MotionCommand::WarmHands)
        );

        let shifts = emote_motion_shifts(db.db().base());
        let shift = |weenie_class_id, emote_set, field, stored, corrected| EmoteMotionShift {
            weenie_class_id,
            emote_set,
            field,
            stored,
            corrected,
        };
        assert_eq!(
            shifts,
            [
                shift(
                    10980,
                    72547,
                    EmoteMotionField::Motion(0),
                    0x4300_0119,
                    0x4300_011C
                ),
                shift(
                    10980,
                    72548,
                    EmoteMotionField::Substyle,
                    0x4300_0119,
                    0x4300_011C
                ),
                shift(
                    24588,
                    89413,
                    EmoteMotionField::Motion(1),
                    0x4300_0117,
                    0x4300_011A
                ),
                shift(
                    25682,
                    49419,
                    EmoteMotionField::Motion(0),
                    0x1300_0116,
                    0x1300_0119
                ),
            ]
        );
    }

    #[test]
    fn the_emote_motion_rule_covers_a_style_and_counts_what_it_changes() {
        let mut w = emoting(
            9010,
            &[(
                1,
                HEART_BEAT,
                Some(0x4300_0119),
                &[
                    (0, MOTION, Some(0x1300_0116), None),
                    (1, FORCE_MOTION, Some(0x4300_0117), None),
                ],
            )],
        );
        w.weenie_properties_emote[0].style = Some(0x4300_0119);
        assert_eq!(emote_motion_shifts_of(&w).len(), 4);
        assert_eq!(apply(&mut w), 4);
        let e = &w.weenie_properties_emote[0];
        assert_eq!(
            (e.style, e.substyle),
            (Some(0x4300_011C), Some(0x4300_011C))
        );
        assert_eq!(
            (action(&w, 1, 0).motion, action(&w, 1, 1).motion),
            (Some(0x1300_0119), Some(0x4300_011A))
        );
        assert_eq!(apply(&mut w), 0, "applied once");
    }

    /// The Taunt entries' (wcid, set) pairs, as the pack stores them.
    const TAUNTS: [(u32, u32); 9] = [
        (52575, 85961),
        (52583, 33142),
        (52585, 33143),
        (52587, 33144),
        (52589, 33145),
        (52620, 68870),
        (52626, 38325),
        (52627, 85916),
        (52712, 35686),
    ];

    fn taunting(wcid: u32, set: u32, script: i32) -> Weenie {
        emoting(
            wcid,
            &[(
                set,
                TAUNT,
                None,
                &[
                    (0, PHYS_SCRIPT, None, Some(script)),
                    (1, MOTION, Some(0x4300_00FC), None),
                ],
            )],
        )
    }

    #[test]
    fn the_taunt_scripts_read_with_retails_level_up() {
        use crate::{MemContent, WorldDatabase};

        let entries: Vec<_> = WEENIE_CORRECTIONS
            .iter()
            .filter(|c| matches!(c.stored, Value::EmoteScript(..)))
            .collect();
        assert_eq!(entries.len(), 9);
        let mut db = MemContent::new();
        for (wcid, set) in TAUNTS {
            let stored = taunting(wcid, set, 161);
            assert!(stale_corrections(&stored).is_empty(), "{wcid}");
            db = db.weenie(stored);
        }
        // Retail's 138 elsewhere: a Carenzi Racer's GotoSet and a crystal's QuestFailure, and one
        // of the family retail was not seen taunting with, are left alone.
        db = db
            .weenie(emoting(
                38947,
                &[(35844, 32, None, &[(2, PHYS_SCRIPT, None, Some(138))])],
            ))
            .weenie(emoting(
                40091,
                &[(77622, 13, None, &[(1, PHYS_SCRIPT, None, Some(138))])],
            ))
            .weenie(taunting(52519, 85959, 161))
            .weenie(
                Weenie::new(9020, "restricted", WeenieType::Generic)
                    .with_did(PropertyDataId::RestrictionEffect, 152),
            );
        for (wcid, set) in TAUNTS {
            let read = db.get_weenie(wcid).expect("the weenie");
            assert_eq!(action(&read, set, 0).p_script, Some(138), "{wcid}");
            assert_eq!(
                action(&read, set, 1).motion,
                Some(0x4300_00FC),
                "{wcid}: the rest of the set is kept"
            );
            let kept = db.db().base().get_stored_weenie(wcid).expect("the weenie");
            assert_eq!(
                action(&kept, set, 0).p_script,
                Some(161),
                "{wcid}: the stored row is kept"
            );
            let cached = db.get_cached_weenie(wcid).expect("the weenie");
            let script =
                cached.properties_emote.as_ref().unwrap()[0].properties_emote_action[0].p_script;
            assert_eq!(script, Some(PlayScript::LevelUp), "{wcid}");
        }
        assert_eq!(
            action(&db.get_weenie(38947).unwrap(), 35844, 2).p_script,
            Some(138)
        );
        assert_eq!(
            action(&db.get_weenie(40091).unwrap(), 77622, 1).p_script,
            Some(138)
        );
        assert_eq!(
            action(&db.get_weenie(52519).unwrap(), 85959, 0).p_script,
            Some(161)
        );
        assert_eq!(
            did_row(
                &db.get_weenie(9020).unwrap(),
                PropertyDataId::RestrictionEffect
            ),
            Some(152)
        );
    }

    #[test]
    fn a_taunt_entry_is_stale_when_its_action_changed_or_is_gone() {
        for w in [
            taunting(52712, 35686, 138),                  // already retail's value
            taunting(52712, 1, 161),                      // another set id
            emoting(52712, &[(35686, TAUNT, None, &[])]), // no action 0
            Weenie::new(52712, "ruukranger", WeenieType::Creature), // no emote table
        ] {
            assert_eq!(stale_corrections(&w).len(), 1);
            let mut read = w.clone();
            assert_eq!(apply(&mut read), 0);
            assert_eq!(read, w);
        }
    }

    #[test]
    fn the_digest_is_stable_and_follows_every_entry_and_rule() {
        let listing = canonical_listing();
        assert!(listing.starts_with("empyrean corrections v2\nentries for eras eor\n"));
        assert_eq!(
            listing,
            canonical_listing(),
            "the same corrections list the same way"
        );
        assert_eq!(digest(), digest_of(&listing));
        let d = digest();
        assert!(
            d.len() == 19 && d.starts_with("v1:") && d[3..].bytes().all(|b| b.is_ascii_hexdigit()),
            "{d}"
        );
        let lines = listing.lines().count();
        assert_eq!(
            lines,
            2 + WEENIE_CORRECTIONS.len()
                + SPELL_CORRECTIONS.len()
                + 2
                + PLAY_SCRIPT_SHIFT_CASES.len()
                + emote_motion_shift_mapping().len()
        );

        let mapping = emote_motion_shift_mapping();
        let digest_with =
            |w: &[WeenieCorrection],
             s: &[SpellCorrection],
             c: &[ScriptShiftCase],
             m: &[(u32, u32)]| digest_of(&listing_of(w, s, c, m));
        assert_eq!(
            digest_with(
                WEENIE_CORRECTIONS,
                SPELL_CORRECTIONS,
                PLAY_SCRIPT_SHIFT_CASES,
                &mapping
            ),
            d
        );

        // Any change to an entry, its record or its evidence changes the digest.
        let mut changed = Vec::new();
        for i in 0..4 {
            let mut w = WEENIE_CORRECTIONS.to_vec();
            match i {
                0 => w[3].corrected = Value::Int(PropertyInt::ItemType, Some(MISC + 1)),
                1 => w[3].stored = Value::Int(PropertyInt::ItemType, None),
                2 => w[3].divergence = "V999",
                _ => w[3].evidence = "elsewhere.md",
            }
            changed.push(digest_with(
                &w,
                SPELL_CORRECTIONS,
                PLAY_SCRIPT_SHIFT_CASES,
                &mapping,
            ));
        }
        changed.push(digest_with(
            &WEENIE_CORRECTIONS[1..],
            SPELL_CORRECTIONS,
            PLAY_SCRIPT_SHIFT_CASES,
            &mapping,
        ));
        let mut s = SPELL_CORRECTIONS.to_vec();
        s[0].corrected = SpellValue::Wcid(None);
        changed.push(digest_with(
            WEENIE_CORRECTIONS,
            &s,
            PLAY_SCRIPT_SHIFT_CASES,
            &mapping,
        ));
        // And any change to a rule's definition.
        let mut c = PLAY_SCRIPT_SHIFT_CASES.to_vec();
        c[2].when = ScriptShiftWhen::Always;
        changed.push(digest_with(
            WEENIE_CORRECTIONS,
            SPELL_CORRECTIONS,
            &c,
            &mapping,
        ));
        changed.push(digest_with(
            WEENIE_CORRECTIONS,
            SPELL_CORRECTIONS,
            &PLAY_SCRIPT_SHIFT_CASES[..2],
            &mapping,
        ));
        changed.push(digest_with(
            WEENIE_CORRECTIONS,
            SPELL_CORRECTIONS,
            PLAY_SCRIPT_SHIFT_CASES,
            &mapping[1..],
        ));
        let mut all = changed.clone();
        all.push(d.to_owned());
        all.sort();
        all.dedup();
        assert_eq!(
            all.len(),
            changed.len() + 1,
            "every change gives its own digest: {changed:?}"
        );
    }

    #[test]
    fn the_emote_motion_mapping_is_the_rule() {
        let mapping = emote_motion_shift_mapping();
        assert!(
            mapping.contains(&(0x1300_0116, 0x1300_0119))
                && mapping.contains(&(0x4300_0117, 0x4300_011A))
        );
        for &(stored, corrected) in &mapping {
            assert_eq!(emote_motion_shift(stored), Some(corrected));
        }
        // Every value the rule changes is three below a command, so the mapping lists them all.
        for c in MotionCommand::ALL {
            if let Some(stored) = c.0.checked_sub(3) {
                assert_eq!(
                    emote_motion_shift(stored).is_some(),
                    mapping.iter().any(|m| m.0 == stored),
                    "{stored:#X}"
                );
            }
        }
    }

    #[test]
    fn corrections_name_their_record_and_come_in_class_id_order() {
        assert!(WEENIE_CORRECTIONS
            .windows(2)
            .all(|p| p[0].weenie_class_id <= p[1].weenie_class_id));
        for c in WEENIE_CORRECTIONS {
            assert!(c.divergence.starts_with('V') && !c.evidence.is_empty());
            let same_property = match (c.stored, c.corrected) {
                (Value::Int(a, _), Value::Int(b, _)) => a == b,
                (Value::Bool(a, _), Value::Bool(b, _)) => a == b,
                (Value::DataId(a, _), Value::DataId(b, _)) => a == b,
                (Value::EmoteScript(a, _), Value::EmoteScript(b, _)) => a == b,
                _ => false,
            };
            assert!(same_property && c.stored != c.corrected, "{c:?}");
        }
        assert!(
            PLAY_SCRIPT_SHIFT_DIVERGENCE.starts_with('V') && !PLAY_SCRIPT_SHIFT_EVIDENCE.is_empty()
        );
        assert!(
            EMOTE_MOTION_SHIFT_DIVERGENCE.starts_with('V')
                && !EMOTE_MOTION_SHIFT_EVIDENCE.is_empty()
        );
    }
}
