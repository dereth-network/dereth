//! Divergence: V367, V368, V452
//! Empyrean's own command names sit after ACE's rows and answer on the console; reportbug keeps
//! ACE's slot with Empyrean's handler and points at the issue tracker; save-world saves what a
//! clean shutdown saves, with the world left running.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_command::command_manager;
use empyrean_command::empyrean;
use empyrean_entity::enums::AccessLevel;

/// ACE commands whose row Empyrean's replaces (same name and slot, our handler).
pub const REPLACED: [&str; 1] = ["reportbug"];

/// Commands only Empyrean has.
pub const EMPYREAN_NAMES: [&str; 13] = [
    "emphelp",
    "empcommands",
    "empversion",
    "source",
    "dn",
    "derethnetwork",
    "dng",
    "reload-dat-overlay",
    "playtest-character",
    "save-world",
    "import-character",
    "import-treasure",
    "roll-treasure",
];

/// ACE's names for Empyrean's commands: ACE's rows, listed as "Same as @<ours> (ACE's name).".
pub const ALIASES: [&str; 3] = ["acehelp", "acecommands", "aceversion"];

/// Whether an ACE comparison leaves the command out (see the module docs).
#[must_use]
pub fn is_brand_row(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    REPLACED.contains(&command.as_str())
        || EMPYREAN_NAMES.contains(&command.as_str())
        || ALIASES.contains(&command.as_str())
}

/// The table without `config-write` (not in ACE).
fn table() -> Vec<empyrean_command::command_handler_info::CommandHandlerInfo> {
    command_manager::get_commands()
        .into_iter()
        .filter(|i| i.attribute.command != "config-write")
        .collect()
}

/// Each of ours is ACE's row under our name, forwarding to ACE's handler, after every ACE row; ACE's
/// name is ACE's row with the alias description.
#[test]
fn empyrean_s_names_are_ace_s_commands() {
    let _table = super::command_manager::initialized_table();
    let file = empyrean_common::vectors::load_named("commands", "command_registry");
    let all = table();
    let last_ace = all
        .iter()
        .rposition(|i| !EMPYREAN_NAMES.contains(&i.attribute.command.as_str()))
        .unwrap();
    assert_eq!(empyrean::help_commands::RENAMED.len(), ALIASES.len());
    for (ours, ace) in empyrean::help_commands::RENAMED {
        let ace_vector = &file
            .cases
            .iter()
            .find(|c| c.output["command"] == ace)
            .unwrap()
            .output;
        let ace_row = all.iter().find(|i| i.attribute.command == ace).unwrap();
        let our_at = all
            .iter()
            .position(|i| i.attribute.command == ours)
            .unwrap();
        let our_row = &all[our_at];
        assert!(our_at > last_ace, "{ours} comes after ACE's rows");
        assert_eq!(
            our_row.handler_name, ace_row.handler_name,
            "{ours} forwards to {ace}'s handler"
        );
        for (row, description) in [
            (
                our_row,
                ace_vector["description"].as_str().unwrap().to_owned(),
            ),
            (ace_row, empyrean::help_commands::alias_description(ours)),
        ] {
            let a = &row.attribute;
            assert_eq!(
                AccessLevel::NAMES[usize::try_from(a.access.0).unwrap()],
                ace_vector["access"].as_str().unwrap(),
                "{ours}"
            );
            assert_eq!(
                i64::from(a.flags.0),
                ace_vector["flags"].as_i64().unwrap(),
                "{ours}"
            );
            assert_eq!(
                i64::from(a.parameter_count),
                ace_vector["parameter_count"].as_i64().unwrap(),
                "{ours}"
            );
            assert_eq!(a.usage, ace_vector["usage"].as_str().unwrap(), "{ours}");
            assert_eq!(a.description, description, "{}", a.command);
        }
    }
    assert_eq!(
        empyrean::help_commands::alias_description("emphelp"),
        "Same as @emphelp (ACE's name)."
    );
}

