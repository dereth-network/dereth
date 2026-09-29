//! On the update whose `recenter()` moves the viewer block, the local body's parts and the chase
//! camera come out in the space the window moved to. Retail's part update writes the object's
//! cell-local frame and applies the viewer-block origin at draw time; our part frames carry that
//! origin (so vertices stay block-local), so anything writing a part frame or a camera must run
//! after the re-centre. Parts placed first would sit `(-192 * dx, -192 * dy, 0)` away for one
//! frame. The picture of the same crossing, which needs a device, is the `gpu` tier's
//! `world::landblock_recentre`. Fixture: the retail dats and a body, in the world the runtime
//! simulates with no device, crossings out of Holtburg. Fails when the retail dats are absent.

use std::sync::Arc;

use crate::common::sim::SimWorld;

use dereth_client::character::CharacterInput;
use dereth_client::world::{block_xy, SceneConfig, DEFAULT_LANDBLOCK};
use dereth_dat::RetailDatStore;
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};

/// `BLOCK_LENGTH`: one landblock, and therefore the exact size of a part misplacement.
const BLOCK: f32 = 192.0;

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
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

/// One scene `update` and nothing else, **no clock**. Every station in this module is at
/// `LocalTime(0.0)` with `dt = 0.0`, so two updates at one station hold the same pose by
/// construction and a difference between them cannot be about the idle cycle.
fn tick(scene: &mut SimWorld) {
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
}

