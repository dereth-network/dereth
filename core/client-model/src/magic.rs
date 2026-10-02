//! The spellbook, components, the casting state machine and the spell-economy fields.
//!
//! **The client never plays the windup or the gesture.** No animation, no mana deduction, no fizzle
//! roll, and no range check happens locally; the busy counter is the entire client-side casting
//! state.
//!
//! The enchantment registry itself lives in [`crate::enchant`].

use crate::weenie::item_type;
use crate::world::World;
use crate::{NoticeSink, Request, RequestSink};
use dereth_assets::tables::{SpellBase, SpellTable};
use dereth_primitives::ObjectId;
use dereth_protocol::combat::{MagicCastTargetedSpell, MagicCastUntargetedSpell};
use std::collections::{BTreeMap, BTreeSet};

/// Spell-index flags. These are the **client's own** names, rather than names inferred from the server
/// implementation. \[verified\]
pub mod spell_index {
    pub const RESISTABLE: u32 = 0x0_0001;
    pub const PK_SENSITIVE: u32 = 0x0_0002;
    pub const BENEFICIAL: u32 = 0x0_0004;
    pub const SELF_TARGETED: u32 = 0x0_0008;
    pub const REVERSED: u32 = 0x0_0010;
    pub const NOT_INDOOR: u32 = 0x0_0020;
    pub const NOT_OUTDOOR: u32 = 0x0_0040;
    pub const NOT_RESEARCHABLE: u32 = 0x0_0080;
    pub const PROJECTILE: u32 = 0x0_0100;
    pub const CREATURE_SPELL: u32 = 0x0_0200;
    pub const EXCLUDED_FROM_ITEM_DESCRIPTIONS: u32 = 0x0_0400;
    pub const IGNORES_MANA_CONVERSION: u32 = 0x0_0800;
    pub const NON_TRACKING_PROJECTILE: u32 = 0x0_1000;
    pub const FELLOWSHIP_SPELL: u32 = 0x0_2000;
    pub const FAST_CAST: u32 = 0x0_4000;
    pub const INDOOR_LONG_RANGE: u32 = 0x0_8000;
    pub const DAMAGE_OVER_TIME: u32 = 0x1_0000;
}

/// `SpellbookFilter`.
pub mod spellbook_filter {
    pub const UNDEF: u32 = 0;
    pub const CREATURE: u32 = 1;
    pub const ITEM: u32 = 2;
    pub const LIFE: u32 = 4;
    pub const WAR: u32 = 8;
    pub const LEVEL_1: u32 = 16;
    pub const LEVEL_2: u32 = 32;
    pub const LEVEL_3: u32 = 64;
    pub const LEVEL_4: u32 = 128;
    pub const LEVEL_5: u32 = 256;
    pub const LEVEL_6: u32 = 512;
    pub const LEVEL_7: u32 = 1024;
    pub const LEVEL_8: u32 = 2048;
    /// Honoured by the filtering predicate, but **no client path sets it**.
    pub const LEVEL_9: u32 = 4096;
    pub const VOID: u32 = 8192;
    pub const DEFAULT: u32 = 16383;
}

/// The spell-component category values.
pub mod component_category {
    pub const SCARAB: u32 = 0;
    pub const HERB: u32 = 1;
    pub const POWDERED_GEM: u32 = 2;
    pub const ALCHEMICAL_SUBSTANCE: u32 = 3;
    pub const TALISMAN: u32 = 4;
    pub const TAPER: u32 = 5;
    pub const PEA: u32 = 6;
    pub const NUM: u32 = 7;
    pub const UNDEF: u32 = 8;
}

/// The spell banks — eight spell-bar tabs.
pub const NUM_SPELLCAST_BANKS: usize = 8;

/// The lowest taper is **0x3F (63)**, *Red Taper*; there are twelve tapers, 63..74.
pub const LOWEST_TAPER_ID: u32 = 0x3F;
pub const NUM_TAPERS: u32 = 12;

/// `PortalLinkType`.
pub mod portal_link {
    pub const UNDEF: u32 = 0;
    pub const LINKED_LIFESTONE: u32 = 1;
    pub const LINKED_PORTAL_ONE: u32 = 2;
    pub const LINKED_PORTAL_TWO: u32 = 3;
}

/// `PortalRecallType`.
pub mod portal_recall {
    pub const UNDEF: u32 = 0;
    pub const LAST_LIFESTONE: u32 = 1;
    pub const LINKED_LIFESTONE: u32 = 2;
    pub const LAST_PORTAL: u32 = 3;
    pub const LINKED_PORTAL_ONE: u32 = 4;
    pub const LINKED_PORTAL_TWO: u32 = 5;
}

/// `PortalSummonType`.
pub mod portal_summon {
    pub const UNDEF: u32 = 0;
    pub const LINKED_PORTAL_ONE: u32 = 1;
    pub const LINKED_PORTAL_TWO: u32 = 2;
}

/// `ComponentTrackerUpdate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentTrackerUpdate {
    None = 0,
    Add = 1,
    Remove = 2,
}

pub use dereth_rules::magic::{
    find_most_powerful_power_component, num_spell_components, scarab_only_filler_count,
    scarab_only_formula, scarab_power_level, SCARAB_ONLY_FILLER_SCID,
};

/// Computes the 1–8 spell level the UI shows.
#[must_use]
pub fn spell_level_by_rough_heuristic(power: u32) -> u32 {
    if power <= 6 {
        power
    } else if power <= 8 {
        power - 1
    } else {
        power - 2
    }
}

/// A formula is complete only when its first **five** slots are non-zero.
///
/// Target-type lookup requires completeness before returning any type, so an incomplete formula
/// makes the spell untargetable.
#[must_use]
pub fn formula_is_complete(components: &[u32]) -> bool {
    components.len() >= 5 && components[..5].iter().all(|c| *c != 0)
}

/// Decrypts a formula by subtracting the key from every **non-zero** slot.
///
/// The spell-formula lookup applies this to `_formula._comps` with
/// `descHash % 0xBEADCF45 + nameHash % 0x12107680`, which `dereth_assets` has already computed into
/// `SpellBase::comp_key`. **Slot positions survive**, which is why the cast path needs this form:
/// `SpellBase::comps` drops the zero slots and so cannot be indexed by formula position.
#[must_use]
pub fn decrypt_formula(raw_comps: &[u32; 8], comp_key: u32) -> [u32; 8] {
    let mut out = [0u32; 8];
    for (o, r) in out.iter_mut().zip(raw_comps.iter()) {
        *o = if *r == 0 { 0 } else { r.wrapping_sub(comp_key) };
    }
    out
}

/// Computes the client's string hash over an **account name**.
///
/// It is the same PJW/ELF hash `dereth_assets` already transcribed for the spell table's component
/// key -- the client calls this one function for both, caching the answer in
/// the string buffer itself. An earlier report recorded that the hash was missing; it already existed
/// under the name
/// [`dereth_assets::tables::spell_hash`], and this wrapper names that fact rather than copying it.
///
/// The client's narrow string holds **cp1252** bytes and the hash walks them as *signed* chars, so the
/// `String` is re-encoded rather than hashed as UTF-8. Every account name this can be given came
/// off the wire through `dereth_protocol::cp1252::decode`, so the encode cannot fail; an unencodable
/// name hashes as empty, which is the client's own empty-string answer (the hash returns 0
/// for a zero-length string).
#[must_use]
pub fn account_name_hash(account: &str) -> u32 {
    let bytes = dereth_protocol::cp1252::encode(account).unwrap_or_default();
    dereth_assets::tables::spell_hash(&bytes)
}

/// Replaces a formula's **taper** slots with values derived from a hash of the player's account
/// name.
///
/// This is the last step after reading and decrypting the dat formula: select the spell's formula
/// version, then randomize its taper slots from the account-name hash. The formula-version field is
/// the switch: `1`, `2`, `3`, and **anything else leaves the formula
/// alone**. The shipped spell table has 5888
/// version-1, 163 version-2 and 215 version-3 spells and no other value.
///
/// The name comes from [`crate::player::PlayerSystem::account`].
///
/// The reference server implements the same three formulas and hashes the session account name, so
/// the client and server expect the same tapers for the same account. Its implementation indexes a
/// **dense** list; this
/// indexes the eight slots. They agree for every shipped spell because no shipped formula has an
/// interior hole, which the formula-table regression test pins.
pub fn randomize_for_name(comps: &mut [u32; 8], account: &str, formula_version: u32) {
    match formula_version {
        1 => randomize_version_1(comps, account),
        2 => randomize_version_2(comps, account),
        3 => randomize_version_3(comps, account),
        // neither 1, 2 nor 3 -- returns at once, formula untouched.
        _ => {}
    }
}

/// The divisor guard the client does not have: its `div` would raise `#DE`.
///
/// Every `div` below is reached with a divisor the client's own guards keep non-zero for every
/// shipped formula (proved over the whole table by the formula-taper test), so this arm is
/// unreachable in practice and exists only so that a corrupt dat is a wrong number rather than a
/// panic in a release client.
fn div_or_zero(a: u32, b: u32) -> u32 {
    a.checked_div(b).unwrap_or(0)
}

/// Applies version 1 of account-specific taper randomisation.
///
/// The slot layout is positional and depends on how many components the formula has: a five-slot
/// formula is `scarab, herb, powder, potion, talisman` with **no** tapers, and each extra slot
/// inserts one taper ahead of the next reagent -- at `1` (six slots), then `3` (seven), then `6`
/// (eight). Those three branches compare the component count with 5, 6, and 7 respectively.
///
/// Only slots **3** and **6** are account-derived: the `comps[1]` arm never touches
/// the seed, so a six-slot formula's taper is the same for every account -- and is already what
/// the dat stores, the table having been generated with this very function.
///
/// The `if (herb + scarab == 0) scarab = 1` is the client's, including the fact that
/// the assignment **persists** into the two later arms.
fn randomize_version_1(c: &mut [u32; 8], account: &str) {
    let n = num_spell_components(c);
    // The hash modulo 0x13D573 is the seed.
    let seed = account_name_hash(account) % 0x13_D573;
    // An index at or past 8 reads 0 rather than the array.
    let at = |c: &[u32; 8], i: usize| if i < 8 { c[i] } else { 0 };

    let mut scarab = c[0];
    let taper1 = n > 5;
    let mut i = if taper1 { 2 } else { 1 };
    let herb = at(c, i);
    i += 1;
    let taper2 = n > 6;
    if taper2 {
        i += 1;
    }
    let powder = at(c, i);
    i += 1;
    let potion = at(c, i);
    i += 1;
    let taper3 = n > 7;
    if taper3 {
        i += 1;
    }
    let talisman = at(c, i);

    if taper1 {
        if herb.wrapping_add(scarab) == 0 {
            scarab = 1;
        }
        // talisman + 2*herb + potion + powder + scarab.
        let v = talisman
            .wrapping_add(herb.wrapping_mul(2))
            .wrapping_add(potion)
            .wrapping_add(powder)
            .wrapping_add(scarab);
        c[1] = LOWEST_TAPER_ID + v % NUM_TAPERS;
    }
    if taper2 {
        let pp = powder.wrapping_add(potion);
        if scarab.wrapping_add(pp) == 0 {
            scarab = 1;
        }
        // seed / (powder + potion + scarab), times
        // talisman + 2*(powder + potion) + herb + scarab.
        let q = div_or_zero(seed, pp.wrapping_add(scarab));
        let v = talisman
            .wrapping_add(pp.wrapping_mul(2))
            .wrapping_add(herb)
            .wrapping_add(scarab);
        c[3] = LOWEST_TAPER_ID + q.wrapping_mul(v) % NUM_TAPERS;
    }
    if taper3 {
        if talisman.wrapping_add(scarab) == 0 {
            scarab = 1;
        }
        // seed / (talisman + scarab), times
        // potion + 2*talisman + powder + herb + scarab.
        let q = div_or_zero(seed, talisman.wrapping_add(scarab));
        let v = potion
            .wrapping_add(talisman.wrapping_mul(2))
            .wrapping_add(powder)
            .wrapping_add(herb)
            .wrapping_add(scarab);
        c[6] = LOWEST_TAPER_ID + q.wrapping_mul(v) % NUM_TAPERS;
    }
}

