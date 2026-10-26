//! Gear: the places a character wears and wields things, what a wield requirement asks of a
//! build, and the tier of loot a character of a level hunts.
//!
//! **Each slot is rolled on its own.** A slot's roll is seeded from the recipe's seed, the slot
//! and how many times the slot was rolled again, so rolling one slot again leaves every other as
//! it was.
//!
//! **The tier.** A character's gear is loot of the tier its level hunts at: the highest tier whose
//! creatures' median level is at or below its level. Where the place it plays in drops a higher
//! tier, the place's is taken, so a character is never geared below its level. Better gear is one
//! tier up.

use serde::{Deserialize, Serialize};

use crate::order::Gear;
use crate::{Build, Tables, ATTRIBUTES, ATTRIBUTE_NAMES};

/// A place a character wears or wields something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Weapon,
    /// A shield, or a second weapon in the off hand.
    Shield,
    Head,
    Chest,
    Abdomen,
    UpperArms,
    LowerArms,
    Hands,
    UpperLegs,
    LowerLegs,
    Feet,
    Neck,
    LeftWrist,
    RightWrist,
    LeftFinger,
    RightFinger,
    Cloak,
    Trinket,
}

/// Where a weapon is wielded: in one hand, a missile weapon, a caster, or two hands.
pub const WEAPON_LOCATIONS: u32 = 0x0010_0000 | 0x0040_0000 | 0x0100_0000 | 0x0200_0000;

/// One-handed melee.
pub const ONE_HANDED: u32 = 0x0010_0000;

/// Where ammunition goes.
pub const AMMUNITION: u32 = 1 << 23;

/// The armor slots, covered once each by a suit.
pub const ARMOR_LOCATIONS: u32 = 0x0000_7F21;

impl Slot {
    /// Every slot, in the order a character sheet lists them.
    pub const ALL: [Self; 18] = [
        Self::Weapon,
        Self::Shield,
        Self::Head,
        Self::Chest,
        Self::Abdomen,
        Self::UpperArms,
        Self::LowerArms,
        Self::Hands,
        Self::UpperLegs,
        Self::LowerLegs,
        Self::Feet,
        Self::Neck,
        Self::LeftWrist,
        Self::RightWrist,
        Self::LeftFinger,
        Self::RightFinger,
        Self::Cloak,
        Self::Trinket,
    ];

    /// The armor slots, in the order a suit is rolled: the body first, so a piece that covers
    /// several (a coat, a hauberk) comes before the ones it would leave nothing for.
    pub const ARMOR: [Self; 9] = [
        Self::Chest,
        Self::Abdomen,
        Self::UpperArms,
        Self::LowerArms,
        Self::UpperLegs,
        Self::LowerLegs,
        Self::Head,
        Self::Hands,
        Self::Feet,
    ];

    /// The jewelry slots.
    pub const JEWELRY: [Self; 6] = [
        Self::Neck,
        Self::LeftWrist,
        Self::RightWrist,
        Self::LeftFinger,
        Self::RightFinger,
        Self::Trinket,
    ];

    /// Where on the character the slot is, as the server's equip masks say it; the weapon slot
    /// is any place a weapon is wielded.
    #[must_use]
    pub const fn mask(self) -> u32 {
        match self {
            Self::Weapon => WEAPON_LOCATIONS,
            Self::Shield => 0x0020_0000,
            Self::Head => 0x0000_0001,
            Self::Chest => 0x0000_0200,
            Self::Abdomen => 0x0000_0400,
            Self::UpperArms => 0x0000_0800,
            Self::LowerArms => 0x0000_1000,
            Self::Hands => 0x0000_0020,
            Self::UpperLegs => 0x0000_2000,
            Self::LowerLegs => 0x0000_4000,
            Self::Feet => 0x0000_0100,
            Self::Neck => 0x0000_8000,
            Self::LeftWrist => 0x0001_0000,
            Self::RightWrist => 0x0002_0000,
            Self::LeftFinger => 0x0004_0000,
            Self::RightFinger => 0x0008_0000,
            Self::Cloak => 0x0800_0000,
            Self::Trinket => 0x0400_0000,
        }
    }

