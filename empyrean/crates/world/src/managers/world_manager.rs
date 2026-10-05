// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/WorldManager.cs
//! Port of `Source/ACE.Server/Managers/WorldManager.cs`.
//!
//! # The world loop
//!
//! ACE's `UpdateWorld` is a `while (!pendingWorldStop)` loop on the "World Manager" thread. Its body
//! is [`World::tick`], a step function over an injected clock reading; the
//! loop around it is [`update_world`], which adds ACE's `Thread.Sleep` and advances
//! `Timers.PortalYearTicks` by the iteration's measured time. The binary runs [`update_world`] with
//! the system clock; the test harness calls [`World::tick`] on a virtual clock.
//!
//! One iteration runs, in ACE's order:
//!
//! 1. *(not ACE)* the network hand-off: the datagrams received since the last iteration go to the
//!    transport, and its events reach the world ([`handle_network_events`]). In ACE the listener
//!    threads do this the moment a datagram arrives; here the world thread does it at the top of
//!    each iteration;
//! 2. `PlayerManager.Tick`;
//! 3. `NetworkManager.InboundMessageQueue.RunActions()`;
//! 4. *(not ACE)* the shard callbacks ([`run_shard_callbacks`]; see below);
//! 5. `ActionQueue.RunActions()`;
//! 6. `DelayManager.RunActions()`;
//! 7. [`update_game_world`], at most 60 times a second (`LandblockManager.Tick` and
//!    `HouseManager.Tick` are `not_ported!`);
//! 8. `NetworkManager.DoSessionWork()`, through the [`NetDriver`], then the game half of each
//!    session's `TickOutbound` (`sessions::tick_outbound`);
//! 9. `ServerPerformanceMonitor.Tick()`;
//! 10. *(not ACE)* `PropertyManager`'s 300 s worker timer
//!     (`property_manager::run_worker_timer`): ACE's `System.Timers.Timer` raises `DoWork` on a
//!     pool thread; here the world thread checks the timer once per iteration.
//!
//! # Where the shard's callbacks land
//!
//! ACE's `SerializedShardDatabase` runs each job as a `Task` on its own thread and invokes the
//! job's callback there, at once. `UpdateWorld`'s own comment sets the rule for those callbacks:
//! "Database results are returned from a task spawned in SerializedShardDatabase (via callback).
//! Minimal processing should be done from the callback. [...] The processing of these results
//! should be queued to an ActionQueue", and above `ActionQueue.RunActions()`: "This will consist of
//! PlayerEnterWorld actions, as well as other game world actions that require thread safety".
//! `WorldManager.PlayerEnterWorld` is the pattern: its `GetPossessedBiotasInParallel` callback does
//! `ActionQueue.EnqueueAction(... DoPlayerEnterWorld ...)`, and `Container`'s inventory load does
//! `EnqueueAction(... SortBiotasIntoInventory ...)` on the container's own queue.
//!
//! Here the callbacks run on the world thread, in a stage of their own placed
//! immediately before `ActionQueue.RunActions()`: a result that arrived since the last iteration is
//! queued by its callback and processed in this iteration's world-queue stage (or, for an object's
//! own queue, in this iteration's `UpdateGameWorld`), which is where ACE processes a result that
//! arrives before `ActionQueue.RunActions()` begins. Each callback runs inside its own
//! `catch_unwind`, logged with the message ACE's `SerializedShardDatabase.DoWork` logs when a task
//! (and so its callback) throws, and the rest still run.
//!
//! Each stage runs inside `catch_unwind`: a panic is logged with the stage's name, counted in
//! [`WorldManagerState::stage_exceptions`], and the loop goes on.
//! DIVERGE: ACE has no such guard; an exception escaping a stage ends the world thread and, being
//! unhandled, the process.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use empyrean_common::clock::{Clock, ClockSnapshot, SnapshotClock, Stopwatch};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::performance::rate_limiter::RateLimiter;
use empyrean_entity::position::Position;
use empyrean_entity::ObjectGuid;
use empyrean_net::{Event, ServerNet, SessionId};

use crate::entity::actions::action_queue::{self, ActionQueue};
use crate::entity::actions::delay_manager::{self, DelayManager};
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::entity::timers;
use crate::managers::property_manager;
use crate::managers::server_performance_monitor::{self as perf, MonitorType};
use crate::network::managers::inbound_message_manager;
use crate::sessions::{self, SessionData};
use crate::World;

// ACE: WorldManager.WorldStatusState
/// Whether players may enter the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorldStatusState {
    #[default]
    Closed,
    Open,
}

