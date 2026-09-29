// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/HouseList.cs
//! Port of `Source/ACE.Server/Entity/HouseList.cs`.
//!
//! ACE's static class keeps its two lists in static fields; here they are
//! [`HouseListState`], held in `World.house_manager.house_list`.

use std::collections::HashSet;

use empyrean_common::dotnet::DotNetDict;
use empyrean_content::entity::HouseListResults;
use empyrean_entity::enums::HouseType;
use empyrean_entity::ObjectGuid;

use crate::World;

/// `HouseList.AllHouses` and `HouseList.Available` (both `null` until first built).
#[derive(Debug, Default)]
pub struct HouseListState {
    // ACE: HouseList.AllHouses
    pub all_houses: Option<Vec<HouseListResults>>,
    // ACE: HouseList.Available
    pub available: Option<DotNetDict<HouseType, Vec<HouseListResults>>>,
}

fn state(w: &mut World) -> &mut HouseListState {
    &mut w.house_manager.house_list
}

/// `Available[houseType]`.
///
/// # Panics
/// For a type with no list (`KeyNotFoundException`).
fn available_of(w: &mut World, house_type: HouseType) -> &mut Vec<HouseListResults> {
    state(w)
        .available
        .as_mut()
        .expect("Available is built")
        .get_mut(&house_type)
        .unwrap_or_else(|| {
            panic!(
                "System.Collections.Generic.KeyNotFoundException: HouseType {}",
                house_type.0
            )
        })
}

// ACE: HouseList.GetHouseList
pub fn get_house_list(w: &mut World) {
    // PCAP Part 1\Dr_Doom_Random_Running_Around\pkt_2017-1-29_1485735833_log.pcap (18538)
    // PCAP Part 1\Fenn-pcap6\pkt_2017-1-31_1485869242_log.pcap (541)

    // get a list of all the houses in the world
    find_all_houses(w);

    // get a list of which houses are occupied for this shard
    let houses_owned = get_houses_owned(w);

    // build available
    build_available(w, &houses_owned);

    let apartments = available_of(w, HouseType::Apartment).len();
    let cottages = available_of(w, HouseType::Cottage).len();
    let villas = available_of(w, HouseType::Villa).len();
    let mansions = available_of(w, HouseType::Mansion).len();

    // `Console.WriteLine`
    log::info!("Apartments: {apartments}");
    log::info!("Cottages: {cottages}");
    log::info!("Villas: {villas}");
    log::info!("Mansions: {mansions}");
}

// ACE: HouseList.FindAllHouses
pub fn find_all_houses(w: &mut World) {
    // select * from weenie inner join weenie_properties_int wint on weenie.class_Id=wint.object_Id inner join landblock_instance winst on weenie.class_Id=winst.weenie_Class_Id where weenie.`type`=53 and wint.`type`=155 order by wint.`value`;

    if state(w).all_houses.is_none() {
        let all = w.content.get_houses_all();
        state(w).all_houses = Some(all);
    }

    log::info!(
        "Total houses: {}",
        state(w).all_houses.as_ref().map_or(0, Vec::len)
    );
}

// ACE: HouseList.GetHousesOwned
pub fn get_houses_owned(w: &mut World) -> HashSet<u32> {
    // select * from biota where weenie_Type=53;

    let houses_owned = w.shard.base_database().get_houses_owned();

    log::info!("Owned houses: {}", houses_owned.len());

    // build owned hashset
    let mut owned = HashSet::new();

    for house in houses_owned {
        if owned.contains(&house.id) {
            log::info!(
                "HouseList.GetOwned(): duplicate owned house id {}",
                house.id
            );
            continue;
        }
        owned.insert(house.id);
    }
    owned
}

