//! The two documents: the portal table and the places.
//!
//! Both are tab-separated text with LF line endings (`→` marks a tab). The portal table:
//!
//! ```text
//! dereth-navigation-store→2
//! world→<world name>
//! portal→<source>→<wcid>→<name>→<from>→<to>→0→0→0→<min level or ->→<max level or ->→-→<requires>
//! ```
//!
//! There is one `portal` line per portal the world stands somewhere with a destination: each
//! placed instance whose weenie is a portal, and each portal a spawner makes (a placed one, or one
//! a spawner it makes makes in turn), ordered by its `from` cell, then its weenie class id, then the
//! guid of the instance that placed or spawned it, then where it stands. A portal leads where its
//! weenie says, else to the destination marker linked to it, else to its weenie's offset from where
//! it stands. A spawned portal stands at the point its spawner names, else at the spawner offset by
//! the spawner's entry (turned with it, unless the entry scatters it about the spawner); a
//! scattered portal is written at the middle of the ground it is scattered over.
//!
//! `<source>` is `published` in a server's own table and `hint` in one built from the public
//! database of a world's era, which the world may have changed. A position is `0x<cell, 8 hex
//! digits> <x> <y> <z>`, the coordinates to three decimals; an outdoor cell is written as the land
//! cell its x and y fall in. The level limits are the weenie's `MinLevel` and `MaxLevel`, `-` when
//! it has none. A name (the weenie's `Name`, else its class name) has each tab, line feed and
//! carriage return replaced by a space, and each backslash doubled. The three zeros and the `-` are
//! when the portal was last confirmed, its uses, its failures and why it was retired, which a world
//! database does not know. `<requires>` is what the portal asks beyond its levels, of the
//! character or of the world ([`crate::requirement`]), `-` for nothing.
//!
//! Version 1 was the same less `<requires>`; a reader of version 2 reads it too.
//!
//! The places:
//!
//! ```text
//! dereth-places-store→1
//! world→<world name>
//! place→<kind>→<source>→<wcid or ->→<name>→<at>→0→0→-→<to or ->
//! ```
//!
//! There is one `place` line per placed instance whose weenie is a lifestone (`lifestone`), a
//! portal (`portal`, with where it leads), a vendor (`vendor`), or a creature that cannot be
//! attacked (`npc`), leaving out any whose weenie hides it from the interface; one per portal a
//! spawner makes, as the portal table lists it; and one per town (`town`) and named landmark
//! (`landmark`) at its drop point ([`crate::towns`]), which has no class. They are ordered by
//! cell, then kind in that order, then weenie class id, then the guid of the instance that placed
//! or spawned it. Positions and names are written as the portal table writes them. The two zeros
//! are when the place was first and last seen, which a world database does not know, and the `-`
//! is a house's owner, which it never names.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::content::{id, spawn_where, Placement, Spot, Weenie, WorldSource};
use crate::requirement::{write_field, Requirement};
use crate::text::{field, land_cell, position};
use crate::towns::{self, Town, TownKind};

/// The portal table's format version.
pub const STORE_VERSION: u32 = 2;

/// The portal table's first line.
pub const HEADER: &str = "dereth-navigation-store\t3\n";

/// The places document's first line.
pub const PLACES_HEADER: &str = "dereth-places-store\t1\n";

/// How deep a chain of spawners making spawners is followed.
const SPAWN_DEPTH: usize = 6;

/// Where the documents say their knowledge came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The world's own server publishes it.
    Published,
    /// It was built from the public database of the world's era, which the world may have changed.
    Hint,
}

impl Source {
    /// The word the documents write.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Hint => "hint",
        }
    }
}

/// Both documents of one world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documents {
    pub portals: String,
    pub places: String,
}

impl Documents {
    /// Both documents of `content` for the world named `world_name`, as its own server publishes
    /// them.
    #[must_use]
    pub fn of(world_name: &str, content: &dyn WorldSource) -> Self {
        WorldPlaces::of(content).documents(world_name, Source::Published)
    }
}

