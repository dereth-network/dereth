//! Archives, read in memory: zip (stored and deflated members, zip64 sizes, nested archives) and
//! 7z (through `sevenz-rust2`, LZMA and LZMA2). rar is recognised so it can be skipped and
//! counted, never read.
//!
//! An archive's members come back as bytes; the caller decides what each one is by its first
//! bytes, not by its name.

use std::io::{Cursor, Read};

/// What a file's first bytes say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pcap,
    PcapNg,
    Zip,
    SevenZ,
    Rar,
    Empty,
    Other,
}

impl Kind {
    /// The name the index records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pcap => "pcap",
            Self::PcapNg => "pcapng",
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::Rar => "rar",
            Self::Empty => "empty",
            Self::Other => "other",
        }
    }

    /// Whether this is an archive this module opens.
    #[must_use]
    pub const fn is_archive(self) -> bool {
        matches!(self, Self::Zip | Self::SevenZ)
    }
}

/// Classify by content.
#[must_use]
pub fn sniff(head: &[u8]) -> Kind {
    if head.is_empty() {
        return Kind::Empty;
    }
    match crate::pcap::sniff(head) {
        Some(crate::pcap::Format::Pcap) => return Kind::Pcap,
        Some(crate::pcap::Format::PcapNg) => return Kind::PcapNg,
        None => {}
    }
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Kind::Zip
    } else if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        Kind::SevenZ
    } else if head.starts_with(b"Rar!\x1A\x07") {
        Kind::Rar
    } else {
        Kind::Other
    }
}

/// One archive member.
#[derive(Debug)]
pub struct Member {
    /// The member's path inside the archive, with forward slashes.
    pub name: String,
    /// Its bytes, or why they could not be read (encrypted, unsupported method, bad CRC).
    pub data: Result<Vec<u8>, String>,
}

