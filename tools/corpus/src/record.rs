//! `dereth-corpus record <slug>` -- recording a session straight into the corpus.
//!
//! A logging UDP proxy sits between the retail client and a server. The client is pointed at the
//! proxy (`-h 127.0.0.1:9100`), the proxy forwards every datagram both ways and writes one JSON
//! line per datagram, `{t, dir, pair, len, data}`, to the untracked
//! `fixtures/packet-captures/raw/<slug>.jsonl`. When the recording stops it is split into one file
//! per login, each is pseudonymised by the same pass `scrub` runs, and only the scrubbed files are
//! written into `fixtures/packet-captures/`.
//!
//! # Two ports
//!
//! The protocol uses a pair of adjacent ports: the client talks to the server's port `P`, but
//! sends its `ConnectResponse` to `P + 1`, and checks only the source *address* of what comes
//! back, never the port. So the proxy listens on two adjacent ports and forwards each to the
//! matching server port, through one upstream socket per pair so that replies come back on the
//! listener they belong to. `pair` in a line is that index: 0 for `P`, 1 for `P + 1`.
//!
//! Nothing is bound with address reuse: on Windows that would let a second proxy bind the same
//! port and have the system quietly deliver the traffic to the first, so a forgotten proxy would
//! make a later recording come out empty. Without it the second bind fails, loudly.
//!
//! # Stopping
//!
//! Ctrl-C stops a recording. So does a clean logout: once a datagram carrying `Disconnect` has
//! crossed and the link has then been quiet for `--linger` seconds (three by default), the
//! recording ends by itself. `--keep-going` turns the second off, for a recording that should
//! span a logout and a fresh login.
//!
//! # Splitting
//!
//! Each login is its own key stream, so a file holding two is two recordings. A session starts at
//! the server's `ConnectRequest`, and the recording of it starts at the last client `LoginRequest`
//! before that -- the client resends its `LoginRequest` until it is answered, and the corpus wants
//! each recording to open with the one that was. One `ConnectRequest` gives `<slug>.jsonl`; more
//! give `<slug>-1.jsonl`, `<slug>-2.jsonl`, ..., each rebased to start at `t = 0`, with the whole
//! capture kept as `raw/unsplit/<slug>.jsonl`. Datagrams before the first answered login (earlier
//! resends of it) are dropped, and counted.

use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::net::{SocketAddr, UdpSocket};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use dereth_transport::wire::PacketFlags;

use crate::corpus::PACKET_CAPTURES;
use crate::pyjson;

type Error = Box<dyn std::error::Error>;

/// The proxy's default listen address: the client is launched with `-h 127.0.0.1:9100`.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:9100";
/// The server's default address: a local server on the port ACE and Empyrean both default to.
pub const DEFAULT_SERVER: &str = "127.0.0.1:9000";
/// How long a link must be quiet after a `Disconnect` before the recording stops by itself.
pub const DEFAULT_LINGER: Duration = Duration::from_secs(3);

/// How many adjacent ports the protocol uses.
const PAIRS: usize = 2;

/// The parsed `record` command line.
#[derive(Debug)]
pub struct Args {
    /// The recording's name.
    pub slug: String,
    /// The checkout.
    pub repo: PathBuf,
    /// Where the proxy listens; it also listens on the next port up.
    pub listen: SocketAddr,
    /// Where the server is; the proxy also forwards to the next port up.
    pub server: SocketAddr,
    /// Stop after a clean logout.
    pub stop_on_logout: bool,
    /// How long the link must be quiet after a `Disconnect` before that stop.
    pub linger: Duration,
    /// The stand-in dictionary, as for `scrub`.
    pub pseudonyms: Option<PathBuf>,
}

/// Why a slug is refused, or `None` for a good one.
///
/// Lower-case kebab-case, two to four words of `[a-z0-9]`, starting with a letter, and not ending
/// in a number, which is how the parts of a split recording are told apart.
#[must_use]
pub fn slug_problem(slug: &str) -> Option<String> {
    let words: Vec<&str> = slug.split('-').collect();
    if !(2..=4).contains(&words.len()) {
        return Some(format!(
            "{slug:?} has {} word(s); a slug is two to four",
            words.len()
        ));
    }
    if words.iter().any(|w| w.is_empty()) {
        return Some(format!("{slug:?} has an empty word"));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Some(format!(
            "{slug:?} is not lower-case kebab-case (a-z, 0-9 and -)"
        ));
    }
    if !slug.as_bytes()[0].is_ascii_lowercase() {
        return Some(format!("{slug:?} does not start with a letter"));
    }
    if words
        .last()
        .is_some_and(|w| w.bytes().all(|b| b.is_ascii_digit()))
    {
        return Some(format!(
            "{slug:?} ends in a number, which is how the parts of a split recording are named"
        ));
    }
    None
}

