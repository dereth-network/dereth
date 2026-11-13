//! Plants: a seed planted in its garden grows into a plant its planter harvests when it is ripe; a
//! dye plant its planter also waters, fertilizes and may pollinate.
//!
//! **Planting.** A seed may be used only near its garden (its template's place and radius). Its
//! use plays the planting behavior with the seed in hand; when that has played out (2.0 s) the
//! planter is told of a new pet (no time left in the world, the plants' class and flags, no icon,
//! the plant's name), the plant stands two meters straight ahead of the planter facing as it faces,
//! the seed is spent and the use answered "You use {the} %1.". A species whose row plants several
//! from one seed has each told and standing two meters from the planter, spread evenly across
//! [`SPREAD`] either side of straight ahead, all facing as the planter faces; the quests the seed's
//! template names for its use move as it is planted.
//!
//! **A species of its own.** A world's row may give a species its own time to ripen, its own lines
//! (a string table holding the dye plants' lines for a plant not ripe and one its user did not plant
//! under the same ids, with its own countdown, ripe and harvest lines), and no needs: such a plant
//! is never watered, fertilized or withered, keeps its own look until it is harvested, and shows
//! only its countdown, or that it is ripe, when examined.
//!
//! **Growing.** A dye plant thinks every 10 to 20 seconds. At each thought its look moves on to the
//! stage its age has reached (a stage each 45 s: two growing looks, then the ripe look, the look of
//! its species' plant item), shown to everyone who sees it; and while it is not yet ripe, its need
//! of water and its need of fertilizer may each grow: by nothing (three times in four), by one
//! level or by two. A dye plant is ripe 180 s after it was planted; its needs no longer grow.
//!
//! **Its needs** run from "Far too much!" through "Too much", "None needed" (twice), "Needed",
//! "Badly needed", "Urgently needed" and "Desperately needed" to "Death imminent!"; a new plant
//! badly needs both. A watering can used on it lowers its need of water by one level, a bag of
//! fertilizer its need of fertilizer. A need that grows past "Death imminent!", or a plant cared
//! for past "Far too much!", withers the plant: its planter is told why, and it is gone.
//!
//! **A species' own care.** A world's row may name the items that water and fertilize its plants,
//! in place of the can and the bag; it may plant its seed in a garden instead (below); and it may
//! give a plant that needs care lines of its own. Such a plant keeps its own look as it grows; its
//! withering, a harvest too early and a harvest by another player are told in its own lines where
//! its row names them; and examined while it grows it shows its stage's line and how it fares, in
//! place of its needs and pollen: healthy (no need above "None needed", none below it), or each of
//! dry (water "Needed" or more), overwatered ("Too much" or less), hungry and overfed (the same of
//! fertilizer) that holds, in that order.
//!
//! **A garden.** A seed whose species names a garden is not planted by its use: it is given to an
//! object of the garden's template, which keeps the seed (one of a stack) and says its planting
//! line, and the plant stands where the garden stands, facing as it faces, its giver's plant. A
//! garden holds one plant at a time: while its plant stands, a seed given to it is handed back
//! with its busy line. A quest step the give triggers runs instead.
//!
//! **Pollen** used on a plant brushes onto its leaves ("You brush the pollen on to the plants'
//! leaves.", after the use's answer); a plant already pollinated does not take another (the
//! breeze blows it away). Examining a plant shows its needs, its pollen and the time until it is
//! ripe, or that it is ripe.
//!
//! **Only its planter cares for it.** A can, a bag or a pollen used on a plant by anyone else plays
//! its use as ever, and when that has played out the use is answered with the item kept and unworn,
//! and the player told, as at a harvest, that it did not plant this plant and leaves it alone: its
//! needs and its pollen stay as they were.
//!
//! **Harvesting.** A player uses the plant (walking to it first when it is farther than 4 m): it
//! plays the plants' behavior, and when that has played out the use is answered, with a text: a
//! plant not yet ripe "is not mature yet" and stays; one the player did not plant is left alone; a
//! ripe one is taken off its planter's pets and leaves the world, and gives its plant item and a
//! pollen of its kind, or, pollinated with a pollen whose cross is known, seeds of the cross's
//! species. What it gives comes into the planter's hands as an item taken does, moving the quests
//! the taking moves.
//!
//! **Its end.** A plant leaves the world with its planter, as a pet does.

use std::collections::{BTreeMap, HashMap};

use auberean_protocol::physics::ObjectEvent;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::{BehaviorParams, IconLayer, PartGroupDesc, VisualDesc};
use auberean_protocol::ServerMessage;
use yalain_content::talk::{How, Words};
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::Use;
use crate::things::{Kind, Thing};
use crate::{progress, Clock, Server, Timer};

