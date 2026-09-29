// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesEnchantmentRegistryExtensions.cs
//! `PropertiesEnchantmentRegistryExtensions`: queries over a biota's
//! `ICollection<PropertiesEnchantmentRegistry>`, which may be `null` (`None`). ACE's
//! `ReaderWriterLockSlim` parameter is dropped (see [`crate::models`]).
//!
//! ACE hands back the registry's own entry objects, and callers sometimes mutate them. Here the
//! read queries return references into the list, [`get_enchantment_by_spell_mut`] and
//! [`get_enchantments_by_category_indices`] serve the callers that update a returned
//! entry in place, and [`heart_beat_enchantments_and_return_expired`]
//! returns copies of the expired entries (its caller removes them by spell and caster id).
//!
//! The "top layer" queries are LINQ `GroupBy` + `OrderByDescending(...).ThenByDescending(...)
//! .First()`: groups come out in the order their category first appears, and within a group the
//! earliest entry among the equal best wins (the sort is stable).

use std::cmp::Ordering;

use crate::enums::{EnchantmentTypeFlags, EquipmentSet, SpellCategory, SpellId};
use crate::models::properties_enchantment_registry::PropertiesEnchantmentRegistry;

/// A copy of the list (`ToList()`), or `None` for a null one. ACE's copy shares the entry
/// objects; this one copies them.
// ACE: PropertiesEnchantmentRegistryExtensions.Clone
#[must_use]
pub fn clone(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
) -> Option<Vec<PropertiesEnchantmentRegistry>> {
    let value = value?;

    Some(value.clone())
}

// ACE: PropertiesEnchantmentRegistryExtensions.HasEnchantments
#[must_use]
pub fn has_enchantments(value: Option<&Vec<PropertiesEnchantmentRegistry>>) -> bool {
    let Some(value) = value else { return false };

    !value.is_empty()
}

/// Whether any entry is for `spell_id`. ACE compares `int SpellId` with `uint spellId`, which C#
/// does as `long`, so a negative `SpellId` never matches.
// ACE: PropertiesEnchantmentRegistryExtensions.HasEnchantment
#[must_use]
pub fn has_enchantment(value: Option<&Vec<PropertiesEnchantmentRegistry>>, spell_id: u32) -> bool {
    let Some(value) = value else { return false };

    value
        .iter()
        .any(|e| i64::from(e.spell_id) == i64::from(spell_id))
}

/// The first entry for `spell_id` (from `caster_guid` when given).
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentBySpell
#[must_use]
pub fn get_enchantment_by_spell(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    spell_id: i32,
    caster_guid: Option<u32>,
) -> Option<&PropertiesEnchantmentRegistry> {
    let value = value?;

    value
        .iter()
        .find(|e| e.spell_id == spell_id && caster_guid.is_none_or(|c| e.caster_object_id == c))
}

/// [`get_enchantment_by_spell`] for callers that update the entry in place, as ACE's do through
/// the returned reference.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentBySpell
pub fn get_enchantment_by_spell_mut(
    value: Option<&mut Vec<PropertiesEnchantmentRegistry>>,
    spell_id: i32,
    caster_guid: Option<u32>,
) -> Option<&mut PropertiesEnchantmentRegistry> {
    let value = value?;

    value
        .iter_mut()
        .find(|e| e.spell_id == spell_id && caster_guid.is_none_or(|c| e.caster_object_id == c))
}

/// The first entry for `spell_id` from equipment set `spell_set_id`.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentBySpellSet
#[must_use]
pub fn get_enchantment_by_spell_set(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    spell_id: i32,
    spell_set_id: EquipmentSet,
) -> Option<&PropertiesEnchantmentRegistry> {
    let value = value?;

    value
        .iter()
        .find(|e| e.spell_id == spell_id && e.spell_set_id == spell_set_id)
}

/// Every entry in `spell_category`, in list order; `None` for a null list.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsByCategory
#[must_use]
pub fn get_enchantments_by_category(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    spell_category: SpellCategory,
) -> Option<Vec<&PropertiesEnchantmentRegistry>> {
    let value = value?;

    Some(
        value
            .iter()
            .filter(|e| e.spell_category == spell_category)
            .collect(),
    )
}

/// [`get_enchantments_by_category`] as positions in the list, for callers that update a returned
/// entry in place (ACE's `EnchantmentManager.Add` refreshes the entry it picks from this list).
/// Same filter and order.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsByCategory
#[must_use]
pub fn get_enchantments_by_category_indices(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    spell_category: SpellCategory,
) -> Option<Vec<usize>> {
    let value = value?;

    Some(
        value
            .iter()
            .enumerate()
            .filter(|(_, e)| e.spell_category == spell_category)
            .map(|(i, _)| i)
            .collect(),
    )
}

/// Every entry whose `StatModType` has all of `stat_mod_type`'s bits; `None` for a null list.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsByStatModType
#[must_use]
pub fn get_enchantments_by_stat_mod_type(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    stat_mod_type: EnchantmentTypeFlags,
) -> Option<Vec<&PropertiesEnchantmentRegistry>> {
    let value = value?;

    Some(
        value
            .iter()
            .filter(|e| (e.stat_mod_type & stat_mod_type) == stat_mod_type)
            .collect(),
    )
}

