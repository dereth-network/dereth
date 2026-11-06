//! Our own client against the server, both in this process on the in-memory network and the
//! virtual clock, with the September 2013 install's data: the client's data files answer the
//! server's question and its script library runs both the server's calls and the player's own
//! requests.

mod admin;
mod dye_plants;
mod effects;
mod interactions;
mod mounts;
mod patrols;
mod quests;
mod session;
mod skills;
mod walls;
mod world_patch;
