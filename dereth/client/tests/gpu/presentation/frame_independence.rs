//! Body animation and camera smoothing share the physics tick: a display frame on which the
//! physics gate stays shut draws exactly what the frame before it drew, at any refresh rate, and
//! a bodyless remote animation waits for the same tick while a position correction does not.
//! Fixture: the player's body on the retail dats' default landblock, run forward for ten seconds
//! at 60, 120, 144 and 165 Hz through the scene update and camera update; and `long-solo-play`'s
//! Sparring Golem replayed through `Corpus` into a scene with no body attached. A software device;
//! a machine without the retail dats fails.
//!
//! # The shared clock
//!
//! * The drawn part pose is a copy of the part's cell and frame (turned for billboards), with no
//!   time term: the client does not interpolate or extrapolate between physics ticks.
//! * Part poses are written only when the part array's frame is set, and the part-array
//!   animation update (with the sequence clock) runs only from dynamic position integration and
//!   static-object animation, both inside the tick.
//! * The camera smoother runs only from the player's physics update, inside the tick, and its
//!   `stiffness * dt * 10` lerp uses the tick's elapsed time, not the display frame's.
//!
//! So the body and the camera are on the same clock, in the same call, in that order. With no
//! changed input, a frame that does not tick re-runs the viewer's swept sphere from an unchanged
//! pivot toward an unchanged sought position and copies the unchanged part pose: a pure repeat,
//! with no relative body/camera sawtooth. Authoritative position corrections still take effect on
//! a frame whose animation clock stays frozen.
//!
//! # The assertion
//!
//! On every display frame the gate did not open, the drawn body and the drawn camera are
//! unchanged from the previous frame. The body is checked first, so a failure says which of the
//! two moved.

#![cfg(gpu)]
#![allow(clippy::pedantic)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
use dereth_protocol::{write_body, Opcode};
use {
    dereth_client_runtime::landblock::load_region, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

/// The four refresh rates in play. 60 and 120 are the two that sit *on* `MIN_QUANTUM` (two and
/// four frames respectively land on 1/30 s in exact arithmetic, so the accumulated clock decides
/// the gap frame by frame); 144 and 165 clear it every fifth and sixth frame with room to spare
/// and are perfectly regular.
const RATES: [f64; 4] = [60.0, 120.0, 144.0, 165.0];

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// One display frame: whether the gate opened, where the body was drawn and where the camera was.
struct Shot {
    ticked: bool,
    body: (f32, f32, f32),
    camera: (f32, f32, f32),
}

/// Ten seconds of held *run forward* at `hz`, through `App::frame`'s own order.
fn run(store: &Arc<RetailDatStore>, hz: f64) -> Vec<Shot> {
    let mut gpu = crate::common::test_gpu(320, 240);
    let mut scene = WorldScene::load(store, &mut gpu, SceneConfig::default()).expect("scene");
    let region = load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");

    let mut input = CharacterInput::default();
    let mut t = 0.0f64;
    #[allow(clippy::cast_possible_truncation)]
    let dtf = (1.0 / hz) as f32;
    // Settle: the body has to be standing on the real floor and the camera has to have finished
    // swinging out to its resting boom length before anything is measured.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let settle = hz as u32;
    for _ in 0..settle {
        t += 1.0 / hz;
        scene.update(Default::default(), input, LocalTime(t), dtf);
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            LocalTime(t),
            1.0 / hz,
        );
    }
    input.forward = true;
    input.run = true;

    let mut seen = scene
        .character
        .as_ref()
        .expect("the body")
        .stats
        .physics_ticks;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (hz * 10.0) as u32;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        t += 1.0 / hz;
        scene.update(Default::default(), input, LocalTime(t), dtf);
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            LocalTime(t),
            1.0 / hz,
        );
        let c = scene.character.as_ref().expect("the body");
        let ticks = c.stats.physics_ticks;
        let ticked = ticks != seen;
        seen = ticks;
        let b = c.render_frame().origin;
        let cam = scene.camera.position;
        out.push(Shot {
            ticked,
            body: (b.x, b.y, b.z),
            camera: (cam.x, cam.y, cam.z),
        });
    }
    out
}

