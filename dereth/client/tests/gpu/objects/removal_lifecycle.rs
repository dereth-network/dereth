//! Removing an object retires every owner the App holds for it: the model, the stream's presence,
//! the collision body, the render body, opened-corpse state, the selection and the toolbar name.
//! Deletion (`Item_DeleteObject 0xF747`) and due destruction are immediate and allow a recreate
//! under the same id; a pickup (`0xF74A`) only leaves the world; a `0x0024` UI remove refreshes the
//! inventory list.
//!
//! Fixture: long-solo-play's corpse create, destroy effect and delete fed through a socket-free
//! replay endpoint, and its first `0x0024` event; constructed inventory and descriptor state in a
//! headless `App` with the shipped UI and geometry.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
use dereth_client::world::SceneReads;
use dereth_client::{app::App, config::Config, net::ClientNetwork, world::SceneConfig};
use dereth_client_model::{objects::ObjectInventory, Weenie};
use dereth_client_net::client_session::{
    testing::{Corpus, CorpusBlob, Direction},
    SessionEvent,
};
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::{
    objects::{InventoryPickupEvent, ItemCreateObject, ItemDeleteObject, ObjectCreatePayload},
    types::{PhysicsDesc, PhysicsEventStamp, PublicWeenieDesc},
    Message, Opcode,
};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

fn app(scene: bool) -> App {
    app_with_scene(scene.then_some(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..Default::default()
    }))
}

fn app_with_scene(scene: Option<SceneConfig>) -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dere-removal-lifecycle-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    if let Some(scene) = scene {
        app.load_static_scene(scene).unwrap();
    }
    assert!(app.frame());
    app
}

struct RecordedPeer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl RecordedPeer {
    fn attach(app: &mut App) -> Self {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "removal-lifecycle", "unused", 0).unwrap();
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        app.attach_replay_network(net)
            .expect("socket-free replay endpoint");
        Self {
            crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
            sequence: 1,
            blob: 0,
        }
    }

    fn send(&mut self, app: &mut App, row: &CorpusBlob) {
        assert_eq!(row.dir, Direction::ServerToClient);
        assert_eq!(row.queue, dereth_primitives::NetQueue::WorldObjects);
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: 10,
                },
                row.payload.clone(),
            ))
            .unwrap();
        let raw = packet.serialize(Some(self.crypto.next())).unwrap();
        app.replay_network_mut().unwrap().feed(
            &raw,
            "127.0.0.1:19000".parse().unwrap(),
            LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
        );
    }
}

