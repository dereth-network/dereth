//! Not ACE: what a shard database holds under a world's placement guids (the static range), read
//! without changing the file, for a check before the server is pointed at another world.
//!
//! Two kinds of row name a placement by its guid and would follow the guid, not the placement, into
//! a world that numbers its placements differently:
//! - a **saved static**: a biota whose id is a placement's guid (an owned house, its slumlord, a
//!   hook or storage holding items). A landblock gives it to whichever placement has that guid.
//! - a **house permission**, keyed by the house's guid.
//!
//! Other rows that can hold a placement's guid (a corpse's killer, an enchantment's caster, the
//! house and slumlord instance ids) either change nothing when the guid goes stale or name the
//! house a saved static and a permission already name.
//!
//! A world whose newest checkpoint re-keys placements ([`WorldKeys`]) refuses a shard with such a
//! row under a guid its keys move ([`refusals`]), unless the shard is already numbered by a
//! world's keys: [`take_world_keys`] marks it so when it first passes (the `world_numbering` row),
//! and from then on its rows are under the world's own guids. A world's guids are the same across
//! its layers, whichever checkpoint keyed them.

use std::path::Path;

use empyrean_common::world_keys::WorldKeys;
use empyrean_entity::enums::WeenieType;
use empyrean_entity::object_guid::ObjectGuid;
use rusqlite::{Connection, OpenFlags};

use crate::sqlite_shard::SHARD_SCHEMA;
use crate::upgrade;
use crate::StoreError;

/// A biota saved under a placement's guid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedStatic {
    /// Its id: the placement's guid.
    pub guid: u32,
    /// The weenie it was made from.
    pub weenie_class_id: u32,
    /// Its weenie type.
    pub weenie_type: i32,
}

/// A house permission: who may enter (and, with `storage`, open the chests of) a house.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HousePermission {
    /// The house's guid.
    pub house: u32,
    /// The player's guid.
    pub player: u32,
}

/// Every row of a shard that names a placement by its guid, in guid order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedPlacements {
    /// The saved statics.
    pub statics: Vec<SavedStatic>,
    /// The house permissions.
    pub house_permissions: Vec<HousePermission>,
    /// The checkpoint whose world's keys the shard is numbered by, once a server has started it on
    /// such a world ([`take_world_keys`]); `None` while it is numbered as a world database's dump.
    pub numbered_by: Option<String>,
}

/// The shard's own record of the world numbering its rows: one row, the checkpoint whose keys it
/// took.
const NUMBERING: &str = "CREATE TABLE IF NOT EXISTS world_numbering (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    checkpoint TEXT NOT NULL
)";

/// Every row of `saved` that would follow its guid elsewhere in a world keyed by `keys`, one plain
/// line each: what the row is, and what the keys do to its guid. None for a shard already numbered
/// by a world's keys.
#[must_use]
pub fn refusals(saved: &SavedPlacements, keys: &WorldKeys) -> Vec<String> {
    if saved.numbered_by.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for s in &saved.statics {
        if let Some(why) = keys.why(s.guid) {
            let kind = usize::try_from(s.weenie_type)
                .ok()
                .and_then(|t| WeenieType::NAMES.get(t))
                .copied()
                .unwrap_or("an unknown type");
            out.push(format!(
                "saved static 0x{:08X} (weenie {}, {kind}): {why}",
                s.guid, s.weenie_class_id
            ));
        }
    }
    for p in &saved.house_permissions {
        if let Some(why) = keys.why(p.house) {
            out.push(format!(
                "house permission of house 0x{:08X} for player 0x{:08X}: {why}",
                p.house, p.player
            ));
        }
    }
    out
}

/// A server starting the shard on `conn` on a world keyed by `keys`: the rows that refuse it
/// ([`refusals`]), changing nothing; else none, and the shard is marked numbered by those keys.
///
/// # Errors
/// When SQLite can't read the rows or write the mark.
pub fn take_world_keys(conn: &Connection, keys: &WorldKeys) -> Result<Vec<String>, StoreError> {
    let saved = of(conn)?;
    let refused = refusals(&saved, keys);
    if refused.is_empty() && saved.numbered_by.as_deref() != Some(keys.checkpoint.as_str()) {
        conn.execute_batch(NUMBERING)?;
        conn.execute(
            "INSERT OR REPLACE INTO world_numbering (id, checkpoint) VALUES (1, ?1)",
            [&keys.checkpoint],
        )?;
    }
    Ok(refused)
}

