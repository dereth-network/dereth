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
//!
//! The index is made at the start of a frame, outside any plugin's share of it, once a plugin has
//! searched; places the client learns as it plays are added to it rather than its being made
//! again.

use std::collections::HashMap;

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
    src: Src,
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

/// What a search found: the nearest it was asked for, and how many there are in all.
#[derive(Debug, PartialEq)]
pub(crate) struct Found {
    pub items: Vec<Src>,
    pub total: usize,
}

/// The index of a frame's stores.
#[derive(Debug, Default)]
pub(crate) struct PlaceIndex {
    key: Option<Key>,
    entries: Vec<Indexed>,
    /// Every word of every name, lower case, with its entry; in order.
    words: Vec<(Box<str>, u32)>,
    /// The entries by the square they stand in.
    squares: HashMap<(i32, i32), Vec<u32>>,
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

    /// At the start of a frame: the index brought up to `stores`, when a plugin has searched.
    pub fn refresh(&mut self, stores: &Stores<'_>) {
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
                let (from, to) = (square_of(&self.entries[i].at), square_of(&p.at));
                if from != to {
                    let id = index(i);
                    if let Some(v) = self.squares.get_mut(&from) {
                        v.retain(|&e| e != id);
                    }
                    self.squares.entry(to).or_default().push(id);
                }
                self.entries[i].at = p.at;
            }
        }
        for (k, p) in places.iter().enumerate().skip(self.places_len) {
            let id = index(self.entries.len());
            self.entries.push(Indexed {
                name: p.name.to_lowercase().into_boxed_str(),
                written: p.name.as_str().into(),
                kind: p.kind.word(),
                at: p.at,
                src: Src::Place(index(k)),
            });
            let e = &self.entries[id as usize];
            for w in words(&e.name) {
                let w: Box<str> = w.into();
                let at = self.words.partition_point(|x| (&x.0, x.1) < (&w, id));
                self.words.insert(at, (w, id));
            }
            self.squares.entry(square_of(&e.at)).or_default().push(id);
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
        let add = |entries: &mut Vec<Indexed>, name: &str, kind, at: Position, src| {
            entries.push(Indexed {
                name: name.to_lowercase().into_boxed_str(),
                written: name.into(),
                kind,
                at,
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
            self.squares.entry(square_of(&e.at)).or_default().push(i);
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

    /// The nearest of `ids` (all of what `q` asks for), and how many of them are within reach.
    fn nearest(&self, ids: &[u32], q: &Query) -> Found {
        let mut kept: Vec<(f32, u32)> = ids
            .iter()
            .filter_map(|&i| {
                let e = &self.entries[i as usize];
                let d = q
                    .near
                    .as_ref()
                    .map_or(0.0, |n| dereth_navigation::global_distance(n, &e.at));
                q.within.is_none_or(|w| d <= w).then_some((d, i))
            })
            .collect();
        let total = kept.len();
        if q.limit > 0 && q.limit < kept.len() {
            kept.select_nth_unstable_by(q.limit - 1, |a, b| a.0.total_cmp(&b.0));
            kept.truncate(q.limit);
        }
        kept.sort_by(|a, b| a.0.total_cmp(&b.0));
        Found {
            items: kept
                .into_iter()
                .map(|(_, i)| self.entries[i as usize].src)
                .collect(),
            total,
        }
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
        let Some(near) = q.near else {
            let ids: Vec<u32> = (0..index(self.entries.len()))
                .filter(|&i| Self::of_kind(&self.entries[i as usize], q))
                .take(q.limit.max(1))
                .collect();
            self.touched += self.entries.len();
            return Found {
                items: ids
                    .into_iter()
                    .map(|i| self.entries[i as usize].src)
                    .collect(),
                total,
            };
        };
        let (cx, cy) = square_of(&near);
        let rings = self
            .squares
            .keys()
            .map(|&(x, y)| (x - cx).abs().max((y - cy).abs()))
            .max()
            .unwrap_or(0);
        let mut kept: Vec<(f32, u32)> = Vec::new();
        for ring in 0..=rings {
            // What this ring holds is at least this far: a square less than its distance out.
            #[allow(clippy::cast_precision_loss)]
            let beyond = (ring - 1).max(0) as f32 * SQUARE;
            if kept.len() >= q.limit.max(1) {
                kept.sort_by(|a, b| a.0.total_cmp(&b.0));
                kept.truncate(q.limit.max(1));
                if kept.last().is_some_and(|k| k.0 <= beyond) {
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
                        let d = dereth_navigation::global_distance(&near, &e.at);
                        if q.within.is_none_or(|w| d <= w) {
                            kept.push((d, i));
                        }
                    }
                }
            }
        }
        kept.sort_by(|a, b| a.0.total_cmp(&b.0));
        kept.truncate(q.limit.max(1));
        Found {
            items: kept
                .into_iter()
                .map(|(_, i)| self.entries[i as usize].src)
                .collect(),
            total,
        }
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
        index.refresh(&Stores {
            portals: Some(&portals),
            places: Some(&places),
            map: None,
        });
        assert_eq!(index.added, 2);
        let before = index.touched;
        assert_eq!(found(&mut index, &places), 2);
        assert_eq!(
            index.touched - before,
            2,
            "a search after it looks at its two matches only"
        );
    }

    fn index_of(i: usize) -> u32 {
        index(i)
    }
}
