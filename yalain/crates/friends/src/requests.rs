//! The client's requests, each answered once, with the codes whose lines the client shows.
//!
//! | request | answered |
//! |---|---|
//! | `GetFriendsList` | every friend with its mode; held a moment for a character's first person view |
//! | `FriendAdd` | this world's: the character must exist; another world's: the character and the world on the network. Kept (at most [`MAX_LISTED`]), or, for a linked character, listed by the hub; the friend's state as an event first, then done |
//! | `FriendDelete` | forgotten, or for a person friend's row, its person friend removed at the hub; not in the list says so |
//! | `GetIgnoredList` | every ignored name |
//! | `IgnoredAdd`, `IgnoredDelete` | kept (at most [`MAX_LISTED`]) or forgotten |
//! | `SetLoc` | the character's state; its friends in the world are told its new mode, when it changed |
//! | `SendToUserEx` | this world's: shown to the character when present and not ignoring the teller; away and linked, through the link, as another world's; another world's: through the link, answered when it ends; from a gagged teller: denied, and it is told why |
//!
//! The stream answers the rest itself (the ticket's place, `Flags`, the reserved requests).

use std::time::Duration;

use auberean_friends::{Answer, Cx, Event, Extra, Hr, Mode, Request};
use dereth_network_protocol::limits::{Bucket, Limits, MAX_SQUELCHED_WORLDS};
use dereth_network_protocol::Address;
use yalain_federation::{Ended, Federation, State, TellEnd, AWAY_TEXT, DO_NOT_DISTURB_TEXT};
use yalain_federation::{NOT_ON, TOO_QUICKLY, WORLD_OFF};
use yalain_store::{NetworkList, Store};

