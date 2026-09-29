// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/ShardDbContext.cs
//! `SqliteShard`: the shard database in embedded SQLite (a file or `:memory:`), replacing ACE's
//! MySQL `ShardDbContext`. Implements the [`ShardDatabase`] backend primitives and
//! [`ShardConfigDatabase`].
//!
//! Every read orders its rows the way MySQL/InnoDB returns them for ACE's query: a child table
//! read by `object_Id` comes back in primary-key order when the key starts with `object_Id`
//! (`type`, `position_Type`, `spell`, ...), and in `id` order when it is read through a secondary
//! `object_Id` index; `biota_properties_body_part` and `biota_properties_book_page_data` come back in
//! their unique-key order (`key`, `page_Id`); emotes and their actions in `id` order (Entity
//! Framework's `Include` orders by key). That order becomes the entity model's dictionary order.

use std::path::Path;

use empyrean_common::dotnet::CsCast;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::StoreError;
use crate::models::shard as db;
use crate::models::shard::{Biota, Character};
use crate::shard_config_database::ShardConfigDatabase;
use crate::shard_database::{BiotaQuery, CharacterQuery, PopulatedCollectionFlags, ShardDatabase};
use crate::upgrade::{self, Schema, UpgradePolicy, Versioning};

/// Schema migrations, applied in order; `PRAGMA user_version` records how many have run (see
/// [`crate::upgrade`]). Version 1 is the schema Empyrean 0.1.0 shipped.
pub const SHARD_MIGRATIONS: &[&str] = &[include_str!("schema/shard_v001.sql")];

/// The shard schema: [`SHARD_MIGRATIONS`], versioned in `PRAGMA user_version`.
pub const SHARD_SCHEMA: Schema = Schema {
    name: "shard",
    label: "shard",
    migrations: SHARD_MIGRATIONS,
    versioning: Versioning::UserVersion,
};

/// The shard database over one SQLite connection.
pub struct SqliteShard {
    conn: Connection,
    in_batch: bool,
}

impl std::fmt::Debug for SqliteShard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteShard")
            .field("path", &self.conn.path())
            .field("in_batch", &self.in_batch)
            .finish()
    }
}

/// Opens `conn` for empyrean-store: the name collation ([`crate::collation`]) registered, foreign
/// keys on, and (for a file) WAL with `synchronous = NORMAL`, which survives a process crash at any
/// point (a committed transaction is in the WAL before `COMMIT` returns). The schema is brought up
/// to date separately ([`crate::upgrade`]).
pub(crate) fn prepare_connection(conn: &Connection, file: bool) -> Result<(), StoreError> {
    crate::collation::register(conn)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    if file {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
    }
    Ok(())
}

/// Runs `f` inside a savepoint: released on success, rolled back on error.
pub(crate) fn in_savepoint<T>(
    conn: &Connection,
    f: impl FnOnce(&Connection) -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    conn.execute_batch("SAVEPOINT serv_store_write")?;
    match f(conn) {
        Ok(v) => {
            conn.execute_batch("RELEASE serv_store_write")?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO serv_store_write; RELEASE serv_store_write");
            Err(e)
        }
    }
}

impl SqliteShard {
    /// Opens (creating if needed) the shard database file at `path`, upgrading its schema as
    /// [`crate::upgrade`] describes (backups beside the file).
    ///
    /// # Errors
    /// When the file cannot be opened, was written by a newer release, cannot be backed up before
    /// an upgrade, or cannot be migrated.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_with(path, &UpgradePolicy::default())
    }

    /// [`SqliteShard::open`] with the backups kept as `policy` says.
    ///
    /// # Errors
    /// As [`SqliteShard::open`].
    pub fn open_with(path: impl AsRef<Path>, policy: &UpgradePolicy) -> Result<Self, StoreError> {
        let (conn, _) = upgrade::open(path.as_ref(), &SHARD_SCHEMA, policy)?;
        Ok(Self {
            conn,
            in_batch: false,
        })
    }

    /// A fresh in-memory shard database (the unit tier's).
    ///
    /// # Errors
    /// When SQLite cannot create it.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        prepare_connection(&conn, false)?;
        upgrade::upgrade(&conn, None, &SHARD_SCHEMA, &UpgradePolicy::default())?;
        Ok(Self {
            conn,
            in_batch: false,
        })
    }

    /// The connection (for tests and tools).
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Whether a batch transaction is open.
    #[must_use]
    pub fn in_batch(&self) -> bool {
        self.in_batch
    }
}

