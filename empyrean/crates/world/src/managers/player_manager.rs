// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/PlayerManager.cs
//! Port of `Source/ACE.Server/Managers/PlayerManager.cs`.
//!
//! ACE's static `PlayerManager` keeps every player of the shard: the online ones (`Player`
//! objects) and the offline ones (`OfflinePlayer`), with a name index and an account index over
//! both. Here the state is [`PlayerManagerState`] (`w.player_manager`); an online player is a guid
//! into `w.objects`, and index entries are [`IPlayer`] references.
//!
//! `playersLock` is not needed: only the world thread touches this state.
//!
//! `Player` members that are not ported yet (`LogOut_Inner`, `ForceLogoff`, allegiance and friend
//! updates, ...) are called through `not_ported!` pointers named after them, at the end of this
//! file. `Player.Session` and `Player.Account` are not `Player` fields yet either: the session is
//! found by its `Player` ([`player_session`]) and the account is kept on the online entry.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{format as dotnet_format, CsCast, DotNetDateTime, DotNetDict};
use empyrean_entity::enums::{
    AccessLevel, Channel, ChatMessageType, PlayerKillerStatus, PropertyBool, PropertyFloat,
    PropertyInt, PropertyString,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionTerminationReason};
use empyrean_store::models::auth::Account;

use crate::entity::i_player::{self, IPlayer};
use crate::entity::offline_player::OfflinePlayer;
use crate::network::game_event::events::game_event_channel_broadcast::game_event_channel_broadcast;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::World;

/// A `playerNames` key: `StringComparer.OrdinalIgnoreCase`. It keeps the name as given and
/// compares by .NET's ordinal case folding (each character upper-cased on its own, never expanded).
#[derive(Debug, Clone)]
pub struct OrdinalIgnoreCase(pub String);

impl OrdinalIgnoreCase {
    fn folded(&self) -> impl Iterator<Item = char> + '_ {
        self.0.chars().map(fold_char)
    }
}

impl PartialEq for OrdinalIgnoreCase {
    fn eq(&self, other: &Self) -> bool {
        self.folded().eq(other.folded())
    }
}

impl Eq for OrdinalIgnoreCase {}

impl Hash for OrdinalIgnoreCase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for c in self.folded() {
            c.hash(state);
        }
    }
}

/// .NET `OrdinalIgnoreCase`: the simple (single-character) upper-case mapping.
fn fold_char(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
#[must_use]
pub fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    a.chars().map(fold_char).eq(b.chars().map(fold_char))
}

/// One `onlinePlayers` value. ACE stores the `Player`; here its guid, with the account ACE reads
/// from `Player.Account` (not a `Player` field yet), taken from the player's `OfflinePlayer` when
/// it came online.
#[derive(Debug, Clone)]
pub struct OnlinePlayer {
    pub guid: ObjectGuid,
    pub account: Option<Account>,
}

/// One `playersPendingFinalLogoff` entry: the player and its `Player.LogOffFinalizedTime`, which
/// only this queue reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingFinalLogoff {
    pub player: ObjectGuid,
    pub log_off_finalized_time: f64,
}

/// The mutable static state of ACE's `PlayerManager`, held as a field of `World`.
#[derive(Debug)]
pub struct PlayerManagerState {
    // ACE: PlayerManager.onlinePlayers
    pub online_players: DotNetDict<u32, OnlinePlayer>,
    // ACE: PlayerManager.offlinePlayers
    pub offline_players: DotNetDict<u32, OfflinePlayer>,
    // ACE: PlayerManager.playerNames
    /// Indexed by player name.
    pub player_names: DotNetDict<OrdinalIgnoreCase, IPlayer>,
    // ACE: PlayerManager.playerAccounts
    /// Indexed by account id.
    pub player_accounts: DotNetDict<u32, DotNetDict<u32, IPlayer>>,
    // ACE: PlayerManager.lastDatabaseSave
    pub last_database_save: DotNetDateTime,
    // ACE: PlayerManager.playersPendingLogoff
    pub players_pending_logoff: VecDeque<ObjectGuid>,
    // ACE: PlayerManager.playersPendingFinalLogoff
    pub players_pending_final_logoff: VecDeque<PendingFinalLogoff>,
}

impl Default for PlayerManagerState {
    fn default() -> Self {
        Self {
            online_players: DotNetDict::new(),
            offline_players: DotNetDict::new(),
            player_names: DotNetDict::new(),
            player_accounts: DotNetDict::new(),
            last_database_save: DotNetDateTime::MIN_VALUE,
            players_pending_logoff: VecDeque::new(),
            players_pending_final_logoff: VecDeque::new(),
        }
    }
}

// ACE: PlayerManager.databaseSaveInterval
/// OfflinePlayers will be saved to the database every 1 hour.
#[must_use]
pub fn database_save_interval() -> TimeSpan {
    TimeSpan::from_hours(1.0)
}

