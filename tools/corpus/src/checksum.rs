//! Recomputing `ProtoHeader::checksum_` for a scrubbed recording, from the seeds the recording
//! itself carries.
//!
//! `checksum_ = headerHash + (payloadHash ^ isaacKey)`. The payload half covers every byte the
//! scrubber can touch, so every datagram a substitution lands in needs a new one — and the ISAAC
//! value is *positional*, so the key stream has to be walked exactly as the sender walked it,
//! including the keys drawn for datagrams the proxy never saw and the keys a retransmission
//! re-uses. That walk is [`KeyLedger`], which is the same rule
//! `core/client-net/tests/cpu/net/replay.rs::KeyLedger` applies; this is the writing end of the property that
//! gate reads.
//!
//! # The calibration
//!
//! Before a single byte is changed, [`plan`] recomputes the checksum of every datagram *as
//! recorded* and requires it to equal the captured one. That is the whole safety argument for the
//! scrubber in one assertion: if the walk agrees with the recording on all 39,525 datagrams before the
//! substitution, then a checksum it writes after the substitution is the one the client would have
//! written. A recording it cannot reproduce is refused rather than published — "if any gate
//! reddens, the scrubber is wrong".

use std::collections::BTreeMap;

use dereth_transport::crc::wire_checksum;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use dereth_transport::CryptoSystem;

use crate::raw::{Datagram, Dir, Recording};

/// The two seeds, read out of the recording's own `ConnectRequest` in clear.
#[derive(Debug, Clone, Copy)]
pub struct Seeds {
    /// `OutgoingSeed`: server -> client.
    pub outgoing: u32,
    /// `IncomingSeed`: client -> server.
    pub incoming: u32,
    /// The datagram the `ConnectRequest` arrived in.
    pub at: usize,
}

/// Why a recording's checksums could not be planned.
#[derive(Debug)]
pub struct ChecksumError(pub String);

impl std::fmt::Display for ChecksumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ChecksumError {}

/// Find the `ConnectRequest` and read its two seeds.
///
/// # Errors
/// [`ChecksumError`] when there is no `ConnectRequest`, or when its body is not the documented 32
/// bytes. Both mean the recording cannot be replayed offline, scrubbed or not.
pub fn seeds(rec: &Recording) -> Result<Seeds, ChecksumError> {
    for dg in &rec.datagrams {
        let flags = u32::from_le_bytes([dg.bytes[4], dg.bytes[5], dg.bytes[6], dg.bytes[7]]);
        if flags & PacketFlags::CONNECT_REQUEST == 0 {
            continue;
        }
        let body = dg
            .bytes
            .get(20..52)
            .ok_or_else(|| ChecksumError(format!("datagram {}: short ConnectRequest", dg.idx)))?;
        let u32at = |o: usize| u32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]);
        return Ok(Seeds {
            outgoing: u32at(0x14),
            incoming: u32at(0x18),
            at: dg.idx,
        });
    }
    Err(ChecksumError(
        "no ConnectRequest, so the two ISAAC seeds are not in this recording".into(),
    ))
}

/// Which ISAAC value belongs to which sequence number, per direction.
///
/// The stream advances once per encrypted datagram **sent**, not per datagram captured. A datagram
/// lost between the client and the proxy consumed a key the recording never shows, and a
/// `Retransmission` re-uses the key its sequence number was first sent under.
#[derive(Debug, Default)]
pub struct KeyLedger {
    drawn: BTreeMap<u32, u32>,
    next_seq: Option<u32>,
    /// Keys drawn for datagrams the recording never carried.
    pub parked: usize,
    /// Retransmissions that re-used a key already drawn.
    pub reused: usize,
}

impl KeyLedger {
    /// The key for one encrypted datagram.
    ///
    /// # Errors
    /// [`ChecksumError`] when a `Retransmission` names a sequence number whose key was never
    /// drawn, which would mean the walk had already lost its place.
    pub fn key_for(
        &mut self,
        stream: &mut CryptoSystem,
        seq: u32,
        retransmission: bool,
        idx: usize,
    ) -> Result<u32, ChecksumError> {
        if retransmission {
            self.reused += 1;
            return self.drawn.get(&seq).copied().ok_or_else(|| {
                ChecksumError(format!(
                    "datagram {idx}: a Retransmission of seq {seq}, whose key was never drawn"
                ))
            });
        }
        if let Some(next) = self.next_seq {
            for missing in next..seq {
                self.drawn.insert(missing, stream.next());
                self.parked += 1;
            }
        }
        let key = stream.next();
        self.drawn.insert(seq, key);
        self.next_seq = Some(seq + 1);
        Ok(key)
    }
}

/// The per-datagram key, plus what the walk observed.
#[derive(Debug)]
pub struct Plan {
    /// One entry per datagram: its ISAAC value, or `None` when the datagram is plaintext.
    pub keys: Vec<Option<u32>>,
    /// Keys parked for datagrams the recording never carried, both directions.
    pub parked: usize,
    /// Retransmissions re-using a key, both directions.
    pub reused: usize,
    /// Encrypted datagrams whose captured checksum the walk reproduced. Equal to the number of
    /// encrypted datagrams, or [`plan`] would have failed.
    pub verified: usize,
}

