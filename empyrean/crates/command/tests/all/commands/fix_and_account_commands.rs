//! ACE: Source/ACE.Server/Command/Handlers/DeveloperFixCommands.cs::DeveloperFixCommands
//! Console account/character commands (create, access, delete) and the developer fix/verify
//! commands (shortcut and spell bars, plating, attributes, vitals, skill credits, xp, armor,
//! shields, wield levels) report and repair a seeded shard as ACE does.
//! Fixture: synthetic command arguments, handler tables and isolated server state.

use std::net::{IpAddr, Ipv4Addr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use empyrean_command::command_handler::CommandHandler;
use empyrean_command::command_manager;
use empyrean_command::handlers::{
    account_commands as acc, character_commands as chc, developer_database_commands as ddc,
    developer_fix_commands as dfc,
};
use empyrean_common::dotnet::{DotNetDict, TimeSpan};
use empyrean_dat::fake::sample;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, CombatUse, EquipMask, PropertyAttribute, PropertyAttribute2nd, PropertyInstanceId,
    PropertyInt, PropertyInt64, PropertyString, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesSkill};
use empyrean_store::models::shard::{
    Biota, Character, CharacterPropertiesQuestRegistry, CharacterPropertiesShortcutBar,
    CharacterPropertiesSpellBar,
};
use empyrean_store::shard_database_offline_tools::load_biota;
use empyrean_store::{MemShard, ShardDatabaseWithCaching, ShardHandle};
use empyrean_testkit::TestServer;
use empyrean_world::entity::core_plating;
use empyrean_world::managers::{player_manager, property_manager};
use empyrean_world::World;

pub(crate) const DELTA: u32 = 0x5000_0001;
const ECHO: u32 = 0x5000_0002;

fn dats() -> Arc<empyrean_dat::DatManager> {
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::XP_TABLE, sample::xp_table())
        .with_portal(file_id::CHAR_GEN, sample::char_gen())
        .with_skill_table(sample::skill_table())
        .build()
        .expect("fake dats")
}

/// A player in the shard: an account at `level`, a biota named `name` (edited by `edit`), and its
/// character row (edited by `character`).
pub(crate) fn seed_player(
    w: &World,
    account: &str,
    level: AccessLevel,
    guid: u32,
    name: &str,
    edit: impl FnOnce(&mut empyrean_entity::Biota),
    character: impl FnOnce(&mut Character),
) {
    let account_id = w
        .auth
        .lock()
        .create_account(account, "pw", level, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .expect("a new account")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    edit(&mut biota);
    let mut c = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    character(&mut c);
    assert!(w
        .shard
        .base_database()
        .add_character_in_parallel(&mut biota, &mut [], &c));
}

pub(crate) fn server(setup: impl FnOnce(&mut World)) -> TestServer {
    let ts = TestServer::with_setup(dats(), setup);
    let _ = TestServer::take_not_ported();
    ts
}

/// What a console command wrote: each `Console.WriteLine` and console `log.Info`.
pub(crate) fn console(
    ts: &mut TestServer,
    handler: CommandHandler,
    parameters: &[&str],
) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    command_manager::take_console_output()
}

/// `console`, then one world iteration (the shard's callbacks), capturing both.
fn console_then_step(
    ts: &mut TestServer,
    handler: CommandHandler,
    parameters: &[&str],
) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    ts.advance(0.5);
    command_manager::take_console_output()
}

pub(crate) fn stored(ts: &TestServer, id: u32) -> Biota {
    load_biota(&mut **ts.shard(), id).expect("stored")
}

fn attributes(b: &mut empyrean_entity::Biota, list: &[(PropertyAttribute, u32, u32, u32)]) {
    let d = b.properties_attribute.get_or_insert_with(DotNetDict::new);
    for &(a, init_level, level_from_cp, cp_spent) in list {
        d.insert(
            a,
            PropertiesAttribute {
                init_level,
                level_from_cp,
                cp_spent,
            },
        );
    }
}

// ---------------------------------------------------------------------------------------------
// AccountCommands, CharacterCommands
// ---------------------------------------------------------------------------------------------

#[test]
fn accounts_are_created_and_changed_from_the_console() {
    let mut ts = server(|_| {});

    let created = console(&mut ts, acc::handle_account_create, &["U64Alpha", "pw1"]);
    let alpha = ts
        .auth()
        .get_account_by_name("u64alpha")
        .expect("created, lower case");
    assert_eq!(
        created,
        [format!(
            "Account successfully created for u64alpha ({}) with access rights as a Player.",
            alpha.account_id
        )]
    );
    assert_eq!(
        console(&mut ts, acc::handle_account_create, &["u64alpha", "x"]),
        ["Account already exists. Try a new name."]
    );

    // the access level: a name, an undefined number (the default), a failed parse (Player)
    let dev = console(
        &mut ts,
        acc::handle_account_create,
        &["u64bravo", "pw", "developer"],
    );
    assert!(
        dev[0].ends_with("with access rights as a Developer."),
        "{dev:?}"
    );
    assert!(console(
        &mut ts,
        acc::handle_account_create,
        &["u64charlie", "pw", "9"]
    )[0]
    .ends_with("as a Player."));
    assert!(console(
        &mut ts,
        acc::handle_account_create,
        &["u64delta", "pw", "nope"]
    )[0]
    .ends_with("as a Player."));
    ts.world.auth.set_auto_promote_next_account_to_admin(true);
    assert!(console(
        &mut ts,
        acc::handle_account_create,
        &["u64echo", "pw", "ADMIN"]
    )[0]
    .ends_with("with access rights as an Admin."));
    assert!(
        !ts.world.auth.auto_promote_next_account_to_admin(),
        "an admin account ends the auto-promotion"
    );
    assert_eq!(
        ts.auth()
            .get_account_by_name("u64echo")
            .unwrap()
            .access_level,
        5
    );

    assert_eq!(
        console(&mut ts, acc::handle_account_get, &["u64alpha"]),
        [format!("User: u64alpha, ID: {}", alpha.account_id)]
    );
    let missing = catch_unwind(AssertUnwindSafe(|| {
        console(&mut ts, acc::handle_account_get, &["nobody"])
    }));
    assert!(missing.is_err(), "ACE dereferences the null account");
    let _ = command_manager::take_console_output();

    assert_eq!(
        console(
            &mut ts,
            acc::handle_account_update_access_level,
            &["Nobody"]
        ),
        ["Account nobody does not exist."]
    );
    assert_eq!(
        console(
            &mut ts,
            acc::handle_account_update_access_level,
            &["U64Alpha", "envoy"]
        ),
        ["Account u64alpha updated with access rights set as an Envoy."]
    );
    assert_eq!(
        ts.auth()
            .get_account_by_name("u64alpha")
            .unwrap()
            .access_level,
        3
    );
    assert_eq!(
        console(
            &mut ts,
            acc::handle_account_update_access_level,
            &["u64alpha"]
        ),
        ["Account u64alpha updated with access rights set as a Player."]
    );
    assert_eq!(
        ts.auth()
            .get_account_by_name("u64alpha")
            .unwrap()
            .access_level,
        0
    );

    assert_eq!(
        console(&mut ts, acc::handle_account_set_password, &["nobody", "x"]),
        ["Account nobody does not exist."]
    );
    assert_eq!(
        console(
            &mut ts,
            acc::handle_account_set_password,
            &["U64ALPHA", "n3w"]
        ),
        ["Account password for u64alpha successfully changed."]
    );
    let mut account = ts.auth().get_account_by_name("u64alpha").unwrap();
    let mut auth = ts.world.auth.lock();
    assert!(account.password_matches("n3w", &mut **auth));
    assert!(!account.password_matches("pw1", &mut **auth));
}

