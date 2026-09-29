//! MemoryNet session set-up for the net scenarios.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use empyrean_net::driver::memory::{LinkModel, MemoryNet};
use empyrean_net::testing::{ClientStatus, Harness};
use empyrean_net::{
    CharacterError, GameMessageGroup, NetConfig, OutboundMessage, ServerNet, SessionId,
    TransportMessages,
};

/// The virtual step between harness frames.
pub const TICK: Duration = Duration::from_millis(10);

#[must_use]
pub fn server_ip() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
}

/// A client address: host `10.0.1.<host>`, the given port.
#[must_use]
pub fn client_addr(host: u8, port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 1, host)), port)
}

/// ACE `GameMessageOpcode.CharacterError`.
pub const OPCODE_CHARACTER_ERROR: u32 = 0xF659;
/// ACE `GameMessageOpcode.AccountBoot`.
pub const OPCODE_ACCOUNT_BOOT: u32 = 0xF7DC;

/// The test stand-in for empyrean-world's `GameMessageCharacterError`: `UIQueue`, the opcode, then the
/// `uint` error (empyrean-world's messages tests check the real builder's bytes).
fn character_error(error: CharacterError) -> OutboundMessage {
    let mut data = OPCODE_CHARACTER_ERROR.to_le_bytes().to_vec();
    data.extend_from_slice(&(error as u32).to_le_bytes());
    OutboundMessage {
        group: GameMessageGroup::UIQueue,
        data,
    }
}

/// The test stand-in for empyrean-world's `GameMessageBootAccount`: the opcode (the reason is not
/// checked here).
fn boot_account(_reason: Option<&str>) -> OutboundMessage {
    OutboundMessage {
        group: GameMessageGroup::UIQueue,
        data: OPCODE_ACCOUNT_BOOT.to_le_bytes().to_vec(),
    }
}

/// The transport's message builders for these tests.
#[must_use]
pub fn messages() -> TransportMessages {
    TransportMessages {
        character_error,
        boot_account,
    }
}

#[must_use]
pub fn server(config: NetConfig) -> ServerNet {
    ServerNet::new(config, messages())
}

/// A harness over a perfect network with ACE's default configuration.
#[must_use]
pub fn harness() -> Harness {
    Harness::new(MemoryNet::new(server(NetConfig::default()), server_ip()))
}

/// A harness whose two directions follow the given models.
#[must_use]
pub fn lossy_harness(to_server: LinkModel, to_clients: LinkModel) -> Harness {
    Harness::new(MemoryNet::with_links(
        server(NetConfig::default()),
        server_ip(),
        to_server,
        to_clients,
    ))
}

/// Runs until client `i` is connected, at most `max`.
pub fn connect(h: &mut Harness, i: usize, max: Duration) -> bool {
    h.run_until(max, TICK, |h| {
        h.clients[i].status() == ClientStatus::Connected
    })
}

/// The server session whose client-to-server endpoint is client `i`'s address.
#[must_use]
pub fn session_of(h: &Harness, i: usize) -> Option<SessionId> {
    let addr = h.clients[i].addr;
    h.net
        .server
        .sessions()
        .find(|s| s.core.end_point_c2s == addr)
        .map(|s| s.core.id)
}

/// Advances the harness clock by `d` in `TICK` steps.
pub fn run_for(h: &mut Harness, d: Duration) {
    h.run_until(d, TICK, |_| false);
}

use dereth_transport::conn::{build_login_request, ConnectRequest, ConnectionAuthenticator};
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

/// A retail-shaped `LoginRequest` datagram (password as an archive string).
#[must_use]
pub fn login_request_datagram(account: &str, password: &str, version: &str) -> Vec<u8> {
    let auth = ConnectionAuthenticator::account_password(account, password);
    let mut body = build_login_request(&auth);
    if version != "1802" {
        // Replace the leading PString "1802" (2 + 4 bytes, padded to 8) with `version`.
        let mut v = dereth_transport::wire::optional::pstring_pack(version.as_bytes());
        v.extend_from_slice(&body[8..]);
        body = v;
    }
    let mut p = OutPacket::new(ProtoHeader::default());
    p.add_optional_header(PacketFlags::LOGIN_REQUEST, body)
        .expect("section");
    p.serialize(None).expect("plaintext")
}

/// A `ConnectResponse` datagram carrying `cookie`.
#[must_use]
pub fn connect_response_datagram(cookie: u64, net_id: u16) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        rec_id: net_id,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec())
        .expect("section");
    p.serialize(None).expect("plaintext")
}

/// The `ConnectRequest` among `datagrams`, with its header.
#[must_use]
pub fn find_connect_request(
    datagrams: &[empyrean_net::Outgoing],
) -> Option<(ProtoHeader, ConnectRequest, bool)> {
    datagrams.iter().find_map(|d| {
        let p = ParsedPacket::parse(&d.bytes).ok()?;
        let body = p.optional.get(&PacketFlags::CONNECT_REQUEST)?;
        Some((
            p.header,
            ConnectRequest::from_bytes(body).ok()?,
            p.checksum_ok(None),
        ))
    })
}

/// A server with one session that has completed the handshake, driven by hand (no client
/// transport). Returns the server, the session, the client's address and the time.
#[must_use]
pub fn raw_connected() -> (
    ServerNet,
    SessionId,
    SocketAddr,
    empyrean_net::ClockSnapshot,
) {
    use empyrean_net::{AccountSelect, ClockSnapshot, ClockSnapshotExt, Event, PortKind};
    let mut net = server(NetConfig::default());
    let from = client_addr(200, 41_000);
    let mut now = ClockSnapshot::at_seconds(100.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("raw", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!("login request")
    };
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "raw".into(), 1);
    let _ = net.poll(now).count();
    let cookie = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    now = now.advanced(Duration::from_millis(50));
    net.on_datagram(
        PortKind::S2C,
        from,
        &connect_response_datagram(cookie, session.client_id),
        now,
    );
    assert!(matches!(
        net.events().next(),
        Some(Event::ConnectResponse { .. })
    ));
    // The TimeSync.
    let _ = net.poll(now).count();
    now = now.advanced(Duration::from_millis(10));
    (net, session, from, now)
}

/// Every packet in `out`, parsed. An encrypted packet with sequence 0 (which ACE sends when a
/// session is terminated before its ConnectRequest, and which the shared parser rightly refuses)
/// is parsed as if its sequence were 1, so its contents can be inspected.
#[must_use]
pub fn parse_all(out: &[empyrean_net::Outgoing]) -> Vec<ParsedPacket> {
    out.iter()
        .map(|o| {
            let mut bytes = o.bytes.clone();
            let h = ProtoHeader::from_bytes(&bytes).expect("header");
            if h.seq_id == 0 && h.header.is_encrypted() {
                bytes[0] = 1;
            }
            ParsedPacket::parse(&bytes).expect("server packets parse")
        })
        .collect()
}
