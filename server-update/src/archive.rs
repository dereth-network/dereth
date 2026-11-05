//! Unpacking a release archive: a `.zip` (Windows) or a `.tar.gz` (elsewhere) holding one folder
//! named after the archive, and in it only plain files. Anything else (a link, a nested folder, a
//! path that leaves the folder) refuses the archive.

use std::io::Read;
use std::path::Path;

/// One file of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Marked executable in the archive.
    pub executable: bool,
}

/// The files under `root/` of an archive named `name` (`.zip` or `.tar.gz`).
///
/// # Errors
/// When the archive cannot be read or holds anything but plain files in `root/`.
pub fn read(name: &str, bytes: &[u8], root: &str) -> Result<Vec<File>, String> {
    let entries = if name.ends_with(".zip") {
        zip_entries(bytes)?
    } else if name.ends_with(".tar.gz") {
        let mut tar = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut tar)
            .map_err(|e| format!("{name}: gzip: {e}"))?;
        tar_entries(&tar)?
    } else {
        return Err(format!("{name} is neither a .zip nor a .tar.gz"));
    };
    let mut files = Vec::new();
    for e in entries {
        let path = e.path.trim_end_matches('/');
        if path == root && e.dir {
            continue;
        }
        let file_name = path
            .strip_prefix(root)
            .and_then(|p| p.strip_prefix('/'))
            .filter(|p| !p.is_empty() && !p.contains(['/', '\\']) && *p != "." && *p != "..")
            .ok_or_else(|| format!("{name}: `{}` is not a file of {root}/: refused", e.path))?;
        if e.dir || e.link {
            return Err(format!("{name}: `{}` is not a plain file: refused", e.path));
        }
        files.push(File {
            name: file_name.to_owned(),
            bytes: e.bytes,
            executable: e.executable,
        });
    }
    Ok(files)
}

/// Writes `files` into `dir` (created when absent), executables marked so on Unix.
///
/// # Errors
/// When a file cannot be written.
pub fn write(files: &[File], dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for f in files {
        let path = dir.join(&f.name);
        std::fs::write(&path, &f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if f.executable {
            make_executable(&path)?;
        }
    }
    Ok(())
}

/// Marks `path` executable (Unix; nothing to do elsewhere).
///
/// # Errors
/// When its permissions cannot be set.
pub fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

struct Entry {
    path: String,
    dir: bool,
    link: bool,
    executable: bool,
    bytes: Vec<u8>,
}

fn zip_entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| format!("zip: {e}"))?;
        let mode = f.unix_mode().unwrap_or(0);
        let link = mode & 0o170_000 == 0o120_000;
        let dir = f.is_dir();
        let mut data = Vec::new();
        if !dir && !link {
            f.read_to_end(&mut data).map_err(|e| format!("zip: {e}"))?;
        }
        out.push(Entry {
            path: f.name().to_owned(),
            dir,
            link,
            executable: mode & 0o111 != 0,
            bytes: data,
        });
    }
    Ok(out)
}

fn octal(b: &[u8]) -> Result<u64, String> {
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

fn tar_entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
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
        let mode = octal(&h[100..108])?;
        let size = octal(&h[124..136])?;
        let len = usize::try_from(size).map_err(|_| "tar: entry too large".to_owned())?;
        let start = at + 512;
        let data = bytes
            .get(start..start + len)
            .ok_or("tar: entry runs past the end")?;
        let (dir, link) = match h[156] {
            b'0' | 0 => (false, false),
            b'5' => (true, false),
            _ => (false, true),
        };
        out.push(Entry {
            path,
            dir,
            link,
            executable: mode & 0o111 != 0,
            bytes: if dir || link {
                Vec::new()
            } else {
                data.to_vec()
            },
        });
        at = start + len.div_ceil(512) * 512;
    }
    Ok(out)
}
