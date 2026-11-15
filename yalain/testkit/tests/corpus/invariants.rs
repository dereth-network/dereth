//! The world connections' invariants held on the January 2017 recordings themselves: every
//! message about an object names one the client was told of and has not forgotten, and each
//! object's stamps only move forward.
//!
//! A client keeps what it knows across the hand-offs between world servers (a new world
//! connection does not make again what the last one made), and forgets everything when its
//! player leaves the world; so the check runs per capture (one run of the client), in time order
//! across its world connections, starting again at each player made.
//!
//! The recorded reports of a player's place stand as a world holds reports, in the same order.
//!
//! And they are followed by players' bodies on the server: every report of a player's place is
//! followed by a body of the player's own (the human's collision spheres and step heights) through
//! the September 2013 world's land, rooms and what the install stands on them, and the corrections
//! it would send are counted against those the 2017 servers sent. Each capture is one run of a
//! client: its player is the character it entered as, or, for a capture begun after that, the
//! object the server told at the very place the client reported. Where the recorded server put that
//! player (a trip's end, a correction, a new instance) the body is put too. A report the body
//! reaches is taken; one it does not is not, and two seconds after the first such report with none
//! reached since, the client would be told where the body stands: a correction. The recorded client
//! never heard ours, so the body is put at its next report after one, as a client that heard it
//! would have reported from there.

use std::collections::BTreeMap;

use auberean_protocol::{ClientMessage, ServerMessage};
use rusqlite::{Connection, OpenFlags};
use yalain_store::StoredPosition;
use yalain_testkit::invariants::Checker;
use yalain_world::walls::{Following, Walls};

fn open(path: &std::path::Path) -> Connection {
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(uri, flags).expect("the capture index opens read-only")
}

/// Every 2017 world connection's server messages, in the order the client took them, keep the
/// known-object and stamp invariants, apart from the breaches tallied and explained.
/// Behaviour: server.replay.recorded-world-invariants
#[test]
fn the_recorded_2017_world_connections_keep_the_object_and_stamp_invariants() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT s.capture_id, m.raw, m.session_id FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND s.world_login = 1 AND c.file LIKE '2017-%' \
             AND m.status = 'ac2' AND m.dir = 's2c' ORDER BY s.capture_id, m.t, m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = stmt.query([]).expect("runs");
    let mut checked: BTreeMap<String, usize> = BTreeMap::new();
    let mut breaches: BTreeMap<String, usize> = BTreeMap::new();
    let mut sessions = 0usize;
    let mut current = None;
    let mut c = Checker::new();
    // Before a capture has seen its player enter, the client holds objects the recording never
    // saw made (the capture began in mid-session).
    let mut entered = false;
    let mut flush = |c: &mut Checker, entered: bool| {
        for (k, n) in std::mem::take(&mut c.checked) {
            *checked.entry(k).or_default() += n;
        }
        for (k, n) in std::mem::take(&mut c.breaches) {
            let k = if entered {
                k
            } else {
                format!("{k} (the capture began in mid-session)")
            };
            *breaches.entry(k).or_default() += n;
        }
    };
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("id");
        let raw: Vec<u8> = r.get(1).expect("raw");
        if current != Some(sid) {
            flush(&mut c, entered);
            c = Checker::new();
            entered = false;
            current = Some(sid);
            sessions += 1;
        }
        c.on_connection(r.get(2).expect("session"));
        if let Ok(m) = ServerMessage::decode(&raw) {
            if matches!(m, ServerMessage::PlayerObject(_)) {
                flush(&mut c, entered);
                c = Checker::new();
                entered = true;
            }
            c.see(&m);
        }
    }
    flush(&mut c, entered);
    println!("{sessions} captures' 2017 world connections; messages about an object, by kind:");
    for (k, n) in &checked {
        println!("  {n:9}  {k}");
    }
    println!("breaches:");
    for (k, n) in &breaches {
        println!("  {n:9}  {k}");
    }
    assert!(sessions > 0);
}