// ACE: PlayerManager.playerFinalLogoutDuration
#[must_use]
pub fn player_final_logout_duration() -> TimeSpan {
    TimeSpan::from_minutes(15.0)
}

// ACE: PlayerManager.Initialize
/// This will load all the players from the database into the OfflinePlayers dictionary. It
/// should be called before WorldManager is initialized.
///
/// DIVERGE: ACE builds the `OfflinePlayer`s with `Parallel.ForEach`, so their dictionary order
/// is unspecified; here they are added in the shard's order.
pub fn initialize(w: &mut World) {
    let results = w.shard.base_database().get_all_player_biotas_in_parallel();

    for result in results {
        let offline_player = {
            let mut shard = w.shard.base_database();
            let mut auth = w.auth.lock();
            OfflinePlayer::new(result, &mut **shard, &mut **auth)
        };
        let guid = offline_player.guid;
        let name = offline_player.name().unwrap_or_default();
        let account_id = offline_player.account.as_ref().map(|a| a.account_id);

        let pm = &mut w.player_manager;
        pm.offline_players.insert(guid.full(), offline_player);

        pm.player_names
            .insert(OrdinalIgnoreCase(name.clone()), IPlayer::Offline(guid));

        if let Some(account_id) = account_id {
            pm.player_accounts
                .get_or_insert_with(account_id, DotNetDict::new)
                .insert(guid.full(), IPlayer::Offline(guid));
        } else {
            log::error!(
                "PlayerManager.Initialize: couldn't find account for player {name} ({guid:?})"
            );
        }
    }
}

// ACE: PlayerManager.AddPlayerToLogoffQueue
pub fn add_player_to_logoff_queue(w: &mut World, player: ObjectGuid) {
    if !w.player_manager.players_pending_logoff.contains(&player) {
        w.player_manager.players_pending_logoff.push_back(player);
    }
}

// ACE: PlayerManager.AddPlayerToFinalLogoffQueue
pub fn add_player_to_final_logoff_queue(w: &mut World, player: ObjectGuid) {
    if !w
        .player_manager
        .players_pending_final_logoff
        .iter()
        .any(|p| p.player == player)
    {
        // player.LogOffFinalizedTime = Time.GetFutureUnixTime(playerFinalLogoutDuration.TotalSeconds);
        let log_off_finalized_time =
            w.now.unix_time + player_final_logout_duration().total_seconds();
        w.player_manager
            .players_pending_final_logoff
            .push_back(PendingFinalLogoff {
                player,
                log_off_finalized_time,
            });
    }
}

// ACE: PlayerManager.RemovePlayerFromFinalLogoffQueue
pub fn remove_player_from_final_logoff_queue(w: &mut World, player: ObjectGuid) {
    // LinkedList.Remove(T): the first occurrence.
    if let Some(i) = w
        .player_manager
        .players_pending_final_logoff
        .iter()
        .position(|p| p.player == player)
    {
        w.player_manager.players_pending_final_logoff.remove(i);
    }
}

// ACE: PlayerManager.Tick
pub fn tick(w: &mut World) {
    // Database Save
    if w.player_manager.last_database_save + database_save_interval() <= w.now.utc {
        save_offline_players_with_changes(w);
    }

    let current_unix_time = w.now.unix_time;

    while let Some(&first) = w.player_manager.players_pending_logoff.front() {
        let logoff_timestamp = w
            .objects
            .get(first)
            .and_then(|o| o.get_property(PropertyFloat::LogoffTimestamp));

        // `first.LogoffTimestamp <= currentUnixTime`: a null double? compares false.
        if logoff_timestamp.is_some_and(|t| t <= current_unix_time) {
            w.player_manager.players_pending_logoff.pop_front();
            player_log_out_inner(w, first, false);
            if let Some(session) = player_session(w, first) {
                let now = w.now.utc;
                if let Some(s) = w.sessions.get_mut(session) {
                    s.log_off_request_time = now;
                }
            }
        } else {
            break;
        }
    }

    while let Some(&first) = w.player_manager.players_pending_final_logoff.front() {
        if current_unix_time >= first.log_off_finalized_time {
            w.player_manager.players_pending_final_logoff.pop_front();
            player_set_forced_log_off_requested(w, first.player, true);
            if let Some(session) = player_session(w, first.player) {
                let msg = game_message_boot_account(Some(
                    " because the character was forced to log off by system",
                ));
                let now = w.now;
                w.net.terminate(
                    session,
                    SessionTerminationReason::AutoForcedLogOff,
                    Some(msg.into_outbound()),
                    String::new(),
                    now,
                );
            }
            player_force_logoff(w, first.player);
        } else {
            break;
        }
    }
}

// ACE: PlayerManager.SaveOfflinePlayersWithChanges
/// This will save any player in the OfflinePlayers dictionary that has ChangesDetected. The
/// biotas are saved in one batch.
pub fn save_offline_players_with_changes(w: &mut World) {
    w.player_manager.last_database_save = w.now.utc;

    let now = w.now.utc;
    let mut biotas = Vec::new();

    for player in w.player_manager.offline_players.values_mut() {
        if player.changes_detected {
            player.save_biota_to_database(now, false);
            biotas.push(player.biota.clone());
        }
    }

    w.shard.save_biotas_in_parallel(biotas, None, true);
}

