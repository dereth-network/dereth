// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player.cs
//! Port of `Source/ACE.Server/WorldObjects/Player.cs`.

/// Non-property fields declared in `Player.cs`.
#[derive(Debug)]
pub struct PlayerFields {
    // ACE: Player.Account
    pub account: Option<empyrean_store::models::auth::Account>,
    /// The shard `character` row (`null` only for a player constructed without one).
    // ACE: Player.Character
    pub character: Option<empyrean_store::models::shard::Character>,
    /// The possessions the login constructor receives (`inventory`, `wieldedItems`). ACE's
    /// constructor sorts them into `Inventory` and `EquippedObjects` itself; here every object
    /// lives in `World.objects`, which a constructor cannot reach, so they wait here until
    /// [`player_ctor_load_possessions`] runs right after the player joins the store.
    pub login_possessions: Option<LoginPossessions>,
    /// `Session.SetAccessLevel(AccessLevel.Advocate)` from `SetEphemeralValues`, applied by
    /// `DoPlayerEnterWorld` as soon as the constructor returns (a constructor cannot reach
    /// `World.sessions`; nothing reads the session in between).
    pub pending_session_access_level: Option<empyrean_entity::enums::AccessLevel>,
    /// Returns TRUE if a Player Killer has clicked logout after being involved in a PK battle
    /// within the past 2 mins. The server delays the logout for 20s, and the client remains in
    /// frozen state during this delay.
    // ACE: Player.PKLogout
    pub pk_logout: bool,
    // ACE: Player.IsLoggingOut
    pub is_logging_out: bool,
    // ACE: Player.ForcedLogOffRequested
    pub forced_log_off_requested: bool,
    // ACE: Player.LastContact
    pub last_contact: bool,
    // ACE: Player.LastJumpTime
    pub last_jump_time: empyrean_common::dotnet::datetime::DotNetDateTime,
    /// The last `/objsend` (`PlayerCommands.HandleObjSend`); `DateTime.MinValue` until then.
    // ACE: Player.PrevObjSend
    pub prev_obj_send: empyrean_common::dotnet::datetime::DotNetDateTime,
    // ACE: Player.LastGroundPos
    pub last_ground_pos: Option<empyrean_entity::Position>,
    // ACE: Player.SnapPos
    pub snap_pos: Option<empyrean_entity::Position>,
    // ACE: Player.SquelchManager
    pub squelch_manager: crate::world_objects::managers::squelch_manager::SquelchManager,
    // ACE: Player.ConfirmationManager
    pub confirmation_manager:
        crate::world_objects::managers::confirmation_manager::ConfirmationManager,
    // ACE: Player.Adminvision
    pub adminvision: bool,
}

impl Default for PlayerFields {
    fn default() -> Self {
        PlayerFields {
            account: None,
            character: None,
            login_possessions: None,
            pending_session_access_level: None,
            pk_logout: false,
            is_logging_out: false,
            forced_log_off_requested: false,
            last_contact: true,
            last_jump_time: empyrean_common::dotnet::datetime::DotNetDateTime::MIN_VALUE,
            prev_obj_send: empyrean_common::dotnet::datetime::DotNetDateTime::MIN_VALUE,
            last_ground_pos: None,
            snap_pos: None,
            squelch_manager: Default::default(),
            confirmation_manager: Default::default(),
            adminvision: false,
        }
    }
}

/// The shard biotas the login constructor was given (`Player(Biota, inventory, wieldedItems, ..)`).
#[derive(Debug, Default)]
pub struct LoginPossessions {
    pub inventory: Vec<empyrean_store::models::shard::Biota>,
    pub wielded_items: Vec<empyrean_store::models::shard::Biota>,
}

/// The player's `Player.cs` fields.
///
/// # Panics
/// When `this` is gone or not a player.
#[must_use]
pub fn fields(w: &crate::World, this: empyrean_entity::ObjectGuid) -> &PlayerFields {
    &player_data(w, this).expect("ACE: this is a Player").player
}

/// The player's `Player.cs` fields, mutably.
///
/// # Panics
/// When `this` is gone or not a player.
pub fn fields_mut(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> &mut PlayerFields {
    &mut player_data_mut(w, this)
        .expect("ACE: this is a Player")
        .player
}

// ACE: Player.IsJumping
/// Off the walkable surface (for a non-full-physics player, only while it also has velocity: a
/// fix for OnWalkable briefly dropping for one AutoPos frame).
///
/// A player whose body has left the world (a log-off's `FinalizeLogout` took it off its
/// landblock, while a strike queued before is still to land) reads as ACE's destroyed body does.
#[must_use]
pub fn is_jumping(w: &crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    use crate::physics::phys_ext;
    let Some(h) = phys_ext::physics_obj(w, this) else {
        // DIVERGE: ACE's player keeps its `PhysicsObj` after `RemoveWorldObject` destroys it, and
        // `leave_world` zeroes its TransientState, so it is off the walkable surface; its velocity
        // is `LeaveGround`'s (a player standing through the LogOut motion has none). The port drops
        // the destroyed body with the landblock's DIVERGE, so it reads not-walkable and no velocity
        // (a body that left mid-jump would keep its jump velocity in ACE).
        return crate::world_objects::player_tick::fast_tick(w, this);
    };
    let on_walkable = w
        .physics
        .get(h)
        .is_some_and(|o| o.transient_state.on_walkable());
    if crate::world_objects::player_tick::fast_tick(w, this) {
        !on_walkable
    } else {
        // for npks only, fixes a bug where OnWalkable can briefly lose state for 1 AutoPos frame
        // a good repro for this is collision w/ monsters near the top of ramps
        !on_walkable && phys_ext::velocity(w, h) != empyrean_common::dotnet::Vector3::ZERO
    }
}

// ---- dispatch targets ----

// ACE: Player.HandleActionEmote
/// A `/e` emote: the EmoteText to everyone in local range (no squelch type), heard by NPCs.
pub fn handle_action_emote(w: &mut crate::World, this: empyrean_entity::ObjectGuid, message: &str) {
    if !chat::object(w, this).is_gagged() {
        let sender_name = crate::world_objects::player_properties::get_name_with_suffix(w, this);
        let msg = crate::network::game_messages::messages::game_message_emote_text::game_message_emote_text(this.full(), &sender_name, message);
        crate::world_objects::world_object_networking::enqueue_broadcast_range(
            w,
            this,
            &msg,
            crate::world_objects::world_object::LOCAL_BROADCAST_RANGE,
            None,
        );

        on_talk(w, this, message);
    } else {
        send_gag_error(w, this);
    }
}

// ACE: Player.HandleActionEnterPkLite
/// `/pkl`: a permanent non-PK becomes PK Lite after the entry animation.
pub fn handle_action_enter_pk_lite(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::entity::actions::action_chain::ActionChain;
    use crate::entity::actions::i_actor::Actor;
    use empyrean_common::dotnet::CsCast;
    use empyrean_entity::enums::{
        ChatMessageType, CombatMode, MotionCommand, MotionStance, PlayerKillerStatus, PropertyInt,
        WeenieError,
    };

    // ensure permanent npk
    let o = chat::object(w, this);
    if o.player_killer_status() != PlayerKillerStatus::NPK || o.minimum_time_since_pk().is_some() {
        send_weenie_error(w, this, WeenieError::OnlyNonPKsMayEnterPKLite);
        return;
    }

    if crate::world_objects::player_magic::is_busy(w, this)
        || crate::world_objects::player_melee::teleporting(w, this)
        || crate::world_objects::player_melee::suicide_in_progress(w, this)
    {
        send_weenie_error(w, this, WeenieError::YoureTooBusy);
        return;
    }

    let mut anim_time = 0.0f32;

    if crate::world_objects::player_move::creature_combat_mode(w, this) != CombatMode::NonCombat {
        let session = session_of(w, this);
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        let msg = crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int(
            o,
            PropertyInt::CombatMode,
            CombatMode::NonCombat.0.cs_cast(),
        );
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        anim_time +=
            crate::world_objects::creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
    }

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(anim_time));
    action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
        crate::world_objects::player_magic::set_is_busy(w, this, true);

        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!("{name} is looking for a fight!"),
            ChatMessageType::Broadcast,
        );
        crate::world_objects::world_object_networking::enqueue_broadcast_range(w, this, &msg, crate::world_objects::world_object::LOCAL_BROADCAST_RANGE, None);

        // perform pk lite entry motion / effect
        player_send_motion_as_commands(w, this, MotionCommand::EnterPKLite, MotionStance::NonCombat);

        let mut inner_chain = ActionChain::new();

        // wait for animation to complete
        let motion_table_id = chat::object(w, this).motion_table_id();
        let motion_table = w.dats.portal_dat().read_from_dat::<empyrean_dat::file_types::MotionTable>(motion_table_id);
        let anim_time = crate::physics::motion_table::get_animation_length_of(w, motion_table.as_deref(), MotionCommand::EnterPKLite);
        inner_chain.add_delay_seconds(w, f64::from(anim_time));
        inner_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
            crate::world_objects::player_magic::set_is_busy(w, this, false);

            if crate::managers::property_manager::get_bool(w, "allow_pkl_bump", false, true).item {
                // check for collisions
                w.objects.get_mut(this).expect("ACE: this is null").set_player_killer_status_prop(PlayerKillerStatus::PKLite);

                let h = crate::physics::phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
                let colliding = w.physics.ethereal_check_for_collisions(h);

                if colliding {
                    // try initial placement
                    // DIVERGE: ACE's SetPositionSimple(Position, sliding: true) places with the
                    // Teleport | SendPositionEvent | Slide flags; the shared physics world's
                    // placement takes no flags (arch).
                    let position = crate::physics::phys_ext::position(w, h).expect("ACE: PhysicsObj.Position");
                    let result = crate::physics::phys_ext::set_position(w, h, &position);

                    if result {
                        // handle landblock update?
                        crate::world_objects::world_object::sync_location(w, this);

                        // force broadcast
                        let _ = w
                            .objects
                            .get_mut(this)
                            .expect("ACE: this is null")
                            .sequences
                            .get_next_sequence(crate::network::sequence::sequence_type::SequenceType::ObjectForcePosition);
                        crate::world_objects::world_object_networking::send_update_position(w, this, false);
                    }
                }
            }
            crate::world_objects::player_properties::update_property_int(
                w,
                this,
                this,
                PropertyInt::PlayerKillerStatus,
                Some(PlayerKillerStatus::PKLite.0.cs_cast()),
                true,
            );

            send_weenie_error(w, this, WeenieError::YouAreNowPKLite);
        });

        inner_chain.enqueue_chain(w);
    });
    action_chain.enqueue_chain(w);
}

