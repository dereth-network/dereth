//! ACE: Source/ACE.Server/Command/CommandManager.cs::GetCommands
//! The client's Admin window runs on a table of its own: the table is held to the commands this
//! server answers from chat, so a command added, renamed or moved to the console shows up here.
//! Fixture: the command table alone.

use std::collections::BTreeSet;

use dereth_client_model::cmd::admin::ADMIN_COMMANDS;
use empyrean_command::command_handler_flag::CommandHandlerFlag;
use empyrean_command::command_manager;
use empyrean_entity::enums::AccessLevel;

/// Staff commands the window leaves out: their handlers are empty, or a line cannot reach them,
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
    let client: BTreeSet<String> = ADMIN_COMMANDS
        .iter()
        .map(|c| c.verb.to_ascii_lowercase())
        .collect();
    let missing: Vec<&String> = server.difference(&client).collect();
    let extra: Vec<&String> = client.difference(&server).collect();
    assert!(missing.is_empty(), "the window lacks {missing:?}");
    assert!(extra.is_empty(), "this server does not answer {extra:?}");
}
