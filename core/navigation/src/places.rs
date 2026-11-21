//! The places of one world that this client has learned: the lifestones, portals, vendors and
//! people the server put in front of the character, and the houses whose owners it was told.
//!
//! A world's places are the server's own: no two servers place the same things, and none of
//! this is in the data files. So the client keeps what each world showed it, in a store of its
//! own per world (the world's name and the server's address), beside that world's portal
//! store, which holds where each portal sends. Each place says where it came from, as a portal
//! edge does ([`EdgeSource`]): what the character saw, or what the server publishes, or a hint.
//!
//! Only things the server placed in the world are kept: never a player, nor anything carried.
//! A place is never forgotten for not being seen again, since a session sees only the ground it
//! covers; seeing it again updates it.
//!
//! The store is text, one place a line, so it diffs and survives being edited by hand; see
//! [`PlaceStore::to_text`].

use std::collections::HashMap;
use std::fmt::Write as _;

use dereth_primitives::Position;

use crate::path::global_distance;
use crate::portals::{escape, opt, read_opt, read_position, unescape, write_position};
use crate::{EdgeSource, PortalSighting, StoreError};

/// The place store's format version, the first line's second field.
pub const PLACES_VERSION: u32 = 1;

/// The object-description flags a place is told by.
mod flag {
    pub const PLAYER: u32 = 0x0000_0008;
    pub const ATTACKABLE: u32 = 0x0000_0010;
    pub const UI_HIDDEN: u32 = 0x0000_0080;
    pub const VENDOR: u32 = 0x0000_0200;
    pub const LIFESTONE: u32 = 0x0000_4000;
    pub const PORTAL: u32 = 0x0004_0000;
}

/// The item type's creature bit.
const CREATURE: u32 = 0x0000_0010;

/// What a place is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaceKind {
    Lifestone,
    /// A portal, where it stands; where it sends is the portal store's.
    Portal,
    Vendor,
    /// A person who is no vendor and cannot be attacked.
    Npc,
    /// A house, known by its crystal, with its owner when it has one.
    House,
    /// A town, at its drop point: where a portal to it leaves a character.
    Town,
    /// A named place that is no town, at its drop point.
    Landmark,
    /// What works a door (a lever, a switch), with where the door stands as its `to`: a door
    /// that cannot be used opens only so. Only ever published.
    Lever,
}

impl PlaceKind {
    /// Every kind, in the order they are listed.
    pub const ALL: [Self; 8] = [
        Self::Lifestone,
        Self::Portal,
        Self::Vendor,
        Self::Npc,
        Self::House,
        Self::Town,
        Self::Landmark,
        Self::Lever,
    ];

    /// The kind as one word: `lifestone`, `portal`, `vendor`, `npc`, `house`, `town`, `landmark`
    /// or `lever`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Lifestone => "lifestone",
            Self::Portal => "portal",
            Self::Vendor => "vendor",
            Self::Npc => "npc",
            Self::House => "house",
            Self::Town => "town",
            Self::Landmark => "landmark",
            Self::Lever => "lever",
        }
    }

    /// Read [`Self::word`].
    #[must_use]
    pub fn parse(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.word() == w)
    }

    /// The place an object the server created is, from its description's flags and item type;
    /// `None` for anything else (a player, an item, a monster, a door). A house is not told
    /// by its description: it is learned from what the server says of it when its crystal is
    /// used. A town and a landmark are never seen: they are only ever published.
    #[must_use]
    pub const fn of(bitfield: u32, item_type: u32) -> Option<Self> {
        if bitfield & (flag::PLAYER | flag::UI_HIDDEN) != 0 {
            None
        } else if bitfield & flag::LIFESTONE != 0 {
            Some(Self::Lifestone)
        } else if bitfield & flag::PORTAL != 0 {
            Some(Self::Portal)
        } else if bitfield & flag::VENDOR != 0 {
            Some(Self::Vendor)
        } else if item_type & CREATURE != 0 && bitfield & flag::ATTACKABLE == 0 {
            Some(Self::Npc)
        } else {
            None
        }
    }
}

/// One place: what it is, its name, where it stands and what is known about it.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownPlace {
    pub kind: PlaceKind,
    /// Where this knowledge came from.
    pub source: EdgeSource,
    /// Its weenie class, when known.
    pub wcid: Option<u32>,
    pub name: String,
    /// Where it stands.
    pub at: Position,
    /// When it was first and last seen, in seconds since 1970.
    pub first_seen: u64,
    pub last_seen: u64,
    /// A house's owner, by name; `None` for a house for sale, and for anything else.
    pub owner: Option<String>,
    /// Where a portal was last learned to send the character, when it was used.
    pub to: Option<Position>,
}

