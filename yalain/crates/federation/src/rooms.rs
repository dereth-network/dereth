//! The Global room, carried in one of this game's chat rooms, the world's own rooms mirrored into
//! Discord, both as the hub says, and a line a character's person says from elsewhere
//! (`docs/network/07-ac2-world-behaviour.md` section 7).
//!
//! - **Which room.** The link names a world's rooms `general`, `trade`, `lfg` and `roleplay`
//!   ([`dereth_network_protocol::rooms::OWN_ROOMS`]); this game's client has General, Trade and PvP
//!   for everyone ([`Room::of`]). The hub keeps which of them carries the Global room, tells the
//!   world at each welcome and whenever it changes (`rooms`), and the world keeps the last it was
//!   told. Until a hub has told it, the world carries the room its settings name
//!   ([`crate::Settings::global_room`]). A hub that holds none is sent that one, once on each link; a hub
//!   that holds one has the last word, and the setting is logged once if it differs.
//! - **Out.** A line a character on the network says in that room has reached the world's own
//!   players; [`Federation::room_said`] sends it on to the hub. A line in one of the world's own
//!   rooms the hub mirrors goes the same way, under its own name, and from there to Discord alone.
//! - **In.** A line from another world is kept for the host ([`Federation::take_heard`]), which
//!   shows it to the players in the room.
//! - **From anywhere.** `@dng <text>` is kept for the host ([`Federation::take_global_lines`]),
//!   which says it as a line the client said in the room.
//! - **Said from Discord.** The hub asks the world to say a line as one of its characters; the host
//!   looks the character up ([`Federation::take_says`]), and [`Federation::answer_say`] applies the
//!   network's rules and answers the hub.

use dereth_network_protocol::rooms::{
    held_back_line, too_new_line, too_quickly_line, KeptRooms, RoomsMessage, OFF, OWN_ROOMS,
    UNCARRIED_SECS,
};
use dereth_network_protocol::say::{marked, reason, SayMessage, SayOutcome};
use dereth_network_protocol::text::{sanitize_line, sanitize_name, MAX_TEXT_CHARS};
use dereth_network_protocol::wire::GLOBAL_ROOM;
use dereth_network_protocol::{Address, Message, Outcome};
use yalain_store::{Store, StoreResult};

use crate::{Federation, Friends, LinkState};

/// The world's record of its rooms as its hub last told them.
pub const RECORD: &str = "dereth-network-rooms";

/// The room a world carries the Global room in when its settings name none: General.
pub const DEFAULT_ROOM: &str = "general";

/// The key the answers owed to the hub's own requests wait under, among the answers owed to each
/// other world: no world's slug, which is letters, digits and hyphens.
const HUB: &str = "@hub";

/// A room every character of this game has, which can carry the network's lines. A kingdom's, an
/// allegiance's and a fellowship's room reach only their own members, so they cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Room {
    General,
    Trade,
    Pvp,
}

impl Room {
    /// The room a link's name for a world's room means here. `general` and `trade` are General and
    /// Trade. This game has no room for looking for company or for roleplay, so `lfg` and
    /// `roleplay` are PvP, the one room left that nobody needs: no recorded session sent or heard
    /// a line in it.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        match word {
            "general" => Some(Self::General),
            "trade" => Some(Self::Trade),
            "lfg" | "roleplay" => Some(Self::Pvp),
            _ => None,
        }
    }

    /// The room's name as players know it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Trade => "Trade",
            Self::Pvp => "PvP",
        }
    }
}

/// Who said a line the host is to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Speaker {
    /// A character of another world.
    Remote(Address),
    /// A character of this world, by name, whose person said it elsewhere.
    Own(String),
}

/// A line for the players in a room, which the host shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    pub room: Room,
    /// The name it shows, marked with where it was said if it was said elsewhere.
    pub name: String,
    pub text: String,
    pub speaker: Speaker,
}

/// A character's `@dng`, which the host says as a line the client said in the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalLine {
    pub character: u64,
    pub room: Room,
    pub text: String,
}

/// The hub's asking to say a line as one of this world's characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SayAsk {
    pub id: String,
    pub room: String,
    pub who: Address,
    pub text: String,
    pub via: String,
}

