// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Location.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Location.cs`.
//!
//! Teleporting (`Teleport`, its physics state changes and `OnTeleportComplete`), the recalls
//! (lifestone, marketplace, house, allegiance hometown and mansion, the PK arenas), the no-log
//! landblocks and `GetRotateDelay`. Calls into systems ported in other files (houses,
//! allegiances, combat mode, visibility, lifestone protection) go through pointer functions at the
//! end of the file, in ACE's order.

use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_dat::file_types::MotionTable;
use empyrean_entity::enums::{
    ChatMessageType, CloakStatus, CombatMode, HeritageGroup, MotionCommand, MotionStance,
    PhysicsState, PlayerKillerStatus, PositionType, PropertyBool, PropertyInt, WeenieError,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::spell::Spell;
use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::{
    game_message_player_teleport, game_message_private_update_property_int,
    game_message_system_chat,
};
use crate::network::motion::movement_data::Motion;
use crate::physics::{motion_table, phys_ext};
use crate::world_objects::world_object::{self, WorldObject, LOCAL_BROADCAST_RANGE};
use crate::world_objects::world_object_networking::{self, shims};
use crate::world_objects::{player_networking, player_tick};
use crate::World;

/// Non-property fields declared in `Player_Location.cs`.
#[derive(Debug)]
pub struct PlayerLocationFields {
    // ACE: Player.LastTeleportTime
    pub last_teleport_time: DotNetDateTime,
    // ACE: Player.LastPortalTeleportTimestampError
    /// Prevent message spam
    pub last_portal_teleport_timestamp_error: Option<f64>,
}

impl Default for PlayerLocationFields {
    fn default() -> Self {
        PlayerLocationFields {
            last_teleport_time: DotNetDateTime::MIN_VALUE,
            last_portal_teleport_timestamp_error: None,
        }
    }
}

// ACE: Player.NoLog_Landblocks
/// The landblocks a player may not log in on: they are moved to their lifestone first.
// https://asheron.fandom.com/wiki/Special:Search?query=Lifestone+on+Relog%3A+Yes+
// https://docs.google.com/spreadsheets/d/122xOw3IKCezaTDjC_hggWSVzYJ_9M_zUUtGEXkwNXfs/edit#gid=846612575
pub const NO_LOG_LANDBLOCKS: [u16; 37] = [
    0x0002, // Viamontian Garrison
    0x0007, // Town Network
    0x0056, // Augmentation Realm Main Level
    0x005F, // Tanada House of Pancakes (Seasonal)
    0x0067, // PKL Arena
    0x006D, // Augmentation Realm Upper Level
    0x007D, // Augmentation Realm Lower Level
    0x00AB, // Derethian Combat Arena
    0x00AC, // Derethian Combat Arena
    0x00C3, // Blighted Putrid Moarsman Tunnels
    0x00D7, // Jester's Prison
    0x00EA, // Mhoire Armory
    0x015D, // Mountain Cavern
    0x027F, // East Fork Dam Hive
    0x03A7, // Mount Elyrii Hive
    0x5764, // Oubliette of Mhoire Castle
    0x634C, // Tainted Grotto
    0x6544, // Greater Battle Dungeon
    0x6651, // Hoshino Tower
    0x7E04, // Thug Hideout
    0x8A04, // Night Club (Seasonal Anniversary)
    0x8B04, // Frozen Wight Lair
    0x9EE5, // Northwatch Castle Black Market
    0xB5F0, // Aerfalle's Sanctum
    0xF92F, // Freebooter Keep Black Market
    0x00B0, // Colosseum Arena One
    0x00B1, // Colosseum Arena Two
    0x00B2, // Colosseum Arena Three
    0x00B3, // Colosseum Arena Four
    0x00B4, // Colosseum Arena Five
    0x00B6, // Colosseum Arena Mini-Bosses
    0x5960, // Gauntlet Arena One (Celestial Hand)
    0x5961, // Gauntlet Arena Two (Celestial Hand)
    0x5962, // Gauntlet Arena One (Eldritch Web)
    0x5963, // Gauntlet Arena Two (Eldritch Web)
    0x5964, // Gauntlet Arena One (Radiant Blood)
    0x5965, // Gauntlet Arena Two (Radiant Blood)
];

// ACE: Player.HandleNoLogLandblock
/// Called when a player first logs in: a player (not staff) saved on a no-log landblock is moved to
/// its lifestone, if it has one. Returns `playerWasMovedFromNoLogLandblock`.
pub fn handle_no_log_landblock(biota: &mut empyrean_entity::Biota) -> bool {
    use empyrean_entity::enums::{PositionType, WeenieType};

    let player_was_moved_from_no_log_landblock = false;

    if biota.weenie_type == WeenieType::Sentinel || biota.weenie_type == WeenieType::Admin {
        return player_was_moved_from_no_log_landblock;
    }

    // A biota loaded from the shard always has its position dictionary (possibly empty).
    let Some(positions) = biota.properties_position.as_mut() else {
        return player_was_moved_from_no_log_landblock;
    };
    let Some(location) = positions.get(&PositionType::Location) else {
        return player_was_moved_from_no_log_landblock;
    };

    #[allow(clippy::cast_possible_truncation)] // `(ushort)(location.ObjCellId >> 16)`
    let landblock = (location.obj_cell_id >> 16) as u16;

    if !NO_LOG_LANDBLOCKS.contains(&landblock) {
        return player_was_moved_from_no_log_landblock;
    }

    let Some(lifestone) = positions.get(&PositionType::Sanctuary).cloned() else {
        return player_was_moved_from_no_log_landblock;
    };

    let location = positions
        .get_mut(&PositionType::Location)
        .expect("found above");
    location.obj_cell_id = lifestone.obj_cell_id;
    location.position_x = lifestone.position_x;
    location.position_y = lifestone.position_y;
    location.position_z = lifestone.position_z;
    location.rotation_x = lifestone.rotation_x;
    location.rotation_y = lifestone.rotation_y;
    location.rotation_z = lifestone.rotation_z;
    location.rotation_w = lifestone.rotation_w;

    true
}

// ---- dispatch targets ----

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerLocationFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_location
}