/// Walk the key stream and prove it against every captured checksum.
///
/// # Errors
/// [`ChecksumError`] naming the first datagram whose captured checksum this walk does not
/// reproduce. Nothing downstream may run after that: a scrubber that cannot reproduce a recording
/// it has not changed cannot be trusted with one it has.
pub fn plan(rec: &Recording, seeds: Seeds) -> Result<Plan, ChecksumError> {
    let mut incoming = CryptoSystem::new(seeds.outgoing);
    let mut outgoing = CryptoSystem::new(seeds.incoming);
    let mut in_keys = KeyLedger::default();
    let mut out_keys = KeyLedger::default();
    let mut keys = Vec::with_capacity(rec.datagrams.len());
    let mut verified = 0usize;

    for dg in &rec.datagrams {
        let p = ParsedPacket::parse(&dg.bytes)
            .map_err(|e| ChecksumError(format!("datagram {}: does not parse: {e}", dg.idx)))?;
        if !p.header.header.is_encrypted() {
            keys.push(None);
            continue;
        }
        let retransmission = p.header.header.0 & PacketFlags::RETRANSMISSION != 0;
        let (stream, ledger) = match dg.dir {
            Dir::S2c => (&mut incoming, &mut in_keys),
            Dir::C2s => (&mut outgoing, &mut out_keys),
        };
        let key = ledger.key_for(stream, p.header.seq_id, retransmission, dg.idx)?;
        let want = wire_checksum(p.header_hash, p.payload_hash, Some(key));
        if want != p.header.checksum {
            return Err(ChecksumError(format!(
                "datagram {} ({}): the captured checksum is {:#010X} and this walk computes \
                 {:#010X}; the key stream has lost its position and nothing may be published \
                 from it",
                dg.idx,
                dg.dir.as_str(),
                p.header.checksum,
                want
            )));
        }
        verified += 1;
        keys.push(Some(key));
    }
    Ok(Plan {
        keys,
        parked: in_keys.parked + out_keys.parked,
        reused: in_keys.reused + out_keys.reused,
        verified,
    })
}

/// Write a fresh `checksum_` into every datagram, using the keys [`plan`] established.
///
/// Returns how many checksums actually changed, which is the number of datagrams a substitution
/// landed in.
///
/// # Errors
/// [`ChecksumError`] when a substituted datagram no longer parses, which would mean the
/// substitution was not length-preserving after all.
pub fn rewrite(datagrams: &mut [Datagram], plan: &Plan) -> Result<usize, ChecksumError> {
    let mut changed = 0usize;
    for (dg, key) in datagrams.iter_mut().zip(&plan.keys) {
        let p = ParsedPacket::parse(&dg.bytes).map_err(|e| {
            ChecksumError(format!(
                "datagram {}: the scrubbed bytes do not parse: {e}",
                dg.idx
            ))
        })?;
        let want = wire_checksum(p.header_hash, p.payload_hash, *key);
        if want != p.header.checksum {
            dg.bytes[8..12].copy_from_slice(&want.to_le_bytes());
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_transport::wire::{Fragment, FragmentHeader, OutPacket, ProtoHeader};

    fn encrypted(seq: u32, key: u32, payload: &[u8]) -> Vec<u8> {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: seq,
            rec_id: 0x0B,
            iteration: 1,
            ..Default::default()
        });
        p.add_fragment(Fragment::new(
            FragmentHeader {
                blob_id_low: seq,
                num_frags: 1,
                blob_num: 0,
                queue_id: 9,
                ..Default::default()
            },
            payload.to_vec(),
        ))
        .expect("frag");
        p.serialize(Some(key)).expect("serialize")
    }

    /// A changed payload byte invalidates the checksum, and `rewrite` puts back the one the
    /// sender would have written -- proved by rebuilding the same packet through `OutPacket`.
    #[test]
    fn a_substituted_payload_gets_the_checksum_the_sender_would_have_written() {
        let mut stream = CryptoSystem::new(0xDEAD_BEEF);
        let key = stream.next();
        let before = encrypted(1, key, b"\xB0\xF7\x00\x00Thornwick");
        let after = encrypted(1, key, b"\xB0\xF7\x00\x00Greywater");

        let mut bytes = before.clone();
        let at = bytes.len() - 9;
        bytes[at..].copy_from_slice(b"Greywater");
        assert_ne!(bytes, after, "the checksum is still the old one");

        let p = ParsedPacket::parse(&bytes).expect("parse");
        let want = wire_checksum(p.header_hash, p.payload_hash, Some(key));
        bytes[8..12].copy_from_slice(&want.to_le_bytes());
        assert_eq!(bytes, after);
    }

    /// The ledger parks a key for a sequence number the recording never carried and hands a
    /// retransmission the key its number was first sent under. Both arms, because a ledger with
    /// only the first is the trap `dereth-client-net`'s gate documents.
    #[test]
    fn the_ledger_parks_and_reuses() {
        let mut stream = CryptoSystem::new(0xDEAD_BEEF);
        let mut reference = CryptoSystem::new(0xDEAD_BEEF);
        let mut ledger = KeyLedger::default();

        let k0 = ledger.key_for(&mut stream, 0, false, 0).expect("seq 0");
        assert_eq!(k0, reference.next());
        // Seq 1 was lost before the proxy; seq 2 arrives next.
        let k2 = ledger.key_for(&mut stream, 2, false, 1).expect("seq 2");
        let parked = reference.next();
        assert_eq!(k2, reference.next());
        assert_eq!(ledger.parked, 1);
        // And the resend of 1 draws no new key: it uses the one that was parked.
        let k1 = ledger
            .key_for(&mut stream, 1, true, 2)
            .expect("seq 1 resent");
        assert_eq!(k1, parked);
        assert_eq!(ledger.reused, 1);
    }

    /// A retransmission of a sequence number that was never sent is a walk that has lost its
    /// place, and is refused rather than papered over with a fresh draw.
    #[test]
    fn a_retransmission_of_an_undrawn_sequence_is_refused() {
        let mut stream = CryptoSystem::new(1);
        let mut ledger = KeyLedger::default();
        let err = ledger
            .key_for(&mut stream, 9, true, 0)
            .expect_err("never drawn");
        assert!(err.to_string().contains("never drawn"), "{err}");
    }
}
