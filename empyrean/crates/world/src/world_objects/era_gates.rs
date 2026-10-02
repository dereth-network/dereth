//! Not ACE: the systems a world lacks (`EraFeatures`), refused where a player asks for one.
//!
//! A world's systems are its era's table with its `[era]` settings over it (`World::era`). The
//! client is told the same set and hides what the world lacks, but a client that asks anyway is
//! refused here: each gate answers whether the world has the system and, when it does not, tells
//! the player so. The gates themselves stand at ACE's sites, each marked with its divergence row.

use empyrean_entity::enums::{ChatMessageType, HouseType};
use empyrean_entity::ObjectGuid;

use crate::world_objects::player;
use crate::World;

/// Whether the world has the system: `has` is the system's flag, `what` its name in the line the
/// player is told when it is missing ("This world has no {what}.").
pub fn has(w: &mut World, player: ObjectGuid, has: bool, what: &str) -> bool {
    if !has {
        player::send_message(
            w,
            player,
            &format!("This world has no {what}."),
            ChatMessageType::Broadcast,
        );
    }
    has
}

/// Whether the world has houses of `kind`: apartments with apartments, every other kind with
/// housing.
#[must_use]
pub fn has_house_kind(w: &World, kind: HouseType) -> bool {
    if kind == HouseType::Apartment {
        w.era.features.apartments
    } else {
        w.era.features.housing
    }
}

/// [`has_house_kind`], telling the player when the world lacks it.
pub fn has_house(w: &mut World, player: ObjectGuid, kind: HouseType) -> bool {
    let what = if kind == HouseType::Apartment {
        "apartments"
    } else {
        "housing"
    };
    let available = has_house_kind(w, kind);
    has(w, player, available, what)
}
