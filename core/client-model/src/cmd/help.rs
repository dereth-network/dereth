//! The help command -- `@help`, `@?` and `@help <command>`.
//!
//! **Checked against retail as a whole command.** The obvious reading — the group summaries
//! concatenated with the forward-slash note *appended* — gets most of it the wrong way round:
//!
//! | the obvious reading | what the client does |
//! |---|---|
//! | the note is appended | it is printed **first**, as its own scroll line in both arms, and it is the literal `"\nNote: …(@).\n\n"` with its own blank lines |
//! | the groups are printed bare | they follow `"Available help:\n"` in one string |
//! | `@help <cmd>` prints the long text bare | it is appended to `"For more information, type @help <command>.\n"`, and that one string is the second scroll line |
//! | a command with no help function prints only the note | it prints **`"Unknown command"`** on chat type `0x1A` and *nothing else*; the null-help-function arm and the no-entry arm land on the same literal |
//! | the group walk starts at `commands` | `commands` is **last**; its group summary is the twelfth and final append |
//!
//! The two-line shape is why the return type is `(text, chat_type)` pairs rather than one string:
//! retail really does make two calls, both on chat type **0** at the current command source, and a
//! single joined string cannot express the refusal's `0x1A`.
//!
//! ```text
//!   note        = "\nNote: You may substitute a forward slash (/) for the at symbol (@).\n\n"
//!   if (argc < 1) {
//!       accumulator = "Available help:\n"
//!       + seven inline literals and five group summaries, in order
//!   } else {
//!       accumulator = "For more information, type @help <command>.\n"
//!       name = trim(argv[0], leading = true, trailing = false, "/@")
//!       entry = the command table's entry for name
//!       if (entry == NULL || entry->help == NULL) -> "Unknown command", chat type 0x1A
//!       else accumulator += entry->help(HelpType = 2)
//!   }
//!   add_text_to_scroll(note,        0, true, source)
//!   add_text_to_scroll(accumulator, 0, true, source)
//! ```
//!
//! Sources: the shape above, read in the client itself.

use super::interp::CommandInterp;
use super::table::{HELP_GROUP_ORDER, HELP_TEXTS};

/// `HelpType` — the argument every help function takes, and **there are three of them**.
///
/// The values are the client's own two comparisons: zero is Summary and one is List; anything
/// else falls to the long branch. The help command pushes **2**, so `@help <command>` is always
/// the long form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpType {
    /// The summary kind (**0**) — the one line `@help` prints for the group.
    Summary,
    /// The list kind (**1**) — the one line the `@commands` listing prints. Nothing in this
    /// build reaches it yet; it is here because with a two-valued enum `Summary` would return
    /// the List string as well for the five functions that have both.
    List,
    /// **2**, the value the help command actually passes — the long form `@help <command>` prints.
    Long,
}

/// `add_text_to_scroll(text, 0, true, current_command_source)` -- both of the help command's
/// successful lines pass the `0`.
pub const HELP_CHAT_TYPE: u32 = 0;
/// `add_text_to_scroll("Unknown command", 0x1A, true, source)` -- both refusal arms.
pub const HELP_REFUSAL_CHAT_TYPE: u32 = 0x1A;

/// A narrow 70-byte literal. Printed **first**, on its own, in both successful arms.
pub const HELP_NOTE: &str =
    "\nNote: You may substitute a forward slash (/) for the at symbol (@).\n\n";
/// A narrow literal. The bare `@help` accumulator's first line.
pub const HELP_AVAILABLE: &str = "Available help:\n";
/// A narrow literal. The `@help <command>` accumulator's first line -- the long help text
/// is appended **to this**, which is why the sentence comes before the command's own help.
pub const FOR_MORE_INFORMATION: &str = "For more information, type @help <command>.\n";
/// A narrow literal. The character set `trim(leading, !trailing, …)` strips off the front
/// of the typed name, so `@help /house`, `@help @house` and `@help house` are one command.
pub const HELP_SIGILS: &[char] = &['/', '@'];
/// A **wide** literal. The only thing an unknown command -- or a known one with a NULL
/// help function, which is eight of the 131 registered names — prints.
pub const UNKNOWN_COMMAND: &str = "Unknown command";

/// One help function's strings.
///
/// `Summary` is the **first** literal of the summary branch and not the whole slice: the table
/// was built by splitting each help function at its first `if`, so a function with both a summary
/// and a list arm — the help for `@house`, `@squelch`, `@emote`, `@fillcomps` and `@friends` —
/// leaves two literals on that side, and retail prints exactly one of them.
#[must_use]
pub fn help_text(function: &str, kind: HelpType) -> Option<&'static [&'static str]> {
    HELP_TEXTS
        .iter()
        .find(|(n, _, _)| *n == function)
        .map(|(_, s, l)| match kind {
            HelpType::Summary => &s[..s.len().min(1)],
            HelpType::List if s.len() >= 2 => &s[1..2],
            HelpType::List => *s,
            HelpType::Long => *l,
        })
}

