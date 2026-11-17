//! The invariants a world connection's server messages keep, checked the same way on a recorded
//! session and on the server's answer to its replay:
//!
//! - **known object**: a message about an object names one the connection made (or its player)
//!   and has not told the client to forget since, taken as the client takes it: a message about an
//!   object it does not have waits for the object to be made, and is dropped once 25 s have passed
//!   since the last such message with no making; an object told to be forgotten goes before the
//!   client's next instant, so what comes in the same instant still finds it;
//! - **stamps**: per object and connection, the position stamp of each position is newer than the
//!   last one, and the teleport stamp never older; each making of an object carries an instance
//!   stamp no older than the last.
//!
//! Stamps are 16-bit and wrap: the larger of two is newer while they are less than half the circle
//! apart, and the smaller from there on, exactly half included.

use std::collections::{BTreeMap, HashMap, HashSet};

use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;

/// Is stamp `a` newer than `b`?
#[must_use]
pub const fn newer(a: u16, b: u16) -> bool {
    if a.abs_diff(b) < 0x8000 {
        b < a
    } else {
        a < b
    }
}

/// The object a message is about, with the stamps it names it with.
#[must_use]
pub fn object_named(m: &ServerMessage) -> Option<ObjectStamp> {
    use ServerMessage as M;
    Some(match m {
        M::PlayBehavior(e) => e.sender,
        M::StopBehavior(e) | M::DoMode(e) | M::MotionInterpChange(e) | M::EndLayer(e) => e.sender,
        M::StopFx(e) | M::GlobalModifier(e) | M::SetMode(e) | M::DoFxScript(e) => e.sender,
        M::MoveSlider(e) => e.sender,
        M::MoveTo(e) => e.sender,
        M::Position(p) | M::PositionCell(p) => p.sender,
        M::Parent(e) => e.sender,
        M::HoverHeight(e) | M::VelocityScale(e) | M::AccelerationScale(e) | M::JumpScale(e) => {
            e.sender
        }
        M::LookAt(e) => e.sender,
        M::Push(e) => e.sender,
        M::PushToward(e) => e.sender,
        M::LetGoBehavior(e) => e.sender,
        M::UpdateVisualDesc(e) => e.sender,
        M::DoFx(e) => e.sender,
        M::DoTargetedFx(e) => e.sender,
        M::TerrainChange(e) => e.sender,
        M::ClearTerrainChanges(e) | M::StopAllBehaviors(e) | M::StopMoveCancelsBehaviors(e) => {
            e.sender
        }
        M::DoStory(e) => e.sender,
        M::InitializeMissile(e) => e.sender,
        M::ObjectLeaves(e) => e.sender,
        M::ForgetObject(o) | M::Contain(o) => o.0,
        M::DeParent(e) => e.sender,
        M::TurnHead(e) => e.sender,
        M::CellObjectCall(c) | M::VisualObjectCall(c) => c.object,
        M::UpdateIntVisual(q) | M::UpdateBoolVisual(q) => q.object,
        M::UpdateFloatVisual(q) => q.object,
        M::UpdateStringVisual(q) => q.object,
        M::UpdateDataIdVisual(q) => q.object,
        M::UpdateInstanceIdVisual(q) => q.object,
        M::UpdatePositionVisual(q) => q.object,
        M::UpdateTimestampVisual(q) => q.object,
        M::UpdateStringInfoVisual(q) => q.object,
        M::UpdateLongIntVisual(q) => q.object,
        _ => return None,
    })
}

/// A short name of a message's kind, for tallies.
#[must_use]
pub fn kind(m: &ServerMessage) -> String {
    match m {
        ServerMessage::PlayerCall(c) => format!("call {:08X}", c.function.0),
        ServerMessage::CellObjectCall(c) | ServerMessage::VisualObjectCall(c) => {
            format!("object call {:08X}", c.call.function.0)
        }
        other => format!("{:#07x}", other.message_type()),
    }
}

/// How long the client keeps messages about an object it does not have, after the last of them
/// came, before it drops them for want of the object's making.
pub const WAITS_FOR_MAKING: f64 = 25.0;

/// The invariants over one connection's messages, in the order the client takes them, each with
/// the time it came.
#[derive(Debug, Default)]
pub struct Checker {
    known: HashSet<u64>,
    /// Told to be forgotten in the current instant: still there until the next.
    forgetting: HashSet<u64>,
    /// Per object not made: when the last message about it came, and the kinds of those waiting.
    waiting: HashMap<u64, (f64, Vec<String>)>,
    /// The time of the current instant.
    now: f64,
    /// The connection the next messages come on: each world server keeps its own stamps.
    connection: i64,
    /// Per connection and object: the last position stamp and teleport stamp.
    positions: HashMap<(i64, u64), (u16, u16)>,
    instances: HashMap<u64, u16>,
    /// How many messages of each kind were checked.
    pub checked: BTreeMap<String, usize>,
    /// Each breach, by what was breached and the message kind.
    pub breaches: BTreeMap<String, usize>,
}

