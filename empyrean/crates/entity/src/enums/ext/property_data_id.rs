// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyDataId.cs

use crate::enums::*;

/// `System.Enum.GetName(typeof(E), value)` with ACE's `uint` argument.
fn name<E: AceEnum>(value: u32) -> Option<String> {
    get_name::<E>(i64::from(value)).map(str::to_owned)
}

impl PropertyDataId {
    /// The name of `value` in the enum this property holds, for display.
    // ACE: PropertyDataIdExtensions.GetValueEnumName
    pub fn get_value_enum_name(self, value: u32) -> Option<String> {
        match self {
            PropertyDataId::ActivationAnimation
            | PropertyDataId::InitMotion
            | PropertyDataId::UseTargetAnimation
            | PropertyDataId::UseTargetFailureAnimation
            | PropertyDataId::UseTargetSuccessAnimation
            | PropertyDataId::UseUserAnimation => return name::<MotionCommand>(value),
            PropertyDataId::PhysicsScript | PropertyDataId::RestrictionEffect => {
                return name::<PlayScript>(value)
            }
            PropertyDataId::ActivationSound | PropertyDataId::UseSound => {
                return name::<Sound>(value)
            }
            PropertyDataId::WieldedTreasureType
            | PropertyDataId::DeathTreasureType
            | PropertyDataId::InventoryTreasureType
            | PropertyDataId::ShopTreasureType => {
                // ACE source has `// todo` then `break` here: these fall through to `return null`.
            }
            PropertyDataId::Spell
            | PropertyDataId::DeathSpell
            | PropertyDataId::ProcSpell
            | PropertyDataId::RedSurgeSpell
            | PropertyDataId::BlueSurgeSpell
            | PropertyDataId::YellowSurgeSpell => return name::<SpellId>(value),
            PropertyDataId::ItemSkillLimit | PropertyDataId::ItemSpecializedOnly => {
                return name::<Skill>(value)
            }
            PropertyDataId::PCAPRecordedParentLocation => return name::<ParentLocation>(value),
            PropertyDataId::PCAPRecordedDefaultScript => return name::<MotionCommand>(value),
            _ => {}
        }

        None
    }

    // ACE: PropertyDataIdExtensions.IsHexData
    pub fn is_hex_data(self) -> bool {
        match self {
            PropertyDataId::AccountHouseId
            | PropertyDataId::AlternateCurrency
            | PropertyDataId::AugmentationCreateItem
            | PropertyDataId::AugmentationEffect
            | PropertyDataId::BlueSurgeSpell
            | PropertyDataId::DeathSpell
            | PropertyDataId::DeathTreasureType
            | PropertyDataId::HouseId
            | PropertyDataId::ItemSkillLimit
            | PropertyDataId::ItemSpecializedOnly
            | PropertyDataId::InventoryTreasureType
            | PropertyDataId::LastPortal
            | PropertyDataId::LinkedPortalOne
            | PropertyDataId::LinkedPortalTwo
            | PropertyDataId::OlthoiDeathTreasureType
            | PropertyDataId::OriginalPortal
            | PropertyDataId::PhysicsScript
            | PropertyDataId::ProcSpell
            | PropertyDataId::RedSurgeSpell
            | PropertyDataId::RestrictionEffect
            | PropertyDataId::ShopTreasureType
            | PropertyDataId::Spell
            | PropertyDataId::SpellComponent
            | PropertyDataId::UseCreateItem
            | PropertyDataId::UseSound
            | PropertyDataId::VendorsClassId
            | PropertyDataId::WieldedTreasureType
            | PropertyDataId::YellowSurgeSpell => false,

            x if x >= PropertyDataId::PCAPRecordedWeenieHeader => false,

            _ => true,
        }
    }
}