/// Behaviour: objects.removal.a-delete-retires-model-presence-render-body-and-selection
#[test]
fn recorded_corpse_destroy_then_delete_retires_every_app_owner() {
    const CORPSE: ObjectId = ObjectId(0x8000_0A3C);
    let corpus = Corpus::load("long-solo-play")
        .unwrap()
        .expect("long-solo-play fixture");
    let row = |idx| {
        corpus
            .blobs
            .iter()
            .find(|r| r.idx == idx)
            .expect("recorded row")
    };
    let create = row(862);
    let destroy = row(1099);
    let delete = row(1114);
    assert_eq!(
        (create.opcode, destroy.opcode, delete.opcode),
        (0xF745, 0xF755, 0xF747)
    );
    assert_eq!(
        destroy.payload.as_slice(),
        &[0x55, 0xF7, 0, 0, 0x3C, 0x0A, 0, 0x80, 0x59, 0, 0, 0, 0, 0, 0x80, 0x3F],
        "the effect is the destroy script type (89), intensity 1"
    );
    assert_eq!(
        delete.payload.as_slice(),
        &[0x47, 0xF7, 0, 0, 0x3C, 0x0A, 0, 0x80, 0, 0, 0, 0],
        "the recorded delete names instance 0"
    );
    assert_eq!(delete.t_rel_micros - destroy.t_rel_micros, 1_016_608);

    let payload = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&create.payload[4..]))
        .unwrap()
        .0;
    assert_eq!(payload.id, CORPSE);
    assert_eq!(payload.physicsdesc.timestamps.instance, 0);
    assert_ne!(
        payload.wdesc.bitfield & dereth_client_model::weenie::bitfield::CORPSE,
        0
    );
    let landblock = u16::try_from(
        payload
            .physicsdesc
            .position
            .as_ref()
            .expect("world corpse position")
            .objcell_id
            >> 16,
    )
    .unwrap();
    let mut app = app_with_scene(Some(SceneConfig {
        landblock,
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..Default::default()
    }));
    let mut peer = RecordedPeer::attach(&mut app);

    peer.send(&mut app, create);
    assert!(app.frame());
    assert!(
        app.objects().world.weenie(CORPSE).is_some(),
        "the App model accepted the recorded 0xF745"
    );
    assert!(
        app.objects().presence(CORPSE).is_some(),
        "stream owns recorded instance 0"
    );
    assert!(
        app.objects().physics.handle(CORPSE).is_some(),
        "real collision body exists"
    );
    assert!(
        app.world_state()
            .unwrap()
            .server_object_frame(CORPSE)
            .is_some(),
        "real rendered body exists"
    );
    app.objects_mut().world.opened_corpses.insert(CORPSE);
    app.objects_mut().world.set_selected_object(
        Some(CORPSE),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    let selected_name = app.objects().world.weenie(CORPSE).unwrap().pwd.name.clone();

    peer.send(&mut app, destroy);
    assert!(app.frame());
    assert_eq!(
        app.objects().stats.script_type_events,
        1,
        "the recorded Destroy reached 0xF755"
    );
    assert!(
        app.objects().world.weenie(CORPSE).is_some(),
        "Destroy is an effect, not deletion"
    );
    assert!(app
        .world_state()
        .unwrap()
        .server_object_frame(CORPSE)
        .is_some());
    assert!(
        app.objects().world.weenie(CORPSE).unwrap().selected,
        "the selected-ring source is live"
    );
    assert_eq!(
        toolbar_selection_name(&mut app),
        selected_name,
        "the shipped toolbar still names it"
    );

    peer.send(&mut app, delete);
    assert!(app.frame());
    assert_eq!(
        app.objects().stats.removes,
        1,
        "the recorded instance-0 0xF747 was accepted"
    );
    assert!(
        app.objects().world.weenie(CORPSE).is_none(),
        "model retired"
    );
    assert!(app.objects().presence(CORPSE).is_none(), "presence retired");
    assert!(
        app.objects().physics.handle(CORPSE).is_none(),
        "collision body retired"
    );
    assert!(
        app.world_state()
            .unwrap()
            .server_object_frame(CORPSE)
            .is_none(),
        "render body retired"
    );
    assert!(
        !app.objects().world.opened_corpses.contains(&CORPSE),
        "opened-corpse state retired"
    );
    assert_eq!(app.objects().world.selected, None, "selection retired");
    assert_eq!(
        toolbar_selection_name(&mut app),
        "",
        "the shipped toolbar consumed the clear"
    );
    assert!(
        !app.replay_network_mut()
            .unwrap()
            .session
            .instances_mut()
            .knows(CORPSE),
        "session instance gate retired after the accepted deletion"
    );
}
fn screen(app: &App) -> &GamePlayScreen {
    let any: &dyn std::any::Any = app.ui().unwrap().flow.current().unwrap();
    any.downcast_ref().unwrap()
}
fn toolbar_selection_name(app: &mut App) -> String {
    let name = screen(app)
        .toolbar_children
        .get("sel_object_name")
        .expect("the shipped toolbar has its selected-object name field");
    app.ui_mut()
        .unwrap()
        .ui
        .text_element_mut(name)
        .expect("the selected-object name field is a text element")
        .glyphs
        .inq_text(false)
}
fn create(app: &mut App, id: ObjectId) {
    create_instance(app, id, 0, false);
}
fn recorded_animated_descriptor() -> ObjectCreatePayload {
    let corpus = Corpus::load("long-solo-play").unwrap().unwrap();
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == 0xF745
                && r.payload.get(4..8) == Some(&0x800009D2_u32.to_le_bytes())
        })
        .expect("recorded remote mover create");
    let p = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .unwrap()
        .0;
    assert!(p.physicsdesc.mtable_id.is_some_and(|id| id != 0));
    assert!(p.physicsdesc.setup_id.is_some());
    p
}
#[test]
fn recorded_motion_station_has_explicit_setup_and_table() {
    let _ = recorded_animated_descriptor();
}

