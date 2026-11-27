//! The places and portals a plugin searches (`nav.search`), indexed once for each state of the
//! client's stores rather than read whole for each search: a world's era brings thousands of
//! them, and a search is asked for at each key the player types.
//!
//! - **By name.** Every word of every name, lower case, in one sorted list: the places whose
//!   names have a word beginning with the first word searched for are found by halving the list,
//!   and only they are looked at further. A search that goes on from the last one (a key more)
//!   looks only at what the last one found.
//! - **Nearest first, with no name.** The places by the landblock they stand in: read ring by ring
//!   out from where the search is measured from, until the nearest asked for are certain.
//! - **Where in the world.** A dungeon's landblock is no place on the land, so what stands in a
//!   dungeon is measured where the dungeon is: where a portal into it stands. From inside one, what
//!   is in it comes first, measured along it, and then the rest, from where it is.
//!
//! The index is made at the start of a frame, outside any plugin's share of it, once a plugin has
//! searched or one that plans journeys (`nav.journey`) is active, so the first search does not pay
//! for making it (a whole era's places take longer than an entry's budget); places the client
//! learns as it plays are added to it rather than its being made again.

use std::collections::{HashMap, HashSet};

use dereth_client_contract::panels::map::MapPlace;
use dereth_navigation::{PlaceStore, PortalStore};
use dereth_primitives::Position;

/// The side of the squares the places are kept by, metres: a landblock's.
const SQUARE: f32 = 192.0;

/// Where an indexed entry comes from, by its index in that list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Src {
    Portal(u32),
    Map(u32),
    Place(u32),
}

impl Src {
    const fn portal(self) -> bool {
        matches!(self, Self::Portal(_))
    }
}

/// The lists a search is over, as the frame has them.
#[derive(Clone, Copy, Default)]
pub(crate) struct Stores<'a> {
    pub portals: Option<&'a PortalStore>,
    pub places: Option<&'a PlaceStore>,
    pub map: Option<&'a [MapPlace]>,
}

/// Which state of the stores an index was made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    portals: (usize, u64, usize),
    places: (usize, u64, usize),
    map: (usize, usize),
}

impl Key {
    fn of(s: &Stores<'_>) -> Self {
        Self {
            portals: s.portals.map_or((0, 0, 0), |p| {
                (
                    std::ptr::from_ref(p) as usize,
                    p.revision(),
                    p.portals().len(),
                )
            }),
            places: s.places.map_or((0, 0, 0), |p| {
                (
                    std::ptr::from_ref(p) as usize,
                    p.revision(),
                    p.places().len(),
                )
            }),
            map: s.map.map_or((0, 0), |m| (m.as_ptr() as usize, m.len())),
        }
    }
}

/// One place or portal, as the index keeps it.
#[derive(Debug)]
struct Indexed {
    /// Its name, lower case; and as the store has it, to tell when it changed.
    name: Box<str>,
    written: Box<str>,
    kind: &'static str,
    at: Position,
    /// Where it is in the world: where it stands, or where its dungeon is.
    world: Position,
    src: Src,
}

/// The landblock a place is in.
fn block(p: &Position) -> u16 {
    u16::try_from(p.cell.0 >> 16).unwrap_or(u16::MAX)
}

/// Whether a place is indoors: in a dungeon or a building.
fn inside(p: &Position) -> bool {
    !crate::places::outdoors(p.cell)
}

/// Where each dungeon is in the world, by its landblock: where a portal into it from elsewhere
/// stands, one on the land before one in another dungeon (whose own place is taken in turn). A
/// landblock with a portal standing outdoors in it is land, whose buildings are where they stand.
#[derive(Debug, Default)]
struct Dungeons(HashMap<u16, Position>);

impl Dungeons {
    /// How many dungeons entered from another dungeon are followed out to the land.
    const DEPTH: usize = 4;

    fn of(portals: Option<&PortalStore>) -> Self {
        let Some(portals) = portals else {
            return Self::default();
        };
        let edges = || portals.portals().iter().filter(|e| e.retired.is_none());
        let land: HashSet<u16> = edges()
            .filter(|e| !inside(&e.from))
            .map(|e| block(&e.from))
            .collect();
        let mut ways: HashMap<u16, Position> = HashMap::new();
        for e in edges() {
            let Some(to) = e.to.filter(inside) else {
                continue;
            };
            let b = block(&to);
            if land.contains(&b) || block(&e.from) == b {
                continue;
            }
            let way = ways.entry(b).or_insert(e.from);
            if inside(way) && !inside(&e.from) {
                *way = e.from;
            }
        }
        let world = ways
            .keys()
            .map(|&b| {
                let mut at = ways[&b];
                for _ in 0..Self::DEPTH {
                    match ways.get(&block(&at)) {
                        Some(out) if inside(&at) => at = *out,
                        _ => break,
                    }
                }
                (b, at)
            })
            .collect();
        Self(world)
    }

