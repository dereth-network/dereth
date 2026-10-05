use super::*;

/// Behaviour: map.teleport.refuses-invalid-coordinates
#[test]
fn map_teleport_uses_bounded_landscape_cells_and_preserves_destination() {
    let mut world = dereth_client_model::World::new();
    let mut interaction = Interaction::new();
    for (x, y, expected) in [
        (0, 0, Some(1)),
        (7, 7, Some(0x40)),
        (0xa9, 0xb4, Some(0x1516_000d)),
        (0x7f8, 0, None),
        (0, 0x7f8, None),
        (u32::MAX, 0, None),
        (0, u32::MAX, None),
    ] {
        interaction.queue(vec![], vec![UiRequest::MapTeleport { x, y }]);
        assert!(interaction
            .run_ui_requests(&mut world, false, ServerTime(1.0))
            .is_empty());
        let requests = interaction.take_pending_requests();
        if let Some(cell) = expected {
            let [Request::AdvocateTeleport(request)] = requests.as_slice() else {
                panic!("{requests:?}")
            };
            assert_eq!(request.destination.objcell_id, cell);
            assert_eq!(
                request.destination.frame.origin,
                dereth_primitives::Vec3::new(10.0, 10.0, 0.0).into()
            );
            assert_eq!(request.destination.frame.orientation.w, 1.0);
            assert!(request.target_name.is_empty());
        } else {
            assert!(requests.is_empty(), "{x}, {y}");
        }
    }
}

/// Behaviour: chat.target-sweep
#[test]
fn chat_target_sweep_runs_without_a_panel_obeys_the_boundary_and_replaces_nameless_targets() {
    use dereth_client_contract::chat::mainchat::AutoTargetWorld;
    use dereth_client_model::chat::TalkFocus;
    let mut game = dereth_client_model::World::new();
    let mut inter = Interaction::new();
    let mut facts = AutoTargetWorld {
        player_id: 1,
        selected_id: 2,
        selected_talkable: true,
        selected_name: "First".into(),
        in_range_of_player: vec![2],
        ..Default::default()
    };
    inter.update_chat_target(&mut game, 4.0, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(2)));
    game.chat.set_talk_focus(TalkFocus::Selected);
    facts.in_range_of_player.clear();
    inter.update_chat_target(&mut game, 4.999, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(2)));
    inter.update_chat_target(&mut game, 5.0, &facts);
    assert_eq!(game.chat.last_speakable_target, None);
    assert_eq!(game.chat.talk_focus, TalkFocus::All);
    facts.selected_name.clear();
    facts.in_range_of_player = vec![2, 3];
    inter.update_chat_target(&mut game, 6.0, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(2)));
    assert!(!game.chat.is_talk_focus_enabled(TalkFocus::Selected));
    facts.selected_id = 3;
    facts.selected_name = "Named".into();
    inter.update_chat_target(&mut game, 7.0, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(3)));
    assert!(game.chat.is_talk_focus_enabled(TalkFocus::Selected));
    facts.container_id = 9;
    facts.in_range_of_player = vec![9];
    inter.update_chat_target(&mut game, 8.0, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(3)));
    facts.in_range_of_player.clear();
    facts.owned_by_player = true;
    inter.update_chat_target(&mut game, 9.0, &facts);
    assert_eq!(game.chat.last_speakable_target, Some(ObjectId(3)));
}
/// Behaviour: chat.entry-history
#[test]
fn runtime_entry_requests_update_before_following_input_and_submission_records_once_per_window() {
    use dereth_client_contract::chat::entry::{EntryAction, ReplyTarget};
    let mut game = dereth_client_model::World::new();
    game.chat.last_teller_name = "Peer".into();
    let mut inter = Interaction::new();
    inter.queue(
        Vec::new(),
        vec![UiRequest::ChatEntry {
            window: 8,
            text: "old".into(),
            action: EntryAction::Reply {
                target: ReplyTarget::LastTeller,
                prefix: "@tell ".into(),
            },
        }],
    );
    inter.run_ui_requests(&mut game, false, ServerTime(1.0));
    let mut update = inter.take_chat_entry_updates().pop().unwrap();
    assert_eq!(update.text, "@tell Peer, ");
    assert!(inter.take_chat_entry_updates().is_empty());
    update.text.push_str("hello");
    inter.queue(
        Vec::new(),
        vec![
            UiRequest::ChatLine {
                window: 8,
                text: update.text.clone(),
            },
            UiRequest::ChatLine {
                window: 8,
                text: String::new(),
            },
            UiRequest::ChatLine {
                window: 1,
                text: "other".into(),
            },
        ],
    );
    inter.run_ui_requests(&mut game, false, ServerTime(2.0));
    assert_eq!(game.chat.entries[&8].history(), [update.text]);
    assert_eq!(game.chat.entries[&1].history(), ["other"]);
    assert!(game.chat.entries[&8].text.is_empty());
    assert_eq!(inter.stats.chat_lines_sent, 2);
}