/// Every file member of an archive (directories are left out).
///
/// # Errors
/// When the archive's own directory cannot be read; a single bad member is reported in its
/// [`Member::data`] instead.
pub fn members(kind: Kind, bytes: &[u8]) -> Result<Vec<Member>, String> {
    match kind {
        Kind::Zip => zip_members(bytes),
        Kind::SevenZ => sevenz_members(bytes),
        _ => Err(format!(
            "{} is not an archive this reader opens",
            kind.as_str()
        )),
    }
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// Largest member this reader inflates.
const MAX_MEMBER: u64 = 4 * 1024 * 1024 * 1024;

fn zip_members(b: &[u8]) -> Result<Vec<Member>, String> {
    // The end-of-central-directory record is within the last 64 KiB + 22 bytes.
    let lo = b.len().saturating_sub(65_557);
    let eocd = (lo..b.len().saturating_sub(21))
        .rev()
        .find(|&i| b[i..].starts_with(b"PK\x05\x06"))
        .ok_or("zip: no end-of-central-directory record")?;
    let mut count = u64::from(u16_at(b, eocd + 10).ok_or("zip: short EOCD")?);
    let mut cd_off = u64::from(u32_at(b, eocd + 16).ok_or("zip: short EOCD")?);
    // zip64: the locator sits just before the EOCD.
    if eocd >= 20 && b[eocd - 20..].starts_with(b"PK\x06\x07") {
        let z64 = u64_at(b, eocd - 20 + 8).ok_or("zip64: short locator")?;
        let z64 = usize::try_from(z64).map_err(|_| "zip64: offset")?;
        if b.get(z64..).is_some_and(|s| s.starts_with(b"PK\x06\x06")) {
            count = u64_at(b, z64 + 32).ok_or("zip64: short EOCD")?;
            cd_off = u64_at(b, z64 + 48).ok_or("zip64: short EOCD")?;
        }
    }
    let mut o = usize::try_from(cd_off).map_err(|_| "zip: offset")?;
    let mut out = Vec::new();
    for _ in 0..count {
        if !b.get(o..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
            return Err(format!(
                "zip: central directory entry at {o} has no signature"
            ));
        }
        let flags = u16_at(b, o + 8).ok_or("zip: short entry")?;
        let method = u16_at(b, o + 10).ok_or("zip: short entry")?;
        let crc = u32_at(b, o + 16).ok_or("zip: short entry")?;
        let mut csize = u64::from(u32_at(b, o + 20).ok_or("zip: short entry")?);
        let mut usize_ = u64::from(u32_at(b, o + 24).ok_or("zip: short entry")?);
        let name_len = usize::from(u16_at(b, o + 28).ok_or("zip: short entry")?);
        let extra_len = usize::from(u16_at(b, o + 30).ok_or("zip: short entry")?);
        let comment_len = usize::from(u16_at(b, o + 32).ok_or("zip: short entry")?);
        let mut local = u64::from(u32_at(b, o + 42).ok_or("zip: short entry")?);
        let name_bytes = b.get(o + 46..o + 46 + name_len).ok_or("zip: short name")?;
        let name = String::from_utf8_lossy(name_bytes).replace('\\', "/");
        let extra = b
            .get(o + 46 + name_len..o + 46 + name_len + extra_len)
            .unwrap_or(&[]);
        // zip64 extra field: the sizes and offset that overflowed, in this order.
        let mut e = 0;
        while e + 4 <= extra.len() {
            let id = u16_at(extra, e).unwrap_or(0);
            let len = usize::from(u16_at(extra, e + 2).unwrap_or(0));
            if id == 1 {
                let mut p = e + 4;
                for field in [&mut usize_, &mut csize, &mut local] {
                    if *field == 0xFFFF_FFFF {
                        if let Some(v) = u64_at(extra, p) {
                            *field = v;
                        }
                        p += 8;
                    }
                }
            }
            e += 4 + len;
        }
        o += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        let data = zip_member_data(b, flags, method, crc, csize, usize_, local);
        out.push(Member { name, data });
    }
    Ok(out)
}

fn zip_member_data(
    b: &[u8],
    flags: u16,
    method: u16,
    crc: u32,
    csize: u64,
    usize_: u64,
    local: u64,
) -> Result<Vec<u8>, String> {
    if flags & 1 != 0 {
        return Err("encrypted".into());
    }
    if usize_ > MAX_MEMBER {
        return Err(format!(
            "{usize_} bytes is larger than this reader inflates"
        ));
    }
    let l = usize::try_from(local).map_err(|_| "offset")?;
    if !b.get(l..).is_some_and(|s| s.starts_with(b"PK\x03\x04")) {
        return Err("local header has no signature".into());
    }
    let n = usize::from(u16_at(b, l + 26).ok_or("short local header")?);
    let x = usize::from(u16_at(b, l + 28).ok_or("short local header")?);
    let start = l + 30 + n + x;
    let end = start + usize::try_from(csize).map_err(|_| "size")?;
    let raw = b
        .get(start..end)
        .ok_or("member runs past the end of the archive")?;
    let data = match method {
        0 => raw.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(usize::try_from(usize_).unwrap_or(0));
            flate2::read::DeflateDecoder::new(raw)
                .read_to_end(&mut out)
                .map_err(|e| format!("inflate: {e}"))?;
            out
        }
        m => return Err(format!("compression method {m} is not read")),
    };
    if crc32fast::hash(&data) != crc {
        return Err("CRC mismatch".into());
    }
    Ok(data)
}

fn sevenz_members(b: &[u8]) -> Result<Vec<Member>, String> {
    let mut reader =
        sevenz_rust2::ArchiveReader::new(Cursor::new(b), sevenz_rust2::Password::empty())
            .map_err(|e| format!("7z: {e}"))?;
    let mut out = Vec::new();
    reader
        .for_each_entries(|entry, r| {
            if entry.is_directory {
                return Ok(true);
            }
            let mut data = Vec::new();
            let got = r
                .read_to_end(&mut data)
                .map(|_| data)
                .map_err(|e| format!("7z: {e}"));
            out.push(Member {
                name: entry.name.replace('\\', "/"),
                data: got,
            });
            Ok(true)
        })
        .map_err(|e| format!("7z: {e}"))?;
    Ok(out)
}

/// A zip written in memory, for tests: `(name, bytes, deflate?)`.
pub mod build {
    use std::io::Write;