/// The mutable static state of ACE's `WorldManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct WorldManagerState {
    // ACE: WorldManager.ActionQueue
    pub action_queue: ActionQueue,
    // ACE: WorldManager.DelayManager
    pub delay_manager: DelayManager,
    // ACE: WorldManager.WorldActive
    /// True while [`update_world`] is looping.
    pub world_active: bool,
    // ACE: WorldManager.pendingWorldStop
    /// Set by [`stop_world`]; the loop ends at its next check.
    pub pending_world_stop: bool,
    // ACE: WorldManager.WorldStatus
    pub world_status: WorldStatusState,
    // ACE: WorldManager.updateGameWorldRateLimiter
    /// `new RateLimiter(60, TimeSpan.FromSeconds(1))`. ACE builds it when the class is first used;
    /// here at the first [`update_game_world`], because its stopwatch reads the world's clock.
    pub update_game_world_rate_limiter: Option<RateLimiter>,
    /// Not ACE: panics caught around the loop's stages (see the module docs).
    pub stage_exceptions: u64,
    /// Not ACE: shard callbacks that panicked in [`run_shard_callbacks`].
    pub shard_callback_exceptions: u64,
}

/// What one iteration of the loop reports to [`update_world`], which decides the sleep from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateWorldTick {
    /// `UpdateGameWorld()` ran (the 60 Hz limiter let it through).
    pub game_world_updated: bool,
    /// `NetworkManager.DoSessionWork()`'s session count.
    pub session_count: usize,
}

/// What carries the transport's datagrams: ACE's listener threads and socket sends. `empyrean-server`
/// implements it over UDP, `empyrean-testkit` over `MemoryNet`.
pub trait NetDriver {
    /// Not ACE: hands `net` every datagram received since the last call. ACE's listener threads
    /// call `NetworkManager.ProcessPacket` as each datagram arrives.
    fn receive(&mut self, net: &mut ServerNet, now: ClockSnapshot);

    /// `NetworkManager.DoSessionWork()`: ticks every session (ACK, resend, send) and puts what they
    /// produce on the wire. Returns the session count.
    fn do_session_work(&mut self, net: &mut ServerNet, now: ClockSnapshot) -> usize;
}

/// A driver with no wire: what the sessions send is dropped. For worlds without clients.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoWire;

impl NetDriver for NoWire {
    fn receive(&mut self, _net: &mut ServerNet, _now: ClockSnapshot) {}

    fn do_session_work(&mut self, net: &mut ServerNet, now: ClockSnapshot) -> usize {
        // The pass's own count, including sessions dropped during it, as ACE counts them (F15b).
        let count = net.do_session_work(now);
        net.drain_outgoing().for_each(drop);
        count
    }
}

/// What ACE does beside the loop on other threads, and the loop's `Thread.Sleep`.
pub trait WorldHost {
    /// `Thread.Sleep(duration)`.
    fn sleep(&mut self, duration: Duration);

    /// Not ACE: runs on the world thread after each iteration. ACE's console and shutdown threads
    /// work concurrently with the loop (commands, `ServerManager.ShutdownServer`'s wait loop);
    /// here they get a turn between iterations.
    fn between_iterations(&mut self, w: &mut World);
}

// ACE: WorldManager.Initialize
/// Logs what ACE logs when the world starts. ACE also starts the "World Manager" thread here; the
/// caller does that (`empyrean-server`), running [`world_thread`] on it.
pub fn initialize(w: &World) {
    log::debug!(
        "ServerTime initialized to {}",
        timers::world_start_lore_time(w)
    );
    log::debug!(
        "Current maximum allowed sessions: {}",
        w.net.config.maximum_allowed_sessions
    );

    let world_closed = property_manager_get_bool_world_closed(w);
    log::info!(
        "World started and is currently {:?}{}",
        w.world_manager.world_status,
        if world_closed {
            ""
        } else {
            " and will open automatically when server startup is complete."
        }
    );
    if w.world_manager.world_status == WorldStatusState::Closed {
        log::info!("To open world to players, use command: world open");
    }
}

/// `PropertyManager.GetBool("world_closed", false).Item`.
#[must_use]
pub fn property_manager_get_bool_world_closed(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "world_closed", false, true).item
}

/// The body of the thread `Initialize` starts: `LandblockManager.PreloadConfigLandblocks()`, then
/// [`update_world`].
pub fn world_thread(
    w: &mut World,
    driver: &mut dyn NetDriver,
    clock: &dyn Clock,
    host: &mut dyn WorldHost,
) {
    crate::managers::landblock_manager::preload_config_landblocks(
        w,
        &empyrean_common::config_manager::ConfigManager::config().server,
    );
    update_world(w, driver, clock, host);
}

// ACE: WorldManager.Open
pub fn open(w: &mut World, player: Option<ObjectGuid>) {
    w.world_manager.world_status = WorldStatusState::Open;
    crate::managers::player_manager::broadcast_to_audit_channel(w, player, "World is now open");
}

