//! ACE: Source/ACE.Server/Entity/Chess/ChessMatch.cs::ChessMatch
//! Fool's mate ends in checkmate; illegal move refused with client reason; AI reply equals ACE's
//! ChessLogic; stalemate unrated; walking away forfeits; legacy game when disabled; en passant
//! reported; captures take the piece.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

// V311, V326.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::geometry::AnimFrame;
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::tables::CombatManeuver;
use dereth_assets::{Animation, CombatManeuverTable, MotionTable};
use dereth_physics::geom::Sphere;
use dereth_physics::SetupGeometry;
use dereth_primitives::{DataId, Vec3};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::trade::{
    GameGameOver, GameJoin, GameJoinGameResponse, GameMove, GameMoveData, GameMoveResponse,
    GameOpponentTurn, GameStartGame,
};
use dereth_protocol::Message;
use empyrean_content::models::world::weenie_properties_attribute::WeeniePropertiesAttribute;
use empyrean_content::models::world::weenie_properties_attribute_2nd::WeeniePropertiesAttribute2nd;
use empyrean_content::models::world::weenie_properties_skill::WeeniePropertiesSkill;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_entity::enums::{
    AttackType, MotionCommand as Mc, MotionStance, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString, Skill, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::entity::chess::chess_match;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::player_manager::OnlinePlayer;
use empyrean_world::managers::{landblock_manager, property_manager};
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{game, player_chess};
use empyrean_world::World;

use crate::monster_ai::{EmptyShard, LB};

const PLAYER_WCID: u32 = 1;
const BOARD: u32 = 2;
/// The pieces' weenies: 100 + 10 * colour + type.
const PIECE_BASE: u32 = 100;

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;

/// A small body for the pieces (0.2 m) so neighbours on 1 m squares do not touch.
const PIECE_SETUP: u32 = 0x0200_1001;
/// The board: no collision spheres, so pieces walk across it.
const BOARD_SETUP: u32 = 0x0200_1002;

/// The board's centre.
const CX: f32 = 100.0;
const CY: f32 = 100.0;

// game-event types inside a 0xF7B0
const JOIN_RESPONSE: u32 = 0x0281;
const START_GAME: u32 = 0x0282;
const MOVE_RESPONSE: u32 = 0x0283;
const OPPONENT_TURN: u32 = 0x0284;
const GAME_OVER: u32 = 0x028C;
const CREATE_OBJECT: u32 = 0xF745;

const TYPES: [&str; 6] = ["pawn", "rook", "knight", "bishop", "queen", "king"];

const MT: u32 = 0x0900_0318;
const CMT: u32 = 0x3000_0318;
const CYCLE: u32 = 0x0300_0218;
const ATTACK: u32 = 0x0300_0219;
const NC: u32 = MotionStance::NonCombat.0;
const HC: u32 = MotionStance::HandCombat.0;

fn anim(id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(id),
        low_frame: 0,
        high_frame: -1,
        framerate: 30.0,
    }
}

fn data(key: u32, anims: Vec<AnimData>, velocity: Option<Vec3>, omega: Option<Vec3>) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: 0,
        anims,
        velocity,
        omega,
    }
}

fn key(style: u32, motion: u32) -> u32 {
    style.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

/// `monster_ai.rs`'s synthetic motion table with a slow run (1 m/s) and a quick turn (a half turn a
/// second): the pieces are ticked every 0.2 s, and a MoveToPosition only notices arrival between
/// steps, so a step must be shorter than its 0.3 m arrival distance.
fn chess_dats() -> Arc<empyrean_dat::DatManager> {
    let mut cycles = Vec::new();
    let mut modifiers = Vec::new();
    let mut style_defaults = BTreeMap::new();
    for style in [NC, HC] {
        style_defaults.insert(style, Mc::Ready.0);
        cycles.push(data(key(style, Mc::Ready.0), vec![anim(CYCLE)], None, None));
        cycles.push(data(
            key(style, Mc::RunForward.0),
            vec![anim(CYCLE)],
            Some(Vec3::new(0.0, 1.0, 0.0)),
            None,
        ));
        let omega = Some(Vec3::new(0.0, 0.0, -std::f32::consts::PI));
        cycles.push(MotionData {
            bitfield: 2,
            ..data(key(style, Mc::TurnRight.0), vec![anim(CYCLE)], None, omega)
        });
        modifiers.push(data(key(style, Mc::TurnRight.0), Vec::new(), None, omega));
    }
    let mut links = BTreeMap::new();
    links.insert(
        key(NC, Mc::Ready.0),
        vec![data(HC, vec![anim(CYCLE)], None, None)],
    );
    // a capturing piece fights its victim: the hand-combat swing its maneuver table offers
    links.insert(
        key(HC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(CYCLE)], None, None),
            data(Mc::AttackHigh1.0, vec![anim(ATTACK)], None, None),
        ],
    );
    let table = MotionTable {
        id: DataId(MT),
        default_style: NC,
        style_defaults,
        cycles,
        modifiers,
        links,
    };
    let maneuver = |height: u32, t: AttackType| CombatManeuver {
        style: HC,
        attack_height: height,
        attack_type: t.0.cast_unsigned(),
        min_skill_level: 0,
        motion: Mc::AttackHigh1.0,
    };
    let cmt = CombatManeuverTable {
        id: DataId(CMT),
        maneuvers: vec![
            maneuver(1, AttackType::Punch),
            maneuver(2, AttackType::Punch),
            maneuver(3, AttackType::Kick),
        ],
    };
    let animation = |id: u32, n: u32| Animation {
        id: DataId(id),
        flags: 0,
        num_parts: 0,
        num_frames: n,
        has_hooks: false,
        pos_frames: None,
        part_frames: (0..n)
            .map(|_| AnimFrame {
                frames: Vec::new(),
                hooks: Vec::new(),
            })
            .collect(),
    };
    empyrean_testkit::dats::with_stat_tables(empyrean_dat::FakeDats::new())
        .with_portal(MT, table)
        .with_portal(CMT, cmt)
        .with_portal(CYCLE, animation(CYCLE, 10))
        .with_portal(ATTACK, animation(ATTACK, 30))
        .build()
        .expect("fake dats")
}

