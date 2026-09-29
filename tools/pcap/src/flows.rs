//! UDP flows to sessions.
//!
//! A session is one client connection: every datagram between one client endpoint and one server
//! address on one server port pair `P`/`P + 1`. The retail captures show exactly how the pair is
//! used: the client sends to `P`, except its `ConnectResponse` and `CICMDCommand`, which go to
//! `P + 1`; the server sends from `P + 1`, except its `ConnectRequest`, which comes from `P`. So
//! each datagram names its own `P` ([`pair_base`]), and pairs are never chained: a login server on
//! `9000/9001` and a world server on `9002/9003` are two connections even though `9001` and `9002`
//! are adjacent. A server that sends everything from `P` (the reference server's layout) still
//! pairs, because a datagram whose own `P` has no connection yet joins the one on its port or the
//! port below before it starts a new one. Which side is the server is decided by the port alone: a
//! port in the configured server range ([`PortSet`]) is the server's.
//!
//! One port pair can carry several connections in turn: a client that logs out and back in from
//! the same socket, a referral that moves the client to another server process behind the same
//! ports (it reconnects with a `WorldLoginRequest` and the sequence numbers start again), or a
//! capture tool that reports every datagram under one fabricated local endpoint. A handshake
//! datagram (the client's `LoginRequest`, `WorldLoginRequest` or `ConnectResponse`, the server's
//! `ConnectRequest`) that arrives after the session has carried sequenced traffic starts a new
//! session; the rest of one handshake, and retries of an unanswered request, do not. Any one of
//! the four is enough, because a capture often misses some of them.
//!
//! The old connection may go on talking for a while after the new one starts (acknowledgements,
//! a last few messages), so it stays open beside the new one, and each sequenced datagram goes to
//! whichever of the two its sequence number continues, until the old one disconnects or has been
//! silent for [`OLD_CONNECTION_QUIET`] seconds.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use crate::link::Datagram;

/// Which way a datagram travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    /// Client to server.
    C2s,
    /// Server to client.
    S2c,
}

impl Dir {
    /// `"c2s"` or `"s2c"`, the spelling the index stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::C2s => "c2s",
            Self::S2c => "s2c",
        }
    }

    /// 0 for client to server, 1 for server to client.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::C2s => 0,
            Self::S2c => 1,
        }
    }
}

/// The server's UDP ports: a list of inclusive ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSet(pub Vec<(u16, u16)>);

impl Default for PortSet {
    /// The retail servers' ports.
    fn default() -> Self {
        Self(vec![(9000, 9013)])
    }
}

impl PortSet {
    /// Parse `"9000-9013"`, `"9000-9013,9050,9100-9101"`.
    ///
    /// # Errors
    /// On anything that is not a comma-separated list of ports and inclusive ranges.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut v = Vec::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (a, b) = part.split_once('-').unwrap_or((part, part));
            let a: u16 = a
                .trim()
                .parse()
                .map_err(|_| format!("bad port `{a}` in `{s}`"))?;
            let b: u16 = b
                .trim()
                .parse()
                .map_err(|_| format!("bad port `{b}` in `{s}`"))?;
            if a > b {
                return Err(format!("empty port range `{part}`"));
            }
            v.push((a, b));
        }
        if v.is_empty() {
            return Err("no server ports".into());
        }
        Ok(Self(v))
    }

    #[must_use]
    pub fn contains(&self, p: u16) -> bool {
        self.0.iter().any(|(a, b)| (*a..=*b).contains(&p))
    }

    /// The spelling [`PortSet::parse`] reads.
    #[must_use]
    pub fn to_spec(&self) -> String {
        self.0
            .iter()
            .map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Where one datagram goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The session, numbered from 0 in order of first datagram within the capture.
    pub session: usize,
    pub dir: Dir,
    /// This datagram opened the session.
    pub opened: bool,
    /// This datagram's `LoginRequest` closed an earlier session on the same flow.
    pub closed: Option<usize>,
}

/// The endpoints of a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub client: (Ipv4Addr, u16),
    pub server_ip: Ipv4Addr,
    /// Every server port the session used, ascending.
    pub server_ports: BTreeSet<u16>,
}

