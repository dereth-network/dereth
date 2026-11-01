//! Dye plants: a seed planted in its garden grows into a plant its planter waters, fertilizes,
//! may pollinate, and harvests when it is ripe.
//!
//! **Planting.** A seed may be used only near its garden (its template's place and radius). Its
//! use plays the planting behavior with the seed in hand; when that has played out (2.0 s) the
//! planter is told of a new pet (no time left in the world, the plants' class and flags, no icon,
//! the plant's name), the plant stands two meters straight ahead of the planter facing as it faces,
//! the seed is spent and the use answered "You use {the} %1.".
//!
//! **Growing.** A plant thinks every 10 to 20 seconds. At each thought its look moves on to the
//! stage its age has reached (a stage each 45 s: two growing looks, then the ripe look, the look of
//! its species' plant item), shown to everyone who sees it; and while it is not yet ripe, its need
//! of water and its need of fertilizer may each grow: by nothing (three times in four), by one
//! level or by two. A plant is ripe 180 s after it was planted; its needs no longer grow.
//!
//! **Its needs** run from "Far too much!" through "Too much", "None needed" (twice), "Needed",
//! "Badly needed", "Urgently needed" and "Desperately needed" to "Death imminent!"; a new plant
//! badly needs both. A watering can used on it lowers its need of water by one level, a bag of
//! fertilizer its need of fertilizer. A need that grows past "Death imminent!", or a plant cared
//! for past "Far too much!", withers the plant: its planter is told why, and it is gone.
//!
//! **Pollen** used on a plant brushes onto its leaves ("You brush the pollen on to the plants'
//! leaves.", after the use's answer); a plant already pollinated does not take another (the
//! breeze blows it away). Examining a plant shows its needs, its pollen and the time until it is
//! ripe, or that it is ripe.
//!
//! **Harvesting.** A player uses the plant (walking to it first when it is farther than 4 m): it
//! plays the plants' behavior, and when that has played out the use is answered, with a text: a
//! plant not yet ripe "is not mature yet" and stays; one the player did not plant is left alone; a
//! ripe one is taken off its planter's pets and leaves the world, and gives its plant item and a
//! pollen of its kind, or, pollinated with a pollen whose cross is known, seeds of the cross's
//! species.
//!
//! **Its end.** A plant leaves the world with its planter, as a pet does.

use std::collections::{BTreeMap, HashMap};

use auberean_protocol::physics::ObjectEvent;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::{BehaviorParams, IconLayer, PartGroupDesc, VisualDesc};
use auberean_protocol::ServerMessage;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::Use;
use crate::things::{Kind, Thing};
use crate::{progress, Clock, Server, Timer};

/// One species of dye plant: the planted plant's template, its seed, its pollen (the clear plant
/// has none), the plant item a harvest gives, and its name among the plants' texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Species {
    pub plant: u32,
    pub seed: u32,
    pub pollen: Option<u32>,
    pub harvest: u32,
    pub name: u32,
}

impl Species {
    #[must_use]
    pub const fn new(plant: u32, seed: u32, pollen: Option<u32>, harvest: u32, name: u32) -> Self {
        Self {
            plant,
            seed,
            pollen,
            harvest,
            name,
        }
    }
}

/// The thirteen species: Thananim (black), Siennan (bronze), Berimphur (brown), Acromitan (the
/// clear one, that takes dye away), Colban (dark blue), Verdalim (dark green), Aurim (gold), Lapyan
/// (light blue), Minalim (light green), Bergaphur (orange), Rosim (pink), Relanim (purple) and
/// Hennacin (red).
pub const SPECIES: [Species; 13] = [
    Species::new(
        0x4700_341D,
        0x4700_1452,
        Some(0x4700_1446),
        0x4700_0FF5,
        0x8129_71D6,
    ),
    Species::new(
        0x4700_341F,
        0x4700_1453,
        Some(0x4700_1447),
        0x4700_0FF7,
        0x839B_71D6,
    ),
    Species::new(
        0x4700_3420,
        0x4700_1454,
        Some(0x4700_1448),
        0x4700_0FF9,
        0x811B_71D6,
    ),
    Species::new(0x4700_3421, 0x4700_1455, None, 0x4700_0FFB, 0),
    Species::new(
        0x4700_3422,
        0x4700_1456,
        Some(0x4700_1449),
        0x4700_0FFD,
        0x821B_71D6,
    ),
    Species::new(
        0x4700_3423,
        0x4700_1457,
        Some(0x4700_144A),
        0x4700_0FFF,
        0x801B_71D6,
    ),
    Species::new(
        0x4700_3424,
        0x4700_1458,
        Some(0x4700_144B),
        0x4700_1001,
        0x831B_71D6,
    ),
    Species::new(
        0x4700_3425,
        0x4700_1459,
        Some(0x4700_144C),
        0x4700_1003,
        0x859B_71D6,
    ),
    Species::new(
        0x4700_3426,
        0x4700_145A,
        Some(0x4700_144D),
        0x4700_1005,
        0x829B_71D6,
    ),
    Species::new(
        0x4700_3427,
        0x4700_145B,
        Some(0x4700_144E),
        0x4700_1006,
        0x812B_71D6,
    ),
    Species::new(
        0x4700_3428,
        0x4700_145C,
        Some(0x4700_144F),
        0x4700_1007,
        0x812A_71D6,
    ),
    Species::new(
        0x4700_3429,
        0x4700_145D,
        Some(0x4700_1450),
        0x4700_1009,
        0x851B_71D6,
    ),
    Species::new(
        0x4700_342A,
        0x4700_145E,
        Some(0x4700_1451),
        0x4700_100A,
        0x809B_71D6,
    ),
];