// ACE: WorldManager.Close
pub fn close(w: &mut World, player: Option<ObjectGuid>, boot_players: bool) {
    w.world_manager.world_status = WorldStatusState::Closed;
    let mut msg = "World is now closed".to_owned();
    if boot_players {
        msg += ", and booting all online players.";
    }

    crate::managers::player_manager::broadcast_to_audit_channel(w, player, &msg);

    if boot_players {
        crate::managers::player_manager::boot_all_players(w);
    }
}

// ACE: WorldManager.PlayerEnterWorld
/// Loads the character's possessions from the shard, then enqueues `DoPlayerEnterWorld` on the
/// world queue.
pub fn player_enter_world(
    w: &mut World,
    session: SessionId,
    character: crate::sessions::CharacterSummary,
) {
    let Some(offline_player) = crate::managers::player_manager::get_offline_player(w, character.id)
    else {
        log::error!("PlayerEnterWorld requested for character.Id 0x{:08X} not found in PlayerManager OfflinePlayers.", character.id);
        return;
    };
    // ACE hands `offlinePlayer.Biota` itself to the action; the biota is read when it runs, and this
    // copy stands in only if the offline player is gone by then.
    let player_biota = offline_player.biota.clone();

    let start = w.now.utc;
    let character_id = character.id;
    w.shard.get_possessed_biotas_in_parallel(
        character_id,
        Some(Box::new(
            move |w: &mut World,
                  biotas: empyrean_store::entity::possessed_biotas::PossessedBiotas| {
                log::debug!(
                    "GetPossessedBiotasInParallel for {} took {} ms",
                    character.name,
                    empyrean_common::dotnet::format::format(
                        (w.now.utc - start).total_milliseconds(),
                        "N0"
                    )
                );

                enqueue_action(
                    w,
                    Action::delegate(move |w: &mut World| {
                        let player_biota =
                            crate::managers::player_manager::get_offline_player(w, character_id)
                                .map_or(player_biota, |o| o.biota.clone());
                        do_player_enter_world(w, session, character, player_biota, biotas);
                    }),
                );
            },
        )),
    );
}

