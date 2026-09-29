// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_House.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_House.cs`.
//!
//! `Player.House` holds a house guid; `house.rs`'s module doc says what it names. Every
//! `Allegiance` read goes through [`allegiance`], which points at the allegiance ports.

use empyrean_common::dereth_date_time::DerethDateTime;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::time::Time;
use empyrean_entity::enums::{
    AllegianceHouseAction, CharacterTitle, ChatMessageType, HeritageGroup, HouseStatus, HouseType,
    PropertyBool, PropertyInt, WeenieError, WeenieErrorWithString, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::house_list;
use crate::entity::i_player::{self, IPlayer};
use crate::entity::landblock;
use crate::entity::position_extensions::get_map_coord_str;
use crate::entity::world_object_info::WorldObjectInfoOf;
use crate::managers::{house_manager, landblock_manager, player_manager, property_manager};
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_house_available_houses::game_event_house_available_houses;
use crate::network::game_event::events::game_event_house_data::game_event_house_data;
use crate::network::game_event::events::game_event_house_status::game_event_house_status;
use crate::network::game_event::events::game_event_house_update_har::game_event_update_har;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_set_stack_size::game_message_set_stack_size;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::house_payment::HousePayment;
use crate::sessions::SessionData;
use crate::world_objects::player_inventory::{self, RemoveFromInventoryAction, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    container, house, player_character, player_commerce, player_location, slum_lord, world_object,
    world_object_equipment, world_object_links,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_House.cs`.
#[derive(Debug, Default)]
pub struct PlayerHouseFields {
    /// The player's house (a guid; see the module doc)
    // ACE: Player.House
    pub house: Option<ObjectGuid>,
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

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerHouseFields {
    &mut obj_mut(w, this)
        .player
        .as_mut()
        .expect("a Player")
        .player_house
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

fn ip_name(w: &World, p: IPlayer) -> String {
    i_player::name(w, p).unwrap_or_default()
}

/// `Session.Network.EnqueueSend(msg)`.
///
/// # Panics
/// When the player has no session (ACE's `NullReferenceException`).
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let session = player_manager::player_session(w, this)
        .expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, msg);
}

/// A game event built over the player's session.
fn event(
    w: &mut World,
    this: ObjectGuid,
    f: impl FnOnce(&mut SessionData) -> GameMessage,
) -> GameMessage {
    let session = player_manager::player_session(w, this)
        .expect("System.NullReferenceException: Player.Session");
    f(w.sessions.get_mut(session).expect("the player's session"))
}

/// `Session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
pub(crate) fn system_chat(
    w: &mut World,
    this: ObjectGuid,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    send(
        w,
        this,
        game_message_system_chat(message, chat_message_type),
    );
}

fn broadcast(w: &mut World, this: ObjectGuid, message: &str) {
    system_chat(w, this, message, ChatMessageType::Broadcast);
}

fn weenie_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    let m = event(w, this, |d| game_event_weenie_error(d, error));
    send(w, this, m);
}

fn weenie_error_with_string(
    w: &mut World,
    this: ObjectGuid,
    error: WeenieErrorWithString,
    s: &str,
) {
    let m = event(w, this, |d| {
        game_event_weenie_error_with_string(d, error, s)
    });
    send(w, this, m);
}

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`): `HeritageGroup == Olthoi || OlthoiAcid`.
#[must_use]
pub fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    let heritage_group = obj(w, this).heritage_group();
    heritage_group == HeritageGroup::Olthoi || heritage_group == HeritageGroup::OlthoiAcid
}

/// `Player.IsTrading` (`Player_Trade.cs`).
fn is_trading(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_trade::is_trading(w, this)
}

/// `item.IsBeingTradedOrContainsItemBeingTraded(ItemsInTradeWindow)`.
fn is_being_traded(w: &World, this: ObjectGuid, item: ObjectGuid) -> bool {
    let window = crate::world_objects::player_trade::items_in_trade_window(w, this);
    dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, item, &window)
}

/// The player's `House` (a guid).
#[must_use]
pub fn house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    w.objects.get(this)?.player.as_ref()?.player_house.house
}

/// `Player.House = value`.
pub fn set_house(w: &mut World, this: ObjectGuid, value: Option<ObjectGuid>) {
    fields_mut(w, this).house = value;
}

/// `House` resolved to an object in the store (see `house::resolve`).
fn house_obj(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let h = house(w, this)?;
    let r = house::resolve(w, h);
    if r != Some(h) {
        set_house(w, this, r);
    }
    r
}

// ACE: Player.Guests
#[must_use]
pub fn guests(w: &World, this: ObjectGuid) -> Option<DotNetDict<ObjectGuid, bool>> {
    let h = house(w, this)?;
    w.objects.get(h).filter(|o| o.is_house())?;
    Some(house::fields(w, h).guests.clone())
}

/// `Guests` of the resolved `House` (the caller has checked `House != null`).
fn guests_of_house(w: &mut World, this: ObjectGuid) -> DotNetDict<ObjectGuid, bool> {
    let h = house_obj(w, this).expect("System.NullReferenceException: House");
    house::fields(w, h).guests.clone()
}

fn i_player_guid_equals(p: IPlayer, this: ObjectGuid) -> bool {
    // `guest.Equals(this)`: reference equality; only the online player is this object.
    p.is_online() && p.guid() == this
}

/// `Player.Account` (`Player.cs`).
///
/// # Panics
/// When the player has none (ACE's `NullReferenceException`).
pub(crate) fn account(w: &World, this: ObjectGuid) -> &empyrean_store::models::auth::Account {
    obj(w, this)
        .player
        .as_ref()
        .and_then(|p| p.player.account.as_ref())
        .expect("System.NullReferenceException: Player.Account")
}

fn account_id(w: &World, this: ObjectGuid) -> u32 {
    account(w, this).account_id
}

// ================================================================================ allegiance pointers

/// `Player.Allegiance`, `AllegianceNode` and the allegiance members housing reads (ported in
/// the allegiance files).
pub mod allegiance {
    use empyrean_entity::enums::AllegiancePermissionLevel;
    use empyrean_entity::ObjectGuid;

    use crate::entity::allegiance_node;
    use crate::entity::i_player::IPlayer;
    use crate::managers::allegiance_manager;
    use crate::world_objects::{allegiance, player_allegiance};
    use crate::World;

    /// `player.Allegiance` (the allegiance object's guid).
    #[must_use]
    pub fn player_allegiance(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
        player_allegiance::i_player_allegiance(w, IPlayer::Online(player))
    }

    /// `player.Allegiance?.MonarchId` (the allegiance object's `MonarchId`).
    #[must_use]
    pub fn monarch_id(w: &World, player: ObjectGuid) -> Option<u32> {
        let a = player_allegiance(w, player)?;
        w.objects.get(a)?.monarch_id()
    }

    /// `player.AllegianceNode?.Rank`.
    #[must_use]
    pub fn player_allegiance_node_rank(w: &World, player: ObjectGuid) -> Option<u32> {
        player_allegiance::allegiance_node_rank(w, player)
    }

    /// `Allegiance.Monarch.Player` (`PlayerManager.FindByGuid(Monarch.PlayerGuid)`).
    #[must_use]
    pub fn monarch_player(w: &World, allegiance: ObjectGuid) -> Option<IPlayer> {
        allegiance_node::player(w, allegiance::monarch_player_guid(w, allegiance))
    }

    /// `player.AllegiancePermissionLevel`.
    #[must_use]
    pub fn permission_level(w: &World, player: ObjectGuid) -> AllegiancePermissionLevel {
        player_allegiance::allegiance_permission_level(w, player)
    }

    /// `Allegiance.GetHouse()`.
    pub fn get_house(w: &mut World, allegiance: ObjectGuid) -> Option<ObjectGuid> {
        allegiance::get_house(w, allegiance)
    }

    /// `AllegianceManager.GetAllegiance(player)`.
    pub fn allegiance_manager_get_allegiance(w: &mut World, player: IPlayer) -> Option<ObjectGuid> {
        allegiance_manager::get_allegiance(w, Some(player))
    }

    /// `allegiance.Members.TryGetValue(guid, out node) ? node.Rank`.
    #[must_use]
    pub fn members_rank(w: &World, allegiance: ObjectGuid, member: ObjectGuid) -> Option<u32> {
        let (_, node) = allegiance::members(w, allegiance)
            .into_iter()
            .find(|(g, _)| *g == member)?;
        Some(allegiance::tree(w, allegiance)?.node(node).rank)
    }
}

// ================================================================================ buying

/// Called when player clicks the 'Buy house' button,
/// after adding the items required
// ACE: Player.HandleActionBuyHouse
pub fn handle_action_buy_house(
    w: &mut World,
    this: ObjectGuid,
    slumlord_id: u32,
    item_ids: Vec<u32>,
) {
    //Console.WriteLine($"\n{Name}.HandleActionBuyHouse()");
    log::info!("[HOUSE] {}.HandleActionBuyHouse()", name(w, this));

    // verify player doesn't already own a house
    let house_instance = get_house_instance(w, this);

    if house_instance.is_some() {
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.HouseAlreadyOwned));
        broadcast(w, this, "You already own a house!");
        log::info!("[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - Already owns another house", name(w, this));
        return;
    }

    let lb = obj(w, this)
        .current_landblock
        .expect("System.NullReferenceException: CurrentLandblock");
    let slumlord = landblock::get_object(w, lb, ObjectGuid::new(slumlord_id), true);
    if let Some(s) = slumlord {
        assert!(
            obj(w, s).is_slum_lord(),
            "System.InvalidCastException: (SlumLord)0x{slumlord_id:08X}"
        );
    }
    let Some(slumlord) = slumlord else {
        log::error!(
            "[HOUSE] {}.HandleActionBuyHouse: Couldn't find slumlord 0x{:08X}!",
            name(w, this),
            slumlord_id
        );
        return;
    };

    if let Some(min_level) = obj(w, slumlord).min_level() {
        let player_level = obj(w, this).level().unwrap_or(1);
        if player_level < min_level {
            weenie_error_with_string(
                w,
                this,
                WeenieErrorWithString::YouMustBeAboveLevel_ToBuyHouse,
                &min_level.to_string(),
            );
            log::info!(
                "[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - MinLevel",
                name(w, this)
            );
            return;
        }
    }

    if obj(w, slumlord).house_requires_monarch() {
        let is_monarch = allegiance::player_allegiance(w, this).is_some()
            && allegiance::monarch_id(w, this) == Some(this.full());
        if !is_monarch {
            weenie_error(w, this, WeenieError::YouMustBeMonarchToPurchaseDwelling);
            log::info!("[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - HouseRequiresMonarch", name(w, this));
            return;
        }
    }

    if let Some(slumlord_min) = obj(w, slumlord).allegiance_min_level() {
        let mut allegiance_min_level =
            property_manager::get_long(w, "mansion_min_rank", -1, true).item;
        if allegiance_min_level == -1 {
            allegiance_min_level = i64::from(slumlord_min);
        }

        if allegiance_min_level > 0
            && (allegiance::player_allegiance(w, this).is_none()
                || i64::from(
                    allegiance::player_allegiance_node_rank(w, this)
                        .expect("System.NullReferenceException: AllegianceNode"),
                ) < allegiance_min_level)
        {
            weenie_error_with_string(
                w,
                this,
                WeenieErrorWithString::YouMustBeAboveAllegianceRank_ToBuyHouse,
                &allegiance_min_level.to_string(),
            );
            log::info!("[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - AllegianceMinLevel", name(w, this));
            return;
        }
    }

    let slumlord_house =
        slum_lord::house(w, slumlord).expect("System.NullReferenceException: slumlord.House");
    if obj(w, slumlord_house).house_type() != HouseType::Apartment {
        if property_manager::get_bool(w, "house_15day_account", false, true).item
            && !obj(w, this).account15_days()
        {
            let create_time = account(w, this).create_time;
            let account_time_span = w.now.utc - create_time;
            if account_time_span.total_days() < 15.0 {
                let msg = "Your account must be at least 15 days old to purchase this dwelling. This applies to all housing except apartments.";
                let m = event(w, this, |d| {
                    game_event_communication_transient_string(d, msg)
                });
                send(w, this, m);
                broadcast(w, this, msg);
                log::info!("[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - house_15day_account", name(w, this));
                return;
            }
        }

        if property_manager::get_bool(w, "house_30day_cooldown", false, true).item {
            // fix gap
            if !obj(w, this).account15_days() {
                manage_account15_days_house_purchase_timestamp(w, this);
            }

            let last_purchase_time = Time::get_date_time_from_timestamp(f64::from(
                obj(w, this).house_purchase_timestamp().unwrap_or(0),
            ));
            let last_purchase_time_plus30 = last_purchase_time.add_days(30.0);

            if last_purchase_time_plus30 > w.now.utc {
                weenie_error(w, this, WeenieError::YouMustWaitToPurchaseHouse);
                log::info!("[HOUSE] {}.HandleActionBuyHouse(): Failed pre-purchase requirement - house_30day_cooldown", name(w, this));
                return;
            }
        }
    }

    let verified = verify_purchase(w, this, slumlord, &item_ids);
    if !verified {
        log::warn!(
            "[HOUSE] {} tried to purchase house {} without the required items!",
            name(w, this),
            slumlord
        );
        return;
    }

    //Console.WriteLine("\nInventory check passed!");
    log::info!(
        "[HOUSE] {}.HandleActionBuyHouse(): Inventory check passed!",
        name(w, this)
    );

    // get the list of items / amounts to consume for purchase
    let house_profile = slum_lord::get_house_profile(w, slumlord);
    let items = get_inventory_items(w, this, &item_ids);

    let consume_items = get_consume_items(w, &house_profile.buy, &items);

    if !try_consume_purchase_items(w, this, &consume_items) {
        let item_id_list = item_ids
            .iter()
            .map(|i| format!("{i:08X}"))
            .collect::<Vec<_>>()
            .join(", ");
        let consume_items_list = consume_items
            .iter()
            .map(|i| {
                format!(
                    "{} ({}) x{}",
                    i.base.name.clone().unwrap_or_default(),
                    i.base.guid,
                    i.value.unwrap_or(0)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        log::error!(
            "[HOUSE] {}.HandleActionBuyHouse({:08X}, {}) - TryConsumePurchaseItems failed with {}",
            name(w, this),
            slumlord_id,
            item_id_list,
            consume_items_list
        );

        return;
    }

    set_house_owner(w, this, slumlord);

    give_deed(w, this, slumlord);
}

// ACE: Player.GiveDeed
pub fn give_deed(w: &mut World, this: ObjectGuid, slum_lord: ObjectGuid) {
    let deed = w
        .content
        .get_cached_weenie_by_class_name("deed")
        .and_then(|weenie| world_object_equipment::create_new_world_object(w, weenie))
        .expect("System.NullReferenceException: CreateNewWorldObject(\"deed\")");
    let deed_guid = deed.guid;
    let _ = w.objects.insert(deed);

    let title = obj(w, this)
        .character_title_id()
        .and_then(|t| player_character::get_title(w, CharacterTitle(t.cast_unsigned())));
    let title_str = title.map_or_else(String::new, |t| format!(", {t}"));

    let dereth_date_time = DerethDateTime::convert_real_world_to_lore_date_time(w.now.utc);
    let date = dereth_date_time.date_to_string();
    let time = dereth_date_time.time_to_string();
    let slum_lord_location = obj(w, slum_lord)
        .location()
        .expect("System.NullReferenceException: slumLord.Location");
    let mut location = get_map_coord_str(&slum_lord_location);
    if location.is_none() {
        location =
            house_manager::apartment_block(slum_lord_location.landblock()).map(str::to_owned);
        if location.is_none() {
            log::error!(
                "{}.GiveDeed() - couldn't find location {}",
                name(w, this),
                slum_lord_location.to_loc_string()
            );
        }
    }

    let long_desc = format!(
        "Bought by {}{title_str} on {date} at {time}\n\nPurchased at {}",
        name(w, this),
        location.unwrap_or_default()
    );
    obj_mut(w, deed_guid).set_long_desc(Some(long_desc));

    if player_inventory::try_create_in_inventory_with_networking(w, this, deed_guid).is_none() {
        // not ACE: the deed that did not fit is unreachable (C# collects it)
        w.objects.remove(deed_guid);
    }
}

/// Removes the house deed from the player's inventory when they abandon the house
// ACE: Player.RemoveDeed
pub fn remove_deed(w: &mut World, this: ObjectGuid) {
    let deeds = container::get_inventory_items_of_wcid(w, this, 9549);
    // `if (deeds == null)`: never true, GetInventoryItemsOfWCID returns a list
    for deed in deeds {
        player_inventory::try_consume_from_inventory_with_networking(w, this, deed, i32::MAX);
    }
}

// ================================================================================ rent

// ACE: Player.HandleActionRentHouse
pub fn handle_action_rent_house(
    w: &mut World,
    this: ObjectGuid,
    slumlord_id: u32,
    item_ids: Vec<u32>,
) {
    //Console.WriteLine($"{Name}.HandleActionRentHouse({slumlord_id:X8}, {string.Join(", ", item_ids.Select(i => i.ToString("X8")))})");
    let id_list = item_ids
        .iter()
        .map(|i| format!("{i:08X}"))
        .collect::<Vec<_>>()
        .join(", ");
    log::info!(
        "[HOUSE] {}.HandleActionRentHouse({:08X}, {})",
        name(w, this),
        slumlord_id,
        id_list
    );

    let slumlord = player_inventory::find_object(
        w,
        this,
        ObjectGuid::new(slumlord_id),
        SearchLocations::Landblock,
    )
    .result
    .filter(|&s| obj(w, s).is_slum_lord());
    let Some(slumlord) = slumlord else {
        log::warn!(
            "[HOUSE] {}.HandleActionRentHouse({:08X}): Could not find SlumLord in world.",
            name(w, this),
            slumlord_id
        );
        return;
    };

    if slum_lord::is_rent_paid(w, slumlord) {
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.HouseRentFailed));  // WeenieError.HouseRentFailed == blank message
        broadcast(w, this, "The maintenance has already been paid for this period.\nYou may not prepay next period's maintenance.");

        log::info!("[HOUSE] {}.HandleActionRentHouse({:08X}): The maintenance has already been paid for this period.", name(w, this), slumlord_id);
        return;
    }

    let slumlord_owner = obj(w, slumlord).house_owner();
    let (owner, _) = player_manager::find_by_guid(w, slumlord_owner.unwrap_or(0));
    if let Some(owner) = owner {
        let character_houses = house_manager::get_character_houses(w, owner.guid().full());
        let owner_account = i_player::account(w, owner)
            .expect("System.NullReferenceException: owner.Account")
            .account_id;
        let account_houses = house_manager::get_account_houses(w, owner_account);

        let owner_houses = if property_manager::get_bool(w, "house_per_char", false, true).item {
            character_houses
        } else {
            account_houses
        };

        if owner_houses.len() > 1 {
            broadcast(w, this, "The owner of this house currently owns multiple houses. Maintenance cannot be paid until they only own 1 house.");

            log::info!(
                "[HOUSE] {}.HandleActionRentHouse({:08X}): The owner of this house currently owns multiple houses. Maintenance cannot be paid until they only own 1 house.",
                name(w, this),
                slumlord_id
            );
            return;
        }
    } else {
        log::error!(
            "[HOUSE] {}.HandleActionRentHouse({:08X}): couldn't find house owner {}",
            name(w, this),
            slumlord_id,
            slumlord_owner.map_or_else(String::new, |o| o.to_string())
        );
    }

    let mut log_line = "[HOUSE] HandleActionRentHouse:\n".to_owned();
    log_line += &format!("{} ({})\n", name(w, slumlord), slumlord);
    let rent_items = slum_lord::get_rent_items(w, slumlord);
    //Console.WriteLine("Required items:");
    log_line += "Required items:";
    for buy_item in &rent_items {
        let stack_str = buy_item
            .stack_size()
            .filter(|&s| s > 1)
            .map_or_else(String::new, |s| format!("{s} "));
        log_line += &format!(
            "{stack_str}{}\n",
            buy_item
                .get_property(empyrean_entity::enums::PropertyString::Name)
                .unwrap_or_default()
        );
    }

    //Console.WriteLine("\nSent items:");
    log_line += "\nSent items:\n";
    for item_id in &item_ids {
        let Some(item) = container::get_inventory_item(w, this, ObjectGuid::new(*item_id)) else {
            log_line += &format!("Couldn't find inventory item {item_id:08X}\n");
            continue;
        };
        let stack_str = obj(w, item)
            .stack_size()
            .filter(|&s| s > 1)
            .map_or_else(String::new, |s| format!("{s} "));
        log_line += &format!("{stack_str}{} ({})\n", name(w, item), item);

        if is_trading(w, this) && is_being_traded(w, this, item) {
            log_line += &format!(
                "{stack_str}{} ({}) is currently being traded, skipping.\n",
                name(w, item),
                item
            );
            continue;
        }
    }
    log_line += "\n";
    log::info!("{log_line}");

    // filter to items found in player's inventory
    let items = get_inventory_items(w, this, &item_ids);

    // get the list of items / amounts to consume for rent remaining
    let house_profile = slum_lord::get_house_profile(w, slumlord);
    let mut consume_items = get_consume_items(w, &house_profile.rent, &items);

    if is_trading(w, this) {
        let window = crate::world_objects::player_trade::items_in_trade_window(w, this);
        consume_items.retain(|item| !window.contains(&item.base.guid));
    }

    if consume_items.is_empty() {
        log::warn!("[HOUSE] {}.HandleActionRentHouse({:08X}): Nothing sent could be transferred to slumlord for rent.", name(w, this), slumlord_id);
        return;
    }

    for consume_item in &consume_items {
        try_consume_item_for_rent(w, this, slumlord, consume_item);
    }

    container::merge_all_stackables(w, slumlord);

    // force save to database
    dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);

    for item in container::inventory_values(w, slumlord) {
        dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
    }

    dispatch::act_on_use::act_on_use(w, slumlord, this);

    handle_action_query_house(w, this);

    let maintenance_status = format!(
        "Maintenance {}paid.",
        if slum_lord::is_rent_paid(w, slumlord) {
            ""
        } else {
            "partially "
        }
    );

    broadcast(w, this, &maintenance_status);

    log::info!(
        "[HOUSE] {}.HandleActionRentHouse({:08X}): {}",
        name(w, this),
        slumlord_id,
        maintenance_status
    );
}

/// Returns the WorldObjects for item_ids that are in the player's inventory
// ACE: Player.GetInventoryItems
pub fn get_inventory_items(w: &World, this: ObjectGuid, item_ids: &[u32]) -> Vec<ObjectGuid> {
    let mut inventory_items = Vec::new();

    for &item_id in item_ids {
        match player_inventory::find_object(
            w,
            this,
            ObjectGuid::new(item_id),
            SearchLocations::MyInventory,
        )
        .result
        {
            Some(item) => inventory_items.push(item),
            None => {
                log::error!(
                    "{}.GetInventoryItems() - couldn't find {:08X}",
                    name(w, this),
                    item_id
                )
            }
        }
    }

    inventory_items
}

/// Returns the amount of items to consume for a house purchase / maintenance payment
// ACE: Player.GetConsumeItems
#[must_use]
pub fn get_consume_items(
    w: &World,
    house_items: &[HousePayment],
    player_items: &[ObjectGuid],
) -> Vec<WorldObjectInfoOf<i32>> {
    let mut consume_items = Vec::new();

    for house_item in house_items {
        consume_items.extend(house_item.get_consume_items(w, player_items));
    }

    consume_items
}

/// Moves or splits an item from player inventory => slumlord inventory
// ACE: Player.TryConsumeItemForRent
pub fn try_consume_item_for_rent(
    w: &mut World,
    this: ObjectGuid,
    slumlord: ObjectGuid,
    item_info: &WorldObjectInfoOf<i32>,
) -> bool {
    let Some(item) = item_info.base.try_get_world_object(w) else {
        log::error!(
            "[HOUSE] {}.TryConsumeItemForRent({:08X}) - couldn't get item",
            name(w, this),
            item_info.base.guid.full()
        );
        return false;
    };

    let amount = item_info.value.unwrap_or(0);
    let stack_size = obj(w, item).stack_size().unwrap_or(1);

    if amount > stack_size {
        log::error!(
            "[HOUSE] {}.TryConsumeItemForRent({} ({}) - amount {} > stacksize {}",
            name(w, this),
            name(w, item),
            item,
            amount,
            stack_size
        );
        return false;
    }

    if amount == stack_size {
        try_move_item_for_rent(w, this, slumlord, item)
    } else {
        try_split_item_for_rent(w, this, slumlord, item, amount)
    }
}

fn stack_prefix(w: &World, g: ObjectGuid) -> String {
    let s = w
        .objects
        .get(g)
        .and_then(WorldObject::stack_size)
        .unwrap_or(1);
    if s > 1 {
        format!("{s}x ")
    } else {
        String::new()
    }
}

/// Moves an item from player inventory => slumlord inventory
// ACE: Player.TryMoveItemForRent
pub fn try_move_item_for_rent(
    w: &mut World,
    this: ObjectGuid,
    slumlord: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    // verify slumlord can add item to inventory
    if !container::can_add_to_inventory(w, slumlord, item) {
        log::error!(
            "[HOUSE] {}.TryMoveItemForRent({} ({}), {} ({}) ) - CanAddToInventory failed!",
            name(w, this),
            name(w, slumlord),
            slumlord,
            name(w, item),
            item
        );
        return false;
    }

    // remove entire item from player's inventory
    if player_inventory::try_remove_from_inventory_with_networking(
        w,
        this,
        item,
        RemoveFromInventoryAction::SpendItem,
    )
    .is_none()
    {
        log::error!(
            "[HOUSE] {}.TryMoveItemForRent({} ({}), {} ({}) ) - TryRemoveFromInventoryWithNetworking failed!",
            name(w, this),
            name(w, slumlord),
            slumlord,
            name(w, item),
            item
        );
        return false;
    }

    // add to slumlord inventory
    if !container::try_add_to_inventory(w, slumlord, item, 0, false, true) {
        log::error!(
            "[HOUSE] {}.TryMoveItemForRent({} ({}), {} ({}) ) - TryAddToInventory failed!",
            name(w, this),
            name(w, slumlord),
            slumlord,
            name(w, item),
            item
        );
        return false;
    }

    log::info!(
        "[HOUSE] {}.TryMoveItemForRent({} ({}), {}{} ({})) - Successfully moved to Slumlord.",
        name(w, this),
        name(w, slumlord),
        slumlord,
        stack_prefix(w, item),
        name(w, item),
        item
    );
    true
}

/// Splits an item from player inventory => slumlord inventory
// ACE: Player.TrySplitItemForRent
pub fn try_split_item_for_rent(
    w: &mut World,
    this: ObjectGuid,
    slumlord: ObjectGuid,
    item: ObjectGuid,
    amount: i32,
) -> bool {
    // create a new item w/ stacksize = amount for the slumlord's inventory
    let wcid = obj(w, item).biota.weenie_class_id;
    let new_item = world_object_equipment::create_new_world_object_by_wcid(w, wcid)
        .expect("System.NullReferenceException: CreateNewWorldObject");
    let new_item_guid = new_item.guid;
    let _ = w.objects.insert(new_item);
    obj_mut(w, new_item_guid).set_stack_size(Some(amount));

    let fail = |w: &mut World, what: &str| {
        log::error!(
            "[HOUSE] {}.TrySplitItemForRent({} ({}), {} ({}), {}) - {}",
            name(w, this),
            name(w, slumlord),
            slumlord,
            name(w, item),
            item,
            amount,
            what
        );
        world_object::destroy(w, new_item_guid, true, false);
        false
    };

    // verify it can be added to slumlord's inventory
    if !container::can_add_to_inventory(w, slumlord, new_item_guid) {
        let what = format!(
            "CanAddToInventory failed for split item {} ({})!",
            name(w, new_item_guid),
            new_item_guid
        );
        return fail(w, &what);
    }

    // fetch container for AdjustStack
    let Some((_, found_container)) = container::get_inventory_item_with_container(w, this, item)
    else {
        return fail(w, "GetInventoryItem failed!");
    };

    // subtract amount from player's item stacksize
    if !player_inventory::adjust_stack(w, this, item, -amount, Some(found_container), Some(this)) {
        return fail(w, "failed to adjust stack!");
    }

    // force save of new player stack
    dispatch::save_biota_to_database::save_biota_to_database(w, item, true);

    // send network updates
    let m = game_message_set_stack_size(obj_mut(w, item));
    send(w, this, m);
    let encumbrance = obj(w, this).encumbrance_val().unwrap_or(0);
    let m = game_message_private_update_property_int(
        obj_mut(w, this),
        PropertyInt::EncumbranceVal,
        encumbrance,
    );
    send(w, this, m);

    if obj(w, item).biota.weenie_type == WeenieType::Coin {
        player_commerce::update_coin_value(w, this, true);
    }

    if !container::try_add_to_inventory(w, slumlord, new_item_guid, 0, false, true) {
        let what = format!(
            "TryAddToInventory failed for split item {} ({})!",
            name(w, new_item_guid),
            new_item_guid
        );
        return fail(w, &what);
    }

    log::info!(
        "[HOUSE] {}.TrySplitItemForRent({} ({}), {} ({}), {}) - Created new item {}{} ({}) and moved to Slumlord.",
        name(w, this),
        name(w, slumlord),
        slumlord,
        name(w, item),
        item,
        amount,
        stack_prefix(w, new_item_guid),
        name(w, new_item_guid),
        new_item_guid
    );

    // force save of new slumlord stack
    dispatch::save_biota_to_database::save_biota_to_database(w, new_item_guid, true);

    true
}

// ================================================================================ abandon, login, eviction

// ACE: Player.HandleActionAbandonHouse
pub fn handle_action_abandon_house(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"\n{Name}.HandleActionAbandonHouse()");
    log::info!("[HOUSE] {}.HandleActionAbandonHouse()", name(w, this));

    let house_instance = get_house_instance(w, this);

    if house_instance.is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    // only the character who directly owns the house can use /house abandon
    if obj(w, this).house_instance().is_none() {
        weenie_error(w, this, WeenieError::OnlyHouseOwnerCanUseCommand);
        return;
    }

    let house = get_house(w, this);
    if let Some(house) = house {
        {
            let h = obj_mut(w, house);
            h.set_house_owner_prop(None);
            h.set_monarch_id(None);
            h.set_house_owner_name(None);
        }
        house::clear_permissions(w, house);

        dispatch::save_biota_to_database::save_biota_to_database(w, house, true);

        // relink
        world_object_links::update_links(w, house);

        if house::has_dungeon(w, house) {
            if let Some(dungeon_house) = house::get_dungeon_house(w, house) {
                world_object_links::update_links(w, dungeon_house);
            }
        }

        // player slumlord 'off' animation
        let slumlord =
            house::slum_lord(w, house).expect("System.NullReferenceException: house.SlumLord");
        container::clear_inventory(w, slumlord, false);
        slum_lord::off(w, slumlord);

        // reset slumlord name
        slum_lord::set_and_broadcast_name(w, slumlord, None);

        dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);

        house_list::add_to_available(w, slumlord, house);
    }

    {
        let o = obj_mut(w, this);
        o.set_house_id(None);
        o.set_house_instance(None);
        //HousePurchaseTimestamp = null;
        o.set_house_rent_timestamp(None);
    }

    set_house(w, this, None);
    obj_mut(w, this)
        .player
        .as_mut()
        .expect("a Player")
        .player_tick
        .house_rent_warn_timestamp = 0.0;

    dispatch::save_biota_to_database::save_biota_to_database(w, this, true);

    // send text message
    broadcast(w, this, "You abandon your house!");

    let house = house.expect("System.NullReferenceException: house");
    house_manager::remove_rent_queue(w, house.full());
    let house_type = obj(w, house).house_type();
    house_manager::decrement_total_owned_housing_by_type(w, house_type);

    house::clear_restrictions(w, house);

    remove_deed(w, this);

    handle_action_query_house(w, this);
}

// ACE: Player.HandleHouseOnLogin
pub fn handle_house_on_login(w: &mut World, this: ObjectGuid) {
    let evicted = obj(w, this)
        .get_property(PropertyBool::HouseEvicted)
        .unwrap_or(false);
    if evicted {
        let mut evict_chain = ActionChain::new();
        evict_chain.add_delay_seconds(w, f64::from(5.0f32)); // todo: need inventory callback
        evict_chain.add_action(Actor::Object(this), move |w| handle_eviction(w, this));
        evict_chain.enqueue_chain(w);
        return;
    }

    let house_instance = get_house_instance(w, this);

    if house(w, this).is_none() {
        load_house(w, this, house_instance, false);
    }
    let Some(h) = house_obj(w, this) else { return };
    let Some(slumlord) = house::slum_lord(w, h) else {
        return;
    };

    let house_owner = get_house_owner(w, this).expect("System.NullReferenceException: houseOwner");

    // var purchaseTime = (uint)(houseOwner.HousePurchaseTimestamp ?? 0);

    let owner_purchase =
        i_player::get_property(w, house_owner, PropertyInt::HousePurchaseTimestamp);
    if obj(w, this).house_purchase_timestamp() != owner_purchase {
        obj_mut(w, this).set_house_purchase_timestamp(owner_purchase);
        let value = obj(w, this).house_purchase_timestamp().unwrap_or(0);
        let m1 = game_message_private_update_property_int(
            obj_mut(w, this),
            PropertyInt::HousePurchaseTimestamp,
            value,
        );
        let m2 = game_message_system_chat(
            "Updating housing information...",
            ChatMessageType::Broadcast,
        );
        let m3 = event(w, this, |d| {
            game_event_house_status(d, WeenieError::HouseEvicted)
        });
        send(w, this, m1);
        send(w, this, m2);
        send(w, this, m3);
    }

    // var rentTime = (uint)(houseOwner.HouseRentTimestamp ?? 0);

    let owner_rent = i_player::get_property(w, house_owner, PropertyInt::HouseRentTimestamp);
    if obj(w, this).house_rent_timestamp() != owner_rent {
        obj_mut(w, this).set_house_rent_timestamp(owner_rent);
    }

    if slum_lord::inventory_loaded(w, slumlord) {
        handle_house_on_login_inner(w, this);
    } else {
        house_manager::register_callback(
            w,
            Some(h),
            Box::new(move |w: &mut World, _house: ObjectGuid| handle_house_on_login_inner(w, this)),
        );
    }
}

// ACE: Player.HandleHouseOnLogin_Inner
pub fn handle_house_on_login_inner(w: &mut World, this: ObjectGuid) {
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(5.0f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        let Some(h) = house_obj(w, this) else { return };
        let Some(slumlord) = house::slum_lord(w, h) else { return };

        if obj(w, h).house_status() == HouseStatus::Active && !slum_lord::is_rent_paid(w, slumlord) && property_manager::get_bool(w, "house_rent_enabled", true, true).item {
            let days = if house::is_apartment(w, h) { "90" } else { "30" };
            system_chat(
                w,
                this,
                &format!("Warning!  You have not paid your maintenance costs for the last {days} day maintenance period.  Please pay these costs by this deadline or you will lose your house, and all your items within it."),
                ChatMessageType::System,
            );
        }

        if obj(w, h).house_owner() == Some(this.full()) && !slum_lord::has_requirements(w, slumlord, this) && property_manager::get_bool(w, "house_purchase_requirements", false, true).item {
            let rank_str = allegiance::player_allegiance_node_rank(w, this).map_or_else(String::new, |r| r.to_string());
            let min = slum_lord::get_allegiance_min_level(w, slumlord);
            system_chat(
                w,
                this,
                &format!("Warning!  Your allegiance rank {rank_str} is now below the requirements for owning a mansion.  Please raise your allegiance rank to {min} before the end of the maintenance period or you will lose your mansion, and all your items within it."),
                ChatMessageType::System,
            );
        }

        // TODO: for account houses, run this even if char doesn't own house
        is_multi_house_owner(w, this, true);
    });
    action_chain.enqueue_chain(w);
}

// ACE: Player.HandleEviction
pub fn handle_eviction(w: &mut World, this: ObjectGuid) {
    broadcast(w, this, "Your house has reverted due to non-payment of the maintenance costs.  All items stored in the house have been lost.");
    remove_deed(w, this);

    obj_mut(w, this).remove_property(PropertyBool::HouseEvicted);
}

/// Sets this player as the owner of a house
// ACE: Player.SetHouseOwner
pub fn set_house_owner(w: &mut World, this: ObjectGuid, slumlord: ObjectGuid) {
    let house =
        slum_lord::house(w, slumlord).expect("System.NullReferenceException: slumlord.House");

    //Console.WriteLine($"Setting {Name} as owner of {house.Name}");
    log::info!(
        "[HOUSE] Setting {} (0x{}) as owner of {} (0x{:08X})",
        name(w, this),
        this,
        name(w, house),
        house.full()
    );

    // set player properties
    let house_id = obj(w, house).house_id();
    let house_type = obj(w, house).house_type();
    {
        let o = obj_mut(w, this);
        o.set_house_id(house_id);
        o.set_house_instance(Some(house.full()));
    }

    let house_purchase_timestamp = w.now.unix_time;
    if house_type != HouseType::Apartment {
        obj_mut(w, this).set_house_purchase_timestamp(Some(house_purchase_timestamp.cs_cast()));
    }
    let due = house::get_rent_due(w, house, house_purchase_timestamp.cs_cast());
    obj_mut(w, this).set_house_rent_timestamp(Some(due.cast_signed()));
    obj_mut(w, this)
        .player
        .as_mut()
        .expect("a Player")
        .player_tick
        .house_rent_warn_timestamp = 0.0;

    // set house properties
    let player_name = name(w, this);
    {
        let h = obj_mut(w, house);
        h.set_house_owner_prop(Some(this.full()));
        h.set_house_owner_name(Some(player_name.clone()));
        h.set_open_to_everyone(false);
    }
    dispatch::save_biota_to_database::save_biota_to_database(w, house, true);

    // relink
    world_object_links::update_links(w, house);

    if house::has_dungeon(w, house) {
        if let Some(dungeon_house) = house::get_dungeon_house(w, house) {
            world_object_links::update_links(w, dungeon_house);
        }
    }

    // notify client w/ HouseID
    broadcast(w, this, "Congratulations!  You now own this dwelling.");

    // player slumlord 'on' animation
    slum_lord::on(w, slumlord);

    // set house name
    slum_lord::set_and_broadcast_name(w, slumlord, Some(&player_name));

    container::clear_inventory(w, slumlord, false);

    dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);

    house_list::remove_from_available(w, slumlord, house);

    dispatch::save_biota_to_database::save_biota_to_database(w, this, true);

    if house_type != HouseType::Apartment {
        let value = obj(w, this).house_purchase_timestamp().unwrap_or(0);
        let m = game_message_private_update_property_int(
            obj_mut(w, this),
            PropertyInt::HousePurchaseTimestamp,
            value,
        );
        send(w, this, m);
    }

    // set house data
    // why has this changed? use callback?
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(3.0f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        handle_action_query_house(w, this);
        house::update_restriction_db(w, house, None);

        // boot anyone who may have been wandering around inside...
        handle_action_boot_all(w, this, false);

        house_manager::add_rent_queue(w, IPlayer::Online(this), house.full());
        dispatch::act_on_use::act_on_use(w, slumlord, this);
    });
    action_chain.enqueue_chain(w);
}

/// Removes verified items from inventory for house purchase
// ACE: Player.TryConsumePurchaseItems
pub fn try_consume_purchase_items(
    w: &mut World,
    this: ObjectGuid,
    purchase_items: &[WorldObjectInfoOf<i32>],
) -> bool {
    for purchase_item in purchase_items {
        let Some(item) = player_inventory::find_object(
            w,
            this,
            purchase_item.base.guid,
            SearchLocations::MyInventory,
        )
        .result
        else {
            // this should never happen, due to previous verifications
            log::error!(
                "[HOUSE] {}.ConsumeItemsForHousePurchase(): couldn't find {}!",
                name(w, this),
                purchase_item.base.guid
            );
            return false;
        };

        let amount = purchase_item.value.unwrap_or(0);
        let stack_size = obj(w, item).stack_size().unwrap_or(1);

        if amount > stack_size {
            // this should also never happen, due to previous checks
            log::error!(
                "[HOUSE] {}.ConsumeItemsForHousePurchase(): {} ({}) amount({}) > stackSize({})!",
                name(w, this),
                name(w, item),
                item,
                amount,
                stack_size
            );
            return false;
        }

        let item_name = name(w, item);
        if !player_inventory::try_consume_from_inventory_with_networking(w, this, item, amount) {
            // all of these things should never happen, just being absolutely certain...
            log::error!(
                "[HOUSE] {}.ConsumeItemsForHousePurchase(): TryConsumeFromInventoryWithNetworking({} ({}), {}) failed!",
                name(w, this),
                item_name,
                item,
                amount
            );
            return false;
        }

        // force save partial stack reductions
        dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
    }
    true
}

/// Verifies the player inventory has required items to purchase house
// ACE: Player.VerifyPurchase
pub fn verify_purchase(
    w: &mut World,
    this: ObjectGuid,
    slumlord: ObjectGuid,
    item_ids: &[u32],
) -> bool {
    // verify house is not already owned
    if obj(w, slumlord).house_owner().is_some() {
        return false;
    }

    //Console.WriteLine($"{slumlord.Name} ({slumlord.Guid})");
    let mut log_line = "[HOUSE] VerifyPurchase:\n".to_owned();
    log_line += &format!("{} ({})\n", name(w, slumlord), slumlord);
    let buy_items = slum_lord::get_buy_items(w, slumlord);
    //Console.WriteLine("Required items:");
    log_line += "Required items:";
    for buy_item in &buy_items {
        let stack_str = buy_item
            .stack_size()
            .filter(|&s| s > 1)
            .map_or_else(String::new, |s| format!("{s} "));
        log_line += &format!(
            "{stack_str}{}\n",
            buy_item
                .get_property(empyrean_entity::enums::PropertyString::Name)
                .unwrap_or_default()
        );
    }

    //Console.WriteLine("\nSent items:");
    log_line += "\nSent items:\n";
    let mut sent_items = Vec::new();
    for item_id in item_ids {
        let Some(item) = container::get_inventory_item(w, this, ObjectGuid::new(*item_id)) else {
            log_line += &format!("Couldn't find inventory item {item_id:08X}\n");
            continue;
        };
        let stack_str = obj(w, item)
            .stack_size()
            .filter(|&s| s > 1)
            .map_or_else(String::new, |s| format!("{s} "));
        log_line += &format!("{stack_str}{} ({})\n", name(w, item), item);

        if is_trading(w, this) && is_being_traded(w, this, item) {
            log_line += &format!(
                "{stack_str}{} ({}) is currently being traded, skipping.\n",
                name(w, item),
                item
            );
            continue;
        }
        sent_items.push(item);
    }
    log_line += "\n";
    log::info!("{log_line}");

    // compare list of input items
    // to required items for purchase
    has_items(w, &sent_items, &buy_items)
}

/// Returns TRUE if player inventory contains the required items to purchase house
// ACE: Player.HasItems
#[must_use]
pub fn has_items(w: &World, sent_items: &[ObjectGuid], buy_items: &[WorldObject]) -> bool {
    // requires: no duplicate individual items in list,
    // ie. items have already been stacked
    for buy_item in buy_items {
        // special handling for currency
        let buy_name = buy_item
            .get_property(empyrean_entity::enums::PropertyString::Name)
            .expect("System.NullReferenceException: buyItem.Name");
        if buy_name == "Pyreal" {
            if !has_currency(
                w,
                sent_items,
                buy_item.stack_size().unwrap_or(1).cast_unsigned(),
                true,
            ) {
                return false;
            }
        } else if !has_item(w, sent_items, buy_item) {
            return false;
        }
    }
    true
}

/// Returns TRUE if player inventory contains an item required to purchase house
// ACE: Player.HasItem
#[must_use]
pub fn has_item(w: &World, sent_items: &[ObjectGuid], buy_item: &WorldObject) -> bool {
    let buy_name = buy_item
        .get_property(empyrean_entity::enums::PropertyString::Name)
        .unwrap_or_default();
    let stack_str = buy_item
        .stack_size()
        .filter(|&s| s > 1)
        .map_or_else(String::new, |s| format!("{s} "));
    //Console.WriteLine($"Checking for item: {stackStr}{buyItem.Name}");
    log::info!("[HOUSE] Checking for item: {stack_str}{buy_name}");

    // get all items of this wcid from inventory
    let item_matches: Vec<ObjectGuid> = sent_items
        .iter()
        .copied()
        .filter(|&i| obj(w, i).biota.weenie_class_id == buy_item.biota.weenie_class_id)
        .collect();
    let total_stack = item_matches.iter().fold(0i32, |a, &i| {
        a.checked_add(obj(w, i).stack_size().unwrap_or(1))
            .expect("System.OverflowException: Sum")
    });

    if item_matches.is_empty() {
        //Console.WriteLine("No matching items found.");
        log::info!("[HOUSE] No matching items found.");
        return false;
    }
    let required = buy_item.stack_size().unwrap_or(1);
    if total_stack < required {
        //Console.WriteLine($"Found {totalStack} items, requires {required}.");
        log::info!("[HOUSE] Found {total_stack} items, requires {required}.");
        return false;
    }
    true
}

/// Determines if a player has at least some amount of currency in their inventory
// ACE: Player.HasCurrency
#[must_use]
pub fn has_currency(
    w: &World,
    sent_items: &[ObjectGuid],
    amount: u32,
    use_trade_notes: bool,
) -> bool {
    //Console.WriteLine($"Checking for currency: {amount}");
    log::info!("[HOUSE] Checking for currency: {amount}");
    let total_currency = get_total_currency(w, sent_items, use_trade_notes);
    total_currency >= amount
}

/// Returns the total amount of currency in the player's inventory
// ACE: Player.GetTotalCurrency
#[must_use]
pub fn get_total_currency(w: &World, sent_items: &[ObjectGuid], use_trade_notes: bool) -> u32 {
    let total_pyreals = get_total_pyreals(w, sent_items);
    //Console.WriteLine($"Total pyreals: {totalPyreals}");
    log::info!("[HOUSE] Total pyreals: {total_pyreals}");
    if !use_trade_notes {
        return total_pyreals;
    }

    let total_trade_notes = get_total_trade_notes(w, sent_items);
    //Console.WriteLine($"Total trade notes: {totalTradeNotes}");
    log::info!("[HOUSE] Total trade notes: {total_trade_notes}");

    total_pyreals.wrapping_add(total_trade_notes)
}

// ACE: Player.GetTotalPyreals
#[must_use]
pub fn get_total_pyreals(w: &World, sent_items: &[ObjectGuid]) -> u32 {
    let mut total_pyreals: u32 = 0;
    for &coin_stack in sent_items
        .iter()
        .filter(|&&i| obj(w, i).biota.weenie_class_id == 273)
    {
        // pyreals
        total_pyreals =
            total_pyreals.wrapping_add(obj(w, coin_stack).value().unwrap_or(0).cast_unsigned());
    }

    total_pyreals
}

// ACE: Player.GetTotalTradeNotes
#[must_use]
pub fn get_total_trade_notes(w: &World, sent_items: &[ObjectGuid]) -> u32 {
    let mut total_value: u32 = 0;
    for &trade_note in sent_items.iter().filter(|&&i| {
        house::weenie_class_name(w, obj(w, i).biota.weenie_class_id).starts_with("tradenote")
    }) {
        total_value =
            total_value.wrapping_add(obj(w, trade_note).value().unwrap_or(0).cast_unsigned());
    }

    total_value
}

// ================================================================================ owner lookups

// ACE: Player.GetHouseOwner
pub fn get_house_owner(w: &World, this: ObjectGuid) -> Option<IPlayer> {
    // if this character owns a house, always use that
    if obj(w, this).house_instance().is_some() {
        return Some(IPlayer::Online(this));
    }

    // if server is running house_per_char mode (non-default),
    // only use the HouseInstance for the current character
    if property_manager::get_bool(w, "house_per_char", false, true).item {
        return Some(IPlayer::Online(this));
    }

    // else return the account house owner
    get_account_house_owner(w, this)
}

// ACE: Player.GetHouseInstance
#[must_use]
pub fn get_house_instance(w: &World, this: ObjectGuid) -> Option<u32> {
    get_house_owner(w, this).and_then(|o| i_player::house_instance(w, o))
}

// ACE: Player.HandleActionQueryHouse
pub fn handle_action_query_house(w: &mut World, this: ObjectGuid) {
    let house_owner = get_house_owner(w, this);

    let house_instance = house_owner.and_then(|o| i_player::house_instance(w, o));

    // no house owned - send 0x226 HouseStatus?
    let Some(house_instance) = house_instance else {
        let m = event(w, this, |d| {
            game_event_house_status(d, WeenieError::BadParam)
        });
        send(w, this, m);
        return;
    };

    // house owned - send 0x225 HouseData?
    if house(w, this).is_none() {
        load_house(w, this, Some(house_instance), false);
    }

    house_manager::get_house(
        w,
        house_instance,
        Box::new(move |w: &mut World, house: ObjectGuid| {
            let house_data = house::get_house_data(w, house, house_owner);
            let m = event(w, this, |d| game_event_house_data(d, &house_data));
            send(w, this, m);
        }),
    );
}

// ACE: Player.LoadHouse
pub fn load_house(
    w: &mut World,
    this: ObjectGuid,
    house_instance: Option<u32>,
    force_load: bool,
) -> Option<ObjectGuid> {
    if house(w, this).is_some() && !force_load {
        return house(w, this);
    }

    let Some(house_instance) = house_instance else {
        return house(w, this);
    };

    let h = house::load(w, house_instance, false);
    set_house(w, this, h);

    h
}

// ACE: Player.GetHouse
pub fn get_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let house_instance = get_house_instance(w, this);

    get_house_by_instance(w, this, house_instance)
}

// ACE: Player.GetHouse
pub fn get_house_by_instance(
    w: &mut World,
    this: ObjectGuid,
    house_instance: Option<u32>,
) -> Option<ObjectGuid> {
    let house_guid = house_instance?;
    let landblock_id = house::landblock_id_of_house_guid(house_guid);
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    let h = if is_loaded {
        landblock::get_object(w, landblock_id, ObjectGuid::new(house_guid), true)
            .filter(|&g| obj(w, g).is_house())
    } else {
        house::load(w, house_guid, false)
    };
    set_house(w, this, h);
    h
}

// ================================================================================ guests

// ACE: Player.HandleActionAddGuest
pub fn handle_action_add_guest(w: &mut World, this: ObjectGuid, guest_name: &str) {
    //Console.WriteLine($"{Name}.HandleActionAddGuest({guestName})");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let house = get_house(w, this);
    let (guest, is_online) = player_manager::find_by_name(w, guest_name);

    let Some(guest) = guest else {
        broadcast(w, this, &format!("{guest_name} not found"));
        return;
    };

    if i_player_guid_equals(guest, this) {
        broadcast(w, this, "You already have access to your house.");
        return;
    }

    let account_players = player_manager::get_account_players(w, account_id(w, this))
        .cloned()
        .unwrap_or_default();

    let guests = guests_of_house(w, this);
    if guests.contains_key(&guest.guid()) || account_players.contains_key(&guest.guid().full()) {
        broadcast(
            w,
            this,
            &format!("{} is already on your guest list.", ip_name(w, guest)),
        );
        return;
    }

    if guests.len() == house::MAX_GUESTS {
        broadcast(
            w,
            this,
            &format!(
                "Your guest list has already reached the maximum limit ({})",
                house::MAX_GUESTS
            ),
        );
        return;
    }

    house::add_guest(
        w,
        house.expect("System.NullReferenceException: house"),
        guest,
        false,
    );

    broadcast(
        w,
        this,
        &format!("{} has been added to your guest list.", ip_name(w, guest)),
    );

    // notify online guest for addition
    if is_online {
        let online_guest = player_manager::get_online_player(w, guest.guid().full())
            .expect("System.NullReferenceException: onlineGuest");
        broadcast(
            w,
            online_guest,
            &format!("{} has added you to their house guest list.", name(w, this)),
        );
    }
}

// ACE: Player.HandleActionRemoveGuest
pub fn handle_action_remove_guest(
    w: &mut World,
    this: ObjectGuid,
    guest_name: &str,
    send_msg_to_owner: bool,
) {
    //Console.WriteLine($"{Name}.HandleActionRemoveGuest({guestName})");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let house = get_house(w, this).expect("System.NullReferenceException: house");
    let (guest, is_online) = player_manager::find_by_name(w, guest_name);

    let Some(guest) = guest else {
        broadcast(w, this, &format!("{guest_name} not found"));
        return;
    };

    if i_player_guid_equals(guest, this) {
        broadcast(w, this, "You have permanent access to your house.");
        return;
    }

    if !guests_of_house(w, this).contains_key(&guest.guid()) {
        broadcast(
            w,
            this,
            &format!("{} is not on your guest list.", ip_name(w, guest)),
        );
        return;
    }

    house::remove_guest(w, house, guest);

    if send_msg_to_owner {
        broadcast(
            w,
            this,
            &format!("{} removed from your guest list.", ip_name(w, guest)),
        );
    }

    // notify online guest removed
    if is_online {
        let online_guest = player_manager::get_online_player(w, guest.guid().full())
            .expect("System.NullReferenceException: onlineGuest");
        broadcast(
            w,
            online_guest,
            &format!(
                "{} has removed you from their house guest list.",
                name(w, this)
            ),
        );

        // if guest access is removed while player is in house,
        // they will be stuck in restriction space
        if house::on_property(w, house, online_guest) {
            let online_guest_name = name(w, online_guest);
            handle_action_boot(w, this, &online_guest_name, false);
        }
    }
}

// ACE: Player.HandleActionRemoveAllGuests
pub fn handle_action_remove_all_guests(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionRemoveAllGuests()");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let _house = get_house(w, this);

    let guests = guests_of_house(w, this);
    if guests.is_empty() {
        broadcast(w, this, "Your house guest list is empty.");
        return;
    }

    for guid in guests.keys().copied().collect::<Vec<_>>() {
        let (guest, _) = player_manager::find_by_guid(w, guid.full());
        let guest_name = ip_name(w, guest.expect("System.NullReferenceException: guest"));
        handle_action_remove_guest(w, this, &guest_name, false);
    }

    broadcast(w, this, "You have removed all the guests from your house.");
}

// ACE: Player.HandleActionGuestList
pub fn handle_action_guest_list(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionGuestList()");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let Some(house) = get_house(w, this) else {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    };

    let session = player_manager::player_session(w, this)
        .expect("System.NullReferenceException: Player.Session");
    let m = game_event_update_har(w, session, house);
    enqueue_send(w, session, m);
}

// ACE: Player.HandleActionSetOpenStatus
pub fn handle_action_set_open_status(w: &mut World, this: ObjectGuid, open_status: bool) {
    //Console.WriteLine($"{Name}.HandleActionSetOpenStatus({openStatus})");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let house = get_house(w, this).expect("System.NullReferenceException: house");

    if open_status == house::open_status(w, house) {
        if open_status {
            broadcast(w, this, "You already have an open house.");
        } else {
            broadcast(w, this, "Your house is already closed to the public.");
        }

        return;
    }

    house::set_open_status(w, house, open_status);
    {
        let h = obj_mut(w, house);
        let v = house_open_status_of(h);
        h.biota.set_property(PropertyBool::Open, v);
        h.wo.world_object_database.changes_detected = true;
    }
    house::update_restriction_db(w, house, None);

    if open_status {
        broadcast(w, this, "Your house is now open to the public.");
    } else {
        broadcast(w, this, "Your house is now closed to the public.");

        // boot anyone not on the guest list,
        // else they will be stuck in restricted space
        handle_action_boot_all(w, this, false);
    }

    if obj(w, house).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, house, true);
    }
}

fn house_open_status_of(h: &WorldObject) -> bool {
    h.open_to_everyone()
}

// ACE: Player.HandleActionSetHooksVisible
pub fn handle_action_set_hooks_visible(w: &mut World, this: ObjectGuid, visible: bool) {
    //Console.WriteLine($"{Name}.HandleActionSetHooksVisible({visible})");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let visible_str = if visible { "visible" } else { "invisible" };
    broadcast(w, this, &format!("Your hooks are set to {visible_str}."));

    let house = get_house(w, this).expect("System.NullReferenceException: house");

    if visible == obj(w, house).house_hooks_visible().unwrap_or(true) {
        return;
    }

    obj_mut(w, house).set_house_hooks_visible(Some(visible));

    let hooks: Vec<ObjectGuid> = house::hooks(w, house)
        .into_iter()
        .filter(|&h| container::inventory_values(w, h).is_empty())
        .collect();
    for hook in hooks {
        crate::world_objects::hook::update_hook_visibility(w, hook);
    }

    // if house has dungeon, repeat this process
    if house::has_dungeon(w, house) {
        let Some(dungeon_house) = house::get_dungeon_house(w, house) else {
            return;
        };

        obj_mut(w, dungeon_house).set_house_hooks_visible(Some(visible));

        let hooks: Vec<ObjectGuid> = house::hooks(w, dungeon_house)
            .into_iter()
            .filter(|&h| container::inventory_values(w, h).is_empty())
            .collect();
        for hook in hooks {
            crate::world_objects::hook::update_hook_visibility(w, hook);
        }

        if obj(w, dungeon_house).current_landblock.is_none() {
            dispatch::save_biota_to_database::save_biota_to_database(w, dungeon_house, true);
        }
    }

    if obj(w, house).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, house, true);
    }
}

// ACE: Player.HandleActionModifyStorage
pub fn handle_action_modify_storage(
    w: &mut World,
    this: ObjectGuid,
    guest_name: &str,
    has_permission: bool,
    send_msg_to_owner: bool,
) {
    //Console.WriteLine($"{Name}.HandleActionModifyStorage({guestName}, {hasPermission})");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let house = get_house(w, this).expect("System.NullReferenceException: house");
    let (storage, is_online) = player_manager::find_by_name(w, guest_name);

    let Some(storage) = storage else {
        broadcast(w, this, &format!("{guest_name} not found"));
        return;
    };

    if i_player_guid_equals(storage, this) {
        broadcast(w, this, "You have permanent access to your house storage.");
        return;
    }

    let guests = guests_of_house(w, this);
    let storage_access = guests.get(&storage.guid()).copied();
    let existing = storage_access.is_some();
    let storage_access = storage_access.unwrap_or(false);
    if has_permission {
        if existing {
            if storage_access {
                broadcast(
                    w,
                    this,
                    &format!(
                        "{} already has access to your home's storage.",
                        ip_name(w, storage)
                    ),
                );
                return;
            }
            house::modify_guest(w, house, storage, true);
        } else {
            if guests.len() == house::MAX_GUESTS {
                broadcast(
                    w,
                    this,
                    &format!(
                        "Your guest list has already reached the maximum limit ({})",
                        house::MAX_GUESTS
                    ),
                );
                return;
            }
            house::add_guest(w, house, storage, true);
        }

        let and_str = if existing { "" } else { "and " };
        if send_msg_to_owner {
            broadcast(
                w,
                this,
                &format!(
                    "You have granted {} access to your home's storage.  This is denoted by  the asterisk next to their name in the guest list.",
                    ip_name(w, storage)
                ),
            ); // spacing from PCAP
        }

        // notify online storage guest added
        if is_online {
            let online_guest = player_manager::get_online_player(w, storage.guid().full())
                .expect("System.NullReferenceException: onlineGuest");
            broadcast(
                w,
                online_guest,
                &format!(
                    "{} has granted you access to their house {and_str}storage.",
                    name(w, this)
                ),
            );
        }
    } else {
        if !existing || !storage_access {
            let storage_str = if existing { " storage" } else { "" };
            broadcast(
                w,
                this,
                &format!(
                    "{} doesn't have access to your house{storage_str}.",
                    ip_name(w, storage)
                ),
            );
            return;
        }

        house::modify_guest(w, house, storage, false);

        broadcast(
            w,
            this,
            &format!(
                "{} no longer has access to your house storage.",
                ip_name(w, storage)
            ),
        );

        // notify online storage guest added
        if is_online {
            let online_guest = player_manager::get_online_player(w, storage.guid().full())
                .expect("System.NullReferenceException: onlineGuest");
            broadcast(
                w,
                online_guest,
                &format!(
                    "{} has revoked access to their house storage.",
                    name(w, this)
                ),
            );

            // if they are in house, and have storage opened?
        }
    }
}

// ACE: Player.HandleActionAllStorage
pub fn handle_action_all_storage(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionAllStorage()");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let _house = get_house(w, this);

    let guests = guests_of_house(w, this);
    if guests.is_empty() {
        broadcast(w, this, "Your house guest list is empty.");
        return;
    }

    let no_storage: Vec<ObjectGuid> = guests
        .iter()
        .filter(|(_, v)| !**v)
        .map(|(k, _)| *k)
        .collect();

    if no_storage.is_empty() {
        broadcast(
            w,
            this,
            "Your house guests already have access to your storage.",
        );
        return;
    }

    for guid in no_storage {
        let (guest, _) = player_manager::find_by_guid(w, guid.full());
        let guest_name = ip_name(w, guest.expect("System.NullReferenceException: guest"));
        handle_action_modify_storage(w, this, &guest_name, true, false);
    }

    broadcast(
        w,
        this,
        "You grant item storage permission to all your guests",
    );
}

// ACE: Player.HandleActionRemoveAllStorage
pub fn handle_action_remove_all_storage(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionRemoveAllStorage()");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let _house = get_house(w, this);

    let guests = guests_of_house(w, this);
    if guests.is_empty() {
        broadcast(w, this, "Your house guest list is empty.");
        return;
    }

    let storage: Vec<ObjectGuid> = guests
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect();

    if storage.is_empty() {
        broadcast(w, this, "Your house guests don't have storage access.");
        return;
    }

    for guid in storage {
        let (guest, _) = player_manager::find_by_guid(w, guid.full());
        let guest_name = ip_name(w, guest.expect("System.NullReferenceException: guest"));
        handle_action_modify_storage(w, this, &guest_name, false, false);
    }

    broadcast(
        w,
        this,
        "You remove item storage permission from all your guests",
    );
}

// ACE: Player.HandleActionBoot
pub fn handle_action_boot(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
    allegiance_house: bool,
) {
    //Console.WriteLine($"{Name}.HandleActionBoot({playerName})");
    if house(w, this).is_none() && !allegiance_house {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    let house = if allegiance_house {
        let allegiance = allegiance::player_allegiance(w, this)
            .expect("System.NullReferenceException: Allegiance");
        allegiance::get_house(w, allegiance)
    } else {
        get_house(w, this)
    };
    let player = player_manager::get_online_player_by_name(w, player_name);

    let Some(player) = player else {
        broadcast(w, this, &format!("{player_name} is not online."));
        return;
    };

    // is this player in the house landcell?
    let house = house.expect("System.NullReferenceException: house");
    let owner = if allegiance_house {
        "allegiance"
    } else {
        "your"
    };
    if !house::on_property(w, house, player) {
        broadcast(
            w,
            this,
            &format!("{} is not on {owner} property.", name(w, player)),
        );
        return;
    }

    // play script?
    let boot_spot =
        house::boot_spot(w, house).expect("System.NullReferenceException: house.BootSpot");
    let location = obj(w, boot_spot)
        .location()
        .expect("System.NullReferenceException: BootSpot.Location");
    player_location::teleport(w, player, &location, false);

    let owner = if allegiance_house {
        "the allegiance"
    } else {
        "your"
    };
    broadcast(
        w,
        this,
        &format!("Booted {} from {owner} house.", name(w, player)),
    );

    let owner = if allegiance_house {
        "the allegiance"
    } else {
        "their"
    };
    broadcast(
        w,
        player,
        &format!("{} has booted you from {owner} house.", name(w, this)),
    );
}

// ACE: Player.HandleActionBootAll
pub fn handle_action_boot_all(w: &mut World, this: ObjectGuid, guests: bool) {
    //Console.WriteLine($"{Name}.HandleActionBootAll()");
    if house(w, this).is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    // since it can be an open house, the guest list wouldn't be enough here?
    let house = get_house(w, this).expect("System.NullReferenceException: house");

    let booted = house::boot_all(w, house, this, guests, false);

    if guests && booted == 0 {
        let else_str = if house::on_property(w, house, this) {
            "else "
        } else {
            ""
        };
        broadcast(
            w,
            this,
            &format!("There is no one {else_str}on your property."),
        );
    }
}

/// Called when player is exiting portal space
///
/// Returns TRUE if player was booted from house property they don't have access to, upon exiting portal space
// ACE: Player.CheckHouse
pub fn check_house(w: &mut World, this: ObjectGuid) -> bool {
    let Some(lb) = obj(w, this).current_landblock else {
        return false;
    };
    if obj(w, this).ignore_house_barriers() {
        return false;
    }

    let houses = w
        .landblock_manager
        .landblocks
        .get(lb)
        .map(|l| l.houses.clone())
        .unwrap_or_default();
    for house in houses {
        let root_house =
            house::root_house(w, house).expect("System.NullReferenceException: RootHouse");

        if !house::on_property(w, root_house, this) {
            continue;
        }

        if obj(w, root_house).house_owner().is_some()
            && !house::has_permission(w, root_house, this, false)
        {
            let has_dungeon = w
                .landblock_manager
                .landblocks
                .get_mut(lb)
                .is_some_and(|l| l.has_dungeon());
            if !obj(w, root_house).is_open()
                || (obj(w, root_house).house_type() != HouseType::Apartment && has_dungeon)
            {
                let boot_spot = house::boot_spot(w, root_house)
                    .expect("System.NullReferenceException: rootHouse.BootSpot");
                let location = obj(w, boot_spot)
                    .location()
                    .expect("System.NullReferenceException: BootSpot.Location");
                player_location::teleport(w, this, &location, false);
                return true;
            }
        }

        //if (rootHouse.HouseOwner == null && rootHouse.HouseType != HouseType.Apartment && CurrentLandblock.HasDungeon)
        //{
        //    Teleport(rootHouse.BootSpot.Location);
        //    return true;
        //}
    }
    false
}

/// Handles the @hslist \[housetype\] command
// ACE: Player.HandleActionListAvailable
pub fn handle_action_list_available(w: &mut World, this: ObjectGuid, house_type: HouseType) {
    //Console.WriteLine($"{Name}.HandleActionListAvailable({houseType})");

    let locations = house_list::get_available_locations(w, house_type);
    let mut unique: Vec<u32> = Vec::new();
    for l in &locations {
        if !unique.contains(l) {
            unique.push(*l);
        }
    }

    let total = i32::try_from(locations.len()).unwrap_or(i32::MAX);
    let m = event(w, this, |d| {
        game_event_house_available_houses(d, house_type, &unique, total)
    });
    send(w, this, m);
}

// ================================================================================ allegiance permissions

// ACE: Player.HandleActionModifyAllegianceGuestPermission
pub fn handle_action_modify_allegiance_guest_permission(
    w: &mut World,
    this: ObjectGuid,
    add: bool,
) {
    //Console.WriteLine($"{Name}.HandleActionModifyAllegianceGuestPermission({add})");
    let house_instance = get_house_instance(w, this);

    if house_instance.is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    // only the character who directly owns the house can use /house guest add_allegiance / remove_allegiance
    if obj(w, this).house_instance().is_none() {
        weenie_error(w, this, WeenieError::OnlyHouseOwnerCanUseCommand);
        return;
    }

    // check if player is in an allegiance
    let Some(allegiance) = allegiance::player_allegiance(w, this) else {
        weenie_error(w, this, WeenieError::YouAreNotInAllegiance);
        return;
    };

    let house = get_house(w, this).expect("System.NullReferenceException: house");
    let monarch = allegiance::monarch_player(w, allegiance)
        .expect("System.NullReferenceException: Allegiance.Monarch");

    if add {
        if obj(w, house).monarch_id().is_some() {
            broadcast(w, this, "The monarchy already has access to your dwelling.");
            return;
        }

        let guests = guests_of_house(w, this);
        if guests.len() == house::MAX_GUESTS {
            broadcast(
                w,
                this,
                &format!(
                    "Your guest list has already reached the maximum limit ({})",
                    house::MAX_GUESTS
                ),
            );
            return;
        }

        let monarch_id = allegiance::monarch_id(w, this);
        obj_mut(w, house).set_monarch_id(monarch_id);

        if guests.contains_key(&monarch.guid()) {
            house::modify_guest(w, house, monarch, false); // handle case: the monarch already has guest/storage access already, now adding allegiance
        } else {
            house::add_guest(w, house, monarch, false);
        }

        broadcast(
            w,
            this,
            "You have granted your monarchy access to your dwelling.",
        );
    } else {
        if obj(w, house).monarch_id().is_none() {
            broadcast(
                w,
                this,
                "The monarchy did not have access to your dwelling.",
            );
            return;
        }

        obj_mut(w, house).set_monarch_id(None);

        house::remove_guest(w, house, monarch);

        handle_action_boot_all(w, this, false); // boot anyone who doesn't have guest access

        broadcast(
            w,
            this,
            "You have revoked access to your dwelling to your monarchy.",
        );
    }
}

// ACE: Player.HandleActionModifyAllegianceStoragePermission
pub fn handle_action_modify_allegiance_storage_permission(
    w: &mut World,
    this: ObjectGuid,
    add: bool,
) {
    //Console.WriteLine($"{Name}.HandleActionModifyAllegianceStoragePermission({add})");
    let house_instance = get_house_instance(w, this);

    if house_instance.is_none() {
        weenie_error(w, this, WeenieError::YouMustOwnHouseToUseCommand);
        return;
    }

    // only the character who directly owns the house can use /house storage add_allegiance / remove_allegiance
    if obj(w, this).house_instance().is_none() {
        weenie_error(w, this, WeenieError::OnlyHouseOwnerCanUseCommand);
        return;
    }

    // check if player is in an allegiance
    let Some(allegiance) = allegiance::player_allegiance(w, this) else {
        weenie_error(w, this, WeenieError::YouAreNotInAllegiance);
        return;
    };

    let house = get_house(w, this).expect("System.NullReferenceException: house");
    let monarch = allegiance::monarch_player(w, allegiance)
        .expect("System.NullReferenceException: Allegiance.Monarch");
    let guests = guests_of_house(w, this);
    let house_monarch = obj(w, house).monarch_id();

    if add {
        if house_monarch.is_some_and(|m| guests.get(&ObjectGuid::new(m)).copied() == Some(true)) {
            broadcast(
                w,
                this,
                "The monarchy already has storage access in your dwelling.",
            );
            return;
        }

        if house_monarch.is_none() && guests.len() == house::MAX_GUESTS {
            broadcast(
                w,
                this,
                &format!(
                    "Your guest list has already reached the maximum limit ({})",
                    house::MAX_GUESTS
                ),
            );
            return;
        }

        let monarch_id = allegiance::monarch_id(w, this);
        obj_mut(w, house).set_monarch_id(monarch_id);

        if guests.contains_key(&monarch.guid()) {
            house::modify_guest(w, house, monarch, true); // handle case: the monarch already has guest/storage access already, now adding allegiance
        } else {
            house::add_guest(w, house, monarch, true);
        }

        broadcast(
            w,
            this,
            "You have granted your monarchy access to your storage.",
        );
    } else {
        if house_monarch.is_none_or(|m| guests.get(&ObjectGuid::new(m)).copied() == Some(false)) {
            broadcast(
                w,
                this,
                "The monarchy did not have storage access to your dwelling.",
            );
            return;
        }

        // downgrade to guest access
        house::modify_guest(w, house, monarch, false);

        broadcast(w, this, "You have revoked storage access to your monarchy.");
    }
}

// ================================================================================ /allegiance house commands

// ACE: Player.HandleActionDoAllegianceHouseAction
pub fn handle_action_do_allegiance_house_action(
    w: &mut World,
    this: ObjectGuid,
    action: AllegianceHouseAction,
) {
    //Console.WriteLine($"{Name}.DoAllegianceHouseAction({action})");

    let Some(allegiance) = allegiance::player_allegiance(w, this) else {
        weenie_error(w, this, WeenieError::YouAreNotInAllegiance);
        return;
    };

    if allegiance::permission_level(w, this)
        < empyrean_entity::enums::AllegiancePermissionLevel::Castellan
    {
        weenie_error(w, this, WeenieError::YouDoNotHaveAuthorityInAllegiance);
        return;
    }

    let Some(allegiance_house) = allegiance::get_house(w, allegiance) else {
        weenie_error(w, this, WeenieError::YourMonarchDoesNotOwnAMansionOrVilla);
        return;
    };

    let house_type = obj(w, allegiance_house).house_type();
    if house_type != HouseType::Villa && house_type != HouseType::Mansion {
        weenie_error(w, this, WeenieError::YourMonarchsHouseIsNotAMansionOrVilla);
        return;
    }

    if action == AllegianceHouseAction::Help {
        let help = "Note: You may substitute a forward slash(/) for the at symbol(@).\n".to_owned()
            + "@allegiance house guest open - Adds your allegiance to the allegiance house guest list.\n"
            + "@allegiance house guest close - Removes your allegiance from the allegiance house guest list.\n"
            + "@allegiance house storage open - Adds your allegiance to the allegiance house storage list.\n"
            + "@allegiance house storage close - Removes your allegiance from the allegiance house storage list.";

        let status = if obj(w, allegiance_house).monarch_id().is_none() {
            "\nYour monarchy currently does not have guest or storage access to allegiance housing."
        } else {
            let monarch = allegiance::monarch_player(w, allegiance)
                .expect("System.NullReferenceException: Allegiance.Monarch");
            let storage = house::fields(w, allegiance_house)
                .guests
                .get(&monarch.guid())
                .copied()
                .unwrap_or(false);
            if storage {
                "\nYour monarchy currently has guest and storage access to allegiance housing."
            } else {
                "\nYour monarchy currently has guest access to allegiance housing."
            }
        };

        broadcast(w, this, &(help + status));
        return;
    }

    match action {
        AllegianceHouseAction::GuestOpen => {
            handle_action_do_allegiance_house_action_guest_open(w, this, allegiance_house)
        }
        AllegianceHouseAction::GuestClose => {
            handle_action_do_allegiance_house_action_guest_close(w, this, allegiance_house)
        }
        AllegianceHouseAction::StorageOpen => {
            handle_action_do_allegiance_house_action_storage_open(w, this, allegiance_house)
        }
        AllegianceHouseAction::StorageClose => {
            handle_action_do_allegiance_house_action_storage_close(w, this, allegiance_house)
        }
        _ => {}
    }

    if obj(w, allegiance_house).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, allegiance_house, true);
    }
}

fn monarch_of(w: &World, this: ObjectGuid) -> IPlayer {
    let allegiance =
        allegiance::player_allegiance(w, this).expect("System.NullReferenceException: Allegiance");
    allegiance::monarch_player(w, allegiance)
        .expect("System.NullReferenceException: Allegiance.Monarch")
}

// ACE: Player.HandleActionDoAllegianceHouseAction_GuestOpen
pub fn handle_action_do_allegiance_house_action_guest_open(
    w: &mut World,
    this: ObjectGuid,
    allegiance_house: ObjectGuid,
) {
    if obj(w, allegiance_house).monarch_id().is_some() {
        broadcast(
            w,
            this,
            "The monarchy already has access to the allegiance dwelling.",
        );
        return;
    }

    if house::fields(w, allegiance_house).guests.len() == house::MAX_GUESTS {
        broadcast(
            w,
            this,
            &format!(
                "The allegiance house guest list has already reached the maximum limit ({})",
                house::MAX_GUESTS
            ),
        );
        return;
    }

    let monarch_id = allegiance::monarch_id(w, this);
    obj_mut(w, allegiance_house).set_monarch_id(monarch_id);

    // AddHouseGuest
    let monarch = monarch_of(w, this);
    house::add_guest(w, allegiance_house, monarch, false);

    broadcast(
        w,
        this,
        "You have granted your monarchy access to the allegiance dwelling.",
    );
}

// ACE: Player.HandleActionDoAllegianceHouseAction_GuestClose
pub fn handle_action_do_allegiance_house_action_guest_close(
    w: &mut World,
    this: ObjectGuid,
    allegiance_house: ObjectGuid,
) {
    if obj(w, allegiance_house).monarch_id().is_none() {
        broadcast(
            w,
            this,
            "The monarchy already does not have access to the allegiance dwelling.",
        );
        return;
    }

    obj_mut(w, allegiance_house).set_monarch_id(None);

    // RemoveHouseGuest
    let monarch = monarch_of(w, this);
    house::remove_guest(w, allegiance_house, monarch);

    let _booted = house::boot_all(w, allegiance_house, this, false, true);

    broadcast(
        w,
        this,
        "You have revoked allegiance access to the allegiance dwelling.",
    );
}

// ACE: Player.HandleActionDoAllegianceHouseAction_StorageOpen
pub fn handle_action_do_allegiance_house_action_storage_open(
    w: &mut World,
    this: ObjectGuid,
    allegiance_house: ObjectGuid,
) {
    let house_monarch = obj(w, allegiance_house).monarch_id();
    let guests = house::fields(w, allegiance_house).guests.clone();
    if house_monarch.is_some_and(|m| guests.get(&ObjectGuid::new(m)).copied() == Some(true)) {
        broadcast(
            w,
            this,
            "The monarchy already has storage access in the allegiance dwelling.",
        );
        return;
    }

    if house_monarch.is_none() && guests.len() == house::MAX_GUESTS {
        broadcast(
            w,
            this,
            &format!(
                "Your allegiance house guest list has already reached the maximum limit ({})",
                house::MAX_GUESTS
            ),
        );
        return;
    }

    let monarch_id = allegiance::monarch_id(w, this);
    obj_mut(w, allegiance_house).set_monarch_id(monarch_id);

    // AddHouseGuest
    let monarch = monarch_of(w, this);
    if guests.contains_key(&monarch.guid()) {
        // handle guest -> storage access upgrade
        house::modify_guest(w, allegiance_house, monarch, true);
    } else {
        house::add_guest(w, allegiance_house, monarch, true);
    }

    broadcast(
        w,
        this,
        "You have granted your monarchy access to allegiance storage.",
    );
}

// ACE: Player.HandleActionDoAllegianceHouseAction_StorageClose
pub fn handle_action_do_allegiance_house_action_storage_close(
    w: &mut World,
    this: ObjectGuid,
    allegiance_house: ObjectGuid,
) {
    let house_monarch = obj(w, allegiance_house).monarch_id();
    let guests = house::fields(w, allegiance_house).guests.clone();
    if house_monarch.is_none_or(|m| guests.get(&ObjectGuid::new(m)).copied() == Some(false)) {
        broadcast(
            w,
            this,
            "The monarchy already does not have storage access to the allegiance dwelling.",
        );
        return;
    }

    // ModifyHouseGuest - downgrade to guest access
    let monarch = monarch_of(w, this);
    house::modify_guest(w, allegiance_house, monarch, false);

    broadcast(
        w,
        this,
        "You have revoked your monarchy's access to the allegiance housing storage.",
    );
}

// ================================================================================ account houses

// ACE: Player.GetAccountHouseOwner
#[must_use]
pub fn get_account_house_owner(w: &World, this: ObjectGuid) -> Option<IPlayer> {
    // `PlayerManager.GetAccountPlayers` answers null for an unknown account; `.Values` then throws
    let account_players = player_manager::get_account_players(w, account_id(w, this))
        .expect("System.NullReferenceException: accountPlayers");

    let mut account_house_owners: Vec<IPlayer> = account_players
        .values()
        .copied()
        .filter(|&i| i_player::house_instance(w, i).is_some())
        .collect();

    // `OrderBy(i => i.HousePurchaseTimestamp)`: a stable sort, null first
    account_house_owners
        .sort_by_key(|&i| i_player::get_property(w, i, PropertyInt::HousePurchaseTimestamp));
    account_house_owners.first().copied()
}

// ACE: Player.GetAccountHouse
pub fn get_account_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    if let Some(house_instance) = obj(w, this).house_instance() {
        return get_house_by_instance(w, this, Some(house_instance));
    }

    // Not ACE's (fix, V295): ACE's null check is inverted (`if (accountHouseOwner != null) return null;`), so a character on an account whose house another character owns gets no house, and an account with no house has a null owner passed on. The owner comes from the mode-aware lookup the login uses instead: with one house per character only the character's own house counts (it has none here, so none), otherwise the account's house owner. In normal play this is not reached (the login loads the account's house into the house slot, which empties only on eviction); the results are ACE's in every reachable case.
    let house_owner = get_house_owner(w, this);

    //Console.WriteLine($"Account House Owner: {accountHouseOwner.Name}");

    get_house_of_player(w, house_owner)
}

// ACE: Player.GetHouse
pub fn get_house_of_player(w: &mut World, player: Option<IPlayer>) -> Option<ObjectGuid> {
    let house_guid = player.and_then(|p| i_player::house_instance(w, p))?;

    // is landblock loaded?
    let landblock_id = house::landblock_id_of_house_guid(house_guid);
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if is_loaded {
        return landblock::get_object(w, landblock_id, ObjectGuid::new(house_guid), true)
            .filter(|&g| obj(w, g).is_house());
    }

    // load an offline copy
    house::load(w, house_guid, false)
}

// ACE: Player.IsMultiHouseOwner
pub fn is_multi_house_owner(w: &mut World, this: ObjectGuid, show_msg: bool) -> bool {
    let character_houses = house_manager::get_character_houses(w, this.full());
    let account_houses = house_manager::get_account_houses(w, account_id(w, this));

    //if (showMsg)
    //Session.Network.EnqueueSend(new GameMessageSystemChat($"AccountHouses: {accountHouses.Count}, CharacterHouses: {characterHouses.Count}", ChatMessageType.Broadcast));

    if property_manager::get_bool(w, "house_per_char", false, true).item {
        // 1 house per character
        if character_houses.len() > 1 && show_msg {
            show_multi_house_warning(w, this, &character_houses, "character");
        }

        character_houses.len() > 1
    } else {
        // 1 house per account (retail default)
        if account_houses.len() > 1 && show_msg {
            show_multi_house_warning(w, this, &account_houses, "account");
        }

        account_houses.len() > 1
    }
}

// ACE: Player.GetMultiHouses
pub fn get_multi_houses(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    if property_manager::get_bool(w, "house_per_char", false, true).item {
        house_manager::get_character_houses(w, this.full())
    } else {
        house_manager::get_account_houses(w, account_id(w, this))
    }
}

// ACE: Player.ShowMultiHouseWarning
pub fn show_multi_house_warning(
    w: &mut World,
    this: ObjectGuid,
    houses: &[ObjectGuid],
    r#type: &str,
) {
    // this is a dangerous situation, and we want to clean it up asap
    broadcast(
        w,
        this,
        &format!(
            "Warning! You currently own {} different houses on this {}.",
            houses.len(),
            r#type
        ),
    );
    broadcast(w, this, &format!("Each {} is only allowed to own 1 house, so you will have to choose which house you want to keep.", r#type));
    broadcast(w, this, "You currently own houses at:");

    for (i, &house) in houses.iter().enumerate() {
        let Some(slumlord) = house::slum_lord(w, house) else {
            log::error!(
                "{}.IsMultiHouseOwner(): {} slumlord is null!",
                name(w, this),
                house
            );
            continue;
        };
        let location = obj(w, slumlord)
            .location()
            .expect("System.NullReferenceException: slumlord.Location");
        let coords = house_manager::get_coords(&location);
        broadcast(w, this, &format!("{}. {}", i + 1, coords));
    }
    broadcast(
        w,
        this,
        &format!(
            "Please choose the house you want to keep with /house-select # , where # is 1-{}",
            houses.len()
        ),
    );
}

/// When house_15day_account is true (retail server default),
/// new characters logging in on accounts less than 15 days old
/// have their HousePurchaseTimestamp set to 15 days before account creation.
///
/// This is for the House panel to show the correct date the character may purchase a house,
/// which the client automatically calculates as 30 days after HousePurchaseTimestamp
// ACE: Player.FifteenDaysBeforeAccountCreation
fn fifteen_days_before_account_creation(w: &World, this: ObjectGuid) -> i32 {
    let create_time = account(w, this).create_time;
    Time::get_unix_time_at(create_time.add_days(-15.0)).cs_cast()
}

/// Munges the HousePurchaseTimestamp for new accounts for correct display on House panel
// ACE: Player.ManageAccount15Days_HousePurchaseTimestamp
pub fn manage_account15_days_house_purchase_timestamp(w: &mut World, this: ObjectGuid) {
    // http://acpedia.org/wiki/Housing_FAQ#Purchase_timer

    if obj(w, this).house_rent_timestamp().is_some() {
        return;
    }

    if property_manager::get_bool(w, "house_15day_account", false, true).item
        && !obj(w, this).account15_days()
    {
        // this is set so the next purchase time displays properly on house tab
        let v = fifteen_days_before_account_creation(w, this);
        obj_mut(w, this).set_house_purchase_timestamp(Some(v));
    } else if obj(w, this).house_purchase_timestamp()
        == Some(fifteen_days_before_account_creation(w, this))
    {
        // account is now 15+ days old and still has not purchased a house, remove unneeded HousePurchaseTimestamp
        // also if server admin sets house_15day_account to false, this corrects the next purchase time on the House panel
        obj_mut(w, this).set_house_purchase_timestamp(None);
    }
}
