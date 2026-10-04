//! A creature the server says is wielding a weapon is drawn holding it, at the placement the
//! server named, through the real `WorldScene` on a real device: the weapon follows the hand, a
//! parent event moves a drawn object onto its holder, a missing holding location holds nothing,
//! the player's own weapon hangs off the local body, and a put-away weapon stops being drawn.
//!
//! Fixture: the retail dats, with anchors chosen from the packet corpus. Every one of the corpus's
//! 37 `Item_ParentEvent 0xF749`s is a human setup (`0x02000001`, 36 of them, and `0x0200004E`
//! once) holding an item at `ParentLocation::RightHand (1)` with `Placement::RightHandCombat (1)`,
//! or at `LeftHand (2)`/`LeftHand (3)`; the human setup carries a `RightHand` holding location on
//! part **15** at an offset of (0.0250, 0.0000, -0.0760). The dat held-items tests are the
//! independent oracle for those numbers; expected frames here use
//! `dereth_animation::frame::combine` on frames read from the dat, in the client's child-update
//! composition order.
//!
//! `WorldScene::sync_objects` builds the part meshes and needs a device; the helper requests
//! software rendering. Missing dats and a missing device fail the test. The local-player case
//! alone returns early if its setup has no right-hand holding location. Six tests inspect model frames;
//! one compares pixels.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::{retail_store, test_gpu};
use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_client::models::{child_frame, placement_frames};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemParentEvent, ObjectCreatePayload};
use dereth_protocol::types::{
    physicsdesc::flags, ObjDesc, PhysicsDesc, PositionWire, PublicWeenieDesc,
};
use dereth_protocol::types::{PhysicsEventStamp, PhysicsTimestamps};
use dereth_protocol::{write_body, Opcode};
use dereth_render::device::Gpu;

/// The human setup 36 of the corpus's 37 parent events name as the holder.
const HOLDER_SETUP: u32 = 0x0200_0001;
/// One of the three item setups the corpus wields: a single-part weapon carrying placement keys
/// `0, 1, 2, 3, 101, 103, 104`, so `RightHandCombat (1)` and `Resting (101)` both exist and differ.
const ITEM_SETUP: u32 = 0x0200_1713;
/// `ParentLocation::RightHand` and `Placement::RightHandCombat`, the pair 34 of the 37 carry.
const RIGHT_HAND: u32 = 1;
const RIGHT_HAND_COMBAT: u32 = 1;

/// The land cell the holder stands in, **outdoors, on the block this bench flies its camera over**.
///
/// It must be outdoors: interior objects are submitted only for cells in the reached-cell draw
/// list, and the pixel bench draws outdoors over `0x8602` without an attached local body or an
/// interior portal walk, so an object in an interior cell would be absent from both pixel arms.
///
/// [`outdoor`] names the land cell an arbitrary offset actually falls in and normalises the offset
/// into it, which is what the pixel bench needs: it puts the holder wherever the camera looks.
const CELL: u32 = 0x8602_0001;
const LANDBLOCK: u16 = 0x8602;

/// The outdoor land cell of [`LANDBLOCK`] an offset falls in, and the offset normalised into it.
/// This uses the same outside-coordinate adjustment as `Character::teleport`. The pair
/// describes one world point, so `cell_origin + at` still names it after the wrap.
/// See [`CELL`].
fn outdoor(at: Vec3) -> (dereth_primitives::CellId, Vec3) {
    let mut cell = dereth_primitives::CellId(CELL);
    let mut origin = at;
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
    (cell, origin)
}
const HOLDER: u32 = 0x5000_0001;
const ITEM: u32 = 0x5000_0002;

fn setup_of(store: &RetailDatStore, id: u32) -> Setup {
    let did = DataId(id);
    let bytes = store.read_typed(DbType::Setup, did).expect("reads");
    Setup::decode_payload(did, &bytes).expect("decodes")
}