#[test]
fn set_characteraccess_sets_the_characters_permission_flags() {
    // V363 (a fix): ACE's shard call was never implemented and threw, so nothing changed
    use empyrean_entity::enums::PropertyBool;
    use empyrean_world::entity::i_player;
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |_| {},
            |_| {},
        )
    });
    let flags = |ts: &TestServer| -> Vec<PropertyBool> {
        let p = player_manager::find_by_name(&ts.world, "Delta Tester")
            .0
            .expect("Delta");
        [
            PropertyBool::IsAdmin,
            PropertyBool::IsArch,
            PropertyBool::IsEnvoy,
            PropertyBool::IsSentinel,
            PropertyBool::IsAdvocate,
        ]
        .into_iter()
        .filter(|&f| i_player::get_property(&ts.world, p, f) == Some(true))
        .collect()
    };
    assert_eq!(console(&mut ts, chc::handle_character_tokenization, &["Somebody", "admin"]), [
        "There is no character by the name of Somebody found in the database. Has it been deleted?"
    ]);
    assert_eq!(
        console(
            &mut ts,
            chc::handle_character_tokenization,
            &["Delta Tester", "envoy"]
        ),
        ["Character Delta Tester has been made an Envoy."]
    );
    assert_eq!(
        flags(&ts),
        [PropertyBool::IsEnvoy, PropertyBool::IsSentinel]
    );
    assert_eq!(
        console(
            &mut ts,
            chc::handle_character_tokenization,
            &["delta tester", "4"]
        ),
        ["Character delta tester has been made a Developer."]
    );
    assert_eq!(flags(&ts), [PropertyBool::IsArch]);
    assert_eq!(
        console(
            &mut ts,
            chc::handle_character_tokenization,
            &["Delta Tester"]
        ),
        ["Character Delta Tester has been made a Player."]
    );
    assert_eq!(flags(&ts), []);
}

#[test]
fn deletecharacter_deletes_an_offline_character() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |_| {},
            |_| {},
        )
    });
    assert!(player_manager::find_by_name(&ts.world, "Delta Tester")
        .0
        .is_some());

    assert_eq!(
        console(
            &mut ts,
            chc::handle_character_forced_delete,
            &["Nobody", "Here"]
        ),
        ["There is no character named Nobody Here in the database."]
    );

    let out = console_then_step(
        &mut ts,
        chc::handle_character_forced_delete,
        &["Delta", "Tester"],
    );
    assert_eq!(
        out,
        ["Successfully deleted character Delta Tester (0x50000001)."]
    );
    let c = ts
        .shard()
        .get_character_stub_by_guid(DELTA)
        .expect("the row stays");
    assert!(c.is_deleted);
    assert!(c.delete_time > 0);
    assert!(
        player_manager::find_by_name(&ts.world, "Delta Tester")
            .0
            .is_none(),
        "gone from the player manager"
    );
}

// ---------------------------------------------------------------------------------------------
// DeveloperDatabaseCommands
// ---------------------------------------------------------------------------------------------

#[test]
fn database_queue_info_and_the_perf_test_report_from_the_console() {
    let mut ts = server(|_| {});
    assert_eq!(
        console_then_step(&mut ts, ddc::handle_database_queue_info, &[]),
        [
            "Current database queue count: 0",
            "Current database queue wait time: 0 ms"
        ]
    );

    let out = console_then_step(&mut ts, ddc::handle_database_perf_test, &["3"]);
    assert_eq!(out, [
        "Starting Shard Database Performance Tests.\nBiotas per test: 3\nThis may take several minutes to complete...\nCurrent database queue count: 0",
        "Current database queue wait time: 0 ms",
        "3 individual add    Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        "3 individual save   Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        "3 individual remove Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        // V365 (a fix): the bulk stages report and the run completes (ACE's bulk add waited
        // forever: nothing more)
        "3 bulk add          Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        "3 bulk save         Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        "3 bulk remove       Duration:   0.0 s. Queue Wait Time:   0 ms. Average Execution Time:   0 ms. Success/Fail: 0/0.",
        "Database Performance Tests Completed",
    ]);
    // a count that does not parse is 0 (int.TryParse's out value)
    let out = console_then_step(&mut ts, ddc::handle_database_perf_test, &["lots"]);
    assert!(out[0].contains("Biotas per test: 0"), "{out:?}");
    assert!(
        out[2].ends_with("Average Execution Time: NaN ms. Success/Fail: 0/0."),
        "{out:?}"
    );
}

#[test]
fn shard_cache_retention_times_need_the_caching_shard() {
    let mut ts = server(|_| {});
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_pbrt, &[]),
        ["DatabaseManager is not using ShardDatabaseWithCaching"]
    );

    let clock: Arc<dyn empyrean_common::clock::Clock> =
        Arc::new(empyrean_common::clock::VirtualClock::default());
    let caching = ShardDatabaseWithCaching::new(
        MemShard::new(),
        Arc::clone(&clock),
        TimeSpan::from_minutes(31.0),
        TimeSpan::from_minutes(11.0),
    );
    ts.world.shard = ShardHandle::synchronous(Box::new(caching), clock);
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_pbrt, &[]),
        ["Shard Database, Player Biota Cache - Retention Time 31 m"]
    );
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_pbrt, &["45"]),
        ["Shard Database, Player Biota Cache - Retention Time 45 m"]
    );
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_pbrt, &["-1"]),
        ["Unable to parse argument. Specify retention time in integer minutes."]
    );
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_npbrt, &[]),
        ["Shard Database, Non-Player Biota Cache - Retention Time 11 m"]
    );
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_npbrt, &["0"]),
        ["Shard Database, Non-Player Biota Cache - Retention Time 0 m"]
    );
    assert_eq!(
        console(&mut ts, ddc::handle_database_shard_cache_pbrt, &[]),
        ["Shard Database, Player Biota Cache - Retention Time 45 m"]
    );
}

#[test]
fn fix_shortcut_bars_rebuilds_bars_with_duplicate_objects() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |_| {},
            |c| {
                let s = |i: u32, o: u32| CharacterPropertiesShortcutBar {
                    character_id: DELTA,
                    shortcut_bar_index: i,
                    shortcut_object_id: o,
                };
                c.character_properties_shortcut_bar =
                    vec![s(0, 0x8000_0001), s(1, 0x8000_0001), s(2, 0x8000_0002)];
            },
        );
        seed_player(
            w,
            "u64e",
            AccessLevel::Player,
            ECHO,
            "Echo Tester",
            |_| {},
            |c| {
                c.character_properties_shortcut_bar = vec![CharacterPropertiesShortcutBar {
                    character_id: ECHO,
                    shortcut_bar_index: 0,
                    shortcut_object_id: 0x8000_0003,
                }];
            },
        );
    });
    let header = [
        "",
        "This command will attempt to fix duplicate shortcuts found in player shortcut bars. Unless explictly indicated, command will dry run only",
        "If the command outputs nothing or errors, you are ready to proceed with updating your shard db with 2019-04-17-00-Character_Shortcut_Changes.sql script",
        "",
    ];
    let mut dry: Vec<&str> = header.to_vec();
    dry.extend([
        "This will be a dry run and show which characters that would be affected. To perform fix, please use command: fix-shortcut-bars execute",
        "Player Delta Tester (1342177281) was found to have errors in their shortcuts.",
        "Total players found with bugged shortcuts: 1",
        "dry run completed. Use fix-shortcut-bars execute to actually run command",
    ]);
    assert_eq!(console(&mut ts, ddc::handle_fix_shortcut_bars, &[]), dry);

    let mut run: Vec<&str> = header.to_vec();
    run.extend([
        "Player Delta Tester (1342177281) was found to have errors in their shortcuts.",
        "Total players found with bugged shortcuts: 1",
        "Executing changes...",
    ]);
    assert_eq!(
        console(&mut ts, ddc::handle_fix_shortcut_bars, &["EXECUTE"]),
        run
    );
    let c = ts.shard().get_character(DELTA).expect("the character");
    let bar: Vec<(u32, u32)> = c
        .character_properties_shortcut_bar
        .iter()
        .map(|s| (s.shortcut_bar_index, s.shortcut_object_id))
        .collect();
    assert_eq!(
        bar,
        [(0, 0x8000_0001), (2, 0x8000_0002)],
        "the duplicate object's second slot is gone"
    );
}

