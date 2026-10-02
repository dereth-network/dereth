// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Tick.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Tick.cs`.
//!
//! The movement and position members: `FastTick`, `OnMoveToState` (both methods),
//! `UpdateObjectPhysics`, `UpdatePlayerPhysics`, `UpdatePlayerPosition`, `ValidateMovement` and
//! `SyncLocationWithPhysics`, with ACE's statics; and the rest of the file (`Player_Tick`,
//! `Heartbeat`, the mana and gag ticks, `EnqueueAction`, `HandleMotionDone`).
//!
//! The server-only `PhysicsObj` members these call (`set_request_pos`, `update_object_server`,
//! `update_object_server_new`, `UpdateObjectInternalServer`, `set_current_pos`) are ACE's
//! additions to its physics port; the shared crate has no place for them, so they are ported at
//! the bottom of this file over the shared `PhysicsWorld` (see the divergence note there).

use dereth_physics::{PhysHandle, Transition};
use dereth_primitives::{CellId, Frame, Position as PPosition, Quat, Vec3};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{Quaternion, Vector3};
use empyrean_entity::enums::{MotionCommand, MotionStance, PlayerKillerStatus, Skill};
use empyrean_entity::shared_types::{data_quat, data_vec3};
use empyrean_entity::{ObjectGuid, Position};

use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::{
    game_message_update_motion, game_message_update_position,
};
use crate::network::motion::move_to_state::MoveToState;
use crate::network::motion::movement_data::{Motion, MovementData};
use crate::network::motion::raw_motion_state::RawMotionState;
use crate::network::sequence::sequence_type::SequenceType;
use crate::physics::phys_ext;
use crate::world_objects::player_move;
use crate::world_objects::world_object_networking::{self, shims};
use crate::World;

/// Non-property fields declared in `Player_Tick.cs`.
#[derive(Debug, Default)]
pub struct PlayerTickFields {
    /// The player's own queue: `Player.EnqueueAction` puts every action for the player here, and
    /// `Player_Tick` runs it.
    // ACE: Player.actionQueue
    pub action_queue: crate::entity::actions::action_queue::ActionQueue,
    // ACE: Player.initialAge
    pub initial_age: i32,
    /// `DateTime.MinValue` until the first age update (the default).
    // ACE: Player.initialAgeTime
    pub initial_age_time: empyrean_common::dotnet::datetime::DotNetDateTime,
    // ACE: Player.nextAgeUpdateTime
    // Not ACE's (retail captures, V280): no field. ACE kept a schedule of its
    // own for the `Age` update; here the update rides the player's heartbeat, whose
    // `NextHeartbeatTime` is the one schedule (see `player_heartbeat`).

    // ACE: Player.houseRentWarnTimestamp
    pub house_rent_warn_timestamp: f64,
    /// Not ACE's (retail, V255): whether the AllegianceUpdateDone that pairs with the
    /// first allegiance update of this login has been sent.
    pub login_allegiance_update_done_sent: bool,
    /// Not ACE's (retail captures, V281): the client's allegiance-update
    /// subscription (AllegianceUpdateRequest's `on_off`) is off. Kept negated so a player starts
    /// subscribed, as retail clients asked to be at 98% of logins; it lives on the player, so it
    /// lasts across landblock changes and teleports, as retail kept it across referrals.
    pub allegiance_updates_off: bool,
    /// Not ACE's (retail captures, V289): the player's view of the allegiance
    /// as the last AllegianceUpdate sent to it showed it (`allegiance_view_snapshot`); `None`
    /// until the first.
    pub allegiance_view_sent: Option<Vec<u8>>,
    /// Not ACE's (V289): when (Unix seconds) the last AllegianceUpdate was sent to the player.
    pub allegiance_update_sent_time: Option<f64>,
    /// Not ACE's (V289): when (Unix seconds) an unsubscribed player's view is next compared.
    pub allegiance_view_next_check: f64,
    /// Not ACE's (retail captures, V280): when the player's last heartbeat ran.
    pub last_heartbeat_time: Option<f64>,
    /// Not ACE's (V280): the seconds the current (or last) heartbeat credits, the time since the
    /// one before it; `None` until the first.
    pub heartbeat_elapsed: Option<f64>,
    // ACE: Player.InUpdate
    pub in_update: bool,

    /// `PhysicsObj.RequestPos` and `PhysicsObj.requestCachedVelocity`: the body's requested
    /// position. DIVERGE: fields of ACE's `PhysicsObj`, which the shared crate does not have; only
    /// players request positions, so they are kept with the player.
    pub request_pos: Option<PPosition>,
    pub request_cached_velocity: Vec3,

    // ACE: Player.gagNoticeSent
    pub gag_notice_sent: bool,
}

fn fields(w: &World, this: ObjectGuid) -> &PlayerTickFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_tick
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerTickFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_tick
}

// ACE: Player.ageUpdateInterval
/// Not ACE's (retail, V277): ACE updated `Age` every 7 s; the retail server did so every 4 to
/// 6 s, drawn uniformly and afresh each time (the retail captures: 2.6 M updates, flat from 4 to
/// 6 s, successive gaps independent). Not ACE's (retail captures, V280): that is the player's one
/// tick, which also carried the vital regeneration, the fellow vital updates and
/// AllegianceUpdateDone, so it is the player's heartbeat interval too (ACE's is a fixed 5 s). The
/// shortest gap.
pub const PLAYER_TICK_INTERVAL_MIN: f32 = 4.0;
/// Not ACE's (V277, V280): the longest gap between player ticks (exclusive).
pub const PLAYER_TICK_INTERVAL_MAX: f32 = 6.0;

/// Not ACE's (V277, V280): the seconds until the player's next heartbeat,
/// uniform in [4, 6), drawn afresh for each. ACE makes no draw here, so it comes from the
/// server's second seeded stream and leaves ACE's own sequence of draws as it was.
#[must_use]
pub fn next_player_tick_interval() -> f64 {
    let (min, max) = (PLAYER_TICK_INTERVAL_MIN, PLAYER_TICK_INTERVAL_MAX);
    empyrean_common::random::with_port_rng(|r| {
        r.next_double() * f64::from(max - min) + f64::from(min)
    })
}

// ACE: Player.houseRentWarnInterval
pub const HOUSE_RENT_WARN_INTERVAL: f64 = 3600.0;

// ACE: Player.Player_Tick
/// The player's own tick, run by its landblock every tick: the failed-save boots, the player's
/// action queue, the fellowship vital update and the house rent warning. Not ACE's (retail captures,
/// V280): ACE's 7 s `Age` update is here; it rides the player's
/// heartbeat instead (see [`player_heartbeat`]).
pub fn player_tick(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    current_unix_time: f64,
) {
    if save_failed_boot(w, this) {
        return;
    }

    crate::entity::actions::action_queue::run_actions(
        w,
        crate::entity::actions::i_actor::Actor::Object(this),
    );

    if w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_none()
    {
        return;
    }

    fellow_vital_update_tick(w, this);

    unsubscribed_allegiance_update(w, this);

    if let Some(house) = crate::world_objects::player_house::house(w, this).filter(|_| {
        crate::managers::property_manager::get_bool(w, "house_rent_enabled", false, true).item
    }) {
        let warn = w
            .objects
            .get(this)
            .expect("a player")
            .player
            .as_ref()
            .expect("a player")
            .player_tick
            .house_rent_warn_timestamp;
        if warn > 0.0 && current_unix_time > warn {
            crate::managers::house_manager::get_house(
                w,
                house.full(),
                Box::new(
                    move |w: &mut crate::World, house: empyrean_entity::ObjectGuid| {
                        let slumlord = crate::world_objects::house::slum_lord(w, house)
                            .expect("System.NullReferenceException: house.SlumLord");
                        if w.objects.get(house).is_some_and(|h| {
                            h.house_status() == empyrean_entity::enums::HouseStatus::Active
                        }) && !crate::world_objects::slum_lord::is_rent_paid(w, slumlord)
                        {
                            let days = if crate::world_objects::house::is_apartment(w, house) {
                                "90"
                            } else {
                                "30"
                            };
                            crate::world_objects::player_house::system_chat(
                            w,
                            this,
                            &format!("Warning!  You have not paid your maintenance costs for the last {days} day maintenance period.  Please pay these costs by this deadline or you will lose your house, and all your items within it."),
                            empyrean_entity::enums::ChatMessageType::Broadcast,
                        );
                        }
                    },
                ),
            );

            let next = empyrean_common::time::Time::get_unix_time_at(
                w.now.utc.add_seconds(HOUSE_RENT_WARN_INTERVAL),
            );
            w.objects
                .get_mut(this)
                .expect("a player")
                .player
                .as_mut()
                .expect("a player")
                .player_tick
                .house_rent_warn_timestamp = next;
        } else if warn == 0.0 {
            let next = empyrean_common::time::Time::get_unix_time_at(
                w.now.utc.add_seconds(HOUSE_RENT_WARN_INTERVAL),
            );
            w.objects
                .get_mut(this)
                .expect("a player")
                .player
                .as_mut()
                .expect("a player")
                .player_tick
                .house_rent_warn_timestamp = next;
        }
    }
}

