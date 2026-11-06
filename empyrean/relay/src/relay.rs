//! Every session's bridge over the later protocol's server, with no socket: a driver hands it
//! datagrams and sends what it returns.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use dereth_primitives::era::EraId;
use dereth_primitives::LocalTime;
use dereth_protocol::login::{CharacterError as CharacterErrorMessage, LoginAccountBooted};
use dereth_protocol::{write_blob, Message};
use empyrean_net::status_ping::{
    SecretSource, StatusFacts, StatusPing, StatusPingConfig, WorldState,
};
use empyrean_net::{
    CharacterError, ClockSnapshot, Event, NetAuthType, NetConfig, OutboundMessage, Outgoing,
    PortKind, ServerNet, SessionId, SessionState, SessionTerminationReason, TransportMessages,
};

use crate::bridge::{check_credentials, Bridge};
use crate::down;

/// The name the status ping gives this software.
pub const SOFTWARE: &str = "ac1999 relay";

/// What a relay is set up with.
#[derive(Debug, Clone)]
pub struct Config {
    /// The later protocol's game port.
    pub port: u16,
    /// The ac1999 server.
    pub upstream: SocketAddr,
    /// The world's name for Dereth's overlay folder and its status.
    pub world: String,
    /// Seed for the later protocol's cookies and key seeds.
    pub seed: u64,
}

/// One datagram for a server, and the session whose socket sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToServer {
    pub session: SessionId,
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}

fn encoded<M: Message>(message: &M) -> OutboundMessage {
    down::plain(write_blob(message).expect("the transport's own messages encode"))
}

fn character_error(error: CharacterError) -> OutboundMessage {
    encoded(&CharacterErrorMessage {
        char_error: error as u32,
    })
}

fn boot_account(reason: Option<&str>) -> OutboundMessage {
    encoded(&LoginAccountBooted {
        reason: reason.map(str::to_owned),
    })
}

/// What a finished session did, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub upstream: down::Tally,
    pub forwarded: BTreeMap<u32, down::Count>,
    pub refused: BTreeMap<u32, down::Count>,
    pub delivered: BTreeMap<u32, down::Count>,
}

impl Finished {
    /// A session refused before it had a bridge: nothing was exchanged.
    fn refused() -> Self {
        Self {
            upstream: down::Tally::default(),
            forwarded: BTreeMap::new(),
            refused: BTreeMap::new(),
            delivered: BTreeMap::new(),
        }
    }

    fn of(b: &Bridge) -> Self {
        Self {
            upstream: b.tally().clone(),
            forwarded: b.forwarded.clone(),
            refused: b.refused.clone(),
            delivered: b.delivered.clone(),
        }
    }
}

/// The relay.
#[derive(Debug)]
pub struct Relay {
    config: Config,
    net: ServerNet,
    bridges: BTreeMap<SessionId, Bridge>,
    /// Sessions whose credentials the 1999 login cannot hold, and why, until their handshake ends.
    refusing: BTreeMap<SessionId, String>,
    to_server: Vec<ToServer>,
    finished: Vec<Finished>,
}

impl Relay {
    /// A relay answering the status ping as a Launch world.
    #[must_use]
    pub fn new(config: Config, fresh_secret: SecretSource) -> Self {
        let mut net = ServerNet::new(
            NetConfig {
                port: config.port,
                rng_seed: config.seed,
                ..NetConfig::default()
            },
            TransportMessages {
                character_error,
                boot_account,
            },
        );
        let era = EraId::Launch;
        net.status_ping = Some(StatusPing::new(
            StatusPingConfig::default(),
            StatusFacts {
                state: WorldState::Open,
                players: 0,
                era: era.name().to_owned(),
                features: era.features(),
                software: SOFTWARE.to_owned(),
                software_version: env!("CARGO_PKG_VERSION").to_owned(),
                world_name: config.world.clone(),
            },
            fresh_secret,
        ));
        Self {
            config,
            net,
            bridges: BTreeMap::new(),
            refusing: BTreeMap::new(),
            to_server: Vec::new(),
            finished: Vec::new(),
        }
    }

    /// The later protocol's server.
    pub fn net(&mut self) -> &mut ServerNet {
        &mut self.net
    }

    /// The sessions with a bridge.
    pub fn sessions(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.bridges.keys().copied()
    }

    /// A session's bridge.
    #[must_use]
    pub fn bridge(&self, session: SessionId) -> Option<&Bridge> {
        self.bridges.get(&session)
    }

