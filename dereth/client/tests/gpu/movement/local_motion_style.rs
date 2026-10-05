//! The local player's own `Movement_SetObjectMovement 0xF74C` reaches his body: the header's style
//! word is applied on every arm before the arm is unpacked (so a `MoveTo`/`TurnTo` buffer changes
//! the stance too), and the interpreted arm's forward, sidestep and turn commands reach his driver.
//! The two halves are asserted separately because each is the only route to what it carries.
//! Fixture: every retained raw capture, reassembled and decoded here, and `long-solo-play` (the
//! capture that carries all four stances) replayed through `ObjectStream` and
//! `WorldScene::sync_objects` into a real scene with a local body; style indices map through the
//! motion command table (60/61/63/73 = HandCombat / NonCombat / BowCombat / Magic).

#![cfg(gpu)]

use super::common::recorded_movement_events;
use dereth_scene::world_scene::SceneWrites;

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
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

/// The movement bodies the corpus carries, measured here rather than inherited: the total, where
/// each came from, how many take each arm, how many are addressed to the recording's own
/// character, and which style indices the server sent him. This is an independent second reading
/// of the figures the client-net movement census asserts at the transport seam; the two agree.
#[test]
fn every_style_word_the_server_sends_the_player_is_a_known_stance() {
    let (all, chars) = corpus_buffers();
    let mut by_type = [0usize; 10];
    let mut player_by_type = [0usize; 10];
    let mut player_autonomous = 0usize;
    let mut player_styles: BTreeMap<u16, usize> = BTreeMap::new();
    for b in &all {
        by_type[b.body.movement_type as usize] += 1;
        if chars[&b.session].contains(&b.mover) {
            player_by_type[b.body.movement_type as usize] += 1;
            *player_styles.entry(b.body.current_style).or_default() += 1;
            if b.autonomous {
                player_autonomous += 1;
            }
        }
    }
    let mut per_session: BTreeMap<&str, usize> = BTreeMap::new();
    for b in &all {
        *per_session.entry(b.session.as_str()).or_default() += 1;
    }
    eprintln!(
        "player movement census: total={} per_session={per_session:?}",
        all.len()
    );
    eprintln!("player movement by_type={by_type:?} player_by_type={player_by_type:?}");
    eprintln!("player movement player_autonomous={player_autonomous} styles={player_styles:?}");
    eprintln!("player movement chars={chars:?}");
    // The expected side is the corpus's own `0xF74C` netblob count, while `all` above is this
    // file's independent reassembly and decode of the retained raw datagrams. The per-session
    // table below names which recording moved.
    assert_eq!(
        all.len(),
        recorded_movement_events(),
        "every recorded 0xF74C body was decoded"
    );
    // Where they came from, so a total that moves names the session that moved it.
    // Every recorded buffer is the interpreted arm or one of the four move-to/turn-to arms.
    assert_eq!(
        by_type[1..6].iter().sum::<usize>(),
        0,
        "a 0xF74C with an unknown arm: {by_type:?}"
    );
    assert!(
        player_by_type[0] > 0 && player_autonomous > 0,
        "the corpus carries interpreted buffers and autonomous echoes addressed to the player \
         ({player_by_type:?}, {player_autonomous} autonomous)"
    );
    assert_eq!(
        player_styles.keys().copied().collect::<Vec<_>>(),
        vec![0u16, 60, 61, 63, 64, 73],
        "the style indices the server sent the player"
    );
    // How often each, so "the set is the same six" is not the whole claim.
    assert_eq!(
        MotionCommand::from_index(60),
        Some(MotionCommand(0x8000_003C))
    ); // HandCombat
    assert_eq!(
        MotionCommand::from_index(61),
        Some(MotionCommand(0x8000_003D))
    ); // NonCombat
    assert_eq!(
        MotionCommand::from_index(63),
        Some(MotionCommand(0x8000_003F))
    ); // BowCombat
    assert_eq!(
        MotionCommand::from_index(73),
        Some(MotionCommand(0x8000_0049))
    ); // Magic
    assert_eq!(
        MotionCommand::from_index(61),
        Some(MotionCommand::NON_COMBAT)
    );
    assert_eq!(
        MotionCommand::from_index(63),
        Some(MotionCommand::BOW_COMBAT)
    );
    // The sixth, `requested-death-vitae-salvage`'s: index 64 is the table entry one past
    // BowCombat.
    assert_eq!(
        MotionCommand::from_index(64),
        Some(MotionCommand(0x8000_0040))
    );
    assert!(MotionCommand::from_index(64)
        .expect("index 64 is in the table")
        .is_style());
    // Five distinct stances beside the style-zero header, so this is a transition and not one
    // constant repeated.
    assert!(player_styles.len() >= 6);
}

