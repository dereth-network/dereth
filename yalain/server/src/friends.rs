//! The friends service's connections: a TCP listener on the login port's number (the game's
//! login is UDP on the same number), each connection read and written without blocking on the
//! world's own loop.
//!
//! The service itself is `yalain_friends`'s: this module hands it bytes and the time, checks its
//! tickets by the login's own rule, tells it who enters and leaves the world, hands it the
//! network's commands said in chat (an admin's `@dn verify` and `@dn admin` to the network's half), and shows its
//! lines in the players' chat windows. It also carries the Dereth Network's rooms between the
//! world's chat service and the network's half: lines said, lines heard, `@dng`, and the lines the
//! hub asks a character to say ([`Friends::rooms`]). With
//! `--no-friends` there is no listener: nobody is present, `@dn` still answers, and each entry
//! shows the line that the friends list is out of reach (as it does for an account with no
//! stream).

use std::collections::HashMap;
use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use yalain_federation::{Character, Federation, Room, Speaker};
use yalain_friends::{Out, Service, StreamId};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;
use yalain_world::admin::Ticket;
use yalain_world::chat::NetworkCommand;
use yalain_world::Server;

/// The most bytes read from one connection in one pass.
const READ_PER_PASS: usize = 16 * 1024;

/// The most bytes waiting to be written to one connection; a client that reads slower is closed.
const MAX_PENDING: usize = 1024 * 1024;

/// What carries a stream.
#[derive(Debug)]
enum Carrier {
    Tcp(TcpStream),
    /// The friends stream of the game's WebSocket connection at this address.
    Game(SocketAddr),
}

/// The most bytes in one message of a game connection's friends stream.
const CARRIED_PIECE: usize = 16 * 1024;

/// One connection.
#[derive(Debug)]
struct Conn {
    carrier: Carrier,
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
    /// The streams the game's connections carry, by the connection's address.
    carried: HashMap<SocketAddr, StreamId>,
    /// What to send on those connections' streams, an empty message to end one.
    carried_out: Vec<(SocketAddr, Vec<u8>)>,
    next: StreamId,
    /// Each account's character in the world, as last told to the service.
    in_world: HashMap<u64, u64>,
}

