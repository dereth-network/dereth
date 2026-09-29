// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/SquelchManager.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/SquelchManager.cs`.
//!
//! A player's account, character and global squelches. The manager lives on the player
//! (`PlayerFields.squelch_manager`); its `Player` is the `this` every function takes. The rows are
//! the player's `Character` squelch rows (`CharacterDatabaseLock` is not needed: only the world
//! thread touches the Character); the global mask is the `SquelchGlobal` property.

use empyrean_entity::enums::{ChatMessageType, SquelchMask};
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::Character;

use crate::entity::i_player::{self, IPlayer};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_communication_set_squelch::game_event_set_squelch_db;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::squelch_db::{squelch_db_new, SquelchDB};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: SquelchManager
#[derive(Debug, Default)]
pub struct SquelchManager {
    // ACE: SquelchManager.Player
    // The player who owns these squelches: the `this` of every function below.

    // ACE: SquelchManager.Squelches
    /// The SquelchDB contains the account and character squelches, in the network protocol
    /// Dictionary format.
    pub squelches: SquelchDB,
}

// ---- helpers ------------------------------------------------------------------------------------

fn object(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .expect("ACE: SquelchManager.Player is null (NullReferenceException)")
}

fn manager(w: &World, this: ObjectGuid) -> &SquelchManager {
    &object(w, this)
        .player
        .as_ref()
        .expect("a player")
        .player
        .squelch_manager
}

/// `Player.Character` (a missing Character is ACE's `NullReferenceException`).
fn character(w: &World, this: ObjectGuid) -> &Character {
    shims::player_character(object(w, this))
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

fn character_mut(w: &mut World, this: ObjectGuid) -> &mut Character {
    w.objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .and_then(|p| p.player.character.as_mut())
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

/// `Player.CharacterChangesDetected = true;`
fn set_character_changes_detected(w: &mut World, this: ObjectGuid) {
    w.objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("a player")
        .player_database
        .character_changes_detected = true;
}

/// `Player.Account.AccountId`.
fn account_id(w: &World, this: ObjectGuid) -> u32 {
    object(w, this)
        .player
        .as_ref()
        .and_then(|p| p.player.account.as_ref())
        .expect("ACE: Player.Account is null (NullReferenceException)")
        .account_id
}

/// `Player.Session.Network.EnqueueSend(new GameMessageSystemChat(message, ChatMessageType.Broadcast))`.
fn send_broadcast(w: &mut World, this: ObjectGuid, message: &str) {
    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, ChatMessageType::Broadcast),
    );
}

/// `IPlayer.Name`.
fn i_player_name(w: &World, player: IPlayer) -> String {
    i_player::name(w, player).unwrap_or_default()
}

/// `IPlayer.Account.AccountId` (a missing account is ACE's `NullReferenceException`).
fn i_player_account_id(w: &World, player: IPlayer) -> u32 {
    i_player::account(w, player)
        .expect("ACE: player.Account is null (NullReferenceException)")
        .account_id
}

/// `player.SquelchManager.Squelches`: the owner's current SquelchDB.
#[must_use]
pub fn squelches(w: &World, this: ObjectGuid) -> &SquelchDB {
    &manager(w, this).squelches
}

/// `player.SquelchManager.Squelches.Contains(source, messageType)`, where `source` may be null
/// (then only the global squelches apply: a null is not a `Player`).
#[must_use]
pub fn squelches_contains(
    w: &World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    message_type: ChatMessageType,
) -> bool {
    // `ObjectGuid(0)` names no object, which is what `Contains` sees for a null source.
    squelches(w, this).contains(w, source.unwrap_or(ObjectGuid::new(0)), message_type)
}

// ---- members ------------------------------------------------------------------------------------

// ACE: SquelchManager.SquelchManager
/// Constructs a new SquelchManager for a Player. The player is still under construction (not in
/// `w.objects`), so its object is passed in; `UpdateSquelchDB` reads its Character and
/// `SquelchGlobal`. A player built without a Character (the tests' Weenie-built players) gets the
/// SquelchDB of an empty Character.
#[must_use]
pub fn squelch_manager_new(w: &World, player: &WorldObject) -> SquelchManager {
    // DIVERGE: the object is passed in (it is not in `w.objects` yet), and a player with no
    // Character (only the Weenie constructor, whose Character is not ported) reads no rows where
    // ACE's always has one (arch).
    let rows = shims::player_character(player)
        .map(Character::get_squelches)
        .unwrap_or_default();
    SquelchManager {
        squelches: squelch_db_new(w, &rows, player.squelch_global()),
    }
}

// ACE: SquelchManager.HasSquelches
/// Returns TRUE if this player has squelched any accounts / characters / globals.
#[must_use]
pub fn has_squelches(w: &World, this: ObjectGuid) -> bool {
    let squelches = squelches(w, this);
    !squelches.accounts.is_empty()
        || !squelches.characters.is_empty()
        || !squelches.globals.filters.is_empty()
}

