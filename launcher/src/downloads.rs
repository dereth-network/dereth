//! The Library's downloads on this computer: a data set from the community's archive, got into the
//! launcher's library folder by `dereth-datfetch`'s steps, with this module doing the requests,
//! reads and writes, in a thread of its own and one more for each connection of a download.
//!
//! A set's files are written to its folder in the library (`<library>/<set id>`), which holds
//! [`INCOMPLETE`] until every file is in and checked, and is only added to the Library then. Its
//! archives are kept in `<library>/downloads` until each is unpacked, so a download stopped part
//! way goes on from where it stopped the next time.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use dereth_datfetch::{Catalogue, DownloadEnd, Fetch, Next, Progress, SetSource, Stage, Step};
use dereth_launch::datset::{self, DatFileState, SetKind};
use dereth_launch::downloads::{self, INCOMPLETE};
use serde::Serialize;

/// The folder a set's archives are kept in while it is got.
pub const DOWNLOADS: &str = "downloads";

/// How a download is going, as the Library shows it in line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub id: String,
    pub label: String,
    /// `asking`, `downloading`, `checking`, `unpacking`, `done`, `failed` or `cancelled`.
    pub stage: &'static str,
    /// Which archive of how many.
    pub archive: usize,
    pub archives: usize,
    /// The stage's bytes so far, of how many.
    pub done: u64,
    pub total: u64,
    /// Why it failed, for the player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Status {
    pub fn starting(source: &SetSource) -> Status {
        Status {
            id: source.id.clone(),
            label: source.label.clone(),
            stage: Stage::Asking.name(),
            archive: 1,
            archives: source.archives.len(),
            done: 0,
            total: 0,
            message: None,
        }
    }

    pub fn at(&mut self, p: Progress) {
        self.stage = p.stage.name();
        self.archive = p.archive;
        self.archives = p.archives;
        self.done = p.done;
        self.total = p.total;
    }

    /// Whether it is still going.
    #[must_use]
    pub fn going(&self) -> bool {
        !matches!(self.stage, "done" | "failed" | "cancelled")
    }
}

/// What the host answered a request to its API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Body(String),
    /// A proof-of-work challenge (its `X-Hashcash` header), to answer and ask again.
    Challenge(String),
}

/// The requests a download makes: the launcher's are [`Https`]; a test's answers its own.
pub trait Net: Send + Sync {
    /// POST `body` to `url`, with the answer to a challenge when there is one.
    ///
    /// # Errors
    /// The request failed, in words for the log.
    fn post(&self, url: &str, body: &str, hashcash: Option<&str>) -> Result<Answer, String>;

    /// GET `url`, handing each piece of the body to `piece` until it answers false. Answers the
    /// HTTP status.
    ///
    /// # Errors
    /// The connection failed, in words for the log.
    fn get(&self, url: &str, piece: &mut dyn FnMut(&mut [u8]) -> bool) -> Result<u16, String>;
}

/// Requests over HTTPS.
pub struct Https(ureq::Agent);

impl Https {
    #[must_use]
    pub fn new() -> Https {
        Https(
            ureq::Agent::config_builder()
                .https_only(true)
                .http_status_as_error(false)
                .timeout_connect(Some(Duration::from_secs(20)))
                .timeout_recv_response(Some(Duration::from_secs(60)))
                .timeout_recv_body(Some(Duration::from_secs(120)))
                .build()
                .into(),
        )
    }
}

impl Default for Https {
    fn default() -> Self {
        Self::new()
    }
}

impl Net for Https {
    fn post(&self, url: &str, body: &str, hashcash: Option<&str>) -> Result<Answer, String> {
        let mut request = self
            .0
            .post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "Dereth-launcher");
        if let Some(h) = hashcash {
            request = request.header("X-Hashcash", h);
        }
        let mut response = request
            .send(body)
            .map_err(|e| crate::components::plain(&e))?;
        if let Some(c) = response
            .headers()
            .get("X-Hashcash")
            .and_then(|v| v.to_str().ok())
        {
            if hashcash.is_none() {
                return Ok(Answer::Challenge(c.to_owned()));
            }
        }
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        response
            .body_mut()
            .with_config()
            .limit(1 << 20)
            .read_to_string()
            .map(Answer::Body)
            .map_err(|e| crate::components::plain(&e))
    }

    fn get(&self, url: &str, piece: &mut dyn FnMut(&mut [u8]) -> bool) -> Result<u16, String> {
        let mut response = self
            .0
            .get(url)
            .header("User-Agent", "Dereth-launcher")
            .call()
            .map_err(|e| crate::components::plain(&e))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Ok(status);
        }
        let mut body = response.body_mut().with_config().limit(u64::MAX).reader();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = body.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 || !piece(&mut buf[..n]) {
                return Ok(status);
            }
        }
    }
}

