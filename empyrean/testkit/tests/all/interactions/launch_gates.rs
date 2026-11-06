//! Divergence: V461
//! Launch action gates with valid injected items and virtual-time players.

mod skills_and_recalls {
    use crate::support::creature_and_item_world::*;
    use empyrean_common::era::{EraExt as _, EraId};
    use empyrean_world::world_objects::{
        player, player_location, player_magic, skill_alteration_device,
    };

    #[test]
    fn a_marketplace_recall_queued_before_launch_cannot_teleport_after_the_delay() {
        let (mut ts, _client) = server();
        let before = ts
            .world
            .objects
            .get(PLAYER)
            .unwrap()
            .get_position(empyrean_entity::enums::PositionType::Location)
            .unwrap();
        player_location::handle_action_tele_to_market_place(&mut ts.world, PLAYER);
        assert!(
            ts.world
                .objects
                .get(PLAYER)
                .unwrap()
                .wo
                .world_object
                .is_busy
        );
        ts.world.era = EraId::Launch.rules();
        ts.advance(15.0);
        let o = ts.world.objects.get(PLAYER).unwrap();
        let after = o
            .get_position(empyrean_entity::enums::PositionType::Location)
            .unwrap();
        assert_eq!(
            (before.landblock_id(), before.pos()),
            (after.landblock_id(), after.pos())
        );
        assert!(!o.wo.world_object.is_busy);
    }

    #[test]
    fn existing_eras_still_apply_a_confirmed_skill_gem() {
        for era in [EraId::Eor, EraId::Infiltration] {
            let (mut ts, _client) = server();
            ts.world.era = era.rules();
            let gem = in_pack(&mut ts, GEM_WCID);
            {
                let o = ts.world.objects.get_mut(PLAYER).unwrap();
                let s = o.get_creature_skill(Skill::MeleeDefense, true).unwrap();
                s.set_advancement_class(o, SkillAdvancementClass::Trained);
                o.set_available_skill_credits(Some(30));
            }
            skill_alteration_device::act_on_use(&mut ts.world, gem, PLAYER, true);
            let o = ts.world.objects.get(PLAYER).unwrap();
            assert_eq!(
                o.skills()
                    .get(&Skill::MeleeDefense)
                    .unwrap()
                    .advancement_class(o),
                SkillAdvancementClass::Specialized
            );
            assert!(o.available_skill_credits().unwrap() < 30);
        }
    }

    #[test]
    fn launch_keeps_ordinary_training_but_refuses_later_skills() {
        use empyrean_world::world_objects::player_skills;
        let (mut ts, _client) = server();
        ts.world.era = EraId::Launch.rules();
        {
            let o = ts.world.objects.get_mut(PLAYER).unwrap();
            let s = o.get_creature_skill(Skill::MeleeDefense, true).unwrap();
            s.set_advancement_class(o, SkillAdvancementClass::Untrained);
            o.set_available_skill_credits(Some(30));
        }
        assert!(!player_skills::handle_action_train_skill(
            &mut ts.world,
            PLAYER,
            Skill::DualWield,
            0
        ));
        assert_eq!(
            ts.world
                .objects
                .get(PLAYER)
                .unwrap()
                .available_skill_credits(),
            Some(30)
        );
        let cost = ts
            .world
            .dats
            .portal_dat()
            .skill_table()
            .skills
            .get(&6)
            .unwrap()
            .trained_cost;
        assert!(player_skills::handle_action_train_skill(
            &mut ts.world,
            PLAYER,
            Skill::MeleeDefense,
            cost
        ));
        let o = ts.world.objects.get(PLAYER).unwrap();
        assert_eq!(o.available_skill_credits(), Some(30 - cost));
        assert_eq!(
            o.skills()
                .get(&Skill::MeleeDefense)
                .unwrap()
                .advancement_class(o),
            SkillAdvancementClass::Trained
        );
    }