// ACE: WorldManager.DoPlayerEnterWorld
/// The character load's end, on the world queue: the no-log landblock check, the weenie-type
/// fix-up for elevated accounts, `new Player` (or Admin/Sentinel) with its possessions, the
/// property strip or restore, the fallback location, `Player.PlayerEnterWorld` (the login message
/// sequence), placement through `LandblockManager.AddObject` (with ACE's relocation when the
/// player cannot be placed), then the dat warning, the popups, the welcome text and the MOTD.
#[allow(clippy::too_many_lines)]
pub fn do_player_enter_world(
    w: &mut World,
    session: SessionId,
    mut character: crate::sessions::CharacterSummary,
    mut player_biota: empyrean_entity::Biota,
    possessed_biotas: empyrean_store::entity::possessed_biotas::PossessedBiotas,
) {
    use empyrean_entity::enums::{
        AccessLevel, ChatMessageType, CloakStatus, PhysicsState, PropertyBool, PropertyInt,
        WeenieType,
    };

    use crate::dispatch::Class;
    use crate::network::game_event::events::game_event_popup_string::game_event_popup_string;
    use crate::network::game_messages::game_message;
    use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
    use crate::physics::phys_ext;
    use crate::world_objects::world_object::CtorEnv;

    let player_logged_in_on_no_log_landblock =
        crate::world_objects::player_location::handle_no_log_landblock(&mut player_biota);

    let mut strip_admin_properties = false;
    let mut add_admin_properties = false;
    let mut add_sentinel_properties = false;
    let override_character_permissions = w
        .auth
        .lock()
        .accounts_config()
        .override_character_permissions;
    let access_level = w
        .sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level);
    if override_character_permissions {
        if access_level <= AccessLevel::Advocate {
            // check for elevated characters
            if player_biota.weenie_type == WeenieType::Admin
                || player_biota.weenie_type == WeenieType::Sentinel
            {
                // Downgrade weenie
                character.is_plussed = false;
                player_biota.weenie_type = WeenieType::Creature;
                strip_admin_properties = true;
            }
        } else if access_level >= AccessLevel::Sentinel && access_level <= AccessLevel::Envoy {
            if player_biota.weenie_type == WeenieType::Creature
                || player_biota.weenie_type == WeenieType::Admin
            {
                // Up/downgrade weenie
                character.is_plussed = true;
                player_biota.weenie_type = WeenieType::Sentinel;
                add_sentinel_properties = true;
            }
        } else {
            // Developers and Admins
            if player_biota.weenie_type == WeenieType::Creature
                || player_biota.weenie_type == WeenieType::Sentinel
            {
                // Up/downgrade weenie
                character.is_plussed = true;
                player_biota.weenie_type = WeenieType::Admin;
                add_admin_properties = true;
            }
        }
    }

    let class = if player_biota.weenie_type == WeenieType::Admin {
        Class::Admin
    } else if player_biota.weenie_type == WeenieType::Sentinel {
        Class::Sentinel
    } else {
        Class::Player
    };
    let total_logins = character.total_logins;
    let mut player = CtorEnv::with_world(w, |env| {
        crate::world_objects::player::player_from_biota_with_character(
            env,
            class,
            player_biota,
            possessed_biotas.inventory,
            possessed_biotas.wielded_items,
            character,
            Some(session),
        )
    });
    let player_guid = player.guid;

    // What the constructor could not reach (see `PlayerFields`): the session's elevation, and its
    // possessions into 4.5a's Inventory and EquippedObjects.
    // DIVERGE: ACE's constructor elevates the Session itself and sorts the possessions into the
    // Player's collections; a constructor here cannot reach World, so both happen right after it
    // returns, before anything reads them (arch).
    let elevation = player
        .player
        .as_mut()
        .expect("a player")
        .player
        .pending_session_access_level
        .take();
    if let Some(access_level) = elevation {
        if let Some(s) = w.sessions.get_mut(session) {
            s.set_access_level(access_level);
        }
    }
    if w.objects.insert(player).is_err() {
        panic!("DoPlayerEnterWorld: 0x{player_guid} is already in the world");
    }
    crate::world_objects::player::player_ctor_load_possessions(w, player_guid);

    if let Some(s) = w.sessions.get_mut(session) {
        s.set_player(Some(player_guid));
    }

    if strip_admin_properties {
        // continue stripping properties
        {
            let o = w.objects.get_mut(player_guid).expect("inserted");
            o.set_cloak_status(CloakStatus::Undef);
            o.set_attackable(true);
            o.set_property(PropertyBool::DamagedByCollisions, true);
            o.set_advocate_level(None);
            o.set_channels_active(None);
            o.set_channels_allowed(None);
            o.set_invincible(false);
        }
        // `Cloaked = null` (SetPhysicsState)
        phys_ext::set_physics_state(w, player_guid, PhysicsState::Cloaked, None);
        {
            let o = w.objects.get_mut(player_guid).expect("inserted");
            o.set_ignore_house_barriers(false);
            o.set_ignore_portal_restrictions(false);
            o.set_safe_spell_components(false);
        }
        // `ReportCollisions = true` (SetPhysicsPropertyState)
        phys_ext::set_physics_property_state(
            w,
            player_guid,
            PropertyBool::ReportCollisions,
            PhysicsState::ReportCollisions,
            Some(true),
        );

        let o = w.objects.get_mut(player_guid).expect("inserted");
        o.wo.world_object_database.changes_detected = true;
        o.player
            .as_mut()
            .expect("a player")
            .player_database
            .character_changes_detected = true;
    }

    if add_sentinel_properties || add_admin_properties {
        // continue restoring properties to default
        let name = if add_admin_properties {
            "admin"
        } else {
            "sentinel"
        };
        let cached = w.content.get_cached_weenie_by_class_name(name);
        let weenie = CtorEnv::with_world(w, |env| {
            crate::factories::world_object_factory::create_world_object(
                env,
                cached,
                ObjectGuid::new(ObjectGuid::INVALID.full()),
            )
        });

        if let Some(weenie) = weenie {
            {
                let o = w.objects.get_mut(player_guid).expect("inserted");
                o.set_cloak_status(CloakStatus::Off);
                o.set_attackable(weenie.attackable());
                o.set_property(PropertyBool::DamagedByCollisions, false);
                o.set_advocate_level(weenie.get_property(PropertyInt::AdvocateLevel));
                o.set_channels_active(
                    weenie
                        .get_property(PropertyInt::ChannelsActive)
                        .map(empyrean_entity::enums::Channel),
                );
                o.set_channels_allowed(
                    weenie
                        .get_property(PropertyInt::ChannelsAllowed)
                        .map(empyrean_entity::enums::Channel),
                );
                o.set_invincible(false);
            }
            // `Cloaked = false` (SetPhysicsState)
            phys_ext::set_physics_state(w, player_guid, PhysicsState::Cloaked, Some(false));

            let o = w.objects.get_mut(player_guid).expect("inserted");
            o.wo.world_object_database.changes_detected = true;
            o.player
                .as_mut()
                .expect("a player")
                .player_database
                .character_changes_detected = true;
        }
    }

    // If the client is missing a location, we start them off in the starter town they chose
    {
        let o = w.objects.get_mut(player_guid).expect("inserted");
        if o.location().is_none() {
            let location = match o.instantiation() {
                Some(instantiation) => Position::from_position(&instantiation),
                None => ultimate_fallback_position(), // ultimate fallback
            };
            o.set_location(Some(location));
        }
    }

    let olthoi_player_returned_to_lifestone = {
        let o = w.objects.get(player_guid).expect("inserted");
        let is_olthoi_player = o
            .player
            .as_ref()
            .is_some_and(|p| p.player_properties.is_olthoi_player);
        is_olthoi_player && total_logins >= 1 && o.login_at_lifestone()
    };
    if olthoi_player_returned_to_lifestone {
        let o = w.objects.get_mut(player_guid).expect("inserted");
        let sanctuary = o
            .sanctuary()
            .expect("ACE: new Position(null) (NullReferenceException)");
        o.set_location(Some(Position::from_position(&sanctuary)));
    }

    crate::world_objects::player_networking::player_enter_world(w, player_guid);

    let success = crate::managers::landblock_manager::add_object(w, player_guid, true);
    if !success {
        // send to lifestone, or fallback location
        let fix_loc = w
            .objects
            .get(player_guid)
            .and_then(crate::world_objects::world_object::WorldObject::sanctuary)
            .unwrap_or_else(ultimate_fallback_position);

        log::error!(
            "WorldManager.DoPlayerEnterWorld: failed to spawn {}, relocating to {}",
            crate::dispatch::name::name(w, player_guid).unwrap_or_default(),
            fix_loc.to_loc_string()
        );

        if let Some(o) = w.objects.get_mut(player_guid) {
            o.set_location(Some(Position::from_position(&fix_loc)));
        }
        crate::managers::landblock_manager::add_object(w, player_guid, true);

        let mut action_chain = crate::entity::actions::action_chain::ActionChain::new();
        action_chain.add_delay_seconds(w, 5.0);
        action_chain.add_action(Actor::Object(player_guid), move |w: &mut World| {
            // `if (session != null && session.Player != null) session.Player.Teleport(fixLoc);`
            if let Some(player) = w.sessions.get(session).and_then(|s| s.player) {
                crate::world_objects::player_location::teleport(w, player, &fix_loc, false);
            }
        });
        action_chain.enqueue_chain(w);
    }

    // These warnings are set by DDD_InterrogationResponse
    let dat_warn = w.sessions.get(session).is_some_and(|s| {
        s.dat_warn_cell || s.dat_warn_language || s.dat_warn_portal || s.dat_warn_high_res
    });
    if dat_warn && property_manager::get_bool(w, "show_dat_warning", false, true).item {
        let msg = property_manager::get_string(w, "dat_older_warning_msg", "", true).item;
        let chat_msg = game_message_system_chat(&msg, ChatMessageType::System);
        game_message::enqueue_send(w, session, chat_msg);
    }

    let popup_header = property_manager::get_string(w, "popup_header", "", true).item;
    let popup_motd = property_manager::get_string(w, "popup_motd", "", true).item;
    let is_olthoi_player = w
        .objects
        .get(player_guid)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player);
    let popup_welcome = if is_olthoi_player {
        property_manager::get_string(w, "popup_welcome_olthoi", "", true).item
    } else {
        property_manager::get_string(w, "popup_welcome", "", true).item
    };

    // `character.TotalLogins`: PlayerEnterWorld has counted this login.
    let total_logins = w
        .objects
        .get(player_guid)
        .and_then(crate::world_objects::world_object_networking::shims::player_character)
        .map_or(total_logins, |c| c.total_logins);
    // DIVERGE: an era whose first login opens the Welcome Letter (`EraRules::welcome_letter`)
    // uses the letter from the starter gear, so it lies open for the new player to read, rather
    // than showing the training halls' welcome (ClassicACE's `PlayerEnterWorld` at its older
    // rulesets).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Managers/WorldManager.cs
    if let Some(letter_wcid) = w.era.welcome_letter.filter(|_| total_logins <= 1) {
        let letter = crate::world_objects::container::inventory_values(w, player_guid)
            .into_iter()
            .find(|&g| {
                w.objects
                    .get(g)
                    .is_some_and(|o| o.biota.weenie_class_id == letter_wcid)
            });
        if let Some(letter) = letter {
            crate::dispatch::act_on_use::act_on_use(w, letter, player_guid);
        }
    } else if total_logins <= 1 {
        let text = if is_olthoi_player {
            append_lines(&[&popup_welcome, &popup_motd])
        } else {
            append_lines(&[&popup_header, &popup_motd, &popup_welcome])
        };
        let msg = popup_string(w, session, &text);
        game_message::enqueue_send(w, session, msg);
    } else if !popup_motd.is_empty() {
        let text = append_lines(&[&popup_header, &popup_motd]);
        let msg = popup_string(w, session, &text);
        game_message::enqueue_send(w, session, msg);
    }

    // DIVERGE: ACE's welcome names ACEmulator, its own repository and `@acehelp`; ours names Empyrean,
    // this server's source (AGPL-3.0 section 13: `server.source_url`, else the build's) and `@emphelp`.
    let info = empyrean_common::brand::welcome(&empyrean_common::brand::source_url());
    game_message::enqueue_send(
        w,
        session,
        game_message_system_chat(&info, ChatMessageType::Broadcast),
    );

    let server_motd = property_manager::get_string(w, "server_motd", "", true).item;
    if !server_motd.is_empty() {
        game_message::enqueue_send(
            w,
            session,
            game_message_system_chat(&format!("{server_motd}\n"), ChatMessageType::Broadcast),
        );
    }

    if olthoi_player_returned_to_lifestone {
        game_message::enqueue_send(
            w,
            session,
            game_message_system_chat(
                "You have returned to the Olthoi Queen to serve the hive.",
                ChatMessageType::Broadcast,
            ),
        );
    } else if player_logged_in_on_no_log_landblock {
        // see http://acpedia.org/wiki/Mount_Elyrii_Hive
        game_message::enqueue_send(
            w,
            session,
            game_message_system_chat(
                "The currents of portal space cannot return you from whence you came. Your previous location forbids login.",
                ChatMessageType::Broadcast,
            ),
        );
    }

    fn popup_string(
        w: &mut World,
        session: SessionId,
        text: &str,
    ) -> crate::network::game_messages::game_message::GameMessage {
        let data = w
            .sessions
            .get_mut(session)
            .expect("ACE: session is null (NullReferenceException)");
        game_event_popup_string(data, text)
    }
}

