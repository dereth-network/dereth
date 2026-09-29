//! `dereth-pcap ingest`: every capture under the corpus roots into the index.
//!
//! * **Incremental.** A top-level file whose size and modification time match its row is not
//!   opened again; a capture whose content hash is already indexed is not parsed again, whatever
//!   its name or container.
//! * **Resumable.** A capture's row is marked complete only when its last session is written;
//!   an interrupted run leaves incomplete captures, which the next run removes and redoes. A
//!   top-level file's own row is written after all its members, so an interrupted archive is
//!   reopened.
//! * **Parallel, bounded.** Worker threads take files from a shared list; each reads its capture
//!   as a stream and sends rows in small batches over a bounded channel to the one thread that
//!   writes SQLite. Nothing holds a whole capture set, and a capture on disk is never held whole
//!   (an archive member is, because the archive is read in memory).

use std::collections::{BTreeMap, HashSet};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::archive::{self, Kind};
use crate::flows::{Endpoints, Flows, PortSet};
use crate::game::Game;
use crate::index::{self, CaptureStats, FileRow};
use crate::link::Link;
use crate::pcap::CaptureReader;
use crate::transport::{MessageRow, PacketRow, Session, Sink, Summary};

/// Options for one ingest run.
#[derive(Debug, Clone)]
pub struct Options {
    pub roots: Vec<PathBuf>,
    pub index: PathBuf,
    pub ports: PortSet,
    pub jobs: usize,
    pub rebuild: bool,
    /// Print a line per file.
    pub verbose: bool,
}

/// What a run did.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub files_seen: usize,
    pub files_unchanged: usize,
    pub files_processed: usize,
    pub captures_ingested: usize,
    pub captures_already_indexed: usize,
    pub incomplete_removed: usize,
    pub orphans_removed: usize,
    pub sessions: u64,
    pub messages: u64,
    pub packets: u64,
    /// Sessions written, by game: ac1, ac2, unknown.
    pub games: [u64; 3],
    /// Sessions whose first messages were decoded as another game than the whole session votes
    /// for, and were decoded again.
    pub redecoded: u64,
    pub wall: Duration,
    pub index_bytes: u64,
}

/// Rows per batch sent to the writer.
const BATCH: usize = 2048;
/// Batches in flight before workers wait.
const CHANNEL: usize = 32;
/// Deepest archive nesting opened.
const MAX_DEPTH: usize = 4;

enum Event {
    BeginFile {
        root: String,
        path: String,
    },
    File(FileRow),
    BeginCapture {
        token: u64,
        hash: String,
        file: String,
        member: String,
        format: &'static str,
        size: u64,
    },
    BeginSession {
        token: u64,
        local: usize,
    },
    Rows {
        token: u64,
        local: usize,
        packets: Vec<PacketRow>,
        messages: Vec<MessageRow>,
    },
    EndSession {
        token: u64,
        local: usize,
        endpoints: Endpoints,
        summary: Box<Summary>,
    },
    EndCapture {
        token: u64,
        stats: CaptureStats,
    },
}

/// A top-level file to process.
#[derive(Debug, Clone)]
struct Job {
    root: PathBuf,
    rel: String,
    size: u64,
    mtime: Option<i64>,
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Job>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let path = e.path();
        let name = e.file_name();
        if name.to_string_lossy().starts_with(".index") {
            continue;
        }
        let meta = e.metadata()?;
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let mtime = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_secs()).ok());
            out.push(Job {
                root: root.to_path_buf(),
                rel,
                size: meta.len(),
                mtime,
            });
        }
    }
    Ok(())
}

