//! Trade between players.
//!
//! **Asking**: the asker is walked to the other; on arrival both are told of the trade (each with
//! the other's name). A partner who is fighting refuses it (and the asker waits twenty seconds
//! before asking again); asking the one it already trades with is refused. The other says yes by
//! asking back, which opens the trade for both; no by closing it.
//!
//! **In the trade**: an item offered (from the pack, so many of it) is told to both, the other side
//! also made aware of the item; an item offered twice is refused (the refusal goes to the trade's
//! asker). An item taken back is told to both. Any change to the offers clears both sides'
//! acceptance. Accepting with what the trade holds tells both; an acceptance whose holdings differ
//! from the trade's is answered to its sender alone as the trade refreshed; one already given is
//! not answered. Declining tells both and takes back the decliner's acceptance.
//!
//! **No room**: when both have accepted and a side's pack has no room for every item it would
//! receive, both are told the trade failed, naming which side (the asker's want of room first);
//! nothing moves, the acceptances are cleared and the trade stays open.
//!
//! **An attuned item**: when both have accepted and either side offers an item bound to its holder,
//! both are told items are traded, its move refused, the trade reset and the trouble; nothing
//! moves, and the trade stays open with nothing offered.
//!
//! **Both accepted**: both are told items are traded; each item moves into the other's pack at its
//! first slot (a part of a stack as a new item, the rest staying), the move told to both and the
//! item contained for its receiver; the trade is reset and both told it is finished. The trade
//! stays open for more.
//!
//! **Closing**: by either side (told to both); by the server when the two are too far apart, when
//! one leaves the world, when one starts a fight.

use auberean_protocol::physics::ObjectOnly;
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::inventory::{self, Move};
use crate::social::{self, name_text, name_var, show, world_of, NOTICE, REFUSED};
use crate::state::Held;
use crate::{progress, session, Clock, Server};

/// The trade's calls.
pub mod function {
    /// The client's.
    pub const OPEN: u32 = 0x8215_002F;
    pub const OFFER: u32 = 0x8215_0015;
    pub const REVOKE: u32 = 0x8215_003F;
    pub const ACCEPT: u32 = 0x8215_0035;
    pub const DECLINE: u32 = 0x8215_0023;
    pub const CLOSE: u32 = 0x8215_003E;
    /// The server's.
    pub const REGISTERED: u32 = 0x8215_0046;
    pub const OPENED: u32 = 0x8215_004E;
    pub const OFFERED: u32 = 0x8215_0044;
    pub const REVOKED: u32 = 0x8215_0048;
    pub const ACCEPTED: u32 = 0x8215_0050;
    pub const DECLINED: u32 = 0x8215_004F;
    pub const REFRESHED: u32 = 0x8215_0056;
    pub const RESET: u32 = 0x8215_0052;
    pub const FAILED: u32 = 0x8215_0051;
    pub const CLOSED: u32 = 0x8215_0053;
}

/// Why the server closes a trade.
pub mod error {
    /// The trade's asker, or the other, has no room in its pack for what it would receive.
    pub const ASKER_NO_ROOM: u32 = 6014;
    pub const OTHER_NO_ROOM: u32 = 6053;
    pub const TOO_FAR: u32 = 6013;
    pub const LOGGED_OUT: u32 = 6016;
    pub const FIGHTING: u32 = 6024;
}

/// The script package of the trade's record as the client sends it.
pub const RECORD: u16 = 0x235;

/// A trade's move of an item.
pub const MOVE_TYPE: u32 = 36;
/// The visual quality naming an item's container, and an item's quantity.
pub const CONTAINER: u32 = 258;
pub const QUANTITY: u32 = 262;

/// How far apart the two may stand (centre to centre, meters), and how often it is looked at.
pub const MOST_APART: f32 = 6.0;
pub const LOOK_EVERY: f64 = 1.0;
/// How long a refused asker waits before asking again, in seconds.
pub const WAIT_AFTER_REFUSAL: f64 = 20.0;

