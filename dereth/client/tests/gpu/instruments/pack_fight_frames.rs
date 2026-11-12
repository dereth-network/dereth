//! Creatures fighting as a server's monsters fight, drawn by the remaster without art packs and
//! with them, so what a pack's body plays can be set beside what the parts it stands for play.
//! Each creature is made in its combat stance round a target, runs to it, swings at it while it
//! stands near, and runs after it again when it moves off; the target runs between a few places.
//! Every frame each creature's row is written: what its parts play (the forward and turn
//! commands, the animation, how far through it and how many frames it has), the slot whose clip
//! its body plays and how far through it, how far it moved and how far of that ahead. Pictures
//! are taken at even moments and laid out as one strip. The slots each body fills are printed
//! first.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACKS`: the art packs, a `;` list (required).
//! - `DERETH_FIGHT_KINDS`: the creatures, `setup:mtable`, a comma list, hex (required): a setup a
//!   pack has a body for, and the motion table its creatures' weenies give it.
//! - `DERETH_FIGHT_AT`: where the fight is, `block:x,y` (block in hex, block-local meters;
//!   default open ground south of Holtburg).
//! - `DERETH_FIGHT_EACH`: creatures of each kind (default 2).
//! - `DERETH_FIGHT_SECONDS`: how long it runs (default 16).
//! - `DERETH_FIGHT_SLEEP`: the creatures sleep from the start until this many seconds, then wake
//!   and fight (default 0: no sleep).
//! - `DERETH_FIGHT_CSV`: where the rows go, `{way}` in it replaced by `retail` or `pack`
//!   (default none).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the strips go, `strip-retail.png` and `strip-pack.png`
//!   (none are written without it).
//! - `DERETH_FIGHT_SHOTS`: pictures in a strip (default 12), from the third second on.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use dereth_animation::data::{AnimAssets, MotionTableData};
use dereth_animation::MotionCommand as C;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, ObjectId};
use dereth_protocol::movement::{
    motion_flags, movement_type, InterpretedMotionState, MoveToArm, MovementBody, MovementBuffer,
    MovementParameters, MovementSetObjectMovement,
};
use dereth_protocol::Opcode;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks};
use dereth_world_data::anim_assets::DatAnimAssets;