/// Behaviour: spellbook.filter.changes-are-shared-before-the-server-replies
#[test]
fn a_spellbook_filter_change_updates_the_model_view_and_saved_module() {
    use crate::hud::{Hud, HudView};
    use dereth_client_contract::view::GameView;
    let mut game = dereth_client_model::World::new();
    let module = dereth_protocol::login::PlayerModule::default();
    game.player_system.apply_player_module(&module);
    let mut hud = Hud::new();
    hud.player_module = Some(module);
    let mut inter = Interaction::new();
    let mask = 0x1011;
    inter.queue(Vec::new(), vec![UiRequest::SetSpellbookFilter { mask }]);
    assert!(inter
        .run_ui_requests(&mut game, false, ServerTime(1.0))
        .is_empty());
    assert_eq!(game.player_system.spell_filters, mask);
    assert_eq!(
        game.player_system
            .client_packed_module()
            .unwrap()
            .spell_filters,
        mask
    );
    assert_eq!(
        HudView {
            hud: &hud,
            world: &game
        }
        .spell_filters(),
        mask
    );
    assert!(
        matches!(inter.pending_requests(), [Request::SpellbookFilterEvent(m)] if m.filter_mask == mask)
    );
}

#[test]
fn hover_cannot_replace_pending_click_and_hidden_or_absent_answers_complete_with_zero() {
    let mut game = dereth_client_model::World::new();
    let target = ObjectId(42);
    let mut weenie = dereth_client_model::weenie::Weenie::new(target);
    weenie.pwd.bitfield = 0x80; // Hidden UI flag; visibility checks use the low byte.
    game.tables.weenies.insert(target, weenie);
    game.selected = Some(ObjectId(99));
    let mut inter = Interaction::new();
    for reason in [
        SearchReason::Select,
        SearchReason::Drop,
        SearchReason::TargetedUse,
    ] {
        inter.reason = reason;
        inter.pick.set_found_object(ObjectId(17), 2);
        inter.dispatch_ui_hover(
            (100, 100),
            Some(target),
            (800, 600),
            &mut game,
            ServerTime(1.0),
        );
        assert_eq!(
            inter.reason, reason,
            "a mouse-move or frame-loop hover must not overwrite pending work"
        );
        assert_eq!(inter.pick.click_object(), (ObjectId(17), 2));
    }
    for answer in [target, ObjectId(123)] {
        inter.reason = SearchReason::Drop;
        inter.drop_item = ObjectId(456); // Absent source: refuse, but still finish the drop tail.
        inter.pick.set_found_object(answer, 3);
        inter.on_world_object_found(answer, &mut game, ServerTime(2.0));
        assert_eq!(inter.pick.click_object(), (ObjectId(0), -1));
        assert_eq!(inter.reason, SearchReason::None);
        assert_eq!(inter.drop_item, ObjectId(0));
        assert_eq!(game.selected, Some(ObjectId(99)));
        assert!(inter.pending_requests().is_empty());
    }
    game.weenie_mut(target).unwrap().pwd.bitfield = 0;
    inter.dispatch_ui_hover(
        (100, 100),
        Some(target),
        (800, 600),
        &mut game,
        ServerTime(3.0),
    );
    assert_eq!(inter.pick.click_object(), (target, -1));
    assert_eq!(
        game.selected,
        Some(ObjectId(99)),
        "positive hover remains non-selecting"
    );
}