fn create_instance(app: &mut App, id: ObjectId, instance: u16, animated: bool) {
    let scene = app.world_scene().unwrap();
    let block = scene.viewer_block().unwrap();
    let origin = dereth_physics::math::localtoglobal(
        &scene.camera.frame(),
        dereth_primitives::Vec3::new(0.0, 10.0, -1.0),
    );
    let mut payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(0x02000001),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: (u32::try_from(block.0).unwrap() << 24)
                    | (u32::try_from(block.1).unwrap() << 16)
                    | 1,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: dereth_primitives::Quat::IDENTITY.into(),
                },
            }),
            ..Default::default()
        },
        wdesc: PublicWeenieDesc {
            obj_type: dereth_client_model::weenie::item_type::CONTAINER,
            bitfield: dereth_client_model::weenie::bitfield::CORPSE,
            ..Default::default()
        },
    };
    payload.physicsdesc.timestamps.instance = instance;
    if animated {
        // Recorded setup/table/appearance combination; id, position and lifecycle are constructed.
        // The Aluvian setup's default table is 0, so it cannot stand in for an animated object.
        let recorded = recorded_animated_descriptor();
        payload.objdesc = recorded.objdesc;
        payload.physicsdesc.setup_id = recorded.physicsdesc.setup_id;
        payload.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::MTABLE;
        payload.physicsdesc.mtable_id = recorded.physicsdesc.mtable_id;
    }
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(payload)).unwrap(),
        },
        LocalTime(0.0),
    );
}

/// Behaviour: objects.deletion.the-shards-remove-detaches-the-item-and-defers-the-rest
#[test]
fn recorded_ui_remove_refreshes_real_inventory_list_and_surviving_item_remains() {
    let raw = Corpus::load("long-solo-play")
        .unwrap()
        .unwrap()
        .blobs
        .into_iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == 0x24)
        .unwrap()
        .payload;
    let id = ObjectId(u32::from_le_bytes(raw[4..8].try_into().unwrap()));
    let survivor = ObjectId(0x8000_7777);
    let player = ObjectId(0x5000_0017);
    let mut app = app(false);
    app.apply_hud_events(&[SessionEvent::PlayerDescription(Box::default())]);
    app.objects_mut().world.set_player(player);
    for object in [player, id, survivor] {
        let mut w = Weenie::new(object);
        w.valid = true;
        w.pwd.name = "Removal station".into();
        w.pwd.items_capacity = Some(24);
        w.pwd.container_id = Some(if object == player {
            ObjectId(0)
        } else {
            player
        });
        app.objects_mut().world.tables.weenies.insert(object, w);
    }
    app.objects_mut().world.tables.inventories.insert(
        player,
        ObjectInventory {
            container: player,
            items: vec![id, survivor],
            ..Default::default()
        },
    );
    assert!(app.frame());
    let list = screen(&app).inventory.item_list.as_ref().unwrap();
    assert_eq!(
        list.slots.iter().filter_map(|s| s.item).collect::<Vec<_>>(),
        vec![id, survivor],
        "actual populated DAT ItemList"
    );
    app.apply_interaction_events(&[SessionEvent::UiEvent {
        opcode: Opcode(0x24),
        blob: raw,
    }]);
    assert!(app.frame());
    assert_eq!(
        screen(&app)
            .inventory
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .filter_map(|s| s.item)
            .collect::<Vec<_>>(),
        vec![survivor],
        "real production refill"
    );
}

