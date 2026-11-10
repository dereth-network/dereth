//! Getting one data set, step by step, with the caller doing the I/O.
//!
//! [`Fetch::step`] says what to do next ([`Step`]): ask the host's API, download a range, read
//! some of an archive kept so far, empty the files about to be written, or remove an archive. The
//! caller does it and reports back through the one method each step names. Each archive is
//! downloaded into storage the caller keeps (the archive's name is [`SetSource::archive_names`]),
//! checked against the MAC its link carries, unpacked into the set's files, which are checked
//! against their sizes and hashes, and then removed.
//!
//! An archive is downloaded over several connections at once, each getting one range of it at a
//! time ([`Fetch::next_range`]). What arrives is handed back for writing in order, so the archive
//! kept is always the file's first bytes: a range that arrives ahead of the ones before it is held
//! until they are in.
//!
//! An archive partly downloaded before is continued from where it stopped: the caller says how
//! much of each it holds when the fetch starts.

use sha2::{Digest, Sha256};

use crate::archive::{ArchiveError, Index, IndexReader, Unpacker, READ_SIZE};
use crate::mega::{ApiError, FileRef, MacCheck};
use crate::sources::SetSource;

/// How many times a download, or the API, is tried again before the fetch gives up.
pub const ATTEMPTS: u32 = 5;

/// How many connections a download uses at once.
pub const CONNECTIONS: usize = 4;

/// How long each range of a download is.
pub const RANGE_LEN: u64 = 8 << 20;

/// How far past the bytes kept a range may begin: at most this much is held in memory, waiting for
/// the bytes before it.
pub const WINDOW: u64 = 64 << 20;

/// What the caller does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// POST `body` to `url`, after waiting `after_ms` (a retry's back-off; zero at first). Report
    /// the answer's body with [`Fetch::api_reply`]. An answer that carries an `X-Hashcash` header
    /// is a challenge: answer it ([`crate::mega::hashcash`]) and send the same request again with
    /// the answer in an `X-Hashcash` header.
    Api {
        url: String,
        body: String,
        after_ms: u32,
    },
    /// Download the archive `archive`'s bytes `from..to`: first cut the archive to `from` bytes,
    /// then run up to `connections` connections at once, each asking [`Fetch::next_range`] for a
    /// range until it answers [`Next::Finished`]. When every connection has finished, ask for the
    /// next step.
    Download {
        archive: String,
        from: u64,
        to: u64,
        connections: usize,
    },
    /// Read `len` bytes at `offset` of the archive `archive` and hand them to [`Fetch::read`].
    Read {
        archive: String,
        offset: u64,
        len: u64,
    },
    /// Create these files of the set, empty, then call [`Fetch::done`].
    Prepare { files: Vec<String> },
    /// Remove the archive `archive`, then call [`Fetch::done`].
    Remove { archive: String },
    /// An optional archive could not be had: remove the archive `archive` and these files of the
    /// set, written so far, then call [`Fetch::done`]. The set goes on without them; `message`
    /// says so, for the log.
    Skip {
        archive: String,
        files: Vec<String>,
        message: String,
    },
    /// Every file is in place and checked. `note` says which optional files the set is without,
    /// in a line for the player.
    Done {
        files: Vec<String>,
        note: Option<String>,
    },
    /// It cannot be had this way. The archives kept so far are kept, so a later try continues
    /// them; the set's files written so far are not to be offered.
    Failed(Failure),
}

/// Why a set could not be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    /// What to tell the player.
    pub message: String,
}

/// What kind of failure, for what the player can do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Every link is dead, or answers a file other than the one listed: the list needs updating.
    Gone,
    /// The host's download limit for this connection is spent; it comes back after a while.
    OverQuota,
    /// The connection kept failing.
    Network,
    /// What arrived is not the file: damaged, or changed.
    Damaged,
    /// The archive is packed in a way this cannot unpack.
    Unsupported,
}

impl FailureKind {
    /// The name a page or a log uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            FailureKind::Gone => "gone",
            FailureKind::OverQuota => "over-quota",
            FailureKind::Network => "network",
            FailureKind::Damaged => "damaged",
            FailureKind::Unsupported => "unsupported",
        }
    }
}

