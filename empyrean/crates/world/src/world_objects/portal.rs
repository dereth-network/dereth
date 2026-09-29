// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Portal.cs
//! Port of `Source/ACE.Server/WorldObjects/Portal.cs`.
//!
//! A portal's destination (and its appraisal text), the use requirements (teleport timing, PK
//! timer, levels, restriction bits, account, advocate and quest checks) and the use itself: the
//! thread-safe teleport, then the last-portal record, the portal emote and `ITeleported`.

use empyrean_common::dotnet::Vector3;
use empyrean_entity::enums::{ChatMessageType, PlayerKillerStatus, PortalBitmask, WeenieError};
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::i_action::Action;
use crate::network::game_event::events::game_event_weenie_error;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{self, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat;
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::world_object_networking::shims;
use crate::world_objects::{player_location, player_networking};
use crate::World;

/// Non-property fields declared in `Portal.cs`.
#[derive(Debug, Default)]
pub struct PortalFields {}

pub use crate::entity::activation_result::ActivationResult;

// ---- virtual-dispatch targets ----

// ACE: Portal.EnterWorld
/// The base enter-world; a portal with a relative destination and no destination then gets one
/// relative to where it spawned.
pub fn portal_enter_world(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    let success = crate::world_objects::world_object::world_object_enter_world(w, this);

    if !success {
        let o = w.objects.get(this).expect("ACE: this is null");
        let location = o.location().map(|l| l.to_loc_string());
        let name = shims::name(w, this).unwrap_or_default();
        log::error!("{name} ({this:?}) failed to spawn @ {location:?}");
        return false;
    }

    let o = w.objects.get(this).expect("ACE: this is null");
    if let (Some(rel), Some(location), None) =
        (o.relative_destination(), o.location(), o.destination())
    {
        let mut relative_destination = Position::from_position(&location);
        relative_destination.set_pos(
            relative_destination.pos()
                + Vector3::new(rel.position_x, rel.position_y, rel.position_z),
        );
        // Not ACE's (a fix, V298): the arrival rotation is `RelativeDestination`'s
        // whole rotation; ACE took its X and W with the portal's own Y and Z, so a turned portal
        // gave a mixed, non-unit rotation.
        relative_destination.rotation_x = rel.rotation_x;
        relative_destination.rotation_y = rel.rotation_y;
        relative_destination.rotation_z = rel.rotation_z;
        relative_destination.rotation_w = rel.rotation_w;
        let cell = crate::entity::position_extensions::get_cell(w, &relative_destination);
        relative_destination.set_landblock_id(empyrean_entity::LandblockId::new(cell));

        update_portal_destination(
            w.objects.get_mut(this).expect("ACE: this is null"),
            Some(relative_destination),
        );
    }

    true
}

// ACE: Portal.UpdatePortalDestination
/// Sets the destination and, unless the portal hides it, the appraisal text: the name and the
/// destination's map coordinates.
pub fn update_portal_destination(o: &mut WorldObject, destination: Option<Position>) {
    o.set_destination(destination);

    if o.portal_show_destination().unwrap_or(true) {
        // `Name`: a portal's is its property (no Portal class overrides the virtual)
        let mut appraisal_portal_destination: String = o
            .get_property(empyrean_entity::enums::PropertyString::Name)
            .unwrap_or_default();

        if let Some(destination) = o.destination() {
            if let Some(dest_coords) =
                crate::entity::position_extensions::get_map_coord_str(&destination)
            {
                appraisal_portal_destination += &format!(" ({dest_coords}).");
            }
        }

        o.set_appraisal_portal_destination(Some(appraisal_portal_destination));
    }
}

// ACE: Portal.SetLinkProperties
/// A linked link spot becomes the portal's destination.
pub fn portal_set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    let link = w.objects.get(wo).expect("ACE: wo is null");
    let is_link_spot =
        world_object::CtorEnv::with_world(w, |env| world_object::is_link_spot(env, link));
    if is_link_spot {
        let location = Position::from_position(&link.location().expect("ACE: wo.Location is null"));
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .set_position(
                empyrean_entity::enums::PositionType::Destination,
                Some(location),
            );
    }
}

// ACE: Portal.IsGateway
#[must_use]
pub fn is_gateway(o: &WorldObject) -> bool {
    o.biota.weenie_class_id == 1955
}

