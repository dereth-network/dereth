//! Treasure items' own properties: what a creature's loot makes each item carry beyond its
//! template, chosen as it drops, by the creature that dropped it.
//!
//! **The dropper's levels.** The tier of a potion's effect follows the dead creature's level; its
//! loot base is its loot quality override in tens when its template sets one (a level 43 creature's
//! 440 is 44), else its level.
//!
//! **Equipment** (a weapon but a lute or a flute, armor or clothing, a shield) whose template sets
//! no least level gets its own: the base plus 0, 1, 2 or 3 (about 36, 43, 18 and 3 in a hundred),
//! at most 50. A base above 51 may also give it an Arcane Lore needed: an item level drawn evenly
//! between six and one below the base, read through the game's Arcane Lore
//! table (level 50 none, 51 seventeen, ... 150 887) when it is above 50. (2017: 1,952 of 1,952
//! dropped weapons, armor and shields examined whose templates set none showed their own, none of
//! 41 lutes and flutes; below a base of 48, 1,214 of 1,256 at the base plus 0 to 3; from a base of
//! 50, 533 of 533 at 50; no Arcane Lore at a base of 51 or below, 0 of 1,616, and a share and a
//! value growing with the base above it.)
//!
//! **A treasure potion** is drunk for one effect of its own: one of nine kinds (attack speed, damage
//! reflected, health over time, health regained, greatest health, greatest vigor, run speed, vigor
//! over time, vigor regained) at the tier of the dropper's level (up to 15, 16 to 29, 30 to 39, 40
//! to 44, 45 and up), at a spellcraft drawn evenly from 0 to 100 whatever the level. It is named
//! for its effect ("Potion of Revival") and colored by it. (2017: 86 of 87 examined potions'
//! effects in their level's tier; spellcraft 3 to 98, no tie to the level; 26 of 26 drinks put on
//! the potion's own effect, at its spellcraft, which its profile shows cut to a whole number, 21 of
//! 21; 126 of 126 potions made so named and colored.)

use crate::rng::Rng;

/// What an item carries of its own beyond its template.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Own {
    /// The least level it may be put on at.
    pub min_level: Option<i32>,
    /// The Arcane Lore it takes to put it on.
    pub arcane_lore: Option<i32>,
    /// The effect it is drunk for.
    pub effect: Option<OwnEffect>,
    /// What a recipe's item level gave it.
    pub made: Option<crate::made::Made>,
    /// The color a dye gave it: the appearance it is in when it lies in a pack, and the dye's value.
    pub dye: Option<(u32, f32)>,
    /// The craft flags a recipe gave it, beside its template's.
    pub craft_flags: u32,
}

/// A potion's own effect: its saved state and spellcraft, the table of its name (which holds the
/// potion's name for it), and the strength of the potion's color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OwnEffect {
    pub state: u32,
    pub spellcraft: f32,
    pub table: u32,
    pub color: f32,
}

impl Own {
    /// Does it carry nothing of its own?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.min_level.is_none()
            && self.arcane_lore.is_none()
            && self.effect.is_none()
            && self.made.is_none()
            && self.dye.is_none()
            && self.craft_flags == 0
    }
}

/// The script packages whose items are treasure equipment, and the instruments among them, which
/// are not.
pub const EQUIPMENT: [&str; 3] = ["WeaponTemplate", "ClothingTemplate", "ShieldTemplate"];
pub const INSTRUMENTS: [&str; 1] = ["LuteTemplate"];
/// The script package of the treasure potions.
pub const POTIONS: [&str; 1] = ["PotionMagic"];
/// The script package of the Tesserae.
pub const TESSERAE: &str = "HeroSkillCreditToken";

/// The packages of the catalog's treasure equipment and potions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kinds {
    pub equipment: std::collections::HashSet<u32>,
    pub potions: std::collections::HashSet<u32>,
    /// Each potion effect's name table and the strength of the color it gives its potion.
    pub potion_looks: std::collections::HashMap<u32, (u32, f32)>,
}

/// The strength of the color a potion effect gives its potion, from its saved state's graph.
#[must_use]
pub fn potion_color(
    lib: &auberean_script::library::Library,
    g: &auberean_script::graph::Graph,
) -> Option<f32> {
    let nav = crate::start::Nav::new(lib, g);
    let root = nav.object(g.root()?).ok()?;
    nav.number(root, "m_tsysAprValue").ok()
}

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

/// The highest least level an item's own may be.
pub const MOST_LEVEL: i32 = 50;

/// How many of the 2017 items' least levels stood 0, 1, 2 and 3 above the base.
const ABOVE_BASE: [f32; 4] = [436.0, 519.0, 220.0, 39.0];

