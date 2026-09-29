//! `dereth-corpus corpus` -- the derived artefacts of the packet captures.
//!
//! The recordings are found, not listed: every `fixtures/packet-captures/<slug>.jsonl` is one, in
//! slug order. From them this writes:
//!
//! * `fixtures/message-corpus/<slug>/blobs.jsonl` -- every reassembled message of the recording,
//!   both directions, in the order each one's last fragment arrived, one JSON object per line:
//!   `{idx, dir, t_rel, queue, blob_id, opcode, payload_hex}`;
//! * `fixtures/message-corpus/index.json` -- per recording, the blob count each way, the
//!   duplicate datagrams skipped and the client-to-server opcode census;
//! * `fixtures/packet-captures/index.json` -- per recording, the datagram census and the checks
//!   every recording passed, and the recordings that end without a `Disconnect`;
//! * `fixtures/packet-captures/manifest.json` -- per recording, the message census in the three
//!   opcode spaces and the keys it was the first to carry, beside the fields a person writes (what
//!   was done in it, when, and the slices tests cite), which are carried over from the committed
//!   file untouched.
//!
//! The committed copies are a **checked cache**: `cargo test -p dereth-corpus` regenerates all of
//! them into a temporary directory and compares them byte for byte with the tree (see the
//! `committed_files_are_what_the_generator_writes` test below), so a recording and the corpus
//! derived from it cannot drift apart silently. Adding a recording is: put the file in the folder
//! (`dereth-corpus record` does), run this, and write the new manifest entry's `actions` and
//! `date`, which this reports as missing.
//!
//! # The reassembly is the transport's
//!
//! Each datagram is taken apart by `dereth_transport::wire::ParsedPacket::parse` and each message put
//! together by `dereth_transport::blob::NetBlob` -- the code the client runs. One rule is applied on top,
//! the one `dereth-client-net`'s sequence gate applies: a fragment-carrying datagram whose (direction,
//! sequence number) has already been delivered is a duplicate delivery and is skipped, counted as
//! `duplicate_datagrams`. `fellowship-one-vassal` is the recording that needs it: the proxy
//! forwarded both the original and the resend of server datagram 74, and without the rule its
//! server-to-client count is 764 where the client's own is 763. The client crate's own reading of
//! the same datagrams is held to these counts by the independent reassembler in this crate's
//! `corpus_matches_raw_recordings` test.
//!
//! # What every recording must pass
//!
//! It starts at `t = 0`; every datagram parses (so its header size agrees with its payload and its
//! `EncryptedChecksum` bit is set exactly when a fragment or a non-disposable optional header is
//! present); and it is one connection -- exactly one `ConnectRequest`, answering the first
//! `LoginRequest`. A recording that also carries a `Disconnect` is **locked**: it must reassemble
//! completely, and it is what the message corpus and both indexes describe. One without a
//! `Disconnect` (the two deliberate unclean logouts) is named in the capture index's
//! `without_disconnect` list and nowhere else, which is what lets the tests that want only clean
//! sessions find the others without a list of their own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_client_net::replay::{Capture, Direction};
use dereth_transport::blob::NetBlob;
use dereth_transport::wire::{PacketFlags, ParsedPacket};

use crate::pyjson::{self, Value};

/// Where the recordings live, relative to the checkout.
pub const PACKET_CAPTURES: &str = "fixtures/packet-captures";
/// Where the message corpus lives, relative to the checkout.
pub const MESSAGE_CORPUS: &str = "fixtures/message-corpus";

/// The ordered game-action envelope, client to server; its sub-type is the dword at offset 8.
const GAME_ACTION: u32 = 0xF7B1;
/// The ordered game-event envelope, server to client; its sub-type is the dword at offset 12.
const GAME_EVENT: u32 = 0xF7B0;

/// The slug rule, as the manifest states it and `dereth-corpus record` enforces it.
pub const SLUG_RULE: &str = "Lower-case kebab-case, two to four words, naming what distinguishes \
     the recording; the last word is not a number, because a recording that holds several logins \
     is kept as `<slug>-1`, `<slug>-2`, ... Unique, and stable once assigned. The slug is the \
     recording's only name: its file is fixtures/packet-captures/<slug>.jsonl and every index and \
     loader keys on it.";