/// The fellowship vital update of `Player_Tick`: a fellow whose vitals changed has its fellows
/// told, once, and the flag is cleared.
fn fellow_vital_update_tick(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    if crate::world_objects::player_fellowship::fellow_vital_update(w, this) {
        if let Some(fellowship) = crate::world_objects::player_fellowship::fellowship(w, this) {
            crate::entity::fellowship::on_vital_update(w, &fellowship, this);
            crate::world_objects::player_fellowship::set_fellow_vital_update(w, this, false);
        }
    }
}

/// The `Age` update of `Player_Tick`: `Age` becomes the age at the first update plus the whole
/// seconds since, and the client is told. Not ACE's (retail captures, V277, V280, V255): it
/// runs on each of the player's 4-6 s heartbeats rather than on a 7 s schedule of its
/// own, and AllegianceUpdateDone follows it.
fn age_update(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use empyrean_common::dotnet::CsCast;
    use empyrean_entity::enums::PropertyInt;

    let utc_now = w.now.utc;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let Some(p) = o.player.as_mut() else { return };
    if p.player_tick.initial_age_time
        == empyrean_common::dotnet::datetime::DotNetDateTime::MIN_VALUE
    {
        let age = o.age().unwrap_or(1);
        let tick = &mut o.player.as_mut().expect("a player").player_tick;
        tick.initial_age = age;
        tick.initial_age_time = utc_now;
    }

    let tick = &o.player.as_ref().expect("a player").player_tick;
    let elapsed: i32 = (utc_now - tick.initial_age_time).total_seconds().cs_cast();
    let age = tick.initial_age.wrapping_add(elapsed);
    o.set_age(Some(age));

    // The value keeps ACE's exact accumulation (whole seconds since the first update, not a
    // per-update step); the client only displays it.
    let value = o.age().unwrap_or(1);
    let msg = crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int(
        o,
        PropertyInt::Age,
        value,
    );
    let session = crate::world_objects::world_object_networking::shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);

    // Not ACE's (V255): AllegianceUpdateDone rides the Age update.
    // Not ACE's (retail captures, V281): only while the player's
    // allegiance-update subscription is on.
    // Not ACE's (retail captures, V289): when the player's view of the
    // allegiance has changed since the last AllegianceUpdate it was sent (a pass-up counter
    // anywhere in its chain, or the structure), the tick carries a fresh AllegianceUpdate instead
    // of the done. Retail had no periodic update: this is what looked like one.
    if !fields(w, this).allegiance_updates_off {
        if allegiance_view_changed(w, this) {
            send_current_allegiance_update(w, this);
        } else {
            send_allegiance_update_done(w, this);
        }
    }
}

/// Not ACE's (retail captures, V289): the shortest gap between two
/// AllegianceUpdates pushed to an unsubscribed player because its view changed.
pub const UNSUBSCRIBED_ALLEGIANCE_UPDATE_SPACING: f64 = 30.0;

/// Not ACE's (V289): how often an unsubscribed player's view is compared once the spacing has
/// passed, so the push goes out within a second of the change, off the heartbeat.
const UNSUBSCRIBED_ALLEGIANCE_VIEW_CHECK_INTERVAL: f64 = 1.0;

/// Not ACE's (retail captures, V289): whether the player's view of the
/// allegiance differs from what its last AllegianceUpdate showed. A player that was never sent
/// one has nothing to compare against, and so no change.
fn allegiance_view_changed(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    use crate::entity::i_player::IPlayer;
    use crate::world_objects::player_allegiance::{i_player_allegiance, i_player_allegiance_node};

    if fields(w, this).allegiance_view_sent.is_none() {
        return false;
    }
    let allegiance = i_player_allegiance(w, IPlayer::Online(this));
    let node = i_player_allegiance_node(w, IPlayer::Online(this));
    let view =
        crate::network::game_event::events::game_event_allegiance_update::allegiance_view_snapshot(
            w, allegiance, node,
        );
    fields(w, this).allegiance_view_sent.as_ref() != Some(&view)
}

/// Not ACE's (V289): sends the player an AllegianceUpdate of its allegiance as it stands.
fn send_current_allegiance_update(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::entity::i_player::IPlayer;
    use crate::world_objects::player_allegiance::{i_player_allegiance, i_player_allegiance_node};

    if crate::world_objects::world_object_networking::shims::player_session(w, this).is_none() {
        return;
    }
    let allegiance = i_player_allegiance(w, IPlayer::Online(this));
    let node = i_player_allegiance_node(w, IPlayer::Online(this));
    crate::world_objects::allegiance::send_allegiance_update(w, this, allegiance, node);
}

/// Not ACE's (retail captures, V289): an unsubscribed player is still pushed
/// the changes to its view of the allegiance, off the heartbeat and never sooner than 30 s after
/// the previous AllegianceUpdate, and never with a done.
fn unsubscribed_allegiance_update(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let now = w.now.unix_time;
    let f = fields(w, this);
    if !f.allegiance_updates_off || now < f.allegiance_view_next_check {
        return;
    }
    let Some(sent) = f.allegiance_update_sent_time else {
        return;
    };
    if now - sent < UNSUBSCRIBED_ALLEGIANCE_UPDATE_SPACING {
        return;
    }
    fields_mut(w, this).allegiance_view_next_check =
        now + UNSUBSCRIBED_ALLEGIANCE_VIEW_CHECK_INTERVAL;
    if allegiance_view_changed(w, this) {
        send_current_allegiance_update(w, this);
    }
}

/// Not ACE's (retail captures, V289): records that the player was just sent
/// an AllegianceUpdate showing `view`, the baseline its later changes are measured from. Every
/// update counts: the login pair, the answers to requests, swearing and breaking, the member
/// refreshes and the changed ones.
pub fn note_allegiance_update_sent(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    view: Vec<u8>,
) {
    let now = w.now.unix_time;
    let Some(p) = w.objects.get_mut(this).and_then(|o| o.player.as_mut()) else {
        return;
    };
    p.player_tick.allegiance_view_sent = Some(view);
    p.player_tick.allegiance_update_sent_time = Some(now);
}

/// Not ACE's (retail captures, V281): records the client's allegiance-update
/// subscription (AllegianceUpdateRequest's `on_off`), which decides whether the player's ticks
/// carry AllegianceUpdateDone.
pub fn set_allegiance_updates(w: &mut crate::World, this: empyrean_entity::ObjectGuid, on: bool) {
    fields_mut(w, this).allegiance_updates_off = !on;
}

