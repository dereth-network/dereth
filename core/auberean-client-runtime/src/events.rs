//! The stream of what happened, as the contract's events ([`GameEvent`]): the session's phases and
//! its end, the login's progress, the character list, the lines the scripts write for the chat
//! windows, and the server's calls a front end reads as events rather than as panels' notices
//! (sayings and tells, offers, a trade's steps, effects, a making's answer).
//!
//! The runtime appends to it as things take effect; a host takes it with
//! [`crate::Client::take_events`]. A host that never takes it loses nothing it needs: the oldest
//! [`EVENTS_KEPT`] go once twice that many wait.

use auberean_client_contract::event::{
    ChatEvent, CraftEvent, EffectEvent, GameEvent, SocialEvent, TradeEvent,
};
use auberean_client_contract::session::{Character, CharacterList};
use auberean_script::args::Arg;
use auberean_script::graph::Body;
use auberean_script::native_object::{NativeObject, TextInfo};
use auberean_script::Library;
use auberean_text::DatSource;

/// How many events are kept for a host that has not taken them.
pub const EVENTS_KEPT: usize = 4096;

/// The server's calls told as events: a saying (on the speaker), a tell, a place in a fellowship
/// offered, an oath offered, a trade set up, opened and accepted, an effect put on and taken off,
/// and a making answered.
pub mod function {
    pub const SAYING: u32 = 0x8205_0016;
    pub const TELL: u32 = 0x8205_0017;
    pub const RECRUITMENT_OFFERED: u32 = 0x820C_002E;
    pub const OATH_OFFERED: u32 = 0x8203_002F;
    pub const TRADE_REGISTERED: u32 = 0x8215_0046;
    pub const TRADE_OPENED: u32 = 0x8215_004E;
    pub const TRADE_ACCEPTED: u32 = 0x8215_0050;
    pub const EFFECT_ADDED: u32 = 0x8208_0017;
    pub const EFFECT_REMOVED: u32 = 0x8208_0016;
    pub const CRAFT_DONE: u32 = 0x8206_001C;

    /// Every call told as an event.
    pub const ALL: [u32; 10] = [
        SAYING,
        TELL,
        RECRUITMENT_OFFERED,
        OATH_OFFERED,
        TRADE_REGISTERED,
        TRADE_OPENED,
        TRADE_ACCEPTED,
        EFFECT_ADDED,
        EFFECT_REMOVED,
        CRAFT_DONE,
    ];
}

/// The events waiting for the host, and a copy of each for the frame's hook while one is
/// installed ([`crate::frame_hook`]).
#[derive(Debug, Default)]
pub(crate) struct Events {
    waiting: Vec<GameEvent>,
    for_hook: Option<Vec<GameEvent>>,
}

impl Events {
    pub(crate) fn push(&mut self, e: GameEvent) {
        if let Some(h) = &mut self.for_hook {
            keep(h, e.clone());
        }
        keep(&mut self.waiting, e);
    }

    pub(crate) fn take(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.waiting)
    }

    /// Keep a copy of each event from now on for a hook (`true`), or stop (`false`); either way
    /// the copies kept so far go.
    pub(crate) fn copy_for_hook(&mut self, copy: bool) {
        self.for_hook = copy.then(Vec::new);
    }

    /// The copies kept for the hook since it last took them.
    pub(crate) fn take_for_hook(&mut self) -> Vec<GameEvent> {
        self.for_hook
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

/// `e` at the end of `list`, the oldest [`EVENTS_KEPT`] gone first once twice that many wait.
fn keep(list: &mut Vec<GameEvent>, e: GameEvent) {
    if list.len() >= 2 * EVENTS_KEPT {
        list.drain(..EVENTS_KEPT);
    }
    list.push(e);
}

/// The character list as the contract carries it.
pub(crate) fn characters(list: &auberean_protocol::login::CharacterList) -> CharacterList {
    CharacterList {
        characters: list
            .characters
            .iter()
            .map(|c| Character {
                id: c.id,
                name: String::from_utf16_lossy(&c.name.0),
                seconds_greyed_out: c.seconds_greyed_out,
            })
            .collect(),
        slots: list.slots,
        account: list.account.text(),
        expansion: list.has_expansion != 0,
        third_flag: list.unknown != 0,
    }
}

/// A server's call as its event, when it is one of [`function::ALL`] and its arguments read as
/// the function's parameters: `object` is the object a call on an object is for (the speaker of a
/// saying); its text is written out over `text`.
pub(crate) fn of_call(
    called: u32,
    object: Option<u64>,
    args: &[u8],
    lib: &Library,
    text: &mut DatSource,
) -> Option<GameEvent> {
    use self::function as f;
    if !f::ALL.contains(&called) {
        return None;
    }
    // The values come last parameter first.
    let mut p = auberean_script::args::decode(args, lib).ok()?;
    p.reverse();
    let int = |i: usize| match p.get(i) {
        Some(Arg::Int(v)) => Some(*v),
        _ => None,
    };
    let long = |i: usize| match p.get(i) {
        Some(Arg::LongInt(v)) => Some(*v),
        _ => None,
    };
    let mut words = |i: usize| p.get(i).and_then(text_info).map(|t| write(t, text));
    Some(match called {
        f::SAYING => GameEvent::Chat(ChatEvent::Said {
            speaker: object,
            text: words(0)?,
        }),
        f::TELL => GameEvent::Chat(ChatEvent::Told {
            teller: long(0)?,
            name: words(1)?,
            text: words(2)?,
        }),
        f::RECRUITMENT_OFFERED => GameEvent::Social(SocialEvent::RecruitmentOffered {
            leader: long(0)?,
            leader_name: words(1)?,
            fellowship: words(2)?,
        }),
        f::OATH_OFFERED => GameEvent::Social(SocialEvent::OathOffered {
            vassal: long(0)?,
            name: words(1)?,
        }),
        f::TRADE_REGISTERED => GameEvent::Trade(TradeEvent::Registered {
            asker: long(0)?,
            asked: long(1)?,
            partner: words(2)?,
        }),
        f::TRADE_OPENED => GameEvent::Trade(TradeEvent::Opened),
        f::TRADE_ACCEPTED => GameEvent::Trade(TradeEvent::Accepted { by: long(0)? }),
        f::EFFECT_ADDED => GameEvent::Effect(EffectEvent::Added { effect: int(0)? }),
        f::EFFECT_REMOVED => GameEvent::Effect(EffectEvent::Removed { effect: int(0)? }),
        f::CRAFT_DONE => GameEvent::Craft(CraftEvent::Done {
            recipe: int(0)?,
            error: int(1)?,
            shown: int(2)? != 0,
        }),
        _ => return None,
    })
}

/// The string reference an argument carries: the first in its graph.
fn text_info(a: &Arg) -> Option<&TextInfo> {
    let Arg::Object(g) = a else { return None };
    g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o) => match &o.native {
            Some(NativeObject::TextInfo(t)) => Some(t),
            _ => None,
        },
        _ => None,
    })
}

/// A script's string reference written out, as the chat windows write it.
pub(crate) fn write(t: &TextInfo, text: &mut DatSource) -> String {
    crate::text::from_script(t).text(text)
}
