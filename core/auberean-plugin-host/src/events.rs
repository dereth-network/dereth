//! What a plugin hears of the game: the client's events ([`GameEvent`]) as plugin events.
//!
//! Auberean's events are the interface's shared ones that name no game's content
//! ([`Game::events`] for [`Game::Auberean`]: the session's, `chat.line`, the windows', the
//! actions', the services', the plugin's own and its contexts') and the game's own ([`EVENTS`]).
//! Each is delivered in the order the client told it, after the frame's contexts change.
//!
//! An object of the game is named by its id as text, `0x` and sixteen hex digits ([`id_text`]):
//! the game's ids are 64 bits wide, more than a plugin's number holds exactly.

use auberean_client_contract::event::{
    ChatEvent, CraftEvent, EffectEvent, GameEvent, SessionEvent, SocialEvent, TradeEvent,
};
use auberean_client_contract::session::Phase;
use dereth_plugin_api::{Event, Game, Value};

/// Auberean's own events, with the fields each one's argument carries, in the order the spec lists
/// them (`docs/plugins/auberean.md`; a test holds the two together).
pub const EVENTS: &[(&str, &str)] = &[
    (
        "chat.said",
        "speaker (nil when the server named no one), text",
    ),
    ("chat.told", "teller, name, text"),
    (
        "social.recruitment_offered",
        "leader, leader_name, fellowship",
    ),
    ("social.oath_offered", "vassal, name"),
    ("trade.registered", "asker, asked, partner"),
    ("trade.opened", ""),
    ("trade.accepted", "by"),
    ("effects.added", "effect"),
    ("effects.removed", "effect"),
    ("craft.done", "recipe, error (0: it went), shown"),
];

/// Every event an Auberean plugin may hear, with its fields: the shared ones, then the game's own,
/// then the notices' ([`crate::notices::EVENTS`]).
pub fn all() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    Game::Auberean
        .events()
        .chain(EVENTS.iter())
        .chain(crate::notices::EVENTS.iter())
}

/// Whether `name` is an event an Auberean plugin may subscribe to.
#[must_use]
pub fn is_known(name: &str) -> bool {
    all().any(|(n, _)| *n == name)
}

/// A game object's id as a plugin reads it: `0x` and sixteen lower-case hex digits.
#[must_use]
pub fn id_text(id: u64) -> String {
    format!("0x{id:016x}")
}

/// An id a plugin gives: its text form (with or without `0x`), or a whole number small enough to
/// be exact.
#[must_use]
pub fn parse_id(v: &Value) -> Option<u64> {
    match v {
        Value::Str(s) => {
            let hex = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            (!hex.is_empty() && hex.len() <= 16)
                .then(|| u64::from_str_radix(hex, 16).ok())
                .flatten()
        }
        Value::Int(i) if (0..=1 << 53).contains(i) => u64::try_from(*i).ok(),
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Value::Num(n) if n.fract() == 0.0 && (0.0..=9_007_199_254_740_992.0).contains(n) => {
            Some(*n as u64)
        }
        _ => None,
    }
}

fn id(v: u64) -> Value {
    Value::from(id_text(v))
}

/// Why the server refused an entry, as a word: the reason's number is the event's `code`.
#[must_use]
pub const fn refusal_word(reason: u32) -> &'static str {
    match reason {
        11 => "character_in_world",
        20 => "booted",
        _ => "refused",
    }
}

/// The session as the events have told it so far, for the session's plugin events and
/// `session.info`.
#[derive(Debug, Clone, Default)]
pub struct Session {
    pub phase: Option<Phase>,
    /// The account's characters, as last listed: id and name.
    pub characters: Vec<(u64, String)>,
    /// The character entering or in the world, when the host knows it.
    pub playing: Option<u64>,
    /// The connection's end was told.
    pub disconnected: bool,
}

impl Session {
    /// The name of the character playing, when known.
    #[must_use]
    pub fn playing_name(&self) -> Option<&str> {
        let id = self.playing?;
        self.characters
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, n)| n.as_str())
    }

    /// The character's id and name, as the session's events carry them.
    fn character(&self) -> Value {
        Value::map()
            .with("id", self.playing.map(id_text))
            .with("character", self.playing_name().map(str::to_owned))
    }
}

