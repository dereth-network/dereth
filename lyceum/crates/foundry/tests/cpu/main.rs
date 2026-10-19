//! Behaviour: none (tooling: the pack tool)
//! The pack tool's compiler, checks, bake and import, over a small set of files each test makes.

mod common;
// A world compiled: built, in conflict, imported, baked and checked.
mod world;
// A world open for editing: the session, its scratch overlay, the server's spawns, the map and
// the model names.
mod session;
// The server's content: the SQL a pack writes, its checks and the ids it names.
mod server;
