-- Ported from ACE (ACEmulator), AGPL-3.0: Database/Base/ShardBase.sql
-- (and the Entity Framework mapping in Source/ACE.Database/Models/Shard/ShardDbContext.cs).
--
-- empyrean-store shard schema, version 1: ACE's shard database for SQLite.
--
-- Table and column names are exactly ACE's (`weenie_Class_Id`, `biota_properties_d_i_d`), as are
-- the primary keys, unique keys, secondary indexes and ON DELETE CASCADE foreign keys (enabled per
-- connection with PRAGMA foreign_keys = ON). Type mapping, MySQL -> SQLite:
--   int/smallint/tinyint/bigint [unsigned], bit(1) -> INTEGER   (bit(1) as 0/1)
--   float, double                                  -> REAL      (a MySQL float round-trips exactly)
--   varchar(n), text                               -> TEXT      (no length limits)
--   blob, varbinary(n)                             -> BLOB
--
-- Divergences from ShardBase.sql (receipt 1.2 lists them all):
--   S1. Collation. ACE's tables are utf8mb4 with the server's default collation, which on the
--       MariaDB 12.3 ACE runs against is utf8mb4_uca1400_ai_ci (case- and accent-insensitive,
--       PAD SPACE). The columns ACE compares or keys by name (`character.name`,
--       `character_properties_quest_registry.quest_Name`, `config_properties_*.key`) use the same
--       collation here, registered on every connection by empyrean-store (`collation::register`); the
--       other text columns, which ACE never compares in SQL, keep SQLite's BINARY.
--   S2. AUTO_INCREMENT is SQLite's INTEGER PRIMARY KEY AUTOINCREMENT (ids are never reused, as
--       InnoDB's are not).
--   S3. `character.delete_Time` is bigint unsigned in MySQL; it is stored as the same 64 bits in a
--       signed INTEGER.
--   S4. Table comments are dropped (SQLite has none); column comments are kept below as SQL comments
--       where ACE's add information.

CREATE TABLE biota (
    id                         INTEGER NOT NULL PRIMARY KEY,           -- Unique Object Id within the Shard
    weenie_Class_Id            INTEGER NOT NULL,                       -- Weenie Class Id of the Weenie this Biota was created from
    weenie_Type                INTEGER NOT NULL DEFAULT 0,             -- WeenieType for this Object
    populated_Collection_Flags INTEGER NOT NULL DEFAULT 4294967295
);
CREATE INDEX biota_wcid_idx ON biota (weenie_Class_Id);
CREATE INDEX biota_type_idx ON biota (weenie_Type);

CREATE TABLE biota_properties_allegiance (
    allegiance_Id   INTEGER NOT NULL REFERENCES biota (id) ON DELETE CASCADE,
    character_Id    INTEGER NOT NULL REFERENCES "character" (id) ON DELETE CASCADE,
    banned          INTEGER NOT NULL,
    approved_Vassal INTEGER NOT NULL,
    PRIMARY KEY (allegiance_Id, character_Id)
);
CREATE INDEX FK_allegiance_character_Id ON biota_properties_allegiance (character_Id);

CREATE TABLE biota_properties_anim_part (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id    INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    "index"      INTEGER NOT NULL,
    animation_Id INTEGER NOT NULL,
    "order"      INTEGER DEFAULT NULL
);
CREATE INDEX wcid_animpart_idx ON biota_properties_anim_part (object_Id);