/// `record <slug>`: refuse a bad or taken slug, proxy until stopped, then split and scrub.
///
/// # Errors
/// On a refused slug, a port that will not bind, an empty capture, or a capture that cannot be
/// split or scrubbed. The raw capture is kept in every case after it was written.
pub fn run(args: &Args) -> Result<(), Error> {
    if let Some(why) = slug_problem(&args.slug) {
        return Err(why.into());
    }
    let paths = Paths::new(&args.repo);
    paths.refuse_taken(&args.slug)?;
    let raw = paths.raw.join(format!("{}.jsonl", args.slug));

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    // A second handler in one process is refused; the first one sets the same kind of flag.
    let _ = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst));

    let counts = proxy(args, &raw, &stop)?;
    if counts.total() == 0 {
        let _ = std::fs::remove_file(&raw);
        return Err(format!(
            "nothing was captured. Check, in order:\n  \
             1. the client was launched with  -h {}  (the proxy's port, not the server's)\n  \
             2. no other proxy holds the port -- this one refuses to start if one does\n  \
             3. the server really is on {}",
            args.listen, args.server
        )
        .into());
    }
    println!(
        "\ncaptured {} client-to-server and {} server-to-client datagrams into {}",
        counts.c2s,
        counts.s2c,
        raw.display()
    );
    finish(&paths, &args.slug, args.pseudonyms.clone()).map(|_| ())
}

/// The directories one recording touches.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The tracked folder the scrubbed recordings go into.
    pub out: PathBuf,
    /// The untracked folder the raw capture and its parts go into.
    pub raw: PathBuf,
    /// The untracked folder the real-to-stand-in map goes into.
    pub map: PathBuf,
    /// The checkout.
    pub repo: PathBuf,
}

impl Paths {
    /// The standard layout under `repo`.
    #[must_use]
    pub fn new(repo: &Path) -> Self {
        let out = repo.join(PACKET_CAPTURES);
        Self {
            raw: out.join("raw"),
            map: out.join("scrub-map"),
            out,
            repo: repo.to_owned(),
        }
    }

    /// Refuse a slug the corpus, or an earlier raw capture, already uses: a slug is stable once
    /// assigned, and a recording is never overwritten.
    fn refuse_taken(&self, slug: &str) -> Result<(), Error> {
        let taken = |dir: &Path| -> Result<Option<String>, Error> {
            if !dir.is_dir() {
                return Ok(None);
            }
            Ok(crate::corpus::recordings_in(dir)?
                .into_iter()
                .find(|n| is_part_of(n, slug)))
        };
        for dir in [&self.out, &self.raw] {
            if let Some(n) = taken(dir)? {
                return Err(format!(
                    "{slug}: {}/{n}.jsonl already exists; a recording is never overwritten -- \
                     choose another slug",
                    dir.display()
                )
                .into());
            }
        }
        Ok(())
    }
}

