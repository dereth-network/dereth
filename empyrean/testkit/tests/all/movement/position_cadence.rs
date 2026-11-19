//! ACE: Source/ACE.Server/WorldObjects/Player_Tick.cs::UpdatePlayerPosition
//! Which of a moving player's positions (UpdatePosition `0xF748`) reach the players around it, and
//! when: by the observer's outdoor create window and the mover's cell.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats; flat land, and one
//! interior cell.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::{Animation, MotionTable};
use dereth_primitives::{DataId, Vec3};
use dereth_protocol::movement::{
    MoveTimestamps, MoveToStatePack, MovementMoveToState, RawMotionState,
};
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_entity::enums::{
    MotionCommand as Mc, MotionStance, PropertyDataId, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::Position;
use empyrean_testkit::{ClientId, EmptyShard, TestServer};
use empyrean_world::managers::{guid_manager, player_manager};

use super::player_movement::{
    autonomous_position, enter, fake_dats, outdoor, seed_with, server_with, wire, BLOCKS, HOME,
};

const UPDATE_POSITION: u32 = 0xF748;

/// A position a client received for one object, read off the message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Received {
    pub(crate) cell: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The force-position stamp: a new one tells the player's own client to go there.
    pub(crate) force: u16,
}

/// The positions `id` received for `guid` from message `mark` on.
pub(crate) fn positions_of(ts: &TestServer, id: ClientId, mark: usize, guid: u32) -> Vec<Received> {
    let f = |b: &[u8], at: usize| f32::from_le_bytes(b[at..at + 4].try_into().expect("a float"));
    ts.received_raw(id)[mark..]
        .iter()
        .filter(|m| m.opcode == UPDATE_POSITION && m.body[0..4] == guid.to_le_bytes())
        .map(|m| {
            let b = &m.body;
            let flags = u32::from_le_bytes(b[4..8].try_into().expect("flags"));
            // the rotation's components the flags keep, then the velocity and the placement
            let mut at = 24;
            for bit in [0x8, 0x10, 0x20, 0x40] {
                if flags & bit == 0 {
                    at += 4;
                }
            }
            if flags & 1 != 0 {
                at += 12;
            }
            if flags & 2 != 0 {
                at += 4;
            }
            // the instance, position, teleport and force-position stamps
            Received {
                cell: u32::from_le_bytes(b[8..12].try_into().expect("a cell")),
                x: f(b, 12),
                y: f(b, 16),
                force: u16::from_le_bytes(b[at + 6..at + 8].try_into().expect("a stamp")),
            }
        })
        .collect()
}

/// The players' motion table and its one animation.
pub(crate) const MT: u32 = 0x0900_0320;
const CYCLE: u32 = 0x0300_0220;

/// The fake dats with a motion table that runs at 4 m/s a unit of speed and turns at a quarter
/// turn a second, in the peace stance.
pub(crate) fn dats() -> Arc<empyrean_dat::DatManager> {
    let nc = MotionStance::NonCombat.0;
    let key = |motion: u32| nc.wrapping_shl(16) | (motion & 0xFF_FFFF);
    let data = |motion: u32, velocity: Option<Vec3>, omega: Option<Vec3>| MotionData {
        key: key(motion),
        bitfield: 0,
        flags: 0,
        anims: vec![AnimData {
            anim_id: DataId(CYCLE),
            low_frame: 0,
            high_frame: -1,
            framerate: 30.0,
        }],
        velocity,
        omega,
    };
    let omega = Some(Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2));
    let table = MotionTable {
        id: DataId(MT),
        default_style: nc,
        style_defaults: BTreeMap::from([(nc, Mc::Ready.0)]),
        cycles: vec![
            data(Mc::Ready.0, None, None),
            data(Mc::WalkForward.0, Some(Vec3::new(0.0, 3.12, 0.0)), None),
            data(Mc::RunForward.0, Some(Vec3::new(0.0, 4.0, 0.0)), None),
            MotionData {
                bitfield: 2,
                ..data(Mc::TurnRight.0, None, omega)
            },
        ],
        modifiers: vec![MotionData {
            anims: Vec::new(),
            ..data(Mc::TurnRight.0, None, omega)
        }],
        // a short link into each forward motion, as the retail tables have
        links: BTreeMap::from([(
            key(Mc::Ready.0),
            vec![
                MotionData {
                    key: Mc::WalkForward.0,
                    ..data(Mc::WalkForward.0, Some(Vec3::new(0.0, 3.12, 0.0)), None)
                },
                MotionData {
                    key: Mc::RunForward.0,
                    ..data(Mc::RunForward.0, Some(Vec3::new(0.0, 4.0, 0.0)), None)
                },
            ],
        )]),
    };
    let cycle = Animation {
        id: DataId(CYCLE),
        flags: 0,
        num_parts: 0,
        num_frames: 10,
        has_hooks: false,
        pos_frames: None,
        part_frames: (0..10)
            .map(|_| dereth_assets::geometry::AnimFrame {
                frames: Vec::new(),
                hooks: Vec::new(),
            })
            .collect(),
    };
    fake_dats()
        .with_portal(MT, table)
        .with_portal(CYCLE, cycle)
        .build()
        .expect("fake dats")
}

/// `HoldKey.Run`, `MotionCommand.WalkForward` and `MotionCommand.TurnRight`.
const HOLD_RUN: u32 = 2;
const WALK_FORWARD: u32 = 0x4500_0005;
pub(crate) const TURN_RIGHT: u32 = 0x6500_000D;

/// How fast the fixture's players run, in meters a second: the table's 4 m/s at the run rate
/// Run 100 gives.
pub(crate) const RUN_SPEED: f32 = 4.0 * 23.0 / 12.0;

