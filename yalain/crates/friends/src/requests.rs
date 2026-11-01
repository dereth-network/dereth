//! The client's requests, each answered once, with the codes whose lines the client shows.
//!
//! | request | answered |
//! |---|---|
//! | `GetFriendsList` | every friend with its mode |
//! | `FriendAdd` | this world's: the character must exist; another world's: the character and the world on the network. Kept (at most [`MAX_LISTED`]); the friend's state as an event first, then done |
//! | `FriendDelete` | forgotten; not in the list says so |
//! | `GetIgnoredList` | every ignored name |
//! | `IgnoredAdd`, `IgnoredDelete` | kept (at most [`MAX_LISTED`]) or forgotten |
//! | `SetLoc` | the character's state; its friends in the world are told its new mode, when it changed |
//! | `SendToUserEx` | this world's: shown to the character when present and not ignoring the teller; another world's: through the link, answered when it ends |
//!
//! The stream answers the rest itself (the ticket's place, `Flags`, the reserved requests).

use auberean_friends::{Answer, Cx, Event, Extra, Hr, Mode, Request};
use dereth_network_protocol::limits::{Bucket, Limits};
use dereth_network_protocol::Address;
use yalain_federation::{Ended, Federation, State, TellEnd, AWAY_TEXT, DO_NOT_DISTURB_TEXT};
use yalain_federation::{NOT_ON, TOO_QUICKLY, WORLD_OFF};
use yalain_store::{NetworkList, Store};

use crate::names::Target;
use crate::{
    secs, Adding, Service, StreamId, Waiting, ADD_WAIT, ALREADY_IN_LIST, FRIENDS_FULL,
    FRIEND_REFUSED, IGNORED_FULL, MAX_LISTED, NOT_IN_LIST,
};

/// The code a tell to another world ends with, as the teller's client reads it: done (its own
/// echo), not logged in (it names the recipient), or could not be sent (any other).
#[must_use]
pub fn tell_code(ended: Ended) -> Hr {
    match ended {
        Ended::Delivered | Ended::Away | Ended::Squelched => Hr::OK,
        Ended::Offline => Hr::NOT_ONLINE,
        Ended::Unavailable | Ended::NotOnNetwork | Ended::NetworkUnavailable => Hr::TIMED_OUT,
        Ended::RateLimited | Ended::TooQuickly | Ended::NotOn | Ended::WorldOff => Hr::DENIED,
    }
}

/// The away line a teller is shown for a recipient of this world.
fn away_line(name: &str, state: &State) -> Option<String> {
    let text = match state {
        State::Available => return None,
        State::Away(t) => t.as_deref().unwrap_or(AWAY_TEXT),
        State::DoNotDisturb(t) => t.as_deref().unwrap_or(DO_NOT_DISTURB_TEXT),
    };
    Some(format!("{name} is away: {text}"))
}

