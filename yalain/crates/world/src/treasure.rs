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
//! **Its numbers.** A template's damage and armor are placeholders, so a treasure weapon, shield or
//! piece of armor gets its own by its item level (its least level, or its dropper's base from level
//! 50 on), near what a made one has there: see [`numbers`]. The curves are fit to the recorded items,
//! not known. A shield is told by its class, as most shields' templates hold no weapon's damage.
//!
//! **Its enchantments.** It may draw some by its level: see [`enchantments`]. Each is cast at one
//! below its item level, and it is named for one of them. An effect on what it hits lands by that
//! effect's own chance.
//!
//! **Treasure jewelry** is plain with no least level, or by its dropper's level has one enchantment
//! on its wearer and a least level.
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
use yalain_content::treasure_tables::Rules;
pub use yalain_content::treasure_tables::{Sort, TreasureTables};

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
    /// What it drew of its look as it dropped.
    pub look: Option<Look>,
    /// The hand it was last held in (0: never): where its use puts it.
    pub hand: u32,
    /// The name table of the effect it is named for ("of the Inferno").
    pub suffix: Option<u32>,
    /// The character it is bound to, having been used in combat (0: nobody).
    pub bound_to: u64,
    /// A recipe attuned it to whoever holds it.
    pub attuned: bool,
}

/// What a treasure item drew of its look: the appearance table, and each key's value (no key, 0,
/// where it has fewer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub table: u32,
    pub values: [(u32, f32); 3],
}

impl Look {
    /// Its keys and their values.
    pub fn keys(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.values.iter().copied().filter(|(k, _)| *k != 0)
    }
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
            && self.look.is_none()
            && self.hand == 0
            && self.suffix.is_none()
            && self.bound_to == 0
            && !self.attuned
    }
}

/// The script packages whose items are treasure equipment, and the instruments among them, which
/// are not.
pub const EQUIPMENT: [&str; 3] = ["WeaponTemplate", "ClothingTemplate", "ShieldTemplate"];
pub const INSTRUMENTS: [&str; 1] = ["LuteTemplate"];
/// The script package of the shields among the treasure equipment.
pub const SHIELDS: [&str; 1] = ["ShieldTemplate"];
/// The script package of treasure jewelry.
pub const JEWELRY: [&str; 1] = ["JewelryTemplate"];
/// The script package of the treasure potions.
pub const POTIONS: [&str; 1] = ["PotionMagic"];
/// The script package of the Tesserae.
pub const TESSERAE: &str = "HeroSkillCreditToken";
/// The script packages of the tarot's trumps: the greater, and the lesser of each suit.
pub const TRUMPS: [&str; 5] = [
    "TrumpTemplate",
    "TrumpDrumsTemplate",
    "TrumpSceptersTemplate",
    "TrumpSkullsTemplate",
    "TrumpSwordsTemplate",
];
/// The property that makes an item a rare.
pub const RARE: &str = "Item_IsRareItem";

/// What treasure reads from the install: the packages of the catalog's treasure equipment and
/// potions, and the game's Arcane Lore table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kinds {
    pub equipment: std::collections::HashSet<u32>,
    /// The shields among the equipment.
    pub shields: std::collections::HashSet<u32>,
    pub jewelry: std::collections::HashSet<u32>,
    pub potions: std::collections::HashSet<u32>,
    /// The packages of the tarot's trumps.
    pub trumps: std::collections::HashSet<u32>,
    /// Each potion effect's name table and the strength of the color it gives its potion.
    pub potion_looks: std::collections::HashMap<u32, (u32, f32)>,
    /// The game's Arcane Lore table ([`ARCANE_LORE_TABLE`]).
    pub lore: crate::craft::LevelMap,
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

/// The sort of a treasure item of a template: a shield by its class, which a caller knows; else a
/// weapon when it has a weapon's damage, armor when it has a rating. None for anything else.
#[must_use]
pub fn sort_of(props: &yalain_content::Props, shield: bool) -> Option<Sort> {
    if shield {
        Some(Sort::Shield)
    } else if props.int("Weapon_Damage").is_some() {
        Some(Sort::Weapon)
    } else if props.int("Item_ArmorLevel").is_some() {
        Some(Sort::Armor)
    } else {
        None
    }
}

