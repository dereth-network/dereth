//! The world's step: camps keep their creatures alive, creatures act, corpses go.
//!
//! **Camps.** Each camp of the spawn table keeps its population alive: at the first step every camp
//! is filled; when one of its creatures dies, a replacement appears after the camp's period.
//! Outdoors a creature appears at a point drawn inside the camp's radius (on the ground, when the
//! host knows the land); indoors at one of the camp's spots. A creature's greatest health is its
//! template's, varied a little each time one appears.
//!
//! **Items.** A camp whose template is an item (no creature, a template with a value) keeps it
//! lying at the camp's place: once it is taken (picked up, or gone), a new one lies there after the
//! camp's period (2017: quest items lying alone came back 20.0-20.1 s after their taking, 9 of 9
//! timed; keys 30.0-30.1 s, 6 of 6; a dungeon's mushrooms 181-193 s, 6 of 6).
//!
//! **Summons.** A camp never makes a summoned creature (a template with a pet's AI or a pet's NPC
//! table): those were players' pets and conjurings, seen beside their owners and never made by a
//! camp (2017: every one of the 417 creatures given a player as its pet was of such a template).
//!
//! **Moving.** A camp's movement settings stand over its creatures' templates' (a camp may make
//! them stationary, free of their home, wanderers or not, of another perception), and a camp that
//! names a route sets each creature it makes off along it, toward the camp's starting waypoint
//! and the camp's way round ([`crate::ai`]).
//!
//! **Appearing.** A new creature is made for every client whose view holds its place, with its
//! entry effect.

use dereth_primitives::num::math;
use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::things::{Creature, Kind, Mode, Thing};
use crate::{presence, Clock, Server};

/// How often the world takes a step, in seconds.
pub const STEP_SECONDS: f64 = 0.25;

/// How much a creature's greatest health varies either side of its template's, as a share: the
/// 2017 creatures' selection information held the template's value or one within 3% of it
/// (9,048 of 9,336).
pub const HEALTH_SPREAD: f32 = 0.03;

/// The share of creatures that appear with exactly their template's greatest health (3,743 of
/// 9,048 in 2017).
pub const HEALTH_EXACT: f32 = 0.41;

/// The AI superclass of pets and other summoned creatures (`AI_AISuperClass`).
pub const PET_AI: u32 = 0xFF03;

/// Is a template a summoned creature: a pet's AI, or a pet's NPC table?
#[must_use]
pub fn is_summon(props: &yalain_content::Props) -> bool {
    props.word("AI_AISuperClass") == Some(PET_AI)
        || props
            .text("AI_NPCTable")
            .is_some_and(|t| t.starts_with("NPCTable_Pet"))
}

/// A camp's state: its creatures alive, and when each replacement is due.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CampState {
    pub alive: Vec<u64>,
    pub due: Vec<f64>,
    pub filled: bool,
}

/// The practice dummies that send back a share of each attack on them: every level's, with a quest
/// on its death and without. The one of level 30 on another kind of table never does.
pub const PRACTICE_DUMMIES: [u32; 9] = [
    0x4700_2DAB,
    0x4700_2DAE,
    0x4700_2DAF,
    0x4700_2DB1,
    0x4700_2DB2,
    0x4700_2DB3,
    0x4700_2DB4,
    0x4700_2DB5,
    0x4700_2DB6,
];
/// What a practice dummy holds from its first moment: 0.01 of an attack sent back for each point of
/// spellcraft.
pub const PRACTICE_DUMMY_EFFECT: u32 = 0x7000_1C79;

/// The spellcraft of a practice dummy's effect, by its level. 2017's shares: 0.35 at 40 (74 of 74
/// attacks with a skill), 0.40 at 50 (10 of 10), 0.45 at 60 (1 of 1), 0.50 at 70 (28 of 28). None
/// recorded at 30, where this gives 0.30; its effect's appearance shows in 45 of 54.
#[must_use]
pub fn practice_dummy_spellcraft(level: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let level = level as f32;
    15.0 + level / 2.0
}

