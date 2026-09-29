//! A remote object's `MoveTo`/`TurnTo` arms of `Movement_SetObjectMovement 0xF74C` reach its own
//! move-to manager, asserted **by arm**, because one working arm would carry three broken ones in
//! any aggregate: `MoveToObject` (6) and `TurnToObject` (8) resolve the named object and fall back
//! to `MoveToPosition` / `TurnToHeading` when it is missing; `MoveToPosition` (7) and
//! `TurnToHeading` (9) do not resolve anything. Most object-shaped arms name the session's own
//! character, whom the world's server objects do not hold with a local body, so the resolver also
//! consults the local character; no arm addressed to the player names the player himself. The
//! move-to manager then issues the walk and turn commands every frame. Fixture: every retained raw
//! capture replayed through `ObjectStream` and the world's object dispatch (re-entering the world
//! where each recording did) on the retail dats with no device, and the reassembled message corpus
//! for the arm census.

use crate::common::sim::SimWorld;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_animation::table::MovementType;
use dereth_animation::MotionCommand;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::SceneConfig;
use dereth_client_net::client_session::testing::{
    session_names, shared_session, Corpus, Direction,
};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::{connection_sequence_number, recorded_enter_world_requests};
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::movement::{
    movement_type, MoveToArm, MovementBody, MovementSetObjectMovement,
};
use dereth_protocol::{Message, Opcode};

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
    body: MovementBody,
}

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
                body: buf.body,
            });
        }
    }
    assert!(!all.is_empty(), "the corpus carries no 0xF74C at all");
    (all, chars)
}

// ---------------------------------------------------------------------------------------------
// 2. The census, by arm.
// ---------------------------------------------------------------------------------------------

