use super::*;

#[test]
fn header_keeps_full_recipient_and_separate_clock_bytes() {
    let bytes = [
        1, 0, 0, 0, 2, 8, 0, 0, 0x78, 0x56, 0x34, 0x12, 0xef, 0xcd, 0xab, 0x90, 3, 0, 0x42, 0x17,
    ];
    let h = Header::decode(&bytes).unwrap();
    assert_eq!((h.recipient, h.time, h.table), (0x1234_5678, 0x42, 0x17));
    assert_eq!(h.encode(), bytes);
    for end in 0..20 {
        assert_eq!(Header::decode(&bytes[..end]), Err(Error::Truncated));
    }
}

#[test]
fn generic_checksum_uses_ac1999_seed_and_odd_tail_without_body_length() {
    // Header sum: 0x03291975 + 0x00140000 + flags 2 + length 3.
    // Three-byte tail contributes 0x11223300, without a length term.
    let bytes = [
        0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0x7a, 0x4c, 0x5f, 0x14, 3, 0, 0, 0, 0x11, 0x22, 0x33,
    ];
    assert!(Packet::decode(&bytes).unwrap().generic_checksum_ok());
    assert_eq!(
        encode_generic(
            Header {
                flags: flags::CHECKSUM,
                ..Header::default()
            },
            &[0x11, 0x22, 0x33]
        )
        .unwrap(),
        bytes
    );
    let mut bad = bytes;
    bad[12] ^= 1;
    assert!(!Packet::decode(&bad).unwrap().generic_checksum_ok());
    assert_eq!(Packet::decode(&bytes[..22]).unwrap_err(), Error::Length);
    assert_eq!(Packet::decode(&[0; 485]).unwrap_err(), Error::Oversized);
    assert_eq!(
        encode_generic(Header::default(), &[0; 465]),
        Err(Error::Oversized)
    );
    assert_eq!(
        encode_generic(Header::default(), &[0; 464]).unwrap().len(),
        484
    );
}

/// An ac1999 server logs a login with no iterations whole, password included, so none is made.
#[test]
fn a_login_is_never_made_without_both_iterations() {
    for (portal_iteration, cell_iteration) in [(0, 0), (0, 67), (105, 0)] {
        assert_eq!(
            login_request(
                CLIENT_VERSION,
                "a",
                "b",
                LoginMetadata {
                    portal_iteration,
                    cell_iteration,
                },
            ),
            Err(Error::Iterations),
            "portal {portal_iteration}, cell {cell_iteration}"
        );
    }
}

#[test]
fn login_is_sequence_one_and_opens_with_the_magic_word() {
    let bytes = login_request(
        CLIENT_VERSION,
        "Demo",
        "Secret",
        LoginMetadata {
            portal_iteration: 7,
            cell_iteration: 9,
        },
    )
    .unwrap();
    let p = Packet::decode(&bytes).unwrap();
    assert_eq!((p.header.sequence, p.header.flags), (1, 2));
    assert_eq!((p.header.recipient, p.header.table), (0, 0));
    assert!(p.generic_checksum_ok());
    assert_eq!(p.body.len(), 96);
    assert_eq!(
        &p.body[..16],
        &[0x15, 0xcd, 0x5b, 0x07, 8, 0, b'4', b'.', b'3', b'5', b'7', b'5', b'2', 0, 0, 0]
    );
    // The account field: marker, the lowercased text, a reserved word.
    assert_eq!(&p.body[16..20], &[0xef, 0xbe, 0xef, 0xbe]);
    assert_eq!(&p.body[20..40], b"demo:secret\0\0\0\0\0\0\0\0\0");
    assert_eq!(&p.body[40..44], &[0; 4]);
    // The empty second account field.
    assert_eq!(&p.body[44..48], &[0xef, 0xbe, 0xef, 0xbe]);
    assert_eq!(&p.body[48..72], &[0; 24]);
    let words: Vec<u32> = p.body[72..]
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(words, [1, 0, 7, 9, 0, u32::MAX]);
    assert!(!format!("{p:?}").contains("ecret"));
}

