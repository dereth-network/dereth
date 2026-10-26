//! The plugin host's question to the player, apart from the game's: a file a plugin asks for, and
//! the player's answer to it.
//!
//! The interface shows it as a dialog in its own look. An answer carries the id of the question it
//! was given to, so it answers that question alone and never another put in its place.

/// A question the plugin host puts to the player. It never passes through the game's dialog
/// service, whose questions come from the game and its requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostQuestion {
    /// Changes with every question put, so that an answer reaches only the question it was
    /// given to.
    pub id: u64,
    pub title: String,
    /// Who asks: the plugin's name, version and id.
    pub asker: String,
    pub text: String,
    /// Lines the player picks one of before answering (a file each); none is picked to begin
    /// with.
    pub picks: Vec<String>,
    /// Whether the first answer needs a line picked: until one is, it cannot be given (with none
    /// to pick, never).
    pub needs_pick: bool,
    /// The first button's words: the answer yes.
    pub yes: String,
    /// The second button's words, the answer no; none for a notice with one button.
    pub no: Option<String>,
}

/// The player's answer to a [`HostQuestion`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostReply {
    /// The id of the question answered.
    pub id: u64,
    /// The first button (or yes), else the last.
    pub yes: bool,
    /// The line the player picked, from 0.
    pub picked: Option<usize>,
}

impl HostReply {
    /// The answer to `question` with nothing picked.
    #[must_use]
    pub fn to(question: &HostQuestion, yes: bool) -> Self {
        Self {
            id: question.id,
            yes,
            picked: None,
        }
    }
}
