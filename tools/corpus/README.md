# dereth-corpus

The recorded sessions in [`fixtures/packet-captures/`](../../fixtures/packet-captures/README.md)
are the only record of what the real client puts on the wire, and everything in
[`fixtures/message-corpus/`](../../fixtures/message-corpus/) is derived from them. This tool
makes both: it records a session through a logging proxy, pseudonymises it before it is kept, and
regenerates the derived files from whatever recordings the folder holds.

```
cargo run -p dereth-corpus --release -- record <slug>   # record, split and pseudonymise one session
cargo run -p dereth-corpus --release -- corpus          # regenerate the message corpus and indexes
cargo run -p dereth-corpus --release -- corpus --check  # compare instead of writing
cargo run -p dereth-corpus --release -- scrub           # re-pseudonymise every raw recording
```

`cargo run -p dereth-corpus -- --help` (or any unknown command) prints every option.

## What a recording is

A recording is the UDP traffic between the retail client and a server, one JSON line per
datagram:

```json
{"t": 0.007451, "dir": "s2c", "pair": 1, "len": 52, "data": "0000…"}
```

`t` is seconds from the recording's first datagram, `dir` is `c2s` or `s2c`, and `data` is the
datagram in hex. The protocol uses two adjacent server ports, `P` and `P + 1`; the client sends
its `ConnectResponse` to `P + 1`, and `pair` says which of the two a datagram went through (0 or
1). The server's `ConnectRequest` carries both key-stream seeds in clear, which is what lets a
replay, or the scrubber, rebuild every checksum offline.

The committed recordings were made with the retail client acclient 00.00.11.4186 (built
5 September 2013) against a local ACE server. They prove what that client puts on the wire in the
scenes recorded, and what ACE answered; they say nothing about what the retail servers sent.

## Recording one

You need the retail client and a server it can log in to, listening on its default port
(`127.0.0.1:9000`; ACE and Empyrean both default to it). Then:

1. Start the proxy, naming the recording:

   ```
   cargo run -p dereth-corpus --release -- record vendor-buy-and-sell
   ```

   It listens on `127.0.0.1:9100` and `9101` and forwards to `127.0.0.1:9000` and `9001`
   (`--listen` and `--server` change either; each names the first port of the pair). It refuses to
   start if another proxy already holds the port, and says how to find it.

2. Launch the client at the **proxy's** port, not the server's:

   ```
   acclient.exe -a <account> -v <password> -h 127.0.0.1:9100
   ```

3. Play the scene. Log out cleanly unless the point of the recording is that you don't.

4. The recording stops by itself once the link has been quiet for three seconds after the
   logout's `Disconnect` (`--linger` changes the wait, `--keep-going` turns it off for a recording
   that should span a logout and a fresh login), or on Ctrl-C.

When it stops, the raw capture is in the untracked `fixtures/packet-captures/raw/<slug>.jsonl`.
It is split into one recording per login and each is pseudonymised into
`fixtures/packet-captures/`; the tool prints the names it wrote. Then:

5. `cargo run -p dereth-corpus --release -- corpus` regenerates `fixtures/message-corpus/`, the
   capture index and the manifest. The new recording appears in all three with no code edit.
6. Describe it: fill in the new entry's `actions`, `actions_source`, `date` and `date_source` in
   `fixtures/packet-captures/manifest.json` (the `corpus` run names the entries still empty), and
   add a row to the recordings table in the folder's README.
7. `cargo test -p dereth-corpus` must pass. Adding a recording changes the corpus-wide figures, so
   a test that pinned one of them will say so; tests that read their figures from the corpus (see
   below) follow on their own.

**Slugs** are lower-case kebab-case, two to four words, naming what distinguishes the recording
(`house-purchase-refused`, `long-movement-run`). The last word is not a number, because that is
how the parts of a split recording are named. A slug the corpus or the raw folder already uses is
refused: a recording is never overwritten.

## Pseudonymisation

The raw capture carries the account name and password in the login request and character names
throughout — in the character list, in object creation, in chat, tells, the friends list and the
allegiance roster. Nothing unscrubbed is written into the tracked folder.