/// How a portal comes to stand where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Standing {
    /// Placed by the world database, as this instance.
    Placed(u32),
    /// Made by a spawner, which this instance placed.
    Spawned(u32),
}

impl Standing {
    const fn guid(self) -> u32 {
        match self {
            Self::Placed(g) | Self::Spawned(g) => g,
        }
    }
}

/// A portal of the world: where it stands, where it leads, and what it asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    pub wcid: u32,
    pub name: String,
    pub from: Spot,
    pub to: Spot,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
    /// What it asks beyond its levels, sorted.
    pub requires: Vec<Requirement>,
    pub standing: Standing,
    /// Whether its weenie hides it from the interface.
    pub hidden: bool,
    /// Its body: its setup and scale, and whether a body touching it is taken.
    pub shape: Option<Shape>,
}

/// A portal's body as the portal table writes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub setup: u32,
    pub scale: f32,
    /// Whether a body that touches it is taken: false for a portal that ignores collisions.
    pub touches: bool,
}

impl Shape {
    /// The shape of the portal weenie `w`: its setup, its scale (1 when it names none) and its
    /// physics state's ignore-collisions bit. `None` when it names no setup.
    #[must_use]
    pub fn of(w: &Weenie) -> Option<Self> {
        /// The physics state's bit for an object nothing collides with.
        const IGNORE_COLLISIONS: i32 = 0x10;
        let setup = w.did(id::did::SETUP).filter(|s| *s != 0)?;
        #[allow(clippy::cast_possible_truncation)]
        let scale = w.float(id::float::DEFAULT_SCALE).map_or(1.0, |s| s as f32);
        let ignores = w
            .int(id::int::PHYSICS_STATE)
            .is_some_and(|s| s & IGNORE_COLLISIONS != 0);
        Some(Self {
            setup,
            scale,
            touches: !ignores,
        })
    }

    /// The shape's field in the portal table: `0x020001B3 1`, then ` untouched` for one a body
    /// is never taken by touching.
    #[must_use]
    pub fn field(self) -> String {
        format!(
            "0x{:08X} {}{}",
            self.setup,
            self.scale,
            if self.touches { "" } else { " untouched" }
        )
    }
}

/// What a place is, in the order the places document lists them within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaceKind {
    Lifestone,
    Portal,
    Vendor,
    Npc,
    Town,
    Landmark,
}

impl PlaceKind {
    /// The kind as the places document writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Lifestone => "lifestone",
            Self::Portal => "portal",
            Self::Vendor => "vendor",
            Self::Npc => "npc",
            Self::Town => "town",
            Self::Landmark => "landmark",
        }
    }

    const fn of(weenie_type: u32) -> Option<Self> {
        match weenie_type {
            id::kind::LIFESTONE => Some(Self::Lifestone),
            id::kind::PORTAL => Some(Self::Portal),
            id::kind::VENDOR => Some(Self::Vendor),
            id::kind::CREATURE | id::kind::COW => Some(Self::Npc),
            _ => None,
        }
    }
}

/// A place of the world.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub kind: PlaceKind,
    pub wcid: Option<u32>,
    pub name: String,
    pub at: Spot,
    pub to: Option<Spot>,
    /// The guid of the instance that placed or spawned it; `0` for a town or a landmark.
    pub guid: u32,
}

/// Everything the documents list for one world, read once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldPlaces {
    /// In the portal table's order.
    pub portals: Vec<Portal>,
    /// In the places document's order.
    pub places: Vec<Place>,
    /// The towns and named landmarks, by name.
    pub towns: Vec<Town>,
}

/// The weenies of a source, each read once.
struct Weenies<'a> {
    content: &'a dyn WorldSource,
    read: HashMap<u32, Option<Arc<Weenie>>>,
}