/// `at` facing north (the way the fixture's players run with no rotation) or south.
pub(crate) fn facing(at: Position, north: bool) -> Position {
    let mut p = at;
    (p.rotation_w, p.rotation_z) = if north { (1.0, 0.0) } else { (0.0, 1.0) };
    p
}

/// Where a fixture player running from `from` the way it faces (north or south) is `secs` on.
pub(crate) fn ahead(from: &Position, secs: f32) -> Position {
    let north = from.rotation_w > 0.5;
    let y = from.position_y + if north { RUN_SPEED } else { -RUN_SPEED } * secs;
    let lb = u16::try_from(from.cell() >> 16).expect("a landblock");
    facing(outdoor(lb, from.position_x, y, from.position_z), north)
}

/// A MoveToState at `at`: running forward when `run`, turning right when `turn`, else standing.
pub(crate) fn move_to_state(at: &Position, run: bool, turn: bool) -> MovementMoveToState {
    MovementMoveToState(MoveToStatePack {
        raw_motion_state: RawMotionState {
            current_holdkey: Some(HOLD_RUN),
            forward_command: run.then_some(WALK_FORWARD),
            forward_holdkey: run.then_some(HOLD_RUN),
            turn_command: turn.then_some(TURN_RIGHT),
            turn_holdkey: turn.then_some(HOLD_RUN),
            ..RawMotionState::default()
        },
        position: wire(at),
        timestamps: MoveTimestamps::default(),
        contact: true,
        longjump_mode: false,
    })
}

/// A server with one character per `(account, guid, at)`, each with the motion table and Run
/// trained, in the world and out of the login bubble; their clients, in order.
pub(crate) fn world(players: &[(&str, u32, Position)]) -> (TestServer, Vec<ClientId>) {
    world_with(players, |_| ())
}

/// [`world`], with `edit` run on the server before anyone is placed.
pub(crate) fn world_with(
    players: &[(&str, u32, Position)],
    edit: impl FnOnce(&mut TestServer),
) -> (TestServer, Vec<ClientId>) {
    let mut ts = server_with(dats());
    edit(&mut ts);
    ts.world.content = Arc::new(MemContent::new().weenie(
        Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ),
    ));
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    for &(account, guid, at) in players {
        seed_with(&ts, account, guid, account, at, |b| {
            b.set_property(PropertyDataId::MotionTable, MT);
            b.properties_skill
                .get_or_insert_with(DotNetDict::new)
                .insert(
                    Skill::Run,
                    PropertiesSkill {
                        init_level: 100,
                        sac: SkillAdvancementClass::Trained,
                        ..PropertiesSkill::default()
                    },
                );
        });
    }
    player_manager::initialize(&mut ts.world);
    let clients = players
        .iter()
        .map(|&(account, guid, at)| enter(&mut ts, account, guid, at).0)
        .collect();
    ts.advance(1.0);
    (ts, clients)
}

