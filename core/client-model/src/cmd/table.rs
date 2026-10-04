//! The command table.
//!
//! Taken from the retail client's own registration sequences: the first registration pass
//! (**116** entries) and the
//! Turbine chat pass (**15** more, added once the chat client connects).
//!
//! Order is load-bearing. Registration is "add if not already present", so the **first
//! registration of a name wins**.
//! The one exception is `@a`: the Turbine chat pass **removes and deletes** the existing entry
//! before re-adding it, so once the Turbine chat client
//! is up `@a` stops being the pre-Turbine-chat `@a` channel handler and becomes the Turbine-chat
//! allegiance channel.
//!
//! Abbreviations are **separate table entries, not prefix matching**: `@allegi` is not
//! `@allegiance`, it is forwarded to the server.
//!
//! An entry whose `handler` is `None` is a **help group**: it exists only so `@help <group>` prints
//! a summary, and typing it as a command falls through to the channel command and then to the
//! server.

/// One command-table record (24 bytes in the client): the name, the handler and the help text.
///
/// The handler and help keys are this client's own names, never shown to the player. A handler
/// key is the full command word the handler runs, as typed (`allegiance`, `friends_add`,
/// `messagetypes`); a handler reached only through abbreviations is spelled out
/// (`allegiance_broadcast` for `@ab`, `house_recall` for `@hr`, `mansion_recall` for `@hom`), the
/// Turbine-chat allegiance channel is `guild` (its full word, `@guild`), and the one handler behind
/// the fellowship and allegiance channel words (`@f`, `@m`, `@p`, `@v`, …) is `channel_shortcut`.
/// A help key is `help_` plus its handler's key (`help_house`), or plus the group's name for a
/// help group (`help_commands`, `help_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandEntry {
    pub name: &'static str,
    /// `None` is a real value: a help-group entry has no handler.
    pub handler: Option<&'static str>,
    pub help: Option<&'static str>,
}

