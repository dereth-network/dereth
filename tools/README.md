# Tools

The workspace's programs include `xtask/`, `corpus/`, `pack/` and `pcap/`. They are crates like any
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

## `dereth-pack` — the client's own records

Packs PNG pictures into a client layer, the container of Dereth's own records that the client
reads over the portal files beneath any world's overlay. A manifest names the files it lies over,
the container it writes and each record's id, format (`rgb`, the image record of the files from
before Throne of Destiny, or `surface`, the later files' `R8G8B8` image) and picture; transparent
pixels are laid on the manifest's background. The same input always gives the same bytes.

```text
cargo run -p dereth-pack -- core/client-runtime/assets/classic-portal/pack.tsv
cargo run -p dereth-pack -- <manifest> --check
```

The client builds in the classic portal's layer from `core/client-runtime/assets/classic-portal/`;
`cargo test -p dereth-pack` fails while that container is not what its manifest makes. The
library (`dereth_pack`) is the start of an editor for these records.

## `dereth-pcap` — the community-capture index

Reads a folder of retail packet captures you bring yourself (pcap, pcapng, zip, 7z), reassembles
and decodes every session with the workspace's own transport and codecs, and writes a SQLite
index: `ingest`, `stats`, `query` and `triage` (decode failures grouped by type and error, as
markdown). See [`pcap/README.md`](pcap/README.md) for the workflow and for what these captures can
and cannot tell you.
