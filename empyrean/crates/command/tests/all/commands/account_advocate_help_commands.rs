//! Vectors: empyrean/fixtures/vectors/commands/{help,fix,db,perf,advocate}_*
//! Acecommands/acehelp, contains-ignore-case, credits, heritage augs, armor levels, emote hashes,
//! shortcut SQL, perf lines and bestow parsing equal ACE vectors; handlers registered once; loot-
//! test params checked; content folder swap; SQL file past blank lines.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::collections::{BTreeMap, HashSet};

use empyrean_command::command_handler::CommandHandler;
use empyrean_command::command_manager;
use empyrean_command::handlers::processors::database_perf_test;
use empyrean_command::handlers::{
    advocate_commands as adv, developer_database_commands as ddc, developer_fix_commands as dfc,
    developer_loot_commands as dlc, help_commands as hc,
};
use empyrean_common::dotnet::{DotNetDict, TimeSpan};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, i64_of, u64_of};
use empyrean_entity::enums::{EquipMask, HeritageGroup};
use empyrean_world::entity::tinker_log::TinkerLog;
use serde_json::Value;

/// ACE's handler method name for each registered handler, with the handler's name below
/// `empyrean_command::handlers` (`module::function`).
fn table() -> [(&'static str, &'static str); 37] {
    [
        (
            "HandleAccountCreate",
            "account_commands::handle_account_create",
        ),
        ("HandleAccountGet", "account_commands::handle_account_get"),
        (
            "HandleAccountUpdateAccessLevel",
            "account_commands::handle_account_update_access_level",
        ),
        (
            "HandleAccountSetPassword",
            "account_commands::handle_account_set_password",
        ),
        ("HandlePasswd", "account_commands::handle_passwd"),
        ("HandleAttackable", "advocate_commands::handle_attackable"),
        ("HandleBestow", "advocate_commands::handle_bestow"),
        ("HandleRemove", "advocate_commands::handle_remove"),
        ("HandleTele", "advocate_commands::handle_tele"),
        (
            "HandleCharacterTokenization",
            "character_commands::handle_character_tokenization",
        ),
        (
            "HandleCharacterForcedDelete",
            "character_commands::handle_character_forced_delete",
        ),
        (
            "HandleDatabaseQueueInfo",
            "developer_database_commands::handle_database_queue_info",
        ),
        (
            "HandleDatabasePerfTest",
            "developer_database_commands::handle_database_perf_test",
        ),
        (
            "HandleFixShortcutBars",
            "developer_database_commands::handle_fix_shortcut_bars",
        ),
        (
            "HandleDatabaseShardCachePBRT",
            "developer_database_commands::handle_database_shard_cache_pbrt",
        ),
        (
            "HandleDatabaseShardCacheNPBRT",
            "developer_database_commands::handle_database_shard_cache_npbrt",
        ),
        (
            "HandleFixSpellBars",
            "developer_database_commands::handle_fix_spell_bars",
        ),
        (
            "HandleFixGearPlating",
            "developer_database_commands::handle_fix_gear_plating",
        ),
        (
            "HandleVerifyAll",
            "developer_fix_commands::handle_verify_all",
        ),
        (
            "HandleVerifyAttributes",
            "developer_fix_commands::handle_verify_attributes",
        ),
        (
            "HandleVerifyVitals",
            "developer_fix_commands::handle_verify_vitals",
        ),
        (
            "HandleVerifySkills",
            "developer_fix_commands::handle_verify_skills",
        ),
        (
            "HandleVerifySkillCredits",
            "developer_fix_commands::handle_verify_skill_credits",
        ),
        (
            "HandleVerifyHeritageAugs",
            "developer_fix_commands::handle_verify_heritage_augs",
        ),
        (
            "HandleVerifyMaxAugs",
            "developer_fix_commands::handle_verify_max_augs",
        ),
        (
            "HandleVerifyExperience",
            "developer_fix_commands::handle_verify_experience",
        ),
        (
            "HandleFixBiotaEmoteDelay",
            "developer_fix_commands::handle_fix_biota_emote_delay",
        ),
        (
            "HandleFixArmorLevel",
            "developer_fix_commands::handle_fix_armor_level",
        ),
        (
            "HandleVerifyClothingWieldLevel",
            "developer_fix_commands::handle_verify_clothing_wield_level",
        ),
        (
            "HandleVerifyLegendaryWieldLevel",
            "developer_fix_commands::handle_verify_legendary_wield_level",
        ),
        (
            "HandleRemoveShieldRatings",
            "developer_fix_commands::handle_remove_shield_ratings",
        ),
        (
            "HandleFixMeleeRares",
            "developer_fix_commands::handle_fix_melee_rares",
        ),
        (
            "HandleEnchantments",
            "developer_fix_commands::handle_enchantments",
        ),
        (
            "TestLootGenerator",
            "developer_loot_commands::test_loot_generator",
        ),
        (
            "TestLootGeneratorCorpse",
            "developer_loot_commands::test_loot_generator_corpse",
        ),
        ("HandleACEHelp", "help_commands::handle_ace_help"),
        ("HandleACECommands", "help_commands::handle_ace_commands"),
    ]
}

