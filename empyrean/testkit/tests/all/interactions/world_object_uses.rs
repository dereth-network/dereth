//! ACE: Source/ACE.Server/WorldObjects/Player_Use.cs::HandleActionUseItem
//! World object uses through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod uses {
    //! ACE: Source/ACE.Server/WorldObjects/Player_Use.cs::HandleActionUseItem
    use crate::support::object_use_world::*;

    /// `Player_Book.cs` over the `Writing_*` actions: AddPage answers 0xB6 with the new index and the
    /// player as author; ModifyPage writes the text and answers 0xB5; BookData (0xAA) answers the
    /// whole book (0xB4); BookPageData (0xAE) one page (0xB8); a full book adds nothing and sends
    /// nothing (`Book.AddPage` returns null); DeletePage answers 0xB7.
    #[test]
    fn a_player_writes_reads_and_deletes_book_pages() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let book = in_pack(&mut ts, BOOK);
        ts.advance(0.5);
        let book_id = ObjectId(book.full());

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookAddPage { book_id })
        });
        let r = first(&a, BOOK_ADD_PAGE_RESPONSE).decode::<WritingBookAddPageResponse>();
        assert_eq!((r.book_id, r.page_number, r.success), (book_id, 0, 1));
        assert_eq!(
            pages(&ts, book),
            [(ALPHA, Some("Alpha".to_owned()), Some(String::new()))]
        );
        assert_eq!(
            obj(&ts, book).get_property(PropertyInt::AppraisalPages),
            Some(1)
        );

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &WritingBookModifyPage {
                    book_id,
                    page: 0,
                    text: "Dear diary".to_owned(),
                },
            );
        });
        assert_eq!(kinds(&a), [BOOK_MODIFY_PAGE_RESPONSE]);
        let r = first(&a, BOOK_MODIFY_PAGE_RESPONSE).decode::<WritingBookModifyPageResponse>();
        assert_eq!((r.book_id, r.page_number, r.success), (book_id, 0, 1));
        assert_eq!(pages(&ts, book)[0].2.as_deref(), Some("Dear diary"));

        // V342 (a fix): an edit the book refuses (here the same text again) is answered as refused;
        // ACE reported success for every edit.
        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &WritingBookModifyPage {
                    book_id,
                    page: 0,
                    text: "Dear diary".to_owned(),
                },
            );
        });
        let r = first(&a, BOOK_MODIFY_PAGE_RESPONSE).decode::<WritingBookModifyPageResponse>();
        assert_eq!((r.book_id, r.page_number, r.success), (book_id, 0, 0));

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookData { book_id })
        });
        let open = first(&a, BOOK_OPEN).decode::<WritingBookOpen>();
        assert_eq!(open.book_id, book_id);

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookPageData { book_id, page: 0 })
        });
        let page = first(&a, BOOK_PAGE_DATA_RESPONSE).decode::<BookPageDataResponse>();
        assert_eq!((page.object_id, page.page), (book_id, 0));

        // two pages fill it; the third AddPage is refused with no answer
        let _ = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookAddPage { book_id })
        });
        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookAddPage { book_id })
        });
        assert!(
            all(&a, BOOK_ADD_PAGE_RESPONSE).is_empty(),
            "a full book: {:04X?}",
            kinds(&a)
        );
        assert_eq!(pages(&ts, book).len(), 2);

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &WritingBookDeletePage { book_id, page: 0 })
        });
        let r = first(&a, BOOK_DELETE_PAGE_RESPONSE).decode::<WritingBookDeletePageResponse>();
        assert_eq!((r.page_number, r.success), (0, 1));
        assert_eq!(pages(&ts, book).len(), 1);
        assert_eq!(
            obj(&ts, book).get_property(PropertyInt::AppraisalPages),
            Some(1)
        );
    }

    /// `PKModifier.ActOnUse`: an NPK player uses a PK altar (PkLevelModifier 1): the use message, the
    /// player's PkLevelModifier and PlayerKillerStatus become PK (broadcast as a public int), and
    /// UseDone; a second use by the now-PK player gets the altar's ActivationFailure text (none here:
    /// an empty line).
    #[test]
    fn an_npk_player_becomes_pk_at_the_altar() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let altar = on_ground(&mut ts, PK_ALTAR, at(21.5, 20.0));
        ts.advance(0.5);
        assert_eq!(obj(&ts, ObjectGuid::new(ALPHA)).pk_level(), PKLevel::NPK);

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(altar.full()),
                },
            )
        });
        assert_eq!(
            chats(&a),
            ["You feel a harsh dissonance, and you sense that an act of killing is now possible."]
        );
        let p = obj(&ts, ObjectGuid::new(ALPHA));
        assert_eq!(
            (p.pk_level(), p.player_killer_status()),
            (PKLevel::PK, PlayerKillerStatus::PK)
        );
        assert!(
            !all(&a, PUBLIC_INT).is_empty(),
            "the PlayerKillerStatus broadcast: {:04X?}",
            kinds(&a)
        );
        assert_eq!(use_dones(&a), [0]);
        assert!(
            !obj(&ts, altar).wo.world_object.is_busy && !p.wo.world_object.is_busy,
            "Reset"
        );

        let a = during(&mut ts, alpha, 1.0, |ts| {
            empyrean_world::world_objects::player_use::try_use_item(
                &mut ts.world,
                ObjectGuid::new(ALPHA),
                altar,
                true,
            )
        });
        assert_eq!(
            chats(&a),
            [""],
            "a PK player at a PK altar: ActivationFailure"
        );
    }

    /// `AdvocateFane.ActOnUse`: an NPK player who is not yet an advocate bows, the fane's use message
    /// arrives and AdvocateQuest is set; a second use is refused by the (failure) branch with no text.
    #[test]
    fn the_advocate_fane_sets_the_advocate_quest() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let fane = on_ground(&mut ts, FANE, at(21.5, 20.0));
        ts.advance(0.5);

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(fane.full()),
                },
            )
        });
        assert_eq!(chats(&a), ["You are now an Advocate."]);
        assert!(obj(&ts, ObjectGuid::new(ALPHA)).advocate_quest_player());
        assert!(!obj(&ts, fane).wo.world_object.is_busy);

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(fane.full()),
                },
            )
        });
        assert!(
            chats(&a).is_empty(),
            "already an advocate: the failure motion only"
        );
        assert_eq!(use_dones(&a), [0]);
        assert!(
            !obj(&ts, fane).wo.world_object.is_busy,
            "Reset after the failure"
        );
    }

    /// `Lifestone.ActOnUse`: the player's Sanctuary becomes where they stand, the use message arrives
    /// (Magic), and Stamina is halved (`Math.Round(Current / 2f)`).
    #[test]
    fn a_lifestone_binds_the_player_and_halves_stamina() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = on_ground(&mut ts, LIFESTONE, at(21.5, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);
        assert!(obj(&ts, p).sanctuary().is_none());

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(stone.full()),
                },
            )
        });
        assert_eq!(
            chats(&a),
            ["You have attuned your spirit to this Lifestone."]
        );
        let o = obj(&ts, p);
        assert_eq!(
            o.sanctuary().map(|s| s.cell()),
            o.location().map(|l| l.cell())
        );
        assert_eq!(o.stamina().current(o), 50);
    }

    /// `ManaStone.HandleActionUseOnTarget`: an empty stone drains a wand in the pack (the wand is
    /// consumed, the stone holds `Math.Round(Efficiency * mana)`, the Magical UI effect); the charged
    /// stone then tops up another wand (the rest of the wand's need), and with a 0 destroy chance it
    /// is emptied (ItemCurMana null, UiEffects Undef).
    #[test]
    fn a_mana_stone_drains_one_item_and_charges_another() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = in_pack(&mut ts, STONE);
        let wand = in_pack(&mut ts, WAND);
        obj_mut(&mut ts, wand).set_property(PropertyInt::ItemCurMana, 1001);
        obj_mut(&mut ts, wand).set_property(PropertyInt::ItemMaxMana, 2000);
        ts.advance(0.5);

        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseWithTargetEvent {
                    object: ObjectId(stone.full()),
                    target: ObjectId(wand.full()),
                },
            );
        });
        assert_eq!(
            chats(&a),
            ["The Mana Stone drains 500 points of mana from the Wand.\nThe Wand is destroyed."],
            "Math.Round(500.5) is 500"
        );
        assert!(ts.world.objects.get(wand).is_none(), "the wand is consumed");
        assert_eq!(obj(&ts, stone).item_cur_mana(), Some(500));
        assert_eq!(obj(&ts, stone).ui_effects(), Some(UiEffects::Magical));
        assert_eq!(use_dones(&a), [0]);

        let other = in_pack(&mut ts, WAND);
        obj_mut(&mut ts, other).set_property(PropertyInt::ItemCurMana, 1800);
        obj_mut(&mut ts, other).set_property(PropertyInt::ItemMaxMana, 2000);
        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseWithTargetEvent {
                    object: ObjectId(stone.full()),
                    target: ObjectId(other.full()),
                },
            );
        });
        assert_eq!(
            chats(&a),
            ["The Mana Stone gives 200 points of mana to the Wand."]
        );
        assert_eq!(obj(&ts, other).item_cur_mana(), Some(2000));
        assert_eq!(
            (
                obj(&ts, stone).item_cur_mana(),
                obj(&ts, stone).ui_effects()
            ),
            (None, Some(UiEffects::Undef))
        );
        assert_eq!(use_dones(&a), [0]);
    }

    /// `Aetheria.UseObjectOnTarget` / `ActivateSigil` (Aetheria.cs): the mana stone on a blue
    /// coalesced aetheria, after the clap, reveals a sigil: one of the five aetheria sets with the
    /// blue icon for it, a surge spell (5204-5208), the SigilOne slot, the new name, the chat line and
    /// a successful UseDone.
    #[test]
    fn a_mana_stone_reveals_an_aetheria_sigil() {
        use empyrean_world::entity::aetheria::{icons, AetheriaColor, Sigil};

        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = in_pack(&mut ts, 42645);
        let aetheria = in_pack(&mut ts, 42635);
        ts.advance(0.5);

        let a = during(&mut ts, alpha, 3.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseWithTargetEvent {
                    object: ObjectId(stone.full()),
                    target: ObjectId(aetheria.full()),
                },
            );
        });
        assert_eq!(
            chats(&a),
            ["A sigil rises to the surface as you bathe the aetheria in mana."]
        );
        assert_eq!(use_dones(&a), [0]);
        let o = obj(&ts, aetheria);
        assert_eq!(
            o.get_property(PropertyString::Name).as_deref(),
            Some("Aetheria")
        );
        assert_eq!(
            o.get_property(PropertyInt::ValidLocations),
            Some(0x1000_0000)
        );
        let set = o.get_property(PropertyInt::EquipmentSetId).expect("a set");
        assert!((35..=39).contains(&set), "{set}");
        let sigil = Sigil::from_i32(set - 35).unwrap();
        assert_eq!(
            o.get_property(PropertyDataId::Icon),
            Some(icons(AetheriaColor::Blue, sigil))
        );
        let surge = o.get_property(PropertyDataId::ProcSpell).expect("a surge");
        assert!((5204..=5208).contains(&surge), "{surge}");
        assert_eq!(
            o.get_property(empyrean_entity::enums::PropertyBool::ProcSpellSelfTargeted)
                .is_some(),
            [5204, 5206, 5208].contains(&surge)
        );
    }

    /// Divergence: V424
    /// A world without aetheria refuses the mana stone on coalesced aetheria: no sigil rises, the
    /// aetheria keeps its name and no slot, the player is told why and the use ends.
    #[test]
    fn a_world_without_aetheria_reveals_no_sigil() {
        use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
        let mut ts = server();
        let eor = EraId::Eor.rules();
        ts.world.era = with_features(
            eor,
            EraFeatures {
                aetheria: false,
                ..eor.features
            },
        );
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = in_pack(&mut ts, 42645);
        let aetheria = in_pack(&mut ts, 42635);
        ts.advance(0.5);
        let name = obj(&ts, aetheria).get_property(PropertyString::Name);

        let a = during(&mut ts, alpha, 3.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseWithTargetEvent {
                    object: ObjectId(stone.full()),
                    target: ObjectId(aetheria.full()),
                },
            );
        });
        assert_eq!(chats(&a), ["This world has no aetheria."]);
        assert_eq!(use_dones(&a), [0]);
        let o = obj(&ts, aetheria);
        assert_eq!(o.get_property(PropertyString::Name), name);
        assert_eq!(o.get_property(PropertyInt::EquipmentSetId), None);
        assert!(ts.world.objects.get(stone).is_some(), "the stone is kept");
    }

    /// A tableless pk players use waits as aces does until the next use.
    #[test]
    fn a_tableless_pk_players_use_waits_as_aces_does_until_the_next_use() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let altar = on_ground(&mut ts, PK_ALTAR, at(21.5, 20.0));
        ts.advance(0.5);
        let _ = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(altar.full()),
                },
            )
        });
        let g = ObjectGuid::new(ALPHA);
        assert!(
            empyrean_world::world_objects::player_tick::fast_tick(&ts.world, g),
            "PK now"
        );
        assert_eq!(
            obj(&ts, g).get_property(PropertyDataId::MotionTable),
            None,
            "no motion table"
        );
        let h = empyrean_world::physics::phys_ext::physics_obj(&ts.world, g).expect("a body");

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(altar.full()),
                },
            )
        });
        assert!(
            use_dones(&a).is_empty() && chats(&a).is_empty(),
            "the Use waits"
        );
        assert!(
            empyrean_world::world_objects::player_move2::move_to_params_has_callback(&ts.world, g),
            "its callback is pending"
        );
        assert!(
            empyrean_world::physics::motion::motions_pending(&ts.world, h),
            "the queued Ready never completes"
        );

        let a = during(&mut ts, alpha, 1.0, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryUseEvent {
                    object: ObjectId(altar.full()),
                },
            )
        });
        assert_eq!(
            use_dones(&a),
            [0],
            "the next Use fails the pending one (callback(false))"
        );
    }
}
