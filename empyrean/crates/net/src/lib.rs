//! The port of `Source/ACE.Server/Network`'s transport: sans-IO server sessions over the shared
//! wire layer, with UDP and in-memory drivers.
//!
//! **Depends on** `empyrean-common`, the shared `dereth-primitives`, `dereth-transport` and
//! `dereth-protocol`, and, behind a feature, the client's session (`dereth-client-net`) for the
//! test bot. **Used by** the gameplay crate (`empyrean-world`), the commands (`empyrean-command`),
//! the server (`empyrean-server`) and the test kit (`empyrean-testkit`).
//!
//! **Must never** do I/O or read a clock in its core: [`ServerNet`] is driven by calls that each
//! take a [`ClockSnapshot`], and the sockets live in the drivers ([`driver`]: a UDP driver owning
//! ACE's two sockets, and [`driver::memory::MemoryNet`] for tests). Only the transport half of
//! ACE's `Session` is here; the game half (the player, the character list, data patches, the
//! log-off timer) is the world's, which reacts to [`Event`]s and drives the session state through
//! [`ServerNet::set_state`].
//!
//! [`ServerNet`] is ACE's `NetworkManager` plus every session it owns, driven by exactly these
//! calls:
//!
//! - [`ServerNet::on_datagram`]: a datagram arrived on port `P` or `P + 1` (ACE
//!   `ConnectionListener.OnDataReceive`);
//! - [`ServerNet::poll`]: ACE `NetworkManager.DoSessionWork`, returning the datagrams to send;
//! - [`ServerNet::send`]: ACE `NetworkSession.EnqueueSend(GameMessage)`;
//! - [`ServerNet::events`]: what the world must act on (login requests, connect responses, inbound
//!   messages, disconnects);
//! - [`ServerNet::account_select_callback`], [`ServerNet::accept_login`] and
//!   [`ServerNet::reject_login`]: the transport half of ACE's `AuthenticationHandler`, around the
//!   world's account lookup.
//!
//! The wire layer (the header, the optional sections, fragments, the checksum and ISAAC) is
//! `dereth-transport`'s; ACE's session logic on top of it (sequencing, reordering, NAK handling,
//! bundling, the handshake, timeouts) is ported here. Client to server, every message arrives
//! exactly once in fragment-sequence order; server to client, exactly once but under loss not
//! necessarily in order, because ACE stamps its blobs ephemeral with no ordering stamp. Both are
//! ACE's behaviour and are ported as they are.

pub mod client_message;
pub mod client_packet;
pub mod connection_listener;
pub mod driver;
pub mod enums;
pub mod extensions;
pub mod game_message_group;
pub mod handlers;
pub mod managers;
pub mod message_buffer;
pub mod message_fragment;
pub mod network_bundle;
pub mod network_session;
pub mod network_statistics;
pub mod packet_direction;
pub mod packet_header_flags_util;
pub mod packets;
pub mod server_packet;
pub mod session;
pub mod session_connection_data;
pub mod session_termination_details;
pub mod status_ping;
pub mod transport_messages;

#[cfg(feature = "testing")]
pub mod testing;

use std::net::SocketAddr;
use std::time::Duration;

use empyrean_common::clock::duration_to_ticks;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::time::Time;

pub use client_message::ClientMessage;
pub use enums::{
    CharacterError, NetAuthType, SessionState, SessionTerminationPhase, SessionTerminationReason,
};
pub use game_message_group::GameMessageGroup;
pub use managers::network_manager::{AccountSelect, NetConfig, ServerNet};
pub use network_statistics::NetworkStatistics;
pub use packets::packet_inbound_login_request::PacketInboundLoginRequest;
pub use transport_messages::TransportMessages;

pub use empyrean_common::clock::ClockSnapshot;

/// Builds [`ClockSnapshot`]s without a [`empyrean_common::clock::Clock`], for drivers and tests.
///
/// The transport reads two of the snapshot's clocks: `utc` (ACE `DateTime.UtcNow`) and
/// `portal_year_ticks` (ACE `Timers.PortalYearTicks`, sent in `TimeSync`, `EchoResponse`, the
/// `ConnectRequest` and the header's time field). The in-memory driver schedules its links on
/// `monotonic`.
pub trait ClockSnapshotExt: Sized {
    /// `utc` and `unix_time` at `seconds` after the Unix epoch, `portal_year_ticks` at `seconds`,
    /// and `monotonic` at `seconds` from its origin; the tests' virtual clock.
    #[must_use]
    fn at_seconds(seconds: f64) -> Self;

