//! What each side of a replay waits for before it sends each recorded message.
//!
//! **The server's side** ([`ServerPlan`]). Every recorded server message, referral and world switch
//! is held back by a *gate*: the latest of these recorded before it,
//!
//! - a connection coming up, or
//! - a client message a server answers ([`GATING`]: the data iterations, entering the world, the
//!   scene drawn, logging off, creating and deleting a character), matched live by type and
//!   occurrence (the second live entry into the world is the second recorded one),
//!
//! and goes out its recorded delay after that gate fires live (scaled), never before the item
//! recorded ahead of it on the same connection. So the double never answers what the client has
//! not asked, keeps the recorded pacing inside each answer, and sends each connection's items in
//! the recorded order. Everything else the client sends is recorded and holds nothing back: a
//! recorded server sent its world updates whether or not the client moved.
//!
//! The data download is answered, not replayed: a recorded answer to a recorded request for the
//! same record is kept and sent when the client asks for that record; a request nothing recorded
//! answers is refused as the 2017 servers refused most.
//!
//! **A replaced connection.** The client keeps one connection per server: a new connection to a
//! world server it already has one to replaces the old one. What was recorded on the old one
//! after the referral to the new one is left out.
//!
//! **The player's side** ([`PlayerPlan`]). Every recorded client message waits until every server
//! message recorded before it has arrived, and goes out its recorded delay after the latest server
//! event before it. A test plays the player's own actions from it.
//!
//! Both relations point strictly backwards in the recording, so the two sides cannot wait on each
//! other.

use std::collections::HashMap;

use super::script::{Event, Msg, Script};

/// The client messages a server answers, which hold back what the recorded server sent after them.
pub const GATING: [u32; 6] = [
    DATA_HELD,
    PLAY_CHARACTER,
    SCENE_DRAWN,
    LOG_OFF,
    CREATE_CHARACTER,
    DELETE_CHARACTER,
];

/// The client's data iterations.
pub const DATA_HELD: u32 = 0x10038;
/// Entering the world with a character.
pub const PLAY_CHARACTER: u32 = 0x1002B;
/// The scene around the player is drawn.
pub const SCENE_DRAWN: u32 = 0x100D1;
/// Leaving the world.
pub const LOG_OFF: u32 = 0x10027;
/// Creating a character.
pub const CREATE_CHARACTER: u32 = 0x1002A;
/// Deleting a character.
pub const DELETE_CHARACTER: u32 = 0x10029;
/// A request for a data record: kind, id.
pub const DATA_REQUEST: u32 = 0x10034;
/// A data record: two words, kind, id, then the record.
pub const DATA_RECORD: u32 = 0x10035;
/// A refused data request: kind, id, error.
pub const DATA_REFUSED: u32 = 0x10036;

/// A data request's kind and id.
#[must_use]
pub fn data_request(raw: &[u8]) -> Option<(u32, u32)> {
    if super::script::message_type(raw) != DATA_REQUEST || raw.len() < 12 {
        return None;
    }
    Some((
        u32::from_le_bytes(raw[4..8].try_into().ok()?),
        u32::from_le_bytes(raw[8..12].try_into().ok()?),
    ))
}

/// The kind and id an answer to a data request is about.
#[must_use]
pub fn data_answer_key(raw: &[u8]) -> Option<(u32, u32)> {
    let at = match super::script::message_type(raw) {
        DATA_REFUSED => 4,
        DATA_RECORD => 12,
        _ => return None,
    };
    let b = raw.get(at..at + 8)?;
    Some((
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
    ))
}

/// The refusal the 2017 servers sent for most data requests: kind, id, error 1.
#[must_use]
pub fn data_refusal(kind: u32, id: u32) -> Vec<u8> {
    let mut m = Vec::with_capacity(16);
    m.extend_from_slice(&DATA_REFUSED.to_le_bytes());
    m.extend_from_slice(&kind.to_le_bytes());
    m.extend_from_slice(&id.to_le_bytes());
    m.extend_from_slice(&1u32.to_le_bytes());
    m
}

/// An event a server item waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gate {
    /// The connection came up.
    Connect(usize),
    /// The `nth` (from 0) client message of this type, on any connection.
    Client { mtype: u32, nth: usize },
}

