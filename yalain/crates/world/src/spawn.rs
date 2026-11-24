//! The world's step: camps keep their creatures alive, creatures act, corpses go.
//!
//! **Camps.** Each camp of the spawn table keeps its population alive: at the first step every camp
//! is filled; when one of its creatures dies, a replacement appears the camp's period after its body
//! leaves the world (at the operator's respawn rate: [`crate::rates`]).
//! Outdoors a creature appears at a point drawn inside the camp's radius (on the ground, when the
//! host knows the land); indoors at one of the camp's spots. A creature's greatest health is its
//! template's, varied a little each time one appears.
//!
//! **Items.** A camp whose template is an item (no creature, a template with a value) keeps it
//! lying at the camp's place: once it is taken (picked up, or gone), a new one lies there after the
//! camp's period (2017: quest items lying alone came back 20.0-20.1 s after their taking, 9 of 9
//! timed; keys 30.0-30.1 s, 6 of 6; a dungeon's mushrooms 181-193 s, 6 of 6).
//!
//! **Chests.** A camp whose template is a chest's keeps one standing at its place, as an effect
//! places one ([`crate::placing`]): anyone may open it. Once it is gone (a chest of a kind that goes
//! once closed, closed after its opening) a new one stands there after the camp's period: a rare
//! prospector's corpse where creatures roam, with a long period, comes back now and then.
//!
//! **Summons.** A camp never makes a summoned creature (a template with a pet's AI or a pet's NPC
//! table): those were players' pets and conjurings, seen beside their owners and never made by a
//! camp (2017: every one of the 417 creatures given a player as its pet was of such a template).
//!
//! **Master camps.** A camp may keep its creatures for a master camp (a portal beacon's guards for
//! the beacon): it keeps none while its master keeps none, and as the master's last dies its
//! creatures leave the world at once, with no fall and no corpse; it fills again, all at once, when
//! the master makes another. An outpost's guards go with their beacon, as players' guides of the
//! time tell it.
//!
//! **Moving.** A camp's movement settings stand over its creatures' templates' (a camp may make
//! them stationary, free of their home, wanderers or not, of another perception), and a camp that
//! names a route sets each creature it makes off along it, toward the camp's starting waypoint
//! and the camp's way round ([`crate::ai`]).
//!
//! **Looked at only with something to do.** After the first step a camp is looked at only when it
//! may have something to do: a replacement of its has come due, a creature or item it keeps has
//! gone or been taken up from the ground, it is new to the world (a reload's), its event has
//! begun, or its master camp has made a creature. An idle world's step looks at no camp, however many it has; what a camp makes, and when,
//! is as it would be if every camp were looked at every step.
//!
//! **Appearing.** A new creature is made for every client whose view holds its place, with its
//! entry effect, and its place is told once more on its stream, where it stands. It holds the
//! effects the world's rows give its template from that moment, at its level's spellcraft, for as
//! long as it stands ([`holds_its_effects`]); so does a creature a death or an effect makes.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

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

/// A camp's state: its creatures alive, when each replacement is due, and the characters that
/// harmed its last creature to die (a pet's harm counted as its owner's; not kept across a
/// restart).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CampState {
    pub alive: Vec<u64>,
    pub due: Vec<f64>,
    pub filled: bool,
    pub fought: Vec<u64>,
}

impl CampState {
    /// When its earliest replacement is due, if one is.
    #[must_use]
    pub fn earliest(&self) -> Option<f64> {
        self.due.iter().copied().min_by(f64::total_cmp)
    }
}

/// Which camps the step looks at: those that may have something to do.
#[derive(Debug, Clone, Default)]
pub struct CampWatch {
    /// How many of the spawn table's camps it knows of: any beyond are new, and looked at.
    known: usize,
    /// Camps with a replacement due, by when.
    due: BinaryHeap<Reverse<DueAt>>,
    /// Camps to look at at the next step: new, their event begun, or one of their things gone.
    pending: BTreeSet<usize>,
    /// The camp that keeps each thing a camp made.
    kept_by: HashMap<u64, usize>,
    /// How many camps the last step looked at.
    pub looked_at: usize,
}

