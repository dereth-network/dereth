//! From a captured frame to a UDP datagram: the link layer (Ethernet with or without VLAN tags,
//! Linux cooked v1 and v2, BSD loopback, raw IP), IPv4, fragment reassembly and UDP.
//!
//! Only IPv4/UDP is game traffic; everything else is counted in [`LinkStats`] and dropped.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

/// One UDP datagram, whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Datagram {
    pub ts: f64,
    pub src: (Ipv4Addr, u16),
    pub dst: (Ipv4Addr, u16),
    pub payload: Vec<u8>,
}

/// What the link layer dropped or repaired, per capture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkStats {
    /// Frames whose link type this reader does not handle.
    pub unknown_link: u64,
    /// Frames that are not IPv4 (ARP, IPv6, ...).
    pub not_ipv4: u64,
    /// IPv4 packets that are not UDP.
    pub not_udp: u64,
    /// Frames cut short by the snap length or malformed, so the datagram is incomplete.
    pub truncated: u64,
    /// IPv4 fragments seen.
    pub ip_fragments: u64,
    /// Datagrams rebuilt from fragments.
    pub ip_reassembled: u64,
    /// Fragment trains that never completed (at the end of the capture, or evicted).
    pub ip_unreassembled: u64,
    /// IPv4 headers whose total length is zero, read to the end of the frame instead. Captures
    /// written by a send/receive hook rather than a network tap build the IP header themselves
    /// and leave the length unset; the UDP length is still right.
    pub ip_len_zero: u64,
}

/// The fragments of one IPv4 datagram still being put together.
#[derive(Debug, Default)]
struct Train {
    pieces: BTreeMap<usize, Vec<u8>>,
    total: Option<usize>,
    first_ts: f64,
    header: Vec<u8>,
}

/// How many fragment trains may be open at once before the oldest is given up.
const MAX_TRAINS: usize = 256;

/// The stateful half of the link layer: the IPv4 fragment trains.
#[derive(Debug, Default)]
pub struct Link {
    trains: BTreeMap<(u32, u32, u16), Train>,
    pub stats: LinkStats,
}

impl Link {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one frame; returns the UDP datagram it completes, if any.
    pub fn frame(&mut self, link_type: u32, ts: f64, data: &[u8]) -> Option<Datagram> {
        let ip = ipv4_of(link_type, data, &mut self.stats)?;
        self.ipv4(ts, ip)
    }

    fn ipv4(&mut self, ts: f64, ip: &[u8]) -> Option<Datagram> {
        if ip.len() < 20 || ip[0] >> 4 != 4 {
            self.stats.not_ipv4 += 1;
            return None;
        }
        let ihl = usize::from(ip[0] & 0x0F) * 4;
        let mut total = usize::from(u16::from_be_bytes([ip[2], ip[3]]));
        if total == 0 {
            self.stats.ip_len_zero += 1;
            total = ip.len();
        }
        if ihl < 20 || total < ihl {
            self.stats.truncated += 1;
            return None;
        }
        if ip.len() < total {
            self.stats.truncated += 1;
            return None;
        }
        // Ethernet pads short frames; the IP total length is the truth.
        let ip = &ip[..total];
        if ip[9] != 17 {
            self.stats.not_udp += 1;
            return None;
        }
        let src = Ipv4Addr::new(ip[12], ip[13], ip[14], ip[15]);
        let dst = Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]);
        let frag = u16::from_be_bytes([ip[6], ip[7]]);
        let more = frag & 0x2000 != 0;
        let offset = usize::from(frag & 0x1FFF) * 8;
        if !more && offset == 0 {
            return self.udp(ts, src, dst, &ip[ihl..]);
        }
        self.stats.ip_fragments += 1;
        let id = u16::from_be_bytes([ip[4], ip[5]]);
        let key = (u32::from(src), u32::from(dst), id);
        if !self.trains.contains_key(&key) && self.trains.len() >= MAX_TRAINS {
            // Give up the oldest train.
            let oldest = self
                .trains
                .iter()
                .min_by(|a, b| a.1.first_ts.total_cmp(&b.1.first_ts))
                .map(|(k, _)| *k);
            if let Some(k) = oldest {
                self.trains.remove(&k);
                self.stats.ip_unreassembled += 1;
            }
        }
        let train = self.trains.entry(key).or_insert_with(|| Train {
            first_ts: ts,
            ..Train::default()
        });
        if offset == 0 {
            train.header = ip[..ihl].to_vec();
        }
        let piece = ip[ihl..].to_vec();
        if !more {
            train.total = Some(offset + piece.len());
        }
        train.pieces.insert(offset, piece);
        let total = train.total?;
        let mut have = 0usize;
        for (off, p) in &train.pieces {
            if *off > have {
                return None;
            }
            have = have.max(off + p.len());
        }
        if have < total {
            return None;
        }
        let train = self.trains.remove(&key)?;
        let mut whole = vec![0u8; total];
        for (off, p) in &train.pieces {
            let end = (off + p.len()).min(total);
            whole[*off..end].copy_from_slice(&p[..end - off]);
        }
        self.stats.ip_reassembled += 1;
        self.udp(ts, src, dst, &whole)
    }

    fn udp(&mut self, ts: f64, src: Ipv4Addr, dst: Ipv4Addr, u: &[u8]) -> Option<Datagram> {
        if u.len() < 8 {
            self.stats.truncated += 1;
            return None;
        }
        let sport = u16::from_be_bytes([u[0], u[1]]);
        let dport = u16::from_be_bytes([u[2], u[3]]);
        let len = usize::from(u16::from_be_bytes([u[4], u[5]]));
        if len < 8 || len > u.len() {
            self.stats.truncated += 1;
            return None;
        }
        Some(Datagram {
            ts,
            src: (src, sport),
            dst: (dst, dport),
            payload: u[8..len].to_vec(),
        })
    }

    /// Count the fragment trains still open when the capture ends.
    pub fn finish(&mut self) {
        self.stats.ip_unreassembled += self.trains.len() as u64;
        self.trains.clear();
    }
}