/// `Session.Network.EnqueueSend(new GameEventWeenieError(Session, error))`.
fn send_weenie_error(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    error: empyrean_entity::enums::WeenieError,
) {
    let session = session_of(w, this);
    let msg = crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error(
        crate::network::game_event::game_event_message::session_data(w, session),
        error,
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: Player.HandleActionForceObjDescSend
/// Sends object description if the client requests it.
///
/// Not ACE's (retail captures, V288): ACE searched everywhere, the whole
/// landblock included, and answered with an appearance update. Retail answered the client's ask
/// with a fresh CreateObject, the same as a normal create, to the asking client only, and only for
/// an object the player still knows ([`known_object_to_describe`]); a stranger in the landblock,
/// a forgotten object or one never sent gets nothing. The reply leaves the known objects and the
/// forget queue alone, and needs no rate limit.
pub fn handle_action_force_obj_desc_send(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    item_guid: u32,
) {
    let Some((wo, from_known)) =
        known_object_to_describe(w, this, empyrean_entity::ObjectGuid::new(item_guid))
    else {
        //log.DebugFormat("HandleActionForceObjDescSend() - couldn't find object {0:X8}", itemGuid);
        return;
    };
    // A known world object is created as the player's normal create made it (admin vision
    // included, a server-only object withheld); a held or wielded item as its own creates are.
    let adminvision = from_known
        && crate::world_objects::world_object_networking::shims::player_adminvision(w, this);
    if from_known && w.objects.get(wo).is_some_and(|o| o.visibility()) && !adminvision {
        return;
    }
    let session = session_of(w, this);
    let msg = crate::network::game_messages::messages::game_message_create_object::game_message_create_object(w, wo, adminvision, adminvision);
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

/// Not ACE's (retail captures, V288): the object `guid` if the player knows
/// it, and whether it came from the known objects: the player itself, its inventory and equipped
/// items, the container it has open, a trade partner's offered item, its known objects, or an item
/// wielded by one of them. Never a search of the landblock.
fn known_object_to_describe(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    guid: empyrean_entity::ObjectGuid,
) -> Option<(empyrean_entity::ObjectGuid, bool)> {
    use crate::world_objects::player_inventory::{find_object, SearchLocations as S};

    let held = S::MyInventory | S::MyEquippedItems | S::LastUsedContainer | S::TradedByOther;
    if let Some(item) = find_object(w, this, guid, held).result {
        return Some((item, false));
    }
    if let Some(known) = find_object(w, this, guid, S::ObjectsKnownByMe).result {
        return Some((known, true));
    }
    // an item a known creature wields where others see it (the ones its create sent along)
    let sent_along = |item: empyrean_entity::ObjectGuid| {
        w.objects
            .get(item)
            .and_then(crate::world_objects::world_object::WorldObject::current_wielded_location)
            .is_some_and(|l| {
                !(l & empyrean_entity::enums::EquipMask::SelectablePlusAmmo).is_empty()
            })
    };
    crate::world_objects::player_tracking::get_known_objects(w, this)
        .into_iter()
        .filter(|&o| {
            w.objects
                .get(o)
                .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
        })
        .find_map(|o| crate::world_objects::creature_equipment::get_equipped_item(w, o, guid))
        .filter(|&item| sent_along(item))
        .map(|item| (item, false))
}

// ACE: Player.HandleActionIdentifyObject
/// Called when player presses the 'e' key to appraise an object.
pub fn handle_action_identify_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    object_guid: u32,
) {
    use crate::world_objects::player_inventory::{find_object, SearchLocations};

    //Console.WriteLine($"{Name}.HandleActionIdentifyObject({objectGuid:X8})");

    if object_guid == 0 {
        // Deselect the formerly selected Target
        //selectedTarget = ObjectGuid.Invalid;
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        o.set_requested_appraisal_target(None);
        o.set_current_appraisal_target(None);
        return;
    }

    let found = find_object(
        w,
        this,
        empyrean_entity::ObjectGuid::new(object_guid),
        SearchLocations::Everywhere,
    );

    let Some(wo) = found.result else {
        //log.DebugFormat("{0}.HandleActionIdentifyObject({1:X8}): couldn't find object", Name, objectGuid);
        let session = session_of(w, this);
        let msg = crate::network::game_event::events::game_event_identify_object_response::game_event_identify_object_response_empty(
            crate::network::game_event::game_event_message::session_data(w, session),
            object_guid,
        );
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        return;
    };

    let current_time = w.now.unix_time;

    // compare with previously requested appraisal target
    let o = chat::object(w, this);
    if Some(object_guid) == o.requested_appraisal_target() {
        if Some(object_guid) == o.current_appraisal_target() {
            // continued success, rng roll no longer needed
            let session = session_of(w, this);
            let msg = crate::network::game_event::events::game_event_identify_object_response::game_event_identify_object_response(w, session, wo, true);
            crate::network::game_messages::game_message::enqueue_send(w, session, msg);
            on_appraisal(w, this, wo, true);
            return;
        }

        // `currentTime < AppraisalRequestedTimestamp + 5.0f` (a null timestamp lifts to false)
        if o.appraisal_requested_timestamp()
            .is_some_and(|t| current_time < t + 5.0)
        {
            // rate limit for unsuccessful appraisal spam
            let session = session_of(w, this);
            let msg = crate::network::game_event::events::game_event_identify_object_response::game_event_identify_object_response(w, session, wo, false);
            crate::network::game_messages::game_message::enqueue_send(w, session, msg);
            on_appraisal(w, this, wo, false);
            return;
        }
    }

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.set_requested_appraisal_target(Some(object_guid));
    o.set_appraisal_requested_timestamp(Some(current_time));

    examine(w, this, wo);
}

// ACE: Player.HandleActionQueryHealth
/// Selects a creature (0 deselects) and sends its health.
pub fn handle_action_query_health(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    object_guid: u32,
) {
    if object_guid == 0 {
        // Deselect the formerly selected Target
        update_selected_target(w, this, None);
        return;
    }

    // `CurrentLandblock?.GetObject(objectGuid) as Creature`
    let obj = chat::object(w, this)
        .current_landblock
        .and_then(|lb| {
            crate::entity::landblock::get_object(
                w,
                lb,
                empyrean_entity::ObjectGuid::new(object_guid),
                true,
            )
        })
        .filter(|&g| {
            w.objects
                .get(g)
                .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
        });

    let Some(obj) = obj else {
        // Deselect the formerly selected Target
        update_selected_target(w, this, None);
        return;
    };

    update_selected_target(w, this, Some(obj));

    let session = session_of(w, this);
    crate::world_objects::world_object::query_health(w, obj, session);
}

// ACE: Player.HandleActionQueryItemMana
pub fn handle_action_query_item_mana(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    item_guid: u32,
) {
    if item_guid == 0 {
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .set_mana_query_target(None);
        return;
    }

    // the object could be in the world or on the player, first check player
    let guid = empyrean_entity::ObjectGuid::new(item_guid);
    let item = crate::world_objects::container::get_inventory_item(w, this, guid)
        .or_else(|| crate::world_objects::creature_equipment::get_equipped_item(w, this, guid));

    if let Some(item) = item {
        let session = session_of(w, this);
        crate::world_objects::world_object::query_item_mana(w, item, session);
    }

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_mana_query_target(Some(item_guid));
}

// ACE: Player.HandleActionSoulEmote
/// A soul emote's text, to everyone in local range (an Olthoi player's only with `NoOlthoiTalk`).
pub fn handle_action_soul_emote(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    message: &str,
) {
    if !chat::object(w, this).is_gagged() {
        let is_olthoi_player = chat::is_olthoi_player(w, this);
        if !is_olthoi_player || (is_olthoi_player && chat::object(w, this).no_olthoi_talk()) {
            let name = crate::dispatch::name::name(w, this).unwrap_or_default();
            let msg = crate::network::game_messages::messages::game_message_soul_emote::game_message_soul_emote(this.full(), &name, message);
            crate::world_objects::world_object_networking::enqueue_broadcast_range(
                w,
                this,
                &msg,
                crate::world_objects::world_object::LOCAL_BROADCAST_RANGE,
                None,
            );
        }

        on_talk(w, this, message);
    } else {
        send_gag_error(w, this);
    }
}

// ACE: Player.HandleActionTalk
/// Local speech: a HearSpeech to everyone within `LocalBroadcastRange` who does not squelch the
/// speaker's Speech, then the NPCs in range hear it (`OnTalk`).
pub fn handle_action_talk(w: &mut crate::World, this: empyrean_entity::ObjectGuid, message: &str) {
    use empyrean_entity::enums::ChatMessageType;

    if !chat::object(w, this).is_gagged() {
        let sender_name = crate::world_objects::player_properties::get_name_with_suffix(w, this);
        let msg = crate::network::game_messages::messages::game_message_hear_speech::game_message_hear_speech(message, &sender_name, this.full(), ChatMessageType::Speech);
        crate::world_objects::world_object_networking::enqueue_broadcast_range(
            w,
            this,
            &msg,
            crate::world_objects::world_object::LOCAL_BROADCAST_RANGE,
            Some(ChatMessageType::Speech),
        );

        on_talk(w, this, message);
    } else {
        send_gag_error(w, this);
    }
}

// ACE: Player.SendGagError
pub fn send_gag_error(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let msg =
        "You are unable to talk locally, globally, or send tells because you have been gagged.";
    chat::send_transient_and_world_broadcast(w, this, msg);
}

// ACE: Player.SendGagNotice
pub fn send_gag_notice(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let msg = "Your chat privileges have been suspended.";
    chat::send_transient_and_world_broadcast(w, this, msg);
}

// ACE: Player.SendUngagNotice
pub fn send_ungag_notice(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let msg = "Your chat privileges have been restored.";
    chat::send_transient_and_world_broadcast(w, this, msg);
}

// ACE: Player.OnTalk
/// Every creature this player knows about within `LocalBroadcastRange` (in a dungeon, only those
/// in its landblock) hears the message (`EmoteManager.OnHearChat`).
pub fn on_talk(w: &mut crate::World, this: empyrean_entity::ObjectGuid, message: &str) {
    let Some(o) = w.objects.get(this) else { return };
    let Some(h) = o.phys else { return };
    if o.current_landblock.is_none() {
        return;
    }

    let is_dungeon = crate::entity::landblock::current_physics_landblock_is_dungeon(w, this);

    let range_squared = crate::world_objects::world_object::LOCAL_BROADCAST_RANGE_SQ;

    let location = chat::object(w, this)
        .location()
        .expect("ACE: Location is null (NullReferenceException)");
    for creature in crate::physics::object_maint::get_known_objects_values_as_creature(w, h) {
        let creature_location = w
            .objects
            .get(creature)
            .and_then(crate::world_objects::world_object::WorldObject::location)
            .expect("ACE: creature.Location is null (NullReferenceException)");
        if is_dungeon && location.landblock() != creature_location.landblock() {
            continue;
        }

        let dist_squared = location.squared_distance_to(&creature_location);
        if dist_squared <= range_squared {
            crate::world_objects::managers::emote_manager::on_hear_chat(w, creature, this, message);
        }
    }
}

/// Helpers for the chat members above.
mod chat {
    use empyrean_entity::enums::ChatMessageType;
    use empyrean_entity::ObjectGuid;

    use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
    use crate::network::game_event::game_event_message::session_data;
    use crate::network::game_messages::game_message::enqueue_send_many;
    use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
    use crate::world_objects::world_object::WorldObject;
    use crate::World;

    pub(super) fn object(w: &World, this: ObjectGuid) -> &WorldObject {
        w.objects.get(this).expect("ACE: this is null")
    }

    /// `Player.IsOlthoiPlayer` (`Player_Properties.cs`), as `SetEphemeralValues` set it.
    pub(super) fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
        object(w, this)
            .player
            .as_ref()
            .is_some_and(|p| p.player_properties.is_olthoi_player)
    }

    /// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg), new
    /// GameMessageSystemChat(msg, ChatMessageType.WorldBroadcast))`.
    pub(super) fn send_transient_and_world_broadcast(w: &mut World, this: ObjectGuid, msg: &str) {
        let session = crate::world_objects::world_object_networking::shims::player_session(w, this)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        let transient = game_event_communication_transient_string(session_data(w, session), msg);
        let chat = game_message_system_chat(msg, ChatMessageType::WorldBroadcast);
        enqueue_send_many(w, session, [transient, chat]);
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Player.InitPhysicsObj
pub fn player_init_physics_obj(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::physics::phys_ext;
    use empyrean_entity::enums::{PhysicsState, PropertyBool};

    crate::world_objects::world_object::world_object_init_physics_obj(w, this);

    // set pink bubble state
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
    phys_ext::set_physics_state(w, this, PhysicsState::Hidden, Some(true));
}

// ACE: Player.OnCollideEnvironment
#[allow(unused_variables)]
pub fn player_on_collide_environment(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    //HandleFallingDamage();
}

// ACE: Player.OnCollideObject
pub fn player_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    //Console.WriteLine($"{Name}.OnCollideObject({target.Name})");

    let Some(t) = w.objects.get(target) else {
        return;
    };
    if t.report_collisions() == Some(false) {
        return;
    }

    if t.is_portal() {
        crate::dispatch::on_collide_object::on_collide_object_player(w, target, this);
    } else if t.is_pressure_plate() {
        crate::world_objects::pressure_plate::pressure_plate_on_collide_object(w, target, this);
    } else if t.is_hotspot() {
        crate::world_objects::hotspot::hotspot_on_collide_object(w, target, this);
    } else if t.is_spell_projectile() {
        crate::world_objects::spell_projectile::spell_projectile_on_collide_object(w, target, this);
    } else if t.projectile.as_ref().is_some_and(|p| p.target.is_some()) {
        crate::world_objects::projectile_collision_helper::on_collide_object(w, target, this);
    }
}

// ACE: Player.OnCollideObjectEnd
pub fn player_on_collide_object_end(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    if w.objects
        .get(target)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_hotspot)
    {
        crate::world_objects::hotspot::hotspot_on_collide_object_end(w, target, this);
    }
}

// ACE: Player.GetBurdenMod
/// Returns a modifier for a player's Run, Jump, Melee Defense, and Missile Defense skills if
/// they are overburdened.
pub fn player_get_burden_mod(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> f32 {
    use dereth_rules::burden as encumbrance_system;

    let strength = crate::world_objects::creature_combat::attribute_current(
        w,
        this,
        empyrean_entity::enums::PropertyAttribute::Strength,
    );

    let o = chat::object(w, this);
    let capacity = crate::physics::weenie_object::encumbrance_system_encumbrance_capacity(
        empyrean_common::dotnet::CsCast::cs_cast(strength),
        o.augmentation_increased_carrying_capacity(),
    );

    let burden = encumbrance_system::load(capacity, o.encumbrance_val().unwrap_or(0));

    //Console.WriteLine($"Burden mod: {burdenMod}");
    encumbrance_system::load_mod(burden)
}

// ACE: Player.Name
/// `IsPlussed && CloakStatus < CloakStatus.Player ? "+" + base.Name : base.Name`.
pub fn player_name(w: &crate::World, this: empyrean_entity::ObjectGuid) -> Option<String> {
    let base_name = crate::world_objects::world_object::world_object_name(w, this);
    let cloak_status = w
        .objects
        .get(this)
        .map(crate::world_objects::world_object::WorldObject::cloak_status);
    if crate::world_objects::player_properties::is_plussed(w, this)
        && cloak_status.is_some_and(|c| c < empyrean_entity::enums::CloakStatus::Player)
    {
        Some(format!("+{}", base_name.unwrap_or_default()))
    } else {
        base_name
    }
}

// ACE: Player.Name
/// `base.Name = value.TrimStart('+')`.
pub fn player_set_name(w: &mut crate::World, this: empyrean_entity::ObjectGuid, value: String) {
    crate::world_objects::world_object::world_object_set_name(
        w,
        this,
        value.trim_start_matches('+').to_owned(),
    )
}

// ---- constructors and SetEphemeralValues ----

/// The extra arguments of Player's two constructors: `(Weenie, ObjectGuid, uint accountId)` and
/// `(Biota, IEnumerable<Biota> inventory, IEnumerable<Biota> wieldedItems, Character, Session)`.
#[derive(Debug, Clone)]
pub enum PlayerCtorArgs {
    Weenie {
        account_id: u32,
    },
    Biota {
        inventory: Vec<empyrean_store::models::shard::Biota>,
        wielded_items: Vec<empyrean_store::models::shard::Biota>,
        character: Box<empyrean_store::models::shard::Character>,
        session: Option<empyrean_net::SessionId>,
    },
}

/// `new Player(weenie, guid, accountId)`, or `new Sentinel(..)` / `new Admin(..)` for those classes.
///
/// # Panics
/// When `class` is not Player, Sentinel or Admin.
pub fn player_from_weenie(
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    class: crate::dispatch::Class,
    weenie: std::sync::Arc<empyrean_entity::Weenie>,
    guid: empyrean_entity::ObjectGuid,
    account_id: u32,
) -> crate::world_objects::world_object::WorldObject {
    player_construct(
        env,
        class,
        crate::world_objects::world_object::CtorSource::Weenie(weenie, guid),
        PlayerCtorArgs::Weenie { account_id },
    )
}

/// `new Player(biota, inventory, wieldedItems, character, session)` with its `Character`: the
/// constructor `WorldManager.DoPlayerEnterWorld` calls (or the Sentinel/Admin one).
///
/// # Panics
/// When `class` is not Player, Sentinel or Admin.
pub fn player_from_biota_with_character(
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    class: crate::dispatch::Class,
    biota: empyrean_entity::Biota,
    inventory: Vec<empyrean_store::models::shard::Biota>,
    wielded_items: Vec<empyrean_store::models::shard::Biota>,
    character: empyrean_store::models::shard::Character,
    session: Option<empyrean_net::SessionId>,
) -> crate::world_objects::world_object::WorldObject {
    player_construct(
        env,
        class,
        crate::world_objects::world_object::CtorSource::Biota(biota),
        PlayerCtorArgs::Biota {
            inventory,
            wielded_items,
            character: Box::new(character),
            session,
        },
    )
}

fn player_construct(
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    class: crate::dispatch::Class,
    src: crate::world_objects::world_object::CtorSource,
    args: PlayerCtorArgs,
) -> crate::world_objects::world_object::WorldObject {
    use crate::dispatch::Class;

    let mut o = crate::world_objects::world_object::WorldObject::allocate(class);
    match class {
        Class::Player => player_ctor(&mut o, env, src, args),
        Class::Sentinel => crate::world_objects::sentinel::sentinel_ctor(&mut o, env, src, args),
        Class::Admin => crate::world_objects::admin::admin_ctor(&mut o, env, src, args),
        _ => panic!("{} is not a Player class", class.name()),
    }
    o
}

/// Player's two constructors after the `Creature` constructor.
// ACE: Player.Player
pub fn player_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
    args: PlayerCtorArgs,
) {
    crate::world_objects::creature::creature_ctor(o, env, src);

    match args {
        PlayerCtorArgs::Weenie { account_id } => {
            let character = empyrean_store::models::shard::Character {
                id: o.guid.full(),
                account_id,
                name: o
                    .get_property(empyrean_entity::enums::PropertyString::Name)
                    .unwrap_or_default(),
                ..empyrean_store::models::shard::Character::default()
            };
            let account = env.w.auth.lock().get_account_by_id(character.account_id);
            if let Some(p) = o.player.as_mut() {
                p.player.character = Some(character);
                p.player_database.character_changes_detected = true;

                p.player.account = account;
            }

            player_set_ephemeral_values(o, env, None);

            // Make sure properties this WorldObject requires are not null.
            o.set_available_experience(Some(o.available_experience().unwrap_or(0)));
            o.set_total_experience(Some(o.total_experience().unwrap_or(0)));

            o.set_attackable(true);

            o.set_property(
                empyrean_entity::enums::PropertyString::DateOfBirth,
                env.w.now.utc.format("dd MMMM yyyy"),
            );

            // `IsOlthoiPlayer`, as SetEphemeralValues has just computed it.
            if player_is_olthoi(o) {
                // `GenerateContainList();`
                // DIVERGE: the new items need guids and the world, so the list is marked here and generated by `PlayerFactory.Create` straight after this constructor, with the player in `World.objects` (V195).
                if let Some(c) = o.container.as_mut() {
                    c.container.generate_contain_list_pending = true;
                }
            } else if let Some(create_list) = o.biota.properties_create_list_mut() {
                create_list.clear();
            }
        }
        PlayerCtorArgs::Biota {
            inventory,
            wielded_items,
            character,
            session,
        } => {
            // `Character = character; Session = session;` (the session is found through
            // `Session.Player`, which `session.SetPlayer` sets right after construction).
            // `Account = DatabaseManager.Authentication.GetAccountById(Character.AccountId);`
            let account = env.w.auth.lock().get_account_by_id(character.account_id);
            if let Some(p) = o.player.as_mut() {
                p.player.character = Some(*character);
                p.player.account = account;
            }

            player_set_ephemeral_values(o, env, session);

            // `SortBiotasIntoInventory(inventory); AddBiotasToEquippedObjects(wieldedItems);
            //  UpdateCoinValue(false);`: in `player_ctor_load_possessions`, once the player is in
            // `World.objects`.
            if let Some(p) = o.player.as_mut() {
                p.player.login_possessions = Some(LoginPossessions {
                    inventory,
                    wielded_items,
                });
            }
        }
    }
}

/// The tail of the login constructor, `Player(Biota, inventory, wieldedItems, character, session)`:
/// the possessions into 4.5a's `Container.Inventory` and `Creature.EquippedObjects` (with their
/// burden, placements, rating cache and `Children`), then the coin value. The caller runs it
/// right after inserting the player into `World.objects`, before anything reads the player.
///
/// # Panics
/// When `this` is not a live player, and where ACE throws (see `Container.SortBiotasIntoInventory`).
// ACE: Player.Player
// DIVERGE: ACE runs these three calls inside the constructor (before the Sentinel/Admin
// SetEphemeralValues of a subclass); 4.5a's ports need the player and its possessions in
// World.objects, which a constructor cannot reach, so they run as soon as it is inserted (arch).
pub fn player_ctor_load_possessions(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let Some(possessions) = fields_mut(w, this).login_possessions.take() else {
        return;
    };

    // `WorldObjectFactory.CreateWorldObject(Shard.Biota)` converts each shard biota first.
    let convert =
        |biotas: Vec<empyrean_store::models::shard::Biota>| -> Vec<empyrean_entity::Biota> {
            biotas.iter().map(|b| empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(b, false)).collect()
        };

    crate::world_objects::container::sort_biotas_into_inventory(
        w,
        this,
        convert(possessions.inventory),
    );
    crate::world_objects::creature_equipment::add_biotas_to_equipped_objects(
        w,
        this,
        convert(possessions.wielded_items),
    );

    crate::world_objects::player_commerce::update_coin_value(w, this, false);
}

/// `HeritageGroup == HeritageGroup.Olthoi || HeritageGroup == HeritageGroup.OlthoiAcid`, the value
/// `SetEphemeralValues` stores in `IsOlthoiPlayer`.
fn player_is_olthoi(o: &crate::world_objects::world_object::WorldObject) -> bool {
    let heritage_group = o.heritage_group();
    heritage_group == empyrean_entity::enums::HeritageGroup::Olthoi
        || heritage_group == empyrean_entity::enums::HeritageGroup::OlthoiAcid
}

// ACE: Player.SetEphemeralValues
fn player_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    session: Option<empyrean_net::SessionId>,
) {
    use empyrean_common::dotnet::CsCast;
    use empyrean_entity::enums::{AccessLevel, HeritageGroup, MotionStance};

    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Player;

    // This is the default send upon log in and the most common. Anything with a velocity will need to add that flag.
    // This should be handled automatically...
    //PositionFlags |= PositionFlags.OrientationHasNoX | PositionFlags.OrientationHasNoY | PositionFlags.IsGrounded | PositionFlags.HasPlacementID;

    o.set_first_enter_world_done(false);

    // `SetStance(MotionStance.NonCombat, false)`: WorldObject.SetStance without its broadcast, on
    // the object under construction.
    // DIVERGE: inlined, because WorldObject.SetStance takes the World and the object is not in it
    // yet; with broadcast false its body is exactly this (arch).
    {
        let mut motion =
            crate::network::motion::movement_data::Motion::from_stance(MotionStance::NonCombat);
        if crate::managers::property_manager::get_bool(env.w, "persist_movement", false, true).item
        {
            let current =
                crate::world_objects::world_object_networking::shims::current_motion_state(o)
                    .cloned()
                    .expect("ACE: Motion.Persist(null) (NullReferenceException)");
            motion.persist(&current);
        }
        o.wo.world_object_properties.current_motion_state = Some(motion);
    }

    // radius for object updates
    o.wo.world_object.listening_radius = 5.0;

    let session_access_level = session
        .and_then(|s| env.w.sessions.get(s))
        .map(|s| s.access_level);
    if let Some(access_level) = session_access_level {
        if env
            .w
            .auth
            .lock()
            .accounts_config()
            .override_character_permissions
        {
            if access_level == AccessLevel::Admin {
                o.set_is_admin_prop(true);
            }
            if access_level == AccessLevel::Developer {
                o.set_is_arch(true);
            }
            if access_level == AccessLevel::Sentinel {
                o.set_is_sentinel_prop(true);
            }
            if access_level == AccessLevel::Envoy {
                o.set_is_envoy(true);
                o.set_is_sentinel_prop(true); //IsEnvoy is not recognized by the client and therefore the client should treat the user as a Sentinel.
            }
            if access_level == AccessLevel::Advocate {
                o.set_is_advocate(true);
            } else if o.is_advocate() && !o.advocate_quest_player() {
                // An advocate flag left from an earlier Advocate account level, not earned by the
                // advocate quest, is cleared.
                o.set_is_advocate(false);
            }
        }
    }

    let is_olthoi_player = player_is_olthoi(o);
    let is_gear_knight_player =
        crate::managers::property_manager::get_bool(env.w, "gearknight_core_plating", false, true)
            .item
            && o.heritage_group() == HeritageGroup::Gearknight;
    if let Some(p) = o.player.as_mut() {
        p.player_properties.is_olthoi_player = is_olthoi_player;
        p.player_properties.is_gear_knight_player = is_gear_knight_player;
    }

    let capacity: u8 = 7i32
        .wrapping_add(o.augmentation_extra_pack_slot())
        .cs_cast();
    o.set_container_capacity(Some(capacity));

    if let Some(access_level) = session_access_level {
        if o.advocate_quest_player() && o.is_advocate() {
            // Advocate permissions are per character regardless of override
            if access_level == AccessLevel::Player {
                // Elevate to Advocate permissions: `Session.SetAccessLevel(AccessLevel.Advocate)`,
                // applied by DoPlayerEnterWorld once the constructor returns.
                if let Some(p) = o.player.as_mut() {
                    p.player.pending_session_access_level = Some(AccessLevel::Advocate);
                }
            }
            if o.advocate_level().is_some_and(|l| l > 4) {
                o.set_is_psr(true); // Enable AdvocateTeleport via MapClick
            }
        }
    }

    // `CombatTable = DatManager.PortalDat.ReadFromDat<CombatManeuverTable>(CombatTableDID.Value);`
    let combat_table_did = o.combat_table_did().expect(
        "ACE: CombatTableDID.Value (InvalidOperationException: Nullable object must have a value)",
    );
    let combat_table = env
        .w
        .dats
        .portal_dat()
        .read_from_dat::<dereth_assets::CombatManeuverTable>(combat_table_did);
    if let Some(c) = o.creature.as_mut() {
        c.creature_combat.combat_table = combat_table;
    }

    // `_questManager = new QuestManager(this);`
    if let Some(c) = o.creature.as_mut() {
        c.creature.quest_manager =
            Some(crate::managers::quest_manager::QuestManager::new_creature());
    }

    // `ContractManager = new ContractManager(this);`
    let contract_manager = o
        .player
        .as_ref()
        .and_then(|p| p.player.character.as_ref())
        .map(|character| {
            crate::world_objects::managers::contract_manager::contract_manager_new(
                &env.w.dats,
                character,
            )
        })
        .unwrap_or_default();
    if let Some(p) = o.player.as_mut() {
        p.player_character.contract_manager = contract_manager;
    }

    // `ConfirmationManager = new ConfirmationManager(this);`
    if let Some(p) = o.player.as_mut() {
        p.player.confirmation_manager =
            crate::world_objects::managers::confirmation_manager::ConfirmationManager::new();
    }

    // `LootPermission = new Dictionary<ObjectGuid, DateTime>();`
    if let Some(p) = o.player.as_mut() {
        p.player_death.loot_permission = empyrean_common::dotnet::DotNetDict::new();
    }

    // `SquelchManager = new SquelchManager(this);`
    let squelch_manager =
        crate::world_objects::managers::squelch_manager::squelch_manager_new(env.w, o);
    if let Some(p) = o.player.as_mut() {
        p.player.squelch_manager = squelch_manager;
    }

    if let Some(p) = o.player.as_mut() {
        p.player_magic.magic_state = crate::entity::magic_state::MagicState::new();
    }

    if let Some(p) = o.player.as_mut() {
        p.player_use.food_state = crate::entity::food_state::FoodState::new(o.guid);
    }

    if let Some(p) = o.player.as_mut() {
        p.player_magic.record_cast = crate::entity::record_cast::RecordCast::new();
    }

    // `AttackQueue = new AttackQueue(this);`
    let guid = o.guid;
    if let Some(p) = o.player.as_mut() {
        p.player_melee.attack_queue = crate::entity::attack_queue::AttackQueue::new(guid);
    }

    if o.player_kills_pk().is_none() {
        o.set_player_kills_pk(Some(0));
    }
    if o.player_kills_pkl().is_none() {
        o.set_player_kills_pkl(Some(0));
    }

    // return; // todo (ACE keeps the old Load() code commented out after this point)
}

