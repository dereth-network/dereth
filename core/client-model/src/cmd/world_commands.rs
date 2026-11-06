//! The commands a world lacks, from the systems it announces: the client refuses them and help
//! leaves them out. A world without the `@lifestone` recall describes the command it has instead.
//!
//! Every answer here reads the world's systems ([`EraFeatures`]), never the era's name, so a world
//! that turns a system back on gets its commands back.

use dereth_primitives::EraFeatures;

use super::table::{CommandEntry, CommandHandler};

/// Whether the world has what a command handler does.
#[must_use]
pub fn handler_available(features: &EraFeatures, handler: CommandHandler) -> bool {
    use CommandHandler as H;
    match handler {
        H::Marketplace => features.marketplace_recall,
        H::Endurance => features.endurance_benefits,
        H::Pklite => features.pk_lite,
        H::Pkarena => features.pk_arena_recalls,
        H::Pklarena => features.pk_lite && features.pk_arena_recalls,
        H::Die => features.lifestone_recall,
        H::Sepaku => !features.lifestone_recall,
        H::HouseRecall | H::MansionRecall => features.apartments,
        H::House | H::Hslist => features.housing,
        H::Corpse => features.corpse_command,
        H::Permit | H::Consent => features.corpse_permissions,
        H::Log => features.chat_log,
        H::Motd | H::AllegianceBroadcast | H::Speaker => features.allegiance_messages,
        H::Guild => features.allegiance_chat,
        H::General | H::Trade | H::Lfg | H::Roleplay | H::Society | H::Olthoi => {
            features.global_chat_rooms
        }
        _ => true,
    }
}

/// Whether the world has what an `@allegiance` subcommand (its first word) asks for.
#[must_use]
pub fn allegiance_subcommand_available(features: &EraFeatures, sub: &str) -> bool {
    let eq = |lit: &str| sub.eq_ignore_ascii_case(lit);
    if eq("boot") {
        features.allegiance_boot
    } else if eq("motd") || eq("broadcast") || eq("br") {
        features.allegiance_messages
    } else if eq("officer") || eq("ban") {
        features.allegiance_officers_and_bans
    } else if eq("title") || eq("lock") {
        features.allegiance_officers_and_bans && features.allegiance_titles_and_lock
    } else if eq("house") {
        features.allegiance_house_commands
    } else if eq("name") {
        features.allegiance_name
    } else if eq("chat") || eq("ch") {
        features.allegiance_chat
    } else {
        true
    }
}

/// Whether the world has what an `@allegiance chat` subcommand (its first word) asks for: moving
/// a member out of the room came with the officers, and gagging one with their titles.
#[must_use]
pub fn allegiance_chat_subcommand_available(features: &EraFeatures, sub: &str) -> bool {
    let eq = |lit: &str| sub.eq_ignore_ascii_case(lit);
    if eq("kick") {
        features.allegiance_officers_and_bans
    } else if eq("gag") || eq("ungag") {
        features.allegiance_officers_and_bans && features.allegiance_titles_and_lock
    } else {
        true
    }
}

/// Whether the world has what the words after `@allegiance` ask for.
#[must_use]
pub fn allegiance_command_available(features: &EraFeatures, args: &[String]) -> bool {
    let Some(sub) = args.first() else {
        return true;
    };
    allegiance_subcommand_available(features, sub)
        && (!(sub.eq_ignore_ascii_case("chat") || sub.eq_ignore_ascii_case("ch"))
            || args
                .get(1)
                .is_none_or(|chat| allegiance_chat_subcommand_available(features, chat)))
}

/// Whether the world has what an `@house` subcommand (its first word) asks for: the recalls came
/// with the apartments, the rest with the houses.
#[must_use]
pub fn house_subcommand_available(features: &EraFeatures, sub: &str) -> bool {
    let eq = |lit: &str| sub.eq_ignore_ascii_case(lit);
    if eq("recall") || eq("re") || eq("mansion_recall") || eq("alleg_recall") || eq("ma") {
        features.apartments
    } else {
        true
    }
}

/// `@lifestone`'s help on a world where it kills the character, in the words the later `@die`
/// help uses for the same command.
pub const LIFESTONE_DEATH_HELP: &str =
    "@lifestone - Kills your character and leaves a corpse, returning you to your lifestone.\n";

/// What help describes on a world: which commands to leave out, and which words of `@allegiance`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelpScope {
    /// The command names (every alias) the world lacks, lowercase.
    pub hidden: Vec<String>,
    /// The `@allegiance` subcommands it lacks.
    pub hidden_allegiance: Vec<&'static str>,
    /// The `@allegiance chat` subcommands it lacks.
    pub hidden_allegiance_chat: Vec<&'static str>,
    /// The `@house` subcommands it lacks.
    pub hidden_house: Vec<&'static str>,
    /// Whether `@lifestone` kills the character rather than recalls it.
    pub lifestone_kills: bool,
}

