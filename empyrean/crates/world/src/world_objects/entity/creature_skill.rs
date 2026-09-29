// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Entity/CreatureSkill.cs
//! Port of `Source/ACE.Server/WorldObjects/Entity/CreatureSkill.cs`.
//!
//! ACE's `CreatureSkill` wraps the `PropertiesSkill` record of its creature's biota. Here it is a
//! `Copy` handle naming the skill; members take the creature explicitly and read the record from
//! its biota (`Creature.GetCreatureSkill` adds the record before building a handle, so it is
//! always there).

use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_entity::enums::{Skill, SkillAdvancementClass};
use empyrean_entity::models::properties_skill::PropertiesSkill;

use crate::entity::attribute_formula;
use empyrean_entity::ObjectGuid;

use crate::world_objects::entity::creature_attribute::{
    count_i32, em, int_to_f32, uint_to_f32, StatCtx,
};
use crate::world_objects::player_properties::player_vitae;
use crate::world_objects::player_skills;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// The in-world creature `this`.
fn in_world(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Creature {this:?}: missing object"))
}

// ACE: CreatureSkill
/// One skill of a creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CreatureSkill {
    // ACE: CreatureSkill.Skill
    pub skill: Skill,
}

impl CreatureSkill {
    // ACE: CreatureSkill.CreatureSkill
    /// `new CreatureSkill(creature, skill, propertiesSkill)`: the record is the biota's entry for
    /// `skill`, which the caller has already found or added.
    #[must_use]
    pub fn new(skill: Skill) -> Self {
        Self { skill }
    }

    // ACE: CreatureSkill.PropertiesSkill
    /// The underlying database record.
    ///
    /// # Panics
    /// When the biota has no record for this skill.
    #[must_use]
    pub fn properties_skill(self, creature: &WorldObject) -> &PropertiesSkill {
        creature.biota.get_skill(self.skill).unwrap_or_else(|| {
            panic!(
                "CreatureSkill({}): the biota record is gone",
                self.skill.to_dotnet_string()
            )
        })
    }

    pub(crate) fn properties_skill_mut(self, creature: &mut WorldObject) -> &mut PropertiesSkill {
        creature
            .biota
            .properties_skill
            .as_mut()
            .and_then(|d| d.get_mut(&self.skill))
            .unwrap_or_else(|| {
                panic!(
                    "CreatureSkill({}): the biota record is gone",
                    self.skill.to_dotnet_string()
                )
            })
    }

    // ACE: CreatureSkill.InitLevel
    /// A bonus from character creation: +5 for trained, +10 for specialized.
    #[must_use]
    pub fn init_level(self, creature: &WorldObject) -> u32 {
        self.properties_skill(creature).init_level
    }

    /// The `InitLevel` setter.
    pub fn set_init_level(self, creature: &mut WorldObject, value: u32) {
        self.properties_skill_mut(creature).init_level = value;
    }

    // ACE: CreatureSkill.AdvancementClass
    #[must_use]
    pub fn advancement_class(self, creature: &WorldObject) -> SkillAdvancementClass {
        self.properties_skill(creature).sac
    }

    /// The `AdvancementClass` setter: a change sets the creature's `ChangesDetected`.
    pub fn set_advancement_class(self, creature: &mut WorldObject, value: SkillAdvancementClass) {
        if self.properties_skill(creature).sac != value {
            creature.wo.world_object_database.changes_detected = true;
        }

        self.properties_skill_mut(creature).sac = value;
    }

    // ACE: CreatureSkill.IsUsable
    /// Trained and specialized skills are usable; an untrained one when the dat `SkillTable`
    /// gives it a `MinLevel` of 1.
    #[must_use]
    pub fn is_usable(self, w: &World, creature: &WorldObject) -> bool {
        let advancement_class = self.advancement_class(creature);
        if advancement_class == SkillAdvancementClass::Trained
            || advancement_class == SkillAdvancementClass::Specialized
        {
            return true;
        }

        if advancement_class == SkillAdvancementClass::Untrained {
            let key: u32 = self.skill.0.cs_cast();
            let skill_table_record = w.dats.portal_dat().skill_table().skills.get(&key);

            if skill_table_record.map(|r| r.min_level) == Some(1) {
                return true;
            }
        }
        false
    }