    /// Its name in a recipe and in the text (`upper_arms`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Shield => "shield",
            Self::Head => "head",
            Self::Chest => "chest",
            Self::Abdomen => "abdomen",
            Self::UpperArms => "upper_arms",
            Self::LowerArms => "lower_arms",
            Self::Hands => "hands",
            Self::UpperLegs => "upper_legs",
            Self::LowerLegs => "lower_legs",
            Self::Feet => "feet",
            Self::Neck => "neck",
            Self::LeftWrist => "left_wrist",
            Self::RightWrist => "right_wrist",
            Self::LeftFinger => "left_finger",
            Self::RightFinger => "right_finger",
            Self::Cloak => "cloak",
            Self::Trinket => "trinket",
        }
    }

    /// Whether it is an armor slot.
    #[must_use]
    pub const fn armor(self) -> bool {
        self.mask() & ARMOR_LOCATIONS != 0
    }

    /// Where an item that may go where `valid` says is worn in this slot: an armor piece over
    /// everything it covers, a weapon where it is wielded, anything else in the slot alone. `None`
    /// when the item cannot go in this slot.
    #[must_use]
    pub fn location(self, valid: u32) -> Option<u32> {
        match self {
            Self::Weapon => [0x0010_0000, 0x0040_0000, 0x0100_0000, 0x0200_0000]
                .into_iter()
                .find(|bit| valid & bit != 0),
            _ if self.armor() => {
                (valid & self.mask() != 0 && valid & !ARMOR_LOCATIONS == 0).then_some(valid)
            }
            _ => (valid & self.mask() != 0).then_some(self.mask()),
        }
    }

    /// The seed this slot is rolled with, from the recipe's seed and the times it was rolled
    /// again.
    #[must_use]
    pub fn seed(self, seed: u64, roll: u32) -> u64 {
        let index = Self::ALL
            .iter()
            .position(|s| *s == self)
            .and_then(|i| u64::try_from(i).ok())
            .unwrap_or(0);
        mix(mix(seed ^ (index + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ u64::from(roll))
    }
}

/// The seed of everything a suit shares (its armor type), from the recipe's seed.
#[must_use]
pub fn suit_seed(seed: u64) -> u64 {
    mix(seed ^ 0x5157_0000_0000_0000)
}

/// A 64-bit mix (SplitMix64's finisher): nearby seeds give unrelated rolls.
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One wield requirement as an item states it: what kind (a skill, an attribute, a level...),
/// of what, and how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub kind: i32,
    pub of: i32,
    pub at: i32,
}

/// Whether a character built as `b` meets `req` with its base values (no buffs): a skill's or
/// attribute's base, a vital's maximum, its level, a skill's training, its heritage. A kind it
/// cannot tell is not met.
#[must_use]
pub fn meets(b: &Build, t: &Tables<'_>, req: &Requirement) -> bool {
    let at = u32::try_from(req.at).unwrap_or(0);
    let of = u32::try_from(req.of).unwrap_or(0);
    match req.kind {
        // Skill or raw skill: the base value.
        1 | 2 => b.skill(t, of) >= at,
        3 | 4 => b.attribute(of) >= at,
        // A vital's maximum, by its current id or its maximum's.
        5 | 6 => b.vital(t, of | 1) >= at,
        7 => b.budget.level >= at,
        8 => b
            .skills
            .iter()
            .find(|s| s.id == of)
            .is_some_and(|s| s.sac as u32 >= at),
        12 => b.heritage == at,
        _ => false,
    }
}

/// What `req` asks, as a player reads it ("Heavy Weapons 250", "level 180").
#[must_use]
pub fn requirement_text(t: &Tables<'_>, req: &Requirement) -> String {
    let of = u32::try_from(req.of).unwrap_or(0);
    let at = req.at;
    match req.kind {
        1 | 2 => format!("{} {at}", t.skill_name(of)),
        3 | 4 => {
            let i = ATTRIBUTES.iter().position(|a| *a == of);
            format!("{} {at}", i.map_or("attribute", |i| ATTRIBUTE_NAMES[i]))
        }
        7 => format!("level {at}"),
        8 => format!("{} trained", t.skill_name(of)),
        kind => format!("requirement {kind} {at}"),
    }
}

/// Each tier's median creature level, from `(level, tier)` of each creature that drops loot; a
/// tier no creature drops is never reached (`u32::MAX`).
#[must_use]
pub fn level_medians(creatures: impl IntoIterator<Item = (u32, u32)>) -> [u32; 8] {
    let mut levels: [Vec<u32>; 8] = Default::default();
    for (level, tier) in creatures {
        if let Some(v) = usize::try_from(tier)
            .ok()
            .filter(|t| (1..=8).contains(t))
            .and_then(|t| levels.get_mut(t - 1))
        {
            v.push(level);
        }
    }
    levels.map(|mut v| {
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(u32::MAX)
    })
}

