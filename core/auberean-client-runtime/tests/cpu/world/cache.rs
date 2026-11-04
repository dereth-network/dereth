//! The client's one data cache: a record the server sends is read from then on by every part of
//! the client (the scripts, the text, the requests), and an object's creation asks it for the
//! records the object is made from.

use std::sync::Arc;

use auberean_assets::entity::Entity;
use auberean_assets::requests::{Listener, GATHER_NONE};
use auberean_assets::strings::{StringEntry, StringTable};
use auberean_assets::tables::IdMap;
use auberean_client_runtime::creation;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::host::model_natives;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_dat::{QualifiedId, RecordType};
use auberean_protocol::login::{DataRecord, DataRefused};
use auberean_protocol::physics::{CreateObject, ObjectOnly};
use auberean_protocol::types::{ObjectStamp, PartGroupDesc, VisualDesc, WeenieDesc};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::library::Library;
use auberean_script::Program;

use super::download::{file, file_with};

const PORTAL: u64 = 0x1_0000_0000;
const CELL: u64 = 0x2_0000_0001;
const LOCAL: u64 = 0x3_8000_0001;
/// A blob on the login connection, taken as it comes.
const LOGIN: u64 = 0x2300_0000_0000_0000;
/// Messages that take effect as they come, in order.
const NOW: u64 = 3 << 56;

fn client(scripts: bool) -> Client {
    let portal = file(PORTAL, &[]);
    let program = || {
        let lib = Library::default();
        let table = model_natives(&lib);
        Arc::new(Program::link(lib, table).expect("links"))
    };
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![portal.clone(), file(CELL, &[]), file(LOCAL, &[])],
                language: ENGLISH,
            },
            scripts: scripts.then(|| (portal, program())),
        },
    )
}

/// The records the client asked the server for, in order.
fn asked(c: &Client) -> Vec<(u32, u32)> {
    c.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) => Some((r.kind, r.id)),
            _ => None,
        })
        .collect()
}

fn send_record<T: auberean_assets::Encode>(
    c: &mut Client,
    n: u64,
    dat_file: u64,
    q: QualifiedId,
    v: &T,
) {
    c.take_message(
        LOGIN | n,
        ServerMessage::DataRecord(DataRecord {
            dat_file,
            kind: q.kind,
            id: q.id,
            size: 0,
            bytes: auberean_assets::encode(q.id, v),
        }),
    );
}

fn refuse(c: &mut Client, n: u64, q: QualifiedId) {
    c.take_message(
        LOGIN | n,
        ServerMessage::DataRefused(DataRefused {
            kind: q.kind,
            id: q.id,
            error: 1,
        }),
    );
}

/// A record the server sends for a request is stored, and from then on the scripts read it
/// through the same records' caches the requests decoded it into; a string table it sends is
/// written from by the scripts' text as the files' own tables are.
///
/// Behaviour: data.cache.one-cache-for-the-whole-client
#[test]
fn a_record_the_server_sends_is_read_by_the_scripts_and_their_text_through_the_one_cache() {
    let mut c = client(true);
    let map = QualifiedId {
        id: 0x2800_0001,
        kind: RecordType::ID_MAP.0,
    };
    let table = QualifiedId {
        id: 0x2500_0001,
        kind: RecordType::STRING_TABLE.0,
    };
    // Nothing reads them before they come.
    let before = c.with_host(|h| {
        let db = auberean_script::Host::database(h).expect("the data files");
        let got = db.get::<IdMap>(map.id).is_some();
        let text = auberean_text::Source::table(&mut h.text, table.id);
        (got, text.is_some())
    });
    assert_eq!(before, Some((false, false)));
    let to = Listener { to: 1, user: 5 };
    c.data_download_mut().get(GATHER_NONE, map, Some(to));
    c.upkeep();
    assert_eq!(asked(&c), vec![(map.kind, map.id)]);
    send_record(&mut c, 1, PORTAL, map, &IdMap::default());
    let mut t = StringTable::default();
    t.entries.push(StringEntry {
        token: 7,
        ..StringEntry::default()
    });
    send_record(&mut c, 2, LOCAL, table, &t);
    c.upkeep();
    let cache = std::ptr::from_ref(c.data_download().cache().database()) as usize;
    let after = c.with_host(|h| {
        let db = auberean_script::Host::database(h).expect("the data files");
        let same = std::ptr::from_ref(db) as usize == cache;
        let got = db.get::<IdMap>(map.id).is_some();
        let text = auberean_text::Source::table(&mut h.text, table.id);
        (same, got, text.is_some_and(|t| t.entry(7).is_some()))
    });
    assert_eq!(after, Some((true, true, true)));
}

