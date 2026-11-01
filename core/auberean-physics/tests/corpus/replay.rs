//! Bodies driven by the recorded sessions' own movement reports.
//!
//! The client reports where the player's body is (its position at the last physics tick), which
//! way it faces, and what movement the player is asking for, when the asking changes and otherwise
//! every 0.2 s or so while it moves. Between two reports of the same steady run, nothing changes
//! but the ticks: a body put where the first report says, moving as it asks at its full speed, must
//! reach the second report's position after a whole number of ticks.

use dereth_primitives::num::math;
use std::collections::{BTreeMap, HashMap};

use auberean_assets::animation::{MotionDesc, MotionParams};
use auberean_assets::setup::Setup;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_physics::body::{setup_spheres, Body, BodyConfig};
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const PLAYER_OBJECT: u32 = 0x10060;
const VELOCITY_SCALE: u32 = 0x10092;
const CLIENT_POSITION: u32 = 0x1008E;
const CREATE_OBJECT: u32 = 0x10058;
const UPDATE_VISUAL: u32 = 0x1009F;
/// The human male's setup, whose collision spheres the replayed body takes.
const HUMAN_SETUP: u32 = 0x0400_0029;
const PLAYER_MOTION: u32 = 0x3200_0000;
const EARTH: u32 = 0x4000_0001;
const PEACE: u32 = 0x4000_0001;
/// The report flag of a jump.
const JUMP: u32 = 0x2;

fn world() -> Option<(WorldRecords, RegionSet)> {
    let dir = require_target_install();
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .ok()?
        .into_iter()
        .filter_map(|(n, p)| DatFile::open(&p).ok().map(|d| (n, d)))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal)?;
    let cell = take(&DatName::Cell(1))?;
    let highres = take(&DatName::HighRes);
    let ctx = Context::from_portal(&portal).ok()?;
    let records = WorldRecords::new(portal, cell, highres, ctx);
    let region = records.region().ok()?;
    Some((records, region))
}

/// A motion byte as the request it carries: 0 to 255 over -1 to 1, the middle byte exactly 0.
fn motion(b: u32) -> f32 {
    let b = b & 0xFF;
    if b == 0x7F {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let v = b as f32 / 127.5 - 1.0;
        v.clamp(-1.0, 1.0)
    }
}

/// The heading a report's top byte gives, degrees clockwise from north.
fn heading(word: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let b = (word >> 24) as f32;
    b * 360.0 / 255.0
}

#[derive(Debug, Default)]
struct Tally {
    pairs: u64,
    /// Steady pairs not judged: the body was not moving at the speed it asked for.
    slowed: u64,
    within_3mm: u64,
    within_1cm: u64,
    within_tick: u64,
    z_within_5cm: u64,
    direction_within_byte: u64,
    errors: Vec<f32>,
    worst: Vec<(f32, String)>,
}