#[derive(Debug)]
struct Group {
    client: (Ipv4Addr, u16),
    server_ip: Ipv4Addr,
    /// The port the connection began on; `base + 1` belongs to it too.
    base: u16,
    /// The connection the next login opened.
    session: usize,
    /// The connection before it, still open: the old connection can go on carrying datagrams for
    /// a while after the new one starts.
    previous: Option<usize>,
    /// Only handshake datagrams since the current connection began.
    handshaking: bool,
}

/// The session router for one capture.
#[derive(Debug, Default)]
pub struct Flows {
    ports: PortSet,
    groups: Vec<Group>,
    endpoints: Vec<Endpoints>,
    /// The highest sequence number seen per session and direction.
    last_seq: Vec<[Option<u32>; 2]>,
    /// When each session last carried a datagram, and whether it has disconnected.
    last_ts: Vec<f64>,
    ended: Vec<bool>,
    /// Datagrams with no server port on either side.
    pub non_game: u64,
    /// Datagrams with a server port on both sides.
    pub ambiguous: u64,
}

fn header_u32(payload: &[u8], at: usize) -> Option<u32> {
    payload
        .get(at..at + 4)
        .map(|f| u32::from_le_bytes([f[0], f[1], f[2], f[3]]))
}

/// The `P` of the port pair a datagram belongs to, from its direction, its server port and its
/// flags (see the module documentation).
#[must_use]
pub fn pair_base(dir: Dir, port: u16, flags: u32) -> u16 {
    use dereth_transport::PacketFlags as F;
    let upper = match dir {
        Dir::C2s => flags & (F::CONNECT_RESPONSE | F::CICMD_COMMAND) != 0,
        Dir::S2c => flags & F::CONNECT_REQUEST == 0,
    };
    if upper {
        port.saturating_sub(1)
    } else {
        port
    }
}

/// Whether a datagram is part of a connection's handshake: the client's `LoginRequest`,
/// `WorldLoginRequest` (after a referral) or `ConnectResponse`, or the server's `ConnectRequest`.
fn is_handshake(dir: Dir, payload: &[u8]) -> bool {
    use dereth_transport::PacketFlags as F;
    let mask = match dir {
        Dir::C2s => F::LOGIN_REQUEST | F::WORLD_LOGIN_REQUEST | F::CONNECT_RESPONSE,
        Dir::S2c => F::CONNECT_REQUEST,
    };
    header_u32(payload, 4).is_some_and(|f| f & mask != 0)
}

/// How far behind a connection's highest sequence number a datagram may be and still continue it
/// (reordering and retransmission).
const SEQ_BEHIND: u32 = 64;

/// Seconds of silence after which an old connection is no longer a candidate.
pub const OLD_CONNECTION_QUIET: f64 = 30.0;

/// Whether a datagram ends its connection: `Disconnect` or `NetErrorDisconnect`.
fn is_disconnect(payload: &[u8]) -> bool {
    use dereth_transport::PacketFlags as F;
    header_u32(payload, 4).is_some_and(|f| f & (F::DISCONNECT | F::NET_ERROR_DISCONNECT) != 0)
}

impl Flows {
    #[must_use]
    pub fn new(ports: PortSet) -> Self {
        Self {
            ports,
            ..Self::default()
        }
    }

    /// The endpoints of every session opened so far, by session number.
    #[must_use]
    pub fn endpoints(&self) -> &[Endpoints] {
        &self.endpoints
    }

    fn open(&mut self, client: (Ipv4Addr, u16), server: (Ipv4Addr, u16), fresh: bool) -> usize {
        self.endpoints.push(Endpoints {
            client,
            server_ip: server.0,
            server_ports: BTreeSet::from([server.1]),
        });
        // A connection opened by a login starts both sequences at 2; one first seen mid-stream
        // takes its sequences from what it carries.
        self.last_seq.push(if fresh {
            [Some(1), Some(1)]
        } else {
            [None, None]
        });
        self.last_ts.push(f64::NEG_INFINITY);
        self.ended.push(false);
        self.endpoints.len() - 1
    }

    /// Which of a group's open connections a sequenced datagram continues: the one whose highest
    /// sequence number in that direction it follows most closely.
    fn pick(&self, g: &Group, dir: Dir, seq: u32, ts: f64) -> usize {
        let Some(prev) = g.previous else {
            return g.session;
        };
        if seq == 0 || self.ended[prev] || ts - self.last_ts[prev] > OLD_CONNECTION_QUIET {
            return g.session;
        }
        let distance = |s: usize| {
            self.last_seq[s][dir.index()]
                .filter(|l| seq.saturating_add(SEQ_BEHIND) >= *l)
                .map(|l| seq.abs_diff(l))
        };
        match (distance(g.session), distance(prev)) {
            (Some(a), Some(b)) if b < a => prev,
            (None, Some(_)) => prev,
            _ => g.session,
        }
    }