pub use yalain_content::plants::{
    Conditions, Cross, DyePlants, Garden, Species, Texts, FERTILIZER, RIPE_SECONDS, WATERING_CAN,
};

/// The kinds of dye plant row a world has none of: it has no species or crosses of such a kind.
#[must_use]
pub fn kinds_missing(rows: &DyePlants) -> Vec<&'static str> {
    [
        ("plant", rows.species.is_empty()),
        ("plant_cross", rows.crosses.is_empty()),
    ]
    .into_iter()
    .filter_map(|(kind, none)| none.then_some(kind))
    .collect()
}

/// The planting behavior (a seed's use behavior), and how long it plays before the plant stands,
/// in seconds.
pub const PLANTING_BEHAVIOR: u32 = 0x4100_0021;
pub const PLANTING_SECONDS: f64 = 2.0;

/// The bits of a harvest's behavior: the behavior, the words of flags `0x20` and `0x100` (each 1),
/// the context.
pub const HARVEST_BITS: u32 = BehaviorParams::BEHAVIOR | 0x20 | 0x100 | BehaviorParams::CONTEXT;

/// A harvest's answer's control flags.
pub const HARVEST_FLAGS: u32 = 956;

/// How near its plant a harvester stands, in meters (the watering can's reach).
pub const REACH: f32 = 4.0;

/// How far ahead of its planter a plant stands, in meters.
pub const AHEAD: f32 = 2.0;

/// How far either side of straight ahead the plants one seed plants spread, in degrees: the first
/// and the last stand so far off the planter's heading, the others evenly between (inferred: 2017
/// recorded one plant a seed, and no seed that plants several).
pub const SPREAD: f32 = 45.0;

/// A plant's pet data: its class and flags.
pub const PET_CLASS: u32 = 0x0400_0000;
pub const PET_FLAGS: u32 = 132_272;

/// How long each look stage lasts and how many stages there are, in seconds since it was planted.
pub const STAGE_SECONDS: f64 = 45.0;
pub const STAGES: u8 = 3;

/// The time between a plant's thoughts, in seconds: at least, at most.
pub const THINK_SECONDS: (f32, f32) = (10.0, 20.0);

/// The chances a need grows by one level and by two at a thought.
pub const GROWS_ONE: f32 = 0.14;
pub const GROWS_TWO: f32 = 0.12;

/// The need levels: far too much, too much, none (twice), needed, badly, urgently, desperately
/// needed, death imminent; a new plant's.
pub const NEED_LEVELS: [u32; 9] = [
    0x8863_162D,
    0x886C_162D,
    0x886D_162D,
    0x886E_162D,
    0x886F_162D,
    0x8868_162D,
    0x8869_162D,
    0x886A_162D,
    0x886B_162D,
];
pub const NEW_NEED: i32 = 5;
const LAST_NEED: i32 = 8;

/// The setups of the growing looks' part group, by stage.
pub const GROWING_SETUPS: [u32; 2] = [0x0400_0693, 0x0400_0694];
/// The part group the growing looks change.
pub const PLANT_PART: u32 = 0x4000_0001;

/// The plants' texts (string ids) in their table, and the planter's name in them.
pub mod text {
    pub const TABLE: u32 = 0x2500_1B4D;
    pub const PLAYER: u32 = 0x15B2_0D98;
    pub const BRUSHED: u32 = 0x591B_7041;
    pub const BLOWN_AWAY: u32 = 0xF078_99F5;
    pub const NOT_MATURE: u32 = 0x4176_2D1C;
    pub const NOT_YOURS: u32 = 0x4F0C_A5F2;
    pub const RIPE_WITH_POLLEN: u32 = 0x39D2_1C3D;
    pub const RIPE: u32 = 0xE85F_039D;
    /// One seed, two seeds, three seeds.
    pub const SEEDS: [u32; 3] = [0x5037_D7A1, 0x0837_D7A2, 0x4837_D7A2];
    /// Withered: for want of water, of fertilizer; from too much water, too much fertilizer.
    pub const NO_WATER: u32 = 0x22E7_E8CE;
    pub const NO_FERTILIZER: u32 = 0x7C23_61E8;
    pub const TOO_MUCH_WATER: u32 = 0xAFA0_82C9;
    pub const TOO_MUCH_FERTILIZER: u32 = 0x6478_13ED;
    /// The examination's lines.
    pub const WATER: u32 = 0x74AF_5CCF;
    pub const FERTILIZER: u32 = 0x56E6_CB7E;
    pub const POLLEN: u32 = 0x9FDE_DDEA;
    pub const NO_POLLEN: u32 = 0x819B_71D6;
    pub const MATURITY_IN: u32 = 0x96B0_A216;
    pub const MATURE: u32 = 0x3952_1DBD;
    /// How a plant's text is shown.
    pub const SHOWN_AS: u32 = 8;
}

