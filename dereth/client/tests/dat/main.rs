//! DAT-tier tests for `dereth-client`: each module reaches the retail dats under
//! `$DERETH_TEST_DAT_DIR` -- through `client_dir()`, `RetailDatStore`, `App`, or a fixture set
//! derived from them. Selected by `--features retail-dats`.
//!
//! The modules are grouped by area (`audio`, `inventory`, `login`, `movement`, `net`, `objects`, `panels`, `rendering`,
//! `selection`, `ui`, `world`), so `--test dat objects::` runs one area. Every assertion here
//! depends on the installed data: authored cells, meshes, animations, cursors, sounds, surfaces,
//! setup records, or objects decoded from those inputs. Behaviour checks whose oracle is a model
//! rule or a recording live in the scenario test crate instead.

mod common;

mod audio;
mod inventory;
mod login;
mod movement;
mod net;
mod objects;
mod panels;
mod rendering;
mod selection;
mod ui;
mod world;
