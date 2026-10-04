//! A combat-stance change *stops* the character, and a **held key** starts him running again when
//! the stance change completes; an auto-run is cancelled by the change and does not resume.
//! Fixture: the retained `combat-mode-while-moving` recording (read in place, not in the promoted
//! corpus), its four `0x0053 Combat_ChangeCombatMode` edges and the server's four non-autonomous
//! `0xF74C Movement_SetObjectMovement` answers, driven into a `Character` on Holtburg terrain.
//!
//! # What the recording shows
//!
//! Both transitions are `NonCombat -> Magic -> NonCombat`. The client sends **nothing else** on the
//! edge — no `0xF61C Movement_MoveToState` at all — and the server's answer arrives 8 to 16 ms
//! later carrying `autonomous = false`, an incremented `server_control_timestamp`, the new
//! `current_style`, and **`forward_command = Ready`**. That last field is the stop, and it is
//! authored by the server, not by the client.
//!
//! The `0xF753 Movement_AutonomousPosition` stream gives ground speed between consecutive reports
//! in the same cell, in m/s:
//!
//! ```text
//! auto-run    19.19 -> 20.19  2.670   (0x0053 Magic at 19.49, mid-interval)
//!             20.19 -> 23.09  0.002   <-- stopped dead, 2.9 s, and NEVER resumes
//!             (the next motion at 24.30 is a fresh WalkBackwards keypress)
//!
//! held W      28.18 -> 29.18  5.725   (running)
//!             29.18 -> 30.19  4.000   (0x0053 Magic at 29.73, mid-interval)
//!             30.19 -> 31.19  1.443   <-- the stop
//!             31.19 -> 32.19  5.856   <-- RESUMED, still in Magic, key never released
//!             32.19 -> 33.19  5.857
//!             33.19 -> 34.19  0.149   (0x0053 NonCombat at 33.06 stops him again)
//!             34.52 -> 36.03  2.286   (resumed again; W released at 35.02)
//! ```
//!
//! The client's own `0xF61C` stream says the key was held throughout: `WalkForward` was last sent
//! at **27.64** and the next client movement message is **35.02**, `Ready`.
//!
//! # The original mechanism, end to end
//!
//! ```text
//! dispatch case 0xF74C
//!   accepted = apply object movement(physics, object, buffer, size)
//!              ... last_move_was_autonomous = autonomous; unpack_movement(...)
//!              ... returns 1 only for the player
//!   if accepted, hand movement control to the server
//!
//! hand movement control to the server
//!   controlled_by_server = 1
//!   cancel auto-run
//!   finish any jump
//!
//! per-frame command-interpreter update
//!   if (player && enabled && controlled_by_server
//!       && !motions_pending(player) && !is_moving_to(player)) {
//!       if (Substate.head==0 && Turn.head==0 && Sidestep.head==0 && !auto_run) return;
//!       take movement control from the server
//!   }
//!
//! take movement control from the server
//!   controlled_by_server = 0
//!   player->last_move_was_autonomous = 1
//!   stop the player completely; stop interpolation
//!   restore the held-run state
//!   apply the current movement lists
//! ```
//!
//! **Cancelling auto-run while handing movement control to the server is the whole auto-run /
//! held-key split.** The run lock is cancelled; the three command lists are not touched. The
//! per-frame update therefore returns early for an auto-runner — nothing left to re-issue, control
//! is never taken back, the character stays where the server stopped him — while a held `W` is
//! still on the substate list, control *is* taken back, and applying the current movement lists
//! re-issues `WalkForward` with its start flag set.
//!
//! **`motions_pending` is the delay**: the retake waits for the stance-change animation to drain,
//! so the stop is not instantaneous. Combat-readiness checks read the same predicate.
//!
//! The stop and the resume are asserted **separately**: `Character::apply_input` is edge-triggered
//! on `input == applied`, so a build that applies the server's `Ready` but never retakes control
//! stops the body and never resumes it, and a test of the stop alone would pass over that.
//!
//! # Oracles
//!
//! | claim | oracle |
//! |---|---|
//! | what the client sends on the toggle edge | the retained `combat-mode-while-moving` recording, reassembled by `dereth-transport`'s own `ParsedPacket`/`Fragment` |
//! | what the server answers with | the same file's four non-autonomous `0xF74C` for the player |
//! | the control-transfer chain | the original's dispatch, loss, per-frame update, and retake operations |
//! | the stance indices | the original `command_ids` table: 61 = `NonCombat`, 73 = `Magic` |
//!
//! Every path is an `expect`. **It never skips.**

use super::common::client_dir;
use super::common::workspace_root_buf as workspace_root;
use dereth_client::world::SceneWrites;

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_client::character::{Character, CharacterInput, MovementCommands};
use dereth_client::net::ClientNetwork;
use dereth_client::world::DEFAULT_LANDBLOCK;
use dereth_client_net::client_session::testing::{shared_session, Datagram};
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::actions::movement::{action, on_action};
use dereth_dat::RetailDatStore;
use dereth_input::ActionId;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::movement::{MovementBuffer, MovementSetObjectMovement};
use dereth_protocol::{Message, Opcode};
use dereth_transport::wire::ParsedPacket;

/// The middle of Holtburg's own landblock.
const SPAWN: (f32, f32) = (96.0, 96.0);

/// `command_ids[61]` — `NonCombat`. Pinned as a literal index *and* as a literal command, because
/// a table read through the same accessor it is written through cannot detect a wrong entry.
const STYLE_NONCOMBAT: u16 = 61;
/// `command_ids[73]` — `Magic`, the stance toggled into in both recorded transitions.
const STYLE_MAGIC: u16 = 73;
/// `command_ids[3]` — `Ready`, the forward command the server's answer carries. This is the stop.
const READY_INDEX: u16 = 3;

/// `0x0053 Combat_ChangeCombatMode`.
const COMBAT_CHANGE_COMBAT_MODE: u32 = 0x0053;
/// `0xF61C Movement_MoveToState` — the client's own movement message.
const MOVE_TO_STATE: u32 = 0xF61C;
/// `0xF7B1`, the ordered game-action header. The real type is the dword after the sequence.
const GAME_ACTION: u32 = 0xF7B1;

/// `COMBAT_MODE`: `NONCOMBAT` is 1 and `MAGIC` is 8. Read from `enums.tsv` rather than inferred
/// from the recording, so a wrong recording could disagree with it.
const NONCOMBAT_COMBAT_MODE: u32 = 1;
const MAGIC_COMBAT_MODE: u32 = 8;

// ---------------------------------------------------------------------------------------------
// 1. The retained `combat-mode-while-moving` recording, read in place.
// ---------------------------------------------------------------------------------------------

/// `combat-mode-while-moving` is read in place from the capture folder: it is not in the
/// promoted corpus, whose every count its promotion would change.
fn load() -> &'static [Datagram] {
    let p = workspace_root().join("fixtures/packet-captures/combat-mode-while-moving.jsonl");
    assert!(
        p.is_file(),
        "combat-mode-while-moving is this test's oracle; it is \
         absent at {p:?}"
    );
    shared_session("combat-mode-while-moving")
}