impl Friends {
    /// The service, listening on `at` (none: no listener).
    ///
    /// # Errors
    /// The port cannot be bound.
    pub fn start(service: Service, at: Option<SocketAddr>) -> Result<Self, crate::Failure> {
        let listener = match at {
            Some(at) => {
                let l = TcpListener::bind(at).map_err(|e| {
                    crate::Failure::binding(
                        format!(
                            "binding the ports: the friends service on TCP {at} (--no-friends \
                             turns it off)"
                        ),
                        &e,
                    )
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
            carried: HashMap::new(),
            carried_out: Vec::new(),
            next: 1,
            in_world: HashMap::new(),
        })
    }

    /// What a game connection sent on its friends stream: its first bytes open the stream, and an
    /// empty message ends it.
    pub fn on_carried<S: Store>(
        &mut self,
        now: Duration,
        from: SocketAddr,
        bytes: &[u8],
        store: &mut S,
        fed: Option<&mut Federation>,
    ) {
        if bytes.is_empty() {
            if let Some(id) = self.carried.remove(&from) {
                self.conns.remove(&id);
                self.service.closed(now, id, store, fed);
            }
            return;
        }
        let id = match self.carried.get(&from) {
            Some(id) => *id,
            None => {
                let id = self.next;
                self.next += 1;
                self.carried.insert(from, id);
                self.conns.insert(
                    id,
                    Conn {
                        carrier: Carrier::Game(from),
                        pending: Vec::new(),
                        closing: false,
                    },
                );
                self.service.accept(now, id, from.ip());
                id
            }
        };
        self.service.on_bytes(now, id, bytes, store, fed);
    }

    /// The streams whose game connection has gone end with it.
    pub fn drop_carried<S: Store>(
        &mut self,
        now: Duration,
        open: impl Fn(SocketAddr) -> bool,
        store: &mut S,
        mut fed: Option<&mut Federation>,
    ) {
        let gone: Vec<SocketAddr> = self.carried.keys().copied().filter(|a| !open(*a)).collect();
        for from in gone {
            if let Some(id) = self.carried.remove(&from) {
                self.conns.remove(&id);
                self.service.closed(now, id, store, fed.as_deref_mut());
            }
        }
    }

    /// What to send on game connections' friends streams.
    pub fn take_carried_out(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        std::mem::take(&mut self.carried_out)
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
        for r in std::mem::take(&mut server.renames) {
            self.service.renamed(
                now,
                r.character,
                &r.old,
                &r.new,
                &mut server.store,
                fed.as_deref_mut(),
            );
        }
        self.service.set_gagged(server.gags.characters());
        if let Some(f) = fed.as_deref_mut() {
            f.set_too_new(yalain_world::saves::too_new_in_world(server));
        }
        busy |= self.read(now, &mut server.store, fed.as_deref_mut());
        for t in self.service.take_tickets() {
            // A banned account's ticket is refused as banned: the client says the account is.
            match yalain_world::admin::ticket(
                server,
                &t.ticket.account,
                t.ticket.secret.as_bytes(),
                unix_now(),
            ) {
                Ticket::Admitted(a) => self.service.admit(
                    now,
                    t.stream,
                    t.cx,
                    a.id,
                    &mut server.store,
                    fed.as_deref_mut(),
                ),
                refused => self.service.refuse(
                    now,
                    t.stream,
                    t.cx,
                    refused == Ticket::Banned,
                    &mut server.store,
                    fed.as_deref_mut(),
                ),
            }
        }
        self.world(now, server, fed.as_deref_mut());
        for c in std::mem::take(&mut server.network_commands) {
            let lines = if c.admin && yalain_world::chat::is_verify(&c.words) {
                vec![verify_said(fed.as_deref_mut(), now.as_secs_f64(), &c)]
            } else if c.admin && yalain_world::chat::is_admin_word(&c.words) {
                self.operator_said(&mut server.store, fed.as_deref_mut(), &c)
            } else {
                self.service.said(
                    now,
                    &mut server.store,
                    fed.as_deref_mut(),
                    c.character,
                    &c.name,
                    &c.words,
                )
            };
            // One message: separate ones of a long answer may overtake each other on the wire.
            if !lines.is_empty() {
                yalain_world::login::notice(net, c.world, &lines.join("\n"));
            }
        }
        for t in std::mem::take(&mut server.unheard_tells) {
            self.service.unheard_tell(
                now,
                t.character,
                &t.name,
                &t.tellee,
                &t.text,
                fed.as_deref_mut(),
            );
        }
        self.rooms(now, server, net, fed.as_deref_mut());
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

    /// The Dereth Network's rooms, one pass: `@dng` is said as a line of the room, the lines the
    /// world's players said go to the network's half, and what comes from the network reaches the
    /// players in the room. A player who ignores a speaker does not hear it.
    fn rooms<S: Store>(
        &mut self,
        now: Duration,
        server: &mut Server<S>,
        net: &mut ServerNet,
        fed: Option<&mut Federation>,
    ) {
        let Some(f) = fed else {
            server.room_lines.clear();
            return;
        };
        let t = now.as_secs_f64();
        for line in f.take_global_lines() {
            yalain_world::chat::say_in_room(
                server,
                net,
                line.character,
                room_id(line.room),
                &line.text,
            );
        }
        for said in std::mem::take(&mut server.room_lines) {
            if let Some(room) = room_of(said.room) {
                f.room_said(
                    &mut self.service,
                    t,
                    said.character,
                    &said.name,
                    room,
                    &said.text,
                );
            }
        }
        for ask in f.take_says() {
            let character = character_named(server, ask.who.name());
            f.answer_say(t, &ask, character);
        }
        for line in f.take_heard() {
            let service = &self.service;
            let ignoring = &server.ignoring;
            let room = room_id(line.room);
            let hears = |id: u64, name: &str| match &line.speaker {
                Speaker::Remote(from) => !yalain_federation::Friends::ignores(service, name, from),
                Speaker::Own(speaker) => !ignoring
                    .get(&id)
                    .is_some_and(|names| names.contains(&speaker.to_lowercase())),
            };
            yalain_world::chat::hear_from_afar(server, net, room, &line.name, &line.text, hears);
        }
    }

    /// `@dn admin ...`, said in game by an admin's character: the lines to answer it with.
    fn operator_said<S: Store>(
        &mut self,
        store: &mut S,
        fed: Option<&mut Federation>,
        c: &NetworkCommand,
    ) -> Vec<String> {
        let args: Vec<&str> = c.words.split_whitespace().skip(1).collect();
        match fed {
            Some(f) => f.operator_said(store, &mut self.service, c.character, &args),
            None => vec![yalain_federation::WORLD_OFF.to_owned()],
        }
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
                            carrier: Carrier::Tcp(tcp),
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
            let Carrier::Tcp(tcp) = &mut c.carrier else {
                continue;
            };
            match tcp.read(&mut buf) {
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
        // After those who left, whom nobody is to see come back on their way out; before those who
        // entered, whom nobody is to see arrive.
        self.service.set_hidden(
            now,
            yalain_world::presence::cloaked_characters(server),
            &mut server.store,
            fed.as_deref_mut(),
        );
        self.service
            .set_suffixed(yalain_world::presence::suffixed_characters(server));
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
            if let Carrier::Game(to) = c.carrier {
                if !c.pending.is_empty() {
                    any = true;
                    for piece in c.pending.chunks(CARRIED_PIECE) {
                        self.carried_out.push((to, piece.to_vec()));
                    }
                    c.pending.clear();
                }
                if c.closing {
                    self.carried_out.push((to, Vec::new()));
                    gone.push(*id);
                }
                continue;
            }
            let Carrier::Tcp(tcp) = &mut c.carrier else {
                continue;
            };
            while !c.pending.is_empty() {
                match tcp.write(&c.pending) {
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
                match c.carrier {
                    Carrier::Tcp(tcp) => {
                        let _ = tcp.shutdown(std::net::Shutdown::Both);
                    }
                    Carrier::Game(to) => {
                        self.carried.remove(&to);
                    }
                }
            }
        }
        (any, gone)
    }

    /// Closes every connection.
    pub fn stop(&mut self) {
        for (_, c) in self.conns.drain() {
            if let Carrier::Tcp(tcp) = c.carrier {
                let _ = tcp.shutdown(std::net::Shutdown::Both);
            }
        }
        self.carried.clear();
        self.listener = None;
    }
}

/// The world connection of a character in the world.
/// The chat room of one of the rooms the network can use.
fn room_id(room: Room) -> u32 {
    match room {
        Room::General => yalain_world::chat::GENERAL_ROOM,
        Room::Trade => yalain_world::chat::TRADE_ROOM,
        Room::Pvp => yalain_world::chat::PVP_ROOM,
    }
}

/// The room the network knows a chat room as, if it can use it.
fn room_of(id: u32) -> Option<Room> {
    [Room::General, Room::Trade, Room::Pvp]
        .into_iter()
        .find(|r| room_id(*r) == id)
}

/// A character the hub asks to speak as, as the world has it: none for no such character, or one
/// deleted. Whether its account is banned is asked of the store, so that a ban made since it
/// entered counts, and it holds whether or not the character is in the world.
fn character_named<S: Store>(server: &mut Server<S>, name: &str) -> Option<Character> {
    let c = server
        .store
        .character_named(name)
        .ok()
        .flatten()
        .filter(|c| c.deleted_at.is_none())?;
    let account = server
        .store
        .accounts()
        .ok()?
        .into_iter()
        .find(|a| a.id == c.account);
    let banned =
        account.is_some_and(|a| yalain_world::admin::banned(&mut server.store, &a, unix_now()));
    // A gag holds whether or not the character is in the world.
    let gagged = server.gags.gagged(c.id) || matches!(server.store.gag(c.id), Ok(Some(_)));
    let too_new = yalain_world::saves::too_new(server, c.id);
    Some(Character {
        id: c.id,
        name: c.name,
        banned,
        gagged,
        too_new,
    })
}

/// `@dn verify <token>`, said in game at `t` by an admin's character: the line to answer it with
/// at once. The hub's answer comes later, to the same character ([`show_operator`]).
fn verify_said(fed: Option<&mut Federation>, t: f64, c: &NetworkCommand) -> String {
    let args: Vec<&str> = c.words.split_whitespace().skip(1).collect();
    match fed {
        Some(f) => f.verify(t, Some(c.character), &c.name, &args),
        None => yalain_federation::WORLD_OFF.to_owned(),
    }
}

/// A line for the operator who typed a verification: in its character's chat window while it is
/// in the world, and in the log otherwise (the console's, or a character's that has left).
pub fn show_operator<S: Store>(
    server: &Server<S>,
    net: &mut ServerNet,
    who: Option<u64>,
    line: &str,
) {
    match who.and_then(|id| world_of(server, id)) {
        Some(world) => yalain_world::login::notice(net, world, line),
        None => tracing::info!("Dereth Network: {line}"),
    }
}

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

    /// The world tells the service who has a suffix on every pass: a name as a client shows it finds
    /// the character until the suffix comes off, and never while it is cloaked.
    /// Behaviour: server.friends.suffixed-name
    #[test]
    fn the_service_knows_the_names_clients_are_shown_while_they_are_shown() {
        use yalain_friends::Target;
        let mut h = Harness::new();
        let service = Service::new(yalain_friends::Config::new("Dawnsong", ""));
        let mut f = Friends::start(service, None).expect("no listener");
        let (_, id) = h.player_in_world("one", "Elara");
        let shown = "Elara St. 3@Dawnsong";
        let world = |h: &Harness| {
            *h.server
                .players
                .iter()
                .find(|(_, p)| p.character == id)
                .expect("in the world")
                .0
        };
        let at_now = at(&h);
        f.world(at_now, &mut h.server, None);
        assert_eq!(f.service.target(shown), Target::NoSuchUser);
        let w = world(&h);
        let p = h.server.players.get_mut(&w).expect("in the world");
        "St. 3".clone_into(&mut p.play.suffix);
        f.world(at_now, &mut h.server, None);
        assert_eq!(f.service.target(shown), Target::Local("Elara".into()));
        let p = h.server.players.get_mut(&w).expect("in the world");
        p.admin.cloaked = true;
        f.world(at_now, &mut h.server, None);
        assert_eq!(f.service.target(shown), Target::NoSuchUser);
        let p = h.server.players.get_mut(&w).expect("in the world");
        p.admin.cloaked = false;
        p.play.suffix.clear();
        f.world(at_now, &mut h.server, None);
        assert_eq!(f.service.target(shown), Target::NoSuchUser);
    }

    /// A game connection's first bytes on its friends stream open the stream, what the world says
    /// goes back by that connection in messages of its own, and an empty message ends it. A stream
    /// whose connection has gone ends too.
    /// Behaviour: none (this project's carriage of the stream for a browser: no retail behavior)
    #[test]
    fn a_game_connection_carries_a_friends_stream_from_its_first_bytes_to_its_end() {
        use auberean_friends::PREAMBLE;
        let mut h = Harness::new();
        let service = Service::new(yalain_friends::Config::new("Dawnsong", ""));
        let mut f = Friends::start(service, None).expect("no listener");
        let from = SocketAddr::from(([127, 0, 0, 1], 50_000));
        let t = at(&h);
        f.on_carried(t, from, &PREAMBLE, &mut h.server.store, None);
        assert_eq!(f.carried.len(), 1, "its first bytes open it");
        f.on_carried(t, from, b"", &mut h.server.store, None);
        assert!(f.carried.is_empty(), "an empty message ends it");
        assert!(f.conns.is_empty());
        f.on_carried(t, from, &PREAMBLE, &mut h.server.store, None);
        f.drop_carried(t, |_| true, &mut h.server.store, None);
        assert_eq!(f.carried.len(), 1, "open, it stays");
        f.drop_carried(t, |_| false, &mut h.server.store, None);
        assert!(f.carried.is_empty(), "gone, it ends");
    }

    /// A stream for an account, admitted without a check of its ticket.
    fn stream_for(h: &mut Harness, f: &mut Friends, fed: &mut Federation, id: u64, account: &str) {
        use auberean_friends::{Cx, Element, Hello, Request, Ticket, PREAMBLE, PROTO};
        f.service.accept(
            at(h),
            id,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        );
        let mut bytes = PREAMBLE.to_vec();
        for e in [
            Element::Hello(Hello {
                proto: PROTO,
                software: "test/1".into(),
                locale: None,
            }),
            Element::Request {
                cx: Cx(1),
                request: Request::PresentTicket {
                    email: String::new(),
                    ticket: Ticket::new(account, "ticket").encode().expect("a ticket"),
                },
            },
        ] {
            e.write_frame(&mut bytes).expect("fits");
        }
        f.service
            .on_bytes(at(h), id, &bytes, &mut h.server.store, Some(&mut *fed));
        let check = f.service.take_tickets().pop().expect("its ticket");
        let account = h
            .server
            .sessions
            .values()
            .find(|x| x.account.name == account)
            .map(|x| x.account.id)
            .expect("its login");
        f.service.admit(
            at(h),
            check.stream,
            check.cx,
            account,
            &mut h.server.store,
            Some(fed),
        );
    }

    /// `/rename` reaches the friends service and the network at their next pass, with no relog: the
    /// character goes on present and on the network by the new name, and what the world's own tell
    /// honours follows the list of one who ignores it.
    /// Behaviour: server.friends.rename-seen-at-once
    #[test]
    fn a_rename_said_in_the_world_reaches_the_friends_service_and_the_network() {
        use auberean_friends::{Element, Request};
        use auberean_script::args::Arg;
        use auberean_script::native_object::NativeObject;
        use yalain_world::graph::Builder;
        const RENAME: u32 = 0x8201_0130;
        let mut h = Harness::new();
        let (_, elara) = h.player_in_world("one", "Elara");
        let (_, cara) = h.player_in_world("two", "Cara");
        let me = h.at_character_list("three");
        let keeper = h.create_character(me, "three", "Keeper");
        let clock = h.clock();
        for line in ["set-accountadmin three", "set-characteradmin Keeper"] {
            let c = yalain_world::admin::parse(line)
                .expect("a command")
                .expect("it reads");
            yalain_world::admin::run(
                &mut h.server,
                &mut h.net,
                clock,
                &yalain_world::admin::Issuer::Console,
                &c,
            );
        }
        h.enter_world(me, "three", keeper);
        h.run(10);
        let (mut f, mut fed) = linked(&h, None);
        stream_for(&mut h, &mut f, &mut fed, 1, "one");
        stream_for(&mut h, &mut f, &mut fed, 2, "two");
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        f.world(at(&h), &mut h.server, Some(&mut fed));
        let mut bytes = Vec::new();
        Element::Request {
            cx: auberean_friends::Cx(2),
            request: Request::IgnoredAdd {
                user: "Elara".into(),
            },
        }
        .write_frame(&mut bytes)
        .expect("fits");
        f.service
            .on_bytes(at(&h), 2, &bytes, &mut h.server.store, Some(&mut fed));
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));

        let mut b = Builder::new();
        b.native(118, NativeObject::Text("Renamed".encode_utf16().collect()));
        // Its parameters as declared travel last first.
        let call = yalain_testkit::play::server_call(
            RENAME,
            &[Arg::Object(b.graph()), Arg::LongInt(elara)],
        );
        h.bots[me].send(true, auberean_protocol::queue::WEENIE, &call);
        h.run(20);
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert!(f.service.is_present("Renamed"));
        assert!(!f.service.is_present("Elara"));
        assert!(fed.is_on("Renamed"));
        assert!(!fed.is_on("Elara"));
        assert_eq!(
            h.server.ignoring[&cara],
            std::collections::HashSet::from(["renamed".to_owned()])
        );
        // The pass after has nothing to do: it is not seen to leave and come again.
        let _ = fed.take_frames(h.now.0);
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert!(f.service.is_present("Renamed"));
        assert_eq!(fed.take_frames(h.now.0), Vec::<String>::new());
    }

    /// A cloak reaches the friends service and the network from the world at its next pass: a friend
    /// in the world and another world's watch see the character go offline, and it is not shown
    /// online to either on its way out of the world, nor as it comes back still cloaked.
    /// Behaviour: server.friends.hidden-is-not-on-the-network
    #[test]
    fn a_cloak_reaches_the_network_and_a_cloaked_character_comes_and_goes_unseen() {
        use auberean_friends::{Element, Event, FrameReader, Mode, Request};
        use dereth_network_protocol::{Address, Message};
        let frames = |fed: &mut Federation, t: f64| -> Vec<Message> {
            fed.take_frames(t)
                .iter()
                .filter_map(|f| Message::from_json(f).ok())
                .collect()
        };
        let mut h = Harness::new();
        let a = h.at_character_list("one");
        let elara = h.create_character(a, "one", "Elara");
        let clock = h.clock();
        for line in ["set-accountadmin one", "set-characteradmin Elara"] {
            let c = yalain_world::admin::parse(line)
                .expect("a command")
                .expect("it reads");
            yalain_world::admin::run(
                &mut h.server,
                &mut h.net,
                clock,
                &yalain_world::admin::Issuer::Console,
                &c,
            );
        }
        h.enter_world(a, "one", elara);
        h.player_in_world("two", "Viewer");
        h.run(10);
        let (mut f, mut fed) = linked(&h, None);
        let t = h.now.0;
        stream_for(&mut h, &mut f, &mut fed, 1, "one");
        stream_for(&mut h, &mut f, &mut fed, 2, "two");
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        f.world(at(&h), &mut h.server, Some(&mut fed));
        assert!(f.service.is_present("Elara"));

        // What the friend's client is told of Elara from now on.
        let mut bytes = Vec::new();
        Element::Request {
            cx: auberean_friends::Cx(2),
            request: Request::FriendAdd {
                user: "Elara".into(),
            },
        }
        .write_frame(&mut bytes)
        .expect("fits");
        f.service
            .on_bytes(at(&h), 2, &bytes, &mut h.server.store, Some(&mut fed));
        let mut reader = FrameReader::new();
        let mut told_friend = |f: &mut Friends| -> Vec<Mode> {
            let mut modes = Vec::new();
            for o in f.service.take_out() {
                let yalain_friends::Out::Write(2, b) = o else {
                    continue;
                };
                reader.push(&b);
                while let Some(text) = reader.next_frame().expect("a frame") {
                    if let Ok(Element::Event(Event::FriendStat { user, mode })) =
                        Element::parse(&text)
                    {
                        if user.starts_with("Elara") {
                            modes.push(mode);
                        }
                    }
                }
            }
            modes
        };
        let _ = told_friend(&mut f);

        let bob = Address::parse("Bob@coldeve").expect("an address");
        let told = |online| Message::Presence {
            to: bob.clone(),
            of: Address::parse("Elara@dawnsong").expect("an address"),
            online,
        };
        let _ = frames(&mut fed, t);
        let watch = Message::Watch {
            owner: bob.clone(),
            target: Address::parse("Elara@dawnsong").expect("an address"),
            on: true,
        };
        fed.on_frame(&mut f.service, t, &watch.to_json());
        assert!(frames(&mut fed, t).contains(&told(true)));

        h.server
            .players
            .values_mut()
            .find(|p| p.character == elara)
            .expect("in the world")
            .admin
            .cloaked = true;
        f.world(at(&h), &mut h.server, Some(&mut fed));
        assert!(frames(&mut fed, t).contains(&told(false)), "cloaked: gone");
        assert_eq!(told_friend(&mut f), [Mode::OFFLINE], "to its friend too");

        h.bots[a].leave_world();
        while h.server.players.values().any(|p| p.character == elara) {
            h.step();
        }
        f.world(at(&h), &mut h.server, Some(&mut fed));
        assert!(
            !frames(&mut fed, t).contains(&told(true)),
            "not online on its way out"
        );
        assert_eq!(told_friend(&mut f), [], "its friend is told nothing");

        h.run(400);
        h.enter_world(a, "one", elara);
        assert!(
            h.server
                .players
                .values()
                .any(|p| p.character == elara && p.admin.cloaked),
            "still cloaked"
        );
        f.world(at(&h), &mut h.server, Some(&mut fed));
        assert!(f.service.is_present("Elara"));
        assert!(
            !frames(&mut fed, t).contains(&told(true)),
            "nor as it comes back"
        );
        assert_eq!(told_friend(&mut f), [], "its friend is told nothing");
    }

    /// An admin's `@dn admin ...` said in game is the operator's command, answered in its chat
    /// window; a player's is a word the friends service does not know: its help. With the network
    /// off the world says so.
    /// Behaviour: server.network.operator-words-in-game-for-admins
    #[test]
    fn dn_admin_said_by_an_admin_blocks_and_a_players_is_the_help() {
        use yalain_federation::{Settings, WORLD_OFF};
        let mut h = Harness::new();
        let service = Service::new(yalain_friends::Config::new("Dawnsong", "dawnsong"));
        let mut f = Friends::start(service, None).expect("no listener");
        let mut fed = Federation::new(
            Settings {
                enabled: true,
                slug: "dawnsong".into(),
                software: "yalain test".into(),
                ..Settings::default()
            },
            1,
        );
        let (a, elara) = h.player_in_world("one", "Elara");
        let (b, cara) = h.player_in_world("two", "Cara");
        let said = |h: &mut Harness, b: usize, character: u64, name: &str, admin: bool| {
            let world = world_of(&h.server, character).expect("in the world");
            h.server.network_commands.push(NetworkCommand {
                world,
                character,
                name: name.into(),
                words: "admin block coldeve".into(),
                admin,
            });
            h.bots[b].inbox.len()
        };
        let from = [
            said(&mut h, a, elara, "Elara", true),
            said(&mut h, b, cara, "Cara", false),
        ];
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(
            h.bots[a].notices(from[0]),
            ["coldeve is blocked for everyone on this world."]
        );
        assert_eq!(
            h.bots[b].notices(from[1]),
            [yalain_friends::HELP.join("\n")],
            "all of it, in order"
        );
        assert_eq!(fed.blocks(), ["coldeve"]);
        let from = said(&mut h, a, elara, "Elara", true);
        f.pump(at(&h), &mut h.server, &mut h.net, None);
        h.run(20);
        assert_eq!(h.bots[a].notices(from), [WORLD_OFF]);
    }

    /// A world on the network, carrying the Global room where `room` says, its link up.
    fn linked(h: &Harness, room: Option<&'static str>) -> (Friends, Federation) {
        use yalain_federation::Settings;
        let service = Service::new(yalain_friends::Config::new("Dawnsong", "dawnsong"));
        let f = Friends::start(service, None).expect("no listener");
        let mut fed = Federation::new(
            Settings {
                enabled: true,
                slug: "dawnsong".into(),
                software: "yalain test".into(),
                global_room: room,
                global_room_named: false,
                websocket_url: None,
            },
            1,
        );
        let t = h.now.0;
        fed.on_link_up();
        let _hello = fed.take_frames(t);
        let welcome = dereth_network_protocol::Message::Welcome {
            proto: dereth_network_protocol::wire::PROTOCOL,
            world: "dawnsong".into(),
            hub: "test hub".into(),
            worlds: Vec::new(),
            rooms: vec!["global".into()],
            limits: dereth_network_protocol::limits::Limits::DEFAULT,
        };
        let mut f = f;
        fed.on_frame(&mut f.service, t, &welcome.to_json());
        let _join = fed.take_frames(t);
        (f, fed)
    }

    /// A line a client sent to a chat room.
    fn room_line(h: &mut Harness, b: usize, room: u32, text: &str) {
        use auberean_protocol::login::ChatPacket;
        use auberean_protocol::ClientMessage;
        use dereth_protocol::turbine::{ExtraBlob, SendToRoomById};
        let packet = SendToRoomById {
            context: 7,
            room,
            text: text.into(),
            extra: ExtraBlob::Speaker {
                sender: 1,
                chat_type: 100 << 16 | 1,
            },
        }
        .network_packet()
        .expect("packs");
        h.bots[b].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
        h.run(20);
    }

    /// The room lines a client was sent since `from`: its room, the name and the text.
    fn room_lines(h: &Harness, b: usize, from: usize) -> Vec<(u32, String, String)> {
        use auberean_protocol::login::ChatPacket;
        use auberean_protocol::ServerMessage;
        use dereth_protocol::turbine::{decode_incoming, IncomingPayload};
        h.bots[b].inbox[from..]
            .iter()
            .filter_map(|r| match r.message() {
                Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                    &[
                        &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                        &p,
                    ]
                    .concat(),
                )
                .ok()
                .map(|i| i.payload),
                _ => None,
            })
            .filter_map(|p| match p {
                IncomingPayload::RoomEvent(e) => Some((e.room, e.name, e.text)),
                _ => None,
            })
            .collect()
    }