/// Every report of a player's place recorded from 2005 to 2017 stands as a world holds reports: a
/// land cell holds its point.
/// Behaviour: server.places.a-land-cell-holds-its-point
#[test]
fn recorded_reports_of_a_players_place_hold_their_land_cells() {
    use auberean_protocol::ClientMessage;
    use yalain_world::world::land_cell_holds;

    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND c.file NOT LIKE '2009-%' AND m.mtype = 65678",
        )
        .expect("prepares");
    let mut rows = stmt.query([]).expect("runs");
    let (mut reports, mut on_land, mut wrong_cell) = (0u64, 0u64, 0u64);
    while let Some(row) = rows.next().expect("a row") {
        let raw: Vec<u8> = row.get(0).expect("a message");
        let Ok(ClientMessage::Position(c)) = ClientMessage::decode(&raw) else {
            panic!("a report that does not read");
        };
        reports += 1;
        if c.cell & 0xFFFF < 0x100 {
            on_land += 1;
            wrong_cell += u64::from(!land_cell_holds(c.cell, [c.origin.x, c.origin.y]));
        }
    }
    assert_eq!(reports, 1_825_084, "the reports recorded");
    assert_eq!(on_land, 1_503_957, "of which on the land");
    assert_eq!(wrong_cell, 0);
}

/// The install's world records, opened anew.
pub(crate) fn world_records() -> auberean_world::records::WorldRecords {
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::DatFile;

    let dir = auberean_dat::testing::require_target_install();
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .map(|(n, p)| (n, DatFile::open(&p).expect("open")))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal).expect("a portal file");
    let cell = take(&DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    auberean_world::records::WorldRecords::new(portal, cell, None, ctx)
}

/// The land as the walls read it and the install's rooms, as a world's checks see them.
fn known_world() -> (yalain_world::walls::Walls, yalain_world::reported::Known) {
    let records = world_records();
    let rooms = yalain_world::reported::Rooms::read(&records);
    let region = records.region().expect("the region");
    let land = std::sync::Arc::new(auberean_physics::land::Land::new());
    let walls = yalain_world::walls::Walls::new(
        land.clone(),
        auberean_physics::world::World::new(),
        Box::new(yalain_world::walls::RecordBlocks::new(records, region)),
    );
    (walls, yalain_world::reported::Known { land, rooms })
}

/// Every report of a player's place recorded in January 2017 passes the checks a world holds
/// reports to: its reach in the time since the last place taken, steady going, the rooms' doorways
/// and sight, the land. Each capture is one run of a client: its player is the character it
/// entered as, and where the server put that player (a trip's end, a place told back, a new
/// instance) starts the reckoning again from there and then.
/// Behaviour: server.places.recorded-reports-pass-the-checks
#[test]
fn every_2017_report_of_a_players_place_passes_the_checks_a_world_holds_reports_to() {
    use auberean_protocol::ClientMessage;
    use yalain_store::StoredPosition;
    use yalain_world::reported::{check, Travel};

    let (mut walls, known) = known_world();
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT s.capture_id, s.t_first + m.t, m.raw FROM message m \
             JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND ((m.dir = 'c2s' AND m.mtype IN (65678, 65579)) OR (m.dir = 's2c' AND m.mtype = 65670)) \
             ORDER BY s.capture_id, s.t_first + m.t, s.idx, m.idx",
        )
        .expect("prepares");
    let mut rows = stmt.query([]).expect("runs");
    let place_of = |cell: u32, o: auberean_primitives::Vec3| StoredPosition {
        cell,
        origin: [o.x, o.y, o.z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    // Whether stamps `a` are older than `b` in any part (each wraps).
    #[allow(clippy::cast_possible_wrap)]
    let older =
        |a: [u16; 3], b: [u16; 3]| a.iter().zip(b).any(|(x, y)| (x.wrapping_sub(y) as i16) < 0);
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut refused: BTreeMap<String, u64> = BTreeMap::new();
    let mut capture = None;
    let mut me = None;
    // The place last taken or given, the stamps it came with, and the reckoning since.
    let mut held: Option<(StoredPosition, [u16; 3], Travel)> = None;
    while let Some(row) = rows.next().expect("a row") {
        let id: i64 = row.get(0).expect("a capture");
        let t: f64 = row.get(1).expect("a time");
        let raw: Vec<u8> = row.get(2).expect("a message");
        if capture != Some(id) {
            capture = Some(id);
            me = None;
            held = None;
        }
        if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&raw) {
            me = Some(p.character);
            held = None;
            continue;
        }
        if let Ok(ServerMessage::Position(p)) = ServerMessage::decode(&raw) {
            if Some(p.sender.id) != me {
                continue;
            }
            let stamps = [
                p.sender.instance_stamp,
                p.teleport_stamp,
                p.force_position_stamp,
            ];
            if held.as_ref().is_none_or(|h| h.1 != stamps) {
                *counts.entry("places the server gave").or_default() += 1;
                held = Some((place_of(p.cell, p.origin), stamps, Travel::placed(t)));
            }
            continue;
        }
        let Ok(ClientMessage::Position(c)) = ClientMessage::decode(&raw) else {
            continue;
        };
        *counts.entry("reports").or_default() += 1;
        let to = place_of(c.cell, c.origin);
        let stamps = [c.instance_stamp, c.teleport_stamp, c.force_position_stamp];
        let Some((from, at, travel)) = held.as_mut() else {
            *counts
                .entry("first of a run, before any place given")
                .or_default() += 1;
            held = Some((to, stamps, Travel::placed(t)));
            continue;
        };
        if *at != stamps {
            if older(stamps, *at) {
                *counts.entry("from before a place given").or_default() += 1;
            } else {
                *counts
                    .entry("at stamps no place given was seen with")
                    .or_default() += 1;
                held = Some((to, stamps, Travel::placed(t)));
            }
            continue;
        }
        *counts.entry("checked").or_default() += 1;
        if c.cell & 0xFFFF < 0x100 {
            walls.read_place(c.cell);
        }
        let meters = match check(travel, Some(&known), from, &to, t, 1.0) {
            Ok(m) => m,
            Err(why) => {
                let k = format!(
                    "{why:?} {:08X} -> {:08X}, {:.1} m in {:.2} s",
                    from.cell,
                    to.cell,
                    yalain_world::reported::meters(from, &to),
                    t - travel.since()
                );
                eprintln!("refused: capture {id}: {k}");
                *refused.entry(format!("{why:?}")).or_default() += 1;
                yalain_world::reported::meters(from, &to)
            }
        };
        travel.take(meters, t);
        *from = to;
    }
    println!("2017 reports of a player's place:");
    for (k, n) in &counts {
        println!("  {n:9}  {k}");
    }
    println!("refused: {refused:?}; blocks read {}", walls.blocks_read());
    assert_eq!(counts.get("reports"), Some(&1_806_692));
    assert_eq!(refused, BTreeMap::new());
}