use super::stations::{self, Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn hex(v: &str) -> u32 {
    u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a hex id")
}

fn number(name: &str, default: f32) -> f32 {
    env(name).map_or(default, |v| v.trim().parse().expect("a number"))
}

/// The command whose low half is `low`, as a motion table's keys keep it.
fn command(low: u32) -> C {
    (0..u16::MAX)
        .map_while(C::from_index)
        .find(|c| c.0 & 0xFFFF == low & 0xFFFF && c.0 & 0x00FF_0000 == 0)
        .unwrap_or(C(low))
}

fn name(c: C) -> String {
    c.name()
        .map_or_else(|| format!("{:08X}", c.0), str::to_owned)
}

/// What each animation of `t` is played for, its names joined: `Style.Motion` for a cycle,
/// `Style.Motion~` for a modifier and `Style.From>To` for a link or an action.
fn labels(t: &MotionTableData) -> BTreeMap<u32, String> {
    let style = |key: u32| name(C(0x8000_0000 | (key >> 16)));
    let mut out: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut add = |anims: &[dereth_primitives::records::AnimData], label: String| {
        for a in anims {
            let names = out.entry(a.anim_id.0).or_default();
            if !names.contains(&label) {
                names.push(label.clone());
            }
        }
    };
    for (&key, md) in &t.cycles {
        add(&md.anims, format!("{}.{}", style(key), name(command(key))));
    }
    for (&key, md) in &t.modifiers {
        add(&md.anims, format!("{}.{}~", style(key), name(command(key))));
    }
    for (&key, to) in &t.links {
        for (&dest, md) in to {
            add(
                &md.anims,
                format!("{}.{}>{}", style(key), name(command(key)), name(C(dest))),
            );
        }
    }
    out.into_iter().map(|(k, v)| (k, v.join("|"))).collect()
}

/// One kind of creature: its setup and motion table, its animations' names, the stance it
/// fights in, and its attacks with how long each takes.
struct Kind {
    pack: String,
    setup: u32,
    mtable: u32,
    labels: BTreeMap<u32, String>,
    style: C,
    attacks: Vec<(C, f32)>,
}

/// How long `md` plays at speed 1, in seconds.
fn seconds(assets: &DatAnimAssets, md: &dereth_animation::data::MotionData) -> f32 {
    md.anims
        .iter()
        .map(|a| {
            let frames = assets.animation(a.anim_id).map_or(1, |d| d.num_frames);
            #[allow(clippy::cast_precision_loss)] // an animation's frames
            let high = if a.high_frame < 0 {
                frames.saturating_sub(1) as f32
            } else {
                a.high_frame as f32
            };
            #[allow(clippy::cast_precision_loss)]
            let span = (high - a.low_frame.max(0) as f32 + 1.0).max(1.0);
            span / a.framerate.abs().max(1.0)
        })
        .sum()
}

/// The kinds `DERETH_FIGHT_KINDS` names, each a setup a pack has a body for and the motion
/// table its creatures' weenies give it (`setup:mtable`, a comma list, hex), with the slots each
/// one fills printed: the first body of each pack whose setup is named.
fn kinds(store: &Arc<dereth_dat::RetailDatStore>, packs: &[ArtPack]) -> Vec<Kind> {
    let assets = DatAnimAssets::new(Arc::clone(store));
    let named: Vec<(u32, u32)> = env("DERETH_FIGHT_KINDS")
        .expect("DERETH_FIGHT_KINDS names the creatures, setup:mtable")
        .split(',')
        .map(|k| {
            let (s, m) = k.split_once(':').expect("setup:mtable");
            (hex(s), hex(m))
        })
        .collect();
    let mut out: Vec<Kind> = Vec::new();
    for (pack, skin) in packs
        .iter()
        .flat_map(|p| p.skins.iter().map(move |k| (p, k)))
    {
        if out.iter().any(|k| k.pack == pack.name) {
            continue;
        }
        let Some(&(_, mtable)) = named.iter().find(|n| n.0 == skin.setup.0) else {
            continue;
        };
        let mtable = dereth_primitives::DataId(mtable);
        let table = assets.motion_table(mtable).expect("the motion table reads");
        let labels = labels(&table);
        let clips: Vec<&str> = skin.clips.iter().map(|(slot, _)| slot.as_str()).collect();
        let styles: Vec<C> = table.style_defaults.keys().map(|&s| C(s)).collect();
        let style = [C::HAND_COMBAT, C::SWORD_COMBAT, C::SWORD_SHIELD_COMBAT]
            .into_iter()
            .find(|s| styles.contains(s))
            .or_else(|| styles.iter().copied().find(|&s| s != C::NON_COMBAT))
            .unwrap_or(table.default_style);
        let key = |s: C, m: C| dereth_animation::table::MotionTable::key(s, m);
        let attacks: Vec<(C, f32)> = table
            .links
            .get(&key(style, C::READY))
            .map(|to| {
                to.iter()
                    .filter(|(&d, _)| name(C(d)).starts_with("Attack"))
                    .map(|(&d, md)| (C(d), seconds(&assets, md)))
                    .collect()
            })
            .unwrap_or_default();
        println!(
            "{}: {:08X} (motion table {:08X}), fought in {}, its slots: {}",
            pack.name,
            skin.setup.0,
            mtable.0,
            name(style),
            clips.join(" ")
        );
        out.push(Kind {
            pack: pack.name.clone(),
            setup: skin.setup.0,
            mtable: mtable.0,
            labels,
            style,
            attacks,
        });
    }
    out
}

/// The server's side of the fight: each object's movement stamps.
#[derive(Default)]
struct Server {
    stamps: BTreeMap<u32, u16>,
}

fn index(c: C) -> u16 {
    c.to_index().expect("a command the wire carries")
}

impl Server {
    /// Send object `id` the movement `body`, as `0xF74C Movement_SetObjectMovement`.
    fn send(&mut self, shot: &mut Shot, id: u32, body: MovementBody) {
        let stamp = self.stamps.entry(id).or_insert(0);
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

    /// `id` stands in `style` and plays `forward` (an action, or the stance's ready), stuck to
    /// `sticky` when given, as a monster's swing is sent.
    fn motion(&mut self, shot: &mut Shot, id: u32, style: C, forward: C, sticky: Option<u32>) {
        let state = InterpretedMotionState {
            current_style: Some(index(style)),
            forward_command: (forward != C::READY).then(|| index(forward)),
            turn_speed: sticky.map(|_| 2.25),
            ..InterpretedMotionState::default()
        };
        self.send(
            shot,
            id,
            MovementBody {
                movement_type: movement_type::INVALID,
                motion_flags: if sticky.is_some() {
                    motion_flags::STICK_TO_OBJECT
                } else {
                    0
                },
                current_style: index(style),
                interpreted: Some(state),
                sticky_object: sticky.map(ObjectId),
                ..MovementBody::default()
            },
        );
    }

    /// `id`, in `style`, runs to `arm`'s object or place.
    fn move_to(&mut self, shot: &mut Shot, id: u32, style: C, arm: &MoveToArm) {
        self.send(
            shot,
            id,
            MovementBody {
                movement_type: match arm {
                    MoveToArm::MoveToObject { .. } => movement_type::MOVE_TO_OBJECT,
                    _ => movement_type::MOVE_TO_POSITION,
                },
                motion_flags: 0,
                current_style: index(style),
                unhandled: MovementBody::encode_move_to(arm),
                ..MovementBody::default()
            },
        );
    }
}

/// A monster's attack chase: sticky, failing on a walk, turning to the target at the end.
const ATTACK_CHASE: u32 = 0x1_EFF0;

/// What a creature is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Doing {
    Waiting,
    Chasing { since: f64 },
    Swinging { until: f64 },
}

/// One creature of the fight.
struct Fighter {
    id: u32,
    kind: usize,
    doing: Doing,
    swings: usize,
    last: Option<(f32, f32)>,
}

/// The target's runs: when, and to where from the fight's middle.
const RUNS: [(f64, f32, f32); 3] = [(7.0, 5.0, 3.0), (11.0, -4.0, 6.0), (14.0, 0.0, 0.0)];

/// The fight drawn one way; its rows, and its pictures.
#[allow(clippy::too_many_lines)] // one fight, start to end
fn fight(
    store: &Arc<dereth_dat::RetailDatStore>,
    pack: &Arc<ArtPack>,
    kinds: &[Kind],
    packs: RemasterPacks,
    size: (u32, u32),
) -> (String, Vec<Vec<u8>>) {
    let at = env("DERETH_FIGHT_AT").unwrap_or_else(|| "A9B3:96,96".into());
    let (block, rest) = at.split_once(':').expect("block:x,y");
    let (x, y) = rest.split_once(',').expect("block:x,y");
    #[allow(clippy::cast_possible_truncation)]
    let block = hex(block) as u16;
    let (cx, cy): (f32, f32) = (
        x.trim().parse().expect("a number"),
        y.trim().parse().expect("a number"),
    );
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    // The viewer's body stands off to the side, so the creatures near it move.
    let station = Station {
        name: "fight",
        why: "creatures fighting",
        place: Place::Ground {
            block,
            x: cx - 24.0,
            y: cy,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_pack(Some(Arc::clone(pack)), None);
    shot.scene.draw.set_remaster_packs(packs, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
        .expect("the land source opens");
    let ground = |x: f32, y: f32| land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
    let gz = ground(cx, cy);
    shot.hold_camera(
        dereth_primitives::Vec3::new(cx, cy - 9.0, gz + 3.5),
        0.0,
        -0.36,
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a block-local position, 0..192, to its outdoor cell's index
    let cell_of =
        |x: f32, y: f32| (u32::from(block) << 16) | ((x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1);
    const TARGET: u32 = 0x8300_2000;
    shot.create_object(
        TARGET,
        (0x0200_0001, Some(0x0900_0001)),
        block,
        (cx, cy),
        0.0,
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let each = number("DERETH_FIGHT_EACH", 2.0) as usize;
    let mut fighters = Vec::new();
    let count = kinds.len() * each;
    for k in 0..count {
        let id = 0x8300_3000 + u32::try_from(k).expect("a few");
        #[allow(clippy::cast_precision_loss)]
        let turn = std::f32::consts::TAU * k as f32 / count as f32;
        let (x, y) = (cx + 10.0 * math::cosf(turn), cy + 10.0 * math::sinf(turn));
        // Facing the middle: a heading from north, turning west.
        let heading = math::atan2f(-(cx - x), cy - y).to_degrees();
        let kind = k % kinds.len();
        let kd = &kinds[kind];
        shot.create_object(id, (kd.setup, Some(kd.mtable)), block, (x, y), heading);
        fighters.push(Fighter {
            id,
            kind,
            doing: Doing::Waiting,
            swings: 0,
            last: None,
        });
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let mut server = Server::default();
    let assets = DatAnimAssets::new(Arc::clone(store));
    let fps = 30.0;
    shot.frame_seconds = 1.0 / fps;
    let seconds = f64::from(number("DERETH_FIGHT_SECONDS", 16.0));
    let asleep = f64::from(number("DERETH_FIGHT_SLEEP", 0.0));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds * fps) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let shots = number("DERETH_FIGHT_SHOTS", 12.0) as u32;
    // The third second on, at thirty frames a second.
    let first_shot = 90;
    let shot_every = ((frames - first_shot) / shots.max(1)).max(1);
    let mut pictures = Vec::new();
    let mut rows = String::from(
        "frame,t,id,setup,forward,turn,node,node_anim,node_label,cyclic,frame_number,node_frames,\
         clip_slot,share,paced,moved,ahead,to_target,doing\n",
    );
    let mut runs = RUNS.iter().peekable();
    let place = |shot: &Shot, id: u32| {
        shot.scene
            .server_object_frame(ObjectId(id))
            .map(|f| (f.origin.x, f.origin.y))
    };
    // Which way an object faces across the ground: its own forward, north turned by its heading.
    let facing = |shot: &Shot, id: u32| {
        shot.scene.server_object_frame(ObjectId(id)).map(|f| {
            let q = f.rotation;
            let yaw = 2.0 * math::atan2f(q.z, q.w);
            (-math::sinf(yaw), math::cosf(yaw))
        })
    };
    for frame in 0..frames {
        let t = f64::from(frame) / fps;
        if frame == 6 {
            let first = if asleep > 0.0 { C::SLEEPING } else { C::READY };
            for f in &fighters {
                server.motion(&mut shot, f.id, kinds[f.kind].style, first, None);
            }
        }
        if asleep > 0.0 && (t - asleep).abs() < 0.5 / fps {
            for f in &fighters {
                server.motion(&mut shot, f.id, kinds[f.kind].style, C::READY, None);
            }
        }
        if let Some(&&(when, dx, dy)) = runs.peek() {
            if t >= when {
                runs.next();
                let (x, y) = (cx + dx, cy + dy);
                let arm = MoveToArm::MoveToPosition {
                    origin: dereth_protocol::types::Origin {
                        objcell_id: cell_of(x, y),
                        origin: dereth_primitives::Vec3::new(x, y, ground(x, y)).into(),
                    },
                    params: MovementParameters::MoveTo {
                        bitfield: dereth_animation::motion::DEFAULT_FLAGS,
                        distance_to_object: 0.6,
                        min_distance: 0.0,
                        fail_distance: f32::MAX,
                        speed: 1.0,
                        walk_run_threshold: 1.0,
                        desired_heading: 0.0,
                    },
                    run_rate: 1.5,
                };
                server.move_to(&mut shot, TARGET, C::NON_COMBAT, &arm);
            }
        }
        let target = place(&shot, TARGET).unwrap_or((cx, cy));
        for f in &mut fighters {
            let me = place(&shot, f.id).unwrap_or(target);
            let d = ((me.0 - target.0).powi(2) + (me.1 - target.1).powi(2)).sqrt();
            let kd = &kinds[f.kind];
            let chase = |server: &mut Server, shot: &mut Shot| {
                let arm = MoveToArm::MoveToObject {
                    target: ObjectId(TARGET),
                    origin: dereth_protocol::types::Origin {
                        objcell_id: cell_of(target.0, target.1),
                        origin: dereth_primitives::Vec3::new(
                            target.0,
                            target.1,
                            ground(target.0, target.1),
                        )
                        .into(),
                    },
                    params: MovementParameters::MoveTo {
                        bitfield: ATTACK_CHASE,
                        distance_to_object: 0.6,
                        min_distance: 0.0,
                        fail_distance: f32::MAX,
                        speed: 1.0,
                        walk_run_threshold: 15.0,
                        desired_heading: 0.0,
                    },
                    run_rate: 1.8,
                };
                server.move_to(shot, f.id, kd.style, &arm);
            };
            // Near enough to swing: within a stride of the target's edge.
            let reach = 2.4;
            match f.doing {
                Doing::Waiting if t >= asleep + 1.0 + 0.15 * f64::from(f.id & 0xFF) => {
                    chase(&mut server, &mut shot);
                    f.doing = Doing::Chasing { since: t };
                }
                Doing::Chasing { since } if t > since + 0.5 => {
                    let idle = shot
                        .scene
                        .server_object_move_to_state(ObjectId(f.id))
                        .is_none_or(|s| s.2 == 0);
                    if d <= reach && idle && !kd.attacks.is_empty() {
                        let (attack, length) = kd.attacks[f.swings % kd.attacks.len()];
                        f.swings += 1;
                        server.motion(&mut shot, f.id, kd.style, attack, Some(TARGET));
                        f.doing = Doing::Swinging {
                            until: t + f64::from(length) + 0.4,
                        };
                    } else if idle {
                        chase(&mut server, &mut shot);
                        f.doing = Doing::Chasing { since: t };
                    }
                }
                Doing::Swinging { until } if t >= until => {
                    if d > reach + 0.8 || kd.attacks.is_empty() {
                        chase(&mut server, &mut shot);
                        f.doing = Doing::Chasing { since: t };
                    } else {
                        let (attack, length) = kd.attacks[f.swings % kd.attacks.len()];
                        f.swings += 1;
                        server.motion(&mut shot, f.id, kd.style, attack, Some(TARGET));
                        f.doing = Doing::Swinging {
                            until: t + f64::from(length) + 0.4,
                        };
                    }
                }
                _ => {}
            }
        }
        shot.step(&mut gpu);
        let playing: BTreeMap<u32, (Option<&str>, f32, bool)> = shot
            .scene
            .draw
            .remaster_skin_playing(&mut gpu)
            .into_iter()
            .map(|(id, a, s, p)| (id, (a, s, p)))
            .collect();
        let target = place(&shot, TARGET).unwrap_or((cx, cy));
        for f in &mut fighters {
            let id = ObjectId(f.id);
            let kd = &kinds[f.kind];
            let label = |a: u32| {
                kd.labels
                    .get(&a)
                    .cloned()
                    .unwrap_or_else(|| format!("{a:08X}"))
            };
            let me = place(&shot, f.id);
            let moved = me.zip(f.last).map_or(0.0, |(a, b)| {
                ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
            });
            let ahead = me
                .zip(f.last)
                .zip(facing(&shot, f.id))
                .map_or(0.0, |((a, b), (fx, fy))| {
                    (a.0 - b.0) * fx + (a.1 - b.1) * fy
                });
            f.last = me;
            let d = me.map_or(0.0, |me| {
                ((me.0 - target.0).powi(2) + (me.1 - target.1).powi(2)).sqrt()
            });
            let forward = shot
                .scene
                .server_object_motion(id)
                .map_or_else(String::new, |m| name(m.1));
            let turn = shot
                .scene
                .server_object_turn(id)
                .map_or_else(String::new, name);
            let (node, anim, cyclic, frame_number) = shot.scene.server_object_pose(id).map_or(
                (None, 0, false, 0.0),
                |(nodes, curr, first_cyclic, frame_number)| {
                    let anim = curr.and_then(|c| nodes.get(c)).map_or(0, |a| a.0);
                    (
                        curr,
                        anim,
                        curr.zip(first_cyclic).is_some_and(|(c, f)| c >= f),
                        f64::from(frame_number),
                    )
                },
            );
            let node_frames = assets
                .animation(dereth_primitives::DataId(anim))
                .map_or(0, |a| a.num_frames);
            let (clip, share, paced) = playing.get(&f.id).copied().unwrap_or((None, 0.0, false));
            writeln!(
                rows,
                "{frame},{t:.3},{:08X},{:08X},{forward},{turn},{},{anim:08X},{},{},{frame_number:.2},{node_frames},\
                 {},{share:.3},{},{moved:.4},{ahead:.4},{d:.3},{:?}",
                f.id,
                kd.setup,
                node.map_or_else(String::new, |n| n.to_string()),
                label(anim),
                u8::from(cyclic),
                clip.unwrap_or(""),
                u8::from(paced),
                f.doing,
            )
            .expect("a row");
        }
        if frame >= first_shot
            && (frame - first_shot) % shot_every == 0
            && pictures.len() < shots as usize
        {
            pictures.push(stations::capture(&mut gpu));
        }
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    (rows, pictures)
}

/// `pictures`, each `size`, halved and laid four to a row.
fn strip(pictures: &[Vec<u8>], size: (u32, u32)) -> (u32, u32, Vec<u8>) {
    let (w, h) = (size.0 / 2, size.1 / 2);
    let across = 4.min(u32::try_from(pictures.len()).unwrap_or(1)).max(1);
    let down = u32::try_from(pictures.len()).unwrap_or(1).div_ceil(across);
    let (sw, sh) = (w * across, h * down);
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for (k, p) in pictures.iter().enumerate() {
        let k = u32::try_from(k).expect("a few");
        let (ox, oy) = ((k % across) * w, (k / across) * h);
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
    (sw, sh, out)
}

/// The fight without the packs and with them; the rows and strips written where asked.
#[test]
#[ignore = "instrument: draws pack creatures fighting with and without their packs; run with --ignored"]
fn draw_pack_creatures_fighting_with_and_without_their_packs() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let paths: Vec<std::path::PathBuf> = env("DERETH_ART_PACKS")
        .expect("DERETH_ART_PACKS names the packs")
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|s| std::path::PathBuf::from(s.trim()))
        .collect();
    let pack = Arc::new(ArtPack::open_stack(&paths).expect("the packs open"));
    let each: Vec<ArtPack> = paths
        .iter()
        .map(|p| ArtPack::open_path(p).expect("the pack opens"))
        .collect();
    let kinds = kinds(&store, &each);
    drop(each);
    assert!(
        !kinds.is_empty(),
        "the packs have bodies with motion tables"
    );
    for k in &kinds {
        println!(
            "{:08X}: fights in {} with {}",
            k.setup,
            name(k.style),
            k.attacks
                .iter()
                .map(|(a, s)| format!("{} {s:.2}s", name(*a)))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let size = super::remaster_frames::size();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    for (way, packs) in [
        ("retail", RemasterPacks::default()),
        ("pack", RemasterPacks::all()),
    ] {
        let (rows, pictures) = fight(&store, &pack, &kinds, packs, size);
        if let Some(path) = env("DERETH_FIGHT_CSV") {
            let path = path.replace("{way}", way);
            std::fs::write(&path, rows).expect("the rows are written");
            println!("{way}: rows {path}");
        }
        if let Some(dir) = dir.as_ref() {
            let (w, h, rgba) = strip(&pictures, size);
            stations::write_png(&dir.join(format!("strip-{way}.png")), w, h, &rgba);
        }
        println!("{way}: {} pictures", pictures.len());
    }
}
