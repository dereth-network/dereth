//! Not ACE: a character file, which carries one character and everything it owns from one world
//! to another.
//!
//! The file is a shard database of its own, in this crate's format: the character, its character
//! record and every object it holds, as a character backup keeps them. Two tables more say what it
//! is (its [`Manifest`]): one row for each fact about it (where it was made, its era, the recipe it
//! was made from and what the checks found), and one row for each weenie class the character and
//! its objects are made of, with the class's name in the world that wrote it and, for one of that
//! world's own weenies, its symbol. A file is written whole and closed as one file, with no journal
//! beside it, so it can be copied anywhere.
//!
//! Object ids belong to the world that gave them, so a character read from a file is given the
//! ids of the world it enters ([`renumber`]) before it is saved there.

use std::collections::BTreeMap;

use empyrean_entity::enums::{PropertyDataId, PropertyInstanceId, PropertyInt, PropertyString};
use empyrean_entity::Biota;

use crate::error::StoreError;
use crate::models::shard::Character;
use crate::sqlite_shard::SqliteShard;

/// The version of the file's manifest this build writes and reads.
pub const FORMAT: u32 = 1;

/// What a character file says of itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub format: u32,
    /// The key of the world that wrote it (its data overlay's); empty for a world without one.
    pub world: String,
    /// The era that world plays (`eor`, `infiltration`).
    pub era: String,
    /// The character's name, and its object id in the file.
    pub name: String,
    pub id: u32,
    /// The recipe it was made from, as the world builder's file holds it, and its hash; empty
    /// for a character no recipe made.
    pub recipe: String,
    pub hash: String,
    /// What the checks of a legitimate character found when it was written; empty for nothing.
    pub checks: String,
    /// The server that wrote it, and its version.
    pub made_by: String,
    /// Every weenie class the character and its objects are made of.
    pub classes: Vec<Class>,
}

/// A weenie class a character file uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub class: u32,
    /// Its class name in the world that wrote the file; empty when that world had none.
    pub name: String,
    /// Its symbol in the world's packs, for one of the world's own weenies.
    pub symbol: Option<String>,
}

const CREATE: &str = "CREATE TABLE IF NOT EXISTS character_file (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS character_file_class (
        class INTEGER PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        symbol TEXT
    );";

