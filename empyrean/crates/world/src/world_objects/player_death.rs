// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Death.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Death.cs`.
//!
//! Members are free functions over `(w, this)`; `this` must be a Player with a session (ACE
//! dereferences `Player.Session` without a check). Members ported in other files are
//! pointers at the end of the file; the inventory ones are `SHIM:`s over the item's
//! `ContainerId`/`WielderId` (see `creature_death`).

use empyrean_common::dotnet::{
    format as dotnet_format, CsCast, DotNetDateTime, DotNetDict, TimeSpan,
};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    BondedStatus, CharacterOption, ChatMessageType, CombatMode, DamageType, EnchantmentMask,
    PKLevel, PlayScript, PlayerKillerStatus, PropertyInt, SpellId, Vital, WeenieError, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::death_item::DeathItems;
use crate::entity::death_message::{string_format, DeathMessage};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_magic_purge_bad_enchantments::game_event_magic_purge_bad_enchantments;
use crate::network::game_event::events::game_event_magic_purge_enchantments::game_event_magic_purge_enchantments;
use crate::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use crate::network::game_event::events::game_event_victim_notification::game_event_victim_notification;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_player_killed::game_message_player_killed;
use crate::network::game_messages::messages::game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_set_stack_size::game_message_set_stack_size;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::enchantment::enchantment_new;
use crate::world_objects::creature_death::{self, name_of};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::player_skills::{send, session};
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Player_Death.cs`.
#[derive(Debug, Default)]
pub struct PlayerDeathFields {
    // ACE: Player.LootPermission
    /// A list of players who have granted corpse looting permissions with /permit (the `Player`
    /// constructor makes it empty).
    pub loot_permission: DotNetDict<ObjectGuid, DotNetDateTime>,

    // ACE: Player.IsInDeathProcess
    pub is_in_death_process: bool,

    // ACE: Player.suicideInProgress
    pub suicide_in_progress: bool,
}

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

fn fields(w: &World, this: ObjectGuid) -> &PlayerDeathFields {
    &obj(w, this)
        .player
        .as_ref()
        .expect("Player_Death on an object that is not a Player")
        .player_death
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerDeathFields {
    &mut obj_mut(w, this)
        .player
        .as_mut()
        .expect("Player_Death on an object that is not a Player")
        .player_death
}

fn chat(msg: &str) -> GameMessage {
    game_message_system_chat(msg, ChatMessageType::Broadcast)
}

/// `new GameEventWeenieError(Session, error)`, sent.
fn send_weenie_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    let s = session(w, this);
    let data = w
        .sessions
        .get_mut(s)
        .expect("NullReferenceException: Player.Session");
    let msg = game_event_weenie_error(data, error);
    enqueue_send(w, s, msg);
}

// ACE: Player.OnDeath
/// Called when a player dies, in conjunction with Die(): the PK broadcast, the base death
/// (XP, kill tasks, message), then the victim notification and the nearby broadcast.
pub fn player_on_death(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<DamageHistoryInfo>,
    damage_type: DamageType,
    critical_hit: bool,
) -> DeathMessage {
    let top_damager = damage_history::of(w, this).get_top_damager(false);

    handle_pk_death_broadcast(w, this, last_damager.as_ref(), top_damager.as_ref());

    let death_message =
        creature_death::creature_on_death(w, this, last_damager.clone(), damage_type, critical_hit);

    let last_damager_obj = last_damager.as_ref().and_then(|l| l.try_get_attacker(w));

    if let Some(last_damager_obj) = last_damager_obj {
        emote_manager_on_kill(w, last_damager_obj, this);
    }

    let name = name_of(w, this);
    let last_damager_name = last_damager
        .as_ref()
        .map(|l| l.name.clone().unwrap_or_default());

    let player_msg = match &last_damager_name {
        Some(killer) => string_format(death_message.victim, &[&name, killer]),
        None => death_message.victim.to_owned(),
    };

    let s = session(w, this);
    let data = w
        .sessions
        .get_mut(s)
        .expect("NullReferenceException: Player.Session");
    let msg_your_death = game_event_victim_notification(data, &player_msg);
    enqueue_send(w, s, msg_your_death);

    // broadcast to nearby players
    let nearby_msg = match &last_damager_name {
        Some(killer) => string_format(death_message.broadcast, &[&name, killer]),
        None => death_message.broadcast.to_owned(),
    };

    let broadcast_msg = game_message_player_killed(
        &nearby_msg,
        this,
        last_damager
            .as_ref()
            .map_or(ObjectGuid::INVALID, |l| l.guid),
    );

    log::info!("[CORPSE] {nearby_msg}");

    let mut exclude_players: Vec<ObjectGuid> = Vec::new();

    let nearby_players =
        world_object_enqueue_broadcast_excluding(w, this, &exclude_players, true, &broadcast_msg);

    exclude_players.extend(nearby_players);

    if creature_death::player_has_fellowship(w, this) {
        fellowship_on_death(w, this);
    }

    // if the player's lifestone is in a different landblock, also broadcast their demise to that landblock
    let (sanctuary, location) = {
        let o = obj(w, this);
        (o.sanctuary(), o.location())
    };
    if crate::managers::property_manager::get_bool(w, "lifestone_broadcast_death", false, true).item
    {
        if let Some(sanctuary) = sanctuary {
            let location = location.expect("NullReferenceException: Player.Location");
            if location.landblock() != sanctuary.landblock() {
                // ActionBroadcastKill might not work if other players around lifestone aren't aware of this player yet...
                // instead, we get all of the players in the lifestone landblock + adjacent landblocks,
                // and possibly limit that to some radius around the landblock?
                landblock_enqueue_broadcast_at_lifestone(
                    w,
                    this,
                    &exclude_players,
                    &sanctuary,
                    broadcast_msg,
                );
            }
        }
    }

    death_message
}

// ACE: Player.HandlePKDeathBroadcast
/// A PK death stamps the killer and tells every player online; a PK-lite death counts it.
///
/// # Panics
/// On a PK death with a null `last_damager` (ACE: `NullReferenceException` on its Name).
pub fn handle_pk_death_broadcast(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<&DamageHistoryInfo>,
    top_damager: Option<&DamageHistoryInfo>,
) {
    let Some(top_damager) = top_damager.filter(|t| t.is_player()) else {
        return;
    };

    let Some(pk_player) = top_damager
        .try_get_attacker(w)
        .filter(|g| obj(w, *g).is_player())
    else {
        return;
    };

    if crate::world_objects::player_combat::is_pk_death(w, this, Some(top_damager.guid.full())) {
        let now = w.now.unix_time;
        let o = obj_mut(w, pk_player);
        o.set_pk_timestamp(now);
        let kills = o.player_kills_pk().map(|k| k.wrapping_add(1));
        o.set_player_kills_pk(kills);

        let last_damager = last_damager.expect("NullReferenceException: lastDamager.Name");
        let mut global_pk_de = format!(
            "{} has defeated {}!",
            last_damager.name.as_deref().unwrap_or(""),
            name_of(w, this)
        );

        let location = obj(w, this)
            .location()
            .expect("NullReferenceException: Player.Location");
        if (location.cell() & 0xFFFF) < 0x100 {
            let coords = crate::entity::position_extensions::get_map_coord_str(&location)
                .unwrap_or_default();
            global_pk_de += &format!(" The kill occured at {coords}");
        }

        global_pk_de += "\n[PKDe]";

        player_manager::broadcast_to_all(w, &chat(&global_pk_de));
    } else if crate::world_objects::player_combat::is_pk_lite_death(
        w,
        this,
        Some(top_damager.guid.full()),
    ) {
        let o = obj_mut(w, pk_player);
        let kills = o.player_kills_pkl().map(|k| k.wrapping_add(1));
        o.set_player_kills_pkl(kills);
    }
}

// ACE: Player.InflictVitaePenalty
/// Inflicts vitae: records the death level, resets the vitae XP pool, adds 5% vitae and tells the
/// client. `amount` is unused in ACE.
pub fn inflict_vitae_penalty(w: &mut World, this: ObjectGuid, _amount: i32) {
    {
        let o = obj_mut(w, this);
        let level = o.level();
        o.set_death_level(level); // for calculating vitae XP
        o.set_vitae_cp_pool(Some(0)); // reset vitae XP earned
    }

    let o = obj_mut(w, this);
    let death_level = o.death_level().unwrap_or(0);
    let msg_death_level =
        game_message_private_update_property_int(o, PropertyInt::DeathLevel, death_level);
    let vitae_cp_pool = o
        .vitae_cp_pool()
        .expect("InvalidOperationException: VitaeCpPool.Value");
    let msg_vitae_cp_pool =
        game_message_private_update_property_int(o, PropertyInt::VitaeCpPool, vitae_cp_pool);

    send(w, this, [msg_death_level, msg_vitae_cp_pool]);

    let vitae = emc::update_vitae(w, this);

    let spell_id: u32 = SpellId::Vitae.0.cs_cast();
    let spell = crate::entity::spell::Spell::new(w, spell_id, true);
    let mask = EnchantmentMask(spell.stat_mod_type().0);
    let vitae_enchantment = enchantment_new(w, this, this.full(), spell_id, 0, mask, Some(vitae));
    let s = session(w, this);
    let data = w
        .sessions
        .get_mut(s)
        .expect("NullReferenceException: Player.Session");
    let msg = game_event_magic_update_enchantment(data, &vitae_enchantment);
    enqueue_send(w, s, msg);
}

// ACE: Player.Die
/// Broadcasts the player death animation, updates vitae, and sends network messages for player
/// death. Queues the action to call TeleportOnDeath and enter portal space soon.
pub fn player_die(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<DamageHistoryInfo>,
    top_damager: Option<DamageHistoryInfo>,
) {
    let mut top_damager = top_damager;
    fields_mut(w, this).is_in_death_process = true;

    if top_damager.as_ref().is_some_and(|t| t.guid == this)
        && crate::world_objects::player_combat::is_pk_type(w, this)
    {
        let top_damager_other = damage_history::of(w, this).get_top_damager(false);

        if let Some(other) = top_damager_other.filter(DamageHistoryInfo::is_player) {
            top_damager = Some(other);
        }
    }

    let health = obj(w, this).health();
    dispatch::update_vital::update_vital(w, this, health, 0);
    {
        let o = obj_mut(w, this);
        let deaths = o.num_deaths().wrapping_add(1);
        o.set_num_deaths(deaths);
    }
    fields_mut(w, this).suicide_in_progress = false;

    // todo: since we are going to be using 'time since Player last died to an OlthoiPlayer'
    // as a factor in slag generation, this will eventually be moved to after the slag generation

    //if (topDamager != null && topDamager.IsOlthoiPlayer)
    //OlthoiLootTimestamp = (int)Time.GetUnixTime();

    if creature_combat_mode(w, this) == CombatMode::Magic && magic_state_is_casting(w, this) {
        player_fail_cast(w, this, false);
    }

    // TODO: instead of setting IsBusy here,
    // eventually all of the places that check for states such as IsBusy || Teleporting
    // might want to use a common function, and IsDead should return a separate error
    player_set_is_busy(w, this, true);

    // killer = top damager for looting rights
    if let Some(top_damager) = &top_damager {
        obj_mut(w, this).set_killer_id(Some(top_damager.guid.full()));
    }

    // broadcast death animation
    // `var deathAnim = new Motion(MotionStance.NonCombat, MotionCommand.Dead); EnqueueBroadcastMotion(deathAnim);`
    creature_enqueue_broadcast_motion_dead(w, this);

    // create network messages for player death
    let o = obj_mut(w, this);
    let msg_health_update = game_message_private_update_attribute2nd_level(o, Vital::Health, 0);

    // TODO: death sounds? seems to play automatically in client
    // var msgDeathSound = new GameMessageSound(Guid, Sound.Death1, 1.0f);
    let num_deaths = o.num_deaths();
    let msg_num_deaths =
        game_message_private_update_property_int(o, PropertyInt::NumDeaths, num_deaths);

    // send network messages for player death
    send(w, this, [msg_health_update, msg_num_deaths]);

    if last_damager.as_ref().is_some_and(|l| l.guid == this) {
        // suicide
        send_weenie_error(w, this, WeenieError::YouKilledYourself);
    }

    let had_vitae = emc::has_vitae(w, this);

    let top_damager_guid = top_damager.as_ref().map(|t| t.guid.full());

    // update vitae
    // players who died in a PKLite fight do not accrue vitae
    if !crate::world_objects::player_combat::is_pk_lite_death(w, this, top_damager_guid) {
        inflict_vitae_penalty(w, this, 5);
    }

    if crate::world_objects::player_combat::is_pk_death(w, this, top_damager_guid)
        || obj(w, this).augmentation_spells_remain_past_death() == 0
    {
        let s = session(w, this);
        let data = w
            .sessions
            .get_mut(s)
            .expect("NullReferenceException: Player.Session");
        let msg_purge_enchantments = game_event_magic_purge_enchantments(data);
        emc::remove_all_enchantments(w, this);
        send(w, this, [msg_purge_enchantments]);
    } else {
        let s = session(w, this);
        let data = w
            .sessions
            .get_mut(s)
            .expect("NullReferenceException: Player.Session");
        let msg_purge_bad_enchantments = game_event_magic_purge_bad_enchantments(data);
        emc::remove_all_bad_enchantments(w, this);
        send(
            w,
            this,
            [msg_purge_bad_enchantments, chat("Your augmentation prevents the tides of death from ripping away your current enchantments!")],
        );
    }

    // wait for the death animation to finish
    let mut die_chain = ActionChain::new();
    let anim_length = motion_table_get_animation_length_dead(w, this);
    die_chain.add_delay_seconds(w, f64::from(anim_length + 1.0f32));

    die_chain.add_action(Actor::Object(this), move |w| {
        creature_death::create_corpse(w, this, top_damager.as_ref(), had_vitae);

        thread_safe_teleport_on_death(w, this); // enter portal space

        if crate::world_objects::player_combat::is_pk_death(w, this, top_damager_guid)
            || crate::world_objects::player_combat::is_pk_lite_death(w, this, top_damager_guid)
        {
            set_minimum_time_since_pk(w, this);
        }

        player_set_is_busy(w, this, false);
    });

    die_chain.enqueue_chain(w);
}

// ACE: Player.ThreadSafeTeleportOnDeath
/// Called when the player enters portal space after dying: teleports to the lifestone (or the
/// instantiation point, or where they are), then restores 75% vitals after 3 s.
pub fn thread_safe_teleport_on_death(w: &mut World, this: ObjectGuid) {
    // teleport to sanctuary or best location
    let new_position = {
        let o = obj(w, this);
        o.sanctuary()
            .or_else(|| o.instantiation())
            .or_else(|| o.location())
    }
    .expect("NullReferenceException: ThreadSafeTeleport(null position)");

    let callback = Action::delegate(move |w| {
        // Stand back up
        player_set_combat_mode(w, this, CombatMode::NonCombat);

        set_lifestone_protection(w, this);

        let mut teleport_chain = ActionChain::new();
        if !player_is_logging_out(w, this) {
            // If we're in the process of logging out, we skip the delay
            teleport_chain.add_delay_seconds(w, f64::from(3.0f32));
        }
        teleport_chain.add_action(Actor::Object(this), move |w| {
            // currently happens while in portal space
            let (new_health, new_stamina, new_mana) = {
                let o = obj(w, this);
                let (health, stamina, mana) = (o.health(), o.stamina(), o.mana());
                let at_75 = |max: u32| -> u32 {
                    let m: f32 = max.cs_cast();
                    empyrean_common::dotnet::math::round(f64::from(m * 0.75f32)).cs_cast()
                };
                (
                    at_75(health.max_value(&mut StatCtx::in_world(w, this))),
                    at_75(stamina.max_value(&mut StatCtx::in_world(w, this))),
                    at_75(mana.max_value(&mut StatCtx::in_world(w, this))),
                )
            };

            let o = obj_mut(w, this);
            let msg_health_update =
                game_message_private_update_attribute2nd_level(o, Vital::Health, new_health);
            let msg_stamina_update =
                game_message_private_update_attribute2nd_level(o, Vital::Stamina, new_stamina);
            let msg_mana_update =
                game_message_private_update_attribute2nd_level(o, Vital::Mana, new_mana);

            let (health, stamina, mana) = {
                let o = obj(w, this);
                (o.health(), o.stamina(), o.mana())
            };
            dispatch::update_vital::update_vital_uint(w, this, health, new_health);
            dispatch::update_vital::update_vital_uint(w, this, stamina, new_stamina);
            dispatch::update_vital::update_vital_uint(w, this, mana, new_mana);

            send(
                w,
                this,
                [msg_health_update, msg_stamina_update, msg_mana_update],
            );

            // reset damage history for this player
            damage_history::of_mut(w, this).reset();

            creature_on_health_update(w, this);

            fields_mut(w, this).is_in_death_process = false;

            if player_is_logging_out(w, this) {
                player_log_out_final(w, this, true);
            }
        });

        teleport_chain.enqueue_chain(w);
    });

    crate::managers::world_manager::thread_safe_teleport(
        w,
        this,
        new_position,
        Some(callback),
        false,
    );
}

// ACE: Player.HandleActionDie
/// Called when player uses the /die command.
pub fn handle_action_die(w: &mut World, this: ObjectGuid) {
    let is_dead = {
        let o = obj(w, this);
        o.health().current(o) == 0
    };
    if is_dead || player_teleporting(w, this) {
        send_weenie_error(w, this, WeenieError::YoureTooBusy);
        return;
    }

    if fields(w, this).suicide_in_progress {
        return;
    }

    fields_mut(w, this).suicide_in_progress = true;

    if crate::managers::property_manager::get_bool(w, "suicide_instant_death", false, true).item {
        let info = DamageHistoryInfo::new(w, this, 0.0);
        let top = damage_history::of(w, this).top_damager();
        dispatch::die::die(w, this, Some(info), top);
    } else {
        let num_deaths = obj(w, this).num_deaths();
        handle_suicide(w, this, num_deaths, 0);
    }
}

// ACE: Player.SuicideMessages
pub const SUICIDE_MESSAGES: [&str; 5] = [
    "I feel faint...",
    "My sight is growing dim...",
    "My life is flashing before my eyes...",
    "I see a light...",
    "Oh cruel, cruel world!",
];

// ACE: Player.HandleSuicide
/// One step of /die: speak the next line every 3 s, then die.
pub fn handle_suicide(w: &mut World, this: ObjectGuid, num_deaths: i32, step: i32) {
    if !fields(w, this).suicide_in_progress || num_deaths != obj(w, this).num_deaths() {
        return;
    }

    if let Some(line) = usize::try_from(step)
        .ok()
        .and_then(|s| SUICIDE_MESSAGES.get(s))
    {
        // `EnqueueBroadcast(new GameMessageHearSpeech(line, GetNameWithSuffix(), Guid.Full, ChatMessageType.Speech), LocalBroadcastRange);`
        world_object_enqueue_broadcast_speech(w, this, line);

        container_on_talk(w, this, line);

        let mut suicide_chain = ActionChain::new();
        suicide_chain.add_delay_seconds(w, f64::from(3.0f32));
        suicide_chain.add_action(Actor::Object(this), move |w| {
            handle_suicide(w, this, num_deaths, step + 1)
        });
        suicide_chain.enqueue_chain(w);
    } else {
        let info = DamageHistoryInfo::new(w, this, 0.0);
        let top = damage_history::of(w, this).top_damager();
        dispatch::die::die(w, this, Some(info), top);
    }
}

// ACE: Player.CalculateDeathItems
/// The items a player loses on death, moved into the corpse: half the coins, the rolled number
/// of the highest (adjusted) value items, and the slippery items. Nothing on a no-drop landblock
/// or in a PK-lite death.
pub fn calculate_death_items(
    w: &mut World,
    this: ObjectGuid,
    corpse: ObjectGuid,
) -> Vec<ObjectGuid> {
    // if player dies in a PKLite battle,
    // they don't drop any items, and revert back to NPK status

    // if player dies on a No Drop landblock,
    // they don't drop any items

    let corpse_killer = obj(w, corpse).killer_id();
    if crate::world_objects::corpse::is_on_no_drop_landblock(w, corpse)
        || crate::world_objects::player_combat::is_pk_lite_death(w, this, corpse_killer)
    {
        return Vec::new();
    }

    let num_items_dropped = get_num_items_dropped(w, this, corpse);

    let num_coins_dropped = get_num_coins_dropped(w, this);

    let level = obj(w, this).level().unwrap_or(1);
    let can_drop_wielded = level >= 35;

    // get all items in inventory
    let mut inventory = player_get_all_possessions(w, this);

    // exclude pyreals from randomized death item calculation
    inventory.retain(|i| obj(w, *i).biota.weenie_class_id != COIN_STACK_WCID);

    // exclude wielded items if < level 35
    if !can_drop_wielded {
        inventory.retain(|i| obj(w, *i).current_wielded_location().is_none());
    }

    // exclude bonded items
    inventory.retain(|i| obj(w, *i).get_property(PropertyInt::Bonded).unwrap_or(0) == 0);

    // what the drop message reads of the items destroyed below (see `DeadItem`)
    let mut dead = DeadItems::new();
    for g in player_get_all_possessions(w, this) {
        if obj(w, g).bonded() == Some(BondedStatus::Destroy) {
            dead.remember(w, g);
        }
    }

    // handle items with BondedStatus.Destroy
    let destroyed_items = handle_destroy_bonded(w, this);

    // construct the list of death items
    let sorted = DeathItems::new(w, &inventory);

    let mut drop_items: Vec<ObjectGuid> = Vec::new();

    if num_coins_dropped > 0 {
        // add pyreals to dropped items
        let pyreals = player_spend_currency(w, this, COIN_STACK_WCID, num_coins_dropped.cs_cast());
        drop_items.extend(pyreals);
        //Console.WriteLine($"Dropping {numCoinsDropped} pyreals");
    }

    // Remove the items from inventory
    let mut i = 0;
    while i < num_items_dropped && usize::try_from(i).is_ok_and(|i| i < sorted.inventory.len()) {
        let death_item = &sorted.inventory[usize::try_from(i).unwrap_or(0)];
        let item = death_item.world_object;

        // split stack if needed
        if obj(w, item).stack_size().unwrap_or(1) > 1 {
            let found = player_find_object_in_possessions(w, this, item);
            if let Some(stack) = found.result {
                player_adjust_stack(
                    w,
                    this,
                    stack,
                    -1,
                    found.found_in_container,
                    found.root_owner,
                );
                let msg = game_message_set_stack_size(obj_mut(w, stack));
                send(w, this, [msg]);

                let wcid = obj(w, item).biota.weenie_class_id;
                let drop_item = creature_death::create_new_world_object_by_wcid(w, wcid)
                    .expect("NullReferenceException: CreateNewWorldObject(wcid)");
                world_object_set_stack_size(w, drop_item, Some(1));

                //Console.WriteLine("Dropping " + deathItem.WorldObject.Name + " (stack)");
                drop_items.push(drop_item);
            } else {
                log::warn!(
                    "Couldn't find death item stack 0x{:08X}:{} for player {}",
                    item.full(),
                    death_item.name.as_deref().unwrap_or(""),
                    name_of(w, this)
                );
            }
        } else if player_try_remove_from_inventory_with_networking(w, this, item)
            || player_try_dequip_object_with_networking(w, this, item)
        {
            //Console.WriteLine("Dropping " + deathItem.WorldObject.Name);
            drop_items.push(item);
        } else {
            log::warn!(
                "Couldn't find death item 0x{:08X}:{} for player {}",
                item.full(),
                death_item.name.as_deref().unwrap_or(""),
                name_of(w, this)
            );
        }
        i += 1;
    }

    // handle items with BondedStatus.Slippery: always drop on death
    let slippery_items = get_slippery_items(w, this);

    for item in slippery_items {
        if player_try_remove_from_inventory_with_networking(w, this, item)
            || player_try_dequip_object_with_networking(w, this, item)
        {
            drop_items.push(item);
        }
    }

    let destroy_coins =
        crate::managers::property_manager::get_bool(w, "corpse_destroy_pyreals", false, true).item;

    // add items to corpse
    for drop_item in drop_items.clone() {
        // coins already removed from SpendCurrency
        if destroy_coins && obj(w, drop_item).biota.weenie_type == WeenieType::Coin {
            dead.remember(w, drop_item);
            crate::world_objects::world_object::destroy(w, drop_item, true, false);
            continue;
        }

        if !creature_death::container_try_add_to_inventory(w, corpse, drop_item) {
            log::warn!(
                "Player_Death: couldn't add item to {}'s corpse: {}",
                name_of(w, this),
                name_of(w, drop_item)
            );

            if !creature_death::container_try_add_to_inventory(w, this, drop_item) {
                log::warn!(
                    "Player_Death: couldn't re-add item to {}'s inventory: {}",
                    name_of(w, this),
                    name_of(w, drop_item)
                );
            }
        }
    }

    // notify player of destroyed items?
    drop_items.extend(destroyed_items);

    // send network messages
    let drop_list = drop_message(w, &drop_items, num_coins_dropped, &dead);
    if !drop_list.trim().is_empty() {
        send(w, this, [chat(&drop_list)]);

        death_item_log(w, this, &drop_items, corpse);
    }

    drop_items
}

// ACE: Player.DeathItemLog
/// The `[CORPSE]` log line listing the dropped items.
pub fn death_item_log(w: &World, this: ObjectGuid, drop_items: &[ObjectGuid], corpse: ObjectGuid) {
    if drop_items.is_empty() {
        return;
    }

    let mut msg = format!(
        "[CORPSE] {} dropped items on corpse (0x{}): ",
        name_of(w, this),
        corpse
    );

    let destroy_coins =
        crate::managers::property_manager::get_bool(w, "corpse_destroy_pyreals", false, true).item;
    for drop_item in drop_items {
        // a destroyed coin stack is gone from the store: ACE still reads the dead object
        let Some(o) = w.objects.get(*drop_item) else {
            continue;
        };
        let stack_size = o.stack_size();
        let label = if stack_size.is_some_and(|s| s > 1) {
            format!(
                "{} {}",
                dotnet_format(stack_size.unwrap_or(0), "N0"),
                crate::world_objects::world_object::get_plural_name(w, *drop_item)
            )
        } else {
            name_of(w, *drop_item)
        };
        let destroyed = if o.biota.weenie_class_id == 273 && destroy_coins {
            format!(
                " which {} destroyed",
                if stack_size.is_some_and(|s| s > 1) {
                    "were"
                } else {
                    "was"
                }
            )
        } else {
            String::new()
        };
        msg += &format!("{label} (0x{drop_item}){destroyed}, ");
    }

    msg.truncate(msg.len() - 2);

    log::info!("{msg}");
}

// ACE: Player.MaxItemsDropped
/// The maximum # of items a player can drop.
pub const MAX_ITEMS_DROPPED: i32 = 14;

// ACE: Player.GetNumItemsDropped
/// Rolls for the # of items to drop for a player death: none to level 10, 0 or 1 to level 20,
/// then `level / 20` (the era's divisor) plus 0 to 2 (one `ThreadSafeRandom.Next` draw from
/// level 11), capped at 14, less 5 per Clutch of the Miser unless a PK death.
pub fn get_num_items_dropped(w: &World, this: ObjectGuid, corpse: ObjectGuid) -> i32 {
    // take augments into consideration?

    let level = obj(w, this).level().unwrap_or(1);

    if level <= 10 {
        return 0;
    }

    if (11..=20).contains(&level) {
        return ThreadSafeRandom::next(0, 1);
    }

    // level 21+
    // DIVERGE: the era's divisor (`EraFormulas::death_items_level_divisor`): level / 10 before
    // the later halving, ClassicACE's `GetNumItemsDropped` outside its end-of-retail ruleset.
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Death.cs
    let divisor = w.era.formulas.death_items_level_divisor;
    let mut num_items_dropped = level / divisor + ThreadSafeRandom::next(0, 2);

    num_items_dropped = num_items_dropped.min(MAX_ITEMS_DROPPED); // is this really a max cap?

    // The number of items you drop can be reduced with the Clutch of the Miser augmentation. If you get the
    // augmentation three times you will no longer drop any items (except half of your Pyreals and all Rares except if you're a PK).
    // If you drop no items, you will not leave a corpse.

    let aug = obj(w, this).augmentation_less_death_item_loss();
    if !crate::world_objects::player_combat::is_pk_death(w, this, obj(w, corpse).killer_id())
        && aug > 0
    {
        num_items_dropped = 0.max(num_items_dropped.wrapping_sub(aug.wrapping_mul(5)));
    }

    num_items_dropped
}

// ACE: Player.GetNumCoinsDropped
/// if level > 5, lose half coins (trade notes excluded).
#[must_use]
pub fn get_num_coins_dropped(w: &World, this: ObjectGuid) -> i32 {
    let o = obj(w, this);
    let level = o.level().unwrap_or(1);
    let coins = o.coin_value().unwrap_or(0);

    if level > 5 {
        coins / 2
    } else {
        0
    }
}

/// What `DropMessage` reads of an item: its `Name`, `StackSize` and `GetPluralName()`.
#[derive(Debug, Clone)]
pub struct DeadItem {
    pub name: String,
    pub stack_size: Option<i32>,
    pub plural_name: String,
}

/// DIVERGE: `CalculateDeathItems` destroys the coins it drops (`corpse_destroy_pyreals`) and the
/// `BondedStatus.Destroy` items, then `DropMessage` reads their `Name`, `StackSize` and plural from
/// the dead C# objects. They are gone from the store here (reading them would panic, and the death
/// would never finish), so what the message reads is remembered before the destroy.
#[derive(Debug, Default)]
pub struct DeadItems(std::collections::HashMap<ObjectGuid, DeadItem>);

impl DeadItems {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers what the drop message reads of `g`, before it is destroyed.
    pub fn remember(&mut self, w: &World, g: ObjectGuid) {
        let Some(o) = w.objects.get(g) else { return };
        let item = DeadItem {
            name: name_of(w, g),
            stack_size: o.stack_size(),
            plural_name: crate::world_objects::world_object::get_plural_name(w, g),
        };
        self.0.insert(g, item);
    }

    fn name(&self, w: &World, g: ObjectGuid) -> String {
        match self.0.get(&g) {
            Some(d) if w.objects.get(g).is_none() => d.name.clone(),
            _ => name_of(w, g),
        }
    }

    fn stack_size(&self, w: &World, g: ObjectGuid) -> Option<i32> {
        match w.objects.get(g) {
            Some(o) => o.stack_size(),
            None => self.0.get(&g).and_then(|d| d.stack_size),
        }
    }

    fn plural_name(&self, w: &World, g: ObjectGuid) -> String {
        match self.0.get(&g) {
            Some(d) if w.objects.get(g).is_none() => d.plural_name.clone(),
            _ => crate::world_objects::world_object::get_plural_name(w, g),
        }
    }
}

// ACE: Player.DropMessage
/// Builds the network text message for list of items dropped. `dead` answers for the items
/// destroyed since they were chosen (see [`DeadItems`]).
#[must_use]
pub fn drop_message(
    w: &World,
    drop_items: &[ObjectGuid],
    num_coins_dropped: i32,
    dead: &DeadItems,
) -> String {
    let mut msg = String::new();
    let mut coin_msg = true;

    for (i, drop_item) in drop_items.iter().enumerate() {
        let name = dead.name(w, *drop_item);
        let is_coin = name == "Pyreal";

        if is_coin && !coin_msg {
            continue;
        }

        if i == 0 {
            msg += "You've lost ";
        } else {
            msg += ", ";

            if i == drop_items.len() - 1 {
                msg += "and ";
            }
        }

        let mut stack_size = dead.stack_size(w, *drop_item).unwrap_or(1);
        if is_coin {
            stack_size = num_coins_dropped;
            coin_msg = false;
        } else {
            msg += "your ";
        }

        if stack_size == 1 {
            msg += &name;
        } else {
            msg += &format!(
                "{} {}",
                dotnet_format(stack_size, "N0"),
                dead.plural_name(w, *drop_item)
            );
        }
    }
    if !msg.is_empty() {
        msg += "!";
    }

    msg
}

// ACE: Player.PermitTime
/// How long a /permit lasts: one hour.
#[must_use]
pub fn permit_time() -> TimeSpan {
    TimeSpan::from_hours(1.0)
}

// ACE: Player.HandleActionAddPlayerPermission
/// /permit: lets an online player who accepts permissions loot one of this player's corpses.
pub fn handle_action_add_player_permission(w: &mut World, this: ObjectGuid, player_name: &str) {
    // is this player online?
    let Some(player) = player_manager::get_online_player_by_name(w, player_name) else {
        send(w, this, [chat(&format!("{player_name} is not online."))]);
        return;
    };

    let (name, other_name) = (name_of(w, this), name_of(w, player));

    // check for self-permit
    if name == other_name {
        send(
            w,
            this,
            [chat("You already have permission to loot your corpse.")],
        );
        return;
    }

    // verify other player has /consent on
    if !player_get_character_option(w, player, CharacterOption::AcceptCorpseLootingPermissions) {
        send(
            w,
            this,
            [chat(&format!(
                "{other_name} is not accepting corpse looting permissions from other players."
            ))],
        );
        return;
    }

    // do they already have permission?
    if has_loot_permission(w, player, this) {
        send(
            w,
            this,
            [chat(&format!(
                "{other_name} already has permission to loot your corpse."
            ))],
        );
        return;
    }

    let expires = w.now.utc + permit_time();
    fields_mut(w, player).loot_permission.add(this, expires);

    // send messages to both players
    send(
        w,
        player,
        [chat(&format!(
            "{name} has given you permission to loot one of his or her corpses. This permission will last one hour."
        ))],
    );

    send(
        w,
        this,
        [chat(&format!(
            "You have given permission to {other_name} to loot one of your corpses. This permission will last one hour."
        ))],
    );
}

// ACE: Player.HandleActionRemovePlayerPermission
/// Revokes a /permit.
///
/// Not ACE's (fix, V322): a player who is not online is answered with the
/// "doesn't have permission" line ACE's null check meant, naming the player as typed. ACE read the
/// missing player's name before that check and threw, so nothing was answered.
pub fn handle_action_remove_player_permission(w: &mut World, this: ObjectGuid, player_name: &str) {
    // is this player online?
    let Some(player) = player_manager::get_online_player_by_name(w, player_name) else {
        send(
            w,
            this,
            [chat(&format!(
                "{player_name} doesn't have permission to loot your corpse."
            ))],
        );
        return;
    };

    let (name, other_name) = (name_of(w, this), name_of(w, player));

    // check for self-revoke
    if name == other_name {
        send(
            w,
            this,
            [chat("You always have permission to loot your corpse.")],
        );
        return;
    }

    // do they already have permission?
    if !has_loot_permission(w, player, this) {
        send(
            w,
            this,
            [chat(&format!(
                "{other_name} doesn't have permission to loot your corpse."
            ))],
        );
        return;
    }

    // remove looting permissions
    fields_mut(w, player).loot_permission.remove(&this);

    // send messages to both players
    send(
        w,
        player,
        [chat(&format!(
            "{name} has revoked permission to loot one of his or her corpses."
        ))],
    );

    send(
        w,
        this,
        [chat(&format!(
            "{other_name}'s permission to loot your corpse has been revoked."
        ))],
    );
}

// ACE: Player.PrunePermissions
/// Cleans out any expired permissions.
pub fn prune_permissions(w: &mut World, this: ObjectGuid) {
    let now = w.now.utc;
    let f = fields_mut(w, this);
    let mut kept = DotNetDict::new();
    for (k, v) in f.loot_permission.iter() {
        if *v >= now {
            kept.insert(*k, *v);
        }
    }
    f.loot_permission = kept;
}

// ACE: Player.HasLootPermission
/// Whether the owner of `guid` has permitted this player to loot a corpse (after pruning).
pub fn has_loot_permission(w: &mut World, this: ObjectGuid, guid: ObjectGuid) -> bool {
    prune_permissions(w, this);

    fields(w, this).loot_permission.contains_key(&guid)
}

// ACE: Player.HandleActionDisplayPlayerConsentList
/// @consent who: the players whose corpses this player may loot.
pub fn handle_action_display_player_consent_list(w: &mut World, this: ObjectGuid) {
    prune_permissions(w, this);

    if fields(w, this).loot_permission.is_empty() {
        send(
            w,
            this,
            [chat("You do not have permission to loot anyone's corpse.")],
        );
        return;
    }

    let mut player_names = Vec::new();

    let keys: Vec<ObjectGuid> = fields(w, this).loot_permission.keys().copied().collect();
    for player_guid in keys {
        // is the granter required to stay online?
        let (player, _) = player_manager::find_by_guid(w, player_guid.full());

        let Some(player) = player else {
            // DIVERGE: ACE writes this line to the console; here it is a log line.
            log::info!(
                "{}.HandleActionDisplayPlayerConsentList(): couldn't find player guid {player_guid}",
                name_of(w, this)
            );
            continue;
        };
        player_names.push(crate::entity::i_player::name(w, player).unwrap_or_default());
    }
    let list = format!(
        "You have permissions to loot a corpse from these players:\n{}",
        player_names.join("\n")
    );
    send(w, this, [chat(&list)]);
}

// ACE: Player.HandleActionClearPlayerConsentList
/// @consent clear.
pub fn handle_action_clear_player_consent_list(w: &mut World, this: ObjectGuid) {
    prune_permissions(w, this);

    if fields(w, this).loot_permission.is_empty() {
        send(
            w,
            this,
            [chat("You do not have permission to loot anyone's corpse.")],
        );
        return;
    }

    fields_mut(w, this).loot_permission.clear();

    send(
        w,
        this,
        [chat("You have cleared your consent list. Players will have to permit you again to allow you access to their corpse.")],
    );
}

// ACE: Player.HandleActionRemoveFromPlayerConsentList
/// A player can remove corpse looting permissions that were granted to them. `player_name` is the
/// granter.
pub fn handle_action_remove_from_player_consent_list(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
) {
    let (player, _) = player_manager::find_by_name(w, player_name);

    let Some(player) = player else {
        send(w, this, [chat(&format!("{player_name} is not online."))]);
        return;
    };

    let other_name = crate::entity::i_player::name(w, player).unwrap_or_default();

    // check for self-revoke
    if name_of(w, this) == other_name {
        send(
            w,
            this,
            [chat("You always have permission to loot your corpse.")],
        );
        return;
    }

    // do we have permissions?
    if !has_loot_permission(w, this, player.guid()) {
        send(
            w,
            this,
            [chat(&format!(
                "You don't have permission to loot {other_name}'s corpse."
            ))],
        );
        return;
    }

    // remove looting permissions
    fields_mut(w, this).loot_permission.remove(&player.guid());

    send(
        w,
        this,
        [chat(&format!(
            "You have removed your permissions to loot {other_name}'s corpse."
        ))],
    );
}

// ACE: Player.SetLifestoneProtection
pub fn set_lifestone_protection(w: &mut World, this: ObjectGuid) {
    let o = obj_mut(w, this);
    o.set_under_lifestone_protection(true);
    o.set_lifestone_protection_timestamp(Some(0.0));
}

// ACE: Player.HandleLifestoneProtection
/// The lifestone's magic turns an attack away.
pub fn handle_lifestone_protection(w: &mut World, this: ObjectGuid) {
    send_weenie_error(w, this, WeenieError::LifestoneMagicProtectsYou);
    // `EnqueueBroadcast(new GameMessageScript(Guid, PlayScript.ShieldUpBlue));`
    world_object_enqueue_broadcast_script(w, this, PlayScript::ShieldUpBlue);
}

// ACE: Player.LifestoneProtectionTime
/// One minute.
pub const LIFESTONE_PROTECTION_TIME_SECONDS: f64 = 60.0;

// ACE: Player.LifestoneProtectionTick
/// Counts the protection time on each heartbeat; after a minute it ends.
pub fn lifestone_protection_tick(w: &mut World, this: ObjectGuid) {
    if !obj(w, this).under_lifestone_protection() {
        return;
    }

    // Not ACE's (retail captures, V280): the seconds since the previous
    // heartbeat (4 to 6 s), not ACE's fixed interval.
    let interval = crate::world_objects::player_tick::heartbeat_credit(w, this);
    let o = obj_mut(w, this);
    let timestamp = o.lifestone_protection_timestamp().map(|t| t + interval);
    o.set_lifestone_protection_timestamp(timestamp);

    // `LifestoneProtectionTimestamp < LifestoneProtectionTime.TotalSeconds` (false for null)
    if timestamp.is_some_and(|t| t < LIFESTONE_PROTECTION_TIME_SECONDS) {
        return;
    }

    o.set_under_lifestone_protection(false);
    o.set_lifestone_protection_timestamp(None);

    send(
        w,
        this,
        [game_message_system_chat(
            "You're no longer protected by the Lifestone's magic!",
            ChatMessageType::Magic,
        )],
    );
}

// ACE: Player.LifestoneProtectionDispel
pub fn lifestone_protection_dispel(w: &mut World, this: ObjectGuid) {
    let o = obj_mut(w, this);
    o.set_under_lifestone_protection(false);
    o.set_lifestone_protection_timestamp(None);

    send(
        w,
        this,
        [game_message_system_chat(
            "Your actions have dispelled the Lifestone's magic!",
            ChatMessageType::Magic,
        )],
    );
}

// ACE: Player.SetMinimumTimeSincePK
/// After a PK or PK-lite death: temporarily NPK, with the respite timer started.
pub fn set_minimum_time_since_pk(w: &mut World, this: ObjectGuid) {
    if crate::entity::damage_history_info::player_is_olthoi_player(w, this) {
        return;
    }

    let (status, minimum) = {
        let o = obj(w, this);
        (o.player_killer_status(), o.minimum_time_since_pk())
    };
    if status == PlayerKillerStatus::NPK && minimum.is_none() {
        return;
    }

    let prev_status = status;

    {
        let o = obj_mut(w, this);
        o.set_minimum_time_since_pk_prop(Some(0.0));
        o.set_player_killer_status_prop(PlayerKillerStatus::NPK);
    }

    if prev_status == PlayerKillerStatus::PK {
        world_object_enqueue_broadcast_player_killer_status(w, this);
        send_weenie_error(w, this, WeenieError::YouAreTemporarilyNoLongerPK);
    } else if prev_status == PlayerKillerStatus::PKLite {
        world_object_enqueue_broadcast_player_killer_status(w, this);
        send_weenie_error(w, this, WeenieError::YouAreNonPKAgain);
    }
}

// ACE: Player.PK_DeathTick
/// Counts the PK respite timer on each heartbeat; when it runs out, restores the server's PK
/// status.
pub fn pk_death_tick(w: &mut World, this: ObjectGuid) {
    let minimum = obj(w, this).minimum_time_since_pk();
    if minimum.is_none()
        || (crate::managers::property_manager::get_bool(
            w,
            "pk_server_safe_training_academy",
            false,
            true,
        )
        .item
            && obj(w, this).recalls_disabled())
    {
        return;
    }

    let pk_server = crate::managers::property_manager::get_bool(w, "pk_server", false, true).item;
    let pkl_server = crate::managers::property_manager::get_bool(w, "pkl_server", false, true).item;
    let pk_level = PKLevel(obj(w, this).pk_level_modifier().cs_cast());
    if pk_level == PKLevel::NPK && !pk_server && !pkl_server {
        obj_mut(w, this).set_minimum_time_since_pk_prop(None);
        return;
    }

    // Not ACE's (retail captures, V280): the seconds since the previous
    // heartbeat (4 to 6 s), not ACE's fixed interval.
    let interval = crate::world_objects::player_tick::heartbeat_credit(w, this);
    let minimum = minimum.map(|m| m + interval);
    obj_mut(w, this).set_minimum_time_since_pk_prop(minimum);

    let respite =
        crate::managers::property_manager::get_double(w, "pk_respite_timer", 0.0, true).item;
    if minimum.is_some_and(|m| m < respite) {
        return;
    }

    obj_mut(w, this).set_minimum_time_since_pk_prop(None);

    let mut pk_level = pk_level;

    if crate::managers::property_manager::get_bool(w, "pk_server", false, true).item {
        pk_level = PKLevel::PK;
    } else if crate::managers::property_manager::get_bool(w, "pkl_server", false, true).item {
        pk_level = PKLevel::PKLite;
    }

    let werror = match pk_level {
        PKLevel::PK => {
            obj_mut(w, this).set_player_killer_status_prop(PlayerKillerStatus::PK);
            WeenieError::YouArePKAgain
        }

        PKLevel::PKLite => {
            obj_mut(w, this).set_player_killer_status_prop(PlayerKillerStatus::PKLite);
            WeenieError::YouAreNowPKLite
        }

        // `case PKLevel.NPK: return;` (any other value falls out of the switch with WeenieError.None)
        PKLevel::NPK => return,
        _ => WeenieError::None,
    };

    world_object_enqueue_broadcast_player_killer_status(w, this);
    send_weenie_error(w, this, werror);
}

// ACE: Player.GetSlipperyItems
/// The possessions that always drop on death.
#[must_use]
pub fn get_slippery_items(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let all_possessions = player_get_all_possessions(w, this);

    all_possessions
        .into_iter()
        .filter(|i| obj(w, *i).bonded() == Some(BondedStatus::Slippery))
        .collect()
}

// ACE: Player.HandleDestroyBonded
/// Destroys the possessions bonded to be destroyed on death, and returns them.
pub fn handle_destroy_bonded(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let mut destroyed_items = Vec::new();

    let all_possessions = player_get_all_possessions(w, this);
    let destroy: Vec<ObjectGuid> = all_possessions
        .into_iter()
        .filter(|i| obj(w, *i).bonded() == Some(BondedStatus::Destroy))
        .collect();
    for destroy_item in destroy {
        let stack_size = obj(w, destroy_item).stack_size().unwrap_or(1);
        player_try_consume_from_inventory_with_networking(w, this, destroy_item, stack_size);
        destroyed_items.push(destroy_item);
    }
    destroyed_items
}

// ACE: Player.OlthoiDeathTreasureType
/// The death treasure an Olthoi player drops to a player killer: the world database's 2222, or
/// ACE's built-in profile.
#[must_use]
pub fn olthoi_death_treasure_type(
    w: &World,
) -> std::sync::Arc<empyrean_content::models::world::treasure_death::TreasureDeath> {
    w.content
        .get_cached_death_treasure(2222)
        .unwrap_or_else(|| {
            std::sync::Arc::new(
                empyrean_content::models::world::treasure_death::TreasureDeath {
                    treasure_type: 2222,
                    tier: 8,
                    loot_quality_mod: 0.0,
                    unknown_chances: 19,
                    item_chance: 100,
                    item_min_amount: 1,
                    item_max_amount: 2,
                    item_treasure_type_selection_chances: 8,
                    magic_item_chance: 100,
                    magic_item_min_amount: 2,
                    magic_item_max_amount: 3,
                    magic_item_treasure_type_selection_chances: 8,
                    mundane_item_chance: 100,
                    mundane_item_min_amount: 0,
                    mundane_item_max_amount: 1,
                    mundane_item_type_selection_chances: 7,
                    ..Default::default()
                },
            )
        })
}

// ACE: Player.CalculateDeathItems_Olthoi
/// Determines the amount of slag to drop on a Player corpse when killed by an OlthoiPlayer or the
/// loot to drop when an OlthoiPlayer is killed by a Player Killer.
pub fn calculate_death_items_olthoi(
    w: &mut World,
    this: ObjectGuid,
    corpse: ObjectGuid,
    had_vitae: bool,
    killer_is_olthoi_player: bool,
    killer_is_pk_player: bool,
) -> Vec<ObjectGuid> {
    if killer_is_olthoi_player {
        let Some(slag) = loot_generation_factory_roll_slag_player(w, this, had_vitae) else {
            return Vec::new();
        };

        if !creature_death::container_try_add_to_inventory(w, corpse, slag) {
            log::warn!(
                "CalculateDeathItems_Olthoi: couldn't add item to {}'s corpse: {}",
                name_of(w, this),
                name_of(w, slag)
            );
        }

        vec![slag]
    } else if killer_is_pk_player {
        if had_vitae {
            return Vec::new();
        }

        let profile = olthoi_death_treasure_type(w);
        let mut items = loot_generation_factory_create_random_loot_objects(w, &profile);

        if let Some(gland) = loot_generation_factory_roll_gland(w, this, had_vitae) {
            items.push(gland);
        }

        for wo in &items {
            if !creature_death::container_try_add_to_inventory(w, corpse, *wo) {
                log::warn!(
                    "CalculateDeathItems_Olthoi: couldn't add item to {}'s corpse: {}",
                    name_of(w, this),
                    name_of(w, *wo)
                );
            }
        }

        items
    } else {
        Vec::new()
    }
}

// ---- dispatch targets and virtual-dispatch targets: ported above ----

/// `(uint)WeenieClassName.W_COINSTACK_CLASS` (`Player_Commerce.coinStackWcid`).
const COIN_STACK_WCID: u32 = 273;

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files.
// ---------------------------------------------------------------------------------------------

/// `Player.GetAllPossessions()` (`Player_Inventory.cs`).
pub(crate) fn player_get_all_possessions(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::player_inventory::get_all_possessions(w, this)
}

/// `FindObject(guid, SearchLocations.MyInventory | SearchLocations.MyEquippedItems, out
/// foundInContainer, out rootContainer, out _)` (`Player_Inventory.cs`).
fn player_find_object_in_possessions(
    w: &World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> crate::world_objects::player_inventory::Found {
    use crate::world_objects::player_inventory::{find_object, SearchLocations};
    find_object(
        w,
        this,
        item,
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    )
}

/// `AdjustStack(stack, amount, foundInContainer, rootContainer)` (`Player_Inventory.cs`).
fn player_adjust_stack(
    w: &mut World,
    this: ObjectGuid,
    stack: ObjectGuid,
    amount: i32,
    container: Option<ObjectGuid>,
    root_container: Option<ObjectGuid>,
) {
    crate::world_objects::player_inventory::adjust_stack(
        w,
        this,
        stack,
        amount,
        container,
        root_container,
    );
}

/// `wo.SetStackSize(value)` (`WorldObject_Properties.cs`, 4.5a's port in `stackable.rs`).
fn world_object_set_stack_size(w: &mut World, wo: ObjectGuid, value: Option<i32>) {
    obj_mut(w, wo).set_stack_size(value);
}

/// `TryRemoveFromInventoryWithNetworking(guid, out _, RemoveFromInventoryAction.ToCorpseOnDeath)`
/// (`Player_Inventory.cs`).
fn player_try_remove_from_inventory_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    use crate::world_objects::player_inventory::{
        try_remove_from_inventory_with_networking, RemoveFromInventoryAction,
    };
    try_remove_from_inventory_with_networking(
        w,
        this,
        item,
        RemoveFromInventoryAction::ToCorpseOnDeath,
    )
    .is_some()
}

/// `TryDequipObjectWithNetworking(guid, out _, DequipObjectAction.ToCorpseOnDeath)`
/// (`Player_Inventory.cs`).
fn player_try_dequip_object_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    use crate::world_objects::player_inventory::{
        try_dequip_object_with_networking, DequipObjectAction,
    };
    try_dequip_object_with_networking(w, this, item, DequipObjectAction::ToCorpseOnDeath).is_some()
}