/// Not ACE's (retail captures, V280): the seconds a per-beat timer is credited
/// on this object's heartbeat. ACE credits its fixed `CachedHeartbeatInterval` (5 s); a player's
/// heartbeat comes every 4 to 6 s, so it credits the time since its previous heartbeat, which
/// keeps each timer on real time. A player's first heartbeat, and any other object, credits ACE's.
#[must_use]
pub fn heartbeat_credit(w: &crate::World, this: empyrean_entity::ObjectGuid) -> f64 {
    let o = w.objects.get(this).expect("ACE: this is null");
    o.player
        .as_ref()
        .and_then(|p| p.player_tick.heartbeat_elapsed)
        .unwrap_or(o.wo.world_object_tick.cached_heartbeat_interval)
}

/// Not ACE's (retail captures, V280): the factor on a player's health
/// regeneration step. Retail's health step scaled with the time since the last tick, the
/// per-5-s rate times elapsed / 5 (the slope of gain on interval equals the mean rate); stamina
/// and mana, and every other creature's health, keep ACE's fixed step (factor 1). Before a
/// player's first heartbeat the factor is 1, ACE's 5 s step.
#[must_use]
pub fn health_regen_scale(o: &crate::world_objects::world_object::WorldObject) -> f64 {
    o.player
        .as_ref()
        .and_then(|p| p.player_tick.heartbeat_elapsed)
        .map_or(1.0, |elapsed| elapsed / HEALTH_REGEN_RATE_SECONDS)
}

/// Not ACE's (V280): the seconds a health regeneration rate is quoted over (ACE's heartbeat).
pub const HEALTH_REGEN_RATE_SECONDS: f64 = 5.0;

/// Not ACE's (V255): an allegiance member is sent AllegianceUpdateDone right
/// after each `Age` update, as the retail server did (88-92% of the retail events follow an Age
/// update on the same tick); a player outside an allegiance is sent none. The client drops it.
pub fn send_allegiance_update_done(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::entity::i_player::IPlayer;
    use crate::world_objects::player_allegiance::i_player_allegiance;

    if i_player_allegiance(w, IPlayer::Online(this)).is_none() {
        return;
    }
    let Some(session) =
        crate::world_objects::world_object_networking::shims::player_session(w, this)
    else {
        return;
    };
    let done = crate::network::game_event::events::game_event_allegiance_allegiance_update_done::game_event_allegiance_allegiance_update_done(
        crate::network::game_event::game_event_message::session_data(w, session),
        empyrean_entity::enums::WeenieError::None,
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, done);
}

/// Not ACE's (V255): the retail server paired one AllegianceUpdateDone with
/// the allegiance update sent at login, and with no later update. The login update answers the
/// request the client makes as it builds its allegiance panel, so this is sent after the first
/// requested update of this login, once; later updates are unpaired. Not ACE's (retail captures,
/// V281): only a subscribing request (`on_off = 1`, as the login one is)
/// is paired, so a player that has turned its subscription off is sent no done.
pub fn send_login_allegiance_update_done(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    if fields(w, this).login_allegiance_update_done_sent || fields(w, this).allegiance_updates_off {
        return;
    }
    fields_mut(w, this).login_allegiance_update_done_sent = true;
    send_allegiance_update_done(w, this);
}

/// The head of `Player_Tick`: a player whose Character or Biota save failed is booted (unless it
/// is logging out) and the flag cleared; either way the tick ends there. Answers
/// whether it ended the tick.
fn save_failed_boot(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    use empyrean_net::enums::{CharacterError, SessionTerminationReason};

    let Some(p) = w.objects.get(this).and_then(|o| o.player.as_ref()) else {
        return false;
    };
    let (character_save_failed, biota_save_failed, is_logging_out) = (
        p.player_database.character_save_failed,
        p.player_database.biota_save_failed,
        p.player.is_logging_out,
    );

    let reason = if character_save_failed {
        // Boot the player as their Character object is not saving properly
        (
            SessionTerminationReason::CharacterSaveFailed,
            "CharacterSaveFailed",
        )
    } else if biota_save_failed {
        // Boot the player as their Biota object is not saving properly
        (SessionTerminationReason::BiotaSaveFailed, "BiotaSaveFailed")
    } else {
        return false;
    };

    if !is_logging_out {
        let o = w.objects.get(this).expect("present");
        let name = o
            .get_property(empyrean_entity::enums::PropertyString::Name)
            .unwrap_or_default();
        let account = o
            .player
            .as_ref()
            .and_then(|p| p.player.account.as_ref())
            .map(|a| a.account_name.clone())
            .unwrap_or_default();
        log::error!(
            "{name} | 0x{this} | Account: {account} - disconnected for {}",
            reason.1
        );
        //Session.SendCharacterError(CharacterError.AccountLogin); // forces client to error screen
        let session = crate::world_objects::world_object_networking::shims::player_session(w, this)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        let msg = crate::network::game_messages::messages::game_message_character_error::game_message_character_error(CharacterError::AccountLogin);
        let now = w.now;
        w.net.terminate(
            session,
            reason.0,
            Some(msg.into_outbound()),
            String::new(),
            now,
        );
        //Session.LogOffPlayer(true);
        let p = &mut w
            .objects
            .get_mut(this)
            .expect("present")
            .player
            .as_mut()
            .expect("a player")
            .player_database;
        if character_save_failed {
            p.character_save_failed = false;
        } else {
            p.biota_save_failed = false;
        }
    }
    true
}

/// How long a player may stay in portal space before the heartbeat logs it off (five minutes).
// ACE: Player.MaximumTeleportTime
pub const MAXIMUM_TELEPORT_TIME_MINUTES: f64 = 5.0;

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

/// Called every ~5 seconds for Players: the landblock, mana, target-vitals, lifestone, PK and gag
/// timers, the destruction queue of the player's known objects, the periodic save (every
/// `player_save_interval` seconds, 300 by default), the portal-space timeout, then Creature's
/// heartbeat.
///
/// Not ACE's (retail captures, V280): retail's player had one tick, every
/// uniform 4 to 6 s (mean 5), that carried the `Age` update, AllegianceUpdateDone, the vital
/// regeneration and the fellow vital updates. So this is that tick: it starts with the `Age`
/// update, sends the fellow vital update its regeneration raised before it ends, and schedules
/// the next one 4 to 6 s on (drawn afresh each time) instead of ACE's fixed 5 s. Each per-beat
/// timer is credited the seconds since the previous heartbeat ([`heartbeat_credit`]), and health
/// regenerates in proportion to them ([`health_regen_scale`]).
// ACE: Player.Heartbeat
pub fn player_heartbeat(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    current_unix_time: f64,
) {
    use empyrean_common::dotnet::datetime::DotNetDateTime;

    // Not ACE's (V280): the seconds this beat stands for, the time since the previous one (ACE's
    // interval on the first).
    {
        let cached = w
            .objects
            .get(this)
            .expect("ACE: this is null")
            .wo
            .world_object_tick
            .cached_heartbeat_interval;
        let tick = fields_mut(w, this);
        tick.heartbeat_elapsed = Some(
            tick.last_heartbeat_time
                .map_or(cached, |last| current_unix_time - last),
        );
        tick.last_heartbeat_time = Some(current_unix_time);
    }
    age_update(w, this);

    crate::world_objects::player_location::notify_landblocks(w, this);

    mana_consumers_tick(w, this);

    crate::world_objects::player_vitals::handle_target_vitals(w, this);

    crate::world_objects::player_death::lifestone_protection_tick(w, this);

    crate::world_objects::player_death::pk_death_tick(w, this);

    gags_tick(w, this);

    if let Some(h) = crate::physics::phys_ext::physics_obj(w, this) {
        crate::physics::object_maint::destroy_objects(w, h);
    }

    // Check if we're due for our periodic SavePlayer
    let utc_now = w.now.utc;
    let interval = crate::world_objects::player_database::player_save_interval_secs(w);
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let db = &mut o.wo.world_object_database;
    if db.last_requested_database_save == DotNetDateTime::MIN_VALUE {
        db.last_requested_database_save = utc_now;
    }

    #[allow(clippy::cast_precision_loss)] // `AddSeconds(long)` converts the long to double
    let due = db.last_requested_database_save.add_seconds(interval as f64) <= utc_now;
    if due {
        crate::world_objects::player_database::save_player_to_database(w, this);
    }

    // A player stuck in portal space for longer than the maximum teleport time is logged off
    // (immediately with a session, or through the ordinary log-out without one). "Now" is read
    // off the clock the start was stamped with (`Time.GetUnixTime()`, the same instant as
    // `DateTime.UtcNow`).
    let now = empyrean_common::time::Time::get_date_time_from_timestamp(w.now.unix_time);
    let stuck = w.objects.get(this).is_some_and(|o| {
        o.wo.world_object.teleporting
            && now
                > empyrean_common::time::Time::get_date_time_from_timestamp(
                    o.last_teleport_start_timestamp().unwrap_or(0.0),
                )
                .add_minutes(MAXIMUM_TELEPORT_TIME_MINUTES)
    });
    if stuck {
        match crate::world_objects::world_object_networking::shims::player_session(w, this) {
            Some(session) => crate::sessions::log_off_player(w, session, true),
            None => {
                crate::world_objects::player::log_out(w, this, false, false);
            }
        }
    }

    crate::world_objects::creature_tick::creature_heartbeat(w, this, current_unix_time);

    // Not ACE's (V280): the fellow vital update rides the tick whose regeneration raised it
    // (ACE's `Player_Tick` sends it on the landblock's next tick).
    if w.objects.get(this).is_none() {
        return;
    }
    fellow_vital_update_tick(w, this);

    // Not ACE's (V280): the next tick 4 to 6 s on, in place of the fixed interval the base
    // heartbeat scheduled.
    let next = current_unix_time + next_player_tick_interval();
    w.objects
        .get_mut(this)
        .expect("present")
        .wo
        .world_object_tick
        .next_heartbeat_time = next;
}