/// One plant in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct Plant {
    pub id: u64,
    /// Its planter (a character id).
    pub owner: u64,
    pub species: Species,
    /// When it was planted (world seconds), and when it is ripe on the game clock.
    pub planted: f64,
    pub ripe_game: f64,
    /// The look stages shown.
    pub stage: u8,
    /// Its needs of water and of fertilizer (levels of [`NEED_LEVELS`]).
    pub water: i32,
    pub fertilizer: i32,
    /// The pollen brushed on it.
    pub pollen: Option<u32>,
    /// The garden it stands in (its place in the world's list of placed objects), if any.
    pub garden: Option<usize>,
}

/// Each species' ripe look, by its plant item.
pub type Looks = HashMap<u32, VisualDesc>;

/// The plants of the world, its species and crosses (the world's rows), and each species' ripe
/// look.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plants {
    pub by_id: BTreeMap<u64, Plant>,
    pub kinds: DyePlants,
    pub looks: Looks,
}

/// A look as an object's visual description in a message carries it, from the install's
/// description (its own id left out).
#[must_use]
pub fn look_of(v: &auberean_assets::visual::VisualDesc) -> VisualDesc {
    let group = |g: &auberean_assets::visual::PartGroup| PartGroupDesc {
        bits: g.flags & 0x7F,
        key: g.key.unwrap_or(0),
        parent_key: g.parent_key.unwrap_or(0),
        connection_point: g.connection_point.unwrap_or(0),
        setup: g.setup.map_or(0, |d| d.0),
        anim_map: g.anim_map.map_or(0, |d| d.0),
        appearance: Table::from_inserted(
            SMALL,
            g.appearance_keys
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, values)| (k, Table::from_inserted(SMALL, values)))
                .collect::<Vec<_>>(),
        ),
        fx_table: g.fx_table.map_or(0, |d| d.0),
        ..PartGroupDesc::default()
    };
    VisualDesc {
        bits: v.flags & (VisualDesc::ICON | VisualDesc::GLOBAL | VisualDesc::PART_GROUPS),
        icon: v
            .icon
            .iter()
            .flatten()
            .map(|l| IconLayer(l.layer, l.picture.0, l.shift_color.0))
            .collect(),
        global: v.global.as_ref().map(group).unwrap_or_default(),
        part_groups: Table::from_inserted(
            SMALL,
            v.part_groups
                .iter()
                .flatten()
                .map(|(k, g)| (*k, group(g)))
                .collect::<Vec<_>>(),
        ),
        ..VisualDesc::default()
    }
}

/// A growing look: the plant's part group built on the stage's setup.
#[must_use]
pub fn growing_look(stage: u8) -> VisualDesc {
    let setup = GROWING_SETUPS[usize::from(stage.clamp(1, 2)) - 1];
    VisualDesc {
        bits: VisualDesc::PART_GROUPS,
        part_groups: Table::from_inserted(
            SMALL,
            vec![(
                PLANT_PART,
                PartGroupDesc {
                    bits: PartGroupDesc::KEY | PartGroupDesc::SETUP,
                    key: PLANT_PART,
                    setup,
                    ..PartGroupDesc::default()
                },
            )],
        ),
        ..VisualDesc::default()
    }
}

/// The string table of a species' lines, and the ids of its countdown label (none: no countdown),
/// its ripe line and its harvest's line (where its harvest gives no pollen and no seeds).
fn lines(species: &Species) -> (u32, Option<u32>, u32, Option<u32>) {
    species.texts.map_or(
        (text::TABLE, Some(text::MATURITY_IN), text::MATURE, None),
        |t| (t.table, t.maturity_in, t.mature, Some(t.harvest)),
    )
}

/// A species' line in its own table for a moment its row names, else the dye plants' id there.
fn own_line(species: &Species, of: impl Fn(&Texts) -> Option<u32>, dye: u32) -> u32 {
    species.texts.as_ref().and_then(of).unwrap_or(dye)
}

/// The dye plants' lines for an examination: their table, the countdown's label and the ripe line.
pub const DYE_LINES: (u32, Option<u32>, u32) = (text::TABLE, Some(text::MATURITY_IN), text::MATURE);

/// A text of a species' table naming a player.
fn told(table: u32, id: u32, name: &str) -> ServerMessage {
    let t = progress::text_with(
        (id, table),
        vec![progress::nested(
            text::PLAYER,
            crate::profile::literal(name),
        )],
    );
    crate::chat::show_text(text::SHOWN_AS, t)
}