/// `new Position(0xA9B40019, 84, 7.1f, 94, 0, 0, -0.0784591f, 0.996917f)`: DoPlayerEnterWorld's
/// ultimate fallback location.
fn ultimate_fallback_position() -> Position {
    Position::from_components(
        0xA9B4_0019,
        84.0,
        7.1,
        94.0,
        0.0,
        0.0,
        -0.078_459_1,
        0.996_917,
        false,
    )
}

// ACE: WorldManager.AppendLines
/// Joins the non-empty lines, each followed by `\n`, then `Regex.Replace(result, "\n$", "")`.
///
/// .NET's `$` also matches before a final `\n`, so the replace removes the last `\n` and, when the
/// text ends in two, the one before it too (a line that already ends in `\n` loses both).
pub fn append_lines(lines: &[&str]) -> String {
    let mut result = String::new();
    for line in lines {
        if !line.is_empty() {
            result += line;
            result.push('\n');
        }
    }

    if result.ends_with("\n\n") {
        result.truncate(result.len() - 2);
    } else if result.ends_with('\n') {
        result.truncate(result.len() - 1);
    }
    result
}

// ACE: WorldManager.ThreadSafeTeleport
/// Enqueues the teleport on the world queue (it runs next tick), then `action_to_follow_up_with`.
pub fn thread_safe_teleport(
    w: &mut World,
    player: ObjectGuid,
    new_position: Position,
    action_to_follow_up_with: Option<Action>,
    from_portal: bool,
) {
    enqueue_action(
        w,
        Action::delegate(move |w: &mut World| {
            crate::world_objects::player_location::teleport(w, player, &new_position, from_portal);

            if let Some(action_to_follow_up_with) = action_to_follow_up_with {
                enqueue_action(w, action_to_follow_up_with);
            }
        }),
    );
}