    /// Where `p` is in the world: where it stands, or in a dungeon where the dungeon is.
    fn world(&self, p: &Position) -> Position {
        if inside(p) {
            if let Some(at) = self.0.get(&block(p)) {
                return *at;
            }
        }
        *p
    }

    /// The landblock of the dungeon `p` is in, when it is in one whose place is known.
    fn of_place(&self, p: &Position) -> Option<u16> {
        (inside(p) && self.0.contains_key(&block(p))).then(|| block(p))
    }
}

/// Where a search measures from: the place, where it is in the world, and the dungeon it is in.
struct Measure {
    at: Position,
    world: Position,
    dungeon: Option<u16>,
}

impl Measure {
    /// How far entry `e` is: along the dungeon when both are in it, else between where each is in
    /// the world; and whether it is outside the dungeon measured from (what is in it comes first).
    fn far(&self, e: &Indexed) -> (bool, f32) {
        if self
            .dungeon
            .is_some_and(|d| inside(&e.at) && block(&e.at) == d)
        {
            (false, dereth_navigation::global_distance(&self.at, &e.at))
        } else {
            (
                self.dungeon.is_some(),
                dereth_navigation::global_distance(&self.world, &e.world),
            )
        }
    }
}

/// The last search by name, for one that goes on from it.
#[derive(Debug)]
struct Last {
    text: String,
    kinds: Option<Vec<String>>,
    portals: bool,
    places: bool,
    matches: Vec<u32>,
}

/// What a search asks for.
#[derive(Debug, Default)]
pub(crate) struct Query {
    /// The words searched for, lower case; none for every place.
    pub words: Vec<String>,
    /// Places of these kinds only, when given; the portals are portals.
    pub kinds: Option<Vec<String>>,
    /// Whether the portals are searched, and the places.
    pub portals: bool,
    pub places: bool,
    /// Measured from here, nearest first; within this many metres of it, when given.
    pub near: Option<Position>,
    pub within: Option<f32>,
    /// The most to answer.
    pub limit: usize,
}

/// What a search found: the nearest it was asked for, and how many there are in all; when it was
/// measured, each one's distance and where in the world it was measured from.
#[derive(Debug, PartialEq)]
pub(crate) struct Found {
    pub items: Vec<Src>,
    pub total: usize,
    pub far: Vec<f32>,
    pub from: Option<Position>,
}

/// The index of a frame's stores.
#[derive(Debug, Default)]
pub(crate) struct PlaceIndex {
    key: Option<Key>,
    entries: Vec<Indexed>,
    /// Every word of every name, lower case, with its entry; in order.
    words: Vec<(Box<str>, u32)>,
    /// The entries by the square they are in, in the world.
    squares: HashMap<(i32, i32), Vec<u32>>,
    /// Where the dungeons are in the world.
    dungeons: Dungeons,
    /// How many entries there are of each kind, portals and places apart.
    counts: HashMap<(bool, &'static str), usize>,
    last: Option<Last>,
    /// Where the places' entries begin, and how many there are.
    places_from: usize,
    places_len: usize,
    /// A plugin has searched: the index is kept up at the start of each frame.
    wanted: bool,
    /// How many entries the searches have looked at, how many times the index was made whole,
    /// and how many places were added to it as they came.
    pub touched: usize,
    pub built: usize,
    pub added: usize,
}

/// The words of what a plugin searched for, lower case, as a name's are read.
pub(crate) fn query_words(text: &str) -> Vec<String> {
    words(&text.to_lowercase()).map(str::to_owned).collect()
}

/// A name's words, lower case: what letters, digits and apostrophes run together.
fn words(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
}

/// Where `p` is in the world, across, for the squares.
fn across(p: &Position) -> (f32, f32) {
    let g = dereth_navigation::global(p);
    (g.x, g.y)
}

fn square_of(p: &Position) -> (i32, i32) {
    let (x, y) = across(p);
    (
        dereth_primitives::num::floor_to_i32(x / SQUARE),
        dereth_primitives::num::floor_to_i32(y / SQUARE),
    )
}

impl PlaceIndex {
    /// The index of `stores`, brought up to them when they have changed since.
    pub fn of(&mut self, stores: &Stores<'_>) -> &mut Self {
        self.wanted = true;
        self.keep_up(stores);
        self
    }

