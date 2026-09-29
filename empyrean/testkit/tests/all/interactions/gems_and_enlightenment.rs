//! ACE: Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs::SkillAlterationDevice
//! Gems and enlightenment through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod uses {
    //! ACE: Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs::SkillAlterationDevice
    use crate::support::creature_and_item_world::*;

    /// A skill alteration gem specializes a skill.
    #[test]
    fn a_skill_alteration_gem_specializes_a_skill() {
        let (mut ts, client) = server();
        let gem = in_pack(&mut ts, GEM_WCID);
        {
            let o = ts.world.objects.get_mut(PLAYER).unwrap();
            let s = o.get_creature_skill(Skill::MeleeDefense, true).unwrap();
            s.set_advancement_class(o, SkillAdvancementClass::Trained);
            s.set_init_level(o, 5);
            o.set_available_skill_credits(Some(30));
        }

        let from = ts.received_raw(client).len();
        dispatch::act_on_use::act_on_use(&mut ts.world, gem, PLAYER);
        ts.advance(0.1);
        let sent = got(&ts, client, from);
        let errors: Vec<u32> = sent
            .iter()
            .filter(|m| m.kind == WEENIE_ERROR)
            .map(|m| m.decode::<CommunicationWeenieError>().error_type)
            .collect();
        assert!(
            errors.is_empty(),
            "the confirmation is asked, not refused: {errors:?}"
        );
        // ConfirmationManager.EnqueueSend(Confirmation_AlterSkill): the yes answer runs
        // Confirmation_AlterSkill.ProcessConfirmation -> ActOnUse(player, confirmed: true)
        let manager = &ts
            .world
            .objects
            .get(PLAYER)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager;
        let context_id = manager
            .get(ConfirmationType::AlterSkill)
            .expect("Confirmation_AlterSkill is pending")
            .context_id;
        assert!(
            ts.world.objects.get(gem).is_some(),
            "nothing is spent before the answer"
        );

        // (ProcessConfirmation looks the player up in PlayerManager's online list)
        ts.world.player_manager.online_players.insert(
            PLAYER.full(),
            empyrean_world::managers::player_manager::OnlinePlayer {
                guid: PLAYER,
                account: None,
            },
        );
        let from = ts.received_raw(client).len();
        assert!(confirmation_manager::handle_response(
            &mut ts.world,
            PLAYER,
            ConfirmationType::AlterSkill,
            context_id,
            true,
            false
        ));
        ts.advance(0.1);
        let sent = got(&ts, client, from);
        let kinds: Vec<u32> = sent.iter().map(|m| m.kind).collect();
        let skill_at = kinds
            .iter()
            .position(|&k| k == PRIVATE_SKILL)
            .expect("the skill update");
        assert_eq!(
            &kinds[skill_at..skill_at + 3],
            &[PRIVATE_SKILL, PRIVATE_INT, WEENIE_ERROR_STR],
            "{kinds:X?}"
        );
        let skill = sent[skill_at].decode::<QualitiesPrivateUpdateSkill>().0;
        assert_eq!(
            (skill.property_id, skill.value.sac),
            (u32::try_from(Skill::MeleeDefense.0).unwrap(), 3),
            "specialized"
        );
        assert_eq!(skill.value.init_level, 10);
        let credits = sent[skill_at + 1].decode::<QualitiesPrivateUpdateInt>().0;
        // SpecializeSkill spends UpgradeCostFromTrainedToSpecialized (20 - 10); the heritage's 20 is
        // only the 70-credit cap's count
        assert_eq!(
            (credits.property_id, credits.value),
            (u32::from(PropertyInt::AvailableSkillCredits.0), 20),
            "30 - (20 - 10)"
        );
        let ok = sent[skill_at + 2].decode::<CommunicationWeenieErrorWithString>();
        assert_eq!(
            (ok.error_type, ok.text.as_str()),
            (
                u32::try_from(WeenieErrorWithString::YouHaveSucceededSpecializing_Skill.0).unwrap(),
                "Melee Defense"
            )
        );
        assert!(ts.world.objects.get(gem).is_none(), "the gem is consumed");
        let o = obj(&ts, PLAYER);
        assert_eq!(
            o.skills()
                .get(&Skill::MeleeDefense)
                .unwrap()
                .advancement_class(o),
            SkillAdvancementClass::Specialized
        );
    }

    /// `Enlightenment.HandleEnlightenment` on an eligible character (level 275, all 65 aura counts, a
    /// society master, room in the pack): the society, luminance, aetheria, attributes, skills and
    /// level are reset in ACE's order, the enlightenment rises to 1 with its title and certificate,
    /// and the world hears it.
    #[test]
    fn enlightenment_resets_an_eligible_character() {
        let (mut ts, client) = server();
        let npc = new_object(&mut ts.world, NPC_WCID);
        {
            let o = ts.world.objects.get_mut(PLAYER).unwrap();
            o.set_property(PropertyInt::Level, 275);
            o.set_property(PropertyInt64::TotalExperience, 191_226_310_247);
            o.set_property(PropertyInt64::AvailableExperience, 1_000);
            o.set_property(PropertyInt::SocietyRankCelhan, 1001);
            o.set_property(PropertyInt::Faction1Bits, 1);
            for (prop, v) in [
                (PropertyInt::LumAugAllSkills, 10),
                (PropertyInt::LumAugSurgeChanceRating, 5),
                (PropertyInt::LumAugCritDamageRating, 5),
                (PropertyInt::LumAugCritReductionRating, 5),
                (PropertyInt::LumAugDamageRating, 5),
                (PropertyInt::LumAugDamageReductionRating, 5),
                (PropertyInt::LumAugItemManaUsage, 5),
                (PropertyInt::LumAugItemManaGain, 5),
                (PropertyInt::LumAugHealingRating, 5),
                (PropertyInt::LumAugSkilledCraft, 5),
                (PropertyInt::LumAugSkilledSpec, 10),
            ] {
                o.set_property(prop, v);
            }
            o.set_property(PropertyInt64::AvailableLuminance, 500_000);
            o.set_property(PropertyInt64::MaximumLuminance, 1_000_000);
        }
        assert!(
            enlightenment::verify_lum_augs(obj(&ts, PLAYER))
                && enlightenment::verify_society_master(obj(&ts, PLAYER))
        );
        // online, as DoPlayerEnterWorld leaves it (PlayerManager.BroadcastToAll reaches online players)
        ts.world.player_manager.online_players.insert(
            PLAYER.full(),
            empyrean_world::managers::player_manager::OnlinePlayer {
                guid: PLAYER,
                account: None,
            },
        );

        let from = ts.received_raw(client).len();
        enlightenment::handle_enlightenment(&mut ts.world, npc, PLAYER);
        ts.advance(0.1);
        let sent = got(&ts, client, from);

        let o = obj(&ts, PLAYER);
        assert_eq!(
            (o.level(), o.total_experience(), o.available_experience()),
            (Some(1), Some(0), Some(0))
        );
        assert_eq!(o.enlightenment(), 1);
        assert_eq!((o.society_rank_celhan(), o.faction1_bits()), (None, None));
        assert_eq!(
            (
                o.lum_aug_all_skills(),
                o.lum_aug_skilled_spec(),
                o.available_luminance()
            ),
            (0, 0, None)
        );
        assert_eq!(
            o.available_skill_credits(),
            Some(50),
            "Aluvian's 50, no credit quests"
        );

        let lines = chats(&sent);
        for line in [
            "Your Luminance and Luminance Auras fade from your spirit.",
            "Your mastery of Aetheric magics fades.",
            "Your attribute training fades.",
            "You have become enlightened and view the world with new eyes.",
            "You have risen to a higher tier of enlightenment!",
            "Alpha has achieved the 1st level of Enlightenment!",
        ] {
            assert!(lines.contains(&line.to_owned()), "{line}: {lines:?}");
        }
        // RemoveLevel's two updates, in order: TotalExperience 0 then Level 1
        let total_at = sent
            .iter()
            .position(|m| {
                m.kind == PRIVATE_INT64
                    && m.decode::<QualitiesPrivateUpdateInt64>().0.property_id
                        == u32::from(PropertyInt64::TotalExperience.0)
            })
            .expect("TotalExperience update");
        let level = sent[total_at + 1].decode::<QualitiesPrivateUpdateInt>().0;
        assert_eq!(
            (level.property_id, level.value),
            (u32::from(PropertyInt::Level.0), 1)
        );
        // the certificate came from the NPC
        assert_eq!(
            container::get_num_inventory_items_of_wcid(&ts.world, PLAYER, CERTIFICATE_WCID),
            1
        );

        // a second enlightenment at level 1 is refused
        let from = ts.received_raw(client).len();
        enlightenment::handle_enlightenment(&mut ts.world, npc, PLAYER);
        ts.advance(0.1);
        assert_eq!(
            chats(&got(&ts, client, from)),
            ["You must be level 275 for enlightenment."]
        );
    }
}
