//! Made items' own properties: what a recipe's item level makes a product carry beyond its
//! template. A template's numbers are placeholders (every sword's damage 10, every helm's armor
//! 10); a product is worth its level.
//!
//! **A recipe's outcome says what its product takes**: a mutation through the item level's table
//! names the item for its level ("Crude" to level 9, "Basic" to 19, "Sturdy" to 29, "Fine" to 39)
//! and sets its level as the least it may be used at. A mutation through a quality table makes it
//! equipment of that quality: a flawed making's table reads two levels below the spinner, a plain
//! one's the spinner, an expert one's two above, and the damage or armor is that quality's. A
//! recipe's trade goods have no quality: they are worth what went into them, 4 a unit of the first
//! material and 2 of the second. (2017: 351 items made at a spinner; every one with the item
//! level's mutation so named, 329 of 329, none without it; the least level the spinner's, 41 of 41
//! above level 1; trade goods' value exactly so, 263 of 263; 326 of 330 profiles byte for byte.)
//!
//! **Equipment**: a weapon's greatest damage by quality, its shield's a little over half that, with
//! the template's variance; armor's rating by quality, a robe's more, a leg piece's half, the
//! smaller pieces' a quarter (never below 2, 3 from level 10); 53 of 53 armor pieces and 13 of 13
//! weapons within one point of it. A weapon's vigor cost is its level's or one less, its combat
//! delay and an armor piece's drawn evenly; these and its value differ between items of one level
//! (2017: delays of 6 to 15 and 7 to 21). A weapon has two enchantment slots, a piece of armor
//! one, a robe three. The rule above level 30 follows the recorded treasure items of those levels:
//! no made item above level 30 was recorded.
//!
//! **A tool** takes no name and keeps its value: its recipe's table gives the craft skill level it
//! asks of whoever uses it (3 a level for a gathering tool, 10 for a working tool), a gathering
//! tool yields five above its level, and an expert making lasts twice as long. (2017: 7 of 7 tools'
//! profiles; a knife's cuts follow its yield, see [`crate::butchery`].)
//!
//! **What a product's own permanent effects do to it**: more damage, less of a span between its
//! least and greatest, half the vigor: the effect's magnitude at its spinner, read as the item is
//! made, and shown among its enchantments.

/// The table through which a mutation means the item's level: it reads the spinner.
pub const ITEM_LEVEL_TABLE: u32 = 0x6F00_06B6;

/// The item qualities a made item's own effects change: its greatest damage, the span below that a
/// hit may fall.
pub mod stat {
    pub const DAMAGE: u32 = 0x4_0258;
    pub const SPAN: u32 = 0x3_03E8;
    pub const VIGOR: u32 = 0x4_0261;
    pub const COMBAT_DELAY: u32 = 0x4_0259;
    pub const ARMOR: u32 = 0x4_0262;
    pub const LEAST_LEVEL: u32 = 0x4_02BC;
}

/// The tables through which a mutation makes a tool: a gathering tool's, reading the craft skill it
/// asks (3 a level, 1 at the first), and a working tool's (10 a level).
pub const GATHERING_TOOL_TABLE: u32 = 0x6F00_56CD;
pub const WORKING_TOOL_TABLE: u32 = 0x6F00_56C4;

/// A gathering tool's yield above its level (2017: 6 of 6 made; 6, 16, 22, 24, 25, 28, 33 among
/// knives by the craft skill they ask).
pub const TOOL_YIELD_ABOVE_LEVEL: u32 = 5;

/// A tool's greatest durability by the outcome's check: an expert making's twice a plain one's
/// (2017: 200 of the one expert, 100 of the four plain; a flawed one's half is this server's own).
#[must_use]
pub const fn tool_durability(expert: bool, flawed: bool) -> u32 {
    if expert {
        200
    } else if flawed {
        50
    } else {
        100
    }
}

/// The quality tables read ten for each level.
const QUALITY_PER_LEVEL: u32 = 10;

/// What a made item carries of its own.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Made {
    /// The string its name begins with.
    pub prefix: u32,
    /// What it is worth.
    pub value: u32,
    /// A weapon's greatest damage and the share below that a hit may fall.
    pub damage: Option<(i32, f32)>,
    /// A piece of armor's rating.
    pub armor: Option<i32>,
    pub delay: Option<i32>,
    pub vigor: Option<i32>,
    /// Its enchantment slots.
    pub slots: u32,
    /// It can be made harmonic.
    pub harmonic: bool,
    /// Where it is worn besides its template's places (a robe's legs).
    pub covers: u32,
    /// Its own effects change its damage, its vigor cost.
    pub damage_changed: bool,
    pub vigor_changed: bool,
    /// A tool's greatest durability.
    pub durability: Option<u32>,
    /// A tool's yield: what each use it makes gives more.
    pub tool_yield: Option<u32>,
    /// The craft skill level a tool asks.
    pub craft_needed: Option<u32>,
}

