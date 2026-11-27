//! The chat lines the player's scripts wrote, kept by the shell so that a switch of interface
//! carries the log: an interface shown again is handed the lines written while another was shown,
//! and one shown for the first time the lines written before it (as many as are kept), ahead of
//! the frame's own. Each interface keeps its own log from what it is handed. When the character
//! leaves the world the lines written so far are forgotten: the game's chat windows go with the
//! play screen, and the next entry's start empty, under whichever interface is shown then.

use std::collections::VecDeque;

use crate::interface::{Choice, Shown};

/// The most lines kept for an interface shown later.
pub const KEPT: usize = 500;

/// The chat lines written, and how far each interface shown has had them.
#[derive(Debug, Default)]
pub struct ChatHistory {
    /// The last [`KEPT`] lines written, oldest first.
    lines: VecDeque<Shown>,
    /// How many lines have been written in all.
    written: u64,
    /// For each interface shown, how many of the lines written it has been handed.
    handed: Vec<(Choice, u64)>,
}

impl ChatHistory {
    /// The lines `shown` is handed this frame: those it has not had yet (the lines written while
    /// another interface was shown, or all those kept, for one never shown before), then the
    /// frame's own, `fresh`. The earlier lines come before every one of the frame's notices.
    pub fn hand(&mut self, shown: Choice, fresh: Vec<Shown>) -> Vec<Shown> {
        let had = self
            .handed
            .iter()
            .find(|(c, _)| *c == shown)
            .map_or(0, |(_, n)| *n);
        let first_kept = self.written - self.lines.len() as u64;
        let missed = self.written.saturating_sub(had.max(first_kept));
        #[allow(clippy::cast_possible_truncation)] // no more than the lines kept
        let mut out: Vec<Shown> = self
            .lines
            .iter()
            .skip(self.lines.len() - missed as usize)
            .cloned()
            .map(|line| Shown {
                notices_before: 0,
                ..line
            })
            .collect();
        for line in &fresh {
            if self.lines.len() == KEPT {
                self.lines.pop_front();
            }
            self.lines.push_back(line.clone());
        }
        self.written += fresh.len() as u64;
        match self.handed.iter_mut().find(|(c, _)| *c == shown) {
            Some((_, n)) => *n = self.written,
            None => self.handed.push((shown, self.written)),
        }
        out.extend(fresh);
        out
    }

    /// The character left the world: no interface is handed the lines written so far.
    // D2-01936, D2-01928
    pub fn forget(&mut self) {
        self.lines.clear();
        for (_, n) in &mut self.handed {
            *n = self.written;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own: the shell keeping the log across a switch of interface)
    use super::*;

    fn line(text: &str) -> Shown {
        Shown {
            kind: 0x10,
            about: None,
            text: text.to_owned(),
            info: None,
            notices_before: 3,
        }
    }

    fn texts(lines: &[Shown]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn the_interface_shown_throughout_is_handed_only_each_frame_s_own_lines() {
        let mut h = ChatHistory::default();
        assert_eq!(texts(&h.hand(Choice::Retail, vec![line("one")])), ["one"]);
        assert_eq!(texts(&h.hand(Choice::Retail, vec![])), Vec::<&str>::new());
        let two = h.hand(Choice::Retail, vec![line("two")]);
        assert_eq!(texts(&two), ["two"]);
        assert_eq!(two[0].notices_before, 3, "the frame's own lines as written");
    }

    #[test]
    fn an_interface_shown_later_starts_from_the_lines_before_it_and_one_shown_again_from_those_it_missed(
    ) {
        let mut h = ChatHistory::default();
        h.hand(Choice::Retail, vec![line("one"), line("two")]);
        let first = h.hand(Choice::Horizon, vec![line("three")]);
        assert_eq!(texts(&first), ["one", "two", "three"]);
        assert_eq!(first[0].notices_before, 0, "before the frame's notices");
        assert_eq!(first[2].notices_before, 3);
        h.hand(Choice::Horizon, vec![line("four")]);
        assert_eq!(
            texts(&h.hand(Choice::Retail, vec![line("five")])),
            ["three", "four", "five"],
            "what was written while the other was shown"
        );
        assert_eq!(
            texts(&h.hand(Choice::Horizon, vec![])),
            ["five"],
            "and back again"
        );
    }

    #[test]
    fn no_interface_is_handed_the_lines_written_before_the_world_was_left() {
        let mut h = ChatHistory::default();
        h.hand(Choice::Retail, vec![line("one")]);
        h.hand(Choice::Horizon, vec![line("two")]);
        h.forget();
        assert_eq!(
            texts(&h.hand(Choice::Retail, vec![line("three")])),
            ["three"],
            "shown before"
        );
        let mut never = ChatHistory::default();
        never.hand(Choice::Retail, vec![line("one")]);
        never.forget();
        assert_eq!(
            texts(&never.hand(Choice::Horizon, vec![line("two")])),
            ["two"],
            "shown for the first time"
        );
    }

    #[test]
    fn no_more_than_the_lines_kept_are_handed_to_an_interface_shown_later() {
        let mut h = ChatHistory::default();
        let many: Vec<Shown> = (0..KEPT + 20).map(|n| line(&n.to_string())).collect();
        h.hand(Choice::Retail, many);
        let later = h.hand(Choice::Horizon, vec![]);
        assert_eq!(later.len(), KEPT);
        assert_eq!(later[0].text, "20", "the oldest let go");
        assert_eq!(later[KEPT - 1].text, (KEPT + 19).to_string());
    }
}