/// An object's creation asks the data cache for the records it is made from: its visual
/// description's, then its script state and qualities records by its entity, then the entity;
/// those the files lack are asked of the server. A record sent is stored and read from then on;
/// a refused one ends its request, and is asked for again only by the next object made from it.
///
/// Behaviour: data.cache.an-objects-creation-asks-for-its-records
#[test]
fn an_objects_creation_asks_the_server_for_the_records_its_files_lack_and_a_refusal_ends_the_ask() {
    let mut c = client(false);
    let entity = 0x4700_0005;
    let make = |id: u64, stamp: u16| {
        let physics = auberean_protocol::types::PhysicsDesc {
            instance_stamp: stamp,
            ..Default::default()
        };
        CreateObject {
            id,
            visual: VisualDesc {
                motion_interp: RecordType::MOTION_DESC.base_id() + 1,
                modes: RecordType::VALID_MODES.base_id() + 1,
                global: PartGroupDesc {
                    setup: RecordType::SETUP.base_id() + 2,
                    ..PartGroupDesc::default()
                },
                ..VisualDesc::default()
            },
            physics,
            weenie: WeenieDesc {
                mask: WeenieDesc::ENTITY,
                entity_did: entity,
                ..WeenieDesc::default()
            },
        }
    };
    let wanted = creation::records(&make(1, 1));
    let q = |id: u32, kind: RecordType| QualifiedId { id, kind: kind.0 };
    let ids: Vec<u32> = wanted.iter().map(|q| q.id).collect();
    // The valid modes are left out; the script state and the qualities follow the entity's id.
    assert!(!ids.contains(&(RecordType::VALID_MODES.base_id() + 1)));
    for want in [
        q(RecordType::SETUP.base_id() + 2, RecordType::SETUP),
        q(
            RecordType::MOTION_DESC.base_id() + 1,
            RecordType::MOTION_DESC,
        ),
        q(0x7100_0005, RecordType::SCRIPT_STATE),
        q(0x8100_0005, RecordType::QUALITIES),
        q(entity, RecordType::ENTITY),
    ] {
        assert!(wanted.contains(&want), "{want:?} in {wanted:?}");
    }
    c.take_message(
        NOW,
        ServerMessage::CreateObject(make(0x8130_0000_1000_0001, 1)),
    );
    c.upkeep();
    let mut first: Vec<(u32, u32)> = asked(&c);
    first.sort_unstable();
    let mut all: Vec<(u32, u32)> = wanted.iter().map(|q| (q.kind, q.id)).collect();
    all.sort_unstable();
    assert_eq!(
        first, all,
        "every record is missing, so every one is asked for"
    );
    // The entity comes; every other record is refused.
    let stored = Entity {
        flags: 1,
        id: Some(entity),
        ..Entity::default()
    };
    send_record(&mut c, 1, PORTAL, q(entity, RecordType::ENTITY), &stored);
    for (n, r) in wanted.iter().filter(|r| r.id != entity).enumerate() {
        refuse(&mut c, 2 + n as u64, *r);
    }
    c.upkeep();
    assert!(c.data_download().requests.pending().is_empty());
    let db = c.data_download().cache().database();
    assert_eq!(db.get::<Entity>(entity).and_then(|e| e.id), Some(entity));
    let n = asked(&c).len();
    c.upkeep();
    assert_eq!(asked(&c).len(), n, "a refusal is not asked again by itself");
    // The next object made from the entity asks again for what was refused, not for the entity.
    c.take_message(
        NOW,
        ServerMessage::CreateObject(make(0x8130_0000_1000_0002, 1)),
    );
    c.upkeep();
    let again: Vec<(u32, u32)> = asked(&c)[n..].to_vec();
    assert_eq!(again.len(), wanted.len() - 1);
    assert!(!again.contains(&(RecordType::ENTITY.0, entity)));
}

