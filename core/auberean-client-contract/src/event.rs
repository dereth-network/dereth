//! What happened in the game, in order: the stream of events the runtime appends to as it runs and
//! a host takes ([`GameEvent`]).
//!
//! Each event is plain data, its text written out and its objects named by id, so a front end, a
//! headless driver or a plugin needs nothing of the server's calls or the scripts to read it. Every
//! enum here may gain variants: a reader matches what it wants and lets the rest go.

use crate::session::{CharacterList, Heartbeat, LoginProgress, Phase, SessionEnd};

/// One thing that happened.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum GameEvent {
    /// The session: its phase, the login, the character list, its end.
    Session(SessionEvent),
    /// Chat: lines for the chat windows, sayings and tells heard.
    Chat(ChatEvent),
    /// Fellowship and allegiance: what other players offer.
    Social(SocialEvent),
    /// A trade between the player and another.
    Trade(TradeEvent),
    /// The effects on the player.
    Effect(EffectEvent),
    /// Making things.
    Craft(CraftEvent),
}

/// The session.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SessionEvent {
    /// The session moved to this phase.
    Phase(Phase),
    /// The login moved on.
    LoginProgress(LoginProgress),
    /// The connection was measured again.
    Heartbeat(Heartbeat),
    /// The account's characters were listed (again, after one is made or deleted).
    Characters(CharacterList),
    /// The server refused an entry into the world, or ended the session in it, for this reason (11:
    /// the character is still in the world; 20: booted).
    EntryRefused { reason: u32 },
    /// The session ended under the player and can go no further: told once.
    Ended(SessionEnd),
}

/// A line the player's scripts wrote for the chat windows: what kind of text it is (each chat
/// window shows some kinds), the object it is about, and its words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatLine {
    pub kind: u32,
    pub about: Option<u64>,
    pub text: String,
}

/// Chat.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChatEvent {
    /// A line for the chat windows.
    Line(ChatLine),
    /// Someone nearby said something: the speaker (none when the server named no object), the
    /// words.
    Said { speaker: Option<u64>, text: String },
    /// A tell to the player: who told it, by id and by name, and the words.
    Told {
        teller: u64,
        name: String,
        text: String,
    },
    /// The answer to an [`crate::Action::AskHistory`]: the list asked of, and its item there
    /// (none when the list has nothing more that way).
    History {
        list: crate::ChatHistory,
        text: Option<String>,
    },
    /// A line typed named a command no one knows: the line.
    UnknownCommand { text: String },
}

/// What other players offer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SocialEvent {
    /// A fellowship's leader offers the player a place in it: the leader, by id and by name, and
    /// the fellowship's name.
    RecruitmentOffered {
        leader: u64,
        leader_name: String,
        fellowship: String,
    },
    /// Another player offers the player its oath, to be the player's vassal: by id and by name.
    OathOffered { vassal: u64, name: String },
}

/// A trade.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TradeEvent {
    /// A trade was set up between two players: the one who asked for it, the one asked, and the
    /// other side's name as the player sees it.
    Registered {
        asker: u64,
        asked: u64,
        partner: String,
    },
    /// The trade window opened on both sides.
    Opened,
    /// One side accepted the trade as it stands.
    Accepted { by: u64 },
}

/// The effects on the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EffectEvent {
    /// An effect was put on the player, by its number.
    Added { effect: u32 },
    /// An effect was taken off the player, by its number.
    Removed { effect: u32 },
}

/// Making things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CraftEvent {
    /// The server answered a making of this recipe: why it failed (0: it went), and whether the
    /// panels are told.
    Done {
        recipe: u32,
        error: u32,
        shown: bool,
    },
}