#[test]
fn fix_spell_bars_renumbers_each_bar_from_one() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |_| {},
            |c| {
                let s = |n: u32, i: u32, spell: u32| CharacterPropertiesSpellBar {
                    character_id: DELTA,
                    spell_bar_number: n,
                    spell_bar_index: i,
                    spell_id: spell,
                };
                c.character_properties_spell_bar =
                    vec![s(1, 1, 10), s(1, 3, 11), s(1, 7, 12), s(2, 1, 13)];
            },
        );
    });
    let out = console(&mut ts, ddc::handle_fix_spell_bars, &[]);
    assert_eq!(out[4], "This will be a dry run and show which characters that would be affected. To perform fix, please use command: fix-spell-bars execute");
    assert_eq!(
        out[5..8],
        [
            "",
            "Press enter to start.",
            "Starting FixSpellBarsPR2918 process. This could take a while..."
        ]
    );
    assert_eq!(out[8..], [
        "FixSpellBarsPR2918: Character 0x50000001, SpellBarNumber = 1 | SpellBarIndex = 001; OK",
        "FixSpellBarsPR2918: Character 0x50000001, SpellBarNumber = 1 | SpellBarIndex = 003; Fixed - 002",
        "FixSpellBarsPR2918: Character 0x50000001, SpellBarNumber = 1 | SpellBarIndex = 007; Fixed - 003",
        "FixSpellBarsPR2918: Character 0x50000001, SpellBarNumber = 2 | SpellBarIndex = 001; OK",
        "2 CharacterPropertiesSpellBar records need to be fixed!",
        "dry run completed. Use fix-spell-bars execute to actually run command",
    ]);
    let out = console(&mut ts, ddc::handle_fix_spell_bars, &["execute"]);
    assert_eq!(
        out[out.len() - 2..],
        [
            "Saving changes...",
            "Fixed 2 CharacterPropertiesSpellBar records."
        ]
    );
    let c = ts.shard().get_character(DELTA).expect("the character");
    let mut bar: Vec<(u32, u32, u32)> = c
        .character_properties_spell_bar
        .iter()
        .map(|s| (s.spell_bar_number, s.spell_bar_index, s.spell_id))
        .collect();
    bar.sort_unstable();
    assert_eq!(bar, [(1, 1, 10), (1, 2, 11), (1, 3, 12), (2, 1, 13)]);
}

#[test]
fn fix_gear_plating_renames_owned_plated_items() {
    const PLATED: u32 = 0x8000_0100;
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |_| {},
            |_| {},
        );
        let mut item = empyrean_entity::Biota {
            id: PLATED,
            weenie_class_id: 3001,
            weenie_type: WeenieType::Generic,
            ..Default::default()
        };
        item.set_property(PropertyString::Name, "Plated Gauntlets".to_owned());
        item.set_property(PropertyString::GearPlatingName, "Wrong Plating".to_owned());
        item.set_property(
            PropertyInt::ValidLocations,
            EquipMask::HandWear.0.cast_signed(),
        );
        item.set_property(PropertyInstanceId::Owner, DELTA);
        assert!(w.shard.base_database().save_biota(&mut item, false));
    });
    let right = core_plating::get_gear_plating_name(EquipMask::HandWear);
    assert_ne!(right, "Wrong Plating");
    let out = console(&mut ts, ddc::handle_fix_gear_plating, &[]);
    assert!(
        out.contains(&format!(
            "Char: Delta Tester - Change `Plated Gauntlets` from \"Wrong Plating\" to \"{right}\""
        )),
        "{out:?}"
    );
    assert!(
        out.contains(
            &" -- There are 1 items that have incorrect Gear Plating Name values. --".to_owned()
        ),
        "{out:?}"
    );
    assert_eq!(
        out.last().map(String::as_str),
        Some("Dry run completed. Use \"fix-gear-plating execute\" to actually run command")
    );

    let out = console(&mut ts, ddc::handle_fix_gear_plating, &["execute"]);
    assert_eq!(out.last().map(String::as_str), Some("Finished."));
    let b = stored(&ts, PLATED);
    assert_eq!(
        b.get_property_string(PropertyString::GearPlatingName),
        Some(right.as_str())
    );
    assert!(
        console(&mut ts, ddc::handle_fix_gear_plating, &[]).contains(
            &" -- There are 0 items that have incorrect Gear Plating Name values. --".to_owned()
        )
    );
}

// ---------------------------------------------------------------------------------------------
// DeveloperFixCommands over offline players
// ---------------------------------------------------------------------------------------------

#[test]
fn verify_attributes_reports_and_fixes_ranks_caps_and_unknown_attributes() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |b| {
                attributes(
                    b,
                    &[
                        (PropertyAttribute::Undef, 10, 0, 0),
                        (PropertyAttribute::Strength, 100, 5, 110),
                        (PropertyAttribute::Endurance, 100, 2, 300),
                    ],
                );
            },
            |_| {},
        );
    });
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &[]),
        [
            "Delta Tester has unknown attribute Undef",
            "Delta Tester's Strength rank is 5, should be 1",
            "Delta Tester's Endurance attribute total xp is 300, should be capped at 277",
            "Dry run completed. Type 'verify-attributes fix' to fix any issues.",
        ]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &["fix"]),
        [
            "Delta Tester has unknown attribute Undef -- fixed",
            "Delta Tester's Strength rank is 5, should be 1 -- fixed",
            "Delta Tester's Endurance attribute total xp is 300, should be capped at 277 -- fixed",
        ]
    );
    let b = stored(&ts, DELTA);
    let rows: Vec<(u16, u32, u32)> = b
        .biota_properties_attribute
        .iter()
        .map(|a| (a.r#type, a.level_from_cp, a.cp_spent))
        .collect();
    assert_eq!(rows, [(1, 1, 110), (2, 2, 277)]);
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &[]),
        ["Verified attributes for 1 players"]
    );
}