// ACE: WorldManager.EnqueueAction
pub fn enqueue_action(w: &mut World, action: Action) {
    w.world_manager.action_queue.enqueue_action(action);
}

// ACE: WorldManager.UpdateWorld
/// The world loop: runs [`World::tick`] until [`stop_world`], sleeping as ACE does, and advances
/// `Timers.PortalYearTicks` by each iteration's measured time (`worldTickTimer`).
pub fn update_world(
    w: &mut World,
    driver: &mut dyn NetDriver,
    clock: &dyn Clock,
    host: &mut dyn WorldHost,
) {
    log::debug!("Starting UpdateWorld thread");

    w.world_manager.world_active = true;
    let mut world_tick_timer = Stopwatch::new();

    while !w.world_manager.pending_world_stop {
        world_tick_timer.restart(clock);

        let now = ClockSnapshot::take(clock, w.timers.portal_year_ticks);
        let UpdateWorldTick {
            game_world_updated,
            session_count,
        } = w.tick(now, driver);

        // We only relax the CPU if our game world is able to update at the target rate.
        // We do not sleep if our game world just updated. This is to prevent the scenario where our game world can't keep up. We don't want to add further delays.
        // If our game world is able to keep up, it will not be updated on most ticks. It's on those ticks (between updates) that we will relax the CPU.
        if !game_world_updated {
            host.sleep(Duration::from_millis(if session_count == 0 {
                10
            } else {
                1
            }));
            // Relax the CPU more if no sessions are connected
        }

        timers::advance_portal_year_ticks(w, world_tick_timer.elapsed(clock));

        host.between_iterations(w);
    }

    // World has finished operations and concedes the thread to garbage collection
    w.world_manager.world_active = false;
}

