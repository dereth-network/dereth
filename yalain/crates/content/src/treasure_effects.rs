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

/// How many of 2017's jewelry items first seen in a corpse had none and one, by the level of what
/// dropped them: up to 9, 10 to 19, 20 to 29, 30 to 39, 40 to 49, 50 and up. A plain piece has no
/// least level of its own.
pub const JEWELRY_COUNTS: [[f32; 4]; 6] = [
    [9.0, 1.0, 0.0, 0.0],
    [14.0, 2.0, 0.0, 0.0],
    [23.0, 5.0, 0.0, 0.0],
    [24.0, 1.0, 0.0, 0.0],
    [22.0, 4.0, 0.0, 0.0],
    [24.0, 11.0, 0.0, 0.0],
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
        weight: 32.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002616, 0x6F0003DD, 0.10, 0.76),
            tier(16, 0x6F0003DB, 0x6F0003DB, 0.10, 0.76),
            tier(30, 0x6F002618, 0x6F0003DF, 0.10, 0.76),
            tier(40, 0x6F002617, 0x6F0003DE, 0.10, 0.76),
            tier(45, 0x6F002615, 0x6F0003DC, 0.10, 0.76),
        ],
    },
    Family {
        when: When::Hit,
        weight: 20.0,
        by_level: true,
        tiers: &[
            tier(0, 0x6F002621, 0x6F0003E3, 0.10, 0.96),
            tier(16, 0x6F00261F, 0x6F0003E2, 0.10, 0.96),
            tier(30, 0x6F002624, 0x6F0003E4, 0.10, 0.96),
            tier(40, 0x6F00261E, 0x6F0003E1, 0.10, 0.96),
            tier(45, 0x6F0003E0, 0x6F0003E0, 0.10, 0.96),
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
    Family {
        when: When::Permanent,
        weight: 11.0,
        by_level: false,
        tiers: &[
            tier(6, 0x6F00057A, 0x00000000, 1.00, 0.00),
            tier(6, 0x6F00057B, 0x00000000, 1.00, 0.00),
        ],
    },
    Family {
        when: When::Equipped,
        weight: 10.0,
        by_level: false,
        tiers: &[
            tier(50, 0x6F002675, 0x00000000, 1.00, 0.00),
            tier(50, 0x6F002672, 0x00000000, 1.00, 0.00),
            tier(50, 0x6F002671, 0x00000000, 1.00, 0.00),
        ],
    },
    Family {
        when: When::Equipped,
        weight: 5.0,
        by_level: false,
        tiers: &[
            tier(50, 0x6F002646, 0x00000000, 1.00, 0.00),
            tier(50, 0x6F002648, 0x00000000, 1.00, 0.00),
        ],
    },
    Family {
        when: When::Hit,
        weight: 12.0,
        by_level: false,
        tiers: &[
            tier(50, 0x6F002620, 0x6F002620, 0.10, 0.00),
            tier(50, 0x6F0024FC, 0x6F0024FC, 0.10, 0.00),
            tier(50, 0x6F0024E2, 0x6F0024E2, 0.10, 0.00),
            tier(50, 0x6F00261C, 0x6F00261C, 0.10, 0.00),
            tier(50, 0x6F0024EB, 0x6F0024EB, 0.10, 0.00),
            tier(50, 0x6F002532, 0x6F002532, 0.10, 0.00),
            tier(50, 0x6F002530, 0x6F002530, 0.10, 0.00),
        ],
    },
    Family {
        when: When::Hit,
        weight: 4.0,
        by_level: false,
        tiers: &[tier(45, 0x6F000574, 0x6F002607, 0.10, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 7.0,
        by_level: false,
        tiers: &[
            tier(37, 0x6F0024C8, 0x00000000, 1.00, 0.00),
            tier(37, 0x6F0024CB, 0x00000000, 1.00, 0.00),
            tier(37, 0x6F0024CE, 0x00000000, 1.00, 0.00),
        ],
    },
    Family {
        when: When::Hit,
        weight: 3.0,
        by_level: false,
        tiers: &[tier(47, 0x6F002500, 0x6F002500, 0.10, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 0.0,
        by_level: false,
        tiers: &[tier(0, 0x6F00266E, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Permanent,
        weight: 0.0,
        by_level: false,
        tiers: &[tier(0, 0x6F0024F4, 0x00000000, 1.00, 0.70)],
    },
    Family {
        when: When::Hit,
        weight: 11.0,
        by_level: false,
        tiers: &[
            tier(17, 0x6F0003AF, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B0, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B1, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B2, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B3, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B4, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B5, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B6, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B7, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B8, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003B9, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BA, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BB, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BC, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BD, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BE, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003BF, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C0, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C1, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C2, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C3, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C4, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C5, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C6, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C7, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C8, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003C9, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003CA, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003CB, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003CC, 0x00000000, 1.00, 0.00),
            tier(17, 0x6F0003CD, 0x00000000, 1.00, 0.00),
        ],
    },
];

/// The families an armor item draws from.
pub const ARMOR_FAMILIES: &[Family] = &[
    Family {
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
    },
    Family {
        when: When::Equipped,
        weight: 32.0,
        by_level: false,
        tiers: &[
            tier(0, 0x6F0003E6, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003E7, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003E8, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003E9, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003EB, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003EC, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003ED, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003EE, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003EF, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F0, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F1, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F2, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F3, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F4, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F5, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F6, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F7, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F8, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003F9, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FA, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FB, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FC, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FD, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FE, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0003FF, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000400, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000401, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000402, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000403, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000404, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000405, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000406, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000407, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000408, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000409, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00040F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000410, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000411, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000412, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000413, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000414, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000415, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000416, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000417, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000418, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000419, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00041F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000420, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000421, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000422, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000423, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000424, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000425, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000426, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000427, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000428, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000429, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00042F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000430, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000431, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000432, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000433, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000434, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000435, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000436, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000437, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000438, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000439, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00043F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000440, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000441, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000442, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000443, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000444, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000445, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000446, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000447, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000448, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000449, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00044F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000450, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000451, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000452, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000453, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000454, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000455, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000456, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000457, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000458, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000459, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00045F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000460, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000461, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000462, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000463, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000464, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000465, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000466, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000467, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000468, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000469, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00046F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000470, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000471, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000472, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000473, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000474, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000475, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000476, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000477, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000478, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000479, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00047F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000480, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000481, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000482, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000483, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000484, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000485, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000486, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000487, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000488, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000489, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00048F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000490, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000491, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000492, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000493, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000494, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000495, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000496, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000497, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000498, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F000499, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00049F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A0, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A1, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A2, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A3, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A4, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A5, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A6, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A7, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A8, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004A9, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AA, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AB, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AC, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AD, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AE, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F0004AF, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00062A, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00062B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00062C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00062D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002755, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002756, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002757, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002758, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00275B, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00275C, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00275D, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00275E, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F00275F, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002760, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002887, 0x00000000, 1.00, 0.00),
            tier(0, 0x6F002888, 0x00000000, 1.00, 0.00),
        ],
    },
];

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

/// The families a jewelry item draws from.
pub const JEWELRY_FAMILIES: &[Family] = &[
    Family {
        when: When::Equipped,
        weight: 19.0,
        by_level: false,
        tiers: &[tier(11, 0x6F00054B, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 16.0,
        by_level: false,
        tiers: &[tier(6, 0x6F00054A, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 12.0,
        by_level: false,
        tiers: &[tier(11, 0x6F00054C, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 10.0,
        by_level: false,
        tiers: &[tier(8, 0x6F00054E, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 8.0,
        by_level: false,
        tiers: &[tier(18, 0x6F00054D, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 3.0,
        by_level: false,
        tiers: &[tier(43, 0x6F000554, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 1.0,
        by_level: false,
        tiers: &[tier(50, 0x6F000550, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 1.0,
        by_level: false,
        tiers: &[tier(42, 0x6F000552, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 1.0,
        by_level: false,
        tiers: &[tier(50, 0x6F000555, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 1.0,
        by_level: false,
        tiers: &[tier(50, 0x6F000556, 0x00000000, 1.00, 0.00)],
    },
    Family {
        when: When::Equipped,
        weight: 1.0,
        by_level: false,
        tiers: &[tier(44, 0x6F000664, 0x00000000, 1.00, 0.00)],
    },
];

/// An enchantment that comes with another, and the other: one draw, showing two lines.
pub const PAIRS: &[(u32, u32)] = &[
    (0x6F0024C8, 0x6F00266E),
    (0x6F0024CB, 0x6F00266E),
    (0x6F0024CE, 0x6F00266E),
    (0x6F002500, 0x6F0024F4),
];

/// Enchantments no item was named for.
pub const UNNAMING: &[u32] = &[0x6F0024C8, 0x6F0024CB, 0x6F0024CE, 0x6F00266E, 0x6F002500];

/// Every state a treasure item may carry or put on what it hits.
pub fn states() -> impl Iterator<Item = u32> {
    [
        WEAPON_FAMILIES,
        ARMOR_FAMILIES,
        SHIELD_FAMILIES,
        JEWELRY_FAMILIES,
    ]
    .into_iter()
    .flatten()
    .flat_map(|f| f.tiers)
    .flat_map(|t| [t.state, t.puts_on])
    .filter(|s| *s != 0)
}

/// The tier an item carrying this state has, among every family.
#[must_use]
pub fn tier_of(state: u32) -> Option<(When, Tier)> {
    [
        WEAPON_FAMILIES,
        ARMOR_FAMILIES,
        SHIELD_FAMILIES,
        JEWELRY_FAMILIES,
    ]
    .into_iter()
    .flatten()
    .find_map(|f| {
        f.tiers
            .iter()
            .find(|t| t.state == state)
            .map(|t| (f.when, *t))
    })
}
