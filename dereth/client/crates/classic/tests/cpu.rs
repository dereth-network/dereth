//! Classic panel gestures through the shared runtime.

use dereth_classic_ui::panels::{Context, ControlEvent, PanelAction};
use dereth_primitives::ObjectId;

/// Behaviour: container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open
#[test]
fn choosing_a_pack_updates_the_shared_pickup_destination() {
    use dereth_client_model::{weenie::Weenie, World};
    use dereth_client_runtime::{hud::Hud, interaction::Interaction};
    let player = ObjectId(1);
    let pack = ObjectId(2);
    let mut world = World::new();
    world.player = Some(player);
    world.tables.weenies.insert(player, Weenie::new(player));
    let mut bag = Weenie::new(pack);
    bag.pwd.container_id = Some(player);
    world.tables.weenies.insert(pack, bag);
    let mut inventory = dereth_client_model::objects::ObjectInventory::default();
    inventory.containers.push(pack);
    world.tables.inventories.insert(player, inventory);
    let hud = Hud::new();
    let pregame = Default::default();
    let keyboard = Default::default();
    let settings = Default::default();
    let classic = Default::default();
    let mut panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
    for (control, destination) in [("containers", pack), ("backpack", player)] {
        let view = dereth_client_runtime::hud::HudView {
            hud: &hud,
            world: &world,
        };
        let ctx = Context {
            now: dereth_primitives::LocalTime(0.0),
            resources: &dereth_classic_ui::resources::Resources::default(),
            layout: Default::default(),
            game: &view,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let actions = panel.event(
            ControlEvent::Select {
                id: control.into(),
                index: 0,
            },
            &ctx,
        );
        let requests = actions
            .into_iter()
            .filter_map(|action| match action {
                PanelAction::Game(request) => Some(request),
                _ => None,
            })
            .collect();
        let mut interaction = Interaction::new();
        interaction.queue(Vec::new(), requests);
        interaction.run_ui_requests(&mut world, false, dereth_primitives::ServerTime(1.0));
        assert_eq!(world.open_container, Some(destination));
    }
}

fn inventory_world() -> dereth_client_model::World {
    use dereth_client_model::{
        objects::ObjectInventory,
        weenie::{bitfield, Weenie},
        World,
    };
    let mut world = World::new();
    let player = ObjectId(1);
    world.player = Some(player);
    for (id, parent, kind) in [
        (1, None, 0x10),
        (2, Some(player), 0x200),
        (3, Some(player), 1),
        (4, Some(ObjectId(2)), 1),
        (5, None, 1),
    ] {
        let mut item = Weenie::new(ObjectId(id));
        item.pwd.obj_type = kind;
        item.pwd.container_id = parent;
        item.valid = true;
        if id <= 2 {
            item.pwd.items_capacity = Some(24);
        }
        if id == 1 {
            item.pwd.containers_capacity = Some(7);
        }
        if id == 2 {
            item.pwd.bitfield = bitfield::OPENABLE;
        }
        if id == 1 {
            item.pwd.bitfield = bitfield::PLAYER;
        }
        world.tables.weenies.insert(ObjectId(id), item);
    }
    world.tables.inventories.insert(
        player,
        ObjectInventory {
            items: vec![ObjectId(3)],
            containers: vec![ObjectId(2)],
            ..Default::default()
        },
    );
    world.tables.inventories.insert(
        ObjectId(2),
        ObjectInventory {
            items: vec![ObjectId(4)],
            ..Default::default()
        },
    );
    world
}

/// Behaviour: inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things
#[test]
fn a_departed_open_pack_returns_both_the_grid_and_next_pickup_to_the_player() {
    use dereth_client_contract::GameView;
    use dereth_client_model::inventory::SplitState;
    use dereth_client_model::{RecordingRequests, RecordingSink, Request};
    use dereth_client_runtime::hud::{Hud, HudView};
    let mut world = inventory_world();
    assert!(world.on_new_parent_container(ObjectId(2)));
    let mut out = RecordingSink::default();
    // An unannounced containment update does not run the inventory notice.
    world.server_says_move_item(ObjectId(2), ObjectId(0), 0, ObjectId(0), 0, false, &mut out);
    assert_eq!(world.open_container, Some(ObjectId(2)));
    world.server_says_move_item(ObjectId(2), ObjectId(1), 0, ObjectId(0), 0, true, &mut out);
    assert_eq!(
        world.open_container,
        Some(ObjectId(2)),
        "a reorder within the player keeps the pack open"
    );
    world.server_says_move_item(ObjectId(3), ObjectId(0), 0, ObjectId(0), 0, true, &mut out);
    assert_eq!(
        world.open_container,
        Some(ObjectId(2)),
        "a different moved object leaves the pack open"
    );
    world.server_says_move_item(ObjectId(2), ObjectId(0), 0, ObjectId(0), 0, true, &mut out);
    assert_eq!(world.open_container, Some(ObjectId(1)));
    let hud = Hud::new();
    assert_eq!(
        HudView {
            hud: &hud,
            world: &world
        }
        .open_inventory_container(),
        Some(ObjectId(1))
    );
    let mut requests = RecordingRequests::default();
    assert!(world.place_in_backpack(
        &mut requests,
        &mut out,
        ObjectId(5),
        false,
        SplitState::whole_stack(1),
        dereth_primitives::ServerTime(1.0)
    ));
    assert!(
        matches!(requests.0.as_slice(), [Request::PutItemInContainer(m)] if m.container == ObjectId(1))
    );
}

/// Behaviour: container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open
#[test]
fn a_recreated_classic_inventory_reads_the_shared_pack_selection() {
    use dereth_classic_ui::panels::ControlKind;
    use dereth_client_contract::{snapshot::GameSnapshot, GameView};
    use dereth_client_runtime::hud::{Hud, HudView};
    let mut world = inventory_world();
    assert!(world.on_new_parent_container(ObjectId(2)));
    let hud = Hud::new();
    let snapshot = GameSnapshot::from_view(&HudView {
        hud: &hud,
        world: &world,
    });
    assert_eq!(snapshot.open_inventory_container(), Some(ObjectId(2)));
    for _ in 0..2 {
        let panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
        let context = Context {
            now: dereth_primitives::LocalTime(0.0),
            resources: &dereth_classic_ui::resources::Resources::default(),
            layout: Default::default(),
            game: &snapshot,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        };
        let frame = panel.frame(&context);
        let grid = frame.controls.iter().find(|c| c.id == "items").unwrap();
        let ControlKind::Items { entries, .. } = &grid.kind else {
            panic!("an item grid")
        };
        assert_eq!(entries[0].id, ObjectId(4), "the open side pack's item");
    }
}

/// Behaviour: inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things
#[test]
fn deleting_the_open_pack_restores_the_main_pack_before_removal() {
    let mut world = inventory_world();
    world.on_new_parent_container(ObjectId(2));
    let mut saw_move = false;
    world.delete_object_with_dispatch(
        ObjectId(2),
        dereth_primitives::ServerTime(1.0),
        &mut |world, notice| {
            if matches!(
                notice,
                dereth_client_model::Notice::ItemMoved {
                    object: ObjectId(2),
                    ..
                }
            ) {
                assert!(world.weenie(ObjectId(2)).is_some());
                assert_eq!(world.open_container, Some(ObjectId(1)));
                saw_move = true;
            }
        },
    );
    assert!(saw_move);
    assert!(world.weenie(ObjectId(2)).is_none());
    assert_eq!(world.open_container, Some(ObjectId(1)));
}

fn character_squelch_from_panel(name: &str, add: bool) {
    use dereth_client_contract::{view::SquelchEntry, GameView, UiRequest};
    use dereth_client_model::{Request, World};
    use dereth_client_runtime::interaction::Interaction;
    #[derive(Debug)]
    struct View(String);
    impl GameView for View {
        fn squelch_list(&self) -> Vec<SquelchEntry> {
            vec![SquelchEntry {
                name: self.0.clone(),
                account: false,
            }]
        }
    }
    dereth_client_contract::options::store::init();
    let view = View(name.to_owned());
    let context = Context {
        now: dereth_primitives::LocalTime(0.0),
        resources: &dereth_classic_ui::resources::Resources::default(),
        layout: Default::default(),
        game: &view,
        pregame: &Default::default(),
        keyboard: &Default::default(),
        settings: &Default::default(),
        map_teleport_allowed: false,
        classic: &Default::default(),
    };
    let mut panel = dereth_classic_ui::panels::services::make("squelch").unwrap();
    panel.event(
        if add {
            ControlEvent::Edit {
                id: "entry".into(),
                text: name.to_owned(),
            }
        } else {
            ControlEvent::Select {
                id: "squelch-rows".into(),
                index: 0,
            }
        },
        &context,
    );
    let actions = panel.event(
        ControlEvent::Activate(
            if add {
                "squelch-character"
            } else {
                "unsquelch"
            }
            .into(),
        ),
        &context,
    );
    assert_eq!(
        actions,
        vec![PanelAction::Game(UiRequest::ModifyCharacterSquelch {
            object: ObjectId(0),
            add,
            account: name.to_owned(),
            message_type: 1,
        })]
    );
    let mut interaction = Interaction::new();
    interaction.queue(
        Vec::new(),
        actions
            .into_iter()
            .filter_map(|action| match action {
                PanelAction::Game(request) => Some(request),
                _ => None,
            })
            .collect(),
    );
    let mut world = World::new();
    world.chat.last_teller_name = "Somebody Else".into();
    interaction.run_ui_requests(&mut world, false, dereth_primitives::ServerTime(1.0));
    let [Request::ModifyCharacterSquelch(message)] = interaction.pending_requests() else {
        panic!("one character squelch message")
    };
    assert_eq!(message.add, i32::from(add));
    assert_eq!(message.character_id, ObjectId(0));
    assert_eq!(message.character_name, name);
    assert_eq!(message.msg_type, 1);
}

/// Behaviour: chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name
#[test]
fn adding_a_character_squelch_keeps_the_name_literal() {
    for name in ["+Name", "-reply"] {
        character_squelch_from_panel(name, true);
    }
}

/// Behaviour: chat.squelch-panel.removing-sends-the-kind-the-row-itself-names
#[test]
fn removing_a_character_squelch_keeps_the_name_literal() {
    for name in ["+Name", "-reply"] {
        character_squelch_from_panel(name, false);
    }
}

fn equipment_world(mask: u32) -> dereth_client_model::World {
    let mut world = inventory_world();
    world
        .tables
        .weenies
        .get_mut(world.player.unwrap())
        .unwrap()
        .qualities
        .get_or_insert_with(Default::default)
        .ints
        .get_or_insert_with(Default::default)
        .insert(0x142, 7);
    let item = world.tables.weenies.get_mut(ObjectId(3)).unwrap();
    item.pwd.name = "Shirt".into();
    item.pwd.valid_locations = Some(mask);
    item.pwd.priority = Some(4);
    world
}

fn equipment_drop(
    world: &mut dereth_client_model::World,
    control: &str,
) -> dereth_client_runtime::interaction::Interaction {
    use dereth_classic_ui::panels::DragPayload;
    use dereth_client_runtime::{hud::Hud, interaction::Interaction};
    let hud = Hud::new();
    let view = dereth_client_runtime::hud::HudView { hud: &hud, world };
    let context = Context {
        now: dereth_primitives::LocalTime(0.0),
        resources: &dereth_classic_ui::resources::Resources::default(),
        layout: Default::default(),
        game: &view,
        pregame: &Default::default(),
        keyboard: &Default::default(),
        settings: &Default::default(),
        classic: &Default::default(),
        map_teleport_allowed: false,
    };
    let mut panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
    let actions = panel.event(
        ControlEvent::Drop {
            id: control.into(),
            payload: DragPayload::Object(ObjectId(3)),
            slot: 0,
        },
        &context,
    );
    assert_eq!(actions.len(), 1);
    let requests = actions
        .into_iter()
        .map(|action| match action {
            PanelAction::Game(request) => request,
            _ => panic!("equipment is a shared game request"),
        })
        .collect();
    let mut interaction = Interaction::new();
    world.set_waiting_state(ObjectId(3), true);
    interaction.queue(Vec::new(), requests);
    assert!(interaction
        .run_ui_requests(world, false, dereth_primitives::ServerTime(1.0))
        .is_empty());
    interaction
}

/// Behaviour: inventory.equip.a-weapon-let-go-on-the-shield-place-is-taken-into-the-off-hand
#[test]
fn classic_equipment_preserves_pairs_offhand_and_later_locations() {
    use dereth_client_model::Request;
    for (control, valid, expected) in [
        ("equip:1", 0x30000, 0x10000),
        ("equip:3", 0x30000, 0x20000),
        ("equip:2", 0xc0000, 0x40000),
        ("equip:4", 0xc0000, 0x80000),
        ("equip:5", 0x100000, 0x100000),
        ("equip:7", 0x100000, 0x200000),
        ("equip:5", 0x2000000, 0x2000000),
        ("accessory:0", 0x8000000, 0x8000000),
        ("accessory:1", 0x4000000, 0x4000000),
        ("accessory:2", 0x10000000, 0x10000000),
        ("accessory:3", 0x20000000, 0x20000000),
        ("accessory:4", 0x40000000, 0x40000000),
    ] {
        let mut world = equipment_world(valid);
        let interaction = equipment_drop(&mut world, control);
        let [Request::GetAndWieldItem(message)] = interaction.pending_requests() else {
            panic!(
                "{control} with {valid:#x} sends one wield: {:?}",
                interaction.pending_requests()
            );
        };
        assert_eq!((message.item, message.slot), (ObjectId(3), expected));
        assert_eq!(interaction.stats.requests_refused, 0);
    }
}

/// Behaviour: inventory.body.something-that-could-be-worn-or-wielded-is-still-worn-on-the-picture
#[test]
fn classic_clothing_and_canvas_send_the_whole_mask_but_other_slots_choose_one() {
    use dereth_client_model::Request;
    for (control, valid, expected, wear) in [
        ("equip:8", 0x1e, 0x1e, true),
        ("paperdoll", 0x200 | 0x100000, 0x200 | 0x100000, true),
        ("equip:5", 0x200 | 0x100000, 0x200, false),
    ] {
        let interaction = equipment_drop(&mut equipment_world(valid), control);
        let [Request::GetAndWieldItem(message)] = interaction.pending_requests() else {
            panic!("one wield")
        };
        assert_eq!(message.slot, expected);
        assert_eq!(interaction.stats.wears_requested, u64::from(wear));
        assert_eq!(interaction.stats.wields_requested, u64::from(!wear));
    }
}

/// Behaviour: inventory.equip.a-place-the-thing-cannot-go-in-refuses-it-and-a-place-it-can-takes-it
#[test]
fn classic_wrong_slot_is_silent_but_unwearable_canvas_speaks_once() {
    for (control, expected) in [
        ("equip:0", None),
        ("paperdoll", Some("You can't put that item there")),
    ] {
        let mut world = equipment_world(0x100000);
        let interaction = equipment_drop(&mut world, control);
        assert!(interaction.pending_requests().is_empty());
        assert_eq!(interaction.stats.requests_refused, 1);
        assert_eq!(interaction.last_refusal.as_deref(), expected);
        assert!(!world.weenie(ObjectId(3)).unwrap().waiting);
    }
}

/// Behaviour: inventory.equip.a-shield-is-refused-in-words-while-a-weapon-for-both-hands-is-held
#[test]
fn classic_shield_hover_and_drop_refuse_a_two_handed_weapon() {
    use dereth_client_contract::{snapshot::GameSnapshot, GameView};
    let mut world = equipment_world(0x200000);
    let ready = world.tables.weenies.get_mut(ObjectId(4)).unwrap();
    ready.pwd.name = "Spadone".into();
    ready.pwd.combat_use = Some(5);
    world.inv_slots.set(0x2000000, ObjectId(4));
    let hud = dereth_client_runtime::hud::Hud::new();
    let view = dereth_client_runtime::hud::HudView {
        hud: &hud,
        world: &world,
    };
    let snapshot = GameSnapshot::from_view(&view);
    assert_eq!(
        view.equipment_hover(ObjectId(3)).at_location(0x200000),
        Some(false)
    );
    assert_eq!(
        snapshot.equipment_hover(ObjectId(3)),
        view.equipment_hover(ObjectId(3))
    );
    let interaction = equipment_drop(&mut world, "equip:7");
    assert!(interaction.pending_requests().is_empty());
    assert_eq!(interaction.stats.requests_refused, 1);
    assert_eq!(
        interaction.last_refusal.as_deref(),
        Some("A shield may not be worn with the Spadone")
    );
}

/// Behaviour: inventory.body.something-already-worn-is-told-so-when-it-is-dropped
#[test]
fn classic_worn_clothing_speaks_but_a_worn_ring_refuses_silently() {
    for (control, mask, expected) in [
        ("equip:8", 2, Some("The Shirt is already being worn")),
        ("paperdoll", 2, Some("The Shirt is already being worn")),
        ("equip:2", 0x40000, None),
    ] {
        let mut world = equipment_world(mask);
        let item = world.tables.weenies.get_mut(ObjectId(3)).unwrap();
        item.pwd.location = Some(mask);
        item.pwd.wielder_id = world.player;
        world.inventory_mask = mask;
        world.clothing_priority_mask = 4;
        world
            .inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(3), mask, 4);
        world.inv_slots.set(mask, ObjectId(3));
        let interaction = equipment_drop(&mut world, control);
        assert!(interaction.pending_requests().is_empty());
        assert_eq!(interaction.stats.requests_refused, 1);
        assert_eq!(interaction.last_refusal.as_deref(), expected);
    }
}