impl Service {
    pub(crate) fn request(
        &mut self,
        stream: StreamId,
        cx: Cx,
        request: Request,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        let Some(h) = self.here_on(stream).cloned() else {
            // The stream answers requests while no character is in the world itself; a request
            // that slips between its leaving and the stream hearing of it is answered alike.
            self.answer(stream, cx, Hr::NOT_CONNECTED, Answer::empty(request.kind()));
            return;
        };
        let fed_ref = fed.as_deref();
        match request {
            Request::GetFriendsList => {
                let friends = self.friends_of(&h.name, fed_ref);
                self.answer(stream, cx, Hr::OK, Answer::GetFriendsList { friends });
            }
            Request::GetIgnoredList => {
                let ignored = self
                    .lists
                    .get(&h.character)
                    .map(|l| {
                        l.ignored
                            .iter()
                            .map(|e| match crate::names::read_entry(e) {
                                Target::Local(n) => self.written_local(&n),
                                _ => e.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.answer(stream, cx, Hr::OK, Answer::GetIgnoredList { ignored });
            }
            Request::FriendAdd { user } => {
                // A friend on another world whose world is asked is answered when it answers.
                if let Some(hr) = self.befriend(stream, cx, &h.name, h.character, &user, store, fed)
                {
                    self.answer(stream, cx, hr, Answer::FriendAdd);
                }
            }
            Request::FriendDelete { user } => {
                let hr = self.unfriend(&h.name, h.character, &user, store, fed);
                self.answer(stream, cx, hr, Answer::FriendDelete);
            }
            Request::IgnoredAdd { user } => {
                let hr = self.ignore(&h.name, h.character, &user, true, store);
                self.answer(stream, cx, hr, Answer::IgnoredAdd);
            }
            Request::IgnoredDelete { user } => {
                let hr = self.ignore(&h.name, h.character, &user, false, store);
                self.answer(stream, cx, hr, Answer::IgnoredDelete);
            }
            Request::SetLoc { flags, .. } => {
                let state = if flags & 2 != 0 {
                    State::Away(None)
                } else if flags & 4 != 0 {
                    State::DoNotDisturb(None)
                } else {
                    State::Available
                };
                let mode = crate::mode_of(&state);
                if let Some(here) = self.here.get_mut(&h.name.to_lowercase()) {
                    here.state = state;
                }
                self.answer(stream, cx, Hr::OK, Answer::SetLoc);
                // The client prints a line for every state it is sent, and asks for the same
                // state more than once: friends are told only of a change.
                if crate::mode_of(&h.state) != mode {
                    self.tell_friends_of(&h.name, mode);
                }
            }
            Request::SendToUserEx { user, text, extra } => {
                self.tell(stream, cx, &h, &user, &text, extra, store, fed);
            }
            // The stream answers these itself.
            Request::PresentTicket { .. }
            | Request::Flags { .. }
            | Request::CallCreate
            | Request::SendToRoomEx => {
                self.answer(
                    stream,
                    cx,
                    Hr::NOT_IMPLEMENTED,
                    Answer::empty(request.kind()),
                );
            }
        }
    }

    /// The entry a list keeps for a character of this world: its name as the store writes it,
    /// when it exists.
    fn local_entry(store: &mut impl Store, name: &str) -> Option<(u64, String)> {
        match store.character_named(name) {
            Ok(Some(c)) if c.deleted_at.is_none() => Some((c.id, c.name)),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("friends service: the store: {e}");
                None
            }
        }
    }

    /// Adds a friend: the code to answer with now, or `None` for a friend on another world whose
    /// world was asked for its state ([`Service::added`] answers).
    #[allow(clippy::too_many_arguments)]
    fn befriend(
        &mut self,
        stream: StreamId,
        cx: Cx,
        owner: &str,
        character: u64,
        user: &str,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) -> Option<Hr> {
        let (entry, remote) = match self.target(user) {
            Target::NoSuchUser => return Some(Hr::NO_SUCH_USER),
            Target::Local(n) => match Self::local_entry(store, &n) {
                Some((id, name)) if id != character => (name, None),
                _ => return Some(Hr::NO_SUCH_USER),
            },
            Target::Remote(a) => {
                match fed.as_deref() {
                    Some(f) if f.enabled() => {
                        if !f.is_on(owner) {
                            self.line(character, NOT_ON);
                            return Some(FRIEND_REFUSED);
                        }
                    }
                    _ => {
                        self.line(character, WORLD_OFF);
                        return Some(FRIEND_REFUSED);
                    }
                }
                (a.to_string(), Some(a))
            }
        };
        if self.listed(character, NetworkList::Friends, &entry) {
            return Some(ALREADY_IN_LIST);
        }
        if self.count(character, NetworkList::Friends) >= MAX_LISTED {
            self.line(character, FRIENDS_FULL);
            return Some(FRIEND_REFUSED);
        }
        if let Err(e) = self.keep(store, character, owner, NetworkList::Friends, &entry, true) {
            tracing::warn!("friends service: the store: {e}");
            return Some(FRIEND_REFUSED);
        }
        // The friend's state first, then the answer, as the original service did. Another
        // world's friend is shown once, with the state its world gives: the client prints a line
        // for each state, so a guess followed by the truth would read as a false departure.
        let mut fed = fed;
        if let (Some(a), Some(f)) = (&remote, fed.as_deref_mut()) {
            if f.friend_added(self, owner, a) {
                let key = (owner.to_lowercase(), a.key());
                self.assumed_offline.remove(&key);
                self.adding.insert(
                    key,
                    Adding {
                        stream,
                        cx,
                        friend: a.clone(),
                        until: self.now + ADD_WAIT,
                    },
                );
                return None;
            }
        }
        let (user, mode) = self.status_of(owner, &entry, fed.as_deref());
        self.event(stream, Event::FriendStat { user, mode });
        Some(Hr::OK)
    }

    /// A friend on another world being added has its state, from its world or for want of an
    /// answer: shown, then the add answered.
    pub(crate) fn added(&mut self, key: &(String, String), online: bool) {
        let Some(a) = self.adding.remove(key) else {
            return;
        };
        let mode = if online { Mode::ONLINE } else { Mode::OFFLINE };
        self.event(
            a.stream,
            Event::FriendStat {
                user: a.friend.to_string(),
                mode,
            },
        );
        self.answer(a.stream, a.cx, Hr::OK, Answer::FriendAdd);
    }

    fn unfriend(
        &mut self,
        owner: &str,
        character: u64,
        user: &str,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) -> Hr {
        let (entry, remote) = match self.target(user) {
            Target::NoSuchUser => return Hr::NO_SUCH_USER,
            Target::Local(n) => (n, None),
            Target::Remote(a) => (a.to_string(), Some(a)),
        };
        if !self.listed(character, NetworkList::Friends, &entry) {
            return NOT_IN_LIST;
        }
        if let Err(e) = self.keep(store, character, owner, NetworkList::Friends, &entry, false) {
            tracing::warn!("friends service: the store: {e}");
            return FRIEND_REFUSED;
        }
        if let (Some(a), Some(f)) = (remote, fed) {
            f.friend_removed(owner, &a);
        }
        Hr::OK
    }

    fn ignore(
        &mut self,
        owner: &str,
        character: u64,
        user: &str,
        on: bool,
        store: &mut impl Store,
    ) -> Hr {
        let entry = match self.target(user) {
            Target::NoSuchUser => return Hr::NO_SUCH_USER,
            Target::Local(n) if on => match Self::local_entry(store, &n) {
                Some((id, name)) if id != character => name,
                _ => return Hr::NO_SUCH_USER,
            },
            Target::Local(n) => n,
            Target::Remote(a) => a.to_string(),
        };
        let listed = self.listed(character, NetworkList::Ignored, &entry);
        if on && listed {
            return ALREADY_IN_LIST;
        }
        if !on && !listed {
            return NOT_IN_LIST;
        }
        if on && self.count(character, NetworkList::Ignored) >= MAX_LISTED {
            // The client reads this code as the list being out of reach; the line says why.
            self.line(character, IGNORED_FULL);
            return Hr::NOT_ONLINE;
        }
        if let Err(e) = self.keep(store, character, owner, NetworkList::Ignored, &entry, on) {
            tracing::warn!("friends service: the store: {e}");
            return Hr::NOT_ONLINE;
        }
        Hr::OK
    }

    #[allow(clippy::too_many_arguments)]
    fn tell(
        &mut self,
        stream: StreamId,
        cx: Cx,
        teller: &crate::Here,
        user: &str,
        text: &str,
        extra: Option<Extra>,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        let hr = match self.target(user) {
            Target::NoSuchUser => Hr::NO_SUCH_USER,
            Target::Local(n) => self.tell_here(teller, &n, text, extra, store),
            Target::Remote(a) => {
                return self.tell_away(stream, cx, teller, &a, text, fed);
            }
        };
        self.answer(stream, cx, hr, Answer::SendToUserEx);
    }

    /// A tell to a character of this world.
    fn tell_here(
        &mut self,
        teller: &crate::Here,
        name: &str,
        text: &str,
        extra: Option<Extra>,
        store: &mut impl Store,
    ) -> Hr {
        let Some(to) = self.here.get(&name.to_lowercase()).cloned() else {
            return match Self::local_entry(store, name) {
                Some(_) => Hr::NOT_ONLINE,
                None => Hr::NO_SUCH_USER,
            };
        };
        let now = secs(self.now);
        let rate = Limits::DEFAULT.tells_per_character;
        if !self
            .sends
            .entry(teller.character)
            .or_insert_with(|| Bucket::new(rate, now))
            .take(now)
        {
            self.line(teller.character, TOO_QUICKLY);
            return Hr::DENIED;
        }
        // An ignored teller is shown what a delivered tell shows, and the tell goes nowhere.
        if self.ignores_local(to.character, &teller.name) {
            return Hr::OK;
        }
        let user = self.written_local(&teller.name);
        self.event(
            to.stream,
            Event::UTellEv {
                user,
                text: text.to_owned(),
                extra,
            },
        );
        if let Some(line) = away_line(&to.name, &to.state) {
            self.line(teller.character, line);
        }
        Hr::OK
    }

    /// A tell to another world's character, through the link: answered when it ends.
    fn tell_away(
        &mut self,
        stream: StreamId,
        cx: Cx,
        teller: &crate::Here,
        to: &Address,
        text: &str,
        fed: Option<&mut Federation>,
    ) {
        let Some(f) = fed else {
            self.line(teller.character, WORLD_OFF);
            self.answer(stream, cx, Hr::DENIED, Answer::SendToUserEx);
            return;
        };
        let Some(account) = self.streams.get(&stream).and_then(|s| s.account.clone()) else {
            return;
        };
        let tell = self.fresh_tell();
        self.waiting.insert(
            tell,
            Waiting {
                stream,
                account: account.id,
                cx,
                teller: teller.character,
            },
        );
        let now = secs(self.now);
        f.tell(self, now, tell, &teller.name, to, text);
    }

    /// A tell to another world ended: its request is answered, and its line shown.
    pub(crate) fn tell_done(&mut self, tell: u64, end: TellEnd) {
        let Some(w) = self.waiting.remove(&tell) else {
            return;
        };
        let bound = self
            .streams
            .get(&w.stream)
            .and_then(|s| s.account.as_ref())
            .is_some_and(|a| a.id == w.account);
        if let Some(line) = end.line {
            self.line(w.teller, line);
        }
        if bound {
            self.answer(w.stream, w.cx, tell_code(end.ended), Answer::SendToUserEx);
        }
    }
}