#[test]
fn verify_attributes_redistributes_augmented_innate_points_when_the_cap_is_on() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64e",
            AccessLevel::Player,
            ECHO,
            "Echo Tester",
            |b| {
                attributes(
                    b,
                    &[
                        (PropertyAttribute::Strength, 102, 0, 0),
                        (PropertyAttribute::Endurance, 60, 0, 0),
                        (PropertyAttribute::Coordination, 90, 0, 0),
                    ],
                )
            },
            |_| {},
        );
    });
    // (on by default) off: nothing to report
    assert!(property_manager::modify_bool(
        &ts.world,
        "attribute_augmentation_safety_cap",
        false
    ));
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &[]),
        ["Verified attributes for 1 players"]
    );
    assert!(property_manager::modify_bool(
        &ts.world,
        "attribute_augmentation_safety_cap",
        true
    ));
    let note = "Echo Tester's Strength is currently 102, augmented above 100.\n5 points will be redistributed to lowest eligible innate attribute to fix this issue.\n";
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &[]),
        [
            note,
            "Dry run completed. Type 'verify-attributes fix' to fix any issues."
        ]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_attributes, &["fix"]),
        [note]
    );
    let b = stored(&ts, ECHO);
    let rows: Vec<(u16, u32)> = b
        .biota_properties_attribute
        .iter()
        .map(|a| (a.r#type, a.init_level))
        .collect();
    assert_eq!(
        rows,
        [(1, 97), (2, 65), (4, 90)],
        "5 points from Strength to the lowest, Endurance"
    );
    assert_eq!(
        b.get_property_bool(empyrean_entity::enums::PropertyBool::FreeAttributeResetRenewed),
        Some(true)
    );
}

#[test]
fn verify_vitals_and_skills_report_and_fix() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |b| {
                let v = b
                    .properties_attribute_2nd
                    .get_or_insert_with(DotNetDict::new);
                v.insert(
                    PropertyAttribute2nd::MaxHealth,
                    PropertiesAttribute2nd {
                        init_level: 10,
                        level_from_cp: 3,
                        cp_spent: 73,
                        current_level: 10,
                    },
                );
                v.insert(
                    PropertyAttribute2nd::Health,
                    PropertiesAttribute2nd {
                        init_level: 10,
                        level_from_cp: 0,
                        cp_spent: 0,
                        current_level: 10,
                    },
                );
                let s = b.properties_skill.get_or_insert_with(DotNetDict::new);
                let skill = |sac, pp, level_from_pp, init_level| PropertiesSkill {
                    sac,
                    pp,
                    level_from_pp,
                    init_level,
                    ..PropertiesSkill::default()
                };
                s.insert(Skill::Axe, skill(SkillAdvancementClass::Trained, 0, 0, 0));
                s.insert(
                    Skill::MeleeDefense,
                    skill(SkillAdvancementClass::Trained, 55, 0, 5),
                );
                s.insert(
                    Skill::MissileDefense,
                    skill(SkillAdvancementClass::Untrained, 10, 0, 0),
                );
                s.insert(
                    Skill::ArcaneLore,
                    skill(SkillAdvancementClass::Specialized, 0, 0, 0),
                );
            },
            |_| {},
        );
    });
    assert_eq!(
        console(&mut ts, dfc::handle_verify_vitals, &[]),
        [
            "Delta Tester's MaxHealth rank is 3, should be 1",
            "Delta Tester has unknown vital Health",
            "Dry run completed. Type 'verify-vitals fix' to fix any issues.",
        ]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_skills, &[]),
        [
            "Delta Tester has unknown skill Axe",
            "Delta Tester has Trained skill MeleeDefense with 5 InitLevel",
            "Delta Tester's MeleeDefense rank is 0, should be 1",
            "Delta Tester has Untrained skill MissileDefense with 10 xp (rank 0)",
            "Delta Tester has Specialized skill ArcaneLore with 0 InitLevel",
            "Dry run completed. Type 'verify-skills fix' to fix any issues.",
        ]
    );
    let _ = console(&mut ts, dfc::handle_verify_vitals, &["fix"]);
    let _ = console(&mut ts, dfc::handle_verify_skills, &["fix"]);
    assert_eq!(
        console(&mut ts, dfc::handle_verify_vitals, &[]),
        ["Verified vitals for 1 players"]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_skills, &[]),
        ["Verified skills for 1 players"]
    );
    let b = stored(&ts, DELTA);
    let skills: Vec<(i32, u32, u16, u32, u32)> = b
        .biota_properties_skill
        .iter()
        .map(|s| {
            (
                i32::from(s.r#type),
                s.sac,
                s.level_from_pp,
                s.pp,
                s.init_level,
            )
        })
        .collect();
    assert_eq!(
        skills,
        [(6, 2, 1, 55, 0), (7, 1, 0, 0, 0), (14, 3, 0, 0, 10)],
        "Trained 2, Untrained 1, Specialized 3"
    );
}

#[test]
fn verify_skill_credits_counts_heritage_level_and_quest_credits() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |b| {
                b.set_property(PropertyInt::HeritageGroup, 1);
                b.set_property(PropertyInt::Level, 1);
                b.set_property(PropertyInt::AvailableSkillCredits, 35);
                b.set_property(PropertyInt::TotalSkillCredits, 50);
                let s = b.properties_skill.get_or_insert_with(DotNetDict::new);
                s.insert(
                    Skill::MeleeDefense,
                    PropertiesSkill {
                        sac: SkillAdvancementClass::Trained,
                        ..PropertiesSkill::default()
                    },
                );
            },
            |c| {
                c.character_properties_quest_registry = vec![CharacterPropertiesQuestRegistry {
                    character_id: DELTA,
                    quest_name: "oswaldmanualcompleted".to_owned(),
                    last_time_completed: 1,
                    num_times_completed: 1,
                }];
            },
        );
        // admins are skipped
        seed_player(
            w,
            "u64e",
            AccessLevel::Admin,
            ECHO,
            "Echo Tester",
            |b| {
                b.set_property(PropertyInt::HeritageGroup, 1);
            },
            |_| {},
        );
    });
    // 50 (heritage) + 0 (level 1) + 1 (Oswald, matched as MySQL's collation does) = 51; 10 spent
    assert_eq!(
        console(&mut ts, dfc::handle_verify_skill_credits, &[]),
        [
            "Delta Tester (0x50000001) should have 41 available skill credits, but they have 35",
            "Delta Tester (0x50000001) should have 51 total skill credits, but they have 50",
            "Dry run completed. Type 'verify-skill-credits fix' to fix any issues.",
        ]
    );
    let _ = console(&mut ts, dfc::handle_verify_skill_credits, &["fix"]);
    assert_eq!(
        console(&mut ts, dfc::handle_verify_skill_credits, &[]),
        ["Verified skill credits for 2 players"]
    );
    let b = stored(&ts, DELTA);
    assert_eq!(
        b.get_property_int(PropertyInt::AvailableSkillCredits),
        Some(41)
    );
    assert_eq!(b.get_property_int(PropertyInt::TotalSkillCredits), Some(51));
}

#[test]
fn verify_heritage_and_max_augs() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |b| {
                b.set_property(PropertyInt::HeritageGroup, 1);
                b.set_property(PropertyInt::AugmentationInnateStrength, 11);
                b.set_property(PropertyInt::AugmentationBonusXp, -1);
            },
            |_| {},
        );
        seed_player(
            w,
            "u64e",
            AccessLevel::Player,
            ECHO,
            "Echo Tester",
            |_| {},
            |_| {},
        );
    });
    assert_eq!(
        console(&mut ts, dfc::handle_verify_heritage_augs, &[]),
        [
            "AugmentationJackOfAllTrades=0 for Aluvian player Delta Tester",
            "Couldn't find heritage for Echo Tester",
            "Dry run completed. Type 'verify-heritage-augs fix' to fix any issues.",
        ]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_max_augs, &["fix"]),
        [
            "Delta Tester has 11 AugmentationInnateStrength, max should be 10 -- fixed",
            "Delta Tester has -1 AugmentationBonusXp, min should be 0 -- fixed",
        ]
    );
    let _ = console(&mut ts, dfc::handle_verify_heritage_augs, &["fix"]);
    let b = stored(&ts, DELTA);
    assert_eq!(
        b.get_property_int(PropertyInt::AugmentationJackOfAllTrades),
        Some(1)
    );
    assert_eq!(
        b.get_property_int(PropertyInt::AugmentationInnateStrength),
        Some(10)
    );
    assert_eq!(
        b.get_property_int(PropertyInt::AugmentationBonusXp),
        Some(0)
    );
}

