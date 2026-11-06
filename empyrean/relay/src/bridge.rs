//! One Dereth session and the login it holds upstream, with no socket and no clock of its own.
//!
//! The order is the later client's, because the 1999 login needs what only it can say: Dereth is
//! accepted and asked what it holds; the relay logs in upstream reporting those iterations, collects
//! the server's push up to the character list, serves the push through the later download messages,
//! and only when Dereth has ended the download passes it the character list.

use std::collections::VecDeque;
use std::net::SocketAddr;

use dereth_primitives::LocalTime;
use dereth_protocol::{
    admin::{DddBeginDdd, DddError, DddInterrogationResponse, DddOverlayManifest, DddRequestData},
    login::LoginEnterGameServerReady,
    read_body, write_blob, Message,
};
use empyrean_net::OutboundMessage;

use crate::codec::{dat::CellRequest, News, Roster};
use crate::dats::{self, ClientFiles, Push};
use crate::down::{self, Down, Out};
use crate::handshake::{self, Handshake, Reply};
use crate::up::{self, Up, UpState};
use crate::wire::{self, connection, LoginMetadata};

/// The acknowledgment of a grant is repeated this often until the first ordinary packet.
const ACK_RESEND: f64 = 1.0 / 3.0;
/// The error a record nobody can send is refused with: ACE's `DDDHandler` writes 1 here, an enum it
/// leaves unnamed, and the client answers it by being free to ask again.
const DATA_NOT_AVAILABLE: u32 = 1;
/// Bytes of download passed to the later transport in one step. It has no pacing of its own, and the
/// first message of a burst must not be lost to a full socket buffer.
const SERVE_BYTES_PER_STEP: usize = 256 * 1024;

