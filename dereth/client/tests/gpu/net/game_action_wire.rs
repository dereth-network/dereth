//! Game actions on the wire: every recorded `0xF7B1` game action this client sends is reproduced
//! byte for byte by building the same request and encoding it; one release above the power-slider
//! cap sends two targeted-melee attacks (released power, then capped power) with the documented
//! bytes; and a failed send rolls the action stamp back, leaving no hole in the sequence.
//! Fixture: every recording's client datagrams through the shared capture reader, and a `Session`
//! backed by `MockTransport`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::combat::game_view_clicks::{seed_player, weenie};

use std::collections::BTreeMap;

use dereth_client::interaction::{self};
use dereth_client_model::{RecordingRequests, Request};
use dereth_client_net::client_session::testing::{
    session_names, shared_session, Corpus, MockTransport,
};
use dereth_client_net::client_session::Session;
use dereth_primitives::{LocalTime, NetQueue, ObjectId};
use dereth_protocol::{read_body, Message};
use dereth_transport::wire::ParsedPacket;

/// Every recording the corpus index names, in name order, so the byte-for-byte sender check below
/// is a claim about the whole corpus.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

/// The public game-action wrapper opcode, `0xF7B1`.
const GAME_ACTION: u32 = 0xF7B1;

/// One game action exactly as the retail client sent it: the whole blob payload, plus the two
/// fields read out of it.
#[derive(Debug, Clone)]
struct RetailAction {
    session: String,
    /// `[0xF7B1][stamp][sub-opcode][body]`, the bytes `dereth_client_net` handed the transport.
    blob: Vec<u8>,
    stamp: u32,
    sub: u32,
}

impl RetailAction {
    fn body(&self) -> &[u8] {
        &self.blob[12..]
    }
}

/// Single-fragment `0xF7B1` actions read across the discovered corpus.
///
/// Read at the fragment level rather than re-running the session. These actions fit within the
/// 448-byte fragment payload; the reader explicitly skips packets
/// that fail parsing, multi-fragment blobs and payloads shorter than the twelve-byte header.
/// `ParsedPacket::parse` is `dereth_client_net`'s parser, so framing is not re-implemented here.
fn retail_game_actions() -> Vec<RetailAction> {
    let mut out = Vec::new();
    for session in corpus_sessions() {
        for r in shared_session(&session).iter().filter(|r| r.c2s) {
            let Ok(p) = ParsedPacket::parse(&r.raw) else {
                continue;
            };
            for f in &p.fragments {
                if f.header.num_frags != 1 || f.payload.len() < 12 {
                    continue;
                }
                let dw = |i: usize| {
                    u32::from_le_bytes([
                        f.payload[i],
                        f.payload[i + 1],
                        f.payload[i + 2],
                        f.payload[i + 3],
                    ])
                };
                if dw(0) != GAME_ACTION {
                    continue;
                }
                out.push(RetailAction {
                    session: session.clone(),
                    blob: f.payload.clone(),
                    stamp: dw(4),
                    sub: dw(8),
                });
            }
        }
    }
    out
}

/// Decode one retail action's body into the [`Request`] this client would have produced for it.
///
/// The decode is [`dereth_protocol`]'s; the choice of request is the same choice
/// `dereth_client_model`'s senders make. `None` means the sub-opcode is not one this file covers.
fn request_for(a: &RetailAction) -> Option<Request> {
    use dereth_protocol::{combat as c, items as it, objects as ob};
    let b = a.body();
    Some(match a.sub {
        x if x == it::InventoryPutItemInContainer::OPCODE.0 => {
            Request::PutItemInContainer(read_body(b).ok()?)
        }
        x if x == it::InventoryGetAndWieldItem::OPCODE.0 => {
            Request::GetAndWieldItem(read_body(b).ok()?)
        }
        x if x == it::InventoryDropItem::OPCODE.0 => Request::DropItem(read_body(b).ok()?),
        x if x == it::InventoryUseEvent::OPCODE.0 => Request::UseEvent(read_body(b).ok()?),
        x if x == it::InventoryUseWithTargetEvent::OPCODE.0 => {
            Request::UseWithTargetEvent(read_body(b).ok()?)
        }
        x if x == it::InventoryStackableMerge::OPCODE.0 => {
            Request::StackableMerge(read_body(b).ok()?)
        }
        x if x == it::InventoryStackableSplitToContainer::OPCODE.0 => {
            Request::StackableSplitToContainer(read_body(b).ok()?)
        }
        x if x == it::InventoryStackableSplitTo3d::OPCODE.0 => {
            Request::StackableSplitTo3d(read_body(b).ok()?)
        }
        x if x == it::InventoryStackableSplitToWield::OPCODE.0 => {
            Request::StackableSplitToWield(read_body(b).ok()?)
        }
        x if x == it::InventoryGiveObjectRequest::OPCODE.0 => {
            Request::GiveObjectRequest(read_body(b).ok()?)
        }
        x if x == it::ItemQueryItemMana::OPCODE.0 => Request::QueryItemMana(read_body(b).ok()?),
        x if x == it::InventoryCreateTinkeringTool::OPCODE.0 => {
            Request::CreateTinkeringTool(read_body(b).ok()?)
        }
        x if x == ob::ItemAppraise::OPCODE.0 => Request::Appraise(read_body(b).ok()?),
        x if x == c::CombatChangeCombatMode::OPCODE.0 => {
            Request::ChangeCombatMode(read_body(b).ok()?)
        }
        x if x == c::CombatTargetedMeleeAttack::OPCODE.0 => {
            Request::TargetedMeleeAttack(read_body(b).ok()?)
        }
        x if x == c::CombatTargetedMissileAttack::OPCODE.0 => {
            Request::TargetedMissileAttack(read_body(b).ok()?)
        }
        x if x == c::CombatCancelAttack::OPCODE.0 => Request::CancelAttack(read_body(b).ok()?),
        x if x == c::CombatQueryHealth::OPCODE.0 => Request::QueryHealth(read_body(b).ok()?),
        x if x == c::MagicCastTargetedSpell::OPCODE.0 => {
            Request::CastTargetedSpell(read_body(b).ok()?)
        }
        x if x == c::MagicCastUntargetedSpell::OPCODE.0 => {
            Request::CastUntargetedSpell(read_body(b).ok()?)
        }
        _ => return None,
    })
}