/// A camp's replacement due at a time (world seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
struct DueAt(f64, usize);

impl Eq for DueAt {}

impl PartialOrd for DueAt {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DueAt {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0).then(self.1.cmp(&other.1))
    }
}

impl CampWatch {
    /// Watch a spawn table's camps afresh, from their states: each is looked at at the next step.
    #[must_use]
    pub fn of(states: &[CampState]) -> Self {
        let mut w = Self {
            known: states.len(),
            pending: (0..states.len()).collect(),
            ..Self::default()
        };
        for (i, st) in states.iter().enumerate() {
            for id in &st.alive {
                w.kept_by.insert(*id, i);
            }
            w.due_at(i, st);
        }
        w
    }

    /// A camp is looked at at the next step.
    pub fn look_at(&mut self, camp: usize) {
        self.pending.insert(camp);
    }

    /// A camp is looked at when its earliest replacement is due, if it has one.
    fn due_at(&mut self, camp: usize, st: &CampState) {
        if let Some(t) = st.earliest() {
            self.due.push(Reverse(DueAt(t, camp)));
        }
    }

    /// The camps to look at now, in order: new ones, those pending, those whose replacement is due
    /// and those that kept a thing gone or taken up since.
    fn now(&mut self, camps: usize, left: &[u64], now: f64) -> Vec<usize> {
        self.pending.extend(self.known..camps);
        self.known = camps;
        let mut look = std::mem::take(&mut self.pending);
        while let Some(Reverse(DueAt(t, i))) = self.due.peek().copied() {
            if t > now {
                break;
            }
            self.due.pop();
            look.insert(i);
        }
        for id in left {
            if let Some(i) = self.kept_by.remove(id) {
                look.insert(i);
            }
        }
        look.into_iter().filter(|i| *i < camps).collect()
    }
}

/// A new creature whose template the world's rows align ([`CreatureRows::aligned`]) holds the
/// kingdom its template names (its faction) from its first moment, and shows it, until the row's
/// time has gone: players of that kingdom may not harm it meanwhile ([`crate::combat`]). Called
/// before it appears, so it appears holding it.
///
/// [`CreatureRows::aligned`]: yalain_content::creature_rows::CreatureRows::aligned
pub(crate) fn aligns<S: Store>(s: &mut Server<S>, id: u64, clock: Clock) {
    let Some(template) = s.things.get(id).map(|t| t.template) else {
        return;
    };
    let Some(&seconds) = s.creature_rows.aligned.get(&template) else {
        return;
    };
    let Some(kingdom) = s.catalog.get(template).and_then(|t| t.facts.faction) else {
        return;
    };
    if let Some(t) = s.things.get_mut(id) {
        t.kingdom = Some(kingdom);
    }
    s.after(clock, seconds, crate::Timer::AlignmentEnds { creature: id });
}

/// An aligned creature's time has gone: it holds no kingdom, and every client that knows it is told
/// it anew, as a client's own ask for it again is answered.
pub(crate) fn alignment_ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    if t.kingdom.take().is_none() {
        return;
    }
    let m = auberean_protocol::ServerMessage::CreateObject(crate::things::create(t));
    presence::to_thing_knowers(s, net, id, yalain_net::BlobKind::UNORDERED, &m);
}

