// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/RestrictionDB.cs
//! Port of `Source/ACE.Server/Network/Structure/RestrictionDB.cs`.

use dereth_protocol::archive::PHash;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::ObjectGuid;

use super::hash_comparer::{sorted, GuidComparer};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: RestrictionDB
/// The restriction DB contains the access control list for a house.
#[derive(Debug, Clone)]
pub struct RestrictionDB {
    // ACE: RestrictionDB.HouseOwner
    pub house_owner: u32,
    // ACE: RestrictionDB.Version
    /// If high word is not 0, this value indicates the version of the message.
    pub version: u32,
    // ACE: RestrictionDB.OpenStatus
    /// 0 = private dwelling, 1 = open to public
    pub open_status: bool,
    // ACE: RestrictionDB.MonarchID
    /// Allegiance monarch (if allegiance access granted)
    pub monarch_id: ObjectGuid,
    // ACE: RestrictionDB.Table
    /// Set of permissions on a per user basis. Key is the character id, value is 0 = dwelling
    /// access only, 1 = storage access as well.
    pub table: DotNetDict<ObjectGuid, u32>,
}

impl Default for RestrictionDB {
    // ACE: RestrictionDB.RestrictionDB
    /// `new RestrictionDB()`: `Version = 0x10000002`, an empty table, no owner.
    fn default() -> Self {
        RestrictionDB {
            house_owner: 0,
            version: 0x1000_0002,
            open_status: false,
            monarch_id: ObjectGuid::default(),
            table: DotNetDict::new(),
        }
    }
}

// ACE: RestrictionDB.RestrictionDB
/// `new RestrictionDB(House house)`: the guests (except the monarch), and every other character
/// on the owner's account with storage access. `None` is a null house.
pub fn restriction_db_new(w: &World, house: Option<ObjectGuid>) -> RestrictionDB {
    let mut r = RestrictionDB::default();

    let Some(house) = house else { return r };
    let h = w.objects.get(house).expect("ACE: house is null");

    r.house_owner = h.house_owner().unwrap_or(0);

    r.open_status = h.open_to_everyone();

    if let Some(monarch_id) = h.monarch_id() {
        r.monarch_id = ObjectGuid::new(monarch_id); // for allegiance guest/storage access
    }

    for (key, value) in crate::world_objects::house::fields(w, house)
        .guests
        .iter()
        .map(|(k, v)| (*k, *v))
    {
        if key != r.monarch_id {
            r.table.add(key, u32::from(value));
        }
    }

    let Some(house_owner) = h.house_owner() else {
        return r;
    };

    // add in players on house owner's account
    let owner = shims::player_manager_find_by_guid(w, house_owner);

    // added for people deleting accounts from their account db...
    let Some(account_id) = owner.and_then(|o| o.account_id) else {
        log::info!(
            "RestrictionDB({:08X}): couldn't find house owner {:08X}",
            shims::house_instance(w, house).unwrap_or(0),
            house_owner
        );
        return r;
    };

    let account_players =
        shims::player_manager_get_account_players(w, account_id).unwrap_or_default();

    for (key, value) in account_players {
        if key == r.house_owner {
            continue;
        }

        r.table.try_add(value.guid, 1);
    }
    r
}

// ACE: RestrictionDBExtensions.Write
/// `writer.Write(RestrictionDB restrictions)`.
pub fn write(writer: &mut Vec<u8>, restrictions: &RestrictionDB) {
    write_record(writer, &[], |w| record(restrictions).write(w));
}

/// this # of buckets was sent over the wire in retail header; however, this value ends up being
/// unused, and the "real" # of buckets originates from a hardcoded value in g_bucketSizeArray in
/// the client constant data
const HEADER_NUM_BUCKETS: i32 = 768;

/// in RestrictionDB constructor in acclient, client uses PHashTable for this (as opposed to the
/// typical PackableHashTable) which inits an IntrusiveHashTable with size 64; this gets bumped up
/// to the next largest value in a hardcoded g_bucketSizeArray, which is 89
const ACTUAL_NUM_BUCKETS: u16 = 89;

// ACE: RestrictionDBExtensions.Write
/// `writer.Write(Dictionary<ObjectGuid, uint> db)`, in the client's bucket order.
pub fn write_table(writer: &mut Vec<u8>, db: &DotNetDict<ObjectGuid, u32>) {
    write_record(writer, &[], |w| {
        w.phash(&table_record(db), |w, k, v| {
            w.u32(k.0);
            w.u32(*v);
            Ok(())
        })
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(restrictions: &RestrictionDB) -> dereth_protocol::types::RestrictionDb {
    debug_assert!(
        restrictions.version & 0xFFFF_0000 != 0,
        "the oldest restriction form has no version dword"
    );
    dereth_protocol::types::RestrictionDb {
        version: restrictions.version,
        bitmask: u32::from(restrictions.open_status),
        monarch_iid: restrictions.monarch_id.into(),
        table: table_record(&restrictions.table),
    }
}

/// The table as dereth-protocol's record, in the client's bucket order. ACE's header is a
/// `PackableHashTable` one (`ushort` count, `ushort` 768 buckets); the record's header is a bucket
/// byte over a 24-bit count, which is the same four bytes for 768 (0x0300) buckets.
#[must_use]
pub fn table_record(db: &DotNetDict<ObjectGuid, u32>) -> PHash<dereth_primitives::ObjectId, u32> {
    let guid_comparer = GuidComparer::new(ACTUAL_NUM_BUCKETS);
    let sorted = sorted(db.iter().map(|(k, v)| (*k, *v)), &guid_comparer);
    PHash {
        bucket_index: super::packable_hash_table::bucket_byte(HEADER_NUM_BUCKETS),
        entries: sorted.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}