fn mock_session() -> Session<MockTransport> {
    Session::new(MockTransport::new())
}

/// Behaviour: net.game-actions.every-retail-action-is-reproduced-byte-for-byte
///
/// Oracle: every capture in `fixtures/packet-captures`, the retail client's own `0xF7B1` blobs.
///
/// For every game action the recorded client sent whose opcode this file covers, the request is
/// decoded and pushed straight back out through [`interaction::send_request`] — the one place a
/// [`Request`] becomes bytes — and the result is compared with the recording **byte for byte**.
///
/// The one substituted field is the `OrderedActionHeader` stamp at bytes 4..8: it comes from the session's own
/// global counter, whose value depends on how many actions preceded it in that session, and
/// `dereth_client_net::client_session::outbound`'s tests already pin it. Everything else — the `0xF7B1` wrapper, the
/// sub-opcode, every field of every body, the queue and the ordered flag — is compared as recorded.
#[test]
fn every_retail_game_action_is_reproduced_byte_for_byte() {
    let actions = retail_game_actions();
    assert!(!actions.is_empty(), "the corpus must carry game actions");
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    let mut unowned: BTreeMap<u32, usize> = BTreeMap::new();
    let mut compared = 0usize;

    for a in &actions {
        let Some(req) = request_for(a) else {
            *unowned.entry(a.sub).or_default() += 1;
            continue;
        };
        *seen.entry(a.sub).or_default() += 1;

        let mut s = mock_session();
        let stamp = s.next_action_stamp();
        assert!(
            interaction::send_request(&mut s, &req),
            "{:#06X} from {} would not send: {req:?}",
            a.sub,
            a.session
        );
        let sent = s.transport.sent.last().expect("one blob");
        assert_eq!(sent.queue, NetQueue::Weenie, "a game action rides queue 5");
        assert!(sent.ordered, "and is marked ordered");

        // The recording, with only the stamp replaced by the one our counter allocated.
        let mut expected = a.blob.clone();
        assert_eq!(
            u32::from_le_bytes([expected[4], expected[5], expected[6], expected[7]]),
            a.stamp,
            "the field being substituted really is the action-order header stamp"
        );
        expected[4..8].copy_from_slice(&stamp.to_le_bytes());
        assert_eq!(
            sent.payload, expected,
            "sub-opcode {:#06X} from {}: ours {:02X?} vs retail {:02X?}",
            a.sub, a.session, sent.payload, a.blob
        );
        compared += 1;
    }

    eprintln!(
        "game actions: {compared} retail game actions reproduced; by sub-opcode {seen:#06X?}"
    );
    eprintln!("game actions: sub-opcodes not covered here, not compared: {unowned:#06X?}");

    assert!(compared > 0, "no recorded action was comparable");
    let has = |op: u32| seen.contains_key(&op);
    assert!(has(0x0036), "a *use* (Inventory_UseEvent) is in the corpus");
    assert!(
        has(0x0019) || has(0x001A),
        "an *inventory move* is in the corpus"
    );
    assert!(has(0x0053), "a *combat mode change* is in the corpus");
    assert!(has(0x0263), "and Item_QueryItemMana");
    // Every recorded targeted melee attack and every recorded get-and-wield was byte-checked by
    // the loop above: the decoded corpus's own count of each is the number compared.
    let recorded = |sub: u32| -> usize {
        Corpus::shared_all()
            .iter()
            .map(|c| c.count_action(sub))
            .sum()
    };
    for (sub, name) in [
        (0x0008, "Combat_TargetedMeleeAttack"),
        (0x001A, "Inventory_GetAndWieldItem"),
    ] {
        let want = recorded(sub);
        assert!(want > 0, "the corpus carries a {name}");
        assert_eq!(
            seen.get(&sub).copied().unwrap_or(0),
            want,
            "every recorded {name} is byte-checked here"
        );
    }
}