// ---- movement ---------------------------------------------------------------------------------

// ACE: Player.MaxSpeed
pub const MAX_SPEED: f32 = 50.0;
// ACE: Player.MaxSpeedSq
pub const MAX_SPEED_SQ: f32 = MAX_SPEED * MAX_SPEED;

// ACE: Player.DebugPlayerMoveToStatePhysics
/// A static ACE only sets from a debugging command; always false here.
pub const DEBUG_PLAYER_MOVE_TO_STATE_PHYSICS: bool = false;

// ACE: Player.FastTick
/// Flag indicates if player is doing full physics simulation (`IsPKType`).
#[must_use]
pub fn fast_tick(w: &World, this: ObjectGuid) -> bool {
    is_pk_type(w, this)
}

/// `Player.IsPKType` (`Player_Combat.cs`): a PK or a PK Lite.
fn is_pk_type(w: &World, this: ObjectGuid) -> bool {
    let status = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .player_killer_status();
    status == PlayerKillerStatus::PK || status == PlayerKillerStatus::PKLite
}

// ACE: Player.OnMoveToState
/// For advanced spellcasting / players glitching around during powersliding: the client's
/// self-player uses DoMotion/StopMotion, the server and other players on the client use
/// apply_raw_movement. Only a full-physics (`FastTick`) player simulates the MoveToState.
pub fn on_move_to_state(w: &mut World, this: ObjectGuid, move_to_state: &MoveToState) {
    if !fast_tick(w, this) {
        return;
    }

    if DEBUG_PLAYER_MOVE_TO_STATE_PHYSICS {
        log::debug!("{}", move_to_state.raw_motion_state.to_string_flags(true));
    }

    if record_cast_enabled(w, this) {
        crate::entity::record_cast::on_move_to_state(w, this, move_to_state);
    }

    let h = physics_obj(w, this);
    phys_ext::restart_clock_if_idle(w, h);

    if !crate::managers::property_manager::get_bool(w, "client_movement_formula", false, true).item
        || move_to_state.standing_long_jump
    {
        on_move_to_state_server_method(w, this, move_to_state);
    } else {
        on_move_to_state_client_method(w, this, move_to_state);
    }

    if shims::player_magic_state_is_casting(w, this)
        && magic_state_pending_turn_release(w, this)
        && move_to_state.raw_motion_state.turn_command == MotionCommand(0)
    {
        crate::world_objects::player_magic::on_turn_release(w, this);
    }
}

// ACE: Player.OnMoveToState_ClientMethod
/// Each axis pressed, changed or released since `LastMoveToState` becomes a DoMotion/StopMotion
/// with the client's hold key (3.6's physics half).
pub fn on_move_to_state_client_method(
    w: &mut World,
    this: ObjectGuid,
    move_to_state: &MoveToState,
) {
    let raw_state = to_physics_raw_state(&move_to_state.raw_motion_state);
    let prev_state = match player_move::fields(w, this).last_move_to_state.as_ref() {
        Some(m) => to_physics_raw_state(&m.raw_motion_state),
        None => to_physics_raw_state(&crate::network::motion::raw_motion_state::none()),
    };

    let h = physics_obj(w, this);
    phys_ext::apply_raw_motion_state_client_method(w, h, &raw_state, &prev_state);
}

// ACE: Player.OnMoveToState_ServerMethod
/// The client's raw state becomes the interpreter's (a standing long jump holds forward and
/// sidestep), then `apply_raw_movement(true, allowJump)` (3.6's physics half).
pub fn on_move_to_state_server_method(
    w: &mut World,
    this: ObjectGuid,
    move_to_state: &MoveToState,
) {
    let raw_state = to_physics_raw_state(&move_to_state.raw_motion_state);
    let h = physics_obj(w, this);
    phys_ext::apply_raw_motion_state(w, h, &raw_state, move_to_state.standing_long_jump);
}

/// The network `RawMotionState` as the physics interpreter's (`RawMotionState.SetState`'s input;
/// `SetState` applies the zero defaults itself).
fn to_physics_raw_state(s: &RawMotionState) -> dereth_animation::motion::RawMotionState {
    use dereth_animation::command::MotionCommand as C;
    use dereth_animation::motion::HoldKey as K;
    let key = |k: empyrean_entity::enums::HoldKey| match k.0 {
        1 => K::None,
        2 => K::Run,
        _ => K::Invalid,
    };
    dereth_animation::motion::RawMotionState {
        current_holdkey: key(s.current_hold_key),
        current_style: C(s.current_style.0),
        forward_command: C(s.forward_command.0),
        forward_holdkey: key(s.forward_hold_key),
        forward_speed: s.forward_speed,
        sidestep_command: C(s.sidestep_command.0),
        sidestep_holdkey: key(s.sidestep_hold_key),
        sidestep_speed: s.sidestep_speed,
        turn_command: C(s.turn_command.0),
        turn_holdkey: key(s.turn_hold_key),
        turn_speed: s.turn_speed,
        ..Default::default()
    }
}

/// The player's physics tick: the requested location first, then the physics update while the
/// body moves. Answers whether the player changed landblock. ACE's stopwatch and its performance
/// log lines are diagnostics and are not kept.
// ACE: Player.UpdateObjectPhysics
pub fn player_update_object_physics(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    let mut landblock_update = false;

    fields_mut(w, this).in_update = true;

    // update position through physics engine
    let requested = w
        .objects
        .get(this)
        .and_then(|o| o.wo.world_object.requested_location);
    if let Some(requested_location) = requested {
        landblock_update = update_player_position(w, this, requested_location, false);
        if let Some(o) = w.objects.get_mut(this) {
            o.wo.world_object.requested_location = None;
        }
    }

    if let Some(h) = phys_ext::physics_obj(w, this) {
        let v = phys_ext::velocity(w, h);
        #[allow(clippy::float_cmp)] // C#'s Vector3 !=
        if fast_tick(w, this) && phys_ext::is_moving_or_animating(w, h) || v != Vector3::ZERO {
            update_player_physics(w, this);

            let has_callback =
                crate::world_objects::player_move2::move_to_params_has_callback(w, this);
            if has_callback && !phys_ext::is_moving_or_animating(w, h) {
                crate::world_objects::player_move2::handle_move_to_callback(w, this);
            }
        }
    }

    fields_mut(w, this).in_update = false;

    landblock_update
}

