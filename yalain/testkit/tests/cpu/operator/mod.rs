//! What the server's operator does while it runs: reloading the world database, the periodic
//! save, shutting the world down, moderation, the operator's characters, the logins the world
//! holds and the message of the day, and the fixed form of the console's answers.

mod admin_calls;
mod admins;
mod answers;
mod characters;
mod item_ids;
mod moderation;
mod reload;
mod reload_camps;
mod reload_placed;
mod reload_veins;
mod saves;
mod sessions;
mod shutdown;
mod store_faults;