/// The first registration pass, in registration order.
pub const INITIALIZE_COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        name: "?",
        handler: Some("help"),
        help: None,
    },
    CommandEntry {
        name: "help",
        handler: Some("help"),
        help: None,
    },
    CommandEntry {
        name: "commands",
        handler: None,
        help: Some("help_commands"),
    },
    CommandEntry {
        name: "allegiances",
        handler: None,
        help: Some("help_allegiances"),
    },
    CommandEntry {
        name: "allegiance",
        handler: Some("allegiance"),
        help: Some("help_allegiance"),
    },
    CommandEntry {
        name: "all",
        handler: Some("allegiance"),
        help: Some("help_allegiance"),
    },
    CommandEntry {
        name: "ab",
        handler: Some("allegiance_broadcast"),
        help: Some("help_allegiance"),
    },
    CommandEntry {
        name: "alh",
        handler: Some("allegiance_hometown"),
        help: Some("help_allegiance"),
    },
    CommandEntry {
        name: "ah",
        handler: Some("allegiance_hometown"),
        help: Some("help_allegiance"),
    },
    CommandEntry {
        name: "motd",
        handler: Some("motd"),
        help: Some("help_motd"),
    },
    CommandEntry {
        name: "speaker",
        handler: Some("speaker"),
        help: Some("help_speaker"),
    },
    CommandEntry {
        name: "channels",
        handler: None,
        help: Some("help_channels"),
    },
    CommandEntry {
        name: "a",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "co-vassals",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "covassals",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "covassal",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "c",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "fellowship",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "fellows",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "fellow",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "f",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "group",
        handler: Some("channel_shortcut"),
        help: Some("help_channel_shortcut"),
    },
    CommandEntry {
        name: "g",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "party",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "monarch",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "m",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "patron",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "p",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "vassals",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "vassal",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "v",
        handler: Some("channel_shortcut"),
        help: None,
    },
    CommandEntry {
        name: "join",
        handler: Some("join"),
        help: Some("help_join"),
    },
    CommandEntry {
        name: "leave",
        handler: Some("leave"),
        help: Some("help_leave"),
    },
    CommandEntry {
        name: "chatting",
        handler: None,
        help: Some("help_chatting"),
    },
    CommandEntry {
        name: "chat",
        handler: Some("chat"),
        help: Some("help_chat"),
    },
    CommandEntry {
        name: "notell",
        handler: Some("notell"),
        help: Some("help_notell"),
    },
    CommandEntry {
        name: "reply",
        handler: Some("reply"),
        help: Some("help_reply"),
    },
    CommandEntry {
        name: "r",
        handler: Some("reply"),
        help: Some("help_reply"),
    },
    CommandEntry {
        name: "rp",
        handler: Some("reply"),
        help: Some("help_reply"),
    },
    CommandEntry {
        name: "mr",
        handler: None,
        help: Some("help_reply"),
    },
    CommandEntry {
        name: "pr",
        handler: None,
        help: Some("help_reply"),
    },
    CommandEntry {
        name: "retell",
        handler: Some("retell"),
        help: Some("help_retell"),
    },
    CommandEntry {
        name: "rt",
        handler: Some("retell"),
        help: Some("help_retell"),
    },
    CommandEntry {
        name: "say",
        handler: Some("say"),
        help: Some("help_say"),
    },
    CommandEntry {
        name: "s",
        handler: Some("say"),
        help: Some("help_say"),
    },
    CommandEntry {
        name: "tell",
        handler: Some("tell"),
        help: Some("help_tell"),
    },
    CommandEntry {
        name: "t",
        handler: Some("tell"),
        help: Some("help_tell"),
    },
    CommandEntry {
        name: "send",
        handler: Some("tell"),
        help: Some("help_tell"),
    },
    CommandEntry {
        name: "whisper",
        handler: Some("tell"),
        help: Some("help_tell"),
    },
    CommandEntry {
        name: "w",
        handler: Some("tell"),
        help: Some("help_tell"),
    },
    CommandEntry {
        name: "afk",
        handler: Some("afk"),
        help: Some("help_afk"),
    },
    CommandEntry {
        name: "death",
        handler: None,
        help: Some("help_death"),
    },
    CommandEntry {
        name: "consent",
        handler: Some("consent"),
        help: Some("help_consent"),
    },
    CommandEntry {
        name: "corpse",
        handler: Some("corpse"),
        help: Some("help_corpse"),
    },
    CommandEntry {
        name: "cor",
        handler: Some("corpse"),
        help: Some("help_corpse"),
    },
    CommandEntry {
        name: "die",
        handler: Some("die"),
        help: Some("help_die"),
    },
    CommandEntry {
        name: "lifestone",
        handler: Some("lifestone"),
        help: Some("help_lifestone"),
    },
    CommandEntry {
        name: "lif",
        handler: Some("lifestone"),
        help: Some("help_lifestone"),
    },
    CommandEntry {
        name: "ls",
        handler: Some("lifestone"),
        help: Some("help_lifestone"),
    },
    CommandEntry {
        name: "marketplace",
        handler: Some("marketplace"),
        help: Some("help_marketplace"),
    },
    CommandEntry {
        name: "mar",
        handler: Some("marketplace"),
        help: Some("help_marketplace"),
    },
    CommandEntry {
        name: "mp",
        handler: Some("marketplace"),
        help: Some("help_marketplace"),
    },
    CommandEntry {
        name: "permit",
        handler: Some("permit"),
        help: Some("help_permit"),
    },
    CommandEntry {
        name: "pkarena",
        handler: Some("pkarena"),
        help: Some("help_pkarena"),
    },
    CommandEntry {
        name: "pka",
        handler: Some("pkarena"),
        help: Some("help_pkarena"),
    },
    CommandEntry {
        name: "pklarena",
        handler: Some("pklarena"),
        help: Some("help_pklarena"),
    },
    CommandEntry {
        name: "pla",
        handler: Some("pklarena"),
        help: Some("help_pklarena"),
    },
    CommandEntry {
        name: "e",
        handler: Some("emote"),
        help: Some("help_emote"),
    },
    CommandEntry {
        name: "em",
        handler: Some("emote"),
        help: Some("help_emote"),
    },
    CommandEntry {
        name: "emote",
        handler: Some("emote"),
        help: Some("help_emote"),
    },
    CommandEntry {
        name: "me",
        handler: Some("emote"),
        help: Some("help_emote"),
    },
    CommandEntry {
        name: "emotes",
        handler: Some("emotes"),
        help: Some("help_emotes"),
    },
    CommandEntry {
        name: "fillcomps",
        handler: Some("fillcomps"),
        help: Some("help_fillcomps"),
    },
    CommandEntry {
        name: "loadfile",
        handler: Some("loadfile"),
        help: Some("help_loadfile"),
    },
    CommandEntry {
        name: "friends",
        handler: Some("friends"),
        help: Some("help_friends"),
    },
    CommandEntry {
        name: "friends_add",
        handler: Some("friends_add"),
        help: Some("help_friends"),
    },
    CommandEntry {
        name: "friends_remove",
        handler: Some("friends_remove"),
        help: Some("help_friends"),
    },
    CommandEntry {
        name: "house",
        handler: Some("house"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "hou",
        handler: Some("house"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "hslist",
        handler: Some("hslist"),
        help: Some("help_hslist"),
    },
    CommandEntry {
        name: "hor",
        handler: Some("house_recall"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "hr",
        handler: Some("house_recall"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "hom",
        handler: Some("mansion_recall"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "hoa",
        handler: Some("mansion_recall"),
        help: Some("help_house"),
    },
    CommandEntry {
        name: "squelch",
        handler: Some("squelch"),
        help: Some("help_squelch"),
    },
    CommandEntry {
        name: "unsquelch",
        handler: Some("unsquelch"),
        help: Some("help_squelch"),
    },
    CommandEntry {
        name: "messagetypes",
        handler: Some("messagetypes"),
        help: Some("help_messagetypes"),
    },
    CommandEntry {
        name: "message_types",
        handler: Some("messagetypes"),
        help: Some("help_messagetypes"),
    },
    CommandEntry {
        name: "msgtypes",
        handler: Some("messagetypes"),
        help: Some("help_messagetypes"),
    },
    CommandEntry {
        name: "msg_types",
        handler: Some("messagetypes"),
        help: Some("help_messagetypes"),
    },
    CommandEntry {
        name: "status",
        handler: None,
        help: Some("help_status"),
    },
    CommandEntry {
        name: "age",
        handler: Some("age"),
        help: Some("help_age"),
    },
    CommandEntry {
        name: "birth",
        handler: Some("birth"),
        help: Some("help_birth"),
    },
    CommandEntry {
        name: "day",
        handler: Some("day"),
        help: Some("help_day"),
    },
    CommandEntry {
        name: "endurance",
        handler: Some("endurance"),
        help: Some("help_endurance"),
    },
    CommandEntry {
        name: "framerate",
        handler: Some("framerate"),
        help: Some("help_framerate"),
    },
    CommandEntry {
        name: "loc",
        handler: Some("loc"),
        help: Some("help_loc"),
    },
    CommandEntry {
        name: "pklite",
        handler: Some("pklite"),
        help: Some("help_pklite"),
    },
    CommandEntry {
        name: "pkl",
        handler: Some("pklite"),
        help: Some("help_pklite"),
    },
    CommandEntry {
        name: "render",
        handler: Some("render"),
        help: None,
    },
    CommandEntry {
        name: "version",
        handler: Some("version"),
        help: Some("help_version"),
    },
    CommandEntry {
        name: "saveui",
        handler: Some("saveui"),
        help: Some("help_saveui"),
    },
    CommandEntry {
        name: "loadui",
        handler: Some("loadui"),
        help: Some("help_loadui"),
    },
    CommandEntry {
        name: "saveautoui",
        handler: Some("saveautoui"),
        help: Some("help_saveautoui"),
    },
    CommandEntry {
        name: "loadautoui",
        handler: Some("loadautoui"),
        help: Some("help_loadautoui"),
    },
    CommandEntry {
        name: "lockui",
        handler: Some("lockui"),
        help: Some("help_lockui"),
    },
    CommandEntry {
        name: "text",
        handler: None,
        help: Some("help_text"),
    },
    CommandEntry {
        name: "filter",
        handler: Some("filter"),
        help: Some("help_filter"),
    },
    CommandEntry {
        name: "unfilter",
        handler: Some("unfilter"),
        help: Some("help_unfilter"),
    },
    CommandEntry {
        name: "log",
        handler: Some("log"),
        help: Some("help_log"),
    },
    CommandEntry {
        name: "title",
        handler: Some("title"),
        help: Some("help_title"),
    },
    CommandEntry {
        name: "index",
        handler: Some("index"),
        help: None,
    },
    CommandEntry {
        name: "clist",
        handler: Some("clist"),
        help: None,
    },
    CommandEntry {
        name: "on",
        handler: Some("on"),
        help: None,
    },
    CommandEntry {
        name: "off",
        handler: Some("off"),
        help: None,
    },
];

/// The Turbine chat pass, in registration order.
/// They do **not** go through the AC protocol at all; they send Turbine chat directly.
pub const TURBINE_CHAT_COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        name: "a",
        handler: Some("guild"),
        help: Some("help_guild"),
    },
    CommandEntry {
        name: "guild",
        handler: Some("guild"),
        help: Some("help_guild"),
    },
    CommandEntry {
        name: "gu",
        handler: Some("guild"),
        help: Some("help_guild"),
    },
    CommandEntry {
        name: "general",
        handler: Some("general"),
        help: Some("help_general"),
    },
    CommandEntry {
        name: "cg",
        handler: Some("general"),
        help: Some("help_general"),
    },
    CommandEntry {
        name: "trade",
        handler: Some("trade"),
        help: Some("help_trade"),
    },
    CommandEntry {
        name: "ct",
        handler: Some("trade"),
        help: Some("help_trade"),
    },
    CommandEntry {
        name: "lfg",
        handler: Some("lfg"),
        help: Some("help_lfg"),
    },
    CommandEntry {
        name: "clfg",
        handler: Some("lfg"),
        help: Some("help_lfg"),
    },
    CommandEntry {
        name: "roleplay",
        handler: Some("roleplay"),
        help: Some("help_roleplay"),
    },
    CommandEntry {
        name: "crp",
        handler: Some("roleplay"),
        help: Some("help_roleplay"),
    },
    CommandEntry {
        name: "society",
        handler: Some("society"),
        help: Some("help_society"),
    },
    CommandEntry {
        name: "soc",
        handler: Some("society"),
        help: Some("help_society"),
    },
    CommandEntry {
        name: "olthoi",
        handler: Some("olthoi"),
        help: Some("help_olthoi"),
    },
    CommandEntry {
        name: "o",
        handler: Some("olthoi"),
        help: Some("help_olthoi"),
    },
];

/// The names the Turbine-chat startup removes before re-adding, so the Turbine-chat entry wins.
pub const TURBINE_REPLACES: &[&str] = &["a"];

/// The order the help command actually appends them in, checked against retail.
///
/// The group summary for `commands` is the **twelfth and last** append, after
/// the `text` group's. Seven of the twelve lines are inline literals in the help command itself and
/// five are help-function calls with the summary kind, but every one of the twelve names resolves
/// in this table to a help function whose first summary literal is byte-identical to the line the
/// help command appends, so the walk is data-driven here and the inline seven are not a second
/// copy.
pub const HELP_GROUP_ORDER: &[&str] = &[
    "allegiances",
    "channels",
    "chatting",
    "death",
    "emote",
    "fillcomps",
    "friends",
    "house",
    "squelch",
    "status",
    "text",
    "commands",
];

/// The help command **always** prints this — and **first**, not appended; see
/// `cmd::help::HELP_NOTE`, which carries the literal with the blank lines the client's own literal
/// really has. This constant is the trimmed form of that literal, kept for callers that want the
/// note without its blank lines.
pub const HELP_TRAILER: &str =
    "Note: You may substitute a forward slash (/) for the at symbol (@).";

/// Every help function's string literals, split at its summary-kind branch, as retail has them.
/// The first field is the name the table's `help` entries key on. `@help` prints the summaries;
/// `@help <command>` prints the long form.
pub const HELP_TEXTS: &[(&str, &[&str], &[&str])] = &[
    ("help_log", &["@log - Commands to echo chat text to a logfile.\n"], &["@log <name> - Echoes chat text to a logfile. All the information that appears in your chat window after you type this command will be copied into a text file. Choose the file you are copying to by naming it in the command. If this file already exists, it will add the additional text to the end of it. To turn off logging, simply retype @log.\n@log AClog.txt - Echoes chat text to a log file named Aclog.txt in your Asheron's Call directory. After you use this command, all the information that appears in your chat window will be written to a file in your Asheron's Call directory named Aclog.txt.\n@log - If you are currently copying the text in your chat window to a logfile, this command will stop the process.\n"]),
    ("help_afk", &["@afk - Set your away-from-keyboard status.\n"], &["@afk - Turns on AFK (away-from-keyboard) mode. When set to AFK, other players that send you directed chatyou will receive a customizable message that your are not currently at the keyboard.\n", "@afk on - Turns on AFK mode. When set to AFK, other players that send you directed chatyou will receive a customizable message that your are not currently at the keyboard.\n", "@afk off - Turn off AFK mode.\n", "@afk msg <message> - Set the message that will be sent to players that send you directed chat while you are in AFK mode. Issuing \"@afk msg\" with no message will set your AFK message back to the default. Your custom AFK message is limited to 192 characters.\n"]),
    ("help_squelch_advanced", &["@squelch - Shows the current list of squelched characters.\n@squelch [-account] <name> - Squelches all messages from a character. With the account flag, this command also stops everything except normal chat coming from the target's other characters.\n@squelch [-message_type] <character> - This will filter out all text messages of a certain type  from a specific character.  For example, the following will filter out all tell messages from Oswald:\n     Example: @squelch -tell Oswald.\n@squelch -reply [-account] [-message_type] - This filters out all text messages from whoever last tell'd you.  You may also use the -account flag and/or limit the squelch by indicating specific message types. For example, this will filter out all tell messages from the account of Oswald, assuming that Oswald was the last person who sent you an @tell:\n     Example: @squelch -reply -account -tell\n\n"], &["@squelch - Shows the current list of squelched characters.\n@squelch [-account] <name> - Squelches all messages from a character. With the account flag, this command also stops everything except normal chat coming from the target's other characters.\n@squelch [-message_type] <character> - This will filter out all text messages of a certain type  from a specific character.  For example, the following will filter out all tell messages from Oswald:\n     Example: @squelch -tell Oswald.\n@squelch -reply [-account] [-message_type] - This filters out all text messages from whoever last tell'd you.  You may also use the -account flag and/or limit the squelch by indicating specific message types. For example, this will filter out all tell messages from the account of Oswald, assuming that Oswald was the last person who sent you an @tell:\n     Example: @squelch -reply -account -tell\n\n"]),
    ("help_unsquelch_advanced", &["@unsquelch - Shows the current list of squelched characters.\n@unsquelch <name> - Removes all squelches from a character, including account squelch.\n@unsquelch [-message_type] <character> : This allows text messages of type message_type to come from a squelched character. For example the following allows assessment messages from a character name Oswald:\n     Example: @unsquelch -assessment Oswald\n@unsquelch -reply [-account] [-message_type] : This allows text messages of type message_type from whoever last sent you an @tell. For example, the following will allow any character on Oswald's account to once again send you @tells, assuming that Oswald was the last person who sent you an @tell:\n     Example: @squelch -reply -account -tell\n\n"], &["@unsquelch - Shows the current list of squelched characters.\n@unsquelch <name> - Removes all squelches from a character, including account squelch.\n@unsquelch [-message_type] <character> : This allows text messages of type message_type to come from a squelched character. For example the following allows assessment messages from a character name Oswald:\n     Example: @unsquelch -assessment Oswald\n@unsquelch -reply [-account] [-message_type] : This allows text messages of type message_type from whoever last sent you an @tell. For example, the following will allow any character on Oswald's account to once again send you @tells, assuming that Oswald was the last person who sent you an @tell:\n     Example: @squelch -reply -account -tell\n\n"]),
    ("help_age", &["@age - Displays your total gameplay time.\n"], &["@age - Displays your total gameplay time.\n"]),
    ("help_commands", &["@help commands - Lists all commands.\n"], &["@allegiances", "a", "c", "m", "p", "v", "f", "@chatting", "@death", "@emote - Performs a text emote.\n@emotes - Lists all standard emotes.\n", "@fillcomps - Helps you buy components in bulk.\n", "@saveui <filename> - Saves the current user interface.\n", "@loadui <filename> - Loads a previously saved user interface.\n", "@saveui <filename> - Saves the current user interface.\n", "@loadui <filename> - Loads a previously saved user interface.\n", "@lockui - Toggles the locked state of the UI layout.\n", "@friends - Helps you manage your friends list.\n", "@house - Commands that help you manage your house, including guest and storage management.\n", "@squelch - Squelches a character or account.\n@unsquelch - Unsquelches a squelched character or account.\n@messagetypes - Lists all types of messages that can be squelched or filtered.\n", "@status", "@text"]),
    ("help_allegiance", &["@allegiance - Commands to help manage your allegiance.\n"], &["@allegiance boot [-account] <name> - Removes a character from your allegiance.\n@allegiance ban <add/remove> <name> - Bans all characters on the given character's account from your allegiance (and boots them too!)\n@allegiance ban list - List the characters whose accounts are banned from your allegiance.\n WARNING! Officers banning or booting a character by account could wind up in a situation where they are no longer in the allegiance if they boot a character that is above them in the hierarchy.\n@allegiance info <name> - Requests information on a member of your allegiance.\n@allegiance chat <on/off> - Turn allegiance chat on and off.\n@allegiance chat kick <name>[, <reason>] - Kick a player temporarily from the allegiance chat room.\n@allegiance chat gag <name> - Gags a player so that they cannot see or speak in the allegiance chat room for 5 minutes.\n@allegiance chat ungag <name> - Ungags a gagged allegiance member so that they may once again see and speak in the allegiance chat room.\n@allegiance broadcast <message> - Broadcast a message to the entire allegiance. Limited to 10/day. Also: @ab\n@allegiance officer <add/set> <level #> <name> - Assigns the position of officer, with the given level of permissions, to the named character.\n@allegiance officer <remove> <name> - Removed the named character as an allegiance officer.\n@allegiance officer clear - Clears all officer positions.\n@allegiance officer [list] - list your allegiance officer. Can be used by anyone in an allegiance.\n@allegiance title set <level #> <title> - Sets the title of the given officer level.\n@allegiance title clear - Clears all officer titles.\n@allegiance title [list] - Lists all the officer titles for your allegiance.\n@allegiance name <set/clear> - Displays, sets, or clears the name of your allegiance.\n@allegiance lock <on/off/toggle/check> - Locks, unlocks, or displays the locked state of your allegiance.\n@allegiance lock bypass <clear/name> - Sets, clears, or displays a single character as an approved vassal. That character may then swear into a ...", "@allegiance motd - Displays or sets the message of the day for your allegiance, see @help motd for more information.\n"]),
    ("help_allegiances", &["@help allegiances - Commands to help you deal with your Allegiance.\n"], &["@allegiance - Commands to help manage your allegiance.\n", "@allegiance motd - Displays or sets the message of the day for your allegiance, see @help motd for more information.\n"]),
    ("help_birth", &["@birth - Displays when your character was created.\n"], &["@birth - Displays when your character was created.\n"]),
    ("help_channels", &["@help channels - How to communicate with people in your allegiance or fellowship.\n"], &["a", "c", "m", "p", "v", "f"]),
    ("help_chat", &["@chat - Sets whether or not you receive normal chat.\n"], &["@chat <on/off> - Sets whether or not you receive normal chat. When set to \"off\", you will no longer receive any spoken speech (normal chat).  However, you will still receive tells.\n"]),
    ("help_chatting", &["@help chatting - How to chat publically and privately.\n"], &["@chat - Sets whether or not you receive normal chat.\n", "@notell - Sets whether or not you receive @tell's.\n", "@reply", "@retell - Sends some text to the last person you @tell'd.\n", "@say - Says some text to everyone around you.", "@tell - Sends a private message to another character.\n", "a", "c", "m", "p", "v", "f", "@afk - Set your away-from-keyboard status.\n"]),
    ("help_consent", &["@consent - Commands to help you manage the corpse-looting permissions that others give you.\n"], &["The @consent commands allow you to display and manage your corpse-looting consent list. This list lets you control whether others may permit you to loot their corpse and also allows you to monitor who has given you permission. You may have a maximum of 20 separate permissions at any given time. You will not be able to loot a corpse that was the victim of a player killer, even if its owner has given you permission. Also, players who have squelched you are not able to permit you to loot their corpse. Note that you can toggle your consent on/off via the Character Options panel as well as through these commands.\n@consent on - Turns on your ability to accept permissions from other players.\n@consent off - Turns off your ability to accept permissions from other players.\n@consent who - Lists those who have given you permission to loot their corpses.\n@consent remove <name> - Removes the permission a player granted to you.\n@consent clear - Clears your entire consent list.\n\n"]),
    ("help_corpse", &["@corpse - Displays the location of your last outdoor death.\n"], &["@corpse - Displays the location of your last outdoor death. Even if your corpse has disappeared or if you have subsequently died indoors, typing this command will display your last outdoor corpse location.\n"]),
    ("help_day", &["@day - A toggle that lightens the outdoor landscape. Note that this command may take several seconds to take effect. \n"], &["@day - A toggle that lightens the outdoor landscape. Note that this command may take several seconds to take effect. \n"]),
    ("help_death", &["@help death - Commands for making, finding, and looting corpses.\n"], &["@permit - Commands to give or revoke permission for others to loot your corpse.\n", "@consent - Commands to help you manage the corpse-looting permissions that others give you.\n", "@corpse - Displays the location of your last outdoor death.\n", "@die - Kills your character and leaves a corpse, returning you to your lifestone.\n", "@lifestone", "@marketplace", "@pkarena", "@pklarena"]),
    ("help_die", &["@die - Kills your character and leaves a corpse, returning you to your lifestone.\n"], &["@die - If you wish to kill your character and leave a corpse, you may use the @die command.  This will result in your character's death, you will leave behind a corpse with some of your items, and you will appear at your lifestone.  If you wish to travel to your lifestone without leaving behind a corpse, you may use the @lifestone command.\n"]),
    ("help_emote", &["@help emote - How to perform text and action emotes.\n"], &["The @emote command causes your character to emote some text, by performing an action in the third person. For example, if you typed the following while logged in as a character named Arville:\n    @emote looks around the town curiously.\nthen the chat windows of everyone around you would display:\n    Arville looks around the town curiously.\nYou can use any of these shorter forms of the command as well:\n         @e <text>\n         @em <text>\n         ; <text>\n         : <text>\n\n", "You can also use a variety of standard emotes. These emotes come with special animations as well as text. Type @emotes to see a list.\n\n", "@emote - Performs a text emote.\n@emotes - Lists all standard emotes.\n"]),
    ("help_emotes", &["Standard Emotes:\nNote: These commands should be bound on either side by asterisks. (Example: *wave*)\nShakeFist; Beckon; BeSeeingYou; BlowKiss; BowDeep; ClapHands; Cry; Laugh; Nod; Point; Shrug; Wave; Akimbo; HeartyLaugh; Salute; TapFoot; WaveHigh; WaveLow; Yawn; Stretch; Cringe; Kneel; Plead; Shiver; Shoo; Slouch; Spit; Surrender; Woah; Winded; YMCA; Eat; Drink; Teapot; Pray; Mock; Cheer; Helper; Warm Hands; Scratch Head; Shake Head\n\n"], &["Standard Emotes:\nNote: These commands should be bound on either side by asterisks. (Example: *wave*)\nShakeFist; Beckon; BeSeeingYou; BlowKiss; BowDeep; ClapHands; Cry; Laugh; Nod; Point; Shrug; Wave; Akimbo; HeartyLaugh; Salute; TapFoot; WaveHigh; WaveLow; Yawn; Stretch; Cringe; Kneel; Plead; Shiver; Shoo; Slouch; Spit; Surrender; Woah; Winded; YMCA; Eat; Drink; Teapot; Pray; Mock; Cheer; Helper; Warm Hands; Scratch Head; Shake Head\n\n"]),
    ("help_endurance", &["@endurance - Explains how endurance affects your character.\n"], &["The endurance attribute has a number of abilities tied to it.\nFirst, some combination of strength and endurance (with endurance being more important) now allows one to regenerate hit points at a faster rate the higher one's endurance is.  This bonus is in addition to any regeneration spells one may have placed upon themselves.  This endurance regeneration bonus caps at around 110%.\nSecond, the higher a player's Endurance, the less stamina one uses while attacking.  This benefit is tied to Endurance only, and it caps out at around 50% less stamina used per attack.  The minimum stamina used per attack remains one.\nThird, the higher a player's Endurance, the more likely they are not to use a point of stamina to successfully evade a missile or melee attack.  A player is required to have Melee Defense for melee attacks or Missile Defense for missile attacks trained or specialized in order for this specific ability to work.  This benefit is tied to Endurance only, and it caps out at around a 75% chance to avoid losing a point of stamina per successful evasion.\nFourth, some combination of strength and endurance (the two are roughly of equivalent importance) now allows one to partially resist drain and harm attacks, up to a maximum of roughly 50%.\nFifth, some combination of strength and endurance (the two are roughly of equivalent importance) now allows one to have a level of \"natural resistances\" to the 7 damage types, the same as a certain level of life protections.  This caps out at a 50% resistance (the equivalent to level 5 life prots) to these damage types.  This resistance is not additive to life protections: higher level life protections will overwrite these natural resistances, although life vulns will take these natural resistances into account, if the player does not have a higher level life protection cast upon him.\nThe natural resistances, drain resistances, and regeneration rate info are now visible on the Character Information Panel, in what was once the Burden panel.  This panel now displays the above thr..."]),
    ("help_fillcomps", &["@help fillcomps - A command to help you buy components in bulk.\n", "@fillcomps - Helps you buy components in bulk.\n"], &["The @fillcomps command assists in the bulk purchase of spell components. It is the sole interface for filling the buy list, which is the column of red zeros to the right in your components panel. To designate which components you would like to buy, change the zeros to the number of each component you would like to buy. The types of components you can buy are scarabs, herbs, powders, potions, and talismans.\n\nThis is the proper syntax: @fillcomps <component type> <pyreal value>\n\n@fillcomps - Fills the buy list with all of the components that are desired.\n@fillcomps <component type> - Fills the buy list with all of the components of the given type.\n@fillcomps <pyreal value> - Fills the buy list with all of the components until the total price of the components exceeds the given value.\n@fillcomps <component type> <pyreal value> - Fills the buy list with all of the components of the given type until the total price of components exceeds the given value.\n@fillcomps clear - Sets the requested amount for all components to zero.\n"]),
    ("help_filter", &["@filter - Commands to filter out incoming messages.\n"], &["The @filter commands filter out all incoming messages of a certain type. Type @messagetypes to see a list of the message types that you can filter.\n@filter - List all the filters currently in place.\n@filter <-message_type> - Filters out all incoming messages of a specific type. For example, the following will filter out all spellcasting text: \n     Example: @filter -spellcasting\n@filter -all - Filters out all incoming messages of all types.\n\n"]),
    ("help_framerate", &["@framerate - Toggles the framerate display.\n"], &["@framerate - Toggles the framerate display.\n"]),
    ("help_friends", &["@help friends - Commands to help you manage your friends list.\n", "@friends - Helps you manage your friends list.\n"], &["Every time someone on your friends list logs in or out, you will receive notification. In addition, you can query the online status of your friends list at any time. Your friends list can contain up to 50 characters.\n@friends - Shows all your current friends and indicates if any of them are online.\n@friends online - Shows your current online friends.\n@friends add <name> - Adds a character to your friends list.\n@friends remove <name> - Removes a character from your friends list.\n@friends remove -all - Clears your friends list.\n@friends old - Shows the characters who were on your old-style friends list prior to the January 2006 update, so you can move them to your new-style friends list if necessary.\n"]),
    ("help_house", &["@help house - Commands that help you manage your house, including guest and storage management.\n", "@house - Commands that help you manage your house, including guest and storage management.\n"], &["@house abandon - Abandons your house.\n@house boot <name> - Removes a player from your house.\n@house boot -all - Removes everyone from your house.\n@house guest add <name> - Adds players to your house guest list.\n@house guest remove <name> - Removes players from your house guest list.\n@house guest add_allegiance - Adds your allegiance to the guest list.\n@house guest remove_allegiance - Removes your allegiance from the guest list.\n@house guest remove_all - Removes all guests from your house guest list.\n@house guest list - Shows the current guest list.\n@house recall - Teleports you to your house.\n@house storage add <name> - Gives a player permission to use your house storage.\n@house storage remove <name> - Removes permission to use your house storage from a player.\n@house storage add_allegiance - Grants storage permission to your allegiance.\n@house storage remove_allegiance - Removes storage permission from your allegiance.\n@house storage remove_all - Removes all storage permissions from guests.\n@house open - Creates an open house.\n@house close - Closes your house.\n@house hooks on|off - Makes the hooks in your house visible or invisible.\n@house mansion_recall - Teleports you to your allegiance mansion or villa.\n@house alleg_recall - Teleports you to your allegiance mansion or villa.\n@house available - See @hslist\n"]),
    ("help_hslist", &["@hslist <house type> - Lists the number and, if appropriate, positions of houses currently available for purchase. Types include: Apartment, Cottage, Villa, Mansion\n"], &["@hslist <house type> - Lists the number and, if appropriate, positions of houses currently available for purchase. Types include: Apartment, Cottage, Villa, Mansion\n"]),
    ("help_join", &["@join <channel tag> - Allows you to hear and speak on the given channel.\n"], &["@join <channel tag> - Allows you to hear and speak on the given channel.\n"]),
    ("help_leave", &["@leave <channel tag> - Prevents you from hearing or speaking on the given channel.\n"], &["@leave <channel tag> - Prevents you from hearing or speaking on the given channel.\n"]),
    ("help_lifestone", &["@lifestone - Returns you to the last lifestone you used without killing you.\n"], &["@lifestone - Returns you to the last lifestone you used without killing you.\n"]),
    ("help_loadautoui", &["@help loadautoui - Forces a previously saved layout to load for this user and resolution.\n", "@loadautoui - Forces a previously saved layout to load for this user and resolution.\n"], &["@loadautoui - Forces a previously saved layout to load for this user and resolution."]),
    ("help_loadfile", &["@loadfile - Reads in the given text file and executes each line in the chat entry field.\n"], &["usage: @loadfile filename\nThis command reads in the specified text file and executes every line as if you typed it in the chat entry field.  Essentially, this plays a script.\n"]),
    ("help_loadui", &["@help loadui <filename> - Loads a previously saved user interface layout from disk using the provided file name.\n", "@loadui <filename> - Loads a previously saved user interface.\n"], &["@loadui <filename> - Loads a previously saved user interface layout from disk using the provided file name"]),
    ("help_loc", &["@loc - Displays your current position.\n"], &["@loc - Displays your current position in your chat window. Use this information when you wish to submit a bug report.\n"]),
    ("help_lockui", &["@help lockui - Toggles the locked state of the UI layout.\n"], &["@lockui - Toggles the locked state of the UI layout.\n"]),
    ("help_marketplace", &["@marketplace - Teleports you to the Marketplace of Dereth.\n"], &["@marketplace - Teleports you to the Marketplace of Dereth.\n"]),
    ("help_messagetypes", &[], &[]),
    ("help_motd", &["@allegiance motd - Displays or sets the message of the day for your allegiance, see @help motd for more information.\n"], &["@allegiance motd - Displays the message of the day for your allegiance.\n@allegiance motd set <text> - Sets the MOTD. Can only be used by monarchs.\n@allegiance motd clear- Clears the MOTD. Can only be used by monarchs.\n"]),
    ("help_notell", &["@notell - Sets whether or not you receive @tell's.\n"], &["@notell <on/off> - Sets whether or not you receive @tells. When set to \"on\", you will not receive any tells.\n"]),
    ("help_pkarena", &["@pkarena - Teleports you to the PK Arena. You must be PK to use this command.\n"], &["@pkarena - Teleports you to the PK Arena. You must be PK to use this command.\n"]),
    ("help_pklarena", &["@pklarena - Teleports you to the PKL Arena. You must be PKL to use this command.\n"], &["@pklarena - Teleports you to the PKL Arena. You must be PKL to use this command.\n"]),
    ("help_pklite", &["@pklite - Sets your status to Player Killer Lite. Type @help pklite for more details.\n"], &["@pklite - Sets your status to Player Killer Lite (PK Lite). PK Lite characters can attack other PK Lite characters. They cannot, however, attack Player Killer (PK) characters. PK Lite characters operate under the same combat rules as PK characters, except that if you are killed  in a PK Lite battle, you will not accrue vitae and you will not drop any coins or items. Only Non-Player Killers may use this command to enter PK Lite. Dying in a PK Lite battle and logging off will restore your status to Non-Player Killer.\n"]),
    ("help_permit", &["@permit - Commands to give or revoke permission for others to loot your corpse.\n"], &["The @permit command gives or revokes corpse-looting permissions to other players. You can permit other players to loot any one of your corpses. You may not @permit a player again until he or she has looted your corpse. Permissions expire either after one hour or when the permitted player logs off. If you were killed by a player killer, no one can loot your corpse except you or your killer, even if you give someone else permission.\n@permit add <name> - Allows  another player to loot your corpse.\n@permit remove <name> - Removes permission to access your corpse from the named character.\nType @help consent for more details on corpse looting.\n"]),
    ("help_retell", &["@retell - Sends some text to the last person you @tell'd.\n"], &["@retell <text> - Sends the text to the last person you @tell'd. You may also use @rt.\n"]),
    ("help_reply", &["@reply - Sends some text to the last person who @tell'd you.\n", "@pr - Sends some text to the last person who @p'd you.\n", "@mr - Sends some text to the last person who @m'd you.\n"], &["@reply <text> - Sends the text to the last person who @tell'd you. You may also use @r or @rp.\n", "@pr <text> - Sends the text to the last vassal who used @p to send  you a message.\n", "@mr <text> - Sends the text to the last person who used @m to send  you a message. This only works for monarchs.\n"]),
    ("help_saveautoui", &["@help saveautoui - Saves the current user interface layout to disk for use by this character at a specific resolution. This layout will be loaded for this character when the resolution changes to this specific size.\n", "@saveautoui - Saves the current user interface to be loaded for specific character and resolution.\n"], &["@saveautoui - Stores the current layout to a character and resolution specific file. This layout will automatically be used when the resolution changes for this character to the current size.\n"]),
    ("help_saveui", &["@help saveui <filename> - Saves the current user interface layout to disk. If a file name is provided the layout will be saved using provided file name.\n", "@saveui <filename> - Saves the current user interface.\n"], &["@saveui <filename> - Saves the current user interface layout to disk using the provided file name. If no file name is provided the layout is saved with a name that is unique for your server, character and resolution.\n"]),
    ("help_say", &["@say - Says some text to everyone around you."], &["@say <text> - Says the text to the all the people around you. This is useful when your normal default chat is set to some other channel -- for instance, when you have selected Talk to Fellows as your default.\nAlso: @s\n"]),
    ("help_speaker", &["@speaker - No longer used, see @allegiance officer for a similar command.\n"], &["@speaker - No longer used, see @allegiance officer for a similar command.\n"]),
    ("help_squelch", &["@help squelch - Commands that let you block out messages from other players.\n", "@squelch - Squelches a character or account.\n@unsquelch - Unsquelches a squelched character or account.\n@messagetypes - Lists all types of messages that can be squelched or filtered.\n"], &["The @squelch commands let you block out messages from specific characters or players. The @unsquelch commands lets squelched messages reach you again. Use the options on these commands to squelch all message types or just some types of messages; one character or an entire account. You may have up to 32 players squelched at once. Note that NPCs cannot be permanently squelched.\n\n", "Type @messagetypes for a complete list of message types.\n"]),
    ("help_status", &["@help status - Commands that display useful information.\n"], &["@age", "@birth", "@day", "@endurance - Explains how endurance affects your character.\n", "@framerate", "@loc - Displays your current position.\n", "@pklite - Sets your status to Player Killer Lite. Type @help pklite for more details.\n", "@version"]),
    ("help_channel_shortcut", &[".\n", " - Sends a broadcast to your ", "@"], &[".\n", " - Sends a broadcast to your ", "@"]),
    ("help_tell", &["@tell - Sends a private message to another character.\n"], &["@tell <name>, <text>  - Sends a long-distance, private message to the specified character. Note that you must put a comma after the character's name.\nAlso: @t, @send, @whisper, @w\n"]),
    ("help_text", &["@help text - Commands that help you manage your text window.\n"], &["@filter - Commands to filter out incoming messages.\n", "@unfilter - Commands to remove filters from incoming messages.\n", "@loadfile - Reads in the given text file and executes each line in the chat entry field.\n", "@log - Commands to echo chat text to a logfile.\n", "@title"]),
    ("help_title", &["@title <new title> - Sets the title of the popup chat window.\n"], &["@title <new title> - Sets the title of the popup chat window.\n"]),
    ("help_guild", &["@a - Sends a message to your Allegiance. Also: @guild, @gu\n"], &["@a - Sends a message to your Allegiance. Also: @guild, @gu\n"]),
    ("help_general", &["@general - Sends a message to the global General chat channel. Also: @cg\n"], &["@general - Sends a message to the global General chat channel. Also: @cg\n"]),
    ("help_lfg", &["@lfg - Sends a message to the global Looking For Group (LFG) chat channel. Also: @clfg\n"], &["@lfg - Sends a message to the global Looking For Group (LFG) chat channel. Also: @clfg\n"]),
    ("help_olthoi", &["@olthoi - If you are an Olthoi, sends a message to the global Olthoi chat channel. Also: @o\n"], &["@olthoi - If you are an Olthoi, sends a message to the global Olthoi chat channel. Also: @o\n"]),
    ("help_roleplay", &["@roleplay - Sends a message to the global Roleplay chat channel. Also: @crp\n"], &["@roleplay - Sends a message to the global Roleplay chat channel. Also: @crp\n"]),
    ("help_society", &["@society - Sends a message to the your Society chat channel. Also: @soc\n"], &["@society - Sends a message to the your Society chat channel. Also: @soc\n"]),
    ("help_trade", &["@trade - Sends a message to the global Trade chat channel. Also: @ct\n"], &["@trade - Sends a message to the global Trade chat channel. Also: @ct\n"]),
    ("help_unfilter", &["@unfilter - Commands to remove filters from incoming messages.\n"], &["The @unfilter commands remove specific filters from your incoming messages. For a complete list of message types that you can filter, type @help messagetypes.\n@unfilter <-message_type> - Removes filters on incoming messages of a specific type.  For example, the following allows spellcasting text to resume:\n     Example: @unfilter -spellcasting\n@unfilter -all - Removes all filters on incoming messages of all types.\n"]),
    ("help_version", &["@version - Tells you what version of the software you are using.\n"], &["@version - Tells you what version of the software you are using.\n"]),
];
