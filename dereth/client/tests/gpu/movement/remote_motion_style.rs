//! A remote creature's stance and pose follow the style word in its
//! `Movement_SetObjectMovement 0xF74C` header on every arm, `MoveTo`/`TurnTo` included, and a
//! style word that repeats the stance in force changes nothing. The recordings never move a
//! creature between NonCombat and HandCombat on a `MoveTo`/`TurnTo` arm, so the
//! discriminating station re-delivers one of `long-solo-play`'s own recorded bodies, byte for
//! byte, at a moment when the creature stands in another stance; the oracle for the resulting pose
//! is the cycle the same creature stands in when the recording puts it in HandCombat. Fixture:
//! every retained raw capture, decoded here, and `long-solo-play` replayed through `ObjectStream`
//! and `WorldScene::sync_objects` into a real scene; poses come from the creatures' own motion
//! tables in `client_portal.dat`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::recorded_movement_events;
use dereth_scene::world_scene::SceneWrites;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::movement::{movement_type, MovementBody, MovementSetObjectMovement};
use dereth_protocol::{Message, Opcode};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

// ---------------------------------------------------------------------------------------------
// 1. The retained capture corpus. A missing fixture directory is a broken checkout.
// ---------------------------------------------------------------------------------------------

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// 2. The wire census — the denominators every claim below is a fraction of.
// ---------------------------------------------------------------------------------------------

struct Wire {
    session: String,
    mover: ObjectId,
    autonomous: bool,
    body: MovementBody,
    /// The recorded blob, byte for byte, so a test can re-deliver this exact body.
    blob: Vec<u8>,
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
                blob: m.body.clone(),
            });
        }
    }
    assert!(!all.is_empty(), "the corpus carries no 0xF74C at all");
    (all, chars)
}

