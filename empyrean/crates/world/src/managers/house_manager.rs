// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/HouseManager.cs
//! Port of `Source/ACE.Server/Managers/HouseManager.cs`.
//!
//! The rent queue holds house guids (`PlayerHouse.House`); `house.rs`'s module doc says how a
//! guid names the live house or its offline copy. The rent timer runs on `w.now` (virtual time in
//! tests). The offline-copy registry `House.Load` uses lives here too (not ACE).

use std::collections::BTreeMap;

use empyrean_common::clock::SnapshotClock;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{DotNetDict, TimeSpan};
use empyrean_common::performance::rate_limiter::RateLimiter;
use empyrean_entity::enums::{
    ChatMessageType, HouseStatus, HouseType, PositionType, PropertyBool, PropertyDataId,
    PropertyInstanceId, PropertyInt, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::entity::house_callback::{HouseAction, HouseCallback};
use crate::entity::house_list::{self, HouseListState};
use crate::entity::i_player::{self, IPlayer};
use crate::entity::player_house::PlayerHouse;
use crate::entity::position_extensions::get_map_coord_str;
use crate::managers::{landblock_manager, player_manager, property_manager, world_manager};
use crate::network::structure::house_data::{house_data_new, HouseData};
use crate::world_objects::{
    container, house, player_house, slum_lord, world_object_equipment, world_object_links,
};
use crate::{dispatch, World};

/// One offline copy made by `House.Load` (not ACE: see `house.rs`).
#[derive(Debug, Clone)]
pub struct OfflineCopy {
    /// Bumped per copy, so a slumlord inventory load that finishes after its copy left the store
    /// is dropped.
    pub generation: u64,
    /// The objects `House.Load` created (`linkedHouses`); their links and items hang off them.
    pub tops: Vec<ObjectGuid>,
}

/// The mutable static state of ACE's `HouseManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct HouseManagerState {
    /// A lookup table of HouseId => HouseGuid
    // ACE: HouseManager.HouseIdToGuid
    pub house_id_to_guid: Option<DotNetDict<u32, Vec<u32>>>,
    /// A list of all player-owned houses on the server,
    /// sorted by RentDue times (`null` until built)
    // ACE: HouseManager.RentQueue
    pub rent_queue: Option<BTreeMap<(DotNetDateTime, u32), PlayerHouse>>,
    /// HouseManager actions to run when slumlord inventory has completed loading
    // ACE: HouseManager.SlumlordCallbacks
    pub slumlord_callbacks: DotNetDict<u32, Vec<HouseCallback>>,
    /// The rate at which HouseManager.Tick() executes (created on first use)
    // ACE: HouseManager.updateHouseManagerRateLimiter
    pub update_house_manager_rate_limiter: Option<RateLimiter>,
    // ACE: HouseManager.TotalOwnedHousingByType
    pub total_owned_housing_by_type: DotNetDict<HouseType, i32>,
    /// `HouseList`'s static lists.
    pub house_list: HouseListState,
    /// `Hook.cachedHookReferences` (a static of `Hook.cs`), by weenie.
    // ACE: Hook.cachedHookReferences
    pub cached_hook_references:
        std::collections::HashMap<u32, crate::world_objects::hook::HookReference>,
    /// Not ACE: the offline copies `House.Load` put in the store, by house guid.
    pub offline: BTreeMap<ObjectGuid, OfflineCopy>,
    next_generation: u64,
}

// ================================================================================ offline copies (not ACE)

/// Registers the objects `House.Load` just created for `root`.
pub fn register_offline_copy(w: &mut World, root: ObjectGuid, tops: Vec<ObjectGuid>) {
    let hm = &mut w.house_manager;
    hm.next_generation += 1;
    let generation = hm.next_generation;
    hm.offline.insert(root, OfflineCopy { generation, tops });
}

/// Whether `house` is a registered offline copy.
#[must_use]
pub fn is_offline_copy(w: &World, house: ObjectGuid) -> bool {
    w.house_manager.offline.contains_key(&house)
}

/// The offline copies of houses whose guid lies on `landblock`.
#[must_use]
pub fn offline_copies_on_landblock(w: &World, landblock: u16) -> Vec<ObjectGuid> {
    w.house_manager
        .offline
        .keys()
        .copied()
        .filter(|g| (g.full() >> 12) & 0xFFFF == u32::from(landblock))
        .collect()
}

/// Removes an offline copy's objects (the created houses, their links, recursively, and every
/// container's items) from the store. Nothing is destroyed or deleted from the shard.
pub fn evict_offline_copy(w: &mut World, root: ObjectGuid) {
    let Some(copy) = w.house_manager.offline.remove(&root) else {
        return;
    };
    let mut stack = copy.tops;
    while let Some(g) = stack.pop() {
        let Some(o) = w.objects.get(g) else { continue };
        if o.current_landblock.is_some() {
            continue;
        }
        stack.extend(o.wo.world_object_links.child_links.iter().copied());
        if o.is_container() {
            stack.extend(container::inventory_values(w, g));
        }
        w.objects.remove(g);
    }
}