    // ACE: CreatureSkill.ExperienceSpent
    /// The experience put into this skill, raised directly and earned through use.
    #[must_use]
    pub fn experience_spent(self, creature: &WorldObject) -> u32 {
        self.properties_skill(creature).pp
    }

    /// The `ExperienceSpent` setter: a change sets the creature's `ChangesDetected`.
    pub fn set_experience_spent(self, creature: &mut WorldObject, value: u32) {
        if self.properties_skill(creature).pp != value {
            creature.wo.world_object_database.changes_detected = true;
        }

        self.properties_skill_mut(creature).pp = value;
    }

    // ACE: CreatureSkill.ExperienceLeft
    /// The skill experience remaining until max rank; 0 without an XP table (below trained).
    #[must_use]
    pub fn experience_left(self, w: &World, creature: &WorldObject) -> u32 {
        let Some(skill_xp_table) =
            player_skills::get_skill_xp_table(w, self.advancement_class(creature))
        else {
            return 0;
        };

        // a player can actually have negative experience remaining,
        // if they had a Trained skill maxed, and then specialized it in skill temple afterwards.

        // (confirmed this is how it was in retail)

        let remaining_xp = i64::from(skill_xp_table[skill_xp_table.len() - 1])
            - i64::from(self.experience_spent(creature));

        0i64.max(remaining_xp).cs_cast()
    }

    // ACE: CreatureSkill.Ranks
    /// The number of times this skill has been raised, derived from `ExperienceSpent`.
    #[must_use]
    pub fn ranks(self, creature: &WorldObject) -> u16 {
        self.properties_skill(creature).level_from_pp
    }

    /// The `Ranks` setter: a change sets the creature's `ChangesDetected`.
    pub fn set_ranks(self, creature: &mut WorldObject, value: u16) {
        if self.properties_skill(creature).level_from_pp != value {
            creature.wo.world_object_database.changes_detected = true;
        }

        self.properties_skill_mut(creature).level_from_pp = value;
    }

    // ACE: CreatureSkill.IsMaxRank
    /// True if this skill has been raised the maximum number of times (false below trained).
    #[must_use]
    pub fn is_max_rank(self, w: &World, creature: &WorldObject) -> bool {
        let Some(skill_xp_table) =
            player_skills::get_skill_xp_table(w, self.advancement_class(creature))
        else {
            return false;
        };

        i32::from(self.ranks(creature)) >= count_i32(skill_xp_table.len()).wrapping_sub(1)
    }

    // ACE: CreatureSkill.Base
    /// The unenchanted formula (when usable), the creation bonus and the ranks; a player adds
    /// [`get_aug_bonus_base`](Self::get_aug_bonus_base).
    #[must_use]
    pub fn base(self, w: &World, creature: &WorldObject) -> u32 {
        let mut total: u32 = 0;

        if self.is_usable(w, creature) {
            total = attribute_formula::get_formula_skill(
                &mut StatCtx::detached(w, creature),
                self.skill,
                false,
            );
        }

        total = total.wrapping_add(
            self.init_level(creature)
                .wrapping_add(u32::from(self.ranks(creature))),
        );

        if creature.is_player() {
            total = total.wrapping_add(self.get_aug_bonus_base(creature));
        }

        total
    }

