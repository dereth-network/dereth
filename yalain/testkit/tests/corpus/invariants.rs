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

use std::collections::BTreeMap;

use auberean_protocol::ServerMessage;
use rusqlite::{Connection, OpenFlags};
use yalain_testkit::invariants::Checker;

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

/// The land as the walls read it and the install's rooms, as a world's checks see them.
fn known_world() -> (yalain_world::walls::Walls, yalain_world::reported::Known) {
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::DatFile;
    use auberean_world::records::WorldRecords;

    let dir = auberean_dat::testing::require_target_install();
    let open_records = || {
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
        WorldRecords::new(portal, cell, None, ctx)
    };
    let records = open_records();
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
        let meters = match check(travel, Some(&known), from, &to, t) {
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