// ACE: PlayerManager.AddOfflinePlayer
/// This would be used when a new player is created after the server has started. When a new
/// Player is created, they're created in an offline state, and then set to online shortly after as
/// the login sequence continues.
///
/// # Panics
/// When the player has no account (ACE's `NullReferenceException` on `Account.AccountId`).
pub fn add_offline_player(w: &mut World, player: ObjectGuid) {
    let Some(biota) = w.objects.get(player).map(|o| o.biota.clone()) else {
        return;
    };
    add_offline_player_biota(w, biota);
}

/// The body of [`add_offline_player`] from the player's biota (`new OfflinePlayer(player.Biota)`),
/// for callers that hold the biota rather than a live `Player`.
///
/// # Panics
/// When the player has no account (ACE's `NullReferenceException` on `Account.AccountId`).
pub fn add_offline_player_biota(w: &mut World, biota: empyrean_entity::Biota) {
    let offline_player = {
        let mut shard = w.shard.base_database();
        let mut auth = w.auth.lock();
        OfflinePlayer::new(biota, &mut **shard, &mut **auth)
    };
    let guid = offline_player.guid;
    let name = offline_player.name().unwrap_or_default();
    let account_id = offline_player.account.as_ref().map(|a| a.account_id);

    let pm = &mut w.player_manager;
    pm.offline_players.insert(guid.full(), offline_player);

    pm.player_names
        .insert(OrdinalIgnoreCase(name), IPlayer::Offline(guid));

    let account_id = account_id.expect("Object reference not set to an instance of an object.");
    pm.player_accounts
        .get_or_insert_with(account_id, DotNetDict::new)
        .insert(guid.full(), IPlayer::Offline(guid));
}

// ACE: PlayerManager.GetOfflinePlayer
/// This will return `None` if the player wasn't found.
#[must_use]
pub fn get_offline_player(w: &World, guid: u32) -> Option<&OfflinePlayer> {
    w.player_manager.offline_players.get(&guid)
}

// ACE: PlayerManager.GetOfflinePlayer
/// The `string` overload: the first offline player named `name` or `+name`, ignoring case.
#[must_use]
pub fn get_offline_player_by_name<'a>(w: &'a World, name: &str) -> Option<&'a OfflinePlayer> {
    let admin = format!("+{name}");

    w.player_manager.offline_players.values().find(|p| {
        let n = p.name().unwrap_or_default();
        equals_ordinal_ignore_case(&n, name) || equals_ordinal_ignore_case(&n, &admin)
    })
}

// ACE: PlayerManager.GetAllPlayers
#[must_use]
pub fn get_all_players(w: &World) -> Vec<IPlayer> {
    let offline_players = get_all_offline(w);
    let online_players = get_all_online(w);

    let mut all_players = Vec::new();

    all_players.extend(offline_players.into_iter().map(IPlayer::Offline));
    all_players.extend(online_players.into_iter().map(IPlayer::Online));

    all_players
}

// ACE: PlayerManager.GetAccountPlayers
#[must_use]
pub fn get_account_players(w: &World, account_id: u32) -> Option<&DotNetDict<u32, IPlayer>> {
    w.player_manager.player_accounts.get(&account_id)
}

// ACE: PlayerManager.GetOfflineCount
#[must_use]
pub fn get_offline_count(w: &World) -> usize {
    w.player_manager.offline_players.len()
}

// ACE: PlayerManager.GetAllOffline
#[must_use]
pub fn get_all_offline(w: &World) -> Vec<ObjectGuid> {
    w.player_manager
        .offline_players
        .values()
        .map(|p| p.guid)
        .collect()
}

// ACE: PlayerManager.GetOnlineCount
#[must_use]
pub fn get_online_count(w: &World) -> i32 {
    i32::try_from(w.player_manager.online_players.len()).unwrap_or(i32::MAX)
}

// ACE: PlayerManager.GetOnlinePlayer
/// This will return `None` if the player wasn't found.
#[must_use]
pub fn get_online_player(w: &World, guid: u32) -> Option<ObjectGuid> {
    w.player_manager.online_players.get(&guid).map(|e| e.guid)
}

// ACE: PlayerManager.GetOnlinePlayer
/// The `string` overload: the first online player named `name` or `+name`, ignoring case.
#[must_use]
pub fn get_online_player_by_name(w: &World, name: &str) -> Option<ObjectGuid> {
    let admin = format!("+{name}");

    w.player_manager
        .online_players
        .values()
        .find(|p| {
            let n = online_name(w, p.guid);
            equals_ordinal_ignore_case(&n, name) || equals_ordinal_ignore_case(&n, &admin)
        })
        .map(|p| p.guid)
}

