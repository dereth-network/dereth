# Tools

The workspace's three programs live in `xtask/`, `corpus/` and `pcap/`. They are crates like any
other, but they are things people run, so each has
its own page. The workspace carries no scripts: every check and runner is Rust, reached through
`cargo xtask`. `sweep-timings/` holds the recorded per-test times the sweep deals shards by.

## `xtask` — the task runner

`cargo xtask <task>` runs the checks this workspace holds itself to, each from one command: the CI
tiers (`ci tier0|tier1|tier2`), the tests (`test`, in the `test-release` profile), the lints clippy
cannot express (`lint`, `comment-lint`, `test-lint`, `seams`, `crate-docs`, `fmt-check`, `doc`),
the source rules (`separation` and `output-hygiene`, both also in `ci tier0`), the repository-wide
checks (`line-endings --tree`, `skip-claims --tree`) and the per-track acceptance gates
(`gate <track>`, `gates`). `cargo xtask` with no task prints the full list. Its source is
`xtask/`; each check's module opens with the rule it enforces.

`cargo xtask sweep` runs test targets one at a time, or one tier binary sharded across
single-threaded processes (`--shard-binary <crate>:<tier>`), with a verdict that cannot read as
silence: a target that emits no result line, a binary that runs no test and a shard that dies are
each a failure, never a skip. It holds the known-red table and fails on a stale entry.
`DERETH_TEST_SHARDS=N` or `--shards N` sets the process count, `--serial` forces one process,
and the shard logs and per-test timings go under `sweep/` in cargo's target directory. `cargo
xtask sweep --help` lists the rest.

## `dereth-corpus` — recording sessions and the message corpus

Records a session between the retail client and a server through a logging proxy
(`record <slug>`), pseudonymises it before anything is kept (`scrub`), and regenerates the
message corpus, the capture index and the manifest from the recordings (`corpus`, and
`corpus --check`, which `cargo test -p dereth-corpus` runs). See [`corpus/README.md`](corpus/README.md)
for how to add a recording and how tests read the corpus.

## `dereth-pcap` — the community-capture index

Reads a folder of retail packet captures you bring yourself (pcap, pcapng, zip, 7z), reassembles
and decodes every session with the workspace's own transport and codecs, and writes a SQLite
index: `ingest`, `stats`, `query` and `triage` (decode failures grouped by type and error, as
markdown). See [`pcap/README.md`](pcap/README.md) for the workflow and for what these captures can
and cannot tell you.
