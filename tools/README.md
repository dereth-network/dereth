# Tools

The workspace's programs include `xtask/`, `corpus/`, `pack/`, `art/`, `pcap/` and `plugin/`. They are
crates like any other, but they are things people run, so each has
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

## `dereth-plugin` — the plugin author's tool

Packs a plugin folder into a zip, prints a package's content hash (and a zip's SHA-256), makes an
author key, signs a package (`plugin.sig`) and checks a signature, exactly as the hosts read and
check them. The formats are in `docs/plugins/packages.md`.

## `dereth-corpus` — recording sessions and the message corpus

Records a session between the retail client and a server through a logging proxy
(`record <slug>`), pseudonymises it before anything is kept (`scrub`), and regenerates the
message corpus, the capture index and the manifest from the recordings (`corpus`, and
`corpus --check`, which `cargo test -p dereth-corpus` runs). See [`corpus/README.md`](corpus/README.md)
for how to add a recording and how tests read the corpus.

## Lyceum's `lyceum` — content packs, bake, import and the client layer

The pack tool is Lyceum's now, a product of its own beside the client and the server:
[`lyceum/`](../lyceum/README.md) holds the editor, the Foundry and the `lyceum` command, which
builds a world's packs into its overlay, bakes an overlay into a copy of a player's files, takes
modified files apart into an overlay, and packs Dereth's client layer:

```text
cargo run -p lyceum-cli -- layer core/client-runtime/assets/classic-portal/pack.tsv
cargo run -p lyceum-cli -- layer <manifest> --check
```

The client builds in the classic portal's layer from `core/client-runtime/assets/classic-portal/`;
`cargo test -p lyceum-foundry` fails while that container is not what its manifest makes.

## `dereth-art` — the art survey and art packs

Surveys every model the world is built from, over your own dats: the scenery the client grows on
every landblock (placed as the client places it), each landblock's statics and buildings, the
environment pieces of every building interior and dungeon cell and the statics inside them, the
player bodies and the clothing tables' part swaps, and what the server spawns, from a table the
server's importer writes. For each model it records where it is used and how often, its size,
triangles, parts, textures and collision, and draws a 128-pixel picture of it on the CPU; it ranks
the models by how much of each use they cover.

```text
cargo run --profile test-release -p empyrean-import -- spawns --out <folder>/spawns.tsv
cargo run --profile test-release -p dereth-art -- survey --dat-dir <dats> --out <folder> --spawns <folder>/spawns.tsv
```

`--dat-dir` defaults to `DERETH_TEST_DAT_DIR` and `--out` to `target/art-survey`; `--landblocks
XXYY,...` surveys only those, `--threads` sets the thread count and `--no-thumbnails` skips the
pictures. It writes only into the output folder: `models.tsv` (one row per model), `uses.tsv` (one
row per model and use), `summary.md` (the headline numbers and the rankings), `report.html` (a
sortable, filterable page that needs only the files beside it) and `thumbs/`. A `tags.tsv` there
(`id<TAB>tags`, or `--tags <file>`) fills the report's tags column. The pictures and tables
describe your retail data: keep them to yourself.

`dereth-art pack` packs an art pack's folder into one file the client opens as it opens the
folder (`--art-pack`, `DERETH_ART_PACK` and `[Fidelity] ArtPack` take either), but without
decoding or compressing a picture: it holds the manifest, every model and buffer, and every
picture already made as the client makes it. The same folder always gives the same bytes.

```text
cargo run --release -p dereth-art -- pack <folder> [--out <file>]
cargo run --release -p dereth-art -- pack <folder> --check
```

`--out` defaults to `<folder>.dat` beside the folder. `--check` packs the folder again in memory
and says whether the file is what it packs into now, and which records are not. What the pack
cannot read or make is listed and left out, as the client leaves it out of the folder. One file
holds up to 2 GB.

## `dereth-pcap` — the community-capture index

Reads a folder of retail packet captures you bring yourself (pcap, pcapng, zip, 7z), reassembles
and decodes every session with the workspace's own transport and codecs, and writes a SQLite
index: `ingest`, `stats`, `query` and `triage` (decode failures grouped by type and error, as
markdown). See [`pcap/README.md`](pcap/README.md) for the workflow and for what these captures can
and cannot tell you.