/// `new SlumLord(biota)`'s inventory load for an offline copy's slumlord: the shard's items are
/// sorted into it (ACE's `SortBiotasIntoInventory` on the object's queue), then
/// `OnInitialInventoryLoadCompleted` runs the callbacks. A load finishing after its copy left the
/// store is dropped; an item an unloaded landblock left in the store (its container gone) makes
/// way for the shard's.
pub fn load_offline_inventory(w: &mut World, root: ObjectGuid, container: ObjectGuid) {
    let Some(generation) = w.house_manager.offline.get(&root).map(|c| c.generation) else {
        return;
    };
    let Some(biota_id) = w.objects.get(container).map(|o| o.biota.id) else {
        return;
    };
    // this load replaces the constructor's own (`container::post_insert_load_inventory`)
    if let Some(c) = w
        .objects
        .get_mut(container)
        .and_then(|o| o.container.as_mut())
    {
        c.container.inventory_load_pending = false;
    }
    let still_current = move |w: &World| {
        w.house_manager.offline.get(&root).map(|c| c.generation) == Some(generation)
            && w.objects.contains(container)
    };

    w.shard.get_inventory_in_parallel(
        biota_id,
        false,
        Some(Box::new(move |w: &mut World, biotas: Vec<empyrean_store::models::shard::Biota>| {
            if !still_current(w) {
                return;
            }
            let biotas: Vec<empyrean_entity::Biota> =
                biotas.iter().map(|b| empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(b, false)).collect();
            dispatch::enqueue_action::enqueue_action(
                w,
                container,
                Action::delegate(move |w: &mut World| {
                    if !still_current(w) {
                        return;
                    }
                    for b in &biotas {
                        let id = ObjectGuid::new(b.id);
                        let stale = w.objects.get(id).is_some_and(|o| o.wo.world_object_properties.container.is_none_or(|c| !w.objects.contains(c) || c == container));
                        if stale {
                            w.objects.remove(id);
                        }
                    }
                    container::sort_biotas_into_inventory(w, container, biotas);
                }),
            );
        })),
    );
}

// ================================================================================ HouseManager.cs

// ACE: HouseManager.Initialize
pub fn initialize(w: &mut World) {
    let t = &mut w.house_manager.total_owned_housing_by_type;
    t.insert(HouseType::Apartment, 0);
    t.insert(HouseType::Cottage, 0);
    t.insert(HouseType::Villa, 0);
    t.insert(HouseType::Mansion, 0);

    build_house_id_to_guid(w);

    build_rent_queue(w);
}