/// Applies version 2 of account-specific taper randomisation.
///
/// No slot count and no guards: it always writes slots 3 and 6, and every version-2 spell in the
/// shipped table has all eight slots filled.
fn randomize_version_2(c: &mut [u32; 8], account: &str) {
    let seed = account_name_hash(account) % 0x13_D573;
    let (s, c1, c2, c4, c5, c7) = (c[0], c[1], c[2], c[4], c[5], c[7]);
    // comps[7] + 3*comps[0] + 2*comps[4]*comps[5] + comps[2] + comps[1].
    let v3 = c7
        .wrapping_add(s.wrapping_mul(3))
        .wrapping_add(c4.wrapping_mul(c5).wrapping_mul(2))
        .wrapping_add(c2)
        .wrapping_add(c1);
    c[3] = LOWEST_TAPER_ID + v3 % NUM_TAPERS;
    // divisor comps[1]*comps[7] + 2*comps[4]; then seed / it;
    // times comps[7] + 3*comps[0]*comps[2] + 2*comps[5] + comps[4].
    let q = div_or_zero(seed, c1.wrapping_mul(c7).wrapping_add(c4.wrapping_mul(2)));
    let v6 = c7
        .wrapping_add(s.wrapping_mul(c2).wrapping_mul(3))
        .wrapping_add(c5.wrapping_mul(2))
        .wrapping_add(c4);
    c[6] = LOWEST_TAPER_ID + q.wrapping_mul(v6) % NUM_TAPERS;
}

/// Applies version 3 of account-specific taper randomisation.
///
/// Seven hash reads, all on the same string: the client re-reads the string's cached hash once
/// per seed, computing it only if it is not yet filled. There is one hash. The six seeds are six different moduli of it, and the seventh read
/// feeds `key % 0x65039 % 0xC` inside the slot-6 sum.
///
/// Slots 0, 1, 2, 4, 5 and 7 fold into six residues mod 12 and slots 3 and 6 are written from
/// them. Slot 7 of a seven-slot formula is 0, which is ACE's explicit `comps.Count < 8` arm.
fn randomize_version_3(c: &mut [u32; 8], account: &str) {
    let key = account_name_hash(account);
    let h0 = c[0].wrapping_add(key % 0x13_D573) % NUM_TAPERS;
    let h1 = c[1].wrapping_add(key % 0x4_AEFD) % NUM_TAPERS;
    let h2 = c[2].wrapping_add(key % 0x9_6A7F) % NUM_TAPERS;
    let h4 = c[4].wrapping_add(key % 0x10_0A03) % NUM_TAPERS;
    let h5 = c[5].wrapping_add(key % 0xE_B2EF) % NUM_TAPERS;
    let h7 = c[7].wrapping_add(key % 0x12_1E7D) % NUM_TAPERS;
    let h01 = h0.wrapping_mul(h1);
    let h25 = h2.wrapping_mul(h5);
    // (h4 + 1)*h7 + h0*h1 + h2*h5 + h5 + h4 + h2 + h1 + h0.
    let v3 = (h4 + 1)
        .wrapping_mul(h7)
        .wrapping_add(h01)
        .wrapping_add(h25)
        .wrapping_add(h5)
        .wrapping_add(h4)
        .wrapping_add(h2)
        .wrapping_add(h1)
        .wrapping_add(h0);
    c[3] = LOWEST_TAPER_ID + v3 % NUM_TAPERS;
    // ((h2*h5*h1*h0 + 7)*h4 + 1)*h7.
    let big = h25
        .wrapping_mul(h1)
        .wrapping_mul(h0)
        .wrapping_add(7)
        .wrapping_mul(h4)
        .wrapping_add(1)
        .wrapping_mul(h7);
    // 11*h2*h5 + 5*h0*h1 + h5 + big + key % 0x65039 % 0xC + h4 + h2 + h1 + h0.
    let v6 = h25
        .wrapping_mul(0xB)
        .wrapping_add(h01.wrapping_mul(5))
        .wrapping_add(h5)
        .wrapping_add(big)
        .wrapping_add(key % 0x6_5039 % NUM_TAPERS)
        .wrapping_add(h4)
        .wrapping_add(h2)
        .wrapping_add(h1)
        .wrapping_add(h0);
    c[6] = LOWEST_TAPER_ID + v6 % NUM_TAPERS;
}

/// The five `AugmentationInfused*Magic` int properties, by `_school`.
///
/// The formula selector's table: 1 War, 2 Life, 3 ItemEnch, 4 CreatureEnch,
/// 5 Void.
#[must_use]
pub fn infused_augmentation_for_school(school: u32) -> Option<u32> {
    Some(match school {
        1 => 0x129, // AugmentationInfusedWarMagic
        2 => 0x128, // AugmentationInfusedLifeMagic
        3 => 0x127, // AugmentationInfusedItemMagic
        4 => 0x126, // AugmentationInfusedCreatureMagic
        5 => 0x148, // AugmentationInfusedVoidMagic
        _ => return None,
    })
}

/// Which formula [`World::get_appropriate_spell_formula`] selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellFormulaKind {
    /// The full five-to-eight component formula, with the
    /// per-account taper randomisation.
    Customized,
    /// The "infused"/spell-pack shortcut.
    ScarabOnly,
}

/// Desired-component-level bound: `0 <= n < 0x1389` (0..5000). Anything outside resets to 0
/// and sends `Character_SetDesiredComponentLevel` (0x0224).
pub const MAX_DESIRED_COMP_LEVEL: i32 = 0x1389;

/// One `SpellComponentBase` row, as much of it as the tracker and the two panels read.
///
/// The whole record is name, category, icon id, type, gesture, time, text and a component-data
/// field; only the first three are read on this side.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComponentBase {
    /// The component base's name — de-obfuscated when the dat is decoded.
    pub name: String,
    /// The component base's category, a [`component_category`] value.
    pub category: u32,
    /// The component base's icon id.
    pub icon: u32,
}

/// The two dat lookups every `ComponentTracker` operation begins with, hoisted into one table the
/// host fills once.
///
/// WCID-to-SCID conversion is **not** arithmetic. It uses the dual enum map loaded as
/// `(3, 0x10000001, 0x28)`, whose forward direction is
/// SCID -> WCID, walked backwards by [`Self::wcid_to_scid`]. A miss leaves the out
/// parameter at **0** and returns nothing else, so the caller can only tell by querying the
/// component-base table next. Both steps are reproduced here: [`Self::wcid_to_scid`]
/// answers `0` on a miss and [`Self::inq_spell_component_base`] answers `None`.
///
/// The client re-acquires and releases the dat object on **every single call**, including inside
/// [`Self::determine_component_category`], which the tracker calls on every add, remove and stack update.
/// That is a performance quirk with no behavioural consequence and is not reproduced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComponentCatalogue {
    /// The reverse of the `DualDidMapper`'s `enum_to_id`: component WCID -> SCID.
    wcid_to_scid: BTreeMap<u32, u32>,
    /// The **forward** SCID-to-WCID direction. The tracker only
    /// ever asks the reverse; the cast path asks this one, because a spell formula holds SCIDs and
    /// component ownership is keyed by WCID.
    scid_to_wcid: BTreeMap<u32, u32>,
    /// The SCID -> component-base map.
    bases: BTreeMap<u32, ComponentBase>,
}

impl ComponentCatalogue {
    /// Build from the two decoded dat objects: the `DualDidMapper`'s `enum_to_id` pairs
    /// (SCID, WCID) and the `SpellComponentTable`'s SCID -> row map.
    ///
    /// The mapper is walked in reverse because the tracker only ever asks the WCID direction.
    #[must_use]
    pub fn new(
        scid_to_wcid: impl IntoIterator<Item = (u32, u32)>,
        bases: impl IntoIterator<Item = (u32, ComponentBase)>,
    ) -> Self {
        let pairs: Vec<(u32, u32)> = scid_to_wcid.into_iter().collect();
        Self {
            wcid_to_scid: pairs.iter().map(|(scid, wcid)| (*wcid, *scid)).collect(),
            scid_to_wcid: pairs.into_iter().collect(),
            bases: bases.into_iter().collect(),
        }
    }

    /// Whether the host has filled this yet. **Three states, not two**: an empty catalogue makes
    /// every category `Undef` and every name empty, which is the client's own no-dat arm and must
    /// not be confused with "no components owned".
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bases.is_empty()
    }

    /// How many WCIDs the mapper carries — the denominator a test needs before believing a zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.wcid_to_scid.len()
    }

    /// Maps a WCID to its SCID, returning **0 on a miss** as the client leaves it.
    #[must_use]
    pub fn wcid_to_scid(&self, wcid: u32) -> u32 {
        self.wcid_to_scid.get(&wcid).copied().unwrap_or(0)
    }

    /// Maps an SCID to a WCID through the same dual enum map that [`Self::wcid_to_scid`] walks
    /// backwards.
    ///
    /// A miss leaves the caller's out parameter at `INVALID_DID`; **0** is used for it here, and
    /// the choice is unobservable because component ownership is a lookup in
    /// the class-id set, which no real component class is keyed under. Either sentinel refuses.
    #[must_use]
    pub fn scid_to_wcid(&self, scid: u32) -> u32 {
        self.scid_to_wcid.get(&scid).copied().unwrap_or(0)
    }

    /// Looks up a component-base record by **SCID**.
    #[must_use]
    pub fn inq_spell_component_base(&self, scid: u32) -> Option<&ComponentBase> {
        self.bases.get(&scid)
    }

    /// Returns **the icon a component row in the spellbook's
    /// strip actually draws.**
    ///
    /// The panel loads the component table, maps the object's class id to an SCID, and queries the
    /// component base. A missing table, missing row, or `INVALID_DID` icon returns with the image
    /// untouched. Only after all three guards does it clear the image, select three-alpha blitting,
    /// and install the table's component icon.
    ///
    /// So the drawn icon is the **table's** component-base icon id, keyed by
    /// `wcid_to_scid(classID)`, and **not** the object's own `pwd.icon_id`.
    /// The stored row icon is the separate component icon id, which is read from the object
    /// and copied into
    /// the panel row's own icon field — and which **nothing in the client ever draws**. Two icon
    /// fields, one of them dead; this is the live one.
    ///
    /// `None` is each of the two `return`s above, and it means *leave the row's image alone*
    /// rather than *clear it*: the image clear is on the far side of both guards.
    #[must_use]
    pub fn component_icon(&self, wcid: u32) -> Option<u32> {
        self.inq_spell_component_base(self.wcid_to_scid(wcid))
            .map(|b| b.icon)
            .filter(|i| *i != 0)
    }

    /// Bridges a component WCID to its category.
    ///
    /// `Undef` (8) when either hop misses. Note that this is **not** used as a rejection gate:
    /// [`ComponentTracker::add_component`] indexes the category lists with whatever comes back, so a component
    /// the SCID map does not know lands in the `Undef` bucket rather than being dropped. The real
    /// gate is `ITEM_TYPE & TYPE_SPELL_COMPONENTS`, applied by the caller — see
    /// the component-update method below.
    #[must_use]
    pub fn determine_component_category(&self, wcid: u32) -> u32 {
        self.inq_spell_component_base(self.wcid_to_scid(wcid))
            .map_or(component_category::UNDEF, |b| b.category)
    }

    /// Returns the component-base name for a WCID.
    ///
    /// The empty string is the miss, matching the client's empty output buffer, which is only
    /// overwritten on a hit.
    #[must_use]
    pub fn comp_name_from_wcid(&self, wcid: u32) -> &str {
        self.inq_spell_component_base(self.wcid_to_scid(wcid))
            .map_or("", |b| b.name.as_str())
    }
}