/// Texts (in the trade's table).
pub mod text {
    pub const TABLE: u32 = 0x2500_05E5;
    pub const TRADING: (u32, u32) = (0x2CAF_D828, TABLE);
    pub const FINISHED: (u32, u32) = (0x5EF2_06B9, TABLE);
    pub const PROBLEM: (u32, u32) = (0xAF5F_E73E, TABLE);
    pub const FIGHTING: (u32, u32) = (0x0F4F_EB12, TABLE);
    pub const WAIT: (u32, u32) = (0x7A18_4D8F, TABLE);
    pub const ALREADY: (u32, u32) = (0xFD48_9ED8, TABLE);
    pub const OFFER_FAILED: (u32, u32) = (0x207E_D0DB, TABLE);
    pub const NAME: u32 = 0x9D06_6C50;
}

/// One trade between two players.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trade {
    /// The one who asked, and the one asked.
    pub master: u64,
    pub slave: u64,
    /// The one asked has said yes.
    pub open: bool,
    /// What each side offers: item, how many.
    pub master_items: Vec<(u64, u32)>,
    pub slave_items: Vec<(u64, u32)>,
    pub master_accepted: bool,
    pub slave_accepted: bool,
}

impl Trade {
    #[must_use]
    pub fn has(&self, id: u64) -> bool {
        self.master == id || self.slave == id
    }

    #[must_use]
    pub fn other(&self, id: u64) -> u64 {
        if self.master == id {
            self.slave
        } else {
            self.master
        }
    }

    fn items_mut(&mut self, id: u64) -> &mut Vec<(u64, u32)> {
        if self.master == id {
            &mut self.master_items
        } else {
            &mut self.slave_items
        }
    }

    fn items(&self, id: u64) -> &[(u64, u32)] {
        if self.master == id {
            &self.master_items
        } else {
            &self.slave_items
        }
    }

    fn accepted_mut(&mut self, id: u64) -> &mut bool {
        if self.master == id {
            &mut self.master_accepted
        } else {
            &mut self.slave_accepted
        }
    }
}

/// Every trade of the world, and when refused askers may ask again.
#[derive(Debug, Clone, Default)]
pub struct Trades {
    pub list: Vec<Trade>,
    pub wait_until: Vec<(u64, f64)>,
    /// When the distances were last looked at.
    pub last_look: f64,
}

impl Trades {
    #[must_use]
    pub fn of(&self, id: u64) -> Option<usize> {
        self.list.iter().position(|t| t.has(id))
    }
}

fn call(f: u32, args: &[Arg]) -> ServerMessage {
    session::call(f, args).expect("plain values pack")
}

fn to<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    if let Some(w) = world_of(s, id) {
        progress::private(net, w, m);
    }
}

fn both<S: Store>(s: &Server<S>, net: &mut ServerNet, t: &Trade, m: &ServerMessage) {
    to(s, net, t.master, m);
    to(s, net, t.slave, m);
}

fn ids(v: &[Value]) -> Vec<u64> {
    v.iter()
        .filter_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .collect()
}

fn closed(src: u64, err: u32) -> ServerMessage {
    call(function::CLOSED, &[Arg::Int(err), Arg::LongInt(src)])
}

/// A player asks another for a trade, or (asked already) says yes.
pub fn open<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(&target) = ids(v).first() else {
        s.unhandled.note("a trade asked for that does not read");
        return;
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let myname = social::name_of(s, world);
    if let Some(i) = s.trades.of(me) {
        let t = s.trades.list[i].clone();
        if t.slave == me && t.master == target && !t.open {
            s.trades.list[i].open = true;
            both(s, net, &t, &call(function::OPENED, &[]));
        } else {
            show(
                net,
                world,
                REFUSED,
                progress::text_with(text::ALREADY, vec![name_var(text::NAME, &myname)]),
            );
        }
        return;
    }
    let now = clock.local.0;
    s.trades.wait_until.retain(|(_, t)| *t > now);
    if s.trades.wait_until.iter().any(|(id, _)| *id == me) {
        show(
            net,
            world,
            REFUSED,
            progress::text_with(text::WAIT, vec![name_var(text::NAME, &myname)]),
        );
        return;
    }
    let Some(other) = crate::presence::player_for(s, world, target) else {
        s.unhandled
            .note("a trade asked of someone not in the world");
        return;
    };
    if other == world {
        return;
    }
    social::approach(s, net, world, other, social::Approach::Trade, clock);
}