/// The manifest keys this generator owns in each session entry. Every other key in an entry is
/// written by a person and carried over from the committed file as it stands.
const MANIFEST_GENERATED: [&str; 13] = [
    "blobs",
    "blobs_client_to_server",
    "blobs_server_to_client",
    "bytes",
    "datagrams",
    "datagrams_client_to_server",
    "datagrams_server_to_client",
    "duration_s",
    "first_contact",
    "fragment_packets",
    "opcode_census",
    "slug",
    "unanswered_login_requests",
];

/// One reassembled message.
#[derive(Debug)]
struct Blob {
    dir: Direction,
    t_rel: f64,
    queue: u16,
    blob_id: u64,
    payload: Vec<u8>,
}

impl Blob {
    fn opcode(&self) -> u32 {
        self.dword(0).unwrap_or(0)
    }

    fn dword(&self, at: usize) -> Option<u32> {
        let b = self.payload.get(at..at + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The message's key in the three disjoint opcode spaces: `c2s/action/XXXX` for a game
    /// action, `s2c/event/XXXX` for a game event, `<dir>/bare/XXXX` for everything else.
    fn space_key(&self) -> String {
        let dir = dir_str(self.dir);
        let op = self.opcode();
        let wrapped = match self.dir {
            Direction::C2s if op == GAME_ACTION => self.dword(8).map(|s| ("action", s)),
            Direction::S2c if op == GAME_EVENT => self.dword(12).map(|s| ("event", s)),
            _ => None,
        };
        match wrapped {
            Some((space, sub)) => format!("{dir}/{space}/{sub:04X}"),
            None => format!("{dir}/bare/{op:04X}"),
        }
    }
}

const fn dir_str(d: Direction) -> &'static str {
    match d {
        Direction::C2s => "c2s",
        Direction::S2c => "s2c",
    }
}

/// What one recording's checks measured.
#[derive(Debug)]
struct Summary {
    datagrams: usize,
    client_to_server: usize,
    bytes: usize,
    duration_s: f64,
    fragment_packets: usize,
    unanswered_login_requests: usize,
    /// Whether it carries a `Disconnect`, i.e. whether it is locked.
    clean: bool,
}

type Error = Box<dyn std::error::Error>;

/// Every recording in `dir`: the stems of the `*.jsonl` files directly in it, sorted. The
/// untracked `raw/` and `scrub-map/` directories beside them are not descended into.
///
/// # Errors
/// When `dir` cannot be read.
pub fn recordings_in(dir: &Path) -> Result<Vec<String>, Error> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names: Vec<String> = Vec::new();
    for entry in rd {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|x| x == "jsonl") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_owned());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Check one recording and reassemble it.
