//! An object is drawn at the placement the server names for it, and a placement its setup does not
//! carry falls back to key `0` and then the identity. The server names a placement in three message
//! shapes: `0xF745`/`0xF7DB` descriptions (the `0x00020000` field, installed only when no movement
//! buffer is present), `0xF748`/`0xF619` positions (flag `0x0002`, applied only when no animation is
//! active) and `0xF749` parent messages (the fourth dword, applied without that guard).
//!
//! Fixture: the retail dats, and the recordings in `fixtures/packet-captures/` replayed through the
//! real transport and `ObjectStream`, compared with a separate walk of the same wire bodies that
//! bypasses `ObjectStream`.

use crate::common::workspace_root_buf as repo_root;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_client::net::ClientNetwork;
use dereth_client_net::client_session::testing::capture::{self, peer as addr, Datagram};
use dereth_client_net::recording;
use dereth_primitives::LocalTime;

/// The sorted JSONL recordings in `fixtures/packet-captures/`. A session need not end in a clean
/// log-off. An unavailable directory contributes no paths.
fn sessions() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(repo_root().join("fixtures/packet-captures")) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    v.sort();
    v
}

/// One recording's datagrams, through the workspace's shared reader. An unreadable recording
/// reads as empty, which every caller below treats as one with nothing to replay.
fn load(path: &Path) -> Vec<Datagram> {
    capture::load(path).unwrap_or_default()
}

/// The recording's own connection sequence number, or 0 when it has no login request.
fn connection_sequence_number(records: &[Datagram]) -> u32 {
    recording::connection_sequence_number(records).unwrap_or(0)
}

/// Feed all server-direction rows, then poll available reassembled (opcode,body) messages.
/// Empty input or network construction failure returns no messages; ObjectStream is not used.
fn server_blobs(path: &Path) -> Vec<(u32, Vec<u8>)> {
    let records = load(path);
    if records.is_empty() {
        return Vec::new();
    }
    let Ok(mut net) = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    ) else {
        return Vec::new();
    };
    for r in records.iter().filter(|r| !r.c2s) {
        net.feed(&r.raw, addr(r.pair), LocalTime(r.t));
    }
    let mut out = Vec::new();
    while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
        out.push((m.opcode, m.body));
    }
    out
}

// ------------------------------------------------------------------------------------------------
// The draw side compares the models' frame resolution with the animation part array's. The
// archive decoding is shared with production.
// ------------------------------------------------------------------------------------------------

use dereth_assets::{Decode, Setup};
use dereth_client::models::{
    placement_frames, resolve_parts_at, PLACEMENT_DEFAULT, PLACEMENT_RESTING,
};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, Quat, Vec3};

/// The retail store, or **fail**: a missing install is a failure, never a skip.
fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn setup_of(store: &RetailDatStore, id: DataId) -> Option<Setup> {
    let bytes = store.read_typed(DbType::Setup, id).ok()?;
    Setup::decode_payload(id, &bytes).ok()
}

fn identity() -> Frame {
    Frame::new(Vec3::ZERO, Quat::IDENTITY)
}

/// Query an absent placement and compare every returned part frame with key 0, then identity
/// for a missing default/frame. Original lookup retried literal 0 before giving up. This preserves
/// that fallback when accepting wire IDs. The setup-ID census must exceed 100; decode failures
/// are skipped and the absent-key premise is asserted for each decoded record. Key0 must occur,
/// but the counter for records without it need not be positive, and empty part lists add no checks.
#[test]
fn a_placement_the_setup_does_not_carry_falls_back_exactly_as_before() {
    let store = store();
    // Use 0x0FFF as the absent-key probe; assert its absence in each decoded setup below, then
    // compare the returned frames with key 0/default-frame fallback.
    const ABSENT: u32 = 0x0FFF;

    let ids = block_setup_ids(&store);
    assert!(
        ids.len() > 100,
        "the block census must have loaded: {}",
        ids.len()
    );
    let (mut with_default, mut without) = (0usize, 0usize);
    for id in &ids {
        let did = DataId(*id);
        let Some(s) = setup_of(&store, did) else {
            continue;
        };
        assert!(!s.placement_frames.contains_key(&ABSENT));
        let default = s.placement_frames.get(&PLACEMENT_DEFAULT);
        if default.is_some() {
            with_default += 1;
        } else {
            without += 1;
        }
        let drawn = resolve_parts_at(&store, did, ABSENT);
        for (i, p) in drawn.iter().enumerate() {
            let expected = default
                .and_then(|f| f.frames.get(i).copied())
                .unwrap_or_else(identity);
            assert_eq!(
                p.frame, expected,
                "{did} part {i} falls back to key 0 then the identity"
            );
        }
    }
    eprintln!(
        "{} setup IDs in the census: {with_default} decoded records have key 0, {without} lack it",
        ids.len()
    );
    assert!(with_default > 0, "the key-0 fallback arm must be exercised");
}