/// Not ACE: the status ping's live facts (V442), read before the pass's datagrams: whether the
/// world is open and how many are on.
fn refresh_status_ping(w: &mut World) {
    use empyrean_net::status_ping::WorldState;
    let state = if w.server_manager.shutdown_initiated {
        WorldState::ShuttingDown
    } else if w.world_manager.world_status == WorldStatusState::Open {
        WorldState::Open
    } else {
        WorldState::Starting
    };
    let players = u16::try_from(crate::managers::player_manager::get_online_count(w).max(0))
        .unwrap_or(u16::MAX);
    if let Some(ping) = w.net.status_ping.as_mut() {
        ping.facts.state = state;
        ping.facts.players = players;
    }
}

impl World {
    /// One iteration of `WorldManager.UpdateWorld`'s loop at `now`, without its `Thread.Sleep`
    /// and without advancing `Timers.PortalYearTicks` (the caller measures the iteration; see
    /// [`update_world`]). The stages are listed in the module docs.
    pub fn tick(&mut self, now: ClockSnapshot, driver: &mut dyn NetDriver) -> UpdateWorldTick {
        self.now = now;

        refresh_status_ping(self);
        run_stage(self, "NetworkManager.ProcessPacket", (), |w| {
            driver.receive(&mut w.net, now)
        });
        handle_network_events(self);

        perf::restart_event(self, MonitorType::PlayerManagerTick);
        run_stage(
            self,
            "PlayerManager.Tick",
            (),
            crate::managers::player_manager::tick,
        );
        perf::register_event_end(self, MonitorType::PlayerManagerTick);

        perf::restart_event(
            self,
            MonitorType::NetworkManagerInboundClientMessageQueueRun,
        );
        run_stage(
            self,
            "NetworkManager.InboundMessageQueue.RunActions",
            (),
            inbound_message_manager::run_inbound_message_queue,
        );
        perf::register_event_end(
            self,
            MonitorType::NetworkManagerInboundClientMessageQueueRun,
        );

        // Not ACE: the shard's callbacks re-enter the world here (see the module docs).
        run_stage(
            self,
            "SerializedShardDatabase callbacks",
            (),
            run_shard_callbacks,
        );

        // This will consist of PlayerEnterWorld actions, as well as other game world actions that require thread safety
        perf::restart_event(self, MonitorType::ActionQueueRunActions);
        run_stage(self, "WorldManager.ActionQueue.RunActions", (), |w| {
            action_queue::run_actions(w, Actor::World)
        });
        perf::register_event_end(self, MonitorType::ActionQueueRunActions);

        perf::restart_event(self, MonitorType::DelayManagerRunActions);
        run_stage(
            self,
            "WorldManager.DelayManager.RunActions",
            (),
            delay_manager::run_actions,
        );
        perf::register_event_end(self, MonitorType::DelayManagerRunActions);

        perf::restart_event(self, MonitorType::UpdateGameWorld);
        let game_world_updated = run_stage(
            self,
            "WorldManager.UpdateGameWorld",
            false,
            update_game_world,
        );
        perf::register_event_end(self, MonitorType::UpdateGameWorld);

        perf::restart_event(self, MonitorType::NetworkManagerDoSessionWork);
        let session_count = run_stage(self, "NetworkManager.DoSessionWork", 0, |w| {
            driver.do_session_work(&mut w.net, now)
        });
        run_stage(
            self,
            "Session.TickOutbound (game half)",
            (),
            sessions::tick_outbound,
        );
        perf::register_event_end(self, MonitorType::NetworkManagerDoSessionWork);

        perf::tick(self);

        run_stage(
            self,
            "PropertyManager worker timer",
            (),
            crate::managers::property_manager::run_worker_timer,
        );

        UpdateWorldTick {
            game_world_updated,
            session_count,
        }
    }
}

// ACE: WorldManager.UpdateGameWorld
/// Projected to run at a reasonable rate for gameplay (30-60fps)
pub fn update_game_world(w: &mut World) -> bool {
    let clock = SnapshotClock(w.now);
    let limiter = w
        .world_manager
        .update_game_world_rate_limiter
        .get_or_insert_with(|| RateLimiter::new(60, TimeSpan::from_seconds(1.0), &clock));

    if limiter.get_seconds_to_wait_before_next_event(&clock) > 0.0 {
        return false;
    }

    limiter.register_event(&clock);

    perf::restart_cumulative_events(w);
    perf::restart_event(w, MonitorType::UpdateGameWorldEntire);

    let portal_year_ticks = w.timers.portal_year_ticks;
    crate::managers::landblock_manager::tick(w, portal_year_ticks);

    crate::managers::house_manager::tick(w);

    perf::register_event_end(w, MonitorType::UpdateGameWorldEntire);
    perf::register_cumulative_events(w);

    true
}