/// Oracle: the `SearchReason` ordering and every threshold comparison in the smart-box
/// wrapper's mouse table.
#[test]
fn the_search_reasons_are_ordered_as_the_client_compares_them() {
    assert!(SearchReason::None < SearchReason::MouseOver);
    assert!(SearchReason::MouseOver < SearchReason::Select);
    assert!(SearchReason::Select < SearchReason::Examine);
    assert!(SearchReason::Examine < SearchReason::Use);
    assert!(SearchReason::Use < SearchReason::Drop);
    assert!(SearchReason::Drop < SearchReason::Drag);
    assert!(SearchReason::Drag < SearchReason::TargetedUse);
    assert_eq!(SearchReason::TargetedUse as i32, 7);
}

/// Oracle: the toolbar's message-1 handler maps element ids `0x10000192` through
/// `0x10000195` to combat-mode toggling.
#[test]
fn the_four_stance_icons_are_the_ids_the_toolbar_names() {
    for id in 0x1000_0192..=0x1000_0195u32 {
        assert!(is_combat_mode_button(ElementId(id)), "{id:#X}");
    }
    assert!(!is_combat_mode_button(ElementId(0x1000_0191)));
    assert!(!is_combat_mode_button(ElementId(0x1000_0196)));
}

/// Oracle: `crate::actions::names::ACTION_ENUM_NAMES`, the shipped `ActionMap`'s own enum table.
#[test]
fn the_action_ids_are_the_ones_the_shipped_action_map_names() {
    let name = |a: u32| crate::actions::names::enum_name_for_action(crate::actions::ActionId(a));
    assert_eq!(name(action::COMBAT_TOGGLE_COMBAT.0), "CombatToggleCombat");
    assert_eq!(name(action::COMBAT_LOW_ATTACK.0), "CombatLowAttack");
    assert_eq!(name(action::COMBAT_MEDIUM_ATTACK.0), "CombatMediumAttack");
    assert_eq!(name(action::COMBAT_HIGH_ATTACK.0), "CombatHighAttack");
    assert_eq!(name(action::SELECTION_EXAMINE.0), "SelectionExamine");
    assert_eq!(name(action::USE.0), "USE");
}

/// Oracle: `SplitState`'s contract: `split_size == max_split_size` means *move everything*;
/// there is no separate boolean.
#[test]
fn the_default_split_state_is_a_whole_stack() {
    let i = Interaction::new();
    assert!(
        dereth_client_model::World::new().split.is_whole_stack(),
        "0 >= 0, so an untouched slider moves the stack"
    );
    assert_eq!(i.search_reason(), SearchReason::None);
    assert_eq!(i.target_mode(), TargetMode::None);
}

#[test]
fn trade_split_notices_replay_in_the_order_the_batch_raised_them() {
    const SOURCE: ObjectId = ObjectId(0x7100_0001);
    const RESULT: ObjectId = ObjectId(0x7100_0002);
    fn game() -> dereth_client_model::World {
        let mut game = dereth_client_model::World::new();
        let mut result = dereth_client_model::Weenie::new(RESULT);
        result.pwd.wcid = 0xCAFE;
        result.pwd.stack_size = Some(3);
        game.tables.weenies.insert(RESULT, result);
        game.trade.pending_split = Some(dereth_client_model::trade::PendingTradeSplit {
            source: SOURCE,
            wcid: 0xCAFE,
            stack_size: 3,
        });
        game
    }

    let mut matched_first = game();
    let mut out = Notices::default();
    dereth_client_model::NoticeSink::emit(
        &mut out,
        Notice::ItemAttributesChanged {
            object: RESULT,
            kind: 1,
        },
    );
    dereth_client_model::NoticeSink::emit(
        &mut out,
        Notice::AttemptFailed {
            object: SOURCE,
            reason: 0,
        },
    );
    let mut inter = Interaction::new();
    inter.absorb(&mut matched_first, out, RecordingRequests::default());
    assert_eq!(inter.take_trade_for_dummies(), vec![RESULT]);

    let mut failed_first = game();
    let mut out = Notices::default();
    dereth_client_model::NoticeSink::emit(
        &mut out,
        Notice::AttemptFailed {
            object: SOURCE,
            reason: 0,
        },
    );
    dereth_client_model::NoticeSink::emit(
        &mut out,
        Notice::ItemAttributesChanged {
            object: RESULT,
            kind: 1,
        },
    );
    let mut inter = Interaction::new();
    inter.absorb(&mut failed_first, out, RecordingRequests::default());
    assert!(inter.take_trade_for_dummies().is_empty());
    assert!(failed_first.trade.pending_split.is_none());
}

