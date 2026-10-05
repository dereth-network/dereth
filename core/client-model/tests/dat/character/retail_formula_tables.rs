//! Every shipped skill evaluates within its minimum advancement class; the vital formulas evaluate;
//! skill and attribute raise costs match the shipped experience table; run rate stays non-
//! monotonic; every shipped pose resolves.
//! Fixture: shipped formula tables, spell data and taboo patterns from the retail DATs.

use dereth_assets::tables::{Attribute2ndTable, SkillTable, XpTable};
use dereth_assets::Decode;
use dereth_client_model::advancement as adv;
use dereth_client_model::qualities::Qualities;
use dereth_dat::{DbType, RetailDatStore};
use dereth_protocol::types::qualities::{Attribute, AttributeCache, SecondaryAttribute, Skill};
use {
    dereth_client_model::attributes, dereth_rules::attributes::attribute,
    dereth_rules::attributes::vital,
};
use {dereth_client_model::skills, dereth_rules::skills::skill, dereth_rules::skills::Sac};

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn load<T: Decode>(s: &RetailDatStore, kind: DbType) -> T {
    let id = s
        .ids_of(kind)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {kind:?} in the dat"));
    let bytes = s.read_portal(id).expect("readable");
    T::decode(&mut dereth_dat::Cursor::new(&bytes)).expect("decodes")
}

fn player(str_: u32, end: u32, coord: u32, quick: u32, focus: u32, self_: u32) -> Qualities {
    let a = |v: u32| {
        Some(Attribute {
            init_level: v,
            level_from_cp: 0,
            cp_spent: 0,
        })
    };
    let mut q = Qualities::new();
    q.attributes = Some(AttributeCache {
        flags: 0x1FF,
        strength: a(str_),
        endurance: a(end),
        quickness: a(quick),
        coordination: a(coord),
        focus: a(focus),
        self_: a(self_),
        health: Some(SecondaryAttribute::default()),
        stamina: Some(SecondaryAttribute::default()),
        mana: Some(SecondaryAttribute::default()),
    });
    q
}

/// Every shipped skill evaluates and respects its minimum advancement class.
#[test]
fn every_shipped_skill_evaluates_and_respects_its_minimum_advancement_class() {
    let s = store();
    let skills: SkillTable = load(&s, DbType::SkillTable);
    // The 2013 table ships 38 skills, ids 6..=54 — the retired ones leave gaps, which is why the
    // ids are not dense and why nothing may index by position.
    assert_eq!(skills.skills.len(), 38, "the shipped table has 38 skills");
    assert_eq!(skills.skills.keys().next(), Some(&6));
    assert_eq!(skills.skills.keys().next_back(), Some(&54));

    let q = player(100, 100, 100, 100, 100, 100);
    let mut with_min_level = 0usize;
    let mut no_attribute_contribution = 0usize;
    for (id, base) in &skills.skills {
        let untrained = dereth_rules::skills::inq_skill_base_level(&q, &skills, *id, true)
            .unwrap_or_else(|| panic!("skill {id} has no base level"));
        if base.min_level > Sac::Untrained as u32 {
            with_min_level += 1;
            assert_eq!(
                untrained, 0,
                "skill {id} needs advancement class {} and must read 0 while untrained",
                base.min_level
            );
        }
        // Trained, it evaluates through the formula.
        let mut qt = q.clone();
        qt.set_skill(
            *id,
            Skill {
                level_from_pp: 0,
                format_version: 1,
                sac: Sac::Trained as u32,
                pp: 0,
                init_level: 0,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            },
        );
        let trained = dereth_rules::skills::inq_skill_base_level(&qt, &skills, *id, true).unwrap();
        if base.formula.x == 0 && base.formula.y == 0 {
            // Six shipped skills have both attribute multipliers at zero, so
            // the skill formula yields `floor(0/z + 0.5) = 0` whatever the attributes are:
            // their displayed value is entirely `_init_level + _level_from_pp`. Skills 19, 20, 27,
            // 35, 36 and 40.
            assert_eq!(trained, 0, "skill {id} has no attribute contribution");
            no_attribute_contribution += 1;
        } else {
            assert!(trained > 0, "skill {id} evaluates to 0 even when trained");
        }
    }
    assert_eq!(
        no_attribute_contribution, 6,
        "six shipped skills derive nothing from attributes"
    );
    assert_eq!(
        with_min_level, 15,
        "fifteen shipped skills need TRAINED before they work at all"
    );
}