/// What the host knows of a character the hub asks to speak as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    pub id: u64,
    /// Its name, as the world writes it.
    pub name: String,
    /// Its account is banned now.
    pub banned: bool,
    pub gagged: bool,
    /// It is younger than the operator asks of one sending to the network.
    pub too_new: bool,
}

/// A line handed to the hub, not yet answered.
#[derive(Debug, Clone)]
pub(crate) struct PendingRoom {
    character: u64,
    /// The room it was said in, by its name on the link.
    room: String,
    /// When it was handed to the link; none while it waits for one.
    pub(crate) sent: Option<f64>,
    /// Its speaker is told nothing of how the hub took it: its person said it elsewhere.
    quiet: bool,
}

/// A room as the link names where the Global room is carried: one of the world's own, or `off`.
fn link_word(word: Option<&str>) -> &str {
    word.unwrap_or(OFF)
}

/// A room as players know it, or "no room".
fn shown(room: Option<Room>) -> &'static str {
    room.map_or("no room", Room::name)
}

impl Federation {
    /// The link's name for the room that carries the Global room here now: where the hub last said,
    /// else where the world's settings say; none carries no room.
    fn global_word(&self) -> Option<&str> {
        let setting = self.settings.global_room;
        match self.rooms.as_ref().and_then(|r| r.global.as_deref()) {
            Some(OFF) => None,
            Some(word) if Room::of(word).is_some() => Some(word),
            _ => setting,
        }
    }

    /// The room that carries the Global room here now.
    #[must_use]
    pub fn global_room(&self) -> Option<Room> {
        self.global_word().and_then(Room::of)
    }

    /// The world's own rooms whose lines go to the hub to be mirrored into Discord now, as the hub
    /// last said, by their names on the link: never the room carrying the Global room, and a room
    /// the hub names twice (`lfg` and `roleplay` are one room here) under the first.
    #[must_use]
    pub fn mirrored_rooms(&self) -> Vec<(String, Room)> {
        let global = self.global_room();
        let mut out: Vec<(String, Room)> = Vec::new();
        let told = self.rooms.iter().flat_map(|r| r.mirrored.iter());
        for word in told {
            if let Some(room) = Room::of(word) {
                if Some(room) != global && out.iter().all(|(_, r)| *r != room) {
                    out.push((word.clone(), room));
                }
            }
        }
        out
    }

    /// The name on the link of what `room` passes on to the hub: the Global room, or one of the
    /// world's own rooms the hub mirrors.
    fn carried(&self, room: Room) -> Option<String> {
        if self.global_room() == Some(room) {
            return Some(GLOBAL_ROOM.to_owned());
        }
        self.mirrored_rooms()
            .into_iter()
            .find(|(_, r)| *r == room)
            .map(|(word, _)| word)
    }

    /// Reads what the store keeps of the world's rooms.
    pub(crate) fn load_rooms(&mut self, store: &mut impl Store) -> StoreResult<()> {
        self.rooms = store.world_record(RECORD)?.and_then(|bytes| {
            let read = serde_json::from_slice(&bytes);
            read.map_err(|e| tracing::warn!("Dereth Network: the kept rooms do not read: {e}"))
                .ok()
        });
        Ok(())
    }

    /// Writes what the store keeps of the world's rooms, when it changed.
    pub(crate) fn keep_rooms(&mut self, store: &mut impl Store) -> StoreResult<()> {
        if let (true, Some(rooms)) = (self.rooms_dirty, &self.rooms) {
            let json = serde_json::to_vec(rooms).unwrap_or_default();
            store.set_world_record(RECORD, &json)?;
            self.rooms_dirty = false;
        }
        Ok(())
    }

    /// At each welcome: the Global room is joined if the hub offers it and the world carries it,
    /// then each of the world's own rooms the hub last said it mirrors, ahead of what was queued;
    /// what the hub said of them before is forgotten, since this hub may mirror them now, and the
    /// hub is to tell the world its rooms again.
    pub(crate) fn rooms_welcomed(&mut self, t: f64, offered: &[String]) {
        let queued: Vec<_> = self.outbox.drain(..).collect();
        if self.global_room().is_some() && offered.iter().any(|r| r == GLOBAL_ROOM) {
            self.send(
                t,
                Message::RoomJoin {
                    room: GLOBAL_ROOM.into(),
                    on: true,
                },
            );
        }
        self.uncarried.clear();
        self.rooms_told = false;
        self.seeded = false;
        for (room, _) in self.mirrored_rooms() {
            self.send(t, Message::RoomJoin { room, on: true });
        }
        self.outbox.extend(queued);
    }

