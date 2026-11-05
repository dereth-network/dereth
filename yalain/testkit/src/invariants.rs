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
