//! Fetching an art pack: a packed file of up to 2 GiB, streamed to a file beside where it is kept
//! and hashed as it comes ([`dereth_launch::art::stream`]), never held in memory. A download that
//! stops part way goes on from where it stopped the next time it is asked for, when the host
//! answers ranges. Before anything is fetched the folder's disk must have room for what is left of
//! it.

use std::path::Path;
use std::time::Duration;

use dereth_launch::art::{self, Download};

use crate::backend::Asked;

/// The free room on the disk holding `dir` (or the nearest folder above it that is there), in
/// bytes; `None` when the system will not say.
#[must_use]
pub fn free_space(dir: &Path) -> Option<u64> {
    let mut at = dir;
    while !at.exists() {
        at = at.parent()?;
    }
    free_space_at(at)
}

#[cfg(unix)]
fn free_space_at(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt as _;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: `path` is a NUL-terminated string that outlives the call, and `stat` is a writable
    // `statvfs` the call fills in; it is read only when the call says it succeeded.
    let stat = unsafe {
        if libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    #[allow(clippy::unnecessary_cast)] // the fields' types differ between systems
    Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

#[cfg(windows)]
fn free_space_at(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is a NUL-terminated wide string that outlives the call, and `free` is a
    // writable `u64`; the other outputs are optional and passed as null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(any(unix, windows)))]
fn free_space_at(_dir: &Path) -> Option<u64> {
    None
}

/// Ask GitHub's API for `api`, sending `etag` (the tag of the answer kept) as `If-None-Match`, so
/// an answer unchanged since comes back as not modified and does not count against GitHub's limit.
///
/// # Errors
/// Not GitHub's API, or GitHub's error in words for a person (a rate limit among them).
pub fn ask_github(api: &str, etag: Option<&str>) -> Result<Asked, String> {
    if !api.starts_with("https://api.github.com/") {
        return Err(format!("{api} is not GitHub's API"));
    }
    let mut request = agent()
        .get(api)
        .header("User-Agent", "Dereth-launcher")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(tag) = etag {
        request = request.header("If-None-Match", tag);
    }
    let mut response = request.call().map_err(|e| crate::components::plain(&e))?;
    if response.status().as_u16() == 304 {
        return Ok(Asked::NotModified);
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|t| t.to_str().ok())
        .map(str::to_owned);
    let body = response
        .body_mut()
        .with_config()
        .limit(4 << 20)
        .read_to_vec()
        .map_err(|e| crate::components::plain(&e))?;
    Ok(Asked::Fresh { body, etag })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        // A slow download is not cut short, but nothing waits forever.
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_global(Some(Duration::from_secs(6 * 60 * 60)))
        .build()
        .into()
}

/// Fetch `d` into its file ([`Download::partial`]), going on from what an earlier try left there,
/// and tell `progress` how far it has got. Answers the whole file's length and SHA-256, for the
/// shelf to check ([`dereth_launch::art::Shelf::finish`]).
///
/// # Errors
/// Not `https://`, too little room on the disk, the host's error, or the download stopped.
pub fn fetch_pack(d: &Download, progress: &dyn Fn(u64, u64)) -> Result<(u64, String), String> {
    if !d.url.to_ascii_lowercase().starts_with("https://") {
        return Err(format!("{} is not an https:// address", d.url));
    }
    let dir = d.partial.parent().unwrap_or(&d.partial);
    let at = art::resume_at(&d.partial, d.size);
    if !art::room_for(free_space(dir), d.size, at) {
        return Err(format!(
            "{} needs {} MB free on the disk that holds {}.",
            d.name,
            (d.size - at + (64 << 20)) >> 20,
            dir.display()
        ));
    }
    let mut request = agent().get(&d.url).header("User-Agent", "Dereth-launcher");
    if at > 0 {
        request = request.header("Range", &format!("bytes={at}-"));
    }
    let mut response = match request.call() {
        // What was kept is no longer a start of the file: begin again.
        Err(ureq::Error::StatusCode(416)) if at > 0 => {
            let _ = std::fs::remove_file(&d.partial);
            return fetch_pack(d, progress);
        }
        Err(e) => return Err(crate::components::plain(&e)),
        Ok(r) => r,
    };
    // A host that does not answer ranges sends the whole file.
    let from = if response.status().as_u16() == 206 {
        at
    } else {
        0
    };
    let mut body = response
        .body_mut()
        .with_config()
        .limit(d.size.saturating_sub(from).saturating_add(1))
        .reader();
    art::stream(&d.partial, from, d.size, &mut body, &mut |done, total| {
        progress(done, total);
    })
}