/// Behaviour: objects.removal.a-delete-retires-model-presence-render-body-and-selection
#[test]
fn a_delete_immediately_removes_model_presence_and_the_render_body_then_allows_recreate() {
    let mut app = app(true);
    let id = ObjectId(0x83007777);
    create(&mut app, id);
    assert!(app.frame());
    assert!(app.world_state().unwrap().server_object_frame(id).is_some());
    app.objects_mut().world.opened_corpses.insert(id);
    let delete = ItemDeleteObject {
        id,
        instance_sequence: 0,
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemDeleteObject::OPCODE,
            body: dereth_protocol::write_body(&delete).unwrap(),
        },
        LocalTime(100.0),
    );
    assert!(
        app.objects().world.weenie(id).is_none(),
        "0xF747 removes the model immediately, not through a deferred UI path"
    );
    assert!(app.frame());
    assert!(app.world_state().unwrap().server_object_frame(id).is_none());
    assert!(app.objects().presence(id).is_none());
    assert!(!app.objects().world.opened_corpses.contains(&id));
    create(&mut app, id);
    assert!(app.frame());
    assert!(app.world_state().unwrap().server_object_frame(id).is_some());
}

/// A pickup (`Inventory_PickupEvent 0xF74A`) makes a loose object disappear but is not a
/// deletion: it updates `POSITION_TS`, unparents the object and removes it from the physical world,
/// without clearing the selection. The model, the instance and the toolbar name survive for a later
/// parent event.
#[test]
fn a_pickup_leaves_the_selected_object_and_toolbar_identity_alive() {
    let mut app = app(true);
    let id = ObjectId(0x8300_777A);
    create(&mut app, id);
    assert!(app.frame());
    app.objects_mut()
        .world
        .tables
        .weenies
        .get_mut(id)
        .unwrap()
        .pwd
        .name = "Selected pickup".into();
    app.objects_mut().world.set_selected_object(
        Some(id),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    assert!(app.frame());
    assert_eq!(toolbar_selection_name(&mut app), "Selected pickup");

    let pickup = InventoryPickupEvent {
        id,
        timestamps: PhysicsEventStamp {
            instance: 0,
            event: 1,
        },
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: InventoryPickupEvent::OPCODE,
            body: dereth_protocol::write_body(&pickup).unwrap(),
        },
        LocalTime(100.0),
    );
    assert!(app.frame());
    assert!(
        app.objects().world.weenie(id).is_some(),
        "0xF74A does not delete the model"
    );
    assert!(
        app.objects().presence(id).is_some(),
        "0xF74A retains the instance for a later parent event"
    );
    assert!(
        app.world_state().unwrap().server_object_frame(id).is_none(),
        "leaving the world retires the draw"
    );
    assert_eq!(
        app.objects().world.selected,
        Some(id),
        "0xF74A does not remove the object, so the selection stays"
    );
    assert_eq!(toolbar_selection_name(&mut app), "Selected pickup");
}

