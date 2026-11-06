//! Character generation's appearance choices: per race and sex, the physique types a new avatar
//! has, the values each can take, the defaults and the random pick, what the client sends of them,
//! and the look they give.
//!
//! The values of build and height step by 0.01 between a race and sex's bounds; every other type is
//! an appearance table's key, and its values are that key's change-set intensities (its base
//! tables' with the table's, in order). A choice is one of its type's values.
//! A new avatar's choices are picked at random, then its height and build are set to the middle of
//! their lists. The look is the body's description with a scale of build × height across and height
//! up, and each choice as its table's value in the whole avatar's changes; a garment's choice also
//! wears the garment (its key 16 at 1).

use std::collections::BTreeMap;

use auberean_primitives::{DataId, Vec3};

use crate::look::{
    appearance_key_steps, Desc, Group, Keyed, LookRecords, GLOBAL_KEY, TABLE_BUCKETS,
};

/// The physique types, in the order they are worked through.
pub const BUILD: u32 = 0x1;
pub const HEIGHT: u32 = 0x2;
pub const SKIN_TONE: u32 = 0x4;
pub const SKIN_DETAIL: u32 = 0x8;
pub const HEAD_DETAIL: u32 = 0x10;
pub const HEAD_FRILL: u32 = 0x20;
pub const FRILL_COLOR: u32 = 0x40;
pub const SPECIAL: u32 = 0x80;
pub const SHIRT: u32 = 0x100;
pub const PANTS: u32 = 0x200;
pub const BOOTS: u32 = 0x400;
pub const FACE_DETAIL: u32 = 0x800;
pub const EYE_COLOR: u32 = 0x1000;
pub const TYPES: [u32; 13] = [
    BUILD,
    HEIGHT,
    SKIN_TONE,
    SKIN_DETAIL,
    HEAD_DETAIL,
    HEAD_FRILL,
    FRILL_COLOR,
    SPECIAL,
    SHIRT,
    PANTS,
    BOOTS,
    FACE_DETAIL,
    EYE_COLOR,
];
/// Every type.
pub const ALL: u32 = 0xFFFF_FFFF;
/// The garments' types.
pub const CLOTHING: u32 = SHIRT | PANTS | BOOTS;
/// The appearance page's three pages: the types each shows.
pub const HEAD_PAGE: u32 = 0x1870;
pub const BODY_PAGE: u32 = 0x8F;
pub const CLOTHING_PAGE: u32 = 0x700;

/// The sexes.
pub const MALE: u32 = 0x1000;
pub const FEMALE: u32 = 0x2000;
/// The races.
pub const HUMAN: u32 = 1;
pub const LUGIAN: u32 = 2;
pub const TUMEROK: u32 = 4;
pub const DRUDGE: u32 = 8;
pub const EMPYREAN: u32 = 16;

/// The entity a player's character is made from.
pub const PLAYER_ENTITY: u32 = 0x4700_0530;
/// The entity an administrator's character is made from.
pub const ADMIN_ENTITY: u32 = 0x4700_0531;

/// The entity a new character is made from: the administrator's when the creation asked for it.
// D2-11915, D2-01310
#[must_use]
pub const fn avatar_entity(admin: bool) -> u32 {
    if admin {
        ADMIN_ENTITY
    } else {
        PLAYER_ENTITY
    }
}
/// The key of a garment's table that wears it.
pub const WORN: u32 = 0x10;

/// Each race's class, implement and start area.
pub const RACES: [(u32, u32, u32, u32); 5] = [
    (HUMAN, 2, 4, 0x4000_0001),
    (LUGIAN, 1, 2, 0x4000_0006),
    (TUMEROK, 4, 8, 0x4000_0005),
    (EMPYREAN, 8, 0x0010_0000, 0x4000_0007),
    (DRUDGE, 16, 0x8000, 0x4000_0009),
];

