//! A server object's particles are drawn at the object in the renderer's space, whichever
//! landblock the object stands in relative to the viewer's, and they stay at the object when the
//! viewer walks into another landblock, including particles a far emitter holds frozen.
//!
//! Fixture: the retail dats and a body in the world the runtime simulates with no device, out of
//! Holtburg. The object is a campfire (setup `0x020005AE`, whose default script hangs its flames
//! and smoke on the object itself rather than on a part), created one landblock east of the body
//! by an encoded `0xF745 Item_CreateObject` through the object stream. With no device nothing
//! steps particles, so each frame the test runs the particle update a drawn scene runs.

use std::sync::Arc;

use crate::common::sim::SimWorld;

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::{PhysicsDesc, PublicWeenieDesc};
use dereth_protocol::Message;
use {
    dereth_client_runtime::landblock::block_xy,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
};

/// The campfire's setup: all three of its default script's emitters hang on the object itself.
const CAMPFIRE: u32 = 0x0200_05AE;
const FIRE: ObjectId = ObjectId(0x7000_F1BE);
/// One landblock, and so the size of a particle drawn in the wrong landblock.
const BLOCK: f32 = 192.0;
/// How far from the campfire's own origin its flames and smoke may be drawn.
const NEAR: f32 = 8.0;
const STEP: f64 = 1.0 / 30.0;

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

/// Stand the body at `(x, y)` in `block`, on the ground.
fn stand_in(scene: &mut SimWorld, block: LandblockId, x: f32, y: f32) {
    let land = Arc::clone(scene.ws().character.as_ref().expect("a body").land());
    let z = land.ground_height(block, x, y).unwrap_or(0.0) + 1.0;
    let c = scene.ws_mut().character.as_mut().expect("a body");
    c.teleport(Position::new(
        block.cell(1),
        Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
    ));
    scene.ws_mut().follow_character_now();
}

/// Create the campfire at the middle of `block`, on the ground.
fn spawn_campfire(scene: &SimWorld, stream: &mut ObjectStream, block: LandblockId) {
    let land = Arc::clone(scene.ws().character.as_ref().expect("a body").land());
    let z = land.ground_height(block, 96.0, 96.0).unwrap_or(0.0);
    let payload = ObjectCreatePayload {
        id: FIRE,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(CAMPFIRE),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: block.cell(1).0,
                frame: dereth_protocol::types::Frame {
                    origin: Vec3::new(96.0, 96.0, z).into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(payload)).expect("encodes"),
        },
        LocalTime(0.0),
    );
}

/// One frame in `App::frame`'s order, then the particle step a drawn scene runs for the object,
/// drawn or degraded out.
fn frame(scene: &mut SimWorld, stream: &mut ObjectStream, t: &mut f64, draw_particles: bool) {
    *t += STEP;
    scene.sync_objects(stream).expect("object dispatch");
    #[allow(clippy::cast_possible_truncation)] // one frame's step
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(*t),
        STEP as f32,
    );
    scene.stream();
    if let Some(o) = scene.ws().objects.get(&FIRE) {
        let mut d = o.sim.driver.borrow_mut();
        d.cur_time = dereth_primitives::ServerTime(*t);
        d.update_particles(draw_particles);
    }
}

/// The campfire's drawn origin and every live particle it has, both in render space.
fn fire(scene: &SimWorld) -> (Vec3, Vec<Vec3>) {
    let o = scene
        .ws()
        .objects
        .get(&FIRE)
        .expect("the campfire is in the scene");
    let d = o.sim.driver.borrow();
    let particles = d
        .particles
        .iter()
        .flat_map(|e| e.live().map(|p| p.frame.origin))
        .collect();
    (o.frame.origin, particles)
}