    /// At the start of a frame: the index brought up to `stores`, when a plugin has searched or
    /// `searching` says one may.
    pub fn refresh(&mut self, stores: &Stores<'_>, searching: bool) {
        self.wanted |= searching;
        if self.wanted {
            self.keep_up(stores);
        }
    }

    fn keep_up(&mut self, stores: &Stores<'_>) {
        let key = Key::of(stores);
        let Some(old) = self.key else {
            self.build(stores);
            self.key = Some(key);
            return;
        };
        if old == key {
            return;
        }
        // The places alone changed, in the store they were in: those learned are added, and those
        // that moved are moved; one whose name changed has the index made again.
        let grown = old.portals == key.portals
            && old.map == key.map
            && old.places.0 == key.places.0
            && key.places.2 >= self.places_len;
        if !(grown && stores.places.is_some_and(|p| self.add_places(p))) {
            self.build(stores);
        }
        self.key = Some(key);
    }

    /// Bring the places' entries up to `store`, which has only grown: answers false when one of
    /// them changed its name, which the words would not follow.
    fn add_places(&mut self, store: &PlaceStore) -> bool {
        let places = store.places();
        for (k, p) in places.iter().take(self.places_len).enumerate() {
            let i = self.places_from + k;
            if *self.entries[i].written != *p.name {
                return false;
            }
            if self.entries[i].at != p.at {
                let world = self.dungeons.world(&p.at);
                let (from, to) = (square_of(&self.entries[i].world), square_of(&world));
                if from != to {
                    let id = index(i);
                    if let Some(v) = self.squares.get_mut(&from) {
                        v.retain(|&e| e != id);
                    }
                    self.squares.entry(to).or_default().push(id);
                }
                self.entries[i].at = p.at;
                self.entries[i].world = world;
            }
        }
        for (k, p) in places.iter().enumerate().skip(self.places_len) {
            let id = index(self.entries.len());
            self.entries.push(Indexed {
                name: p.name.to_lowercase().into_boxed_str(),
                written: p.name.as_str().into(),
                kind: p.kind.word(),
                at: p.at,
                world: self.dungeons.world(&p.at),
                src: Src::Place(index(k)),
            });
            let e = &self.entries[id as usize];
            for w in words(&e.name) {
                let w: Box<str> = w.into();
                let at = self.words.partition_point(|x| (&x.0, x.1) < (&w, id));
                self.words.insert(at, (w, id));
            }
            self.squares
                .entry(square_of(&e.world))
                .or_default()
                .push(id);
            *self.counts.entry((false, e.kind)).or_default() += 1;
            self.added += 1;
        }
        self.places_len = places.len();
        self.last = None;
        true
    }

    fn build(&mut self, stores: &Stores<'_>) {
        self.built += 1;
        self.entries.clear();
        self.words.clear();
        self.squares.clear();
        self.counts.clear();
        self.last = None;
        self.dungeons = Dungeons::of(stores.portals);
        let dungeons = &self.dungeons;
        let add = |entries: &mut Vec<Indexed>, name: &str, kind, at: Position, src| {
            entries.push(Indexed {
                name: name.to_lowercase().into_boxed_str(),
                written: name.into(),
                kind,
                at,
                world: dungeons.world(&at),
                src,
            });
        };
        if let Some(p) = stores.portals {
            for (i, e) in p.portals().iter().enumerate() {
                add(
                    &mut self.entries,
                    &e.name,
                    "portal",
                    e.from,
                    Src::Portal(index(i)),
                );
            }
        }
        if let Some(m) = stores.map {
            for (i, p) in m.iter().enumerate() {
                if let Some(at) = crate::places::from_map(p.ns, p.ew, 0.0) {
                    add(
                        &mut self.entries,
                        &p.name,
                        p.kind.word(),
                        at,
                        Src::Map(index(i)),
                    );
                }
            }
        }
        self.places_from = self.entries.len();
        self.places_len = stores.places.map_or(0, |s| s.places().len());
        if let Some(s) = stores.places {
            for (i, p) in s.places().iter().enumerate() {
                // A lever is no place to go: it is what opens a door on the way.
                if p.kind == dereth_navigation::PlaceKind::Lever {
                    continue;
                }
                add(
                    &mut self.entries,
                    &p.name,
                    p.kind.word(),
                    p.at,
                    Src::Place(index(i)),
                );
            }
        }
        for (i, e) in self.entries.iter().enumerate() {
            let i = index(i);
            for w in words(&e.name) {
                self.words.push((w.into(), i));
            }
            self.squares.entry(square_of(&e.world)).or_default().push(i);
            *self.counts.entry((e.src.portal(), e.kind)).or_default() += 1;
        }
        self.words.sort_unstable();
    }