/// The level 8 item self spells ACE ranks above an equal-power level 8 item other spell. Not
/// ACE's (retail): the stacking does not use the list (see [`compare_layers`]); it is kept as the
/// record of ACE's key.
// ACE: PropertiesEnchantmentRegistryExtensions.Level8AuraSelfSpells
pub const LEVEL8_AURA_SELF_SPELLS: [SpellId; 6] = [
    SpellId::BloodDrinkerSelf8,
    SpellId::DefenderSelf8,
    SpellId::HeartSeekerSelf8,
    SpellId::SpiritDrinkerSelf8,
    SpellId::SwiftKillerSelf8,
    SpellId::HermeticLinkSelf8,
];

/// `double.CompareTo`: NaN is below every number and equal to itself.
fn compare_double(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

/// The sort keys, compared lexicographically, both descending: `PowerLevel`, then the start time.
///
/// Not ACE's (retail): ACE put two keys between them, a level 8 aura
/// self spell above an equal-power one ([`LEVEL8_AURA_SELF_SPELLS`]) and a set spell ranked by its
/// spell id instead of its start time; retail's registry has neither, and its server kept every
/// tie in the registry (an aura's Self and Other versions side by side, a set's spells re-added
/// whole on each equip change). This accepts retail's quirk that ACE's set-spell key patched: when
/// two sets each carry a level of one spell at the same power (Gauntlet Damage Boost I and II), the
/// one added later wins, even the lower level.
fn compare_layers(
    a: &PropertiesEnchantmentRegistry,
    b: &PropertiesEnchantmentRegistry,
) -> Ordering {
    a.power_level
        .cmp(&b.power_level)
        .then_with(|| compare_double(a.start_time, b.start_time))
}

/// The additive and multiplicative bits of a `StatMod` type.
const KIND_BITS: EnchantmentTypeFlags =
    EnchantmentTypeFlags(EnchantmentTypeFlags::Additive.0 | EnchantmentTypeFlags::Multiplicative.0);

/// `group e by e.SpellCategory` then the best of each group by [`compare_layers`], descending.
///
/// Retail's duel, not ACE's `OrderByDescending(..).First()`:
/// * one survivor per category across **both** kinds: the multiplicative entries duel first, then
///   the additive ones, and only then are the survivors narrowed to `kind` (an entry with both bits
///   is additive). ACE queried the two kinds separately, so a category holding one of each applied
///   both;
/// * on a full tie the **later** entry wins (ACE kept the first);
/// * no level 8 aura or set-spell keys (see [`compare_layers`]).
fn top_layer<'a>(
    entries: impl Iterator<Item = &'a PropertiesEnchantmentRegistry>,
    kind: EnchantmentTypeFlags,
) -> Vec<&'a PropertiesEnchantmentRegistry> {
    let is_additive = |e: &PropertiesEnchantmentRegistry| {
        (e.stat_mod_type & EnchantmentTypeFlags::Additive).0 != 0
    };
    let entries: Vec<&'a PropertiesEnchantmentRegistry> = entries.collect();
    // The categories keep ACE's order, that of their first appearance; only the winner is retail's.
    let mut groups: Vec<(SpellCategory, &'a PropertiesEnchantmentRegistry)> = Vec::new();
    for &e in &entries {
        if !groups.iter().any(|(k, _)| *k == e.spell_category) {
            groups.push((e.spell_category, e));
        }
    }
    let multiplicative = entries.iter().copied().filter(|e| !is_additive(e));
    for e in multiplicative.chain(entries.iter().copied().filter(|e| is_additive(e))) {
        let (_, best) = groups
            .iter_mut()
            .find(|(k, _)| *k == e.spell_category)
            .expect("every category is listed");
        if !std::ptr::eq(*best, e) && compare_layers(e, best) != Ordering::Less {
            *best = e;
        }
    }
    let wanted = |e: &PropertiesEnchantmentRegistry| {
        if (kind & KIND_BITS).0 == 0 {
            return true;
        }
        let multiplicative =
            (e.stat_mod_type & EnchantmentTypeFlags::Multiplicative).0 != 0 && !is_additive(e);
        ((kind & EnchantmentTypeFlags::Additive).0 != 0 && is_additive(e))
            || ((kind & EnchantmentTypeFlags::Multiplicative).0 != 0 && multiplicative)
    };
    groups
        .into_iter()
        .map(|(_, e)| e)
        .filter(|e| wanted(e))
        .collect()
}

/// A `StatMod` type without its additive and multiplicative bits.
fn without_kind(stat_mod_type: EnchantmentTypeFlags) -> EnchantmentTypeFlags {
    EnchantmentTypeFlags(stat_mod_type.0 & !KIND_BITS.0)
}