/// Steps the player's body and syncs the location's rotation; a full-physics player also
/// resyncs a PK logout and the magic turn.
// ACE: Player.UpdatePlayerPhysics
pub fn update_player_physics(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let h = physics_obj(w, this);

    if DEBUG_PLAYER_MOVE_TO_STATE_PHYSICS {
        log::debug!("{this:?}.UpdatePlayerPhysics()");
    }

    phys_ext::update_object(w, h);

    // sync ace position?
    let q = phys_ext::position(w, h).expect("a body").frame.rotation;
    if let Some(o) = w.objects.get_mut(this) {
        let mut location = o.location().expect("ACE: Location is null");
        location.set_rotation(Quaternion {
            x: q.x,
            y: q.y,
            z: q.z,
            w: q.w,
        });
        o.set_location(Some(location));
    }

    if !fast_tick(w, this) {
        return;
    }

    // ensure PKLogout position is synced up for other players
    if crate::world_objects::player::fields(w, this).pk_logout {
        let motion = Motion::new(MotionStance::NonCombat, MotionCommand::Ready, 1.0);
        let movement_data = MovementData::from_motion(this, &motion);
        let numbering = w.dats.portal_dat().command_numbering();
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        let msg =
            game_message_update_motion::game_message_update_motion(o, &movement_data, numbering);
        world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
        phys_ext::stop_completely(w, h, true);

        if !phys_ext::is_moving_or_animating(w, h) {
            crate::world_objects::world_object::sync_location(w, this);
            let msg = game_message_update_position::game_message_update_position(w, this, false);
            world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
        }
    }

    // ACE comments out re-applying the latest MoveToState here (the 'client blip forward' bug is
    // preferred to an invisible run on the server).

    if shims::player_magic_state_is_casting(w, this) && magic_state_pending_turn_release(w, this) {
        crate::world_objects::player_magic::check_turn(w, this);
    }
}

// ACE: Player.MoveToState_UpdatePosition_Threshold
/// The maximum rate UpdatePosition packets from MoveToState will be broadcast for each player
/// (AutonomousPosition still always broadcasts UpdatePosition): 1 second, estimated from retail.
#[must_use]
pub fn move_to_state_update_position_threshold() -> TimeSpan {
    TimeSpan::from_seconds(1.0)
}

/// ACE's `PhysicsGlobals.EpsilonSq`.
const EPSILON_SQ: f32 = 0.0002 * 0.0002;

// ACE: Player.UpdatePlayerPosition
/// Used by physics engine to actually update a player position; automatically notifies clients
/// of the updated position. Returns true if the object moves to a different landblock. The
/// stopwatch and its performance log are diagnostics and are not kept.
#[allow(clippy::too_many_lines)] // ACE's one method
pub fn update_player_position(
    w: &mut World,
    this: ObjectGuid,
    new_position: Position,
    force_update: bool,
) -> bool {
    let mut verify_contact = false;

    // possible bug: while teleporting, client can still send AutoPos packets from old landblock
    if teleporting(w, this) && !force_update {
        return false;
    }

    // pre-validate movement
    if !validate_movement(w, this, &new_position) {
        log::error!(
            "{this:?}.UpdatePlayerPosition() - movement pre-validation failed from {} to {}",
            location(w, this).to_loc_string(),
            new_position.to_loc_string()
        );
        return false;
    }

    let mut success = true;

    if let Some(h) = phys_ext::physics_obj(w, this) {
        let location = location(w, this);
        let dist_sq = location.squared_distance_to(&new_position);

        if dist_sq > EPSILON_SQ {
            if new_position.landblock() == 0x18A && location.landblock() != 0x18A {
                log::info!("{this:?} is getting swanky");
            }

            if !teleporting(w, this) {
                let block_dist = phys_ext::get_block_dist(location.cell(), new_position.cell());

                // verify movement
                if dist_sq > MAX_SPEED_SQ && block_dist > 1 {
                    //Session.Network.EnqueueSend(new GameMessageSystemChat("Movement error", ChatMessageType.Broadcast));
                    log::warn!(
                        "MOVEMENT SPEED: {this:?} trying to move from {} to {}, speed: {}",
                        location.to_loc_string(),
                        new_position.to_loc_string(),
                        f64::from(dist_sq).sqrt()
                    );
                    return false;
                }

                // verify z-pos
                let move_fields = crate::world_objects::player::fields(w, this);
                let last_ground_pos = move_fields.last_ground_pos;
                let since_jump = w.now.utc - move_fields.last_jump_time;
                if block_dist == 0
                    && last_ground_pos
                        .is_some_and(|g| new_position.position_z - g.position_z > 10.0)
                    && since_jump > TimeSpan::from_seconds(1.0)
                    && jump_skill_current(w, this) < 1000
                {
                    verify_contact = true;
                }
            }

            if let Some(cur_cell) = phys_ext::get_landcell_loading(w, new_position.cell()) {
                //if (PhysicsObj.CurCell == null || curCell.ID != PhysicsObj.CurCell.ID)
                //PhysicsObj.change_cell_server(curCell);

                set_request_pos(
                    w,
                    this,
                    h,
                    new_position.pos(),
                    new_position.rotation(),
                    Some(cur_cell),
                    location.landblock_id().raw(),
                );
                success = if fast_tick(w, this) {
                    update_object_server_new(w, this, h, true)
                } else {
                    update_object_server(w, this, h, true)
                };

                if phys_ext::cur_cell(w, h).is_none() && cur_cell.0 >> 16 != 0x18A {
                    // `PhysicsObj.CurCell = curCell`
                    if let Some(e) = phys_ext::ext_mut(w, h) {
                        e.cur_cell = Some(cur_cell);
                    }
                }

                if verify_contact && crate::world_objects::player::is_jumping(w, this) {
                    let last_ground_pos = crate::world_objects::player::fields(w, this)
                        .last_ground_pos
                        .expect("checked above");
                    let block_dist =
                        phys_ext::get_block_dist(new_position.cell(), last_ground_pos.cell());

                    if block_dist <= 1 {
                        log::warn!(
                            "z-pos hacking detected for {this:?}, lastGroundPos: {} - requestPos: {}",
                            last_ground_pos.to_loc_string(),
                            new_position.to_loc_string()
                        );
                        let o = w.objects.get_mut(this).expect("ACE: this is null");
                        o.set_location(Some(Position::from_position(&last_ground_pos)));
                        o.sequences
                            .get_next_sequence(SequenceType::ObjectForcePosition);
                        world_object_networking::send_update_position(w, this, false);
                        return false;
                    }
                }

                check_monsters(w, this);
            }
        } else if let Some(o) = w.physics.get_mut(h) {
            // `PhysicsObj.Position.Frame.Orientation = newPosition.Rotation`
            let r = new_position.rotation();
            let mut frame = o.position.frame;
            frame.rotation = Quat::new(r.w, r.x, r.y, r.z);
            o.set_frame(frame);
        }
    }

    // double update path: landblock physics update -> updateplayerphysics() -> update_object_server() -> Teleport() -> updateplayerphysics() -> return to end of original branch
    if teleporting(w, this) && !force_update {
        return true;
    }

    if !success {
        return false;
    }

    let landblock_update = location(w, this).cell() >> 16 != new_position.cell() >> 16;

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_location(Some(new_position));

    if record_cast_enabled(w, this) {
        let loc = location(w, this).to_loc_string();
        crate::entity::record_cast::log(w, this, &format!("CurPos: {loc}"));
    }

    let o = w.objects.get(this).expect("ACE: this is null");
    let broadcast = o.wo.world_object.requested_location_broadcast;
    let since_update = w.now.utc - o.wo.world_object_networking.last_update_position;
    if broadcast || since_update >= move_to_state_update_position_threshold() {
        world_object_networking::send_update_position(w, this, false);
    } else {
        let msg = game_message_update_position::game_message_update_position(w, this, false);
        let session = shims::player_session(w, this)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        game_message::enqueue_send(w, session, msg);
    }

    if !fields(w, this).in_update {
        crate::managers::landblock_manager::relocate_object_for_physics(w, this, true);
    }

    landblock_update
}