impl HelpScope {
    /// The scope of a world playing `features`, over the command table `table`.
    #[must_use]
    pub fn of(features: &EraFeatures, table: &[CommandEntry]) -> Self {
        let hidden = table
            .iter()
            .filter(|e| e.handler.is_some_and(|h| !handler_available(features, h)))
            .map(|e| e.name.to_ascii_lowercase())
            .collect();
        let hidden_allegiance = [
            "boot",
            "ban",
            "officer",
            "title",
            "lock",
            "house",
            "name",
            "motd",
            "broadcast",
            "chat",
        ]
        .into_iter()
        .filter(|sub| !allegiance_subcommand_available(features, sub))
        .collect();
        let hidden_allegiance_chat = ["kick", "gag", "ungag"]
            .into_iter()
            .filter(|sub| !allegiance_chat_subcommand_available(features, sub))
            .collect();
        let hidden_house = ["recall", "mansion_recall", "alleg_recall"]
            .into_iter()
            .filter(|sub| !house_subcommand_available(features, sub))
            .collect();
        Self {
            hidden,
            hidden_allegiance,
            hidden_allegiance_chat,
            hidden_house,
            lifestone_kills: !features.lifestone_recall,
        }
    }

    /// Whether help says nothing of the command `name` (typed without its `@`).
    #[must_use]
    pub fn hides(&self, name: &str) -> bool {
        self.hidden.iter().any(|h| h.eq_ignore_ascii_case(name))
    }