fn root_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Run an ingest.
///
/// # Errors
/// When the roots cannot be walked or the index cannot be written.
pub fn run(opts: &Options) -> Result<Report, String> {
    let start = Instant::now();
    if opts.rebuild {
        for suffix in ["", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{suffix}", opts.index.display()));
            if p.exists() {
                std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
    }
    let conn = index::open_for_write(&opts.index)?;
    let incomplete_removed =
        index::purge_captures(&conn, "complete = 0").map_err(|e| e.to_string())?;
    let mut report = Report {
        incomplete_removed,
        ..Report::default()
    };
    conn.execute("DELETE FROM session WHERE complete = 0", [])
        .map_err(|e| e.to_string())?;

    // What is already indexed.
    let mut known: HashSet<String> = HashSet::new();
    {
        let mut st = conn
            .prepare("SELECT hash FROM capture WHERE complete = 1")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        for h in rows {
            known.insert(h.map_err(|e| e.to_string())?);
        }
    }
    let mut unchanged: HashSet<(String, String, u64, Option<i64>)> = HashSet::new();
    {
        let mut st = conn
            .prepare("SELECT root, path, size, mtime FROM file WHERE member = ''")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (a, b, c, d) = row.map_err(|e| e.to_string())?;
            unchanged.insert((a, b, u64::try_from(c).unwrap_or(0), d));
        }
    }

    let mut jobs = Vec::new();
    for root in &opts.roots {
        walk(root, root, &mut jobs).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    report.files_seen = jobs.len();
    let jobs: Vec<Job> = jobs
        .into_iter()
        .filter(|j| !unchanged.contains(&(root_str(&j.root), j.rel.clone(), j.size, j.mtime)))
        .collect();
    report.files_unchanged = report.files_seen - jobs.len();
    report.files_processed = jobs.len();
    let total = jobs.len();

    let known = Arc::new(Mutex::new(known));
    let queue = Arc::new(Mutex::new(
        jobs.into_iter().collect::<std::collections::VecDeque<_>>(),
    ));
    let (tx, rx) = sync_channel::<Event>(CHANNEL);
    let done_files = Arc::new(AtomicUsize::new(0));
    let already = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for w in 0..opts.jobs.max(1) {
        let queue = Arc::clone(&queue);
        let known = Arc::clone(&known);
        let tx = tx.clone();
        let ports = opts.ports.clone();
        let done_files = Arc::clone(&done_files);
        let already = Arc::clone(&already);
        let verbose = opts.verbose;
        handles.push(std::thread::spawn(move || {
            let mut ctx = Worker {
                tx,
                known,
                ports,
                next_token: (w as u64) << 40,
                already,
            };
            loop {
                let job = queue.lock().ok().and_then(|mut q| q.pop_front());
                let Some(job) = job else { break };
                let t = Instant::now();
                ctx.top(&job);
                let n = done_files.fetch_add(1, Ordering::Relaxed) + 1;
                if verbose {
                    eprintln!(
                        "[{n}/{total}] {} ({:.1}s)",
                        job.rel,
                        t.elapsed().as_secs_f64()
                    );
                }
            }
        }));
    }
    drop(tx);
    let written = write_loop(&conn, &rx)?;
    for h in handles {
        let _ = h.join();
    }
    report.captures_ingested = written.captures;
    report.sessions = written.sessions;
    report.messages = written.messages;
    report.packets = written.packets;
    report.games = written.games;
    report.redecoded = written.redecoded;
    report.captures_already_indexed = already.load(Ordering::Relaxed);

    // Captures no file refers to any more (a file that changed or went away).
    let roots: Vec<String> = opts.roots.iter().map(|r| root_str(r)).collect();
    report.orphans_removed = remove_vanished(&conn, &roots)?;
    let changed = report.captures_ingested + report.orphans_removed + report.incomplete_removed > 0;
    index::build_indexes(&conn, changed).map_err(|e| e.to_string())?;
    index::set_meta(&conn, "server_ports", &opts.ports.to_spec()).map_err(|e| e.to_string())?;
    index::set_meta(&conn, "roots", &roots.join(";")).map_err(|e| e.to_string())?;
    let now: String = conn
        .query_row("SELECT datetime('now')", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    index::set_meta(&conn, "last_ingest", &now).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|e| e.to_string())?;
    drop(conn);
    report.wall = start.elapsed();
    report.index_bytes = std::fs::metadata(&opts.index).map(|m| m.len()).unwrap_or(0);
    Ok(report)
}

/// Remove file rows for files no longer on disk under the roots being ingested, then captures no
/// file row refers to.
fn remove_vanished(conn: &Connection, roots: &[String]) -> Result<usize, String> {
    let mut gone = Vec::new();
    {
        let mut st = conn
            .prepare("SELECT file_id, root, path FROM file WHERE member = ''")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, root, path) = row.map_err(|e| e.to_string())?;
            if roots.contains(&root) && !Path::new(&root).join(&path).exists() {
                gone.push((id, root, path));
            }
        }
    }
    for (_, root, path) in &gone {
        conn.execute(
            "DELETE FROM file WHERE root = ?1 AND path = ?2",
            [root, path],
        )
        .map_err(|e| e.to_string())?;
    }
    index::purge_captures(
        conn,
        "hash NOT IN (SELECT hash FROM file WHERE hash IS NOT NULL)",
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Default)]
struct Written {
    captures: usize,
    sessions: u64,
    messages: u64,
    packets: u64,
    games: [u64; 3],
    redecoded: u64,
}

fn write_loop(conn: &Connection, rx: &Receiver<Event>) -> Result<Written, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let mut w = Written::default();
    let mut captures: BTreeMap<u64, Option<i64>> = BTreeMap::new();
    let mut sessions: BTreeMap<(u64, usize), i64> = BTreeMap::new();
    let mut codecs = index::Codecs::default();
    conn.execute_batch("BEGIN").map_err(e)?;
    let mut last_commit = Instant::now();
    let mut rows_since = 0usize;
    for ev in rx {
        match ev {
            Event::BeginFile { root, path } => {
                conn.execute(
                    "DELETE FROM file WHERE root = ?1 AND path = ?2",
                    [&root, &path],
                )
                .map_err(e)?;
            }
            Event::File(f) => index::insert_file(conn, &f).map_err(e)?,
            Event::BeginCapture {
                token,
                hash,
                file,
                member,
                format,
                size,
            } => {
                let id =
                    index::begin_capture(conn, &hash, &file, &member, format, size).map_err(e)?;
                captures.insert(token, id);
            }
            Event::BeginSession { token, local } => {
                if let Some(Some(c)) = captures.get(&token) {
                    let id = index::begin_session(conn, *c, local).map_err(e)?;
                    sessions.insert((token, local), id);
                }
            }
            Event::Rows {
                token,
                local,
                packets,
                messages,
            } => {
                if let Some(id) = sessions.get(&(token, local)) {
                    index::insert_packets(conn, *id, &packets).map_err(e)?;
                    index::insert_messages(conn, &mut codecs, *id, &messages).map_err(e)?;
                    w.packets += packets.len() as u64;
                    w.messages += messages.len() as u64;
                    rows_since += packets.len() + messages.len();
                }
            }
            Event::EndSession {
                token,
                local,
                endpoints,
                mut summary,
            } => {
                if let Some(id) = sessions.remove(&(token, local)) {
                    if summary.game.decoder() != summary.decoded_as.decoder() {
                        summary.status =
                            index::redecode_session(conn, &mut codecs, id, summary.game)
                                .map_err(e)?;
                        summary.decoded_as = summary.game;
                        w.redecoded += 1;
                    }
                    index::end_session(conn, id, &endpoints, &summary).map_err(e)?;
                    w.sessions += 1;
                    w.games[match summary.game {
                        Game::Ac1 => 0,
                        Game::Ac2 => 1,
                        Game::Unknown => 2,
                    }] += 1;
                }
            }
            Event::EndCapture { token, stats } => {
                if let Some(Some(id)) = captures.remove(&token) {
                    index::end_capture(conn, id, &stats).map_err(e)?;
                    w.captures += 1;
                }
            }
        }
        if rows_since > 200_000 || last_commit.elapsed() > Duration::from_secs(5) {
            conn.execute_batch("COMMIT; BEGIN").map_err(e)?;
            last_commit = Instant::now();
            rows_since = 0;
        }
    }
    conn.execute_batch("COMMIT").map_err(e)?;
    Ok(w)
}