    #[test]
    fn launch_refuses_skill_gems_before_and_after_confirmation() {
        let (mut ts, client) = server();
        let gem = in_pack(&mut ts, GEM_WCID);
        {
            let o = ts.world.objects.get_mut(PLAYER).unwrap();
            let s = o.get_creature_skill(Skill::MeleeDefense, true).unwrap();
            s.set_advancement_class(o, SkillAdvancementClass::Trained);
            s.set_init_level(o, 5);
            o.set_available_skill_credits(Some(30));
        }
        ts.world.era = EraId::Launch.rules();
        let from = ts.received_raw(client).len();
        for confirmed in [false, true] {
            skill_alteration_device::act_on_use(&mut ts.world, gem, PLAYER, confirmed);
        }
        ts.advance(0.1);
        let o = ts.world.objects.get(PLAYER).unwrap();
        assert_eq!(
            o.skills()
                .get(&Skill::MeleeDefense)
                .unwrap()
                .advancement_class(o),
            SkillAdvancementClass::Trained
        );
        assert_eq!(o.available_skill_credits(), Some(30));
        assert!(ts.world.objects.get(gem).is_some());
        assert!(o
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager
            .get(ConfirmationType::AlterSkill)
            .is_none());
        assert_eq!(
            chats(&got(&ts, client, from)),
            ["This world has no skill alteration."; 2],
            "each use is told, and nothing else is sent"
        );
    }

    #[test]
    fn launch_pk_lite_and_marketplace_requests_schedule_no_effects() {
        let (mut ts, client) = server();
        ts.world.era = EraId::Launch.rules();
        let position = |o: &empyrean_world::world_objects::world_object::WorldObject| {
            o.get_position(empyrean_entity::enums::PositionType::Location)
                .map(|p| (p.landblock_id(), p.pos(), p.rotation()))
        };
        let before = position(ts.world.objects.get(PLAYER).unwrap());
        let status = ts.world.objects.get(PLAYER).unwrap().player_killer_status();
        let from = ts.received_raw(client).len();
        player::handle_action_enter_pk_lite(&mut ts.world, PLAYER);
        player_location::handle_action_tele_to_market_place(&mut ts.world, PLAYER);
        assert!(!player_magic::is_busy(&ts.world, PLAYER));
        ts.advance(30.0);
        let o = ts.world.objects.get(PLAYER).unwrap();
        assert_eq!(o.player_killer_status(), status);
        assert_eq!(position(o), before);
        let sent = got(&ts, client, from);
        assert!(!sent.iter().any(|m| m.kind == WEENIE_ERROR));
        assert_eq!(
            chats(&sent),
            [
                "This world has no PK Lite.",
                "This world has no Marketplace recall."
            ]
        );
        assert!(!player_magic::is_busy(&ts.world, PLAYER));
    }

    #[test]
    fn a_pending_skill_confirmation_cannot_cross_into_launch() {
        let (mut ts, _client) = server();
        let gem = in_pack(&mut ts, GEM_WCID);
        {
            let o = ts.world.objects.get_mut(PLAYER).unwrap();
            let s = o.get_creature_skill(Skill::MeleeDefense, true).unwrap();
            s.set_advancement_class(o, SkillAdvancementClass::Trained);
            o.set_available_skill_credits(Some(30));
        }
        skill_alteration_device::act_on_use(&mut ts.world, gem, PLAYER, false);
        assert!(ts
            .world
            .objects
            .get(PLAYER)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager
            .get(ConfirmationType::AlterSkill)
            .is_some());
        ts.world.era = EraId::Launch.rules();
        skill_alteration_device::act_on_use(&mut ts.world, gem, PLAYER, true);
        let o = ts.world.objects.get(PLAYER).unwrap();
        assert_eq!(
            o.skills()
                .get(&Skill::MeleeDefense)
                .unwrap()
                .advancement_class(o),
            SkillAdvancementClass::Trained
        );
        assert_eq!(o.available_skill_credits(), Some(30));
        assert!(ts.world.objects.get(gem).is_some());
    }
}

mod augmentations {
    use crate::support::tracking_world::*;
    use empyrean_common::era::{EraExt as _, EraId};
    use empyrean_world::world_objects::augmentation_device;

    #[test]
    fn launch_augmentation_use_confirmation_and_direct_completion_preserve_the_gem_and_xp() {
        let mut ts = server();
        join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let gem = in_pack(&mut ts.world, ALPHA, STRENGTH_GEM);
        let player = ObjectGuid::new(ALPHA);
        ts.world.era = EraId::Launch.rules();
        for confirmed in [false, true] {
            augmentation_device::act_on_use(&mut ts.world, gem, player, confirmed);
        }
        augmentation_device::do_augmentation(&mut ts.world, gem, player);
        let o = ts.world.objects.get(player).unwrap();
        assert_eq!(innate(&ts.world, ALPHA, PropertyAttribute::Strength), 50);
        assert_eq!(o.augmentation_innate_family(), 0);
        assert_eq!(o.available_experience(), Some(5000));
        assert!(container::inventory_values(&ts.world, player).contains(&gem));
        assert!(o
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager
            .get(ConfirmationType::Augmentation)
            .is_none());
    }
}