fn piece_weenie(wcid: u32, class_name: &str) -> Weenie {
    let mut d = Weenie::new(wcid, class_name, WeenieType::GamePiece)
        .with_string(PropertyString::Name, class_name)
        .with_did(PropertyDataId::Setup, PIECE_SETUP)
        .with_did(PropertyDataId::MotionTable, MT)
        .with_did(PropertyDataId::CombatTable, CMT)
        .with_bool(PropertyBool::Attackable, false)
        // as the retail pieces are: a captured piece leaves no corpse
        .with_bool(PropertyBool::NoCorpse, true);
    d.weenie_properties_attribute = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ]
    .into_iter()
    .map(|a| WeeniePropertiesAttribute {
        object_id: wcid,
        r#type: a.0,
        init_level: 60,
        ..Default::default()
    })
    .collect();
    d.weenie_properties_attribute_2nd = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ]
    .into_iter()
    .map(|v| WeeniePropertiesAttribute2nd {
        object_id: wcid,
        r#type: v.0,
        init_level: 50,
        current_level: 50,
        ..Default::default()
    })
    .collect();
    d.weenie_properties_skill = vec![WeeniePropertiesSkill {
        object_id: wcid,
        r#type: u16::try_from(Skill::Run.0).unwrap(),
        sac: 2,
        init_level: 10,
        ..Default::default()
    }];
    d
}

fn content() -> MemContent {
    let mut c = MemContent::new()
        .weenie(
            Weenie::new(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_string(PropertyString::Name, "human")
                .with_did(PropertyDataId::Setup, TEST_SETUP),
        )
        .weenie(
            Weenie::new(BOARD, "chessboard", WeenieType::Game)
                .with_string(PropertyString::Name, "Chess Board")
                .with_did(PropertyDataId::Setup, BOARD_SETUP)
                .with_float(PropertyFloat::HeartbeatInterval, 1.0),
        );
    for (ci, monster) in ["drudge", "mosswart"].into_iter().enumerate() {
        for (ti, t) in TYPES.into_iter().enumerate() {
            let wcid = PIECE_BASE + 10 * u32::try_from(ci).unwrap() + u32::try_from(ti).unwrap();
            c = c.weenie(piece_weenie(wcid, &format!("{monster}{t}")));
        }
    }
    c
}

fn server() -> TestServer {
    let mut ts = TestServer::with_setup(chess_dats(), |w| {
        w.content = Arc::new(content());
        guid_manager::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 0);
        let small = SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.2), 0.2)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.2), 0.4),
            radius: 0.2,
            height: 1.0,
            ..SetupGeometry::default()
        };
        phys_ext::register_setup(w, PIECE_SETUP, small);
        let flat = SetupGeometry {
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.1),
            radius: 0.0,
            height: 0.1,
            ..SetupGeometry::default()
        };
        phys_ext::register_setup(w, BOARD_SETUP, flat);
    });
    ts.advance(0.1);
    ts
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// A client logged in as `account` whose session plays `guid`, in the world and online in
/// `PlayerManager` (as `DoPlayerEnterWorld` leaves it).
fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
) -> (ClientId, SessionId) {
    let before: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    let id = ts.connect(account, "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .find(|s| !before.contains(s))
        .expect("the new session");

    let w = &mut ts.world;
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            empyrean_world::dispatch::Class::Player,
            weenie,
            ObjectGuid::new(guid),
            1,
        )
    });
    o.set_property(PropertyString::Name, name.to_owned());
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");
    assert!(w.player_manager.online_players.try_add(
        guid,
        OnlinePlayer {
            guid: ObjectGuid::new(guid),
            account: None
        }
    ));

    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(guid)));
    assert!(
        landblock_manager::add_object(w, ObjectGuid::new(guid), false),
        "the player joins its landblock"
    );
    ts.advance(0.1);
    (id, session)
}