/// **The remote arm census, and what each arm names.**
#[test]
fn every_remote_move_to_object_names_the_sessions_character_and_no_player_arm_names_the_player() {
    let (all, chars) = corpus_buffers();
    let mut remote_by_arm = [0usize; 10];
    let mut player_by_arm = [0usize; 10];
    let (mut remote, mut arms) = (0usize, 0usize);
    // How many of each object-shaped arm name the session's own character.
    let (mut move_to_names_char, mut turn_to_names_char) = (0usize, 0usize);
    // Player-addressed arms that name the player himself -- if this is ever non-zero, putting the
    // player into `move_to_target`'s object table changes `apply_player_movement` too.
    let mut player_arm_names_self = 0usize;
    let mut turn_to_names_a_stranger: Vec<(String, ObjectId, ObjectId)> = Vec::new();
    let mut positions_named: BTreeSet<u32> = BTreeSet::new();
    let mut headings_named = 0usize;
    let mut heading_override_matters = 0usize;
    for b in &all {
        let mine = chars[&b.session].contains(&b.mover);
        if mine {
            player_by_arm[b.body.movement_type as usize] += 1;
        } else {
            remote += 1;
            remote_by_arm[b.body.movement_type as usize] += 1;
        }
        if b.body.movement_type == movement_type::INVALID {
            continue;
        }
        let arm = b
            .body
            .decode_move_to()
            .expect("a recorded arm decodes")
            .expect("not the interpreted arm");
        if mine {
            let target = match arm {
                MoveToArm::MoveToObject { target, .. } | MoveToArm::TurnToObject { target, .. } => {
                    Some(target)
                }
                _ => None,
            };
            if target == Some(b.mover) {
                player_arm_names_self += 1;
            }
            continue;
        }
        arms += 1;
        match arm {
            MoveToArm::MoveToObject { target, .. } => {
                if chars[&b.session].contains(&target) {
                    move_to_names_char += 1;
                }
            }
            MoveToArm::TurnToObject {
                target,
                desired_heading,
                params,
            } => {
                if chars[&b.session].contains(&target) {
                    turn_to_names_char += 1;
                } else {
                    turn_to_names_a_stranger.push((b.session.clone(), b.mover, target));
                }
                // The `TurnToObject` arm reads the heading **before** unpacking the parameters
                // and writes it **over** the `desired_heading` they carry. Whether that override
                // does anything is a property of the traffic, and this is the count that says so:
                // the recorded headings never differ, so the override is unobservable here.
                let carried = match params {
                    dereth_protocol::movement::MovementParameters::TurnTo {
                        desired_heading,
                        ..
                    }
                    | dereth_protocol::movement::MovementParameters::MoveTo {
                        desired_heading,
                        ..
                    } => desired_heading,
                };
                if (desired_heading - carried).abs() > 1e-6 {
                    heading_override_matters += 1;
                }
            }
            MoveToArm::MoveToPosition { origin, .. } => {
                positions_named.insert(origin.objcell_id);
            }
            MoveToArm::TurnToHeading { .. } => headings_named += 1,
        }
    }
    eprintln!(
        "remote arms: remote arms {remote_by_arm:?}; player arms {player_by_arm:?}; \
         MoveToObject naming the character {move_to_names_char}, TurnToObject {turn_to_names_char}"
    );
    eprintln!("remote arms: turn-to arms naming someone else {turn_to_names_a_stranger:?}");
    // Every remote buffer is the interpreted arm or one of the four move-to/turn-to arms.
    assert_eq!(
        remote_by_arm[0] + arms,
        remote,
        "a remote 0xF74C with an arm other than interpreted, MoveTo or TurnTo: {remote_by_arm:?}"
    );
    assert!(
        remote_by_arm[6] > 0 && remote_by_arm[8] > 0,
        "the corpus carries remote MoveToObject and TurnToObject arms: {remote_by_arm:?}"
    );

    // **The fact the resolver turns on.** Every remote `MoveToObject` names the recording's own
    // character.
    assert_eq!(
        move_to_names_char, remote_by_arm[6],
        "every remote MoveToObject names the session's character"
    );
    // Remote `TurnToObject` arms name the session's own character or someone the session's
    // `0xF658` never listed: in the `fellowship-*` recordings another fellowship member, elsewhere
    // a world object (the villa landblock `0x79DAF0__`, items). Those reach the resolver with an id
    // the character list does not hold.
    eprintln!("remote arms: turn-to arms naming someone else {turn_to_names_a_stranger:?}");
    assert_eq!(
        turn_to_names_char + turn_to_names_a_stranger.len(),
        remote_by_arm[8],
        "every remote TurnToObject is accounted for by one of the two"
    );
    assert_eq!(
        headings_named, remote_by_arm[9],
        "every remote TurnToHeading is decoded as one"
    );
    eprintln!(
        "remote arms: {} distinct cells named by MoveToPosition",
        positions_named.len()
    );
    assert_eq!(
        heading_override_matters, 0,
        "the TurnToObject arm's own heading now differs from the one its parameters carry, so the \
         override is observable -- assert the resulting heading"
    );

    // **The safety measurement for the shared resolver**, over a denominator that is not empty.
    assert!(
        player_by_arm[6] + player_by_arm[8] > 0,
        "the corpus carries no object-shaped arm addressed to the player: {player_by_arm:?}"
    );
    assert_eq!(
        player_arm_names_self, 0,
        "a player-addressed arm names the player himself, so adding the player to \
         `move_to_target` changes `apply_player_movement` after all -- re-measure the player's arms"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The seam.
// ---------------------------------------------------------------------------------------------

/// The retail dats, or **fail**: a missing dat is a failure, never a silent pass.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// The world a recording entered, with the body, and no device.
fn world_at(store: &Arc<RetailDatStore>, landblock: u16) -> SimWorld {
    SimWorld::load(
        store,
        SceneConfig {
            landblock,
            character: true,
            land_radius: 1,
            scenery_radius: 0,
            ..SceneConfig::default()
        },
    )
}

/// One recorded `MoveTo`/`TurnTo` buffer, followed to the object's own `MoveToManager`.
#[derive(Debug, Clone)]
struct ArmStation {
    mover: ObjectId,
    /// The wire arm: 6, 7, 8 or 9.
    wire_arm: u8,
    /// The player at the moment the arm was sampled. A recording that enters the world as more
    /// than one character has more than one, so the session's last is not the answer.
    player: ObjectId,
    /// Whether the buffer named an object and whether that object resolved.
    named: Option<ObjectId>,
    /// The movement manager's `movement_type` field immediately after `sync_objects`.
    kind: MovementType,
    /// The movement manager's `top_level_object_id` field.
    top: ObjectId,
    /// The physics object's current target record.
    target: Option<ObjectId>,
    moving: bool,
}

/// Replay `session` and run the world's object dispatch on every frame at which some object has a
/// movement buffer waiting. The same dispatch a drawn scene's `sync_objects` runs, and nothing is
/// synthesised in it.
fn drive(session: &str) -> (Vec<ArmStation>, u64, ObjectId, Vec<(ObjectId, u8)>) {
    let store = store();
    let records = shared_session(session);
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entries = recorded_enter_world_requests(
        records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.c2s)
            .map(|(i, r)| (i, r.raw.as_slice())),
    )
    .into_iter()
    .peekable();
    let mut scene: Option<SimWorld> = None;
    let mut stations = Vec::new();
    let mut absent: Vec<(ObjectId, u8)> = Vec::new();
    let mut stopped_at: Option<f64> = None;
    // Latched: `stream.player()` is `None` again after the capture's own log-off.
    let mut player_id = ObjectId(0);

    for (index, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let _ = stream.pump(&mut net, now);
        // Enter the world where the recording's client did, as the character it named -- every
        // time, not once: `requested-death-vitae-salvage` re-enters six times on one connection
        // and `pre-relog-play` twice. See `common::enter_world`.
        if let Some(e) = entries.next_if(|e| e.record == index) {
            net.enter_world(e.character, &e.account);
        }
        let Some(player) = stream.player() else {
            continue;
        };
        player_id = player;
        if scene.is_none() {
            let Some(pos) = stream.presence(player).and_then(|p| p.position) else {
                continue;
            };
            let block = pos.cell.landblock();
            scene = Some(world_at(
                &store,
                (u16::from(block.x()) << 8) | u16::from(block.y()),
            ));
        }
        let s = scene.as_mut().expect("built above");
        // Sampled before `sync_objects` consumes them.
        let waiting: Vec<(ObjectId, u8, Option<ObjectId>)> = stream
            .presences()
            .filter(|(id, _)| *id != player)
            .filter_map(|(id, p)| {
                let b = p.pending_movement.as_ref()?;
                if b.body.movement_type == movement_type::INVALID {
                    return None;
                }
                let named = match b.body.decode_move_to().ok().flatten() {
                    Some(MoveToArm::MoveToObject { target, .. })
                    | Some(MoveToArm::TurnToObject { target, .. }) => Some(target),
                    _ => None,
                };
                Some((id, b.body.movement_type, named))
            })
            .collect();
        // **`sync_objects` runs on every iteration, and skipping it when nothing is waiting is a
        // real bug rather than an optimisation.** `Presence::pending_movement` is a *single slot*:
        // a second `0xF74C` for the same object before the scene has consumed the first
        // overwrites it, so a drive that only syncs when an arm is parked loses every arm an
        // interpreted buffer happens to follow.
        // **A `sync_objects` that fails is a stop, not a panic**: a scene limit is reported with
        // how far the drive got, rather than presented as a shortfall in `apply_movement`. Every
        // session currently reports `stopped_at None`, printed below.
        if let Err(e) = s.sync_objects(&mut stream) {
            eprintln!(
                "remote arms {session}: the scene stopped at t = {:.1} s: {e}",
                r.t
            );
            stopped_at = Some(r.t);
            break;
        }
        if waiting.is_empty() {
            continue;
        }
        // **Sampled immediately**, before any frame is advanced: a move-to time step can complete a
        // node and clean the manager up on the very next step, so a sample taken after stepping
        // reads the recovery rather than the buffer. This samples the state while it exists.
        for (id, wire_arm, named) in waiting {
            let Some((moving, kind, top)) = s.ws().server_object_move_to(id) else {
                // A presence the scene never built a `SceneObject` for: `apply_movement` is gated
                // on the object table exactly as retail's unpacking is gated on the object having
                // a physics body, so there is nothing to unpack the buffer into.
                absent.push((id, wire_arm));
                continue;
            };
            stations.push(ArmStation {
                player,
                mover: id,
                wire_arm,
                named,
                kind,
                top,
                target: s.ws().server_object_target(id),
                moving,
            });
        }
    }
    let Some(s) = scene.as_ref() else {
        // A login-only recording never places a player, so there is no scene and no arm; its
        // zero is still measured, because the census reads the same recording.
        assert!(
            stations.is_empty() && absent.is_empty(),
            "{session}: arms with no scene"
        );
        eprintln!("remote arms {session}: never entered the world, no scene");
        return (stations, 0, player_id, absent);
    };
    eprintln!(
        "remote arms {session}: {} objects live at the end, stopped_at {stopped_at:?}",
        s.ws().server_object_count()
    );
    if !absent.is_empty() {
        eprintln!(
            "remote arms {session}: {} arm buffers for a presence with no SceneObject: {absent:?}",
            absent.len()
        );
    }
    (
        stations,
        s.object_steps().remote_move_tos_performed,
        player_id,
        absent,
    )
}

/// **Every recorded remote arm reaches the move-to manager, asserted BY ARM.**
///
/// Every recorded remote `MoveTo`/`TurnTo` buffer is followed to the object's own `MoveToManager`,
/// and each of the four arms is counted separately: one arm working and three not would pass an
/// aggregate test.
///
/// The expected end state of each arm is retail's:
///
/// | wire arm | manager `movement_type` | `top_level_object_id` |
/// |---|---|---|
/// | 6 `MoveToObject`, target resolved | `MoveToObject` | the target's top-level id |
/// | 6, target missing | `MoveToPosition` (the fall-through) | 0 |
/// | 7 `MoveToPosition` | `MoveToPosition` | 0 |
/// | 8 `TurnToObject`, target resolved | `TurnToObject` | the target's top-level id |
/// | 8, target missing | `TurnToHeading` (the fall-through) | 0 |
/// | 9 `TurnToHeading` | `TurnToHeading` | 0 |
#[test]
fn every_recorded_remote_arm_reaches_the_move_to_manager() {
    // The dats are required: they fail when absent.
    let _ = store();
    let mut seen = [0usize; 10];
    let mut right = [0usize; 10];
    let mut targeted = [0usize; 10];
    let mut absent = [0usize; 10];
    let mut moving = [0usize; 10];
    let mut performed = 0u64;
    let mut named_player = [0usize; 10];
    let mut named_stranger: Vec<(ObjectId, ObjectId)> = Vec::new();
    // Arms whose target this client never had and whose fall-back turn was already satisfied, so
    // the manager finished and cleaned up inside the call. Named below.
    let mut done_on_arrival: Vec<(String, ObjectId, u8, Option<ObjectId>)> = Vec::new();
    let mut arrived = [0usize; 10];
    // Every recording, because the bound below is corpus arithmetic and the drive has to be over
    // the same corpus the census reads. A recording with no remote arm (or a login-only one) is
    // driven anyway so its zero is a measured zero. `drive` re-enters the world wherever the
    // recording did, so a recording with several enter-worlds is followed across all of them.
    for session in corpus_sessions() {
        let session = session.as_str();
        let (stations, n, _last_player, missing) = drive(session);
        performed += n;
        for (_, arm) in &missing {
            absent[*arm as usize] += 1;
        }
        eprintln!(
            "remote arms {session}: {} arm stations, performed={n}",
            stations.len()
        );
        for st in &stations {
            seen[st.wire_arm as usize] += 1;
            // Every object-shaped arm in this corpus names the player, so the resolved arm is the
            // one that must be taken; a fall-through here would mean `move_to_target` lost him.
            let expect_kind = match st.wire_arm {
                6 => MovementType::MoveToObject,
                7 => MovementType::MoveToPosition,
                8 => MovementType::TurnToObject,
                9 => MovementType::TurnToHeading,
                other => panic!("wire arm {other} is not one of the four"),
            };
            let expect_top = match st.wire_arm {
                6 | 8 => {
                    // The expectation is the id the **wire** named, which is the resolver's
                    // contract: most name the player, some name another player or a world object
                    // (`every_remote_move_to_object_names_the_sessions_character_and_no_player_arm_names_the_player`
                    // names them). The split is counted below so that the stranger arms cannot
                    // quietly stop arriving.
                    let named = st.named.unwrap_or_else(|| {
                        panic!("{:?}'s arm {} named nothing", st.mover, st.wire_arm)
                    });
                    if named == st.player {
                        named_player[st.wire_arm as usize] += 1;
                    } else {
                        named_stranger.push((st.mover, named));
                    }
                    named
                }
                _ => ObjectId(0),
            };
            if st.kind == expect_kind && st.top == expect_top {
                right[st.wire_arm as usize] += 1;
            } else if st.kind == MovementType::Invalid && !st.moving && st.target.is_none() {
                arrived[st.wire_arm as usize] += 1;
                done_on_arrival.push((session.to_string(), st.mover, st.wire_arm, st.named));
            } else {
                eprintln!(
                    "remote arms:   {:?} arm {} -> kind {:?} top {:?} (wanted {expect_kind:?} / \
                     {expect_top:?})",
                    st.mover, st.wire_arm, st.kind, st.top
                );
            }
            if st.target == Some(st.player) {
                targeted[st.wire_arm as usize] += 1;
            }
            if st.moving {
                moving[st.wire_arm as usize] += 1;
            }
        }
    }
    eprintln!(
        "remote arms: arms seen {:?}, in the right state {:?}, watching the player {:?}, \
         performed={performed}",
        [seen[6], seen[7], seen[8], seen[9]],
        [right[6], right[7], right[8], right[9]],
        [targeted[6], targeted[7], targeted[8], targeted[9]]
    );
    eprintln!(
        "remote arms: arms whose mover the scene never built {:?}",
        [absent[6], absent[7], absent[8], absent[9]]
    );
    eprintln!(
        "remote arms: object-shaped arms naming the player {:?}, naming another player {named_stranger:?}",
        [named_player[6], named_player[8]]
    );
    // **The denominator, stated, and the shortfall DERIVED rather than pinned.**
    //
    // Not every recorded remote arm reaches `apply_movement` in this drive, and exactly one thing
    // accounts for the difference -- **not in this seam**, and measured: **the arms shadowed inside
    // their own datagram.** `Presence::pending_movement` is a *single slot* -- `ObjectStream` parks
    // a buffer for the scene to apply next frame, where retail unpacks it immediately -- so when
    // the server puts two `0xF74C` for one object in one datagram, only the second survives.
    // [`the_single_slot_park_keeps_only_the_last_arm_an_object_gets_in_one_datagram`] counts them from the corpus alone,
    // per arm; it is a property of the recording, not of this run.
    //
    // So the expectation below is **arithmetic on the corpus, not a literal read off a run**: per
    // arm, the recorded census minus the shadow census is the most a per-frame consumer of the
    // parked slot can see, and this drive must see exactly that. Two corpus sources decoded by
    // different code could disagree here, per arm.
    //
    // Then, of everything that did reach `apply_movement`, every arm is in the state retail leaves
    // it, counted separately for each of the four so that one working arm cannot carry three
    // broken ones.
    let (recorded, shadowed_by_arm, recorded_total, shadowed_total) = netblob_arm_census();
    let reachable: [usize; 4] = [6usize, 7, 8, 9].map(|a| {
        recorded[a]
            .checked_sub(shadowed_by_arm[a])
            .expect("more arms are shadowed than were recorded")
    });
    eprintln!(
        "remote arms: recorded {:?} minus shadowed {:?} = reachable {reachable:?}; the drive saw {:?}",
        [recorded[6], recorded[7], recorded[8], recorded[9]],
        [shadowed_by_arm[6], shadowed_by_arm[7], shadowed_by_arm[8], shadowed_by_arm[9]],
        [seen[6], seen[7], seen[8], seen[9]]
    );
    assert_eq!(
        [seen[6], seen[7], seen[8], seen[9]],
        reachable,
        "recorded remote arms reaching apply_movement, BY ARM. The expectation is the corpus's own \
         arithmetic -- the recorded census minus the single-slot park's shadow count, per arm -- and \
         not a literal. A mismatch is either a regression in this seam or a change in what the park \
         loses; `the_single_slot_park_keeps_only_the_last_arm_an_object_gets_in_one_datagram` says which."
    );
    assert_eq!(
        seen[6] + seen[7] + seen[8] + seen[9],
        recorded_total - shadowed_total,
        "the four arms must sum to the recorded total minus the shadowed total ({recorded_total} - \
         {shadowed_total})"
    );
    for arm in [6usize, 7, 8, 9] {
        assert!(
            seen[arm] > 0,
            "arm {arm} was never exercised at all, so the tuple above is carrying it"
        );
    }
    // The movement manager's `is_moving_to` method says the manager is *running* the arm, not merely
    // holding its type. Per arm, for the same reason as everything else here.
    //
    // **An arm can already be where it was sent.** A `TurnToObject` whose target nothing in its
    // recording creates does not resolve and falls back to `TurnToHeading` with the heading the
    // buffer carries; when the mover already faces that way (as in `pre-relog-play`, where the
    // fallback heading is 0 and so is the mover's last one), the turn node is satisfied the moment
    // it begins, the manager cleans up inside the same call, and the sample reads `Invalid` and not
    // moving. That is the manager's own already-at-heading exit, not a lost arm, and only a
    // turn-to arm can end that way.
    eprintln!("remote arms: finished inside the call that armed them {done_on_arrival:?}");
    assert!(
        done_on_arrival.iter().all(|(_, _, arm, _)| *arm == 8),
        "an arm other than TurnToObject finished inside the call that armed it: {done_on_arrival:?}"
    );
    assert_eq!(
        (
            moving[6] + arrived[6],
            moving[7] + arrived[7],
            moving[8] + arrived[8],
            moving[9] + arrived[9]
        ),
        (seen[6], seen[7], seen[8], seen[9]),
        "an arm reached the manager and left it not moving-to, BY ARM"
    );
    // ...and every arm that reached `apply_movement` left its `MoveToManager` in the state
    // retail leaves it. (Not every recorded arm: the shadowed ones never arrive -- see the
    // derivation above.)
    assert_eq!(
        (
            right[6] + arrived[6],
            right[7] + arrived[7],
            right[8] + arrived[8],
            right[9] + arrived[9]
        ),
        (seen[6], seen[7], seen[8], seen[9]),
        "remote arms whose movement manager holds the arm the buffer named, BY ARM"
    );
    // The two object-shaped arms, and only those, arm a target: nothing walks or turns toward an
    // object until target handling answers.
    //
    // The arms naming someone other than the player are subtracted and asserted separately by
    // identity below, which also says the resolver armed a target for the stranger arms.
    assert_eq!(
        (targeted[6], targeted[7], targeted[8], targeted[9]),
        (seen[6], 0, seen[8] - named_stranger.len(), 0),
        "remote arms that raised `set_target` for the player, BY ARM"
    );
    assert_eq!(
        named_player[6] + named_player[8] + named_stranger.len(),
        seen[6] + seen[8]
    );
    // The strangers are the fellowship members, world objects and items the census test names; a
    // stranger is never the session's own character.
    eprintln!("remote arms: arms naming someone other than the player {named_stranger:?}");
    // `remote_move_tos_performed` counts the same population the tuple above does, so it is the
    // same subtraction.
    assert_eq!(
        usize::try_from(performed).expect("a count"),
        recorded_total - shadowed_total,
        "stats.remote_move_tos_performed must be the recorded census minus what the single-slot \
         park shadows ({recorded_total} - {shadowed_total})"
    );
}

/// Behaviour: movement.move-to.a-remote-creature-walks-toward-the-destination-the-server-names
/// A remote move-to is recorded, targeted, queued — **and the creature takes steps.**
///
/// Over a window of `long-solo-play`, every sample in which a remote object's `MoveToManager` is
/// moving-to has all four of:
///
/// * `is_moving_to()` — the arm the server named,
/// * `initialized` — target information has arrived,
/// * a queued `pending_actions` node,
/// * and a `current_command` or `aux_command` — the manager saying *"I told this creature to walk
///   or turn"*. Only begin-move-forward and begin-turn-to-heading set those fields non-zero, which
///   is why this is the observable rather than the server's own interpreted state: most arms are
///   turns and never touch `forward_command`.
///
/// Begin-turn-to-heading waits while the interpreter has motions pending, so the interpreter's
/// pending-motion list must drain: `object_step::drive_object_motion` hands each completed motion
/// to the interpreter's completion step (the `AnimEvent::MotionDone` routing). Without it the list
/// only grows and no remote creature is ever issued a command. So the pending list is asserted to
/// be a transient, not a latch, and the commands issued include a walk and a turn.
#[test]
fn a_remote_creature_with_a_destination_walks_toward_it() {
    // The dats are required: they fail when absent.
    let store = store();
    let session = "long-solo-play";
    let records = shared_session(session);
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a net");
    let mut stream = ObjectStream::new();
    let mut entered = false;
    let mut scene: Option<SimWorld> = None;

    // Same window as `movement/remote_stick_to_object.rs`, whose drawn scene cannot hold the whole
    // capture (it exhausts the render device's descriptor heap), so the two measure the same span.
    let mut clock = records.first().map_or(0.0, |r| r.t);
    let end = 165.0f64;
    let mut next = 0usize;
    let mut frames = 0usize;
    // (moving objects seen, of which issued a non-Ready forward or turn command)
    let mut moving_samples = 0usize;
    let mut moving_with_a_command = 0usize;
    let mut idle_samples = 0usize;
    let mut idle_with_a_move_to_command = 0usize;
    let mut moving_initialized = 0usize;
    let mut moving_with_pending = 0usize;
    let mut moving_with_motions_pending = 0usize;
    let mut max_motions_pending = 0usize;
    let mut commands: BTreeSet<MotionCommand> = BTreeSet::new();

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
            scene = Some(world_at(
                &store,
                (u16::from(block.x()) << 8) | u16::from(block.y()),
            ));
        }
        let s = scene.as_mut().expect("built above");
        s.sync_objects(&mut stream).expect("objects sync");
        s.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            now,
            1.0 / 30.0,
        );
        frames += 1;
        let ids: Vec<ObjectId> = stream.presences().map(|(id, _)| id).collect();
        let mut any_moving = false;
        for id in ids {
            if id == player {
                continue;
            }
            let Some((moving, _, _)) = s.ws().server_object_move_to(id) else {
                continue;
            };
            let Some((cur, aux, pending, initialized)) = s.ws().server_object_move_to_state(id)
            else {
                continue;
            };
            // **The observable is the move-to manager's current command, not the interpreted
            // state.** Only begin-move-forward and begin-turn-to-heading set it non-zero, so it is
            // the manager saying *"I told this creature to walk/turn"* -- and most arms are
            // turns, which never touch `forward_command`.
            let issued = cur != MotionCommand::NONE || aux != MotionCommand::NONE;
            let _ = pending;
            if moving {
                any_moving = true;
                moving_samples += 1;
                if initialized {
                    moving_initialized += 1;
                }
                if pending > 0 {
                    moving_with_pending += 1;
                }
                let mp = s.ws().server_object_motions_pending(id).unwrap_or(0);
                if mp > 0 {
                    moving_with_motions_pending += 1;
                }
                max_motions_pending = max_motions_pending.max(mp);
                if issued {
                    moving_with_a_command += 1;
                    if cur != MotionCommand::NONE {
                        commands.insert(cur);
                    }
                    if aux != MotionCommand::NONE {
                        commands.insert(aux);
                    }
                }
            } else {
                idle_samples += 1;
                // **The control.** Move-to cleanup stops whichever of `current_command` and
                // `aux_command` is set, then initializes both back to `NONE`, so an
                // object with no move-to must be holding none. If this is ever non-zero the
                // "issued" count above is measuring something other than the move-to.
                if issued {
                    idle_with_a_move_to_command += 1;
                }
            }
        }
        clock = if any_moving && next < records.len() {
            clock + 1.0 / 30.0
        } else if next < records.len() {
            records[next].t.max(clock + 1.0 / 30.0)
        } else {
            clock + 1.0 / 30.0
        };
    }
    let steps = scene.as_ref().expect("driven").object_steps();
    eprintln!(
        "remote arms: {frames} frames; {moving_samples} move-to samples of which {moving_with_a_command} \
         carried a forward command; {idle_samples} idle samples, {idle_with_a_move_to_command} \
         of those carrying one; commands issued {commands:?}; \
         performed={} targets={} failed={} last_err=0x{:X}",
        steps.remote_move_tos_performed,
        steps.remote_target_updates,
        steps.remote_move_tos_failed,
        steps.remote_last_move_to_error
    );
    eprintln!(
        "remote arms: {moving_initialized} of {moving_samples} move-to samples had a target, \
         {moving_with_pending} had a queued node, {moving_with_motions_pending} had motions pending (max depth {max_motions_pending})"
    );
    // The premise: a move-to really was in flight on this run.
    assert!(
        moving_samples > 0,
        "no remote object was ever moving-to in the window -- no subject"
    );
    assert!(
        steps.remote_move_tos_performed > 0,
        "no arm reached the move-to manager's movement step"
    );
    // Target information reached it, so the manager could take a step at all.
    assert!(
        steps.remote_target_updates > 0,
        "a remote move-to was recorded and never fed a target, so the move-to manager's per-frame \
         `!initialized` guard kept it frozen"
    );
    // Every sample is queued and targeted...
    assert_eq!(
        moving_initialized, moving_samples,
        "a remote move-to was recorded and never fed a target, so the move-to manager's per-frame `!initialized` guard is what is holding it and the explanation below is wrong"
    );
    assert_eq!(
        moving_with_pending, moving_samples,
        "a move-to with no queued node -- `pending_actions` is empty, which is a different \
         failure from the one this test measures"
    );
    // ...and every sample issues a command.
    assert_eq!(
        moving_with_a_command, moving_samples,
        "a remote move-to sample that issued NO command. Turn-to-heading waits on `motions_pending()`, so the first thing to check is whether the \
         `AnimEvent::MotionDone` routing in `object_step::drive_object_motion` is still there -- \
         it is the whole difference between 'the creature is told where to go' and 'it walks'."
    );
    // ...and `motions_pending()` is a transient, not a latch. The bound is a fraction of the
    // measured population rather than a pinned number, so growing the window cannot stale it.
    assert!(
        moving_with_motions_pending * 20 < moving_samples,
        "`pending_motions` is non-empty in {moving_with_motions_pending} of {moving_samples} \
         moving samples -- it has become a latch rather than a transient, which is \
         exactly the state that freezes every remote creature"
    );
    assert!(
        max_motions_pending <= 2,
        "the pending-motion list reached a depth of {max_motions_pending}; without the completion \
         routing it climbs and never comes back down"
    );
    // The premise for both of those: something really did move. A creature that never got a
    // command and never queued a motion would satisfy the two bounds above trivially.
    assert!(
        commands.iter().any(|c| *c == MotionCommand::WALK_FORWARD),
        "no creature was ever told to walk; the commands issued were {commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|c| *c == MotionCommand::TURN_LEFT || *c == MotionCommand::TURN_RIGHT),
        "no creature was ever told to turn, and most of the corpus's arms are turns; \
         the commands issued were {commands:?}"
    );
    assert_eq!(
        idle_with_a_move_to_command, 0,
        "an object with no move-to was holding a move-to command, so the counts above are not \
         measuring the move-to"
    );
    eprintln!(
        "remote arms: {moving_initialized} of {moving_samples} move-to samples had a target, \
         {moving_with_pending} had a queued node, {moving_with_motions_pending} had motions pending (max depth {max_motions_pending})"
    );
}