/// Changing target mode clears a pending leave request, while setting the current mode again
/// preserves it. A generic use can therefore enter targeted-use mode without a stale request
/// canceling that transition during the next use-time update.
#[test]
fn target_mode_change_cancels_pending_leave_but_same_mode_does_not() {
    let mut i = Interaction::new();
    i.set_target_mode(TargetMode::Use);
    i.leave_target_mode = true;
    i.set_target_mode(TargetMode::UseTarget);
    i.run_leave_target_mode();
    assert_eq!(i.target_mode(), TargetMode::UseTarget);
    i.leave_target_mode = true;
    i.set_target_mode(TargetMode::UseTarget);
    i.run_leave_target_mode();
    assert_eq!(i.target_mode(), TargetMode::None);
    assert!(!i.leave_target_mode);
}

/// The make-shortcut key on the selected object: the first empty slot takes it, with the
/// add sent to the server; asked again, the object already has one and the player is told;
/// with every slot full the player is told that instead.
#[test]
fn the_make_shortcut_key_fills_the_first_empty_slot_or_says_why_not() {
    const PLAYER: ObjectId = ObjectId(0x5000_0001);
    const COAT: ObjectId = ObjectId(0x8000_0010);
    let mut game = dereth_client_model::World::new();
    game.player = Some(PLAYER);
    game.tables
        .weenies
        .insert(PLAYER, dereth_client_model::weenie::Weenie::new(PLAYER));
    let mut coat = dereth_client_model::weenie::Weenie::new(COAT);
    coat.pwd.name = "Academy Coat".to_owned();
    coat.pwd.container_id = Some(PLAYER);
    game.tables.weenies.insert(COAT, coat);
    // Slot 0 is taken by something else, so the first empty slot is 1.
    game.player_system
        .add_shortcut(dereth_protocol::login::ShortCutData {
            index: 0,
            object_id: PLAYER,
            spell_id: 0,
        });
    let lines = |game: &dereth_client_model::World| -> Vec<(u32, String)> {
        game.scroll
            .pending()
            .iter()
            .map(|f| (f.chat_type, f.body.clone()))
            .collect()
    };

    let mut inter = Interaction::new();
    inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
    assert!(inter
        .run_ui_requests(&mut game, false, ServerTime(1.0))
        .is_empty());
    assert_eq!(game.player_system.shortcut_at(1), Some(COAT));
    assert!(
        matches!(
            inter.pending_requests(),
            [Request::AddShortCut(m)] if m.shortcut.index == 1 && m.shortcut.object_id == COAT
        ),
        "{:?}",
        inter.pending_requests()
    );

    let mut inter = Interaction::new();
    inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
    inter.run_ui_requests(&mut game, false, ServerTime(2.0));
    assert!(
        inter.pending_requests().is_empty(),
        "{:?}",
        inter.pending_requests()
    );
    assert!(
        lines(&game).contains(&(
            0x1A,
            "There is already a shortcut to the Academy Coat".to_owned()
        )),
        "{:?}",
        lines(&game)
    );

    game.player_system.remove_shortcut(1);
    for n in 1..dereth_client_model::player::SHORTCUT_SLOTS {
        game.player_system
            .add_shortcut(dereth_protocol::login::ShortCutData {
                index: i32::try_from(n).unwrap(),
                object_id: PLAYER,
                spell_id: 0,
            });
    }
    let mut inter = Interaction::new();
    inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
    inter.run_ui_requests(&mut game, false, ServerTime(3.0));
    assert!(
        inter.pending_requests().is_empty(),
        "{:?}",
        inter.pending_requests()
    );
    assert!(
        lines(&game).contains(&(0x1A, "There are no free shortcut slots".to_owned())),
        "{:?}",
        lines(&game)
    );
}

