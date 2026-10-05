//! The corpus tool: records sessions through a logging proxy, pseudonymises them before they are
//! kept, and generates the message corpus and capture index from them.
//!
//! **Depends on** the transport (`dereth-transport`), the codecs
//! (`dereth-protocol`) and the client's capture harness (`dereth-client-net`). **Used by** nothing:
//! it is a tool, and its tests keep the committed corpus equal to what the recordings generate
//! (`dereth-corpus corpus --check`).
//!
//! **Must never** re-encode a message through our own codecs: every byte of every output datagram
//! is a byte of the input datagram except those inside a substituted name and the four of the
//! checksum. That is the difference between publishing a recording and publishing our decoder's
//! opinion of one. The real-to-stand-in map never leaves the machine, and nothing unscrubbed is
//! ever written into the tracked folder.
//!
//! ```text
//! cargo run -p dereth-corpus --release -- record <slug> [--repo DIR] [--listen HOST:PORT]
//!                                            [--server HOST:PORT] [--linger SECS]
//!                                            [--keep-going] [--pseudonyms FILE]
//! cargo run -p dereth-corpus --release -- scrub [--repo DIR] [--raw-dir DIR] [--out DIR]
//!                                            [--map-dir DIR] [--pseudonyms FILE]
//!                                            [--extras FILE] [--sessions a,b,c] [--fresh]
//!                                            [--dry-run]
//! cargo run -p dereth-corpus --release -- corpus [--repo DIR] [--out DIR] [--check]
//! ```
//!
//! `record` (the [`record`] module) proxies a session between the retail client and a server
//! into the untracked `fixtures/packet-captures/raw/`, splits it into one file per login, and
//! runs `scrub` on the parts.
//!
//! `scrub` reads the recordings under `fixtures/packet-captures/raw/`, decodes **only** the four
//! messages that name an identity, gives each identity a stand-in of exactly the same byte length
//! (from the private pseudonym dictionary when the checkout has one, generated otherwise, and the
//! same stand-in an earlier run gave it), replaces every occurrence in reassembled blobs (so a name
//! cut in half by the fragmenter is still found), recomputes each datagram's checksum from the
//! key-stream seeds the recording carries in clear, and writes
//! `fixtures/packet-captures/<slug>.jsonl`. The map goes to the gitignored
//! `fixtures/packet-captures/scrub-map/`, because it is the only thing that could undo the scrub.
//!
//! `corpus` (the [`corpus`] module) regenerates the message corpus, the capture index and the
//! manifest's generated fields from whatever recordings the folder holds.
//!
//! The scrub is possible because the protocol has no confidentiality: only the payload half of
//! the checksum is XORed with one key-stream draw, and the two seeds travel in the connect request
//! in clear. A same-length substitution with recomputed checksums is byte for byte what the client
//! would have sent had the account been called something else, which is why the corpus gates are
//! its acceptance test.

mod checksum;
mod corpus;
mod field_dump;
mod identities;
mod pseudonyms;
mod pyjson;
mod raw;
mod record;
mod regions;
mod substitute;
mod walk;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use identities::{Found, IdKind};
use pseudonyms::{Assignment, Book};
use raw::Recording;
use substitute::Matcher;

fn main() -> std::process::ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dereth-corpus: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The parsed `scrub` command line.
#[derive(Debug)]
struct Args {
    /// Where the recordings are read from. `fixtures/packet-captures/raw` under `--repo` unless
    /// overridden -- the untracked directory `record` writes into, one login per file.
    raw_dir: PathBuf,
    out: PathBuf,
    map_dir: PathBuf,
    /// The stand-in dictionary; `None` when the checkout has none, and every character stand-in
    /// is then a generated filler.
    pseudonyms: Option<PathBuf>,
    extras: Option<PathBuf>,
    /// The recordings to scrub, by slug: every `*.jsonl` directly in `raw_dir` unless
    /// `--sessions` names some. Their order is the order stand-ins are handed out in.
    sessions: Vec<String>,
    /// Ignore the stand-ins an earlier run handed out (the map's `_corpus.json`) and number
    /// every identity afresh.
    fresh: bool,
    dry_run: bool,
}