/// A `0xF745` at a real cell, or, with `parent`, one that arrives already attached, which is how
/// 16 of the corpus's creates arrive.
fn create_ev(
    id: u32,
    setup: u32,
    at: Option<Vec3>,
    parent: Option<(u32, u32)>,
    placement: Option<u32>,
) -> SessionEvent {
    let mut physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP,
        setup_id: Some(setup),
        timestamps: PhysicsTimestamps::default(),
        ..PhysicsDesc::default()
    };
    if let Some(p) = at {
        physicsdesc.bitfield |= flags::POSITION;
        // The land cell this offset falls in, and the offset wrapped into it.
        let (cell, origin) = outdoor(p);
        physicsdesc.position = Some(PositionWire {
            objcell_id: cell.0,
            frame: dereth_protocol::types::Frame {
                origin: dereth_protocol::types::Vec3 {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
                ..dereth_protocol::types::Frame::default()
            },
        });
    }
    if let Some((who, loc)) = parent {
        physicsdesc.bitfield |= flags::PARENT;
        physicsdesc.parent = Some((ObjectId(who), loc));
    }
    if let Some(pid) = placement {
        physicsdesc.bitfield |= flags::ANIMFRAME;
        physicsdesc.animframe_id = Some(pid);
    }
    let body = write_body(&ItemCreateObject(ObjectCreatePayload {
        id: ObjectId(id),
        objdesc: ObjDesc::default(),
        physicsdesc,
        wdesc: PublicWeenieDesc::default(),
    }))
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body,
    }
}

fn parent_ev(creature: u32, item: u32, location: u32, placement: u32, ts: u16) -> SessionEvent {
    let body = write_body(&ItemParentEvent {
        creature: ObjectId(creature),
        item: ObjectId(item),
        location,
        placement_frame: placement,
        timestamps: PhysicsEventStamp {
            instance: 0,
            event: ts,
        },
    })
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_PARENT_EVENT,
        body,
    }
}

/// A `0xF748` moving one object to a new origin, with the outdoor cell adjusted if needed.
fn move_ev(id: u32, at: Vec3, ts: u16) -> SessionEvent {
    use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
    // As [`create_ev`]: the cell the offset falls in, and the wrapped offset.
    let (cell, origin) = outdoor(at);
    let pp = PositionPack {
        origin: dereth_protocol::types::Origin {
            objcell_id: cell.0,
            origin: dereth_protocol::types::Vec3 {
                x: origin.x,
                y: origin.y,
                z: origin.z,
            },
        },
        position_timestamp: ts,
        ..PositionPack::default()
    };
    let body = write_body(&MovementPositionEvent {
        id: ObjectId(id),
        position: pp,
    })
    .expect("enc");
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_POSITION_EVENT,
        body,
    }
}

fn scene_of(store: &Arc<RetailDatStore>, gpu: &mut Gpu, character: bool) -> WorldScene {
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        start_cell: Some(dereth_primitives::CellId(CELL)),
        character,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    WorldScene::load(store, gpu, cfg).expect("the landscape loads")
}

fn step(scene: &mut WorldScene) {
    scene.update(
        dereth_client::camera::CameraInput::default(),
        dereth_client::character::CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z).magnitude()
}