/// Whether `name` is `slug` or one of its split parts, `slug-<n>`.
fn is_part_of(name: &str, slug: &str) -> bool {
    name == slug
        || name
            .strip_prefix(slug)
            .and_then(|r| r.strip_prefix('-'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// How many datagrams crossed each way.
#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    /// Client to server.
    pub c2s: usize,
    /// Server to client.
    pub s2c: usize,
}

impl Counts {
    fn total(self) -> usize {
        self.c2s + self.s2c
    }
}

/// What every forwarding thread shares: the output, and what the stop rule reads.
struct Shared {
    out: BufWriter<File>,
    t0: Option<Instant>,
    client: [Option<SocketAddr>; PAIRS],
    counts: Counts,
    last: Option<Instant>,
    disconnect_seen: bool,
    failure: Option<String>,
}

/// Run the proxy until `stop` is set or, with `stop_on_logout`, until the link has been quiet for
/// `linger` after a `Disconnect`. Writes the capture to `raw` as it goes.
///
/// # Errors
/// When a port will not bind or the capture cannot be written.
pub fn proxy(args: &Args, raw: &Path, stop: &Arc<AtomicBool>) -> Result<Counts, Error> {
    let mut listeners = Vec::with_capacity(PAIRS);
    for i in 0..PAIRS {
        let at = offset(args.listen, i)?;
        let s = UdpSocket::bind(at).map_err(|e| {
            format!(
                "cannot bind {at}: {e}\nAnother proxy is almost certainly still running and \
                 holding the port. On Windows, find it with\n  \
                 Get-NetUDPEndpoint -LocalPort {} | Select OwningProcess\nand stop it with\n  \
                 Stop-Process -Id <pid> -Force",
                at.port()
            )
        })?;
        listeners.push(s);
    }
    let mut upstream = Vec::with_capacity(PAIRS);
    for _ in 0..PAIRS {
        let any: SocketAddr = if args.server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }
        .parse()
        .expect("a literal address");
        upstream.push(UdpSocket::bind(any)?);
    }
    if let Some(parent) = raw.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let shared = Arc::new(Mutex::new(Shared {
        out: BufWriter::new(File::create(raw)?),
        t0: None,
        client: [None; PAIRS],
        counts: Counts::default(),
        last: None,
        disconnect_seen: false,
        failure: None,
    }));

    println!(
        "proxy listening on {}..{} -> {}..{}",
        args.listen,
        offset(args.listen, PAIRS - 1)?.port(),
        args.server,
        offset(args.server, PAIRS - 1)?.port()
    );
    println!("launch the client with:  -h {}", args.listen);
    println!(
        "writing {}   (Ctrl-C to stop{})",
        raw.display(),
        if args.stop_on_logout {
            "; a clean logout stops it too"
        } else {
            ""
        }
    );

    let mut threads = Vec::new();
    for i in 0..PAIRS {
        let server_at = offset(args.server, i)?;
        for from_client in [true, false] {
            let (recv, send) = if from_client {
                (listeners[i].try_clone()?, upstream[i].try_clone()?)
            } else {
                (upstream[i].try_clone()?, listeners[i].try_clone()?)
            };
            recv.set_read_timeout(Some(Duration::from_millis(100)))?;
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(stop);
            threads.push(std::thread::spawn(move || {
                forward(&recv, &send, from_client, i, server_at, &shared, &stop);
            }));
        }
    }

    let mut shown = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(50));
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let s = shared.lock().unwrap_or_else(PoisonError::into_inner);
        if s.failure.is_some() {
            break;
        }
        if args.stop_on_logout
            && s.disconnect_seen
            && s.last.is_some_and(|t| t.elapsed() >= args.linger)
        {
            println!("\nclean logout: the link has been quiet since its Disconnect");
            break;
        }
        if shown.elapsed() >= Duration::from_secs(2) && s.counts.total() > 0 {
            print!("  {} c2s / {} s2c\r", s.counts.c2s, s.counts.s2c);
            let _ = std::io::stdout().flush();
            shown = Instant::now();
        }
    }
    stop.store(true, Ordering::SeqCst);
    for t in threads {
        let _ = t.join();
    }
    let mut s = shared.lock().unwrap_or_else(PoisonError::into_inner);
    s.out.flush()?;
    if let Some(f) = s.failure.take() {
        return Err(f.into());
    }
    Ok(s.counts)
}

/// `at` with its port moved up by `by`.
fn offset(at: SocketAddr, by: usize) -> Result<SocketAddr, Error> {
    let port = u16::try_from(usize::from(at.port()) + by)
        .map_err(|_| format!("{at}: no port {by} above it"))?;
    let mut out = at;
    out.set_port(port);
    Ok(out)
}

/// One direction of one pair: receive, log, forward, until `stop`.
///
/// The line is written and the datagram forwarded under one lock, so a reply can never be logged
/// ahead of the datagram that caused it.
fn forward(
    recv: &UdpSocket,
    send: &UdpSocket,
    from_client: bool,
    pair: usize,
    server_at: SocketAddr,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) {
    let mut buf = vec![0u8; 65_536];
    while !stop.load(Ordering::SeqCst) {
        let (n, from) = match recv.recv_from(&mut buf) {
            Ok(got) => got,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                // A reset is Windows reporting that an earlier send found no one listening; the
                // socket is still good.
                continue;
            }
            Err(e) => {
                let mut s = shared.lock().unwrap_or_else(PoisonError::into_inner);
                s.failure = Some(format!("receiving on pair {pair}: {e}"));
                stop.store(true, Ordering::SeqCst);
                return;
            }
        };
        if n == 0 {
            continue;
        }
        let data = &buf[..n];
        let now = Instant::now();
        let mut s = shared.lock().unwrap_or_else(PoisonError::into_inner);
        let to = if from_client {
            s.client[pair] = Some(from);
            server_at
        } else {
            match s.client[pair] {
                Some(c) => c,
                // Nothing to deliver it to yet.
                None => continue,
            }
        };
        let t0 = *s.t0.get_or_insert(now);
        let line = line_for(
            pyjson::round(now.duration_since(t0).as_secs_f64(), 6),
            from_client,
            pair,
            data,
        );
        let wrote = writeln!(s.out, "{line}").and_then(|()| s.out.flush());
        if let Err(e) = wrote {
            s.failure = Some(format!("writing the capture: {e}"));
            stop.store(true, Ordering::SeqCst);
            return;
        }
        let _ = send.send_to(data, to);
        if from_client {
            s.counts.c2s += 1;
        } else {
            s.counts.s2c += 1;
        }
        s.last = Some(now);
        if flags_of(data) & PacketFlags::DISCONNECT != 0 {
            s.disconnect_seen = true;
        }
    }
}