/// One component **class**, and every object of that class the player holds. The original
/// `ComponentData` layout occupies `0x8C` bytes; this Rust type models its behavior rather than
/// that physical layout.
///
/// The client keeps `objects` as an `objectID -> stackSize` hash and `numItems` as their running
/// sum, and it is the sum — not a set membership — that [`ComponentTracker::num_component`] returns and that decides
/// when the class stops being owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentData {
    /// The component class id.
    pub class_id: u32,
    /// A copy of `pwd._name`, **not** the component-base
    /// name, because [`ComponentTracker::add_component`]'s `strcmp` compares against exactly this field.
    pub name: String,
    /// The icon id copied from the object.
    pub icon: u32,
    /// Object id -> that object's stack size, with the client's
    /// `stack_size == 0 ? 1` substitution already applied.
    objects: BTreeMap<ObjectId, u32>,
    /// The running item count across the row's objects.
    num_items: i64,
}

impl ComponentData {
    /// Constructs a component row and adds its first object.
    fn new(obj: ObjectId, wcid: u32, name: &str, icon: u32, stack: u32) -> Self {
        let mut d = Self {
            class_id: wcid,
            name: name.to_owned(),
            icon,
            objects: BTreeMap::new(),
            num_items: 0,
        };
        d.add_item(obj, stack);
        d
    }

    /// Adds an object; a stack size of 0 counts as **one**.
    fn add_item(&mut self, obj: ObjectId, stack: u32) {
        let n = if stack == 0 { 1 } else { stack };
        self.objects.insert(obj, n);
        self.num_items += i64::from(n);
    }

    /// Removes an object with the same 0 -> 1 substitution. The subtraction uses
    /// the **incoming** weenie's stack size rather than the stored one, exactly as the client does
    /// — which is why a stack whose size changed without a stack-size update leaves `numItems`
    /// wrong in retail too.
    fn remove_item(&mut self, obj: ObjectId, stack: u32) {
        self.objects.remove(&obj);
        self.num_items -= i64::from(if stack == 0 { 1 } else { stack });
    }

    /// Updates one object's stack size and the running item count.
    ///
    /// `numItems += new - old`, and the returned code is `new < old ? Remove : Add`, with `None`
    /// when they are equal. `old` is the stored stack size, or — when the object is not in the
    /// hash at all — the client's uninitialised local, which cannot be reproduced and cannot be
    /// equal to any real stack size; `0` stands in for it, so an unknown object reads as a gain.
    fn update_stack_size(&mut self, obj: ObjectId, stack: u32) -> ComponentTrackerUpdate {
        let old = self.objects.get(&obj).copied().unwrap_or(0);
        let new = if stack == 0 { 1 } else { stack };
        if new == old {
            return ComponentTrackerUpdate::None;
        }
        self.num_items += i64::from(new) - i64::from(old);
        self.objects.insert(obj, new);
        if new < old {
            ComponentTrackerUpdate::Remove
        } else {
            ComponentTrackerUpdate::Add
        }
    }

    /// Returns the running item count.
    #[must_use]
    pub fn num_items(&self) -> i64 {
        self.num_items
    }

    /// Returns the representative object a component row
    /// selects when clicked. The client takes the hash's `begin()`, which is bucket order; this
    /// takes the lowest id, which is stable and is the only ordering a rebuild can agree on.
    #[must_use]
    pub fn first_object_id(&self) -> Option<ObjectId> {
        self.objects.keys().next().copied()
    }

    /// How many distinct objects of this class the player holds.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}

/// The client's live component inventory. Its original `ComponentTracker` layout occupies
/// `0x108` bytes; this Rust type does not claim that physical layout.
///
/// **Quantity is not checked by [`Self::component_is_owned`]**: it is a plain class-id hash-set membership
/// test on the class-id set, which is exactly why the client will happily send a cast the server
/// then
/// refuses for lack of components. The *count* lives on each component row and is what
/// the component panel and vendor component list read.
///
/// **A plain set of WCIDs plus a count per WCID would be wrong in three ways.** Removing one of
/// two stacks must not un-own the class: the client only does so when `numItems` reaches 0.
/// Adding derives the count from `pwd.stack_size` with a `0 -> 1` substitution rather than
/// taking an explicit count. And the stack-size update applies a **delta** for one object and
/// can never un-own a class, rather than setting the count absolutely.
#[derive(Debug, Clone, Default)]
pub struct ComponentTracker {
    /// Object id -> component WCID, used for ownership lookup and the add/remove decision.
    object_ids: BTreeMap<ObjectId, u32>,
    /// The class-id set tested by [`Self::component_is_owned`].
    class_ids: BTreeSet<u32>,
    /// One component-data list per category, kept sorted by name. Nine buckets, not seven:
    /// category determination can answer
    /// `Undef` (8) and [`Self::add_component`] indexes the array with it unchecked.
    lists: Vec<Vec<ComponentData>>,
}

impl ComponentTracker {
    /// Constructs an empty tracker with all nine category buckets.
    #[must_use]
    pub fn new() -> Self {
        Self {
            object_ids: BTreeMap::new(),
            class_ids: BTreeSet::new(),
            lists: vec![Vec::new(); component_category::UNDEF as usize + 1],
        }
    }

    fn list_mut(&mut self, category: u32) -> &mut Vec<ComponentData> {
        if self.lists.is_empty() {
            self.lists = vec![Vec::new(); component_category::UNDEF as usize + 1];
        }
        let i = (category as usize).min(self.lists.len() - 1);
        &mut self.lists[i]
    }

    /// One category's rows, in the client's own name order — what `SpellComponentPanel` walks.
    #[must_use]
    pub fn category(&self, category: u32) -> &[ComponentData] {
        self.lists.get(category as usize).map_or(&[], Vec::as_slice)
    }

    /// Every row, category by category, `Scarab` first — the client's walk.
    ///
    /// It stops at [`component_category::NUM`] because the panel has exactly seven header rows;
    /// anything the SCID map did not know sits in the `Undef` bucket and is **not drawn**, which
    /// is the client's behaviour and not an omission here.
    pub fn categories(&self) -> impl Iterator<Item = (u32, &[ComponentData])> {
        (0..component_category::NUM).map(move |c| (c, self.category(c)))
    }

    /// Tests class ownership by WCID.
    #[must_use]
    pub fn component_is_owned(&self, wcid: u32) -> bool {
        self.class_ids.contains(&wcid)
    }

    /// Returns the component WCID associated with an owned object id.
    #[must_use]
    pub fn object_is_owned_component(&self, obj: ObjectId) -> Option<u32> {
        self.object_ids.get(&obj).copied()
    }

    /// Returns a class row's running item count, or **0** when the class has no row.
    #[must_use]
    pub fn num_component(&self, cat: &ComponentCatalogue, wcid: u32) -> i64 {
        let category = cat.determine_component_category(wcid);
        self.category(category)
            .iter()
            .find(|d| d.class_id == wcid)
            .map_or(0, ComponentData::num_items)
    }

    /// Resolves a spell-examine component row to an owned object. The row's SCID
    /// is first mapped to a WCID; a missing player tracker returns no object. The tracker then walks
    /// category lists 0 through 6 in order, stopping with no result if it encounters a null list.
    ///
    /// Three things that are easy to miss, and each of them is a rule:
    ///
    /// * **The pane is never read.** The function depends on the SCID and the player's tracker
    ///   and nothing about the pane. So it lives
    ///   here, on the tracker, rather than on the pane.
    /// * **The walk covers component lists 0..7**, i.e. [`component_category::NUM`]
    ///   categories and **not** the `Undef` bucket. A component the SCID map could not categorise
    ///   sits in `Undef` and is unreachable from a click, exactly as it is undrawn by the panel.
    /// * **The first match wins, at both levels**: the first list in category order that holds a
    ///   row whose `classID` is this WCID, and then that row's first object. The client does not
    ///   prefer the larger stack, the
    ///   lower id or the equipped one — and since one class has exactly one row per category,
    ///   the outer walk can match at most once anyway.
    ///
    /// A miss at **either** hop answers `None`, the client's zero result for "this row is not a
    /// component I am carrying".
    #[must_use]
    pub fn component_object_id(&self, cat: &ComponentCatalogue, scid: u32) -> Option<ObjectId> {
        // The client's SCID-to-WCID lookup leaves `INVALID_DID` on a miss and this leaves `0`; neither is a real
        // component class, so both refuse the walk below. See [`ComponentCatalogue::scid_to_wcid`].
        let wcid = cat.scid_to_wcid(scid);
        if wcid == 0 {
            return None;
        }
        (0..component_category::NUM)
            .find_map(|c| self.category(c).iter().find(|d| d.class_id == wcid))
            .and_then(ComponentData::first_object_id)
    }

    /// Add a component object.
    ///
    /// The category list is kept sorted by `pwd._name` and the match is a **`strcmp` on the name**,
    /// not on the WCID: an exact name hit adds the object to that row, a name that sorts
    /// before the cursor inserts a new row there, and running off the end appends. Only
    /// inserting a new row adds to the class-id set.
    pub fn add_component(
        &mut self,
        cat: &ComponentCatalogue,
        obj: ObjectId,
        wcid: u32,
        name: &str,
        icon: u32,
        stack: u32,
    ) {
        self.object_ids.insert(obj, wcid);
        let category = cat.determine_component_category(wcid);
        let list = self.list_mut(category);
        match list.iter().position(|d| d.name.as_str() >= name) {
            Some(i) if list[i].name == name => list[i].add_item(obj, stack),
            Some(i) => {
                list.insert(i, ComponentData::new(obj, wcid, name, icon, stack));
                self.class_ids.insert(wcid);
            }
            None => {
                list.push(ComponentData::new(obj, wcid, name, icon, stack));
                self.class_ids.insert(wcid);
            }
        }
    }

    /// Remove a component object.
    ///
    /// The row is found by **`classID`** here (not by name), one object's worth is taken off
    /// `numItems`, and the class stops being owned only when `numItems` reaches 0.
    pub fn remove_component(
        &mut self,
        cat: &ComponentCatalogue,
        obj: ObjectId,
        wcid: u32,
        stack: u32,
    ) {
        self.object_ids.remove(&obj);
        let category = cat.determine_component_category(wcid);
        let list = self.list_mut(category);
        let Some(i) = list.iter().position(|d| d.class_id == wcid) else {
            return;
        };
        list[i].remove_item(obj, stack);
        if list[i].num_items == 0 {
            list.remove(i);
            self.class_ids.remove(&wcid);
        }
    }

    /// Finds the row by category and `classID`, then updates that object's stack size. A class with
    /// no row answers `None` without touching
    /// anything, which is the client's `return` on a null `ComponentData`.
    pub fn update_stack_size(
        &mut self,
        cat: &ComponentCatalogue,
        obj: ObjectId,
        wcid: u32,
        stack: u32,
    ) -> ComponentTrackerUpdate {
        let category = cat.determine_component_category(wcid);
        let list = self.list_mut(category);
        let Some(d) = list.iter_mut().find(|d| d.class_id == wcid) else {
            return ComponentTrackerUpdate::None;
        };
        d.update_stack_size(obj, stack)
    }

    /// The **one funnel** all three of the
    /// operations above have, and the route by which they reach a caller.
    ///
    /// ```text
    /// owned   = the object is owned by the player
    /// tracked = the object id is already in the tracker
    /// not owned:   tracked -> remove_component, Remove;  else None
    /// not tracked: add_component, Add
    /// otherwise:   update_stack_size
    /// ```
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    pub fn update_component(
        &mut self,
        cat: &ComponentCatalogue,
        obj: ObjectId,
        wcid: u32,
        name: &str,
        icon: u32,
        stack: u32,
        owned_by_player: bool,
    ) -> ComponentTrackerUpdate {
        let tracked = self.object_ids.contains_key(&obj);
        if !owned_by_player {
            if tracked {
                self.remove_component(cat, obj, wcid, stack);
                return ComponentTrackerUpdate::Remove;
            }
            return ComponentTrackerUpdate::None;
        }
        if !tracked {
            self.add_component(cat, obj, wcid, name, icon, stack);
            return ComponentTrackerUpdate::Add;
        }
        self.update_stack_size(cat, obj, wcid, stack)
    }