// ACE: SquelchManager.IsLegalChannel
/// Returns TRUE if this channel can be squelched.
#[must_use]
pub fn is_legal_channel(channel: ChatMessageType) -> bool {
    matches!(
        channel,
        ChatMessageType::Speech
            | ChatMessageType::Tell
            | ChatMessageType::Combat
            | ChatMessageType::Magic
            | ChatMessageType::Emote
            | ChatMessageType::Appraisal
            | ChatMessageType::Spellcasting
            | ChatMessageType::Allegiance
            | ChatMessageType::Fellowship
            | ChatMessageType::CombatEnemy
            | ChatMessageType::CombatSelf
            | ChatMessageType::Recall
            | ChatMessageType::Craft
            | ChatMessageType::Salvaging
    )
}

// ACE: SquelchManager.HandleActionModifyCharacterSquelch
/// Called when adding or removing a character squelch. `this` is the player that owns the
/// `SquelchManager`.
pub fn handle_action_modify_character_squelch(
    w: &mut World,
    this: ObjectGuid,
    squelch: bool,
    player_guid: u32,
    player_name: &str,
    message_type: ChatMessageType,
) {
    //Console.WriteLine($"{Player.Name}.HandleActionModifyCharacterSquelch({squelch}, {playerGuid:X8}, {playerName}, {messageType})");

    if message_type != ChatMessageType::AllChannels && !is_legal_channel(message_type) {
        send_broadcast(
            w,
            this,
            &format!("{message_type} is not a legal squelch channel"),
        );
        return;
    }

    let player = if player_guid != 0 {
        let (found, _) = player_manager::find_by_guid(w, player_guid);

        let Some(found) = found else {
            send_broadcast(w, this, "Couldn't find player to squelch.");
            return;
        };
        found
    } else {
        if is_null_or_white_space(player_name) {
            return;
        }

        let (found, _) = player_manager::find_by_name(w, player_name);

        let Some(found) = found else {
            send_broadcast(w, this, &format!("{player_name} not found."));
            return;
        };
        found
    };

    if player.guid() == this {
        send_broadcast(w, this, "You can't squelch yourself!");
        return;
    }

    if squelch {
        squelch_character(w, this, player, message_type);
    } else {
        unsquelch_character(w, this, player, message_type);
    }

    update_squelch_db(w, this);

    send_squelch_db(w, this);
}

// ACE: SquelchManager.SquelchCharacter
/// Not ACE's (a fix, V320): a character an account squelch was made through is
/// already squelched on every channel, and squelching it again is answered "already squelched"
/// with the account squelch left as it is. A character has one squelch row, and ACE (which looks
/// only among the character squelches) overwrote the account row with account 0, turning the
/// account squelch into a character squelch on the one channel asked for.
pub fn squelch_character(
    w: &mut World,
    this: ObjectGuid,
    player: IPlayer,
    message_type: ChatMessageType,
) {
    let existing = squelches(w, this)
        .characters
        .get(&player.guid().full())
        .cloned();

    let mut new_mask = message_type.to_mask();

    let channel_msg = if message_type == ChatMessageType::AllChannels {
        String::new()
    } else {
        format!(" on the {message_type} channel")
    };

    let through_account = character(w, this)
        .get_squelches()
        .iter()
        .any(|s| s.squelch_character_id == player.guid().full() && s.squelch_account_id != 0);
    if through_account {
        let name = i_player_name(w, player);
        send_broadcast(
            w,
            this,
            &format!("{name} is already squelched{channel_msg}."),
        );
        return;
    }

    if let Some(existing) = existing {
        let existing_mask = existing.filters[0];

        new_mask = existing_mask.add(new_mask);

        if existing_mask == new_mask {
            let name = i_player_name(w, player);
            send_broadcast(
                w,
                this,
                &format!("{name} is already squelched{channel_msg}."),
            );
            return;
        }
    }

    character_mut(w, this).add_or_update_squelch(player.guid().full(), 0, new_mask.0);
    set_character_changes_detected(w, this);

    let name = i_player_name(w, player);
    send_broadcast(w, this, &format!("{name} has been squelched{channel_msg}."));
}