/// Behaviour: inventory.drag-hint.a-body-slot-shows-whether-it-could-take-what-is-carried
#[test]
fn equipment_hover_snapshot_preserves_offhand_refusal_and_unchanged_states() {
    use dereth_client_contract::{snapshot::GameSnapshot, GameView};
    use dereth_client_runtime::hud::Hud;
    let hud = Hud::new();
    let mut world = equipment_world(0x100000);
    for (mask, worn, expected_slot, expected_canvas) in [
        (0x100000, false, Some(true), Some(false)),
        (0, false, Some(false), Some(false)),
        (2, false, Some(false), Some(true)),
        (2, true, Some(false), None),
    ] {
        let item = world.tables.weenies.get_mut(ObjectId(3)).unwrap();
        item.pwd.valid_locations = Some(mask);
        item.pwd.location = worn.then_some(mask);
        item.pwd.wielder_id = worn.then_some(ObjectId(1));
        world.clothing_priority_mask = if worn { 4 } else { 0 };
        world.inventory_mut(ObjectId(1)).unwrap().set_placement(
            ObjectId(3),
            if worn { mask } else { 0 },
            4,
        );
        let view = dereth_client_runtime::hud::HudView {
            hud: &hud,
            world: &world,
        };
        let snapshot = GameSnapshot::from_view(&view);
        for source in [&view as &dyn GameView, &snapshot] {
            let hover = source.equipment_hover(ObjectId(3));
            assert_eq!(hover.at_location(0x200000), expected_slot);
            assert_eq!(hover.canvas, expected_canvas);
            assert_eq!(source.equipment_hover(ObjectId(999)).at_location(2), None);
        }
    }
    let mut world = equipment_world(0x01000002);
    world.combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let view = dereth_client_runtime::hud::HudView {
        hud: &hud,
        world: &world,
    };
    let snapshot = GameSnapshot::from_view(&view);
    for source in [&view as &dyn GameView, &snapshot] {
        assert_eq!(
            source.equipment_hover(ObjectId(3)).at_location(2),
            Some(false)
        );
        assert_eq!(source.equipment_hover(ObjectId(3)).canvas, Some(true));
    }
}

