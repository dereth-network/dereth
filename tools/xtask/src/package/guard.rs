//! The deny scan: nothing but the allowlist goes into a release, and nothing that is or could be
//! the game's data, a server's state or a private configuration goes in whatever its name.
//!
//! It runs twice, over the staging folder and again over the finished archive's entries, so a
//! fault in staging or in the archive writer cannot carry a file past it. Any finding fails the
//! package.

use std::path::Path;

/// No single file in a release is larger than this. The server binaries are tens of megabytes;
/// the smallest retail data file is hundreds.
pub const FILE_CAP: u64 = 64 << 20;

/// No archive holds more than this in all.
pub const ARCHIVE_CAP: u64 = 256 << 20;

/// Where the game's data container keeps its header, and the magic it starts with.
const DAT_HEADER_OFFSET: usize = 0x140;
const DAT_MAGIC: u32 = 0x5442;

/// How many leading bytes of each file the scan keeps: enough for the container header.
pub const HEAD_LEN: usize = 0x200;

/// What one scanned entry is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    /// A symbolic or hard link, or any other entry that is not a plain file or folder, described.
    Link(String),
}

/// One file, folder or link found in the staging folder or an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path below the package's top-level folder, `/`-separated; empty for that folder.
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    /// The first [`HEAD_LEN`] bytes of a file.
    pub head: Vec<u8>,
}

#[cfg(test)]
impl Entry {
    /// A plain file with these contents (its head kept).
    pub fn file(path: &str, bytes: &[u8]) -> Self {
        Self {
            path: path.to_owned(),
            kind: Kind::File,
            size: bytes.len() as u64,
            head: bytes[..bytes.len().min(HEAD_LEN)].to_vec(),
        }
    }
}

/// Why a name is refused, whatever the allowlist says: the game's data files and anything named
/// like them, a server's world and databases, its private configuration, captures, and the
/// recording folders a capture lives in before it is scrubbed.
pub fn name_finding(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    let components: Vec<&str> = lower.split('/').filter(|c| !c.is_empty()).collect();
    if components.contains(&"fixtures") || lower.contains("packet-captures") {
        return Some("a fixture or capture path");
    }
    if components.contains(&"scrub-map") || components.contains(&"raw") {
        return Some("a capture's unscrubbed folder");
    }
    let name = components.last().copied().unwrap_or("");
    if name.ends_with(".dat") {
        return Some("a .dat file");
    }
    if name.starts_with("client_") || name.starts_with("acclient") {
        return Some("named like a game client file");
    }
    if name.ends_with(".what") {
        return Some("a client install's file list");
    }
    if name.starts_with("world.pack") {
        return Some("a world pack");
    }
    if name.ends_with(".db") || name.contains(".db-") || name.ends_with(".sqlite") {
        return Some("a database");
    }
    if name == "empyrean.toml" {
        return Some("a server's own configuration (only the .example ships)");
    }
    if name.ends_with(".pcap") || name.ends_with(".pcapng") {
        return Some("a packet capture");
    }
    None
}

/// The game's data container header, at its offset, in these leading bytes.
pub fn has_dat_magic(head: &[u8]) -> bool {
    head.get(DAT_HEADER_OFFSET..DAT_HEADER_OFFSET + 4)
        .is_some_and(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == DAT_MAGIC)
}

/// Every finding over one package's entries: each entry must be a plain file on `allowlist` (or
/// the top-level folder), with a name the rules allow, no data-container header, and under
/// [`FILE_CAP`]; every allowlisted file must be present; and the whole under `total_cap`.
pub fn scan(entries: &[Entry], allowlist: &[String], total_cap: u64) -> Vec<String> {
    let mut findings = Vec::new();
    let mut total = 0u64;
    for e in entries {
        let shown = if e.path.is_empty() { "." } else { &e.path };
        if let Some(why) = name_finding(&e.path) {
            findings.push(format!("{shown}: {why}"));
        }
        match &e.kind {
            Kind::Link(what) => findings.push(format!("{shown}: {what}; a package holds no links")),
            Kind::Dir if e.path.is_empty() => {}
            Kind::Dir => findings.push(format!("{shown}: a folder; a package is flat")),
            Kind::File => {
                total = total.saturating_add(e.size);
                if !allowlist.contains(&e.path) {
                    findings.push(format!("{shown}: not on the allowlist"));
                }
                if has_dat_magic(&e.head) {
                    findings.push(format!("{shown}: carries the game's data-file header"));
                }
                if e.size > FILE_CAP {
                    findings.push(format!(
                        "{shown}: {} bytes, over the {} byte cap for one file",
                        e.size, FILE_CAP
                    ));
                }
            }
        }
    }
    for a in allowlist {
        if !entries.iter().any(|e| e.kind == Kind::File && e.path == *a) {
            findings.push(format!("{a}: on the allowlist but missing"));
        }
    }
    if total > total_cap {
        findings.push(format!(
            "{total} bytes in all, over the {total_cap} byte cap for one package"
        ));
    }
    findings
}

/// The entries of a staging folder, recursively; links are reported as links, never followed.
pub fn staged_entries(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut out = vec![Entry {
        path: String::new(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }];
    walk(dir, "", &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> std::io::Result<()> {
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let meta = std::fs::symlink_metadata(item.path())?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            out.push(Entry {
                path,
                kind: Kind::Link("a symbolic link".to_owned()),
                size: 0,
                head: Vec::new(),
            });
        } else if ft.is_dir() {
            out.push(Entry {
                path: path.clone(),
                kind: Kind::Dir,
                size: 0,
                head: Vec::new(),
            });
            walk(&item.path(), &path, out)?;
        } else if ft.is_file() {
            if hard_linked(&meta) {
                out.push(Entry {
                    path,
                    kind: Kind::Link("a hard link".to_owned()),
                    size: meta.len(),
                    head: Vec::new(),
                });
                continue;
            }
            let mut head = vec![0u8; HEAD_LEN];
            let mut f = std::fs::File::open(item.path())?;
            let n = read_up_to(&mut f, &mut head)?;
            head.truncate(n);
            out.push(Entry {
                path,
                kind: Kind::File,
                size: meta.len(),
                head,
            });
        } else {
            out.push(Entry {
                path,
                kind: Kind::Link("neither a file nor a folder".to_owned()),
                size: 0,
                head: Vec::new(),
            });
        }
    }
    Ok(())
}

fn read_up_to(r: &mut impl std::io::Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// A file with more than one name. Staging copies every file, so a second name means something
/// linked into the folder behind the copy's back. (Windows does not report the count through the
/// standard library; there the archive scan, which sees only plain entries, is the check.)
#[cfg(unix)]
fn hard_linked(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

#[cfg(not(unix))]
fn hard_linked(_meta: &std::fs::Metadata) -> bool {
    false
}