/// What a connection of a download does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// GET `url`, the archive's bytes `from..to`: hand each piece that arrives to
    /// [`Fetch::range_piece`], which answers what to write, and report how it ended with
    /// [`Fetch::range_ended`].
    Range {
        id: usize,
        url: String,
        from: u64,
        to: u64,
    },
    /// The ranges left begin too far past the bytes kept: ask again in a moment.
    Wait,
    /// No range is left for this connection.
    Finished,
}

/// Bytes for the caller to write: `bytes` at `offset` of the set's file `file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub file: String,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// How a download ended, as the caller saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEnd {
    /// The response ended (all of it, or the connection closed early: the fetch can tell).
    Ended,
    /// The host answered an HTTP status other than 200; 509 is its download limit.
    Status(u16),
    /// The connection failed; the text is for the log.
    Failed(String),
}

/// Where a fetch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Asking,
    Downloading,
    Checking,
    Unpacking,
    Done,
    Failed,
}

impl Stage {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Stage::Asking => "asking",
            Stage::Downloading => "downloading",
            Stage::Checking => "checking",
            Stage::Unpacking => "unpacking",
            Stage::Done => "done",
            Stage::Failed => "failed",
        }
    }
}

/// How far a fetch is: its stage, which archive (from 1) of how many, and the stage's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub archive: usize,
    pub archives: usize,
    pub done: u64,
    pub total: u64,
}

/// A set file's check as its bytes are written: in order, to its size, to its hash.
#[derive(Debug, Clone)]
struct Output {
    name: String,
    size: u64,
    sha256: Option<String>,
    written: u64,
    hash: Sha256,
}

#[derive(Debug)]
enum State {
    /// Ask the API about the current mirror.
    Ask {
        after_ms: u32,
    },
    AwaitApi,
    Download {
        url: String,
    },
    /// Downloading, by these ranges.
    Downloading(Box<Ranges>),
    Mac {
        check: Box<MacCheck>,
        pos: u64,
    },
    Index(IndexReader),
    Prepare(Box<Unpacker>),
    Unpack(Box<Unpacker>),
    Remove,
    /// An optional archive is being given up, with why.
    Skip(String),
    Done,
    Failed(Failure),
}

/// One range of a download.
#[derive(Debug)]
struct Range {
    from: u64,
    to: u64,
    /// How many of its bytes have arrived.
    got: u64,
    /// The bytes arrived but not yet handed back for writing, which end at `from + got`.
    held: Vec<u8>,
    ended: bool,
}

/// A download's ranges, in order.
#[derive(Debug)]
struct Ranges {
    url: String,
    list: Vec<Range>,
    /// The first range not yet handed to a connection.
    next: usize,
    in_flight: usize,
    /// How the first range that went wrong ended: no more ranges are handed out.
    stop: Option<DownloadEnd>,
}

/// One data set being got.
#[derive(Debug)]
pub struct Fetch {
    set: SetSource,
    names: Vec<String>,
    archive: usize,
    mirror: usize,
    file: Option<FileRef>,
    /// How much of the current archive is kept.
    kept: Vec<u64>,
    attempts: u32,
    /// Whether the current archive was downloaded again after failing its MAC.
    redone: bool,
    seq: u64,
    state: State,
    outputs: Vec<Output>,
    /// The archive members wanted, and the output each one is.
    members: Vec<(usize, usize)>,
    range_len: u64,
    window: u64,
    /// The optional archives given up.
    skipped: Vec<usize>,
}

impl Fetch {
    /// Get `set`. `kept[i]` is how many bytes of its archive `i` the caller already keeps (from a
    /// try before), under [`SetSource::archive_names`]; missing ones are none.
    #[must_use]
    pub fn new(set: SetSource, kept: &[u64]) -> Fetch {
        let names = set.archive_names();
        let kept = (0..set.archives.len())
            .map(|i| kept.get(i).copied().unwrap_or(0).min(set.archives[i].size))
            .collect();
        let mut f = Fetch {
            set,
            names,
            archive: 0,
            mirror: 0,
            file: None,
            kept,
            attempts: 0,
            redone: false,
            seq: 1,
            state: State::Ask { after_ms: 0 },
            outputs: Vec::new(),
            members: Vec::new(),
            range_len: RANGE_LEN,
            window: WINDOW,
            skipped: Vec::new(),
        };
        f.begin_archive();
        f
    }

    /// The same, with ranges of `range_len` bytes beginning at most `window` past the bytes kept:
    /// a test's archives are smaller than [`RANGE_LEN`].
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_ranges(mut self, range_len: u64, window: u64) -> Fetch {
        self.range_len = range_len.max(1);
        self.window = window.max(self.range_len);
        self
    }

