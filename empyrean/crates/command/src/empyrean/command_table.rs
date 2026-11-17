//! Empyrean's changes to ACE's command rows. Not ACE.
//!
//! The ported handler files keep ACE's table row for row; this module takes rows out of it and
//! rewrites others after they are loaded, so an upstream ACE change still lands only in the ported
//! files (V553).
//!
//! - [`REMOVED`]: rows that do nothing here: .NET runtime commands, retail server-farm and Decal
//!   plugin commands, ACE's empty placeholders and tests that do nothing.
//! - [`EDITS`]: rows kept with a description, usage, flags or parameter count of our own.
//!
//! The usage of `import-json`, `import-sql` and `export-json` is ACE's text with our changes.

use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;

/// ACE's rows left out of the table.
pub const REMOVED: [&str; 39] = [
    // .NET runtime
    "forcegc",
    "forcegc2",
    "gcstatus",
    "generate-classnames",
    "reload-loot-tables",
    "clearphysicscaches",
    // retail's server farm and Decal plugins
    "myserver",
    "serverlist",
    "lbinterval",
    "lbthresh",
    "stormnumstormed",
    "stormthresh",
    "reloadsysmsg",
    "unlock",
    "adminui",
    "querypluginlist",
    "queryplugin",
    // ACE's empty placeholders, and names with a space the lookup can never match
    "draw",
    "freeze",
    "unfreeze",
    "limbo",
    "repeat",
    "snoop",
    "housekeep",
    "location",
    "raise",
    "setadvclass",
    "spendxp",
    "trainskill",
    "userlimit",
    "dumpattackers",
    "radar",
    "deaf",
    "magic god",
    "rares dump",
    // tests that do nothing, and unported dat exports that report success
    "readdat",
    "equiptest",
    "wave-export",
    "image-export",
];

/// A row kept with some fields of our own; `None` keeps ACE's.
#[derive(Debug, Clone, Copy)]
pub struct RowEdit {
    pub command: &'static str,
    pub description: Option<&'static str>,
    pub usage: Option<&'static str>,
    pub flags: Option<CommandHandlerFlag>,
    pub parameter_count: Option<i32>,
}

const fn text(command: &'static str, description: &'static str, usage: &'static str) -> RowEdit {
    RowEdit {
        command,
        description: Some(description),
        usage: Some(usage),
        flags: None,
        parameter_count: None,
    }
}

const fn describe(command: &'static str, description: &'static str) -> RowEdit {
    RowEdit {
        command,
        description: Some(description),
        usage: None,
        flags: None,
        parameter_count: None,
    }
}

const fn usage(command: &'static str, usage: &'static str) -> RowEdit {
    RowEdit {
        command,
        description: None,
        usage: Some(usage),
        flags: None,
        parameter_count: None,
    }
}

const fn flags(command: &'static str, flags: CommandHandlerFlag) -> RowEdit {
    RowEdit {
        command,
        description: None,
        usage: None,
        flags: Some(flags),
        parameter_count: None,
    }
}

const fn count(
    command: &'static str,
    parameter_count: i32,
    usage: Option<&'static str>,
) -> RowEdit {
    RowEdit {
        command,
        description: None,
        usage,
        flags: None,
        parameter_count: Some(parameter_count),
    }
}

const WORLD: CommandHandlerFlag = CommandHandlerFlag::RequiresWorld;
const FIX: &str = "[fix]";
const EXECUTE: &str = "[execute]";

