//! A `Movement_SetObjectMovement 0xF74C` whose stick-to-object flag names a target sticks a remote
//! creature to it: every buffer first takes any stick off, the named id resolves (one hop to its
//! parent, before the target's radius and height are read), the position manager records it, a
//! later plain buffer or a second without an update takes it off, and each frame pulls the
//! creature toward its target until it stands off it. The server sends a stuck creature almost no
//! `0xF748` positions, so without the pull it stands where it was last told while the player
//! walks away. The player's own stick is recorded but not pulled; a server object's pull is not
//! collided against the world. Fixture: every retained raw capture, decoded here, and
//! `early-inventory-and-casting` and `long-solo-play` replayed through `ObjectStream`,
//! `WorldScene::sync_objects` and the frame update into a real scene.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::recorded_movement_events;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Position};
use dereth_protocol::movement::{
    motion_flags, movement_type, MovementBody, MovementSetObjectMovement,
};
use dereth_protocol::{Message, Opcode};
use dereth_render::device::Gpu;

// ---------------------------------------------------------------------------------------------
// 1. The corpus.
// ---------------------------------------------------------------------------------------------

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

struct Wire {
    session: String,
    mover: ObjectId,
    autonomous: bool,
    body: MovementBody,
}

/// Every recorded `0xF74C`, with the character ids its session's `0xF658` named.
fn corpus_buffers() -> (Vec<Wire>, BTreeMap<String, Vec<ObjectId>>) {
    let mut all = Vec::new();
    let mut chars: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    for session in corpus_sessions() {
        let records = shared_session(&session);
        let csn = connection_sequence_number(records).unwrap_or(0);
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a replay net");
        for r in records.iter().filter(|r| !r.c2s) {
            net.feed(&r.raw, r.peer(), LocalTime(r.t));
        }
        let mine = chars.entry(session.clone()).or_default();
        while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
            if m.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0 {
                let mut r = dereth_protocol::Reader::new(&m.body);
                if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                    for c in &set.characters {
                        if !mine.contains(&c.gid) {
                            mine.push(c.gid);
                        }
                    }
                }
                continue;
            }
            if m.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 {
                continue;
            }
            let mut r = dereth_protocol::Reader::new(&m.body);
            let msg = MovementSetObjectMovement::read(&mut r).expect("a 0xF74C decodes");
            let buf = msg.decoded_movement().expect("its buffer decodes");
            all.push(Wire {
                session: session.clone(),
                mover: msg.id,
                autonomous: buf.autonomous,
                body: buf.body,
            });
        }
    }
    assert!(!all.is_empty(), "the corpus carries no 0xF74C at all");
    (all, chars)
}

// ---------------------------------------------------------------------------------------------
// 2. The census.
// ---------------------------------------------------------------------------------------------

