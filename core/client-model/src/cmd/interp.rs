//! `on_chat_command`'s sigil handling and `do_command`'s tokenisation: the chat-command entry
//! point, the command dispatcher, the word splitter, the channel command and the argument
//! re-join. An unrecognised command falls through to the server verbatim, **confirmed live**:
//! a command only the server knows produced the server's answer in the chat window.

use super::table::{
    CommandEntry, CommandHandler, DERETH_COMMANDS, INITIALIZE_COMMANDS, TURBINE_CHAT_COMMANDS,
};

/// The talk-focus enumeration, with values 1 through 13.
///
/// **The names of 3…6 are easy to get wrong.** The numbers and the channel bits
/// below are the client's own literals. The chat menu settles the names, because it tags every menu
/// row with enum attribute `0x1000000B` next to the row's own string id:
///
/// ```text
/// ID_Chat_TellToMonarch   attribute 0x1000000b = 5
/// ID_Chat_TellToPatron    attribute 0x1000000b = 4
/// ID_Chat_TellToVassals   attribute 0x1000000b = 6
/// ID_Chat_TellToFellows   attribute 0x1000000b = 3
/// ```
///
/// So 3 is **Fellows**, 4 is **Patron**, 5 is **Monarch**, 6 is **Vassals** — not
/// Allegiance / Fellowship / Patron / Vassals, which would mis-name three of the four and
/// send an allegiance line to the fellowship. `dereth_client_model::chat::TalkFocus` carries the
/// same names; this is the second copy, and the two are cross-checked by
/// [`TalkFocus::menu_label`]'s test.
///
/// The channel bits are unchanged and do **not** follow the menu's naming: focus 3 (Fellows) is
/// `0x800` and focus 4 (Patron) is `0x2000`. That is what retail does.
///
/// Values 1…6 are verified. The Turbine focuses 7…13 are named after their menu rows too, and none
/// of them is spoken by this client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalkFocus {
    /// Public chat -> `Communication_Talk` (0x0015).
    Say = 1,
    /// `Communication_TalkDirect` (0x0032) to the last speakable target.
    Tell = 2,
    /// `ID_Chat_TellToFellows` — channel broadcast (0x0147) on bit `0x800`.
    Fellows = 3,
    /// `ID_Chat_TellToPatron` — channel broadcast on bit `0x2000`.
    Patron = 4,
    /// `ID_Chat_TellToMonarch` — channel broadcast on bit `0x4000`.
    Monarch = 5,
    /// `ID_Chat_TellToVassals` — channel broadcast on bit `0x1000`.
    Vassals = 6,
    TurbineAllegiance = 7,
    TurbineGeneral = 8,
    TurbineTrade = 9,
    TurbineLfg = 10,
    TurbineRoleplay = 11,
    TurbineSociety = 12,
    TurbineOlthoi = 13,
}

impl TalkFocus {
    /// The channel bit `on_chat_command` passes to the channel-broadcast request, where it uses one.
    #[must_use]
    pub const fn channel_bit(self) -> Option<u32> {
        match self {
            Self::Fellows => Some(0x800),
            Self::Vassals => Some(0x1000),
            Self::Patron => Some(0x2000),
            Self::Monarch => Some(0x4000),
            _ => None,
        }
    }

    /// The string id gives this focus's own menu row in the chat menu,
    /// immediately before tagging it with attribute `0x1000000B` = the focus id. This is the
    /// oracle that names 3…6, and it is carried here so the naming can be tested
    /// rather than argued about.
    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::Say => "ID_Chat_TellToAll",
            Self::Tell => "ID_Chat_TellToSelectedNoSelection",
            Self::Fellows => "ID_Chat_TellToFellows",
            Self::Patron => "ID_Chat_TellToPatron",
            Self::Monarch => "ID_Chat_TellToMonarch",
            Self::Vassals => "ID_Chat_TellToVassals",
            Self::TurbineAllegiance => "ID_Chat_TellToAllegiance",
            Self::TurbineGeneral => "ID_Chat_TellToGeneral",
            Self::TurbineTrade => "ID_Chat_TellToTrade",
            Self::TurbineLfg => "ID_Chat_TellToLFG",
            Self::TurbineRoleplay => "ID_Chat_TellToRoleplay",
            Self::TurbineSociety => "ID_Chat_TellToSociety",
            Self::TurbineOlthoi => "ID_Chat_TellToOlthoi",
        }
    }

    #[must_use]
    pub const fn from_raw(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Say,
            2 => Self::Tell,
            3 => Self::Fellows,
            4 => Self::Patron,
            5 => Self::Monarch,
            6 => Self::Vassals,
            7 => Self::TurbineAllegiance,
            8 => Self::TurbineGeneral,
            9 => Self::TurbineTrade,
            10 => Self::TurbineLfg,
            11 => Self::TurbineRoleplay,
            12 => Self::TurbineSociety,
            13 => Self::TurbineOlthoi,
            _ => return None,
        })
    }
}