// ---------------------------------------------------------------------------------------------
// 3. The seam. A real `WorldScene` with a real local body, driven by the capture's own datagrams
//    through `WorldScene::sync_objects` — the application's own path and no other.
// ---------------------------------------------------------------------------------------------

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// One `0xF74C` seen arriving at `sync_objects`, with what the player's own driver held on either
/// side of it.
#[derive(Debug, Clone)]
struct Station {
    movement_type: u8,
    style_index: u16,
    /// The `InterpretedMotionState.current_style` the buffer itself carried, when it carried one.
    state_style: Option<u16>,
    /// `forward_command` index the buffer carried, when it carried a state.
    state_forward: Option<u16>,
    style_before: MotionCommand,
    style_after: MotionCommand,
    forward_before: MotionCommand,
    forward_after: MotionCommand,
    move_tos_before: u64,
    move_tos_after: u64,
    /// True for the one station the corpus does not supply on its own — see
    /// [`the_style_word_on_a_move_to_arm_changes_the_players_stance`].
    injected: bool,
}

/// Replay `session` and run the real `WorldScene::sync_objects` on every frame at which the
/// player has a movement buffer waiting, stopping after `limit` of them.
///
/// This is the whole application path: `dereth_client_net`'s transport, `dereth_client_net::client_session`'s dispatchers,
/// `ObjectStream::pump` (which applies retail's three movement-update gates) and then
/// `WorldScene::sync_objects`, which is where `apply_player_movement` lives.
fn drive(session: &str, limit: usize) -> (Vec<Station>, ObjectId, bool, u64) {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    // Every `MoveTo`/`TurnTo` body this session sent its own character, in wire order, kept whole
    // so one of them can be re-delivered below.
    let edge_candidates: Vec<(u8, u16, Vec<u8>)> = {
        let (all, chars) = corpus_buffers();
        all.iter()
            .filter(|b| {
                b.session == session
                    && !b.autonomous
                    && b.body.movement_type != movement_type::INVALID
                    && chars[&b.session].contains(&b.mover)
            })
            .map(|b| (b.body.movement_type, b.body.current_style, b.blob.clone()))
            .collect()
    };
    let records = shared_session(session);
    assert!(!records.is_empty(), "{session} is empty");
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<WorldScene> = None;
    let mut stations = Vec::new();
    let mut player = ObjectId(0);
    let mut player_ever_a_scene_object = false;
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
        let Some(id) = stream.player() else { continue };
        player = id;
        // Build the scene once the server has said where the player is — the same decision
        // `App::load_pending_scene` makes with a live session.
        if scene.is_none() {
            let Some(pos) = stream.presence(id).and_then(|p| p.position) else {
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
                dereth_world_data::landblock::load_region(&store).expect("the region decodes");
            s.attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        let waiting = stream.presence(id).and_then(|p| p.pending_movement.clone());
        let Some(buf) = waiting else { continue };
        if buf.autonomous {
            // The upstream autonomous-player echo guard has already refused these; nothing remains
            // to observe on the local-body path.
            let _ = stream.take_movement(id);
            continue;
        }
        let c = s.character.as_ref().expect("the scene has a local body");
        let (style_before, forward_before) = {
            let d = c.driver();
            (
                d.movement.interp.interpreted_state.current_style,
                d.movement.interp.interpreted_state.forward_command,
            )
        };
        let move_tos_before = c.stats.move_tos_performed;
        s.sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        // **Sampled here, before any frame is advanced.** A one-shot action completes on the very
        // next completed-motion check and returns the interpreted state to `Ready`, so a sample
        // taken after `Character::update` reads the recovery rather than the buffer (a `Pickup`
        // would read as `Ready`). Sampling first measures the transient command while it exists.
        if s.server_object_motion(id).is_some() {
            player_ever_a_scene_object = true;
        }
        let c = s.character.as_ref().expect("the scene has a local body");
        let (style_after, forward_after) = {
            let d = c.driver();
            (
                d.movement.interp.interpreted_state.current_style,
                d.movement.interp.interpreted_state.forward_command,
            )
        };
        let move_tos_after = c.stats.move_tos_performed;
        // Now let the body take a frame, so a move-to that was recorded can begin.
        t += 1.0 / 30.0;
        if let Some(c) = s.character.as_mut() {
            c.update(LocalTime(t));
        }
        stations.push(Station {
            movement_type: buf.body.movement_type,
            style_index: buf.body.current_style,
            state_style: buf.body.interpreted.as_ref().and_then(|s| s.current_style),
            state_forward: buf
                .body
                .interpreted
                .as_ref()
                .and_then(|s| s.forward_command),
            style_before,
            style_after,
            forward_before,
            forward_after,
            move_tos_before,
            move_tos_after,
            injected: false,
        });
    }

    // ------------------------------------------------------------------------------------
    // The discriminating station the corpus does not contain.
    //
    // Measured by [`no_recorded_move_to_arm_ever_changes_the_players_stance`]: none of the
    // server's stance changes for the player rides on a `MoveTo`/`TurnTo` buffer, so a suite
    // built only from the recording cannot tell a build that applies the header's style word
    // from one that ignores it on those arms. The case is therefore constructed with a
    // **recorded body**: one of this session's own `TurnToObject` blobs, delivered byte for byte
    // through `ObjectStream`'s own `0xF74C` path, at a moment when the stance in force differs
    // from the one its header names. Nothing is synthesised; only the delivery order is ours.
    let stance = stations
        .last()
        .map_or(MotionCommand::NON_COMBAT, |x| x.style_after);
    // The **last** such body whose style word is not the stance already in force: last, so its
    // movement timestamp is the newest in the session and the movement update's first gate (the
    // timestamp must be newer than the last one applied) lets it through.
    let edge_blob = edge_candidates
        .iter()
        .rev()
        .find(|(_, ix, _)| MotionCommand::from_index(*ix) != Some(stance))
        .cloned();
    if let (Some(edge), Some(s)) = (edge_blob, scene.as_mut()) {
        let (style_before, forward_before) = {
            let d = s.character.as_ref().expect("a body").driver();
            (
                d.movement.interp.interpreted_state.current_style,
                d.movement.interp.interpreted_state.forward_command,
            )
        };
        assert_ne!(
            MotionCommand::from_index(edge.1),
            Some(stance),
            "the injected body must name a stance the player is not already in"
        );
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                body: edge.2.clone(),
            },
            LocalTime(t),
        );
        assert!(
            stream
                .presence(player)
                .is_some_and(|p| p.pending_movement.is_some()),
            "the re-delivered body did not survive the movement update's timestamp gates"
        );
        let move_tos_before = s
            .character
            .as_ref()
            .expect("a body")
            .stats
            .move_tos_performed;
        s.sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        let c = s.character.as_ref().expect("a body");
        let d = c.driver();
        stations.push(Station {
            movement_type: edge.0,
            style_index: edge.1,
            state_style: None,
            state_forward: None,
            style_before,
            style_after: d.movement.interp.interpreted_state.current_style,
            forward_before,
            forward_after: d.movement.interp.interpreted_state.forward_command,
            move_tos_before,
            move_tos_after: c.stats.move_tos_performed,
            injected: true,
        });
    }
    (
        stations,
        player,
        player_ever_a_scene_object,
        stream.stats.movement_own_echo,
    )
}

