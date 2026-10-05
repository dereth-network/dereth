//! The frame a landblock crossing is drawn on is the same picture as the next frame: no flicker.
//! On the update whose `recenter()` moves the viewer block, the local body's parts and the chase
//! camera come out in the space the window moved to. Retail's part update writes the object's
//! cell-local frame and applies the viewer-block origin at draw time; our part frames carry that
//! origin (so vertices stay block-local), so anything writing a part frame or a camera must run
//! after the re-centre, as `WorldScene::advance_objects` does for the server's objects. Parts
//! placed first would sit `(-192 * dx, -192 * dy, 0)` away for one frame. Calibrated both ways: a
//! frame five blocks away must differ, and the same scene drawn twice must not. Where the body's
//! parts and the camera are placed on the crossing update, which needs no device, is the `dat`
//! tier's `world::landblock_recentre`. Fixture: the retail dats on a WARP device at 800x600,
//! crossings out of Holtburg. Fails when the retail dats are absent or a device is absent.
//!
//! Sections of this module:
//! * `frame_writers`: every writer of a drawn frame (the body's parts and the server's objects)
//!   runs after the re-centre.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
    dereth_world_data::landblock::block_xy, dereth_world_data::landblock::DEFAULT_LANDBLOCK,
};

const W: usize = 800;
const H: usize = 600;
/// `BLOCK_LENGTH`: one landblock, and therefore the exact size of a part misplacement.
const BLOCK: f32 = 192.0;

/// The thresholds every count in this module is reported at. **A count produced by a threshold is
/// not a fact until the threshold is stated.** This keeps a visually reassuring total from
/// hiding which pixel differences produced it.
const BRACKETS: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above.
    LandblockId(((x as u16) << 8) | (y as u16))
}

fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let cfg = SceneConfig::default();
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// One `WorldScene::update` and nothing else, **no clock**. Every station in this module is at
/// `LocalTime(0.0)` with `dt = 0.0`, so two frames taken at two stations hold the same pose by
/// construction and a differential between them cannot be about the idle cycle.
fn tick(scene: &mut WorldScene) {
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
}