/// ACE's rows with fields of our own.
pub const EDITS: [RowEdit; 83] = [
    // admin
    text(
        "delete",
        "Deletes the selected object. Players cannot be deleted this way.",
        "",
    ),
    text(
        "removespell",
        "Removes the specified spell from your own spellbook.",
        "<spell id>",
    ),
    text(
        "adminhouse",
        "House management for admins. With no subcommand it lists them.",
        "<dump | dump_all | rent | payrent> ...",
    ),
    text(
        "dispel",
        "Removes all enchantments from you and from the items you are wearing.",
        "",
    ),
    describe("fumble", "Makes the selected player drop everything they carry onto the ground."),
    flags("modifyvital", WORLD),
    flags("modifyskill", WORLD),
    flags("modifyattr", WORLD),
    text(
        "heal",
        "Fully restores the health, stamina and mana of you or the selected player.",
        "",
    ),
    text(
        "gamecastlocalemote",
        "Same as @gamecastemote on this server: sends text to all players, formatted exactly as entered.",
        "<message>",
    ),
    usage(
        "qst",
        "[fellow] list [filter] | bestow <quest> | stamp <quest> <count> | erase [quest] | bits <quest>",
    ),
    text(
        "sticky",
        "Sets whether you keep all your items when you die. Any argument but off turns it on.",
        "[on | off]",
    ),
    usage("modifybool", "<property name> <true | false>"),
    usage("fetchbool", "<property name>"),
    usage("modifylong", "<property name> <whole number>"),
    usage("fetchlong", "<property name>"),
    usage("modifydouble", "<property name> <number>"),
    usage("fetchdouble", "<property name>"),
    usage("modifystring", "<property name> <text>"),
    usage("fetchstring", "<property name>"),
    // admin stats
    usage("serverperformance", "[start | stop | reset] [cumulative]"),
    // developer
    count("listcb", 1, Some("<setup id>")),
    count("echo", 1, None),
    usage("effect", "<effect name or number> [scale]"),
    usage("movement", "<motion command number>"),
    text(
        "MoveTo",
        "Spawns a training wand in front of you and moves it to your pack, to test the MoveToObject message.",
        "",
    ),
    text(
        "listplayers",
        "Lists the players connected to the server, optionally only those of one access level.",
        "[access level]",
    ),
    text(
        "setcoin",
        "Sets the coin value your character shows. For debugging only.",
        "<amount>",
    ),
    usage("grantitemxp", "<amount>"),
    usage("getspellformula", "<account name> <spell id>"),
    usage("getallspellformula", "<account name>"),
    usage(
        "getproperty",
        "<PropertyType.Name>, for example PropertyInt.Level",
    ),
    usage(
        "setproperty",
        "<PropertyType.Name> <value | null>, for example PropertyInt.Level 100",
    ),
    usage("enable-aetheria", "[flags]"),
    usage("testdeathitems", "[player name]"),
    text(
        "forcelogout",
        "Same as @forcelogoff: forces the named player, or the last appraised one, to log off.",
        "[player name]",
    ),
    usage("forcelogoff", "[player name]"),
    describe(
        "removeitemspell",
        "Removes a spell from the last appraised item's spellbook.",
    ),
    usage("targetloc", "[guid]"),
    describe("damagehistory", "Shows your recent damage history."),
    describe("fast", "Casts Quickness, Sprint and Strength Self VIII on you."),
    describe("slow", "Casts Slowness, Leaden Feet and Weakness Self VIII on you."),
    describe("rip", "Kills you at once, without the confirmation @die asks for."),
    usage("debugspell", "[on | off]"),
    usage("recordcast", "[on | off]"),
    text(
        "pscript",
        "Plays a PlayScript on the last appraised object.",
        "<PlayScript name or number>",
    ),
    usage("testaim", "<aim motion, for example AimHigh15>"),
    usage("monsterspell", "<spell id or name> [target guid]"),
    text(
        "trywield",
        "Tries to wield an item from your pack in the given equipment slot.",
        "<item guid> <EquipMask>",
    ),
    // developer database, fixes and loot
    usage("fix-shortcut-bars", EXECUTE),
    usage("fix-spell-bars", EXECUTE),
    usage("fix-gear-plating", EXECUTE),
    text(
        "database-shard-cache-pbrt",
        "Shows or sets how many minutes player biotas stay in the shard database cache.",
        "[minutes]",
    ),
    text(
        "database-shard-cache-npbrt",
        "Shows or sets how many minutes other biotas stay in the shard database cache.",
        "[minutes]",
    ),
    usage("verify-player-data", FIX),
    usage("verify-attributes", FIX),
    usage("verify-vitals", FIX),
    usage("verify-skills", FIX),
    usage("verify-skill-credits", FIX),
    text(
        "verify-heritage-augs",
        "Verifies all players have their heritage augmentation, and grants any missing one with fix.",
        FIX,
    ),
    usage("verify-max-augs", FIX),
    usage("verify-xp", FIX),
    usage("fix-biota-emote-delay", FIX),
    usage("verify-armor-levels", FIX),
    usage("verify-clothing-wield-level", FIX),
    usage("verify-legendary-wield-level", FIX),
    usage("verify-shield-rating", FIX),
    usage("verify-melee-rares", FIX),
    usage("verify-beneficial-enchantments", FIX),
    usage(
        "testlootgen",
        "<number of items> <loot tier> [melee | missile | caster | jewelry | armor | cloak | pet | aetheria | all] [-log]",
    ),
    usage(
        "testlootgencorpse",
        "<DID> <number of corpses> [melee | missile | caster | jewelry | armor | cloak | pet | aetheria | all] [-log]",
    ),
    // developer content
    usage(
        "import-json",
        "[type] <wcid>\n<type> - landblock, quest, recipe, weenie (default if not specified)\n<wcid> - filename prefix to search for. can be 'all' to import all files for this content type",
    ),
    usage(
        "import-sql",
        "[type] <wcid>\n<type> - landblock, encounter, event, quest, recipe, spell, weenie (default if not specified)\n<wcid> - filename prefix to search for. can be 'all' to import all files for this content type",
    ),
    usage(
        "export-json",
        "[type] <id>\n<type> - landblock, quest, recipe, weenie (default if not specified)\n<id> - wcid or content id to export",
    ),
    usage("clearcache", "[landblock | recipe | spell | weenie | wield]"),
    flags("nudge", WORLD),
    flags("rotate", WORLD),
    flags("rotate-x", WORLD),
    flags("rotate-y", WORLD),
    flags("rotate-z", WORLD),
    // player
    usage("house-select", "<house number>"),
    usage("castmeter", "[on | off]"),
    usage("objsend", "[creature]"),
];

/// Takes [`REMOVED`] out of ACE's `rows` and applies [`EDITS`] to them.
pub fn apply(rows: &mut Vec<CommandHandlerInfo>) {
    rows.retain(|r| !REMOVED.contains(&r.attribute.command.as_str()));
    for edit in &EDITS {
        for row in rows
            .iter_mut()
            .filter(|r| r.attribute.command == edit.command)
        {
            let a = &mut row.attribute;
            if let Some(description) = edit.description {
                description.clone_into(&mut a.description);
            }
            if let Some(usage) = edit.usage {
                usage.clone_into(&mut a.usage);
            }
            if let Some(flags) = edit.flags {
                a.flags = flags;
            }
            if let Some(parameter_count) = edit.parameter_count {
                a.parameter_count = parameter_count;
            }
        }
    }
}