// ACE: Player.SendMessage
/// A system chat line to this player (ACE's `source` is null: only a global squelch of
/// `msg_type` drops it). `msg_type` defaults to `ChatMessageType.Broadcast` in ACE.
pub fn send_message(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    msg: &str,
    msg_type: empyrean_entity::enums::ChatMessageType,
) {
    send_message_from(w, this, msg, msg_type, None);
}

// ACE: Player.SendMessage
/// A system chat line to this player, unless it squelches `source` on a squelchable channel.
pub fn send_message_from(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    msg: &str,
    msg_type: empyrean_entity::enums::ChatMessageType,
    source: Option<empyrean_entity::ObjectGuid>,
) {
    use crate::world_objects::managers::squelch_manager;

    if squelch_manager::is_legal_channel(msg_type)
        && squelch_manager::squelches_contains(w, this, source, msg_type)
    {
        return;
    }

    let session = crate::world_objects::world_object_networking::shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let m =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            msg, msg_type,
        );
    crate::network::game_messages::game_message::enqueue_send(w, session, m);
}

// ================================================================================ logout

/// `Session.Network.EnqueueSend(msg)` for this player (a player with no session sends nothing).
fn send_to_player(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    msg: crate::network::game_messages::game_message::GameMessage,
) {
    if let Some(session) =
        crate::world_objects::world_object_networking::shims::player_session(w, this)
    {
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
    }
}