    /// The `room.say`s the world sends the hub now: their rooms, speakers and words.
    fn room_says(fed: &mut Federation, t: f64) -> Vec<(String, String, String)> {
        fed.take_frames(t)
            .iter()
            .filter_map(|f| dereth_network_protocol::Message::from_json(f).ok())
            .filter_map(|m| match m {
                dereth_network_protocol::Message::RoomSay {
                    room, from, text, ..
                } => Some((room, from.to_string(), text)),
                _ => None,
            })
            .collect()
    }

    fn hub_line(fed: &mut Federation, f: &mut Friends, t: f64, text: &str) {
        let line = dereth_network_protocol::Message::RoomLine {
            room: "global".into(),
            from: dereth_network_protocol::Address::parse("Bob@coldeve").expect("an address"),
            text: text.into(),
            via: None,
        };
        fed.on_frame(&mut f.service, t, &line.to_json());
    }

    /// `@dn ...` said by a character: what it is answered with comes in its chat window.
    fn dn(h: &mut Harness, character: u64, name: &str, words: &str) {
        let world = world_of(&h.server, character).expect("in the world");
        h.server.network_commands.push(NetworkCommand {
            world,
            character,
            name: name.into(),
            words: words.into(),
            admin: false,
        });
    }

    /// A game tell the world could not deliver reaches the friends service in the same pass, and goes
    /// on to the hub when its name is an away, linked character's.
    /// Behaviour: server.network.tell-away-linked
    #[test]
    fn a_game_tell_that_reached_no_player_goes_on_through_the_host() {
        let mut h = Harness::new();
        let (_, elara) = h.player_in_world("one", "Elara");
        h.run(10);
        let (mut f, mut fed) = linked(&h, None);
        let t = h.now.0;
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        let dee = h.server.store.next_character_number().expect("a number");
        h.server
            .store
            .insert_character(&yalain_store::Character {
                id: dee,
                account: dee,
                name: "Dee".into(),
                race: 4,
                sex: 0x2000,
                class: 4,
                implement: 8,
                start_area: 0x4000_0005,
                entity_did: 0x4700_110F,
                physique: Vec::new(),
                look: Vec::new(),
                created_at: 0,
                deleted_at: None,
                last_played: 0,
            })
            .expect("kept");
        fed.set_on(&mut h.server.store, &f.service, dee, "Dee", true)
            .expect("kept");
        assert_eq!(fed.link_person(&mut h.server.store, t, dee, "Dee"), None);
        let _ = fed.take_frames(t);
        h.server
            .unheard_tells
            .push(yalain_world::chat::UnheardTell {
                character: elara,
                name: "Elara".into(),
                tellee: "Dee".into(),
                text: "you there?".into(),
            });
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert!(h.server.unheard_tells.is_empty(), "taken");
        let sent: Vec<String> = fed
            .take_frames(h.now.0)
            .iter()
            .filter_map(|f| dereth_network_protocol::Message::from_json(f).ok())
            .filter_map(|m| match m {
                dereth_network_protocol::Message::Tell { to, text, .. } => {
                    Some(format!("{to}: {text}"))
                }
                _ => None,
            })
            .collect();
        assert_eq!(sent, ["Dee@dawnsong: you there?"]);
    }

