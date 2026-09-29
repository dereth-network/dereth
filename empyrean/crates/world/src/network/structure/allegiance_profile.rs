// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/AllegianceProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/AllegianceProfile.cs`.
//!
//! ACE's profile holds the live `Allegiance` and `AllegianceNode`, which `Write` (and
//! `AllegianceHierarchy`'s `Write`) read. Here the profile carries the values those writers read
//! ([`AllegianceView`], [`AllegianceNodeView`]), taken when the profile is built (DIVERGE arch, as
//! for `AllegianceData`); `world_objects/allegiance.rs` takes them.

use empyrean_entity::{ObjectGuid, Position};

use super::allegiance_data::AllegianceData;
use super::allegiance_hierarchy::{self, allegiance_hierarchy_new};
use crate::network::game_messages::game_message::write_record;

/// What the writers read from the `Allegiance`.
#[derive(Debug, Clone, Default)]
pub struct AllegianceView {
    /// `allegiance.AllegianceName`.
    pub allegiance_name: Option<String>,
    /// `allegiance.Monarch.Player.Name`.
    pub monarch_player_name: String,
    /// `allegiance.Biota.Id`, the allegiance chat room.
    pub biota_id: u32,
    /// `allegiance.Sanctuary`.
    pub sanctuary: Option<Position>,
    /// `new AllegianceData(allegiance.Monarch)`.
    pub monarch: AllegianceData,
    /// Not ACE's (retail, V289; the retail captures): the time field of the allegiance
    /// name. Retail filled it with the server's current time (Unix seconds) at each send when the
    /// allegiance has no name; 0 for a named one, as ACE sends it.
    pub unnamed_name_time: u32,
}

/// The patron of a node, as the hierarchy writer reads it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllegiancePatronView {
    /// `node.Patron.IsMonarch`.
    pub is_monarch: bool,
    /// `node.Patron.PlayerGuid`.
    pub player_guid: ObjectGuid,
    /// `new AllegianceData(node.Patron)`.
    pub data: AllegianceData,
}

/// What the writers read from the `AllegianceNode`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllegianceNodeView {
    /// `node.PlayerGuid`.
    pub player_guid: ObjectGuid,
    /// `node.IsMonarch`.
    pub is_monarch: bool,
    /// `node.TotalFollowers`.
    pub total_followers: i32,
    /// `node.TotalVassals`.
    pub total_vassals: i32,
    /// `node.Monarch.TotalFollowers`.
    pub monarch_total_followers: i32,
    /// `node.Monarch.PlayerGuid`.
    pub monarch_player_guid: ObjectGuid,
    /// `node.Patron`.
    pub patron: Option<AllegiancePatronView>,
    /// `new AllegianceData(node)`.
    pub data: AllegianceData,
    /// `new AllegianceData(vassal)` for each of `node.Vassals.Values`, in dictionary order.
    pub vassals: Vec<AllegianceData>,
}

// ACE: AllegianceProfile
#[derive(Debug, Clone, Default)]
pub struct AllegianceProfile {
    // ACE: AllegianceProfile.Allegiance
    pub allegiance: Option<AllegianceView>,
    // ACE: AllegianceProfile.Node
    pub node: Option<AllegianceNodeView>,
}

// ACE: AllegianceProfile.AllegianceProfile
/// `new AllegianceProfile(Allegiance allegiance, AllegianceNode node)`: the allegiance by its
/// guid, the node by its reference. `Write` reads them only when both are set, so the views are
/// taken only then (a node no longer in its allegiance reads as null).
pub fn allegiance_profile_new(
    w: &mut crate::World,
    allegiance: Option<ObjectGuid>,
    node: Option<crate::entity::allegiance_node::NodeRef>,
) -> AllegianceProfile {
    let (Some(allegiance), Some(node)) = (allegiance, node) else {
        return AllegianceProfile::default();
    };
    let Some(node) = crate::world_objects::allegiance::allegiance_node_view(w, node) else {
        return AllegianceProfile::default();
    };
    let allegiance = crate::world_objects::allegiance::allegiance_view(w, allegiance);
    AllegianceProfile {
        allegiance: Some(allegiance),
        node: Some(node),
    }
}

// ACE: AllegianceProfileExtensions.Write
/// `writer.Write(AllegianceProfile profile)`: the member counts, then the hierarchy.
pub fn write(writer: &mut Vec<u8>, profile: &AllegianceProfile) {
    // uint - totalMembers - The number of allegiance members.
    // uint - totalVassals - Your personal number of followers.
    // AllegianceHierarchy - allegianceHierarchy

    let mut total_members: u32 = 0;
    let mut total_vassals: u32 = 0;

    if let (Some(_), Some(node)) = (&profile.allegiance, &profile.node) {
        total_members = node.monarch_total_followers.cast_unsigned().wrapping_add(1); // includes monarch
        total_vassals = node.total_followers.cast_unsigned();
    }

    let allegiance_hierarchy = allegiance_hierarchy_new(profile);
    let record = dereth_protocol::social::AllegianceProfile {
        total_members,
        total_vassals,
        hierarchy: allegiance_hierarchy::record(&allegiance_hierarchy),
    };
    let strings = allegiance_hierarchy::strings(&record.hierarchy);
    write_record(writer, &strings, |w| record.write(w));
}

/// Not ACE's (retail, V289; the retail captures): the profile with the fields that move
/// with the clock cleared (the unnamed allegiance's name time and each record's sworn times), so
/// that two profiles compare equal when the player's view of the allegiance is the same. What
/// changes the view is a pass-up counter or the structure: members, parents, ranks, levels, the
/// online bit, loyalty and leadership, names, the totals.
#[must_use]
pub fn without_clock_fields(profile: &AllegianceProfile) -> AllegianceProfile {
    fn clear(data: &mut AllegianceData) {
        if let Some(node) = data.node.as_mut() {
            node.time_online = 0;
            node.allegiance_age = 0;
        }
    }
    let mut profile = profile.clone();
    if let Some(allegiance) = profile.allegiance.as_mut() {
        allegiance.unnamed_name_time = 0;
        clear(&mut allegiance.monarch);
    }
    if let Some(node) = profile.node.as_mut() {
        if let Some(patron) = node.patron.as_mut() {
            clear(&mut patron.data);
        }
        clear(&mut node.data);
        node.vassals.iter_mut().for_each(clear);
    }
    profile
}