#[test]
fn verify_xp_finds_unspent_xp_and_refunds_it() {
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64d",
            AccessLevel::Player,
            DELTA,
            "Delta Tester",
            |b| {
                b.set_property(PropertyInt::HeritageGroup, 1);
                b.set_property(PropertyInt::AugmentationJackOfAllTrades, 1);
                b.set_property(PropertyInt64::TotalExperience, 1000);
                b.set_property(PropertyInt64::AvailableExperience, 0);
                attributes(b, &[(PropertyAttribute::Strength, 10, 1, 110)]);
                // (a stored empty collection loads as null, which ACE dereferences: real players
                // always have vitals and skills)
                b.properties_attribute_2nd
                    .get_or_insert_with(DotNetDict::new)
                    .insert(
                        PropertyAttribute2nd::MaxHealth,
                        PropertiesAttribute2nd::default(),
                    );
                b.properties_skill
                    .get_or_insert_with(DotNetDict::new)
                    .insert(Skill::MeleeDefense, PropertiesSkill::default());
            },
            |_| {},
        );
    });
    assert_eq!(console(&mut ts, dfc::handle_verify_experience, &[]), [
        "Delta Tester is calculated to have spent 110 experience, which currently differs by 890",
        "Found issues for 1 players",
        "Dry run completed. Type 'verify-xp fix' to fix any issues.",
    ]);
    assert_eq!(console(&mut ts, dfc::handle_verify_experience, &["fix"]), [
        "Delta Tester is calculated to have spent 110 experience, which currently differs by 890 -- fixed",
        "Fixed issues for 1 players",
    ]);
    let b = stored(&ts, DELTA);
    assert_eq!(
        b.get_property_int64(PropertyInt64::AvailableExperience),
        Some(890)
    );
    assert_eq!(b.get_property_int64(PropertyInt64::VerifyXp), Some(890));
}

// ---------------------------------------------------------------------------------------------
// DeveloperFixCommands over shard items
// ---------------------------------------------------------------------------------------------

pub(crate) fn item(
    w: &World,
    id: u32,
    wcid: u32,
    weenie_type: WeenieType,
    edit: impl FnOnce(&mut empyrean_entity::Biota),
) {
    let mut b = empyrean_entity::Biota {
        id,
        weenie_class_id: wcid,
        weenie_type,
        ..Default::default()
    };
    edit(&mut b);
    assert!(w.shard.base_database().save_biota(&mut b, false));
}

#[test]
fn verify_armor_levels_caps_loot_armor() {
    let mut ts = server(|w| {
        item(w, 0x8000_0001, 3001, WeenieType::Clothing, |b| {
            b.set_property(PropertyString::Name, "Plate Hauberk".to_owned());
            b.set_property(PropertyInt::ArmorLevel, 400);
            b.set_property(PropertyInt::ItemWorkmanship, 5);
            b.set_property(
                PropertyInt::ValidLocations,
                EquipMask::ChestArmor.0.cast_signed(),
            );
        });
        item(w, 0x8000_0002, 3002, WeenieType::Clothing, |b| {
            b.set_property(PropertyString::Name, "Tinkered Gauntlets".to_owned());
            b.set_property(PropertyInt::ArmorLevel, 400);
            b.set_property(PropertyInt::ItemWorkmanship, 5);
            b.set_property(
                PropertyInt::ValidLocations,
                EquipMask::HandWear.0.cast_signed(),
            );
            b.set_property(PropertyString::TinkerLog, "64,64".to_owned());
            b.set_property(PropertyInt::NumTimesTinkered, 2);
        });
        item(w, 0x8000_0003, 3003, WeenieType::Clothing, |b| {
            b.set_property(PropertyString::Name, "Unenchantable Plate".to_owned());
            b.set_property(PropertyInt::ArmorLevel, 900);
            b.set_property(PropertyInt::ItemWorkmanship, 5);
            b.set_property(
                PropertyInt::ValidLocations,
                EquipMask::ChestArmor.0.cast_signed(),
            );
            b.set_property(PropertyInt::ResistMagic, 9999);
        });
    });
    assert_eq!(
        console(&mut ts, dfc::handle_fix_armor_level, &[]),
        [
            "Fetching shard armors (this may take awhile on large servers) ...",
            "Plate Hauberk, 400 => 315",
            "Tinkered Gauntlets, 400 (2) => 385",
            "Found 3 armors, 2 will be adjusted",
            "Dry run completed. Type 'verify-armor-levels fix' to fix any issues.",
        ]
    );
    let _ = console(&mut ts, dfc::handle_fix_armor_level, &["fix"]);
    assert_eq!(
        stored(&ts, 0x8000_0001).get_property_int(PropertyInt::ArmorLevel),
        Some(315)
    );
    assert_eq!(
        stored(&ts, 0x8000_0002).get_property_int(PropertyInt::ArmorLevel),
        Some(385)
    );
    assert_eq!(
        console(&mut ts, dfc::handle_fix_armor_level, &[])[1..],
        ["Verified 3 armors."]
    );
}

#[test]
fn verify_shield_rating_removes_crit_ratings_from_loot_shields() {
    let mut ts = server(|w| {
        item(w, 0x8000_0010, 3010, WeenieType::Generic, |b| {
            b.set_property(PropertyString::Name, "Kite Shield".to_owned());
            b.set_property(PropertyInt::ItemWorkmanship, 4);
            b.set_property(PropertyInt::CombatUse, i32::from(CombatUse::Shield.0));
            b.set_property(PropertyInt::GearCritDamage, 2);
            b.set_property(PropertyInt::GearCritDamageResist, 3);
        });
        // not loot-generated (no workmanship): left alone
        item(w, 0x8000_0011, 3011, WeenieType::Generic, |b| {
            b.set_property(PropertyString::Name, "Quest Shield".to_owned());
            b.set_property(PropertyInt::CombatUse, i32::from(CombatUse::Shield.0));
            b.set_property(PropertyInt::GearCritDamage, 5);
        });
    });
    assert_eq!(
        console(&mut ts, dfc::handle_remove_shield_ratings, &[]),
        [
            "Found 2 bugged shields:",
            "80000010 - Kite Shield (CD: 2)",
            "80000010 - Kite Shield (CDR: 3)",
            "Dry run completed. Type 'verify-shield-rating fix' to fix any issues.",
        ]
    );
    let _ = console(&mut ts, dfc::handle_remove_shield_ratings, &["fix"]);
    let b = stored(&ts, 0x8000_0010);
    assert_eq!(b.get_property_int(PropertyInt::GearCritDamage), None);
    assert_eq!(b.get_property_int(PropertyInt::GearCritDamageResist), None);
    assert_eq!(
        stored(&ts, 0x8000_0011).get_property_int(PropertyInt::GearCritDamage),
        Some(5)
    );
    assert_eq!(
        console(&mut ts, dfc::handle_remove_shield_ratings, &[]),
        ["Verified 1 shields"]
    );
}

