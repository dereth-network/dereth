//! The release archives, written so that the same files and the same commit give the same bytes:
//! entries in name order under one top-level folder, every time the commit's, fixed permissions
//! (0755 for the binaries, 0644 otherwise), no owner names, and a fixed compression level with no
//! timestamp of its own. And read back, for the deny scan of what was actually written.

use std::io::{Read, Write};
use std::path::Path;

use super::guard::{Entry, Kind, HEAD_LEN};

/// One file of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name below the top-level folder.
    pub name: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

/// Seconds since 1970 as (year, month, day, hour, minute, second), UTC.
pub fn civil(epoch: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let small = |v: i64| u32::try_from(v).unwrap_or(0);
    (
        year,
        small(month),
        small(day),
        small(secs / 3600),
        small(secs / 60 % 60),
        small(secs % 60),
    )
}

fn sorted(members: &[Member]) -> Vec<&Member> {
    let mut m: Vec<&Member> = members.iter().collect();
    m.sort_by(|a, b| a.name.cmp(&b.name));
    m
}

/// A `.zip` of `members` under `root/`.
pub fn zip_bytes(root: &str, members: &[Member], epoch: i64) -> Result<Vec<u8>, String> {
    use zip::write::SimpleFileOptions;
    let (y, mo, d, h, mi, s) = civil(epoch.max(315_532_800)); // a zip time starts in 1980
    let year = u16::try_from(y).map_err(|_| format!("year {y} does not fit a zip time"))?;
    let when = zip::DateTime::from_date_and_time(
        year,
        u8::try_from(mo).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
        u8::try_from(h).unwrap_or(0),
        u8::try_from(mi).unwrap_or(0),
        u8::try_from(s).unwrap_or(0),
    )
    .map_err(|e| format!("zip time: {e}"))?;
    let base = SimpleFileOptions::default()
        .last_modified_time(when)
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.add_directory(format!("{root}/"), base.unix_permissions(0o755))
        .map_err(|e| format!("zip: {e}"))?;
    for m in sorted(members) {
        let mode = if m.executable { 0o755 } else { 0o644 };
        w.start_file(format!("{root}/{}", m.name), base.unix_permissions(mode))
            .map_err(|e| format!("zip: {e}"))?;
        w.write_all(&m.bytes).map_err(|e| format!("zip: {e}"))?;
    }
    let out = w.finish().map_err(|e| format!("zip: {e}"))?;
    Ok(out.into_inner())
}

/// A ustar header block.
fn tar_header(
    name: &str,
    size: u64,
    mode: u32,
    epoch: i64,
    dir: bool,
) -> Result<[u8; 512], String> {
    let mut h = [0u8; 512];
    let (prefix, base) = if name.len() <= 100 {
        ("", name)
    } else {
        let cut = name[..name.len().min(156)]
            .rfind('/')
            .filter(|&i| name.len() - i - 1 <= 100 && i <= 155)
            .ok_or_else(|| format!("`{name}` is too long for a tar entry"))?;
        (&name[..cut], &name[cut + 1..])
    };
    h[..base.len()].copy_from_slice(base.as_bytes());
    let octal = |field: &mut [u8], v: u64| {
        let text = format!("{v:0width$o}", width = field.len() - 1);
        field[..text.len()].copy_from_slice(text.as_bytes());
    };
    octal(&mut h[100..108], u64::from(mode));
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], u64::try_from(epoch.max(0)).unwrap_or(0));
    h[156] = if dir { b'5' } else { b'0' };
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let text = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(text.as_bytes());
    Ok(h)
}

/// The uncompressed tar of `members` under `root/`.
pub fn tar_bytes(root: &str, members: &[Member], epoch: i64) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(&tar_header(&format!("{root}/"), 0, 0o755, epoch, true)?);
    for m in sorted(members) {
        let mode = if m.executable { 0o755 } else { 0o644 };
        let name = format!("{root}/{}", m.name);
        out.extend_from_slice(&tar_header(
            &name,
            m.bytes.len() as u64,
            mode,
            epoch,
            false,
        )?);
        out.extend_from_slice(&m.bytes);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    Ok(out)
}