// ACE: Portal.OnCollideObject
/// A player walked into the portal: activate it.
pub fn portal_on_collide_object_player(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    crate::dispatch::on_activate::on_activate(w, this, player);
}

// ACE: Portal.OnCastSpell
/// A portal with a spell casts it; otherwise it is used.
pub fn portal_on_cast_spell(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .spell_did()
        .is_some()
    {
        crate::world_objects::world_object_use::world_object_on_cast_spell(w, this, activator);
    } else {
        crate::dispatch::act_on_use::act_on_use(w, this, activator);
    }
}

// ACE: Portal.minTimeSinceLastPortal
/// If a player tries to use 2 portals in under this amount of time, they receive an error
/// message.
pub const MIN_TIME_SINCE_LAST_PORTAL: f32 = 3.5;

// ACE: Portal.CheckUseRequirements
/// The virtual target: [`check_use_requirements`]'s result.
pub fn portal_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> crate::entity::activation_result::ActivationResult {
    check_use_requirements(w, this, activator)
}

// ACE: Portal.CheckUseRequirements
/// Whether `activator` may use the portal: only a player not already teleporting, to a portal
/// with a destination; not within 3.5 s of the last portal (the error at most once per 3.5 s);
/// not on a PK timer; and, unless the player ignores portal restrictions, its level, the
/// restriction bits, the account, advocacy and the quest restriction. A portal quest is flagged.
#[allow(clippy::too_many_lines)] // ACE's one method
pub fn check_use_requirements(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) -> ActivationResult {
    let Some(player_obj) = w.objects.get(activator).filter(|o| o.is_player()) else {
        return ActivationResult::new(false);
    };

    if player_obj.wo.world_object.teleporting {
        return ActivationResult::new(false);
    }

    let portal = w.objects.get(this).expect("ACE: this is null");
    if portal.destination().is_none() {
        let text = format!(
            "Portal destination for portal ID {} not yet implemented!",
            portal.biota.weenie_class_id
        );
        let msg =
            game_message_system_chat::game_message_system_chat(&text, ChatMessageType::System);
        send(w, activator, msg);
        return ActivationResult::new(false);
    }

    if let Some(last_portal_teleport_timestamp) = player_obj.last_portal_teleport_timestamp() {
        let current_time = w.now.unix_time;

        let time_since_last_portal = current_time - last_portal_teleport_timestamp;

        if time_since_last_portal < f64::from(MIN_TIME_SINCE_LAST_PORTAL) {
            // prevent message spam
            if let Some(error_time) =
                player_location::fields(w, activator).last_portal_teleport_timestamp_error
            {
                let time_since_last_portal_error = current_time - error_time;

                if time_since_last_portal_error < f64::from(MIN_TIME_SINCE_LAST_PORTAL) {
                    return ActivationResult::new(false);
                }
            }

            w.objects
                .get_mut(activator)
                .and_then(|o| o.player.as_mut())
                .expect("a player")
                .player_location
                .last_portal_teleport_timestamp_error = Some(current_time);

            return refuse(w, activator, WeenieError::YouHaveBeenTeleportedTooRecently);
        }
    }

    let portal = w.objects.get(this).expect("ACE: this is null");
    if crate::world_objects::player_move::pk_timer_active(w, activator)
        && !portal.portal_ignores_pk_attack_timer()
    {
        return refuse(w, activator, WeenieError::YouHaveBeenInPKBattleTooRecently);
    }

    let player_obj = w.objects.get(activator).expect("ACE: player is null");
    if !player_obj.ignore_portal_restrictions() {
        let level = player_obj.level();
        let min_level = portal.min_level();
        let max_level = portal.max_level();
        if matches!((level, min_level), (Some(l), Some(m)) if l < m) {
            // You are not powerful enough to interact with that portal!
            return refuse(
                w,
                activator,
                WeenieError::YouAreNotPowerfulEnoughToUsePortal,
            );
        }

        if matches!((level, max_level), (Some(l), Some(m)) if l > m)
            && max_level != Some(0)
            && crate::managers::property_manager::get_bool(
                w,
                "use_portal_max_level_requirement",
                true,
                true,
            )
            .item
        {
            // You are too powerful to interact with that portal!
            return refuse(w, activator, WeenieError::YouAreTooPowerfulToUsePortal);
        }

        let portal = w.objects.get(this).expect("ACE: this is null");
        let restrictions = portal.portal_restrictions();
        let has = |flag: PortalBitmask| restrictions.0 & flag.0 == flag.0;
        let status = w
            .objects
            .get(activator)
            .expect("ACE: player is null")
            .player_killer_status();

        if restrictions == PortalBitmask::Undef {
            // Players may not interact with that portal.
            return refuse(w, activator, WeenieError::PlayersMayNotUsePortal);
        }

        if has(PortalBitmask::NoPk) && status == PlayerKillerStatus::PK {
            // Player killers may not interact with that portal!
            return refuse(w, activator, WeenieError::PKsMayNotUsePortal);
        }

        if has(PortalBitmask::NoPKLite) && status == PlayerKillerStatus::PKLite {
            // Lite Player Killers may not interact with that portal!
            return refuse(w, activator, WeenieError::PKLiteMayNotUsePortal);
        }

        if has(PortalBitmask::NoNPK) && status == PlayerKillerStatus::NPK {
            // Non-player killers may not interact with that portal!
            return refuse(w, activator, WeenieError::NonPKsMayNotUsePortal);
        }

        let is_olthoi = is_olthoi_player(w, activator);
        if has(PortalBitmask::OnlyOlthoiPCs) && !is_olthoi {
            // Only Olthoi may pass through this portal!
            return refuse(w, activator, WeenieError::OnlyOlthoiMayUsePortal);
        }

        let gateway = is_gateway(w.objects.get(this).expect("ACE: this is null"));
        if (has(PortalBitmask::NoOlthoiPCs) || gateway) && is_olthoi {
            // Olthoi may not pass through this portal!
            return refuse(w, activator, WeenieError::OlthoiMayNotUsePortal);
        }

        if has(PortalBitmask::NoVitae)
            && crate::world_objects::managers::enchantment_manager_with_caching::has_vitae(
                w, activator,
            )
        {
            // You may not pass through this portal while Vitae weakens you!
            return refuse(w, activator, WeenieError::YouMayNotUsePortalWithVitae);
        }

        let player_obj = w.objects.get(activator).expect("ACE: player is null");
        if has(PortalBitmask::NoNewAccounts) && !player_obj.account15_days() {
            // This character must be two weeks old or have been created on an account at least two weeks old to use this portal!
            return refuse(w, activator, WeenieError::YouMustBeTwoWeeksOldToUsePortal);
        }

        let portal = w.objects.get(this).expect("ACE: this is null");
        // `player.AccountRequirements < AccountRequirements` (a null portal requirement compares false)
        if portal
            .account_requirements_portal()
            .is_some_and(|r| player_obj.account_requirements_player().0 < r.0)
        {
            // You must purchase Asheron's Call -- Throne of Destiny to use this portal.
            return refuse(
                w,
                activator,
                WeenieError::MustPurchaseThroneOfDestinyToUsePortal,
            );
        }

        if portal.advocate_quest_portal().unwrap_or(false) && !player_obj.is_advocate() {
            // You must be an Advocate to interact with that portal.
            return refuse(w, activator, WeenieError::YouMustBeAnAdvocateToUsePortal);
        }
    }

    let portal = w.objects.get(this).expect("ACE: this is null");
    let ignore_restrictions = w
        .objects
        .get(activator)
        .expect("ACE: player is null")
        .ignore_portal_restrictions();
    if let Some(quest_restriction) = portal.quest_restriction().filter(|_| !ignore_restrictions) {
        let has_quest = quest_manager_has_quest(w, activator, &quest_restriction);
        let can_solve = quest_manager_can_solve(w, activator, &quest_restriction);

        let success = has_quest && !can_solve;

        if !success {
            quest_manager_handle_portal_quest_error(w, activator, &quest_restriction);
            return ActivationResult::new(false);
        }
    }

    // handle quest initial flagging
    if w.objects
        .get(this)
        .expect("ACE: this is null")
        .quest()
        .is_some()
    {
        emote_manager_on_quest(w, this, activator);
    }

    ActivationResult::new(true)
}