    /// The set being got.
    #[must_use]
    pub fn set(&self) -> &SetSource {
        &self.set
    }

    /// It cannot be had this way; an optional archive is given up instead, and the set goes on.
    fn fail(&mut self, kind: FailureKind, message: impl Into<String>) {
        if self.set.archives[self.archive].optional {
            self.state = State::Skip(message.into());
            return;
        }
        self.state = State::Failed(Failure {
            kind,
            message: message.into(),
        });
    }

    /// The names of the files of archive `i`.
    fn files_of(&self, i: usize) -> Vec<String> {
        self.set.archives[i]
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect()
    }

    /// The line that says which optional files the set is without.
    fn note(&self) -> Option<String> {
        let files: Vec<String> = self
            .skipped
            .iter()
            .flat_map(|&i| self.files_of(i))
            .collect();
        (!files.is_empty()).then(|| {
            format!(
                "{} couldn't be downloaded; the set works without it.",
                files.join(", ")
            )
        })
    }

    fn size(&self) -> u64 {
        self.set.archives[self.archive].size
    }

    /// Start on the current archive: an archive kept whole is checked without asking the host,
    /// since its MAC is in the link the list gives.
    fn begin_archive(&mut self) {
        self.mirror = 0;
        self.attempts = 0;
        self.redone = false;
        self.file = None;
        if self.kept[self.archive] == self.size() {
            match self.set.archives[self.archive].mirrors[0].file() {
                Ok(file) => {
                    self.state = State::Mac {
                        check: Box::new(file.mac()),
                        pos: 0,
                    };
                    self.file = Some(file);
                }
                Err(e) => self.fail(FailureKind::Gone, e.to_string()),
            }
        } else {
            self.state = State::Ask { after_ms: 0 };
        }
    }

    /// Try the next mirror; a mirror's bytes are only ever continued from the same mirror.
    fn next_mirror(&mut self, why: &str) {
        self.mirror += 1;
        self.attempts = 0;
        if self.mirror >= self.set.archives[self.archive].mirrors.len() {
            self.fail(
                FailureKind::Gone,
                format!(
                    "{} cannot be downloaded from the archive any more: {why}",
                    self.set.label
                ),
            );
        } else {
            self.kept[self.archive] = 0;
            self.state = State::Ask { after_ms: 0 };
        }
    }

    /// Try the same thing again after a pause, or give up.
    fn retry(&mut self, kind: FailureKind, why: &str) {
        self.attempts += 1;
        if self.attempts >= ATTEMPTS {
            self.fail(
                kind,
                format!("{} could not be downloaded: {why}", self.set.label),
            );
        } else {
            self.state = State::Ask {
                after_ms: 1000 << self.attempts.min(5),
            };
        }
    }

