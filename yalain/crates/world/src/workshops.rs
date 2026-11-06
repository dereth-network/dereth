//! Workshops: the crafting, refining and spellbinding stations standing in the towns, whose
//! nearness is what many recipes need.
//!
//! A workshop puts its effect on a player while the player is within one land cell of the workshop's
//! own (the 3 by 3 land cells around it), and takes it off when the player goes beyond them: a
//! record held until taken off, gone when the player leaves the world, cast by the workshop at
//! spellcraft 1, shown in the client's effects panel. A recipe that needs the effect is refused
//! without it ("You must be near a Crafting Workshop to craft this item.", the recipe's own text).
//!
//! Indoors nothing changes: a player who walks into a building keeps what it had, and is looked at
//! again when it comes out, unless it has gone to another part of the world altogether.

use std::collections::HashMap;

use yalain_content::{Catalog, ContentObject};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::effects::{self, record_flag, Cast, On, Shape};
use crate::{presence, Clock, Server};

/// The property of a template naming the effect its objects put on players nearby.
pub const EFFECT_PROPERTY: &str = "ForgeEffect";

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

/// The world's workshops, by the land cells each reaches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Workshops {
    by_cell: HashMap<(i64, i64), Vec<Workshop>>,
    land_cells: HashMap<u64, (i64, i64)>,
}

impl Workshops {
    /// The workshops among the placed objects.
    #[must_use]
    pub fn of(objects: &[ContentObject], catalog: &Catalog) -> Self {
        let mut w = Self::default();
        for (i, o) in objects.iter().enumerate() {
            let Some(effect) = catalog
                .get(o.template)
                .and_then(|t| t.props.word(EFFECT_PROPERTY))
                .filter(|e| *e != 0)
            else {
                continue;
            };
            w.place(
                presence::content_id(objects, i),
                presence::land_cell(o.cell, o.origin),
                effect,
            );
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

    /// The workshops reaching a land cell.
    #[must_use]
    pub fn reaching(&self, land_cell: (i64, i64)) -> &[Workshop] {
        self.by_cell.get(&land_cell).map_or(&[], Vec::as_slice)
    }

    /// Is there no workshop at all?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.land_cells.is_empty()
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
    let reaching: Vec<Workshop> = if indoors {
        Vec::new()
    } else {
        s.workshops.reaching(land_cell).to_vec()
    };
    let gone: Vec<u32> = p
        .play
        .effects
        .iter()
        .filter(|e| e.flags == RECORD_FLAGS && s.workshops.land_cells.contains_key(&e.caster))
        .filter(|e| {
            if indoors {
                !s.workshops.in_landblocks_around(e.caster, land_cell)
            } else {
                !reaching.iter().any(|w| w.id == e.caster)
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
    for w in reaching {
        if held.contains(&w.id) {
            continue;
        }
        let Some(def) = s.effect_table.get(w.effect).cloned() else {
            continue;
        };
        effects::add_quietly(
            s,
            net,
            On::Player(world),
            &def,
            Cast {
                caster: w.id,
                spellcraft: 1.0,
                item: w.id,
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
