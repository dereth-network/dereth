// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/StarterGearFactory.cs
//! Port of `Source/ACE.Server/Factories/StarterGearFactory.cs`.
//!
//! ACE reads `starterGear.json` from beside its executable when the class is first touched and
//! deserializes it into a `StarterGearConfiguration`. Here the file is compiled in: the server's
//! generator turns ACE's copy into `starter_gear_json::SKILLS` with the
//! deserializer's rules applied (case-sensitive names, strings read as numbers, ACE's
//! `StringToBoolConverter`). The ACE vector harness records what ACE's own deserializer builds from
//! the same file (area `chargen`, `starter_gear`), and the unit tests compare the two.
//!
//! The configuration types are ACE's `Source/ACE.Server/Entity/Starter*.cs` classes. The types
//! are defined here, next to their only user, and can move to `entity/starter_*.rs` unchanged.

mod starter_gear_json;

/// ACE's `StarterGearConfiguration` (`Source/ACE.Server/Entity/StarterGearConfiguration.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterGearConfiguration {
    /// `Skills` (`"skills"`), in file order.
    pub skills: &'static [StarterGearSkill],
}

/// ACE's `StarterGearSkill` (`Source/ACE.Server/Entity/StarterGearSkill.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterGearSkill {
    /// `SkillId` (`"id"`, a `ushort`).
    pub skill_id: u16,
    /// `Name`: not used, but the file has it for readability. `None` is C#'s `null`.
    pub name: Option<&'static str>,
    /// `Gear`.
    pub gear: &'static [StarterItem],
    /// `Heritage`.
    pub heritage: &'static [StarterHeritage],
    /// `Spells`.
    pub spells: &'static [StarterSpell],
}

/// ACE's `StarterHeritage` (`Source/ACE.Server/Entity/StarterHeritage.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterHeritage {
    /// `HeritageId` (`"id"`, a `ushort`).
    pub heritage_id: u16,
    /// `Name`: not used, but the file has it for readability.
    pub name: Option<&'static str>,
    /// `Gear`.
    pub gear: &'static [StarterItem],
    /// `Spells`: "only needed to give an Olthoi Spitter starter spells"; `PlayerFactory` never
    /// reads them.
    pub spells: &'static [StarterSpell],
}

/// ACE's `StarterItem` (`Source/ACE.Server/Entity/StarterItem.cs`). Its constructor sets
/// `StackSize = 1`, the value of an entry without `"stacksize"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterItem {
    /// `WeenieId` (`"weenieId"`).
    pub weenie_id: u32,
    /// `StackSize` (`"stacksize"`, a `ushort`).
    pub stack_size: u16,
}

/// ACE's `StarterSpell` (`Source/ACE.Server/Entity/StarterSpell.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterSpell {
    /// `SpellId` (`"spellId"`).
    pub spell_id: u32,
    /// `Name`: not used, but in the json file for readability.
    pub name: Option<&'static str>,
    /// `SpecializedOnly` (`"specializedOnly"`, through `StringToBoolConverter`).
    pub specialized_only: bool,
}

/// `_config`: the deserialized file. ACE loads it when the class is first used and logs (keeping
/// `null`) when it cannot; the compiled-in table always exists.
// ACE: StarterGearFactory.LoadConfigFromResource
// Not ACE's (a fix, V292): `starterGear.json` spells the key of two heritage
// items (skill 6 for Gear Knight, skill 7 for Tumerok) as "weenieID"; ACE's deserializer matches
// names case-sensitively, so both loaded with `WeenieId = 0` and those characters never received the
// Training Club / Training Spear. The generated table reads both spellings.
// DIVERGE: the file is compiled in by the server's generator instead of being read (or
// copied from /ace/Config in a container) at start-up; an edited starterGear.json needs a rebuild.
static CONFIG: StarterGearConfiguration = StarterGearConfiguration {
    skills: starter_gear_json::SKILLS,
};

// ACE: StarterGearFactory.GetStarterGearConfiguration
/// The configuration `PlayerFactory.Create` grants starter items and spells from. ACE returns
/// `null` when the file failed to load; the compiled-in table cannot fail.
#[must_use]
pub fn get_starter_gear_configuration() -> Option<&'static StarterGearConfiguration> {
    Some(&CONFIG)
}
