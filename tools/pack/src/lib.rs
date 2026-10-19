//! Compiles content packs into a world's overlay, bakes an overlay into a copy of a player's data
//! files, and packs pictures into Dereth's client layer.
//!
//! **Depends on** `dereth-dat` (the containers, the overlay and its writer), `dereth-assets` (every
//! record's decoder and the world builder's encoders), `dereth-terrain` (the ground under a static
//! placed on it), `dereth-primitives` (ids and frames) and `png`. **Used by** its own command line,
//! `dereth-pack`, and by nothing else yet: it is meant to be the editing logic an editor shares.
//!
//! **Must never** write a player's data files: a build writes an overlay folder and SQL files,
//! a bake writes copies, and the originals are only read.
//!
//! A **pack** is a folder of text and pictures ([`source`]): new records described in text, edits
//! to the world's own records as operations (place, move or remove a static; set a terrain
//! vertex), and server content as ACE's SQL. Everything new is named by a symbol, never numbered.
//! A **world** ([`world`]) lists its packs in order; [`compile`] reads the data files on this
//! machine, numbers the symbols for that world alone and pins them in its lockfile ([`lock`]),
//! applies the operations, encodes the records, checks the result ([`validate`]) and writes the
//! world's one overlay and its SQL with the ids filled in, the same bytes every time. [`bake`]
//! writes an overlay into a copy of a player's files for a client that reads only its own files;
//! [`import`] takes any modified set of files apart into an overlay. [`layer`] is the client
//! layer, the container of Dereth's own pictures.

pub mod bake;
pub mod compile;
pub mod dats;
pub mod error;
pub mod import;
pub mod layer;
pub mod lock;
pub mod source;
pub mod sql;
pub mod symbol;
pub mod validate;
pub mod world;

pub use error::{Error, Loc};
