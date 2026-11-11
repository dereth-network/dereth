//! Workshops: the crafting, refining and spellbinding stations standing in the towns, whose
//! nearness is what many recipes need.
//!
//! A workshop puts its effect on a player while the player is within one land cell of the workshop's
//! own (the 3 by 3 land cells around it), and takes it off when the player goes beyond them: a
//! record held until taken off, gone when the player leaves the world, cast by the workshop at
//! spellcraft 1, shown in the client's effects panel. A recipe that needs the effect is refused
//! without it ("You must be near a Crafting Workshop to craft this item.", the recipe's own text).
//!
//! Indoors nothing changes for a workshop standing outdoors: a player who walks into a building
//! keeps what it had, and is looked at again when it comes out, unless it has gone to another part
//! of the world altogether.
//!
//! **A workshop standing indoors** (in a dungeon's room) reaches only players indoors in its own
//! landblock: in its own cell or one its cell's doorways join or it sees, and within its template's
//! reach of it, by straight-line distance ([`REACH_PROPERTY`], else [`INDOOR_REACH`]). Its record
//! comes on as a player comes within that reach and goes as the player leaves it, or goes outdoors.
//! No recording shows a workshop indoors: this is the server's own rule.

use std::collections::HashMap;

use yalain_content::{Catalog, ContentObject};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::effects::{self, record_flag, Cast, On, Shape};
use crate::{presence, Clock, Server};

/// The property of a template naming the effect its objects put on players nearby.
pub const EFFECT_PROPERTY: &str = "ForgeEffect";

/// The property of a template naming how far, in meters, its effect reaches: what an indoor
/// workshop goes by (outdoors, the land cells around decide).
pub const REACH_PROPERTY: &str = "ForgeEffectRadius";

/// How far an indoor workshop whose template names no reach reaches, in meters: what 13 of the 14
/// templates with a workshop's effect name.
pub const INDOOR_REACH: f32 = 8.0;

/// A workshop's record: held until taken off, gone at logout (2017: flags 81 and categories 1 on
/// all 534 records).
pub const RECORD_FLAGS: u32 = record_flag::INFINITE | record_flag::LOGOUT | record_flag::TRANSIENT;
pub const RECORD_CATEGORIES: u32 = 1;

/// A workshop: its object, its land cell, the effect it puts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workshop {
    pub id: u64,
    pub land_cell: (i64, i64),
    pub effect: u32,
}

/// A workshop standing indoors: its object, its interior cell and place in the cell's landblock,
/// the effect it puts on, how far it reaches in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndoorWorkshop {
    pub id: u64,
    pub cell: u32,
    pub origin: [f32; 3],
    pub effect: u32,
    pub reach: f32,
}

/// The world's workshops: those outdoors by the land cells each reaches, those indoors by
/// themselves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Workshops {
    by_cell: HashMap<(i64, i64), Vec<Workshop>>,
    land_cells: HashMap<u64, (i64, i64)>,
    indoors: Vec<IndoorWorkshop>,
}

impl Workshops {
    /// The workshops among the placed objects.
    #[must_use]
    pub fn of(objects: &[ContentObject], catalog: &Catalog) -> Self {
        let mut w = Self::default();
        for (i, o) in objects.iter().enumerate() {
            let Some(template) = catalog.get(o.template) else {
                continue;
            };
            let Some(effect) = template.props.word(EFFECT_PROPERTY).filter(|e| *e != 0) else {
                continue;
            };
            let id = presence::content_id(objects, i);
            if presence::indoors(o.cell) {
                w.place_indoors(IndoorWorkshop {
                    id,
                    cell: o.cell,
                    origin: o.origin,
                    effect,
                    reach: template
                        .props
                        .float(REACH_PROPERTY)
                        .filter(|r| *r > 0.0)
                        .unwrap_or(INDOOR_REACH),
                });
            } else {
                w.place(id, presence::land_cell(o.cell, o.origin), effect);
            }
        }
        w
    }

    /// A workshop stands in a land cell.
    pub fn place(&mut self, id: u64, land_cell: (i64, i64), effect: u32) {
        let workshop = Workshop {
            id,
            land_cell,
            effect,
        };
        self.land_cells.insert(id, land_cell);
        for dx in -1..=1 {
            for dy in -1..=1 {
                self.by_cell
                    .entry((land_cell.0 + dx, land_cell.1 + dy))
                    .or_default()
                    .push(workshop);
            }
        }
    }

