//! The enchantments treasure items draw as they drop: for each sort of item, how many by the item's
//! level, and the families of effects, each with how often the January 2017 servers' items showed
//! it. The numbers are counts of those items; which tier a level takes in a family of five is
//! given by the five level bands.

/// When an enchantment applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// On the item itself, always.
    Permanent,
    /// On whoever wields or wears it.
    Equipped,
    /// On what it hits.
    Hit,
}

/// One effect of a family: the least level that reaches it, its saved state, what it puts on what
/// the item hits (0 for none) and how often, and the value of the glow it gives a weapon (0 for
/// none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tier {
    pub least: i32,
    pub state: u32,
    pub puts_on: u32,
    pub chance: f32,
    pub glow: f32,
}

/// A family of effects: when they apply, its share, whether the level takes its one tier (else
/// any it reaches), and its tiers from the lowest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Family {
    pub when: When,
    pub weight: f32,
    pub by_level: bool,
    pub tiers: &'static [Tier],
}

const fn tier(least: i32, state: u32, puts_on: u32, chance: f32, glow: f32) -> Tier {
    Tier {
        least,
        state,
        puts_on,
        chance,
        glow,
    }
}

/// How many of 2017's weapon items had none, one, two and three or more, by level: up to 9, 10 to 19,
/// 20 to 29, 30 to 39, 40 to 49, 50 and up.
pub const WEAPON_COUNTS: [[f32; 4]; 6] = [
    [102.0, 20.0, 0.0, 0.0],
    [150.0, 37.0, 0.0, 0.0],
    [152.0, 29.0, 7.0, 1.0],
    [135.0, 30.0, 12.0, 3.0],
    [188.0, 24.0, 18.0, 11.0],
    [246.0, 55.0, 39.0, 21.0],
];

/// How many of 2017's armor items had none, one, two and three or more, by level: up to 9, 10 to 19,
/// 20 to 29, 30 to 39, 40 to 49, 50 and up.
pub const ARMOR_COUNTS: [[f32; 4]; 6] = [
    [65.0, 12.0, 0.0, 0.0],
    [103.0, 17.0, 0.0, 0.0],
    [102.0, 20.0, 0.0, 0.0],
    [93.0, 18.0, 2.0, 0.0],
    [136.0, 60.0, 0.0, 0.0],
    [173.0, 89.0, 1.0, 0.0],
];

/// How many of 2017's shield items had none, one, two and three or more, by level: up to 9, 10 to 19,
/// 20 to 29, 30 to 39, 40 to 49, 50 and up.
pub const SHIELD_COUNTS: [[f32; 4]; 6] = [
    [15.0, 2.0, 0.0, 0.0],
    [32.0, 3.0, 0.0, 0.0],
    [29.0, 8.0, 0.0, 0.0],
    [26.0, 4.0, 0.0, 0.0],
    [43.0, 6.0, 0.0, 0.0],
    [56.0, 14.0, 0.0, 0.0],
];

