# Contributing

## Which checks to run, and when

There is one command for each moment, and each covers the client and the server alike:

| When | Command | Needs | Takes (warm) |
|---|---|---|---|
| While working, before each commit | `cargo xtask check` | nothing | 1-2 minutes |
| Before pushing or opening a PR | `cargo xtask ci tier0` | nothing | about 3 minutes |
| With the retail data | `cargo xtask ci tier1` | the dats, `world.pack` and ACE's world dump (below) | about 15 minutes |
| Milestones, on hardware | `cargo xtask ci tier2` | a graphics device and the dats | -- |

- **`check`** runs clippy and the tests of the crates you changed since your branch left
  `origin/HEAD` (`--base <ref>` to compare with another branch, `-p <crate>` to name crates
  instead), clippy over the crates that depend on them, and the fast rules: formatting, the
  comment and test lints, the seams, the crate headers.
- **`ci tier0` is exactly what CI runs. Run it before you push.** It builds everything, runs every
  test that needs no game data, clippy with every warning an error, every rule and the
  documentation check.
- **`ci tier1`** runs every crate's data tier and the server's real-content tests.
- **`ci tier2`** runs the tests that draw on a real graphics device and the comparisons against
  the original game; `--milestone` asks whether anything has been certified against it.

`cargo xtask` with no arguments lists these and every single check. Each prints a pass/fail table;
a check that could not run says why and is never counted as a pass.

## Tests

Every command builds and runs the tests in the **`test-release` profile**: optimised, with debug
assertions and overflow checks kept on, so the tests run at release speed and a `debug_assert!` or an
overflow still fails them. `--debug` on any `cargo xtask` command selects the unoptimised dev profile,
for investigating one failure. A hand-typed command takes the same profile:
`cargo test --profile test-release -p <crate>`.

A crate's integration tests are one binary per tier (`cpu`, `dat`, `gpu`); [README.md](README.md#test-tiers)
explains them.

**Your own retail data.** The game's data files and `world.pack` are never part of this repository
and never shipped with it; the tests that need them read your copies through three variables that only
tests read (no program does):

- `DERETH_TEST_DAT_DIR`: the folder holding `client_portal.dat`, `client_cell_1.dat`,
  `client_highres.dat` and `client_local_English.dat`. There is no default. Set it in your environment,
  or once for every build in a checkout with an `[env]` entry in a `.cargo/config.toml` above the
  workspace (`DERETH_TEST_DAT_DIR = { value = "<folder>", relative = true }` for a path relative to
  the folder holding that `.cargo/`).
- `EMPYREAN_TEST_WORLD_PACK`: a `world.pack` built with `empyrean-import` (see
  [empyrean/SETUP.md](empyrean/SETUP.md); default: `world.pack` at the top of the checkout).
- `EMPYREAN_WORLD_SQL`: ACE's world-database dump, for the importer's real-dump tests (default: the
  pinned release in the per-user cache that `cargo run -p empyrean-import -- fetch` downloads it into).

**Test data in the tree.** `fixtures/` holds the shared recordings, the packet captures and the
message corpus reassembled from them, which the client's and the server's tests both read.
`empyrean/fixtures/vectors/` holds ACE's answers to the server's test questions, produced by running
ACE's own code, so they stay with the server.

## Behaviours, divergences and test anchors

A claim about what the client does is a row of the **behaviour registry**
(`dereth/testkit/src/behaviours/`): an id, a sentence, and the evidence it rests on.
`dereth/DIVERGENCES.md` lists where this client knowingly differs from the original and is generated
from the registry; `empyrean/DIVERGENCES.md` lists where the server knowingly differs from ACE.

A **test anchor** says which claim a test proves, and `cargo xtask test-lint` requires one. A client
integration test carries `/// Behaviour: <id>` naming a registry row (a module that proves no
behaviour says `//! Behaviour: none (<why>)`); a server test module carries `ACE:` (the ACE source it
holds the port to), `Vectors:` (the recorded ACE answers it replays) or `Divergence:` (the numbered
divergence it pins). Tests are named for what they prove, and an `#[ignore]` says why.

## Releases

Empyrean is released with two commands: `cargo xtask release empyrean <version>` sets the version,
runs the checks and tags the release, and pushing the tag starts the release workflow, which drafts
the GitHub release. `cargo xtask package empyrean` builds the same archives locally.
[empyrean/RELEASING.md](empyrean/RELEASING.md) describes the whole flow.

## Documentation

The crates are not published to crates.io, so there is no docs.rs. Build the API documentation
locally with `cargo doc --workspace --no-deps --document-private-items --open`. (`cargo xtask doc`
is a check that the documentation's links and comments stay correct, not a way to read it.)

## Commits

- Comments, messages and commit text describe behaviour. They carry no planning labels (task ids,
  ticket or milestone names) and no pointers to documents outside this repository;
  `cargo xtask comment-lint` and `test-lint` enforce it.
- The code is rustfmt-clean (`cargo fmt --all`).
- A behaviour change comes with the test that pins it; a refactor keeps the same tests green and
  says which.