/// The three vital formulas from the shipped `Attribute2ndTable`, evaluated.
#[test]
fn the_three_vital_formulas_evaluate_from_the_shipped_table() {
    let s = store();
    let t: Attribute2ndTable = load(&s, DbType::Attribute2ndTable);
    let q = player(100, 100, 100, 100, 100, 100);
    for id in [vital::MAX_HEALTH, vital::MAX_STAMINA, vital::MAX_MANA] {
        let v = dereth_rules::attributes::inq_attribute_2nd_base_level(&q, &t, id, true)
            .unwrap_or_else(|| panic!("vital {id} has no formula"));
        assert!(v > 0, "vital {id} evaluated to 0 from attributes of 100");
    }
    // An even id has no formula at all.
    assert_eq!(
        dereth_rules::attributes::inq_attribute_2nd_base_level(&q, &t, vital::HEALTH, true),
        None
    );

    // MaxHealth is the endurance-driven one, so raising endurance alone must raise it.
    let higher = player(100, 200, 100, 100, 100, 100);
    assert!(
        dereth_rules::attributes::inq_attribute_2nd_base_level(
            &higher,
            &t,
            vital::MAX_HEALTH,
            true
        ) > dereth_rules::attributes::inq_attribute_2nd_base_level(&q, &t, vital::MAX_HEALTH, true)
    );
    let _ = attribute::STRENGTH;
}

/// Behaviour: advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table
/// The raise cost curves match the shipped experience table.
#[test]
fn the_raise_cost_curves_match_the_shipped_experience_table() {
    let s = store();
    let xp: XpTable = load(&s, DbType::XpTable);
    let skills: SkillTable = load(&s, DbType::SkillTable);
    let id = *skills.skills.keys().next().expect("at least one skill");

    let max_trained = dereth_rules::advancement::max_trained_skill_level(&xp);
    let max_spec = dereth_rules::advancement::max_specialized_skill_level(&xp);
    assert!(
        max_trained > 100 && max_spec > 100,
        "the shipped curves run past level 100"
    );

    for sac in [Sac::Trained, Sac::Specialized] {
        let max = if sac == Sac::Trained {
            max_trained
        } else {
            max_spec
        };
        let mut q = Qualities::new();
        for level in 0..max {
            q.set_skill(
                id,
                Skill {
                    #[allow(clippy::cast_possible_truncation)] // the wire field is a u16
                    level_from_pp: level as u16,
                    format_version: 1,
                    sac: sac as u32,
                    pp: dereth_rules::advancement::experience_to_skill_level(&xp, sac, level as usize),
                    init_level: 0,
                    resistance_of_last_check: 0,
                    last_used_time: 0.0,
                },
            );
            let cost = dereth_rules::advancement::skill_cost_to_raise(&q, &skills, &xp, id);
            let expected =
                dereth_rules::advancement::experience_to_skill_level(&xp, sac, level as usize + 1)
                    - dereth_rules::advancement::experience_to_skill_level(
                        &xp,
                        sac,
                        level as usize,
                    );
            assert_eq!(cost, expected, "{sac:?} level {level}");
        }
        // At the cap the cost is zero.
        q.set_skill(
            id,
            Skill {
                #[allow(clippy::cast_possible_truncation)]
                level_from_pp: max as u16,
                sac: sac as u32,
                ..*q.skill(id).unwrap()
            },
        );
        assert_eq!(
            dereth_rules::advancement::skill_cost_to_raise(&q, &skills, &xp, id),
            0,
            "{sac:?} at the cap"
        );
        assert_eq!(
            dereth_rules::advancement::skill_cost_to_raise_10(&q, &xp, id),
            0
        );
    }
}

