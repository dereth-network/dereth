//! Athenaeum's world machinery: a world's packs over the world database read and checked, built
//! into the server's tables against an install, counted, and edited row by row with undo and
//! redo.
//!
//! **Depends on** the shared pack format (`dereth-pack`) in Asheron's Call 2's dialect
//! (`auberean-pack`); the world database (`yalain-world-db`), whose rows, checks, merge over a
//! base and compile a world uses as the server does, and the server's tables it compiles into
//! (`yalain-content`); the data files (`auberean-dat`, `auberean-assets`) and the script library
//! (`auberean-script`) a world is built against; what the land grows (`auberean-world`) and the
//! overlay containers and shared helpers (`dereth-dat`, `dereth-primitives`) its client's records
//! are built with; `toml_edit`, to change a pack's rows in place; and logging (`tracing`).
//! **Used by** the `athenaeum` command (`athenaeum-cli`) and the editor
//! (`athenaeum-editor`).
//!
//! A **world** is a folder: its `world.toml` names the world database it lies over (`[base]`, a
//! release of `yalain-world`) and its own packs in load order, and the packs hold server rows,
//! the same rows the database's own packs hold (where people and objects stand, camps, places,
//! loot, stores, quests, what people say). The world's rows replace the database's by key and take
//! them away with `[[remove]]`; the server serves the two together.
//!
//! - [`world`]: a world's files and its base, read from disk.
//! - [`new`]: a new world over the database, with an empty pack to fill.
//! - [`build`]: a world compiled against an install into the server's tables.
//! - [`overlay`]: the client's records a world changes, built into a folder of their own.
//! - [`facts`]: what a world changes, counted.
//! - [`edit`]: a world held open for editing: rows placed, moved, turned, changed, removed and put
//!   back, every change a step undone and redone as text, saved only where the text changed.
//! - [`toml`]: rows written as the database writes them.
//!
//! **Must never** write a base database or an install: a world's edits go into its own packs, and
//! a build writes only its output folder.

pub mod build;
pub mod edit;
pub mod facts;
pub mod new;
pub mod overlay;
pub mod toml;
pub mod world;

pub use edit::{Entry, Layer, Session};
pub use world::Opened;