/// The targeted-melee action layout and the release-above-cap double send.
///
/// Drives `end_attack_request` and isolates the two-message response to one release above the
/// slider cap, which the recordings do not isolate. It checks powers 1.0 then 0.5 and a 24-byte
/// wrapper/stamp/action payload; the sub-opcode plus target, height and power occupy sixteen of
/// those bytes. The recorded attacks are byte-checked by the corpus sender test.
#[test]
fn an_attack_is_two_messages_and_the_documented_bytes() {
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5000_0002);
    let monster = ObjectId(0x8000_0777);
    seed_player(&mut w, player);
    seed_creature(&mut w, monster);
    w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
    w.combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    // The slider cap: `ui_requested_power` resets to 0.5, and the release is at full power.
    assert_eq!(w.combat.ui_requested_power, 0.5);

    let mut req = RecordingRequests::default();
    // Hold for the one-second POWER_BAR_CHARGE_SECONDS interval so the bar reaches full power and
    // the release takes the level >= ui_requested_power branch.
    let down = LocalTime(100.0);
    let up = LocalTime(101.0);
    w.start_attack_request(true, down)
        .expect("a valid target is selected");
    assert!(
        w.combat.power_bar_level(up) >= w.combat.ui_requested_power,
        "the bar is charged"
    );
    w.end_attack_request(
        &mut req,
        dereth_client_model::combat::AttackHeight::Medium,
        Some(1.0),
        true,
        up,
    );

    // A release above the slider cap sends released power first and capped
    // power second, producing two attack messages from this request.
    assert_eq!(req.0.len(), 2, "the shipped double send: {:?}", req.0);
    let powers: Vec<f32> = req
        .0
        .iter()
        .map(|r| match r {
            Request::TargetedMeleeAttack(m) => m.power_level,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        powers,
        vec![1.0, 0.5],
        "full power, then clamped to the cap"
    );

    // ...and both reach the wire as game actions with consecutive stamps and no hole.
    let mut s = mock_session();
    for r in &req.0 {
        assert!(interaction::send_request(&mut s, r));
    }
    assert_eq!(s.transport.sent.len(), 2);
    for (i, b) in s.transport.sent.iter().enumerate() {
        assert_eq!(b.queue, NetQueue::Weenie);
        assert_eq!(
            b.payload.len(),
            4 + 4 + 4 + 4 + 4 + 4,
            "opcode, stamp, sub, target, height, power"
        );
        assert_eq!(&b.payload[0..4], &GAME_ACTION.to_le_bytes());
        // The first game action carries stamp 1 because allocation pre-increments the counter;
        // the pair here is 1 then 2, as the corpus shows.
        assert_eq!(
            &b.payload[4..8],
            &(u32::try_from(i).unwrap() + 1).to_le_bytes(),
            "no hole"
        );
        assert_eq!(
            &b.payload[8..12],
            &0x0008u32.to_le_bytes(),
            "Combat_TargetedMeleeAttack"
        );
        assert_eq!(&b.payload[12..16], &monster.0.to_le_bytes());
    }
}

/// A failed send rolls back the global action counter. The retry must reuse
/// its stamp: a hole would cause the server to discard later ordered actions.
#[test]
fn a_failed_send_leaves_no_hole_in_the_action_sequence() {
    let mut s = mock_session();
    let use_it = Request::UseEvent(dereth_protocol::items::InventoryUseEvent {
        object: ObjectId(9),
    });
    assert!(interaction::send_request(&mut s, &use_it));
    assert_eq!(s.next_action_stamp(), 2, "the first action took stamp 1");

    // The transport reports the send did not go out.
    s.report_send_failed();
    assert_eq!(s.next_action_stamp(), 1, "the stamp is available again");

    assert!(interaction::send_request(&mut s, &use_it));
    let payloads: Vec<u32> = s
        .transport
        .sent
        .iter()
        .map(|b| u32::from_le_bytes([b.payload[4], b.payload[5], b.payload[6], b.payload[7]]))
        .collect();
    assert_eq!(payloads, vec![1, 1], "the retry reuses the stamp");
}

fn seed_creature(w: &mut dereth_client_model::World, id: ObjectId) {
    let m = weenie(w, id);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    // `BF_ATTACKABLE`: attackability for a non-player, non-pet creature requires bit 4 of its
    // description flags, not only the creature type. In long-solo-play 44 of 97 creatures carry
    // the bit; the rest are non-combatant NPCs, which the predicate refuses.
    m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
}