/// What a treasure weapon, shield or piece of armor is, at its item level: its template's numbers are
/// placeholders. A shield has a weapon's numbers with its own rating, delay and slot, whatever its
/// template holds (2017: 207 of 207 examined dropped shields showed a shield's damage, rating, delay
/// and vigor cost). None for jewelry. Worth what its drop drew; it carries no name word.
pub fn numbers(
    rng: &mut Rng,
    tables: &TreasureTables,
    props: &yalain_content::Props,
    sort: Sort,
    level: i32,
    value: u32,
) -> Option<crate::made::Made> {
    use crate::made::{self, Made, Piece};
    let (is_weapon, is_armor) = match sort {
        Sort::Weapon => (true, false),
        Sort::Shield => (true, true),
        Sort::Armor => (false, true),
        Sort::Jewelry => return None,
    };
    let quality = level.max(1).cast_unsigned();
    let of = tables.sort(sort);
    let drawn = |rng: &mut Rng| {
        let Some(d) = of.delays.as_ref() else {
            return 0;
        };
        d.from + i32::try_from(rng.weighted(d.weights.iter().copied()).unwrap_or(0)).unwrap_or(0)
    };
    #[allow(clippy::cast_precision_loss)]
    let rated = |rng: &mut Rng, exactly: f32| {
        let spread = of.rating_spread;
        let draw = of.rating_share_at(level) + rng.between(-spread, spread);
        dereth_primitives::num::to_i32((exactly * draw).round()).max(1)
    };
    let mut own = Made {
        value,
        ..Made::default()
    };
    if is_weapon {
        let (least, most) = of.damage_share.unwrap_or_default();
        let share = rng.between(least, most);
        let (low, high) = of.vigor_at(level);
        let vigor =
            low + i32::try_from(rng.below((high - low + 1).cast_unsigned() as usize)).unwrap_or(0);
        let variance = props.float("Weapon_Variance");
        #[allow(clippy::cast_precision_loss)]
        if is_armor {
            let (lo, hi) = of.span.unwrap_or_default();
            let variance = variance.unwrap_or_else(|| rng.between(lo, hi));
            let most = made::shield_damage(quality) as f32 * share;
            own.damage = Some((dereth_primitives::num::to_i32(most.round()), variance));
            own.armor = Some(rated(rng, made::shield_rating(quality) as f32));
            own.delay = Some(drawn(rng));
            own.vigor = Some(vigor);
            own.slots = u32::from(rng.chance(of.slot_chance));
        } else {
            let most = made::damage(quality) as f32 * share;
            let variance = variance.unwrap_or(0.0);
            own.damage = Some((dereth_primitives::num::to_i32(most.round()), variance));
            own.delay = Some(drawn(rng));
            own.vigor = Some(vigor);
            own.slots = 1;
        }
        return Some(own);
    }
    let places = props.word("Item_ValidInventoryLocations").unwrap_or(0);
    let precluded = props.word("Item_PrecludedInventoryLocations").unwrap_or(0);
    let piece = Piece::of(places, precluded);
    own.armor = Some(rated(rng, made::armor_rating_exactly(piece, quality)));
    own.delay = Some(drawn(rng));
    if piece == Piece::Robe {
        own.slots = 3;
        own.covers = precluded;
    } else {
        own.slots = u32::from(rng.chance(of.slot_chance));
    }
    Some(own)
}

