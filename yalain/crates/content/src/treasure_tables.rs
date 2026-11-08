//! The numbers treasure draws by, held in one value: a treasure item's own least level and Arcane
//! Lore, a potion's effect, an item's look, numbers, word and enchantments. The formulas that draw
//! with them are the world's; [`TreasureTables::retail_2017`] holds the numbers fit to the January
//! 2017 servers' items, which a world plays by unless it gives its own.
//!
//! Every list a draw picks from by weight keeps its order: the draws go by it.

use crate::treasure_effects::{self as effects, Tier, When};

/// The sorts of item that draw numbers and enchantments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sort {
    Weapon,
    Armor,
    Shield,
    Jewelry,
}

impl Sort {
    /// Every sort, in the order the tables hold them.
    pub const ALL: [Self; 4] = [Self::Weapon, Self::Armor, Self::Shield, Self::Jewelry];

    /// Its name, as the world's rows give it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Armor => "armor",
            Self::Shield => "shield",
            Self::Jewelry => "jewelry",
        }
    }

    /// The sort a row names.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// The highest least level an item's own may be.
pub const MOST_LEVEL: i32 = 50;

/// How many of the 2017 items' least levels stood 0, 1, 2 and 3 above the base.
pub const ABOVE_BASE: [f32; 4] = [436.0, 519.0, 220.0, 39.0];

/// The treasure potions' effects: nine kinds, each with its five tiers from the lowest.
pub const POTION_EFFECTS: [[u32; 5]; 9] = [
    // Attack speed: haste, coordination, dexterity, nimbleness, exactitude.
    [
        0x7000_09F7,
        0x7000_09F4,
        0x7000_09F5,
        0x7000_09F8,
        0x7000_09F6,
    ],
    // Damage reflected: thorns, spines, feedback, reflecting, refraction.
    [
        0x7000_09FD,
        0x7000_09FC,
        0x7000_09F9,
        0x7000_09FA,
        0x7000_09FB,
    ],
    // Health over time: recovery, convalescence, reconstitution, regeneration, revival.
    [
        0x7000_0A00,
        0x7000_09FE,
        0x7000_09FF,
        0x7000_0A01,
        0x7000_0A02,
    ],
    // Health regained: rejuvenation, fortitude, constitution, mending, stability.
    [
        0x7000_0A06,
        0x7000_0A04,
        0x7000_0A03,
        0x7000_0A05,
        0x7000_0A07,
    ],
    // Greatest health: toughness, immovability, tenacity, obstinance, perseverance.
    [
        0x7000_0A0C,
        0x7000_0A08,
        0x7000_0A0B,
        0x7000_0A09,
        0x7000_0A0A,
    ],
    // Greatest vigor: resolve, contemplation, clarity, animus, unconquerable will.
    [
        0x7000_0A10,
        0x7000_0A0F,
        0x7000_0A0E,
        0x7000_0A0D,
        0x7000_0A11,
    ],
    // Run speed: rapidity, speed, swiftness, velocity, acceleration.
    [
        0x7000_0A13,
        0x7000_0A14,
        0x7000_0A15,
        0x7000_0A16,
        0x7000_0A12,
    ],
    // Vigor over time: devotion, ardor, zeal, diligence, vigilance.
    [
        0x7000_0A18,
        0x7000_0A17,
        0x7000_0A1B,
        0x7000_0A19,
        0x7000_0A1A,
    ],
    // Vigor regained: inner calm, focus, renewal, restoration, invigoration.
    [
        0x7000_0A1C,
        0x7000_0A20,
        0x7000_0A1E,
        0x7000_0A1F,
        0x7000_0A1D,
    ],
];

/// The names of the treasure potions' nine kinds, as [`POTION_EFFECTS`] holds them: the keys of a
/// world's potion rows.
pub const POTION_KINDS: [&str; 9] = [
    "attack-speed",
    "damage-reflected",
    "health-over-time",
    "health-regained",
    "greatest-health",
    "greatest-vigor",
    "run-speed",
    "vigor-over-time",
    "vigor-regained",
];