// ACE: WorldManager.StopWorld
/// Function to begin ending the operations inside of an active world.
pub fn stop_world(w: &mut World) {
    w.world_manager.pending_world_stop = true;
}

/// Not ACE: the loop's shard callback stage (see the module docs). Runs, in completion order, the
/// callbacks of the shard jobs that finished since the last call, each inside its own
/// `catch_unwind`.
pub fn run_shard_callbacks(w: &mut World) {
    for callback in w.shard.take_completed() {
        if let Err(panic) = catch_unwind(AssertUnwindSafe(|| callback(&mut *w))) {
            // What SerializedShardDatabase.DoWork logs when a task, and so its callback, throws.
            log::error!(
                "[DATABASE] DoWork task failed with exception: {}",
                panic_message(panic.as_ref())
            );
            // perhaps add failure callbacks?
            // swallow for now.  can't block other db work because 1 fails.
            w.world_manager.shard_callback_exceptions += 1;
        }
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned())
}

/// Not ACE: runs `f` as the loop stage `name`, returning `default` if it panics (see the module
/// docs).
fn run_stage<R>(w: &mut World, name: &str, default: R, f: impl FnOnce(&mut World) -> R) -> R {
    match catch_unwind(AssertUnwindSafe(|| f(&mut *w))) {
        Ok(r) => r,
        Err(panic) => {
            let message = panic_message(panic.as_ref());
            log::error!("UpdateWorld stage {name} threw an exception: {message}");
            w.world_manager.stage_exceptions += 1;
            default
        }
    }
}

/// Not ACE: hands the transport's events to the world, oldest first, each inside its own
/// `catch_unwind`. These are the world's halves of what ACE's network threads call:
///
/// - `LoginRequest`: `AuthenticationHandler.HandleLoginRequest` starts `DoLogin`. The
///   game half of the session ([`SessionData`]) is created here, when the world first hears of it.
/// - `ConnectResponse`: `AuthenticationHandler.HandleConnectResponse`.
/// - `Message`: `InboundMessageManager.HandleClientMessage`, which enqueues the handler.
/// - `SpeedHack`: the player half of `NetworkSession.VerifyEcho`.
/// - `Disconnected`: the world half of `Session.DropSession`: `LogOffPlayer()` when there is a
///   player (which keeps the session's game half, as ACE keeps `session.Player`), else the game
///   half is dropped.
pub fn handle_network_events(w: &mut World) {
    let events: Vec<Event> = w.net.events().collect();
    for event in events {
        run_stage(w, "NetworkManager (session event)", (), |w| {
            handle_network_event(w, event)
        });
    }
}

// ACE: NetworkSession.VerifyEcho
/// The world half of `VerifyEcho`'s speed-hack verdict: ACE's action chain on the
/// player, which tells the client, logs the player off and resets the echo check.
pub fn network_session_verify_echo_speed_hack(w: &mut World, session: SessionId) {
    let Some(player) = w.sessions.player(session) else {
        return;
    };
    let mut action_chain = crate::entity::actions::action_chain::ActionChain::new();
    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        //session.Network.EnqueueSend(new GameMessageBootAccount(session));
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            "TimeSync: client speed error",
            empyrean_entity::enums::ChatMessageType::Broadcast,
        );
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        sessions::log_off_player(w, session, false);

        w.net.reset_echo_verification(session);
    });
    action_chain.enqueue_chain(w);
}

fn handle_network_event(w: &mut World, event: Event) {
    match event {
        Event::LoginRequest {
            session,
            from,
            request,
        } => {
            if w.sessions.get(session).is_none() {
                w.sessions.insert(session, SessionData::default());
            }
            // Task t = new Task(() => DoLogin(session, loginRequest));
            crate::network::handlers::authentication_handler::do_login(w, session, from, &request);
        }
        Event::ConnectResponse { session } => {
            crate::network::handlers::authentication_handler::handle_connect_response(w, session);
        }
        Event::Message {
            session, message, ..
        } => {
            inbound_message_manager::handle_client_message(w, message, session);
        }
        Event::SpeedHack { session } => network_session_verify_echo_speed_hack(w, session),
        Event::Disconnected { session, .. } => {
            if w.sessions.player(session).is_some() {
                sessions::log_off_player(w, session, false);
            } else {
                w.sessions.remove(session);
            }
        }
    }
}
