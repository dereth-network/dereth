//! Fellowships: made, recruited into, left, disbanded; the fellows' vitals, levels and places
//! kept up to date; experience shared.
//!
//! **Making one** (a name of letters, spaces, apostrophes and hyphens): the maker enters the
//! fellowship's chat channel, is sent the fellowship (itself its leader and only fellow) and told it
//! made it. A name with anything else is refused.
//!
//! **Recruiting**: a player who is not one is refused, as is one already in a fellowship or too
//! far to walk to; otherwise the leader is walked to the recruit, told it waits, and the recruit is
//! asked. Unanswered, the asking runs out after twenty seconds. Accepting: the recruit is told it
//! joined, sent the whole fellowship and enters the channel; every fellow already in it is sent the
//! new fellow and told it joined. Declining tells the leader.
//!
//! **Leaving** (or leaving the world, or being dismissed): the one leaving is cleared of the
//! fellowship, told it left and leaves the channel; every other fellow is told it left. A leader
//! leaving leaves the fellowship to its oldest fellow (each client works this out itself).
//! **Disbanding**: every fellow is cleared of it and told.
//!
//! **The fellows' updates**: each fellow is looked at every three and a half to seven and a half
//! seconds; what changed of its health, vigor, their greatest, its level and its cell is sent to
//! every fellow, itself included.
//!
//! **Shared experience**: in a fellowship that shares it, each part of a kill a fellow earns (its
//! own, or its pet's) pays each fellow within 160 m of it whose level is within five of the
//! leader's (or both 150 or more): the creature's experience by the fellow's own level, times a
//! share by how many fellows are of a level to share (1, 0.6, 0.45, 0.35, 0.3, 0.25, 0.214, 0.1875,
//! 0.1667, then 0.1), and for a group creature a factor by the fellowship's size, times the part's
//! share of the health taken, rounded to the nearest. Its focus is shared the same way to the
//! fellows within 100 m, without a group creature's factor. A share comes without a text (2017: 2,001
//! fellowship kills paid, none with a text; in fellowships of two of a level to share, 58 of 89
//! kills paid every part so within 1).

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject};
use yalain_net::{ConnId, LocalTime, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{hash, tag, Builder, Slot};
use crate::player::quality;
use crate::social::{self, name_text, name_var, show, world_of, NOTICE, REFUSED};
use crate::{progress, quals, session, Clock, Server, Timer};

/// The fellowship's calls.
pub mod function {
    /// The client's.
    pub const CREATE: u32 = 0x820C_001B;
    pub const DISBAND: u32 = 0x820C_0003;
    pub const LEAVE: u32 = 0x820C_001C;
    pub const RECRUIT: u32 = 0x820C_0015;
    pub const DISMISS: u32 = 0x820C_000E;
    pub const ACCEPT: u32 = 0x820C_000D;
    pub const DECLINE: u32 = 0x820C_001A;
    /// The server's.
    pub const UPDATE_FELLOWSHIP: u32 = 0x820C_002B;
    pub const CLEAR_FELLOWSHIP: u32 = 0x820C_0028;
    pub const UPDATE_FELLOW: u32 = 0x820C_0027;
    pub const CLEAR_FELLOW: u32 = 0x820C_0026;
    pub const REQUEST_RECRUITMENT: u32 = 0x820C_002E;
    pub const EXPIRE_RECRUITMENT: u32 = 0x820C_0031;
    pub const HEALTH: u32 = 0x820C_0030;
    pub const VIGOR: u32 = 0x820C_002D;
    pub const MAX_HEALTH: u32 = 0x820C_0033;
    pub const MAX_VIGOR: u32 = 0x820C_0021;
    pub const LEVEL: u32 = 0x820C_0032;
    pub const CELL: u32 = 0x820C_003C;
}

/// Script packages: the fellowship (8 slots), a fellow (5), a fellow's vitals (7).
pub const FELLOWSHIP: (u16, usize) = (0x1F0, 8);
pub const FELLOW: (u16, usize) = (0x1EE, 5);
pub const VITALS: (u16, usize) = (0x1EF, 7);
/// Native types: a string, a string reference, a table of 64-bit values to references.
const STRING: u16 = 0x76;
const TEXT: u16 = 0x6A;
const LONG_REF_HASH: u16 = 0x40;

/// The fellowship's flags: shares experience; how loot goes (by the looting method, 1 to 4).
pub mod flag {
    pub const SHARES_XP: u32 = 0x1;
}

/// The most fellows.
pub const MOST: usize = 9;
/// How long an asking waits for its answer, in seconds.
pub const ASKING_LASTS: f64 = 20.0;
/// How often a fellow is looked at: at least, and up to this much more, in seconds.
pub const CHECK_EVERY: f64 = 3.5;
pub const CHECK_SPREAD: f64 = 4.0;
/// The first chat room the fellowships' rooms are numbered from.
pub const FIRST_ROOM: u32 = 0x100;
/// How far from the killer a fellow shares a kill, in meters.
pub const SHARE_RANGE: f32 = 160.0;
/// The level gap a share allows, and the level from which every fellow shares.
pub const LEVEL_GAP: i32 = 5;
pub const HIGH_LEVEL: i32 = 150;

/// Texts: (string id, table).
pub mod text {
    pub const TABLE: u32 = 0x2500_04E2;
    pub const CREATED: (u32, u32) = (0x6FA3_556E, TABLE);
    pub const WAITING: (u32, u32) = (0x3568_94A1, TABLE);
    pub const YOU_JOIN: (u32, u32) = (0xF9F5_5E5A, TABLE);
    pub const JOINED: (u32, u32) = (0x89CB_54FF, TABLE);
    pub const HAS_JOINED: (u32, u32) = (0x26F1_33CC, TABLE);
    pub const YOU_LEAVE: (u32, u32) = (0xAE7A_E1DD, TABLE);
    pub const HAS_LEFT: (u32, u32) = (0x1918_3190, TABLE);
    pub const DISBANDED: (u32, u32) = (0x4C0F_644B, TABLE);
    pub const FELLOW_DIED: (u32, u32) = (0x79CF_AB19, TABLE);
    pub const ALREADY_IN: (u32, u32) = (0x7CCB_9E00, TABLE);
    pub const ONLY_PLAYERS: (u32, u32) = (0xDAAB_8624, TABLE);
    pub const NOT_FROM_HERE: (u32, u32) = (0xC6CD_3C60, TABLE);
    pub const BAD_NAME: (u32, u32) = (0x455E_192A, TABLE);
    pub const DECLINES: (u32, u32) = (0x6F25_C372, TABLE);
    /// The channel's lines, naming the channel.
    pub const CHANNEL_ENTERED: (u32, u32) = (0x6B5B_5AAD, 0x2500_0002);
    pub const CHANNEL_EXITED: (u32, u32) = (0x34C2_844D, 0x2500_0002);
    pub const CHANNEL: u32 = 0x937E_C8A5;
    pub const FELLOWSHIP_CHANNEL: (u32, u32) = (0x97FC_36B8, 0x2500_000B);
    /// A name.
    pub const NAME: u32 = 0x9D06_6C50;
}

/// What a fellow's update says of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vitals {
    pub health: u32,
    pub vigor: u32,
    pub max_health: u32,
    pub max_vigor: u32,
    pub level: u32,
    pub cell: u32,
}

