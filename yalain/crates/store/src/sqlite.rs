//! The store in an embedded SQLite file.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    physique_bytes, physique_from, position_bytes, position_from, Account, Character, Listed,
    NetworkList, Store, StoreError, StoreResult, WorldState,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS account (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    secret BLOB,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS character (
    id INTEGER PRIMARY KEY,
    account INTEGER NOT NULL REFERENCES account (id),
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    race INTEGER NOT NULL,
    sex INTEGER NOT NULL,
    class INTEGER NOT NULL,
    implement INTEGER NOT NULL,
    start_area INTEGER NOT NULL,
    entity_did INTEGER NOT NULL,
    physique BLOB NOT NULL,
    look BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    deleted_at INTEGER,
    last_played INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS counter (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS world_state (
    id INTEGER PRIMARY KEY REFERENCES character (id),
    position BLOB,
    instances INTEGER NOT NULL,
    qualities BLOB NOT NULL,
    options BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS play_state (
    id INTEGER PRIMARY KEY REFERENCES character (id),
    play BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS kept_place (
    id INTEGER NOT NULL REFERENCES character (id),
    kind INTEGER NOT NULL,
    position BLOB NOT NULL,
    PRIMARY KEY (id, kind)
);
CREATE TABLE IF NOT EXISTS world_record (name TEXT PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS network_character (id INTEGER PRIMARY KEY REFERENCES character (id));
CREATE TABLE IF NOT EXISTS network_block (block TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS network_list (
    id INTEGER NOT NULL REFERENCES character (id),
    list INTEGER NOT NULL,
    entry TEXT NOT NULL COLLATE NOCASE,
    PRIMARY KEY (id, list, entry)
);
";

const CHARACTER_COLUMNS: &str =
    "id, account, name, race, sex, class, implement, start_area, entity_did, physique, look, created_at, deleted_at, last_played";

#[allow(clippy::needless_pass_by_value)]
fn err(e: rusqlite::Error) -> StoreError {
    StoreError(e.to_string())
}

/// The SQLite store.
#[derive(Debug)]
pub struct SqliteStore {
    db: Connection,
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn character(r: &Row<'_>) -> rusqlite::Result<Character> {
    Ok(Character {
        id: r.get::<_, i64>(0)? as u64,
        account: r.get::<_, i64>(1)? as u64,
        name: r.get(2)?,
        race: r.get::<_, i64>(3)? as u32,
        sex: r.get::<_, i64>(4)? as u32,
        class: r.get::<_, i64>(5)? as u32,
        implement: r.get::<_, i64>(6)? as u32,
        start_area: r.get::<_, i64>(7)? as u32,
        entity_did: r.get::<_, i64>(8)? as u32,
        physique: physique_from(&r.get::<_, Vec<u8>>(9)?),
        look: r.get(10)?,
        created_at: r.get(11)?,
        deleted_at: r.get(12)?,
        last_played: r.get(13)?,
    })
}

impl SqliteStore {
    /// Open (or make) the store in a file.
    ///
    /// # Errors
    /// The file cannot be opened or its tables made.
    pub fn open(path: &Path) -> StoreResult<Self> {
        Self::with(Connection::open(path).map_err(err)?)
    }

    /// A store in memory, gone when dropped.
    ///
    /// # Errors
    /// The tables cannot be made.
    pub fn open_in_memory() -> StoreResult<Self> {
        Self::with(Connection::open_in_memory().map_err(err)?)
    }

    fn with(db: Connection) -> StoreResult<Self> {
        db.execute_batch(SCHEMA).map_err(err)?;
        Ok(Self { db })
    }
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
impl Store for SqliteStore {
    fn account_by_name(&mut self, name: &str) -> StoreResult<Option<Account>> {
        self.db
            .query_row(
                "SELECT id, name, secret, created_at FROM account WHERE name = ?1",
                [name],
                |r| {
                    Ok(Account {
                        id: r.get::<_, i64>(0)? as u64,
                        name: r.get(1)?,
                        secret: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(err)
    }

    fn create_account(
        &mut self,
        name: &str,
        secret: Option<Vec<u8>>,
        now: i64,
    ) -> StoreResult<Account> {
        self.db
            .execute(
                "INSERT INTO account (name, secret, created_at) VALUES (?1, ?2, ?3)",
                params![name, secret, now],
            )
            .map_err(err)?;
        let id = self.db.last_insert_rowid() as u64;
        Ok(Account {
            id,
            name: name.to_string(),
            secret,
            created_at: now,
        })
    }

    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE account SET secret = ?2 WHERE id = ?1",
                params![account as i64, secret],
            )
            .map_err(err)?;
        if n == 0 {
            return Err(StoreError(format!("no account {account}")));
        }
        Ok(())
    }

    fn characters(&mut self, account: u64) -> StoreResult<Vec<Character>> {
        let mut s = self
            .db
            .prepare(&format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE account = ?1 ORDER BY last_played, rowid"))
            .map_err(err)?;
        let rows = s.query_map([account as i64], character).map_err(err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)
    }

    fn character(&mut self, id: u64) -> StoreResult<Option<Character>> {
        self.db
            .query_row(
                &format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE id = ?1"),
                [id as i64],
                character,
            )
            .optional()
            .map_err(err)
    }

    fn character_named(&mut self, name: &str) -> StoreResult<Option<Character>> {
        self.db
            .query_row(
                &format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE name = ?1"),
                [name],
                character,
            )
            .optional()
            .map_err(err)
    }

    fn name_taken(&mut self, name: &str) -> StoreResult<bool> {
        self.db
            .query_row(
                "SELECT count(*) FROM character WHERE name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(err)
    }

    fn next_character_number(&mut self) -> StoreResult<u64> {
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO counter (name, value) VALUES (?1, 1) ON CONFLICT (name) DO UPDATE SET value = value + 1",
            ["character"],
        )
        .map_err(err)?;
        let n: i64 = tx
            .query_row(
                "SELECT value FROM counter WHERE name = ?1",
                ["character"],
                |r| r.get(0),
            )
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(n as u64)
    }

    fn insert_character(&mut self, c: &Character) -> StoreResult<()> {
        let sql = format!(
            "INSERT INTO character ({CHARACTER_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
        );
        self.db
            .execute(
                &sql,
                params![
                    c.id as i64,
                    c.account as i64,
                    c.name,
                    i64::from(c.race),
                    i64::from(c.sex),
                    i64::from(c.class),
                    i64::from(c.implement),
                    i64::from(c.start_area),
                    i64::from(c.entity_did),
                    physique_bytes(&c.physique),
                    c.look,
                    c.created_at,
                    c.deleted_at,
                    c.last_played
                ],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn set_deleted(&mut self, id: u64, at: Option<i64>) -> StoreResult<()> {
        self.db
            .execute(
                "UPDATE character SET deleted_at = ?1 WHERE id = ?2",
                params![at, id as i64],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn set_last_played(&mut self, id: u64, at: i64) -> StoreResult<()> {
        self.db
            .execute(
                "UPDATE character SET last_played = ?1 WHERE id = ?2",
                params![at, id as i64],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn purge_character(&mut self, id: u64) -> StoreResult<()> {
        self.db
            .execute("DELETE FROM world_state WHERE id = ?1", [id as i64])
            .map_err(err)?;
        self.db
            .execute("DELETE FROM kept_place WHERE id = ?1", [id as i64])
            .map_err(err)?;
        self.db
            .execute("DELETE FROM play_state WHERE id = ?1", [id as i64])
            .map_err(err)?;
        self.db
            .execute("DELETE FROM network_character WHERE id = ?1", [id as i64])
            .map_err(err)?;
        self.db
            .execute("DELETE FROM network_list WHERE id = ?1", [id as i64])
            .map_err(err)?;
        self.db
            .execute("DELETE FROM character WHERE id = ?1", [id as i64])
            .map(|_| ())
            .map_err(err)
    }

    fn world_state(&mut self, id: u64) -> StoreResult<Option<WorldState>> {
        let mut places = Vec::new();
        {
            let mut st = self
                .db
                .prepare("SELECT kind, position FROM kept_place WHERE id = ?1 ORDER BY kind")
                .map_err(err)?;
            let rows = st
                .query_map([id as i64], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(err)?;
            for row in rows {
                let (kind, bytes) = row.map_err(err)?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                if let Some(p) = position_from(&bytes) {
                    places.push((kind as u32, p));
                }
            }
        }
        let play: Vec<u8> = self
            .db
            .query_row(
                "SELECT play FROM play_state WHERE id = ?1",
                [id as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?
            .unwrap_or_default();
        self.db
            .query_row(
                "SELECT position, instances, qualities, options FROM world_state WHERE id = ?1",
                [id as i64],
                |r| {
                    let position: Option<Vec<u8>> = r.get(0)?;
                    #[allow(clippy::cast_possible_truncation)]
                    Ok(WorldState {
                        position: position.as_deref().and_then(position_from),
                        instances: r.get::<_, i64>(1)? as u16,
                        qualities: r.get(2)?,
                        options: r.get(3)?,
                        places: places.clone(),
                        play: play.clone(),
                    })
                },
            )
            .optional()
            .map_err(err)
    }

    fn set_world_state(&mut self, id: u64, s: &WorldState) -> StoreResult<()> {
        self.db
            .execute(
                "INSERT INTO world_state (id, position, instances, qualities, options) VALUES (?1, ?2, ?3, ?4, ?5)                  ON CONFLICT (id) DO UPDATE SET position = ?2, instances = ?3, qualities = ?4, options = ?5",
                params![
                    id as i64,
                    s.position.as_ref().map(position_bytes),
                    i64::from(s.instances),
                    s.qualities,
                    s.options
                ],
            )
            .map_err(err)?;
        self.db
            .execute(
                "INSERT INTO play_state (id, play) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET play = ?2",
                params![id as i64, s.play],
            )
            .map_err(err)?;
        self.db
            .execute("DELETE FROM kept_place WHERE id = ?1", [id as i64])
            .map_err(err)?;
        for (kind, p) in &s.places {
            self.db
                .execute(
                    "INSERT INTO kept_place (id, kind, position) VALUES (?1, ?2, ?3)",
                    params![id as i64, i64::from(*kind), position_bytes(p)],
                )
                .map_err(err)?;
        }
        Ok(())
    }

    fn world_record(&mut self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        self.db
            .query_row(
                "SELECT value FROM world_record WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)
    }

    fn set_world_record(&mut self, name: &str, value: &[u8]) -> StoreResult<()> {
        self.db
            .execute(
                "INSERT INTO world_record (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2",
                params![name, value],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn network_characters(&mut self) -> StoreResult<Vec<u64>> {
        let mut st = self
            .db
            .prepare("SELECT id FROM network_character ORDER BY id")
            .map_err(err)?;
        let rows = st.query_map([], |r| r.get::<_, i64>(0)).map_err(err)?;
        rows.map(|r| r.map(|id| id as u64).map_err(err)).collect()
    }

    fn set_network_on(&mut self, id: u64, on: bool) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_character (id) VALUES (?1)"
        } else {
            "DELETE FROM network_character WHERE id = ?1"
        };
        self.db.execute(sql, [id as i64]).map(|_| ()).map_err(err)
    }

    fn network_blocks(&mut self) -> StoreResult<Vec<String>> {
        let mut st = self
            .db
            .prepare("SELECT block FROM network_block ORDER BY block")
            .map_err(err)?;
        let rows = st.query_map([], |r| r.get(0)).map_err(err)?;
        rows.map(|r| r.map_err(err)).collect()
    }

    fn set_network_block(&mut self, block: &str, on: bool) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_block (block) VALUES (?1)"
        } else {
            "DELETE FROM network_block WHERE block = ?1"
        };
        self.db.execute(sql, [block]).map(|_| ()).map_err(err)
    }

    fn network_lists(&mut self) -> StoreResult<Vec<Listed>> {
        let mut st = self
            .db
            .prepare("SELECT id, list, entry FROM network_list ORDER BY id, list, entry")
            .map_err(err)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?;
        let mut out = Vec::new();
        for row in rows {
            let (character, list, entry) = row.map_err(err)?;
            // A list a later version keeps is left alone.
            if let Some(list) = u32::try_from(list).ok().and_then(NetworkList::from_number) {
                out.push(Listed {
                    character,
                    list,
                    entry,
                });
            }
        }
        out.sort();
        Ok(out)
    }

    fn set_listed(
        &mut self,
        character: u64,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_list (id, list, entry) VALUES (?1, ?2, ?3)"
        } else {
            "DELETE FROM network_list WHERE id = ?1 AND list = ?2 AND entry = ?3"
        };
        self.db
            .execute(
                sql,
                params![character as i64, i64::from(list.number()), entry],
            )
            .map(|_| ())
            .map_err(err)
    }
}
