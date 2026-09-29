// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Managers/NetworkManager.cs

use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};

use dereth_transport::wire::PacketFlags;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::{CsCast, DotNetDateTime};

use crate::client_packet::ClientPacket;
use crate::enums::{
    CharacterError, SessionState, SessionTerminationPhase, SessionTerminationReason,
};
use crate::handlers::authentication_handler;
pub use crate::handlers::authentication_handler::AccountSelect;
use crate::network_session::NetworkSession;
use crate::network_statistics::NetworkStatistics;
use crate::packets::packet_inbound_connect_response::PacketInboundConnectResponse;
use crate::packets::packet_outbound_connect_request::packet_outbound_connect_request;
use crate::session::{NetIo, Session, SessionCore};
use crate::session_connection_data::SessionRandom;
use crate::transport_messages::TransportMessages;
use crate::{Event, OutboundMessage, Outgoing, PortKind, SessionId};

/// ACE `NetworkManager.ServerId`: "Hard coded server Id, this will need to change if we move to
/// multi-process or multi-server model".
pub const SERVER_ID: u16 = 0xB;

/// The network settings ACE reads from `Config.Server.Network`, with ACE's defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetConfig {
    /// `Port`: the client-to-server port `P`; the server-to-client port is `P + 1`.
    pub port: u16,
    /// `MaximumAllowedSessions` (128). It may exceed 256: a session's client id (its slot, and
    /// the header id on its client's packets) can then be 256 or more, which the server's receive
    /// path accepts (the client's 256-slot header-id limit applies only to what the client
    /// receives), as retail's servers did with ids up to 397 (V283).
    pub maximum_allowed_sessions: u32,
    /// `DefaultSessionTimeout`, seconds (60).
    pub default_session_timeout: u32,
    /// `MaximumAllowedSessionsPerIPAddress` (-1, unlimited).
    pub maximum_allowed_sessions_per_ip_address: i32,
    /// `AllowUnlimitedSessionsFromIPAddresses`.
    pub allow_unlimited_sessions_from_ip_addresses: Vec<IpAddr>,
    /// Seed for the seeds and cookies (see [`SessionRandom`]); the server binary draws it from
    /// entropy.
    pub rng_seed: u64,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            port: 9000,
            maximum_allowed_sessions: 128,
            default_session_timeout: 60,
            maximum_allowed_sessions_per_ip_address: -1,
            allow_unlimited_sessions_from_ip_addresses: Vec::new(),
            rng_seed: 0x0005_EED0_FACE,
        }
    }
}

/// ACE `NetworkManager` and every session it owns. See the crate documentation.
#[derive(Debug)]
pub struct ServerNet {
    pub config: NetConfig,
    session_map: Vec<Option<Session>>,
    generations: Vec<u32>,
    events: VecDeque<Event>,
    outgoing: Vec<Outgoing>,
    stats: NetworkStatistics,
    rand: SessionRandom,
    /// ACE `ServerManager.ShutdownInProgress`.
    pub shutdown_in_progress: bool,
    /// ACE `ServerManager.ShutdownInitiated` and `ShutdownTime`: `Some(t)` once a shutdown is
    /// scheduled for `t` (on the `utc` clock).
    pub shutdown_time: Option<DotNetDateTime>,
    /// The world's builders of the messages the transport sends (see [`TransportMessages`]).
    pub messages: TransportMessages,
}

/// Splits borrows so a session and the manager's sinks can be used together.
macro_rules! io {
    ($self:ident, $now:expr) => {
        NetIo {
            now: $now,
            outgoing: &mut $self.outgoing,
            events: &mut $self.events,
            stats: &mut $self.stats,
            default_session_timeout: $self.config.default_session_timeout,
            messages: $self.messages,
        }
    };
}

impl ServerNet {
    /// A transport with no sessions. `messages` builds the game messages it sends on its own.
    #[must_use]
    pub fn new(config: NetConfig, messages: TransportMessages) -> Self {
        let slots = config.maximum_allowed_sessions as usize;
        let rand = SessionRandom::new(config.rng_seed);
        Self {
            config,
            session_map: (0..slots).map(|_| None).collect(),
            generations: vec![0; slots],
            events: VecDeque::new(),
            outgoing: Vec::new(),
            stats: NetworkStatistics::default(),
            rand,
            shutdown_in_progress: false,
            shutdown_time: None,
            messages,
        }
    }