// ACE: Player.buggedCells
const BUGGED_CELLS: [u32; 2] = [0xD699_0112, 0xD599_012C];

// ACE: Player.ValidateMovement
/// A move is refused with no current landblock; outside a teleport, a change of landblock is
/// refused between two interior cells (unless both are the known bugged cells) and from a dungeon
/// into a loaded dungeon.
pub fn validate_movement(w: &mut World, this: ObjectGuid, new_position: &Position) -> bool {
    let Some(current_landblock) = w.objects.get(this).and_then(|o| o.current_landblock) else {
        return false;
    };

    let location = location(w, this);
    if !teleporting(w, this) && location.landblock() != new_position.cell() >> 16 {
        if (location.cell() & 0xFFFF) >= 0x100
            && (new_position.cell() & 0xFFFF) >= 0x100
            && (!BUGGED_CELLS.contains(&location.cell())
                || !BUGGED_CELLS.contains(&new_position.cell()))
        {
            return false;
        }

        let is_dungeon = w
            .landblock_manager
            .landblocks
            .get_mut(current_landblock)
            .is_some_and(crate::entity::landblock::Landblock::is_dungeon);
        if is_dungeon {
            // `LScape.get_landblock(newPosition.Cell)`: the physics landblock, only if it is loaded.
            #[allow(clippy::cast_possible_truncation)] // the landblock half of the cell id
            let dest_block = (new_position.cell() >> 16) as u16;
            let dest_loaded = w.phys_ext.landblocks.contains_key(&dest_block);
            let dest = empyrean_entity::LandblockId::new(new_position.cell());
            if dest_loaded
                && w.landblock_manager
                    .landblocks
                    .get_mut(dest)
                    .is_some_and(crate::entity::landblock::Landblock::is_dungeon)
            {
                return false;
            }
        }
    }
    true
}

// ACE: Player.SyncLocationWithPhysics
/// Copies the body's position into `Location`; answers whether the landblock changed.
// ACE-BUG: `blockcell << 16 != CurrentLandblock.Id.Landblock` shifts the wrong way, so the answer
// is true for every cell but 0 (ACE has no caller).
pub fn sync_location_with_physics(w: &mut World, this: ObjectGuid) -> bool {
    let h = physics_obj(w, this);
    if phys_ext::cur_cell(w, h).is_none() {
        log::info!("{this:?}.SyncLocationWithPhysics(): CurCell is null!");
        return false;
    }

    let p = phys_ext::position(w, h).expect("a body");
    let blockcell = p.cell.0;

    let current_landblock = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: CurrentLandblock is null");
    let landblock_update = blockcell.wrapping_shl(16) != u32::from(current_landblock.landblock());

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_location(Some(physics_position_to_location(&p)));

    landblock_update
}

// ACE: Player.EnqueueAction
/// Prepare new action to run on this player: the player's own queue, run by `Player_Tick`.
pub fn player_enqueue_action(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    action: crate::entity::actions::i_action::Action,
) {
    fields_mut(w, this).action_queue.enqueue_action(action);
}

/// `Player.actionQueue`, for [`run_actions`](crate::entity::actions::action_queue::run_actions);
/// `None` when `this` is not a player in the store.
pub fn action_queue_mut(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> Option<&mut crate::entity::actions::action_queue::ActionQueue> {
    w.objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .map(|p| &mut p.player_tick.action_queue)
}

/// A motion's AnimationDone for a FastTick player: the fast-chug consumable sequence and the
/// FastTick cast sequence advance on it.
// ACE: Player.HandleMotionDone
pub fn player_handle_motion_done(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    motion_id: u32,
    success: bool,
) {
    //Console.WriteLine($"{Name}.HandleMotionDone({(MotionCommand)motionID}, {success})");

    if !fast_tick(w, this) {
        return;
    }

    if crate::world_objects::player_use::fields(w, this)
        .food_state
        .is_chugging
    {
        crate::world_objects::player_use::handle_motion_done_use_consumable(
            w, this, motion_id, success,
        );
    }

    if crate::world_objects::player_magic::fields(w, this)
        .magic_state
        .is_casting
    {
        crate::world_objects::player_magic::handle_motion_done_magic(w, this, motion_id, success);
    }
}

// ---- small readers ----------------------------------------------------------------------------

fn physics_obj(w: &World, this: ObjectGuid) -> PhysHandle {
    phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)")
}

fn location(w: &World, this: ObjectGuid) -> Position {
    w.objects
        .get(this)
        .and_then(crate::world_objects::world_object::WorldObject::location)
        .expect("ACE: Location is null")
}

fn teleporting(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .is_some_and(|o| o.wo.world_object.teleporting)
}

/// `GetCreatureSkill(Skill.Jump).Current`.
fn jump_skill_current(w: &mut World, this: ObjectGuid) -> u32 {
    let skill = w
        .objects
        .get_mut(this)
        .expect("ACE: this is null")
        .get_creature_skill(Skill::Jump, true);
    skill.map_or(0, |s| s.current(w, this))
}

/// `PhysicsObj.Position` as an ACE `Position`.
#[must_use]
pub fn physics_position_to_location(p: &PPosition) -> Position {
    let q = p.frame.rotation;
    Position::from_vectors(
        p.cell.0,
        Vector3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
        Quaternion {
            x: q.x,
            y: q.y,
            z: q.z,
            w: q.w,
        },
    )
}

// ---- pointers to members ported in other files --------------------------------------------------

/// `RecordCast.Enabled` (`Entity/RecordCast.cs`, the cast recorder): off until an admin command
/// enables it, which is not ported; the recorder's calls are `not_ported!` in their branches.
fn record_cast_enabled(_w: &World, _this: ObjectGuid) -> bool {
    false
}

/// `MagicState.PendingTurnRelease` (`Entity/MagicState.cs`).
fn magic_state_pending_turn_release(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_magic::fields(w, this)
        .magic_state
        .pending_turn_release
}

/// `Player.CheckMonsters` (`Player_Monster.cs`): wakes nearby monsters.
pub(crate) fn check_monsters(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player_monster::check_monsters(w, this);
}

// ---- ACE's server-side PhysicsObj members ------------------------------------------------------
//
// ACE's physics port adds a legacy (`update_object_server`) and a full (`update_object_server_new`)
// server update for a player's requested position. Both end by putting the body exactly at the
// requested position (`set_current_pos`); the legacy one commits its transition first
// (`SetPositionInternal`: frame, cell, contact plane and transient state, collisions, shadows),
// through the shared crate's public commit (`phys_ext::set_position_internal`).

/// ACE's `PhysicsGlobals.HugeQuantum` and `MinQuantum`.
const HUGE_QUANTUM: f64 = 2.0;
const MIN_QUANTUM: f64 = 1.0 / 30.0;

fn to_ppos(cell: u32, pos: Vector3, rotation: Quaternion) -> PPosition {
    PPosition::new(
        CellId(cell),
        Frame::new(data_vec3(pos), data_quat(rotation)),
    )
}

// ACE: PhysicsObj.set_request_pos
/// Records the requested position (its cell: `cell`, else the landcell under it) and the cached
/// velocity. A body with no cell takes the landcell of `block_cell_id`, or gives up.
pub fn set_request_pos(
    w: &mut World,
    this: ObjectGuid,
    h: PhysHandle,
    pos: Vector3,
    rotation: Quaternion,
    cell: Option<CellId>,
    block_cell_id: u32,
) {
    if phys_ext::cur_cell(w, h).is_none() {
        let Some(c) = phys_ext::get_landcell(w, block_cell_id) else {
            return;
        };
        if let Some(e) = phys_ext::ext_mut(w, h) {
            e.cur_cell = Some(c);
        }
    }

    let cell_id = match cell {
        // `RequestPos.GetCell(CurCell.ID)`: the outdoor cell under the origin
        None => {
            let cur = phys_ext::cur_cell(w, h).expect("set above").0;
            let mut p = Position::from_vectors(cur, pos, rotation);
            p.set_land_cell();
            p.cell()
        }
        Some(c) => c.0,
    };

    let cached = w.physics.get(h).map_or(Vec3::ZERO, |o| o.cached_velocity);
    let f = fields_mut(w, this);
    f.request_pos = Some(to_ppos(cell_id, pos, rotation));
    f.request_cached_velocity = cached;
}