/// A qualities record no file holds answers nothing until the server sends it, and from then on
/// the object made from it reads its qualities there.
///
/// Behaviour: data.cache.a-qualities-record-that-comes-later-is-read
#[test]
fn a_qualities_record_the_server_sends_later_is_read_by_an_object_made_before_it() {
    use auberean_client_runtime::repository::{answer, INT};
    let mut c = client(true);
    let id = 0x8130_0000_1000_0001;
    c.take_message(
        NOW,
        ServerMessage::CreateObject(CreateObject {
            id,
            weenie: WeenieDesc {
                mask: WeenieDesc::ENTITY,
                entity_did: 0x4700_0005,
                ..WeenieDesc::default()
            },
            ..CreateObject::default()
        }),
    );
    let quality = |c: &mut Client| c.with_host(|h| answer(h, &INT, id, 7)).expect("a host");
    assert_eq!(quality(&mut c), None, "no file holds it");
    // A qualities record of one integer: its flags, its id, then its table.
    let mut bytes = Vec::new();
    for word in [0x2u32, 0x8100_0005, 1, 7, 42] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    c.take_message(
        LOGIN | 1,
        ServerMessage::DataRecord(DataRecord {
            dat_file: PORTAL,
            kind: RecordType::QUALITIES.0,
            id: 0x8100_0005,
            size: 0,
            bytes,
        }),
    );
    c.upkeep();
    assert_eq!(quality(&mut c), Some(42), "it came");
}

/// An object whose records the files lack is made at once: its records are asked for behind it.
///
/// Behaviour: data.cache.objects-made-before-their-records
#[test]
fn an_object_is_made_at_once_while_its_records_are_asked_for() {
    let mut c = client(false);
    let id = 0x8130_0000_1000_0001;
    c.take_message(
        NOW,
        ServerMessage::CreateObject(CreateObject {
            id,
            weenie: WeenieDesc {
                mask: WeenieDesc::ENTITY,
                entity_did: 0x4700_0005,
                ..WeenieDesc::default()
            },
            ..CreateObject::default()
        }),
    );
    assert!(c.model.object(id).is_some());
    assert!(!c.data_download().requests.pending().is_empty());
}