/// The human male's setup, whose collision spheres and step heights a player's body takes.
const HUMAN_SETUP: u32 = 0x0400_0029;

/// How near in time a correction of ours and one the recorded server sent count as the same, in
/// seconds.
const SAME_TIME: f64 = 3.0;

/// The blocks where the recorded clients were carried by something not identified, and corrected
/// there by the recorded servers (`3A73` to `3C75`, and the rooms of `07F9`).
fn carried(cell: u32) -> bool {
    let (x, y) = (cell >> 24, (cell >> 16) & 0xFF);
    ((0x3A..=0x3C).contains(&x) && (0x73..=0x75).contains(&y)) || cell >> 16 == 0x07F9
}

/// The install's walls, with players given the human's body.
fn walls() -> Walls {
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::DatFile;
    use auberean_world::records::WorldRecords;

    let dir = auberean_dat::testing::require_target_install();
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .map(|(n, p)| (n, DatFile::open(&p).expect("open")))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal).expect("a portal file");
    let cell = take(&DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    let records = WorldRecords::new(portal, cell, None, ctx);
    let region = records.region().expect("the region");
    let mut world = auberean_physics::world::World::new();
    if let Some(list) = records.get::<auberean_assets::property::PropertyList>(0x3400_0000) {
        world.set_property_list(list);
    }
    let human =
        yalain_world::walls::setup_walker(&records, HUMAN_SETUP, 1.0).expect("the human's body");
    let mut w = Walls::new(
        std::sync::Arc::new(auberean_physics::land::Land::new()),
        world,
        Box::new(yalain_world::walls::RecordBlocks::new(records, region)),
    );
    w.player_walker = human;
    w
}

/// A correction: the capture, when, where the body stood and how far the client's last report was
/// from it, and what led up to it.
#[derive(Debug, Clone)]
pub(crate) struct Correction {
    pub capture: i64,
    pub t: f64,
    pub cell: u32,
    pub apart: f32,
    pub kind: &'static str,
}

