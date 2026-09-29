//! Contracts for corpus replay.
//! Fixture: shared recorded messages and synthetic state.

use crate::common::model_replay::*;

#[test]
fn every_public_weenie_desc_in_the_corpus_consumes_its_payload_exactly() {
    let mut total = 0usize;
    for (name, blobs) in corpus::load_all() {
        assert!(
            !blobs.is_empty(),
            "{name}: the capture corpus is this test's oracle"
        );
        for b in blobs.iter().filter(|b| b.dir == Dir::S2c) {
            match b.opcode() {
                OP_ITEM_CREATE_OBJECT => {
                    let m = read_body::<dereth_protocol::objects::ItemCreateObject>(b.body())
                        .unwrap_or_else(|e| panic!("{name}: 0xF745 at t={}: {e}", b.t));
                    assert!(
                        !m.0.wdesc.name.is_empty(),
                        "{name}: an unnamed object at t={}",
                        b.t
                    );
                    total += 1;
                }
                OP_ITEM_UPDATE_OBJECT => {
                    read_body::<dereth_protocol::objects::ItemUpdateObject>(b.body())
                        .unwrap_or_else(|e| panic!("{name}: 0xF7DB at t={}: {e}", b.t));
                    total += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        total,
        input_count(OP_ITEM_CREATE_OBJECT) + input_count(OP_ITEM_UPDATE_OBJECT),
        "object descriptors (0xF745 + 0xF7DB) in the corpus"
    );
}

#[test]
fn replaying_the_create_and_remove_stream_builds_the_object_tables() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut any_player = false;
    let mut in_world = 0usize;
    let mut login_only: Vec<&str> = Vec::new();
    for (name, blobs) in &all {
        let r = replay(blobs);
        if r.create_players == 0 {
            assert_eq!(
                r.creates, 0,
                "{name}: objects created without a 0xF746 Login_CreatePlayer"
            );
            assert_eq!(
                r.player_descriptions, 0,
                "{name}: a 0x0013 without a character in world"
            );
            login_only.push(name);
            continue;
        }
        in_world += 1;
        assert!(r.creates > 0, "{name}: no objects were created at all");
        assert_eq!(
            r.stale_instances, 0,
            "{name}: the server never sends a stale instance sequence in a clean session"
        );
        if r.player.is_some() {
            any_player = true;
        }
        assert!(
            r.world_objects <= r.creates + r.merges,
            "{name}: more objects in the table than the capture created"
        );
    }
    assert!(any_player, "at least one session must log a character in");
    assert_eq!(
        in_world,
        recorded_world_sessions(),
        "the captures that enter the world, as many as carry a recorded 0x0013"
    );
    assert_eq!(
        login_only,
        all.iter()
            .filter(|(_, bs)| !bs
                .iter()
                .any(|b| b.dir == Dir::S2c && b.opcode() == OP_LOGIN_CREATE_PLAYER))
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>(),
        "and two are login-only"
    );
}

#[test]
fn the_create_player_and_delete_messages_all_decode_and_are_not_skipped() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let (mut players, mut deletes) = (0usize, 0usize);
    for (_, blobs) in &all {
        let r = replay(blobs);
        players += r.create_players;
        deletes += r.deletes;
    }
    assert_eq!(
        players,
        input_count(OP_LOGIN_CREATE_PLAYER),
        "0xF746 Login_CreatePlayer across the corpus"
    );
    assert_eq!(
        deletes,
        input_count(OP_ITEM_DELETE_OBJECT),
        "0xF747 Item_DeleteObject across the corpus -- counting zero would mean they are being \
         skipped again, and counting fewer than this means a recording went unread"
    );
    eprintln!("{players} create-player and {deletes} delete message(s) decoded");
}

#[test]
fn every_player_description_in_the_corpus_decodes_its_player_module() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut seen = 0usize;
    for (name, blobs) in &all {
        let r = replay(blobs);
        assert_eq!(
            r.player_modules_decoded, r.player_descriptions,
            "{name}: {} of {} PlayerModules decoded",
            r.player_modules_decoded, r.player_descriptions
        );
        seen += r.player_descriptions;
    }
    assert_eq!(
        seen,
        description_count(),
        "Login_PlayerDescription events across the corpus"
    );
    eprintln!("{seen} player description(s) decoded in full");
}

