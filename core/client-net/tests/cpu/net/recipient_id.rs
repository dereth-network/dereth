//! Contracts for recipient id.
//! Fixture: shared recorded messages and synthetic state.

use std::net::SocketAddr;
use std::time::Duration;

use dereth_client_net::net::{Net, NetConfig};
use dereth_primitives::LocalTime;
use dereth_transport::flow::FlowQueue;
use dereth_transport::wire::{PacketFlags, ParsedPacket, ProtoHeader};
use dereth_transport::{CryptoSystem, OutPacket};

const TRAINING_CONNECT_REQUEST: &str = "00000000000004004dd440cf0b008269200001006dd57982690bb241\
b8b3d1040ede5b6c01000000e2fd827cc6f2cbf800000000";

const TRAINING_CONNECT_RESPONSE: &str = "0000000000000800ac02302c0100000008000100b8b3d1040ede5b6c";

const TRAINING_ICMD: &str = "0000000000004000e67039bb00000000080000000100000000000000";

const TRAINING_DISCONNECT: &str = "0000000000800000def0f2ba0100000000000100";

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "hex literal must be even length");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
}

fn server() -> SocketAddr {
    "127.0.0.1:19000".parse().expect("addr")
}

struct Datagram {
    dir: String,
    flags: u32,
    rec_id: u16,
    iteration: u16,
    raw: Vec<u8>,
}

fn capture(name: &str) -> Vec<Datagram> {
    let source = dereth_client_net::client_session::testing::capture::shared_session(name);
    assert!(!source.is_empty(), "{name} is empty");
    source
        .iter()
        .map(|d| {
            let parsed = ParsedPacket::parse(&d.raw).expect("recorded datagram parses");
            Datagram {
                dir: if d.c2s { "c2s" } else { "s2c" }.to_owned(),
                flags: parsed.header.header.0,
                rec_id: parsed.header.rec_id,
                iteration: parsed.header.iteration,
                raw: d.raw.clone(),
            }
        })
        .collect()
}

fn assigned_net_id(dgs: &[Datagram]) -> u16 {
    let cr = dgs
        .iter()
        .find(|d| d.dir == "s2c" && d.flags & PacketFlags::CONNECT_REQUEST != 0)
        .expect("every session opens with a ConnectRequest");
    let p = &cr.raw[20..];
    assert!(p.len() >= 28, "the ConnectRequest payload is 32 bytes");
    u16::try_from(u32::from_le_bytes([p[16], p[17], p[18], p[19]])).expect("NetID fits a u16")
}

#[test]
fn every_recorded_server_datagram_carries_the_server_recipient() {
    for name in dereth_client_net::client_session::testing::session_names() {
        let packets = capture(name);
        let server: Vec<_> = packets.iter().filter(|d| d.dir == "s2c").collect();
        assert!(!server.is_empty());
        assert!(packets.iter().any(|d| d.dir == "c2s"));
        for packet in server {
            assert_eq!(packet.rec_id, 11, "{name}: server recipient");
        }
    }
}

#[test]
fn the_stamped_recipient_is_the_net_id_the_server_assigned() {
    let mut assigned = std::collections::BTreeMap::<&str, u16>::new();
    for name in dereth_client_net::client_session::testing::session_names() {
        let dgs = capture(name);
        let net_id = assigned_net_id(&dgs);
        assigned.insert(name, net_id);

        for d in dgs.iter().filter(|d| d.dir == "c2s") {
            if d.flags & PacketFlags::LOGIN_REQUEST != 0
                || d.flags & PacketFlags::CICMD_COMMAND != 0
            {
                assert_eq!(
                    d.rec_id, 0,
                    "{name}: the login request and the CICMD keep-alive write a literal 0"
                );
                continue;
            }
            assert_eq!(
                d.rec_id, net_id,
                "{name}: every other client datagram carries the assigned NetID"
            );
        }
    }

    assert!(
        assigned.values().any(|id| *id != 0),
        "fixtures distinguish an assigned recipient from the default"
    );
    assert!(assigned.values().any(|id| *id == 0));
}

#[test]
fn the_pinned_short_play_with_training_datagrams_decode_as_the_client_reads_them() {
    let cr = ParsedPacket::parse(&unhex(TRAINING_CONNECT_REQUEST)).expect("ConnectRequest parses");
    assert_eq!(cr.header.rec_id, 11, "the server stamps its own id");
    let net_id = {
        let p = &unhex(TRAINING_CONNECT_REQUEST)[20..];
        u32::from_le_bytes([p[16], p[17], p[18], p[19]])
    };
    assert_eq!(
        net_id, 1,
        "the assignment short-play-with-training is the only capture of"
    );

    let resp =
        ParsedPacket::parse(&unhex(TRAINING_CONNECT_RESPONSE)).expect("ConnectResponse parses");
    assert_eq!(resp.header.rec_id, 1, "already stamped, 22 ms later");
    assert_eq!(resp.header.iteration, 1);

    let icmd = ParsedPacket::parse(&unhex(TRAINING_ICMD)).expect("CICMD parses");
    assert!(icmd.header.header.contains(PacketFlags::CICMD_COMMAND));
    assert_eq!(icmd.header.rec_id, 0, "the null ReceiverData arm");
    assert_eq!(
        icmd.header.iteration, 0,
        "the same arm zeroes iteration_ too"
    );

    let bye = ParsedPacket::parse(&unhex(TRAINING_DISCONNECT)).expect("Disconnect parses");
    assert!(bye.header.header.contains(PacketFlags::DISCONNECT));
    assert_eq!(
        bye.header.rec_id, 1,
        "the server log-off passes a real receiver"
    );
    assert_eq!(bye.header.iteration, 1);
}