/// One reassembled blob from the **client** side, with the wall-clock time of its last fragment.
///
/// The server side goes through `ClientNetwork`, which is the shipped reassembly. The client side has
/// no such consumer in this workspace — nothing here is a server — so it is reassembled here over
/// `dereth_client_net`'s own `ParsedPacket`/`Fragment`, which are the same decoders `ClientNetwork` uses one
/// layer down. `the_two_reassemblies_agree_on_the_server_side` calibrates it: run against the
/// **server** stream it must produce exactly what `ClientNetwork` produces, or neither number is
/// evidence.
struct Blob {
    t: f64,
    opcode: u32,
    payload: Vec<u8>,
}

fn reassemble(records: &[Datagram], c2s: bool) -> Vec<Blob> {
    let mut pending: BTreeMap<u64, (u16, BTreeMap<u16, Vec<u8>>)> = BTreeMap::new();
    let mut out = Vec::new();
    for r in records.iter().filter(|r| r.c2s == c2s) {
        let Ok(p) = ParsedPacket::parse(&r.raw) else {
            continue;
        };
        for f in &p.fragments {
            let slot = pending
                .entry(f.header.blob_id())
                .or_insert_with(|| (f.header.num_frags, BTreeMap::new()));
            slot.0 = f.header.num_frags;
            slot.1.insert(f.header.blob_num, f.payload.clone());
            if slot.1.len() != usize::from(slot.0) {
                continue;
            }
            let payload: Vec<u8> = slot.1.values().flatten().copied().collect();
            pending.remove(&f.header.blob_id());
            assert!(
                payload.len() >= 4,
                "a reassembled blob is shorter than its opcode"
            );
            let opcode = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
            out.push(Blob {
                t: r.t,
                opcode,
                payload,
            });
        }
    }
    assert!(
        pending.is_empty(),
        "combat-mode-while-moving is a clean recording; no blob may be left incomplete"
    );
    out
}

/// Every ordered client-to-server game action, as `(t, sub_opcode, body)`.
fn client_actions(records: &[Datagram]) -> Vec<(f64, u32, Vec<u8>)> {
    reassemble(records, true)
        .into_iter()
        .filter(|b| b.opcode == GAME_ACTION)
        .map(|b| {
            let sub =
                u32::from_le_bytes([b.payload[8], b.payload[9], b.payload[10], b.payload[11]]);
            (b.t, sub, b.payload[12..].to_vec())
        })
        .collect()
}

/// Everything the transport hands up from the **server** stream, in arrival order.
fn server_messages(records: &[Datagram]) -> Vec<(u32, Vec<u8>)> {
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net =
        ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a replay net");
    for r in records.iter().filter(|r| !r.c2s) {
        net.feed(&r.raw, r.peer(), LocalTime(r.t));
    }
    let mut out = Vec::new();
    while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
        out.push((m.opcode, m.body.clone()));
    }
    out
}

/// The session's own character id, from `0xF746 Login_CreatePlayer` — the message that names the
/// body this client is driving. The transport hands the body up **without** its opcode, so the id
/// is at offset 0.
fn player_id(records: &[Datagram]) -> ObjectId {
    for (op, body) in server_messages(records) {
        if op == Opcode::LOGIN_CREATE_PLAYER.0 {
            let id = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            return ObjectId(id);
        }
    }
    panic!("combat-mode-while-moving must carry a Login_CreatePlayer");
}

/// The **four** server answers to the four `0x0053 Combat_ChangeCombatMode` toggles.
///
/// Paired by arrival time, not by shape. Nine non-autonomous `0xF74C` in this recording carry a
/// stance word and `forward_command = Ready`: four answer a mode toggle and five answer a pickup.
/// Picking them by shape would silently take all nine — a census of the wrong space returns a
/// confident number — so each toggle claims the first such buffer that arrives
/// **after** it, and the pairing itself is asserted: four toggles, four answers, each within
/// 100 ms, and the two sets disjoint.
fn stance_answers(records: &[Datagram]) -> Vec<(f64, MovementBuffer)> {
    let me = player_id(records);
    let mut server: Vec<(f64, MovementBuffer)> = Vec::new();
    for b in reassemble(records, false) {
        if b.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 {
            continue;
        }
        let mut r = dereth_protocol::Reader::new(&b.payload[4..]);
        let msg = MovementSetObjectMovement::read(&mut r).expect("a recorded 0xF74C decodes");
        if msg.id != me {
            continue;
        }
        let buf = msg.decoded_movement().expect("its movement buffer decodes");
        if buf.autonomous {
            continue;
        }
        server.push((b.t, buf));
    }
    assert!(
        !server.is_empty(),
        "the player is sent server-authored movement in combat-mode-while-moving"
    );

    let mut out: Vec<(f64, MovementBuffer)> = Vec::new();
    for (t, _) in toggles(records) {
        let (at, buf) = server
            .iter()
            .find(|(bt, _)| *bt > t)
            .unwrap_or_else(|| panic!("no server movement answers the toggle at {t}"));
        assert!(
            at - t < 0.100,
            "the answer to the toggle at {t} arrived {at}, more than 100 ms later"
        );
        out.push((*at, buf.clone()));
    }
    assert_eq!(out.len(), 4, "one answer per toggle");
    let times: Vec<f64> = out.iter().map(|(t, _)| *t).collect();
    for i in 1..times.len() {
        assert!(
            times[i] > times[i - 1],
            "the four answers are four distinct buffers"
        );
    }
    out
}

/// The four `0x0053 Combat_ChangeCombatMode` the client sent, as `(t, mode)`.
fn toggles(records: &[Datagram]) -> Vec<(f64, u32)> {
    client_actions(records)
        .into_iter()
        .filter(|(_, sub, _)| *sub == COMBAT_CHANGE_COMBAT_MODE)
        .map(|(t, _, body)| (t, u32::from_le_bytes([body[0], body[1], body[2], body[3]])))
        .collect()
}

