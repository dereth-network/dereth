//! Contracts for sound event corpus.
//! Fixture: shared recorded messages and synthetic state.

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_primitives::NetQueue;
use dereth_protocol::objects::EffectsSoundEvent;
use dereth_protocol::types::PhysicsDesc;
use dereth_protocol::{read_body_padded, Message, Opcode, Reader};

const F750: u32 = 0xF750;

const F745: u32 = 0xF745;
const F7DB: u32 = 0xF7DB;

fn body(b: &CorpusBlob) -> &[u8] {
    &b.payload[4..]
}

#[test]
fn the_opcode_constant_is_f750() {
    assert_eq!(Opcode::EFFECTS_SOUND_EVENT.0, F750);
    assert_eq!(EffectsSoundEvent::OPCODE.0, F750);
}

#[test]
fn every_recorded_sound_event_is_server_to_client() {
    let sounds: Vec<_> = Corpus::shared_all()
        .into_iter()
        .flat_map(|c| &c.blobs)
        .filter(|b| b.opcode == F750)
        .collect();
    assert!(!sounds.is_empty());
    for b in sounds {
        assert_eq!(b.dir, Direction::ServerToClient);
    }
}

#[test]
fn every_recorded_sound_field_matches_its_wire_value() {
    let mut checked = 0;
    let mut volumes = std::collections::BTreeSet::new();
    for corpus in Corpus::shared_all() {
        for b in corpus.blobs.iter().filter(|b| b.opcode == F750) {
            assert_eq!(b.queue, NetQueue::WorldObjects);
            assert_eq!(b.payload.len(), 16);
            let m: EffectsSoundEvent = read_body_padded(body(b)).expect("every sound decodes");
            assert_eq!(
                m.id.0,
                u32::from_le_bytes(b.payload[4..8].try_into().unwrap())
            );
            assert_eq!(
                m.sound_type,
                i32::from_le_bytes(b.payload[8..12].try_into().unwrap())
            );
            assert!(m.sound_type > 0 && m.sound_type < 205);
            assert_eq!(
                m.volume.to_bits(),
                u32::from_le_bytes(b.payload[12..16].try_into().unwrap())
            );
            volumes.insert(m.volume.to_bits());
            checked += 1;
        }
    }
    assert!(checked > 0);
    assert!(
        volumes.len() > 1,
        "the fixtures distinguish message volume from a constant"
    );
}

/// Behaviour: audio.sound-event.every-recorded-sound-names-an-object-the-session-created
#[test]
fn every_sound_event_names_an_object_its_own_capture_created() {
    let mut checked = 0usize;
    for corpus in Corpus::shared_all() {
        let name = &corpus.name;
        let mut created = std::collections::BTreeSet::new();
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient {
                continue;
            }
            if b.opcode == F745 || b.opcode == F7DB {
                let mut r = Reader::body(body(b));
                if let Ok(id) = r.u32() {
                    created.insert(id);
                }
            }
        }
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient && b.opcode == F750)
        {
            let m: EffectsSoundEvent = read_body_padded(body(b)).expect("decode");
            assert!(
                created.contains(&m.id.0),
                "{name}: 0x{:08X} was never created",
                m.id.0
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn every_sounded_object_overrides_its_setups_sound_table() {
    let mut sounded_creates = 0usize;
    let mut with_stable = 0usize;
    let mut with_setup = 0usize;
    let mut sessions_with_creates = 0usize;
    let mut tables = std::collections::BTreeSet::new();
    let mut setups = std::collections::BTreeSet::new();
    for corpus in Corpus::shared_all() {
        let mut sounded = std::collections::BTreeSet::new();
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient && b.opcode == F750)
        {
            let m: EffectsSoundEvent = read_body_padded(body(b)).expect("decode");
            sounded.insert(m.id.0);
        }
        let before = sounded_creates;
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || (b.opcode != F745 && b.opcode != F7DB) {
                continue;
            }
            let mut r = Reader::body(body(b));
            let Ok(id) = r.u32() else { continue };
            if !sounded.contains(&id) {
                continue;
            }
            let Ok(_) = dereth_protocol::types::ObjDesc::read(&mut r) else {
                continue;
            };
            let Ok(pd) = PhysicsDesc::read(&mut r) else {
                continue;
            };
            sounded_creates += 1;
            with_setup += usize::from(pd.setup_id.is_some());
            if let Some(sid) = pd.setup_id {
                setups.insert(sid);
            }
            if let Some(s) = pd.stable_id {
                with_stable += 1;
                tables.insert(s);
            }
        }
        sessions_with_creates += usize::from(sounded_creates > before);
    }
    assert_eq!(
        sounded_creates, with_stable,
        "creates of the 34 objects the server sounds"
    );
    assert_eq!(
        with_stable, with_setup,
        "every one carries a stable sound-table override"
    );
    assert_eq!(
        with_setup, sounded_creates,
        "and a setup record, whose default the override replaces"
    );
    assert!(sessions_with_creates > 0 && sounded_creates > 0);
    assert!(
        tables.iter().all(|t| t >> 24 == 0x20),
        "every override is a 0x20 sound table: {tables:?}"
    );
    assert!(
        setups.iter().all(|s| s >> 24 == 0x02),
        "sound sources have setup records"
    );
}