// ACE: PhysicsObj.update_object_server
/// The legacy movement system: moves the body to the requested position (a teleport places it
/// there). Answers false when the move failed.
pub fn update_object_server(
    w: &mut World,
    this: ObjectGuid,
    h: PhysHandle,
    force_pos: bool,
) -> bool {
    // the house barriers' answers for this mover (the transitions below run the entry check)
    phys_ext::prepare_mover(w, h);
    let now = phys_ext::physics_timer_current_time(w);
    let update_time = w.physics.get(h).map_or(now, |o| o.update_time);
    let delta_time = now - update_time;

    let is_teleport = teleporting(w, this);
    let mut success = true;
    if !is_teleport {
        success = update_object_internal_server(w, this, h, delta_time);
    }

    let request_pos = fields(w, this).request_pos.expect("ACE: RequestPos");
    if force_pos && success {
        set_current_pos(w, this, h, &request_pos);
    }

    // temp for players
    if let Some(o) = w.physics.get_mut(h) {
        if o.transient_state.in_contact() {
            o.cached_velocity = Vec3::ZERO;
        }
    }

    if is_teleport {
        // `SetPosition(SendPositionEvent | Slide | Placement | Teleport)`
        phys_ext::set_position(w, h, &request_pos);

        // hack...
        if let Some(o) = w.physics.get_mut(h) {
            if !o.transient_state.on_walkable() {
                o.velocity_vector = Vec3::new(0.0, 0.0, -0.0002);
            }
        }
    }

    if let Some(o) = w.physics.get_mut(h) {
        o.update_time = now;
    }

    success
}

// ACE: PhysicsObj.update_object_server_new
/// The full / updated movement system (a `FastTick` player): steps the body up to now, then
/// moves it to the requested position.
pub fn update_object_server_new(
    w: &mut World,
    this: ObjectGuid,
    h: PhysHandle,
    force_pos: bool,
) -> bool {
    // the house barriers' answers for this mover (the steps and transition below run the entry
    // check)
    phys_ext::prepare_mover(w, h);
    let detached = w
        .physics
        .get(h)
        .is_none_or(|o| o.parent.is_some() || o.state.is_frozen())
        || phys_ext::cur_cell(w, h).is_none();
    if detached {
        if let Some(o) = w.physics.get_mut(h) {
            o.transient_state.set_active_bit(false);
        }
        return false;
    }

    let now = phys_ext::physics_timer_current_time(w);
    let update_time = w.physics.get(h).map_or(now, |o| o.update_time);
    let delta_time = now - update_time;

    let is_teleport = teleporting(w, this);

    // commented out for debugging
    if delta_time > HUGE_QUANTUM && !is_teleport {
        if let Some(o) = w.physics.get_mut(h) {
            o.update_time = now; // consume time?
        }
        return false;
    }

    let mut request_pos = fields(w, this).request_pos.expect("ACE: RequestPos");
    let request_cell = request_pos.cell.0;

    let mut success = true;

    if !is_teleport {
        let position = phys_ext::position(w, h).expect("a body");
        if phys_ext::get_block_dist(position.cell.0, request_pos.cell.0) > 1 {
            log::warn!(
                "WARNING: failed transition for {this:?} from {position:?} to {request_pos:?}"
            );
            success = false;
        }

        // the MaxQuantum sub-steps up to now (the shared crate's quanta, V1)
        if delta_time > MIN_QUANTUM {
            phys_ext::update_object(w, h);
        }

        success &= request_cell >> 16 != 0x18A
            || phys_ext::cur_cell(w, h).is_some_and(|c| c.0 >> 16 == request_cell >> 16);
    }

    request_pos.cell = CellId(request_cell);

    if force_pos && success {
        // attempt transition to request pos, to trigger any collision detection
        let position = phys_ext::position(w, h).expect("a body");
        if let Some(transit) = w.physics.transition(h, &position, &request_pos, false) {
            track_collisions(w, h, &transit);
        }

        set_current_pos(w, this, h, &request_pos);
    }

    // for teleport, use SetPosition?
    if is_teleport {
        phys_ext::set_position(w, h, &request_pos);
    }

    if let Some(o) = w.physics.get_mut(h) {
        o.update_time = now;
    }

    success
}

// ACE: PhysicsObj.UpdateObjectInternalServer
/// The legacy movement system's step: a transition from the body's position to the requested
/// one (more than one landblock away fails), its cached velocity and collisions.
fn update_object_internal_server(
    w: &mut World,
    this: ObjectGuid,
    h: PhysHandle,
    quantum: f64,
) -> bool {
    let position = phys_ext::position(w, h).expect("a body");
    let request_pos = fields(w, this).request_pos.expect("ACE: RequestPos");
    if phys_ext::get_block_dist(position.cell.0, request_pos.cell.0) > 1 {
        log::warn!("WARNING: failed transition for {this:?} from {position:?} to {request_pos:?}");
        return false;
    }

    let request_cell = request_pos.cell.0;

    let transit = w.physics.transition(h, &position, &request_pos, false);
    if let Some(transit) = transit {
        let offset = dereth_physics::math::get_offset(&position, &transit.sphere_path.curr_pos);
        #[allow(clippy::cast_possible_truncation)] // `(float)quantum`
        let q = quantum as f32;
        if let Some(o) = w.physics.get_mut(h) {
            o.cached_velocity = Vec3::new(offset.x / q, offset.y / q, offset.z / q);
        }
        phys_ext::set_position_internal(w, h, &transit);
    } else {
        log::debug!("{this:?}.UpdateObjectInternalServer({quantum}) - failed transition from {position:?} to {request_pos:?}");
    }

    // DetectionManager, TargetManager, MovementManager, PartArray, PositionManager, particles and
    // scripts: the shared crate runs these in its own update.

    request_cell >> 16 != 0x18A
        || phys_ext::cur_cell(w, h).is_some_and(|c| c.0 >> 16 == request_cell >> 16)
}

/// `track_object_collision` for each object the transition touched, and the mover's collision
/// report: recorded in the collision table and reported to the mover's `WeenieObject` (as
/// `phys_ext` routes a placement's notices, V6).
fn track_collisions(w: &mut World, h: PhysHandle, transit: &Transition) {
    for (other, _) in &transit.collision_info.collide_object {
        let Some(oo) = w.physics.get(*other) else {
            continue;
        };
        let reports = w
            .physics
            .get(h)
            .is_some_and(|o| o.state.reports_collisions());
        let mover_wo = phys_ext::weenie_obj(w, h);

        // `if (obj.State.HasFlag(PhysicsState.Static)) return report_environment_collision(..)`
        // (and `report_object_collision`'s ReportCollisionsAsEnvironment arm)
        if oo.state.is_static() || oo.state.reports_as_environment() {
            if reports {
                mover_wo.do_collision_environment(w);
            }
            continue;
        }

        let other_id = oo.id.0;
        let ethereal = oo.state.is_ethereal();
        let ignores = oo.state.ignores_collisions();
        let now = phys_ext::physics_timer_current_time(w);
        if let Some(e) = phys_ext::ext_mut(w, h) {
            e.collision_table.insert(
                other_id,
                phys_ext::CollisionRecord {
                    touched_time: now,
                    ethereal,
                },
            );
        }

        // report_object_collision: the mover's report (the other object's report is addressed
        // to itself and dropped as a self-collision, as `phys_ext` notes)
        if !ignores && reports {
            let target_wo = phys_ext::weenie_obj(w, *other);
            mover_wo.do_collision_object(w, &target_wo);
        }
    }
}