/// The chessboard, entered at the board's centre.
fn board(ts: &mut TestServer) -> ObjectGuid {
    let w = &mut ts.world;
    let weenie = w.content.get_cached_weenie(BOARD).expect("board weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    w.objects
        .get_mut(guid)
        .unwrap()
        .set_location(Some(at(CX, CY)));
    assert!(
        empyrean_world::dispatch::enter_world::enter_world(w, guid),
        "the board enters the world"
    );
    guid
}

/// A received message: its kind (the game-event type inside a 0xF7B0, else the opcode) and blob.
#[derive(Debug, Clone)]
struct Got {
    kind: u32,
    blob: Vec<u8>,
}

impl Got {
    fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        let m = M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind));
        if split.order.is_none() {
            assert_eq!(split.sub_type.0, M::OPCODE.0);
        }
        m
    }
}

fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == 0xF7B0 {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .collect()
}

fn all(g: &[Got], kind: u32) -> Vec<&Got> {
    g.iter().filter(|m| m.kind == kind).collect()
}

fn one<M: Message>(g: &[Got], kind: u32) -> M {
    let found = all(g, kind);
    assert_eq!(
        found.len(),
        1,
        "one 0x{kind:04X}: {:04X?}",
        g.iter().map(|m| m.kind).collect::<Vec<_>>()
    );
    found[0].decode::<M>()
}

