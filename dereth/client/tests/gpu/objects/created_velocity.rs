//! An object created with a velocity in its `PhysicsDesc` (a spell projectile) moves: the client
//! integrates it every frame, with or without a motion table. Every non-static create is active
//! from the moment it is created, whether or not its descriptor declares a velocity and whether or
//! not it has gravity.
//!
//! # 1. The create applies the descriptor's velocity
//!
//! Applying the physics description to a new body ends with these steps:
//!
//! ```text
//! copy all three descriptor velocity components into a vector;
//! set the body's velocity, with activation enabled;
//! assign all three angular-velocity components directly;
//! // The angular-velocity assignment does not itself activate the body.
//! ```
//!
//! This is **unconditional**: no stamp gate or non-zero test. `Item_UpdateObject 0xF7DB` destroys
//! and recreates the object, so it applies the description the same way. A duplicate create that
//! merges into an existing object instead sends descriptor velocity, omega and `VECTOR_TS` through
//! the same gated vector update as `Movement_VectorUpdate 0xF74E`.
//!
//! # 2. Something advances it every frame
//!
//! The physics tick visits **every** maintained object, with no "is it animated" condition. Each
//! object's update returns early unless `transient_state & ACTIVE_TS`, which setting the velocity
//! has just enabled. Active objects step through the fixed-quantum ladder to the integrator:
//! `origin += v*dt + 0.5*a*dt²`.
//!
//! # 3. Why every create is active
//!
//! Two steps of the create each activate the new body on their own. First, the body enters the
//! world at its create position, and a successful placement sets `ACTIVE_TS` (`0x80`) on any
//! body that is not `STATIC_PS`. Then applying the create's physics description ends by
//! assigning the descriptor's velocity, which decodes as zero when the velocity flag is clear.
//! Velocity assignment compares the vector first: a changed vector is stored, its speed clamped
//! to 50 m/s and the object marked as having jumped this frame; an unchanged vector skips those
//! steps but still reaches activation, which again leaves `STATIC_PS` bodies alone. The first is
//! sampled here before the velocity is applied (`placed_active`); the second is the physics
//! crate's `an_unchanged_velocity_still_activates_a_sleeping_body`. Either alone keeps the body
//! active from birth, so a test that sampled only after both could not see one of them go.
//!
//! # 4. Why the fall does not show it
//!
//! The outer per-object update refuses only a parented object, an object with no cell and a
//! `FROZEN_PS` (`0x1000000`) object; the activity gate is in the inner update. Before that gate the
//! outer sweep activates eligible objects within 96 m and clears `ACTIVE_TS` (mask `0xFFFFFF7F`)
//! beyond it, every tick. So activation at creation only differs from activation at the first tick
//! in the interval before that tick: the fall is the same either way, and the `born_active`
//! assertion is what pins creation-time activation. The gravity-free case pins that an activated
//! object with nothing to integrate stays put.
//!
//! Fixture: long-solo-play's remote mover create supplies the landblock and a setup with real
//! geometry; the bolt's velocity, state and missing motion table, and the dropped item's state and
//! missing velocity flag, are constructed, in a live `WorldScene` with a physics owner attached.
//! The census reads every recorded session's creates.
#![cfg(gpu)]

use dereth_physics::math::V3 as _;
use dereth_physics::{PhysicsState, TransientState};
use dereth_scene::world_scene::SceneWrites;

use super::common::{retail_store, test_gpu};

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
use dereth_protocol::{Message, Opcode};
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The bolt. A guid in the non-player range, like every server-created object.
const BOLT: ObjectId = ObjectId(0x8000_0F27);

/// 15 m/s, straight and level. Well under the 50 m/s speed cap, so nothing is clamped, and far
/// enough per 1/30 s step (0.5 m) that a stationary object cannot be mistaken for a moving one.
const SPEED: f32 = 15.0;

/// The dropped item. A guid in the non-player range.
const ITEM: ObjectId = ObjectId(0x8000_0F51);

/// The commonest non-missile state word among the corpus's velocity-carrying creates, `0x10418`:
/// `HAS_PHYSICS_BSP | GRAVITY | IGNORE_COLLISIONS | REPORT_COLLISIONS`. A dropped or thrown item
/// that has gravity.
const GRAVITY_ITEM_STATE: u32 = 0x0001_0418;

