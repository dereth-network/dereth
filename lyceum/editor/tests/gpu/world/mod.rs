//! The world and its dungeons: a world on older files, dungeon mode and its floors, the flying
//! camera, the links between spawns, the marks on the world view, and the server's own spawns
//! given back to the editor's world.

mod classic;
pub(crate) mod dungeon;
mod floors;
mod fly;
mod links;
#[cfg(feature = "live-server-tests")]
mod live_restore;
mod marks;