/// Sends `act` from `id`, then runs until `until` holds for what each listed client has received
/// since (or `max` virtual seconds), and returns those messages.
fn exchange(
    ts: &mut TestServer,
    ids: &[ClientId],
    max: f64,
    act: impl FnOnce(&mut TestServer),
    until: impl Fn(&[Vec<Got>]) -> bool,
) -> Vec<Vec<Got>> {
    let from: Vec<usize> = ids.iter().map(|&id| ts.received_raw(id).len()).collect();
    act(ts);
    let collect = |ts: &TestServer| {
        ids.iter()
            .zip(&from)
            .map(|(&id, &n)| got(ts, id, n))
            .collect::<Vec<_>>()
    };
    let done = ts.run_until(max, |ts| until(&collect(ts)));
    let out = collect(ts);
    assert!(
        done,
        "timed out: {:04X?}",
        out.iter()
            .map(|g| g.iter().map(|m| m.kind).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    out
}

fn sq(s: &str) -> (i32, i32) {
    let b = s.as_bytes();
    (i32::from(b[0] - b'a'), i32::from(b[1] - b'1'))
}

fn game_move(mv: &str) -> GameMove {
    let (from, to) = (sq(&mv[0..2]), sq(&mv[2..4]));
    GameMove {
        x_from: from.0,
        y_from: from.1,
        x_to: to.0,
        y_to: to.1,
    }
}

fn setup_two_players() -> (TestServer, ClientId, ClientId, ObjectGuid) {
    let mut ts = server();
    let (a, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(CX, CY - 5.5));
    let (b, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(CX, CY + 5.5));
    let board = board(&mut ts);
    ts.advance(0.5);
    (ts, a, b, board)
}

/// The weenie pieces of the board's match (the guids its logic records) that are live GamePieces
/// named `prefix`*.
fn pieces_named(w: &World, board: ObjectGuid, prefix: &str) -> usize {
    let Some(m) = game::chess_match(w, board) else {
        return 0;
    };
    let guids: Vec<ObjectGuid> = m
        .get(w)
        .logic
        .board
        .iter()
        .flatten()
        .map(|p| p.guid)
        .collect();
    guids
        .into_iter()
        .filter_map(|g| w.objects.get(g))
        .filter(|o| {
            o.is_game_piece()
                && o.get_property(PropertyString::Name)
                    .is_some_and(|n| n.starts_with(prefix))
        })
        .count()
}

/// Joins `id` to the board and returns what it received: the JoinGameResponse (with its team) and
/// the 16 pieces of its colour appearing.
fn join_board(ts: &mut TestServer, id: ClientId, board: ObjectGuid) -> Vec<Got> {
    let join = GameJoin {
        game_id: board.full(),
        which_team: u32::MAX,
    };
    exchange(
        ts,
        &[id],
        5.0,
        |ts| ts.send_game_action(id, &join),
        |g| !all(&g[0], JOIN_RESPONSE).is_empty() && all(&g[0], CREATE_OBJECT).len() >= 16,
    )
    .remove(0)
}

/// One move: `mover` plays `mv`; runs until the mover has its MoveResponse (the moved piece has
/// walked to its square and reported ready) and, when `other` is given, that player has the
/// OpponentTurn.
fn play(
    ts: &mut TestServer,
    mover: ClientId,
    other: Option<ClientId>,
    mv: &str,
) -> (Vec<Got>, Vec<Got>) {
    let m = game_move(mv);
    let ids: Vec<ClientId> = std::iter::once(mover).chain(other).collect();
    let mut got = exchange(
        ts,
        &ids,
        30.0,
        |ts| ts.send_game_action(mover, &m),
        |g| {
            !all(&g[0], MOVE_RESPONSE).is_empty()
                && g.get(1).is_none_or(|o| !all(o, OPPONENT_TURN).is_empty())
        },
    );
    let o = if got.len() > 1 {
        got.remove(1)
    } else {
        Vec::new()
    };
    (got.remove(0), o)
}

/// Runs until both clients have had a StartGame since the given message counts (the Start action
/// may run while the second player's pieces are still arriving) and returns those StartGames.
fn wait_start(
    ts: &mut TestServer,
    a: ClientId,
    b: ClientId,
    from: (usize, usize),
) -> [GameStartGame; 2] {
    let started = |ts: &TestServer| {
        [
            all(&got(ts, a, from.0), START_GAME).len(),
            all(&got(ts, b, from.1), START_GAME).len(),
        ]
    };
    assert!(
        ts.run_until(5.0, |ts| started(ts) == [1, 1]),
        "both players get one StartGame: {:?}",
        started(ts)
    );
    [
        one(&got(ts, a, from.0), START_GAME),
        one(&got(ts, b, from.1), START_GAME),
    ]
}

fn from_to(d: &GameMoveData) -> ((u32, u32), (u32, u32)) {
    (d.from.expect("from"), d.to.expect("to"))
}

fn xy(s: &str) -> (u32, u32) {
    let (x, y) = sq(s);
    (u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
}

fn prop(ts: &TestServer, player: u32, p: PropertyInt) -> Option<i32> {
    ts.world
        .objects
        .get(ObjectGuid::new(player))
        .and_then(|o: &WorldObject| o.get_property(p))
}

// ------------------------------------------------------------------ scenarios

/// Two players join a board (White then Black, each seeing its 16 pieces appear), the game starts
/// (`StartGame`, White to move), and they play the fool's mate: `Qh4` is checkmate (V311/V326, V311:
/// the client's rules; `OKMoveToEmptySquare | OKMoveCheckmate`, 0x801, where ACE reported check
/// only, 0x401, and the mated side had to resign). White is told of the move (its client shows "You
/// have been checkmated!"), then both get `GameOver` with Black the winner; the chess counters and
/// Elo ranks are updated and every piece removed.
#[test]
fn two_players_play_the_fools_mate_and_the_game_ends_in_checkmate() {
    let (mut ts, a, b, board) = setup_two_players();

    let g = join_board(&mut ts, a, board);
    let r: GameJoinGameResponse = one(&g, JOIN_RESPONSE);
    assert_eq!(
        (r.game_id, r.team),
        (board.full(), 0),
        "Alpha sits as White"
    );
    assert_eq!(
        pieces_named(&ts.world, board, "drudge"),
        16,
        "White's drudges on the board"
    );
    assert_eq!(pieces_named(&ts.world, board, "mosswart"), 0);
    assert!(all(&g, CREATE_OBJECT).len() >= 16, "Alpha sees them appear");
    let m = player_chess::chess_match(&ts.world, ObjectGuid::new(ALPHA))
        .expect("Alpha is in the match");
    assert!(
        game::chess_match(&ts.world, board).is_some_and(|b| b == m),
        "the board holds the same match"
    );

    let from = (ts.received_raw(a).len(), ts.received_raw(b).len());
    let g = join_board(&mut ts, b, board);
    let r: GameJoinGameResponse = one(&g, JOIN_RESPONSE);
    assert_eq!(
        (r.game_id, r.team),
        (board.full(), 1),
        "Bravo sits as Black"
    );
    assert_eq!(pieces_named(&ts.world, board, "mosswart"), 16);

    // the Start action runs on the board's next heartbeat
    for s in wait_start(&mut ts, a, b, from) {
        assert_eq!((s.game_id, s.team), (board.full(), 0), "White moves first");
    }

    // 1. f3 e5 2. g4
    for (mv, mover, other, team) in [("f2f3", a, b, 0), ("e7e5", b, a, 1), ("g2g4", a, b, 0)] {
        let (mine, theirs) = play(&mut ts, mover, Some(other), mv);
        let resp: GameMoveResponse = one(&mine, MOVE_RESPONSE);
        assert_eq!(
            (resp.game_id, resp.result),
            (board.full(), 1),
            "{mv}: the mover's result"
        );
        let turn: GameOpponentTurn = one(&theirs, OPPONENT_TURN);
        assert_eq!(turn.game_id, board.full());
        assert_eq!(turn.team, team, "{mv}: the moving side");
        assert_eq!(turn.move_data.move_type, GameMoveData::FROM_TO);
        assert_eq!(
            turn.move_data.player.0,
            if team == 0 { ALPHA } else { BRAVO },
            "{mv}: the mover"
        );
        assert_eq!(
            from_to(&turn.move_data),
            (xy(&mv[0..2]), xy(&mv[2..4])),
            "{mv}"
        );
    }

    // 2... Qh4#: the move is reported, then the game is over
    let piece_guids: Vec<ObjectGuid> = m
        .get(&ts.world)
        .logic
        .board
        .iter()
        .flatten()
        .map(|p| p.guid)
        .collect();
    assert_eq!(piece_guids.len(), 32);
    let from = [ts.received_raw(a).len(), ts.received_raw(b).len()];
    let over = exchange(
        &mut ts,
        &[b, a],
        30.0,
        |ts| ts.send_game_action(b, &game_move("d8h4")),
        |g| g.iter().all(|c| !all(c, GAME_OVER).is_empty()),
    );
    let resp: GameMoveResponse = one(&over[0], MOVE_RESPONSE);
    assert_eq!(
        (resp.game_id, resp.result),
        (board.full(), 0x801),
        "d8h4: OKMoveToEmptySquare | OKMoveCheckmate"
    );
    let turn: GameOpponentTurn = one(&over[1], OPPONENT_TURN);
    assert_eq!(
        (turn.team, from_to(&turn.move_data)),
        (1, (xy("d8"), xy("h4"))),
        "White sees the mating move"
    );
    let kinds: Vec<u32> = over[1]
        .iter()
        .map(|m| m.kind)
        .filter(|&k| k == OPPONENT_TURN || k == GAME_OVER)
        .collect();
    assert_eq!(
        kinds,
        [OPPONENT_TURN, GAME_OVER],
        "the move first, then the game over"
    );
    for g in &over {
        let o: GameGameOver = one(g, GAME_OVER);
        assert_eq!((o.game_id, o.team_winner), (board.full(), 1), "Black wins");
    }
    // the logic ends with the queen on h4
    let queen = m
        .get(&ts.world)
        .logic
        .get_piece(&empyrean_world::entity::chess::chess_piece_coord::ChessPieceCoord::new_xy(7, 3))
        .map(|p| p.r#type);
    assert_eq!(queen, Some(empyrean_entity::enums::ChessPieceType::Queen));
    // every piece is destroyed and leaves the world
    assert!(
        piece_guids.iter().all(|&g| ts
            .world
            .objects
            .get(g)
            .is_none_or(|o| o.wo.world_object.is_destroyed)),
        "every piece destroyed"
    );
    ts.advance(0.5);
    for (id, n) in [(a, from[0]), (b, from[1])] {
        let mut deleted: Vec<u32> = all(&got(&ts, id, n), 0xF747)
            .iter()
            .map(|m| {
                m.decode::<dereth_protocol::objects::ItemDeleteObject>()
                    .id
                    .0
            })
            .collect();
        deleted.sort_unstable();
        let mut pieces: Vec<u32> = piece_guids.iter().map(|g| g.full()).collect();
        pieces.sort_unstable();
        assert_eq!(deleted, pieces, "a DeleteObject for each piece");
    }

    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessTotalGames), Some(1));
    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessGamesLost), Some(1));
    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessGamesWon), None);
    assert_eq!(prop(&ts, BRAVO, PropertyInt::ChessTotalGames), Some(1));
    assert_eq!(prop(&ts, BRAVO, PropertyInt::ChessGamesWon), Some(1));
    // equal ranks: the expectation is 0.5, so 50 * (0 - 0.5) = -25 for the loser
    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessRank), Some(1375));
    assert_eq!(prop(&ts, BRAVO, PropertyInt::ChessRank), Some(1425));
    assert!(player_chess::chess_match(&ts.world, ObjectGuid::new(ALPHA)).is_none());
    assert!(player_chess::chess_match(&ts.world, ObjectGuid::new(BRAVO)).is_none());
    assert!(
        game::chess_match(&ts.world, board).is_none(),
        "the board is free again"
    );
    assert_eq!(
        m.get(&ts.world).state,
        empyrean_entity::enums::ChessState::Finished
    );
    assert_eq!(ts.world.chess_matches.len(), 1);
}