    /// Whether entry `e` is of what `q` asks for, but for its name.
    fn of_kind(e: &Indexed, q: &Query) -> bool {
        Self::counted(e.src.portal(), e.kind, q)
    }

    /// Whether an entry of the portals (`portal`) or of the places, of `kind`, is what `q` asks
    /// for, but for its name: `q`'s kinds are of places, and the portals are portals.
    fn counted(portal: bool, kind: &str, q: &Query) -> bool {
        if portal {
            q.portals
        } else {
            q.places && q.kinds.as_ref().is_none_or(|k| k.iter().any(|k| k == kind))
        }
    }

    /// Whether every word searched for begins a word of `e`'s name.
    fn named(e: &Indexed, q: &Query) -> bool {
        q.words
            .iter()
            .all(|w| words(&e.name).any(|n| n.starts_with(w.as_str())))
    }

    /// The entries one of whose words begins with `w`.
    fn beginning(&self, w: &str) -> Vec<u32> {
        let from = self.words.partition_point(|(n, _)| &**n < w);
        let mut out: Vec<u32> = self.words[from..]
            .iter()
            .take_while(|(n, _)| n.starts_with(w))
            .map(|&(_, i)| i)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Where `q` measures from, when it is measured.
    fn measure(&self, q: &Query) -> Option<Measure> {
        q.near.map(|at| Measure {
            at,
            world: self.dungeons.world(&at),
            dungeon: self.dungeons.of_place(&at),
        })
    }

    /// What `kept` holds (each whether it is outside the dungeon measured from, its distance and
    /// its entry), as found.
    fn found(&self, kept: Vec<(bool, f32, u32)>, total: usize, from: Option<&Measure>) -> Found {
        Found {
            far: if from.is_some() {
                kept.iter().map(|k| k.1).collect()
            } else {
                Vec::new()
            },
            items: kept
                .into_iter()
                .map(|(_, _, i)| self.entries[i as usize].src)
                .collect(),
            total,
            from: from.map(|f| f.world),
        }
    }

    /// The nearest `q` asks for, and how many there are.
    pub fn search(&mut self, q: &Query) -> Found {
        if q.words.is_empty() {
            return self.browse(q);
        }
        let text = q.words.join(" ");
        let goes_on = self.last.as_ref().is_some_and(|l| {
            text.starts_with(l.text.as_str())
                && l.kinds == q.kinds
                && l.portals == q.portals
                && l.places == q.places
        });
        let base = match self.last.take() {
            Some(l) if goes_on => l.matches,
            _ => self.beginning(&q.words[0]),
        };
        self.touched += base.len();
        let matches: Vec<u32> = base
            .into_iter()
            .filter(|&i| {
                let e = &self.entries[i as usize];
                Self::of_kind(e, q) && Self::named(e, q)
            })
            .collect();
        let found = self.nearest(&matches, q);
        self.last = Some(Last {
            text,
            kinds: q.kinds.clone(),
            portals: q.portals,
            places: q.places,
            matches,
        });
        found
    }

    /// The nearest of `ids` (all of what `q` asks for), and how many of them are within reach:
    /// those named exactly as searched for first, however far, then what is in the dungeon
    /// measured from, then the rest.
    fn nearest(&self, ids: &[u32], q: &Query) -> Found {
        let from = self.measure(q);
        let mut kept: Vec<(bool, bool, f32, u32)> = ids
            .iter()
            .filter_map(|&i| {
                let e = &self.entries[i as usize];
                let (away, d) = from.as_ref().map_or((false, 0.0), |f| f.far(e));
                let exact =
                    !q.words.is_empty() && words(&e.name).eq(q.words.iter().map(String::as_str));
                q.within
                    .is_none_or(|w| d <= w)
                    .then_some((!exact, away, d, i))
            })
            .collect();
        let total = kept.len();
        let order = |a: &(bool, bool, f32, u32), b: &(bool, bool, f32, u32)| {
            a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.total_cmp(&b.2))
        };
        if q.limit > 0 && q.limit < kept.len() {
            kept.select_nth_unstable_by(q.limit - 1, order);
            kept.truncate(q.limit);
        }
        kept.sort_by(order);
        let kept = kept.into_iter().map(|(_, a, d, i)| (a, d, i)).collect();
        self.found(kept, total, from.as_ref())
    }