/// Behaviour: presentation.frame.a-frame-with-no-physics-tick-draws-what-the-last-drew
///
/// A frame on which the gate stayed shut draws exactly what the frame before it drew.
#[test]
fn a_frame_that_did_not_tick_draws_exactly_what_the_frame_before_it_drew() {
    let store = store();
    for hz in RATES {
        let frames = run(&store, hz);
        let ticks = frames.iter().filter(|f| f.ticked).count();
        #[allow(clippy::cast_precision_loss)]
        let rate = ticks as f64 / 10.0;

        // Non-vacuity in both directions: the gate must have opened, and it must also have stayed
        // shut on some frames, or "unchanged on the frames that did not tick" is a claim about the
        // empty set.
        assert!(
            ticks > 100,
            "{hz} Hz: only {ticks} gate openings in ten seconds of running"
        );
        let quiet = frames.len() - ticks;
        assert!(
            quiet > 100,
            "{hz} Hz: only {quiet} of {} frames left the gate shut, so this station has nothing \
             to measure",
            frames.len()
        );

        // The body must actually have run, or every frame is trivially unchanged.
        let (a, z) = (frames[0].body, frames[frames.len() - 1].body);
        let (tx, ty) = (z.0 - a.0, z.1 - a.1);
        let travelled = tx.mul_add(tx, ty * ty).sqrt();
        assert!(
            travelled > 15.0,
            "{hz} Hz: the body must run, not {travelled:.2} m"
        );

        eprintln!(
            "frame independence @ {hz} Hz: {ticks} ticks in 10 s = {rate:.2} Hz effective, {quiet} quiet frames, \
             {travelled:.2} m travelled"
        );

        let mut body_moved = 0usize;
        let mut cam_moved = 0usize;
        let mut worst = 0.0f32;
        let mut first = String::new();
        for (i, w) in frames.windows(2).enumerate() {
            if w[1].ticked {
                continue;
            }
            let db = (
                w[1].body.0 - w[0].body.0,
                w[1].body.1 - w[0].body.1,
                w[1].body.2 - w[0].body.2,
            );
            let dc = (
                w[1].camera.0 - w[0].camera.0,
                w[1].camera.1 - w[0].camera.1,
                w[1].camera.2 - w[0].camera.2,
            );
            let bm = db.2.mul_add(db.2, db.0.mul_add(db.0, db.1 * db.1)).sqrt();
            let cm = dc.2.mul_add(dc.2, dc.0.mul_add(dc.0, dc.1 * dc.1)).sqrt();
            if bm > 0.0 {
                body_moved += 1;
            }
            if cm > 0.0 {
                cam_moved += 1;
                worst = worst.max(cm);
                if first.is_empty() {
                    first = format!(
                        "First at frame {i} -> {}, {cm:.5} m while the body sat still.",
                        i + 1
                    );
                }
            }
        }

        assert_eq!(
            body_moved, 0,
            "{hz} Hz: the drawn body moved on {body_moved} frames whose physics-tick gate \
             never opened -- part-array pose updates are reachable from \
             something that is not the tick"
        );
        assert_eq!(
            cam_moved, 0,
            "{hz} Hz: the camera moved on {cam_moved} of {quiet} frames on which the body did \
             not tick, worst {worst:.5} m. {first} Camera smoothing runs only in the player's \
             physics update, inside the physics tick, so the smoother cannot run on a frame \
             that did not tick, and the drawn body's offset from the drawn camera is constant \
             between ticks."
        );
    }
}

/// `long-solo-play`'s Sparring Golem, whose recorded approach drives the remote animation path.
/// It is deliberately observed without attaching a `Character`, so it has no physics adapter and
/// `WorldScene::advance_objects` owns its animation ladder.
const BODYLESS_MOVER: ObjectId = ObjectId(0x8000_09d2);
const BODYLESS_PLAYER: ObjectId = ObjectId(0x5000_000a);
const APPROACH_MICROS: u64 = 91_500_548;

/// A display-frame position correction for the bodyless object. This is constructed only to move
/// the already-recorded object a small distance; the animation and all of its assets remain the
/// capture's. Received-position handling runs outside the physics tick, so the correction must be
/// visible even when the shared tick gate stays shut.
fn position_correction(stream: &ObjectStream) -> SessionEvent {
    let p = stream
        .presence(BODYLESS_MOVER)
        .expect("recorded mover remains present");
    let mut at = p.position.expect("recorded mover is positioned");
    at.frame.origin.x += 0.125;
    let body = write_body(&MovementPositionEvent {
        id: BODYLESS_MOVER,
        position: PositionPack {
            origin: dereth_protocol::types::Origin {
                objcell_id: at.cell.0,
                origin: at.frame.origin.into(),
            },
            orientation: at.frame.rotation.into(),
            instance_timestamp: p.instance,
            position_timestamp: p.position_ts.wrapping_add(1),
            teleport_timestamp: p.teleport_ts,
            force_position_timestamp: p.force_position_ts,
            ..PositionPack::default()
        },
    })
    .expect("position correction encodes");
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_POSITION_EVENT,
        body,
    }
}

