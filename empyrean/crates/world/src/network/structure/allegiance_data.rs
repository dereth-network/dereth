// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/AllegianceData.cs
//! Port of `Source/ACE.Server/Network/Structure/AllegianceData.cs`.
//!
//! ACE's `AllegianceData` holds an `AllegianceNode` and resolves the node's player inside `Write`
//! (`PlayerManager.FindByGuid` and the player's properties). Here the reads `Write` makes of the
//! node are taken when the data is built
//! ([`allegiance_data_new`]) and kept in [`AllegianceDataNode`]; `Write` then does the rest of
//! ACE's work (the bitfield and the byte layout). Built and written in one synchronous step, as ACE
//! does, the bytes are the same (DIVERGE arch).

use empyrean_entity::enums::{Gender, HeritageGroup};

use crate::network::game_messages::game_message::{ace_str, write_record};

/// `ACE.Server.Network.Enum.AllegianceIndex` (`[Flags]`, an `int` enum). Not ported yet by its
/// owner; the members `Write` uses.
pub mod allegiance_index {
    pub const LOGGED_IN: u32 = 0x1;
    pub const HAS_ALLEGIANCE_AGE: u32 = 0x4;
    pub const HAS_PACKED_LEVEL: u32 = 0x8;
    pub const MAY_PASSUP_EXPERIENCE: u32 = 0x10;
}

/// The values `AllegianceDataExtensions.Write` reads from a non-null node and its player.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllegianceDataNode {
    /// `player.Guid.Full`.
    pub character_id: u32,
    /// `player.AllegianceXPCached`.
    pub allegiance_xp_cached: u64,
    /// `player.AllegianceXPGenerated`.
    pub allegiance_xp_generated: u64,
    /// `playerIsOnline` from `PlayerManager.FindByGuid`.
    pub player_is_online: bool,
    /// `node.IsMonarch`.
    pub is_monarch: bool,
    /// `node.Player.ExistedBeforeAllegianceXpChanges`.
    pub existed_before_allegiance_xp_changes: bool,
    /// `player.Gender` (`int?`, cast with `(Gender)`: a null here is ACE's exception).
    pub gender: i32,
    /// `player.Heritage`.
    pub heritage: i32,
    /// `node.Rank`.
    pub rank: u32,
    /// `player.Level`.
    pub level: i32,
    /// `player.GetCurrentLoyalty()`.
    pub loyalty: i32,
    /// `player.GetCurrentLeadership()`.
    pub leadership: i32,
    /// `player.Name`.
    pub name: String,
    /// Not ACE's (retail, V285; the retail captures): the in-game seconds the player has
    /// been sworn to its patron (ACE sends 0).
    pub time_online: u32,
    /// Not ACE's (retail, V285): the real seconds the player has been sworn to its patron (ACE sends 0).
    pub allegiance_age: u32,
}

// ACE: AllegianceData
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllegianceData {
    // ACE: AllegianceData.Node
    /// The node (as the values `Write` reads from it), or `null`.
    pub node: Option<AllegianceDataNode>,
}

// ACE: AllegianceData.AllegianceData
/// `new AllegianceData(AllegianceNode node)`, with the node's reads taken here (see the module
/// documentation). `None` is a null node; a node no longer in its allegiance reads as null.
pub fn allegiance_data_new(
    w: &mut crate::World,
    node: Option<crate::entity::allegiance_node::NodeRef>,
) -> AllegianceData {
    let node = node.and_then(|n| {
        let tree = crate::world_objects::allegiance::tree(w, n.allegiance)?.clone();
        let id = crate::world_objects::allegiance::member_node(w, n)?;
        Some(crate::world_objects::allegiance::node_data(w, &tree, id))
    });
    AllegianceData { node }
}

// ACE: AllegianceDataExtensions.Write
/// `writer.Write(AllegianceData data)`.
pub fn write(writer: &mut Vec<u8>, data: &AllegianceData) {
    let record = record(data);
    write_record(writer, &[record.name.as_str()], |w| record.write(w));
}

/// The dereth-protocol record the `Write` extension above writes: ACE's values, with its narrowing to
/// the wire's byte and ushort fields done here.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C# `(ushort)`/`(uint)`/`(byte)` narrowing
pub fn record(data: &AllegianceData) -> dereth_protocol::social::AllegianceData {
    let mut character_id: u32 = 0;
    let mut cp_cached: u32 = 0;
    let mut cp_tithed: u32 = 0;
    let mut bitfield = allegiance_index::HAS_ALLEGIANCE_AGE | allegiance_index::HAS_PACKED_LEVEL;
    let mut gender = Gender::Female;
    let mut hg = HeritageGroup::Aluvian;
    let mut rank: u16 = 0;
    let mut level: u32 = 0;
    let mut loyalty: u16 = 0;
    let mut leadership: u16 = 0;
    let u_time_online: u64 = 0;
    let mut time_online: u32 = 0;
    let mut allegiance_age: u32 = 0;
    let mut name = String::new();

    if let Some(node) = &data.node {
        character_id = node.character_id;
        // `(uint)Math.Min(ulong, uint.MaxValue)`.
        cp_cached = node.allegiance_xp_cached.min(u64::from(u32::MAX)) as u32;
        cp_tithed = node.allegiance_xp_generated.min(u64::from(u32::MAX)) as u32;

        if node.player_is_online {
            bitfield |= allegiance_index::LOGGED_IN;
        }

        if !node.is_monarch && node.existed_before_allegiance_xp_changes {
            bitfield |= allegiance_index::MAY_PASSUP_EXPERIENCE;
        }

        gender = Gender(node.gender);
        hg = HeritageGroup(node.heritage);
        rank = node.rank as u16;
        level = node.level as u32;
        loyalty = node.loyalty as u16;
        leadership = node.leadership as u16;

        //if (!node.IsMonarch)
        //{
        // TODO: Get/set total time sworn to patron (allegianceAge) and total in-game time since swearing to patron (timeOnline)
        //}

        // Not ACE's (retail, V285; the retail captures): the times the member has been
        // sworn to its patron (zero for a monarch, which has none).
        time_online = node.time_online;
        allegiance_age = node.allegiance_age;

        name.clone_from(&node.name);
    }

    // The record writes the level when the bitfield says it is packed, and the two times when it
    // says the allegiance age is there; without the age flag it writes the time online as a
    // double, which for ACE's constant 0 is the same eight zero bytes as its `ulong`.
    debug_assert!(u_time_online == 0);
    dereth_protocol::social::AllegianceData {
        id: dereth_primitives::ObjectId(character_id),
        name: ace_str(name.as_str()),
        gender: u32::from(gender.0 as u8),
        heritage_group: u32::from(hg.0 as u8),
        rank: u32::from(rank),
        level,
        bitfield,
        cp_tithed,
        cp_cached,
        loyalty: u32::from(loyalty),
        leadership: u32::from(leadership),
        time_online: time_online.cast_signed(),
        allegiance_age: allegiance_age.cast_signed(),
    }
}