/// An illegal move (a pawn three squares from the start rank to the fifth) is refused with the
/// client's reason, `BadMoveDirection` (-100: the client answers any move a piece cannot make so,
/// never `BadMoveDistance`, V326; V311/V326, V311: ACE answered every illegal move with
/// `BadMoveInvalidCommand`, -1); nothing moves and the opponent hears nothing. A move out of turn is
/// dropped silently (`MoveDelayed` returns when it is not that colour's turn).
#[test]
fn an_illegal_move_is_refused_with_the_clients_reason() {
    let (mut ts, a, b, board) = setup_two_players();
    join_board(&mut ts, a, board);
    let from = (ts.received_raw(a).len(), ts.received_raw(b).len());
    join_board(&mut ts, b, board);
    wait_start(&mut ts, a, b, from);

    let got = exchange(
        &mut ts,
        &[a, b],
        5.0,
        |ts| ts.send_game_action(a, &game_move("e2e5")),
        |g| !all(&g[0], MOVE_RESPONSE).is_empty(),
    );
    let resp: GameMoveResponse = one(&got[0], MOVE_RESPONSE);
    assert_eq!(
        (resp.game_id, resp.result),
        (board.full(), -100),
        "BadMoveDirection"
    );
    assert!(
        all(&got[1], OPPONENT_TURN).is_empty() && all(&got[1], MOVE_RESPONSE).is_empty(),
        "Black hears nothing"
    );

    // Black moving while White is to move: dropped
    let before = ts.received_raw(b).len();
    ts.send_game_action(b, &game_move("e7e5"));
    ts.advance(3.0);
    let later = got_since(&ts, b, before);
    assert!(
        all(&later, MOVE_RESPONSE).is_empty(),
        "no answer to a move out of turn"
    );

    // the legal move still works afterwards
    let (mine, theirs) = play(&mut ts, a, Some(b), "e2e4");
    assert_eq!(one::<GameMoveResponse>(&mine, MOVE_RESPONSE).result, 1);
    assert_eq!(
        from_to(&one::<GameOpponentTurn>(&theirs, OPPONENT_TURN).move_data),
        (xy("e2"), xy("e4"))
    );
}