/// The netblob corpus's own arm census: `(by_arm, shadow_by_arm, arms, shadowed)`.
///
/// **Extracted so the shortfall can be asserted as arithmetic rather than as a literal.**
/// [`the_single_slot_park_keeps_only_the_last_arm_an_object_gets_in_one_datagram`] is the test that *pins* these
/// numbers and explains the mechanism; [`every_recorded_remote_arm_reaches_the_move_to_manager`]
/// subtracts them from the recorded census to predict how many arms a per-frame consumer of the
/// parked slot can possibly see, and that prediction is what it asserts. Both callers read the same
/// function, and the two things it joins come from **different corpus sources decoded by different
/// code**: `by_arm` from the retained netblobs, `seen` from driving the reassembled captures
/// through the client. They could disagree.
fn netblob_arm_census() -> ([usize; 10], [usize; 10], usize, usize) {
    let (mut arms, mut shadowed) = (0usize, 0usize);
    let mut by_arm = [0usize; 10];
    let mut shadow_by_arm = [0usize; 10];
    for session in corpus_sessions() {
        let corpus = Corpus::shared(&session);
        let mut chars: BTreeSet<ObjectId> = BTreeSet::new();
        // (t_rel, mover, movement_type) for every s2c 0xF74C, in wire order.
        let mut rows: Vec<(u64, ObjectId, u8)> = Vec::new();
        for blob in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient)
        {
            // The blob carries its own opcode in the first four bytes.
            let body = &blob.payload[4..];
            if blob.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0 {
                let mut r = dereth_protocol::Reader::new(body);
                if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                    for c in &set.characters {
                        chars.insert(c.gid);
                    }
                }
            } else if blob.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 {
                let mut r = dereth_protocol::Reader::new(body);
                let msg = MovementSetObjectMovement::read(&mut r).expect("a 0xF74C decodes");
                let buf = msg.decoded_movement().expect("its buffer decodes");
                rows.push((blob.t_rel_micros, msg.id, buf.body.movement_type));
            }
        }
        for i in 0..rows.len() {
            let (t, mover, kind) = rows[i];
            if chars.contains(&mover) || kind == movement_type::INVALID {
                continue;
            }
            arms += 1;
            by_arm[kind as usize] += 1;
            // Another 0xF74C for the same object later in the SAME datagram.
            if rows[i + 1..]
                .iter()
                .take_while(|(t2, _, _)| *t2 == t)
                .any(|(_, m, _)| *m == mover)
            {
                shadowed += 1;
                shadow_by_arm[kind as usize] += 1;
            }
        }
    }
    (by_arm, shadow_by_arm, arms, shadowed)
}