/// Where a set is got: its folder in the library, and the folder its archives are kept in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub set: PathBuf,
    pub archives: PathBuf,
}

impl Place {
    /// Set `id`'s place in the library folder `library`.
    #[must_use]
    pub fn in_library(library: &Path, id: &str) -> Place {
        Place {
            set: library.join(id),
            archives: library.join(DOWNLOADS),
        }
    }
}

/// The set `id` on offer in `catalogue`, by the kind of set it ends as.
#[must_use]
pub fn source<'a>(catalogue: &'a Catalogue, id: &str) -> Option<(&'a SetSource, SetKind)> {
    let s = catalogue.set(id)?;
    Some((s, downloads::kind_of(s).1))
}

/// The files a folder holds as a set of `kind`, read as the Library reads them.
#[must_use]
pub fn read_set(kind: SetKind, dir: &Path) -> Vec<DatFileState> {
    match kind {
        SetKind::Modern => datset::scan_dir(dir),
        SetKind::Classic => datset::scan_classic_dir(dir),
        SetKind::Auberean => dereth_launch::auberean::scan_dir(dir, &[]),
    }
}

/// How a download ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// Every file is in its folder and checked; the folder is whole. The line says which optional
    /// files it is without.
    Done(Option<String>),
    Failed(String),
    Cancelled,
}