    /// With no name: the nearest of the kinds asked for, the squares read ring by ring out from
    /// where it is measured; and how many there are of them in all.
    fn browse(&mut self, q: &Query) -> Found {
        let total = self
            .counts
            .iter()
            .filter(|((portal, kind), _)| Self::counted(*portal, kind, q))
            .map(|(_, n)| n)
            .sum();
        let Some(from) = self.measure(q) else {
            let kept: Vec<(bool, f32, u32)> = (0..index(self.entries.len()))
                .filter(|&i| Self::of_kind(&self.entries[i as usize], q))
                .take(q.limit.max(1))
                .map(|i| (false, 0.0, i))
                .collect();
            self.touched += self.entries.len();
            return self.found(kept, total, None);
        };
        // What is in the dungeon measured from is where the dungeon is: all in the first square.
        let (cx, cy) = square_of(&from.world);
        let rings = self
            .squares
            .keys()
            .map(|&(x, y)| (x - cx).abs().max((y - cy).abs()))
            .max()
            .unwrap_or(0);
        let order =
            |a: &(bool, f32, u32), b: &(bool, f32, u32)| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1));
        let mut kept: Vec<(bool, f32, u32)> = Vec::new();
        for ring in 0..=rings {
            // What this ring holds is at least this far: a square less than its distance out.
            #[allow(clippy::cast_precision_loss)]
            let beyond = (ring - 1).max(0) as f32 * SQUARE;
            if kept.len() >= q.limit.max(1) {
                kept.sort_by(order);
                kept.truncate(q.limit.max(1));
                if kept.last().is_some_and(|k| !k.0 || k.1 <= beyond) {
                    break;
                }
            }
            if q.within.is_some_and(|w| w < beyond) {
                break;
            }
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    if dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    for &i in self.squares.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                        self.touched += 1;
                        let e = &self.entries[i as usize];
                        if !Self::of_kind(e, q) {
                            continue;
                        }
                        let (away, d) = from.far(e);
                        if q.within.is_none_or(|w| d <= w) {
                            kept.push((away, d, i));
                        }
                    }
                }
            }
        }
        kept.sort_by(order);
        kept.truncate(q.limit.max(1));
        self.found(kept, total, Some(&from))
    }
}