    /// How many objects the tracker holds, across every class. A denominator for a count that
    /// would otherwise report a bare zero.
    #[must_use]
    pub fn tracked_objects(&self) -> usize {
        self.object_ids.len()
    }
}

/// The spell book — one packed table from spell id to `SpellBookPage`.
pub type SpellBook = BTreeMap<u32, dereth_protocol::types::qualities::SpellBookPage>;

/// Spellbook visibility filtering by school and level.
///
/// The `Level_9` bit is honoured even though no client path sets it.
#[must_use]
pub fn is_filtered_out(filters: u32, school: u32, level: u32) -> bool {
    let school_bit = match school {
        1 => spellbook_filter::WAR,
        2 => spellbook_filter::LIFE,
        3 => spellbook_filter::ITEM,
        4 => spellbook_filter::CREATURE,
        5 => spellbook_filter::VOID,
        _ => 0,
    };
    if school_bit != 0 && filters & school_bit == 0 {
        return true;
    }
    if (1..=9).contains(&level) {
        let level_bit = spellbook_filter::LEVEL_1 << (level - 1);
        if filters & level_bit == 0 {
            return true;
        }
    }
    false
}

/// The refusal strings produced by client-side spell casting.
pub mod messages {
    pub const MISSING_COMPONENTS: &str = "You do not have all of this spell's components";
    pub const NEED_TARGET: &str = "You must select a suitable target before casting this spell";
    pub const WOULD_REQUIRE_NO_TARGET: &str = "This spell would require no target";
    pub const WOULD_REQUIRE_A_TARGET: &str = "This spell would require a target";
    pub const CANNOT_CAST_ON_SELF: &str = "You cannot cast this spell upon yourself";
    pub const RESEARCH_NEEDS_MAGIC_MODE: &str =
        "You must first enter magic mode to test spell formulae";
    pub const RESEARCH_NEEDS_TARGET: &str = "You must first select a target for the spell";
    pub const NO_SPELL_RESEARCH: &str = "This world has no spell research.";
    pub const STACK_OF_ITEMS: &str = "Cannot cast spell on a stack of items.";

    /// Not `"{name} is not a valid target for this spell"`: **that sentence does not appear
    /// anywhere in the retail client**. The client references exactly five literals here, and the
    /// formatted one is `"This spell cannot be cast on %s"`. `%s` is the object's name of
    /// name type 2, i.e. [`crate::weenie::NameType::Appropriate`].
    #[must_use]
    pub fn cannot_be_cast_on(name: &str) -> String {
        format!("This spell cannot be cast on {name}")
    }

    /// The client's **success** message, `"Casting %hs"`, shown when the compatibility check's
    /// fourth argument is true, as casting supplies it. `%hs` is the narrow spell name.
    #[must_use]
    pub fn casting(spell_name: &str) -> String {
        format!("Casting {spell_name}")
    }
}

/// `bool` quality **0x44 (68)** — `SpellComponentsRequired`, read from the local player
/// description before component checks.
pub const SPELL_COMPONENTS_REQUIRED: u32 = 0x44;

impl World {
    /// Checks whether an object is compatible with a spell target-type mask.
    ///
    /// The constant `0x8107` is the redirectable item-enchantment target type, the "wielded gear"
    /// family, and it appears twice with different meanings — as the self-cast exemption and as the
    /// type-match escape.
    ///
    /// **This has a `quiet` parameter and a notice sink**, because the client's
    /// refusals are display-string notices on channel `0x1A` made *here*, and
    /// `quiet` is exactly the flag that suppresses them: every quiet early return sits
    /// immediately before the `StringInfo` it would otherwise build. The two
    /// refusals that carry **no** literal — an object the maintenance system does not know, and a
    /// missing player — emit nothing in either mode, which is why they return an empty string.
    ///
    /// # Errors
    /// The refusal text; `quiet` suppresses the *message*, not the refusal.
    pub fn object_compatible_with_spell_target_type(
        &self,
        out: &mut dyn NoticeSink,
        obj: Option<ObjectId>,
        target_type_mask: u32,
        quiet: bool,
    ) -> Result<(), String> {
        const GEAR: u32 = item_type::REDIRECTABLE_ITEM_ENCHANTMENT_TARGET;
        fn refuse(out: &mut dyn NoticeSink, quiet: bool, text: String) -> Result<(), String> {
            if !quiet && !text.is_empty() {
                out.emit(crate::Notice::DisplayString {
                    channel: crate::chat::REFUSAL_CHANNEL,
                    text: text.clone(),
                });
            }
            Err(text)
        }
        if target_type_mask == 0 {
            return if obj.is_none() {
                Ok(())
            } else {
                refuse(out, quiet, messages::WOULD_REQUIRE_NO_TARGET.into())
            };
        }
        let Some(id) = obj else {
            return refuse(out, quiet, messages::WOULD_REQUIRE_A_TARGET.into());
        };
        if target_type_mask & GEAR == 0 && self.player == Some(id) {
            return refuse(out, quiet, messages::CANNOT_CAST_ON_SELF.into());
        }
        let Some(w) = self.weenie(id) else {
            return Err(String::new());
        };
        if w.pwd.stack_size.unwrap_or(0) >= 2 {
            return refuse(out, quiet, messages::STACK_OF_ITEMS.into());
        }
        let type_mismatch = target_type_mask & w.inq_type() == 0 && target_type_mask & GEAR == 0;
        // Bit 4 of `_bitfield` is inferred to mean "attackable" from the reference server, but the
        // client itself branches on this bit here, so the behavior is reproduced with that
        // inference named.
        const BF_ATTACKABLE: u32 = 0x10;
        let not_attackable = !w.is_player() && w.pwd.bitfield & BF_ATTACKABLE == 0;
        if type_mismatch || not_attackable {
            let name = w.object_name(crate::weenie::NameType::Appropriate);
            return refuse(out, quiet, messages::cannot_be_cast_on(&name));
        }
        if w.pwd.pet_owner.unwrap_or_default().0 != 0 {
            let name = w.object_name(crate::weenie::NameType::Appropriate);
            return refuse(out, quiet, messages::cannot_be_cast_on(&name));
        }
        if self.player.is_none_or(|p| self.weenie(p).is_none()) {
            return Err(String::new());
        }
        Ok(())
    }

    /// Tests whether the player carries the school's spell pack WCID.
    ///
    /// The whole function is a walk of the object's **side-pack list** (the side packs, not the
    /// items) comparing each pack's `pwd._wcid` against the argument. This is the client's second
    /// route to the scarab-only formula.
    ///
    /// `wcid` 0 is `INVALID_DID`, which is what [`Self::school_of_magic_to_wcid`] answers when the
    /// dat mapper is absent; no weenie carries WCID 0, so this then refuses — the client's own
    /// no-dat behaviour rather than a substitute for it.
    #[must_use]
    pub fn magic_pack_is_owned(&self, pack_wcid: u32) -> bool {
        let Some(p) = self.player else { return false };
        let Some(inv) = self.inventory(p) else {
            return false;
        };
        inv.containers
            .iter()
            .any(|c| self.weenie(*c).is_some_and(|w| w.pwd.wcid == pack_wcid))
    }

    /// Maps a magic school to its spell-pack WCID through the enum map loaded as
    /// `(4, 0x10000001, 0x28)`. The result is **`INVALID_DID` when the map is absent or misses**.
    ///
    /// **The map is filled from the dat.** The two hops are group `0x10000001`
    /// (`WEENIE_CATEGORIES` -> `DidMapper 0x25000005`)
    /// then value `4` (`SchoolOfMagic`, also spelled `ComponentPacks`) -> `DualDidMapper
    /// 0x27000003`, whose five public rows in the shipped dat are
    ///
    /// | school | name | WCID |
    /// |---:|---|---:|
    /// | 1 | `War` | 15271 (`0x3BA7`) |
    /// | 2 | `Life` | 15270 (`0x3BA6`) |
    /// | 3 | `ItemEnchantment` | 15269 (`0x3BA5`) |
    /// | 4 | `CreatureEnchantment` | 15268 (`0x3BA4`) |
    /// | 5 | `Void` | 43173 (`0xA8A5`) |
    ///
    /// — the five **Foci**, and the same five values the reference server hard-codes. The dat-load
    /// pass resolves
    /// them by that enum walk rather than by a literal DataID, and the `0x0013` arm hands them here.
    /// An absent or unreadable mapper still leaves this 0, which is the client's own
    /// `INVALID_DID` arm.
    #[must_use]
    pub fn school_of_magic_to_wcid(&self, school: u32) -> u32 {
        self.magic
            .school_pack_wcid
            .get(&school)
            .copied()
            .unwrap_or(0)
    }

    /// Choose the formula kind for a spell's school.
    ///
    /// An Infused-Magic augmentation for the spell's school, **or** possession of that school's
    /// spell pack, is what reduces the requirement to the scarab alone.
    ///
    /// `pack_owned` is supplied by the caller because the client makes the two calls at the call
    /// site too; [`Self::spell_formula`] passes
    /// [`Self::magic_pack_is_owned`]`(`[`Self::school_of_magic_to_wcid`]`(school))`.
    /// For the five WCIDs see [`Self::school_of_magic_to_wcid`].
    #[must_use]
    pub fn get_appropriate_spell_formula(&self, school: u32, pack_owned: bool) -> SpellFormulaKind {
        let Some(q) = self.player_qualities() else {
            return SpellFormulaKind::Customized; // not in world yet
        };
        let n = infused_augmentation_for_school(school).map_or(0, |p| q.inq_int(p));
        if n < 1 && !pack_owned {
            return SpellFormulaKind::Customized;
        }
        SpellFormulaKind::ScarabOnly
    }

    /// Reports the player's **bool quality 0x44**, which controls component requirements.
    #[must_use]
    pub fn are_spell_components_required(&self) -> bool {
        self.player_qualities()
            .is_some_and(|q| q.inq_bool(SPELL_COMPONENTS_REQUIRED))
    }