/// The name a level gives, a string of the item names' table: by tens, from the lowest.
#[must_use]
pub const fn prefix(level: u32) -> u32 {
    match level {
        ..=9 => 0x2AF5_7668,
        10..=19 => 0x2AF5_7664,
        20..=29 => 0x2AF5_7660,
        30..=39 => 0x2AF5_767C,
        40..=49 => 0x2AF5_7678,
        // The treasure items of these levels bear them, in no order the recordings settle.
        50..=59 => 0x2AF5_764C,
        60..=69 => 0x2AF5_7674,
        70..=79 => 0xA2F5_7669,
        80..=89 => 0xB2F5_7669,
        90..=99 => 0x82F5_7669,
        _ => 0x9AF5_7669,
    }
}

/// The level a quality table's value stands for.
#[must_use]
pub const fn quality_level(value: u32) -> u32 {
    value / QUALITY_PER_LEVEL
}

/// A value read between points, each (level, value) from the lowest: the nearest end's outside
/// them.
fn between(points: &[(u32, f32)], level: u32) -> f32 {
    let Some(&(first, low)) = points.first() else {
        return 0.0;
    };
    if level <= first {
        return low;
    }
    for w in points.windows(2) {
        let ((a, va), (b, vb)) = (w[0], w[1]);
        if level <= b {
            #[allow(clippy::cast_precision_loss)]
            let share = (level - a) as f32 / (b - a) as f32;
            return va + (vb - va) * share;
        }
    }
    points.last().map_or(0.0, |p| p.1)
}

/// A weapon's greatest damage at a quality, as recorded for made weapons to level 30 and for
/// treasure weapons past it.
const DAMAGE: [(u32, f32); 10] = [
    (1, 13.0),
    (5, 30.0),
    (7, 39.0),
    (10, 54.0),
    (12, 62.0),
    (20, 105.0),
    (29, 154.0),
    (30, 161.0),
    (50, 315.0),
    (100, 800.0),
];

/// A weapon's greatest damage at a quality.
#[must_use]
pub fn damage(quality: u32) -> i32 {
    dereth_primitives::num::to_i32(between(&DAMAGE, quality).round())
}

/// What a shield does of a weapon's damage.
const SHIELD_SHARE: f32 = 0.55;

/// A shield's greatest damage at a quality.
#[must_use]
pub fn shield_damage(quality: u32) -> i32 {
    dereth_primitives::num::to_i32((between(&DAMAGE, quality) * SHIELD_SHARE).round())
}

/// Where a piece of armor is worn, by what it gives of a whole suit's rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// A chest piece.
    Chest,
    /// A robe, in place of chest and legs.
    Robe,
    Legs,
    /// A helm, boots, gloves.
    Small,
}

impl Piece {
    /// Of where it may be worn and what it rules out (their bits).
    #[must_use]
    pub const fn of(places: u32, precluded: u32) -> Self {
        const CHEST: u32 = 0x2;
        const LEGS: u32 = 0x8;
        if places & CHEST != 0 {
            if precluded & LEGS != 0 {
                Self::Robe
            } else {
                Self::Chest
            }
        } else if places & LEGS != 0 {
            Self::Legs
        } else {
            Self::Small
        }
    }
}

/// A chest piece's rating at a quality: the curve through the recorded 3 at 5, 7 at 10, 12 at 15,
/// 19 at 20 and 32 at 28.
fn chest_rating(quality: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let l = quality as f32;
    l * (0.48 + 0.0235 * l)
}

/// What a robe gives of a chest piece's rating (2017: 24 at quality 20, 19 at 17).
const ROBE_SHARE: f32 = 1.27;

/// What a shield's rating gives of a chest piece's (2017: 32 at quality 29, the one recorded).
const SHIELD_RATING_SHARE: f32 = 0.95;

/// A shield's rating at a quality.
#[must_use]
pub fn shield_rating(quality: u32) -> i32 {
    dereth_primitives::num::to_i32((chest_rating(quality) * SHIELD_RATING_SHARE).round())
}

