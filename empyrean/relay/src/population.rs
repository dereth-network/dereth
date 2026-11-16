//! How many players the world has, asked of it as a player can ask.
//!
//! The 1999 wire tells a client no count at login. An ac1999 server answers a player's `@pop`
//! with one line in the chat, `Current world population: N`, where N counts the characters in the
//! world. Once the player is in the world the relay sends that
//! command itself, as the player's own chat would go, then again every [`EVERY`] seconds for as long
//! as the session lasts, and passes each count on ([`crate::hub`]).
//!
//! Only the answer to the relay's own ask is kept out of the player's chat: a `@pop` the player
//! types is answered in the chat as usual. An ask that goes unanswered for [`ANSWER_WAIT`] seconds
//! is the last of the session, so a world that has dropped the command is not asked for ever.

/// The command the relay sends.
pub const ASK: &str = "@pop";

/// The answer's words before the count.
const ANSWER: &str = "Current world population: ";

/// How long after one ask the next is sent, in seconds.
pub const EVERY: f64 = 15.0 * 60.0;

/// How long an ask waits for its answer, in seconds.
pub const ANSWER_WAIT: f64 = 60.0;

/// The count in `line`, when it is the world's answer to the command, exactly.
#[must_use]
pub fn answer(line: &str) -> Option<u32> {
    let digits = line.strip_prefix(ANSWER)?;
    let players: u32 = digits.parse().ok()?;
    (players.to_string() == digits).then_some(players)
}

/// When to ask, and whether an ask is waiting for its answer, in one session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Census {
    /// Whether a character is in the world.
    in_world: bool,
    /// When the next ask is due, once a character has first entered the world.
    next: Option<f64>,
    /// When the ask that has not been answered yet was sent.
    waiting: Option<f64>,
    /// Whether an ask went unanswered, and no more are sent.
    given_up: bool,
}

impl Census {
    /// A character entered the world at `now`: the first ask is due at once; after that, the asks
    /// keep their time, whoever enters again.
    pub fn entered(&mut self, now: f64) {
        self.in_world = true;
        self.next.get_or_insert(now);
    }

    /// The character left the world: no ask is sent until one enters again.
    pub fn left(&mut self) {
        self.in_world = false;
    }

    /// Whether an ask is to be sent at `now`; when it is, the next is due [`EVERY`] seconds on.
    pub fn due(&mut self, now: f64) -> bool {
        if self.waiting.is_some_and(|sent| now - sent >= ANSWER_WAIT) {
            self.waiting = None;
            self.given_up = true;
        }
        if self.given_up || !self.in_world || self.waiting.is_some() {
            return false;
        }
        if self.next.is_none_or(|next| now < next) {
            return false;
        }
        self.next = Some(now + EVERY);
        self.waiting = Some(now);
        true
    }

    /// The world gave its count: whether it answers the relay's ask, which is then answered. An
    /// answer nothing waits for is the player's own.
    pub fn answered(&mut self) -> bool {
        self.waiting.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (when the relay asks the world how many players it has).

    use super::*;

    #[test]
    fn only_the_answer_exactly_gives_a_count() {
        assert_eq!(answer("Current world population: 0"), Some(0));
        assert_eq!(answer("Current world population: 37"), Some(37));
        for not in [
            "Current world population: ",
            "Current world population: 037",
            "Current world population: +3",
            "Current world population: 3 ",
            "Current world population: -1",
            "Current world population: 99999999999",
            "current world population: 3",
            " Current world population: 3",
            "Tester says, \"Current world population: 3\"",
        ] {
            assert_eq!(answer(not), None, "{not}");
        }
    }

    #[test]
    fn the_first_ask_waits_for_the_world_and_the_next_comes_every_fifteen_minutes() {
        let mut census = Census::default();
        assert!(!census.due(0.0), "nobody in the world");
        census.entered(100.0);
        assert!(census.due(100.0), "at once");
        assert!(!census.due(100.0), "once");
        assert!(census.answered());
        assert!(!census.due(100.0 + EVERY - 1.0));
        assert!(census.due(100.0 + EVERY), "fifteen minutes on");
        assert!(census.answered());
        assert!(!census.answered(), "a second answer is nobody's ask");
        assert!(census.due(100.0 + 2.0 * EVERY));
    }

    #[test]
    fn a_character_out_of_the_world_is_not_asked_for_and_entering_again_keeps_the_time() {
        let mut census = Census::default();
        census.entered(0.0);
        assert!(census.due(0.0));
        assert!(census.answered());
        census.left();
        assert!(!census.due(EVERY), "on the character list");
        census.entered(EVERY + 5.0);
        assert!(census.due(EVERY + 5.0), "due while away: asked on entering");
        assert!(census.answered());
        census.left();
        census.entered(EVERY + 10.0);
        assert!(!census.due(EVERY + 10.0), "entering again is no new ask");
        assert!(census.due(2.0 * EVERY + 5.0));
    }

    #[test]
    fn an_ask_left_unanswered_is_the_last() {
        let mut census = Census::default();
        census.entered(0.0);
        assert!(census.due(0.0));
        assert!(!census.due(ANSWER_WAIT - 1.0), "still waiting");
        assert!(!census.due(ANSWER_WAIT), "given up");
        assert!(!census.answered(), "a late answer is nobody's ask");
        assert!(!census.due(10.0 * EVERY), "never again this session");
    }
}