/// A `.tar.gz` of `members` under `root/`: the gzip header carries no time and no name.
pub fn tar_gz_bytes(root: &str, members: &[Member], epoch: i64) -> Result<Vec<u8>, String> {
    let tar = tar_bytes(root, members, epoch)?;
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), flate2::Compression::new(9));
    gz.write_all(&tar).map_err(|e| format!("gzip: {e}"))?;
    gz.finish().map_err(|e| format!("gzip: {e}"))
}

/// The entries of a `.zip`, with paths as stored.
pub fn zip_entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| format!("zip: {e}"))?;
        let path = f.name().to_owned();
        let link = f.unix_mode().is_some_and(|m| m & 0o170_000 == 0o120_000);
        if link {
            out.push(Entry {
                path,
                kind: Kind::Link("a symbolic link".to_owned()),
                size: f.size(),
                head: Vec::new(),
            });
        } else if f.is_dir() {
            out.push(Entry {
                path,
                kind: Kind::Dir,
                size: 0,
                head: Vec::new(),
            });
        } else {
            let mut data = Vec::new();
            f.read_to_end(&mut data).map_err(|e| format!("zip: {e}"))?;
            out.push(Entry {
                path,
                kind: Kind::File,
                size: data.len() as u64,
                head: data[..data.len().min(HEAD_LEN)].to_vec(),
            });
        }
    }
    Ok(out)
}

fn octal_field(b: &[u8]) -> Result<u64, String> {
    let text: String = b
        .iter()
        .take_while(|&&c| c != 0 && c != b' ')
        .map(|&c| char::from(c))
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| format!("tar: bad number `{text}`"))
}

/// The entries of an uncompressed tar.
pub fn tar_entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 512 <= bytes.len() {
        let h = &bytes[at..at + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let field = |r: std::ops::Range<usize>| {
            let s = &h[r];
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            String::from_utf8_lossy(&s[..end]).into_owned()
        };
        let name = field(0..100);
        let prefix = field(345..500);
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let size = octal_field(&h[124..136])?;
        let len = usize::try_from(size).map_err(|_| "tar: entry too large".to_owned())?;
        let data_start = at + 512;
        let data = bytes
            .get(data_start..data_start + len)
            .ok_or("tar: entry runs past the end")?;
        let kind = match h[156] {
            b'0' | 0 => Kind::File,
            b'5' => Kind::Dir,
            b'1' => Kind::Link("a hard link".to_owned()),
            b'2' => Kind::Link("a symbolic link".to_owned()),
            other => Kind::Link(format!("a tar entry of type `{}`", char::from(other))),
        };
        out.push(Entry {
            path,
            head: if kind == Kind::File {
                data[..len.min(HEAD_LEN)].to_vec()
            } else {
                Vec::new()
            },
            kind,
            size,
        });
        at = data_start + len.div_ceil(512) * 512;
    }
    Ok(out)
}

/// The entries of a `.tar.gz`.
pub fn tar_gz_entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut tar)
        .map_err(|e| format!("gzip: {e}"))?;
    tar_entries(&tar)
}

/// The entries of the archive at `path`, by its extension, with the top-level folder `root`
/// taken off each path (the folder itself becomes the empty path). An entry outside `root/` keeps
/// its full path, prefixed with `../`, so no allowlist can match it.
pub fn archive_entries(path: &Path, root: &str) -> Result<Vec<Entry>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let raw = if name.ends_with(".zip") {
        zip_entries(&bytes)?
    } else if name.ends_with(".tar.gz") {
        tar_gz_entries(&bytes)?
    } else {
        return Err(format!("{name}: neither a .zip nor a .tar.gz"));
    };
    Ok(relative_to_root(raw, root))
}

/// Paths below `root/`; anything else is marked as outside it.
pub fn relative_to_root(entries: Vec<Entry>, root: &str) -> Vec<Entry> {
    let prefix = format!("{root}/");
    entries
        .into_iter()
        .map(|mut e| {
            if e.path == prefix || e.path == root {
                e.path = String::new();
            } else if let Some(rest) = e.path.strip_prefix(&prefix) {
                e.path = rest.trim_end_matches('/').to_owned();
            } else {
                e.path = format!("../{}", e.path);
            }
            e
        })
        .collect()
}
