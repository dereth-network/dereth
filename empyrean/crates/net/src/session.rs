// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Session.cs
//
// The transport half only. The game half of ACE's `Session` (`Player`, `Characters`,
// `GameEventSequence`, `LogOffPlayer`, `SendFinalLogOffMessages`, the character-select ping reply,
// the DDD queue, `CheckCharactersForDeletion`, `WorldBroadcast`) is world-level: the world owns
// it and reaches the transport through `ServerNet`'s API and `Event`s.

use std::collections::VecDeque;
use std::net::SocketAddr;

use dereth_transport::wire::PacketFlags;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::DotNetDateTime;

use crate::client_packet::ClientPacket;
use crate::enums::{
    CharacterError, SessionState, SessionTerminationPhase, SessionTerminationReason,
};
use crate::network_session::NetworkSession;
use crate::network_statistics::NetworkStatistics;
use crate::packets::packet_inbound_login_request::PacketInboundLoginRequest;
use crate::server_packet::ServerPacket;
use crate::session_connection_data::SessionRandom;
use crate::session_termination_details::SessionTerminationDetails;
use crate::{Event, OutboundMessage, Outgoing, SessionId};

/// What a session method may touch outside itself: the clock sample, the outgoing datagrams,
/// the world's event queue, the counters, and the one configuration value the session reads.
#[derive(Debug)]
pub struct NetIo<'a> {
    pub now: ClockSnapshot,
    pub outgoing: &'a mut Vec<Outgoing>,
    pub events: &'a mut VecDeque<Event>,
    pub stats: &'a mut NetworkStatistics,
    /// ACE `NetworkManager.DefaultSessionTimeout`: seconds (config, 60 by default).
    pub default_session_timeout: u32,
    /// The world's builders of the messages the transport sends.
    pub messages: crate::transport_messages::TransportMessages,
}

/// The transport half of ACE `Session`, without its `Network`.
#[derive(Debug, Clone)]
pub struct SessionCore {
    pub id: SessionId,
    /// False for the throwaway session `NetworkManager.SendLoginRequestReject` builds, which is
    /// never in the session map.
    pub registered: bool,
    pub end_point_c2s: SocketAddr,
    pub end_point_s2c: Option<SocketAddr>,
    pub state: SessionState,
    pub account_id: u32,
    pub account: Option<String>,
    pub access_level: u32,
    pub logging_identifier: String,
    pub pending_termination: Option<SessionTerminationDetails>,
    /// Stands for ACE's `Player != null && logOffRequestTime == DateTime.MinValue`, the guard in
    /// `NetworkSession.VerifyEcho`. The world sets it.
    pub player_active: bool,
    /// The login request, kept for the world's `AccountSelectCallback` (ACE passes it along the
    /// `DoLogin` task).
    pub login_request: Option<PacketInboundLoginRequest>,
}

impl SessionCore {
    // ACE: Session.Terminate
    /// The half of `Terminate` that records the termination (the enqueues are [`Session::terminate`]).
    /// A second call replaces the first, restarting the 2-second flush window, as in ACE.
    pub fn terminate(
        &mut self,
        reason: SessionTerminationReason,
        extra_reason: String,
        utc_now: DotNetDateTime,
    ) {
        self.pending_termination = Some(SessionTerminationDetails::new(
            reason,
            extra_reason,
            utc_now,
        ));
    }

    // ACE: Session.SetAccount
    pub fn set_account(&mut self, account_id: u32, account: String, access_level: u32) {
        self.account_id = account_id;
        self.account = Some(account);
        self.access_level = access_level;
    }

    // ACE: Session.SetAccessLevel
    pub fn set_access_level(&mut self, access_level: u32) {
        self.access_level = access_level;
    }

    // ACE: Session.SetS2CEndpoint
    pub fn set_s2c_endpoint(&mut self, end_point: SocketAddr) {
        self.end_point_s2c = Some(end_point);
    }
}