/// The least dropper level of each tier of a potion's effect: up to 15, 16 to 29, 30 to 39, 40 to
/// 44, 45 and up.
pub const POTION_TIERS: [i32; 5] = [0, 16, 30, 40, 45];

/// A treasure potion's spellcraft, drawn evenly between these whatever the level.
pub const POTION_SPELLCRAFT: (f32, f32) = (0.0, 100.0);

/// A treasure weapon's damage, of a made weapon's at its level, at least and at most (2017: 689 of
/// 727 weapons below level 50 with no effects at 0.84 to 0.97, the median 0.90). A shield's too.
pub const WEAPON_DAMAGE_SHARE: (f32, f32) = (0.84, 0.97);

/// How many of the 2017 treasure weapons had each combat delay, from 4; armor's, from 4; shields',
/// from 5.
pub const WEAPON_DELAYS: [f32; 16] = [
    2.0, 17.0, 29.0, 41.0, 60.0, 58.0, 107.0, 113.0, 116.0, 108.0, 100.0, 86.0, 40.0, 44.0, 33.0,
    19.0,
];
pub const ARMOR_DELAYS: [f32; 18] = [
    1.0, 3.0, 5.0, 7.0, 5.0, 10.0, 13.0, 16.0, 13.0, 10.0, 8.0, 11.0, 4.0, 7.0, 5.0, 7.0, 2.0, 1.0,
];
pub const SHIELD_DELAYS: [f32; 7] = [14.0, 33.0, 40.0, 42.0, 31.0, 29.0, 12.0];

/// A treasure weapon's vigor cost by level: from each level, at least and at most (2017, of 973
/// weapons: 1 to level 7, 3 at 8 and 9, 4 or 5 to 20, 6 or 7 to 29, 10 to 12 to 39, 14 to 17 to
/// 49, 16 to 19 at 50); above that it follows a made weapon's, one more for each five levels.
pub const WEAPON_VIGOR: [VigorBand; 7] = [
    VigorBand::new(0, 1, 1),
    VigorBand::new(8, 3, 3),
    VigorBand::new(10, 4, 5),
    VigorBand::new(21, 6, 7),
    VigorBand::new(30, 10, 12),
    VigorBand::new(40, 14, 17),
    VigorBand {
        every: Some(5),
        ..VigorBand::new(50, 16, 19)
    },
];

/// A treasure shield's vigor cost by level: from each level, at least and at most (2017, of 177
/// dropped shields examined with no effects, 170 within: 1 to level 7, 2 or 3 to 16, 4 or 5 to 27,
/// 5 to 7 to 39, 6 to 9 from 40, none above 9 to level 60).
pub const SHIELD_VIGOR: [VigorBand; 5] = [
    VigorBand::new(0, 1, 1),
    VigorBand::new(8, 2, 3),
    VigorBand::new(17, 4, 5),
    VigorBand::new(28, 5, 7),
    VigorBand::new(40, 6, 9),
];

/// A treasure piece of armor's rating, of a made piece's at its level, at each of these levels and
/// in proportion between them: it comes nearer with the level (2017: 0.8 at 10 to 1.0 at 49, of
/// 130 pieces with no effects). A shield's too.
pub const RATING_SHARE: [(i32, f32); 2] = [(0, 0.75), (MOST_LEVEL, 1.0)];

/// How far a treasure item's rating share is drawn either side of its level's: a tenth.
pub const RATING_SPREAD: f32 = 0.1;

/// How often a treasure piece of armor or a shield has an enchantment slot; a robe has three, a
/// weapon one (2017: 46 of 109 armor pieces but robes, 114 of 201 shields; 21 of 21 robes; 1,264 of
/// 1,334 weapons).
pub const ARMOR_SLOT_CHANCE: f32 = 0.5;

/// A treasure shield's span of damage when its template names none, at least and at most, drawn
/// evenly (2017: 207 of 207 examined dropped shields' damage lines, none of whose templates names
/// one, at 0.25 to 0.29 below their greatest).
pub const SHIELD_SPAN: (f32, f32) = (0.25, 0.29);