/// Compare model and animation-part-array frame results for the union of decoded setup keys
/// plus 0x0FFF. Require more than two keys and more than 500 frame-list comparisons, matching
/// hit/miss and equal frame arrays. Setup/animation decoding may skip a record. This population must give zero agreed-None cases;
/// separate synthetic no-setup and empty-setup cases exercise the give-up guards.
#[test]
fn the_part_array_and_models_resolve_the_same_placement() {
    use dereth_animation::data::AnimAssets;
    use dereth_animation::parts::PartArray;
    use dereth_animation::seq::Sequence;
    use std::sync::Arc;

    let store = store();
    let store = Arc::new(store);
    let assets = dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(&store));

    let ids = block_setup_ids(&store);
    let mut every_key: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for id in &ids {
        if let Some(s) = setup_of(&store, DataId(*id)) {
            every_key.extend(s.placement_frames.keys().copied());
        }
    }
    every_key.insert(0x0FFF);
    assert!(every_key.len() > 2, "keys seen: {every_key:?}");

    let (mut compared, mut agreed_none) = (0usize, 0usize);
    for id in &ids {
        let did = DataId(*id);
        let Some(dat) = setup_of(&store, did) else {
            continue;
        };
        let Some(sd) = AnimAssets::setup(&assets, did) else {
            continue;
        };
        for key in &every_key {
            let mut seq = Sequence::new();
            let mut pa = PartArray::new();
            pa.setup = Some(Arc::clone(&sd));
            let hit = pa.set_placement_frame(*key, &mut seq);
            let mine = placement_frames(&dat, *key);
            assert_eq!(hit, mine.is_some(), "{did} key {key}: hit/miss disagrees");
            match (seq.get_curr_animframe(), mine) {
                (Some(af), Some(frames)) => {
                    assert_eq!(
                        af.frames.as_slice(),
                        frames,
                        "{did} key {key}: frames differ"
                    );
                    compared += 1;
                }
                (None, None) => agreed_none += 1,
                (a, b) => panic!("{did} key {key}: {} vs {}", a.is_some(), b.is_some()),
            }
        }
    }
    eprintln!(
        "{} setups x {} placement keys: {compared} frame lists equal between dereth_animation::PartArray and dereth_client::models, {agreed_none} agreed on no entry",
        ids.len(),
        every_key.len()
    );
    assert!(compared > 500, "only {compared} comparisons");
    // The training block has 84 setups with both keys and 86 default-only, 170 in all. Here the
    // compared population must agree on no None result; the synthetic guards below exercise
    // missing setup/default data without requiring a current exact 170-record census.
    assert_eq!(
        agreed_none, 0,
        "the retail block cannot exercise the give-up arm"
    );

    // Give up with no setup, or after both the named-key and default-key lookup miss in an
    // empty setup. In both cases the sequence has no animation frame; no rendered pixels tested.
    let mut seq = Sequence::new();
    let mut bare = PartArray::new();
    assert!(!bare.set_placement_frame(PLACEMENT_RESTING, &mut seq));
    assert!(seq.get_curr_animframe().is_none());
    bare.setup = Some(Arc::new(dereth_animation::data::SetupData::default()));
    assert!(!bare.set_placement_frame(PLACEMENT_RESTING, &mut seq));
    assert!(
        seq.get_curr_animframe().is_none(),
        "a setup with no placement frames gives up"
    );
}

/// Distinct setup-space IDs found in the loaded training-dungeon environment-cell statics.
/// This re-derives the population for both tests; decoder/load behavior determines membership.
fn block_setup_ids(store: &RetailDatStore) -> Vec<u32> {
    use dereth_client::env_cells::{cell_statics, EnvCellLoader};
    let mut loader = EnvCellLoader::new();
    let mut out: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for d in loader.load_block(store, 0x8602) {
        for s in cell_statics(&d) {
            if s.id.0 & 0xFF00_0000 == 0x0200_0000 {
                out.insert(s.id.0);
            }
        }
    }
    out.into_iter().collect()
}