/// A piece of armor's rating at a quality.
#[must_use]
pub fn armor_rating(piece: Piece, quality: u32) -> i32 {
    let chest = chest_rating(quality);
    let rating = match piece {
        Piece::Chest => chest.round(),
        Piece::Robe => (chest * ROBE_SHARE).round(),
        Piece::Legs => (chest / 2.0).round().max(2.0),
        Piece::Small => {
            let least = if quality >= 10 { 3.0 } else { 2.0 };
            (chest / 4.0).round().max(least)
        }
    };
    dereth_primitives::num::to_i32(rating)
}

/// A piece of armor's rating at a quality before any rounding or floor.
#[must_use]
pub fn armor_rating_exactly(piece: Piece, quality: u32) -> f32 {
    let chest = chest_rating(quality);
    match piece {
        Piece::Chest => chest,
        Piece::Robe => chest * ROBE_SHARE,
        Piece::Legs => chest / 2.0,
        Piece::Small => chest / 4.0,
    }
}

/// A weapon's greatest vigor cost at a level: its level's, by tens above 8; nothing below.
#[must_use]
pub const fn vigor(level: u32) -> i32 {
    match level {
        ..=7 => 0,
        8..=9 => 3,
        10..=20 => 5,
        21..=29 => 7,
        30..=39 => 12,
        40..=49 => 17,
        _ => 17 + (level as i32 - 50) / 5,
    }
}

/// The combat delays made items were recorded with: a weapon's, a piece of armor's.
pub const WEAPON_DELAY: (i32, i32) = (6, 15);
pub const ARMOR_DELAY: (i32, i32) = (7, 21);

/// What a piece of equipment made at a level is worth, at least and at most (2017: 64 of 66 within
/// it: 1 to level 9, 2 to 4 at 12, 6 or 7 at 20, 7 to 9 at 25, 11 to 13 at 29; the curve is this
/// server's own above level 30).
#[must_use]
pub fn equipment_value(level: u32) -> (u32, u32) {
    let v = level * level / 64;
    (v.saturating_sub(2).max(1), v + 2)
}

/// What a recipe's trade goods are worth by the units of their materials used: 4 a unit of the
/// first, 2 of the second (2017: 263 of 263).
#[must_use]
pub const fn trade_goods_value(first: u32, second: u32) -> u32 {
    first * 4 + second * 2
}

/// The craft skills no tool's requirement names though its effect lifts them (2017: a Shaping
/// Hammer named four of the five armorsmithing skills its effect lifts, not the Drudges').
const NOT_NAMED_BY_TOOLS: [u32; 2] = [0x6F00_0691, 0x6F00_0692];

/// The craft skills a tool's requirement names, in the order of their records: a gathering tool's
/// the skill it gathers with, a working tool's those its worn effects lift.
#[must_use]
pub fn tool_skills(
    template: u32,
    worn: &[crate::effect_defs::EffectDef],
    skills: &[crate::craft::CraftSkillDef],
) -> Vec<(u32, u32)> {
    let mining = crate::mining::SKILLS
        .into_iter()
        .find(|k| crate::mining::skill_tools(*k).is_some_and(|(_, t)| t.contains(&template)));
    let gathers = if crate::butchery::is_knife(template) {
        Some(crate::butchery::SKILL)
    } else {
        mining
    };
    let mut named: Vec<&crate::craft::CraftSkillDef> = skills
        .iter()
        .filter(|d| match gathers {
            Some(k) => d.did == k,
            None => {
                !NOT_NAMED_BY_TOOLS.contains(&d.did)
                    && worn
                        .iter()
                        .any(|e| e.changes(1.0).iter().any(|c| c.0 == d.bonus_stat))
            }
        })
        .collect();
    named.sort_by_key(|d| d.did);
    named.into_iter().map(|d| d.common_name).collect()
}