// ACE: PhysicsObj.set_current_pos
/// Puts the body exactly at `new_pos`; on a cell change, a player in contact takes the terrain
/// cell as its contact plane cell, and the server half of the cell change runs.
fn set_current_pos(w: &mut World, this: ObjectGuid, h: PhysHandle, new_pos: &PPosition) {
    let body_cell = w.physics.get(h).and_then(|o| o.cell);
    if let Some(o) = w.physics.get_mut(h) {
        o.position.cell = new_pos.cell;
        o.set_frame(new_pos.frame);
    }

    if body_cell != Some(new_pos.cell) {
        if let Some(new_cell) = phys_ext::get_landcell_loading(w, new_pos.cell.0) {
            if crate::world_objects::player::fields(w, this).last_contact
                && !phys_ext::is_env_cell(new_cell)
            {
                // `landCell.find_terrain_poly(origin, ref walkable)`: the terrain under it
                if let Some(o) = w.physics.get_mut(h) {
                    o.contact_plane_cell_id = new_pos.cell;
                }
            }
            w.physics.leave_cell(h);
            w.physics.enter_cell(h, new_cell);
            phys_ext::sync_cell(w, h);
        }
    }

    let cached = fields(w, this).request_cached_velocity;
    if let Some(o) = w.physics.get_mut(h) {
        o.cached_velocity = cached;
    }
}

// ACE: Player.GagsTick
/// The gag notice once, then the gag counts down by the heartbeat interval and is lifted (saved,
/// with the ungag notice) when it runs out.
pub fn gags_tick(w: &mut World, this: ObjectGuid) {
    let o = w.objects.get(this).expect("ACE: this is null");
    if !o.is_gagged() {
        return;
    }

    if !fields(w, this).gag_notice_sent {
        crate::world_objects::player::send_gag_notice(w, this);
        fields_mut(w, this).gag_notice_sent = true;
    }

    // check for gag expiration, if expired, remove gag.
    // Not ACE's (retail captures, V280): the seconds since the previous
    // heartbeat (4 to 6 s), not ACE's fixed interval.
    let interval = heartbeat_credit(w, this);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.set_gag_duration(o.gag_duration() - interval);

    if o.gag_duration() <= 0.0 {
        o.set_is_gagged(false);
        o.set_gag_timestamp(0.0);
        o.set_gag_duration(0.0);
        crate::dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
        crate::world_objects::player::send_ungag_notice(w, this);
        fields_mut(w, this).gag_notice_sent = false;
    }
}

// ACE: Player.ManaConsumersTick
/// Called every ~5 secs for equipped mana consuming items.
pub fn mana_consumers_tick(w: &mut World, this: ObjectGuid) {
    use empyrean_common::dotnet::CsCast;
    let o = w.objects.get(this).expect("ACE: this is null");
    if !o
        .creature
        .as_ref()
        .is_some_and(|c| c.creature_equipment.equipped_objects_loaded)
    {
        return;
    }

    for item in crate::world_objects::creature_equipment::equipped_objects_values(w, this) {
        let i = w.objects.get(item).expect("an equipped object is live");
        if !i.is_affecting() {
            continue;
        }

        let (Some(_), Some(_), Some(mana_rate)) =
            (i.item_cur_mana(), i.item_max_mana(), i.mana_rate())
        else {
            continue;
        };

        let mut burn_rate = -mana_rate;

        let lum_aug_item_mana_usage = w
            .objects
            .get(this)
            .expect("ACE: this is null")
            .lum_aug_item_mana_usage();
        if lum_aug_item_mana_usage != 0 {
            burn_rate *= f64::from(
                crate::world_objects::creature_rating::get_negative_rating_mod(
                    lum_aug_item_mana_usage.wrapping_mul(5),
                    false,
                ),
            );
        }

        // Not ACE's (retail captures, V280): the burn over the seconds since
        // the previous heartbeat (4 to 6 s), not ACE's fixed interval.
        let interval = heartbeat_credit(w, this);
        let i = w.objects.get_mut(item).expect("an equipped object is live");
        let add: f32 = (burn_rate * interval).cs_cast();
        i.wo.world_object_magic.item_mana_rate_accumulator += add;

        if i.wo.world_object_magic.item_mana_rate_accumulator < 1.0 {
            continue;
        }

        let mut mana_to_burn: i32 = f64::from(i.wo.world_object_magic.item_mana_rate_accumulator)
            .floor()
            .cs_cast();

        let cur = i.item_cur_mana().expect("checked above");
        if mana_to_burn > cur {
            mana_to_burn = cur;
        }

        i.set_item_cur_mana(Some(cur.wrapping_sub(mana_to_burn)));

        let burned: f32 = mana_to_burn.cs_cast();
        i.wo.world_object_magic.item_mana_rate_accumulator -= burned;

        if i.item_cur_mana().is_some_and(|m| m > 0) {
            check_low_mana(w, this, item, burn_rate);
        } else {
            handle_mana_depleted(w, this, item);
        }
    }
}

// ACE: Player.CheckLowMana
/// Warns once when an item has two minutes of mana or less left; true while it is low.
fn check_low_mana(w: &mut World, this: ObjectGuid, item: ObjectGuid, burn_rate: f64) -> bool {
    const LOW_MANA_WARNING_SECONDS: i32 = 120;

    let i = w.objects.get_mut(item).expect("an equipped object is live");
    let seconds_until_empty = i.item_cur_mana().map(|m| f64::from(m) / burn_rate);

    // `secondsUntilEmpty > lowManaWarningSeconds` (a null compares false)
    if seconds_until_empty.is_some_and(|s| s > f64::from(LOW_MANA_WARNING_SECONDS)) {
        i.wo.world_object_magic.item_mana_depletion_message = false;
        return false;
    }
    if !i.wo.world_object_magic.item_mana_depletion_message {
        let name = crate::dispatch::name::name(w, item).unwrap_or_default();
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!("Your {name} is low on Mana."),
            empyrean_entity::enums::ChatMessageType::Magic,
        );
        let session = crate::managers::player_manager::player_session(w, this)
            .expect("ACE: Session is null (NullReferenceException)");
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        w.objects
            .get_mut(item)
            .expect("an equipped object is live")
            .wo
            .world_object_magic
            .item_mana_depletion_message = true;
    }
    true
}

// ACE: Player.HandleManaDepleted
/// "Your {item} is out of Mana." and the depletion sound; two seconds later the item's spells
/// are removed; the item stops affecting its wielder at once.
fn handle_mana_depleted(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    let name = crate::dispatch::name::name(w, item).unwrap_or_default();
    let msg =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!("Your {name} is out of Mana."),
            empyrean_entity::enums::ChatMessageType::Magic,
        );
    let sound = crate::network::game_messages::messages::game_message_sound::game_message_sound(
        this,
        empyrean_entity::enums::Sound::ItemManaDepleted,
        1.0,
    );
    let session = crate::managers::player_manager::player_session(w, this)
        .expect("ACE: Session is null (NullReferenceException)");
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
    crate::network::game_messages::game_message::enqueue_send(w, session, sound);

    // unsure if these messages / sounds were ever sent in retail,
    // or if it just purged the enchantments invisibly
    // doing a delay here to prevent 'SpellExpired' sounds from overlapping with 'ItemManaDepleted'
    let mut action_chain = crate::entity::actions::action_chain::ActionChain::new();
    action_chain.add_delay_seconds(w, 2.0);
    action_chain.add_action(
        crate::entity::actions::i_actor::Actor::Object(this),
        move |w: &mut World| {
            let spell_ids: Vec<i32> = w
                .objects
                .get(item)
                .and_then(|o| {
                    o.biota
                        .properties_spell_book
                        .as_ref()
                        .map(|b| b.keys().copied().collect())
                })
                .unwrap_or_default();
            for spell_id in spell_ids {
                crate::world_objects::creature_magic::remove_item_spell(
                    w,
                    this,
                    w.objects.contains(item).then_some(item),
                    spell_id.cast_unsigned(),
                    false,
                );
            }
        },
    );
    action_chain.enqueue_chain(w);

    crate::world_objects::world_object_magic::on_spells_deactivated(w, item);
}
