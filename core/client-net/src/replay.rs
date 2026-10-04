//! The capture/replay harness — the transport's acceptance gate (`feature = "replay"`).
//!
//! A capture is self-checking because it contains **both** the stimulus and the client's own
//! response:
//!
//! ```text
//! for each datagram in the capture, in order:
//!     if dir == s2c:  net.feed(raw_bytes, t_rel)
//!     if dir == c2s:  expect the NEXT datagram our stack emits to equal raw_bytes
//! ```
//!
//! The comparison is field-aware rather than a blind `memcmp`, because our stack legitimately emits
//! different bytes in three places:
//!
//! | field | compare? | why |
//! |---|---|---|
//! | `seq_id`, `header`, `rec_id`, `datalen`, `iteration` | exactly | all deterministic |
//! | `checksum` | exactly | deterministic given the seeds from the recording's `ConnectRequest` |
//! | optional-header presence and order | exactly | the ascending-mask invariant |
//! | `TimeSync` payload (f64 game time) | ignore the value, assert presence | rewritten before every send |
//! | `EchoRequest` / `EchoResponse` payloads | ignore the values | same |
//! | `interval` and the `Flow` section | modulo a fixed offset from the first packet | wall-clock driven |
//! | fragment payload bytes | exactly | the whole point |
//!
//! **A correction to that table.** "Compare `checksum` exactly" and "ignore the TimeSync payload"
//! cannot both hold literally: the checksum covers the payload, so a differing TimeSync value
//! necessarily changes the checksum. `normalise_against` resolves it the way the rule is meant —
//! it copies the captured values of the ignored fields into our packet and *recomputes* our
//! checksum, so the checksum comparison stays exact and stays meaningful.

use std::collections::BTreeMap;
use std::path::Path;

use dereth_transport::conn::ConnectRequest;
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, WireError};

/// Which way a captured datagram went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client to server: our stack must emit an equal datagram.
    C2s,
    /// Server to client: feed it straight into `ProcessPacket`.
    S2c,
}

/// One captured datagram.
#[derive(Debug, Clone)]
pub struct CapturedDatagram {
    pub idx: usize,
    pub dir: Direction,
    /// Seconds since the capture started. Time is driven from this, not from the wall clock, so the
    /// timer-driven sends land where they landed live.
    pub t_rel: f64,
    pub raw: Vec<u8>,
}

/// What a replay needs from the recording's `ConnectRequest`. The two seeds are what make a replay
/// deterministic: the server sends them **in clear** in the one `ConnectRequest` every recording
/// carries, so the whole ISAAC stream -- and therefore every checksum in the capture -- is
/// reproducible offline with no live server. [`Capture::load`] reads them out of the recording
/// itself; nothing beside the recording is consulted.
#[derive(Debug, Clone, Default)]
pub struct CaptureMeta {
    /// `ConnectRequest.OutgoingSeed` — server -> client, the client's incoming key stream.
    pub outgoing_seed: u32,
    /// `ConnectRequest.IncomingSeed` — client -> server, the client's outgoing key stream.
    pub incoming_seed: u32,
    /// `ConnectRequest.NetID`, the `rec_id` the client puts on everything it sends afterwards.
    pub net_id: u16,
    /// The `iteration` of the `ConnectRequest` datagram's own header.
    pub iteration: u16,
}

/// A loaded recording: `fixtures/packet-captures/<slug>.jsonl`, the proxy's `{t, dir, pair, len, data}` lines.
#[derive(Debug, Clone)]
pub struct Capture {
    pub scenario: String,
    pub meta: CaptureMeta,
    pub packets: Vec<CapturedDatagram>,
}

/// Why a replay could not run or did not match.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error(
        "no capture at {0}; record a session there, then regenerate the reassembled messages from it"
    )]
    NoCapture(String),
    #[error("malformed capture: {0}")]
    Malformed(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("packet {idx}: {source}")]
    Wire {
        idx: usize,
        #[source]
        source: WireError,
    },
    #[error("packet {idx}: {field} differs -- captured {expected}, emitted {actual}")]
    Mismatch {
        idx: usize,
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error(
        "packet {idx}: our stack emitted nothing where the capture has a client->server datagram"
    )]
    NothingEmitted { idx: usize },
}

