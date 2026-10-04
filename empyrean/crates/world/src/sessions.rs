// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Session.cs
//! The **game half** of ACE's `Session`.
//!
//! ACE's `Session` mixes the transport (endpoints, `NetworkSession`, termination) with game state
//! (the account, the character list, the `Player`, the log-off timer, DDD). `empyrean-net` owns the
//! transport half and names each session by a [`SessionId`]; this module keeps the game members of
//! each session in [`Sessions`], a map from that handle to [`SessionData`], stored in
//! `World.sessions`.
//!
//! `State`, `AccountId`, `Account` and `AccessLevel` exist on both halves: the transport keeps its
//! copy for its own checks (`CheckState`, the login path), and the world keeps this one for
//! dispatch. Whoever changes one (login, enter world, log off) changes both, through
//! `ServerNet::set_state` / `set_access_level`.
//!
//! Members that need the world or the database are free functions here. ACE's
//! `TickOutbound` is split the same way: empyrean-net's `Session::tick_outbound` sends and times out,
//! and [`tick_outbound`] here runs the game half (the log-off timer, the character-select ping
//! reply and the DDD queue) for every session, after `DoSessionWork`.

use std::collections::{BTreeMap, VecDeque};

use empyrean_common::clock::SnapshotClock;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{CsCast, DotNetDateTime};
use empyrean_common::performance::rate_limiter::RateLimiter;
use empyrean_dat::DatDatabaseType;
use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_entity::ObjectGuid;
use empyrean_net::enums::CharacterError;
use empyrean_net::{SessionId, SessionState};

use crate::managers::player_manager;
use crate::network::game_event::events::game_event_ping_response::game_event_ping_response;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_character_error::game_message_character_error;
use crate::network::game_messages::messages::game_message_character_list::game_message_character_list;
use crate::network::game_messages::messages::game_message_character_log_off::game_message_character_log_off;
use crate::network::game_messages::messages::game_message_ddd_data_message::game_message_ddd_data_message;
use crate::network::game_messages::messages::game_message_server_name::game_message_server_name;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::InboundMessageManagerState;
use crate::World;

/// ACE's `ACE.Database.Models.Shard.Character` row as the session holds it (`Session.Characters`):
/// the store's own row, so that a delete or a restore saves the whole character back.
pub type CharacterSummary = empyrean_store::models::shard::Character;

// ACE: Session
/// The game members of one ACE `Session`.
#[derive(Debug, Clone)]
pub struct SessionData {
    // ACE: Session.GameEventSequence
    pub game_event_sequence: u32,
    // ACE: Session.State
    pub state: SessionState,
    // ACE: Session.AccountId
    pub account_id: u32,
    // ACE: Session.Account
    pub account: Option<String>,
    // ACE: Session.AccessLevel
    pub access_level: AccessLevel,
    // ACE: Session.Characters
    pub characters: Vec<CharacterSummary>,
    // ACE: Session.Player
    pub player: Option<ObjectGuid>,
    // ACE: Session.logOffRequestTime
    pub log_off_request_time: DotNetDateTime,
    // ACE: Session.lastCharacterSelectPingReply
    pub last_character_select_ping_reply: DotNetDateTime,
    // ACE: Session.BootSessionReason
    pub boot_session_reason: Option<String>,
    // ACE: Session.DatWarnCell
    pub dat_warn_cell: bool,
    // ACE: Session.DatWarnPortal
    pub dat_warn_portal: bool,
    // ACE: Session.DatWarnLanguage
    pub dat_warn_language: bool,
    // ACE: Session.DatWarnHighRes
    pub dat_warn_high_res: bool,
    // ACE: Session.BeginDDDSent
    pub begin_ddd_sent: bool,
    // ACE: Session.BeginDDDSentTime
    pub begin_ddd_sent_time: DotNetDateTime,
    // ACE: Session.dddDataQueue
    pub ddd_data_queue: Option<VecDeque<(u32, DatDatabaseType)>>,
    // ACE: Session.LastPassTime
    pub last_pass_time: DotNetDateTime,
    /// Not ACE (V437): the patch records a minute this session is sent, for a client that keeps
    /// overlays; 0 is ACE's shared limit.
    pub overlay_records_per_minute: u32,
    /// Not ACE (V437): that session's own limiter.
    pub overlay_rate_limiter: Option<RateLimiter>,
    /// Not ACE (V437): the patch bytes a second that session is sent, and the tenth of a second
    /// being counted: when it began and the bytes sent in it.
    pub overlay_bytes_per_second: u32,
    pub overlay_window: (DotNetDateTime, u64),
}