/// One fellow.
#[derive(Debug, Clone, PartialEq)]
pub struct Fellow {
    pub id: u64,
    /// When it joined (game time): the oldest leads when the leader leaves.
    pub joined: f64,
    /// What its fellows were last sent of it.
    pub sent: Vitals,
    /// When it is next looked at.
    pub next_check: f64,
}

/// One fellowship.
#[derive(Debug, Clone, PartialEq)]
pub struct Fellowship {
    pub name: String,
    pub leader: u64,
    pub flags: u32,
    pub room: u32,
    /// Its fellows, in the order they joined.
    pub fellows: Vec<Fellow>,
}

impl Fellowship {
    #[must_use]
    pub fn shares_xp(&self) -> bool {
        self.flags & flag::SHARES_XP != 0
    }

    #[must_use]
    pub fn has(&self, id: u64) -> bool {
        self.fellows.iter().any(|f| f.id == id)
    }
}

/// An asking out: leader, recruit, and when it runs out (the transport's clock).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Asking {
    pub leader: u64,
    pub recruit: u64,
    pub until: f64,
}

/// Every fellowship of the world (none outlives its fellows' sessions).
#[derive(Debug, Clone, Default)]
pub struct Fellowships {
    pub list: Vec<Fellowship>,
    pub asking: Vec<Asking>,
    pub next_room: u32,
}

