//! The scannable regions of a recording, and the map back to the bytes on disk.
//!
//! # Why a name can straddle two datagrams
//!
//! A blob is fragmented across datagrams at 448 payload bytes, and the fragmenter does not care
//! what it cuts through. A character name near a fragment boundary is therefore *not* present as a
//! contiguous run in any single datagram — it exists only in the reassembled blob. A scrubber that
//! scanned datagram payloads one at a time would silently miss those occurrences, and "silently"
//! is the whole problem: nothing downstream would ever notice.
//!
//! So the unit of scanning is the **reassembled blob**, and every region carries a map from its
//! own offsets back to `(datagram, byte range)` pairs — a *list* of them per piece, because a
//! retransmitted datagram carries a second copy of the same fragment and both copies have to be
//! substituted or the two would disagree.
//!
//! The optional-header sections are the other half of a datagram's payload. They are never
//! fragmented, so each becomes a one-piece region of its own. Sections and fragments together
//! account for every payload byte (`walk::layout` refuses a datagram where they do not), which is
//! what lets the scrubber claim the scan covers the recording rather than most of it.

use std::collections::BTreeMap;

use crate::raw::{Dir, Recording};
use crate::walk::{layout, WalkError};

/// One place a run of bytes lives on disk.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Index of the datagram within the recording.
    pub datagram: usize,
    /// The byte range within that datagram.
    pub at: std::ops::Range<usize>,
}

/// A contiguous run of a region, and every place it occurs.
#[derive(Debug, Clone)]
pub struct Piece {
    /// Where this run starts within the region's buffer.
    pub at: usize,
    /// How long it is.
    pub len: usize,
    /// Every datagram byte range carrying it: one normally, more when a datagram was retransmitted.
    pub occurrences: Vec<Segment>,
}

/// What a region is.
#[derive(Debug, Clone)]
pub enum Kind {
    /// A reassembled blob: its direction, its opcode and its index in the blob stream.
    Blob {
        /// Which way it went.
        dir: Dir,
        /// The blob's first dword.
        opcode: u32,
        /// The datagram its first fragment arrived in.
        first_datagram: usize,
        /// Whether every fragment the blob claims was present.
        complete: bool,
    },
    /// One optional-header section: its datagram and its `header_` mask.
    Section {
        /// Which way it went.
        dir: Dir,
        /// The datagram it belongs to.
        datagram: usize,
        /// Its `header_` bit.
        mask: u32,
    },
}

/// A buffer to scan, plus the map back to disk.
#[derive(Debug, Clone)]
pub struct Region {
    /// What this region is.
    pub kind: Kind,
    /// The bytes to scan.
    pub buf: Vec<u8>,
    /// The runs of `buf`, in order, and where each one lives.
    pub pieces: Vec<Piece>,
}

impl Region {
    /// Every disk byte position carrying region offset `off`.
    ///
    /// Normally one; two or more when the fragment was retransmitted.
    #[must_use]
    pub fn disk_positions(&self, off: usize) -> Vec<(usize, usize)> {
        let Some(p) = self
            .pieces
            .iter()
            .find(|p| off >= p.at && off < p.at + p.len)
        else {
            return Vec::new();
        };
        let within = off - p.at;
        p.occurrences
            .iter()
            .map(|s| (s.datagram, s.at.start + within))
            .collect()
    }
}

/// Every region of one recording, plus what the reassembly found.
#[derive(Debug)]
pub struct Regions {
    /// The regions, blobs first (in completion order) and then sections.
    pub regions: Vec<Region>,
    /// Blobs whose fragments did not all arrive. Zero for every locked recording.
    pub incomplete_blobs: usize,
    /// Fragments that arrived more than once. The transport's retransmissions.
    pub duplicate_fragments: usize,
}

/// One blob under construction.
#[derive(Debug, Default)]
struct Pending {
    num_frags: u16,
    first_datagram: usize,
    /// `blob_num` -> (bytes, every place they live).
    parts: BTreeMap<u16, (Vec<u8>, Vec<Segment>)>,
}

