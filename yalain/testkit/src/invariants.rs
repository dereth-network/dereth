//! The invariants a world connection's server messages keep, checked the same way on a recorded
//! session and on the server's answer to its replay:
//!
//! - **known object**: a message about an object names one the connection made (or its player)
//!   and has not told the client to forget since;
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

/// The invariants over one connection's messages, in the order the client takes them.
#[derive(Debug, Default)]
pub struct Checker {
    known: HashSet<u64>,
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
        *self
            .breaches
            .entry(format!("{what}: {}", kind(m)))
            .or_default() += 1;
    }

    /// Take the next message.
    pub fn see(&mut self, m: &ServerMessage) {
        match m {
            ServerMessage::CreateObject(o) => {
                let stamp = o.physics.instance_stamp;
                if let Some(&last) = self.instances.get(&o.id) {
                    if newer(last, stamp) {
                        self.breach("instance stamp older than before", m);
                    }
                }
                self.instances.insert(o.id, stamp);
                self.known.insert(o.id);
                return;
            }
            ServerMessage::PlayerObject(p) => {
                self.known.insert(p.id);
                return;
            }
            _ => {}
        }
        let Some(o) = object_named(m) else {
            return;
        };
        *self.checked.entry(kind(m)).or_default() += 1;
        if !self.known.contains(&o.id) {
            self.breach("about an object not made", m);
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
            self.known.remove(&o.id);
        }
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
#[derive(Debug, Default)]
pub struct VisualOrder {
    books: BTreeMap<(u64, u16), VisualBook>,
    /// How many messages came at a stamp the client had already passed, by kind: it drops them.
    pub dropped: BTreeMap<String, usize>,
    misrouted: Vec<String>,
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
            self.flush((o.id, o.physics.instance_stamp));
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