pub fn handler_of(method: &str) -> Option<&'static str> {
    table().iter().find(|(m, _)| *m == method).map(|(_, h)| *h)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("an array")
        .iter()
        .map(|s| s.as_str().expect("a string").to_owned())
        .collect()
}

/// The command types this port has registered (the registry test's `mine`).
const PORTED_TYPES: [&str; 14] = [
    "AccountCommands",
    "AdminCommands",
    "AdminShardCommands",
    "AdminStatCommands",
    "AdvocateCommands",
    "CharacterCommands",
    "ConsoleCommands",
    "DeveloperContentCommands",
    "DeveloperDatabaseCommands",
    "DeveloperFixCommands",
    "DeveloperLootCommands",
    "HelpCommands",
    "PlayerCommands",
    "SentinelCommands",
];

/// ACE's command name => the type of its handler in the final table.
fn ace_command_types() -> BTreeMap<String, String> {
    vectors::load_named("commands", "command_registry")
        .cases
        .iter()
        .map(|c| {
            (
                c.output["command"].as_str().unwrap().to_owned(),
                c.output["type"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// An `acecommands` listing split into its entries (a description may span lines), keeping only the
/// commands whose ACE handler is in a ported type (6.3's DeveloperCommands and the mod loader's
/// ModCommands are not in this port's table yet; other tests' own commands are not in ACE's).
fn listing_entries(text: &str, types: &BTreeMap<String, String>) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for line in text.strip_suffix('\n').unwrap_or(text).split('\n') {
        if line.starts_with('@') || entries.is_empty() {
            entries.push(line.to_owned());
        } else if let Some(last) = entries.last_mut() {
            last.push('\n');
            last.push_str(line);
        }
    }
    entries
        .into_iter()
        .filter(|e| {
            let Some(rest) = e.strip_prefix('@') else {
                return true;
            };
            let command = rest.split(" - ").next().unwrap_or("");
            // (Empyrean's rows are checked in `brand`)
            types
                .get(command)
                .is_some_and(|t| PORTED_TYPES.contains(&t.as_str()))
                && !crate::brand::is_brand_row(command)
        })
        .collect()
}

/// What a console command wrote: each `Console.WriteLine` (a line may hold several).
fn console(
    ts: &mut empyrean_testkit::TestServer,
    handler: CommandHandler,
    parameters: &[&str],
) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    command_manager::take_console_output()
}

#[test]
fn acecommands_from_the_console_matches_ace() {
    command_manager::initialize(None);
    let mut ts = empyrean_testkit::TestServer::new();
    let types = ace_command_types();
    let file = vectors::load_named("commands", "help_acecommands_console");
    for case in &file.cases {
        let parameters = strings(&case.input["parameters"]);
        let p: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let got = console(&mut ts, hc::handle_ace_commands, &p)
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        let expected = &vectors::brand_ruled(case.output.as_str().unwrap());
        assert_eq!(
            listing_entries(&got, &types),
            listing_entries(expected, &types),
            "{parameters:?}"
        );
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn acehelp_from_the_console_matches_ace() {
    command_manager::initialize(None);
    let mut ts = empyrean_testkit::TestServer::new();
    let types = ace_command_types();
    let file = vectors::load_named("commands", "help_acehelp_console");
    for case in &file.cases {
        let parameters = strings(&case.input["parameters"]);
        let p: Vec<&str> = parameters.iter().map(String::as_str).collect();
        if parameters
            .first()
            .is_some_and(|p| crate::brand::is_brand_row(p))
        {
            // Brand-specific help, including case-insensitive aliases, is covered by
            // `commands::empyrean_commands::both_names_answer_on_the_console`.
            continue;
        }
        let got = console(&mut ts, hc::handle_ace_help, &p)
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        let expected = &vectors::brand_ruled(case.output.as_str().unwrap());
        if parameters.first().is_some_and(|p| p == "commands") {
            assert_eq!(
                listing_entries(&got, &types),
                listing_entries(expected, &types),
                "{parameters:?}"
            );
        } else {
            assert_eq!(&got, expected, "{parameters:?}");
        }
    }
}

#[test]
fn contains_ordinal_ignore_case_as_net() {
    assert!(hc::contains_ordinal_ignore_case(
        "Admin set-accountaccess Change",
        "ADMIN SET"
    ));
    assert!(hc::contains_ordinal_ignore_case("abc", ""));
    assert!(!hc::contains_ordinal_ignore_case("abc", "abcd"));
    // .NET's ordinal casing never maps a non-ASCII character to an ASCII one
    assert!(!hc::contains_ordinal_ignore_case("Admin", "\u{131}"));
}

#[test]
fn additional_credits_match_ace() {
    let file = vectors::load_named("commands", "fix_additional_credits");
    for case in &file.cases {
        let level = i32::try_from(i64_of(&case.input["level"]).unwrap()).unwrap();
        assert_eq!(
            i64::from(dfc::get_additional_credits(level)),
            i64_of(&case.output).unwrap(),
            "{level}"
        );
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn heritage_augs_match_ace() {
    let file = vectors::load_named("commands", "fix_heritage_aug");
    for case in &file.cases {
        let h = i32::try_from(i64_of(&case.input["heritage"]).unwrap()).unwrap();
        let got = dfc::get_heritage_aug(HeritageGroup(h)).map(|p| i64::from(p.0));
        assert_eq!(got, i64_of(&case.output), "{h}");
    }
}

#[test]
fn armor_levels_match_ace_over_a_seeded_generator() {
    let file = vectors::load_named("commands", "fix_armor_level");
    for case in &file.cases {
        let i = &case.input;
        ThreadSafeRandom::seed(u64_of(&i["seed"]).unwrap());
        let tinker_log = i["tinker_log"].as_str().map(|s| TinkerLog::new(Some(s)));
        let got = dfc::get_armor_level(
            i32::try_from(i64_of(&i["armor_level"]).unwrap()).unwrap(),
            EquipMask(u32::try_from(u64_of(&i["equip_mask"]).unwrap()).unwrap()),
            tinker_log.as_ref(),
            i32::try_from(i64_of(&i["num_tinkers"]).unwrap()).unwrap(),
            i32::try_from(i64_of(&i["imbued_effect"]).unwrap()).unwrap(),
        );
        let next = ThreadSafeRandom::next(0, 1000);
        assert_eq!(
            i64::from(got),
            i64_of(&case.output["level"]).unwrap(),
            "{i}"
        );
        assert_eq!(
            i64::from(next),
            i64_of(&case.output["next"]).unwrap(),
            "{i}: draws"
        );
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn emote_hashes_match_ace() {
    let file = vectors::load_named("commands", "fix_emote_hash");
    let opt_u32 = |v: &Value| u64_of(v).map(|x| u32::try_from(x).unwrap());
    for case in &file.cases {
        let i = &case.input;
        let got = dfc::calculate_emote_hash(
            opt_u32(&i["category"]).unwrap(),
            f32::from_bits(opt_u32(&i["probability_bits"]).unwrap()),
            opt_u32(&i["wcid"]),
            opt_u32(&i["style"]),
            opt_u32(&i["substyle"]),
            None,
            i64_of(&i["vendor_type"]).map(|x| i32::try_from(x).unwrap()),
        );
        assert_eq!(i64::from(got), i64_of(&case.output).unwrap(), "{i}");
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn shortcut_sql_matches_ace() {
    let file = vectors::load_named("commands", "db_output_shortcut_sql");
    for case in &file.cases {
        let id = u32::try_from(u64_of(&case.input["character_id"]).unwrap()).unwrap();
        let mut d = DotNetDict::new();
        for pair in case.input["idx_to_obj"].as_array().unwrap() {
            d.insert(
                u32::try_from(u64_of(&pair[0]).unwrap()).unwrap(),
                u32::try_from(u64_of(&pair[1]).unwrap()).unwrap(),
            );
        }
        assert_eq!(
            ddc::output_shortcut_sql_command(Some("x"), id, &d),
            strings(&case.output)
        );
    }
}

#[test]
fn perf_report_lines_match_ace() {
    let file = vectors::load_named("commands", "perf_report_result");
    for case in &file.cases {
        let i = &case.input;
        let ts = |k: &str| TimeSpan::from_ticks(i64_of(&i[k]).unwrap());
        let got = database_perf_test::report_text(
            i["description"].as_str().unwrap(),
            i32::try_from(i64_of(&i["biotas_per_test"]).unwrap()).unwrap(),
            ts("duration"),
            ts("queue_wait_time"),
            ts("total_query_execution_time"),
            i64_of(&i["true_results"]).unwrap(),
            i64_of(&i["false_results"]).unwrap(),
        );
        assert_eq!(got, case.output.as_str().unwrap(), "{i}");
    }
}

#[test]
fn bestow_name_parsing_matches_ace() {
    let file = vectors::load_named("commands", "advocate_bestow_name");
    let mut fixed = 0;
    for case in &file.cases {
        let parameters = strings(&case.input["parameters"]);
        let (level, advocate_level, advocate_name) = adv::parse_bestow(&parameters);
        assert_eq!(
            level,
            case.output["level"].as_str().unwrap(),
            "{parameters:?}"
        );
        assert_eq!(
            advocate_level.map(i64::from),
            i64_of(&case.output["advocate_level"]),
            "{parameters:?}"
        );
        // V360/V362 (a fix): the name is the words before the level; ACE trimmed every trailing
        // character of the level's digits ("Bob 11 1" gave "Bob")
        let name = parameters[..parameters.len() - 1]
            .join(" ")
            .trim()
            .to_owned();
        assert_eq!(advocate_name, name, "{parameters:?}");
        if advocate_name != case.output["advocate_name"].as_str().unwrap() {
            fixed += 1;
        }
    }
    assert!(fixed > 0, "the vectors hold names ACE cut short");
    assert_eq!(
        adv::parse_bestow(&strings(&serde_json::json!(["Bob", "11", "1"]))).2,
        "Bob 11"
    );
}

#[test]
fn every_handler_is_registered_once() {
    command_manager::initialize(None);
    let mine: HashSet<&str> = [
        "AccountCommands",
        "AdvocateCommands",
        "CharacterCommands",
        "DeveloperDatabaseCommands",
        "DeveloperFixCommands",
        "DeveloperLootCommands",
        "HelpCommands",
    ]
    .into_iter()
    .collect();
    let types = ace_command_types();
    let expected: Vec<&String> = types
        .iter()
        .filter(|(_, t)| mine.contains(t.as_str()))
        .map(|(c, _)| c)
        .collect();
    for command in expected {
        let found = command_manager::get_command_by_name(command);
        assert_eq!(found.len(), 1, "{command}");
        assert!(
            table().iter().any(|(_, h)| found[0]
                .handler_name
                .strip_prefix("empyrean_command::handlers::")
                == Some(*h)),
            "{command}"
        );
    }
}

#[test]
fn loot_test_commands_check_their_parameters_on_the_console() {
    let mut ts = empyrean_testkit::TestServer::new();
    let info = console(&mut ts, dlc::test_loot_generator, &["-info"]);
    assert_eq!(info.len(), 1);
    assert!(
        info[0].starts_with("Usage: \n<number of items> <loot tier>"),
        "{info:?}"
    );
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator, &["x"]),
        ["Number of items is not an integer"]
    );
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator, &["5", "x"]),
        ["Tier is not an integer"]
    );
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator, &["5", "9"]),
        ["Tier must be 1-8.  You entered tier 9, which does not exist!"]
    );
    let bad_table = "Invalid Table Option.  Available Tables to show are melee, missile, caster, jewelry, armor, cloak, pet, aetheria or all.";
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator, &["5", "1", "nope"]),
        [bad_table]
    );
    assert_eq!(
        console(
            &mut ts,
            dlc::test_loot_generator,
            &["5", "1", "MELEE", "log"]
        ),
        ["Invalid Option.  To log a file, use option -log"]
    );

    assert!(
        console(&mut ts, dlc::test_loot_generator_corpse, &["-info"])[0]
            .contains("testlootgencorpse 998 50 caster")
    );
    let sim = " LootFactory Simulator \n ---------------------\n";
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator_corpse, &["1"]),
        [format!("{sim} Need to specify number of coprses\n")]
    );
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator_corpse, &["x", "1"]),
        [format!("{sim} DID specified is not an integer \n")]
    );
    assert_eq!(
        console(&mut ts, dlc::test_loot_generator_corpse, &["1", "x"]),
        [format!("{sim} Invalid Parameter - Must be a number \n")]
    );
    assert_eq!(
        console(
            &mut ts,
            dlc::test_loot_generator_corpse,
            &["1", "2", "pet", "-x"]
        ),
        ["Invalid Option.  To log a file, use option -log"]
    );
}