/// The most a weapon's glow draws (2017: 920 weapons with no effects at 0.03 to 0.61 from the 5th to
/// the 95th in a hundred, the median 0.31, none above 0.65; it follows no level, Spearman 0.03).
pub const GLOW_DRAW: f32 = 0.65;

/// How often a color is drawn from the round tenths, which 2017's colors show about twice as often
/// as the rest (1,585 of key 2).
pub const ROUND_COLOR_CHANCE: f32 = 0.1;

/// The least level of each band the counts of an item's enchantments, and its words, go by.
pub const COUNT_BANDS: [i32; 6] = [0, 10, 20, 30, 40, 50];

/// The Arcane Lore a treasure item may take: drawn only when its dropper's base is above `above`,
/// at an item level drawn evenly from the base plus `from` to the base plus `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoreRule {
    pub above: i32,
    pub from: i32,
    pub to: i32,
}

/// How a treasure item's look is drawn: a weapon's glow evenly up to `glow_most`; a color, by
/// `round_chance` one of the first `round_tenths` round tenths when its key's highest is
/// `round_from` or more, else in hundredths up to its key's highest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookRule {
    pub glow_most: f32,
    pub round_chance: f32,
    pub round_from: f32,
    pub round_tenths: u32,
}

/// What every treasure item draws by, whatever its sort.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    /// How often an item's own least level stands 0, 1, 2 ... above its dropper's base.
    pub above_base: Vec<f32>,
    /// The highest least level an item's own may be.
    pub most_level: i32,
    pub lore: LoreRule,
    /// The least dropper level of each tier of a potion's effect, from 0.
    pub potion_tiers: Vec<i32>,
    /// A potion's spellcraft, drawn evenly between these.
    pub potion_spellcraft: (f32, f32),
    pub look: LookRule,
}

impl Rules {
    /// The tier (0 the lowest) of a treasure potion dropped by a creature of this level.
    #[must_use]
    pub fn potion_tier(&self, level: i32) -> usize {
        band_of(self.potion_tiers.iter().copied(), level)
    }
}

/// A band of an item's level: from its least level, how often an item had none, one, two, ...
/// enchantments.
#[derive(Debug, Clone, PartialEq)]
pub struct CountBand {
    pub from: i32,
    pub weights: Vec<f32>,
}

/// A band of an item's level: from its least level, its vigor cost at least and at most, both one
/// more for each `every` levels above `from` when it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VigorBand {
    pub from: i32,
    pub least: i32,
    pub most: i32,
    pub every: Option<i32>,
}

impl VigorBand {
    #[must_use]
    pub const fn new(from: i32, least: i32, most: i32) -> Self {
        Self {
            from,
            least,
            most,
            every: None,
        }
    }
}

/// How often an item had each combat delay, from the least.
#[derive(Debug, Clone, PartialEq)]
pub struct Delays {
    pub from: i32,
    pub weights: Vec<f32>,
}

/// A family of enchantments: when they apply, its share, whether the level takes its one tier (else
/// any it reaches), and its tiers from the lowest.
#[derive(Debug, Clone, PartialEq)]
pub struct Family {
    pub when: When,
    pub weight: f32,
    pub by_level: bool,
    pub tiers: Vec<Tier>,
}

/// A word before a treasure item's name: its string, the combat delays it goes with, and how often
/// it is drawn in each count band.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub word: u32,
    pub delays: (i32, i32),
    pub weights: Vec<f32>,
}

/// The look a template's treasure items draw: its appearance table, and each key with the highest
/// value it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct LookRow {
    pub template: u32,
    pub table: u32,
    pub keys: Vec<(u32, f32)>,
}

