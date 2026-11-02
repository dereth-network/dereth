//! The world the client keeps its objects in: its region, the objects' upkeep, the creations under
//! way, and what the client asks of the scene for its objects.

use auberean_client_net::LocalTime;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::world::{release_obj_cell, ClientWorld, PhysicsObjOrder, WorldScene};
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::login::{DataRefused, Empty};
use auberean_protocol::physics::{CreateObject, Fx, ObjectEvent, ObjectOnly, Target};
use auberean_protocol::types::{ObjectStamp, ParentLink, PhysicsDesc, VisualDesc, WeenieDesc};
use auberean_protocol::{ClientMessage, ServerMessage};

use super::download::file;

const PORTAL: u64 = 0x1_0000_0000;
const CELL: u64 = 0x2_0000_0001;
const LOCAL: u64 = 0x3_8000_0001;
/// A blob on the login connection, taken as it comes.
const LOGIN: u64 = 0x2300_0000_0000_0000;
/// Messages that take effect as they come, in order.
const NOW: u64 = 3 << 56;
/// A visual message.
const VISUAL: u64 = 2 << 56;

fn client(files: bool) -> Client {
    let files = if files {
        vec![file(PORTAL, &[]), file(CELL, &[]), file(LOCAL, &[])]
    } else {
        Vec::new()
    };
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files,
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

fn create(id: u64, instance: u16) -> ServerMessage {
    ServerMessage::CreateObject(CreateObject {
        id,
        physics: PhysicsDesc {
            instance_stamp: instance,
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    })
}

fn forget(id: u64, instance: u16) -> ServerMessage {
    ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id,
        instance_stamp: instance,
        other_stamp: 0,
    }))
}

fn fx(id: u64, instance: u16, visual: u16) -> ServerMessage {
    ServerMessage::DoFx(ObjectEvent {
        sender: ObjectStamp {
            id,
            instance_stamp: instance,
            other_stamp: visual,
        },
        action: Fx { fx: 1, scalar: 1.0 },
    })
}

fn force_creates(c: &Client) -> Vec<u64> {
    c.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::ForceCreate(Target(id)) => Some(*id),
            _ => None,
        })
        .collect()
}

/// What the scene was asked, and what it answers.
#[derive(Default)]
struct Scene {
    made: Vec<PhysicsObjOrder>,
    statics: Vec<u64>,
    parented: Vec<u64>,
    left: Vec<u64>,
    refreshed: u32,
}

impl WorldScene for Scene {
    fn make_physics_obj(&mut self, order: &PhysicsObjOrder) -> bool {
        self.made.push(order.clone());
        true
    }
    fn is_static(&self, id: u64) -> bool {
        self.statics.contains(&id)
    }
    fn has_parent(&self, id: u64) -> bool {
        self.parented.contains(&id)
    }
    fn leave_visibility(&mut self, id: u64) {
        self.left.push(id);
    }
    fn refresh_landscape(&mut self) -> bool {
        self.refreshed += 1;
        true
    }
}

/// A region's number puts the world in the region whose record the region mapping names: the
/// scene is asked once to take that record up, the same number again changes nothing, a number
/// with no record leaves the world where it was, and 0 lets the region go. The client without data
/// finds no region for the player's creation's number.
///
/// Behaviour: model.world.a-regions-number-puts-the-world-in-its-region
#[test]
fn a_regions_number_puts_the_world_in_its_region_once() {
    let mut w = ClientWorld::new();
    assert_eq!(w.region(), 0);
    assert!(w.set_region(1, |n| (n == 1).then_some((0x0F00_0000, 1))));
    assert_eq!(w.region(), 1);
    assert_eq!(w.take_region_change(), Some(0x0F00_0000));
    assert!(w.set_region(1, |_| panic!("the current region is not looked up")));
    assert_eq!(w.take_region_change(), None);
    assert!(!w.set_region(2, |_| None));
    assert_eq!((w.region(), w.take_region_change()), (1, None));
    assert!(w.set_region(0, |_| None));
    assert_eq!((w.region(), w.take_region_change()), (0, Some(0)));

    let mut c = client(false);
    c.take_message(
        NOW,
        ServerMessage::PlayerObject(auberean_protocol::login::PlayerObject {
            id: 0x2130_0000_0000_0001,
            region: 1,
        }),
    );
    assert_eq!(c.world.region(), 0);
    assert_eq!(c.world.take_region_change(), None);
}

/// The scene is asked for a game object's physics object with the physics record its qualities
/// name, its weenie and visual descriptions; an object the client does not have makes none. The
/// world's resources made again are the landscape's.
///
/// Behaviour: model.world.a-game-objects-physics-object-is-made-from-its-descriptions
#[test]
fn the_scene_makes_a_game_objects_physics_object_from_its_records_and_descriptions() {
    let mut c = client(false);
    let id = 0x8130_0000_1000_0001;
    let mut made = CreateObject {
        id,
        visual: VisualDesc {
            did: 0x1000_0042,
            ..VisualDesc::default()
        },
        weenie: WeenieDesc {
            mask: WeenieDesc::PACKAGE,
            package_id: 0x0000_0298,
            ..WeenieDesc::default()
        },
        ..CreateObject::default()
    };
    made.physics.instance_stamp = 1;
    c.take_message(NOW, ServerMessage::CreateObject(made.clone()));
    c.model
        .object_mut(id)
        .expect("made")
        .qualities
        .data_ids
        .insert(1, 0x3400_0007);
    let mut s = Scene::default();
    assert!(c.create_physics_obj(id, &mut s));
    assert!(!c.create_physics_obj(id + 1, &mut s));
    assert_eq!(s.made.len(), 1);
    let o = &s.made[0];
    assert_eq!((o.id, o.physics_record), (id, 0x3400_0007));
    assert_eq!(o.visual, made.visual);
    assert_eq!(o.weenie.package_id, 0x0000_0298);
    assert!(c.world.refresh(&mut s));
    assert_eq!(s.refreshed, 1);
}

