//! Vectors: empyrean/fixtures/vectors/commands/admin_*
//! Admin command helpers (enum/bool parse, HUD line, wildcard, culture order, environ list,
//! shutdown broadcast) equal ACE's compiled output; world open/close, shutdown, stat, watchmen,
//! properties, exit, event and dat-export usage behave as ACE on the console.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_command::command_handler::CommandHandler;
use empyrean_command::command_manager;
use empyrean_command::handlers::{
    admin_commands as ac, admin_shard_commands as asc, admin_stat_commands as ast,
};
use empyrean_common::dotnet::TimeSpan;
use empyrean_common::vectors::{self, i64_of, u64_of};
use empyrean_entity::enums::WeenieType;
use empyrean_testkit::TestServer;
use empyrean_world::managers::server_manager;
use empyrean_world::managers::world_manager::WorldStatusState;

pub fn handler_of(method: &str) -> Option<&'static str> {
    let table: [(&str, &str); 115] = [
        ("HandleAdminvision", "admin_commands::handle_adminvision"),
        ("HandleAdminui", "admin_commands::handle_adminui"),
        (
            "HandleDeleteSelected",
            "admin_commands::handle_delete_selected",
        ),
        ("HandleDraw", "admin_commands::handle_draw"),
        ("HandleFinger", "admin_commands::handle_finger"),
        ("HandleFreeze", "admin_commands::handle_freeze"),
        ("HandleUnFreeze", "admin_commands::handle_un_freeze"),
        ("HandleGag", "admin_commands::handle_gag"),
        ("HandleUnGag", "admin_commands::handle_un_gag"),
        ("HandleHome", "admin_commands::handle_home"),
        ("HandleMRT", "admin_commands::handle_mrt"),
        ("HandleLimbo", "admin_commands::handle_limbo"),
        ("HandleMyIID", "admin_commands::handle_my_iid"),
        ("HandleMyServer", "admin_commands::handle_my_server"),
        ("HandlePk", "admin_commands::handle_pk"),
        (
            "HandleQuerypluginlist",
            "admin_commands::handle_querypluginlist",
        ),
        ("HandleQueryplugin", "admin_commands::handle_queryplugin"),
        ("HandleRepeat", "admin_commands::handle_repeat"),
        ("HandleRegen", "admin_commands::handle_regen"),
        ("HandleSave", "admin_commands::handle_save"),
        ("HandleServerlist", "admin_commands::handle_serverlist"),
        ("HandleSnoop", "admin_commands::handle_snoop"),
        ("HandleSmite", "admin_commands::handle_smite"),
        ("HandleTeleto", "admin_commands::handle_teleto"),
        ("HandleTeleToMe", "admin_commands::handle_tele_to_me"),
        ("HandleTeleReturn", "admin_commands::handle_tele_return"),
        ("HandleTeleAllTo", "admin_commands::handle_tele_all_to"),
        ("HandleTeleportPoi", "admin_commands::handle_teleport_poi"),
        ("HandleTeleportLOC", "admin_commands::handle_teleport_loc"),
        ("HandleTime", "admin_commands::handle_time"),
        ("HandleTrophies", "admin_commands::handle_trophies"),
        ("HandleUnlock", "admin_commands::handle_unlock"),
        ("HandleGamecast", "admin_commands::handle_gamecast"),
        ("HandleAddSpell", "admin_commands::handle_add_spell"),
        ("HandleRemoveSpell", "admin_commands::handle_remove_spell"),
        ("HandleAdminhouse", "admin_commands::handle_adminhouse"),
        ("HandleBornAgain", "admin_commands::handle_born_again"),
        ("HandleCopychar", "admin_commands::handle_copychar"),
        ("HandleCreate", "admin_commands::handle_create"),
        (
            "HandleCreateLiveOps",
            "admin_commands::handle_create_live_ops",
        ),
        ("HandleCreateNamed", "admin_commands::handle_create_named"),
        ("HandleCI", "admin_commands::handle_ci"),
        ("HandleCrack", "admin_commands::handle_crack"),
        ("HandleDeathxp", "admin_commands::handle_deathxp"),
        ("Handlede_n", "admin_commands::handlede_n"),
        (
            "Handledirect_emote_name",
            "admin_commands::handledirect_emote_name",
        ),
        ("Handlede_s", "admin_commands::handlede_s"),
        (
            "Handledirect_emote_select",
            "admin_commands::handledirect_emote_select",
        ),
        ("HandleDispel", "admin_commands::handle_dispel"),
        ("HandleEvent", "admin_commands::handle_event"),
        ("HandleFumble", "admin_commands::handle_fumble"),
        ("HandleGod", "admin_commands::handle_god"),
        ("HandleUngod", "admin_commands::handle_ungod"),
        ("HandleMagicGod", "admin_commands::handle_magic_god"),
        ("HandleModifyVital", "admin_commands::handle_modify_vital"),
        ("HandleModifySkill", "admin_commands::handle_modify_skill"),
        (
            "HandleModifyAttribute",
            "admin_commands::handle_modify_attribute",
        ),
        ("HandleHeal", "admin_commands::handle_heal"),
        ("HandleHousekeep", "admin_commands::handle_housekeep"),
        ("HandleIDlist", "admin_commands::handle_i_dlist"),
        (
            "HandleGameCastLocalEmote",
            "admin_commands::handle_game_cast_local_emote",
        ),
        ("HandleLocation", "admin_commands::handle_location"),
        ("HandleMorph", "admin_commands::handle_morph"),
        ("Handleqst", "admin_commands::handleqst"),
        ("HandleRaise", "admin_commands::handle_raise"),
        ("HandleRename", "admin_commands::handle_rename"),
        ("Handlesetadvclass", "admin_commands::handlesetadvclass"),
        ("HandleSpendxp", "admin_commands::handle_spendxp"),
        ("Handletrainskill", "admin_commands::handletrainskill"),
        ("Handlereloadsysmsg", "admin_commands::handlereloadsysmsg"),
        (
            "HandleGameCastLocal",
            "admin_commands::handle_game_cast_local",
        ),
        ("HandleSticky", "admin_commands::handle_sticky"),
        ("Handleuserlimit", "admin_commands::handleuserlimit"),
        ("Handlewatchmen", "admin_commands::handlewatchmen"),
        (
            "HandleGameCastEmote",
            "admin_commands::handle_game_cast_emote",
        ),
        ("HandleWe", "admin_commands::handle_we"),
        ("Handledumpattackers", "admin_commands::handledumpattackers"),
        ("Handleknownobjs", "admin_commands::handleknownobjs"),
        ("Handlelbinterval", "admin_commands::handlelbinterval"),
        ("Handlelbthresh", "admin_commands::handlelbthresh"),
        ("Handleradar", "admin_commands::handleradar"),
        ("HandleRaresDump", "admin_commands::handle_rares_dump"),
        (
            "Handlestormnumstormed",
            "admin_commands::handlestormnumstormed",
        ),
        ("Handlestormthresh", "admin_commands::handlestormthresh"),
        ("HandleDisplayProps", "admin_commands::handle_display_props"),
        (
            "HandleModifyServerBoolProperty",
            "admin_commands::handle_modify_server_bool_property",
        ),
        (
            "HandleFetchServerBoolProperty",
            "admin_commands::handle_fetch_server_bool_property",
        ),
        (
            "HandleModifyServerLongProperty",
            "admin_commands::handle_modify_server_long_property",
        ),
        (
            "HandleFetchServerLongProperty",
            "admin_commands::handle_fetch_server_long_property",
        ),
        (
            "HandleModifyServerFloatProperty",
            "admin_commands::handle_modify_server_float_property",
        ),
        (
            "HandleFetchServerFloatProperty",
            "admin_commands::handle_fetch_server_float_property",
        ),
        (
            "HandleModifyServerStringProperty",
            "admin_commands::handle_modify_server_string_property",
        ),
        (
            "HandleFetchServerStringProperty",
            "admin_commands::handle_fetch_server_string_property",
        ),
        (
            "HandleModifyPropertyDescription",
            "admin_commands::handle_modify_property_description",
        ),
        (
            "HandleResyncServerProperties",
            "admin_commands::handle_resync_server_properties",
        ),
        (
            "HandleFixAllegiances",
            "admin_commands::handle_fix_allegiances",
        ),
        (
            "HandleShowAllegiances",
            "admin_commands::handle_show_allegiances",
        ),
        (
            "HandleGetEnchantments",
            "admin_commands::handle_get_enchantments",
        ),
        ("HandleCM", "admin_commands::handle_cm"),
        ("HandleCISalvage", "admin_commands::handle_ci_salvage"),
        (
            "HandleSetLBEnviron",
            "admin_commands::handle_set_lb_environ",
        ),
        (
            "HandleSetGlobalEnviron",
            "admin_commands::handle_set_global_environ",
        ),
        ("HandleMoveToMe", "admin_commands::handle_move_to_me"),
        (
            "HandleReloadLootTables",
            "admin_commands::handle_reload_loot_tables",
        ),
        (
            "HandleCancelShutdown",
            "admin_shard_commands::handle_cancel_shutdown",
        ),
        (
            "HandleSetShutdownInterval",
            "admin_shard_commands::handle_set_shutdown_interval",
        ),
        (
            "ShutdownServerNow",
            "admin_shard_commands::shutdown_server_now",
        ),
        ("ShutdownServer", "admin_shard_commands::shutdown_server"),
        ("HandleHelp", "admin_shard_commands::handle_help"),
        ("HandleAllStats", "admin_stat_commands::handle_all_stats"),
        (
            "HandleServerStatus",
            "admin_stat_commands::handle_server_status",
        ),
        (
            "HandleServerPerformance",
            "admin_stat_commands::handle_server_performance",
        ),
        (
            "HandleLandblockStats",
            "admin_stat_commands::handle_landblock_stats",
        ),
        (
            "HandleLBGroupStats",
            "admin_stat_commands::handle_lb_group_stats",
        ),
        ("HandleGCStatus", "admin_stat_commands::handle_gc_status"),
    ];
    table.iter().find(|(m, _)| *m == method).map(|(_, h)| *h)
}