impl KnownPlace {
    /// Whether a place of `kind` and class `wcid` standing at `at` is this one: the same kind
    /// and class (when both know it), within three metres.
    #[must_use]
    pub fn same_place(&self, kind: PlaceKind, wcid: Option<u32>, at: &Position) -> bool {
        let class = match (self.wcid, wcid) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        self.kind == kind && class && global_distance(&self.at, at) < 3.0
    }
}

/// The landblock a place stands in.
const fn block(at: &Position) -> u32 {
    at.cell.0 >> 16
}

/// One world's places.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceStore {
    /// The world these places are of: the server's own identity, never mixed with another's.
    pub world: String,
    places: Vec<KnownPlace>,
    /// Each kind's places by the landblock they stand in, so a place is found among its
    /// neighbours alone: a world's server may publish thousands.
    index: HashMap<(PlaceKind, u32), Vec<usize>>,
    /// Counts the changes, so a reader can tell when to read again.
    revision: u64,
}

impl PlaceStore {
    /// An empty store for `world`.
    #[must_use]
    pub fn new(world: impl Into<String>) -> Self {
        Self {
            world: world.into(),
            ..Self::default()
        }
    }

    /// The store of `places`, in that order.
    fn of(world: String, places: Vec<KnownPlace>, revision: u64) -> Self {
        let mut index: HashMap<(PlaceKind, u32), Vec<usize>> = HashMap::new();
        for (i, p) in places.iter().enumerate() {
            index.entry((p.kind, block(&p.at))).or_default().push(i);
        }
        Self {
            world,
            places,
            index,
            revision,
        }
    }

    /// The first place kept that `place` is ([`KnownPlace::same_place`]), looked for in its
    /// landblock and the eight beside it.
    fn find(&self, place: &KnownPlace) -> Option<usize> {
        let b = block(&place.at);
        let (x, y) = (b >> 8, b & 0xFF);
        let mut found: Option<usize> = None;
        for nx in x.saturating_sub(1)..=(x + 1).min(0xFF) {
            for ny in y.saturating_sub(1)..=(y + 1).min(0xFF) {
                let Some(near) = self.index.get(&(place.kind, nx << 8 | ny)) else {
                    continue;
                };
                for &i in near {
                    if found.is_none_or(|f| i < f)
                        && self.places[i].same_place(place.kind, place.wcid, &place.at)
                    {
                        found = Some(i);
                    }
                }
            }
        }
        found
    }

    /// Every place, every source.
    #[must_use]
    pub fn places(&self) -> &[KnownPlace] {
        &self.places
    }

    /// Goes up by one each time the store changes.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// The character saw `place` at `now`: kept, or the place already kept updated (its name,
    /// its owner, when it was last seen, and its source, now that it is seen). Whether
    /// anything a reader shows changed.
    pub fn see(&mut self, place: KnownPlace) -> bool {
        let Some(i) = self.find(&place) else {
            self.index
                .entry((place.kind, block(&place.at)))
                .or_default()
                .push(self.places.len());
            self.places.push(place);
            self.revision += 1;
            return true;
        };
        let p = &mut self.places[i];
        let was_in = block(&p.at);
        let before = (p.name.clone(), p.owner.clone(), p.to, p.source, p.wcid);
        p.last_seen = p.last_seen.max(place.last_seen);
        // A place the server told of was never seen: its first sighting is this one.
        if p.first_seen == 0 {
            p.first_seen = place.first_seen;
        }
        let wins = place.source >= p.source;
        if wins {
            p.name = place.name;
            p.owner = place.owner;
            p.at = place.at;
            p.source = place.source;
        }
        p.wcid = p.wcid.or(place.wcid);
        // Where a portal leads is taken from a source no less trusted, or when nothing knew it.
        if place.to.is_some() && (wins || p.to.is_none()) {
            p.to = place.to;
        }
        let shown = before != (p.name.clone(), p.owner.clone(), p.to, p.source, p.wcid);
        let (kind, now_in) = (p.kind, block(&p.at));
        if now_in != was_in {
            if let Some(v) = self.index.get_mut(&(kind, was_in)) {
                v.retain(|&j| j != i);
            }
            let v = self.index.entry((kind, now_in)).or_default();
            v.push(i);
            v.sort_unstable();
        }
        if shown {
            self.revision += 1;
        }
        shown
    }

    /// Take in published or seeded places, each as [`Self::see`] takes it, but never over a
    /// place the character saw itself.
    pub fn merge(&mut self, places: impl IntoIterator<Item = KnownPlace>) {
        for p in places {
            self.see(p);
        }
    }

    /// The store less the places of one source.
    #[must_use]
    pub fn without(&self, source: EdgeSource) -> Self {
        Self::of(
            self.world.clone(),
            self.places
                .iter()
                .filter(|p| p.source != source)
                .cloned()
                .collect(),
            self.revision,
        )
    }