/// What one sort of treasure item draws by. A field its sort has no use for is empty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SortTables {
    /// How many enchantments, by level band; the word weights follow the same bands.
    pub counts: Vec<CountBand>,
    /// Its damage, of a made weapon's at its level, at least and at most.
    pub damage_share: Option<(f32, f32)>,
    /// Its vigor cost by level.
    pub vigor: Vec<VigorBand>,
    /// A shield's span of damage when its template names none.
    pub span: Option<(f32, f32)>,
    /// How often it has each combat delay.
    pub delays: Option<Delays>,
    /// Its rating, of a made one's, at levels and in proportion between them, the ends held.
    pub rating_share: Vec<(i32, f32)>,
    /// How far its rating share is drawn either side of its level's.
    pub rating_spread: f32,
    /// How often it has an enchantment slot.
    pub slot_chance: f32,
    /// The families of enchantments it draws from, in order.
    pub families: Vec<Family>,
    /// The words before its name, in order.
    pub words: Vec<Word>,
}

impl SortTables {
    /// The count band of an item level.
    #[must_use]
    pub fn band(&self, level: i32) -> usize {
        band_of(self.counts.iter().map(|b| b.from), level)
    }

    /// Its vigor cost at a level: at least and at most.
    #[must_use]
    pub fn vigor_at(&self, level: i32) -> (i32, i32) {
        let Some(b) = self
            .vigor
            .get(band_of(self.vigor.iter().map(|b| b.from), level))
        else {
            return (1, 1);
        };
        let above = b
            .every
            .filter(|e| *e > 0)
            .map_or(0, |e| (level - b.from).max(0) / e);
        (b.least + above, b.most + above)
    }

    /// Its rating share at a level.
    #[must_use]
    pub fn rating_share_at(&self, level: i32) -> f32 {
        let points = &self.rating_share;
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return 1.0;
        };
        if level <= first.0 {
            return first.1;
        }
        if level >= last.0 {
            return last.1;
        }
        let i = points.iter().rposition(|p| p.0 <= level).unwrap_or(0);
        let (l0, s0) = points[i];
        let (l1, s1) = points[i + 1];
        #[allow(clippy::cast_precision_loss)]
        let (l, l0, l1) = (level as f32, l0 as f32, l1 as f32);
        s0 + (s1 - s0) * (l - l0) / (l1 - l0)
    }
}

/// The index of the last band whose least level an item level reaches (the first when it reaches
/// none).
fn band_of(froms: impl Iterator<Item = i32>, level: i32) -> usize {
    froms
        .enumerate()
        .filter(|(_, from)| *from <= level)
        .last()
        .map_or(0, |(i, _)| i)
}

/// What a world's rows give of the treasure numbers: its rules, and each sort's numbers (no
/// families or words), by sort. A kind the world gives no rows of is none or empty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureRows {
    pub rules: Option<Rules>,
    pub sorts: std::collections::BTreeMap<Sort, SortTables>,
    /// Each sort's words, by their order.
    pub words: std::collections::BTreeMap<Sort, std::collections::BTreeMap<u32, Word>>,
    pub looks: Vec<LookRow>,
    /// Each potion kind's states, one a tier, by the kind's order.
    pub potions: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Each sort's families of enchantments, by their order.
    pub families: std::collections::BTreeMap<Sort, std::collections::BTreeMap<u32, Family>>,
    /// The families' pairs (a tier's state, and the state that comes with it) and the states no
    /// item is named for, from their tiers.
    pub pairs: Vec<(u32, u32)>,
    pub unnaming: Vec<u32>,
}

/// A list's entries in their order: what is looked up in it, not drawn from it.
fn sorted<T: Ord + Copy>(list: &[T]) -> Vec<T> {
    let mut v = list.to_vec();
    v.sort_unstable();
    v
}

/// Every number treasure draws by.
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureTables {
    pub rules: Rules,
    pub weapon: SortTables,
    pub armor: SortTables,
    pub shield: SortTables,
    pub jewelry: SortTables,
    /// The templates whose items draw a look.
    pub looks: Vec<LookRow>,
    /// The potions' effects: each kind's saved state at each tier, in order.
    pub potions: Vec<Vec<u32>>,
    /// An enchantment that comes with another, and the other: one draw, showing two lines.
    pub pairs: Vec<(u32, u32)>,
    /// Enchantments no item is named for.
    pub unnaming: Vec<u32>,
}

impl Default for TreasureTables {
    fn default() -> Self {
        Self::retail_2017()
    }
}