/// A practice dummy holds its effect as it appears: for as long as it stands, shown at once.
fn holds_its_effect<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(t) = s.things.get(id) else {
        return;
    };
    let (template, level) = (t.template, t.creature.as_ref().map_or(0, |c| c.stats.level));
    if !PRACTICE_DUMMIES.contains(&template) {
        return;
    }
    let Some(def) = s.effect_table.get(PRACTICE_DUMMY_EFFECT).cloned() else {
        return;
    };
    let cast = crate::effects::Cast {
        caster: id,
        spellcraft: practice_dummy_spellcraft(level),
        ..crate::effects::Cast::default()
    };
    let shape = crate::effects::Shape {
        timeout: Some(f32::MAX),
        ..crate::effects::Shape::TIMED
    };
    crate::effects::add_on(
        s,
        net,
        crate::effects::On::Creature(id),
        &def,
        cast,
        shape,
        clock,
    );
}

/// The side of a landblock and of a land cell, in meters.
pub const BLOCK_SIDE: f32 = 160.0;
pub const CELL_SIDE: f32 = 20.0;
/// The side of the whole land, of 256 landblocks.
pub const LAND_SIDE: f32 = 256.0 * BLOCK_SIDE;

/// An outdoor point given relative to a landblock, as a cell and a point of the landblock it lies
/// in (landblocks are 160 m on a side; land cells 20 m, numbered from 1 along y within each x).
#[must_use]
pub fn outdoor(landblock: u32, x: f32, y: f32) -> (u32, f32, f32) {
    let (mut bx, mut by) = (
        i64::from(landblock >> 8 & 0xFF),
        i64::from(landblock & 0xFF),
    );
    // No point of the land is further from a block than this.
    let (mut x, mut y) = (
        x.clamp(-LAND_SIDE, LAND_SIDE),
        y.clamp(-LAND_SIDE, LAND_SIDE),
    );
    while x < 0.0 {
        x += BLOCK_SIDE;
        bx -= 1;
    }
    while x >= BLOCK_SIDE {
        x -= BLOCK_SIDE;
        bx += 1;
    }
    while y < 0.0 {
        y += BLOCK_SIDE;
        by -= 1;
    }
    while y >= BLOCK_SIDE {
        y -= BLOCK_SIDE;
        by += 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (cx, cy) = ((x / CELL_SIDE) as u32, (y / CELL_SIDE) as u32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let block = ((bx.clamp(0, 255) as u32) << 8) | by.clamp(0, 255) as u32;
    (block << 16 | (cx.min(7) * 8 + cy.min(7) + 1), x, y)
}

/// The world's step.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    for i in 0..s.spawns.camps.len() {
        fill(s, net, i, clock);
    }
    crate::ai::step(s, net, clock);
    crate::reload::retire_rested(s, net);
    crate::walkers::step(s, net, clock);
    crate::pets::step(s, net, clock);
    crate::death::expire_corpses(s, net, now);
    crate::mining::expire(s, net, now);
    crate::regen::step(s, net, now);
    crate::talk::step(s, net, clock);
}

/// Make the creatures a camp lacks that are due.
fn fill<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize, clock: Clock) {
    let now = clock.local.0;
    let Some(state) = s.camps.get_mut(i) else {
        return;
    };
    let population = s.spawns.camps[i].population as usize;
    // Its items taken from where they lay are replaced after its period, as its dead creatures are.
    let regen = s.spawns.camps[i].regen;
    let things = &s.things;
    let taken: Vec<u64> = state
        .alive
        .iter()
        .copied()
        .filter(|id| {
            things
                .get(*id)
                .is_none_or(|t| t.kind == Kind::Item && (t.at.is_none() || t.container.is_some()))
        })
        .collect();
    for id in taken {
        state.alive.retain(|x| *x != id);
        state.due.push(now + regen);
    }
    let mut make = 0;
    if state.filled {
        state.due.sort_by(f64::total_cmp);
        while state.alive.len() + make < population && state.due.first().is_some_and(|t| *t <= now)
        {
            state.due.remove(0);
            make += 1;
        }
    } else {
        state.filled = true;
        make = population.saturating_sub(state.alive.len());
    }
    for _ in 0..make {
        if let Some(id) = spawn(s, i, now) {
            s.camps[i].alive.push(id);
            presence::thing_appears(s, net, id);
            holds_its_effect(s, net, id, clock);
        }
    }
}