/// `TryConsumeFromInventoryWithNetworking(item, amount)` (`Player_Inventory.cs`).
fn player_try_consume_from_inventory_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    amount: i32,
) {
    crate::world_objects::player_inventory::try_consume_from_inventory_with_networking(
        w, this, item, amount,
    );
}

/// `SpendCurrency(coinStackWcid, amount)` (`Player_Commerce.cs`): the coin stacks taken.
fn player_spend_currency(
    w: &mut World,
    this: ObjectGuid,
    wcid: u32,
    amount: u32,
) -> Vec<ObjectGuid> {
    crate::world_objects::player_commerce::spend_currency(w, this, wcid, amount, false)
        .expect("System.ArgumentNullException: dropItems.AddRange(null)")
}

/// `lastDamagerObj.EmoteManager.OnKill(this)`.
fn emote_manager_on_kill(w: &mut World, killer: ObjectGuid, player: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_kill(w, killer, player);
}

/// `EnqueueBroadcast(excludePlayers, sendSelf, msg)` (`WorldObject_Networking.cs`): the players it
/// reached (`List<Player>`; none when the object has no physics body).
fn world_object_enqueue_broadcast_excluding(
    w: &mut World,
    this: ObjectGuid,
    exclude_players: &[ObjectGuid],
    send_self: bool,
    msg: &GameMessage,
) -> Vec<ObjectGuid> {
    crate::world_objects::world_object_networking::enqueue_broadcast_excluding(
        w,
        this,
        exclude_players,
        send_self,
        std::slice::from_ref(msg),
    )
    .unwrap_or_default()
}

