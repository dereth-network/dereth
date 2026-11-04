//! Divergence: V497
//! A Launch world names physics scripts and sounds by the rows the 1999 files hold, and a client
//! plays what it receives as sent, as it does from a server that speaks those numbers itself: the
//! number a later script or sound goes out as finds the 1999 row with what the later files hold at
//! the later number. Fixture: the hash-pinned launch dats of `EMPYREAN_TEST_LAUNCH_FIXTURE` and the
//! retail dats of `DERETH_TEST_DAT_DIR`.

use std::sync::Arc;

use dereth_assets::{PhysicsScriptTable, SoundTable};
use dereth_primitives::ContainerEra::Classic;
use dereth_world_data::effect_numbering::{script_in_files, sound_in_files};
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::{PlayScript, Sound};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState};
use empyrean_world::network::game_messages::game_message::{
    enqueue_send, start_capture, take_sent,
};
use empyrean_world::network::game_messages::messages::game_message_script::game_message_script;
use empyrean_world::network::game_messages::messages::game_message_sound::game_message_sound;
use empyrean_world::sessions::SessionData;

use super::{fixture, world};

fn end_of_retail() -> Arc<DatManager> {
    let real = RealDats::open(&dereth_dat::testing::dat_dir()).expect("the retail dats");
    DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
}

/// The tables hold the files' own numbers: 1999's `Create` is at 87, where a client looking up what
/// a 1999 server sends finds it, and it plays what the later files play at 88.
#[test]
fn a_later_script_number_goes_out_as_the_1999_row_with_the_same_scripts() {
    let (then, later) = (&fixture().dats, end_of_retail());
    let (mut compared, mut differ) = (0, Vec::new());
    for id in 0x3400_0000u32..0x3400_0100 {
        let (Some(a), Some(b)) = (
            then.portal_dat().read_from_dat::<PhysicsScriptTable>(id),
            later.portal_dat().read_from_dat::<PhysicsScriptTable>(id),
        ) else {
            continue;
        };
        for script in [
            PlayScript::AttribUpRed,
            PlayScript::Fizzle,
            PlayScript::PortalEntry,
            PlayScript::PortalExit,
            PlayScript::Create,
            PlayScript::Destroy,
            PlayScript::LevelUp,
        ] {
            let Some(row) = script_in_files(Classic, script.0).and_then(|k| a.script_table.get(&k))
            else {
                continue;
            };
            compared += 1;
            if b.script_table.get(&script.0) != Some(row) {
                differ.push((id, script.0));
            }
        }
    }
    assert!(compared > 100, "only {compared} rows compared");
    assert!(differ.is_empty(), "{differ:x?} of {compared}");
}

/// The player's sound table and a door's: each later sound goes out as the 1999 row with its waves.
#[test]
fn a_later_sound_number_goes_out_as_the_1999_row_with_the_same_waves() {
    let (then, later) = (&fixture().dats, end_of_retail());
    let row = |t: &SoundTable, key: u32| {
        t.nodes
            .iter()
            .find(|n| n.key == key)
            .map(|n| n.data.iter().map(|e| e.sound_id).collect::<Vec<_>>())
    };
    let table = |d: &DatManager, id: u32| {
        d.portal_dat()
            .read_from_dat::<SoundTable>(id)
            .expect("the table")
    };
    const PLAYER: u32 = 0x2000_0001;
    for sound in [
        Sound::RaiseTrait,
        Sound::WieldObject,
        Sound::UnwieldObject,
        Sound::ReceiveItem,
        Sound::PickUpItem,
        Sound::DropItem,
        Sound::ResistSpell,
        Sound::SpellExpire,
        Sound::ItemManaDepleted,
    ] {
        let (a, b) = (table(then, PLAYER), table(&later, PLAYER));
        let raw = row(&a, sound_in_files(Classic, sound.0));
        assert!(raw.is_some(), "{:x} has a 1999 row", sound.0);
        assert_eq!(raw, row(&b, sound.0), "{:x}", sound.0);
        assert!(
            row(&a, sound.0).is_none(),
            "{:x} is not a 1999 row",
            sound.0
        );
    }
    // The lock sounds are in no player table: every 1999 table with one matches the later table.
    let mut locks = 0;
    for id in 0x2000_0000u32..0x2000_0100 {
        let (Some(a), Some(b)) = (
            then.portal_dat().read_from_dat::<SoundTable>(id),
            later.portal_dat().read_from_dat::<SoundTable>(id),
        ) else {
            continue;
        };
        for sound in [
            Sound::PicklockFail,
            Sound::LockSuccess,
            Sound::OpenFailDueToLock,
        ] {
            if let Some(waves) = row(&a, sound_in_files(Classic, sound.0)) {
                locks += 1;
                assert_eq!(Some(waves), row(&b, sound.0), "{id:x} {:x}", sound.0);
            }
        }
    }
    assert!(locks >= 9, "only {locks} lock rows");
}

