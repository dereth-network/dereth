//! The goodbye is the retail disconnect bytes; nothing goes out before it is asked for; one packet
//! per connection; the client sends nothing after it; the comparison notices every field.
//! Fixture: recorded messages and synthetic state or packets.

use std::net::SocketAddr;
use std::time::Duration;

use dereth_client_net::net::{Net, NetConfig};
use dereth_primitives::LocalTime;
use dereth_transport::wire::{PacketFlags, ParsedPacket};

/// The 20 bytes the retail client sends, taken verbatim from recorded session 06's last
/// client datagram (t = 5.726 s) — and byte-identical to `short-second-connection`'s and
/// `ddd-interrogation-only`'s.
///
/// Pinned as a literal on purpose (the test-design constraints: *a test that reads a constant through the same
/// symbol it writes it through cannot detect a wrong constant*). Decoded:
///
/// | field | value |
/// |---|---|
/// | `seq_id` | 0 — unsequenced, as the optional-header send writes it |
/// | `header` | `0x00008000` `Disconnect` |
/// | `checksum` | `0xBAF2F0DD`, **not** ISAAC-XORed: the section is disposable |
/// | `rec_id` | 0 — the receiver's network id |
/// | `interval` | 0 — the optional-header send writes a literal 0 |
/// | `datalen` | 0 — the header carries no payload |
/// | `iteration` | 1 — the receiver iteration echoed from the `ConnectRequest` |
const RETAIL_DISCONNECT: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, // seq_id
    0x00, 0x80, 0x00, 0x00, // header = 0x00008000
    0xDD, 0xF0, 0xF2, 0xBA, // checksum
    0x00, 0x00, // rec_id
    0x00, 0x00, // interval
    0x00, 0x00, // datalen
    0x01, 0x00, // iteration
];

fn server() -> SocketAddr {
    "127.0.0.1:19000".parse().expect("addr")
}

/// A `Net` with one connection registered exactly as the connection-request handler registers the
/// login server: recipient/network id 0 and iteration 1, which is what
/// every recorded session carries.
fn connected_net() -> Net {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(0, 0, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(server()));
    net
}

fn disconnect_datagrams(net: &mut Net) -> Vec<Vec<u8>> {
    net.take_outgoing()
        .into_iter()
        .map(|(b, _)| b)
        .filter(|b| {
            ParsedPacket::parse(b)
                .map(|p| p.header.header.contains(PacketFlags::DISCONNECT))
                .unwrap_or(false)
        })
        .collect()
}

/// The claim: `Net::log_off_server` emits the retail bytes, exactly.
///
/// Oracle: the optional-header send path and the recorded datagram above.
#[test]
fn the_goodbye_is_the_bytes_retail_sends() {
    let mut net = connected_net();
    let sent = net.log_off_server();
    assert_eq!(
        sent, 1,
        "one standalone packet per connection in the connection list"
    );

    let out = net.take_outgoing();
    assert_eq!(out.len(), 1, "the goodbye is one datagram and nothing else");
    let (bytes, dest) = &out[0];
    assert_eq!(
        dest,
        &Some(server()),
        "to the connection's own address -- the recorded Disconnect goes to the port the server \
         answered from (pair 0), not to the handshake port + 1 the CICMD keep-alive uses"
    );
    assert_eq!(
        bytes.as_slice(),
        RETAIL_DISCONNECT.as_slice(),
        "the emitted goodbye differs from the one in recorded session 06"
    );

    // The fields, named, so a failure says which one moved rather than dumping 20 bytes.
    let p = ParsedPacket::parse(bytes).expect("our own packet parses");
    assert_eq!(
        p.header.seq_id, 0,
        "the optional-header send writes seq_id = 0"
    );
    assert_eq!(p.header.header.0, PacketFlags::DISCONNECT);
    assert_eq!(p.header.rec_id, 0);
    assert_eq!(
        p.header.interval, 0,
        "the optional-header send writes interval = 0"
    );
    assert_eq!(p.header.datalen, 0, "a zero-length optional header");
    assert_eq!(p.header.iteration, 1, "receiver-header iteration");
    assert_eq!(p.optional.len(), 1);
    assert!(
        p.optional[&PacketFlags::DISCONNECT].is_empty(),
        "0x8000 carries no data (01-packet-format.md §3.8)"
    );
    assert!(
        !p.header.header.is_encrypted(),
        "flags = 3 is disposable, so no ISAAC key is drawn and the checksum is a constant"
    );
}