/// The player's `Player_Location` fields.
///
/// # Panics
/// When `this` is not a live player.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerLocationFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_location
}

// ACE: Player.MarketplaceDrop
/// The marketplace portal's destination from the world database, else ACE's fixed position.
#[must_use]
#[allow(clippy::approx_constant)] // ACE's literal rotation
pub fn marketplace_drop(w: &World) -> Position {
    w.content
        .get_cached_weenie_by_class_name("portalmarketplace")
        .and_then(|weenie| weenie.get_position(PositionType::Destination))
        .unwrap_or_else(|| {
            Position::from_components(
                0x016C_01BC,
                49.206,
                -31.935,
                0.005,
                0.0,
                0.0,
                -0.707_107,
                0.707_107,
                false,
            )
        })
}

// ACE: Player.TeleToPosition
/// Teleports the player to position; true on success (position is set), false otherwise.
pub fn tele_to_position(w: &mut World, this: ObjectGuid, position_type: PositionType) -> bool {
    let position = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .get_position(position_type);

    if let Some(position) = position {
        let mut teleport_dest = Position::from_position(&position);
        world_object::adjust_dungeon(w, &mut teleport_dest);

        teleport(w, this, &teleport_dest, false);
        return true;
    }

    false
}

// ACE: Player.motionLifestoneRecall
#[must_use]
pub fn motion_lifestone_recall() -> Motion {
    Motion::new(MotionStance::NonCombat, MotionCommand::LifestoneRecall, 1.0)
}

// ACE: Player.motionHouseRecall
#[must_use]
pub fn motion_house_recall() -> Motion {
    Motion::new(MotionStance::NonCombat, MotionCommand::HouseRecall, 1.0)
}

// ACE: Player.RecallMoveThreshold
pub const RECALL_MOVE_THRESHOLD: f32 = 8.0;
// ACE: Player.RecallMoveThresholdSq
pub const RECALL_MOVE_THRESHOLD_SQ: f32 = RECALL_MOVE_THRESHOLD * RECALL_MOVE_THRESHOLD;

// ACE: Player.TooBusyToRecall
/// Recalls could be started from portal space?
#[must_use]
pub fn too_busy_to_recall(w: &World, this: ObjectGuid) -> bool {
    // `IsBusy || suicideInProgress`: `suicideInProgress` is only set by the /die command
    // (`Player_Death.cs`), which is not ported, so it is false here.
    w.objects
        .get(this)
        .expect("ACE: this is null")
        .wo
        .world_object
        .is_busy
}

/// The common refusals of every recall, in ACE's order: an Olthoi (when `olthoi` is set), a PK
/// timer, disabled recalls, too busy. Answers whether the recall may go on.
fn recall_checks(w: &mut World, this: ObjectGuid, olthoi: bool) -> bool {
    if olthoi && is_olthoi_player(w, this) {
        send_weenie_error(w, this, WeenieError::OlthoiCanOnlyRecallToLifestone);
        return false;
    }

    if crate::world_objects::player_move::pk_timer_active(w, this) {
        send_weenie_error(w, this, WeenieError::YouHaveBeenInPKBattleTooRecently);
        return false;
    }

    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .recalls_disabled()
    {
        send_weenie_error(w, this, WeenieError::ExitTrainingAcademyToUseCommand);
        return false;
    }

    if too_busy_to_recall(w, this) {
        send_weenie_error(w, this, WeenieError::YoureTooBusy);
        return false;
    }

    true
}