/// The body entity each race and sex is drawn from.
pub const BODIES: [(u32, u32, u32); 10] = [
    (HUMAN, MALE, 0x4700_00CD),
    (HUMAN, FEMALE, 0x4700_00CE),
    (LUGIAN, MALE, 0x4700_00CF),
    (LUGIAN, FEMALE, 0x4700_00D1),
    (TUMEROK, MALE, 0x4700_00D0),
    (TUMEROK, FEMALE, 0x4700_00D2),
    (DRUDGE, MALE, 0x4700_0514),
    (DRUDGE, FEMALE, 0x4700_0514),
    (EMPYREAN, MALE, 0x4700_0512),
    (EMPYREAN, FEMALE, 0x4700_0513),
];

/// Build's and height's bounds per race and sex: `(race, sex, build low, build high, height low,
/// height high)`.
pub const SIZES: [(u32, u32, f32, f32, f32, f32); 10] = [
    (HUMAN, MALE, 0.85, 1.15, 0.95, 1.1),
    (HUMAN, FEMALE, 0.85, 1.15, 1.0, 1.15),
    (LUGIAN, MALE, 1.0, 1.2, 1.0, 1.15),
    (LUGIAN, FEMALE, 0.8, 1.0, 1.0, 1.15),
    (TUMEROK, MALE, 1.0, 1.2, 1.05, 1.2),
    (TUMEROK, FEMALE, 0.8, 1.0, 0.9, 1.05),
    (DRUDGE, MALE, 0.9, 1.0, 0.9, 1.0),
    (DRUDGE, FEMALE, 0.8, 1.0, 0.8, 1.0),
    (EMPYREAN, MALE, 0.75, 1.0, 1.15, 1.4),
    (EMPYREAN, FEMALE, 0.75, 1.0, 1.10, 1.35),
];