/// A gate with its recorded time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateSpec {
    pub gate: Gate,
    pub t: f64,
}

/// What a server item sends.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Msg(Msg),
    /// A referral to the recorded connection `to`.
    Referral {
        to: usize,
    },
    /// The server switch that makes this connection the current world again.
    Switch,
    /// The server lets go of the connection.
    Goodbye,
    /// The server falls silent on the connection: it sends nothing more, not even
    /// acknowledgements, and the client lets go of it in its own time.
    Silence,
}

/// One thing the server sends, when its gate has fired and its delay has passed.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Recorded time.
    pub t: f64,
    /// Index into [`ServerPlan::gates`].
    pub gate: usize,
    /// Recorded seconds after the gate.
    pub delay: f64,
    pub kind: ItemKind,
}

/// Per connection: the recorded time of the referral to a later connection to the same server,
/// which replaced it.
#[must_use]
pub fn replaced_at(script: &Script) -> Vec<Option<f64>> {
    script
        .conns
        .iter()
        .map(|c| {
            let d = script.conns.iter().find(|d| {
                d.n > c.n && d.server_port & !1 == c.server_port & !1 && d.server_id == c.server_id
            })?;
            Some(
                script
                    .events
                    .iter()
                    .find_map(|e| match e {
                        Event::Referral {
                            t, to: Some(to), ..
                        } if *to == d.n => Some(*t),
                        _ => None,
                    })
                    .unwrap_or(d.t_request),
            )
        })
        .collect()
}

/// What both plans share: the gates, each event's gate, which server messages answer which data
/// requests, and which events a replaced connection leaves out.
struct Core {
    gates: Vec<GateSpec>,
    gate_of: Vec<(usize, f64)>,
    answers: Vec<Option<usize>>,
    within: Vec<bool>,
}

impl Core {
    fn new(script: &Script) -> Self {
        let replaced = replaced_at(script);
        let within: Vec<bool> = script
            .events
            .iter()
            .map(|e| replaced[e.conn()].is_none_or(|r| e.t() < r))
            .collect();
        let mut gates: Vec<GateSpec> = script
            .conns
            .iter()
            .map(|c| GateSpec {
                gate: Gate::Connect(c.n),
                t: c.t_connect,
            })
            .collect();
        let mut nth: HashMap<u32, usize> = HashMap::new();
        for (i, e) in script.events.iter().enumerate() {
            if !within[i] {
                continue;
            }
            if let Event::Msg { t, msg } = e {
                if msg.c2s && GATING.contains(&msg.mtype) {
                    let n = nth.entry(msg.mtype).or_default();
                    gates.push(GateSpec {
                        gate: Gate::Client {
                            mtype: msg.mtype,
                            nth: *n,
                        },
                        t: *t,
                    });
                    *n += 1;
                }
            }
        }
        gates.sort_by(|a, b| a.t.total_cmp(&b.t));
        let gate_of = script
            .events
            .iter()
            .map(|e| {
                let t = e.t();
                let g = gates.iter().rposition(|g| g.t <= t).unwrap_or(0);
                (g, (t - gates[g].t).max(0.0))
            })
            .collect();
        let mut pending: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        let mut answers = vec![None; script.events.len()];
        for (i, e) in script.events.iter().enumerate() {
            let Event::Msg { msg, .. } = e else { continue };
            if msg.c2s {
                if let Some(k) = data_request(&msg.raw) {
                    pending.entry(k).or_default().push(i);
                }
            } else {
                answers[i] = data_answer_key(&msg.raw)
                    .and_then(|k| pending.get_mut(&k))
                    .filter(|v| !v.is_empty())
                    .map(|v| v.remove(0));
            }
        }
        Core {
            gates,
            gate_of,
            answers,
            within,
        }
    }
}

/// The server's side of a script.
#[derive(Debug, Clone, Default)]
pub struct ServerPlan {
    pub gates: Vec<GateSpec>,
    /// Per recorded connection, in recorded order.
    pub items: Vec<Vec<Item>>,
    /// Recorded answers to recorded data requests, by kind and id, in recorded order.
    pub data_answers: HashMap<(u32, u32), Vec<Msg>>,
    /// Server messages left out because a later connection replaced theirs.
    pub cut: usize,
}