/// The rows of the shard database at `path` that name a placement by its guid. The file is opened
/// read-only: it is neither upgraded nor backed up, and a server may have it open.
///
/// # Errors
/// A file that doesn't open, isn't a shard database, or a newer release wrote.
pub fn read(path: &Path) -> Result<SavedPlacements, StoreError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let found = upgrade::version(&conn, &SHARD_SCHEMA)?;
    let known = SHARD_SCHEMA.current();
    if found > known {
        return Err(StoreError::NewerSchema {
            database: SHARD_SCHEMA.name,
            path: Some(path.to_path_buf()),
            found,
            known,
            backup: None,
        });
    }
    if found < 1 {
        return Err(StoreError::NotAShard(path.to_path_buf()));
    }
    of(&conn)
}

/// The rows of the shard database on `conn` that name a placement by its guid.
///
/// # Errors
/// When SQLite can't read them.
pub fn of(conn: &Connection) -> Result<SavedPlacements, StoreError> {
    let mut q = conn.prepare(
        "SELECT id, weenie_Class_Id, weenie_Type FROM biota WHERE id >= ?1 AND id <= ?2 ORDER BY id",
    )?;
    let statics = q
        .query_map(
            [ObjectGuid::STATIC_OBJECT_MIN, ObjectGuid::STATIC_OBJECT_MAX],
            |r| {
                Ok(SavedStatic {
                    guid: r.get(0)?,
                    weenie_class_id: r.get(1)?,
                    weenie_type: r.get(2)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut q = conn.prepare(
        "SELECT house_Id, player_Guid FROM house_permission ORDER BY house_Id, player_Guid",
    )?;
    let house_permissions = q
        .query_map([], |r| {
            Ok(HousePermission {
                house: r.get(0)?,
                player: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let numbered: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'world_numbering')",
        [],
        |r| r.get(0),
    )?;
    let numbered_by = if numbered {
        conn.query_row(
            "SELECT checkpoint FROM world_numbering WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?
    } else {
        None
    };
    Ok(SavedPlacements {
        statics,
        house_permissions,
        numbered_by,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the rows of a shard that name a world's placements, read before a cutover)

    use super::*;
    use crate::SqliteShard;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("saved-placements-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir
    }

    /// Saved statics are the biotas in the static range, with their class and type, and house
    /// permissions are read whole; players and dynamic objects aren't statics. The file is only
    /// read: its schema version and contents stay as they were.
    #[test]
    fn a_shards_saved_statics_and_house_permissions_are_read_without_changing_it() {
        let dir = scratch("read");
        let path = dir.join("shard.sqlite");
        {
            let shard = SqliteShard::open(&path).expect("opens");
            shard
                .connection()
                .execute_batch(
                    "INSERT INTO biota (id, weenie_Class_Id, weenie_Type) VALUES
                       (0x50000001, 1, 10), (0x70009001, 9621, 53), (0x70009002, 9625, 55),
                       (0x80000001, 300, 1);
                     INSERT INTO house_permission (house_Id, player_Guid, storage) VALUES
                       (0x70009001, 0x50000001, 1);",
                )
                .expect("rows");
        }
        let before = std::fs::read(&path).expect("read");
        let saved = read(&path).expect("reads");
        assert_eq!(
            saved.statics,
            [
                SavedStatic {
                    guid: 0x7000_9001,
                    weenie_class_id: 9621,
                    weenie_type: 53
                },
                SavedStatic {
                    guid: 0x7000_9002,
                    weenie_class_id: 9625,
                    weenie_type: 55
                },
            ]
        );
        assert_eq!(
            saved.house_permissions,
            [HousePermission {
                house: 0x7000_9001,
                player: 0x5000_0001
            }]
        );
        assert_eq!(std::fs::read(&path).expect("read"), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is no shard, or one a newer release wrote, is refused; a missing file too.
    #[test]
    fn a_file_that_is_no_shard_or_a_newer_ones_is_refused() {
        let dir = scratch("refused");
        let other = dir.join("other.sqlite");
        Connection::open(&other)
            .expect("opens")
            .execute_batch("CREATE TABLE t (x);")
            .expect("t");
        assert!(matches!(read(&other), Err(StoreError::NotAShard(_))));
        let newer = dir.join("newer.sqlite");
        drop(SqliteShard::open(&newer).expect("opens"));
        Connection::open(&newer)
            .expect("opens")
            .pragma_update(None, "user_version", SHARD_SCHEMA.current() + 1)
            .expect("version");
        assert!(matches!(read(&newer), Err(StoreError::NewerSchema { .. })));
        assert!(read(&dir.join("missing.sqlite")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A shard starting on a world whose checkpoint re-keys placements is refused, and left as it
    /// was, while a saved static or a house permission is under a guid the keys move or give to
    /// another placement: one plain line each. Once nothing is, it is marked numbered by the
    /// world's keys, and from then on its rows are the world's own: a house saved under the guid
    /// the keys gave it doesn't refuse it.
    #[test]
    fn a_shard_is_refused_on_a_world_that_re_keys_a_saved_house_until_it_is_numbered_by_the_world()
    {
        let dir = scratch("keys");
        let path = dir.join("shard.sqlite");
        let shard = SqliteShard::open(&path).expect("opens");
        let conn = shard.connection();
        let keys =
            WorldKeys::of("2005-02-infiltration", [(0x7000_9001, 0x7000_9005)]).expect("keys");
        conn.execute_batch(
            "INSERT INTO biota (id, weenie_Class_Id, weenie_Type) VALUES
               (0x70009001, 9621, 53), (0x70009002, 9625, 55);
             INSERT INTO house_permission (house_Id, player_Guid, storage) VALUES
               (0x70009001, 0x50000001, 1);",
        )
        .expect("rows");
        let refused = take_world_keys(conn, &keys).expect("checks");
        assert_eq!(
            refused,
            [
                "saved static 0x70009001 (weenie 9621, House): 2005-02-infiltration's keys.toml moves its placement to 0x70009005",
                "house permission of house 0x70009001 for player 0x50000001: 2005-02-infiltration's keys.toml moves its placement to 0x70009005",
            ]
        );
        assert_eq!(of(conn).expect("reads").numbered_by, None);
        conn.execute_batch(
            "DELETE FROM house_permission; DELETE FROM biota WHERE id = 0x70009001;
             INSERT INTO biota (id, weenie_Class_Id, weenie_Type) VALUES (0x70009005, 9686, 56);",
        )
        .expect("rows");
        let refused = take_world_keys(conn, &keys).expect("checks");
        assert_eq!(
            refused,
            ["saved static 0x70009005 (weenie 9686, Hook): 2005-02-infiltration's keys.toml gives its guid to the placement that was 0x70009001"]
        );
        conn.execute_batch("DELETE FROM biota WHERE id = 0x70009005;")
            .expect("rows");
        assert!(take_world_keys(conn, &keys).expect("checks").is_empty());
        assert_eq!(
            of(conn).expect("reads").numbered_by.as_deref(),
            Some("2005-02-infiltration")
        );
        // Saved since under the world's numbering: the house the keys moved, at its new guid.
        conn.execute_batch(
            "INSERT INTO biota (id, weenie_Class_Id, weenie_Type) VALUES (0x70009005, 9621, 53);
             INSERT INTO house_permission (house_Id, player_Guid, storage) VALUES
               (0x70009005, 0x50000001, 1);",
        )
        .expect("rows");
        assert!(take_world_keys(conn, &keys).expect("checks").is_empty());
        let later = WorldKeys::of("2014-05-may-patch", [(0x7000_9005, 0x7000_9006)]).expect("keys");
        assert!(take_world_keys(conn, &later).expect("checks").is_empty());
        assert_eq!(
            read(&path).expect("reads").numbered_by.as_deref(),
            Some("2014-05-may-patch")
        );
        drop(shard);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