    // ------------------------------------------------------------------------------------------
    // The sans-IO surface.
    // ------------------------------------------------------------------------------------------

    /// ACE `NetworkManager.DoSessionWork`, then every datagram produced since the last call. A
    /// driver that needs `DoSessionWork`'s session count (the world loop's 1 ms or 10 ms sleep)
    /// calls [`do_session_work`](Self::do_session_work) and [`drain_outgoing`](Self::drain_outgoing)
    /// instead: the count includes sessions dropped in this pass, which
    /// [`get_session_count`](Self::get_session_count) no longer sees.
    pub fn poll(&mut self, now: ClockSnapshot) -> std::vec::Drain<'_, Outgoing> {
        self.do_session_work(now);
        self.drain_outgoing()
    }

    /// Every datagram produced since the last call, oldest first.
    pub fn drain_outgoing(&mut self) -> std::vec::Drain<'_, Outgoing> {
        self.outgoing.drain(..)
    }

    /// Everything the world has to act on, oldest first.
    pub fn events(&mut self) -> std::collections::vec_deque::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// ACE `NetworkSession.EnqueueSend(GameMessage)` for a live session; a stale handle is ignored,
    /// as ACE ignores sends to a released session.
    pub fn send(&mut self, session: SessionId, message: OutboundMessage) {
        if let Some(s) = self.session_mut(session) {
            s.network.enqueue_send(message);
        }
    }

    /// The transport half of ACE's `AccountSelectCallback`: see
    /// [`authentication_handler::account_select_callback`].
    pub fn account_select_callback(
        &mut self,
        session: SessionId,
        now: ClockSnapshot,
    ) -> AccountSelect {
        let Some(index) = self.resolve(session) else {
            return AccountSelect::Terminated;
        };
        let mut io = io!(self, now);
        let s = self.session_map[index].as_mut().expect("resolved");
        authentication_handler::account_select_callback(s, &mut io)
    }

    /// The world accepted the account: ACE's `SetAccount` and `State = AuthConnectResponse`.
    pub fn accept_login(
        &mut self,
        session: SessionId,
        account_id: u32,
        account: String,
        access_level: u32,
    ) {
        if let Some(s) = self.session_mut(session) {
            authentication_handler::accept_login(s, account_id, account, access_level);
        }
    }

    /// The world refused the account: ACE's `session.Terminate(reason, message)` from
    /// `AccountSelectCallback`.
    pub fn reject_login(
        &mut self,
        session: SessionId,
        reason: SessionTerminationReason,
        message: Option<OutboundMessage>,
        extra_reason: String,
        now: ClockSnapshot,
    ) {
        self.terminate(session, reason, message, extra_reason, now);
    }

    /// ACE `Session.Terminate(reason, message, null, extraReason)`.
    pub fn terminate(
        &mut self,
        session: SessionId,
        reason: SessionTerminationReason,
        message: Option<OutboundMessage>,
        extra_reason: String,
        now: ClockSnapshot,
    ) {
        if let Some(s) = self.session_mut(session) {
            s.terminate(reason, message, None, extra_reason, now.utc);
        }
    }

    /// ACE `session.State = ...` from the world (for example `WorldConnected` on entering the
    /// world, `AuthConnected` after logging off).
    pub fn set_state(&mut self, session: SessionId, state: SessionState) {
        if let Some(s) = self.session_mut(session) {
            s.core.state = state;
        }
    }

    /// See [`SessionCore::player_active`].
    pub fn set_player_active(&mut self, session: SessionId, active: bool) {
        if let Some(s) = self.session_mut(session) {
            s.core.player_active = active;
        }
    }

    /// ACE `Session.SetAccessLevel`.
    pub fn set_access_level(&mut self, session: SessionId, access_level: u32) {
        if let Some(s) = self.session_mut(session) {
            s.core.set_access_level(access_level);
        }
    }

    /// The end of the speed-hack action chain (see [`Event::SpeedHack`]).
    pub fn reset_echo_verification(&mut self, session: SessionId) {
        if let Some(s) = self.session_mut(session) {
            s.network.reset_echo_verification();
        }
    }

    /// A driver's `SendTo` failed: ACE's `catch (SocketException)` in `SendPacketRaw`.
    pub fn on_send_error(&mut self, session: SessionId, error: &str, now: ClockSnapshot) {
        log::error!("SendTo failed for session {session:?}: {error}");
        if let Some(s) = self.session_mut(session) {
            s.terminate(
                SessionTerminationReason::SendToSocketException,
                None,
                None,
                error.to_string(),
                now.utc,
            );
        }
    }

    /// The network counters.
    #[must_use]
    pub const fn statistics(&self) -> &NetworkStatistics {
        &self.stats
    }

    /// A live session's transport half, for the world and for tests.
    #[must_use]
    pub fn session(&self, session: SessionId) -> Option<&Session> {
        self.resolve(session)
            .and_then(|i| self.session_map[i].as_ref())
    }

    /// Every live session, in slot order.
    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.session_map.iter().flatten()
    }

    fn resolve(&self, session: SessionId) -> Option<usize> {
        let i = usize::from(session.client_id);
        let s = self.session_map.get(i)?.as_ref()?;
        (s.core.id == session).then_some(i)
    }

    fn session_mut(&mut self, session: SessionId) -> Option<&mut Session> {
        let i = self.resolve(session)?;
        self.session_map[i].as_mut()
    }

    // ------------------------------------------------------------------------------------------
    // ACE NetworkManager.
    // ------------------------------------------------------------------------------------------

    // ACE: NetworkManager.ProcessPacket
    /// Routes one unpacked packet by listener: `P + 1` takes only the `ConnectResponse` (and
    /// ignores `CICMDCommand`); `P` takes login requests and session traffic.
    pub fn process_packet(
        &mut self,
        listener: PortKind,
        packet: ClientPacket,
        end_point: SocketAddr,
        now: ClockSnapshot,
    ) {
        if listener == PortKind::S2C {
            if packet.has_flag(PacketFlags::CONNECT_RESPONSE) {
                let Some(connect_response) = PacketInboundConnectResponse::new(&packet) else {
                    return;
                };
                // "This should be set on the second packet to the server from the client. This
                // completes the three-way handshake."
                let found = self.session_map.iter().position(|k| {
                    k.as_ref().is_some_and(|k| {
                        k.core.state == SessionState::AuthConnectResponse
                            && k.network.connection_data.connection_cookie == connect_response.check
                            && k.core.end_point_c2s.ip() == end_point.ip()
                    })
                });
                if let Some(i) = found {
                    let session = self.session_map[i].as_mut().expect("found");
                    session.core.set_s2c_endpoint(end_point);
                    session.core.state = SessionState::AuthConnected;
                    session.network.discard_sent_connect_request();
                    session.network.send_resync = true;
                    // `AuthenticationHandler.HandleConnectResponse` (world status and the character
                    // list) runs in the world on this event.
                    self.events.push_back(Event::ConnectResponse {
                        session: session.core.id,
                    });
                }
            } else if packet.header.rec_id == 0 && packet.has_flag(PacketFlags::CICMD_COMMAND) {
                // "TODO: Not sure what to do with these packets yet"
            } else {
                log::error!(
                    "Packet from {end_point} rejected. Packet sent to listener 1 and is not a ConnectResponse or CICMDCommand"
                );
            }
            return;
        }

        if packet.has_flag(PacketFlags::LOGIN_REQUEST) {
            // Not ACE's (a fix): the client re-sends its LoginRequest
            // every 2 s until the ConnectRequest arrives. A copy for the login this endpoint
            // already has in progress (or answered, or completed) is the same login; ACE started
            // a second login on it, or dropped the answered session as a bad handshake, either
            // way aborting the login. It is handled here, before the capacity checks, so a copy
            // never draws a reject either. Retail ignored the copy: a lost
            // ConnectRequest is recovered by the server's own one-second re-send.
            if let Some(session) = self.session_map.iter().flatten().find(|s| {
                s.core.end_point_c2s == end_point
                    && authentication_handler::is_repeated_login_request(
                        &packet,
                        s.core.login_request.as_ref(),
                    )
            }) {
                authentication_handler::repeated_login_request(session);
                return;
            }
            let max = self.config.maximum_allowed_sessions as usize;
            if self.get_authenticated_session_count() >= max {
                log::info!("Login Request from {end_point} rejected. Server full.");
                self.send_login_request_reject_to(
                    listener,
                    end_point,
                    CharacterError::LogonServerFull,
                    now,
                );
            } else if self.shutdown_in_progress {
                log::info!("Login Request from {end_point} rejected. Server is shutting down.");
                self.send_login_request_reject_to(
                    listener,
                    end_point,
                    CharacterError::ServerCrash1,
                    now,
                );
            } else if self
                .shutdown_time
                .is_some_and(|t| (t - now.utc).total_minutes() < 2.0)
            {
                log::info!("Login Request from {end_point} rejected. Server shutting down in less than 2 minutes.");
                self.send_login_request_reject_to(
                    listener,
                    end_point,
                    CharacterError::ServerCrash1,
                    now,
                );
            } else {
                let ip_allows_unlimited = self
                    .config
                    .allow_unlimited_sessions_from_ip_addresses
                    .contains(&end_point.ip());
                let per_ip = self.config.maximum_allowed_sessions_per_ip_address;
                if ip_allows_unlimited
                    || per_ip == -1
                    || i64::try_from(
                        self.get_session_endpoint_total_by_address_count(end_point.ip()),
                    )
                    .unwrap_or(i64::MAX)
                        < i64::from(per_ip)
                {
                    if let Some(i) = self.find_or_create_session(end_point, now) {
                        let mut io = io!(self, now);
                        let bad_handshake = self.session_map[i]
                            .as_ref()
                            .is_some_and(|s| s.core.state == SessionState::AuthConnectResponse);
                        if bad_handshake {
                            // "connect request packet sent to the client was corrupted in transit
                            // and session entered an unspecified state. ignore the request and
                            // remove the broken session and the client will start a new session."
                            log::warn!("Bad handshake from {end_point}, aborting session.");
                            let mut removed = self.session_map[i].take().expect("present");
                            // ACE goes on to call `session.ProcessPacket(packet)` on the removed
                            // session; its `CheckState` refuses a LoginRequest in this state, so
                            // that call does nothing.
                            removed.process_packet(packet, &mut io);
                            // DIVERGE: ACE just forgets the session (no `DropSession`, so no
                            // log-off). The world is told, so it can release the account.
                            io.events.push_back(Event::Disconnected {
                                session: removed.core.id,
                                reason: SessionTerminationReason::BadHandshake,
                                extra_reason: String::new(),
                                account: removed.core.account.clone(),
                            });
                            return;
                        }
                        let session = self.session_map[i].as_mut().expect("created");
                        session.process_packet(packet, &mut io);
                    } else {
                        log::info!("Login Request from {end_point} rejected. Failed to find or create session.");
                        self.send_login_request_reject_to(
                            listener,
                            end_point,
                            CharacterError::LogonServerFull,
                            now,
                        );
                    }
                } else {
                    log::info!(
                        "Login Request from {end_point} rejected. Session would exceed MaximumAllowedSessionsPerIPAddress limit."
                    );
                    self.send_login_request_reject_to(
                        listener,
                        end_point,
                        CharacterError::LogonServerFull,
                        now,
                    );
                }
            }
        } else if self.session_map.len() > usize::from(packet.header.rec_id) {
            let i = usize::from(packet.header.rec_id);
            let mut io = io!(self, now);
            if let Some(session) = self.session_map[i].as_mut() {
                // Keyed by the full source address, port included.
                if session.core.end_point_c2s == end_point {
                    session.process_packet(packet, &mut io);
                } else {
                    log::debug!(
                        "Session for Id {i} has IP {} but packet has IP {end_point}",
                        session.core.end_point_c2s
                    );
                }
            } else {
                log::debug!("Unsolicited Packet from {end_point} with Id {i}");
            }
        } else {
            log::debug!(
                "Unsolicited Packet from {end_point} with Id {}",
                packet.header.rec_id
            );
        }
    }

    // ACE: NetworkManager.SendLoginRequestReject
    /// The listener overload: a throwaway session outside the map (`ClientId` one past its end)
    /// sends the `ConnectRequest` and the error.
    fn send_login_request_reject_to(
        &mut self,
        listener: PortKind,
        end_point: SocketAddr,
        error: CharacterError,
        now: ClockSnapshot,
    ) {
        let _ = listener;
        // `(ushort)(sessionMap.Length + 1)`.
        let client_id: u16 =
            (i32::try_from(self.session_map.len()).unwrap_or(i32::MAX) + 1).cs_cast();
        let id = SessionId {
            client_id,
            generation: 0,
        };
        let mut temp_session =
            Session::new(id, false, end_point, SERVER_ID, now.utc, &mut self.rand);
        let mut io = io!(self, now);
        send_login_request_reject(
            &mut temp_session.network,
            &mut temp_session.core,
            error,
            &mut io,
        );
    }

    // ACE: NetworkManager.GetSessionCount
    #[must_use]
    pub fn get_session_count(&self) -> usize {
        self.session_map.iter().flatten().count()
    }

    // ACE: NetworkManager.GetAuthenticatedSessionCount
    #[must_use]
    pub fn get_authenticated_session_count(&self) -> usize {
        self.session_map
            .iter()
            .flatten()
            .filter(|s| s.core.account_id != 0)
            .count()
    }

    // ACE: NetworkManager.GetUniqueSessionEndpointCount
    #[must_use]
    pub fn get_unique_session_endpoint_count(&self) -> usize {
        let mut ips: Vec<IpAddr> = self
            .session_map
            .iter()
            .flatten()
            .map(|s| s.core.end_point_c2s.ip())
            .collect();
        ips.sort();
        ips.dedup();
        ips.len()
    }

    // ACE: NetworkManager.GetSessionEndpointTotalByAddressCount
    #[must_use]
    pub fn get_session_endpoint_total_by_address_count(&self, address: IpAddr) -> usize {
        self.session_map
            .iter()
            .flatten()
            .filter(|s| s.core.end_point_c2s.ip() == address)
            .count()
    }

    // ACE: NetworkManager.FindOrCreateSession
    /// The session whose client-to-server endpoint is exactly `end_point` (address **and** port),
    /// or a new one in the first free slot; `None` when every slot is taken.
    pub fn find_or_create_session(
        &mut self,
        end_point: SocketAddr,
        now: ClockSnapshot,
    ) -> Option<usize> {
        if let Some(i) = self.session_map.iter().position(|s| {
            s.as_ref()
                .is_some_and(|s| s.core.end_point_c2s == end_point)
        }) {
            return Some(i);
        }
        let i = self.session_map.iter().position(Option::is_none)?;
        log::debug!("Creating new session for {end_point} with id {i}");
        self.generations[i] = self.generations[i].wrapping_add(1);
        let id = SessionId {
            client_id: u16::try_from(i).ok()?,
            generation: self.generations[i],
        };
        self.session_map[i] = Some(Session::new(
            id,
            true,
            end_point,
            SERVER_ID,
            now.utc,
            &mut self.rand,
        ));
        Some(i)
    }

    // ACE: NetworkManager.Find
    /// By account id (the `uint` overload).
    #[must_use]
    pub fn find_by_account_id(&self, account_id: u32) -> Option<SessionId> {
        self.session_map
            .iter()
            .flatten()
            .find(|s| s.core.account_id == account_id)
            .map(|s| s.core.id)
    }

    // ACE: NetworkManager.Find
    /// By account name (the `string` overload).
    #[must_use]
    pub fn find_by_account(&self, account: &str) -> Option<SessionId> {
        self.session_map
            .iter()
            .flatten()
            .find(|s| s.core.account.as_deref() == Some(account))
            .map(|s| s.core.id)
    }

    // ACE: NetworkManager.RemoveSession
    fn remove_session(&mut self, index: usize) -> Option<Session> {
        self.session_map.get_mut(index)?.take()
    }

    // ACE: NetworkManager.DoSessionWork
    /// Ticks every session outbound, then drops the ones whose termination has finished.
    ///
    /// DIVERGE: ACE ticks sessions with `Parallel.ForEach`; here they tick in slot order. Sessions
    /// share nothing, so only the interleaving of their datagrams differs.
    pub fn do_session_work(&mut self, now: ClockSnapshot) -> usize {
        let mut io = io!(self, now);
        for session in self.session_map.iter_mut().flatten() {
            session.tick_outbound(&mut io);
        }

        let mut session_count = 0;
        for i in 0..self.session_map.len() {
            let completed = self.session_map[i].as_ref().is_some_and(|s| {
                s.core.pending_termination.as_ref().is_some_and(|p| {
                    p.termination_status == SessionTerminationPhase::SessionWorkCompleted
                })
            });
            if completed {
                self.drop_session(i);
            }
            if self.session_map[i].is_some() || completed {
                session_count += 1;
            }
        }
        session_count
    }

    // ACE: Session.DropSession
    /// Removes a session whose termination has finished and tells the world. `LogOffPlayer` is
    /// the world's, on the [`Event::Disconnected`].
    fn drop_session(&mut self, index: usize) {
        let Some(mut session) = self.remove_session(index) else {
            return;
        };
        let Some(pending) = session.core.pending_termination.clone() else {
            return;
        };
        if pending.reason != SessionTerminationReason::PongSentClosingConnection {
            let mut reas = if pending.reason == SessionTerminationReason::None {
                String::new()
            } else {
                format!(", Reason: {}", pending.reason.get_description())
            };
            if !pending.extra_reason.trim().is_empty() {
                reas = format!("{reas}, {}", pending.extra_reason);
            }
            log::debug!(
                "Session {}\\{} dropped. Account: {}{reas}",
                session.network.client_id,
                session.core.end_point_c2s,
                session.core.account.as_deref().unwrap_or("")
            );
        }
        session.network.release_resources();
        if let Some(p) = session.core.pending_termination.as_mut() {
            p.termination_status = SessionTerminationPhase::WorldManagerWorkCompleted;
        }
        self.events.push_back(Event::Disconnected {
            session: session.core.id,
            reason: pending.reason,
            extra_reason: pending.extra_reason,
            account: session.core.account.clone(),
        });
    }

    // ACE: NetworkManager.DisconnectAllSessionsForShutdown
    pub fn disconnect_all_sessions_for_shutdown(&mut self, now: ClockSnapshot) {
        for session in self.session_map.iter_mut().flatten() {
            session.terminate(
                SessionTerminationReason::ServerShuttingDown,
                Some((self.messages.character_error)(
                    CharacterError::ServerCrash1,
                )),
                None,
                String::new(),
                now.utc,
            );
        }
    }
}

// ACE: NetworkManager.SendLoginRequestReject
/// The session overload: the `ConnectRequest` first ("First we must send the connect request
/// response"), then the error, then an immediate `Update`. Returns false where ACE would throw
/// (the seeds were already discarded, so `DataWriter.Write(null)` fails), leaving the caller's
/// following `Terminate` unreached.
pub fn send_login_request_reject(
    network: &mut NetworkSession,
    core: &mut SessionCore,
    error: CharacterError,
    io: &mut NetIo<'_>,
) -> bool {
    let cd = &network.connection_data;
    let (Some(server_seed), Some(client_seed)) = (cd.server_seed, cd.client_seed) else {
        log::error!("SendLoginRequestReject on a session whose seeds were already discarded");
        return false;
    };
    let connect_request = packet_outbound_connect_request(
        io.now.portal_year_ticks,
        cd.connection_cookie,
        u32::from(network.client_id),
        server_seed,
        client_seed,
    );
    network.connection_data.discard_seeds();
    network.enqueue_send_packet(connect_request);

    network.enqueue_send((io.messages.character_error)(error));

    network.update(core, io);
    true
}