#[test]
fn every_recorded_keep_alive_carries_recipient_zero() {
    let mut total = 0usize;
    let mut per = std::collections::BTreeMap::<&str, usize>::new();
    let mut discriminating = 0usize;
    for name in dereth_client_net::client_session::testing::session_names() {
        let dgs = capture(name);
        let net_id = assigned_net_id(&dgs);
        for d in dgs
            .iter()
            .filter(|d| d.dir == "c2s" && d.flags & PacketFlags::CICMD_COMMAND != 0)
        {
            total += 1;
            *per.entry(name).or_default() += 1;
            assert_eq!(d.rec_id, 0, "{name}: keep-alive recipient");
            assert_eq!(d.iteration, 0, "{name}: keep-alive iteration");
            if net_id != 0 {
                discriminating += 1;
            }
        }
    }
    assert!(total > 0 && discriminating > 0);
    assert_eq!(per.values().sum::<usize>(), total);
}

/// Behaviour: link.header.the-recipient-is-the-net-id-the-server-assigned
#[test]
fn the_flow_queue_stamps_the_assigned_net_id_on_every_packet() {
    let emit = |net_id: u16| -> Vec<u8> {
        let mut q = FlowQueue::new(net_id, 1);
        q.enqueue_optional_header(PacketFlags::ACK_SEQUENCE, 7u32.to_le_bytes().to_vec())
            .expect("one disposable section");
        let mut crypto = CryptoSystem::new(0xDEAD_BEEF);
        let out = q.transmit_new_packets(&mut crypto, LocalTime(1.0));
        assert_eq!(out.len(), 1, "one packet for one section");
        out.into_iter().next().expect("packet")
    };

    let zero = emit(0);
    let one = emit(1);
    assert_eq!(zero.len(), one.len(), "same stimulus, same size");

    let pz = ParsedPacket::parse(&zero).expect("parses");
    let po = ParsedPacket::parse(&one).expect("parses");
    assert_eq!(pz.header.rec_id, 0);
    assert_eq!(
        po.header.rec_id, 1,
        "the server's assignment reaches the wire"
    );
    assert_eq!(pz.header.seq_id, po.header.seq_id);
    assert_eq!(pz.header.header.0, po.header.header.0);
    assert_eq!(pz.header.iteration, po.header.iteration);
    assert_ne!(
        pz.header.checksum, po.header.checksum,
        "rec_id is inside the header hash, so the two arms could not agree by accident"
    );

    let differing: Vec<usize> = (0..zero.len()).filter(|&i| zero[i] != one[i]).collect();
    assert!(
        differing.iter().all(|&i| (8..14).contains(&i)),
        "only the checksum and rec_id may move: {differing:?}"
    );
}

#[test]
fn the_goodbye_carries_the_assigned_net_id_and_matches_short_play_with_training_byte_for_byte() {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(0, 1, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(server()));
    assert_eq!(net.log_off_server(), 1, "one packet per connection");

    let out = net.take_outgoing();
    assert_eq!(out.len(), 1, "the goodbye is one datagram and nothing else");
    let (bytes, _dest) = &out[0];
    assert_eq!(
        bytes,
        &unhex(TRAINING_DISCONNECT),
        "short-play-with-training's recorded Disconnect, whose rec_id is 1"
    );

    let mut net0 = Net::new(NetConfig::default());
    net0.add_connection(0, 0, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(server()));
    net0.log_off_server();
    let zero = net0.take_outgoing().remove(0).0;
    let differing: Vec<usize> = (0..20).filter(|&i| zero[i] != bytes[i]).collect();
    assert_eq!(
        differing,
        vec![8, 12],
        "one checksum byte and the low byte of rec_id, and nothing else"
    );
}