/// A recorded create, used only for the landblock and a setup id that has real geometry. The
/// velocity, the state word and the *absence* of a motion table are this test's.
fn recorded_create() -> ObjectCreatePayload {
    let corpus = Corpus::load("long-solo-play")
        .expect("locked corpus decodes")
        .expect("long-solo-play");
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && r.payload.get(4..8) == Some(&0x8000_09D2_u32.to_le_bytes())
        })
        .expect("long-solo-play carries the remote mover's create");
    ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("decodes")
        .0
}

/// A projectile's descriptor: a setup, a position, a velocity, and **no motion table**, which is
/// what a spell bolt is.
fn bolt_create(at: Position, velocity: Vec3) -> ObjectCreatePayload {
    let recorded = recorded_create();
    ObjectCreatePayload {
        id: BOLT,
        objdesc: recorded.objdesc,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP | flags::VELOCITY,
            setup_id: recorded.physicsdesc.setup_id,
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            velocity: Some(velocity.into()),
            // Zero is `STATIC_PS` clear and `GRAVITY_PS` clear: a straight bolt that physics is
            // allowed to activate, matching straight flight without gravity.
            state: 0,
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    }
}

/// A live scene around `at`'s landblock with a physics owner attached, and the observer placed
/// `observer` metres from `at` in its loaded block. A `character: false` scene has no physics world
/// until a character is attached, so without one no integrated position can be read.
fn physics_scene(
    store: &std::sync::Arc<RetailDatStore>,
    gpu: &mut Gpu,
    at: Position,
    observer: Vec3,
) -> WorldScene {
    let block = at.cell.landblock();
    let mut scene = WorldScene::load(
        store,
        gpu,
        SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..SceneConfig::default()
        },
    )
    .expect("the recorded block loads");
    let region = dereth_world_data::landblock::load_region(store).expect("the region");
    scene
        .attach_character(store, &region, gpu)
        .expect("a physics owner");
    let mut place = at;
    place.frame.origin = place.frame.origin.add(observer);
    let character = scene.character.as_mut().expect("attached");
    character.land().load_block_cells(block);
    character.teleport(place);
    scene
}

fn feed(stream: &mut ObjectStream, p: ObjectCreatePayload, now: f64) {
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(p)).expect("encodes"),
        },
        LocalTime(now),
    );
}

/// Behaviour: objects.create.a-create-carrying-a-velocity-moves
///
/// **The object-stream half.** The create's own velocity leaves the object stream for the body,
/// as applying the physics description does.
#[test]
fn a_create_carrying_a_velocity_parks_it_for_the_body() {
    let mut stream = ObjectStream::new();
    let recorded = recorded_create();
    let at: Position = recorded
        .physicsdesc
        .position
        .expect("the recorded create is placed")
        .into();
    let v = Vec3::new(SPEED, 0.0, 0.0);
    feed(&mut stream, bolt_create(at, v), 0.0);

    assert_eq!(
        stream.take_vector_update(BOLT),
        Some((v, Vec3::ZERO)),
        "applying the physics description ends by setting the descriptor's velocity with \
         activation and assigning the omega; a create whose velocity nothing reads is a bolt that \
         never leaves the caster"
    );
}