/// Behaviour: objects.corpus.every-recorded-create-description-and-quality-update-lands
#[test]
fn the_quality_updates_from_the_corpus_land_and_are_not_spuriously_rejected() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut applied = 0usize;
    let mut stale = 0usize;
    let mut stale_by_session: Vec<(String, usize)> = Vec::new();
    let mut early = 0usize;
    let mut without_player = 0usize;
    let mut early_by_session: Vec<(String, usize)> = Vec::new();
    for (name, blobs) in &all {
        let r = replay(blobs);
        applied += r.stat_updates;
        without_player += r.stat_updates_without_player;
        stale += r.stat_updates_stale;
        if r.stat_updates_stale > 0 {
            stale_by_session.push((name.clone(), r.stat_updates_stale));
        }
        early += r.stat_updates_before_create;
        if r.stat_updates_before_create > 0 {
            early_by_session.push((name.clone(), r.stat_updates_before_create));
        }
        for (op, id, key) in &r.early {
            assert!(
                r.created_ever.contains(id),
                "{name}: {op:#06X} on {key:?} names {id:?}, which the session never creates"
            );
        }
    }
    let expected = all
        .iter()
        .flat_map(|(_, bs)| bs)
        .filter(|b| b.dir == Dir::S2c && matches!(b.opcode(), 0x02CD..=0x02D4 | 0x02D7..=0x02DA))
        .count();
    assert!(applied > 0);
    assert_eq!(
        applied + stale + early + without_player,
        expected,
        "every recorded quality update is accounted for"
    );
    assert_eq!(
        stale, 0,
        "{stale} in-order updates were rejected by the sequence gate"
    );
    assert_eq!(
        stale_by_session
            .iter()
            .map(|(n, c)| (n.as_str(), *c))
            .collect::<Vec<_>>(),
        Vec::<(&str, usize)>::new(),
        "no recording in the corpus has a sequence-gate rejection left"
    );
    assert!(
        early > 0,
        "recordings exercise updates before object creation"
    );
    assert_eq!(
        early_by_session.iter().map(|(_, n)| n).sum::<usize>(),
        early
    );
    eprintln!("{applied} quality updates applied, {early} arrived before their object's create");
}

#[test]
fn the_enter_world_edge_relatches_the_player_and_retires_the_old_sequence_gate() {
    let all = corpus::load_all();
    let (_, blobs) = all
        .iter()
        .find(|(n, _)| n == "requested-death-vitae-salvage")
        .expect("`requested-death-vitae-salvage` is locked in fixtures/message-corpus; a missing fixture is a failure, not a skip");
    let r = replay(blobs);
    assert_eq!(
        r.create_players,
        blobs
            .iter()
            .filter(|b| b.dir == Dir::S2c && b.opcode() == OP_LOGIN_CREATE_PLAYER)
            .count(),
        "six character sessions on one connection"
    );
    assert_eq!(
        r.latched,
        blobs
            .iter()
            .filter(|b| b.dir == Dir::S2c && b.opcode() == OP_LOGIN_CREATE_PLAYER)
            .map(
                |b| read_body::<dereth_protocol::objects::LoginCreatePlayer>(b.body())
                    .unwrap()
                    .player_id
            )
            .collect::<Vec<_>>(),
        "three characters, and the player id is the one the capture's own 0xF746 names"
    );
    assert_eq!(
        r.stat_updates_stale, 0,
        "no in-order quality update is refused by a stamper the last session left behind"
    );
}

#[test]
fn the_ac_qualities_trailing_dword_is_not_the_wcid() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut checked = 0usize;
    let mut agreed = 0usize;
    let mut table: Vec<(String, u32, Option<u32>)> = Vec::new();
    for (name, blobs) in &all {
        let player = blobs.iter().find_map(|b| {
            (b.dir == Dir::S2c && b.opcode() == OP_LOGIN_CREATE_PLAYER)
                .then(|| read_body::<dereth_protocol::objects::LoginCreatePlayer>(b.body()).ok())
                .flatten()
                .map(|m| m.player_id)
        });
        let Some(player) = player else { continue };
        let pwd_wcid = blobs.iter().find_map(|b| {
            if b.dir != Dir::S2c || b.opcode() != OP_ITEM_CREATE_OBJECT {
                return None;
            }
            let m = read_body::<dereth_protocol::objects::ItemCreateObject>(b.body()).ok()?;
            (m.0.id == player).then_some(m.0.wdesc.wcid)
        });
        for b in blobs.iter().filter(|b| b.dir == Dir::S2c) {
            if b.opcode() != OP_GAME_EVENT {
                continue;
            }
            let Ok(ui) = split_ui_blob(&b.payload) else {
                continue;
            };
            if ui.sub_type.0 != EV_PLAYER_DESCRIPTION {
                continue;
            }
            let mut rr = ui.body;
            let Ok(q) = dereth_protocol::types::qualities::AcQualities::read(&mut rr) else {
                continue;
            };
            checked += 1;
            assert_eq!(
                q.has_health, 1,
                "{name}: the trailing dword is 1 in every recorded player description"
            );
            if Some(q.has_health) == pwd_wcid {
                agreed += 1;
            }
            table.push((name.clone(), q.has_health, pwd_wcid));
        }
    }
    assert_eq!(
        checked,
        description_count(),
        "Login_PlayerDescription events in the corpus"
    );

    let dwords: std::collections::BTreeSet<u32> = table.iter().map(|(_, d, _)| *d).collect();
    let wcids: std::collections::BTreeSet<Option<u32>> = table.iter().map(|(_, _, w)| *w).collect();
    assert_eq!(
        dwords.iter().copied().collect::<Vec<_>>(),
        vec![1],
        "the trailing dword takes exactly one value across the whole corpus"
    );
    assert!(
        wcids.len() > 1,
        "the class id varies while the health flag does not"
    );
    assert!(
        agreed > 0 && checked > agreed,
        "recordings include both equal and unequal class ids"
    );
}