/// `long-solo-play` is the capture whose character fights, shoots and casts: it is the only one that
/// carries all four stances. 90 stations is enough to cross every one of them.
fn long_solo_play_stations() -> (Vec<Station>, ObjectId, bool, u64) {
    drive("long-solo-play", 90)
}

/// **The interpreted arm** reaches the local player's driver.
///
/// The observable is deliberately *not* the stance: it is `forward_command`, which no style word
/// carries and which only interpreted-movement application can write. A build that applied the
/// style word but skipped the interpreted arm passes the stance test and fails this one.
#[test]
fn the_players_own_interpreted_state_reaches_his_body() {
    let (stations, _, _, _) = long_solo_play_stations();
    let interpreted: Vec<&Station> = stations
        .iter()
        .filter(|s| s.movement_type == movement_type::INVALID)
        .collect();
    assert!(
        interpreted.len() >= 20,
        "the capture must drive the player's interpreted arm; {} stations",
        interpreted.len()
    );
    // Every one of them must leave the driver holding exactly the command the buffer named.
    let mut checked = 0usize;
    for s in &interpreted {
        let Some(ix) = s.state_forward else { continue };
        let Some(want) = MotionCommand::from_index(ix) else {
            continue;
        };
        assert_eq!(
            s.forward_after, want,
            "an interpreted buffer naming forward index {ix} left the player on {:?}",
            s.forward_after
        );
        checked += 1;
    }
    assert!(
        checked >= 20,
        "only {checked} interpreted buffers carried a forward command"
    );
    // ...and it must actually *change*, at least once, or the assertion above is satisfied by a
    // driver that was already there, so the fixture requires both sides of the state edge.
    assert!(
        interpreted
            .iter()
            .any(|s| s.forward_before != s.forward_after),
        "not one interpreted buffer moved the player's forward command"
    );
}

