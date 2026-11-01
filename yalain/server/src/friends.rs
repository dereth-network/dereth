//! The friends service's connections: a TCP listener on the login port's number (the game's
//! login is UDP on the same number), each connection read and written without blocking on the
//! world's own loop.
//!
//! The service itself is `yalain_friends`'s: this module hands it bytes and the time, checks its
//! tickets by the login's own rule, tells it who enters and leaves the world, hands it the
//! network's commands said in chat, and shows its lines in the players' chat windows. With
//! `--no-friends` there is no listener: nobody is present, `@dn` still answers, and each entry
//! shows the line that the friends list is out of reach (as it does for an account with no
//! stream).

use std::collections::HashMap;
use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use yalain_federation::Federation;
use yalain_friends::{Out, Service, StreamId};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;
use yalain_world::Server;

/// The most bytes read from one connection in one pass.
const READ_PER_PASS: usize = 16 * 1024;

/// The most bytes waiting to be written to one connection; a client that reads slower is closed.
const MAX_PENDING: usize = 1024 * 1024;

/// One connection.
#[derive(Debug)]
struct Conn {
    tcp: TcpStream,
    /// Bytes waiting for the socket.
    pending: Vec<u8>,
    /// The service closed it: it goes once what waits is written.
    closing: bool,
}

/// The friends service and its connections.
#[derive(Debug)]
pub struct Friends {
    pub service: Service,
    listener: Option<TcpListener>,
    conns: HashMap<StreamId, Conn>,
    next: StreamId,
    /// Each account's character in the world, as last told to the service.
    in_world: HashMap<u64, u64>,
}

impl Friends {
    /// The service, listening on `at` (none: no listener).
    ///
    /// # Errors
    /// The port cannot be bound.
    pub fn start(service: Service, at: Option<SocketAddr>) -> Result<Self, String> {
        let listener = match at {
            Some(at) => {
                let l = TcpListener::bind(at).map_err(|e| {
                    format!("the friends service on TCP {at}: {e} (--no-friends turns it off)")
                })?;
                l.set_nonblocking(true)
                    .map_err(|e| format!("the friends service: {e}"))?;
                tracing::info!("friends service listening on TCP {at}");
                Some(l)
            }
            None => None,
        };
        Ok(Self {
            service,
            listener,
            conns: HashMap::new(),
            next: 1,
            in_world: HashMap::new(),
        })
    }

    /// One pass of the world's loop: new connections, what they sent, the tickets, who entered
    /// and left the world, the network's commands, the clock; then what to write and the lines
    /// to show. Whether anything was read or written.
    pub fn pump<S: Store>(
        &mut self,
        now: Duration,
        server: &mut Server<S>,
        net: &mut ServerNet,
        mut fed: Option<&mut Federation>,
    ) -> bool {
        let mut busy = self.accept(now);
        busy |= self.read(now, &mut server.store, fed.as_deref_mut());
        for t in self.service.take_tickets() {
            let account = yalain_world::login::admits(
                server.cfg.accounts,
                server.cfg.secret_cost,
                &mut server.store,
                &t.ticket.account,
                t.ticket.secret.as_bytes(),
                unix_now(),
            );
            match account {
                Some(a) => self.service.admit(
                    now,
                    t.stream,
                    t.cx,
                    a.id,
                    &mut server.store,
                    fed.as_deref_mut(),
                ),
                None => self.service.refuse(
                    now,
                    t.stream,
                    t.cx,
                    false,
                    &mut server.store,
                    fed.as_deref_mut(),
                ),
            }
        }
        self.world(now, server, fed.as_deref_mut());
        for c in std::mem::take(&mut server.network_commands) {
            let lines = self.service.said(
                &mut server.store,
                fed.as_deref_mut(),
                c.character,
                &c.name,
                &c.words,
            );
            for line in lines {
                yalain_world::login::notice(net, c.world, &line);
            }
        }
        self.service
            .poll(now, &mut server.store, fed.as_deref_mut());
        let (wrote, lost) = self.write();
        busy |= wrote;
        for id in lost {
            self.service
                .closed(now, id, &mut server.store, fed.as_deref_mut());
        }
        for (character, line) in self.service.take_lines() {
            if let Some(world) = world_of(server, character) {
                yalain_world::login::notice(net, world, &line);
            }
        }
        // What the network's directory has to tell a character in the world (its page, or that
        // the hub took it out), as the same system line; the rest wait for its next login.
        if let Some(f) = fed {
            for (name, line) in f.take_notices(|n| world_named(server, n).is_some()) {
                if let Some(world) = world_named(server, &name) {
                    yalain_world::login::notice(net, world, &line);
                }
            }
        }
        if let Some(ignoring) = self.service.local_ignores() {
            server.ignoring = ignoring;
        }
        // Without a listener no stream comes, and the world says so at each entry.
        if self.listener.is_some() {
            if let Some(accounts) = self.service.accounts_with_streams() {
                server.friends_streams = Some(accounts);
            }
        }
        busy
    }