#[test]
fn every_sender_reproduces_the_captured_request_byte_for_byte() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut checked: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();

    for (name, blobs) in &all {
        for b in blobs.iter().filter(|b| b.dir == Dir::C2s) {
            if b.opcode() != OP_GAME_ACTION {
                continue;
            }
            let Ok(a) = unpack_action(&b.payload) else {
                continue;
            };
            let sub = a.sub_type.0;
            let Some(produced) = reproduce(sub, a.body) else {
                continue;
            };
            let encoded = encode(a.stamp, &produced);
            assert_eq!(
                encoded, b.payload,
                "{name}: sub-opcode {sub:#06X} at t={} re-encoded differently",
                b.t
            );
            *checked.entry(sub).or_insert(0) += 1;
        }
    }
    for sub in [
        0x0019, 0x001A, 0x0035, 0x0036, 0x004A, 0x0053, 0x00C8, 0x00CD, 0x01B7, 0x01BF,
    ] {
        let expected = all
            .iter()
            .flat_map(|(_, bs)| bs)
            .filter(|b| b.dir == Dir::C2s && b.as_action().is_some_and(|(_, op, _)| op == sub))
            .count();
        assert!(expected > 0, "sender {sub:#06X} has recorded examples");
        assert_eq!(
            checked.get(&sub).copied().unwrap_or(0),
            expected,
            "every supported request is reproduced"
        );
    }
    eprintln!("senders reproduced byte-for-byte: {checked:?}");
}

#[test]
fn the_notice_stream_is_deterministic_and_covers_the_documented_variants() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut with_notices = 0usize;
    for (name, blobs) in &all {
        let a = replay(blobs);
        let b = replay(blobs);
        assert_eq!(
            a.notices.len(),
            b.notices.len(),
            "{name}: notice count is not deterministic"
        );
        assert_eq!(
            a.notices, b.notices,
            "{name}: the notice sequence is not deterministic"
        );
        if a.create_players == 0 {
            assert!(
                a.notices.is_empty(),
                "{name}: notices without a character in world"
            );
            continue;
        }
        with_notices += 1;
        assert!(!a.notices.is_empty(), "{name}: no notices at all");

        assert!(
            a.notices
                .iter()
                .any(|n| matches!(n, Notice::ObjectCreated(_))),
            "{name}: no ObjectCreated"
        );
        assert!(
            a.notices
                .iter()
                .any(|n| matches!(n, Notice::ItemMoved { .. })),
            "{name}: no ItemMoved"
        );
        assert!(
            a.notices
                .iter()
                .any(|n| matches!(n, Notice::ItemAttributesChanged { .. })),
            "{name}: no ItemAttributesChanged"
        );
    }
    assert_eq!(
        with_notices,
        recorded_world_sessions(),
        "every capture that reaches the world raises these notices"
    );
}

#[test]
fn the_recordings_exercise_every_supported_notice_kind() {
    let mut seen: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    for (_, blobs) in corpus::load_all() {
        for n in replay(&blobs).notices {
            seen.insert(name_of(&n));
        }
    }
    let want = [
        "AppraisalReady",
        "AttemptFailed",
        "DisplayString",
        "InventoryChanged",
        "ItemAttributesChanged",
        "ItemMoved",
        "ObjectCreated",
        "ObjectDeleted",
        "StatUpdated",
    ];
    for kind in want {
        assert!(seen.contains(kind), "missing notice {kind}");
    }
}

fn input_count(op: u32) -> usize {
    corpus::load_all()
        .iter()
        .flat_map(|(_, bs)| bs)
        .filter(|b| b.dir == Dir::S2c && b.opcode() == op)
        .count()
}
fn description_count() -> usize {
    corpus::load_all()
        .iter()
        .flat_map(|(_, bs)| bs)
        .filter(|b| {
            b.dir == Dir::S2c
                && b.as_event()
                    .is_some_and(|(_, _, op, _)| op == EV_PLAYER_DESCRIPTION)
        })
        .count()
}