/// Stand the body in the middle of `block`. The teleport itself is not the subject: it is how the
/// body is placed *before* the crossing under test.
fn stand_in(scene: &mut SimWorld, block: LandblockId) {
    let land = Arc::clone(scene.ws().character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
    let c = scene.ws_mut().character.as_mut().expect("a body");
    c.teleport(Position::new(
        block.cell(1),
        Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
    ));
}

/// Where the part update last put the body's first part, in the renderer's
/// viewer-block-relative space.
fn first_part(scene: &SimWorld) -> Vec3 {
    let c = scene.ws().character.as_ref().expect("a body");
    let d = c.driver();
    d.part_array
        .parts
        .first()
        .map_or(Vec3::ZERO, |p| p.pos.origin)
}

/// The body's own render frame.
fn body(scene: &SimWorld) -> Vec3 {
    scene
        .ws()
        .character
        .as_ref()
        .expect("a body")
        .render_frame()
        .origin
}

fn cam_bits(scene: &SimWorld) -> [u32; 3] {
    let p = scene.ws().camera.position;
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// One crossing: where the body and the camera were placed on the crossing update and on the next.
struct Crossing {
    /// The viewer block before and after the crossing update.
    viewer_block: ((i32, i32), (i32, i32)),
    /// The body's first part on the crossing update and on the settled one.
    part0: (Vec3, Vec3),
    /// The body's own render frame on the same two updates, so that "the part was placed at all"
    /// can be told from "the part was placed in the same wrong space twice".
    body: (Vec3, Vec3),
    /// The camera's bits on the crossing update and on the settled one.
    camera: ([u32; 3], [u32; 3]),
}

/// Walk the body from the home block into the block `(dx, dy)` away and read the update the
/// crossing happens on.
fn cross(store: &Arc<RetailDatStore>, (dx, dy): (i32, i32)) -> Crossing {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = SimWorld::load(store, SceneConfig::default());
    stand_in(&mut scene, block_at(hx, hy));
    scene.ws_mut().follow_character_now();
    tick(&mut scene);
    scene.stream();

    // --- the crossing. The teleport moves the body into the next block and `follow_character_now`
    // puts the camera behind it there; neither re-centres the window. The single scene `update`
    // below is the one under test: it is the one whose `recenter()` moves the viewer.
    let before_block = scene.ws().viewer_block().expect("a viewer block");
    stand_in(&mut scene, block_at(hx + dx, hy + dy));
    scene.ws_mut().follow_character_now();

    tick(&mut scene);
    scene.stream();
    let after_block = scene.ws().viewer_block().expect("a viewer block");
    let part0_crossing = first_part(&scene);
    let body_crossing = body(&scene);
    let cam_crossing = cam_bits(&scene);

    // --- one further update at the same station. No teleport, no input, no clock. The only thing
    // that can change is the body's own placement and the camera derived from it.
    tick(&mut scene);
    scene.stream();
    Crossing {
        viewer_block: (before_block, after_block),
        part0: (part0_crossing, first_part(&scene)),
        body: (body_crossing, body(&scene)),
        camera: (cam_crossing, cam_bits(&scene)),
    }
}

/// The four crossings measured: one block on each axis, both together, and a four-block hop.
/// The last is there because a camera patched by subtracting the shift loses low bits as a
/// function of the *size* of the shift, and at one block that subtraction happens to be exact.
const CROSSINGS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (4, 4)];

/// Behaviour: world.streaming.the-crossing-frame-draws-body-and-camera-in-the-new-block
///
/// **The body's parts and the chase camera come out of the crossing frame in the space the
/// re-centre moved to, not the one it left.**
///
/// Both are asserted against the *next* update at the same station rather than against a written
/// constant: one further scene `update` re-derives both from the same inputs, so equality with it
/// is exactly the statement "the crossing frame's placement was already settled". A constant would
/// have to be re-derived here and could agree with a wrong answer.
///
/// **The two halves cover the two wrong orderings.** Parts placed before the re-centre differ by
/// exactly `(-192 * dx, -192 * dy, 0)`; a camera shifted by subtraction differs only on a shift of
/// more than one block, because the subtraction is exact at one and loses three low bits of `f32`
/// at four.
#[test]
fn a_crossing_frame_places_the_body_and_the_camera_in_the_block_it_moved_to() {
    let store = store();

    let mut checked = 0usize;
    for legs in CROSSINGS {
        let c = cross(&store, legs);

        eprintln!(
            "crossing {legs:?}: viewer block {:?} -> {:?}; part[0] {:?} -> {:?}; camera \
             {:08x?} -> {:08x?}",
            c.viewer_block.0, c.viewer_block.1, c.part0.0, c.part0.1, c.camera.0, c.camera.1
        );

        // The premise: this update really did re-centre. Without it the two updates below would be
        // identical because nothing happened, allowing a broken setup to pass.
        assert_eq!(
            (
                c.viewer_block.1 .0 - c.viewer_block.0 .0,
                c.viewer_block.1 .1 - c.viewer_block.0 .1
            ),
            legs,
            "the window did not re-centre by the block the body crossed into, so nothing below is \
             a statement about a crossing"
        );

        // **The other premise: the parts were placed at all.** Both assertions below compare the
        // crossing update against the settled one, and a body whose parts were **never placed**
        // satisfies both of them: `part[0]` is a default `Frame` on both updates, so they are
        // equal. A placed part sits inside the body's own bounding volume; an unplaced one sits
        // at the render space's origin, which at these stations is 96 m away on each axis.
        for (which, part, body) in [
            ("crossing", c.part0.0, c.body.0),
            ("settled", c.part0.1, c.body.1),
        ] {
            assert!(
                (part.x - body.x).abs() < 2.0
                    && (part.y - body.y).abs() < 2.0
                    && (part.z - body.z).abs() < 3.0,
                "the {which} update's part[0] is at {part:?} and the body at {body:?}: the parts \
                 were not placed at all, so nothing below is a statement about *where* they were \
                 placed"
            );
        }

        // The parts. An offset of a whole number of landblocks is placement before the re-centre;
        // the assertion is equality, so any offset at all fails it.
        assert_eq!(
            c.part0.0,
            c.part0.1,
            "the crossing update's part placement differs from the settled one by {:?}, i.e. \
             ({}, {}) landblocks -- `update_parts` ran on the wrong side of `recenter()`",
            (
                c.part0.1.x - c.part0.0.x,
                c.part0.1.y - c.part0.0.y,
                c.part0.1.z - c.part0.0.z
            ),
            (c.part0.1.x - c.part0.0.x) / BLOCK,
            (c.part0.1.y - c.part0.0.y) / BLOCK,
        );

        // The camera, **bit for bit**. A tolerance here would pass over exactly what the
        // four-block leg exists to catch: a patched camera is wrong by three low bits of `f32`,
        // which is far below any tolerance anyone would write and is worth thousands of pixels.
        assert_eq!(
            c.camera.0, c.camera.1,
            "the crossing update's camera differs from the settled one in its bits, so it was \
             patched by subtracting the block shift rather than re-derived after `recenter()`"
        );
        checked += 1;
    }
    assert_eq!(checked, CROSSINGS.len(), "every crossing was measured");
}