    /// The link went: requests not yet sent are dropped, and an operator waiting for the hub's
    /// answer is told it will not come.
    pub(crate) fn rooms_link_down(&mut self, friends: &mut impl Friends) {
        self.requests.clear();
        self.says.clear();
        let asks: Vec<Option<u64>> = self.global_asks.drain().map(|(_, who)| who).collect();
        for who in asks {
            tell_operator(
                friends,
                who,
                "The link to the Dereth Network's hub went before it answered. Say @dn admin global again once it is back.",
            );
        }
    }

    /// One of the rooms messages from the hub: the world's rooms are applied; a request, which only
    /// a world sends, is ignored.
    pub(crate) fn on_rooms_message(&mut self, friends: &mut impl Friends, t: f64, m: RoomsMessage) {
        if let RoomsMessage::Setup {
            id,
            global,
            mirrored,
        } = m
        {
            self.on_rooms(friends, t, id.as_deref(), global.as_deref(), &mirrored);
        }
    }

    /// The hub's word on the world's rooms: the room that carries the Global room (none while the
    /// hub holds none) and the own rooms it mirrors. They hold at once and are kept: the rooms that
    /// stop being carried are left on the link, and those that start are joined. A hub that holds
    /// no room is sent the one the world carries the Global room in now, once on each link; a hub
    /// that holds one has the last word, and the settings are logged once if they differ. An
    /// operator's `@dn admin global` the message answers (`id`) is told where the room is now.
    fn on_rooms(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        id: Option<&str>,
        global: Option<&str>,
        mirrored: &[String],
    ) {
        let (global_before, mirrored_before) = (self.global_room(), self.mirrored_rooms());
        self.rooms_told = true;
        let kept = KeptRooms::told(self.rooms.as_ref(), global, mirrored);
        if self.rooms.as_ref() != Some(&kept) {
            self.rooms = Some(kept);
            self.rooms_dirty = true;
        }
        let (global_now, mirrored_now) = (self.global_room(), self.mirrored_rooms());
        if global_now != global_before {
            tracing::info!(
                "Dereth Network: the Global room is carried in {} now",
                shown(global_now)
            );
            if global_now.is_some() != global_before.is_some() {
                self.send(
                    t,
                    Message::RoomJoin {
                        room: GLOBAL_ROOM.into(),
                        on: global_now.is_some(),
                    },
                );
            }
        }
        for (room, _) in &mirrored_before {
            if !mirrored_now.iter().any(|(r, _)| r == room) {
                self.send(
                    t,
                    Message::RoomJoin {
                        room: room.clone(),
                        on: false,
                    },
                );
            }
        }
        for (room, _) in &mirrored_now {
            if !mirrored_before.iter().any(|(r, _)| r == room) {
                self.uncarried.remove(room);
                self.send(
                    t,
                    Message::RoomJoin {
                        room: room.clone(),
                        on: true,
                    },
                );
            }
        }
        match global {
            None if !self.seeded => {
                self.seeded = true;
                let room = link_word(self.global_word()).to_owned();
                tracing::info!(
                    "Dereth Network: the hub holds no room for this world's Global room; it is told {room}, as this world carries it now"
                );
                let id = self.fresh_id('g');
                self.requests.push_back(
                    RoomsMessage::Global {
                        id,
                        room,
                        seed: true,
                    }
                    .to_json(),
                );
            }
            Some(hub)
                if self.settings.global_room_named
                    && hub != link_word(self.settings.global_room)
                    && !self.settings_differ_logged =>
            {
                self.settings_differ_logged = true;
                tracing::info!(
                    "Dereth Network: the hub carries this world's Global room in {hub}, so federation.global_room ({}) is not read: it was read until the hub kept a room. Move the room with @dn admin global, or on the hub's operator page",
                    link_word(self.settings.global_room)
                );
            }
            _ => {}
        }
        if let Some(who) = id.and_then(|id| self.global_asks.remove(id)) {
            let line = format!(
                "The Dereth Network's hub carries this world's Global room in {} now.",
                shown(global_now)
            );
            tell_operator(friends, who, &line);
        }
    }