    /// One datagram from Dereth.
    pub fn on_later_datagram(
        &mut self,
        kind: PortKind,
        from: SocketAddr,
        bytes: &[u8],
        now: ClockSnapshot,
    ) {
        self.net.on_datagram(kind, from, bytes, now);
    }

    /// One datagram from a session's server.
    pub fn on_server_datagram(
        &mut self,
        session: SessionId,
        from: SocketAddr,
        bytes: &[u8],
        now: ClockSnapshot,
    ) {
        if let Some(b) = self.bridges.get_mut(&session) {
            b.from_server(bytes, from, local(now));
        }
    }

    fn on_event(&mut self, event: Event, now: ClockSnapshot) {
        match event {
            Event::LoginRequest {
                session, request, ..
            } => self.on_login(session, &request, now),
            Event::ConnectResponse { session } => {
                self.net.set_state(session, SessionState::AuthConnected);
                if let Some(why) = self.refusing.get(&session).cloned() {
                    self.net.terminate(
                        session,
                        SessionTerminationReason::NotAuthorizedPasswordMismatch,
                        Some(boot_account(Some(&why))),
                        String::new(),
                        now,
                    );
                    return;
                }
                if let Some(b) = self.bridges.get_mut(&session) {
                    b.on_connected();
                }
            }
            Event::Message {
                session, message, ..
            } => {
                if let Some(b) = self.bridges.get_mut(&session) {
                    b.from_dereth(&message.data, local(now));
                }
            }
            Event::SpeedHack { session } => self.net.reset_echo_verification(session),
            Event::Disconnected { session, .. } => {
                if self.refusing.remove(&session).is_some() {
                    self.finished.push(Finished::refused());
                }
                if let Some(mut b) = self.bridges.remove(&session) {
                    b.on_disconnected();
                    for (bytes, to) in b.take_for_server() {
                        self.to_server.push(ToServer { session, to, bytes });
                    }
                    self.finished.push(Finished::of(&b));
                }
            }
        }
    }

    fn on_login(
        &mut self,
        session: SessionId,
        request: &empyrean_net::PacketInboundLoginRequest,
        now: ClockSnapshot,
    ) {
        let password = (request.net_auth_type == NetAuthType::AccountPassword)
            .then(|| request.password.clone())
            .flatten()
            .unwrap_or_default();
        if self.net.account_select_callback(session, now) == empyrean_net::AccountSelect::Terminated
        {
            return;
        }
        let id = u32::from(session.client_id) + 1;
        self.net
            .accept_login(session, id, request.account.clone(), 0);
        // A client hears no message before its handshake ends, so a refusal waits for that.
        if let Err(why) = check_credentials(&request.account, &password) {
            self.refusing.insert(session, why);
            return;
        }
        self.bridges.insert(
            session,
            Bridge::new(
                self.config.upstream,
                &self.config.world,
                &request.account,
                &password,
            ),
        );
    }

    /// One turn: the later sessions' timers, what happened on them, the bridges' timers, and what
    /// each bridge has to send.
    pub fn step(&mut self, now: ClockSnapshot) {
        let events: Vec<Event> = self.net.events().collect();
        for event in events {
            self.on_event(event, now);
        }
        let t = local(now);
        let mut ended = Vec::new();
        for (session, b) in &mut self.bridges {
            b.tick(t);
            for message in b.take_for_dereth() {
                self.net.send(*session, message);
            }
            for (bytes, to) in b.take_for_server() {
                self.to_server.push(ToServer {
                    session: *session,
                    to,
                    bytes,
                });
            }
            if let Some(ending) = b.take_ending() {
                ended.push((*session, ending));
            }
        }
        for (session, ending) in ended {
            let message = boot_account(Some(&ending.reason));
            self.net.terminate(
                session,
                SessionTerminationReason::WorldClosed,
                Some(message),
                String::new(),
                now,
            );
        }
        self.net.do_session_work(now);
        let events: Vec<Event> = self.net.events().collect();
        for event in events {
            self.on_event(event, now);
        }
    }

    /// Datagrams for Dereth.
    pub fn take_for_dereth(&mut self) -> Vec<Outgoing> {
        self.net.drain_outgoing().collect()
    }

    /// Datagrams for the servers.
    pub fn take_for_servers(&mut self) -> Vec<ToServer> {
        std::mem::take(&mut self.to_server)
    }

    /// What the sessions that ended did.
    pub fn take_finished(&mut self) -> Vec<Finished> {
        std::mem::take(&mut self.finished)
    }
}

fn local(now: ClockSnapshot) -> LocalTime {
    LocalTime(now.monotonic.as_secs_f64())
}