/// A spell's scripts are in the later numbering on the server, for the spells the two sets share
/// and that kept their effects, so they go out as the file's own number.
#[test]
fn a_launch_spells_scripts_are_in_the_later_numbering() {
    let then = fixture().dats.portal_dat().spell_table();
    let later = end_of_retail();
    let later = later.portal_dat().spell_table();
    let (mut same, mut compared) = (0, 0);
    for (id, spell) in &then.spells {
        let Some(other) = later.spells.get(id) else {
            continue;
        };
        compared += 1;
        if (
            spell.caster_effect,
            spell.target_effect,
            spell.fizzle_effect,
        ) == (
            other.caster_effect,
            other.target_effect,
            other.fizzle_effect,
        ) {
            same += 1;
        }
    }
    assert!(compared > 500, "only {compared} spells shared");
    assert!(
        same * 10 > compared * 9,
        "{same} of {compared} name the later files' scripts"
    );
}

/// What a Launch world sends for a later script or sound is the 1999 row, and a script below the
/// moved ones is unchanged.
#[test]
fn a_launch_world_sends_the_rows_its_files_hold() {
    const SESSION: SessionId = SessionId {
        client_id: 1,
        generation: 1,
    };
    let mut w = world();
    w.sessions.insert(
        SESSION,
        SessionData {
            state: SessionState::WorldConnected,
            ..Default::default()
        },
    );
    let guid = ObjectGuid::new(0x5000_0001);
    let number = |bytes: &[u8]| u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    start_capture();
    enqueue_send(
        &mut w,
        SESSION,
        game_message_script(guid, PlayScript::Create, 1.0),
    );
    enqueue_send(
        &mut w,
        SESSION,
        game_message_sound(guid, Sound::PickUpItem, 1.0),
    );
    enqueue_send(
        &mut w,
        SESSION,
        game_message_script(guid, PlayScript::AttribUpRed, 1.0),
    );
    let sent = take_sent();
    assert_eq!(number(&sent[0].2), PlayScript::Create.0 - 1);
    assert_eq!(number(&sent[1].2), 0x7A);
    assert_eq!(
        number(&sent[2].2),
        PlayScript::AttribUpRed.0,
        "below the moved rows"
    );
}

/// The Launch world base's creatures name the row just above their stored default script.
#[test]
fn a_launch_creatures_default_script_goes_out_one_above_what_it_stores() {
    use empyrean_world::factories::world_object_factory as factory;
    use empyrean_world::managers::guid_manager;
    use empyrean_world::world_objects::world_object::CtorEnv;
    use empyrean_world::world_objects::world_object_networking::default_script_on_the_wire;
    const OLTHOI_NOBLE: u32 = 212;
    let mut w = world();
    let weenie = w
        .content
        .get_cached_weenie(OLTHOI_NOBLE)
        .expect("the Olthoi");
    let guid = guid_manager::new_dynamic_guid(&mut w);
    let o = CtorEnv::with_world(&w, |env| {
        factory::create_world_object(env, Some(weenie), guid)
    })
    .expect("constructible");
    assert_eq!(o.default_script_id(), Some(86));
    assert_eq!(default_script_on_the_wire(&w, &o), 87);
}