/// Behaviour: objects.placement.a-server-named-placement-draws-its-own-frames
/// Sample ObjectStream after every recording row is ticked and pumped; these are record
/// iterations, not rendered App frames. Keep the first strict peak census per recording and
/// the first non-Resting placement per object ID. Across sessions the first mapping wins again.
/// Summed peaks are per-session maxima, not one simultaneous corpus-wide world.
///
/// The census is taken at each recording's peak, not at its end: a recording that logs off
/// clean ends with an empty world, and one that does not log off still holds its presences.
///
/// Compare replay mappings with wire_non_resting, which bypasses ObjectStream but shares packet
/// transport and body decoders. It retains selected first mappings, not a complete chronology.
/// Per-session peak-census sums, a nonempty replay mapping, a nonzero placement and a live
/// log-off case prevent specified vacuous passes. The final partition does not require a live
/// no-log-off case to exist.
#[test]
fn the_corpus_poses_objects_at_the_placements_the_server_names() {
    use std::collections::BTreeMap as Map;

    let mut sessions_seen = 0usize;
    let mut live = 0usize;
    let mut end_empty = 0usize;
    let mut end_holding = 0usize;
    let mut live_with_log_off = 0usize;
    let mut corpus_peak = 0usize;
    let mut corpus_peak_census: Map<u32, usize> = Map::new();
    let mut replay_named: Map<u32, u32> = Map::new();
    let mut wire_named: Map<u32, u32> = Map::new();
    let mut creates_with_field = 0usize;

    for path in sessions() {
        let Some(s) = replay_objects(&path) else {
            continue;
        };
        sessions_seen += 1;
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (wire, with_field) = wire_non_resting(&path);
        creates_with_field += with_field;
        let log_offs = server_log_offs(&path);

        // Self-consistency: the census must have been taken at the record iteration whose
        // peak it reports, or the numbers below are two different worlds added together.
        assert_eq!(
            s.peak_census.values().sum::<usize>(),
            s.peak_presences,
            "{name}: the peak census was taken at a different record iteration from the peak it names"
        );
        // A recording with no observed presence (a login-only one) cannot test teardown;
        // membership is decided by the sampled count.
        if s.peak_presences > 0 {
            live += 1;
            if log_offs > 0 {
                live_with_log_off += 1;
            }
            if s.final_presences == 0 {
                end_empty += 1;
            } else {
                end_holding += 1;
            }
            // The partition, asserted per session and in **both** directions, because an equality
            // over the totals alone cannot tell "the teardown fired" from "there was nothing to
            // tear down". See the corpus-level identity below for why it is stated this way.
            //
            // These two locate a failure rather than guard against it: the guard is the
            // corpus-level `end_empty == live_with_log_off`, where a spurious reset on a recording
            // with no log-off reddens. What these two add is the session's name in the message.
            if log_offs > 0 {
                assert_eq!(
                    s.final_presences, 0,
                    "{name}: the server sent {log_offs} `0xF653 Login_LogOffCharacter` but the world still holds {} presence(s) after the last record",
                    s.final_presences
                );
            } else {
                assert!(
                    s.final_presences > 0,
                    "{name}: this recording carries no `0xF653 Login_LogOffCharacter` at all, so \
                     nothing ever asked the client to end the character session -- an empty world \
                     at the last datagram means something else cleared it"
                );
            }
        }
        eprintln!(
            "  {name}: {} record iterations, {} events | presences: peak {} {:?}, last record {} | server log-offs {} | first off-Resting mappings: {} in the replay, {} named on the wire",
            s.frames,
            s.events,
            s.peak_presences,
            s.peak_census,
            s.final_presences,
            log_offs,
            s.ever_non_resting.len(),
            wire.len()
        );

        corpus_peak += s.peak_presences;
        for (k, v) in &s.peak_census {
            *corpus_peak_census.entry(*k).or_default() += v;
        }
        for (id, pid) in &s.ever_non_resting {
            replay_named.entry(*id).or_insert(*pid);
        }
        for (id, pid) in wire {
            wire_named.entry(id).or_insert(pid);
        }
    }

    assert!(
        sessions_seen > 0,
        "the capture corpus is this file's oracle and is empty"
    );

    let non_resting_at_peak: usize = corpus_peak_census
        .iter()
        .filter(|(k, _)| **k != PLACEMENT_RESTING)
        .map(|(_, v)| *v)
        .sum();
    // Count retained first placements, separating DEFAULT 0 from nonzero IDs. Original decoding
    // supplied 0 when both movement and animframe flags were absent; an explicit 0 can also equal
    // DEFAULT. This aggregate value alone does not distinguish those two field-presence cases.
    let mut by_placement: Map<u32, usize> = Map::new();
    for pid in replay_named.values() {
        *by_placement.entry(*pid).or_default() += 1;
    }
    let server_named = replay_named
        .values()
        .filter(|p| **p != PLACEMENT_DEFAULT)
        .count();

    eprintln!(
        "corpus: {sessions_seen} sessions, {live} of which ever hold a presence, {live_with_log_off} \
         of those carrying a server log-off | {end_empty} empty at the last datagram, \
         {end_holding} still holding | peak presences summed over sessions {corpus_peak}, \
         census at those peaks {corpus_peak_census:?} ({non_resting_at_peak} off Resting) | \
         {} first off-Resting ID mappings, by placement {by_placement:?}, of which {server_named} \
         nonzero placements, out of {creates_with_field} decoded create/update bodies that carried the \
         animframe field at all",
        replay_named.len()
    );

    // Require a positive sum of per-session presence peaks; a final empty world alone would
    // not establish that any replay had held a presence.
    assert!(
        corpus_peak > 0,
        "no capture ever held a presence at any frame -- the replay plumbing is broken"
    );
    // 1b. The teardown, as a **partition of the live recordings** rather than as a flat equality.
    //
    //     A flat `end_empty == live` is the wrong population: a recording that carries no
    //     `0xF653 Login_LogOffCharacter` in either direction (it ends on a bare packet-level
    //     `Disconnect` flag) never ends the character session, so `ObjectStream::reset` never runs
    //     and its presences are rightly still there at the last datagram.
    //
    //     So the invariant is stated against the population that can exercise it, in both
    //     directions, and the two arms partition `live` by construction:
    //
    //       * every live recording whose server stream carried the log-off ends empty;
    //       * every live recording whose server stream did not still holds its world.
    //
    //     Per-session assertions name the failing recording. These checks require an exhaustive
    //     partition, equality of empty ends with live log-off cases, and at least one such case.
    //     There is no end_holding>0 assertion: the no-log-off arm can be absent from a new corpus.
    assert_eq!(
        end_empty + end_holding,
        live,
        "the two arms of the teardown partition do not cover the live recordings"
    );
    assert_eq!(
        end_empty, live_with_log_off,
        "a recording whose server stream carried `0xF653 Login_LogOffCharacter` still holds \
         presences after its last datagram, or one that carried none came out empty; \
         `ObjectStream::reset` at log-off is then not the whole explanation for an empty world"
    );
    assert!(
        live_with_log_off > 0,
        "no live recording in the corpus carries a server log-off, so the equality above is vacuous and no live teardown case is exercised"
    );

    // 2. The count that was sampled, against an oracle decoded off the wire without `ObjectStream`.
    //    A bare `non_resting > 0` would be satisfied by one accidental object.
    assert_eq!(
        replay_named, wire_named,
        "the first off-Resting mappings observed by ObjectStream differ from the selected create/parent wire mappings: {} in replay against {} on wire",
        replay_named.len(),
        wire_named.len()
    );
    assert!(
        !replay_named.is_empty(),
        "the selected create/parent placement oracle has no non-Resting mapping, so this test observed no such case"
    );
    assert!(
        server_named > 0,
        "every retained non-Resting mapping has PLACEMENT_DEFAULT; at least one nonzero placement is required"
    );
}