impl Weenies<'_> {
    fn get(&mut self, wcid: u32) -> Option<Arc<Weenie>> {
        let content = self.content;
        self.read
            .entry(wcid)
            .or_insert_with(|| content.weenie(wcid))
            .clone()
    }
}

impl WorldPlaces {
    /// The places of `content`.
    #[must_use]
    pub fn of(content: &dyn WorldSource) -> Self {
        let mut weenies = Weenies {
            content,
            read: HashMap::new(),
        };
        let placements = content.placements();
        let by_guid: HashMap<u32, usize> = placements
            .iter()
            .enumerate()
            .map(|(i, p)| (p.guid, i))
            .collect();
        let mut parent: HashMap<u32, u32> = HashMap::new();
        for p in &placements {
            for &child in &p.links {
                parent.entry(child).or_insert(p.guid);
            }
        }
        let events: HashMap<String, bool> = content
            .events()
            .into_iter()
            .map(|e| (e.name.to_ascii_lowercase(), e.running()))
            .collect();
        let gate = Gate { events: &events };

        let mut portals: Vec<Portal> = Vec::new();
        let mut places: Vec<Place> = Vec::new();
        for p in &placements {
            let Some(w) = weenies.get(p.wcid) else {
                continue;
            };
            // What the spawners this instance is linked under ask of the world.
            let mut inherited = Vec::new();
            let mut up = p.guid;
            let mut seen = HashSet::from([up]);
            while let Some(&q) = parent.get(&up) {
                if !seen.insert(q) {
                    break;
                }
                if let Some(qw) = by_guid
                    .get(&q)
                    .and_then(|&i| weenies.get(placements[i].wcid))
                {
                    gate.of(&qw, &mut inherited);
                }
                up = q;
            }
            if w.weenie_type == id::kind::PORTAL {
                let to = destination(&w, &p.at).or_else(|| {
                    p.links.iter().find_map(|c| {
                        let child = &placements[*by_guid.get(c)?];
                        let cw = weenies.get(child.wcid)?;
                        (cw.weenie_type == id::kind::GENERIC
                            && cw.class_name.eq_ignore_ascii_case("portaldestination"))
                        .then(|| landed(&child.at))
                    })
                });
                if let Some(to) = to {
                    portals.push(portal(&w, &p.at, to, Standing::Placed(p.guid), &inherited));
                }
            }
            if let Some(kind) = PlaceKind::of(w.weenie_type) {
                if let Some(place) = place(kind, p, &w) {
                    let to = (kind == PlaceKind::Portal)
                        .then(|| {
                            portals
                                .last()
                                .filter(|e| e.standing == Standing::Placed(p.guid))
                        })
                        .flatten()
                        .map(|e| e.to);
                    places.push(Place { to, ..place });
                }
            }
            if !w.spawns.is_empty() {
                let mut asks = inherited.clone();
                gate.of(&w, &mut asks);
                spawned(
                    &mut weenies,
                    &gate,
                    &w,
                    &p.at,
                    p.guid,
                    &asks,
                    0,
                    &mut portals,
                );
            }
        }

        // A spawner that makes the same portal at the same point more than once lists it once.
        let mut once = HashSet::new();
        portals.retain(|e| {
            once.insert((
                e.standing,
                e.wcid,
                position(&e.from),
                position(&e.to),
                write_field(&e.requires),
            ))
        });
        for e in &portals {
            if let Standing::Spawned(guid) = e.standing {
                if !e.hidden {
                    places.push(Place {
                        kind: PlaceKind::Portal,
                        wcid: Some(e.wcid),
                        name: e.name.clone(),
                        at: e.from,
                        to: Some(e.to),
                        guid,
                    });
                }
            }
        }
        portals.sort_by(|a, b| {
            (a.from.cell, a.wcid, a.standing.guid())
                .cmp(&(b.from.cell, b.wcid, b.standing.guid()))
                .then_with(|| position(&a.from).cmp(&position(&b.from)))
                .then_with(|| position(&a.to).cmp(&position(&b.to)))
        });

        let towns = towns::find(content, &mut |wcid| weenies.get(wcid), &portals, &places);
        for t in &towns {
            places.push(Place {
                kind: match t.kind {
                    TownKind::Town => PlaceKind::Town,
                    TownKind::Landmark => PlaceKind::Landmark,
                },
                wcid: None,
                name: t.name.clone(),
                at: t.at,
                to: None,
                guid: 0,
            });
        }
        places.sort_by(|a, b| {
            (a.at.cell, a.kind, a.wcid.unwrap_or(0), a.guid)
                .cmp(&(b.at.cell, b.kind, b.wcid.unwrap_or(0), b.guid))
                .then_with(|| position(&a.at).cmp(&position(&b.at)))
                .then_with(|| a.name.cmp(&b.name))
        });
        Self {
            portals,
            places,
            towns,
        }
    }