/// `if (CombatMode != CombatMode.NonCombat)`: the forced peace mode every recall starts with.
fn force_peace_mode(w: &mut World, this: ObjectGuid) {
    if crate::world_objects::player_move::creature_combat_mode(w, this) != CombatMode::NonCombat {
        // this should be handled by a different thing, probably a function that forces player into peacemode
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        let update_combat_mode =
            game_message_private_update_property_int::game_message_private_update_property_int(
                o,
                PropertyInt::CombatMode,
                CombatMode::NonCombat.0,
            );
        creature_set_combat_mode(w, this, CombatMode::NonCombat);
        send(w, this, update_combat_mode);
    }
}

/// `EnqueueBroadcast(new GameMessageSystemChat($"{Name} {text}", ChatMessageType.Recall), LocalBroadcastRange, ChatMessageType.Recall)`.
fn broadcast_recall(w: &mut World, this: ObjectGuid, text: &str) {
    let name = shims::name(w, this).unwrap_or_default();
    let msg = game_message_system_chat::game_message_system_chat(
        &format!("{name} {text}"),
        ChatMessageType::Recall,
    );
    world_object_networking::enqueue_broadcast_range(
        w,
        this,
        &msg,
        LOCAL_BROADCAST_RANGE,
        Some(ChatMessageType::Recall),
    );
}

/// `DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(motion)`.
fn recall_animation_length(w: &World, this: ObjectGuid, motion: MotionCommand) -> f32 {
    let motion_table_id = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .motion_table_id();
    let mt = w
        .dats
        .portal_dat()
        .read_from_dat::<MotionTable>(motion_table_id);
    motion_table::get_animation_length_of(w, mt.as_deref(), motion)
}

/// The recall's delayed tail: after `delay` seconds the player is no longer busy; if it moved
/// more than 8 m it is told so, otherwise `then` runs (it re-verifies and teleports).
fn recall_chain(
    w: &mut World,
    this: ObjectGuid,
    delay: f32,
    then: impl FnOnce(&mut World) + Send + 'static,
) {
    let start_pos = Position::from_position(&location(w, this));

    // Wait for animation
    let mut action_chain = ActionChain::new();

    // Then do teleport
    action_chain.add_delay_seconds(w, f64::from(delay));
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object
        .is_busy = true;
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .wo
            .world_object
            .is_busy = false;
        let end_pos = Position::from_position(&location(w, this));
        if start_pos.squared_distance_to(&end_pos) > RECALL_MOVE_THRESHOLD_SQ {
            send_weenie_error(w, this, WeenieError::YouHaveMovedTooFar);
            return;
        }
        then(w);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.HandleActionTeleToHouse
/// `/house recall`: to the player's (or the account's) house, after the house recall animation.
pub fn handle_action_tele_to_house(w: &mut World, this: ObjectGuid) {
    // DIVERGE: house recalls go with apartments (`EraFeatures::apartments`); a world without
    // them refuses this one (V420).
    if !crate::world_objects::era_gates::has(w, this, w.era.features.apartments, "house recall") {
        return;
    }

    if !recall_checks(w, this, true) {
        return;
    }

    let house = player_house(w, this).or_else(|| player_get_account_house(w, this));

    let Some(house) = house else {
        send_weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    };

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is recalling home.");

    send_motion_as_commands(w, this, MotionCommand::HouseRecall, MotionStance::NonCombat);

    // the order of ACE's reads: the start position, then the animation length, then IsBusy
    let anim_length = recall_animation_length(w, this, MotionCommand::HouseRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        if let Some(dest) = house_slum_lord_location(w, house) {
            teleport(w, this, &dest, false);
        }
    });
}

// ACE: Player.HandleActionTeleToLifestone
/// Handles teleporting a player to the lifestone (/ls or /lifestone command): half the mana, the
/// lifestone recall animation, then the teleport to `Sanctuary`.
pub fn handle_action_tele_to_lifestone(w: &mut World, this: ObjectGuid) {
    if !recall_checks(w, this, false) {
        return;
    }

    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .sanctuary()
        .is_none()
    {
        let msg = game_message_system_chat::game_message_system_chat(
            "Your spirit has not been attuned to a sanctuary location.",
            ChatMessageType::Broadcast,
        );
        send(w, this, msg);
        return;
    }

    // FIXME(ddevec): I should probably make a better interface for this
    let o = w.objects.get(this).expect("ACE: this is null");
    let mana = o.mana();
    let half: u32 = mana.current(o) / 2;
    crate::dispatch::update_vital::update_vital(w, this, mana, half.cs_cast());

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is recalling to the lifestone.");

    send_motion_as_commands(
        w,
        this,
        MotionCommand::LifestoneRecall,
        MotionStance::NonCombat,
    );

    let anim_length = recall_animation_length(w, this, MotionCommand::LifestoneRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        let sanctuary = w
            .objects
            .get(this)
            .expect("ACE: this is null")
            .sanctuary()
            .expect("ACE: Sanctuary is null");
        teleport(w, this, &sanctuary, false);
    });
}