/// `world.rs::apply_player_movement`'s private wire -> runtime conversion, mirrored.
///
/// It is `fn interpreted_state` in `world.rs` and is not public; this is the same body. It is
/// **pinned against literals** in `the_conversion_is_the_one_the_frame_uses` rather than trusted,
/// because a mirrored converter is a second implementation. The literal pin keeps the oracle
/// independent of the mirrored converter.
fn interpreted_state(
    wire: &dereth_protocol::movement::InterpretedMotionState,
) -> dereth_animation::motion::InterpretedMotionState {
    use dereth_animation::motion::{ActionNode, InterpretedMotionState};
    let cmd = |i: Option<u16>, fallback: MotionCommand| {
        i.and_then(MotionCommand::from_index).unwrap_or(fallback)
    };
    let base = InterpretedMotionState::default();
    InterpretedMotionState {
        current_style: cmd(wire.current_style, base.current_style),
        forward_command: cmd(wire.forward_command, base.forward_command),
        forward_speed: wire.forward_speed.unwrap_or(base.forward_speed),
        sidestep_command: cmd(wire.sidestep_command, base.sidestep_command),
        sidestep_speed: wire.sidestep_speed.unwrap_or(base.sidestep_speed),
        turn_command: cmd(wire.turn_command, base.turn_command),
        turn_speed: wire.turn_speed.unwrap_or(base.turn_speed),
        actions: wire
            .actions
            .iter()
            .filter_map(|a| {
                Some(ActionNode {
                    action: MotionCommand::from_index(a.command_index)?,
                    speed: a.speed,
                    stamp: u32::from(a.stamp()),
                    autonomous: a.autonomous(),
                })
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The census. Every number the stations rest on, measured here first.
// ---------------------------------------------------------------------------------------------

/// The reassembly calibration: run the hand loop over the **server** stream and require it to
/// agree with `ClientNetwork`, which is the shipped implementation.
#[test]
fn the_two_reassemblies_agree_on_the_server_side() {
    let records = load();
    let mine = reassemble(&records, false);
    let theirs = server_messages(&records);
    assert!(
        !mine.is_empty(),
        "no server-to-client blob in combat-mode-while-moving"
    );
    assert_eq!(
        mine.len(),
        theirs.len(),
        "manual reassembly and the network client's delivery path must produce the same blob count, or neither the \
         client-side census nor the server-side one is evidence"
    );
    // As **sets**, not in order: `ClientNetwork` delivers through the three receive queues, so an
    // unordered blob can overtake an ordered one that arrived earlier. What must agree is which
    // blobs exist and what is in them.
    let mut a: Vec<(u32, Vec<u8>)> = mine
        .iter()
        .map(|b| (b.opcode, b.payload[4..].to_vec()))
        .collect();
    let mut b: Vec<(u32, Vec<u8>)> = theirs.clone();
    a.sort();
    b.sort();
    assert_eq!(
        a, b,
        "the same blobs, with the same bodies, from both reassemblies"
    );
    // And a positive on the side actually being measured, so a c2s loop that silently found
    // nothing cannot read as a clean zero.
    let c2s = reassemble(&records, true);
    assert!(
        !c2s.is_empty(),
        "no client-to-server blob in combat-mode-while-moving"
    );
}

/// **The whole client-side edge, with denominators.**
#[test]
fn the_capture_records_both_transitions() {
    let records = load();
    let actions = client_actions(&records);
    assert!(
        !actions.is_empty(),
        "no ordered client game action in combat-mode-while-moving"
    );

    let toggles = toggles(&records);
    assert_eq!(
        toggles.len(),
        4,
        "Combat_ChangeCombatMode messages in combat-mode-while-moving"
    );
    assert_eq!(
        toggles.iter().map(|(_, m)| *m).collect::<Vec<_>>(),
        vec![
            MAGIC_COMBAT_MODE,
            NONCOMBAT_COMBAT_MODE,
            MAGIC_COMBAT_MODE,
            NONCOMBAT_COMBAT_MODE
        ],
        "two NonCombat -> Magic -> NonCombat cycles"
    );

    // **What the client sends on the toggle edge: nothing else.** Not one `0xF61C` in the 300 ms
    // after any of the four toggles, which is more than an order of magnitude longer than the
    // 8-16 ms the server took to answer. The character is stopped without the client saying so.
    let moves: Vec<f64> = actions
        .iter()
        .filter(|(_, sub, _)| *sub == MOVE_TO_STATE)
        .map(|(t, _, _)| *t)
        .collect();
    assert!(
        !moves.is_empty(),
        "no client movement message in combat-mode-while-moving"
    );
    for (t, _) in &toggles {
        let near = moves.iter().filter(|m| (*m - t).abs() < 0.300).count();
        assert_eq!(
            near, 0,
            "no client movement message within 300 ms of the toggle at {t}"
        );
    }

    // The key was still held across the second transition: `WalkForward` at 27.639 and the next
    // client movement message at 35.024, with both toggles (29.727, 33.061) inside that gap.
    let held_from = moves
        .iter()
        .copied()
        .find(|m| (*m - 27.639).abs() < 0.01)
        .expect("27.639");
    let released = moves
        .iter()
        .copied()
        .find(|m| (*m - 35.024).abs() < 0.01)
        .expect("35.024");
    assert!(
        toggles[2].0 > held_from && toggles[3].0 < released,
        "the second transition is entirely inside one press of W"
    );

    // **The server's answer.** Four buffers, all server-authored, alternating stance.
    let answers = stance_answers(&records);
    let bufs: Vec<MovementBuffer> = answers.iter().map(|(_, b)| b.clone()).collect();
    for ((t, _), (at, _)) in toggles.iter().zip(answers.iter()) {
        assert!(
            at - t > 0.0 && at - t < 0.100,
            "the server answered the toggle at {t} in {} ms",
            (at - t) * 1000.0
        );
    }
    assert_eq!(
        bufs.iter()
            .map(|b| b.body.current_style)
            .collect::<Vec<_>>(),
        vec![STYLE_MAGIC, STYLE_NONCOMBAT, STYLE_MAGIC, STYLE_NONCOMBAT],
        "the stance word of each answer, as a command_ids index"
    );
    for b in &bufs {
        assert!(
            !b.autonomous,
            "a stance answer is server-authored, never an echo"
        );
        let s = b
            .body
            .interpreted
            .as_ref()
            .expect("case 0 carries an interpreted state");
        assert_eq!(
            s.forward_command,
            Some(READY_INDEX),
            "forward_command = Ready is the stop"
        );
        assert_eq!(
            s.forward_speed, None,
            "and no forward speed travels with it"
        );
        assert_eq!(
            s.current_style,
            Some(b.body.current_style),
            "style word and state agree"
        );
    }
    // `server_control_timestamp` advances on every one of them, which is what makes each a
    // distinct grant of server control rather than one repeated.
    let stamps: Vec<u16> = bufs.iter().map(|b| b.server_control_timestamp).collect();
    assert_eq!(
        stamps,
        vec![1, 2, 3, 4],
        "each answer takes the next server-control stamp"
    );
}

/// The mirrored converter, pinned against the exact `command_ids` constants consumed by the frame.
#[test]
fn the_conversion_is_the_one_the_frame_uses() {
    assert_eq!(
        MotionCommand::from_index(STYLE_NONCOMBAT),
        Some(MotionCommand(0x8000_003D))
    );
    assert_eq!(
        MotionCommand::from_index(STYLE_MAGIC),
        Some(MotionCommand(0x8000_0049))
    );
    assert_eq!(
        MotionCommand::from_index(READY_INDEX),
        Some(MotionCommand(0x4100_0003))
    );
    assert_eq!(
        MotionCommand::from_index(STYLE_NONCOMBAT),
        Some(MotionCommand::NON_COMBAT)
    );
    assert_eq!(
        MotionCommand::from_index(READY_INDEX),
        Some(MotionCommand::READY)
    );

    let records = load();
    let bufs: Vec<MovementBuffer> = stance_answers(&records)
        .into_iter()
        .map(|(_, b)| b)
        .collect();
    let magic = interpreted_state(bufs[0].body.interpreted.as_ref().expect("interpreted"));
    assert_eq!(magic.current_style, MotionCommand(0x8000_0049), "Magic");
    assert_eq!(magic.forward_command, MotionCommand(0x4100_0003), "Ready");
    let noncombat = interpreted_state(bufs[1].body.interpreted.as_ref().expect("interpreted"));
    assert_eq!(
        noncombat.current_style,
        MotionCommand(0x8000_003D),
        "NonCombat"
    );
    assert_ne!(
        magic.current_style, noncombat.current_style,
        "the two answers differ"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The three stations. A real body on Holtburg's terrain and a real `MovementCommands`.
// ---------------------------------------------------------------------------------------------

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn settled_character(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let mut c =
        Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the character is created");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle before anything is measured"
    );
    c
}

fn ev(a: ActionId, start: bool) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: a,
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 0,
    }
}

/// The stance change, applied to the body exactly as `world.rs::apply_player_movement` applies it:
/// the pre-switch style word applied before the switch and then `case 0`'s
/// `move_to_interpreted_state`. Nothing is synthesised; `buf` came off disk.
fn apply_server_stance(c: &Character, buf: &MovementBuffer) {
    let state = buf.body.interpreted.as_ref().map(interpreted_state);
    let style = MotionCommand::from_index(buf.body.current_style)
        .or_else(|| state.as_ref().map(|s| s.current_style));
    if let Some(style) = style {
        c.driver_mut().apply_movement_style(style);
    }
    if let Some(state) = state.as_ref() {
        c.driver_mut().move_to_interpreted_state(state, true);
    }
}

/// A running body is above this; a stopped one is below the floor. Both are stated rather than
/// implied, because a count produced by a threshold is not a fact until the threshold is stated.
/// The recording's own numbers are 5.86 m/s running, 1.44 m/s across the second
/// containing the stop, and 0.002 m/s once stopped.
const RUNNING: f32 = 3.0;
const STOPPED: f32 = 0.5;

/// One continuous driven run over the frame the application runs: the per-frame retake, then the
/// body's own update, thirty times a second.
///
/// Driving it in one piece is the point: the **stop** and the **resume** are two windows of the
/// *same* run with the retake between them, so neither can be a different body, a different setup
/// or a different oracle.
struct Run {
    /// The frame the per-frame update handed control back on, if it ever did.
    retaken_at: Option<u32>,
    /// How many times it handed control back. More than one would mean the flag is oscillating.
    retakes: u32,
    /// The body's ground position at the end of each frame.
    at: Vec<(f32, f32)>,
}

fn drive(
    c: &mut Character,
    mc: &mut MovementCommands,
    input: &mut CharacterInput,
    from: u32,
    frames: u32,
) -> Run {
    let mut r = Run {
        retaken_at: None,
        retakes: 0,
        at: Vec::with_capacity(frames as usize),
    };
    for i in 0..frames {
        // The command-interpreter update runs ahead of the physics step performed by
        // `Character::update`.
        let d = c.driver();
        let (pending, moving) = (d.movement.motions_pending(), d.movement.is_moving_to());
        drop(d);
        if mc.use_time(pending, moving, input) {
            r.retakes += 1;
            if r.retaken_at.is_none() {
                r.retaken_at = Some(i);
            }
            c.take_control_from_server();
        }
        c.input = *input;
        c.update(LocalTime(f64::from(from + i + 1) / 30.0));
        let o = c.position().frame.origin;
        r.at.push((o.x, o.y));
    }
    r
}

/// Ground speed in m/s over frames `[a, b)` of a [`Run`], at 30 Hz.
fn over(r: &Run, a: u32, b: u32) -> f32 {
    let (a, b) = (a as usize, b as usize);
    assert!(
        b > a && b <= r.at.len(),
        "window {a}..{b} is outside the {}-frame run",
        r.at.len()
    );
    let (p, q) = (r.at[a], r.at[b - 1]);
    let dist = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
    dist / ((b - a) as f32 / 30.0)
}

/// Everything the three stations share: a settled body on Holtburg with the shipped *Run as
/// Default Movement* option on, plus the interpreter that will drive it.
fn running_body() -> (Character, MovementCommands, CharacterInput) {
    let store = store();
    let c = settled_character(&store);
    let mut mc = MovementCommands::default();
    let mut input = CharacterInput::default();
    // **Run, not walk.** *Run as Default Movement* (ordinal 10, bit `0x0400`) is on for every
    // shipped character. The original hold-run update XORs the physical key against it, so with
    // the key *up* the character runs. Without this the body walks at 2.41 m/s and the tests
    // would be comparing a walk against the recording's 5.86 m/s run.
    mc.ui_toggles_run = true;
    assert!(mc.on_action(
        on_action(&ev(action::TOGGLE_RUN_WALK, false), |_| None),
        &mut input
    ));
    assert!(input.run, "run is the default state and the key walks you");
    (c, mc, input)
}

/// The recorded server answer to toggle `n` (0-based), off disk.
fn answer(n: usize) -> MovementBuffer {
    stance_answers(&load()).remove(n).1
}

/// **Station 1 — auto-run.** The stance change stops him and he never starts again.
///
/// Handing control to the server cancels auto-run, so the per-frame update's inner `if` finds
/// three empty lists and no run lock and returns without retaking control. The recording says the
/// same thing: 0.002 m/s for 2.9 s, through the toggle back to NonCombat, until a **new** key.
#[test]
fn auto_run_is_cancelled_by_the_stance_change_and_does_not_resume() {
    let (mut c, mut mc, mut input) = running_body();

    // `0x30 Autorun`, through the real decode.
    assert!(mc.on_action(on_action(&ev(action::AUTORUN, true), |_| None), &mut input));
    assert!(mc.lists.auto_run, "the run lock is on");
    assert!(input.forward, "auto-run sets the forward input");
    assert_eq!(
        mc.take_notices(),
        vec!["AutoRun ON"],
        "auto-run's own message-window line, drained here so the one below is the stance change's"
    );
    c.input = input;

    // **The premise.** Without it every assertion below is satisfied by a body that never moved.
    let up = drive(&mut c, &mut mc, &mut input, 60, 90);
    assert_eq!(
        up.retakes, 0,
        "nothing has taken control yet, so nothing can hand it back"
    );
    let before = over(&up, 60, 90);
    assert!(
        before > RUNNING,
        "the auto-runner must actually be running, got {before} m/s"
    );

    // The server's own recorded answer to the first toggle, applied the way the frame applies it.
    apply_server_stance(&c, &answer(0));
    mc.lose_control_to_server(&mut input);

    // **The stop**, asserted on its own.
    assert!(
        mc.lists.controlled_by_server,
        "the server-control flag is set"
    );
    assert!(
        !mc.lists.auto_run,
        "Original auto-run clearing cancelled the run lock"
    );
    assert!(
        !input.auto_run,
        "and the projection of it, which is what `char_input()` reports"
    );
    assert_eq!(
        mc.take_notices(),
        vec!["AutoRun OFF"],
        "the auto-run message-window line is sent even with sendEvent = 0"
    );
    assert_eq!(
        c.driver().movement.interp.interpreted_state.current_style,
        MotionCommand(0x8000_0049),
        "and the stance really changed to Magic"
    );

    // **The resume, asserted separately — and there must not be one.**
    let run = drive(&mut c, &mut mc, &mut input, 150, 150);
    assert_eq!(
        run.retakes, 0,
        "the per-frame update must not retake control for a cancelled run lock"
    );
    assert_eq!(run.retaken_at, None);
    assert!(
        mc.lists.controlled_by_server,
        "so the interpreter stays server-controlled"
    );
    let settling = over(&run, 0, 30);
    let stopped = over(&run, 60, 150);
    assert!(
        stopped < STOPPED,
        "auto-run never resumes, got {stopped} m/s over three seconds"
    );
    assert!(
        settling > stopped,
        "and the stop is a deceleration, not an instant freeze: {settling} m/s in the first second \
         against {stopped} m/s afterwards"
    );
}

/// Behaviour: movement.stance-change.stops-the-body-and-a-held-key-resumes-running
/// **Station 2 — the key still held.** The stance change stops him and he starts again by himself.
///
/// This is the half that needs the per-frame retake of movement control.
#[test]
fn a_held_key_survives_the_stance_change_and_resumes_when_it_completes() {
    let (mut c, mut mc, mut input) = running_body();

    // `0x29 MoveForward` down, and **never released** for the rest of this test.
    assert!(mc.on_action(
        on_action(&ev(action::MOVE_FORWARD, true), |_| None),
        &mut input
    ));
    assert_eq!(
        mc.lists.substate.len(),
        1,
        "WalkForward is on the substate list"
    );
    assert!(
        !mc.lists.auto_run,
        "and this is a held key, not the run lock"
    );
    assert!(input.forward);
    c.input = input;

    let up = drive(&mut c, &mut mc, &mut input, 60, 90);
    assert_eq!(up.retakes, 0);
    let before = over(&up, 60, 90);
    assert!(
        before > RUNNING,
        "the premise: he must be running, got {before} m/s"
    );

    apply_server_stance(&c, &answer(2));
    mc.lose_control_to_server(&mut input);
    assert!(mc.lists.controlled_by_server);
    assert_eq!(
        mc.take_notices(),
        Vec::<&'static str>::new(),
        "no run lock, so no AutoRun line"
    );
    assert_eq!(
        mc.lists.substate.len(),
        1,
        "the held-key substate entry survives server control"
    );

    // One continuous run across the stop **and** the resume, with the retake in the middle.
    let run = drive(&mut c, &mut mc, &mut input, 150, 150);

    // The key was never released.
    assert!(input.forward, "the key is still held throughout");
    assert_eq!(mc.lists.substate.len(), 1);

    // **Not instantaneous.** The per-frame update waits for pending motions to drain, including
    // the stance animation. If the retake landed on frame 0 the whole gate would be inert here.
    assert_eq!(run.retakes, 1, "control is taken back exactly once");
    let k = run.retaken_at.expect("control is taken back");
    assert!(
        k >= 15,
        "the retake waits for the stance animation; it came on frame {k}"
    );
    assert!(
        !mc.lists.controlled_by_server,
        "and the interpreter is autonomous again"
    );
    // **The invariant the whole gate rests on**: the movement interpreter's pending motions and
    // the motion-table queue drain together. A leak in either jams `motions_pending()` true for ever, and the retake --
    // and the combat-readiness gate's `!motions_pending` with it -- never opens again.
    assert!(
        c.driver().movement.interp.pending_motions.is_empty()
            && c.driver().motion_table.pending().is_empty(),
        "both motion queues drain: interp {:?}, table {:?}",
        c.driver().movement.interp.pending_motions,
        c.driver().motion_table.pending()
    );

    // **The stop**, in the half second immediately before the retake, and **the resume**, in the
    // last second of the same run. Two windows of one run, split at the retake.
    let stopped = over(&run, k - 15, k);
    let resumed = over(&run, 120, 150);
    assert!(
        stopped < STOPPED,
        "the stance change stopped him, got {stopped} m/s"
    );
    assert!(
        resumed > RUNNING,
        "a held key resumes movement by itself, got {resumed} m/s"
    );
    assert_eq!(
        c.driver().movement.interp.interpreted_state.current_style,
        MotionCommand(0x8000_0049),
        "and he is still in Magic stance while running"
    );
}

/// **Station 3 — neither.** No key, no run lock: the stance changes and nothing moves, before or
/// after. The discriminator for the other two, and the reason neither of them is measuring a body
/// that simply drifts.
#[test]
fn with_no_movement_held_the_stance_change_moves_nothing() {
    let (mut c, mut mc, mut input) = running_body();
    c.input = input;

    let up = drive(&mut c, &mut mc, &mut input, 60, 90);
    assert_eq!(up.retakes, 0);
    let before = over(&up, 60, 90);
    assert!(
        before < STOPPED,
        "a body with no input stands still, got {before} m/s"
    );

    apply_server_stance(&c, &answer(0));
    mc.lose_control_to_server(&mut input);
    assert!(mc.lists.controlled_by_server);
    assert_eq!(
        mc.take_notices(),
        Vec::<&'static str>::new(),
        "the run lock was already off"
    );

    let run = drive(&mut c, &mut mc, &mut input, 150, 150);
    assert_eq!(
        run.retakes, 0,
        "nothing to re-issue, so control is never taken back"
    );
    assert!(mc.lists.controlled_by_server);
    let after = over(&run, 0, 150);
    assert!(after < STOPPED, "and he never starts, got {after} m/s");
    assert_eq!(
        c.driver().movement.interp.interpreted_state.current_style,
        MotionCommand(0x8000_0049),
        "the stance still changed: this station is not a dead body"
    );
}

/// The per-frame retake gate itself, at the four corners, without a body — so a station that failed for a
/// physics reason cannot be mistaken for the gate being wrong.
#[test]
fn stance_resume_requires_all_four_conditions() {
    use dereth_client_runtime::actions::movement::{CommandEntry, CommandLists};
    let entry = |command: u32| CommandEntry {
        command,
        speed: 1.0,
        head_is_mouse: false,
        hold_run: false,
    };

    let mut l = CommandLists::default();
    assert_eq!(
        l.autonomy_level, 2,
        "the original constructor's literal value"
    );
    // Not server-controlled: the gate cannot fire whatever else is true.
    l.substate.push(entry(0x4500_0005));
    assert!(!l.can_take_control_from_server(false, false));

    // Server-controlled with a held key: fires, but only once the motions have drained.
    assert_eq!(
        l.lose_control_to_server(),
        None,
        "no run lock to cancel, so no message"
    );
    assert!(l.controlled_by_server);
    assert!(
        !l.can_take_control_from_server(true, false),
        "motions_pending holds it off"
    );
    assert!(
        !l.can_take_control_from_server(false, true),
        "and so does move-to activity"
    );
    assert!(l.can_take_control_from_server(false, false));
    assert!(l.take_control_from_server());
    assert!(!l.controlled_by_server);
    assert!(!l.take_control_from_server(), "and it is idempotent");

    // Server-controlled with nothing held: never fires.
    let mut l = CommandLists::default();
    assert_eq!(l.lose_control_to_server(), None);
    assert!(
        !l.can_take_control_from_server(false, false),
        "empty lists and no run lock"
    );

    // The auto-run corner, which is the claim: the lock is what would have kept the gate open, and
    // handing control to the server is what closes it. The two arms are set up identically except for
    // whether the cancel runs, so nothing but the cancel can explain the difference.
    let mut l = CommandLists::default();
    assert_eq!(l.set_auto_run(true), Some("AutoRun ON"));
    l.controlled_by_server = true;
    assert!(
        l.can_take_control_from_server(false, false),
        "with the lock still on it fires"
    );
    let mut l = CommandLists::default();
    assert_eq!(l.set_auto_run(true), Some("AutoRun ON"));
    assert_eq!(l.lose_control_to_server(), Some("AutoRun OFF"));
    assert!(
        l.controlled_by_server,
        "the same flag the arm above set by hand"
    );
    assert!(!l.auto_run);
    assert!(
        !l.can_take_control_from_server(false, false),
        "and now it never will"
    );

    // Autonomy level 0 disables both halves outright in the original control flow.
    let mut l = CommandLists {
        autonomy_level: 0,
        ..CommandLists::default()
    };
    assert_eq!(l.lose_control_to_server(), None);
    assert!(!l.controlled_by_server);
    l.controlled_by_server = true;
    assert!(!l.take_control_from_server());
}

// ---------------------------------------------------------------------------------------------
// 4. The two call sites in the running client.
//
// Everything above drives `MovementCommands` and `Character` directly. None of it can see whether
// the running client ever calls any of it: the producers are in `world.rs` and `app.rs`.
//
// * `world.rs::apply_player_movement` latches the object-movement application result — 1 iff the
//   buffer is **non-autonomous** and **the player's**, which is the original function's guard
//   (non-autonomous, with a player object) and its result flag together.
// * `app.rs::command_interpreter_control_transfer` drains it into the server-control transition,
//   then runs the per-frame retake and returns control when its gates open.
// * `App::frame` calls that, once per frame, immediately after `sync_objects`.
//
// Three stations, one per link, because a link nothing proves is present may as well be absent.
// ---------------------------------------------------------------------------------------------

/// The scene `attach_character` builds at the middle of Holtburg's own landblock — the same body
/// [`settled_character`] builds, inside the `WorldScene` the application actually holds it in, so
/// the seam under test is reached through the type `App::frame` hands it.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn holtburg_scene(
    store: &Arc<RetailDatStore>,
    gpu: &mut dereth_render::device::Gpu,
) -> dereth_client::world::WorldScene {
    use dereth_client::world::{SceneConfig, WorldScene};
    let cfg = SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut s = WorldScene::load(store, gpu, cfg).expect("Holtburg's landscape loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    s.attach_character(store, &region, gpu)
        .expect("the body is created");
    for i in 1..=60 {
        s.character
            .as_mut()
            .expect("a body")
            .update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        s.character.as_ref().expect("a body").on_ground(),
        "the body must settle first"
    );
    s
}

/// **Station 4 — the `world.rs` latch, over the recording, through the application's own path.**
///
/// `combat-mode-while-moving` is replayed through `dereth_client_net`'s transport, `dereth_client_net::client_session`'s dispatchers,
/// `ObjectStream::pump` (which applies the object-movement message's three timestamp gates) and then the
/// real `WorldScene::sync_objects`, which is where `apply_player_movement` lives. Nothing is
/// synthesised and nothing is applied by hand.
///
/// The expectation is taken from a **different** reader than the thing under test: the stream's
/// own `pending_movement`, sampled before each sync, says whether a buffer for the player is
/// waiting and whether it is autonomous; the latch says whether `apply_player_movement` accepted
/// one. A disagreement in either direction fails.
///
/// Both directions are asserted, which is the point: an autonomous player buffer must latch
/// **nothing** (the original application returns 0 for it), and a fresh scene that has synced nothing
/// must read `false` — otherwise a latch stuck at `true` would pass the positive half alone.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[test]
fn the_scene_latches_lose_control_for_the_players_own_non_autonomous_buffers() {
    use dereth_client::app::command_interpreter_control_transfer as transfer;
    use dereth_client::objects::ObjectStream;
    use dereth_client_net::client_session::SessionEvent;

    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);

    // **Calibration, and it is the direction a stuck latch would pass.** A scene that has never
    // synced anything must read `false`, and the seam must do nothing at all over it.
    {
        let mut fresh = holtburg_scene(&store, &mut gpu);
        let mut mc = MovementCommands::default();
        let mut input = CharacterInput::default();
        assert_eq!(
            transfer(Some(&mut fresh), &mut mc, &mut input),
            (false, false),
            "no dispatch has happened, so nothing may be lost and nothing retaken"
        );
        assert!(
            !mc.lists.controlled_by_server,
            "and the interpreter is still autonomous"
        );
        assert_eq!(
            transfer(None, &mut mc, &mut input),
            (false, false),
            "and with no scene at all the step is a no-op, which is every frame before login"
        );
    }

    let records = load();
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net =
        ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("a replay net");
    let mut stream = ObjectStream::new();
    let mut scene: Option<dereth_client::world::WorldScene> = None;
    let mut mc = MovementCommands::default();
    let mut input = CharacterInput::default();
    let mut entered = false;

    // **The run lock is on before the replay starts, and this is the second signal.**
    //
    // Toggling combat while auto-running prints **"AutoRun OFF"** in the retail message window.
    // The original auto-run setter builds and sends that literal before its `sendEvent` gate, so
    // it is client-local and appears on no capture: `combat-mode-while-moving` can pin the
    // movement and can never touch it.
    //
    // It matters here because it discriminates a wire that reaches the same *movement* by the
    // wrong route: cancelling the three command lists would also stop an auto-runner, and would
    // be indistinguishable in the position trace — but it would print nothing, because the line
    // belongs to auto-run cancellation and nothing else raises it. So the message is asserted **through the
    // production path**: the latch `world.rs` set, drained by the free function `App::frame`
    // calls, which is the only producer of it in this build.
    let notice_arms_the_lock =
        mc.on_action(on_action(&ev(action::AUTORUN, true), |_| None), &mut input);
    assert!(
        notice_arms_the_lock && mc.lists.auto_run,
        "the run lock is on before the replay"
    );
    assert_eq!(
        mc.take_notices(),
        vec!["AutoRun ON"],
        "drained, so the next line is the stop's"
    );
    let mut notices_at_the_first_loss: Option<Vec<&'static str>> = None;

    // What the **stream** says was waiting, and what the **latch** says was accepted. Two readers.
    let (mut waiting_nonautonomous, mut waiting_autonomous) = (0u32, 0u32);
    let (mut losses, mut losses_on_a_frame_with_no_server_buffer) = (0u32, 0u32);
    let mut first_loss_set_the_flag: Option<bool> = None;
    // The denominator for the negative half: how many times the seam ran at all.
    let mut synced = 0u32;

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
        let Some(id) = stream.player() else { continue };
        if scene.is_none() {
            let Some(pos) = stream.presence(id).and_then(|p| p.position) else {
                continue;
            };
            let block = pos.cell.landblock();
            let cfg = dereth_client::world::SceneConfig {
                landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
                character: true,
                land_radius: 1,
                scenery_radius: 0,
                ..dereth_client::world::SceneConfig::default()
            };
            let mut s = dereth_client::world::WorldScene::load(&store, &mut gpu, cfg)
                .expect("the recording's landscape loads");
            let region = dereth_client::world::load_region(&store).expect("the region decodes");
            s.attach_character(&store, &region, &mut gpu)
                .expect("the body is created");
            scene = Some(s);
        }
        let s = scene.as_mut().expect("built above");
        // The other reader: what is waiting for `apply_player_movement`, before it runs.
        let autonomous = stream
            .presence(id)
            .and_then(|p| p.pending_movement.as_ref())
            .map(|b| b.autonomous);
        match autonomous {
            Some(true) => waiting_autonomous += 1,
            Some(false) => waiting_nonautonomous += 1,
            None => {}
        }
        s.sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync");
        synced += 1;
        let (lost, _) = transfer(Some(s), &mut mc, &mut input);
        if lost {
            losses += 1;
            if autonomous != Some(false) {
                losses_on_a_frame_with_no_server_buffer += 1;
            }
            if first_loss_set_the_flag.is_none() {
                first_loss_set_the_flag = Some(mc.lists.controlled_by_server);
                notices_at_the_first_loss = Some(mc.take_notices());
            }
            // The latch is a **take**: one dispatch may not be acted on twice.
            assert!(
                !transfer(Some(s), &mut mc, &mut input).0,
                "the latch was not cleared, so a second frame would lose control again"
            );
        }
    }

    assert!(
        scene.is_some(),
        "the recording must have produced a scene with a body"
    );
    // The denominator, said out loud: a replay that reached no buffer would satisfy every equality
    // below by having nothing on either side, so the premise is asserted explicitly.
    assert!(
        waiting_nonautonomous >= 4,
        "combat-mode-while-moving carries at least the four stance answers; the replay saw {waiting_nonautonomous}"
    );
    assert_eq!(
        losses, waiting_nonautonomous,
        "the control-loss count must match the non-autonomous player-buffer count"
    );
    assert_eq!(
        losses_on_a_frame_with_no_server_buffer, 0,
        "a frame with no server buffer waiting may not latch anything, and {synced} frames ran"
    );
    assert!(
        synced > waiting_nonautonomous * 4,
        "the negative half needs frames with nothing pending to be a negative at all: \
         {synced} syncs against {waiting_nonautonomous} buffers"
    );
    assert_eq!(
        first_loss_set_the_flag,
        Some(true),
        "the first loss must have set the command interpreter's server-control flag"
    );
    // **The in-play message, reproduced by the wire.** One line, exactly, on the first
    // server buffer — and the run lock is off afterwards, which is the half the trace can see.
    assert_eq!(
        notices_at_the_first_loss.as_deref(),
        Some(&["AutoRun OFF"][..]),
        "disabling auto-run inside the server-control-loss arm must raise its message-window line"
    );
    assert!(!mc.lists.auto_run, "and the run lock it cancelled is off");
    assert!(
        mc.take_notices().is_empty(),
        "and only the first loss says it: disabling already-disabled auto-run is idempotent, so a second cancel is silent"
    );
    // **The inconclusive column, said out loud rather than passed silently.** The negative above
    // is over the frames with *nothing* waiting. `combat-mode-while-moving` carries no **autonomous** buffer
    // addressed to the player, so `apply_player_movement`'s own `if buf.autonomous { return; }`
    // — the original `(autonomous == 0 || not-the-player)` guard — is **not
    // exercised by this recording**. It is covered where it can be: `MovementBuffer::autonomous`
    // is asserted `false` on all four stance answers in
    // `the_capture_records_both_transitions`, and the corpus's autonomous player buffers live in
    // the wider retained corpus, which this file deliberately does not read. A count that cannot
    // be examined is reported rather than silently skipped.
    eprintln!(
        "control-loss latch: combat-mode-while-moving replay — {synced} syncs; {waiting_nonautonomous} non-autonomous player \
         buffers reached `apply_player_movement` and server-control loss was observed {losses} times; \
         autonomous player buffers in this recording: {waiting_autonomous} (so that arm of the \
         guard is UNEXERCISED here, not confirmed)"
    );
}

/// One frame of the application's movement step, in `App::frame`'s own order: the control
/// transfer, then the body.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn seam_frame(
    scene: &mut dereth_client::world::WorldScene,
    mc: &mut MovementCommands,
    input: &mut CharacterInput,
    t: &mut f64,
    at: &mut Vec<(f32, f32)>,
) -> bool {
    let (_, retook) =
        dereth_client::app::command_interpreter_control_transfer(Some(scene), mc, input);
    *t += 1.0 / 30.0;
    let c = scene.character.as_mut().expect("a body");
    c.input = *input;
    c.update(LocalTime(*t));
    let o = c.position().frame.origin;
    at.push((o.x, o.y));
    retook
}