impl Checker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The next messages come on another connection of the same client (a hand-off to another
    /// world server).
    pub fn on_connection(&mut self, conn: i64) {
        self.connection = conn;
    }

    fn breach(&mut self, what: &str, m: &ServerMessage) {
        self.breach_of(what, &kind(m));
    }

    fn breach_of(&mut self, what: &str, kind: &str) {
        *self.breaches.entry(format!("{what}: {kind}")).or_default() += 1;
    }

    /// A new instant at `t`: the objects forgotten before it go, and the messages that have waited
    /// too long for their object's making are dropped.
    fn next_instant(&mut self, t: f64) {
        for id in std::mem::take(&mut self.forgetting) {
            self.known.remove(&id);
        }
        let dropped: Vec<u64> = self
            .waiting
            .iter()
            .filter(|(_, (last, _))| t - last > WAITS_FOR_MAKING)
            .map(|(id, _)| *id)
            .collect();
        for id in dropped {
            if let Some((_, kinds)) = self.waiting.remove(&id) {
                for k in kinds {
                    self.breach_of("about an object not made", &k);
                }
            }
        }
        self.now = t;
    }

    /// An object is made: what waited for it is taken.
    fn made(&mut self, id: u64) {
        self.known.insert(id);
        self.forgetting.remove(&id);
        self.waiting.remove(&id);
    }

    /// Take the next message, which came at `t` (seconds, on any clock that only moves forward).
    pub fn see(&mut self, t: f64, m: &ServerMessage) {
        if t > self.now {
            self.next_instant(t);
        }
        match m {
            ServerMessage::CreateObject(o) => {
                let stamp = o.physics.instance_stamp;
                if let Some(&last) = self.instances.get(&o.id) {
                    if newer(last, stamp) {
                        self.breach("instance stamp older than before", m);
                    }
                }
                self.instances.insert(o.id, stamp);
                self.made(o.id);
                return;
            }
            ServerMessage::PlayerObject(p) => {
                self.made(p.id);
                return;
            }
            _ => {}
        }
        let Some(o) = object_named(m) else {
            return;
        };
        *self.checked.entry(kind(m)).or_default() += 1;
        if !self.known.contains(&o.id) {
            let w = self.waiting.entry(o.id).or_insert((t, Vec::new()));
            w.0 = t;
            w.1.push(kind(m));
        }
        if let ServerMessage::Position(p) | ServerMessage::PositionCell(p) = m {
            let key = (self.connection, o.id);
            if let Some(&(pos, tel)) = self.positions.get(&key) {
                if !newer(p.position_stamp, pos) {
                    self.breach("position stamp not newer", m);
                }
                if newer(tel, p.teleport_stamp) {
                    self.breach("teleport stamp older than before", m);
                }
            }
            self.positions
                .insert(key, (p.position_stamp, p.teleport_stamp));
        }
        if let ServerMessage::ForgetObject(_) = m {
            if self.known.contains(&o.id) {
                self.forgetting.insert(o.id);
            }
        }
    }

    /// The connection's messages are over: what still waits for a making never had one.
    pub fn finish(&mut self) {
        self.next_instant(f64::INFINITY);
    }

    /// How many breaches in all.
    #[must_use]
    pub fn breach_count(&self) -> usize {
        self.breaches.values().sum()
    }
}

/// The messages whose second stamp is not a visual one: a position's and a containing's own counts.
const fn stampless(m: &ServerMessage) -> bool {
    matches!(
        m,
        ServerMessage::Position(_)
            | ServerMessage::PositionCell(_)
            | ServerMessage::Contain(_)
            | ServerMessage::ForgetObject(_)
    )
}

/// A message waiting for a visual stamp that has not come: the client holds it, and all of that
/// object's later visual messages behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub object: u64,
    /// The last stamp the client took for the object.
    pub current: u16,
    /// The stamp the waiting message carries.
    pub stamp: u16,
    /// Its kind ([`kind`]).
    pub kind: String,
}

/// The connection's sequence a message was sent at, from its blob id.
const fn sequence(blob_id: u64) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let q = blob_id as u32;
    q
}

#[derive(Debug, Default)]
struct VisualBook {
    /// The connection has been told of the object; until then its messages wait for that.
    made: bool,
    current: u16,
    held: Vec<(u16, String)>,
}

/// One connection's visual messages taken as the client takes them: per object and incarnation,
/// each stamp must be the one after the last, and a message about an object not yet made waits for
/// it. Those still waiting for a making are not counted ([`Checker`] answers for those).
///
/// A making moves the object's stamp only forward, as for a client that still holds the object.
/// A client that has let the object go (it does so a while after the object leaves its view)
/// starts again from the making's stamp, so each making must carry a stamp at or after every
/// visual stamp sent about the object before it: [`Self::behind`] lists those that do not, taken in
/// the order they were sent, not the order they came.
#[derive(Debug, Default)]
pub struct VisualOrder {
    books: BTreeMap<(u64, u16), VisualBook>,
    /// How many messages came at a stamp the client had already passed, by kind: it drops them.
    pub dropped: BTreeMap<String, usize>,
    misrouted: Vec<String>,
    /// Per object and incarnation, the visual stamps sent and the makings, each with the
    /// connection's sequence it was sent at.
    sent: BTreeMap<(u64, u16), Sent>,
    behind: Vec<Behind>,
}