// ACE: Player.motionMarketplaceRecall
#[must_use]
pub fn motion_marketplace_recall() -> Motion {
    Motion::new(
        MotionStance::NonCombat,
        MotionCommand::MarketplaceRecall,
        1.0,
    )
}

// ACE: Player.HandleActionTeleToMarketPlace
/// The marketplace recall: a fixed 14 s delay (retail's animation is shorter than the table's
/// 18.4 s), then the teleport to the marketplace drop.
pub fn handle_action_tele_to_market_place(w: &mut World, this: ObjectGuid) {
    if !recall_checks(w, this, true) {
        return;
    }

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is recalling to the marketplace.");

    send_motion_as_commands(
        w,
        this,
        MotionCommand::MarketplaceRecall,
        MotionStance::NonCombat,
    );

    // TODO: (OptimShi): Actual animation length is longer than in retail. 18.4s
    recall_chain(w, this, 14.0, move |w: &mut World| {
        let drop = marketplace_drop(w);
        teleport(w, this, &drop, false);
    });
}

// ACE: Player.motionAllegianceHometownRecall
#[must_use]
pub fn motion_allegiance_hometown_recall() -> Motion {
    Motion::new(
        MotionStance::NonCombat,
        MotionCommand::AllegianceHometownRecall,
        1.0,
    )
}

// ACE: Player.HandleActionRecallAllegianceHometown
pub fn handle_action_recall_allegiance_hometown(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionRecallAllegianceHometown()");

    if !recall_checks(w, this, true) {
        return;
    }

    // check if player is in an allegiance
    if !verify_recall_allegiance_hometown(w, this) {
        return;
    }

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is going to the Allegiance hometown.");

    send_motion_as_commands(
        w,
        this,
        MotionCommand::AllegianceHometownRecall,
        MotionStance::NonCombat,
    );

    let anim_length = recall_animation_length(w, this, MotionCommand::AllegianceHometownRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        // re-verify
        if !verify_recall_allegiance_hometown(w, this) {
            return;
        }

        if let Some(sanctuary) = allegiance_sanctuary(w, this) {
            teleport(w, this, &sanctuary, false);
        }
    });
}

// ACE: Player.VerifyRecallAllegianceHometown
fn verify_recall_allegiance_hometown(w: &mut World, this: ObjectGuid) -> bool {
    if !player_has_allegiance(w, this) {
        send_weenie_error(w, this, WeenieError::YouAreNotInAllegiance);
        return false;
    }

    if allegiance_sanctuary(w, this).is_none() {
        send_weenie_error(w, this, WeenieError::YourAllegianceDoesNotHaveHometown);
        return false;
    }

    true
}

// ACE: Player.HandleActionTeleToMansion
/// Recalls you to your allegiance's Mansion or Villa.
pub fn handle_action_tele_to_mansion(w: &mut World, this: ObjectGuid) {
    // DIVERGE: house recalls go with apartments (`EraFeatures::apartments`); a world without
    // them refuses this one (V420).
    if !crate::world_objects::era_gates::has(w, this, w.era.features.apartments, "house recall") {
        return;
    }

    //Console.WriteLine($"{Name}.HandleActionTeleToMansion()");

    if !recall_checks(w, this, true) {
        return;
    }

    if verify_tele_to_mansion(w, this).is_none() {
        return;
    }

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is recalling to the Allegiance housing.");

    send_motion_as_commands(w, this, MotionCommand::HouseRecall, MotionStance::NonCombat);

    let anim_length = recall_animation_length(w, this, MotionCommand::HouseRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        // re-verify
        let Some(allegiance_house) = verify_tele_to_mansion(w, this) else {
            return;
        };

        if let Some(dest) = house_slum_lord_location(w, allegiance_house) {
            teleport(w, this, &dest, false);
        }
    });
}

// ACE: Player.VerifyTeleToMansion
/// The allegiance house the mansion recall goes to: the player must be in an allegiance whose
/// monarch owns an open villa or mansion.
fn verify_tele_to_mansion(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    // check if player is in an allegiance
    if !player_has_allegiance(w, this) {
        send_weenie_error(w, this, WeenieError::YouAreNotInAllegiance);
        return None;
    }

    let Some(allegiance_house) = allegiance_get_house(w, this) else {
        send_weenie_error(w, this, WeenieError::YourMonarchDoesNotOwnAMansionOrVilla);
        return None;
    };

    if !house_is_villa_or_mansion(w, allegiance_house) {
        send_weenie_error(w, this, WeenieError::YourMonarchsHouseIsNotAMansionOrVilla);
        return None;
    }

    // ensure allegiance housing has allegiance permissions enabled
    if !house_has_monarch_id(w, allegiance_house) {
        send_weenie_error(w, this, WeenieError::YourMonarchHasClosedTheMansion);
        return None;
    }

    Some(allegiance_house)
}

