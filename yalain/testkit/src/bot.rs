//! A client that speaks the wire: the game login, the handshake answer, messages both ways on
//! the login and world connections, and the referral from one to the other.
//!
//! It does what the September 2013 client does on the transport: the login to the login
//! service's even port (resent every two seconds until answered), the handshake answer to the
//! odd port (resent until the server's first packet arrives), the world login to the address a
//! referral names (resent every third of a second until answered).

use std::net::SocketAddr;

use auberean_protocol::{ClientMessage, ServerMessage};
use dereth_transport::conn::{handshake_port, ConnectRequest, Referral};
use dereth_transport::flow::{
    HANDSHAKE_RESEND, LOGIN_RESEND, LOGIN_RESEND_TRIES, WORLD_LOGIN_RESEND,
};
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use yalain_net::blob::{blob_id, BlobKind};
use yalain_net::endpoint::{standalone, Endpoint, Role};
use yalain_net::login::build_login;
use yalain_net::LocalTime;

/// A message the bot received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    /// On the world connection (else the login connection).
    pub world: bool,
    pub queue: u16,
    pub blob_id: u64,
    pub bytes: Vec<u8>,
}

impl Received {
    /// The message, read.
    #[must_use]
    pub fn message(&self) -> Option<ServerMessage> {
        ServerMessage::decode(&self.bytes).ok()
    }
}

#[derive(Debug)]
struct Link {
    ep: Endpoint,
    /// The handshake answer and where it goes, until the server's first packet arrives.
    answer: Option<(Vec<u8>, SocketAddr, LocalTime)>,
    sequence: u32,
}

#[derive(Debug, Clone, Copy)]
struct WorldLogin {
    cookie: u64,
    addr: SocketAddr,
    sent: Option<LocalTime>,
}

/// One client.
#[derive(Debug)]
pub struct Bot {
    pub addr: SocketAddr,
    /// The login service's even port.
    pub server: SocketAddr,
    login_packet: Option<(Vec<u8>, LocalTime, u32)>,
    login: Option<Link>,
    world: Option<Link>,
    world_login: Option<WorldLogin>,
    /// Follow a referral to the world (as the client does).
    pub follow_referrals: bool,
    pub inbox: Vec<Received>,
    pub referrals: Vec<Referral>,
    /// The server said goodbye on the login connection.
    pub dropped: bool,
    out: Vec<(SocketAddr, Vec<u8>)>,
}

impl Bot {
    #[must_use]
    pub fn new(addr: SocketAddr, server: SocketAddr) -> Self {
        Self {
            addr,
            server,
            login_packet: None,
            login: None,
            world: None,
            world_login: None,
            follow_referrals: true,
            inbox: Vec::new(),
            referrals: Vec::new(),
            dropped: false,
            out: Vec::new(),
        }
    }

    /// Log in with an account and ticket.
    pub fn log_in(&mut self, account: &str, ticket: &[u8], now: LocalTime) {
        self.log_in_with(build_login(account, ticket, 1_485_419_089), now);
    }

    /// Log in with a login body written elsewhere (another client's, byte for byte).
    pub fn log_in_with(&mut self, body: Vec<u8>, now: LocalTime) {
        let packet = standalone(0, 0, PacketFlags::LOGIN_REQUEST, body);
        self.out.push((self.server, packet.clone()));
        self.login_packet = Some((packet, now, 1));
    }

    /// Is the login connection up?
    #[must_use]
    pub fn logged_in(&self) -> bool {
        self.login.is_some()
    }

    /// Is the world connection up?
    #[must_use]
    pub fn in_world(&self) -> bool {
        self.world.is_some()
    }

    /// Send a message on the login connection (`world` false) or the world connection.
    pub fn send(&mut self, world: bool, queue: u16, m: &ClientMessage) {
        self.send_bytes(world, queue, m.encode());
    }

    /// Send a message's bytes as they are.
    pub fn send_bytes(&mut self, world: bool, queue: u16, bytes: Vec<u8>) {
        let link = if world {
            self.world.as_mut()
        } else {
            self.login.as_mut()
        };
        let Some(l) = link else { return };
        let seq = l.sequence;
        l.sequence += 1;
        #[allow(clippy::cast_possible_truncation)]
        l.ep.send_blob(
            queue,
            blob_id(BlobKind::UNORDERED, 0, seq as u16, seq),
            bytes,
        );
    }

    /// The messages received on one connection, read.
    #[must_use]
    pub fn messages(&self, world: bool) -> Vec<ServerMessage> {
        self.inbox
            .iter()
            .filter(|r| r.world == world)
            .filter_map(Received::message)
            .collect()
    }