impl Fellowships {
    /// The index of the fellowship a character is in.
    #[must_use]
    pub fn of(&self, id: u64) -> Option<usize> {
        self.list.iter().position(|f| f.has(id))
    }

    /// The fellows of the fellowship whose chat room this is.
    #[must_use]
    pub fn room_members(&self, room: u32) -> Option<Vec<u64>> {
        self.list
            .iter()
            .find(|f| f.room == room)
            .map(|f| f.fellows.iter().map(|x| x.id).collect())
    }
}

/// The share of a kill's experience each fellow gets, by how many fellows are of a level to share.
#[must_use]
pub fn share(eligible: usize) -> f64 {
    match eligible {
        0 | 1 => 1.0,
        2 => 0.6,
        3 => 0.45,
        4 => 0.35,
        5 => 0.3,
        6 => 0.25,
        7 => 0.214,
        8 => 0.1875,
        9 => 0.1667,
        _ => 0.1,
    }
}

/// The factor a group creature's experience takes, by the fellowship's size.
#[must_use]
pub fn group_factor(size: usize) -> f64 {
    match size {
        0..=2 => 1.0,
        3 => 1.5,
        4 => 2.2,
        5 => 2.5,
        6 => 2.7,
        _ => 2.8,
    }
}

/// Is a fellow of a level to share, against its leader's?
#[must_use]
pub fn eligible(level: i32, leader: i32) -> bool {
    (level - leader).abs() <= LEVEL_GAP || (level >= HIGH_LEVEL && leader >= HIGH_LEVEL)
}

fn call(f: u32, args: &[Arg]) -> ServerMessage {
    session::call(f, args).expect("plain values pack")
}

fn send_to<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    if let Some(w) = world_of(s, id) {
        progress::private(net, w, m);
    }
}

fn show_to<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    id: u64,
    kind: u32,
    t: auberean_script::native_object::TextInfo,
) {
    if let Some(w) = world_of(s, id) {
        show(net, w, kind, t);
    }
}

fn channel(which: (u32, u32)) -> auberean_script::native_object::TextInfo {
    progress::text_with(
        which,
        vec![progress::nested(
            text::CHANNEL,
            progress::text(text::FELLOWSHIP_CHANNEL),
        )],
    )
}

/// A fellow's name as the world writes it.
fn fellow_name<S: Store>(s: &Server<S>, id: u64) -> String {
    world_of(s, id).map_or_else(String::new, |w| social::name_of(s, w))
}

/// A fellow's level as it stands.
fn level_of<S: Store>(s: &Server<S>, id: u64) -> i32 {
    world_of(s, id)
        .and_then(|w| s.players.get(&w))
        .map_or(1, |p| crate::examine::level_of(&p.qualities))
}

/// A fellow's vitals as they stand.
fn vitals_now<S: Store>(s: &Server<S>, id: u64) -> Option<Vitals> {
    let p = s.players.get(&world_of(s, id)?)?;
    let int = |k: u32| {
        quals::int(&p.qualities, k)
            .unwrap_or(0)
            .max(0)
            .cast_unsigned()
    };
    Some(Vitals {
        health: int(quality::HEALTH),
        vigor: int(quality::VIGOR),
        max_health: int(quality::HEALTH_MAX),
        max_vigor: int(quality::VIGOR_MAX),
        level: crate::examine::level_of(&p.qualities)
            .max(1)
            .cast_unsigned(),
        cell: p.position.cell,
    })
}

