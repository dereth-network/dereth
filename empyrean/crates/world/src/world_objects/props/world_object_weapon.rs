// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Weapon.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Weapon.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/WorldObject_Weapon.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    AttackType, CreatureType, DamageType, PropertyBool, PropertyDataId, PropertyFloat, PropertyInt,
    Skill, WeaponType,
};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: WorldObject.WeaponSkill
    pub fn weapon_skill(&self) -> Skill {
        Skill(self.get_property(PropertyInt::WeaponSkill).unwrap_or(0))
    }

    // ACE: WorldObject.WeaponSkill
    pub fn set_weapon_skill(&mut self, value: Skill) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WeaponSkill);
        } else {
            self.set_property(PropertyInt::WeaponSkill, value.0);
        }
    }

    // ACE: WorldObject.W_DamageType
    pub fn w_damage_type(&self) -> DamageType {
        DamageType(self.get_property(PropertyInt::DamageType).unwrap_or(0))
    }

    // ACE: WorldObject.W_DamageType
    pub fn set_w_damage_type(&mut self, value: DamageType) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::DamageType);
        } else {
            self.set_property(PropertyInt::DamageType, value.0);
        }
    }

    // ACE: WorldObject.W_AttackType
    pub fn w_attack_type(&self) -> AttackType {
        AttackType(self.get_property(PropertyInt::AttackType).unwrap_or(0))
    }

    // ACE: WorldObject.W_AttackType
    pub fn set_w_attack_type(&mut self, value: AttackType) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::AttackType);
        } else {
            self.set_property(PropertyInt::AttackType, value.0);
        }
    }

    // ACE: WorldObject.W_WeaponType
    pub fn w_weapon_type(&self) -> WeaponType {
        WeaponType(self.get_property(PropertyInt::WeaponType).unwrap_or(0))
    }

    // ACE: WorldObject.W_WeaponType
    pub fn set_w_weapon_type(&mut self, value: WeaponType) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WeaponType);
        } else {
            self.set_property(PropertyInt::WeaponType, value.0);
        }
    }

    // ACE: WorldObject.AutoWieldLeft
    pub fn auto_wield_left(&self) -> bool {
        self.get_property(PropertyBool::AutowieldLeft)
            .unwrap_or(false)
    }

    // ACE: WorldObject.AutoWieldLeft
    pub fn set_auto_wield_left(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AutowieldLeft);
        } else {
            self.set_property(PropertyBool::AutowieldLeft, value);
        }
    }

    // ACE: WorldObject.IsCleaving
    pub fn is_cleaving(&self) -> bool {
        self.get_property(PropertyInt::Cleaving).is_some()
    }

    // ACE: WorldObject.CleaveTargets
    pub fn cleave_targets(&self) -> i32 {
        if !self.is_cleaving() {
            return 0;
        }

        self.get_property(PropertyInt::Cleaving)
            .expect("InvalidOperationException: Nullable object must have a value.")
            .wrapping_sub(1)
    }

    // ACE: WorldObject.CriticalFrequency
    pub fn critical_frequency(&self) -> Option<f64> {
        self.get_property(PropertyFloat::CriticalFrequency)
    }

    // ACE: WorldObject.CriticalFrequency
    pub fn set_critical_frequency(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::CriticalFrequency),
            Some(v) => self.set_property(PropertyFloat::CriticalFrequency, v),
        }
    }

    // ACE: WorldObject.SlayerCreatureType
    pub fn slayer_creature_type(&self) -> Option<CreatureType> {
        self.get_property(PropertyInt::SlayerCreatureType)
            .map(|v| CreatureType(v.cs_cast()))
    }

    // ACE: WorldObject.SlayerCreatureType
    pub fn set_slayer_creature_type(&mut self, value: Option<CreatureType>) {
        match value {
            None => self.remove_property(PropertyInt::SlayerCreatureType),
            Some(v) => self.set_property(PropertyInt::SlayerCreatureType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.SlayerDamageBonus
    pub fn slayer_damage_bonus(&self) -> Option<f64> {
        self.get_property(PropertyFloat::SlayerDamageBonus)
    }

    // ACE: WorldObject.SlayerDamageBonus
    pub fn set_slayer_damage_bonus(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::SlayerDamageBonus),
            Some(v) => self.set_property(PropertyFloat::SlayerDamageBonus, v),
        }
    }

    // ACE: WorldObject.ResistanceModifierType
    pub fn resistance_modifier_type(&self) -> Option<DamageType> {
        self.get_property(PropertyInt::ResistanceModifierType)
            .map(DamageType)
    }

    // ACE: WorldObject.ResistanceModifierType
    pub fn set_resistance_modifier_type(&mut self, value: Option<DamageType>) {
        match value {
            None => self.remove_property(PropertyInt::ResistanceModifierType),
            Some(v) => self.set_property(PropertyInt::ResistanceModifierType, v.0),
        }
    }

    // ACE: WorldObject.ResistanceModifier
    pub fn resistance_modifier(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistanceModifier)
    }

    // ACE: WorldObject.ResistanceModifier
    pub fn set_resistance_modifier(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistanceModifier),
            Some(v) => self.set_property(PropertyFloat::ResistanceModifier, v),
        }
    }

    // ACE: WorldObject.ResistMagic
    pub fn resist_magic(&self) -> Option<i32> {
        self.get_property(PropertyInt::ResistMagic)
    }

    // ACE: WorldObject.ResistMagic
    pub fn set_resist_magic(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ResistMagic),
            Some(v) => self.set_property(PropertyInt::ResistMagic, v),
        }
    }

    // ACE: WorldObject.IgnoreMagicArmor
    pub fn ignore_magic_armor(&self) -> bool {
        self.get_property(PropertyBool::IgnoreMagicArmor)
            .unwrap_or(false)
    }

    // ACE: WorldObject.IgnoreMagicArmor
    pub fn set_ignore_magic_armor(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IgnoreMagicArmor);
        } else {
            self.set_property(PropertyBool::IgnoreMagicArmor, value);
        }
    }

    // ACE: WorldObject.IgnoreMagicResist
    pub fn ignore_magic_resist(&self) -> bool {
        self.get_property(PropertyBool::IgnoreMagicResist)
            .unwrap_or(false)
    }

    // ACE: WorldObject.IgnoreMagicResist
    pub fn set_ignore_magic_resist(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IgnoreMagicResist);
        } else {
            self.set_property(PropertyBool::IgnoreMagicResist, value);
        }
    }

    // ACE: WorldObject.IgnoreArmor
    pub fn ignore_armor(&self) -> Option<f64> {
        self.get_property(PropertyFloat::IgnoreArmor)
    }

    // ACE: WorldObject.IgnoreArmor
    pub fn set_ignore_armor(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::IgnoreArmor),
            Some(v) => self.set_property(PropertyFloat::IgnoreArmor, v),
        }
    }

    // ACE: WorldObject.IgnoreShield
    pub fn ignore_shield(&self) -> Option<f64> {
        self.get_property(PropertyFloat::IgnoreShield)
    }

    // ACE: WorldObject.IgnoreShield
    pub fn set_ignore_shield(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::IgnoreShield),
            Some(v) => self.set_property(PropertyFloat::IgnoreShield, v),
        }
    }

    // ACE: WorldObject.ProcSpell
    pub fn proc_spell(&self) -> Option<u32> {
        self.get_property(PropertyDataId::ProcSpell)
    }

    // ACE: WorldObject.ProcSpell
    pub fn set_proc_spell(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::ProcSpell),
            Some(v) => self.set_property(PropertyDataId::ProcSpell, v),
        }
    }

    // ACE: WorldObject.ProcSpellRate
    pub fn proc_spell_rate(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ProcSpellRate)
    }

    // ACE: WorldObject.ProcSpellRate
    pub fn set_proc_spell_rate(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ProcSpellRate),
            Some(v) => self.set_property(PropertyFloat::ProcSpellRate, v),
        }
    }

    // ACE: WorldObject.ProcSpellSelfTargeted
    pub fn proc_spell_self_targeted(&self) -> bool {
        self.get_property(PropertyBool::ProcSpellSelfTargeted)
            .unwrap_or(false)
    }

    // ACE: WorldObject.ProcSpellSelfTargeted
    pub fn set_proc_spell_self_targeted(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::ProcSpellSelfTargeted);
        } else {
            self.set_property(PropertyBool::ProcSpellSelfTargeted, value);
        }
    }
}
