//! Music made of instruments: the music files (pieces, styles, instrument collections, scripts,
//! as RIFF files), the instrument synthesizer, and the player that turns a piece into notes on
//! time.
//!
//! - [`riff`]: the chunk tree every music file is;
//! - [`dls`]: instrument collections;
//! - [`form`]: pieces, bands, styles and scripts;
//! - [`library`]: the files by name, and what a piece needs;
//! - [`synth`]: the synthesizer;
//! - [`chord`]: chords, scales, and a style note's key;
//! - [`script`]: the scripts' routines;
//! - [`perf`]: the performance, which plays pieces on paths in music time;
//! - [`effects`]: the compressors on the paths.

pub mod chord;
pub mod dls;
pub mod effects;
pub mod form;
pub mod library;
pub mod perf;
pub mod riff;
pub mod script;
pub mod synth;