/// A player's name as plants' texts give it, and its connection.
fn named<S: Store>(s: &Server<S>, character: u64) -> Option<(ConnId, String)> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(c, p)| (*c, crate::player::world_name(&p.name, p.sex)))
}

/// How far off its planter's heading the `k`th of `n` plants one seed plants stands, in degrees
/// (counterclockwise): straight ahead for one, else evenly from [`SPREAD`] one side to the other.
#[must_use]
pub fn off_heading(k: u32, n: u32) -> f32 {
    if n <= 1 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let share = k as f32 / (n - 1) as f32;
    SPREAD - 2.0 * SPREAD * share
}

/// The place two meters from a planter at an angle off its heading (degrees, counterclockwise),
/// facing as it faces, on the ground where the ground is known.
fn ahead<S: Store>(s: &Server<S>, at: &StoredPosition, off: f32) -> StoredPosition {
    let [w, x, y, z] = at.rotation;
    let (fx, fy) = (2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z));
    let (sin, cos) = dereth_primitives::num::math::sin_cosf(off.to_radians());
    let (fx, fy) = (fx * cos - fy * sin, fx * sin + fy * cos);
    let (px, py) = (at.origin[0] + AHEAD * fx, at.origin[1] + AHEAD * fy);
    let (cell, px, py) = if crate::presence::indoors(at.cell) {
        (at.cell, px, py)
    } else {
        crate::spawn::outdoor(at.cell >> 16, px, py)
    };
    let pz = s
        .ground
        .as_ref()
        .and_then(|g| g.height(cell, [px, py, at.origin[2]]))
        .unwrap_or(at.origin[2]);
    StoredPosition {
        cell,
        origin: [px, py, pz],
        rotation: at.rotation,
    }
}

/// Is an item template a seed one of the world's species plants where it is used?
#[must_use]
pub fn is_seed<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.plants.kinds.sown_by(template).is_some()
}

/// A seed's planting has played out: the planter is told of each new plant, which stands ahead of
/// it. Returns the plants, none for an item that is no seed.
pub fn sow<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    seed: u32,
    clock: Clock,
) -> Vec<u64> {
    let Some(species) = s.plants.kinds.sown_by(seed) else {
        return Vec::new();
    };
    let Some(from) = s.players.get(&world).map(|p| p.position) else {
        return Vec::new();
    };
    (0..species.count)
        .filter_map(|k| {
            let at = ahead(s, &from, off_heading(k, species.count));
            plant(s, net, world, species, (at, None), clock)
        })
        .collect()
}

/// One plant of a species stands for a planter at a place, in a garden or not.
fn plant<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    species: Species,
    (at, garden): (StoredPosition, Option<usize>),
    clock: Clock,
) -> Option<u64> {
    let template = s.catalog.get(species.plant)?.clone();
    let owner = s.players.get(&world).map(|p| p.character)?;
    let id = s.things.new_id();
    let name = template.description.name.unwrap_or_default();
    progress::private(
        net,
        world,
        &crate::pets::added(id, progress::text(name), 0, PET_CLASS, PET_FLAGS),
    );
    let mut t = Thing::of(id, &template, Kind::Plant);
    t.at = Some(at);
    let health = template
        .props
        .int("GameplayStatistics_MaxHealth")
        .and_then(|h| i32::try_from(h).ok())
        .unwrap_or(1);
    let vigor = template
        .props
        .int("GameplayStatistics_MaxVigor")
        .and_then(|h| i32::try_from(h).ok())
        .unwrap_or(0);
    (t.health, t.max_health, t.vigor, t.max_vigor) = (health, health, vigor, vigor);
    t.summoner = Some(owner);
    s.things.insert(t);
    let now = clock.local.0;
    s.plants.by_id.insert(
        id,
        Plant {
            id,
            owner,
            species,
            planted: now,
            ripe_game: s.game_now + species.ripe,
            stage: 0,
            water: NEW_NEED,
            fertilizer: NEW_NEED,
            pollen: None,
            garden,
        },
    );
    crate::presence::thing_appears(s, net, id);
    // One that needs no care has nothing to think of.
    if species.tended {
        let wait = f64::from(s.rng.between(THINK_SECONDS.0, THINK_SECONDS.1));
        s.after(clock, wait, Timer::PlantThinks { plant: id });
    }
    Some(id)
}

/// Is a plant ripe now (`now` in world seconds)?
fn ripe_at(p: &Plant, now: f64) -> bool {
    now - p.planted >= p.species.ripe
}