    /// The portal table, for the world named `world_name`, its knowledge from `source`.
    #[must_use]
    pub fn portal_table(&self, world_name: &str, source: Source) -> String {
        let mut out = String::from(HEADER);
        out.push_str("world\t");
        out.push_str(&field(world_name));
        out.push('\n');
        let level = |v: Option<i32>| v.map_or_else(|| "-".to_owned(), |v| v.to_string());
        for e in &self.portals {
            out.push_str(&format!(
                "portal\t{}\t{}\t{}\t{}\t{}\t0\t0\t0\t{}\t{}\t-\t{}\t{}\n",
                source.word(),
                e.wcid,
                field(&e.name),
                position(&e.from),
                position(&e.to),
                level(e.min_level),
                level(e.max_level),
                field(&write_field(&e.requires)),
                e.shape.map_or_else(|| "-".to_owned(), Shape::field),
            ));
        }
        out
    }

    /// The places, for the world named `world_name`, their knowledge from `source`.
    #[must_use]
    pub fn places_document(&self, world_name: &str, source: Source) -> String {
        let mut out = String::from(PLACES_HEADER);
        out.push_str("world\t");
        out.push_str(&field(world_name));
        out.push('\n');
        for p in &self.places {
            out.push_str(&format!(
                "place\t{}\t{}\t{}\t{}\t{}\t0\t0\t-\t{}\n",
                p.kind.word(),
                source.word(),
                p.wcid.map_or_else(|| "-".to_owned(), |w| w.to_string()),
                field(&p.name),
                position(&p.at),
                p.to.as_ref().map_or_else(|| "-".to_owned(), position),
            ));
        }
        out
    }

    /// Both documents, for the world named `world_name`, their knowledge from `source`.
    #[must_use]
    pub fn documents(&self, world_name: &str, source: Source) -> Documents {
        Documents {
            portals: self.portal_table(world_name, source),
            places: self.places_document(world_name, source),
        }
    }
}

/// The portal table of `content` for the world named `world_name`, as its own server publishes
/// it.
#[must_use]
pub fn portal_table(world_name: &str, content: &dyn WorldSource) -> String {
    WorldPlaces::of(content).portal_table(world_name, Source::Published)
}

/// The places of `content` for the world named `world_name`, as its own server publishes them.
#[must_use]
pub fn places(world_name: &str, content: &dyn WorldSource) -> String {
    WorldPlaces::of(content).places_document(world_name, Source::Published)
}

/// What spawners ask of the world before they make anything.
struct Gate<'a> {
    /// Each event the database names (lower case), and whether it runs unless switched off.
    events: &'a HashMap<String, bool>,
}

impl Gate<'_> {
    /// What the spawner `w` asks of the world, added to `asks`.
    fn of(&self, w: &Weenie, asks: &mut Vec<Requirement>) {
        if w.spawns.is_empty() {
            return;
        }
        if let Some(name) = w
            .string(id::string::GENERATOR_EVENT)
            .filter(|n| !n.trim().is_empty())
        {
            // A spawner naming an event the database does not hold runs as any other.
            if let Some(&on) = self.events.get(&name.to_ascii_lowercase()) {
                asks.push(Requirement::Event {
                    name: name.to_owned(),
                    on,
                });
            }
        }
        match w.int(id::int::GENERATOR_TIME_TYPE) {
            Some(4) => asks.push(Requirement::Night),
            Some(5) => asks.push(Requirement::Day),
            _ => {}
        }
    }
}

