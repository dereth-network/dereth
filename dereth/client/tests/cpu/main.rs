//! CPU-tier tests for `dereth-client`: no retail dats, no GPU device. These are the modules
//! `cargo test -p dereth-client --test cpu` runs with no feature flags at all.
//!
//! * `inventory` replays the recorded captures through `ObjectStream` and `interaction` directly,
//!   entering the world wherever each recording did, and reads load off enchanted strength.
//! * `magic` drives casting against a synthetic spell table.
//! * `movement` checks the jump-velocity inquiry.
//! * `rendering` checks the terrain split rule between the renderer and physics, and that the
//!   default build leaves the high-fidelity presentation out.
//!
//! `common/` supplies the fixture-path and corpus-size helpers.

mod common;

mod inventory;
mod magic;
mod movement;
mod movement_corpus_decode;
mod presentation;
mod rendering;
