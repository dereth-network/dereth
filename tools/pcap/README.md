# dereth-pcap

An index of community packet captures from the last days of retail Asheron's Call. It reads a
folder of captures, reassembles and decodes every session with the workspace's own transport
(`dereth-transport`) and codecs (`dereth-protocol`), and writes a SQLite index you can ask
questions of. Every decode failure in it is a finding about those two crates.

**We host no captures.** Community archives of end-of-retail traffic exist; bring your own and
point the tool at them. Nothing from them is committed here, and the index is a local file.

## What these captures are evidence of

Community captures are end-of-retail: the final retail client (acclient 00.00.11.6096, the build
Dereth targets) against the end-of-retail servers, which Empyrean emulates. They are evidence about
the protocol and about what the retail servers sent — which messages exist, how often, and what
shape they have — and that is useful to client and server work alike.

They are **not evidence of client behaviour.** A capture shows what crossed the wire, not what the
client did with it: that a message arrived says nothing about what the client drew, played or
sent in reply because of it. Our own recordings
([`fixtures/packet-captures/`](../../fixtures/packet-captures/README.md), the 2013 retail
client against a local ACE server) are what prove what the client puts on the wire, and only in
the scenes they record.

## Setting up

```
cargo build -p dereth-pcap --release
```

The captures are found through `DERETH_RETAIL_PCAPS`, a path list (`;`-separated on Windows,
`:` elsewhere) of folders, or `--roots <paths>`. Everything under them is read: pcap and pcapng,
and either inside zip or 7z archives. The index is written to `<first root>/.index/retail.sqlite`
unless `DERETH_PCAP_INDEX` or `--index <file>` says otherwise; it can always be rebuilt.

## Commands

- **`ingest`** reads every capture under the roots into the index. It is incremental by content
  hash, so a re-run only reads what is new; `--rebuild` starts over, `--jobs N` sets the worker
  count, and `--server-ports 9000-9013` (the default, the retail ports) decides which UDP flows
  are game sessions. A session is one client connection; most community captures start after the
  handshake, so most sessions are *partial* (no key-stream seeds, so no checksum check, and no
  client version).
- **`stats`** summarises the index: captures, sessions (by game: the archives also hold Asheron's
  Call 2 traffic, which shares the transport and is stored undecoded), messages each way, the
  commonest types, and decode failures.
- **`query "<SQL>"`** runs SQL against the index, read-only, as a table or with `--csv`. The tables
  are `capture`, `session`, `packet` and `message`, with the views `v_session` and `v_message`
  joining them up; filter on `game = 'ac1'` for any Asheron's Call statistic. Four functions are
  added: `fields(dir, raw)` (a message's decoded fields as JSON), `guids(dir, raw)`,
  `type_name(mtype)` and `sid(key)`.
- **`triage`** writes the decode failures as markdown to the file `--out FILE` names (`--examples N`): grouped by
  direction, message type and error kind, each group with its count, example messages and the hex
  around the byte where decoding failed.

**Cite sessions by key, not by row id.** `session_id` and `message_id` are renumbered by every
rebuild. A session's `session_key` is `<capture>:<ordinal>` — the first 16 hex digits of the
capture's content hash and the session's position in it — so it is the same on every machine that
has the same capture under any name; a message's `msg_key` is `<session_key>#<idx>`. `sid(key)`
turns either back into the current `session_id`:

```
dereth-pcap query "SELECT msg_key, mtype_hex, error_kind, error_offset, hex(raw)
                   FROM v_message WHERE session_id = sid('40f9083a1db8c4e9:33') AND idx = 5"
```

## From a triage finding to a fix

`triage` gives every group a class, and every group starts in class 1:

1. **Our bug** — the decoder is wrong about a message retail really sent.
2. **A build difference** — a known protocol difference between client builds explains it.
3. **Retail only** — a message the retail servers sent that ACE never does, so nothing here has a
   codec for it yet.

A group moves out of class 1 only with evidence.

For our bug, the loop is:

1. Pick a group from `triage` and pull its examples with `query`: the bytes (`hex(raw)`), the
   failing offset, and the same message type in sessions where it decodes.
2. Fix the codec in `dereth-protocol`.
3. Add a test there that states the shape the capture revealed, built from bytes the test writes
   itself. Captured bytes are not ours to commit, and a test pinned to one capture proves one
   message.
4. Hold the whole corpus to it with a property test that reads the index through
   `dereth_pcap::corpus::open_or_skip("<test name>")`: every message of that type now decodes.
   Where `DERETH_RETAIL_PCAPS` is unset the test is a printed, counted skip, never a silent pass;
   where it is set but not ingested, it fails.
5. Re-run `ingest --rebuild` (an incremental ingest skips captures it has already read, so it
   would keep the old decoder's verdicts) and `triage`: the group is gone.