/// Behaviour: link.keep-alive.the-recipient-is-always-zero
#[test]
fn the_keep_alive_recipient_is_a_hard_zero_in_retail() {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(0, 1, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(server()));

    net.tick(LocalTime(0.0), Duration::from_millis(50));
    assert!(
        net.take_outgoing().is_empty(),
        "the clock-starting tick emits nothing"
    );
    net.tick(LocalTime(120.0), Duration::from_millis(50));

    let icmds: Vec<(Vec<u8>, Option<SocketAddr>)> = net
        .take_outgoing()
        .into_iter()
        .filter(|(b, _)| {
            ParsedPacket::parse(b)
                .map(|p| p.header.header.contains(PacketFlags::CICMD_COMMAND))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(icmds.len(), 1, "one keep-alive per 220 intervals");

    let (bytes, dest) = &icmds[0];
    let p = ParsedPacket::parse(bytes).expect("parses");
    assert_eq!(
        dest.map(|a| a.port()),
        Some(19001),
        "to the server's port + 1, as the local-interval bump builds the address"
    );
    assert_eq!(p.header.seq_id, 0, "unsequenced");

    assert_eq!(
        p.header.iteration, 0,
        "the null arm zeroes iteration as well"
    );
    assert_eq!(
        p.header.rec_id, 0,
        "the optional-header send's no-receiver arm writes a hard 0 whatever \
         the session's NetID, and all 13 recorded keep-alives carry 0 -- short-play-with-training's included, \
         which is the only one of the thirteen recorded on a NetID that is not 0. This connection \
         is NetID 1, so an assigned-id producer would read 1 here."
    );

    let sec = p
        .optional
        .get(&PacketFlags::CICMD_COMMAND)
        .expect("the CICMD section");
    assert_eq!(sec.len(), 8, "Cmd and Param");
    assert_eq!(&sec[0..4], &1u32.to_le_bytes(), "the no-op command = 1");
    assert_eq!(&sec[4..8], &0u32.to_le_bytes(), "parameter = 0");
}

#[test]
fn the_replay_anchor_takes_the_recipient_out_of_the_packet_it_is_checking() {
    let captured = unhex(TRAINING_ICMD);
    let parsed = ParsedPacket::parse(&captured).expect("parses");

    let mut ours = OutPacket::new(ProtoHeader {
        seq_id: parsed.header.seq_id,
        rec_id: parsed.header.rec_id, // <- the blindness, quoted from tests/replay.rs:190
        interval: parsed.header.interval,
        iteration: parsed.header.iteration,
        ..Default::default()
    });
    ours.add_optional_header(
        PacketFlags::CICMD_COMMAND,
        parsed.optional[&PacketFlags::CICMD_COMMAND].clone(),
    )
    .expect("section");
    assert_eq!(
        ours.serialize(None).expect("serialize"),
        captured,
        "the anchor reproduces it exactly -- and would do so for any value of net_id, because it \
         never consults one"
    );

    let mut wrong = OutPacket::new(ProtoHeader {
        rec_id: 1,
        ..Default::default()
    });
    wrong
        .add_optional_header(
            PacketFlags::CICMD_COMMAND,
            parsed.optional[&PacketFlags::CICMD_COMMAND].clone(),
        )
        .expect("section");
    assert_ne!(
        wrong.serialize(None).expect("serialize"),
        captured,
        "a recipient of 1 does not reproduce the recorded keep-alive"
    );
}

/// Behaviour: link.keep-alive.the-recipient-is-always-zero
#[test]
fn this_build_takes_the_null_arm_at_the_keep_alive_and_the_receiver_arm_at_the_goodbye() {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(0, 1, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(server()));

    net.tick(LocalTime(0.0), std::time::Duration::from_millis(50));
    assert!(
        net.take_outgoing().is_empty(),
        "the clock-starting tick emits nothing"
    );
    net.tick(LocalTime(120.0), std::time::Duration::from_millis(50));

    let keep_alives: Vec<Vec<u8>> = net
        .take_outgoing()
        .into_iter()
        .map(|(b, _)| b)
        .filter(|b| {
            ParsedPacket::parse(b)
                .map(|p| p.header.header.contains(PacketFlags::CICMD_COMMAND))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(keep_alives.len(), 1, "one keep-alive per 220 intervals");
    let ka = ParsedPacket::parse(&keep_alives[0]).expect("parses");
    assert_eq!(
        ka.header.rec_id, 0,
        "the null arm: rec_id = 0 on a NetID-1 connection"
    );
    assert_eq!(
        ka.header.iteration, 0,
        "the null arm: iteration_ = 0 as well"
    );
    assert_eq!(ka.header.interval, 0, "written by neither arm");

    assert_eq!(net.log_off_server(), 1, "one goodbye per connection");
    let out = net.take_outgoing();
    assert_eq!(out.len(), 1);
    let bye = ParsedPacket::parse(&out[0].0).expect("parses");
    assert!(bye.header.header.contains(PacketFlags::DISCONNECT));
    assert_eq!(
        bye.header.rec_id, 1,
        "the receiver arm stamps the assigned network id"
    );
    assert_eq!(
        bye.header.iteration, 1,
        "the receiver arm stamps the connection iteration"
    );

    assert_ne!(
        (ka.header.rec_id, ka.header.iteration),
        (bye.header.rec_id, bye.header.iteration),
        "if these agreed, one of the two arms would not be implemented"
    );
}