    // ACE: CreatureSkill.Current
    /// The enchanted formula (when usable), the creation bonus and the ranks, the player's base
    /// augmentations; times the multiplicative enchantments and vitae; plus the player's current
    /// augmentations and the additive enchantments, rounded half away from zero; never below 0.
    /// The creature is `this`, in the world (its caching `EnchantmentManager`).
    ///
    /// # Panics
    /// When `this` is not in the world (ACE: `NullReferenceException`).
    #[must_use]
    pub fn current(self, w: &mut World, this: ObjectGuid) -> u32 {
        let mut total: u32 = 0;

        if self.is_usable(w, in_world(w, this)) {
            total = attribute_formula::get_formula_skill(
                &mut StatCtx::in_world(w, this),
                self.skill,
                true,
            );
        }

        let creature = in_world(w, this);
        total = total.wrapping_add(
            self.init_level(creature)
                .wrapping_add(u32::from(self.ranks(creature))),
        );

        let player = creature.is_player();

        // base gets scaled by vitae
        if player {
            total = total.wrapping_add(self.get_aug_bonus_base(creature));
        }

        // apply multiplicative enchantments
        let multiplier = em::get_skill_mod_multiplier(w, this, self.skill);

        let mut f_total = uint_to_f32(total) * multiplier;

        if player {
            let vitae = player_vitae(&StatCtx::in_world(w, this));

            #[allow(clippy::float_cmp)]
            if vitae != 1.0f32 {
                f_total *= vitae;
            }

            // everything beyond this point does not get scaled by vitae
            f_total += uint_to_f32(self.get_aug_bonus_current(in_world(w, this)));
        }

        let additives = em::get_skill_mod_additives(w, this, self.skill);

        let mut i_total = float_extensions::round(f_total + int_to_f32(additives), 0);

        i_total = i_total.max(0); // skill level cannot be debuffed below 0

        i_total.cs_cast()
    }

    // ACE: CreatureSkill.GetAugBonus_Base
    /// Luminance all-skills, one skilled-family augmentation (x10), and enlightenment for a
    /// trained or specialized skill. `player` is the creature, which must be a Player.
    #[must_use]
    pub fn get_aug_bonus_base(self, player: &WorldObject) -> u32 {
        // TODO: verify which of these are base, and which are current
        let mut total: u32 = 0;

        // **Retail's gate, not ACE's (V245):** added only when positive, as the
        // client does. ACE tests `!= 0` and casts a negative count to uint, so the sums wrapped.
        if player.lum_aug_all_skills() > 0 {
            total = total.wrapping_add(player.lum_aug_all_skills().cs_cast());
        }

        if player.augmentation_skilled_melee() > 0
            && player_skills::MELEE_SKILLS.contains(&self.skill)
        {
            total = total.wrapping_add(
                player
                    .augmentation_skilled_melee()
                    .wrapping_mul(10)
                    .cs_cast(),
            );
        } else if player.augmentation_skilled_missile() > 0
            && player_skills::MISSILE_SKILLS.contains(&self.skill)
        {
            total = total.wrapping_add(
                player
                    .augmentation_skilled_missile()
                    .wrapping_mul(10)
                    .cs_cast(),
            );
        } else if player.augmentation_skilled_magic() > 0
            && player_skills::MAGIC_SKILLS.contains(&self.skill)
        {
            total = total.wrapping_add(
                player
                    .augmentation_skilled_magic()
                    .wrapping_mul(10)
                    .cs_cast(),
            );
        }

        // **Retail's gate, not ACE's (V244):** Enlightenment is added only when
        // positive, as the client does; ACE tests `!= 0` and added a negative count.
        if self.advancement_class(player) >= SkillAdvancementClass::Trained
            && player.enlightenment() > 0
        {
            total = total.wrapping_add(player.enlightenment().cs_cast());
        }

        total
    }

    // ACE: CreatureSkill.GetAugBonus_Current
    /// Jack of All Trades (x5) and, for a specialized skill, luminance skilled-spec (x2).
    #[must_use]
    pub fn get_aug_bonus_current(self, player: &WorldObject) -> u32 {
        // TODO: verify which of these are base, and which are current
        let mut total: u32 = 0;

        if player.augmentation_jack_of_all_trades() != 0 {
            total = total.wrapping_add(
                player
                    .augmentation_jack_of_all_trades()
                    .wrapping_mul(5)
                    .cs_cast(),
            );
        }

        // **Retail's gate, not ACE's (V245):** added only when positive, as the
        // client does. ACE tests `!= 0` and casts a negative count to uint before `* 2`, so the sum
        // wrapped.
        if self.advancement_class(player) == SkillAdvancementClass::Specialized
            && player.lum_aug_skilled_spec() > 0
        {
            let lum: u32 = player.lum_aug_skilled_spec().cs_cast();
            total = total.wrapping_add(lum.wrapping_mul(2));
        }

        total
    }
}