fn got_since(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    got(ts, id, from)
}

/// A lone player with `chess_ai_start_time` set: the server says it will start with the AI, the AI
/// takes Black after that many seconds (its mosswarts appear), the game starts, and after White's
/// `e2e4` the AI answers with the move ACE's own `ChessLogic` chose for that position (`b8c6`,
/// `chess/scripted.json`; the AI V311/V326 keeps, scoring for Black over the client's legal moves,
/// chooses it too), sent as an OpponentTurn from guid 0.
#[test]
fn a_player_against_the_ai_gets_the_ais_reply() {
    let (mut ts, a, _b, board) = setup_two_players();
    assert!(property_manager::modify_double(
        &ts.world,
        "chess_ai_start_time",
        3.0,
        false
    ));

    let g = join_board(&mut ts, a, board);
    assert_eq!(one::<GameJoinGameResponse>(&g, JOIN_RESPONSE).team, 0);
    let chats: Vec<String> = ts
        .received::<dereth_protocol::comms::CommunicationTextboxString>(a)
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert!(
        chats.iter().any(|t| t == "If another player doesn't join within 3 seconds, the game will automatically start with AI"),
        "{chats:?}"
    );

    let started = exchange(
        &mut ts,
        &[a],
        10.0,
        |_| {},
        |g| !all(&g[0], START_GAME).is_empty(),
    );
    assert_eq!(one::<GameStartGame>(&started[0], START_GAME).team, 0);
    assert_eq!(
        pieces_named(&ts.world, board, "mosswart"),
        16,
        "the AI's army"
    );

    // the AI's reply as ACE's ChessLogic recorded it
    let scripted = empyrean_common::vectors::load_named("chess", "scripted");
    let case = scripted
        .cases
        .iter()
        .find(|c| c.input["steps"] == serde_json::json!(["e2e4", "ai"]))
        .expect("the e2e4 / ai case");
    let ai = &case.output["steps"][1][0];
    let (from, to) = (ai[1].as_str().unwrap(), ai[2].as_str().unwrap());
    let parse = |s: &str| {
        let v: Vec<u32> = s.split(',').map(|n| n.parse().unwrap()).collect();
        (v[0], v[1])
    };
    assert_eq!((parse(from), parse(to)), (xy("b8"), xy("c6")));

    let m = game_move("e2e4");
    let got = exchange(
        &mut ts,
        &[a],
        40.0,
        |ts| ts.send_game_action(a, &m),
        |g| !all(&g[0], OPPONENT_TURN).is_empty(),
    );
    assert_eq!(
        one::<GameMoveResponse>(&got[0], MOVE_RESPONSE).result,
        1,
        "e2e4"
    );
    let turn: GameOpponentTurn = one(&got[0], OPPONENT_TURN);
    assert_eq!(turn.team, 1, "the AI plays Black");
    assert_eq!(turn.move_data.player.0, 0, "the AI's guid is 0");
    assert_eq!(
        from_to(&turn.move_data),
        (parse(from), parse(to)),
        "ACE's AI reply"
    );

    let mm = chess_match::ChessMatch::get_color(
        player_chess::chess_match(&ts.world, ObjectGuid::new(ALPHA))
            .unwrap()
            .get(&ts.world),
        ObjectGuid::new(0),
    );
    assert_eq!(mm, empyrean_entity::enums::ChessColor::Black);
}

/// Both sides seated and the game started (White to move).
fn started_game() -> (TestServer, ClientId, ClientId, ObjectGuid) {
    let (mut ts, a, b, board) = setup_two_players();
    join_board(&mut ts, a, board);
    let from = (ts.received_raw(a).len(), ts.received_raw(b).len());
    join_board(&mut ts, b, board);
    wait_start(&mut ts, a, b, from);
    (ts, a, b, board)
}

