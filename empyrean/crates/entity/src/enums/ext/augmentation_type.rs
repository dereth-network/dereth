// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AugmentationType.cs

/// ACE's `AugTypeHelper` static class. Its members are plain static functions in ACE, not
/// extension methods, so they stay free functions here.
pub mod aug_type_helper {
    use crate::enums::{AugmentationType, PlayScript, PropertyAttribute, Skill};

    /// Whether this augmentation belongs to the 'innate attribute' family (shared cap of 10).
    // ACE: AugTypeHelper.IsAttribute
    pub fn is_attribute(r#type: AugmentationType) -> bool {
        r#type >= AugmentationType::Strength && r#type <= AugmentationType::Self_
    }

    /// Whether this augmentation belongs to the 'resistance' family (shared cap of 2).
    // ACE: AugTypeHelper.IsResist
    pub fn is_resist(r#type: AugmentationType) -> bool {
        r#type >= AugmentationType::ResistSlash && r#type <= AugmentationType::ResistElectric
    }

    /// Whether this augmentation specializes a skill.
    // ACE: AugTypeHelper.IsSkill
    pub fn is_skill(r#type: AugmentationType) -> bool {
        matches!(
            r#type,
            AugmentationType::Salvage
                | AugmentationType::ItemTinkering
                | AugmentationType::ArmorTinkering
                | AugmentationType::MagicItemTinkering
                | AugmentationType::WeaponTinkering
        )
    }

    // ACE: AugTypeHelper.GetAttribute
    pub fn get_attribute(r#type: AugmentationType) -> PropertyAttribute {
        match r#type {
            AugmentationType::Strength => PropertyAttribute::Strength,
            AugmentationType::Endurance => PropertyAttribute::Endurance,
            AugmentationType::Coordination => PropertyAttribute::Coordination,
            AugmentationType::Quickness => PropertyAttribute::Quickness,
            AugmentationType::Focus => PropertyAttribute::Focus,
            AugmentationType::Self_ => PropertyAttribute::Self_,
            _ => PropertyAttribute::Undef,
        }
    }

    // ACE: AugTypeHelper.GetSkill
    pub fn get_skill(r#type: AugmentationType) -> Skill {
        match r#type {
            AugmentationType::Salvage => Skill::Salvaging,
            AugmentationType::ItemTinkering => Skill::ItemTinkering,
            AugmentationType::ArmorTinkering => Skill::ArmorTinkering,
            AugmentationType::MagicItemTinkering => Skill::MagicItemTinkering,
            AugmentationType::WeaponTinkering => Skill::WeaponTinkering,
            _ => Skill::None,
        }
    }

    // ACE: AugTypeHelper.GetEffect
    pub fn get_effect(r#type: AugmentationType) -> PlayScript {
        if is_attribute(r#type) {
            PlayScript::AugmentationUseAttribute
        } else if is_resist(r#type) {
            PlayScript::AugmentationUseResistances
        } else if is_skill(r#type) {
            PlayScript::AugmentationUseSkill
        } else {
            PlayScript::AugmentationUseOther
        }
    }
}