    /// What to do next.
    pub fn step(&mut self) -> Step {
        let archive = self.names[self.archive].clone();
        match &mut self.state {
            State::Ask { after_ms } => {
                let after_ms = *after_ms;
                let mirror = &self.set.archives[self.archive].mirrors[self.mirror];
                match mirror.file() {
                    Ok(file) => {
                        self.seq += 1;
                        let r = file.info_request(self.seq);
                        self.file = Some(file);
                        self.state = State::AwaitApi;
                        Step::Api {
                            url: r.url,
                            body: r.body,
                            after_ms,
                        }
                    }
                    Err(e) => {
                        let why = e.to_string();
                        self.next_mirror(&why);
                        self.step()
                    }
                }
            }
            State::AwaitApi | State::Downloading(_) => Step::Failed(Failure {
                kind: FailureKind::Network,
                message: "the last step was not reported".into(),
            }),
            State::Download { url } => {
                let url = std::mem::take(url);
                let from = self.kept[self.archive];
                let to = self.set.archives[self.archive].size;
                let mut list = Vec::new();
                let mut at = from;
                while at < to {
                    // Ranges end on multiples of their length, so a continued download's ranges
                    // are the ones it had before.
                    let end = ((at / self.range_len + 1) * self.range_len).min(to);
                    list.push(Range {
                        from: at,
                        to: end,
                        got: 0,
                        held: Vec::new(),
                        ended: false,
                    });
                    at = end;
                }
                let connections = CONNECTIONS.min(list.len()).max(1);
                self.state = State::Downloading(Box::new(Ranges {
                    url,
                    list,
                    next: 0,
                    in_flight: 0,
                    stop: None,
                }));
                Step::Download {
                    archive,
                    from,
                    to,
                    connections,
                }
            }
            State::Mac { pos, .. } => {
                let len = (self.set.archives[self.archive].size - *pos).min(READ_SIZE);
                Step::Read {
                    archive,
                    offset: *pos,
                    len,
                }
            }
            State::Index(r) => match r.wanted() {
                Some((offset, len)) => Step::Read {
                    archive,
                    offset,
                    len,
                },
                None => Step::Failed(Failure {
                    kind: FailureKind::Damaged,
                    message: "the archive's index".into(),
                }),
            },
            State::Prepare(_) => Step::Prepare {
                files: self.set.archives[self.archive]
                    .files
                    .iter()
                    .map(|f| f.name.clone())
                    .collect(),
            },
            State::Unpack(u) => match u.next_read() {
                Ok(Some((offset, len))) => Step::Read {
                    archive,
                    offset,
                    len,
                },
                Ok(None) => {
                    if let Some(why) = self.check_outputs() {
                        self.fail(FailureKind::Damaged, why);
                    } else {
                        self.state = State::Remove;
                    }
                    self.step()
                }
                Err(e) => {
                    let kind = archive_failure(&e);
                    self.fail(kind, e.to_string());
                    self.step()
                }
            },
            State::Remove => Step::Remove { archive },
            State::Skip(why) => {
                let message = why.clone();
                Step::Skip {
                    archive,
                    files: self.files_of(self.archive),
                    message,
                }
            }
            State::Done => Step::Done {
                files: (0..self.set.archives.len())
                    .filter(|i| !self.skipped.contains(i))
                    .flat_map(|i| self.files_of(i))
                    .collect(),
                note: self.note(),
            },
            State::Failed(f) => Step::Failed(f.clone()),
        }
    }

    /// The API's answer to [`Step::Api`]: its body, or why there was none.
    pub fn api_reply(&mut self, reply: Result<&str, &str>) {
        if !matches!(self.state, State::AwaitApi) {
            return;
        }
        let Some(file) = self.file.clone() else {
            return;
        };
        let body = match reply {
            Ok(b) => b,
            Err(e) => {
                self.retry(
                    FailureKind::Network,
                    &format!("the host did not answer ({e})"),
                );
                return;
            }
        };
        match file.read_info(body) {
            Ok(info) => {
                if info.size != self.size() {
                    self.next_mirror("the file at its link has changed");
                } else if info.name.is_none() {
                    self.next_mirror("its link's key does not open it");
                } else {
                    self.state = State::Download { url: info.url };
                }
            }
            Err(ApiError::OverQuota) => self.fail(
                FailureKind::OverQuota,
                "the archive's download limit for this connection is spent; try again in a few hours",
            ),
            Err(ApiError::TryAgain) => self.retry(FailureKind::Network, "the host is busy"),
            Err(e @ (ApiError::Gone | ApiError::TakenDown | ApiError::Refused(_) | ApiError::Malformed(_))) => {
                self.next_mirror(&e.to_string());
            }
        }
    }

    /// What a connection of a [`Step::Download`] does next.
    pub fn next_range(&mut self) -> Next {
        let kept = self.kept[self.archive];
        let window = self.window;
        let State::Downloading(r) = &mut self.state else {
            return Next::Finished;
        };
        if r.stop.is_some() || r.next >= r.list.len() {
            return Next::Finished;
        }
        let id = r.next;
        let range = &r.list[id];
        if range.from >= kept.saturating_add(window) {
            return Next::Wait;
        }
        r.next += 1;
        r.in_flight += 1;
        Next::Range {
            id,
            url: format!("{}/{}-{}", r.url, range.from, range.to - 1),
            from: range.from,
            to: range.to,
        }
    }

