//! `MemShard`: the shard database in plain Rust collections, for tests. Not ACE.
//!
//! It implements the same backend primitives as [`crate::SqliteShard`] with the same row order
//! (primary-key or index order; see [`BiotaQuery`]), the same name and key matching (the
//! [`crate::collation`] of the SQLite schema), and these of SQLite's constraints: the primary and
//! unique keys within one biota's child rows, the NOT NULL book-page text columns, and the allegiance-to-character foreign key.
//! It does not check surrogate-id collisions between different biotas (the SQLite backend does).
//!
//! A batch keeps what it changes: the first time it touches a biota or a character it keeps that
//! row's value before the batch, and it copies the small tables (the id sequences and the server
//! configuration) whole; a rollback puts all of it back. Copying the whole store at every batch
//! instead would cost each save time in proportion to the shard, milliseconds for a few thousand
//! biotas, inline on the world thread of a test server. [`MemShard::fail_next_commits`],
//! [`MemShard::fail_writes_of`] and [`MemShard::fail_reads_of`] inject failures for tests.

use std::collections::{BTreeMap, HashSet};

use crate::collation::{self, CollationKey};
use crate::error::StoreError;
use crate::models::shard as db;
use crate::models::shard::{Biota, Character};
use crate::shard_config_database::ShardConfigDatabase;
use crate::shard_database::{BiotaQuery, CharacterQuery, PopulatedCollectionFlags, ShardDatabase};

/// The name/key collation's key (MariaDB's `utf8mb4_uca1400_ai_ci`, as the SQLite schema has it).
fn coll_key(s: &str) -> CollationKey {
    CollationKey::new(s)
}

/// What a batch changed, to put back on rollback.
#[derive(Debug, Default)]
struct Batch {
    /// Each biota the batch touched, as it was before (`None`: it did not exist).
    biotas: BTreeMap<u32, Option<Biota>>,
    /// Each character the batch touched, as it was before.
    characters: BTreeMap<u32, Option<Character>>,
    sequences: BTreeMap<&'static str, u32>,
    config_bool: BTreeMap<CollationKey, db::ConfigPropertiesBoolean>,
    config_long: BTreeMap<CollationKey, db::ConfigPropertiesLong>,
    config_double: BTreeMap<CollationKey, db::ConfigPropertiesDouble>,
    config_string: BTreeMap<CollationKey, db::ConfigPropertiesString>,
}

#[derive(Debug, Clone, Default)]
struct State {
    biotas: BTreeMap<u32, Biota>,
    characters: BTreeMap<u32, Character>,
    /// The last surrogate id handed out per table (AUTOINCREMENT: never reused).
    sequences: BTreeMap<&'static str, u32>,
    config_bool: BTreeMap<CollationKey, db::ConfigPropertiesBoolean>,
    config_long: BTreeMap<CollationKey, db::ConfigPropertiesLong>,
    config_double: BTreeMap<CollationKey, db::ConfigPropertiesDouble>,
    config_string: BTreeMap<CollationKey, db::ConfigPropertiesString>,
}

/// The in-memory shard database.
#[derive(Debug, Default)]
pub struct MemShard {
    state: State,
    batch: Option<Batch>,
    fail_commits: u32,
    fail_writes: HashSet<u32>,
    fail_reads: HashSet<u32>,
}

impl MemShard {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next `n` batch commits fail (the batch is rolled back).
    pub fn fail_next_commits(&mut self, n: u32) {
        self.fail_commits = n;
    }

    /// Makes every write of biota `id` fail with a constraint error (until cleared).
    pub fn fail_writes_of(&mut self, id: u32, fail: bool) {
        if fail {
            self.fail_writes.insert(id);
        } else {
            self.fail_writes.remove(&id);
        }
    }

    /// Makes every read of biota `id` fail with a backend error (until cleared).
    pub fn fail_reads_of(&mut self, id: u32, fail: bool) {
        if fail {
            self.fail_reads.insert(id);
        } else {
            self.fail_reads.remove(&id);
        }
    }

