//! Divergence: V386, V389, V391, V392
//! A headless client on the world `EMPYREAN_TEST_WORLD_PACK` names, under the rules of the era
//! the pack was built for: log in, read the character list's Throne of Destiny flag, create a
//! character, enter the world where the era starts it, and walk and run there. Then the same on
//! the February 2005 dat set with an Infiltration pack.
//! Fixture: the retail dats and world.pack of the real-content tier, over the virtual-time server;
//! the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) and an Infiltration world.pack
//! (`EMPYREAN_TEST_INFILTRATION_PACK`).

#[cfg(feature = "real-content")]
mod era_real {
    //! Divergence: V386, V389, V391, V392
    use empyrean_common::era::{EraExt as _, StartPositions};
    use empyrean_content::WorldDatabase;

    use crate::support::real_content_bot::real::*;

    /// A melee skill, Melee Defense, Healing, Jump and Run: the melee skill is the era's (Heavy
    /// Weapons at the end of retail, Unarmed Combat before the 2012 consolidation).
    fn skills(content: &PackContent) -> [usize; 5] {
        let melee = if content.era().rules().creation_skills.is_some() {
            13
        } else {
            44
        };
        [melee, 6, 21, 22, 24]
    }

    /// Log in, create, enter and move, on whatever era the pack is for.
    #[test]
    fn a_new_character_enters_the_packs_era_world_and_moves() {
        let content = pack();
        let s = skills(&content);
        enter_and_move(create_character_with(dats(), content, &s));
    }

    /// The same on the February 2005 dat set: its character-generation table (three heritages,
    /// the six outdoor starter areas), its skill table (the old weapon skills), its cells and its
    /// landscape, with the Infiltration pack. The first starter area is Holtburg South.
    #[test]
    fn a_new_character_enters_holtburg_on_the_february_2005_dats_and_moves() {
        let content = infiltration_pack();
        assert!(
            content.era().rules().creation_skills.is_some(),
            "an Infiltration pack"
        );
        let s = skills(&content);
        let l = enter_and_move(create_character_with(pre_tod_dats(), content, &s));
        let p = l.ts.world.objects.get(l.g).expect("the character");
        // The weenie's motion and combat tables, which the older table does not name.
        for table in [
            empyrean_entity::enums::PropertyDataId::MotionTable,
            empyrean_entity::enums::PropertyDataId::CombatTable,
        ] {
            assert!(p.get_property(table).is_some_and(|id| id != 0), "{table:?}");
        }
        assert_eq!(
            p.get_property(empyrean_entity::enums::PropertyFloat::DefaultScale),
            None,
            "full size"
        );
        let unarmed = p
            .skills()
            .get(&empyrean_entity::enums::Skill::UnarmedCombat)
            .map(|s| s.advancement_class(p));
        assert_eq!(
            unarmed,
            Some(empyrean_entity::enums::SkillAdvancementClass::Trained)
        );

        // A drudge skulker of the era's world, fought unarmed on the older cells until a blow
        // lands: the attack resolves through the old Unarmed Combat skill.
        let mut l = l;
        let drudge = create_a_drudge(&mut l);
        let full = l.health(drudge).expect("the drudge");
        l.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).expect("a mode"),
        });
        l.advance(1.0);
        let mark = l.mark();
        let start = l.ts.seconds();
        while l.health(drudge).is_some_and(|h| h == full) {
            assert!(
                l.ts.seconds() - start < 60.0,
                "a blow lands within a minute"
            );
            let at = l.location_of(drudge);
            l.walk_toward(&at, 1.0);
            l.action(&CombatTargetedMeleeAttack {
                target: ObjectId(drudge.full()),
                attack_height: 2,
                power_level: 0.5,
            });
            l.advance(0.5);
        }
        assert!(
            !l.since::<AttackerNotification>(mark).is_empty(),
            "the character's hit lands (AttackerNotification)"
        );
    }

    fn enter_and_move(mut l: Loop) -> Loop {
        let rules = l.ts.world.era;
        assert_eq!(
            rules.id,
            l.ts.world.content.era(),
            "the world plays the pack's era"
        );

        // Login_LoginCharacterSet (0xF658): the era's Throne of Destiny flag.
        let list = l.ts.received::<LoginCharacterSet>(l.id);
        assert_eq!(
            list[0].has_throne_of_destiny,
            u32::from(rules.account_has_tod),
            "era {}",
            rules.id
        );

        let mark = l.mark();
        l.enter_world();
        assert_eq!(
            l.since::<LoginCreatePlayer>(mark).len(),
            1,
            "Login_CreatePlayer"
        );
        let at = l.location();
        let recalls_disabled =
            l.ts.world.objects.get(l.g).and_then(|o| {
                o.get_property(empyrean_entity::enums::PropertyBool::RecallsDisabled)
            });
        match rules.start_positions {
            StartPositions::FromCharGen => {
                assert_eq!(at.cell(), ACADEMY_CELL, "the Training Academy");
                assert_eq!(recalls_disabled, Some(true));
            }
            StartPositions::Towns(towns) => {
                let holtburg = StartPositions::town(towns, "Holtburg");
                assert!(
                    holtburg.areas.iter().any(|a| a.cell == at.cell()),
                    "a Holtburg starter area, not {:08X}",
                    at.cell()
                );
                assert_eq!(recalls_disabled, None, "recalls are enabled");
            }
        }

        // Walk one metre and run one and a half: the server moves the character and tells the
        // client.
        let mark = l.mark();
        l.action(&forward(&at, HOLD_NONE));
        l.advance(0.3);
        assert_eq!(
            l.last_forward(mark),
            Some(Some(WALK_FORWARD & 0xFFFF)),
            "walking"
        );
        let mut a = at;
        a.position_x -= 1.0;
        l.action(&autonomous(&a));
        l.advance(0.3);
        assert_eq!(l.location().position_x, a.position_x, "walked 1 m");

        let mark = l.mark();
        l.action(&forward(&a, HOLD_RUN));
        l.advance(0.3);
        assert_eq!(
            l.last_forward(mark),
            Some(Some(RUN_FORWARD & 0xFFFF)),
            "running"
        );
        let mut b = a;
        b.position_x -= 1.5;
        l.action(&autonomous(&b));
        l.advance(0.3);
        assert_eq!(l.location().position_x, b.position_x, "ran 1.5 m");
        assert!(
            !l.since::<MovementPositionEvent>(mark).is_empty(),
            "Movement_PositionEvent"
        );
        l.action(&stop(&b));
        l.advance(0.3);

        l.assert_all_decode(0, "the era world");
        l
    }
}
