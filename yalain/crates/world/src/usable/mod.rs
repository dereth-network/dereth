//! Objects and items a player uses for what they do in the world: doors and chests, their locks
//! and keys, surveyor's maps.
//!
//! - [`doors`]: a door opens for a while when used and closes by itself; a locked one opens only
//!   to the key that fits it, which is used up.
//! - [`chests`]: a chest opens to show what it holds, made anew each time; a locked one opens to
//!   its key, used up.
//! - [`maps`]: a surveyor's map leads its holder to a hidden place within its region, where it
//!   conjures a resource node.
//! - [`interactions`]: an item used on an object its template's interaction table names (a hoard's
//!   key on its stockpile) is carried out at once.
//! - [`vaults`]: a vault takes a player to its dungeon only on its glyph, which is used up.
//! - [`shrines`]: a faction shrine records the player's kingdom rating.
//! - [`portals`]: a portal a player summons to its lifestone or its last portal's place, which
//!   anyone may use while it stands.

pub mod chests;
pub mod doors;
pub mod interactions;
pub mod maps;
pub mod portals;
pub mod shrines;
pub mod vaults;