/// A stalemate offer reaches the opponent (`OpponentStalemate`, the offering colour, on); when the
/// opponent offers too, `Finish(ChessWinnerStalemate)`: `GameOver` -1 to both, a game each on the
/// total, no win, no loss and no rank change (only a winner >= 0 is rated).
#[test]
fn a_stalemate_offered_and_accepted_ends_the_game_unrated() {
    use dereth_protocol::trade::{GameOpponentStalemateState, GameStalemate};
    const OPPONENT_STALEMATE: u32 = 0x0285;

    let (mut ts, a, b, board) = started_game();
    let got = exchange(
        &mut ts,
        &[b],
        5.0,
        |ts| ts.send_game_action(a, &GameStalemate { on: 1 }),
        |g| !all(&g[0], OPPONENT_STALEMATE).is_empty(),
    );
    let s: GameOpponentStalemateState = one(&got[0], OPPONENT_STALEMATE);
    assert_eq!(
        (s.game_id, s.team, s.on),
        (board.full(), 0, 1),
        "White offers"
    );

    let over = exchange(
        &mut ts,
        &[a, b],
        5.0,
        |ts| ts.send_game_action(b, &GameStalemate { on: 1 }),
        |g| g.iter().all(|c| !all(c, GAME_OVER).is_empty()),
    );
    for g in &over {
        assert_eq!(
            one::<GameGameOver>(g, GAME_OVER).team_winner,
            -1,
            "stalemate"
        );
    }
    for p in [ALPHA, BRAVO] {
        assert_eq!(prop(&ts, p, PropertyInt::ChessTotalGames), Some(1));
        assert_eq!(prop(&ts, p, PropertyInt::ChessGamesWon), None);
        assert_eq!(prop(&ts, p, PropertyInt::ChessGamesLost), None);
        assert_eq!(prop(&ts, p, PropertyInt::ChessRank), None, "unrated");
    }
}

/// The leash: every 5 s the board checks both players; one more than 40 m away quits
/// (`QuitDelayed`), so the other colour wins.
#[test]
fn walking_away_from_the_board_forfeits() {
    let (mut ts, a, b, _board) = started_game();
    {
        let w = &mut ts.world;
        let g = ObjectGuid::new(BRAVO);
        let h = phys_ext::physics_obj(w, g).expect("a body");
        assert!(phys_ext::set_position(
            w,
            h,
            &phys_ext::to_physics_position(&at(CX, CY + 45.0))
        ));
        empyrean_world::world_objects::world_object::sync_location(w, g);
    }
    let over = exchange(
        &mut ts,
        &[a, b],
        7.0,
        |_| {},
        |g| g.iter().all(|c| !all(c, GAME_OVER).is_empty()),
    );
    for g in &over {
        assert_eq!(
            one::<GameGameOver>(g, GAME_OVER).team_winner,
            0,
            "Black walked away, White wins"
        );
    }
    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessGamesWon), Some(1));
    assert_eq!(prop(&ts, BRAVO, PropertyInt::ChessGamesLost), Some(1));
}

/// With `chess_enabled` off, `Game.ActOnJoin_Legacy`: the player is told it plays Black, both
/// armies appear, 5 s later they die and the game is over (White wins), 2 s after that the popup
/// and the game-mode exit (`GameOver` board 0, -2); the player's games-lost counter goes up, but a
/// null counter stays null (ACE's `int?` `++`).
#[test]
fn with_chess_disabled_the_legacy_game_plays_out() {
    use dereth_protocol::comms::CommunicationPopUpString;
    const POPUP: u32 = 0x0004;

    let (mut ts, a, _b, board) = setup_two_players();
    assert!(property_manager::modify_bool(
        &ts.world,
        "chess_enabled",
        false
    ));
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_property(PropertyInt::ChessGamesLost, 2);

    let join = GameJoin {
        game_id: board.full(),
        which_team: u32::MAX,
    };
    let g = exchange(
        &mut ts,
        &[a],
        5.0,
        |ts| ts.send_game_action(a, &join),
        |g| !all(&g[0], JOIN_RESPONSE).is_empty() && all(&g[0], CREATE_OBJECT).len() >= 32,
    )
    .remove(0);
    let r: GameJoinGameResponse = one(&g, JOIN_RESPONSE);
    assert_eq!(
        (r.game_id, r.team),
        (board.full(), 1),
        "the legacy game seats the player as Black"
    );
    assert!(game::chess_match(&ts.world, board).is_none(), "no match");

    let g = exchange(
        &mut ts,
        &[a],
        6.0,
        |_| {},
        |g| !all(&g[0], GAME_OVER).is_empty(),
    )
    .remove(0);
    let o: GameGameOver = one(&g, GAME_OVER);
    assert_eq!((o.game_id, o.team_winner), (board.full(), 0));

    let g = exchange(
        &mut ts,
        &[a],
        3.0,
        |_| {},
        |g| !all(&g[0], POPUP).is_empty(),
    )
    .remove(0);
    assert_eq!(
        one::<CommunicationPopUpString>(&g, POPUP).message,
        "gamesdeadlol"
    );
    let o: GameGameOver = one(&g, GAME_OVER);
    assert_eq!(
        (o.game_id, o.team_winner),
        (0, -2),
        "the client leaves game mode"
    );
    assert_eq!(prop(&ts, ALPHA, PropertyInt::ChessGamesLost), Some(3));
    assert_eq!(
        prop(&ts, ALPHA, PropertyInt::ChessTotalGames),
        Some(1),
        "an unset counter counts from 0 (V309)"
    );
}