/// Behaviour: objects.removal.a-delete-retires-model-presence-render-body-and-selection
#[test]
fn no_packet_app_frame_runs_final_destruction_and_releases_presence_render_and_selection() {
    let mut app = app(true);
    let id = ObjectId(0x83007778);
    create(&mut app, id);
    assert!(app.frame());
    app.objects_mut().world.set_selected_object(
        Some(id),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    let now = app.interaction().last_use_time.0;
    // Constructed already-due timer, actual next-frame maintenance: no manual use_time call.
    app.objects_mut()
        .world
        .schedule_destroy(id, ServerTime(now - 25.0));
    assert!(app.frame());
    assert!(
        app.objects().world.weenie(id).is_none(),
        "no-link/no-packet frame still maintains objects"
    );
    assert_eq!(app.objects().world.selected, None);
    assert!(app.objects().presence(id).is_none());
    assert!(app.world_state().unwrap().server_object_frame(id).is_none());
}

#[test]
fn same_id_replacement_retires_exact_old_physical_and_motion_owners_before_next_operation() {
    let mut app = app(true);
    let id = ObjectId(0x83007779);
    create_instance(&mut app, id, 1, true);
    assert!(app.frame());
    let old = app
        .objects()
        .physics
        .handle(id)
        .expect("real collision body");
    let weak = app
        .world_state()
        .unwrap()
        .server_object_motion_lifetime(id)
        .unwrap();
    assert!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .world
            .get(old)
            .unwrap()
            .motion
            .is_some(),
        "both renderer and physical SharedMotion own the old driver"
    );
    let delete = ItemDeleteObject {
        id,
        instance_sequence: 1,
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemDeleteObject::OPCODE,
            body: dereth_protocol::write_body(&delete).unwrap(),
        },
        LocalTime(100.0),
    );
    create_instance(&mut app, id, 2, true); // no intervening renderer/physics sync
    assert!(app.frame());
    let new = app.objects().physics.handle(id).unwrap();
    assert_ne!(old, new);
    let world = &app.world_state().unwrap().character.as_ref().unwrap().world;
    assert!(world.get(old).is_none());
    assert!(world.get(new).unwrap().motion.is_some());
    assert!(
        weak.upgrade().is_none(),
        "neither the removed renderer nor old physical callback retains the old driver"
    );
    let fresh = app
        .world_state()
        .unwrap()
        .server_object_motion_lifetime(id)
        .unwrap();
    assert!(fresh.upgrade().is_some());
    // A genuinely later operation still affects the replacement, not a stale handle.
    let state = dereth_protocol::objects::ItemSetState {
        id,
        state: 0x4000,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 2,
            event: 1,
        },
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::objects::ItemSetState::OPCODE,
            body: dereth_protocol::write_body(&state).unwrap(),
        },
        LocalTime(101.0),
    );
    assert!(app.frame());
    assert_eq!(app.objects().physics_state(id), Some(0x4000));
    assert!(fresh.upgrade().is_some());
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemDeleteObject::OPCODE,
            body: dereth_protocol::write_body(&ItemDeleteObject {
                id,
                instance_sequence: 2,
            })
            .unwrap(),
        },
        LocalTime(102.0),
    );
    assert!(app.frame());
    assert!(fresh.upgrade().is_none());
    assert_eq!(
        app.world_scene().unwrap().live_appearances(),
        0,
        "last rendered appearance owner releases cache entry"
    );
}

#[test]
fn pending_cell_load_blocks_maintenance_but_absent_ui_or_link_does_not() {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dere-removal-lifecycle-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    let id = ObjectId(0x83007780);
    let mut w = Weenie::new(id);
    w.valid = true;
    app.objects_mut().world.tables.weenies.insert(id, w);
    app.objects_mut()
        .world
        .schedule_destroy(id, ServerTime(-100.0));
    app.defer_static_scene(SceneConfig::default()); // no player position: synchronous loader is blocked
    assert!(app.frame());
    assert!(app.objects().world.weenie(id).is_some());
    drop(app);
    // A separate unblocked App avoids any test-only setter for the owner's pending load.
    let mut unblocked = App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dere-removal-lifecycle-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    let mut w = Weenie::new(id);
    w.valid = true;
    unblocked.objects_mut().world.tables.weenies.insert(id, w);
    unblocked
        .objects_mut()
        .world
        .schedule_destroy(id, ServerTime(-100.0));
    assert!(unblocked.frame());
    assert!(unblocked.objects().world.weenie(id).is_none());
    assert!(unblocked.ui().is_none());
}