/// **Where the shadowed arms go, measured from the recording alone.**
///
/// `ObjectStream` **parks** a movement buffer on the presence and lets the scene apply it on the
/// next `sync_objects`; retail plays it immediately inside its movement update.
/// `Presence::pending_movement` is one slot, so two `0xF74C` for the same object in one
/// **datagram** leave only the second — and the server does exactly that.
///
/// Measured over the retained reassembled netblobs rather than over the independent capture decode,
/// because that is the only corpus source in which a blob is a whole message *and* carries the
/// arrival time of the datagram it came in: two blobs with the same `t_rel` were in the same
/// datagram, so a per-frame consumer of the parked slot sees only the later one.
///
/// This is a deviation of the object stream, not of `apply_movement`, and it bounds what *any*
/// consumer of the parked slot can see. Asserted so that the shortfall in
/// [`every_recorded_remote_arm_reaches_the_move_to_manager`] has a measured mechanism attached,
/// and so the day the park stops losing them a suite says so.
#[test]
fn the_single_slot_park_keeps_only_the_last_arm_an_object_gets_in_one_datagram() {
    let (by_arm, shadow_by_arm, arms, shadowed) = netblob_arm_census();
    eprintln!(
        "remote arms: {shadowed} of {arms} remote arm buffers are shadowed inside their own datagram; \
         by arm {:?} of {:?}",
        [shadow_by_arm[6], shadow_by_arm[7], shadow_by_arm[8], shadow_by_arm[9]],
        [by_arm[6], by_arm[7], by_arm[8], by_arm[9]]
    );
    // The netblob corpus reproduces the reassembly's own census, which is the calibration: two
    // decoders, two corpus sources, one answer.
    let (all, chars) = corpus_buffers();
    let mut reassembled = [0usize; 10];
    for b in all.iter().filter(|b| !chars[&b.session].contains(&b.mover)) {
        reassembled[b.body.movement_type as usize] += 1;
    }
    assert_eq!(
        (by_arm[6], by_arm[7], by_arm[8], by_arm[9]),
        (
            reassembled[6],
            reassembled[7],
            reassembled[8],
            reassembled[9]
        ),
        "the netblob corpus disagrees with the reassembled one about the arm census"
    );
    // The server does send two arms for one object in one datagram, and the park keeps only the
    // second. It never loses a MoveToObject or a TurnToHeading that way.
    assert!(
        shadowed > 0,
        "no remote arm buffer is shadowed by a later 0xF74C in the same datagram"
    );
    assert_eq!(
        (shadow_by_arm[6], shadow_by_arm[9]),
        (0, 0),
        "the single-slot park loses a MoveToObject or a TurnToHeading"
    );
    assert!(arms >= shadowed, "more shadowed arms than arms");
}