/// A fellow's record: when it joined, its level, its vitals as last sent, its name.
fn fellow_record(b: &mut Builder, f: &Fellow, name: &str) -> u32 {
    let me = b.reserve(FELLOW.0);
    let vitals = b.object(
        VITALS.0,
        &[
            Slot::Int(f.sent.max_health),
            Slot::Tagged(if f.sent.cell == 0 { tag::INT } else { 4 }, f.sent.cell),
            Slot::Int(f.sent.health),
            Slot::Int(0),
            Slot::Int(f.sent.max_vigor),
            Slot::Int(f.sent.vigor),
            Slot::Int(0),
        ],
    );
    let n = b.native(TEXT, NativeObject::TextInfo(name_text(name)));
    let ts = f.joined.to_bits();
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        me,
        &[
            Slot::Tagged(0x84, ts as u32),
            Slot::Tagged(tag::HIGH, (ts >> 32) as u32),
            Slot::Tagged(4, f.sent.level),
            Slot::Ref(Some(vitals)),
            Slot::Ref(Some(n)),
        ],
    );
    me
}

/// The whole fellowship, as the call sending it carries it.
fn fellowship_message<S: Store>(s: &Server<S>, i: usize) -> ServerMessage {
    let f = &s.fellowships.list[i];
    let mut b = Builder::new();
    let root = b.reserve(FELLOWSHIP.0);
    let table = b.next_id();
    // The table first, then each fellow (its record, vitals and name).
    let mut entries = Vec::new();
    let mut at = table + 1;
    for _ in &f.fellows {
        entries.push(at);
        at += 3;
    }
    let pairs: Vec<(Item, Item)> = f
        .fellows
        .iter()
        .zip(&entries)
        .map(|(x, r)| (Item::Long(x.id), Item::Ref(*r)))
        .collect();
    b.native(
        LONG_REF_HASH,
        hash(Elem::Long, Elem::Ref, session::TABLE_SIZE, &pairs),
    );
    for x in &f.fellows {
        fellow_record(&mut b, x, &fellow_name(s, x.id));
    }
    let name = b.native(STRING, NativeObject::Text(f.name.encode_utf16().collect()));
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        root,
        &[
            Slot::Long(0),
            Slot::Int(f.flags),
            Slot::Int(f.room),
            Slot::Ref(Some(table)),
            Slot::Tagged(0x84, f.leader as u32),
            Slot::Tagged(tag::HIGH, (f.leader >> 32) as u32),
            Slot::Ref(Some(name)),
        ],
    );
    call(function::UPDATE_FELLOWSHIP, &[Arg::Object(b.graph())])
}

/// One fellow, as the call sending it carries it.
fn fellow_message<S: Store>(s: &Server<S>, f: &Fellow) -> ServerMessage {
    let mut b = Builder::new();
    fellow_record(&mut b, f, &fellow_name(s, f.id));
    call(
        function::UPDATE_FELLOW,
        &[Arg::Object(b.graph()), Arg::LongInt(f.id)],
    )
}

fn string_of(v: &[Value]) -> Option<String> {
    v.iter().find_map(|x| match x.root() {
        Some(Node::Text(t)) => Some(t.clone()),
        _ => None,
    })
}

fn id_of(v: &[Value]) -> u64 {
    v.iter()
        .find_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .unwrap_or(0)
}

/// Is a fellowship's name made of letters, spaces, apostrophes and hyphens?
#[must_use]
pub fn name_ok(name: &str) -> bool {
    !name.trim().is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphabetic() || matches!(c, ' ' | '\'' | '-'))
}