/// A cross: a plant of one species pollinated with another's pollen gives so many seeds of a
/// third.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cross {
    /// The plant's template, and the pollen's.
    pub plant: u32,
    pub pollen: u32,
    /// The seeds' template, and how many.
    pub seeds: u32,
    pub count: u32,
}

/// The crosses known: Berimphur with Hennacin pollen gives three Siennan seeds, Aurim with
/// Siennan pollen two Colban seeds.
pub const CROSSES: [Cross; 2] = [
    Cross {
        plant: 0x4700_3420,
        pollen: 0x4700_1451,
        seeds: 0x4700_1453,
        count: 3,
    },
    Cross {
        plant: 0x4700_3424,
        pollen: 0x4700_1447,
        seeds: 0x4700_1456,
        count: 2,
    },
];

/// The watering can and the bag of fertilizer.
pub const WATERING_CAN: u32 = 0x4700_145F;
pub const FERTILIZER: u32 = 0x4700_1445;

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

/// A plant's pet data: its class and flags.
pub const PET_CLASS: u32 = 0x0400_0000;
pub const PET_FLAGS: u32 = 132_272;

/// How long each look stage lasts, how many stages there are, and when a plant is ripe, in
/// seconds since it was planted.
pub const STAGE_SECONDS: f64 = 45.0;
pub const STAGES: u8 = 3;
pub const RIPE_SECONDS: f64 = 180.0;

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
}

/// Each species' ripe look, by its plant item.
pub type Looks = HashMap<u32, VisualDesc>;

/// The plants of the world, and each species' ripe look.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plants {
    pub by_id: BTreeMap<u64, Plant>,
    pub looks: Looks,
}

/// The species a seed plants.
#[must_use]
pub fn sown_by(seed: u32) -> Option<Species> {
    SPECIES.iter().copied().find(|s| s.seed == seed)
}

/// The species whose pollen this is.
#[must_use]
pub fn pollen_of(pollen: u32) -> Option<Species> {
    SPECIES.iter().copied().find(|s| s.pollen == Some(pollen))
}

/// The cross of a plant and a pollen, when known.
#[must_use]
pub fn cross(plant: u32, pollen: u32) -> Option<Cross> {
    CROSSES
        .iter()
        .copied()
        .find(|c| c.plant == plant && c.pollen == pollen)
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

/// A text of the plants' table naming a player.
fn told(id: u32, name: &str) -> ServerMessage {
    let t = progress::text_with(
        (id, text::TABLE),
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

/// The place two meters straight ahead of a planter, facing as it faces, on the ground where the
/// ground is known.
fn ahead<S: Store>(s: &Server<S>, at: &StoredPosition) -> StoredPosition {
    let [w, x, y, z] = at.rotation;
    let (fx, fy) = (2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z));
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

/// Is an item template a seed of a dye plant?
#[must_use]
pub fn is_seed(template: u32) -> bool {
    sown_by(template).is_some()
}

/// A seed's planting has played out: the planter is told of its new plant, which stands ahead of
/// it. Returns the plant, or none for an item that is no seed.
pub fn sow<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    seed: u32,
    clock: Clock,
) -> Option<u64> {
    let species = sown_by(seed)?;
    let template = s.catalog.get(species.plant)?.clone();
    let (owner, at) = s.players.get(&world).map(|p| (p.character, p.position))?;
    let id = s.things.new_id();
    let name = template.description.name.unwrap_or_default();
    progress::private(
        net,
        world,
        &crate::pets::added(id, progress::text(name), 0, PET_CLASS, PET_FLAGS),
    );
    let mut t = Thing::of(id, &template, Kind::Plant);
    t.at = Some(ahead(s, &at));
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
            ripe_game: s.game_now + RIPE_SECONDS,
            stage: 0,
            water: NEW_NEED,
            fertilizer: NEW_NEED,
            pollen: None,
        },
    );
    crate::presence::thing_appears(s, net, id);
    let wait = f64::from(s.rng.between(THINK_SECONDS.0, THINK_SECONDS.1));
    s.after(clock, wait, Timer::PlantThinks { plant: id });
    Some(id)
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
        let look = if stage >= STAGES {
            s.plants.looks.get(&p.species.harvest).cloned()
        } else {
            Some(growing_look(stage))
        };
        if let Some(look) = look {
            show(s, net, id, look);
        }
    }
    if age >= RIPE_SECONDS {
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
    if let Some((world, name)) = named(s, p.owner) {
        progress::private(net, world, &told(why, &name));
        progress::private(net, world, &crate::pets::removed(id));
    }
    crate::death::vanish(s, net, id);
}

