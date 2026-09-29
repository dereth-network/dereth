// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/ShardDatabaseOfflineTools.cs
//! `ShardDatabaseOfflineTools`: the shard maintenance the server runs at start-up (purges and
//! prunes) and the checks for old schema patches.
//!
//! Every function works on a [`ShardDatabase`] directly (ACE opens a new `ShardDbContext`, not the
//! serialized queue); the server passes `DatabaseManager.Shard.BaseDatabase`.
//!
//! DIVERGE (arch):
//! * ACE's queries over one table (`context.BiotaPropertiesIID.Where(...)` and the like) read the
//!   biotas and characters through the backend primitives instead: every biota by id, every
//!   character by id ([`all_biotas`], [`all_characters`]). The rows found are the same; they come
//!   in primary-key order, which is the order InnoDB returns ACE's unordered queries in.
//! * A `SaveChanges` is one batch transaction (`begin_batch` .. `commit_batch`); a failed commit
//!   is logged where ACE catches the exception.
//! * `Parallel.ForEach` runs its items in order on the calling thread.
//! * `ToLocalTime()` reads as UTC (the store has no time zone), as elsewhere in the port.

use std::collections::{HashMap, HashSet};

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_common::time::Time;
use empyrean_entity::enums::{PositionType, PropertyInstanceId, PropertyString, WeenieType};

use crate::error::StoreError;
use crate::models::shard::{Biota, Character};
use crate::shard_database::{BiotaQuery, CharacterQuery, PopulatedCollectionFlags, ShardDatabase};

// -------------------------------------------------------------------------------------------------
// Table reads (not ACE)
// -------------------------------------------------------------------------------------------------

/// Every biota, with all its child rows, by id (`context.Biota` with its collections).
///
/// # Panics
/// A backend failure (ACE's unhandled EF exception).
pub fn all_biotas(db: &mut dyn ShardDatabase) -> Vec<Biota> {
    let ids = db
        .query_biota_ids(BiotaQuery::IdRange {
            min: 0,
            max: u32::MAX,
        })
        .unwrap_or_else(|e| panic!("{e}"));
    ids.into_iter()
        .filter_map(|id| load_biota(db, id))
        .collect()
}

/// One biota with all its child rows, read past any cache (a fresh context's read).
///
/// # Panics
/// A backend failure.
pub fn load_biota(db: &mut dyn ShardDatabase, id: u32) -> Option<Biota> {
    let mut biota = db.load_biota_row(id).unwrap_or_else(|e| panic!("{e}"))?;
    let flags = PopulatedCollectionFlags(biota.populated_collection_flags);
    db.load_biota_collections(&mut biota, flags)
        .unwrap_or_else(|e| panic!("{e}"));
    Some(biota)
}