    /// Casts a spell from the UI using the client's validation order.
    ///
    /// Three details a plausible paraphrase gets wrong:
    ///
    /// 1. **The scarab-only arm is not `components[..1]`.** The client does not take merely the
    ///    first component: it keeps **every power component** of the
    ///    decrypted formula and then appends one to four copies of SCID `0xBC`. See
    ///    [`scarab_only_formula`].
    /// 2. **The component list is not the caller's, as WCIDs.** The client resolves it itself, from
    ///    the formula's SCIDs through the SCID-to-WCID map, and iterates
    ///    exactly [`num_spell_components`] slots.
    /// 3. **Refusals are shown.** Every client refusal is a display-string notice on channel
    ///    `0x1A`, and on the *successful* targeted path
    ///    it also prints `"Casting <name>"` — see [`messages::casting`].
    ///
    /// **The customized arm is randomized.** The client applies [`randomize_for_name`] to the
    /// plain decrypted formula, replacing slots
    /// **1, 3 and 6**
    /// with taper ids in `63..=74` derived from a hash of the **account name**. [`Self::
    /// spell_formula`] does the same, from [`crate::player::PlayerSystem::account`], so the
    /// components this checks ownership of are the account's and not the dat's canonical ones.
    /// The **scarab-only** arm never needed it: it reads the decrypted dat formula, not the
    /// per-account customized formula.
    ///
    /// # Errors
    /// The refusal text, which has already been emitted on [`crate::chat::REFUSAL_CHANNEL`]. A
    /// spell the table does not know **returns silently** — `Ok(())` with nothing sent and nothing
    /// shown — which is what the client does when its spell-base lookup fails.
    pub fn cast_spell(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        spell_id: u32,
    ) -> Result<(), String> {
        // The client loads spell table 0x10000005 if it has none, then returns silently if the
        // spell is not in it — a missing table and an unknown spell are the same silent arm.
        let Some(table) = self.magic.spell_table.clone() else {
            return Ok(());
        };
        let Some(base) = table.spells.get(&spell_id).cloned() else {
            return Ok(()); // silently return
        };
        // The client tests whether components are required **first** and only then asks for a
        // formula; the formula selection has no side effects, so the order is not
        // observable, but it is kept because it is the client's.
        if self.are_spell_components_required() {
            let formula = self.spell_formula(&base);
            let n = num_spell_components(&formula);
            // Each of the first `num_spell_components(f)` slots is mapped SCID -> WCID and tested
            // for ownership — the bound is the **count** of non-zero slots and the index walks from 0, so a
            // formula with a hole asks about a zero slot. Reproduced by `take(n)`.
            for scid in formula.iter().take(n) {
                let wcid = self.magic.catalogue.scid_to_wcid(*scid);
                if !self.magic.components.component_is_owned(wcid) {
                    return self.refuse_cast(out, messages::MISSING_COMPONENTS.into());
                }
            }
        }
        if base.bitfield & spell_index::SELF_TARGETED != 0 {
            // Free hands and cast on the player's own id — with no
            // player the client passes **0**, which turns into
            // the *untargeted* event, not an empty error.
            let me = self.player;
            self.free_hands_and_cast(req, spell_id, me);
            return Ok(());
        }
        let target_type = spell_target_type(&base);
        if target_type == 0 {
            // The client inlines the untargeted arm of `free_hands_and_cast` here rather than
            // calling it: free the hands, send the untargeted cast event, raise the busy count —
            // the same three steps in the same order.
            self.free_hands_and_cast(req, spell_id, None);
            return Ok(());
        }
        let Some(selected) = self.selected else {
            return self.refuse_cast(out, messages::NEED_TARGET.into());
        };
        // The compatibility check re-reads the spell base only to reach its target type, delegates to
        // the target-type compatibility test, and on success prints `"Casting %hs"`.
        self.object_compatible_with_spell_target_type(out, Some(selected), target_type, false)?;
        out.emit(crate::Notice::DisplayString {
            channel: crate::chat::REFUSAL_CHANNEL,
            text: messages::casting(&base.name),
        });
        self.free_hands_and_cast(req, spell_id, Some(selected));
        // The client next re-selects `(selected, force = 0)`, which is a **no-op**: the body is
        // guarded by `(force != 0) || (selected != id)`,
        // and here `force` is 0 and `id` *is* the selected id. Nothing is done in its place, and this
        // comment exists so that the absence reads as a reading rather than as an omission.
        Ok(())
    }

    /// The spell research page's test: the formula's components tried on the selected target.
    ///
    /// The early clients refused a test outside magic mode and one with nothing selected, and
    /// otherwise sent the formula as eight component class ids (unused slots zero) and the
    /// target, raising the busy count as a cast does; the server answers as it answers a cast.
    /// `research` is whether the world has spell research at all: a world without it is refused
    /// before anything is sent.
    ///
    /// # Errors
    /// The refusal text, already emitted on [`crate::chat::REFUSAL_CHANNEL`].
    pub fn test_spell_formula(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        components: &[u32],
        research: bool,
    ) -> Result<(), String> {
        if !research {
            return self.refuse_cast(out, messages::NO_SPELL_RESEARCH.into());
        }
        if components.is_empty() {
            return Ok(());
        }
        if self.combat.combat_mode != crate::combat::CombatMode::Magic {
            return self.refuse_cast(out, messages::RESEARCH_NEEDS_MAGIC_MODE.into());
        }
        let Some(target) = self.selected.filter(|t| t.0 != 0) else {
            return self.refuse_cast(out, messages::RESEARCH_NEEDS_TARGET.into());
        };
        let mut slots = [0u32; 8];
        for (slot, wcid) in slots.iter_mut().zip(components) {
            *slot = self.magic.catalogue.wcid_to_scid(*wcid);
        }
        req.send(Request::TestSpellFormula(
            dereth_protocol::combat::MagicTestSpellFormula {
                components: slots,
                target,
            },
        ));
        self.magic.busy_count += 1;
        Ok(())
    }

    /// The cast refusal tail: a display-string notice on channel `0x1A` with the literal, then return.
    fn refuse_cast(&self, out: &mut dyn NoticeSink, text: String) -> Result<(), String> {
        out.emit(crate::Notice::DisplayString {
            channel: crate::chat::REFUSAL_CHANNEL,
            text: text.clone(),
        });
        Err(text)
    }

    /// Resolves the formula that casting checks for component ownership.
    ///
    /// `pack_owned` is not the caller's: it is
    /// [`Self::magic_pack_is_owned`]`(`[`Self::school_of_magic_to_wcid`]`(school))`, the same two
    /// calls the client makes. An unavailable WCID map appears as an empty map rather than as a
    /// missing parameter.
    #[must_use]
    pub fn spell_formula(&self, base: &SpellBase) -> [u32; 8] {
        let plain = decrypt_formula(&base.raw_comps, base.comp_key);
        let pack_owned = self.magic_pack_is_owned(self.school_of_magic_to_wcid(base.school));
        match self.get_appropriate_spell_formula(base.school, pack_owned) {
            // The customized formula is the decrypted formula with account-specific taper
            // randomisation applied over it; returning `plain` would leave the three taper slots
            // holding the dat's canonical ids rather than the account's. The name comes from
            // `PlayerSystem::account`; the two formula-selection paths are the client's only uses
            // of that field.
            SpellFormulaKind::Customized => {
                let mut f = plain;
                randomize_for_name(&mut f, &self.player_system.account, base.formula_version);
                f
            }
            SpellFormulaKind::ScarabOnly => scarab_only_formula(&plain),
        }
    }

    /// Free the hands and cast the spell at the target.
    ///
    /// Stops movement, sends one message and increments the busy count. That is the entire
    /// client-side casting state — no windup, no gesture, no mana deduction.
    fn free_hands_and_cast(
        &mut self,
        req: &mut dyn RequestSink,
        spell_id: u32,
        target: Option<ObjectId>,
    ) {
        match target {
            Some(t) => req.send(Request::CastTargetedSpell(MagicCastTargetedSpell {
                target: t,
                spell_id,
            })),
            None => req.send(Request::CastUntargetedSpell(MagicCastUntargetedSpell {
                spell_id,
            })),
        }
        self.magic.busy_count += 1;
    }

    /// Handles `0x01C7 Item_UseDone(error)`, the **universal "action finished" acknowledgement**.
    ///
    /// The whole handler is two things:
    ///
    /// ```text
    /// busy_count -= 1;  if (busy_count == 0) update the cursor state;
    /// if (error != 0) handle_failure_event(error, "");
    /// ```
    ///
    /// The chat half is the caller's, because the failure-event table lives above this crate
    /// (`dereth_ui_screens::chat::failure`); this returns the code so the caller can print it.
    ///
    /// **One deliberate difference.** The count is raised by the cast path
    /// ([`Self::free_hands_and_cast`]), by using an object, by a targeted use, by a shop request,
    /// by a swing the server commenced, by an examine, by an allegiance request and by a teleport.
    /// Retail's count wraps below zero, so an acknowledgement nobody asked for leaves the busy
    /// cursor up until enough raises bring it back round to zero; this one floors at zero instead,
    /// so a stray acknowledgement never leaves the cursor stuck.
    pub fn use_done(&mut self, error: u32) -> u32 {
        self.magic.busy_count = self.magic.busy_count.saturating_sub(1);
        error
    }
}

impl World {
    /// Updates the spell-component tracker from one object: **the tracker's production writer.**
    ///
    /// The client's own gate is *outside* this function, at each of its three call sites, and it is
    /// an `ITEM_TYPE` test on the weenie's item-type query:
    ///
    /// ```text
    /// t = object_item_type(obj);
    /// if (t & 0x1000) update_spell_component(obj, notify 1)    // TYPE_SPELL_COMPONENTS
    /// if (t & 0x200)  ... walk the contained items and repeat   // TYPE_CONTAINER
    /// ```
    ///
    /// The same gate occurs on both server-directed item movement and item-attribute changes. It is
    /// reproduced
    /// here rather than at the call site so that every route into the tracker carries it — there
    /// are three in the client and they must not disagree.
    ///
    /// The client's component update itself defers while the player id is 0, queueing the id on
    /// a pending-components list for player initialization to drain. That queue is not
    /// reproduced: this build has no separate "player not yet initialised" window at this seam
    /// ([`Self::player`] is set by the same `0xF746` that creates the objects), and an object that
    /// arrives before the player is re-offered by the `0x0013` sweep below.
    ///
    /// Returns the `ComponentTrackerUpdate` so a caller can count what happened rather than assume
    /// it.
    pub fn update_spell_component(&mut self, id: ObjectId) -> ComponentTrackerUpdate {
        let Some(w) = self.weenie(id) else {
            return ComponentTrackerUpdate::None;
        };
        if w.inq_type() & item_type::SPELL_COMPONENTS == 0 {
            return ComponentTrackerUpdate::None;
        }
        let wcid = w.pwd.wcid;
        let name = w.pwd.name.clone();
        let icon = w.pwd.icon_id;
        let stack = u32::from(w.pwd.stack_size.unwrap_or(0));
        let owned = self.is_owned_by_player(id);
        // `self.magic.catalogue` and `self.magic.components` are two fields of the same struct, so
        // the borrow has to be split by taking the tracker out and putting it back.
        let mut tracker = std::mem::take(&mut self.magic.components);
        let r =
            tracker.update_component(&self.magic.catalogue, id, wcid, &name, icon, stack, owned);
        self.magic.components = tracker;
        // The client sends an update-components notice here for **every** component object offered
        // with `notify != 0`,
        // whatever the tracker update answered. See
        // [`MagicState::component_serial`] for why the pull side is a serial and why the
        // container walk's `notify = 0` coalescing needs nothing extra here.
        self.magic.component_serial = self.magic.component_serial.wrapping_add(1);
        r
    }

    /// Resolves a component object against this world's own tracker and SCID map. The two client
    /// lookups are fields of this struct here.
    ///
    /// The catalogue is `self.magic.catalogue` and not the HUD's dat copy deliberately: the
    /// tracker's rows were *bucketed* with this catalogue (the add indexes the
    /// category lists by `determine_component_category(wcid)`), so a walk driven by a different one
    /// could look in the wrong bucket. `0x0013` fills both.
    #[must_use]
    pub fn component_object_id(&self, scid: u32) -> Option<ObjectId> {
        self.magic
            .components
            .component_object_id(&self.magic.catalogue, scid)
    }

    /// The container arm of the same two lines: `TYPE_CONTAINER` (`0x200`) makes the client walk
    /// the contained-items list and offer every child.
    ///
    /// Returns how many objects were offered — the denominator for the count of changes.
    pub fn update_spell_components_under(&mut self, id: ObjectId) -> (usize, usize) {
        let mut offered = 0usize;
        let mut changed = 0usize;
        let mut queue = vec![id];
        let is_container = self
            .weenie(id)
            .is_some_and(|w| w.inq_type() & item_type::CONTAINER != 0);
        if is_container {
            queue.extend(self.exhaustive_contained_items(id));
        }
        for c in queue {
            offered += 1;
            if self.update_spell_component(c) != ComponentTrackerUpdate::None {
                changed += 1;
            }
        }
        (offered, changed)
    }

    /// The client's message `0x2F` arm — the player
    /// typed a number into the row's edit field `0x1000046B` and committed it.
    ///
    /// ```text
    /// n = the field 0x1000046b's text as an integer
    /// if ((-1 < n) && (n < 0x1389)) { send_desired_component_level(wcid, n);
    ///                                 store_desired_component_level(wcid, n); }
    /// else                            set the field's text to the stored desired level  // revert
    /// ```
    ///
    /// Both halves, in that order: the wire first, then the local mirror. Out of range is **not** a
    /// send — the panel puts the stored value back — which is why this returns the level the field
    /// should now show rather than a bare bool.
    ///
    /// `0x1389` is written here as the literal the panel tests, and
    /// [`crate::player::MAX_DESIRED_COMP_LEVEL`] is its inclusive twin; the two are asserted equal
    /// in this module's tests so a
    /// wrong constant cannot hide behind either spelling.
    pub fn set_desired_component_level(
        &mut self,
        req: &mut dyn RequestSink,
        wcid: u32,
        level: i32,
    ) -> i32 {
        if !(0..MAX_DESIRED_COMP_LEVEL).contains(&level) {
            return self.player_system.desired_comp_level(wcid);
        }
        req.send(Request::SetDesiredComponentLevel(
            dereth_protocol::combat::ComponentLevelRequest {
                component_did: wcid,
                level,
            },
        ));
        self.player_system.set_desired_comp_level(wcid, level);
        level
    }