impl TreasureTables {
    /// The numbers fit to the items the January 2017 servers made.
    #[must_use]
    pub fn retail_2017() -> Self {
        let counts = |c: &[[f32; 4]; 6]| -> Vec<CountBand> {
            COUNT_BANDS
                .iter()
                .zip(c)
                .map(|(from, w)| CountBand {
                    from: *from,
                    weights: w.to_vec(),
                })
                .collect()
        };
        let families = |f: &[effects::Family]| -> Vec<Family> {
            f.iter()
                .map(|f| Family {
                    when: f.when,
                    weight: f.weight,
                    by_level: f.by_level,
                    tiers: f.tiers.to_vec(),
                })
                .collect()
        };
        let words = |w: &[crate::treasure_words::Word]| -> Vec<Word> {
            w.iter()
                .map(|w| Word {
                    word: w.word,
                    delays: w.delays,
                    weights: w.by_level.to_vec(),
                })
                .collect()
        };
        let delays = |from: i32, w: &[f32]| {
            Some(Delays {
                from,
                weights: w.to_vec(),
            })
        };
        Self {
            rules: Rules {
                above_base: ABOVE_BASE.to_vec(),
                most_level: MOST_LEVEL,
                lore: LoreRule {
                    above: 51,
                    from: -6,
                    to: -1,
                },
                potion_tiers: POTION_TIERS.to_vec(),
                potion_spellcraft: POTION_SPELLCRAFT,
                look: LookRule {
                    glow_most: GLOW_DRAW,
                    round_chance: ROUND_COLOR_CHANCE,
                    round_from: 0.1,
                    round_tenths: 4,
                },
            },
            weapon: SortTables {
                counts: counts(&effects::WEAPON_COUNTS),
                damage_share: Some(WEAPON_DAMAGE_SHARE),
                vigor: WEAPON_VIGOR.to_vec(),
                delays: delays(4, &WEAPON_DELAYS),
                families: families(effects::WEAPON_FAMILIES),
                words: words(crate::treasure_words::WEAPON_WORDS),
                ..SortTables::default()
            },
            armor: SortTables {
                counts: counts(&effects::ARMOR_COUNTS),
                delays: delays(4, &ARMOR_DELAYS),
                rating_share: RATING_SHARE.to_vec(),
                rating_spread: RATING_SPREAD,
                slot_chance: ARMOR_SLOT_CHANCE,
                families: families(effects::ARMOR_FAMILIES),
                words: words(crate::treasure_words::ARMOR_WORDS),
                ..SortTables::default()
            },
            shield: SortTables {
                counts: counts(&effects::SHIELD_COUNTS),
                damage_share: Some(WEAPON_DAMAGE_SHARE),
                vigor: SHIELD_VIGOR.to_vec(),
                span: Some(SHIELD_SPAN),
                delays: delays(5, &SHIELD_DELAYS),
                rating_share: RATING_SHARE.to_vec(),
                rating_spread: RATING_SPREAD,
                slot_chance: ARMOR_SLOT_CHANCE,
                families: families(effects::SHIELD_FAMILIES),
                words: words(crate::treasure_words::SHIELD_WORDS),
            },
            jewelry: SortTables {
                counts: counts(&effects::JEWELRY_COUNTS),
                families: families(effects::JEWELRY_FAMILIES),
                ..SortTables::default()
            },
            looks: crate::treasure_looks::TREASURE_LOOKS
                .iter()
                .map(|(template, table, keys)| LookRow {
                    template: *template,
                    table: *table,
                    keys: keys.to_vec(),
                })
                .collect(),
            potions: POTION_EFFECTS.iter().map(|k| k.to_vec()).collect(),
            pairs: sorted(effects::PAIRS),
            unnaming: sorted(effects::UNNAMING),
        }
    }

    /// What one sort draws by.
    #[must_use]
    pub const fn sort(&self, sort: Sort) -> &SortTables {
        match sort {
            Sort::Weapon => &self.weapon,
            Sort::Armor => &self.armor,
            Sort::Shield => &self.shield,
            Sort::Jewelry => &self.jewelry,
        }
    }