/// A making of an object at a stamp before one already sent about it: a client that had let the
/// object go waits for the stamps between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Behind {
    pub object: u64,
    /// The stamp the making carries.
    pub made_at: u16,
    /// The stamp sent before it.
    pub sent: u16,
}

#[derive(Debug, Default)]
struct Sent {
    visual: Vec<(u32, u16)>,
    made: Vec<(u32, u16)>,
}

impl VisualOrder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the next message on the world connection, with its blob id.
    pub fn see(&mut self, blob_id: u64, raw: &[u8]) {
        let read = ServerMessage::decode(raw).ok();
        if let Some(ServerMessage::CreateObject(o)) = &read {
            let book = self
                .books
                .entry((o.id, o.physics.instance_stamp))
                .or_default();
            let stamp = o.physics.visual_ordering_stamp;
            if !book.made || newer(stamp, book.current) {
                book.current = stamp;
            }
            book.made = true;
            let key = (o.id, o.physics.instance_stamp);
            let sent = self.sent.entry(key).or_default();
            if let Some(&(_, before)) = sent
                .visual
                .iter()
                .find(|(q, v)| *q < sequence(blob_id) && newer(*v, stamp))
            {
                self.behind.push(Behind {
                    object: o.id,
                    made_at: stamp,
                    sent: before,
                });
            }
            sent.made.push((sequence(blob_id), stamp));
            self.flush(key);
            return;
        }
        if let Some(ServerMessage::ForgetObject(o)) = &read {
            self.books.remove(&(o.0.id, o.0.instance_stamp));
            return;
        }
        let visual = (blob_id >> 56) & 0x1F == 2;
        if let Some(m) = read.as_ref().filter(|m| !stampless(m)) {
            let stamp = object_named(m).map(|o| o.other_stamp);
            // Private, its stamp counts for nothing, and the next one waits for it; visual with no
            // stamp, the client drops it.
            if (!visual && (blob_id >> 56) & 0x1F == 1 && stamp.is_some_and(|s| s != 0))
                || (visual && stamp == Some(0))
            {
                self.misrouted.push(kind(m));
            }
        }
        if !visual || raw.len() < 16 {
            return;
        }
        let word = |at: usize| u16::from_le_bytes([raw[at], raw[at + 1]]);
        let Ok(id) = <[u8; 8]>::try_from(&raw[4..12]) else {
            return;
        };
        let key = (u64::from_le_bytes(id), word(12));
        let name = read.as_ref().map_or_else(|| "unread".to_owned(), kind);
        let stamp = word(14);
        let sent = self.sent.entry(key).or_default();
        if let Some(&(_, made_at)) = sent
            .made
            .iter()
            .find(|(q, m)| sequence(blob_id) < *q && newer(stamp, *m))
        {
            self.behind.push(Behind {
                object: key.0,
                made_at,
                sent: stamp,
            });
        }
        sent.visual.push((sequence(blob_id), stamp));
        self.books
            .entry(key)
            .or_default()
            .held
            .push((word(14), name));
        self.flush(key);
    }

    fn flush(&mut self, key: (u64, u16)) {
        let Some(book) = self.books.get_mut(&key).filter(|b| b.made) else {
            return;
        };
        loop {
            let want = book.current.wrapping_add(1);
            if let Some(at) = book.held.iter().position(|(s, _)| *s == want) {
                book.held.remove(at);
                book.current = want;
            } else if let Some(at) = book.held.iter().position(|(s, _)| !newer(*s, want)) {
                let (_, name) = book.held.remove(at);
                *self.dropped.entry(name).or_default() += 1;
            } else {
                return;
            }
        }
    }

    /// The makings sent at a stamp before one already sent about the object.
    #[must_use]
    pub fn behind(&self) -> Vec<Behind> {
        self.behind.clone()
    }

    /// The messages that came as the wrong kind for what they carry.
    #[must_use]
    pub fn misrouted(&self) -> Vec<String> {
        self.misrouted.clone()
    }

    /// What is waiting now for a stamp that has not come.
    #[must_use]
    pub fn held(&self) -> Vec<Held> {
        self.books
            .iter()
            .filter(|(_, b)| b.made)
            .flat_map(|((object, _), b)| {
                b.held.iter().map(|(stamp, kind)| Held {
                    object: *object,
                    current: b.current,
                    stamp: *stamp,
                    kind: kind.clone(),
                })
            })
            .collect()
    }
}