fn run(argv: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match argv.first().map(String::as_str) {
        Some("scrub") => {
            let args = parse(&argv[1..])?;
            scrub(&args)
        }
        Some("field-dump") => {
            let out = match &argv[1..] {
                [] => None,
                [flag, file] if flag == "--out" => Some(PathBuf::from(file)),
                _ => return Err("usage: field-dump [--out FILE]".into()),
            };
            field_dump::run(out);
            Ok(())
        }
        Some("corpus") => corpus::run(&parse_corpus(&argv[1..])?),
        Some("record") => record::run(&parse_record(&argv[1..])?),
        _ => {
            eprintln!("{USAGE}");
            Err("expected `record`, `scrub`, `corpus` or `field-dump`".into())
        }
    }
}

const USAGE: &str = "\
usage: dereth-corpus <command> [options]

commands:
  record <slug> [--listen HOST:PORT] [--server HOST:PORT] [--linger SECS] [--keep-going]
                [--pseudonyms FILE] [--repo DIR]
        proxy a session between the retail client and a server (defaults: listen on
        127.0.0.1:9100 and 9101, forward to 127.0.0.1:9000 and 9001; launch the client with
        -h 127.0.0.1:9100). Stops on Ctrl-C, or once the link is quiet for --linger seconds
        (3) after a clean logout unless --keep-going. The raw capture goes to the untracked
        fixtures/packet-captures/raw/; it is then split into one file per login and
        pseudonymised into fixtures/packet-captures/<slug>.jsonl (or <slug>-1, <slug>-2, ...).
        A slug is lower-case kebab-case, two to four words, not ending in a number.
  scrub [--raw-dir DIR] [--out DIR] [--map-dir DIR] [--pseudonyms FILE] [--extras FILE]
        [--sessions a,b,c] [--fresh] [--dry-run] [--repo DIR]
        pseudonymise the raw recordings (every raw/*.jsonl unless --sessions names some) into
        fixtures/packet-captures/; the real-to-stand-in map goes to the untracked scrub-map/,
        and stand-ins it already holds are reused unless --fresh
  corpus [--check] [--out DIR] [--repo DIR]
        regenerate fixtures/message-corpus/, the capture index and manifest.json from every
        recording in fixtures/packet-captures/; --check compares instead of writing
  field-dump [--out FILE]
        decode every message of the recordings' corpus and write each one's fields, its decode
        outcome and any bytes left over to FILE (default: corpus_census.txt in cargo's target
        directory)

--repo defaults to the workspace holding fixtures/ at or above the current directory.";

/// `record <slug> [...]`: see [`record`].
fn parse_record(argv: &[String]) -> Result<record::Args, Box<dyn std::error::Error>> {
    let mut slug: Option<String> = None;
    let mut repo: Option<PathBuf> = None;
    let mut listen = record::DEFAULT_LISTEN.to_owned();
    let mut server = record::DEFAULT_SERVER.to_owned();
    let mut linger = record::DEFAULT_LINGER;
    let mut stop_on_logout = true;
    let mut pseudonyms: Option<PathBuf> = None;
    let mut i = 0;
    while i < argv.len() {
        let next = |i: usize| -> Result<String, String> {
            argv.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{} wants a value", argv[i]))
        };
        match argv[i].as_str() {
            "--repo" => {
                repo = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--listen" => {
                listen = next(i)?;
                i += 2;
            }
            "--server" => {
                server = next(i)?;
                i += 2;
            }
            "--linger" => {
                let secs: f64 = next(i)?
                    .parse()
                    .map_err(|e| format!("--linger wants seconds: {e}"))?;
                linger = std::time::Duration::try_from_secs_f64(secs)
                    .map_err(|e| format!("--linger: {e}"))?;
                i += 2;
            }
            "--keep-going" => {
                stop_on_logout = false;
                i += 1;
            }
            "--pseudonyms" => {
                pseudonyms = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            other if !other.starts_with("--") && slug.is_none() => {
                slug = Some(other.to_owned());
                i += 1;
            }
            other => return Err(format!("unexpected argument {other:?}").into()),
        }
    }
    let slug = slug.ok_or("record wants a slug: `dereth-corpus record <slug>`")?;
    let addr = |what: &str, v: &str| -> Result<std::net::SocketAddr, String> {
        v.parse()
            .map_err(|e| format!("{what} {v:?} is not HOST:PORT: {e}"))
    };
    Ok(record::Args {
        listen: addr("--listen", &listen)?,
        server: addr("--server", &server)?,
        repo: repo.map_or_else(find_repo, Ok)?,
        slug,
        stop_on_logout,
        linger,
        pseudonyms,
    })
}

/// `corpus [--repo DIR] [--out DIR] [--check]`: see [`corpus`].
fn parse_corpus(argv: &[String]) -> Result<corpus::Args, Box<dyn std::error::Error>> {
    let mut repo: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut check = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            flag @ ("--repo" | "--out") => {
                let v = PathBuf::from(
                    argv.get(i + 1)
                        .ok_or_else(|| format!("{flag} wants a value"))?,
                );
                if flag == "--repo" {
                    repo = Some(v);
                } else {
                    out = Some(v);
                }
                i += 2;
            }
            "--check" => {
                check = true;
                i += 1;
            }
            other => return Err(format!("unexpected argument {other:?}").into()),
        }
    }
    let repo = repo.map_or_else(find_repo, Ok)?;
    Ok(corpus::Args {
        out: out.unwrap_or_else(|| repo.clone()),
        repo,
        check,
    })
}

fn parse(argv: &[String]) -> Result<Args, Box<dyn std::error::Error>> {
    let mut repo: Option<PathBuf> = None;
    let mut raw_dir: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut map_dir: Option<PathBuf> = None;
    let mut pseudonyms: Option<PathBuf> = None;
    let mut extras: Option<PathBuf> = None;
    let mut sessions: Vec<String> = Vec::new();
    let mut fresh = false;
    let mut dry_run = false;
    let mut i = 0;
    while i < argv.len() {
        let next = |i: usize| -> Result<String, String> {
            argv.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{} wants a value", argv[i]))
        };
        match argv[i].as_str() {
            "--repo" => {
                repo = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--raw-dir" => {
                raw_dir = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--map-dir" => {
                map_dir = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--pseudonyms" => {
                pseudonyms = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--extras" => {
                extras = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--sessions" => {
                sessions = next(i)?.split(',').map(str::to_owned).collect();
                i += 2;
            }
            "--fresh" => {
                fresh = true;
                i += 1;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            other => return Err(format!("unexpected argument {other:?}").into()),
        }
    }
    let repo = repo.map_or_else(find_repo, Ok)?;
    let raw_dir = raw_dir.unwrap_or_else(|| repo.join(RAW));
    let sessions = if sessions.is_empty() {
        corpus::recordings_in(&raw_dir)?
    } else {
        sessions
    };
    if sessions.is_empty() {
        return Err(format!("{}: no recordings to scrub", raw_dir.display()).into());
    }
    Ok(Args {
        out: out.unwrap_or_else(|| repo.join(corpus::PACKET_CAPTURES)),
        map_dir: map_dir.unwrap_or_else(|| repo.join(SCRUB_MAP)),
        pseudonyms: default_pseudonyms(&repo, pseudonyms)?,
        extras: extras.or_else(|| default_extras(&repo)),
        raw_dir,
        sessions,
        fresh,
        dry_run,
    })
}

/// The untracked directory unscrubbed recordings are kept in, relative to the checkout.
const RAW: &str = "fixtures/packet-captures/raw";
/// The untracked directory the real-to-stand-in map is kept in, relative to the checkout.
const SCRUB_MAP: &str = "fixtures/packet-captures/scrub-map";

/// The stand-in dictionary: the one named, which must exist, else the checkout's
/// `pseudonyms.json` when it has one, else none (generated fillers).
fn default_pseudonyms(
    repo: &Path,
    named: Option<PathBuf>,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if let Some(p) = named {
        if !p.is_file() {
            return Err(format!("{}: no such pseudonym dictionary", p.display()).into());
        }
        return Ok(Some(p));
    }
    let p = repo.join(corpus::PACKET_CAPTURES).join("pseudonyms.json");
    Ok(p.is_file().then_some(p))
}

/// The manual extras file in the map directory, when there is one.
fn default_extras(repo: &Path) -> Option<PathBuf> {
    let p = repo.join(SCRUB_MAP).join("extras.json");
    p.is_file().then_some(p)
}

/// Walk up from the current directory for the workspace holding
/// `fixtures/packet-captures/index.json`.
///
/// Not `fixtures/packet-captures/raw`: that directory is untracked and gitignored, so a fresh
/// checkout does not have one until `record` writes into it, and a missing input should be
/// reported as the missing recording it is rather than as a missing checkout.
fn find_repo() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir
            .join(corpus::PACKET_CAPTURES)
            .join("index.json")
            .is_file()
        {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(
                "no checkout with fixtures/packet-captures/index.json above the current \
                 directory; pass --repo"
                    .into(),
            );
        }
    }
}

/// What one recording's scrub produced. Counts only — never a value.
#[derive(Debug, Default)]
struct SessionCounts {
    datagrams: usize,
    blobs: usize,
    incomplete_blobs: usize,
    duplicate_fragments: usize,
    /// Accepted substitution sites, by kind.
    sites: BTreeMap<&'static str, usize>,
    /// Sites rejected because the match sat inside a longer alphanumeric word, by kind.
    rejected_boundary: BTreeMap<&'static str, usize>,
    /// Sites rejected by the short-name guard, by kind.
    rejected_short: BTreeMap<&'static str, usize>,
    /// Rejected sites by the identity's byte length, which is what the guards key on.
    rejected_by_len: BTreeMap<usize, usize>,
    /// Sites found in a UTF-16LE encoding rather than ASCII.
    utf16_sites: usize,
    /// Accepted sites by direction: client to server, then server to client.
    sites_c2s: usize,
    sites_s2c: usize,
    /// Accepted sites in a field the protocol *declares* to be an identity: the `LoginRequest`
    /// section and the five identity-bearing opcodes discovery reads.
    sites_declared: usize,
    /// Accepted sites anywhere else -- chat, tells, the friends list, an allegiance roster, an
    /// `@`-command echo, a `0xF7B0` event body. These are the ones a field-by-field redactor
    /// would have missed, and they are the majority.
    sites_in_text: usize,
    /// The datagram the `ConnectRequest` -- and so the two ISAAC seeds -- arrived in.
    connect_request_at: usize,
    datagrams_changed: usize,
    checksums_rewritten: usize,
    verified: usize,
    parked: usize,
    reused: usize,
    discovered: BTreeMap<&'static str, usize>,
}

fn scrub(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut book = match &args.pseudonyms {
        Some(p) => Book::load(p)?,
        None => {
            eprintln!(
                "no pseudonym dictionary in this checkout: character stand-ins are generated \
                 fillers"
            );
            Book::fillers_only()
        }
    };
    let prior = if args.fresh {
        Vec::new()
    } else {
        read_corpus_map(&args.map_dir.join("_corpus.json"))?
    };
    if !prior.is_empty() {
        eprintln!(
            "{} stand-in(s) from an earlier run are reused (--fresh ignores them)",
            prior.len()
        );
    }
    book.remember(&prior);

    // Phase one: read every recording and discover its identities. The map is built over the whole
    // corpus before a single byte is substituted, so a name first seen in `first-login-walk-jump`
    // is scrubbed out of `long-solo-play`'s chat as well.
    let mut recordings: Vec<Recording> = Vec::new();
    let mut found: Vec<Found> = Vec::new();
    // One identity, one stand-in, however many recordings it turns up in -- and however it is
    // cased. The client lower-cases the account before it packs the `LoginRequest` while the
    // server echoes it back in `LoginCharacterSet` with the case it stored, so the same account
    // arrives twice per session in two spellings; without this, each spelling in each recording
    // would draw its own ordinal and the corpus would read as seventeen accounts instead of
    // three.
    let mut global: std::collections::BTreeSet<(IdKind, String)> =
        std::collections::BTreeSet::new();
    let mut per_session_found: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut counts: BTreeMap<String, SessionCounts> = BTreeMap::new();
    let mut built: Vec<regions::Regions> = Vec::new();

    for name in &args.sessions {
        let path = args.raw_dir.join(format!("{name}.jsonl"));
        let rec = Recording::read(&path)?;
        let regs = regions::build(&rec)?;
        let d = identities::discover(name, &regs.regions);
        if !d.decode_failures.is_empty() {
            for (op, what) in &d.decode_failures {
                eprintln!("  {name}: opcode {op:#06X} did not decode: {what}");
            }
            return Err(format!(
                "{name}: {} identity-bearing message(s) did not decode; an identity that cannot be \
                 read is an identity that cannot be removed",
                d.decode_failures.len()
            )
            .into());
        }
        let mut c = SessionCounts {
            datagrams: rec.datagrams.len(),
            blobs: regs
                .regions
                .iter()
                .filter(|r| matches!(r.kind, regions::Kind::Blob { .. }))
                .count(),
            incomplete_blobs: regs.incomplete_blobs,
            duplicate_fragments: regs.duplicate_fragments,
            ..SessionCounts::default()
        };
        for f in &d.found {
            *c.discovered.entry(f.kind.as_str()).or_default() += 1;
            if !global.insert((f.kind, f.value.to_ascii_lowercase())) {
                continue;
            }
            per_session_found
                .entry(name.clone())
                .or_default()
                .push(found.len());
            found.push(f.clone());
        }
        if d.unread_secrets > 0 {
            return Err(format!(
                "{name}: {} of {} LoginRequest section(s) carry extra data this pass cannot \
                 read, so a password would be published; add the shape to \
                 `identities::extra_string` rather than publishing the recording",
                d.unread_secrets, d.login_requests
            )
            .into());
        }
        if d.create_object_only > 0 {
            eprintln!(
                "  {name}: {} character name(s) came from a create-object that the character set \
                 did not name",
                d.create_object_only
            );
        }
        counts.insert(name.clone(), c);
        recordings.push(rec);
        built.push(regs);
    }

    if let Some(path) = &args.extras {
        let extras = read_extras(path)?;
        let before = found.len();
        identities::add_extras(&mut found, &extras);
        if found.len() > before {
            eprintln!(
                "  extras: {} additional identity(ies) from {}",
                found.len() - before,
                path.display()
            );
        }
    }

    // Phase two: hand out the stand-ins, corpus-wide, in first-appearance order.
    let mut assignments: Vec<Assignment> = Vec::new();
    for f in &found {
        if let Some(a) = book.assign(f) {
            assignments.push(a);
        }
    }
    let pairs: Vec<(String, String, usize)> = assignments
        .iter()
        .enumerate()
        .flat_map(|(i, a)| {
            pseudonyms::variants(a)
                .into_iter()
                .map(move |(p, r)| (p, r, i))
        })
        .collect();
    let matcher = Matcher::new(&pairs);
    eprintln!(
        "{} identity(ies) -> {} pattern(s) (ASCII and UTF-16LE, four case shapes)",
        assignments.len(),
        matcher.len()
    );
    if matcher.is_empty() {
        return Err("no identities were discovered; nothing would be scrubbed".into());
    }

    // Phase three: substitute, re-checksum, write.
    for ((name, mut rec), regs) in args
        .sessions
        .iter()
        .cloned()
        .zip(recordings)
        .zip(built.iter())
    {
        debug_assert_eq!(
            rec.name, name,
            "the recordings are in the order they were read"
        );
        let c = counts.get_mut(&name).expect("counted in phase one");
        let seeds = checksum::seeds(&rec)?;
        c.connect_request_at = seeds.at;
        let plan = checksum::plan(&rec, seeds)?;
        c.verified = plan.verified;
        c.parked = plan.parked;
        c.reused = plan.reused;

        let mut edits: Vec<(usize, usize, Vec<u8>)> = Vec::new();
        for region in &regs.regions {
            let (dir, declared) = match region.kind {
                regions::Kind::Blob { dir, opcode, .. } => {
                    (dir, DECLARED_OPCODES.contains(&opcode))
                }
                regions::Kind::Section { dir, mask, .. } => (
                    dir,
                    mask == dereth_transport::wire::PacketFlags::LOGIN_REQUEST,
                ),
            };
            let mut here = 0usize;
            let hits = matcher.scan(&region.buf, |at, p| {
                here += 1;
                if p.stride == 2 {
                    c.utf16_sites += 1;
                }
                for (dgi, pos) in region.disk_positions(at) {
                    edits.push((dgi, pos, p.replace.clone()));
                }
            });
            match dir {
                raw::Dir::C2s => c.sites_c2s += here,
                raw::Dir::S2c => c.sites_s2c += here,
            }
            if declared {
                c.sites_declared += here;
            } else {
                c.sites_in_text += here;
            }
            for (owner, h) in hits.iter().enumerate() {
                let a = &assignments[owner];
                *c.sites.entry(a.kind.as_str()).or_default() += h.applied;
                *c.rejected_boundary.entry(a.kind.as_str()).or_default() += h.rejected_boundary;
                *c.rejected_short.entry(a.kind.as_str()).or_default() += h.rejected_short;
                if h.rejected() > 0 {
                    *c.rejected_by_len.entry(a.real.len()).or_default() += h.rejected();
                }
            }
        }
        for (dgi, pos, bytes) in &edits {
            rec.datagrams[*dgi].bytes[*pos..*pos + bytes.len()].copy_from_slice(bytes);
        }
        c.datagrams_changed = {
            let mut set = std::collections::BTreeSet::new();
            for (dgi, _, _) in &edits {
                set.insert(*dgi);
            }
            set.len()
        };
        c.checksums_rewritten = checksum::rewrite(&mut rec.datagrams, &plan)?;

        // Nothing is published that the transport cannot read back: every scrubbed datagram is
        // re-parsed and re-checksummed against the same key stream before the file is written.
        let replan = checksum::plan(&rec, seeds)?;
        if replan.verified != plan.verified {
            return Err(format!(
                "{name}: {} datagrams verified before the scrub and {} after",
                plan.verified, replan.verified
            )
            .into());
        }

        if !args.dry_run {
            rec.write(&args.out.join(format!("{name}.jsonl")))?;
            write_map(args, &name, &assignments, &per_session_found, c)?;
        }
        println!("{}", line_for(&name, c));
    }

    if !args.dry_run {
        let known: std::collections::BTreeSet<(IdKind, String)> = prior
            .iter()
            .map(|a| (a.kind, a.real.to_ascii_lowercase()))
            .collect();
        let mut all = prior.clone();
        all.extend(
            assignments
                .iter()
                .filter(|a| !known.contains(&(a.kind, a.real.to_ascii_lowercase())))
                .cloned(),
        );
        write_corpus_map(args, &all)?;
        eprintln!(
            "scrubbed corpus -> {}\nprivate map -> {} (gitignored)",
            args.out.display(),
            args.map_dir.display()
        );
    }
    Ok(())
}

/// The opcodes whose bodies *declare* an identity, i.e. the ones `identities::discover` reads.
/// A substitution anywhere else is an in-text occurrence -- the kind a field-by-field redactor
/// cannot reach.
const DECLARED_OPCODES: [u32; 5] = [0xF658, 0xF657, 0xF655, 0xF745, 0xF7DB];

/// One public result line: counts and nothing else.
fn line_for(name: &str, c: &SessionCounts) -> String {
    let n = |m: &BTreeMap<&'static str, usize>, k: &str| m.get(k).copied().unwrap_or(0);
    format!(
        "{name:<12} datagrams {:>6} | blobs {:>6} | sites acct {:>4} pass {:>2} char {:>5} \
         extra {:>4} | c2s {:>4} s2c {:>5} | declared {:>4} in-text {:>5} | rejected bdry {:>4} \
         short {:>4} | utf16 {:>3} | datagrams changed {:>5} | checksums rewritten {:>5} | \
         verified {:>6} | parked {:>4} | reused {:>4}",
        c.datagrams,
        c.blobs,
        n(&c.sites, "account"),
        n(&c.sites, "password"),
        n(&c.sites, "character"),
        n(&c.sites, "extra"),
        c.sites_c2s,
        c.sites_s2c,
        c.sites_declared,
        c.sites_in_text,
        c.rejected_boundary.values().sum::<usize>(),
        c.rejected_short.values().sum::<usize>(),
        c.utf16_sites,
        c.datagrams_changed,
        c.checksums_rewritten,
        c.verified,
        c.parked,
        c.reused,
    )
}

/// The manual extras file: `{"accounts": [...], "passwords": [...], "characters": [...]}`.
fn read_extras(path: &Path) -> Result<BTreeMap<IdKind, Vec<String>>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let mut out = BTreeMap::new();
    for (key, kind) in [
        ("accounts", IdKind::Account),
        ("passwords", IdKind::Password),
        ("characters", IdKind::Character),
        ("other", IdKind::Extra),
    ] {
        let list: Vec<String> = v
            .get(key)
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if !list.is_empty() {
            out.insert(kind, list);
        }
    }
    Ok(out)
}

/// The private per-session map. **Gitignored**: it is the only thing that could reverse the scrub.
fn write_map(
    args: &Args,
    session: &str,
    assignments: &[Assignment],
    per_session: &BTreeMap<String, Vec<usize>>,
    c: &SessionCounts,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows: Vec<serde_json::Value> = per_session
        .get(session)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|i| assignments.get(*i))
        .map(row)
        .collect();
    let doc = serde_json::json!({
        "_about": "PRIVATE. The real-to-pseudonym map for one recording, written by \
                   `dereth-corpus scrub`. This directory is gitignored and must stay that way: it \
                   is the only artefact that can undo the scrub.",
        "session": session,
        "first_seen_here": rows,
        "counts": {
            "datagrams": c.datagrams,
            "blobs": c.blobs,
            "incomplete_blobs": c.incomplete_blobs,
            "duplicate_fragments": c.duplicate_fragments,
            "sites": c.sites,
            "sites_client_to_server": c.sites_c2s,
            "sites_in_a_declared_identity_field": c.sites_declared,
            "sites_in_text": c.sites_in_text,
            "sites_server_to_client": c.sites_s2c,
            "connect_request_datagram": c.connect_request_at,
            "rejected_word_boundary": c.rejected_boundary,
            "rejected_short_name_guard": c.rejected_short,
            "rejected_by_identity_length": c.rejected_by_len
                .iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>(),
            "utf16_sites": c.utf16_sites,
            "datagrams_changed": c.datagrams_changed,
            "checksums_rewritten": c.checksums_rewritten,
            "encrypted_datagrams_verified": c.verified,
            "keys_parked": c.parked,
            "keys_reused": c.reused,
            "discovered": c.discovered,
        },
    });
    write_json(&args.map_dir.join(format!("{session}.json")), &doc)
}

/// The whole assignment table, once.
fn write_corpus_map(
    args: &Args,
    assignments: &[Assignment],
) -> Result<(), Box<dyn std::error::Error>> {
    let doc = serde_json::json!({
        "_about": "PRIVATE. Every real-to-pseudonym assignment in the corpus, in the order they \
                   were handed out. Gitignored. The next run reuses these stand-ins, so a \
                   recording added later names the same people the same way.",
        "assignments": assignments.iter().map(row).collect::<Vec<_>>(),
    });
    write_json(&args.map_dir.join("_corpus.json"), &doc)
}

/// The assignment table an earlier run wrote, or nothing when there is none.
fn read_corpus_map(path: &Path) -> Result<Vec<Assignment>, Box<dyn std::error::Error>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display()).into()),
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    for r in v["assignments"].as_array().into_iter().flatten() {
        let (Some(kind), Some(real), Some(pseudonym)) = (
            r["kind"].as_str().and_then(IdKind::parse),
            r["real"].as_str(),
            r["pseudonym"].as_str(),
        ) else {
            return Err(
                format!("{}: a row without kind, real and pseudonym", path.display()).into(),
            );
        };
        if real.len() != pseudonym.len() {
            return Err(format!("{}: a stand-in of the wrong length", path.display()).into());
        }
        out.push(Assignment {
            kind,
            real: real.to_owned(),
            pseudonym: pseudonym.to_owned(),
            session: r["first_session"].as_str().unwrap_or_default().to_owned(),
            datagram: r["first_datagram"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or_default(),
            source: r["source"].as_str().unwrap_or_default().to_owned(),
            generated: r["generated"].as_bool().unwrap_or_default(),
        });
    }
    Ok(out)
}

fn row(a: &Assignment) -> serde_json::Value {
    serde_json::json!({
        "kind": a.kind.as_str(),
        "real": a.real,
        "pseudonym": a.pseudonym,
        "len": a.real.len(),
        "first_session": a.session,
        "first_datagram": a.datagram,
        "source": a.source,
        "generated": a.generated,
    })
}

fn write_json(path: &Path, doc: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(doc)?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}