// ACE: Player.motionPkArenaRecall
#[must_use]
pub fn motion_pk_arena_recall() -> Motion {
    Motion::new(MotionStance::NonCombat, MotionCommand::PKArenaRecall, 1.0)
}

/// A PK arena drop: the world database's portal destination, else ACE's fixed position.
fn arena_loc(w: &World, class_name: &str, cell: u32, x: f32, y: f32, qz: f32, qw: f32) -> Position {
    Position::from_position(
        &w.content
            .get_cached_weenie_by_class_name(class_name)
            .and_then(|weenie| weenie.get_position(PositionType::Destination))
            .unwrap_or_else(|| {
                Position::from_components(cell, x, y, 0.005, 0.0, 0.0, qz, qw, false)
            }),
    )
}

// ACE: Player.pkArenaLocs
#[must_use]
pub fn pk_arena_locs(w: &World) -> Vec<Position> {
    vec![
        arena_loc(
            w,
            "portalpkarenanew1",
            0x0066_0117,
            30.0,
            -50.0,
            0.000_000,
            1.000_000,
        ),
        arena_loc(
            w,
            "portalpkarenanew2",
            0x0066_0106,
            10.0,
            0.0,
            -0.947_071,
            0.321_023,
        ),
        arena_loc(
            w,
            "portalpkarenanew3",
            0x0066_0103,
            30.0,
            -30.0,
            -0.699_713,
            0.714_424,
        ),
        arena_loc(
            w,
            "portalpkarenanew4",
            0x0066_011E,
            50.0,
            0.0,
            -0.961_021,
            -0.276_474,
        ),
        arena_loc(
            w,
            "portalpkarenanew5",
            0x0066_0127,
            60.0,
            -30.0,
            0.681_639,
            0.731_689,
        ),
    ]
}

// ACE: Player.HandleActionTeleToPkArena
pub fn handle_action_tele_to_pk_arena(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionTeleToPkArena()");

    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .player_killer_status()
        != PlayerKillerStatus::PK
    {
        send_weenie_error(w, this, WeenieError::OnlyPKsMayUseCommand);
        return;
    }

    if !recall_checks(w, this, false) {
        return;
    }

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is going to the PK Arena.");

    send_motion_as_commands(
        w,
        this,
        MotionCommand::PKArenaRecall,
        MotionStance::NonCombat,
    );

    let anim_length = recall_animation_length(w, this, MotionCommand::PKArenaRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        let locs = pk_arena_locs(w);
        let rng = ThreadSafeRandom::next(
            0,
            i32::try_from(locs.len())
                .unwrap_or(i32::MAX)
                .wrapping_sub(1),
        );
        let loc = locs[usize::try_from(rng).expect("ACE: index in range")];

        teleport(w, this, &loc, false);
    });
}

// ACE: Player.pklArenaLocs
#[must_use]
pub fn pkl_arena_locs(w: &World) -> Vec<Position> {
    vec![
        arena_loc(
            w,
            "portalpklarenanew1",
            0x0067_0117,
            30.0,
            -50.0,
            0.000_000,
            1.000_000,
        ),
        arena_loc(
            w,
            "portalpklarenanew2",
            0x0067_0106,
            10.0,
            0.0,
            -0.947_071,
            0.321_023,
        ),
        arena_loc(
            w,
            "portalpklarenanew3",
            0x0067_0103,
            30.0,
            -30.0,
            -0.699_713,
            0.714_424,
        ),
        arena_loc(
            w,
            "portalpklarenanew4",
            0x0067_011E,
            50.0,
            0.0,
            -0.961_021,
            -0.276_474,
        ),
        arena_loc(
            w,
            "portalpklarenanew5",
            0x0067_0127,
            60.0,
            -30.0,
            0.681_639,
            0.731_689,
        ),
    ]
}

// ACE: Player.HandleActionTeleToPklArena
pub fn handle_action_tele_to_pkl_arena(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionTeleToPkLiteArena()");

    if is_olthoi_player(w, this) {
        send_weenie_error(w, this, WeenieError::OlthoiCanOnlyRecallToLifestone);
        return;
    }

    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .player_killer_status()
        != PlayerKillerStatus::PKLite
    {
        send_weenie_error(w, this, WeenieError::OnlyPKLiteMayUseCommand);
        return;
    }

    if !recall_checks(w, this, false) {
        return;
    }

    force_peace_mode(w, this);

    broadcast_recall(w, this, "is going to the PKL Arena.");

    send_motion_as_commands(
        w,
        this,
        MotionCommand::PKArenaRecall,
        MotionStance::NonCombat,
    );

    let anim_length = recall_animation_length(w, this, MotionCommand::PKArenaRecall);
    recall_chain(w, this, anim_length, move |w: &mut World| {
        let locs = pkl_arena_locs(w);
        let rng = ThreadSafeRandom::next(
            0,
            i32::try_from(locs.len())
                .unwrap_or(i32::MAX)
                .wrapping_sub(1),
        );
        let loc = locs[usize::try_from(rng).expect("ACE: index in range")];

        teleport(w, this, &loc, false);
    });
}