/// The outer physics gate covers static-object animation and every dynamic-object update; a
/// render-only object does not acquire a second display-rate clock because it has no physical
/// adapter. The fractional sequence frame is the clock itself, while the part frames are what the
/// draw consumes. A same-frame `Movement_PositionEvent 0xF748` is the control proving the gate
/// does not hold back the placement half of `advance_objects`.
#[test]
fn a_bodyless_remote_animation_uses_the_shared_tick_but_position_corrections_do_not() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let rows: Vec<_> = Corpus::shared("long-solo-play")
        .blobs
        .iter()
        .filter(|r| {
            r.dir == Direction::ServerToClient
                && r.payload.len() >= 8
                && [
                    Opcode::ITEM_CREATE_OBJECT.0,
                    Opcode::MOVEMENT_POSITION_EVENT.0,
                    Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0,
                ]
                .contains(&r.opcode)
                && [BODYLESS_PLAYER, BODYLESS_MOVER].contains(&ObjectId(u32::from_le_bytes(
                    r.payload[4..8].try_into().unwrap(),
                )))
                && r.t_rel_micros <= APPROACH_MICROS
        })
        .collect();
    let mut stream = ObjectStream::new();
    for row in rows.iter().filter(|r| r.t_rel_micros < APPROACH_MICROS) {
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
        );
    }
    let start = stream
        .presence(BODYLESS_MOVER)
        .and_then(|p| p.position)
        .expect("recorded bodyless mover");
    let block = start.cell.landblock();
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..Default::default()
        },
    )
    .expect("scene");
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("render-side production sync");

    const START: f64 = 91.5;
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START - 0.1),
        1.0 / 120.0,
    );
    let approach = rows.last().expect("recorded approach");
    assert_eq!(approach.t_rel_micros, APPROACH_MICROS);
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(approach.opcode),
            body: approach.payload[4..].to_vec(),
        },
        LocalTime(START),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("recorded approach syncs");
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START),
        1.0 / 120.0,
    );

    let before_pose = scene
        .server_object_sequence(BODYLESS_MOVER)
        .expect("animated mover");
    assert!(
        before_pose.has_anims(),
        "the recorded mover must carry an animation"
    );
    let before_parts = scene
        .server_object_part_frames(BODYLESS_MOVER)
        .expect("drawn parts");
    let before_position = scene
        .server_object_frame(BODYLESS_MOVER)
        .expect("drawn mover");

    // One 120 Hz display interval is below the 1/30 s outer gate. Neither the fractional
    // animation clock nor the part pose the draw consumes may advance.
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START + 1.0 / 120.0),
        1.0 / 120.0,
    );
    let quiet_pose = scene
        .server_object_sequence(BODYLESS_MOVER)
        .expect("animated mover");
    let quiet_parts = scene
        .server_object_part_frames(BODYLESS_MOVER)
        .expect("drawn parts");
    assert_eq!(
        quiet_pose.frame_number(),
        before_pose.frame_number(),
        "the bodyless ladder advanced on a display frame below the physics tick's gate"
    );
    assert_eq!(
        quiet_parts, before_parts,
        "part-array pose updates exposed a new animation pose on a no-tick display frame"
    );

    // The next display interval is quiet too. Deliver F748 before it: placement must still change
    // even though the animation clock stays frozen. Its root move necessarily moves the part
    // frames as well, so the fractional sequence is the animation-side observable on this arm.
    stream.apply_event(
        &position_correction(&stream),
        LocalTime(START + 2.0 / 120.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("position correction syncs");
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START + 2.0 / 120.0),
        1.0 / 120.0,
    );
    let corrected_pose = scene
        .server_object_sequence(BODYLESS_MOVER)
        .expect("animated mover");
    let corrected = scene
        .server_object_frame(BODYLESS_MOVER)
        .expect("drawn mover");
    assert_eq!(
        corrected_pose.frame_number(),
        quiet_pose.frame_number(),
        "the bodyless ladder advanced on the quiet frame that received F748"
    );
    assert_ne!(
        corrected.origin, before_position.origin,
        "gating the animation ladder must not gate a same-frame authoritative F748 placement"
    );

    // The residual reaches the shared gate after three more display intervals. The fractional
    // frame must then advance, so the quiet-frame assertions cannot pass on a frozen object.
    for frame in 3..=6 {
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(START + f64::from(frame) / 120.0),
            1.0 / 120.0,
        );
    }
    let ticked_pose = scene
        .server_object_sequence(BODYLESS_MOVER)
        .expect("animated mover");
    assert_ne!(
        ticked_pose.frame_number(),
        corrected_pose.frame_number(),
        "the bodyless animation never advanced when the shared gate opened"
    );
}