/// How much a need grows at a thought, by a roll in `[0, 1)`.
#[must_use]
pub fn growth(roll: f32) -> i32 {
    if roll < GROWS_TWO {
        2
    } else if roll < GROWS_TWO + GROWS_ONE {
        1
    } else {
        0
    }
}

/// The look stage a plant of this age has reached.
#[must_use]
pub fn stage_at(age: f64) -> u8 {
    let stage = (age / STAGE_SECONDS).floor().clamp(0.0, f64::from(STAGES));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let stage = stage as u8;
    stage
}

/// A plant thinks: its look moves on to its age's stage, and while it is not ripe its needs may
/// grow (past the last level it withers); it thinks again until it is ripe.
pub fn thinks<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(p) = s.plants.by_id.get(&id).cloned() else {
        return;
    };
    let age = clock.local.0 - p.planted;
    let stage = stage_at(age);
    if stage > p.stage {
        if let Some(q) = s.plants.by_id.get_mut(&id) {
            q.stage = stage;
        }
        // A species with lines of its own keeps its own look.
        let look = if p.species.texts.is_some() {
            None
        } else if stage >= STAGES {
            s.plants.looks.get(&p.species.harvest).cloned()
        } else {
            Some(growing_look(stage))
        };
        if let Some(look) = look {
            show(s, net, id, look);
        }
    }
    if age >= p.species.ripe {
        return;
    }
    let (w, f) = (growth(s.rng.unit()), growth(s.rng.unit()));
    let Some(q) = s.plants.by_id.get_mut(&id) else {
        return;
    };
    q.water += w;
    q.fertilizer += f;
    let why = if q.water > LAST_NEED {
        Some(text::NO_WATER)
    } else if q.fertilizer > LAST_NEED {
        Some(text::NO_FERTILIZER)
    } else {
        None
    };
    if let Some(why) = why {
        withers(s, net, id, why);
        return;
    }
    let wait = f64::from(s.rng.between(THINK_SECONDS.0, THINK_SECONDS.1));
    s.after(clock, wait, Timer::PlantThinks { plant: id });
}

/// A plant takes a new look, shown to everyone who sees it and to whoever comes to see it.
fn show<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, look: VisualDesc) {
    let sender = crate::inventory::visual(s, id);
    if let Some(t) = s.things.get_mut(id) {
        t.drawn_as = Some(look.clone());
    }
    let m = ServerMessage::UpdateVisualDesc(ObjectEvent {
        sender,
        action: look,
    });
    crate::combat::to_all_seeing(s, net, id, &m);
}

/// A plant withers: its planter is told why, it is taken off the planter's pets, and it leaves the
/// world.
fn withers<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, why: u32) {
    let Some(p) = s.plants.by_id.remove(&id) else {
        return;
    };
    let why = own_line(&p.species, |t| t.withers, why);
    if let Some((world, name)) = named(s, p.owner) {
        progress::private(net, world, &told(lines(&p.species).0, why, &name));
        progress::private(net, world, &crate::pets::removed(id));
    }
    crate::death::vanish(s, net, id);
}

/// Whether an object is a plant another player planted: caring for it is left to its planter.
#[must_use]
pub fn not_planted_by<S: Store>(s: &Server<S>, world: ConnId, target: u64) -> bool {
    let me = s.players.get(&world).map(|p| p.character);
    s.plants
        .by_id
        .get(&target)
        .is_some_and(|p| Some(p.owner) != me)
}

/// A player leaves alone a plant it did not plant: it is told so, as at a harvest.
pub fn left_alone<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, plant: u64) {
    let (table, said) = s
        .plants
        .by_id
        .get(&plant)
        .map_or((text::TABLE, text::NOT_YOURS), |p| {
            let said = own_line(&p.species, |t| t.not_yours, text::NOT_YOURS);
            (lines(&p.species).0, said)
        });
    if let Some(name) = s
        .players
        .get(&world)
        .map(|q| crate::player::world_name(&q.name, q.sex))
    {
        progress::private(net, world, &told(table, said, &name));
    }
}