/// **The corpus alone cannot see the style word on a `MoveTo`/`TurnTo` arm**, and this measures
/// that rather than assuming it.
///
/// None of the server's stance changes for the player rides on a `MoveTo`/`TurnTo` buffer: ACE
/// always sends the stance in an interpreted buffer, and every `MoveTo`/`TurnTo` it sends
/// afterwards repeats the style already in force. So a suite assembled only out of the recording
/// cannot observe the style word being applied on those arms, which is why the next test
/// constructs its station.
#[test]
fn no_recorded_move_to_arm_ever_changes_the_players_stance() {
    let (all, chars) = corpus_buffers();
    let mut in_force: BTreeMap<String, u16> = BTreeMap::new();
    let (mut edges, mut on_a_move_to_arm) = (0usize, 0usize);
    for b in &all {
        if b.autonomous || !chars[&b.session].contains(&b.mover) {
            continue;
        }
        // `NonCombat` (index 61) is where both motion-state constructors start.
        let cur = in_force.entry(b.session.clone()).or_insert(61);
        if *cur != b.body.current_style {
            edges += 1;
            if b.body.movement_type != movement_type::INVALID {
                on_a_move_to_arm += 1;
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
    eprintln!("player stance edges={edges} on_a_move_to_arm={on_a_move_to_arm}");
    // The claim is the zero: it holds across every recording, including `melee-attack-run`,
    // whose move-to arms ACE composes by a different path.
    assert!(
        edges > 0,
        "the server sent the player no stance change at all"
    );
    assert_eq!(
        on_a_move_to_arm, 0,
        "a MoveTo/TurnTo arm carried one after all -- re-aim the test"
    );
}

/// **The header's style word**, isolated on the arms that carry no interpreted state.
///
/// A `MoveTo`/`TurnTo` buffer has **no** `InterpretedMotionState` in it at all, so the only thing
/// in it that can change a stance is the style index in its header, which retail reads and
/// applies before the arm is unpacked. A build that handled only the interpreted arm passes the
/// test above and fails this one.
///
/// Because the recording never exercises that edge (the test above measures it at zero), the
/// last station is one of `long-solo-play`'s own `TurnToObject` bodies, re-delivered byte for byte
/// through `ObjectStream`'s `0xF74C` path at a moment when the player is in a different stance.
/// The body is recorded; only the moment is ours.
#[test]
fn the_style_word_on_a_move_to_arm_changes_the_players_stance() {
    let (stations, _, _, _) = long_solo_play_stations();
    let move_tos: Vec<&Station> = stations
        .iter()
        .filter(|s| s.movement_type != movement_type::INVALID)
        .collect();
    assert!(
        move_tos.len() >= 5,
        "the capture must drive the player's MoveTo/TurnTo arms; {} stations",
        move_tos.len()
    );
    for s in &move_tos {
        assert!(
            s.state_style.is_none(),
            "a MoveTo arm carries no interpreted state"
        );
        let want = MotionCommand::from_index(s.style_index)
            .unwrap_or_else(|| panic!("style index {} is not in command_ids", s.style_index));
        assert_eq!(
            s.style_after, want,
            "a type-{} buffer with style index {} left the player on {:?}",
            s.movement_type, s.style_index, s.style_after
        );
    }
    // The edge, and it is the constructed one.
    let edge = move_tos
        .iter()
        .find(|s| s.injected)
        .expect("the constructed MoveTo/TurnTo station is missing");
    assert_ne!(
        edge.style_before, edge.style_after,
        "a re-delivered TurnToObject naming style index {} left the stance at {:?}",
        edge.style_index, edge.style_after
    );
    assert_eq!(
        edge.style_after,
        MotionCommand::from_index(edge.style_index).expect("in command_ids"),
        "the stance did not follow the re-delivered body's own style word"
    );
    // It really is an arm with nothing else in it that could have moved the stance.
    assert_ne!(edge.movement_type, movement_type::INVALID);
}

/// Behaviour: movement.style.the-players-stance-follows-the-servers-style-word
/// **The stance follows the server across every transition the capture holds**, and it holds all
/// four styles: `NonCombat`, `HandCombat`, `BowCombat` and `Magic`.
///
/// Asserted at every driven station, over styles the corpus actually carries.
#[test]
fn the_players_stance_follows_the_servers_current_style() {
    let (stations, _, _, _) = long_solo_play_stations();
    let interpreted = stations
        .iter()
        .filter(|s| s.movement_type == movement_type::INVALID)
        .count();
    let injected = stations.iter().filter(|s| s.injected).count();
    eprintln!(
        "player stance: {} stations driven through WorldScene::sync_objects; {interpreted} interpreted, {} MoveTo/TurnTo, of which {injected} constructed",
        stations.len(),
        stations.len() - interpreted,
    );
    assert!(
        interpreted > 0 && interpreted < stations.len(),
        "the driven window holds interpreted and MoveTo/TurnTo stations ({interpreted} of {})",
        stations.len()
    );
    assert_eq!(injected, 1, "the one constructed station");
    for s in &stations {
        // On an interpreted buffer the state's own `current_style` is the later word: the
        // interpreted state is applied after the header's style and copies its own.
        let want_ix = s.state_style.unwrap_or(s.style_index);
        let Some(want) = MotionCommand::from_index(want_ix) else {
            continue;
        };
        assert_eq!(
            s.style_after, want,
            "type {} style_ix {} state_style {:?}: the player is on {:?}",
            s.movement_type, s.style_index, s.state_style, s.style_after
        );
    }
    let seen: std::collections::BTreeSet<MotionCommand> =
        stations.iter().map(|s| s.style_after).collect();
    for want in [
        MotionCommand::NON_COMBAT,
        MotionCommand::HAND_COMBAT,
        MotionCommand::BOW_COMBAT,
        MotionCommand::from_index(73).expect("Magic"),
    ] {
        assert!(
            seen.contains(&want),
            "the player never reached {want:?}; saw {seen:?}"
        );
    }
    // Entering combat and leaving it, both seen, both from the server's own bytes.
    assert!(
        stations
            .windows(2)
            .any(|w| w[0].style_after == MotionCommand::NON_COMBAT
                && w[1].style_after != MotionCommand::NON_COMBAT),
        "the capture never shows the player entering combat"
    );
    assert!(
        stations
            .windows(2)
            .any(|w| w[0].style_after != MotionCommand::NON_COMBAT
                && w[1].style_after == MotionCommand::NON_COMBAT),
        "the capture never shows the player leaving combat"
    );
}

/// **Why the player's buffer has its own route, pinned structurally.**
///
/// With a local body the player has no `SceneObject`, so `apply_movement` (the remote objects'
/// route to `MotionDriver::unpack_interpreted_movement`) can never apply his buffer; it must go
/// through `apply_player_movement`.
#[test]
fn the_player_has_no_scene_object_for_apply_movement_to_find() {
    let (stations, player, ever, _) = long_solo_play_stations();
    assert!(!stations.is_empty(), "the drive produced no stations");
    assert_ne!(player, ObjectId(0), "the capture named a player");
    assert!(
        !ever,
        "the player has a SceneObject: `apply_movement` could reach him and this pin is void"
    );
}

/// **The `MoveToObject` arm reaches the move-to manager**, from the capture's own bytes.
///
/// The approach is a different arm from the stance; this asserts that a recorded `MoveToObject`
/// addressed to the player reaches the move-to manager alongside the stance change.
#[test]
fn a_recorded_move_to_object_still_reaches_perform_movement() {
    let (stations, _, _, _) = long_solo_play_stations();
    let sixes: Vec<&Station> = stations
        .iter()
        .filter(|s| s.movement_type == movement_type::MOVE_TO_OBJECT)
        .collect();
    assert!(
        !sixes.is_empty(),
        "the drive saw no type-6 buffer; widen the limit or pick another capture"
    );
    for s in &sixes {
        assert_eq!(
            s.move_tos_after,
            s.move_tos_before + 1,
            "a recorded MoveToObject did not reach perform_move_to"
        );
    }
    // And the `TurnTo` arm reaches it too, so the count above measures the `MoveToObject` arm
    // rather than "some arm ran".
    let turns = stations
        .iter()
        .filter(|s| s.movement_type == movement_type::TURN_TO_OBJECT)
        .filter(|s| s.move_tos_after == s.move_tos_before + 1)
        .count();
    assert!(turns > 0, "no TurnToObject reached perform_move_to either");
}

/// **Why `apply_player_movement`'s own autonomous guard cannot be falsified here, structurally.**
///
/// Retail refuses an autonomous buffer about the player ("the server echoing what this client
/// raised"). This build checks that twice: once in `objects.rs::set_object_movement`, where it
/// belongs and counts its refusals, and once as the opening line of `apply_player_movement`. The
/// upstream check runs first and never parks the buffer, so the downstream one is unreachable:
/// the two read the same predicate off the same buffer, so the second can only agree with the
/// first. The counter below makes that a measurement rather than an argument.
///
/// The same holds for the interpreted-state application's "skip an autonomous action node when
/// this is the player" rule: of the non-autonomous buffers the server addressed to a player, a few
/// carry an action node and **none** carries an autonomous one. ACE marks a node autonomous
/// exactly when it echoes what the client raised, and puts those in a buffer that is autonomous
/// as a whole, which the gate above has already dropped.
#[test]
fn the_autonomous_echo_is_refused_upstream_so_the_second_guard_is_unreachable() {
    let (stations, _, _, own_echo) = long_solo_play_stations();
    assert!(
        own_echo > 0,
        "long-solo-play must exercise the echo rule for this to be a measurement; {own_echo}"
    );
    assert!(!stations.is_empty());
    // Nothing autonomous ever reached `sync_objects`: `drive` records a station only for a buffer
    // that `ObjectStream` actually parked, and it parks none of the echoes.
    let (all, chars) = corpus_buffers();
    let (mut non_auto, mut with_action, mut with_autonomous_action) = (0usize, 0usize, 0usize);
    for b in &all {
        if b.autonomous || !chars[&b.session].contains(&b.mover) {
            continue;
        }
        non_auto += 1;
        if let Some(s) = b.body.interpreted.as_ref() {
            if !s.actions.is_empty() {
                with_action += 1;
            }
            if s.actions
                .iter()
                .any(dereth_protocol::movement::MotionAction::autonomous)
            {
                with_autonomous_action += 1;
            }
        }
    }
    eprintln!("player echoes non_auto={non_auto} with_action={with_action} with_auto_action={with_autonomous_action}");
    // The census above gives the denominator (`player_by_type` in its print).
    // `with_autonomous_action` being 0 is this test's claim and what makes the second check
    // unreachable.
    // The zero below is the claim, over a denominator that is not empty.
    assert!(
        non_auto > 0 && with_action > 0,
        "non-autonomous 0xF74C addressed to a player {non_auto}, of which with an action node \
         {with_action}"
    );
    assert_eq!(
        with_autonomous_action, 0,
        "an autonomous action node inside a non-autonomous buffer -- if this ever fires, \
         the player's skip-autonomous-node rule has become observable and needs a test of its own"
    );
}