/// Behaviour: objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand
///
/// **The acceptance.** A creature the server says is wielding a weapon is drawn holding it, at the
/// placement the server named.
///
/// Three things are asserted:
///
/// 1. the wielded object, which has a parent and no position, enters the drawable scene state;
/// 2. its frame is `combine(holder.parts[15].pos, holding.frame)`, with the **holder's** frame in
///    it, which is what makes the weapon follow the hand;
/// 3. its parts are posed at `RightHandCombat`, not the setup's initial placement pose.
///
/// This test reads counts and model frames; the pixel test below checks actual drawing.
#[test]
fn a_wielded_weapon_is_drawn_in_the_holders_hand() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let holder_dat = setup_of(&store, HOLDER_SETUP);
    let item_dat = setup_of(&store, ITEM_SETUP);
    let holding = holder_dat.holding_locations[&RIGHT_HAND];
    let combat = placement_frames(&item_dat, RIGHT_HAND_COMBAT)
        .expect("the item carries RightHandCombat")
        .to_vec();
    let resting = placement_frames(&item_dat, dereth_client::models::PLACEMENT_RESTING)
        .expect("and Resting")
        .to_vec();
    assert_ne!(
        combat, resting,
        "the two poses must differ or the third claim proves nothing"
    );

    let mut scene = scene_of(&store, &mut gpu, false);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    stream.apply_event(
        &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            None,
            Some((HOLDER, RIGHT_HAND)),
            Some(RIGHT_HAND_COMBAT),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);

    // (1) Present in the drawable scene state; pixel coverage is checked separately below.
    assert_eq!(
        scene.server_object_count(),
        2,
        "the holder and the item it holds"
    );
    assert_eq!(scene.draw.stats.server_objects_held, 1);
    assert_eq!(scene.draw.stats.held_without_holding_location, 0);

    let holder_parts = scene
        .server_object_part_frames(ObjectId(HOLDER))
        .expect("holder parts");
    let holder_root = scene
        .server_object_frame(ObjectId(HOLDER))
        .expect("holder frame");
    let item_frame = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("item frame");

    // (2) The frame is the holder's own part frame composed with the holding location.
    let want = child_frame(&holder_root, &holder_parts, &to_anim(holding));
    assert_eq!(
        item_frame, want,
        "the held object hangs off the holder's part 15"
    );
    // And that is genuinely far from anywhere the object could have been put without the holder:
    // a hand is over a metre off the feet.
    assert!(
        dist(item_frame.origin, holder_root.origin) > 0.5,
        "the hand must be well away from the holder's own origin, or (2) proves little: {:.3} m",
        dist(item_frame.origin, holder_root.origin)
    );

    // (3) Posed at the placement the server named.
    let item_parts = scene
        .server_object_part_frames(ObjectId(ITEM))
        .expect("item parts");
    assert_eq!(item_parts.len(), item_dat.parts.len());
    for (i, got) in item_parts.iter().enumerate() {
        assert_eq!(
            *got,
            dereth_animation::frame::combine(&item_frame, &combat[i]),
            "part {i} at RightHandCombat"
        );
        assert_ne!(
            *got,
            dereth_animation::frame::combine(&item_frame, &resting[i]),
            "part {i} must not be at Resting"
        );
    }
    eprintln!(
        "holder {HOLDER_SETUP:#010X} at ({:.3}, {:.3}, {:.3}) draws item {ITEM_SETUP:#010X} at \
         ({:.3}, {:.3}, {:.3}) -- {:.3} m off the holder's own origin",
        holder_root.origin.x,
        holder_root.origin.y,
        holder_root.origin.z,
        item_frame.origin.x,
        item_frame.origin.y,
        item_frame.origin.z,
        dist(item_frame.origin, holder_root.origin)
    );
}

/// Behaviour: objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand
///
/// **The weapon follows the hand.** Moving the holder moves the held object by exactly the same
/// vector, within 0.0001 m on each axis, with unchanged rotation: the child update composes the
/// parent's frame.
#[test]
fn moving_the_holder_moves_what_it_holds() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let mut scene = scene_of(&store, &mut gpu, false);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    stream.apply_event(
        &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            None,
            Some((HOLDER, RIGHT_HAND)),
            Some(RIGHT_HAND_COMBAT),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let before = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("item frame");

    let delta = Vec3::new(3.0, -2.0, 0.0);
    stream.apply_event(&move_ev(HOLDER, delta, 5), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let after = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("item frame");

    let moved = Vec3::new(
        after.origin.x - before.origin.x,
        after.origin.y - before.origin.y,
        after.origin.z - before.origin.z,
    );
    assert!((moved.x - delta.x).abs() < 1e-4, "{moved:?}");
    assert!((moved.y - delta.y).abs() < 1e-4, "{moved:?}");
    assert!((moved.z - delta.z).abs() < 1e-4, "{moved:?}");
    assert_eq!(
        after.rotation, before.rotation,
        "the holder did not turn, so nor did the item"
    );
}

/// Behaviour: objects.parent.a-new-parent-event-attaches-the-child-in-wire-order
///
/// **A `0xF749` moves an already-drawn object onto its holder.** The item is on the ground, drawn
/// at its own position; the server says the creature has picked it into its right hand; it is
/// drawn in the hand.
#[test]
fn a_parent_event_moves_an_already_drawn_object_onto_its_holder() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let holder_dat = setup_of(&store, HOLDER_SETUP);
    let holding = holder_dat.holding_locations[&RIGHT_HAND];

    let mut scene = scene_of(&store, &mut gpu, false);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    stream.apply_event(
        &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    // The item on the floor, four metres away.
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            Some(Vec3::new(4.0, 0.0, 0.0)),
            None,
            Some(101),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    assert_eq!(
        scene.draw.stats.server_objects_held, 0,
        "nothing is held yet"
    );
    let on_the_floor = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("item frame");
    let holder_root = scene
        .server_object_frame(ObjectId(HOLDER))
        .expect("holder frame");
    assert!(dist(on_the_floor.origin, holder_root.origin) > 3.5);

    stream.apply_event(
        &parent_ev(HOLDER, ITEM, RIGHT_HAND, RIGHT_HAND_COMBAT, 7),
        LocalTime(0.0),
    );
    assert_eq!(stream.stats.parent_events, 1);
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);

    assert_eq!(scene.server_object_count(), 2, "it is still drawn");
    assert_eq!(scene.draw.stats.server_objects_held, 1);
    let in_hand = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("item frame");
    let holder_parts = scene
        .server_object_part_frames(ObjectId(HOLDER))
        .expect("holder parts");
    assert_eq!(
        in_hand,
        child_frame(&holder_root, &holder_parts, &to_anim(holding))
    );
    assert!(
        dist(in_hand.origin, on_the_floor.origin) > 3.0,
        "the item must actually have left the floor: {:.3} m",
        dist(in_hand.origin, on_the_floor.origin)
    );
    eprintln!(
        "0xF749 moved the item {:.3} m, from the floor to the holder's right hand",
        dist(in_hand.origin, on_the_floor.origin)
    );
}

