//! ACE: Source/ACE.Server/Factories/PlayerFactory.cs::Create
//! A character is created over the wire (Ok, first guid, listed); the same name in another case
//! answers NameInUse once; a taboo name answers NameBanned.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::NetQueue;
use dereth_protocol::login::{CharGenVerificationResponse, CharacterSendCharGenResult};
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_entity::enums::{PropertyInt, WeenieType};
use empyrean_net::SessionId;
use empyrean_testkit::{ClientId, ClientStatus, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::world_manager::WorldStatusState;

const ACCOUNT: &str = "acct";
const ACCOUNT_ID: u32 = 1;

fn logged_in() -> (TestServer, ClientId, SessionId) {
    let mut ts = TestServer::with_dats(crate::support::login_fixture::dats(&[0x0100_0001], true));
    ts.world.content = Arc::new(
        MemContent::new().weenie(
            Weenie::new(1, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        ),
    );
    ts.world.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);

    crate::support::login_fixture::connect_account(ts, ACCOUNT)
}

fn request(name: &str) -> CharacterSendCharGenResult {
    crate::support::login_fixture::request(ACCOUNT, name)
}

fn create(ts: &mut TestServer, id: ClientId, name: &str) -> Vec<CharGenVerificationResponse> {
    let before = ts.received::<CharGenVerificationResponse>(id).len();
    ts.send_message(id, NetQueue::Logon, &request(name));
    ts.advance(0.5);
    ts.received::<CharGenVerificationResponse>(id)
        .split_off(before)
}

#[test]
fn a_character_is_created_over_the_wire_then_its_name_is_taken() {
    let (mut ts, id, session) = logged_in();

    let first = create(&mut ts, id, "Aldric");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].response_type, 1, "Ok");
    assert_eq!(first[0].identity.gid.0, 0x5000_0001);
    assert_eq!(first[0].identity.name, "Aldric");

    let listed: BTreeMap<u32, String> = ts
        .shard()
        .get_characters(ACCOUNT_ID, false)
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    assert_eq!(listed, BTreeMap::from([(0x5000_0001, "Aldric".to_owned())]));
    assert_eq!(
        ts.world.sessions.get(session).map(|s| s.characters.len()),
        Some(1)
    );

    // The same name again, in another case: NameInUse, once (V242; ACE answered twice).
    let again = create(&mut ts, id, "ALDRIC");
    assert_eq!(
        again.iter().map(|r| r.response_type).collect::<Vec<_>>(),
        [3]
    );

    // A taboo name: NameBanned.
    let banned = create(&mut ts, id, "Tabooword");
    assert_eq!(
        banned.iter().map(|r| r.response_type).collect::<Vec<_>>(),
        [4]
    );

    assert_eq!(ts.shard().get_characters(ACCOUNT_ID, false).len(), 1);
    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
}

pub(crate) use crate::support::empty_shard::EmptyShard;