// ACE: Player.SendMotionAsCommands
/// A full-physics player runs the motion as an action chain; any other broadcasts it as a
/// command on a Ready motion.
pub fn send_motion_as_commands(
    w: &mut World,
    this: ObjectGuid,
    motion_command: MotionCommand,
    motion_stance: MotionStance,
) {
    if player_tick::fast_tick(w, this) {
        let mut action_chain = ActionChain::new();
        world_object_networking::enqueue_motion_action(
            w,
            this,
            &mut action_chain,
            &[motion_command],
            1.0,
            Some(motion_stance),
            false,
            false,
        );
        action_chain.enqueue_chain(w);
    } else {
        let mut motion = Motion::new(motion_stance, MotionCommand::Ready, 1.0);
        motion.motion_state.add_command(this, motion_command, 1.0);
        world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);
    }
}

// ACE: Player.Teleport
/// Not thread-safe (ACE: use `WorldManager.ThreadSafeTeleport` from a multi-threaded
/// subsection). The player enters portal space: the teleport message, a "fake" update position
/// at the destination so the client starts loading, the portal-space physics state, then the
/// physics update to the destination. A fog colour still set first clears it and teleports a
/// second later.
pub fn teleport(w: &mut World, this: ObjectGuid, new_position_in: &Position, from_portal: bool) {
    let mut new_position = Position::from_position(new_position_in);
    //newPosition.PositionZ += 0.005f;
    let obj_scale = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .obj_scale()
        .unwrap_or(1.0);
    new_position.position_z += 0.005 * obj_scale;

    //Console.WriteLine($"{Name}.Teleport() - Sending to {newPosition.ToLOCString()}");

    // Check currentFogColor set for player. If LandblockManager.GlobalFogColor is set, don't bother checking, dungeons didn't clear like this on retail worlds.
    // if not clear, reset to clear before portaling in case portaling to dungeon (no current way to fast check unloaded landblock for IsDungeon or current FogColor)
    // client doesn't respond to any change inside dungeons, and only queues for change if in dungeon, executing change upon next teleport
    // so if we delay teleport long enough to ensure clear arrives before teleport, we don't get fog carrying over into dungeon.

    let current_fog_color = w
        .objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("a player")
        .player_networking
        .current_fog_color;
    if current_fog_color.is_some_and(|c| c != empyrean_entity::enums::EnvironChangeType::Clear)
        && w.landblock_manager.global_fog_color.is_none()
    {
        let original = *new_position_in;
        let mut delay_telport = ActionChain::new();
        delay_telport.add_action(Actor::Object(this), move |w: &mut World| {
            player_networking::clear_fog_color(w, this)
        });
        delay_telport.add_delay_seconds(w, 1.0);
        delay_telport.add_action(Actor::Object(this), move |w: &mut World| {
            crate::managers::world_manager::thread_safe_teleport(w, this, original, None, false);
        });

        delay_telport.enqueue_chain(w);

        return;
    }

    let now_utc = w.now.utc;
    let now_unix = w.now.unix_time;
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.wo.world_object.teleporting = true;
    o.set_last_teleport_start_timestamp(Some(now_unix));

    if from_portal {
        o.set_last_portal_teleport_timestamp(Some(now_unix));
    }
    fields_mut(w, this).last_teleport_time = now_utc;

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let msg = game_message_player_teleport::game_message_player_teleport(o);
    send(w, this, msg);

    // load quickly, but player can load into landblock before server is finished loading

    // send a "fake" update position to get the client to start loading asap,
    // also might fix some decal bugs
    let prev_loc = location(w, this);
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_location(Some(new_position));
    world_object_networking::send_update_position(w, this, false);
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_location(Some(prev_loc));

    do_teleport_physics_state_changes(w, this);

    // force out of hotspots
    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    phys_ext::report_collision_end(w, h, true);

    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .under_lifestone_protection()
    {
        crate::world_objects::player_death::lifestone_protection_dispel(w, this);
    }

    player_handle_pre_teleport_visibility(w, this, &new_position);

    player_tick::update_player_position(w, this, Position::from_position(&new_position), true);
}

// ACE: Player.DoPreTeleportHide
pub fn do_pre_teleport_hide(w: &mut World, this: ObjectGuid) {
    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .wo
        .world_object
        .teleporting
    {
        return;
    }
    world_object::play_particle_effect(
        w,
        this,
        empyrean_entity::enums::PlayScript::Hide,
        this,
        1.0,
    );
}