/// Both names answer from the console; the listing shows ours with ACE's description and ACE's as
/// the same command.
#[test]
fn both_names_answer_on_the_console() {
    use empyrean_command::handlers::help_commands as hc;
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    let mut console = |handler: empyrean_command::command_handler::CommandHandler,
                       parameters: &[&str]| {
        let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
        command_manager::start_console_capture();
        handler(&mut ts.world, None, &parameters);
        command_manager::take_console_output()
    };
    assert_eq!(
        console(hc::handle_ace_help, &["emphelp"]),
        ["emphelp - Displays help.", "Usage: emphelp (command)"]
    );
    assert_eq!(
        console(hc::handle_ace_help, &["acehelp"]),
        [
            "acehelp - Same as @emphelp (ACE's name).",
            "Usage: acehelp (command)"
        ]
    );
    assert_eq!(
        console(hc::handle_ace_help, &["empversion"]),
        ["Unknown command: empversion"],
        "in game only, as ACE's"
    );
    // Command names and `commands` ignore case; both aliases reach the same handler.
    assert_eq!(
        console(hc::handle_ace_help, &["EmpHelp"]),
        ["emphelp - Displays help.", "Usage: emphelp (command)"]
    );
    assert_eq!(
        console(hc::handle_ace_help, &["Acehelp"]),
        [
            "acehelp - Same as @emphelp (ACE's name).",
            "Usage: acehelp (command)"
        ]
    );
    assert_eq!(
        console(hc::handle_ace_help, &["Commands"]),
        console(hc::handle_ace_help, &["commands"])
    );
    assert!(console(hc::handle_ace_help, &["COMMANDS"])[0]
        .starts_with("For more information, type emphelp"));
    let listing = console(hc::handle_ace_commands, &["player"]).join(
        "
",
    );
    for line in [
        "@emphelp - Displays help.",
        "@empcommands - Lists all commands.",
        "@acehelp - Same as @emphelp (ACE's name).",
        "@acecommands - Same as @empcommands (ACE's name).",
    ] {
        assert!(listing.lines().any(|l| l == line), "{line} in {listing}");
    }
    assert!(
        listing.starts_with(
            "For more information, type emphelp < command >.
"
        ),
        "{listing}"
    );
}

#[test]
fn reportbug_is_empyrean_s_in_ace_s_slot() {
    let _table = super::command_manager::initialized_table();
    let file = empyrean_common::vectors::load_named("commands", "command_registry");
    let ace_slot = file
        .cases
        .iter()
        .position(|c| c.output["command"] == "reportbug")
        .unwrap();
    let all: Vec<_> = command_manager::get_commands()
        .into_iter()
        .filter(|i| i.attribute.command != "config-write")
        .collect();
    // ACE's rows up to reportbug are the same rows (the registry test checks each of them), so
    // reportbug's index among them is ACE's slot.
    let ours = all
        .iter()
        .position(|i| i.attribute.command == "reportbug")
        .unwrap();
    let ace_before = file.cases[..ace_slot]
        .iter()
        .filter(|c| ours_has(&all, c.output["command"].as_str().unwrap()))
        .count();
    assert_eq!(ours, ace_before);
    let row = &all[ours];
    assert_eq!(
        row.handler_name,
        "empyrean_command::empyrean::report_bug::handle_reportbug"
    );
    assert_eq!(
        (row.attribute.access, row.attribute.parameter_count),
        (AccessLevel::Player, -1)
    );
}

fn ours_has(
    all: &[empyrean_command::command_handler_info::CommandHandlerInfo],
    command: &str,
) -> bool {
    all.iter().any(|i| i.attribute.command == command)
}

#[test]
fn reportbug_points_at_the_issue_tracker() {
    let msg = empyrean::report_bug::report_bug_message();
    assert!(
        msg.contains("\n-=-\nhttps://github.com/dereth-network/dereth/issues\n-=-\n"),
        "{msg}"
    );
    assert!(!msg.contains('?'), "no query parameters: {msg}");
}

/// Divergence: V451
#[test]
fn the_network_commands_are_rows_of_their_own_after_aces() {
    command_manager::initialize(None);
    let all = table();
    let last_ace = all
        .iter()
        .rposition(|i| !EMPYREAN_NAMES.contains(&i.attribute.command.as_str()))
        .unwrap();
    for (name, access) in [
        ("dn", AccessLevel::Player),
        ("derethnetwork", AccessLevel::Player),
        ("dng", AccessLevel::Player),
    ] {
        let at = all
            .iter()
            .position(|i| i.attribute.command == name)
            .unwrap_or_else(|| panic!("{name} is registered"));
        assert!(at > last_ace, "{name} comes after ACE's rows");
        assert_eq!(all[at].attribute.access, access, "{name}'s access level");
    }
}