fn read(dir: &Path, name: &str) -> Result<(Summary, Vec<Blob>, usize), Error> {
    let cap = Capture::load(dir, name).map_err(|e| format!("{name}: {e}"))?;
    let first = cap
        .packets
        .first()
        .ok_or_else(|| format!("{name}: empty"))?;
    // `Capture::load` rebases `t` to the first datagram, so a recording that does not start at
    // zero is caught by reading its first line itself.
    let text = std::fs::read_to_string(dir.join(format!("{name}.jsonl")))?;
    let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let v: serde_json::Value = serde_json::from_str(first_line)?;
    if v["t"].as_f64() != Some(0.0) {
        return Err(format!(
            "{name}: the recording starts at t={}, not 0. A recording is rebased to its own first \
             datagram (`dereth-corpus record` does this when it splits one); nothing downstream \
             rebases it for you.",
            v["t"]
        )
        .into());
    }

    let mut logins = 0usize;
    let mut connect_requests = 0usize;
    let mut first_login_answered = false;
    let mut disconnects = 0usize;
    let mut frags = 0usize;
    let mut c2s = 0usize;
    let mut bytes = 0usize;

    let mut pending: BTreeMap<(u8, u64), NetBlob> = BTreeMap::new();
    let mut seen: BTreeSet<(u8, u32)> = BTreeSet::new();
    let mut duplicates = 0usize;
    let mut blobs = Vec::new();

    for dg in &cap.packets {
        bytes += dg.raw.len();
        let p = ParsedPacket::parse(&dg.raw)
            .map_err(|e| format!("{name}: datagram {} does not parse: {e}", dg.idx))?;
        let flags = p.header.header;
        let dkey = u8::from(dg.dir == Direction::S2c);
        if flags.has_fragments() {
            frags += 1;
        }
        match dg.dir {
            Direction::C2s => {
                c2s += 1;
                if flags.contains(PacketFlags::LOGIN_REQUEST) {
                    logins += 1;
                }
            }
            Direction::S2c => {
                if flags.contains(PacketFlags::CONNECT_REQUEST) {
                    connect_requests += 1;
                    if logins == 1 {
                        first_login_answered = true;
                    }
                }
            }
        }
        if flags.contains(PacketFlags::DISCONNECT) {
            disconnects += 1;
        }

        if !flags.has_fragments() {
            continue;
        }
        if !seen.insert((dkey, p.header.seq_id)) {
            duplicates += 1;
            continue;
        }
        for f in &p.fragments {
            let id = f.header.blob_id();
            let blob = pending
                .entry((dkey, id))
                .or_insert_with(|| NetBlob::for_recv(0));
            if !blob.receive_add_fragment(f) {
                return Err(format!(
                    "{name}: datagram {}: a fragment of blob {id:#x} was refused",
                    dg.idx
                )
                .into());
            }
            if !blob.is_complete() {
                continue;
            }
            let mut done = pending.remove(&(dkey, id)).expect("present");
            let payload = done.take_payload();
            if payload.len() < 4 {
                return Err(
                    format!("{name}: a reassembled blob is shorter than its opcode").into(),
                );
            }
            blobs.push(Blob {
                dir: dg.dir,
                t_rel: pyjson::round(dg.t_rel, 6),
                queue: done.queue_id,
                blob_id: id,
                payload,
            });
        }
    }
    let clean = disconnects > 0;
    // A session cut off mid-message is what an unclean logout is; a clean one has no such excuse.
    if clean && !pending.is_empty() {
        return Err(format!(
            "{name}: {} blob(s) never completed; the recording is truncated",
            pending.len()
        )
        .into());
    }
    if logins == 0 {
        return Err(
            format!("{name}: no LoginRequest; this file does not contain a session start").into(),
        );
    }
    if connect_requests != 1 || !first_login_answered {
        return Err(format!(
            "{name}: expected one connection (exactly one ConnectRequest, answering the first \
             LoginRequest); found {logins} LoginRequest(s) and {connect_requests} \
             ConnectRequest(s) -- a recording holding several logins is split by \
             `dereth-corpus record`"
        )
        .into());
    }
    let last = cap.packets.last().expect("non-empty");
    let summary = Summary {
        datagrams: cap.packets.len(),
        client_to_server: c2s,
        bytes,
        duration_s: pyjson::round(last.t_rel - first.t_rel, 3),
        fragment_packets: frags,
        unanswered_login_requests: logins - 1,
        clean,
    };
    Ok((summary, blobs, duplicates))
}

fn blob_line(idx: usize, b: &Blob) -> String {
    let mut hex = String::with_capacity(b.payload.len() * 2);
    for x in &b.payload {
        hex.push_str(&format!("{x:02x}"));
    }
    format!(
        "{{\"idx\": {idx}, \"dir\": \"{}\", \"t_rel\": {}, \"queue\": {}, \"blob_id\": {}, \
         \"opcode\": {}, \"payload_hex\": \"{hex}\"}}",
        dir_str(b.dir),
        pyjson::float_repr(b.t_rel),
        b.queue,
        b.blob_id,
        b.opcode(),
    )
}