/// The other types per race and sex: `(race, sex, type, label, key, table)`. A type listed twice
/// has its values twice over.
pub const CHOICES: [(u32, u32, u32, &str, u32, u32); 68] = [
    (HUMAN, MALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_000C),
    (HUMAN, MALE, HEAD_DETAIL, "ID_Hairstyle", 0x3, 0x2000_000D),
    (HUMAN, MALE, HEAD_FRILL, "ID_Beard", 0xC, 0x2000_000D),
    (HUMAN, MALE, FRILL_COLOR, "ID_HairColor", 0xB, 0x2000_000D),
    (HUMAN, MALE, SHIRT, "ID_ShirtColor", 0x2, 0x2000_0016),
    (HUMAN, MALE, PANTS, "ID_PantsColor", 0x2, 0x2000_0014),
    (HUMAN, MALE, BOOTS, "ID_BootsColor", 0x2, 0x2000_0019),
    (
        HUMAN,
        MALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_000E,
    ),
    (HUMAN, FEMALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_000F),
    (HUMAN, FEMALE, HEAD_DETAIL, "ID_Hairstyle", 0x3, 0x2000_0010),
    (HUMAN, FEMALE, FRILL_COLOR, "ID_HairColor", 0xB, 0x2000_0010),
    (HUMAN, FEMALE, SHIRT, "ID_ShirtColor", 0x2, 0x2000_0017),
    (HUMAN, FEMALE, PANTS, "ID_PantsColor", 0x2, 0x2000_0013),
    (HUMAN, FEMALE, BOOTS, "ID_BootsColor", 0x2, 0x2000_001A),
    (
        HUMAN,
        FEMALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_0011,
    ),
    (LUGIAN, MALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_0000),
    (LUGIAN, MALE, SKIN_DETAIL, "ID_SkinDetail", 0xF, 0x2000_0002),
    (LUGIAN, MALE, HEAD_DETAIL, "ID_Crest", 0x3, 0x2000_0001),
    (LUGIAN, MALE, HEAD_FRILL, "ID_Beard", 0xC, 0x2000_0001),
    (LUGIAN, MALE, FRILL_COLOR, "ID_CrestColor", 0xB, 0x2000_0001),
    (LUGIAN, MALE, SHIRT, "ID_SashColor", 0x2, 0x2000_00DD),
    (LUGIAN, MALE, PANTS, "ID_KiltColor", 0x2, 0x2000_00DE),
    (LUGIAN, MALE, BOOTS, "ID_SandalsColor", 0x2, 0x2000_001C),
    (
        LUGIAN,
        MALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_0002,
    ),
    (LUGIAN, FEMALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_0003),
    (
        LUGIAN,
        FEMALE,
        SKIN_DETAIL,
        "ID_SkinDetail",
        0xF,
        0x2000_0005,
    ),
    (LUGIAN, FEMALE, HEAD_DETAIL, "ID_Crest", 0x3, 0x2000_0004),
    (
        LUGIAN,
        FEMALE,
        FRILL_COLOR,
        "ID_CrestColor",
        0xB,
        0x2000_0004,
    ),
    (LUGIAN, FEMALE, SHIRT, "ID_SashColor", 0x2, 0x2000_00DF),
    (LUGIAN, FEMALE, PANTS, "ID_KiltColor", 0x2, 0x2000_00E0),
    (LUGIAN, FEMALE, BOOTS, "ID_SandalsColor", 0x2, 0x2000_0043),
    (
        LUGIAN,
        FEMALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_0005,
    ),
    (TUMEROK, MALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_0006),
    (TUMEROK, MALE, SKIN_DETAIL, "ID_Tatoo", 0xF, 0x2000_0008),
    (TUMEROK, MALE, HEAD_DETAIL, "ID_Hairstyle", 0x3, 0x2000_0007),
    (TUMEROK, MALE, HEAD_FRILL, "ID_Beard", 0xC, 0x2000_0007),
    (TUMEROK, MALE, FRILL_COLOR, "ID_HairColor", 0xB, 0x2000_0007),
    (TUMEROK, MALE, SHIRT, "ID_HarnessColor", 0x2, 0x2000_0038),
    (TUMEROK, MALE, PANTS, "ID_LeggingsColor", 0x2, 0x2000_0039),
    (
        TUMEROK,
        MALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_0008,
    ),
    (TUMEROK, FEMALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_0009),
    (TUMEROK, FEMALE, SKIN_DETAIL, "ID_Tatoo", 0xF, 0x2000_000B),
    (
        TUMEROK,
        FEMALE,
        HEAD_DETAIL,
        "ID_Hairstyle",
        0x3,
        0x2000_000A,
    ),
    (
        TUMEROK,
        FEMALE,
        FRILL_COLOR,
        "ID_HairColor",
        0xB,
        0x2000_000A,
    ),
    (
        TUMEROK,
        FEMALE,
        SPECIAL,
        "ID_Tailspike",
        0x4100_0003,
        0x2000_000A,
    ),
    (TUMEROK, FEMALE, SHIRT, "ID_HarnessColor", 0x2, 0x2000_0038),
    (TUMEROK, FEMALE, PANTS, "ID_LeggingsColor", 0x2, 0x2000_0039),
    (
        TUMEROK,
        FEMALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_000B,
    ),
    (DRUDGE, MALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_00ED),
    (DRUDGE, FEMALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_00ED),
    (EMPYREAN, MALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_00E1),
    (
        EMPYREAN,
        MALE,
        HEAD_DETAIL,
        "ID_Hairstyle",
        0x3,
        0x2000_00E4,
    ),
    (EMPYREAN, MALE, HEAD_FRILL, "ID_Beard", 0xC, 0x2000_00E4),
    (
        EMPYREAN,
        MALE,
        FRILL_COLOR,
        "ID_HairColor",
        0xB,
        0x2000_00E4,
    ),
    (EMPYREAN, MALE, SHIRT, "ID_ShirtColor", 0x2, 0x2000_00E6),
    (EMPYREAN, MALE, PANTS, "ID_PantsColor", 0x2, 0x2000_00E9),
    (EMPYREAN, MALE, BOOTS, "ID_BootsColor", 0x2, 0x2000_00E7),
    (
        EMPYREAN,
        MALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_00E1,
    ),
    (
        EMPYREAN,
        MALE,
        EYE_COLOR,
        "ID_EyeColor",
        0x4100_002E,
        0x2000_00E3,
    ),
    (EMPYREAN, FEMALE, SKIN_TONE, "ID_SkinTone", 0x1, 0x2000_00E2),
    (
        EMPYREAN,
        FEMALE,
        HEAD_DETAIL,
        "ID_Hairstyle",
        0x3,
        0x2000_00E5,
    ),
    (
        EMPYREAN,
        FEMALE,
        FRILL_COLOR,
        "ID_HairColor",
        0xB,
        0x2000_00E5,
    ),
    (EMPYREAN, FEMALE, SHIRT, "ID_ShirtColor", 0x2, 0x2000_00EA),
    (EMPYREAN, FEMALE, PANTS, "ID_PantsColor", 0x2, 0x2000_00EC),
    (EMPYREAN, FEMALE, BOOTS, "ID_BootsColor", 0x2, 0x2000_00EB),
    (EMPYREAN, FEMALE, BOOTS, "ID_BootsColor", 0x2, 0x2000_00EB),
    (
        EMPYREAN,
        FEMALE,
        FACE_DETAIL,
        "ID_Face",
        0x4100_001C,
        0x2000_00E2,
    ),
    (
        EMPYREAN,
        FEMALE,
        EYE_COLOR,
        "ID_EyeColor",
        0x4100_002E,
        0x2000_00E3,
    ),
];