fn kept(place: &Place, source: &SetSource) -> Vec<u64> {
    source
        .archive_names()
        .iter()
        .map(|n| {
            std::fs::metadata(place.archives.join(n))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .collect()
}

/// Get `source`, a set of `kind`, into `place`, telling `report` how far it has got, until it is
/// done, fails or `cancel` is set. A folder not left whole is removed; archives are kept.
pub fn run(
    source: &SetSource,
    kind: SetKind,
    place: &Place,
    net: &dyn Net,
    cancel: &AtomicBool,
    report: &(dyn Fn(Progress) + Sync),
) -> End {
    let end = get(source, kind, place, net, cancel, report);
    if !matches!(end, End::Done(_)) {
        let _ = std::fs::remove_dir_all(&place.set);
    }
    end
}

fn io(what: &str, path: &Path, e: &std::io::Error) -> String {
    format!("could not {what} {}: {e}", path.display())
}

/// A download in progress, shared by its connections: each piece is decrypted and written while
/// the lock is held, so the archive is written in order.
struct Downloading<'a> {
    fetch: &'a mut Fetch,
    file: File,
    /// Why writing failed, which stops every connection.
    failed: Option<String>,
    pieces: u32,
}

fn hold<'a, 'b>(state: &'a Mutex<Downloading<'b>>) -> std::sync::MutexGuard<'a, Downloading<'b>> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One connection of a download: it gets the ranges the fetch hands it until none is left.
fn connection(
    state: &Mutex<Downloading<'_>>,
    path: &Path,
    net: &dyn Net,
    cancel: &AtomicBool,
    report: &(dyn Fn(Progress) + Sync),
) {
    loop {
        if cancel.load(Ordering::Relaxed) || hold(state).failed.is_some() {
            return;
        }
        let next = hold(state).fetch.next_range();
        let (id, got) = match next {
            Next::Range { id, url, .. } => {
                let got = net.get(&url, &mut |bytes| {
                    if cancel.load(Ordering::Relaxed) {
                        return false;
                    }
                    let mut s = hold(state);
                    if s.failed.is_some() {
                        return false;
                    }
                    let Some(writes) = s.fetch.range_piece(id, bytes) else {
                        return false;
                    };
                    for (at, bytes) in writes {
                        let written = s
                            .file
                            .seek(SeekFrom::Start(at))
                            .and_then(|_| s.file.write_all(&bytes));
                        if let Err(e) = written {
                            s.failed = Some(io("write", path, &e));
                            return false;
                        }
                    }
                    s.pieces += 1;
                    if s.pieces.is_multiple_of(8) {
                        report(s.fetch.progress());
                    }
                    true
                });
                (id, got)
            }
            Next::Wait => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Next::Finished => return,
        };
        hold(state).fetch.range_ended(
            id,
            &match got {
                Ok(200) => DownloadEnd::Ended,
                Ok(s) => DownloadEnd::Status(s),
                Err(e) => DownloadEnd::Failed(e),
            },
        );
    }
}

fn get(
    source: &SetSource,
    kind: SetKind,
    place: &Place,
    net: &dyn Net,
    cancel: &AtomicBool,
    report: &(dyn Fn(Progress) + Sync),
) -> End {
    for dir in [&place.set, &place.archives] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return End::Failed(io("make", dir, &e));
        }
    }
    let marker = place.set.join(INCOMPLETE);
    if let Err(e) = File::create(&marker) {
        return End::Failed(io("write", &marker, &e));
    }
    let mut fetch = Fetch::new(source.clone(), &kept(place, source));
    let mut outputs: Vec<(String, File)> = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return End::Cancelled;
        }
        let step = fetch.step();
        report(fetch.progress());
        match step {
            Step::Api {
                url,
                body,
                after_ms,
            } => {
                std::thread::sleep(Duration::from_millis(u64::from(after_ms)));
                let reply = match net.post(&url, &body, None) {
                    Ok(Answer::Challenge(c)) => match dereth_datfetch::mega::hashcash(&c) {
                        Some(answer) => net.post(&url, &body, Some(&answer)),
                        None => Err("the archive host set a challenge this cannot answer".into()),
                    },
                    other => other,
                };
                match reply {
                    Ok(Answer::Body(b)) => fetch.api_reply(Ok(&b)),
                    Ok(Answer::Challenge(_)) => {
                        fetch.api_reply(Err("the archive host kept setting challenges"));
                    }
                    Err(e) => fetch.api_reply(Err(&e)),
                }
            }
            Step::Download {
                archive,
                from,
                connections,
                ..
            } => {
                let path = place.archives.join(&archive);
                let file = match OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&path)
                {
                    Ok(f) => f,
                    Err(e) => return End::Failed(io("write", &path, &e)),
                };
                if let Err(e) = file.set_len(from) {
                    return End::Failed(io("write", &path, &e));
                }
                let state = Mutex::new(Downloading {
                    fetch: &mut fetch,
                    file,
                    failed: None,
                    pieces: 0,
                });
                std::thread::scope(|s| {
                    for _ in 0..connections.max(1) {
                        s.spawn(|| connection(&state, &path, net, cancel, report));
                    }
                });
                let state = state.into_inner().unwrap_or_else(PoisonError::into_inner);
                let mut file = state.file;
                let _ = file.flush();
                if let Some(e) = state.failed {
                    return End::Failed(e);
                }
                if cancel.load(Ordering::Relaxed) {
                    return End::Cancelled;
                }
            }
            Step::Read {
                archive,
                offset,
                len,
            } => {
                let path = place.archives.join(&archive);
                let mut buf = vec![0u8; usize::try_from(len).unwrap_or(0)];
                let read = File::open(&path).and_then(|mut f| {
                    f.seek(SeekFrom::Start(offset))?;
                    f.read_exact(&mut buf)
                });
                if let Err(e) = read {
                    return End::Failed(io("read", &path, &e));
                }
                for w in fetch.read(&buf) {
                    let Some((_, out)) = outputs.iter_mut().find(|(n, _)| *n == w.file) else {
                        return End::Failed(format!("{} was not prepared", w.file));
                    };
                    if let Err(e) = out
                        .seek(SeekFrom::Start(w.offset))
                        .and_then(|_| out.write_all(&w.bytes))
                    {
                        return End::Failed(io("write", &place.set.join(&w.file), &e));
                    }
                }
            }
            Step::Prepare { files } => {
                for name in files {
                    let path = place.set.join(&name);
                    match File::create(&path) {
                        Ok(f) => {
                            outputs.retain(|(n, _)| *n != name);
                            outputs.push((name, f));
                        }
                        Err(e) => return End::Failed(io("write", &path, &e)),
                    }
                }
                fetch.done();
            }
            Step::Remove { archive } => {
                let _ = std::fs::remove_file(place.archives.join(&archive));
                fetch.done();
            }
            Step::Skip {
                archive,
                files,
                message,
            } => {
                eprintln!(
                    "dereth-launcher: {} goes on without {}: {message}",
                    source.label,
                    files.join(", ")
                );
                let _ = std::fs::remove_file(place.archives.join(&archive));
                outputs.retain(|(n, _)| !files.contains(n));
                for f in &files {
                    let _ = std::fs::remove_file(place.set.join(f));
                }
                fetch.done();
            }
            Step::Done { note, .. } => {
                for (_, f) in &mut outputs {
                    let _ = f.flush();
                }
                drop(outputs);
                // The files are each checked by their hashes as they were written; last, that the
                // Library reads them as the set they are listed as.
                if let Err(why) = downloads::check(source, &read_set(kind, &place.set)) {
                    return End::Failed(why);
                }
                if let Err(e) = std::fs::remove_file(&marker) {
                    return End::Failed(io("finish", &marker, &e));
                }
                return End::Done(note);
            }
            Step::Failed(f) => return End::Failed(f.message),
        }
    }
}

#[cfg(test)]
mod tests;