/// The `Player` data of a live player, or `None`.
fn player_data(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> Option<&crate::world_objects::kinds::PlayerData> {
    w.objects.get(this)?.player.as_deref()
}

fn player_data_mut(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> Option<&mut crate::world_objects::kinds::PlayerData> {
    w.objects.get_mut(this)?.player.as_deref_mut()
}

/// Do the player log out work. If you want to force a player to logout, use
/// `Session.LogOffPlayer()`. Answers false for a delayed player-killer log-off.
// ACE: Player.LogOut
pub fn log_out(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    client_session_terminated_abruptly: bool,
    force_immediate: bool,
) -> bool {
    use empyrean_entity::enums::{ChatMessageType, PhysicsState, PropertyBool};

    // (a player missing from the store reads as no PK timer, as the rest of the log-out tolerates it)
    let pk_logout_active = w.objects.get(this).is_some()
        && crate::world_objects::player_combat::pk_logout_active(w, this);
    if pk_logout_active && !force_immediate {
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.YouHaveBeenInPKBattleTooRecently));
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            "Beginning delayed player killer logoff...",
            ChatMessageType::Broadcast,
        );
        send_to_player(w, this, msg);

        if !player_data(w, this).is_some_and(|p| p.player.pk_logout) {
            if let Some(p) = player_data_mut(w, this) {
                p.player.pk_logout = true;
            }

            // `IsFrozen = true`
            crate::physics::phys_ext::set_physics_property_state(
                w,
                this,
                PropertyBool::IsFrozen,
                PhysicsState::Frozen,
                Some(true),
            );
            crate::world_objects::world_object::enqueue_broadcast_physics_state(w, this);

            #[allow(clippy::cast_precision_loss)]
            // `Time.GetFutureUnixTime(double)` takes the long as a double
            let pk_timer =
                crate::managers::property_manager::get_long(w, "pk_timer", 0, true).item as f64;
            let logoff_timestamp = w.now.unix_time + pk_timer;
            if let Some(o) = w.objects.get_mut(this) {
                o.set_logoff_timestamp(Some(logoff_timestamp));
            }
            crate::managers::player_manager::add_player_to_logoff_queue(w, this);
        }
        return false;
    }

    log_out_inner(w, this, client_session_terminated_abruptly);

    true
}