/// A player makes a fellowship: its name, whether it is social (shares no experience), how loot
/// is shared.
pub fn create<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    // The values come last parameter first: the looting method, social, the name.
    let (Some(Value::Int(looting)), Some(Value::Int(social))) = (v.first(), v.get(1)) else {
        s.unhandled.note("a fellowship made that does not read");
        return;
    };
    let Some(name) = string_of(v) else {
        s.unhandled.note("a fellowship made without a name");
        return;
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if s.fellowships.of(me).is_some() {
        return;
    }
    if !name_ok(&name) {
        show(
            net,
            world,
            REFUSED,
            progress::text_with(text::BAD_NAME, vec![name_var(text::NAME, &name)]),
        );
        return;
    }
    let mut flags = if *social == 0 { flag::SHARES_XP } else { 0 };
    if (1..=4).contains(looting) {
        flags |= 1 << looting;
    }
    let room = FIRST_ROOM + s.fellowships.next_room;
    s.fellowships.next_room += 1;
    let now = net.game_time(clock.local);
    s.fellowships.list.push(Fellowship {
        name: name.clone(),
        leader: me,
        flags,
        room,
        fellows: vec![Fellow {
            id: me,
            joined: now,
            sent: Vitals::default(),
            next_check: clock.local.0 + CHECK_EVERY,
        }],
    });
    let i = s.fellowships.list.len() - 1;
    show(net, world, NOTICE, channel(text::CHANNEL_ENTERED));
    let m = fellowship_message(s, i);
    progress::private(net, world, &m);
    show(
        net,
        world,
        NOTICE,
        progress::text_with(text::CREATED, vec![name_var(text::NAME, &name)]),
    );
}

/// The leader asks a player in: refused when it is not a player, is in a fellowship already, or
/// is too far to walk to; else the leader is walked to it.
pub fn recruit<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let target = id_of(v);
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.fellowships.of(me) else {
        return;
    };
    if s.fellowships.list[i].leader != me || s.fellowships.list[i].fellows.len() >= MOST {
        return;
    }
    let Some(other) = crate::presence::player_for(s, world, target) else {
        show(net, world, REFUSED, progress::text(text::ONLY_PLAYERS));
        return;
    };
    if s.fellowships.of(target).is_some() {
        let name = social::name_of(s, other);
        show(
            net,
            world,
            REFUSED,
            progress::text_with(text::ALREADY_IN, vec![name_var(text::NAME, &name)]),
        );
        return;
    }
    if social::gap(s, world, other).is_none_or(|g| g > social::GIVE_UP) {
        show(net, world, REFUSED, progress::text(text::NOT_FROM_HERE));
        return;
    }
    social::approach(s, net, world, other, social::Approach::Recruit, clock);
}

/// The leader has reached its recruit: it is told it waits, and the recruit is asked.
pub fn ask_recruit<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    recruit: ConnId,
    clock: Clock,
) {
    let (Some(leader), Some(rid)) = (
        s.players.get(&world).map(|p| p.character),
        s.players.get(&recruit).map(|p| p.character),
    ) else {
        return;
    };
    let Some(i) = s.fellowships.of(leader) else {
        return;
    };
    let fname = s.fellowships.list[i].name.clone();
    let lname = social::name_of(s, world);
    show(net, world, NOTICE, progress::text(text::WAITING));
    s.fellowships.asking.retain(|a| a.recruit != rid);
    s.fellowships.asking.push(Asking {
        leader,
        recruit: rid,
        until: clock.local.0 + ASKING_LASTS,
    });
    let m = call(
        function::REQUEST_RECRUITMENT,
        &[
            crate::chat::text_arg(name_text(&fname)),
            crate::chat::text_arg(name_text(&lname)),
            Arg::LongInt(leader),
        ],
    );
    progress::private(net, recruit, &m);
    s.after(
        clock,
        ASKING_LASTS,
        Timer::Asking {
            leader,
            recruit: rid,
        },
    );
}

/// An asking's time is up: the recruit is told it ran out, if it is still unanswered.
pub fn asking_ends<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    leader: u64,
    recruit: u64,
    clock: Clock,
) {
    let Some(at) = s.fellowships.asking.iter().position(|a| {
        a.leader == leader && a.recruit == recruit && a.until <= clock.local.0 + 1e-6
    }) else {
        return;
    };
    s.fellowships.asking.remove(at);
    send_to(
        s,
        net,
        recruit,
        &call(function::EXPIRE_RECRUITMENT, &[Arg::LongInt(leader)]),
    );
}