impl CommandInterp {
    /// Runs the help command and returns the `add_text_to_scroll` calls it makes.
    ///
    /// The caller passes them to `add_text_to_scroll(text, ty, true, current_command_source)` in
    /// order. The help command returns **true** on every path, so never adds *"That
    /// is not a valid command."* on top of whatever came back from here.
    #[must_use]
    pub fn do_help(&self, args: &[String]) -> Vec<(String, u32)> {
        if args.is_empty() {
            let mut listing = String::from(HELP_AVAILABLE);
            for group in HELP_GROUP_ORDER {
                let Some(entry) = self.lookup(group) else {
                    continue;
                };
                let Some(func) = entry.help else { continue };
                for line in help_text(func, HelpType::Summary).unwrap_or(&[]) {
                    listing.push_str(line);
                }
            }
            return vec![
                (HELP_NOTE.to_owned(), HELP_CHAT_TYPE),
                (listing, HELP_CHAT_TYPE),
            ];
        }
        let name = args[0].trim_start_matches(HELP_SIGILS);
        let long = self
            .lookup(name)
            .and_then(|entry| entry.help)
            .and_then(|func| help_text(func, HelpType::Long));
        let Some(long) = long else {
            return vec![(UNKNOWN_COMMAND.to_owned(), HELP_REFUSAL_CHAT_TYPE)];
        };
        let mut body = String::from(FOR_MORE_INFORMATION);
        for line in long {
            body.push_str(line);
        }
        vec![
            (HELP_NOTE.to_owned(), HELP_CHAT_TYPE),
            (body, HELP_CHAT_TYPE),
        ]
    }

    /// The whole of what `@help …` puts on the scroll, joined — the shape this crate's own tests
    /// read, and a convenience for anything that only wants the text.
    #[must_use]
    pub fn help(&self, name: Option<&str>) -> String {
        let args: Vec<String> = name.map(|n| vec![n.to_owned()]).unwrap_or_default();
        self.do_help(&args).into_iter().map(|(t, _)| t).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `@afk` help text is the retail client's own strings: a one-line summary and a
    /// four-line long form.
    #[test]
    fn help_afk_matches_the_binarys_strings() {
        let summary = help_text("help_afk", HelpType::Summary).expect("help_afk exists");
        assert_eq!(summary, ["@afk - Set your away-from-keyboard status.\n"]);
        let long = help_text("help_afk", HelpType::Long).expect("help_afk exists");
        assert_eq!(long.len(), 4);
        assert!(long[2].starts_with("@afk off - Turn off AFK mode."));
    }

    /// Summary is one literal even when the function has a list branch.
    #[test]
    fn summary_is_one_literal_even_when_the_function_has_a_list_branch() {
        assert_eq!(
            help_text("help_house", HelpType::Summary).expect("the house help exists"),
            ["@help house - Commands that help you manage your house, including guest and storage management.\n"]
        );
        assert_eq!(
            help_text("help_house", HelpType::List).expect("the house help exists"),
            ["@house - Commands that help you manage your house, including guest and storage management.\n"]
        );
        // A one-branch function answers the same string to both.
        assert_eq!(
            help_text("help_allegiances", HelpType::Summary),
            help_text("help_allegiances", HelpType::List)
        );
    }

    /// Oracle: the note, then the "Available help:" seed, then the twelve appends, then the two
    /// scroll lines -- the note is its own line and it comes **first**.
    #[test]
    fn bare_help_prints_the_note_then_the_groups_in_retails_order() {
        let c = CommandInterp::new();
        let lines = c.do_help(&[]);
        assert_eq!(lines.len(), 2, "two scroll-line calls, not one");
        assert_eq!(lines[0], (HELP_NOTE.to_owned(), 0));
        assert_eq!(lines[1].1, 0);
        let listing = &lines[1].0;
        assert!(listing.starts_with(HELP_AVAILABLE), "{listing}");
        let mut at = 0usize;
        for expected in [
            "@help allegiances - Commands to help you deal with your Allegiance.\n",
            "@help channels - How to communicate with people in your allegiance or fellowship.\n",
            "@help chatting - How to chat publically and privately.\n",
            "@help death - Commands for making, finding, and looting corpses.\n",
            "@help emote - How to perform text and action emotes.\n",
            "@help fillcomps - A command to help you buy components in bulk.\n",
            "@help friends - Commands to help you manage your friends list.\n",
            "@help house - Commands that help you manage your house, including guest and storage management.\n",
            "@help squelch - Commands that let you block out messages from other players.\n",
            "@help status - Commands that display useful information.\n",
            "@help text - Commands that help you manage your text window.\n",
            "@help commands - Lists all commands.\n",
        ] {
            let found = listing[at..]
                .find(expected)
                .unwrap_or_else(|| panic!("missing, or out of order: {expected:?} in {listing}"));
            at += found + expected.len();
        }
        assert_eq!(at, listing.len(), "nothing after the twelfth group");
    }

    /// Oracle: the per-command arm and its two refusal branches. The owner's own check -- `@help house`.
    #[test]
    fn per_command_help_and_the_unknown_command_case() {
        let c = CommandInterp::new();
        let house = c.do_help(&["house".to_owned()]);
        assert_eq!(house.len(), 2);
        assert_eq!(house[0], (HELP_NOTE.to_owned(), 0));
        assert!(
            house[1].0.starts_with(FOR_MORE_INFORMATION),
            "{}",
            house[1].0
        );
        assert!(
            house[1]
                .0
                .contains("@house abandon - Abandons your house.\n"),
            "{}",
            house[1].0
        );
        assert!(
            house[1].0.ends_with("@house available - See @hslist\n"),
            "{}",
            house[1].0
        );
        assert_eq!(house[1].1, 0);
        // The sigil is stripped off the front, either one, and a run of them.
        assert_eq!(c.do_help(&["@house".to_owned()]), house);
        assert_eq!(c.do_help(&["/house".to_owned()]), house);
        // `@index` is registered and has **no help function**: retail's null-pointer arm.
        assert_eq!(
            c.do_help(&["index".to_owned()]),
            vec![(UNKNOWN_COMMAND.to_owned(), 0x1A)]
        );
        // A word that is not registered at all: the same sentence, the same channel.
        assert_eq!(
            c.do_help(&["wibble".to_owned()]),
            vec![(UNKNOWN_COMMAND.to_owned(), 0x1A)]
        );
    }
}