    /// The client's `clear` arm — the `/fillcomps
    /// clear` command.
    ///
    /// ```text
    /// send_desired_component_level(INVALID_DID, -1);
    /// clear_desired_component_levels();
    /// notify_update_spell_components(2);
    /// ```
    ///
    /// The `(INVALID_DID, -1)` pair is a **sentinel**: it is the only place a negative level goes
    /// on the wire, and it bypasses the desired-level setter's `level < 0` rejection by not calling
    /// it at all. `INVALID_DID` is 0.
    pub fn clear_desired_components(&mut self, req: &mut dyn RequestSink) {
        req.send(Request::SetDesiredComponentLevel(
            dereth_protocol::combat::ComponentLevelRequest {
                component_did: 0,
                level: -1,
            },
        ));
        self.player_system.clear_desired_comps();
    }

    /// The client's drain of its pending-components list, as a
    /// sweep over everything the player already holds.
    ///
    /// The client queues the objects that arrived before the player id was set and replays them
    /// here; this offers the player's whole inventory instead, which reaches the same set and is
    /// the only form available at a seam that has no queue. Returns `(offered, changed)`.
    pub fn initialize_spell_components(&mut self) -> (usize, usize) {
        let Some(p) = self.player else { return (0, 0) };
        let items = self.exhaustive_contained_items(p);
        let mut offered = 0usize;
        let mut changed = 0usize;
        for c in items {
            offered += 1;
            if self.update_spell_component(c) != ComponentTrackerUpdate::None {
                changed += 1;
            }
        }
        // The client sends one update-components notice after the drain, **unconditionally**, so a player
        // carrying no components at all still refreshes every listener.
        self.magic.component_serial = self.magic.component_serial.wrapping_add(1);
        (offered, changed)
    }

    /// Tests whether the player owns the component represented by a formula slot's **SCID**.
    ///
    /// The client first maps SCID to WCID, then tests ownership. Ownership is **not** the object-id
    /// walk and **not** a count: it is bare membership in the class-id set, keyed by WCID. The set is
    /// populated when a new component-data row is inserted and erased when removal makes
    /// `numItems` reach 0, so "owned" means *the class has a
    /// row with at least one item*, tested as membership rather than by counting.
    ///
    /// **A missing tracker answers "not owned"**, selecting the arm that shows the missing mark.
    /// An empty catalogue maps every SCID to `0`, which no real class is
    /// keyed under, so it lands on the same answer.
    #[must_use]
    pub fn spell_component_is_owned(&self, scid: u32) -> bool {
        let wcid = self.magic.catalogue.scid_to_wcid(scid);
        wcid != 0 && self.magic.components.component_is_owned(wcid)
    }
}

/// Returns what a spell may be cast at, read from its formula's **target component**.
///
/// The formula is decrypted first, and only a complete one (its first five slots non-zero) names a
/// target; an incomplete one answers 0, which makes the spell untargeted. See
/// [`formula_target_type`] for which slot is the target component. The spell record's separate
/// non-component target field plays no part.
#[must_use]
pub fn spell_target_type(base: &SpellBase) -> u32 {
    formula_target_type(&decrypt_formula(&base.raw_comps, base.comp_key))
}

/// Returns the target type a **decrypted** formula names, or 0 unless it is complete.
///
/// The target component is the last filled slot of the run that starts at slot 5: the walk goes up
/// from slot 5 while the slot is non-zero, stopping at slot 8, and takes the slot before the one it
/// stopped at. A formula with only its five required slots therefore aims with slot 4, and a full
/// eight-slot formula with slot 7. That component is mapped by
/// [`dereth_rules::weenie::spell_target_type_of_component`].
#[must_use]
pub fn formula_target_type(comps: &[u32; 8]) -> u32 {
    if !formula_is_complete(comps) {
        return 0;
    }
    let mut i = 5;
    while i < 8 && comps[i] != 0 {
        i += 1;
    }
    dereth_rules::weenie::spell_target_type_of_component(comps[i - 1])
}

/// Client-side spell-casting state.
#[derive(Debug, Clone, Default)]
pub struct MagicState {
    pub components: ComponentTracker,
    /// The spell-component table joined to the WCID mapper; see
    /// [`ComponentCatalogue`]. The host fills it from the dats once; until it does, every category
    /// is `Undef` and every component name is empty.
    pub catalogue: ComponentCatalogue,
    /// The spell-index `BENEFICIAL` bit per spell id — the one value the total-update path reads
    /// from the spell table, hoisted so the enchantment
    /// registry can count without holding a dat. Absent means the table has not been loaded, which
    /// is the client's null-table result: **the counters are then left alone**
    /// rather than guessed.
    pub spell_beneficial: BTreeMap<u32, bool>,
    /// The update-spell-components notice represented as a serial.
    ///
    /// The client *pushes* that notice; this build's panels *pull*, so the notice is a counter and
    /// a reader that sees a different value does what the listener does. There are five direct
    /// call sites and no indirect route:
    ///
    /// | site | argument | when |
    /// |---|---|---|
    /// | component update | the update code written through the out parameter | **per component object offered**, when the caller's `notify` is non-zero |
    /// | two container-move exits | the last child's update | **once**, after walking a moved container's contents, each of which was offered with `notify = 0` |
    /// | player initialization | literal `1` | after the `0x0013` drain, unconditionally |
    /// | `/fillcomps clear` | literal `2` | not a tracker change at all |
    ///
    /// **Neither listener reads the argument.** Each returns without touching it and re-resolves
    /// every row regardless. So the
    /// notice is an edge, which is what a serial is.
    ///
    /// Bumped for every object that passes the component-update path's `ITEM_TYPE &
    /// 0x1000` gate — retail's `notify` condition — and once for the `0x0013` drain. A pull that
    /// runs once a frame coalesces a container's worth of bumps into one refresh, which is what
    /// the client's `notify = 0` walk plus one trailing notice does by hand.
    pub component_serial: u64,
    /// The busy count: how many actions the player asked for are still waiting on their answer.
    /// While it is not zero the pointer is the hourglass. It is also the **whole** of the
    /// client's casting state.
    pub busy_count: u32,
    /// The spell table the client lazily loads as `(6, 2, 0x10000005)` on the **first** cast or
    /// spell-compatibility test and then keeps.
    ///
    /// It lives here rather than being handed to the casting function because that is
    /// where the client keeps it: the cast's first act is to load the table into the magic system
    /// when it has none. `None` is the dat object missing, on which
    /// the spell-base lookup fails and the cast **returns silently** — the same arm as an unknown spell.
    /// An `Arc` because the host loads it once and this is a second reference, not a second copy.
    pub spell_table: Option<std::sync::Arc<SpellTable>>,
    /// The enum map from school to spell-pack WCID, loaded as `(4, 0x10000001, 0x28)`.
    ///
    /// **Empty until the host supplies the dat mapping.** An empty map is exactly the client with
    /// that dat object
    /// missing — the school-to-WCID lookup leaves `INVALID_DID`, `magic_pack_is_owned` compares every side
    /// pack against it and finds none. Filling it is the whole of what the pack route still needs;
    /// nothing else about that route is missing. A test may fill it to reach the arm.
    pub school_pack_wcid: BTreeMap<u32, u32>,
}

impl MagicState {
    /// The client's only table read, as a closure the registry can take.
    ///
    /// `None` is both "no spell table", which returns 0 and changes no counter, and "the table does
    /// not know this spell"; the client handles both through the same missing-row arm.
    #[must_use]
    pub fn spell_is_beneficial(&self, spell_id: u16) -> Option<bool> {
        self.spell_beneficial.get(&u32::from(spell_id)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's scarab table, including the fact that 0x6F maps to 0.
    #[test]
    fn scarab_power_levels_match_the_documented_table() {
        for i in 1..=6u32 {
            assert_eq!(scarab_power_level(i), i);
        }
        assert_eq!(scarab_power_level(0x6E), 7);
        assert_eq!(scarab_power_level(0x70), 8);
        assert_eq!(scarab_power_level(0xC0), 9);
        assert_eq!(scarab_power_level(0xC1), 10);
        assert_eq!(
            scarab_power_level(0x6F),
            0,
            "0x6F is in InqScarabOnlyFormula's keep-list but has no power level"
        );
        assert_eq!(scarab_power_level(0), 0);
    }

    /// Oracle: the client's rough spell-level heuristic.
    #[test]
    fn the_rough_heuristic_maps_ten_powers_onto_eight_levels() {
        let levels: Vec<u32> = (1..=10).map(spell_level_by_rough_heuristic).collect();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6, 6, 7, 7, 8]);
    }

    /// Oracle: formula completeness tests the **first five** slots.
    #[test]
    fn an_incomplete_formula_makes_a_spell_untargetable() {
        assert!(formula_is_complete(&[1, 2, 3, 4, 5]));
        assert!(
            formula_is_complete(&[1, 2, 3, 4, 5, 0, 0, 0]),
            "only the first five matter"
        );
        assert!(!formula_is_complete(&[1, 2, 3, 4, 0, 6, 7, 8]));
        assert!(!formula_is_complete(&[1, 2, 3, 4]));
    }

    /// Oracle: the target component is the last non-zero slot of the run from slot 5 up, read from
    /// the decrypted formula.
    #[test]
    fn the_target_slot_is_the_last_nonzero_slot_from_five_up() {
        // 0x31 is a creature component, 0x39 an item one, 0x3B a portal one, 0x40 names nothing.
        let five = [1, 2, 3, 4, 0x39, 0, 0, 0];
        let six = [1, 2, 3, 4, 0x40, 0x3B, 0, 0];
        let seven = [1, 2, 3, 4, 0x40, 0x40, 0x31, 0];
        let eight = [1, 2, 3, 4, 0x40, 0x40, 0x40, 0x39];
        assert_eq!(formula_target_type(&five), 0x0008_8B8F, "slot 4");
        assert_eq!(formula_target_type(&six), 0x1001_0000, "slot 5");
        assert_eq!(formula_target_type(&seven), 0x10, "slot 6");
        assert_eq!(formula_target_type(&eight), 0x0008_8B8F, "slot 7");
        // A hole at slot 5 stops the walk there even with later slots filled.
        assert_eq!(
            formula_target_type(&[1, 2, 3, 4, 0x31, 0, 0x3B, 0x3B]),
            0x10
        );
        assert_eq!(
            formula_target_type(&[1, 2, 3, 0, 0x31, 0x31, 0, 0]),
            0,
            "incomplete"
        );
        // Slot 4 names nothing and nothing follows: untargeted.
        assert_eq!(formula_target_type(&[1, 2, 3, 4, 0x40, 0, 0, 0]), 0);

        // Through the spell record: the stored slots are the decrypted ones plus the key, and the
        // record's non-component target field is ignored.
        let key = 0x1234_5678u32;
        let mut raw = [0u32; 8];
        for (r, d) in raw.iter_mut().zip(seven) {
            *r = if d == 0 { 0 } else { d.wrapping_add(key) };
        }
        let mut base = SpellBase {
            name: String::new(),
            description: String::new(),
            school: 1,
            icon: 0,
            category: 0,
            bitfield: 0,
            base_mana: 0,
            base_range_constant: 0.0,
            base_range_mod: 0.0,
            power: 0,
            spell_economy_mod: 0.0,
            formula_version: 0,
            component_loss: 0.0,
            meta_spell_type: 2,
            meta_spell_id: 0,
            duration: None,
            portal_lifetime: None,
            raw_comps: raw,
            comp_key: key,
            comps: Vec::new(),
            caster_effect: 0,
            target_effect: 0,
            fizzle_effect: 0,
            recovery_interval: 0.0,
            recovery_amount: 0.0,
            display_order: 0,
            non_component_target_type: 0x0008_8B8F,
            mana_mod: 0,
        };
        assert_eq!(spell_target_type(&base), 0x10);
        base.raw_comps[6] = 0;
        assert_eq!(spell_target_type(&base), 0, "slot 5 names nothing");
    }