/// What a treasure item of a template draws of its look, if its template is one known to: a weapon's
/// glow evenly up to its rule's most; a color in hundredths, up to its key's highest, or now and then
/// a round tenth. Fit to the items the 2017 servers made (4,952 of 4,998 values within their rows),
/// so our choice; the highest values on enchanted weapons need their enchantments.
pub fn draw_look(rng: &mut Rng, tables: &TreasureTables, template: u32) -> Option<Look> {
    use yalain_content::treasure_looks::GLOW;
    let row = tables.look_of(template)?;
    let rule = &tables.rules.look;
    let mut values = [(0, 0.0); 3];
    for (slot, (key, top)) in values.iter_mut().zip(row.keys.iter()) {
        let value = if *key == GLOW {
            rng.between(0.0, rule.glow_most)
        } else if *top >= rule.round_from && rng.chance(rule.round_chance) {
            #[allow(clippy::cast_precision_loss)]
            let tenth = rng.below(rule.round_tenths as usize) as f32;
            tenth / 10.0
        } else {
            let steps = dereth_primitives::num::to_i32(top * 100.0)
                .max(0)
                .cast_unsigned();
            #[allow(clippy::cast_precision_loss)]
            let step = rng.below(steps as usize + 1) as f32;
            step / 100.0
        };
        *slot = (*key, value);
    }
    Some(Look {
        table: row.table,
        values,
    })
}

/// What a treasure item draws of enchantments at an item level. How many goes by its sort and level
/// band, as often as 2017's drops had them (from level 10: a weapon one or more 10 to 16 in a
/// hundred, two or more only from 20; a piece of armor nearly always one, 7 to 17 in a hundred; a
/// shield one, 8 to 15). Which: a family by how often 2017 showed it, none twice; in a family of five
/// the tier of the level by the same bands as potions (2017: 256 of 268 of the weapons' effects), in
/// any other one it reaches, evenly. Those 2017 showed in pairs come together as one draw. Which is
/// fit to the recorded profiles, so our choice; those of a player's own skill are left out: 45 of the
/// 740 lines.
pub fn enchantments(
    rng: &mut Rng,
    tables: &TreasureTables,
    sort: Sort,
    level: i32,
) -> Vec<(
    yalain_content::treasure_effects::When,
    yalain_content::treasure_effects::Tier,
)> {
    use yalain_content::treasure_effects as t;
    use yalain_content::treasure_tables::Family;
    let of = tables.sort(sort);
    let how_many = of
        .counts
        .get(of.band(level))
        .map_or(0, |b| rng.weighted(b.weights.iter().copied()).unwrap_or(0));
    let mut open: Vec<&Family> = of
        .families
        .iter()
        .filter(|f| f.tiers.iter().any(|x| x.least <= level))
        .collect();
    let mut out = Vec::new();
    for _ in 0..how_many {
        let Some(i) = rng.weighted(open.iter().map(|f| f.weight)) else {
            break;
        };
        let f = open.swap_remove(i);
        let reached: Vec<&t::Tier> = f.tiers.iter().filter(|x| x.least <= level).collect();
        let tier = if f.by_level {
            reached.last().copied()
        } else {
            reached.get(rng.below(reached.len())).copied()
        };
        if let Some(tier) = tier {
            out.push((f.when, *tier));
            let other = tables.pairs.iter().find(|(a, _)| *a == tier.state);
            if let Some(paired) = other.and_then(|(_, b)| tables.tier_of(*b)) {
                out.push(paired);
            }
        }
    }
    out.sort_by_key(|(when, _)| match when {
        t::When::Permanent => 0,
        t::When::Equipped => 1,
        t::When::Hit => 2,
    });
    out
}

/// The word before a treasure item's name, once its effects have changed its numbers: among those of
/// its sort that go with its combat delay, by how many of 2017's items at its level showed each.
/// With none for its delay, among those of its level. Our fit: of 2,371 recorded items, it gives the
/// word each showed a chance of 0.49 for a weapon, 0.42 for armor, 0.34 for a shield.
#[must_use]
pub fn word_of(rng: &mut Rng, tables: &TreasureTables, sort: Sort, level: i32, delay: i32) -> u32 {
    use yalain_content::treasure_tables::Word;
    if sort == Sort::Jewelry {
        return 0;
    }
    let of = tables.sort(sort);
    let band = of.band(level);
    let weight = |x: &Word| x.weights.get(band).copied().unwrap_or(0.0);
    let of_level = || of.words.iter().filter(|x| weight(x) > 0.0);
    let goes = |x: &&Word| (x.delays.0..=x.delays.1).contains(&delay);
    let mut open: Vec<&Word> = of_level().filter(goes).collect();
    if open.is_empty() {
        open = of_level().collect();
    }
    rng.weighted(open.iter().map(|x| weight(x)))
        .map_or(crate::inventory::treasure_name::PREFIX_TEXT.0, |i| {
            open[i].word
        })
}

