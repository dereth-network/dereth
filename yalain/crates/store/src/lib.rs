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
//! What the world keeps for the Dereth Network, the network that links worlds: which characters
//! their players put on the network, and the operator's blocks of other worlds and their
//! characters. A store kept before the network was added gains both, empty, when it is opened.
//!
//! What each character lists on its friends service: its friends and the names it ignores
//! ([`Listed`]), kept as the service writes them. A store kept before the friends service gains
//! the lists, empty, when it is opened.
//!
//! What the operator's moderation keeps: each account's access level and its ban ([`Ban`]), and
//! the gagged characters with the seconds of their gags still to run. A store kept before them
//! gains them when it is opened: every account a player's, none banned, nobody gagged.
//!
//! **Must never** read a character's world state: its qualities and options are the server's
//! bytes, kept and handed back.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use memory::MemoryStore;
#[cfg(feature = "sqlite")]
pub use sqlite::{backups, restore, snapshot, SqliteStore, KEEP_BACKUPS, SCHEMA_VERSION};

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
    /// Its account may make admin characters and give admin commands. Only the operator sets it.
    pub admin: bool,
    /// The named groups its admin commands are checked against, as the operator set them.
    pub groups: Vec<String>,
    /// Its ban, while one is kept (it may have run out: the server lifts it at the next login).
    pub ban: Option<Ban>,
}

/// An account's ban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban {
    /// When it was made, in seconds since 1970.
    pub at: i64,
    /// When it ends, in seconds since 1970.
    pub until: i64,
    /// Who made it: the operator's console, or an account's name.
    pub by: String,
    /// The reason given, if one was.
    pub reason: Option<String>,
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

/// One of the lists a character keeps on its friends service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetworkList {
    /// Its friends.
    Friends,
    /// The names whose tells it does not take.
    Ignored,
}

impl NetworkList {
    /// The list's number as the store keeps it.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self {
            Self::Friends => 1,
            Self::Ignored => 2,
        }
    }

    /// The list a kept number names.
    #[must_use]
    pub const fn from_number(n: u32) -> Option<Self> {
        match n {
            1 => Some(Self::Friends),
            2 => Some(Self::Ignored),
            _ => None,
        }
    }
}

/// One name a character keeps on one of its lists, as the friends service wrote it (a character
/// of the world by its name, another world's by its address). A list holds a name once, without
/// regard to case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listed {
    pub character: u64,
    pub list: NetworkList,
    pub entry: String,
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

    /// Give an account the secret its ticket is checked against, in place of any it had.
    ///
    /// # Errors
    /// No account has this id, or the database failed.
    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()>;

    /// Every account, by name (without regard to case).
    ///
    /// # Errors
    /// The database failed.
    fn accounts(&mut self) -> StoreResult<Vec<Account>>;

    /// Set whether an account is an admin's.
    ///
    /// # Errors
    /// No account has this id, or the database failed.
    fn set_account_admin(&mut self, account: u64, admin: bool) -> StoreResult<()>;

    /// Set an account's named groups.
    ///
    /// # Errors
    /// No account has this id, or the database failed.
    fn set_account_groups(&mut self, account: u64, groups: &[String]) -> StoreResult<()>;

    /// Set the entity template a character is made from (what makes it an admin character).
    ///
    /// # Errors
    /// No character has this id, or the database failed.
    fn set_character_entity(&mut self, character: u64, entity: u32) -> StoreResult<()>;

    /// Keep an account's ban, or lift it (`None`).
    ///
    /// # Errors
    /// No account has this id, or the database failed.
    fn set_account_ban(&mut self, account: u64, ban: Option<&Ban>) -> StoreResult<()>;

    /// Give a character a new name (free: the caller checks). Every character's lists that hold it
    /// by its old name hold it by the new one, in the same breath.
    ///
    /// # Errors
    /// No character has this id, the name is taken, or the database failed.
    fn rename_character(&mut self, character: u64, name: &str) -> StoreResult<()>;

    /// The seconds a character's gag has still to run; `None` when it is not gagged.
    ///
    /// # Errors
    /// The database failed.
    fn gag(&mut self, character: u64) -> StoreResult<Option<f64>>;

    /// Keep a character's gag with the seconds it has still to run, or lift it (`None`).
    /// Forgetting the character lifts it.
    ///
    /// # Errors
    /// The database failed.
    fn set_gag(&mut self, character: u64, seconds: Option<f64>) -> StoreResult<()>;

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

    /// Set aside `n` numbers for the world's things, never to be given again: the first of them,
    /// from 1. Kept before it returns.
    ///
    /// # Errors
    /// The database failed.
    fn reserve_thing_numbers(&mut self, n: u64) -> StoreResult<u64>;

    /// Keep a new character.
    ///
    /// # Errors
    /// Its id or name is taken, or the database failed.
    fn insert_character(&mut self, c: &Character) -> StoreResult<()>;

    /// Keep a new character with what it is in the world, both or neither (a character the
    /// operator makes with a build, or brings in from another world).
    ///
    /// # Errors
    /// Its id or name is taken, or the database failed; nothing is kept then.
    fn insert_character_with_state(&mut self, c: &Character, state: &WorldState)
        -> StoreResult<()>;

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

    /// Keep these characters' world states and these world records together: after a failure or a
    /// crash, all of them or none. A state of a character the store no longer has is left out.
    ///
    /// # Errors
    /// The database failed, and nothing was kept.
    fn save_all(
        &mut self,
        states: &[(u64, &WorldState)],
        records: &[(&str, &[u8])],
    ) -> StoreResult<()>;

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

    /// The characters on the Dereth Network, by id: every character starts off it, and is on it
    /// once its player puts it on.
    ///
    /// # Errors
    /// The database failed.
    fn network_characters(&mut self) -> StoreResult<Vec<u64>>;

    /// Put a character on the Dereth Network, or take it off. Forgetting the character takes it
    /// off.
    ///
    /// # Errors
    /// The database failed.
    fn set_network_on(&mut self, id: u64, on: bool) -> StoreResult<()>;

    /// The operator's blocks on the Dereth Network: other worlds' names and their characters'
    /// addresses, each as the network writes its identity (in small letters).
    ///
    /// # Errors
    /// The database failed.
    fn network_blocks(&mut self) -> StoreResult<Vec<String>>;

    /// Keep an operator's block, or lift it.
    ///
    /// # Errors
    /// The database failed.
    fn set_network_block(&mut self, block: &str, on: bool) -> StoreResult<()>;

    /// Every name every character lists, by character, list and name.
    ///
    /// # Errors
    /// The database failed.
    fn network_lists(&mut self) -> StoreResult<Vec<Listed>>;

    /// Keep a name on a character's list, or take it off (any case). Forgetting the character
    /// forgets its lists.
    ///
    /// # Errors
    /// The database failed.
    fn set_listed(
        &mut self,
        character: u64,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()>;
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