/// The game's Arcane Lore table: the lore of each item level from 50 to 150.
pub const ARCANE_LORE: [u16; 101] = [
    0, 17, 26, 34, 41, 49, 56, 65, 74, 82, 90, 98, 105, 112, 120, 127, 134, 142, 149, 157, 165,
    172, 180, 188, 196, 204, 212, 220, 229, 237, 245, 254, 262, 270, 279, 287, 296, 304, 312, 321,
    329, 337, 345, 354, 362, 370, 379, 387, 395, 404, 412, 421, 429, 438, 447, 455, 464, 473, 481,
    490, 499, 508, 517, 526, 535, 544, 554, 563, 572, 581, 591, 600, 610, 619, 629, 638, 648, 657,
    667, 677, 687, 696, 706, 716, 726, 736, 746, 755, 765, 775, 785, 795, 805, 816, 826, 836, 846,
    856, 866, 876, 887,
];

/// The Arcane Lore of an item level (between the table's levels in proportion, whole), none at 50
/// or below.
#[must_use]
pub fn lore_at(level: f32) -> Option<i32> {
    if level <= 50.0 {
        return None;
    }
    let at = (level - 50.0).min(100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (at.floor() as usize).min(ARCANE_LORE.len() - 2);
    #[allow(clippy::cast_precision_loss)]
    let frac = at - i as f32;
    let lo = f32::from(ARCANE_LORE[i]);
    let hi = f32::from(ARCANE_LORE[i + 1]);
    let lore = dereth_primitives::num::to_i32(lo + (hi - lo) * frac);
    (lore > 0).then_some(lore)
}

/// The loot base of a creature: its loot quality override in tens, else its level.
#[must_use]
pub fn base_of(level: i32, quality: Option<i64>) -> i32 {
    quality
        .filter(|q| *q > 0)
        .and_then(|q| i32::try_from(q / 10).ok())
        .unwrap_or(level)
}

/// The tier (0 the lowest) of a treasure potion dropped by a creature of this level.
#[must_use]
pub const fn potion_tier(level: i32) -> usize {
    match level {
        ..=15 => 0,
        16..=29 => 1,
        30..=39 => 2,
        40..=44 => 3,
        _ => 4,
    }
}

/// A treasure item's own least level, from its dropper's base.
pub fn min_level(rng: &mut Rng, base: i32) -> i32 {
    let above = rng.weighted(ABOVE_BASE).unwrap_or(0);
    (base + i32::try_from(above).unwrap_or(0)).clamp(1, MOST_LEVEL)
}

/// A treasure item's own Arcane Lore, from its dropper's base: none when the item level drawn is
/// 50 or below.
pub fn arcane_lore(rng: &mut Rng, base: i32) -> Option<i32> {
    if base <= 51 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let top = (base - 1) as f32;
    lore_at(rng.between(top - 5.0, top))
}

/// A treasure potion's effect, from its dropper's level: its kind drawn evenly, at the level's
/// tier, at a spellcraft drawn evenly from 0 to 100.
pub fn potion_effect(rng: &mut Rng, level: i32) -> (u32, f32) {
    let kind = rng.below(POTION_EFFECTS.len());
    (
        POTION_EFFECTS[kind][potion_tier(level)],
        rng.between(0.0, 100.0),
    )
}

/// Give a treasure item dropped by a creature of this level and loot quality what it carries of its
/// own: equipment whose template sets no least level its own (and an Arcane Lore when it draws
/// one), a treasure potion its effect. Any other item is left as it is.
pub fn endow<S: yalain_store::Store>(
    s: &mut crate::Server<S>,
    item: u64,
    level: i32,
    quality: Option<i64>,
) {
    let Some((template, package)) = s
        .things
        .get(item)
        .map(|t| (t.template, t.description.package))
    else {
        return;
    };
    let base = base_of(level, quality);
    let mut own = Own::default();
    if s.treasure.equipment.contains(&package) {
        let set = s
            .catalog
            .get(template)
            .is_some_and(|t| t.props.int("Usage_MinLevel").is_some());
        if !set {
            own.min_level = Some(min_level(&mut s.rng, base));
            own.arcane_lore = arcane_lore(&mut s.rng, base);
        }
    } else if s.treasure.potions.contains(&package) {
        let (state, spellcraft) = potion_effect(&mut s.rng, level);
        if let Some(&(table, color)) = s.treasure.potion_looks.get(&state) {
            own.effect = Some(OwnEffect {
                state,
                spellcraft,
                table,
                color,
            });
        }
    }
    if let Some(t) = s.things.get_mut(item) {
        t.own = own;
    }
}
