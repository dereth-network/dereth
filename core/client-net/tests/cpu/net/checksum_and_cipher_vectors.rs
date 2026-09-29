//! Behaviour: none (checks data formats, fixture conformance or host contracts)
//! Hash32 matches pinned edge cases and the published 0..599 sweep; ISAAC matches published streams
//! for five seeds and the client construction state; the worked packet reproduces end to end.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_transport::{hash32, CryptoSystem, OutPacket, ParsedPacket, ProtoHeader};

fn hex_bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).expect("ascii"), 16).expect("hex"))
        .collect()
}

/// FNV-1a 64 over the little-endian bytes of each word, in order: the digest the pinned sweeps use.
fn fnv1a64(words: impl IntoIterator<Item = u32>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// MT19937 seeded the way CPython seeds `random.Random(n)` for a non-negative int below 2^32
/// (`init_by_array` over a one-word key), with `randrange(256)` drawn as CPython draws it:
/// `getrandbits(9)` (the top 9 bits of one output), rejected while `>= 256`.
struct PyRandom {
    mt: [u32; 624],
    index: usize,
}

impl PyRandom {
    fn new(seed: u32) -> Self {
        let mut mt = [0u32; 624];
        mt[0] = 19_650_218;
        for i in 1..624 {
            let prev = mt[i - 1];
            mt[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(u32::try_from(i).expect("i < 624"));
        }
        // init_by_array with key = [seed]; the key index j is always 0.
        let mut i = 1usize;
        for _ in 0..624 {
            let prev = mt[i - 1];
            mt[i] = (mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_664_525)).wrapping_add(seed);
            i += 1;
            if i >= 624 {
                mt[0] = mt[623];
                i = 1;
            }
        }
        for _ in 0..623 {
            let prev = mt[i - 1];
            mt[i] = (mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(u32::try_from(i).expect("i < 624"));
            i += 1;
            if i >= 624 {
                mt[0] = mt[623];
                i = 1;
            }
        }
        mt[0] = 0x8000_0000;
        Self { mt, index: 624 }
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= 624 {
            for k in 0..624 {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % 624] & 0x7fff_ffff);
                let mut v = self.mt[(k + 397) % 624] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= 0x9908_b0df;
                }
                self.mt[k] = v;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    fn randrange_256(&mut self) -> u8 {
        loop {
            let r = self.next_u32() >> 23;
            if let Ok(b) = u8::try_from(r) {
                return b;
            }
        }
    }
}

/// The sweep's buffers: one per length 0..600, drawn in order from one generator.
fn sweep_buffers() -> Vec<Vec<u8>> {
    let mut rnd = PyRandom::new(20_130_905);
    (0..600usize)
        .map(|n| (0..n).map(|_| rnd.randrange_256()).collect())
        .collect()
}

/// Hash32 matches the pinned edge case lengths.
#[test]
fn hash32_matches_the_pinned_edge_case_lengths() {
    let cases: [(&str, u32); 14] = [
        ("", 0x0000_0000),
        ("31", 0x3101_0000),
        ("f61a", 0xF61C_0000),
        ("04aec6", 0x04B1_C600),
        ("254ecf85", 0x85D3_4E25),
        ("1a04624947", 0x9067_041A),
        ("9780857f35a5", 0xB530_8097),
        ("c7153f5d9c72cb", 0xF9B8_E0C7),
        ("027c4a82a583f445", 0xC846_FFA7),
        ("d359ae7fa6562db75b", 0x91E4_B079),
        ("c582d0dce1daece354e8", 0x15AF_5DA6),
        ("cca10d326e9482a7c1aac1", 0x9B45_F73A),
        ("c39e2abfefb7adc6bb8d64d2", 0x5848_E46D),
        ("9d4aa075580f51b56634c1fe2a", 0x53BF_8E5B),
    ];
    for (len, (hex, expected)) in cases.iter().enumerate() {
        let buf = hex_bytes(hex);
        assert_eq!(buf.len(), len);
        assert_eq!(hash32(&buf), *expected, "length {len}");
    }

    // The size and placeholder constants the vectors were published alongside.
    assert_eq!(dereth_transport::crc::CHECKSUM_PLACEHOLDER, 0xBADD_70DD);
    assert_eq!(dereth_transport::wire::HEADER_SIZE, 20);
    assert_eq!(dereth_transport::wire::FRAG_HEADER_SIZE, 16);
    assert_eq!(dereth_transport::wire::MAX_FRAG_DATA, 448);
    assert_eq!(dereth_transport::wire::MAX_FRAG_SIZE, 464);
}

/// Hash32 matches the published sweep for every length 0 to 599.
#[test]
fn hash32_matches_the_published_sweep_for_every_length_0_to_599() {
    let buffers = sweep_buffers();

    // The generator port reproduces the published blob's opening bytes (lengths 0..=7 in a row).
    let head: Vec<u8> = buffers.iter().take(8).flatten().copied().collect();
    assert_eq!(
        head,
        hex_bytes("31f61a04aec6254ecf851a046249479780857f35a5c7153f5d9c72cb"),
        "the CPython Mersenne Twister port drifted; the checksums below are then meaningless"
    );
    assert_eq!(buffers.iter().map(Vec::len).sum::<usize>(), 179_700);

    let checksums: Vec<u32> = buffers.iter().map(|b| hash32(b)).collect();
    assert_eq!(checksums[597], 0xB4B9_161D);
    assert_eq!(checksums[598], 0xAA85_2361);
    assert_eq!(checksums[599], 0x2B76_F0AB);
    assert_eq!(
        fnv1a64(checksums.iter().copied()),
        0x96a9_8576_548d_7928,
        "FNV-1a 64 over the 600 published checksums, lengths 0..600 in order"
    );
}

/// Isaac matches the published streams for 1000 draws across five seeds.
#[test]
fn isaac_matches_the_published_streams_for_1000_draws_across_five_seeds() {
    #[rustfmt::skip]
    let streams: [(u32, [u32; 4], u32, u64); 5] = [
        (0x0000_0000, [0x1826_00F3, 0x300B_4A8D, 0x301B_6622, 0xB08A_CD21], 0xA4C5_E05F, 0x8e6c_53c1_9517_5e2e),
        (0x0000_0001, [0x3816_8FDD, 0x173F_F3D7, 0x832D_DC13, 0xC429_1D70], 0xF6FD_AAF2, 0x9ad0_df97_0e57_9424),
        (0xDEAD_BEEF, [0x5DA2_2D96, 0xDB3B_A3B6, 0x9FD9_67F9, 0x0748_7047], 0x4048_D3DC, 0x709e_e044_07f4_37df),
        (0x1234_5678, [0xFDD4_AFEB, 0x3983_11D2, 0xC718_29A4, 0xB19D_F96A], 0x48A2_EDC5, 0xd379_3eb8_334a_1da0),
        (0xFFFF_FFFF, [0x4317_C641, 0x1D12_4727, 0xE877_449D, 0x5C32_6457], 0x53C3_D78B, 0x6da1_e7b1_5e67_6f18),
    ];
    for (seed, first, thousandth, digest) in streams {
        let mut cs = CryptoSystem::new(seed);
        let draws: Vec<u32> = (0..1000).map(|_| cs.next()).collect();
        assert_eq!(draws[..4], first, "seed {seed:#010X}, first draws");
        assert_eq!(draws[999], thousandth, "seed {seed:#010X}, draw 999");
        assert_eq!(
            fnv1a64(draws.iter().copied()),
            digest,
            "seed {seed:#010X}, digest of 1000 draws"
        );
    }
}

/// Isaac construction state matches the client transcription.
#[test]
fn isaac_construction_state_matches_the_client_transcription() {
    let dump = include_str!("../oracle/isaac_state.txt");
    let mut randa = None;
    let mut randb = None;
    let mut randc = None;
    let mut randcnt = None;
    let mut randmem: Vec<u32> = Vec::new();
    let mut randrsl: Vec<u32> = Vec::new();

    for line in dump
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let (key, rest) = line.split_once(' ').expect("key value");
        // Decimal, not hex: eight hex digits are indistinguishable from an address, and one of
        // the 512 words below falls inside the range the public-tree gate strips.
        let words = || {
            rest.split_whitespace()
                .map(|w| w.parse::<u32>().expect("decimal"))
                .collect::<Vec<u32>>()
        };
        match key {
            "randa" => randa = Some(rest.parse::<u32>().expect("decimal")),
            "randb" => randb = Some(rest.parse::<u32>().expect("decimal")),
            "randc" => randc = Some(rest.parse::<u32>().expect("decimal")),
            "randcnt" => randcnt = Some(rest.parse::<u32>().expect("decimal")),
            "randmem" => randmem = words(),
            "randrsl" => randrsl = words(),
            other => panic!("unexpected key {other}"),
        }
    }

    let cs = CryptoSystem::new(0xDEAD_BEEF);
    let (a, b, c, cnt) = cs.state_words();
    assert_eq!(Some(a), randa);
    assert_eq!(Some(b), randb);
    assert_eq!(Some(c), randc);
    assert_eq!(Some(cnt), randcnt);

    // randc is the seed plus one: `isaac()` increments it exactly once during construction. If the
    // seed had not reached randc before randinit, this would be 1.
    assert_eq!(c, 0xDEAD_BEEFu32.wrapping_add(1));

    let (mem, rsl) = cs.state_arrays();
    assert_eq!(mem.len(), 256);
    assert_eq!(mem.as_slice(), randmem.as_slice(), "randmem after randinit");
    assert_eq!(rsl.as_slice(), randrsl.as_slice(), "randrsl after randinit");

    // And the first draw is randrsl[255], which is also randb.
    let mut cs = CryptoSystem::new(0xDEAD_BEEF);
    assert_eq!(cs.next(), rsl[255]);
    assert_eq!(rsl[255], b);
}

/// The worked packet reproduces end to end.
#[test]
fn the_worked_packet_reproduces_end_to_end() {
    const DATAGRAM: &str = concat!(
        "01000000060000002e4bd6490b0000011c000100",
        "010000000000008001001c0000000900b0f700000100005002000000",
    );
    const HEADER_WITH_ZERO_CHECKSUM: &str = "0100000006000000000000000b0000011c000100";
    const FRAGMENT: &str = "010000000000008001001c0000000900b0f700000100005002000000";
    const ISAAC_SEED: u32 = 0xDEAD_BEEF;

    let datagram = hex_bytes(DATAGRAM);
    assert_eq!(datagram.len(), 48);

    let packet = ParsedPacket::parse(&datagram).expect("the worked packet must parse");

    // The header, with its checksum zeroed, is exactly what was published.
    let mut zeroed = packet.header;
    zeroed.checksum = 0;
    assert_eq!(
        zeroed.to_bytes().to_vec(),
        hex_bytes(HEADER_WITH_ZERO_CHECKSUM)
    );
    assert_eq!(packet.header.header.0, 0x0000_0006);

    assert_eq!(packet.header_hash, 0xBBF2_710B);
    assert_eq!(packet.fragments.len(), 1);
    assert_eq!(packet.fragments[0].to_bytes(), hex_bytes(FRAGMENT));
    assert_eq!(packet.fragments[0].hash(), 0xD041_F7B5);
    assert_eq!(packet.payload_hash, 0xD041_F7B5);

    // The key is the *first* draw of CryptoSystem(seed) -- one value per encrypted packet.
    let mut incoming = CryptoSystem::new(ISAAC_SEED);
    let key = incoming.next();
    assert_eq!(key, 0x5DA2_2D96);

    assert_eq!(packet.header.checksum, 0x49D6_4B2E);
    assert_eq!(
        packet.header.checksum,
        0xBBF2_710Bu32.wrapping_add(0xD041_F7B5 ^ key)
    );
    assert!(packet.checksum_ok(Some(key)));
    assert!(!packet.checksum_ok(Some(key ^ 1)));
    assert_eq!(packet.recovered_key(), key);

    // And the sender's half: rebuilding it produces the identical 48 bytes.
    let mut out = OutPacket::new(ProtoHeader {
        seq_id: packet.header.seq_id,
        rec_id: packet.header.rec_id,
        interval: packet.header.interval,
        iteration: packet.header.iteration,
        ..Default::default()
    });
    out.add_fragment(packet.fragments[0].clone()).expect("frag");
    let mut outgoing = CryptoSystem::new(ISAAC_SEED);
    let bytes = out.serialize(Some(outgoing.next())).expect("serialize");
    assert_eq!(bytes, datagram);
}