/// `LandblockManager.GetLandblock(new LandblockId(Sanctuary.Landblock << 16 | 0xFFFF), true)`, then
/// `lifestoneBlock.EnqueueAction(new ActionEventDelegate(() => lifestoneBlock.EnqueueBroadcast(
/// excludePlayers, true, Sanctuary, LocalBroadcastRangeSq, broadcastMsg)))`.
fn landblock_enqueue_broadcast_at_lifestone(
    w: &mut World,
    _this: ObjectGuid,
    exclude_players: &[ObjectGuid],
    sanctuary: &empyrean_entity::Position,
    msg: GameMessage,
) {
    let lifestone_block = crate::managers::landblock_manager::get_landblock(
        w,
        empyrean_entity::LandblockId::new(sanctuary.landblock() << 16 | 0xFFFF),
        true,
        false,
    );
    let exclude_players = exclude_players.to_vec();
    let sanctuary = *sanctuary;
    w.landblock_manager
        .landblocks
        .expect_mut(lifestone_block)
        .enqueue_action(crate::entity::actions::i_action::Action::delegate(
            move |w: &mut World| {
                crate::entity::landblock::enqueue_broadcast(
                    w,
                    lifestone_block,
                    Some(&exclude_players),
                    true,
                    Some(&sanctuary),
                    Some(crate::world_objects::world_object::LOCAL_BROADCAST_RANGE_SQ),
                    &[msg],
                );
            },
        ));
}