    /// A piece of range `id`, as it arrived: decrypted in place. Answers what to write in the
    /// archive now, in order, each `(offset, bytes)`; `None` to stop this connection (the piece
    /// runs past the range's end). A range already out goes on to its end even once another has
    /// gone wrong, so the bytes kept reach as far as they can before the download is tried again.
    pub fn range_piece(&mut self, id: usize, bytes: &mut [u8]) -> Option<Vec<(u64, Vec<u8>)>> {
        let decryptor = self.file.as_ref()?.decryptor();
        let State::Downloading(r) = &mut self.state else {
            return None;
        };
        if id >= r.next {
            return None;
        }
        let range = r.list.get_mut(id).filter(|g| !g.ended)?;
        let at = range.from + range.got;
        let end = at.checked_add(bytes.len() as u64)?;
        if end > range.to {
            return None;
        }
        decryptor.apply(at, bytes);
        range.got += bytes.len() as u64;
        range.held.extend_from_slice(bytes);
        Some(self.release())
    }

    /// Hand back, in order, the bytes held that now follow the bytes kept.
    fn release(&mut self) -> Vec<(u64, Vec<u8>)> {
        let mut out = Vec::new();
        let State::Downloading(r) = &mut self.state else {
            return out;
        };
        let kept = &mut self.kept[self.archive];
        for range in &mut r.list {
            if range.to <= *kept {
                continue;
            }
            let written = range.from + range.got - range.held.len() as u64;
            if written != *kept {
                break;
            }
            if !range.held.is_empty() {
                out.push((*kept, std::mem::take(&mut range.held)));
                *kept = range.from + range.got;
            }
            if *kept < range.to {
                break;
            }
        }
        out
    }

    /// How range `id` ended. Once the last range out has ended, the download is over: the archive
    /// is whole, or it is tried again from the bytes kept.
    pub fn range_ended(&mut self, id: usize, end: &DownloadEnd) {
        let size = self.size();
        let State::Downloading(r) = &mut self.state else {
            return;
        };
        if id >= r.next {
            return;
        }
        let Some(range) = r.list.get_mut(id).filter(|g| !g.ended) else {
            return;
        };
        range.ended = true;
        r.in_flight -= 1;
        let whole = range.got == range.to - range.from;
        if r.stop.is_none() && !(whole && *end == DownloadEnd::Ended) {
            r.stop = Some(end.clone());
        }
        if r.in_flight > 0 || (r.stop.is_none() && r.next < r.list.len()) {
            return;
        }
        let stop = r.stop.take();
        if self.kept[self.archive] == size {
            if let Some(file) = &self.file {
                self.state = State::Mac {
                    check: Box::new(file.mac()),
                    pos: 0,
                };
            }
            return;
        }
        match stop.unwrap_or(DownloadEnd::Ended) {
            DownloadEnd::Ended => self.retry(FailureKind::Network, "the connection closed early"),
            DownloadEnd::Status(509) => self.fail(
                FailureKind::OverQuota,
                "the archive's download limit for this connection is spent; try again in a few hours",
            ),
            DownloadEnd::Status(s) => {
                self.retry(FailureKind::Network, &format!("the host answered HTTP {s}"));
            }
            DownloadEnd::Failed(e) => self.retry(FailureKind::Network, &e),
        }
    }

    /// The bytes [`Step::Read`] asked for. Answers what to write into the set's files.
    pub fn read(&mut self, bytes: &[u8]) -> Vec<Write> {
        let size = self.size();
        match &mut self.state {
            State::Mac { check, pos } => {
                check.update(bytes);
                *pos += bytes.len() as u64;
                if *pos >= size {
                    self.mac_done();
                }
                Vec::new()
            }
            State::Index(r) => {
                match r.feed(bytes) {
                    Ok(Some(index)) => self.plan(index),
                    Ok(None) => {}
                    Err(e) => {
                        let kind = archive_failure(&e);
                        self.fail(kind, e.to_string());
                    }
                }
                Vec::new()
            }
            State::Unpack(u) => match u.feed(bytes) {
                Ok(pieces) => {
                    let mut out = Vec::with_capacity(pieces.len());
                    for p in pieces {
                        let Some(&(_, o)) = self.members.iter().find(|(m, _)| *m == p.member)
                        else {
                            continue;
                        };
                        let output = &mut self.outputs[o];
                        if p.offset != output.written {
                            self.fail(
                                FailureKind::Damaged,
                                "the archive's members came out of order",
                            );
                            return Vec::new();
                        }
                        output.hash.update(&p.bytes);
                        output.written += p.bytes.len() as u64;
                        out.push(Write {
                            file: output.name.clone(),
                            offset: p.offset,
                            bytes: p.bytes,
                        });
                    }
                    out
                }
                Err(e) => {
                    let kind = archive_failure(&e);
                    self.fail(kind, e.to_string());
                    Vec::new()
                }
            },
            _ => Vec::new(),
        }
    }