// ACE: PlayerManager.GetAllOnline
#[must_use]
pub fn get_all_online(w: &World) -> Vec<ObjectGuid> {
    w.player_manager
        .online_players
        .values()
        .map(|p| p.guid)
        .collect()
}

// ACE: PlayerManager.SwitchPlayerFromOfflineToOnline
/// This will return true if the player was successfully added. It will return false if the player
/// was not found in the OfflinePlayers dictionary (which should never happen), or player already
/// exists in the OnlinePlayers dictionary (which should never happen). This will always be
/// preceded by a call to GetOfflinePlayer().
pub fn switch_player_from_offline_to_online(w: &mut World, player: ObjectGuid) -> bool {
    let Some(offline_player) = w.player_manager.offline_players.remove(&player.full()) else {
        return false; // This should never happen
    };

    if offline_player.changes_detected {
        if let Some(o) = w.objects.get_mut(player) {
            o.wo.world_object_database.changes_detected = true;
        }
    }

    // player.Allegiance = offlinePlayer.Allegiance; player.AllegianceNode = offlinePlayer.AllegianceNode;
    player_set_allegiance(
        w,
        player,
        offline_player.allegiance,
        offline_player.allegiance_node,
    );

    let account = offline_player.account.clone();
    if !w.player_manager.online_players.try_add(
        player.full(),
        OnlinePlayer {
            guid: player,
            account,
        },
    ) {
        return false;
    }

    let name = offline_player.name().unwrap_or_default();
    w.player_manager
        .player_names
        .insert(OrdinalIgnoreCase(name), IPlayer::Online(player));

    let account_id = offline_player
        .account
        .as_ref()
        .expect("Object reference not set to an instance of an object.")
        .account_id;
    w.player_manager
        .player_accounts
        .get_mut(&account_id)
        .expect("The given key was not present in the dictionary.")
        .insert(offline_player.guid.full(), IPlayer::Online(player));

    allegiance_manager_load_player(w, player);

    let appear_offline = player_get_appear_offline(w, player);
    player_send_friend_status_updates(w, player, false, !appear_offline);

    true
}

// ACE: PlayerManager.SwitchPlayerFromOnlineToOffline
/// This will return true if the player was successfully added. It will return false if the player
/// was not found in the OnlinePlayers dictionary (which should never happen), or player already
/// exists in the OfflinePlayers dictionary (which should never happen).
pub fn switch_player_from_online_to_offline(w: &mut World, player: ObjectGuid) -> bool {
    if w.player_manager
        .online_players
        .remove(&player.full())
        .is_none()
    {
        return false; // This should never happen
    }

    // DIVERGE: `new OfflinePlayer(player.Biota)` shares the biota; this copies it, and
    // `player::release_logged_off_player` hands over the player's final biota (V204).
    let Some(biota) = w.objects.get(player).map(|o| o.biota.clone()) else {
        return false;
    };
    let mut offline_player = {
        let mut shard = w.shard.base_database();
        let mut auth = w.auth.lock();
        OfflinePlayer::new(biota, &mut **shard, &mut **auth)
    };

    let (allegiance, allegiance_node) = player_get_allegiance(w, player);
    offline_player.allegiance = allegiance;
    offline_player.allegiance_node = allegiance_node;

    let guid = offline_player.guid;
    let name = offline_player.name().unwrap_or_default();
    let account_id = offline_player.account.as_ref().map(|a| a.account_id);

    if !w
        .player_manager
        .offline_players
        .try_add(guid.full(), offline_player)
    {
        return false;
    }

    w.player_manager
        .player_names
        .insert(OrdinalIgnoreCase(name), IPlayer::Offline(guid));

    let account_id = account_id.expect("Object reference not set to an instance of an object.");
    w.player_manager
        .player_accounts
        .get_mut(&account_id)
        .expect("The given key was not present in the dictionary.")
        .insert(guid.full(), IPlayer::Offline(guid));

    let appear_offline = player_get_appear_offline(w, player);
    player_send_friend_status_updates(w, player, !appear_offline, false);
    player_handle_allegiance_on_logout(w, player);

    true
}

// ACE: PlayerManager.HandlePlayerDelete
/// Called when a character is initially deleted on the character select screen.
pub fn handle_player_delete(w: &mut World, character_guid: u32) {
    crate::managers::allegiance_manager::handle_player_delete(w, character_guid);

    crate::managers::house_manager::handle_player_delete(w, character_guid);
}