    /// Take a datagram from the network.
    pub fn on_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        let Ok(p) = ParsedPacket::parse(bytes) else {
            return;
        };
        if let Some(body) = p.optional.get(&PacketFlags::CONNECT_REQUEST) {
            let Ok(cr) = ConnectRequest::from_bytes(body) else {
                return;
            };
            let world = if from == self.server && self.login.is_none() {
                false
            } else if self.world_login.is_some_and(|w| w.addr == from) {
                self.world_login = None;
                true
            } else {
                return;
            };
            #[allow(clippy::cast_possible_truncation)]
            let own = (cr.net_id & 0xFFFF) as u16;
            let ep = Endpoint::new(
                Role::Client,
                own,
                p.header.rec_id,
                p.header.iteration,
                cr.seeds(),
                from,
                0.0,
                now,
            );
            let answer = standalone(
                own,
                p.header.iteration,
                PacketFlags::CONNECT_RESPONSE,
                cr.cookie.to_le_bytes().to_vec(),
            );
            let to = SocketAddr::new(from.ip(), handshake_port(from.port()));
            self.out.push((to, answer.clone()));
            let link = Link {
                ep,
                answer: Some((answer, to, now)),
                sequence: 1,
            };
            if world {
                self.world = Some(link);
            } else {
                self.login_packet = None;
                self.login = Some(link);
            }
            return;
        }
        let world = match (&self.login, &self.world) {
            (Some(l), _) if handshake_port(l.ep.peer.port()) == from.port() => false,
            (_, Some(w)) if handshake_port(w.ep.peer.port()) == from.port() => true,
            _ => return,
        };
        let link = if world {
            self.world.as_mut()
        } else {
            self.login.as_mut()
        };
        let Some(l) = link else { return };
        l.answer = None;
        let Ok(inc) = l.ep.feed(&p, bytes.len(), now) else {
            return;
        };
        for b in inc.blobs {
            self.inbox.push(Received {
                world,
                queue: b.queue_id,
                blob_id: b.id.0,
                bytes: b.payload,
            });
        }
        if let Some(r) = inc.referral {
            self.referrals.push(r);
            if self.follow_referrals {
                self.follow(r);
            }
        }
        if inc.disconnect {
            if world {
                self.world = None;
            } else {
                self.login = None;
                self.dropped = true;
            }
        }
    }

    /// Follow a referral: log in to the world it names with its cookie.
    pub fn follow(&mut self, r: Referral) {
        self.world = None;
        self.world_login = Some(WorldLogin {
            cookie: r.cookie,
            addr: SocketAddr::V4(r.addr),
            sent: None,
        });
    }

    /// Run the bot's connections for this moment; returns the datagrams to send.
    pub fn tick(&mut self, now: LocalTime) -> Vec<(SocketAddr, Vec<u8>)> {
        if let Some((packet, at, tries)) = self.login_packet.as_mut() {
            if now.seconds_since(*at) >= LOGIN_RESEND && *tries < LOGIN_RESEND_TRIES {
                *at = now;
                *tries += 1;
                self.out.push((self.server, packet.clone()));
            }
        }
        if let Some(w) = self.world_login.as_mut() {
            if w.sent
                .is_none_or(|t| now.seconds_since(t) >= WORLD_LOGIN_RESEND)
            {
                w.sent = Some(now);
                let packet = standalone(
                    0,
                    0,
                    PacketFlags::WORLD_LOGIN_REQUEST,
                    w.cookie.to_le_bytes().to_vec(),
                );
                self.out.push((w.addr, packet));
            }
        }
        for l in [self.login.as_mut(), self.world.as_mut()]
            .into_iter()
            .flatten()
        {
            if let Some((answer, to, at)) = l.answer.as_mut() {
                if now.seconds_since(*at) >= HANDSHAKE_RESEND {
                    *at = now;
                    self.out.push((*to, answer.clone()));
                }
            }
            let peer = l.ep.peer;
            for dg in l.ep.tick(now) {
                self.out.push((peer, dg));
            }
        }
        std::mem::take(&mut self.out)
    }

    /// Leave the world alone: a goodbye on the world connection, the login connection kept.
    pub fn leave_world(&mut self) {
        if let Some(l) = self.world.take() {
            self.out.push((l.ep.peer, l.ep.disconnect_packet()));
        }
    }

    /// Leave: a goodbye on each connection.
    pub fn leave(&mut self) {
        for l in [self.login.take(), self.world.take()].into_iter().flatten() {
            self.out.push((l.ep.peer, l.ep.disconnect_packet()));
        }
    }
}