    /// Where this world carries the Global room, and whence it knows, as `@dn admin global` says it.
    fn global_status(&self) -> String {
        let room = shown(self.global_room());
        let from = if self.rooms.as_ref().is_some_and(|r| r.global.is_some()) {
            "as the Dereth Network's hub keeps it"
        } else {
            "as federation.global_room says, until the Dereth Network's hub keeps a room"
        };
        format!(
            "This world carries the Global room in {room}, {from}. Say @dn admin global <general|trade|lfg|roleplay|off> to move it."
        )
    }

    /// `@dn admin global [<room>]`: where this world carries the Global room, or asks the hub to
    /// carry it in `room` (or nowhere, `off`) from now on. The hub's answer goes to `who` (the
    /// console when none); the line returned is the immediate one. `usage` is the command's own
    /// help line, for a word that is no room.
    pub(crate) fn admin_global(
        &mut self,
        who: Option<u64>,
        args: &[&str],
        usage: &str,
    ) -> Vec<String> {
        let room = match args {
            [] => return vec![self.global_status()],
            [word] => word.to_ascii_lowercase(),
            _ => return vec![usage.to_owned()],
        };
        let room = match room.as_str() {
            OFF | "none" => OFF.to_owned(),
            word if OWN_ROOMS.contains(&word) => room,
            _ => return vec![usage.to_owned()],
        };
        if self.link != LinkState::Up {
            return vec![
                "The Dereth Network's hub can't be reached now. Try again once the link is up."
                    .to_owned(),
            ];
        }
        if !self.rooms_told {
            return vec![
                "This network's hub does not keep where a world carries the Global room: this world carries it as federation.global_room says."
                    .to_owned(),
            ];
        }
        let shown_as = shown(Room::of(&room));
        let id = self.fresh_id('g');
        self.requests.push_back(
            RoomsMessage::Global {
                id: id.clone(),
                room,
                seed: false,
            }
            .to_json(),
        );
        self.global_asks.insert(id, who);
        vec![format!(
            "Asking the Dereth Network's hub to carry this world's Global room in {shown_as}..."
        )]
    }

    /// Whether a local character's room line is within the hub's pace for it; takes one.
    fn may_say_line(&mut self, t: f64, name: &str) -> bool {
        let rate = self.limits.room_lines_per_character;
        self.room_sends
            .entry(name.to_lowercase())
            .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
            .take(t)
    }

    /// A line `name` (`character`) said in `room` at `t`, after it has reached the world's own
    /// players. It goes to the network when the speaker is on it and the room carries the Global
    /// room, or is one of the world's own rooms for Discord the hub has not lately said it does not
    /// carry.
    pub fn room_said(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        character: u64,
        name: &str,
        room: Room,
        text: &str,
    ) {
        if !self.enabled() {
            return;
        }
        let Some(word) = self.carried(room) else {
            return;
        };
        let Some(name) = self.on_name(name).map(str::to_owned) else {
            return;
        };
        if self.uncarried.get(&word).is_some_and(|&until| t < until) {
            return;
        }
        if self.too_new(&name) {
            friends.show_line(character, &too_new_line(&word));
            return;
        }
        let Some(from) = self.own_address(&name) else {
            return;
        };
        if !self.may_say_line(t, &name) {
            friends.show_line(character, &too_quickly_line(&word));
            return;
        }
        self.send_line(t, character, word, from, text, None);
    }

    /// Hands a line to the link, its answer awaited.
    fn send_line(
        &mut self,
        t: f64,
        character: u64,
        room: String,
        from: Address,
        text: &str,
        via: Option<String>,
    ) {
        let id = self.fresh_id('r');
        self.pending_rooms.insert(
            id.clone(),
            PendingRoom {
                character,
                room: room.clone(),
                sent: None,
                quiet: via.is_some(),
            },
        );
        self.send(
            t,
            Message::RoomSay {
                id,
                room,
                from,
                text: text.chars().take(MAX_TEXT_CHARS).collect(),
                via,
            },
        );
    }