/// Starts the log-out: busy and logging out, into the final log-off queue, out of the fellowship,
/// trade and the Turbine chat channels (unless the session ended abruptly), the pet destroyed;
/// then `LogOut_Final` unless the player is dying.
// ACE: Player.LogOut_Inner
pub fn log_out_inner(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    client_session_terminated_abruptly: bool,
) {
    use empyrean_entity::enums::{CharacterOption, FactionBits};

    use crate::world_objects::player_networking::leave_turbine_chat_channel;
    use crate::world_objects::world_object_networking::shims::player_get_character_option;

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    o.wo.world_object.is_busy = true;
    if let Some(p) = o.player.as_mut() {
        p.player.is_logging_out = true;
    }

    crate::managers::player_manager::add_player_to_final_logoff_queue(w, this);

    if crate::world_objects::player_fellowship::fellowship(w, this).is_some() {
        crate::world_objects::player_fellowship::fellowship_quit(w, this, false);
    }

    if crate::world_objects::player_trade::is_trading(w, this)
        && crate::world_objects::player_trade::trade_partner(w, this)
            != empyrean_entity::ObjectGuid::INVALID
    {
        let trade_partner = crate::managers::player_manager::get_online_player(
            w,
            crate::world_objects::player_trade::trade_partner(w, this).full(),
        );

        if let Some(trade_partner) = trade_partner {
            crate::world_objects::player_trade::handle_action_close_trade_negotiations(
                w,
                trade_partner,
                empyrean_entity::enums::EndTradeReason::Normal,
            );
        }
    }

    if !client_session_terminated_abruptly
        && crate::managers::property_manager::get_bool(w, "use_turbine_chat", false, true).item
    {
        let is_olthoi_player =
            player_data(w, this).is_some_and(|p| p.player_properties.is_olthoi_player);
        if is_olthoi_player {
            leave_turbine_chat_channel(w, this, "Olthoi", false);
        } else {
            if player_get_character_option(w, this, CharacterOption::ListenToGeneralChat) {
                leave_turbine_chat_channel(w, this, "General", false);
            }
            if player_get_character_option(w, this, CharacterOption::ListenToTradeChat) {
                leave_turbine_chat_channel(w, this, "Trade", false);
            }
            if player_get_character_option(w, this, CharacterOption::ListenToLFGChat) {
                leave_turbine_chat_channel(w, this, "LFG", false);
            }
            if player_get_character_option(w, this, CharacterOption::ListenToRoleplayChat) {
                leave_turbine_chat_channel(w, this, "Roleplay", false);
            }
            if player_get_character_option(w, this, CharacterOption::ListenToAllegianceChat)
                && crate::world_objects::player_allegiance::i_player_allegiance(
                    w,
                    crate::entity::i_player::IPlayer::Online(this),
                )
                .is_some()
            {
                leave_turbine_chat_channel(w, this, "Allegiance", false);
            }
            let society = w
                .objects
                .get(this)
                .and_then(|o| o.faction1_bits())
                .unwrap_or(FactionBits::None);
            if player_get_character_option(w, this, CharacterOption::ListenToSocietyChat)
                && society != FactionBits::None
            {
                leave_turbine_chat_channel(w, this, "Society", false);
            }
        }
    }

    if let Some(pet) = crate::world_objects::player_use::fields(w, this)
        .current_active_pet
        .filter(|&p| w.objects.contains(p))
    {
        crate::world_objects::world_object::destroy(w, pet, true, false);
    }

    // If we're in the dying animation process, we cannot logout until that animation completes..
    if player_data(w, this).is_some_and(|p| p.player_death.is_in_death_process) {
        return;
    }

    log_out_final(w, this, false);
}

