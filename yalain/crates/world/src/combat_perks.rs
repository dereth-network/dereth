//! Heroes' combat perks: a perk's skill records its kind and the value at its rank (the install's
//! perk records, three to six ranks each), and the player holds the highest rank it has of a kind
//! ([`crate::skills::perk_value`]). What each kind does (Turbine, May 2005, with each perk's
//! first-rank amount; the later ranks as their records hold):
//!
//! - **Night Stalker, Day Stalker:** accuracy by night, by day (+15, 30, 50);
//! - **Hunter** of a creature category: accuracy against creatures of it (+15, 30, 50);
//! - **Defense Against** a creature category: evasion against creatures of it (+15, 30, 50);
//! - **Dungeon Crawler, Plains Walker:** evasion indoors, outdoors (+20, 35, 50);
//! - **Kingdom Soldier, Kingdom Guard:** the damage it does to a player of another kingdom (×1.01 to
//!   ×1.03), the damage it takes from one (×0.98 to ×0.94); nothing for a neutral player, or
//!   between two of one kingdom (the Hero guide);
//! - **Deadly Precision:** the chance of a critical (+0.5%, 1%, 1.5%);
//! - **Fortitude:** heals of health it receives (+4% to +24%);
//! - **Imposing Facade, Unimposing Facade:** the share of the damage it does that creatures notice
//!   when they choose whom to fight (×1.03 to ×1.18, ×0.97 to ×0.82);
//! - **Commanding Presence:** how far from it a pet's attack order may send the pet (+5 to +30 m);
//! - **Aggressive Demeanor, Nurturing Demeanor:** its pets' damage, its pets' greatest health (×1.02
//!   to ×1.12);
//! - **Anatomy** of a race: the damage it does to a player of that race (×1.01 to ×1.03, the
//!   install's own text: "Increases damage done when attacking Human players").
//!
//! A creature's category is its template's (Nature, Decay, Martial, Arcane: the install's
//! creatures of each are the beasts and plants, the dead, the armed peoples, the made and the
//! magical).

use yalain_store::Store;

use crate::ratings::Rater;
use crate::world::Player;
use crate::Server;

/// The kinds of the combat perks.
pub mod kind {
    pub const NIGHT_STALKER: u32 = 3;
    pub const DAY_STALKER: u32 = 4;
    /// Hunter of Nature, Martial, Decay, Arcane.
    pub const HUNTER: [(u32, u32); 4] = [
        (5, super::category::NATURE),
        (6, super::category::MARTIAL),
        (7, super::category::DECAY),
        (8, super::category::ARCANE),
    ];
    /// Defense Against Nature, Decay, Arcane, Martial.
    pub const DEFENSE: [(u32, u32); 4] = [
        (9, super::category::NATURE),
        (10, super::category::DECAY),
        (11, super::category::ARCANE),
        (12, super::category::MARTIAL),
    ];
    pub const COMMANDING_PRESENCE: u32 = 13;
    pub const AGGRESSIVE_DEMEANOR: u32 = 14;
    pub const NURTURING_DEMEANOR: u32 = 15;
    pub const KINGDOM_SOLDIER: u32 = 21;
    pub const KINGDOM_GUARD: u32 = 22;
    pub const DEADLY_PRECISION: u32 = 23;
    pub const FORTITUDE: u32 = 24;
    pub const IMPOSING_FACADE: u32 = 25;
    pub const UNIMPOSING_FACADE: u32 = 26;
    pub const DUNGEON_CRAWLER: u32 = 30;
    pub const PLAINS_WALKER: u32 = 31;
    /// Anatomy of Humans, Lugians, Tumeroks, Drudges, Empyreans: each with the race it names.
    pub const ANATOMY: [(u32, u32); 5] = [(32, 1), (33, 2), (34, 4), (35, 8), (36, 16)];
}

/// A creature template's category.
pub mod category {
    pub const NATURE: u32 = 1;
    pub const DECAY: u32 = 2;
    pub const MARTIAL: u32 = 3;
    pub const ARCANE: u32 = 4;
}

/// What a player holds of a kind of perk.
fn perk<S: Store>(s: &Server<S>, p: &Player, kind: u32) -> Option<f64> {
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = crate::skills::held(&starting, &p.play.skills);
    crate::skills::perk_value(&s.skill_table, &skills, kind)
}

/// The category of a creature, by its template (none for a player or a template that names none).
#[must_use]
pub fn category_of<S: Store>(s: &Server<S>, who: Rater) -> Option<u32> {
    let template = match who {
        Rater::Player(_) => return None,
        Rater::Thing(id) => s.things.get(id)?.template,
        Rater::Template(t) => t,
    };
    s.catalog
        .get(template)?
        .props
        .word(yalain_content::props::creature_property::CATEGORY)
}