/// What a permanent effect does to an item's numbers (2017: a combat delay reduced to 1 and no
/// lower; the rest by what the effect says). A weapon's own effects change its numbers as they do a
/// made one's.
pub fn apply_numbers(own: &mut Own, def: &crate::effect_defs::EffectDef, spellcraft: f32) {
    use crate::made::stat;
    let Some(m) = own.made.as_mut() else {
        return;
    };
    for (which, multiply, change) in def.changes(spellcraft) {
        if multiply {
            continue;
        }
        let by = dereth_primitives::num::to_i32(change.round());
        match which {
            // A weapon's span narrows by its share (2017: of weapons with such an edge, each 0.03 to
            // 0.09 under the plain ones of its template, within the effect's own scale).
            stat::SPAN => {
                if let Some(d) = m.damage.as_mut() {
                    d.1 = (d.1 + change).max(0.0);
                    m.damage_changed = true;
                }
            }
            stat::DAMAGE => {
                if let Some(d) = m.damage.as_mut() {
                    d.0 = (d.0 + by).max(1);
                    m.damage_changed = true;
                }
            }
            stat::VIGOR => {
                if let Some(v) = m.vigor.as_mut() {
                    *v = (*v + by).max(1);
                    m.vigor_changed = true;
                }
            }
            stat::COMBAT_DELAY => m.delay = m.delay.map(|d| (d + by).max(1)),
            stat::ARMOR => m.armor = m.armor.map(|a| (a + by).max(1)),
            stat::LEAST_LEVEL => own.min_level = own.min_level.map(|l| (l + by).max(1)),
            _ => {}
        }
    }
}

/// What a weapon's slayer does to a creature of a superclass at a spellcraft: nothing to any other
/// (2017: a slayer cast at 30, one weapon, 182 harms of exactly 30 told over 180 landed hits on the
/// superclass it names, basic and special alike, and 12 over 902 on any other). That it goes by
/// spellcraft is our reading of the one weapon and the effect's number of 1.
#[must_use]
pub fn slayer_harm(classes: &[(u32, f32)], superclass: u32, spellcraft: f32) -> i32 {
    classes
        .iter()
        .find(|(c, _)| *c == superclass)
        .map_or(0, |(_, by)| dereth_primitives::num::to_i32(spellcraft * by))
}

/// A hit lands: each slayer of the striker's weapon harms what it hits if it names it, told as its
/// own story; each effect the weapon puts on what it hits, by that effect's own chance.
pub fn on_hit<S: yalain_store::Store>(
    s: &mut crate::Server<S>,
    net: &mut yalain_net::ServerNet,
    world: yalain_net::ConnId,
    target: u64,
    clock: crate::Clock,
) {
    use yalain_content::treasure_effects as t;
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(weapon) = crate::skill_use::wielded(s, me).first().map(|(i, _)| *i) else {
        return;
    };
    let list = s
        .things
        .get(weapon)
        .map(|w| w.enchantments.clone())
        .unwrap_or_default();
    for (state, spellcraft) in list {
        if let Some(crate::effect_defs::Kind::Slayer(classes)) =
            s.effect_table.get(state).map(|d| &d.kind)
        {
            let superclass = s
                .things
                .get(target)
                .and_then(|t| s.catalog.get(t.template))
                .and_then(|t| {
                    t.props
                        .word(yalain_content::props::creature_property::SUPER_CLASS)
                });
            let harm = superclass.map_or(0, |c| slayer_harm(classes, c, spellcraft));
            if harm > 0 {
                crate::effects::creature_health(s, net, target, -harm, me, clock);
            }
        }
        let Some((t::When::Hit, tier)) = s.treasure_tables.tier_of(state) else {
            continue;
        };
        if tier.puts_on != 0 && s.rng.chance(tier.chance) {
            crate::effects::apply_on(
                s,
                net,
                crate::effects::On::Creature(target),
                tier.puts_on,
                crate::effects::Cast {
                    caster: me,
                    spellcraft,
                    pk: true,
                    item: weapon,
                    ..crate::effects::Cast::default()
                },
                clock,
            );
        }
    }
}