/// A camp makes one creature, or lays one item. Returns its id.
pub fn spawn<S: Store>(s: &mut Server<S>, i: usize, now: f64) -> Option<u64> {
    let mut camp = s.spawns.camps.get(i)?.clone();
    camp.templates
        .retain(|(t, _)| !s.catalog.get(*t).is_some_and(|t| is_summon(&t.props)));
    if camp.templates.is_empty() {
        s.unhandled
            .note(format!("a camp of summoned creatures ({})", camp.id));
        return None;
    }
    let pick = s.rng.weighted(
        camp.templates
            .iter()
            .map(|(_, w)| f32::from(u16::try_from(*w).unwrap_or(u16::MAX))),
    )?;
    let template = s.catalog.get(camp.templates[pick].0)?.clone();
    let Some(mut stats) = yalain_content::CreatureStats::from_props(&template.props) else {
        if is_item(&template.props) {
            let (at, _) = place(s, &camp, i);
            return lay(s, template.id, at);
        }
        s.unhandled.note(format!(
            "a camp's template {:08X} is no creature",
            template.id
        ));
        return None;
    };
    stats.set_movement(&camp.moves.settings);
    let patrol = camp
        .moves
        .route
        .as_ref()
        .filter(|r| !r.waypoints.is_empty())
        .map(|r| crate::ai::Patrol::new(r.clone(), camp.moves.start, camp.moves.backward));
    let (mut at, heading) = place(s, &camp, i);
    // A spot is where one was seen; on open ground one that flies appears at its height.
    if camp.spots.is_empty() {
        let hover = s.walls.as_ref().and_then(|w| w.hover_at_rest(template.id));
        at.origin[2] += hover.unwrap_or(0.0);
    }
    Some(make(
        s,
        &template,
        stats,
        (at, heading),
        Some(i),
        patrol,
        now,
    ))
}

/// Make a creature of a template at a place, facing a way, for a camp if it has one.
pub fn make<S: Store>(
    s: &mut Server<S>,
    template: &yalain_content::Template,
    stats: yalain_content::CreatureStats,
    (at, heading): (StoredPosition, u8),
    camp: Option<usize>,
    patrol: Option<crate::ai::Patrol>,
    now: f64,
) -> u64 {
    let max_health = if s.rng.chance(HEALTH_EXACT) {
        stats.max_health
    } else {
        #[allow(clippy::cast_precision_loss)]
        let h = dereth_primitives::num::to_i32(
            (stats.max_health as f32 * s.rng.between(1.0 - HEALTH_SPREAD, 1.0 + HEALTH_SPREAD))
                .round(),
        );
        h.max(1)
    };
    let id = s.things.new_id();
    let mut t = Thing::of(id, template, Kind::Creature);
    t.at = Some(at);
    t.heading_motion = u32::from(heading) << 24 | crate::world::IDLE_MOTION;
    t.max_health = max_health;
    t.health = max_health;
    t.max_vigor = stats.max_vigor;
    t.vigor = stats.max_vigor;
    t.fresh = true;
    let next_idle = crate::ai::first_idle(s, &stats, now);
    t.creature = Some(Creature {
        stats,
        camp,
        home: at,
        mode: Mode::Idle,
        target: None,
        next_attack: now,
        next_wander: now + f64::from(s.rng.between(5.0, 20.0)),
        next_idle,
        moving_to: None,
        wander_to: None,
        blocked: 0,
        way: crate::ai::Way::default(),
        damage_by: Vec::new(),
        taken_by: Vec::new(),
        unreached: Vec::new(),
        ranged: false,
        closing: None,
        barred: Vec::new(),
        next_look: now + f64::from(s.rng.unit()),
        lost_since: None,
        joining: None,
        opening: None,
        harm: Vec::new(),
        taunts: Vec::new(),
        considered: f64::NEG_INFINITY,
        target_since: 0.0,
        kin_helped: None,
        weapons: Vec::new(),
        weapon_count: 1,
        patrol,
        actions: 0,
    });
    s.things.insert(t);
    crate::arms::arm(s, id);
    id
}