/// Behaviour: objects.create.a-create-carrying-a-velocity-moves
///
/// **The acceptance half: a position that changes across simulated time.**
///
/// Per-object physics advancement reaches the integrator without consulting a motion table. The
/// constructed spell bolt deliberately has none.
#[test]
fn a_projectile_created_with_a_velocity_travels() {
    let store = retail_store();
    let mut gpu = test_gpu(320, 240);

    let recorded = recorded_create();
    let mut at: Position = recorded.physicsdesc.position.expect("placed").into();
    // Five metres up, so the bolt flies through air rather than into the terrain it was standing
    // on. This is the test avoiding an unrelated collision, not the client avoiding one.
    at.frame.origin.z += 5.0;
    let mut scene = physics_scene(&store, &mut gpu, at, Vec3::new(3.0, 0.0, 0.0));

    let mut stream = ObjectStream::new();
    let v = Vec3::new(SPEED, 0.0, 0.0);
    let mut now = 1000.0_f64;
    feed(&mut stream, bolt_create(at, v), now);
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the create reaches the scene");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics(&store, &mut c.world);
    }
    // The drain runs before the body exists on the create frame, so it runs again once it does;
    // the client instead applies the description to a body object creation has already made.
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the drain after the body exists");

    let start = scene
        .server_object_position(BOLT)
        .expect("the bolt is in the scene");
    assert_eq!(
        start.frame.origin, at.frame.origin,
        "it starts where the server put it"
    );

    // Half a second of simulated time at the frame rate `advance_objects` and `use_time` share.
    for _ in 0..15 {
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }

    let end = scene
        .server_object_position(BOLT)
        .expect("the bolt is still in the scene");
    let travelled = end.frame.origin.sub(start.frame.origin);
    let distance = travelled.mag2().sqrt();
    assert!(
        distance > 1.0,
        "the bolt has not moved: it was created at {:?} with velocity {:?} and half a second \
         later it is at {:?} ({:.3} m). The physics integrator integrates \
         `origin += v*dt` for every active object swept by the physics tick, and \
         nothing in that loop asks whether the object has a motion table",
        start.frame.origin,
        v,
        end.frame.origin,
        distance
    );
    assert!(
        travelled.x > 1.0 && travelled.y.abs() < 1.0,
        "and it travels along the velocity it was given, not somewhere else: {travelled:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Active from birth.
// ---------------------------------------------------------------------------------------------

/// A create with **no `VELOCITY` flag**: the client still assigns it zero velocity, which activates
/// it.
fn silent_create(at: Position, state: u32) -> ObjectCreatePayload {
    let recorded = recorded_create();
    ObjectCreatePayload {
        id: ITEM,
        objdesc: recorded.objdesc,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: recorded.physicsdesc.setup_id,
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            velocity: None,
            state,
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    }
}

struct Outcome {
    /// `transient_state` once the body has entered the world, before the descriptor's velocity
    /// is applied to it.
    placed_active: bool,
    /// `transient_state` on the frame after the create, before any tick.
    born_active: bool,
    /// `transient_state` after `frames` ticks.
    active: bool,
    /// How far the object moved over `frames`.
    moved: Vec3,
}

fn drop_an_item(up: f32, state: u32, frames: usize) -> Outcome {
    let store = retail_store();
    let mut gpu = test_gpu(320, 240);

    let recorded = recorded_create();
    let mut at: Position = recorded.physicsdesc.position.expect("placed").into();
    at.frame.origin.z += up;
    let mut scene = physics_scene(&store, &mut gpu, at, Vec3::new(0.0, 8.0, 0.0));

    let mut stream = ObjectStream::new();
    let mut now = 1000.0_f64;
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(silent_create(at, state)))
                .expect("encodes"),
        },
        LocalTime(now),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the create reaches the scene");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics(&store, &mut c.world);
    }
    let placed_active = transient(&scene).is_some_and(TransientState::is_active);
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the drain after the body exists");

    let start = scene
        .server_object_position(ITEM)
        .expect("the item is in the scene");
    let born_active = transient(&scene).is_some_and(TransientState::is_active);

    for _ in 0..frames {
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }

    let end = scene
        .server_object_position(ITEM)
        .expect("the item is still in the scene");
    Outcome {
        placed_active,
        born_active,
        active: transient(&scene).is_some_and(TransientState::is_active),
        moved: end.frame.origin.sub(start.frame.origin),
    }
}

fn transient(scene: &WorldScene) -> Option<TransientState> {
    let c = scene.character.as_ref()?;
    let h = c.world.by_object_id(ITEM)?;
    Some(c.world.get(h)?.transient_state)
}

/// Behaviour: objects.create.a-create-is-active-from-birth
///
/// **A create with `GRAVITY_PS` and no declared velocity, five metres up, is active before the
/// first tick, and already as soon as its body has entered the world.**
///
/// Entering the world at the create position activates the body (`placed_active`, sampled before
/// the descriptor's velocity is applied); the zero velocity the description then assigns keeps it
/// so (`born_active`). Removing the placement's activation fails the first; activation deferred to
/// the first tick fails both.
#[test]
fn a_gravity_carrying_create_is_active_from_birth() {
    let o = drop_an_item(5.0, GRAVITY_ITEM_STATE, 60);
    eprintln!(
        "gravity create: placed_active={} born_active={} active_after={} moved={:?}",
        o.placed_active, o.born_active, o.active, o.moved
    );
    assert!(
        o.placed_active,
        "a successful placement at the create position sets ACTIVE_TS on a body that is not static"
    );
    assert!(
        o.born_active,
        "and applying the physics description, whose velocity assignment activates even an \
         unchanged zero vector, leaves it active"
    );
}