/// The asker has reached the other: both are told of the trade, unless the other is fighting or
/// trading.
pub fn register<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    other: ConnId,
    clock: Clock,
) {
    let (Some(me), Some(them)) = (
        s.players.get(&world).map(|p| p.character),
        s.players.get(&other).map(|p| p.character),
    ) else {
        return;
    };
    let (myname, theirname) = (social::name_of(s, world), social::name_of(s, other));
    let fighting = s.players.get(&other).is_some_and(|p| p.attacking.is_some());
    if fighting || s.trades.of(them).is_some() || s.trades.of(me).is_some() {
        show(
            net,
            world,
            REFUSED,
            progress::text_with(text::FIGHTING, vec![name_var(text::NAME, &theirname)]),
        );
        s.trades
            .wait_until
            .push((me, clock.local.0 + WAIT_AFTER_REFUSAL));
        return;
    }
    s.trades.list.push(Trade {
        master: me,
        slave: them,
        ..Trade::default()
    });
    let told = |name: &str| {
        call(
            function::REGISTERED,
            &[
                crate::chat::text_arg(name_text(name)),
                Arg::LongInt(them),
                Arg::LongInt(me),
            ],
        )
    };
    progress::private(net, world, &told(&theirname));
    progress::private(net, other, &told(&myname));
}

/// End a trade: both told who ended it and why.
pub fn close_trade<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize, src: u64, err: u32) {
    let t = s.trades.list.remove(i);
    both(s, net, &t, &closed(src, err));
}

/// A player closes its trade (or says no to one asked of it).
pub fn close<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if let Some(i) = s.trades.of(me) {
        close_trade(s, net, i, me, 0);
    }
}

/// A player leaves the world: its trade is closed for the other.
pub fn left<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    if let Some(i) = s.trades.of(id) {
        let t = s.trades.list.remove(i);
        to(s, net, t.other(id), &closed(id, error::LOGGED_OUT));
    }
}

/// A player starts a fight: its trade is closed.
pub fn fighting<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if let Some(i) = s.trades.of(me) {
        close_trade(s, net, i, me, error::FIGHTING);
    }
}

/// Close the trades whose two have drawn too far apart (looked at about once a second).
pub fn look<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if clock.local.0 - s.trades.last_look < LOOK_EVERY {
        return;
    }
    s.trades.last_look = clock.local.0;
    let mut i = 0;
    while i < s.trades.list.len() {
        let t = &s.trades.list[i];
        let apart = match (world_of(s, t.master), world_of(s, t.slave)) {
            (Some(a), Some(b)) => match (s.players.get(&a), s.players.get(&b)) {
                (Some(pa), Some(pb)) => crate::presence::distance(&pa.position, &pb.position)
                    .is_none_or(|d| d > MOST_APART),
                _ => true,
            },
            _ => true,
        };
        // A trade asked for is not yet looked at: the asker is still walking.
        if apart && t.open {
            let master = t.master;
            close_trade(s, net, i, master, error::TOO_FAR);
        } else {
            i += 1;
        }
    }
}

fn clear_acceptance(t: &mut Trade) {
    t.master_accepted = false;
    t.slave_accepted = false;
}

/// A player offers so many of an item from its pack.
pub fn offer<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    // The values come last parameter first: how many, then the item.
    let (Some(Value::Int(n)), Some(&item)) = (v.first(), ids(v).first()) else {
        s.unhandled.note("an offer that does not read");
        return;
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.trades.of(me) else {
        return;
    };
    let held = s
        .players
        .get(&world)
        .and_then(|p| {
            p.play
                .items
                .iter()
                .find(|h| h.id == item && h.location == 0)
        })
        .copied();
    let Some(held) = held else {
        s.unhandled.note("an item offered that is not in the pack");
        return;
    };
    if !s.trades.list[i].open {
        return;
    }
    if s.trades.list[i].items(me).iter().any(|(x, _)| *x == item) {
        let master = s.trades.list[i].master;
        to(
            s,
            net,
            master,
            &crate::chat::show_text(REFUSED, progress::text(text::OFFER_FAILED)),
        );
        return;
    }
    let n = (*n).clamp(1, held.quantity.max(1));
    let t = &mut s.trades.list[i];
    t.items_mut(me).push((item, n));
    clear_acceptance(t);
    let t = t.clone();
    both(
        s,
        net,
        &t,
        &call(
            function::OFFERED,
            &[Arg::Int(n), Arg::LongInt(item), Arg::LongInt(me)],
        ),
    );
    // The other side is made aware of the item, in the offerer's pack.
    let other = t.other(me);
    if let Some(ow) = world_of(s, other) {
        let known = s
            .players
            .get(&ow)
            .is_some_and(|p| p.known.contains_key(&item));
        if !known {
            if let Some(th) = s.things.get(item) {
                let m = ServerMessage::CreateObject(inventory::item_create(th));
                net.send(ow, queue::EVENT, BlobKind::UNORDERED, m.encode());
            }
            if let Some(p) = s.players.get_mut(&ow) {
                p.known.insert(
                    item,
                    crate::presence::Seen {
                        position_stream: 0,
                        head_stream: 0,
                    },
                );
            }
        }
    }
}