impl Capture {
    /// Load `<captures_dir>/<slug>.jsonl`, the recording itself.
    ///
    /// Each line is one datagram as the capture proxy wrote it: `t` (seconds since the recording
    /// started), `dir` (`c2s` or `s2c`), `pair`, `len` and `data` (the datagram in hex). The two
    /// ISAAC seeds, the net id and the iteration are read out of the recording's one server ->
    /// client `ConnectRequest`, which carries them in clear; a recording with none, or with more
    /// than one, is refused, because either way the key stream cannot be placed.
    ///
    /// # Errors
    /// [`ReplayError::NoCapture`] when the file is absent. That is a real state and this function
    /// still reports it as one; what must not treat it as a *passing* state is the **test**, which
    /// names its recordings and `expect`s every one of them. [`ReplayError::Malformed`] for a line
    /// that does not parse, a `len` that disagrees with `data`, or a missing or repeated
    /// `ConnectRequest`.
    pub fn load(captures_dir: &Path, slug: &str) -> Result<Self, ReplayError> {
        let path = captures_dir.join(format!("{slug}.jsonl"));
        if !path.is_file() {
            return Err(ReplayError::NoCapture(path.display().to_string()));
        }
        let lines = std::fs::read_to_string(&path)?;
        let mut packets = Vec::new();
        let mut meta: Option<CaptureMeta> = None;
        let mut t0: Option<f64> = None;
        for (n, line) in lines.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| ReplayError::Malformed(format!("{slug} line {n}: {e}")))?;
            let dir = match v["dir"].as_str() {
                Some("c2s") => Direction::C2s,
                Some("s2c") => Direction::S2c,
                other => {
                    return Err(ReplayError::Malformed(format!(
                        "{slug} line {n}: dir {other:?}"
                    )))
                }
            };
            let data = v["data"]
                .as_str()
                .ok_or_else(|| ReplayError::Malformed(format!("{slug} line {n}: data")))?;
            let raw = decode_hex(data)
                .ok_or_else(|| ReplayError::Malformed(format!("{slug} line {n}: bad hex")))?;
            if let Some(len) = v["len"].as_u64() {
                if usize::try_from(len).ok() != Some(raw.len()) {
                    return Err(ReplayError::Malformed(format!(
                        "{slug} line {n}: len {len} but {} bytes of data",
                        raw.len()
                    )));
                }
            }
            let t = v["t"]
                .as_f64()
                .ok_or_else(|| ReplayError::Malformed(format!("{slug} line {n}: t")))?;
            let t0 = *t0.get_or_insert(t);
            let idx = packets.len();
            if dir == Direction::S2c {
                if let Some(found) = connect_request_of(&raw, idx)? {
                    if meta.is_some() {
                        return Err(ReplayError::Malformed(format!(
                            "{slug}: a second ConnectRequest at datagram {idx}; a replay assumes \
                             one connection per recording"
                        )));
                    }
                    meta = Some(found);
                }
            }
            packets.push(CapturedDatagram {
                idx,
                dir,
                t_rel: t - t0,
                raw,
            });
        }
        let meta = meta.ok_or_else(|| {
            ReplayError::Malformed(format!(
                "{slug}: no ConnectRequest, so the two ISAAC seeds are not in this recording and \
                 a replay of it cannot be deterministic"
            ))
        })?;
        Ok(Self {
            scenario: slug.to_string(),
            meta,
            packets,
        })
    }

    /// Every client -> server datagram, in order.
    #[must_use]
    pub fn client_datagrams(&self) -> Vec<&CapturedDatagram> {
        self.packets
            .iter()
            .filter(|p| p.dir == Direction::C2s)
            .collect()
    }
}

