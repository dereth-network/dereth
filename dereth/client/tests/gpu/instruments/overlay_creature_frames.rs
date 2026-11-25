//! A creature a world's overlay adds, drawn in the retail look as a server would move it: standing,
//! walking, swinging in its combat stance, taking a hit and dying, a camera following it. It is
//! drawn twice, once with its skinned body and once with its parts' own models, and each way is
//! written as one sheet: a row of moments per subject. It asserts nothing.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_CREATURE_OVERLAY`: the overlay folder (required), and `DERETH_CREATURE_WORLD` the
//!   world it was written for (default `ac2-creatures`).
//! - `DERETH_CREATURE_SETUP`, `DERETH_CREATURE_MTABLE`: the creature's setup and motion table
//!   (hex, required).
//! - `DERETH_CREATURE_DUMP`: where the sheets go, `skinned.png` and `parts.png` (required).
//! - `DERETH_CREATURE_SUBJECTS`: a comma list of `idle`, `walk`, `run`, `turn`, `attack`, `engage`,
//!   `track`, `hit` and `die` (default `idle,walk,attack,hit,die`). `engage` starts at peace, takes
//!   the combat stance, and swings a second later, as a server has a creature engage; `track`
//!   turns the creature to face a target that circles it, as a server turns a creature to its
//!   target, and prints how far it faces off the target at each moment.
//! - `DERETH_CREATURE_TARGET`: a drudge standing in front of the creature, this many meters from
//!   its edge (its setup's radius at its scale), as a server's melee range leaves its target.
//! - `DERETH_CREATURE_TARGET_BEARING`, `DERETH_CREATURE_TARGET_SPIN`: for `track`, where the
//!   target starts, degrees to the creature's right of its front (default 120), and how fast it
//!   circles the creature, degrees a second to its right (default 90).
//! - `DERETH_CREATURE_ATTACKS`: the attacks swung, a comma list of motion names (default
//!   `AttackHigh1`).
//! - `DERETH_CREATURE_MOMENTS`: moments a row (default 6).
//! - `DERETH_CREATURE_DISTANCE`, `DERETH_CREATURE_AIM`: the camera's distance and the height it
//!   looks at, meters (default 14 and 2.5); `DERETH_CREATURE_VIEW`: its bearing from the
//!   creature's front, degrees (default 60).
//! - `DERETH_CREATURE_SCALE`: its size, as its weenie's scale gives it (default 1).
//! - `DERETH_CREATURE_SWAPS`: a look, as its weenie's texture swaps on part 0: a comma list of
//!   `old>new` texture ids (hex); `DERETH_CREATURE_TRANSLUCENCY` its weenie's translucency.
//! - `DERETH_CREATURE_AT`: where it stands, `block:x,y` (default open ground south of Holtburg).
//! - `DERETH_CREATURE_WAYS`: `skinned`, `parts` or both (default both).
//! - `DERETH_CREATURE_SEQUENCE`: `1` also writes every frame of each way as
//!   `<way>/NNNN.png`, for a video.
//! - `DERETH_CREATURE_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_CREATURE_LOOK`: `classic` (the default) or `remaster`, the look it is drawn in.

#![cfg(gpu)]

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_animation::MotionCommand as C;
use dereth_dat::overlay::OverlayDir;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, ObjectId, Vec3};
use dereth_protocol::movement::{
    movement_type, InterpretedMotionState, MotionAction, MovementBody, MovementBuffer,
    MovementSetObjectMovement,
};
use dereth_protocol::Opcode;

use super::stations::{self, Clock, Place, Shot, Station};

const ID: u32 = 0x8300_5000;
/// The person the creature faces, when there is one.
const TARGET: u32 = 0x8300_5001;
/// The target's setup and motion table: a drudge's, a body the size of a person's.
const TARGET_BODY: (u32, u32) = (0x0200_07DD, 0x0900_0008);
/// How long after taking the combat stance an engaging creature swings, seconds.
const ENGAGE_AT: f64 = 1.0;
/// How often a tracking creature is told again to turn to its target, seconds.
const RETURN_EVERY: f64 = 0.5;
const FPS: f64 = 30.0;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn hex(v: &str) -> u32 {
    u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a hex id")
}

