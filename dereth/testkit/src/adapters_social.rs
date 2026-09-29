//! The one thing the allegiance scenarios of both tiers need and neither could hold: a synthesised
//! allegiance answer.
//!
//! The claims about the allegiance tab are split across the tiers -- the tree walk
//! is the model's and opens no dat, the tab that draws it is the shipped interface's -- and both
//! sides start from the same thing: an `Allegiance_AllegianceUpdate` carrying a hierarchy. That
//! hierarchy is forty lines of blocks the version word gates open, and the two test binaries cannot
//! share a module, so without this it would be written twice and drift once.
//!
//! **There is no recording of a character inside an allegiance with members**, and none is
//! manufactured to pretend otherwise. What is built here is declared
//! synthesised everywhere it is used; the one populated roster the corpus does hold is a recording
//! and is replayed as one.

use dereth_primitives::ObjectId;
use dereth_protocol::social::{
    allegiance_version, AllegianceData, AllegianceHierarchy, AllegiancePools, AllegianceProfile,
    AllegianceUpdate,
};

/// One member record, with the two length gates every record the shard sends carries.
///
/// Without them the record is shorter on the wire and the round trip is lossy by construction, so
/// a synthesised record that left them out would not be the shape a shard ever sends.
#[must_use]
pub fn allegiance_member(id: ObjectId, name: &str, rank: u32, logged_in: bool) -> AllegianceData {
    let mut bitfield = AllegianceData::MAY_PASSUP_EXPERIENCE
        | AllegianceData::HAS_PACKED_LEVEL
        | AllegianceData::HAS_ALLEGIANCE_AGE;
    if logged_in {
        bitfield |= AllegianceData::LOGGED_IN;
    }
    AllegianceData {
        id,
        name: name.to_owned(),
        gender: 1,
        heritage_group: 1,
        rank,
        level: 10 * rank,
        bitfield,
        cp_tithed: 0,
        // A number that differs per member and is nobody's rank, so a row that drew the wrong
        // member's is visible. The low bits only: the ids are high and the product must not wrap.
        cp_cached: 7 * (id.0 & 0xFFF),
        loyalty: 0,
        leadership: 0,
        time_online: 0,
        allegiance_age: 0,
    }
}

/// A whole answer carrying `members`, each paired with the id it hangs off.
///
/// `total_members` and `total_vassals` are the header fields, which are **not** the number of
/// records: the shard sends the player's own corner of the tree and counts the whole allegiance,
/// and telling them apart is the point of two of the claims below.
#[must_use]
pub fn allegiance_answer(
    name: &str,
    total_members: u32,
    total_vassals: u32,
    members: Vec<(Option<ObjectId>, AllegianceData)>,
) -> AllegianceUpdate {
    AllegianceUpdate {
        rank: 3,
        profile: AllegianceProfile {
            total_members,
            total_vassals,
            hierarchy: AllegianceHierarchy {
                // Every block this version gates open has to be present: the writer emits a block
                // only when its `Option` is `Some`, while the reader is driven purely by the
                // version word, so a version-11 hierarchy with one absent encodes to bytes that
                // will not decode.
                version: allegiance_version::APPROVED_VASSAL,
                officers: Some(dereth_protocol::archive::PHash::new(Vec::new())),
                officer_titles: Some(Vec::new()),
                pools: Some(AllegiancePools::default()),
                motd: Some((String::new(), String::new())),
                chat_room_id: Some(0),
                bind_point: Some(dereth_protocol::types::PositionWire::default()),
                allegiance_name: Some((name.to_owned(), 0)),
                is_locked: Some(0),
                approved_vassal: Some(0),
                old_officer: None,
                members,
            },
        },
    }
}