    /// A line each way, through the world's own chat service: a line said in the room that carries
    /// the Global room goes to the hub from a character on the network, and a line from another
    /// world reaches this world's players in that room. `@dng` is a line said in that room from
    /// anywhere, and off the network it says why not.
    /// Behaviour: server.network.global-room-a-line-each-way
    #[test]
    fn a_line_goes_each_way_between_the_world_and_the_hub_and_dng_says_one() {
        let mut h = Harness::new();
        let (a, elara) = h.player_in_world("one", "Elara");
        let (b, cara) = h.player_in_world("two", "Cara");
        h.run(10);
        let (mut f, mut fed) = linked(&h, Some("general"));
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        let _ = fed.take_frames(h.now.0);

        // Out, in the room that carries it; a line in another room stays here.
        room_line(&mut h, a, 1, "Anyone near Arwic?");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(
            room_says(&mut fed, h.now.0),
            [(
                "global".to_owned(),
                "Elara@dawnsong".to_owned(),
                "Anyone near Arwic?".to_owned()
            )]
        );
        room_line(&mut h, a, 8, "A sword for sale");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(room_says(&mut fed, h.now.0), []);

        // Off the network, heard here and not sent.
        room_line(&mut h, b, 1, "Only here");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(room_says(&mut fed, h.now.0), []);

        // In: every player in the world, on the network or not.
        let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
        hub_line(&mut fed, &mut f, h.now.0, "Ballroom at 8!");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        let line = (1, "Bob@coldeve".to_owned(), "Ballroom at 8!".to_owned());
        assert_eq!(room_lines(&h, a, from[0]), std::slice::from_ref(&line));
        assert_eq!(room_lines(&h, b, from[1]), [line]);

        // `@dng`, from anywhere: shown here as the room's own line, and sent.
        let from = h.bots[b].inbox.len();
        dn(&mut h, elara, "Elara", "global dance at 8");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(
            room_lines(&h, b, from),
            [(1, "Elara".to_owned(), "dance at 8".to_owned())]
        );
        assert_eq!(
            room_says(&mut fed, h.now.0),
            [(
                "global".to_owned(),
                "Elara@dawnsong".to_owned(),
                "dance at 8".to_owned()
            )]
        );

        // Off the network: told so, and nothing is said.
        let from = h.bots[b].inbox.len();
        dn(&mut h, cara, "Cara", "global hello");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(h.bots[b].notices(from), [yalain_federation::NOT_ON]);
        assert_eq!(room_lines(&h, b, from), []);
        assert_eq!(room_says(&mut fed, h.now.0), []);

        // A player who squelches the speaker's world does not hear it, and the others do.
        fed.set_on(&mut h.server.store, &f.service, cara, "Cara", true)
            .expect("kept");
        dn(&mut h, cara, "Cara", "squelch coldeve");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
        hub_line(&mut fed, &mut f, h.now.0, "Second line");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(room_lines(&h, a, from[0]).len(), 1);
        assert_eq!(room_lines(&h, b, from[1]), []);
    }