/// **What the corpus itself shows of a remote stance change on a `MoveTo`/`TurnTo` arm**, measured
/// rather than assumed.
///
/// ACE sends a stance in an interpreted buffer, and a `MoveTo`/`TurnTo` it sends afterwards
/// repeats the style already in force. The stance changes that do ride on an arm are asserted
/// below one by one: every one is a `TURN_TO_OBJECT` addressed to another player and leaves a
/// **style-zero header** for `NonCombat`. None moves a creature between NonCombat and
/// HandCombat, which is what the stance test needs, so that station is constructed.
///
/// It also counts what the constructed station is **built from**: recorded remote
/// `MoveTo`/`TurnTo` bodies whose header names style 60 (`HandCombat`), while a creature is
/// created standing in 61 (`NonCombat`). The body that carries the discriminating word exists in
/// the capture; what the corpus never does is deliver one to an object not already in that
/// stance.
///
/// It measures the rest of the interpreted arm too: the header word against the interpreted
/// state's own style, and the stick-to-object and standing-long-jump flags.
#[test]
fn every_stance_change_on_a_remote_arm_is_a_turn_to_object_out_of_a_style_zero_header() {
    let (all, chars) = corpus_buffers();
    // `NonCombat` (index 61) is where both motion-state constructors start.
    let mut in_force: BTreeMap<(String, ObjectId), u16> = BTreeMap::new();
    let (mut edges, mut on_a_move_to_arm) = (0usize, 0usize);
    let (mut remote, mut arms, mut off_stance_arms) = (0usize, 0usize, 0usize);
    // Interpreted-arm buffers whose header style word differs from the style their own
    // `InterpretedMotionState` carries. See the assertion below for what this number is for.
    let (mut case0, mut header_differs, mut no_state) = (0usize, 0usize, 0usize);
    // The rest of the interpreted arm's body: the stick-to-object and standing-long-jump flags.
    let (mut sticky, mut longjump) = (0usize, 0usize);
    let mut natural: Vec<(String, ObjectId, u8, u16, u16)> = Vec::new();
    let mut pairs: BTreeMap<(u16, Option<u16>), usize> = BTreeMap::new();
    let mut arm_styles: BTreeMap<(String, u16), usize> = BTreeMap::new();
    let mut differ_by_session: BTreeMap<String, usize> = BTreeMap::new();
    let mut longjump_by_session: BTreeMap<String, usize> = BTreeMap::new();
    let mut sticky_by_session: BTreeMap<String, usize> = BTreeMap::new();
    let mut remote_by_session: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_type = [0usize; 10];
    let mut movers: BTreeSet<(String, ObjectId)> = BTreeSet::new();
    let mut styles: BTreeMap<u16, usize> = BTreeMap::new();
    for b in &all {
        // "Remote" is exactly "not the session's own character" — the same partition
        // `movement/local_motion_style.rs` takes the other side of.
        if chars[&b.session].contains(&b.mover) {
            continue;
        }
        remote += 1;
        *remote_by_session.entry(b.session.clone()).or_default() += 1;
        by_type[b.body.movement_type as usize] += 1;
        movers.insert((b.session.clone(), b.mover));
        *styles.entry(b.body.current_style).or_default() += 1;
        if b.body.movement_type != movement_type::INVALID {
            arms += 1;
            *arm_styles
                .entry((b.session.clone(), b.body.current_style))
                .or_default() += 1;
            if b.body.current_style != 61 {
                off_stance_arms += 1;
            }
        }
        if b.body.sticky_object.is_some() {
            sticky += 1;
            *sticky_by_session.entry(b.session.clone()).or_default() += 1;
        }
        if b.body.motion_flags & dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP != 0 {
            longjump += 1;
            *longjump_by_session.entry(b.session.clone()).or_default() += 1;
        }
        if b.body.movement_type == movement_type::INVALID {
            case0 += 1;
            let state_ix = b.body.interpreted.as_ref().and_then(|s| s.current_style);
            *pairs.entry((b.body.current_style, state_ix)).or_default() += 1;
            match state_ix {
                Some(s) if s != b.body.current_style => {
                    header_differs += 1;
                    *differ_by_session.entry(b.session.clone()).or_default() += 1;
                }
                Some(_) => {}
                None => {
                    no_state += 1;
                    *differ_by_session.entry(b.session.clone()).or_default() += 1;
                }
            }
        }
        let cur = in_force.entry((b.session.clone(), b.mover)).or_insert(61);
        if *cur != b.body.current_style {
            edges += 1;
            if b.body.movement_type != movement_type::INVALID {
                on_a_move_to_arm += 1;
                natural.push((
                    b.session.clone(),
                    b.mover,
                    b.body.movement_type,
                    *cur,
                    b.body.current_style,
                ));
            }
        }
        // After the buffer: the interpreted arm copies the state's own style, and every other
        // arm leaves the header's.
        *cur = b
            .body
            .interpreted
            .as_ref()
            .and_then(|s| s.current_style)
            .unwrap_or(b.body.current_style);
    }
    eprintln!(
        "remote style: {remote} remote 0xF74C from {} objects; by arm {by_type:?}; style words {styles:?}",
        movers.len()
    );
    eprintln!("remote style natural={natural:?}");
    eprintln!("remote style arm_styles={arm_styles:?}");
    eprintln!("remote style (header,state) pairs on the interpreted arm = {pairs:?}");
    eprintln!("remote style differ={differ_by_session:?}");
    eprintln!("remote style longjump={longjump_by_session:?} sticky={sticky_by_session:?}");
    eprintln!("remote style remote_by_session={remote_by_session:?}");
    eprintln!(
        "remote style all={} remote={remote} arms={arms} edges={edges} on_arm={on_a_move_to_arm} \
         off_stance_arms={off_stance_arms} case0={case0} header_differs={header_differs} \
         sticky={sticky} longjump={longjump}",
        all.len()
    );
    // The expected side is the corpus's own `0xF74C` netblob count, while `all` above is this
    // file's own reassembly and decode of the retained raw datagrams.
    assert_eq!(
        all.len(),
        recorded_movement_events(),
        "every recorded 0xF74C body was decoded"
    );
    // No remote buffer carries an arm other than the interpreted one and the four move-to/turn-to
    // arms.
    assert_eq!(
        by_type[1..6].iter().sum::<usize>(),
        0,
        "a remote 0xF74C with an unknown arm: {by_type:?}"
    );
    assert!(remote > 0 && arms > 0, "the corpus carries no remote arm");

    // Every stance change that rides on an arm is a `TURN_TO_OBJECT` (arm 8) toward another
    // player, and each leaves the **style-zero header** for `NonCombat`. They do not replace the
    // constructed station: a change out of style 0 is not the NonCombat/HandCombat change the
    // stance test needs, and no recorded arm carries that one.
    assert!(
        on_a_move_to_arm > 0 && on_a_move_to_arm <= edges,
        "stance changes riding on a remote MoveTo/TurnTo arm: {on_a_move_to_arm} of {edges}"
    );
    assert!(
        natural.iter().all(
            |(_, _, arm, from, to)| *arm == movement_type::TURN_TO_OBJECT
                && *from == 0
                && *to == 61
        ),
        "a stance change on a remote arm that is not TurnToObject from style 0 to NonCombat: \
         {natural:?}"
    );

    // The material the constructed station is built out of: a recorded remote arm whose header
    // names HandCombat (60) while the creature stands in NonCombat.
    assert!(
        off_stance_arms > 0,
        "no recorded remote arm names a style other than NonCombat"
    );
    assert!(
        arm_styles.keys().any(|(_, style)| *style == 60),
        "no recorded remote arm names HandCombat: {arm_styles:?}"
    );
    // The set of style indices the server sends remote objects is the finding; 62 is the table
    // entry between `NonCombat` (61) and `BowCombat` (63), and 64 the one past `BowCombat`, which
    // the player's own arm carries too (`movement::local_motion_style`).
    assert_eq!(
        styles.keys().copied().collect::<Vec<_>>(),
        vec![0u16, 60, 61, 62, 64, 73],
        "the style indices the server sent remote objects"
    );
    // Pinned as literals: a table read through the same accessor it is written through cannot
    // detect a wrong index.
    assert_eq!(
        MotionCommand::from_index(60),
        Some(MotionCommand(0x8000_003C))
    ); // HandCombat
    assert_eq!(
        MotionCommand::from_index(61),
        Some(MotionCommand(0x8000_003D))
    ); // NonCombat
    assert_eq!(
        MotionCommand::from_index(60),
        Some(MotionCommand::HAND_COMBAT)
    );
    assert_eq!(
        MotionCommand::from_index(61),
        Some(MotionCommand::NON_COMBAT)
    );
    // Table entry 0 is a style-flagged command like every other entry the server puts in this
    // word, so the header's style application acts on a style-zero header exactly as it acts on
    // a 60 or a 61 -- it is not a "no style" sentinel the client skips.
    assert_eq!(
        MotionCommand::from_index(0),
        Some(MotionCommand(0x8000_0000))
    );
    assert!(MotionCommand::from_index(0)
        .expect("index 0 is in the table")
        .is_style());
    assert_eq!(
        MotionCommand::from_index(73),
        Some(MotionCommand(0x8000_0049))
    ); // Magic
    assert_eq!(MotionCommand::from_index(73), Some(MotionCommand::MAGIC));

    // **Why the interpreted arm's own header word cannot be measured here, stated as a number.**
    //
    // On the interpreted arm the pre-switch style application is followed immediately by
    // interpreted-movement application, which copies the state's own `current_style` over
    // whatever the header's word left -- so the header word is observable on that arm only when
    // the two differ. Across every remote interpreted buffer in the corpus they never do, so a
    // build that skipped the header word on that arm is indistinguishable against this corpus;
    // this is the measurement that says so.
    eprintln!(
        "remote style: {case0} remote interpreted buffers, {header_differs} of which name a header style that \
         differs from their own interpreted state's"
    );
    assert_eq!(
        header_differs, 0,
        "the corpus DOES separate the two words on the interpreted arm after all -- an \
         interpreted buffer that carries a style has always carried the one its header names"
    );
    // Some remote interpreted buffers carry **no `current_style` in their interpreted state at
    // all**, and the header word is 0 in every one of them. That is an absence, not a
    // disagreement. Every pair is `(w, Some(w))` or `(0, None)`, which is the whole claim: a
    // header word and the interpreted state beside it never disagree in any recording.
    assert!(
        case0 > 0 && no_state > 0,
        "no remote interpreted buffer without a style word"
    );
    assert!(
        pairs
            .keys()
            .all(|&(header, state)| state == Some(header) || (header == 0 && state.is_none())),
        "a remote interpreted buffer whose header and state styles disagree: {pairs:?}"
    );
    eprintln!(
        "remote style: interpreted buffers without a matching state style {differ_by_session:?}"
    );

    // **The rest of the interpreted arm, measured.**
    //
    // Retail's interpreted movement also sticks to the named object when the stick-to-object
    // motion flag is set, and copies the standing-long-jump flag into motion state. Both are
    // counted here, per recording, so a change in either is a red rather than nothing happening.
    eprintln!(
        "remote style: remote buffers carrying StickToObject {sticky}, StandingLongJump {longjump}"
    );
    // `movement::remote_stick_to_object` asserts what the stick does, and `movement::remote_move_to`
    // that a standing long jump reaches the remote interpreter; here the corpus only has to carry
    // both.
    assert!(
        sticky > 0 && longjump > 0,
        "the corpus carries no remote StickToObject or StandingLongJump \
         ({sticky_by_session:?}, {longjump_by_session:?})"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The seam. A real `WorldScene` driven by the capture's own datagrams through
//    `WorldScene::sync_objects` — the application's own path and no other.
// ---------------------------------------------------------------------------------------------

/// The retail dats, or **fail**: a missing dat is a failure, never a silent pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The creature's pose: the animation ids its sequence holds, the node it is playing, the node it
/// wraps back to, and the frame it is on. Named by the crate so this file and the accessor cannot
/// drift apart.
use dereth_scene::world_scene::ObjectPose as Pose;

/// One `0xF74C` seen arriving at `sync_objects` for a **remote** object, with what that object's
/// own driver held on either side of it.
#[derive(Debug, Clone)]
struct Station {
    id: ObjectId,
    movement_type: u8,
    style_index: u16,
    /// The `InterpretedMotionState.current_style` the buffer itself carried, when it carried one.
    state_style: Option<u16>,
    style_before: MotionCommand,
    style_after: MotionCommand,
    pose_before: Pose,
    pose_after: Pose,
    /// True for the one station the corpus does not supply on its own.
    injected: bool,
}

/// Everything a re-delivery needs: the recorded blob and what it says about itself.
#[derive(Clone)]
struct Candidate {
    mover: ObjectId,
    movement_type: u8,
    style_index: u16,
    blob: Vec<u8>,
}

/// Replay `session` and run the real `WorldScene::sync_objects` on every frame at which some
/// **remote** object has a movement buffer waiting, stopping after `limit` of them.
///
/// This is the whole application path: `dereth_client_net`'s transport, `dereth_client_net::client_session`'s dispatchers,
/// `ObjectStream::pump` (which applies retail's three movement-update gates) and then
/// `WorldScene::sync_objects`, which is where `apply_movement` lives.
fn drive(session: &str, limit: usize) -> (Vec<Station>, usize) {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    // Every `MoveTo`/`TurnTo` body this session sent something that is *not* its own character,
    // in wire order, kept whole so one of them can be re-delivered below.
    let candidates: Vec<Candidate> = {
        let (all, chars) = corpus_buffers();
        all.iter()
            .filter(|b| {
                b.session == session
                    && !b.autonomous
                    && b.body.movement_type != movement_type::INVALID
                    && !chars[&b.session].contains(&b.mover)
            })
            .map(|b| Candidate {
                mover: b.mover,
                movement_type: b.body.movement_type,
                style_index: b.body.current_style,
                blob: b.blob.clone(),
            })
            .collect()
    };
    assert!(
        !candidates.is_empty(),
        "{session} sends no remote MoveTo/TurnTo body at all"
    );

    let records = shared_session(session);
    assert!(!records.is_empty(), "{session} is empty");
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<WorldScene> = None;
    let mut stations: Vec<Station> = Vec::new();
    let mut t = 0.0f64;

    for r in records {
        if stations.len() >= limit {
            break;
        }
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
        // Build the scene once the server has said where the player is — the same decision
        // `App::load_pending_scene` makes with a live session.
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
            let region =
                dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
            s.attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        // Which remote objects have a buffer parked *this* frame, sampled before `sync_objects`
        // consumes them.
        let waiting: Vec<(ObjectId, u8, u16, Option<u16>)> = stream
            .presences()
            .filter(|(id, _)| *id != player)
            .filter_map(|(id, p)| {
                let b = p.pending_movement.as_ref()?;
                Some((
                    id,
                    b.body.movement_type,
                    b.body.current_style,
                    b.body.interpreted.as_ref().and_then(|x| x.current_style),
                ))
            })
            .collect();
        let before: Vec<Option<(MotionCommand, Pose)>> = waiting
            .iter()
            .map(|(id, ..)| Some((s.server_object_style(*id)?, s.server_object_pose(*id)?)))
            .collect();
        s.sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        // **Sampled here, before any frame is advanced.** A one-shot action completes on the very
        // next completed-motion check and returns the interpreted state to `Ready`, so a sample
        // taken after the object is stepped reads the recovery rather than the buffer. This
        // samples the transient state while it exists.
        for (i, (id, movement_type, style_index, state_style)) in waiting.into_iter().enumerate() {
            // Only objects the scene actually built have a driver to observe: `apply_movement`
            // runs inside `if let Some(o) = self.objects.get_mut(&id)`.
            let Some((style_before, pose_before)) = before[i].clone() else {
                continue;
            };
            let Some(style_after) = s.server_object_style(id) else {
                continue;
            };
            let Some(pose_after) = s.server_object_pose(id) else {
                continue;
            };
            stations.push(Station {
                id,
                movement_type,
                style_index,
                state_style,
                style_before,
                style_after,
                pose_before,
                pose_after,
                injected: false,
            });
        }
        t += 1.0 / 30.0;
    }

    // ------------------------------------------------------------------------------------
    // The discriminating station the corpus does not contain.
    //
    // Measured by the wire census above: no recorded arm moves a remote creature between
    // NonCombat and HandCombat, so a suite built only from the recording cannot tell a build that
    // applies the header's style word from one that ignores it on those arms. The case is
    // therefore constructed where the two should disagree, using a **recorded body**: one of this session's own
    // `MoveTo`/`TurnTo` blobs for a creature, delivered byte for byte through `ObjectStream`'s own
    // `0xF74C` path, at a moment when that creature's stance differs from the one its header
    // names. Nothing is synthesised; only the delivery moment is ours.
    let s = scene.as_mut().expect("the drive built a scene");
    // The **last** candidate that qualifies: last, so its movement timestamp is the newest that
    // object has been sent and the movement update's first gate (the timestamp must be newer than
    // the last one applied) lets it through.
    let chosen = candidates.iter().rev().find(|c| {
        // The creature must be resident (so `apply_movement` can reach it), must have a pose to
        // move (an object with no motion table has nothing to observe), and its stance must
        // differ from the word the recorded header carries.
        s.server_object_style(c.mover)
            .is_some_and(|cur| MotionCommand::from_index(c.style_index).is_some_and(|w| w != cur))
            && s.server_object_pose(c.mover)
                .is_some_and(|p| !p.0.is_empty())
    });
    let injected = match chosen.cloned() {
        None => 0,
        Some(c) => {
            let style_before = s.server_object_style(c.mover).expect("checked above");
            let pose_before = s.server_object_pose(c.mover).expect("checked above");
            assert_ne!(
                MotionCommand::from_index(c.style_index),
                Some(style_before),
                "the re-delivered body must name a stance the creature is not already in"
            );
            stream.apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                    body: c.blob.clone(),
                },
                LocalTime(t),
            );
            assert!(
                stream
                    .presence(c.mover)
                    .is_some_and(|p| p.pending_movement.is_some()),
                "the re-delivered body did not survive the movement update's timestamp gates"
            );
            s.sync_objects(&store, &mut gpu, &mut stream)
                .expect("objects sync");
            stations.push(Station {
                id: c.mover,
                movement_type: c.movement_type,
                style_index: c.style_index,
                state_style: None,
                style_before,
                style_after: s
                    .server_object_style(c.mover)
                    .expect("the creature is still there"),
                pose_before,
                pose_after: s
                    .server_object_pose(c.mover)
                    .expect("the creature is still there"),
                injected: true,
            });
            1
        }
    };
    if let Some(mut s) = scene {
        s.release_textures(&mut gpu);
    }
    (stations, injected)
}

