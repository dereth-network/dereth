//! **An object the server names a placement for is drawn at that placement's frames, not at
//! `0x65`**, asserted through the real `WorldScene`, on a real device.
//!
//! Fixture: the retail dats, and a `0xF745` assembled with `dereth_protocol`'s own encoder and
//! pushed through `ObjectStream` and `WorldScene::sync_objects`. The anchor comes from the packet
//! corpus: every recorded create that names a non-`Resting` placement **and** that this client
//! draws is setup `0x02000124` at `MissileFlight (52)`, with no motion table (the corpus claim is
//! `dat::objects::server_placement::the_corpus_poses_objects_at_the_placements_the_server_names`).
//! The expected part frames are read out of the setup record's own `placement_frames` and
//! composed by `dereth_world_render`'s `combine`, as the client's part-array placement does.
//!
//! `WorldScene::sync_objects` builds the part meshes, so it needs a device: WARP, like
//! `objects::populated_world`, so nothing depends on the machine's adapter. **Fails** when there
//! are no dats or no device.

#![cfg(gpu)]

use super::common::{retail_store, test_gpu};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::{Decode, Setup};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, LocalTime, ObjectId, Vec3};
use dereth_protocol::types::{
    physicsdesc::flags, ObjDesc, PhysicsDesc, PositionWire, PublicWeenieDesc,
};
use dereth_protocol::{write_body, Opcode};
use {
    dereth_client_runtime::models::placement_frames,
    dereth_client_runtime::models::PLACEMENT_DEFAULT,
    dereth_client_runtime::models::PLACEMENT_RESTING,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The corpus's anchor. Carries placement keys `0, 1, 2, 3, 4, 6, 52, 101, 103, 121..130`.
const SETUP: u32 = 0x0200_0124;
/// The land cell the object stands in, **outdoors, on the block this bench flies its camera over**.
///
/// **Not an interior cell.** Visible-cell drawing submits objects **only for cells in
/// `cell_draw_list`**, so an object whose draw cell is an interior one (such as `0x8602_01AD`,
/// the training dungeon's shot room) is submitted only on a frame whose traversal reached that
/// cell. Every frame this file draws is an outdoor one over `0x8602`: there is no body, no portal
/// walk, and `WorldScene::drawn_cells()` is `None`. A dungeon room's object would not be in
/// retail's list either, and the pixel test would compare two frames the object is absent from.
///
/// `0x8602_0001` is land cell 1 of the same block; [`outdoor`] names the cell an arbitrary offset
/// actually falls in, which is what the pixel bench needs.
const CELL: u32 = 0x8602_0001;
const LANDBLOCK: u16 = 0x8602;
const OBJ: u32 = 0x5000_0057;

/// The outdoor land cell of [`LANDBLOCK`] an offset falls in: the client's outside-position
/// adjustment, the same operation `Character::teleport` uses and the same one
/// `rendering::landscape_draw_distance`'s bench uses to place its target. See [`CELL`].
fn outdoor(at: Vec3) -> (dereth_primitives::CellId, Vec3) {
    let mut cell = dereth_primitives::CellId(CELL);
    let mut origin = at;
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
    (cell, origin)
}

/// A `0xF745 Item_CreateObject` for one object, optionally naming a placement in the `PhysicsDesc`
/// **animframe** field, where the client's description handling reads it.
fn create(placement: Option<u32>) -> SessionEvent {
    let mut physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP | flags::POSITION,
        setup_id: Some(SETUP),
        position: Some(PositionWire {
            objcell_id: CELL,
            ..PositionWire::default()
        }),
        ..PhysicsDesc::default()
    };
    if let Some(p) = placement {
        physicsdesc.bitfield |= flags::ANIMFRAME;
        physicsdesc.animframe_id = Some(p);
    }
    let body = write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(OBJ),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body,
    }
}