mod far_observers {
    //! Divergence: V548
    //! Outdoors, an observer whose create window (the round window of outdoor cells around it) a
    //! player is outside gets its position only when the player enters a new cell; one with the
    //! player in its window gets every position, and so does everyone while the player is indoors.
    use super::*;
    use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Plane, Sphere};
    use empyrean_world::physics::phys_ext;

    const RUNNER: u32 = 0x5000_0001;
    const NEAR: u32 = 0x5000_0002;
    const FAR: u32 = 0x5000_0003;
    const OBSERVER_3: u32 = 0x5000_0004;
    const OBSERVER_4: u32 = 0x5000_0005;

    /// An interior cell of `HOME` that is not seen from outside, like a cellar's.
    const CELLAR: u32 = (HOME as u32) << 16 | 0x0100;

    /// The runner's start, facing north.
    fn start() -> Position {
        facing(outdoor(HOME, 10.0, 10.5, 20.0), true)
    }

    /// Starts the runner north from `from`, then reports where it has run every 0.4 s, `n` times;
    /// answers the positions reported.
    fn run_north_from(
        ts: &mut TestServer,
        runner: ClientId,
        from: Position,
        n: u16,
    ) -> Vec<Position> {
        ts.send_game_action(runner, &move_to_state(&from, true, false));
        ts.advance(0.4);
        (1..=n)
            .map(|k| {
                let at = ahead(&from, 0.4 * f32::from(k));
                ts.send_game_action(runner, &autonomous_position(&at, true));
                ts.advance(0.4);
                at
            })
            .collect()
    }

    /// [`run_north_from`] the runner's [`start`].
    fn run_north(ts: &mut TestServer, runner: ClientId, n: u16) -> Vec<Position> {
        run_north_from(ts, runner, start(), n)
    }

    /// Whether outdoor positions `a` and `b` of `HOME` are in each other's create window: their
    /// outdoor cells differ by (dx, dy) with dx² + dy² ≤ 13.
    fn in_window(a: &Position, b: &Position) -> bool {
        #[allow(clippy::cast_possible_truncation)]
        let cell = |p: &Position| {
            (
                (p.position_x / 24.0).floor() as i32,
                (p.position_y / 24.0).floor() as i32,
            )
        };
        let ((ax, ay), (bx, by)) = (cell(a), cell(b));
        (ax - bx).pow(2) + (ay - by).pow(2) <= 13
    }

    /// The y of each position `id` received for the runner since `mark` that is one of `ys`.
    fn got_of(ts: &TestServer, id: ClientId, mark: usize, ys: &[f32]) -> Vec<f32> {
        positions_of(ts, id, mark, RUNNER)
            .iter()
            .map(|p| p.y)
            .filter(|y| ys.contains(y))
            .collect()
    }

    #[test]
    fn outdoors_an_observer_outside_the_window_gets_a_players_position_only_in_a_new_cell() {
        let (mut ts, clients) = world(&[
            ("runner", RUNNER, start()),
            // 50 m east of the runner, two cells over
            ("near", NEAR, outdoor(HOME, 60.0, 10.5, 20.0)),
            // about 120 m north-east of the runner, three cells over and three up
            ("far", FAR, outdoor(HOME, 95.0, 95.0, 20.0)),
        ]);
        let [runner, near, far] = clients[..] else {
            unreachable!()
        };
        let marks = [near, far].map(|id| ts.received_raw(id).len());

        // the first four reports stay in the runner's cell (y up to 24), the fifth enters the next
        let reported = run_north(&mut ts, runner, 5);
        let ys: Vec<f32> = reported.iter().map(|p| p.position_y).collect();
        assert!(reported[..4].iter().all(|p| p.cell() == start().cell()));
        assert_ne!(reported[4].cell(), start().cell());

        assert_eq!(
            got_of(&ts, near, marks[0], &ys),
            ys,
            "the near observer gets every report"
        );
        assert_eq!(
            got_of(&ts, far, marks[1], &ys),
            [ys[4]],
            "the far observer gets the new cell's alone"
        );
    }

    /// The window's edge, where it and a distance part: cells (0, 3) and (2, 3) away are in it
    /// (the second over 100 m off), cells (0, 4) and (3, 3) away are not (the first under 96 m).
    #[test]
    fn the_window_is_round_in_cells_not_a_distance() {
        // the runner runs north inside cell (2, 2), from its south-west corner
        let from = facing(outdoor(HOME, 48.5, 48.5, 20.0), true);
        let in_0_3 = outdoor(HOME, 48.5, 125.0, 20.0);
        let in_2_3 = outdoor(HOME, 119.5, 143.5, 20.0);
        let out_0_4 = outdoor(HOME, 48.5, 144.5, 20.0);
        let out_3_3 = outdoor(HOME, 125.0, 125.0, 20.0);
        let (mut ts, clients) = world(&[
            ("runner", RUNNER, from),
            ("in03", NEAR, in_0_3),
            ("in23", FAR, in_2_3),
            ("out04", OBSERVER_3, out_0_4),
            ("out33", OBSERVER_4, out_3_3),
        ]);
        let [runner, a, b, c, d] = clients[..] else {
            unreachable!()
        };
        let marks = [a, b, c, d].map(|id| ts.received_raw(id).len());

        let reported = run_north_from(&mut ts, runner, from, 6);
        assert!(reported.iter().all(|p| p.cell() == from.cell()));
        let ys: Vec<f32> = reported.iter().map(|p| p.position_y).collect();
        for p in &reported {
            assert!(p.distance_to(&in_2_3) > 100.0 && p.distance_to(&out_0_4) < 96.0);
            assert!(in_window(p, &in_0_3) && in_window(p, &in_2_3));
            assert!(!in_window(p, &out_0_4) && !in_window(p, &out_3_3));
        }

        assert_eq!(got_of(&ts, a, marks[0], &ys), ys, "(0, 3) is in the window");
        assert_eq!(got_of(&ts, b, marks[1], &ys), ys, "(2, 3) is in the window");
        assert_eq!(
            got_of(&ts, c, marks[2], &ys),
            Vec::<f32>::new(),
            "(0, 4) is out"
        );
        assert_eq!(
            got_of(&ts, d, marks[3], &ys),
            Vec::<f32>::new(),
            "(3, 3) is out"
        );
    }

    #[test]
    fn an_observer_whose_window_the_player_enters_gets_every_position_again() {
        // five cells north of the runner's start
        let observer = outdoor(HOME, 12.0, 130.5, 20.0);
        let (mut ts, clients) = world(&[("runner", RUNNER, start()), ("far", FAR, observer)]);
        let [runner, far] = clients[..] else {
            unreachable!()
        };
        let mark = ts.received_raw(far).len();

        let reported = run_north(&mut ts, runner, 15);
        let ys: Vec<f32> = reported.iter().map(|p| p.position_y).collect();
        let got = got_of(&ts, far, mark, &ys);
        let mut cell = start().cell();
        for p in &reported {
            let new_cell = p.cell() != cell;
            cell = p.cell();
            assert_eq!(
                got.contains(&p.position_y),
                new_cell || in_window(p, &observer),
                "{p:?}: {got:?}"
            );
        }
        // the run passes within 96 m of the observer before it enters the window, and then
        // reports more than once in the window
        assert!(reported
            .iter()
            .any(|p| !in_window(p, &observer) && p.distance_to(&observer) < 96.0));
        assert!(reported.iter().filter(|p| in_window(p, &observer)).count() > 1);
    }

    /// Flat land with [`CELLAR`] added, and the synthetic body.
    fn with_cellar(ts: &mut TestServer) {
        let blocks = BLOCKS.map(dereth_primitives::LandblockId);
        let mut land = empyrean_dat::physics::flat_land_source(&blocks, 10);
        // a room that holds every point above its floor (z 0)
        let above_the_floor = BspTree {
            nodes: vec![BspNode {
                sphere: Sphere::default(),
                splitting_plane: Plane {
                    normal: dereth_primitives::Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            }],
            polygons: Vec::new().into(),
        };
        land.add_cell(dereth_physics::EnvCellGeometry {
            id: dereth_primitives::CellId(CELLAR),
            cell_bsp: Some(Arc::new(above_the_floor)),
            holds_without_portals: true,
            seen_outside: false,
            ..Default::default()
        });
        phys_ext::use_land_source(&mut ts.world, Arc::new(land));
        phys_ext::register_setup(
            &mut ts.world,
            empyrean_testkit::land::TEST_SETUP,
            empyrean_testkit::land::test_setup_geometry(),
        );
    }

    #[test]
    fn indoors_every_observer_gets_every_position() {
        let inside = |y: f32| {
            let mut p = facing(outdoor(HOME, 48.5, y, 20.0), true);
            p.set_landblock_id(empyrean_entity::LandblockId::new(CELLAR));
            p
        };
        // four cells north of the cellar's spot: out of its window were it outdoors
        let observer = outdoor(HOME, 48.5, 144.5, 20.0);
        let (mut ts, clients) = world_with(
            &[("mover", RUNNER, inside(50.0)), ("far", FAR, observer)],
            with_cellar,
        );
        let [mover, far] = clients[..] else {
            unreachable!()
        };
        let mark = ts.received_raw(far).len();

        let ys = [51.0, 52.0, 53.0, 54.0];
        for y in ys {
            ts.send_game_action(mover, &autonomous_position(&inside(y), true));
            ts.advance(0.4);
        }
        let g = empyrean_entity::ObjectGuid::new(RUNNER);
        assert_eq!(
            ts.world
                .objects
                .get(g)
                .and_then(empyrean_world::world_objects::world_object::WorldObject::location)
                .map(|p| (p.cell(), p.position_y)),
            Some((CELLAR, 54.0)),
            "the mover moved in the cellar"
        );
        assert_eq!(got_of(&ts, far, mark, &ys), ys, "every report");
    }
}