/// `long-solo-play` is the capture whose character fights: it is the only one that sends a remote
/// creature a `MoveTo` body in `HandCombat`, which is the word the constructed station needs.
fn long_solo_play_stations() -> (Vec<Station>, usize) {
    drive("long-solo-play", 400)
}

/// The node the sequence wraps back to — the creature's **standing** animation, which is what a
/// stance is on screen. `None` when the sequence holds no animation at all.
fn cyclic_anim(p: &Pose) -> Option<DataId> {
    p.0.get(p.2?).copied()
}

// ---------------------------------------------------------------------------------------------
// 4. The premise: on this corpus the arm is exercised and its style word is inert
// ---------------------------------------------------------------------------------------------

/// **The recorded remote `MoveTo`/`TurnTo` buffers that reach `sync_objects` in the driven window
/// all repeat the stance already in force, and their style word moves nothing.**
///
/// The seam-level counterpart of the wire census above, with the pose read on both sides. It is
/// also retail's same-style check asserted on the shipped path: a style word repeating the stance
/// in force must issue **no** style motion, and a build that re-issued one every time would leave
/// the state word alone but restart the stance cycle. The pose is therefore compared whole, not
/// just the stance word.
///
/// Its premise is asserted first, because a drive that reached **no** arm at all would satisfy
/// every `for` loop below vacuously.
#[test]
fn every_recorded_remote_arm_repeats_the_stance_in_force_and_is_inert() {
    // The dats and the device are required: `store()` and `crate::common::test_gpu(320, 240)` fail when they are absent.
    let _ = store();
    let _ = crate::common::test_gpu(320, 240);
    let (stations, injected) = long_solo_play_stations();
    let arms: Vec<&Station> = stations
        .iter()
        .filter(|s| !s.injected && s.movement_type != movement_type::INVALID)
        .collect();
    let movers: BTreeSet<ObjectId> = stations.iter().map(|s| s.id).collect();
    eprintln!(
        "remote style: {} remote stations over {} objects; {} recorded arms; {injected} constructed",
        stations.len(),
        movers.len(),
        arms.len()
    );
    // The premise, in both halves: the drive is the size it was measured at, and the arms are
    // really there.
    assert!(
        !arms.is_empty(),
        "no recorded MoveTo/TurnTo buffer reached a remote driver in the {} driven stations",
        stations.len()
    );
    assert_eq!(injected, 1, "the one constructed station");
    assert!(
        movers.len() >= 20,
        "only {} remote objects were driven",
        movers.len()
    );

    // Retail skips style application when the style word repeats the stance in force, then
    // continues into the arm's own movement in the same call. So "inert" is a claim about the
    // style word, not the whole arm. The `TURN_TO_OBJECT` arms begin their turn in that call:
    // move-to performance cancels first, the cancel completes pending motions, and the turn
    // finds nothing pending and starts synchronously.
    //
    // What discriminates a build that re-issued the style motion: cyclic-animation replacement
    // answers it by restarting **the same** stance cycle at its first frame. So a pose that moved
    // must have moved onto a *different* cyclic animation, and a pose that did not move must be
    // identical, frame number included.
    let (mut inert, mut began) = (0usize, 0usize);
    for s in &arms {
        // A `MoveTo`/`TurnTo` buffer carries no `InterpretedMotionState` at all, which is why the
        // header's style word is the only thing in it that can change a stance.
        assert!(
            s.state_style.is_none(),
            "a type-{} buffer for {:?} carried an interpreted state",
            s.movement_type,
            s.id
        );
        let want = MotionCommand::from_index(s.style_index)
            .unwrap_or_else(|| panic!("style index {} is not in command_ids", s.style_index));
        assert_eq!(
            s.style_before, want,
            "{:?}: a recorded arm named {} while the creature stood in {:?}, so the corpus DOES \
             carry a natural instance and this file's premise is wrong",
            s.id, s.style_index, s.style_before
        );
        assert_eq!(
            s.style_after, want,
            "{:?}: the stance did not survive the arm",
            s.id
        );
        if s.pose_before == s.pose_after {
            inert += 1;
        } else {
            began += 1;
            assert_eq!(
                s.movement_type,
                movement_type::TURN_TO_OBJECT,
                "{:?}: only an arm with a node of its own may move the pose here",
                s.id
            );
            assert_ne!(
                cyclic_anim(&s.pose_after),
                cyclic_anim(&s.pose_before),
                "{:?}: the pose restarted the SAME stance cycle -- exactly what a re-issued style word looks like, and the same-style check is what stops every 0xF74C doing it ({:?} -> {:?})",
                s.id, s.pose_before, s.pose_after
            );
        }
    }
    // The census of what the arms do to the pose. Stated, so that a build in which
    // the turns stopped beginning -- or in which something else started moving poses --
    // is a red here rather than a quieter suite.
    // Most arms leave the pose alone; the TurnToObject arms that begin their turn in the same call
    // are the only ones that move it (asserted per arm above).
    eprintln!(
        "remote style: {inert} arms left the pose alone, {began} TurnToObject arms began a turn"
    );
    assert!(inert > 0, "no recorded arm left the pose alone");
    // ...and the arms really are more than one `case`, so "inert" is not a statement about one.
    let kinds: BTreeSet<u8> = arms.iter().map(|s| s.movement_type).collect();
    assert!(kinds.len() >= 2, "only one arm type was driven: {kinds:?}");
}