/// Behaviour: objects.parent.a-holder-holds-only-at-a-place-its-own-body-has
///
/// **A holder whose setup has no entry at the named `ParentLocation` holds nothing.**
///
/// The setup's holding-location lookup fails, so attachment is refused: no parent edge and no
/// drawable item frame, rather than falling back to the holder's root and putting the weapon at
/// its feet. The refusal happens in the model before the renderer's late missing-location counter,
/// which stays zero.
#[test]
fn a_parent_location_the_holder_does_not_carry_holds_nothing() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let holder_dat = setup_of(&store, HOLDER_SETUP);
    // 7 is `ParentLocation::Mouth`, which this setup does not carry (it has 0..6, 8, 9).
    const MOUTH: u32 = 7;
    assert!(!holder_dat.holding_locations.contains_key(&MOUTH));

    let mut scene = scene_of(&store, &mut gpu, false);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    stream.apply_event(
        &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            None,
            Some((HOLDER, MOUTH)),
            Some(RIGHT_HAND_COMBAT),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    assert_eq!(
        stream.presence(ObjectId(ITEM)).unwrap().parent,
        None,
        "attachment is refused at the model owner before installing an edge"
    );
    assert_eq!(
        scene.server_object_count(),
        1,
        "only the holder has a world position"
    );
    assert!(scene.server_object_frame(ObjectId(ITEM)).is_none());
    assert_eq!(
        scene.draw.stats.held_without_holding_location, 0,
        "the refused link no longer reaches a late renderer validation path"
    );
}