/// What `on_chat_command` decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The verb resolved to a registered local meaning; this crate does not execute it.
    Handled {
        verb: String,
        handler: CommandHandler,
        args: Vec<String>,
    },
    /// Everything unrecognised. The **whole original line**, `@` and all, goes to the server as
    /// `Talk` (0x0015). There is no access gate and no allow-list, and no local "unknown command"
    /// message. Confirmed live: `@acehelp` — a command that exists only in ACE — produced ACE's
    /// help output.
    ForwardVerbatim(String),
    /// Not a command at all: routed by the window's talk focus.
    Chat {
        destination: TalkFocus,
        text: String,
    },
    /// A registered handler returned `false`: failure event `0x26`.
    Failed(&'static str),
    /// An empty line does nothing at all.
    Empty,
}

/// `WeenieError::ThatIsNotAValidCommand` (0x26). A handler returning `false` is a **different
/// thing** from an unknown command, and produces this.
pub const NOT_A_VALID_COMMAND: &str = "That is not a valid command.";

/// Split into words with the delimiter set `" \t"`.
///
/// Split on space **and** tab, empty tokens dropped, and there is **no quoting, no
/// escaping and no `--` terminator**. A double quote is just another character. Handlers that need
/// free text re-join with the join below.
#[must_use]
pub fn find_all_words(s: &str) -> Vec<String> {
    s.split([' ', '\t'])
        .filter(|w| !w.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Pop one token, or the empty string and empty tail when no arguments remain.
pub(crate) fn next_arg(args: &[String]) -> (&str, &[String]) {
    match args.split_first() {
        Some((first, rest)) => (first.as_str(), rest),
        None => ("", &[]),
    }
}

/// Re-join with a single space, which is why the original spacing is lost
/// for a locally-handled command but preserved for a forwarded one.
#[must_use]
pub fn join_args(args: &[String]) -> String {
    args.join(" ")
}

/// The chat command interpreter — the communication half, **not**
/// movement-command dispatch.
#[derive(Debug)]
pub struct CommandInterp {
    /// The case-insensitive command table and its command records.
    /// Kept as a vector in registration order so "first registration wins" is visible.
    table: Vec<CommandEntry>,
    /// The last command line.
    pub last_line: String,
    /// The chat window the current command came from.
    pub current_command_source: u32,
}

/// Only window ids **1** and **8** are routed by talk focus; every other window sends public chat.
pub const TALK_FOCUS_WINDOWS: [u32; 2] = [1, 8];

impl Default for CommandInterp {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandInterp {
    /// The first command registration pass -- 115 entries, first registration
    /// of a name wins -- followed by this client's own commands, each only where its name is
    /// still free.
    #[must_use]
    pub fn new() -> Self {
        let mut table: Vec<CommandEntry> =
            Vec::with_capacity(INITIALIZE_COMMANDS.len() + DERETH_COMMANDS.len());
        for e in INITIALIZE_COMMANDS.iter().chain(DERETH_COMMANDS) {
            if !table.iter().any(|x| x.name.eq_ignore_ascii_case(e.name)) {
                table.push(*e);
            }
        }
        Self {
            table,
            last_line: String::new(),
            current_command_source: 1,
        }
    }

    /// The Turbine chat pass -- 15 more once
    /// the local chatclient interfaces initialize (not a server handshake). `@a` **replaces**
    /// its existing entry; everything else is added
    /// only if absent.
    pub fn add_turbine_chat_commands(&mut self) {
        for e in TURBINE_CHAT_COMMANDS {
            if super::table::TURBINE_REPLACES
                .iter()
                .any(|n| n.eq_ignore_ascii_case(e.name))
            {
                self.table.retain(|x| !x.name.eq_ignore_ascii_case(e.name));
                self.table.push(*e);
            } else if !self
                .table
                .iter()
                .any(|x| x.name.eq_ignore_ascii_case(e.name))
            {
                self.table.push(*e);
            }
        }
    }

    /// Case-insensitive, **exact** match. No prefix or fuzzy matching; every alias is an explicit
    /// table entry.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&CommandEntry> {
        self.table
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn entries(&self) -> &[CommandEntry] {
        &self.table
    }

    /// The chat-command entry point.
    ///
    /// ```text
    /// trim leading and trailing " \t\r\n"
    /// '/' -> rewrite to '@', fall through
    /// '@' -> do_command
    /// ':' or ';' -> replace with ' ' and prepend "@emote", then do_command
    /// otherwise -> chat, routed by the talk focus
    /// ```
    ///
    /// The Decal plugin hook (its slot is unnamed and remains unverified) runs *before* all of this
    /// and can swallow the line entirely. Plugin support is out of scope, so it is not built.
    pub fn on_chat_command(
        &mut self,
        line: &str,
        source_window: u32,
        talk_focus: TalkFocus,
    ) -> CommandOutcome {
        self.current_command_source = source_window;
        // Trim leading and trailing ASCII whitespace: space, tab, CR, and LF.
        let trimmed = line.trim_matches([' ', '\t', '\r', '\n']);
        self.last_line = trimmed.to_owned();
        let Some(first) = trimmed.chars().next() else {
            return CommandOutcome::Empty;
        };
        match first {
            '/' => {
                // Replace the leading '/' with '@', then fall through to `do_command`.
                self.last_line = format!("@{}", &trimmed['/'.len_utf8()..]);
                self.do_command()
            }
            '@' => self.do_command(),
            ':' | ';' => {
                // The sigil becomes a space and "@emote" is prepended, so `:waves` is `@emote waves`.
                // Applied *after* trimming, so leading whitespace is already gone.
                self.last_line = format!("@emote {}", &trimmed[first.len_utf8()..]);
                self.do_command()
            }
            _ => {
                let destination = if TALK_FOCUS_WINDOWS.contains(&source_window) {
                    talk_focus
                } else {
                    TalkFocus::Say
                };
                CommandOutcome::Chat {
                    destination,
                    text: trimmed.to_owned(),
                }
            }
        }
    }

    /// The command dispatcher.
    ///
    /// ```text
    /// words = split(last_line, " \t")
    /// if (!words.pop_front(&verb)) return false;
    /// verb = verb.substring(1)                  // drop the leading '@'
    /// verb.trim(trailing, ",")                  // "@tell bob," -> "tell"
    /// ```
    ///
    /// The trailing-comma trim is load-bearing for how players type tells.
    fn do_command(&mut self) -> CommandOutcome {
        let words = find_all_words(&self.last_line);
        let Some((first, rest)) = words.split_first() else {
            return CommandOutcome::Empty;
        };
        // substring(1) drops the '@' the sigil handling guaranteed.
        let verb = first
            .get(1..)
            .unwrap_or("")
            .trim_end_matches(',')
            .to_owned();
        let args: Vec<String> = rest.to_vec();
        match self.lookup(&verb) {
            Some(e) => match e.handler {
                Some(handler) => CommandOutcome::Handled {
                    verb,
                    handler,
                    args,
                },
                // A help-group entry falls through to the channel command and then to the server.
                None => CommandOutcome::ForwardVerbatim(self.last_line.clone()),
            },
            None => CommandOutcome::ForwardVerbatim(self.last_line.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered command-interpreter behavior §2 — the sigil table.
    #[test]
    fn the_sigils_normalise_as_documented() {
        let mut c = CommandInterp::new();
        let out = c.on_chat_command("/help", 1, TalkFocus::Say);
        assert!(matches!(out, CommandOutcome::Handled { ref verb, .. } if verb == "help"));
        assert_eq!(c.last_line, "@help");

        c.on_chat_command(":waves", 1, TalkFocus::Say);
        assert_eq!(c.last_line, "@emote waves");
        c.on_chat_command(";waves", 1, TalkFocus::Say);
        assert_eq!(c.last_line, "@emote waves");
        // The trim runs first, so leading whitespace is gone before the sigil is examined.
        c.on_chat_command("   :waves  ", 1, TalkFocus::Say);
        assert_eq!(c.last_line, "@emote waves");

        assert_eq!(
            c.on_chat_command("", 1, TalkFocus::Say),
            CommandOutcome::Empty
        );
        assert_eq!(
            c.on_chat_command("   ", 1, TalkFocus::Say),
            CommandOutcome::Empty
        );
    }

    /// Oracle: command interpretation — the trailing-comma trim on the verb.
    #[test]
    fn the_verb_loses_a_trailing_comma() {
        let mut c = CommandInterp::new();
        match c.on_chat_command("@tell bob, hi there", 1, TalkFocus::Say) {
            CommandOutcome::Handled { verb, args, .. } => {
                assert_eq!(verb, "tell");
                assert_eq!(args, vec!["bob,", "hi", "there"]);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            c.on_chat_command("@tell bob,", 1, TalkFocus::Say),
            CommandOutcome::Handled { ref verb, .. } if verb == "tell"
        ));
    }

    /// Oracle: split on space **and** tab, empty tokens dropped, no quoting.
    #[test]
    fn tokenisation_has_no_quoting_and_drops_empty_tokens() {
        assert_eq!(find_all_words("@tell\tbob\thi"), vec!["@tell", "bob", "hi"]);
        assert_eq!(
            find_all_words("@tell   bob    hi"),
            vec!["@tell", "bob", "hi"]
        );
        // A double quote is just another character.
        assert_eq!(
            find_all_words(r#"@say "two words""#),
            vec!["@say", "\"two", "words\""]
        );
    }

    /// An unknown `@word` is **forwarded verbatim**, never "not a valid command", and there is no
    /// access gate; a live server answered a command only it knows.
    #[test]
    fn an_unknown_command_is_forwarded_verbatim() {
        let mut c = CommandInterp::new();
        assert_eq!(
            c.on_chat_command("@acehelp", 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim("@acehelp".into())
        );
        // The `/` is rewritten to `@` before forwarding -- that is the only rewrite.
        assert_eq!(
            c.on_chat_command("/smite", 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim("@smite".into())
        );
        for group in [
            "commands",
            "allegiances",
            "channels",
            "chatting",
            "mr",
            "pr",
            "death",
            "status",
            "text",
        ] {
            assert_eq!(c.lookup(group).unwrap().handler, None);
            let line = format!("@{group}   retained spacing");
            assert_eq!(
                c.on_chat_command(&line, 1, TalkFocus::Say),
                CommandOutcome::ForwardVerbatim(line)
            );
        }
        assert!(c.lookup("clear").is_none());
        // GM commands, and every access level, go the same way.
        assert_eq!(
            c.on_chat_command("@teleto somewhere", 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim("@teleto somewhere".into())
        );
    }

    /// Oracle: abbreviations are separate entries, not prefix matching.
    #[test]
    fn abbreviations_are_not_prefixes() {
        let mut c = CommandInterp::new();
        assert_eq!(
            c.on_chat_command("@ALL chat on", 1, TalkFocus::Say),
            CommandOutcome::Handled {
                verb: "ALL".into(),
                handler: CommandHandler::Allegiance,
                args: vec!["chat".into(), "on".into()],
            }
        );
        assert!(matches!(
            c.on_chat_command("@allegi", 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim(_)
        ));
    }

    /// Registering the global chat-channel commands *replaces* the `@a` entry; every other one is
    /// added only if absent.
    #[test]
    fn turbine_chat_replaces_only_the_a_entry() {
        let mut c = CommandInterp::new();
        assert_eq!(
            c.lookup("a").unwrap().handler,
            Some(CommandHandler::ChannelShortcut)
        );
        let before = c.entries().len();
        c.add_turbine_chat_commands();
        assert_eq!(c.lookup("a").unwrap().handler, Some(CommandHandler::Guild));
        // 15 registrations, one of which replaces rather than adds.
        assert_eq!(c.entries().len(), before + 14);
        assert_eq!(
            c.lookup("guild").unwrap().handler,
            Some(CommandHandler::Guild)
        );
        // `o` was not previously registered, so it is added.
        assert_eq!(c.lookup("o").unwrap().handler, Some(CommandHandler::Olthoi));
        c.add_turbine_chat_commands();
        assert_eq!(c.entries().len(), before + 14);
        assert_eq!(c.entries().iter().filter(|e| e.name == "a").count(), 1);
        assert_eq!(c.lookup("A").unwrap().handler, Some(CommandHandler::Guild));
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn tod_is_this_clients_own_command_and_shadows_no_retail_one() {
        use crate::cmd::table::DERETH_COMMANDS;
        for own in DERETH_COMMANDS {
            assert!(
                !INITIALIZE_COMMANDS
                    .iter()
                    .chain(TURBINE_CHAT_COMMANDS)
                    .any(|e| e.name.eq_ignore_ascii_case(own.name)),
                "{} is one of retail's names",
                own.name
            );
        }
        let mut c = CommandInterp::new();
        c.add_turbine_chat_commands();
        for line in ["/tod 0.5", "@TOD 0.5"] {
            match c.on_chat_command(line, 1, TalkFocus::Say) {
                CommandOutcome::Handled { handler, args, .. } => {
                    assert_eq!(handler, CommandHandler::Tod, "{line}");
                    assert_eq!(args, vec!["0.5"], "{line}");
                }
                other => panic!("{line}: {other:?}"),
            }
        }
        assert!(matches!(
            c.on_chat_command("@tod", 1, TalkFocus::Say),
            CommandOutcome::Handled {
                handler: CommandHandler::Tod,
                ..
            }
        ));
        // Not a prefix: a longer word is still the server's.
        assert_eq!(
            c.on_chat_command("@today", 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim("@today".into())
        );
    }

    /// Oracle: the recovered command-interpreter behavior §2 — only windows 1 and 8 are routed by talk focus.
    #[test]
    fn only_two_windows_honour_the_talk_focus() {
        let mut c = CommandInterp::new();
        assert_eq!(
            c.on_chat_command("hello", 1, TalkFocus::Patron),
            CommandOutcome::Chat {
                destination: TalkFocus::Patron,
                text: "hello".into()
            }
        );
        assert_eq!(
            c.on_chat_command("hello", 8, TalkFocus::Fellows),
            CommandOutcome::Chat {
                destination: TalkFocus::Fellows,
                text: "hello".into()
            }
        );
        assert_eq!(
            c.on_chat_command("hello", 4, TalkFocus::Fellows),
            CommandOutcome::Chat {
                destination: TalkFocus::Say,
                text: "hello".into()
            }
        );
    }

    /// Oracle: the client's own channel-broadcast literals -- the four
    /// channel bits, keyed by **focus number**, which is the thing that was always right.
    #[test]
    fn the_channel_bits_are_the_documented_ones() {
        let bit = |n: u32| TalkFocus::from_raw(n).and_then(TalkFocus::channel_bit);
        assert_eq!(bit(3), Some(0x800));
        assert_eq!(bit(4), Some(0x2000));
        assert_eq!(bit(5), Some(0x4000));
        assert_eq!(bit(6), Some(0x1000));
        assert_eq!(bit(1), None, "Say is the local talk event, not a channel");
        assert_eq!(bit(2), None, "Tell is the direct talk event");
    }

    /// The focus numbers carry the names init talk focus menu tags them with.
    #[test]
    fn the_focus_numbers_carry_the_names_init_talk_focus_menu_tags_them_with() {
        // Menu order of the thirteen `0x1000000b = N` tags, each with the string id of the row it
        // tags.
        let menu: [(u32, &str); 13] = [
            (5, "ID_Chat_TellToMonarch"),
            (2, "ID_Chat_TellToSelectedNoSelection"),
            (4, "ID_Chat_TellToPatron"),
            (1, "ID_Chat_TellToAll"),
            (6, "ID_Chat_TellToVassals"),
            (3, "ID_Chat_TellToFellows"),
            (7, "ID_Chat_TellToAllegiance"),
            (8, "ID_Chat_TellToGeneral"),
            (9, "ID_Chat_TellToTrade"),
            (10, "ID_Chat_TellToLFG"),
            (11, "ID_Chat_TellToRoleplay"),
            (12, "ID_Chat_TellToSociety"),
            (13, "ID_Chat_TellToOlthoi"),
        ];
        assert_eq!(
            menu.len(),
            13,
            "thirteen rows, and the squelch toggle is not one of them"
        );
        for (n, label) in menu {
            let f = TalkFocus::from_raw(n).expect("1..=13 all resolve");
            assert_eq!(f.menu_label(), label, "focus {n} is the {label} row");
        }
        // The three that were wrong, spelled out so the failure names them.
        assert_eq!(TalkFocus::from_raw(3), Some(TalkFocus::Fellows));
        assert_eq!(TalkFocus::from_raw(4), Some(TalkFocus::Patron));
        assert_eq!(TalkFocus::from_raw(5), Some(TalkFocus::Monarch));
        assert_eq!(TalkFocus::from_raw(6), Some(TalkFocus::Vassals));
        // ..and the number, not the name, is what carries the channel: Fellows is 0x800.
        assert_eq!(TalkFocus::Fellows.channel_bit(), Some(0x800));
        assert_eq!(TalkFocus::Patron.channel_bit(), Some(0x2000));
    }
}