/// **The item falls the 5 m it was raised, straight down, and stays active.**
///
/// It lands at `z = -4.9990005` relative to its start, on the terrain below. The fall does not
/// depend on creation-time activation, because the outer update reactivates eligible objects inside
/// 96 m on every sweep before the inner integration gate; the separate `born_active` assertion
/// covers the interval before the first tick.
#[test]
fn an_item_created_with_gravity_falls_straight_down_and_stays_active() {
    let o = drop_an_item(5.0, GRAVITY_ITEM_STATE, 60);
    assert!(
        (o.moved.z + 5.0).abs() < 0.05,
        "it falls the 5.0 m it was raised and stops on the ground: {:?}",
        o.moved
    );
    assert_eq!(o.moved.x, 0.0, "and straight down");
    assert_eq!(o.moved.y, 0.0);
    assert!(
        o.active,
        "still active: the per-object update re-arms it every sweep regardless"
    );
}

/// Behaviour: objects.create.a-create-is-active-from-birth
///
/// **A create with no gravity is active from birth and does not move at all.**
///
/// Acceleration is zero when `state & GRAVITY_PS` is clear, and the integrator's first test is
/// `v² <= 0`. So waking this zero-velocity, gravity-free object costs one pass through an
/// integrator with nothing to integrate: activating every create is safe for objects that are not
/// falling items.
#[test]
fn a_gravity_free_create_is_active_from_birth_and_does_not_move() {
    let o = drop_an_item(5.0, GRAVITY_ITEM_STATE & !PhysicsState::GRAVITY_PS, 60);
    eprintln!(
        "gravity-free create: born_active={} active_after={} moved={:?}",
        o.born_active, o.active, o.moved
    );
    assert!(o.born_active, "activation does not depend on gravity");
    assert_eq!(
        o.moved,
        Vec3::ZERO,
        "and it goes nowhere: acceleration is the zero vector and `v² <= 0` takes the arm that \
         does not integrate"
    );
}

/// **Every velocity-carrying create the shard sent has a non-zero velocity, and most have no motion
/// table.**
///
/// Every recorded `Item_CreateObject 0xF745` / `Item_UpdateObject 0xF7DB`, counted by whether its
/// `PhysicsDesc` carries `VELOCITY` and, for those that do, whether the vector is non-zero and
/// whether they also carry a motion table. Most carry none, so a client that integrated only
/// objects with a motion table would leave most of them where they were created.
#[test]
fn every_recorded_create_carrying_a_velocity_has_a_nonzero_one_and_most_have_no_motion_table() {
    let mut with = 0usize;
    let mut nonzero = 0usize;
    let mut tableless = 0usize;
    for c in Corpus::shared_all() {
        for row in c.blobs.iter().filter(|r| {
            r.dir == Direction::ServerToClient
                && (r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                    || r.opcode == Opcode::ITEM_UPDATE_OBJECT.0)
        }) {
            let Ok(m) =
                ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            else {
                continue;
            };
            let d = &m.0.physicsdesc;
            if d.bitfield & flags::VELOCITY == 0 {
                continue;
            }
            with += 1;
            if d.velocity.is_some_and(|v| Vec3::from(v) != Vec3::ZERO) {
                nonzero += 1;
                if d.mtable_id.is_none_or(|t| t == 0) {
                    tableless += 1;
                }
            }
        }
    }
    assert!(
        with > 0,
        "the corpus carries creates whose descriptor declares a velocity"
    );
    assert_eq!(
        nonzero, with,
        "and not one of them is a zero vector dressed up as a flag"
    );
    assert!(
        tableless * 2 > with,
        "most of them also have no motion table, so integrating only objects with a motion table \
         would publish a position for few of them: {tableless} of {with} have none"
    );
}