// ACE: PlayerManager.ProcessDeletedPlayer
/// This will return true if the player was successfully found and removed from the
/// OfflinePlayers dictionary. It will return false if the player was not found in the
/// OfflinePlayers dictionary (which should never happen).
///
/// # Panics
/// When the player has no account, or its account has no entry (ACE throws too).
pub fn process_deleted_player(w: &mut World, guid: u32) -> bool {
    let pm = &mut w.player_manager;
    let Some(offline_player) = pm.offline_players.remove(&guid) else {
        return false; // This should never happen
    };

    pm.player_names.remove(&OrdinalIgnoreCase(
        offline_player.name().unwrap_or_default(),
    ));

    let account_id = offline_player
        .account
        .as_ref()
        .expect("Object reference not set to an instance of an object.")
        .account_id;
    pm.player_accounts
        .get_mut(&account_id)
        .expect("The given key was not present in the dictionary.")
        .remove(&offline_player.guid.full());

    true
}

// ACE: PlayerManager.FindByName
/// This will return `None` if the name was not found; the flag is `isOnline`.
#[must_use]
pub fn find_by_name(w: &World, name: &str) -> (Option<IPlayer>, bool) {
    let player = w
        .player_manager
        .player_names
        .get(&OrdinalIgnoreCase(name.trim_start_matches('+').to_owned()))
        .copied();

    let is_online = player.is_some_and(IPlayer::is_online);

    (player, is_online)
}

// ACE: PlayerManager.FindByGuid
/// This will return `None` if the guid was not found; the flag is `isOnline`.
#[must_use]
pub fn find_by_guid(w: &World, guid: u32) -> (Option<IPlayer>, bool) {
    if let Some(online_player) = w.player_manager.online_players.get(&guid) {
        return (Some(IPlayer::Online(online_player.guid)), true);
    }

    if let Some(offline_player) = w.player_manager.offline_players.get(&guid) {
        return (Some(IPlayer::Offline(offline_player.guid)), false);
    }

    (None, false)
}

// ACE: PlayerManager.FindAllByMonarch
/// Returns a list of all players who are under a monarch.
#[must_use]
pub fn find_all_by_monarch(w: &World, monarch: ObjectGuid) -> Vec<IPlayer> {
    let mut results = Vec::new();

    // this kind of sucks, possibly investigate?
    let online_players_result = w
        .player_manager
        .online_players
        .values()
        .map(|p| IPlayer::Online(p.guid))
        .filter(|&p| i_player::monarch_id(w, p) == Some(monarch.full()));
    let offline_players_result = w
        .player_manager
        .offline_players
        .values()
        .filter(|p| p.monarch_id() == Some(monarch.full()))
        .map(|p| IPlayer::Offline(p.guid));

    results.extend(online_players_result);
    results.extend(offline_players_result);

    results
}

// ACE: PlayerManager.GetOnlineInverseFriends
/// This will return a list of Players that have this guid as a friend.
#[must_use]
pub fn get_online_inverse_friends(w: &World, guid: ObjectGuid) -> Vec<ObjectGuid> {
    let mut results = Vec::new();

    for player in w.player_manager.online_players.values() {
        if character_has_as_friend(w, player.guid, guid.full()) {
            results.push(player.guid);
        }
    }

    results
}

// ACE: PlayerManager.BroadcastToAll
/// Broadcasts GameMessage to all online sessions.
pub fn broadcast_to_all(w: &mut World, msg: &GameMessage) {
    for player in get_all_online(w) {
        if let Some(session) = player_session(w, player) {
            enqueue_send(w, session, msg.clone());
        }
    }
}

// ACE: PlayerManager.BroadcastToAuditChannel
pub fn broadcast_to_audit_channel(w: &mut World, issuer: Option<ObjectGuid>, message: &str) {
    if let Some(issuer) = issuer {
        broadcast_to_channel(w, Channel::Audit, issuer, message, true, true);
    } else {
        broadcast_to_channel_from_console(w, Channel::Audit, message);
    }

    //if (PropertyManager.GetBool("log_audit", true).Item)
    //log.Info($"[AUDIT] {(issuer != null ? $"{issuer.Name} says on the Audit channel: " : "")}{message}");

    //LogBroadcastChat(Channel.Audit, issuer, message);
}

// ACE: PlayerManager.BroadcastToChannel
pub fn broadcast_to_channel(
    w: &mut World,
    channel: Channel,
    sender: ObjectGuid,
    message: &str,
    ignore_squelch: bool,
    ignore_active: bool,
) {
    let sender_channels_active = channels_active(w, sender);
    if sender_channels_active.is_some_and(|c| has_flag(c, channel)) || ignore_active {
        let sender_name = online_name(w, sender);
        let players: Vec<ObjectGuid> = get_all_online(w)
            .into_iter()
            .filter(|&p| has_flag(channels_active(w, p).unwrap_or(Channel(0)), channel))
            .collect();
        for player in players {
            if !squelch_manager_squelches_contains(w, player, sender) || ignore_squelch {
                let Some(session) = player_session(w, player) else {
                    continue;
                };
                let name = if sender == player {
                    String::new()
                } else {
                    sender_name.clone()
                };
                let Some(s) = w.sessions.get_mut(session) else {
                    continue;
                };
                let msg = game_event_channel_broadcast(s, channel, &name, message);
                enqueue_send(w, session, msg);
            }
        }

        log_broadcast_chat(w, channel, Some(sender), message);
    }
}

