//! Vectors: empyrean/fixtures/vectors/commands (parsing, handler and parameter cases)
//! ParseCommand, StuffRawIntoParameters, GetCommandHandler, the command table,
//! ResolveACEParameters, TryParsePosition and .NET TryParse (incl. trailing NULs) equal ACE
//! vectors.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_command::command_manager::{
    self, get_command_handler, parse_command, stuff_raw_into_parameters,
};
use empyrean_command::command_parameter_helpers::{
    dotnet_parse, resolve_ace_parameters, try_parse_position, ACECommandParameter,
    ACECommandParameterType as T, AceParamValue, EnumValues,
};
use empyrean_common::vectors::{self, f64_of, i64_of, u64_of};
use empyrean_entity::enums::{AccessLevel, CloakStatus};
use empyrean_entity::Position;
use empyrean_testkit::TestServer;
use serde_json::Value;

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| {
        a.iter()
            .map(|s| s.as_str().expect("a string").to_owned())
            .collect()
    })
}

fn console_text(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

#[test]
fn parse_command_matches_ace() {
    let file = vectors::load_named("commands", "parse_command");
    let mut unclosed = 0;
    for case in &file.cases {
        let line = case.input["line"].as_str().unwrap();
        let got = parse_command(line);
        if let Some(t) = vectors::throws(&case.output) {
            assert_eq!(t, "System.IndexOutOfRangeException", "{line:?}");
            assert!(got.is_err(), "{line:?} should throw");
            continue;
        }
        let (command, parameters) = got.unwrap_or_else(|e| panic!("{line:?}: {e}"));
        assert_eq!(
            command.as_deref(),
            case.output["command"].as_str(),
            "{line:?}: command"
        );
        let expected = strings(&case.output["parameters"]);
        if has_unclosed_quote(line) {
            // V361 (a fix): ACE added the words after a never-closed quote again on their own;
            // here the quote takes them once, so the parameters are ACE's without that tail.
            // (a quote on the last word leaves nothing to repeat)
            let (got, ace) = (parameters.unwrap(), expected.unwrap());
            if got != ace {
                assert!(
                    got.len() < ace.len() && ace.starts_with(&got),
                    "{line:?}: {got:?} vs ACE's {ace:?}"
                );
                unclosed += 1;
            }
            continue;
        }
        assert_eq!(parameters, expected, "{line:?}: parameters");
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
    assert!(unclosed > 0, "the vectors hold a never-closed quote");
    let (_, parameters) = parse_command("x \"a b").unwrap();
    assert_eq!(parameters.unwrap(), ["a b"]);
    let (_, parameters) = parse_command("x one \"a b c").unwrap();
    assert_eq!(parameters.unwrap(), ["one", "a b c"]);
}

/// Whether the line's parameters hold an opening quote that no later word closes.
fn has_unclosed_quote(line: &str) -> bool {
    let words: Vec<&str> = line.split(' ').filter(|s| !s.is_empty()).skip(1).collect();
    let mut i = 0;
    while i < words.len() {
        if words[i].starts_with('"') && !words[i].ends_with('"') {
            match (i + 1..words.len()).find(|&j| words[j].ends_with('"')) {
                Some(j) => i = j,
                None => return true,
            }
        }
        i += 1;
    }
    false
}

#[test]
fn stuff_raw_into_parameters_matches_ace() {
    let file = vectors::load_named("commands", "stuff_raw_into_parameters");
    for case in &file.cases {
        let raw = case.input["raw"].as_str().unwrap();
        let command = case.input["command"].as_str().unwrap();
        let parameters = strings(&case.input["parameters"]).unwrap();
        let got = stuff_raw_into_parameters(raw, command, &parameters);
        assert_eq!(Some(got), strings(&case.output), "{raw:?} / {command:?}");
    }
}

#[test]
fn get_command_handler_on_the_console_matches_ace() {
    command_manager::initialize(None);
    let ts = TestServer::new();
    let file = vectors::load_named("commands", "get_command_handler_console");
    for case in &file.cases {
        let command = case.input["command"].as_str();
        if command.is_some_and(crate::brand::is_brand_row) {
            continue;
        }
        let parameters = strings(&case.input["parameters"]);
        command_manager::start_console_capture();
        let (response, info) = get_command_handler(&ts.world, None, command, parameters.as_deref())
            .expect("no session, no throw");
        let console = console_text(&command_manager::take_console_output());
        assert_eq!(
            format!("{response:?}"),
            case.output["response"].as_str().unwrap(),
            "{command:?} {parameters:?}"
        );
        assert_eq!(
            info.map(|i| i.attribute.command),
            case.output["command"].as_str().map(str::to_owned),
            "{command:?}"
        );
        assert_eq!(
            console,
            case.output["console"].as_str().unwrap(),
            "{command:?} console"
        );
    }
}

fn content_handler_of(method: &str) -> Option<&'static str> {
    let table: [(&str, &str); 19] = [
        (
            "HandleImportJson",
            "developer_content_commands::handle_import_json",
        ),
        (
            "HandleImportSQLFolders",
            "developer_content_commands::handle_import_sql_folders",
        ),
        (
            "HandleImportSQL",
            "developer_content_commands::handle_import_sql",
        ),
        (
            "HandleCreateInst",
            "developer_content_commands::handle_create_inst",
        ),
        (
            "HandleRemoveInst",
            "developer_content_commands::handle_remove_inst",
        ),
        (
            "HandleAddEncounter",
            "developer_content_commands::handle_add_encounter",
        ),
        (
            "HandleRemoveEnc",
            "developer_content_commands::handle_remove_enc",
        ),
        (
            "HandleExportJsonFolder",
            "developer_content_commands::handle_export_json_folder",
        ),
        (
            "HandleExportJson",
            "developer_content_commands::handle_export_json",
        ),
        (
            "HandleExportSqlFolder",
            "developer_content_commands::handle_export_sql_folder",
        ),
        (
            "HandleExportSql",
            "developer_content_commands::handle_export_sql",
        ),
        (
            "HandleClearCache",
            "developer_content_commands::handle_clear_cache",
        ),
        ("HandleNudge", "developer_content_commands::handle_nudge"),
        ("HandleRotate", "developer_content_commands::handle_rotate"),
        (
            "HandleRotateX",
            "developer_content_commands::handle_rotate_x",
        ),
        (
            "HandleRotateY",
            "developer_content_commands::handle_rotate_y",
        ),
        (
            "HandleRotateZ",
            "developer_content_commands::handle_rotate_z",
        ),
        (
            "HandleGenerateClassNames",
            "developer_content_commands::handle_generate_class_names",
        ),
        (
            "HandleVLOCtoLOC",
            "developer_content_commands::handle_vloc_to_loc",
        ),
    ];
    table.iter().find(|(m, _)| *m == method).map(|(_, h)| *h)
}

/// The name of the handler ported from ACE's method `method` of `ty`, below
/// `empyrean_command::handlers` (`module::function`).
fn handler_of(ty: &str, method: &str) -> Option<&'static str> {
    match ty {
        "AdminCommands" | "AdminShardCommands" | "AdminStatCommands" => {
            return crate::admin::handler_of(method)
        }
        "DeveloperCommands" => return crate::developer::handler_of(method),
        "DeveloperContentCommands" => return content_handler_of(method),
        "AccountCommands"
        | "AdvocateCommands"
        | "CharacterCommands"
        | "DeveloperDatabaseCommands"
        | "DeveloperFixCommands"
        | "DeveloperLootCommands"
        | "HelpCommands" => return crate::remaining::handler_of(method),
        _ => {}
    }
    let table: [(&str, &str); 30] = [
        ("ShowVersion", "console_commands::show_version"),
        ("Exit", "console_commands::exit"),
        (
            "ExportCellDatContents",
            "console_commands::export_cell_dat_contents",
        ),
        (
            "ExportPortalDatContents",
            "console_commands::export_portal_dat_contents",
        ),
        (
            "ExportHighresDatContents",
            "console_commands::export_highres_dat_contents",
        ),
        (
            "ExportLanguageDatContents",
            "console_commands::export_language_dat_contents",
        ),
        ("ExportWaveFiles", "console_commands::export_wave_files"),
        ("ExportImageFile", "console_commands::export_image_file"),
        ("HandlePop", "player_commands::handle_pop"),
        ("HandleQuests", "player_commands::handle_quests"),
        ("HandleHouseSelect", "player_commands::handle_house_select"),
        ("HandleDebugCast", "player_commands::handle_debug_cast"),
        ("HandleFixCast", "player_commands::handle_fix_cast"),
        ("HandleCastMeter", "player_commands::handle_cast_meter"),
        ("HandleConfig", "player_commands::handle_config"),
        ("HandleObjSend", "player_commands::handle_obj_send"),
        ("HandleACEversion", "player_commands::handle_ac_eversion"),
        ("HandleReportbug", "player_commands::handle_reportbug"),
        ("HandleCloak", "sentinel_commands::handle_cloak"),
        (
            "HandleNeverSayDie",
            "sentinel_commands::handle_never_say_die",
        ),
        (
            "HandlePortalBypass",
            "sentinel_commands::handle_portal_bypass",
        ),
        ("HandleFellowBuff", "sentinel_commands::handle_fellow_buff"),
        ("HandleBuff", "sentinel_commands::handle_buff"),
        ("HandleRun", "sentinel_commands::handle_run"),
        ("HandleBoot", "sentinel_commands::handle_boot"),
        ("HandleBanAccount", "sentinel_commands::handle_ban_account"),
        (
            "HandleUnBanAccount",
            "sentinel_commands::handle_un_ban_account",
        ),
        ("HandleBanlist", "sentinel_commands::handle_banlist"),
        ("HandleDeaf", "sentinel_commands::handle_deaf"),
        (
            "HandleDeafHearOrMute",
            "sentinel_commands::handle_deaf_hear_or_mute",
        ),
    ];
    table.iter().find(|(m, _)| *m == method).map(|(_, h)| *h)
}

