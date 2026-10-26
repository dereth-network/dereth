//! What a character carries: whether it fits its packs, and what it weighs against what its
//! strength can carry.
//!
//! The main pack holds 102 things and seven side packs; a side pack holds its own number. A stack
//! is one thing. Past its capacity a character is slowed but can carry on, so that is a warning;
//! past three times its capacity it could not have picked the last thing up, so that is refused.

use crate::check::{Field, Finding};

/// What the main pack holds, and how many side packs.
pub const MAIN_PACK: u32 = 102;
pub const SIDE_PACKS: u32 = 7;

/// One thing carried in the packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried {
    pub name: String,
    /// The burden of one.
    pub burden: u32,
    pub count: u32,
    /// The most one stack holds: 1 for what does not stack.
    pub stack: u32,
    /// For a side pack, the things it holds; 0 for anything else.
    pub holds: u32,
}

impl Carried {
    /// How many stacks of it there are.
    #[must_use]
    pub fn stacks(&self) -> u32 {
        self.count.max(1).div_ceil(self.stack.max(1))
    }
}

/// What a character carries, added up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Load {
    pub burden: u64,
    /// What its strength carries at full speed.
    pub capacity: u64,
    /// The things in its packs, side packs aside, and the room for them.
    pub things: u32,
    pub room: u32,
    pub side_packs: u32,
    /// The side packs the main pack takes beyond its seven, one for the pack augmentation.
    pub extra_packs: u32,
}

/// The load of a character of base `strength`, with `augmentations` of increased carrying
/// capacity, wearing `worn` burden and carrying `carried`.
#[must_use]
pub fn load(strength: u32, augmentations: u32, worn: u64, carried: &[Carried]) -> Load {
    let capacity = dereth_rules::burden::encumbrance_capacity(
        i32::try_from(strength).unwrap_or(i32::MAX),
        i32::try_from(augmentations).unwrap_or(i32::MAX),
    );
    let mut out = Load {
        burden: worn,
        capacity: u64::try_from(capacity).unwrap_or(0),
        room: MAIN_PACK,
        ..Load::default()
    };
    for c in carried {
        out.burden += u64::from(c.burden) * u64::from(c.count.max(1));
        if c.holds > 0 {
            out.side_packs += c.count.max(1);
            out.room += c.holds * c.count.max(1);
        } else {
            out.things += c.stacks();
        }
    }
    out
}

/// What is wrong with load `l`: more things than the packs hold, more side packs than the main
/// pack takes, or more burden than could be picked up (refused); more than full speed carries (a
/// warning).
#[must_use]
pub fn findings(l: &Load) -> Vec<Finding> {
    let mut out = Vec::new();
    if l.things > l.room {
        out.push(Finding {
            field: Field::Items,
            text: format!(
                "It carries {} things, more than its packs hold ({}).",
                l.things, l.room
            ),
            warning: false,
        });
    }
    if l.side_packs > SIDE_PACKS + l.extra_packs {
        out.push(Finding {
            field: Field::Items,
            text: format!(
                "It carries {} side packs; the main pack takes {}.",
                l.side_packs,
                SIDE_PACKS + l.extra_packs
            ),
            warning: false,
        });
    }
    if l.burden > l.capacity.saturating_mul(3) {
        out.push(Finding {
            field: Field::Items,
            text: format!(
                "Its burden ({}) is more than three times what it can carry ({}).",
                l.burden, l.capacity
            ),
            warning: false,
        });
    } else if l.burden > l.capacity {
        out.push(Finding {
            field: Field::Items,
            text: format!(
                "Its burden ({}) is more than it carries at full speed ({}).",
                l.burden, l.capacity
            ),
            warning: true,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's packs and burden)
    use super::*;

    fn thing(burden: u32, count: u32, stack: u32) -> Carried {
        Carried {
            name: "x".into(),
            burden,
            count,
            stack,
            holds: 0,
        }
    }

    #[test]
    fn stacks_take_a_place_each_and_side_packs_add_their_room() {
        let l = load(100, 0, 0, &[thing(5, 250, 100), thing(50, 5, 1)]);
        assert_eq!((l.things, l.room, l.side_packs), (3 + 5, 102, 0));
        assert_eq!(l.burden, 5 * 250 + 50 * 5);
        assert_eq!(l.capacity, 15_000, "150 for each point of strength");
        assert!(findings(&l).is_empty());
        let packs = Carried {
            holds: 24,
            ..thing(15, 2, 1)
        };
        let full = load(100, 0, 0, &[thing(1, 120, 1), packs.clone()]);
        assert_eq!((full.things, full.room), (120, 150));
        assert!(findings(&full).is_empty(), "the side packs take the rest");
        let over = load(100, 0, 0, &[thing(1, 103, 1)]);
        assert!(findings(&over).iter().any(|f| !f.warning));
        let too_many = load(100, 0, 0, &[Carried { count: 8, ..packs }]);
        assert!(findings(&too_many).iter().any(|f| !f.warning));
    }

    #[test]
    fn past_full_speed_is_a_warning_and_past_three_times_is_refused() {
        let heavy = |burden| findings(&load(100, 0, burden, &[]));
        assert!(heavy(15_000).is_empty());
        let slow = heavy(15_001);
        assert!(slow.len() == 1 && slow[0].warning, "{slow:?}");
        let refused = heavy(45_001);
        assert!(refused.len() == 1 && !refused[0].warning, "{refused:?}");
        // Each carrying capacity augmentation adds a fifth.
        assert_eq!(load(100, 1, 0, &[]).capacity, 18_000);
    }
}