/// One capture line, in the shape every recording in the corpus has.
fn line_for(t: f64, c2s: bool, pair: usize, data: &[u8]) -> String {
    format!(
        "{{\"t\": {}, \"dir\": \"{}\", \"pair\": {pair}, \"len\": {}, \"data\": \"{}\"}}",
        pyjson::float_repr(t),
        if c2s { "c2s" } else { "s2c" },
        data.len(),
        crate::raw::encode_hex(data)
    )
}

/// The transport header's flags, or 0 for a datagram too short to have them.
fn flags_of(data: &[u8]) -> u32 {
    data.get(4..8)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// One line of a capture, as the splitter needs it.
#[derive(Debug, Clone)]
struct Row {
    t: f64,
    c2s: bool,
    pair: usize,
    data: Vec<u8>,
}

fn read_rows(path: &Path) -> Result<Vec<Row>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = |what: &str| format!("{} line {}: {what}", path.display(), n + 1);
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| bad(&e.to_string()))?;
        let data = v["data"]
            .as_str()
            .and_then(crate::raw::decode_hex)
            .ok_or_else(|| bad("no hex `data`"))?;
        rows.push(Row {
            t: v["t"].as_f64().ok_or_else(|| bad("no `t`"))?,
            c2s: match v["dir"].as_str() {
                Some("c2s") => true,
                Some("s2c") => false,
                _ => return Err(bad("`dir` is neither c2s nor s2c").into()),
            },
            pair: v["pair"]
                .as_u64()
                .and_then(|p| usize::try_from(p).ok())
                .ok_or_else(|| bad("no `pair`"))?,
            data,
        });
    }
    Ok(rows)
}

/// The logins in a capture, as datagram ranges: one per server `ConnectRequest`, each starting at
/// the last client `LoginRequest` before it and running to the next one's start (the last to the
/// end). Empty when the server answered no login.
fn logins(rows: &[Row]) -> Vec<Range<usize>> {
    let is = |r: &Row, c2s: bool, flag: u32| r.c2s == c2s && flags_of(&r.data) & flag != 0;
    let mut starts: Vec<usize> = Vec::new();
    for (i, _) in rows
        .iter()
        .enumerate()
        .filter(|(_, r)| is(r, false, PacketFlags::CONNECT_REQUEST))
    {
        let Some(start) = (0..i)
            .rev()
            .find(|&j| is(&rows[j], true, PacketFlags::LOGIN_REQUEST))
        else {
            continue;
        };
        if starts.last().is_none_or(|&s| s < start) {
            starts.push(start);
        }
    }
    (0..starts.len())
        .map(|n| starts[n]..starts.get(n + 1).copied().unwrap_or(rows.len()))
        .collect()
}

/// Write `rows`, rebased so the first is at `t = 0`.
fn write_rows(path: &Path, rows: &[Row]) -> Result<(), Error> {
    let t0 = rows.first().map_or(0.0, |r| r.t);
    let mut out = String::new();
    for r in rows {
        out.push_str(&line_for(
            pyjson::round(r.t - t0, 6),
            r.c2s,
            r.pair,
            &r.data,
        ));
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()).into())
}