/// An item used on an object has been carried out and answered: when the object is a plant, its
/// species' water or fertilizer (a watering can or a bag of fertilizer, unless its row names
/// others) lowers its need by one level (a plant cared for past the first level withers; a ripe
/// plant's needs no longer count), and a pollen brushes onto it (onto one not yet pollinated).
pub fn cared<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (item, target): (u32, u64),
    clock: Clock,
) {
    let is_pollen = s.plants.kinds.pollen_of(item).is_some();
    let Some(p) = s.plants.by_id.get_mut(&target) else {
        return;
    };
    let p_species = p.species;
    let (water, fertilizer) = (p.species.water, p.species.fertilizer);
    // A ripe plant's needs no longer count, and one that needs no care has none.
    let ripe = ripe_at(p, clock.local.0);
    if (ripe || !p.species.tended) && (item == water || item == fertilizer) {
        return;
    }
    if item == water || item == fertilizer {
        let (need, why) = if item == water {
            (&mut p.water, text::TOO_MUCH_WATER)
        } else {
            (&mut p.fertilizer, text::TOO_MUCH_FERTILIZER)
        };
        *need -= 1;
        if *need < 0 {
            withers(s, net, target, why);
        }
        return;
    }
    if !is_pollen {
        return;
    }
    let took = p.pollen.is_none();
    if took {
        p.pollen = Some(item);
    }
    let said = if took {
        text::BRUSHED
    } else {
        text::BLOWN_AWAY
    };
    if let Some(name) = s
        .players
        .get(&world)
        .map(|q| crate::player::world_name(&q.name, q.sex))
    {
        progress::private(net, world, &told(lines(&p_species).0, said, &name));
    }
}

/// A player uses a plant: walked to it when it is farther than [`REACH`], else it begins.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    let Some(at) = s.things.get(u.item).and_then(|t| t.at) else {
        return;
    };
    let near = s
        .players
        .get(&world)
        .is_some_and(|p| crate::presence::distance(&p.position, &at).is_some_and(|d| d <= REACH));
    if near {
        begin(s, net, world, u, clock);
    } else {
        crate::places::walk_to(s, net, world, u, REACH);
    }
}

/// A player at a plant begins to harvest it: it plays the behavior its template names for a use,
/// the plants' own where it names none, and the harvest comes when that has played out.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    use crate::usable::interactions::{PLANT_BEHAVIOR, USER_BEHAVIOR};
    let Some(behavior) = s
        .things
        .get(u.item)
        .and_then(|t| s.catalog.get(t.template))
        .map(|t| t.props.word(USER_BEHAVIOR).unwrap_or(PLANT_BEHAVIOR))
    else {
        return;
    };
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut b = BehaviorParams {
        bits: HARVEST_BITS,
        behavior,
        context,
        ..BehaviorParams::default()
    };
    // The words of flags 0x20 and 0x100 (the second and the seventh of the flags with a word).
    b.words[1] = 1;
    b.words[6] = 1;
    let plays = ServerMessage::PlayBehavior(crate::places::player_event(p, b));
    let me = p.character;
    crate::combat::to_all_seeing(s, net, me, &plays);
    s.after(
        clock,
        crate::usable::interactions::behavior_seconds(behavior),
        Timer::Harvest {
            world,
            used: Box::new(u),
        },
    );
}

/// A harvest's behavior has played out: the use is answered and the player told what came of it:
/// not ripe yet, not its plant, or ripe: the plant gone, its yield in the pack.
pub fn harvested<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) {
    let Some(p) = s.plants.by_id.get(&u.item).cloned() else {
        return;
    };
    let Some((me, name, from)) = s.players.get(&world).map(|q| {
        (
            q.character,
            crate::player::world_name(&q.name, q.sex),
            q.position,
        )
    }) else {
        return;
    };
    let distance = s
        .things
        .get(u.item)
        .and_then(|t| t.at)
        .and_then(|at| crate::presence::distance(&from, &at))
        .unwrap_or(0.0);
    let done = crate::places::use_done(u, me, HARVEST_FLAGS, false, distance);
    progress::private(net, world, &done);
    let (table, _, _, own_harvest) = lines(&p.species);
    if p.owner != me {
        let said = own_line(&p.species, |t| t.not_yours, text::NOT_YOURS);
        progress::private(net, world, &told(table, said, &name));
        return;
    }
    if !ripe_at(&p, clock.local.0) {
        let said = own_line(&p.species, |t| t.not_mature, text::NOT_MATURE);
        progress::private(net, world, &told(table, said, &name));
        return;
    }
    let crossed = p
        .pollen
        .and_then(|pollen| s.plants.kinds.cross(p.species.plant, pollen));
    let said = match (crossed, own_harvest) {
        (Some(c), _) => text::SEEDS[usize::try_from(c.count.clamp(1, 3)).unwrap_or(1) - 1],
        (None, Some(own)) => own,
        (None, None) if p.species.pollen.is_some() => text::RIPE_WITH_POLLEN,
        (None, None) => text::RIPE,
    };
    progress::private(net, world, &told(table, said, &name));
    s.plants.by_id.remove(&u.item);
    progress::private(net, world, &crate::pets::removed(u.item));
    crate::death::vanish(s, net, u.item);
    let yields: Vec<(u32, u32)> = match crossed {
        Some(c) => vec![(c.seeds, c.count)],
        None => std::iter::once((p.species.harvest, 1))
            .chain(p.species.pollen.map(|x| (x, 1)))
            .collect(),
    };
    for (template, n) in yields {
        if crate::inventory::give_from(s, net, world, template, n, u.item).is_some() {
            crate::quests::took(s, net, world, template, clock);
        }
    }
    crate::world::keep_player(s, world);
}