/// Record-iteration samples from one replay. The peak avoids an end-only empty-world census;
/// the final count separately checks the partition by observed server log-off, including
/// recordings that end while still holding objects.
struct Sampled {
    /// `ObjectStream::len()` at its high-water mark across the replay.
    peak_presences: usize,
    /// Census at the first record iteration attaining this strict maximum; its sum is the peak.
    peak_census: BTreeMap<u32, usize>,
    /// Object ID to first non-Resting placement observed after a record iteration.
    ever_non_resting: BTreeMap<u32, u32>,
    /// Count after the last record. Expected zero for a live recording with a server log-off,
    /// positive for a live recording without one; it is not universally zero.
    final_presences: usize,
    frames: usize,
    events: usize,
}

/// Feed server rows and tick/pump after every row, including client-direction rows, discarding
/// outgoing packets. Enter the first character on the first suitable CharacterSet event.
/// Empty/unreadable input or network construction failure yields no Sampled value. There is
/// no App/render loop; frames below counts record iterations and events counts pump results.
fn replay_objects(path: &Path) -> Option<Sampled> {
    use dereth_client_net::client_session::SessionEvent;

    let records = load(path);
    if records.is_empty() {
        return None;
    }
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .ok()?;
    let mut objects = dereth_client::objects::ObjectStream::new();
    let mut entered = false;
    let mut out = Sampled {
        peak_presences: 0,
        peak_census: BTreeMap::new(),
        ever_non_resting: BTreeMap::new(),
        final_presences: 0,
        frames: 0,
        events: 0,
    };
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            out.events += 1;
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
        out.frames += 1;
        // Sample after this row's tick and pump. Replace the peak only on strict growth, so a
        // later equal-sized world does not replace its census. Final state may remain populated.
        if objects.len() > out.peak_presences {
            out.peak_presences = objects.len();
            out.peak_census =
                objects
                    .presences()
                    .fold(BTreeMap::new(), |mut m: BTreeMap<u32, usize>, (_, p)| {
                        *m.entry(p.placement).or_default() += 1;
                        m
                    });
        }
        for (id, p) in objects.presences() {
            if p.placement != PLACEMENT_RESTING {
                out.ever_non_resting.entry(id.0).or_insert(p.placement);
            }
        }
    }
    out.final_presences = objects.len();
    Some(out)
}