// ACE: Portal.ActOnUse
/// A player uses the portal: the teleport to its destination (dungeon-adjusted) runs on the
/// world queue; then the portal is recorded as the last one (unless it cannot be recalled to),
/// the portal emote runs and the player is told `ITeleported`.
pub fn portal_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    let portal = w.objects.get(this).expect("ACE: this is null");
    let mut portal_dest = Position::from_position(
        &portal
            .destination()
            .expect("ACE: Destination is null (NullReferenceException)"),
    );
    world_object::adjust_dungeon(w, &mut portal_dest);

    let portal = w.objects.get(this).expect("ACE: this is null");
    let no_recall = no_recall(portal);
    let last_portal_did = portal
        .original_portal()
        .unwrap_or(portal.biota.weenie_class_id);
    crate::managers::world_manager::thread_safe_teleport(
        w,
        player,
        portal_dest,
        Some(Action::delegate(move |w: &mut World| {
            // If the portal just used is able to be recalled to,
            // save the destination coordinates to the LastPortal character position save table
            if !no_recall {
                // if walking through a summoned portal
                w.objects
                    .get_mut(player)
                    .expect("ACE: player is null")
                    .set_last_portal_did(Some(last_portal_did));
            }

            emote_manager_on_portal(w, this, player);

            player_networking::send_weenie_error(w, player, WeenieError::ITeleported);
        })),
        true,
    );
}

