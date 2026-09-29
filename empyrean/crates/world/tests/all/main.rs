//! Integration tests for `empyrean-world`: one binary per crate.

/// CantripChance's tables are process-wide statics (as in ACE): a test that rescales them holds
/// this for writing (and restores the default rates); the loot replays hold it for reading.
static CANTRIP_TABLES: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn cantrip_tables_read() -> std::sync::RwLockReadGuard<'static, ()> {
    CANTRIP_TABLES
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn cantrip_tables_write() -> std::sync::RwLockWriteGuard<'static, ()> {
    CANTRIP_TABLES
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

mod combat;
mod content;
mod inventory;
mod magic;
mod monsters;
mod motion;
mod movement;
mod net;
mod objects;
mod persistence;
mod physics;
mod players;
mod social;
mod support;
mod vectors;
mod world;

use magic::enchantment_layer_oracle as enchant_oracle;
use net::dispatch;
use net::protocol_identity as proto_identity;
use support::log_capture;