// ACE: HouseList.BuildAvailable
pub fn build_available(w: &mut World, houses_owned: &HashSet<u32>) {
    let mut available = DotNetDict::new();
    available.add(HouseType::Apartment, Vec::new());
    available.add(HouseType::Cottage, Vec::new());
    available.add(HouseType::Villa, Vec::new());
    available.add(HouseType::Mansion, Vec::new());

    let all = state(w).all_houses.clone().unwrap_or_default();
    for house in all {
        if houses_owned.contains(&house.landblock_instance.guid) {
            continue;
        }

        available
            .get_mut(&house.house_type)
            .unwrap_or_else(|| {
                panic!(
                    "System.Collections.Generic.KeyNotFoundException: HouseType {}",
                    house.house_type.0
                )
            })
            .push(house);
    }
    state(w).available = Some(available);
}

// ACE: HouseList.GetAvailableLocations
pub fn get_available_locations(w: &mut World, house_type: HouseType) -> Vec<u32> {
    // cache results?
    if state(w).available.is_none() {
        get_house_list(w);
    }

    available_of(w, house_type)
        .iter()
        .map(|i| i.landblock_instance.obj_cell_id | 0x0001)
        .collect()
}

/// `slumLord.House?.HouseType ?? houseToX.HouseType`.
fn house_type_of(w: &World, slum_lord: ObjectGuid, house: ObjectGuid) -> HouseType {
    let parent = w
        .objects
        .get(slum_lord)
        .and_then(|o| o.wo.world_object_links.parent_link)
        .filter(|&p| w.objects.get(p).is_some_and(|o| o.is_house()));
    match parent {
        Some(p) => w.objects.get(p).expect("present").house_type(),
        None => w
            .objects
            .get(house)
            .expect("System.NullReferenceException: house")
            .house_type(),
    }
}

// ACE: HouseList.RemoveFromAvailable
pub fn remove_from_available(w: &mut World, slum_lord: ObjectGuid, house_to_remove: ObjectGuid) {
    if state(w).available.is_none() {
        return; // no results cached, move on.
    }

    let house_type = house_type_of(w, slum_lord, house_to_remove);

    let list = available_of(w, house_type);
    if let Some(i) = list
        .iter()
        .position(|i| i.landblock_instance.guid == slum_lord.full())
    {
        list.remove(i);
    }
}

// ACE: HouseList.AddToAvailable
pub fn add_to_available(w: &mut World, slum_lord: ObjectGuid, house_to_add: ObjectGuid) {
    if state(w).available.is_none() {
        return; // no results cached, move on.
    }

    let house_type = house_type_of(w, slum_lord, house_to_add);

    {
        let list = available_of(w, house_type);
        if let Some(i) = list
            .iter()
            .position(|i| i.landblock_instance.guid == slum_lord.full())
        {
            list.remove(i);
        }
    }

    let wcid = w
        .objects
        .get(slum_lord)
        .expect("System.NullReferenceException: slumLord")
        .biota
        .weenie_class_id;
    let weenie = w.content.get_weenie(wcid);
    //var landblockInstance = slumLord.House.LinkedInstances.FirstOrDefault(i => i.Guid == slumLord.Guid.Full);
    let linked_of = |w: &World, h: Option<ObjectGuid>| {
        h.and_then(|h| w.objects.get(h)).and_then(|o| {
            o.wo.world_object_links
                .linked_instances
                .iter()
                .find(|i| i.guid == slum_lord.full())
                .cloned()
        })
    };
    let slum_lord_house = w
        .objects
        .get(slum_lord)
        .and_then(|o| o.wo.world_object_links.parent_link)
        .filter(|&p| house_is(w, p));
    let landblock_instance =
        linked_of(w, slum_lord_house).or_else(|| linked_of(w, Some(house_to_add)));
    //var landblockInstance = houseToAdd.LinkedInstances.FirstOrDefault(i => i.Guid == slumLord.Guid.Full);

    let (Some(weenie), Some(landblock_instance)) = (weenie, landblock_instance) else {
        return;
    };

    available_of(w, house_type).push(HouseListResults::new(weenie, landblock_instance));
}

fn house_is(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(|o| o.is_house())
}