#[test]
fn content_folders_swap_only_their_own_level() {
    // V364 (a fix): ACE replaced every "sql"/"json" in the whole path
    use empyrean_command::handlers::developer_content_commands::swap_segment;
    assert_eq!(
        swap_segment(r"C:\mysql\content\sql\weenies\", "sql", "json"),
        r"C:\mysql\content\json\weenies\"
    );
    assert_eq!(
        swap_segment("/srv/json-data/content/json/recipes/", "json", "sql"),
        "/srv/json-data/content/sql/recipes/"
    );
    assert_eq!(
        swap_segment("/c/json/json/quests/", "json", "sql"),
        "/c/json/sql/quests/",
        "the last such level"
    );
    assert_eq!(
        swap_segment(
            &swap_segment(
                "/c/content/json/spawnmaps/",
                "spawnmaps",
                "landblock_instances"
            ),
            "json",
            "sql"
        ),
        "/c/content/sql/landblock_instances/"
    );
    assert_eq!(
        swap_segment("/c/content/myjson/", "json", "sql"),
        "/c/content/mysql/",
        "no such level: every occurrence, as before"
    );
}

#[test]
fn a_sql_file_type_is_read_past_blank_lines() {
    // V364 (a fix): ACE's reader never ended on a blank first line
    use empyrean_command::handlers::developer_content_commands::{get_sql_file_type, FileType};
    let dir = std::env::temp_dir().join(format!("empyrean-sql-file-type-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = |name: &str, text: &str| {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().into_owned()
    };
    assert_eq!(
        get_sql_file_type(&file(
            "a.sql",
            "\r\n  \r\nDELETE FROM `weenie` WHERE `class_Id` = 1;\r\n"
        )),
        FileType::Weenie
    );
    assert_eq!(
        get_sql_file_type(&file("b.sql", "DELETE FROM `recipe` WHERE `id` = 1;\n")),
        FileType::Recipe
    );
    assert_eq!(
        get_sql_file_type(&file("c.sql", "\n\n")),
        FileType::Undefined
    );
    assert_eq!(get_sql_file_type(&file("d.sql", "")), FileType::Undefined);
    assert_eq!(
        get_sql_file_type(&file("e.sql", "\n-- a comment\nDELETE FROM `weenie`;\n")),
        FileType::Undefined,
        "the first line that is not blank decides"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