/// The families a weapon item draws from.
pub const WEAPON_FAMILIES: &[Family] = &[
    Family {
        when: When::Hit,
        weight: 27.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F0024E5, 0x6F00038B, 0.10, 0.92),
            tier(16, 0x6F0024E1, 0x6F000389, 0.10, 0.92),
            tier(30, 0x6F0024E0, 0x6F000388, 0.10, 0.92),
            tier(40, 0x6F0024DF, 0x6F000387, 0.10, 0.92),
            tier(45, 0x6F0024E3, 0x6F00038A, 0.10, 0.92),
        ],
    },
    Family {
        when: When::Hit,
        weight: 29.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F0024E8, 0x6F00038C, 0.10, 0.88),
            tier(16, 0x6F0024F0, 0x6F000390, 0.10, 0.88),
            tier(30, 0x6F0024EE, 0x6F00038F, 0.10, 0.88),
            tier(40, 0x6F0024E9, 0x6F00038D, 0.10, 0.88),
            tier(45, 0x6F0024EA, 0x6F00038E, 0.10, 0.88),
        ],
    },
    Family {
        when: When::Hit,
        weight: 27.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F0024FE, 0x6F000395, 0.10, 0.70),
            tier(16, 0x6F0024F9, 0x6F000393, 0.10, 0.70),
            tier(30, 0x6F0024F3, 0x6F000391, 0.10, 0.70),
            tier(40, 0x6F0024F5, 0x6F000392, 0.10, 0.70),
            tier(45, 0x6F0024FD, 0x6F000394, 0.10, 0.70),
        ],
    },
    Family {
        when: When::Hit,
        weight: 25.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002502, 0x6F000397, 0.10, 0.80),
            tier(16, 0x6F002503, 0x6F000398, 0.10, 0.80),
            tier(30, 0x6F002505, 0x6F00039A, 0.10, 0.80),
            tier(40, 0x6F002501, 0x6F000396, 0.10, 0.85),
            tier(45, 0x6F002504, 0x6F000399, 0.10, 0.80),
        ],
    },
    Family {
        when: When::Hit,
        weight: 25.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002524, 0x6F00039E, 0.10, 0.84),
            tier(16, 0x6F002525, 0x6F00039F, 0.10, 0.84),
            tier(30, 0x6F002523, 0x6F00039D, 0.10, 0.84),
            tier(40, 0x6F002522, 0x6F00039C, 0.10, 0.84),
            tier(45, 0x6F002521, 0x6F00039B, 0.10, 0.84),
        ],
    },
    Family {
        when: When::Hit,
        weight: 20.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002526, 0x6F0003A0, 0.10, 0.00),
            tier(16, 0x6F002528, 0x6F0003A2, 0.10, 0.00),
            tier(30, 0x6F002529, 0x6F0003A3, 0.10, 0.00),
            tier(40, 0x6F00252A, 0x6F0003A4, 0.10, 0.00),
            tier(45, 0x6F002527, 0x6F0003A1, 0.10, 0.00),
        ],
    },
    Family {
        when: When::Hit,
        weight: 25.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F00252E, 0x6F0003A8, 0.10, 0.88),
            tier(16, 0x6F00252F, 0x6F0003A9, 0.10, 0.88),
            tier(30, 0x6F00252B, 0x6F0003A5, 0.10, 0.88),
            tier(40, 0x6F00252D, 0x6F0003A7, 0.10, 0.88),
            tier(45, 0x6F00252C, 0x6F0003A6, 0.10, 0.88),
        ],
    },
    Family {
        when: When::Hit,
        weight: 27.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002533, 0x6F0003AB, 0.10, 0.84),
            tier(16, 0x6F002537, 0x6F0003AE, 0.10, 0.84),
            tier(30, 0x6F002535, 0x6F0003AD, 0.10, 0.84),
            tier(40, 0x6F002534, 0x6F0003AC, 0.10, 0.84),
            tier(45, 0x6F002531, 0x6F0003AA, 0.10, 0.84),
        ],
    },
    Family {
        when: When::Hit,
        weight: 28.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002616, 0x6F0003DD, 0.10, 0.76),
            tier(16, 0x6F002614, 0x6F0003DB, 0.10, 0.76),
            tier(30, 0x6F002618, 0x6F0003DF, 0.10, 0.76),
            tier(40, 0x6F002617, 0x6F0003DE, 0.10, 0.76),
            tier(45, 0x6F002615, 0x6F0003DC, 0.10, 0.76),
        ],
    },
    Family {
        when: When::Hit,
        weight: 9.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002621, 0x6F0003E3, 0.10, 0.96),
            tier(16, 0x6F00261F, 0x6F0003E2, 0.10, 0.96),
            tier(30, 0x6F002624, 0x6F0003E4, 0.10, 0.96),
            tier(40, 0x6F00261E, 0x6F0003E1, 0.10, 0.96),
            tier(45, 0x6F00261B, 0x6F0003E0, 0.10, 0.96),
        ],
    },
    Family {
        when: When::Permanent,
        weight: 29.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F0003D2, 0x00000000, 1.00, 1.00),
            tier(16, 0x6F0003D5, 0x00000000, 1.00, 1.00),
            tier(30, 0x6F0003D3, 0x00000000, 1.00, 1.00),
            tier(40, 0x6F0003D4, 0x00000000, 1.00, 1.00),
            tier(45, 0x6F0003D1, 0x00000000, 1.00, 0.92),
        ],
    },
    Family {
        when: When::Permanent,
        weight: 26.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F0003DA, 0x00000000, 1.00, 0.92),
            tier(16, 0x6F0003D6, 0x00000000, 1.00, 0.92),
            tier(30, 0x6F0003D9, 0x00000000, 1.00, 0.92),
            tier(40, 0x6F0003D8, 0x00000000, 1.00, 0.92),
            tier(45, 0x6F0003D7, 0x00000000, 1.00, 0.92),
        ],
    },
    Family {
        when: When::Permanent,
        weight: 37.0,
        by_level: false,
        tiers: &[
            tier(18, 0x6F0024D7, 0x00000000, 1.00, 0.00),
            tier(3, 0x6F0024D6, 0x00000000, 1.00, 0.00),
            tier(30, 0x6F0024D5, 0x00000000, 1.00, 0.00),
        ],
    },
    Family {
        when: When::Permanent,
        weight: 11.0,
        by_level: false,
        tiers: &[tier(6, 0x6F00251D, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 12.0,
        by_level: false,
        tiers: &[tier(19, 0x6F0004B0, 0x00000000, 1.00, 0.00)],
    },
];

/// The families a armor item draws from.
pub const ARMOR_FAMILIES: &[Family] = &[Family {
    when: When::Permanent,
    weight: 182.0,
    by_level: false,
    tiers: &[
        tier(6, 0x6F0024C0, 0x00000000, 1.00, 0.00),
        tier(31, 0x6F0024C4, 0x00000000, 1.00, 0.00),
        tier(16, 0x6F0024C5, 0x00000000, 1.00, 0.00),
        tier(31, 0x6F0024C3, 0x00000000, 1.00, 0.00),
        tier(48, 0x6F0024C2, 0x00000000, 1.00, 0.00),
    ],
}];

/// The families a shield item draws from.
pub const SHIELD_FAMILIES: &[Family] = &[
    Family {
        when: When::Permanent,
        weight: 19.0,
        by_level: false,
        tiers: &[tier(9, 0x6F0024D9, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Permanent,
        weight: 9.0,
        by_level: false,
        tiers: &[tier(7, 0x6F00251E, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 8.0,
        by_level: false,
        tiers: &[tier(21, 0x6F0003E5, 0x00000000, 1.00, 0.00)],
    },
];

/// Every state a treasure item may carry or put on what it hits.
pub fn states() -> impl Iterator<Item = u32> {
    [WEAPON_FAMILIES, ARMOR_FAMILIES, SHIELD_FAMILIES]
        .into_iter()
        .flatten()
        .flat_map(|f| f.tiers)
        .flat_map(|t| [t.state, t.puts_on])
        .filter(|s| *s != 0)
}

/// The tier an item carrying this state has, among every family.
#[must_use]
pub fn tier_of(state: u32) -> Option<(When, Tier)> {
    [WEAPON_FAMILIES, ARMOR_FAMILIES, SHIELD_FAMILIES]
        .into_iter()
        .flatten()
        .find_map(|f| {
            f.tiers
                .iter()
                .find(|t| t.state == state)
                .map(|t| (f.when, *t))
        })
}
