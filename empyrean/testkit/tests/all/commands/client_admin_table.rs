//! ACE: Source/ACE.Server/Command/CommandManager.cs::GetCommands
//! The client's Admin plugin runs on a table of its own (`dereth/plugins/admin/commands.txt`): the
//! table is held to the commands this server answers from chat, so a command added, renamed or
//! moved to the console shows up here.
//! Fixture: the command table alone.

use std::collections::BTreeSet;

use empyrean_command::command_handler_flag::CommandHandlerFlag;
use empyrean_command::command_manager;
use empyrean_entity::enums::AccessLevel;

/// Staff commands the plugin leaves out: their handlers are empty, or a line cannot reach them,
/// or (`accountcreate`, `set-accountpassword`) they carry a password its boxes would show.
const NOT_IN_THE_WINDOW: &[&str] = &[
    "accountcreate",
    "adminui",
    "deaf",
    "draw",
    "dumpattackers",
    "freeze",
    "housekeep",
    "lbinterval",
    "lbthresh",
    "limbo",
    "location",
    "magic god",
    "myserver",
    "queryplugin",
    "querypluginlist",
    "radar",
    "raise",
    "rares dump",
    "reloadsysmsg",
    "repeat",
    "serverlist",
    "set-accountpassword",
    "setadvclass",
    "snoop",
    "spendxp",
    "stormnumstormed",
    "stormthresh",
    "trainskill",
    "unfreeze",
    "unlock",
    "userlimit",
];

/// The verbs of the plugin's table: the first column of each row.
fn client_table() -> BTreeSet<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dereth/plugins/admin/commands.txt"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let fields: Vec<&str> = l.split('\t').collect();
            assert_eq!(fields.len(), 5, "a row is five columns: {l:?}");
            fields[0].to_ascii_lowercase()
        })
        .collect()
}

#[test]
fn the_client_s_table_is_what_this_server_answers_from_chat() {
    command_manager::initialize(None);
    let server: BTreeSet<String> = command_manager::get_commands()
        .iter()
        .filter(|c| c.attribute.access >= AccessLevel::Advocate)
        .filter(|c| c.attribute.flags.0 & CommandHandlerFlag::ConsoleInvoke.0 == 0)
        .map(|c| c.attribute.command.to_ascii_lowercase())
        .filter(|c| !NOT_IN_THE_WINDOW.contains(&c.as_str()))
        .collect();
    let client = client_table();
    let missing: Vec<&String> = server.difference(&client).collect();
    let extra: Vec<&String> = client.difference(&server).collect();
    assert!(missing.is_empty(), "the Admin plugin lacks {missing:?}");
    assert!(extra.is_empty(), "this server does not answer {extra:?}");
}
