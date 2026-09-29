// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Skills.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Skills.cs`.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{MagicSchool, Skill, SkillAdvancementClass};

use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Creature_Skills.cs`.
#[derive(Debug, Default)]
pub struct CreatureSkillsFields {
    // ACE: Creature.Skills
    /// The skills wrapped so far: the constructor wraps every biota skill, and
    /// [`WorldObject::get_creature_skill`] adds the rest on demand (insertion order is kept, as
    /// `Player.SpendAllXp` iterates it).
    pub skills: DotNetDict<Skill, CreatureSkill>,
}

impl WorldObject {
    /// `Creature.Skills`.
    ///
    /// # Panics
    /// When this object is not a Creature.
    #[must_use]
    pub fn skills(&self) -> &DotNetDict<Skill, CreatureSkill> {
        &self
            .creature
            .as_ref()
            .expect("Creature.Skills on an object that is not a Creature")
            .creature_skills
            .skills
    }

    /// `Creature.Skills`, for the constructor.
    ///
    /// # Panics
    /// When this object is not a Creature.
    pub fn skills_mut(&mut self) -> &mut DotNetDict<Skill, CreatureSkill> {
        &mut self
            .creature
            .as_mut()
            .expect("Creature.Skills on an object that is not a Creature")
            .creature_skills
            .skills
    }

    // ACE: Creature.GetCreatureSkill
    /// The `CreatureSkill` wrapper around the biota's record for `skill`. With `add`, a skill the
    /// biota lacks is added as Untrained (setting `ChangesDetected`); without, `None`.
    pub fn get_creature_skill(&mut self, skill: Skill, add: bool) -> Option<CreatureSkill> {
        if let Some(value) = self.skills().get(&skill) {
            return Some(*value);
        }

        if add {
            let (properties_skill, skill_added) = self.biota.get_or_add_skill(skill);

            if skill_added {
                properties_skill.sac = SkillAdvancementClass::Untrained;
                self.wo.world_object_database.changes_detected = true;
            }

            self.skills_mut().insert(skill, CreatureSkill::new(skill));
        } else {
            let properties_skill = self.biota.get_skill(skill);

            if properties_skill.is_some() {
                self.skills_mut().insert(skill, CreatureSkill::new(skill));
            } else {
                return None;
            }
        }

        self.skills().get(&skill).copied()
    }

    // ACE: Creature.GetCreatureSkill
    /// The `MagicSchool` overload: the school's skill (added if absent); `None` for other schools.
    pub fn get_creature_skill_school(&mut self, skill: MagicSchool) -> Option<CreatureSkill> {
        match skill {
            MagicSchool::CreatureEnchantment => {
                self.get_creature_skill(Skill::CreatureEnchantment, true)
            }
            MagicSchool::ItemEnchantment => self.get_creature_skill(Skill::ItemEnchantment, true),
            MagicSchool::LifeMagic => self.get_creature_skill(Skill::LifeMagic, true),
            MagicSchool::VoidMagic => self.get_creature_skill(Skill::VoidMagic, true),
            MagicSchool::WarMagic => self.get_creature_skill(Skill::WarMagic, true),
            _ => None,
        }
    }
}

// ACE: Creature.GetCurrentLoyalty
/// The IPlayer wrapper the AllegianceManager uses for passup.
pub fn get_current_loyalty(w: &mut World, this: empyrean_entity::ObjectGuid) -> u32 {
    current_of(w, this, Skill::Loyalty)
}

// ACE: Creature.GetCurrentLeadership
/// The IPlayer wrapper the AllegianceManager uses for passup.
pub fn get_current_leadership(w: &mut World, this: empyrean_entity::ObjectGuid) -> u32 {
    current_of(w, this, Skill::Leadership)
}

/// `GetCreatureSkill(skill).Current`.
fn current_of(w: &mut World, this: empyrean_entity::ObjectGuid, skill: Skill) -> u32 {
    let cs = w
        .objects
        .get_mut(this)
        .expect("Creature: missing object")
        .get_creature_skill(skill, true)
        .expect("GetCreatureSkill(skill, add: true) always answers");
    cs.current(w, this)
}