use crate::names::Target;
use crate::{
    secs, Adding, ListWait, Service, StreamId, Waiting, ADD_WAIT, ALREADY_IN_LIST, FRIENDS_FULL,
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
        Ended::RateLimited
        | Ended::TooQuickly
        | Ended::TooNew
        | Ended::NotOn
        | Ended::WorldOff
        | Ended::NotMember => Hr::DENIED,
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
        mut fed: Option<&mut Federation>,
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
                let due = !self.hidden.contains(&h.character)
                    && fed_ref.is_some_and(|f| f.view_due(&h.name));
                if due {
                    self.lists_waiting.push(ListWait {
                        stream,
                        cx,
                        name: h.name.to_lowercase(),
                        until: self.now + ADD_WAIT,
                    });
                } else {
                    self.give_list(stream, cx, &h.name, fed);
                }
            }
            Request::GetIgnoredList => {
                let ignored = self
                    .lists
                    .get(&h.character)
                    .map(|l| {
                        l.ignored
                            .iter()
                            .filter(|e| !crate::names::is_world_entry(e))
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
                // Past its pace of the hub's requests a channel stays as it was, squelched or not.
                if let (Target::Channel(_), Some(f)) = (self.target(&user), fed.as_deref_mut()) {
                    if let Err(why) = f.may_leave_channel(crate::secs(self.now), &h.name) {
                        self.line(h.character, why);
                        self.answer(stream, cx, Hr::NOT_ONLINE, Answer::IgnoredAdd);
                        return;
                    }
                }
                let hr = self.ignore(&h.name, h.character, &user, true, store);
                // Squelching a channel's address leaves the channel.
                if let (Hr::OK, Target::Channel(a), Some(f)) =
                    (hr, self.target(&user), fed.as_deref_mut())
                {
                    f.leave_channel(crate::secs(self.now), h.character, &h.name, a.name());
                }
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
        mut fed: Option<&mut Federation>,
    ) -> Option<Hr> {
        let person_add = self.person_add(user, owner, fed.as_deref());
        if let (Some(a), Some(f)) = (person_add, fed.as_deref_mut()) {
            return match self.ask_person_friend(character, owner, &a, f) {
                Ok(()) => {
                    let key = (owner.to_lowercase(), a.key());
                    self.assumed_offline.remove(&key);
                    self.adding.insert(
                        key,
                        Adding {
                            stream,
                            cx,
                            friend: a,
                            until: self.now + ADD_WAIT,
                        },
                    );
                    None
                }
                Err(hr) => Some(hr),
            };
        }
        let (entry, remote) = match self.list_friend(character, owner, user, store, fed.as_deref())
        {
            Ok(listed) => listed,
            Err(hr) => return Some(hr),
        };
        // The friend's state first, then the answer, as the original service did. Another
        // world's friend is shown once, with the state its world gives: the client prints a line
        // for each state, so a guess followed by the truth would read as a false departure.
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

    /// The friend's address, when adding `user` is the hub's to do: another world's character, for
    /// one whose friends are its person's.
    fn person_add(&self, user: &str, owner: &str, fed: Option<&Federation>) -> Option<Address> {
        match self.target(user) {
            Target::Remote(a) if fed.is_some_and(|f| f.friends_are_persons(owner)) => Some(a),
            _ => None,
        }
    }

    /// Asks the hub to list a friend for a linked character. Refused here, the code to answer with;
    /// the character has been told why if its client would not say.
    fn ask_person_friend(
        &mut self,
        character: u64,
        owner: &str,
        friend: &Address,
        f: &mut Federation,
    ) -> Result<(), Hr> {
        if self.lists_friend(character, owner, friend, f) {
            return Err(ALREADY_IN_LIST);
        }
        if self.count(character, NetworkList::Friends) + f.person_friends(owner).len() >= MAX_LISTED
        {
            self.line(character, FRIENDS_FULL);
            return Err(FRIEND_REFUSED);
        }
        f.befriend(secs(self.now), character, owner, friend)
            .map_err(|why| {
                self.line(character, why);
                FRIEND_REFUSED
            })
    }

    /// Whether a character lists a friend on another world, kept or through a person friend.
    fn lists_friend(&self, character: u64, owner: &str, friend: &Address, f: &Federation) -> bool {
        self.listed(character, NetworkList::Friends, &friend.to_string())
            || f.person_handle(owner, friend).is_some()
    }

    /// Keeps a friend on a character's list, if it may be: the entry kept, and the friend's address
    /// when it is on another world. Refused, the code to answer with; the character has been told
    /// why if its client would not say.
    fn list_friend(
        &mut self,
        character: u64,
        owner: &str,
        user: &str,
        store: &mut impl Store,
        fed: Option<&Federation>,
    ) -> Result<(String, Option<Address>), Hr> {
        let (entry, remote) = match self.target(user) {
            Target::NoSuchUser | Target::Channel(_) => return Err(Hr::NO_SUCH_USER),
            Target::Local(n) => match Self::local_entry(store, &n) {
                Some((id, name)) if id != character => (name, None),
                _ => return Err(Hr::NO_SUCH_USER),
            },
            Target::Remote(a) => {
                match fed {
                    Some(f) if f.enabled() => {
                        if !f.is_on(owner) {
                            self.line(character, NOT_ON);
                            return Err(FRIEND_REFUSED);
                        }
                    }
                    _ => {
                        self.line(character, WORLD_OFF);
                        return Err(FRIEND_REFUSED);
                    }
                }
                (a.to_string(), Some(a))
            }
        };
        if self.listed(character, NetworkList::Friends, &entry) {
            return Err(ALREADY_IN_LIST);
        }
        let persons = fed.map_or(0, |f| f.person_friends(owner).len());
        if self.count(character, NetworkList::Friends) + persons >= MAX_LISTED {
            self.line(character, FRIENDS_FULL);
            return Err(FRIEND_REFUSED);
        }
        if let Err(e) = self.keep(store, character, owner, NetworkList::Friends, &entry, true) {
            tracing::warn!("friends service: the store: {e}");
            return Err(FRIEND_REFUSED);
        }
        Ok((entry, remote))
    }

    /// `@dn friends add`: as the client's own add, with no request to answer. A friend on another
    /// world being watched is shown when its world answers. For a linked character the hub lists the
    /// friend, and the line comes with its answer.
    pub(crate) fn friend_said(
        &mut self,
        character: u64,
        owner: &str,
        user: &str,
        store: &mut impl Store,
        f: &mut Federation,
    ) -> Vec<String> {
        if let Some(a) = self.person_add(user, owner, Some(f)) {
            return match self.ask_person_friend(character, owner, &a, f) {
                Err(ALREADY_IN_LIST) => {
                    vec!["That character is already in your friends list".to_owned()]
                }
                _ => Vec::new(),
            };
        }
        let (entry, remote) = match self.list_friend(character, owner, user, store, Some(f)) {
            Ok(listed) => listed,
            Err(Hr::NO_SUCH_USER) => return vec![Self::no_such_friend(self.target(user))],
            Err(ALREADY_IN_LIST) => {
                return vec!["That character is already in your friends list".to_owned()];
            }
            Err(_) => return Vec::new(),
        };
        let watched = remote
            .as_ref()
            .is_some_and(|a| f.friend_added(self, owner, a));
        let (shown, mode) = self.status_of(owner, &entry, Some(f));
        if !watched {
            if let Some(stream) = self.here.get(&owner.to_lowercase()).map(|h| h.stream) {
                self.event(
                    stream,
                    Event::FriendStat {
                        user: shown.clone(),
                        mode,
                    },
                );
            }
        }
        vec![format!("{shown} has been added to your friends list.")]
    }

    /// What `@dn friends add` answers for what cannot be listed.
    fn no_such_friend(target: Target) -> String {
        match target {
            Target::Channel(_) => "A channel is not a character; it cannot be a friend.".to_owned(),
            _ => "That character does not exist".to_owned(),
        }
    }

    /// `@dn friends remove`: as the client's own delete, with no request to answer.
    pub(crate) fn unfriend_said(
        &mut self,
        character: u64,
        owner: &str,
        user: &str,
        store: &mut impl Store,
        f: &mut Federation,
    ) -> Vec<String> {
        let shown = match self.target(user) {
            Target::Local(n) => self.written_local(&n),
            Target::Remote(a) | Target::Channel(a) => a.to_string(),
            Target::NoSuchUser => user.to_owned(),
        };
        match self.unfriend(owner, character, user, store, Some(f)) {
            Hr::OK => vec![format!("{shown} has been removed from your friends list.")],
            NOT_IN_LIST | Hr::NO_SUCH_USER => {
                vec!["That character is not in your friends list!".to_owned()]
            }
            _ => Vec::new(),
        }
    }

    /// `@dn squelch <world>` or `@dn unsquelch <world>`, `world` one that is not this one: the line
    /// to answer it with.
    pub(crate) fn squelch_world(
        &mut self,
        character: u64,
        owner: &str,
        world: &str,
        on: bool,
        store: &mut impl Store,
    ) -> String {
        let entry = crate::names::world_entry(world);
        let listed = self.listed(character, NetworkList::Ignored, &entry);
        if on == listed {
            return if on {
                format!("{world} is already squelched.")
            } else {
                format!("{world} is not squelched.")
            };
        }
        if on && self.count_worlds(character) >= MAX_SQUELCHED_WORLDS {
            return format!(
                "You squelch {MAX_SQUELCHED_WORLDS} worlds already. Unsquelch one first."
            );
        }
        if let Err(e) = self.keep(store, character, owner, NetworkList::Ignored, &entry, on) {
            tracing::warn!("friends service: the store: {e}");
            return "The Dereth Network can't keep that now. Try again later.".to_owned();
        }
        if on {
            format!("Every character of {world} has been squelched.")
        } else {
            format!("{world} has been unsquelched.")
        }
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
            Target::NoSuchUser | Target::Channel(_) => return Hr::NO_SUCH_USER,
            Target::Local(n) => (n, None),
            Target::Remote(a) => (a.to_string(), Some(a)),
        };
        let mut fed = fed;
        if let (Some(a), Some(f)) = (&remote, fed.as_deref_mut()) {
            if f.person_handle(owner, a).is_some() {
                f.unfriend_person(secs(self.now), character, owner, a);
                return Hr::OK;
            }
            // A row that went in play: the client still shows it, and done takes it off the page.
            if !self.listed(character, NetworkList::Friends, &entry) && f.take_gone_row(owner, a) {
                return Hr::OK;
            }
        }
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
            Target::Remote(a) | Target::Channel(a) => a.to_string(),
        };
        let listed = self.listed(character, NetworkList::Ignored, &entry);
        if on && listed {
            return ALREADY_IN_LIST;
        }
        if !on && !listed {
            return NOT_IN_LIST;
        }
        if on
            && self.count(character, NetworkList::Ignored) - self.count_worlds(character)
                >= MAX_LISTED
        {
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
        if self.gagged.contains(&teller.character) {
            self.line(teller.character, crate::GAGGED);
            self.answer(stream, cx, Hr::DENIED, Answer::SendToUserEx);
            return;
        }
        let hr = match self.target(user) {
            Target::NoSuchUser => Hr::NO_SUCH_USER,
            Target::Local(n) => {
                let away = fed
                    .as_deref()
                    .and_then(|f| f.away_linked(&*self, &teller.name, &n));
                if let Some(a) = away {
                    return self.tell_away(stream, cx, teller, &a, text, fed);
                }
                self.tell_here(teller, &n, text, extra, store)
            }
            Target::Remote(a) | Target::Channel(a) => {
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
        // A hidden character is answered for as one that is offline is.
        let to = self
            .here
            .get(&name.to_lowercase())
            .filter(|h| !self.hidden.contains(&h.character))
            .cloned();
        let Some(to) = to else {
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
                asked: Some((stream, account.id, cx)),
                teller: teller.character,
            },
        );
        let now = secs(self.now);
        f.tell(self, now, tell, &teller.name, to, text);
    }

    /// The game's own tell (`character` named by `name`, its teller) reached no player in the world:
    /// its teller has been told so. When its name is an away, linked character's, it goes on to the
    /// hub, so that it may reach its player's Discord. Nobody waits on the answer, which shows only
    /// its line, when it has one.
    pub fn unheard_tell(
        &mut self,
        now: Duration,
        character: u64,
        name: &str,
        tellee: &str,
        text: &str,
        fed: Option<&mut Federation>,
    ) {
        let Some(f) = fed else {
            return;
        };
        self.now = now;
        let Target::Local(n) = self.target(tellee) else {
            return;
        };
        let Some(to) = f.away_linked(&*self, name, &n) else {
            return;
        };
        let tell = self.fresh_tell();
        self.waiting.insert(
            tell,
            Waiting {
                asked: None,
                teller: character,
            },
        );
        f.tell(self, secs(now), tell, name, &to, text);
    }

    /// A tell to another world ended: its request is answered, and its line shown.
    pub(crate) fn tell_done(&mut self, tell: u64, end: TellEnd) {
        let Some(w) = self.waiting.remove(&tell) else {
            return;
        };
        if let Some(line) = end.line {
            self.line(w.teller, line);
        }
        let Some((stream, account, cx)) = w.asked else {
            return;
        };
        let bound = self
            .streams
            .get(&stream)
            .and_then(|s| s.account.as_ref())
            .is_some_and(|a| a.id == account);
        if bound {
            self.answer(stream, cx, tell_code(end.ended), Answer::SendToUserEx);
        }
    }
}