/// The recruit takes the place: it joins, and every fellow is told.
pub fn accept<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let leader = id_of(v);
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(at) = s
        .fellowships
        .asking
        .iter()
        .position(|a| a.leader == leader && a.recruit == me)
    else {
        s.unhandled.note("a place taken that was not offered");
        return;
    };
    s.fellowships.asking.remove(at);
    if s.fellowships.of(me).is_some() {
        return;
    }
    let Some(i) = s.fellowships.of(leader) else {
        return;
    };
    if s.fellowships.list[i].fellows.len() >= MOST {
        return;
    }
    let now = net.game_time(clock.local);
    let cell = s.players.get(&world).map_or(0, |p| p.position.cell);
    let fellow = Fellow {
        id: me,
        joined: now,
        sent: Vitals {
            cell,
            ..Vitals::default()
        },
        next_check: clock.local.0 + CHECK_EVERY,
    };
    let others: Vec<u64> = s.fellowships.list[i].fellows.iter().map(|f| f.id).collect();
    s.fellowships.list[i].fellows.push(fellow.clone());
    let fname = s.fellowships.list[i].name.clone();
    show(net, world, NOTICE, progress::text(text::YOU_JOIN));
    let m = fellowship_message(s, i);
    progress::private(net, world, &m);
    show(
        net,
        world,
        NOTICE,
        progress::text_with(text::JOINED, vec![name_var(text::NAME, &fname)]),
    );
    show(net, world, NOTICE, channel(text::CHANNEL_ENTERED));
    let joined = fellow_message(s, &fellow);
    let my_name = social::name_of(s, world);
    for id in others {
        send_to(s, net, id, &joined);
        show_to(
            s,
            net,
            id,
            NOTICE,
            progress::text_with(text::HAS_JOINED, vec![name_var(text::NAME, &my_name)]),
        );
    }
}

/// The recruit turns the place down: the leader is told.
pub fn decline<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let leader = id_of(v);
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let before = s.fellowships.asking.len();
    s.fellowships
        .asking
        .retain(|a| !(a.leader == leader && a.recruit == me));
    if s.fellowships.asking.len() == before {
        return;
    }
    let name = social::name_of(s, world);
    show_to(
        s,
        net,
        leader,
        NOTICE,
        progress::text_with(text::DECLINES, vec![name_var(text::NAME, &name)]),
    );
}

/// A fellow leaves (by asking, by being dismissed, or by leaving the world): cleared and told; the
/// others told it left; the oldest fellow leads when the leader left.
pub fn leave<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(i) = s.fellowships.of(id) else {
        return;
    };
    let name = fellow_name(s, id);
    send_to(s, net, id, &call(function::CLEAR_FELLOWSHIP, &[]));
    show_to(s, net, id, NOTICE, progress::text(text::YOU_LEAVE));
    show_to(s, net, id, NOTICE, channel(text::CHANNEL_EXITED));
    let f = &mut s.fellowships.list[i];
    f.fellows.retain(|x| x.id != id);
    if f.fellows.is_empty() {
        s.fellowships.list.remove(i);
        return;
    }
    if f.leader == id {
        if let Some(oldest) = f
            .fellows
            .iter()
            .min_by(|a, b| a.joined.total_cmp(&b.joined))
            .map(|x| x.id)
        {
            f.leader = oldest;
        }
    }
    let others: Vec<u64> = f.fellows.iter().map(|x| x.id).collect();
    let cleared = call(function::CLEAR_FELLOW, &[Arg::LongInt(id)]);
    for o in others {
        send_to(s, net, o, &cleared);
        show_to(
            s,
            net,
            o,
            NOTICE,
            progress::text_with(text::HAS_LEFT, vec![name_var(text::NAME, &name)]),
        );
    }
}

/// A player asks to leave its fellowship.
pub fn leave_request<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    if let Some(id) = s.players.get(&world).map(|p| p.character) {
        leave(s, net, id);
    }
}

/// The leader sends a fellow out.
pub fn dismiss<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let target = id_of(v);
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.fellowships.of(me) else {
        return;
    };
    if s.fellowships.list[i].leader == me && target != me && s.fellowships.list[i].has(target) {
        leave(s, net, target);
    }
}