/// The guid of the weenie piece the match's logic has on `square`.
fn piece_on(ts: &TestServer, board: ObjectGuid, square: &str) -> ObjectGuid {
    let (x, y) = sq(square);
    let m = game::chess_match(&ts.world, board).expect("a match");
    m.get(&ts.world)
        .logic
        .get_piece(&empyrean_world::entity::chess::chess_piece_coord::ChessPieceCoord::new_xy(x, y))
        .map(|p| p.guid)
        .expect("a piece")
}

/// Plays `moves` (move, mover, other, the mover's expected result); the last one captures
/// `victim_square`'s piece: after it the victim's weenie dies and is removed, the capturing piece
/// stands on its destination, and the side to move has the OpponentTurn.
fn play_capture(
    ts: &mut TestServer,
    board: ObjectGuid,
    moves: &[(&str, ClientId, ClientId, i32)],
    victim_square: &str,
) {
    let (last, rest) = moves.split_last().expect("a move");
    for &(mv, mover, other, expected) in rest {
        let (mine, _) = play(ts, mover, Some(other), mv);
        assert_eq!(
            one::<GameMoveResponse>(&mine, MOVE_RESPONSE).result,
            expected,
            "{mv}: the mover's result"
        );
    }
    let victim = piece_on(ts, board, victim_square);
    let (mv, mover, other, expected) = *last;
    let (mine, theirs) = play(ts, mover, Some(other), mv);
    assert_eq!(
        one::<GameMoveResponse>(&mine, MOVE_RESPONSE).result,
        expected,
        "{mv}: the mover's result"
    );
    assert_eq!(
        from_to(&one::<GameOpponentTurn>(&theirs, OPPONENT_TURN).move_data),
        (xy(&mv[0..2]), xy(&mv[2..4])),
        "{mv}"
    );

    let (x, y) = sq(&mv[2..4]);
    let taker = piece_on(ts, board, &mv[2..4]);
    let at = ts
        .world
        .objects
        .get(taker)
        .and_then(WorldObject::location)
        .expect("the capturing piece");
    #[allow(clippy::cast_precision_loss)]
    let (ex, ey) = (CX + x as f32 - 3.5, CY + y as f32 - 3.5);
    assert!(
        (at.position_x - ex).abs() < 0.5 && (at.position_y - ey).abs() < 0.5,
        "{mv}: the capturing piece walked onto its square: {at:?}"
    );
    ts.advance(10.0);
    assert!(
        ts.world
            .objects
            .get(victim)
            .is_none_or(|o| o.wo.world_object.is_destroyed),
        "{mv}: the captured piece is removed"
    );
}

/// An en passant capture is reported to the mover as OKMoveEnPassant (3), and the captured pawn
/// is taken as in any capture (V306; ACE reported 2). 1. e4 a6 2. e5 d5 3. exd6 e.p.
#[test]
fn an_en_passant_capture_is_reported_as_en_passant() {
    let (mut ts, a, b, board) = started_game();
    play_capture(
        &mut ts,
        board,
        &[
            ("e2e4", a, b, 1),
            ("a7a6", b, a, 1),
            ("e4e5", a, b, 1),
            ("d7d5", b, a, 1),
            ("e5d6", a, b, 3),
        ],
        "d5",
    );
    assert_eq!(
        pieces_named(&ts.world, board, "mosswart"),
        15,
        "Black's d-pawn is taken"
    );
    assert_eq!(pieces_named(&ts.world, board, "drudge"), 16);
}

/// Black's en passant: 1. a3 d5 2. a4 d4 3. e4 dxe3 e.p. (V306).
#[test]
fn blacks_en_passant_capture_is_reported_as_en_passant() {
    let (mut ts, a, b, board) = started_game();
    play_capture(
        &mut ts,
        board,
        &[
            ("a2a3", a, b, 1),
            ("d7d5", b, a, 1),
            ("a3a4", a, b, 1),
            ("d5d4", b, a, 1),
            ("e2e4", a, b, 1),
            ("d4e3", b, a, 3),
        ],
        "e4",
    );
    assert_eq!(
        pieces_named(&ts.world, board, "drudge"),
        15,
        "White's e-pawn is taken"
    );
    assert_eq!(pieces_named(&ts.world, board, "mosswart"), 16);
}

/// A capture: the capturing piece walks up to the victim, fights it, then walks onto the square;
/// the mover gets OKMoveToOccupiedSquare (2). 1. e4 d5 2. exd5
#[test]
fn a_capture_takes_the_piece() {
    let (mut ts, a, b, board) = started_game();
    play_capture(
        &mut ts,
        board,
        &[("e2e4", a, b, 1), ("d7d5", b, a, 1), ("e4d5", a, b, 2)],
        "d5",
    );
    assert_eq!(
        pieces_named(&ts.world, board, "mosswart"),
        15,
        "Black's d-pawn is taken"
    );
}