/// Build the scene, put one object in it, and hand back its part frames plus its world frame.
fn part_frames_for(placement: Option<u32>) -> Option<(Vec<Frame>, Frame)> {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    stream.apply_event(&create(placement), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the object syncs");
    assert_eq!(
        scene.server_object_count(),
        1,
        "the object reached the scene"
    );
    // The part-array update runs in the frame step, not in the create: `draw`
    // places every part from the current animation frame before anything is submitted. One step
    // with no input is what puts the placement frame into `parts[i].pos`.
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
    let parts = scene.server_object_part_frames(ObjectId(OBJ))?;
    let world = scene.server_object_frame(ObjectId(OBJ))?;
    Some((parts, world))
}

fn setup_of(store: &RetailDatStore) -> Setup {
    let bytes = store
        .read_typed(DbType::Setup, DataId(SETUP))
        .expect("reads");
    Setup::decode_payload(DataId(SETUP), &bytes).expect("decodes")
}

/// Behaviour: objects.placement.an-object-is-drawn-at-its-named-placement
///
/// **The named placement is drawn.** Two runs of the same scene differing only in whether the
/// create named a placement: the named one draws at `MissileFlight`'s frames, the unnamed one at
/// `Resting`'s, and the two are not the same pose.
///
/// Removing `world.rs`'s `set_placement_frame` call, or `objects.rs`'s read of the animframe
/// field, makes the two runs identical and reddens this.
#[test]
fn an_object_the_server_names_a_placement_for_is_drawn_at_it() {
    let store = retail_store();
    let dat = setup_of(&store);
    let named = dat.placement_frames[&52].frames.clone();
    // The control is `Placement.Default`, not `Resting`: a create whose `PhysicsDesc` names neither
    // a movement buffer nor an animframe carries a **zeroed** placement field, and the client's
    // description handling installs that over the `0x65` placed during setup creation. `Resting`
    // is what an object with no descriptor at all keeps, which is scenery, not this.
    let unnamed = dat.placement_frames[&PLACEMENT_DEFAULT].frames.clone();
    assert_ne!(
        named, unnamed,
        "the two keys must differ or this proves nothing"
    );
    assert_ne!(
        unnamed, dat.placement_frames[&PLACEMENT_RESTING].frames,
        "and key 0 must differ from 0x65, or the control would not distinguish them"
    );

    let (with_placement, world_a) =
        part_frames_for(Some(52)).expect("the part frames for that placement");
    let (without, world_b) = part_frames_for(None).expect("the part frames for that placement");

    // Same object, same cell, same server position: only the pose is under test.
    assert_eq!(
        world_a, world_b,
        "the object's world frame must not depend on the placement"
    );
    assert_eq!(with_placement.len(), dat.parts.len());
    assert_eq!(without.len(), dat.parts.len());

    // The part-array update puts part i at combine(world, placement_frame[i]).
    for (i, got) in with_placement.iter().enumerate() {
        let want = dereth_world_render::math::combine(&world_a, &named[i]);
        assert_eq!(*got, want, "part {i} must sit at MissileFlight's frame");
    }
    for (i, got) in without.iter().enumerate() {
        let want = dereth_world_render::math::combine(&world_b, &unnamed[i]);
        assert_eq!(
            *got, want,
            "part {i} must sit at Placement.Default with no placement named"
        );
    }

    let moved = with_placement
        .iter()
        .zip(without.iter())
        .map(|(a, b)| {
            let d = Vec3::new(
                a.origin.x - b.origin.x,
                a.origin.y - b.origin.y,
                a.origin.z - b.origin.z,
            );
            d.magnitude()
        })
        .fold(0.0f32, f32::max);
    eprintln!(
        "setup {SETUP:#010X}: MissileFlight moves the drawn parts {moved:.4} m off this test's \
         control, a descriptor naming nothing (Placement.Default). Against Resting the same \
         setup moves 0.05 m."
    );
    assert!(moved > 0.0, "the two poses must actually differ on screen");
}

/// Behaviour: objects.placement.an-object-is-drawn-at-its-named-placement
///
/// **An object whose setup has no entry at the named id is unchanged.** Through the same path:
/// the client's placement selection falls back to key
/// `0`, and this setup's key `0` is not its key `0x65`, so a fallback that silently kept `0x65`
/// would pass and a fallback that dropped to the identity would fail.
#[test]
fn a_placement_the_setup_does_not_carry_falls_back_to_key_zero() {
    let store = retail_store();
    let dat = setup_of(&store);
    // 0x0FFF is not in ACE's `Placement`, so nothing carries it.
    assert!(!dat.placement_frames.contains_key(&0x0FFF));
    let fallback = placement_frames(&dat, 0x0FFF)
        .expect("key 0 exists")
        .to_vec();
    assert_eq!(fallback, dat.placement_frames[&0].frames);

    let (parts, world) = part_frames_for(Some(0x0FFF)).expect("the part frames for that placement");
    for (i, got) in parts.iter().enumerate() {
        let want = dereth_world_render::math::combine(&world, &fallback[i]);
        assert_eq!(*got, want, "part {i} falls back to Placement.Default");
    }
}

/// Behaviour: objects.placement.an-object-is-drawn-at-its-named-placement
///
/// **The placement is not fixed for the life of an object.** A later `0xF748` carrying
/// `HAS_PLACEMENT_ID` re-poses one already on screen through the client's position-event
/// handling: the runtime half, as opposed to the install half on create.
///
/// Removing `world.rs`'s per-frame re-apply reddens this and nothing else, because a create alone
/// never exercises it.
#[test]
fn a_later_position_event_reposes_an_object_already_on_screen() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let dat = setup_of(&store);
    let named = dat.placement_frames[&52].frames.clone();
    // Created naming nothing, so `set_description` installed `Placement.Default` — see
    // `an_object_the_server_names_a_placement_for_is_drawn_at_it` for why that is the control.
    let unnamed = dat.placement_frames[&PLACEMENT_DEFAULT].frames.clone();

    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let mut stream = ObjectStream::new();

    stream.apply_event(&create(None), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let world = scene.server_object_frame(ObjectId(OBJ)).expect("a frame");
    let before = scene
        .server_object_part_frames(ObjectId(OBJ))
        .expect("parts");
    for (i, got) in before.iter().enumerate() {
        assert_eq!(
            *got,
            dereth_world_render::math::combine(&world, &unnamed[i])
        );
    }

    // The server then says "MissileFlight" on a position event.
    stream.apply_event(&position_naming(52), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let world = scene.server_object_frame(ObjectId(OBJ)).expect("a frame");
    let after = scene
        .server_object_part_frames(ObjectId(OBJ))
        .expect("parts");
    for (i, got) in after.iter().enumerate() {
        assert_eq!(
            *got,
            dereth_world_render::math::combine(&world, &named[i]),
            "part {i} re-posed"
        );
    }
    assert_ne!(before, after, "the object must actually have moved");
}

fn step(scene: &mut WorldScene) {
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
}

/// A `0xF748 Movement_UpdatePosition` for the same object, at the same place, naming a placement —
/// so the only thing that can change is the pose.
fn position_naming(placement: u32) -> SessionEvent {
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    let pp = PositionPack {
        flags: position_flags::HAS_PLACEMENT_ID,
        origin: dereth_protocol::types::Origin {
            objcell_id: CELL,
            origin: dereth_protocol::types::Vec3::default(),
        },
        placement_id: Some(placement),
        position_timestamp: 5,
        ..PositionPack::default()
    };
    let body = write_body(&MovementPositionEvent {
        id: ObjectId(OBJ),
        position: pp,
    })
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_POSITION_EVENT,
        body,
    }
}

/// Behaviour: objects.placement.an-object-is-drawn-at-its-named-placement
///
/// **The pixel differential.** Two frames of the same scene through the same WARP device, differing
/// only in whether the create named a placement, and the changed pixels are confined to a declared
/// rectangle with **zero** outside it.
///
/// The anchor is setup `0x02000BCC` at `MissileFlight (52)` — the one non-`Resting` placement
/// the recorded corpus names for an object this client draws — chosen by measurement: of every
/// setup with a frame for that key, it moves a part furthest off `Resting`, **1.488 m**. The
/// corpus's own instance of that id is setup `0x02000124`, which moves 0.05 m —
/// too little to see, which is a fact about the recordings and not about the mechanism.
#[test]
fn the_named_placement_moves_pixels_and_only_where_the_object_is() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);

    // Where the camera is looking, from the same ray the picker builds — so the object is put in
    // front of the lens rather than at a guessed point. `selection_ray` through the middle pixel is
    // the view axis by construction.
    let viewport = (800u32, 600u32);
    let mut shot = |placement: Option<u32>| -> Option<(Vec<u8>, u32, u32)> {
        use dereth_client_runtime::pick::PickScene;
        let cfg = SceneConfig {
            landblock: LANDBLOCK,
            start_cell: Some(dereth_primitives::CellId(CELL)),
            character: true,
            land_radius: 1,
            scenery_radius: 0,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        let mut stream = ObjectStream::new();

        // Pass 1: the object at the cell's own origin, to learn where that lands in render space.
        stream.apply_event(&create_at(MOVER_SETUP, None, Vec3::ZERO), LocalTime(0.0));
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("syncs");
        step(&mut scene);
        let cell_origin = scene
            .server_object_frame(ObjectId(OBJ))
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
        // Six metres down the view axis: far enough not to be inside the near plane, close enough
        // that a 1.5 m displacement is many pixels wide.
        let target = Vec3::new(
            viewer.origin.x + ray.x * 6.0,
            viewer.origin.y + ray.y * 6.0,
            viewer.origin.z + ray.z * 6.0,
        );
        let at = Vec3::new(
            target.x - cell_origin.x,
            target.y - cell_origin.y,
            target.z - cell_origin.z,
        );

        // Pass 2: the object where the camera is pointing, with or without a named placement.
        let mut stream = ObjectStream::new();
        stream.apply_event(&create_at(MOVER_SETUP, placement, at), LocalTime(0.0));
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

    let (a, w, h) = shot(Some(52)).expect("frame with a placement");
    let (b, w2, h2) = shot(None).expect("frame without one");
    assert_eq!((w, h), (w2, h2));
    assert_eq!(a.len(), b.len());

    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a[i..i + 4] != b[i..i + 4] {
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
        "setup {MOVER_SETUP:#010X} at MissileFlight vs Resting: {n} of {total} pixels differ \
         ({:.2} %), bounding box ({x0},{y0})-({x1},{y1})",
        100.0 * n as f32 / total as f32
    );
    assert!(
        n > 0,
        "the two poses drew the same frame -- the object was not on screen"
    );

    // Every changed pixel is inside that one box, and the box is a small part of the frame: this
    // moves an object, not the world.
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a[i..i + 4] != b[i..i + 4] {
                assert!(x >= x0 && x <= x1 && y >= y0 && y <= y1);
            }
        }
    }
    assert!(
        n * 4 < total,
        "{n} pixels changed -- a placement must not repaint the scene"
    );
}

