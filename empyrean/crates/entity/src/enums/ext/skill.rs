// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Skill.cs

use super::spaced_capitals;
use crate::enums::Skill;

/// ACE's `SkillExtensions` static lists. ACE keeps them in `List<Skill>`; order is kept.
pub mod skill_extensions {
    use crate::enums::Skill;

    // ACE: SkillExtensions.RetiredMelee
    pub const RETIRED_MELEE: &[Skill] = &[
        Skill::Axe,
        Skill::Dagger,
        Skill::Mace,
        Skill::Spear,
        Skill::Staff,
        Skill::Sword,
        Skill::UnarmedCombat,
    ];

    // ACE: SkillExtensions.RetiredMissile
    pub const RETIRED_MISSILE: &[Skill] = &[
        Skill::Bow,
        Skill::Crossbow,
        Skill::Sling,
        Skill::ThrownWeapon,
    ];

    /// `RetiredMelee.Concat(RetiredMissile)`.
    // ACE: SkillExtensions.RetiredWeapons
    pub const RETIRED_WEAPONS: &[Skill] = &[
        Skill::Axe,
        Skill::Dagger,
        Skill::Mace,
        Skill::Spear,
        Skill::Staff,
        Skill::Sword,
        Skill::UnarmedCombat,
        Skill::Bow,
        Skill::Crossbow,
        Skill::Sling,
        Skill::ThrownWeapon,
    ];
}

impl Skill {
    /// Adds a space in front of each capitalised word.
    // ACE: SkillExtensions.ToSentence
    pub fn to_sentence(self) -> String {
        let s = match self {
            Skill::None => "None",
            Skill::Axe => "Axe",
            Skill::Bow => "Bow",
            Skill::Crossbow => "Crossbow",
            Skill::Dagger => "Dagger",
            Skill::Mace => "Mace",
            Skill::MeleeDefense => "Melee Defense",
            Skill::MissileDefense => "Missile Defense",
            Skill::Sling => "Sling",
            Skill::Spear => "Spear",
            Skill::Staff => "Staff",
            Skill::Sword => "Sword",
            Skill::ThrownWeapon => "Thrown Weapon",
            Skill::UnarmedCombat => "Unarmed Combat",
            Skill::ArcaneLore => "Arcane Lore",
            Skill::MagicDefense => "Magic Defense",
            Skill::ManaConversion => "Mana Conversion",
            Skill::Spellcraft => "Spellcraft",
            Skill::ItemTinkering => "Item Tinkering",
            Skill::AssessPerson => "Assess Person",
            Skill::Deception => "Deception",
            Skill::Healing => "Healing",
            Skill::Jump => "Jump",
            Skill::Lockpick => "Lockpick",
            Skill::Run => "Run",
            Skill::Awareness => "Awareness",
            Skill::ArmsAndArmorRepair => "Arms And Armor Repair",
            Skill::AssessCreature => "Assess Creature",
            Skill::WeaponTinkering => "Weapon Tinkering",
            Skill::ArmorTinkering => "Armor Tinkering",
            Skill::MagicItemTinkering => "Magic Item Tinkering",
            Skill::CreatureEnchantment => "Creature Enchantment",
            Skill::ItemEnchantment => "Item Enchantment",
            Skill::LifeMagic => "Life Magic",
            Skill::WarMagic => "War Magic",
            Skill::Leadership => "Leadership",
            Skill::Loyalty => "Loyalty",
            Skill::Fletching => "Fletching",
            Skill::Alchemy => "Alchemy",
            Skill::Cooking => "Cooking",
            Skill::Salvaging => "Salvaging",
            Skill::TwoHandedCombat => "Two Handed Combat",
            Skill::Gearcraft => "Gearcraft",
            Skill::VoidMagic => "Void Magic",
            Skill::HeavyWeapons => "Heavy Weapons",
            Skill::LightWeapons => "Light Weapons",
            Skill::FinesseWeapons => "Finesse Weapons",
            Skill::MissileWeapons => "Missile Weapons",
            Skill::Shield => "Shield",
            Skill::DualWield => "Dual Wield",
            Skill::Recklessness => "Recklessness",
            Skill::SneakAttack => "Sneak Attack",
            Skill::DirtyFighting => "Dirty Fighting",
            Skill::Challenge => "Challenge",
            Skill::Summoning => "Summoning",
            _ => return spaced_capitals(&self.to_dotnet_string()),
        };
        s.into()
    }
}

/// ACE's `SkillHelper` static sets. ACE keeps them in `HashSet<Skill>`, which (with no
/// removals) enumerates in insertion order; the slices keep that order.
pub mod skill_helper {
    use crate::enums::Skill;

    // ACE: SkillHelper.ValidSkills
    pub const VALID_SKILLS: &[Skill] = &[
        Skill::MeleeDefense,
        Skill::MissileDefense,
        Skill::ArcaneLore,
        Skill::MagicDefense,
        Skill::ManaConversion,
        Skill::ItemTinkering,
        Skill::AssessPerson,
        Skill::Deception,
        Skill::Healing,
        Skill::Jump,
        Skill::Lockpick,
        Skill::Run,
        Skill::AssessCreature,
        Skill::WeaponTinkering,
        Skill::ArmorTinkering,
        Skill::MagicItemTinkering,
        Skill::CreatureEnchantment,
        Skill::ItemEnchantment,
        Skill::LifeMagic,
        Skill::WarMagic,
        Skill::Leadership,
        Skill::Loyalty,
        Skill::Fletching,
        Skill::Alchemy,
        Skill::Cooking,
        Skill::Salvaging,
        Skill::TwoHandedCombat,
        Skill::VoidMagic,
        Skill::HeavyWeapons,
        Skill::LightWeapons,
        Skill::FinesseWeapons,
        Skill::MissileWeapons,
        Skill::Shield,
        Skill::DualWield,
        Skill::Recklessness,
        Skill::SneakAttack,
        Skill::DirtyFighting,
        Skill::Summoning,
    ];

    // ACE: SkillHelper.AttackSkills
    pub const ATTACK_SKILLS: &[Skill] = &[
        Skill::Axe,
        Skill::Bow,
        Skill::Crossbow,
        Skill::Dagger,
        Skill::Mace,
        Skill::Sling,
        Skill::Spear,
        Skill::Staff,
        Skill::Sword,
        Skill::ThrownWeapon,
        Skill::UnarmedCombat,
        Skill::FinesseWeapons,
        Skill::HeavyWeapons,
        Skill::LightWeapons,
        Skill::MissileWeapons,
        Skill::TwoHandedCombat,
        Skill::WarMagic,
        Skill::LifeMagic,
        Skill::VoidMagic,
        Skill::DualWield,
    ];

    // ACE: SkillHelper.DefenseSkills
    pub const DEFENSE_SKILLS: &[Skill] = &[
        Skill::MeleeDefense,
        Skill::MissileDefense,
        Skill::MagicDefense,
        Skill::Shield,
    ];
}