#[test]
fn verify_legendary_and_clothing_wield_levels() {
    let legendary = *empyrean_world::factories::loot_tables::CANTRIP_SETS
        .legendary_cantrips
        .iter()
        .next()
        .expect("a legendary cantrip");
    let epic = *empyrean_world::factories::loot_tables::CANTRIP_SETS
        .epic_cantrips
        .iter()
        .next()
        .expect("an epic cantrip");
    let mut ts = server(|w| {
        item(w, 0x8000_0020, 3020, WeenieType::Generic, |b| {
            b.properties_spell_book
                .get_or_insert_with(DotNetDict::new)
                .insert(legendary, 2.0);
            b.set_property(PropertyInt::WieldRequirements, 7);
            b.set_property(PropertyInt::WieldDifficulty, 150);
        });
        item(w, 0x8000_0021, 3021, WeenieType::Clothing, |b| {
            b.properties_spell_book
                .get_or_insert_with(DotNetDict::new)
                .insert(epic, 1.0);
        });
    });
    assert_eq!(
        console(&mut ts, dfc::handle_verify_legendary_wield_level, &[]),
        [
            "Found issues for 1 of 1 legendary items",
            "Dry run completed. Type 'verify-legendary-wield-level fix' to fix any issues.",
        ]
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_legendary_wield_level, &["fix"]),
        [
            "Found issues for 1 of 1 legendary items",
            "UPDATE biota_properties_int SET value=180 WHERE object_Id=0x80000020 AND type=160;",
        ]
    );
    assert_eq!(
        stored(&ts, 0x8000_0020).get_property_int(PropertyInt::WieldDifficulty),
        Some(180)
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_legendary_wield_level, &[]),
        ["Verified wield levels for 1 legendary items"]
    );

    let clothing_name = empyrean_tables::enums::WeenieClassName(3021).to_dotnet_string();
    assert_eq!(
        console(&mut ts, dfc::handle_verify_clothing_wield_level, &[]),
        [
            "Missing wield difficulty:".to_owned(),
            format!("80000021 - {clothing_name} - 3"),
            "Dry run completed. Type 'verify-clothing-wield-level fix' to fix any issues."
                .to_owned(),
        ]
    );
    let _ = console(&mut ts, dfc::handle_verify_clothing_wield_level, &["fix"]);
    let b = stored(&ts, 0x8000_0021);
    assert_eq!(
        (
            b.get_property_int(PropertyInt::WieldRequirements),
            b.get_property_int(PropertyInt::WieldSkillType),
            b.get_property_int(PropertyInt::WieldDifficulty)
        ),
        (Some(7), Some(1), Some(150))
    );
    assert_eq!(
        console(&mut ts, dfc::handle_verify_clothing_wield_level, &[]),
        ["Verified wield levels for 1 pieces of t7 / t8 clothing"]
    );
}

#[test]
fn verify_attributes_renews_the_free_reset_only_for_a_redistributed_player() {
    // V365 (a fix): ACE never reset the flag between players, so a later player fixed for
    // another attribute issue also got FreeAttributeResetRenewed
    const FOXTROT: u32 = 0x5000_0003;
    let mut ts = server(|w| {
        seed_player(
            w,
            "u64e",
            AccessLevel::Player,
            ECHO,
            "Echo Tester",
            |b| {
                attributes(
                    b,
                    &[
                        (PropertyAttribute::Strength, 102, 0, 0),
                        (PropertyAttribute::Endurance, 60, 0, 0),
                    ],
                )
            },
            |_| {},
        );
        seed_player(
            w,
            "u64f",
            AccessLevel::Player,
            FOXTROT,
            "Foxtrot Tester",
            |b| attributes(b, &[(PropertyAttribute::Strength, 100, 5, 110)]),
            |_| {},
        );
    });
    assert!(property_manager::modify_bool(
        &ts.world,
        "attribute_augmentation_safety_cap",
        true
    ));
    let _ = console(&mut ts, dfc::handle_verify_attributes, &["fix"]);
    let renewed = |ts: &TestServer, id: u32| {
        stored(ts, id)
            .get_property_bool(empyrean_entity::enums::PropertyBool::FreeAttributeResetRenewed)
    };
    assert_eq!(renewed(&ts, ECHO), Some(true));
    assert_eq!(
        stored(&ts, FOXTROT)
            .biota_properties_attribute
            .iter()
            .map(|a| a.level_from_cp)
            .collect::<Vec<_>>(),
        [1],
        "Foxtrot's rank was fixed"
    );
    assert_eq!(
        renewed(&ts, FOXTROT),
        None,
        "Foxtrot's points were not redistributed"
    );
}

