// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Set.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Set.cs`.
//!
//! The item-level members (`ItemLevel`, `HasItemSet`, `HasItemLevel`, `AddItemXP`) and the item
//! set spells (`GetSpellSetAll`, `GetSpellSet`, `ItemSetContains`, over the portal dat's
//! `SpellTable.SpellSet`); the property wrappers are generated in `props/world_object_set.rs`.

use empyrean_common::dotnet::{CsCast, DotNetHashSet};
use empyrean_entity::enums::EquipmentSet;
use empyrean_entity::ObjectGuid;

use crate::entity::experience_system;
use crate::entity::spell::Spell;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `WorldObject_Set.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectSetFields {}

impl WorldObject {
    /// Returns the level for an item: null without `HasItemLevel`.
    ///
    /// # Panics
    /// With `HasItemLevel` but no `ItemTotalXp` (ACE's `InvalidOperationException` on `.Value`).
    // ACE: WorldObject.ItemLevel
    #[must_use]
    pub fn item_level(&self) -> Option<i32> {
        if !self.has_item_level() {
            return None;
        }

        let total_xp = self
            .item_total_xp()
            .expect("ACE: ItemTotalXp.Value (InvalidOperationException)");
        let base_xp = self.item_base_xp().expect("ACE: ItemBaseXp.Value");
        let max_level = self.item_max_level().expect("ACE: ItemMaxLevel.Value");
        let xp_style = self.item_xp_style().expect("ACE: ItemXpStyle.Value");
        // `(ulong)` of a `long`: unchecked, a negative value wraps
        Some(experience_system::item_total_xp_to_level(
            total_xp.cs_cast(),
            base_xp.cs_cast(),
            max_level,
            xp_style,
        ))
    }

    /// Returns TRUE if this item is part of a set.
    // ACE: WorldObject.HasItemSet
    #[must_use]
    pub fn has_item_set(&self) -> bool {
        self.equipment_set_id().is_some()
    }

    /// Returns TRUE if this item can be leveled.
    // ACE: WorldObject.HasItemLevel
    #[must_use]
    pub fn has_item_level(&self) -> bool {
        // seems like ItemMaxLevel would be good enough here,
        // but using the client formula from ItemExamineUI::Appraisal_ShowItemLevelInfo
        self.item_base_xp().is_some_and(|x| x > 0)
            && self.item_max_level().is_some_and(|x| x > 0)
            && self.item_xp_style().is_some_and(|s| s.0 > 0)
    }

    /// Grants XP to an item that can be leveled up, capped at its maximum level's XP; answers the
    /// amount added.
    // ACE: WorldObject.AddItemXP
    pub fn add_item_xp(&mut self, amount: i64) -> i64 {
        if !self.has_item_level() {
            // `Console.WriteLine`
            log::info!(
                "{}.GrantItemXp({amount}): no item level",
                self.get_property(empyrean_entity::enums::PropertyString::Name)
                    .unwrap_or_default()
            );
            return 0;
        }

        let mut item_total_xp = self.item_total_xp().unwrap_or(0);
        let prev_amount = item_total_xp;
        item_total_xp = item_total_xp.wrapping_add(amount);

        let max_level = self.item_max_level().expect("ACE: ItemMaxLevel.Value");
        let base_xp: u64 = self
            .item_base_xp()
            .expect("ACE: ItemBaseXp.Value")
            .cs_cast();
        let xp_style = self.item_xp_style().expect("ACE: ItemXpStyle.Value");
        let max_level_xp: i64 =
            experience_system::item_level_to_total_xp(max_level, base_xp, max_level, xp_style)
                .cs_cast();

        if item_total_xp > max_level_xp {
            item_total_xp = max_level_xp;
        }

        if item_total_xp == prev_amount {
            return 0;
        }

        self.set_item_total_xp(Some(item_total_xp));

        // return amount added
        item_total_xp.wrapping_sub(prev_amount)
    }
}

/// `DatManager.PortalDat.SpellTable.SpellSet.TryGetValue((uint)equipmentSet, out var spellSet)`:
/// the set's tiers, keyed by the total level (the piece count) in ascending order (ACE's
/// `SortedDictionary<uint, SpellSetTiers> SpellSetTiers`).
fn spell_set_tiers(
    w: &World,
    equipment_set: EquipmentSet,
) -> Option<&std::collections::BTreeMap<u32, Vec<u32>>> {
    let key: u32 = equipment_set.0.cs_cast();
    w.dats
        .portal_dat()
        .spell_table()
        .spellsets
        .get(&key)
        .map(|s| &s.tiers)
}

/// Returns all spells from all levels in the item set.
// ACE: WorldObject.GetSpellSetAll
#[must_use]
pub fn get_spell_set_all(w: &World, equipment_set: EquipmentSet) -> Vec<Spell> {
    let mut spells = Vec::new();

    let Some(spell_set_tiers) = spell_set_tiers(w, equipment_set) else {
        return spells;
    };

    let mut spell_ids = DotNetHashSet::new();

    for level in spell_set_tiers.values() {
        for &spell in level {
            spell_ids.insert(spell);
        }
    }

    for &spell_id in spell_ids.iter() {
        spells.push(Spell::new(w, spell_id, false));
    }

    spells
}

/// Returns the item set spells for a particular level: the set of the first item, at the summed
/// item levels of `set_items` plus `level_diff` (or their count, for a set without item levels),
/// capped at the set's highest tier, from the tier at or below that level.
///
/// # Panics
/// When the first item has no `EquipmentSetId` (ACE's `InvalidOperationException` on the
/// nullable cast), or the item levels overflow an `int` (`Enumerable.Sum` is checked).
// ACE: WorldObject.GetSpellSet
#[must_use]
pub fn get_spell_set(w: &World, set_items: &[ObjectGuid], level_diff: i32) -> Vec<Spell> {
    let mut spells = Vec::new();

    let Some(&first_set_item) = set_items.first() else {
        return spells;
    };
    let first_set_item = w
        .objects
        .get(first_set_item)
        .expect("ACE: setItems holds a null item (NullReferenceException)");

    let equipment_set = first_set_item
        .equipment_set_id()
        .expect("ACE: (uint)equipmentSet of null (InvalidOperationException)");
    let item_xp_style = first_set_item.item_xp_style().map_or(0, |s| s.0);

    let Some(spell_set_tiers) = spell_set_tiers(w, equipment_set) else {
        return spells;
    };

    // apply maximum level cap here?
    let mut level: u32 = if item_xp_style > 0 {
        let sum = set_items
            .iter()
            .map(|&i| {
                w.objects
                    .get(i)
                    .expect("ACE: setItems holds a null item")
                    .item_level()
                    .unwrap_or(0)
            })
            .try_fold(0i32, i32::checked_add)
            .expect("ACE: Enumerable.Sum overflowed (OverflowException)");
        sum.wrapping_add(level_diff).cs_cast()
    } else {
        u32::try_from(set_items.len()).unwrap_or(u32::MAX)
    };

    // `SpellSet.HighestTier`: the last key of the tiers, or 0
    let highest_tier = spell_set_tiers.keys().next_back().copied().unwrap_or(0);

    //Console.WriteLine($"Total level: {level}");
    level = level.min(highest_tier);

    // `SpellSetTiersNoGaps.TryGetValue(level)`: each level from 0 to HighestTier holds the tier at
    // or below it, once there is one.
    let Some((_, spell_set_tier)) = spell_set_tiers.range(..=level).next_back() else {
        return spells;
    };

    for &spell_id in spell_set_tier {
        spells.push(Spell::new(w, spell_id, false));
    }

    spells
}

/// Returns TRUE if spell is contained within any tier for this equipment set.
// ACE: WorldObject.ItemSetContains
#[must_use]
pub fn item_set_contains(w: &World, this: ObjectGuid, spell_id: u32) -> bool {
    let o = w
        .objects
        .get(this)
        .expect("ACE: this is null (NullReferenceException)");
    if !o.has_item_set() {
        return false;
    }

    // get all spells from this set - cache?
    let Some(equipment_set) = o.equipment_set_id() else {
        return false;
    };
    let Some(spell_set_tiers) = spell_set_tiers(w, equipment_set) else {
        return false;
    };

    for tier in spell_set_tiers.values() {
        if tier.contains(&spell_id) {
            return true;
        }
    }

    false
}
