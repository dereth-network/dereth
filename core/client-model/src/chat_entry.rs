//! One retained chat entry per logical window, shared by all interfaces.

use dereth_client_contract::chat::{
    entry::{EntryAction, EntryUpdate, ReplyTarget},
    window::ReplyTargets,
};
use dereth_client_contract::options::interface::Interface;

/// Text and command history. Focus, glyphs and selection belong to the active widget.
#[derive(Debug, Clone, Default)]
pub struct ChatEntry {
    pub text: String,
    history: Vec<String>,
    position: Option<usize>,
}

impl ChatEntry {
    #[must_use]
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Record exactly one submitted line, preserving the original wide text.
    pub fn submit(&mut self, text: &str, interface: Interface) {
        if text.is_empty() {
            return;
        }
        self.history.push(text.to_owned());
        let limit = match interface {
            Interface::Classic => 10,
            Interface::Retail => 100,
        };
        if self.history.len() > limit {
            self.history.drain(..self.history.len() - limit);
        }
        self.position = None;
        self.text.clear();
    }

    /// Apply one entry operation. A missing reply target can return interface feedback.
    pub fn apply(
        &mut self,
        window: u32,
        text: String,
        action: EntryAction,
        targets: &ReplyTargets,
        interface: Interface,
    ) -> (Option<EntryUpdate>, Option<&'static str>) {
        self.text = text;
        let mut focus = false;
        let mut cursor = None;
        let mut warning = None;
        match action {
            EntryAction::Draft => return (None, None),
            EntryAction::Previous => {
                let next = match self.position {
                    Some(0) if !self.history.is_empty() => return (None, None),
                    Some(p) if p < self.history.len() => p - 1,
                    _ if self.history.is_empty() => return (None, None),
                    _ => self.history.len() - 1,
                };
                self.position = Some(next);
                self.text.clone_from(&self.history[next]);
            }
            EntryAction::Next => match self.position.filter(|p| *p + 1 < self.history.len()) {
                Some(p) => {
                    self.position = Some(p + 1);
                    self.text.clone_from(&self.history[p + 1]);
                }
                None => {
                    self.position = None;
                    self.text.clear();
                }
            },
            EntryAction::RecallLast => {
                let Some(last) = self.history.last() else {
                    return (None, None);
                };
                self.text.clone_from(last);
            }
            EntryAction::Reply { target, prefix } => {
                let Some(name) = reply_name(targets, target) else {
                    let warning =
                        (interface == Interface::Classic).then_some(reply_warning(target));
                    return (None, warning);
                };
                let prefix = if interface == Interface::Classic {
                    "@t "
                } else {
                    &prefix
                };
                self.text = format!("{prefix}{name}, ");
                focus = true;
            }
            EntryAction::StartTell { name } => {
                self.text = format!("@tell {name}, ");
                focus = true;
            }
            EntryAction::ExpandAlias => {
                if interface == Interface::Classic {
                    let Some(target) = exact_alias(&self.text) else {
                        return (None, None);
                    };
                    if let Some(name) = reply_name(targets, target) {
                        self.text = format!("@t {name}, ");
                        focus = true;
                    } else {
                        self.text.clear();
                        warning = Some(reply_warning(target));
                    }
                } else {
                    let Some((value, at)) = expand_alias(&self.text, targets) else {
                        return (None, None);
                    };
                    self.text = value;
                    cursor = Some(at);
                }
            }
        }
        (
            Some(EntryUpdate {
                window,
                cursor: cursor.unwrap_or_else(|| self.text.chars().count()),
                text: self.text.clone(),
                focus,
            }),
            warning,
        )
    }
}

fn reply_warning(target: ReplyTarget) -> &'static str {
    match target {
        ReplyTarget::LastTeller => "Someone must @tell you first!",
        ReplyTarget::Monarch => "One of your vassals must @m you first!",
        ReplyTarget::Patron => "One of your vassals must @p you first!.",
    }
}
fn exact_alias(text: &str) -> Option<ReplyTarget> {
    let text = text.strip_prefix('@').or_else(|| text.strip_prefix('/'))?;
    match text.to_ascii_lowercase().as_str() {
        "r" | "rp" | "reply" => Some(ReplyTarget::LastTeller),
        "mr" => Some(ReplyTarget::Monarch),
        "pr" => Some(ReplyTarget::Patron),
        _ => None,
    }
}

