//! Accounts and characters: the server's persistent state, in embedded SQLite or in memory for
//! tests.
//!
//! **Depends on** nothing of the game; SQLite with the `sqlite` feature. **Used by** the game
//! server, through the [`Store`] trait: [`MemoryStore`] for tests, `SqliteStore` (feature
//! `sqlite`) for a running server.
//!
//! A character is kept with what it was created from (its choices) and how the character list
//! shows it (its encoded look), so a list can be sent without recomputing anything. A deleted
//! character keeps its row, with the time it was deleted, until the server purges it.
//!
//! What a character is in the world ([`WorldState`]: where it stands, its qualities, its options)
//! is kept beside it, written when it leaves the world and read when it enters again. The store
//! keeps the qualities and options as the bytes the server hands it; only the server reads them.
//!
//! **Must never** read a character's world state: its qualities and options are the server's
//! bytes, kept and handed back.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use memory::MemoryStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;

use core::fmt;

/// A store failed: the database could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

/// The result of a store call.
pub type StoreResult<T> = Result<T, StoreError>;

/// An account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: u64,
    /// The name it logs in with; unique without regard to case.
    pub name: String,
    /// The credential check the server keeps (salt and hash), if the account has one.
    pub secret: Option<Vec<u8>>,
    /// Seconds since 1970.
    pub created_at: i64,
}

/// A character.
#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    /// The character's object id in the world.
    pub id: u64,
    pub account: u64,
    /// Unique without regard to case, deleted characters included until purged.
    pub name: String,
    pub race: u32,
    pub sex: u32,
    pub class: u32,
    pub implement: u32,
    pub start_area: u32,
    pub entity_did: u32,
    /// The physique chosen at creation: per physique type, its value.
    pub physique: Vec<(u32, f32)>,
    /// How the character list shows it, encoded as it is sent.
    pub look: Vec<u8>,
    /// Seconds since 1970.
    pub created_at: i64,
    /// When it was deleted, if it was.
    pub deleted_at: Option<i64>,
    /// When it last left the world (its creation, until it has played): the character list
    /// orders by it, the longest unplayed first.
    pub last_played: i64,
}

/// Where a character stands: a cell, and a place and heading in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredPosition {
    pub cell: u32,
    /// x, y, z in the cell's landblock.
    pub origin: [f32; 3],
    /// The rotation, scalar first (w, x, y, z).
    pub rotation: [f32; 4],
}

/// What a character is in the world, kept between its sessions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorldState {
    /// Where it stood when it last left the world; `None` before its first entry.
    pub position: Option<StoredPosition>,
    /// How many times its object has been made in the world.
    pub instances: u16,
    /// Its qualities, in the server's encoding.
    pub qualities: Vec<u8>,
    /// Its options as the client last sent them, in the server's encoding.
    pub options: Vec<u8>,
    /// The places it keeps, by kind (where it is bound to a lifestone, the last portal it took).
    pub places: Vec<(u32, StoredPosition)>,
    /// What it holds and has done in play (its items, quests and skills), in the server's
    /// encoding; empty for a character that has not played.
    pub play: Vec<u8>,
}

/// The server's persistent state.
pub trait Store {
    /// The account with this name (any case).
    ///
    /// # Errors
    /// The database failed.
    fn account_by_name(&mut self, name: &str) -> StoreResult<Option<Account>>;

    /// Make an account.
    ///
    /// # Errors
    /// The name is taken, or the database failed.
    fn create_account(
        &mut self,
        name: &str,
        secret: Option<Vec<u8>>,
        now: i64,
    ) -> StoreResult<Account>;

    /// Give an account the secret its ticket is checked against, for one that has none.
    ///
    /// # Errors
    /// No account has this id, or the database failed.
    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()>;

    /// An account's characters, deleted ones included, the longest unplayed first (ties in the
    /// order they were kept).
    ///
    /// # Errors
    /// The database failed.
    fn characters(&mut self, account: u64) -> StoreResult<Vec<Character>>;

    /// One character.
    ///
    /// # Errors
    /// The database failed.
    fn character(&mut self, id: u64) -> StoreResult<Option<Character>>;

    /// The character with this name (any case), deleted or not.
    ///
    /// # Errors
    /// The database failed.
    fn character_named(&mut self, name: &str) -> StoreResult<Option<Character>>;

    /// Is a character name in use (any case)?
    ///
    /// # Errors
    /// The database failed.
    fn name_taken(&mut self, name: &str) -> StoreResult<bool>;

    /// A number never handed out before, for a new character's id.
    ///
    /// # Errors
    /// The database failed.
    fn next_character_number(&mut self) -> StoreResult<u64>;

    /// Keep a new character.
    ///
    /// # Errors
    /// Its id or name is taken, or the database failed.
    fn insert_character(&mut self, c: &Character) -> StoreResult<()>;

    /// Mark a character deleted at a time, or restore it (`None`).
    ///
    /// # Errors
    /// The database failed.
    fn set_deleted(&mut self, id: u64, at: Option<i64>) -> StoreResult<()>;

    /// Note when a character last left the world.
    ///
    /// # Errors
    /// The database failed.
    fn set_last_played(&mut self, id: u64, at: i64) -> StoreResult<()>;

    /// Forget a character, and what it was in the world.
    ///
    /// # Errors
    /// The database failed.
    fn purge_character(&mut self, id: u64) -> StoreResult<()>;

    /// What a character is in the world; `None` before it was first kept.
    ///
    /// # Errors
    /// The database failed.
    fn world_state(&mut self, id: u64) -> StoreResult<Option<WorldState>>;

    /// Keep what a character is in the world.
    ///
    /// # Errors
    /// The database failed.
    fn set_world_state(&mut self, id: u64, state: &WorldState) -> StoreResult<()>;

    /// A record the whole world keeps, by its name (the allegiances, the consignments); `None`
    /// before it was first kept.
    ///
    /// # Errors
    /// The database failed.
    fn world_record(&mut self, name: &str) -> StoreResult<Option<Vec<u8>>>;

    /// Keep a record of the whole world.
    ///
    /// # Errors
    /// The database failed.
    fn set_world_record(&mut self, name: &str, value: &[u8]) -> StoreResult<()>;
}

/// A position as stored: the cell, then seven floats.
#[must_use]
pub fn position_bytes(p: &StoredPosition) -> Vec<u8> {
    let mut out = p.cell.to_le_bytes().to_vec();
    for f in p.origin.iter().chain(&p.rotation) {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// The inverse of [`position_bytes`]; `None` for anything but 32 bytes.
#[must_use]
pub fn position_from(b: &[u8]) -> Option<StoredPosition> {
    let (words, rest) = b.as_chunks::<4>();
    if words.len() != 8 || !rest.is_empty() {
        return None;
    }
    let f = |i: usize| f32::from_le_bytes(words[i]);
    Some(StoredPosition {
        cell: u32::from_le_bytes(words[0]),
        origin: [f(1), f(2), f(3)],
        rotation: [f(4), f(5), f(6), f(7)],
    })
}

/// The physique as stored: each pair as a little-endian word and float.
#[must_use]
pub fn physique_bytes(p: &[(u32, f32)]) -> Vec<u8> {
    p.iter()
        .flat_map(|(k, v)| k.to_le_bytes().into_iter().chain(v.to_le_bytes()))
        .collect()
}

/// The inverse of [`physique_bytes`].
#[must_use]
pub fn physique_from(b: &[u8]) -> Vec<(u32, f32)> {
    b.as_chunks::<8>()
        .0
        .iter()
        .map(|c| {
            (
                u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
            )
        })
        .collect()
}