    #[must_use]
    pub fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data, deflate) in entries {
            let comp = if *deflate {
                let mut e =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(data).expect("in memory");
                e.finish().expect("in memory")
            } else {
                data.to_vec()
            };
            let crc = crc32fast::hash(data);
            let off = u32::try_from(out.len()).unwrap_or(u32::MAX);
            let method: u16 = if *deflate { 8 } else { 0 };
            let nlen = u16::try_from(name.len()).unwrap_or(0);
            let csz = u32::try_from(comp.len()).unwrap_or(0);
            let usz = u32::try_from(data.len()).unwrap_or(0);
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&csz.to_le_bytes());
            out.extend_from_slice(&usz.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&comp);
            cd.extend_from_slice(b"PK\x01\x02");
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0; 4]);
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&csz.to_le_bytes());
            cd.extend_from_slice(&usz.to_le_bytes());
            cd.extend_from_slice(&nlen.to_le_bytes());
            cd.extend_from_slice(&[0; 12]);
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = u32::try_from(out.len()).unwrap_or(u32::MAX);
        let n = u16::try_from(entries.len()).unwrap_or(0);
        let cd_len = u32::try_from(cd.len()).unwrap_or(0);
        out.extend_from_slice(&cd);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&cd_len.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_stored_and_deflated_members() {
        let pcap = crate::pcap::build::pcap(1, false, false, &[(1.0, b"frame")]);
        let z = build::zip(&[
            ("a/notes.csv", b"x,y\n1,2\n", true),
            ("a/cap.pcap", &pcap, false),
            ("b/c.pcap", &pcap, true),
        ]);
        assert_eq!(sniff(&z), Kind::Zip);
        let m = members(Kind::Zip, &z).expect("reads");
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].name, "a/notes.csv");
        assert_eq!(m[0].data.as_deref().unwrap(), b"x,y\n1,2\n");
        assert_eq!(sniff(m[1].data.as_ref().unwrap()), Kind::Pcap);
        assert_eq!(m[2].data.as_deref().unwrap(), &pcap[..]);
    }

    #[test]
    fn a_corrupt_member_is_reported_without_failing_the_archive() {
        let mut z = build::zip(&[
            ("x.bin", b"hello hello hello", false),
            ("y.bin", b"ok", false),
        ]);
        // Flip a byte of the first member's data: its CRC no longer matches.
        let at = 30 + "x.bin".len();
        z[at] ^= 0xFF;
        let m = members(Kind::Zip, &z).expect("reads");
        assert_eq!(m[0].data.as_ref().unwrap_err(), "CRC mismatch");
        assert_eq!(m[1].data.as_deref().unwrap(), b"ok");
    }

    #[test]
    fn nested_zip_is_a_member_to_open_again() {
        let inner = build::zip(&[("in.pcap", b"\xd4\xc3\xb2\xa1rest", false)]);
        let outer = build::zip(&[("inner.zip", &inner, true)]);
        let m = members(Kind::Zip, &outer).unwrap();
        let bytes = m[0].data.as_ref().unwrap();
        assert_eq!(sniff(bytes), Kind::Zip);
        let m2 = members(Kind::Zip, bytes).unwrap();
        assert_eq!(sniff(m2[0].data.as_ref().unwrap()), Kind::Pcap);
    }

    #[test]
    fn content_decides_the_kind() {
        assert_eq!(sniff(b""), Kind::Empty);
        assert_eq!(sniff(b"Rar!\x1A\x07\x00"), Kind::Rar);
        assert_eq!(
            sniff(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4]),
            Kind::SevenZ
        );
        assert_eq!(sniff(b"\n\r\r\n...."), Kind::PcapNg);
        assert_eq!(sniff(b"hello"), Kind::Other);
    }

    #[test]
    fn seven_zip_members_are_read_in_memory() {
        let pcap = crate::pcap::build::pcap(1, false, false, &[(1.0, b"frame")]);
        let mut w = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
        w.push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("d/cap.pcap"),
            Some(&pcap[..]),
        )
        .unwrap();
        w.push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("notes.txt"),
            Some(&b"hi"[..]),
        )
        .unwrap();
        let z = w.finish().unwrap().into_inner();
        assert_eq!(sniff(&z), Kind::SevenZ);
        let m = members(Kind::SevenZ, &z).expect("reads");
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "d/cap.pcap");
        assert_eq!(m[0].data.as_deref().unwrap(), &pcap[..]);
        assert_eq!(m[1].data.as_deref().unwrap(), b"hi");
    }

    #[test]
    fn a_seven_zip_that_is_not_one_is_an_error_not_a_panic() {
        let junk = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4, 1, 2, 3];
        assert!(members(Kind::SevenZ, &junk).is_err());
    }
}
