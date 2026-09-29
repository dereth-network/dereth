//! Where the bytes of one datagram live.
//!
//! The substitution is a byte-level edit, so it needs offsets rather than values: which byte range
//! of the datagram is an optional-header section, and which is a fragment's payload. Both come
//! from `dereth-transport`'s own tables (`OPTIONAL_HEADERS`, `section_len`, `FRAG_HEADER_SIZE`), so a
//! section this walker mis-measures is a section the transport mis-measures too.
//!
//! A datagram is `[20-byte header][optional sections, ascending mask order][fragments]`. Nothing
//! else is in it, which is what lets the scrubber say that scanning the optional block plus every
//! fragment payload is scanning the whole of the datagram's payload.

use dereth_transport::wire::optional::section_len;
use dereth_transport::wire::{PacketFlags, OPTIONAL_HEADERS};
use dereth_transport::wire::{FRAG_HEADER_SIZE, HEADER_SIZE};

/// One optional-header section's place in the datagram.
#[derive(Debug, Clone)]
pub struct Section {
    /// The `header_` bit that selects it.
    pub mask: u32,
    /// Its body's byte range within the datagram.
    pub at: std::ops::Range<usize>,
}

/// One fragment's place in the datagram.
#[derive(Debug, Clone)]
pub struct Frag {
    /// The blob id, the two halves joined.
    pub blob_id: u64,
    /// How many fragments the blob has.
    pub num_frags: u16,
    /// This fragment's index within the blob.
    pub blob_num: u16,
    /// The fragment's **payload** byte range within the datagram (its 16-byte header excluded).
    pub at: std::ops::Range<usize>,
}

/// What a datagram is made of.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// The optional-header sections, in wire order.
    pub sections: Vec<Section>,
    /// The fragments, in wire order.
    pub frags: Vec<Frag>,
}

/// Why a datagram could not be walked.
#[derive(Debug)]
pub struct WalkError(pub String);

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WalkError {}

/// Walk one datagram.
///
/// # Errors
/// [`WalkError`] when the header's `datalen_` disagrees with the buffer, a section runs past the
/// end, or a fragment claims a size its packet cannot hold. All three are states the recordings
/// are already asserted not to be in (`corpus.rs`), so meeting one here
/// means the file on disk is not the file that was checked.
pub fn layout(buf: &[u8]) -> Result<Layout, WalkError> {
    if buf.len() < HEADER_SIZE {
        return Err(WalkError(format!(
            "{} bytes, shorter than the 20-byte header",
            buf.len()
        )));
    }
    let flags = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let datalen = usize::from(u16::from_le_bytes([buf[16], buf[17]]));
    if HEADER_SIZE + datalen != buf.len() {
        return Err(WalkError(format!(
            "datalen_ {datalen} but {} payload bytes",
            buf.len() - HEADER_SIZE
        )));
    }

    let mut out = Layout::default();
    let mut at = HEADER_SIZE;
    for spec in &OPTIONAL_HEADERS {
        if flags & spec.mask == 0 {
            continue;
        }
        let len = section_len(spec.mask, &buf[at..])
            .map_err(|e| WalkError(format!("section {:#010X}: {e}", spec.mask)))?;
        out.sections.push(Section {
            mask: spec.mask,
            at: at..at + len,
        });
        at += len;
    }

    if flags & PacketFlags::BLOB_FRAGMENTS != 0 {
        while at + FRAG_HEADER_SIZE <= buf.len() {
            let u16at = |o: usize| u16::from_le_bytes([buf[at + o], buf[at + o + 1]]);
            let u32at = |o: usize| {
                u32::from_le_bytes([
                    buf[at + o],
                    buf[at + o + 1],
                    buf[at + o + 2],
                    buf[at + o + 3],
                ])
            };
            let blob_id = u64::from(u32at(0)) | (u64::from(u32at(4)) << 32);
            let num_frags = u16at(8);
            let frag_size = usize::from(u16at(10));
            let blob_num = u16at(12);
            if frag_size < FRAG_HEADER_SIZE || at + frag_size > buf.len() {
                return Err(WalkError(format!(
                    "fragment size {frag_size} overruns its packet"
                )));
            }
            out.frags.push(Frag {
                blob_id,
                num_frags,
                blob_num,
                at: at + FRAG_HEADER_SIZE..at + frag_size,
            });
            at += frag_size;
        }
    }
    if at != buf.len() {
        return Err(WalkError(format!(
            "{} trailing byte(s) after the last section or fragment",
            buf.len() - at
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_transport::wire::{Fragment, FragmentHeader, OutPacket, ProtoHeader};

    /// The walker's ranges are the ranges the serialiser wrote, for a packet carrying one
    /// optional section and two fragments. Built with `dereth-transport`'s own `OutPacket`, so this is
    /// the two implementations agreeing rather than one asserting about itself.
    #[test]
    fn sections_and_fragment_payloads_land_where_the_serialiser_put_them() {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 3,
            rec_id: 0x0B,
            interval: 0x0100,
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::FLOW, vec![1, 2, 3, 4, 5, 6])
            .expect("flow");
        p.add_fragment(Fragment::new(
            FragmentHeader {
                blob_id_low: 7,
                num_frags: 2,
                blob_num: 0,
                ..Default::default()
            },
            b"first-half".to_vec(),
        ))
        .expect("frag 0");
        p.add_fragment(Fragment::new(
            FragmentHeader {
                blob_id_low: 7,
                num_frags: 2,
                blob_num: 1,
                ..Default::default()
            },
            b"second-half".to_vec(),
        ))
        .expect("frag 1");
        let bytes = p.serialize(Some(0x1234_5678)).expect("serialize");

        let l = layout(&bytes).expect("walk");
        assert_eq!(l.sections.len(), 1);
        assert_eq!(l.sections[0].mask, PacketFlags::FLOW);
        assert_eq!(&bytes[l.sections[0].at.clone()], &[1, 2, 3, 4, 5, 6]);
        assert_eq!(l.frags.len(), 2);
        assert_eq!(&bytes[l.frags[0].at.clone()], b"first-half");
        assert_eq!(&bytes[l.frags[1].at.clone()], b"second-half");
        assert_eq!(l.frags[0].blob_id, 7);
        assert_eq!(l.frags[1].blob_num, 1);
    }

    /// A datagram whose `datalen_` disagrees with its buffer is refused rather than walked
    /// half-way: the whole point of the walk is that every payload byte is accounted for.
    #[test]
    fn a_disagreeing_datalen_is_refused() {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 1,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::ACK_SEQUENCE, vec![0; 4])
            .expect("ack");
        let mut bytes = p.serialize(None).expect("serialize");
        bytes.push(0);
        let err = layout(&bytes).expect_err("datalen_ disagrees");
        assert!(err.to_string().contains("datalen_"), "{err}");
    }
}