/// The committed manifest's session entries, by slug: the source of the fields a person writes.
/// An absent manifest is an empty one.
fn manifest_entries(repo: &Path) -> Result<BTreeMap<String, serde_json::Value>, Error> {
    let path = repo.join(PACKET_CAPTURES).join("manifest.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("{}: {e}", path.display()).into()),
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for s in v["sessions"].as_array().into_iter().flatten() {
        if let Some(slug) = s["slug"].as_str() {
            out.insert(slug.to_owned(), s.clone());
        }
    }
    Ok(out)
}

/// The fields a person writes, for a recording the manifest does not describe yet: empty, so the
/// entry has every entry's shape and [`missing_descriptions`] can name it.
fn blank_description() -> BTreeMap<String, Value> {
    [
        ("actions", Value::Arr(Vec::new())),
        ("actions_source", Value::str("")),
        ("date", Value::Null),
        ("date_source", Value::str("")),
        ("slices", Value::Arr(Vec::new())),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// The manifest's recordings whose `actions` or `date` nobody has written yet.
///
/// # Errors
/// When the manifest cannot be read.
pub fn missing_descriptions(repo: &Path) -> Result<Vec<String>, Error> {
    Ok(manifest_entries(repo)?
        .into_iter()
        .filter(|(_, e)| {
            e["actions"].as_array().is_none_or(Vec::is_empty) || e["date"].as_str().is_none()
        })
        .map(|(slug, _)| slug)
        .collect())
}

/// Every generated file, keyed by its path relative to the checkout (forward slashes).
///
/// # Errors
/// When the folder cannot be read or a recording does not pass its checks.
pub fn generate(repo: &Path) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let dir = repo.join(PACKET_CAPTURES);
    let described = manifest_entries(repo)?;
    let mut files = BTreeMap::new();
    let mut scenarios = Vec::new();
    let mut sessions = Vec::new();
    let mut manifest_sessions = Vec::new();
    let mut without_disconnect = Vec::new();
    let mut keys_seen: BTreeSet<String> = BTreeSet::new();
    let mut totals = (0usize, 0usize, 0usize, 0usize);
    for name in recordings_in(&dir)? {
        let name = name.as_str();
        let (s, blobs, duplicates) = read(&dir, name)?;
        if !s.clean {
            without_disconnect.push(Value::str(name));
            continue;
        }
        let mut text = String::new();
        for (i, b) in blobs.iter().enumerate() {
            text.push_str(&blob_line(i, b));
            text.push('\n');
        }
        if blobs.is_empty() {
            text.push('\n');
        }
        files.insert(
            format!("{MESSAGE_CORPUS}/{name}/blobs.jsonl"),
            text.into_bytes(),
        );

        let s2c = blobs.iter().filter(|b| b.dir == Direction::S2c).count();
        let mut census: BTreeMap<String, Value> = BTreeMap::new();
        for b in blobs.iter().filter(|b| b.dir == Direction::C2s) {
            let key = format!("{:04X}", b.opcode());
            let n = match census.get(&key) {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            census.insert(key, Value::Int(n + 1));
        }
        scenarios.push(Value::obj([
            ("name", Value::str(name)),
            ("blobs", Value::int(blobs.len() as u64)),
            ("server_to_client", Value::int(s2c as u64)),
            ("client_to_server", Value::int((blobs.len() - s2c) as u64)),
            ("duplicate_datagrams", Value::int(duplicates as u64)),
            ("client_opcodes", Value::Obj(census)),
        ]));

        totals.0 += s.datagrams;
        totals.1 += s.bytes;
        totals.2 += s.fragment_packets;
        totals.3 += blobs.len();
        let s2c_datagrams = s.datagrams - s.client_to_server;
        sessions.push(Value::obj([
            ("name", Value::str(name)),
            ("datagrams", Value::int(s.datagrams as u64)),
            ("client_to_server", Value::int(s.client_to_server as u64)),
            ("server_to_client", Value::int(s2c_datagrams as u64)),
            ("bytes", Value::int(s.bytes as u64)),
            ("duration_s", Value::Float(s.duration_s)),
            ("fragment_packets", Value::int(s.fragment_packets as u64)),
            ("clean_disconnect", Value::Bool(true)),
            (
                "unanswered_login_requests",
                Value::int(s.unanswered_login_requests as u64),
            ),
        ]));

        // The manifest entry: the person's fields as committed, the generator's recomputed.
        let mut spaces: BTreeMap<String, usize> = BTreeMap::new();
        for b in &blobs {
            *spaces.entry(b.space_key()).or_default() += 1;
        }
        let first_contact: Vec<Value> = spaces
            .keys()
            .filter(|k| !keys_seen.contains(*k))
            .map(|k| Value::str(k))
            .collect();
        keys_seen.extend(spaces.keys().cloned());
        let mut entry: BTreeMap<String, Value> = match described.get(name) {
            Some(serde_json::Value::Object(m)) => m
                .iter()
                .filter(|(k, _)| !MANIFEST_GENERATED.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), Value::from_json(v)))
                .collect(),
            _ => blank_description(),
        };
        let generated = [
            ("slug", Value::str(name)),
            ("blobs", Value::int(blobs.len() as u64)),
            (
                "blobs_client_to_server",
                Value::int((blobs.len() - s2c) as u64),
            ),
            ("blobs_server_to_client", Value::int(s2c as u64)),
            ("bytes", Value::int(s.bytes as u64)),
            ("datagrams", Value::int(s.datagrams as u64)),
            (
                "datagrams_client_to_server",
                Value::int(s.client_to_server as u64),
            ),
            (
                "datagrams_server_to_client",
                Value::int(s2c_datagrams as u64),
            ),
            ("duration_s", Value::Float(s.duration_s)),
            ("fragment_packets", Value::int(s.fragment_packets as u64)),
            ("first_contact", Value::Arr(first_contact)),
            (
                "opcode_census",
                Value::Obj(
                    spaces
                        .into_iter()
                        .map(|(k, n)| (k, Value::int(n as u64)))
                        .collect(),
                ),
            ),
            (
                "unanswered_login_requests",
                Value::int(s.unanswered_login_requests as u64),
            ),
        ];
        debug_assert_eq!(generated.len(), MANIFEST_GENERATED.len());
        for (k, v) in generated {
            entry.insert(k.to_owned(), v);
        }
        manifest_sessions.push(Value::Obj(entry));
    }

    let message_index = Value::obj([("scenarios", Value::Arr(scenarios))]);
    files.insert(
        format!("{MESSAGE_CORPUS}/index.json"),
        pyjson::canonical(&message_index).into_bytes(),
    );

    let n_sessions = sessions.len() as u64;
    let capture_index = Value::obj([
        (
            "note",
            Value::str(
                "Recorded sessions between the retail client and a local ACE server, \
                 pseudonymised in place (by the dereth-corpus tool). Not \
                 reproducible from the retail data; if the raw files are lost the set is lost.",
            ),
        ),
        ("raw_source", Value::str(PACKET_CAPTURES)),
        (
            "private_source",
            Value::str(&format!("{PACKET_CAPTURES}/raw")),
        ),
        ("recipe", Value::str("tools/corpus/README.md")),
        (
            "checks_applied",
            Value::Arr(
                [
                    "the recording starts at t=0, rebased to its own first datagram",
                    "header size field matches payload length",
                    "EncryptedChecksum set iff fragments or a non-disposable optional header",
                    "one connection per session: exactly one ConnectRequest, answering the first \
                     LoginRequest; further LoginRequests are counted as unanswered_login_requests",
                    "session ends with a Disconnect; a recording that does not is listed in \
                     without_disconnect and left out of everything else",
                ]
                .into_iter()
                .map(Value::str)
                .collect(),
            ),
        ),
        (
            "totals",
            Value::obj([
                ("sessions", Value::int(n_sessions)),
                ("datagrams", Value::int(totals.0 as u64)),
                ("bytes", Value::int(totals.1 as u64)),
                ("fragment_packets", Value::int(totals.2 as u64)),
            ]),
        ),
        ("sessions", Value::Arr(sessions)),
        ("without_disconnect", Value::Arr(without_disconnect)),
    ]);
    files.insert(
        format!("{PACKET_CAPTURES}/index.json"),
        pyjson::canonical(&capture_index).into_bytes(),
    );

    let manifest = Value::obj([
        (
            "_about",
            Value::str(
                "What is in each locked recording. The index next to this file carries the \
                 datagram counts; this carries the content. Written by `dereth-corpus corpus`: \
                 the counts, `opcode_census` and `first_contact` are generated from the \
                 recordings, and every other field of an entry (`actions`, `actions_source`, \
                 `date`, `date_source`, `slices`, `derived_from`) is written by hand and carried \
                 over as it stands. A new recording gets an entry with those fields empty; fill \
                 them in.",
            ),
        ),
        (
            "first_contact_note",
            Value::str(
                "The opcode-space keys this recording carried that no earlier recording in index \
                 order carried. It is an ordering fact about this index, not a claim about when \
                 a message was first seen anywhere.",
            ),
        ),
        (
            "opcode_space_warning",
            Value::str(
                "The three are disjoint. A count in one is not a measurement in the other two.",
            ),
        ),
        (
            "opcode_spaces",
            Value::Arr(
                [
                    "bare -- a top-level opcode with no ordering wrapper",
                    "event -- server to client, inside the 0xF7B0 game-event envelope; the sub-type is the \
                     dword at offset 12",
                    "action -- client to server, inside the 0xF7B1 game-action envelope; the sub-type is the \
                     dword at offset 8",
                ]
                .into_iter()
                .map(Value::str)
                .collect(),
            ),
        ),
        ("sessions", Value::Arr(manifest_sessions)),
        ("slug_rule", Value::str(SLUG_RULE)),
        (
            "totals",
            Value::obj([
                ("blobs", Value::int(totals.3 as u64)),
                ("distinct_opcode_keys", Value::int(keys_seen.len() as u64)),
                ("sessions", Value::int(n_sessions)),
            ]),
        ),
    ]);
    files.insert(
        format!("{PACKET_CAPTURES}/manifest.json"),
        pyjson::canonical(&manifest).into_bytes(),
    );
    Ok(files)
}

/// Write every generated file under `out` (a checkout, or a directory standing in for one).
///
/// # Errors
/// On I/O failure.
pub fn write(out: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    for (rel, bytes) in files {
        let path = out.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
    }
    Ok(())
}

/// The differences between `files` and what is on disk under `repo`: a file that differs, one
/// that is missing, and anything under the message corpus the generator did not write.
///
/// # Errors
/// On I/O failure other than a missing file.
pub fn diff(repo: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    for (rel, bytes) in files {
        match std::fs::read(repo.join(rel)) {
            Ok(have) if have == *bytes => {}
            Ok(have) => out.push(format!(
                "{rel}: differs ({} bytes committed, {} generated)",
                have.len(),
                bytes.len()
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                out.push(format!("{rel}: missing"));
            }
            Err(e) => return Err(e.into()),
        }
    }
    let mut stack = vec![repo.join(MESSAGE_CORPUS)];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(repo)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if !files.contains_key(&rel) {
                out.push(format!("{rel}: not generated (a stray)"));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The parsed `corpus` command line.
#[derive(Debug)]
pub struct Args {
    /// The checkout the recordings are read from.
    pub repo: PathBuf,
    /// Where the files are written; the checkout itself unless `--out` says otherwise.
    pub out: PathBuf,
    /// Compare with the checkout instead of writing.
    pub check: bool,
}

/// `dereth-corpus corpus [--repo DIR] [--out DIR] [--check]`.
///
/// # Errors
/// When generation fails, or under `--check` when anything differs.
pub fn run(args: &Args) -> Result<(), Error> {
    let files = generate(&args.repo)?;
    if args.check {
        let d = diff(&args.repo, &files)?;
        if d.is_empty() {
            println!(
                "corpus: {} generated file(s) match {}",
                files.len(),
                args.repo.display()
            );
            return Ok(());
        }
        for line in &d {
            eprintln!("  {line}");
        }
        return Err(format!(
            "{} file(s) out of date; regenerate with `cargo run -p dereth-corpus --release -- corpus`",
            d.len()
        )
        .into());
    }
    write(&args.out, &files)?;
    println!(
        "corpus: wrote {} file(s) under {}",
        files.len(),
        args.out.display()
    );
    let missing = missing_descriptions(&args.out)?;
    if !missing.is_empty() {
        println!(
            "manifest.json: describe {} by hand -- what was done in it (`actions`, \
             `actions_source`) and when it was recorded (`date`, `date_source`)",
            missing.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Every recording in the folder is accounted for exactly once: the capture index describes
    /// it, or lists it as ending without a `Disconnect`.
    #[test]
    fn every_recording_in_the_folder_is_indexed_or_listed_as_unclean() {
        let repo = repo();
        let files = generate(&repo).expect("the recordings generate");
        let index: serde_json::Value =
            serde_json::from_slice(&files[&format!("{PACKET_CAPTURES}/index.json")])
                .expect("the index is JSON");
        let mut named: Vec<String> = index["sessions"]
            .as_array()
            .expect("sessions")
            .iter()
            .map(|s| s["name"].as_str().expect("name").to_owned())
            .chain(
                index["without_disconnect"]
                    .as_array()
                    .expect("without_disconnect")
                    .iter()
                    .map(|s| s.as_str().expect("slug").to_owned()),
            )
            .collect();
        named.sort();
        let found = recordings_in(&repo.join(PACKET_CAPTURES)).expect("the folder reads");
        assert!(!found.is_empty(), "the folder holds recordings");
        assert_eq!(named, found);
    }

    /// The fields a person writes survive regeneration untouched, and a recording nobody has
    /// described is reported rather than given invented content.
    #[test]
    fn the_manifest_keeps_what_a_person_wrote() {
        let repo = repo();
        let files = generate(&repo).expect("the recordings generate");
        let fresh: serde_json::Value =
            serde_json::from_slice(&files[&format!("{PACKET_CAPTURES}/manifest.json")])
                .expect("the manifest is JSON");
        let committed = manifest_entries(&repo).expect("the committed manifest reads");
        for entry in fresh["sessions"].as_array().expect("sessions") {
            let slug = entry["slug"].as_str().expect("slug");
            let Some(old) = committed.get(slug) else {
                continue;
            };
            for (k, v) in old.as_object().expect("an entry is an object") {
                if !MANIFEST_GENERATED.contains(&k.as_str()) {
                    assert_eq!(&entry[k], v, "{slug}: {k}");
                }
            }
        }
        let blank = blank_description();
        assert!(matches!(blank["date"], Value::Null));
        assert!(matches!(&blank["actions"], Value::Arr(a) if a.is_empty()));
    }

    /// **The consistency gate.** The committed message corpus, packet-capture index and manifest
    /// are regenerated from the recordings into a temporary directory and compared byte for byte
    /// with the tree; a stray file under the message corpus fails it too.
    #[test]
    fn committed_files_are_what_the_generator_writes() {
        let repo = repo();
        let files = generate(&repo).expect("the recordings generate");
        let tmp = std::env::temp_dir().join(format!("dere-corpus-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write(&tmp, &files).expect("write to the temporary directory");
        let mut bad = Vec::new();
        for rel in files.keys() {
            let fresh = std::fs::read(tmp.join(rel)).expect("just written");
            match std::fs::read(repo.join(rel)) {
                Ok(have) if have == fresh => {}
                Ok(_) => bad.push(format!("{rel}: differs")),
                Err(_) => bad.push(format!("{rel}: missing")),
            }
        }
        bad.extend(
            diff(&repo, &files)
                .expect("compare")
                .into_iter()
                .filter(|l| l.ends_with("(a stray)")),
        );
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            bad.is_empty(),
            "the committed corpus is stale -- regenerate with \
             `cargo run -p dereth-corpus --release -- corpus`:\n{}",
            bad.join("\n")
        );
    }
}