mod server_movement {
    //! Divergence: V550
    //! The server moves every player between its reports, its location following: the players
    //! around get that position each second while it changes (the player too every half second,
    //! with the option on). With the motion correction option on, the body is swept through the
    //! world's collision to each of the player's reports as it arrives: a report it reaches is
    //! taken, one it cannot reach sends the player back to where the sweep stopped half a second
    //! later, by any distance; with it off (the default), every report is taken. Reports that are
    //! not numbers are ignored, a teleport ends the body's motion.
    use super::*;
    use empyrean_entity::ObjectGuid;
    use empyrean_world::managers::property_manager;
    use empyrean_world::physics::phys_ext;
    use empyrean_world::world_objects::world_object::WorldObject;

    use dereth_protocol::login::CharacterLoginCompleteNotification;
    use dereth_protocol::movement::MovementAutonomousPosition;

    const RUNNER: u32 = 0x5000_0001;
    const NEAR: u32 = 0x5000_0002;
    const OTHER: u32 = 0x5000_0003;

    /// A landblock far from `HOME`, loaded with it (a teleport's destination).
    const FAR_BLOCK: u16 = 0x2B2C;

    /// The runner's start, facing north.
    fn start() -> Position {
        facing(outdoor(HOME, 50.0, 30.5, 20.0), true)
    }

    fn pair() -> (TestServer, ClientId, ClientId) {
        let (ts, clients) = world(&[
            ("runner", RUNNER, start()),
            ("near", NEAR, outdoor(HOME, 60.0, 30.5, 20.0)),
        ]);
        (ts, clients[0], clients[1])
    }

    /// Where the server has the runner's body now.
    fn body(ts: &TestServer) -> Position {
        let h = phys_ext::physics_obj(&ts.world, ObjectGuid::new(RUNNER)).expect("a body");
        let p = phys_ext::position(&ts.world, h).expect("placed");
        empyrean_world::world_objects::player_tick::physics_position_to_location(&p)
    }

    fn location(ts: &TestServer) -> Position {
        ts.world
            .objects
            .get(ObjectGuid::new(RUNNER))
            .and_then(WorldObject::location)
            .expect("a location")
    }

    /// An AutonomousPosition carrying the force-position stamp `force`.
    fn report(at: &Position, force: u16) -> MovementAutonomousPosition {
        let mut m = autonomous_position(at, true);
        m.0.timestamps.force_position = force;
        m
    }

    /// `at`, `east` meters further east, facing north.
    fn east(at: &Position, east: f32) -> Position {
        facing(
            outdoor(HOME, at.position_x + east, at.position_y, at.position_z),
            true,
        )
    }

    /// Whether the runner's own client has been sent somewhere new since message `own`.
    fn sent_back(ts: &TestServer, runner: ClientId, own: usize) -> bool {
        positions_of(ts, runner, own, RUNNER)
            .iter()
            .any(|p| p.force != 0)
    }

    /// The option that sweeps the body to each report and sends back what it cannot reach.
    const CORRECTION: &str = "server_motion_correction";

    /// [`pair`] with the motion correction on.
    fn corrected_pair() -> (TestServer, ClientId, ClientId) {
        let (mut ts, runner, near) = pair();
        set_option(&mut ts, CORRECTION, true);
        (ts, runner, near)
    }

    /// Sets a server option, and has the world read it at once.
    fn set_option(ts: &mut TestServer, key: &str, value: bool) {
        assert!(property_manager::modify_bool(&ts.world, key, value));
        empyrean_world::world_objects::player_tick::refresh_movement_options(&mut ts.world);
    }