    /// Oracle: the five-school infused-magic augmentation table.
    #[test]
    fn the_five_infused_augmentations_map_to_the_documented_properties() {
        assert_eq!(infused_augmentation_for_school(1), Some(0x129));
        assert_eq!(infused_augmentation_for_school(2), Some(0x128));
        assert_eq!(infused_augmentation_for_school(3), Some(0x127));
        assert_eq!(infused_augmentation_for_school(4), Some(0x126));
        assert_eq!(infused_augmentation_for_school(5), Some(0x148));
        assert_eq!(infused_augmentation_for_school(0), None);
    }

    /// A catalogue with two scarabs and a taper, so the category split is exercised.
    fn catalogue() -> ComponentCatalogue {
        ComponentCatalogue::new(
            [(1, 100), (2, 200), (3, 300)],
            [
                (
                    1,
                    ComponentBase {
                        name: "Lead Scarab".into(),
                        category: 0,
                        icon: 0x600_0001,
                    },
                ),
                (
                    2,
                    ComponentBase {
                        name: "Iron Scarab".into(),
                        category: 0,
                        icon: 0x600_0002,
                    },
                ),
                (
                    3,
                    ComponentBase {
                        name: "Red Taper".into(),
                        category: 5,
                        icon: 0x600_0003,
                    },
                ),
            ],
        )
    }

    /// Oracle: ownership tests **presence**, never count, which is why the client sends casts the
    /// server refuses. Quantity lives on the component-data row.
    #[test]
    fn the_component_tracker_tests_presence_not_quantity() {
        let c = catalogue();
        let mut t = ComponentTracker::new();
        assert!(!t.component_is_owned(100));
        t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 1);
        assert!(t.component_is_owned(100));
        assert_eq!(t.num_component(&c, 100), 1);
        // A stack of exactly one is "owned" — the whole point.
        assert_eq!(
            t.update_stack_size(&c, ObjectId(1), 100, 1),
            ComponentTrackerUpdate::None,
            "the same size is not a change"
        );
        assert!(t.component_is_owned(100));
        t.add_component(&c, ObjectId(2), 200, "Iron Scarab", 0, 50);
        assert_eq!(t.num_component(&c, 200), 50);
        t.remove_component(&c, ObjectId(2), 200, 50);
        assert!(
            !t.component_is_owned(200),
            "the last object of a class un-owns it"
        );
    }