    /// The store as text:
    ///
    /// ```text
    /// dereth-places-store<TAB>1
    /// world<TAB><identity>
    /// place<TAB>kind<TAB>source<TAB>wcid<TAB>name<TAB>at<TAB>first seen<TAB>last seen<TAB>owner<TAB>to
    /// ```
    ///
    /// A position is `0xCELLID x y z`; an absent value is `-`; names have their tabs, line
    /// breaks and backslashes escaped.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "dereth-places-store\t{PLACES_VERSION}\nworld\t{}\n",
            escape(&self.world)
        );
        for p in &self.places {
            let _ = writeln!(
                out,
                "place\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                p.kind.word(),
                p.source.word(),
                opt(p.wcid),
                escape(&p.name),
                write_position(&p.at),
                p.first_seen,
                p.last_seen,
                p.owner.as_deref().map_or_else(|| String::from("-"), escape),
                p.to.as_ref()
                    .map_or_else(|| String::from("-"), write_position),
            );
        }
        out
    }

    /// Read a store written by [`Self::to_text`]. A line whose first field this client does not
    /// know, and a place of a kind it does not know, are passed over, so a later client's or
    /// server's store still reads.
    ///
    /// # Errors
    /// [`StoreError`] for text that is not a place store, a newer version, or a `place` line
    /// that does not read.
    pub fn from_text(text: &str) -> Result<Self, StoreError> {
        let mut lines = text.lines().enumerate();
        let (_, first) = lines.next().ok_or(StoreError::NotAStore)?;
        let mut head = first.split('\t');
        if head.next() != Some("dereth-places-store") {
            return Err(StoreError::NotAStore);
        }
        let version: u32 = head
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or(StoreError::NotAStore)?;
        if version > PLACES_VERSION {
            return Err(StoreError::Version(version));
        }
        let mut store = Self::default();
        for (n, line) in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            let bad = |what: &str| StoreError::Line(n + 1, what.to_owned());
            match f.first().copied() {
                Some("world") => store.world = unescape(f.get(1).copied().unwrap_or("")),
                Some("place") if f.len() == 10 => {
                    let Some(kind) = PlaceKind::parse(f[1]) else {
                        continue;
                    };
                    store.places.push(KnownPlace {
                        kind,
                        source: EdgeSource::parse(f[2]).ok_or_else(|| bad("source"))?,
                        wcid: read_opt(f[3]).map_err(|()| bad("wcid"))?,
                        name: unescape(f[4]),
                        at: read_position(f[5]).ok_or_else(|| bad("at"))?,
                        first_seen: f[6].parse().map_err(|_| bad("first seen"))?,
                        last_seen: f[7].parse().map_err(|_| bad("last seen"))?,
                        owner: (f[8] != "-").then(|| unescape(f[8])),
                        to: if f[9] == "-" {
                            None
                        } else {
                            Some(read_position(f[9]).ok_or_else(|| bad("to"))?)
                        },
                    });
                }
                Some("place") => return Err(bad("not a place line")),
                _ => {}
            }
        }
        Ok(Self::of(store.world, store.places, store.revision))
    }
}

/// Watches for where a portal the player used sends the character: the use, then a teleport
/// begun at the portal, then the landing. Any other teleport (a recall, the server's) is not a
/// portal's and teaches nothing; a use that no teleport follows within [`Self::WAIT`] is
/// forgotten, as is one the player walked away from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortalUseWatch {
    /// The portal used and when, in seconds of the caller's clock.
    used: Option<(PortalSighting, f64)>,
    /// The portal whose teleport is under way, and where the body stood as it began.
    going: Option<(PortalSighting, Position)>,
}

impl PortalUseWatch {
    /// How long after a use its teleport may begin, in seconds: the server walks the
    /// character to the portal first.
    pub const WAIT: f64 = 20.0;
    /// How near the portal the body stands when its teleport begins, in metres.
    pub const NEAR: f32 = 10.0;
    /// How far the body must land from where it stood, in metres, for the teleport to have
    /// taken it.
    pub const MOVED: f32 = 15.0;

    /// The player used `portal` at `now`.
    pub fn used(&mut self, portal: PortalSighting, now: f64) {
        self.used = Some((portal, now));
    }

    /// The body at `at` at `now`, in portal space or not. Answers the portal used and where
    /// it landed the character, once it has.
    pub fn frame(
        &mut self,
        at: &Position,
        in_portal_space: bool,
        now: f64,
    ) -> Option<(PortalSighting, Position)> {
        if let Some((_, before)) = &self.going {
            if in_portal_space {
                return None;
            }
            let landed = global_distance(at, before) > Self::MOVED;
            let (portal, _) = self.going.take()?;
            return landed.then_some((portal, *at));
        }
        let (portal, when) = self.used.as_ref()?;
        if now - when > Self::WAIT {
            self.used = None;
            return None;
        }
        if in_portal_space {
            let near = global_distance(at, &portal.position) <= Self::NEAR;
            let (portal, _) = self.used.take()?;
            if near {
                self.going = Some((portal, *at));
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