/// The overlay is read again only over files that can be opened again, and the command says why
/// not otherwise, on the console; a landblock is taken as `XXYY` or as an id, and anything else
/// after the folder is refused.
#[test]
fn reload_dat_overlay_answers_on_the_console_and_takes_landblocks_as_xxyy() {
    use empyrean_command::empyrean::reload_dat_overlay as r;
    let _table = super::command_manager::initialized_table();
    assert_eq!(r::landblock_of("A9B4"), Some(0xA9B4));
    assert_eq!(r::landblock_of("0xA9B4FFFF"), Some(0xA9B4));
    assert_eq!(r::landblock_of("0xA9B40019"), Some(0xA9B4));
    assert_eq!(r::landblock_of("C:/worlds/overlay"), None);
    assert_eq!(r::landblock_of("A9B"), None);
    let rows = command_manager::get_command_by_name("reload-dat-overlay");
    assert_eq!(rows.len(), 1, "registered once");
    assert_eq!(rows[0].attribute.access, AccessLevel::Developer);
    let mut ts = empyrean_testkit::TestServer::new();
    let mut console = |parameters: &[&str]| {
        let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
        command_manager::start_console_capture();
        r::handle_reload_dat_overlay(&mut ts.world, None, &parameters);
        command_manager::take_console_output()
    };
    let said = console(&["overlay-b", "A9B4"]);
    assert_eq!(said.len(), 1);
    assert!(
        said[0].starts_with("The data overlay was not read again: ")
            && said[0].ends_with("cannot be opened again"),
        "{said:?}"
    );
    assert_eq!(
        console(&["overlay-b", "A9B4", "nowhere"]),
        ["nowhere is not a landblock: give it as XXYY"]
    );
}

#[test]
fn overlay_revision_rejection_names_the_exact_revision_and_keeps_the_world() {
    use empyrean_command::empyrean::reload_dat_overlay::handle_reload_dat_overlay;
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    let original = std::sync::Arc::clone(&ts.world.dats);
    command_manager::start_console_capture();
    handle_reload_dat_overlay(
        &mut ts.world,
        None,
        &[
            "--revision".into(),
            "17".into(),
            "unavailable".into(),
            "A9B4".into(),
        ],
    );
    let output = command_manager::take_console_output();
    assert_eq!(output.len(), 1);
    assert!(output[0].starts_with("Overlay revision 17: rejected;"));
    assert!(std::sync::Arc::ptr_eq(&original, &ts.world.dats));
}

/// `save-world` is an admin's, from the console too: it saves now what a clean shutdown saves (here
/// an offline player with changes), says at once what it is saving, and says `Saved.` only once
/// the database has answered for all of it, at the world's next turn.
#[test]
fn save_world_saves_now_and_says_saved_once_the_database_has_answered() {
    use empyrean_command::empyrean::save_world;
    use empyrean_entity::enums::{PropertyString, WeenieType};
    use empyrean_world::entity::offline_player::OfflinePlayer;
    let _table = super::command_manager::initialized_table();
    let rows = command_manager::get_command_by_name("save-world");
    assert_eq!(rows.len(), 1, "registered once");
    assert_eq!(rows[0].attribute.access, AccessLevel::Admin);
    assert_eq!(rows[0].attribute.parameter_count, 0);
    let mut ts = empyrean_testkit::TestServer::new();
    const SAM: u32 = 0x5000_0042;
    let mut biota = empyrean_entity::Biota {
        id: SAM,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(PropertyString::Name, "Sam".to_owned());
    let mut sam = {
        let mut shard = ts.shard();
        let mut auth = ts.auth();
        OfflinePlayer::new(biota, &mut **shard, &mut **auth)
    };
    sam.changes_detected = true;
    ts.world.player_manager.offline_players.insert(SAM, sam);
    assert!(ts.shard().get_biota(SAM, true).is_none(), "not saved yet");

    command_manager::start_console_capture();
    save_world::handle_save_world(&mut ts.world, None, &[]);
    assert_eq!(
        command_manager::take_console_output(),
        ["Saving 0 players and 0 landblocks..."]
    );
    command_manager::start_console_capture();
    ts.step();
    assert_eq!(command_manager::take_console_output(), ["Saved."]);
    assert_eq!(
        ts.shard().get_biota(SAM, true).map(|b| b.id),
        Some(SAM),
        "the offline player with changes is saved"
    );
    assert_eq!(
        save_world::saving_message(1, 1),
        "Saving 1 player and 1 landblock..."
    );
}