CREATE TABLE biota_properties_attribute (
    object_Id      INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type           INTEGER NOT NULL DEFAULT 0,  -- PropertyAttribute
    init_Level     INTEGER NOT NULL DEFAULT 0,  -- innate points
    level_From_C_P INTEGER NOT NULL DEFAULT 0,  -- points raised
    c_P_Spent      INTEGER NOT NULL DEFAULT 0,  -- XP spent on this attribute
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_attribute_2nd (
    object_Id      INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type           INTEGER NOT NULL DEFAULT 0,  -- PropertyAttribute2nd
    init_Level     INTEGER NOT NULL DEFAULT 0,
    level_From_C_P INTEGER NOT NULL DEFAULT 0,
    c_P_Spent      INTEGER NOT NULL DEFAULT 0,
    current_Level  INTEGER NOT NULL DEFAULT 0,  -- current value of the vital
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_body_part (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id         INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    "key"             INTEGER NOT NULL DEFAULT 0,
    d_Type            INTEGER NOT NULL DEFAULT 0,
    d_Val             INTEGER NOT NULL DEFAULT 0,
    d_Var             REAL NOT NULL DEFAULT 0,
    base_Armor        INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Slash    INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Pierce   INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Bludgeon INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Cold     INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Fire     INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Acid     INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Electric INTEGER NOT NULL DEFAULT 0,
    armor_Vs_Nether   INTEGER NOT NULL DEFAULT 0,
    b_h               INTEGER NOT NULL DEFAULT 0,
    h_l_f             REAL NOT NULL DEFAULT 0,
    m_l_f             REAL NOT NULL DEFAULT 0,
    l_l_f             REAL NOT NULL DEFAULT 0,
    h_r_f             REAL NOT NULL DEFAULT 0,
    m_r_f             REAL NOT NULL DEFAULT 0,
    l_r_f             REAL NOT NULL DEFAULT 0,
    h_l_b             REAL NOT NULL DEFAULT 0,
    m_l_b             REAL NOT NULL DEFAULT 0,
    l_l_b             REAL NOT NULL DEFAULT 0,
    h_r_b             REAL NOT NULL DEFAULT 0,
    m_r_b             REAL NOT NULL DEFAULT 0,
    l_r_b             REAL NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX wcid_bodypart_type_uidx ON biota_properties_body_part (object_Id, "key");

CREATE TABLE biota_properties_book (
    object_Id              INTEGER NOT NULL DEFAULT 0 PRIMARY KEY REFERENCES biota (id) ON DELETE CASCADE,
    max_Num_Pages          INTEGER NOT NULL DEFAULT 0,  -- Maximum number of pages per book
    max_Num_Chars_Per_Page INTEGER NOT NULL DEFAULT 0   -- Maximum number of characters per page
);

CREATE TABLE biota_properties_book_page_data (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id      INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    page_Id        INTEGER NOT NULL DEFAULT 0,
    author_Id      INTEGER NOT NULL DEFAULT 0,
    author_Name    TEXT NOT NULL DEFAULT '',
    author_Account TEXT NOT NULL DEFAULT 'prewritten',
    ignore_Author  INTEGER NOT NULL,           -- if this is true, any character in the world can change the page
    page_Text      TEXT NOT NULL
);
CREATE UNIQUE INDEX wcid_pageid_uidx ON biota_properties_book_page_data (object_Id, page_Id);

CREATE TABLE biota_properties_bool (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     INTEGER NOT NULL,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_create_list (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id        INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    destination_Type INTEGER NOT NULL DEFAULT 0,
    weenie_Class_Id  INTEGER NOT NULL DEFAULT 0,
    stack_Size       INTEGER NOT NULL DEFAULT 0,  -- -1 = infinite
    palette          INTEGER NOT NULL DEFAULT 0,
    shade            REAL NOT NULL DEFAULT 0,
    try_To_Bond      INTEGER NOT NULL
);
CREATE INDEX wcid_createlist ON biota_properties_create_list (object_Id);

CREATE TABLE biota_properties_d_i_d (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_emote (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id       INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    category        INTEGER NOT NULL DEFAULT 0,
    probability     REAL NOT NULL DEFAULT 0,
    weenie_Class_Id INTEGER DEFAULT NULL,
    style           INTEGER DEFAULT NULL,
    substyle        INTEGER DEFAULT NULL,
    quest           TEXT,
    vendor_Type     INTEGER DEFAULT NULL,
    min_Health      REAL DEFAULT NULL,
    max_Health      REAL DEFAULT NULL
);
CREATE INDEX wcid_emote ON biota_properties_emote (object_Id);

CREATE TABLE biota_properties_emote_action (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    emote_Id         INTEGER NOT NULL DEFAULT 0 REFERENCES biota_properties_emote (id) ON DELETE CASCADE,
    "order"          INTEGER NOT NULL DEFAULT 0,
    type             INTEGER NOT NULL DEFAULT 0,
    delay            REAL NOT NULL DEFAULT 0,
    extent           REAL NOT NULL DEFAULT 0,
    motion           INTEGER DEFAULT NULL,
    message          TEXT,
    test_String      TEXT,
    min              INTEGER DEFAULT NULL,
    max              INTEGER DEFAULT NULL,
    min_64           INTEGER DEFAULT NULL,
    max_64           INTEGER DEFAULT NULL,
    min_Dbl          REAL DEFAULT NULL,
    max_Dbl          REAL DEFAULT NULL,
    stat             INTEGER DEFAULT NULL,
    display          INTEGER DEFAULT NULL,
    amount           INTEGER DEFAULT NULL,
    amount_64        INTEGER DEFAULT NULL,
    hero_X_P_64      INTEGER DEFAULT NULL,
    percent          REAL DEFAULT NULL,
    spell_Id         INTEGER DEFAULT NULL,
    wealth_Rating    INTEGER DEFAULT NULL,
    treasure_Class   INTEGER DEFAULT NULL,
    treasure_Type    INTEGER DEFAULT NULL,
    p_Script         INTEGER DEFAULT NULL,
    sound            INTEGER DEFAULT NULL,
    destination_Type INTEGER DEFAULT NULL,
    weenie_Class_Id  INTEGER DEFAULT NULL,
    stack_Size       INTEGER DEFAULT NULL,
    palette          INTEGER DEFAULT NULL,
    shade            REAL DEFAULT NULL,
    try_To_Bond      INTEGER DEFAULT NULL,
    obj_Cell_Id      INTEGER DEFAULT NULL,
    origin_X         REAL DEFAULT NULL,
    origin_Y         REAL DEFAULT NULL,
    origin_Z         REAL DEFAULT NULL,
    angles_W         REAL DEFAULT NULL,
    angles_X         REAL DEFAULT NULL,
    angles_Y         REAL DEFAULT NULL,
    angles_Z         REAL DEFAULT NULL
);
CREATE UNIQUE INDEX wcid_category_set_order_uidx ON biota_properties_emote_action (emote_Id, "order");

CREATE TABLE biota_properties_enchantment_registry (
    object_Id            INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    enchantment_Category INTEGER NOT NULL DEFAULT 0,
    spell_Id             INTEGER NOT NULL DEFAULT 0,
    layer_Id             INTEGER NOT NULL DEFAULT 0,
    has_Spell_Set_Id     INTEGER NOT NULL,
    spell_Category       INTEGER NOT NULL DEFAULT 0,
    power_Level          INTEGER NOT NULL DEFAULT 0,
    start_Time           REAL NOT NULL DEFAULT 0,
    duration             REAL NOT NULL DEFAULT 0,
    caster_Object_Id     INTEGER NOT NULL DEFAULT 0,
    degrade_Modifier     REAL NOT NULL DEFAULT 0,
    degrade_Limit        REAL NOT NULL DEFAULT 0,
    last_Time_Degraded   REAL NOT NULL DEFAULT 0,
    stat_Mod_Type        INTEGER NOT NULL DEFAULT 0,
    stat_Mod_Key         INTEGER NOT NULL DEFAULT 0,
    stat_Mod_Value       REAL NOT NULL DEFAULT 0,
    spell_Set_Id         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, spell_Id, caster_Object_Id, layer_Id)
);
CREATE UNIQUE INDEX wcid_enchantmentregistry_objectId_spellId_layerId_uidx
    ON biota_properties_enchantment_registry (object_Id, spell_Id, layer_Id);

CREATE TABLE biota_properties_event_filter (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    event     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, event)
);

CREATE TABLE biota_properties_float (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_generator (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id       INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    probability     REAL NOT NULL DEFAULT 0,
    weenie_Class_Id INTEGER NOT NULL DEFAULT 0,
    delay           REAL DEFAULT 0,
    init_Create     INTEGER NOT NULL DEFAULT 0,
    max_Create      INTEGER NOT NULL DEFAULT 0,
    when_Create     INTEGER NOT NULL DEFAULT 0,
    where_Create    INTEGER NOT NULL DEFAULT 0,
    stack_Size      INTEGER DEFAULT NULL,
    palette_Id      INTEGER DEFAULT NULL,
    shade           REAL DEFAULT NULL,
    obj_Cell_Id     INTEGER DEFAULT NULL,
    origin_X        REAL DEFAULT NULL,
    origin_Y        REAL DEFAULT NULL,
    origin_Z        REAL DEFAULT NULL,
    angles_W        REAL DEFAULT NULL,
    angles_X        REAL DEFAULT NULL,
    angles_Y        REAL DEFAULT NULL,
    angles_Z        REAL DEFAULT NULL
);
CREATE INDEX wcid_generator ON biota_properties_generator (object_Id);

CREATE TABLE biota_properties_i_i_d (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);
CREATE INDEX type_value_idx ON biota_properties_i_i_d (type, value);

CREATE TABLE biota_properties_int (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_int64 (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_palette (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id      INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    sub_Palette_Id INTEGER NOT NULL,
    "offset"       INTEGER NOT NULL,
    length         INTEGER NOT NULL,
    "order"        INTEGER DEFAULT NULL
);
CREATE INDEX wcid_palette_idx ON biota_properties_palette (object_Id);

CREATE TABLE biota_properties_position (
    object_Id     INTEGER NOT NULL REFERENCES biota (id) ON DELETE CASCADE,
    position_Type INTEGER NOT NULL,
    obj_Cell_Id   INTEGER NOT NULL,
    origin_X      REAL NOT NULL,
    origin_Y      REAL NOT NULL,
    origin_Z      REAL NOT NULL,
    angles_W      REAL NOT NULL,
    angles_X      REAL NOT NULL,
    angles_Y      REAL NOT NULL,
    angles_Z      REAL NOT NULL,
    PRIMARY KEY (object_Id, position_Type)
);
CREATE INDEX type_cell_idx ON biota_properties_position (position_Type, obj_Cell_Id);

CREATE TABLE biota_properties_skill (
    object_Id                INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type                     INTEGER NOT NULL DEFAULT 0,
    level_From_P_P           INTEGER NOT NULL DEFAULT 0,  -- points raised
    s_a_c                    INTEGER NOT NULL DEFAULT 0,  -- skill state
    p_p                      INTEGER NOT NULL DEFAULT 0,  -- XP spent on this skill
    init_Level               INTEGER NOT NULL DEFAULT 0,
    resistance_At_Last_Check INTEGER NOT NULL DEFAULT 0,
    last_Used_Time           REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_spell_book (
    object_Id   INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    spell       INTEGER NOT NULL DEFAULT 0,
    probability REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (object_Id, spell)
);

CREATE TABLE biota_properties_string (
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    type      INTEGER NOT NULL DEFAULT 0,
    value     TEXT NOT NULL,
    PRIMARY KEY (object_Id, type)
);

CREATE TABLE biota_properties_texture_map (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    object_Id INTEGER NOT NULL DEFAULT 0 REFERENCES biota (id) ON DELETE CASCADE,
    "index"   INTEGER NOT NULL,
    old_Id    INTEGER NOT NULL,
    new_Id    INTEGER NOT NULL,
    "order"   INTEGER DEFAULT NULL
);
CREATE INDEX wcid_texturemap_idx ON biota_properties_texture_map (object_Id);

CREATE TABLE "character" (
    id                   INTEGER NOT NULL PRIMARY KEY,         -- Id of the Biota for this Character
    account_Id           INTEGER NOT NULL DEFAULT 0,
    name                 TEXT NOT NULL COLLATE utf8mb4_uca1400_ai_ci,  -- S1
    is_Plussed           INTEGER NOT NULL,
    is_Deleted           INTEGER NOT NULL,
    delete_Time          INTEGER NOT NULL DEFAULT 0,           -- S3; marked IsDeleted after this timestamp
    last_Login_Timestamp REAL NOT NULL DEFAULT 0,
    total_Logins         INTEGER NOT NULL DEFAULT 0,
    character_Options_1  INTEGER NOT NULL DEFAULT 0,
    character_Options_2  INTEGER NOT NULL DEFAULT 0,
    gameplay_Options     BLOB,
    spellbook_Filters    INTEGER NOT NULL DEFAULT 16383,
    hair_Texture         INTEGER NOT NULL DEFAULT 0,
    default_Hair_Texture INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX character_account_idx ON "character" (account_Id);
CREATE INDEX character_name_idx ON "character" (name);

CREATE TABLE character_properties_contract_registry (
    character_Id            INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    contract_Id             INTEGER NOT NULL,
    delete_Contract         INTEGER NOT NULL,
    set_As_Display_Contract INTEGER NOT NULL,
    PRIMARY KEY (character_Id, contract_Id)
);

CREATE TABLE character_properties_fill_comp_book (
    character_Id       INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    spell_Component_Id INTEGER NOT NULL DEFAULT 0,
    quantity_To_Rebuy  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, spell_Component_Id)
);

CREATE TABLE character_properties_friend_list (
    character_Id INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    friend_Id    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, friend_Id)
);

CREATE TABLE character_properties_quest_registry (
    character_Id        INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    quest_Name          TEXT NOT NULL COLLATE utf8mb4_uca1400_ai_ci,  -- S1
    last_Time_Completed INTEGER NOT NULL DEFAULT 0,
    num_Times_Completed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, quest_Name)
);

CREATE TABLE character_properties_shortcut_bar (
    character_Id       INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    shortcut_Bar_Index INTEGER NOT NULL DEFAULT 0,
    shortcut_Object_Id INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, shortcut_Bar_Index)
);
CREATE INDEX wcid_shortcutbar_idx ON character_properties_shortcut_bar (character_Id);

CREATE TABLE character_properties_spell_bar (
    character_Id     INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    spell_Bar_Number INTEGER NOT NULL DEFAULT 0,
    spell_Bar_Index  INTEGER NOT NULL DEFAULT 0,
    spell_Id         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, spell_Bar_Number, spell_Id)
);
CREATE INDEX spellBar_idx ON character_properties_spell_bar (spell_Bar_Index);

CREATE TABLE character_properties_squelch (
    character_Id         INTEGER NOT NULL REFERENCES "character" (id) ON DELETE CASCADE,
    squelch_Character_Id INTEGER NOT NULL,
    squelch_Account_Id   INTEGER NOT NULL,
    type                 INTEGER NOT NULL,
    PRIMARY KEY (character_Id, squelch_Character_Id)
);

CREATE TABLE character_properties_title_book (
    character_Id INTEGER NOT NULL DEFAULT 0 REFERENCES "character" (id) ON DELETE CASCADE,
    title_Id     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_Id, title_Id)
);

CREATE TABLE config_properties_boolean (
    "key"       TEXT NOT NULL PRIMARY KEY COLLATE utf8mb4_uca1400_ai_ci,  -- S1
    value       INTEGER NOT NULL,
    description TEXT
);

CREATE TABLE config_properties_double (
    "key"       TEXT NOT NULL PRIMARY KEY COLLATE utf8mb4_uca1400_ai_ci,
    value       REAL NOT NULL,
    description TEXT
);

CREATE TABLE config_properties_long (
    "key"       TEXT NOT NULL PRIMARY KEY COLLATE utf8mb4_uca1400_ai_ci,
    value       INTEGER NOT NULL,
    description TEXT
);

CREATE TABLE config_properties_string (
    "key"       TEXT NOT NULL PRIMARY KEY COLLATE utf8mb4_uca1400_ai_ci,
    value       TEXT NOT NULL,
    description TEXT
);

CREATE TABLE house_permission (
    house_Id    INTEGER NOT NULL REFERENCES biota (id) ON DELETE CASCADE,  -- GUID of House Biota Object
    player_Guid INTEGER NOT NULL,                                        -- GUID of the Player granted permission
    storage     INTEGER NOT NULL,                                        -- Permission includes access to House Storage
    PRIMARY KEY (house_Id, player_Guid)
);
CREATE INDEX biota_Id_house_Id_idx ON house_permission (house_Id);