#[test]
fn verify_clothing_wield_level_fixes_an_item_with_some_of_the_rows() {
    // V365 (a fix): ACE's inserts threw a duplicate key error on the existing row
    let epic = *empyrean_world::factories::loot_tables::CANTRIP_SETS
        .epic_cantrips
        .iter()
        .next()
        .expect("an epic cantrip");
    let mut ts = server(|w| {
        item(w, 0x8000_0022, 3021, WeenieType::Clothing, |b| {
            b.properties_spell_book
                .get_or_insert_with(DotNetDict::new)
                .insert(epic, 1.0);
            b.set_property(PropertyInt::WieldRequirements, 2);
        });
    });
    let _ = console(&mut ts, dfc::handle_verify_clothing_wield_level, &["fix"]);
    let b = stored(&ts, 0x8000_0022);
    assert_eq!(
        (
            b.get_property_int(PropertyInt::WieldRequirements),
            b.get_property_int(PropertyInt::WieldSkillType),
            b.get_property_int(PropertyInt::WieldDifficulty)
        ),
        (Some(7), Some(1), Some(150))
    );
    assert_eq!(
        b.biota_properties_int
            .iter()
            .filter(|r| r.r#type == PropertyInt::WieldRequirements.0)
            .count(),
        1,
        "one row"
    );
}

mod rare_and_enchantment_repairs {
    //! ACE: Source/ACE.Server/Command/Handlers/DeveloperFixCommands.cs::DeveloperFixCommands
    //! Verify-melee-rares checks the world DB version and converts rares; verify-beneficial-
    //! enchantments sets the missing flag; fix-biota-emote-delay matches emotes to their weenie.
    //! Fixture: synthetic command arguments, handler tables and isolated server state.

    use std::sync::Arc;

    use empyrean_command::handlers::developer_fix_commands as dfc;
    use empyrean_common::dotnet::DotNetDict;
    use empyrean_content::models::world::{
        Version, Weenie, WeeniePropertiesEmote, WeeniePropertiesEmoteAction,
    };
    use empyrean_content::MemContent;
    use empyrean_dat::fake::sample;
    use empyrean_dat::FakeDats;
    use empyrean_entity::enums::{
        AccessLevel, EquipMask, PositionType, PropertyInstanceId, PropertyInt, PropertyString,
        WeenieType,
    };
    use empyrean_entity::models::PropertiesPosition;
    use empyrean_store::models::shard::{
        Biota, BiotaPropertiesEmote, BiotaPropertiesEmoteAction,
        BiotaPropertiesEnchantmentRegistry, CharacterPropertiesShortcutBar,
    };
    use empyrean_store::shard_database_offline_tools::load_biota;
    use empyrean_testkit::TestServer;
    use empyrean_world::managers::guid_manager;

    use crate::fix_commands::{console, item, seed_player, server, stored, DELTA};

    use empyrean_testkit::EmptyShard;

    fn version(patch: Option<&str>) -> Version {
        Version {
            id: 1,
            base_version: None,
            patch_version: patch.map(str::to_owned),
            last_modified: empyrean_common::dotnet::DotNetDateTime::UNIX_EPOCH,
        }
    }

    /// The EoR melee rares (45436..=45470), named "Rare <wcid>" and at version 2 as the v0.9.271 world
    /// database has them, and the world database's version.
    fn rare_content(patch: Option<&str>) -> MemContent {
        let mut c = MemContent::new();
        if patch.is_some() {
            c = c.version(version(patch));
        }
        for wcid in 45436..=45470 {
            c = c.weenie(
                Weenie::new(wcid, &format!("u64rare{wcid}"), WeenieType::MeleeWeapon)
                    .with_string(PropertyString::Name, &format!("Rare {wcid}"))
                    .with_int(PropertyInt::Version, 2),
            );
        }
        c
    }

    #[test]
    fn verify_melee_rares_checks_the_world_database_version() {
        let mut ts = server(|_| {});
        let min = "v0.9.271";
        for (patch, line) in [
            (None, format!("Unable to determine World Database version. Your World Database must be {min} or higher to run this command.")),
            (Some("0.9.300"), format!("Unexpected patch version format found. Your World Database must be {min} or higher to run this command.")),
            (Some("v0.9.270"), format!("World Database must be {min} or higher to run this command. Your current World Database patch version is: v0.9.270")),
            (Some("v0.8.999"), format!("World Database must be {min} or higher to run this command. Your current World Database patch version is: v0.8.999")),
            (Some("v0.9.27x"), format!("World Database must be {min} or higher to run this command. Your current World Database patch version is: v0.9.27x")),
        ] {
            ts.world.content = Arc::new(rare_content(patch));
            assert_eq!(console(&mut ts, dfc::handle_fix_melee_rares, &[]), [line], "{patch:?}");
        }
        // a build with a suffix passes on its number
        ts.world.content = Arc::new(rare_content(Some("v0.9.272-rc1")));
        assert_eq!(
            console(&mut ts, dfc::handle_fix_melee_rares, &[]),
            ["Verified 0 melee rares. No changes required."]
        );
    }

    #[test]
    fn verify_melee_rares_converts_and_replaces_rares() {
        const PRE: u32 = 0x8000_0301;
        const V1: u32 = 0x8000_0302;
        const VENDOR: u32 = 0x8000_0303;
        const COIN: u32 = 0x8000_0304;
        let mut ts = server(|w| {
            seed_player(
                w,
                "u64d",
                AccessLevel::Player,
                DELTA,
                "Delta Tester",
                |_| {},
                |c| {
                    c.character_properties_shortcut_bar = vec![CharacterPropertiesShortcutBar {
                        character_id: DELTA,
                        shortcut_bar_index: 0,
                        shortcut_object_id: COIN,
                    }];
                },
            );
            let named = |b: &mut empyrean_entity::Biota, name: &str| {
                b.set_property(PropertyString::Name, name.to_owned())
            };
            item(w, PRE, 30310, WeenieType::MeleeWeapon, |b| {
                named(b, "Ridgeback Dagger");
                b.set_property(PropertyInstanceId::Container, DELTA);
                b.set_property(PropertyInt::PlacementPosition, 3);
            });
            item(w, V1, 45461, WeenieType::MeleeWeapon, |b| {
                named(b, "Frozen Eye");
                b.set_property(PropertyInt::Version, 1);
                let p = PropertiesPosition {
                    obj_cell_id: 0xA9B4_0019,
                    position_x: 1.5,
                    position_y: 2.0,
                    position_z: 3.0,
                    rotation_w: 1.0,
                    ..PropertiesPosition::default()
                };
                b.properties_position
                    .get_or_insert_with(DotNetDict::new)
                    .insert(PositionType::Location, p);
            });
            item(w, VENDOR, 45444, WeenieType::MeleeWeapon, |b| {
                named(b, "Ridgeback Dagger");
                b.set_property(PropertyInstanceId::Wielder, DELTA);
                b.set_property(
                    PropertyInt::CurrentWieldedLocation,
                    EquipMask::MeleeWeapon.0.cast_signed(),
                );
            });
            item(w, COIN, 45493, WeenieType::Generic, |b| {
                named(b, "Rare Coin");
                b.set_property(PropertyInstanceId::Container, DELTA);
            });
        });
        ts.world.content = Arc::new(rare_content(Some("v0.9.280")));
        guid_manager::initialize(&mut ts.world, &mut EmptyShard);

        let out = console(&mut ts, dfc::handle_fix_melee_rares, &[]);
        assert_eq!(out, [
            "0x80000302 Frozen Eye (45461) is on a landblock and located at:\n 0xA9B40019 [1.500000 2.000000 3.000000] 1.000000 0.000000 0.000000 0.000000\n\\----- Version needs updating, no other changes required.",
            "0x80000303 Ridgeback Dagger (45444) is wielded by: \n 0x50000001 Delta Tester in the MeleeWeapon slot\n\\----- Rare obtained from Melee Rare Vendor, needs to be deleted and replaced with a random newly generated melee rare.",
            "0x80000301 Ridgeback Dagger (30310) is contained by: \n 0x50000001 Delta Tester's Main Pack at placement position 3\n\\----- Rare needs to change WCID from 30310 to 45444 and have its version updated.",
            "0x80000304 Rare Coin (45493) is contained by: \n 0x50000001 Delta Tester's Main Pack at placement position 0\n\\----- Rare Coin obtained from Emissary of Asheron (45492), needs to be deleted and replaced with a random newly generated melee rare.",
            "Found 1 Pre-MoA Rares. These need to be converted to Post-MoA WCIDs and their version updated to V2.",
            "Found 2 Post-MoA Rares. 1 are invalid and need to be regenerated due to incorrectly being distributed by Melee Rare Vendor. 1 need to have their version updated to V2.",
            "Found 1 Rare Coins. These need to be deleted and replaced with newly randomly generated rare.",
            "Dry run completed. Type 'verify-melee-rares fix' to fix any issues.",
        ]);

        let out = console(&mut ts, dfc::handle_fix_melee_rares, &["fix"]);
        assert_eq!(out[out.len() - 3..], [
            "Found 1 Pre-MoA Rares. 1 were converted to Post-MoA WCIDs and their version updated to V2.",
            "Found 2 Post-MoA Rares. 0 valid, 1 deleted, 1 replaced, 1 updated to V2.",
            "Found 1 Rare Coins. 1 deleted, 1 replaced.",
        ]);
        let pre = stored(&ts, PRE);
        assert_eq!(
            (
                pre.weenie_class_id,
                pre.get_property_int(PropertyInt::Version)
            ),
            (45444, Some(2))
        );
        assert_eq!(
            stored(&ts, V1).get_property_int(PropertyInt::Version),
            Some(2)
        );
        assert!(
            load_biota(&mut **ts.shard(), VENDOR).is_none(),
            "the vendor rare is deleted"
        );
        assert!(
            load_biota(&mut **ts.shard(), COIN).is_none(),
            "the coin is deleted"
        );

        // the replacements keep the links: the wielded one in its slot, the coin's in the pack and on
        // the shortcut bar
        let replaced = |s: &str| {
            u32::from_str_radix(
                s.split("Replaced with 0x")
                    .nth(1)
                    .unwrap()
                    .get(..8)
                    .unwrap(),
                16,
            )
            .unwrap()
        };
        let vendor_line = out.iter().find(|l| l.starts_with("0x80000303")).unwrap();
        let new_vendor = stored(&ts, replaced(vendor_line));
        assert!(
            (45436..=45470).contains(&new_vendor.weenie_class_id)
                && new_vendor.weenie_class_id != 45444,
            "a different EoR rare"
        );
        assert_eq!(
            new_vendor.get_property_iid(PropertyInstanceId::Wielder),
            Some(DELTA)
        );
        assert_eq!(
            new_vendor.get_property_int(PropertyInt::CurrentWieldedLocation),
            Some(EquipMask::MeleeWeapon.0.cast_signed())
        );
        let coin_line = out.iter().find(|l| l.starts_with("0x80000304")).unwrap();
        let new_coin = replaced(coin_line);
        assert_eq!(
            stored(&ts, new_coin).get_property_iid(PropertyInstanceId::Container),
            Some(DELTA)
        );
        let c = ts.shard().get_character(DELTA).unwrap();
        assert_eq!(
            c.character_properties_shortcut_bar[0].shortcut_object_id,
            new_coin
        );

        let last = console(&mut ts, dfc::handle_fix_melee_rares, &[]);
        assert_eq!(
            last.last().map(String::as_str),
            Some("Verified 4 melee rares. No changes required."),
            "{last:?}"
        );
    }

    #[test]
    fn verify_beneficial_enchantments_sets_the_missing_flag() {
        let mut spells = sample::spell_table();
        let mut beneficial = spells.spells[&1].clone();
        beneficial.bitfield = empyrean_entity::enums::SpellFlags::Beneficial
            .0
            .cast_unsigned();
        beneficial.name = "Test Buff".into();
        let mut harmful = beneficial.clone();
        harmful.bitfield = 0;
        harmful.meta_spell_id = 2;
        spells.spells.insert(1, beneficial);
        spells.spells.insert(2, harmful);
        let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .with_spell_table(spells)
            .build()
            .expect("fake dats");
        let mut ts = TestServer::with_setup(dats, |w| {
            let mut b = Biota {
                id: 0x8000_0400,
                weenie_class_id: 3001,
                weenie_type: 1,
                ..Biota::default()
            };
            let e = |spell_id: i32, layer_id: u16, stat_mod_type: u32| {
                BiotaPropertiesEnchantmentRegistry {
                    object_id: 0x8000_0400,
                    spell_id,
                    layer_id,
                    stat_mod_type,
                    ..BiotaPropertiesEnchantmentRegistry::default()
                }
            };
            b.biota_properties_enchantment_registry =
                vec![e(1, 1, 0x10), e(1, 2, 0), e(2, 1, 0x10)];
            empyrean_store::shard_database::set_biota_populated_collections(&mut b);
            w.shard
                .base_database()
                .write_biota(&mut b)
                .expect("written");
        });
        assert_eq!(
            console(&mut ts, dfc::handle_enchantments, &[]),
            [
                "Spell Test Buff (1) on 0x80000400 is missing Beneficial flag",
                "Dry run completed. Type 'verify-beneficial-enchantments fix' to fix 1 issues.",
            ]
        );
        assert_eq!(
            console(&mut ts, dfc::handle_enchantments, &["fix"]),
            [
                "Spell Test Buff (1) on 0x80000400 is missing Beneficial flag -- fixed",
                "Fixed 1 incorrect enchantments",
            ]
        );
        let b = stored(&ts, 0x8000_0400);
        let mods: Vec<u32> = b
            .biota_properties_enchantment_registry
            .iter()
            .map(|e| e.stat_mod_type)
            .collect();
        assert_eq!(mods, [0x10 | 0x0200_0000, 0, 0x10]);
        assert_eq!(
            console(&mut ts, dfc::handle_enchantments, &[]),
            ["Verified 3 enchantments"]
        );
    }

    #[test]
    fn fix_biota_emote_delay_matches_emotes_to_their_weenie() {
        const GUARD: u32 = 0x8000_0500;
        let emote_action = |order: u32, delay: f32| BiotaPropertiesEmoteAction {
            order,
            r#type: 1,
            delay,
            ..BiotaPropertiesEmoteAction::default()
        };
        let mut ts = server(|w| {
            let mut b = Biota {
                id: GUARD,
                weenie_class_id: 3050,
                weenie_type: 10,
                ..Biota::default()
            };
            b.biota_properties_emote = vec![
                BiotaPropertiesEmote {
                    object_id: GUARD,
                    category: 7,
                    probability: 1.0,
                    biota_properties_emote_action: vec![emote_action(0, 1.0), emote_action(1, 1.0)],
                    ..BiotaPropertiesEmote::default()
                },
                BiotaPropertiesEmote {
                    object_id: GUARD,
                    category: 8,
                    probability: 1.0,
                    biota_properties_emote_action: vec![emote_action(0, 1.0)],
                    ..BiotaPropertiesEmote::default()
                },
            ];
            empyrean_store::shard_database::set_biota_populated_collections(&mut b);
            w.shard
                .base_database()
                .write_biota(&mut b)
                .expect("written");
        });
        let mut weenie = Weenie::new(3050, "u64guard", WeenieType::Creature);
        weenie.weenie_properties_emote.push(WeeniePropertiesEmote {
            id: 1,
            object_id: 3050,
            category: 7,
            probability: 1.0,
            weenie_properties_emote_action: vec![WeeniePropertiesEmoteAction {
                id: 1,
                emote_id: 1,
                order: 0,
                r#type: 1,
                delay: 0.0,
                ..WeeniePropertiesEmoteAction::default()
            }],
            ..WeeniePropertiesEmote::default()
        });
        ts.world.content = Arc::new(MemContent::new().weenie(weenie));

        let name = empyrean_entity::enums::WeenieClassName(3050).to_dotnet_string();
        assert_eq!(console(&mut ts, dfc::handle_fix_biota_emote_delay, &[]), [
            "This command is intended to be run while the world is in offline mode, or there are 0 players connected.",
            "To run this fix, type fix-biota-emote-delay fix",
        ]);
        assert_eq!(
            console(&mut ts, dfc::handle_fix_biota_emote_delay, &["dry"]),
            [
                "Building weenie emote cache".to_owned(),
                "Found 1 weenie templates w/ emote actions with delay 0".to_owned(),
                "Finding biotas for these wcids".to_owned(),
                "Found 1 biotas matching 1 distinct wcids".to_owned(),
                format!("3050 - {name} (1)"),
                "Dry run completed".to_owned(),
            ]
        );
        let out = console(&mut ts, dfc::handle_fix_biota_emote_delay, &["FIX"]);
        assert_eq!(
            out[5..],
            [
                format!("Fixed shard object 80000500 of type 3050 - {name}"),
                "Completed successfully, fixed 1 shard items".to_owned()
            ]
        );
        let b = stored(&ts, GUARD);
        let delays: Vec<(u32, u32, f32)> = b
            .biota_properties_emote
            .iter()
            .flat_map(|e| {
                e.biota_properties_emote_action
                    .iter()
                    .map(move |a| (e.category, a.order, a.delay))
            })
            .collect();
        // only the category-7 emote's order-0 action has a delay-0 twin in the weenie
        assert_eq!(delays, [(7, 0, 0.0), (7, 1, 1.0), (8, 0, 1.0)]);
    }
}
