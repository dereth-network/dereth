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