#[test]
fn the_credential_fills_twenty_characters_and_is_lowercased() {
    assert_eq!(account_text("Ab", "CdEf").unwrap(), "ab:cdef");
    let full = login_request(
        CLIENT_VERSION,
        "a",
        "123456789012345678",
        LoginMetadata {
            portal_iteration: 7,
            cell_iteration: 9,
        },
    )
    .unwrap();
    let p = Packet::decode(&full).unwrap();
    assert_eq!(&p.body[20..40], b"a:123456789012345678");
    for (account, password) in [
        ("", "x"),
        ("x", ""),
        ("a:b", "c"),
        ("a", "b\0"),
        ("é", "x"),
        ("a", "1234567890123456789"),
    ] {
        assert_eq!(
            login_request(CLIENT_VERSION, account, password, LoginMetadata::default()),
            Err(Error::Credentials)
        );
    }
    for version in ["", "é", "bad version", &"9".repeat(64)] {
        assert_eq!(
            login_request(version, "a", "b", LoginMetadata::default()),
            Err(Error::Version)
        );
    }
}

fn grant_body() -> Vec<u8> {
    // Synthetic clocks 1.0 and 2.0; deliberately unaligned seeds after seven version bytes.
    let mut body = vec![
        0, 0, 0, 0, 0, 0, 0xf0, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0x40, 0x78, 0x56, 0x34, 0x12, 7, 0, 0, 0,
    ];
    body.extend_from_slice(b"4.35752");
    body.extend_from_slice(&[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    body
}

fn grant_packet(flags: u32, server: u32, table: u8, body: &[u8]) -> Vec<u8> {
    encode_generic(
        Header {
            flags,
            recipient: server,
            table,
            ..Header::default()
        },
        body,
    )
    .unwrap()
}

#[test]
fn a_grant_names_the_server_in_its_header_and_the_adopted_recipient_in_its_body() {
    let bytes = grant_packet(0x202, 11, 3, &grant_body());
    let g = Grant::decode(Packet::decode(&bytes).unwrap()).unwrap();
    assert_eq!((g.server, g.table), (11, 3));
    assert_eq!(
        (g.server_time, g.world_epoch, g.recipient),
        (1.0, 2.0, 0x1234_5678)
    );
    // The first seed keys what the server sends, the second what the client sends.
    assert_eq!(
        (g.server_to_client_seed, g.client_to_server_seed),
        (0x5566_7788, 0x1122_3344)
    );
    assert!(g.accepts_version(CLIENT_VERSION));
    let ack = g.acknowledgment().unwrap();
    let ack = Packet::decode(&ack).unwrap();
    assert_eq!(
        (ack.header.sequence, ack.header.flags, ack.header.recipient),
        (0, 0x202, 0x1234_5678)
    );
    assert_eq!((ack.header.table, ack.header.time), (3, 0));
    assert!(ack.body.is_empty() && ack.generic_checksum_ok());
}

#[test]
fn a_grant_is_checked_the_way_the_client_checks_it() {
    let body = grant_body();
    for end in 0..body.len() {
        let bytes = grant_packet(0x202, 11, 0, &body[..end]);
        assert!(Grant::decode(Packet::decode(&bytes).unwrap()).is_err());
    }
    let mut extra = body.clone();
    extra.extend([1, 2, 3]);
    assert!(Grant::decode(Packet::decode(&grant_packet(0x202, 11, 0, &extra)).unwrap()).is_ok());
    for (flags, server) in [
        (0x200, 11),
        (0x002, 11),
        (0x1_0202, 11),
        (0x202, 0),
        (0x202, 400),
    ] {
        let bytes = grant_packet(flags, server, 0, &body);
        assert!(Grant::decode(Packet::decode(&bytes).unwrap()).is_err());
    }
    let mut bad = grant_packet(0x202, 11, 0, &body);
    bad[12] ^= 1;
    assert_eq!(
        Grant::decode(Packet::decode(&bad).unwrap()),
        Err(Error::Checksum)
    );
    let mut nan = body;
    nan[..8].copy_from_slice(&f64::NAN.to_le_bytes());
    assert_eq!(
        Grant::decode(Packet::decode(&grant_packet(0x202, 11, 0, &nan)).unwrap()),
        Err(Error::Grant)
    );
}

#[test]
fn a_version_matches_by_the_number_it_begins_with() {
    let with = |v: &str| Grant {
        server: 1,
        table: 0,
        server_time: 0.0,
        world_epoch: 0.0,
        recipient: 1,
        version: v.to_owned(),
        server_to_client_seed: 0,
        client_to_server_seed: 0,
    };
    assert!(with("4.35752").accepts_version("4.35752"));
    assert!(with("4.357520").accepts_version("4.35752"));
    assert!(with("4.35752x").accepts_version("4.35752"));
    assert!(!with("4.35753").accepts_version("4.35752"));
    assert!(!with("5.1").accepts_version("4.35752"));
    assert!(!with("x").accepts_version("4.35752"));
}

#[test]
fn data_checksum_hashes_fragment_parts_and_mixes_in_the_key() {
    // Header base 0x0341218c. Fragment header sum 0x00260002;
    // payload sum 0x11253300. Their sum 0x114b3302, XOR first zero-seed key
    // 0x182600f3 => 0x096d33f1; total 0x0cae557d.
    let bytes = [
        2, 0, 0, 0, 2, 8, 0, 0, 0, 0, 4, 0, 0x7d, 0x55, 0xae, 0x0c, 19, 0, 0, 0, 1, 0, 0, 0, 0, 0,
        0, 0, 1, 0, 19, 0, 0, 0, 3, 0, 0x11, 0x22, 0x33,
    ];
    let packet = Packet::decode(&bytes).unwrap();
    let fragments = packet.fragments(0x1826_00f3).unwrap();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].payload, [0x11, 0x22, 0x33]);
    assert_eq!(packet.fragments(0), Err(Error::Checksum));
    for (offset, value) in [(28, 0), (30, 15), (32, 1)] {
        let mut bad = bytes;
        bad[offset] = value;
        assert_eq!(
            Packet::decode(&bad).unwrap().fragments(0x1826_00f3),
            Err(Error::Fragment)
        );
    }
    let mut other = bytes;
    other[5] = 0;
    assert_eq!(
        Packet::decode(&other).unwrap().fragments(0),
        Err(Error::Flags)
    );
}