/// Plays the log-out motion, then (on the world queue, after the animation) finalizes the log-out;
/// without a landblock, or with `skip_animations`, finalizes at once.
// ACE: Player.LogOut_Final
pub fn log_out_final(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    skip_animations: bool,
) {
    use empyrean_entity::enums::{MotionCommand, MotionStance, PhysicsState, PropertyBool};

    use crate::entity::actions::action_chain::ActionChain;
    use crate::entity::actions::i_actor::Actor;

    let Some(o) = w.objects.get(this) else { return };
    let Some(current_landblock) = o.current_landblock else {
        finalize_logout(w, this);
        return;
    };

    if skip_animations {
        finalize_logout(w, this);
        return;
    }

    if o.get_property(PropertyBool::IsFrozen).unwrap_or(false) {
        // `IsFrozen = false`
        crate::physics::phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::IsFrozen,
            PhysicsState::Frozen,
            Some(false),
        );
    }

    crate::world_objects::world_object::enqueue_broadcast_physics_state(w, this);

    let motion_command = MotionCommand::LogOut;
    // `var motion = new Motion(this, motionCommand);` is never used.
    let stance_non_combat = MotionStance::NonCombat;
    let motion_table_id = w.objects.get(this).map_or(
        0,
        crate::world_objects::world_object::WorldObject::motion_table_id,
    );
    let anim_length = crate::physics::motion_table::get_animation_length(
        w,
        motion_table_id,
        stance_non_combat,
        motion_command,
        1.0,
    );

    let mut logout_chain = ActionChain::new();

    logout_chain.add_action(Actor::Object(this), move |w| {
        player_send_motion_as_commands(w, this, motion_command, stance_non_combat)
    });
    logout_chain.add_delay_seconds(w, f64::from(anim_length));

    // remove the player from landblock management -- after the animation has run
    logout_chain.add_action(Actor::World, move |w| {
        // If we're in the dying animation process, we cannot RemoveWorldObject and logout until that animation completes..
        if player_data(w, this).is_some_and(|p| p.player_death.is_in_death_process) {
            return;
        }

        finalize_logout(w, this);
    });

    // close any open landblock containers (chests / corpses)
    let last_opened_container_id = player_last_opened_container_id(w, this);
    if last_opened_container_id != empyrean_entity::ObjectGuid::default() {
        let container = crate::entity::landblock::get_object(
            w,
            current_landblock,
            last_opened_container_id,
            true,
        )
        .filter(|c| {
            w.objects
                .get(*c)
                .is_some_and(crate::world_objects::world_object::WorldObject::is_container)
        });

        if let Some(container) = container {
            crate::dispatch::close::close(w, container, this);
        }
    }

    logout_chain.enqueue_chain(w);
}

/// Force Log off a player requested to log out by an admin command forcelogoff/forcelogout or the
/// ServerManager. THIS FUNCTION FOR SYSTEM USE ONLY; If you want to force a player to logout, use
/// `Session.LogOffPlayer()`.
// ACE: Player.ForceLogoff
pub fn force_logoff(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    if !player_data(w, this).is_some_and(|p| p.player.forced_log_off_requested) {
        return;
    }

    let account = player_data(w, this)
        .and_then(|p| p.player.account.as_ref())
        .map(|a| a.account_name.clone())
        .unwrap_or_default();
    let name = w
        .objects
        .get(this)
        .and_then(|o| o.get_property(empyrean_entity::enums::PropertyString::Name))
        .unwrap_or_default();
    // DIVERGE: the time is printed in UTC (ACE: `DateTime.Now`, local time); the clock has no zone.
    log::warn!("[LOGOUT] Executing ForcedLogoff for Account {account} with character {name} (0x{this}) at {}.", empyrean_common::extensions::date_time_extensions::to_common_string(w.now.utc));

    finalize_logout(w, this);

    if let Some(p) = player_data_mut(w, this) {
        p.player.forced_log_off_requested = false;
    }
}

/// The end of a log-out: out of the final log-off queue and the landblock, the log-out
/// properties, the full save (`SavePlayerToDatabase`), then offline.
// ACE: Player.FinalizeLogout
///
/// Not ACE's (fix, V323): this save is not the last word. A blow or a death
/// that lands after it (a monster's strike already queued, a death under way as the log-off ends)
/// reaches the offline player with the player's final biota ([`release_logged_off_player`]), which
/// is then marked for the offline players' save. In ACE it changed the in-memory biota only, and
/// NumDeaths, Killer, the vitals and the rest were lost at the next restart.
pub fn finalize_logout(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    crate::managers::player_manager::remove_player_from_final_logoff_queue(w, this);
    if let Some(current_landblock) = w.objects.get(this).and_then(|o| o.current_landblock) {
        crate::entity::landblock::remove_world_object(
            w,
            current_landblock,
            this,
            false,
            false,
            true,
        );
    }
    crate::world_objects::player_database::set_properties_at_log_out(w, this);
    crate::world_objects::player_database::save_player_to_database(w, this);
    crate::managers::player_manager::switch_player_from_online_to_offline(w, this);

    let account = player_data(w, this)
        .and_then(|p| p.player.account.as_ref())
        .map(|a| a.account_name.clone())
        .unwrap_or_default();
    let name = w
        .objects
        .get(this)
        .and_then(|o| o.get_property(empyrean_entity::enums::PropertyString::Name))
        .unwrap_or_default();
    // DIVERGE: the time is printed in UTC (ACE: `DateTime.Now`, local time); the clock has no zone.
    log::debug!(
        "[LOGOUT] Account {account} exited the world with character {name} (0x{this}) at {}.",
        empyrean_common::extensions::date_time_extensions::to_common_string(w.now.utc)
    );
}