/// A creature holds the effects the world's rows give its template from its first moment
/// ([`yalain_content::creature_rows::CreatureEffect`]), each at the spellcraft its row gives the
/// creature's level, for as long as it stands, shown at once. An effect the world's start never
/// read is left off.
pub(crate) fn holds_its_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    clock: Clock,
) {
    let Some(t) = s.things.get(id) else {
        return;
    };
    let (template, level) = (t.template, t.creature.as_ref().map_or(0, |c| c.stats.level));
    #[allow(clippy::cast_precision_loss)]
    let held: Vec<(u32, f32)> = s
        .creature_rows
        .effects_of(template)
        .map(|e| {
            let at = crate::effect_defs::Scale(e.spellcraft.clone()).at(level as f32);
            (e.effect, at.unwrap_or(1.0))
        })
        .collect();
    for (state, spellcraft) in held {
        let Some(def) = s.effect_table.get(state).cloned() else {
            continue;
        };
        let cast = crate::effects::Cast {
            caster: id,
            spellcraft,
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
}

/// The side of a landblock and of a land cell, in meters.
pub const BLOCK_SIDE: f32 = auberean_physics::rules::BLOCK_LENGTH;
pub const CELL_SIDE: f32 = auberean_physics::land::CELL_LENGTH;
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
    use crate::step_timer::Stage;
    let now = clock.local.0;
    let left = s.things.take_left();
    if s.spawns.camps.len() < s.camp_watch.known {
        // The table shrank under it: it watches the camps afresh.
        s.camp_watch = CampWatch::of(&s.camps);
    }
    let look = s.camp_watch.now(s.spawns.camps.len(), &left, now);
    s.camp_watch.looked_at = look.len();
    for i in look {
        fill(s, net, i, clock);
        // One that lacks creatures is looked at again when its next is due; one that lacks none,
        // when one goes.
        let next = s
            .camps
            .get(i)
            .filter(|st| st.filled && st.alive.len() < s.spawns.camps[i].population as usize)
            .and_then(CampState::earliest);
        if let Some(t) = next {
            s.camp_watch.due.push(Reverse(DueAt(t, i)));
        }
    }
    s.step_timer.lap(Stage::Camps);
    crate::ai::step(s, net, clock);
    s.step_timer.lap(Stage::Creatures);
    crate::reload::retire_rested(s, net);
    crate::walkers::step(s, net, clock);
    crate::pets::step(s, net, clock);
    crate::death::expire_corpses(s, net, now);
    crate::mining::expire(s, net, now);
    s.night = s.is_night(net.game_time(clock.local));
    crate::regen::step(s, net, clock);
    crate::talk::step(s, net, clock);
    crate::world::pull_back_due(s, net, clock);
    s.step_timer.lap(Stage::Step);
}

/// Make the creatures a camp lacks that are due.
fn fill<S: Store>(s: &mut Server<S>, net: &mut ServerNet, i: usize, clock: Clock) {
    if !crate::events::camp_stands(s, i) || !master_keeps(s, i) {
        return;
    }
    let now = clock.local.0;
    // Its items taken from where they lay are replaced after its period, as its dead creatures are.
    let regen = period(s, i);
    let Some(state) = s.camps.get_mut(i) else {
        return;
    };
    let population = s.spawns.camps[i].population as usize;
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
    let mut made = false;
    for _ in 0..make {
        if let Some(id) = spawn(s, i, now) {
            s.camps[i].alive.push(id);
            s.camp_watch.kept_by.insert(id, i);
            aligns(s, id, clock);
            presence::thing_appears(s, net, id);
            holds_its_effects(s, net, id, clock);
            // A creature just made has its place told once on its stream, where it was made (2017:
            // 2,831 of 3,335 creatures just made, within 0.1 s of their making; 8,366 of 9,239
            // coming into view had none).
            if s.things.get(id).is_some_and(|t| t.kind == Kind::Creature) {
                crate::ai::stands(s, net, id, clock);
            }
            made = true;
        }
    }
    // The camps it is master of fill again once it keeps a creature.
    if made {
        for j in serving(s, i) {
            s.camp_watch.look_at(j);
        }
    }
}

/// The camps (by their places in the table) that keep their creatures for this one.
fn serving<S: Store>(s: &Server<S>, master: usize) -> Vec<usize> {
    if s.spawns.masters.is_empty() {
        return Vec::new();
    }
    let Some(id) = s.spawns.camps.get(master).map(|c| c.id.as_str()) else {
        return Vec::new();
    };
    s.spawns
        .serving(id)
        .filter_map(|c| s.spawns.camps.iter().position(|x| x.id == c))
        .collect()
}

/// May a camp keep creatures by its master: it has none, or its master keeps one now.
fn master_keeps<S: Store>(s: &Server<S>, i: usize) -> bool {
    if s.spawns.masters.is_empty() {
        return true;
    }
    let Some(master) = s
        .spawns
        .camps
        .get(i)
        .and_then(|c| s.spawns.master_of(&c.id))
    else {
        return true;
    };
    s.spawns
        .camps
        .iter()
        .position(|c| c.id == master)
        .is_none_or(|m| s.camps.get(m).is_some_and(|st| !st.alive.is_empty()))
}

/// A camp's creature has died: if it was the last the camp kept, the creatures of the camps it is
/// master of leave the world, with no fall and no corpse, until it makes another.
pub fn master_lost<S: Store>(s: &mut Server<S>, net: &mut ServerNet, master: usize) {
    if s.camps.get(master).is_none_or(|st| !st.alive.is_empty()) {
        return;
    }
    for j in serving(s, master) {
        let Some(state) = s.camps.get_mut(j) else {
            continue;
        };
        let gone = std::mem::take(state);
        for id in gone.alive {
            crate::death::vanish(s, net, id);
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
        if s.chests.packages.contains(&template.description.package) {
            let (at, _) = place(s, &camp, i);
            return Some(stand(s, &template, at));
        }
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
    } else if let Some(w) = s.walls.as_mut() {
        // In the cell that holds it, as anything put somewhere is (2017: 13,145 of 13,172 creatures
        // made inside were told in the cell holding them).
        at = w.holding_cell(Some((template.id, stats.scale)), &at);
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
    // A camp that names its creatures has them told by that name, examined by it and their
    // corpses named after it.
    if let Some(name) = camp
        .and_then(|i| s.spawns.camps.get(i))
        .and_then(|c| c.name)
    {
        t.description.name = Some(name);
        t.facts.name = Some(name);
    }
    t.at = Some(at);
    t.heading_motion = u32::from(heading) << 24 | crate::world::IDLE_MOTION;
    t.max_health = max_health;
    t.health = max_health;
    t.max_vigor = stats.max_vigor;
    t.vigor = stats.max_vigor;
    t.fresh = true;
    let next_idle = crate::ai::first_idle(s, &stats, template.id, now);
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
        sight: None,
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
        in_sight: Vec::new(),
        victory_until: None,
        idle_until: f64::NEG_INFINITY,
        after_idle: None,
        last_blow: f64::NEG_INFINITY,
        next_vigor_step: f64::NEG_INFINITY,
        home_after: f64::NEG_INFINITY,
        lost_interest: Vec::new(),
    });
    s.things.insert(t);
    crate::arms::arm(s, id);
    crate::defenses::made(s, id, template.id, now);
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

/// Stand a chest of a template where a camp keeps it, no one's. Returns its id.
fn stand<S: Store>(
    s: &mut Server<S>,
    template: &yalain_content::Template,
    at: StoredPosition,
) -> u64 {
    let id = s.things.new_id();
    let mut t = Thing::of(id, template, Kind::Placed);
    t.at = Some(at);
    t.fresh = true;
    s.things.insert(t);
    id
}

/// A camp's period: how long after a body leaves or a taking its replacement is due, at the
/// operator's respawn rate ([`crate::rates`]).
fn period<S: Store>(s: &Server<S>, camp: usize) -> f64 {
    let regen = s.spawns.camps.get(camp).map_or(0.0, |c| c.regen);
    crate::rates::period(s.cfg.rates.respawn, regen)
}

/// A camp's creature has died: a replacement is due the camp's period after `leaves`, when its
/// body leaves the world.
pub fn died<S: Store>(s: &mut Server<S>, camp: usize, id: u64, leaves: f64) {
    let regen = period(s, camp);
    if let Some(state) = s.camps.get_mut(camp) {
        state.alive.retain(|x| *x != id);
        state.due.push(leaves + regen);
        if let Some(t) = state.earliest() {
            s.camp_watch.due.push(Reverse(DueAt(t, camp)));
        }
    }
}