    /// Route one datagram, or `None` when it is not game traffic.
    pub fn route(&mut self, d: &Datagram) -> Option<Route> {
        let s_src = self.ports.contains(d.src.1);
        let s_dst = self.ports.contains(d.dst.1);
        let (dir, client, server) = match (s_src, s_dst) {
            (true, false) => (Dir::S2c, d.dst, d.src),
            (false, true) => (Dir::C2s, d.src, d.dst),
            (true, true) => {
                self.ambiguous += 1;
                return None;
            }
            (false, false) => {
                self.non_game += 1;
                return None;
            }
        };
        let handshake = is_handshake(dir, &d.payload);
        let seq = header_u32(&d.payload, 0).unwrap_or(0);
        let base = pair_base(dir, server.1, header_u32(&d.payload, 4).unwrap_or(0));
        let same = |g: &Group, b: u16| g.client == client && g.server_ip == server.0 && g.base == b;
        let found = self
            .groups
            .iter()
            .position(|g| same(g, base))
            .or_else(|| self.groups.iter().position(|g| same(g, server.1)))
            .or_else(|| {
                self.groups
                    .iter()
                    .position(|g| server.1 > 0 && same(g, server.1 - 1))
            });
        let mut opened = false;
        let mut closed = None;
        let gi = if let Some(gi) = found {
            if handshake && !self.groups[gi].handshaking {
                let session = self.open(client, server, true);
                let g = &mut self.groups[gi];
                closed = g.previous.replace(g.session);
                g.session = session;
                g.handshaking = true;
                opened = true;
            }
            gi
        } else {
            let session = self.open(client, server, handshake);
            self.groups.push(Group {
                client,
                server_ip: server.0,
                base,
                session,
                previous: None,
                handshaking: handshake,
            });
            opened = true;
            self.groups.len() - 1
        };
        let target = if opened {
            self.groups[gi].session
        } else {
            self.pick(&self.groups[gi], dir, seq, d.ts)
        };
        self.last_ts[target] = d.ts;
        if is_disconnect(&d.payload) {
            self.ended[target] = true;
        }
        let g = &mut self.groups[gi];
        if !handshake && seq != 0 && target == g.session {
            g.handshaking = false;
        }
        if seq != 0 {
            let l = &mut self.last_seq[target][dir.index()];
            *l = Some(l.map_or(seq, |l| l.max(seq)));
        }
        self.endpoints[target].server_ports.insert(server.1);
        Some(Route {
            session: target,
            dir,
            opened,
            closed,
        })
    }

    /// The sessions still open, to be finished when the capture ends.
    #[must_use]
    pub fn open_sessions(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .groups
            .iter()
            .flat_map(|g| std::iter::once(g.session).chain(g.previous))
            .collect();
        v.sort_unstable();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 2);
    const S: Ipv4Addr = Ipv4Addr::new(198, 51, 100, 7);

    fn dg(src: (Ipv4Addr, u16), dst: (Ipv4Addr, u16), flags: u32) -> Datagram {
        let mut payload = vec![0u8; 20];
        payload[4..8].copy_from_slice(&flags.to_le_bytes());
        Datagram {
            ts: 0.0,
            src,
            dst,
            payload,
        }
    }

    const LOGIN: u32 = 0x0001_0000;
    const CONNECT: u32 = 0x0004_0000;
    const ACK: u32 = 0x0000_4000;

    #[test]
    fn port_sets_parse_and_print() {
        let p = PortSet::parse("9000-9013, 9050").unwrap();
        assert!(p.contains(9000) && p.contains(9013) && p.contains(9050));
        assert!(!p.contains(9014));
        assert_eq!(p.to_spec(), "9000-9013,9050");
        assert!(PortSet::parse("9013-9000").is_err());
        assert!(PortSet::parse("").is_err());
    }