/// The top layer in each spell category; `None` for a null list.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsTopLayer
#[must_use]
pub fn get_enchantments_top_layer(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
) -> Option<Vec<&PropertiesEnchantmentRegistry>> {
    let value = value?;

    Some(top_layer(value.iter(), EnchantmentTypeFlags::Undef))
}

/// The top layers in each spell category for a `StatMod` type; `None` for a null list.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsTopLayerByStatModType
#[must_use]
pub fn get_enchantments_top_layer_by_stat_mod_type(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    stat_mod_type: EnchantmentTypeFlags,
) -> Option<Vec<&PropertiesEnchantmentRegistry>> {
    let value = value?;

    // Every kind of the category duels; `top_layer` narrows the survivors to the kind asked.
    let kind = stat_mod_type & KIND_BITS;
    let stat_mod_type = without_kind(stat_mod_type);
    let values_by_stat_mod_type = value
        .iter()
        .filter(|e| (e.stat_mod_type & stat_mod_type) == stat_mod_type);

    Some(top_layer(values_by_stat_mod_type, kind))
}

/// The top layers in each spell category for a `StatMod` type and key (ACE's overload with
/// `statModKey` and `handleMultiple`); `None` for a null list.
// ACE: PropertiesEnchantmentRegistryExtensions.GetEnchantmentsTopLayerByStatModType
#[must_use]
pub fn get_enchantments_top_layer_by_stat_mod_type_and_key(
    value: Option<&Vec<PropertiesEnchantmentRegistry>>,
    mut stat_mod_type: EnchantmentTypeFlags,
    stat_mod_key: u32,
    handle_multiple: bool,
) -> Option<Vec<&PropertiesEnchantmentRegistry>> {
    let value = value?;

    // Every kind of the category duels; `top_layer` narrows the survivors to the kind asked.
    let kind = stat_mod_type & KIND_BITS;
    stat_mod_type = without_kind(stat_mod_type);

    let mut multiple_stat = EnchantmentTypeFlags::Undef;

    if handle_multiple {
        // todo: this is starting to get a bit messy here, EnchantmentTypeFlags handling should be more adaptable
        // perhaps the enchantment registry in acclient should be investigated for reference logic

        multiple_stat = stat_mod_type | EnchantmentTypeFlags::MultipleStat;

        stat_mod_type |= EnchantmentTypeFlags::SingleStat;
    }

    let values_by_stat_mod_type_and_key = value.iter().filter(|e| {
        (e.stat_mod_type & stat_mod_type) == stat_mod_type && e.stat_mod_key == stat_mod_key
            || (handle_multiple
                && (e.stat_mod_type & multiple_stat) == multiple_stat
                && (e.stat_mod_type & EnchantmentTypeFlags::Vitae).0 == 0
                && e.stat_mod_key == 0)
    });

    Some(top_layer(values_by_stat_mod_type_and_key, kind))
}

/// Ticks every entry's `StartTime` down by `heartbeat_interval` and returns copies of the ones
/// that have run out (`StartTime <= -Duration`, for a non-negative `Duration`); `None` for a
/// null list.
// ACE: PropertiesEnchantmentRegistryExtensions.HeartBeatEnchantmentsAndReturnExpired
pub fn heart_beat_enchantments_and_return_expired(
    value: Option<&mut Vec<PropertiesEnchantmentRegistry>>,
    heartbeat_interval: f64,
) -> Option<Vec<PropertiesEnchantmentRegistry>> {
    let value = value?;

    let mut expired = Vec::new();

    for enchantment in value.iter_mut() {
        enchantment.start_time -= heartbeat_interval;

        // StartTime ticks backwards to -Duration
        if enchantment.duration >= 0.0 && enchantment.start_time <= -enchantment.duration {
            expired.push(enchantment.clone());
        }
    }

    Some(expired)
}

/// Appends `entity`. The list must exist, as in ACE.
// ACE: PropertiesEnchantmentRegistryExtensions.AddEnchantment
pub fn add_enchantment(
    value: &mut Vec<PropertiesEnchantmentRegistry>,
    entity: PropertiesEnchantmentRegistry,
) {
    value.push(entity);
}

/// Removes the first entry for `spell_id` cast by `caster_object_id`.
// ACE: PropertiesEnchantmentRegistryExtensions.TryRemoveEnchantment
pub fn try_remove_enchantment(
    value: Option<&mut Vec<PropertiesEnchantmentRegistry>>,
    spell_id: i32,
    caster_object_id: u32,
) -> bool {
    let Some(value) = value else { return false };

    let entity = value
        .iter()
        .position(|x| x.spell_id == spell_id && x.caster_object_id == caster_object_id);

    if let Some(entity) = entity {
        value.remove(entity);

        return true;
    }

    false
}

/// Removes every entry whose spell is not in `spells_to_exclude`, keeping the others in order.
// ACE: PropertiesEnchantmentRegistryExtensions.RemoveAllEnchantments
pub fn remove_all_enchantments(
    value: Option<&mut Vec<PropertiesEnchantmentRegistry>>,
    spells_to_exclude: &[i32],
) {
    let Some(value) = value else { return };

    value.retain(|e| spells_to_exclude.contains(&e.spell_id));
}