    /// `@dng <text>`, checked by the caller: the host says it as a line the client said in `room`,
    /// and it comes back to [`Federation::room_said`] under every rule a line said there meets.
    pub fn say_global(&mut self, character: u64, room: Room, text: String) {
        self.global_lines.push(GlobalLine {
            character,
            room,
            text,
        });
    }

    /// The `@dng` lines for the host to say.
    pub fn take_global_lines(&mut self) -> Vec<GlobalLine> {
        std::mem::take(&mut self.global_lines)
    }

    /// The lines for the players in a room, for the host to show.
    pub fn take_heard(&mut self) -> Vec<Heard> {
        std::mem::take(&mut self.heard)
    }

    /// How the hub took one of this world's room lines: only a refusal is news to the speaker. A
    /// `not_member` for one of the world's own rooms says the hub does not mirror it into Discord
    /// now: the speaker is told nothing, and the room's lines stay here for [`UNCARRIED_SECS`].
    pub(crate) fn on_room_result(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        id: &str,
        outcome: Outcome,
    ) {
        let Some(p) = self.pending_rooms.remove(id) else {
            return;
        };
        let own = p.room != GLOBAL_ROOM;
        match outcome {
            Outcome::RateLimited if !p.quiet => {
                friends.show_line(p.character, &held_back_line(&p.room));
            }
            Outcome::NotMember if own => {
                if self
                    .uncarried
                    .insert(p.room.clone(), t + UNCARRIED_SECS)
                    .is_none()
                {
                    tracing::info!(
                        "Dereth Network: the hub does not mirror this world's room {} into Discord; its lines stay here until the world's operator mirrors it on the hub's operator page",
                        p.room
                    );
                }
            }
            Outcome::Ok if own => {
                self.uncarried.remove(&p.room);
            }
            _ => {}
        }
    }

    /// A line from another world's character in the Global room, kept for the world's players
    /// ([`Federation::take_heard`]). A line in one of the world's own rooms, which nothing comes
    /// back into, is never shown. A line its speaker's person said elsewhere (`via`) shows the
    /// speaker's name marked with where (`Ann@coldeve (Discord)`).
    pub(crate) fn on_room_line(
        &mut self,
        t: f64,
        room: &str,
        from: &Address,
        text: &str,
        via: Option<&str>,
    ) {
        let Some(carrying) = self.global_room() else {
            return;
        };
        if room != GLOBAL_ROOM || self.blocked(from) || from.world() == self.settings.slug {
            return;
        }
        let Ok(name) = sanitize_name(&from.to_string()) else {
            return;
        };
        let name = match via {
            Some(via) => marked(&name, via),
            None => name,
        };
        let text = sanitize_line(text);
        if text.is_empty() {
            return;
        }
        // One world's lines are held to the room's per-world pace here too.
        let rate = self.limits.room_lines_per_world;
        if !self.may_receive(t, from.world(), room, rate) {
            return;
        }
        self.heard.push(Heard {
            room: carrying,
            name,
            text,
            speaker: Speaker::Remote(from.clone()),
        });
    }

    /// One of the say messages from the hub: a say waits for the host to look its character up; an
    /// answer, which only a world sends, is ignored.
    pub(crate) fn on_say_message(&mut self, m: SayMessage) {
        let SayMessage::Ask {
            id,
            room,
            who,
            text,
            via,
        } = m
        else {
            return;
        };
        if self.link == LinkState::Up {
            self.says.push(SayAsk {
                id,
                room,
                who,
                text,
                via,
            });
        }
    }

    /// The hub's asks to say a line as one of this world's characters, for the host to look each
    /// character up and hand to [`Federation::answer_say`].
    pub fn take_says(&mut self) -> Vec<SayAsk> {
        std::mem::take(&mut self.says)
    }