    #[test]
    fn others_get_where_the_server_has_moved_a_running_player_each_second() {
        let (mut ts, runner, near) = pair();
        let own = ts.received_raw(runner).len();
        let mark = ts.received_raw(near).len();
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        let t0 = ts.seconds();

        // no report for 3.5 s: note when each position reached the observer
        let mut got: Vec<(f64, f32)> = Vec::new();
        while ts.seconds() < t0 + 3.5 {
            ts.step();
            let all = positions_of(&ts, near, mark, RUNNER);
            for p in &all[got.len()..] {
                got.push((ts.seconds() - t0, p.y));
            }
        }
        // the MoveToState's own, then one a second as the body runs on
        assert!(got.len() >= 4, "{got:?}");
        assert!(
            (got[0].1 - start().position_y).abs() < 0.01,
            "the MoveToState's own: {got:?}"
        );
        for w in got.windows(2) {
            let ((ta, ya), (tb, yb)) = (w[0], w[1]);
            assert!((0.95..=1.1).contains(&(tb - ta)), "a second apart: {got:?}");
            #[allow(clippy::cast_possible_truncation)]
            let ran = RUN_SPEED * (tb - ta) as f32;
            assert!((yb - ya - ran).abs() < 0.5, "run on: {got:?}");
        }
        assert!(
            positions_of(&ts, runner, own, RUNNER).len() <= 1,
            "the player itself gets only its MoveToState's"
        );
    }

    #[test]
    fn a_player_standing_still_sends_no_heartbeat() {
        let (mut ts, runner, near) = pair();
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(1.5);
        let stop = body(&ts);
        ts.send_game_action(runner, &move_to_state(&stop, false, false));
        ts.advance(1.5);

        let mark = ts.received_raw(near).len();
        ts.advance(3.0);
        assert_eq!(positions_of(&ts, near, mark, RUNNER), Vec::new());
    }

    #[test]
    fn with_its_option_on_the_player_gets_where_the_server_has_moved_it_every_half_second() {
        let (mut ts, runner, _near) = pair();
        set_option(&mut ts, "player_own_position_heartbeat", true);
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(0.1);
        let own = ts.received_raw(runner).len();
        ts.advance(2.05);
        let got = positions_of(&ts, runner, own, RUNNER);
        assert_eq!(got.len(), 4, "{got:?}");
        for w in got.windows(2) {
            assert!(w[1].y > w[0].y, "it runs on: {got:?}");
        }
    }

    /// A static pillar 1.5 m wide (3 m across) standing at `at`.
    fn pillar(ts: &mut TestServer, at: &Position) {
        const PILLAR: u32 = 0x0200_1001;
        phys_ext::register_setup(
            &mut ts.world,
            PILLAR,
            dereth_physics::SetupGeometry {
                spheres: vec![dereth_physics::geom::Sphere::new(
                    dereth_primitives::Vec3::new(0.0, 0.0, 1.5),
                    1.5,
                )],
                sorting_sphere: dereth_physics::geom::Sphere::new(
                    dereth_primitives::Vec3::new(0.0, 0.0, 1.5),
                    2.0,
                ),
                radius: 1.5,
                height: 3.0,
                ..dereth_physics::SetupGeometry::default()
            },
        );
        let h = phys_ext::make_object(&mut ts.world, PILLAR, 0x7000_0001, false);
        assert!(phys_ext::enter_world(
            &mut ts.world,
            h,
            &phys_ext::to_physics_position(at)
        ));
    }

    /// The runner standing at `start()` with a pillar 4 m to its east; it reports 8 m east, past
    /// the pillar. The motion correction is on. Answers the server and the runner's client.
    fn report_past_a_pillar() -> (TestServer, ClientId, ClientId) {
        let (mut ts, runner, near) = corrected_pair();
        pillar(&mut ts, &east(&start(), 4.0));
        ts.advance(0.5);
        (ts, runner, near)
    }

    #[test]
    fn a_report_the_sweep_cannot_reach_sends_the_player_back_to_where_the_sweep_stopped() {
        let (mut ts, runner, near) = report_past_a_pillar();
        let own = ts.received_raw(runner).len();
        let mark = ts.received_raw(near).len();
        let claimed = east(&start(), 8.0);
        ts.send_game_action(runner, &report(&claimed, 0));
        ts.advance(0.6);

        let back = positions_of(&ts, runner, own, RUNNER);
        assert_eq!(back.len(), 1, "{back:?}");
        assert_eq!(back[0].force, 1, "a new force stamp");
        // stopped short of the pillar (its 1.5 m and the body's half meter), on the way east
        let x = back[0].x - start().position_x;
        assert!((1.0..2.1).contains(&x), "where the sweep stopped: {back:?}");
        assert!(
            (body(&ts).position_x - back[0].x).abs() < 0.01,
            "the body stays there"
        );
        assert!((location(&ts).position_x - back[0].x).abs() < 0.01);
        // the held body may reach them on its heartbeat first
        assert_eq!(
            positions_of(&ts, near, mark, RUNNER)
                .last()
                .map(|p| p.force),
            Some(1),
            "the players around see it go back"
        );
    }

    /// The runner standing at `start()` on open land, its body left with `edit` from earlier steps,
    /// reporting 6 m north: the report is taken.
    fn a_clear_report_is_taken_whatever_the_body_was_left_with(
        edit: impl Fn(&mut dereth_physics::PhysicsObj),
    ) {
        let (mut ts, runner, _near) = corrected_pair();
        ts.advance(0.5);
        let own = ts.received_raw(runner).len();
        let h = phys_ext::physics_obj(&ts.world, ObjectGuid::new(RUNNER)).expect("a body");
        if let Some(o) = ts.world.physics.get_mut(h) {
            edit(o);
        }
        let mut north = start();
        north.position_y += 6.0;
        ts.send_game_action(runner, &report(&north, 0));
        ts.advance(0.6);
        assert!(!sent_back(&ts, runner, own), "not sent back");
        assert!((body(&ts).position_y - north.position_y).abs() < 0.01);
    }

    #[test]
    fn a_sweep_is_not_held_by_a_wall_the_body_slid_along_before() {
        // facing north into a wall it left behind
        a_clear_report_is_taken_whatever_the_body_was_left_with(|o| {
            o.sliding_normal = dereth_primitives::Vec3::new(0.0, -1.0, 0.0);
            o.transient_state.set_sliding(true);
        });
    }