mod deaths_and_arenas {
    //! Divergence: V472
    //! `Character_TeleToLifestone` (`0x0063`), `Character_Suicide` (`0x0279`) and the PK arena
    //! recalls (`0x0027`, `0x0026`), each sent as a client sends it.
    use crate::support::event_world::*;
    use dereth_protocol::combat::{
        CharacterSuicide, CharacterTeleToLifestone, CharacterTeleToPkArena, CharacterTeleToPklArena,
    };
    use dereth_protocol::comms::CommunicationTextboxString;
    use empyrean_common::era::{EraExt as _, EraId};

    fn joined(era: EraId) -> (TestServer, ClientId) {
        let mut ts = server();
        ts.world.era = era.rules();
        let alpha = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), false);
        (ts, alpha)
    }

    fn deaths(ts: &TestServer) -> i32 {
        ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .expect("the player")
            .num_deaths()
    }

    fn texts(ts: &TestServer, id: ClientId) -> Vec<String> {
        ts.received::<CommunicationTextboxString>(id)
            .into_iter()
            .map(|m| m.text)
            .collect()
    }

    /// At launch the lifestone request is a death, at once; with the recall it is the recall,
    /// which a character never attuned to a lifestone is refused.
    #[test]
    fn a_launch_lifestone_request_kills_the_character_at_once() {
        for era in [EraId::Launch, EraId::Eor] {
            let (mut ts, alpha) = joined(era);
            let before = deaths(&ts);
            // Twice at once: the second finds the character dead, and kills nobody.
            ts.send_game_action(alpha, &CharacterTeleToLifestone);
            ts.send_game_action(alpha, &CharacterTeleToLifestone);
            ts.advance(1.0);
            let said = texts(&ts, alpha);
            let unattuned = "Your spirit has not been attuned to a sanctuary location.";
            if era == EraId::Launch {
                assert_eq!(deaths(&ts), before + 1, "one death");
                assert!(!said.iter().any(|t| t == unattuned), "{said:?}");
            } else {
                assert_eq!(deaths(&ts), before, "a recall kills nobody");
                assert!(said.iter().any(|t| t == unattuned), "{said:?}");
            }
        }
    }

    /// `@die` came with the recall: a Launch world refuses the suicide request and says so, and
    /// the character lives; a later world kills it after its last words.
    #[test]
    fn a_launch_world_refuses_the_suicide_request() {
        for era in [EraId::Launch, EraId::Eor] {
            let (mut ts, alpha) = joined(era);
            let before = deaths(&ts);
            ts.send_game_action(alpha, &CharacterSuicide);
            ts.advance(20.0);
            let said = texts(&ts, alpha);
            let refusal = "This world has no @die command.";
            if era == EraId::Launch {
                assert_eq!(deaths(&ts), before);
                assert!(said.iter().any(|t| t == refusal), "{said:?}");
            } else {
                assert_eq!(deaths(&ts), before + 1);
                assert!(!said.iter().any(|t| t == refusal), "{said:?}");
            }
        }
    }

    /// The PK arena recalls came after launch: both requests are refused with a line, before the
    /// player-killer checks a later world makes.
    #[test]
    fn a_launch_world_refuses_the_pk_arena_recalls() {
        let (mut ts, alpha) = joined(EraId::Launch);
        ts.send_game_action(alpha, &CharacterTeleToPkArena);
        ts.send_game_action(alpha, &CharacterTeleToPklArena);
        ts.advance(1.0);
        let said = texts(&ts, alpha);
        for refusal in [
            "This world has no PK arena recalls.",
            "This world has no PK Lite arena recall.",
        ] {
            assert!(said.iter().any(|t| t == refusal), "{refusal}: {said:?}");
        }
        let (mut ts, alpha) = joined(EraId::Eor);
        ts.send_game_action(alpha, &CharacterTeleToPkArena);
        ts.advance(1.0);
        assert!(
            !texts(&ts, alpha)
                .iter()
                .any(|t| t.starts_with("This world has no")),
            "the end of retail checks the player instead"
        );
    }
}