/// The IPv4 packet inside a frame of the given link type.
fn ipv4_of<'a>(link_type: u32, data: &'a [u8], stats: &mut LinkStats) -> Option<&'a [u8]> {
    match link_type {
        // Ethernet, with any number of 802.1Q / 802.1ad tags.
        1 => {
            let mut o = 12;
            loop {
                let Some(t) = data.get(o..o + 2) else {
                    stats.truncated += 1;
                    return None;
                };
                let ethertype = u16::from_be_bytes([t[0], t[1]]);
                match ethertype {
                    0x8100 | 0x88A8 | 0x9100 => o += 4,
                    0x0800 => return data.get(o + 2..),
                    _ => {
                        stats.not_ipv4 += 1;
                        return None;
                    }
                }
            }
        }
        // BSD loopback: a four-byte address family in host order (2 is IPv4 everywhere).
        0 | 109 => {
            let fam = data.get(..4)?;
            if u32::from_le_bytes([fam[0], fam[1], fam[2], fam[3]]) == 2
                || u32::from_be_bytes([fam[0], fam[1], fam[2], fam[3]]) == 2
            {
                data.get(4..)
            } else {
                stats.not_ipv4 += 1;
                None
            }
        }
        // Raw IP (several link-type numbers mean it), and raw IPv4.
        101 | 12 | 14 | 228 => Some(data),
        // Linux cooked capture v1: 16-byte header, protocol at 14.
        113 => {
            let p = data.get(14..16)?;
            if u16::from_be_bytes([p[0], p[1]]) == 0x0800 {
                data.get(16..)
            } else {
                stats.not_ipv4 += 1;
                None
            }
        }
        // Linux cooked capture v2: 20-byte header, protocol first.
        276 => {
            let p = data.get(0..2)?;
            if u16::from_be_bytes([p[0], p[1]]) == 0x0800 {
                data.get(20..)
            } else {
                stats.not_ipv4 += 1;
                None
            }
        }
        _ => {
            stats.unknown_link += 1;
            None
        }
    }
}

/// Builders for frames, for tests.
pub mod build {
    use std::net::Ipv4Addr;

    /// An IPv4 header plus `payload`, with the given id and fragment field.
    #[must_use]
    pub fn ipv4(
        src: Ipv4Addr,
        dst: Ipv4Addr,
        id: u16,
        frag: u16,
        proto: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let total = u16::try_from(20 + payload.len()).unwrap_or(u16::MAX);
        let mut ip = vec![0x45, 0];
        ip.extend_from_slice(&total.to_be_bytes());
        ip.extend_from_slice(&id.to_be_bytes());
        ip.extend_from_slice(&frag.to_be_bytes());
        ip.extend_from_slice(&[64, proto, 0, 0]);
        ip.extend_from_slice(&src.octets());
        ip.extend_from_slice(&dst.octets());
        ip.extend_from_slice(payload);
        ip
    }

    /// A UDP header plus `payload`.
    #[must_use]
    pub fn udp(sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(8 + payload.len()).unwrap_or(u16::MAX);
        let mut u = Vec::new();
        u.extend_from_slice(&sport.to_be_bytes());
        u.extend_from_slice(&dport.to_be_bytes());
        u.extend_from_slice(&len.to_be_bytes());
        u.extend_from_slice(&[0, 0]);
        u.extend_from_slice(payload);
        u
    }