/// Every `PlayerOption_*` row names the option its action spells, no action or option twice.
#[test]
fn the_player_option_actions_name_their_own_options() {
    use dereth_client_contract::actions::{names::enum_name_for_action, ActionId};
    use dereth_client_model::player::options::PLAYER_OPTIONS;
    let mut actions = std::collections::BTreeSet::new();
    let mut options = std::collections::BTreeSet::new();
    for (a, o) in PLAYER_OPTION_ACTIONS {
        let want = format!("PlayerOption_{}", PLAYER_OPTIONS[o].0);
        assert_eq!(enum_name_for_action(ActionId(a)), want, "{a:#x}");
        assert!(actions.insert(a) && options.insert(o));
    }
    assert_eq!(player_option_action(0x1000_013F), Some(52));
    assert_eq!(player_option_action(0x1000_0125), Some(46));
    assert_eq!(
        player_option_action(0x1000_012F),
        Some(50),
        "show-cloak flips its option"
    );
}

/// A bound hear-PK-deaths key flips the option both ways, and each flip is saved at once as a
/// `0x0005`; a non-auto-saved option only marks the module dirty.
#[test]
fn a_player_option_action_flips_its_option_and_saves_an_auto_saved_one_at_once() {
    use dereth_client_contract::actions::{Action, ActionId};
    use dereth_client_model::player::options::option;
    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    let phys = crate::selection_geometry::SceneSelectionPhysics::default();
    let press = |inter: &mut Interaction, game: &mut dereth_client_model::World, id| {
        inter.on_actions(
            vec![Action::begin(ActionId(id))],
            game,
            &phys,
            0.0,
            false,
            dereth_primitives::LocalTime(1.0),
            None,
        )
    };
    let sent = |inter: &mut Interaction| -> Vec<(u32, u32)> {
        inter
            .take_pending_requests()
            .into_iter()
            .filter_map(|r| match r {
                Request::PlayerOptionChanged(e) => Some((e.option, e.value)),
                _ => None,
            })
            .collect()
    };
    assert!(game.player_system.options.hear_pk_deaths());
    assert!(press(&mut inter, &mut game, 0x1000_013F).is_empty());
    assert!(!game.player_system.options.hear_pk_deaths());
    assert_eq!(sent(&mut inter), vec![(52, 0)]);
    assert!(press(&mut inter, &mut game, 0x1000_013F).is_empty());
    assert!(game.player_system.options.hear_pk_deaths());
    assert_eq!(sent(&mut inter), vec![(52, 1)]);

    // Advanced combat UI is not saved at once: it only dirties the module.
    assert!(!game.player_system.options.get(option::ADVANCED_COMBAT_UI));
    assert!(press(&mut inter, &mut game, 0x1000_007D).is_empty());
    assert!(game.player_system.options.get(option::ADVANCED_COMBAT_UI));
    assert!(sent(&mut inter).is_empty());
    assert!(game.player_system.is_dirty());
    assert_eq!(inter.stats.option_actions_toggled, 3);

    // An action with no arm is handed back.
    let left = press(&mut inter, &mut game, 0x1000_0400);
    assert_eq!(left.len(), 1);
}

