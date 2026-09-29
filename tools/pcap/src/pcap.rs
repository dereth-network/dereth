//! The two capture containers: classic pcap (either byte order, microsecond or nanosecond stamps)
//! and pcapng (section header, interface description, enhanced, simple and the obsolete packet
//! block; any number of interfaces and sections).
//!
//! Both are read as a stream of [`Record`]s from any [`Read`], so a capture is never held whole:
//! a file on disk is read through a buffer, and an archive member is read from the bytes the
//! archive reader produced.
//!
//! A capture that stops in the middle of a record is the common way a capture ends when the
//! recorder was killed; the partial record is dropped and counted in [`CaptureReader::truncated_tail`]
//! rather than failing the file.

use std::io::{self, Read};

/// The container format, from the first four bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Classic libpcap.
    Pcap,
    /// pcapng.
    PcapNg,
}

impl Format {
    /// The name the index records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pcap => "pcap",
            Self::PcapNg => "pcapng",
        }
    }
}

const PCAP_MICRO: u32 = 0xA1B2_C3D4;
const PCAP_NANO: u32 = 0xA1B2_3C4D;
const NG_SHB: u32 = 0x0A0D_0D0A;
const NG_BYTE_ORDER: u32 = 0x1A2B_3C4D;

/// The longest record accepted; anything longer is corruption, not a datagram.
const MAX_RECORD: usize = 16 * 1024 * 1024;

/// Which container a buffer starts with, if either.
#[must_use]
pub fn sniff(head: &[u8]) -> Option<Format> {
    let b: [u8; 4] = head.get(..4)?.try_into().ok()?;
    let le = u32::from_le_bytes(b);
    let be = u32::from_be_bytes(b);
    if [PCAP_MICRO, PCAP_NANO].contains(&le) || [PCAP_MICRO, PCAP_NANO].contains(&be) {
        return Some(Format::Pcap);
    }
    if le == NG_SHB {
        return Some(Format::PcapNg);
    }
    None
}

/// One captured frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Seconds since the Unix epoch.
    pub ts: f64,
    /// The frame's link-layer type (the pcap `LINKTYPE_*` value).
    pub link_type: u32,
    /// The bytes captured, which may be fewer than [`Record::orig_len`] under a snap length.
    pub data: Vec<u8>,
    /// The frame's length on the wire.
    pub orig_len: u32,
}

