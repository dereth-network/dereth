//! A recorded 2017 session's first minute of combat, played through the client (with its scripts)
//! and the scene, a frame every thirtieth of a second: the stories told on the bodies play there.
//! Fixture: the end-of-retail install and the capture index.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::combat::DO_3D_DAMAGE;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::runtime::link_scripts;
use auberean_client_sdk::{Action, Client, ClientData, ClientSettings};
use auberean_scene::story::StoryTally;
use auberean_scene::{movement, Intent, Scene, WorldData};
use auberean_testkit::captures;

/// A frame of the combat replays: a thirtieth of a second.
const FRAME_30: f64 = 1.0 / 30.0;
/// The client's entering the world.
const PLAY_CHARACTER: u32 = 0x1002B;
/// The client's reports of where the player stands.
const CLIENT_POSITION: u32 = 0x1008E;
/// A record the server sends, and its refusal to send one.
const DATA_RECORD: u32 = 0x10035;
const DATA_REFUSED: u32 = 0x10036;
/// The blob ids the replay's own answers to the client's data requests go in: the login
/// connection's, taken as they come.
const ANSWERS: u64 = 0x2300_0000_F000_0000;

/// The records the servers sent in the capture of session `session_key`, by type and id.
pub(crate) fn records_sent(session_key: &str) -> HashMap<(u32, u32), ServerMessage> {
    let index = auberean_testkit::data::require_capture_index();
    let mut sent = HashMap::new();
    captures::capture_messages_in_time_order(&index, &[DATA_RECORD], session_key, |m| {
        if let Ok(ServerMessage::DataRecord(r)) = ServerMessage::decode(&m.raw) {
            sent.insert((r.kind, r.id), ServerMessage::DataRecord(r));
        }
    })
    .expect("the index reads");
    sent
}

/// The records the client asked the server for since `seen` (of its sent messages), answered at
/// once as the servers did: a record they sent in the capture is sent, any other refused.
pub(crate) fn answer_data_requests(
    c: &mut Client,
    seen: &mut usize,
    sent: &HashMap<(u32, u32), ServerMessage>,
    answers: &mut u64,
) {
    let asked: Vec<(u32, u32)> = c.sent()[*seen..]
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) => Some((r.kind, r.id)),
            _ => None,
        })
        .collect();
    *seen = c.sent().len();
    for (kind, id) in asked {
        let m = sent
            .get(&(kind, id))
            .cloned()
            .unwrap_or(ServerMessage::DataRefused(
                auberean_client_sdk::protocol::login::DataRefused { kind, id, error: 1 },
            ));
        *answers += 1;
        c.take_message(ANSWERS | *answers, m);
    }
}
/// A frame's length, seconds.
pub(crate) const FRAME: f64 = 1.0 / 30.0;

/// The world and the client's data with its scripts, opened once.
pub(crate) fn data() -> &'static (WorldData, ClientData) {
    static DATA: OnceLock<(WorldData, ClientData)> = OnceLock::new();
    DATA.get_or_init(open_data)
}

/// A world of its own, for a test whose client's data cache the world reads through alone.
pub(crate) fn own_world() -> &'static WorldData {
    Box::leak(Box::new(open_data().0))
}

/// The world and the client's data with its scripts.
fn open_data() -> (WorldData, ClientData) {
    {
        let dir = auberean_dat::testing::require_target_install();
        let found = install_dats(&dir).expect("list the install");
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
            let (_, p) = found.iter().find(|(n, _)| want(n))?;
            DatFile::open(p).ok()
        };
        let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
        let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
        let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
        let records = auberean_world::records::WorldRecords::new(
            portal,
            cell,
            open(&|n| *n == DatName::HighRes),
            ctx,
        );
        let world = WorldData::new(records).expect("the region");
        let portal = Arc::new(open(&|n| *n == DatName::Portal).expect("a portal file"));
        let english =
            Arc::new(open(&|n| *n == DatName::Local("English".into())).expect("the English file"));
        let program = link_scripts(&portal).expect("the library links");
        let cell = Arc::new(open(&|n| *n == DatName::Cell(1)).expect("a cell file"));
        let data = ClientData {
            files: DataFiles {
                files: vec![Arc::clone(&portal), english, cell],
                language: ENGLISH,
            },
            scripts: Some((portal, Arc::new(program))),
        };
        (world, data)
    }
}