    /// The room the hub's word means here is the one the lines use: `roleplay` is the PvP room, each
    /// way, and an empty setting carries nothing either way.
    /// Behaviour: server.network.global-room-pvp-each-way
    #[test]
    fn roleplay_is_the_pvp_room_each_way_and_an_empty_setting_carries_nothing() {
        let mut h = Harness::new();
        let (a, elara) = h.player_in_world("one", "Elara");
        h.run(10);
        let (mut f, mut fed) = linked(&h, Some("roleplay"));
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        let _ = fed.take_frames(h.now.0);
        room_line(
            &mut h,
            a,
            yalain_world::chat::PVP_ROOM,
            "To the other worlds",
        );
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(room_says(&mut fed, h.now.0).len(), 1);
        let from = h.bots[a].inbox.len();
        hub_line(&mut fed, &mut f, h.now.0, "Ballroom at 8!");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(room_lines(&h, a, from)[0].0, yalain_world::chat::PVP_ROOM);

        let (mut f, mut fed) = linked(&h, None);
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        let _ = fed.take_frames(h.now.0);
        for room in [
            yalain_world::chat::GENERAL_ROOM,
            yalain_world::chat::TRADE_ROOM,
            yalain_world::chat::PVP_ROOM,
        ] {
            room_line(&mut h, a, room, "Stays here");
        }
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(room_says(&mut fed, h.now.0), []);
        let from = h.bots[a].inbox.len();
        hub_line(&mut fed, &mut f, h.now.0, "Nobody hears this");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(room_lines(&h, a, from), []);
        dn(&mut h, elara, "Elara", "global hello");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(h.bots[a].notices(from), [yalain_federation::NOT_CARRIED]);
    }