The scrub decodes only the few messages that *declare* an identity (the login request, the
character list, entering the world, deleting a character, creating the player's own object), gives
each identity a stand-in of exactly the same byte length, and replaces every occurrence of it —
ASCII and UTF-16, in four case shapes, across fragment boundaries — wherever it appears, which is
mostly in text no field-by-field redactor would reach. Then it recomputes every datagram's checksum
from the seeds the recording carries, and re-reads the result through the transport before
writing it. No message is re-encoded: every output byte is an input byte except those inside a
substituted name and the checksum.

- **The map.** The real-to-stand-in table goes to the untracked `fixtures/packet-captures/scrub-map/`
  (`_corpus.json` for the whole corpus, one file per recording). It is the only thing that could
  undo the scrub, so it never leaves your machine. A later run reuses the stand-ins it holds, so a
  second recording by the same account still says `acct01`; `scrub --fresh` numbers afresh.
- **The stand-ins.** Accounts become `acct01`, `acct02`, ...; passwords become a prefix of
  `passwordpassword…`; characters get names from a length-keyed dictionary when the checkout has one
  (`fixtures/packet-captures/pseudonyms.json`, `--pseudonyms FILE`) and generated pronounceable
  fillers otherwise.
- **Anything the decoder cannot see.** A name that only ever appears in free text (a character
  mentioned in chat who never appeared on screen) is not discovered. List it in
  `scrub-map/extras.json` — `{"characters": [...], "accounts": [...], "other": [...]}` — and it is
  scrubbed like the rest.

`scrub` on its own re-runs the pass over every file in `raw/` (or `--sessions a,b`), which is how the
whole corpus is re-pseudonymised after the scrubber changes.

## Splitting

Each login is its own key stream, so a capture that holds two logins is two recordings. A session
begins at the server's `ConnectRequest`; its recording begins at the last client `LoginRequest`
before that (the client resends its login request until one is answered, and each recording opens
with the one that was). A capture with one answered login is kept as `<slug>.jsonl`; one with
several becomes `<slug>-1.jsonl`, `<slug>-2.jsonl`, ..., each rebased to start at `t = 0`, and the
whole capture is kept as `raw/unsplit/<slug>.jsonl`. A capture in which the server answered no
login is not kept at all.

## The corpus the recordings generate

`corpus` reads every `fixtures/packet-captures/*.jsonl` (in slug order) and checks each one: it
starts at `t = 0`, every datagram parses, and it is one connection — exactly one `ConnectRequest`,
answering the first `LoginRequest`. A recording that also ends with a `Disconnect` is **locked**,
and from those it writes:

| file | what it holds |
|---|---|
| `fixtures/message-corpus/<slug>/blobs.jsonl` | every reassembled message of the recording, both directions, in the order each one's last fragment arrived: `{idx, dir, t_rel, queue, blob_id, opcode, payload_hex}` |
| `fixtures/message-corpus/index.json` | per recording: message counts each way, duplicate datagrams skipped, and the client's opcode census |
| `fixtures/packet-captures/index.json` | per recording: the datagram census and the checks it passed; and `without_disconnect`, the recordings that end without one |
| `fixtures/packet-captures/manifest.json` | per recording: the message census in the three opcode spaces (bare, game event, game action) and the keys it was first to carry — generated — beside `actions`, `date`, `slices` and the other fields a person writes, which are carried over untouched |

A recording without a `Disconnect` (a deliberate unclean logout) is listed in
`without_disconnect` and nowhere else.

Reassembly is the transport's own (`dereth-transport`), with the client's duplicate-delivery rule
applied on top. The generated files are a checked cache: `cargo test -p dereth-corpus` regenerates
them into a temporary directory and fails if a committed byte differs, and a second, independent
reassembler in the same tests checks the message corpus against the raw datagrams.

## Reading the corpus from tests

Tests never name a list of recordings or type a count taken from one. They read both from the
corpus through the shared readers in `dereth_client_net::client_session::testing`, which parse
each file once per test binary:

- `Corpus::shared(slug)` — one recording's messages; `Corpus::shared_all()` — every locked
  recording, in index order; `session_names()` — their slugs. A `Corpus` answers `count`,
  `count_event`, `count_action` and the censuses directly.
- `capture::shared_session(slug)` — one recording's raw datagrams (`t`, `c2s`, `pair`, `raw`), for
  a test that works at the transport level; `capture::without_disconnect()` — the recordings that
  end without one.

The pattern the corpus is for is **"every recorded X does Y"**: find every instance of something in
every recording, and hold the code under test to what the recording shows.

```rust
use dereth_client_net::client_session::testing::{Corpus, Direction};

#[test]
fn every_recorded_create_object_decodes() {
    let mut seen = 0;
    for corpus in Corpus::shared_all() {
        for blob in corpus.blobs.iter().filter(|b| {
            b.dir == Direction::ServerToClient && b.opcode == 0xF745
        }) {
            decode_create_object(&blob.payload)
                .unwrap_or_else(|e| panic!("{} #{}: {e}", corpus.name, blob.idx));
            seen += 1;
        }
    }
    // A reader that found nothing would pass the loop above vacuously.
    assert!(seen > 0, "no recording carries a create-object");
}
```

Because the loop is over whatever the corpus holds, a new recording is covered the moment
`corpus` has run, and a test that states a figure asks the corpus for it rather than writing it
down.