struct Worker {
    tx: SyncSender<Event>,
    known: Arc<Mutex<HashSet<String>>>,
    ports: PortSet,
    next_token: u64,
    already: Arc<AtomicUsize>,
}

/// A session's rows on their way to the writer.
struct Batcher {
    tx: SyncSender<Event>,
    token: u64,
    local: usize,
    packets: Vec<PacketRow>,
    messages: Vec<MessageRow>,
    bytes: usize,
}

impl Batcher {
    fn flush(&mut self) {
        if self.packets.is_empty() && self.messages.is_empty() {
            return;
        }
        let _ = self.tx.send(Event::Rows {
            token: self.token,
            local: self.local,
            packets: std::mem::take(&mut self.packets),
            messages: std::mem::take(&mut self.messages),
        });
        self.bytes = 0;
    }

    fn maybe_flush(&mut self) {
        if self.packets.len() + self.messages.len() >= BATCH || self.bytes > 4 << 20 {
            self.flush();
        }
    }
}

impl Sink for Batcher {
    fn packet(&mut self, p: PacketRow) {
        self.packets.push(p);
        self.maybe_flush();
    }
    fn message(&mut self, m: MessageRow) {
        self.bytes += m.raw.len() + m.decoded.fields.as_ref().map_or(0, String::len);
        self.messages.push(m);
        self.maybe_flush();
    }
}

impl Worker {
    fn send(&self, ev: Event) {
        let _ = self.tx.send(ev);
    }

    fn top(&mut self, job: &Job) {
        let root = root_str(&job.root);
        let full = job.root.join(&job.rel);
        self.send(Event::BeginFile {
            root: root.clone(),
            path: job.rel.clone(),
        });
        let mut row = FileRow {
            root: root.clone(),
            path: job.rel.clone(),
            member: String::new(),
            size: job.size,
            mtime: job.mtime,
            hash: None,
            kind: "other",
            status: "not_capture",
            note: None,
        };
        let mut head = [0u8; 8];
        let n = std::fs::File::open(&full)
            .and_then(|mut f| {
                let mut got = 0;
                while got < head.len() {
                    let k = f.read(&mut head[got..])?;
                    if k == 0 {
                        break;
                    }
                    got += k;
                }
                Ok(got)
            })
            .unwrap_or(0);
        let kind = archive::sniff(&head[..n]);
        row.kind = kind.as_str();
        match kind {
            Kind::Pcap | Kind::PcapNg => {
                let hash = match hash_file(&full) {
                    Ok(h) => h,
                    Err(e) => {
                        row.status = "unreadable";
                        row.note = Some(e);
                        self.send(Event::File(row));
                        return;
                    }
                };
                row.hash = Some(hash.clone());
                row.status = self.capture(&hash, &job.rel, "", job.size, || {
                    std::fs::File::open(&full)
                        .map(|f| Box::new(BufReader::with_capacity(1 << 20, f)) as Box<dyn Read>)
                });
            }
            Kind::Zip | Kind::SevenZ => match std::fs::read(&full) {
                Ok(bytes) => {
                    row.hash = Some(blake3::hash(&bytes).to_hex().to_string());
                    let (status, note) = self.archive(&root, &job.rel, "", kind, &bytes, 1);
                    row.status = status;
                    row.note = note;
                }
                Err(e) => {
                    row.status = "unreadable";
                    row.note = Some(e.to_string());
                }
            },
            Kind::Rar => {
                row.status = "skipped";
                row.note = Some("rar is not read: extract the archive first".into());
            }
            Kind::Empty => row.status = "empty",
            Kind::Other => {}
        }
        self.send(Event::File(row));
    }