/// The portals the spawner `w` makes standing at `at` (placed as, or under, instance `guid`),
/// and those the spawners it makes make in turn, added to `out`; `asks` is what the spawners above
/// ask of the world.
#[allow(clippy::too_many_arguments)]
fn spawned(
    weenies: &mut Weenies<'_>,
    gate: &Gate<'_>,
    w: &Weenie,
    at: &Spot,
    guid: u32,
    asks: &[Requirement],
    depth: usize,
    out: &mut Vec<Portal>,
) {
    let mut seen = HashSet::new();
    for s in &w.spawns {
        let in_world = spawn_where::ON_TOP | spawn_where::SCATTER | spawn_where::SPECIFIC;
        if s.where_create & in_world == 0
            || s.where_create & (spawn_where::CONTAIN | spawn_where::WIELD | spawn_where::SHOP) != 0
            || !seen.insert((
                s.wcid,
                s.where_create,
                s.at.cell,
                s.at.origin.map(f32::to_bits),
            ))
        {
            continue;
        }
        let Some(c) = weenies.get(s.wcid) else {
            continue;
        };
        let spot = if s.where_create & spawn_where::SPECIFIC != 0 {
            if s.at.cell != 0 {
                s.at
            } else {
                Spot {
                    rotation: at.rotation,
                    ..moved(at, rotate(at.rotation, s.at.origin))
                }
            }
        } else if s.where_create & spawn_where::SCATTER != 0 && s.at.cell == 0 {
            moved(at, s.at.origin)
        } else {
            *at
        };
        let spot = landed(&spot);
        if c.weenie_type == id::kind::PORTAL {
            if let Some(to) = destination(&c, &spot) {
                out.push(portal(&c, &spot, to, Standing::Spawned(guid), asks));
            }
        }
        if !c.spawns.is_empty() && depth < SPAWN_DEPTH {
            let mut more = asks.to_vec();
            gate.of(&c, &mut more);
            spawned(weenies, gate, &c, &spot, guid, &more, depth + 1, out);
        }
    }
}

/// The portal of weenie `w` standing at `from`, leading to `to`.
fn portal(w: &Weenie, from: &Spot, to: Spot, standing: Standing, world: &[Requirement]) -> Portal {
    let mut requires = asks(w);
    requires.extend(world.iter().cloned());
    requires.sort();
    requires.dedup();
    Portal {
        wcid: w.class_id,
        name: w.name().to_owned(),
        from: landed(from),
        to,
        min_level: w.int(id::int::MIN_LEVEL),
        max_level: w.int(id::int::MAX_LEVEL),
        requires,
        standing,
        hidden: w.flag(id::bool::UI_HIDDEN) == Some(true),
        shape: Shape::of(w),
    }
}

/// What the portal `w` asks of the character who uses it, and of the world for itself.
fn asks(w: &Weenie) -> Vec<Requirement> {
    let mut out = Vec::new();
    // A portal that does not say whom it admits admits anyone.
    let bits = w.int(id::int::PORTAL_BITMASK).unwrap_or(1);
    if bits == 0 {
        out.push(Requirement::Closed);
    }
    for (bit, r) in [
        (0x2, Requirement::NoPk),
        (0x4, Requirement::NoPkLite),
        (0x8, Requirement::NoNpk),
        (0x40, Requirement::OlthoiOnly),
        (0x80, Requirement::NoOlthoi),
        (0x100, Requirement::NoVitae),
        (0x200, Requirement::AccountAge),
    ] {
        if bits & bit != 0 {
            out.push(r);
        }
    }
    #[allow(clippy::cast_sign_loss)]
    if let Some(n) = w.int(id::int::ACCOUNT_REQUIREMENTS).filter(|&n| n > 0) {
        out.push(Requirement::Account(n as u32));
    }
    if w.flag(id::bool::ADVOCATE_QUEST) == Some(true) {
        out.push(Requirement::Advocate);
    }
    if let Some(q) = w
        .string(id::string::QUEST_RESTRICTION)
        .filter(|q| !q.trim().is_empty())
    {
        out.push(Requirement::Quest(q.to_owned()));
    }
    if w.int(id::int::LIFESPAN).is_some_and(|l| l > 0) {
        out.push(Requirement::Temporary);
    }
    out
}