/// `Fellowship.OnDeath(this)` (`Entity/Fellowship.cs`).
fn fellowship_on_death(w: &mut World, this: ObjectGuid) {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, this)
        .expect("ACE: Fellowship is null (NullReferenceException)");
    crate::entity::fellowship::on_death(w, &fellowship, this);
}

/// `CombatMode` (`Creature_Combat.cs` field).
fn creature_combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
    crate::world_objects::creature_combat::fields(obj(w, this)).combat_mode
}

/// `MagicState.IsCasting` (`Player_Magic.cs`).
fn magic_state_is_casting(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_magic::fields(w, this)
        .magic_state
        .is_casting
}

/// `FailCast(tryFizzle)` (`Player_Magic.cs`).
fn player_fail_cast(w: &mut World, this: ObjectGuid, try_fizzle: bool) {
    crate::world_objects::player_magic::fail_cast(w, this, try_fizzle);
}

/// `IsBusy = value` (the `WorldObject.cs` field).
fn player_set_is_busy(w: &mut World, this: ObjectGuid, value: bool) {
    obj_mut(w, this).wo.world_object.is_busy = value;
}

/// `Teleporting` (a `WorldObject.cs` field).
fn player_teleporting(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).wo.world_object.teleporting
}

/// `IsLoggingOut` (`Player.cs` field).
fn player_is_logging_out(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player.is_logging_out)
}