// ACE: Player.DoTeleportPhysicsStateChanges
/// Portal space: hidden, ignoring collisions and not reporting them; broadcast if any changed.
pub fn do_teleport_physics_state_changes(w: &mut World, this: ObjectGuid) {
    let mut broadcast_update = false;

    let old_hidden = hidden(w, this);
    let o = w.objects.get(this).expect("ACE: this is null");
    let old_ignore = o
        .ignore_collisions()
        .expect("ACE: Nullable object must have a value (IgnoreCollisions)");
    let old_report = o
        .report_collisions()
        .expect("ACE: Nullable object must have a value (ReportCollisions)");

    phys_ext::set_physics_state(w, this, PhysicsState::Hidden, Some(true));
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::IgnoreCollisions,
        PhysicsState::IgnoreCollisions,
        Some(true),
    );
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::ReportCollisions,
        PhysicsState::ReportCollisions,
        Some(false),
    );

    let o = w.objects.get(this).expect("ACE: this is null");
    if hidden(w, this) != old_hidden
        || o.ignore_collisions() != Some(old_ignore)
        || o.report_collisions() != Some(old_report)
    {
        broadcast_update = true;
    }

    if broadcast_update {
        world_object::enqueue_broadcast_physics_state(w, this);
    }
}

// ACE: Player.OnTeleportComplete
/// The client left portal space (`GameActionLoginComplete`). While the landblock's critical
/// resources are still loading the player stays in the pink bubble and checks every 0.1 s; then
/// it materializes.
pub fn on_teleport_complete(w: &mut World, this: ObjectGuid) {
    let current_landblock = w.objects.get(this).and_then(|o| o.current_landblock);
    let completed = current_landblock.is_none_or(|id| {
        w.landblock_manager
            .landblocks
            .get(id)
            .is_none_or(crate::entity::landblock::Landblock::create_world_objects_completed)
    });
    if !completed {
        // If the critical landblock resources haven't been loaded yet, we keep the player in the pink bubble state
        // We'll check periodically to see when it's safe to let them materialize in
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, 0.1);
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            on_teleport_complete(w, this)
        });
        action_chain.enqueue_chain(w);
        return;
    }

    // set materialize physics state
    // this takes the player from pink bubbles -> fully materialized
    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .cloak_status()
        != CloakStatus::On
    {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::ReportCollisions,
            PhysicsState::ReportCollisions,
            Some(true),
        );
    }

    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::IgnoreCollisions,
        PhysicsState::IgnoreCollisions,
        Some(false),
    );
    phys_ext::set_physics_state(w, this, PhysicsState::Hidden, Some(false));
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object
        .teleporting = false;

    player_tick::check_monsters(w, this);
    player_check_house(w, this);

    world_object::enqueue_broadcast_physics_state(w, this);

    // hijacking this for both start/end on portal teleport
    let o = w.objects.get(this).expect("ACE: this is null");
    #[allow(clippy::float_cmp)] // C#'s double? ==
    if o.last_teleport_start_timestamp() == o.last_portal_teleport_timestamp() {
        let now = w.now.unix_time;
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .set_last_portal_teleport_timestamp(Some(now));
    }
}

// ACE: Player.SendTeleportedViaMagicMessage
/// "You have been teleported." for a gem or no caster; otherwise, unless the caster is the
/// player, a switch or a silent NPC, who teleported you with what.
pub fn send_teleported_via_magic_message(
    w: &mut World,
    this: ObjectGuid,
    item_caster: Option<ObjectGuid>,
    spell: &Spell,
) {
    let caster = item_caster.and_then(|g| w.objects.get(g));
    match caster {
        None => {
            let msg = game_message_system_chat::game_message_system_chat(
                "You have been teleported.",
                ChatMessageType::Magic,
            );
            send(w, this, msg);
        }
        Some(c) if c.is_gem() => {
            let msg = game_message_system_chat::game_message_system_chat(
                "You have been teleported.",
                ChatMessageType::Magic,
            );
            send(w, this, msg);
        }
        Some(c) => {
            if Some(this) != item_caster
                && !c.is_switch()
                && !c
                    .get_property(PropertyBool::NpcInteractsSilently)
                    .unwrap_or(false)
            {
                let caster_name = shims::name(w, item_caster.expect("matched")).unwrap_or_default();
                let text = format!("{caster_name} teleports you with {}.", spell.name());
                let msg = game_message_system_chat::game_message_system_chat(
                    &text,
                    ChatMessageType::Magic,
                );
                send(w, this, msg);
            }
        }
    }
    //else if (itemCaster is Gem)
    //    Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.ITeleported));
}

// ACE: Player.NotifyLandblocks
/// Players notify their landblock of their activity (the reverse of ACE's original landblock
/// heartbeat checks).
pub fn notify_landblocks(w: &mut World, this: ObjectGuid) {
    // notify current landblock of player activity
    if let Some(id) = w.objects.get(this).and_then(|o| o.current_landblock) {
        crate::entity::landblock::set_active(w, id, false);
    }
}