/// The leader breaks the fellowship up: every fellow cleared of it and told.
pub fn disband<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.fellowships.of(me) else {
        return;
    };
    if s.fellowships.list[i].leader != me {
        return;
    }
    let f = s.fellowships.list.remove(i);
    let cleared = call(function::CLEAR_FELLOWSHIP, &[]);
    for x in &f.fellows {
        send_to(s, net, x.id, &cleared);
        show_to(s, net, x.id, NOTICE, progress::text(text::DISBANDED));
        show_to(s, net, x.id, NOTICE, channel(text::CHANNEL_EXITED));
    }
}

/// A fellow has died: its fellows are told.
pub fn died<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(i) = s.fellowships.of(id) else {
        return;
    };
    let name = fellow_name(s, id);
    let others: Vec<u64> = s.fellowships.list[i]
        .fellows
        .iter()
        .map(|x| x.id)
        .filter(|x| *x != id)
        .collect();
    for o in others {
        show_to(
            s,
            net,
            o,
            NOTICE,
            progress::text_with(text::FELLOW_DIED, vec![name_var(text::NAME, &name)]),
        );
    }
}

/// Look at the fellows whose time has come: what changed of each goes to every fellow.
pub fn tick<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: LocalTime) {
    for i in 0..s.fellowships.list.len() {
        let ids: Vec<u64> = s.fellowships.list[i].fellows.iter().map(|f| f.id).collect();
        for (k, id) in ids.iter().enumerate() {
            if s.fellowships.list[i].fellows[k].next_check > clock.0 {
                continue;
            }
            let spread = f64::from(s.rng.unit()) * CHECK_SPREAD;
            s.fellowships.list[i].fellows[k].next_check = clock.0 + CHECK_EVERY + spread;
            let Some(now) = vitals_now(s, *id) else {
                continue;
            };
            let was = s.fellowships.list[i].fellows[k].sent;
            if now == was {
                continue;
            }
            s.fellowships.list[i].fellows[k].sent = now;
            // In 2017's order: level, greatest health, health, greatest vigor, vigor, place.
            let mut out = Vec::new();
            if now.level != was.level {
                out.push(call(
                    function::LEVEL,
                    &[Arg::Int(now.level), Arg::LongInt(*id)],
                ));
            }
            if now.max_health != was.max_health {
                out.push(call(
                    function::MAX_HEALTH,
                    &[Arg::Int(now.max_health), Arg::LongInt(*id)],
                ));
            }
            if now.health != was.health {
                out.push(call(
                    function::HEALTH,
                    &[Arg::Int(0), Arg::Int(now.health), Arg::LongInt(*id)],
                ));
            }
            if now.max_vigor != was.max_vigor {
                out.push(call(
                    function::MAX_VIGOR,
                    &[Arg::Int(now.max_vigor), Arg::LongInt(*id)],
                ));
            }
            if now.vigor != was.vigor {
                out.push(call(
                    function::VIGOR,
                    &[Arg::Int(0), Arg::Int(now.vigor), Arg::LongInt(*id)],
                ));
            }
            if now.cell != was.cell {
                out.push(call(
                    function::CELL,
                    &[Arg::Int(now.cell), Arg::LongInt(*id)],
                ));
            }
            for to in &ids {
                for m in &out {
                    send_to(s, net, *to, m);
                }
            }
        }
    }
}

