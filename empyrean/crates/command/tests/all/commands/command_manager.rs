//! ACE: Source/ACE.Server/Command/CommandManager.cs::CommandManager
//! CommandManager's add/remove rules, the interactive-console key, console-thread parse vs world-
//! thread run, usage on wrong arg count, ban/unban/banlist, player resolve, config write as
//! empyrean.toml.
//! Fixture: synthetic command arguments, handler tables and isolated server state.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use empyrean_command::command_handler_flag::CommandHandlerFlag;
use empyrean_command::command_manager::{self, WorldCommand};
use empyrean_entity::enums::AccessLevel;
use empyrean_net::SessionId;
use empyrean_testkit::TestServer;
use empyrean_world::World;

fn noop(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {}

fn other(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {}

#[test]
fn try_add_and_try_remove_follow_aces_rules() {
    command_manager::initialize(None);
    let name = "u61-Added";
    assert!(command_manager::try_add_command_handler(
        empyrean_command::handler!(noop),
        name,
        AccessLevel::Envoy,
        CommandHandlerFlag::None,
        "d",
        "u",
        true
    ));
    // the key compares OrdinalIgnoreCase; without overrides a second add fails
    assert!(!command_manager::try_add_command_handler(
        empyrean_command::handler!(other),
        "U61-ADDED",
        AccessLevel::Admin,
        CommandHandlerFlag::None,
        "",
        "",
        false
    ));
    let found = command_manager::get_command_by_name(name);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].handler_name, empyrean_command::handler!(noop).1);
    assert_eq!(
        found[0].attribute.parameter_count, -1,
        "the TryAddCommand overloads leave ParameterCount at -1"
    );
    assert_eq!(found[0].attribute.access, AccessLevel::Envoy);

    // overriding replaces the handler and keeps the first key's spelling
    assert!(command_manager::try_add_command_handler(
        empyrean_command::handler!(other),
        "U61-ADDED",
        AccessLevel::Admin,
        CommandHandlerFlag::None,
        "",
        "",
        true
    ));
    let found = command_manager::get_command_by_name("U61-ADDED");
    assert_eq!(
        found.len(),
        1,
        "GetCommandByName compares the attribute's Command"
    );
    assert_eq!(found[0].handler_name, empyrean_command::handler!(other).1);
    // Command lookup ignores case.
    for spelling in [name, "u61-added", "U61-Added"] {
        let found = command_manager::get_command_by_name(spelling);
        assert_eq!(found.len(), 1, "{spelling}");
        assert_eq!(found[0].handler_name, empyrean_command::handler!(other).1);
    }
    assert!(command_manager::get_command_by_name("u61-adde").is_empty());

    assert!(!command_manager::try_add_command(None, true));
    assert!(command_manager::try_remove_command("u61-added"));
    assert!(!command_manager::try_remove_command(name));
    assert!(command_manager::get_command_by_name("U61-ADDED").is_empty());
}

/// The console prompt follows `server.interactive_console`; no environment variable is read.
#[test]
fn the_interactive_console_key_turns_the_prompt_off() {
    use empyrean_common::config_manager::ConfigManager;
    use empyrean_common::master_configuration::MasterConfiguration;
    let mut config = MasterConfiguration::default();
    let _on = ConfigManager::override_for_thread(config.clone());
    assert!(!command_manager::non_interactive_console(), "on by default");
    config.server.interactive_console = false;
    let _off = ConfigManager::override_for_thread(config);
    assert!(command_manager::non_interactive_console());
}