/// Behaviour: inventory.body.every-place-on-the-figure-is-a-place-to-wear-and-no-cell-of-a-pack-is
#[test]
fn classic_ready_slot_displays_a_confirmed_two_handed_weapon() {
    use dereth_classic_ui::panels::ControlKind;
    let mut world = equipment_world(0x02000000);
    world.server_says_move_item(
        ObjectId(3),
        ObjectId(0),
        0,
        ObjectId(1),
        0x02000000,
        true,
        &mut dereth_client_model::RecordingSink::default(),
    );
    let mut objects = dereth_client_runtime::objects::ObjectStream::new();
    objects.world = world;
    let mut hud = dereth_client_runtime::hud::Hud::new();
    hud.sync(&objects, None);
    let view = hud.view(&objects);
    let context = Context {
        now: dereth_primitives::LocalTime(0.0),
        resources: &dereth_classic_ui::resources::Resources::default(),
        layout: Default::default(),
        game: &view,
        pregame: &Default::default(),
        keyboard: &Default::default(),
        settings: &Default::default(),
        classic: &Default::default(),
        map_teleport_allowed: false,
    };
    let panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
    let frame = panel.frame(&context);
    let ready = frame.controls.iter().find(|c| c.id == "equip:5").unwrap();
    let ControlKind::Items { entries, .. } = &ready.kind else {
        panic!("ready slot")
    };
    assert_eq!(
        entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ObjectId(3)]
    );
}