/// One placed instance's place, less where a portal leads; `None` for a weenie hidden from the
/// interface, and for an `npc` that is no creature or may be attacked.
fn place(kind: PlaceKind, placed: &Placement, weenie: &Weenie) -> Option<Place> {
    /// The item type's creature bit.
    const CREATURE: i32 = 0x10;
    if weenie.flag(id::bool::UI_HIDDEN) == Some(true) {
        return None;
    }
    if kind == PlaceKind::Npc {
        let item_type = weenie.int(id::int::ITEM_TYPE).unwrap_or(0);
        // A creature may be attacked unless its weenie says it may not.
        if item_type & CREATURE == 0 || weenie.flag(id::bool::ATTACKABLE) != Some(false) {
            return None;
        }
    }
    Some(Place {
        kind,
        wcid: Some(placed.wcid),
        name: weenie.name().to_owned(),
        at: landed(&placed.at),
        to: None,
        guid: placed.guid,
    })
}

/// Where the portal `w` standing at `at` leads by its own weenie: its destination, else its
/// offset from where it stands; `None` when it says neither.
pub(crate) fn destination(w: &Weenie, at: &Spot) -> Option<Spot> {
    if let Some((_, d)) = w
        .positions
        .iter()
        .find(|(k, p)| *k == id::position::DESTINATION && p.cell != 0)
    {
        return Some(landed(d));
    }
    let offset = w.position(id::position::RELATIVE_DESTINATION)?;
    Some(landed(&moved(at, offset.origin)))
}

/// `at` with a bare landblock written as its land cell.
pub(crate) fn landed(at: &Spot) -> Spot {
    Spot {
        cell: land_cell(at.cell, at.origin[0], at.origin[1]),
        ..*at
    }
}

/// `at` moved by `by`. Outdoors, the point is carried into the landblock and land cell it moves
/// into; indoors it keeps its cell.
fn moved(at: &Spot, by: [f32; 3]) -> Spot {
    let mut origin = [
        at.origin[0] + by[0],
        at.origin[1] + by[1],
        at.origin[2] + by[2],
    ];
    if at.indoors() {
        return Spot { origin, ..*at };
    }
    let mut bx = at.cell >> 24;
    let mut by_ = (at.cell >> 16) & 0xFF;
    let carry = |v: &mut f32, b: &mut u32| {
        while *v < 0.0 && *b > 0 {
            *v += 192.0;
            *b -= 1;
        }
        while *v >= 192.0 && *b < 0xFF {
            *v -= 192.0;
            *b += 1;
        }
    };
    carry(&mut origin[0], &mut bx);
    carry(&mut origin[1], &mut by_);
    Spot {
        cell: land_cell((bx << 24) | (by_ << 16), origin[0], origin[1]),
        origin,
        rotation: at.rotation,
    }
}

/// `v` turned by the rotation `q` (`w`, `x`, `y`, `z`).
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = q;
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let u = [x, y, z];
    let t = cross(u, v).map(|c| 2.0 * c);
    let ut = cross(u, t);
    [
        v[0] + w * t[0] + ut[0],
        v[1] + w * t[1] + ut[1],
        v[2] + w * t[2] + ut[2],
    ]
}

#[cfg(test)]
#[path = "documents_tests.rs"]
mod tests;