/// An index into a list, as the index keeps it.
fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the host's own index of the places a plugin searches).
    use super::*;
    use dereth_navigation::{EdgeSource, KnownPlace, PlaceKind, PortalEdge};
    use dereth_primitives::{CellId, Frame, Quat, Vec3};

    /// An outdoor place in landblock `(x, y)`.
    fn at(x: u32, y: u32) -> Position {
        Position::new(
            CellId((((x << 8) | y) << 16) | 1),
            Frame::new(Vec3::new(12.0, 12.0, 0.0), Quat::IDENTITY),
        )
    }

    /// A world as big as the end of retail's package: 5,300 portals and 11,300 places, every
    /// 53rd portal a Drudge Hideout.
    fn world() -> (PortalStore, PlaceStore) {
        let mut portals = PortalStore::new("W");
        portals.merge((0..5300_u32).map(|i| PortalEdge {
            source: EdgeSource::Hint,
            wcid: Some(i),
            name: if i % 53 == 0 {
                format!("Drudge Hideout {i}")
            } else {
                format!("Portal to Place {i}")
            },
            from: at(i % 200, i / 200),
            to: None,
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        }));
        let mut places = PlaceStore::new("W");
        places.merge((0..11_300_u32).map(|i| KnownPlace {
            kind: if i % 2 == 0 {
                PlaceKind::Npc
            } else {
                PlaceKind::Vendor
            },
            source: EdgeSource::Hint,
            wcid: Some(100_000 + i),
            name: format!("Person {i}"),
            at: at(i % 200, i / 200 + 40),
            first_seen: 0,
            last_seen: 0,
            owner: None,
            to: None,
        }));
        (portals, places)
    }

    fn query(text: &str, limit: usize) -> Query {
        Query {
            words: query_words(text),
            kinds: None,
            portals: true,
            places: true,
            near: Some(at(0, 0)),
            within: None,
            limit,
        }
    }

    #[test]
    fn a_name_search_looks_only_at_the_places_whose_words_begin_with_it() {
        let (portals, places) = world();
        let stores = Stores {
            portals: Some(&portals),
            places: Some(&places),
            map: None,
        };
        let mut index = PlaceIndex::default();
        let found = index.of(&stores).search(&query("hideout", 8));
        assert_eq!(found.total, 100, "every Drudge Hideout");
        assert_eq!(found.items.len(), 8);
        assert_eq!(index.touched, 100, "the hundred, not the 16,600");
        // The nearest first, as measuring every one would have them.
        let mut by_hand: Vec<(f32, u32)> = portals
            .portals()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name.starts_with("Drudge Hideout"))
            .map(|(i, e)| {
                (
                    dereth_navigation::global_distance(&at(0, 0), &e.from),
                    index_of(i),
                )
            })
            .collect();
        by_hand.sort_by(|a, b| a.0.total_cmp(&b.0));
        let expected: Vec<Src> = by_hand
            .iter()
            .take(8)
            .map(|&(_, i)| Src::Portal(i))
            .collect();
        assert_eq!(found.items, expected);
        // A key more: only what the last search found is looked at.
        let before = index.touched;
        let more = index.of(&stores).search(&query("hideout 10", 8));
        assert_eq!(index.touched - before, 100);
        assert_eq!(
            more.total, 3,
            "the Hideouts whose number begins 10: 106, 1007 and 1060"
        );
        // Two words, either order of the name's words, and no case.
        assert_eq!(index.of(&stores).search(&query("DRUDGE hid", 8)).total, 100);
        assert_eq!(
            index.of(&stores).search(&query("person 113", 8)).total,
            11,
            "Person 113 and 1130 to 1139"
        );
        assert_eq!(index.of(&stores).search(&query("nobody", 8)).total, 0);
        assert_eq!(
            index.built, 1,
            "the index is made once while the stores stay as they are"
        );
    }

    #[test]
    fn a_place_named_exactly_as_searched_for_comes_first_however_far() {
        let edge = |name: &str, x: u32| PortalEdge {
            source: EdgeSource::Hint,
            wcid: Some(x),
            name: name.to_owned(),
            from: at(x, 0),
            to: None,
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        };
        let mut portals = PortalStore::new("W");
        portals.merge([
            edge("Holtburg Casino VIP Portal", 1),
            edge("Portal to Holtburg", 2),
            edge("Holtburg", 90),
        ]);
        let stores = Stores {
            portals: Some(&portals),
            places: None,
            map: None,
        };
        let mut index = PlaceIndex::default();
        let found = index.of(&stores).search(&query("Holtburg", 1));
        assert_eq!(found.total, 3);
        assert_eq!(found.items, [Src::Portal(2)], "the far one named Holtburg");
        let all = index.of(&stores).search(&query("holtburg", 8));
        assert_eq!(all.items, [Src::Portal(2), Src::Portal(0), Src::Portal(1)]);
    }

    #[test]
    fn with_no_name_the_nearest_come_from_the_squares_round_the_character() {
        let (portals, places) = world();
        let stores = Stores {
            portals: Some(&portals),
            places: Some(&places),
            map: None,
        };
        let mut index = PlaceIndex::default();
        let found = index.of(&stores).search(&query("", 8));
        assert_eq!(found.total, 16_600);
        assert_eq!(found.items.len(), 8);
        assert!(
            index.touched < 100,
            "a few squares, not the world: {}",
            index.touched
        );
        // The same as measuring every one would choose, by distance.
        let d = |s: &Src| match *s {
            Src::Portal(i) => {
                dereth_navigation::global_distance(&at(0, 0), &portals.portals()[i as usize].from)
            }
            Src::Place(i) => {
                dereth_navigation::global_distance(&at(0, 0), &places.places()[i as usize].at)
            }
            Src::Map(_) => f32::MAX,
        };
        let mut all: Vec<f32> = portals
            .portals()
            .iter()
            .map(|e| dereth_navigation::global_distance(&at(0, 0), &e.from))
            .chain(
                places
                    .places()
                    .iter()
                    .map(|p| dereth_navigation::global_distance(&at(0, 0), &p.at)),
            )
            .collect();
        all.sort_by(f32::total_cmp);
        let got: Vec<f32> = found.items.iter().map(d).collect();
        assert_eq!(got, all[..8].to_vec());
        // Of one kind only.
        let vendors = index.of(&stores).search(&Query {
            kinds: Some(vec![String::from("vendor")]),
            portals: false,
            ..query("", 3)
        });
        assert_eq!(vendors.total, 5650);
        assert!(vendors
            .items
            .iter()
            .all(|s| matches!(s, Src::Place(i) if i % 2 == 1)));
    }

    #[test]
    fn the_index_is_made_again_when_a_store_changes() {
        let (mut portals, places) = world();
        let mut index = PlaceIndex::default();
        let total = |index: &mut PlaceIndex, portals: &PortalStore| {
            let stores = Stores {
                portals: Some(portals),
                places: Some(&places),
                map: None,
            };
            index.of(&stores).search(&query("arwic", 8)).total
        };
        assert_eq!(total(&mut index, &portals), 0);
        portals.merge([PortalEdge {
            source: EdgeSource::Learned,
            wcid: Some(9999),
            name: String::from("Portal to Arwic"),
            from: at(3, 3),
            to: None,
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        }]);
        assert_eq!(
            total(&mut index, &portals),
            1,
            "the portal learned is found"
        );
        assert_eq!(index.built, 2);
    }

    #[test]
    fn places_learned_in_play_are_added_to_the_index_not_made_again() {
        let (portals, mut places) = world();
        let mut index = PlaceIndex::default();
        let found = |index: &mut PlaceIndex, places: &PlaceStore| {
            let stores = Stores {
                portals: Some(&portals),
                places: Some(places),
                map: None,
            };
            index.of(&stores).search(&query("grocer", 8)).total
        };
        assert_eq!(found(&mut index, &places), 0);
        // A grocer seen; a person seen again, moved.
        places.see(KnownPlace {
            kind: PlaceKind::Vendor,
            source: EdgeSource::Learned,
            wcid: Some(7),
            name: String::from("Grocer Alma"),
            at: at(1, 1),
            first_seen: 10,
            last_seen: 10,
            owner: None,
            to: None,
        });
        assert_eq!(found(&mut index, &places), 1, "the grocer is found");
        assert_eq!((index.built, index.added), (1, 1), "added, not made again");
        // The nearest still nearest first: the grocer at the start's next landblock.
        let near = index
            .of(&Stores {
                portals: Some(&portals),
                places: Some(&places),
                map: None,
            })
            .search(&Query {
                kinds: Some(vec![String::from("vendor")]),
                portals: false,
                ..query("", 1)
            });
        assert_eq!(
            near.items,
            [Src::Place(index_of(places.places().len() - 1))]
        );
        // At the start of a frame, a plugin having searched, the index keeps up by itself.
        places.see(KnownPlace {
            kind: PlaceKind::Npc,
            source: EdgeSource::Learned,
            wcid: Some(8),
            name: String::from("Grocer Bede"),
            at: at(2, 2),
            first_seen: 11,
            last_seen: 11,
            owner: None,
            to: None,
        });
        index.refresh(
            &Stores {
                portals: Some(&portals),
                places: Some(&places),
                map: None,
            },
            false,
        );
        assert_eq!(index.added, 2);
        let before = index.touched;
        assert_eq!(found(&mut index, &places), 2);
        assert_eq!(
            index.touched - before,
            2,
            "a search after it looks at its two matches only"
        );
    }

    #[test]
    fn a_plugin_that_may_search_has_the_index_made_at_the_frame_start_not_in_its_first_search() {
        let portals = PortalStore::new("w");
        let places = PlaceStore::new("w");
        let stores = Stores {
            portals: Some(&portals),
            places: Some(&places),
            map: None,
        };
        let mut index = PlaceIndex::default();
        index.refresh(&stores, false);
        assert_eq!(index.built, 0, "no plugin searches");
        index.refresh(&stores, true);
        assert_eq!(index.built, 1, "made at the frame start");
        index.of(&stores);
        assert_eq!(index.built, 1, "the search finds it made");
    }

    /// A place indoors, in cell `cell` of landblock `(x, y)`, at `(ox, oy)` in its frame.
    fn indoors(x: u32, y: u32, cell: u32, (ox, oy): (f32, f32)) -> Position {
        Position::new(
            CellId((((x << 8) | y) << 16) | cell),
            Frame::new(Vec3::new(ox, oy, 0.0), Quat::IDENTITY),
        )
    }

    fn portal(name: &str, from: Position, to: Position) -> PortalEdge {
        PortalEdge {
            source: EdgeSource::Hint,
            wcid: None,
            name: name.to_owned(),
            from,
            to: Some(to),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        }
    }

    fn person(name: &str, at: Position) -> KnownPlace {
        KnownPlace {
            kind: PlaceKind::Npc,
            source: EdgeSource::Hint,
            wcid: None,
            name: name.to_owned(),
            at,
            first_seen: 0,
            last_seen: 0,
            owner: None,
            to: None,
        }
    }

    /// From inside a dungeon, what is in it comes first, measured along it, then the rest, from
    /// where the dungeon is in the world: its portal's place on the land. The dungeons in the
    /// landblocks beside its own are as far as their own portals; and a building, in land where
    /// portals stand, is where it stands.
    #[test]
    fn from_inside_a_dungeon_the_search_measures_from_where_the_dungeon_is_in_the_world() {
        // The dungeon in landblock 0x5C45, its portal on the land at 0x1010 and its exit; the one
        // beside it, 0x5D45, entered far away at 0xC0C0 by way of another dungeon, 0x7070.
        let door = at(0x10, 0x10);
        let start = indoors(0x5C, 0x45, 0x100, (10.0, -10.0));
        let mut portals = PortalStore::new("W");
        portals.merge([
            portal("Tusker Abode", door, start),
            portal(
                "Surface",
                indoors(0x5C, 0x45, 0x120, (50.0, -10.0)),
                at(0x10, 0x11),
            ),
            portal(
                "Hall",
                at(0xC0, 0xC0),
                indoors(0x70, 0x70, 0x100, (0.0, 0.0)),
            ),
            portal(
                "Deeper Catacombs",
                indoors(0x70, 0x70, 0x110, (5.0, 5.0)),
                indoors(0x5D, 0x45, 0x100, (5.0, 5.0)),
            ),
            // A town's portal, and one that sends into a shop of the town.
            portal("Town Portal", at(0x20, 0x20), at(0x30, 0x30)),
            portal(
                "Shop Portal",
                at(0x40, 0x40),
                indoors(0x20, 0x20, 0x105, (30.0, 30.0)),
            ),
        ]);
        let mut places = PlaceStore::new("W");
        places.merge([
            person("Rolling Ball", indoors(0x5D, 0x45, 0x101, (6.0, 6.0))),
            person("Tusker Guard", indoors(0x5C, 0x45, 0x101, (20.0, -10.0))),
            person("Healer", at(0x11, 0x10)),
            person("Grocer", indoors(0x20, 0x20, 0x106, (30.0, 30.0))),
        ]);
        let stores = Stores {
            portals: Some(&portals),
            places: Some(&places),
            map: None,
        };
        let mut index = PlaceIndex::default();
        let names = |found: &Found| -> Vec<String> {
            found
                .items
                .iter()
                .map(|s| match *s {
                    Src::Portal(i) => portals.portals()[i as usize].name.clone(),
                    Src::Place(i) => places.places()[i as usize].name.clone(),
                    Src::Map(_) => String::new(),
                })
                .collect()
        };
        let from_start = |text: &str, limit| Query {
            near: Some(start),
            ..query(text, limit)
        };
        let found = index.of(&stores).search(&from_start("", 5));
        assert_eq!(
            names(&found),
            [
                "Tusker Guard",
                "Surface",
                "Tusker Abode",
                "Healer",
                "Town Portal"
            ],
            "{found:?}"
        );
        assert_eq!(found.from, Some(door), "measured from the dungeon's portal");
        let along =
            dereth_navigation::global_distance(&start, &indoors(0x5C, 0x45, 0x101, (20.0, -10.0)));
        assert_eq!(found.far[0], along, "along the dungeon");
        assert_eq!(found.far[2], 0.0, "its own portal is where it is");
        assert_eq!(
            found.far[3],
            dereth_navigation::global_distance(&door, &at(0x11, 0x10))
        );
        // By name, the same: the guard first, then from the dungeon's portal.
        let found = index.of(&stores).search(&from_start("tusker", 8));
        assert_eq!(names(&found), ["Tusker Guard", "Tusker Abode"]);
        // The dungeon beside is where its way in is, past the other dungeon: farther than the town.
        let far = index.of(&stores).search(&from_start("", 20));
        let rolling = names(&far).iter().position(|n| n == "Rolling Ball");
        let town = names(&far).iter().position(|n| n == "Town Portal");
        assert!(rolling.is_some() && rolling > town, "{:?}", names(&far));
        assert_eq!(
            index
                .dungeons
                .world(&indoors(0x5D, 0x45, 0x101, (6.0, 6.0))),
            at(0xC0, 0xC0)
        );
        // In the town's shop, the grocer is where he stands: the town is land.
        assert_eq!(
            index
                .dungeons
                .world(&indoors(0x20, 0x20, 0x106, (30.0, 30.0))),
            indoors(0x20, 0x20, 0x106, (30.0, 30.0))
        );
        // Outdoors nothing changes: the nearest from where the character stands.
        let outside = index.of(&stores).search(&Query {
            near: Some(at(0x11, 0x10)),
            ..query("", 1)
        });
        assert_eq!(names(&outside), ["Healer"]);
        assert_eq!(outside.from, Some(at(0x11, 0x10)));
    }

    fn index_of(i: usize) -> u32 {
        index(i)
    }
}
