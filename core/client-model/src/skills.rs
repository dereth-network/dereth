//! The skill table, the skill query's bonus stack, augmentations, and the two movement formulas
//! skills drive.
//!
//! The pure rules live in [`dereth_rules::skills`] and are re-exported here, so every
//! `dereth_client_model::skills::*` path resolves. So do the inquiries (`inq_skill` and the
//! rest), written against `dereth_rules::quality::QualityRead`, which
//! [`Qualities`](crate::qualities::Qualities) implements.

#[cfg(test)]
use crate::qualities::Qualities;
#[cfg(test)]
use dereth_assets::tables::SkillTable;

pub use dereth_rules::skills::*;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use dereth_assets::tables::{SkillBase, SkillFormula};
    use dereth_primitives::DataId;
    use dereth_protocol::types::qualities::{Attribute, AttributeCache, Skill};
    use std::collections::BTreeMap;

    fn table() -> SkillTable {
        let mut skills = BTreeMap::new();
        let base = |min_level: u32| SkillBase {
            description: String::new(),
            name: String::new(),
            icon: 0,
            trained_cost: 4,
            specialized_cost: 8,
            category: skill_category::WEAPON,
            chargen_use: 0,
            min_level,
            // ( Coordination + Quickness ) / 3
            formula: SkillFormula {
                w: 0,
                x: 1,
                y: 1,
                z: 3,
                attr1: 4,
                attr2: 3,
            },
            upper_bound: 0.0,
            lower_bound: 0.0,
            learn_mod: 0.0,
        };
        skills.insert(skill::WAR_MAGIC, base(2));
        skills.insert(skill::MISSILE_WEAPONS, base(1));
        skills.insert(skill::RUN, base(1));
        skills.insert(skill::JUMP, base(1));
        SkillTable {
            id: DataId(0x0E00_0004),
            buckets: 11,
            skills,
        }
    }

    fn player(coord: u32, quick: u32) -> Qualities {
        let mut q = Qualities::new();
        q.attributes = Some(AttributeCache {
            flags: 0x1FF,
            quickness: Some(Attribute {
                init_level: quick,
                level_from_cp: 0,
                cp_spent: 0,
            }),
            coordination: Some(Attribute {
                init_level: coord,
                level_from_cp: 0,
                cp_spent: 0,
            }),
            ..AttributeCache::default()
        });
        q
    }

    fn skill_rec(sac: u32, level_from_pp: u16, init: u32) -> Skill {
        Skill {
            level_from_pp,
            format_version: 1,
            sac,
            pp: 0,
            init_level: init,
            resistance_of_last_check: 0,
            last_used_time: 0.0,
        }
    }

    /// Oracle: `12-skills-and-advancement.md` §3.3 — a skill whose `_sac` is below the
    /// `SkillBase::_min_level` evaluates to 0, not to the formula.
    #[test]
    fn an_untrained_skill_below_min_level_is_zero() {
        let t = table();
        let mut q = player(150, 150);
        // War Magic has min_level = TRAINED and the character is untrained.
        assert_eq!(
            inq_skill_base_level(&q, &t, skill::WAR_MAGIC, true),
            Some(0)
        );
        q.set_skill(skill::WAR_MAGIC, skill_rec(Sac::Trained as u32, 0, 0));
        assert_eq!(
            inq_skill_base_level(&q, &t, skill::WAR_MAGIC, true),
            Some(100)
        );
    }

    /// Oracle: §3.4's transcription of, bonus by bonus and in order.
    #[test]
    fn inq_skill_stacks_the_bonuses_in_the_documented_order() {
        use crate::qualities::{StatKey, StatType, StatValue};
        let t = table();
        let mut q = player(150, 150); // (150 + 150) / 3 = 100
        q.set_skill(
            skill::MISSILE_WEAPONS,
            skill_rec(Sac::Specialized as u32, 40, 10),
        );
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, true), Some(150));
        assert_eq!(inq_skill_level(&q, skill::MISSILE_WEAPONS), 50);

        // LumAugAllSkills is additive before the augmentation +10.
        q.set(
            StatKey::new(StatType::Int, aug::LUM_ALL_SKILLS),
            StatValue::Int(7),
        );
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, true), Some(157));

        // AugmentationSkilledMissile grants a flat +10 to this skill and not to War Magic.
        q.set(
            StatKey::new(StatType::Int, aug::SKILLED_MISSILE),
            StatValue::Int(1),
        );
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, true), Some(167));

        // raw = false adds JoAT +5 and 2x the spec-luminance count, but only after enchantment.
        q.set(
            StatKey::new(StatType::Int, aug::JACK_OF_ALL_TRADES),
            StatValue::Int(1),
        );
        q.set(
            StatKey::new(StatType::Int, aug::LUM_SKILLED_SPEC),
            StatValue::Int(3),
        );
        assert_eq!(
            inq_skill(&q, &t, skill::MISSILE_WEAPONS, false),
            Some(167 + 5 + 6)
        );
    }

    /// One multiplicative enchantment on `family`/`key`, for the enlightenment tests.
    pub(crate) fn multiplier(family: u32, key: u32, value: f32) -> crate::enchant::Enchantment {
        crate::enchant::Enchantment {
            id: 1,
            spell_category: 1,
            power_level: 1,
            start_time: 0.0,
            duration: -1.0,
            caster: dereth_primitives::ObjectId(1),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: dereth_protocol::types::qualities::StatMod {
                kind: crate::enchant::ench_type::MULTIPLICATIVE
                    | crate::enchant::ench_type::SINGLE_STAT
                    | family,
                key,
                value,
            },
            spell_set_id: None,
        }
    }

    /// Enlightenment (property 390): absent, nothing changes; above zero, its count lands on every
    /// trained and specialized skill and on no untrained one, and it is part of the base, so a
    /// multiplicative enchantment scales it.
    ///
    /// Behaviour: skills.enlightenment.each-level-adds-one-to-every-trained-and-specialized-skill
    #[test]
    fn enlightenment_adds_to_trained_and_specialized_skills_before_the_multiplier() {
        use crate::qualities::{StatKey, StatType, StatValue};
        let t = table();
        let mut q = player(150, 150); // (150 + 150) / 3 = 100
        q.set_skill(
            skill::MISSILE_WEAPONS,
            skill_rec(Sac::Specialized as u32, 40, 10),
        );
        q.set_skill(skill::WAR_MAGIC, skill_rec(Sac::Trained as u32, 5, 0));
        q.set_skill(skill::RUN, skill_rec(Sac::Untrained as u32, 0, 0));
        let before: Vec<_> = [skill::MISSILE_WEAPONS, skill::WAR_MAGIC, skill::RUN]
            .iter()
            .map(|&id| (inq_skill(&q, &t, id, true), inq_skill(&q, &t, id, false)))
            .collect();
        assert_eq!(before[0], (Some(150), Some(150)));
        assert_eq!(before[1], (Some(105), Some(105)));
        assert_eq!(before[2], (Some(100), Some(100)));

        // A zero count is the same as no property.
        q.set(
            StatKey::new(StatType::Int, aug::ENLIGHTENMENT),
            StatValue::Int(0),
        );
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, false), Some(150));

        q.set(
            StatKey::new(StatType::Int, aug::ENLIGHTENMENT),
            StatValue::Int(3),
        );
        assert_eq!(
            inq_skill(&q, &t, skill::MISSILE_WEAPONS, true),
            Some(153),
            "specialized"
        );
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, false), Some(153));
        assert_eq!(
            inq_skill(&q, &t, skill::WAR_MAGIC, true),
            Some(108),
            "trained"
        );
        assert_eq!(
            inq_skill(&q, &t, skill::RUN, true),
            Some(100),
            "untrained gets nothing"
        );
        assert_eq!(inq_skill(&q, &t, skill::RUN, false), Some(100));

        // x1.5 on Missile Weapons: (150 + 3) * 1.5 = 229.5, rounded half up to 230; the raw
        // query is untouched.
        q.enchantments.mult_list.push(multiplier(
            crate::enchant::ench_type::SKILL,
            skill::MISSILE_WEAPONS,
            1.5,
        ));
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, false), Some(230));
        assert_eq!(inq_skill(&q, &t, skill::MISSILE_WEAPONS, true), Some(153));

        // Run and Jump take it too once trained, which is what the run rate and the jump
        // velocity read.
        q.set_skill(skill::RUN, skill_rec(Sac::Trained as u32, 0, 0));
        q.set_skill(skill::JUMP, skill_rec(Sac::Specialized as u32, 0, 0));
        let run = inq_skill(&q, &t, skill::RUN, true).expect("run");
        let jump = inq_skill(&q, &t, skill::JUMP, true).expect("jump");
        q.set(
            StatKey::new(StatType::Int, aug::ENLIGHTENMENT),
            StatValue::Int(0),
        );
        assert_eq!(inq_skill(&q, &t, skill::RUN, true), Some(run - 3));
        assert_eq!(inq_skill(&q, &t, skill::JUMP, true), Some(jump - 3));
    }

    /// Oracle: the run-rate enquiry — with no stamina, the run skill counts as zero.
    #[test]
    fn no_stamina_means_no_running() {
        let t = table();
        let mut q = player(150, 150);
        q.set_skill(skill::RUN, skill_rec(Sac::Trained as u32, 100, 0));
        let stamina = |q: &mut Qualities, current: u32| {
            q.attributes.as_mut().unwrap().stamina =
                Some(dereth_protocol::types::qualities::SecondaryAttribute {
                    attribute: Attribute {
                        init_level: 100,
                        level_from_cp: 0,
                        cp_spent: 0,
                    },
                    current_level: current,
                });
        };
        stamina(&mut q, 50);
        let with = inq_run_rate(&q, &t, None).unwrap();
        stamina(&mut q, 0);
        let without = inq_run_rate(&q, &t, None).unwrap();
        assert_eq!(without, get_run_rate(0.0, 0, 1.0));
        assert!(with > without);

        // A stamina inquiry that fails is not a valid zero, exactly as `inq_jump_skill` has it:
        // the run-rate inquiry returns 0 without ever reaching `GetRunRate`.
        q.attributes.as_mut().unwrap().stamina = None;
        assert_eq!(inq_run_rate(&q, &t, None), None);
        stamina(&mut q, 0);
        let mut missing_run = t.clone();
        missing_run.skills.remove(&skill::RUN);
        assert_eq!(
            inq_run_rate(&q, &missing_run, None),
            None,
            "zero stamina does not bypass a required failed Run skill inquiry"
        );
    }
}
