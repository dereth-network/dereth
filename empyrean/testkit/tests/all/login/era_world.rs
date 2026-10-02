//! Divergence: V386, V389
//! A headless client on the world `EMPYREAN_TEST_WORLD_PACK` names, under the rules of the era
//! the pack was built for: log in, read the character list's Throne of Destiny flag, create a
//! character, enter the world where the era starts it, and walk and run there.
//! Fixture: the retail dats and world.pack of the real-content tier, over the virtual-time server.

#[cfg(feature = "real-content")]
mod era_real {
    //! Divergence: V386, V389
    use empyrean_common::era::StartPositions;

    use crate::support::real_content_bot::real::*;

    /// Log in, create, enter and move, on whatever era the pack is for.
    #[test]
    fn a_new_character_enters_the_packs_era_world_and_moves() {
        let mut l = create_character(&[44, 6, 21, 22, 24]);
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
    }
}