/// ACE `Session`: [`SessionCore`] plus its [`NetworkSession`].
#[derive(Debug)]
pub struct Session {
    pub core: SessionCore,
    pub network: NetworkSession,
}

impl Session {
    // ACE: Session.Session
    #[must_use]
    pub fn new(
        id: SessionId,
        registered: bool,
        end_point: SocketAddr,
        server_id: u16,
        utc_now: DotNetDateTime,
        rand: &mut SessionRandom,
    ) -> Self {
        Self {
            core: SessionCore {
                id,
                registered,
                end_point_c2s: end_point,
                end_point_s2c: None,
                state: SessionState::AuthLoginRequest,
                account_id: 0,
                account: None,
                access_level: 0,
                logging_identifier: "Unverified".to_string(),
                pending_termination: None,
                player_active: false,
                login_request: None,
            },
            network: NetworkSession::new(id.client_id, server_id, utc_now, rand),
        }
    }

    // ACE: Session.CheckState
    /// Drops handshake packets that arrive in the wrong state. `HasFlag` is "any of these bits".
    #[must_use]
    pub fn check_state(&self, packet: &ClientPacket) -> bool {
        if packet.has_flag(PacketFlags::LOGIN_REQUEST)
            && self.core.state != SessionState::AuthLoginRequest
        {
            return false;
        }
        if packet.has_flag(PacketFlags::CONNECT_RESPONSE)
            && self.core.state != SessionState::AuthConnectResponse
        {
            return false;
        }
        if packet.has_flag(
            PacketFlags::ACK_SEQUENCE
                | PacketFlags::TIME_SYNC
                | PacketFlags::ECHO_REQUEST
                | PacketFlags::FLOW,
        ) && self.core.state == SessionState::AuthLoginRequest
        {
            return false;
        }
        true
    }

    // ACE: Session.ProcessPacket
    pub fn process_packet(&mut self, packet: ClientPacket, io: &mut NetIo<'_>) {
        if !self.check_state(&packet) {
            return;
        }
        self.network.process_packet(packet, &mut self.core, io);
    }

    // ACE: Session.TickOutbound
    /// Sends what is due and runs the timeout. The game half of ACE's `TickOutbound` (the 6-second
    /// log-off, the 100-second character-select ping reply, the DDD queue) is the world's.
    pub fn tick_outbound(&mut self, io: &mut NetIo<'_>) {
        // "Check if the player has been booted"
        if let Some(status) = self
            .core
            .pending_termination
            .as_ref()
            .map(|p| p.termination_status)
        {
            if status == SessionTerminationPhase::Initialized {
                self.core.state = SessionState::TerminationStarted;
                // "boot messages may need sending"
                self.network.update(&self.core, io);
                if let Some(pending) = self.core.pending_termination.as_mut() {
                    if io.now.utc > pending.termination_end_ticks {
                        pending.termination_status = SessionTerminationPhase::SessionWorkCompleted;
                    }
                }
            }
            return;
        }

        if self.core.state == SessionState::TerminationStarted {
            return;
        }

        // "Checks if the session has stopped responding."
        if io.now.utc >= self.network.timeout_tick {
            self.core.terminate(
                SessionTerminationReason::NetworkTimeout,
                String::new(),
                io.now.utc,
            );
            return;
        }

        self.network.update(&self.core, io);
    }

    // ACE: Session.Terminate
    /// Queues the optional final packet and message, then records the termination.
    pub fn terminate(
        &mut self,
        reason: SessionTerminationReason,
        message: Option<OutboundMessage>,
        packet: Option<ServerPacket>,
        extra_reason: String,
        utc_now: DotNetDateTime,
    ) {
        if let Some(packet) = packet {
            self.network.enqueue_send_packet(packet);
        }
        if let Some(message) = message {
            self.network.enqueue_send(message);
        }
        self.core.terminate(reason, extra_reason, utc_now);
    }

    // ACE: Session.SendCharacterError
    pub fn send_character_error(&mut self, error: CharacterError, io: &NetIo<'_>) {
        self.network
            .enqueue_send((io.messages.character_error)(error));
    }
}