    /// Whether a help line, which starts with the command it describes, describes something the
    /// world lacks.
    #[must_use]
    pub fn hides_line(&self, line: &str) -> bool {
        let mut words = line.split_whitespace();
        let Some(command) = words.next().and_then(|w| w.strip_prefix('@')) else {
            return false;
        };
        if self.hides(command) {
            return true;
        }
        let subs: &[&str] = if command.eq_ignore_ascii_case("allegiance") {
            &self.hidden_allegiance
        } else if command.eq_ignore_ascii_case("house") {
            &self.hidden_house
        } else {
            return false;
        };
        let Some(sub) = words.next() else {
            return false;
        };
        if subs.iter().any(|h| h.eq_ignore_ascii_case(sub)) {
            return true;
        }
        command.eq_ignore_ascii_case("allegiance")
            && sub.eq_ignore_ascii_case("chat")
            && words.next().is_some_and(|chat| {
                self.hidden_allegiance_chat
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(chat))
            })
    }

    /// One help string as this world tells it: its lines without those [`Self::hides_line`]
    /// leaves out, and `@lifestone`'s recall described as the death it is here. `None` when every
    /// line is left out.
    #[must_use]
    pub fn filter(&self, text: &str) -> Option<String> {
        let mut out = String::new();
        let mut kept = false;
        for line in text.split_inclusive('\n') {
            if self.hides_line(line) {
                continue;
            }
            kept = true;
            if self.lifestone_kills && line.starts_with("@lifestone - ") {
                out.push_str(LIFESTONE_DEATH_HELP);
            } else {
                out.push_str(line);
            }
        }
        kept.then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::CommandInterp;
    use dereth_primitives::EraId;

    /// Behaviour: none (the launch world's command scope, read off its announced systems).
    #[test]
    fn a_launch_world_hides_the_later_commands_and_keeps_the_rest() {
        let interp = CommandInterp::new();
        let scope = HelpScope::of(&EraId::Launch.features(), interp.entries());
        for name in [
            "die",
            "pkarena",
            "corpse",
            "permit",
            "consent",
            "log",
            "motd",
            "marketplace",
        ] {
            assert!(scope.hides(name), "{name}");
        }
        for name in ["lifestone", "help", "tell", "allegiance", "afk"] {
            assert!(!scope.hides(name), "{name}");
        }
        assert!(scope.hides_line("@allegiance boot [-account] <name> - Removes a character.\n"));
        assert!(scope.hides_line("@allegiance name <set/clear> - Displays...\n"));
        assert!(!scope.hides_line("@allegiance info <name> - Requests information.\n"));
        assert!(scope.lifestone_kills);
        let later = HelpScope::of(&EraId::Eor.features(), interp.entries());
        assert_eq!(
            later,
            HelpScope {
                hidden: vec!["sepaku".to_owned()],
                ..HelpScope::default()
            },
            "the end of retail hides only the older name of the command that killed"
        );
    }

    /// Behaviour: presentation.era.allegiance-chat-follows-the-world
    /// `@a`, `@guild` and `@gu` and the whole of `@allegiance chat` follow the chat room; kicking
    /// and gagging there also need the officers; `@speaker` follows the allegiance's messages.
    /// `@sepaku` is the older worlds' other name for the `@lifestone` that kills.
    #[test]
    fn sepaku_is_only_on_a_world_whose_lifestone_command_kills() {
        let kills = EraFeatures {
            lifestone_recall: false,
            ..EraFeatures::ALL
        };
        assert!(handler_available(&kills, CommandHandler::Sepaku));
        assert!(!handler_available(
            &EraFeatures::ALL,
            CommandHandler::Sepaku
        ));
    }

    #[test]
    fn the_allegiance_chat_commands_and_their_help_follow_the_world() {
        let mut interp = CommandInterp::new();
        interp.add_turbine_chat_commands();
        let no_chat = EraFeatures {
            allegiance_chat: false,
            ..EraFeatures::ALL
        };
        let scope = HelpScope::of(&no_chat, interp.entries());
        for name in ["a", "guild", "gu"] {
            assert!(scope.hides(name), "{name}");
        }
        assert!(scope.hides_line("@a - Sends a message to your Allegiance. Also: @guild, @gu\n"));
        assert!(scope.hides_line("@allegiance chat <on/off> - Turn allegiance chat on and off.\n"));
        assert!(!scope.hides_line("@allegiance info <name> - Requests information.\n"));
        let args = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert!(!allegiance_command_available(
            &no_chat,
            &args(&["chat", "on"])
        ));
        assert!(!allegiance_command_available(
            &no_chat,
            &args(&["CH", "kick"])
        ));
        assert!(allegiance_command_available(&no_chat, &args(&["info"])));

        let no_officers = EraFeatures {
            allegiance_officers_and_bans: false,
            ..EraFeatures::ALL
        };
        let scope = HelpScope::of(&no_officers, interp.entries());
        assert!(!scope.hides_line("@allegiance chat <on/off> - Turn allegiance chat on and off.\n"));
        for line in [
            "@allegiance chat kick <name> - Kick a player.\n",
            "@allegiance chat gag <name> - Gags a player.\n",
            "@allegiance chat ungag <name> - Ungags a member.\n",
        ] {
            assert!(scope.hides_line(line), "{line}");
        }
        assert!(allegiance_command_available(
            &no_officers,
            &args(&["chat", "off"])
        ));
        assert!(allegiance_command_available(&no_officers, &args(&["chat"])));
        for sub in ["kick", "gag", "UNGAG"] {
            assert!(!allegiance_command_available(
                &no_officers,
                &args(&["chat", sub])
            ));
        }
        assert!(allegiance_command_available(
            &EraFeatures::ALL,
            &args(&["chat", "gag"])
        ));

        // February 2005 has the officers, the bans and the kick, and none of what came later.
        let f = EraFeatures::INFILTRATION;
        for sub in [&["officer"][..], &["ban"], &["chat", "kick"]] {
            assert!(allegiance_command_available(&f, &args(sub)), "{sub:?}");
        }
        for sub in [
            &["title"][..],
            &["lock"],
            &["name"],
            &["house"],
            &["chat", "gag"],
            &["chat", "ungag"],
        ] {
            assert!(!allegiance_command_available(&f, &args(sub)), "{sub:?}");
        }

        let no_messages = EraFeatures {
            allegiance_messages: false,
            ..EraFeatures::ALL
        };
        assert!(HelpScope::of(&no_messages, interp.entries()).hides("speaker"));
        assert!(!HelpScope::of(&EraFeatures::ALL, interp.entries()).hides("speaker"));
    }

    /// Behaviour: presentation.era.house-recalls-follow-the-apartments
    /// The recalls came with the apartments: a world with houses and no apartments has `@house`
    /// and `@hslist` but no recall, by either name.
    #[test]
    fn the_house_recalls_follow_the_apartments_and_the_rest_follows_the_houses() {
        let interp = CommandInterp::new();
        let houses_only = EraFeatures {
            apartments: false,
            ..EraFeatures::ALL
        };
        let scope = HelpScope::of(&houses_only, interp.entries());
        for name in ["hr", "hom", "hor", "hoa"] {
            assert!(scope.hides(name), "{name}");
        }
        for name in ["house", "hslist"] {
            assert!(!scope.hides(name), "{name}");
        }
        for sub in ["recall", "RE", "mansion_recall", "alleg_recall", "ma"] {
            assert!(!house_subcommand_available(&houses_only, sub), "{sub}");
            assert!(house_subcommand_available(&EraFeatures::ALL, sub), "{sub}");
        }
        assert!(house_subcommand_available(&houses_only, "open"));
        assert!(scope.hides_line("@house recall - Teleports you to your house.\n"));
        assert!(scope.hides_line("@house mansion_recall - Teleports you.\n"));
        assert!(!scope.hides_line("@house open - Creates an open house.\n"));
        let no_houses = EraFeatures {
            housing: false,
            apartments: false,
            ..EraFeatures::ALL
        };
        let scope = HelpScope::of(&no_houses, interp.entries());
        for name in ["house", "hslist", "hr", "hom"] {
            assert!(scope.hides(name), "{name}");
        }
    }
}