    fn sort_mut(&mut self, sort: Sort) -> &mut SortTables {
        match sort {
            Sort::Weapon => &mut self.weapon,
            Sort::Armor => &mut self.armor,
            Sort::Shield => &mut self.shield,
            Sort::Jewelry => &mut self.jewelry,
        }
    }

    /// 2017's tables with a world's rows in place of each kind it gives: its rules, its sorts'
    /// numbers, its words, looks, potions and families of enchantments (with their pairs and the
    /// states nothing is named for). A kind is taken whole
    /// or not at all, never merged with 2017's; once a world gives any sort, a sort it leaves out
    /// draws no numbers or enchantments of its own, and once it gives any words or families, a
    /// sort it gives none of draws none. Also returns the kinds the world gave none of, which play 2017's.
    #[must_use]
    pub fn with_rows(rows: &TreasureRows) -> (Self, Vec<&'static str>) {
        let mut t = Self::retail_2017();
        let mut fell_back = Vec::new();
        match &rows.rules {
            Some(rules) => t.rules = rules.clone(),
            None => fell_back.push("treasure_rules"),
        }
        if rows.sorts.is_empty() {
            fell_back.push("treasure_sort");
        } else {
            for sort in Sort::ALL {
                let own = rows.sorts.get(&sort).cloned().unwrap_or_default();
                let to = t.sort_mut(sort);
                *to = SortTables {
                    families: std::mem::take(&mut to.families),
                    words: std::mem::take(&mut to.words),
                    ..own
                };
            }
        }
        if rows.words.is_empty() {
            fell_back.push("treasure_word");
        } else {
            for sort in Sort::ALL {
                t.sort_mut(sort).words = rows
                    .words
                    .get(&sort)
                    .map(|w| w.values().cloned().collect())
                    .unwrap_or_default();
            }
        }
        if rows.looks.is_empty() {
            fell_back.push("treasure_look");
        } else {
            t.looks.clone_from(&rows.looks);
        }
        if rows.potions.is_empty() {
            fell_back.push("treasure_potion");
        } else {
            t.potions = rows.potions.values().cloned().collect();
        }
        if rows.families.is_empty() {
            fell_back.push("treasure_family");
        } else {
            for sort in Sort::ALL {
                t.sort_mut(sort).families = rows
                    .families
                    .get(&sort)
                    .map(|f| f.values().cloned().collect())
                    .unwrap_or_default();
            }
            t.pairs.clone_from(&rows.pairs);
            t.unnaming.clone_from(&rows.unnaming);
        }
        (t, fell_back)
    }

    /// Every family of enchantments, each sort's in turn.
    pub fn families(&self) -> impl Iterator<Item = &Family> {
        Sort::ALL
            .into_iter()
            .flat_map(move |s| self.sort(s).families.iter())
    }

    /// The tier an item carrying this state has, among every family.
    #[must_use]
    pub fn tier_of(&self, state: u32) -> Option<(When, Tier)> {
        self.families().find_map(|f| {
            f.tiers
                .iter()
                .find(|t| t.state == state)
                .map(|t| (f.when, *t))
        })
    }

    /// Every state a treasure item may carry or put on what it hits.
    pub fn states(&self) -> impl Iterator<Item = u32> + '_ {
        self.families()
            .flat_map(|f| f.tiers.iter())
            .flat_map(|t| [t.state, t.puts_on])
            .filter(|s| *s != 0)
    }

    /// Every potion effect's saved state.
    pub fn potion_states(&self) -> impl Iterator<Item = u32> + '_ {
        self.potions.iter().flatten().copied()
    }

    /// The look a template's treasure items draw, if they draw one.
    #[must_use]
    pub fn look_of(&self, template: u32) -> Option<&LookRow> {
        self.looks.iter().find(|r| r.template == template)
    }

    /// The table a template's treasure items draw their look in.
    #[must_use]
    pub fn look_table(&self, template: u32) -> Option<u32> {
        self.look_of(template).map(|r| r.table)
    }
}