    #[test]
    fn a_report_taken_puts_the_body_on_the_ground_where_it_is_now() {
        let (mut ts, runner, _near) = corrected_pair();
        ts.advance(0.5);
        let h = phys_ext::physics_obj(&ts.world, ObjectGuid::new(RUNNER)).expect("a body");
        // a slope the body stood on somewhere else
        if let Some(o) = ts.world.physics.get_mut(h) {
            o.contact_plane = dereth_primitives::shape::Plane {
                normal: dereth_primitives::Vec3::new(0.0, 0.5, 0.866),
                d: -40.0,
            };
        }
        let mut north = start();
        north.position_y += 3.0;
        ts.send_game_action(runner, &report(&north, 0));
        ts.advance(0.6);
        let o = ts.world.physics.get(h).expect("the body");
        assert!(
            (o.contact_plane.normal.z - 1.0).abs() < 1e-3,
            "the flat land under it: {:?}",
            o.contact_plane
        );
        let under = o.contact_plane.normal.dot(o.position.frame.origin) + o.contact_plane.d;
        assert!(under.abs() < 0.1, "through its feet: {under}");
    }

    #[test]
    fn a_body_in_the_air_takes_the_report_unswept() {
        let (mut ts, runner, _near) = report_past_a_pillar();
        let own = ts.received_raw(runner).len();
        // the body 3 m up, falling
        let h = phys_ext::physics_obj(&ts.world, ObjectGuid::new(RUNNER)).expect("a body");
        let mut up = start();
        up.position_z += 3.0;
        assert!(phys_ext::set_position(
            &mut ts.world,
            h,
            &phys_ext::to_physics_position(&up)
        ));
        if let Some(o) = ts.world.physics.get_mut(h) {
            o.transient_state.set_contact(false);
            o.set_on_walkable(false);
        }
        ts.send_game_action(runner, &report(&east(&start(), 8.0), 0));
        ts.advance(0.6);
        assert!(!sent_back(&ts, runner, own), "not sent back");
    }

    #[test]
    fn the_sweep_comes_half_a_second_after_the_report() {
        let (mut ts, runner, _near) = report_past_a_pillar();
        // long after any earlier sweep
        ts.advance(2.0);
        let own = ts.received_raw(runner).len();
        ts.send_game_action(runner, &report(&east(&start(), 8.0), 0));
        ts.advance(0.3);
        assert!(!sent_back(&ts, runner, own), "not yet");
        ts.advance(0.3);
        assert!(sent_back(&ts, runner, own), "half a second on");
    }

    #[test]
    fn a_report_the_sweep_reaches_is_taken_with_nothing_sent_to_the_player() {
        let (mut ts, runner, _near) = report_past_a_pillar();
        let own = ts.received_raw(runner).len();
        // north, clear of the pillar
        let mut claimed = start();
        claimed.position_y += 6.0;
        ts.send_game_action(runner, &report(&claimed, 0));
        ts.advance(0.6);
        assert!((body(&ts).position_y - claimed.position_y).abs() < 0.01);
        assert!((location(&ts).position_y - claimed.position_y).abs() < 0.01);
        assert_eq!(
            positions_of(&ts, runner, own, RUNNER),
            Vec::new(),
            "nothing to the player"
        );
    }

    #[test]
    fn a_turn_in_place_is_taken() {
        let (mut ts, runner, _near) = report_past_a_pillar();
        let own = ts.received_raw(runner).len();
        let turned = facing(start(), false);
        ts.send_game_action(runner, &report(&turned, 0));
        ts.advance(0.6);
        assert_eq!(
            location(&ts).rotation(),
            turned.rotation(),
            "facing the new way"
        );
        assert!(!sent_back(&ts, runner, own));
    }

    #[test]
    fn a_report_far_across_open_land_is_taken_however_far() {
        let (mut ts, runner, _near) = corrected_pair();
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(1.0);

        let own = ts.received_raw(runner).len();
        let claimed = east(&body(&ts), 60.0);
        ts.send_game_action(runner, &report(&claimed, 0));
        ts.advance(0.6);
        assert!((body(&ts).position_x - claimed.position_x).abs() < 0.01);
        assert!(!sent_back(&ts, runner, own));
    }

    /// A running body blocked head-on by something in its path, then put past it by an accepted
    /// report, runs on from there with no new keys. It had stood at each report until new keys,
    /// still pressing against what had blocked it and with no velocity to step by, its reports'
    /// lead growing until the client was pulled back.
    #[test]
    fn a_running_body_blocked_and_then_put_past_the_block_runs_on() {
        // a pillar 1.5 m wide stands 4 m ahead of the runner
        let ahead = facing(outdoor(HOME, 50.0, 34.5, 20.0), true);
        let (mut ts, clients) = world(&[("runner", RUNNER, start())]);
        let runner = clients[0];
        pillar(&mut ts, &ahead);
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(1.0);
        let blocked = body(&ts);
        assert!(
            blocked.position_y < ahead.position_y - 0.5,
            "held short of the pillar: {blocked:?}"
        );
        ts.advance(0.5);
        assert!(
            (body(&ts).position_y - blocked.position_y).abs() < 0.05,
            "still held: {:?}",
            body(&ts)
        );

        // the client slid aside: its report beside the pillar, which the sweep reaches, is taken
        let past = facing(outdoor(HOME, 53.0, 31.5, 20.0), true);
        ts.send_game_action(runner, &report(&past, 0));
        let taken_at = ts.seconds();
        ts.run_until(1.0, |ts| {
            (body(ts).position_x - past.position_x).abs() < 0.01
        });
        ts.advance(0.1 - (ts.seconds() - taken_at).min(0.1));
        let taken = body(&ts).position_y - past.position_y;
        assert!(
            (body(&ts).position_x - past.position_x).abs() < 0.01 && (0.0..1.0).contains(&taken),
            "taken: {:?}",
            body(&ts)
        );
        ts.advance(1.0);
        let ran = body(&ts).position_y - past.position_y;
        assert!(
            ran > RUN_SPEED * 0.7,
            "runs on from the report: {ran} m in 1 s"
        );
    }