/// A player takes an offered item back.
pub fn revoke<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(&item) = ids(v).first() else {
        return;
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.trades.of(me) else {
        return;
    };
    let t = &mut s.trades.list[i];
    let before = t.items(me).len();
    t.items_mut(me).retain(|(x, _)| *x != item);
    if t.items(me).len() == before {
        return;
    }
    clear_acceptance(t);
    let t = t.clone();
    both(
        s,
        net,
        &t,
        &call(function::REVOKED, &[Arg::LongInt(item), Arg::LongInt(me)]),
    );
}

/// What a trade record the client sends holds: each side's items with how many (none when it does
/// not read).
pub fn record_items(v: &[Value]) -> Option<(Offers, Offers)> {
    let g = v
        .iter()
        .find(|x| matches!(x.root(), Some(Node::Script { package, .. }) if *package == RECORD))?;
    let Some(Node::Script { slots, .. }) = g.root() else {
        return None;
    };
    let table = |slot: usize| -> Option<Vec<(u64, u32)>> {
        let r = slots.get(slot)?.1;
        match g.entry(r)? {
            Node::Native { bytes, .. } => {
                // A count (the table's size above it), then each item and how many.
                let header = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
                let n = (header & 0x00FF_FFFF) as usize;
                let body = bytes.get(4..)?;
                if body.len() != n * 12 {
                    return None;
                }
                let mut out: Vec<(u64, u32)> = body
                    .as_chunks::<12>()
                    .0
                    .iter()
                    .filter_map(|c| {
                        Some((
                            u64::from_le_bytes(c.get(..8)?.try_into().ok()?),
                            u32::from_le_bytes(c.get(8..12)?.try_into().ok()?),
                        ))
                    })
                    .collect();
                out.sort_unstable();
                Some(out)
            }
            _ => None,
        }
    };
    Some((table(8)?, table(0)?))
}

/// What one side offers: each item and how many.
pub type Offers = Vec<(u64, u32)>;

fn sorted(v: &[(u64, u32)]) -> Vec<(u64, u32)> {
    let mut v = v.to_vec();
    v.sort_unstable();
    v
}

/// A player accepts the trade as its record holds it.
pub fn accept<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.trades.of(me) else {
        return;
    };
    let t = s.trades.list[i].clone();
    if !t.open {
        return;
    }
    if (t.master == me && t.master_accepted) || (t.slave == me && t.slave_accepted) {
        return;
    }
    if let Some((master, slave)) = record_items(v) {
        if master != sorted(&t.master_items) || slave != sorted(&t.slave_items) {
            progress::private(net, world, &call(function::REFRESHED, &[Arg::LongInt(me)]));
            return;
        }
    }
    *s.trades.list[i].accepted_mut(me) = true;
    let t = s.trades.list[i].clone();
    both(s, net, &t, &call(function::ACCEPTED, &[Arg::LongInt(me)]));
    if t.master_accepted && t.slave_accepted {
        // What is no longer there to give is taken back, and each must look and accept again.
        if offers_gone(s, net, i) {
            return;
        }
        if let Some(why) = no_room(s, &t) {
            clear_acceptance(&mut s.trades.list[i]);
            both(s, net, &t, &call(function::FAILED, &[Arg::Int(why)]));
            return;
        }
        complete(s, net, i, clock);
    }
}