/// One physique type of a race and sex: its label, the appearance table and key it sets (none for
/// build and height), and its values.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub physique_type: u32,
    pub label: &'static str,
    pub key: u32,
    pub table: u32,
    pub values: Vec<f32>,
}

/// Values from `low` to `high` by 0.01, added up as the data files' tables were.
#[must_use]
pub fn by_steps(low: f32, high: f32) -> Vec<f32> {
    let mut out = Vec::new();
    let mut v = low;
    while v <= high && out.len() < 1000 {
        out.push(v);
        v += 0.01_f32;
    }
    out
}

/// A table key's values: the strength steps the key has in the table and its base tables, in
/// order; none for a table not in the data or listing the key with no change sets.
#[must_use]
pub fn key_values(records: &dyn LookRecords, table: u32, key: u32) -> Vec<f32> {
    appearance_key_steps(records, DataId(table), key).unwrap_or_default()
}

/// Every physique type a race and sex has, in type order: each with its label and the values it
/// may take.
// D2-11909, D2-11911
#[must_use]
// D2-01317, D2-01291
pub fn choices(records: &dyn LookRecords, race: u32, sex: u32) -> Vec<Choice> {
    let mut out = Vec::new();
    if let Some(&(_, _, bl, bh, hl, hh)) = SIZES.iter().find(|s| s.0 == race && s.1 == sex) {
        out.push(Choice {
            physique_type: BUILD,
            label: "ID_Build",
            key: 0,
            table: 0,
            values: by_steps(bl, bh),
        });
        out.push(Choice {
            physique_type: HEIGHT,
            label: "ID_Height",
            key: 0,
            table: 0,
            values: by_steps(hl, hh),
        });
    }
    for &(r, s, t, label, key, table) in &CHOICES {
        if r != race || s != sex {
            continue;
        }
        let values = key_values(records, table, key);
        match out.iter_mut().find(|c| c.physique_type == t) {
            Some(c) => c.values.extend(values),
            None => out.push(Choice {
                physique_type: t,
                label,
                key,
                table,
                values,
            }),
        }
    }
    out.sort_by_key(|c| c.physique_type);
    out
}

/// The middle of a list: where height and build start.
#[must_use]
pub fn middle(n: usize) -> usize {
    n.div_ceil(2).saturating_sub(1)
}

/// The random numbers the choices are picked with: the client's one generator, which everything
/// else in the program draws from too.
pub use auberean_primitives::random::Random;

/// One avatar's choices: its race and sex, and a value per physique type. Dropped, its values go.
// D2-11927
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub race: u32,
    pub sex: u32,
    pub values: BTreeMap<u32, f32>,
}

