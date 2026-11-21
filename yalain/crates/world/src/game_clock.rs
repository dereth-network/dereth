//! Where the game clock starts: the moment it counts its seconds from, as Unix seconds. Every
//! game time the world sends (places, time syncs, effects) and the calendar's time of day read
//! from it.
//!
//! A world keeps the epoch it started on ([`RECORD`]), as its kept things (effects' casts and ends,
//! waits, a character's leaving) are on that clock. A world with none recorded takes [`EPOCH`], or,
//! when it already kept a character's play, [`FIRST_EPOCH`], the epoch its things were kept on.

use yalain_store::Store;

/// The game clock's epoch: the 2017 servers' after the last recorded maintenance of retail (2017:
/// the recordings of 31 January from 13:29 to 16:59 UTC, 29 sessions, game time against capture
/// time a median Unix 1,355,582,996.0, within 10 s of this; the clock stood still a few minutes at
/// each week's maintenance, so the week of 25 to 31 January before it reads about 205 s less, and
/// earlier weeks up to 805 s less).
pub const EPOCH: i64 = 1_355_583_001;

/// The epoch worlds kept their things on before [`EPOCH`] (1 December 2012).
pub const FIRST_EPOCH: i64 = 1_354_320_000;

/// The world record holding a world's epoch (eight bytes, little-endian).
pub const RECORD: &str = "game-epoch";

/// The game time at a Unix time, on an epoch.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn game_time_at(unix: f64, epoch: i64) -> f64 {
    unix - epoch as f64
}

/// The epoch a world's game clock counts from: the one it recorded, else [`FIRST_EPOCH`] for a
/// world that has kept a character's play already, else [`EPOCH`]; recorded from then on. A store
/// that cannot be read gives [`EPOCH`].
pub fn epoch_of<S: Store>(store: &mut S) -> i64 {
    match store.world_record(RECORD) {
        Ok(Some(b)) => {
            if let Ok(bytes) = <[u8; 8]>::try_from(b.as_slice()) {
                return i64::from_le_bytes(bytes);
            }
        }
        Ok(None) => {}
        Err(_) => return EPOCH,
    }
    let epoch = if kept_play(store) { FIRST_EPOCH } else { EPOCH };
    if let Err(e) = store.set_world_record(RECORD, &epoch.to_le_bytes()) {
        tracing::warn!("the game clock's epoch is not recorded: {e}");
    }
    epoch
}

/// Has any character of the world kept its play?
fn kept_play<S: Store>(store: &mut S) -> bool {
    let Ok(accounts) = store.accounts() else {
        return false;
    };
    for a in accounts {
        for c in store.characters(a.id).unwrap_or_default() {
            if store
                .world_state(c.id)
                .ok()
                .flatten()
                .is_some_and(|w| !w.play.is_empty())
            {
                return true;
            }
        }
    }
    false
}