/// An item used on an object has been carried out and answered: when the object is a plant, a
/// watering can or a bag of fertilizer lowers its need by one level (a plant cared for past the
/// first level withers; a ripe plant's needs no longer count), and a pollen brushes onto it (onto
/// one not yet pollinated).
pub fn cared<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (item, target): (u32, u64),
    clock: Clock,
) {
    let Some(p) = s.plants.by_id.get_mut(&target) else {
        return;
    };
    // A ripe plant's needs no longer count.
    let ripe = clock.local.0 - p.planted >= RIPE_SECONDS;
    if ripe && (item == WATERING_CAN || item == FERTILIZER) {
        return;
    }
    if item == WATERING_CAN || item == FERTILIZER {
        let (need, why) = if item == WATERING_CAN {
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
    if pollen_of(item).is_none() {
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
        progress::private(net, world, &told(said, &name));
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

/// A player at a plant begins to harvest it: it plays the plants' behavior, and the harvest comes
/// when that has played out.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let Some(behavior) = s
        .things
        .get(u.item)
        .and_then(|t| s.catalog.get(t.template))
        .and_then(|t| t.props.word(crate::usable::interactions::USER_BEHAVIOR))
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
    let ripe = clock.local.0 - p.planted >= RIPE_SECONDS;
    if p.owner != me {
        progress::private(net, world, &told(text::NOT_YOURS, &name));
        return;
    }
    if !ripe {
        progress::private(net, world, &told(text::NOT_MATURE, &name));
        return;
    }
    let crossed = p.pollen.and_then(|pollen| cross(p.species.plant, pollen));
    let said = match crossed {
        Some(c) => text::SEEDS[usize::try_from(c.count.clamp(1, 3)).unwrap_or(1) - 1],
        None if p.species.pollen.is_some() => text::RIPE_WITH_POLLEN,
        None => text::RIPE,
    };
    progress::private(net, world, &told(said, &name));
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
        crate::inventory::give_from(s, net, world, template, n, u.item);
    }
    crate::world::keep_player(s, world);
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

/// What a growing plant's examination shows of it: its needs (levels of [`NEED_LEVELS`]), its
/// pollen's name among the plants' texts ([`text::NO_POLLEN`] for none), and the game time it is
/// ripe at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Growing {
    pub water: i32,
    pub fertilizer: i32,
    pub pollen: u32,
    pub ripe_game: f64,
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
/// its water, fertilizer and pollen and a countdown to its ripening, else that it is ripe; its
/// level, kingdom, health, vigor and armor.
#[must_use]
pub fn profile_of(asked: u32, shown: &Shown, growing: Option<Growing>) -> crate::profile::Profile {
    use crate::profile::{self as pr, look, GENERAL};
    let mut prof = pr::Profile::new(asked);
    let name = shown
        .name
        .map_or_else(|| pr::literal(""), |(sid, tb)| pr::string(tb, sid));
    prof.text(GENERAL, look::QUEST_NAME, name);
    prof.brk(GENERAL);
    prof.text(GENERAL, look::LABEL, pr::literal(""));
    prof.brk(GENERAL);
    let line = |id: u32| pr::string(text::TABLE, id);
    if let Some(g) = growing {
        let need = |n: i32| NEED_LEVELS[usize::try_from(n.clamp(0, LAST_NEED)).unwrap_or(0)];
        for (label, n) in [(text::WATER, g.water), (text::FERTILIZER, g.fertilizer)] {
            prof.text(GENERAL, look::LABEL, line(label));
            prof.text(GENERAL, look::VALUE, line(need(n)));
            prof.brk(GENERAL);
        }
        prof.text(GENERAL, look::LABEL, line(text::POLLEN));
        prof.text(GENERAL, look::VALUE, line(g.pollen));
        prof.brk(GENERAL);
        prof.text(GENERAL, look::LABEL, line(text::MATURITY_IN));
        prof.countdown(GENERAL, look::VALUE, g.ripe_game);
        prof.brk(GENERAL);
    } else {
        prof.text(GENERAL, look::LABEL, line(text::MATURE));
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
        .filter(|p| now - p.planted < RIPE_SECONDS)
        .map(|p| Growing {
            water: p.water,
            fertilizer: p.fertilizer,
            pollen: p
                .pollen
                .and_then(pollen_of)
                .map_or(text::NO_POLLEN, |x| x.name),
            ripe_game: p.ripe_game,
        });
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
    // Players have no kingdom here: the plant shows none.
    let shown = Shown {
        name: t.facts.name,
        level: t.level(),
        kingdom: 0,
        vitals: v,
        armor,
    };
    profile_of(asked, &shown, growing)
}