/// The largest mover off `Resting` at `MissileFlight (52)`, the one non-`Resting` id the corpus
/// names for an object this client draws. 1.488 m, measured over every setup's placement frames
/// for key 52. The corpus's own instance of that id is `0x02000124`, which moves 0.05 m —
/// too little to see, which is a fact about the recordings rather than about the mechanism.
const MOVER_SETUP: u32 = 0x0200_0BCC;

/// [`create`] for an arbitrary setup.
fn create_at(setup: u32, placement: Option<u32>, at: Vec3) -> SessionEvent {
    let (cell, origin) = outdoor(at);
    let mut physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP | flags::POSITION,
        setup_id: Some(setup),
        position: Some(PositionWire {
            // The land cell this offset falls in and the offset normalised into
            // it, not a fixed cell and a raw offset: the bench puts the object wherever the camera
            // is looking, and that can be past the edge of [`CELL`]'s own block.
            // `adjust_to_outside` names the neighbour and wraps the coordinates, so the pair
            // describes one world point -- the same point `cell_origin + at` names. See [`CELL`].
            objcell_id: cell.0,
            frame: dereth_protocol::types::Frame {
                origin: dereth_protocol::types::Vec3 {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
                ..dereth_protocol::types::Frame::default()
            },
        }),
        ..PhysicsDesc::default()
    };
    if let Some(p) = placement {
        physicsdesc.bitfield |= flags::ANIMFRAME;
        physicsdesc.animframe_id = Some(p);
    }
    let body = write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(OBJ),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body,
    }
}