// ---- constructors and SetEphemeralValues ----

/// `new Portal(weenie, guid)` / `new Portal(biota)`: the `WorldObject` constructor, then
/// Portal's `SetEphemeralValues`.
// ACE: Portal.Portal
pub fn portal_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    portal_set_ephemeral_values(o, env);
}

// ACE: Portal.SetEphemeralValues
pub(crate) fn portal_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Portal;

    o.set_activation_response(
        o.activation_response() | empyrean_entity::enums::ActivationResponse::Use,
    );

    let destination = o.destination();
    update_portal_destination(o, destination);
}

// ---- helpers ----------------------------------------------------------------------------------

/// `Portal.NoRecall` (`Portal_Properties.cs`): `(PortalRestrictions & PortalBitmask.NoRecall) != 0`.
fn no_recall(portal: &WorldObject) -> bool {
    portal.no_recall()
}

/// `new ActivationResult(new GameEventWeenieError(player.Session, error))`.
fn refuse(w: &mut World, player: ObjectGuid, error: WeenieError) -> ActivationResult {
    let session = shims::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    ActivationResult::with_message(game_event_weenie_error::game_event_weenie_error(
        session_data(w, session),
        error,
    ))
}

fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session = shims::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_message::enqueue_send(w, session, msg);
}

/// `Player.IsOlthoiPlayer`: `HeritageGroup == Olthoi || OlthoiAcid` (the value
/// `SetEphemeralValues` stores).
fn is_olthoi_player(w: &World, player: ObjectGuid) -> bool {
    let heritage_group = w
        .objects
        .get(player)
        .expect("ACE: player is null")
        .heritage_group();
    heritage_group == empyrean_entity::enums::HeritageGroup::Olthoi
        || heritage_group == empyrean_entity::enums::HeritageGroup::OlthoiAcid
}

// ---- pointers to members other units port -------------------------------------------------------

/// `player.QuestManager.HasQuest(questFormat)` (`Managers/QuestManager.cs`).
fn quest_manager_has_quest(w: &World, player: ObjectGuid, quest: &str) -> bool {
    crate::managers::quest_manager::has_quest(
        w,
        &crate::managers::quest_manager::QuestOwner::Creature(player),
        quest,
    )
}

/// `player.QuestManager.CanSolve(questFormat)` (`Managers/QuestManager.cs`).
fn quest_manager_can_solve(w: &World, player: ObjectGuid, quest: &str) -> bool {
    crate::managers::quest_manager::can_solve(
        w,
        &crate::managers::quest_manager::QuestOwner::Creature(player),
        quest,
    )
}

/// `player.QuestManager.HandlePortalQuestError(questFormat)` (`Managers/QuestManager.cs`).
fn quest_manager_handle_portal_quest_error(w: &mut World, player: ObjectGuid, quest: &str) {
    crate::managers::quest_manager::handle_portal_quest_error(
        w,
        &crate::managers::quest_manager::QuestOwner::Creature(player),
        quest,
    );
}

/// `EmoteManager.OnQuest(player)` (`Managers/EmoteManager.cs`).
fn emote_manager_on_quest(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_quest(w, this, player);
}

/// `EmoteManager.OnPortal(player)` (`Managers/EmoteManager.cs`).
fn emote_manager_on_portal(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_portal(w, this, player);
}