    fn accept(&mut self, now: Duration) -> bool {
        let Some(l) = &self.listener else {
            return false;
        };
        let mut any = false;
        loop {
            match l.accept() {
                Ok((tcp, peer)) => {
                    any = true;
                    if tcp.set_nonblocking(true).is_err() {
                        continue;
                    }
                    let _ = tcp.set_nodelay(true);
                    let id = self.next;
                    self.next += 1;
                    self.conns.insert(
                        id,
                        Conn {
                            tcp,
                            pending: Vec::new(),
                            closing: false,
                        },
                    );
                    self.service.accept(now, id, peer.ip());
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => {
                    tracing::debug!("friends service: accepting: {e}");
                    break;
                }
            }
        }
        any
    }

    fn read<S: Store>(
        &mut self,
        now: Duration,
        store: &mut S,
        mut fed: Option<&mut Federation>,
    ) -> bool {
        let mut any = false;
        let mut buf = vec![0u8; READ_PER_PASS];
        let mut gone = Vec::new();
        let mut got: Vec<(StreamId, Vec<u8>)> = Vec::new();
        for (id, c) in &mut self.conns {
            if c.closing {
                continue;
            }
            match c.tcp.read(&mut buf) {
                Ok(0) => gone.push(*id),
                Ok(n) => got.push((*id, buf[..n].to_vec())),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => gone.push(*id),
            }
        }
        for (id, bytes) in got {
            any = true;
            self.service
                .on_bytes(now, id, &bytes, store, fed.as_deref_mut());
        }
        for id in gone {
            any = true;
            self.conns.remove(&id);
            self.service.closed(now, id, store, fed.as_deref_mut());
        }
        any
    }

    /// Who is in the world now, against what the service was last told. A character is in the
    /// world while its body is, lingering included: a client gone or replaced by a new login
    /// leaves its login behind at once but its body until the linger ends, and the service's
    /// hand-over counts from the body's leaving.
    fn world<S: Store>(
        &mut self,
        now: Duration,
        server: &mut Server<S>,
        mut fed: Option<&mut Federation>,
    ) {
        let mut here: HashMap<u64, (u64, String)> = HashMap::new();
        for p in server.players.values() {
            // Its account: its login's, or, its login gone, the one the service was told.
            let account = server
                .sessions
                .get(&p.login)
                .map(|x| x.account.id)
                .or_else(|| {
                    self.in_world
                        .iter()
                        .find(|(_, c)| **c == p.character)
                        .map(|(a, _)| *a)
                });
            if let Some(account) = account {
                here.insert(account, (p.character, p.name.clone()));
            }
        }
        let left: Vec<u64> = self
            .in_world
            .iter()
            .filter(|(a, c)| here.get(a).is_none_or(|(h, _)| h != *c))
            .map(|(a, _)| *a)
            .collect();
        for account in left {
            self.in_world.remove(&account);
            self.service
                .left(now, account, &mut server.store, fed.as_deref_mut());
        }
        for (account, (character, name)) in here {
            if self.in_world.contains_key(&account) {
                continue;
            }
            self.in_world.insert(account, character);
            self.service.entered(
                now,
                account,
                character,
                &name,
                &mut server.store,
                fed.as_deref_mut(),
            );
        }
    }

    /// Writes what waits; whether anything was written, and the connections closed.
    fn write(&mut self) -> (bool, Vec<StreamId>) {
        let mut any = false;
        for o in self.service.take_out() {
            match o {
                Out::Write(id, bytes) => {
                    if let Some(c) = self.conns.get_mut(&id) {
                        c.pending.extend_from_slice(&bytes);
                    }
                }
                Out::Close(id) => {
                    if let Some(c) = self.conns.get_mut(&id) {
                        c.closing = true;
                    }
                }
            }
        }
        let mut gone = Vec::new();
        for (id, c) in &mut self.conns {
            while !c.pending.is_empty() {
                match c.tcp.write(&c.pending) {
                    Ok(0) => {
                        gone.push(*id);
                        break;
                    }
                    Ok(n) => {
                        any = true;
                        c.pending.drain(..n);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => {
                        gone.push(*id);
                        break;
                    }
                }
            }
            if c.pending.len() > MAX_PENDING || (c.closing && c.pending.is_empty()) {
                gone.push(*id);
            }
        }
        for id in &gone {
            if let Some(c) = self.conns.remove(id) {
                let _ = c.tcp.shutdown(std::net::Shutdown::Both);
            }
        }
        (any, gone)
    }

    /// Closes every connection.
    pub fn stop(&mut self) {
        for (_, c) in self.conns.drain() {
            let _ = c.tcp.shutdown(std::net::Shutdown::Both);
        }
        self.listener = None;
    }
}

/// The world connection of a character in the world.
fn world_of<S: Store>(server: &Server<S>, character: u64) -> Option<ConnId> {
    server
        .players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(c, _)| *c)
}

fn world_named<S: Store>(server: &Server<S>, name: &str) -> Option<ConnId> {
    server
        .players
        .iter()
        .find(|(_, p)| p.name.eq_ignore_ascii_case(name))
        .map(|(c, _)| *c)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The address the friends service listens on: the login's, on TCP.
#[must_use]
pub fn listen_at(bind: IpAddr, login_port: u16) -> SocketAddr {
    SocketAddr::new(bind, login_port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yalain_testkit::Harness;

    fn at(h: &Harness) -> Duration {
        Duration::from_secs_f64(h.now.0)
    }

    /// A character whose client went away (its login gone at once) stays in the world for the
    /// friends service until its body leaves at the end of its linger: the service's hand-over
    /// counts from then.
    /// Behaviour: server.friends.hand-over-keeps-presence
    #[test]
    fn a_character_whose_login_went_stays_in_the_world_until_its_body_leaves() {
        let mut h = Harness::new();
        let service = Service::new(yalain_friends::Config::new("Dawnsong", ""));
        let mut f = Friends::start(service, None).expect("no listener");
        let (a, id) = h.player_in_world("one", "Elara");
        let account = h
            .server
            .sessions
            .values()
            .find(|x| x.account.name == "one")
            .map(|x| x.account.id)
            .expect("its login");
        f.world(at(&h), &mut h.server, None);
        assert_eq!(f.in_world.get(&account), Some(&id));
        h.bots[a].leave();
        assert!(h.run_until(400, |h| !h
            .server
            .sessions
            .values()
            .any(|x| x.account.id == account)));
        let left = h.now.0;
        while h.server.players.values().any(|p| p.character == id) {
            f.world(at(&h), &mut h.server, None);
            assert_eq!(
                f.in_world.get(&account),
                Some(&id),
                "in the world {:.1} s after its login went",
                h.now.0 - left
            );
            assert!(h.now.0 - left < 30.0, "the body leaves");
            h.step();
        }
        assert!(h.now.0 - left > 10.0, "it lingered");
        f.world(at(&h), &mut h.server, None);
        assert_eq!(f.in_world.get(&account), None, "gone with its body");
    }
}
