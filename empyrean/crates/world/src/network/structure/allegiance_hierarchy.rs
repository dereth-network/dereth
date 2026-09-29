// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/AllegianceHierarchy.cs
//! Port of `Source/ACE.Server/Network/Structure/AllegianceHierarchy.cs`.
//!
//! Also home to ACE's `BinaryWriter` overloads for `Position`, `Vector3` and `Quaternion`, which
//! ACE declares in this class and the other structures use.

use dereth_protocol::archive::PHash;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::AllegianceOfficerLevel;
use empyrean_entity::{ObjectGuid, Position, Quaternion, Vector3};

use super::allegiance_data::{self, AllegianceData};
use super::allegiance_profile::AllegianceProfile;
use super::hash_comparer::{sorted, GuidComparer};
use super::packable_hash_table;
use crate::network::game_messages::game_message::{ace_str, write_record};

// ACE: AllegianceHierarchy
#[derive(Debug, Clone, Copy, Default)]
pub struct AllegianceHierarchy<'a> {
    // ACE: AllegianceHierarchy.Profile
    pub profile: Option<&'a AllegianceProfile>,
}

// ACE: AllegianceHierarchy.AllegianceHierarchy
#[must_use]
pub fn allegiance_hierarchy_new(profile: &AllegianceProfile) -> AllegianceHierarchy<'_> {
    AllegianceHierarchy {
        profile: Some(profile),
    }
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(AllegianceHierarchy hierarchy)`. `hierarchy.Profile` is never null in ACE (its
/// only caller passes a new profile); `None` here is ACE's `NullReferenceException`.
pub fn write(writer: &mut Vec<u8>, hierarchy: &AllegianceHierarchy<'_>) {
    let record = record(hierarchy);
    let strings = strings(&record);
    write_record(writer, &strings, |w| record.write(w));
}

/// The strings of a hierarchy record, for `write_record`.
#[must_use]
pub fn strings(record: &dereth_protocol::social::AllegianceHierarchy) -> Vec<&str> {
    let mut strings: Vec<&str> = record
        .officer_titles
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    if let Some((motd, by)) = &record.motd {
        strings.extend([motd.as_str(), by.as_str()]);
    }
    if let Some((name, _)) = &record.allegiance_name {
        strings.push(name.as_str());
    }
    strings.extend(record.members.iter().map(|(_, d)| d.name.as_str()));
    strings
}

/// The dereth-protocol record the `Write` extension above writes: ACE's version-0x000B hierarchy, every
/// section present, then the monarch and the records.
#[must_use]
#[allow(clippy::too_many_lines)] // ACE's one method
pub fn record(hierarchy: &AllegianceHierarchy<'_>) -> dereth_protocol::social::AllegianceHierarchy {
    // recordCount = Monarch + Patron + Vassals?
    // 2 in data for small allegiances?
    let mut record_count: u16 = 0;
    let old_version: u16 = 0x000B;
    let officers: DotNetDict<ObjectGuid, AllegianceOfficerLevel> = DotNetDict::new();
    let officer_titles: Vec<String> = Vec::new();
    let monarch_broadcast_time: u32 = 0;
    let monarch_broadcasts_today: u32 = 0;
    let spokes_broadcast_time: u32 = 0;
    let spokes_broadcasts_today: u32 = 0;
    let mut motd = String::new();
    let mut motd_set_by = String::new();
    let mut chat_room_id: u32 = 0;
    let mut bind_point = Position::new();
    let mut allegiance_name = String::new();
    let mut name_last_set_time: u32 = 0;
    let is_locked = false;
    let approved_vassal: i32 = 0;
    let mut monarch_data: Option<&AllegianceData> = None;
    let mut records: Option<Vec<(ObjectGuid, &AllegianceData)>> = None;

    let profile = hierarchy
        .profile
        .expect("ACE: hierarchy.Profile is null (NullReferenceException)");
    let allegiance = profile.allegiance.as_ref();
    let node = profile.node.as_ref();

    if let (Some(allegiance), Some(node)) = (allegiance, node) {
        // only send these to monarch?
        //foreach (var officer in allegiance.Officers)
        //officers.Add(officer.Key, (AllegianceOfficerLevel)officer.Value.Player.AllegianceOfficerRank);

        // not in retail packets, breaks decal
        /*if (allegiance.HasCustomTitles)
        {
            officerTitles.Add(allegiance.GetOfficerTitle(AllegianceOfficerLevel.Speaker));
            officerTitles.Add(allegiance.GetOfficerTitle(AllegianceOfficerLevel.Seneschal));
            officerTitles.Add(allegiance.GetOfficerTitle(AllegianceOfficerLevel.Castellan));
        }*/

        // Not ACE's (retail, V289; the retail captures): an allegiance with no name sends an
        // empty name, as retail did; ACE sent the monarch's name. The client only stores this field.
        allegiance_name.clone_from(
            allegiance
                .allegiance_name
                .as_ref()
                .unwrap_or(&String::new()),
        );
        //motd = allegiance.AllegianceMotd ?? "";
        //motdSetBy = allegiance.AllegianceMotdSetBy ?? "";
        motd = String::new(); // fixes decal AllegianceUpdate parsing
        motd_set_by = String::new();
        chat_room_id = allegiance.biota_id;
        // Not ACE's (retail, V289; the retail captures): an allegiance with no name
        // carries the current time here; ACE sends 0.
        name_last_set_time = allegiance.unnamed_name_time;

        if let Some(sanctuary) = allegiance.sanctuary {
            bind_point = sanctuary;
        }

        // aclogview (verify):
        // i == 0 : monarch (no guid)
        // i == 1 : patron
        // i == 2 : peer?
        // i  > 2 : vassals

        // peers = others with the same patron?

        record_count = 1; // monarch
        if node.patron.as_ref().is_some_and(|p| !p.is_monarch) {
            // patron
            record_count += 1;
        }
        if !node.is_monarch {
            // self
            record_count += 1;
        }
        if node.total_vassals > 0 {
            // vassals: `recordCount += (ushort)node.TotalVassals`, unchecked
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                record_count = record_count.wrapping_add(node.total_vassals as u16);
            }
        }
        //Console.WriteLine("Records: " + recordCount);

        // monarch
        monarch_data = Some(&allegiance.monarch);

        if record_count > 1 {
            let mut r = Vec::new();

            // patron
            if let Some(patron) = node.patron.as_ref().filter(|p| !p.is_monarch) {
                r.push((node.monarch_player_guid, &patron.data));
            }

            // self
            if !node.is_monarch {
                let patron = node
                    .patron
                    .as_ref()
                    .expect("ACE: node.Patron is null (NullReferenceException)");
                r.push((patron.player_guid, &node.data));
            }

            // vassals
            if node.total_vassals > 0 {
                for vassal in &node.vassals {
                    r.push((node.player_guid, vassal));
                }
            }
            records = Some(r);
        }
    }

    // `recordCount` is the members the record holds: the monarch and each record.
    let mut members = Vec::new();
    if let Some(monarch_data) = monarch_data {
        members.push((None, allegiance_data::record(monarch_data)));
    }
    if let Some(records) = records {
        members.extend(
            records
                .iter()
                .map(|(item1, item2)| (Some((*item1).into()), allegiance_data::record(item2))),
        );
    }
    debug_assert_eq!(
        usize::from(record_count),
        members.len(),
        "TotalVassals is the vassal list's length"
    );

    dereth_protocol::social::AllegianceHierarchy {
        version: u32::from(old_version),
        officers: Some(officers_record(&officers)),
        old_officer: None,
        officer_titles: Some(officer_titles.iter().map(|s| ace_str(s.as_str())).collect()),
        pools: Some(dereth_protocol::social::AllegiancePools {
            monarch_broadcast_time: monarch_broadcast_time.cast_signed(),
            monarch_broadcasts_today,
            spokes_broadcast_time: spokes_broadcast_time.cast_signed(),
            spokes_broadcasts_today,
        }),
        motd: Some((ace_str(motd.as_str()), ace_str(motd_set_by.as_str()))),
        chat_room_id: Some(chat_room_id),
        bind_point: Some(position_record(&bind_point)),
        allegiance_name: Some((
            ace_str(allegiance_name.as_str()),
            name_last_set_time.cast_signed(),
        )),
        is_locked: Some(i32::from(is_locked)),
        approved_vassal: Some(approved_vassal.cast_unsigned()),
        members,
    }
}

/// The officer table as dereth-protocol's record. ACE's header is a `PackableHashTable` one (`ushort`
/// count, `ushort` 256 buckets); the record's is a bucket byte over a 24-bit count, the same four
/// bytes for 256 (0x0100) buckets.
#[must_use]
pub fn officers_record(
    officers: &DotNetDict<ObjectGuid, AllegianceOfficerLevel>,
) -> PHash<u32, u32> {
    let guid_comparer = GuidComparer::new(ACTUAL_NUM_BUCKETS);
    let officers = sorted(officers.iter().map(|(k, v)| (*k, *v)), &guid_comparer);
    PHash {
        bucket_index: packable_hash_table::bucket_byte(HEADER_NUM_BUCKETS),
        entries: officers
            .into_iter()
            .map(|(key, value)| (key.full(), value.0))
            .collect(),
    }
}

/// `writer.Write(Position)` as dereth-protocol's record: cell, origin, then the rotation W first.
#[must_use]
pub fn position_record(position: &Position) -> dereth_protocol::types::PositionWire {
    dereth_protocol::types::PositionWire {
        objcell_id: position.cell(),
        frame: dereth_protocol::types::Frame {
            origin: empyrean_entity::shared_types::wire_vec3(position.pos()),
            orientation: empyrean_entity::shared_types::wire_quat(position.rotation()),
        },
    }
}

/// aside from RestrictionDB, this appears to be the only other place in the client that calls
/// PHashTable/IntrusiveHashTable constructor directly: 256 is ignored, and 23 is used.
const HEADER_NUM_BUCKETS: i32 = 256;
const ACTUAL_NUM_BUCKETS: u16 = 23;

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(Dictionary<ObjectGuid, AllegianceOfficerLevel> officers)`: always sent as empty
/// in retail?
pub fn write_officers(
    writer: &mut Vec<u8>,
    officers: &DotNetDict<ObjectGuid, AllegianceOfficerLevel>,
) {
    write_record(writer, &[], |w| {
        w.phash(&officers_record(officers), |w, k, v| {
            w.u32(*k);
            w.u32(*v);
            Ok(())
        })
    });
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(List<string> strings)`.
pub fn write_strings(writer: &mut Vec<u8>, strings: &[String]) {
    let list: Vec<String> = strings.iter().map(|s| ace_str(s.as_str())).collect();
    let originals: Vec<&str> = strings.iter().map(String::as_str).collect();
    write_record(writer, &originals, |w| {
        w.packed_list(&list, |w, s| w.pstring(s))
    });
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(Position position)`: cell, origin, then the rotation W first.
pub fn write_position(writer: &mut Vec<u8>, position: &Position) {
    write_record(writer, &[], |w| position_record(position).write(w));
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(Vector3 v)`.
pub fn write_vector3(writer: &mut Vec<u8>, v: Vector3) {
    write_record(writer, &[], |w| {
        empyrean_entity::shared_types::wire_vec3(v).write(w)
    });
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(Quaternion q)`: W first.
pub fn write_quaternion(writer: &mut Vec<u8>, q: Quaternion) {
    write_record(writer, &[], |w| {
        empyrean_entity::shared_types::wire_quat(q).write(w)
    });
}

// ACE: AllegianceHierarchyExtensions.Write
/// `writer.Write(List<Tuple<ObjectGuid, AllegianceData>> records)`: no count (ACE comments it out).
pub fn write_records(writer: &mut Vec<u8>, records: &[(ObjectGuid, &AllegianceData)]) {
    //writer.Write(records.Count);
    let list: Vec<(u32, dereth_protocol::social::AllegianceData)> = records
        .iter()
        .map(|(item1, item2)| (item1.full(), allegiance_data::record(item2)))
        .collect();
    let strings: Vec<&str> = list.iter().map(|(_, d)| d.name.as_str()).collect();
    write_record(writer, &strings, |w| {
        for (patron, data) in &list {
            w.u32(*patron);
            data.write(w)?;
        }
        Ok(())
    });
}