fn reply_name(targets: &ReplyTargets, target: ReplyTarget) -> Option<&str> {
    match target {
        ReplyTarget::LastTeller => targets.last_teller.as_deref(),
        ReplyTarget::Monarch => targets.monarch.as_deref(),
        ReplyTarget::Patron => targets.patron.as_deref(),
    }
    .filter(|s| !s.is_empty())
}

/// Replace only the leading command alias; its typed space and following text survive.
#[must_use]
fn expand_alias(text: &str, targets: &ReplyTargets) -> Option<(String, usize)> {
    let trimmed = text.trim_start();
    let mut chars = trimmed.chars();
    if !matches!(chars.next()?, '@' | '/') {
        return None;
    }
    let body: Vec<char> = chars.collect();
    for (alias, target) in [
        ("r ", ReplyTarget::LastTeller),
        ("rp ", ReplyTarget::LastTeller),
        ("reply ", ReplyTarget::LastTeller),
        ("mr ", ReplyTarget::Monarch),
        ("pr ", ReplyTarget::Patron),
    ] {
        let count = alias.chars().count();
        if body.len() < count
            || !body[..count]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case(alias)
        {
            continue;
        }
        let name = reply_name(targets, target)?;
        let replacement = format!("@tell {name},");
        let tail: String = body[count - 1..].iter().collect();
        return Some((
            format!("{replacement}{tail}"),
            replacement.chars().count() + 1,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn targets() -> ReplyTargets {
        ReplyTargets {
            last_teller: Some("Alba".into()),
            monarch: Some("Bex".into()),
            patron: Some("Cato".into()),
        }
    }
    fn edit(entry: &mut ChatEntry, action: EntryAction) -> Option<EntryUpdate> {
        entry
            .apply(8, entry.text.clone(), action, &targets(), Interface::Retail)
            .0
    }
    /// Behaviour: chat.entry-history
    #[test]
    fn history_has_dead_ends_clears_forward_drafts_and_ignores_empty_submissions() {
        let mut e = ChatEntry::default();
        e.submit("", Interface::Retail);
        assert!(e.history().is_empty());
        assert!(edit(&mut e, EntryAction::Previous).is_none());
        for line in ["one", "two", "three"] {
            e.submit(line, Interface::Retail);
        }
        e.text = "draft".into();
        assert_eq!(edit(&mut e, EntryAction::Next).unwrap().text, "");
        for expected in ["three", "two", "one"] {
            assert_eq!(edit(&mut e, EntryAction::Previous).unwrap().text, expected);
        }
        assert!(edit(&mut e, EntryAction::Previous).is_none());
        for expected in ["two", "three", ""] {
            assert_eq!(edit(&mut e, EntryAction::Next).unwrap().text, expected);
        }
        assert_eq!(edit(&mut e, EntryAction::RecallLast).unwrap().text, "three");
    }
    /// Behaviour: chat.entry-interface-history
    #[test]
    fn each_interface_bounds_history_on_submission_and_switching_keeps_it() {
        let mut e = ChatEntry::default();
        for n in 0..120 {
            e.submit(&n.to_string(), Interface::Retail);
        }
        assert_eq!(e.history().len(), 100);
        assert_eq!(e.history()[0], "20");
        e.apply(
            8,
            "draft".into(),
            EntryAction::Draft,
            &targets(),
            Interface::Classic,
        );
        assert_eq!(e.history().len(), 100);
        assert_eq!(e.text, "draft");
        e.submit("classic", Interface::Classic);
        assert_eq!(e.history().len(), 10);
        assert_eq!(e.history()[0], "111");
        e.submit("modern", Interface::Retail);
        assert_eq!(e.history().len(), 11);
    }
    /// Behaviour: chat.entry-replies
    #[test]
    fn canonical_reply_actions_address_distinct_sender_slots_and_keep_interface_prefixes() {
        for (raw, name) in [
            (0x10000020, "Bex"),
            (0x10000021, "Cato"),
            (0x10000022, "Alba"),
        ] {
            let target =
                ReplyTarget::from_action(dereth_client_contract::actions::ActionId(raw)).unwrap();
            for interface in Interface::ALL {
                let mut e = ChatEntry::default();
                let (update, warning) = e.apply(
                    8,
                    "draft".into(),
                    EntryAction::Reply {
                        target,
                        prefix: "localized ".into(),
                    },
                    &targets(),
                    interface,
                );
                let prefix = if interface == Interface::Classic {
                    "@t "
                } else {
                    "localized "
                };
                assert_eq!(update.unwrap().text, format!("{prefix}{name}, "));
                assert_eq!(warning, None);
            }
        }
    }
    /// Behaviour: chat.entry-replies
    #[test]
    fn missing_reply_preserves_the_draft_and_classic_reports_the_exact_warning() {
        for (target, warning) in [
            (ReplyTarget::LastTeller, "Someone must @tell you first!"),
            (
                ReplyTarget::Monarch,
                "One of your vassals must @m you first!",
            ),
            (
                ReplyTarget::Patron,
                "One of your vassals must @p you first!.",
            ),
        ] {
            for interface in Interface::ALL {
                let mut e = ChatEntry::default();
                let (update, message) = e.apply(
                    8,
                    "draft".into(),
                    EntryAction::Reply {
                        target,
                        prefix: "@tell ".into(),
                    },
                    &ReplyTargets::default(),
                    interface,
                );
                assert!(update.is_none());
                assert_eq!(e.text, "draft");
                assert_eq!(
                    message,
                    (interface == Interface::Classic).then_some(warning)
                );
            }
        }
    }
    /// Behaviour: chat.entry-aliases
    #[test]
    fn aliases_require_a_sigil_and_complete_leading_alias_and_preserve_tail_and_cursor() {
        for (alias, name) in [
            ("r", "Alba"),
            ("rp", "Alba"),
            ("reply", "Alba"),
            ("mr", "Bex"),
            ("pr", "Cato"),
        ] {
            for sigil in ['@', '/'] {
                let text = format!("  {sigil}{} hello", alias.to_uppercase());
                let (value, cursor) = expand_alias(&text, &targets()).unwrap();
                assert_eq!(value, format!("@tell {name}, hello"));
                assert_eq!(cursor, format!("@tell {name}, ").chars().count());
            }
        }
        for text in ["r ", "@r", "@replying ", "x @r ", "", "@"] {
            assert!(expand_alias(text, &targets()).is_none(), "{text}");
        }
        assert!(expand_alias("@r ", &ReplyTargets::default()).is_none());
    }
    /// Behaviour: chat.entry-aliases
    #[test]
    fn classic_aliases_match_the_whole_entry_before_space_and_clear_missing_targets() {
        let mut e = ChatEntry::default();
        for (alias, name) in [
            ("r", "Alba"),
            ("rp", "Alba"),
            ("reply", "Alba"),
            ("mr", "Bex"),
            ("pr", "Cato"),
        ] {
            for sigil in ['@', '/'] {
                let (update, warning) = e.apply(
                    8,
                    format!("{sigil}{alias}"),
                    EntryAction::ExpandAlias,
                    &targets(),
                    Interface::Classic,
                );
                assert_eq!(update.unwrap().text, format!("@t {name}, "));
                assert_eq!(warning, None);
            }
        }
        for text in [" @r", "@r ", "@r hello", "r", "@replying", ""] {
            assert!(e
                .apply(
                    8,
                    text.into(),
                    EntryAction::ExpandAlias,
                    &targets(),
                    Interface::Classic
                )
                .0
                .is_none());
            assert_eq!(e.text, text);
        }
        let (update, warning) = e.apply(
            8,
            "@pr".into(),
            EntryAction::ExpandAlias,
            &ReplyTargets::default(),
            Interface::Classic,
        );
        assert_eq!(update.unwrap().text, "");
        assert_eq!(warning, Some("One of your vassals must @p you first!."));
    }

    /// Behaviour: chat.entry-start-tell
    #[test]
    fn start_tell_replaces_the_draft_without_submitting_or_recording_it() {
        for interface in Interface::ALL {
            let mut e = ChatEntry::default();
            let (update, _) = e.apply(
                8,
                "old".into(),
                EntryAction::StartTell {
                    name: "+A Name".into(),
                },
                &targets(),
                interface,
            );
            let update = update.unwrap();
            assert_eq!(update.text, "@tell +A Name, ");
            assert!(update.focus);
            assert_eq!(update.cursor, update.text.chars().count());
            assert!(e.history().is_empty());
        }
    }
}