/// `(value, defined)` of one of the admin handlers' `Enum.TryParse` calls.
fn parse_enum(name: &str, s: &str, ignore_case: bool) -> Option<(i64, bool)> {
    match name {
        "SpellId" => {
            ac::try_parse_spell_id(s, ignore_case).map(|e| (i64::from(e.0), e.is_defined()))
        }
        "AccessLevel" => {
            ac::try_parse_access_level(s, ignore_case).map(|e| (i64::from(e.0), e.is_defined()))
        }
        "EnvironChangeType" => ac::try_parse_environ_change_type(s, ignore_case)
            .map(|e| (i64::from(e.0), e.is_defined())),
        "MaterialType" => {
            ac::try_parse_material_type(s, ignore_case).map(|e| (i64::from(e.0), e.is_defined()))
        }
        "PropertyAttribute2nd" => ac::try_parse_property_attribute_2nd(s, ignore_case)
            .map(|e| (i64::from(e.0), e.is_defined())),
        "PropertyAttribute" => ac::try_parse_property_attribute(s, ignore_case)
            .map(|e| (i64::from(e.0), e.is_defined())),
        "Skill" => ac::try_parse_skill(s, ignore_case).map(|e| (i64::from(e.0), e.is_defined())),
        "SkillAdvancementClass" => ac::try_parse_skill_advancement_class(s, ignore_case)
            .map(|e| (i64::from(e.0), e.is_defined())),
        _ => panic!("unknown enum {name}"),
    }
}