/// Whether `account:password` fits the 1999 login, which holds twenty lowercased bytes.
///
/// # Errors
/// The words Dereth's player reads.
pub fn check_credentials(account: &str, password: &str) -> Result<(), String> {
    wire::account_text(account, password)
        .map(drop)
        .map_err(|_| {
            format!(
                " because this world's login holds at most {} characters of account:password, \
             in plain letters and numbers",
                wire::ACCOUNT_CHARS
            )
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Asked Dereth what it holds.
    Asked,
    /// Logging in upstream and collecting the push.
    Collecting,
    /// Serving the push.
    Serving,
    /// Dereth has its files: everything passes.
    Open,
    /// Over.
    Ended,
}

/// Why the session must end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
    /// The words after "because" in the message Dereth shows.
    pub reason: String,
}

/// One session's bridge. Debug formatting never prints the account's password.
pub struct Bridge {
    upstream: SocketAddr,
    world_key: String,
    account: String,
    password: String,
    phase: Phase,
    asked_again: bool,
    handshake: Option<Handshake>,
    /// The world's clock when its grant arrived.
    world_time: Option<f64>,
    connection: Option<connection::Connection>,
    last_ack: Option<LocalTime>,
    client: ClientFiles,
    push: Push,
    roster: Option<Roster>,
    news: Option<News>,
    serving: VecDeque<Vec<u8>>,
    begun: bool,
    down: Down,
    up: UpState,
    /// The later request a cell record answers: its type, oldest first.
    cell_requests: VecDeque<DddRequestData>,
    to_dereth: Vec<OutboundMessage>,
    to_server: Vec<(Vec<u8>, SocketAddr)>,
    ending: Option<Ending>,
    /// Records of the push held back from Dereth.
    pub withheld: usize,
    /// Sent to the server, by the opcode Dereth used.
    pub forwarded: std::collections::BTreeMap<u32, down::Count>,
    pub refused: std::collections::BTreeMap<u32, down::Count>,
    /// Sent to Dereth, by opcode.
    pub delivered: std::collections::BTreeMap<u32, down::Count>,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridge")
            .field("upstream", &self.upstream)
            .field("world_key", &self.world_key)
            .field("account", &self.account)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

fn tally(map: &mut std::collections::BTreeMap<u32, down::Count>, opcode: u32, bytes: usize) {
    let c = map.entry(opcode).or_default();
    c.messages += 1;
    c.bytes += bytes as u64;
}

impl Bridge {
    /// A bridge for a session that logged in as `account` with `password`, checked already.
    #[must_use]
    pub fn new(upstream: SocketAddr, world_key: &str, account: &str, password: &str) -> Self {
        Self {
            upstream,
            world_key: world_key.to_owned(),
            account: account.to_owned(),
            password: password.to_owned(),
            phase: Phase::Asked,
            asked_again: false,
            handshake: None,
            world_time: None,
            connection: None,
            last_ack: None,
            client: ClientFiles::default(),
            push: Push::default(),
            roster: None,
            news: None,
            serving: VecDeque::new(),
            begun: false,
            down: Down::default(),
            up: UpState::default(),
            cell_requests: VecDeque::new(),
            to_dereth: Vec::new(),
            to_server: Vec::new(),
            ending: None,
            withheld: 0,
            forwarded: std::collections::BTreeMap::new(),
            refused: std::collections::BTreeMap::new(),
            delivered: std::collections::BTreeMap::new(),
        }
    }

    /// What the translation has seen from the server.
    #[must_use]
    pub fn tally(&self) -> &down::Tally {
        &self.down.tally
    }

    /// Records that did not inflate.
    #[must_use]
    pub fn refused_records(&self) -> u32 {
        self.push.refused
    }

    /// Messages for Dereth since the last call.
    pub fn take_for_dereth(&mut self) -> Vec<OutboundMessage> {
        std::mem::take(&mut self.to_dereth)
    }

    /// Datagrams for the server since the last call.
    pub fn take_for_server(&mut self) -> Vec<(Vec<u8>, SocketAddr)> {
        if let Some(c) = &mut self.connection {
            self.to_server.extend(c.take_outgoing());
        }
        std::mem::take(&mut self.to_server)
    }

    /// Why the session must end, once.
    pub fn take_ending(&mut self) -> Option<Ending> {
        self.ending.take()
    }

    fn say(&mut self, message: OutboundMessage) {
        tally(&mut self.delivered, message.opcode(), message.data.len());
        self.to_dereth.push(message);
    }

    fn say_encoded<M: Message>(&mut self, message: &M) {
        match write_blob(message) {
            Ok(data) => {
                let out = self.down.later(data);
                self.say(out);
            }
            Err(e) => log::error!("a message for Dereth did not encode: {e}"),
        }
    }

    fn end(&mut self, reason: String) {
        if self.phase != Phase::Ended {
            self.phase = Phase::Ended;
            self.ending = Some(Ending { reason });
        }
    }

    /// The later handshake finished: ask Dereth what it holds.
    pub fn on_connected(&mut self) {
        let ask = dats::interrogation();
        self.say_encoded(&ask);
    }

    /// Dereth's session ended. The server hears of it and the relay holds nothing more for it.
    pub fn on_disconnected(&mut self) {
        if let Some(c) = &mut self.connection {
            self.to_server.extend(c.goodbye());
            c.close();
        }
        self.phase = Phase::Ended;
    }

    /// One message from Dereth.
    pub fn from_dereth(&mut self, bytes: &[u8], now: LocalTime) {
        let opcode = bytes
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u32::from_le_bytes);
        match opcode {
            o if o == DddInterrogationResponse::OPCODE.0 => {
                self.on_answer(&bytes[4..], now);
                return;
            }
            // Dereth ended the download.
            0xF7EA => {
                self.on_download_ended();
                return;
            }
            o if o == DddRequestData::OPCODE.0 => {
                self.on_cell_request(&bytes[4..], now);
                return;
            }
            _ => {}
        }
        // A game action is counted by the action, not by the envelope around every one of them.
        let opcode = if opcode == 0xF7B1 {
            bytes
                .get(8..12)
                .and_then(|b| b.try_into().ok())
                .map_or(opcode, u32::from_le_bytes)
        } else {
            opcode
        };
        match self.up.message(bytes) {
            Ok(Up::Send { queue, bytes: out }) => {
                tally(&mut self.forwarded, opcode, out.len());
                self.send_up(queue, &out, now);
                if opcode == up::LOGIN_COMPLETE {
                    self.after_login(now);
                }
            }
            Ok(Up::Refuse(text)) => {
                tally(&mut self.refused, opcode, bytes.len());
                let line = down::system_line(text);
                self.say(line);
            }
            Ok(Up::Drop) => tally(&mut self.refused, opcode, bytes.len()),
            Err(e) => {
                tally(&mut self.refused, opcode, bytes.len());
                log::warn!("a message from Dereth ({opcode:#06X}) did not read: {e}");
            }
        }
    }

    /// A character came into the world. The server's squelch list is rebuilt from what the client
    /// holds, which outlives the character for as long as the session lasts.
    fn after_login(&mut self, now: LocalTime) {
        let ids = self.down.squelched_ids();
        if let Up::Send { queue, bytes } = self.up.squelch_upload(&ids) {
            self.send_up(queue, &bytes, now);
        }
        if let Some(list) = self.down.squelch_list() {
            self.say(list);
        }
    }

    fn send_up(&mut self, queue: u16, bytes: &[u8], now: LocalTime) {
        let Some(c) = self.connection.as_mut() else {
            log::warn!("a message for the server with no connection up");
            return;
        };
        if let Err(e) = c.send(queue, bytes, now) {
            log::warn!("a message for the server was not sent: {e}");
        }
    }

    /// Dereth's answer to the interrogation.
    fn on_answer(&mut self, body: &[u8], now: LocalTime) {
        if self.phase != Phase::Asked {
            return;
        }
        let answer = match read_body::<DddInterrogationResponse>(body) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("Dereth's answer to the interrogation did not read: {e}");
                self.end(" because the relay could not read your client's data files".to_owned());
                return;
            }
        };
        // A client keeping a folder for each world answered for the world it played last here. It is
        // told this world's name and asked again, once.
        let ours = dereth_dat::overlay::world_hash(&self.world_key);
        if answer.world().is_some_and(|w| w != ours) && !self.asked_again {
            self.asked_again = true;
            self.say_encoded(&DddOverlayManifest {
                world_key: self.world_key.clone(),
                total_bytes: 0,
                files: Vec::new(),
            });
            let ask = dats::interrogation();
            self.say_encoded(&ask);
            return;
        }
        self.client = ClientFiles::of(&answer);
        log::info!(
            "Dereth holds portal {} and cell {}; logging in upstream",
            self.client.portal,
            self.client.cell
        );
        if self.client.portal == 0 || self.client.cell == 0 {
            log::warn!("Dereth named no iteration for a data file; not logging in upstream");
            self.password = String::new();
            self.end(" because your client didn't say which data files it holds".to_owned());
            return;
        }
        let metadata = LoginMetadata {
            portal_iteration: self.client.portal,
            cell_iteration: self.client.cell,
        };
        match Handshake::new(self.upstream, &self.account, &self.password, metadata) {
            Ok(h) => {
                self.handshake = Some(h);
                self.phase = Phase::Collecting;
                self.password = String::new();
                self.tick(now);
            }
            Err(e) => {
                log::warn!("the upstream login could not start: {e}");
                self.end(" because this world's login could not carry your account".to_owned());
            }
        }
    }

    /// The push is whole: serve it, or nothing when there is nothing to serve.
    fn serve_push(&mut self) {
        let push = std::mem::take(&mut self.push);
        log::info!(
            "the server pushed {} record(s), {} held back, {} that did not inflate",
            push.held() + push.withheld.len() + push.refused as usize,
            push.withheld.len(),
            push.refused
        );
        self.withheld = push.withheld.len();
        let serve = push.serve(&self.world_key, &self.client);
        self.say_encoded(&serve.manifest);
        match serve.begin {
            Some(begin) => {
                self.begin_download(&begin);
                for record in &serve.data {
                    match write_blob(record) {
                        Ok(blob) => self.serving.push_back(blob),
                        Err(e) => log::error!("a record for Dereth did not encode: {e}"),
                    }
                }
                self.phase = Phase::Serving;
            }
            None => {
                // Up to date: the end at once, which Dereth answers in kind.
                self.send_end();
                self.open();
            }
        }
    }

    fn begin_download(&mut self, begin: &DddBeginDdd) {
        self.begun = true;
        self.say_encoded(begin);
    }

    fn send_end(&mut self) {
        self.say(down::plain(0xF7EAu32.to_le_bytes().to_vec()));
    }

    /// Dereth said its download is over.
    fn on_download_ended(&mut self) {
        if self.begun {
            self.begun = false;
            self.send_end();
        }
        if self.phase == Phase::Serving {
            self.open();
        }
    }

    /// The download is done: the character list and the news.
    fn open(&mut self) {
        self.phase = Phase::Open;
        if let Some(roster) = self.roster.take() {
            self.pass_roster(&roster);
        }
        let name = self.world_key.clone();
        self.say_encoded(&down::world_info(&name));
        if let Some(news) = self.news.take() {
            self.say_encoded(&down::screen_message(&news));
        }
    }

    fn pass_roster(&mut self, roster: &Roster) {
        self.up.account.clone_from(&roster.account);
        self.say_encoded(&down::character_set(roster));
    }

    /// A record the client lacks during play: ask the server.
    fn on_cell_request(&mut self, body: &[u8], now: LocalTime) {
        let Ok(request) = read_body::<DddRequestData>(body) else {
            return;
        };
        let Ok(blob) = write_blob(&CellRequest {
            id: request.resource_id,
            cache: dats::cell_cache(request.resource_id),
        }) else {
            return;
        };
        if self.connection.is_none() {
            self.say_encoded(&DddError {
                resource_type: request.resource_type,
                resource_id: request.resource_id,
                error: DATA_NOT_AVAILABLE,
            });
            return;
        }
        self.cell_requests.push_back(request);
        self.send_up(up::QUEUE_DATABASE, &blob, now);
    }

    /// One datagram from the server.
    pub fn from_server(&mut self, bytes: &[u8], from: SocketAddr, now: LocalTime) {
        if self.phase == Phase::Ended {
            return;
        }
        let grant_packet = wire::Packet::decode(bytes)
            .is_ok_and(|p| p.header.has(wire::flags::GRANT) && !p.body.is_empty());
        if self.connection.is_none() || grant_packet {
            self.handshake_datagram(bytes, from, now);
            return;
        }
        if let Some(c) = &mut self.connection {
            if let Err(e) = c.feed(bytes, from, now) {
                log::debug!("a datagram from the server was refused: {e}");
            }
        }
        self.pump(now);
    }

    /// The world's clock when its grant arrived, once it has.
    #[must_use]
    pub fn world_time(&self) -> Option<f64> {
        self.world_time
    }

    fn handshake_datagram(&mut self, bytes: &[u8], from: SocketAddr, now: LocalTime) {
        let Some(h) = self.handshake.as_mut() else {
            return;
        };
        match h.receive(bytes, from) {
            Ok(Reply::Granted { acknowledgment }) => {
                self.to_server
                    .push((acknowledgment.bytes().to_vec(), acknowledgment.destination));
                self.last_ack = Some(now);
                let grant = h.accepted().cloned();
                let keys = h.take_keys();
                if let (Some(grant), Some(keys)) = (grant, keys) {
                    self.world_time = Some(grant.world_epoch);
                    match connection::Connection::new(
                        from,
                        &grant,
                        keys.incoming,
                        keys.outgoing,
                        now,
                    ) {
                        Ok(c) => self.connection = Some(c),
                        Err(e) => {
                            log::error!("the upstream connection did not start: {e}");
                            self.end(" because the world did not answer properly".to_owned());
                        }
                    }
                }
            }
            Ok(Reply::Duplicate { acknowledgment }) => {
                self.to_server
                    .push((acknowledgment.bytes().to_vec(), acknowledgment.destination));
            }
            Ok(Reply::Refused { .. }) => {
                self.end(" because the world refused your account or password".to_owned());
            }
            Err(handshake::Error::Packet(e)) => log::debug!("a login answer did not read: {e}"),
            Err(e) => log::debug!("a login answer was refused: {e}"),
        }
    }

    fn pump(&mut self, now: LocalTime) {
        while let Some(event) = self
            .connection
            .as_mut()
            .and_then(connection::Connection::poll)
        {
            match event {
                connection::Event::Message(m) => {
                    for out in self.down.message(&m.bytes) {
                        self.apply(out, now);
                    }
                }
                connection::Event::Closed => {
                    self.end(" because the world closed the connection".to_owned());
                }
                connection::Event::Connected => self.last_ack = None,
                connection::Event::ServerSwitch { .. } => {}
            }
        }
    }

    fn apply(&mut self, out: Out, now: LocalTime) {
        match out {
            Out::Later(message) => self.say(message),
            Out::Roster(roster) => {
                self.up.account.clone_from(&roster.account);
                match self.phase {
                    Phase::Open => self.pass_roster(&roster),
                    Phase::Collecting => {
                        self.roster = Some(roster);
                        self.serve_push();
                    }
                    Phase::Serving => self.roster = Some(roster),
                    _ => {}
                }
            }
            Out::News(news) => {
                if self.phase == Phase::Open {
                    self.say_encoded(&down::screen_message(&news));
                } else {
                    self.news = Some(news);
                }
            }
            Out::Begin(begin) => self.push.begin(begin),
            Out::Record(record) => self.push.record(&record),
            Out::Touch(_) => {}
            Out::CellRecord(record) => {
                let asked = self.cell_requests.pop_front();
                let resource_type = asked.map_or(0, |r| r.resource_type);
                if let Some(data) = dats::answer_cell(resource_type, &record, self.client.cell) {
                    self.say_encoded(&data);
                }
            }
            Out::CellPair(pair) => {
                let asked = self.cell_requests.pop_front();
                let resource_type = asked.map_or(0, |r| r.resource_type);
                for data in dats::answer_pair(resource_type, &pair, self.client.cell) {
                    self.say_encoded(&data);
                }
            }
            Out::EntryReady => self.say_encoded(&LoginEnterGameServerReady),
            Out::Options(options) => self.up.options = options,
            Out::Question => self.up.questions += 1,
        }
        let _ = now;
    }

    /// One turn of the timers.
    pub fn tick(&mut self, now: LocalTime) {
        if self.phase == Phase::Ended {
            return;
        }
        if self.connection.is_none() {
            if let Some(h) = self.handshake.as_mut() {
                match h.request(now) {
                    Ok(Some(request)) => {
                        self.to_server
                            .push((request.bytes().to_vec(), request.destination));
                    }
                    Ok(None) => {}
                    Err(handshake::Error::Timeout) => {
                        self.end(" because the world did not answer".to_owned());
                    }
                    Err(e) => log::debug!("the upstream login: {e}"),
                }
            }
            return;
        }
        if let (Some(last), Some(h)) = (self.last_ack, self.handshake.as_ref()) {
            if now.0 - last.0 >= ACK_RESEND {
                if let Some(ack) = h.acknowledgment() {
                    self.to_server.push((ack.bytes().to_vec(), ack.destination));
                }
                self.last_ack = Some(now);
            }
        }
        if let Some(c) = &mut self.connection {
            if let Err(e) = c.tick(now) {
                log::debug!("the upstream timers: {e}");
            }
        }
        self.pump(now);
        let mut budget = SERVE_BYTES_PER_STEP;
        while budget > 0 {
            let Some(blob) = self.serving.pop_front() else {
                break;
            };
            budget = budget.saturating_sub(blob.len());
            self.say(down::plain(blob));
        }
    }

    /// Whether the session is over from this side.
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.phase == Phase::Ended
    }
}

#[cfg(test)]
mod tests;