/// Ground speed in m/s over frames `[a, b)` of a recorded track, at 30 Hz — the measurement
/// [`over`] makes, over a plain slice.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn speed(at: &[(f32, f32)], a: usize, b: usize) -> f32 {
    assert!(
        b > a && b <= at.len(),
        "window {a}..{b} is outside a {}-frame run",
        at.len()
    );
    let (p, q) = (at[a], at[b - 1]);
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a frame count under 200, exact in f32.
    let secs = (b - a) as f32 / 30.0;
    ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt() / secs
}

/// **Station 5 — the retake, through the function `App::frame` calls.**
///
/// This is stations 1 and 2 re-run over the **application's** seam rather than over
/// `MovementCommands` directly: the body is the one a `WorldScene` holds, and every frame goes
/// through `dereth_client::app::command_interpreter_control_transfer`, which is the whole of what
/// `App::frame` does at this step. Both arms are built identically and differ only in whether the
/// held movement is the **run lock** or a **key on the substate list** — which is the split
/// server-control transition's auto-run cancellation creates.
///
/// The oracle is the retained `combat-mode-while-moving` recording, quoted in this file's header:
/// auto-run `2.670 -> 0.002 m/s` and never resuming; held `W`
/// `5.725 -> 4.000 -> 1.443 -> 5.856 -> 5.857` inside one 7.4-second press.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[test]
fn the_seam_retakes_control_for_a_held_key_and_never_for_a_cancelled_auto_run() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);

    // `(retakes, frame of the first retake, speed before the stance change, speed after it)`.
    let mut arm = |auto_run: bool| -> (u32, Option<usize>, f32, f32) {
        let mut scene = holtburg_scene(&store, &mut gpu);
        let mut mc = MovementCommands::default();
        let mut input = CharacterInput::default();
        // *Run as Default Movement*, as [`running_body`] explains.
        mc.ui_toggles_run = true;
        assert!(mc.on_action(
            on_action(&ev(action::TOGGLE_RUN_WALK, false), |_| None),
            &mut input
        ));
        let a = if auto_run {
            action::AUTORUN
        } else {
            action::MOVE_FORWARD
        };
        assert!(mc.on_action(on_action(&ev(a, true), |_| None), &mut input));
        assert!(input.forward, "either way the base slot holds WalkForward");
        assert_eq!(
            mc.lists.auto_run, auto_run,
            "and only the auto-run arm holds the run lock"
        );
        let _ = mc.take_notices();

        let mut t = 2.0f64;
        let mut at: Vec<(f32, f32)> = Vec::new();
        let mut retakes = 0u32;

        // **The premise.** Ninety frames of running, so neither arm can satisfy the stop by having
        // never moved — a frozen subject would satisfy every negative assertion at once.
        for _ in 0..90 {
            if seam_frame(&mut scene, &mut mc, &mut input, &mut t, &mut at) {
                retakes += 1;
            }
        }
        assert_eq!(
            retakes, 0,
            "nothing has taken control, so nothing can hand it back"
        );
        let before = speed(&at, 30, 90);

        // The server's own recorded answer to the first toggle, applied to the scene's body the
        // way `world.rs::apply_player_movement` applies it, and then the loss the latch drives.
        // (The latch itself is station 4's subject; this station is the retake.)
        apply_server_stance(scene.character.as_ref().expect("a body"), &answer(0));
        mc.lose_control_to_server(&mut input);
        assert!(mc.lists.controlled_by_server);
        // **The discriminator the position trace cannot make.** Handing control to the server cancels the
        // **run lock** and touches none of the three command lists. A wire that stopped the body
        // by clearing the *lists* instead would produce the same speeds in both arms of this test
        // and would be wrong everywhere else — and it would not print the in-play "AutoRun OFF",
        // which is why that line is worth asserting.
        assert!(
            !mc.lists.auto_run,
            "disabling auto-run cancelled the run lock"
        );
        assert_eq!(
            mc.lists.substate.len(),
            usize::from(!auto_run),
            "and the substate list is untouched, so the held key is still on it"
        );
        assert_eq!(
            mc.take_notices(),
            if auto_run {
                vec!["AutoRun OFF"]
            } else {
                Vec::new()
            },
            "the message-window line is raised for the run lock and for nothing else"
        );
        at.clear();
        let mut retaken_at: Option<usize> = None;
        for i in 0..150 {
            if seam_frame(&mut scene, &mut mc, &mut input, &mut t, &mut at) {
                retakes += 1;
                if retaken_at.is_none() {
                    retaken_at = Some(i);
                }
            }
        }
        let after = speed(&at, 120, 150);
        assert_eq!(
            scene
                .character
                .as_ref()
                .expect("a body")
                .driver()
                .movement
                .interp
                .interpreted_state
                .current_style,
            MotionCommand(0x8000_0049),
            "and he is in Magic stance either way: neither arm is a dead body"
        );
        (retakes, retaken_at, before, after)
    };

    let (auto_retakes, auto_at, auto_before, auto_after) = arm(true);
    let (held_retakes, held_at, held_before, held_after) = arm(false);

    assert!(
        auto_before > RUNNING,
        "the auto-run arm must run first, got {auto_before} m/s"
    );
    assert!(
        held_before > RUNNING,
        "the held-key arm must run first, got {held_before} m/s"
    );
    assert_eq!(
        auto_retakes, 0,
        "disabling auto-run cancelled the lock, so control is never handed back"
    );
    assert_eq!(auto_at, None);
    assert!(
        auto_after < STOPPED,
        "auto-run never resumes, got {auto_after} m/s"
    );
    assert_eq!(
        held_retakes, 1,
        "a held key takes control back exactly once"
    );
    assert!(
        held_after > RUNNING,
        "a held key resumes by itself, got {held_after} m/s"
    );
    // **Not instantaneous**: the per-frame update waits on the gates this seam feeds. A retake
    // on frame 0 would mean the two gates
    // (`motions_pending`, `move-to activity`) were not consulted at all, and every speed assertion
    // above would still pass — so the delay is asserted separately from the resume.
    let k = held_at.expect("the held-key arm takes control back");
    assert!(
        k >= 15,
        "the retake must wait for the stance animation; it came on frame {k}"
    );
    eprintln!(
        "control retake: through `App::frame`'s own step — auto-run {auto_before:.3} -> {auto_after:.3} m/s \
         with 0 retakes; held key {held_before:.3} -> {held_after:.3} m/s with {held_retakes} \
         retake on frame {k} ({:.2} s)",
        f64::from(u32::try_from(k).expect("under 150")) / 30.0
    );
}