    #[test]
    fn reports_sent_before_the_client_took_the_forced_position_are_ignored() {
        let (mut ts, runner, _near) = report_past_a_pillar();
        ts.send_game_action(runner, &report(&east(&start(), 8.0), 0));
        ts.advance(0.6);
        let forced_x = body(&ts).position_x;
        assert!(forced_x - start().position_x < 2.1, "sent back");

        // one more from the client's old spot with the old stamp: ignored
        let own = ts.received_raw(runner).len();
        ts.send_game_action(runner, &report(&east(&start(), 8.5), 0));
        ts.advance(0.6);
        assert_eq!(positions_of(&ts, runner, own, RUNNER), Vec::new());
        assert!((body(&ts).position_x - forced_x).abs() < 0.01);

        // the client where it was sent, with the new stamp: taken
        let mut at = body(&ts);
        at.position_y += 0.3;
        ts.send_game_action(runner, &report(&at, 1));
        ts.advance(0.6);
        assert!((body(&ts).position_y - at.position_y).abs() < 0.01);
    }

    #[test]
    fn by_default_a_report_the_sweep_cannot_reach_sends_the_player_back() {
        let (mut ts, runner, _near) = pair();
        pillar(&mut ts, &east(&start(), 4.0));
        ts.advance(0.5);

        let own = ts.received_raw(runner).len();
        ts.send_game_action(runner, &report(&east(&start(), 8.0), 0));
        ts.advance(0.6);
        assert!(sent_back(&ts, runner, own), "sent back");
        assert!(
            body(&ts).position_x - start().position_x < 4.0,
            "held short of the pillar"
        );
    }

    #[test]
    fn with_the_correction_off_a_report_the_sweep_cannot_reach_is_taken_and_heartbeats_go_on() {
        let (mut ts, runner, near) = pair();
        set_option(&mut ts, CORRECTION, false);
        pillar(&mut ts, &east(&start(), 4.0));
        ts.advance(0.5);

        let own = ts.received_raw(runner).len();
        let claimed = east(&start(), 8.0);
        ts.send_game_action(runner, &report(&claimed, 0));
        ts.advance(0.6);
        assert!(!sent_back(&ts, runner, own), "not sent back");
        assert!((body(&ts).position_x - claimed.position_x).abs() < 0.01);
        assert!((location(&ts).position_x - claimed.position_x).abs() < 0.01);

        ts.send_game_action(runner, &move_to_state(&claimed, true, false));
        ts.advance(0.1);
        let mark = ts.received_raw(near).len();
        ts.advance(1.5);
        assert!(
            !positions_of(&ts, near, mark, RUNNER).is_empty(),
            "the heartbeat goes on"
        );
    }

