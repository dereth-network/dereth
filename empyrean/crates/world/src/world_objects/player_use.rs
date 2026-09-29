// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Use.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Use.cs`.
//!
//! The two use actions: `UseItem` (0x36, a double click: walk into use radius, then
//! `OnActivate`, then `UseDone` after `LastUseTime`) and `UseWithTarget` (0x35: an item used on
//! another object, through its `HandleActionUseOnTarget`), `UseDone` itself, closing a container
//! the client stopped viewing, and the eat/drink motion sequence consumables share.
//!
//! The move-to chain is `Player_Move.cs`'s (`player_move`). `RecipeManager.VerifyUse` (both
//! overloads) is hosted here: `RecipeManager.cs` is not ported and this file is its one caller.

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::DotNetHashSet;
use empyrean_entity::enums::{MotionCommand, MotionStance, Usable, WeenieError};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::food_state::{FoodCallback, FoodState};
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_use_done::game_event_use_done;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    player_magic, player_move, player_networking, player_tick, world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_Use.cs`.
#[derive(Debug)]
pub struct PlayerUseFields {
    /// This is set by HandleActionUseItem / TryUseItem (`Container.Open` sets it, `FinishClose`
    /// clears it).
    // ACE: Player.LastOpenedContainerId
    pub last_opened_container_id: ObjectGuid,
    /// This is set by Hook.ActOnUse
    // ACE: Player.LasUsedHookId
    pub las_used_hook_id: ObjectGuid,
    // ACE: Player.NextUseTime
    pub next_use_time: DotNetDateTime,
    // ACE: Player.LastUseTime
    pub last_use_time: f32,
    /// The player's summoned pet, as a guid.
    // ACE: Player.CurrentActivePet
    pub current_active_pet: Option<ObjectGuid>,
    /// Fast chugging state variable
    // ACE: Player.FoodState
    pub food_state: FoodState,
}

impl Default for PlayerUseFields {
    fn default() -> Self {
        PlayerUseFields {
            last_opened_container_id: ObjectGuid::default(),
            las_used_hook_id: ObjectGuid::default(),
            next_use_time: DotNetDateTime::MIN_VALUE,
            last_use_time: 0.0,
            current_active_pet: None,
            food_state: FoodState::default(),
        }
    }
}

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// This player's `Player_Use` fields.
///
/// # Panics
/// When `this` is gone or not a player.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerUseFields {
    &obj(w, this)
        .player
        .as_ref()
        .expect("System.InvalidCastException: not a Player")
        .player_use
}

/// This player's `Player_Use` fields, mutably.
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerUseFields {
    &mut obj_mut(w, this)
        .player
        .as_mut()
        .expect("System.InvalidCastException: not a Player")
        .player_use
}

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let session = player_session(w, this).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, msg);
}

/// `WorldObject.IsBusy`.
fn is_busy(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).wo.world_object.is_busy
}

fn set_is_busy(w: &mut World, this: ObjectGuid, value: bool) {
    obj_mut(w, this).wo.world_object.is_busy = value;
}

/// `Player.PKLogout` (`Player.cs`).
fn pk_logout(w: &World, this: ObjectGuid) -> bool {
    obj(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player.pk_logout)
}

/// `CurrentMotionState.Stance`.
fn current_stance(w: &World, this: ObjectGuid) -> MotionStance {
    obj(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("System.NullReferenceException: CurrentMotionState")
        .stance
}

// ================================================================================ Player_Use.cs

/// Handles the 'GameAction 0x35 - UseWithTarget' network message when player double clicks an
/// inventory item resulting in a target indicator and then clicks another item.
// ACE: Player.HandleActionUseWithTarget
pub fn handle_action_use_with_target(
    w: &mut World,
    this: ObjectGuid,
    source_object_guid: u32,
    target_object_guid: u32,
) {
    if pk_logout(w, this) {
        send_use_done_event(w, this, WeenieError::YouHaveBeenInPKBattleTooRecently);
        return;
    }

    player_move::stop_existing_move_to_chains(w, this);

    // source item is always in our possession
    let source_item = player_inventory::find_object(
        w,
        this,
        ObjectGuid::new(source_object_guid),
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    )
    .result;

    let Some(source_item) = source_item else {
        log::warn!("{}.HandleActionUseWithTarget({source_object_guid:08X}, {target_object_guid:08X}): couldn't find {source_object_guid:08X}", name(w, this));
        send_use_done_event(w, this, WeenieError::None);
        return;
    };

    // Resolve the guid to an object that is either in our possession or on the Landblock
    let target = player_inventory::find_object(
        w,
        this,
        ObjectGuid::new(target_object_guid),
        SearchLocations::MyInventory
            | SearchLocations::MyEquippedItems
            | SearchLocations::Landblock,
    )
    .result;

    let Some(target) = target else {
        log::warn!("{}.HandleActionUseWithTarget({source_object_guid:08X}, {target_object_guid:08X}): couldn't find {target_object_guid:08X}", name(w, this));
        send_use_done_event(w, this, WeenieError::None);
        return;
    };

    // handle objects with built-in spells
    if let Some(spell_did) = obj(w, source_item).spell_did() {
        if !recipe_manager_verify_use(w, this, source_item, target) {
            //var spell = new Spell((int)sourceItem.SpellDID);
            //if (spell != null)
            //    Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, $"{spell.Name} cannot be cast on {target.Name}."));
            let usable = obj(w, source_item).item_useable().unwrap_or(Usable::Undef);
            let mut action = "";
            if usable.contains(Usable::Wielded) {
                action = "wield";
            } else if usable.contains(Usable::Contained) {
                action = "contain";
            }
            let msg = format!("You must {action} the {} to use it.", name(w, source_item));
            let session =
                player_session(w, this).expect("System.NullReferenceException: Player.Session");
            let m = game_event_communication_transient_string(
                w.sessions
                    .get_mut(session)
                    .expect("the session's game half"),
                &msg,
            );
            enqueue_send(w, session, m);
            send_use_done_event(w, this, WeenieError::None);
            return;
        }
        // check activation requirements
        let result = dispatch::check_use_requirements::check_use_requirements(w, source_item, this);
        if !result.success {
            if let Some(message) = result.message {
                send(w, this, message);
            }

            send_use_done_event(w, this, WeenieError::None);
            return;
        }

        player_magic::handle_action_cast_targeted_spell(
            w,
            this,
            target_object_guid,
            spell_did,
            Some(source_item),
        );
        return;
    }

    // handle casters with built-in spells
    //if (sourceItemIsEquipped)
    //{
    //    ... (commented out in ACE)
    //}

    if is_trading(w, this) {
        let items_in_trade_window = items_in_trade_window(w, this);
        if dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, source_item, &items_in_trade_window) {
            send_use_done_event(w, this, WeenieError::TradeItemBeingTraded);
            //SendWeenieError(WeenieError.TradeItemBeingTraded);
            return;
        }
        if dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, target, &items_in_trade_window) {
            send_use_done_event(w, this, WeenieError::TradeItemBeingTraded);
            //SendWeenieError(WeenieError.TradeItemBeingTraded);
            return;
        }
    }

    // re-verify client checks
    let source_target_type = obj(w, source_item)
        .target_type()
        .unwrap_or(empyrean_entity::enums::ItemType::None);
    if source_target_type.0 & obj(w, target).item_type().0 == 0 {
        // ItemHolder::TargetCompatibleWithObject
        let msg = format!(
            "Cannot use the {} with the {}",
            name(w, source_item),
            name(w, target)
        );
        player_networking::send_transient_error(w, this, &msg);
        send_use_done_event(w, this, WeenieError::None);
        return;
    }

    if obj(w, target).current_landblock.is_some() && target != this {
        // todo: verify target can be used remotely
        // move RecipeManager.VerifyUse logic into base Player_Use
        // this was avoided because i didn't want to deal with the ramifications of random items missing the correct ItemUseable flags,
        // and because there are still some ItemUseable flags with missing logic we haven't quite figured out yet

        if is_busy(w, this) {
            send_use_done_event(w, this, WeenieError::YoureTooBusy);
            return;
        }

        player_move::create_move_to_chain(
            w,
            this,
            target,
            Box::new(move |w: &mut World, success: bool| {
                if success {
                    dispatch::handle_action_use_on_target::handle_action_use_on_target(
                        w,
                        source_item,
                        this,
                        target,
                    );
                } else {
                    send_use_done_event(w, this, WeenieError::None);
                }
            }),
            None,
            true,
        );
    } else {
        dispatch::handle_action_use_on_target::handle_action_use_on_target(
            w,
            source_item,
            this,
            target,
        );
    }
}

/// Handles the 'GameAction 0x36 - UseItem' network message when player double clicks an item.
///
/// # Panics
/// While trading, when the item is not found (ACE dereferences the null item).
// ACE: Player.HandleActionUseItem
pub fn handle_action_use_item(w: &mut World, this: ObjectGuid, item_guid: u32) {
    if pk_logout(w, this) {
        send_use_done_event(w, this, WeenieError::YouHaveBeenInPKBattleTooRecently);
        return;
    }

    player_move::stop_existing_move_to_chains(w, this);

    let item = player_inventory::find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::MyInventory
            | SearchLocations::MyEquippedItems
            | SearchLocations::Landblock,
    )
    .result;

    if is_trading(w, this) {
        let item = item.expect("System.NullReferenceException: item");
        let items_in_trade_window = items_in_trade_window(w, this);
        if dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, item, &items_in_trade_window) {
            send_use_done_event(w, this, WeenieError::TradeItemBeingTraded);
            //SendWeenieError(WeenieError.TradeItemBeingTraded);
            return;
        }
    }

    if let Some(item) = item {
        let o = obj(w, item);
        if o.current_landblock.is_some()
            && !o.visibility()
            && item != fields(w, this).last_opened_container_id
        {
            if is_busy(w, this) {
                send_use_done_event(w, this, WeenieError::YoureTooBusy);
                return;
            }

            player_move::create_move_to_chain(
                w,
                this,
                item,
                Box::new(move |w: &mut World, success: bool| try_use_item(w, this, item, success)),
                None,
                true,
            );
        } else {
            try_use_item(w, this, item, true);
        }
    } else {
        log::debug!(
            "{}.HandleActionUseItem({item_guid:08X}): couldn't find object",
            name(w, this)
        );
        send_use_done_event(w, this, WeenieError::None);
    }
}

/// Attempts to use an item - checks activation requirements. `success` (ACE's default: true) is
/// whether the move-to chain reached the item. `UseDone` follows after `LastUseTime` seconds,
/// unless the use manages it (`LastUseTime == float.MinValue`).
// ACE: Player.TryUseItem
pub fn try_use_item(w: &mut World, this: ObjectGuid, item: ObjectGuid, success: bool) {
    //Console.WriteLine($"{Name}.TryUseItem({item.Name}, {success})");
    fields_mut(w, this).last_use_time = 0.0;

    if success {
        dispatch::on_activate::on_activate(w, item, this);
    }

    let Some(last_use_time) = w
        .objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .map(|p| p.player_use.last_use_time)
    else {
        return;
    };

    // manually managed
    #[allow(clippy::float_cmp)] // C#'s == float.MinValue
    if last_use_time == f32::MIN {
        return;
    }

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(last_use_time));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        send_use_done_event(w, this, WeenieError::None)
    });
    action_chain.enqueue_chain(w);

    fields_mut(w, this).next_use_time =
        w.now.utc + TimeSpan::from_seconds(f64::from(last_use_time));
}

/// Sends the GameEventUseDone network message for a player, with an optional error message
/// (ACE's default: `WeenieError.None`).
// ACE: Player.SendUseDoneEvent
pub fn send_use_done_event(w: &mut World, this: ObjectGuid, error_type: WeenieError) {
    let session = player_session(w, this).expect("System.NullReferenceException: Player.Session");
    let m = game_event_use_done(
        w.sessions
            .get_mut(session)
            .expect("the session's game half"),
        error_type,
    );
    enqueue_send(w, session, m);
}

/// This method processes the Game Action (F7B1) No Longer Viewing Contents (0x0195). This is
/// raised when we have a container open and open up a second container without closing the
/// first container.
// ACE: Player.HandleActionNoLongerViewingContents
pub fn handle_action_no_longer_viewing_contents(w: &mut World, this: ObjectGuid, object_guid: u32) {
    let container = obj(w, this)
        .current_landblock
        .and_then(|lb| {
            crate::entity::landblock::get_object(w, lb, ObjectGuid::new(object_guid), true)
        })
        .filter(|&g| obj(w, g).is_container());

    if let Some(container) = container {
        if obj(w, container).viewer() == this.full() {
            dispatch::close::close(w, container, this);
        }
    }
}

/// The eat/drink sequence: drop to NonCombat if needed, the use motion, `action`, the return to
/// Ready and to the previous stance; busy meanwhile, and `LastUseTime` is the whole length.
/// `anim_mod` (ACE's default: 1.0) cuts the use motion short.
// ACE: Player.ApplyConsumable
pub fn apply_consumable(
    w: &mut World,
    this: ObjectGuid,
    use_motion: MotionCommand,
    action: FoodCallback,
    anim_mod: f32,
) {
    if property_manager::get_bool(w, "allow_fast_chug", false, true).item
        && player_tick::fast_tick(w, this)
    {
        apply_consumable_with_animation_callbacks(w, this, use_motion, action);
        return;
    }
    set_is_busy(w, this, true);

    let mut action_chain = ActionChain::new();

    // if something other that NonCombat.Ready,
    // manually send this swap
    let prev_stance = current_stance(w, this);

    let mut anim_time = 0.0f32;

    if prev_stance != MotionStance::NonCombat {
        anim_time = world_object_networking::enqueue_motion_force(
            w,
            this,
            &mut action_chain,
            MotionStance::NonCombat,
            MotionCommand::Ready,
            Some(MotionCommand(prev_stance.0)),
            1.0,
            1.0,
        );
    }

    // start the eat/drink motion
    let use_anim_time = world_object_networking::enqueue_motion_force(
        w,
        this,
        &mut action_chain,
        MotionStance::NonCombat,
        use_motion,
        None,
        1.0,
        anim_mod,
    );
    anim_time += use_anim_time;

    // apply consumable
    action_chain.add_action(Actor::Object(this), action);

    #[allow(clippy::float_cmp)] // C#'s == 1.0f
    if anim_mod == 1.0 {
        // return to ready stance
        anim_time += world_object_networking::enqueue_motion_force(
            w,
            this,
            &mut action_chain,
            MotionStance::NonCombat,
            MotionCommand::Ready,
            Some(use_motion),
            1.0,
            1.0,
        );
    } else {
        action_chain.add_delay_seconds(w, f64::from(use_anim_time * (1.0 - anim_mod)));
    }

    if prev_stance != MotionStance::NonCombat {
        anim_time += world_object_networking::enqueue_motion_force(
            w,
            this,
            &mut action_chain,
            prev_stance,
            MotionCommand::Ready,
            Some(MotionCommand::NonCombat),
            1.0,
            1.0,
        );
    }

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        set_is_busy(w, this, false)
    });

    action_chain.enqueue_chain(w);

    fields_mut(w, this).last_use_time = anim_time;
}

/// The fast-chug variant (`allow_fast_chug` and a FastTick player): the rest of the sequence
/// runs from the animation callbacks (`HandleMotionDone_UseConsumable`).
// ACE: Player.ApplyConsumableWithAnimationCallbacks
pub fn apply_consumable_with_animation_callbacks(
    w: &mut World,
    this: ObjectGuid,
    use_motion: MotionCommand,
    action: FoodCallback,
) {
    set_is_busy(w, this, true);

    let mut action_chain = ActionChain::new();

    // if combat mode, temporarily drop to non-combat
    let prev_stance = current_stance(w, this);

    let mut anim_time = 0.0f32;

    if prev_stance != MotionStance::NonCombat {
        anim_time = world_object_networking::enqueue_motion_force(
            w,
            this,
            &mut action_chain,
            MotionStance::NonCombat,
            MotionCommand::Ready,
            Some(MotionCommand(prev_stance.0)),
            1.0,
            1.0,
        );
    }

    // start the eat/drink motion
    let use_anim_time = world_object_networking::enqueue_motion_force(
        w,
        this,
        &mut action_chain,
        MotionStance::NonCombat,
        use_motion,
        None,
        1.0,
        1.0,
    );

    anim_time += use_anim_time;
    let _ = anim_time;

    // the rest is based on animation callback now
    fields_mut(w, this)
        .food_state
        .start_chugging(use_motion, action, use_anim_time, prev_stance);

    action_chain.enqueue_chain(w);

    // manually managed
    fields_mut(w, this).last_use_time = f32::MIN;
}

/// The animation callback of the fast-chug sequence: the use motion's end applies the consumable
/// and returns to Ready; Ready's end restores the stance, then `UseDone`.
// ACE: Player.HandleMotionDone_UseConsumable
pub fn handle_motion_done_use_consumable(
    w: &mut World,
    this: ObjectGuid,
    motion_id: u32,
    success: bool,
) {
    //Console.WriteLine($"HandleMotionDone_UseConsumable({(MotionCommand)motionID}, {success})");
    let _ = success;

    if !player_tick::fast_tick(w, this) || !fields(w, this).food_state.is_chugging {
        return;
    }

    if motion_id != fields(w, this).food_state.use_motion.0 {
        return;
    }

    // restore state vars
    let mut anim_time = 0.0f32;
    let mut action_chain = ActionChain::new();
    let use_motion = fields(w, this).food_state.use_motion;
    let prev_stance = fields(w, this).food_state.prev_stance;

    if motion_id != MotionCommand::Ready.0 {
        if let Some(callback) = fields_mut(w, this).food_state.callback.take() {
            callback(w);
        }

        fields_mut(w, this).food_state.use_motion = MotionCommand::Ready;

        anim_time += world_object_networking::enqueue_motion_force(
            w,
            this,
            &mut action_chain,
            MotionStance::NonCombat,
            MotionCommand::Ready,
            Some(use_motion),
            1.0,
            1.0,
        );
    } else {
        fields_mut(w, this).food_state.finish_chugging();

        if prev_stance != MotionStance::NonCombat {
            anim_time += world_object_networking::enqueue_motion_force(
                w,
                this,
                &mut action_chain,
                prev_stance,
                MotionCommand::Ready,
                Some(MotionCommand::NonCombat),
                1.0,
                1.0,
            );
        }

        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            send_use_done_event(w, this, WeenieError::None);
            set_is_busy(w, this, false);
        });
    }
    let _ = anim_time;

    action_chain.enqueue_chain(w);
}

// ================================================================================ hosted

/// `RecipeManager.VerifyUse(player, source, target)` (`Managers/RecipeManager.cs`, hosted: the
/// file is not ported and `HandleActionUseWithTarget` is its caller): whether source and target
/// are where the source's `ItemUseable` flags say they must be.
// ACE: RecipeManager.VerifyUse
pub fn recipe_manager_verify_use(
    w: &World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> bool {
    let usable = obj(w, source).item_useable().unwrap_or(Usable::Undef);

    if usable == Usable::Undef {
        log::warn!(
            "{}.RecipeManager.VerifyUse({} ({}), {} ({})) - source not usable, falling back on defaults",
            name(w, player),
            name(w, source),
            source,
            name(w, target),
            target
        );

        // re-verify
        if player_inventory::find_object(w, player, source, SearchLocations::MyInventory)
            .result
            .is_none()
        {
            return false;
        }

        // almost always MyInventory, but sometimes can be applied to equipped
        if player_inventory::find_object(
            w,
            player,
            target,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        )
        .result
        .is_none()
        {
            return false;
        }

        return true;
    }

    let source_use = usable.get_source_flags();
    let target_use = usable.get_target_flags();

    recipe_manager_verify_use_flags(w, player, source, source_use)
        && recipe_manager_verify_use_flags(w, player, target, target_use)
}

/// `RecipeManager.VerifyUse(player, obj, usable)`: `obj` is found in the search locations the
/// flags name.
// ACE: RecipeManager.VerifyUse
pub fn recipe_manager_verify_use_flags(
    w: &World,
    player: ObjectGuid,
    o: ObjectGuid,
    usable: Usable,
) -> bool {
    let mut search_locations = SearchLocations::None;

    // TODO: figure out other Usable flags
    if usable.contains(Usable::Contained) {
        search_locations =
            search_locations | SearchLocations::MyInventory | SearchLocations::MyEquippedItems;
    }
    if usable.contains(Usable::Wielded) {
        search_locations = search_locations | SearchLocations::MyEquippedItems;
    }
    if usable.contains(Usable::Remote) {
        search_locations = search_locations | SearchLocations::LocationsICanMove;
        // TODO: moveto for this type
    }

    player_inventory::find_object(w, player, o, search_locations)
        .result
        .is_some()
}

// ================================================================================ pointers

/// `Player.IsTrading` (`Player_Trade.cs`).
fn is_trading(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_trade::is_trading(w, this)
}

/// `Player.ItemsInTradeWindow` (`Player_Trade.cs`).
fn items_in_trade_window(w: &World, this: ObjectGuid) -> DotNetHashSet<ObjectGuid> {
    crate::world_objects::player_trade::items_in_trade_window(w, this)
}