/// **The stick-to-object buffers the corpus carries, measured.**
///
/// The same remote population `movement::remote_motion_style` counts with its own reader, and
/// three facts about the sticks in it:
///
/// 1. **Every remote stick names a character**, never scenery or a creature. That is why
///    `WorldScene::objects` alone is the wrong table to resolve a target in: with a local body the
///    player is not in it, so a stick to him would find no target and stick to nothing.
/// 2. The corpus also carries `StickToObject` buffers *addressed to* the session's own character,
///    and **none of them is autonomous**, so the movement update's echo check does not refuse
///    them; they reach `apply_player_movement`.
/// 3. Every standing long jump addressed to the session's own character is an autonomous echo of
///    the client's own jump, so the flag is only observable on the remote arm.
///
/// Every count here is read off the corpus; the only literals are the wire flag bits.
#[test]
fn every_recorded_remote_stick_names_the_sessions_own_character() {
    let (all, chars) = corpus_buffers();
    let (mut remote, mut arms) = (0usize, 0usize);
    let mut by_type = [0usize; 10];
    let mut movers: BTreeSet<(String, ObjectId)> = BTreeSet::new();
    let (mut remote_sticky, mut remote_longjump) = (0usize, 0usize);
    let (mut player_sticky, mut player_sticky_autonomous, mut player_longjump) = (0usize, 0, 0);
    let mut player_longjump_autonomous = 0usize;
    let mut stick_pairs: BTreeSet<(String, ObjectId)> = BTreeSet::new();
    let mut stick_targets: BTreeSet<ObjectId> = BTreeSet::new();
    let (mut stick_names_a_character, mut stick_names_self, mut stick_names_zero) = (0, 0, 0);
    for b in &all {
        let sticky = b.body.motion_flags & motion_flags::STICK_TO_OBJECT != 0;
        let longjump = b.body.motion_flags & motion_flags::STANDING_LONG_JUMP != 0;
        if chars[&b.session].contains(&b.mover) {
            if sticky {
                player_sticky += 1;
                player_sticky_autonomous += usize::from(b.autonomous);
            }
            if longjump {
                player_longjump += 1;
                player_longjump_autonomous += usize::from(b.autonomous);
            }
            continue;
        }
        remote += 1;
        by_type[b.body.movement_type as usize] += 1;
        movers.insert((b.session.clone(), b.mover));
        if b.body.movement_type != movement_type::INVALID {
            arms += 1;
        }
        if longjump {
            remote_longjump += 1;
        }
        if sticky {
            remote_sticky += 1;
            let t = b.body.sticky_object.expect("STICK_TO_OBJECT carries an id");
            stick_pairs.insert((b.session.clone(), b.mover));
            stick_targets.insert(t);
            if t == ObjectId(0) {
                stick_names_zero += 1;
            } else if t == b.mover {
                stick_names_self += 1;
            }
            if chars[&b.session].contains(&t) {
                stick_names_a_character += 1;
            }
        }
    }
    eprintln!(
        "remote stick: {remote} remote 0xF74C from {} objects, by arm {by_type:?}; \
         remote sticks {remote_sticky}, player sticks {player_sticky}",
        movers.len()
    );

    eprintln!(
        "stick census all={} remote={remote} arms={arms} movers={} remote_sticky={remote_sticky} \
         remote_longjump={remote_longjump} names_char={stick_names_a_character} \
         names_self={stick_names_self} names_zero={stick_names_zero} pairs={} targets={} \
         player_sticky={player_sticky} player_sticky_auto={player_sticky_autonomous} \
         player_longjump={player_longjump} player_longjump_auto={player_longjump_autonomous}",
        all.len(),
        movers.len(),
        stick_pairs.len(),
        stick_targets.len()
    );
    eprintln!("stick census stick_pairs={stick_pairs:?} stick_targets={stick_targets:?}");
    // The expected side is the corpus's own `0xF74C` netblob count, while `all` above is this
    // file's independent reassembly and decode of the retained raw datagrams.
    assert_eq!(
        all.len(),
        recorded_movement_events(),
        "every recorded 0xF74C body was decoded"
    );
    // Every remote buffer is the interpreted arm or one of the four move-to/turn-to arms.
    assert_eq!(
        by_type[0] + by_type[6] + by_type[7] + by_type[8] + by_type[9],
        remote,
        "a remote 0xF74C with an arm other than interpreted, MoveTo or TurnTo: {by_type:?}"
    );
    assert!(
        remote_sticky > 0,
        "the corpus carries no remote stick at all"
    );
    assert_eq!(
        stick_names_a_character, remote_sticky,
        "every remote stick names the session's own character -- if this ever moves, \
         `stick_target`'s object table is the thing to re-check"
    );
    assert_eq!(
        (stick_names_self, stick_names_zero),
        (0, 0),
        "no self-stick and no zero id"
    );
    // The line above is also why the stick's hop-then-measure order cannot be observed in this
    // corpus: it reads its radius and height off the object it hopped *to*, while move-to-object
    // reads them off the object the server *named*, and with every stick naming a character --
    // who is never a child, so the hop is the identity -- the two orders cannot disagree. The
    // distinction is kept because retail has it, not because a test here can see it.
    assert!(
        player_sticky > 0,
        "the corpus carries no StickToObject addressed to the session's character"
    );
    assert_eq!(
        player_sticky_autonomous, 0,
        "a StickToObject addressed to the session's character is an autonomous echo"
    );
    // Every `StandingLongJump` addressed to the player is **autonomous** -- the server echoing back
    // the client's own jump -- so the movement update's autonomous-echo check refuses it and
    // `apply_player_movement` returns before reading a flag. The flag is therefore unobservable on
    // the player's arm; the remote arm is where its natural instances are.
    assert!(
        player_longjump > 0 && remote_longjump > 0,
        "the corpus carries StandingLongJump on both arms ({player_longjump} player, \
         {remote_longjump} remote)"
    );
    assert_eq!(
        player_longjump_autonomous, player_longjump,
        "a StandingLongJump buffer addressed to the player is no longer an autonomous echo, so it \
         now reaches `apply_player_movement` and the wiring became falsifiable -- assert it"
    );

    // Pinned as literals: a table read through the same accessor it is written through cannot
    // detect a wrong constant.
    assert_eq!(
        motion_flags::STICK_TO_OBJECT,
        0x01,
        "the high byte's bit 0 -- combined & 0x100"
    );
    assert_eq!(
        motion_flags::STANDING_LONG_JUMP,
        0x02,
        "bit 1 -- combined & 0x200"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The seam: a real `WorldScene` driven by a capture's own datagrams.
// ---------------------------------------------------------------------------------------------

fn warp() -> Gpu {
    crate::common::software_gpu(320, 240)
}

/// The retail dats, or **fail**: a missing dat is a failure, never a silent pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// One recorded stick, as it arrived at `WorldScene::sync_objects`.
#[derive(Debug, Clone)]
struct StickStation {
    mover: ObjectId,
    named: ObjectId,
    /// What the scene's own `PositionManager` held afterwards.
    recorded: Option<ObjectId>,
    /// Whether the mover was resident in the scene at all.
    resident: bool,
    /// Whether the named target resolved -- the local player, or an object in the table.
    resolved: bool,
}

struct Run {
    sticks: Vec<StickStation>,
    /// `stats.remote_sticks_applied` / `_unresolved` at the end of the run.
    applied: u64,
    unresolved: u64,
    /// Sticks that a later plain buffer took back off: every buffer unsticks before it is
    /// unpacked.
    unstuck_by_a_later_buffer: usize,
    objects_built: usize,
}

/// Replay `session` through the whole application path, running `WorldScene::sync_objects` on
/// every frame at which some object has a movement buffer waiting.
///
/// `dereth_client_net`'s transport -> `dereth_client_net::client_session`'s dispatchers ->
/// `ObjectStream::pump` (which applies the movement update's three gates) -> `sync_objects`, which
/// is where `apply_movement` and `apply_player_movement` live. Nothing is synthesised anywhere in
/// this function.
fn drive(session: &str) -> Run {
    let store = store();
    let mut gpu = warp();
    let records = shared_session(session);
    assert!(!records.is_empty(), "{session} is empty");
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<WorldScene> = None;
    let mut out = Run {
        sticks: Vec::new(),
        applied: 0,
        unresolved: 0,
        unstuck_by_a_later_buffer: 0,
        objects_built: 0,
    };
    // Which movers were holding a stick before this frame's buffers were applied.
    let mut held: BTreeSet<ObjectId> = BTreeSet::new();

    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in stream.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        let Some(player) = stream.player() else {
            continue;
        };
        if scene.is_none() {
            let Some(pos) = stream.presence(player).and_then(|p| p.position) else {
                continue;
            };
            let block = pos.cell.landblock();
            let cfg = SceneConfig {
                landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
                character: true,
                land_radius: 1,
                scenery_radius: 0,
                ..SceneConfig::default()
            };
            let mut s = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
            let region = dereth_client::world::load_region(&store).expect("the region decodes");
            s.attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        // Sampled before `sync_objects` consumes them.
        let waiting: Vec<(ObjectId, Option<ObjectId>)> = stream
            .presences()
            .filter_map(|(id, p)| {
                let b = p.pending_movement.as_ref()?;
                Some((id, b.body.sticky_object))
            })
            .collect();
        // **`sync_objects` runs on every iteration, and skipping it when nothing is waiting is a
        // real bug rather than an optimisation.** `Presence::pending_movement` is a *single slot*:
        // a second `0xF74C` for the same object before the scene has consumed the first
        // overwrites it, so a drive that only syncs when a buffer is parked loses every one a
        // later buffer happens to follow.
        //
        // **A failure is a stop, not a panic.** `WorldScene::object_meshes` is keyed by appearance
        // and never evicts, so a long replay walks the render device's 4,096-entry SRV heap to
        // exhaustion; `long-solo-play` reaches it. The drive says where it stopped rather than
        // pretending the shortfall is in `apply_movement`. The recorded sticks are sent between
        // t = 93.9 s and t = 823.6 s, so the stop is past the last of them -- asserted below by
        // the count itself.
        if let Err(e) = s.sync_objects(&store, &mut gpu, &mut stream) {
            eprintln!(
                "remote stick {session}: the scene stopped at t = {:.1} s: {e}",
                r.t
            );
            break;
        }
        if waiting.is_empty() {
            continue;
        }
        for (id, named) in waiting {
            let resident = s.server_object_position(id).is_some()
                || s.server_object_sticky(id).is_some()
                || (id != player && s.server_object_target(id).is_some());
            match named {
                Some(named) => {
                    let recorded = if id == player {
                        s.character
                            .as_ref()
                            .and_then(dereth_client::character::Character::sticky_target)
                    } else {
                        s.server_object_sticky(id)
                    };
                    out.sticks.push(StickStation {
                        mover: id,
                        named,
                        recorded,
                        resident: resident || id == player,
                        resolved: recorded.is_some(),
                    });
                    held.insert(id);
                }
                None => {
                    // Every buffer unsticks before it is unpacked: any buffer at all takes the
                    // stick off.
                    if held.remove(&id) && s.server_object_sticky(id).is_none() {
                        out.unstuck_by_a_later_buffer += 1;
                    }
                }
            }
        }
    }
    let s = scene.as_mut().expect("the drive built a scene");
    out.applied = s.draw.stats.remote_sticks_applied;
    out.unresolved = s.draw.stats.remote_sticks_unresolved;
    out.objects_built = s.draw.stats.server_objects;
    out
}

/// **Every recorded stick is consumed, none is dropped.**
///
/// `early-inventory-and-casting` and `long-solo-play` are driven end to end and every stick buffer
/// is followed to the object's own `PositionManager`.
#[test]
fn every_recorded_remote_stick_reaches_the_position_manager() {
    // The dats and the device are required: both fail when absent.
    let _ = store();
    let _ = warp();
    let mut total_remote = 0usize;
    let mut total_player = 0usize;
    let mut resolved_remote = 0usize;
    let mut resolved_player = 0usize;
    let mut applied = 0u64;
    let mut unresolved = 0u64;
    let mut unstuck = 0usize;
    for session in ["early-inventory-and-casting", "long-solo-play"] {
        let run = drive(session);
        eprintln!(
            "remote stick {session}: {} stick buffers seen, applied={} unresolved={} unstuck_later={}",
            run.sticks.len(),
            run.applied,
            run.unresolved,
            run.unstuck_by_a_later_buffer
        );
        for st in &run.sticks {
            assert!(
                st.resident,
                "{:?} carried a stick and is not in the scene at all",
                st.mover
            );
            // The whole point: the id the server names must resolve, and the manager must hold it.
            if st.recorded.is_some() {
                assert_eq!(
                    st.recorded,
                    Some(st.named),
                    "{:?} was stuck to something other than the id the server named",
                    st.mover
                );
            }
        }
        let (r, p): (Vec<_>, Vec<_>) = run
            .sticks
            .iter()
            .partition(|s| s.mover.0 & 0x8000_0000 != 0);
        total_remote += r.len();
        total_player += p.len();
        resolved_remote += r.iter().filter(|s| s.resolved).count();
        resolved_player += p.iter().filter(|s| s.resolved).count();
        applied += run.applied;
        unresolved += run.unresolved;
        unstuck += run.unstuck_by_a_later_buffer;
    }
    eprintln!(
        "remote stick: remote {resolved_remote}/{total_remote} resolved, \
         player {resolved_player}/{total_player}, applied={applied} unresolved={unresolved}, \
         taken off by a later buffer {unstuck}"
    );
    // **Every stick resolved**, over a denominator read off the two recordings.
    assert!(
        total_remote > 0 && total_player > 0,
        "the two recordings carry sticks on both arms ({total_remote} remote, {total_player} player)"
    );
    assert_eq!(
        resolved_remote, total_remote,
        "every remote stick resolves its target and is recorded"
    );
    assert_eq!(
        resolved_player, total_player,
        "every player stick resolves its target and is recorded"
    );
    assert_eq!(
        applied, total_remote as u64,
        "stats.remote_sticks_applied counts every remote stick once"
    );
    assert_eq!(
        unresolved, 0,
        "a remote stick named an id this client could not resolve"
    );
    // **Every buffer unsticks before it is unpacked, asserted.** Without it a stick would
    // survive every later buffer and expire only on the 1 s timeout.
    assert!(
        unstuck > 0,
        "no plain later buffer took a stick off -- every buffer unsticks before it is unpacked, \
         which nothing else in this build would do"
    );
}

/// Behaviour: movement.stick.a-remote-creature-stuck-to-an-object-is-pulled-toward-it
/// **A stuck creature follows what it is stuck to.**
///
/// This drives the frame loop rather than only `sync_objects`, so
/// `WorldScene::advance_objects` -> `pull_stuck_objects` -> sticky offset adjustment actually runs,
/// and it measures the creature's own position moving toward its target between server updates.
///
/// **The control is the same run.** Every object in the scene that is *not* stuck must sit exactly
/// where the server put it — `server_object_position == server_object_reported_position`, bit for
/// bit — so a pull that leaked into the general case would redden this even though the stuck
/// creature moved correctly. This asserts the premise as well as the difference.
#[test]
fn a_stuck_creature_is_pulled_toward_what_it_is_stuck_to() {
    // The dats and the device are required: both fail when absent.
    let store = store();
    let mut gpu = warp();
    let session = "long-solo-play";
    let records = shared_session(session);
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<WorldScene> = None;

    // Observations, taken every frame once a stick is live.
    let mut samples: Vec<(ObjectId, f32, f32)> = Vec::new(); // (mover, distance, drift)
    let mut ever_initialized: BTreeSet<ObjectId> = BTreeSet::new();
    let mut unstuck_objects_drifted = 0usize;
    let mut unstuck_objects_checked = 0usize;
    let mut ever_stuck: BTreeSet<ObjectId> = BTreeSet::new();
    let mut drifters: BTreeSet<(ObjectId, bool)> = BTreeSet::new();
    let mut frames = 0usize;

    let mut clock = records.first().map_or(0.0, |r| r.t);
    // **A window, and the reason is a real limit rather than a convenience.** `long-solo-play`
    // creates enough objects over its length to exhaust the render device's descriptor heap,
    // which fails `sync_objects` outright. The window below covers the capture's first two stuck
    // creatures -- `0x800009D2` at t = 93.9 s and `0x800009D9` from t = 121.3 s. The *count* test
    // above drives the whole capture; this one only needs frames.
    let end = 165.0f64;
    let mut next = 0usize;
    // The first recorded stick in `long-solo-play` is at t = 93.9 s and the last at t = 823.6 s; a
    // 30 Hz loop over the whole capture is 25,000 frames of landscape work, so the loop below runs
    // at the capture's own record cadence and inserts 30 Hz sub-frames only inside a stick.
    while clock <= end {
        let mut fed = false;
        while next < records.len() && records[next].t <= clock {
            let r = &records[next];
            if !r.c2s {
                net.feed(&r.raw, r.peer(), LocalTime(clock));
            }
            next += 1;
            fed = true;
        }
        let now = LocalTime(clock);
        net.tick(now);
        let _ = net.take_outgoing();
        if fed {
            for e in stream.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
        }
        let Some(player) = stream.player() else {
            clock += 1.0 / 30.0;
            continue;
        };
        if scene.is_none() {
            let Some(pos) = stream.presence(player).and_then(|p| p.position) else {
                clock += 1.0 / 30.0;
                continue;
            };
            let block = pos.cell.landblock();
            let cfg = SceneConfig {
                landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
                character: true,
                land_radius: 1,
                scenery_radius: 0,
                ..SceneConfig::default()
            };
            let mut s = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
            let region = dereth_client::world::load_region(&store).expect("the region decodes");
            s.attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        s.sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        s.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            now,
            1.0 / 30.0,
        );
        frames += 1;

        let stuck: Vec<ObjectId> = stream
            .presences()
            .map(|(id, _)| id)
            .filter(|id| s.server_object_sticky(*id).is_some())
            .collect();
        let body = s
            .character
            .as_ref()
            .map(dereth_client::character::Character::position);
        for id in &stuck {
            ever_stuck.insert(*id);
            if s.server_object_sticky_initialized(*id) {
                ever_initialized.insert(*id);
            }
            let (Some(here), Some(reported), Some(target)) = (
                s.server_object_position(*id),
                s.server_object_reported_position(*id),
                body,
            ) else {
                continue;
            };
            samples.push((*id, planar(&here, &target), planar(&here, &reported)));
        }
        // The control: nothing that is not stuck has moved off the server's word.
        for (id, _) in stream.presences() {
            if s.server_object_sticky(id).is_some() {
                continue;
            }
            let (Some(here), Some(reported)) = (
                s.server_object_position(id),
                s.server_object_reported_position(id),
            ) else {
                continue;
            };
            unstuck_objects_checked += 1;
            if here.frame.origin != reported.frame.origin || here.cell != reported.cell {
                unstuck_objects_drifted += 1;
                drifters.insert((id, ever_stuck.contains(&id)));
            }
        }
        // Inside a live stick, step at 30 Hz; outside one, jump to the next record so the whole
        // 830-second capture stays affordable.
        clock = if stuck.is_empty() && next < records.len() {
            records[next].t.max(clock + 1.0 / 30.0)
        } else {
            clock + 1.0 / 30.0
        };
    }

    // Per mover: the distance to what it is stuck to on the first frame of its stick, the closest
    // it ever gets, and how far it was pulled off the server's own last word. The pull is only
    // evidence if it is **toward** the target, which is what the first two numbers say.
    let movers: BTreeSet<ObjectId> = samples.iter().map(|s| s.0).collect();
    for m in &movers {
        let mine: Vec<&(ObjectId, f32, f32)> = samples.iter().filter(|s| s.0 == *m).collect();
        let first = mine.first().expect("non-empty by construction").1;
        let closest = mine.iter().fold(f32::INFINITY, |a, s| a.min(s.1));
        let drift = mine.iter().fold(0.0f32, |a, s| a.max(s.2));
        eprintln!(
            "remote stick:   {m:?} {} samples, first {first:.3} m -> closest {closest:.3} m, \
             max drift off the server's word {drift:.3} m",
            mine.len()
        );
        assert!(
            closest < first - 1.0,
            "{m:?} was stuck to the body and got no closer than {closest:.3} m from {first:.3} m: \
             the pull is not pointing at the target"
        );
        assert!(
            drift > 1.0,
            "{m:?} was pulled only {drift:.3} m off the server's own position"
        );
    }
    // **The discriminating assertion.**
    //
    // Both numbers above are satisfied by a pull that happens once and is stamped back over on
    // the next frame: if `sync_objects` wrote the server's last position unconditionally every
    // frame, a frame whose quantum is a second or more still takes one large step toward the
    // target, and both thresholds pass.
    //
    // What only an **accumulating** pull can do is arrive. Sticky offset adjustment closes
    // `cylinder_distance - 0.3` per frame, capped at `max_speed x 5 x quantum`, so a creature
    // stuck to something must end up **at the stand-off**, not merely nearer than it started.
    let overall = samples.iter().fold(f32::INFINITY, |a, s| a.min(s.1));
    assert!(
        overall < 2.0,
        "the closest any stuck creature came to what it is stuck to was {overall:.3} m. The pull \
         is firing but not accumulating -- look at who else writes `SceneObject::position` \
         between two frames."
    );
    let pulled = samples.iter().filter(|s| s.2 > 0.001).count();
    let closest = samples.iter().fold(f32::INFINITY, |a, s| a.min(s.1));
    eprintln!(
        "remote stick: {frames} frames, {} stuck-frame samples over {} movers, {pulled} of them pulled \
         off the server's own position; closest approach {closest:.3} m; \
         {unstuck_objects_drifted} of {unstuck_objects_checked} unstuck object samples drifted",
        samples.len(),
        movers.len()
    );
    let s = scene.as_ref().expect("driven");
    eprintln!(
        "remote stick: stats applied={} unresolved={} pulled={}",
        s.draw.stats.remote_sticks_applied,
        s.draw.stats.remote_sticks_unresolved,
        s.draw.stats.remote_sticks_pulled
    );

    // The premise: sticks happened, and they were fed a target.
    assert!(
        !samples.is_empty(),
        "no frame in long-solo-play held a live stick -- the drive is broken"
    );
    assert!(
        !ever_initialized.is_empty(),
        "a stick was recorded and never received target information, so the offset adjustment's \
         initialized gate kept it inert -- the target manager is the thing to check"
    );
    // The acceptance: the creature is moved by the client, off the position the server last gave
    // it, toward what it is stuck to.
    assert!(
        pulled > 0,
        "no stuck creature was ever pulled off the server's own position"
    );
    assert!(
        s.draw.stats.remote_sticks_pulled > 0,
        "stats.remote_sticks_pulled"
    );
    // The control: the pull reached the stuck objects and nothing else.
    assert!(
        unstuck_objects_checked > 100,
        "the control sampled almost nothing: not a control"
    );
    eprintln!("remote stick: drifters (id, was ever stuck) = {drifters:?}");
    let never_stuck_drifters = drifters.iter().filter(|(_, ever)| !ever).count();
    assert_eq!(
        never_stuck_drifters, 0,
        "an object that was NEVER stuck moved off the position the server gave it, so the pull \
         leaked out of the sticky arm: {drifters:?}"
    );
}

fn planar(a: &Position, b: &Position) -> f32 {
    let (ax, ay) = block_xy(a);
    let (bx, by) = block_xy(b);
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// Landblock-absolute x/y, so two positions in different blocks compare.
fn block_xy(p: &Position) -> (f32, f32) {
    let lb = p.cell.landblock();
    (
        f32::from(lb.x()) * 192.0 + p.frame.origin.x,
        f32::from(lb.y()) * 192.0 + p.frame.origin.y,
    )
}

/// `MotionCommand::from_index` returns `None` past the end of the table, where retail reads
/// whatever lies beyond it; the difference matters only if the server sends such an index. This
/// is the count that shows it never does.
///
/// Retail does not check the index at all: the command table is read unchecked for the header's
/// style word and the five words of interpreted-state unpacking (`current_style`,
/// `forward_command`, `sidestep_command`, `turn_command` and each queued action's), so an index
/// of 412 or more reads past the table. This build drops the word instead, a declared deviation;
/// the measurement below makes it a stated one rather than an assumption: every index the
/// server-to-client `0xF74C` stream carries resolves, and the largest is 339. The per-field split
/// is pinned too, because one total cannot say which field moved.
#[test]
fn the_command_index_table_is_dense_over_everything_the_corpus_sends() {
    let (all, _) = corpus_buffers();
    let (mut resolutions, mut out_of_range, mut largest) = (0usize, 0usize, 0u16);
    let mut by_field: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut take = |field: &'static str, i: u16, r: &mut usize, o: &mut usize, m: &mut u16| {
        *r += 1;
        *by_field.entry(field).or_default() += 1;
        *m = (*m).max(i);
        if MotionCommand::from_index(i).is_none() {
            *o += 1;
        }
    };
    for b in &all {
        take(
            "header_style",
            b.body.current_style,
            &mut resolutions,
            &mut out_of_range,
            &mut largest,
        );
        let Some(st) = b.body.interpreted.as_ref() else {
            continue;
        };
        for (name, v) in [
            ("state_current_style", st.current_style),
            ("state_forward_command", st.forward_command),
            ("state_sidestep_command", st.sidestep_command),
            ("state_turn_command", st.turn_command),
        ] {
            if let Some(i) = v {
                take(name, i, &mut resolutions, &mut out_of_range, &mut largest);
            }
        }
        for a in &st.actions {
            take(
                "action",
                a.command_index,
                &mut resolutions,
                &mut out_of_range,
                &mut largest,
            );
        }
    }
    eprintln!(
        "command index: {resolutions} command-index resolutions, largest {largest}, by field {by_field:?}"
    );
    // `out_of_range` being zero is what this test claims: the command table (the final retail
    // client's 412 entries) is dense over everything the corpus sends. Every 0xF74C contributes its
    // header style, so the header field's count is the corpus's own movement total.
    assert_eq!(
        by_field.get("header_style").copied(),
        Some(recorded_movement_events()),
        "every recorded 0xF74C's header style was resolved"
    );
    assert_eq!(
        out_of_range, 0,
        "an index the 412-entry table does not hold -- retail reads past the table here"
    );
    // The literal pins, so the table's own length cannot drift silently under the count above.
    assert!(
        MotionCommand::from_index(411).is_some(),
        "the last row of the command table"
    );
    assert!(MotionCommand::from_index(412).is_none(), "one past the end");
    assert_eq!(
        MotionCommand::from_index(411),
        Some(MotionCommand(0x1000_019B))
    );
}