    /// The operator's least age reaches the network's half from the world: a character under it
    /// says nothing and is told why, and the hub's asks to speak for it are refused; old enough it
    /// goes.
    /// Behaviour: server.network.too-new-sends-nothing
    #[test]
    fn the_worlds_least_age_holds_back_what_a_character_sends_to_the_network() {
        use dereth_network_protocol::say::{SayMessage, SayOutcome};
        use yalain_federation::Room;
        let mut h = Harness::new();
        let (_, elara) = h.player_in_world("one", "Elara");
        h.run(10);
        let (mut f, mut fed) = linked(&h, Some("general"));
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        fed.link_person(&mut h.server.store, h.now.0, elara, "Elara");
        let _ = fed.take_frames(h.now.0);
        h.server.cfg.min_character_age_seconds = 3_600;
        let attempt = |h: &mut Harness, f: &mut Friends, fed: &mut Federation| {
            f.pump(at(h), &mut h.server, &mut h.net, Some(fed));
            let t = h.now.0;
            fed.room_said(&mut f.service, t, elara, "Elara", Room::General, "hi");
            let said = !fed.take_frames(t).is_empty();
            let m = SayMessage::Ask {
                id: "h1".into(),
                room: "global".into(),
                who: dereth_network_protocol::Address::parse("Elara@dawnsong").expect("an address"),
                text: "evening all".into(),
                via: "discord".into(),
            };
            fed.on_frame(&mut f.service, t, &m.to_json());
            f.pump(at(h), &mut h.server, &mut h.net, Some(fed));
            let spoke = fed
                .take_frames(t)
                .iter()
                .find_map(|x| match SayMessage::from_json(x) {
                    Ok(SayMessage::Result {
                        outcome, reason, ..
                    }) => Some((outcome, reason)),
                    _ => None,
                });
            (said, spoke)
        };
        assert_eq!(
            attempt(&mut h, &mut f, &mut fed),
            (
                false,
                Some((SayOutcome::Refused, Some("too_new".to_owned())))
            )
        );
        for p in h.server.players.values_mut() {
            p.play.played_seconds = 3_600;
        }
        let (said, spoke) = attempt(&mut h, &mut f, &mut fed);
        assert!(said, "its line goes");
        assert_eq!(spoke, Some((SayOutcome::Said, None)));
    }

