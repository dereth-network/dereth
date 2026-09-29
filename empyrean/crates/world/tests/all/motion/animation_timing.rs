//! Divergence: V352
//! Server get_animation_length/get_cycle_length/get_attack_frames equal the client animation
//! crate's sequence playback over every motion table in the retail portal dat.
//! Fixture: the retail portal dat and client animation playback.

// V352.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dereth_animation::data::AnimAssets;
use dereth_animation::{HookKind, MotionState, MotionTable, Sequence};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::{MotionCommand, MotionStance};
use empyrean_world::physics::motion::ServerAnimAssets;
use empyrean_world::physics::motion_table as mt;
use empyrean_world::World;

fn dats() -> Arc<DatManager> {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    Arc::clone(DATS.get_or_init(|| {
        let dir = dereth_dat::testing::dat_dir();
        let real = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
    }))
}

fn real_world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, dats())
}

/// The playback step: 1/4096 s, so a time read off it is within two steps of the truth.
const DT: f64 = 1.0 / 4096.0;

/// The client playing `motion` from the stance's default motion, frame by frame: the time until
/// it has finished the motion's animations (the done count the sequence reports) and the times of
/// the attack hooks it fires meanwhile.
fn client_playback(
    table: &MotionTable,
    assets: &dyn AnimAssets,
    style: u32,
    motion: u32,
    speed: f32,
) -> Option<(f64, Vec<f64>)> {
    let sub = table.style_default(dereth_animation::MotionCommand(style))?;
    let mut state = MotionState {
        style: dereth_animation::MotionCommand(style),
        substate: sub,
        ..MotionState::new()
    };
    let mut seq = Sequence::new();
    let n = table.get_object_sequence(
        dereth_animation::MotionCommand(motion),
        &mut state,
        &mut seq,
        speed,
        false,
        assets,
    )?;
    let (mut t, mut done, mut attacks, mut out) = (0.0, 0, Vec::new(), Vec::new());
    while done < n {
        out.clear();
        seq.update(DT, None, &mut out);
        t += DT;
        for h in &out {
            match h.kind {
                HookKind::AnimationDone => done += 1,
                HookKind::Attack(_) if done < n => attacks.push(t),
                _ => {}
            }
        }
        if t > 60.0 {
            return None;
        }
    }
    Some((t, attacks))
}

/// ACE's `GetAnimationLength(AnimData)`: `(HighFrame - LowFrame) / |Framerate|`, `HighFrame` -1 or
/// past the end being the frame count.
#[allow(clippy::cast_possible_wrap)]
fn ace_anim_length(w: &World, a: &dereth_assets::motion::AnimData) -> f64 {
    let nf = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::Animation>(a.anim_id.0)
        .map_or(0, |x| x.num_frames) as i32;
    let high = if a.high_frame == -1 || a.high_frame > nf {
        nf
    } else {
        a.high_frame
    };
    f64::from(high - a.low_frame) / f64::from(a.framerate.abs())
}

/// ACE's `GetAnimationLength(motionTableId, stance, motion, speed)` and `GetAttackFrames`' times.
fn ace_timing(w: &World, id: u32, style: u32, motion: u32, speed: f32) -> (f64, Vec<f64>) {
    let table = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
    let default = mt::get_default_motion(table.as_deref(), MotionStance(style));
    let anims = mt::get_anim_data(
        table.as_deref(),
        MotionStance(style),
        MotionCommand(motion),
        default,
    );
    let length = anims.iter().map(|a| ace_anim_length(w, a)).sum::<f64>() / f64::from(speed);
    let (mut total, mut at) = (0_u32, Vec::new());
    for a in &anims {
        let Some(animation) = w
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::Animation>(a.anim_id.0)
        else {
            continue;
        };
        for frame in &animation.part_frames {
            at.extend(
                frame
                    .hooks
                    .iter()
                    .filter(|h| h.hook_type == 3)
                    .map(|_| f64::from(total)),
            );
            total += 1;
        }
    }
    (
        length,
        at.into_iter()
            .map(|n| n / f64::from(total) * length)
            .collect(),
    )
}

#[derive(Default)]
struct Stat {
    n: usize,
    differ: usize,
    max: f64,
    worst: String,
}

impl Stat {
    fn add(&mut self, diff: f64, tol: f64, what: impl FnOnce() -> String) {
        self.n += 1;
        if diff.is_nan() || diff.abs() > tol {
            self.differ += 1;
        }
        if diff.abs() > self.max {
            self.max = diff.abs();
            self.worst = what();
        }
    }

    fn line(&self, name: &str) -> String {
        format!(
            "{name}: {} samples, {} differ, max {:.4} at {}",
            self.n, self.differ, self.max, self.worst
        )
    }
}