/// A body put at a report of a 2017 session's steady run, moving as it asks at its full speed (the
/// motion values' speeds times its speed scale and its size), reaches the next report's position
/// after a whole number of physics ticks: within 3 mm for three pairs in five, within one tick's
/// travel for 98 in 100, on the same ground (height within 5 cm) for 85 in 100, heading the
/// reported way within the heading byte's width for 80 in 100. A run is judged only when its step
/// before the pair already moved whole ticks at full speed.
///
/// Behaviour: motion.corpus.a-body-driven-by-recorded-reports-reaches-the-next-report
#[test]
fn a_body_driven_by_recorded_reports_reaches_the_next_report() {
    let index = auberean_testkit::data::require_capture_index();
    let (records, region) = world().expect("the September 2013 install's world opens");
    let setup: Setup = records.get(HUMAN_SETUP).expect("the human setup");
    let md: MotionDesc = records
        .get(PLAYER_MOTION)
        .expect("the player's motion description");
    let values = |element: u32| -> MotionParams {
        let (_, set) = md
            .set_of
            .iter()
            .find(|(k, _)| k.element == element && k.mode == PEACE)
            .expect("a set");
        md.sets[*set as usize].default
    };
    let config = BodyConfig {
        spheres: setup_spheres(&setup),
        step_up: setup.step_up_height,
        step_down: setup.step_down_height,
        scale: 1.0,
        ground: values(EARTH),
        air: values(0x4000_0002),
    };
    let land = Land::new();
    let mut trees = Trees::default();
    let mut built: HashMap<(u8, u8), bool> = HashMap::new();
    let mut tally = Tally::default();

    // Per session: the player's id, its speed scale, and its last reports.
    let mut session = -1i64;
    let mut player = 0u64;
    let mut scale = 1.0f32;
    // Each object's own scale along its axes, from its description and the records it starts from.
    let mut sizes: HashMap<u64, Vec3> = HashMap::new();
    // Each object's motion description: its description's, else the first its records name.
    let mut motions: HashMap<u64, u32> = HashMap::new();
    let motion_of = |v: &auberean_protocol::types::VisualDesc| -> u32 {
        if v.bits & auberean_protocol::types::VisualDesc::MOTION_INTERP != 0 && v.motion_interp != 0
        {
            return v.motion_interp;
        }
        let mut next = [v.did, v.parent]
            .into_iter()
            .filter(|&d| d != 0)
            .collect::<Vec<_>>();
        let mut seen = 0;
        while let Some(d) = next.pop() {
            seen += 1;
            if seen > 32 {
                break;
            }
            let Some(rec) = records.get::<auberean_assets::visual::VisualDesc>(d) else {
                continue;
            };
            if let Some(m) = rec.motion_desc {
                return m.0;
            }
            if let Some(p) = rec.parent {
                next.push(p.0);
            }
        }
        PLAYER_MOTION
    };
    let mut configs: HashMap<u32, Option<BodyConfig>> = HashMap::new();
    // Each object's speed scale as it was created.
    let mut speeds: HashMap<u64, f32> = HashMap::new();
    let size_of = |v: &auberean_protocol::types::VisualDesc| -> Vec3 {
        let mut s = if v.bits & auberean_protocol::types::VisualDesc::SCALE != 0 {
            Vec3::new(v.scale.x, v.scale.y, v.scale.z)
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        };
        let mut next = [v.did, v.parent]
            .into_iter()
            .filter(|&d| d != 0)
            .collect::<Vec<_>>();
        let mut seen = 0;
        while let Some(d) = next.pop() {
            seen += 1;
            if seen > 32 {
                break;
            }
            let Some(rec) = records.get::<auberean_assets::visual::VisualDesc>(d) else {
                continue;
            };
            if let Some(k) = rec.scale {
                s = Vec3::new(s.x * k.x, s.y * k.y, s.z * k.z);
            }
            if let Some(p) = rec.parent {
                next.push(p.0);
            }
        }
        s
    };
    let mut history: Vec<ClientPosition> = Vec::new();
    let mut by_scale: BTreeMap<String, u64> = BTreeMap::new();
    let limit = u64::MAX;
    auberean_testkit::captures::messages(
        &index,
        &[
            PLAYER_OBJECT,
            VELOCITY_SCALE,
            CLIENT_POSITION,
            CREATE_OBJECT,
            UPDATE_VISUAL,
        ],
        &[2017],
        |m| {
            if tally.pairs >= limit {
                return;
            }
            if m.session != session {
                session = m.session;
                player = 0;
                scale = 1.0;
                sizes.clear();
                speeds.clear();
                motions.clear();
                history.clear();
            }
            if m.to_client {
                match ServerMessage::decode(&m.raw) {
                    Ok(ServerMessage::PlayerObject(p)) => {
                        player = p.id;
                        if let Some(v) = speeds.get(&player) {
                            scale = *v;
                        }
                    }
                    Ok(ServerMessage::VelocityScale(e)) => {
                        speeds.insert(e.sender.id, e.action.0);
                        if e.sender.id == player {
                            scale = e.action.0;
                        }
                    }
                    Ok(ServerMessage::CreateObject(c)) => {
                        sizes.insert(c.id, size_of(&c.visual));
                        motions.insert(c.id, motion_of(&c.visual));
                        if c.physics.bits & auberean_protocol::types::PhysicsDesc::VELOCITY_SCALE
                            != 0
                        {
                            speeds.insert(c.id, c.physics.velocity_scale);
                            if c.id == player {
                                scale = c.physics.velocity_scale;
                            }
                        }
                    }
                    Ok(ServerMessage::UpdateVisualDesc(e)) => {
                        sizes.insert(e.sender.id, size_of(&e.action));
                    }
                    _ => {}
                }
                return;
            }
            let Ok(ClientMessage::Position(b)) = ClientMessage::decode(&m.raw) else {
                return;
            };
            history.push(b);
            if history.len() < 4 {
                return;
            }
            let n = history.len();
            let (a, b) = (history[n - 2], history[n - 1]);
            // A steady run: the same request and heading for the last three reports, no jump, one
            // landblock, outdoors.
            let steady = history[n - 3..]
                .iter()
                .all(|p| p.heading_motion == a.heading_motion && p.flags & JUMP == 0);
            let asked = (
                (a.heading_motion >> 16) & 0xFF,
                (a.heading_motion >> 8) & 0xFF,
            );
            if !steady || asked == (0x7F, 0x7F) {
                return;
            }
            let dt = b.time - a.time;
            if !(0.12..0.35).contains(&dt)
                || a.cell >> 16 != b.cell >> 16
                || a.cell & 0xFFFF > 0x40
                || b.cell & 0xFFFF > 0x40
            {
                return;
            }
            let inside = |p: &ClientPosition| {
                (3.0..157.0).contains(&p.origin.x) && (3.0..157.0).contains(&p.origin.y)
            };
            if !inside(&a) || !inside(&b) {
                return;
            }
            let (bx, by) = ((a.cell >> 24) as u8, ((a.cell >> 16) & 0xFF) as u8);
            if !*built
                .entry((bx, by))
                .or_insert_with(|| land.add_block(&records, &region, &mut trees, bx, by))
            {
                return;
            }
            let input = MotionInput {
                sideways: motion(a.heading_motion >> 16),
                forward: motion(a.heading_motion >> 8),
                turn: 0.0,
            };
            let h = heading(a.heading_motion);
            let start = Position::new(
                CellId(a.cell),
                Frame::new(
                    Vec3::new(a.origin.x, a.origin.y, a.origin.z),
                    Quat::IDENTITY,
                ),
            );
            let mid = motions.get(&player).copied().unwrap_or(PLAYER_MOTION);
            let Some(config) = configs
                .entry(mid)
                .or_insert_with(|| {
                    let md: MotionDesc = records.get(mid)?;
                    let pick = |element: u32| {
                        md.set_of
                            .iter()
                            .find(|(k, _)| k.element == element && k.mode == PEACE)
                            .map(|(_, set)| md.sets[*set as usize].default)
                    };
                    Some(BodyConfig {
                        ground: pick(EARTH)?,
                        air: pick(0x4000_0002).or(pick(EARTH))?,
                        ..config.clone()
                    })
                })
                .clone()
            else {
                return;
            };
            let mut body = Body::new(config, start, h);
            body.minterp.velocity_scale = scale;
            body.minterp.scale = sizes
                .get(&player)
                .copied()
                .unwrap_or(Vec3::new(1.0, 1.0, 1.0));
            body.steer(input);
            // Already running: at the speed it asks for.
            let want = body
                .minterp
                .local_desired(&body, body.minterp.asked, false, false);
            let (s, c) = math::sin_cosf(h.to_radians());
            body.velocity = Vec3::new(want.x * c + want.y * s, -want.x * s + want.y * c, 0.0);
            // Judged only when the step before this pair already moved a whole number of ticks at
            // full speed: a body slowed (pressed against something, in a crowd) asks for full speed
            // without moving at it.
            let tick_travel = (want.x * want.x + want.y * want.y).sqrt() * 0.1;
            let p = history[n - 3];
            let before =
                ((a.origin.x - p.origin.x).powi(2) + (a.origin.y - p.origin.y).powi(2)).sqrt();
            let k = (before / tick_travel).round();
            if k < 1.0 || (before - k * tick_travel).abs() > 0.01 * k {
                tally.slowed += 1;
                return;
            }
            let travel = |p: Vec3| ((p.x - a.origin.x).powi(2) + (p.y - a.origin.y).powi(2)).sqrt();
            let target = travel(Vec3::new(b.origin.x, b.origin.y, b.origin.z));
            let (mut best, mut best_z) = (f32::MAX, f32::MAX);
            for _ in 0..4 {
                body.tick(&land, 0.1);
                let p = body.position.frame.origin;
                let e = (travel(p) - target).abs();
                if e < best {
                    best = e;
                    best_z = (p.z - b.origin.z).abs();
                }
            }
            tally.pairs += 1;
            *by_scale
                .entry(format!(
                    "{mid:08X} {scale:.3} x {:.3}",
                    body.minterp.scale.y
                ))
                .or_default() += 1;
            tally.errors.push(best);
            if best < 0.003 {
                tally.within_3mm += 1;
            }
            if best < 0.01 {
                tally.within_1cm += 1;
            }
            let speed = (want.x * want.x + want.y * want.y).sqrt();
            if best < speed * 0.1 {
                tally.within_tick += 1;
            }
            if best_z < 0.05 {
                tally.z_within_5cm += 1;
            }
            let dir = math::atan2f(b.origin.x - a.origin.x, b.origin.y - a.origin.y).to_degrees();
            let local = math::atan2f(input.sideways, input.forward).to_degrees();
            let off = (dir - h - local).rem_euclid(360.0);
            if off < 360.0 / 255.0 + 0.2 || off > 359.8 {
                tally.direction_within_byte += 1;
            }
            if best > 0.01 && tally.worst.len() < 12 {
                tally.worst.push((
                    best,
                    format!(
                        "session {session} #{}: {:08X} {:?} -> {:?} dt {dt:.3} scale {scale}",
                        m.index, a.cell, a.origin, b.origin
                    ),
                ));
            }
        },
    )
    .expect("the capture index reads");
    let n = tally.pairs;
    let mut e = tally.errors.clone();
    e.sort_by(f32::total_cmp);
    let pct = |p: f64| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        e[((e.len() - 1) as f64 * p) as usize]
    };
    eprintln!(
        "{} steady pairs not judged: slowed below the speed asked for",
        tally.slowed
    );
    eprintln!(
        "{n} steady pairs: distance within 3 mm {} ({:.1}%), 1 cm {}, one tick {}; height within 5 cm {}; direction within a byte {}; error median {:.4} m, 95% {:.4}, 99% {:.4}, max {:.3}; speed scales {by_scale:?}",
        tally.within_3mm,
        100.0 * tally.within_3mm as f64 / n as f64,
        tally.within_1cm,
        tally.within_tick,
        tally.z_within_5cm,
        tally.direction_within_byte,
        pct(0.5),
        pct(0.95),
        pct(0.99),
        e[e.len() - 1]
    );
    for (err, at) in &tally.worst {
        eprintln!("  {err:.3} m: {at}");
    }
    // Measured over the 2017 corpus: 64.6% within 3 mm, 98.4% within a tick of travel, 87.8% on
    // the same ground, 83.8% heading the reported way. The rest are mostly on steep ground.
    let share = |k: u64| k as f64 / n as f64;
    assert!(n > 90_000, "{n} judged pairs");
    assert!(
        share(tally.within_3mm) >= 0.60,
        "distance within 3 mm for {} of {n}",
        tally.within_3mm
    );
    assert!(
        share(tally.within_tick) >= 0.98,
        "distance within a tick for {} of {n}",
        tally.within_tick
    );
    assert!(
        share(tally.z_within_5cm) >= 0.85,
        "height within 5 cm for {} of {n}",
        tally.z_within_5cm
    );
    assert!(
        share(tally.direction_within_byte) >= 0.80,
        "direction within a byte for {} of {n}",
        tally.direction_within_byte
    );
}