impl Default for SessionData {
    /// A new session: C#'s field defaults (`State` is `AuthLoginRequest`, the enum's zero).
    fn default() -> Self {
        Self {
            game_event_sequence: 0,
            state: SessionState::AuthLoginRequest,
            account_id: 0,
            account: None,
            access_level: AccessLevel::Player,
            characters: Vec::new(),
            player: None,
            log_off_request_time: DotNetDateTime::MIN_VALUE,
            last_character_select_ping_reply: DotNetDateTime::MIN_VALUE,
            boot_session_reason: None,
            dat_warn_cell: false,
            dat_warn_portal: false,
            dat_warn_language: false,
            dat_warn_high_res: false,
            begin_ddd_sent: false,
            begin_ddd_sent_time: DotNetDateTime::MIN_VALUE,
            ddd_data_queue: None,
            last_pass_time: DotNetDateTime::MIN_VALUE,
            overlay_records_per_minute: 0,
            overlay_rate_limiter: None,
            overlay_bytes_per_second: 0,
            overlay_window: (DotNetDateTime::MIN_VALUE, 0),
        }
    }
}

impl SessionData {
    // ACE: Session.SetAccount
    pub fn set_account(
        &mut self,
        account_id: u32,
        account: String,
        account_access_level: AccessLevel,
    ) {
        self.account_id = account_id;
        self.account = Some(account);
        self.access_level = account_access_level;
    }

    // ACE: Session.InitSessionForWorldLogin
    pub fn init_session_for_world_login(&mut self) {
        self.game_event_sequence = 1;
    }

    // ACE: Session.SetAccessLevel
    pub fn set_access_level(&mut self, account_access_level: AccessLevel) {
        self.access_level = account_access_level;
    }

    // ACE: Session.SetPlayer
    pub fn set_player(&mut self, player: Option<ObjectGuid>) {
        self.player = player;
    }

    // ACE: Session.AddToDDDQueue
    /// This will enqueue a file to be sent by `ProcessDDDQueue`.
    pub fn add_to_ddd_queue(
        &mut self,
        dat_file_id: u32,
        dat_database_type: DatDatabaseType,
    ) -> bool {
        self.ddd_data_queue
            .get_or_insert_with(VecDeque::new)
            .push_back((dat_file_id, dat_database_type));
        true
    }
}