    /// Open an archive's members; returns the archive row's status and note.
    fn archive(
        &mut self,
        root: &str,
        path: &str,
        prefix: &str,
        kind: Kind,
        bytes: &[u8],
        depth: usize,
    ) -> (&'static str, Option<String>) {
        let members = match archive::members(kind, bytes) {
            Ok(m) => m,
            Err(e) => return ("unreadable", Some(e)),
        };
        let mut captures = 0usize;
        for m in members {
            let member = if prefix.is_empty() {
                m.name.clone()
            } else {
                format!("{prefix}/{}", m.name)
            };
            let mut row = FileRow {
                root: root.to_string(),
                path: path.to_string(),
                member: member.clone(),
                size: 0,
                mtime: None,
                hash: None,
                kind: "other",
                status: "not_capture",
                note: None,
            };
            match m.data {
                Err(e) => {
                    row.status = "unreadable";
                    row.note = Some(e);
                }
                Ok(data) => {
                    row.size = data.len() as u64;
                    let k = archive::sniff(&data);
                    row.kind = k.as_str();
                    match k {
                        Kind::Pcap | Kind::PcapNg => {
                            let hash = blake3::hash(&data).to_hex().to_string();
                            row.hash = Some(hash.clone());
                            let size = data.len() as u64;
                            let data = Arc::new(data);
                            row.status = self.capture(&hash, path, &member, size, || {
                                Ok(Box::new(std::io::Cursor::new(ArcBytes(Arc::clone(&data))))
                                    as Box<dyn Read>)
                            });
                            captures += 1;
                        }
                        Kind::Zip | Kind::SevenZ if depth < MAX_DEPTH => {
                            row.hash = Some(blake3::hash(&data).to_hex().to_string());
                            let (s, n) = self.archive(root, path, &member, k, &data, depth + 1);
                            row.status = s;
                            row.note = n;
                        }
                        Kind::Zip | Kind::SevenZ => {
                            row.status = "skipped";
                            row.note = Some(format!("nested deeper than {MAX_DEPTH}"));
                        }
                        Kind::Rar => {
                            row.status = "skipped";
                            row.note = Some("rar is not read: extract the archive first".into());
                        }
                        Kind::Empty => row.status = "empty",
                        Kind::Other => {}
                    }
                }
            }
            self.send(Event::File(row));
        }
        ("archive", Some(format!("{captures} capture member(s)")))
    }

    /// Ingest one capture unless its hash is already indexed; returns the file row's status.
    fn capture(
        &mut self,
        hash: &str,
        file: &str,
        member: &str,
        size: u64,
        open: impl Fn() -> std::io::Result<Box<dyn Read>>,
    ) -> &'static str {
        {
            let Ok(mut k) = self.known.lock() else {
                return "error";
            };
            if !k.insert(hash.to_string()) {
                self.already.fetch_add(1, Ordering::Relaxed);
                return "indexed";
            }
        }
        let token = self.next_token;
        self.next_token += 1;
        let reader = match open() {
            Ok(r) => r,
            Err(_) => return "unreadable",
        };
        let mut stats = CaptureStats::default();
        let mut reader = match CaptureReader::new(reader) {
            Ok(r) => r,
            Err(e) => {
                // Not ingestable: no capture row, but the file row says why.
                if let Ok(mut k) = self.known.lock() {
                    k.remove(hash);
                }
                let _ = e;
                return "unreadable";
            }
        };
        self.send(Event::BeginCapture {
            token,
            hash: hash.to_string(),
            file: file.to_string(),
            member: member.to_string(),
            format: reader.format().as_str(),
            size,
        });
        stats.link_type = reader.link_type();
        let tx = self.tx.clone();
        let mut link = Link::new();
        let mut flows = Flows::new(self.ports.clone());
        let mut open_sessions: BTreeMap<usize, (Session, Batcher)> = BTreeMap::new();
        let finish = |local: usize, s: Session, mut b: Batcher, flows: &Flows| {
            let summary = s.finish(&mut b);
            b.flush();
            let endpoints = flows.endpoints()[local].clone();
            let _ = tx.send(Event::EndSession {
                token,
                local,
                endpoints,
                summary: Box::new(summary),
            });
        };
        for rec in reader.by_ref() {
            let rec = match rec {
                Ok(r) => r,
                Err(e) => {
                    stats.error = Some(e.to_string());
                    break;
                }
            };
            stats.records += 1;
            if rec.ts != 0.0 {
                stats.t_first = Some(stats.t_first.map_or(rec.ts, |t| t.min(rec.ts)));
                stats.t_last = Some(stats.t_last.map_or(rec.ts, |t| t.max(rec.ts)));
            }
            let Some(dg) = link.frame(rec.link_type, rec.ts, &rec.data) else {
                continue;
            };
            stats.udp += 1;
            let Some(route) = flows.route(&dg) else {
                continue;
            };
            stats.game += 1;
            if let Some(c) = route.closed {
                if let Some((s, b)) = open_sessions.remove(&c) {
                    finish(c, s, b, &flows);
                }
            }
            if route.opened {
                let _ = tx.send(Event::BeginSession {
                    token,
                    local: route.session,
                });
                stats.sessions += 1;
                open_sessions.insert(
                    route.session,
                    (
                        Session::new(),
                        Batcher {
                            tx: tx.clone(),
                            token,
                            local: route.session,
                            packets: Vec::new(),
                            messages: Vec::new(),
                            bytes: 0,
                        },
                    ),
                );
            }
            if let Some((s, b)) = open_sessions.get_mut(&route.session) {
                s.datagram(route.dir, dg.ts, &dg.payload, b);
            }
        }
        for (local, (s, b)) in std::mem::take(&mut open_sessions) {
            finish(local, s, b, &flows);
        }
        link.finish();
        stats.link = link.stats;
        stats.non_game = flows.non_game + flows.ambiguous;
        stats.truncated_tail = reader.truncated_tail;
        let _ = self.tx.send(Event::EndCapture { token, stats });
        "ingested"
    }
}