/// Not ACE: drops a logged-off player and its possessions from `World.objects` once nothing
/// holds it any more (its session let go of it, or is gone). In ACE the garbage collector takes
/// the `Player` then; here the store owns every object, and a later enter-world builds a new
/// `Player` with the same guid. A player still online or still on a landblock is kept.
pub fn release_logged_off_player(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let Some(o) = w.objects.get(this) else { return };
    if o.current_landblock.is_some()
        || crate::managers::player_manager::get_online_player(w, this.full()).is_some()
    {
        return;
    }

    // DIVERGE: ACE's `OfflinePlayer` shares the `Player`'s biota object, so what changes on the
    // player after `SwitchPlayerFromOnlineToOffline` (the log-off's last writes, a blow or a death
    // landing during the log-off) is what the next `PlayerEnterWorld` builds from. The offline
    // player holds a copy here, so it takes the player's final biota as the player leaves the
    // store.
    //
    // Not ACE's (fix, V323): the final biota is marked for the offline
    // players' save, so what landed after the log-off's save reaches the shard.
    let biota = o.biota.clone();
    if let Some(offline) = w.player_manager.offline_players.get_mut(&this.full()) {
        offline.biota = biota;
        offline.changes_detected = true;
    }

    let mut guids = crate::world_objects::player_database::player_get_all_possessions(w, this);
    guids.push(this);
    for guid in guids {
        if let Some(h) = crate::physics::phys_ext::physics_obj(w, guid) {
            crate::physics::phys_ext::destroy_object(w, h);
        }
        w.objects.remove(guid);
    }
}

/// `SendMotionAsCommands(motionCommand, motionStance)` (`Player_Location.cs`).
fn player_send_motion_as_commands(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    motion_command: empyrean_entity::enums::MotionCommand,
    motion_stance: empyrean_entity::enums::MotionStance,
) {
    crate::world_objects::player_location::send_motion_as_commands(
        w,
        this,
        motion_command,
        motion_stance,
    );
}

/// `LastOpenedContainerId` (`Player_Use.cs` field).
fn player_last_opened_container_id(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::ObjectGuid {
    crate::world_objects::player_use::fields(w, this).last_opened_container_id
}

// ================================================================================ session, appraisal and misc

/// The player's session (`Player.Session`).
///
/// # Panics
/// Without one (ACE: `NullReferenceException`).
fn session_of(w: &crate::World, this: empyrean_entity::ObjectGuid) -> empyrean_net::SessionId {
    crate::world_objects::world_object_networking::shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

// ACE: Player.MaxRadarRange_Indoors
pub const MAX_RADAR_RANGE_INDOORS: f32 = 25.0;
// ACE: Player.MaxRadarRange_Outdoors
pub const MAX_RADAR_RANGE_OUTDOORS: f32 = 75.0;

// ACE: Player.CurrentRadarRange
/// `Location.Indoors ? MaxRadarRange_Indoors : MaxRadarRange_Outdoors`.
#[must_use]
pub fn current_radar_range(w: &crate::World, this: empyrean_entity::ObjectGuid) -> f32 {
    let location = chat::object(w, this)
        .location()
        .expect("ACE: Location is null (NullReferenceException)");
    if location.indoors() {
        MAX_RADAR_RANGE_INDOORS
    } else {
        MAX_RADAR_RANGE_OUTDOORS
    }
}

// ACE: Player.IsDeleted
/// `Character.IsDeleted`.
///
/// # Panics
/// Without a `Character` (ACE: `NullReferenceException`).
#[must_use]
pub fn is_deleted(w: &crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    let o = chat::object(w, this);
    crate::world_objects::world_object_networking::shims::player_character(o)
        .expect("ACE: Player.Character is null (NullReferenceException)")
        .is_deleted
}

// ACE: Player.IsPendingDeletion
/// `Character.DeleteTime > 0 && !IsDeleted`.
///
/// # Panics
/// Without a `Character` (ACE: `NullReferenceException`).
#[must_use]
pub fn is_pending_deletion(w: &crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    let o = chat::object(w, this);
    let character = crate::world_objects::world_object_networking::shims::player_character(o)
        .expect("ACE: Player.Character is null (NullReferenceException)");
    character.delete_time > 0 && !is_deleted(w, this)
}

// ACE: Player.Examine
/// The appraisal roll: for a creature, this player's Assess Person/Creature skill against its
/// Deception; the response and `OnAppraisal`.
pub fn examine(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    obj: empyrean_entity::ObjectGuid,
) {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    use empyrean_entity::enums::{
        CharacterOption, CloakStatus, PropertyAttribute, Skill, SkillAdvancementClass,
    };

    //Console.WriteLine($"{Name}.Examine({obj.Name})");

    let mut success = true;
    let is_creature = chat::object(w, obj).is_creature();
    let is_player = chat::object(w, obj).is_player();

    if is_creature {
        let skill = if is_player {
            Skill::AssessPerson
        } else {
            Skill::AssessCreature
        };

        let mut current_skill: i32 = empyrean_common::dotnet::CsCast::cs_cast(
            crate::world_objects::world_object_magic::creature_get_creature_skill_current(
                w, this, skill,
            ),
        );
        let difficulty: i32 = empyrean_common::dotnet::CsCast::cs_cast(
            crate::world_objects::world_object_magic::creature_get_creature_skill_current(
                w,
                obj,
                Skill::Deception,
            ),
        );

        if crate::managers::property_manager::get_bool(w, "assess_creature_mod", false, true).item
            && skill == Skill::AssessCreature
        {
            let o = chat::object(w, this);
            let advancement_class = o
                .skills()
                .get(&Skill::AssessCreature)
                .expect("KeyNotFoundException: Skills[AssessCreature]")
                .advancement_class(o);
            if advancement_class < SkillAdvancementClass::Trained {
                let focus = crate::world_objects::creature_combat::attribute_current(
                    w,
                    this,
                    PropertyAttribute::Focus,
                );
                let self_ = crate::world_objects::creature_combat::attribute_current(
                    w,
                    this,
                    PropertyAttribute::Self_,
                );
                current_skill =
                    empyrean_common::dotnet::CsCast::cs_cast(focus.wrapping_add(self_) / 2);
            }
        }

        let mut chance =
            crate::world_objects::skill_check::get_skill_chance(current_skill, difficulty, 0.03);

        if difficulty == 0
            || (is_player && obj == this)
            || (is_player
                && !crate::world_objects::player_character::get_character_option(
                    w,
                    obj,
                    CharacterOption::AttemptToDeceiveOtherPlayers,
                ))
        {
            chance = 1.0;
        }

        let o = chat::object(w, this);
        if (o.is_admin() || o.is_sentinel()) && o.cloak_status() == CloakStatus::On {
            chance = 1.0;
        }

        success = chance > ThreadSafeRandom::next_float(0.0, 1.0);
    }

    let o = chat::object(w, obj);
    if o.resist_item_appraisal().is_some_and(|r| r >= 999) {
        success = false;
    }

    if o.is_pet() || o.is_combat_pet() {
        success = true;
    }

    if success {
        if let Some(p) = w.objects.get_mut(this) {
            p.set_current_appraisal_target(Some(obj.full()));
        }
    }

    let session = session_of(w, this);
    let msg = crate::network::game_event::events::game_event_identify_object_response::game_event_identify_object_response(w, session, obj, success);
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);

    on_appraisal(w, this, obj, success);
}

// ACE: Player.OnAppraisal
/// A failed appraisal of a player tells it; an idle monster that tolerates nothing on appraisal
/// attacks.
pub fn on_appraisal(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    obj: empyrean_entity::ObjectGuid,
    success: bool,
) {
    use empyrean_entity::enums::{ChatMessageType, Tolerance};

    let is_player = chat::object(w, obj).is_player();
    if !success
        && is_player
        && !crate::world_objects::managers::squelch_manager::squelches_contains(
            w,
            obj,
            Some(this),
            ChatMessageType::Appraisal,
        )
    {
        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!("{name} tried and failed to assess you!"),
            ChatMessageType::Appraisal,
        );
        let session = session_of(w, obj);
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
    }

    // pooky logic - handle monsters attacking on appraisal
    if chat::object(w, obj).is_creature()
        && crate::world_objects::monster::monster_state(w, obj)
            == crate::world_objects::monster::State::Idle
        && (chat::object(w, obj).tolerance() & Tolerance::Appraise) == Tolerance::Appraise
    {
        crate::world_objects::monster_combat::set_attack_target(w, obj, Some(this));
        crate::world_objects::monster_awareness::wake_up(w, obj, true);
    }
}