    /// A line the hub asks a character to say, by what the world knows of the character: shown to
    /// the room's players marked with where it was said, except one who ignores it, and answered
    /// `said`; a character with no such name, one gagged and one banned are refused, whether or not
    /// it is in the world.
    /// Behaviour: server.network.say-from-the-worlds-own-facts
    #[test]
    fn a_say_from_the_hub_reads_the_worlds_own_facts_of_the_character() {
        use dereth_network_protocol::say::{SayMessage, SayOutcome};
        let mut h = Harness::new();
        let (a, elara) = h.player_in_world("one", "Elara");
        let (b, cara) = h.player_in_world("two", "Cara");
        h.run(10);
        let (mut f, mut fed) = linked(&h, Some("general"));
        fed.set_on(&mut h.server.store, &f.service, elara, "Elara", true)
            .expect("kept");
        fed.link_person(&mut h.server.store, h.now.0, elara, "Elara");
        let _ = fed.take_frames(h.now.0);
        h.server
            .ignoring
            .entry(cara)
            .or_default()
            .insert("elara".to_owned());

        let ask = |fed: &mut Federation, f: &mut Friends, t: f64, who: &str| {
            let m = SayMessage::Ask {
                id: "h1".into(),
                room: "global".into(),
                who: dereth_network_protocol::Address::parse(who).expect("an address"),
                text: "evening all".into(),
                via: "discord".into(),
            };
            fed.on_frame(&mut f.service, t, &m.to_json());
        };
        let answer = |fed: &mut Federation, t: f64| -> Vec<(SayOutcome, Option<String>)> {
            fed.take_frames(t)
                .iter()
                .filter_map(|f| match SayMessage::from_json(f) {
                    Ok(SayMessage::Result {
                        outcome, reason, ..
                    }) => Some((outcome, reason)),
                    _ => None,
                })
                .collect()
        };

        let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
        ask(&mut fed, &mut f, h.now.0, "Elara@dawnsong");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(
            room_lines(&h, a, from[0]),
            [(1, "Elara (Discord)".to_owned(), "evening all".to_owned())]
        );
        assert_eq!(room_lines(&h, b, from[1]), [], "Cara ignores Elara");
        assert_eq!(answer(&mut fed, h.now.0), [(SayOutcome::Said, None)]);

        ask(&mut fed, &mut f, h.now.0, "Nobody@dawnsong");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(
            answer(&mut fed, h.now.0),
            [(SayOutcome::Refused, Some("no_character".to_owned()))]
        );

        h.server
            .store
            .set_gag(elara, Some(600.0))
            .expect("gag kept");
        ask(&mut fed, &mut f, h.now.0, "Elara@dawnsong");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(
            answer(&mut fed, h.now.0),
            [(SayOutcome::Refused, Some("gagged".to_owned()))]
        );
        h.server.store.set_gag(elara, None).expect("gag lifted");

        let account = h
            .server
            .store
            .character(elara)
            .expect("reads")
            .expect("Elara")
            .account;
        let ban = yalain_store::Ban {
            at: 0,
            until: i64::MAX,
            by: "the console".into(),
            reason: None,
        };
        h.server
            .store
            .set_account_ban(account, Some(&ban))
            .expect("ban kept");
        ask(&mut fed, &mut f, h.now.0, "Elara@dawnsong");
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        assert_eq!(
            answer(&mut fed, h.now.0),
            [(SayOutcome::Refused, Some("banned".to_owned()))]
        );
    }