/// **An object with an animation playing refuses the position event's placement.**
///
/// The client's position-event handling checks whether the animation list is nonempty and
/// installs the id only when it is empty. The list is on the `MotionDriver`, so this is the arm the
/// draw owns and the object stream cannot answer; removing the `has_anims()` guard in `world.rs`
/// reddens this and nothing else.
#[test]
fn an_object_with_an_animation_refuses_a_position_events_placement() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);

    // Any real motion table enters the default state, which queues
    // the standing cycle and is what makes `has_anims()` true.
    let tables = store.ids_of(DbType::MTable);
    let Some(mtable) = tables.first().copied() else {
        eprintln!("skipping: no motion tables in the dats");
        return;
    };

    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        start_cell: Some(dereth_primitives::CellId(CELL)),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    stream.apply_event(&create_animated(mtable.0), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let before = scene
        .server_object_part_frames(ObjectId(OBJ))
        .expect("parts");

    stream.apply_event(&position_naming(52), LocalTime(0.0));
    assert_eq!(
        stream
            .presence(ObjectId(OBJ))
            .expect("a presence")
            .pending_placement,
        Some(52),
        "the stream parks it whatever the object is doing"
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("syncs");
    step(&mut scene);
    let after = scene
        .server_object_part_frames(ObjectId(OBJ))
        .expect("parts");

    assert_eq!(
        before, after,
        "an animated object must not be re-posed by a placement frame"
    );
    assert_eq!(
        stream
            .presence(ObjectId(OBJ))
            .expect("a presence")
            .placement,
        PLACEMENT_RESTING,
        "and the refused id is not recorded as installed"
    );
    // The guard has to have been the reason: the same object without an animation *does* move,
    // which `a_later_position_event_reposes_an_object_already_on_screen` asserts on this very
    // setup.
    let dat = setup_of(&store);
    assert_ne!(
        dat.placement_frames[&52].frames,
        dat.placement_frames[&PLACEMENT_RESTING].frames
    );
}

/// [`create`] with a motion table, so the object's sequence has animations in it.
fn create_animated(mtable: u32) -> SessionEvent {
    let mut physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP | flags::POSITION | flags::MTABLE,
        setup_id: Some(SETUP),
        mtable_id: Some(mtable),
        position: Some(PositionWire {
            objcell_id: CELL,
            ..PositionWire::default()
        }),
        ..PhysicsDesc::default()
    };
    physicsdesc.bitfield |= flags::ANIMFRAME;
    physicsdesc.animframe_id = Some(PLACEMENT_RESTING);
    let body = write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(OBJ),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body,
    }
}