/// The plugin events `e` makes, `session` brought up to date with it. `world` and `account` are
/// what the host was told of the session (for `session.pregame`).
pub fn translate(
    e: &GameEvent,
    session: &mut Session,
    world: Option<&str>,
    account: Option<&str>,
) -> Vec<Event> {
    let mut out = Vec::new();
    match e {
        GameEvent::Session(s) => match s {
            SessionEvent::Characters(list) => {
                session.characters = list
                    .characters
                    .iter()
                    .map(|c| (c.id, c.name.clone()))
                    .collect();
                let characters: Vec<Value> = session
                    .characters
                    .iter()
                    .map(|(c, n)| {
                        Value::map()
                            .with("id", id_text(*c))
                            .with("name", n.as_str())
                    })
                    .collect();
                out.push(Event::new(
                    "session.pregame",
                    Value::map()
                        .with("characters", characters)
                        .with("world", world.map(str::to_owned))
                        .with("account", account.map(str::to_owned)),
                ));
            }
            SessionEvent::Phase(p) => {
                let before = session.phase.replace(*p);
                match p {
                    Phase::Entering => out.push(Event::new("session.login", session.character())),
                    Phase::World => out.push(Event::new(
                        "session.enter_world",
                        Value::map().with("id", session.playing.map(id_text)),
                    )),
                    Phase::Characters if before == Some(Phase::World) => {
                        session.playing = None;
                        out.push(Event::new("session.logout", Value::map()));
                    }
                    Phase::Disconnected if !session.disconnected && before.is_some() => {
                        session.disconnected = true;
                        out.push(Event::new("session.disconnect", Value::map()));
                    }
                    _ => {}
                }
            }
            SessionEvent::EntryRefused { reason } => out.push(Event::new(
                "session.login_failed",
                Value::map()
                    .with("code", *reason)
                    .with("reason", refusal_word(*reason)),
            )),
            SessionEvent::Ended(_) if !session.disconnected => {
                session.disconnected = true;
                out.push(Event::new("session.disconnect", Value::map()));
            }
            _ => {}
        },
        GameEvent::Chat(c) => match c {
            ChatEvent::Line(l) => out.push(Event::new(
                "chat.line",
                Value::map()
                    .with("text", l.text.as_str())
                    .with("kind", l.kind)
                    .with("about", l.about.map(id_text)),
            )),
            ChatEvent::Said { speaker, text } => out.push(Event::new(
                "chat.said",
                Value::map()
                    .with("speaker", speaker.map(id_text))
                    .with("text", text.as_str()),
            )),
            ChatEvent::Told { teller, name, text } => out.push(Event::new(
                "chat.told",
                Value::map()
                    .with("teller", id(*teller))
                    .with("name", name.as_str())
                    .with("text", text.as_str()),
            )),
            _ => {}
        },
        GameEvent::Social(s) => match s {
            SocialEvent::RecruitmentOffered {
                leader,
                leader_name,
                fellowship,
            } => out.push(Event::new(
                "social.recruitment_offered",
                Value::map()
                    .with("leader", id(*leader))
                    .with("leader_name", leader_name.as_str())
                    .with("fellowship", fellowship.as_str()),
            )),
            SocialEvent::OathOffered { vassal, name } => out.push(Event::new(
                "social.oath_offered",
                Value::map()
                    .with("vassal", id(*vassal))
                    .with("name", name.as_str()),
            )),
            _ => {}
        },
        GameEvent::Trade(t) => match t {
            TradeEvent::Registered {
                asker,
                asked,
                partner,
            } => out.push(Event::new(
                "trade.registered",
                Value::map()
                    .with("asker", id(*asker))
                    .with("asked", id(*asked))
                    .with("partner", partner.as_str()),
            )),
            TradeEvent::Opened => out.push(Event::new("trade.opened", Value::map())),
            TradeEvent::Accepted { by } => out.push(Event::new(
                "trade.accepted",
                Value::map().with("by", id(*by)),
            )),
            _ => {}
        },
        GameEvent::Effect(f) => match f {
            EffectEvent::Added { effect } => out.push(Event::new(
                "effects.added",
                Value::map().with("effect", *effect),
            )),
            EffectEvent::Removed { effect } => out.push(Event::new(
                "effects.removed",
                Value::map().with("effect", *effect),
            )),
            _ => {}
        },
        GameEvent::Craft(CraftEvent::Done {
            recipe,
            error,
            shown,
        }) => out.push(Event::new(
            "craft.done",
            Value::map()
                .with("recipe", *recipe)
                .with("error", *error)
                .with("shown", *shown),
        )),
        _ => {}
    }
    out
}