/// Behaviour: objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand
///
/// **The pixel differential.** Two scenes through the same software device, with otherwise
/// matching construction and camera inputs, differing in whether the creature wields the weapon.
///
/// Require a nonzero difference covering less than a quarter of the frame. The reported bounding
/// box is derived from those changed pixels; containment in it is a consistency check, not an
/// independent weapon-shaped region or a separate proof of zero change outside such a region.
#[test]
fn wielding_a_weapon_changes_pixels_and_only_where_the_weapon_is() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let viewport = (800u32, 600u32);

    let mut shot = |wielding: bool| -> Option<(Vec<u8>, u32, u32)> {
        use dereth_client::pick::PickScene;
        let mut scene = scene_of(&store, &mut gpu, true);
        let mut stream = ObjectStream::with_store(Arc::clone(&store));

        // Pass 1: the holder at the cell's own origin, to learn where that lands in render space.
        stream.apply_event(
            &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
            LocalTime(0.0),
        );
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("syncs");
        step(&mut scene);
        let cell_origin = scene
            .server_object_frame(ObjectId(HOLDER))
            .expect("a frame")
            .origin;

        let viewer = scene.viewer();
        let ray = dereth_client_runtime::pick_geometry::selection_ray(
            &viewer,
            f32::from(u16::try_from(viewport.0 / 2).unwrap()),
            f32::from(u16::try_from(viewport.1 / 2).unwrap()),
            viewport,
            scene.fov_y_rad(viewport),
        );
        // Four metres down the view axis, and a metre down so the body is in frame rather than
        // its knees.
        let target = Vec3::new(
            viewer.origin.x + ray.x * 4.0,
            viewer.origin.y + ray.y * 4.0,
            viewer.origin.z + ray.z * 4.0 - 1.0,
        );
        let at = Vec3::new(
            target.x - cell_origin.x,
            target.y - cell_origin.y,
            target.z - cell_origin.z,
        );

        // Pass 2: the holder in front of the lens, with or without a weapon in its hand.
        let mut stream = ObjectStream::with_store(Arc::clone(&store));
        stream.apply_event(
            &create_ev(HOLDER, HOLDER_SETUP, Some(at), None, Some(101)),
            LocalTime(0.0),
        );
        if wielding {
            stream.apply_event(
                &create_ev(
                    ITEM,
                    ITEM_SETUP,
                    None,
                    Some((HOLDER, RIGHT_HAND)),
                    Some(RIGHT_HAND_COMBAT),
                ),
                LocalTime(0.0),
            );
        }
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("syncs");
        step(&mut scene);
        scene.reserve_upload_arena(&mut gpu).expect("arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        let image = gpu.capture().expect("capture");
        let (w, h) = (image.width, image.height);
        Some((image.to_rgba(), w, h))
    };

    let (with, w, h) = shot(true).expect("frame with a weapon");
    let (without, w2, h2) = shot(false).expect("frame without one");
    assert_eq!((w, h), (w2, h2));
    assert_eq!(with.len(), without.len());

    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if with[i..i + 4] != without[i..i + 4] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let total = (w * h) as usize;
    eprintln!(
        "wielding {ITEM_SETUP:#010X} at RightHandCombat on {HOLDER_SETUP:#010X}: {n} of {total} \
         pixels differ ({:.2} %), bounding box ({x0},{y0})-({x1},{y1})",
        100.0 * n as f32 / total as f32
    );
    assert!(n > 0, "the weapon drew nothing -- it is not on screen");
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if with[i..i + 4] != without[i..i + 4] {
                assert!(x >= x0 && x <= x1 && y >= y0 && y <= y1);
            }
        }
    }
    assert!(
        n * 4 < total,
        "{n} pixels changed -- a weapon must not repaint the scene"
    );
}

fn to_anim(e: dereth_assets::geometry::LocationEntry) -> dereth_animation::data::LocationEntry {
    dereth_animation::data::LocationEntry {
        part_id: e.part_id,
        frame: e.frame,
    }
}

