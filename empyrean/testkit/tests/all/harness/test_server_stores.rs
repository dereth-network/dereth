//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! TestServer's MemShard/MemAuth: a seeded account and character are read back by world code on
//! the next iteration; the tick record counts a session the pass dropped.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use empyrean_common::clock::Clock;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::AccessLevel;
use empyrean_store::models::shard::Character;
use empyrean_testkit::TestServer;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::actions::i_actor::{enqueue, Actor};
use empyrean_world::World;

#[test]
fn a_test_seeds_an_account_and_a_character_that_world_code_then_reads() {
    let mut ts = TestServer::new();
    ts.advance(1.0);

    let account = ts
        .auth()
        .create_account(
            "f22acct",
            "secret",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created");
    assert_eq!(account.account_id, 1);
    assert!(account.create_time > DotNetDateTime::new(2026, 1, 1));
    assert_eq!(
        account.create_time,
        ts.clock.utc_now(),
        "MemAuth reads the server's virtual clock"
    );

    let player = 0x5000_0001;
    let mut biota = empyrean_entity::Biota {
        id: player,
        weenie_class_id: 1,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    let mut pack = [empyrean_entity::Biota {
        id: 0x8000_0001,
        weenie_class_id: 2,
        ..Default::default()
    }];
    let character = Character {
        id: player,
        account_id: account.account_id,
        name: "F22 Tester".into(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut pack, &character));

    // World code reads them back: the account directly, the characters through the serialized
    // shard, whose callback runs in the next iteration's shard callback stage.
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let out = Arc::clone(&seen);
    enqueue(
        &mut ts.world,
        Actor::World,
        Action::delegate(move |w: &mut World| {
            let account_id = w.auth.lock().get_account_id_by_name("F22ACCT");
            let out = Arc::clone(&out);
            w.shard.get_characters(
                account_id,
                false,
                Some(Box::new(
                    move |_w: &mut World, characters: Vec<Character>| {
                        out.lock().unwrap().extend(
                            characters
                                .into_iter()
                                .map(|c| format!("{:08X} {}", c.id, c.name)),
                        );
                    },
                )),
            );
        }),
    );
    ts.step();
    assert!(
        seen.lock().unwrap().is_empty(),
        "the callback waits for the next iteration"
    );
    ts.step();
    assert_eq!(*seen.lock().unwrap(), ["50000001 F22 Tester"]);
    assert!(
        ts.shard().get_biota(0x8000_0001, false).is_some(),
        "the possessions were saved too"
    );
}

#[test]
fn the_iteration_that_drops_a_session_still_counts_it() {
    // F15b: DoSessionWork's count is taken during the pass, so it includes a session the pass
    // dropped (ACE then sleeps 1 ms, not 10 ms).
    let mut ts = TestServer::new();
    ts.add_client("acct", "");
    assert!(ts.run_until(20.0, |ts| ts.world.net.get_session_count() == 0));
    assert_eq!(
        ts.last_tick().map(|t| t.session_count),
        Some(1),
        "the dropping pass"
    );
    ts.step();
    assert_eq!(ts.last_tick().map(|t| t.session_count), Some(0));
}

mod configuration {
    //! ACE: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs::EnqueueBroadcast

    /// A test configuration does not leak into parallel servers.
    #[test]
    fn a_test_configuration_does_not_leak_into_parallel_servers() {
        let mut config = empyrean_common::master_configuration::MasterConfiguration::default();
        config.server.world_name = "Isolated World".into();
        let ts = empyrean_testkit::TestServer::with_config(
            empyrean_dat::FakeDats::new().build().expect("fake dats"),
            config,
            |_| {},
        );
        assert_eq!(
            empyrean_world::sessions::config_server_world_name(),
            "Isolated World"
        );
        let other = std::thread::spawn(|| {
            let _ts = empyrean_testkit::TestServer::new();
            empyrean_world::sessions::config_server_world_name()
        })
        .join()
        .unwrap();
        assert_eq!(other, "Empyrean");
        assert_eq!(
            empyrean_world::sessions::config_server_world_name(),
            "Isolated World",
            "unchanged by the other server's start-up"
        );
        drop(ts);
        assert_eq!(
            empyrean_world::sessions::config_server_world_name(),
            "Empyrean"
        );
    }
}