/// Write `m` into `file`, replacing any manifest it had.
///
/// # Errors
/// A write SQLite refuses; nothing is changed then.
pub fn write_manifest(file: &SqliteShard, m: &Manifest) -> Result<(), StoreError> {
    let c = file.connection();
    c.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<(), StoreError> {
        c.execute_batch(CREATE)?;
        c.execute_batch("DELETE FROM character_file; DELETE FROM character_file_class;")?;
        let rows = [
            ("format", m.format.to_string()),
            ("world", m.world.clone()),
            ("era", m.era.clone()),
            ("name", m.name.clone()),
            ("id", m.id.to_string()),
            ("recipe", m.recipe.clone()),
            ("hash", m.hash.clone()),
            ("checks", m.checks.clone()),
            ("made_by", m.made_by.clone()),
        ];
        for (key, value) in rows {
            c.execute(
                "INSERT INTO character_file (key, value) VALUES (?1, ?2)",
                (key, value),
            )?;
        }
        for k in &m.classes {
            c.execute(
                "INSERT OR REPLACE INTO character_file_class (class, name, symbol) VALUES (?1, ?2, ?3)",
                (k.class, &k.name, &k.symbol),
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            c.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = c.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// The manifest of `file`; `None` for a shard that is not a character file.
///
/// # Errors
/// A manifest SQLite cannot read, or one that does not read as one (a missing fact or a number
/// that is none).
pub fn read_manifest(file: &SqliteShard) -> Result<Option<Manifest>, StoreError> {
    let c = file.connection();
    let has = c.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('character_file', 'character_file_class')",
        [],
        |r| r.get::<_, i64>(0),
    )?;
    if has != 2 {
        return Ok(None);
    }
    let facts: BTreeMap<String, String> = c
        .prepare("SELECT key, value FROM character_file")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let text = |key: &str| -> Result<String, StoreError> {
        facts
            .get(key)
            .cloned()
            .ok_or_else(|| StoreError::Constraint(format!("the character file has no {key}")))
    };
    let number = |key: &str| -> Result<u32, StoreError> {
        text(key)?
            .parse()
            .map_err(|_| StoreError::Constraint(format!("the character file's {key} is no number")))
    };
    let classes = c
        .prepare("SELECT class, name, symbol FROM character_file_class ORDER BY class")?
        .query_map([], |r| {
            Ok(Class {
                class: r.get(0)?,
                name: r.get(1)?,
                symbol: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Some(Manifest {
        format: number("format")?,
        world: text("world")?,
        era: text("era")?,
        name: text("name")?,
        id: number("id")?,
        recipe: text("recipe")?,
        hash: text("hash")?,
        checks: text("checks")?,
        made_by: text("made_by")?,
        classes,
    }))
}

/// Make `file` one file: its journal folded back in and no journal kept beside it from then on,
/// so the file can be copied once it is closed.
///
/// # Errors
/// A journal SQLite will not fold back.
pub fn as_one_file(file: &SqliteShard) -> Result<(), StoreError> {
    file.connection()
        .pragma_update(None, "journal_mode", "DELETE")?;
    Ok(())
}

/// The new ids a character and its objects were given, from the file's to the world's.
pub type Renumbered = BTreeMap<u32, u32>;

/// Give `player`, the objects it owns (`items`) and its character `record` the world's object
/// ids: `player_id` for the character and the next of `next_id` for each object, on the account
/// `account_id`, named `name`.
///
/// Every id that named one of them names its new id: an object's container, wielder, owner and
/// who it is bonded to, the caster of each spell on them, the author of a page they wrote, and the
/// character record's shortcuts. An object bonded to the character takes its new name as its
/// maker's. Every id that named something else, another world's object or character, is taken
/// out: such a reference (an allegiance, a house, a patron, another character's spell on it, a
/// shortcut to something it no longer holds) and the record's friends and squelches, whose ids
/// are another world's characters. The record's quests, titles, spell bars, contracts and options
/// are kept, and it is counted as logged in once, as a copied character is.
pub fn renumber(
    player: &mut Biota,
    items: &mut [Biota],
    record: &mut Character,
    (player_id, account_id, name): (u32, u32, &str),
    mut next_id: impl FnMut() -> u32,
) -> Renumbered {
    let mut ids = Renumbered::new();
    let was = player.id;
    ids.insert(was, player_id);
    for item in items.iter() {
        ids.insert(item.id, next_id());
    }
    let swap = |id: u32| ids.get(&id).copied();
    let bonded = [
        PropertyInstanceId::AllowedActivator,
        PropertyInstanceId::AllowedWielder,
    ];
    for b in std::iter::once(&mut *player).chain(items.iter_mut()) {
        if let Some(new) = swap(b.id) {
            b.id = new;
        }
        let mut bond = false;
        if let Some(iids) = b.properties_iid.as_mut() {
            let keys: Vec<PropertyInstanceId> = iids.keys().copied().collect();
            for key in keys {
                let Some(old) = iids.get(&key).copied() else {
                    continue;
                };
                match swap(old) {
                    Some(new) => {
                        iids.insert(key, new);
                        bond |= bonded.contains(&key) && old == was;
                    }
                    None => {
                        iids.remove(&key);
                    }
                }
            }
        }
        if bond {
            b.set_property(PropertyString::CraftsmanName, name.to_owned());
        }
        if let Some(spells) = b.properties_enchantment_registry.as_mut() {
            for s in spells {
                s.caster_object_id = swap(s.caster_object_id).unwrap_or(0);
            }
        }
        if let Some(pages) = b.properties_book_page_data.as_mut() {
            for p in pages {
                p.author_id = swap(p.author_id).unwrap_or(0);
            }
        }
    }
    // What made the character another world's: its allegiance and its house.
    if let Some(a) = player.properties_allegiance.as_mut() {
        a.clear();
    }
    if let Some(h) = player.house_permissions.as_mut() {
        h.clear();
    }
    player.try_remove_property(PropertyInt::AllegianceOfficerRank);
    player.try_remove_property(PropertyDataId::HouseId);
    player.set_property(PropertyString::Name, name.to_owned());

    record.id = player_id;
    record.account_id = account_id;
    name.clone_into(&mut record.name);
    record.is_deleted = false;
    record.delete_time = 0;
    record.total_logins = 1;
    record.character_properties_friend_list.clear();
    record.character_properties_squelch.clear();
    record
        .character_properties_shortcut_bar
        .retain(|s| swap(s.shortcut_object_id).is_some());
    for s in &mut record.character_properties_shortcut_bar {
        s.shortcut_object_id = swap(s.shortcut_object_id).unwrap_or(0);
        s.character_id = player_id;
    }
    for q in &mut record.character_properties_quest_registry {
        q.character_id = player_id;
    }
    for t in &mut record.character_properties_title_book {
        t.character_id = player_id;
    }
    for s in &mut record.character_properties_spell_bar {
        s.character_id = player_id;
    }
    for c in &mut record.character_properties_contract_registry {
        c.character_id = player_id;
    }
    for f in &mut record.character_properties_fill_comp_book {
        f.character_id = player_id;
    }
    ids
}