    /// The whole archive is through its MAC check.
    fn mac_done(&mut self) {
        let State::Mac { check, .. } = std::mem::replace(&mut self.state, State::Done) else {
            return;
        };
        if check.matches() {
            let size = self.size();
            self.state = State::Index(IndexReader::new(
                self.set.archives[self.archive].format,
                size,
            ));
        } else if !self.redone {
            // Damaged in transit, or kept from a try that went wrong: once more from the start.
            self.redone = true;
            self.kept[self.archive] = 0;
            self.attempts = 0;
            self.state = State::Ask { after_ms: 0 };
        } else {
            self.fail(
                FailureKind::Damaged,
                format!("{} arrived damaged twice", self.set.label),
            );
        }
    }

    /// The archive's index is read: match its members to the files wanted.
    fn plan(&mut self, index: Index) {
        let files = self.set.archives[self.archive].files.clone();
        self.outputs.clear();
        self.members.clear();
        for f in &files {
            let Some(m) = index.find(&f.name) else {
                self.fail(
                    FailureKind::Damaged,
                    format!("the archive no longer holds {}", f.name),
                );
                return;
            };
            if index.members[m].size != f.size {
                self.fail(
                    FailureKind::Damaged,
                    format!("{} in the archive is not the file listed", f.name),
                );
                return;
            }
            self.members.push((m, self.outputs.len()));
            self.outputs.push(Output {
                name: f.name.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                written: 0,
                hash: Sha256::new(),
            });
        }
        let wanted: Vec<usize> = self.members.iter().map(|&(m, _)| m).collect();
        match Unpacker::new(index, &wanted) {
            Ok(u) => self.state = State::Prepare(Box::new(u)),
            Err(e) => {
                let kind = archive_failure(&e);
                self.fail(kind, e.to_string());
            }
        }
    }

    /// Every file of the archive written: each is its size and its hash, or why not.
    fn check_outputs(&self) -> Option<String> {
        for o in &self.outputs {
            if o.written != o.size {
                return Some(format!("{} came out the wrong size", o.name));
            }
            if let Some(want) = &o.sha256 {
                let got: String = o
                    .hash
                    .clone()
                    .finalize()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                if &got != want {
                    return Some(format!("{} is not the file this set expects", o.name));
                }
            }
        }
        None
    }

    /// [`Step::Prepare`], [`Step::Remove`] or [`Step::Skip`] is done.
    pub fn done(&mut self) {
        match std::mem::replace(&mut self.state, State::Done) {
            State::Prepare(u) => self.state = State::Unpack(u),
            previous @ (State::Remove | State::Skip(_)) => {
                if matches!(previous, State::Skip(_)) {
                    self.skipped.push(self.archive);
                }
                self.kept[self.archive] = 0;
                if self.archive + 1 < self.set.archives.len() {
                    self.archive += 1;
                    self.begin_archive();
                } else {
                    self.state = State::Done;
                }
            }
            other => self.state = other,
        }
    }

    /// How far it is.
    #[must_use]
    pub fn progress(&self) -> Progress {
        let size = self.size();
        let (stage, done, total) = match &self.state {
            State::Ask { .. } | State::AwaitApi | State::Download { .. } => (Stage::Asking, 0, 0),
            State::Downloading(r) => {
                let held: u64 = r.list.iter().map(|g| g.held.len() as u64).sum();
                (Stage::Downloading, self.kept[self.archive] + held, size)
            }
            State::Mac { pos, .. } => (Stage::Checking, *pos, size),
            State::Index(_) | State::Prepare(_) => (Stage::Unpacking, 0, 0),
            State::Unpack(u) => {
                let (d, t) = u.progress();
                (Stage::Unpacking, d, t)
            }
            State::Remove | State::Skip(_) => (Stage::Unpacking, 1, 1),
            State::Done => (Stage::Done, 1, 1),
            State::Failed(_) => (Stage::Failed, 0, 0),
        };
        Progress {
            stage,
            archive: self.archive + 1,
            archives: self.set.archives.len(),
            done,
            total,
        }
    }
}

fn archive_failure(e: &ArchiveError) -> FailureKind {
    match e {
        ArchiveError::Unsupported(_) => FailureKind::Unsupported,
        _ => FailureKind::Damaged,
    }
}