/// Build first non-Resting mappings from decoded create/update and parent messages, sharing
/// transport/decoders with replay but never invoking ObjectStream.
///
/// Original description application installed the animframe only with no movement buffer;
/// absent animframe then defaulted to 0. Original parent application installed the fourth dword
/// on the child without the animation guard. These rules drive the two branches below.
/// Position forms 0xF748/0xF619 are excluded: their original route first unparented, then applied
/// placement only without active animations; current Presence::pending_placement defers that
/// decision. This oracle does not evaluate those pending placements.
///
/// Return first mapping per ID and a count of decoded 0xF745/0xF7DB bodies with animframe present.
/// That count includes bodies later skipped for movement; it is not solely create opcodes or
/// solely messages contributing a retained mapping. Malformed bodies are skipped.
fn wire_non_resting(path: &Path) -> (BTreeMap<u32, u32>, usize) {
    use dereth_protocol::objects::{ItemCreateObject, ItemUpdateObject};
    use dereth_protocol::{read_body_padded, Reader};

    let mut named: BTreeMap<u32, u32> = BTreeMap::new();
    let mut with_field = 0usize;
    for (op, body) in server_blobs(path) {
        match op {
            0xF745 | 0xF7DB => {
                let payload = if op == 0xF745 {
                    read_body_padded::<ItemCreateObject>(&body).map(|m| m.0)
                } else {
                    read_body_padded::<ItemUpdateObject>(&body).map(|m| m.0)
                };
                let Ok(p) = payload else { continue };
                let d = &p.physicsdesc;
                if d.animframe_id.is_some() {
                    with_field += 1;
                }
                if d.movement.is_some() {
                    continue;
                }
                let pid = d.animframe_id.unwrap_or(PLACEMENT_DEFAULT);
                if pid != PLACEMENT_RESTING {
                    named.entry(p.id.0).or_insert(pid);
                }
            }
            0xF749 => {
                let mut r = Reader::body(&body);
                let (Ok(_creature), Ok(item), Ok(_location), Ok(pid)) =
                    (r.u32(), r.u32(), r.u32(), r.u32())
                else {
                    continue;
                };
                if pid != PLACEMENT_RESTING {
                    named.entry(item).or_insert(pid);
                }
            }
            _ => {}
        }
    }
    (named, with_field)
}

/// Count reassembled server 0xF653 log-off messages without ObjectStream. This supplies the
/// teardown premise separately from final presence count, while sharing transport decoding.
/// The client turns these messages into `SessionEvent::LoggedOff` and a stream reset; the helper
/// itself only counts opcodes, not reset invocations or state-change events.
fn server_log_offs(path: &Path) -> usize {
    server_blobs(path)
        .iter()
        .filter(|(op, _)| *op == 0xF653)
        .count()
}