/// The four item keys of the selection map: Select Self selects the player; Give hands the
/// selected item to the creature selected before it, and refuses out loud when that is no
/// creature; Drop refuses an item the player does not carry; Move to Main Pack asks for the
/// main pack. Each is taken, whatever it then does.
#[test]
fn the_select_self_give_drop_and_main_pack_keys_act_on_the_selection() {
    use dereth_client_contract::actions::{Action, ActionId};
    use {dereth_client_model::weenie::Weenie, dereth_rules::weenie::item_type};
    const PLAYER: ObjectId = ObjectId(0x5000_0001);
    const COAT: ObjectId = ObjectId(0x8000_0010);
    const GUARD: ObjectId = ObjectId(0x8000_0020);
    const ROCK: ObjectId = ObjectId(0x8000_0030);
    let mut game = dereth_client_model::World::new();
    game.player = Some(PLAYER);
    game.tables.weenies.insert(PLAYER, Weenie::new(PLAYER));
    let mut coat = Weenie::new(COAT);
    coat.pwd.name = "Academy Coat".to_owned();
    coat.pwd.container_id = Some(PLAYER);
    game.tables.weenies.insert(COAT, coat);
    let mut guard = Weenie::new(GUARD);
    guard.pwd.name = "Town Guard".to_owned();
    guard.pwd.obj_type = item_type::CREATURE;
    game.tables.weenies.insert(GUARD, guard);
    let mut rock = Weenie::new(ROCK);
    rock.pwd.name = "Rock".to_owned();
    game.tables.weenies.insert(ROCK, rock);
    let phys = crate::selection_geometry::SceneSelectionPhysics::default();
    let mut inter = Interaction::new();
    let mut press = |game: &mut dereth_client_model::World, id: u32| {
        let left = inter.on_actions(
            vec![Action::begin(ActionId(id))],
            game,
            &phys,
            0.0,
            false,
            dereth_primitives::LocalTime(1.0),
            None,
        );
        assert!(left.is_empty(), "{id:#x} is taken");
        inter.take_pending_requests()
    };
    let said = |game: &dereth_client_model::World, text: &str| {
        game.scroll
            .pending()
            .iter()
            .any(|f| f.chat_type == 0x1A && f.body.contains(text))
    };

    game.selected = Some(ROCK);
    press(&mut game, action::SELECTION_SELF.0);
    assert_eq!(
        game.selected,
        Some(PLAYER),
        "Select Self selects the player"
    );

    // The main pack: the player has no room in it, and says so.
    game.selected = Some(ROCK);
    assert!(press(&mut game, action::SELECTION_MOVE_TO_MAIN_PACK.0).is_empty());
    assert!(said(&game, "is completely full!"));

    game.selected = Some(COAT);
    game.prev_selected = Some(GUARD);
    let sent = press(&mut game, action::SELECTION_GIVE.0);
    assert!(
        sent.iter().any(
            |r| matches!(r, Request::GiveObjectRequest(g) if g.item == COAT
                && g.target == GUARD)
        ),
        "the coat goes to the guard: {sent:?}"
    );
    assert_eq!(
        game.selected,
        Some(GUARD),
        "and the guard is selected again"
    );

    game.selected = Some(COAT);
    game.prev_selected = Some(ROCK);
    assert!(press(&mut game, action::SELECTION_GIVE.0).is_empty());
    assert!(said(
        &game,
        "You must select a creature or a character to give that to."
    ));

    game.selected = Some(ROCK);
    assert!(press(&mut game, action::SELECTION_DROP.0).is_empty());
    assert!(said(&game, "You must pick that up first"));
}

/// Event `0x0318` queues its one string for a message box, exactly as `0x0004` does.
#[test]
fn event_0318_raises_the_same_pop_up_as_0004() {
    use dereth_client_net::client_session::SessionEvent;
    fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
        let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
        blob.extend(dereth_protocol::write_body(m).expect("a synthetic message encodes"));
        SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob,
        }
    }
    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    apply_events(
        &mut inter,
        &[
            ui_event(&dereth_protocol::comms::CommunicationPopUpString0318 {
                message: "Welcome to the Olthoi Horde.".into(),
            }),
            ui_event(&dereth_protocol::comms::CommunicationPopUpString {
                message: "Welcome back.".into(),
            }),
        ],
        &mut game,
    );
    assert_eq!(
        inter.take_pop_up_strings(),
        vec![
            "Welcome to the Olthoi Horde.".to_owned(),
            "Welcome back.".to_owned()
        ]
    );
    assert_eq!(inter.stats.pop_up_strings, 2);
    assert_eq!(inter.stats.pop_up_strings_undecodable, 0);
}