    #[test]
    fn direction_comes_from_the_server_port_and_port_plus_one_joins_the_session() {
        let mut f = Flows::new(PortSet::default());
        let r0 = f.route(&dg((C, 50000), (S, 9000), LOGIN)).unwrap();
        assert_eq!((r0.session, r0.dir, r0.opened), (0, Dir::C2s, true));
        let r1 = f.route(&dg((S, 9000), (C, 50000), CONNECT)).unwrap();
        assert_eq!((r1.session, r1.dir, r1.opened), (0, Dir::S2c, false));
        // The ConnectResponse goes to port + 1: same session.
        let r2 = f.route(&dg((C, 50000), (S, 9001), 0x0008_0000)).unwrap();
        assert_eq!((r2.session, r2.opened), (0, false));
        assert_eq!(f.endpoints()[0].server_ports, BTreeSet::from([9000, 9001]));
        // Another client endpoint is another session.
        let r3 = f.route(&dg((C, 50001), (S, 9000), ACK)).unwrap();
        assert_eq!((r3.session, r3.opened), (1, true));
        // Not game traffic.
        assert!(f.route(&dg((C, 53), (S, 53), 0)).is_none());
        assert_eq!(f.non_game, 1);
    }

    #[test]
    fn a_second_login_on_one_flow_splits_the_session_but_login_retries_do_not() {
        let mut f = Flows::new(PortSet::default());
        let a = f.route(&dg((C, 12345), (S, 9000), LOGIN)).unwrap();
        let b = f.route(&dg((C, 12345), (S, 9000), LOGIN)).unwrap();
        assert_eq!((a.session, b.session, b.opened), (0, 0, false));
        f.route(&dg((S, 9000), (C, 12345), CONNECT)).unwrap();
        f.route(&seq_dg((C, 12345), (S, 9000), 2, ACK)).unwrap();
        let c = f.route(&dg((C, 12345), (S, 9000), LOGIN)).unwrap();
        // The old connection stays open beside the new one; nothing is closed yet.
        assert_eq!((c.session, c.opened, c.closed), (1, true, None));
        let d = f.route(&dg((S, 9000), (C, 12345), CONNECT)).unwrap();
        assert_eq!(d.session, 1);
        assert_eq!(f.endpoints().len(), 2);
    }

    const WORLD_LOGIN: u32 = 0x0002_0000;

    #[test]
    fn the_server_sends_from_the_upper_port_and_a_capture_may_start_there() {
        // Mid-stream: the first datagram is the server's, from P + 1.
        let mut f = Flows::new(PortSet::default());
        let a = f.route(&seq_dg((S, 9009), (C, 12345), 15714, 6)).unwrap();
        let b = f.route(&seq_dg((C, 12345), (S, 9008), 8258, 6)).unwrap();
        let cicmd = f
            .route(&seq_dg((C, 12345), (S, 9009), 0, 0x0040_0000))
            .unwrap();
        assert_eq!((a.session, b.session, cicmd.session), (0, 0, 0));
        // Another pair on the same address, also mid-stream.
        let c = f
            .route(&seq_dg((S, 9001), (C, 12345), 5909, 0x0C00_4002))
            .unwrap();
        let d = f
            .route(&seq_dg((C, 12345), (S, 9000), 5880, 0x0800_0002))
            .unwrap();
        assert_eq!((c.session, d.session), (1, 1));
        assert_eq!(f.endpoints()[0].server_ports, BTreeSet::from([9008, 9009]));
        assert_eq!(pair_base(Dir::S2c, 9008, 0x0004_0000), 9008);
        assert_eq!(pair_base(Dir::C2s, 9009, 0x0008_0000), 9008);
    }

    #[test]
    fn port_pairs_are_not_chained() {
        let mut f = Flows::new(PortSet::default());
        let login = f.route(&dg((C, 12345), (S, 9000), LOGIN)).unwrap();
        f.route(&dg((C, 12345), (S, 9001), 0x0008_0000)).unwrap();
        // The world server on the next pair is another connection, alive at the same time.
        let world = f.route(&dg((C, 12345), (S, 9002), WORLD_LOGIN)).unwrap();
        assert_ne!(login.session, world.session);
        assert_eq!(
            f.route(&dg((C, 12345), (S, 9003), 0x0008_0000))
                .unwrap()
                .session,
            world.session
        );
        assert_eq!(
            f.route(&dg((S, 9000), (C, 12345), ACK)).unwrap().session,
            login.session
        );
        assert_eq!(
            f.endpoints()[login.session].server_ports,
            BTreeSet::from([9000, 9001])
        );
        assert_eq!(
            f.endpoints()[world.session].server_ports,
            BTreeSet::from([9002, 9003])
        );
    }