// ---------------------------------------------------------------------------------------------
// 5. The constructed station: the stance and the pose follow the style word
// ---------------------------------------------------------------------------------------------

/// Behaviour: movement.style.a-remote-creatures-stance-follows-the-style-word
/// **A creature's stance follows the style word on a `MoveTo`/`TurnTo` arm**, asserted from a
/// **recorded** body re-delivered byte for byte with only the delivery moment constructed.
///
/// The corpus carries **no** natural instance —
/// [`every_stance_change_on_a_remote_arm_is_a_turn_to_object_out_of_a_style_zero_header`] measures
/// that on the wire and [`every_recorded_remote_arm_repeats_the_stance_in_force_and_is_inert`] at
/// the driver — so the station is built: `drive` takes the **last** of `long-solo-play`'s own
/// recorded remote `MoveTo`/`TurnTo` blobs whose header names a stance the creature is not
/// standing in, and hands it to `ObjectStream::apply_event` unmodified. The bytes are the
/// server's; the moment is ours. Nothing is synthesised.
#[test]
fn the_style_word_on_a_remote_arm_changes_the_creatures_stance() {
    // The dats and the device are required: `store()` and `crate::common::test_gpu(320, 240)` fail when they are absent.
    let _ = store();
    let _ = crate::common::test_gpu(320, 240);
    let (stations, _) = long_solo_play_stations();
    let edge = stations
        .iter()
        .find(|s| s.injected)
        .expect("the constructed station is missing");
    eprintln!(
        "remote style constructed: {:?} type {} style {} : {:?} -> {:?}",
        edge.id, edge.movement_type, edge.style_index, edge.style_before, edge.style_after
    );
    // It really is an arm, with nothing else in it that could have moved the stance.
    assert_ne!(
        edge.movement_type,
        movement_type::INVALID,
        "the constructed station is an interpreted buffer"
    );
    assert!(
        edge.state_style.is_none(),
        "the re-delivered body carries an interpreted state"
    );
    // The premise: the stance in force differed from the word the recorded header carries.
    assert_ne!(
        edge.style_before,
        MotionCommand::from_index(edge.style_index).expect("in command_ids"),
        "the creature was already in the stance the body names, so this station proves nothing"
    );
    // The finding.
    assert_ne!(
        edge.style_before, edge.style_after,
        "the stance did not move at all"
    );
    assert_eq!(
        edge.style_after,
        MotionCommand::from_index(edge.style_index).expect("in command_ids"),
        "the stance did not follow the re-delivered body's own style word"
    );
    assert_eq!(edge.style_before, MotionCommand::NON_COMBAT);
    assert_eq!(edge.style_after, MotionCommand::HAND_COMBAT);
}