fn era_context<T>(
    view: &dyn dereth_client_contract::GameView,
    f: impl FnOnce(&Context<'_>) -> T,
) -> T {
    f(&Context {
        now: dereth_primitives::LocalTime(0.0),
        resources: &dereth_classic_ui::resources::Resources::default(),
        layout: Default::default(),
        game: view,
        pregame: &Default::default(),
        keyboard: &Default::default(),
        settings: &Default::default(),
        classic: &Default::default(),
        map_teleport_allowed: false,
    })
}

/// Behaviour: presentation.era.aetheria-slots-follow-character-unlocks
#[test]
fn classic_equipment_refreshes_unlocks_through_the_runtime_view_and_preserves_drop_locations() {
    use dereth_classic_ui::panels::DragPayload;
    use dereth_client_contract::{DropTarget, GameSnapshot, GameView, UiRequest};
    use dereth_client_runtime::hud::{Hud, HudView};
    let mut world = equipment_world(0x70000000);
    let mut hud = Hud::new();
    let player = world.player.unwrap();
    let mut panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
    for bits in [0, 1, 2, 4, 7, 0x100] {
        world
            .tables
            .weenies
            .get_mut(player)
            .unwrap()
            .qualities
            .as_mut()
            .unwrap()
            .ints
            .as_mut()
            .unwrap()
            .insert(0x142, bits);
        let view = HudView {
            hud: &hud,
            world: &world,
        };
        let snap = GameSnapshot::from_view(&view);
        assert_eq!(view.aetheria_slots(), snap.aetheria_slots());
        era_context(&view, |c| {
            // The sigils are in the accessories flyout, opened from its button.
            if bits == 0 {
                panel.event(ControlEvent::Activate("accessories-button".into()), c);
            }
            let f = panel.frame(c);
            let slots: Vec<_> = f
                .controls
                .iter()
                .filter(|k| k.drop_location.is_some_and(|m| m & 0x70000000 != 0))
                .collect();
            assert_eq!(slots.len(), (bits & 7).count_ones() as usize);
            for slot in slots {
                let mask = slot.drop_location.unwrap();
                assert_ne!(bits as u32 & (mask >> 28), 0);
                assert_eq!(
                    panel.event(
                        ControlEvent::Drop {
                            id: slot.id.clone(),
                            payload: DragPayload::Object(ObjectId(3)),
                            slot: 0
                        },
                        c
                    ),
                    vec![PanelAction::Game(UiRequest::DragDrop {
                        item: ObjectId(3),
                        target: DropTarget::EquipLocation { mask, side: 0 }
                    })]
                );
            }
        });
    }
    hud.era.era = dereth_primitives::EraId::Infiltration;
    world
        .tables
        .weenies
        .get_mut(player)
        .unwrap()
        .qualities
        .as_mut()
        .unwrap()
        .ints
        .as_mut()
        .unwrap()
        .insert(0x142, 7);
    era_context(
        &HudView {
            hud: &hud,
            world: &world,
        },
        |c| {
            assert!(!panel
                .frame(c)
                .controls
                .iter()
                .any(|k| k.drop_location.is_some_and(|m| m & 0x70000000 != 0)))
        },
    );
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn classic_magic_filters_gate_inputs_independently_of_skill_loading() {
    use dereth_client_contract::{era::EraUiFacts, GameSnapshot};
    let mut panel = dereth_classic_ui::panels::game::make("spellbook").unwrap();
    for (void, eight) in [(false, true), (true, false), (false, false), (true, true)] {
        let view = GameSnapshot {
            resolved_era_ui: Some(EraUiFacts {
                void_magic: void,
                spell_level_eight: eight,
                spell_favorite_tabs: 7,
            }),
            ..Default::default()
        };
        era_context(&view, |c| {
            let f = panel.frame(c);
            for (bit, shown) in [(0x2000, void), (0x800, eight)] {
                let id = format!("filter:{bit}");
                assert_eq!(f.controls.iter().any(|k| k.id == id), shown);
                let out = panel.event(ControlEvent::Check { id, checked: true }, c);
                assert_eq!(!out.is_empty(), shown);
            }
        });
    }
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn classic_favorites_hide_an_unavailable_current_bank_before_the_next_input() {
    use dereth_client_contract::{EraView, GameView, SpellEntry};
    #[derive(Debug)]
    struct View {
        era: EraView,
        spells: Vec<SpellEntry>,
    }
    impl GameView for View {
        fn era(&self) -> Option<&EraView> {
            Some(&self.era)
        }
        fn spell_tab(&self, tab: usize) -> &[u32] {
            if tab == 7 {
                &[777]
            } else if tab == 0 {
                &[111]
            } else {
                &[]
            }
        }
        fn spellbook(&self) -> &[SpellEntry] {
            &self.spells
        }
        fn spell(&self, id: u32) -> Option<SpellEntry> {
            self.spells.iter().find(|spell| spell.id == id).cloned()
        }
    }
    let mut view = View {
        era: EraView::default(),
        spells: [(111, "VisibleBank"), (777, "HiddenBank")]
            .into_iter()
            .map(|(id, name)| SpellEntry {
                id,
                name: name.into(),
                icon: None,
                school: 1,
                level: 1,
                icon_power: 1,
                display_order: 0,
                bitfield: 0,
            })
            .collect(),
    };
    let mut panel = dereth_classic_ui::panels::game::make("spell-favorites").unwrap();
    era_context(&view, |c| {
        panel.event(ControlEvent::Activate("spell:0".into()), c);
        panel.event(ControlEvent::Activate("tab:7".into()), c);
        panel.event(ControlEvent::Activate("spell:0".into()), c);
        assert!(format!("{:?}", panel.frame(c)).contains("HiddenBank"));
    });
    view.era.era = dereth_primitives::EraId::Infiltration;
    era_context(&view, |c| {
        let f = panel.frame(c);
        assert!(!f.controls.iter().any(|k| k.id == "tab:7"));
        let text = format!("{f:?}");
        assert!(text.contains("VisibleBank"));
        assert!(!text.contains("HiddenBank"));
    });
    assert_eq!(view.spell_tab(7), &[777]);
}