/// Nothing is sent before the goodbye is asked for.
#[test]
fn nothing_is_sent_before_the_goodbye_is_asked_for() {
    let mut net = connected_net();
    for i in 0..20 {
        net.tick(LocalTime(f64::from(i) * 0.5), Duration::from_millis(1));
    }
    assert!(
        disconnect_datagrams(&mut net).is_empty(),
        "a Disconnect appeared without anyone asking for one"
    );
    assert!(!net.log_off_sent());
}

/// The log-off-sent flag is set, and the send returns 0 while it is.
///
/// This is the half a "did the packet go out" assertion cannot see: after the goodbye the client
/// is **silent**, which is why the recorded Disconnect is the last datagram in all seven sessions
/// even though the 2-second ACK cadence was still running when it went.
#[test]
fn the_client_goes_silent_after_the_goodbye() {
    let mut net = connected_net();

    // Calibrate: with the flag clear, this same drive *does* produce traffic. The driver crosses
    // `ICMD_INTERVALS` (220 half-second intervals = 110 s), so the keep-alive of
    // `docs/networking/01-packet-format.md` §3.15 goes out — the one periodic send that does not need the
    // connection to have reached `Connected`, and therefore the honest known-positive for a
    // connection registered but not yet acked.
    let mut before = connected_net();
    for i in 0..250 {
        before.tick(LocalTime(f64::from(i) * 0.5), Duration::from_millis(1));
    }
    let noisy = before.take_outgoing().len();
    assert!(
        noisy > 0,
        "the calibration produced no traffic at all, so the silence below would prove nothing"
    );

    net.log_off_server();
    let _ = net.take_outgoing();
    assert!(net.log_off_sent());
    for i in 0..250 {
        net.tick(LocalTime(f64::from(i) * 0.5), Duration::from_millis(1));
    }
    assert_eq!(
        net.take_outgoing().len(),
        0,
        "the client sent {noisy} datagram(s) over the same drive before the goodbye and must send \
         none after it"
    );
}

/// Behaviour: link.goodbye.the-client-sends-one-disconnect-per-connection-and-then-goes-silent
/// One standalone packet **per connection**, which is what the loop over the connection list
/// in the server log-off is for — and the asymmetric case, since a single-connection test cannot
/// tell a loop from a single send.
#[test]
fn every_connection_gets_its_own_packet() {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(0, 0, 1, 1, 2, Some(server()));
    net.add_connection(
        3,
        3,
        7,
        3,
        4,
        Some("127.0.0.1:19100".parse().expect("addr")),
    );
    assert_eq!(net.log_off_server(), 2);

    let out = net.take_outgoing();
    assert_eq!(out.len(), 2);
    let headers: Vec<(u16, u16)> = out
        .iter()
        .map(|(b, _)| {
            let p = ParsedPacket::parse(b).expect("parses");
            (p.header.rec_id, p.header.iteration)
        })
        .collect();
    assert_eq!(
        headers,
        vec![(0, 1), (3, 7)],
        "each packet carries its own receiver's net id and iteration, not the first one's"
    );
    // And the second connection's bytes are *not* the retail constant, which is what proves the
    // fields above are read per connection rather than hard-coded.
    assert_ne!(out[1].0.as_slice(), RETAIL_DISCONNECT.as_slice());
}

/// Calibration for the comparison itself (§7.8, and §7.14's other half): prove the equality can
/// produce a *difference* before believing that it produced a match.
#[test]
fn the_comparison_notices_every_field() {
    let mut net = connected_net();
    net.log_off_server();
    let ours = net.take_outgoing().remove(0).0;
    assert_eq!(ours.as_slice(), RETAIL_DISCONNECT.as_slice());

    for (what, offset) in [
        ("seq_id", 0usize),
        ("header", 4),
        ("checksum", 8),
        ("rec_id", 12),
        ("interval", 14),
        ("datalen", 16),
        ("iteration", 18),
    ] {
        let mut broken = RETAIL_DISCONNECT;
        broken[offset] ^= 0x01;
        assert_ne!(
            ours.as_slice(),
            broken.as_slice(),
            "a one-bit change to {what} was not noticed, so this comparison proves nothing"
        );
    }
}
