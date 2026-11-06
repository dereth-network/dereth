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

/// Bodies put at the reports of the 2017 sessions' steady runs (outdoors, or inside the rooms of
/// the interiors), each moving as the report asks at its full speed: the motion values' speeds
/// times the speed scale the server last sent for the player in that run of the client, whichever
/// of its sessions sent it, and not times the size its look is drawn at. Each is ticked four times
/// and the tick nearest the next report's distance along the ground is judged. A run is judged only
/// when its step before the pair already moved whole ticks at full speed.
fn replay(indoor: bool) -> Tally {
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

    // Per run of the client (a capture: its login and world sessions, in order): the player's id
    // and its speed scale, which a later session of the run does not send again; per session, its
    // last reports.
    let mut capture = -1i64;
    let mut session = -1i64;
    let mut player = 0u64;
    let mut scale = 1.0f32;
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
        ],
        &[2017],
        |m| {
            if tally.pairs >= limit {
                return;
            }
            if m.capture != capture {
                capture = m.capture;
                player = 0;
                scale = 1.0;
                speeds.clear();
                motions.clear();
            }
            if m.session != session {
                session = m.session;
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
            // landblock, outdoors or in its rooms.
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
            let in_rooms = |c: u32| c & 0xFFFF >= 0x100;
            if !(0.12..0.35).contains(&dt)
                || a.cell >> 16 != b.cell >> 16
                || in_rooms(a.cell) != indoor
                || in_rooms(b.cell) != indoor
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
            if !*built.entry((bx, by)).or_insert_with(|| {
                let land_there = land.add_block(&records, &region, &mut trees, bx, by);
                if indoor {
                    auberean_physics::interior::add_block_cells(&land, &records, &mut trees, bx, by)
                        > 0
                } else {
                    land_there
                }
            }) {
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
            *by_scale.entry(format!("{mid:08X} {scale:.3}")).or_default() += 1;
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
    tally
}

/// The shares a replay's judged pairs must reach: within 3 mm, within a tick's travel, on the same
/// ground (height within 5 cm), heading the reported way within the heading byte's width.
fn judge(tally: &Tally, least: u64, shares: [f64; 4]) {
    let n = tally.pairs;
    #[allow(clippy::cast_precision_loss)]
    let share = |k: u64| k as f64 / n as f64;
    assert!(n > least, "{n} judged pairs");
    assert!(
        share(tally.within_3mm) >= shares[0],
        "distance within 3 mm for {} of {n}",
        tally.within_3mm
    );
    assert!(
        share(tally.within_tick) >= shares[1],
        "distance within a tick for {} of {n}",
        tally.within_tick
    );
    assert!(
        share(tally.z_within_5cm) >= shares[2],
        "height within 5 cm for {} of {n}",
        tally.z_within_5cm
    );
    assert!(
        share(tally.direction_within_byte) >= shares[3],
        "direction within a byte for {} of {n}",
        tally.direction_within_byte
    );
}

/// Outdoors, a body driven by a recorded report reaches the next report's position after a whole
/// number of physics ticks: within 3 mm for three pairs in four, within one tick's travel for 99 in
/// 100, on the same ground (height within 5 cm) for 9 in 10, heading the reported way within the
/// heading byte's width for 85 in 100.
///
/// Behaviour: motion.corpus.a-body-driven-by-recorded-reports-reaches-the-next-report
#[test]
fn a_body_driven_by_recorded_reports_reaches_the_next_report() {
    // Measured over the 2017 corpus: 307,245 pairs judged, 78.7% within 3 mm, 99.7% within a
    // tick of travel, 93.3% on the same ground, 89.7% heading the reported way. The rest are mostly
    // on steep ground.
    judge(&replay(false), 300_000, [0.75, 0.99, 0.90, 0.85]);
}

/// Inside, in the rooms of the interiors (their floors, steps and ramps, and what stands in them),
/// a body driven by a recorded report reaches the next report's position after a whole number of
/// physics ticks: within 3 mm for four pairs in five, within one tick's travel for 99 in 100, on the
/// same ground for 9 in 10, heading the reported way for 9 in 10.
///
/// Behaviour: motion.corpus.a-body-driven-by-recorded-reports-indoors-reaches-the-next-report
#[test]
fn a_body_driven_by_recorded_reports_indoors_reaches_the_next_report() {
    // Measured over the 2017 corpus: 4,492 pairs judged, 83.6% within 3 mm, 99.7% within a tick,
    // 92.4% on the same ground, 96.2% heading the reported way.
    judge(&replay(true), 4_000, [0.80, 0.99, 0.90, 0.90]);
}

/// No recorded player stands outdoors on a block the world's map says may not be walked on:
/// the 2017 sessions' reports from the land cover thousands of blocks, and every one is on a block
/// the map lets a body walk on, though the map shuts more blocks than it opens.
///
/// Behaviour: motion.corpus.no-recorded-player-stands-on-a-block-the-map-shuts
#[test]
fn no_recorded_player_stands_on_a_block_the_map_shuts() {
    let (_records, region) = world().expect("the September 2013 install's world opens");
    let index = auberean_testkit::data::require_capture_index();
    let (mut reports, mut shut) = (0u64, 0u64);
    let mut blocks = std::collections::BTreeSet::new();
    auberean_testkit::captures::messages(&index, &[CLIENT_POSITION], &[2017], |m| {
        if m.to_client {
            return;
        }
        let Ok(ClientMessage::Position(b)) = ClientMessage::decode(&m.raw) else {
            return;
        };
        if b.cell & 0xFFFF >= 0x100 {
            return;
        }
        let (x, y) = (b.cell >> 24, (b.cell >> 16) & 0xFF);
        reports += 1;
        blocks.insert((x, y));
        if !region.passable(x, y) {
            shut += 1;
        }
    })
    .expect("the capture index reads");
    assert_eq!(shut, 0);
    assert!(reports > 1_000_000, "{reports} reports");
    assert!(blocks.len() > 3_000, "{} blocks", blocks.len());
    let mut shut_blocks = 0;
    for x in 0..255 {
        for y in 0..255 {
            if !region.passable(x, y) {
                shut_blocks += 1;
            }
        }
    }
    assert!(shut_blocks > 30_000, "{shut_blocks} blocks the map shuts");
}

/// Falling in the rooms of block `0x0497` whose own properties make gravity 3 m/s2, recorded
/// players fall at 3 m/s2, not the world's 9.8: in the 2017 sessions, between reports 0.05 to 0.45 s
/// apart, of a player going down faster at the second, half of the accelerations lie within 0.5 of
/// 3, in over a thousand samples from those rooms.
///
/// Behaviour: motion.corpus.recorded-players-fall-by-a-rooms-own-gravity
#[test]
fn recorded_players_fall_by_a_rooms_own_gravity() {
    let index = auberean_testkit::data::require_capture_index();
    let in_rooms = |c: u32| c >> 16 == 0x0497 && (0x0195..=0x01DC).contains(&(c & 0xFFFF));
    let mut session = -1i64;
    let mut last: Vec<ClientPosition> = Vec::new();
    let mut seen = Vec::new();
    auberean_testkit::captures::messages(&index, &[CLIENT_POSITION], &[2017], |m| {
        if m.to_client {
            return;
        }
        let Ok(ClientMessage::Position(b)) = ClientMessage::decode(&m.raw) else {
            return;
        };
        if m.session != session {
            session = m.session;
            last.clear();
        }
        last.push(b);
        let n = last.len();
        if n < 3 {
            return;
        }
        let (a, b, c) = (last[n - 3], last[n - 2], last[n - 1]);
        let (d1, d2) = (b.time - a.time, c.time - b.time);
        if !(0.05..0.45).contains(&d1) || !(0.05..0.45).contains(&d2) || !in_rooms(a.cell) {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        let (v1, v2) = (
            (b.origin.z - a.origin.z) / d1 as f32,
            (c.origin.z - b.origin.z) / d2 as f32,
        );
        if v1 < -0.3 && v2 < v1 {
            #[allow(clippy::cast_possible_truncation)]
            seen.push((v2 - v1) / ((d1 + d2) as f32 * 0.5));
        }
    })
    .expect("the capture index reads");
    seen.sort_by(f32::total_cmp);
    assert!(seen.len() > 1_000, "{} samples", seen.len());
    let (q1, q3) = (seen[seen.len() / 4], seen[seen.len() * 3 / 4]);
    assert!(q1 > -3.5 && q3 < -2.5, "the middle half: {q1} to {q3}");
}