// ACE: SquelchManager.UnsquelchCharacter
/// Not ACE's (a fix, V320): removing one channel (not `AllChannels`) marks the
/// Character changed, so the narrowed squelch is saved; ACE did not, and it was saved only with the
/// next other change (a log-off without one lost it).
pub fn unsquelch_character(
    w: &mut World,
    this: ObjectGuid,
    player: IPlayer,
    message_type: ChatMessageType,
) {
    let existing = squelches(w, this)
        .characters
        .get(&player.guid().full())
        .cloned();

    let Some(existing) = existing else {
        let name = i_player_name(w, player);
        send_broadcast(w, this, &format!("{name} is not squelched."));
        return;
    };

    if message_type == ChatMessageType::AllChannels {
        character_mut(w, this).try_remove_squelch(player.guid().full(), 0);
        set_character_changes_detected(w, this);

        let name = i_player_name(w, player);
        send_broadcast(w, this, &format!("{name} has been unsquelched."));
    } else {
        let existing_mask = existing.filters[0];
        let remove_mask = message_type.to_mask();

        if !existing_mask.contains(remove_mask) {
            let name = i_player_name(w, player);
            send_broadcast(
                w,
                this,
                &format!("{name} is not squelched on the {message_type} channel."),
            );
            return;
        }

        let new_mask = existing_mask.remove(remove_mask);

        if new_mask != SquelchMask::None {
            character_mut(w, this).add_or_update_squelch(player.guid().full(), 0, new_mask.0);
        } else {
            character_mut(w, this).try_remove_squelch(player.guid().full(), 0);
        }
        set_character_changes_detected(w, this);

        let name = i_player_name(w, player);
        send_broadcast(
            w,
            this,
            &format!("{name} has been unsquelched on the {message_type} channel."),
        );
    }
}

// ACE: SquelchManager.HandleActionModifyAccountSquelch
/// Called when adding or removing an account squelch. `this` is the player that owns the
/// `SquelchManager`.
pub fn handle_action_modify_account_squelch(
    w: &mut World,
    this: ObjectGuid,
    squelch: bool,
    player_name: &str,
) {
    //Console.WriteLine($"{Player.Name}.HandleActionModifyAccountSquelch({squelch}, {playerName})");

    if is_null_or_white_space(player_name) {
        return;
    }

    let (player, _) = player_manager::find_by_name(w, player_name);

    let Some(player) = player else {
        send_broadcast(w, this, &format!("{player_name} not found."));
        return;
    };

    let player_account_id = i_player_account_id(w, player);
    if player_account_id == account_id(w, this) {
        send_broadcast(w, this, "You can't squelch yourself!");
        return;
    }

    let squelches = character(w, this).get_squelches();

    let existing = squelches
        .iter()
        .find(|i| i.squelch_account_id == player_account_id)
        .cloned();

    let name = i_player_name(w, player);
    if squelch {
        if existing.is_some() {
            send_broadcast(w, this, &format!("{name}'s account is already squelched."));
            return;
        }

        // always all channels?
        character_mut(w, this).add_or_update_squelch(
            player.guid().full(),
            player_account_id,
            SquelchMask::AllChannels.0,
        );
        set_character_changes_detected(w, this);

        send_broadcast(w, this, &format!("{name}'s account has been squelched."));
    } else {
        let Some(existing) = existing else {
            send_broadcast(w, this, &format!("{name}'s account is not squelched."));
            return;
        };

        character_mut(w, this).try_remove_squelch(existing.squelch_character_id, player_account_id);
        set_character_changes_detected(w, this);

        send_broadcast(w, this, &format!("{name}'s account has been unsquelched."));
    }

    update_squelch_db(w, this);

    send_squelch_db(w, this);
}

// ACE: SquelchManager.HandleActionModifyGlobalSquelch
/// Called when modifying the global squelches - @filter. `this` is the player that owns the
/// `SquelchManager`.
pub fn handle_action_modify_global_squelch(
    w: &mut World,
    this: ObjectGuid,
    squelch: bool,
    message_type: ChatMessageType,
) {
    //Console.WriteLine($"{Player.Name}.HandleActionModifyGlobalSquelch({squelch}, {messageType})");

    let mask = message_type.to_mask();

    let existing_mask = object(w, this).squelch_global();
    let new_mask = if squelch {
        existing_mask.add(mask)
    } else {
        existing_mask.remove(mask)
    };

    let op = if squelch { "squelch" } else { "unsquelch" };

    if existing_mask == new_mask {
        send_broadcast(
            w,
            this,
            &format!("The {message_type} channel is already {op}ed."),
        );
        return;
    }

    w.objects
        .get_mut(this)
        .expect("ACE: SquelchManager.Player is null (NullReferenceException)")
        .set_squelch_global(new_mask);

    send_broadcast(
        w,
        this,
        &format!("The {message_type} channel has been {op}ed."),
    );

    update_squelch_db(w, this);

    send_squelch_db(w, this);
}

// ACE: SquelchManager.UpdateSquelchDB
/// Builds the SquelchDB for network sending.
pub fn update_squelch_db(w: &mut World, this: ObjectGuid) {
    let rows = character(w, this).get_squelches();
    let globals = object(w, this).squelch_global();
    let db = squelch_db_new(w, &rows, globals);
    w.objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("a player")
        .player
        .squelch_manager
        .squelches = db;
}

// ACE: SquelchManager.SendSquelchDB
/// Sends the SquelchDB to the player.
pub fn send_squelch_db(w: &mut World, this: ObjectGuid) {
    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let db = squelches(w, this).clone();
    let msg = game_event_set_squelch_db(session_data(w, session), &db);
    enqueue_send(w, session, msg);
}

/// `string.IsNullOrWhiteSpace` (.NET's white space is Unicode White_Space, as Rust's).
fn is_null_or_white_space(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}