mod allegiance_administration {
    //! Divergence: V480, V482, V483, V484
    //! The allegiance's administration requests, each sent as a client sends it: the message of
    //! the day (June 2001), booting (April 2002), officers, locks and bans (May 2004) and the
    //! allegiance's name (May 2006). A Launch world refuses each with a line; a later world
    //! answers it as ACE does.
    use crate::support::event_world::*;
    use dereth_protocol::comms::CommunicationTextboxString;
    use dereth_protocol::social::{
        AllegianceAddAllegianceBan, AllegianceBreakAllegianceBoot, AllegianceChatGag,
        AllegianceClearMotd, AllegianceDoAllegianceHouseAction, AllegianceDoAllegianceLockAction,
        AllegianceListAllegianceOfficerTitles, AllegianceListAllegianceOfficers,
        AllegianceQueryAllegianceName, AllegianceQueryMotd, AllegianceSetAllegianceName,
        AllegianceSetAllegianceOfficer, AllegianceSetMotd,
    };
    use empyrean_common::era::{EraExt as _, EraId};

    fn texts(ts: &TestServer, id: ClientId) -> Vec<String> {
        ts.received::<CommunicationTextboxString>(id)
            .into_iter()
            .map(|m| m.text)
            .collect()
    }

    /// Sends every request; the refusals a Launch world answers them with, in order.
    fn send_each(ts: &mut TestServer, alpha: ClientId) -> Vec<&'static str> {
        ts.send_game_action(
            alpha,
            &AllegianceSetMotd {
                motd: "Welcome".into(),
            },
        );
        ts.send_game_action(alpha, &AllegianceQueryMotd);
        ts.send_game_action(alpha, &AllegianceClearMotd);
        ts.send_game_action(
            alpha,
            &AllegianceBreakAllegianceBoot {
                name: "Bravo".into(),
                account_boot: 0,
            },
        );
        ts.send_game_action(
            alpha,
            &AllegianceSetAllegianceOfficer {
                name: "Bravo".into(),
                level: 1,
            },
        );
        ts.send_game_action(alpha, &AllegianceDoAllegianceLockAction { action: 1 });
        ts.send_game_action(
            alpha,
            &AllegianceAddAllegianceBan {
                name: "Bravo".into(),
            },
        );
        ts.send_game_action(
            alpha,
            &AllegianceSetAllegianceName {
                name: "The Order".into(),
            },
        );
        ts.send_game_action(alpha, &AllegianceQueryAllegianceName);
        ts.advance(1.0);
        vec![
            "This world has no allegiance message of the day.",
            "This world has no allegiance message of the day.",
            "This world has no allegiance message of the day.",
            "This world has no allegiance booting.",
            "This world has no allegiance officers.",
            "This world has no allegiance lock.",
            "This world has no allegiance bans.",
            "This world has no allegiance names.",
            "This world has no allegiance names.",
        ]
    }

    #[test]
    fn a_launch_world_refuses_every_allegiance_administration_request() {
        let mut ts = server();
        ts.world.era = EraId::Launch.rules();
        let alpha = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), false);
        let _bravo = join(&mut ts, "bravo", BRAVO, "Bravo", at(21.0, 20.0), false);
        let refusals = send_each(&mut ts, alpha);
        let said: Vec<String> = texts(&ts, alpha)
            .into_iter()
            .filter(|t| t.starts_with("This world has no"))
            .collect();
        assert_eq!(said, refusals);
    }

    /// February 2005 has officers and bans (May 2004), but not titles, the lock or the gag (May
    /// 2006), the allegiance's name, or `@allegiance house` (after June 2005).
    #[test]
    fn a_february_2005_world_has_the_officers_and_bans_but_nothing_later() {
        let mut ts = server();
        ts.world.era = EraId::Infiltration.rules();
        let alpha = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), false);
        ts.send_game_action(alpha, &AllegianceListAllegianceOfficers);
        ts.send_game_action(
            alpha,
            &AllegianceAddAllegianceBan {
                name: "Bravo".into(),
            },
        );
        ts.send_game_action(alpha, &AllegianceListAllegianceOfficerTitles);
        ts.send_game_action(alpha, &AllegianceDoAllegianceLockAction { action: 1 });
        ts.send_game_action(
            alpha,
            &AllegianceChatGag {
                name: "Bravo".into(),
                gagged: 1,
            },
        );
        ts.send_game_action(alpha, &AllegianceQueryAllegianceName);
        ts.send_game_action(alpha, &AllegianceDoAllegianceHouseAction { action: 1 });
        ts.advance(1.0);
        let said: Vec<String> = texts(&ts, alpha)
            .into_iter()
            .filter(|t| t.starts_with("This world has no"))
            .collect();
        assert_eq!(
            said,
            [
                "This world has no allegiance officer titles.",
                "This world has no allegiance lock.",
                "This world has no allegiance chat gag.",
                "This world has no allegiance names.",
                "This world has no allegiance house commands.",
            ]
        );
    }

    #[test]
    fn a_later_world_answers_the_same_requests_as_ace_does() {
        let mut ts = server();
        ts.world.era = EraId::Eor.rules();
        let alpha = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), false);
        let _bravo = join(&mut ts, "bravo", BRAVO, "Bravo", at(21.0, 20.0), false);
        let _ = send_each(&mut ts, alpha);
        let said = texts(&ts, alpha);
        assert!(
            !said.iter().any(|t| t.starts_with("This world has no")),
            "{said:?}"
        );
    }
}