impl ServerPlan {
    /// Build the server's plan (see the module notes).
    #[must_use]
    pub fn new(script: &Script) -> Self {
        let core = Core::new(script);
        let mut plan = ServerPlan {
            items: vec![Vec::new(); script.conns.len()],
            ..Default::default()
        };
        for (i, e) in script.events.iter().enumerate() {
            let (gate, delay) = core.gate_of[i];
            let item = |kind| Item {
                t: e.t(),
                gate,
                delay,
                kind,
            };
            match e {
                Event::Msg { msg, .. } if !msg.c2s && core.answers[i].is_some() => {
                    if let Some(k) = data_answer_key(&msg.raw) {
                        plan.data_answers.entry(k).or_default().push(msg.clone());
                    }
                }
                Event::Msg { msg, .. } if !msg.c2s => {
                    if core.within[i] {
                        plan.items[msg.conn].push(item(ItemKind::Msg(msg.clone())));
                    } else {
                        plan.cut += 1;
                    }
                }
                Event::Referral {
                    from, to: Some(to), ..
                } if core.within[i] => {
                    plan.items[*from].push(item(ItemKind::Referral { to: *to }));
                }
                Event::Switch { conn, .. } if core.within[i] => {
                    plan.items[*conn].push(item(ItemKind::Switch));
                }
                Event::End {
                    conn, c2s: false, ..
                } if core.within[i] => {
                    plan.items[*conn].push(item(ItemKind::Goodbye));
                }
                Event::Silent { conn, .. } if core.within[i] => {
                    plan.items[*conn].push(item(ItemKind::Silence));
                }
                _ => {}
            }
        }
        plan.gates = core.gates;
        plan
    }
}

/// An event a recorded client message waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Connect(usize),
    /// Entry `idx` of [`PlayerPlan::expected`]`[conn]` has arrived.
    Server {
        conn: usize,
        idx: usize,
    },
}

/// One recorded client message.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientItem {
    pub t: f64,
    /// The latest server event before it: the delay counts from this.
    pub trigger: Trigger,
    pub delay: f64,
    pub msg: Msg,
}

/// The player's side of a script: every recorded client message with what it followed, and every
/// server message the client should receive.
#[derive(Debug, Clone, Default)]
pub struct PlayerPlan {
    /// In recorded order.
    pub items: Vec<ClientItem>,
    /// Per recorded connection, the server messages in recorded order, with whether each is a data
    /// answer (sent on request rather than on time).
    pub expected: Vec<Vec<(Msg, bool)>>,
}

impl PlayerPlan {
    #[must_use]
    pub fn new(script: &Script) -> Self {
        let core = Core::new(script);
        let mut plan = PlayerPlan {
            items: Vec::new(),
            expected: vec![Vec::new(); script.conns.len()],
        };
        let mut connects: Vec<(f64, usize)> =
            script.conns.iter().map(|c| (c.t_connect, c.n)).collect();
        connects.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut next_connect = 0;
        let mut last: Option<(f64, Trigger)> = None;
        for (i, e) in script.events.iter().enumerate() {
            let t = e.t();
            while next_connect < connects.len() && connects[next_connect].0 <= t {
                last = Some((
                    connects[next_connect].0,
                    Trigger::Connect(connects[next_connect].1),
                ));
                next_connect += 1;
            }
            if !core.within[i] {
                continue;
            }
            match e {
                Event::Msg { msg, .. } if !msg.c2s => {
                    let idx = plan.expected[msg.conn].len();
                    plan.expected[msg.conn].push((msg.clone(), core.answers[i].is_some()));
                    last = Some((
                        t,
                        Trigger::Server {
                            conn: msg.conn,
                            idx,
                        },
                    ));
                }
                Event::Msg { msg, .. } => {
                    let Some((te, trigger)) = last else { continue };
                    plan.items.push(ClientItem {
                        t,
                        trigger,
                        delay: (t - te).max(0.0),
                        msg: msg.clone(),
                    });
                }
                _ => {}
            }
        }
        plan
    }
}
