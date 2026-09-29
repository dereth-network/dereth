// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HouseAccess.cs
//! Port of `Source/ACE.Server/Network/Structure/HouseAccess.cs`.

use dereth_protocol::archive::PackedHash;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::HARBitfield;
use empyrean_entity::ObjectGuid;

use super::guest_info::{self, GuestInfo};
use super::hash_comparer::{sorted, GuidComparer};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: HouseAccess
/// Set of information related to house access.
#[derive(Debug, Clone)]
pub struct HouseAccess {
    // ACE: HouseAccess.Version
    /// 0x10000002, seems to be some kind of version. Older version started with bitmask, so
    /// starting with 0x10000000 allows them to determine if this is V1 or V2. The latter half
    /// appears to indicate whether there is a roommate list.
    pub version: u32,
    // ACE: HouseAccess.Bitmask
    /// 0 = private house, 1 = open to public, 2 = Allegiance has access to house, 4 = Allegiance
    /// has access to house and storage
    pub bitmask: HARBitfield,
    // ACE: HouseAccess.MonarchID
    /// populated when any allegiance access is specified
    pub monarch_id: ObjectGuid,
    // ACE: HouseAccess.GuestList
    /// Set of guests with their ID as the key and some additional info for them
    pub guest_list: DotNetDict<ObjectGuid, GuestInfo>,
    // ACE: HouseAccess.Roommates
    /// The ID list for all of your roommates
    pub roommates: Vec<ObjectGuid>,
}

impl Default for HouseAccess {
    // ACE: HouseAccess.HouseAccess
    /// `new HouseAccess()`.
    fn default() -> Self {
        HouseAccess {
            version: 0x1000_0002,
            bitmask: HARBitfield(0),
            monarch_id: ObjectGuid::INVALID,
            guest_list: DotNetDict::new(),
            roommates: Vec::new(),
        }
    }
}

// ACE: HouseAccess.HouseAccess
/// `new HouseAccess(House house)`. `None` is a null house. A guest that `PlayerManager` cannot
/// find is left out (V322).
pub fn house_access_new(w: &World, house: Option<ObjectGuid>) -> HouseAccess {
    let mut har = HouseAccess::default();

    let Some(house) = house else { return har };
    let h = w.objects.get(house).expect("ACE: house is null");

    if let Some(monarch_id) = h.monarch_id() {
        har.monarch_id = ObjectGuid::new(monarch_id); // for allegiance guest/storage access
    }

    if h.open_to_everyone() {
        har.bitmask |= HARBitfield::OpenHouse;
    }

    for (key, value) in crate::world_objects::house::fields(w, house)
        .guests
        .iter()
        .map(|(k, v)| (*k, *v))
    {
        let player = shims::player_manager_find_by_guid(w, key.full());

        // Not ACE's (a fix, V322): a guest the player list cannot find (a
        // deleted character) is left out and the rest of the list is built. ACE read the missing
        // guest's guid after its null-checked add, so the whole access list failed.
        let Some(player) = player else { continue };

        if player.guid != har.monarch_id {
            har.guest_list.add(key, GuestInfo::new(value, &player.name));
        }

        if player.guid == har.monarch_id {
            if value {
                har.bitmask |= HARBitfield::AllegianceStorage;
            } else {
                har.bitmask |= HARBitfield::AllegianceGuests;
            }
        }
    }

    let Some(house_owner) = h.house_owner() else {
        return har;
    };

    // add in players on house owner's account
    let owner = shims::player_manager_find_by_guid(w, house_owner);

    // added for people deleting accounts from their account db...
    let (Some(owner), Some(account_id)) = (owner.clone(), owner.and_then(|o| o.account_id)) else {
        log::info!(
            "HouseAccess({:08X}): couldn't find house owner {:08X}",
            shims::house_instance(w, house).unwrap_or(0),
            house_owner
        );
        return har;
    };

    let account_players =
        shims::player_manager_get_account_players(w, account_id).unwrap_or_default();

    for (key, value) in account_players {
        if owner.guid.full() != key {
            har.roommates.push(value.guid);
        }
    }
    har
}

// ACE: HouseAccessExtensions.Write
/// `writer.Write(HouseAccess har)`.
pub fn write(writer: &mut Vec<u8>, har: &HouseAccess) {
    let strings: Vec<&str> = har
        .guest_list
        .iter()
        .map(|(_, g)| g.guest_name.as_deref().unwrap_or(""))
        .collect();
    write_record(writer, &strings, |w| record(har).write(w));
}

/// not found in retail pcaps, using client HAR constructor default
const GUEST_COMPARER: GuidComparer = GuidComparer::new(64);

// ACE: HouseAccessExtensions.Write
/// `writer.Write(Dictionary<ObjectGuid, GuestInfo> guestList)`.
pub fn write_guest_list(writer: &mut Vec<u8>, guest_list: &DotNetDict<ObjectGuid, GuestInfo>) {
    let strings: Vec<&str> = guest_list
        .iter()
        .map(|(_, g)| g.guest_name.as_deref().unwrap_or(""))
        .collect();
    write_record(writer, &strings, |w| {
        w.packed_hash(&guest_list_record(guest_list), |w, k, v| {
            w.u32(*k);
            v.write(w)
        })
    });
}

// ACE: HouseAccessExtensions.Write
/// `writer.Write(List<ObjectGuid> roommates)`: unused in client ui.
pub fn write_roommates(writer: &mut Vec<u8>, roommates: &[ObjectGuid]) {
    let roommates: Vec<u32> = roommates.iter().map(|r| r.full()).collect();
    write_record(writer, &[], |w| {
        w.packed_list(&roommates, |w, v| {
            w.u32(*v);
            Ok(())
        })
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field. ACE's version (0x10000002) is the one with the roommate list; ACE writes the list whatever
/// the version says, as the record does when it holds one.
#[must_use]
pub fn record(har: &HouseAccess) -> dereth_protocol::trade::Har {
    debug_assert!(
        har.version & 0xFFFF_0000 != 0,
        "the oldest HAR form has no version dword"
    );
    dereth_protocol::trade::Har {
        version: har.version,
        bitmask: har.bitmask.0.cast_unsigned(),
        monarch_iid: har.monarch_id.into(),
        guest_table: guest_list_record(&har.guest_list),
        // unused in client ui
        roommate_list: Some(har.roommates.iter().map(|r| r.full()).collect()),
    }
}

/// The guest table as dereth-protocol's record: [`GUEST_COMPARER`]'s bucket count and order.
#[must_use]
pub fn guest_list_record(
    guest_list: &DotNetDict<ObjectGuid, GuestInfo>,
) -> PackedHash<u32, dereth_protocol::trade::GuestInfo> {
    let sorted = sorted(
        guest_list.iter().map(|(k, v)| (*k, v.clone())),
        &GUEST_COMPARER,
    );
    PackedHash {
        table_size: u32::from(GUEST_COMPARER.num_buckets),
        entries: sorted
            .into_iter()
            .map(|(key, value)| (key.full(), guest_info::record(&value)))
            .collect(),
    }
}