/// Why a capture could not be read.
#[derive(Debug)]
pub enum CaptureError {
    Io(io::Error),
    /// The bytes are not a capture this reader understands, or a block is malformed.
    Format(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Format(s) => write!(f, "format: {s}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<io::Error> for CaptureError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn bad(s: impl Into<String>) -> CaptureError {
    CaptureError::Format(s.into())
}

/// Fill `buf` completely, or report how many bytes there were (0 at a clean end).
fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

#[derive(Debug, Clone, Copy)]
struct Endian(bool);

impl Endian {
    fn u16(self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        if self.0 {
            u16::from_be_bytes(a)
        } else {
            u16::from_le_bytes(a)
        }
    }
    fn u32(self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        if self.0 {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        }
    }
    fn i64(self, b: &[u8]) -> i64 {
        let a: [u8; 8] = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
        if self.0 {
            i64::from_be_bytes(a)
        } else {
            i64::from_le_bytes(a)
        }
    }
}

/// One pcapng interface: its link type, snap length and clock.
#[derive(Debug, Clone, Copy)]
struct Interface {
    link_type: u32,
    snaplen: u32,
    /// Timestamp units per second.
    units: f64,
    /// Seconds added to every stamp.
    offset: i64,
}

#[derive(Debug)]
enum State {
    Pcap {
        endian: Endian,
        nano: bool,
        link_type: u32,
    },
    Ng {
        endian: Endian,
        interfaces: Vec<Interface>,
    },
}

/// A streaming reader over one capture.
#[derive(Debug)]
pub struct CaptureReader<R: Read> {
    r: R,
    state: State,
    format: Format,
    /// Set when the capture ended in the middle of a record.
    pub truncated_tail: bool,
    /// pcapng blocks of a type this reader does not use (name resolution, statistics, custom).
    pub skipped_blocks: u64,
    done: bool,
}

impl<R: Read> CaptureReader<R> {
    /// Open a capture: read its file or first section header.
    ///
    /// # Errors
    /// When the first bytes are neither container, or the header is short.
    pub fn new(mut r: R) -> Result<Self, CaptureError> {
        let mut magic = [0u8; 4];
        if read_full(&mut r, &mut magic)? < 4 {
            return Err(bad("shorter than a capture header"));
        }
        match sniff(&magic) {
            Some(Format::Pcap) => {
                let mut rest = [0u8; 20];
                if read_full(&mut r, &mut rest)? < 20 {
                    return Err(bad("short pcap header"));
                }
                let le = u32::from_le_bytes(magic);
                let endian = Endian(!(le == PCAP_MICRO || le == PCAP_NANO));
                let nano = endian.u32(&magic) == PCAP_NANO;
                let link_type = endian.u32(&rest[16..20]) & 0x0FFF_FFFF;
                Ok(Self {
                    r,
                    state: State::Pcap {
                        endian,
                        nano,
                        link_type,
                    },
                    format: Format::Pcap,
                    truncated_tail: false,
                    skipped_blocks: 0,
                    done: false,
                })
            }
            Some(Format::PcapNg) => {
                let mut me = Self {
                    r,
                    state: State::Ng {
                        endian: Endian(false),
                        interfaces: Vec::new(),
                    },
                    format: Format::PcapNg,
                    truncated_tail: false,
                    skipped_blocks: 0,
                    done: false,
                };
                me.section_header()?;
                Ok(me)
            }
            None => Err(bad("not a pcap or pcapng capture")),
        }
    }

    /// The container format.
    #[must_use]
    pub const fn format(&self) -> Format {
        self.format
    }

    /// The link type of the first interface (pcap has one; a pcapng file may have several).
    #[must_use]
    pub fn link_type(&self) -> Option<u32> {
        match &self.state {
            State::Pcap { link_type, .. } => Some(*link_type),
            State::Ng { interfaces, .. } => interfaces.first().map(|i| i.link_type),
        }
    }

    /// Read a section header block whose type dword has already been consumed.
    fn section_header(&mut self) -> Result<(), CaptureError> {
        let mut head = [0u8; 8];
        if read_full(&mut self.r, &mut head)? < 8 {
            return Err(bad("short section header"));
        }
        let bom_le = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        let endian = if bom_le == NG_BYTE_ORDER {
            Endian(false)
        } else if u32::from_be_bytes([head[4], head[5], head[6], head[7]]) == NG_BYTE_ORDER {
            Endian(true)
        } else {
            return Err(bad("section header has no byte-order magic"));
        };
        let total = endian.u32(&head[0..4]) as usize;
        if total < 28 || !total.is_multiple_of(4) || total > MAX_RECORD {
            return Err(bad(format!("section header length {total}")));
        }
        let mut rest = vec![0u8; total - 12];
        if read_full(&mut self.r, &mut rest)? < rest.len() {
            return Err(bad("short section header"));
        }
        self.state = State::Ng {
            endian,
            interfaces: Vec::new(),
        };
        Ok(())
    }

    fn next_pcap(
        &mut self,
        endian: Endian,
        nano: bool,
        link_type: u32,
    ) -> Result<Option<Record>, CaptureError> {
        let mut h = [0u8; 16];
        let n = read_full(&mut self.r, &mut h)?;
        if n == 0 {
            return Ok(None);
        }
        if n < 16 {
            self.truncated_tail = true;
            return Ok(None);
        }
        let sec = endian.u32(&h[0..4]);
        let frac = endian.u32(&h[4..8]);
        let incl = endian.u32(&h[8..12]) as usize;
        let orig_len = endian.u32(&h[12..16]);
        if incl > MAX_RECORD {
            return Err(bad(format!("record length {incl}")));
        }
        let mut data = vec![0u8; incl];
        if read_full(&mut self.r, &mut data)? < incl {
            self.truncated_tail = true;
            return Ok(None);
        }
        let div = if nano { 1e9 } else { 1e6 };
        let ts = f64::from(sec) + f64::from(frac) / div;
        Ok(Some(Record {
            ts,
            link_type,
            data,
            orig_len,
        }))
    }

    fn next_ng(&mut self) -> Result<Option<Record>, CaptureError> {
        loop {
            let State::Ng { endian, .. } = self.state else {
                unreachable!()
            };
            let mut h = [0u8; 8];
            let n = read_full(&mut self.r, &mut h)?;
            if n == 0 {
                return Ok(None);
            }
            if n < 8 {
                self.truncated_tail = true;
                return Ok(None);
            }
            let block_type = endian.u32(&h[0..4]);
            if block_type == NG_SHB {
                // A new section: it may change the byte order and it resets the interfaces. The
                // block's length is read under the new section's byte order.
                let len_bytes = [h[4], h[5], h[6], h[7]];
                let mut chain = io::Cursor::new(len_bytes.to_vec()).chain(&mut self.r);
                let mut head = [0u8; 8];
                if read_full(&mut chain, &mut head)? < 8 {
                    self.truncated_tail = true;
                    return Ok(None);
                }
                let bom = [head[4], head[5], head[6], head[7]];
                let endian = if u32::from_le_bytes(bom) == NG_BYTE_ORDER {
                    Endian(false)
                } else if u32::from_be_bytes(bom) == NG_BYTE_ORDER {
                    Endian(true)
                } else {
                    return Err(bad("section header has no byte-order magic"));
                };
                let total = endian.u32(&head[0..4]) as usize;
                if total < 28 || !total.is_multiple_of(4) || total > MAX_RECORD {
                    return Err(bad(format!("section header length {total}")));
                }
                let mut rest = vec![0u8; total - 12];
                if read_full(&mut self.r, &mut rest)? < rest.len() {
                    self.truncated_tail = true;
                    return Ok(None);
                }
                self.state = State::Ng {
                    endian,
                    interfaces: Vec::new(),
                };
                continue;
            }
            let total = endian.u32(&h[4..8]) as usize;
            if total < 12 || !total.is_multiple_of(4) || total > MAX_RECORD {
                return Err(bad(format!("block type {block_type:#x} length {total}")));
            }
            let mut body = vec![0u8; total - 8];
            if read_full(&mut self.r, &mut body)? < body.len() {
                self.truncated_tail = true;
                return Ok(None);
            }
            let body = &body[..body.len() - 4];
            let State::Ng { interfaces, .. } = &mut self.state else {
                unreachable!()
            };
            match block_type {
                // Interface description.
                1 => {
                    if body.len() < 8 {
                        return Err(bad("short interface description"));
                    }
                    let mut iface = Interface {
                        link_type: u32::from(endian.u16(&body[0..2])),
                        snaplen: endian.u32(&body[4..8]),
                        units: 1e6,
                        offset: 0,
                    };
                    let mut o = 8;
                    while o + 4 <= body.len() {
                        let code = endian.u16(&body[o..o + 2]);
                        let len = endian.u16(&body[o + 2..o + 4]) as usize;
                        let v = body.get(o + 4..o + 4 + len).unwrap_or(&[]);
                        match code {
                            0 => break,
                            9 if !v.is_empty() => {
                                let r = v[0];
                                let e = i32::from(r & 0x7F);
                                iface.units = if r & 0x80 == 0 {
                                    10f64.powi(e)
                                } else {
                                    2f64.powi(e)
                                };
                            }
                            14 if v.len() >= 8 => iface.offset = endian.i64(v),
                            _ => {}
                        }
                        o += 4 + len.div_ceil(4) * 4;
                    }
                    interfaces.push(iface);
                }
                // Enhanced packet, and the obsolete packet block.
                6 | 2 => {
                    if body.len() < 20 {
                        return Err(bad("short packet block"));
                    }
                    let iface_id = if block_type == 6 {
                        endian.u32(&body[0..4]) as usize
                    } else {
                        usize::from(endian.u16(&body[0..2]))
                    };
                    let hi = u64::from(endian.u32(&body[4..8]));
                    let lo = u64::from(endian.u32(&body[8..12]));
                    let cap = endian.u32(&body[12..16]) as usize;
                    let orig_len = endian.u32(&body[16..20]);
                    let iface = *interfaces.get(iface_id).ok_or_else(|| {
                        bad(format!("packet names interface {iface_id}, not described"))
                    })?;
                    let data = body
                        .get(20..20 + cap)
                        .ok_or_else(|| bad("packet block shorter than its captured length"))?
                        .to_vec();
                    let ticks = (hi << 32) | lo;
                    let ts = ticks as f64 / iface.units + iface.offset as f64;
                    return Ok(Some(Record {
                        ts,
                        link_type: iface.link_type,
                        data,
                        orig_len,
                    }));
                }
                // Simple packet: interface 0, no timestamp.
                3 => {
                    if body.len() < 4 {
                        return Err(bad("short simple packet block"));
                    }
                    let iface = *interfaces
                        .first()
                        .ok_or_else(|| bad("simple packet before any interface"))?;
                    let orig_len = endian.u32(&body[0..4]);
                    let mut cap = (orig_len as usize).min(body.len() - 4);
                    if iface.snaplen != 0 {
                        cap = cap.min(iface.snaplen as usize);
                    }
                    let data = body[4..4 + cap].to_vec();
                    return Ok(Some(Record {
                        ts: 0.0,
                        link_type: iface.link_type,
                        data,
                        orig_len,
                    }));
                }
                _ => self.skipped_blocks += 1,
            }
        }
    }
}

impl<R: Read> Iterator for CaptureReader<R> {
    type Item = Result<Record, CaptureError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let r = match self.state {
            State::Pcap {
                endian,
                nano,
                link_type,
            } => self.next_pcap(endian, nano, link_type),
            State::Ng { .. } => self.next_ng(),
        };
        match r {
            Ok(Some(rec)) => Some(Ok(rec)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// Writers for captures built in memory, used by this crate's tests and by tests that want a
/// synthetic capture without a file.
pub mod build {
    /// A classic pcap: `big` selects the byte order and `nano` the stamp resolution.
    ///
    /// Stamps are small non-negative test values, so the float-to-integer casts cannot truncate.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn pcap(link_type: u32, big: bool, nano: bool, frames: &[(f64, &[u8])]) -> Vec<u8> {
        let w32 = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let w16 = |v: u16| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut out = Vec::new();
        out.extend_from_slice(&w32(if nano { 0xA1B2_3C4D } else { 0xA1B2_C3D4 }));
        out.extend_from_slice(&w16(2));
        out.extend_from_slice(&w16(4));
        out.extend_from_slice(&w32(0));
        out.extend_from_slice(&w32(0));
        out.extend_from_slice(&w32(65535));
        out.extend_from_slice(&w32(link_type));
        for (ts, data) in frames {
            let sec = ts.floor();
            let frac = (ts - sec) * if nano { 1e9 } else { 1e6 };
            out.extend_from_slice(&w32(u32::try_from(sec as u64).unwrap_or(0)));
            out.extend_from_slice(&w32(u32::try_from(frac.round() as u64).unwrap_or(0)));
            let n = u32::try_from(data.len()).unwrap_or(0);
            out.extend_from_slice(&w32(n));
            out.extend_from_slice(&w32(n));
            out.extend_from_slice(data);
        }
        out
    }

    /// One pcapng block.
    fn block(out: &mut Vec<u8>, big: bool, kind: u32, body: &[u8]) {
        let w32 = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let padded = body.len().div_ceil(4) * 4;
        let total = u32::try_from(12 + padded).unwrap_or(0);
        out.extend_from_slice(&w32(kind));
        out.extend_from_slice(&w32(total));
        out.extend_from_slice(body);
        out.resize(out.len() + padded - body.len(), 0);
        out.extend_from_slice(&w32(total));
    }

    /// A pcapng section: a section header, one interface description per entry of `ifaces`
    /// (`(link_type, tsresol option)`), then the frames as enhanced packet blocks
    /// `(interface, ticks, bytes)`, plus one simple packet block per entry of `simple`.
    #[must_use]
    pub fn pcapng_section(
        big: bool,
        ifaces: &[(u16, Option<u8>)],
        frames: &[(u32, u64, &[u8])],
        simple: &[&[u8]],
    ) -> Vec<u8> {
        let w32 = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let w16 = |v: u16| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut out = Vec::new();
        let mut shb = Vec::new();
        shb.extend_from_slice(&w32(0x1A2B_3C4D));
        shb.extend_from_slice(&w16(1));
        shb.extend_from_slice(&w16(0));
        shb.extend_from_slice(&(-1i64).to_le_bytes());
        block(&mut out, big, 0x0A0D_0D0A, &shb);
        for (lt, res) in ifaces {
            let mut idb = Vec::new();
            idb.extend_from_slice(&w16(*lt));
            idb.extend_from_slice(&w16(0));
            idb.extend_from_slice(&w32(0));
            if let Some(r) = res {
                idb.extend_from_slice(&w16(9));
                idb.extend_from_slice(&w16(1));
                idb.extend_from_slice(&[*r, 0, 0, 0]);
                idb.extend_from_slice(&w16(0));
                idb.extend_from_slice(&w16(0));
            }
            block(&mut out, big, 1, &idb);
        }
        for (iface, ticks, data) in frames {
            let mut epb = Vec::new();
            epb.extend_from_slice(&w32(*iface));
            epb.extend_from_slice(&w32(u32::try_from(ticks >> 32).unwrap_or(0)));
            epb.extend_from_slice(&w32(u32::try_from(ticks & 0xFFFF_FFFF).unwrap_or(0)));
            let n = u32::try_from(data.len()).unwrap_or(0);
            epb.extend_from_slice(&w32(n));
            epb.extend_from_slice(&w32(n));
            epb.extend_from_slice(data);
            block(&mut out, big, 6, &epb);
        }
        for data in simple {
            let mut spb = Vec::new();
            spb.extend_from_slice(&w32(u32::try_from(data.len()).unwrap_or(0)));
            spb.extend_from_slice(data);
            block(&mut out, big, 3, &spb);
        }
        // A name-resolution block, which the reader skips and counts.
        block(&mut out, big, 4, &[0, 0, 0, 0]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(bytes: &[u8]) -> (Vec<Record>, CaptureReader<&[u8]>) {
        let mut r = CaptureReader::new(bytes).expect("opens");
        let recs: Vec<Record> = r.by_ref().map(|x| x.expect("record")).collect();
        (recs, r)
    }

    #[test]
    fn pcap_in_both_byte_orders_and_both_resolutions() {
        for big in [false, true] {
            for nano in [false, true] {
                let bytes = build::pcap(1, big, nano, &[(10.5, b"abc"), (11.25, b"defg")]);
                assert_eq!(sniff(&bytes), Some(Format::Pcap));
                let (recs, r) = read_all(&bytes);
                assert_eq!(r.format(), Format::Pcap);
                assert_eq!(recs.len(), 2, "big={big} nano={nano}");
                assert_eq!(recs[0].data, b"abc");
                assert_eq!(recs[1].data, b"defg");
                assert!((recs[0].ts - 10.5).abs() < 1e-6);
                assert!((recs[1].ts - 11.25).abs() < 1e-6);
                assert_eq!(recs[0].link_type, 1);
                assert!(!r.truncated_tail);
            }
        }
    }

    #[test]
    fn a_pcap_cut_mid_record_keeps_what_came_before_and_says_so() {
        let mut bytes = build::pcap(1, false, false, &[(1.0, b"abcdef"), (2.0, b"ghijkl")]);
        bytes.truncate(bytes.len() - 3);
        let (recs, r) = read_all(&bytes);
        assert_eq!(recs.len(), 1);
        assert!(r.truncated_tail);
    }

    #[test]
    fn pcapng_with_two_interfaces_resolutions_and_a_simple_block() {
        for big in [false, true] {
            let bytes = build::pcapng_section(
                big,
                &[(1, None), (101, Some(9))],
                &[(0, 2_500_000, b"one"), (1, 3_000_000_000, b"two!")],
                &[b"simple"],
            );
            assert_eq!(sniff(&bytes), Some(Format::PcapNg));
            let (recs, r) = read_all(&bytes);
            assert_eq!(recs.len(), 3, "big={big}");
            assert_eq!(recs[0].link_type, 1);
            assert!((recs[0].ts - 2.5).abs() < 1e-9);
            assert_eq!(recs[1].link_type, 101);
            assert!((recs[1].ts - 3.0).abs() < 1e-9);
            assert_eq!(recs[1].data, b"two!");
            assert_eq!(recs[2].data, b"simple");
            assert_eq!(r.skipped_blocks, 1);
        }
    }

    #[test]
    fn pcapng_sections_may_change_byte_order() {
        let mut bytes = build::pcapng_section(false, &[(1, None)], &[(0, 1_000_000, b"le")], &[]);
        bytes.extend(build::pcapng_section(
            true,
            &[(101, None)],
            &[(0, 2_000_000, b"be")],
            &[],
        ));
        let (recs, _) = read_all(&bytes);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].data, b"le");
        assert_eq!(recs[1].data, b"be");
        assert_eq!(recs[1].link_type, 101);
    }

    #[test]
    fn a_packet_naming_an_undescribed_interface_is_an_error() {
        let bytes = build::pcapng_section(false, &[(1, None)], &[(3, 0, b"x")], &[]);
        let mut r = CaptureReader::new(&bytes[..]).unwrap();
        assert!(matches!(r.next(), Some(Err(CaptureError::Format(_)))));
    }

    #[test]
    fn not_a_capture() {
        assert_eq!(sniff(b"PK\x03\x04"), None);
        assert!(CaptureReader::new(&b"hello world, not a capture"[..]).is_err());
    }
}