/// Take back what a side offered and no longer holds in its pack as many of (put down, sold, split
/// off, merged away since it was offered), told to both as a side taking it back is. Whether any was.
fn offers_gone<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize) -> bool {
    let t = s.trades.list[i].clone();
    let mut any = false;
    for (who, items) in [(t.master, &t.master_items), (t.slave, &t.slave_items)] {
        let Some(world) = world_of(s, who) else {
            continue;
        };
        for (item, n) in items {
            let held = s.players.get(&world).is_some_and(|p| {
                p.play
                    .items
                    .iter()
                    .any(|h| h.id == *item && h.location == 0 && h.quantity.max(1) >= *n)
            });
            if held {
                continue;
            }
            any = true;
            s.trades.list[i].items_mut(who).retain(|(x, _)| x != item);
            both(
                s,
                net,
                &t,
                &call(function::REVOKED, &[Arg::LongInt(*item), Arg::LongInt(who)]),
            );
        }
    }
    if any {
        clear_acceptance(&mut s.trades.list[i]);
    }
    any
}

/// Whether a side has no room for what it would receive (each item offered arrives as one item in
/// its pack): the asker first.
fn no_room<S: Store>(s: &Server<S>, t: &Trade) -> Option<u32> {
    let fits = |who: u64, incoming: usize| {
        s.players
            .values()
            .find(|p| p.character == who)
            .is_none_or(|p| inventory::fits_in_pack(&p.play.items, incoming))
    };
    if !fits(t.master, t.slave_items.len()) {
        return Some(error::ASKER_NO_ROOM);
    }
    if !fits(t.slave, t.master_items.len()) {
        return Some(error::OTHER_NO_ROOM);
    }
    None
}

/// A player declines: both told, and its acceptance taken back.
pub fn decline<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(i) = s.trades.of(me) else {
        return;
    };
    *s.trades.list[i].accepted_mut(me) = false;
    let t = s.trades.list[i].clone();
    both(s, net, &t, &call(function::DECLINED, &[Arg::LongInt(me)]));
}

/// Both have accepted: the items change hands, the trade is reset and stays open.
fn complete<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize, _clock: Clock) {
    let t = s.trades.list[i].clone();
    both(
        s,
        net,
        &t,
        &crate::chat::show_text(NOTICE, progress::text(text::TRADING)),
    );
    if refuse_attuned(s, net, i, &t) {
        return;
    }
    for (giver, items) in [
        (t.master, t.master_items.clone()),
        (t.slave, t.slave_items.clone()),
    ] {
        let receiver = t.other(giver);
        for (item, n) in items {
            give(s, net, &t, giver, receiver, item, n);
        }
    }
    let tr = &mut s.trades.list[i];
    tr.master_items.clear();
    tr.slave_items.clear();
    clear_acceptance(tr);
    both(s, net, &t, &call(function::RESET, &[]));
    both(
        s,
        net,
        &t,
        &crate::chat::show_text(NOTICE, progress::text(text::FINISHED)),
    );
    for id in [t.master, t.slave] {
        if let Some(w) = world_of(s, id) {
            crate::world::keep_player(s, w);
        }
    }
}

/// An attuned item among the offers stops the trade: its move refused, the first of them alone
/// told, the trade reset and nothing moved (2017: 5 trades, both sides, the master's offers first;
/// a second item in one of them told nothing). Returns whether it did.
fn refuse_attuned<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize, t: &Trade) -> bool {
    let first = [(t.master, &t.master_items), (t.slave, &t.slave_items)]
        .into_iter()
        .flat_map(|(giver, items)| items.iter().map(move |(item, n)| (giver, *item, *n)))
        .find(|(_, item, _)| {
            s.things
                .get(*item)
                .is_some_and(|x| inventory::attunable(&s.catalog, x.template))
        });
    let Some((giver, item, n)) = first else {
        return false;
    };
    let m = Move {
        move_type: MOVE_TYPE,
        item,
        quantity: n,
        from: giver,
        from_slot: -1,
        actual_from_slot: -1,
        target: t.other(giver),
        merge_slot: -1,
        all_taken: true,
        trade: true,
        no_move: true,
        no_anim: true,
        contain: true,
        grab: true,
        unlock: true,
        check_take: true,
        status: crate::drops::status::ATTUNED,
        ..Move::default()
    };
    both(s, net, t, &inventory::move_done(&m));
    let tr = &mut s.trades.list[i];
    tr.master_items.clear();
    tr.slave_items.clear();
    clear_acceptance(tr);
    both(s, net, t, &call(function::RESET, &[]));
    both(
        s,
        net,
        t,
        &crate::chat::show_text(REFUSED, progress::text(text::PROBLEM)),
    );
    true
}

