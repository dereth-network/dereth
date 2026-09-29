// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/WorldObject_Properties.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    ActivationResponse, AmmoType, AppraisalLongDescDecorations, AttunedStatus, BondedStatus,
    CloakStatus, CombatStyle, CombatUse, CoverageMask, CreatureType, EquipMask, GeneratorDestruct,
    GeneratorTimeType, GeneratorType, HeritageGroup, HookGroupType, HouseStatus, HouseType,
    ImbuedEffectType, ItemType, MaterialType, MotionCommand, ParentLocation, Placement,
    PlayerKillerStatus, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyString, RadarBehavior,
    RadarColor, Skill, Sound, SummoningMastery, UiEffects, Usable, WieldRequirement,
};
use empyrean_entity::Position;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: WorldObject.Placement
    pub fn placement(&self) -> Option<Placement> {
        self.get_property(PropertyInt::Placement)
            .map(|v| Placement(v.cs_cast()))
    }

    // ACE: WorldObject.Placement
    pub fn set_placement(&mut self, value: Option<Placement>) {
        match value {
            None => self.remove_property(PropertyInt::Placement),
            Some(v) => self.set_property(PropertyInt::Placement, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.MotionTableId
    pub fn motion_table_id(&self) -> u32 {
        self.get_property(PropertyDataId::MotionTable).unwrap_or(0)
    }

    // ACE: WorldObject.MotionTableId
    pub fn set_motion_table_id(&mut self, value: u32) {
        self.set_property(PropertyDataId::MotionTable, value);
    }

    // ACE: WorldObject.SoundTableId
    pub fn sound_table_id(&self) -> u32 {
        self.get_property(PropertyDataId::SoundTable).unwrap_or(0)
    }

    // ACE: WorldObject.SoundTableId
    pub fn set_sound_table_id(&mut self, value: u32) {
        self.set_property(PropertyDataId::SoundTable, value);
    }

    // ACE: WorldObject.PhysicsTableId
    pub fn physics_table_id(&self) -> u32 {
        self.get_property(PropertyDataId::PhysicsEffectTable)
            .unwrap_or(0)
    }

    // ACE: WorldObject.PhysicsTableId
    pub fn set_physics_table_id(&mut self, value: u32) {
        self.set_property(PropertyDataId::PhysicsEffectTable, value);
    }

    // ACE: WorldObject.SetupTableId
    pub fn setup_table_id(&self) -> u32 {
        self.get_property(PropertyDataId::Setup).unwrap_or(0)
    }

    // ACE: WorldObject.SetupTableId
    pub fn set_setup_table_id(&mut self, value: u32) {
        self.set_property(PropertyDataId::Setup, value);
    }

    // ACE: WorldObject.ObjScale
    pub fn obj_scale(&self) -> Option<f32> {
        self.get_property(PropertyFloat::DefaultScale)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.ObjScale
    pub fn set_obj_scale(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::DefaultScale),
            Some(v) => self.set_property(PropertyFloat::DefaultScale, v.cs_cast()),
        }
    }

    // ACE: WorldObject.Friction
    pub fn friction(&self) -> Option<f32> {
        self.get_property(PropertyFloat::Friction)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.Friction
    pub fn set_friction(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::Friction),
            Some(v) => self.set_property(PropertyFloat::Friction, v.cs_cast()),
        }
    }

    // ACE: WorldObject.Elasticity
    pub fn elasticity(&self) -> Option<f32> {
        self.get_property(PropertyFloat::Elasticity)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.Elasticity
    pub fn set_elasticity(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::Elasticity),
            Some(v) => self.set_property(PropertyFloat::Elasticity, v.cs_cast()),
        }
    }

    // ACE: WorldObject.Translucency
    pub fn translucency(&self) -> Option<f32> {
        self.get_property(PropertyFloat::Translucency)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.Translucency
    pub fn set_translucency(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::Translucency),
            Some(v) => self.set_property(PropertyFloat::Translucency, v.cs_cast()),
        }
    }

    // ACE: WorldObject.DefaultScriptId
    pub fn default_script_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::PhysicsScript)
    }

    // ACE: WorldObject.DefaultScriptId
    pub fn set_default_script_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::PhysicsScript),
            Some(v) => self.set_property(PropertyDataId::PhysicsScript, v),
        }
    }

    // ACE: WorldObject.DefaultScriptIntensity
    pub fn default_script_intensity(&self) -> Option<f32> {
        self.get_property(PropertyFloat::PhysicsScriptIntensity)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.DefaultScriptIntensity
    pub fn set_default_script_intensity(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::PhysicsScriptIntensity),
            Some(v) => self.set_property(PropertyFloat::PhysicsScriptIntensity, v.cs_cast()),
        }
    }

    // ACE: WorldObject.DisplayName
    pub fn display_name(&self) -> Option<String> {
        self.get_property(PropertyString::DisplayName)
    }

    // ACE: WorldObject.DisplayName
    pub fn set_display_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::DisplayName),
            Some(v) => self.set_property(PropertyString::DisplayName, v),
        }
    }

    // ACE: WorldObject.IconId
    pub fn icon_id(&self) -> u32 {
        self.get_property(PropertyDataId::Icon).unwrap_or(0)
    }

    // ACE: WorldObject.IconId
    pub fn set_icon_id(&mut self, value: u32) {
        self.set_property(PropertyDataId::Icon, value);
    }

    // ACE: WorldObject.ItemType
    pub fn item_type(&self) -> ItemType {
        ItemType(
            self.get_property(PropertyInt::ItemType)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: WorldObject.ItemType
    pub fn set_item_type(&mut self, value: ItemType) {
        self.set_property(PropertyInt::ItemType, value.0.cs_cast());
    }

    // ACE: WorldObject.PluralName
    pub fn plural_name(&self) -> Option<String> {
        self.get_property(PropertyString::PluralName)
    }

    // ACE: WorldObject.PluralName
    pub fn set_plural_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::PluralName),
            Some(v) => self.set_property(PropertyString::PluralName, v),
        }
    }

    // ACE: WorldObject.ItemCapacity
    pub fn item_capacity(&self) -> Option<u8> {
        self.get_property(PropertyInt::ItemsCapacity)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.ItemCapacity
    pub fn set_item_capacity(&mut self, value: Option<u8>) {
        match value {
            None => self.remove_property(PropertyInt::ItemsCapacity),
            Some(v) => self.set_property(PropertyInt::ItemsCapacity, v.cs_cast()),
        }
    }

    // ACE: WorldObject.ContainerCapacity
    pub fn container_capacity(&self) -> Option<u8> {
        self.get_property(PropertyInt::ContainersCapacity)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.ContainerCapacity
    pub fn set_container_capacity(&mut self, value: Option<u8>) {
        match value {
            None => self.remove_property(PropertyInt::ContainersCapacity),
            Some(v) => self.set_property(PropertyInt::ContainersCapacity, v.cs_cast()),
        }
    }

    // ACE: WorldObject.AmmoType
    pub fn ammo_type(&self) -> Option<AmmoType> {
        self.get_property(PropertyInt::AmmoType)
            .map(|v| AmmoType(v.cs_cast()))
    }

    // ACE: WorldObject.AmmoType
    pub fn set_ammo_type(&mut self, value: Option<AmmoType>) {
        match value {
            None => self.remove_property(PropertyInt::AmmoType),
            Some(v) => self.set_property(PropertyInt::AmmoType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.Value
    pub fn value(&self) -> Option<i32> {
        self.get_property(PropertyInt::Value)
    }

    // ACE: WorldObject.Value
    pub fn set_value(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Value),
            Some(v) => self.set_property(PropertyInt::Value, v),
        }
    }

    // ACE: WorldObject.IsAffecting
    pub fn is_affecting(&self) -> bool {
        self.get_property(PropertyBool::IsAffecting)
            .unwrap_or(false)
    }

    // ACE: WorldObject.IsAffecting
    pub fn set_is_affecting(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsAffecting);
        } else {
            self.set_property(PropertyBool::IsAffecting, value);
        }
    }

    // ACE: WorldObject.ItemUseable
    pub fn item_useable(&self) -> Option<Usable> {
        self.get_property(PropertyInt::ItemUseable)
            .map(|v| Usable(v.cs_cast()))
    }

    // ACE: WorldObject.ItemUseable
    pub fn set_item_useable(&mut self, value: Option<Usable>) {
        match value {
            None => self.remove_property(PropertyInt::ItemUseable),
            Some(v) => self.set_property(PropertyInt::ItemUseable, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.UseRadius
    pub fn use_radius(&self) -> Option<f32> {
        self.get_property(PropertyFloat::UseRadius)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.UseRadius
    pub fn set_use_radius(&mut self, value: Option<f32>) {
        match value {
            None => self.remove_property(PropertyFloat::UseRadius),
            Some(v) => self.set_property(PropertyFloat::UseRadius, v.cs_cast()),
        }
    }

    // ACE: WorldObject.TargetType
    pub fn target_type(&self) -> Option<ItemType> {
        self.get_property(PropertyInt::TargetType)
            .map(|v| ItemType(v.cs_cast()))
    }

    // ACE: WorldObject.TargetType
    pub fn set_target_type(&mut self, value: Option<ItemType>) {
        match value {
            None => self.remove_property(PropertyInt::TargetType),
            Some(v) => self.set_property(PropertyInt::TargetType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.UiEffects
    pub fn ui_effects(&self) -> Option<UiEffects> {
        self.get_property(PropertyInt::UiEffects)
            .map(|v| UiEffects(v.cs_cast()))
    }

    // ACE: WorldObject.UiEffects
    pub fn set_ui_effects(&mut self, value: Option<UiEffects>) {
        match value {
            None => self.remove_property(PropertyInt::UiEffects),
            Some(v) => self.set_property(PropertyInt::UiEffects, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.CombatUse
    pub fn combat_use(&self) -> Option<CombatUse> {
        self.get_property(PropertyInt::CombatUse)
            .map(|v| CombatUse(v.cs_cast()))
    }

    // ACE: WorldObject.CombatUse
    pub fn set_combat_use(&mut self, value: Option<CombatUse>) {
        match value {
            None => self.remove_property(PropertyInt::CombatUse),
            Some(v) => self.set_property(PropertyInt::CombatUse, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.Damage
    pub fn damage(&self) -> Option<i32> {
        self.get_property(PropertyInt::Damage)
    }

    // ACE: WorldObject.Damage
    pub fn set_damage(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Damage),
            Some(v) => self.set_property(PropertyInt::Damage, v),
        }
    }

    // ACE: WorldObject.DamageMod
    pub fn damage_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::DamageMod)
    }

    // ACE: WorldObject.DamageMod
    pub fn set_damage_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::DamageMod),
            Some(v) => self.set_property(PropertyFloat::DamageMod, v),
        }
    }

    // ACE: WorldObject.DamageVariance
    pub fn damage_variance(&self) -> Option<f64> {
        self.get_property(PropertyFloat::DamageVariance)
    }

    // ACE: WorldObject.DamageVariance
    pub fn set_damage_variance(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::DamageVariance),
            Some(v) => self.set_property(PropertyFloat::DamageVariance, v),
        }
    }

    // ACE: WorldObject.WeaponTime
    pub fn weapon_time(&self) -> Option<i32> {
        self.get_property(PropertyInt::WeaponTime)
    }

    // ACE: WorldObject.WeaponTime
    pub fn set_weapon_time(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WeaponTime),
            Some(v) => self.set_property(PropertyInt::WeaponTime, v),
        }
    }

    // ACE: WorldObject.WeaponDefense
    pub fn weapon_defense(&self) -> Option<f64> {
        self.get_property(PropertyFloat::WeaponDefense)
    }

    // ACE: WorldObject.WeaponDefense
    pub fn set_weapon_defense(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::WeaponDefense),
            Some(v) => self.set_property(PropertyFloat::WeaponDefense, v),
        }
    }

    // ACE: WorldObject.WeaponMissileDefense
    pub fn weapon_missile_defense(&self) -> Option<f64> {
        self.get_property(PropertyFloat::WeaponMissileDefense)
    }

    // ACE: WorldObject.WeaponMissileDefense
    pub fn set_weapon_missile_defense(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::WeaponMissileDefense),
            Some(v) => self.set_property(PropertyFloat::WeaponMissileDefense, v),
        }
    }

    // ACE: WorldObject.WeaponMagicDefense
    pub fn weapon_magic_defense(&self) -> Option<f64> {
        self.get_property(PropertyFloat::WeaponMagicDefense)
    }

    // ACE: WorldObject.WeaponMagicDefense
    pub fn set_weapon_magic_defense(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::WeaponMagicDefense),
            Some(v) => self.set_property(PropertyFloat::WeaponMagicDefense, v),
        }
    }

    // ACE: WorldObject.WeaponOffense
    pub fn weapon_offense(&self) -> Option<f64> {
        self.get_property(PropertyFloat::WeaponOffense)
    }

    // ACE: WorldObject.WeaponOffense
    pub fn set_weapon_offense(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::WeaponOffense),
            Some(v) => self.set_property(PropertyFloat::WeaponOffense, v),
        }
    }

    // ACE: WorldObject.ManaConversionMod
    pub fn mana_conversion_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ManaConversionMod)
    }

    // ACE: WorldObject.ManaConversionMod
    pub fn set_mana_conversion_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ManaConversionMod),
            Some(v) => self.set_property(PropertyFloat::ManaConversionMod, v),
        }
    }

    // ACE: WorldObject.ElementalDamageBonus
    pub fn elemental_damage_bonus(&self) -> Option<i32> {
        self.get_property(PropertyInt::ElementalDamageBonus)
    }

    // ACE: WorldObject.ElementalDamageBonus
    pub fn set_elemental_damage_bonus(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ElementalDamageBonus),
            Some(v) => self.set_property(PropertyInt::ElementalDamageBonus, v),
        }
    }

    // ACE: WorldObject.ElementalDamageMod
    pub fn elemental_damage_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ElementalDamageMod)
    }

    // ACE: WorldObject.ElementalDamageMod
    pub fn set_elemental_damage_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ElementalDamageMod),
            Some(v) => self.set_property(PropertyFloat::ElementalDamageMod, v),
        }
    }

    // ACE: WorldObject.WieldRequirements
    pub fn wield_requirements(&self) -> WieldRequirement {
        WieldRequirement(
            self.get_property(PropertyInt::WieldRequirements)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.WieldRequirements
    pub fn set_wield_requirements(&mut self, value: WieldRequirement) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WieldRequirements);
        } else {
            self.set_property(PropertyInt::WieldRequirements, value.0);
        }
    }

    // ACE: WorldObject.WieldSkillType
    pub fn wield_skill_type(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldSkillType)
    }

    // ACE: WorldObject.WieldSkillType
    pub fn set_wield_skill_type(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldSkillType),
            Some(v) => self.set_property(PropertyInt::WieldSkillType, v),
        }
    }

    // ACE: WorldObject.WieldDifficulty
    pub fn wield_difficulty(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldDifficulty)
    }

    // ACE: WorldObject.WieldDifficulty
    pub fn set_wield_difficulty(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldDifficulty),
            Some(v) => self.set_property(PropertyInt::WieldDifficulty, v),
        }
    }

    // ACE: WorldObject.WieldRequirements2
    pub fn wield_requirements2(&self) -> WieldRequirement {
        WieldRequirement(
            self.get_property(PropertyInt::WieldRequirements2)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.WieldRequirements2
    pub fn set_wield_requirements2(&mut self, value: WieldRequirement) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WieldRequirements2);
        } else {
            self.set_property(PropertyInt::WieldRequirements2, value.0);
        }
    }

    // ACE: WorldObject.WieldSkillType2
    pub fn wield_skill_type2(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldSkillType2)
    }

    // ACE: WorldObject.WieldSkillType2
    pub fn set_wield_skill_type2(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldSkillType2),
            Some(v) => self.set_property(PropertyInt::WieldSkillType2, v),
        }
    }

    // ACE: WorldObject.WieldDifficulty2
    pub fn wield_difficulty2(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldDifficulty2)
    }

    // ACE: WorldObject.WieldDifficulty2
    pub fn set_wield_difficulty2(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldDifficulty2),
            Some(v) => self.set_property(PropertyInt::WieldDifficulty2, v),
        }
    }

    // ACE: WorldObject.WieldRequirements3
    pub fn wield_requirements3(&self) -> WieldRequirement {
        WieldRequirement(
            self.get_property(PropertyInt::WieldRequirements3)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.WieldRequirements3
    pub fn set_wield_requirements3(&mut self, value: WieldRequirement) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WieldRequirements3);
        } else {
            self.set_property(PropertyInt::WieldRequirements3, value.0);
        }
    }

    // ACE: WorldObject.WieldSkillType3
    pub fn wield_skill_type3(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldSkillType3)
    }

    // ACE: WorldObject.WieldSkillType3
    pub fn set_wield_skill_type3(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldSkillType3),
            Some(v) => self.set_property(PropertyInt::WieldSkillType3, v),
        }
    }

    // ACE: WorldObject.WieldDifficulty3
    pub fn wield_difficulty3(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldDifficulty3)
    }

    // ACE: WorldObject.WieldDifficulty3
    pub fn set_wield_difficulty3(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldDifficulty3),
            Some(v) => self.set_property(PropertyInt::WieldDifficulty3, v),
        }
    }

    // ACE: WorldObject.WieldRequirements4
    pub fn wield_requirements4(&self) -> WieldRequirement {
        WieldRequirement(
            self.get_property(PropertyInt::WieldRequirements4)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.WieldRequirements4
    pub fn set_wield_requirements4(&mut self, value: WieldRequirement) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::WieldRequirements4);
        } else {
            self.set_property(PropertyInt::WieldRequirements4, value.0);
        }
    }

    // ACE: WorldObject.WieldSkillType4
    pub fn wield_skill_type4(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldSkillType4)
    }

    // ACE: WorldObject.WieldSkillType4
    pub fn set_wield_skill_type4(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldSkillType4),
            Some(v) => self.set_property(PropertyInt::WieldSkillType4, v),
        }
    }

    // ACE: WorldObject.WieldDifficulty4
    pub fn wield_difficulty4(&self) -> Option<i32> {
        self.get_property(PropertyInt::WieldDifficulty4)
    }

    // ACE: WorldObject.WieldDifficulty4
    pub fn set_wield_difficulty4(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::WieldDifficulty4),
            Some(v) => self.set_property(PropertyInt::WieldDifficulty4, v),
        }
    }

    // ACE: WorldObject.ItemAllegianceRankLimit
    pub fn item_allegiance_rank_limit(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemAllegianceRankLimit)
    }

    // ACE: WorldObject.ItemAllegianceRankLimit
    pub fn set_item_allegiance_rank_limit(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemAllegianceRankLimit),
            Some(v) => self.set_property(PropertyInt::ItemAllegianceRankLimit, v),
        }
    }

    // ACE: WorldObject.Structure
    pub fn structure(&self) -> Option<u16> {
        self.get_property(PropertyInt::Structure)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.Structure
    pub fn set_structure(&mut self, value: Option<u16>) {
        match value {
            None => self.remove_property(PropertyInt::Structure),
            Some(v) => self.set_property(PropertyInt::Structure, v.cs_cast()),
        }
    }

    // ACE: WorldObject.MaxStructure
    pub fn max_structure(&self) -> Option<u16> {
        self.get_property(PropertyInt::MaxStructure)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.MaxStructure
    pub fn set_max_structure(&mut self, value: Option<u16>) {
        match value {
            None => self.remove_property(PropertyInt::MaxStructure),
            Some(v) => self.set_property(PropertyInt::MaxStructure, v.cs_cast()),
        }
    }

    // ACE: WorldObject.StackSize
    pub fn stack_size(&self) -> Option<i32> {
        self.get_property(PropertyInt::StackSize)
    }

    // ACE: WorldObject.StackSize
    pub fn set_stack_size_prop(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::StackSize),
            Some(v) => self.set_property(PropertyInt::StackSize, v),
        }
    }

    // ACE: WorldObject.MaxStackSize
    pub fn max_stack_size(&self) -> Option<u16> {
        self.get_property(PropertyInt::MaxStackSize)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.MaxStackSize
    pub fn set_max_stack_size(&mut self, value: Option<u16>) {
        match value {
            None => self.remove_property(PropertyInt::MaxStackSize),
            Some(v) => self.set_property(PropertyInt::MaxStackSize, v.cs_cast()),
        }
    }

    // ACE: WorldObject.IsSellable
    pub fn is_sellable(&self) -> bool {
        self.get_property(PropertyBool::IsSellable).unwrap_or(true)
    }

    // ACE: WorldObject.IsSellable
    pub fn set_is_sellable(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::IsSellable);
        } else {
            self.set_property(PropertyBool::IsSellable, value);
        }
    }

    // ACE: WorldObject.ContainerId
    pub fn container_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Container)
    }

    // ACE: WorldObject.ContainerId
    pub fn set_container_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Container),
            Some(v) => self.set_property(PropertyInstanceId::Container, v),
        }
    }

    // ACE: WorldObject.WielderId
    pub fn wielder_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Wielder)
    }

    // ACE: WorldObject.WielderId
    pub fn set_wielder_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Wielder),
            Some(v) => self.set_property(PropertyInstanceId::Wielder, v),
        }
    }

    // ACE: WorldObject.ValidLocations
    pub fn valid_locations(&self) -> Option<EquipMask> {
        self.get_property(PropertyInt::ValidLocations)
            .map(|v| EquipMask(v.cs_cast()))
    }

    // ACE: WorldObject.ValidLocations
    pub fn set_valid_locations(&mut self, value: Option<EquipMask>) {
        match value {
            None => self.remove_property(PropertyInt::ValidLocations),
            Some(v) => self.set_property(PropertyInt::ValidLocations, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.CurrentWieldedLocation
    pub fn current_wielded_location(&self) -> Option<EquipMask> {
        self.get_property(PropertyInt::CurrentWieldedLocation)
            .map(|v| EquipMask(v.cs_cast()))
    }

    // ACE: WorldObject.CurrentWieldedLocation
    pub fn set_current_wielded_location(&mut self, value: Option<EquipMask>) {
        match value {
            None => self.remove_property(PropertyInt::CurrentWieldedLocation),
            Some(v) => self.set_property(PropertyInt::CurrentWieldedLocation, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.ClothingPriority
    pub fn clothing_priority(&self) -> Option<CoverageMask> {
        self.get_property(PropertyInt::ClothingPriority)
            .map(|v| CoverageMask(v.cs_cast()))
    }

    // ACE: WorldObject.ClothingPriority
    pub fn set_clothing_priority(&mut self, value: Option<CoverageMask>) {
        match value {
            None => self.remove_property(PropertyInt::ClothingPriority),
            Some(v) => self.set_property(PropertyInt::ClothingPriority, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.TopLayerPriority
    pub fn top_layer_priority(&self) -> Option<bool> {
        self.get_property(PropertyBool::TopLayerPriority)
    }

    // ACE: WorldObject.TopLayerPriority
    pub fn set_top_layer_priority(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::TopLayerPriority),
            Some(v) => self.set_property(PropertyBool::TopLayerPriority, v),
        }
    }

    // ACE: WorldObject.RadarColor
    pub fn radar_color(&self) -> Option<RadarColor> {
        self.get_property(PropertyInt::RadarBlipColor)
            .map(|v| RadarColor(v.cs_cast()))
    }

    // ACE: WorldObject.RadarColor
    pub fn set_radar_color(&mut self, value: Option<RadarColor>) {
        match value {
            None => self.remove_property(PropertyInt::RadarBlipColor),
            Some(v) => self.set_property(PropertyInt::RadarBlipColor, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.RadarBehavior
    pub fn radar_behavior(&self) -> Option<RadarBehavior> {
        self.get_property(PropertyInt::ShowableOnRadar)
            .map(|v| RadarBehavior(v.cs_cast()))
    }

    // ACE: WorldObject.RadarBehavior
    pub fn set_radar_behavior(&mut self, value: Option<RadarBehavior>) {
        match value {
            None => self.remove_property(PropertyInt::ShowableOnRadar),
            Some(v) => self.set_property(PropertyInt::ShowableOnRadar, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.Script
    pub fn script(&self) -> Option<u16> {
        self.get_property(PropertyDataId::PhysicsScript)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.Script
    pub fn set_script(&mut self, value: Option<u16>) {
        match value {
            None => self.remove_property(PropertyDataId::PhysicsScript),
            Some(v) => self.set_property(PropertyDataId::PhysicsScript, v.cs_cast()),
        }
    }

    // ACE: WorldObject.ItemWorkmanship
    pub fn item_workmanship(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemWorkmanship)
    }

    // ACE: WorldObject.ItemWorkmanship
    pub fn set_item_workmanship(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemWorkmanship),
            Some(v) => self.set_property(PropertyInt::ItemWorkmanship, v),
        }
    }

    // ACE: WorldObject.NumItemsInMaterial
    pub fn num_items_in_material(&self) -> Option<i32> {
        self.get_property(PropertyInt::NumItemsInMaterial)
    }

    // ACE: WorldObject.NumItemsInMaterial
    pub fn set_num_items_in_material(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::NumItemsInMaterial),
            Some(v) => self.set_property(PropertyInt::NumItemsInMaterial, v),
        }
    }

    // ACE: WorldObject.AppraisalLongDescDecoration
    pub fn appraisal_long_desc_decoration(&self) -> Option<AppraisalLongDescDecorations> {
        self.get_property(PropertyInt::AppraisalLongDescDecoration)
            .map(AppraisalLongDescDecorations)
    }

    // ACE: WorldObject.AppraisalLongDescDecoration
    pub fn set_appraisal_long_desc_decoration(
        &mut self,
        value: Option<AppraisalLongDescDecorations>,
    ) {
        match value {
            None => self.remove_property(PropertyInt::AppraisalLongDescDecoration),
            Some(v) => self.set_property(PropertyInt::AppraisalLongDescDecoration, v.0),
        }
    }

    // ACE: WorldObject.HouseId
    pub fn house_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::HouseId)
    }

    // ACE: WorldObject.HouseId
    pub fn set_house_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::HouseId),
            Some(v) => self.set_property(PropertyDataId::HouseId, v),
        }
    }

    // ACE: WorldObject.HouseInstance
    pub fn house_instance(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::House)
    }

    // ACE: WorldObject.HouseInstance
    pub fn set_house_instance(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::House),
            Some(v) => self.set_property(PropertyInstanceId::House, v),
        }
    }

    // ACE: WorldObject.HouseOwner
    pub fn house_owner(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::HouseOwner)
    }

    // ACE: WorldObject.HouseOwner
    pub fn set_house_owner_prop(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::HouseOwner),
            Some(v) => self.set_property(PropertyInstanceId::HouseOwner, v),
        }
    }

    // ACE: WorldObject.HouseOwnerName
    pub fn house_owner_name(&self) -> Option<String> {
        self.get_property(PropertyString::HouseOwnerName)
    }

    // ACE: WorldObject.HouseOwnerName
    pub fn set_house_owner_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::HouseOwnerName),
            Some(v) => self.set_property(PropertyString::HouseOwnerName, v),
        }
    }

    // ACE: WorldObject.HouseStatus
    pub fn house_status(&self) -> HouseStatus {
        self.get_property(PropertyInt::HouseStatus)
            .map(HouseStatus)
            .unwrap_or(HouseStatus::Active)
    }

    // ACE: WorldObject.HouseStatus
    pub fn set_house_status(&mut self, value: HouseStatus) {
        if value == HouseStatus::Active {
            self.remove_property(PropertyInt::HouseStatus);
        } else {
            self.set_property(PropertyInt::HouseStatus, value.0);
        }
    }

    // ACE: WorldObject.HouseType
    pub fn house_type(&self) -> HouseType {
        self.get_property(PropertyInt::HouseType)
            .map(HouseType)
            .unwrap_or(HouseType::Undef)
    }

    // ACE: WorldObject.HouseType
    pub fn set_house_type(&mut self, value: HouseType) {
        if value == HouseType::Undef {
            self.remove_property(PropertyInt::HouseType);
        } else {
            self.set_property(PropertyInt::HouseType, value.0);
        }
    }

    // ACE: WorldObject.HookItemType
    pub fn hook_item_type(&self) -> Option<i32> {
        self.get_property(PropertyInt::HookItemType)
    }

    // ACE: WorldObject.HookItemType
    pub fn set_hook_item_type(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HookItemType),
            Some(v) => self.set_property(PropertyInt::HookItemType, v),
        }
    }

    // ACE: WorldObject.HookPlacement
    pub fn hook_placement(&self) -> Option<i32> {
        self.get_property(PropertyInt::HookPlacement)
    }

    // ACE: WorldObject.HookPlacement
    pub fn set_hook_placement(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HookPlacement),
            Some(v) => self.set_property(PropertyInt::HookPlacement, v),
        }
    }

    // ACE: WorldObject.MonarchId
    pub fn monarch_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Monarch)
    }

    // ACE: WorldObject.MonarchId
    pub fn set_monarch_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Monarch),
            Some(v) => self.set_property(PropertyInstanceId::Monarch, v),
        }
    }

    // ACE: WorldObject.PatronId
    pub fn patron_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Patron)
    }

    // ACE: WorldObject.PatronId
    pub fn set_patron_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Patron),
            Some(v) => self.set_property(PropertyInstanceId::Patron, v),
        }
    }

    // ACE: WorldObject.HookType
    pub fn hook_type(&self) -> Option<u16> {
        self.get_property(PropertyInt::HookType)
            .map(|v| v.cs_cast())
    }

    // ACE: WorldObject.HookType
    pub fn set_hook_type(&mut self, value: Option<u16>) {
        match value {
            None => self.remove_property(PropertyInt::HookType),
            Some(v) => self.set_property(PropertyInt::HookType, v.cs_cast()),
        }
    }

    // ACE: WorldObject.IconOverlayId
    pub fn icon_overlay_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::IconOverlay)
    }

    // ACE: WorldObject.IconOverlayId
    pub fn set_icon_overlay_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::IconOverlay),
            Some(v) => self.set_property(PropertyDataId::IconOverlay, v),
        }
    }

    // ACE: WorldObject.IconOverlaySecondary
    pub fn icon_overlay_secondary(&self) -> Option<u32> {
        self.get_property(PropertyDataId::IconOverlaySecondary)
    }

    // ACE: WorldObject.IconOverlaySecondary
    pub fn set_icon_overlay_secondary(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::IconOverlaySecondary),
            Some(v) => self.set_property(PropertyDataId::IconOverlaySecondary, v),
        }
    }

    // ACE: WorldObject.MaterialType
    pub fn material_type(&self) -> Option<MaterialType> {
        self.get_property(PropertyInt::MaterialType)
            .map(|v| MaterialType(v.cs_cast()))
    }

    // ACE: WorldObject.MaterialType
    pub fn set_material_type(&mut self, value: Option<MaterialType>) {
        match value {
            None => self.remove_property(PropertyInt::MaterialType),
            Some(v) => self.set_property(PropertyInt::MaterialType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.GemType
    pub fn gem_type(&self) -> Option<MaterialType> {
        self.get_property(PropertyInt::GemType)
            .map(|v| MaterialType(v.cs_cast()))
    }

    // ACE: WorldObject.GemType
    pub fn set_gem_type(&mut self, value: Option<MaterialType>) {
        match value {
            None => self.remove_property(PropertyInt::GemType),
            Some(v) => self.set_property(PropertyInt::GemType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.GemCount
    pub fn gem_count(&self) -> Option<i32> {
        self.get_property(PropertyInt::GemCount)
    }

    // ACE: WorldObject.GemCount
    pub fn set_gem_count(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GemCount),
            Some(v) => self.set_property(PropertyInt::GemCount, v),
        }
    }

    // ACE: WorldObject.Attuned
    pub fn attuned(&self) -> Option<AttunedStatus> {
        self.get_property(PropertyInt::Attuned).map(AttunedStatus)
    }

    // ACE: WorldObject.Attuned
    pub fn set_attuned(&mut self, value: Option<AttunedStatus>) {
        match value {
            None => self.remove_property(PropertyInt::Attuned),
            Some(v) => self.set_property(PropertyInt::Attuned, v.0),
        }
    }

    // ACE: WorldObject.Bonded
    pub fn bonded(&self) -> Option<BondedStatus> {
        self.get_property(PropertyInt::Bonded).map(BondedStatus)
    }

    // ACE: WorldObject.Bonded
    pub fn set_bonded(&mut self, value: Option<BondedStatus>) {
        match value {
            None => self.remove_property(PropertyInt::Bonded),
            Some(v) => self.set_property(PropertyInt::Bonded, v.0),
        }
    }

    // ACE: WorldObject.IsOpen
    pub fn is_open(&self) -> bool {
        self.get_property(PropertyBool::Open).unwrap_or(false)
    }

    // ACE: WorldObject.IsOpen
    pub fn set_is_open(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Open);
        } else {
            self.set_property(PropertyBool::Open, value);
        }
    }

    // ACE: WorldObject.IconUnderlayId
    pub fn icon_underlay_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::IconUnderlay)
    }

    // ACE: WorldObject.IconUnderlayId
    pub fn set_icon_underlay_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::IconUnderlay),
            Some(v) => self.set_property(PropertyDataId::IconUnderlay, v),
        }
    }

    // ACE: WorldObject.CooldownId
    pub fn cooldown_id(&self) -> Option<i32> {
        self.get_property(PropertyInt::SharedCooldown)
    }

    // ACE: WorldObject.CooldownId
    pub fn set_cooldown_id(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::SharedCooldown),
            Some(v) => self.set_property(PropertyInt::SharedCooldown, v),
        }
    }

    // ACE: WorldObject.CooldownDuration
    pub fn cooldown_duration(&self) -> Option<f64> {
        self.get_property(PropertyFloat::CooldownDuration)
    }

    // ACE: WorldObject.CooldownDuration
    pub fn set_cooldown_duration(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::CooldownDuration),
            Some(v) => self.set_property(PropertyFloat::CooldownDuration, v),
        }
    }

    // ACE: WorldObject.PetOwner
    pub fn pet_owner(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::PetOwner)
    }

    // ACE: WorldObject.PetOwner
    pub fn set_pet_owner(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::PetOwner),
            Some(v) => self.set_property(PropertyInstanceId::PetOwner, v),
        }
    }

    // ACE: WorldObject.IsLocked
    pub fn is_locked(&self) -> bool {
        self.get_property(PropertyBool::Locked).unwrap_or(false)
    }

    // ACE: WorldObject.IsLocked
    pub fn set_is_locked(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Locked);
        } else {
            self.set_property(PropertyBool::Locked, value);
        }
    }

    // ACE: WorldObject.Inscribable
    pub fn inscribable(&self) -> bool {
        self.get_property(PropertyBool::Inscribable)
            .unwrap_or(false)
    }

    // ACE: WorldObject.Inscribable
    pub fn set_inscribable(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Inscribable);
        } else {
            self.set_property(PropertyBool::Inscribable, value);
        }
    }

    // ACE: WorldObject.Stuck
    pub fn stuck(&self) -> bool {
        self.get_property(PropertyBool::Stuck).unwrap_or(false)
    }

    // ACE: WorldObject.Stuck
    pub fn set_stuck(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Stuck);
        } else {
            self.set_property(PropertyBool::Stuck, value);
        }
    }

    // ACE: WorldObject.Attackable
    pub fn attackable(&self) -> bool {
        self.get_property(PropertyBool::Attackable).unwrap_or(true)
    }

    // ACE: WorldObject.Attackable
    pub fn set_attackable(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::Attackable);
        } else {
            self.set_property(PropertyBool::Attackable, value);
        }
    }

    // ACE: WorldObject.HiddenAdmin
    pub fn hidden_admin(&self) -> bool {
        self.get_property(PropertyBool::HiddenAdmin)
            .unwrap_or(false)
    }

    // ACE: WorldObject.HiddenAdmin
    pub fn set_hidden_admin(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::HiddenAdmin);
        } else {
            self.set_property(PropertyBool::HiddenAdmin, value);
        }
    }

    // ACE: WorldObject.UiHidden
    pub fn ui_hidden(&self) -> bool {
        self.get_property(PropertyBool::UiHidden).unwrap_or(false)
    }

    // ACE: WorldObject.UiHidden
    pub fn set_ui_hidden(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::UiHidden);
        } else {
            self.set_property(PropertyBool::UiHidden, value);
        }
    }

    // ACE: WorldObject.IgnoreHouseBarriers
    pub fn ignore_house_barriers(&self) -> bool {
        self.get_property(PropertyBool::IgnoreHouseBarriers)
            .unwrap_or(false)
    }

    // ACE: WorldObject.IgnoreHouseBarriers
    pub fn set_ignore_house_barriers(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IgnoreHouseBarriers);
        } else {
            self.set_property(PropertyBool::IgnoreHouseBarriers, value);
        }
    }

    // ACE: WorldObject.RequiresPackSlot
    pub fn requires_pack_slot(&self) -> bool {
        self.get_property(PropertyBool::RequiresBackpackSlot)
            .unwrap_or(false)
    }

    // ACE: WorldObject.RequiresPackSlot
    pub fn set_requires_pack_slot(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::RequiresBackpackSlot);
        } else {
            self.set_property(PropertyBool::RequiresBackpackSlot, value);
        }
    }

    // ACE: WorldObject.Retained
    pub fn retained(&self) -> bool {
        self.get_property(PropertyBool::Retained).unwrap_or(false)
    }

    // ACE: WorldObject.Retained
    pub fn set_retained(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Retained);
        } else {
            self.set_property(PropertyBool::Retained, value);
        }
    }

    // ACE: WorldObject.WieldOnUse
    pub fn wield_on_use(&self) -> bool {
        self.get_property(PropertyBool::WieldOnUse).unwrap_or(false)
    }

    // ACE: WorldObject.WieldOnUse
    pub fn set_wield_on_use(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::WieldOnUse);
        } else {
            self.set_property(PropertyBool::WieldOnUse, value);
        }
    }

    // ACE: WorldObject.WieldLeft
    pub fn wield_left(&self) -> bool {
        self.get_property(PropertyBool::AutowieldLeft)
            .unwrap_or(false)
    }

    // ACE: WorldObject.WieldLeft
    pub fn set_wield_left(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AutowieldLeft);
        } else {
            self.set_property(PropertyBool::AutowieldLeft, value);
        }
    }

    // ACE: WorldObject.Heritage
    pub fn heritage(&self) -> Option<i32> {
        self.get_property(PropertyInt::HeritageGroup)
    }

    // ACE: WorldObject.Heritage
    pub fn set_heritage(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HeritageGroup),
            Some(v) => self.set_property(PropertyInt::HeritageGroup, v),
        }
    }

    // ACE: WorldObject.Gender
    pub fn gender(&self) -> Option<i32> {
        self.get_property(PropertyInt::Gender)
    }

    // ACE: WorldObject.Gender
    pub fn set_gender(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Gender),
            Some(v) => self.set_property(PropertyInt::Gender, v),
        }
    }

    // ACE: WorldObject.HeritageGroup
    pub fn heritage_group(&self) -> HeritageGroup {
        HeritageGroup(self.get_property(PropertyInt::HeritageGroup).unwrap_or(0))
    }

    // ACE: WorldObject.HeritageGroup
    pub fn set_heritage_group(&mut self, value: HeritageGroup) {
        if value == HeritageGroup::Invalid {
            self.remove_property(PropertyInt::HeritageGroup);
        } else {
            self.set_property(PropertyInt::HeritageGroup, value.0);
        }
    }

    // ACE: WorldObject.HeritageGroupName
    pub fn heritage_group_name(&self) -> Option<String> {
        self.get_property(PropertyString::HeritageGroup)
    }

    // ACE: WorldObject.HeritageGroupName
    pub fn set_heritage_group_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::HeritageGroup),
            Some(v) => self.set_property(PropertyString::HeritageGroup, v),
        }
    }

    // ACE: WorldObject.Sex
    pub fn sex(&self) -> Option<String> {
        self.get_property(PropertyString::Sex)
    }

    // ACE: WorldObject.Sex
    pub fn set_sex(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::Sex),
            Some(v) => self.set_property(PropertyString::Sex, v),
        }
    }

    // ACE: WorldObject.HeadObjectDID
    pub fn head_object_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::HeadObject)
    }

    // ACE: WorldObject.HeadObjectDID
    pub fn set_head_object_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::HeadObject),
            Some(v) => self.set_property(PropertyDataId::HeadObject, v),
        }
    }

    // ACE: WorldObject.HairPaletteDID
    pub fn hair_palette_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::HairPalette)
    }

    // ACE: WorldObject.HairPaletteDID
    pub fn set_hair_palette_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::HairPalette),
            Some(v) => self.set_property(PropertyDataId::HairPalette, v),
        }
    }

    // ACE: WorldObject.SkinPaletteDID
    pub fn skin_palette_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::SkinPalette)
    }

    // ACE: WorldObject.SkinPaletteDID
    pub fn set_skin_palette_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::SkinPalette),
            Some(v) => self.set_property(PropertyDataId::SkinPalette, v),
        }
    }

    // ACE: WorldObject.EyesPaletteDID
    pub fn eyes_palette_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::EyesPalette)
    }

    // ACE: WorldObject.EyesPaletteDID
    pub fn set_eyes_palette_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::EyesPalette),
            Some(v) => self.set_property(PropertyDataId::EyesPalette, v),
        }
    }

    // ACE: WorldObject.EyesTextureDID
    pub fn eyes_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::EyesTexture)
    }

    // ACE: WorldObject.EyesTextureDID
    pub fn set_eyes_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::EyesTexture),
            Some(v) => self.set_property(PropertyDataId::EyesTexture, v),
        }
    }

    // ACE: WorldObject.DefaultEyesTextureDID
    pub fn default_eyes_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::DefaultEyesTexture)
    }

    // ACE: WorldObject.DefaultEyesTextureDID
    pub fn set_default_eyes_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::DefaultEyesTexture),
            Some(v) => self.set_property(PropertyDataId::DefaultEyesTexture, v),
        }
    }

    // ACE: WorldObject.NoseTextureDID
    pub fn nose_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::NoseTexture)
    }

    // ACE: WorldObject.NoseTextureDID
    pub fn set_nose_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::NoseTexture),
            Some(v) => self.set_property(PropertyDataId::NoseTexture, v),
        }
    }

    // ACE: WorldObject.DefaultNoseTextureDID
    pub fn default_nose_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::DefaultNoseTexture)
    }

    // ACE: WorldObject.DefaultNoseTextureDID
    pub fn set_default_nose_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::DefaultNoseTexture),
            Some(v) => self.set_property(PropertyDataId::DefaultNoseTexture, v),
        }
    }

    // ACE: WorldObject.MouthTextureDID
    pub fn mouth_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::MouthTexture)
    }

    // ACE: WorldObject.MouthTextureDID
    pub fn set_mouth_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::MouthTexture),
            Some(v) => self.set_property(PropertyDataId::MouthTexture, v),
        }
    }

    // ACE: WorldObject.DefaultMouthTextureDID
    pub fn default_mouth_texture_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::DefaultMouthTexture)
    }

    // ACE: WorldObject.DefaultMouthTextureDID
    pub fn set_default_mouth_texture_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::DefaultMouthTexture),
            Some(v) => self.set_property(PropertyDataId::DefaultMouthTexture, v),
        }
    }

    // ACE: WorldObject.PaletteBaseDID
    pub fn palette_base_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::PaletteBase)
    }

    // ACE: WorldObject.PaletteBaseDID
    pub fn set_palette_base_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::PaletteBase),
            Some(v) => self.set_property(PropertyDataId::PaletteBase, v),
        }
    }

    // ACE: WorldObject.HairStyle
    pub fn hair_style(&self) -> Option<i32> {
        self.get_property(PropertyInt::Hairstyle)
    }

    // ACE: WorldObject.HairStyle
    pub fn set_hair_style(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Hairstyle),
            Some(v) => self.set_property(PropertyInt::Hairstyle, v),
        }
    }

    // ACE: WorldObject.Level
    pub fn level(&self) -> Option<i32> {
        self.get_property(PropertyInt::Level)
    }

    // ACE: WorldObject.Level
    pub fn set_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Level),
            Some(v) => self.set_property(PropertyInt::Level, v),
        }
    }

    // ACE: WorldObject.UseRequiresLevel
    pub fn use_requires_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseRequiresLevel)
    }

    // ACE: WorldObject.UseRequiresLevel
    pub fn set_use_requires_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseRequiresLevel),
            Some(v) => self.set_property(PropertyInt::UseRequiresLevel, v),
        }
    }

    // ACE: WorldObject.UseRequiresSkill
    pub fn use_requires_skill(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseRequiresSkill)
    }

    // ACE: WorldObject.UseRequiresSkill
    pub fn set_use_requires_skill(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseRequiresSkill),
            Some(v) => self.set_property(PropertyInt::UseRequiresSkill, v),
        }
    }

    // ACE: WorldObject.UseRequiresSkillLevel
    pub fn use_requires_skill_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseRequiresSkillLevel)
    }

    // ACE: WorldObject.UseRequiresSkillLevel
    pub fn set_use_requires_skill_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseRequiresSkillLevel),
            Some(v) => self.set_property(PropertyInt::UseRequiresSkillLevel, v),
        }
    }

    // ACE: WorldObject.UseRequiresSkillSpec
    pub fn use_requires_skill_spec(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseRequiresSkillSpec)
    }

    // ACE: WorldObject.UseRequiresSkillSpec
    pub fn set_use_requires_skill_spec(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseRequiresSkillSpec),
            Some(v) => self.set_property(PropertyInt::UseRequiresSkillSpec, v),
        }
    }

    // ACE: WorldObject.ArmorModVsSlash
    pub fn armor_mod_vs_slash(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsSlash)
    }

    // ACE: WorldObject.ArmorModVsSlash
    pub fn set_armor_mod_vs_slash(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsSlash),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsSlash, v),
        }
    }

    // ACE: WorldObject.ArmorModVsPierce
    pub fn armor_mod_vs_pierce(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsPierce)
    }

    // ACE: WorldObject.ArmorModVsPierce
    pub fn set_armor_mod_vs_pierce(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsPierce),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsPierce, v),
        }
    }

    // ACE: WorldObject.ArmorModVsBludgeon
    pub fn armor_mod_vs_bludgeon(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsBludgeon)
    }

    // ACE: WorldObject.ArmorModVsBludgeon
    pub fn set_armor_mod_vs_bludgeon(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsBludgeon),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsBludgeon, v),
        }
    }

    // ACE: WorldObject.ArmorModVsCold
    pub fn armor_mod_vs_cold(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsCold)
    }

    // ACE: WorldObject.ArmorModVsCold
    pub fn set_armor_mod_vs_cold(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsCold),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsCold, v),
        }
    }

    // ACE: WorldObject.ArmorModVsFire
    pub fn armor_mod_vs_fire(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsFire)
    }

    // ACE: WorldObject.ArmorModVsFire
    pub fn set_armor_mod_vs_fire(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsFire),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsFire, v),
        }
    }

    // ACE: WorldObject.ArmorModVsAcid
    pub fn armor_mod_vs_acid(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsAcid)
    }

    // ACE: WorldObject.ArmorModVsAcid
    pub fn set_armor_mod_vs_acid(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsAcid),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsAcid, v),
        }
    }

    // ACE: WorldObject.ArmorModVsElectric
    pub fn armor_mod_vs_electric(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsElectric)
    }

    // ACE: WorldObject.ArmorModVsElectric
    pub fn set_armor_mod_vs_electric(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsElectric),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsElectric, v),
        }
    }

    // ACE: WorldObject.ArmorModVsNether
    pub fn armor_mod_vs_nether(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ArmorModVsNether)
    }

    // ACE: WorldObject.ArmorModVsNether
    pub fn set_armor_mod_vs_nether(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ArmorModVsNether),
            Some(v) => self.set_property(PropertyFloat::ArmorModVsNether, v),
        }
    }

    // ACE: WorldObject.AbsorbMagicDamage
    pub fn absorb_magic_damage(&self) -> Option<f64> {
        self.get_property(PropertyFloat::AbsorbMagicDamage)
    }

    // ACE: WorldObject.AbsorbMagicDamage
    pub fn set_absorb_magic_damage(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::AbsorbMagicDamage),
            Some(v) => self.set_property(PropertyFloat::AbsorbMagicDamage, v),
        }
    }

    // ACE: WorldObject.ArmorType
    pub fn armor_type(&self) -> Option<i32> {
        self.get_property(PropertyInt::ArmorType)
    }

    // ACE: WorldObject.ArmorType
    pub fn set_armor_type(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ArmorType),
            Some(v) => self.set_property(PropertyInt::ArmorType, v),
        }
    }

    // ACE: WorldObject.ArmorLevel
    pub fn armor_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::ArmorLevel)
    }

    // ACE: WorldObject.ArmorLevel
    pub fn set_armor_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ArmorLevel),
            Some(v) => self.set_property(PropertyInt::ArmorLevel, v),
        }
    }

    // ACE: WorldObject.CombatTableDID
    pub fn combat_table_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::CombatTable)
    }

    // ACE: WorldObject.CombatTableDID
    pub fn set_combat_table_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::CombatTable),
            Some(v) => self.set_property(PropertyDataId::CombatTable, v),
        }
    }

    // ACE: WorldObject.UseCreateContractId
    pub fn use_create_contract_id(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseCreatesContractId)
    }

    // ACE: WorldObject.UseCreateContractId
    pub fn set_use_create_contract_id(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseCreatesContractId),
            Some(v) => self.set_property(PropertyInt::UseCreatesContractId, v),
        }
    }

    // ACE: WorldObject.CreationTimestamp
    pub fn creation_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::CreationTimestamp)
    }

    // ACE: WorldObject.CreationTimestamp
    pub fn set_creation_timestamp(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CreationTimestamp),
            Some(v) => self.set_property(PropertyInt::CreationTimestamp, v),
        }
    }

    // ACE: WorldObject.ReleasedTimestamp
    pub fn released_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ReleasedTimestamp)
    }

    // ACE: WorldObject.ReleasedTimestamp
    pub fn set_released_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ReleasedTimestamp),
            Some(v) => self.set_property(PropertyFloat::ReleasedTimestamp, v),
        }
    }

    // ACE: WorldObject.CheckpointTimestamp
    pub fn checkpoint_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::CheckpointTimestamp)
    }

    // ACE: WorldObject.CheckpointTimestamp
    pub fn set_checkpoint_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::CheckpointTimestamp),
            Some(v) => self.set_property(PropertyFloat::CheckpointTimestamp, v),
        }
    }

    // ACE: WorldObject.PlacementPosition
    pub fn placement_position(&self) -> Option<i32> {
        self.get_property(PropertyInt::PlacementPosition)
    }

    // ACE: WorldObject.PlacementPosition
    pub fn set_placement_position(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PlacementPosition),
            Some(v) => self.set_property(PropertyInt::PlacementPosition, v),
        }
    }

    // ACE: WorldObject.ScribeName
    pub fn scribe_name(&self) -> Option<String> {
        self.get_property(PropertyString::ScribeName)
    }

    // ACE: WorldObject.ScribeName
    pub fn set_scribe_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::ScribeName),
            Some(v) => self.set_property(PropertyString::ScribeName, v),
        }
    }

    // ACE: WorldObject.ScribeAccount
    pub fn scribe_account(&self) -> Option<String> {
        self.get_property(PropertyString::ScribeAccount)
    }

    // ACE: WorldObject.ScribeAccount
    pub fn set_scribe_account(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::ScribeAccount),
            Some(v) => self.set_property(PropertyString::ScribeAccount, v),
        }
    }

    // ACE: WorldObject.ScribeIID
    pub fn scribe_iid(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Scribe)
    }

    // ACE: WorldObject.ScribeIID
    pub fn set_scribe_iid(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Scribe),
            Some(v) => self.set_property(PropertyInstanceId::Scribe, v),
        }
    }

    // ACE: WorldObject.AppraisalPages
    pub fn appraisal_pages(&self) -> Option<i32> {
        self.get_property(PropertyInt::AppraisalPages)
    }

    // ACE: WorldObject.AppraisalPages
    pub fn set_appraisal_pages(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AppraisalPages),
            Some(v) => self.set_property(PropertyInt::AppraisalPages, v),
        }
    }

    // ACE: WorldObject.AppraisalMaxPages
    pub fn appraisal_max_pages(&self) -> Option<i32> {
        self.get_property(PropertyInt::AppraisalMaxPages)
    }

    // ACE: WorldObject.AppraisalMaxPages
    pub fn set_appraisal_max_pages(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AppraisalMaxPages),
            Some(v) => self.set_property(PropertyInt::AppraisalMaxPages, v),
        }
    }

    // ACE: WorldObject.Inscription
    pub fn inscription(&self) -> Option<String> {
        self.get_property(PropertyString::Inscription)
    }

    // ACE: WorldObject.Inscription
    pub fn set_inscription(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::Inscription),
            Some(v) => self.set_property(PropertyString::Inscription, v),
        }
    }

    // ACE: WorldObject.IgnoreAuthor
    pub fn ignore_author(&self) -> Option<bool> {
        self.get_property(PropertyBool::IgnoreAuthor)
    }

    // ACE: WorldObject.IgnoreAuthor
    pub fn set_ignore_author(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::IgnoreAuthor),
            Some(v) => self.set_property(PropertyBool::IgnoreAuthor, v),
        }
    }

    // ACE: WorldObject.StackUnitValue
    pub fn stack_unit_value(&self) -> Option<i32> {
        self.get_property(PropertyInt::StackUnitValue)
    }

    // ACE: WorldObject.StackUnitValue
    pub fn set_stack_unit_value(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::StackUnitValue),
            Some(v) => self.set_property(PropertyInt::StackUnitValue, v),
        }
    }

    // ACE: WorldObject.StackUnitEncumbrance
    pub fn stack_unit_encumbrance(&self) -> Option<i32> {
        self.get_property(PropertyInt::StackUnitEncumbrance)
    }

    // ACE: WorldObject.StackUnitEncumbrance
    pub fn set_stack_unit_encumbrance(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::StackUnitEncumbrance),
            Some(v) => self.set_property(PropertyInt::StackUnitEncumbrance, v),
        }
    }

    // ACE: WorldObject.EncumbranceVal
    pub fn encumbrance_val(&self) -> Option<i32> {
        self.get_property(PropertyInt::EncumbranceVal)
    }

    // ACE: WorldObject.EncumbranceVal
    pub fn set_encumbrance_val(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::EncumbranceVal),
            Some(v) => self.set_property(PropertyInt::EncumbranceVal, v),
        }
    }

    // ACE: WorldObject.BulkMod
    pub fn bulk_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::BulkMod)
    }

    // ACE: WorldObject.BulkMod
    pub fn set_bulk_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::BulkMod),
            Some(v) => self.set_property(PropertyFloat::BulkMod, v),
        }
    }

    // ACE: WorldObject.SizeMod
    pub fn size_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::SizeMod)
    }

    // ACE: WorldObject.SizeMod
    pub fn set_size_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::SizeMod),
            Some(v) => self.set_property(PropertyFloat::SizeMod, v),
        }
    }

    // ACE: WorldObject.PaletteBaseId
    pub fn palette_base_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::PaletteBase)
    }

    // ACE: WorldObject.PaletteBaseId
    pub fn set_palette_base_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::PaletteBase),
            Some(v) => self.set_property(PropertyDataId::PaletteBase, v),
        }
    }

    // ACE: WorldObject.ParentLocation
    pub fn parent_location(&self) -> Option<ParentLocation> {
        self.get_property(PropertyInt::ParentLocation)
            .map(ParentLocation)
    }

    // ACE: WorldObject.ParentLocation
    pub fn set_parent_location(&mut self, value: Option<ParentLocation>) {
        match value {
            None => self.remove_property(PropertyInt::ParentLocation),
            Some(v) => self.set_property(PropertyInt::ParentLocation, v.0),
        }
    }

    // ACE: WorldObject.DefaultCombatStyle
    pub fn default_combat_style(&self) -> Option<CombatStyle> {
        self.get_property(PropertyInt::DefaultCombatStyle)
            .map(CombatStyle)
    }

    // ACE: WorldObject.DefaultCombatStyle
    pub fn set_default_combat_style(&mut self, value: Option<CombatStyle>) {
        match value {
            None => self.remove_property(PropertyInt::DefaultCombatStyle),
            Some(v) => self.set_property(PropertyInt::DefaultCombatStyle, v.0),
        }
    }

    // ACE: WorldObject.ClothingBase
    pub fn clothing_base(&self) -> Option<u32> {
        self.get_property(PropertyDataId::ClothingBase)
    }

    // ACE: WorldObject.ClothingBase
    pub fn set_clothing_base(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::ClothingBase),
            Some(v) => self.set_property(PropertyDataId::ClothingBase, v),
        }
    }

    // ACE: WorldObject.ItemCurMana
    pub fn item_cur_mana(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemCurMana)
    }

    // ACE: WorldObject.ItemCurMana
    pub fn set_item_cur_mana(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemCurMana),
            Some(v) => self.set_property(PropertyInt::ItemCurMana, v),
        }
    }

    // ACE: WorldObject.ItemMaxMana
    pub fn item_max_mana(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemMaxMana)
    }

    // ACE: WorldObject.ItemMaxMana
    pub fn set_item_max_mana(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemMaxMana),
            Some(v) => self.set_property(PropertyInt::ItemMaxMana, v),
        }
    }

    // ACE: WorldObject.ManaRate
    pub fn mana_rate(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ManaRate)
    }

    // ACE: WorldObject.ManaRate
    pub fn set_mana_rate(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ManaRate),
            Some(v) => self.set_property(PropertyFloat::ManaRate, v),
        }
    }

    // ACE: WorldObject.ItemManaCost
    pub fn item_mana_cost(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemManaCost)
    }

    // ACE: WorldObject.ItemManaCost
    pub fn set_item_mana_cost(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemManaCost),
            Some(v) => self.set_property(PropertyInt::ItemManaCost, v),
        }
    }

    // ACE: WorldObject.ItemDifficulty
    pub fn item_difficulty(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemDifficulty)
    }

    // ACE: WorldObject.ItemDifficulty
    pub fn set_item_difficulty(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemDifficulty),
            Some(v) => self.set_property(PropertyInt::ItemDifficulty, v),
        }
    }

    // ACE: WorldObject.AppraisalItemSkill
    pub fn appraisal_item_skill(&self) -> Option<i32> {
        self.get_property(PropertyInt::AppraisalItemSkill)
    }

    // ACE: WorldObject.AppraisalItemSkill
    pub fn set_appraisal_item_skill(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AppraisalItemSkill),
            Some(v) => self.set_property(PropertyInt::AppraisalItemSkill, v),
        }
    }

    // ACE: WorldObject.ItemSkillLimit
    pub fn item_skill_limit(&self) -> Option<Skill> {
        self.get_property(PropertyDataId::ItemSkillLimit)
            .map(|v| Skill(v.cs_cast()))
    }

    // ACE: WorldObject.ItemSkillLimit
    pub fn set_item_skill_limit(&mut self, value: Option<Skill>) {
        match value {
            None => self.remove_property(PropertyDataId::ItemSkillLimit),
            Some(v) => self.set_property(PropertyDataId::ItemSkillLimit, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.ItemSpecializedOnly
    pub fn item_specialized_only(&self) -> Option<Skill> {
        self.get_property(PropertyDataId::ItemSpecializedOnly)
            .map(|v| Skill(v.cs_cast()))
    }

    // ACE: WorldObject.ItemSpecializedOnly
    pub fn set_item_specialized_only(&mut self, value: Option<Skill>) {
        match value {
            None => self.remove_property(PropertyDataId::ItemSpecializedOnly),
            Some(v) => self.set_property(PropertyDataId::ItemSpecializedOnly, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.ItemSkillLevelLimit
    pub fn item_skill_level_limit(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemSkillLevelLimit)
    }

    // ACE: WorldObject.ItemSkillLevelLimit
    pub fn set_item_skill_level_limit(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemSkillLevelLimit),
            Some(v) => self.set_property(PropertyInt::ItemSkillLevelLimit, v),
        }
    }

    // ACE: WorldObject.NpcLooksLikeObject
    pub fn npc_looks_like_object(&self) -> Option<bool> {
        self.get_property(PropertyBool::NpcLooksLikeObject)
    }

    // ACE: WorldObject.NpcLooksLikeObject
    pub fn set_npc_looks_like_object(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::NpcLooksLikeObject),
            Some(v) => self.set_property(PropertyBool::NpcLooksLikeObject, v),
        }
    }

    // ACE: WorldObject.SuppressGenerateEffect
    pub fn suppress_generate_effect(&self) -> Option<bool> {
        self.get_property(PropertyBool::SuppressGenerateEffect)
    }

    // ACE: WorldObject.SuppressGenerateEffect
    pub fn set_suppress_generate_effect(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::SuppressGenerateEffect),
            Some(v) => self.set_property(PropertyBool::SuppressGenerateEffect, v),
        }
    }

    // ACE: WorldObject.CreatureType
    pub fn creature_type(&self) -> Option<CreatureType> {
        self.get_property(PropertyInt::CreatureType)
            .map(|v| CreatureType(v.cs_cast()))
    }

    // ACE: WorldObject.CreatureType
    pub fn set_creature_type(&mut self, value: Option<CreatureType>) {
        match value {
            None => self.remove_property(PropertyInt::CreatureType),
            Some(v) => self.set_property(PropertyInt::CreatureType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.FriendType
    pub fn friend_type(&self) -> Option<CreatureType> {
        self.get_property(PropertyInt::FriendType)
            .map(|v| CreatureType(v.cs_cast()))
    }

    // ACE: WorldObject.FriendType
    pub fn set_friend_type(&mut self, value: Option<CreatureType>) {
        match value {
            None => self.remove_property(PropertyInt::FriendType),
            Some(v) => self.set_property(PropertyInt::FriendType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.FoeType
    pub fn foe_type(&self) -> Option<CreatureType> {
        self.get_property(PropertyInt::FoeType)
            .map(|v| CreatureType(v.cs_cast()))
    }

    // ACE: WorldObject.FoeType
    pub fn set_foe_type(&mut self, value: Option<CreatureType>) {
        match value {
            None => self.remove_property(PropertyInt::FoeType),
            Some(v) => self.set_property(PropertyInt::FoeType, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.LongDesc
    pub fn long_desc(&self) -> Option<String> {
        self.get_property(PropertyString::LongDesc)
    }

    // ACE: WorldObject.LongDesc
    pub fn set_long_desc(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::LongDesc),
            Some(v) => self.set_property(PropertyString::LongDesc, v),
        }
    }

    // ACE: WorldObject.Use
    pub fn use_(&self) -> Option<String> {
        self.get_property(PropertyString::Use)
    }

    // ACE: WorldObject.Use
    pub fn set_use_(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::Use),
            Some(v) => self.set_property(PropertyString::Use, v),
        }
    }

    // ACE: WorldObject.BoostValue
    pub fn boost_value(&self) -> i32 {
        self.get_property(PropertyInt::BoostValue).unwrap_or(0)
    }

    // ACE: WorldObject.BoostValue
    pub fn set_boost_value(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::BoostValue);
        } else {
            self.set_property(PropertyInt::BoostValue, value);
        }
    }

    // ACE: WorldObject.BoosterEnum
    pub fn booster_enum(&self) -> PropertyAttribute2nd {
        PropertyAttribute2nd(
            self.get_property(PropertyInt::BoosterEnum)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: WorldObject.BoosterEnum
    pub fn set_booster_enum(&mut self, value: PropertyAttribute2nd) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::BoosterEnum);
        } else {
            self.set_property(PropertyInt::BoosterEnum, value.0.cs_cast());
        }
    }

    // ACE: WorldObject.UnlimitedUse
    pub fn unlimited_use(&self) -> bool {
        self.get_property(PropertyBool::UnlimitedUse)
            .unwrap_or(false)
    }

    // ACE: WorldObject.UnlimitedUse
    pub fn set_unlimited_use(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::UnlimitedUse);
        } else {
            self.set_property(PropertyBool::UnlimitedUse, value);
        }
    }

    // ACE: WorldObject.SpellDID
    pub fn spell_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::Spell)
    }

    // ACE: WorldObject.SpellDID
    pub fn set_spell_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::Spell),
            Some(v) => self.set_property(PropertyDataId::Spell, v),
        }
    }

    // ACE: WorldObject.ItemSpellcraft
    pub fn item_spellcraft(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemSpellcraft)
    }

    // ACE: WorldObject.ItemSpellcraft
    pub fn set_item_spellcraft(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemSpellcraft),
            Some(v) => self.set_property(PropertyInt::ItemSpellcraft, v),
        }
    }

    // ACE: WorldObject.HealkitMod
    pub fn healkit_mod(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HealkitMod)
    }

    // ACE: WorldObject.HealkitMod
    pub fn set_healkit_mod(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HealkitMod),
            Some(v) => self.set_property(PropertyFloat::HealkitMod, v),
        }
    }

    // ACE: WorldObject.CoinValue
    pub fn coin_value(&self) -> Option<i32> {
        self.get_property(PropertyInt::CoinValue)
    }

    // ACE: WorldObject.CoinValue
    pub fn set_coin_value(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CoinValue),
            Some(v) => self.set_property(PropertyInt::CoinValue, v),
        }
    }

    // ACE: WorldObject.ChessGamesLost
    pub fn chess_games_lost(&self) -> Option<i32> {
        self.get_property(PropertyInt::ChessGamesLost)
    }

    // ACE: WorldObject.ChessGamesLost
    pub fn set_chess_games_lost(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ChessGamesLost),
            Some(v) => self.set_property(PropertyInt::ChessGamesLost, v),
        }
    }

    // ACE: WorldObject.ChessGamesWon
    pub fn chess_games_won(&self) -> Option<i32> {
        self.get_property(PropertyInt::ChessGamesWon)
    }

    // ACE: WorldObject.ChessGamesWon
    pub fn set_chess_games_won(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ChessGamesWon),
            Some(v) => self.set_property(PropertyInt::ChessGamesWon, v),
        }
    }

    // ACE: WorldObject.ChessRank
    pub fn chess_rank(&self) -> Option<i32> {
        self.get_property(PropertyInt::ChessRank)
    }

    // ACE: WorldObject.ChessRank
    pub fn set_chess_rank(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ChessRank),
            Some(v) => self.set_property(PropertyInt::ChessRank, v),
        }
    }

    // ACE: WorldObject.ChessTotalGames
    pub fn chess_total_games(&self) -> Option<i32> {
        self.get_property(PropertyInt::ChessTotalGames)
    }

    // ACE: WorldObject.ChessTotalGames
    pub fn set_chess_total_games(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ChessTotalGames),
            Some(v) => self.set_property(PropertyInt::ChessTotalGames, v),
        }
    }

    // ACE: WorldObject.HeartbeatInterval
    pub fn heartbeat_interval(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HeartbeatInterval)
    }

    // ACE: WorldObject.HeartbeatInterval
    pub fn set_heartbeat_interval(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HeartbeatInterval),
            Some(v) => self.set_property(PropertyFloat::HeartbeatInterval, v),
        }
    }

    // ACE: WorldObject.HeartbeatTimestamp
    pub fn heartbeat_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HeartbeatTimestamp)
    }

    // ACE: WorldObject.HeartbeatTimestamp
    pub fn set_heartbeat_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HeartbeatTimestamp),
            Some(v) => self.set_property(PropertyFloat::HeartbeatTimestamp, v),
        }
    }

    // ACE: WorldObject.InitGeneratedObjects
    pub fn init_generated_objects(&self) -> i32 {
        self.get_property(PropertyInt::InitGeneratedObjects)
            .unwrap_or(0)
    }

    // ACE: WorldObject.InitGeneratedObjects
    pub fn set_init_generated_objects(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::InitGeneratedObjects);
        } else {
            self.set_property(PropertyInt::InitGeneratedObjects, value);
        }
    }

    // ACE: WorldObject.MaxGeneratedObjects
    pub fn max_generated_objects(&self) -> i32 {
        self.get_property(PropertyInt::MaxGeneratedObjects)
            .unwrap_or(0)
    }

    // ACE: WorldObject.MaxGeneratedObjects
    pub fn set_max_generated_objects(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::MaxGeneratedObjects);
        } else {
            self.set_property(PropertyInt::MaxGeneratedObjects, value);
        }
    }

    // ACE: WorldObject.RegenerationInterval
    pub fn regeneration_interval(&self) -> f64 {
        self.get_property(PropertyFloat::RegenerationInterval)
            .unwrap_or(0.0)
    }

    // ACE: WorldObject.RegenerationInterval
    pub fn set_regeneration_interval(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::RegenerationInterval);
        } else {
            self.set_property(PropertyFloat::RegenerationInterval, value);
        }
    }

    // ACE: WorldObject.RegenerationTimestamp
    pub fn regeneration_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::RegenerationTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: WorldObject.RegenerationTimestamp
    pub fn set_regeneration_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::RegenerationTimestamp);
        } else {
            self.set_property(PropertyFloat::RegenerationTimestamp, value);
        }
    }

    // ACE: WorldObject.GeneratorUpdateTimestamp
    pub fn generator_update_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::GeneratorUpdateTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: WorldObject.GeneratorUpdateTimestamp
    pub fn set_generator_update_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::GeneratorUpdateTimestamp);
        } else {
            self.set_property(PropertyFloat::GeneratorUpdateTimestamp, value);
        }
    }

    // ACE: WorldObject.GeneratorEnteredWorld
    pub fn generator_entered_world(&self) -> bool {
        self.get_property(PropertyBool::GeneratorEnteredWorld)
            .unwrap_or(false)
    }

    // ACE: WorldObject.GeneratorEnteredWorld
    pub fn set_generator_entered_world(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::GeneratorEnteredWorld);
        } else {
            self.set_property(PropertyBool::GeneratorEnteredWorld, value);
        }
    }

    // ACE: WorldObject.GeneratedTreasureItem
    pub fn generated_treasure_item(&self) -> bool {
        self.get_property(PropertyBool::GeneratedTreasureItem)
            .unwrap_or(false)
    }

    // ACE: WorldObject.GeneratedTreasureItem
    pub fn set_generated_treasure_item(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::GeneratedTreasureItem);
        } else {
            self.set_property(PropertyBool::GeneratedTreasureItem, value);
        }
    }

    // ACE: WorldObject.TsysMutationData
    pub fn tsys_mutation_data(&self) -> Option<i32> {
        self.get_property(PropertyInt::TsysMutationData)
    }

    // ACE: WorldObject.TsysMutationData
    pub fn set_tsys_mutation_data(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::TsysMutationData),
            Some(v) => self.set_property(PropertyInt::TsysMutationData, v),
        }
    }

    // ACE: WorldObject.Visibility
    pub fn visibility(&self) -> bool {
        self.get_property(PropertyBool::Visibility).unwrap_or(false)
    }

    // ACE: WorldObject.Visibility
    pub fn set_visibility(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Visibility);
        } else {
            self.set_property(PropertyBool::Visibility, value);
        }
    }

    // ACE: WorldObject.PaletteTemplate
    pub fn palette_template(&self) -> Option<i32> {
        self.get_property(PropertyInt::PaletteTemplate)
    }

    // ACE: WorldObject.PaletteTemplate
    pub fn set_palette_template(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PaletteTemplate),
            Some(v) => self.set_property(PropertyInt::PaletteTemplate, v),
        }
    }

    // ACE: WorldObject.Shade
    pub fn shade(&self) -> Option<f64> {
        self.get_property(PropertyFloat::Shade)
    }

    // ACE: WorldObject.Shade
    pub fn set_shade(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::Shade),
            Some(v) => self.set_property(PropertyFloat::Shade, v),
        }
    }

    // ACE: WorldObject.Shade2
    pub fn shade2(&self) -> Option<f64> {
        self.get_property(PropertyFloat::Shade2)
    }

    // ACE: WorldObject.Shade2
    pub fn set_shade2(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::Shade2),
            Some(v) => self.set_property(PropertyFloat::Shade2, v),
        }
    }

    // ACE: WorldObject.Shade3
    pub fn shade3(&self) -> Option<f64> {
        self.get_property(PropertyFloat::Shade3)
    }

    // ACE: WorldObject.Shade3
    pub fn set_shade3(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::Shade3),
            Some(v) => self.set_property(PropertyFloat::Shade3, v),
        }
    }

    // ACE: WorldObject.Shade4
    pub fn shade4(&self) -> Option<f64> {
        self.get_property(PropertyFloat::Shade4)
    }

    // ACE: WorldObject.Shade4
    pub fn set_shade4(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::Shade4),
            Some(v) => self.set_property(PropertyFloat::Shade4, v),
        }
    }

    // ACE: WorldObject.NumTimesTinkered
    pub fn num_times_tinkered(&self) -> i32 {
        self.get_property(PropertyInt::NumTimesTinkered)
            .unwrap_or(0)
    }

    // ACE: WorldObject.NumTimesTinkered
    pub fn set_num_times_tinkered(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::NumTimesTinkered);
        } else {
            self.set_property(PropertyInt::NumTimesTinkered, value);
        }
    }

    // ACE: WorldObject.Location
    pub fn location(&self) -> Option<Position> {
        self.get_position(PositionType::Location)
    }

    // ACE: WorldObject.Location
    pub fn set_location(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Location, value);
    }

    // ACE: WorldObject.Destination
    pub fn destination(&self) -> Option<Position> {
        self.get_position(PositionType::Destination)
    }

    // ACE: WorldObject.Destination
    pub fn set_destination(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Destination, value);
    }

    // ACE: WorldObject.Instantiation
    pub fn instantiation(&self) -> Option<Position> {
        self.get_position(PositionType::Instantiation)
    }

    // ACE: WorldObject.Instantiation
    pub fn set_instantiation(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Instantiation, value);
    }

    // ACE: WorldObject.Sanctuary
    pub fn sanctuary(&self) -> Option<Position> {
        self.get_position(PositionType::Sanctuary)
    }

    // ACE: WorldObject.Sanctuary
    pub fn set_sanctuary(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Sanctuary, value);
    }

    // ACE: WorldObject.Home
    pub fn home(&self) -> Option<Position> {
        self.get_position(PositionType::Home)
    }

    // ACE: WorldObject.Home
    pub fn set_home(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Home, value);
    }

    // ACE: WorldObject.ActivationMove
    pub fn activation_move(&self) -> Option<Position> {
        self.get_position(PositionType::ActivationMove)
    }

    // ACE: WorldObject.ActivationMove
    pub fn set_activation_move(&mut self, value: Option<Position>) {
        self.set_position(PositionType::ActivationMove, value);
    }

    // ACE: WorldObject.Target
    pub fn target(&self) -> Option<Position> {
        self.get_position(PositionType::Target)
    }

    // ACE: WorldObject.Target
    pub fn set_target(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Target, value);
    }

    // ACE: WorldObject.LinkedPortalOne
    pub fn linked_portal_one(&self) -> Option<Position> {
        self.get_position(PositionType::LinkedPortalOne)
    }

    // ACE: WorldObject.LinkedPortalOne
    pub fn set_linked_portal_one(&mut self, value: Option<Position>) {
        self.set_position(PositionType::LinkedPortalOne, value);
    }

    // ACE: WorldObject.LinkedPortalOneDID
    pub fn linked_portal_one_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::LinkedPortalOne)
    }

    // ACE: WorldObject.LinkedPortalOneDID
    pub fn set_linked_portal_one_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::LinkedPortalOne),
            Some(v) => self.set_property(PropertyDataId::LinkedPortalOne, v),
        }
    }

    // ACE: WorldObject.LinkedPortalTwoDID
    pub fn linked_portal_two_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::LinkedPortalTwo)
    }

    // ACE: WorldObject.LinkedPortalTwoDID
    pub fn set_linked_portal_two_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::LinkedPortalTwo),
            Some(v) => self.set_property(PropertyDataId::LinkedPortalTwo, v),
        }
    }

    // ACE: WorldObject.LastPortal
    pub fn last_portal(&self) -> Option<Position> {
        self.get_position(PositionType::LastPortal)
    }

    // ACE: WorldObject.LastPortal
    pub fn set_last_portal(&mut self, value: Option<Position>) {
        self.set_position(PositionType::LastPortal, value);
    }

    // ACE: WorldObject.LastPortalDID
    pub fn last_portal_did(&self) -> Option<u32> {
        self.get_property(PropertyDataId::LastPortal)
    }

    // ACE: WorldObject.LastPortalDID
    pub fn set_last_portal_did(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::LastPortal),
            Some(v) => self.set_property(PropertyDataId::LastPortal, v),
        }
    }

    // ACE: WorldObject.PortalStorm
    pub fn portal_storm(&self) -> Option<Position> {
        self.get_position(PositionType::PortalStorm)
    }

    // ACE: WorldObject.PortalStorm
    pub fn set_portal_storm(&mut self, value: Option<Position>) {
        self.set_position(PositionType::PortalStorm, value);
    }

    // ACE: WorldObject.CrashAndTurn
    pub fn crash_and_turn(&self) -> Option<Position> {
        self.get_position(PositionType::CrashAndTurn)
    }

    // ACE: WorldObject.CrashAndTurn
    pub fn set_crash_and_turn(&mut self, value: Option<Position>) {
        self.set_position(PositionType::CrashAndTurn, value);
    }

    // ACE: WorldObject.PortalSummonLoc
    pub fn portal_summon_loc(&self) -> Option<Position> {
        self.get_position(PositionType::PortalSummonLoc)
    }

    // ACE: WorldObject.PortalSummonLoc
    pub fn set_portal_summon_loc(&mut self, value: Option<Position>) {
        self.set_position(PositionType::PortalSummonLoc, value);
    }

    // ACE: WorldObject.HouseBoot
    pub fn house_boot(&self) -> Option<Position> {
        self.get_position(PositionType::HouseBoot)
    }

    // ACE: WorldObject.HouseBoot
    pub fn set_house_boot(&mut self, value: Option<Position>) {
        self.set_position(PositionType::HouseBoot, value);
    }

    // ACE: WorldObject.LastOutsideDeath
    pub fn last_outside_death(&self) -> Option<Position> {
        self.get_position(PositionType::LastOutsideDeath)
    }

    // ACE: WorldObject.LastOutsideDeath
    pub fn set_last_outside_death(&mut self, value: Option<Position>) {
        self.set_position(PositionType::LastOutsideDeath, value);
    }

    // ACE: WorldObject.LinkedLifestone
    pub fn linked_lifestone(&self) -> Option<Position> {
        self.get_position(PositionType::LinkedLifestone)
    }

    // ACE: WorldObject.LinkedLifestone
    pub fn set_linked_lifestone(&mut self, value: Option<Position>) {
        self.set_position(PositionType::LinkedLifestone, value);
    }

    // ACE: WorldObject.LinkedPortalTwo
    pub fn linked_portal_two(&self) -> Option<Position> {
        self.get_position(PositionType::LinkedPortalTwo)
    }

    // ACE: WorldObject.LinkedPortalTwo
    pub fn set_linked_portal_two(&mut self, value: Option<Position>) {
        self.set_position(PositionType::LinkedPortalTwo, value);
    }

    // ACE: WorldObject.Save1
    pub fn save1(&self) -> Option<Position> {
        self.get_position(PositionType::Save1)
    }

    // ACE: WorldObject.Save1
    pub fn set_save1(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save1, value);
    }

    // ACE: WorldObject.Save2
    pub fn save2(&self) -> Option<Position> {
        self.get_position(PositionType::Save2)
    }

    // ACE: WorldObject.Save2
    pub fn set_save2(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save2, value);
    }

    // ACE: WorldObject.Save3
    pub fn save3(&self) -> Option<Position> {
        self.get_position(PositionType::Save3)
    }

    // ACE: WorldObject.Save3
    pub fn set_save3(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save3, value);
    }

    // ACE: WorldObject.Save4
    pub fn save4(&self) -> Option<Position> {
        self.get_position(PositionType::Save4)
    }

    // ACE: WorldObject.Save4
    pub fn set_save4(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save4, value);
    }

    // ACE: WorldObject.Save5
    pub fn save5(&self) -> Option<Position> {
        self.get_position(PositionType::Save5)
    }

    // ACE: WorldObject.Save5
    pub fn set_save5(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save5, value);
    }

    // ACE: WorldObject.Save6
    pub fn save6(&self) -> Option<Position> {
        self.get_position(PositionType::Save6)
    }

    // ACE: WorldObject.Save6
    pub fn set_save6(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save6, value);
    }

    // ACE: WorldObject.Save7
    pub fn save7(&self) -> Option<Position> {
        self.get_position(PositionType::Save7)
    }

    // ACE: WorldObject.Save7
    pub fn set_save7(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save7, value);
    }

    // ACE: WorldObject.Save8
    pub fn save8(&self) -> Option<Position> {
        self.get_position(PositionType::Save8)
    }

    // ACE: WorldObject.Save8
    pub fn set_save8(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save8, value);
    }

    // ACE: WorldObject.Save9
    pub fn save9(&self) -> Option<Position> {
        self.get_position(PositionType::Save9)
    }

    // ACE: WorldObject.Save9
    pub fn set_save9(&mut self, value: Option<Position>) {
        self.set_position(PositionType::Save9, value);
    }

    // ACE: WorldObject.RelativeDestination
    pub fn relative_destination(&self) -> Option<Position> {
        self.get_position(PositionType::RelativeDestination)
    }

    // ACE: WorldObject.RelativeDestination
    pub fn set_relative_destination(&mut self, value: Option<Position>) {
        self.set_position(PositionType::RelativeDestination, value);
    }

    // ACE: WorldObject.TeleportedCharacter
    pub fn teleported_character(&self) -> Option<Position> {
        self.get_position(PositionType::TeleportedCharacter)
    }

    // ACE: WorldObject.TeleportedCharacter
    pub fn set_teleported_character(&mut self, value: Option<Position>) {
        self.set_position(PositionType::TeleportedCharacter, value);
    }

    // ACE: WorldObject.CurrentCombatTarget
    pub fn current_combat_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentCombatTarget)
    }

    // ACE: WorldObject.CurrentCombatTarget
    pub fn set_current_combat_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentCombatTarget),
            Some(v) => self.set_property(PropertyInstanceId::CurrentCombatTarget, v),
        }
    }

    // ACE: WorldObject.CurrentEnemy
    pub fn current_enemy(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentEnemy)
    }

    // ACE: WorldObject.CurrentEnemy
    pub fn set_current_enemy(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentEnemy),
            Some(v) => self.set_property(PropertyInstanceId::CurrentEnemy, v),
        }
    }

    // ACE: WorldObject.CurrentAttacker
    pub fn current_attacker(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentAttacker)
    }

    // ACE: WorldObject.CurrentAttacker
    pub fn set_current_attacker_prop(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentAttacker),
            Some(v) => self.set_property(PropertyInstanceId::CurrentAttacker, v),
        }
    }

    // ACE: WorldObject.CurrentDamager
    pub fn current_damager(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentDamager)
    }

    // ACE: WorldObject.CurrentDamager
    pub fn set_current_damager(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentDamager),
            Some(v) => self.set_property(PropertyInstanceId::CurrentDamager, v),
        }
    }

    // ACE: WorldObject.CurrentFollowTarget
    pub fn current_follow_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentFollowTarget)
    }

    // ACE: WorldObject.CurrentFollowTarget
    pub fn set_current_follow_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentFollowTarget),
            Some(v) => self.set_property(PropertyInstanceId::CurrentFollowTarget, v),
        }
    }

    // ACE: WorldObject.CurrentFellowshipAppraisalTarget
    pub fn current_fellowship_appraisal_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentFellowshipAppraisalTarget)
    }

    // ACE: WorldObject.CurrentFellowshipAppraisalTarget
    pub fn set_current_fellowship_appraisal_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentFellowshipAppraisalTarget),
            Some(v) => self.set_property(PropertyInstanceId::CurrentFellowshipAppraisalTarget, v),
        }
    }

    // ACE: WorldObject.CombatTarget
    pub fn combat_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CombatTarget)
    }

    // ACE: WorldObject.CombatTarget
    pub fn set_combat_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CombatTarget),
            Some(v) => self.set_property(PropertyInstanceId::CombatTarget, v),
        }
    }

    // ACE: WorldObject.HealthQueryTarget
    pub fn health_query_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::HealthQueryTarget)
    }

    // ACE: WorldObject.HealthQueryTarget
    pub fn set_health_query_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::HealthQueryTarget),
            Some(v) => self.set_property(PropertyInstanceId::HealthQueryTarget, v),
        }
    }

    // ACE: WorldObject.ManaQueryTarget
    pub fn mana_query_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::ManaQueryTarget)
    }

    // ACE: WorldObject.ManaQueryTarget
    pub fn set_mana_query_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::ManaQueryTarget),
            Some(v) => self.set_property(PropertyInstanceId::ManaQueryTarget, v),
        }
    }

    // ACE: WorldObject.PkLevelModifier
    pub fn pk_level_modifier(&self) -> i32 {
        self.get_property(PropertyInt::PkLevelModifier).unwrap_or(0)
    }

    // ACE: WorldObject.PkLevelModifier
    pub fn set_pk_level_modifier(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::PkLevelModifier);
        } else {
            self.set_property(PropertyInt::PkLevelModifier, value);
        }
    }

    // ACE: WorldObject.PlayerKillerStatus
    pub fn player_killer_status(&self) -> PlayerKillerStatus {
        self.get_property(PropertyInt::PlayerKillerStatus)
            .map(|v| PlayerKillerStatus(v.cs_cast()))
            .unwrap_or(PlayerKillerStatus::NPK)
    }

    // ACE: WorldObject.PlayerKillerStatus
    pub fn set_player_killer_status_prop(&mut self, value: PlayerKillerStatus) {
        self.set_property(PropertyInt::PlayerKillerStatus, value.0.cs_cast());
    }

    // ACE: WorldObject.CloakStatus
    pub fn cloak_status(&self) -> CloakStatus {
        CloakStatus(self.get_property(PropertyInt::CloakStatus).unwrap_or(0))
    }

    // ACE: WorldObject.CloakStatus
    pub fn set_cloak_status(&mut self, value: CloakStatus) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::CloakStatus);
        } else {
            self.set_property(PropertyInt::CloakStatus, value.0);
        }
    }

    // ACE: WorldObject.IgnorePortalRestrictions
    pub fn ignore_portal_restrictions(&self) -> bool {
        self.get_property(PropertyBool::IgnorePortalRestrictions)
            .unwrap_or(false)
    }

    // ACE: WorldObject.IgnorePortalRestrictions
    pub fn set_ignore_portal_restrictions(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IgnorePortalRestrictions);
        } else {
            self.set_property(PropertyBool::IgnorePortalRestrictions, value);
        }
    }

    // ACE: WorldObject.Invincible
    pub fn invincible(&self) -> bool {
        self.get_property(PropertyBool::Invincible).unwrap_or(false)
    }

    // ACE: WorldObject.Invincible
    pub fn set_invincible(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Invincible);
        } else {
            self.set_property(PropertyBool::Invincible, value);
        }
    }

    // ACE: WorldObject.XpOverride
    pub fn xp_override(&self) -> Option<i32> {
        self.get_property(PropertyInt::XpOverride)
    }

    // ACE: WorldObject.XpOverride
    pub fn set_xp_override(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::XpOverride),
            Some(v) => self.set_property(PropertyInt::XpOverride, v),
        }
    }

    // ACE: WorldObject.MinLevel
    pub fn min_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::MinLevel)
    }

    // ACE: WorldObject.MinLevel
    pub fn set_min_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::MinLevel),
            Some(v) => self.set_property(PropertyInt::MinLevel, v),
        }
    }

    // ACE: WorldObject.MaxLevel
    pub fn max_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::MaxLevel)
    }

    // ACE: WorldObject.MaxLevel
    pub fn set_max_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::MaxLevel),
            Some(v) => self.set_property(PropertyInt::MaxLevel, v),
        }
    }

    // ACE: WorldObject.FirstEnterWorldDone
    pub fn first_enter_world_done(&self) -> bool {
        self.get_property(PropertyBool::FirstEnterWorldDone)
            .unwrap_or(false)
    }

    // ACE: WorldObject.FirstEnterWorldDone
    pub fn set_first_enter_world_done(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::FirstEnterWorldDone);
        } else {
            self.set_property(PropertyBool::FirstEnterWorldDone, value);
        }
    }

    // ACE: WorldObject.OwnerId
    pub fn owner_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Owner)
    }

    // ACE: WorldObject.OwnerId
    pub fn set_owner_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Owner),
            Some(v) => self.set_property(PropertyInstanceId::Owner, v),
        }
    }

    // ACE: WorldObject.ActivationTarget
    pub fn activation_target(&self) -> u32 {
        self.get_property(PropertyInstanceId::ActivationTarget)
            .unwrap_or(0)
    }

    // ACE: WorldObject.ActivationTarget
    pub fn set_activation_target(&mut self, value: u32) {
        if value == 0 {
            self.remove_property(PropertyInstanceId::ActivationTarget);
        } else {
            self.set_property(PropertyInstanceId::ActivationTarget, value);
        }
    }

    // ACE: WorldObject.TimeToRot
    pub fn time_to_rot(&self) -> Option<f64> {
        self.get_property(PropertyFloat::TimeToRot)
    }

    // ACE: WorldObject.TimeToRot
    pub fn set_time_to_rot(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::TimeToRot),
            Some(v) => self.set_property(PropertyFloat::TimeToRot, v),
        }
    }

    // ACE: WorldObject.AllowedActivator
    pub fn allowed_activator(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::AllowedActivator)
    }

    // ACE: WorldObject.AllowedActivator
    pub fn set_allowed_activator(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::AllowedActivator),
            Some(v) => self.set_property(PropertyInstanceId::AllowedActivator, v),
        }
    }

    // ACE: WorldObject.GeneratorId
    pub fn generator_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Generator)
    }

    // ACE: WorldObject.GeneratorId
    pub fn set_generator_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Generator),
            Some(v) => self.set_property(PropertyInstanceId::Generator, v),
        }
    }

    // ACE: WorldObject.CurrentlyPoweringUp
    pub fn currently_powering_up(&self) -> bool {
        self.get_property(PropertyBool::CurrentlyPoweringUp)
            .unwrap_or(false)
    }

    // ACE: WorldObject.CurrentlyPoweringUp
    pub fn set_currently_powering_up(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::CurrentlyPoweringUp);
        } else {
            self.set_property(PropertyBool::CurrentlyPoweringUp, value);
        }
    }

    // ACE: WorldObject.GeneratorDisabled
    pub fn generator_disabled(&self) -> bool {
        self.get_property(PropertyBool::GeneratorDisabled)
            .unwrap_or(false)
    }

    // ACE: WorldObject.GeneratorDisabled
    pub fn set_generator_disabled(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::GeneratorDisabled);
        } else {
            self.set_property(PropertyBool::GeneratorDisabled, value);
        }
    }

    // ACE: WorldObject.GeneratorStatus
    pub fn generator_status(&self) -> bool {
        self.get_property(PropertyBool::GeneratorStatus)
            .unwrap_or(false)
    }

    // ACE: WorldObject.GeneratorStatus
    pub fn set_generator_status(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::GeneratorStatus);
        } else {
            self.set_property(PropertyBool::GeneratorStatus, value);
        }
    }

    // ACE: WorldObject.GeneratorAutomaticDestruction
    pub fn generator_automatic_destruction(&self) -> bool {
        self.get_property(PropertyBool::GeneratorAutomaticDestruction)
            .unwrap_or(false)
    }

    // ACE: WorldObject.GeneratorAutomaticDestruction
    pub fn set_generator_automatic_destruction(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::GeneratorAutomaticDestruction);
        } else {
            self.set_property(PropertyBool::GeneratorAutomaticDestruction, value);
        }
    }

    // ACE: WorldObject.GeneratorEvent
    pub fn generator_event(&self) -> Option<String> {
        self.get_property(PropertyString::GeneratorEvent)
    }

    // ACE: WorldObject.GeneratorEvent
    pub fn set_generator_event(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::GeneratorEvent),
            Some(v) => self.set_property(PropertyString::GeneratorEvent, v),
        }
    }

    // ACE: WorldObject.GeneratorTimeType
    pub fn generator_time_type(&self) -> GeneratorTimeType {
        GeneratorTimeType(
            self.get_property(PropertyInt::GeneratorTimeType)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.GeneratorTimeType
    pub fn set_generator_time_type(&mut self, value: GeneratorTimeType) {
        if value == GeneratorTimeType::Undef {
            self.remove_property(PropertyInt::GeneratorTimeType);
        } else {
            self.set_property(PropertyInt::GeneratorTimeType, value.0);
        }
    }

    // ACE: WorldObject.GeneratorDestructionType
    pub fn generator_destruction_type(&self) -> GeneratorDestruct {
        GeneratorDestruct(
            self.get_property(PropertyInt::GeneratorDestructionType)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.GeneratorDestructionType
    pub fn set_generator_destruction_type(&mut self, value: GeneratorDestruct) {
        if value == GeneratorDestruct::Undef {
            self.remove_property(PropertyInt::GeneratorDestructionType);
        } else {
            self.set_property(PropertyInt::GeneratorDestructionType, value.0);
        }
    }

    // ACE: WorldObject.GeneratorEndDestructionType
    pub fn generator_end_destruction_type(&self) -> GeneratorDestruct {
        GeneratorDestruct(
            self.get_property(PropertyInt::GeneratorEndDestructionType)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.GeneratorEndDestructionType
    pub fn set_generator_end_destruction_type(&mut self, value: GeneratorDestruct) {
        if value == GeneratorDestruct::Undef {
            self.remove_property(PropertyInt::GeneratorEndDestructionType);
        } else {
            self.set_property(PropertyInt::GeneratorEndDestructionType, value.0);
        }
    }

    // ACE: WorldObject.GeneratorType
    pub fn generator_type(&self) -> GeneratorType {
        GeneratorType(self.get_property(PropertyInt::GeneratorType).unwrap_or(0))
    }

    // ACE: WorldObject.GeneratorType
    pub fn set_generator_type(&mut self, value: GeneratorType) {
        if value == GeneratorType::Undef {
            self.remove_property(PropertyInt::GeneratorType);
        } else {
            self.set_property(PropertyInt::GeneratorType, value.0);
        }
    }

    // ACE: WorldObject.GeneratorStartTime
    pub fn generator_start_time(&self) -> i32 {
        self.get_property(PropertyInt::GeneratorStartTime)
            .unwrap_or(0)
    }

    // ACE: WorldObject.GeneratorStartTime
    pub fn set_generator_start_time(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::GeneratorStartTime);
        } else {
            self.set_property(PropertyInt::GeneratorStartTime, value);
        }
    }

    // ACE: WorldObject.GeneratorEndTime
    pub fn generator_end_time(&self) -> i32 {
        self.get_property(PropertyInt::GeneratorEndTime)
            .unwrap_or(0)
    }

    // ACE: WorldObject.GeneratorEndTime
    pub fn set_generator_end_time(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::GeneratorEndTime);
        } else {
            self.set_property(PropertyInt::GeneratorEndTime, value);
        }
    }

    // ACE: WorldObject.GeneratorInitialDelay
    pub fn generator_initial_delay(&self) -> f64 {
        self.get_property(PropertyFloat::GeneratorInitialDelay)
            .unwrap_or(0.0)
    }

    // ACE: WorldObject.GeneratorInitialDelay
    pub fn set_generator_initial_delay(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::GeneratorInitialDelay);
        } else {
            self.set_property(PropertyFloat::GeneratorInitialDelay, value);
        }
    }

    // ACE: WorldObject.Quest
    pub fn quest(&self) -> Option<String> {
        self.get_property(PropertyString::Quest)
    }

    // ACE: WorldObject.Quest
    pub fn set_quest(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::Quest),
            Some(v) => self.set_property(PropertyString::Quest, v),
        }
    }

    // ACE: WorldObject.QuestRestriction
    pub fn quest_restriction(&self) -> Option<String> {
        self.get_property(PropertyString::QuestRestriction)
    }

    // ACE: WorldObject.QuestRestriction
    pub fn set_quest_restriction(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::QuestRestriction),
            Some(v) => self.set_property(PropertyString::QuestRestriction, v),
        }
    }

    // ACE: WorldObject.Active
    pub fn active(&self) -> bool {
        self.get_property(PropertyInt::Active).unwrap_or(1) != 0
    }

    // ACE: WorldObject.Active
    pub fn set_active(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyInt::Active);
        } else {
            self.set_property(PropertyInt::Active, 0);
        }
    }

    // ACE: WorldObject.ActivationResponse
    pub fn activation_response(&self) -> ActivationResponse {
        ActivationResponse(
            self.get_property(PropertyInt::ActivationResponse)
                .unwrap_or(2),
        )
    }

    // ACE: WorldObject.ActivationResponse
    pub fn set_activation_response(&mut self, value: ActivationResponse) {
        if value == ActivationResponse::Use {
            self.remove_property(PropertyInt::ActivationResponse);
        } else {
            self.set_property(PropertyInt::ActivationResponse, value.0);
        }
    }

    // ACE: WorldObject.ActivationAnimation
    pub fn activation_animation(&self) -> MotionCommand {
        MotionCommand(
            self.get_property(PropertyDataId::ActivationAnimation)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.ActivationAnimation
    pub fn set_activation_animation(&mut self, value: MotionCommand) {
        if value.0 == 0 {
            self.remove_property(PropertyDataId::ActivationAnimation);
        } else {
            self.set_property(PropertyDataId::ActivationAnimation, value.0);
        }
    }

    // ACE: WorldObject.ActivationTalk
    pub fn activation_talk(&self) -> Option<String> {
        self.get_property(PropertyString::ActivationTalk)
    }

    // ACE: WorldObject.ActivationTalk
    pub fn set_activation_talk(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::ActivationTalk),
            Some(v) => self.set_property(PropertyString::ActivationTalk, v),
        }
    }

    // ACE: WorldObject.UseSound
    pub fn use_sound(&self) -> Sound {
        Sound(self.get_property(PropertyDataId::UseSound).unwrap_or(0))
    }

    // ACE: WorldObject.UseSound
    pub fn set_use_sound(&mut self, value: Sound) {
        if value.0 == 0 {
            self.remove_property(PropertyDataId::UseSound);
        } else {
            self.set_property(PropertyDataId::UseSound, value.0);
        }
    }

    // ACE: WorldObject.UseTargetSuccessAnimation
    pub fn use_target_success_animation(&self) -> MotionCommand {
        MotionCommand(
            self.get_property(PropertyDataId::UseTargetSuccessAnimation)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.UseTargetSuccessAnimation
    pub fn set_use_target_success_animation(&mut self, value: MotionCommand) {
        if value.0 == 0 {
            self.remove_property(PropertyDataId::UseTargetSuccessAnimation);
        } else {
            self.set_property(PropertyDataId::UseTargetSuccessAnimation, value.0);
        }
    }

    // ACE: WorldObject.UseTargetFailureAnimation
    pub fn use_target_failure_animation(&self) -> MotionCommand {
        MotionCommand(
            self.get_property(PropertyDataId::UseTargetFailureAnimation)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.UseTargetFailureAnimation
    pub fn set_use_target_failure_animation(&mut self, value: MotionCommand) {
        if value.0 == 0 {
            self.remove_property(PropertyDataId::UseTargetFailureAnimation);
        } else {
            self.set_property(PropertyDataId::UseTargetFailureAnimation, value.0);
        }
    }

    // ACE: WorldObject.UseUserAnimation
    pub fn use_user_animation(&self) -> MotionCommand {
        MotionCommand(
            self.get_property(PropertyDataId::UseUserAnimation)
                .unwrap_or(0),
        )
    }

    // ACE: WorldObject.UseUserAnimation
    pub fn set_use_user_animation(&mut self, value: MotionCommand) {
        if value.0 == 0 {
            self.remove_property(PropertyDataId::UseUserAnimation);
        } else {
            self.set_property(PropertyDataId::UseUserAnimation, value.0);
        }
    }

    // ACE: WorldObject.UseCreateItem
    pub fn use_create_item(&self) -> Option<u32> {
        self.get_property(PropertyDataId::UseCreateItem)
    }

    // ACE: WorldObject.UseCreateItem
    pub fn set_use_create_item(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::UseCreateItem),
            Some(v) => self.set_property(PropertyDataId::UseCreateItem, v),
        }
    }

    // ACE: WorldObject.UseCreateQuantity
    pub fn use_create_quantity(&self) -> Option<i32> {
        self.get_property(PropertyInt::UseCreateQuantity)
    }

    // ACE: WorldObject.UseCreateQuantity
    pub fn set_use_create_quantity(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::UseCreateQuantity),
            Some(v) => self.set_property(PropertyInt::UseCreateQuantity, v),
        }
    }

    // ACE: WorldObject.ResistLockpick
    pub fn resist_lockpick(&self) -> Option<i32> {
        self.get_property(PropertyInt::ResistLockpick)
    }

    // ACE: WorldObject.ResistLockpick
    pub fn set_resist_lockpick(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ResistLockpick),
            Some(v) => self.set_property(PropertyInt::ResistLockpick, v),
        }
    }

    // ACE: WorldObject.VictimId
    pub fn victim_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Victim)
    }

    // ACE: WorldObject.VictimId
    pub fn set_victim_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Victim),
            Some(v) => self.set_property(PropertyInstanceId::Victim, v),
        }
    }

    // ACE: WorldObject.KillerId
    pub fn killer_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Killer)
    }

    // ACE: WorldObject.KillerId
    pub fn set_killer_id(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::Killer),
            Some(v) => self.set_property(PropertyInstanceId::Killer, v),
        }
    }

    // ACE: WorldObject.DamageRating
    pub fn damage_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::DamageRating)
    }

    // ACE: WorldObject.DamageRating
    pub fn set_damage_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::DamageRating),
            Some(v) => self.set_property(PropertyInt::DamageRating, v),
        }
    }

    // ACE: WorldObject.DamageResistRating
    pub fn damage_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::DamageResistRating)
    }

    // ACE: WorldObject.DamageResistRating
    pub fn set_damage_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::DamageResistRating),
            Some(v) => self.set_property(PropertyInt::DamageResistRating, v),
        }
    }

    // ACE: WorldObject.CritDamageRating
    pub fn crit_damage_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::CritDamageRating)
    }

    // ACE: WorldObject.CritDamageRating
    pub fn set_crit_damage_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CritDamageRating),
            Some(v) => self.set_property(PropertyInt::CritDamageRating, v),
        }
    }

    // ACE: WorldObject.CritDamageResistRating
    pub fn crit_damage_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::CritDamageResistRating)
    }

    // ACE: WorldObject.CritDamageResistRating
    pub fn set_crit_damage_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CritDamageResistRating),
            Some(v) => self.set_property(PropertyInt::CritDamageResistRating, v),
        }
    }

    // ACE: WorldObject.CritRating
    pub fn crit_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::CritRating)
    }

    // ACE: WorldObject.CritRating
    pub fn set_crit_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CritRating),
            Some(v) => self.set_property(PropertyInt::CritRating, v),
        }
    }

    // ACE: WorldObject.CritResistRating
    pub fn crit_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::CritResistRating)
    }

    // ACE: WorldObject.CritResistRating
    pub fn set_crit_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CritResistRating),
            Some(v) => self.set_property(PropertyInt::CritResistRating, v),
        }
    }

    // ACE: WorldObject.HealingBoostRating
    pub fn healing_boost_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::HealingBoostRating)
    }

    // ACE: WorldObject.HealingBoostRating
    pub fn set_healing_boost_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HealingBoostRating),
            Some(v) => self.set_property(PropertyInt::HealingBoostRating, v),
        }
    }

    // ACE: WorldObject.HealingResistRating
    pub fn healing_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::HealingResistRating)
    }

    // ACE: WorldObject.HealingResistRating
    pub fn set_healing_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HealingResistRating),
            Some(v) => self.set_property(PropertyInt::HealingResistRating, v),
        }
    }

    // ACE: WorldObject.LifeResistRating
    pub fn life_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::LifeResistRating)
    }

    // ACE: WorldObject.LifeResistRating
    pub fn set_life_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::LifeResistRating),
            Some(v) => self.set_property(PropertyInt::LifeResistRating, v),
        }
    }

    // ACE: WorldObject.DotResistRating
    pub fn dot_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::DotResistRating)
    }

    // ACE: WorldObject.DotResistRating
    pub fn set_dot_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::DotResistRating),
            Some(v) => self.set_property(PropertyInt::DotResistRating, v),
        }
    }

    // ACE: WorldObject.NetherResistRating
    pub fn nether_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::NetherResistRating)
    }

    // ACE: WorldObject.NetherResistRating
    pub fn set_nether_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::NetherResistRating),
            Some(v) => self.set_property(PropertyInt::NetherResistRating, v),
        }
    }

    // ACE: WorldObject.PKDamageRating
    pub fn pk_damage_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::PKDamageRating)
    }

    // ACE: WorldObject.PKDamageRating
    pub fn set_pk_damage_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PKDamageRating),
            Some(v) => self.set_property(PropertyInt::PKDamageRating, v),
        }
    }

    // ACE: WorldObject.PKDamageResistRating
    pub fn pk_damage_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::PKDamageResistRating)
    }

    // ACE: WorldObject.PKDamageResistRating
    pub fn set_pk_damage_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PKDamageResistRating),
            Some(v) => self.set_property(PropertyInt::PKDamageResistRating, v),
        }
    }

    // ACE: WorldObject.Lifespan
    pub fn lifespan(&self) -> Option<i32> {
        self.get_property(PropertyInt::Lifespan)
    }

    // ACE: WorldObject.Lifespan
    pub fn set_lifespan(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Lifespan),
            Some(v) => self.set_property(PropertyInt::Lifespan, v),
        }
    }

    // ACE: WorldObject.HearLocalSignals
    pub fn hear_local_signals(&self) -> bool {
        self.get_property(PropertyInt::HearLocalSignals)
            .unwrap_or(0)
            != 0
    }

    // ACE: WorldObject.HearLocalSignals
    pub fn set_hear_local_signals(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyInt::HearLocalSignals);
        } else {
            self.set_property(PropertyInt::HearLocalSignals, 1);
        }
    }

    // ACE: WorldObject.HearLocalSignalsRadius
    pub fn hear_local_signals_radius(&self) -> i32 {
        self.get_property(PropertyInt::HearLocalSignalsRadius)
            .unwrap_or(0)
    }

    // ACE: WorldObject.HearLocalSignalsRadius
    pub fn set_hear_local_signals_radius(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::HearLocalSignalsRadius);
        } else {
            self.set_property(PropertyInt::HearLocalSignalsRadius, value);
        }
    }

    // ACE: WorldObject.TinkerLog
    pub fn tinker_log(&self) -> Option<String> {
        self.get_property(PropertyString::TinkerLog)
    }

    // ACE: WorldObject.TinkerLog
    pub fn set_tinker_log(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::TinkerLog),
            Some(v) => self.set_property(PropertyString::TinkerLog, v),
        }
    }

    // ACE: WorldObject.CreatureKills
    pub fn creature_kills(&self) -> Option<i32> {
        self.get_property(PropertyInt::CreatureKills)
    }

    // ACE: WorldObject.CreatureKills
    pub fn set_creature_kills(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CreatureKills),
            Some(v) => self.set_property(PropertyInt::CreatureKills, v),
        }
    }

    // ACE: WorldObject.PlayerKillsPk
    pub fn player_kills_pk(&self) -> Option<i32> {
        self.get_property(PropertyInt::PlayerKillsPk)
    }

    // ACE: WorldObject.PlayerKillsPk
    pub fn set_player_kills_pk(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PlayerKillsPk),
            Some(v) => self.set_property(PropertyInt::PlayerKillsPk, v),
        }
    }

    // ACE: WorldObject.PlayerKillsPkl
    pub fn player_kills_pkl(&self) -> Option<i32> {
        self.get_property(PropertyInt::PlayerKillsPkl)
    }

    // ACE: WorldObject.PlayerKillsPkl
    pub fn set_player_kills_pkl(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::PlayerKillsPkl),
            Some(v) => self.set_property(PropertyInt::PlayerKillsPkl, v),
        }
    }

    // ACE: WorldObject.SummoningMastery
    pub fn summoning_mastery(&self) -> Option<SummoningMastery> {
        self.get_property(PropertyInt::SummoningMastery)
            .map(SummoningMastery)
    }

    // ACE: WorldObject.SummoningMastery
    pub fn set_summoning_mastery(&mut self, value: Option<SummoningMastery>) {
        match value {
            None => self.remove_property(PropertyInt::SummoningMastery),
            Some(v) => self.set_property(PropertyInt::SummoningMastery, v.0),
        }
    }

    // ACE: WorldObject.MaximumVelocity
    pub fn maximum_velocity(&self) -> Option<f64> {
        self.get_property(PropertyFloat::MaximumVelocity)
    }

    // ACE: WorldObject.MaximumVelocity
    pub fn set_maximum_velocity(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::MaximumVelocity),
            Some(v) => self.set_property(PropertyFloat::MaximumVelocity, v),
        }
    }

    // ACE: WorldObject.Unique
    pub fn unique(&self) -> Option<i32> {
        self.get_property(PropertyInt::Unique)
    }

    // ACE: WorldObject.Unique
    pub fn set_unique(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Unique),
            Some(v) => self.set_property(PropertyInt::Unique, v),
        }
    }

    // ACE: WorldObject.MutateFilter
    pub fn mutate_filter(&self) -> Option<u32> {
        self.get_property(PropertyDataId::MutateFilter)
    }

    // ACE: WorldObject.MutateFilter
    pub fn set_mutate_filter(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::MutateFilter),
            Some(v) => self.set_property(PropertyDataId::MutateFilter, v),
        }
    }

    // ACE: WorldObject.TsysMutationFilter
    pub fn tsys_mutation_filter(&self) -> Option<u32> {
        self.get_property(PropertyDataId::TsysMutationFilter)
    }

    // ACE: WorldObject.TsysMutationFilter
    pub fn set_tsys_mutation_filter(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::TsysMutationFilter),
            Some(v) => self.set_property(PropertyDataId::TsysMutationFilter, v),
        }
    }

    // ACE: WorldObject.CloakWeaveProc
    pub fn cloak_weave_proc(&self) -> Option<i32> {
        self.get_property(PropertyInt::CloakWeaveProc)
    }

    // ACE: WorldObject.CloakWeaveProc
    pub fn set_cloak_weave_proc(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CloakWeaveProc),
            Some(v) => self.set_property(PropertyInt::CloakWeaveProc, v),
        }
    }

    // ACE: WorldObject.GearDamage
    pub fn gear_damage(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearDamage)
    }

    // ACE: WorldObject.GearDamage
    pub fn set_gear_damage(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearDamage),
            Some(v) => self.set_property(PropertyInt::GearDamage, v),
        }
    }

    // ACE: WorldObject.GearDamageResist
    pub fn gear_damage_resist(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearDamageResist)
    }

    // ACE: WorldObject.GearDamageResist
    pub fn set_gear_damage_resist(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearDamageResist),
            Some(v) => self.set_property(PropertyInt::GearDamageResist, v),
        }
    }

    // ACE: WorldObject.GearCritDamage
    pub fn gear_crit_damage(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearCritDamage)
    }

    // ACE: WorldObject.GearCritDamage
    pub fn set_gear_crit_damage(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearCritDamage),
            Some(v) => self.set_property(PropertyInt::GearCritDamage, v),
        }
    }

    // ACE: WorldObject.GearCritDamageResist
    pub fn gear_crit_damage_resist(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearCritDamageResist)
    }

    // ACE: WorldObject.GearCritDamageResist
    pub fn set_gear_crit_damage_resist(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearCritDamageResist),
            Some(v) => self.set_property(PropertyInt::GearCritDamageResist, v),
        }
    }

    // ACE: WorldObject.GearCrit
    pub fn gear_crit(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearCrit)
    }

    // ACE: WorldObject.GearCrit
    pub fn set_gear_crit(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearCrit),
            Some(v) => self.set_property(PropertyInt::GearCrit, v),
        }
    }

    // ACE: WorldObject.GearCritResist
    pub fn gear_crit_resist(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearCritResist)
    }

    // ACE: WorldObject.GearCritResist
    pub fn set_gear_crit_resist(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearCritResist),
            Some(v) => self.set_property(PropertyInt::GearCritResist, v),
        }
    }

    // ACE: WorldObject.GearHealingBoost
    pub fn gear_healing_boost(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearHealingBoost)
    }

    // ACE: WorldObject.GearHealingBoost
    pub fn set_gear_healing_boost(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearHealingBoost),
            Some(v) => self.set_property(PropertyInt::GearHealingBoost, v),
        }
    }

    // ACE: WorldObject.GearMaxHealth
    pub fn gear_max_health(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearMaxHealth)
    }

    // ACE: WorldObject.GearMaxHealth
    pub fn set_gear_max_health(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearMaxHealth),
            Some(v) => self.set_property(PropertyInt::GearMaxHealth, v),
        }
    }

    // ACE: WorldObject.GearPKDamageRating
    pub fn gear_pk_damage_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearPKDamageRating)
    }

    // ACE: WorldObject.GearPKDamageRating
    pub fn set_gear_pk_damage_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearPKDamageRating),
            Some(v) => self.set_property(PropertyInt::GearPKDamageRating, v),
        }
    }

    // ACE: WorldObject.GearPKDamageResistRating
    pub fn gear_pk_damage_resist_rating(&self) -> Option<i32> {
        self.get_property(PropertyInt::GearPKDamageResistRating)
    }

    // ACE: WorldObject.GearPKDamageResistRating
    pub fn set_gear_pk_damage_resist_rating(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::GearPKDamageResistRating),
            Some(v) => self.set_property(PropertyInt::GearPKDamageResistRating, v),
        }
    }

    // ACE: WorldObject.ResistItemAppraisal
    pub fn resist_item_appraisal(&self) -> Option<i32> {
        self.get_property(PropertyInt::ResistItemAppraisal)
    }

    // ACE: WorldObject.ResistItemAppraisal
    pub fn set_resist_item_appraisal(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ResistItemAppraisal),
            Some(v) => self.set_property(PropertyInt::ResistItemAppraisal, v),
        }
    }

    // ACE: WorldObject.HookGroup
    pub fn hook_group(&self) -> Option<HookGroupType> {
        self.get_property(PropertyInt::HookGroup).map(HookGroupType)
    }

    // ACE: WorldObject.HookGroup
    pub fn set_hook_group(&mut self, value: Option<HookGroupType>) {
        match value {
            None => self.remove_property(PropertyInt::HookGroup),
            Some(v) => self.set_property(PropertyInt::HookGroup, v.0),
        }
    }

    // ACE: WorldObject.ItemAttributeLimit
    pub fn item_attribute_limit(&self) -> Option<PropertyAttribute> {
        self.get_property(PropertyInt::ItemAttributeLimit)
            .map(|v| PropertyAttribute(v.cs_cast()))
    }

    // ACE: WorldObject.ItemAttributeLimit
    pub fn set_item_attribute_limit(&mut self, value: Option<PropertyAttribute>) {
        match value {
            None => self.remove_property(PropertyInt::ItemAttributeLimit),
            Some(v) => self.set_property(PropertyInt::ItemAttributeLimit, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.ItemAttributeLevelLimit
    pub fn item_attribute_level_limit(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemAttributeLevelLimit)
    }

    // ACE: WorldObject.ItemAttributeLevelLimit
    pub fn set_item_attribute_level_limit(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemAttributeLevelLimit),
            Some(v) => self.set_property(PropertyInt::ItemAttributeLevelLimit, v),
        }
    }

    // ACE: WorldObject.ItemAttribute2ndLimit
    pub fn item_attribute2nd_limit(&self) -> Option<PropertyAttribute2nd> {
        self.get_property(PropertyInt::ItemAttribute2ndLimit)
            .map(|v| PropertyAttribute2nd(v.cs_cast()))
    }

    // ACE: WorldObject.ItemAttribute2ndLimit
    pub fn set_item_attribute2nd_limit(&mut self, value: Option<PropertyAttribute2nd>) {
        match value {
            None => self.remove_property(PropertyInt::ItemAttribute2ndLimit),
            Some(v) => self.set_property(PropertyInt::ItemAttribute2ndLimit, v.0.cs_cast()),
        }
    }

    // ACE: WorldObject.ItemAttribute2ndLevelLimit
    pub fn item_attribute2nd_level_limit(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemAttribute2ndLevelLimit)
    }

    // ACE: WorldObject.ItemAttribute2ndLevelLimit
    pub fn set_item_attribute2nd_level_limit(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemAttribute2ndLevelLimit),
            Some(v) => self.set_property(PropertyInt::ItemAttribute2ndLevelLimit, v),
        }
    }

    // ACE: WorldObject.SoldTimestamp
    pub fn sold_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::SoldTimestamp)
    }

    // ACE: WorldObject.SoldTimestamp
    pub fn set_sold_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::SoldTimestamp),
            Some(v) => self.set_property(PropertyFloat::SoldTimestamp, v),
        }
    }

    // ACE: WorldObject.AllowGive
    pub fn allow_give(&self) -> bool {
        self.get_property(PropertyBool::AllowGive).unwrap_or(false)
    }

    // ACE: WorldObject.AllowGive
    pub fn set_allow_give(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AllowGive);
        } else {
            self.set_property(PropertyBool::AllowGive, value);
        }
    }

    // ACE: WorldObject.AiAcceptEverything
    pub fn ai_accept_everything(&self) -> bool {
        self.get_property(PropertyBool::AiAcceptEverything)
            .unwrap_or(false)
    }

    // ACE: WorldObject.AiAcceptEverything
    pub fn set_ai_accept_everything(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AiAcceptEverything);
        } else {
            self.set_property(PropertyBool::AiAcceptEverything, value);
        }
    }

    // ACE: WorldObject.ImbuedEffect
    pub fn imbued_effect(&self) -> ImbuedEffectType {
        ImbuedEffectType(
            self.get_property(PropertyInt::ImbuedEffect)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: WorldObject.ImbuedEffect
    pub fn set_imbued_effect(&mut self, value: ImbuedEffectType) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::ImbuedEffect);
        } else {
            self.set_property(PropertyInt::ImbuedEffect, value.0.cs_cast());
        }
    }

    // ACE: WorldObject.DontTurnOrMoveWhenGiving
    pub fn dont_turn_or_move_when_giving(&self) -> bool {
        self.get_property(PropertyBool::DontTurnOrMoveWhenGiving)
            .unwrap_or(false)
    }

    // ACE: WorldObject.DontTurnOrMoveWhenGiving
    pub fn set_dont_turn_or_move_when_giving(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::DontTurnOrMoveWhenGiving);
        } else {
            self.set_property(PropertyBool::DontTurnOrMoveWhenGiving, value);
        }
    }

    // ACE: WorldObject.RotationSpeed
    pub fn rotation_speed(&self) -> Option<f64> {
        self.get_property(PropertyFloat::RotationSpeed)
    }

    // ACE: WorldObject.RotationSpeed
    pub fn set_rotation_speed(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::RotationSpeed),
            Some(v) => self.set_property(PropertyFloat::RotationSpeed, v),
        }
    }
}