#[test]
#[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
fn server_motion_timings_match_client_playback_on_every_retail_motion_table() {
    let w = real_world();
    let dats = dats();
    let assets = ServerAnimAssets::new(Arc::clone(&dats));
    let mut ids: Vec<u32> = dats
        .portal_dat()
        .all_files()
        .into_iter()
        .filter(|id| id >> 24 == 0x09)
        .collect();
    ids.sort_unstable();
    assert!(!ids.is_empty(), "the portal dat supplies motion tables");

    let tol = DT * 2.0;
    let (mut length, mut cycle, mut attack_count, mut attack) = (
        Stat::default(),
        Stat::default(),
        Stat::default(),
        Stat::default(),
    );
    let (mut ace_length, mut ace_attack_count, mut ace_attack) =
        (Stat::default(), Stat::default(), Stat::default());
    let (mut ace_action_diffs, mut ace_attack_diffs, mut lost_actions) =
        (Vec::new(), Vec::new(), Vec::<String>::new());
    // Every motion whose attack hook count ACE got wrong is a substate: no swing loses its strike.
    for &id in &ids {
        let Some(data) = assets.motion_table(DataId(id)) else {
            continue;
        };
        let table = MotionTable::new(Arc::clone(&data));
        for (&style, &sub) in &data.style_defaults {
            let key = (style << 16) | (sub.0 & 0x00FF_FFFF);
            let motions: Vec<u32> = data
                .links
                .get(&key)
                .map(|l| l.keys().copied().collect())
                .unwrap_or_default();
            for &motion in &motions {
                for speed in [1.0_f32, 1.25] {
                    let Some((client, hooks)) =
                        client_playback(&table, &assets, style, motion, speed)
                    else {
                        continue;
                    };
                    let ours = f64::from(mt::get_animation_length(
                        &w,
                        id,
                        MotionStance(style),
                        MotionCommand(motion),
                        speed,
                    ));
                    let (ace, ace_hooks) = ace_timing(&w, id, style, motion, speed);
                    let at = || format!("{id:08X} {style:08X} {motion:08X} x{speed}");
                    length.add(ours - client, tol, || {
                        format!("{}: ours {ours:.4} client {client:.4}", at())
                    });
                    if motion & 0x1000_0000 != 0 && speed == 1.0 && (ace - client).abs() > tol {
                        ace_action_diffs.push(client - ace);
                    }
                    ace_length.add(ace - client, tol, || {
                        format!("{}: ace {ace:.4} client {client:.4}", at())
                    });

                    let frames =
                        mt::get_attack_frames(&w, id, MotionStance(style), MotionCommand(motion));
                    let ours_hooks: Vec<f64> =
                        frames.iter().map(|(f, _)| f64::from(*f) * ours).collect();
                    attack_count.add(ours_hooks.len() as f64 - hooks.len() as f64, 0.5, || {
                        format!("{}: ours {ours_hooks:?} client {hooks:?}", at())
                    });
                    for (o, c) in ours_hooks.iter().zip(&hooks) {
                        attack.add(o - c, tol, || {
                            format!("{}: ours {o:.4} client {c:.4}", at())
                        });
                    }
                    if speed == 1.0 {
                        ace_attack_count.add(
                            ace_hooks.len() as f64 - hooks.len() as f64,
                            0.5,
                            || format!("{}: ace {ace_hooks:?} client {hooks:?}", at()),
                        );
                        if ace_hooks.len() != hooks.len() && motion & 0x1000_0000 != 0 {
                            lost_actions.push(format!(
                                "{} ace {} client {}",
                                at(),
                                ace_hooks.len(),
                                hooks.len()
                            ));
                        }
                        for (a, c) in ace_hooks.iter().zip(&hooks) {
                            ace_attack_diffs.push(c - a);
                            ace_attack
                                .add(a - c, tol, || format!("{}: ace {a:.4} client {c:.4}", at()));
                        }
                    }
                }
            }
        }
        for (&key, md) in &data.cycles {
            let nodes: Vec<_> = md
                .anims
                .iter()
                .filter_map(|a| dereth_animation::AnimSequenceNode::new(*a, &assets))
                .collect();
            if nodes.iter().any(|n| n.framerate == 0.0) {
                continue; // a cycle that never ends has no length to compare
            }
            let style = 0x8000_0000 | (key >> 16);
            let motion = 0x4000_0000 | (key & 0x00FF_FFFF);
            let client: f64 = nodes
                .iter()
                .map(dereth_animation::AnimSequenceNode::play_time)
                .sum();
            let ours = f64::from(mt::get_cycle_length(
                &w,
                id,
                MotionStance(style),
                MotionCommand(motion),
                1.0,
            ));
            cycle.add(ours - client, 1e-4, || {
                format!("{id:08X} {key:08X}: ours {ours:.4} client {client:.4}")
            });
        }
    }
    println!("motion tables: {}", ids.len());
    for (name, s) in [
        ("link length", &length),
        ("cycle length", &cycle),
        ("attack hook count", &attack_count),
        ("attack hook time", &attack),
        ("ACE link length", &ace_length),
        ("ACE attack hook count", &ace_attack_count),
        ("ACE attack hook time", &ace_attack),
    ] {
        println!("{}", s.line(name));
    }
    for (name, mut d) in [
        (
            "ACE action lengths that differ, client minus ACE",
            ace_action_diffs,
        ),
        ("ACE attack hook times, client minus ACE", ace_attack_diffs),
    ] {
        d.sort_by(f64::total_cmp);
        if let (Some(lo), Some(hi)) = (d.first(), d.last()) {
            let q = |f: f64| d[((d.len() - 1) as f64 * f) as usize];
            println!(
                "{name}: {} values, min {lo:.4} p10 {:.4} median {:.4} p90 {:.4} max {hi:.4}",
                d.len(),
                q(0.1),
                q(0.5),
                q(0.9)
            );
        }
    }
    assert!(
        lost_actions.is_empty(),
        "actions whose attack hook count differs from ACE's: {lost_actions:?}"
    );
    assert!(
        length.n > 0 && cycle.n > 0 && attack.n > 0,
        "the sample covers the dat"
    );
    for (name, s) in [
        ("link length", &length),
        ("cycle length", &cycle),
        ("attack hook count", &attack_count),
        ("attack hook time", &attack),
    ] {
        assert_eq!(s.differ, 0, "{}", s.line(name));
    }
}