/// How near two places must be, in meters, to be one spot.
const ONE_SPOT: f32 = 0.5;

/// A camp's spots no creature or item it keeps now holds, by their index; all of them when each is
/// held. A spot holds one (2017: of 10,731 pairs of one session's creatures seen at one place, 8 were
/// alive together).
fn free_spots<S: Store>(s: &Server<S>, camp: &yalain_content::Camp, i: usize) -> Vec<usize> {
    let held: Vec<StoredPosition> = s
        .camps
        .get(i)
        .into_iter()
        .flat_map(|state| &state.alive)
        .filter_map(|id| s.things.get(*id))
        .filter_map(|t| t.creature.as_ref().map(|c| c.home).or(t.at))
        .collect();
    let free: Vec<usize> = (0..camp.spots.len())
        .filter(|k| {
            let (cell, p) = camp.spots[*k];
            !held.iter().any(|h| {
                let (dx, dy) = (h.origin[0] - p[0], h.origin[1] - p[1]);
                h.cell >> 16 == cell >> 16
                    && (dx * dx + dy * dy).sqrt() <= ONE_SPOT
                    && (h.origin[2] - p[2]).abs() <= 2.0
            })
        })
        .collect();
    if free.is_empty() {
        (0..camp.spots.len()).collect()
    } else {
        free
    }
}

/// Where a camp's next creature or item appears, facing a random way, and that heading.
fn place<S: Store>(
    s: &mut Server<S>,
    camp: &yalain_content::Camp,
    i: usize,
) -> (StoredPosition, u8) {
    let at = if camp.spots.is_empty() {
        // A point drawn uniformly inside the camp's circle.
        let r = camp.radius * s.rng.unit().sqrt();
        let a = s.rng.between(0.0, std::f32::consts::TAU);
        let (cell, x, y) = outdoor(
            camp.cell >> 16,
            camp.origin[0] + r * math::cosf(a),
            camp.origin[1] + r * math::sinf(a),
        );
        let z = s
            .ground
            .as_ref()
            .and_then(|g| g.height(cell, [x, y, camp.origin[2]]))
            .unwrap_or(camp.origin[2]);
        StoredPosition {
            cell,
            origin: [x, y, z],
            rotation: [1.0, 0.0, 0.0, 0.0],
        }
    } else {
        let free = free_spots(s, camp, i);
        let (cell, p) = camp.spots[free[s.rng.below(free.len())]];
        StoredPosition {
            cell,
            origin: p,
            rotation: [1.0, 0.0, 0.0, 0.0],
        }
    };
    // Facing a random way.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let heading = (s.rng.unit() * 256.0) as u8;
    let at = StoredPosition {
        rotation: crate::world::rotation_of_heading(heading),
        ..at
    };
    (at, heading)
}

/// Is a template an item (no creature: one with a value of its own)?
#[must_use]
pub fn is_item(props: &yalain_content::Props) -> bool {
    props.int("Item_Value").is_some()
}

/// Lay an item of a template where a camp keeps it. Returns its id.
fn lay<S: Store>(s: &mut Server<S>, template: u32, at: StoredPosition) -> Option<u64> {
    let value = crate::inventory::item_value(s, template);
    let id = crate::inventory::make_item(s, template, 1, value)?;
    if let Some(t) = s.things.get_mut(id) {
        t.at = Some(at);
    }
    Some(id)
}

/// A camp's creature has died: a replacement is due after the camp's period.
pub fn died<S: Store>(s: &mut Server<S>, camp: usize, id: u64, now: f64) {
    let regen = s.spawns.camps.get(camp).map_or(0.0, |c| c.regen);
    if let Some(state) = s.camps.get_mut(camp) {
        state.alive.retain(|x| *x != id);
        state.due.push(now + regen);
    }
}