/// A fellow's part of a kill's focus (the health it took of all taken), in a fellowship that
/// shares experience: each fellow of a level to share within 100 m of the one whose part it is gets
/// the creature's death focus times the share for how many are of a level to share (never a group
/// creature's factor) times the part's share, rounded to the nearest; by world connection. The part
/// of a fellow not of a level to share is its own whole.
#[must_use]
pub fn focus_shares<S: Store>(
    s: &Server<S>,
    killer: u64,
    death_focus: i32,
    (taken, all): (i64, i64),
) -> Vec<(ConnId, i32)> {
    let Some(f) = s.fellowships.of(killer).map(|i| &s.fellowships.list[i]) else {
        return Vec::new();
    };
    let Some(kw) = world_of(s, killer) else {
        return Vec::new();
    };
    if death_focus <= 0 || !f.shares_xp() {
        return Vec::new();
    }
    let leader = level_of(s, f.leader);
    if !eligible(level_of(s, killer), leader) {
        let whole = crate::xp::part(i64::from(death_focus), taken, all);
        return vec![(kw, i32::try_from(whole).unwrap_or(i32::MAX))];
    }
    let levels: Vec<(u64, i32)> = f
        .fellows
        .iter()
        .map(|x| (x.id, level_of(s, x.id)))
        .collect();
    let mult = share(levels.iter().filter(|(_, l)| eligible(*l, leader)).count());
    let mut out = Vec::new();
    for (id, level) in levels {
        if !eligible(level, leader) {
            continue;
        }
        let Some(w) = world_of(s, id) else {
            continue;
        };
        let near = w == kw
            || match (s.players.get(&w), s.players.get(&kw)) {
                (Some(a), Some(b)) => crate::presence::distance(&a.position, &b.position)
                    .is_some_and(|d| d <= crate::focus::SHARE_RANGE),
                _ => false,
            };
        if !near {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let amount = dereth_primitives::num::to_i32_f64(
            (f64::from(death_focus) * mult * taken as f64 / all.max(1) as f64 + 0.5).floor(),
        )
        .max(0);
        out.push((w, amount));
    }
    out
}

/// A fellow's part of a kill (the health it took of all taken), in a fellowship that shares
/// experience: each fellow's share, by world connection, rounded to the nearest (none when the
/// fellow is not in such a fellowship). The share goes by how many fellows are of a level to share,
/// near or not, and evenly: each gets the creature's experience by its own level times that share
/// (and a group creature's factor), times the part's share of the health taken; only those within
/// 160 m of the fellow whose part it is are paid. The part of a fellow not of a level to share is
/// paid to it alone, whole (with a group creature's factor), and to no other.
pub fn shares<S: Store>(
    s: &Server<S>,
    killer: u64,
    death_xp: i64,
    creature_level: i32,
    group: bool,
    (taken, all): (i64, i64),
) -> Option<Vec<(ConnId, u64)>> {
    let i = s.fellowships.of(killer)?;
    let f = &s.fellowships.list[i];
    if !f.shares_xp() || f.fellows.len() < 2 {
        return None;
    }
    let leader = level_of(s, f.leader);
    let levels: Vec<(u64, i32)> = f
        .fellows
        .iter()
        .map(|x| (x.id, level_of(s, x.id)))
        .collect();
    let n = levels.iter().filter(|(_, l)| eligible(*l, leader)).count();
    let factor = if group {
        group_factor(f.fellows.len())
    } else {
        1.0
    };
    let kw = world_of(s, killer)?;
    let own = level_of(s, killer);
    if !eligible(own, leader) {
        let base = crate::xp::kill_award(death_xp, creature_level, own);
        return Some(vec![(kw, share_of(base, factor, (taken, all)))]);
    }
    let mult = share(n) * factor;
    let mut out = Vec::new();
    for (id, level) in levels {
        if !eligible(level, leader) {
            continue;
        }
        let Some(w) = world_of(s, id) else {
            continue;
        };
        let near = w == kw
            || match (s.players.get(&w), s.players.get(&kw)) {
                (Some(a), Some(b)) => crate::presence::distance(&a.position, &b.position)
                    .is_some_and(|d| d <= SHARE_RANGE),
                _ => false,
            };
        if !near {
            continue;
        }
        let base = crate::xp::kill_award(death_xp, creature_level, level);
        out.push((w, share_of(base, mult, (taken, all))));
    }
    Some(out)
}

/// An award times a factor times a part's share of the health taken, rounded to the nearest.
fn share_of(base: i64, mult: f64, (taken, all): (i64, i64)) -> u64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let amount = (base as f64 * mult * taken as f64 / all.max(1) as f64 + 0.5)
        .floor()
        .max(0.0) as u64;
    amount
}