    /// An Ethernet frame carrying an IPv4/UDP datagram, padded to the 60-byte minimum.
    #[must_use]
    pub fn ethernet_udp(src: (Ipv4Addr, u16), dst: (Ipv4Addr, u16), payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 12];
        f.extend_from_slice(&[0x08, 0x00]);
        f.extend(ipv4(src.0, dst.0, 1, 0, 17, &udp(src.1, dst.1, payload)));
        if f.len() < 60 {
            f.resize(60, 0);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    const A: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
    const B: Ipv4Addr = Ipv4Addr::new(198, 51, 100, 7);

    #[test]
    fn ethernet_padding_is_trimmed_by_the_ip_length() {
        let f = ethernet_udp((A, 50000), (B, 9000), b"hi");
        assert_eq!(f.len(), 60);
        let mut l = Link::new();
        let d = l.frame(1, 1.0, &f).expect("a datagram");
        assert_eq!(d.payload, b"hi");
        assert_eq!(d.src, (A, 50000));
        assert_eq!(d.dst, (B, 9000));
    }

    #[test]
    fn vlan_cooked_loopback_and_raw_frames() {
        let ip = ipv4(A, B, 1, 0, 17, &udp(1, 9000, b"x"));
        let mut vlan = vec![0u8; 12];
        vlan.extend_from_slice(&[0x81, 0x00, 0, 5, 0x08, 0x00]);
        vlan.extend_from_slice(&ip);
        let mut sll = vec![0u8; 14];
        sll.extend_from_slice(&[0x08, 0x00]);
        sll.extend_from_slice(&ip);
        let mut sll2 = vec![0x08, 0x00];
        sll2.resize(20, 0);
        sll2.extend_from_slice(&ip);
        let mut null = 2u32.to_le_bytes().to_vec();
        null.extend_from_slice(&ip);
        let mut l = Link::new();
        for (lt, f) in [
            (1, &vlan),
            (113, &sll),
            (276, &sll2),
            (0, &null),
            (101, &ip),
            (228, &ip),
        ] {
            let d = l
                .frame(lt, 0.0, f)
                .unwrap_or_else(|| panic!("link type {lt}"));
            assert_eq!(d.payload, b"x");
        }
        assert_eq!(l.stats, LinkStats::default());
    }

    #[test]
    fn non_udp_and_non_ip_are_counted() {
        let mut l = Link::new();
        let mut arp = vec![0u8; 12];
        arp.extend_from_slice(&[0x08, 0x06, 0, 0]);
        assert!(l.frame(1, 0.0, &arp).is_none());
        assert!(l.frame(101, 0.0, &ipv4(A, B, 1, 0, 6, &[0; 20])).is_none());
        assert!(l.frame(147, 0.0, &[0; 40]).is_none());
        assert_eq!(
            (l.stats.not_ipv4, l.stats.not_udp, l.stats.unknown_link),
            (1, 1, 1)
        );
    }

    #[test]
    fn ip_fragments_are_reassembled_out_of_order() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
        let u = udp(50000, 9001, &payload);
        let (a, rest) = u.split_at(1480);
        let (b, c) = rest.split_at(1480);
        let f1 = ipv4(A, B, 77, 0x2000, 17, a);
        let f2 = ipv4(A, B, 77, 0x2000 | (1480 / 8), 17, b);
        let f3 = ipv4(A, B, 77, 2960 / 8, 17, c);
        let mut l = Link::new();
        assert!(l.frame(101, 0.0, &f3).is_none());
        assert!(l.frame(101, 0.0, &f1).is_none());
        let d = l.frame(101, 0.0, &f2).expect("complete");
        assert_eq!(d.payload, payload);
        assert_eq!(l.stats.ip_fragments, 3);
        assert_eq!(l.stats.ip_reassembled, 1);
        // An unfinished train is counted when the capture ends.
        assert!(l.frame(101, 0.0, &ipv4(A, B, 78, 0x2000, 17, a)).is_none());
        l.finish();
        assert_eq!(l.stats.ip_unreassembled, 1);
    }

    #[test]
    fn a_zero_ip_length_is_read_to_the_end_of_the_frame() {
        // A hooked-send capture: IP total length 0, UDP length right, Ethernet framing.
        let mut f = ethernet_udp((A, 12345), (B, 9001), b"payload!");
        f.truncate(14 + 20 + 8 + 8);
        f[16] = 0;
        f[17] = 0;
        let mut l = Link::new();
        let d = l.frame(1, 0.0, &f).expect("a datagram");
        assert_eq!(d.payload, b"payload!");
        assert_eq!(l.stats.ip_len_zero, 1);
    }

    #[test]
    fn a_snapped_frame_is_counted_not_misread() {
        let f = ethernet_udp((A, 50000), (B, 9000), &[7u8; 100]);
        let mut l = Link::new();
        assert!(l.frame(1, 0.0, &f[..80]).is_none());
        assert_eq!(l.stats.truncated, 1);
    }
}