/// **The creature's *pose* follows the style word**, and the oracle for what the pose should
/// become is the **same creature's own recorded behaviour**.
///
/// The stance word alone cannot carry this claim. Style application writes the interpreted
/// state's current style **and** asks the motion table for the new stance's cycle, and a build
/// that wrote the word without reaching the table would pass
/// [`the_style_word_on_a_remote_arm_changes_the_creatures_stance`] and leave the creature standing
/// in the animation it had. The parts are posed from the node at the current frame, so these ids
/// are what is on screen.
///
/// **The expected value is not a literal read off this build.** The same creature appears in the
/// capture in both stances, put there by the server's own interpreted buffers, and the standing
/// animation it holds in each is recorded by the natural stations of the same drive. The
/// constructed station is required to land on the cycle the recording says that creature stands in
/// when the server puts it in `HandCombat` — so the oracle is the capture, and a build that
/// invented a different animation fails even though the stance word is right.
#[test]
fn the_creatures_pose_follows_the_style_word() {
    // The dats and the device are required: `store()` and `crate::common::test_gpu(320, 240)` fail when they are absent.
    let _ = store();
    let _ = crate::common::test_gpu(320, 240);
    let (stations, _) = long_solo_play_stations();
    let edge = stations
        .iter()
        .find(|s| s.injected)
        .expect("the constructed station is missing");

    // What the recording says this creature stands in, per stance, from the natural stations only.
    let mut recorded: BTreeMap<MotionCommand, BTreeSet<DataId>> = BTreeMap::new();
    for s in stations.iter().filter(|s| !s.injected && s.id == edge.id) {
        if let Some(a) = cyclic_anim(&s.pose_after) {
            recorded.entry(s.style_after).or_default().insert(a);
        }
    }
    let before = cyclic_anim(&edge.pose_before).expect("the creature was standing in something");
    let after = cyclic_anim(&edge.pose_after).expect("the creature is standing in something");
    eprintln!(
        "remote style pose: {:?} cycles {before:?} -> {after:?}; the capture's own stances for it are {recorded:?}",
        edge.id
    );

    // The premise: the recording really does show this creature in both stances, or there is no
    // oracle here and the assertions below are vacuous.
    assert!(
        recorded.contains_key(&MotionCommand::NON_COMBAT)
            && recorded.contains_key(&MotionCommand::HAND_COMBAT),
        "the capture does not stand {:?} in both stances; {recorded:?}",
        edge.id
    );
    assert_ne!(
        recorded[&MotionCommand::NON_COMBAT],
        recorded[&MotionCommand::HAND_COMBAT],
        "this creature stands in the same animation in both stances, so its pose cannot witness a \
         stance change at all -- pick another"
    );

    // The finding: the pose moved, and it moved to the cycle the capture says HandCombat means
    // for this creature.
    assert_ne!(
        edge.pose_before, edge.pose_after,
        "the stance word changed and the creature did not move a muscle"
    );
    assert!(
        recorded[&MotionCommand::NON_COMBAT].contains(&before),
        "the creature was not standing in the capture's own NonCombat cycle before the station: \
         {before:?} is not in {:?}",
        recorded[&MotionCommand::NON_COMBAT]
    );
    assert!(
        recorded[&MotionCommand::HAND_COMBAT].contains(&after),
        "the re-delivered body left the creature cycling {after:?}, which is not what the capture \
         shows it standing in when the server puts it in HandCombat: {:?}",
        recorded[&MotionCommand::HAND_COMBAT]
    );
    assert_ne!(before, after, "the standing animation did not change");
}