// ACE: PlayerManager.LogBroadcastChat
pub fn log_broadcast_chat(w: &World, channel: Channel, sender: Option<ObjectGuid>, message: &str) {
    let key = match channel {
        Channel::Abuse => "chat_log_abuse",
        Channel::Admin => "chat_log_admin",
        Channel::AllBroadcast => "chat_log_global", // using this to sub in for a WorldBroadcast channel which isn't technically a channel
        Channel::Audit => "chat_log_audit",
        Channel::Advocate1 | Channel::Advocate2 | Channel::Advocate3 => "chat_log_advocate",
        Channel::Debug => "chat_log_debug",
        Channel::Fellow | Channel::FellowBroadcast => "chat_log_fellow",
        Channel::Help => "chat_log_help",
        Channel::Olthoi => "chat_log_olthoi",
        Channel::QA1 | Channel::QA2 => "chat_log_qa",
        Channel::Sentinel => "chat_log_sentinel",
        Channel::SocietyCelHanBroadcast
        | Channel::SocietyEldWebBroadcast
        | Channel::SocietyRadBloBroadcast => "chat_log_society",
        Channel::AllegianceBroadcast
        | Channel::CoVassals
        | Channel::Monarch
        | Channel::Patron
        | Channel::Vassals => "chat_log_allegiance",
        Channel::AlArqas
        | Channel::Holtburg
        | Channel::Lytelthorpe
        | Channel::Nanto
        | Channel::Rithwic
        | Channel::Samsur
        | Channel::Shoushi
        | Channel::Yanshi
        | Channel::Yaraq => "chat_log_townchans",
        _ => return,
    };
    if !property_manager_get_bool(w, key) {
        return;
    }

    let sender_name = sender.map_or_else(
        || "[SYSTEM]".to_owned(),
        |s| {
            w.objects
                .get(s)
                .map(|_| online_name(w, s))
                .unwrap_or_default()
        },
    );
    if channel == Channel::AllBroadcast {
        log::info!("[CHAT][GLOBAL] {sender_name} issued a world broadcast, \"{message}\"");
    } else {
        log::info!(
            "[CHAT][{}] {sender_name} says on the {channel} channel, \"{message}\"",
            channel.to_string().to_uppercase()
        );
    }
}

// ACE: PlayerManager.BroadcastToChannelFromConsole
pub fn broadcast_to_channel_from_console(w: &mut World, channel: Channel, message: &str) {
    broadcast_to_channel_as(w, channel, "CONSOLE", message);

    log_broadcast_chat(w, channel, None, message);
}

// ACE: PlayerManager.BroadcastToChannelFromEmote
pub fn broadcast_to_channel_from_emote(w: &mut World, channel: Channel, message: &str) {
    broadcast_to_channel_as(w, channel, "EMOTE", message);
}

/// The loop `BroadcastToChannelFromConsole` and `BroadcastToChannelFromEmote` share.
fn broadcast_to_channel_as(w: &mut World, channel: Channel, sender_name: &str, message: &str) {
    let players: Vec<ObjectGuid> = get_all_online(w)
        .into_iter()
        .filter(|&p| has_flag(channels_active(w, p).unwrap_or(Channel(0)), channel))
        .collect();
    for player in players {
        let Some(session) = player_session(w, player) else {
            continue;
        };
        let Some(s) = w.sessions.get_mut(session) else {
            continue;
        };
        let msg = game_event_channel_broadcast(s, channel, sender_name, message);
        enqueue_send(w, session, msg);
    }
}

// ACE: PlayerManager.GagPlayer
pub fn gag_player(w: &mut World, issuer: ObjectGuid, player_name: &str) -> bool {
    let (Some(player), _) = find_by_name(w, player_name) else {
        return false;
    };

    i_player::set_property(w, player, PropertyBool::IsGagged, true);
    let now = w.now.unix_time;
    i_player::set_property(w, player, PropertyFloat::GagTimestamp, now);
    i_player::set_property(w, player, PropertyFloat::GagDuration, 300.0);

    i_player::save_biota_to_database(w, player, true);

    let message = format!(
        "{} has gagged {} for five minutes.",
        online_name(w, issuer),
        i_player::name(w, player).unwrap_or_default()
    );
    broadcast_to_audit_channel(w, Some(issuer), &message);

    true
}

// ACE: PlayerManager.UnGagPlayer
pub fn un_gag_player(w: &mut World, issuer: ObjectGuid, player_name: &str) -> bool {
    let (Some(player), _) = find_by_name(w, player_name) else {
        return false;
    };

    i_player::remove_property(w, player, PropertyBool::IsGagged);
    i_player::remove_property(w, player, PropertyFloat::GagTimestamp);
    i_player::remove_property(w, player, PropertyFloat::GagDuration);

    i_player::save_biota_to_database(w, player, true);

    let message = format!(
        "{} has ungagged {}.",
        online_name(w, issuer),
        i_player::name(w, player).unwrap_or_default()
    );
    broadcast_to_audit_channel(w, Some(issuer), &message);

    true
}