    /// A workshop stands indoors.
    pub fn place_indoors(&mut self, workshop: IndoorWorkshop) {
        self.indoors.push(workshop);
    }

    /// The indoor workshops reaching a player at a place indoors, with what `ground` knows of the
    /// rooms (nothing known: any cell of the landblock).
    #[must_use]
    pub fn reaching_indoors(
        &self,
        ground: Option<&dyn crate::Ground>,
        at: &yalain_store::StoredPosition,
    ) -> Vec<IndoorWorkshop> {
        if !presence::indoors(at.cell) {
            return Vec::new();
        }
        self.indoors
            .iter()
            .filter(|w| w.cell >> 16 == at.cell >> 16)
            .filter(|w| crate::reported::way_in(ground, w.cell, at.cell))
            .filter(|w| {
                let there = yalain_store::StoredPosition {
                    cell: w.cell,
                    origin: w.origin,
                    rotation: [1.0, 0.0, 0.0, 0.0],
                };
                presence::distance(at, &there).is_some_and(|d| d <= w.reach)
            })
            .copied()
            .collect()
    }

    /// Is this object one of the world's workshops?
    fn is_workshop(&self, id: u64) -> bool {
        self.land_cells.contains_key(&id) || self.is_indoors(id)
    }

    /// Is this object a workshop standing indoors?
    fn is_indoors(&self, id: u64) -> bool {
        self.indoors.iter().any(|w| w.id == id)
    }

    /// The workshops reaching a land cell.
    #[must_use]
    pub fn reaching(&self, land_cell: (i64, i64)) -> &[Workshop] {
        self.by_cell.get(&land_cell).map_or(&[], Vec::as_slice)
    }

    /// Is there no workshop at all?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.land_cells.is_empty() && self.indoors.is_empty()
    }

    /// Is a workshop's record to stay on a player indoors in this land cell's landblock: is the
    /// workshop in that landblock or one beside it?
    fn in_landblocks_around(&self, id: u64, land_cell: (i64, i64)) -> bool {
        let block = |c: (i64, i64)| (c.0.div_euclid(8), c.1.div_euclid(8));
        let here = block(land_cell);
        self.land_cells.get(&id).is_some_and(|c| {
            let there = block(*c);
            (there.0 - here.0).abs() <= 1 && (there.1 - here.1).abs() <= 1
        })
    }
}

/// A player's place was told: the workshops around it put their effects on it, and those it has
/// left take theirs off.
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    if s.workshops.is_empty() {
        return;
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let cell = p.position.cell;
    let land_cell = presence::land_cell(cell, p.position.origin);
    let indoors = presence::indoors(cell);
    let reaching: Vec<(u64, u32)> = if indoors {
        s.workshops
            .reaching_indoors(s.ground.as_deref(), &p.position)
            .iter()
            .map(|w| (w.id, w.effect))
            .collect()
    } else {
        s.workshops
            .reaching(land_cell)
            .iter()
            .map(|w| (w.id, w.effect))
            .collect()
    };
    let gone: Vec<u32> = p
        .play
        .effects
        .iter()
        .filter(|e| e.flags == RECORD_FLAGS && s.workshops.is_workshop(e.caster))
        .filter(|e| {
            let reached = reaching.iter().any(|(id, _)| *id == e.caster);
            if indoors && !s.workshops.is_indoors(e.caster) {
                !s.workshops.in_landblocks_around(e.caster, land_cell)
            } else {
                !reached
            }
        })
        .map(|e| e.eid)
        .collect();
    let held: Vec<u64> = p
        .play
        .effects
        .iter()
        .filter(|e| e.flags == RECORD_FLAGS)
        .map(|e| e.caster)
        .collect();
    for eid in gone {
        effects::remove(s, net, world, eid);
    }
    for (id, effect) in reaching {
        if held.contains(&id) {
            continue;
        }
        let Some(def) = s.effect_table.get(effect).cloned() else {
            continue;
        };
        effects::add_quietly(
            s,
            net,
            On::Player(world),
            &def,
            Cast {
                caster: id,
                spellcraft: 1.0,
                item: id,
                ..Cast::default()
            },
            Shape {
                flags: RECORD_FLAGS,
                categories: Some(RECORD_CATEGORIES),
                timeout: Some(f32::MAX),
                acting: 0,
            },
            clock,
        );
    }
}