    #[test]
    fn a_world_login_after_a_referral_on_the_same_ports_is_a_new_session() {
        let mut f = Flows::new(PortSet::default());
        let a = f.route(&dg((C, 12345), (S, 9008), WORLD_LOGIN)).unwrap();
        f.route(&dg((S, 9008), (C, 12345), CONNECT)).unwrap();
        f.route(&seq_dg((S, 9008), (C, 12345), 5, 0x0000_0800))
            .unwrap();
        // Retries of the next request belong to one new session.
        let b = f.route(&dg((C, 12345), (S, 9008), WORLD_LOGIN)).unwrap();
        let c = f.route(&dg((C, 12345), (S, 9008), WORLD_LOGIN)).unwrap();
        assert_eq!((b.opened, b.closed), (true, None));
        assert_eq!((c.session, c.opened), (b.session, false));
        // A third login retires the first connection.
        f.route(&dg((S, 9008), (C, 12345), CONNECT)).unwrap();
        f.route(&seq_dg((C, 12345), (S, 9008), 2, ACK)).unwrap();
        let d = f.route(&dg((C, 12345), (S, 9008), WORLD_LOGIN)).unwrap();
        assert_eq!((d.opened, d.closed), (true, Some(a.session)));
    }

    fn seq_dg(src: (Ipv4Addr, u16), dst: (Ipv4Addr, u16), seq: u32, flags: u32) -> Datagram {
        let mut d = dg(src, dst, flags);
        d.payload[0..4].copy_from_slice(&seq.to_le_bytes());
        d
    }

    #[test]
    fn a_connect_response_alone_is_enough_to_see_a_new_connection() {
        // The capture missed the new connection's request and the server's ConnectRequest.
        let mut f = Flows::new(PortSet::default());
        let (c, s) = ((C, 12345), (S, 9008));
        let old = f.route(&seq_dg(s, c, 1742, 6)).unwrap().session;
        f.route(&seq_dg(s, c, 1928, 0x0020_0002)).unwrap();
        let new = f.route(&seq_dg(c, s, 0, 0x0008_0000)).unwrap();
        assert!(new.opened);
        assert_ne!(new.session, old);
        // The new connection's sequence numbers later pass through the old one's range.
        for seq in 2..2000 {
            assert_eq!(
                f.route(&seq_dg(s, c, seq, 6)).unwrap().session,
                new.session,
                "seq {seq}"
            );
        }
    }

    #[test]
    fn an_old_connection_still_talking_on_the_same_ports_keeps_its_own_datagrams() {
        let mut f = Flows::new(PortSet::default());
        let (c, s) = ((C, 12345), (S, 9008));
        let old = f.route(&seq_dg(s, c, 3811, 6)).unwrap().session;
        f.route(&seq_dg(c, s, 900, 6)).unwrap();
        let new = f.route(&seq_dg(c, s, 0, WORLD_LOGIN)).unwrap().session;
        assert_ne!(old, new);
        assert_eq!(f.route(&seq_dg(s, c, 3812, 6)).unwrap().session, old);
        assert_eq!(f.route(&seq_dg(s, c, 0, CONNECT)).unwrap().session, new);
        assert_eq!(f.route(&seq_dg(s, c, 2, 6)).unwrap().session, new);
        assert_eq!(f.route(&seq_dg(c, s, 2, 6)).unwrap().session, new);
        assert_eq!(f.route(&seq_dg(s, c, 3813, 6)).unwrap().session, old);
        assert_eq!(f.route(&seq_dg(c, s, 901, 6)).unwrap().session, old);
        assert_eq!(f.route(&seq_dg(s, c, 3, 6)).unwrap().session, new);
        assert_eq!(f.open_sessions(), vec![old, new]);
    }

    #[test]
    fn a_session_captured_mid_stream_is_still_a_session() {
        let mut f = Flows::new(PortSet::parse("7000-7001").unwrap());
        let r = f.route(&dg((S, 7000), (C, 40000), ACK)).unwrap();
        assert_eq!((r.session, r.dir, r.opened), (0, Dir::S2c, true));
    }
}
