//! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
//! Item actions through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod requests {
    //! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
    use crate::support::inventory_action_world::*;

    /// `Player.HandleActionQueryItemMana` -> `WorldObject.QueryItemMana` (WorldObject.cs): the
    /// examiner gets `GameEventQueryItemManaResponse(target, ItemCurMana / ItemMaxMana, 1)`; an item
    /// without mana answers mana 0 and success 0 ("according to retail PCAPs"); an object the player
    /// neither carries nor wields gets no answer.
    #[test]
    fn query_item_mana_answers_over_the_wire() {
        let mut ts = server();
        let (alpha, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let wand = in_pack(&mut ts, WAND);
        obj_mut(&mut ts, wand).set_property(PropertyInt::ItemCurMana, 500);
        obj_mut(&mut ts, wand).set_property(PropertyInt::ItemMaxMana, 2000);
        let book = in_pack(&mut ts, BOOK);
        let stone = on_ground(&mut ts, LIFESTONE, at(22.0, 20.0));
        ts.advance(0.5);

        empyrean_world::network::game_messages::game_message::start_capture();
        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &ItemQueryItemMana {
                    object: ObjectId(wand.full()),
                },
            )
        });
        assert_eq!(
            kinds(&a),
            [QUERY_ITEM_MANA_RESPONSE],
            "one mana response is sent"
        );
        crate::support::messages::assert_sent_kinds(session, &[QUERY_ITEM_MANA_RESPONSE]);
        let answers: Vec<ItemQueryItemManaResponse> = all(&a, QUERY_ITEM_MANA_RESPONSE)
            .iter()
            .map(|m| m.decode())
            .collect();
        assert_eq!(
            answers,
            [ItemQueryItemManaResponse {
                object: ObjectId(wand.full()),
                mana: 0.25,
                success: 1
            }],
            "{:04X?}",
            kinds(&a)
        );

        empyrean_world::network::game_messages::game_message::start_capture();
        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &ItemQueryItemMana {
                    object: ObjectId(book.full()),
                },
            )
        });
        assert_eq!(
            kinds(&a),
            [QUERY_ITEM_MANA_RESPONSE],
            "one mana response is sent"
        );
        crate::support::messages::assert_sent_kinds(session, &[QUERY_ITEM_MANA_RESPONSE]);
        let answers: Vec<ItemQueryItemManaResponse> = all(&a, QUERY_ITEM_MANA_RESPONSE)
            .iter()
            .map(|m| m.decode())
            .collect();
        assert_eq!(
            answers,
            [ItemQueryItemManaResponse {
                object: ObjectId(book.full()),
                mana: 0.0,
                success: 0
            }]
        );

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &ItemQueryItemMana {
                    object: ObjectId(stone.full()),
                },
            )
        });
        assert!(
            all(&a, QUERY_ITEM_MANA_RESPONSE).is_empty(),
            "not on the player: {:04X?}",
            kinds(&a)
        );
        assert_eq!(
            ts.world
                .objects
                .get(ObjectGuid::new(ALPHA))
                .unwrap()
                .mana_query_target(),
            Some(stone.full())
        );
    }

    /// `@delete` (AdminCommands.HandleDeleteSelected) on the object selected by the mana query:
    /// `WorldObject.DeleteObject(rootOwner)` (WorldObject_Decay.cs) takes it out of the player's pack
    /// with networking (`RemoveFromInventoryAction.ConsumeItem`) and destroys it, and the admin gets
    /// the `GameMessageDeleteObject`.
    #[test]
    fn delete_removes_the_selected_item_from_the_pack() {
        let mut ts = server();
        empyrean_command::command_manager::initialize(None);
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let wand = in_pack(&mut ts, WAND);
        obj_mut(&mut ts, ObjectGuid::new(ALPHA)).set_is_admin_prop(true);
        ts.advance(0.5);
        let _ = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(
                alpha,
                &ItemQueryItemMana {
                    object: ObjectId(wand.full()),
                },
            )
        });

        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &CommunicationTalk {
                    message: "@delete".to_owned(),
                },
            )
        });
        assert!(
            ts.world.objects.get(wand).is_none(),
            "destroyed: {:?} {:04X?}",
            chats(&a),
            kinds(&a)
        );
        let p = ts.world.objects.get(ObjectGuid::new(ALPHA)).unwrap();
        assert!(
            !empyrean_world::world_objects::container::inventory(p).contains_key(&wand),
            "out of the pack"
        );
        let deletes: Vec<ItemDeleteObject> =
            all(&a, OBJECT_DELETE).iter().map(|m| m.decode()).collect();
        assert!(
            !deletes.is_empty() && deletes.iter().all(|d| d.id.0 == wand.full()),
            "{:04X?}",
            kinds(&a)
        );
    }

    /// Player_Inventory.cs gates on `Player.suicideInProgress` (Player_Death.cs) and
    /// `Player.IsOlthoiPlayer` (Player_Properties.cs), now the real fields: a player committing suicide
    /// cannot move an item (WeenieError YoureTooBusy, 0x001D), and an Olthoi player cannot give one
    /// ("Olthoi cannot trade items with other players!").
    #[test]
    fn suicide_and_olthoi_gates_refuse_inventory_actions() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let (_bravo, _) = join(&mut ts, "bravo", ALPHA + 1, "Bravo", at(21.0, 20.0));
        let wand = in_pack(&mut ts, WAND);
        ts.advance(0.5);
        let me = ObjectGuid::new(ALPHA);

        obj_mut(&mut ts, me)
            .player
            .as_mut()
            .unwrap()
            .player_death
            .suicide_in_progress = true;
        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryPutItemInContainer {
                    item: ObjectId(wand.full()),
                    container: ObjectId(ALPHA),
                    slot: 0,
                },
            );
        });
        let errors: Vec<u32> = all(&a, WEENIE_ERROR)
            .iter()
            .map(|m| u32::from_le_bytes(m.blob[16..20].try_into().unwrap()))
            .collect();
        assert_eq!(errors, [0x001D], "YoureTooBusy: {:04X?}", kinds(&a));
        obj_mut(&mut ts, me)
            .player
            .as_mut()
            .unwrap()
            .player_death
            .suicide_in_progress = false;

        obj_mut(&mut ts, me)
            .player
            .as_mut()
            .unwrap()
            .player_properties
            .is_olthoi_player = true;
        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &InventoryGiveObjectRequest {
                    target: ObjectId(ALPHA + 1),
                    item: ObjectId(wand.full()),
                    amount: 1,
                },
            );
        });
        let transients: Vec<String> = all(&a, TRANSIENT)
            .iter()
            .map(|m| m.decode::<CommunicationTransientString>().text)
            .collect();
        assert_eq!(
            transients,
            ["Olthoi cannot trade items with other players!"],
            "{:04X?}",
            kinds(&a)
        );
        assert!(
            empyrean_world::world_objects::container::inventory(ts.world.objects.get(me).unwrap())
                .contains_key(&wand),
            "kept"
        );
    }
}