/// What a replay counted.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    pub counts: BTreeMap<&'static str, u64>,
    pub ours: Vec<Correction>,
    pub retail: Vec<Correction>,
    /// The follows made, and the moves through the transition they took.
    pub follows: u64,
    pub moves: u64,
}

fn meters(a: &StoredPosition, b: &StoredPosition) -> f32 {
    let (ax, ay) = (a.cell >> 24, (a.cell >> 16) & 0xFF);
    let (bx, by) = (b.cell >> 24, (b.cell >> 16) & 0xFF);
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (
        (bx as f32 - ax as f32) * 160.0 + b.origin[0] - a.origin[0],
        (by as f32 - ay as f32) * 160.0 + b.origin[1] - a.origin[1],
    );
    let dz = b.origin[2] - a.origin[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// What led a body to fall behind: the report it first missed.
fn kind_of(missed_far: bool, jumped: bool, from: u32, to: u32) -> &'static str {
    if missed_far {
        "too far to follow"
    } else if jumped {
        "jumping"
    } else if from >> 16 != to >> 16 {
        "across a block edge"
    } else if from != to && from & 0xFFFF >= 0x100 && to & 0xFFFF >= 0x100 {
        "room to room"
    } else if from != to && (from & 0xFFFF >= 0x100) != (to & 0xFFFF >= 0x100) {
        "between a room and the land"
    } else if from != to {
        "land cell to land cell"
    } else if from & 0xFFFF >= 0x100 {
        "in a room"
    } else {
        "on the land"
    }
}

/// Replay every 2017 report through players' bodies. Without `follow`, every body is put at each
/// report instead, the walls reading what a follow would read, so that the replay's own work can be
/// told from the follows'. `trace`, when given, replays that capture alone and prints each report
/// the body missed.
pub(crate) fn replay(follow: bool, trace: Option<i64>) -> Tally {
    let mut walls = walls();
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT s.capture_id, s.t_first + m.t, m.raw FROM message m \
             JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND ((m.dir = 'c2s' AND m.mtype IN (65678, 65579)) OR (m.dir = 's2c' AND m.mtype = 65670)) \
             AND (?1 IS NULL OR s.capture_id = ?1) \
             ORDER BY s.capture_id, s.t_first + m.t, s.idx, m.idx",
        )
        .expect("prepares");
    let mut rows = stmt.query([trace]).expect("runs");
    let place_of = |cell: u32, o: auberean_primitives::Vec3| StoredPosition {
        cell,
        origin: [o.x, o.y, o.z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    // Whether stamps `a` are older than `b` in any part (each wraps).
    #[allow(clippy::cast_possible_wrap)]
    let older =
        |a: [u16; 3], b: [u16; 3]| a.iter().zip(b).any(|(x, y)| (x.wrapping_sub(y) as i16) < 0);
    let mut t = Tally::default();
    let mut capture = None;
    let mut me: Option<u64> = None;
    // The stamps of the place last given or taken, the reckoning, the last report, whether the body
    // is put at the next report, and what the first report missed was.
    let mut stamps: Option<[u16; 3]> = None;
    let mut following = Following::default();
    let mut last: Option<StoredPosition> = None;
    let mut anchor_next = false;
    let mut last_jump = f64::NEG_INFINITY;
    let mut first_miss: Option<&'static str> = None;
    // Before the player is known: the place the client last reported.
    let mut unowned: Option<StoredPosition> = None;
    while let Some(row) = rows.next().expect("a row") {
        let id: i64 = row.get(0).expect("a capture");
        let now: f64 = row.get(1).expect("a time");
        let raw: Vec<u8> = row.get(2).expect("a message");
        if capture != Some(id) {
            capture = Some(id);
            if let Some(m) = me.take() {
                walls.forget_player(m);
            }
            stamps = None;
            unowned = None;
        }
        if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&raw) {
            if let Some(m) = me.replace(p.character) {
                walls.forget_player(m);
            }
            stamps = None;
            continue;
        }
        let Some(player) = me else {
            match (ClientMessage::decode(&raw), ServerMessage::decode(&raw)) {
                (Ok(ClientMessage::Position(c)), _) => {
                    *t.counts.entry("reports").or_default() += 1;
                    *t.counts.entry("before the player is known").or_default() += 1;
                    unowned = Some(place_of(c.cell, c.origin));
                }
                (_, Ok(ServerMessage::Position(p)))
                    if unowned.is_some_and(|u| {
                        u.cell == p.cell && u.origin == [p.origin.x, p.origin.y, p.origin.z]
                    }) =>
                {
                    me = Some(p.sender.id);
                }
                _ => {}
            }
            continue;
        };
        // A correction of ours comes due before what arrives next.
        if let Some(due) = following.pull_back_at() {
            if now >= due {
                if let (Some(at), Some(l)) = (walls.player_at(player), last) {
                    t.ours.push(Correction {
                        capture: id,
                        t: due,
                        cell: at.cell,
                        apart: meters(&at, &l),
                        kind: first_miss.unwrap_or("on the land"),
                    });
                }
                following.reached();
                first_miss = None;
                anchor_next = true;
            }
        }
        if let Ok(ServerMessage::Position(p)) = ServerMessage::decode(&raw) {
            if p.sender.id != player {
                continue;
            }
            let given = [
                p.sender.instance_stamp,
                p.teleport_stamp,
                p.force_position_stamp,
            ];
            let Some(had) = stamps.filter(|s| *s != given) else {
                if stamps.is_none() {
                    stamps = Some(given);
                    walls.place_player(player, &place_of(p.cell, p.origin));
                }
                continue;
            };
            let at = place_of(p.cell, p.origin);
            let kind = if had[1] != given[1] {
                "a trip's end"
            } else if had[2] != given[2] {
                "a correction"
            } else {
                "a new instance"
            };
            *t.counts.entry(kind).or_default() += 1;
            if kind == "a correction" {
                if let Some(l) = last {
                    t.retail.push(Correction {
                        capture: id,
                        t: now,
                        cell: at.cell,
                        apart: meters(&at, &l),
                        kind: "retail",
                    });
                }
            }
            walls.place_player(player, &at);
            following.reached();
            first_miss = None;
            anchor_next = false;
            stamps = Some(given);
            continue;
        }
        let Ok(ClientMessage::Position(c)) = ClientMessage::decode(&raw) else {
            continue;
        };
        *t.counts.entry("reports").or_default() += 1;
        let to = place_of(c.cell, c.origin);
        let reported = [c.instance_stamp, c.teleport_stamp, c.force_position_stamp];
        if c.flags & 2 != 0 {
            last_jump = now;
        }
        match stamps {
            Some(s) if s == reported => {}
            Some(s) if older(reported, s) => {
                *t.counts.entry("from before a place given").or_default() += 1;
                continue;
            }
            _ => {
                *t.counts
                    .entry("at stamps no place given was seen with")
                    .or_default() += 1;
                stamps = Some(reported);
                walls.place_player(player, &to);
                following.reached();
                last = Some(to);
                continue;
            }
        }
        if anchor_next {
            *t.counts
                .entry("the first after a correction of ours")
                .or_default() += 1;
            anchor_next = false;
            walls.place_player(player, &to);
            last = Some(to);
            continue;
        }
        let before = walls.player_at(player);
        last = Some(to);
        if !follow {
            if let Some(b) = before {
                walls.read_around(b.cell);
            }
            walls.read_around(to.cell);
            walls.place_player(player, &to);
            continue;
        }
        t.follows += 1;
        let followed = walls.follow_player(player, &to);
        let Some(f) = followed else {
            *t.counts
                .entry("not judged: a cell the walls lack")
                .or_default() += 1;
            walls.place_player(player, &to);
            following.reached();
            first_miss = None;
            continue;
        };
        t.moves += u64::from(f.moves);
        if f.reached {
            *t.counts.entry("reached").or_default() += 1;
            following.reached();
            first_miss = None;
        } else {
            *t.counts.entry("missed").or_default() += 1;
            if trace.is_some() {
                let b = before.unwrap_or(to);
                println!(
                    "missed at {now:.2}: {:08X} {:?} -> {:08X} {:?}, the body at {:08X} {:?}",
                    b.cell, b.origin, to.cell, to.origin, f.at.cell, f.at.origin
                );
            }
            if first_miss.is_none() {
                let from = before.map_or(to.cell, |b| b.cell);
                first_miss = Some(kind_of(f.moves == 0, now - last_jump <= 3.0, from, to.cell));
            }
            following.missed(now);
        }
    }
    t
}

/// The corrections of `a` with one of `b` within [`SAME_TIME`] in the same capture.
fn matched(a: &[Correction], b: &[Correction]) -> usize {
    a.iter()
        .filter(|x| {
            b.iter()
                .any(|y| y.capture == x.capture && (y.t - x.t).abs() <= SAME_TIME)
        })
        .count()
}

/// The replay's figures, as the instrument and the test print them.
pub(crate) fn print(t: &Tally) {
    println!("2017 reports followed by players' bodies:");
    for (k, n) in &t.counts {
        println!("  {n:9}  {k}");
    }
    let (ours, retail) = (&t.ours, &t.retail);
    println!(
        "corrections: ours {}, retail {}; ours with retail's within {SAME_TIME} s {}, retail's with ours {}",
        ours.len(),
        retail.len(),
        matched(ours, retail),
        matched(retail, ours)
    );
    let bands = [(0.0, 2.0), (2.0, 10.0), (10.0, 50.0), (50.0, f32::MAX)];
    let row = |name: &str, v: &[&Correction]| {
        let n: Vec<usize> = bands
            .iter()
            .map(|(lo, hi)| v.iter().filter(|c| c.apart >= *lo && c.apart < *hi).count())
            .collect();
        println!(
            "  {name:32} {:6} {:6} {:6} {:6}  {:6}",
            n[0],
            n[1],
            n[2],
            n[3],
            v.len()
        );
    };
    println!(
        "  {:32} {:>6} {:>6} {:>6} {:>6}  {:>6}",
        "kind, by meters apart", "0-2", "2-10", "10-50", "50+", "all"
    );
    let mut kinds: BTreeMap<&str, Vec<&Correction>> = BTreeMap::new();
    for c in ours {
        kinds.entry(c.kind).or_default().push(c);
    }
    for (k, v) in &kinds {
        row(k, v);
    }
    row("ours, all", &ours.iter().collect::<Vec<_>>());
    row(
        "ours, in the carried place",
        &ours.iter().filter(|c| carried(c.cell)).collect::<Vec<_>>(),
    );
    row("retail, all", &retail.iter().collect::<Vec<_>>());
    row(
        "retail, in the carried place",
        &retail
            .iter()
            .filter(|c| carried(c.cell))
            .collect::<Vec<_>>(),
    );
    let mut blocks: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for c in ours {
        blocks.entry(c.cell >> 16).or_default().0 += 1;
    }
    for c in retail {
        blocks.entry(c.cell >> 16).or_default().1 += 1;
    }
    let mut top: Vec<_> = blocks.into_iter().collect();
    top.sort_by_key(|(_, (o, r))| std::cmp::Reverse(o.max(r).to_owned()));
    println!("  blocks with the most (ours, retail):");
    for (b, (o, r)) in top.iter().take(20) {
        println!("    {b:04X}  {o:5} {r:5}");
    }
    println!("  ours, each (capture, time, cell, meters apart, kind; with retail's near):");
    for c in ours {
        let near = retail
            .iter()
            .any(|y| y.capture == c.capture && (y.t - c.t).abs() <= SAME_TIME);
        println!(
            "    {:5} {:10.2} {:08X} {:6.1} {}{}",
            c.capture,
            c.t,
            c.cell,
            c.apart,
            c.kind,
            if near { ", retail's too" } else { "" }
        );
    }
    let sessions: std::collections::BTreeSet<i64> = ours.iter().map(|c| c.capture).collect();
    println!("  captures with a correction of ours: {}", sessions.len());
}

/// Every report of a player's place recorded in January 2017, followed by a body of the player's
/// own through the install's walls: the bodies miss at most 182 of the 1,806,692 reports and would
/// send at most 12 corrections, where the 2017 servers sent 600 to the players known.
/// Behaviour: server.places.recorded-reports-followed-by-a-body
#[test]
fn every_2017_report_of_a_players_place_is_followed_by_a_body_of_its_own() {
    let t = replay(true, None);
    print(&t);
    let count = |k: &str| t.counts.get(k).copied().unwrap_or(0);
    assert_eq!(count("reports"), 1_806_692);
    assert_eq!(t.retail.len(), 600, "the corrections the 2017 servers sent");
    assert!(count("missed") <= 182, "missed {}", count("missed"));
    assert!(t.ours.len() <= 12, "ours {}", t.ours.len());
}
