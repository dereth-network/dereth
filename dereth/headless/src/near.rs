//! `near [<n>]`: the objects round the player, nearest first, as lines a script can read. Each
//! names the object's id, its distance, a word for what it is and its name.

use dereth_client_sdk::contract::radar::bitfield;
use dereth_client_sdk::primitives::ObjectId;

/// One object standing round the player: its id, where it stands from the player (meters), its
/// public description's flags, whether its item type is a creature's, and its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Nearby {
    pub id: ObjectId,
    pub offset: (f32, f32, f32),
    pub flags: u32,
    pub creature: bool,
    pub name: String,
}

/// What the object is, by its description's flags and its item type: the first of player,
/// vendor, portal, lifestone and creature it is, else `object`. A door or a sign may be
/// attackable; only a creature's item type makes a creature.
#[must_use]
pub fn kind(flags: u32, creature: bool) -> &'static str {
    if flags & bitfield::PLAYER != 0 {
        "player"
    } else if flags & bitfield::VENDOR != 0 {
        "vendor"
    } else if flags & bitfield::PORTAL != 0 {
        "portal"
    } else if flags & bitfield::LIFE_STONE != 0 {
        "lifestone"
    } else if creature {
        "creature"
    } else {
        "object"
    }
}

/// The `n` nearest of `objects`, nearest first (by id where two are as near), each as
/// `0x<id> <meters>m <kind> <name>`.
#[must_use]
pub fn rows(objects: &[Nearby], n: usize) -> Vec<String> {
    let distance = |o: &Nearby| {
        let (x, y, z) = o.offset;
        (x * x + y * y + z * z).sqrt()
    };
    let mut sorted: Vec<&Nearby> = objects.iter().collect();
    sorted.sort_by(|a, b| {
        distance(a)
            .total_cmp(&distance(b))
            .then(a.id.0.cmp(&b.id.0))
    });
    sorted
        .into_iter()
        .take(n)
        .map(|o| {
            format!(
                "0x{:08X} {:.1}m {} {}",
                o.id.0,
                distance(o),
                kind(o.flags, o.creature),
                o.name
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a script's listing of what stands round the player)
    use super::*;

    fn at(id: u32, x: f32, flags: u32, name: &str) -> Nearby {
        Nearby {
            id: ObjectId(id),
            offset: (x, 0.0, 0.0),
            flags,
            creature: flags & bitfield::ATTACKABLE != 0,
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_nearest_come_first_each_with_its_distance_kind_and_name() {
        let objects = [
            at(
                0x8000_0003,
                12.0,
                bitfield::VENDOR | bitfield::ATTACKABLE,
                "Girdweld",
            ),
            at(0x5000_0002, 3.0, bitfield::PLAYER, "Bob"),
            at(0x7000_0001, 40.0, bitfield::PORTAL, "Portal to Arwic"),
            at(0x7000_0004, 3.0, bitfield::ATTACKABLE, "Drudge Skulker"),
            at(0x7000_0005, 1.5, 0, "Barrel"),
        ];
        assert_eq!(
            rows(&objects, 4),
            [
                "0x70000005 1.5m object Barrel",
                "0x50000002 3.0m player Bob",
                "0x70000004 3.0m creature Drudge Skulker",
                "0x80000003 12.0m vendor Girdweld",
            ]
        );
        assert_eq!(rows(&objects, 0), Vec::<String>::new());
        assert_eq!(rows(&objects, 99).len(), objects.len());
    }

    #[test]
    fn a_lifestone_and_a_portal_are_named_for_what_they_are() {
        assert_eq!(kind(bitfield::LIFE_STONE, false), "lifestone");
        assert_eq!(
            kind(bitfield::PORTAL | bitfield::ATTACKABLE, false),
            "portal"
        );
        assert_eq!(
            kind(bitfield::ATTACKABLE, false),
            "object",
            "an attackable door"
        );
        assert_eq!(kind(0, true), "creature");
    }
}