    /// Says `ask`'s line as its character, as the host found it (none: no such character, or one
    /// deleted), under the rules a line the character says itself meets, and answers the hub. A
    /// line it says goes to the room's players ([`Federation::take_heard`]) and on to the hub, with
    /// the mark of where it was said. A say past the answers the hub is owed is not answered, and
    /// the hub tells the person so.
    pub fn answer_say(&mut self, t: f64, ask: &SayAsk, character: Option<Character>) {
        if self.link != LinkState::Up {
            return;
        }
        if !self.may_owe(HUB) {
            tracing::debug!(
                "Dereth Network: the hub is owed its share of answers; a say is not taken"
            );
            return;
        }
        let refused = self.say(t, ask, character).err();
        let answer = SayMessage::Result {
            id: ask.id.clone(),
            outcome: if refused.is_some() {
                SayOutcome::Refused
            } else {
                SayOutcome::Said
            },
            reason: refused.flatten().map(str::to_owned),
        };
        self.owe_frame(t, HUB, answer.to_json());
    }

    /// The reason a say is refused (none for one this version gives no word for), or the line said.
    fn say(
        &mut self,
        t: f64,
        ask: &SayAsk,
        character: Option<Character>,
    ) -> Result<(), Option<&'static str>> {
        let c = character
            .filter(|_| ask.who.world() == self.settings.slug)
            .ok_or(Some(reason::NO_CHARACTER))?;
        if !self.is_on(&c.name) {
            return Err(Some(reason::NOT_ON_NETWORK));
        }
        // A link holds only for the character that asked for its code: a person linked to an
        // earlier character of this name speaks for no later one.
        if !self.asked_link(c.id) {
            return Err(Some(reason::NOT_LINKED));
        }
        if c.banned {
            return Err(Some(reason::BANNED));
        }
        if c.gagged {
            return Err(Some(reason::GAGGED));
        }
        if c.too_new {
            return Err(Some(reason::TOO_NEW));
        }
        let room = self.carried_as(&ask.room).ok_or(Some(reason::ROOM_OFF))?;
        let text = sanitize_line(&ask.text);
        if text.is_empty() {
            return Err(None);
        }
        let Some(from) = self.own_address(&c.name) else {
            return Err(Some(reason::NO_CHARACTER));
        };
        if !self.may_say_line(t, &c.name) {
            return Err(Some(reason::RATE_LIMITED));
        }
        self.heard.push(Heard {
            room,
            name: marked(&c.name, &ask.via),
            text: text.clone(),
            speaker: Speaker::Own(c.name),
        });
        self.send_line(
            t,
            c.id,
            ask.room.clone(),
            from,
            &text,
            Some(ask.via.clone()),
        );
        Ok(())
    }

    /// The room that carries `word` here, as a say names it: the Global room's for `global`, or one
    /// of the world's own rooms the hub mirrors.
    fn carried_as(&self, word: &str) -> Option<Room> {
        if word == GLOBAL_ROOM {
            return self.global_room();
        }
        self.mirrored_rooms()
            .into_iter()
            .find(|(w, _)| w == word)
            .map(|(_, room)| room)
    }

    /// Writes the operator's status line's words on the rooms.
    pub(crate) fn rooms_status(&self) -> String {
        let mirrored: Vec<&str> = self
            .mirrored_rooms()
            .iter()
            .map(|(_, r)| r.name())
            .collect();
        format!(
            "The Global room is carried in {}. Rooms mirrored into Discord: {}.",
            shown(self.global_room()),
            if mirrored.is_empty() {
                "none".to_owned()
            } else {
                mirrored.join(", ")
            }
        )
    }

    /// The passage of time: answers not coming, and paces and waits that are over.
    pub(crate) fn rooms_tick(&mut self, t: f64, timeout: f64) {
        self.pending_rooms
            .retain(|_, p| p.sent.is_none_or(|s| t - s < timeout));
        self.room_sends.retain(|_, b| !b.full_at(t));
        self.uncarried.retain(|_, &mut until| t < until);
    }
}

/// A line for the operator who asked: in its character's chat window, or the log at the console.
fn tell_operator(friends: &mut impl Friends, who: Option<u64>, line: &str) {
    match who {
        Some(character) => friends.show_line(character, line),
        None => tracing::info!("Dereth Network: {line}"),
    }
}
