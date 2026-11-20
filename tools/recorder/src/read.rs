//! The reader: a recording taken apart into its messages, each named, and the ones that name one
//! object.
//!
//! A recording is read as the capture index reads any capture (`dereth-pcap`): frames to UDP
//! datagrams, datagrams to sessions, sessions through the transport into whole messages, each
//! filed under its game. Then each message is named by its own game's protocol crate: a first-game
//! message by the master opcode table and its decoded fields; a second-game message by its type
//! (`auberean-protocol`) and the object it is about (`auberean-client-model`).
//!
//! # Which messages name an object
//!
//! A message names an object when its decoded fields do (a first-game message's object ids, with
//! the field each is in; a second-game message's subject, or the object a `CreateObject` makes),
//! and also, marked apart, when the object's id is anywhere in its bytes: a second-game message
//! that names the object other than as its subject (a target, a parent) is found that way, as is
//! a message that does not decode.
//!
//! # Which ports are a server's
//!
//! The session router needs the server's ports. They are read from the recording: a port a
//! `LoginRequest` or `WorldLoginRequest` was sent to, and a port a `ConnectRequest` came from, each
//! with the port above it.

use std::collections::BTreeMap;
use std::io::Read;
use std::net::SocketAddrV4;
use std::path::Path;

use auberean_protocol::{ClientMessage, ServerMessage, SECURE_QUEUES};
use dereth_pcap::decode::{type_name, Status};
use dereth_pcap::flows::{Dir, Flows, PortSet};
use dereth_pcap::link::{Datagram, Link};
use dereth_pcap::pcap::CaptureReader;
use dereth_pcap::transport::{MessageRow, Session, VecSink};
use dereth_transport::PacketFlags;

/// Which game a message is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    /// Asheron's Call.
    Dereth,
    /// Asheron's Call 2.
    Auberean,
}

/// One message of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Seconds since the recording's first datagram, at the datagram that completed it.
    pub t: f64,
    /// The session it travelled in, numbered from 1.
    pub session: usize,
    pub dir: Dir,
    pub game: Game,
    /// The queue it travelled on.
    pub queue: u16,
    /// Its type: a first-game message's wrapped type when it has one.
    pub mtype: u32,
    /// Its name, or `unknown`.
    pub name: &'static str,
    /// Whether its game's protocol read it.
    pub read: bool,
    /// The object ids its decoded fields name, with how they name each.
    pub ids: Vec<(u64, String)>,
    pub raw: Vec<u8>,
}

impl Message {
    /// How this message names `id`, or `None` when it does not.
    #[must_use]
    pub fn names(&self, id: u64) -> Option<String> {
        let mut how: Vec<String> = self
            .ids
            .iter()
            .filter(|(i, _)| *i == id)
            .map(|(_, h)| h.clone())
            .collect();
        how.dedup();
        if how.is_empty() {
            let bytes: Vec<u8> = match self.game {
                Game::Auberean => id.to_le_bytes().to_vec(),
                Game::Dereth => u32::try_from(id).ok()?.to_le_bytes().to_vec(),
            };
            let at = self.raw.windows(bytes.len()).position(|w| w == bytes)?;
            how.push(format!("its id at byte {at}"));
        }
        Some(how.join(", "))
    }
}

/// One session of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    pub client: SocketAddrV4,
    /// The server and the first port the session used.
    pub server: SocketAddrV4,
    /// Seconds since the recording's first datagram, at the session's first.
    pub start: f64,
    pub messages: usize,
}

/// A recording, read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub sessions: Vec<SessionInfo>,
    /// Every message, in the order they completed.
    pub messages: Vec<Message>,
}

impl Recording {
    /// The messages that name `id`, with how each names it.
    pub fn naming(&self, id: u64) -> impl Iterator<Item = (&Message, String)> {
        self.messages
            .iter()
            .filter_map(move |m| m.names(id).map(|how| (m, how)))
    }
}

/// Read the recording at `path`.
///
/// # Errors
/// When the file is not a capture, or has no game session.
pub fn read_file(path: &Path, ports: Option<PortSet>) -> Result<Recording, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read(std::io::BufReader::new(f), ports).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a recording.
///
/// # Errors
/// When the bytes are not a capture, or hold no game session.
pub fn read(capture: impl Read, ports: Option<PortSet>) -> Result<Recording, String> {
    let mut link = Link::new();
    let mut datagrams = Vec::new();
    for rec in CaptureReader::new(capture).map_err(|e| e.to_string())? {
        let rec = rec.map_err(|e| e.to_string())?;
        datagrams.extend(link.frame(rec.link_type, rec.ts, &rec.data));
    }
    let ports = match ports {
        Some(p) => p,
        None => server_ports(&datagrams).ok_or(
            "no login in the recording names the server's ports; give them with --server-ports",
        )?,
    };
    Ok(sessions(&datagrams, ports))
}

/// The server's ports, from the handshakes the recording holds.
fn server_ports(datagrams: &[Datagram]) -> Option<PortSet> {
    let mut ports = std::collections::BTreeSet::new();
    for d in datagrams {
        let Some(flags) = d.payload.get(4..8) else {
            continue;
        };
        let flags = u32::from_le_bytes([flags[0], flags[1], flags[2], flags[3]]);
        if flags & (PacketFlags::LOGIN_REQUEST | PacketFlags::WORLD_LOGIN_REQUEST) != 0 {
            ports.insert(d.dst.1);
        }
        if flags & PacketFlags::CONNECT_REQUEST != 0 {
            ports.insert(d.src.1);
        }
    }
    let ranges: Vec<(u16, u16)> = ports
        .into_iter()
        .map(|p| (p, p.saturating_add(1)))
        .collect();
    (!ranges.is_empty()).then_some(PortSet(ranges))
}