    #[test]
    fn with_server_movement_off_nobody_is_brought_back_or_sent_a_heartbeat() {
        let (mut ts, runner, near) = pair();
        set_option(&mut ts, "player_server_movement", false);
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(0.1);
        let own = ts.received_raw(runner).len();
        let mark = ts.received_raw(near).len();
        ts.advance(2.5);
        assert_eq!(
            positions_of(&ts, near, mark, RUNNER),
            Vec::new(),
            "no heartbeat"
        );

        let claimed = east(&start(), 17.0);
        ts.send_game_action(runner, &report(&claimed, 0));
        ts.advance(0.1);
        let got = positions_of(&ts, runner, own, RUNNER);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].force, got[0].x), (0, claimed.position_x));
    }

    #[test]
    fn a_report_that_is_not_a_number_is_ignored_and_a_blocked_report_after_it_still_sends_back() {
        let (mut ts, runner, near) = report_past_a_pillar();
        let own = ts.received_raw(runner).len();
        let mark = ts.received_raw(near).len();
        let mut bad = east(&start(), 1.0);
        bad.position_x = f32::NAN;
        ts.send_game_action(runner, &report(&bad, 0));
        ts.advance(0.6);
        assert_eq!(
            positions_of(&ts, runner, own, RUNNER),
            Vec::new(),
            "not taken"
        );
        assert!(positions_of(&ts, near, mark, RUNNER)
            .iter()
            .all(|p| p.x.is_finite()));
        assert!(location(&ts).position_x.is_finite() && body(&ts).position_x.is_finite());

        ts.send_game_action(runner, &report(&east(&start(), 8.0), 0));
        ts.advance(0.6);
        assert!(sent_back(&ts, runner, own), "sent back");
    }

    #[test]
    fn a_move_to_state_that_is_not_a_number_is_ignored() {
        let (mut ts, runner, near) = pair();
        let mark = ts.received_raw(near).len();
        let mut bad = start();
        bad.position_y = f32::INFINITY;
        ts.send_game_action(runner, &move_to_state(&bad, true, false));
        ts.advance(1.5);
        assert!(
            !ts.received_raw(near)[mark..]
                .iter()
                .any(|m| m.opcode == 0xF74C && m.body[0..4] == RUNNER.to_le_bytes()),
            "no motion goes out"
        );
        assert!((body(&ts).position_y - start().position_y).abs() < 0.01);
    }

    /// Flat land over the fixture's landblocks and the row north of them.
    fn with_north_row(ts: &mut TestServer) {
        let mut blocks: Vec<dereth_primitives::LandblockId> = BLOCKS
            .iter()
            .copied()
            .map(dereth_primitives::LandblockId)
            .collect();
        blocks.extend([0xA8B6, 0xA9B6, 0xAAB6].map(dereth_primitives::LandblockId));
        let land = empyrean_dat::physics::flat_land_source(&blocks, 10);
        phys_ext::use_land_source(&mut ts.world, Arc::new(land));
        phys_ext::register_setup(
            &mut ts.world,
            empyrean_testkit::land::TEST_SETUP,
            empyrean_testkit::land::test_setup_geometry(),
        );
    }

    #[test]
    fn a_client_that_falls_silent_mid_run_is_followed_into_the_next_landblock() {
        // from near the north edge of HOME, north into the next landblock
        let from = facing(outdoor(HOME, 50.0, 180.0, 20.0), true);
        let (mut ts, clients) = world_with(
            &[
                ("runner", RUNNER, from),
                // two landblocks north of HOME, next to the one the runner enters
                ("north", NEAR, outdoor(0xA9B6, 50.0, 20.0, 20.0)),
                // the landblock south of HOME: two from the one the runner enters
                ("south", OTHER, outdoor(0xA9B3, 50.0, 100.0, 20.0)),
            ],
            with_north_row,
        );
        let [runner, north, south] = clients[..] else {
            unreachable!()
        };
        let marks = [north, south].map(|id| ts.received_raw(id).len());

        // the run starts, then the client says nothing more
        ts.send_game_action(runner, &move_to_state(&from, true, false));
        ts.advance(10.0);

        let ran = body(&ts);
        assert_eq!(
            ran.cell() >> 16,
            0xA9B5,
            "the body ran into the next landblock"
        );
        assert!(ran.position_y > 50.0, "{ran:?}");
        assert!(
            location(&ts).cell() >> 16 == 0xA9B5
                && location(&ts).distance_to(&ran) < RUN_SPEED * 1.1,
            "the location follows it, a heartbeat behind at most: {:?}",
            location(&ts)
        );
        assert_eq!(
            ts.world
                .objects
                .get(ObjectGuid::new(RUNNER))
                .and_then(|o| o.current_landblock)
                .map(|b| b.landblock()),
            Some(0xA9B5),
            "and so does its landblock"
        );
        let in_next = |p: &Received| p.cell >> 16 == 0xA9B5;
        assert!(
            positions_of(&ts, north, marks[0], RUNNER)
                .iter()
                .any(in_next),
            "the observer beside the new landblock gets its positions"
        );
        // the first position in the new landblock may go out before it moves there
        let late: Vec<Received> = positions_of(&ts, south, marks[1], RUNNER)
            .into_iter()
            .filter(|p| in_next(p) && p.y > 9.0)
            .collect();
        assert_eq!(late, Vec::new(), "nothing reaches two landblocks back");
    }

    #[test]
    fn a_teleport_taken_while_running_leaves_the_body_standing_at_the_destination() {
        let dest = facing(outdoor(FAR_BLOCK, 50.0, 60.0, 20.0), true);
        let (mut ts, clients) = world(&[
            ("runner", RUNNER, start()),
            // 20 m east of the destination
            ("watcher", NEAR, outdoor(FAR_BLOCK, 70.0, 60.0, 20.0)),
        ]);
        let [runner, watcher] = clients[..] else {
            unreachable!()
        };
        ts.send_game_action(runner, &move_to_state(&start(), true, false));
        ts.advance(0.5);

        empyrean_world::world_objects::player_location::teleport(
            &mut ts.world,
            ObjectGuid::new(RUNNER),
            &dest,
            false,
        );
        ts.advance(3.0);
        ts.send_game_action(runner, &CharacterLoginCompleteNotification);
        let own = ts.received_raw(runner).len();
        let mark = ts.received_raw(watcher).len();
        // the client stands at the destination and says nothing for 3 s
        ts.advance(3.0);
        assert!(
            body(&ts).distance_to(&dest) < 0.1,
            "the body waits: {:?}",
            body(&ts)
        );
        let seen = positions_of(&ts, watcher, mark, RUNNER);
        assert!(
            seen.iter().all(|p| (p.y - dest.position_y).abs() < 0.1),
            "nobody sees it run off: {seen:?}"
        );

        // then it takes a step
        let mut step = dest;
        step.position_y += 0.5;
        ts.send_game_action(runner, &report(&step, 0));
        ts.advance(0.2);
        assert!(!sent_back(&ts, runner, own), "not sent back");
    }

    #[test]
    fn a_report_while_walking_a_move_to_is_taken_wherever_it_is() {
        // a target 60 m north of the runner
        let (mut ts, clients) = world(&[
            ("runner", RUNNER, start()),
            ("target", NEAR, outdoor(HOME, 50.0, 90.5, 20.0)),
        ]);
        let runner = clients[0];
        empyrean_world::world_objects::player_move::create_move_to_chain(
            &mut ts.world,
            ObjectGuid::new(RUNNER),
            ObjectGuid::new(NEAR),
            Box::new(|_, _| {}),
            None,
            false,
        );
        ts.advance(0.5);
        assert!(
            empyrean_world::world_objects::player_move::is_player_moving_to(
                &ts.world,
                ObjectGuid::new(RUNNER)
            )
        );

        // the client, walking it, reports 20 m on from where the server has it
        let own = ts.received_raw(runner).len();
        let mut at = body(&ts);
        at.position_y += 20.0;
        ts.send_game_action(runner, &report(&facing(at, true), 0));
        ts.advance(0.1);
        assert!(!sent_back(&ts, runner, own), "not sent back");
        assert!((location(&ts).position_y - at.position_y).abs() < 0.5);
    }
}