#[test]
fn a_client_packet_carries_a_flow_word_outside_its_length() {
    let bytes = encode_with_flow(
        Header {
            flags: 0x22,
            recipient: 5,
            ..Header::default()
        },
        0x40,
        &[9, 0, 0, 0],
        body_hash(&[9, 0, 0, 0]),
    )
    .unwrap();
    assert_eq!(bytes.len(), 28);
    let p = Packet::decode(&bytes).unwrap();
    assert_eq!(p.header.payload_len, 4);
    assert_eq!(p.body, [0x40, 0, 0, 0].as_slice());
    assert_eq!(p.extra, [9, 0, 0, 0].as_slice());
    assert_eq!(
        p.header.checksum,
        p.header.checksum_base().wrapping_add(0x40).wrapping_add(9)
    );
}

#[test]
fn incoming_queues_do_not_reuse_later_login_queue_semantics() {
    assert_eq!(incoming_queue(3), Ok(NetQueue::WorldObjects));
    assert_eq!(incoming_queue(4), Ok(NetQueue::UiQueue));
    assert_eq!(incoming_queue(7), Ok(NetQueue::ClientCache));
    assert_eq!(incoming_queue(6), Err(Error::Queue));
    assert!(OUTGOING_QUEUES.contains(&LOGIN_QUEUE) && !OUTGOING_QUEUES.contains(&4));
}

#[test]
fn a_zero_seed_still_draws_keys() {
    let mut keys = KeyStream::new(0);
    assert_eq!(keys.next(), 0x1826_00f3);
    assert_eq!(keys.next(), 0x300b_4a8d);
}
