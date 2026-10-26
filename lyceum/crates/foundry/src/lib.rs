//! The Foundry, Lyceum's library: compiles content packs into a world's overlay, bakes an overlay
//! into a copy of a player's data files, and packs pictures into Dereth's client layer.
//!
//! **Depends on** `dereth-dat` (the containers, the overlay and its writer), `dereth-assets` (every
//! record's decoder and the world builder's encoders), `dereth-terrain` (the ground under a static
//! placed on it), `dereth-primitives` (ids and frames), `dereth-physics` (the collision a model's
//! fit is checked against), `dereth-world-data` (a setup's bounds), `png`, `toml_edit` (a source
//! edited in place), and `serde_json` (the server's names for the content forms). **Used by**
//! Lyceum: its command line, `lyceum` (`lyceum-cli`), and its editor (`lyceum-editor`), whose
//! editing logic it is.
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
//! world's one overlay and its SQL with the ids filled in, the same bytes every time. Where two
//! packs set one thing differently, the world's overrides say which wins ([`conflict`]); a build
//! refuses any conflict they do not settle, and lists them all. [`bake`]
//! writes an overlay into a copy of a player's files for a client that reads only its own files;
//! [`import`] takes any modified set of files apart into an overlay. [`export`] writes a world out
//! whole for a server that runs it elsewhere: Empyrean, ACE or GDLE. [`facts`] counts what a world
//! changes from its sources alone. [`layer`] is the client layer, the container of Dereth's own
//! pictures.
//!
//! The editing half has no interface of its own: [`edit`] holds a world open, turns each change
//! into an edit of a pack's sources, undoes and redoes them, compiles only the landblocks a change
//! reaches (the same bytes a full build makes) and saves; [`scratch`] keeps the overlay an editor
//! draws in step with it, writing only what each edit reached; [`brush`] is what a terrain brush
//! sets, [`pick`] what a ray from the camera meets, [`models`] what a model is called, and
//! [`script`] edits written as text, which `lyceum edit` runs. [`content`] is a pack's
//! server content as the editor's forms edit it: ACE SQL held as its statements, and weenies made
//! from the world database's; [`quests`] every quest of the world, where each is named, and a
//! quest copied, renamed or taken away; [`loot`] every loot table of the world, where each is used,
//! and a table changed in the world, made or taken away, with [`wielded`] a wielded treasure's sets
//! read and written as the server draws them. [`footprint`] and [`sql_ids`] are two of the checks: a
//! static placed inside a building, and SQL that names records the files do not hold.
//! [`buildings`] is every building the world places, gathered from all its copies, and [`drops`] a
//! copy of one dropped somewhere else, its rooms, doors and level ground with it. [`sample`] is the
//! sample world Lyceum carries, and a new world made from it or empty.

pub mod bake;
pub mod brush;
pub mod bsp;
pub mod buildings;
pub mod cells;
pub mod compile;
pub mod conflict;
pub mod content;
pub mod content_check;
pub mod dats;
pub mod drops;
pub mod dungeons;
pub mod edit;
pub mod error;
pub mod export;
pub mod facts;
pub mod fit;
pub mod footprint;
pub mod format;
pub mod grid;
pub mod import;
pub mod kit;
pub mod layer;
pub mod lock;
pub mod loot;
pub mod map;
pub mod model_import;
pub mod models;
pub mod moves;
pub mod pick;
pub mod pictures;
pub mod pieces;
pub mod quests;
pub mod revise;
pub mod room;
pub mod sample;
pub mod scratch;
pub mod script;
pub mod source;
pub mod sql;
pub mod sql_ids;
pub mod stab;
pub mod symbol;
pub mod validate;
pub mod walk;
pub mod wielded;
pub mod world;

pub use error::{Error, Loc};
