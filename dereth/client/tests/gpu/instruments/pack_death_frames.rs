//! Creatures dying as a server's monsters die, drawn by the remaster without art packs, with
//! them but their bodies let go with their objects, and with them as they are drawn: each
//! creature stands in its combat stance in a row facing the camera, dies, and once its parts'
//! death has played the server lets it go and lays its corpse where it fell, lying dead. Every
//! frame each creature's and each corpse's row is written: the slot whose clip its body plays and
//! how far through it, whether it is drawn on after its object left, and whether it is hidden
//! under such a body. Pictures are taken at even moments from just before the deaths and laid out
//! as one strip for each way.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACKS`: the art packs, a `;` list (required).
//! - `DERETH_DEATH_KINDS`: the creatures, `setup:mtable`, a comma list, hex (required): a setup a
//!   pack has a body for, and the motion table its creatures' weenies give it; one creature each.
//! - `DERETH_DEATH_AT`: where they stand, `block:x,y` (block in hex, block-local meters; default
//!   open ground south of Holtburg).
//! - `DERETH_DEATH_CSV`: where the rows go, `{way}` in it replaced by the way drawn (default
//!   none).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the strips go, `strip-retail.png`,
//!   `strip-let-go.png` and `strip-pack.png` (none are written without it).
//! - `DERETH_DEATH_SHOTS`: pictures in a strip (default 24), `DERETH_DEATH_EVERY` seconds apart
//!   (default 0.25), from a moment before the deaths.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::MotionCommand as C;
use dereth_primitives::{LandblockId, ObjectId};
use dereth_protocol::movement::{
    movement_type, InterpretedMotionState, MovementBody, MovementBuffer, MovementSetObjectMovement,
};
use dereth_protocol::Opcode;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks};
use dereth_world_data::anim_assets::DatAnimAssets;

use super::stations::{self, Clock, Place, Shot, Station};

/// The longest the remaster draws a body on after its object leaves, seconds.
const GHOST_MOST: f64 = 4.0;

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

/// One kind of creature: its pack, setup and motion table, the stance it dies in, and how long
/// its parts' death takes.
struct Kind {
    pack: String,
    setup: u32,
    mtable: u32,
    style: C,
    dies: f32,
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

/// The kinds `DERETH_DEATH_KINDS` names that a pack has a body for, one for each pack, the
/// slots each body fills printed.
fn kinds(store: &Arc<dereth_dat::RetailDatStore>, packs: &[ArtPack]) -> Vec<Kind> {
    let assets = DatAnimAssets::new(Arc::clone(store));
    let named: Vec<(u32, u32)> = env("DERETH_DEATH_KINDS")
        .expect("DERETH_DEATH_KINDS names the creatures, setup:mtable")
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
        let table = assets
            .motion_table(dereth_primitives::DataId(mtable))
            .expect("the motion table reads");
        let styles: Vec<C> = table.style_defaults.keys().map(|&s| C(s)).collect();
        let style = [C::HAND_COMBAT, C::SWORD_COMBAT, C::SWORD_SHIELD_COMBAT]
            .into_iter()
            .find(|s| styles.contains(s))
            .or_else(|| styles.iter().copied().find(|&s| s != C::NON_COMBAT))
            .unwrap_or(table.default_style);
        let key = dereth_animation::table::MotionTable::key;
        // The parts' death: the link from the stance's ready to dead, else the peace stance's.
        let dies = [style, C::NON_COMBAT]
            .into_iter()
            .find_map(|s| {
                table
                    .links
                    .get(&key(s, C::READY))
                    .and_then(|to| to.get(&C::DEAD.0))
            })
            .map_or(2.0, |md| seconds(&assets, md));
        let clips: Vec<&str> = skin.clips.iter().map(|(slot, _)| slot.as_str()).collect();
        println!(
            "{}: {:08X} (motion table {mtable:08X}), dies in {:.2}s, its slots: {}",
            pack.name,
            skin.setup.0,
            dies,
            clips.join(" ")
        );
        out.push(Kind {
            pack: pack.name.clone(),
            setup: skin.setup.0,
            mtable,
            style,
            dies,
        });
    }
    out
}

/// A movement body standing in `style` playing `forward`.
fn body(style: C, forward: C) -> MovementBody {
    MovementBody {
        movement_type: movement_type::INVALID,
        motion_flags: 0,
        current_style: index(style),
        interpreted: Some(InterpretedMotionState {
            current_style: Some(index(style)),
            forward_command: (forward != C::READY).then(|| index(forward)),
            ..InterpretedMotionState::default()
        }),
        ..MovementBody::default()
    }
}