/// Builds the lookup table for HouseId => HouseGuid
// ACE: HouseManager.BuildHouseIdToGuid
fn build_house_id_to_guid(w: &mut World) {
    // `from weenie in ctx.Weenie join inst in ctx.LandblockInstance on weenie.ClassId equals
    // inst.WeenieClassId where weenie.Type == (int)WeenieType.House`
    let house_type = i32::try_from(WeenieType::House.0).expect("fits");
    let house_weenies: std::collections::HashMap<u32, String> = w
        .content
        .get_all_weenies()
        .into_iter()
        .filter(|x| x.r#type == house_type)
        .map(|x| (x.class_id, x.class_name))
        .collect();

    // DIVERGE: the join is read landblock by landblock over the landblocks holding a slumlord
    // (`GetHousesAll`), not over every instance: the only reader, `GetHouseGuid`, picks the house on
    // the slumlord's own landblock, so the lookups answer the same; a whole-table join per house
    // weenie costs minutes at start-up.
    let mut landblocks: Vec<u16> = w
        .content
        .get_houses_all()
        .iter()
        .map(|r| u16::try_from((r.landblock_instance.guid >> 12) & 0xFFFF).expect("16 bits"))
        .collect();
    landblocks.sort_unstable();
    landblocks.dedup();

    let mut results = Vec::new();
    for landblock in landblocks {
        for inst in w
            .content
            .get_cached_instances_by_landblock(landblock)
            .iter()
        {
            if let Some(class_name) = house_weenies.get(&inst.weenie_class_id) {
                results.push((class_name.clone(), inst.guid));
            }
        }
    }

    let mut house_id_to_guid: DotNetDict<u32, Vec<u32>> = DotNetDict::new();

    for (classname, guid) in results {
        let Some(house_id) = parse_house_id(&classname) else {
            log::error!("[HOUSE] HouseManager.BuildHouseIdToGuid(): couldn't parse {classname}");
            continue;
        };

        house_id_to_guid
            .get_or_insert_with(house_id, Vec::new)
            .push(guid);
    }
    //log.Info($"BuildHouseIdToGuid: {HouseIdToGuid.Count}");
    w.house_manager.house_id_to_guid = Some(house_id_to_guid);
}

/// `uint.TryParse(Regex.Match(classname, @"\d+").Value, out var houseId)`: the first run of
/// digits. (`\d` also matches non-ASCII decimal digits, which `uint.TryParse` refuses; ACE's
/// class names are ASCII.)
#[must_use]
pub fn parse_house_id(classname: &str) -> Option<u32> {
    let start = classname.find(|c: char| c.is_ascii_digit())?;
    let digits: String = classname[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<u32>().ok()
}

/// Builds a list of all the player-owned houses on the server, sorted by RentDue timestamp
// ACE: HouseManager.BuildRentQueue
pub fn build_rent_queue(w: &mut World) {
    w.house_manager.rent_queue = Some(BTreeMap::new());

    //var allPlayers = PlayerManager.GetAllPlayers();
    //var houseOwners = allPlayers.Where(i => i.HouseInstance != null);

    //foreach (var houseOwner in houseOwners)
    //AddRentQueue(houseOwner);

    let slumlord_biotas = w
        .shard
        .base_database()
        .get_biotas_by_type(WeenieType::SlumLord);

    for slumlord in &slumlord_biotas {
        add_rent_queue_biota(w, slumlord);
    }

    //log.Info($"Loaded {RentQueue.Count} active houses.");
    query_multi_house(w);
}

fn iid_of(b: &empyrean_store::models::shard::Biota, t: PropertyInstanceId) -> Option<u32> {
    b.biota_properties_iid
        .iter()
        .find(|i| i.r#type == t.0)
        .map(|i| i.value)
}

fn did_of(b: &empyrean_store::models::shard::Biota, t: PropertyDataId) -> Option<u32> {
    b.biota_properties_did
        .iter()
        .find(|i| i.r#type == t.0)
        .map(|i| i.value)
}

/// Adds a player-owned house to the rent queue
// ACE: HouseManager.AddRentQueue
fn add_rent_queue_biota(w: &mut World, slumlord: &empyrean_store::models::shard::Biota) {
    let Some(biota_owner) = iid_of(slumlord, PropertyInstanceId::HouseOwner) else {
        // this is fine. this is just a house that was purchased, and then later abandoned
        //Console.WriteLine($"HouseManager.AddRentQueue(): couldn't find owner for house {slumlord.Id:X8}");
        return;
    };
    let (owner, _) = player_manager::find_by_guid(w, biota_owner);
    let Some(owner) = owner else {
        log::error!("[HOUSE] HouseManager.AddRentQueue(): couldn't find owner {biota_owner:08X}");
        return;
    };
    let Some(house_id) = did_of(slumlord, PropertyDataId::HouseId) else {
        log::error!(
            "[HOUSE] HouseManager.AddRentQueue(): couldn't find house id for {:08X}",
            slumlord.id
        );
        return;
    };
    let house_guids = w
        .house_manager
        .house_id_to_guid
        .as_ref()
        .and_then(|m| m.get(&house_id))
        .cloned();
    let Some(house_guids) = house_guids else {
        log::error!(
            "[HOUSE] HouseManager.AddRentQueue(): couldn't find house instance for {:08X}",
            slumlord.id
        );
        return;
    };
    let house_instance = get_house_guid(slumlord.id, &house_guids);
    if house_instance == 0 {
        log::error!(
            "[HOUSE] HouseManager.AddRentQueue(): couldn't find house guid for {:08X}",
            slumlord.id
        );
        return;
    }
    if rent_queue_contains_house(w, house_instance) {
        log::error!("[HOUSE] HouseManager.AddRentQueue(): rent queue already contains house {house_instance}");
        return;
    }
    add_rent_queue(w, owner, house_instance);
}

/// Returns TRUE if houseInstance is contained in the RentQueue
// ACE: HouseManager.RentQueueContainsHouse
fn rent_queue_contains_house(w: &World, house_instance: u32) -> bool {
    rent_queue(w)
        .values()
        .any(|i| w.objects.get(i.house).and_then(|h| h.house_instance()) == Some(house_instance))
}

fn rent_queue(w: &World) -> &BTreeMap<(DotNetDateTime, u32), PlayerHouse> {
    w.house_manager
        .rent_queue
        .as_ref()
        .expect("System.NullReferenceException: RentQueue")
}

fn rent_queue_mut(w: &mut World) -> &mut BTreeMap<(DotNetDateTime, u32), PlayerHouse> {
    w.house_manager
        .rent_queue
        .as_mut()
        .expect("System.NullReferenceException: RentQueue")
}

/// Adds a player-owned house to the rent queue
// ACE: HouseManager.AddRentQueue
pub fn add_rent_queue(w: &mut World, player: IPlayer, house_guid: u32) {
    //Console.WriteLine($"AddRentQueue({player.Name}, {houseGuid:X8})");

    let Some(house) = house::load(w, house_guid, false) else {
        // this can happen for basement dungeons
        return;
    };

    let purchase_time = i_player::get_property(w, player, PropertyInt::HousePurchaseTimestamp)
        .unwrap_or(0)
        .cast_unsigned();

    if i_player::get_property(w, player, PropertyInt::HouseRentTimestamp).is_none() {
        log::warn!(
            "[HOUSE] HouseManager.AddRentQueue({}, {:08X}): player has null HouseRentTimestamp",
            i_player::name(w, player).unwrap_or_default(),
            house_guid
        );
        let due = house::get_rent_due(w, house, purchase_time).cast_signed();
        i_player::set_property(w, player, PropertyInt::HouseRentTimestamp, due);
        //return;
    }
    add_rent_queue_house(w, player, house);
}

/// Adds a player-owned house to the rent queue
// ACE: HouseManager.AddRentQueue
fn add_rent_queue_house(w: &mut World, player: IPlayer, house: ObjectGuid) {
    let house_name = dispatch::name::name(w, house).unwrap_or_default();
    let player_house = PlayerHouse::new(w, player, house, &house_name);

    let key = player_house.key();
    let house_type = w
        .objects
        .get(house)
        .expect("System.NullReferenceException: House")
        .house_type();
    rent_queue_mut(w).entry(key).or_insert(player_house);

    increment_total_owned_housing_by_type(w, house_type);
}

/// Called when a player abandons a house
// ACE: HouseManager.RemoveRentQueue
pub fn remove_rent_queue(w: &mut World, house_guid: u32) {
    rent_queue_mut(w).retain(|_, i| i.house.full() != house_guid);
}

type ShardBiota = empyrean_store::models::shard::Biota;

/// Queries the status of multi-house owners on the server
// ACE: HouseManager.QueryMultiHouse
fn query_multi_house(w: &mut World) {
    let slumlord_biotas = w
        .shard
        .base_database()
        .get_biotas_by_type(WeenieType::SlumLord);

    let mut player_houses: DotNetDict<IPlayer, Vec<&ShardBiota>> = DotNetDict::new();
    let mut account_houses: DotNetDict<String, Vec<&ShardBiota>> = DotNetDict::new();

    for slumlord in &slumlord_biotas {
        let Some(biota_owner) = iid_of(slumlord, PropertyInstanceId::HouseOwner) else {
            // this is fine. this is just a house that was purchased, and then later abandoned
            continue;
        };
        let (owner, _) = player_manager::find_by_guid(w, biota_owner);
        let Some(owner) = owner else {
            log::info!("HouseManager.QueryMultiHouse(): couldn't find owner {biota_owner:08X}");
            continue;
        };

        player_houses
            .get_or_insert_with(owner, Vec::new)
            .push(slumlord);

        let account_name =
            i_player::account(w, owner).map_or_else(|| "NULL".to_owned(), |a| a.account_name);

        account_houses
            .get_or_insert_with(account_name, Vec::new)
            .push(slumlord);
    }

    let report = |name: String, houses: &[&ShardBiota]| {
        log::info!("{}: {}", name, houses.len());

        for (i, h) in houses.iter().enumerate() {
            log::info!("{}. {}", i + 1, get_coords_biota(h));
        }
    };

    if property_manager::get_bool(w, "house_per_char", false, true).item {
        // `.Where(Count > 1).OrderByDescending(Count)` (a stable sort)
        let mut results: Vec<(IPlayer, Vec<&ShardBiota>)> = player_houses
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        results.sort_by_key(|r| std::cmp::Reverse(r.1.len()));

        if !results.is_empty() {
            log::info!("Multi-house owners:");
        }

        for (player, houses) in results {
            report(i_player::name(w, player).unwrap_or_default(), &houses);
        }
    } else {
        let mut results: Vec<(String, Vec<&ShardBiota>)> = account_houses
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        results.sort_by_key(|r| std::cmp::Reverse(r.1.len()));

        if !results.is_empty() {
            log::info!("Multi-house owners:");
        }

        for (account, houses) in results {
            report(account, &houses);
        }
    }
}

/// Returns a friendly string for the position of a biota
// ACE: HouseManager.GetCoords
fn get_coords_biota(biota: &ShardBiota) -> String {
    let location_type = PositionType::Location.0;
    let p = biota
        .biota_properties_position
        .iter()
        .find(|i| i.position_type == location_type)
        .expect("System.NullReferenceException: no Location position");

    get_coords(&Position::from_components(
        p.obj_cell_id,
        p.origin_x,
        p.origin_y,
        p.origin_z,
        p.angles_x,
        p.angles_y,
        p.angles_z,
        p.angles_w,
        false,
    ))
}

/// Returns a friendly string a house / slumlord position
// ACE: HouseManager.GetCoords
#[must_use]
pub fn get_coords(position: &Position) -> String {
    if let Some(c) = get_map_coord_str(position) {
        return c;
    }

    let mut coords = String::new();
    // apartment slumlord?
    if let Some(apartment_block) = apartment_block(position.landblock()) {
        coords = format!("{apartment_block} - ");
    } else {
        log::error!("[HOUSE] HouseManager.GetCoords({position}) - couldn't find apartment block");
    }

    coords.push_str(&position.to_string());
    coords
}

/// Runs every ~1 minute
// ACE: HouseManager.Tick
pub fn tick(w: &mut World) {
    let clock = SnapshotClock(w.now);
    let limiter = w
        .house_manager
        .update_house_manager_rate_limiter
        .get_or_insert_with(|| RateLimiter::new(1, TimeSpan::from_minutes(1.0), &clock));
    if limiter.get_seconds_to_wait_before_next_event(&clock) > 0.0 {
        return;
    }

    limiter.register_event(&clock);

    //log.Info($"HouseManager.Tick({RentQueue.Count})");

    // `RentQueue.FirstOrDefault()`: `Initialize` (Program.Main) built the queue before the world
    // loop started. A world run without it (the test server) has no queue, and nothing to rent.
    let Some(queue) = w.house_manager.rent_queue.as_ref() else {
        return;
    };
    let Some(mut next_key) = queue.keys().next().copied() else {
        return;
    };

    let mut current_time = w.now.utc;

    while current_time > next_key.0 {
        let next_entry = rent_queue_mut(w).remove(&next_key).expect("present");
        let house = house::resolve(w, next_entry.house)
            .expect("System.NullReferenceException: nextEntry.House");
        let house_type = w.objects.get(house).expect("present").house_type();
        decrement_total_owned_housing_by_type(w, house_type);

        process_rent(w, next_entry);

        let Some(k) = rent_queue(w).keys().next().copied() else {
            return;
        };
        next_key = k;

        current_time = w.now.utc;
    }
}

/// Called when the RentDue timestamp has been reached for a player house
/// Determines if rent has been paid, and if the owner currently meets the requirements for owning the dwelling,
/// and then calls either HandleRentPaid or HandleEviction
// ACE: HouseManager.ProcessRent
fn process_rent(w: &mut World, player_house: PlayerHouse) {
    // load the most up-to-date copy of the house data
    let house_guid = player_house.house.full();
    get_house(
        w,
        house_guid,
        Box::new(move |w: &mut World, house: ObjectGuid| {
            let mut player_house = player_house;
            player_house.house = house;

            let status = w.objects.get(house).expect("present").house_status();
            let is_in_active_or_disabled = status <= HouseStatus::InActive;
            let is_paid = is_rent_paid(w, &player_house);
            let has_requirements = has_requirements(w, &player_house);
            log::info!(
                "[HOUSE] {}.ProcessRent(): isPaid = {} | HasRequirements = {} | MaintenanceFree = {}",
                player_house.player_name.clone().unwrap_or_default(),
                bool_str(is_paid),
                bool_str(has_requirements),
                bool_str(status == HouseStatus::InActive)
            );

            if is_in_active_or_disabled || (is_paid && has_requirements) {
                handle_rent_paid(w, &player_house);
            } else {
                handle_eviction_player_house(w, &player_house, false);
            }
        }),
    );
}

/// `bool.ToString()`.
fn bool_str(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// `onlinePlayer.HandleActionQueryHouse` after 3 s (`HandleRentPaid`, `HandleEviction`, `PayRent`).
fn query_house_in_3s(w: &mut World, online_player: ObjectGuid) {
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(3.0f32)); // wait for slumlord inventory biotas above to save
    action_chain.add_action(Actor::Object(online_player), move |w| {
        player_house::handle_action_query_house(w, online_player)
    });
    action_chain.enqueue_chain(w);
}

/// Handles a successful rent payment
/// Updates the next rent due timestamp, and clears the slumlord inventory
// ACE: HouseManager.HandleRentPaid
fn handle_rent_paid(w: &mut World, player_house: &PlayerHouse) {
    let (player, _) = player_manager::find_by_guid(w, player_house.player_guid);
    let Some(player) = player else {
        log::warn!(
            "[HOUSE] HouseManager.HandleRentPaid({}): couldn't find player",
            player_house.player_name.clone().unwrap_or_default()
        );
        return;
    };

    let purchase_time = i_player::get_property(w, player, PropertyInt::HousePurchaseTimestamp)
        .unwrap_or(0)
        .cast_unsigned();
    let rent_time = i_player::get_property(w, player, PropertyInt::HouseRentTimestamp)
        .unwrap_or(0)
        .cast_unsigned();

    let next_rent_time = house::get_rent_due(w, player_house.house, purchase_time);

    if next_rent_time <= rent_time {
        log::warn!(
            "[HOUSE] HouseManager.HandleRentPaid({}): nextRentTime {} <= rentTime {}",
            player_house.player_name.clone().unwrap_or_default(),
            next_rent_time,
            rent_time
        );
        return;
    }

    i_player::set_property(
        w,
        player,
        PropertyInt::HouseRentTimestamp,
        next_rent_time.cast_signed(),
    );

    i_player::save_biota_to_database(w, player, true);

    let mut cleared_inventory_status = "";
    if w.objects
        .get(player_house.house)
        .expect("present")
        .house_status()
        == HouseStatus::Active
    {
        // clear out slumlord inventory
        let slumlord = house::slum_lord(w, player_house.house)
            .expect("System.NullReferenceException: SlumLord");
        container::clear_inventory(w, slumlord, false);

        dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);
        cleared_inventory_status = "and cleared ";
    }

    // ACE-BUG: `log.InfoFormat($"...{0}...")` interpolates the literal 0 before formatting, so the log line names "0" rather than the player (log text only).
    log::info!("[HOUSE] HouseManager.HandleRentPaid(0): rent payment successfully collected {cleared_inventory_status}from SlumLord!");

    // re-add item to queue
    add_rent_queue_house(w, player, player_house.house);

    if let Some(online_player) = player_manager::get_online_player(w, player_house.player_guid) {
        query_house_in_3s(w, online_player);
    }
}

/// Handles the eviction process for a player house
// ACE: HouseManager.HandleEviction
fn handle_eviction_player_house(w: &mut World, player_house: &PlayerHouse, force: bool) {
    handle_eviction(
        w,
        player_house.house,
        player_house.player_guid,
        false,
        force,
    );
}

/// Handles the eviction process for a player house
// ACE: HouseManager.HandleEviction
pub fn handle_eviction(
    w: &mut World,
    house: ObjectGuid,
    player_guid: u32,
    multihouse: bool,
    force: bool,
) {
    // clear out slumlord inventory
    let slumlord = house::slum_lord(w, house).expect("System.NullReferenceException: SlumLord");
    container::clear_inventory(w, slumlord, false);

    let (player, is_online) = player_manager::find_by_guid(w, player_guid);
    let player = player.expect("System.NullReferenceException: player");

    if !property_manager::get_bool(w, "house_rent_enabled", true, true).item
        && !multihouse
        && !force
    {
        // rent disabled, push forward
        let purchase_time = i_player::get_property(w, player, PropertyInt::HousePurchaseTimestamp)
            .unwrap_or(0)
            .cast_unsigned();
        let next_rent_time = house::get_rent_due(w, house, purchase_time);
        i_player::set_property(
            w,
            player,
            PropertyInt::HouseRentTimestamp,
            next_rent_time.cast_signed(),
        );

        log::info!(
            "[HOUSE] HouseManager.HandleEviction({}): house rent disabled via config",
            i_player::name(w, player).unwrap_or_default()
        );

        // re-add item to queue
        add_rent_queue_house(w, player, house);
        return;
    }

    // handle eviction
    {
        let h = w.objects.get_mut(house).expect("present");
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
    slum_lord::off(w, slumlord);

    // reset slumlord name
    slum_lord::set_and_broadcast_name(w, slumlord, None);

    dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);

    house_list::add_to_available(w, slumlord, house);

    // if evicting a multihouse owner's previous house,
    // no update for player properties
    if i_player::house_instance(w, player) == Some(house.full()) {
        i_player::remove_property(w, player, PropertyDataId::HouseId);
        i_player::remove_property(w, player, PropertyInstanceId::House);
        //player.HousePurchaseTimestamp = null;
        i_player::remove_property(w, player, PropertyInt::HouseRentTimestamp);
    } else {
        log::warn!(
            "[HOUSE] HouseManager.HandleRentEviction({}, {}, {}): house guids don't match {}",
            house,
            i_player::name(w, player).unwrap_or_default(),
            bool_str(multihouse),
            i_player::house_instance(w, player).map_or_else(String::new, |h| h.to_string())
        );
    }

    house::clear_restrictions(w, house);

    log::info!(
        "[HOUSE] HouseManager.HandleRentEviction({})",
        i_player::name(w, player).unwrap_or_default()
    );

    if multihouse {
        remove_rent_queue(w, house.full());
        let house_type = w.objects.get(house).expect("present").house_type();
        decrement_total_owned_housing_by_type(w, house_type);

        i_player::save_biota_to_database(w, player, true);

        return;
    }

    if !is_online {
        // inform player of eviction when they log in
        if player_manager::get_offline_player(w, player_guid).is_none() {
            log::warn!(
                "[HOUSE] {}.HandleEviction(): couldn't find offline player",
                i_player::name(w, player).unwrap_or_default()
            );
            return;
        }
        let offline_player = IPlayer::Offline(ObjectGuid::new(player_guid));
        i_player::set_property(w, offline_player, PropertyBool::HouseEvicted, true);
        i_player::save_biota_to_database(w, offline_player, true);
        return;
    }

    let online_player = player_manager::get_online_player(w, player_guid)
        .expect("System.NullReferenceException: onlinePlayer");

    player_house::set_house(w, online_player, None);

    // send text message
    player_house::system_chat(
        w,
        online_player,
        "Your house has reverted due to non-payment of the maintenance costs.  All items stored in the house have been lost.",
        ChatMessageType::Broadcast,
    );
    player_house::remove_deed(w, online_player);

    dispatch::save_biota_to_database::save_biota_to_database(w, online_player, true);

    // clear house panel for online player
    query_house_in_3s(w, online_player);
}

/// Returns TRUE if the slumlord contains all of the required items for rent payment
// ACE: HouseManager.IsRentPaid
fn is_rent_paid(w: &mut World, player_house: &PlayerHouse) -> bool {
    let house_data = get_house_data(w, player_house.house);

    for rent_item in &house_data.rent {
        if rent_item.paid < rent_item.num {
            let item_name = if rent_item.num > 1 {
                rent_item.plural_name.clone()
            } else {
                rent_item.name.clone()
            };
            log::info!(
                "[HOUSE] {}.IsRentPaid() - required {}x {} ({}), found {}",
                player_house.player_name.clone().unwrap_or_default(),
                rent_item.num,
                item_name.unwrap_or_default(),
                rent_item.weenie_id,
                rent_item.paid
            );
            return false;
        }
    }
    true
}

/// Returns TRUE if a player currently meets all of the requirements for owning their house (allegiance rank)
// ACE: HouseManager.HasRequirements
fn has_requirements(w: &mut World, player_house: &PlayerHouse) -> bool {
    if !property_manager::get_bool(w, "house_purchase_requirements", false, true).item {
        return true;
    }

    let slumlord =
        house::slum_lord(w, player_house.house).expect("System.NullReferenceException: SlumLord");
    let Some(slumlord_min) = w
        .objects
        .get(slumlord)
        .expect("present")
        .allegiance_min_level()
    else {
        return true;
    };

    let mut allegiance_min_level = property_manager::get_long(w, "mansion_min_rank", -1, true).item;
    if allegiance_min_level == -1 {
        allegiance_min_level = i64::from(slumlord_min);
    }

    let (player, _) = player_manager::find_by_guid(w, player_house.player_guid);

    let Some(player) = player else {
        log::warn!(
            "[HOUSE] {}.HasRequirements() - couldn't find player",
            player_house.player_name.clone().unwrap_or_default()
        );
        return false;
    };

    // ensure allegiance is loaded
    let allegiance = player_house::allegiance::allegiance_manager_get_allegiance(w, player);

    let rank = allegiance
        .and_then(|a| player_house::allegiance::members_rank(w, a, player.guid()))
        .unwrap_or(0);

    if allegiance_min_level > 0 && (allegiance.is_none() || i64::from(rank) < allegiance_min_level)
    {
        log::info!(
            "[HOUSE] {}.HasRequirements() - allegiance rank {} < {}",
            player_house.player_name.clone().unwrap_or_default(),
            rank,
            allegiance_min_level
        );
        return false;
    }
    true
}

/// Returns the HouseData structure for a House (rent and paid items)
// ACE: HouseManager.GetHouseData
fn get_house_data(w: &mut World, house: ObjectGuid) -> HouseData {
    let mut house_data = house_data_new(w);

    let slumlord = house::slum_lord(w, house).expect("System.NullReferenceException: SlumLord");
    let rent = slum_lord::get_rent_items(w, slumlord);
    house_data.set_rent_items(w, &rent);
    house_data.set_paid_items(w, slumlord);

    if w.objects.get(house).expect("present").house_status() == HouseStatus::InActive {
        house_data.maintenance_free = true;
    }

    house_data
}

// This function is called from a database callback.
// We must add thread safety to prevent HouseManager corruption
// ACE: HouseManager.HandlePlayerDelete
pub fn handle_player_delete(w: &mut World, player_guid: u32) {
    world_manager::enqueue_action(
        w,
        Action::delegate(move |w: &mut World| do_handle_player_delete(w, player_guid)),
    );
}

/// Called on character delete, evicts from house
// ACE: HouseManager.DoHandlePlayerDelete
fn do_handle_player_delete(w: &mut World, player_guid: u32) {
    let (player, _) = player_manager::find_by_guid(w, player_guid);
    let Some(player) = player else {
        log::info!("HouseManager.HandlePlayerDelete({player_guid:08X}): couldn't find player guid");
        return;
    };

    if i_player::house_instance(w, player).is_none() {
        return;
    }

    let Some(player_house) = find_player_house(w, player_guid) else {
        return;
    };

    // load the most up-to-date copy of house data
    get_house(
        w,
        player_house.house.full(),
        Box::new(move |w: &mut World, house: ObjectGuid| {
            let mut player_house = player_house;
            player_house.house = house;

            handle_eviction_player_house(w, &player_house, true);

            remove_rent_queue(w, house.full());
            let house_type = w.objects.get(house).expect("present").house_type();
            decrement_total_owned_housing_by_type(w, house_type);
        }),
    );
}

/// Returns the house in the rent queue for a player guid, if exists
// ACE: HouseManager.FindPlayerHouse
fn find_player_house(w: &World, player_guid: u32) -> Option<PlayerHouse> {
    rent_queue(w)
        .values()
        .find(|i| i.player_guid == player_guid)
        .cloned()
}

/// The houses of the queue entries `pred` keeps, in queue order, each resolved (see `house.rs`).
fn houses_where(w: &mut World, pred: impl Fn(&PlayerHouse) -> bool) -> Vec<ObjectGuid> {
    let guids: Vec<ObjectGuid> = rent_queue(w)
        .values()
        .filter(|i| pred(i))
        .map(|i| i.house)
        .collect();
    guids
        .into_iter()
        .filter_map(|g| house::resolve(w, g))
        .collect()
}

/// Returns all of the houses in the rent queue for a house id
// ACE: HouseManager.GetHouseById
pub fn get_house_by_id(w: &mut World, house_id: u32) -> Vec<ObjectGuid> {
    let guids: Vec<ObjectGuid> = rent_queue(w).values().map(|i| i.house).collect();
    let houses: Vec<ObjectGuid> = guids
        .into_iter()
        .filter_map(|g| house::resolve(w, g))
        .collect();
    houses
        .into_iter()
        .filter(|&h| w.objects.get(h).and_then(|o| o.house_id()) == Some(house_id))
        .collect()
}

/// Returns all of the houses in the rent queue for an account
// ACE: HouseManager.GetAccountHouses
pub fn get_account_houses(w: &mut World, account_id: u32) -> Vec<ObjectGuid> {
    houses_where(w, |i| i.account_id == account_id)
}

/// Returns all of the houses in the rent queue for a character
// ACE: HouseManager.GetCharacterHouses
pub fn get_character_houses(w: &mut World, player_guid: u32) -> Vec<ObjectGuid> {
    houses_where(w, |i| i.player_guid == player_guid)
}

/// Returns the house guid for a slumlord guid
// ACE: HouseManager.GetHouseGuid
#[must_use]
pub fn get_house_guid(slumlord_guid: u32, house_guids: &[u32]) -> u32 {
    let slumlord_prefix = slumlord_guid >> 12;

    house_guids
        .iter()
        .copied()
        .find(|i| slumlord_prefix == (i >> 12))
        .unwrap_or(0)
}

/// If the landblock is loaded, return a reference to the current House object
/// else return a copy of the House biota from the latest info in the db
///
/// `callback` is called when the slumlord inventory is fully loaded
// ACE: HouseManager.GetHouse
pub fn get_house(w: &mut World, house_guid: u32, callback: HouseAction) {
    let landblock_id = house::landblock_id_of_house_guid(house_guid);
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if !is_loaded {
        // landblock is unloaded
        // return a copy of the House biota from the latest info in the db
        let house_biota = house::load(w, house_guid, false);

        register_callback(w, house_biota, callback);

        return;
    }

    // landblock is loaded, return a reference to the current House object
    let house =
        crate::entity::landblock::get_object(w, landblock_id, ObjectGuid::new(house_guid), true)
            .filter(|&g| w.objects.get(g).is_some_and(|o| o.is_house()));

    let slumlord = house.and_then(|h| house::slum_lord(w, h));
    if let (Some(house), Some(slumlord)) = (house, slumlord) {
        if slum_lord::inventory_loaded(w, slumlord) {
            callback(w, house);
        } else {
            register_callback(w, Some(house), callback);
        }
    } else if !w
        .landblock_manager
        .landblocks
        .get(landblock_id)
        .is_some_and(crate::entity::landblock::Landblock::create_world_objects_completed)
    {
        let house_biota = house::load(w, house_guid, false);

        register_callback(w, house_biota, callback);
    } else {
        log::error!("[HOUSE] HouseManager.GetHouse({house_guid:08X}): couldn't find house on loaded landblock");
    }
}

/// Registers a callback to run when the slumlord inventory has been loaded
///
/// # Panics
/// For a null house or one with no slumlord (ACE's `NullReferenceException`).
// ACE: HouseManager.RegisterCallback
pub fn register_callback(w: &mut World, house: Option<ObjectGuid>, callback: HouseAction) {
    let house = house.expect("System.NullReferenceException: house");
    let slumlord =
        house::slum_lord(w, house).expect("System.NullReferenceException: house.SlumLord");
    w.house_manager
        .slumlord_callbacks
        .get_or_insert_with(slumlord.full(), Vec::new)
        .push(HouseCallback::new(house, callback));
}

/// Runs any pending HouseManager callbacks for this slumlord house
// ACE: HouseManager.OnInitialInventoryLoadCompleted
pub fn on_initial_inventory_load_completed(w: &mut World, slumlord: ObjectGuid) {
    //Console.WriteLine($"HouseManager.OnInitialInventoryLoadCompleted({slumlord.Name})");

    if let Some(callbacks) = w.house_manager.slumlord_callbacks.remove(&slumlord.full()) {
        for callback in callbacks {
            callback.run(w);
        }
    }
}

/// A mapping of apartment landblocks => apartment complex names
// ACE: HouseManager.ApartmentBlocks
pub const APARTMENT_BLOCKS: [(u32, &str); 50] = [
    // currently used for apartment deeds
    (0x5360, "Sanctum Residential Halls - Alvan Court"),
    (0x5361, "Sanctum Residential Halls - Caerna Dwellings"),
    (0x5362, "Sanctum Residential Halls - Illsin Veranda"),
    (0x5363, "Sanctum Residential Halls - Marin Court"),
    (0x5364, "Sanctum Residential Halls - Ruadnar Court"),
    (0x5365, "Sanctum Residential Halls - Senmai Court"),
    (0x5366, "Sanctum Residential Halls - Sigil Veranda"),
    (0x5367, "Sanctum Residential Halls - Sorveya Court"),
    (0x5368, "Sanctum Residential Halls - Sylvan Dwellings"),
    (0x5369, "Sanctum Residential Halls - Treyval Veranda"),
    (0x7200, "Atrium Residential Halls - Winthur Gate"),
    (0x7300, "Atrium Residential Halls - Larkspur Gardens"),
    (0x7400, "Atrium Residential Halls - Mellas Court"),
    (0x7500, "Atrium Residential Halls - Vesper Gate"),
    (0x7600, "Atrium Residential Halls - Gajin Dwellings"),
    (0x7700, "Atrium Residential Halls - Valorya Gate"),
    (0x7800, "Atrium Residential Halls - Heartland Yard"),
    (0x7900, "Atrium Residential Halls - Ivory Gate"),
    (0x7A00, "Atrium Residential Halls - Alphas Court"),
    (0x7B00, "Atrium Residential Halls - Hasina Gardens"),
    (0x7C00, "Oriel Residential Halls - Sorac Gate"),
    (0x7D00, "Oriel Residential Halls - Maru Veranda"),
    (0x7E00, "Oriel Residential Halls - Forsythian Gardens"),
    (0x7F00, "Oriel Residential Halls - Vindalan Dwellings"),
    (0x8000, "Oriel Residential Halls - Syrah Dwellings"),
    (0x8100, "Oriel Residential Halls - Allain Court"),
    (0x8200, "Oriel Residential Halls - White Lotus Gate"),
    (0x8300, "Oriel Residential Halls - Autumn Moon Gardens"),
    (0x8400, "Oriel Residential Halls - Trellyn Gardens"),
    (0x8500, "Oriel Residential Halls - Endara Gate"),
    (0x8600, "Haven Residential Halls - Celcynd Grotto"),
    (0x8700, "Haven Residential Halls - Trothyr Hollow"),
    (0x8800, "Haven Residential Halls - Jojii Gardens"),
    (0x8900, "Haven Residential Halls - Cedraic Court"),
    (0x8A00, "Haven Residential Halls - Ben Ten Lodge"),
    (0x8B00, "Haven Residential Halls - Dulok Court"),
    (0x8C00, "Haven Residential Halls - Crescent Moon Veranda"),
    (0x8D00, "Haven Residential Halls - Jade Gate"),
    (0x8E00, "Haven Residential Halls - Ispar Yard"),
    (0x8F00, "Haven Residential Halls - Xao Wu Gardens"),
    (0x9000, "Victory Residential Halls - Accord Veranda"),
    (0x9100, "Victory Residential Halls - Candeth Court"),
    (0x9200, "Victory Residential Halls - Celdiseth Court"),
    (0x9300, "Victory Residential Halls - Festivus Court"),
    (0x9400, "Victory Residential Halls - Hibiscus Gardens"),
    (0x9500, "Victory Residential Halls - Meditation Gardens"),
    (0x9600, "Victory Residential Halls - Setera Gardens"),
    (0x9700, "Victory Residential Halls - Spirit Gate"),
    (0x9800, "Victory Residential Halls - Triumphal Gardens"),
    (0x9900, "Victory Residential Halls - Wilamil Court"),
];

/// `ApartmentBlocks.TryGetValue(landblock, out var name)`.
#[must_use]
pub fn apartment_block(landblock: u32) -> Option<&'static str> {
    APARTMENT_BLOCKS
        .iter()
        .find(|(k, _)| *k == landblock)
        .map(|(_, v)| *v)
}

/// Pay rent for a house
// ACE: HouseManager.PayRent
fn pay_rent_player_house(w: &mut World, player_house: PlayerHouse) {
    // load the most up-to-date copy of the house data
    let house_guid = player_house.house.full();
    get_house(
        w,
        house_guid,
        Box::new(move |w: &mut World, house: ObjectGuid| {
            let mut player_house = player_house;
            player_house.house = house;

            let is_paid = is_rent_paid(w, &player_house)
                || w.objects.get(house).expect("present").house_status() <= HouseStatus::InActive;

            if is_paid {
                return;
            }

            let house_data = get_house_data(w, house);
            let slumlord =
                house::slum_lord(w, house).expect("System.NullReferenceException: SlumLord");

            for rent_item in &house_data.rent {
                if rent_item.paid < rent_item.num {
                    let mut amount_left_to_pay = rent_item.num.wrapping_sub(rent_item.paid);

                    while amount_left_to_pay > 0 {
                        let Some(payment) = world_object_equipment::create_new_world_object_by_wcid(
                            w,
                            rent_item.weenie_id,
                        ) else {
                            log::error!("[HOUSE] HouseManager.PayRent({house}): couldn't create payment for WCID {}", rent_item.weenie_id);
                            return;
                        };
                        let payment_guid = payment.guid;
                        let _ = w.objects.insert(payment);

                        let max = w
                            .objects
                            .get(payment_guid)
                            .expect("present")
                            .max_stack_size()
                            .unwrap_or(1);
                        slum_lord::set_stack_size(
                            w,
                            payment_guid,
                            amount_left_to_pay.min(i32::from(max)),
                        );

                        if !container::try_add_to_inventory(
                            w,
                            slumlord,
                            payment_guid,
                            0,
                            false,
                            true,
                        ) {
                            log::error!(
                                "[HOUSE] HouseManager.PayRent({house}): couldn't place {} (0x{payment_guid}) in SlumLord's Inventory",
                                dispatch::name::name(w, payment_guid).unwrap_or_default()
                            );
                            // not ACE: the unplaced payment is unreachable (C# collects it)
                            w.objects.remove(payment_guid);
                            return;
                        }

                        amount_left_to_pay = amount_left_to_pay.wrapping_sub(
                            w.objects
                                .get(payment_guid)
                                .and_then(|o| o.stack_size())
                                .unwrap_or(1),
                        );
                    }
                }
            }

            container::merge_all_stackables(w, slumlord);

            for item in container::inventory_values(w, slumlord) {
                dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
            }

            dispatch::save_biota_to_database::save_biota_to_database(w, slumlord, true);

            if let Some(online_player) =
                player_manager::get_online_player(w, player_house.player_guid)
            {
                query_house_in_3s(w, online_player);
            }

            log::info!("[HOUSE] HouseManager.PayRent({house}): fully paid rent into SlumLord.");
        }),
    );
}

/// Pay rent for a house
// ACE: HouseManager.PayRent
pub fn pay_rent(w: &mut World, house: ObjectGuid) -> bool {
    let owner = w
        .objects
        .get(house)
        .and_then(|o| o.house_owner())
        .unwrap_or(0);
    let found_house = rent_queue(w)
        .values()
        .find(|h| h.player_guid == owner)
        .cloned();

    let Some(found_house) = found_house else {
        return false;
    };

    pay_rent_player_house(w, found_house);

    true
}

/// Pay rent for all owned housing
// ACE: HouseManager.PayAllRent
pub fn pay_all_rent(w: &mut World) {
    let houses: Vec<PlayerHouse> = rent_queue(w).values().cloned().collect();
    for house in houses {
        pay_rent_player_house(w, house);
    }
}

// ACE: HouseManager.TotalOwnedHousing
#[must_use]
pub fn total_owned_housing(w: &World) -> usize {
    w.house_manager.rent_queue.as_ref().map_or(0, BTreeMap::len)
}

/// # Panics
/// For a type `Initialize` did not add (`KeyNotFoundException`).
// ACE: HouseManager.IncrementTotalOwnedHousingByType
pub fn increment_total_owned_housing_by_type(w: &mut World, house_type: HouseType) {
    let v = w
        .house_manager
        .total_owned_housing_by_type
        .get_mut(&house_type)
        .unwrap_or_else(|| {
            panic!(
                "System.Collections.Generic.KeyNotFoundException: HouseType {}",
                house_type.0
            )
        });
    *v = v.wrapping_add(1);
}

/// # Panics
/// As [`increment_total_owned_housing_by_type`].
// ACE: HouseManager.DecrementTotalOwnedHousingByType
pub fn decrement_total_owned_housing_by_type(w: &mut World, house_type: HouseType) {
    let v = w
        .house_manager
        .total_owned_housing_by_type
        .get_mut(&house_type)
        .unwrap_or_else(|| {
            panic!(
                "System.Collections.Generic.KeyNotFoundException: HouseType {}",
                house_type.0
            )
        });
    *v = v.wrapping_sub(1);
}
