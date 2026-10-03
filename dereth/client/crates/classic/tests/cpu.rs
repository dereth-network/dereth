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
