// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/BiotaExtensions.cs
//! `BiotaExtensions`: typed property access on a [`Biota`], as inherent methods.
//!
//! ACE has one overload per property type (Bool, DID, Float, IID, Int, Int64, String); here one
//! generic method takes any [`PropertyKey`]. Positions keep their own methods because their
//! signatures differ. ACE's `ReaderWriterLockSlim` parameter is dropped (see
//! [`crate::models`]); every `out` parameter is part of the return value.

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};

use crate::enums::{PositionType, PropertyString, Skill};
use crate::models::i_weenie::PropertyKey;
use crate::models::{Biota, PropertiesPosition, PropertiesSkill};
use crate::position::Position;

impl Biota {
    // =====================================
    // Get
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    /// The value of `property`, or `None` if the bag is null or lacks it.
    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property<K: PropertyKey>(&self, property: K) -> Option<K::Value> {
        let bag = K::bag(self).as_ref()?;

        bag.get(&property).cloned()
    }

    /// The stored record for position `property`.
    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_position(&self, property: PositionType) -> Option<&PropertiesPosition> {
        let bag = self.properties_position.as_ref()?;

        bag.get(&property)
    }

    /// Position `property` as a [`Position`]. A `RelativeDestination` is built as a relative
    /// position (stored as given, not re-homed).
    // ACE: BiotaExtensions.GetPosition
    #[must_use]
    pub fn get_position(&self, property: PositionType) -> Option<Position> {
        let bag = self.properties_position.as_ref()?;

        let value = bag.get(&property)?;
        Some(Position::from_components(
            value.obj_cell_id,
            value.position_x,
            value.position_y,
            value.position_z,
            value.rotation_x,
            value.rotation_y,
            value.rotation_z,
            value.rotation_w,
            property == PositionType::RelativeDestination,
        ))
    }

    // =====================================
    // Set
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    /// Sets `property`, creating the bag if needed. Returns ACE's `out bool changed`: whether the
    /// property was absent or had a different value (a NaN float always counts as changed). An
    /// unchanged value is not written, so its slot is untouched.
    // ACE: BiotaExtensions.SetProperty
    pub fn set_property<K: PropertyKey>(&mut self, property: K, value: K::Value) -> bool {
        let bag = K::bag_mut(self).get_or_insert_with(DotNetDict::new);

        let changed = bag.get(&property).is_none_or(|existing| value != *existing);

        if changed {
            bag.insert(property, value);
        }

        changed
    }

    /// Stores `value` as position `property`, creating the bag if needed.
    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_position(&mut self, property: PositionType, value: PropertiesPosition) {
        let bag = self.properties_position.get_or_insert_with(DotNetDict::new);

        bag.insert(property, value);
    }

    /// Stores `value` as position `property` (cell, origin and rotation), creating the bag if
    /// needed.
    // ACE: BiotaExtensions.SetPosition
    pub fn set_position(&mut self, property: PositionType, value: &Position) {
        let bag = self.properties_position.get_or_insert_with(DotNetDict::new);

        let entity = PropertiesPosition {
            obj_cell_id: value.cell(),
            position_x: value.position_x,
            position_y: value.position_y,
            position_z: value.position_z,
            rotation_w: value.rotation_w,
            rotation_x: value.rotation_x,
            rotation_y: value.rotation_y,
            rotation_z: value.rotation_z,
        };

        bag.insert(property, entity);
    }

    // =====================================
    // Remove
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    /// Removes `property`; false if the bag is null or lacks it.
    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property<K: PropertyKey>(&mut self, property: K) -> bool {
        let Some(bag) = K::bag_mut(self).as_mut() else {
            return false;
        };

        bag.remove(&property).is_some()
    }

    /// Removes position `property`; false if the bag is null or lacks it.
    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_position(&mut self, property: PositionType) -> bool {
        let Some(bag) = self.properties_position.as_mut() else {
            return false;
        };

        bag.remove(&property).is_some()
    }

    // =====================================
    // BiotaPropertiesSpellBook
    // =====================================

    /// A copy of the spell book, in enumeration order; empty for a null one.
    // ACE: BiotaExtensions.CloneSpells
    #[must_use]
    pub fn clone_spells(&self) -> DotNetDict<i32, f32> {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return DotNetDict::new();
        };

        let mut results = DotNetDict::new();

        for (k, v) in spell_book.iter() {
            results.insert(*k, *v);
        }