/// Shared bytes readable through a cursor.
struct ArcBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for ArcBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn hash_file(p: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(p).map_err(|e| e.to_string())?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::build::ethernet_udp;
    use crate::transport::build::{connect_request, fragments, login_request};
    use dereth_primitives::ObjectId;
    use dereth_transport::CryptoSystem;
    use std::net::Ipv4Addr;

    const CLIENT: (Ipv4Addr, u16) = (Ipv4Addr::new(127, 0, 0, 1), 12345);
    const SERVER: Ipv4Addr = Ipv4Addr::new(198, 51, 100, 7);

    /// One login: the handshake, a server message and event from `P + 1`, a client action to `P`.
    fn login(frames: &mut Vec<(f64, Vec<u8>)>, t: f64, seeds: (u32, u32), world: &str) {
        login_from(frames, CLIENT, t, seeds, world);
    }

    /// [`login`] from another client endpoint.
    fn login_from(
        frames: &mut Vec<(f64, Vec<u8>)>,
        client: (Ipv4Addr, u16),
        t: f64,
        seeds: (u32, u32),
        world: &str,
    ) {
        let (p, p1) = ((SERVER, 9000), (SERVER, 9001));
        let mut s2c = CryptoSystem::new(seeds.0);
        let mut c2s = CryptoSystem::new(seeds.1);
        frames.push((t, ethernet_udp(client, p, &login_request("1802"))));
        frames.push((
            t + 0.1,
            ethernet_udp(p, client, &connect_request(seeds.0, seeds.1)),
        ));
        let info = dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 2,
            world_name: world.into(),
        };
        let blob = dereth_protocol::write_blob(&info).expect("encodes");
        frames.push((
            t + 0.2,
            ethernet_udp(p1, client, &fragments(2, s2c.next(), &[(1, 0, 1, &blob)])),
        ));
        let ev = dereth_protocol::objects::ItemServerSaysRemove {
            object: ObjectId(0x8000_0042),
        };
        let blob =
            dereth_protocol::events::pack_event(ObjectId(0x5000_0001), 1, &ev).expect("encodes");
        frames.push((
            t + 0.3,
            ethernet_udp(p1, client, &fragments(3, s2c.next(), &[(2, 0, 1, &blob)])),
        ));
        let act = dereth_protocol::objects::ItemAppraise {
            target: ObjectId(0x8000_0042),
        };
        let blob = dereth_protocol::actions::pack_action(1, &act).expect("encodes");
        frames.push((
            t + 0.4,
            ethernet_udp(client, p, &fragments(2, c2s.next(), &[(3, 0, 1, &blob)])),
        ));
    }

    fn corpus(dir: &Path) {
        let mut frames = Vec::new();
        login(&mut frames, 1_485_000_000.0, (11, 12), "Frostfell");
        login(&mut frames, 1_485_000_100.0, (21, 22), "Harvestgain");
        let refs: Vec<(f64, &[u8])> = frames.iter().map(|(t, f)| (*t, f.as_slice())).collect();
        let pcap = crate::pcap::build::pcap(1, false, false, &refs);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("two-logins.pcap"), &pcap).unwrap();
        // The same capture inside a zip: indexed once.
        let zip = crate::archive::build::zip(&[
            ("inner/two-logins.pcap", &pcap, true),
            ("notes.csv", b"a,b\n", true),
        ]);
        std::fs::write(dir.join("sub/archive.zip"), zip).unwrap();
        std::fs::write(dir.join("skipped.rar"), b"Rar!\x1A\x07\x00rest").unwrap();
        std::fs::write(dir.join("readme.txt"), b"notes").unwrap();
    }

    fn one(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    }

    #[test]
    fn an_ingest_end_to_end_then_incrementally() {
        let dir = std::env::temp_dir().join(format!("rp1-ingest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        corpus(&dir);
        let opts = Options {
            roots: vec![dir.clone()],
            index: dir.join(".index/retail.sqlite"),
            ports: PortSet::default(),
            jobs: 2,
            rebuild: false,
            verbose: false,
        };
        let r = run(&opts).expect("ingests");
        assert_eq!(
            (
                r.files_seen,
                r.captures_ingested,
                r.captures_already_indexed
            ),
            (4, 1, 1),
            "{r:?}"
        );
        assert_eq!((r.sessions, r.messages), (2, 6));
        let conn = index::open_for_read(&opts.index).unwrap();
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM session WHERE partial = 0 AND login = 1 AND version = '1802'"
            ),
            2
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM session WHERE game = 'ac1' AND game_ac1 = 3 AND game_ac2 = 0"
            ),
            2
        );
        assert_eq!(
            one(&conn, "SELECT count(*) FROM message WHERE status = 'ok'"),
            6
        );
        assert_eq!(
            one(&conn, "SELECT sum(cks_ok_s2c) FROM session"),
            6,
            "ConnectRequest + 2 keyed, twice"
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM session WHERE desynced_after_s2c IS NOT NULL"
            ),
            0
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM session WHERE server_ports = '9000,9001'"
            ),
            2
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(DISTINCT message_id) FROM message_guid WHERE guid = 2147483714"
            ),
            4
        );
        assert_eq!(
            one(&conn, "SELECT count(*) FROM message WHERE json_extract(fields(dir, raw), '$.world_name') = 'Harvestgain'"),
            1
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM file WHERE status = 'skipped' AND kind = 'rar'"
            ),
            1
        );
        assert_eq!(
            one(&conn, "SELECT count(*) FROM file WHERE kind = 'pcap'"),
            2
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(DISTINCT hash) FROM file WHERE kind = 'pcap'"
            ),
            1
        );
        drop(conn);

        // Nothing changed: nothing is reopened.
        let r = run(&opts).expect("ingests again");
        assert_eq!((r.files_unchanged, r.captures_ingested), (4, 0), "{r:?}");

        // The loose copy goes away: the capture stays, because the zip still holds it.
        std::fs::remove_file(dir.join("two-logins.pcap")).unwrap();
        let r = run(&opts).expect("ingests after a removal");
        assert_eq!(r.orphans_removed, 0);
        let conn = index::open_for_read(&opts.index).unwrap();
        assert_eq!(one(&conn, "SELECT count(*) FROM capture"), 1);
        drop(conn);

        // The zip goes too: the capture is no longer referred to and is removed with its rows.
        std::fs::remove_file(dir.join("sub/archive.zip")).unwrap();
        let r = run(&opts).expect("ingests after the last reference went");
        assert_eq!(r.orphans_removed, 1);
        let conn = index::open_for_read(&opts.index).unwrap();
        assert_eq!(
            one(&conn, "SELECT count(*) FROM message")
                + one(&conn, "SELECT count(*) FROM message_guid"),
            0
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A connection from `client` whose server sends `lead_ac1` AC1 messages, then `ac2` AC2
    /// messages, and whose client sends one AC2 message.
    fn other_game(
        frames: &mut Vec<(f64, Vec<u8>)>,
        t: f64,
        client: (Ipv4Addr, u16),
        lead_ac1: u32,
        ac2: u32,
    ) {
        let (p, p1) = ((SERVER, 9004), (SERVER, 9005));
        let seeds = (31, 32);
        let mut s2c = CryptoSystem::new(seeds.0);
        let mut c2s = CryptoSystem::new(seeds.1);
        frames.push((t, ethernet_udp(client, p, &login_request("scrambled"))));
        frames.push((
            t + 0.1,
            ethernet_udp(p, client, &connect_request(seeds.0, seeds.1)),
        ));
        let ac1 = dereth_protocol::write_blob(&dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 2,
            world_name: "W".into(),
        })
        .expect("encodes");
        for i in 0..lead_ac1 + ac2 {
            let blob = if i < lead_ac1 {
                ac1.clone()
            } else {
                [0x86, 0x00, 0x01, 0x00, 1, 2, 3, 4].to_vec()
            };
            let dg = fragments(2 + i, s2c.next(), &[(u64::from(i) + 1, 0, 1, &blob)]);
            frames.push((t + 0.2 + f64::from(i) * 0.01, ethernet_udp(p1, client, &dg)));
        }
        let dg = fragments(
            2,
            c2s.next(),
            &[(1, 0, 1, &[0xC8, 0x00, 0x01, 0x00, 0, 0, 0, 0])],
        );
        frames.push((t + 9.0, ethernet_udp(client, p, &dg)));
    }

    #[test]
    fn each_session_gets_its_game_and_ac2_messages_are_stored_whole() {
        let dir = std::env::temp_dir().join(format!("pcap-game-ingest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut frames = Vec::new();
        login(&mut frames, 1_485_000_000.0, (11, 12), "Frostfell");
        other_game(
            &mut frames,
            1_485_000_100.0,
            (Ipv4Addr::new(127, 0, 0, 2), 2000),
            0,
            40,
        );
        // The first 64 messages are AC1-like, the rest (and most of the session) AC2.
        let lead = u32::try_from(crate::transport::LEAD_IN).unwrap();
        other_game(
            &mut frames,
            1_485_000_200.0,
            (Ipv4Addr::new(127, 0, 0, 3), 3000),
            lead,
            200,
        );
        let refs: Vec<(f64, &[u8])> = frames.iter().map(|(t, f)| (*t, f.as_slice())).collect();
        std::fs::write(
            dir.join("games.pcap"),
            crate::pcap::build::pcap(1, false, false, &refs),
        )
        .unwrap();
        let opts = Options {
            roots: vec![dir.clone()],
            index: dir.join(".index/retail.sqlite"),
            ports: PortSet::default(),
            jobs: 1,
            rebuild: false,
            verbose: false,
        };
        let r = run(&opts).expect("ingests");
        assert_eq!(
            (r.sessions, r.games, r.redecoded),
            (3, [1, 2, 0], 1),
            "{r:?}"
        );
        let conn = index::open_for_read(&opts.index).unwrap();
        // The AC1 session is decoded as before.
        assert_eq!(
            one(&conn, "SELECT count(*) FROM session WHERE game = 'ac1' AND ok_s2c = 2 AND ok_c2s = 1 AND game_ac1 = 3"),
            1
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM v_message WHERE game = 'ac1' AND status = 'ok'"
            ),
            3
        );
        // The AC2 sessions: every message stored whole under its full first dword, none decoded.
        assert_eq!(
            one(&conn, "SELECT count(*) FROM session WHERE game = 'ac2'"),
            2
        );
        assert_eq!(
            one(&conn, "SELECT count(*) FROM session WHERE game = 'ac2' AND unknown_s2c + ok_s2c + error_s2c + unknown_c2s = 0"),
            2
        );
        assert_eq!(
            one(
                &conn,
                "SELECT sum(ac2_s2c) + sum(ac2_c2s) FROM session WHERE game = 'ac2'"
            ),
            40 + 1 + 264 + 1
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM v_message WHERE game = 'ac2' AND status <> 'ac2'"
            ),
            0
        );
        assert_eq!(one(&conn, "SELECT count(*) FROM message WHERE status = 'ac2' AND mtype = 0x10086 AND dir = 's2c'"), 240);
        assert_eq!(one(&conn, "SELECT count(*) FROM message WHERE status = 'ac2' AND mtype = 0x100C8 AND dir = 'c2s'"), 2);
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM message WHERE status = 'ac2' AND codec_id IS NOT NULL"
            ),
            0
        );
        // The re-decoded session left no AC1 decode behind.
        assert_eq!(
            one(&conn, "SELECT count(*) FROM session WHERE game = 'ac2' AND game_ac1 = 64 AND game_ac2 = 201 AND ac2_s2c = 264"),
            1
        );
        assert_eq!(
            one(
                &conn,
                "SELECT count(*) FROM message WHERE status = 'ac2' AND mtype = 0xF7E1"
            ),
            64
        );
        // Reports run and are per game.
        let stats = crate::report::stats(&conn).unwrap();
        assert!(
            stats.contains("AC2 census") && stats.contains("0x10086"),
            "{stats}"
        );
        let triage = crate::report::triage(&conn, 1).unwrap();
        assert!(
            triage.contains("| ac2 | s2c | 0 | 0 | 0 | 0 | 304 | 304 |"),
            "{triage}"
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_capture_is_removed_and_redone() {
        let dir = std::env::temp_dir().join(format!("rp1-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        corpus(&dir);
        let opts = Options {
            roots: vec![dir.clone()],
            index: dir.join(".index/retail.sqlite"),
            ports: PortSet::default(),
            jobs: 1,
            rebuild: false,
            verbose: false,
        };
        run(&opts).expect("ingests");
        // Simulate a run killed mid-capture: the capture is incomplete and the files are unrecorded.
        let conn = index::open_for_write(&opts.index).unwrap();
        conn.execute_batch("UPDATE capture SET complete = 0; DELETE FROM file;")
            .unwrap();
        drop(conn);
        let r = run(&opts).expect("resumes");
        assert_eq!((r.incomplete_removed, r.captures_ingested), (1, 1), "{r:?}");
        let conn = index::open_for_read(&opts.index).unwrap();
        assert_eq!(one(&conn, "SELECT count(*) FROM message"), 6);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pcap_of(frames: &[(f64, Vec<u8>)]) -> Vec<u8> {
        let refs: Vec<(f64, &[u8])> = frames.iter().map(|(t, f)| (*t, f.as_slice())).collect();
        crate::pcap::build::pcap(1, false, false, &refs)
    }

    /// A capture of `n` logins from distinct clients, all different from any other `seed`'s.
    fn logins(seed: u32, n: u8) -> Vec<u8> {
        let mut frames = Vec::new();
        for i in 0..n {
            let client = (Ipv4Addr::new(127, 0, 1, i), 4000 + u16::from(i));
            let t = 1_485_000_000.0 + f64::from(seed) * 1000.0 + f64::from(i) * 10.0;
            let seeds = (seed * 100 + u32::from(i), seed * 100 + 50 + u32::from(i));
            login_from(&mut frames, client, t, seeds, "W");
        }
        pcap_of(&frames)
    }

    fn opts(roots: &[&Path], index: PathBuf, jobs: usize) -> Options {
        Options {
            roots: roots.iter().map(|r| r.to_path_buf()).collect(),
            index,
            ports: PortSet::default(),
            jobs,
            rebuild: true,
            verbose: false,
        }
    }

    type SessionKeys = BTreeMap<String, (String, i64, i64)>;

    /// Every session by its key: its first time and client port (from the capture), and its id.
    fn session_keys(index: &Path) -> SessionKeys {
        let conn = index::open_for_read(index).unwrap();
        let mut st = conn
            .prepare("SELECT session_key, printf('%.3f', t_first), client_port, session_id FROM v_session")
            .unwrap();
        let rows = st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
            })
            .unwrap();
        rows.map(Result::unwrap).collect()
    }

    /// Every message by its key, with its direction, type and bytes.
    fn message_keys(index: &Path) -> BTreeMap<String, (String, i64, Vec<u8>)> {
        let conn = index::open_for_read(index).unwrap();
        let mut st = conn
            .prepare("SELECT msg_key, dir, mtype, raw FROM v_message")
            .unwrap();
        let rows = st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
            })
            .unwrap();
        rows.map(Result::unwrap).collect()
    }

    fn without_ids(m: &SessionKeys) -> BTreeMap<String, (String, i64)> {
        m.iter()
            .map(|(k, (t, p, _))| (k.clone(), (t.clone(), *p)))
            .collect()
    }

    #[test]
    fn session_and_message_keys_do_not_depend_on_ingest_order_workers_or_roots() {
        let dir = std::env::temp_dir().join(format!("pcap-keys-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("one.pcap"), logins(1, 3)).unwrap();
        std::fs::write(a.join("two.pcap"), logins(2, 2)).unwrap();
        std::fs::write(b.join("three.pcap"), logins(3, 2)).unwrap();
        std::fs::write(b.join("four.pcap"), logins(4, 1)).unwrap();
        let (i1, i2, i3) = (
            dir.join("i1.sqlite"),
            dir.join("i2.sqlite"),
            dir.join("i3.sqlite"),
        );
        run(&opts(&[&a, &b], i1.clone(), 1)).expect("ingests a, b");
        run(&opts(&[&b, &a], i2.clone(), 4)).expect("ingests b, a");
        run(&opts(&[&b], i3.clone(), 2)).expect("ingests b alone");
        let (k1, k2, k3) = (session_keys(&i1), session_keys(&i2), session_keys(&i3));
        assert_eq!(k1.len(), 8);
        // The row ids moved; the keys did not.
        assert!(
            k1.iter().any(|(k, v)| k2[k].2 != v.2),
            "the second ingest must number sessions differently"
        );
        assert_eq!(without_ids(&k1), without_ids(&k2));
        let all = without_ids(&k1);
        for (k, v) in without_ids(&k3) {
            assert_eq!(all.get(&k), Some(&v), "{k}");
        }
        // Key form: 16 hex digits of the capture hash, a colon, the ordinal.
        for k in k1.keys() {
            let (h, n) = k.split_once(':').unwrap();
            assert!(
                h.len() == 16
                    && h.bytes().all(|c| c.is_ascii_hexdigit())
                    && n.parse::<u32>().is_ok(),
                "{k}"
            );
        }
        let m1 = message_keys(&i1);
        assert_eq!(m1.len(), 8 * 3);
        assert_eq!(m1, message_keys(&i2));
        // An incremental ingest that adds the other root keeps the keys already given.
        run(&Options {
            rebuild: false,
            ..opts(&[&b, &a], i3.clone(), 3)
        })
        .expect("adds a");
        assert_eq!(without_ids(&session_keys(&i3)), all);
        // `sid()` finds a session's current id from its key.
        let conn = index::open_for_read(&i2).unwrap();
        for (k, (_, _, id)) in &k2 {
            let got: i64 = conn.query_row("SELECT sid(?1)", [k], |r| r.get(0)).unwrap();
            assert_eq!(got, *id, "{k}");
        }
        let none: Option<i64> = conn
            .query_row("SELECT sid('0000000000000000:9')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(none, None);
        // A message key names its session too.
        let (mk, _) = message_keys(&i2).into_iter().next().unwrap();
        let got: i64 = conn
            .query_row("SELECT sid(?1)", [&mk], |r| r.get(0))
            .unwrap();
        assert_eq!(got, k2[mk.split('#').next().unwrap()].2);
        // `stats` cites sessions by key.
        let stats = crate::report::stats(&conn).unwrap();
        assert!(
            stats.contains("session_key") && k2.keys().any(|k| stats.contains(k.as_str())),
            "{stats}"
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_inside_an_archive_has_the_same_keys_as_its_loose_copy() {
        let dir = std::env::temp_dir().join(format!("pcap-keys-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (loose, zipped) = (dir.join("loose"), dir.join("zipped"));
        std::fs::create_dir_all(&loose).unwrap();
        std::fs::create_dir_all(&zipped).unwrap();
        let pcap = logins(5, 2);
        std::fs::write(loose.join("capture.pcap"), &pcap).unwrap();
        let zip = crate::archive::build::zip(&[("deep/renamed.pcap", &pcap, true)]);
        std::fs::write(zipped.join("bundle.zip"), zip).unwrap();
        let (i1, i2) = (dir.join("i1.sqlite"), dir.join("i2.sqlite"));
        run(&opts(&[&loose], i1.clone(), 1)).expect("ingests the loose copy");
        run(&opts(&[&zipped], i2.clone(), 1)).expect("ingests the zipped copy");
        let (k1, k2) = (session_keys(&i1), session_keys(&i2));
        assert_eq!(k1.len(), 2);
        assert_eq!(without_ids(&k1), without_ids(&k2));
        let hash = blake3::hash(&pcap).to_hex().to_string();
        assert!(
            k1.keys()
                .all(|k| k.starts_with(&format!("{}:", &hash[..16]))),
            "{k1:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sessions_of_one_capture_are_numbered_in_time_order() {
        let dir = std::env::temp_dir().join(format!("pcap-keys-time-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The later session's datagrams come first in the file.
        let mut frames = Vec::new();
        login_from(
            &mut frames,
            (Ipv4Addr::new(127, 0, 0, 9), 7000),
            1_485_000_500.0,
            (61, 62),
            "Late",
        );
        login_from(
            &mut frames,
            (Ipv4Addr::new(127, 0, 0, 8), 8000),
            1_485_000_100.0,
            (71, 72),
            "Early",
        );
        std::fs::write(dir.join("c.pcap"), pcap_of(&frames)).unwrap();
        let index = dir.join(".index/i.sqlite");
        run(&opts(&[&dir], index.clone(), 1)).expect("ingests");
        let conn = index::open_for_read(&index).unwrap();
        let rows: Vec<(String, i64, i64)> = conn
            .prepare("SELECT session_key, idx, client_port FROM session ORDER BY t_first")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].0.ends_with(":0") && rows[0].2 == 8000, "{rows:?}");
        assert!(rows[1].0.ends_with(":1") && rows[1].2 == 7000, "{rows:?}");
        // The order the file met them in is still `idx`.
        assert_eq!((rows[0].1, rows[1].1), (1, 0));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