fn number(name: &str, default: f32) -> f32 {
    env(name).map_or(default, |v| v.trim().parse().expect("a number"))
}

fn index(c: C) -> u16 {
    c.to_index().expect("a command the wire carries")
}

fn size() -> (u32, u32) {
    env("DERETH_CREATURE_SIZE").map_or((960, 540), |v| {
        let (w, h) = v.split_once('x').expect("WIDTHxHEIGHT");
        (w.parse().expect("a width"), h.parse().expect("a height"))
    })
}

/// What a server sends: the stance, the forward motion, and any actions begun.
fn body(style: C, forward: C, actions: &[(C, u16)]) -> MovementBody {
    MovementBody {
        movement_type: movement_type::INVALID,
        motion_flags: 0,
        current_style: index(style),
        interpreted: Some(InterpretedMotionState {
            current_style: Some(index(style)),
            forward_command: (forward != C::READY).then(|| index(forward)),
            actions: actions
                .iter()
                .map(|(c, stamp)| MotionAction {
                    command_index: index(*c),
                    stamp_and_autonomy: *stamp & 0x7FFF,
                    speed: 1.0,
                })
                .collect(),
            ..InterpretedMotionState::default()
        }),
        ..MovementBody::default()
    }
}

/// Send the creature `body`, as `0xF74C Movement_SetObjectMovement`.
fn send(shot: &mut Shot, stamp: &mut u16, body: MovementBody) {
    send_to(shot, ID, stamp, body);
}

/// Send `id` `body`, as `0xF74C Movement_SetObjectMovement`.
fn send_to(shot: &mut Shot, id: u32, stamp: &mut u16, body: MovementBody) {
    *stamp = stamp.wrapping_add(1);
    let movement = MovementSetObjectMovement::encode_movement(&MovementBuffer {
        movement_timestamp: *stamp,
        server_control_timestamp: *stamp,
        autonomous: false,
        body,
    })
    .expect("the movement encodes");
    let message = MovementSetObjectMovement {
        id: ObjectId(id),
        instance_sequence: 1,
        movement,
    };
    shot.send(
        Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        dereth_protocol::write_body(&message).expect("encode"),
    );
}

