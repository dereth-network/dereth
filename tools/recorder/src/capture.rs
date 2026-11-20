//! The recording's file: a classic pcap capture, one raw IPv4/UDP frame per datagram.
//!
//! Each datagram is written as a capture taken between the client and the server would show it:
//! from the client's address to the server's port it was sent to, or from the server's address and
//! port to the client. So the file opens in any capture viewer, and the capture index
//! (`dereth-pcap`) and the reader here take it apart with the same code they read any capture
//! with. The frames carry no link-layer header (`LINKTYPE_RAW`); the IPv4 header carries its
//! checksum and the UDP header none, which IPv4 allows.

use std::io::{self, Write};
use std::net::SocketAddrV4;
use std::time::{SystemTime, UNIX_EPOCH};

use dereth_pcap::link::build::{ipv4, udp};

/// The frame type: a bare IP packet.
pub const LINKTYPE_RAW: u32 = 101;

/// The classic pcap magic, microsecond stamps.
const MAGIC: u32 = 0xA1B2_C3D4;

/// The longest frame recorded: an IPv4 packet at its limit.
const SNAPLEN: u32 = 65_535;

/// Writes a capture.
#[derive(Debug)]
pub struct CaptureWriter<W: Write> {
    out: W,
    next_id: u16,
}

impl<W: Write> CaptureWriter<W> {
    /// Start a capture: write its file header.
    ///
    /// # Errors
    /// When the header cannot be written.
    pub fn new(mut out: W) -> io::Result<Self> {
        let mut h = Vec::with_capacity(24);
        h.extend_from_slice(&MAGIC.to_le_bytes());
        h.extend_from_slice(&2u16.to_le_bytes());
        h.extend_from_slice(&4u16.to_le_bytes());
        h.extend_from_slice(&0i32.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(&SNAPLEN.to_le_bytes());
        h.extend_from_slice(&LINKTYPE_RAW.to_le_bytes());
        out.write_all(&h)?;
        Ok(Self { out, next_id: 1 })
    }

    /// Write one datagram that went from `from` to `to` at `at`.
    ///
    /// # Errors
    /// When the record cannot be written, or the datagram is too long for one IPv4 packet.
    pub fn datagram(
        &mut self,
        at: SystemTime,
        from: SocketAddrV4,
        to: SocketAddrV4,
        payload: &[u8],
    ) -> io::Result<()> {
        if payload.len() > usize::from(u16::MAX) - 28 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "a datagram of {} bytes is too long to record",
                    payload.len()
                ),
            ));
        }
        let mut frame = ipv4(
            *from.ip(),
            *to.ip(),
            self.next_id,
            0,
            17,
            &udp(from.port(), to.port(), payload),
        );
        self.next_id = self.next_id.wrapping_add(1);
        let sum = header_checksum(&frame[..20]);
        frame[10..12].copy_from_slice(&sum.to_be_bytes());

        let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
        let len = u32::try_from(frame.len()).unwrap_or(u32::MAX);
        let mut r = Vec::with_capacity(16 + frame.len());
        r.extend_from_slice(
            &u32::try_from(since.as_secs())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        r.extend_from_slice(&since.subsec_micros().to_le_bytes());
        r.extend_from_slice(&len.to_le_bytes());
        r.extend_from_slice(&len.to_le_bytes());
        r.extend_from_slice(&frame);
        self.out.write_all(&r)
    }

    /// Push what is written to the file.
    ///
    /// # Errors
    /// When the writer cannot flush.
    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    /// The writer back.
    pub fn into_inner(self) -> W {
        self.out
    }
}

/// The IPv4 header checksum: the ones' complement of the ones'-complement sum of its words, taken
/// with the checksum field zero.
fn header_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;
    for (i, w) in header.chunks(2).enumerate() {
        if i == 5 {
            continue;
        }
        sum += u32::from(u16::from_be_bytes([w[0], *w.get(1).unwrap_or(&0)]));
    }
    while sum > 0xFFFF {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !u16::try_from(sum).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a tool's contract, not game behavior)
    use super::*;
    use dereth_pcap::link::Link;
    use dereth_pcap::pcap::CaptureReader;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    /// What is written reads back through the capture index's own reader and link layer as the
    /// same datagrams, between the same addresses, at the same times.
    #[test]
    fn a_written_capture_reads_back_as_the_datagrams_written() {
        let client = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 50_123);
        let server = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 9000);
        let t0 = UNIX_EPOCH + Duration::from_micros(1_700_000_000_250_000);
        let mut w = CaptureWriter::new(Vec::new()).expect("a header");
        w.datagram(t0, client, server, b"to the server")
            .expect("written");
        w.datagram(t0 + Duration::from_millis(5), server, client, &[0u8; 1200])
            .expect("written");
        let bytes = w.into_inner();

        let mut link = Link::new();
        let mut got = Vec::new();
        for rec in CaptureReader::new(&bytes[..]).expect("a capture") {
            let rec = rec.expect("a record");
            got.push(
                link.frame(rec.link_type, rec.ts, &rec.data)
                    .expect("a datagram"),
            );
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].src, (*client.ip(), client.port()));
        assert_eq!(got[0].dst, (*server.ip(), server.port()));
        assert_eq!(got[0].payload, b"to the server");
        assert_eq!(got[1].src, (*server.ip(), server.port()));
        assert_eq!(got[1].payload, vec![0u8; 1200]);
        assert!((got[0].ts - 1_700_000_000.25).abs() < 1e-6);
        assert!((got[1].ts - got[0].ts - 0.005).abs() < 1e-6);
    }

    /// The IPv4 header's checksum is the one a receiver verifies: the whole header sums to all
    /// ones.
    #[test]
    fn the_ip_header_checksum_verifies() {
        let mut w = CaptureWriter::new(Vec::new()).expect("a header");
        let a = SocketAddrV4::new(Ipv4Addr::new(10, 1, 2, 3), 1234);
        let b = SocketAddrV4::new(Ipv4Addr::new(192, 168, 7, 9), 9001);
        w.datagram(UNIX_EPOCH, a, b, b"x").expect("written");
        let bytes = w.into_inner();
        let header = &bytes[24 + 16..24 + 16 + 20];
        let mut sum = 0u32;
        for w in header.chunks(2) {
            sum += u32::from(u16::from_be_bytes([w[0], w[1]]));
        }
        while sum > 0xFFFF {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        assert_eq!(sum, 0xFFFF);
    }
}