#[test]
fn the_command_table_matches_aces_for_these_handler_files() {
    command_manager::initialize(None);
    let file = vectors::load_named("commands", "command_registry");
    let mine = [
        "AccountCommands",
        "AdminCommands",
        "AdminShardCommands",
        "AdminStatCommands",
        "AdvocateCommands",
        "CharacterCommands",
        "ConsoleCommands",
        "DeveloperCommands",
        "DeveloperContentCommands",
        "DeveloperDatabaseCommands",
        "DeveloperFixCommands",
        "DeveloperLootCommands",
        "HelpCommands",
        "PlayerCommands",
        "SentinelCommands",
    ];
    // (Empyrean's rows are left out on both sides and checked in `brand`)
    let expected: Vec<&Value> = file
        .cases
        .iter()
        .map(|c| &c.output)
        .filter(|o| {
            mine.contains(&o["type"].as_str().unwrap())
                && !crate::brand::is_brand_row(o["command"].as_str().unwrap())
        })
        .collect();
    let all = command_manager::get_commands();
    let not_ace: Vec<_> = all
        .iter()
        .filter(|i| i.attribute.command == "config-write")
        .collect();
    assert_eq!(not_ace.len(), 1, "config-write is registered once");
    let a = &not_ace[0].attribute;
    assert_eq!(
        (a.access, a.flags),
        (
            AccessLevel::Admin,
            empyrean_command::command_handler_flag::CommandHandlerFlag::ConsoleInvoke
        )
    );
    assert_eq!(
        not_ace[0].handler_name,
        "empyrean_command::handlers::console_commands::handle_config_write"
    );
    let got: Vec<_> = all
        .into_iter()
        .filter(|i| {
            !i.attribute.command.starts_with("u61-")
                && !i.attribute.command.starts_with("u62-")
                && i.attribute.command != "config-write"
                && !crate::brand::is_brand_row(&i.attribute.command)
        })
        .collect();
    assert_eq!(got.len(), expected.len(), "entries");
    for (g, e) in got.iter().zip(&expected) {
        let a = &g.attribute;
        let name = e["command"].as_str().unwrap();
        assert_eq!(a.command, name);
        assert_eq!(
            AccessLevel::NAMES[usize::try_from(a.access.0).unwrap()],
            e["access"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(i64::from(a.flags.0), i64_of(&e["flags"]).unwrap(), "{name}");
        assert_eq!(
            i64::from(a.parameter_count),
            i64_of(&e["parameter_count"]).unwrap(),
            "{name}"
        );
        assert_eq!(a.description, e["description"].as_str().unwrap(), "{name}");
        assert_eq!(a.usage, e["usage"].as_str().unwrap(), "{name}");
        assert_eq!(a.include_raw, e["include_raw"].as_bool().unwrap(), "{name}");
        let method = e["method"].as_str().unwrap();
        let expected = handler_of(e["type"].as_str().unwrap(), method)
            .unwrap_or_else(|| panic!("{name}: no handler named {method}"));
        assert_eq!(
            g.handler_name.strip_prefix("empyrean_command::handlers::"),
            Some(expected),
            "{name}: not {method}"
        );
    }
}

fn p(t: T) -> ACECommandParameter {
    ACECommandParameter {
        r#type: t,
        ..ACECommandParameter::default()
    }
}

fn with_default(mut a: ACECommandParameter, v: AceParamValue) -> ACECommandParameter {
    a.default_value = Some(v);
    a
}

fn required(mut a: ACECommandParameter, error: &str) -> ACECommandParameter {
    a.required = true;
    a.error_message = Some(error.to_owned());
    a
}

fn enum_of(mut a: ACECommandParameter, v: EnumValues) -> ACECommandParameter {
    a.possible_values = Some(v);
    a
}

/// The parameter lists of `CommandVectors.ResolveParameters`, by index.
fn config(c: u64) -> Vec<ACECommandParameter> {
    match c {
        0 => vec![
            p(T::OnlinePlayerNameOrIid),
            with_default(p(T::ULong), AceParamValue::ULong(8)),
        ],
        1 => vec![p(T::OnlinePlayerNameOrIid)],
        2 => vec![required(p(T::Location), "Could not parse location")],
        3 => vec![
            required(p(T::PositiveLong), "need a positive"),
            with_default(p(T::Long), AceParamValue::Long(-1)),
            with_default(p(T::ULong), AceParamValue::ULong(2)),
        ],
        4 => vec![required(p(T::PlayerName), "who?"), p(T::CommaPrefixedText)],
        5 => vec![
            with_default(p(T::SimpleWord), AceParamValue::String("none".to_owned())),
            with_default(
                enum_of(p(T::Enum), EnumValues::of::<AccessLevel>()),
                AceParamValue::Enum("Player".to_owned(), 0),
            ),
        ],
        6 => vec![required(p(T::DoubleQuoteEnclosedText), "quote it")],
        7 => vec![p(T::PlayerName), p(T::Uri)],
        8 => vec![required(p(T::OnlinePlayerIid), "iid please")],
        9 => vec![p(T::Long), p(T::OnlinePlayerName)],
        10 => vec![p(T::OnlinePlayerName)],
        11 => vec![p(T::CommaPrefixedText)],
        12 => vec![p(T::Enum)],
        13 => vec![p(T::SimpleWord), p(T::Location)],
        14 => vec![with_default(
            p(T::Invalid),
            AceParamValue::String("d".to_owned()),
        )],
        15 => vec![
            p(T::PlayerName),
            required(p(T::CommaPrefixedText), "why?"),
            p(T::CommaPrefixedText),
        ],
        16 => vec![enum_of(p(T::Enum), EnumValues::of::<CloakStatus>())],
        _ => panic!("unknown config {c}"),
    }
}

fn assert_position(p: &Position, e: &Value, what: &str) {
    assert_eq!(
        u64::from(p.landblock_id().raw()),
        u64_of(&e["cell"]).unwrap(),
        "{what}: cell"
    );
    for (got, key) in [
        (p.position_x, "x"),
        (p.position_y, "y"),
        (p.position_z, "z"),
        (p.rotation_w, "rw"),
        (p.rotation_x, "rx"),
        (p.rotation_y, "ry"),
        (p.rotation_z, "rz"),
    ] {
        assert!(
            vectors::same_f32(got, vectors::f32_of(&e[key]).unwrap()),
            "{what}: {key} {got} vs {}",
            e[key]
        );
    }
}

fn assert_value(got: Option<&AceParamValue>, e: &Value, what: &str) {
    match got {
        None => assert!(e.is_null(), "{what}: got null, expected {e}"),
        Some(AceParamValue::ULong(v)) => assert_eq!(Some(*v), u64_of(&e["ulong"]), "{what}"),
        Some(AceParamValue::Long(v)) => assert_eq!(Some(*v), i64_of(&e["long"]), "{what}"),
        Some(AceParamValue::String(s)) => {
            assert_eq!(Some(s.as_str()), e["string"].as_str(), "{what}")
        }
        Some(AceParamValue::Uri(s)) => assert_eq!(Some(s.as_str()), e["uri"].as_str(), "{what}"),
        Some(AceParamValue::Enum(n, v)) => {
            assert_eq!(Some(n.as_str()), e["enum"].as_str(), "{what}");
            assert_eq!(Some(*v), i64_of(&e["value"]), "{what}");
        }
        Some(AceParamValue::Position(p)) => {
            assert!(
                e.get("position").is_some(),
                "{what}: got a position, expected {e}"
            );
            assert_position(p, &e["position"], what);
        }
        Some(other) => panic!("{what}: got {other:?}, expected {e}"),
    }
}

#[test]
fn resolve_ace_parameters_matches_ace() {
    let mut ts = TestServer::new();
    let file = vectors::load_named("commands", "resolve_ace_parameters");
    for case in &file.cases {
        let c = u64_of(&case.input["config"]).unwrap();
        let line = case.input["line"].as_str().unwrap();
        let raw = case.input["raw"].as_bool().unwrap();
        let what = format!("config {c} raw {raw} {line:?}");
        let (command, parameters) = parse_command(line).unwrap();
        let mut parameters = parameters.unwrap();
        if raw {
            parameters = stuff_raw_into_parameters(line, command.as_deref().unwrap(), &parameters);
        }
        let mut acps = config(c);
        command_manager::start_console_capture();
        let ok = resolve_ace_parameters(&mut ts.world, None, &parameters, &mut acps, raw);
        let console = console_text(&command_manager::take_console_output());
        assert!(vectors::throws(&case.output).is_none(), "{what}: ACE threw");
        assert_eq!(ok, case.output["ok"].as_bool().unwrap(), "{what}: ok");
        assert_eq!(
            console,
            case.output["console"].as_str().unwrap(),
            "{what}: console"
        );
        let expected = case.output["params"].as_array().unwrap();
        assert_eq!(acps.len(), expected.len());
        for (i, (acp, e)) in acps.iter().zip(expected).enumerate() {
            let what = format!("{what} param {i}");
            assert_eq!(
                acp.defaulted,
                e["defaulted"].as_bool().unwrap(),
                "{what}: defaulted"
            );
            assert_eq!(
                i64::from(acp.parameter_no),
                i64_of(&e["parameter_no"]).unwrap(),
                "{what}: parameter_no"
            );
            assert_value(acp.value.as_ref(), &e["value"], &what);
        }
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn try_parse_position_matches_ace() {
    let mut ts = TestServer::new();
    let file = vectors::load_named("commands", "try_parse_position");
    for case in &file.cases {
        let parameters = strings(&case.input["parameters"]).unwrap();
        let start = usize::try_from(u64_of(&case.input["starting_element"]).unwrap()).unwrap();
        let what = format!("{parameters:?} from {start}");
        // The harness skips `AdjustMapCoords` (it reads the landscape); on the synthetic world it
        // changes nothing, except that a position in landblock row or column 0xFF throws
        // (`LandblockManager.GetLandblock` indexes its 255 x 255 array) and ACE's catch answers
        // its "bad coordinates" error.
        let cell = case.output["position"]["cell"].as_u64().unwrap_or(0);
        if case.output["ok"].as_bool().unwrap()
            && ((cell >> 24) & 0xFF == 0xFF || (cell >> 16) & 0xFF == 0xFF)
        {
            let error = try_parse_position(&mut ts.world, &parameters, start).expect_err(&what);
            assert_eq!(
                error, "There was a problem with that location (bad coordinates?).",
                "{what}"
            );
            continue;
        }
        match try_parse_position(&mut ts.world, &parameters, start) {
            Ok(position) => {
                assert!(
                    case.output["ok"].as_bool().unwrap(),
                    "{what}: ACE failed with {}",
                    case.output["error"]
                );
                assert_eq!(case.output["error"].as_str(), Some(""), "{what}");
                assert_position(&position, &case.output["position"], &what);
            }
            Err(error) => {
                assert!(
                    !case.output["ok"].as_bool().unwrap(),
                    "{what}: ACE succeeded, we said {error}"
                );
                assert_eq!(
                    Some(error.as_str()),
                    case.output["error"].as_str(),
                    "{what}"
                );
            }
        }
    }
}

#[test]
fn dotnet_try_parse_matches_net() {
    let file = vectors::load_named("commands", "dotnet_try_parse");
    for case in &file.cases {
        let s = case.input["s"].as_str().unwrap();
        let o = &case.output;
        assert_eq!(
            dotnet_parse::int_try_parse(s).map(i64::from),
            i64_of(&o["int"]),
            "int {s:?}"
        );
        assert_eq!(
            dotnet_parse::uint_try_parse(s).map(u64::from),
            u64_of(&o["uint"]),
            "uint {s:?}"
        );
        assert_eq!(
            dotnet_parse::long_try_parse(s),
            i64_of(&o["long"]),
            "long {s:?}"
        );
        assert_eq!(
            dotnet_parse::ulong_try_parse(s),
            u64_of(&o["ulong"]),
            "ulong {s:?}"
        );
        assert_eq!(
            dotnet_parse::uint_try_parse_hex(s).map(u64::from),
            u64_of(&o["uint_hex"]),
            "hex {s:?}"
        );
        match (
            dotnet_parse::float_try_parse(s),
            vectors::f32_of(&o["float"]),
        ) {
            (None, None) => {}
            (Some(g), Some(e)) => assert!(vectors::same_f32(g, e), "float {s:?}: {g} vs {e}"),
            (g, e) => panic!("float {s:?}: {g:?} vs {e:?}"),
        }
        match (dotnet_parse::double_try_parse(s), f64_of(&o["double"])) {
            (None, None) => {}
            (Some(g), Some(e)) => assert!(vectors::same_f64(g, e), "double {s:?}: {g} vs {e}"),
            (g, e) => panic!("double {s:?}: {g:?} vs {e:?}"),
        }
    }
}

/// Dotnet try parse matches net with trailing nuls.
#[test]
fn dotnet_try_parse_matches_net_with_trailing_nuls() {
    let file = vectors::load_named("commands", "dotnet_try_parse_nul");
    for case in &file.cases {
        let s = case.input["s"].as_str().unwrap();
        let o = &case.output;
        assert_eq!(
            dotnet_parse::int_try_parse(s).map(i64::from),
            i64_of(&o["int"]),
            "int {s:?}"
        );
        assert_eq!(
            dotnet_parse::uint_try_parse(s).map(u64::from),
            u64_of(&o["uint"]),
            "uint {s:?}"
        );
        assert_eq!(
            dotnet_parse::long_try_parse(s),
            i64_of(&o["long"]),
            "long {s:?}"
        );
        assert_eq!(
            dotnet_parse::ulong_try_parse(s),
            u64_of(&o["ulong"]),
            "ulong {s:?}"
        );
        match (
            dotnet_parse::float_try_parse(s),
            vectors::f32_of(&o["float"]),
        ) {
            (None, None) => {}
            (Some(g), Some(e)) => assert!(vectors::same_f32(g, e), "float {s:?}: {g} vs {e}"),
            (g, e) => panic!("float {s:?}: {g:?} vs {e:?}"),
        }
        match (dotnet_parse::double_try_parse(s), f64_of(&o["double"])) {
            (None, None) => {}
            (Some(g), Some(e)) => assert!(vectors::same_f64(g, e), "double {s:?}: {g} vs {e}"),
            (g, e) => panic!("double {s:?}: {g:?} vs {e:?}"),
        }
    }
}