    /// Every clock advanced by `d` (`utc` to whole 100 ns ticks, truncated).
    #[must_use]
    fn advanced(self, d: Duration) -> Self;
}

impl ClockSnapshotExt for ClockSnapshot {
    fn at_seconds(seconds: f64) -> Self {
        let utc = DotNetDateTime::UNIX_EPOCH.add_seconds(seconds);
        Self {
            portal_year_ticks: seconds,
            unix_time: Time::get_unix_time_at(utc),
            utc,
            monotonic: Duration::from_secs_f64(seconds),
        }
    }

    fn advanced(self, d: Duration) -> Self {
        let utc = self.utc.add_ticks(duration_to_ticks(d));
        Self {
            portal_year_ticks: self.portal_year_ticks + d.as_secs_f64(),
            unix_time: Time::get_unix_time_at(utc),
            utc,
            monotonic: self.monotonic + d,
        }
    }
}

/// Which of ACE's two listeners a datagram arrived on or leaves from.
///
/// ACE binds `Port` (client-to-server, `connectionC2S`) and `Port + 1` (server-to-client,
/// `connectionS2C`). The client sends everything to `P` except its `ConnectResponse` and
/// `CICMDCommand` keep-alive, which go to `P + 1`; once the `ConnectResponse` has arrived the
/// server sends everything from `P + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortKind {
    /// `P`, ACE `connectionC2S`.
    C2S,
    /// `P + 1`, ACE `connectionS2C`.
    S2C,
}

/// One datagram for a driver to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub to: SocketAddr,
    pub via_port_kind: PortKind,
    pub bytes: Vec<u8>,
    /// The session it belongs to, so a driver can report a failed send back through
    /// [`ServerNet::on_send_error`]. `None` for the reject path's throwaway session.
    pub session: Option<SessionId>,
}

/// A session handle: ACE's `ClientId` (the slot in `NetworkManager.sessionMap`, which is also the
/// id the client writes in its packet headers) plus a generation, so a handle to a dropped session
/// never reaches the next session to take the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId {
    pub client_id: u16,
    pub generation: u32,
}

/// One outbound game message: ACE `GameMessage`'s `Group` and `Data` (opcode first).
///
/// The message builders live elsewhere; the transport sees only the bytes and the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundMessage {
    pub group: GameMessageGroup,
    pub data: Vec<u8>,
}

impl OutboundMessage {
    /// ACE `GameMessage.Opcode`, the first dword of `Data` (0 when shorter than four bytes, which a
    /// builder never produces).
    #[must_use]
    pub fn opcode(&self) -> u32 {
        self.data
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u32::from_le_bytes)
    }
}

/// What the world must act on. Drained with [`ServerNet::events`].
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// ACE `AuthenticationHandler.HandleLoginRequest` spawns `DoLogin` (the account lookup and
    /// auto-creation) here. The world does that, then calls
    /// [`ServerNet::account_select_callback`], then [`ServerNet::accept_login`] or
    /// [`ServerNet::reject_login`].
    LoginRequest {
        session: SessionId,
        from: SocketAddr,
        request: PacketInboundLoginRequest,
    },
    /// The three-way handshake completed (ACE `AuthenticationHandler.HandleConnectResponse`): the
    /// world checks `WorldStatus` and sends the character list.
    ConnectResponse { session: SessionId },
    /// ACE `InboundMessageManager.HandleClientMessage(message, session)`, in fragment-sequence
    /// order. `queue` is the fragment header's queue of the fragment that completed the message.
    Message {
        session: SessionId,
        queue: u16,
        message: ClientMessage,
    },
    /// ACE `NetworkSession.VerifyEcho`'s speed-hack verdict. ACE then runs an action chain on the
    /// player (a system chat and `LogOffPlayer`); that is the world's, which afterwards calls
    /// [`ServerNet::reset_echo_verification`].
    SpeedHack { session: SessionId },
    /// ACE `Session.DropSession`: the session is gone. The world logs the player off
    /// (`LogOffPlayer`) and releases the account.
    Disconnected {
        session: SessionId,
        reason: SessionTerminationReason,
        extra_reason: String,
        account: Option<String>,
    },
}