/// A seed given to a placed object: when the seed's species is planted in a garden of the object's
/// template, its plant stands there, or, while the garden's plant stands, the seed is refused with
/// the garden's busy line. Returns `Some(true)` for a seed kept and planted, `Some(false)` for one
/// refused (to be handed back), and `None` for a give that is no garden's.
pub fn given<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    garden: usize,
    given: crate::gives::Given,
    clock: Clock,
) -> Option<bool> {
    let o = s.content.objects.get(garden)?;
    let species = s.plants.kinds.sown_in(given.template, o.template)?;
    let g = species.garden?;
    let at = StoredPosition {
        cell: o.cell,
        origin: o.origin,
        rotation: o.rotation,
    };
    let speaker = crate::talk::content_speaker(s, garden);
    let say = |s: &Server<S>, net: &mut ServerNet, line: Option<(u32, u32)>| {
        if let Some((string, table)) = line {
            let words = Words::Ref(string, table);
            crate::talk::say(s, net, world, &speaker, &words, How::Emote);
        }
    };
    if s.plants.by_id.values().any(|p| p.garden == Some(garden)) {
        say(s, net, g.busy);
        return Some(false);
    }
    s.catalog.get(species.plant)?;
    crate::gives::keep(
        s,
        net,
        world,
        crate::gives::Given {
            quantity: 1,
            ..given
        },
    );
    say(s, net, g.planted);
    plant(s, net, world, species, (at, Some(garden)), clock);
    crate::world::keep_player(s, world);
    Some(true)
}

/// A planter leaves the world: its plants leave it too.
pub fn owner_leaves<S: Store>(s: &mut Server<S>, net: &mut ServerNet, owner: u64) {
    let mine: Vec<u64> = s
        .plants
        .by_id
        .values()
        .filter(|p| p.owner == owner)
        .map(|p| p.id)
        .collect();
    for id in mine {
        s.plants.by_id.remove(&id);
        crate::death::vanish(s, net, id);
    }
}

/// What a growing plant's examination shows of it: its needs (levels of [`NEED_LEVELS`]) and its
/// pollen's name among the plants' texts ([`text::NO_POLLEN`] for none), for one that needs care;
/// or, for one with lines of its own for it, how it fares; and the game time it is ripe at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Growing {
    pub needs: Option<Needs>,
    pub fares: Option<Fares>,
    pub ripe_game: f64,
}

/// How a growing plant fares, as its own lines show it: its stage's line, and the lines of its
/// conditions (healthy, or each that holds), in its species' table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fares {
    pub stage: Option<u32>,
    pub conditions: [Option<u32>; 4],
}

/// How a plant with these needs (levels of [`NEED_LEVELS`]) fares, by its species' lines: healthy,
/// or each of dry, overwatered, hungry and overfed that holds, in that order.
#[must_use]
pub fn fares(c: &Conditions, water: i32, fertilizer: i32) -> [Option<u32>; 4] {
    // "Needed" and past it; "Too much" and before it.
    let (needed, too_much) = (4, 1);
    let held = [
        (water >= needed, c.dry),
        (water <= too_much, c.overwatered),
        (fertilizer >= needed, c.hungry),
        (fertilizer <= too_much, c.overfed),
    ];
    let mut out = [None; 4];
    let mut n = 0;
    for (holds, line) in held {
        if holds {
            out[n] = Some(line);
            n += 1;
        }
    }
    if n == 0 {
        out[0] = Some(c.healthy);
    }
    out
}

/// A growing plant's needs and pollen, as its examination shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub water: i32,
    pub fertilizer: i32,
    pub pollen: u32,
}

/// What a plant's examination shows of the plant itself: its name, its level, its kingdom (its
/// planter's, by faction), its health and vigor, its armor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    pub name: Option<(u32, u32)>,
    pub level: i32,
    pub kingdom: u32,
    pub vitals: crate::examine::Vitals,
    pub armor: i32,
}