    /// Removing a component drops the class **only when `numItems` reaches 0** — the
    /// defect the old WCID-keyed transcription had, and the reason it could not have been right:
    /// two stacks of the same component in two slots is the ordinary case.
    #[test]
    fn removing_one_of_two_stacks_leaves_the_component_owned() {
        let c = catalogue();
        let mut t = ComponentTracker::new();
        t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 40);
        t.add_component(&c, ObjectId(2), 100, "Lead Scarab", 0, 60);
        assert_eq!(t.num_component(&c, 100), 100, "one row, two objects");
        assert_eq!(t.category(0).len(), 1);
        assert_eq!(t.category(0)[0].object_count(), 2);
        t.remove_component(&c, ObjectId(1), 100, 40);
        assert!(t.component_is_owned(100), "the other stack is still held");
        assert_eq!(t.num_component(&c, 100), 60);
        t.remove_component(&c, ObjectId(2), 100, 60);
        assert!(!t.component_is_owned(100));
        assert_eq!(t.category(0).len(), 0);
    }

    /// Add, remove, and stack-size update all substitute **1** for a stack size of 0, and the
    /// update reports its direction.
    #[test]
    fn a_zero_stack_size_counts_as_one_and_an_update_reports_its_direction() {
        let c = catalogue();
        let mut t = ComponentTracker::new();
        t.add_component(&c, ObjectId(1), 300, "Red Taper", 0, 0);
        assert_eq!(
            t.num_component(&c, 300),
            1,
            "an unstackable component is one item"
        );
        assert_eq!(
            t.category(5).len(),
            1,
            "and it is in the Taper bucket, not the Scarab one"
        );
        assert_eq!(
            t.update_stack_size(&c, ObjectId(1), 300, 5),
            ComponentTrackerUpdate::Add
        );
        assert_eq!(t.num_component(&c, 300), 5);
        assert_eq!(
            t.update_stack_size(&c, ObjectId(1), 300, 2),
            ComponentTrackerUpdate::Remove
        );
        assert_eq!(t.num_component(&c, 300), 2);
        assert_eq!(
            t.update_stack_size(&c, ObjectId(1), 300, 2),
            ComponentTrackerUpdate::None
        );
    }

    /// Adding a component keeps each category list sorted by `pwd._name` — the `strcmp`
    /// walk. `SpellComponentPanel` draws the list in that order and does no sorting of its own.
    #[test]
    fn a_category_list_is_kept_in_name_order() {
        let c = catalogue();
        let mut t = ComponentTracker::new();
        t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 1);
        t.add_component(&c, ObjectId(2), 200, "Iron Scarab", 0, 1);
        let names: Vec<&str> = t.category(0).iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["Iron Scarab", "Lead Scarab"]);
    }

    /// The client's three arms, which is the only thing that calls the other
    /// three. `Undef` (8) is where a component the SCID map does not know lands: the add
    /// indexes the category lists unchecked, so it is **kept**, not dropped, and the
    /// panel — which walks 0..7 — never draws it.
    #[test]
    fn the_update_funnel_picks_add_remove_or_stack_and_undef_is_a_real_bucket() {
        let c = catalogue();
        let mut t = ComponentTracker::new();
        let up = |t: &mut ComponentTracker, id, wcid, name, stack, owned| {
            t.update_component(&c, ObjectId(id), wcid, name, 0, stack, owned)
        };
        assert_eq!(
            up(&mut t, 7, 100, "Lead Scarab", 3, false),
            ComponentTrackerUpdate::None
        );
        assert_eq!(
            up(&mut t, 7, 100, "Lead Scarab", 3, true),
            ComponentTrackerUpdate::Add
        );
        assert_eq!(
            up(&mut t, 7, 100, "Lead Scarab", 9, true),
            ComponentTrackerUpdate::Add
        );
        assert_eq!(t.num_component(&c, 100), 9);
        assert_eq!(
            up(&mut t, 7, 100, "Lead Scarab", 9, false),
            ComponentTrackerUpdate::Remove
        );
        assert!(!t.component_is_owned(100));
        assert_eq!(t.tracked_objects(), 0);

        // A WCID the mapper does not carry: `wcid_to_scid` answers 0, `inq_spell_component_base(0)`
        // misses, and the category is `Undef`.
        assert_eq!(
            c.determine_component_category(999),
            component_category::UNDEF
        );
        assert_eq!(c.comp_name_from_wcid(999), "");
        assert_eq!(
            up(&mut t, 8, 999, "Mystery Powder", 1, true),
            ComponentTrackerUpdate::Add
        );
        assert!(t.component_is_owned(999), "kept, in the Undef bucket");
        assert_eq!(t.category(component_category::UNDEF).len(), 1);
        assert_eq!(
            t.categories().map(|(_, l)| l.len()).sum::<usize>(),
            0,
            "and never drawn"
        );
    }

    /// The catalogue's two hops, and the literal each is pinned against.
    ///
    /// WCID-to-SCID lookup walks a `DualDidMapper` **backwards**, so
    /// the constructor takes `(SCID, WCID)` pairs in the mapper's own direction and the lookup
    /// answers the other way. A miss is **0**, not an error — the client's out parameter is left
    /// at its initialiser.
    #[test]
    fn the_catalogue_reverses_the_dual_enum_map_and_answers_zero_on_a_miss() {
        let c = catalogue();
        assert_eq!(c.len(), 3);
        assert!(!c.is_empty());
        assert_eq!(c.wcid_to_scid(100), 1);
        assert_eq!(c.wcid_to_scid(300), 3);
        assert_eq!(
            c.wcid_to_scid(0xDEAD),
            0,
            "a miss leaves the out parameter at 0"
        );
        assert_eq!(c.comp_name_from_wcid(300), "Red Taper");
        assert_eq!(
            c.determine_component_category(300),
            component_category::TAPER
        );
        assert_eq!(component_category::TAPER, 5);
        assert!(ComponentCatalogue::default().is_empty());
        assert_eq!(
            ComponentCatalogue::default().determine_component_category(100),
            component_category::UNDEF,
            "no dats loaded is Undef, not Scarab"
        );
    }

    /// Oracle: the spellbook-filter table, including the otherwise unused `Level_9` bit.
    #[test]
    fn the_spellbook_filter_honours_every_bit_including_the_unset_level_nine() {
        assert_eq!(spellbook_filter::DEFAULT, 16383);
        assert_eq!(spellbook_filter::DEFAULT.count_ones(), 14);
        assert_eq!(spellbook_filter::LEVEL_9, 4096);

        let all = spellbook_filter::DEFAULT;
        assert!(!is_filtered_out(all, 1, 5));
        assert!(
            is_filtered_out(all & !spellbook_filter::WAR, 1, 5),
            "war school filtered out"
        );
        assert!(is_filtered_out(all & !spellbook_filter::LEVEL_5, 1, 5));
        assert!(
            is_filtered_out(all & !spellbook_filter::LEVEL_9, 1, 9),
            "Level_9 is honoured even though nothing sets it"
        );
        assert!(
            !is_filtered_out(all & !spellbook_filter::VOID, 1, 5),
            "a different school"
        );
    }

    /// Oracle: target-type compatibility, arm by arm.
    #[test]
    fn target_type_validation_follows_the_documented_arms() {
        use crate::weenie::Weenie;
        use dereth_protocol::types::PublicWeenieDesc;
        let mut w = World::new();
        w.set_player(ObjectId(1));
        let mut me = Weenie::new(ObjectId(1));
        me.pwd = PublicWeenieDesc {
            name: "Lark".into(),
            bitfield: crate::weenie::bitfield::PLAYER,
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(1), me);
        let mut stack = Weenie::new(ObjectId(2));
        stack.pwd = PublicWeenieDesc {
            name: "Pyreal".into(),
            stack_size: Some(5),
            obj_type: item_type::MONEY,
            bitfield: 0x10,
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(2), stack);

        let mut out = crate::RecordingSink::default();
        let check = |out: &mut crate::RecordingSink, obj, mask| {
            w.object_compatible_with_spell_target_type(out, obj, mask, false)
        };
        // Mask 0: a target is a refusal, no target is fine.
        assert_eq!(check(&mut out, None, 0), Ok(()));
        assert_eq!(
            check(&mut out, Some(ObjectId(1)), 0),
            Err(messages::WOULD_REQUIRE_NO_TARGET.into())
        );
        // A non-zero mask with no target.
        assert_eq!(
            check(&mut out, None, item_type::CREATURE),
            Err(messages::WOULD_REQUIRE_A_TARGET.into())
        );
        // Self-cast without the gear family.
        assert_eq!(
            check(&mut out, Some(ObjectId(1)), item_type::CREATURE),
            Err(messages::CANNOT_CAST_ON_SELF.into())
        );
        // ...but the gear family exempts it.
        assert_eq!(
            check(
                &mut out,
                Some(ObjectId(1)),
                item_type::REDIRECTABLE_ITEM_ENCHANTMENT_TARGET
            ),
            Ok(())
        );
        // A stack is refused before the type test.
        assert_eq!(
            check(&mut out, Some(ObjectId(2)), item_type::MONEY),
            Err(messages::STACK_OF_ITEMS.into())
        );

        let lines: Vec<(u32, String)> = out
            .0
            .iter()
            .filter_map(|n| match n {
                crate::Notice::DisplayString { channel, text } => Some((*channel, text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines.len(),
            4,
            "one per refusal, none for the two successes: {lines:?}"
        );
        assert!(lines.iter().all(|(c, _)| *c == 0x1A), "{lines:?}");
        assert_eq!(
            lines.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
            vec![
                "This spell would require no target",
                "This spell would require a target",
                "You cannot cast this spell upon yourself",
                "Cannot cast spell on a stack of items.",
            ]
        );

        // `quiet = true` refuses **without** the message — the client's quiet early
        // return sitting in front of each `StringInfo`. Both directions, because a probe that
        // only ever reads "no message" cannot tell suppression from an instrument that never
        // looked.
        let mut quiet = crate::RecordingSink::default();
        assert_eq!(
            w.object_compatible_with_spell_target_type(
                &mut quiet,
                Some(ObjectId(2)),
                item_type::MONEY,
                true
            ),
            Err(messages::STACK_OF_ITEMS.into())
        );
        assert!(
            quiet.0.is_empty(),
            "quiet suppresses the message, not the refusal"
        );
    }

    /// The refusal strings are the binarys own literals.
    #[test]
    fn the_refusal_strings_are_the_binarys_own_literals() {
        assert_eq!(
            messages::WOULD_REQUIRE_NO_TARGET,
            "This spell would require no target"
        );
        assert_eq!(
            messages::WOULD_REQUIRE_A_TARGET,
            "This spell would require a target"
        );
        assert_eq!(
            messages::CANNOT_CAST_ON_SELF,
            "You cannot cast this spell upon yourself"
        );
        assert_eq!(
            messages::STACK_OF_ITEMS,
            "Cannot cast spell on a stack of items."
        );
        assert_eq!(
            messages::MISSING_COMPONENTS,
            "You do not have all of this spell's components"
        );
        assert_eq!(
            messages::NEED_TARGET,
            "You must select a suitable target before casting this spell"
        );
        // `"This spell cannot be cast on %s"`, `%s` = the object's name in form 2.
        assert_eq!(
            messages::cannot_be_cast_on("a Drudge Slave"),
            "This spell cannot be cast on a Drudge Slave"
        );
        // `"Casting %hs"`, where `%hs` is the spell's narrow name.
        assert_eq!(
            messages::casting("Strength Self VI"),
            "Casting Strength Self VI"
        );
        // And the one that was wrong: no such sentence exists in the retail client.
        assert!(!messages::cannot_be_cast_on("x").contains("is not a valid target"));
    }

    /// Oracle: §4.3 — the augmentation **or** the spell pack selects the scarab-only formula.
    #[test]
    fn either_the_augmentation_or_the_pack_selects_the_scarab_only_formula() {
        use crate::qualities::{Qualities, StatKey, StatType, StatValue};
        let mut w = World::new();
        assert_eq!(
            w.get_appropriate_spell_formula(1, false),
            SpellFormulaKind::Customized,
            "no player yet"
        );
        w.set_player(ObjectId(1));
        let mut me = crate::weenie::Weenie::new(ObjectId(1));
        me.qualities = Some(Qualities::new());
        w.tables.weenies.insert(ObjectId(1), me);

        assert_eq!(
            w.get_appropriate_spell_formula(1, false),
            SpellFormulaKind::Customized
        );
        assert_eq!(
            w.get_appropriate_spell_formula(1, true),
            SpellFormulaKind::ScarabOnly,
            "owning the school's spell pack is enough"
        );
        w.player_qualities_mut()
            .unwrap()
            .set(StatKey::new(StatType::Int, 0x129), StatValue::Int(1));
        assert_eq!(
            w.get_appropriate_spell_formula(1, false),
            SpellFormulaKind::ScarabOnly,
            "AugmentationInfusedWarMagic is enough on its own"
        );
        assert_eq!(
            w.get_appropriate_spell_formula(2, false),
            SpellFormulaKind::Customized,
            "and it is per school"
        );
    }

    /// Oracle: §1's other enum tables.
    #[test]
    fn the_remaining_enums_match() {
        assert_eq!(spell_index::DAMAGE_OVER_TIME, 0x1_0000);
        assert_eq!(component_category::UNDEF, 8);
        assert_eq!(NUM_SPELLCAST_BANKS, 8);
        assert_eq!(LOWEST_TAPER_ID, 63);
        assert_eq!(NUM_TAPERS, 12);
        assert_eq!(portal_recall::LINKED_PORTAL_TWO, 5);
        assert_eq!(portal_summon::LINKED_PORTAL_TWO, 2);
        assert_eq!(MAX_DESIRED_COMP_LEVEL, 5001);
    }

    /// Decryption subtracts the key from each of eight nonzero component slots. It is applied in
    /// place so **slot
    /// positions survive**.
    #[test]
    fn decrypt_leaves_the_zero_slots_alone_and_keeps_positions() {
        // A key larger than a slot: the client's `-` on a `ulong` wraps, and so must this.
        let raw = [110, 0, 300, 0, 0, 0, 0, 0];
        assert_eq!(decrypt_formula(&raw, 10), [100, 0, 290, 0, 0, 0, 0, 0]);
        assert_eq!(
            decrypt_formula(&raw, 200),
            [110u32.wrapping_sub(200), 0, 100, 0, 0, 0, 0, 0]
        );
        // Zero stays zero whatever the key: that is the whole point of the `if`.
        assert_eq!(decrypt_formula(&[0; 8], 7), [0; 8]);
    }

    /// Pinned behavior: eight independent `if`s, i.e.
    /// the **count** of non-zero slots and not the length of the leading run.
    ///
    /// The distinction is load-bearing: `CastSpell` uses this as its loop bound while indexing
    /// from 0, so on a holed formula it asks about a zero slot. The second assertion pins that.
    #[test]
    fn the_component_count_is_a_population_count_not_a_run_length() {
        assert_eq!(num_spell_components(&[1, 2, 3, 4, 5, 0, 0, 0]), 5);
        assert_eq!(
            num_spell_components(&[1, 0, 3, 0, 5, 0, 7, 0]),
            4,
            "a holed formula"
        );
        assert_eq!(num_spell_components(&[0; 8]), 0);
        assert_eq!(num_spell_components(&[9; 8]), 8);
    }

    /// Oracle: both halves — the
    /// **id** it returns and the **power level** it writes through its out parameter, which is
    /// the one `InqScarabOnlyFormula` switches on.
    #[test]
    fn the_most_powerful_power_component_returns_id_and_level() {
        // `0xC1` is level 10, the highest; `3` is level 3.
        assert_eq!(
            find_most_powerful_power_component(&[3, 0xC1, 0, 0, 0, 0, 0, 0]),
            (0xC1, 10)
        );
        assert_eq!(
            find_most_powerful_power_component(&[0xC1, 3, 0, 0, 0, 0, 0, 0]),
            (0xC1, 10)
        );
        // Nothing with a power level: the pair stays (0, 0), which is the `default` filler count.
        assert_eq!(
            find_most_powerful_power_component(&[0x6F, 0, 0, 0, 0, 0, 0, 0]),
            (0, 0)
        );
        assert_eq!(find_most_powerful_power_component(&[0; 8]), (0, 0));
        // The client's own loop bound: it walks `count` slots **from index 0**, so a power
        // component sitting past the count is never examined. Two non-zero slots -> two slots
        // walked -> the `0xC1` at index 4 is invisible.
        assert_eq!(
            find_most_powerful_power_component(&[1, 0, 0, 0, 0xC1, 0, 0, 0]),
            (1, 1)
        );
    }

    /// Oracle: read arm by arm.
    ///
    /// **This is the arm the old `cast_spell` got wrong** — it took `components[..1]`.
    #[test]
    fn the_scarab_only_formula_is_the_power_components_plus_pyreal_filler() {
        // A level-6 scarab (id 6) and four non-power components. `switch (6) -> 4` fillers.
        let f = scarab_only_formula(&[6, 70, 71, 72, 73, 0, 0, 0]);
        assert_eq!(f, [6, 0xBC, 0xBC, 0xBC, 0xBC, 0, 0, 0]);
        assert_eq!(num_spell_components(&f), 5, "not one component, five");
        // Level 1 -> one filler; level 2 -> two; 3 and 4 and **7** -> three.
        assert_eq!(
            scarab_only_formula(&[1, 70, 71, 0, 0, 0, 0, 0]),
            [1, 0xBC, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            scarab_only_formula(&[2, 70, 0, 0, 0, 0, 0, 0]),
            [2, 0xBC, 0xBC, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            scarab_only_formula(&[3, 70, 0, 0, 0, 0, 0, 0]),
            [3, 0xBC, 0xBC, 0xBC, 0, 0, 0, 0]
        );
        assert_eq!(
            scarab_only_formula(&[0x6E, 70, 0, 0, 0, 0, 0, 0]),
            [0x6E, 0xBC, 0xBC, 0xBC, 0, 0, 0, 0],
            "0x6E is power level 7, and 7 sits with 3 and 4 rather than with 5 and 6"
        );
        assert_eq!(
            scarab_only_formula(&[0x70, 70, 0, 0, 0, 0, 0, 0]),
            [0x70, 0xBC, 0xBC, 0xBC, 0xBC, 0, 0, 0],
            "0x70 is power level 8, and 8 sits with 5, 6, 9 and 10"
        );
        // `0x6F` is in the **keep-list** and has power level 0, so it survives and pads nothing.
        // Both halves matter: keeping it is the switch, padding nothing is the level table.
        assert_eq!(
            scarab_only_formula(&[0x6F, 70, 0, 0, 0, 0, 0, 0]),
            [0x6F, 0, 0, 0, 0, 0, 0, 0]
        );
        // The source loop **breaks** on the first zero slot, so a power component behind a hole is
        // dropped rather than packed.
        assert_eq!(scarab_only_formula(&[0, 1, 0, 0, 0, 0, 0, 0]), [0; 8]);
        // Every filler count, as a literal table, so a wrong arm cannot hide behind a symbol.
        let counts: Vec<u32> = (0..=11).map(scarab_only_filler_count).collect();
        assert_eq!(counts, vec![0, 1, 2, 3, 3, 4, 4, 3, 4, 4, 4, 0]);
        assert_eq!(SCARAB_ONLY_FILLER_SCID, 0xBC);
    }

    /// Pinned behavior: a walk of the side-pack list
    /// comparing `pwd._wcid`, and **not** of the items list.
    #[test]
    fn a_spell_pack_is_looked_for_among_the_side_packs_only() {
        use crate::weenie::Weenie;
        use dereth_protocol::types::PublicWeenieDesc;
        let mut w = World::new();
        assert!(!w.magic_pack_is_owned(1234), "no player");
        w.set_player(ObjectId(1));
        w.tables
            .weenies
            .insert(ObjectId(1), Weenie::new(ObjectId(1)));
        let mut pack = Weenie::new(ObjectId(2));
        pack.pwd = PublicWeenieDesc {
            wcid: 1234,
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(2), pack);
        let mut loose = Weenie::new(ObjectId(3));
        loose.pwd = PublicWeenieDesc {
            wcid: 1234,
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(3), loose);

        let mut inv = crate::objects::ObjectInventory::default();
        inv.items.push(ObjectId(3));
        w.tables.inventories.insert(ObjectId(1), inv.clone());
        assert!(
            !w.magic_pack_is_owned(1234),
            "an item in the main pack is not a side pack"
        );
        inv.containers.push(ObjectId(2));
        w.tables.inventories.insert(ObjectId(1), inv);
        assert!(w.magic_pack_is_owned(1234));
        assert!(
            !w.magic_pack_is_owned(1235),
            "and it is the WCID that decides"
        );
        // `INVALID_DID` — what the school-to-WCID lookup leaves when the dat mapper is absent.
        assert!(!w.magic_pack_is_owned(0));

        // With no school map every result is `INVALID_DID`; with a map, each school uses its mapped
        // WCID. Both directions are checked so "always 0" cannot pass as "the map is empty".
        assert_eq!(w.school_of_magic_to_wcid(1), 0);
        w.magic.school_pack_wcid.insert(1, 1234);
        assert_eq!(w.school_of_magic_to_wcid(1), 1234);
        assert_eq!(w.school_of_magic_to_wcid(2), 0, "and it is per school");
    }
}