// ACE: PlayerManager.BootAllPlayers
pub fn boot_all_players(w: &mut World) {
    for player in get_all_online(w) {
        let Some(session) = player_session(w, player) else {
            continue;
        };
        if w.sessions
            .get(session)
            .is_some_and(|s| s.access_level < AccessLevel::Advocate)
        {
            let msg = game_message_boot_account(Some(" because the world is now closed"));
            let now = w.now;
            w.net.terminate(
                session,
                SessionTerminationReason::WorldClosed,
                Some(msg.into_outbound()),
                "The world is now closed".to_owned(),
                now,
            );
        }
    }
}

// ACE: PlayerManager.UpdatePKStatusForAllPlayers
pub fn update_pk_status_for_all_players(w: &mut World, world_type: &str, enabled: bool) {
    let (online_status, msg) = match world_type {
        "pk_server" => {
            if enabled {
                (
                    PlayerKillerStatus::PK,
                    format!(
                        "This world has been changed to a Player Killer world. All players will become Player Killers in {} seconds.",
                        dotnet_format::to_string(property_manager_get_double(w, "pk_respite_timer"))
                    ),
                )
            } else {
                (
                    PlayerKillerStatus::NPK,
                    "This world has been changed to a Non Player Killer world. All players are now Non-Player Killers.".to_owned(),
                )
            }
        }
        "pkl_server" => {
            if property_manager_get_bool(w, "pk_server") {
                return;
            }
            if enabled {
                (
                    PlayerKillerStatus::PKLite,
                    format!(
                        "This world has been changed to a Player Killer Lite world. All players will become Player Killer Lites in {} seconds.",
                        dotnet_format::to_string(property_manager_get_double(w, "pk_respite_timer"))
                    ),
                )
            } else {
                (
                    PlayerKillerStatus::NPK,
                    "This world has been changed to a Non Player Killer world. All players are now Non-Player Killers.".to_owned(),
                )
            }
        }
        _ => return,
    };

    for player in get_all_online(w) {
        player_set_player_killer_status(w, player, online_status, true);
    }

    for player in get_all_offline(w) {
        let p = IPlayer::Offline(player);
        i_player::set_property(
            w,
            p,
            PropertyInt::PlayerKillerStatus,
            PlayerKillerStatus::NPK.0.cs_cast(),
        );
        i_player::set_property(w, p, PropertyFloat::MinimumTimeSincePk, 0.0);
    }

    broadcast_to_all(
        w,
        &game_message_system_chat(&msg, ChatMessageType::WorldBroadcast),
    );
    log_broadcast_chat(w, Channel::AllBroadcast, None, &msg);
}

// ACE: PlayerManager.IsAccountAtMaxCharacterSlots
/// # Panics
/// When a player has no account (ACE's `NullReferenceException`).
#[must_use]
pub fn is_account_at_max_character_slots(w: &World, account_name: &str) -> bool {
    let slots_available: i32 = property_manager_get_long(w, "max_chars_per_account").cs_cast();

    let matches = |account: Option<&Account>| {
        let account = account.expect("Object reference not set to an instance of an object.");
        equals_ordinal_ignore_case(&account.account_name, account_name)
    };
    let online_players_total = w
        .player_manager
        .online_players
        .values()
        .filter(|p| matches(p.account.as_ref()))
        .count();
    let offline_players_total = w
        .player_manager
        .offline_players
        .values()
        .filter(|p| matches(p.account.as_ref()))
        .count();

    let total = i32::try_from(online_players_total + offline_players_total).unwrap_or(i32::MAX);
    total >= slots_available
}

// ---------------------------------------------------------------------------------------------
// Not ACE: helpers for members that are not `Player` fields yet, and pointers to `Player`,
// `AllegianceManager` and `PropertyManager` members owned by other units.
// ---------------------------------------------------------------------------------------------

/// `player.Session`: the session whose `Player` is `player`. `Player.Session` is not a field yet;
/// the session's `Player` is the link (ACE sets both in `DoPlayerEnterWorld`).
#[must_use]
pub fn player_session(w: &World, player: ObjectGuid) -> Option<SessionId> {
    w.sessions
        .iter()
        .find(|(_, s)| s.player == Some(player))
        .map(|(id, _)| id)
}

/// `player.Name` for a live object.
fn online_name(w: &World, player: ObjectGuid) -> String {
    w.objects
        .get(player)
        .and_then(|o| o.get_property(PropertyString::Name))
        .unwrap_or_default()
}

/// `Player.ChannelsActive`: `(Channel?)GetProperty(PropertyInt.ChannelsActive)`.
fn channels_active(w: &World, player: ObjectGuid) -> Option<Channel> {
    w.objects
        .get(player)
        .and_then(|o| o.get_property(PropertyInt::ChannelsActive))
        .map(Channel)
}