/// One item (or a part of a stack) from one pack to the front of the other.
fn give<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    t: &Trade,
    giver: u64,
    receiver: u64,
    item: u64,
    n: u32,
) {
    let (Some(gw), Some(rw)) = (world_of(s, giver), world_of(s, receiver)) else {
        return;
    };
    let Some(held) = s
        .players
        .get(&gw)
        .and_then(|p| {
            p.play
                .items
                .iter()
                .find(|h| h.id == item && h.location == 0)
        })
        .copied()
    else {
        return;
    };
    let whole = n >= held.quantity;
    let template = held.template;
    let look = s.things.get(item).map_or(0, |x| x.description.look);
    let moved = if whole {
        if let Some(p) = s.players.get_mut(&gw) {
            p.play.items.retain(|h| h.id != item);
            inventory::close_up(&mut p.play.items);
        }
        item
    } else {
        let Some(new) = inventory::split_off(s, item, n) else {
            return;
        };
        let left = held.quantity - n;
        if let Some(p) = s.players.get_mut(&gw) {
            if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
                h.quantity = left;
            }
        }
        if let Some(x) = s.things.get_mut(item) {
            x.quantity = left;
        }
        let stamp = inventory::visual(s, item);
        let m = ServerMessage::UpdateIntVisual(VisualQuality {
            object: stamp,
            quality: QUANTITY,
            value: i32::try_from(left).unwrap_or(i32::MAX),
        });
        progress::private(net, gw, &m);
        new
    };
    // Into the receiver's pack at its first slot, the rest moved down; into the first bag with room
    // when the pack itself is full.
    let mut slot = 0;
    if let Some(p) = s.players.get_mut(&rw) {
        slot = inventory::room_for_one(&mut p.play.items);
        p.play.items.insert(
            0,
            Held {
                id: moved,
                template,
                slot,
                location: 0,
                quantity: n.min(held.quantity),
            },
        );
    }
    if let Some(x) = s.things.get_mut(moved) {
        x.container = Some((receiver, slot));
        x.at = None;
    }
    let m = Move {
        move_type: MOVE_TYPE,
        item,
        split_item: moved,
        quantity: n,
        from: giver,
        from_slot: -1,
        actual_from: giver,
        actual_from_slot: i32::try_from(held.slot).unwrap_or(-1),
        target: receiver,
        actual_target: receiver,
        target_slot: i32::try_from(slot).unwrap_or(0),
        actual_target_slot: i32::try_from(slot).unwrap_or(0),
        merge_slot: -1,
        all_taken: whole,
        trade: true,
        no_move: true,
        no_anim: true,
        contain: true,
        grab: true,
        unlock: true,
        check_take: true,
        template,
        look_entity: look,
        split_quantity: if held.quantity > 1 || !whole { n } else { 0 },
        ..Move::default()
    };
    both(s, net, t, &inventory::move_done(&m));
    let contain = ServerMessage::Contain(ObjectOnly(inventory::contain_stamp(s, moved)));
    net.send(rw, queue::EVENT, BlobKind::UNORDERED, contain.encode());
    if whole {
        for owner in [0, receiver] {
            let stamp = inventory::visual(s, moved);
            let m = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
                object: stamp,
                quality: CONTAINER,
                value: owner,
            });
            progress::private(net, rw, &m);
        }
    } else if let Some(x) = s.things.get(moved) {
        let m = ServerMessage::CreateObject(inventory::item_create(x));
        net.send(rw, queue::EVENT, BlobKind::UNORDERED, m.encode());
    }
    if let Some(p) = s.players.get_mut(&rw) {
        p.known.insert(
            moved,
            crate::presence::Seen {
                position_stream: 0,
                head_stream: 0,
            },
        );
    }
}