#[test]
fn enum_try_parse_matches_net() {
    let file = vectors::load_named("commands", "admin_enum_try_parse");
    for case in &file.cases {
        let name = case.input["enum"].as_str().unwrap();
        let s = case.input["s"].as_str().unwrap();
        let ignore_case = case.input["ignore_case"].as_bool().unwrap();
        let got = parse_enum(name, s, ignore_case);
        let o = &case.output;
        assert!(vectors::throws(o).is_none(), "{name} {s:?}: .NET threw");
        if o["ok"].as_bool().unwrap() {
            let (value, defined) = got.unwrap_or_else(|| panic!("{name} {s:?}: .NET parsed it"));
            assert_eq!(value, i64_of(&o["value"]).unwrap(), "{name} {s:?}: value");
            assert_eq!(
                defined,
                o["defined"].as_bool().unwrap(),
                "{name} {s:?}: defined"
            );
        } else {
            assert_eq!(got, None, "{name} {s:?}: .NET failed");
        }
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn verify_create_weenie_type_matches_ace() {
    let file = vectors::load_named("commands", "admin_verify_create_weenie_type");
    for case in &file.cases {
        let t = u32::try_from(u64_of(&case.input["weenie_type"]).unwrap()).unwrap();
        assert_eq!(
            ac::verify_create_weenie_type(WeenieType(t)),
            case.output.as_bool().unwrap(),
            "{t}"
        );
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn environ_list_msg_matches_ace() {
    let file = vectors::load_named("commands", "admin_environ_list_msg");
    assert_eq!(
        ac::environ_list_msg(),
        file.cases[0].output.as_str().unwrap()
    );
}

#[test]
fn shutdown_broadcast_matches_ace() {
    let file = vectors::load_named("commands", "admin_shutdown");
    for case in &file.cases {
        let interval = u32::try_from(u64_of(&case.input["interval"]).unwrap()).unwrap();
        let parameters: Vec<String> = case.input["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .collect();
        let sdt = TimeSpan::from_seconds(f64::from(interval));
        let generic = asc::shutdown_broadcast_text(sdt, parameters.is_empty(), "System");
        let text = if parameters.is_empty() {
            generic
        } else {
            format!("Broadcast from System> {}\n{generic}", parameters.join(" "))
        };
        let chats = case.output["chats"].as_array().unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(
            text,
            chats[0]["text"].as_str().unwrap(),
            "{interval} {parameters:?}"
        );
        assert_eq!(chats[0]["type"].as_u64(), Some(20), "WorldBroadcast");
        assert_eq!(
            format!(
                "The server will shut down in {}",
                asc::time_remaining_text(sdt)
            ),
            case.output["remaining"].as_str().unwrap(),
            "{interval}"
        );
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn set_shutdown_interval_from_the_console_matches_ace() {
    let mut ts = TestServer::new();
    let file = vectors::load_named("commands", "admin_set_shutdown_interval");
    for case in &file.cases {
        let s = case.input["s"].as_str().unwrap();
        server_manager::set_shutdown_interval(&mut ts.world, 60);
        command_manager::start_console_capture();
        asc::handle_set_shutdown_interval(&mut ts.world, None, &[s.to_owned()]);
        let console = command_manager::take_console_output();
        // ServerManager.SetShutdownInterval's own log line is not the handler's console output
        let expected: Vec<&str> = case.output["console"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .filter(|l| !l.starts_with("Server shutdown interval reset"))
            .collect();
        assert_eq!(console, expected, "{s:?}");
        assert_eq!(
            u64::from(ts.world.server_manager.shutdown_interval),
            u64_of(&case.output["interval"]).unwrap(),
            "{s:?}"
        );
    }
}

#[test]
fn bool_parse_matches_net() {
    let file = vectors::load_named("commands", "admin_bool_parse");
    for case in &file.cases {
        let s = case.input["s"].as_str().unwrap();
        match ac::dotnet_bool_parse(s) {
            Ok(v) => assert_eq!(Some(v), case.output["value"].as_bool(), "{s:?}"),
            Err(_) => {
                assert_eq!(
                    vectors::throws(&case.output),
                    Some("System.FormatException"),
                    "{s:?}"
                )
            }
        }
    }
}

#[test]
fn hud_line_matches_net() {
    let file = vectors::load_named("commands", "admin_hud_line");
    for case in &file.cases {
        let i = &case.input;
        let got = ac::hud_line(
            i["label"].as_str().unwrap(),
            i32::try_from(i64_of(&i["sold"]).unwrap()).unwrap(),
            vectors::f64_of(&i["total"]).unwrap(),
            vectors::f64_of(&i["avail"]).unwrap(),
        );
        assert_eq!(got, case.output.as_str().unwrap(), "{i}");
    }
}

#[test]
fn culture_order_matches_net() {
    let file = vectors::load_named("commands", "admin_culture_order");
    for case in &file.cases {
        let mut names: Vec<String> = case.input["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect();
        names.sort_by(|a, b| ac::culture_compare(a, b));
        let expected: Vec<&str> = case
            .output
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(names, expected);
    }
}

#[test]
fn wildcard_match_matches_net() {
    let file = vectors::load_named("commands", "admin_wildcard_match");
    for case in &file.cases {
        let (text, filter) = (
            case.input["text"].as_str().unwrap(),
            case.input["filter"].as_str().unwrap(),
        );
        assert_eq!(
            ac::wildcard_is_match(text, filter),
            case.output.as_bool().unwrap(),
            "{text:?} ~ {filter:?}"
        );
    }
}

/// The console lines one console command wrote.
fn console(ts: &mut TestServer, handler: CommandHandler, parameters: &[&str]) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    command_manager::take_console_output()
}

#[test]
fn world_open_and_close_from_the_console() {
    let mut ts = TestServer::new();
    ts.world.world_manager.world_status = WorldStatusState::Open;
    let usage = "\nPlease specify state to change\n@world [open | close] <boot>\nIf closing world, using @world close boot will force players to logoff immediately";
    assert_eq!(
        console(&mut ts, asc::handle_help, &[]),
        [format!("World is currently Open{usage}")]
    );
    assert_eq!(
        console(&mut ts, asc::handle_help, &["OPEN"]),
        ["World is already open."]
    );
    assert_eq!(
        console(&mut ts, asc::handle_help, &["close"]),
        ["Closing world..."]
    );
    assert_eq!(
        ts.world.world_manager.world_status,
        WorldStatusState::Closed
    );
    assert_eq!(
        console(&mut ts, asc::handle_help, &["close", "boot"]),
        ["World is already closed."]
    );
    assert_eq!(
        console(&mut ts, asc::handle_help, &["open"]),
        ["Opening world to players..."]
    );
    assert_eq!(ts.world.world_manager.world_status, WorldStatusState::Open);
    assert_eq!(
        console(&mut ts, asc::handle_help, &["close", "BOOT"]),
        ["Closing world, and booting all online players."]
    );
    assert_eq!(
        console(&mut ts, asc::handle_help, &["sideways"]),
        [format!("World is currently Closed{usage}")]
    );
}

#[test]
fn shutdown_from_the_console_starts_the_shutdown_once() {
    let mut ts = TestServer::new();
    server_manager::set_shutdown_interval(&mut ts.world, 300);
    assert!(console(&mut ts, asc::shutdown_server, &["patch", "day"]).is_empty());
    assert!(ts.world.server_manager.shutdown_initiated);
    assert_eq!(
        ts.world.server_manager.shutdown_time,
        ts.world.now.utc.add_seconds(300.0)
    );
    assert_eq!(
        console(&mut ts, asc::shutdown_server, &[]),
        ["Shutdown is already in progress."]
    );
    assert!(console(&mut ts, asc::handle_cancel_shutdown, &[]).is_empty());
    assert!(!ts.world.server_manager.shutdown_initiated);
    // stop-now: interval 0, then the shutdown
    assert!(console(&mut ts, asc::shutdown_server_now, &[]).is_empty());
    assert_eq!(ts.world.server_manager.shutdown_interval, 0);
    assert!(ts.world.server_manager.shutdown_initiated);
}

#[test]
fn stat_reports_from_the_console() {
    let mut ts = TestServer::new();
    let _ = TestServer::take_not_ported();
    assert_eq!(
        console(&mut ts, ast::handle_server_performance, &[]),
        ["Server Performance Monitor not running. To start use /serverperformance start"]
    );
    assert_eq!(
        console(&mut ts, ast::handle_server_performance, &["START"]),
        ["Server Performance Monitor started"]
    );
    assert!(ts.world.performance.is_running);
    assert_eq!(
        console(
            &mut ts,
            ast::handle_server_performance,
            &["stop", "cumulative"]
        ),
        ["Cumulative Server Performance Monitor stopped"]
    );
    assert_eq!(
        console(&mut ts, ast::handle_server_performance, &["stop"]),
        ["Server Performance Monitor stopped"]
    );
    // three parameters: none of the switches, and the monitor is stopped
    assert_eq!(
        console(
            &mut ts,
            ast::handle_server_performance,
            &["start", "cumulative", "x"]
        ),
        ["Server Performance Monitor not running. To start use /serverperformance start"]
    );
    let lb = console(&mut ts, ast::handle_landblock_stats, &[]);
    assert_eq!(
        lb,
        [concat!(
        "Most Busy Landblock - By Average\n",
        "~5m Hits   Avg  Long  Last - ~1h Hits   Avg  Long  Last - Location   Players  Creatures\n",
        "Most Busy Landblock - By Longest\n",
        "~5m Hits   Avg  Long  Last - ~1h Hits   Avg  Long  Last - Location   Players  Creatures\n",
    )]
    );
    let groups = console(&mut ts, ast::handle_lb_group_stats, &[]);
    assert_eq!(groups.len(), 1);
    assert!(groups[0].starts_with("TickPhysicsEfficiencyTracker:   0 %, TickMultiThreadedWorkEfficiencyTracker:   0 %\nLargest Landblock Groups\n"), "{groups:?}");
    let gc = console(&mut ts, ast::handle_gc_status, &[]);
    assert!(gc[0].starts_with("GC.GetTotalMemory: 0 MB"), "{gc:?}");
}

#[test]
fn watchmen_lists_accounts_by_access_level() {
    let mut ts = TestServer::new();
    {
        let mut auth = ts.world.auth.lock();
        for (name, level) in [("u62one", 5u32), ("u62two", 5), ("u62three", 2)] {
            let mut a = auth
                .create_account(
                    name,
                    "pw",
                    empyrean_entity::enums::AccessLevel::Player,
                    std::net::Ipv4Addr::LOCALHOST.into(),
                )
                .expect("a new account");
            a.access_level = level;
            auth.update_account(&a);
        }
    }
    assert_eq!(
        console(&mut ts, ac::handlewatchmen, &["admin"]),
        ["The following accounts have been granted Admin rights:\nu62one\nu62two\n"]
    );
    assert_eq!(
        console(&mut ts, ac::handlewatchmen, &["2"]),
        ["The following accounts have been granted Sentinel rights:\nu62three\n"]
    );
    // no parameter: Advocate; a failed parse: default(AccessLevel), Player; an undefined number: Advocate
    assert_eq!(
        console(&mut ts, ac::handlewatchmen, &[]),
        ["There are no accounts with Advocate rights."]
    );
    assert_eq!(
        console(&mut ts, ac::handlewatchmen, &["nobody"]),
        ["There are no accounts with Player rights."]
    );
    assert_eq!(
        console(&mut ts, ac::handlewatchmen, &["77"]),
        ["There are no accounts with Advocate rights."]
    );
}

#[test]
fn server_properties_from_the_console() {
    let mut ts = TestServer::new();
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_server_bool_property,
            &["world_closed", "maybe"]
        ),
        ["Please input a valid bool"]
    );
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_server_bool_property,
            &["no_such_bool", "true"]
        ),
        ["Unknown bool property was not updated. Type showprops for a list of properties."]
    );
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_server_bool_property,
            &["world_closed", " TRUE "]
        ),
        ["Bool property successfully updated!"]
    );
    let fetched = console(
        &mut ts,
        ac::handle_fetch_server_bool_property,
        &["world_closed"],
    );
    assert_eq!(fetched.len(), 1);
    assert!(
        fetched[0].starts_with("world_closed - ") && fetched[0].ends_with(": True"),
        "{fetched:?}"
    );
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_server_long_property,
            &["max_chars_per_account", "x"]
        ),
        ["Please input a valid long"]
    );
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_server_long_property,
            &["max_chars_per_account", "7"]
        ),
        ["Long property successfully updated!"]
    );
    assert!(console(
        &mut ts,
        ac::handle_fetch_server_long_property,
        &["max_chars_per_account"]
    )[0]
    .ends_with(": 7"));
    assert_eq!(
        console(
            &mut ts,
            ac::handle_modify_property_description,
            &["NUMBER", "x", "y"]
        ),
        ["Please pick from STRING, BOOL, DOUBLE, or LONG"]
    );
}