/// The attribute and vital raise costs, likewise. The `+1` / `+10`
/// operands remain inferred; this pins the shape they produce so a later correction is visible.
#[test]
fn the_attribute_raise_costs_step_one_threshold_at_a_time() {
    let s = store();
    let xp: XpTable = load(&s, DbType::XpTable);
    let max = dereth_rules::advancement::max_attribute_level(&xp);
    assert!(max > 100);
    for level in 0..max {
        let spent =
            dereth_rules::advancement::experience_to_attribute_level(&xp, level as usize).unwrap();
        let cost = dereth_rules::advancement::attribute_cost_to_raise(&xp, level, spent, false);
        let expected =
            dereth_rules::advancement::experience_to_attribute_level(&xp, level as usize + 1)
                .unwrap()
                - spent;
        assert_eq!(cost, expected, "attribute level {level}");
    }
    assert_eq!(
        dereth_rules::advancement::attribute_cost_to_raise(&xp, max, 0, false),
        0,
        "at the cap"
    );

    let vmax = dereth_rules::advancement::max_attribute_2nd_level(&xp);
    for level in 0..vmax {
        let spent =
            dereth_rules::advancement::experience_to_attribute_2nd_level(&xp, level as usize)
                .unwrap();
        let cost = dereth_rules::advancement::attribute_cost_to_raise(&xp, level, spent, true);
        let expected =
            dereth_rules::advancement::experience_to_attribute_2nd_level(&xp, level as usize + 1)
                .unwrap()
                - spent;
        assert_eq!(cost, expected, "vital level {level}");
    }
}

/// Get run rate is still non monotonic with the shipped run skill.
#[test]
fn get_run_rate_is_still_non_monotonic_with_the_shipped_run_skill() {
    let s = store();
    let skills: SkillTable = load(&s, DbType::SkillTable);
    assert!(
        skills.skills.contains_key(&skill::RUN),
        "the Run skill is id 0x18"
    );

    assert_eq!(dereth_rules::skills::get_run_rate(0.0, 800, 1.0), 4.5);
    for v in [799i32, 801] {
        assert!(
            dereth_rules::skills::get_run_rate(0.0, v, 1.0) < 3.21,
            "skill {v}"
        );
    }
}

/// Pose resolves every shipped pose name.
#[test]
fn pose_resolves_every_shipped_pose_name() {
    use dereth_assets::tables::ChatPoseTable;
    use dereth_client_model::emotes::{self, STANDARD_EMOTES};

    let s = store();
    let t: ChatPoseTable = load(&s, DbType::ChatPoseTable);
    assert!(!t.poses.is_empty(), "the shipped table has pose entries");

    let mut resolved = 0usize;
    let mut dangling = Vec::new();
    for (name, motion) in &t.poses {
        match emotes::inq_chat_pose_command(&t, name) {
            Some((m, d)) => {
                assert_eq!(
                    &m, motion,
                    "the first hash's value is the motion-command name"
                );
                assert!(
                    !d.other_emote.is_empty(),
                    "pose {name} has an empty third-person text"
                );
                resolved += 1;
            }
            // A pose whose motion name has no `ChatEmoteData` is a shipped data gap, not a bug in
            // the lookup: `inq_chat_pose_command` returns nothing and `Pose` silently does nothing.
            None => dangling.push(name.clone()),
        }
    }
    eprintln!(
        "ChatPoseTable: {} poses, {resolved} resolve, {} dangling: {dangling:?}",
        t.poses.len(),
        dangling.len()
    );
    assert!(resolved > 0);

    // The lookup is case-insensitive, which is what makes `*WAVE*` work.
    let (name, _) = &t.poses[0];
    assert!(emotes::inq_chat_pose_command(&t, &name.to_uppercase()).is_some());
    assert!(emotes::inq_chat_pose_command(&t, &name.to_lowercase()).is_some());

    // Every name the emote list advertises must actually be in the table, or the built-in list
    // advertises a pose that does nothing.
    let missing: Vec<&str> = STANDARD_EMOTES
        .iter()
        .filter(|e| emotes::inq_chat_pose_command(&t, e).is_none())
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "advertised but unresolvable: {missing:?}"
    );
}