/// Every particle within [`NEAR`] of the campfire horizontally; the message names the landblock
/// offset of the farthest.
fn assert_at_the_fire(scene: &SimWorld, when: &str) -> usize {
    let (at, particles) = fire(scene);
    assert!(
        !particles.is_empty(),
        "{when}: the campfire has live particles"
    );
    for p in &particles {
        let (dx, dy) = (p.x - at.x, p.y - at.y);
        assert!(
            dx.hypot(dy) < NEAR,
            "{when}: a particle is drawn at {p:?}, ({:.2}, {:.2}) landblocks from the campfire at \
             {at:?}",
            dx / BLOCK,
            dy / BLOCK,
        );
    }
    particles.len()
}

/// A body in Holtburg, a campfire one landblock east, and the frames it takes the campfire's
/// default script to light it.
fn lit_campfire_next_door(
    store: &Arc<RetailDatStore>,
) -> (SimWorld, ObjectStream, f64, (i32, i32)) {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = SimWorld::load(store, SceneConfig::default());
    let mut stream = ObjectStream::with_store(Arc::clone(store));
    let mut t = 0.0;
    stand_in(&mut scene, block_at(hx, hy), 170.0, 96.0);
    frame(&mut scene, &mut stream, &mut t, true);
    spawn_campfire(&scene, &mut stream, block_at(hx + 1, hy));
    for _ in 0..90 {
        frame(&mut scene, &mut stream, &mut t, true);
    }
    assert_eq!(
        scene.ws().viewer_block(),
        Some((hx, hy)),
        "the viewer is still in the home landblock, so the campfire stands in another one"
    );
    (scene, stream, t, (hx, hy))
}

/// Behaviour: rendering.effects.an-objects-own-emitters-burn-where-it-stands-in-any-landblock
///
/// **The campfire's flames and smoke are drawn at the campfire**, one landblock east of the
/// viewer's. An emitter hung on the object itself that followed the object's landblock-local
/// position rather than its placed frame draws them at the same local spot in the viewer's
/// landblock instead, exactly one landblock west of the campfire and at its height: fire with no
/// campfire under it.
#[test]
fn a_campfire_in_the_next_landblock_burns_where_it_stands() {
    let store = store();
    let (scene, _stream, _t, _) = lit_campfire_next_door(&store);
    let (at, _) = fire(&scene);
    assert!(
        at.x > BLOCK,
        "the campfire is drawn east of the viewer's landblock: {at:?}"
    );
    let n = assert_at_the_fire(&scene, "lit next door");
    eprintln!("campfire at {at:?}: {n} particles at the fire");
}

/// Behaviour: rendering.effects.an-objects-particles-stay-put-when-the-viewer-changes-landblock
///
/// **Walking into the campfire's landblock leaves its particles where they are in the world.**
/// The particles are born in the renderer's space, which moves a whole landblock when the viewer
/// crosses into the next one; a particle keeps its birth frame for life, and an emitter the
/// viewer is far from holds its particles frozen. Both halves are crossed here: frozen particles
/// straight after the crossing, and drawn ones once the emitter runs again.
#[test]
fn a_campfires_particles_stay_at_the_campfire_when_the_viewer_walks_into_its_landblock() {
    let store = store();
    let (mut scene, mut stream, mut t, (hx, hy)) = lit_campfire_next_door(&store);
    // Degraded out: the particles freeze where they are.
    for _ in 0..3 {
        frame(&mut scene, &mut stream, &mut t, false);
    }
    assert_at_the_fire(&scene, "frozen, before the crossing");
    stand_in(&mut scene, block_at(hx + 1, hy), 20.0, 96.0);
    frame(&mut scene, &mut stream, &mut t, false);
    assert_eq!(
        scene.ws().viewer_block(),
        Some((hx + 1, hy)),
        "the viewer crossed into the campfire's landblock, so the renderer's space moved"
    );
    let (at, _) = fire(&scene);
    assert!(
        at.x > 0.0 && at.x < BLOCK,
        "the campfire is now drawn inside the viewer's landblock: {at:?}"
    );
    assert_at_the_fire(&scene, "frozen, straight after the crossing");
    for _ in 0..30 {
        frame(&mut scene, &mut stream, &mut t, true);
    }
    assert_at_the_fire(&scene, "drawn again after the crossing");
}