    /// An admin's `@dn verify <token>` said in game goes to the network's half as its
    /// character's, and is answered in its chat window at once; the hub's answer comes to the
    /// same window while the character is in the world, and to the log otherwise. A player's is
    /// a word the friends service does not know: its help. With the network off, either is told
    /// the world is not on it.
    /// Behaviour: server.network.verify-in-game-for-admins
    #[test]
    fn an_admins_dn_verify_said_in_game_asks_the_hub_and_is_answered_in_its_chat_window() {
        use yalain_federation::{Settings, VerifyRequest, ASKING, ON_ITS_WAY, WORLD_OFF};
        const TOKEN: &str = "dnv_abcdefghijklmnopqrstuvwxyzAB-_01";
        let mut h = Harness::new();
        let service = Service::new(yalain_friends::Config::new("Dawnsong", "dawnsong"));
        let mut f = Friends::start(service, None).expect("no listener");
        let mut fed = Federation::new(
            Settings {
                enabled: true,
                slug: "dawnsong".into(),
                software: "yalain test".into(),
                ..Settings::default()
            },
            1,
        );
        let (a, elara) = h.player_in_world("one", "Elara");
        let (b, cara) = h.player_in_world("two", "Cara");
        let said = |h: &mut Harness, b: usize, character: u64, name: &str, admin: bool| {
            let world = world_of(&h.server, character).expect("in the world");
            h.server.network_commands.push(NetworkCommand {
                world,
                character,
                name: name.into(),
                words: format!("verify {TOKEN}"),
                admin,
            });
            h.bots[b].inbox.len()
        };
        let from = [
            said(&mut h, a, elara, "Elara", true),
            said(&mut h, b, cara, "Cara", false),
        ];
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        h.run(20);
        assert_eq!(h.bots[a].notices(from[0]), [ASKING]);
        assert_eq!(
            h.bots[b].notices(from[1]),
            [yalain_friends::HELP.join("\n")],
            "a player's is the friends service's"
        );
        assert_eq!(
            fed.take_verify_requests(),
            [VerifyRequest {
                token: TOKEN.into(),
                by: "Elara".into(),
                who: Some(elara),
            }]
        );
        let from = said(&mut h, a, elara, "Elara", true);
        f.pump(at(&h), &mut h.server, &mut h.net, Some(&mut fed));
        show_operator(&h.server, &mut h.net, Some(elara), "The hub's answer.");
        show_operator(&h.server, &mut h.net, None, "The console's.");
        h.run(20);
        assert_eq!(
            h.bots[a].notices(from),
            [ON_ITS_WAY, "The hub's answer."],
            "one at a time, and the answer to its window"
        );
        let from = said(&mut h, a, elara, "Elara", true);
        f.pump(at(&h), &mut h.server, &mut h.net, None);
        h.run(20);
        assert_eq!(h.bots[a].notices(from), [WORLD_OFF]);
    }
}