/// Create the creature as a server would: standing on the ground at block-local `(x, y)` of
/// `block`, turned `heading` degrees, at `scale`, falling as creatures do.
fn create(
    shot: &mut Shot,
    id: u32,
    (setup, mtable): (u32, u32),
    block: u16,
    (x, y, ground): (f32, f32, f32),
    heading: f32,
    scale: f32,
) {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    /// The physics state a server gives a creature: it reports collisions and falls.
    const CREATURE_STATE: u32 = 0x0000_0408;
    let r = heading.to_radians();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a block-local position, 0..192, to its outdoor cell's index
    let cell = (u32::from(block) << 16) | ((x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1);
    // The creature's look, as its weenie gives it.
    let look = id == ID;
    let objdesc = ObjDesc {
        texture_changes: env("DERETH_CREATURE_SWAPS")
            .filter(|_| look)
            .map(|v| {
                v.split(',')
                    .map(|pair| {
                        let (old, new) = pair.split_once('>').expect("old>new");
                        dereth_protocol::types::TextureMapChange {
                            part_index: 0,
                            old_tex_id: hex(old),
                            new_tex_id: hex(new),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ..ObjDesc::default()
    };
    let translucency = env("DERETH_CREATURE_TRANSLUCENCY")
        .filter(|_| look)
        .map(|v| v.trim().parse::<f32>().expect("a number"));
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: ObjectId(id),
        objdesc,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION
                | flags::SETUP
                | flags::MTABLE
                | flags::OBJSCALE
                | if translucency.is_some() {
                    flags::TRANSLUCENCY
                } else {
                    0
                },
            translucency,
            setup_id: Some(setup),
            mtable_id: Some(mtable),
            object_scale: Some(scale),
            state: CREATURE_STATE,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: cell,
                frame: dereth_protocol::types::Frame {
                    origin: Vec3::new(x, y, ground).into(),
                    orientation: dereth_primitives::Quat::new(
                        math::cosf(r * 0.5),
                        0.0,
                        0.0,
                        math::sinf(r * 0.5),
                    )
                    .into(),
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    shot.send(
        Opcode::ITEM_CREATE_OBJECT,
        dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
            .expect("encode"),
    );
}

/// Tell the creature to turn to face the target, as a server does: `TurnToObject` in its
/// stance.
fn turn_to_target(shot: &mut Shot, stamp: &mut u16, style: C) {
    use dereth_protocol::movement::{MoveToArm, MovementParameters};
    let arm = MoveToArm::TurnToObject {
        target: ObjectId(TARGET),
        desired_heading: 0.0,
        params: MovementParameters::TurnTo {
            bitfield: MovementParameters::DEFAULT_BITFIELD,
            speed: 1.0,
            desired_heading: 0.0,
        },
    };
    send(
        shot,
        stamp,
        MovementBody {
            movement_type: movement_type::TURN_TO_OBJECT,
            current_style: index(style),
            unhandled: MovementBody::encode_move_to(&arm),
            ..MovementBody::default()
        },
    );
}

/// Move the target to block-local `(x, y, z)`, as `0xF748 Movement_PositionEvent`.
fn place_target(shot: &mut Shot, block: u16, (x, y, z): (f32, f32, f32), stamp: u16) {
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a block-local position, 0..192, to its outdoor cell's index
    let cell = (u32::from(block) << 16) | ((x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1);
    let body = dereth_protocol::write_body(&MovementPositionEvent {
        id: ObjectId(TARGET),
        position: PositionPack {
            flags: position_flags::IS_GROUNDED,
            origin: dereth_protocol::types::Origin {
                objcell_id: cell,
                origin: Vec3::new(x, y, z).into(),
            },
            orientation: dereth_primitives::Quat::IDENTITY.into(),
            instance_timestamp: 1,
            position_timestamp: stamp,
            teleport_timestamp: 1,
            ..PositionPack::default()
        },
    })
    .expect("encode");
    shot.send(Opcode::MOVEMENT_POSITION_EVENT, body);
}

/// A setup's radius.
fn radius(store: &dereth_dat::RetailDatStore, setup: u32) -> f32 {
    use dereth_assets::Decode;
    let id = dereth_primitives::DataId(setup);
    let bytes = store
        .read_typed(dereth_dat::DbType::Setup, id)
        .expect("the setup reads");
    dereth_assets::Setup::decode_payload(id, &bytes)
        .expect("the setup decodes")
        .radius
}

/// One subject: its name, what the server sends at its start (and how long after the start it
/// is first drawn), and how long it lasts.
struct Subject {
    name: &'static str,
    style: C,
    forward: C,
    actions: Vec<C>,
    lead: f64,
    every: f64,
    /// Starts at peace, takes the stance, then begins its actions a moment later.
    engage: bool,
    /// Turns to face the target as it circles.
    track: bool,
}

fn subjects() -> Vec<Subject> {
    let attacks: Vec<C> = env("DERETH_CREATURE_ATTACKS")
        .unwrap_or_else(|| "AttackHigh1".into())
        .split(',')
        .map(|n| C::from_name(n.trim()).expect("a motion name"))
        .collect();
    let all = |name: &str| -> Subject {
        let s = |style, forward, actions, lead, every| Subject {
            name: match name {
                "idle" => "idle",
                "walk" => "walk",
                "run" => "run",
                "turn" => "turn",
                "attack" => "attack",
                "engage" => "engage",
                "track" => "track",
                "hit" => "hit",
                "die" => "die",
                other => panic!("{other} is not a subject"),
            },
            style,
            forward,
            actions,
            lead,
            every,
            engage: name == "engage",
            track: name == "track",
        };
        match name {
            "idle" => s(C::NON_COMBAT, C::READY, vec![], 0.5, 0.55),
            "walk" => s(C::NON_COMBAT, C::WALK_FORWARD, vec![], 1.0, 0.2),
            "run" => s(C::NON_COMBAT, C::RUN_FORWARD, vec![], 1.0, 0.15),
            "turn" => s(C::NON_COMBAT, C::TURN_RIGHT, vec![], 0.5, 0.3),
            "attack" => s(C::HAND_COMBAT, C::READY, attacks.clone(), 0.2, 0.55),
            "engage" => s(C::HAND_COMBAT, C::READY, attacks.clone(), 0.1, 0.2),
            "track" => s(C::HAND_COMBAT, C::READY, vec![], 0.1, 0.25),
            "hit" => s(C::HAND_COMBAT, C::READY, vec![C::TWITCH1], 0.1, 0.5),
            "die" => s(C::HAND_COMBAT, C::DEAD, vec![], 0.2, 1.1),
            other => panic!("{other} is not a subject"),
        }
    };
    env("DERETH_CREATURE_SUBJECTS")
        .unwrap_or_else(|| "idle,walk,attack,hit,die".into())
        .split(',')
        .map(|n| all(n.trim()))
        .collect()
}

/// The creature drawn one way through every subject: per subject, its moments.
#[allow(clippy::too_many_lines)] // one run, start to end
fn draw(
    store: &Arc<dereth_dat::RetailDatStore>,
    skinned: bool,
    subjects: &[Subject],
    frames_to: Option<std::path::PathBuf>,
) -> Vec<Vec<Vec<u8>>> {
    let (w, h) = size();
    let at = env("DERETH_CREATURE_AT").unwrap_or_else(|| "A9B3:96,80".into());
    let (block, rest) = at.split_once(':').expect("block:x,y");
    let (x, y) = rest.split_once(',').expect("block:x,y");
    #[allow(clippy::cast_possible_truncation)]
    let block = hex(block) as u16;
    let (cx, cy): (f32, f32) = (
        x.trim().parse().expect("a number"),
        y.trim().parse().expect("a number"),
    );
    let setup = hex(&env("DERETH_CREATURE_SETUP").expect("DERETH_CREATURE_SETUP"));
    let mtable = hex(&env("DERETH_CREATURE_MTABLE").expect("DERETH_CREATURE_MTABLE"));
    let remaster = env("DERETH_CREATURE_LOOK").is_some_and(|v| v.trim() == "remaster");
    let mut gpu = if remaster {
        remaster_gpu(w, h)
    } else {
        crate::common::test_gpu(w, h)
    };
    let station = Station {
        name: "overlay creature",
        why: "a creature a world's overlay adds",
        place: Place::Ground {
            block,
            x: cx - 40.0,
            y: cy - 40.0,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.scene.draw.set_skinned_bodies(skinned);
    shot.poll(&mut gpu);
    if remaster {
        start_remaster(&mut shot, store, &mut gpu);
    }
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
        .expect("the land source opens");
    let ground = |x: f32, y: f32| land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
    // Facing east, so a walk crosses the camera's view.
    let heading = 270.0f32;
    let scale = number("DERETH_CREATURE_SCALE", 1.0);
    create(
        &mut shot,
        ID,
        (setup, mtable),
        block,
        (cx, cy, ground(cx, cy)),
        heading,
        scale,
    );
    // The target: in front of the creature, the gap from its edge, facing it.
    let target = env("DERETH_CREATURE_TARGET").map(|g| {
        let gap: f32 = g.trim().parse().expect("a number");
        let reach = radius(store, setup) * scale + radius(store, TARGET_BODY.0) + gap;
        println!(
            "target {reach:.2} m from the creature's middle (its radius {:.2} at its scale)",
            radius(store, setup) * scale
        );
        reach
    });
    // Where the target stands `bearing` degrees to the creature's right of its front.
    let target_at = |reach: f32, bearing: f32| {
        let r = (heading - bearing).to_radians();
        let (x, y) = (cx - math::sinf(r) * reach, cy + math::cosf(r) * reach);
        (x, y, ground(x, y))
    };
    if let Some(reach) = target {
        let start = number("DERETH_CREATURE_TARGET_BEARING", 0.0);
        create(
            &mut shot,
            TARGET,
            TARGET_BODY,
            block,
            target_at(reach, start),
            heading + 180.0 - start,
            1.0,
        );
    }
    shot.frame_seconds = 1.0 / FPS;
    let distance = number("DERETH_CREATURE_DISTANCE", 14.0);
    let aim = number("DERETH_CREATURE_AIM", 2.5);
    let view = number("DERETH_CREATURE_VIEW", 60.0).to_radians();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let moments = number("DERETH_CREATURE_MOMENTS", 6.0) as usize;
    let mut stamp = 0u16;
    let mut action_stamp = 0u16;
    let follow = |shot: &mut Shot| {
        let o = shot
            .scene
            .objects
            .get(&ObjectId(ID))
            .map_or(Vec3::new(cx, cy, ground(cx, cy)), |o| o.frame.origin);
        let r = heading.to_radians();
        let forward = (-math::sinf(r), math::cosf(r));
        // The camera's bearing from the creature: its front turned by `view`.
        let (s, c) = (math::sinf(view), math::cosf(view));
        let dir = (forward.0 * c - forward.1 * s, forward.0 * s + forward.1 * c);
        let eye = Vec3::new(
            o.x + dir.0 * distance,
            o.y + dir.1 * distance,
            o.z + aim + distance * 0.18,
        );
        let yaw = math::atan2f(dir.0, -dir.1);
        let pitch = -math::atan2f(distance * 0.18, distance);
        shot.hold_camera(eye, yaw, pitch);
    };
    follow(&mut shot);
    shot.step_held(&mut gpu);
    if remaster {
        wait_for_ground(&mut shot);
    }
    for _ in 0..30 {
        follow(&mut shot);
        shot.step(&mut gpu);
    }
    let mut frame_no = 0u32;
    let mut out = Vec::new();
    for subject in subjects {
        let actions: Vec<(C, u16)> = subject
            .actions
            .iter()
            .map(|c| {
                action_stamp = action_stamp.wrapping_add(1);
                (*c, action_stamp)
            })
            .collect();
        // Into the stance first, standing: at peace, for a creature about to engage.
        let first = if subject.engage {
            C::NON_COMBAT
        } else {
            subject.style
        };
        send(&mut shot, &mut stamp, body(first, C::READY, &[]));
        for _ in 0..if subject.engage { 60 } else { 20 } {
            follow(&mut shot);
            shot.step(&mut gpu);
        }
        if subject.engage {
            send(&mut shot, &mut stamp, body(subject.style, C::READY, &[]));
        } else {
            send(
                &mut shot,
                &mut stamp,
                body(subject.style, subject.forward, &actions),
            );
        }
        let mut swung = !subject.engage;
        let mut told = f64::MIN;
        let mut placed = 1u16;
        let bearing = number("DERETH_CREATURE_TARGET_BEARING", 120.0);
        let spin = number("DERETH_CREATURE_TARGET_SPIN", 90.0);
        let mut pictures = Vec::new();
        #[allow(clippy::cast_precision_loss)]
        let total = subject.lead + subject.every * moments as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (total * FPS).ceil() as u32;
        let mut next = subject.lead;
        for f in 0..frames {
            let t = f64::from(f + 1) / FPS;
            if !swung && t >= ENGAGE_AT {
                send(
                    &mut shot,
                    &mut stamp,
                    body(subject.style, subject.forward, &actions),
                );
                swung = true;
            }
            if let (true, Some(reach)) = (subject.track, target) {
                #[allow(clippy::cast_possible_truncation)] // a few seconds
                let at = target_at(reach, bearing + spin * t as f32);
                placed = placed.wrapping_add(1);
                place_target(&mut shot, block, at, placed);
                if t - told >= RETURN_EVERY {
                    turn_to_target(&mut shot, &mut stamp, subject.style);
                    told = t;
                }
            }
            follow(&mut shot);
            shot.step(&mut gpu);
            if subject.track && t + 0.5 / FPS >= next && pictures.len() < moments {
                println!(
                    "{}: at {t:.2} s it faces {:.0} degrees off the target",
                    subject.name,
                    off_target(&shot)
                );
            }
            if let Some(dir) = frames_to.as_ref() {
                let rgba = stations::capture(&mut gpu);
                stations::write_png(&dir.join(format!("{frame_no:04}.png")), w, h, &rgba);
                frame_no += 1;
                if t + 0.5 / FPS >= next && pictures.len() < moments {
                    pictures.push(rgba);
                    next += subject.every;
                }
            } else if t + 0.5 / FPS >= next && pictures.len() < moments {
                pictures.push(stations::capture(&mut gpu));
                next += subject.every;
            }
        }
        // Back to standing, unless dead.
        if subject.forward != C::DEAD {
            send(&mut shot, &mut stamp, body(subject.style, C::READY, &[]));
            for _ in 0..45 {
                follow(&mut shot);
                shot.step(&mut gpu);
            }
        }
        println!("{}: {} pictures", subject.name, pictures.len());
        out.push(pictures);
    }
    if remaster {
        stop_remaster(&mut shot, store, &mut gpu);
    }
    out
}

/// A device the remaster can draw on.
#[cfg(feature = "remaster")]
fn remaster_gpu(width: u32, height: u32) -> dereth_render::device::Gpu {
    let cfg = dereth_render::device::DeviceConfig {
        width,
        height,
        remaster: true,
        ..dereth_render::device::DeviceConfig::default()
    };
    dereth_render::device::Gpu::new_on(crate::common::remaster_backend(), None, &cfg)
        .expect("a device")
}

#[cfg(not(feature = "remaster"))]
fn remaster_gpu(_: u32, _: u32) -> dereth_render::device::Gpu {
    panic!("the remaster is not built")
}

/// The remaster turned on, lighting by the pixel.
#[cfg(feature = "remaster")]
fn start_remaster(
    shot: &mut Shot,
    store: &dereth_dat::RetailDatStore,
    gpu: &mut dereth_render::device::Gpu,
) {
    shot.scene
        .draw
        .set_remaster(true, store, gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, gpu);
}

#[cfg(not(feature = "remaster"))]
fn start_remaster(
    _: &mut Shot,
    _: &dereth_dat::RetailDatStore,
    _: &mut dereth_render::device::Gpu,
) {
}

#[cfg(feature = "remaster")]
fn wait_for_ground(shot: &mut Shot) {
    shot.scene.wait_for_remaster_ground();
}

#[cfg(not(feature = "remaster"))]
fn wait_for_ground(_: &mut Shot) {}

#[cfg(feature = "remaster")]
fn stop_remaster(
    shot: &mut Shot,
    store: &dereth_dat::RetailDatStore,
    gpu: &mut dereth_render::device::Gpu,
) {
    shot.scene
        .draw
        .set_remaster(false, store, gpu)
        .expect("the remaster stops");
}

#[cfg(not(feature = "remaster"))]
fn stop_remaster(_: &mut Shot, _: &dereth_dat::RetailDatStore, _: &mut dereth_render::device::Gpu) {
}

/// How far the creature faces off the target, degrees.
fn off_target(shot: &Shot) -> f32 {
    let (Some(c), Some(t)) = (
        shot.scene.objects.get(&ObjectId(ID)),
        shot.scene.objects.get(&ObjectId(TARGET)),
    ) else {
        return f32::NAN;
    };
    let q = c.frame.rotation;
    // The creature's front, (0, 1) turned by its heading about z.
    let yaw = 2.0 * math::atan2f(q.z, q.w);
    let front = (-math::sinf(yaw), math::cosf(yaw));
    let to = (
        t.frame.origin.x - c.frame.origin.x,
        t.frame.origin.y - c.frame.origin.y,
    );
    let cross = front.0 * to.1 - front.1 * to.0;
    let dot = front.0 * to.0 + front.1 * to.1;
    math::atan2f(cross, dot).to_degrees().abs()
}

/// The moments laid out a row a subject, each picture halved.
fn sheet(rows: &[Vec<Vec<u8>>], size: (u32, u32)) -> (u32, u32, Vec<u8>) {
    let (w, h) = (size.0 / 2, size.1 / 2);
    let across = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let across = u32::try_from(across).expect("a few");
    let down = u32::try_from(rows.len()).expect("a few").max(1);
    let (sw, sh) = (w * across, h * down);
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for (r, row) in rows.iter().enumerate() {
        for (k, p) in row.iter().enumerate() {
            let (ox, oy) = (
                u32::try_from(k).expect("a few") * w,
                u32::try_from(r).expect("a few") * h,
            );
            for y in 0..h {
                for x in 0..w {
                    for c in 0..4 {
                        let at = |dx: u32, dy: u32| {
                            u32::from(p[(((y * 2 + dy) * size.0 + x * 2 + dx) * 4 + c) as usize])
                        };
                        let v = (at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4;
                        #[allow(clippy::cast_possible_truncation)] // an average of bytes
                        {
                            out[(((oy + y) * sw + ox + x) * 4 + c) as usize] = v as u8;
                        }
                    }
                }
            }
        }
    }
    (sw, sh, out)
}

/// The creature drawn with its skinned body and with its parts, a sheet each.
#[test]
#[ignore = "instrument: draws a creature a world's overlay adds, skinned and by parts; run with --ignored"]
fn draw_an_overlay_creature_skinned_and_by_its_parts() {
    let _gpu = crate::common::gpu_lock();
    let overlay = std::path::PathBuf::from(
        env("DERETH_CREATURE_OVERLAY").expect("DERETH_CREATURE_OVERLAY names the overlay folder"),
    );
    assert!(
        overlay.join("overlay_portal.dat").is_file(),
        "{} holds no overlay",
        overlay.display()
    );
    let world = env("DERETH_CREATURE_WORLD").unwrap_or_else(|| "ac2-creatures".into());
    let dir = OverlayDir::new(&overlay).expect("the overlay folder");
    let store = Arc::new(
        crate::common::dat_store()
            .with_overlay(&dir, Some(&world))
            .expect("the overlay lays over the dats"),
    );
    let dump = std::path::PathBuf::from(
        env("DERETH_CREATURE_DUMP").expect("DERETH_CREATURE_DUMP names where the sheets go"),
    );
    std::fs::create_dir_all(&dump).expect("the dump folder");
    let subjects = subjects();
    let ways: BTreeMap<&str, bool> = [("skinned", true), ("parts", false)]
        .into_iter()
        .filter(|(n, _)| {
            env("DERETH_CREATURE_WAYS").is_none_or(|w| w.split(',').any(|x| x.trim() == *n))
        })
        .collect();
    for (way, skinned) in ways {
        let frames = (env("DERETH_CREATURE_SEQUENCE").as_deref() == Some("1")).then(|| {
            let d = dump.join(way);
            std::fs::create_dir_all(&d).expect("the frames folder");
            d
        });
        let rows = draw(&store, skinned, &subjects, frames);
        let (w, h, rgba) = sheet(&rows, size());
        stations::write_png(&dump.join(format!("{way}.png")), w, h, &rgba);
        println!("{way}: {}", dump.join(format!("{way}.png")).display());
    }
}