/// Put the body in the middle of `block`, then settle and stream. The teleport itself is not the
/// subject: it is how the body is placed *before* the crossing under test.
fn go_to(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu, block: LandblockId) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();
    tick(scene);
    scene.stream(store, gpu).expect("the streamed blocks build");
}

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Vec<u8> {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Per-pixel maximum absolute channel difference over R, G and B. Alpha is forced opaque by
/// `CapturedImage::to_rgba`, and comparing a channel nothing writes would be comparing the clear.
fn deltas(a: &[u8], b: &[u8]) -> Vec<u8> {
    assert_eq!(a.len(), b.len(), "two frames of different sizes");
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .map(|(p, q)| (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0))
        .collect()
}

fn at(d: &[u8], t: u8) -> usize {
    d.iter().filter(|v| **v >= t).count()
}

fn brackets(d: &[u8]) -> String {
    BRACKETS
        .iter()
        .map(|&t| format!("t>={t}: {}", at(d, t)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where the part update last put the body's first part, in the renderer's
/// viewer-block-relative space.
fn first_part(scene: &WorldScene) -> Vec3 {
    let c = scene.character.as_ref().expect("a body");
    let d = c.driver();
    d.part_array
        .parts
        .first()
        .map_or(Vec3::ZERO, |p| p.pos.origin)
}

/// One crossing, everything it produced.
struct Crossing {
    /// The frame drawn on the update that re-centred the window.
    crossing: Vec<u8>,
    /// The frame drawn one further update later, at the same station, with nothing streamed.
    settled: Vec<u8>,
    /// The same scene drawn twice with no update at all: the differ's known-**same** pair.
    repeat: Vec<u8>,
    /// A frame five landblocks away: the differ's known-different pair.
    far: Vec<u8>,
    /// The viewer block before and after the crossing update.
    viewer_block: ((i32, i32), (i32, i32)),
    /// Blocks resident on the crossing frame and on the settled one.
    resident: (usize, usize),
}

/// Walk the body from the home block into the block `(dx, dy)` away and measure the frame the
/// crossing is drawn on.
fn cross(store: &Arc<RetailDatStore>, gpu: &mut Gpu, (dx, dy): (i32, i32)) -> Crossing {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = embodied(store, gpu);
    go_to(&mut scene, store, gpu, block_at(hx, hy));

    // The known-same calibration, taken before anything moves: draw the settled home scene twice
    // with no update between the two captures.
    let first = shot(&mut scene, gpu);
    let repeat = shot(&mut scene, gpu);
    let repeat = deltas(&first, &repeat);

    // --- the crossing. The teleport moves the body into the next block and `follow_character_now`
    // puts the camera behind it there; neither re-centres the window. The single `WorldScene::
    // update` below is the frame under test: it is the one whose `recenter()` moves the viewer.
    let before_block = scene.viewer_block().expect("a viewer block");
    let target = block_at(hx + dx, hy + dy);
    {
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        let mid = 96.0f32;
        let z = land.ground_height(target, mid, mid).unwrap_or(0.0) + 1.0;
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            target.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();

    tick(&mut scene);
    scene.stream(store, gpu).expect("the streamed blocks build");
    let after_block = scene.viewer_block().expect("a viewer block");
    let resident_crossing = scene.resident_blocks();
    let crossing = shot(&mut scene, gpu);

    // --- one further update at the same station. No teleport, no input, no clock; `stream` has
    // nothing pending, so no block is fetched, released, re-meshed or re-baked. The only thing
    // that can change is the body's own placement and the camera derived from it.
    tick(&mut scene);
    scene.stream(store, gpu).expect("nothing to stream");
    let resident_settled = scene.resident_blocks();
    let settled = shot(&mut scene, gpu);

    go_to(&mut scene, store, gpu, block_at(hx + 5, hy + 5));
    let far = shot(&mut scene, gpu);
    scene.release_textures(gpu);

    Crossing {
        crossing,
        settled,
        repeat,
        far,
        viewer_block: (before_block, after_block),
        resident: (resident_crossing, resident_settled),
    }
}

/// The four crossings measured: one block on each axis, both together, and a four-block hop.
/// The last is there because a camera patched by subtracting the shift loses low bits as a
/// function of the *size* of the shift, and at one block that subtraction happens to be exact.
const CROSSINGS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (4, 4)];

// ---------------------------------------------------------------------------------------------
// The picture
// ---------------------------------------------------------------------------------------------

/// **The frame a crossing is drawn on is the frame the next update produces: there is no
/// flicker.**
///
/// This is the player-visible half, and it is a single-variable differential: between the two
/// captures nothing is teleported, no clock advances, and `stream` fetches, releases and re-bakes
/// nothing (asserted, not assumed). So the only thing that could have moved is the body's own
/// placement and the camera derived from it.
#[test]
fn the_crossing_frame_and_the_next_frame_are_the_same_picture() {
    let store = store();

    for legs in CROSSINGS {
        let mut gpu = crate::common::test_gpu(
            u32::try_from(W).expect("a back-buffer width"),
            u32::try_from(H).expect("a back-buffer height"),
        );
        let c = cross(&store, &mut gpu, legs);
        let moved = deltas(&c.crossing, &c.settled);
        let control = deltas(&c.crossing, &c.far);

        eprintln!("=== crossing {legs:?}");
        eprintln!("    crossing vs the next frame : {}", brackets(&moved));
        eprintln!(
            "    crossing vs five blocks away: {} (known-different)",
            brackets(&control)
        );
        eprintln!(
            "    the same frame drawn twice  : {} (known-same)",
            brackets(&c.repeat)
        );

        // --- both calibrations, before either count above is worth anything.
        assert!(
            at(&control, 1) > 400_000,
            "the differ cannot see a frame taken five landblocks away, so nothing it reports is a \
             measurement: {}",
            brackets(&control)
        );
        for t in BRACKETS {
            assert_eq!(
                at(&c.repeat, t),
                0,
                "drawing the same unchanged scene twice differs by {} px at t>={t}, so this \
                 renderer is not deterministic between two captures and a zero below would mean \
                 nothing either: {}",
                at(&c.repeat, t),
                brackets(&c.repeat)
            );
        }

        // --- the single-variable premise.
        assert_eq!(
            c.resident.0, c.resident.1,
            "the settling update streamed or released a block, so this is not a single-variable \
             differential"
        );

        // --- **and the crossing premise.** A window that never re-centres at all would leave
        // this test green, because two frames of a crossing that never happened agree perfectly,
        // so assert that the subject was alive enough to have failed.
        assert_eq!(
            (
                c.viewer_block.1 .0 - c.viewer_block.0 .0,
                c.viewer_block.1 .1 - c.viewer_block.0 .1
            ),
            legs,
            "the window did not re-centre by the block the body crossed into ({:?} -> {:?}), so \
             these two frames agreeing says nothing about a crossing",
            c.viewer_block.0,
            c.viewer_block.1
        );

        // --- the claim.
        for t in BRACKETS {
            assert_eq!(
                at(&moved, t),
                0,
                "the frame drawn on the crossing differs from the next frame by {} px at t>={t}, \
                 so the body or the camera was placed in the space the re-centre left: {}",
                at(&moved, t),
                brackets(&moved)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Every writer of a drawn frame runs after the re-centre
// ---------------------------------------------------------------------------------------------

mod frame_writers {
    //! Every writer of a drawn frame (the local body's part placement and
    //! `WorldScene::advance_objects` for the server's objects) runs after the window re-centres.
    //! Retail writes cell-local frames and applies the viewer block's origin at draw time, so it has
    //! no ordering; our frames carry that origin, so a writer must run after the re-centre moves the
    //! space it writes in. `WorldScene::recenter` returns a `RenderSpace` both writers take by value,
    //! which the compiler enforces; this module checks the property at runtime, against an absolute
    //! oracle: on a re-centring frame each drawn frame is where the **new** block puts it and
    //! `192 * shift` from where the **old** one would, so an unplaced part fails for being at neither.
    //! Fixture: the retail dats on a WARP device at 320x240, a body and a `0x0200_0001` prop in
    //! Holtburg. Fails when the retail dats are absent or a device is absent.

    use super::{block_at, first_part, store, BLOCK};
    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::camera::CameraInput;
    use dereth_client_runtime::character::CharacterInput;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::num::math;
    use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
    use dereth_protocol::types::PhysicsTimestamps;
    use dereth_protocol::types::{
        physicsdesc::flags, ObjDesc, PhysicsDesc, PositionWire, PublicWeenieDesc,
    };
    use dereth_protocol::{write_body, Opcode};
    use dereth_render::device::Gpu;
    use {
        dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
        dereth_world_data::landblock::block_xy, dereth_world_data::landblock::DEFAULT_LANDBLOCK,
    };

    /// A server object stood in the home block, so that [`WorldScene::advance_objects`], the *other*
    /// writer of a drawn frame, has something to place.
    const PROP: u32 = 0x7000_0042;
    /// The human setup, a real drawable body out of the dats.
    const PROP_SETUP: u32 = 0x0200_0001;

    /// A create-event fixture for one object stood at `at` of `cell`.
    fn create_ev(id: u32, setup: u32, cell: u32, at: Vec3) -> SessionEvent {
        let physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP | flags::POSITION,
            setup_id: Some(setup),
            position: Some(PositionWire {
                objcell_id: cell,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_protocol::types::Vec3 {
                        x: at.x,
                        y: at.y,
                        z: at.z,
                    },
                    ..dereth_protocol::types::Frame::default()
                },
            }),
            timestamps: PhysicsTimestamps::default(),
            ..PhysicsDesc::default()
        };
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

    fn planar(a: Vec3, b: Vec3) -> f32 {
        math::hypotf(a.x - b.x, a.y - b.y)
    }

    /// A landblock-local point re-expressed in the render space of `viewer`, done here rather than
    /// through `WorldScene::render_frame_of` so the oracle is not the code under test.
    ///
    /// The render space's origin is the south-west corner of the viewer's block.
    fn in_space_of(block: (i32, i32), local: Vec3, viewer: (i32, i32)) -> Vec3 {
        #[allow(clippy::cast_precision_loss)] // a block index difference, at most 255
        Vec3::new(
            local.x + (block.0 - viewer.0) as f32 * BLOCK,
            local.y + (block.1 - viewer.1) as f32 * BLOCK,
            local.z,
        )
    }

    struct Scene {
        scene: WorldScene,
        store: Arc<RetailDatStore>,
    }

    impl Scene {
        fn step(&mut self, gpu: &mut Gpu, now: f64) {
            self.scene.update(
                CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            self.scene
                .stream(&self.store, gpu)
                .expect("the streamed blocks build");
        }
    }

    /// A scene with a body **and** a server object, settled on the home block.
    fn settled(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Option<(Scene, Vec3, (i32, i32))> {
        let mut scene =
            WorldScene::load(store, gpu, SceneConfig::default()).expect("the scene loads");
        let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");

        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let home = block_at(hx, hy);
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        let prop_at = Vec3::new(
            96.0,
            96.0,
            land.ground_height(home, 96.0, 96.0).unwrap_or(0.0) + 1.0,
        );

        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                home.cell(1),
                Frame::new(Vec3::new(100.0, 100.0, prop_at.z), Quat::IDENTITY),
            ));
        }

        let mut stream = ObjectStream::new();
        stream.apply_event(
            &create_ev(PROP, PROP_SETUP, home.cell(1).0, prop_at),
            LocalTime(0.0),
        );
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");

        let mut s = Scene {
            scene,
            store: Arc::clone(store),
        };
        for i in 1..=10 {
            s.step(gpu, f64::from(i) / 30.0);
        }
        assert!(
            s.scene.server_object_frame(ObjectId(PROP)).is_some(),
            "the prop object was not created, so `advance_objects` has no subject"
        );
        Some((s, prop_at, (hx, hy)))
    }

    // ---------------------------------------------------------------------------------------------
    // 1. The calibration: the render space really does move, and by how much
    // ---------------------------------------------------------------------------------------------

    /// **The two candidate answers are `192 * shift` apart**, so the assertions below can tell them
    /// apart at all.
    ///
    /// This is the premise for the whole module, stated as its own test rather than buried in an
    /// assertion message: if `recenter` did not move the window, or moved it by zero blocks, then
    /// "the writer used the new space" and "the writer used the old one" are the same claim and
    /// nothing here is a measurement.
    #[test]
    fn a_crossing_moves_the_render_space_by_exactly_the_block_shift() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let Some((mut s, _, (hx, hy))) = settled(&store, &mut gpu) else {
            return;
        };

        let before = s.scene.viewer_block().expect("a viewer block");
        let target = block_at(hx + 1, hy + 1);
        {
            let land = Arc::clone(s.scene.character.as_ref().expect("a body").land());
            let z = land.ground_height(target, 96.0, 96.0).unwrap_or(0.0) + 1.0;
            let c = s.scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                target.cell(1),
                Frame::new(Vec3::new(96.0, 96.0, z), Quat::IDENTITY),
            ));
        }
        s.step(&mut gpu, 1.0);
        let after = s.scene.viewer_block().expect("a viewer block");
        s.scene.release_textures(&mut gpu);

        eprintln!("calibration: viewer block {before:?} -> {after:?}");
        assert_eq!(
            (after.0 - before.0, after.1 - before.1),
            (1, 1),
            "the window did not re-centre, so the two candidate render spaces are the same one"
        );

        // And what that is worth in metres, at the station the tests below use.
        let local = Vec3::new(96.0, 96.0, 0.0);
        let old = in_space_of((hx + 1, hy + 1), local, before);
        let new = in_space_of((hx + 1, hy + 1), local, after);
        assert!(
            (planar(old, new) - BLOCK * std::f32::consts::SQRT_2).abs() < 0.01,
            "a one-block-per-axis shift moves the render space by {:.3} m, not {:.3}",
            planar(old, new),
            BLOCK * std::f32::consts::SQRT_2
        );
    }

    // ---------------------------------------------------------------------------------------------
    // 2. Writer one: the local body
    // ---------------------------------------------------------------------------------------------

    /// Behaviour: world.portals.every-building-opening-leads-into-a-cell-of-its-own-block
    ///
    /// **`Character::place_parts` wrote in the block the window moved to.**
    ///
    /// Two-sided and absolute: the placement is inside the body's own bounding volume of where the
    /// **new** space puts it, and is a whole block shift away from where the **old** space would have.
    /// An unplaced part sits at the render origin and fails both halves, which a differential against
    /// the next frame cannot see.
    #[test]
    fn the_body_writer_is_on_the_far_side_of_the_recentre() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let Some((mut s, _, (hx, hy))) = settled(&store, &mut gpu) else {
            return;
        };

        let before = s.scene.viewer_block().expect("a viewer block");
        let target = block_at(hx + 1, hy);
        let land = Arc::clone(s.scene.character.as_ref().expect("a body").land());
        let z = land.ground_height(target, 96.0, 96.0).unwrap_or(0.0) + 1.0;
        let local = Vec3::new(96.0, 96.0, z);
        {
            let c = s.scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                target.cell(1),
                Frame::new(local, Quat::IDENTITY),
            ));
        }
        s.step(&mut gpu, 1.0);

        let after = s.scene.viewer_block().expect("a viewer block");
        let part = first_part(&s.scene);
        s.scene.release_textures(&mut gpu);

        let right = in_space_of((hx + 1, hy), local, after);
        let stale = in_space_of((hx + 1, hy), local, before);
        eprintln!(
            "body writer: viewer block {before:?} -> {after:?}; part[0] {part:?}; the new space \
         puts the body at {right:?} and the space `recenter` left at {stale:?}"
        );

        assert_ne!(
            before, after,
            "the window did not re-centre, so this frame is not a crossing"
        );
        assert!(
        planar(part, right) < 2.0,
        "part[0] is {:.3} m from where the block the window moved to puts the body ({right:?}); \
         it is at {part:?}",
        planar(part, right)
    );
        assert!(
        planar(part, stale) > BLOCK * 0.9,
        "part[0] is only {:.3} m from where the block the window LEFT puts the body ({stale:?}), \
         so the placement ran before `recenter` and the body is drawn a block behind the camera \
         for a frame",
        planar(part, stale)
    );
    }

    // ---------------------------------------------------------------------------------------------
    // 3. Writer two: the server's objects
    // ---------------------------------------------------------------------------------------------

    /// Behaviour: world.recentre.every-drawn-frame-writer-runs-after-the-recentre
    ///
    /// **`WorldScene::advance_objects` wrote in the block the window moved to.**
    ///
    /// The same claim about the *other* writer, and the one the compile-time token covers least well:
    /// its arm has to work in a scene with no body, so `recenter` can mint its token without a
    /// `Character`. This is the assertion that watches that seam.
    ///
    /// The prop never moves (no server update, no input), so its landblock-local position is a
    /// constant and every metre of change in its drawn frame is the render space moving under it.
    #[test]
    fn the_server_object_writer_is_on_the_far_side_of_the_recentre() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let Some((mut s, prop_at, (hx, hy))) = settled(&store, &mut gpu) else {
            return;
        };

        let before = s.scene.viewer_block().expect("a viewer block");
        let settled_frame = s
            .scene
            .server_object_frame(ObjectId(PROP))
            .expect("the prop is drawn");
        // The premise for this writer: the prop is where the *current* space puts it before anything
        // moves. Without this a prop that was never placed would satisfy the crossing assertion by
        // sitting at the origin in both spaces.
        assert!(
        planar(settled_frame.origin, in_space_of((hx, hy), prop_at, before)) < 0.01,
        "before any crossing the prop is at {:?}, not at {:?} where the home block puts it: it \
         was never placed, and nothing below would be about a re-centre",
        settled_frame.origin,
        in_space_of((hx, hy), prop_at, before)
    );

        let target = block_at(hx, hy + 1);
        {
            let land = Arc::clone(s.scene.character.as_ref().expect("a body").land());
            let z = land.ground_height(target, 96.0, 96.0).unwrap_or(0.0) + 1.0;
            let c = s.scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                target.cell(1),
                Frame::new(Vec3::new(96.0, 96.0, z), Quat::IDENTITY),
            ));
        }
        s.step(&mut gpu, 1.0);

        let after = s.scene.viewer_block().expect("a viewer block");
        let drawn = s
            .scene
            .server_object_frame(ObjectId(PROP))
            .expect("the prop is drawn")
            .origin;
        s.scene.release_textures(&mut gpu);

        let right = in_space_of((hx, hy), prop_at, after);
        let stale = in_space_of((hx, hy), prop_at, before);
        eprintln!(
            "object writer: viewer block {before:?} -> {after:?}; prop drawn at {drawn:?}; the \
         new space puts it at {right:?} and the space `recenter` left at {stale:?}"
        );

        assert_ne!(
            before, after,
            "the window did not re-centre, so this frame is not a crossing"
        );
        assert!(
            planar(drawn, right) < 0.01,
            "the prop is drawn at {drawn:?}; the block the window moved to puts it at {right:?}, \
         {:.3} m away",
            planar(drawn, right)
        );
        assert!(
        planar(drawn, stale) > BLOCK * 0.9,
        "the prop is only {:.3} m from where the block the window LEFT puts it ({stale:?}), so \
         `advance_objects` ran before `recenter`",
        planar(drawn, stale)
    );
    }
}
