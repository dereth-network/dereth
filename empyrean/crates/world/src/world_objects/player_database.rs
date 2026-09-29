// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Database.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Database.cs`.
//!
//! The player's saves. The shard gets owned snapshots of the biotas and of the `Character`,
//! taken on the world thread in ACE's order; the callbacks run on the world thread
//! (persistence runs on the world thread), so ACE's `CharacterDatabaseLock` is not needed.

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::PropertyInt;
use empyrean_entity::ObjectGuid;

use crate::World;

/// Non-property fields declared in `Player_Database.cs`.
#[derive(Debug, Default)]
pub struct PlayerDatabaseFields {
    /// `DateTime.MinValue` until the first character save.
    // ACE: Player.CharacterLastRequestedDatabaseSave
    pub character_last_requested_database_save: DotNetDateTime,
    /// This variable is set to true when a change is made, and set to false before a save is
    /// requested. The primary use for this is to trigger save on add/modify/remove of properties.
    // ACE: Player.CharacterChangesDetected
    pub character_changes_detected: bool,
    /// Set to true when SaveCharacter() returns a failure.
    // ACE: Player.CharacterSaveFailed
    pub character_save_failed: bool,
    /// Set to true when SaveBiotaToDatabase() returns a failure.
    // ACE: Player.BiotaSaveFailed
    pub biota_save_failed: bool,
}

/// Default to 5 minutes.
// ACE: Player.DefaultPlayerSaveIntervalSecs
pub const DEFAULT_PLAYER_SAVE_INTERVAL_SECS: i64 = 300;

/// The time period between automatic saving of player character changes.
// ACE: Player.PlayerSaveIntervalSecs
#[must_use]
pub fn player_save_interval_secs(w: &World) -> i64 {
    crate::managers::property_manager::get_long(
        w,
        "player_save_interval",
        DEFAULT_PLAYER_SAVE_INTERVAL_SECS,
        true,
    )
    .item
}

/// `LogoffTimestamp`, and the loyalty and leadership offline players use for passup rates.
// ACE: Player.SetPropertiesAtLogOut
pub fn set_properties_at_log_out(w: &mut World, this: ObjectGuid) {
    let now = w.now.unix_time;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    o.set_logoff_timestamp(Some(now));
    // These properties are used with offline players to determine passup rates
    // `(int)GetCreatureSkill(Skill.Loyalty).Current`
    let loyalty: i32 =
        crate::world_objects::creature_skills::get_current_loyalty(w, this).cs_cast();
    if let Some(o) = w.objects.get_mut(this) {
        o.set_property(PropertyInt::CurrentLoyaltyAtLastLogoff, loyalty);
    }
    let leadership: i32 =
        crate::world_objects::creature_skills::get_current_leadership(w, this).cs_cast();
    if let Some(o) = w.objects.get_mut(this) {
        o.set_property(PropertyInt::CurrentLeadershipAtLastLogoff, leadership);
    }
}

/// This will make sure a player save happens no later than the current time + seconds.
// ACE: Player.RushNextPlayerSave
pub fn rush_next_player_save(w: &mut World, this: ObjectGuid, seconds: i32) {
    let interval = player_save_interval_secs(w);
    let utc_now = w.now.utc;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let db = &mut o.wo.world_object_database;
    #[allow(clippy::cast_precision_loss)] // `AddSeconds(long)` converts the long to double
    let interval = interval as f64;
    if db.last_requested_database_save.add_seconds(interval)
        <= utc_now.add_seconds(f64::from(seconds))
    {
        return;
    }

    db.last_requested_database_save = utc_now
        .add_seconds(f64::from(seconds))
        .add_seconds(-interval);
}

/// Saves the character to the persistent database. Includes Stats, Position, Skills, etc. Will
/// also save any possessions that are marked with ChangesDetected. The biota snapshots are taken
/// in ACE's order (the player, then each changed possession) and saved as one batch.
// ACE: Player.SavePlayerToDatabase
pub fn save_player_to_database(w: &mut World, this: ObjectGuid) {
    if w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_database.character_changes_detected)
    {
        save_character_to_database(w, this);
    }

    let mut biotas = Vec::new();

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, this, false);
    let Some(o) = w.objects.get(this) else { return };
    biotas.push(o.biota.clone());

    let all_possession = player_get_all_possessions(w, this);

    for possession in all_possession {
        if w.objects
            .get(possession)
            .is_some_and(|p| p.wo.world_object_database.changes_detected)
        {
            crate::dispatch::save_biota_to_database::save_biota_to_database(w, possession, false);
            if let Some(p) = w.objects.get(possession) {
                biotas.push(p.biota.clone());
            }
        }
    }

    let requested_time = w.now.utc;
    let name = w
        .objects
        .get(this)
        .and_then(|o| o.get_property(empyrean_entity::enums::PropertyString::Name))
        .unwrap_or_default();

    w.shard.save_biotas_in_parallel(
        biotas,
        Some(Box::new(move |w: &mut World, result: bool| {
            log::debug!(
                "{} has been saved. It took {} ms to process the request.",
                name,
                empyrean_common::dotnet::format::format(
                    (w.now.utc - requested_time).total_milliseconds(),
                    "N0"
                )
            );

            if !result {
                if let Some(p) = w.objects.get_mut(this).and_then(|o| o.player.as_mut()) {
                    // This will trigger a boot on next player tick
                    p.player_database.biota_save_failed = true;
                }
            }
        })),
        false,
    );
}

/// Saves the player's `Character` row (a snapshot of it).
// ACE: Player.SaveCharacterToDatabase
pub fn save_character_to_database(w: &mut World, this: ObjectGuid) {
    let utc_now = w.now.utc;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let Some(p) = o.player.as_mut() else { return };
    p.player_database.character_last_requested_database_save = utc_now;
    p.player_database.character_changes_detected = false;

    let character = p
        .player
        .character
        .clone()
        .expect("ACE: Player.Character is null (NullReferenceException)");

    // DIVERGE: ACE's session list holds the player's own Character object, so the character the
    // next enter-world uses is this one; here each holds a copy, which the save brings up to date.
    sync_session_character(w, this, &character);

    //DatabaseManager.Shard.SaveCharacter(Character, CharacterDatabaseLock, null);
    w.shard.save_character(
        character,
        Some(Box::new(move |w: &mut World, result: bool| {
            if !result {
                if let Some(p) = w.objects.get_mut(this).and_then(|o| o.player.as_mut()) {
                    // This will trigger a boot on next player tick
                    p.player_database.character_save_failed = true;
                }
            }
        })),
    );
}

/// Not ACE: the session's copy of the character (`session.Characters`) replaced by the player's
/// (see the DIVERGE in [`save_character_to_database`]).
fn sync_session_character(
    w: &mut World,
    this: ObjectGuid,
    character: &empyrean_store::models::shard::Character,
) {
    let Some(session) = crate::managers::player_manager::player_session(w, this) else {
        return;
    };
    if let Some(s) = w.sessions.get_mut(session) {
        if let Some(entry) = s.characters.iter_mut().find(|c| c.id == character.id) {
            entry.clone_from(character);
        }
    }
}

/// `Player.GetAllPossessions()` (`Player_Inventory.cs`, 4.5b's port): the main pack's items, then
/// the items in each side pack, then the equipped items, each in its dictionary's order.
#[must_use]
pub fn player_get_all_possessions(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::player_inventory::get_all_possessions(w, this)
}