// ACE: Player.UpdateSelectedTarget
fn update_selected_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: Option<empyrean_entity::ObjectGuid>,
) {
    if let Some(selected) = crate::world_objects::player_tracking::selected_target(w, this) {
        // `selectedTarget.TryGetWorldObject() as Creature`
        if w.objects
            .get(selected)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
        {
            crate::world_objects::creature::on_target_deselected(w, selected, this);
        }
    }

    if let Some(target) = target {
        // `selectedTarget = new WorldObjectInfo(target)`
        let info = crate::entity::world_object_info::WorldObjectInfo::new(w, target);
        crate::world_objects::player_tracking::set_selected_target(w, this, Some(info));
        if let Some(o) = w.objects.get_mut(this) {
            o.set_health_query_target(Some(target.full()));
        }

        crate::world_objects::creature::on_target_selected(w, target, this);
    } else {
        crate::world_objects::player_tracking::set_selected_target(w, this, None);
        if let Some(o) = w.objects.get_mut(this) {
            o.set_health_query_target(None);
        }
    }
}

// ACE: Player.ActionBroadcastKill
/// Sends a death message broadcast all players on the landblock? that a killer has a victim.
pub fn action_broadcast_kill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    death_message: &str,
    victim_id: empyrean_entity::ObjectGuid,
    killer_id: empyrean_entity::ObjectGuid,
) {
    let death_broadcast = crate::network::game_messages::messages::game_message_player_killed::game_message_player_killed(death_message, victim_id, killer_id);

    // OutdoorChatRange?
    crate::world_objects::world_object_networking::enqueue_broadcast(
        w,
        this,
        true,
        &[death_broadcast],
    );
}

// ACE: Player.PlaySound
/// Emits a sound at location `source_id` and volume (ACE's default 1.0). The client will perform
/// sound attenuation / volume adjustment based on the listener distance from the origin.
pub fn play_sound(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    sound: empyrean_entity::enums::Sound,
    source_id: empyrean_entity::ObjectGuid,
    volume: f32,
) {
    let session = session_of(w, this);
    let msg = crate::network::game_messages::messages::game_message_sound::game_message_sound(
        source_id, sound, volume,
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: Player.HandleMRT
/// Toggles `IgnoreHouseBarriers` (broadcast) and says so.
pub fn handle_mrt(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use empyrean_entity::enums::{ChatMessageType, PropertyBool};

    // This requires the Admin flag set on ObjectDescriptionFlags
    // I would expect this flag to be set in Admin.cs which would be a subclass of Player
    // FIXME: maybe move to Admin class?
    // TODO: reevaluate class location

    // The EnqueueBroadcastUpdateObject below sends the player back into teleport. I assume at this point, this was never done to players
    // EnqueueBroadcastUpdateObject();

    let ignore_house_barriers = chat::object(w, this).ignore_house_barriers();
    crate::world_objects::player_properties::update_property_bool(
        w,
        this,
        this,
        PropertyBool::IgnoreHouseBarriers,
        Some(!ignore_house_barriers),
        true,
    );

    let now = chat::object(w, this).ignore_house_barriers();
    let msg =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!(
                "Bypass Housing Barriers now set to: {}",
                if now { "True" } else { "False" }
            ),
            ChatMessageType::Broadcast,
        );
    let session = session_of(w, this);
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: Player.SendAutonomousPosition
/// Empty in ACE (`GameMessageAutonomousPosition` commented out).
pub fn send_autonomous_position(_w: &mut crate::World, _this: empyrean_entity::ObjectGuid) {
    // Session.Network.EnqueueSend(new GameMessageAutonomousPosition(this));
}

// ACE: Player.HandleActionApplySoundEffect
pub fn handle_action_apply_sound_effect(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    sound: empyrean_entity::enums::Sound,
) {
    play_sound(w, this, sound, this, 1.0);
}

// ACE: Player.OnExhausted
/// Called when the Player's stamina has recently changed to 0.
pub fn on_exhausted(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    // adjust player speed if they are currently pressing movement keys
    handle_run_rate_update(w, this);

    let session = session_of(w, this);
    let msg = crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string(
        crate::network::game_event::game_event_message::session_data(w, session),
        "You're Exhausted!",
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: Player.HandleRunRateUpdate
/// Detects changes in the player's RunRate: if there are changes, re-broadcasts the player
/// movement packet.
///
pub fn handle_run_rate_update(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    use crate::network::motion::movement_data::MovementData;
    use empyrean_entity::enums::MovementType;

    //Console.WriteLine($"{Name}.HandleRunRateUpdates()");

    let current = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .wo
        .world_object_properties
        .current_movement_data
        .clone();

    let Some(prev_invalid) = current
        .invalid
        .as_ref()
        .filter(|_| current.movement_type == MovementType::Invalid)
    else {
        return false;
    };

    let prev_state = prev_invalid.state.clone();

    let move_to_state = crate::world_objects::player_move::fields(w, this)
        .current_move_to_state
        .clone();
    let movement_data = MovementData::from_move_to_state(w, this, &move_to_state);
    let current_state = movement_data
        .invalid
        .as_ref()
        .expect("ACE: Invalid is set for a MoveToState")
        .state
        .clone();

    #[allow(clippy::float_cmp)]
    let changed = current_state.forward_speed != prev_state.forward_speed
        || current_state.turn_speed != prev_state.turn_speed
        || current_state.sidestep_speed != prev_state.sidestep_speed;

    if !changed {
        return false;
    }

    //Console.WriteLine($"Old: {prevState.ForwardSpeed}, New: {currentState.ForwardSpeed}");

    if !prev_invalid.state.has_movement() || is_jumping(w, this) {
        return false;
    }

    //Console.WriteLine($"{Name}.OnRunRateChanged()");

    let mut current_movement_data = MovementData::from_move_to_state(w, this, &move_to_state);

    // verify - forced commands from server should be non-autonomous, but could have been sent as autonomous in retail?
    // if set to autonomous here, the desired effect doesn't happen
    current_movement_data.is_autonomous = false;

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let movement_event = crate::network::game_messages::messages::game_message_update_motion::game_message_update_motion(o, &current_movement_data);
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_properties.current_movement_data = current_movement_data;
    }
    crate::world_objects::world_object_networking::enqueue_broadcast(
        w,
        this,
        true,
        &[movement_event],
    ); // broadcast to all players, including self

    true
}

// ACE: Player.HandleAdminvisionToggle
/// `choice`: -1 check, 0 off, 1 on, 2 toggle.
pub fn handle_adminvision_toggle(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    choice: i32,
) {
    use crate::physics::{object_maint, phys_ext};
    use empyrean_entity::enums::ChatMessageType;

    let old_state = fields(w, this).adminvision;

    match choice {
        0 => fields_mut(w, this).adminvision = false,
        1 => fields_mut(w, this).adminvision = true,
        2 => {
            let a = fields(w, this).adminvision;
            fields_mut(w, this).adminvision = !a;
        }
        _ => {} // -1: Do nothing
    }

    let adminvision = fields(w, this).adminvision;

    // send CO network messages for admin objects
    if adminvision && old_state != adminvision {
        let h = phys_ext::physics_obj(w, this)
            .expect("ACE: PhysicsObj is null (NullReferenceException)");
        let world_object_of = |w: &crate::World, o: dereth_physics::PhysHandle| {
            phys_ext::weenie_obj(w, o).world_object(w)
        };

        let admin_objs = object_maint::get_known_objects_values_where(w, h, |o| {
            world_object_of(w, o)
                .and_then(|g| w.objects.get(g))
                .is_some_and(crate::world_objects::world_object::WorldObject::visibility)
        });
        phys_ext::enqueue_objs(w, h, &admin_objs);

        let nodraw_objs = object_maint::get_known_objects_values_where(w, h, |o| {
            world_object_of(w, o)
                .and_then(|g| w.objects.get(g))
                .is_some_and(|wo| wo.no_draw().unwrap_or(false) || wo.ui_hidden())
        });

        let session = session_of(w, this);
        for o in nodraw_objs {
            let Some(wo) = world_object_of(w, o) else {
                continue;
            };
            let msg = crate::network::game_messages::messages::game_message_update_object::game_message_update_object(w, wo, adminvision, adminvision);
            crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        }

        // sending DO network messages for /adminvision off here doesn't work in client unfortunately?
    }

    let state = if adminvision { "enabled" } else { "disabled" };
    let session = session_of(w, this);
    let msg =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            &format!("Admin Vision is {state}."),
            ChatMessageType::Broadcast,
        );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);

    if old_state != adminvision && !adminvision {
        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            "Note that you will need to log out and back in before the visible items become invisible again.",
            ChatMessageType::Broadcast,
        );
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
    }
}