/// Every session's game half, by transport handle, plus the inbound message queue.
#[derive(Debug, Default)]
pub struct Sessions {
    map: BTreeMap<SessionId, SessionData>,
    /// ACE `InboundMessageManager` state and `NetworkManager.InboundMessageQueue`.
    pub inbound: InboundMessageManagerState,
    // ACE: Session.dddDataQueueRateLimiter
    /// The rate at which ProcessDDDQueue executes (and sends DDD patch data out to client): one
    /// limiter for every session (`static`), `new RateLimiter(1000, TimeSpan.FromMinutes(1))`, built
    /// at first use because its stopwatch reads the world's clock.
    pub ddd_data_queue_rate_limiter: Option<RateLimiter>,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the game half for a new transport session (the world does this when it first hears of
    /// the session). Replaces any stale entry for the same handle.
    pub fn insert(&mut self, id: SessionId, data: SessionData) {
        self.map.insert(id, data);
    }

    /// Drops the game half of a session that is gone.
    pub fn remove(&mut self, id: SessionId) -> Option<SessionData> {
        self.map.remove(&id)
    }

    pub fn get(&self, id: SessionId) -> Option<&SessionData> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut SessionData> {
        self.map.get_mut(&id)
    }

    /// `session.Player`: `None` for an unknown session.
    pub fn player(&self, id: SessionId) -> Option<ObjectGuid> {
        self.map.get(&id).and_then(|s| s.player)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The sessions in handle order.
    pub fn iter(&self) -> impl Iterator<Item = (SessionId, &SessionData)> {
        self.map.iter().map(|(k, v)| (*k, v))
    }
}

// ACE: Session.UpdateCharacters
pub fn update_characters(w: &mut World, session: SessionId, characters: Vec<CharacterSummary>) {
    let Some(s) = w.sessions.get_mut(session) else {
        return;
    };
    s.characters.clear();
    s.characters.extend(characters);

    check_characters_for_deletion(w, session);
}

// ACE: Session.CheckCharactersForDeletion
/// A character whose deletion time has passed is deleted for good: marked, saved, dropped from
/// `PlayerManager` and from the list.
pub fn check_characters_for_deletion(w: &mut World, session: SessionId) {
    let Some(count) = w.sessions.get(session).map(|s| s.characters.len()) else {
        return;
    };
    for i in (0..count).rev() {
        let unix_time = w.now.unix_time;
        let Some(s) = w.sessions.get_mut(session) else {
            return;
        };
        let character = &mut s.characters[i];
        // `Time.GetUnixTime() > Characters[i].DeleteTime`: the ulong converts to double.
        let delete_time: f64 = character.delete_time.cs_cast();
        if character.delete_time > 0 && unix_time > delete_time {
            character.is_deleted = true;

            let snapshot = character.clone();
            let id = character.id;
            w.shard.save_character(snapshot, None);

            player_manager::process_deleted_player(w, id);

            if let Some(s) = w.sessions.get_mut(session) {
                s.characters.remove(i);
            }
        }
    }
}

// ACE: Session.LogOffPlayer
/// Log off the player normally.
pub fn log_off_player(w: &mut World, session: SessionId, force_immediate: bool) {
    let Some(player) = w.sessions.player(session) else {
        return;
    };

    // Character database objects are not cached. Each session gets a new character entity and dbContext from ShardDatabase.
    // To ensure the latest version of the character is saved before any new logins pull these records again, we queue a save here if necessary, at the instant logoff is requested.
    if player_character_changes_detected(w, player) {
        player_save_character_to_database(w, player);
    }

    if w.sessions
        .get(session)
        .is_some_and(|s| s.log_off_request_time == DotNetDateTime::MIN_VALUE)
    {
        let result = player_log_out(w, player, false, force_immediate);

        if result {
            let now = w.now.utc;
            if let Some(s) = w.sessions.get_mut(session) {
                s.log_off_request_time = now;
            }
            // Not ACE: the transport's copy of `Player != null && logOffRequestTime == MinValue`.
            w.net.set_player_active(session, false);
        }
    }
}

// ACE: Session.SendFinalLogOffMessages
/// The end of a log-off: back to character select with a fresh character list.
pub fn send_final_log_off_messages(w: &mut World, session: SessionId) {
    let player = w.sessions.player(session);

    // If we still exist on a landblock, we can't exit yet.
    if player.is_some_and(|p| {
        w.objects
            .get(p)
            .is_some_and(|o| o.current_landblock.is_some())
    }) {
        return;
    }

    let Some(s) = w.sessions.get_mut(session) else {
        return;
    };
    s.log_off_request_time = DotNetDateTime::MIN_VALUE;

    // It's possible for a character change to happen from a GameActionSetCharacterOptions message.
    // This message can be received/processed by the server AFTER LogOfPlayer has been called.
    // What that means is, we could end up with Character changes after the Character has been saved from the initial LogOff request.
    // To make sure we commit these additional changes (if any), we check again here
    if let Some(player) = player {
        if player_character_changes_detected(w, player) {
            player_save_character_to_database(w, player);
        }
    }

    if let Some(s) = w.sessions.get_mut(session) {
        s.set_player(None);
    }
    // Not ACE: nothing holds the logged-off Player now (ACE's garbage collector takes it).
    if let Some(player) = player {
        crate::world_objects::player::release_logged_off_player(w, player);
    }

    if !w.server_manager.shutdown_in_progress {
        enqueue_send(w, session, game_message_character_log_off());

        check_characters_for_deletion(w, session);

        if let Some(s) = w.sessions.get(session) {
            let msg = game_message_character_list(w, &s.characters, s);
            enqueue_send(w, session, msg);
        }

        let server_name_message = game_message_server_name(
            &config_server_world_name(),
            player_manager::get_online_count(w),
            w.net.config.maximum_allowed_sessions.cs_cast(),
        );
        enqueue_send(w, session, server_name_message);
    }

    set_state(w, session, SessionState::AuthConnected);
}

// ACE: Session.SendCharacterError
/// The world's `session.SendCharacterError(error)` (empyrean-net has the transport's).
pub fn send_character_error(w: &mut World, session: SessionId, error: CharacterError) {
    enqueue_send(w, session, game_message_character_error(error));
}

// ACE: Session.WorldBroadcast
/// Sends a broadcast message to the player.
pub fn world_broadcast(w: &mut World, session: SessionId, broadcast_message: &str) {
    let world_broadcast_message =
        game_message_system_chat(broadcast_message, ChatMessageType::WorldBroadcast);
    enqueue_send(w, session, world_broadcast_message);
}

// ACE: Session.ProcessDDDQueue
/// This will Network.EnqueueSend queued data files from DDDManager/DDDHandler.
pub fn process_ddd_queue(w: &mut World, session: SessionId) {
    if w.sessions
        .get(session)
        .is_none_or(|s| s.ddd_data_queue.is_none())
    {
        return;
    }

    let clock = SnapshotClock(w.now);
    // DIVERGE (V437): a client that keeps overlays has a limiter of its own, at its configured
    // rate, and may take several records a tick.
    if w.sessions
        .get(session)
        .is_some_and(|s| s.overlay_records_per_minute > 0)
    {
        process_overlay_ddd_queue(w, session, &clock);
        return;
    }
    let limiter = w
        .sessions
        .ddd_data_queue_rate_limiter
        .get_or_insert_with(|| RateLimiter::new(1000, TimeSpan::from_minutes(1.0), &clock));
    if limiter.get_seconds_to_wait_before_next_event(&clock) > 0.0 {
        return;
    }

    let now = w.now.utc;
    let Some(s) = w.sessions.get_mut(session) else {
        return;
    };

    // give a few seconds breathing room for BeginDDD pack to be sent and arrive before starting transmission from queue
    if s.begin_ddd_sent_time != DotNetDateTime::MIN_VALUE
        && now < s.begin_ddd_sent_time.add_seconds(5.0)
    {
        return;
    }

    if s.begin_ddd_sent_time != DotNetDateTime::MIN_VALUE {
        s.begin_ddd_sent_time = DotNetDateTime::MIN_VALUE;
    }

    let data_file = s.ddd_data_queue.as_mut().and_then(VecDeque::pop_front);
    if let Some((dat_file_id, dat_database_type)) = data_file {
        let msg = game_message_ddd_data_message(w, dat_file_id, dat_database_type);
        enqueue_send(w, session, msg);
        if let Some(limiter) = w.sessions.ddd_data_queue_rate_limiter.as_mut() {
            limiter.register_event(&clock);
        }
    }
}

/// Not ACE (V437): the most patch records one world tick sends one client that keeps overlays.
/// Every queued record is a whole message the transport fragments into the session's packets at
/// its next send; a handful a tick keeps one patching session from taking a whole tick from the
/// world (sessions share the world thread), and the rate bounds what the socket is handed.
pub const OVERLAY_RECORDS_PER_TICK: usize = 32;

/// `ProcessDDDQueue` for a client that keeps overlays: after ACE's pause behind `BeginDDD`, up to
/// [`OVERLAY_RECORDS_PER_TICK`] records a tick, at the session's own rate and within its bytes a
/// second (counted a tenth of a second at a time, so no burst outruns the client's receive
/// buffer).
fn process_overlay_ddd_queue(w: &mut World, session: SessionId, clock: &SnapshotClock) {
    let now = w.now.utc;
    let Some(s) = w.sessions.get_mut(session) else {
        return;
    };
    if s.begin_ddd_sent_time != DotNetDateTime::MIN_VALUE
        && now < s.begin_ddd_sent_time.add_seconds(5.0)
    {
        return;
    }
    s.begin_ddd_sent_time = DotNetDateTime::MIN_VALUE;
    let rate = i32::try_from(s.overlay_records_per_minute).unwrap_or(i32::MAX);
    for _ in 0..OVERLAY_RECORDS_PER_TICK {
        let Some(s) = w.sessions.get_mut(session) else {
            return;
        };
        let limiter = s
            .overlay_rate_limiter
            .get_or_insert_with(|| RateLimiter::new(rate, TimeSpan::from_minutes(1.0), clock));
        if limiter.get_seconds_to_wait_before_next_event(clock) > 0.0 {
            return;
        }
        if s.overlay_window.0 == DotNetDateTime::MIN_VALUE
            || now >= s.overlay_window.0.add_seconds(0.1)
        {
            s.overlay_window = (now, 0);
        }
        let budget = u64::from(s.overlay_bytes_per_second.max(1)).div_ceil(10);
        if s.overlay_window.1 >= budget {
            return;
        }
        let Some((dat_file_id, dat_database_type)) =
            s.ddd_data_queue.as_mut().and_then(VecDeque::pop_front)
        else {
            return;
        };
        let msg = game_message_ddd_data_message(w, dat_file_id, dat_database_type);
        let sent = msg.data.len() as u64;
        if let Some(s) = w.sessions.get_mut(session) {
            s.overlay_window.1 += sent;
        }
        enqueue_send(w, session, msg);
        if let Some(limiter) = w
            .sessions
            .get_mut(session)
            .and_then(|s| s.overlay_rate_limiter.as_mut())
        {
            limiter.register_event(clock);
        }
    }
}

// ACE: Session.TickOutbound
/// The game half of `TickOutbound`, for every session: the 6-second log-off, the 100-second
/// character-select ping reply and the DDD queue. It runs after `DoSessionWork` (empyrean-net's half),
/// so what it queues leaves with the next `DoSessionWork`, as what ACE queues after
/// `Network.Update()` does.
///
/// Not ACE: a game half whose transport session is gone is dropped once its player (if any) is no
/// longer online (ACE's `Session` object simply becomes garbage).
pub fn tick_outbound(w: &mut World) {
    let ids: Vec<SessionId> = w.sessions.iter().map(|(id, _)| id).collect();
    for session in ids {
        let Some(transport) = w.net.session(session) else {
            let player = w.sessions.player(session);
            if player.is_none_or(|p| player_manager::get_online_player(w, p.full()).is_none()) {
                w.sessions.remove(session);
                // Not ACE: nothing holds the logged-off Player now (ACE's garbage collector takes it).
                if let Some(player) = player {
                    crate::world_objects::player::release_logged_off_player(w, player);
                }
            }
            continue;
        };

        // A session being booted, terminating or just timed out does no game work.
        if transport.core.pending_termination.is_some()
            || transport.core.state == SessionState::TerminationStarted
        {
            continue;
        }

        let now = w.now.utc;
        let Some(s) = w.sessions.get_mut(session) else {
            continue;
        };

        // Live server seemed to take about 6 seconds. 4 seconds is nice because it has smooth animation, and saves the user 2 seconds every logoff
        // This could be made 0 for instant logoffs.
        if s.log_off_request_time != DotNetDateTime::MIN_VALUE
            && s.log_off_request_time.add_seconds(6.0) <= now
        {
            send_final_log_off_messages(w, session);
        }

        let Some(s) = w.sessions.get_mut(session) else {
            continue;
        };
        // This section deviates from known retail pcaps/behavior, but appears to be the least harmful way to work around something that seemingly didn't occur to players using ThwargLauncher connecting to retail servers.
        // In order to prevent the launcher from thinking the session is dead, we will send a Ping Response every 100 seconds, this will in effect make the client appear active to the launcher and allow players to create characters in peace.
        if s.state == SessionState::AuthConnected {
            // TODO: why is this needed? Why didn't retail have this problem? Is this fuzzy memory?
            if s.last_character_select_ping_reply == DotNetDateTime::MIN_VALUE {
                s.last_character_select_ping_reply = now.add_seconds(100.0);
            } else if now > s.last_character_select_ping_reply {
                let msg = game_event_ping_response(s);
                s.last_character_select_ping_reply = now.add_seconds(100.0);
                enqueue_send(w, session, msg);
            }
        } else if s.last_character_select_ping_reply != DotNetDateTime::MIN_VALUE {
            s.last_character_select_ping_reply = DotNetDateTime::MIN_VALUE;
        }

        process_ddd_queue(w, session);
    }
}

/// Not ACE: `session.State = state` on both halves (see the module docs).
pub fn set_state(w: &mut World, session: SessionId, state: SessionState) {
    if let Some(s) = w.sessions.get_mut(session) {
        s.state = state;
    }
    w.net.set_state(session, state);
}

/// `ConfigManager.Config.Server.WorldName`.
///
/// # Panics
/// Before `ConfigManager.Initialize` (ACE's `NullReferenceException`).
#[must_use]
pub fn config_server_world_name() -> String {
    empyrean_common::config_manager::ConfigManager::config()
        .server
        .world_name
        .clone()
}

/// `Player.CharacterChangesDetected` (Player_Database.cs); false for a player no longer in the
/// store.
fn player_character_changes_detected(w: &World, player: ObjectGuid) -> bool {
    w.objects
        .get(player)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_database.character_changes_detected)
}

/// `Player.SaveCharacterToDatabase()` (Player_Database.cs).
fn player_save_character_to_database(w: &mut World, player: ObjectGuid) {
    crate::world_objects::player_database::save_character_to_database(w, player);
}

/// `Player.LogOut(clientSessionTerminatedAbruptly, forceImmediate)` (Player.cs).
fn player_log_out(
    w: &mut World,
    player: ObjectGuid,
    client_session_terminated_abruptly: bool,
    force_immediate: bool,
) -> bool {
    crate::world_objects::player::log_out(
        w,
        player,
        client_session_terminated_abruptly,
        force_immediate,
    )
}