#[test]
fn a_console_line_is_parsed_on_the_console_thread_and_run_on_the_world_thread() {
    command_manager::initialize(None);
    let queued: Arc<Mutex<Vec<WorldCommand>>> = Arc::default();
    let q = Arc::clone(&queued);
    let submit = move |c: WorldCommand| {
        q.lock().unwrap().push(c);
        true
    };

    command_manager::start_console_capture();
    // blank lines are skipped; `@pop` is `pop` on the console; the input ends the thread
    command_manager::command_thread(
        Cursor::new("\n   \n@pop\nsudo pop\nversion\nnosuch\n"),
        &submit,
    );
    let banner = command_manager::take_console_output();
    // End of input disables the prompt and terminates the console thread.
    assert_eq!(
        banner,
        [
            "",
            "Empyrean command prompt ready.",
            "",
            "Type \"empcommands\" for help.",
            "",
            "Empyrean command prompt disabled - console input stream was closed"
        ]
    );
    assert_eq!(command_manager::PROMPT, "empyrean>> ");

    // nothing ran yet: the lookups and invokes wait for the world thread
    let commands: Vec<WorldCommand> = std::mem::take(&mut *queued.lock().unwrap());
    assert_eq!(commands.len(), 4);

    let mut ts = TestServer::new();
    // the world database's version row, which `ServerBuildInfo.GetVersionInfo` reports
    ts.world.content = Arc::new(empyrean_content::MemContent::new().version(
        empyrean_content::models::world::version::Version {
            id: 1,
            base_version: Some("v0.9.290".into()),
            patch_version: Some("v0.0.1".into()),
            last_modified: empyrean_common::dotnet::DotNetDateTime::new(2026, 9, 1),
        },
    ));
    command_manager::start_console_capture();
    for c in commands {
        c(&mut ts.world);
    }
    let out = command_manager::take_console_output();
    assert_eq!(out.len(), 4, "{out:?}");
    assert_eq!(out[0], "Current world population: 0");
    assert_eq!(out[1], "SUDO does not work on the console because you already have full access. Remove SUDO from command and execute again.");
    assert!(out[2].starts_with("Server binaries version ") && out[2].contains("Server database version Base: v0.9.290 Patch: v0.0.1 - compiled Tue Sep 1 00:00:00 2026
"), "{:?}", out[2]);
    assert!(
        out[2].contains(&format!(
            ", corrections {}\n",
            empyrean_content::corrections::digest()
        )),
        "{:?}",
        out[2]
    );
    assert_eq!(out[3], "Invalid Command");
}

#[test]
fn a_console_command_with_the_wrong_count_prints_the_usage() {
    command_manager::initialize(None);
    let mut ts = TestServer::new();
    command_manager::start_console_capture();
    let (command, parameters) = command_manager::parse_command("unban").unwrap();
    command_manager::run_console_command(&mut ts.world, "unban", command, parameters);
    let out = command_manager::take_console_output();
    assert_eq!(
        out,
        ["The syntax of the command is incorrect.\nUsage: unban [accountname]\nThis command removes the ban from the specified account. The player will then be able to log into the game."]
    );
}

#[test]
fn console_ban_unban_and_banlist_use_the_auth_database() {
    command_manager::initialize(None);
    let mut ts = TestServer::new();
    ts.auth()
        .create_account(
            "u61acct",
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .unwrap();
    let run = |ts: &mut TestServer, line: &str| {
        command_manager::start_console_capture();
        let (command, parameters) = command_manager::parse_command(line).unwrap();
        command_manager::run_console_command(&mut ts.world, line, command, parameters);
        command_manager::take_console_output()
    };

    assert_eq!(
        run(&mut ts, "unban u61acct"),
        ["Cannot unban\"u61acct\" because that account is not banned."]
    );
    assert_eq!(run(&mut ts, "ban nobody 1 0 0"), ["Cannot ban \"nobody\" because that account cannot be found in database. Check syntax/spelling and try again."]);
    assert_eq!(
        run(&mut ts, "ban u61acct -1 0 0"),
        ["Days must not be less than 0."]
    );
    assert_eq!(
        run(&mut ts, "ban u61acct 0 x 0"),
        ["Hours must not be less than 0."]
    );
    assert_eq!(
        run(&mut ts, "ban u61acct 1 2.5 0 being rude"),
        ["Banned account u61acct for 1 days, 2.5 hours and 0 minutes. Reason: being rude"]
    );

    let account = ts.auth().get_account_by_name("u61acct").unwrap();
    let now = ts.world.now.utc;
    assert_eq!(account.banned_time, Some(now));
    assert_eq!(
        account.ban_expire_time,
        Some(now.add_days(1.0).add_hours(2.5).add_minutes(0.0))
    );
    assert_eq!(
        account.banned_by_account_id,
        Some(0),
        "the console bans as account 0"
    );
    assert_eq!(account.ban_reason.as_deref(), Some("being rude"));

    // (each line is AuthenticationDatabase.GetListofBannedAccounts' text, empyrean-store's port)
    assert_eq!(
        run(&mut ts, "banlist"),
        ["The following accounts are banned:\n-------------------\nu61acct -- banned by CONSOLE until server time Jan 02 2026  2:30AM -- Reason: being rude\n"]
    );
    assert_eq!(run(&mut ts, "unban u61acct"), ["UnBanned account u61acct."]);
    assert_eq!(
        ts.auth()
            .get_account_by_name("u61acct")
            .unwrap()
            .ban_expire_time,
        None
    );
    assert_eq!(
        run(&mut ts, "banlist"),
        ["There are no accounts currently banned."]
    );
}

/// `ResolveACEParameters` with a player online (the vectors run with none). Hand-derived from
/// `CommandParameterHelpers.cs`.
#[test]
fn resolve_finds_online_players_and_cuts_at_either_iid() {
    use empyrean_command::command_parameter_helpers::{
        resolve_ace_parameters, ACECommandParameter, ACECommandParameterType as T, AceParamValue,
    };
    use empyrean_entity::ObjectGuid;
    use empyrean_world::managers::player_manager::OnlinePlayer;

    let mut ts = TestServer::new();
    let guid = ObjectGuid::new(0x5000_0001);
    assert!(ts.world.player_manager.online_players.try_add(
        guid.full(),
        OnlinePlayer {
            guid,
            account: None
        }
    ));
    let params = |s: &str| -> Vec<String> { s.split(' ').map(str::to_owned).collect() };
    let two = || {
        vec![
            ACECommandParameter {
                r#type: T::Long,
                default_value: Some(AceParamValue::Long(-1)),
                ..ACECommandParameter::default()
            },
            ACECommandParameter {
                r#type: T::OnlinePlayerIid,
                required: true,
                ..ACECommandParameter::default()
            },
        ]
    };

    // decimal iid: the blob is cut at group 1 and the number before it is read
    let mut acps = two();
    assert!(resolve_ace_parameters(
        &mut ts.world,
        None,
        &params("7 1342177281"),
        &mut acps,
        false
    ));
    assert_eq!(acps[1].as_player(), Some(guid));
    assert_eq!(acps[0].as_long(), 7);

    // V361 (a fix): a hex iid is cut where it starts too, so the number before it is read
    // (ACE cut at the failed group 1's index, 0, and the number defaulted to -1)
    let mut acps = two();
    assert!(resolve_ace_parameters(
        &mut ts.world,
        None,
        &params("7 0x50000001"),
        &mut acps,
        false
    ));
    assert_eq!(acps[1].as_player(), Some(guid));
    assert!(!acps[0].defaulted);
    assert_eq!(acps[0].as_long(), 7);

    // OnlinePlayerNameOrIid by decimal iid, and an unknown iid's message
    let mut acps = vec![ACECommandParameter {
        r#type: T::OnlinePlayerNameOrIid,
        ..ACECommandParameter::default()
    }];
    assert!(resolve_ace_parameters(
        &mut ts.world,
        None,
        &params("1342177281"),
        &mut acps,
        false
    ));
    assert_eq!(acps[0].as_player(), Some(guid));
    command_manager::start_console_capture();
    let mut acps = vec![ACECommandParameter {
        r#type: T::OnlinePlayerNameOrIid,
        ..ACECommandParameter::default()
    }];
    assert!(!resolve_ace_parameters(
        &mut ts.world,
        None,
        &params("1342177282"),
        &mut acps,
        false
    ));
    assert_eq!(
        command_manager::take_console_output(),
        ["Unable to find player with iid 1342177282"]
    );
}

/// Config write on the console dumps the configuration as empyrean toml.
#[test]
fn config_write_on_the_console_dumps_the_configuration_as_empyrean_toml() {
    use empyrean_common::config_manager::ConfigManager;
    use empyrean_common::toml_config;

    command_manager::initialize(None);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("u68-config-write");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dumped.toml");
    let shown = std::path::absolute(&path).unwrap();
    let line = format!("config-write {}", path.display());

    let run = |ts: &mut TestServer, input: String| {
        let queued: Arc<Mutex<Vec<WorldCommand>>> = Arc::default();
        let q = Arc::clone(&queued);
        let submit = move |c: WorldCommand| {
            q.lock().unwrap().push(c);
            true
        };
        command_manager::start_console_capture();
        command_manager::command_thread(Cursor::new(input), &submit);
        let _banner = command_manager::take_console_output();
        command_manager::start_console_capture();
        for c in std::mem::take(&mut *queued.lock().unwrap()) {
            c(&mut ts.world);
        }
        command_manager::take_console_output()
    };

    let mut ts = TestServer::new();
    let out = run(&mut ts, format!("{line}\n"));
    assert_eq!(
        out,
        [format!(
            "Wrote the configuration in use to {}.",
            shown.display()
        )]
    );
    let written = std::fs::read_to_string(&path).unwrap();
    let parsed = toml_config::from_toml_str(&written).expect("the dump parses");
    assert!(parsed.unknown_keys.is_empty());
    assert_eq!(parsed.config, *ConfigManager::config());

    // An existing file is kept unless -f is given.
    std::fs::write(&path, "kept").unwrap();
    let out = run(&mut ts, format!("{line}\n"));
    assert_eq!(
        out,
        [format!(
            "{} already exists. Use -f to overwrite it: config-write {} -f",
            shown.display(),
            path.display()
        )]
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "kept");
    let out = run(&mut ts, format!("{line} -f\n"));
    assert_eq!(
        out,
        [format!(
            "Wrote the configuration in use to {}.",
            shown.display()
        )]
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), written);

    // Two paths is a usage error.
    let out = run(&mut ts, format!("{line} other.toml\n"));
    assert_eq!(
        out,
        ["config-write [path, default ./empyrean.toml] [-f to overwrite]"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