    /// Whether a batch is open.
    #[must_use]
    pub fn in_batch(&self) -> bool {
        self.batch.is_some()
    }

    /// Keeps biota `id` as it was before the open batch (the first time the batch touches it).
    fn touch_biota(&mut self, id: u32) {
        if let Some(b) = &mut self.batch {
            b.biotas
                .entry(id)
                .or_insert_with(|| self.state.biotas.get(&id).cloned());
        }
    }

    /// Keeps character `id` as it was before the open batch.
    fn touch_character(&mut self, id: u32) {
        if let Some(b) = &mut self.batch {
            b.characters
                .entry(id)
                .or_insert_with(|| self.state.characters.get(&id).cloned());
        }
    }

    /// Every stored biota with all its rows, by id (for tests).
    #[must_use]
    pub fn stored_biotas(&self) -> Vec<Biota> {
        self.state.biotas.values().cloned().collect()
    }

    fn next_id(&mut self, table: &'static str, id: &mut u32) {
        let seq = self.state.sequences.entry(table).or_insert(0);
        if *id == 0 {
            *seq += 1;
            *id = *seq;
        } else if *id > *seq {
            *seq = *id;
        }
    }
}

fn check_unique<T, K: std::hash::Hash + Eq>(
    rows: &[T],
    table: &str,
    key: impl Fn(&T) -> K,
) -> Result<(), StoreError> {
    let mut seen = HashSet::new();
    for r in rows {
        if !seen.insert(key(r)) {
            return Err(StoreError::Constraint(format!(
                "UNIQUE constraint failed: {table}"
            )));
        }
    }
    Ok(())
}

/// The primary/unique keys and NOT NULL columns SQLite would enforce on one biota's rows.
fn check_biota(b: &Biota, characters: &BTreeMap<u32, Character>) -> Result<(), StoreError> {
    check_unique(&b.biota_properties_bool, "biota_properties_bool", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_did, "biota_properties_d_i_d", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_float, "biota_properties_float", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_iid, "biota_properties_i_i_d", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_int, "biota_properties_int", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_int64, "biota_properties_int64", |r| {
        r.r#type
    })?;
    check_unique(&b.biota_properties_string, "biota_properties_string", |r| {
        r.r#type
    })?;
    check_unique(
        &b.biota_properties_attribute,
        "biota_properties_attribute",
        |r| r.r#type,
    )?;
    check_unique(
        &b.biota_properties_attribute_2nd,
        "biota_properties_attribute_2nd",
        |r| r.r#type,
    )?;
    check_unique(&b.biota_properties_skill, "biota_properties_skill", |r| {
        r.r#type
    })?;
    check_unique(
        &b.biota_properties_position,
        "biota_properties_position",
        |r| r.position_type,
    )?;
    check_unique(
        &b.biota_properties_spell_book,
        "biota_properties_spell_book",
        |r| r.spell,
    )?;
    check_unique(
        &b.biota_properties_event_filter,
        "biota_properties_event_filter",
        |r| r.event,
    )?;
    check_unique(
        &b.biota_properties_enchantment_registry,
        "biota_properties_enchantment_registry",
        |r| (r.spell_id, r.caster_object_id, r.layer_id),
    )?;
    check_unique(
        &b.biota_properties_enchantment_registry,
        "biota_properties_enchantment_registry",
        |r| (r.spell_id, r.layer_id),
    )?;
    check_unique(
        &b.biota_properties_body_part,
        "biota_properties_body_part",
        |r| r.key,
    )?;
    check_unique(
        &b.biota_properties_book_page_data,
        "biota_properties_book_page_data",
        |r| r.page_id,
    )?;
    check_unique(&b.house_permission, "house_permission", |r| r.player_guid)?;
    check_unique(
        &b.biota_properties_allegiance,
        "biota_properties_allegiance",
        |r| r.character_id,
    )?;
    for e in &b.biota_properties_emote {
        check_unique(
            &e.biota_properties_emote_action,
            "biota_properties_emote_action",
            |r| r.order,
        )?;
    }
    for p in &b.biota_properties_book_page_data {
        if p.author_name.is_none() || p.author_account.is_none() || p.page_text.is_none() {
            return Err(StoreError::Constraint(
                "NOT NULL constraint failed: biota_properties_book_page_data".into(),
            ));
        }
    }
    for a in &b.biota_properties_allegiance {
        if !characters.contains_key(&a.character_id) {
            return Err(StoreError::Constraint(
                "FOREIGN KEY constraint failed".into(),
            ));
        }
    }
    Ok(())
}