/// Behaviour: objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand
///
/// **The player's own weapon.** The player is the one holder with no [`SceneObject`]: his body is
/// built locally as [`dereth_client::character::Character`] and the server's copy is not drawn, so
/// a weapon wielded by *him* hangs off that local part array instead.
///
/// It takes a different branch in `place_held_objects` from every other holder, so it gets its own
/// test. The holder here is `ALUVIAN_MALE_SETUP`, the setup the local body is built from, and the
/// expected frame is composed from `Character`'s own part frames, so using the server's (absent)
/// `SceneObject` for the player, or falling back to his root frame, fails.
#[test]
fn a_weapon_wielded_by_the_player_hangs_off_his_own_body() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let player_setup = dereth_client::character::ALUVIAN_MALE_SETUP;
    let holder_dat = setup_of(&store, player_setup.0);
    let Some(holding) = holder_dat.holding_locations.get(&RIGHT_HAND).copied() else {
        eprintln!("skipping: the local body's setup carries no RightHand holding location");
        return;
    };

    // The local body. `SceneConfig::character` alone does not build one: `attach_character` is a
    // separate call the application makes, and every test that needs a body makes it too.
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut scene = scene_of(&store, &mut gpu, true);
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    // The player identity event arrives before the player's object description.
    stream.apply_event(
        &SessionEvent::PlayerCreated(ObjectId(HOLDER)),
        LocalTime(0.0),
    );
    stream.apply_event(
        &create_ev(HOLDER, player_setup.0, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    // ...and the weapon in his right hand.
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            None,
            Some((HOLDER, RIGHT_HAND)),
            Some(RIGHT_HAND_COMBAT),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    // Three steps, not one: the settled composition from the local body's parts is asserted, not
    // every transient frame after attaching at the configured start cell.
    step(&mut scene);
    step(&mut scene);
    step(&mut scene);

    // The player has no `SceneObject`, so the scene holds exactly one object: the weapon.
    assert_eq!(
        scene.server_object_count(),
        1,
        "only the weapon is a SceneObject"
    );
    assert_eq!(
        scene.draw.stats.server_objects_held, 1,
        "and it is drawn, on the local body"
    );
    assert_eq!(scene.draw.stats.held_without_holding_location, 0);

    let item_frame = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("the weapon is drawn");
    let (body_root, body_parts) = scene
        .character_frames()
        .expect("the local body exists with character: true");
    assert_eq!(
        item_frame,
        child_frame(&body_root, &body_parts, &to_anim(holding)),
        "the player's weapon hangs off his own part array"
    );
    // The weapon sits at the **hand**, one holding-location offset away: 0.0800 m, the magnitude
    // of `RightHand`'s (0.0250, 0.0000, -0.0760) in the dat. A composition that dropped the part
    // frame and used the root would put it at the feet, a metre away.
    let to_hand = dist(item_frame.origin, body_parts[15].origin);
    let to_root = dist(item_frame.origin, body_root.origin);
    assert!(
        to_hand < 0.12,
        "the weapon must be in the hand, not near it: {to_hand:.4} m"
    );
    assert!(
        to_root > 0.5,
        "and the hand must be well clear of the body's own origin, or this proves little: {to_root:.3} m"
    );
    eprintln!(
        "the player's own weapon is drawn at ({:.3}, {:.3}, {:.3}) -- {to_hand:.4} m from his right hand and {to_root:.3} m from his feet",
        item_frame.origin.x,
        item_frame.origin.y,
        item_frame.origin.z,
    );
}

/// Behaviour: objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand
///
/// **A wielded item the server says has been put away stops being drawn.**
///
/// A pickup (`0xF74A`) leaves the world and removes the item from cell lists, so the object's
/// `SceneObject::drawn` state clears and its last hand frame is no longer drawable. The item keeps
/// its place in the object map and the holder keeps its frame: the item is hidden, not left at the
/// old hand position and not deleted. Frame availability is compared, not pixel captures.
#[test]
fn a_wielded_weapon_the_server_puts_away_stops_being_drawn() {
    use dereth_protocol::objects::InventoryPickupEvent;

    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let mut scene = scene_of(&store, &mut gpu, false);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    stream.apply_event(
        &create_ev(HOLDER, HOLDER_SETUP, Some(Vec3::ZERO), None, Some(101)),
        LocalTime(0.0),
    );
    stream.apply_event(
        &create_ev(
            ITEM,
            ITEM_SETUP,
            None,
            Some((HOLDER, RIGHT_HAND)),
            Some(RIGHT_HAND_COMBAT),
        ),
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);

    // Calibration: a drawable hand frame must exist first, or its later absence proves nothing.
    let held = scene
        .server_object_frame(ObjectId(ITEM))
        .expect("the weapon is drawn to begin with");
    assert_eq!(scene.draw.stats.server_objects_held, 1);
    let holder_root = scene
        .server_object_frame(ObjectId(HOLDER))
        .expect("the holder is drawn");
    assert!(
        dist(held.origin, holder_root.origin) > 0.5,
        "and it is drawn at the hand, a long way from the holder's own origin"
    );

    // `0xF74A Inventory_PickupEvent`, the opcode as a number, so the test cannot pass by agreeing
    // with the production code on a wrong one.
    let body = write_body(&InventoryPickupEvent {
        id: ObjectId(ITEM),
        timestamps: PhysicsEventStamp {
            instance: 0,
            event: 5,
        },
    })
    .expect("encode");
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(0xF74A),
            body,
        },
        LocalTime(0.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);

    assert_eq!(
        scene.server_object_count(),
        2,
        "the object is NOT deleted: a pickup is not a 0xF747"
    );
    assert_eq!(
        scene.draw.stats.server_objects_held, 0,
        "nothing is held any more"
    );
    assert!(
        scene.server_object_frame(ObjectId(ITEM)).is_none(),
        "and the weapon is no longer submitted -- it was still at {held:?} before"
    );
    // The holder is untouched.
    assert!(scene.server_object_frame(ObjectId(HOLDER)).is_some());
    eprintln!(
        "the weapon was drawn at ({:.3}, {:.3}, {:.3}) and is not drawn at all after the 0xF74A",
        held.origin.x, held.origin.y, held.origin.z
    );
}