/// `LogOut_Final(skipAnimations)` (`Player.cs`).
fn player_log_out_final(w: &mut World, this: ObjectGuid, skip_animations: bool) {
    crate::world_objects::player::log_out_final(w, this, skip_animations);
}

/// `SetCombatMode(combatMode)` (`Creature_Combat.cs`).
fn player_set_combat_mode(w: &mut World, this: ObjectGuid, mode: CombatMode) {
    crate::world_objects::creature_combat::set_combat_mode(w, this, mode);
}

/// `OnHealthUpdate()` (`Creature.cs`).
fn creature_on_health_update(w: &mut World, this: ObjectGuid) {
    crate::world_objects::creature::on_health_update(w, this);
}

/// `EnqueueBroadcastMotion(new Motion(MotionStance.NonCombat, MotionCommand.Dead))`.
fn creature_enqueue_broadcast_motion_dead(w: &mut World, this: ObjectGuid) {
    let death_anim = crate::network::motion::movement_data::Motion::new(
        empyrean_entity::enums::MotionStance::NonCombat,
        empyrean_entity::enums::MotionCommand::Dead,
        1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast_motion(
        w,
        this,
        &death_anim,
        None,
        None,
    );
}

/// `DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(MotionCommand.Dead)`.
fn motion_table_get_animation_length_dead(w: &World, this: ObjectGuid) -> f32 {
    let id = obj(w, this).motion_table_id();
    let mt = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
    crate::physics::motion_table::get_animation_length_of(
        w,
        mt.as_deref(),
        empyrean_entity::enums::MotionCommand::Dead,
    )
}

/// `EnqueueBroadcast(new GameMessageHearSpeech(line, GetNameWithSuffix(), Guid.Full, ChatMessageType.Speech), LocalBroadcastRange)`.
fn world_object_enqueue_broadcast_speech(w: &mut World, this: ObjectGuid, line: &str) {
    let sender_name = crate::world_objects::player_properties::get_name_with_suffix(w, this);
    let msg =
        crate::network::game_messages::messages::game_message_hear_speech::game_message_hear_speech(
            line,
            &sender_name,
            this.full(),
            ChatMessageType::Speech,
        );
    crate::world_objects::world_object_networking::enqueue_broadcast_range(
        w,
        this,
        &msg,
        crate::world_objects::world_object::LOCAL_BROADCAST_RANGE,
        None,
    );
}

/// `OnTalk(message)` (`Player.cs`).
fn container_on_talk(w: &mut World, this: ObjectGuid, line: &str) {
    crate::world_objects::player::on_talk(w, this, line);
}

/// `EnqueueBroadcast(new GameMessageScript(Guid, script))`.
fn world_object_enqueue_broadcast_script(w: &mut World, this: ObjectGuid, script: PlayScript) {
    let msg = crate::network::game_messages::messages::game_message_script::game_message_script(
        this, script, 1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// `EnqueueBroadcast(new GameMessagePublicUpdatePropertyInt(this, PropertyInt.PlayerKillerStatus, (int)PlayerKillerStatus))`.
fn world_object_enqueue_broadcast_player_killer_status(w: &mut World, this: ObjectGuid) {
    let o = obj_mut(w, this);
    let status: i32 = empyrean_common::dotnet::CsCast::cs_cast(o.player_killer_status().0);
    let msg = crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int(
        o,
        PropertyInt::PlayerKillerStatus,
        status,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// `player.GetCharacterOption(option)` (`Player_Character.cs`).
fn player_get_character_option(w: &World, player: ObjectGuid, option: CharacterOption) -> bool {
    crate::world_objects::player_character::get_character_option(w, player, option)
}

/// Adds a newly created object (4.10's factory returns owned objects) to `World.objects`.
fn insert_new_object(w: &mut World, wo: WorldObject) -> ObjectGuid {
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    guid
}

/// `LootGenerationFactory.RollSlag(this, hadVitae)` (4.10); the slag joins `World.objects`.
fn loot_generation_factory_roll_slag_player(
    w: &mut World,
    player: ObjectGuid,
    had_vitae: bool,
) -> Option<ObjectGuid> {
    let slag = crate::factories::loot_generation_factory_olthoi_play::roll_slag_for_player(
        w, player, had_vitae,
    )?;
    Some(insert_new_object(w, slag))
}

/// `LootGenerationFactory.RollGland(this, hadVitae)` (4.10); the gland joins `World.objects`.
fn loot_generation_factory_roll_gland(
    w: &mut World,
    player: ObjectGuid,
    had_vitae: bool,
) -> Option<ObjectGuid> {
    let gland =
        crate::factories::loot_generation_factory_olthoi_play::roll_gland(w, player, had_vitae)?;
    Some(insert_new_object(w, gland))
}

/// `LootGenerationFactory.CreateRandomLootObjects(profile)` (4.10); the items join `World.objects`.
fn loot_generation_factory_create_random_loot_objects(
    w: &mut World,
    profile: &empyrean_content::models::world::treasure_death::TreasureDeath,
) -> Vec<ObjectGuid> {
    let items = crate::factories::loot_generation_factory::create_random_loot_objects(w, profile);
    items
        .into_iter()
        .map(|wo| insert_new_object(w, wo))
        .collect()
}