/// Send object `id` the movement `body`, as `0xF74C Movement_SetObjectMovement`.
fn send(shot: &mut Shot, stamps: &mut BTreeMap<u32, u16>, id: u32, body: MovementBody) {
    let stamp = stamps.entry(id).or_insert(0);
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

/// One creature: its id, its kind, where it stands, and when its object is let go.
struct Dying {
    id: u32,
    corpse: u32,
    kind: usize,
    at: (f32, f32),
    leaves: f64,
}

/// When the creatures die, seconds.
const DIES_AT: f64 = 1.0;

/// The deaths drawn one way; their rows, and their pictures.
#[allow(clippy::too_many_lines)] // one run, start to end
fn deaths(
    store: &Arc<dereth_dat::RetailDatStore>,
    pack: &Arc<ArtPack>,
    kinds: &[Kind],
    (packs, ghosts): (RemasterPacks, bool),
    size: (u32, u32),
) -> (String, Vec<Vec<u8>>) {
    let at = env("DERETH_DEATH_AT").unwrap_or_else(|| "A9B3:96,96".into());
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
    let station = Station {
        name: "deaths",
        why: "creatures dying",
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
    shot.scene.draw.set_remaster_skin_ghosts(ghosts, &mut gpu);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
        .expect("the land source opens");
    let ground = |x: f32, y: f32| land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
    // A row facing the camera, three and a half meters apart.
    #[allow(clippy::cast_precision_loss)] // a few
    let span = (kinds.len().saturating_sub(1)) as f32 * 3.5;
    let mut dying = Vec::new();
    for (k, kind) in kinds.iter().enumerate() {
        let id = 0x8300_4000 + u32::try_from(k).expect("a few");
        #[allow(clippy::cast_precision_loss)]
        let at = (cx - span / 2.0 + k as f32 * 3.5, cy);
        shot.create_object(id, (kind.setup, Some(kind.mtable)), block, at, 180.0);
        dying.push(Dying {
            id,
            corpse: id + 0x100,
            kind: k,
            at,
            leaves: DIES_AT + f64::from(kind.dies),
        });
    }
    shot.hold_camera(
        dereth_primitives::Vec3::new(cx, cy - 7.0, ground(cx, cy - 7.0) + 1.8),
        0.0,
        -0.12,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let mut stamps = BTreeMap::new();
    let fps = 30.0;
    shot.frame_seconds = 1.0 / fps;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let shots = number("DERETH_DEATH_SHOTS", 24.0) as u32;
    let every = f64::from(number("DERETH_DEATH_EVERY", 0.25));
    let first = DIES_AT - 0.1;
    let last = dying.iter().map(|d| d.leaves).fold(first, f64::max);
    // On until the last body let go could have been drawn on as long as it may be, and a moment.
    let seconds = (first + every * f64::from(shots)).max(last + GHOST_MOST + 0.5);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds * fps) as u32;
    let mut next_shot = first;
    let mut pictures = Vec::new();
    let mut rows = String::from("frame,t,id,setup,what,slot,share,ghost,hidden\n");
    for frame in 0..frames {
        let t = f64::from(frame) / fps;
        if frame == 3 {
            for d in &dying {
                send(
                    &mut shot,
                    &mut stamps,
                    d.id,
                    body(kinds[d.kind].style, C::READY),
                );
            }
        }
        let now = |when: f64| (t - when).abs() < 0.5 / fps;
        for d in &dying {
            let kind = &kinds[d.kind];
            if now(DIES_AT) {
                send(&mut shot, &mut stamps, d.id, body(kind.style, C::DEAD));
            }
            if now(d.leaves) {
                // As a server lets a dead creature go: its corpse laid where it fell, lying dead,
                // and the creature itself deleted.
                let mut w = dereth_protocol::Writer::new();
                body(C::NON_COMBAT, C::DEAD)
                    .write(&mut w)
                    .expect("the movement writes");
                shot.create_object_moving(
                    d.corpse,
                    (kind.setup, Some(kind.mtable)),
                    block,
                    d.at,
                    180.0,
                    Some(w.into_inner()),
                );
                shot.delete_object(d.id);
            }
        }
        shot.step(&mut gpu);
        let playing: BTreeMap<u32, (Option<&str>, f32)> = shot
            .scene
            .draw
            .remaster_skin_playing(&mut gpu)
            .into_iter()
            .map(|(id, a, s, _)| (id, (a, s)))
            .collect();
        let (ghosted, hidden) = shot.scene.draw.remaster_skin_ghosts(&mut gpu);
        for d in &dying {
            for (id, what) in [(d.id, "creature"), (d.corpse, "corpse")] {
                let Some(&(slot, share)) = playing.get(&id) else {
                    continue;
                };
                writeln!(
                    rows,
                    "{frame},{t:.3},{id:08X},{:08X},{what},{},{share:.3},{},{}",
                    kinds[d.kind].setup,
                    slot.unwrap_or(""),
                    u8::from(ghosted.contains(&id)),
                    u8::from(hidden.contains(&id)),
                )
                .expect("a row");
            }
        }
        if t + 0.5 / fps >= next_shot && pictures.len() < shots as usize {
            pictures.push(stations::capture(&mut gpu));
            next_shot += every;
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

/// The deaths without the packs, with them but the bodies let go with their objects, and with
/// them as they are drawn; the rows and strips written where asked.
#[test]
#[ignore = "instrument: draws pack creatures dying with and without their bodies drawn on; run with --ignored"]
fn draw_pack_creatures_dying_with_and_without_their_bodies_drawn_on() {
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
    let size = super::remaster_frames::size();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    for (way, packs, ghosts) in [
        ("retail", RemasterPacks::default(), true),
        ("let-go", RemasterPacks::all(), false),
        ("pack", RemasterPacks::all(), true),
    ] {
        let (rows, pictures) = deaths(&store, &pack, &kinds, (packs, ghosts), size);
        if let Some(path) = env("DERETH_DEATH_CSV") {
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