/// The saved state of the game's Arcane Lore table: the lore of each item level from 50 to 150.
pub const ARCANE_LORE_TABLE: u32 = 0x7000_1BE5;

/// Read the Arcane Lore table's graph.
///
/// # Errors
/// Its list is missing, or holds fewer than two levels.
pub fn lore_table(
    lib: &auberean_script::library::Library,
    g: &auberean_script::graph::Graph,
) -> Result<crate::craft::LevelMap, String> {
    let table = crate::craft::LevelMap::from_state(lib, g)?;
    if table.values.len() < 2 {
        return Err(format!(
            "the Arcane Lore table holds {} levels",
            table.values.len()
        ));
    }
    Ok(table)
}

/// The Arcane Lore of an item level by the game's table (between the table's levels in proportion,
/// whole), none at its first level or below.
#[must_use]
pub fn lore_at(table: &crate::craft::LevelMap, level: f32) -> Option<i32> {
    let values = &table.values;
    #[allow(clippy::cast_precision_loss)]
    let first = table.first as f32;
    if level <= first || values.len() < 2 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let at = (level - first).min((values.len() - 1) as f32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (at.floor() as usize).min(values.len() - 2);
    #[allow(clippy::cast_precision_loss)]
    let frac = at - i as f32;
    #[allow(clippy::cast_precision_loss)]
    let (lo, hi) = (values[i] as f32, values[i + 1] as f32);
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

/// A treasure item's own least level, from its dropper's base.
pub fn min_level(rng: &mut Rng, rules: &Rules, base: i32) -> i32 {
    let above = rng.weighted(rules.above_base.iter().copied()).unwrap_or(0);
    (base + i32::try_from(above).unwrap_or(0)).clamp(1, rules.most_level.max(1))
}

/// A treasure item's own Arcane Lore, from its dropper's base, by the game's table: none when its
/// base is not above its rule's, or the item level drawn is the table's first or below.
pub fn arcane_lore(
    rng: &mut Rng,
    rules: &Rules,
    table: &crate::craft::LevelMap,
    base: i32,
) -> Option<i32> {
    let lore = rules.lore;
    if base <= lore.above {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (from, to) = ((base + lore.from) as f32, (base + lore.to) as f32);
    lore_at(table, rng.between(from, to))
}

/// A treasure potion's effect, from its dropper's level: its kind drawn evenly, at the level's
/// tier, at a spellcraft drawn evenly from 0 to 100. None when its kind has no effect at that tier.
pub fn potion_effect(rng: &mut Rng, tables: &TreasureTables, level: i32) -> Option<(u32, f32)> {
    let kind = rng.below(tables.potions.len());
    let state = tables
        .potions
        .get(kind)
        .and_then(|k| k.get(tables.rules.potion_tier(level)).copied());
    let (lo, hi) = tables.rules.potion_spellcraft;
    let spellcraft = rng.between(lo, hi);
    state.map(|s| (s, spellcraft))
}

/// Give a treasure item dropped by a creature of this level and loot quality what it carries of its
/// own: equipment its own least level (and an Arcane Lore when it draws one), numbers and
/// enchantments, whatever least level its template holds (2017: 129 of 130 examined drops of
/// templates that set one showed their own, from their dropper); a treasure potion its effect. Any
/// other item is left as it is.
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
    let rules = &s.treasure_tables.rules;
    if s.treasure.equipment.contains(&package) {
        own.look = draw_look(&mut s.rng, &s.treasure_tables, template);
        let least = min_level(&mut s.rng, rules, base);
        own.min_level = Some(least);
        own.arcane_lore = arcane_lore(&mut s.rng, rules, &s.treasure.lore, base);
        let value = s.things.get(item).map_or(0, |t| t.value);
        let shield = s.treasure.shields.contains(&package);
        if let Some(t) = s.catalog.get(template) {
            let props = t.props.clone();
            let item_level = least.max(base);
            if let Some(sort) = sort_of(&props, shield) {
                own.made = numbers(
                    &mut s.rng,
                    &s.treasure_tables,
                    &props,
                    sort,
                    item_level,
                    value,
                );
                put_enchantments(s, item, &mut own, sort, item_level);
                if let Some(m) = own.made.as_mut() {
                    let delay = m.delay.unwrap_or(0);
                    m.prefix = word_of(&mut s.rng, &s.treasure_tables, sort, item_level, delay);
                }
            }
        }
    } else if s.treasure.jewelry.contains(&package) {
        // Enchanted or plain by its dropper, with a least level only if enchanted (2017: 73 of 73
        // with a least level had one effect, 120 of 120 with none had none).
        let least = min_level(&mut s.rng, rules, base);
        if put_enchantments(s, item, &mut own, Sort::Jewelry, least.max(base)) {
            own.min_level = Some(least);
            own.arcane_lore =
                arcane_lore(&mut s.rng, &s.treasure_tables.rules, &s.treasure.lore, base);
        }
    } else if s.treasure.potions.contains(&package) {
        let drawn = potion_effect(&mut s.rng, &s.treasure_tables, level);
        if let Some((state, spellcraft, &(table, color))) = drawn.and_then(|(state, spellcraft)| {
            let look = s.treasure.potion_looks.get(&state)?;
            Some((state, spellcraft, look))
        }) {
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

/// An item's enchantments drawn at its item level: kept with it, its numbers changed by those on it
/// always, and a weapon's glow the first of them that has one (2017: 149 of 162 weapons). Returns
/// whether it drew any.
fn put_enchantments<S: yalain_store::Store>(
    s: &mut crate::Server<S>,
    item: u64,
    own: &mut Own,
    sort: Sort,
    level: i32,
) -> bool {
    use yalain_content::treasure_effects::When;
    #[allow(clippy::cast_precision_loss)]
    let spellcraft = (level - 1).max(1) as f32;
    let drawn = enchantments(&mut s.rng, &s.treasure_tables, sort, level);
    // Named for one of them (2017: of 109 weapons named for one of two or more, 74 for an effect on
    // a hit, 30 on itself, 5 on the wielder; no order or value of the effects says which).
    let namers: Vec<u32> = drawn
        .iter()
        .map(|(_, tier)| tier.state)
        .filter(|state| !s.treasure_tables.unnaming.contains(state))
        .collect();
    if !namers.is_empty() {
        let named = namers[s.rng.below(namers.len())];
        own.suffix = s
            .effect_table
            .get(named)
            .and_then(|d| d.name)
            .map(|(_, table)| table);
    }
    let mut glow = None;
    for (when, tier) in &drawn {
        if *when == When::Permanent {
            if let Some(d) = s.effect_table.get(tier.state) {
                apply_numbers(own, d, spellcraft);
            }
        }
        if glow.is_none() && tier.glow > 0.0 {
            glow = Some(tier.glow);
        }
    }
    if let (Some(glow), Some(look)) = (glow, own.look.as_mut()) {
        for (key, value) in &mut look.values {
            if *key == yalain_content::treasure_looks::GLOW {
                *value = glow;
            }
        }
    }
    if let Some(t) = s.things.get_mut(item) {
        for (when, tier) in &drawn {
            if *when == When::Equipped {
                t.bonuses.push((tier.state, spellcraft));
            } else {
                t.enchantments.push((tier.state, spellcraft));
            }
        }
    }
    !drawn.is_empty()
}