/// The tier a character of `level` hunts at: the highest whose median creature is at or below
/// it, at least 1.
#[must_use]
pub fn tier_for_level(medians: &[u32; 8], level: u32) -> u32 {
    medians
        .iter()
        .rposition(|m| *m <= level)
        .map_or(1, |i| u32::try_from(i + 1).unwrap_or(1))
}

/// The tier gear `gear` is rolled at, for a character whose level hunts at `level_tier` playing
/// where the creatures drop `place`: for its level, the higher of the two; better, one more; a
/// chosen tier as it is; none (no gear, or starter gear) 0.
#[must_use]
pub fn tier(gear: Gear, place: Option<u32>, level_tier: u32) -> u32 {
    let own = level_tier.max(place.unwrap_or(0)).clamp(1, 8);
    match gear {
        Gear::None | Gear::Starter => 0,
        Gear::Level => own,
        Gear::Better => (own + 1).min(8),
        Gear::Tier(t) => t.clamp(1, 8),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's gear)
    use super::*;

    #[test]
    fn rolling_one_slot_again_changes_its_seed_and_no_other_slots() {
        let seeds: Vec<u64> = Slot::ALL.iter().map(|s| s.seed(7, 0)).collect();
        let mut unique = seeds.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), Slot::ALL.len(), "each slot rolls on its own");
        assert_ne!(Slot::Chest.seed(7, 1), Slot::Chest.seed(7, 0));
        assert_eq!(Slot::Head.seed(7, 0), seeds[2], "another slot is as it was");
        assert_ne!(Slot::Chest.seed(8, 0), Slot::Chest.seed(7, 0));
        assert_ne!(suit_seed(7), suit_seed(8));
    }

    #[test]
    fn a_piece_is_worn_over_what_it_covers_a_ring_on_its_own_finger_a_weapon_where_it_is_wielded() {
        // Sleeves cover the upper and lower arms.
        assert_eq!(Slot::UpperArms.location(0x1800), Some(0x1800));
        assert_eq!(Slot::Chest.location(0x1800), None);
        // A shirt's under-slot is not armor.
        assert_eq!(Slot::Chest.location(0x0202), None);
        assert_eq!(Slot::RightFinger.location(0x000C_0000), Some(0x0008_0000));
        assert_eq!(Slot::Weapon.location(0x0200_0000), Some(0x0200_0000));
        assert_eq!(Slot::Weapon.location(0x0020_0000), None, "a shield");
        assert_eq!(Slot::Shield.location(0x0020_0000), Some(0x0020_0000));
        assert!(Slot::ARMOR.iter().all(|s| s.armor()));
        assert_eq!(
            Slot::ARMOR.iter().fold(0, |m, s| m | s.mask()),
            ARMOR_LOCATIONS
        );
    }

    #[test]
    fn a_tier_is_the_highest_whose_median_creature_the_level_reaches_and_the_place_can_raise_it() {
        let creatures = [(5, 1), (10, 1), (15, 1), (30, 2), (40, 2), (60, 3), (90, 4)];
        let m = level_medians(creatures);
        assert_eq!(&m[..4], &[10, 40, 60, 90]);
        assert_eq!(m[4], u32::MAX, "no creature drops tier 5");
        assert_eq!(tier_for_level(&m, 1), 1);
        assert_eq!(tier_for_level(&m, 45), 2);
        assert_eq!(tier_for_level(&m, 275), 4);
        assert_eq!(tier(Gear::Level, None, 2), 2);
        assert_eq!(tier(Gear::Level, Some(5), 2), 5, "the place drops more");
        assert_eq!(tier(Gear::Level, Some(1), 4), 4, "never under its level");
        assert_eq!(tier(Gear::Better, Some(5), 2), 6);
        assert_eq!(tier(Gear::Better, None, 8), 8);
        assert_eq!(tier(Gear::Tier(3), Some(7), 6), 3, "a chosen tier");
        assert_eq!(tier(Gear::Starter, Some(7), 6), 0);
        assert_eq!(tier(Gear::None, None, 6), 0);
    }
}