/// Build the scan regions of a whole recording.
///
/// # Errors
/// [`WalkError`] when a datagram does not walk; see [`crate::walk::layout`].
pub fn build(rec: &Recording) -> Result<Regions, WalkError> {
    let mut pending: BTreeMap<(Dir, u64), Pending> = BTreeMap::new();
    let mut order: Vec<(Dir, u64)> = Vec::new();
    let mut sections: Vec<Region> = Vec::new();
    let mut duplicate_fragments = 0usize;

    for dg in &rec.datagrams {
        let l = layout(&dg.bytes).map_err(|e| WalkError(format!("datagram {}: {e}", dg.idx)))?;
        for s in l.sections {
            sections.push(Region {
                kind: Kind::Section {
                    dir: dg.dir,
                    datagram: dg.idx,
                    mask: s.mask,
                },
                buf: dg.bytes[s.at.clone()].to_vec(),
                pieces: vec![Piece {
                    at: 0,
                    len: s.at.len(),
                    occurrences: vec![Segment {
                        datagram: dg.idx,
                        at: s.at,
                    }],
                }],
            });
        }
        for f in l.frags {
            let key = (dg.dir, f.blob_id);
            let slot = pending.entry(key).or_insert_with(|| {
                order.push(key);
                Pending {
                    num_frags: f.num_frags,
                    first_datagram: dg.idx,
                    parts: BTreeMap::new(),
                }
            });
            let seg = Segment {
                datagram: dg.idx,
                at: f.at.clone(),
            };
            match slot.parts.get_mut(&f.blob_num) {
                Some((_, places)) => {
                    // A retransmission. The bytes are the same; what matters is that both copies
                    // are substituted, so the second place is recorded against the first's bytes.
                    duplicate_fragments += 1;
                    places.push(seg);
                }
                None => {
                    slot.parts
                        .insert(f.blob_num, (dg.bytes[f.at].to_vec(), vec![seg]));
                }
            }
        }
    }

    let mut regions = Vec::new();
    let mut incomplete_blobs = 0usize;
    for key in order {
        let Some(p) = pending.remove(&key) else {
            continue;
        };
        let complete = usize::from(p.num_frags) == p.parts.len()
            && (0..p.num_frags).all(|n| p.parts.contains_key(&n));
        if !complete {
            incomplete_blobs += 1;
        }
        let mut buf = Vec::new();
        let mut pieces = Vec::new();
        for (_, (bytes, places)) in p.parts {
            pieces.push(Piece {
                at: buf.len(),
                len: bytes.len(),
                occurrences: places,
            });
            buf.extend_from_slice(&bytes);
        }
        let opcode = if buf.len() >= 4 {
            u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
        } else {
            0
        };
        regions.push(Region {
            kind: Kind::Blob {
                dir: key.0,
                opcode,
                first_datagram: p.first_datagram,
                complete,
            },
            buf,
            pieces,
        });
    }
    regions.extend(sections);
    Ok(Regions {
        regions,
        incomplete_blobs,
        duplicate_fragments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::Recording;
    use dereth_transport::wire::{Fragment, FragmentHeader, OutPacket, PacketFlags, ProtoHeader};

    /// The write-back `main` performs, as one call: put `replacement` at region offset `at` in
    /// every datagram carrying it, and report how many bytes were written.
    fn apply(
        region: &Region,
        datagrams: &mut [crate::raw::Datagram],
        at: usize,
        replacement: &[u8],
    ) -> usize {
        let mut written = 0usize;
        for (i, b) in replacement.iter().enumerate() {
            for (dg, pos) in region.disk_positions(at + i) {
                datagrams[dg].bytes[pos] = *b;
                written += 1;
            }
        }
        written
    }

    fn datagram_line(dir: &str, bytes: &[u8]) -> String {
        format!(
            "{{\"t\": 0.0, \"dir\": \"{dir}\", \"pair\": 0, \"len\": {}, \"data\": \"{}\"}}",
            bytes.len(),
            crate::raw::encode_hex(bytes)
        )
    }

    fn frag_packet(seq: u32, blob_id: u32, num: u16, of: u16, payload: &[u8]) -> Vec<u8> {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: seq,
            rec_id: 0x0B,
            iteration: 1,
            ..Default::default()
        });
        p.add_fragment(Fragment::new(
            FragmentHeader {
                blob_id_low: blob_id,
                blob_id_high: 0,
                num_frags: of,
                blob_num: num,
                queue_id: 9,
                ..Default::default()
            },
            payload.to_vec(),
        ))
        .expect("frag");
        p.serialize(Some(1)).expect("serialize")
    }

    fn recording_of(lines: &[String]) -> Recording {
        // One file per call: two tests with the same line count ran in parallel on one path and
        // raced (seen once in the root gate on merge). The counter makes the name unique.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("dere-corpus-regions-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).expect("tmp");
        let path = dir.join("t.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");
        let rec = Recording::read(&path).expect("read");
        std::fs::remove_file(&path).ok();
        rec
    }

    /// **The property this module exists for.** A name cut in half by the fragmenter is found in
    /// the reassembled blob and written back into both datagrams.
    #[test]
    fn a_name_split_across_two_fragments_is_one_region() {
        // "....Thornwick" with the split falling inside the name.
        let a = frag_packet(1, 77, 0, 2, b"\xB0\xF7\x00\x00Thor");
        let b = frag_packet(2, 77, 1, 2, b"nwick!!!");
        let rec = recording_of(&[datagram_line("s2c", &a), datagram_line("s2c", &b)]);
        let built = build(&rec).expect("regions");
        let blob = built
            .regions
            .iter()
            .find(|r| matches!(r.kind, Kind::Blob { .. }))
            .expect("one blob");
        assert_eq!(&blob.buf[4..], b"Thornwick!!!");
        assert_eq!(built.incomplete_blobs, 0);
        assert_eq!(built.duplicate_fragments, 0);

        let mut dgs = rec.datagrams.clone();
        let written = apply(blob, &mut dgs, 4, b"Ravenshold!!");
        assert_eq!(written, 12);
        assert!(dgs[0].bytes.ends_with(b"Rave"));
        assert!(dgs[1].bytes.ends_with(b"nshold!!"));
    }

    /// A retransmitted fragment is substituted in both copies, or the corpus would carry one
    /// scrubbed datagram and one that is not.
    #[test]
    fn a_retransmitted_fragment_is_substituted_in_every_copy() {
        let a = frag_packet(1, 90, 0, 1, b"\xB0\xF7\x00\x00Aldis");
        let rec = recording_of(&[datagram_line("c2s", &a), datagram_line("c2s", &a)]);
        let built = build(&rec).expect("regions");
        assert_eq!(built.duplicate_fragments, 1);
        let blob = &built.regions[0];
        let mut dgs = rec.datagrams.clone();
        assert_eq!(apply(blob, &mut dgs, 4, b"Brann"), 10);
        assert!(dgs[0].bytes.ends_with(b"Brann"));
        assert!(dgs[1].bytes.ends_with(b"Brann"));
    }

    /// The optional block is scanned too: the account in a `LoginRequest` is not in a fragment.
    #[test]
    fn optional_sections_become_their_own_regions() {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 0,
            ..Default::default()
        });
        let auth =
            dereth_transport::conn::ConnectionAuthenticator::account_password("ac01", "secret");
        p.add_optional_header(
            PacketFlags::LOGIN_REQUEST,
            dereth_transport::conn::build_login_request(&auth),
        )
        .expect("login request");
        let bytes = p.serialize(None).expect("serialize");
        let rec = recording_of(&[datagram_line("c2s", &bytes)]);
        let built = build(&rec).expect("regions");
        let sec = built
            .regions
            .iter()
            .find(|r| matches!(r.kind, Kind::Section { mask, .. } if mask == PacketFlags::LOGIN_REQUEST))
            .expect("the LoginRequest section");
        assert!(sec.buf.windows(4).any(|w| w == b"ac01"));
    }
}