/// Every character row, deleted ones too, with its property lists, by id (`context.Character`).
///
/// # Panics
/// A backend failure.
pub fn all_characters(db: &mut dyn ShardDatabase) -> Vec<Character> {
    let mut characters = db
        .query_characters(CharacterQuery::All)
        .unwrap_or_else(|e| panic!("{e}"));
    for character in &mut characters {
        db.load_character_properties(character)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    characters
}

/// `context.SaveChanges()` over `writes`, in one transaction.
///
/// # Errors
/// The first failed write, or the commit; the batch is rolled back.
pub fn save_changes(
    db: &mut dyn ShardDatabase,
    writes: impl FnOnce(&mut dyn ShardDatabase) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    db.begin_batch()?;
    if let Err(e) = writes(db) {
        db.rollback_batch();
        return Err(e);
    }
    db.commit_batch()
}

/// `biota.GetProperty(PropertyString.Name)`.
fn name_of(biota: &Biota) -> Option<&str> {
    biota
        .biota_properties_string
        .iter()
        .find(|r| r.r#type == PropertyString::Name.0)
        .map(|r| r.value.as_str())
}

/// The `object_Id`s of the IID rows of `type` whose value is `value`, by object id.
fn instance_id_objects(
    db: &mut dyn ShardDatabase,
    r#type: PropertyInstanceId,
    value: u32,
) -> Vec<u32> {
    db.query_biota_ids(BiotaQuery::InstanceId {
        r#type: r#type.0,
        value,
    })
    .unwrap_or_else(|e| panic!("{e}"))
}

// -------------------------------------------------------------------------------------------------
// ACE
// -------------------------------------------------------------------------------------------------

// ACE: ShardDatabaseOfflineTools.GetInventoryBiotas
/// The biota rows contained by `parent_id` (with `included_nested_items`, each container's
/// contents follow it).
fn get_inventory_biotas(
    db: &mut dyn ShardDatabase,
    parent_id: u32,
    included_nested_items: bool,
) -> Vec<Biota> {
    let mut inventory = Vec::new();

    let results = instance_id_objects(db, PropertyInstanceId::Container, parent_id);

    for result in results {
        let Some(object) = db.load_biota_row(result).unwrap_or_else(|e| panic!("{e}")) else {
            continue;
        };
        let is_container = object.weenie_type == WeenieType::Container.0.cast_signed();
        let object_id = object.id;
        inventory.push(object);

        if included_nested_items && is_container {
            let sub_items = get_inventory_biotas(db, object_id, false);

            inventory.extend(sub_items);
        }
    }

    inventory
}

// ACE: ShardDatabaseOfflineTools.GetWieldedGuids
fn get_wielded_guids(db: &mut dyn ShardDatabase, parent_id: u32) -> Vec<u32> {
    instance_id_objects(db, PropertyInstanceId::Wielder, parent_id)
}

/// `PurgeCharacter`'s and `PurgePlayer`'s `out` counts: `(charactersPurged, playerBiotasPurged,
/// possessionsPurged)`.
pub type PurgeCounts = (i32, i32, i32);

// ACE: ShardDatabaseOfflineTools.PurgeCharacter
/// Purges a character: its inventory, its wielded items, its player biota, then its character
/// row. Both ACE overloads (with and without a context).
pub fn purge_character(
    db: &mut dyn ShardDatabase,
    character_id: u32,
    reason: Option<&str>,
) -> PurgeCounts {
    let mut characters_purged = 0;
    let mut player_biotas_purged = 0;
    let mut possessions_purged = 0;

    let mut deletes: Vec<u32> = Vec::new();

    // First purge the inventory
    let inventory_biotas = get_inventory_biotas(db, character_id, true);

    for biota in &inventory_biotas {
        deletes.push(biota.id);

        possessions_purged += 1;
    }

    // Then the wielded items
    let wielded_guids = get_wielded_guids(db, character_id);

    for guid in wielded_guids {
        deletes.push(guid);

        possessions_purged += 1;
    }

    // Second to last, the payer biota
    if db
        .load_biota_row(character_id)
        .unwrap_or_else(|e| panic!("{e}"))
        .is_some()
    {
        deletes.push(character_id);

        player_biotas_purged += 1;
    }

    // Lastly, the character record
    let character = db
        .query_characters(CharacterQuery::Id {
            id: character_id,
            include_deleted: true,
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .next();

    if character.is_some() {
        characters_purged += 1;
    }

    let mut message = format!("[DATABASE][PURGE] Character 0x{character_id:08X}");

    if let Some(character) = &character {
        let deleted_on = Time::get_date_time_from_timestamp(character.delete_time.cs_cast());
        message += &format!(
            ":{}, deleted on {}",
            character.name,
            to_common_string(deleted_on)
        );
    }

    message += &format!(", and {possessions_purged} of their possessions has been purged.");

    if let Some(reason) = reason.filter(|r| !r.trim().is_empty()) {
        message += &format!(" Reason: {reason}.");
    }

    log::info!("{message}");

    let result = save_changes(db, |db| {
        for id in &deletes {
            db.delete_biota(*id)?;
        }
        if character.is_some() {
            db.delete_character(character_id)?;
        }
        Ok(())
    });
    if let Err(ex) = result {
        log::error!(
            "[DATABASE][PURGE] PurgeCharacter 0x{character_id:08X} failed with exception: {ex}"
        );
    }

    (characters_purged, player_biotas_purged, possessions_purged)
}

// ACE: ShardDatabaseOfflineTools.PurgeCharactersInParallel
/// Purges the characters deleted more than `days_limiter` days before `now`, and those marked
/// deleted without a delete time. `now` is `DateTime.UtcNow`.
pub fn purge_characters_in_parallel(
    db: &mut dyn ShardDatabase,
    days_limiter: i32,
    now: DotNetDateTime,
) -> PurgeCounts {
    let delete_limit = Time::get_unix_time_at(now.add_days(-f64::from(days_limiter)));
    let delete_limit: u64 = delete_limit.cs_cast();

    let results: Vec<Character> = db
        .query_characters(CharacterQuery::All)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .filter(|r| {
            (r.delete_time > 0 && r.delete_time < delete_limit)
                || (r.is_deleted && r.delete_time == 0)
        })
        .collect();

    let mut characters_purged_total = 0;
    let mut player_biotas_purged_total = 0;
    let mut possessions_purged_total = 0;

    for result in results {
        let (characters_purged_result, player_biotas_purged_result, possessions_purged_result) =
            purge_character(db, result.id, None);

        characters_purged_total += characters_purged_result;
        player_biotas_purged_total += player_biotas_purged_result;
        possessions_purged_total += possessions_purged_result;
    }

    (
        characters_purged_total,
        player_biotas_purged_total,
        possessions_purged_total,
    )
}

// ACE: ShardDatabaseOfflineTools.PurgePlayer
/// Purges a player: its inventory, its wielded items, its character row, then its player biota.
/// Both ACE overloads.
pub fn purge_player(
    db: &mut dyn ShardDatabase,
    player_id: u32,
    reason: Option<&str>,
) -> PurgeCounts {
    let mut characters_purged = 0;
    let mut player_biotas_purged = 0;
    let mut possessions_purged = 0;

    let mut deletes: Vec<u32> = Vec::new();

    // First purge the inventory
    let inventory_biotas = get_inventory_biotas(db, player_id, true);

    for biota in &inventory_biotas {
        deletes.push(biota.id);

        possessions_purged += 1;
    }

    // Then the wielded items
    let wielded_guids = get_wielded_guids(db, player_id);

    for guid in wielded_guids {
        deletes.push(guid);

        possessions_purged += 1;
    }

    // Second to last, the character record
    let has_character = !db
        .query_characters(CharacterQuery::Id {
            id: player_id,
            include_deleted: true,
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .is_empty();
    if has_character {
        characters_purged += 1;
    }

    // Lastly, the payer biota
    let player = load_biota(db, player_id);

    if player.is_some() {
        player_biotas_purged += 1;
    }

    let mut message = format!("[DATABASE][PURGE] Player 0x{player_id:08X}");

    if let Some(player) = &player {
        if let Some(name) = name_of(player).filter(|n| !n.trim().is_empty()) {
            message += &format!(":{name}");
        }
    }

    message += &format!(", and {possessions_purged} of their possessions has been purged.");

    if let Some(reason) = reason.filter(|r| !r.trim().is_empty()) {
        message += &format!(" Reason: {reason}.");
    }

    log::info!("{message}");

    let result = save_changes(db, |db| {
        for id in &deletes {
            db.delete_biota(*id)?;
        }
        if has_character {
            db.delete_character(player_id)?;
        }
        if player.is_some() {
            db.delete_biota(player_id)?;
        }
        Ok(())
    });
    if let Err(ex) = result {
        log::error!("[DATABASE][PURGE] PurgePlayer 0x{player_id:08X} failed with exception: {ex}");
    }

    (characters_purged, player_biotas_purged, possessions_purged)
}

// ACE: ShardDatabaseOfflineTools.PurgeBiota
/// Purges one biota; false when there is none. Both ACE overloads.
pub fn purge_biota(db: &mut dyn ShardDatabase, id: u32, reason: Option<&str>) -> bool {
    let Some(biota) = load_biota(db, id) else {
        return false;
    };

    let mut message = format!("[DATABASE][PURGE] Biota 0x{id:08X}");

    if let Some(name) = name_of(&biota).filter(|n| !n.trim().is_empty()) {
        message += &format!(":{name}");
    }

    message += &format!(
        ", WeenieType: {}",
        WeenieType(biota.weenie_type.cast_unsigned()).to_dotnet_string()
    );

    //if (NonPurgeableWeenieTypes.Contains((WeenieType)biota.WeenieType)) ... (commented out in ACE)

    message += ", has been purged.";

    if let Some(reason) = reason.filter(|r| !r.trim().is_empty()) {
        message += &format!(" Reason: {reason}.");
    }

    log::info!("{message}");

    if let Err(ex) = save_changes(db, |db| db.delete_biota(id)) {
        log::error!("[DATABASE][PURGE] PurgeBiota 0x{id:08X} failed with exception: {ex}");
    }

    true
}

// ACE: ShardDatabaseOfflineTools.PurgeOrphanedBiotasInParallel
/// Purges characters without a player biota, player biotas without a character, items whose
/// container or wielder is gone, objects with no container, wielder or location (allegiances
/// aside), and allegiances whose monarch is gone or is duplicated. Returns the number purged.
pub fn purge_orphaned_biotas_in_parallel(db: &mut dyn ShardDatabase) -> i32 {
    let mut total_number_of_biotas_purged = 0;

    // Purge characters that do not have an associated biota
    // select * from `character` left join biota on biota.id=`character`.id where biota.id is null;
    let player_biota_ids: Vec<u32> = db
        .query_biota_ids(BiotaQuery::IdRange {
            min: 0x5000_0000,
            max: 0x5FFF_FFFF,
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let player_biota_set: HashSet<u32> = to_hash_set(player_biota_ids.iter().copied());

    let character_ids: Vec<u32> = db
        .query_characters(CharacterQuery::All)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .map(|c| c.id)
        .collect();
    let character_set: HashSet<u32> = to_hash_set(character_ids.iter().copied());

    // `Except` keeps the first sequence's (HashSet insertion) order
    let results: Vec<u32> = except(&character_ids, &player_biota_set);

    for result in results {
        let (characters_purged, player_biotas_purged, possession_purged) =
            purge_character(db, result, Some("No Player biota counterpart found"));

        if characters_purged != 1 {
            log::error!("[DATABASE][PURGE] PurgeOrphanedBiotasInParallel failed to purge exactly 1 character. This should not happen!");
        }

        if player_biotas_purged != 0 {
            log::error!("[DATABASE][PURGE] PurgeOrphanedBiotasInParallel purged a player biota and character record. This should not happen!");
        }

        total_number_of_biotas_purged += characters_purged;
        total_number_of_biotas_purged += player_biotas_purged;
        total_number_of_biotas_purged += possession_purged;
    }

    // Purge player biotas that do not have an associated character
    // select * from biota left join `character` on character.id=biota.id where biota.id >= 0x50000000 and biota.id <= 0x5FFFFFFF and character.id is null;
    let results: Vec<u32> = except(&player_biota_ids, &character_set);

    for result in results {
        let (characters_purged, player_biotas_purged, possession_purged) =
            purge_player(db, result, Some("No Character record counterpart found"));

        if characters_purged != 0 {
            log::error!("[DATABASE][PURGE] PurgeOrphanedBiotasInParallel purged a character record and a player biota. This should not happen!");
        }

        if player_biotas_purged != 1 {
            log::error!("[DATABASE][PURGE] PurgeOrphanedBiotasInParallel failed to purge exactly 1 player biota. This should not happen!");
        }

        total_number_of_biotas_purged += characters_purged;
        total_number_of_biotas_purged += player_biotas_purged;
        total_number_of_biotas_purged += possession_purged;
    }

    // Purge contained items that belong to a parent container that no longer exists
    // select * from biota_properties_i_i_d iid left join biota on biota.id=iid.`value` where iid.`type`=2 and biota.id is null;
    let snapshot = all_biotas(db);
    let biotas: Vec<(u32, i32)> = snapshot.iter().map(|b| (b.id, b.weenie_type)).collect();
    let biota_set: HashSet<u32> = to_hash_set(biotas.iter().map(|(id, _)| *id));
    let container_pointers: HashMap<u32, u32> =
        iid_pointers(&snapshot, PropertyInstanceId::Container);
    let container_order: Vec<(u32, u32)> =
        iid_pointer_list(&snapshot, PropertyInstanceId::Container);

    let results: Vec<u32> = container_order
        .iter()
        .filter(|(_, value)| !biota_set.contains(value))
        .map(|(object_id, _)| *object_id)
        .collect();

    for result in results {
        if purge_biota(db, result, Some("Parent container not found")) {
            total_number_of_biotas_purged += 1;
        }
    }

    // Purge wielded items that belong to a parent wielder that no longer exists
    // select * from biota_properties_i_i_d iid left join biota on biota.id=iid.`value` where iid.`type`=3 and biota.id is null;
    let snapshot = all_biotas(db);
    let wielder_pointers: HashMap<u32, u32> = iid_pointers(&snapshot, PropertyInstanceId::Wielder);
    let wielder_order: Vec<(u32, u32)> = iid_pointer_list(&snapshot, PropertyInstanceId::Wielder);

    let results: Vec<u32> = wielder_order
        .iter()
        .filter(|(_, value)| !biota_set.contains(value))
        .map(|(object_id, _)| *object_id)
        .collect();

    for result in results {
        if purge_biota(db, result, Some("Parent wielder not found")) {
            total_number_of_biotas_purged += 1;
        }
    }

    // Purge items that don't have a container, wielder or location
    let snapshot = all_biotas(db);
    let location_pointers: HashSet<u32> = to_hash_set(
        snapshot
            .iter()
            .filter(|b| {
                b.biota_properties_position
                    .iter()
                    .any(|p| p.position_type == PositionType::Location.0)
            })
            .map(|b| b.id),
    );

    let mut results = Vec::new();

    for &(id, weenie_type) in &biotas {
        if weenie_type == WeenieType::Allegiance.0.cast_signed() {
            continue;
        }

        if container_pointers.contains_key(&id)
            || wielder_pointers.contains_key(&id)
            || location_pointers.contains(&id)
        {
            continue;
        }

        results.push(id);
    }

    for result in results {
        if purge_biota(
            db,
            result,
            Some("No parent Container, parent Wielder, or Location"),
        ) {
            total_number_of_biotas_purged += 1;
        }
    }

    // Purge allegiances that have no monarch, or that are unused duplicates
    let snapshot = all_biotas(db);
    let monarchs: HashSet<u32> = snapshot
        .iter()
        .filter(|b| b.weenie_type != WeenieType::Allegiance.0.cast_signed())
        .map(|b| b.id)
        .collect();

    // from biota join monarch (IID Monarch) where biota.WeenieType == Allegiance
    let allegiances: Vec<(u32, u32)> = snapshot
        .iter()
        .filter(|b| b.weenie_type == WeenieType::Allegiance.0.cast_signed())
        .filter_map(|b| {
            b.biota_properties_iid
                .iter()
                .find(|r| r.r#type == PropertyInstanceId::Monarch.0)
                .map(|r| (b.id, r.value))
        })
        .collect();

    let mut unique_monarchs = HashSet::new();
    let mut missing_monarch_allegiances = Vec::new();
    let mut duplicate_allegiances = Vec::new();

    for (allegiance_id, monarch_id) in allegiances {
        // Not ACE's (a fix): a valid monarch is one whose biota still
        // exists (not an allegiance), so an allegiance whose monarch was deleted is purged. ACE's
        // set held every Monarch IID value, the allegiance's own row included, so an allegiance's
        // monarch was always found and "Allegiance has no valid monarch" never fired.
        if !monarchs.contains(&monarch_id) {
            missing_monarch_allegiances.push(allegiance_id);
        } else if !unique_monarchs.insert(monarch_id) {
            duplicate_allegiances.push(allegiance_id);
        }
    }

    for allegiance_id in missing_monarch_allegiances {
        if purge_biota(db, allegiance_id, Some("Allegiance has no valid monarch")) {
            total_number_of_biotas_purged += 1;
        }
    }

    for allegiance_id in duplicate_allegiances {
        if purge_biota(db, allegiance_id, Some("Allegiance is an unused duplicate")) {
            total_number_of_biotas_purged += 1;
        }
    }

    total_number_of_biotas_purged
}

/// `first.Except(second)`: the distinct items of `first` not in `second`, in `first`'s order.
fn except(first: &[u32], second: &HashSet<u32>) -> Vec<u32> {
    let mut seen = HashSet::new();
    first
        .iter()
        .copied()
        .filter(|x| !second.contains(x) && seen.insert(*x))
        .collect()
}

/// `context.BiotaPropertiesIID.Where(r => r.Type == type).ToDictionary(i => i.ObjectId, i => i.Value)`.
fn iid_pointers(biotas: &[Biota], r#type: PropertyInstanceId) -> HashMap<u32, u32> {
    iid_pointer_list(biotas, r#type).into_iter().collect()
}

/// The IID rows of `type` as `(object_Id, value)`, by object id.
fn iid_pointer_list(biotas: &[Biota], r#type: PropertyInstanceId) -> Vec<(u32, u32)> {
    biotas
        .iter()
        .flat_map(|b| {
            b.biota_properties_iid
                .iter()
                .filter(move |r| r.r#type == r#type.0)
                .map(move |r| (b.id, r.value))
        })
        .collect()
}

// ACE: ShardDatabaseOfflineTools.FixAnimPartAndTextureMapFromPR2731
/// Removes the ordered anim part and texture map rows of every biota that also has unordered ones
/// (PR 2731's duplicated rows). Returns the number of rows removed.
pub fn fix_anim_part_and_texture_map_from_pr2731(db: &mut dyn ShardDatabase) -> i32 {
    let mut number_of_records_fixed = 0;

    let mut biotas = all_biotas(db);

    let anim_part_null_records: HashSet<u32> = biotas
        .iter()
        .filter(|b| {
            b.biota_properties_anim_part
                .iter()
                .any(|r| r.order.is_none())
        })
        .map(|b| b.id)
        .collect();
    let texture_map_null_records: HashSet<u32> = biotas
        .iter()
        .filter(|b| {
            b.biota_properties_texture_map
                .iter()
                .any(|r| r.order.is_none())
        })
        .map(|b| b.id)
        .collect();

    let mut changed = Vec::new();
    for b in &mut biotas {
        let mut touched = false;
        if anim_part_null_records.contains(&b.id) {
            let before = b.biota_properties_anim_part.len();
            b.biota_properties_anim_part.retain(|r| r.order.is_none());
            let removed = before - b.biota_properties_anim_part.len();
            number_of_records_fixed += i32::try_from(removed).unwrap_or(i32::MAX);
            touched |= removed > 0;
        }
        if texture_map_null_records.contains(&b.id) {
            let before = b.biota_properties_texture_map.len();
            b.biota_properties_texture_map.retain(|r| r.order.is_none());
            let removed = before - b.biota_properties_texture_map.len();
            number_of_records_fixed += i32::try_from(removed).unwrap_or(i32::MAX);
            touched |= removed > 0;
        }
        if touched {
            changed.push(b.clone());
        }
    }

    // (ACE's SaveChanges here is not wrapped: an exception propagates)
    save_changes(db, |db| {
        for mut b in changed {
            crate::shard_database::set_biota_populated_collections(&mut b);
            db.write_biota(&mut b)?;
        }
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{e}"));

    number_of_records_fixed
}

// ACE: ShardDatabaseOfflineTools.CheckForPR2918Script
/// Checks that the 2020-04-11 spell bar patch was applied: spell bar rows numbered 0 mean it was
/// not, and ACE exits the process.
pub fn check_for_pr2918_script(db: &mut dyn ShardDatabase) {
    log::info!("Checking for 2020-04-11-00-Update-Character-SpellBars.sql patch");

    let characters = all_characters(db);
    let character_spell_bars_not_fixed = characters
        .iter()
        .flat_map(|c| c.character_properties_spell_bar.iter())
        .filter(|c| c.spell_bar_number == 0)
        .count();

    if character_spell_bars_not_fixed > 0 {
        log::warn!("2020-04-11-00-Update-Character-SpellBars.sql patch not yet applied. Please apply this patch ASAP! Skipping FixSpellBarsPR2918 for now...");
        log::error!("2020-04-11-00-Update-Character-SpellBars.sql patch not yet applied. You must apply this patch before proceeding further...");
        std::process::exit(1);
    }

    log::info!(
        "2020-04-11-00-Update-Character-SpellBars.sql patch has been successfully installed. Before opening world to players, make sure you've run fix-spell-bars command from console"
    );
}

// ACE: ShardDatabaseOfflineTools.CheckForBiotaPropertiesPaletteOrderColumnInShard
/// ACE reads a palette row and, when MySQL reports the `order` column missing, adds it (or exits).
/// DIVERGE (forced): the column is part of this store's schema (`shard_v001.sql`), so the read
/// cannot fail and there is nothing to repair.
pub fn check_for_biota_properties_palette_order_column_in_shard(_db: &mut dyn ShardDatabase) {}

/// `context.SaveChanges()` after a prune: the changed characters, rewritten whole.
fn write_pruned(db: &mut dyn ShardDatabase, characters: Vec<Character>) {
    save_changes(db, |db| {
        for c in &characters {
            db.write_character(c)?;
        }
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{e}"));
}

/// `validCharacterIds`: the characters neither deleted nor pending deletion.
fn valid_character_ids(characters: &[Character]) -> HashSet<u32> {
    characters
        .iter()
        .filter(|c| !c.is_deleted && c.delete_time == 0)
        .map(|c| c.id)
        .collect()
}

// ACE: ShardDatabaseOfflineTools.PruneDeletedCharactersFromFriendLists
/// Removes friends that are deleted (or pending deletion, or gone). Returns the number removed.
pub fn prune_deleted_characters_from_friend_lists(db: &mut dyn ShardDatabase) -> i32 {
    let mut number_of_records_fixed = 0;

    let mut characters = all_characters(db);
    let valid_character_ids = valid_character_ids(&characters);

    let mut changed = Vec::new();
    for c in &mut characters {
        let before = c.character_properties_friend_list.len();
        c.character_properties_friend_list.retain(|invalid_friend| {
            if valid_character_ids.contains(&invalid_friend.friend_id) {
                return true;
            }
            log::info!(
                "[PRUNE] Character 0x{:08X} had 0x{:08X} for a friend, which is not found in database, and has been removed from their friends list.",
                invalid_friend.character_id,
                invalid_friend.friend_id
            );
            number_of_records_fixed += 1;
            false
        });
        if c.character_properties_friend_list.len() != before {
            changed.push(c.clone());
        }
    }

    write_pruned(db, changed);

    number_of_records_fixed
}

// ACE: ShardDatabaseOfflineTools.PruneDeletedObjectsFromShortcutBars
/// Removes shortcuts to objects that are gone. Returns the number removed.
pub fn prune_deleted_objects_from_shortcut_bars(db: &mut dyn ShardDatabase) -> i32 {
    let mut number_of_records_fixed = 0;

    let valid_object_ids: HashSet<u32> = to_hash_set(
        db.query_biota_ids(BiotaQuery::IdRange {
            min: 0,
            max: u32::MAX,
        })
        .unwrap_or_else(|e| panic!("{e}")),
    );

    let mut characters = all_characters(db);

    let mut changed = Vec::new();
    for c in &mut characters {
        let before = c.character_properties_shortcut_bar.len();
        c.character_properties_shortcut_bar.retain(|invalid_shortcut| {
            if valid_object_ids.contains(&invalid_shortcut.shortcut_object_id) {
                return true;
            }
            log::info!(
                "[PRUNE] Character 0x{:08X} had 0x{:08X} as a shortcut (in position {}), which is not found in database, and has been removed from their shortcut bar.",
                invalid_shortcut.character_id,
                invalid_shortcut.shortcut_object_id,
                invalid_shortcut.shortcut_bar_index
            );
            number_of_records_fixed += 1;
            false
        });
        if c.character_properties_shortcut_bar.len() != before {
            changed.push(c.clone());
        }
    }

    write_pruned(db, changed);

    number_of_records_fixed
}

// ACE: ShardDatabaseOfflineTools.PruneDeletedCharactersFromSquelchLists
/// Removes character squelches of characters that are deleted (or pending, or gone). Returns the
/// number removed.
pub fn prune_deleted_characters_from_squelch_lists(db: &mut dyn ShardDatabase) -> i32 {
    let mut number_of_records_fixed = 0;

    let mut characters = all_characters(db);
    let valid_character_ids = valid_character_ids(&characters);

    let mut changed = Vec::new();
    for c in &mut characters {
        let before = c.character_properties_squelch.len();
        c.character_properties_squelch.retain(|invalid_squelch_character| {
            if invalid_squelch_character.squelch_account_id != 0 || valid_character_ids.contains(&invalid_squelch_character.squelch_character_id) {
                return true;
            }
            log::info!(
                "[PRUNE] Character 0x{:08X} had 0x{:08X} squelched, which is not found in database, and has been removed from their squelch list.",
                invalid_squelch_character.character_id,
                invalid_squelch_character.squelch_character_id
            );
            number_of_records_fixed += 1;
            false
        });
        if c.character_properties_squelch.len() != before {
            changed.push(c.clone());
        }
    }

    write_pruned(db, changed);

    number_of_records_fixed
}

// ACE: ShardDatabaseOfflineTools.ToHashSet
fn to_hash_set<T: std::hash::Hash + Eq>(source: impl IntoIterator<Item = T>) -> HashSet<T> {
    source.into_iter().collect()
}