/// Route the datagrams into sessions, reassemble each, and name every message.
fn sessions(datagrams: &[Datagram], ports: PortSet) -> Recording {
    let t0 = datagrams.first().map_or(0.0, |d| d.ts);
    let mut flows = Flows::new(ports);
    let mut open: BTreeMap<usize, (Session, VecSink)> = BTreeMap::new();
    let mut done: BTreeMap<usize, VecSink> = BTreeMap::new();
    let mut starts: BTreeMap<usize, f64> = BTreeMap::new();
    for d in datagrams {
        let Some(route) = flows.route(d) else {
            continue;
        };
        if let Some(c) = route.closed {
            if let Some((s, mut sink)) = open.remove(&c) {
                let _ = s.finish(&mut sink);
                done.insert(c, sink);
            }
        }
        if route.opened {
            starts.insert(route.session, d.ts);
            open.insert(route.session, (Session::new(), VecSink::default()));
        }
        if let Some((s, sink)) = open.get_mut(&route.session) {
            s.datagram(route.dir, d.ts, &d.payload, sink);
        }
    }
    for (n, (s, mut sink)) in open {
        let _ = s.finish(&mut sink);
        done.insert(n, sink);
    }

    let mut out = Recording::default();
    for (n, sink) in done {
        let start = starts.get(&n).copied().unwrap_or(t0);
        let e = &flows.endpoints()[n];
        let first_port = e.server_ports.first().copied().unwrap_or(0);
        out.sessions.push(SessionInfo {
            client: SocketAddrV4::new(e.client.0, e.client.1),
            server: SocketAddrV4::new(e.server_ip, first_port),
            start: start - t0,
            messages: sink.messages.len(),
        });
        let session = out.sessions.len();
        out.messages.extend(
            sink.messages
                .into_iter()
                .map(|m| message(m, session, start - t0)),
        );
    }
    out.messages
        .sort_by(|a, b| a.t.total_cmp(&b.t).then(a.session.cmp(&b.session)));
    out
}

/// One reassembled message, named by its game.
fn message(m: MessageRow, session: usize, start: f64) -> Message {
    let mut out = Message {
        t: start + m.t,
        session,
        dir: m.dir,
        game: Game::Dereth,
        queue: m.queue,
        mtype: m.decoded.mtype,
        name: "unknown",
        read: m.decoded.status == Status::Ok,
        ids: Vec::new(),
        raw: Vec::new(),
    };
    if m.decoded.status == Status::Ac2 {
        out.game = Game::Auberean;
        let (name, read, ids) = second_game(m.dir, m.queue, &m.raw);
        out.name = name;
        out.read = read;
        out.ids = ids;
    } else {
        out.name = type_name(m.decoded.mtype).unwrap_or("unknown");
        out.ids = m
            .decoded
            .guids
            .iter()
            .map(|(id, path)| (u64::from(*id), path.clone()))
            .collect();
    }
    out.raw = m.raw;
    out
}

/// A second-game message's name, whether it read, and the object it is about.
fn second_game(dir: Dir, queue: u16, raw: &[u8]) -> (&'static str, bool, Vec<(u64, String)>) {
    let t = auberean_protocol::message_type(raw).unwrap_or(0);
    match dir {
        Dir::S2c => match ServerMessage::decode_fields(raw) {
            Ok(m) => {
                let about = match &m {
                    ServerMessage::CreateObject(c) => Some(c.id),
                    _ => auberean_client_model::events::subject(&m).map(|s| s.id),
                };
                let ids = about.map(|id| (id, "its subject".to_owned()));
                (m.name(), true, ids.into_iter().collect())
            }
            Err(_) => (
                ServerMessage::type_name(t).unwrap_or("unknown"),
                false,
                Vec::new(),
            ),
        },
        Dir::C2s if SECURE_QUEUES.contains(&queue) => ("secure", false, Vec::new()),
        Dir::C2s => match ClientMessage::decode_fields(raw) {
            Ok(m) => (m.name(), true, Vec::new()),
            Err(_) => (
                ClientMessage::type_name(t).unwrap_or("unknown"),
                false,
                Vec::new(),
            ),
        },
    }
}

/// An object id as typed: hexadecimal after `0x`, else decimal, else hexadecimal.
///
/// # Errors
/// When it is none of those.
pub fn parse_id(text: &str) -> Result<u64, String> {
    let t = text.trim();
    let hex = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"));
    match hex {
        Some(h) => u64::from_str_radix(&h.replace('_', ""), 16).ok(),
        None => t.parse().ok().or_else(|| u64::from_str_radix(t, 16).ok()),
    }
    .ok_or_else(|| format!("{text:?} is not an object id (such as 0x8000ABCD)"))
}

/// The recording's sessions, one line each.
#[must_use]
pub fn describe_sessions(r: &Recording) -> String {
    let mut s = String::new();
    for (n, e) in r.sessions.iter().enumerate() {
        s.push_str(&format!(
            "session {}: {} to {}, from {:.3} s, {} messages\n",
            n + 1,
            e.client,
            e.server,
            e.start,
            e.messages
        ));
    }
    s
}

/// One message as a line: its time, session, direction, type and name, and `how` when given.
#[must_use]
pub fn line(m: &Message, how: Option<&str>) -> String {
    let mut s = format!(
        "{:>10.3}  #{} {}  0x{:04X}  {}",
        m.t,
        m.session,
        m.dir.as_str(),
        m.mtype,
        m.name
    );
    if !m.read {
        s.push_str(" (unread)");
    }
    if let Some(how) = how {
        s.push_str("  ");
        s.push_str(how);
    }
    s
}

#[cfg(test)]
mod tests;