fn client(data: &ClientData) -> Client {
    let settings = ClientSettings {
        login_server: std::net::SocketAddr::from(([127, 0, 0, 1], 9000)),
        account: Vec::new(),
        ticket: Vec::new(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    Client::new(settings, data.clone())
}

/// What a replay saw: the scene's tally, the floating texts the scripts sent the panels, and the
/// most objects the client had made at once.
pub(crate) struct Replay {
    tally: StoryTally,
    floating: u64,
    /// The modes the objects were told their bodies went into, in order.
    pub(crate) modes: Vec<(u64, u32)>,
}

/// The capture of session `session_key` played from its start until `seconds` after the player
/// entered the world.
fn replay(session_key: &str, seconds: f64) -> Replay {
    played(session_key, seconds).2
}

/// [`replay`], with the client and the scene as the replay left them.
pub(crate) fn played(session_key: &str, seconds: f64) -> (Client, Scene<'static>, Replay) {
    played_in_frames(session_key, seconds, FRAME, &[], &mut |_, _| {})
}

/// [`played`] with frames `step` seconds long, the messages of types `left_out` left out (the
/// recorded client's own reports of where the player stood among them: the player then moves only
/// as the server moves it), and `observe` handed the client and the scene after each frame.
pub(crate) fn played_in_frames(
    session_key: &str,
    seconds: f64,
    step: f64,
    left_out: &[u32],
    observe: &mut dyn FnMut(&mut Client, &mut Scene<'_>),
) -> (Client, Scene<'static>, Replay) {
    let graphics = auberean_draw::quality::Graphics::first_run();
    played_with(
        &data().0,
        session_key,
        seconds,
        step,
        left_out,
        &graphics,
        &mut |_| {},
        observe,
    )
}

/// [`played_in_frames`] in `world`, drawn by the preferences `graphics`, with the client handed
/// to `prepare` before the replay starts.
#[allow(clippy::too_many_arguments)] // the replay's settings, each its own
pub(crate) fn played_with(
    world: &'static WorldData,
    session_key: &str,
    seconds: f64,
    step: f64,
    left_out: &[u32],
    graphics: &auberean_draw::quality::Graphics,
    prepare: &mut dyn FnMut(&mut Client),
    observe: &mut dyn FnMut(&mut Client, &mut Scene<'_>),
) -> (Client, Scene<'static>, Replay) {
    let data = &data().1;
    let index = auberean_testkit::data::require_capture_index();
    // The client's data requests are answered as they are made; the recorded answers to the
    // recorded client's are left out.
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| {
            ServerMessage::is_known_type(*t)
                && !left_out.contains(t)
                && *t != DATA_RECORD
                && *t != DATA_REFUSED
        })
        .collect();
    let sent = records_sent(session_key);
    let (mut seen, mut answers) = (0usize, 0u64);
    types.push(PLAY_CHARACTER);
    if !left_out.contains(&CLIENT_POSITION) {
        types.push(CLIENT_POSITION);
    }
    let mut c = client(data);
    prepare(&mut c);
    let mut scene = Scene::new(world, graphics);
    let intent = Intent::default();
    let mut next = f64::NEG_INFINITY;
    let mut entered: Option<f64> = None;
    let mut floating = 0u64;
    let mut modes: Vec<(u64, u32)> = Vec::new();
    let mut frame = |c: &mut Client,
                     scene: &mut Scene<'_>,
                     t: f64,
                     floating: &mut u64,
                     modes: &mut Vec<(u64, u32)>| {
        c.set_time(LocalTime(t));
        c.upkeep();
        answer_data_requests(c, &mut seen, &sent, &mut answers);
        #[allow(clippy::cast_possible_truncation)]
        scene.update(c, &intent, t, step as f32);
        *floating += c
            .notices
            .iter()
            .filter(|n| n.native == DO_3D_DAMAGE)
            .count() as u64;
        modes.extend(
            c.notices
                .iter()
                .filter(|n| n.native == auberean_scene::objects::MODE_NOTICE)
                .map(|n| (n.about.unwrap_or(0), n.words.first().copied().unwrap_or(0))),
        );
        c.notices.clear();
        c.shown.clear();
    };
    captures::capture_messages_in_time_order(&index, &types, session_key, |m| {
        if entered.is_some_and(|e| m.time > e + seconds) {
            return;
        }
        if next == f64::NEG_INFINITY {
            next = m.time;
        }
        while next <= m.time {
            frame(&mut c, &mut scene, next, &mut floating, &mut modes);
            observe(&mut c, &mut scene);
            next += step;
        }
        if !m.to_client {
            match ClientMessage::decode(&m.raw) {
                Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                    character: p.character,
                }),
                // The recorded player moved itself: its body stands where it said it stood.
                Ok(ClientMessage::Position(p)) if c.body.placed() => {
                    #[allow(clippy::cast_precision_loss)]
                    let heading = (p.heading_motion >> 24) as f32 * 360.0 / 255.0;
                    c.body.place(p.cell, p.origin, heading);
                }
                _ => {}
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        c.take_message(m.blob_id, msg);
        if entered.is_none() && c.model.player_object().is_some() {
            entered = Some(m.time);
        }
    })
    .expect("the index reads");
    // The last stories play out.
    for _ in 0..90 {
        frame(&mut c, &mut scene, next, &mut floating, &mut modes);
        observe(&mut c, &mut scene);
        next += step;
    }
    let r = Replay {
        tally: scene.combat.clone(),
        floating,
        modes,
    };
    (c, scene, r)
}

/// The first 40 seconds in the world of a 2017 session that fights at once (a human among
/// creatures that shoot): every story told on a body the scene has plays there. The attackers'
/// figures take their behaviors (once their modes are the server's); each hook asks what it shows;
/// every hit's damage is shown by the target's scripts, which send the floating text to the panels;
/// the stories on the player's body begin and end its attacks; the arrows are made, held in the
/// shooter's hand and fired, and enter the world in flight.
///
/// Behaviour: desktop.combat.recorded-stories-play-on-the-bodies
#[test]
fn recorded_stories_play_on_the_attackers_bodies_with_hits_texts_and_arrows() {
    let r = replay("1e177f6bfede2082:3", 40.0);
    let t = &r.tally;
    eprintln!("{t:?}; {} floating texts", r.floating);
    assert!(t.stories >= 15, "stories told: {t:?}");
    assert!(
        t.behaviors >= 10 && t.refused * 10 <= t.behaviors,
        "the attackers' figures took their behaviors: {t:?}"
    );
    assert!(t.hooks >= 2 * t.stories, "every story's hooks ran: {t:?}");
    assert!(t.hits >= 10, "hits shown: {t:?}");
    assert!(r.floating >= 5, "floating texts sent: {}", r.floating);
    assert!(
        t.attacks_started >= 10 && t.attacks_ended == t.attacks_started,
        "the player's attacks began and ended: {t:?}"
    );
    assert!(t.missiles_made >= 3, "arrows made: {t:?}");
    assert!(t.missiles_held >= 2, "arrows held: {t:?}");
    assert_eq!(
        t.missiles_fired, t.missiles_made,
        "every arrow fired: {t:?}"
    );
    assert_eq!(
        t.missiles_flying, t.missiles_fired,
        "every arrow flew: {t:?}"
    );
}

/// Four minutes in the world of two 2017 sessions: every missile fired is placed and flies. In the
/// first the player shoots a bow, its arrows made, held in its hand and let go of there; in the
/// second creatures' missiles are made as they are fired, from a look whose setup has no collision
/// spheres, at the shooter's place for them.
///
/// Behaviour: desktop.combat.recorded-missiles-are-placed-and-fly
#[test]
fn recorded_missiles_are_placed_and_fly_the_players_own_arrows_too() {
    let bow = replay("aae4d0e791da97b9:1", 240.0).tally;
    eprintln!("the player's bow: {bow:?}");
    assert!(bow.players_made >= 13, "the player's arrows made: {bow:?}");
    assert_eq!(bow.players_held, bow.players_made, "held in hand: {bow:?}");
    assert_eq!(
        bow.players_flying, bow.players_held,
        "let go and flying: {bow:?}"
    );
    assert_eq!(
        bow.missiles_flying, bow.missiles_fired,
        "every missile flew: {bow:?}"
    );
    let bare = replay("ce4c9e777aec77da:1", 240.0).tally;
    eprintln!("missiles of no spheres: {bare:?}");
    assert!(
        bare.missiles_fired >= 10 && bare.missiles_held < bare.missiles_fired,
        "missiles made as they are fired: {bare:?}"
    );
    assert_eq!(
        bare.missiles_flying, bare.missiles_fired,
        "every missile flew: {bare:?}"
    );
}

/// Four minutes of a 2017 session in which the player shoots a bow (its attacks' animations are
/// blends of two clips): each arrow is fired when the animation reaches its release, so it is let
/// go of where the drawn bow holds it, in front of the player and higher than a meter; it flies
/// clear of its shooter, and all but one strike the target they were aimed at.
///
/// Behaviour: desktop.combat.the-players-arrows-leave-the-drawn-bow-and-strike-their-target
#[test]
fn the_players_arrows_leave_the_drawn_bow_and_strike_their_target() {
    let t = replay("aae4d0e791da97b9:1", 240.0).tally;
    eprintln!("{t:?}");
    assert!(t.players_flying >= 13, "the player's arrows let go: {t:?}");
    assert_eq!(
        t.players_let_go_ahead, t.players_flying,
        "every arrow let go of at the drawn bow: {t:?}"
    );
    assert!(
        t.players_hit_target + 1 >= t.players_flying,
        "the player's arrows strike their target: {t:?}"
    );
    assert!(
        t.missiles_hit_target + 2 >= t.missiles_flying,
        "every missile but two strikes its target: {t:?}"
    );
}

/// In the world, a turn the player begins asks its scripts to stop an attack they started while
/// the body moved on its own; running forward does not.
///
/// Behaviour: desktop.combat.a-turn-begun-stops-an-attack-made-while-moving-on-its-own
#[test]
fn a_turn_begun_stops_an_attack_made_while_moving_on_its_own() {
    use auberean_client_sdk::runtime::interface::function::STOP_ATTACKING_IF_AUTO_MOVING;
    let (mut c, mut scene, _) = played("1e177f6bfede2082:3", 5.0);
    let mut t = c.local_time();
    let mut frame = |c: &mut Client, scene: &mut Scene<'_>, started: Vec<u32>| {
        t += FRAME;
        c.set_time(LocalTime(t));
        let intent = Intent {
            started,
            ..Intent::default()
        };
        c.asked.clear();
        #[allow(clippy::cast_possible_truncation)]
        scene.update(c, &intent, t, FRAME as f32);
        c.asked
            .iter()
            .filter(|(f, _)| *f == STOP_ATTACKING_IF_AUTO_MOVING)
            .count()
    };
    assert_eq!(
        frame(&mut c, &mut scene, vec![movement::FORWARD]),
        0,
        "running on"
    );
    assert_eq!(
        frame(&mut c, &mut scene, vec![movement::TURN_LEFT]),
        1,
        "a turn left"
    );
    assert_eq!(
        frame(&mut c, &mut scene, vec![movement::TURN_RIGHT]),
        1,
        "a turn right"
    );
    assert_eq!(frame(&mut c, &mut scene, Vec::new()), 0, "nothing begun");
}

/// While a behavior that locks the player's actions plays, what the keys ask of the body waits: it
/// stops at once and asks nothing, and stopping the behavior gives the keys back. A behavior asked
/// by a key plays and stops on the figure as one asked by its id alone.
///
/// Behaviour: desktop.bodies.a-behavior-that-locks-actions-holds-the-players-keys
#[test]
fn a_behavior_that_locks_actions_holds_the_players_keys() {
    use auberean_client_sdk::runtime::host::Behavior;
    use auberean_client_sdk::script::native_object::BehaviorParams;
    let (mut c, mut scene, _) = played("1e177f6bfede2082:3", 5.0);
    let id = c.model.player_object().expect("the player").id;
    let mut t = c.local_time();
    let mut frame = |c: &mut Client, scene: &mut Scene<'_>, intent: &Intent| {
        t += FRAME;
        c.set_time(LocalTime(t));
        #[allow(clippy::cast_possible_truncation)]
        scene.update(c, intent, t, FRAME as f32);
        scene.body(id).expect("its body").minterp.asked.y
    };
    let run = Intent {
        forward: 1.0,
        ..Intent::default()
    };
    assert_eq!(frame(&mut c, &mut scene, &run), 1.0, "the keys are heard");
    let behavior = auberean_physics::body::JUMP_BEHAVIOR;
    let params = BehaviorParams {
        behavior,
        lock_actions: 1,
        ..BehaviorParams::default()
    };
    c.scripts
        .as_mut()
        .expect("the scripts run")
        .host
        .behaviors
        .push(Behavior::Play { object: id, params });
    frame(&mut c, &mut scene, &run);
    assert!(
        scene.player_figure_plays(behavior),
        "the figure took the behavior"
    );
    frame(&mut c, &mut scene, &run);
    for _ in 0..5 {
        assert_eq!(
            frame(&mut c, &mut scene, &run),
            0.0,
            "the body stopped, the keys held"
        );
    }
    c.scripts
        .as_mut()
        .expect("the scripts run")
        .host
        .behaviors
        .push(Behavior::Stop {
            object: id,
            behavior,
        });
    frame(&mut c, &mut scene, &run);
    frame(&mut c, &mut scene, &run);
    assert_eq!(frame(&mut c, &mut scene, &run), 1.0, "the keys are back");
    // A key's behavior: played, then stopped.
    let key = Intent {
        behaviors: vec![(behavior, true)],
        ..Intent::default()
    };
    frame(&mut c, &mut scene, &key);
    assert!(scene.player_figure_plays(behavior), "the key played it");
    let key = Intent {
        behaviors: vec![(behavior, false)],
        ..Intent::default()
    };
    frame(&mut c, &mut scene, &key);
    assert!(
        !scene.player_figure_plays(behavior),
        "the key's release stopped it"
    );
}

/// What the objects' scripts set on their bodies reaches the bodies: in a recorded session each
/// body's own scale (the creatures' 0.85 to 1.2, as the scripts read it from their qualities when
/// they were made) and whether it slides along edges; and when the scripts then scale a creature,
/// hide it, turn its edge sliding over, make it undetectable and give it a jump scale, the next
/// frame its body takes all five, its entry in the world's table reaches as far as its whole
/// scale, and the frame does not draw it. After each frame the scripts can read every body's
/// element and the motion it was last asked for.
///
/// Behaviour: desktop.bodies.the-scripts-scale-hide-and-slide-the-bodies
#[test]
fn the_scripts_scale_hide_and_slide_the_bodies() {
    let (mut c, mut scene, _) = played("1e177f6bfede2082:3", 40.0);
    let states = &c.scripts.as_ref().expect("the scripts run").host.bodies;
    let (mut scaled, mut sliding) = (0, 0);
    let mut creature = None;
    for (id, st) in states {
        let Some(b) = scene.body(*id) else {
            continue;
        };
        if let Some(s) = st.scale {
            assert!((b.object_scale.x - s).abs() < 1e-6, "{id:#x}: {s}");
            scaled += 1;
        }
        if let Some(on) = st.edge_slide {
            assert_eq!(b.does_edge_slide(), on, "{id:#x}");
            sliding += 1;
        }
        assert_eq!(b.is_no_draw(), st.no_draw, "{id:#x}");
        if st.scale.is_some() && b.in_world && creature.is_none_or(|c| *id < c) {
            creature = Some(*id);
        }
    }
    eprintln!("{scaled} scaled, {sliding} told of edges");
    assert!(
        scaled >= 10 && sliding >= 10,
        "{scaled} scaled, {sliding} told of edges"
    );
    let id = creature.expect("a scaled creature in the world");
    let slides = scene.body(id).is_some_and(|b| b.does_edge_slide());
    let key = scene.table().find(id).expect("in the table");
    let before = (
        scene.body(id).expect("its body").total_scale(None),
        scene.table().reach(key).expect("its reach"),
    );
    let mut t = c.local_time();
    scene.frame(t);
    assert!(scene.seen.bodies.iter().any(|s| s.id == id), "drawn before");
    {
        let st = c
            .scripts
            .as_mut()
            .expect("the scripts run")
            .host
            .bodies
            .entry(id)
            .or_default();
        st.scale = Some(1.7);
        st.no_draw = true;
        st.edge_slide = Some(!slides);
        st.undetectable = Some(true);
        st.jump_scale = Some(2.5);
    }
    t += FRAME;
    c.set_time(LocalTime(t));
    #[allow(clippy::cast_possible_truncation)]
    scene.update(&mut c, &Intent::default(), t, FRAME as f32);
    let b = scene.body(id).expect("its body");
    assert!(
        (b.object_scale.z - 1.7).abs() < 1e-6,
        "{:?}",
        b.object_scale
    );
    assert!(b.is_no_draw());
    assert_eq!(b.does_edge_slide(), !slides);
    assert!(b.is_undetectable(), "made undetectable");
    assert!(
        (b.jump_scale - 2.5).abs() < 1e-6,
        "jump scale {}",
        b.jump_scale
    );
    let readings = &c.scripts.as_ref().expect("the scripts run").host.readings;
    let player = c.model.player.expect("the player");
    for who in [id, player] {
        let body = scene.body(who).expect("a body");
        let r = readings.get(&who).expect("its reading");
        assert_eq!(r.element, body.element(), "{who:#x}");
        let m = body.minterp.asked;
        assert_eq!(r.last_motion, [m.x, m.y, m.z], "{who:#x}");
    }
    let whole = b.total_scale(None);
    let key = scene.table().find(id).expect("in the table");
    let reach = scene.table().reach(key).expect("its reach");
    let grew = whole.z / before.0.z;
    assert!(grew > 1.0, "{whole:?} from {:?}", before.0);
    assert!(
        (reach / before.1 - grew).abs() < 1e-3,
        "reach {reach} from {} for {grew}",
        before.1
    );
    scene.frame(t);
    assert!(scene.drawn_batches(id).is_none(), "not drawn");
}

/// A minute in the world of a 2017 session in which the server scales the speed and the
/// acceleration of the player (2.185 times) and of the creatures about it: each scale
/// reaches the body it was sent for within a second (the frame after it takes effect), whether
/// the body is the player's or another's, and stays until the next.
///
/// Behaviour: desktop.bodies.the-servers-speed-scales-reach-the-bodies
#[test]
fn the_servers_speed_and_acceleration_scales_reach_the_bodies() {
    use std::collections::BTreeMap;
    const SESSION: &str = "9e56c155870fcf8d:1";
    const SECONDS: f64 = 60.0;
    // The scales the session sent, in the order it sent them: the time, the object, which scale
    // (0 speed, 1 acceleration, 2 jump), the value.
    let index = auberean_testkit::data::require_capture_index();
    let mut sent: Vec<(f64, u64, usize, f32)> = Vec::new();
    let mut entered = None;
    captures::capture_messages_in_time_order(
        &index,
        &[0x10060, 0x10092, 0x10093, 0x100BA],
        SESSION,
        |m| {
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            let (e, kind) = match msg {
                ServerMessage::PlayerObject(_) => {
                    entered.get_or_insert(m.time);
                    return;
                }
                ServerMessage::VelocityScale(e) => (e, 0),
                ServerMessage::AccelerationScale(e) => (e, 1),
                ServerMessage::JumpScale(e) => (e, 2),
                _ => return,
            };
            sent.push((m.time, e.sender.id, kind, e.action.0));
        },
    )
    .expect("the index reads");
    let entered = entered.expect("the player entered");
    // What each of those objects' bodies held after each frame.
    let ids: Vec<u64> = sent.iter().map(|s| s.1).collect();
    let mut held: Vec<(f64, BTreeMap<u64, [f32; 3]>)> = Vec::new();
    let (c, _, _) = played_in_frames(SESSION, SECONDS, FRAME, &[], &mut |c, scene| {
        let now = ids
            .iter()
            .filter_map(|id| {
                let b = scene.body(*id)?;
                Some((
                    *id,
                    [b.velocity_scale(), b.acceleration_scale(), b.jump_scale()],
                ))
            })
            .collect();
        held.push((c.local_time(), now));
    });
    let player = c.model.player.expect("the player");
    let (mut judged, mut reached, mut players) = (0, 0, 0);
    let mut missed = Vec::new();
    for (i, &(t, id, kind, v)) in sent.iter().enumerate() {
        if t > entered + SECONDS - 1.0 {
            continue;
        }
        // Until the next scale of the same kind for the same object, or a second.
        let until = sent[i + 1..]
            .iter()
            .find(|s| s.1 == id && s.2 == kind)
            .map_or(t + 1.0, |s| s.0.min(t + 1.0));
        let frames: Vec<f32> = held
            .iter()
            .filter(|(at, _)| *at > t && *at <= until)
            .filter_map(|(_, bodies)| bodies.get(&id).map(|s| s[kind]))
            .collect();
        if frames.is_empty() {
            continue;
        }
        judged += 1;
        players += usize::from(id == player);
        if frames.iter().any(|s| (s - v).abs() < 1e-6) {
            reached += 1;
        } else if missed.len() < 8 {
            missed.push(format!("{id:#x} kind {kind} {v} at {t:.2}: {frames:?}"));
        }
    }
    eprintln!("{reached} of {judged} scales reached their bodies ({players} the player's)");
    assert!(
        judged >= 30 && players >= 1,
        "{judged} judged, {players} the player's"
    );
    assert_eq!(reached, judged, "missed: {missed:#?}");
}

/// With the scene placing the bodies, each object's scripts are told it is in the world exactly
/// while its body stands in it (or, with no body, while its place's cell is loaded): creatures
/// with a place in cells no longer loaded around the player were told they left; an object held
/// or contained is out of it.
///
/// Behaviour: desktop.bodies.an-object-is-in-the-world-while-its-body-stands-there
#[test]
fn an_object_is_in_the_world_while_its_body_stands_there() {
    use auberean_client_sdk::runtime::lifecycle::function::{
        HANDLE_ENTER_WORLD, HANDLE_LEAVE_WORLD,
    };
    let (c, scene, _) = played("1e177f6bfede2082:3", 40.0);
    let (mut standing, mut placed_out) = (0, 0);
    for o in c.model.objects() {
        let stands = scene.stands(o.id);
        assert_eq!(c.life.in_world(o.id), stands, "{:#x}", o.id);
        standing += usize::from(stands);
        placed_out += usize::from(
            !stands
                && o.in_world
                && o.place.is_some()
                && o.parent.is_none()
                && o.container.is_none(),
        );
    }
    let told = |f| c.life.told.get(&f).copied().unwrap_or(0);
    eprintln!(
        "{standing} in the world, {placed_out} placed but out; told {} in, {} out",
        told(HANDLE_ENTER_WORLD),
        told(HANDLE_LEAVE_WORLD)
    );
    assert!(standing >= 10, "{standing} in the world");
    assert!(
        placed_out >= 3 && told(HANDLE_LEAVE_WORLD) >= 3,
        "{placed_out} placed in cells not loaded"
    );
    assert!(c.bodies_place);
}

/// A minute of a recorded combat session: the effects the bodies' animations start at moments of
/// their cycles are played on the bodies, and stopped when the animations that started them end;
/// each frame's trigger update is then cleaned up, so an effect starts once.
///
/// Behaviour: desktop.combat.the-animations-own-effects-play-on-the-bodies
#[test]
fn the_animations_own_effects_play_on_the_bodies() {
    let (_, scene, _) = played("1e177f6bfede2082:3", 60.0);
    let (started, stopped) = scene.status.figure_fx;
    eprintln!("{started} started, {stopped} stopped");
    assert!(started >= 30, "{started} started");
    assert!(stopped >= 30, "{stopped} stopped");
}

/// A recorded session's bodies, frame by frame: the systems their effects carry draw at the body's
/// whole scale, its look's times its own, however the look alone is scaled.
///
/// Behaviour: desktop.looks.a-bodys-systems-draw-at-its-whole-scale
#[test]
fn a_bodys_systems_draw_at_its_whole_scale() {
    let mut t = f64::NEG_INFINITY;
    let mut own_scale = std::collections::BTreeSet::new();
    let mut wrong = Vec::new();
    played_in_frames("aae4d0e791da97b9:1", 240.0, 0.125, &[], &mut |c, scene| {
        t = if t.is_finite() { t + 0.125 } else { 0.0 };
        let _ = scene.frame(t);
        for o in c.model.objects() {
            let Some(b) = scene.body(o.id) else { continue };
            let own = b.object_scale;
            for (drawn, want) in scene.system_scales(o.id) {
                if !drawn.is_empty() && (own.x - 1.0).abs() > 1e-3 {
                    own_scale.insert(o.id);
                }
                wrong.extend(drawn.into_iter().filter(|d| {
                    (d.x - want.x).abs() > 1e-3
                        || (d.y - want.y).abs() > 1e-3
                        || (d.z - want.z).abs() > 1e-3
                }));
            }
        }
    });
    eprintln!("{} bodies with their own scale", own_scale.len());
    assert!(
        !own_scale.is_empty(),
        "a body with its own scale drew a system"
    );
    assert!(
        wrong.is_empty(),
        "{} drew at another scale: {:?}",
        wrong.len(),
        wrong.first()
    );
}

/// A recorded session in which the player shoots a bow, played in frames an eighth of a second
/// long (several physics ticks each): the missiles travel from where they were placed, and those
/// that stick (2017's sticky arrows) stick in the target they struck.
///
/// Behaviour: desktop.combat.a-missile-that-strikes-sticks-or-fades
#[test]
fn a_missile_that_strikes_sticks_or_fades() {
    let t = played_in_frames("aae4d0e791da97b9:1", 240.0, 0.125, &[], &mut |_, _| {})
        .2
        .tally;
    eprintln!("{t:?}");
    assert!(t.players_flying >= 13, "the player's arrows placed: {t:?}");
    assert!(t.missiles_travelled >= 28, "missiles travelled: {t:?}");
    assert!(
        t.missiles_stuck >= 2,
        "missiles stuck in their targets: {t:?}"
    );
}

/// The first 40 seconds of a 2017 session in which creatures attack the player: a story whose
/// hit strikes the player with a harmful skill makes its attacker the player's last attacker,
/// which the last attacker's key selects.
///
/// Behaviour: desktop.combat.the-creature-that-strikes-the-player-is-its-last-attacker
#[test]
fn the_creature_that_strikes_the_player_is_its_last_attacker() {
    let (c, _, r) = played("1e177f6bfede2082:3", 40.0);
    let attacker = c.last_attacker;
    let player = c.model.player.expect("the player");
    assert!(attacker != 0, "an attacker noted: {:?}", r.tally);
    assert_ne!(attacker, player, "not the player itself");
    assert!(
        c.model.object(attacker).is_some(),
        "an object the client knows: {attacker:#x}"
    );
}

/// The mode a creature's death behavior carries: dead.
const DEAD: u32 = 0x4100_0005;

/// Four minutes of a recorded session in which a creature dies near the player: the death behavior
/// (which carries the dead mode) puts the creature's figure in that mode before it plays, and the
/// figure stays in it, through the positions with idle motion the server sends after it, until the
/// creature goes.
///
/// Behaviour: desktop.combat.a-dead-creature-stays-down
#[test]
fn a_dead_creature_stays_down() {
    // Each dead creature: the position stamp it had when it died, and the positions since.
    let mut dead: std::collections::BTreeMap<u64, (u16, u32)> = std::collections::BTreeMap::new();
    let mut stood = Vec::new();
    played_in_frames("1e177f6bfede2082:3", 240.0, FRAME, &[], &mut |c, scene| {
        for o in c.model.objects() {
            let Some(f) = scene.figure(o.id) else {
                continue;
            };
            let stamp = o.place.as_ref().map_or(0, |p| p.position_stamp);
            if f.modes.desired_mode == DEAD {
                let d = dead.entry(o.id).or_insert((stamp, 0));
                if stamp != d.0 {
                    d.0 = stamp;
                    d.1 += 1;
                }
            } else if dead.contains_key(&o.id) && !stood.contains(&o.id) {
                stood.push(o.id);
            }
        }
    });
    eprintln!("died (positions after): {dead:x?}; stood up again: {stood:x?}");
    assert!(!dead.is_empty(), "none died");
    assert!(
        dead.values().any(|(_, n)| *n >= 1),
        "no position after a death: {dead:x?}"
    );
    assert!(stood.is_empty(), "stood up again: {stood:x?}");
}

/// The server's message setting an object's mode outright.
const SET_MODE: u32 = 0x1_00A9;
/// The mode a corpse is created in.
const CORPSE: u32 = 0x4100_0003;

/// The same session with the server's outright mode changes left out: a corpse is created lying
/// dead, its figure in the corpse mode its creation carries from the first frame it is drawn.
///
/// Behaviour: desktop.combat.a-corpse-is-made-lying-dead
#[test]
fn a_corpse_is_made_lying_dead() {
    let mut corpses = std::collections::BTreeMap::new();
    played_in_frames(
        "aae4d0e791da97b9:1",
        240.0,
        FRAME,
        &[SET_MODE],
        &mut |c, scene| {
            for o in c.model.objects() {
                let carries =
                    o.physics.bits & auberean_client_sdk::protocol::types::PhysicsDesc::MODE != 0
                        && o.physics.mode == CORPSE;
                if let Some(f) = scene.figure(o.id).filter(|_| carries) {
                    corpses.entry(o.id).or_insert(f.modes.desired_mode);
                }
            }
        },
    );
    eprintln!("corpses' first modes: {corpses:x?}");
    assert!(!corpses.is_empty(), "no corpse");
    assert!(corpses.values().all(|m| *m == CORPSE), "{corpses:x?}");
}

/// Twenty-five minutes of a 2017 session among players who emote: every behavior the server asks
/// of a body with an emote is first handed to that object's scripts, which take each one, and so
/// it plays.
///
/// Behaviour: desktop.bodies.a-behavior-with-an-emote-plays-when-the-objects-scripts-take-it
#[test]
fn a_behavior_with_an_emote_plays_when_the_objects_scripts_take_it() {
    let t = played_in_frames("478cf1fc123fa081:1", 1500.0, 0.25, &[], &mut |_, _| {})
        .2
        .tally;
    eprintln!("{t:?}");
    assert!(t.emotes_asked >= 15, "emotes asked: {t:?}");
    assert_eq!(t.emotes_refused, 0, "every one taken: {t:?}");
}

/// Eight minutes of a 2017 session in which the player shoots creatures down, each frame drawn: the
/// servers play the wounded effect (`0x41000002`, at one less the creature's share of its health)
/// on a creature struck below four fifths of its health, again at each later blow, and its effect
/// table lays value 2 of four appearance tables over it at that strength. Each wounded body's own
/// descriptions take the wound's appearance and its figure is drawn as the total they give: five
/// creatures' parts otherwise than the descriptions they were sent say (the player's, wounded
/// lightly, draw no differently). A wound the servers stop comes off.
///
/// Behaviour: desktop.looks.an-effects-appearance-dresses-the-body-it-plays-on
#[test]
fn a_wounded_creature_bleeds_until_its_wound_stops() {
    use std::collections::{BTreeMap, BTreeSet};
    const BLOOD: u32 = 0x2000_003A;
    const STEP: f64 = 0.125;
    let (world, _) = data();
    let anims = auberean_animation::CachedRecords::new(&world.records);
    let mut t = f64::NEG_INFINITY;
    let mut wounded: BTreeSet<u64> = BTreeSet::new();
    let mut healed: BTreeSet<u64> = BTreeSet::new();
    let mut dressed: BTreeSet<u64> = BTreeSet::new();
    let mut mismatched: BTreeSet<u64> = BTreeSet::new();
    let mut strengths: BTreeMap<u64, f32> = BTreeMap::new();
    played_in_frames("aae4d0e791da97b9:1", 480.0, STEP, &[], &mut |c, scene| {
        t = if t.is_finite() { t + STEP } else { 0.0 };
        let _ = scene.frame(t);
        for o in c.model.objects() {
            let Some(d) = scene.dress(o.id) else {
                continue;
            };
            let blood = d
                .effects
                .global
                .appearance
                .get(BLOOD)
                .and_then(|k| k.get(2))
                .copied();
            match (blood, strengths.get(&o.id).copied()) {
                (Some(v), _) if v > 0.0 => {
                    wounded.insert(o.id);
                    strengths.insert(o.id, v);
                }
                (_, Some(_)) => {
                    healed.insert(o.id);
                    strengths.remove(&o.id);
                }
                _ => {}
            }
            let Some(f) = scene.figure(o.id) else {
                continue;
            };
            if blood.is_some() {
                let total = d.total(&anims).expect("its total");
                let sent =
                    auberean_scene::look_of(&world.records, &anims, &o.visual).expect("its look");
                let looks: Vec<_> = f.parts.iter().map(|p| p.look.clone()).collect();
                let of = |l: &auberean_world::look::Look| -> Vec<_> {
                    f.parts
                        .iter()
                        .map(|p| {
                            l.atoms(&anims, p.key)
                                .map(auberean_world::look::PartLook::from_atoms)
                                .unwrap_or_default()
                        })
                        .collect()
                };
                if looks != of(&total) {
                    mismatched.insert(o.id);
                } else if looks != of(&sent) {
                    dressed.insert(o.id);
                }
            }
        }
    });
    eprintln!(
        "{} wounded, {} dressed, {} healed, {} mismatched",
        wounded.len(),
        dressed.len(),
        healed.len(),
        mismatched.len()
    );
    assert!(wounded.len() >= 6, "wounded: {wounded:x?}");
    assert!(dressed.len() >= 5, "dressed: {dressed:x?}");
    assert!(healed.len() >= 2, "healed: {healed:x?}");
    assert!(mismatched.is_empty(), "drawn as the total: {mismatched:x?}");
}

/// Four minutes of a 2017 session in which the player shoots a bow, the player moved only as the
/// server moves it: before each shot the servers order the player's own body to turn to its target
/// (a move to the object asking only to turn) and stop it after; the body takes each order and
/// turns, so at each order's end it faces its target within the turn's ten degrees.
///
/// Behaviour: desktop.combat.the-server-turns-the-player-to-what-it-attacks
#[test]
fn the_server_turns_the_player_to_face_what_it_shoots() {
    use auberean_physics::geom::{heading_to, Heading};
    use auberean_physics::motion::params::flags;
    // The order under way: its target, and how far the body faced from it at the last frame.
    let mut under_way: Option<(u64, f32)> = None;
    let (mut orders, mut faced) = (0u32, 0u32);
    let mut off = Vec::new();
    played_in_frames(
        "aae4d0e791da97b9:1",
        240.0,
        FRAME,
        &[CLIENT_POSITION],
        &mut |c, scene| {
            let Some(b) = c.model.player.and_then(|p| scene.body(p)) else {
                return;
            };
            let turning = b
                .minterp
                .move_to
                .as_ref()
                .filter(|m| {
                    m.moving_to
                        && m.params.flags & flags::TURN != 0
                        && m.params.flags & flags::MOVE_TOWARDS == 0
                        && m.params.to_object()
                })
                .map(|m| m.params.target_object);
            let away = turning.and_then(|t| {
                let target = scene.body(t)?;
                let h = heading_to(&b.position, &target.position);
                Some(h.minus(Heading::new(b.heading)).abs().degrees())
            });
            match (turning.zip(away), under_way) {
                (Some((t, a)), _) => under_way = Some((t, a)),
                (None, Some((_, a))) => {
                    orders += 1;
                    faced += u32::from(a <= 10.0);
                    off.push(a.round());
                    under_way = None;
                }
                (None, None) => {}
            }
        },
    );
    eprintln!("{orders} turns ordered, {faced} faced their target: {off:?}");
    assert!(orders >= 5, "turns ordered: {orders}");
    assert_eq!(faced, orders, "each faced its target: {off:?}");
}

/// A 2017 session in which the server sends the player's own object a new description just after
/// it enters the world: the player's figure takes it, its parts drawn as the new description says
/// and no longer as at first, as another body's figure takes a new description.
///
/// Behaviour: desktop.looks.an-effects-appearance-dresses-the-body-it-plays-on
#[test]
fn the_players_own_new_description_dresses_its_figure() {
    use auberean_world::look::PartLook;
    let (world, _) = data();
    let anims = auberean_animation::CachedRecords::new(&world.records);
    let session = "9d5beaba36365ea1:1";
    let mut first: Option<Vec<(u32, PartLook)>> = None;
    let (c, scene, _) = played_in_frames(session, 30.0, FRAME, &[], &mut |c, scene| {
        if first.is_none() {
            first = c
                .model
                .player
                .and_then(|p| scene.figure(p))
                .map(|f| f.parts.iter().map(|p| (p.key, p.look.clone())).collect());
        }
    });
    let player = c.model.player_object().expect("the player");
    let figure = scene.figure(player.id).expect("its figure");
    let drawn = |f: &auberean_animation::body::Body| -> Vec<(u32, PartLook)> {
        f.parts.iter().map(|p| (p.key, p.look.clone())).collect()
    };
    let now = drawn(figure);
    let look = auberean_scene::look_of(&world.records, &anims, &player.visual).expect("its look");
    let fresh = auberean_animation::body::Body::from_look(&anims, &look).expect("a figure");
    let (as_said, changed) = (now == drawn(&fresh), first.as_ref() != Some(&now));
    eprintln!("{session}: drawn as said {as_said}, changed from the first {changed}");
    assert!(as_said, "drawn as its description says");
    assert!(changed, "no longer as at first");
}

/// What a body holds takes the profile its own qualities give when it is made, as every object's
/// body does: in a recorded session each held item slides along edges, ignores collisions and is
/// scaled as its qualities say.
///
/// Behaviour: desktop.bodies.a-held-object-takes-the-profile-its-qualities-give
#[test]
fn a_held_object_takes_the_profile_its_qualities_give() {
    use auberean_physics::body::flags;
    let (c, scene, _) = played("1e177f6bfede2082:3", 40.0);
    let (world, _) = data();
    let mut held = 0;
    for o in c.model.objects().filter(|o| o.parent.is_some()) {
        let Some(b) = scene.body(o.id) else {
            continue;
        };
        let p = auberean_scene::profile::profile_of(o, |id| world.records.get(id));
        let id = o.id;
        assert_eq!(
            b.does_edge_slide(),
            p.state & flags::EDGE_SLIDE != 0,
            "{id:#x}: {:#x}",
            p.state
        );
        assert_eq!(
            b.ignores_collisions(),
            p.state & flags::IGNORE_COLLISIONS != 0,
            "{id:#x}: {:#x}",
            p.state
        );
        assert!((b.object_scale.z - p.scale).abs() < 1e-6, "{id:#x}");
        held += 1;
    }
    assert!(held >= 1, "a held item");
}

/// Seven minutes of a 2017 session in which a missile that goes off on a miss is shot at
/// creatures: each stops on the creature it meets, as the object's kind says, rather than flying
/// on through it to whatever lies behind. Every body is taken for a player or a creature by its
/// kind: the player's own, a creature's, and not an item's.
///
/// Behaviour: desktop.combat.a-missile-that-goes-off-on-a-miss-stops-on-the-creature-it-meets
#[test]
fn a_missile_that_goes_off_on_a_miss_stops_on_the_creature_it_meets() {
    use auberean_client_sdk::runtime::repository::{answer, is_creature, INT};
    let (mut players, mut creatures, mut neither) = (0u64, 0u64, 0u64);
    // Each missile by its id, a later one taking a used id starting over: what it was aimed at,
    // what it struck, and whether that is a creature.
    let mut flown: Vec<(u64, u64, Option<u64>, bool)> = Vec::new();
    let mut flying: HashMap<u64, usize> = HashMap::new();
    played_in_frames("9f6f8d17804be3b5:2", 420.0, FRAME_30, &[], &mut |c, s| {
        let ids: Vec<u64> = c.model.objects().map(|o| o.id).collect();
        for id in ids {
            let Some(noted) = s.noted_player_or_creature(id) else {
                continue;
            };
            let kind = c.with_host(|h| answer(h, &INT, id, 0x14)).flatten();
            match kind.map(|k| k & 0xFF) {
                Some(0x1F) => {
                    assert!(noted, "{id:x}: a player's kind");
                    players += 1;
                }
                Some(0x4F) => {
                    assert!(noted, "{id:x}: a creature's kind");
                    creatures += 1;
                }
                _ => {
                    assert!(!noted, "{id:x}: {kind:x?}, neither");
                    neither += 1;
                }
            }
        }
        let made = s.made_objects();
        flying.retain(|id, _| made.contains(id));
        for id in made {
            let Some(b) = s.body(id) else { continue };
            let Some(g) = b.missile.as_deref().filter(|g| g.params.detonates_on_miss) else {
                continue;
            };
            let at = *flying.entry(id).or_insert_with(|| {
                flown.push((id, g.targeting.target, None, false));
                flown.len() - 1
            });
            if !g.activated && flown[at].2.is_none() {
                let creature = b
                    .struck
                    .is_some_and(|o| c.with_host(|h| is_creature(h, o)) == Some(true));
                flown[at].2 = b.struck;
                flown[at].3 = creature;
            }
        }
    });
    eprintln!("{flown:x?}");
    assert!(flown.len() >= 10, "missiles flown: {flown:x?}");
    assert!(
        players > 0 && creatures > 0 && neither > 0,
        "{players} players, {creatures} creatures, {neither} neither"
    );
    for (id, target, struck, creature) in &flown {
        assert_eq!(*struck, Some(*target), "{id:x} went off on its target");
        assert!(creature, "{id:x} struck a creature");
    }
}