/// Split the raw capture `raw/<slug>.jsonl` into one recording per login and scrub them into the
/// tracked folder. Returns the names written.
///
/// # Errors
/// When the capture holds no answered login, or the scrub refuses it. Nothing is written into the
/// tracked folder unless the scrub of every part succeeded.
pub fn finish(
    paths: &Paths,
    slug: &str,
    pseudonyms: Option<PathBuf>,
) -> Result<Vec<String>, Error> {
    let raw = paths.raw.join(format!("{slug}.jsonl"));
    let rows = read_rows(&raw)?;
    let parts = logins(&rows);
    if parts.is_empty() {
        return Err(format!(
            "{}: the server answered no login (no ConnectRequest), so there is no session to \
             keep; the capture stays where it is",
            raw.display()
        )
        .into());
    }
    let names: Vec<String> = if parts.len() == 1 {
        vec![slug.to_owned()]
    } else {
        (1..=parts.len()).map(|n| format!("{slug}-{n}")).collect()
    };
    let dropped = parts[0].start;
    if parts.len() > 1 || dropped > 0 {
        let unsplit = paths.raw.join("unsplit");
        std::fs::create_dir_all(&unsplit)?;
        let kept = unsplit.join(format!("{slug}.jsonl"));
        std::fs::rename(&raw, &kept)?;
        for (name, range) in names.iter().zip(&parts) {
            write_rows(
                &paths.raw.join(format!("{name}.jsonl")),
                &rows[range.clone()],
            )?;
        }
        println!("the whole capture is kept as {}", kept.display());
    }
    if dropped > 0 {
        println!("{dropped} datagram(s) before the first answered login were left out");
    }
    for (name, range) in names.iter().zip(&parts) {
        let seg = &rows[range.clone()];
        let c2s = seg.iter().filter(|r| r.c2s).count();
        let clean = seg
            .iter()
            .any(|r| flags_of(&r.data) & PacketFlags::DISCONNECT != 0);
        println!(
            "  {name}: {} datagrams ({c2s} c2s / {} s2c), {:.1} s, {}",
            seg.len(),
            seg.len() - c2s,
            seg.last().map_or(0.0, |r| r.t) - seg.first().map_or(0.0, |r| r.t),
            if clean {
                "clean disconnect"
            } else {
                "no Disconnect: `corpus` lists it with the recordings that do not end cleanly"
            }
        );
    }

    let scrub = crate::Args {
        raw_dir: paths.raw.clone(),
        out: paths.out.clone(),
        map_dir: paths.map.clone(),
        pseudonyms: crate::default_pseudonyms(&paths.repo, pseudonyms)?,
        extras: crate::default_extras(&paths.repo),
        sessions: names.clone(),
        fresh: false,
        dry_run: false,
    };
    crate::scrub(&scrub)?;
    println!(
        "\nwrote {} -- now run `cargo run -p dereth-corpus --release -- corpus` to regenerate \
         the message corpus and indexes, and describe the recording in manifest.json",
        names
            .iter()
            .map(|n| format!("{PACKET_CAPTURES}/{n}.jsonl"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slug rule: two to four lower-case kebab words, starting with a letter, not ending in a
    /// number.
    #[test]
    fn slugs_are_two_to_four_lower_case_kebab_words() {
        for good in [
            "long-solo-play",
            "ddd-interrogation-only",
            "a-b",
            "vendor-buy-sell-2x",
        ] {
            assert_eq!(slug_problem(good), None, "{good}");
        }
        for bad in [
            "solo",
            "one-two-three-four-five",
            "Long-solo",
            "long_solo",
            "long--solo",
            "-long-solo",
            "2-long",
            "long-solo-2",
            "long solo",
        ] {
            assert!(slug_problem(bad).is_some(), "{bad}");
        }
    }

    /// A split part belongs to its slug, and a longer slug that merely starts the same way does
    /// not.
    #[test]
    fn a_part_is_the_slug_and_a_number() {
        assert!(is_part_of("house-trade", "house-trade"));
        assert!(is_part_of("house-trade-2", "house-trade"));
        assert!(!is_part_of("house-trade-long", "house-trade"));
        assert!(!is_part_of("house-trade-", "house-trade"));
    }

    fn row(c2s: bool, flags: u32) -> Row {
        let mut data = vec![0u8; 20];
        data[4..8].copy_from_slice(&flags.to_le_bytes());
        Row {
            t: 0.0,
            c2s,
            pair: 0,
            data,
        }
    }

    /// A login starts at the last resend before its answer, and the next starts where it ends;
    /// an unanswered resend inside a session stays in it.
    #[test]
    fn a_login_runs_from_its_answered_request_to_the_next() {
        let login = PacketFlags::LOGIN_REQUEST;
        let connect = PacketFlags::CONNECT_REQUEST;
        let rows = vec![
            row(true, login),    // 0: a resend nobody answered
            row(true, login),    // 1: the one that was
            row(false, connect), // 2
            row(true, 0),        // 3
            row(true, login),    // 4: an unanswered login inside the session
            row(true, 0x8000),   // 5: the logout
            row(true, login),    // 6: the second login
            row(false, connect), // 7
            row(false, 0),       // 8
        ];
        // The unanswered login at 4 is not the second session's start: 6 is the last before 7.
        assert_eq!(logins(&rows), vec![1..6, 6..9]);
        assert!(logins(&rows[..2]).is_empty());
    }
}