mod fellowships_appraisal_and_notes {
    //! Divergence: V485, V486, V487, V489, V490, V491
    //! A fellowship that asks not to share experience (`Fellowship_Create`, `0x00A2`), an item
    //! appraised with `Item_Appraise` (`0x00C8`), and a trade note's prices and issuer, on a
    //! Launch world and the end of retail.
    use crate::support::event_world::*;
    use dereth_primitives::ObjectId;
    use dereth_protocol::objects::ItemAppraise;
    use dereth_protocol::social::{
        FellowshipChangeFellowOpenness, FellowshipCreate, FellowshipRecruit,
    };
    use empyrean_common::era::{EraExt as _, EraId};
    use empyrean_entity::enums::CharacterOption;
    use empyrean_entity::enums::ItemType;
    use empyrean_world::world_objects::player_character;
    use empyrean_world::world_objects::{player_fellowship, vendor};

    const VENDOR: u32 = 3000;
    const OTHER_VENDOR: u32 = 3001;
    const NOTE: u32 = 3002;
    const GUARDED: u32 = 3003;

    fn joined(era: EraId) -> (TestServer, ClientId) {
        let mut ts = server();
        ts.world.era = era.rules();
        let content = crate::support::event_world::content()
            .weenie(weenie(VENDOR, "vendor", "Vendor", WeenieType::Vendor))
            .weenie(weenie(
                OTHER_VENDOR,
                "vendor2",
                "Other Vendor",
                WeenieType::Vendor,
            ))
            .weenie(
                weenie(NOTE, "note", "Trade Note", WeenieType::Generic)
                    .with_int(
                        PropertyInt::ItemType,
                        ItemType::PromissoryNote.0.cast_signed(),
                    )
                    .with_int(PropertyInt::Value, 1000),
            )
            .weenie(
                weenie(GUARDED, "gem", "Cut Gem", WeenieType::Generic)
                    .with_int(PropertyInt::ItemType, ItemType::Gem.0.cast_signed())
                    .with_int(PropertyInt::ResistItemAppraisal, 900),
            );
        ts.world.content = Arc::new(content);
        let alpha = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), false);
        (ts, alpha)
    }

    #[test]
    fn a_launch_fellowship_shares_experience_whatever_its_founder_asks() {
        for (era, shares) in [(EraId::Launch, true), (EraId::Eor, false)] {
            let (mut ts, alpha) = joined(era);
            ts.send_game_action(
                alpha,
                &FellowshipCreate {
                    name: "Wanderers".into(),
                    share_xp: 0,
                },
            );
            ts.advance(0.5);
            let fellowship = player_fellowship::fellowship(&ts.world, ObjectGuid::new(ALPHA))
                .expect("a fellowship");
            assert_eq!(fellowship.get(&ts.world).desired_share_xp, shares, "{era}");
        }
    }

    #[test]
    fn a_launch_fellowship_shares_no_loot_whatever_its_founder_saved() {
        for (era, shares) in [(EraId::Launch, false), (EraId::Eor, true)] {
            let (mut ts, alpha) = joined(era);
            let me = ObjectGuid::new(ALPHA);
            player_character::set_character_option(
                &mut ts.world,
                me,
                CharacterOption::ShareFellowshipLoot,
                true,
            );
            ts.send_game_action(
                alpha,
                &FellowshipCreate {
                    name: "Wanderers".into(),
                    share_xp: 1,
                },
            );
            ts.advance(0.5);
            let fellowship = player_fellowship::fellowship(&ts.world, me).expect("a fellowship");
            assert_eq!(fellowship.get(&ts.world).share_loot, shares, "{era}");
            assert!(
                player_character::get_character_option(
                    &ts.world,
                    me,
                    CharacterOption::ShareFellowshipLoot
                ),
                "{era}: the saved value stands"
            );
        }
    }

    #[test]
    fn a_launch_invitation_asks_whatever_the_invited_player_saved() {
        const BRAVO: u32 = 0x5000_0002;
        for (era, joins_at_once) in [(EraId::Launch, false), (EraId::Eor, true)] {
            let (mut ts, alpha) = joined(era);
            let _bravo = join(&mut ts, "bravo", BRAVO, "Bravo", at(21.0, 20.0), false);
            let them = ObjectGuid::new(BRAVO);
            player_character::set_character_option(
                &mut ts.world,
                them,
                CharacterOption::AutomaticallyAcceptFellowshipRequests,
                true,
            );
            ts.send_game_action(
                alpha,
                &FellowshipCreate {
                    name: "Wanderers".into(),
                    share_xp: 1,
                },
            );
            ts.advance(0.5);
            ts.send_game_action(
                alpha,
                &FellowshipRecruit {
                    target: ObjectId(BRAVO),
                },
            );
            ts.advance(0.5);
            assert_eq!(
                player_fellowship::fellowship(&ts.world, them).is_some(),
                joins_at_once,
                "{era}"
            );
        }
    }

    #[test]
    fn a_launch_fellowship_cannot_be_opened() {
        for (era, opens) in [(EraId::Launch, false), (EraId::Eor, true)] {
            let (mut ts, alpha) = joined(era);
            ts.send_game_action(
                alpha,
                &FellowshipCreate {
                    name: "Wanderers".into(),
                    share_xp: 1,
                },
            );
            ts.advance(0.5);
            ts.send_game_action(alpha, &FellowshipChangeFellowOpenness { open: 1 });
            ts.advance(0.5);
            let fellowship = player_fellowship::fellowship(&ts.world, ObjectGuid::new(ALPHA))
                .expect("a fellowship");
            assert_eq!(fellowship.get(&ts.world).open, opens, "{era}");
        }
    }

    #[test]
    fn a_launch_appraisal_of_a_guarded_item_needs_the_skill() {
        for (era, known) in [(EraId::Launch, false), (EraId::Eor, true)] {
            let (mut ts, alpha) = joined(era);
            let gem = on_ground(&mut ts.world, GUARDED, at(20.5, 20.0));
            ts.send_game_action(
                alpha,
                &ItemAppraise {
                    target: ObjectId(gem.full()),
                },
            );
            ts.advance(0.5);
            let target = ts
                .world
                .objects
                .get(ObjectGuid::new(ALPHA))
                .and_then(|o| o.current_appraisal_target());
            assert_eq!(target.is_some(), known, "{era}");
        }
    }

    #[test]
    fn a_launch_trade_note_cashes_in_full_only_at_its_issuer() {
        let (mut ts, _alpha) = joined(EraId::Launch);
        let w = &mut ts.world;
        let issuer = new_object(w, VENDOR);
        let other = new_object(w, OTHER_VENDOR);
        let note = new_object(w, NOTE);
        assert_eq!(vendor::get_sell_cost(w, issuer, note), 1100, "sold at 1.1");
        assert_eq!(
            vendor::get_buy_cost(w, issuer, note),
            800,
            "issued by nobody yet"
        );
        vendor::stamp_note_issuer(w, issuer, &[note]);
        assert_eq!(vendor::get_buy_cost(w, issuer, note), 1000, "its issuer");
        assert_eq!(
            vendor::get_buy_cost(w, other, note),
            800,
            "any other vendor"
        );

        w.era = EraId::Eor.rules();
        assert_eq!(vendor::get_sell_cost(w, issuer, note), 1150);
        assert_eq!(vendor::get_buy_cost(w, other, note), 1000);
        let fresh = new_object(w, NOTE);
        vendor::stamp_note_issuer(w, issuer, &[fresh]);
        let stamped: Option<u32> = w
            .objects
            .get(fresh)
            .unwrap()
            .get_property(PropertyDataId::VendorsClassId);
        assert_eq!(stamped, None, "a later world stamps nothing");
    }
}