/// Console exit shuts the server down now.
#[test]
fn console_exit_shuts_the_server_down_now() {
    let mut ts = TestServer::new();
    server_manager::set_shutdown_interval(&mut ts.world, 300);
    let _ = TestServer::take_not_ported();
    let _ = console(
        &mut ts,
        empyrean_command::handlers::console_commands::exit,
        &[],
    );
    assert!(TestServer::take_not_ported().is_empty(), "no pointer hit");
    assert_eq!(ts.world.server_manager.shutdown_interval, 0);
    assert!(ts.world.server_manager.shutdown_initiated);
}

#[test]
fn an_event_that_cannot_start_says_so_on_the_console() {
    // V360/V362 (a fix): ACE wrote the failure to the session only, and threw from the console
    let mut ts = TestServer::new();
    assert_eq!(
        console(&mut ts, ac::handle_event, &["start", "nosuchevent"]),
        ["Unable to start event named nosuchevent ."]
    );
}

#[test]
fn a_dat_export_with_the_wrong_parameter_count_prints_its_usage_and_stops() {
    // V363 (a fix): ACE printed the usage and went on to export into the first parameter
    use empyrean_command::handlers::console_commands as cc;
    let mut ts = TestServer::new();
    for (handler, name) in [
        (cc::export_cell_dat_contents as CommandHandler, "cell"),
        (cc::export_portal_dat_contents, "portal"),
        (cc::export_language_dat_contents, "language"),
    ] {
        let usage = format!("{name}-export <export-directory-without-spaces>");
        assert_eq!(
            console(&mut ts, handler, &["one", "two"]),
            std::slice::from_ref(&usage)
        );
        assert_eq!(console(&mut ts, handler, &[]), [usage]);
    }
}