/// `Enum.HasFlag`: every bit of `flag` is set.
const fn has_flag(value: Channel, flag: Channel) -> bool {
    value.0 & flag.0 == flag.0
}

/// `Player.LogOut_Inner(clientSessionTerminatedAbruptly)` (Player.cs).
fn player_log_out_inner(
    w: &mut World,
    player: ObjectGuid,
    client_session_terminated_abruptly: bool,
) {
    crate::world_objects::player::log_out_inner(w, player, client_session_terminated_abruptly);
}

/// `player.ForcedLogOffRequested = value` (a `Player.cs` field).
fn player_set_forced_log_off_requested(w: &mut World, player: ObjectGuid, value: bool) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player.forced_log_off_requested = value;
    }
}

/// `Player.ForceLogoff()` (Player.cs).
fn player_force_logoff(w: &mut World, player: ObjectGuid) {
    crate::world_objects::player::force_logoff(w, player);
}

/// `player.Allegiance = ..; player.AllegianceNode = ..` (Player_Allegiance.cs; each the guid of an
/// allegiance object, as `OfflinePlayer` keeps them).
fn player_set_allegiance(
    w: &mut World,
    player: ObjectGuid,
    allegiance: Option<ObjectGuid>,
    node: Option<ObjectGuid>,
) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player_allegiance.allegiance = allegiance;
        p.player_allegiance.allegiance_node = node;
    }
}

/// `(player.Allegiance, player.AllegianceNode)` (Player_Allegiance.cs).
fn player_get_allegiance(
    w: &World,
    player: ObjectGuid,
) -> (Option<ObjectGuid>, Option<ObjectGuid>) {
    w.objects
        .get(player)
        .and_then(|o| o.player.as_ref())
        .map_or((None, None), |p| {
            (
                p.player_allegiance.allegiance,
                p.player_allegiance.allegiance_node,
            )
        })
}

/// `AllegianceManager.LoadPlayer(player)`.
fn allegiance_manager_load_player(w: &mut World, player: ObjectGuid) {
    crate::managers::allegiance_manager::load_player(w, Some(IPlayer::Online(player)));
}

/// `player.GetAppearOffline()` (`Player_Character.cs`).
fn player_get_appear_offline(w: &World, player: ObjectGuid) -> bool {
    crate::world_objects::player_character::get_appear_offline(w, player)
}

/// `player.SendFriendStatusUpdates(previouslyOnline, isOnline)` (`Player_Networking.cs`).
fn player_send_friend_status_updates(
    w: &mut World,
    player: ObjectGuid,
    previously_online: bool,
    is_online: bool,
) {
    crate::world_objects::player_networking::send_friend_status_updates(
        w,
        player,
        previously_online,
        is_online,
    );
}

/// `player.HandleAllegianceOnLogout()`.
fn player_handle_allegiance_on_logout(w: &mut World, player: ObjectGuid) {
    crate::world_objects::player_allegiance::handle_allegiance_on_logout(w, player);
}

/// `player.SetPlayerKillerStatus(status, broadcast)` (`Player_Networking.cs`).
fn player_set_player_killer_status(
    w: &mut World,
    player: ObjectGuid,
    status: PlayerKillerStatus,
    broadcast: bool,
) {
    crate::world_objects::player_networking::set_player_killer_status(w, player, status, broadcast);
}

/// `player.SquelchManager.Squelches.Contains(sender)` (`messageType` defaults to `AllChannels`).
fn squelch_manager_squelches_contains(w: &World, player: ObjectGuid, sender: ObjectGuid) -> bool {
    crate::world_objects::managers::squelch_manager::squelches_contains(
        w,
        player,
        Some(sender),
        empyrean_entity::enums::ChatMessageType::AllChannels,
    )
}

/// `player.Character.HasAsFriend(guid, player.CharacterDatabaseLock)` (a missing Character is ACE's
/// `NullReferenceException`).
fn character_has_as_friend(w: &World, player: ObjectGuid, guid: u32) -> bool {
    w.objects
        .get(player)
        .and_then(crate::world_objects::world_object_networking::shims::player_character)
        .expect("ACE: Player.Character is null (NullReferenceException)")
        .has_as_friend(guid)
}

/// `PropertyManager.GetBool(key).Item` (the default fallback, `false`).
#[must_use]
pub fn property_manager_get_bool(w: &World, key: &str) -> bool {
    crate::managers::property_manager::get_bool(w, key, false, true).item
}

/// `PropertyManager.GetLong(key).Item` (the default fallback, `0`).
#[must_use]
pub fn property_manager_get_long(w: &World, key: &str) -> i64 {
    crate::managers::property_manager::get_long(w, key, 0, true).item
}

/// `PropertyManager.GetDouble(key).Item` (the default fallback, `0.0f`).
#[must_use]
pub fn property_manager_get_double(w: &World, key: &str) -> f64 {
    crate::managers::property_manager::get_double(w, key, 0.0, true).item
}