fn u32s(conn: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<Vec<u32>, StoreError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(p, |r| r.get::<_, u32>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn rows<T>(
    conn: &Connection,
    sql: &str,
    id: u32,
    f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, StoreError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map([id], f)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

impl ShardDatabase for SqliteShard {
    fn load_biota_row(&mut self, id: u32) -> Result<Option<Biota>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, weenie_Class_Id, weenie_Type, populated_Collection_Flags FROM biota WHERE id = ?1")?;
        Ok(stmt
            .query_row([id], |r| {
                Ok(Biota {
                    id: r.get(0)?,
                    weenie_class_id: r.get(1)?,
                    weenie_type: r.get(2)?,
                    populated_collection_flags: r.get(3)?,
                    ..Default::default()
                })
            })
            .optional()?)
    }

    #[allow(clippy::too_many_lines)]
    fn load_biota_collections(
        &mut self,
        b: &mut Biota,
        flags: PopulatedCollectionFlags,
    ) -> Result<(), StoreError> {
        type F = PopulatedCollectionFlags;
        let c = &self.conn;
        let id = b.id;

        if flags.has_flag(F::BIOTA_PROPERTIES_ANIM_PART) {
            b.biota_properties_anim_part = rows(c, "SELECT id, object_Id, \"index\", animation_Id, \"order\" FROM biota_properties_anim_part WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesAnimPart { id: r.get(0)?, object_id: r.get(1)?, index: r.get(2)?, animation_id: r.get(3)?, order: r.get(4)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_ATTRIBUTE) {
            b.biota_properties_attribute = rows(c, "SELECT object_Id, type, init_Level, level_From_C_P, c_P_Spent FROM biota_properties_attribute WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesAttribute { object_id: r.get(0)?, r#type: r.get(1)?, init_level: r.get(2)?, level_from_cp: r.get(3)?, cp_spent: r.get(4)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_ATTRIBUTE_2ND) {
            b.biota_properties_attribute_2nd = rows(c, "SELECT object_Id, type, init_Level, level_From_C_P, c_P_Spent, current_Level FROM biota_properties_attribute_2nd WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesAttribute2nd {
                    object_id: r.get(0)?,
                    r#type: r.get(1)?,
                    init_level: r.get(2)?,
                    level_from_cp: r.get(3)?,
                    cp_spent: r.get(4)?,
                    current_level: r.get(5)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_BODY_PART) {
            b.biota_properties_body_part = rows(c, "SELECT id, object_Id, \"key\", d_Type, d_Val, d_Var, base_Armor, armor_Vs_Slash, armor_Vs_Pierce, armor_Vs_Bludgeon, armor_Vs_Cold, armor_Vs_Fire, armor_Vs_Acid, armor_Vs_Electric, armor_Vs_Nether, b_h, h_l_f, m_l_f, l_l_f, h_r_f, m_r_f, l_r_f, h_l_b, m_l_b, l_l_b, h_r_b, m_r_b, l_r_b FROM biota_properties_body_part WHERE object_Id = ?1 ORDER BY \"key\"", id, |r| {
                Ok(db::BiotaPropertiesBodyPart {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    key: r.get(2)?,
                    d_type: r.get(3)?,
                    d_val: r.get(4)?,
                    d_var: r.get(5)?,
                    base_armor: r.get(6)?,
                    armor_vs_slash: r.get(7)?,
                    armor_vs_pierce: r.get(8)?,
                    armor_vs_bludgeon: r.get(9)?,
                    armor_vs_cold: r.get(10)?,
                    armor_vs_fire: r.get(11)?,
                    armor_vs_acid: r.get(12)?,
                    armor_vs_electric: r.get(13)?,
                    armor_vs_nether: r.get(14)?,
                    bh: r.get(15)?,
                    hlf: r.get(16)?,
                    mlf: r.get(17)?,
                    llf: r.get(18)?,
                    hrf: r.get(19)?,
                    mrf: r.get(20)?,
                    lrf: r.get(21)?,
                    hlb: r.get(22)?,
                    mlb: r.get(23)?,
                    llb: r.get(24)?,
                    hrb: r.get(25)?,
                    mrb: r.get(26)?,
                    lrb: r.get(27)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_BOOK) {
            b.biota_properties_book = rows(c, "SELECT object_Id, max_Num_Pages, max_Num_Chars_Per_Page FROM biota_properties_book WHERE object_Id = ?1", id, |r| {
                Ok(db::BiotaPropertiesBook { object_id: r.get(0)?, max_num_pages: r.get(1)?, max_num_chars_per_page: r.get(2)? })
            })?
            .into_iter()
            .next();
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_BOOK_PAGE_DATA) {
            b.biota_properties_book_page_data = rows(c, "SELECT id, object_Id, page_Id, author_Id, author_Name, author_Account, ignore_Author, page_Text FROM biota_properties_book_page_data WHERE object_Id = ?1 ORDER BY page_Id", id, |r| {
                Ok(db::BiotaPropertiesBookPageData {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    page_id: r.get(2)?,
                    author_id: r.get(3)?,
                    author_name: r.get(4)?,
                    author_account: r.get(5)?,
                    ignore_author: r.get(6)?,
                    page_text: r.get(7)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_BOOL) {
            b.biota_properties_bool = rows(c, "SELECT object_Id, type, value FROM biota_properties_bool WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesBool { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_CREATE_LIST) {
            b.biota_properties_create_list = rows(c, "SELECT id, object_Id, destination_Type, weenie_Class_Id, stack_Size, palette, shade, try_To_Bond FROM biota_properties_create_list WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesCreateList {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    destination_type: r.get(2)?,
                    weenie_class_id: r.get(3)?,
                    stack_size: r.get(4)?,
                    palette: r.get(5)?,
                    shade: r.get(6)?,
                    try_to_bond: r.get(7)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_DID) {
            b.biota_properties_did = rows(c, "SELECT object_Id, type, value FROM biota_properties_d_i_d WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesDID { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_EMOTE) {
            let mut emotes = rows(c, "SELECT id, object_Id, category, probability, weenie_Class_Id, style, substyle, quest, vendor_Type, min_Health, max_Health FROM biota_properties_emote WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesEmote {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    category: r.get(2)?,
                    probability: r.get(3)?,
                    weenie_class_id: r.get(4)?,
                    style: r.get(5)?,
                    substyle: r.get(6)?,
                    quest: r.get(7)?,
                    vendor_type: r.get(8)?,
                    min_health: r.get(9)?,
                    max_health: r.get(10)?,
                    biota_properties_emote_action: Vec::new(),
                })
            })?;
            for emote in &mut emotes {
                emote.biota_properties_emote_action = rows(c, "SELECT id, emote_Id, \"order\", type, delay, extent, motion, message, test_String, min, max, min_64, max_64, min_Dbl, max_Dbl, stat, display, amount, amount_64, hero_X_P_64, percent, spell_Id, wealth_Rating, treasure_Class, treasure_Type, p_Script, sound, destination_Type, weenie_Class_Id, stack_Size, palette, shade, try_To_Bond, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z FROM biota_properties_emote_action WHERE emote_Id = ?1 ORDER BY id", emote.id, |r| {
                    Ok(db::BiotaPropertiesEmoteAction {
                        id: r.get(0)?,
                        emote_id: r.get(1)?,
                        order: r.get(2)?,
                        r#type: r.get(3)?,
                        delay: r.get(4)?,
                        extent: r.get(5)?,
                        motion: r.get(6)?,
                        message: r.get(7)?,
                        test_string: r.get(8)?,
                        min: r.get(9)?,
                        max: r.get(10)?,
                        min_64: r.get(11)?,
                        max_64: r.get(12)?,
                        min_dbl: r.get(13)?,
                        max_dbl: r.get(14)?,
                        stat: r.get(15)?,
                        display: r.get(16)?,
                        amount: r.get(17)?,
                        amount_64: r.get(18)?,
                        hero_xp_64: r.get(19)?,
                        percent: r.get(20)?,
                        spell_id: r.get(21)?,
                        wealth_rating: r.get(22)?,
                        treasure_class: r.get(23)?,
                        treasure_type: r.get(24)?,
                        p_script: r.get(25)?,
                        sound: r.get(26)?,
                        destination_type: r.get(27)?,
                        weenie_class_id: r.get(28)?,
                        stack_size: r.get(29)?,
                        palette: r.get(30)?,
                        shade: r.get(31)?,
                        try_to_bond: r.get(32)?,
                        obj_cell_id: r.get(33)?,
                        origin_x: r.get(34)?,
                        origin_y: r.get(35)?,
                        origin_z: r.get(36)?,
                        angles_w: r.get(37)?,
                        angles_x: r.get(38)?,
                        angles_y: r.get(39)?,
                        angles_z: r.get(40)?,
                    })
                })?;
            }
            b.biota_properties_emote = emotes;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_ENCHANTMENT_REGISTRY) {
            b.biota_properties_enchantment_registry = rows(c, "SELECT object_Id, enchantment_Category, spell_Id, layer_Id, has_Spell_Set_Id, spell_Category, power_Level, start_Time, duration, caster_Object_Id, degrade_Modifier, degrade_Limit, last_Time_Degraded, stat_Mod_Type, stat_Mod_Key, stat_Mod_Value, spell_Set_Id FROM biota_properties_enchantment_registry WHERE object_Id = ?1 ORDER BY spell_Id, caster_Object_Id, layer_Id", id, |r| {
                Ok(db::BiotaPropertiesEnchantmentRegistry {
                    object_id: r.get(0)?,
                    enchantment_category: r.get(1)?,
                    spell_id: r.get(2)?,
                    layer_id: r.get(3)?,
                    has_spell_set_id: r.get(4)?,
                    spell_category: r.get(5)?,
                    power_level: r.get(6)?,
                    start_time: r.get(7)?,
                    duration: r.get(8)?,
                    caster_object_id: r.get(9)?,
                    degrade_modifier: r.get(10)?,
                    degrade_limit: r.get(11)?,
                    last_time_degraded: r.get(12)?,
                    stat_mod_type: r.get(13)?,
                    stat_mod_key: r.get(14)?,
                    stat_mod_value: r.get(15)?,
                    spell_set_id: r.get(16)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_EVENT_FILTER) {
            b.biota_properties_event_filter = rows(c, "SELECT object_Id, event FROM biota_properties_event_filter WHERE object_Id = ?1 ORDER BY event", id, |r| {
                Ok(db::BiotaPropertiesEventFilter { object_id: r.get(0)?, event: r.get(1)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_FLOAT) {
            b.biota_properties_float = rows(c, "SELECT object_Id, type, value FROM biota_properties_float WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesFloat { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_GENERATOR) {
            b.biota_properties_generator = rows(c, "SELECT id, object_Id, probability, weenie_Class_Id, delay, init_Create, max_Create, when_Create, where_Create, stack_Size, palette_Id, shade, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z FROM biota_properties_generator WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesGenerator {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    probability: r.get(2)?,
                    weenie_class_id: r.get(3)?,
                    delay: r.get(4)?,
                    init_create: r.get(5)?,
                    max_create: r.get(6)?,
                    when_create: r.get(7)?,
                    where_create: r.get(8)?,
                    stack_size: r.get(9)?,
                    palette_id: r.get(10)?,
                    shade: r.get(11)?,
                    obj_cell_id: r.get(12)?,
                    origin_x: r.get(13)?,
                    origin_y: r.get(14)?,
                    origin_z: r.get(15)?,
                    angles_w: r.get(16)?,
                    angles_x: r.get(17)?,
                    angles_y: r.get(18)?,
                    angles_z: r.get(19)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_IID) {
            b.biota_properties_iid = rows(c, "SELECT object_Id, type, value FROM biota_properties_i_i_d WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesIID { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_INT) {
            b.biota_properties_int = rows(c, "SELECT object_Id, type, value FROM biota_properties_int WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesInt { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_INT64) {
            b.biota_properties_int64 = rows(c, "SELECT object_Id, type, value FROM biota_properties_int64 WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesInt64 { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_PALETTE) {
            b.biota_properties_palette = rows(c, "SELECT id, object_Id, sub_Palette_Id, \"offset\", length, \"order\" FROM biota_properties_palette WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesPalette {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    sub_palette_id: r.get(2)?,
                    offset: r.get(3)?,
                    length: r.get(4)?,
                    order: r.get(5)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_POSITION) {
            b.biota_properties_position = rows(c, "SELECT object_Id, position_Type, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z FROM biota_properties_position WHERE object_Id = ?1 ORDER BY position_Type", id, |r| {
                Ok(db::BiotaPropertiesPosition {
                    object_id: r.get(0)?,
                    position_type: r.get(1)?,
                    obj_cell_id: r.get(2)?,
                    origin_x: r.get(3)?,
                    origin_y: r.get(4)?,
                    origin_z: r.get(5)?,
                    angles_w: r.get(6)?,
                    angles_x: r.get(7)?,
                    angles_y: r.get(8)?,
                    angles_z: r.get(9)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_SKILL) {
            b.biota_properties_skill = rows(c, "SELECT object_Id, type, level_From_P_P, s_a_c, p_p, init_Level, resistance_At_Last_Check, last_Used_Time FROM biota_properties_skill WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesSkill {
                    object_id: r.get(0)?,
                    r#type: r.get(1)?,
                    level_from_pp: r.get(2)?,
                    sac: r.get(3)?,
                    pp: r.get(4)?,
                    init_level: r.get(5)?,
                    resistance_at_last_check: r.get(6)?,
                    last_used_time: r.get(7)?,
                })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_SPELL_BOOK) {
            b.biota_properties_spell_book = rows(c, "SELECT object_Id, spell, probability FROM biota_properties_spell_book WHERE object_Id = ?1 ORDER BY spell", id, |r| {
                Ok(db::BiotaPropertiesSpellBook { object_id: r.get(0)?, spell: r.get(1)?, probability: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_STRING) {
            b.biota_properties_string = rows(c, "SELECT object_Id, type, value FROM biota_properties_string WHERE object_Id = ?1 ORDER BY type", id, |r| {
                Ok(db::BiotaPropertiesString { object_id: r.get(0)?, r#type: r.get(1)?, value: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_TEXTURE_MAP) {
            b.biota_properties_texture_map = rows(c, "SELECT id, object_Id, \"index\", old_Id, new_Id, \"order\" FROM biota_properties_texture_map WHERE object_Id = ?1 ORDER BY id", id, |r| {
                Ok(db::BiotaPropertiesTextureMap {
                    id: r.get(0)?,
                    object_id: r.get(1)?,
                    index: r.get(2)?,
                    old_id: r.get(3)?,
                    new_id: r.get(4)?,
                    order: r.get(5)?,
                })
            })?;
        }
        if flags.has_flag(F::HOUSE_PERMISSION) {
            b.house_permission = rows(c, "SELECT house_Id, player_Guid, storage FROM house_permission WHERE house_Id = ?1 ORDER BY player_Guid", id, |r| {
                Ok(db::HousePermission { house_id: r.get(0)?, player_guid: r.get(1)?, storage: r.get(2)? })
            })?;
        }
        if flags.has_flag(F::BIOTA_PROPERTIES_ALLEGIANCE) {
            b.biota_properties_allegiance = rows(c, "SELECT allegiance_Id, character_Id, banned, approved_Vassal FROM biota_properties_allegiance WHERE allegiance_Id = ?1 ORDER BY character_Id", id, |r| {
                Ok(db::BiotaPropertiesAllegiance { allegiance_id: r.get(0)?, character_id: r.get(1)?, banned: r.get(2)?, approved_vassal: r.get(3)? })
            })?;
        }
        Ok(())
    }

    fn write_biota(&mut self, biota: &mut Biota) -> Result<(), StoreError> {
        let mut work = biota.clone();
        in_savepoint(&self.conn, |c| write_biota_rows(c, &mut work))?;
        *biota = work;
        Ok(())
    }

    fn delete_biota(&mut self, id: u32) -> Result<(), StoreError> {
        in_savepoint(&self.conn, |c| {
            c.prepare_cached("DELETE FROM biota WHERE id = ?1")?
                .execute([id])?;
            Ok(())
        })
    }

    fn query_biota_ids(&mut self, query: BiotaQuery) -> Result<Vec<u32>, StoreError> {
        let c = &self.conn;
        match query {
            BiotaQuery::IdRange { min, max } => u32s(c, "SELECT id FROM biota WHERE id >= ?1 AND id <= ?2 ORDER BY id", params![min, max]),
            BiotaQuery::WeenieClassId(wcid) => u32s(c, "SELECT id FROM biota WHERE weenie_Class_Id = ?1 ORDER BY id", [wcid]),
            BiotaQuery::WeenieType(t) => u32s(c, "SELECT id FROM biota WHERE weenie_Type = ?1 ORDER BY id", [t]),
            BiotaQuery::InstanceId { r#type, value } => u32s(
                c,
                "SELECT object_Id FROM biota_properties_i_i_d WHERE type = ?1 AND value = ?2 ORDER BY object_Id",
                params![r#type, value],
            ),
            BiotaQuery::LocationInCellRange { min, max } => u32s(
                c,
                "SELECT object_Id FROM biota_properties_position WHERE position_Type = 1 AND obj_Cell_Id >= ?1 AND obj_Cell_Id <= ?2 AND object_Id >= 2147483648 ORDER BY obj_Cell_Id, object_Id",
                params![min, max],
            ),
            BiotaQuery::TypeWithInstanceId { weenie_type, iid_type, iid_value: None } => u32s(
                c,
                "SELECT b.id FROM biota b JOIN biota_properties_i_i_d i ON b.id = i.object_Id WHERE b.weenie_Type = ?1 AND i.type = ?2 ORDER BY b.id",
                params![weenie_type, iid_type],
            ),
            BiotaQuery::TypeWithInstanceId { weenie_type, iid_type, iid_value: Some(v) } => u32s(
                c,
                "SELECT b.id FROM biota b JOIN biota_properties_i_i_d i ON b.id = i.object_Id WHERE b.weenie_Type = ?1 AND i.type = ?2 AND i.value = ?3 ORDER BY b.id",
                params![weenie_type, iid_type, v],
            ),
        }
    }

    fn count_biotas(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM biota", [], |r| r.get(0))?)
    }

    fn query_characters(
        &mut self,
        query: CharacterQuery<'_>,
    ) -> Result<Vec<Character>, StoreError> {
        const COLS: &str = "SELECT id, account_Id, name, is_Plussed, is_Deleted, delete_Time, last_Login_Timestamp, total_Logins, character_Options_1, character_Options_2, gameplay_Options, spellbook_Filters, hair_Texture, default_Hair_Texture FROM \"character\"";
        let map = |r: &Row<'_>| -> rusqlite::Result<Character> {
            Ok(Character {
                id: r.get(0)?,
                account_id: r.get(1)?,
                name: r.get(2)?,
                is_plussed: r.get(3)?,
                is_deleted: r.get(4)?,
                delete_time: r.get::<_, i64>(5)?.cs_cast(),
                last_login_timestamp: r.get(6)?,
                total_logins: r.get(7)?,
                character_options_1: r.get(8)?,
                character_options_2: r.get(9)?,
                gameplay_options: r.get(10)?,
                spellbook_filters: r.get(11)?,
                hair_texture: r.get(12)?,
                default_hair_texture: r.get(13)?,
                ..Default::default()
            })
        };
        let (sql, p): (String, Vec<rusqlite::types::Value>) = match query {
            CharacterQuery::Account { account_id, include_deleted } => (
                format!("{COLS} WHERE account_Id = ?1 AND (?2 OR is_Deleted = 0) ORDER BY id"),
                vec![i64::from(account_id).into(), i64::from(include_deleted).into()],
            ),
            CharacterQuery::Id { id, include_deleted } => (
                format!("{COLS} WHERE id = ?1 AND (?2 OR is_Deleted = 0) ORDER BY id"),
                vec![i64::from(id).into(), i64::from(include_deleted).into()],
            ),
            CharacterQuery::NameNotDeleted(name) => {
                (format!("{COLS} WHERE name = ?1 AND is_Deleted = 0 ORDER BY id"), vec![name.to_owned().into()])
            }
            CharacterQuery::NameAvailable(name) => (
                format!("{COLS} WHERE is_Deleted = 0 AND NOT (delete_Time <> 0) AND name = ?1 ORDER BY id"),
                vec![name.to_owned().into()],
            ),
            CharacterQuery::NotDeleted => (format!("{COLS} WHERE is_Deleted = 0 ORDER BY id"), vec![]),
            CharacterQuery::All => (format!("{COLS} ORDER BY id"), vec![]),
        };
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p), map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn load_character_properties(&mut self, ch: &mut Character) -> Result<(), StoreError> {
        let c = &self.conn;
        let id = ch.id;
        ch.character_properties_contract_registry = rows(c, "SELECT character_Id, contract_Id, delete_Contract, set_As_Display_Contract FROM character_properties_contract_registry WHERE character_Id = ?1 ORDER BY contract_Id", id, |r| {
            Ok(db::CharacterPropertiesContractRegistry {
                character_id: r.get(0)?,
                contract_id: r.get(1)?,
                delete_contract: r.get(2)?,
                set_as_display_contract: r.get(3)?,
            })
        })?;
        ch.character_properties_fill_comp_book = rows(c, "SELECT character_Id, spell_Component_Id, quantity_To_Rebuy FROM character_properties_fill_comp_book WHERE character_Id = ?1 ORDER BY spell_Component_Id", id, |r| {
            Ok(db::CharacterPropertiesFillCompBook { character_id: r.get(0)?, spell_component_id: r.get(1)?, quantity_to_rebuy: r.get(2)? })
        })?;
        ch.character_properties_friend_list = rows(c, "SELECT character_Id, friend_Id FROM character_properties_friend_list WHERE character_Id = ?1 ORDER BY friend_Id", id, |r| {
            Ok(db::CharacterPropertiesFriendList { character_id: r.get(0)?, friend_id: r.get(1)? })
        })?;
        ch.character_properties_quest_registry = rows(c, "SELECT character_Id, quest_Name, last_Time_Completed, num_Times_Completed FROM character_properties_quest_registry WHERE character_Id = ?1 ORDER BY quest_Name", id, |r| {
            Ok(db::CharacterPropertiesQuestRegistry {
                character_id: r.get(0)?,
                quest_name: r.get(1)?,
                last_time_completed: r.get(2)?,
                num_times_completed: r.get(3)?,
            })
        })?;
        ch.character_properties_shortcut_bar = rows(c, "SELECT character_Id, shortcut_Bar_Index, shortcut_Object_Id FROM character_properties_shortcut_bar WHERE character_Id = ?1 ORDER BY shortcut_Bar_Index", id, |r| {
            Ok(db::CharacterPropertiesShortcutBar { character_id: r.get(0)?, shortcut_bar_index: r.get(1)?, shortcut_object_id: r.get(2)? })
        })?;
        ch.character_properties_spell_bar = rows(c, "SELECT character_Id, spell_Bar_Number, spell_Bar_Index, spell_Id FROM character_properties_spell_bar WHERE character_Id = ?1 ORDER BY spell_Bar_Number, spell_Id", id, |r| {
            Ok(db::CharacterPropertiesSpellBar { character_id: r.get(0)?, spell_bar_number: r.get(1)?, spell_bar_index: r.get(2)?, spell_id: r.get(3)? })
        })?;
        ch.character_properties_squelch = rows(c, "SELECT character_Id, squelch_Character_Id, squelch_Account_Id, type FROM character_properties_squelch WHERE character_Id = ?1 ORDER BY squelch_Character_Id", id, |r| {
            Ok(db::CharacterPropertiesSquelch {
                character_id: r.get(0)?,
                squelch_character_id: r.get(1)?,
                squelch_account_id: r.get(2)?,
                r#type: r.get(3)?,
            })
        })?;
        ch.character_properties_title_book = rows(c, "SELECT character_Id, title_Id FROM character_properties_title_book WHERE character_Id = ?1 ORDER BY title_Id", id, |r| {
            Ok(db::CharacterPropertiesTitleBook { character_id: r.get(0)?, title_id: r.get(1)? })
        })?;
        Ok(())
    }

    fn write_character(&mut self, character: &Character) -> Result<(), StoreError> {
        in_savepoint(&self.conn, |c| write_character_rows(c, character))
    }

    fn begin_batch(&mut self) -> Result<(), StoreError> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        self.in_batch = true;
        Ok(())
    }

    fn commit_batch(&mut self) -> Result<(), StoreError> {
        self.in_batch = false;
        if let Err(e) = self.conn.execute_batch("COMMIT") {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(e.into());
        }
        Ok(())
    }

    fn rollback_batch(&mut self) {
        self.in_batch = false;
        let _ = self.conn.execute_batch("ROLLBACK");
    }

    fn delete_character(&mut self, id: u32) -> Result<(), StoreError> {
        in_savepoint(&self.conn, |c| {
            c.prepare_cached("DELETE FROM \"character\" WHERE id = ?1")?
                .execute([id])?;
            Ok(())
        })
    }
}

/// Inserts a row with a surrogate id: explicit when `*id != 0`, otherwise the next one, which is
/// written back.
fn insert_with_id(
    c: &Connection,
    sql_with_id: &str,
    id: &mut u32,
    rest: &[&dyn rusqlite::ToSql],
) -> Result<(), StoreError> {
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(rest.len() + 1);
    let explicit: Option<u32> = if *id == 0 { None } else { Some(*id) };
    p.push(&explicit);
    p.extend_from_slice(rest);
    c.prepare_cached(sql_with_id)?.execute(p.as_slice())?;
    if *id == 0 {
        *id = c.last_insert_rowid().cs_cast();
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn write_biota_rows(c: &Connection, b: &mut Biota) -> Result<(), StoreError> {
    let id = b.id;
    c.prepare_cached(
        "INSERT INTO biota (id, weenie_Class_Id, weenie_Type, populated_Collection_Flags) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET weenie_Class_Id = excluded.weenie_Class_Id, weenie_Type = excluded.weenie_Type, populated_Collection_Flags = excluded.populated_Collection_Flags",
    )?
    .execute(params![id, b.weenie_class_id, b.weenie_type, b.populated_collection_flags])?;

    for table in [
        "biota_properties_anim_part",
        "biota_properties_attribute",
        "biota_properties_attribute_2nd",
        "biota_properties_body_part",
        "biota_properties_book",
        "biota_properties_book_page_data",
        "biota_properties_bool",
        "biota_properties_create_list",
        "biota_properties_d_i_d",
        "biota_properties_emote",
        "biota_properties_enchantment_registry",
        "biota_properties_event_filter",
        "biota_properties_float",
        "biota_properties_generator",
        "biota_properties_i_i_d",
        "biota_properties_int",
        "biota_properties_int64",
        "biota_properties_palette",
        "biota_properties_position",
        "biota_properties_skill",
        "biota_properties_spell_book",
        "biota_properties_string",
        "biota_properties_texture_map",
    ] {
        c.prepare_cached(&format!("DELETE FROM {table} WHERE object_Id = ?1"))?
            .execute([id])?;
    }
    c.prepare_cached("DELETE FROM house_permission WHERE house_Id = ?1")?
        .execute([id])?;
    c.prepare_cached("DELETE FROM biota_properties_allegiance WHERE allegiance_Id = ?1")?
        .execute([id])?;

    for r in &mut b.biota_properties_anim_part {
        insert_with_id(c, "INSERT INTO biota_properties_anim_part (id, object_Id, \"index\", animation_Id, \"order\") VALUES (?1, ?2, ?3, ?4, ?5)", &mut r.id, &[&r.object_id, &r.index, &r.animation_id, &r.order])?;
    }
    for r in &b.biota_properties_attribute {
        c.prepare_cached("INSERT INTO biota_properties_attribute (object_Id, type, init_Level, level_From_C_P, c_P_Spent) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![r.object_id, r.r#type, r.init_level, r.level_from_cp, r.cp_spent])?;
    }
    for r in &b.biota_properties_attribute_2nd {
        c.prepare_cached("INSERT INTO biota_properties_attribute_2nd (object_Id, type, init_Level, level_From_C_P, c_P_Spent, current_Level) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?
            .execute(params![r.object_id, r.r#type, r.init_level, r.level_from_cp, r.cp_spent, r.current_level])?;
    }
    for r in &mut b.biota_properties_body_part {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_body_part (id, object_Id, \"key\", d_Type, d_Val, d_Var, base_Armor, armor_Vs_Slash, armor_Vs_Pierce, armor_Vs_Bludgeon, armor_Vs_Cold, armor_Vs_Fire, armor_Vs_Acid, armor_Vs_Electric, armor_Vs_Nether, b_h, h_l_f, m_l_f, l_l_f, h_r_f, m_r_f, l_r_f, h_l_b, m_l_b, l_l_b, h_r_b, m_r_b, l_r_b) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            &mut r.id,
            &[
                &r.object_id, &r.key, &r.d_type, &r.d_val, &r.d_var, &r.base_armor, &r.armor_vs_slash, &r.armor_vs_pierce,
                &r.armor_vs_bludgeon, &r.armor_vs_cold, &r.armor_vs_fire, &r.armor_vs_acid, &r.armor_vs_electric,
                &r.armor_vs_nether, &r.bh, &r.hlf, &r.mlf, &r.llf, &r.hrf, &r.mrf, &r.lrf, &r.hlb, &r.mlb, &r.llb, &r.hrb,
                &r.mrb, &r.lrb,
            ],
        )?;
    }
    if let Some(r) = &b.biota_properties_book {
        c.prepare_cached("INSERT INTO biota_properties_book (object_Id, max_Num_Pages, max_Num_Chars_Per_Page) VALUES (?1, ?2, ?3)")?
            .execute(params![r.object_id, r.max_num_pages, r.max_num_chars_per_page])?;
    }
    for r in &mut b.biota_properties_book_page_data {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_book_page_data (id, object_Id, page_Id, author_Id, author_Name, author_Account, ignore_Author, page_Text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &mut r.id,
            &[&r.object_id, &r.page_id, &r.author_id, &r.author_name, &r.author_account, &r.ignore_author, &r.page_text],
        )?;
    }
    for r in &b.biota_properties_bool {
        c.prepare_cached(
            "INSERT INTO biota_properties_bool (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &mut b.biota_properties_create_list {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_create_list (id, object_Id, destination_Type, weenie_Class_Id, stack_Size, palette, shade, try_To_Bond) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &mut r.id,
            &[&r.object_id, &r.destination_type, &r.weenie_class_id, &r.stack_size, &r.palette, &r.shade, &r.try_to_bond],
        )?;
    }
    for r in &b.biota_properties_did {
        c.prepare_cached(
            "INSERT INTO biota_properties_d_i_d (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for e in &mut b.biota_properties_emote {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_emote (id, object_Id, category, probability, weenie_Class_Id, style, substyle, quest, vendor_Type, min_Health, max_Health) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            &mut e.id,
            &[&e.object_id, &e.category, &e.probability, &e.weenie_class_id, &e.style, &e.substyle, &e.quest, &e.vendor_type, &e.min_health, &e.max_health],
        )?;
        let emote_id = e.id;
        for a in &mut e.biota_properties_emote_action {
            // Entity Framework sets the foreign key from the parent emote (ConvertFromEntityBiota's
            // placeholder uint.MaxValue included).
            a.emote_id = emote_id;
            insert_with_id(
                c,
                "INSERT INTO biota_properties_emote_action (id, emote_Id, \"order\", type, delay, extent, motion, message, test_String, min, max, min_64, max_64, min_Dbl, max_Dbl, stat, display, amount, amount_64, hero_X_P_64, percent, spell_Id, wealth_Rating, treasure_Class, treasure_Type, p_Script, sound, destination_Type, weenie_Class_Id, stack_Size, palette, shade, try_To_Bond, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41)",
                &mut a.id,
                &[
                    &a.emote_id, &a.order, &a.r#type, &a.delay, &a.extent, &a.motion, &a.message, &a.test_string, &a.min, &a.max,
                    &a.min_64, &a.max_64, &a.min_dbl, &a.max_dbl, &a.stat, &a.display, &a.amount, &a.amount_64, &a.hero_xp_64,
                    &a.percent, &a.spell_id, &a.wealth_rating, &a.treasure_class, &a.treasure_type, &a.p_script, &a.sound,
                    &a.destination_type, &a.weenie_class_id, &a.stack_size, &a.palette, &a.shade, &a.try_to_bond, &a.obj_cell_id,
                    &a.origin_x, &a.origin_y, &a.origin_z, &a.angles_w, &a.angles_x, &a.angles_y, &a.angles_z,
                ],
            )?;
        }
    }
    for r in &b.biota_properties_enchantment_registry {
        c.prepare_cached("INSERT INTO biota_properties_enchantment_registry (object_Id, enchantment_Category, spell_Id, layer_Id, has_Spell_Set_Id, spell_Category, power_Level, start_Time, duration, caster_Object_Id, degrade_Modifier, degrade_Limit, last_Time_Degraded, stat_Mod_Type, stat_Mod_Key, stat_Mod_Value, spell_Set_Id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)")?
            .execute(params![
                r.object_id, r.enchantment_category, r.spell_id, r.layer_id, r.has_spell_set_id, r.spell_category, r.power_level,
                r.start_time, r.duration, r.caster_object_id, r.degrade_modifier, r.degrade_limit, r.last_time_degraded,
                r.stat_mod_type, r.stat_mod_key, r.stat_mod_value, r.spell_set_id
            ])?;
    }
    for r in &b.biota_properties_event_filter {
        c.prepare_cached(
            "INSERT INTO biota_properties_event_filter (object_Id, event) VALUES (?1, ?2)",
        )?
        .execute(params![r.object_id, r.event])?;
    }
    for r in &b.biota_properties_float {
        c.prepare_cached(
            "INSERT INTO biota_properties_float (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &mut b.biota_properties_generator {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_generator (id, object_Id, probability, weenie_Class_Id, delay, init_Create, max_Create, when_Create, where_Create, stack_Size, palette_Id, shade, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            &mut r.id,
            &[
                &r.object_id, &r.probability, &r.weenie_class_id, &r.delay, &r.init_create, &r.max_create, &r.when_create,
                &r.where_create, &r.stack_size, &r.palette_id, &r.shade, &r.obj_cell_id, &r.origin_x, &r.origin_y, &r.origin_z,
                &r.angles_w, &r.angles_x, &r.angles_y, &r.angles_z,
            ],
        )?;
    }
    for r in &b.biota_properties_iid {
        c.prepare_cached(
            "INSERT INTO biota_properties_i_i_d (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &b.biota_properties_int {
        c.prepare_cached(
            "INSERT INTO biota_properties_int (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &b.biota_properties_int64 {
        c.prepare_cached(
            "INSERT INTO biota_properties_int64 (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &mut b.biota_properties_palette {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_palette (id, object_Id, sub_Palette_Id, \"offset\", length, \"order\") VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &mut r.id,
            &[&r.object_id, &r.sub_palette_id, &r.offset, &r.length, &r.order],
        )?;
    }
    for r in &b.biota_properties_position {
        c.prepare_cached("INSERT INTO biota_properties_position (object_Id, position_Type, obj_Cell_Id, origin_X, origin_Y, origin_Z, angles_W, angles_X, angles_Y, angles_Z) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")?
            .execute(params![r.object_id, r.position_type, r.obj_cell_id, r.origin_x, r.origin_y, r.origin_z, r.angles_w, r.angles_x, r.angles_y, r.angles_z])?;
    }
    for r in &b.biota_properties_skill {
        c.prepare_cached("INSERT INTO biota_properties_skill (object_Id, type, level_From_P_P, s_a_c, p_p, init_Level, resistance_At_Last_Check, last_Used_Time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
            .execute(params![r.object_id, r.r#type, r.level_from_pp, r.sac, r.pp, r.init_level, r.resistance_at_last_check, r.last_used_time])?;
    }
    for r in &b.biota_properties_spell_book {
        c.prepare_cached("INSERT INTO biota_properties_spell_book (object_Id, spell, probability) VALUES (?1, ?2, ?3)")?
            .execute(params![r.object_id, r.spell, r.probability])?;
    }
    for r in &b.biota_properties_string {
        c.prepare_cached(
            "INSERT INTO biota_properties_string (object_Id, type, value) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.object_id, r.r#type, r.value])?;
    }
    for r in &mut b.biota_properties_texture_map {
        insert_with_id(
            c,
            "INSERT INTO biota_properties_texture_map (id, object_Id, \"index\", old_Id, new_Id, \"order\") VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &mut r.id,
            &[&r.object_id, &r.index, &r.old_id, &r.new_id, &r.order],
        )?;
    }
    for r in &b.house_permission {
        c.prepare_cached(
            "INSERT INTO house_permission (house_Id, player_Guid, storage) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![r.house_id, r.player_guid, r.storage])?;
    }
    for r in &b.biota_properties_allegiance {
        c.prepare_cached("INSERT INTO biota_properties_allegiance (allegiance_Id, character_Id, banned, approved_Vassal) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![r.allegiance_id, r.character_id, r.banned, r.approved_vassal])?;
    }
    Ok(())
}

fn write_character_rows(c: &Connection, ch: &Character) -> Result<(), StoreError> {
    let id = ch.id;
    let delete_time: i64 = ch.delete_time.cs_cast();
    // An INSERT of spellbook_Filters' CLR default (0) takes the column default 16383, as
    // Entity Framework leaves such a column out of its INSERT; an update writes the value as is.
    c.prepare_cached(
        "INSERT INTO \"character\" (id, account_Id, name, is_Plussed, is_Deleted, delete_Time, last_Login_Timestamp, total_Logins, character_Options_1, character_Options_2, gameplay_Options, spellbook_Filters, hair_Texture, default_Hair_Texture) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, CASE WHEN ?12 = 0 THEN 16383 ELSE ?12 END, ?13, ?14) \
         ON CONFLICT(id) DO UPDATE SET account_Id = excluded.account_Id, name = excluded.name, is_Plussed = excluded.is_Plussed, is_Deleted = excluded.is_Deleted, delete_Time = excluded.delete_Time, last_Login_Timestamp = excluded.last_Login_Timestamp, total_Logins = excluded.total_Logins, character_Options_1 = excluded.character_Options_1, character_Options_2 = excluded.character_Options_2, gameplay_Options = excluded.gameplay_Options, spellbook_Filters = ?12, hair_Texture = excluded.hair_Texture, default_Hair_Texture = excluded.default_Hair_Texture",
    )?
    .execute(params![
        id, ch.account_id, ch.name, ch.is_plussed, ch.is_deleted, delete_time, ch.last_login_timestamp, ch.total_logins,
        ch.character_options_1, ch.character_options_2, ch.gameplay_options, ch.spellbook_filters, ch.hair_texture,
        ch.default_hair_texture
    ])?;

    for table in [
        "character_properties_contract_registry",
        "character_properties_fill_comp_book",
        "character_properties_friend_list",
        "character_properties_quest_registry",
        "character_properties_shortcut_bar",
        "character_properties_spell_bar",
        "character_properties_squelch",
        "character_properties_title_book",
    ] {
        c.prepare_cached(&format!("DELETE FROM {table} WHERE character_Id = ?1"))?
            .execute([id])?;
    }

    // Property rows take the owning character's id (Entity Framework's relationship fix-up).
    for r in &ch.character_properties_contract_registry {
        c.prepare_cached("INSERT INTO character_properties_contract_registry (character_Id, contract_Id, delete_Contract, set_As_Display_Contract) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![id, r.contract_id, r.delete_contract, r.set_as_display_contract])?;
    }
    for r in &ch.character_properties_fill_comp_book {
        c.prepare_cached("INSERT INTO character_properties_fill_comp_book (character_Id, spell_Component_Id, quantity_To_Rebuy) VALUES (?1, ?2, ?3)")?
            .execute(params![id, r.spell_component_id, r.quantity_to_rebuy])?;
    }
    for r in &ch.character_properties_friend_list {
        c.prepare_cached("INSERT INTO character_properties_friend_list (character_Id, friend_Id) VALUES (?1, ?2)")?.execute(params![id, r.friend_id])?;
    }
    for r in &ch.character_properties_quest_registry {
        c.prepare_cached("INSERT INTO character_properties_quest_registry (character_Id, quest_Name, last_Time_Completed, num_Times_Completed) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![id, r.quest_name, r.last_time_completed, r.num_times_completed])?;
    }
    for r in &ch.character_properties_shortcut_bar {
        c.prepare_cached("INSERT INTO character_properties_shortcut_bar (character_Id, shortcut_Bar_Index, shortcut_Object_Id) VALUES (?1, ?2, ?3)")?
            .execute(params![id, r.shortcut_bar_index, r.shortcut_object_id])?;
    }
    for r in &ch.character_properties_spell_bar {
        c.prepare_cached("INSERT INTO character_properties_spell_bar (character_Id, spell_Bar_Number, spell_Bar_Index, spell_Id) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![id, r.spell_bar_number, r.spell_bar_index, r.spell_id])?;
    }
    for r in &ch.character_properties_squelch {
        c.prepare_cached("INSERT INTO character_properties_squelch (character_Id, squelch_Character_Id, squelch_Account_Id, type) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![id, r.squelch_character_id, r.squelch_account_id, r.r#type])?;
    }
    for r in &ch.character_properties_title_book {
        c.prepare_cached(
            "INSERT INTO character_properties_title_book (character_Id, title_Id) VALUES (?1, ?2)",
        )?
        .execute(params![id, r.title_id])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// ShardConfigDatabase
// ---------------------------------------------------------------------------------------------

macro_rules! config_table {
    ($exists:ident, $add:ident, $get:ident, $get_all:ident, $save:ident, $table:literal, $model:ident, $vty:ty) => {
        fn $exists(&mut self, key: &str) -> bool {
            self.conn
                .query_row(concat!("SELECT EXISTS (SELECT 1 FROM ", $table, " WHERE \"key\" = ?1)"), [key], |r| r.get(0))
                .unwrap_or_else(|e| panic!("{e}"))
        }

        fn $add(&mut self, key: &str, value: $vty, description: Option<&str>) {
            self.conn
                .execute(concat!("INSERT INTO ", $table, " (\"key\", value, description) VALUES (?1, ?2, ?3)"), params![key, value, description])
                .unwrap_or_else(|e| panic!("DbUpdateException: {e}"));
        }

        fn $get(&mut self, key: &str) -> Option<db::$model> {
            self.conn
                .query_row(concat!("SELECT \"key\", value, description FROM ", $table, " WHERE \"key\" = ?1"), [key], |r| {
                    Ok(db::$model { key: r.get(0)?, value: r.get(1)?, description: r.get(2)? })
                })
                .optional()
                .unwrap_or_else(|e| panic!("{e}"))
        }

        fn $get_all(&mut self) -> Vec<db::$model> {
            let mut stmt = self
                .conn
                .prepare_cached(concat!("SELECT \"key\", value, description FROM ", $table, " ORDER BY \"key\""))
                .unwrap_or_else(|e| panic!("{e}"));
            stmt.query_map([], |r| Ok(db::$model { key: r.get(0)?, value: r.get(1)?, description: r.get(2)? }))
                .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                .unwrap_or_else(|e| panic!("{e}"))
        }

        fn $save(&mut self, stat: &db::$model) {
            // EntityState.Modified: an UPDATE of every column, which throws when no row matched.
            let n = self
                .conn
                .execute(concat!("UPDATE ", $table, " SET value = ?2, description = ?3 WHERE \"key\" = ?1"), params![stat.key, stat.value, stat.description])
                .unwrap_or_else(|e| panic!("DbUpdateException: {e}"));
            assert!(n == 1, "DbUpdateConcurrencyException: expected to affect 1 row(s), but actually affected {n} row(s)");
        }
    };
}

impl ShardConfigDatabase for SqliteShard {
    config_table!(
        bool_exists,
        add_bool,
        get_bool,
        get_all_bools,
        save_bool,
        "config_properties_boolean",
        ConfigPropertiesBoolean,
        bool
    );
    config_table!(
        long_exists,
        add_long,
        get_long,
        get_all_longs,
        save_long,
        "config_properties_long",
        ConfigPropertiesLong,
        i64
    );
    config_table!(
        double_exists,
        add_double,
        get_double,
        get_all_doubles,
        save_double,
        "config_properties_double",
        ConfigPropertiesDouble,
        f64
    );
    config_table!(
        string_exists,
        add_string,
        get_string,
        get_all_strings,
        save_string,
        "config_properties_string",
        ConfigPropertiesString,
        &str
    );
}