impl Avatar {
    /// A new avatar of a race and sex: every type picked at random, then height and build at the
    /// middle of their lists.
    // D2-11934, D2-01321
    #[must_use]
    pub fn new(choices: &[Choice], race: u32, sex: u32, random: &mut Random) -> Self {
        let mut a = Self {
            race,
            sex,
            values: BTreeMap::new(),
        };
        a.randomize(choices, ALL, random);
        for t in [HEIGHT, BUILD] {
            if let Some(c) = choices.iter().find(|c| c.physique_type == t) {
                if let Some(v) = c.values.get(middle(c.values.len())) {
                    a.values.insert(t, *v);
                }
            }
        }
        a
    }

    /// Pick every type of `mask` at random from its values (a type the avatar does not have is
    /// 0).
    // D2-11912, D2-01320
    pub fn randomize(&mut self, choices: &[Choice], mask: u32, random: &mut Random) {
        for t in TYPES {
            if mask & t != t {
                continue;
            }
            let mut v = 0.0;
            if let Some(c) = choices.iter().find(|c| c.physique_type == t) {
                if !c.values.is_empty() {
                    let n = i32::try_from(c.values.len()).unwrap_or(i32::MAX);
                    let k = usize::try_from(random.roll_int(0, n - 1)).unwrap_or(0);
                    v = c.values.get(k).copied().unwrap_or(0.0);
                }
            }
            self.values.insert(t, v);
        }
    }

    /// A type's value (none for a type the avatar does not have).
    // D2-11922
    #[must_use]
    pub fn value(&self, physique_type: u32) -> Option<f32> {
        self.values.get(&physique_type).copied()
    }

    /// The physique table the client sends: every type in order, 0 for one the avatar lacks.
    #[must_use]
    pub fn physique(&self) -> Vec<(u32, f32)> {
        TYPES
            .iter()
            .map(|t| (*t, self.value(*t).unwrap_or(0.0)))
            .collect()
    }

    /// The race's class, implement and start area.
    #[must_use]
    pub fn class_implement_start(&self) -> Option<(u32, u32, u32)> {
        RACES
            .iter()
            .find(|r| r.0 == self.race)
            .map(|r| (r.1, r.2, r.3))
    }

    /// The look the choices give, over `body` (the race and sex's body description): the scale,
    /// then each of the avatar's types in order as its table's value in the whole avatar's
    /// changes; a garment's type wears the garment first.
    // D2-11914, D2-13841, D2-01311
    #[must_use]
    pub fn look(&self, choices: &[Choice], body: DataId) -> Desc {
        let b = self.value(BUILD).unwrap_or(1.0);
        let h = self.value(HEIGHT).unwrap_or(1.0);
        let mut global = Group {
            key: GLOBAL_KEY,
            ..Group::default()
        };
        for t in TYPES.iter().filter(|t| **t > HEIGHT) {
            let Some(c) = choices.iter().find(|c| c.physique_type == *t) else {
                continue;
            };
            let Some(v) = self.value(*t) else { continue };
            if t & CLOTHING == *t {
                global
                    .augment_appearance(c.table, &Keyed::stored(TABLE_BUCKETS, vec![(WORN, 1.0)]));
            }
            global.augment_appearance(c.table, &Keyed::stored(TABLE_BUCKETS, vec![(c.key, v)]));
        }
        Desc {
            parent: body,
            scale: Vec3::new(b * h, b * h, h),
            child_scale: 1.0,
            particle_scale: 1.0,
            global,
            ..Desc::default()
        }
    }
}

/// The body entity of a race and sex.
// D2-11913
#[must_use]
pub fn body_entity(race: u32, sex: u32) -> Option<u32> {
    BODIES
        .iter()
        .find(|b| b.0 == race && b.1 == sex)
        .map(|b| b.2)
}

/// The slider stop a value stands at: the first value equal to it.
// D2-01870
#[must_use]
pub fn stop_of(values: &[f32], v: f32) -> Option<usize> {
    #[allow(clippy::float_cmp)]
    values.iter().position(|x| *x == v)
}

/// The value at a slider position (0 to 1) over `n` stops.
// D2-01869
#[must_use]
pub fn stop_at(position: f32, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    #[allow(clippy::cast_precision_loss)]
    let k = ((n - 1) as f32 * position + 2.5e-5_f32).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let k = k as usize;
    k.min(n - 1)
}