// ACE: Player.RunFactor
pub const RUN_FACTOR: f32 = 1.5;

// ACE: Player.GetRotateDelay
/// Returns the amount of time for player to rotate by the # of degrees from the input angle,
/// using the omega speed from its MotionTable.
pub fn player_get_rotate_delay(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    angle: f32,
) -> f32 {
    crate::world_objects::creature_navigation::creature_get_rotate_delay(w, this, angle)
        / RUN_FACTOR
}

// ---- small helpers ------------------------------------------------------------------------------

fn location(w: &World, this: ObjectGuid) -> Position {
    w.objects
        .get(this)
        .and_then(world_object::WorldObject::location)
        .expect("ACE: Location is null")
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: game_message::GameMessage) {
    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_message::enqueue_send(w, session, msg);
}

/// `Session.Network.EnqueueSend(new GameEventWeenieError(Session, error))`.
fn send_weenie_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    player_networking::send_weenie_error(w, this, error);
}

/// `Hidden.Value`: `GetPhysicsState(PhysicsState.Hidden)`.
fn hidden(w: &World, this: ObjectGuid) -> bool {
    assert!(
        phys_ext::physics_obj(w, this).is_some(),
        "ACE: Nullable object must have a value (Hidden)"
    );
    phys_ext::get_physics_state(w, this, PhysicsState::Hidden)
}

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`): the value `SetEphemeralValues` stores,
/// `HeritageGroup == Olthoi || OlthoiAcid`.
fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    let heritage_group = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .heritage_group();
    heritage_group == HeritageGroup::Olthoi || heritage_group == HeritageGroup::OlthoiAcid
}

// ---- pointers to members ported in other files --------------------------------------------------

/// `Creature.SetCombatMode(newCombatMode)` (`Creature_Combat.cs`).
fn creature_set_combat_mode(w: &mut World, this: ObjectGuid, combat_mode: CombatMode) {
    crate::world_objects::creature_combat::set_combat_mode(w, this, combat_mode);
}

/// `Player.House` (`Player_House.cs`): the player's house.
fn player_house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_house::house(w, this)
}

/// `Player.GetAccountHouse()` (`Player_House.cs`).
fn player_get_account_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_house::get_account_house(w, this)
}

/// `house.SlumLord.Location` (`House.cs`).
fn house_slum_lord_location(w: &mut World, house: ObjectGuid) -> Option<Position> {
    let house = crate::world_objects::house::resolve(w, house)?;
    let slum_lord = crate::world_objects::house::slum_lord(w, house)
        .expect("System.NullReferenceException: house.SlumLord");
    w.objects.get(slum_lord).and_then(WorldObject::location)
}

/// `allegianceHouse.HouseType == Villa || Mansion` (`House.cs`).
fn house_is_villa_or_mansion(w: &World, house: ObjectGuid) -> bool {
    let t = w
        .objects
        .get(house)
        .expect("System.NullReferenceException: allegianceHouse")
        .house_type();
    t == empyrean_entity::enums::HouseType::Villa || t == empyrean_entity::enums::HouseType::Mansion
}

/// `allegianceHouse.MonarchId != null` (`House.cs`).
fn house_has_monarch_id(w: &World, house: ObjectGuid) -> bool {
    w.objects
        .get(house)
        .expect("System.NullReferenceException: allegianceHouse")
        .monarch_id()
        .is_some()
}

/// `Player.Allegiance` (`Player_Allegiance.cs`, set by `AllegianceManager` at login).
fn player_allegiance(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_allegiance::i_player_allegiance(
        w,
        crate::entity::i_player::IPlayer::Online(this),
    )
}

/// `Allegiance != null`.
fn player_has_allegiance(w: &World, this: ObjectGuid) -> bool {
    player_allegiance(w, this).is_some()
}

/// `Allegiance.Sanctuary`: the allegiance object's hometown.
fn allegiance_sanctuary(w: &World, this: ObjectGuid) -> Option<Position> {
    player_allegiance(w, this)
        .and_then(|a| w.objects.get(a))
        .and_then(WorldObject::sanctuary)
}

/// `Allegiance.GetHouse()` (`WorldObjects/Allegiance.cs`).
fn allegiance_get_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let allegiance =
        player_allegiance(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
    crate::world_objects::allegiance::get_house(w, allegiance)
}

/// `Player.HandlePreTeleportVisibility(newPosition)` (`Player_Tracking.cs`).
fn player_handle_pre_teleport_visibility(w: &mut World, this: ObjectGuid, new_position: &Position) {
    crate::world_objects::player_tracking::handle_pre_teleport_visibility(w, this, new_position);
}

/// `Player.CheckHouse()` (`Player_House.cs`).
fn player_check_house(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player_house::check_house(w, this);
}