/// An object is made from the records its creation read: they go into their caches, the records
/// they name too, and the object holds them while it lives. The next object made from them finds
/// them in memory and reads none; once the objects are destroyed, nothing holds them.
///
/// Behaviour: data.cache.objects-hold-the-records-they-are-made-from
#[test]
fn an_object_holds_the_records_it_was_made_from_and_the_next_finds_them_in_memory() {
    let parent = 0x4700_0006;
    let entity = 0x4700_0005;
    let named = Entity {
        flags: 1 | 8,
        id: Some(entity),
        object: Some(parent),
        ..Entity::default()
    };
    let top = Entity {
        flags: 1,
        id: Some(parent),
        ..Entity::default()
    };
    let portal = file_with(
        PORTAL,
        &[
            (entity, auberean_assets::encode(entity, &named)),
            (parent, auberean_assets::encode(parent, &top)),
        ],
    );
    let mut c = Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![portal, file(CELL, &[]), file(LOCAL, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    );
    let make = |id: u64| CreateObject {
        id,
        weenie: WeenieDesc {
            mask: WeenieDesc::ENTITY,
            entity_did: entity,
            ..WeenieDesc::default()
        },
        ..CreateObject::default()
    };
    // The script state and qualities records are in no file: refused, as the server would.
    let refuse_lacking = |c: &mut Client, from: usize, n: u64| {
        let lacking: Vec<(u32, u32)> = asked(c)[from..].to_vec();
        for (i, (kind, id)) in lacking.iter().enumerate() {
            refuse(
                c,
                n + i as u64,
                QualifiedId {
                    id: *id,
                    kind: *kind,
                },
            );
        }
        lacking.len()
    };
    let first = 0x8130_0000_1000_0001;
    c.take_message(NOW, ServerMessage::CreateObject(make(first)));
    assert!(
        c.data_download().requests.queued() > 0,
        "the entity is read from the file"
    );
    c.upkeep();
    assert_eq!(refuse_lacking(&mut c, 0, 1), 2);
    c.upkeep();
    c.upkeep();
    assert!(c.data_download().requests.pending().is_empty());
    assert_eq!(
        c.data_download().held_by(first),
        2,
        "the entity and the entity it names"
    );
    let second = 0x8130_0000_1000_0002;
    let n = asked(&c).len();
    c.take_message(NOW, ServerMessage::CreateObject(make(second)));
    assert_eq!(
        c.data_download().requests.queued(),
        0,
        "nothing is read: the entity is in memory"
    );
    c.upkeep();
    refuse_lacking(&mut c, n, 10);
    c.upkeep();
    c.upkeep();
    assert!(c.data_download().held_by(second) >= 1);
    for id in [first, second] {
        c.take_message(
            NOW,
            ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
                id,
                ..ObjectStamp::default()
            })),
        );
    }
    assert_eq!(c.data_download().held_by(first), 0);
    assert_eq!(c.data_download().held_by(second), 0);
}

/// Every asker of the data cache's requests has a number of its own: an object's creation, the
/// string downloads and the scene's cells are each told only their own batches.
///
/// Behaviour: data.cache.each-asker-has-its-own-number
#[test]
fn each_asker_of_the_data_cache_has_a_number_of_its_own() {
    use auberean_client_runtime::download::listeners;
    let mut all = listeners::ALL.to_vec();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), listeners::ALL.len(), "{:x?}", listeners::ALL);
    assert_eq!(creation::CREATION.to, listeners::CREATION);
    assert_eq!(
        auberean_client_runtime::strings::STRINGS,
        listeners::STRINGS
    );
    assert_eq!(listeners::CELLS, u32::MAX - 2);
}

/// Leaving the server resets the data cache: a goodbye goes out, the download is idle whatever it
/// was doing, and every batch and kept get is let go, so nobody waits for a record the server will
/// not send.
///
/// Behaviour: data.download.leaving-the-server-resets-the-cache
#[test]
fn leaving_the_server_resets_the_data_cache() {
    use auberean_dat::disk::download::DddState;
    let mut c = client(false);
    let to = Listener { to: 1, user: 5 };
    let record = QualifiedId {
        id: 0x2800_0001,
        kind: RecordType::ID_MAP.0,
    };
    c.data_download_mut().get(GATHER_NONE, record, Some(to));
    c.upkeep();
    assert_eq!(
        asked(&c),
        vec![(record.kind, record.id)],
        "it waits on the server"
    );
    c.data_download_mut().download.state = DddState::Patching;
    assert_eq!(
        c.data_download().requests.live_batches(),
        1,
        "one batch waits"
    );
    assert!(c.data_download().requests.live() > 0, "for its record");
    c.disconnect();
    let d = c.data_download();
    assert_eq!(d.download.state, DddState::Idle);
    assert_eq!(d.requests.live_batches(), 0, "the batch let go");
    assert!(d.requests.pending().is_empty(), "no get kept");
}