// ---------------------------------------------------------------------------------------------
// 6. The interpreted arm, unchanged
// ---------------------------------------------------------------------------------------------

/// **The interpreted arm reaches a remote object's driver** and carries the stance changes.
///
/// It is also why the corpus cannot see the arm case: every stance the server gives a remote
/// object arrives here, in an `InterpretedMotionState`, and interpreted-movement application
/// copies the state's own `current_style` over whatever the header's word left.
#[test]
fn the_interpreted_arm_still_carries_every_stance_the_corpus_holds() {
    // The dats and the device are required: `store()` and `crate::common::test_gpu(320, 240)` fail when they are absent.
    let _ = store();
    let _ = crate::common::test_gpu(320, 240);
    let (stations, _) = long_solo_play_stations();
    let interpreted: Vec<&Station> = stations
        .iter()
        .filter(|s| s.movement_type == movement_type::INVALID)
        .collect();
    assert!(
        interpreted.len() >= 300,
        "the capture must drive the remote interpreted arm; {} stations",
        interpreted.len()
    );
    let mut checked = 0usize;
    for s in &interpreted {
        let want_ix = s.state_style.unwrap_or(s.style_index);
        let Some(want) = MotionCommand::from_index(want_ix) else {
            continue;
        };
        assert_eq!(
            s.style_after, want,
            "{:?}: an interpreted buffer naming style {want_ix} left the creature on {:?}",
            s.id, s.style_after
        );
        checked += 1;
    }
    assert!(
        checked >= 300,
        "only {checked} interpreted buffers named a style"
    );
    // ...and it must actually *change* at least once, or the assertion above is satisfied by a
    // driver that was already there, so at least one state edge is required.
    let moved = interpreted
        .iter()
        .filter(|s| s.style_before != s.style_after)
        .count();
    assert!(
        moved > 0,
        "not one interpreted buffer moved a remote object's stance"
    );
    eprintln!(
        "remote style: {} interpreted stations, {moved} of which moved a stance",
        interpreted.len()
    );
}
