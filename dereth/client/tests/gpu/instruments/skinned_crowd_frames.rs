//! A run past crowds of creatures, timed a frame at a time: a body run along a straight line
//! through a few blocks while creatures stand in groups beside it, each made as the server would
//! make it once the body comes near and deleted once it is far, so their art pack bodies arrive
//! and leave as they do on a run through the world. Each frame's time on the processor, by its
//! parts, and on the device is printed as a summary, and each frame's row written to a file.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACKS`: the art packs, a `;` list, the stack's bottom first (default none).
//! - `DERETH_CROWD_KINDS`: the creatures, a comma list of `setup:mtable` (hex; default the
//!   human's).
//! - `DERETH_CROWD_FROM`, `DERETH_CROWD_TO`: where the run starts and ends, `block:x,y` (block in
//!   hex, block-local meters; default the middle of Holtburg's block to two blocks east of it).
//! - `DERETH_CROWD_SPEED`: meters a second (default 11, a run).
//! - `DERETH_CROWD_GROUPS`, `DERETH_CROWD_GROUP`: the groups along the run and the creatures in
//!   each (default 9 of 8), each group two of the kinds, beside the run.
//! - `DERETH_CROWD_NEAR`, `DERETH_CROWD_FAR`: how near a creature is made and how far it is
//!   deleted, meters (default 150 and 220).
//! - `DERETH_CROWD_BOXES`: the boxes, as `--fidelity` takes them (default the remaster and its
//!   art pack, over the boxes ticked at first).
//! - `DERETH_CROWD_FPS`: frames a second (default 60).
//! - `DERETH_CROWD_CSV`: where each frame's row is written (default none).
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_primitives::num::math;
use dereth_primitives::LandblockId;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::stations::{Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn hex(v: &str) -> u32 {
    u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a hex id")
}

fn number(name: &str, default: f32) -> f32 {
    env(name).map_or(default, |v| v.trim().parse().expect("a number"))
}

/// `block:x,y` as the block and its block-local place.
fn spot(v: &str) -> (u16, f32, f32) {
    let (block, rest) = v.split_once(':').expect("block:x,y");
    let (x, y) = rest.split_once(',').expect("block:x,y");
    #[allow(clippy::cast_possible_truncation)]
    let block = hex(block) as u16;
    (
        block,
        x.trim().parse().expect("a number"),
        y.trim().parse().expect("a number"),
    )
}

/// The place of `(block, x, y)` in `origin`'s block's own coordinates.
fn local_to(origin: u16, (block, x, y): (u16, f32, f32)) -> (f32, f32) {
    let dx = f32::from(block >> 8) - f32::from(origin >> 8);
    let dy = f32::from(block & 0xFF) - f32::from(origin & 0xFF);
    (x + dx * 192.0, y + dy * 192.0)
}

/// The block and block-local place of `(x, y)` in `origin`'s block's coordinates.
fn block_of(origin: u16, (x, y): (f32, f32)) -> (u16, f32, f32) {
    let (bx, by) = ((x / 192.0).floor(), (y / 192.0).floor());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a run of a few blocks at most, inside the map
    let block =
        ((i32::from(origin >> 8) + bx as i32) << 8 | (i32::from(origin & 0xFF) + by as i32)) as u16;
    (block, x - bx * 192.0, y - by * 192.0)
}

/// One creature of the crowd.
struct Creature {
    id: u32,
    kind: (u32, u32),
    at: (f32, f32),
    heading: f32,
    alive: bool,
}

/// What `values` hold at `share` of the way up (0 to 1).
fn rank(values: &[f64], share: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((v.len() - 1) as f64 * share).round() as usize;
    v[i]
}

/// The run, timed; the summary printed and the rows written.
#[test]
#[ignore = "instrument: times a run past crowds of creatures; run with --ignored"]
fn time_a_run_past_crowds_of_creatures() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let packs: Vec<std::path::PathBuf> = env("DERETH_ART_PACKS")
        .map(|v| {
            v.split(';')
                .filter(|s| !s.trim().is_empty())
                .map(|s| std::path::PathBuf::from(s.trim()))
                .collect()
        })
        .unwrap_or_default();
    let opened = Instant::now();
    let pack =
        (!packs.is_empty()).then(|| Arc::new(ArtPack::open_stack(&packs).expect("the packs open")));
    println!(
        "packs: {} opened in {:.0} ms",
        packs.len(),
        opened.elapsed().as_secs_f64() * 1000.0
    );
    let kinds: Vec<(u32, u32)> = env("DERETH_CROWD_KINDS")
        .unwrap_or_else(|| "02000001:09000001".into())
        .split(',')
        .map(|k| {
            let (s, m) = k.split_once(':').expect("setup:mtable");
            (hex(s), hex(m))
        })
        .collect();
    let from = spot(&env("DERETH_CROWD_FROM").unwrap_or_else(|| "A9B4:96,96".into()));
    let to = spot(&env("DERETH_CROWD_TO").unwrap_or_else(|| "ABB4:96,96".into()));
    let origin = from.0;
    let a = local_to(origin, from);
    let b = local_to(origin, to);
    let speed = number("DERETH_CROWD_SPEED", 11.0);
    let fps = number("DERETH_CROWD_FPS", 60.0);
    let near = number("DERETH_CROWD_NEAR", 150.0);
    let far = number("DERETH_CROWD_FAR", 220.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let groups = number("DERETH_CROWD_GROUPS", 9.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let group = number("DERETH_CROWD_GROUP", 8.0) as u32;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = (dx / length, dy / length);
    // Facing along the run: yaw is from north, turning west.
    let yaw = math::atan2f(-ux, uy);
    // The crowd: each group beside the run, on alternate sides, in a ring.
    let mut crowd = Vec::new();
    for g in 0..groups {
        #[allow(clippy::cast_precision_loss)]
        let along = length * (g as f32 + 0.5) / groups as f32;
        let side = if g % 2 == 0 { 22.0 } else { -22.0 };
        let center = (a.0 + ux * along + uy * side, a.1 + uy * along - ux * side);
        for i in 0..group {
            #[allow(clippy::cast_precision_loss)]
            let turn = std::f32::consts::TAU * i as f32 / group as f32;
            let kind = kinds[((g * 2 + i % 2) as usize) % kinds.len()];
            crowd.push(Creature {
                id: 0x8300_1000 + g * 64 + i,
                kind,
                at: (
                    center.0 + 4.0 * math::cosf(turn),
                    center.1 + 4.0 * math::sinf(turn),
                ),
                heading: turn.to_degrees(),
                alive: false,
            });
        }
    }
    let size = super::remaster_frames::size();
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let station = Station {
        name: "crowd",
        why: "a run past crowds",
        place: Place::Ground {
            block: from.0,
            x: from.1,
            y: from.2,
            yaw: yaw.to_degrees(),
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 8,
    };
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.frame_seconds = 1.0 / f64::from(fps);
    shot.follow = Some([4.0, 2.2, -0.2, yaw]);
    shot.poll(&mut gpu);
    if let Some(p) = pack.as_ref() {
        shot.scene.draw.set_remaster_pack(Some(Arc::clone(p)), None);
    }
    let boxes = env("DERETH_CROWD_BOXES").unwrap_or_else(|| "Remaster=1,RemasterArtPack=1".into());
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(&store), &region)
        .expect("the land source opens");
    let ground_at = |p: (f32, f32)| {
        let (block, x, y) = block_of(origin, p);
        land.ground_height(LandblockId(block), x, y).unwrap_or(0.0)
    };
    // Settled at the start: the ground and the far trees made, as a player's would be.
    shot.place_body(origin, a, ground_at(a), yaw);
    shot.step(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..120 {
        shot.step(&mut gpu);
    }
    let mut csv = String::from(
        "frame,ms,sync,update,stream,draw,end,walk,remaster,gpu,gpu_skin,gpu_accel,pose,\
         made_structures,refits,alive,made,deleted\n",
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (length / speed * fps) as u32;
    let mut cpu = Vec::new();
    let mut gpu_totals = Vec::new();
    let mut creations = Vec::new();
    let counts = |shot: &Shot, gpu: &mut Gpu| {
        let posing = shot
            .scene
            .draw
            .remaster_counts(gpu)
            .map_or(0, |c| c.skin_nanos);
        let a = shot
            .scene
            .draw
            .remaster_acceleration(gpu)
            .unwrap_or_default();
        (posing, a.made, a.refits)
    };
    let mut last = counts(&shot, &mut gpu);
    for frame in 0..frames {
        #[allow(clippy::cast_precision_loss)]
        let along = speed * frame as f32 / fps;
        let p = (a.0 + ux * along, a.1 + uy * along);
        shot.place_body(origin, p, ground_at(p), yaw);
        let (mut made, mut deleted) = (0, 0);
        for c in &mut crowd {
            let d = ((c.at.0 - p.0).powi(2) + (c.at.1 - p.1).powi(2)).sqrt();
            if !c.alive && d < near {
                let (block, x, y) = block_of(origin, c.at);
                shot.create_object(c.id, (c.kind.0, Some(c.kind.1)), block, (x, y), c.heading);
                c.alive = true;
                made += 1;
            } else if c.alive && d > far {
                shot.delete_object(c.id);
                c.alive = false;
                deleted += 1;
            }
        }
        shot.step(&mut gpu);
        let l = shot.last;
        // What the client spends: the upload arena's reserve is this harness's own (it waits for
        // the device to go idle).
        let total = l.sync + l.update + l.stream + l.draw + l.end;
        let costs = shot.scene.draw.frame_costs();
        let timings = shot.scene.draw.remaster_timings(&mut gpu);
        let device: f64 = timings.iter().map(|t| t.1).sum();
        let of = |name: &str| -> f64 { timings.iter().filter(|t| t.0 == name).map(|t| t.1).sum() };
        let now = counts(&shot, &mut gpu);
        #[allow(clippy::cast_precision_loss)]
        let posing = now.0.saturating_sub(last.0) as f64 / 1e6;
        let alive = crowd.iter().filter(|c| c.alive).count();
        writeln!(
            csv,
            "{frame},{total:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{device:.3},{:.3},{:.3},\
             {posing:.3},{},{},{alive},{made},{deleted}",
            l.sync,
            l.update,
            l.stream,
            l.draw,
            l.end,
            costs.walk,
            costs.remaster,
            of("skinning"),
            of("acceleration"),
            now.1.saturating_sub(last.1),
            now.2.saturating_sub(last.2),
        )
        .expect("a row");
        last = now;
        cpu.push(total);
        gpu_totals.push(device);
        if made > 0 {
            creations.push(format!("{frame}:{total:.1}ms/{made}"));
        }
    }
    let median = rank(&cpu, 0.5);
    let over = cpu.iter().filter(|v| **v > 2.0 * median).count();
    println!(
        "frames {frames}: processor ms median {median:.2} p95 {:.2} p99 {:.2} max {:.2}; \
         over twice the median {over}; device ms median {:.2} p99 {:.2} max {:.2}",
        rank(&cpu, 0.95),
        rank(&cpu, 0.99),
        rank(&cpu, 1.0),
        rank(&gpu_totals, 0.5),
        rank(&gpu_totals, 0.99),
        rank(&gpu_totals, 1.0),
    );
    creations.truncate(40);
    println!(
        "frames that made creatures (frame:ms/made): {}",
        creations.join(" ")
    );
    if let Some(path) = env("DERETH_CROWD_CSV") {
        std::fs::write(&path, csv).expect("the rows are written");
        println!("rows: {path}");
    }
}