/// A cell let go of loses the objects standing in it whose physics objects move and hang from
/// nothing: a static one stays, a game object hanging from another stays, a stand-in the scene
/// says hangs stays, and objects in other cells are not touched.
///
/// Behaviour: model.world.a-cell-let-go-of-loses-its-moving-loose-objects
#[test]
fn a_cell_let_go_of_loses_the_objects_in_it_that_move_and_hang_from_nothing() {
    let mut c = client(false);
    let cell = 0x7D64_0021;
    let [moving, still, held, elsewhere] = [1u64, 2, 3, 4].map(|n| 0x8130_0000_1000_0000 | n);
    let stand_in = 0x1200_0000_0048_0001;
    for id in [moving, still, elsewhere] {
        c.take_message(NOW, create(id, 1));
    }
    let mut h = CreateObject {
        id: held,
        ..CreateObject::default()
    };
    h.physics.instance_stamp = 1;
    h.physics.parent = ParentLink {
        parent: moving,
        location: 1,
        stamp: 1,
    };
    c.take_message(NOW, ServerMessage::CreateObject(h));
    assert!(c.queue.create_dummy_object(stand_in));
    for id in [moving, still, held, stand_in] {
        c.queue.change_cell(id, Some(cell));
    }
    c.queue.change_cell(elsewhere, Some(cell + 1));
    let mut s = Scene {
        statics: vec![still],
        parented: vec![stand_in],
        ..Scene::default()
    };
    let left = release_obj_cell(&c.queue, &c.model, cell, &mut s);
    assert_eq!(left, vec![moving]);
    assert_eq!(s.left, vec![moving]);
    s.parented.clear();
    assert_eq!(
        release_obj_cell(&c.queue, &c.model, cell, &mut s),
        vec![stand_in, moving]
    );
}

/// The objects' upkeep does not run while the scene waits for the cells around the player: an
/// object the server had the client forget stays until it stops waiting.
///
/// Behaviour: model.world.the-objects-upkeep-waits-for-the-cells
#[test]
fn the_objects_upkeep_waits_while_the_scene_waits_for_the_cells() {
    let mut c = client(false);
    let id = 0x8130_0000_1000_0001;
    c.take_message(NOW, create(id, 1));
    c.take_message(NOW, forget(id, 1));
    c.world.blocking_for_cells = true;
    c.upkeep();
    assert!(c.model.object(id).is_some());
    c.world.blocking_for_cells = false;
    c.upkeep();
    assert!(c.model.object(id).is_none());
}

/// An object is being made until its creation's records have all finished and the objects'
/// upkeep has made it (an upkeep that waits for the cells makes nothing): a queue waiting for it
/// is not given up meanwhile, and is once it is made and has waited 25 seconds. Leaving the world
/// gives up the creations under way.
///
/// Behaviour: model.world.an-object-is-being-made-until-the-upkeep-makes-it
#[test]
fn an_object_is_being_made_until_its_records_finish_and_the_upkeep_makes_it() {
    let mut c = client(true);
    let id = 0x8130_0000_1000_0001;
    let entity = |id: u64| {
        let mut m = CreateObject {
            id,
            weenie: WeenieDesc {
                mask: WeenieDesc::ENTITY,
                entity_did: 0x4700_0005,
                ..WeenieDesc::default()
            },
            ..CreateObject::default()
        };
        m.physics.instance_stamp = 1;
        ServerMessage::CreateObject(m)
    };
    c.take_message(NOW, entity(id));
    assert!(c.data_download().is_object_being_created(id));
    // About a newer incarnation: it waits.
    c.take_message(VISUAL, fx(id, 2, 1));
    assert_eq!(c.queue.waiting_objects(), 1);
    c.set_time(LocalTime(30.0));
    c.upkeep();
    assert!(
        force_creates(&c).is_empty(),
        "not given up while being made"
    );
    assert_eq!(c.queue.waiting_objects(), 1);
    // Every record is refused: the creation is done, and made at the next upkeep.
    let asked: Vec<(u32, u32)> = c
        .sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) => Some((r.kind, r.id)),
            _ => None,
        })
        .collect();
    assert!(!asked.is_empty());
    for (n, (kind, rid)) in asked.iter().enumerate() {
        c.take_message(
            LOGIN | (n as u64 + 1),
            ServerMessage::DataRefused(DataRefused {
                kind: *kind,
                id: *rid,
                error: 1,
            }),
        );
    }
    assert!(
        c.data_download().is_object_being_created(id),
        "until the upkeep makes it"
    );
    // The objects' upkeep waits while the scene waits for the cells: still being made.
    c.world.blocking_for_cells = true;
    c.upkeep();
    assert!(c.data_download().is_object_being_created(id));
    c.world.blocking_for_cells = false;
    c.upkeep();
    assert!(!c.data_download().is_object_being_created(id));
    assert_eq!(force_creates(&c), vec![id], "given up, 30 seconds waiting");

    let other = id + 1;
    c.take_message(NOW, entity(other));
    assert!(c.data_download().is_object_being_created(other));
    c.take_message(NOW, ServerMessage::CharExitGame(Empty));
    assert!(!c.data_download().is_object_being_created(other));
}