        results
    }

    // ACE: BiotaExtensions.HasKnownSpell
    #[must_use]
    pub fn has_known_spell(&self) -> bool {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return false;
        };

        !spell_book.is_empty()
    }

    // ACE: BiotaExtensions.GetKnownSpellsIds
    #[must_use]
    pub fn get_known_spells_ids(&self) -> Vec<i32> {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return Vec::new();
        };

        spell_book.keys().copied().collect()
    }

    // ACE: BiotaExtensions.GetKnownSpellsIdsWhere
    pub fn get_known_spells_ids_where(&self, predicate: impl Fn(i32) -> bool) -> Vec<i32> {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return Vec::new();
        };

        spell_book
            .keys()
            .copied()
            .filter(|&k| predicate(k))
            .collect()
    }

    // ACE: BiotaExtensions.GetKnownSpellsProbabilities
    #[must_use]
    pub fn get_known_spells_probabilities(&self) -> Vec<f32> {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return Vec::new();
        };

        spell_book.values().copied().collect()
    }

    // ACE: BiotaExtensions.SpellIsKnown
    #[must_use]
    pub fn spell_is_known(&self, spell: i32) -> bool {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return false;
        };

        spell_book.contains_key(&spell)
    }

    /// The probability of `spell`, adding it with `probability` if absent. Returns
    /// `(probability, spellAdded)`. ACE's default `probability` is 2.0.
    // ACE: BiotaExtensions.GetOrAddKnownSpell
    pub fn get_or_add_known_spell(&mut self, spell: i32, probability: f32) -> (f32, bool) {
        if let Some(&value) = self
            .properties_spell_book
            .as_ref()
            .and_then(|b| b.get(&spell))
        {
            return (value, false);
        }

        let spell_book = self
            .properties_spell_book
            .get_or_insert_with(DotNetDict::new);

        spell_book.insert(spell, probability);

        (probability, true)
    }

    /// The known spells that are in `match_`, in spell-book order.
    // ACE: BiotaExtensions.GetMatchingSpells
    #[must_use]
    pub fn get_matching_spells(&self, match_: &DotNetHashSet<i32>) -> DotNetDict<i32, f32> {
        let Some(spell_book) = self.properties_spell_book.as_ref() else {
            return DotNetDict::new();
        };

        let mut results = DotNetDict::new();

        for (k, v) in spell_book.iter() {
            if match_.contains(k) {
                results.insert(*k, *v);
            }
        }

        results
    }

    // ACE: BiotaExtensions.TryRemoveKnownSpell
    pub fn try_remove_known_spell(&mut self, spell: i32) -> bool {
        let Some(spell_book) = self.properties_spell_book.as_mut() else {
            return false;
        };

        spell_book.remove(&spell).is_some()
    }

    // ACE: BiotaExtensions.ClearSpells
    pub fn clear_spells(&mut self) {
        let Some(spell_book) = self.properties_spell_book.as_mut() else {
            return;
        };

        spell_book.clear();
    }

    // =====================================
    // BiotaPropertiesSkill
    // =====================================

    // ACE: BiotaExtensions.GetSkill
    #[must_use]
    pub fn get_skill(&self, skill: Skill) -> Option<&PropertiesSkill> {
        self.properties_skill.as_ref()?.get(&skill)
    }

    /// The record for `skill`, adding a default one if absent. Returns `(record, skillAdded)`.
    // ACE: BiotaExtensions.GetOrAddSkill
    pub fn get_or_add_skill(&mut self, skill: Skill) -> (&mut PropertiesSkill, bool) {
        let skills = self.properties_skill.get_or_insert_with(DotNetDict::new);

        let skill_added = !skills.contains_key(&skill);

        (
            skills.get_or_insert_with(skill, PropertiesSkill::default),
            skill_added,
        )
    }

    // =====================================
    // HousePermissions
    // =====================================

    /// A copy of the house permissions, in enumeration order; empty for a null dictionary.
    // ACE: BiotaExtensions.CloneHousePermissions
    #[must_use]
    pub fn clone_house_permissions(&self) -> DotNetDict<u32, bool> {
        let Some(house_permissions) = self.house_permissions.as_ref() else {
            return DotNetDict::new();
        };

        house_permissions.iter().map(|(k, v)| (*k, *v)).collect()
    }

    // ACE: BiotaExtensions.HasHouseGuest
    #[must_use]
    pub fn has_house_guest(&self, guest_guid: u32) -> bool {
        let Some(house_permissions) = self.house_permissions.as_ref() else {
            return false;
        };

        house_permissions.contains_key(&guest_guid)
    }

    // ACE: BiotaExtensions.GetHouseGuestStoragePermission
    #[must_use]
    pub fn get_house_guest_storage_permission(&self, guest_guid: u32) -> Option<bool> {
        self.house_permissions.as_ref()?.get(&guest_guid).copied()
    }

    // ACE: BiotaExtensions.AddOrUpdateHouseGuest
    pub fn add_or_update_house_guest(&mut self, guest_guid: u32, storage: bool) {
        let house_permissions = self.house_permissions.get_or_insert_with(DotNetDict::new);

        house_permissions.insert(guest_guid, storage);
    }

    // ACE: BiotaExtensions.RemoveHouseGuest
    pub fn remove_house_guest(&mut self, guest_guid: u32) -> bool {
        let Some(house_permissions) = self.house_permissions.as_mut() else {
            return false;
        };

        house_permissions.remove(&guest_guid).is_some()
    }

    // =====================================
    // Utility
    // =====================================

    /// The `Name` string property.
    ///
    /// # Panics
    /// ACE-BUG: ACE does not check `PropertiesString` for null here and throws a
    /// `NullReferenceException` when the biota has no strings; so does this.
    // ACE: BiotaExtensions.GetName
    #[must_use]
    pub fn get_name(&self) -> Option<String> {
        let strings = self
            .properties_string
            .as_ref()
            .expect("NullReferenceException: Biota.PropertiesString is null");

        strings.get(&PropertyString::Name).cloned()
    }
}