/// The seeds, net id and iteration of a server -> client datagram, if it is a `ConnectRequest`.
fn connect_request_of(raw: &[u8], idx: usize) -> Result<Option<CaptureMeta>, ReplayError> {
    // `header` is the dword at offset 4. Only a datagram that says it carries the section is
    // parsed here; the replay loop parses every one of them anyway.
    let flags = raw
        .get(4..8)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if flags & PacketFlags::CONNECT_REQUEST == 0 {
        return Ok(None);
    }
    let parsed = ParsedPacket::parse(raw).map_err(|source| ReplayError::Wire { idx, source })?;
    let Some(body) = parsed.optional.get(&PacketFlags::CONNECT_REQUEST) else {
        return Ok(None);
    };
    let cr =
        ConnectRequest::from_bytes(body).map_err(|source| ReplayError::Wire { idx, source })?;
    Ok(Some(CaptureMeta {
        outgoing_seed: cr.outgoing_seed,
        incoming_seed: cr.incoming_seed,
        net_id: u16::try_from(cr.net_id).map_err(|_| {
            ReplayError::Malformed(format!(
                "datagram {idx}: ConnectRequest net id {:#x} does not fit rec_id",
                cr.net_id
            ))
        })?,
        iteration: parsed.header.iteration,
    }))
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

/// The optional-header sections whose payloads the replay ignores.
///
/// All three carry flag `0x08` (time-sensitive), so the client rewrites them
/// immediately before every send and no two runs can agree on the bytes.
pub const TIME_SENSITIVE_SECTIONS: [u32; 3] = [
    PacketFlags::TIME_SYNC,
    PacketFlags::ECHO_REQUEST,
    PacketFlags::ECHO_RESPONSE,
];

/// Normalise our emitted packet against a captured one, so the remaining comparison can be exact.
///
/// Substitutes the captured values of the fields the comparison ignores — the time-sensitive
/// section payloads, `interval`, and the `Flow` section — into our packet and **recomputes the
/// checksum**. After this, any surviving difference is a real one.
///
/// `key` is the ISAAC value our stack used for this packet, or `None` for a plaintext one.
///
/// # Errors
/// A [`WireError`] if either side does not parse, or if our packet's encryption disagrees with the
/// key supplied.
pub fn normalise_against(
    ours: &mut OutPacket,
    captured: &ParsedPacket,
    key: Option<u32>,
) -> Result<Vec<u8>, WireError> {
    for mask in TIME_SENSITIVE_SECTIONS {
        if let (Some(theirs), Some(mine)) = (captured.optional.get(&mask), ours.optional.get(&mask))
        {
            if theirs.len() == mine.len() {
                ours.optional.insert(mask, theirs.clone());
            }
        }
    }
    // `Flow` carries a byte count for an interval that our simulated clock numbers differently.
    if let Some(theirs) = captured.optional.get(&PacketFlags::FLOW) {
        if ours.optional.contains_key(&PacketFlags::FLOW) {
            ours.optional.insert(PacketFlags::FLOW, theirs.clone());
        }
    }
    ours.header.interval = captured.header.interval;
    ours.serialize(key)
}

/// Compare an emitted datagram against a captured one, field by field.
///
/// Call it **after** `normalise_against`; it then reduces to a byte comparison with a useful
/// error message. It reports the first field that differs rather than a hex diff, because "the
/// checksum differs" and "fragment 2's payload differs" send you to completely different places.
///
/// # Errors
/// [`ReplayError::Mismatch`] naming the first differing field, or [`ReplayError::Wire`] if either
/// side does not parse.
pub fn compare(idx: usize, captured: &[u8], emitted: &[u8]) -> Result<(), ReplayError> {
    let wire = |source| ReplayError::Wire { idx, source };
    let exp = ParsedPacket::parse(captured).map_err(wire)?;
    let act = ParsedPacket::parse(emitted).map_err(wire)?;

    let fail = |field: &'static str, e: String, a: String| ReplayError::Mismatch {
        idx,
        field,
        expected: e,
        actual: a,
    };

    macro_rules! eq {
        ($field:literal, $e:expr, $a:expr) => {
            if $e != $a {
                return Err(fail($field, format!("{:#X?}", $e), format!("{:#X?}", $a)));
            }
        };
    }

    eq!("seq_id", exp.header.seq_id, act.header.seq_id);
    eq!("header", exp.header.header.0, act.header.header.0);
    eq!("rec_id", exp.header.rec_id, act.header.rec_id);
    eq!("datalen", exp.header.datalen, act.header.datalen);
    eq!("iteration", exp.header.iteration, act.header.iteration);

    // Presence *and* order. A `BTreeMap`'s key order is the wire order, which is the invariant.
    let exp_masks: Vec<u32> = exp.optional.keys().copied().collect();
    let act_masks: Vec<u32> = act.optional.keys().copied().collect();
    eq!("optional-header masks", exp_masks, act_masks);

    for (mask, bytes) in &exp.optional {
        // The mask lists were compared above, so this lookup cannot miss; the `else` keeps the
        // brief's no-`expect`-in-library-code rule rather than relying on that reasoning.
        let Some(ours) = act.optional.get(mask) else {
            return Err(fail(
                "optional-header masks",
                format!("{mask:#010X}"),
                "absent".into(),
            ));
        };
        if bytes != ours {
            return Err(fail(
                "optional-header payload",
                format!("{mask:#010X} {}", hex(bytes)),
                format!("{mask:#010X} {}", hex(ours)),
            ));
        }
    }

    eq!("fragment count", exp.fragments.len(), act.fragments.len());
    for (i, (e, a)) in exp.fragments.iter().zip(&act.fragments).enumerate() {
        if e.header != a.header {
            return Err(fail(
                "fragment header",
                format!("frag {i} {:?}", e.header),
                format!("frag {i} {:?}", a.header),
            ));
        }
        if e.payload != a.payload {
            return Err(fail(
                "fragment payload",
                format!("frag {i} {}", hex(&e.payload)),
                format!("frag {i} {}", hex(&a.payload)),
            ));
        }
    }

    // Last, because it proves the key stream, the header and every section at once and is the
    // least useful to see first: if anything above differs, the checksum differing tells you
    // nothing new.
    eq!("checksum", exp.header.checksum, act.header.checksum);

    // And finally the raw bytes, which cannot now differ but would catch a field this function has
    // forgotten to compare.
    if captured != emitted {
        return Err(fail("raw bytes", hex(captured), hex(emitted)));
    }
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A capture's interval offset, established from its first client -> server datagram.
///
/// `interval` is a free-running 0.5-second counter that starts whenever the client did, so a
/// replay's absolute value is meaningless; only the differences are. Returns
/// `captured - ours` for the first packet.
#[must_use]
pub fn interval_offset(first_captured: u16, first_ours: u16) -> u16 {
    first_captured.wrapping_sub(first_ours)
}

/// The masks a captured packet carries, in wire order. Diagnostics.
#[must_use]
pub fn masks_of(packet: &ParsedPacket) -> Vec<u32> {
    packet.optional.keys().copied().collect()
}

/// Group a capture's client datagrams by their `header` masks, for a quick "what does this capture
/// exercise" summary when a replay fails.
#[must_use]
pub fn coverage(capture: &Capture) -> BTreeMap<u32, usize> {
    let mut out = BTreeMap::new();
    for dg in capture.client_datagrams() {
        if let Ok(p) = ParsedPacket::parse(&dg.raw) {
            *out.entry(p.header.header.0).or_insert(0) += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_transport::wire::{Fragment, FragmentHeader, ProtoHeader};

    /// The comparison is exact once the ignored fields are normalised away.
    ///
    /// Oracle: the worked packet from `docs/networking/01-packet-format.md` §6, i.e. real
    /// client-derived bytes rather than something invented for the harness.
    #[test]
    fn compare_accepts_an_identical_datagram() {
        let dg = worked_datagram();
        assert!(compare(0, &dg, &dg).is_ok());
    }

    /// And names the field when one differs.
    #[test]
    fn compare_names_the_differing_field() {
        let dg = worked_datagram();

        let mut other = dg.clone();
        other[0] = 0x02; // seq_id
        let err = compare(0, &dg, &other).expect_err("seq_id differs");
        assert!(
            matches!(
                err,
                ReplayError::Mismatch {
                    field: "seq_id",
                    ..
                }
            ),
            "{err}"
        );

        let mut other = dg.clone();
        other[8] ^= 0xFF; // checksum
        let err = compare(0, &dg, &other).expect_err("checksum differs");
        assert!(
            matches!(
                err,
                ReplayError::Mismatch {
                    field: "checksum",
                    ..
                }
            ),
            "{err}"
        );

        let mut other = dg.clone();
        other[44] ^= 0xFF; // a fragment payload byte
        let err = compare(0, &dg, &other).expect_err("fragment payload differs");
        assert!(
            matches!(
                err,
                ReplayError::Mismatch {
                    field: "checksum" | "fragment payload",
                    ..
                }
            ),
            "{err}"
        );
    }

    /// Normalisation copies the captured TimeSync payload in and recomputes the checksum, so a
    /// packet that differs only in its time-sensitive payload compares equal — which is what §5.4's
    /// rule means, and what a literal reading of it cannot deliver.
    #[test]
    fn normalisation_makes_a_time_sensitive_payload_comparable() {
        let key = 0x5DA2_2D96u32;
        let build = |game_time: f64| {
            let mut p = OutPacket::new(ProtoHeader {
                seq_id: 7,
                rec_id: 0x0B,
                interval: 0x0100,
                iteration: 1,
                ..Default::default()
            });
            p.add_optional_header(PacketFlags::TIME_SYNC, game_time.to_le_bytes().to_vec())
                .expect("timesync");
            p
        };

        let captured_bytes = build(1234.5).serialize(Some(key)).expect("captured");
        let captured = ParsedPacket::parse(&captured_bytes).expect("parse");

        // Our stack stamps a different game time, so a blind memcmp fails...
        let mut ours = build(9999.0);
        let naive = ours.clone().serialize(Some(key)).expect("naive");
        assert!(compare(0, &captured_bytes, &naive).is_err());

        // ...and normalisation makes it exact.
        let normalised = normalise_against(&mut ours, &captured, Some(key)).expect("normalise");
        assert!(compare(0, &captured_bytes, &normalised).is_ok());
    }

    /// Normalisation also carries `interval` and the `Flow` section across, both of which are
    /// wall-clock driven.
    #[test]
    fn normalisation_carries_interval_and_flow() {
        let build = |interval: u16, flow: [u8; 6]| {
            let mut p = OutPacket::new(ProtoHeader {
                seq_id: 3,
                rec_id: 0x0B,
                interval,
                iteration: 1,
                ..Default::default()
            });
            p.add_optional_header(PacketFlags::FLOW, flow.to_vec())
                .expect("flow");
            p.add_fragment(Fragment::new(FragmentHeader::default(), vec![1, 2, 3, 4]))
                .expect("frag");
            p
        };
        let key = 0x1234_5678u32;
        let captured_bytes = build(0x0100, [1, 2, 3, 4, 5, 6])
            .serialize(Some(key))
            .expect("cap");
        let captured = ParsedPacket::parse(&captured_bytes).expect("parse");

        let mut ours = build(0x0999, [9, 9, 9, 9, 9, 9]);
        let normalised = normalise_against(&mut ours, &captured, Some(key)).expect("normalise");
        assert!(compare(0, &captured_bytes, &normalised).is_ok());
        // The fragment payload was never touched, which is the point of the exception.
        assert_eq!(
            ParsedPacket::parse(&normalised).expect("parse").fragments[0].payload,
            vec![1, 2, 3, 4]
        );
    }

    /// The interval offset is established once, from the first packet, and then applied.
    #[test]
    fn interval_offset_is_a_fixed_wrapping_difference() {
        assert_eq!(interval_offset(0x0100, 0x0002), 0x00FE);
        assert_eq!(interval_offset(0x0002, 0x0100), 0xFF02);
        assert_eq!(
            0x0002u16.wrapping_add(interval_offset(0x0100, 0x0002)),
            0x0100
        );
    }

    /// A missing corpus is a clean, named error, not a panic — so the gate test can report it.
    #[test]
    fn a_missing_capture_is_a_named_error() {
        let err = Capture::load(Path::new("no/such/place"), "handshake")
            .expect_err("there is no corpus there");
        assert!(matches!(err, ReplayError::NoCapture(_)), "{err}");
        assert!(err.to_string().contains("record a session there"), "{err}");
    }

    /// The seeds come out of the recording's own `ConnectRequest`, and a recording without one is
    /// refused rather than replayed under default seeds.
    #[test]
    fn the_seeds_are_read_from_the_recordings_connect_request() {
        let cr = ConnectRequest {
            server_time: 1.5,
            cookie: 7,
            net_id: 0x0B,
            outgoing_seed: 0x1122_3344,
            incoming_seed: 0x5566_7788,
        };
        let mut p = OutPacket::new(ProtoHeader {
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::CONNECT_REQUEST, cr.to_bytes().to_vec())
            .expect("connect request");
        let bytes = p.serialize(None).expect("serialize");
        let dir = std::env::temp_dir().join(format!("dereth-net-replay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let line = format!(
            "{{\"t\": 0.0, \"dir\": \"s2c\", \"pair\": 0, \"len\": {}, \"data\": \"{}\"}}\n",
            bytes.len(),
            hex(&bytes)
        );
        std::fs::write(dir.join("one.jsonl"), &line).expect("write");
        let capture = Capture::load(&dir, "one").expect("loads");
        assert_eq!(capture.meta.outgoing_seed, 0x1122_3344);
        assert_eq!(capture.meta.incoming_seed, 0x5566_7788);
        assert_eq!(capture.meta.net_id, 0x0B);
        assert_eq!(capture.meta.iteration, 1);
        assert_eq!(capture.packets.len(), 1);

        std::fs::write(dir.join("two.jsonl"), line.replace("s2c", "c2s")).expect("write");
        let err = Capture::load(&dir, "two").expect_err("no ConnectRequest from the server");
        assert!(matches!(err, ReplayError::Malformed(_)), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn worked_datagram() -> Vec<u8> {
        decode_hex(
            "01000000060000002e4bd6490b0000011c000100\
             010000000000008001001c0000000900b0f700000100005002000000",
        )
        .expect("the worked example from docs/networking/01-packet-format.md section 6")
    }
}