/// Puts every child list in the order the SQLite backend reads it back.
fn sort_rows(b: &mut Biota) {
    b.biota_properties_anim_part.sort_by_key(|r| r.id);
    b.biota_properties_attribute.sort_by_key(|r| r.r#type);
    b.biota_properties_attribute_2nd.sort_by_key(|r| r.r#type);
    b.biota_properties_body_part.sort_by_key(|r| r.key);
    b.biota_properties_book_page_data.sort_by_key(|r| r.page_id);
    b.biota_properties_bool.sort_by_key(|r| r.r#type);
    b.biota_properties_create_list.sort_by_key(|r| r.id);
    b.biota_properties_did.sort_by_key(|r| r.r#type);
    b.biota_properties_emote.sort_by_key(|r| r.id);
    for e in &mut b.biota_properties_emote {
        e.biota_properties_emote_action.sort_by_key(|r| r.id);
    }
    b.biota_properties_enchantment_registry
        .sort_by_key(|r| (r.spell_id, r.caster_object_id, r.layer_id));
    b.biota_properties_event_filter.sort_by_key(|r| r.event);
    b.biota_properties_float.sort_by_key(|r| r.r#type);
    b.biota_properties_generator.sort_by_key(|r| r.id);
    b.biota_properties_iid.sort_by_key(|r| r.r#type);
    b.biota_properties_int.sort_by_key(|r| r.r#type);
    b.biota_properties_int64.sort_by_key(|r| r.r#type);
    b.biota_properties_palette.sort_by_key(|r| r.id);
    b.biota_properties_position.sort_by_key(|r| r.position_type);
    b.biota_properties_skill.sort_by_key(|r| r.r#type);
    b.biota_properties_spell_book.sort_by_key(|r| r.spell);
    b.biota_properties_string.sort_by_key(|r| r.r#type);
    b.biota_properties_texture_map.sort_by_key(|r| r.id);
    b.house_permission.sort_by_key(|r| r.player_guid);
    b.biota_properties_allegiance
        .sort_by_key(|r| r.character_id);
}

impl ShardDatabase for MemShard {
    fn load_biota_row(&mut self, id: u32) -> Result<Option<Biota>, StoreError> {
        if self.fail_reads.contains(&id) {
            return Err(StoreError::Injected("load_biota_row"));
        }
        Ok(self.state.biotas.get(&id).map(|b| Biota {
            id: b.id,
            weenie_class_id: b.weenie_class_id,
            weenie_type: b.weenie_type,
            populated_collection_flags: b.populated_collection_flags,
            ..Default::default()
        }))
    }

    fn load_biota_collections(
        &mut self,
        b: &mut Biota,
        flags: PopulatedCollectionFlags,
    ) -> Result<(), StoreError> {
        type F = PopulatedCollectionFlags;
        let Some(s) = self.state.biotas.get(&b.id) else {
            return Ok(());
        };
        macro_rules! take {
            ($flag:ident, $field:ident) => {
                if flags.has_flag(F::$flag) {
                    b.$field = s.$field.clone();
                }
            };
        }
        take!(BIOTA_PROPERTIES_ANIM_PART, biota_properties_anim_part);
        take!(BIOTA_PROPERTIES_ATTRIBUTE, biota_properties_attribute);
        take!(
            BIOTA_PROPERTIES_ATTRIBUTE_2ND,
            biota_properties_attribute_2nd
        );
        take!(BIOTA_PROPERTIES_BODY_PART, biota_properties_body_part);
        take!(BIOTA_PROPERTIES_BOOK, biota_properties_book);
        take!(
            BIOTA_PROPERTIES_BOOK_PAGE_DATA,
            biota_properties_book_page_data
        );
        take!(BIOTA_PROPERTIES_BOOL, biota_properties_bool);
        take!(BIOTA_PROPERTIES_CREATE_LIST, biota_properties_create_list);
        take!(BIOTA_PROPERTIES_DID, biota_properties_did);
        take!(BIOTA_PROPERTIES_EMOTE, biota_properties_emote);
        take!(
            BIOTA_PROPERTIES_ENCHANTMENT_REGISTRY,
            biota_properties_enchantment_registry
        );
        take!(BIOTA_PROPERTIES_EVENT_FILTER, biota_properties_event_filter);
        take!(BIOTA_PROPERTIES_FLOAT, biota_properties_float);
        take!(BIOTA_PROPERTIES_GENERATOR, biota_properties_generator);
        take!(BIOTA_PROPERTIES_IID, biota_properties_iid);
        take!(BIOTA_PROPERTIES_INT, biota_properties_int);
        take!(BIOTA_PROPERTIES_INT64, biota_properties_int64);
        take!(BIOTA_PROPERTIES_PALETTE, biota_properties_palette);
        take!(BIOTA_PROPERTIES_POSITION, biota_properties_position);
        take!(BIOTA_PROPERTIES_SKILL, biota_properties_skill);
        take!(BIOTA_PROPERTIES_SPELL_BOOK, biota_properties_spell_book);
        take!(BIOTA_PROPERTIES_STRING, biota_properties_string);
        take!(BIOTA_PROPERTIES_TEXTURE_MAP, biota_properties_texture_map);
        take!(HOUSE_PERMISSION, house_permission);
        take!(BIOTA_PROPERTIES_ALLEGIANCE, biota_properties_allegiance);
        Ok(())
    }

    fn write_biota(&mut self, biota: &mut Biota) -> Result<(), StoreError> {
        if self.fail_writes.contains(&biota.id) {
            return Err(StoreError::Injected("write_biota"));
        }
        check_biota(biota, &self.state.characters)?;

        let mut work = biota.clone();
        for r in &mut work.biota_properties_anim_part {
            self.next_id("biota_properties_anim_part", &mut r.id);
        }
        for r in &mut work.biota_properties_body_part {
            self.next_id("biota_properties_body_part", &mut r.id);
        }
        for r in &mut work.biota_properties_book_page_data {
            self.next_id("biota_properties_book_page_data", &mut r.id);
        }
        for r in &mut work.biota_properties_create_list {
            self.next_id("biota_properties_create_list", &mut r.id);
        }
        for e in &mut work.biota_properties_emote {
            self.next_id("biota_properties_emote", &mut e.id);
            let emote_id = e.id;
            for a in &mut e.biota_properties_emote_action {
                a.emote_id = emote_id;
                self.next_id("biota_properties_emote_action", &mut a.id);
            }
        }
        for r in &mut work.biota_properties_generator {
            self.next_id("biota_properties_generator", &mut r.id);
        }
        for r in &mut work.biota_properties_palette {
            self.next_id("biota_properties_palette", &mut r.id);
        }
        for r in &mut work.biota_properties_texture_map {
            self.next_id("biota_properties_texture_map", &mut r.id);
        }

        let mut stored = work.clone();
        sort_rows(&mut stored);
        self.touch_biota(stored.id);
        self.state.biotas.insert(stored.id, stored);
        *biota = work;
        Ok(())
    }

    fn delete_biota(&mut self, id: u32) -> Result<(), StoreError> {
        if self.fail_writes.contains(&id) {
            return Err(StoreError::Injected("delete_biota"));
        }
        self.touch_biota(id);
        self.state.biotas.remove(&id);
        Ok(())
    }

    fn query_biota_ids(&mut self, query: BiotaQuery) -> Result<Vec<u32>, StoreError> {
        let biotas = &self.state.biotas;
        Ok(match query {
            BiotaQuery::IdRange { min, max } => {
                biotas.range(min..=max).map(|(id, _)| *id).collect()
            }
            BiotaQuery::WeenieClassId(wcid) => biotas
                .values()
                .filter(|b| b.weenie_class_id == wcid)
                .map(|b| b.id)
                .collect(),
            BiotaQuery::WeenieType(t) => biotas
                .values()
                .filter(|b| b.weenie_type == t)
                .map(|b| b.id)
                .collect(),
            BiotaQuery::InstanceId { r#type, value } => biotas
                .values()
                .filter(|b| {
                    b.biota_properties_iid
                        .iter()
                        .any(|r| r.r#type == r#type && r.value == value)
                })
                .map(|b| b.id)
                .collect(),
            BiotaQuery::LocationInCellRange { min, max } => {
                let mut hits: Vec<(u32, u32)> = biotas
                    .values()
                    .filter(|b| b.id >= 0x8000_0000)
                    .flat_map(|b| {
                        b.biota_properties_position
                            .iter()
                            .filter(move |p| {
                                p.position_type == 1 && p.obj_cell_id >= min && p.obj_cell_id <= max
                            })
                            .map(|p| (p.obj_cell_id, p.object_id))
                    })
                    .collect();
                hits.sort_unstable();
                hits.into_iter().map(|(_, id)| id).collect()
            }
            BiotaQuery::TypeWithInstanceId {
                weenie_type,
                iid_type,
                iid_value,
            } => biotas
                .values()
                .filter(|b| {
                    b.weenie_type == weenie_type
                        && b.biota_properties_iid
                            .iter()
                            .any(|r| r.r#type == iid_type && iid_value.is_none_or(|v| r.value == v))
                })
                .map(|b| b.id)
                .collect(),
        })
    }

    fn count_biotas(&mut self) -> Result<i64, StoreError> {
        Ok(i64::try_from(self.state.biotas.len()).unwrap_or(i64::MAX))
    }

    fn query_characters(
        &mut self,
        query: CharacterQuery<'_>,
    ) -> Result<Vec<Character>, StoreError> {
        let stub = |c: &Character| Character {
            character_properties_contract_registry: Vec::new(),
            character_properties_fill_comp_book: Vec::new(),
            character_properties_friend_list: Vec::new(),
            character_properties_quest_registry: Vec::new(),
            character_properties_shortcut_bar: Vec::new(),
            character_properties_spell_bar: Vec::new(),
            character_properties_squelch: Vec::new(),
            character_properties_title_book: Vec::new(),
            ..c.clone()
        };
        let matches = |c: &Character| match query {
            CharacterQuery::Account {
                account_id,
                include_deleted,
            } => c.account_id == account_id && (include_deleted || !c.is_deleted),
            CharacterQuery::Id {
                id,
                include_deleted,
            } => c.id == id && (include_deleted || !c.is_deleted),
            CharacterQuery::NameNotDeleted(name) => collation::eq(&c.name, name) && !c.is_deleted,
            CharacterQuery::NameAvailable(name) => {
                !c.is_deleted && c.delete_time == 0 && collation::eq(&c.name, name)
            }
            CharacterQuery::NotDeleted => !c.is_deleted,
            CharacterQuery::All => true,
        };
        Ok(self
            .state
            .characters
            .values()
            .filter(|c| matches(c))
            .map(stub)
            .collect())
    }

    fn load_character_properties(&mut self, ch: &mut Character) -> Result<(), StoreError> {
        if let Some(s) = self.state.characters.get(&ch.id) {
            ch.character_properties_contract_registry =
                s.character_properties_contract_registry.clone();
            ch.character_properties_fill_comp_book = s.character_properties_fill_comp_book.clone();
            ch.character_properties_friend_list = s.character_properties_friend_list.clone();
            ch.character_properties_quest_registry = s.character_properties_quest_registry.clone();
            ch.character_properties_shortcut_bar = s.character_properties_shortcut_bar.clone();
            ch.character_properties_spell_bar = s.character_properties_spell_bar.clone();
            ch.character_properties_squelch = s.character_properties_squelch.clone();
            ch.character_properties_title_book = s.character_properties_title_book.clone();
        } else {
            ch.character_properties_contract_registry.clear();
            ch.character_properties_fill_comp_book.clear();
            ch.character_properties_friend_list.clear();
            ch.character_properties_quest_registry.clear();
            ch.character_properties_shortcut_bar.clear();
            ch.character_properties_spell_bar.clear();
            ch.character_properties_squelch.clear();
            ch.character_properties_title_book.clear();
        }
        Ok(())
    }

    fn write_character(&mut self, character: &Character) -> Result<(), StoreError> {
        let mut c = character.clone();
        let id = c.id;
        macro_rules! own {
            ($field:ident) => {
                for r in &mut c.$field {
                    r.character_id = id;
                }
            };
        }
        own!(character_properties_contract_registry);
        own!(character_properties_fill_comp_book);
        own!(character_properties_friend_list);
        own!(character_properties_quest_registry);
        own!(character_properties_shortcut_bar);
        own!(character_properties_spell_bar);
        own!(character_properties_squelch);
        own!(character_properties_title_book);

        check_unique(
            &c.character_properties_contract_registry,
            "character_properties_contract_registry",
            |r| r.contract_id,
        )?;
        check_unique(
            &c.character_properties_fill_comp_book,
            "character_properties_fill_comp_book",
            |r| r.spell_component_id,
        )?;
        check_unique(
            &c.character_properties_friend_list,
            "character_properties_friend_list",
            |r| r.friend_id,
        )?;
        check_unique(
            &c.character_properties_quest_registry,
            "character_properties_quest_registry",
            |r| coll_key(&r.quest_name),
        )?;
        check_unique(
            &c.character_properties_shortcut_bar,
            "character_properties_shortcut_bar",
            |r| r.shortcut_bar_index,
        )?;
        check_unique(
            &c.character_properties_spell_bar,
            "character_properties_spell_bar",
            |r| (r.spell_bar_number, r.spell_id),
        )?;
        check_unique(
            &c.character_properties_squelch,
            "character_properties_squelch",
            |r| r.squelch_character_id,
        )?;
        check_unique(
            &c.character_properties_title_book,
            "character_properties_title_book",
            |r| r.title_id,
        )?;

        c.character_properties_contract_registry
            .sort_by_key(|r| r.contract_id);
        c.character_properties_fill_comp_book
            .sort_by_key(|r| r.spell_component_id);
        c.character_properties_friend_list
            .sort_by_key(|r| r.friend_id);
        c.character_properties_quest_registry
            .sort_by_key(|r| coll_key(&r.quest_name));
        c.character_properties_shortcut_bar
            .sort_by_key(|r| r.shortcut_bar_index);
        c.character_properties_spell_bar
            .sort_by_key(|r| (r.spell_bar_number, r.spell_id));
        c.character_properties_squelch
            .sort_by_key(|r| r.squelch_character_id);
        c.character_properties_title_book
            .sort_by_key(|r| r.title_id);

        // An insert of the CLR default gets the column default, as Entity Framework
        // leaves it out of the INSERT (see `Character::default`); an update writes the value.
        if c.spellbook_filters == 0 && !self.state.characters.contains_key(&id) {
            c.spellbook_filters = crate::models::shard::SPELLBOOK_FILTERS_DEFAULT;
        }

        self.touch_character(id);
        self.state.characters.insert(id, c);
        Ok(())
    }

    fn begin_batch(&mut self) -> Result<(), StoreError> {
        let s = &self.state;
        self.batch = Some(Batch {
            biotas: BTreeMap::new(),
            characters: BTreeMap::new(),
            sequences: s.sequences.clone(),
            config_bool: s.config_bool.clone(),
            config_long: s.config_long.clone(),
            config_double: s.config_double.clone(),
            config_string: s.config_string.clone(),
        });
        Ok(())
    }

    fn commit_batch(&mut self) -> Result<(), StoreError> {
        if self.fail_commits > 0 {
            self.fail_commits -= 1;
            self.rollback_batch();
            return Err(StoreError::Injected("commit_batch"));
        }
        self.batch = None;
        Ok(())
    }

    fn rollback_batch(&mut self) {
        let Some(b) = self.batch.take() else { return };
        let s = &mut self.state;
        for (id, before) in b.biotas {
            match before {
                Some(v) => s.biotas.insert(id, v),
                None => s.biotas.remove(&id),
            };
        }
        for (id, before) in b.characters {
            match before {
                Some(v) => s.characters.insert(id, v),
                None => s.characters.remove(&id),
            };
        }
        s.sequences = b.sequences;
        s.config_bool = b.config_bool;
        s.config_long = b.config_long;
        s.config_double = b.config_double;
        s.config_string = b.config_string;
    }

    fn delete_character(&mut self, id: u32) -> Result<(), StoreError> {
        if self.fail_writes.contains(&id) {
            return Err(StoreError::Injected("delete_character"));
        }
        self.touch_character(id);
        self.state.characters.remove(&id);
        // ON DELETE CASCADE: biota_properties_allegiance.character_Id
        let cascaded: Vec<u32> = self
            .state
            .biotas
            .values()
            .filter(|b| {
                b.biota_properties_allegiance
                    .iter()
                    .any(|r| r.character_id == id)
            })
            .map(|b| b.id)
            .collect();
        for biota in cascaded {
            self.touch_biota(biota);
            if let Some(b) = self.state.biotas.get_mut(&biota) {
                b.biota_properties_allegiance
                    .retain(|r| r.character_id != id);
            }
        }
        Ok(())
    }
}

macro_rules! mem_config {
    ($exists:ident, $add:ident, $get:ident, $get_all:ident, $save:ident, $map:ident, $model:ident, $vty:ty, $conv:expr) => {
        fn $exists(&mut self, key: &str) -> bool {
            self.state.$map.contains_key(&coll_key(key))
        }

        fn $add(&mut self, key: &str, value: $vty, description: Option<&str>) {
            let k = coll_key(key);
            assert!(!self.state.$map.contains_key(&k), "DbUpdateException: UNIQUE constraint failed: config key {key}");
            let conv = $conv;
            self.state.$map.insert(k, db::$model { key: key.to_owned(), value: conv(value), description: description.map(str::to_owned) });
        }

        fn $get(&mut self, key: &str) -> Option<db::$model> {
            self.state.$map.get(&coll_key(key)).cloned()
        }

        fn $get_all(&mut self) -> Vec<db::$model> {
            self.state.$map.values().cloned().collect()
        }

        fn $save(&mut self, stat: &db::$model) {
            let row = self
                .state
                .$map
                .get_mut(&coll_key(&stat.key))
                .expect("DbUpdateConcurrencyException: expected to affect 1 row(s), but actually affected 0 row(s)");
            row.value = stat.value.clone();
            row.description = stat.description.clone();
        }
    };
}

impl ShardConfigDatabase for MemShard {
    mem_config!(
        bool_exists,
        add_bool,
        get_bool,
        get_all_bools,
        save_bool,
        config_bool,
        ConfigPropertiesBoolean,
        bool,
        |v| v
    );
    mem_config!(
        long_exists,
        add_long,
        get_long,
        get_all_longs,
        save_long,
        config_long,
        ConfigPropertiesLong,
        i64,
        |v| v
    );
    mem_config!(
        double_exists,
        add_double,
        get_double,
        get_all_doubles,
        save_double,
        config_double,
        ConfigPropertiesDouble,
        f64,
        |v| v
    );
    mem_config!(
        string_exists,
        add_string,
        get_string,
        get_all_strings,
        save_string,
        config_string,
        ConfigPropertiesString,
        &str,
        |v: &str| v.to_owned()
    );
}