/// What a player's perks add to its accuracy against a target: its Stalker by the time of day, its
/// Hunter of the target's category.
#[must_use]
pub fn accuracy<S: Store>(s: &Server<S>, p: &Player, target: Rater) -> f64 {
    let stalker = if s.night {
        kind::NIGHT_STALKER
    } else {
        kind::DAY_STALKER
    };
    let hunter = category_of(s, target).and_then(|c| {
        kind::HUNTER
            .iter()
            .find(|(_, of)| *of == c)
            .and_then(|(k, _)| perk(s, p, *k))
    });
    perk(s, p, stalker).unwrap_or(0.0) + hunter.unwrap_or(0.0)
}

/// What a player's perks add to its evasion against an attacker: its Dungeon Crawler indoors, its
/// Plains Walker outdoors, its Defense Against the attacker's category.
#[must_use]
pub fn evasion<S: Store>(s: &Server<S>, p: &Player, attacker: Rater) -> f64 {
    let walker = if crate::presence::indoors(p.position.cell) {
        kind::DUNGEON_CRAWLER
    } else {
        kind::PLAINS_WALKER
    };
    let defense = category_of(s, attacker).and_then(|c| {
        kind::DEFENSE
            .iter()
            .find(|(_, of)| *of == c)
            .and_then(|(k, _)| perk(s, p, *k))
    });
    perk(s, p, walker).unwrap_or(0.0) + defense.unwrap_or(0.0)
}

/// What a player's Deadly Precision adds to the chance of its criticals.
#[must_use]
pub fn critical<S: Store>(s: &Server<S>, p: &Player) -> f64 {
    perk(s, p, kind::DEADLY_PRECISION).unwrap_or(0.0)
}

/// What a heal of a player's health is multiplied by: one and its Fortitude.
#[must_use]
pub fn heals<S: Store>(s: &Server<S>, p: &Player) -> f64 {
    1.0 + perk(s, p, kind::FORTITUDE).unwrap_or(0.0)
}

/// What the damage a player does is multiplied by as creatures notice it: its Facade's (one
/// without; the imposing one first when it holds both).
#[must_use]
pub fn noticed<S: Store>(s: &Server<S>, p: &Player) -> f64 {
    perk(s, p, kind::IMPOSING_FACADE)
        .or_else(|| perk(s, p, kind::UNIMPOSING_FACADE))
        .unwrap_or(1.0)
}

/// How much farther than the pets' own range a player's attack order may send its pets, in meters.
#[must_use]
pub fn command_range<S: Store>(s: &Server<S>, p: &Player) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let more = perk(s, p, kind::COMMANDING_PRESENCE).unwrap_or(0.0) as f32;
    more
}

/// What a player's pets' damage, and their greatest health, are multiplied by.
#[must_use]
pub fn pet_damage<S: Store>(s: &Server<S>, p: &Player) -> f64 {
    perk(s, p, kind::AGGRESSIVE_DEMEANOR).unwrap_or(1.0)
}

#[must_use]
pub fn pet_health<S: Store>(s: &Server<S>, p: &Player) -> f64 {
    perk(s, p, kind::NURTURING_DEMEANOR).unwrap_or(1.0)
}

/// What a hit of one player on another is multiplied by for the target's race: the attacker's
/// Anatomy of that race; one without it.
#[must_use]
pub fn against_race<S: Store>(s: &Server<S>, attacker: &Player, target: &Player) -> f64 {
    kind::ANATOMY
        .iter()
        .find(|(_, race)| *race == target.race)
        .and_then(|(k, _)| perk(s, attacker, *k))
        .unwrap_or(1.0)
}

/// What a hit of one player on another is multiplied by: the attacker's Kingdom Soldier and the
/// target's Kingdom Guard, when the two are of different kingdoms and neither is neutral (one in
/// no kingdom counts as neutral); one otherwise.
#[must_use]
pub fn between_kingdoms<S: Store>(s: &Server<S>, attacker: &Player, target: &Player) -> f64 {
    use crate::kingdoms::{counted_of, kingdom};
    let (a, t) = (
        counted_of(&attacker.qualities),
        counted_of(&target.qualities),
    );
    if a == t || a == kingdom::NEUTRAL || t == kingdom::NEUTRAL {
        return 1.0;
    }
    perk(s, attacker, kind::KINGDOM_SOLDIER).unwrap_or(1.0)
        * perk(s, target, kind::KINGDOM_GUARD).unwrap_or(1.0)
}