/// How an effect a making gave an item shows: its name's table, the spellcraft when it shows one,
/// else the amount in hundredths of what it changes.
#[must_use]
pub fn effect_shown(
    d: &crate::effect_defs::EffectDef,
    when: crate::profile::When,
    spellcraft: f32,
) -> Option<crate::profile::ItemEffect> {
    let shown = crate::profile::EffectShown::of(d)?;
    if !shown.visible {
        return None;
    }
    let crate::effect_defs::Kind::Numeric {
        flags, magnitude, ..
    } = &d.kind
    else {
        return Some(crate::profile::ItemEffect {
            when,
            table: shown.table,
            spellcraft: shown
                .spellcraft_shown
                .then_some(dereth_primitives::num::to_i32(spellcraft)),
            amount: None,
            ends: None,
        });
    };
    let at = magnitude.at(spellcraft).unwrap_or(0.0);
    let amount = if shown.spellcraft_shown {
        // One that grows with its spellcraft shows the points it comes to, not the spellcraft.
        crate::profile::Amount::Points(
            dereth_primitives::num::floor_to_i32(at)
                .max(0)
                .cast_unsigned(),
        )
    } else if flags & crate::effect_defs::numeric::MULTIPLY != 0 {
        crate::profile::Amount::Percent((1.0 - at).abs() * 100.0)
    } else {
        crate::profile::Amount::Percent(at * 100.0)
    };
    Some(crate::profile::ItemEffect {
        when,
        table: shown.table,
        spellcraft: None,
        amount: Some(amount),
        ends: None,
    })
}

/// A made item's profile shows what it carries: its name after its level's word, its stats, its
/// enchantment slots, its effects with their amounts, that it binds.
pub fn show<S: yalain_store::Store>(
    s: &crate::Server<S>,
    t: &crate::things::Thing,
    v: &mut crate::profile::ItemView,
) {
    use crate::profile::When;
    v.binds = t.binds_on_use;
    if let Some(m) = t.own.made {
        apply_to_view(&m, &t.facts, v);
        if let (Some(now), Some(d)) = (t.durability, v.durability.as_mut()) {
            d.0 = i32::try_from(now).unwrap_or(i32::MAX);
        }
        if m.craft_needed.is_some() {
            let worn: Vec<crate::effect_defs::EffectDef> = t
                .bonuses
                .iter()
                .filter_map(|(state, _)| s.effect_table.get(*state).cloned())
                .collect();
            v.tool_skills = tool_skills(t.template, &worn, &s.crafting.skills)
                .into_iter()
                .map(crate::progress::text)
                .collect();
        }
    }
    let of = |list: &[(u32, f32)], when: When| -> Vec<crate::profile::ItemEffect> {
        list.iter()
            .filter_map(|(state, sc)| effect_shown(s.effect_table.get(*state)?, when, *sc))
            .collect()
    };
    let on_hit = |(state, _): &(u32, f32)| {
        matches!(
            yalain_content::treasure_effects::tier_of(*state),
            Some((yalain_content::treasure_effects::When::Hit, _))
        )
    };
    let (hits, kept): (Vec<_>, Vec<_>) = t.enchantments.iter().copied().partition(on_hit);
    let mut effects = of(&kept, When::Permanent);
    effects.extend(of(&t.bonuses, When::Equipped));
    effects.extend(of(&hits, When::Hit));
    v.effects.extend(effects);
}

/// An item's combat delay: its own, else its template's, as examining it shows it.
#[must_use]
pub fn combat_delay(m: Option<&Made>, props: Option<&yalain_content::Props>) -> i32 {
    let own = m.and_then(|m| m.delay);
    let of_template = || {
        let p = props?;
        let number = if p.int("Weapon_Damage").is_some() {
            p.int("Weapon_Speed")
        } else {
            p.int("Item_CombatDelay")
        };
        number.and_then(|d| i32::try_from(d).ok())
    };
    own.or_else(of_template).unwrap_or(0).max(0)
}

/// What a made item carries, on a view of its template.
pub fn apply_to_view(m: &Made, facts: &yalain_content::Facts, v: &mut crate::profile::ItemView) {
    if m.prefix != 0 {
        v.name = Some(crate::profile::made_name(facts, m.prefix));
    }
    if m.value > 0 {
        v.value = m.value;
    }
    if let Some(most) = m.durability {
        let most = i32::try_from(most).unwrap_or(i32::MAX);
        v.durability = Some((most, most));
    }
    v.tool = m.craft_needed.map(|craft| crate::profile::ToolView {
        yields: m.tool_yield,
        craft,
    });
    v.slots = m.slots;
    v.harmonic = m.harmonic;
    v.damage_changed = m.damage_changed;
    v.vigor_changed = m.vigor_changed;
    if let (Some(w), Some(delay), Some(vigor)) = (m.damage, m.delay, m.vigor) {
        v.weapon = Some((w.0, w.1, delay, vigor));
        if v.shield.is_some() {
            v.shield = m.armor;
        }
    } else if let (Some(rating), Some(delay)) = (m.armor, m.delay) {
        let places = v.armor.map_or(0, |a| a.2);
        v.armor = Some((rating, delay, places | m.covers));
    }
}