/// **Station 6 — the call site in `App::frame`.**
///
/// The two stations above drive `command_interpreter_control_transfer` themselves, so **neither
/// can see whether the frame calls it**. So this asserts the *call*, once per drawn frame, and asserts the two effect counters are
/// **zero** beside it: a headless `App` has no link, so nothing may have been invented.
///
/// Three counters rather than one distinguish an instrument with no third state: `(0, 0, 0)` is
/// "the step never ran" and `(n, 0, 0)` is "it ran and no dispatch
/// arrived", and only the second is a working client.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[test]
fn the_frame_runs_the_control_transfer_step_once_per_frame() {
    use dereth_client::app::App;
    use dereth_client::config::Config;

    let dat_dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's fixture; set DERETH_TEST_DAT_DIR if they moved"
    );
    let cfg = Config {
        headless: true,
        frames: None,
        dat_dir,
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("a headless app: retail dats and a WARP device");
    app.load_first_pixel_scene()
        .expect("the first-pixel surface decodes");

    for n in 1..=3u64 {
        assert!(
            app.frame(),
            "a frame with nothing asking to quit continues the loop"
        );
        assert_eq!(app.frames_drawn(), n);
        assert_eq!(
            app.probe().control_transfer_counts(),
            (n, 0, 0),
            "frame {n}: the command interpreter's control-transfer step did not run"
        );
        // The teleport and position steps, asserted alongside so the frame order cannot drift unnoticed:
        // this step runs after `sync_objects`, which runs after both of those.
        assert_eq!(
            app.probe().player_teleport_use_times(),
            n,
            "frame {n}: the teleport step did not run"
        );
        assert_eq!(
            app.probe().position_use_times(),
            n,
            "frame {n}: the position step did not run"
        );
    }
    app.shutdown();
}