/// A plant's examination profile: its name (a quest creature's), an empty description; growing,
/// its water, fertilizer and pollen (one that needs care) or how it fares, and a countdown to its
/// ripening where its species has one, else that it is ripe, in its species' lines (`lines`: their
/// table, the countdown's label and the ripe line); its level, kingdom, health, vigor and armor.
#[must_use]
pub fn profile_of(
    asked: u32,
    shown: &Shown,
    growing: Option<Growing>,
    (table, maturity_in, mature): (u32, Option<u32>, u32),
) -> crate::profile::Profile {
    use crate::profile::{self as pr, look, GENERAL};
    let mut prof = pr::Profile::new(asked);
    let name = shown
        .name
        .map_or_else(|| pr::literal(""), |(sid, tb)| pr::string(tb, sid));
    prof.text(GENERAL, look::QUEST_NAME, name);
    prof.brk(GENERAL);
    prof.text(GENERAL, look::LABEL, pr::literal(""));
    prof.brk(GENERAL);
    let line = |id: u32| pr::string(table, id);
    if let Some(g) = growing {
        if let Some(n) = g.needs {
            let need = |n: i32| NEED_LEVELS[usize::try_from(n.clamp(0, LAST_NEED)).unwrap_or(0)];
            for (label, n) in [(text::WATER, n.water), (text::FERTILIZER, n.fertilizer)] {
                prof.text(GENERAL, look::LABEL, line(label));
                prof.text(GENERAL, look::VALUE, line(need(n)));
                prof.brk(GENERAL);
            }
            prof.text(GENERAL, look::LABEL, line(text::POLLEN));
            prof.text(GENERAL, look::VALUE, line(n.pollen));
            prof.brk(GENERAL);
        }
        if let Some(f) = g.fares {
            for l in f
                .stage
                .into_iter()
                .chain(f.conditions.into_iter().flatten())
            {
                prof.text(GENERAL, look::LABEL, line(l));
                prof.brk(GENERAL);
            }
        }
        if let Some(label) = maturity_in {
            prof.text(GENERAL, look::LABEL, line(label));
            prof.countdown(GENERAL, look::VALUE, g.ripe_game);
            prof.brk(GENERAL);
        }
    } else {
        prof.text(GENERAL, look::LABEL, line(mature));
        prof.brk(GENERAL);
    }
    prof.brk(GENERAL);
    pr::creature_lines(
        &mut prof,
        shown.level,
        shown.kingdom,
        shown.vitals,
        shown.armor,
    );
    prof
}

/// The examination profile of a plant as it is now (`now` in world seconds).
#[must_use]
pub fn profile<S: Store>(
    s: &Server<S>,
    asked: u32,
    t: &Thing,
    now: f64,
) -> crate::profile::Profile {
    let growing = s
        .plants
        .by_id
        .get(&t.id)
        .filter(|p| !ripe_at(p, now))
        .map(|p| {
            // One that needs care and has lines of its own for how it fares shows those.
            let own = p
                .species
                .texts
                .and_then(|t| t.conditions.map(|c| (t, c)))
                .filter(|_| p.species.tended);
            Growing {
                needs: (p.species.tended && own.is_none()).then(|| Needs {
                    water: p.water,
                    fertilizer: p.fertilizer,
                    pollen: p
                        .pollen
                        .and_then(|x| s.plants.kinds.pollen_of(x))
                        .map_or(text::NO_POLLEN, |x| x.name),
                }),
                fares: own.map(|(t, c)| Fares {
                    stage: t
                        .stages
                        .map(|st| st[usize::from(stage_at(now - p.planted).min(STAGES - 1))]),
                    conditions: fares(&c, p.water, p.fertilizer),
                }),
                ripe_game: p.ripe_game,
            }
        });
    let own = s.plants.by_id.get(&t.id).map(|p| lines(&p.species));
    let (table, maturity_in, mature, _) =
        own.unwrap_or((text::TABLE, Some(text::MATURITY_IN), text::MATURE, None));
    let armor = s
        .catalog
        .get(t.template)
        .and_then(|x| x.props.int("NPC_ArmorLevel"))
        .and_then(|a| i32::try_from(a).ok())
        .unwrap_or(0);
    let v = crate::examine::Vitals {
        health: t.health.max(0).cast_unsigned(),
        max_health: t.max_health.max(0).cast_unsigned(),
        vigor: t.vigor.max(0).cast_unsigned(),
        max_vigor: t.max_vigor.max(0).cast_unsigned(),
        pk_damage: 0,
        pk_vigor_loss: 0,
    };
    // The plant shows its planter's kingdom.
    let kingdom = s
        .plants
        .by_id
        .get(&t.id)
        .and_then(|p| s.players.values().find(|q| q.character == p.owner))
        .and_then(|q| crate::kingdoms::of(&q.qualities))
        .unwrap_or(0);
    let shown = Shown {
        name: t.facts.name,
        level: t.level(),
        kingdom,
        vitals: v,
        armor,
    };
    profile_of(asked, &shown, growing, (table, maturity_in, mature))
}
