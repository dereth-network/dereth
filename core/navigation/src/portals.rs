//! What this client knows about one world's portals and its characters' recalls: learned by using
//! them, published by the server, or seeded as hints, and never assumed permanent.
//!
//! A world's portals are not fixed: each server has its own, and they change over time. So the
//! store is kept per world, every edge says where it came from and when it was last confirmed, and
//! what the character sees overrides what it was told:
//!
//! | Source | Confidence | Replaced by |
//! |---|---|---|
//! | [`EdgeSource::Learned`]: the character used it and arrived | highest | a later use |
//! | [`EdgeSource::Published`]: the server's own table | middle | anything learned |
//! | [`EdgeSource::Hint`]: a seeded table for a known content version | lowest | anything else |
//!
//! A use that lands somewhere else updates the edge to where it landed; a use that is refused
//! retires it, with the reason, until a later use succeeds. Recalls are kept per character, and
//! only ever learned: where a recall sends a character is the server's business.
//!
//! The store is text, one record a line, so it diffs and survives being edited by hand; see
//! [`PortalStore::to_text`].

use std::fmt::Write as _;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::path::global_distance;

/// The store's format version, the first line's second field.
pub const STORE_VERSION: u32 = 1;

/// Where an edge came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeSource {
    /// A seeded table for a known content version: least trusted.
    Hint,
    /// The server's own table.
    Published,
    /// The character used it and arrived: most trusted.
    Learned,
}

impl EdgeSource {
    /// The source as one word: `hint`, `published` or `learned`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Published => "published",
            Self::Learned => "learned",
        }
    }

    fn parse(w: &str) -> Option<Self> {
        match w {
            "hint" => Some(Self::Hint),
            "published" => Some(Self::Published),
            "learned" => Some(Self::Learned),
            _ => None,
        }
    }

    /// How far a route trusts an edge from this source, 0 to 1.
    #[must_use]
    pub const fn confidence(self) -> f32 {
        match self {
            Self::Hint => 0.5,
            Self::Published => 0.9,
            Self::Learned => 1.0,
        }
    }
}

/// A portal as the character sees it standing in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalSighting {
    /// The portal's weenie class, when known.
    pub wcid: Option<u32>,
    /// Its name.
    pub name: String,
    /// Where it stands.
    pub position: Position,
}

/// One portal: where it stands, where it sends, and what is known about it.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalEdge {
    /// Where this knowledge came from.
    pub source: EdgeSource,
    /// The portal's weenie class, when known.
    pub wcid: Option<u32>,
    /// Its name.
    pub name: String,
    /// Where it stands.
    pub from: Position,
    /// Where it sends, when known.
    pub to: Option<Position>,
    /// When it was last confirmed, in seconds since 1970; `0` for never.
    pub verified: u64,
    /// Uses that arrived.
    pub uses: u32,
    /// Uses that were refused or landed elsewhere.
    pub failures: u32,
    /// The lowest and highest character level it admits, when known.
    pub levels: (Option<i32>, Option<i32>),
    /// Why it is not used, when it is retired: the last refusal.
    pub retired: Option<String>,
}

impl PortalEdge {
    /// Whether `other` is the same portal: the same class (when both know it) standing within
    /// three metres.
    #[must_use]
    pub fn same_portal(&self, wcid: Option<u32>, at: &Position) -> bool {
        let class = match (self.wcid, wcid) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        class && global_distance(&self.from, at) < 3.0
    }

    /// Whether a character of `level` may use it, as far as is known.
    #[must_use]
    pub fn admits(&self, level: Option<i32>) -> bool {
        let Some(l) = level else { return true };
        self.levels.0.is_none_or(|min| l >= min) && self.levels.1.is_none_or(|max| l <= max)
    }
}

/// A recall: a spell (cast, or cast by an item), or one every character has by command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Recall {
    /// A recall spell; an item that casts it recalls to the same place.
    Spell(u32),
    /// The command that sends a character to the lifestone it last used.
    Lifestone,
    /// The command that sends a character to the marketplace.
    Marketplace,
}

impl Recall {
    /// The recall as one store field: `spell <id>`, `lifestone` or `marketplace`.
    #[must_use]
    pub fn word(self) -> String {
        match self {
            Self::Spell(s) => format!("spell {s}"),
            Self::Lifestone => String::from("lifestone"),
            Self::Marketplace => String::from("marketplace"),
        }
    }

    /// Read [`Self::word`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "lifestone" => Some(Self::Lifestone),
            "marketplace" => Some(Self::Marketplace),
            _ => s.strip_prefix("spell ")?.parse().ok().map(Self::Spell),
        }
    }
}

/// Where a character's recall sent it.
#[derive(Debug, Clone, PartialEq)]
pub struct RecallEdge {
    /// The character.
    pub character: String,
    /// The recall.
    pub recall: Recall,
    /// Where it sent the character, the last time.
    pub to: Position,
    /// When, in seconds since 1970.
    pub verified: u64,
    /// Times it was used.
    pub uses: u32,
}

/// Why a store could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The first line is not this format.
    #[error("not a navigation store")]
    NotAStore,
    /// A later format version.
    #[error("store version {0} is newer than this client's {STORE_VERSION}")]
    Version(u32),
    /// A line that does not read.
    #[error("line {0}: {1}")]
    Line(usize, String),
}

/// One world's portal and recall knowledge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortalStore {
    /// The world this knowledge is about: the server's own identity, never mixed with another's.
    pub world: String,
    portals: Vec<PortalEdge>,
    recalls: Vec<RecallEdge>,
}

/// A position as one field: `0xCELLID x y z`.
fn write_position(p: &Position) -> String {
    let o = p.frame.origin;
    format!("0x{:08X} {:.3} {:.3} {:.3}", p.cell.0, o.x, o.y, o.z)
}

fn read_position(s: &str) -> Option<Position> {
    let mut w = s.split(' ');
    let cell = u32::from_str_radix(w.next()?.trim_start_matches("0x"), 16).ok()?;
    let x = w.next()?.parse().ok()?;
    let y = w.next()?.parse().ok()?;
    let z = w.next()?.parse().ok()?;
    Some(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
    ))
}

/// A free-text field with its tabs and line breaks escaped.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| String::from("-"), |v| v.to_string())
}

fn read_opt<T: std::str::FromStr>(s: &str) -> Result<Option<T>, ()> {
    if s == "-" {
        Ok(None)
    } else {
        s.parse().map(Some).map_err(|_| ())
    }
}

impl PortalStore {
    /// An empty store for `world`.
    #[must_use]
    pub fn new(world: impl Into<String>) -> Self {
        Self {
            world: world.into(),
            portals: Vec::new(),
            recalls: Vec::new(),
        }
    }

    /// Every portal edge, every source, retired ones included.
    #[must_use]
    pub fn portals(&self) -> &[PortalEdge] {
        &self.portals
    }

    /// Every recall edge.
    #[must_use]
    pub fn recalls(&self) -> &[RecallEdge] {
        &self.recalls
    }

    /// The store less the edges of one source: what to keep for a world when that source's
    /// table is kept apart (hints, taken in afresh from their own file each time).
    #[must_use]
    pub fn without(&self, source: EdgeSource) -> Self {
        Self {
            world: self.world.clone(),
            portals: self
                .portals
                .iter()
                .filter(|e| e.source != source)
                .cloned()
                .collect(),
            recalls: self.recalls.clone(),
        }
    }

    fn find_mut(
        &mut self,
        source: EdgeSource,
        wcid: Option<u32>,
        at: &Position,
    ) -> Option<&mut PortalEdge> {
        self.portals
            .iter_mut()
            .find(|e| e.source == source && e.same_portal(wcid, at))
    }

    /// The character used `portal` at `now` and arrived at `arrival`: the learned edge is
    /// confirmed, or corrected to where it really sends, or made.
    pub fn learn_use(&mut self, portal: &PortalSighting, arrival: &Position, now: u64) {
        if let Some(e) = self.find_mut(EdgeSource::Learned, portal.wcid, &portal.position) {
            let moved = e.to.is_none_or(|t| global_distance(&t, arrival) > 10.0);
            if moved {
                e.failures += u32::from(e.to.is_some());
                e.to = Some(*arrival);
            }
            e.uses += 1;
            e.verified = now;
            e.retired = None;
            e.name.clone_from(&portal.name);
            return;
        }
        self.portals.push(PortalEdge {
            source: EdgeSource::Learned,
            wcid: portal.wcid,
            name: portal.name.clone(),
            from: portal.position,
            to: Some(*arrival),
            verified: now,
            uses: 1,
            failures: 0,
            levels: (None, None),
            retired: None,
        });
    }

    /// The character used `portal` at `now` and it refused (`reason`): every edge for that
    /// portal is retired until a use succeeds, so routes stop counting on it.
    pub fn learn_refusal(&mut self, portal: &PortalSighting, reason: &str, now: u64) {
        let mut learned = false;
        for e in &mut self.portals {
            if e.same_portal(portal.wcid, &portal.position) {
                e.failures += 1;
                e.retired = Some(reason.to_owned());
                e.verified = now;
                learned |= e.source == EdgeSource::Learned;
            }
        }
        // The refusal is the character's own knowledge, kept even where only a hint or the
        // server's table spoke of the portal before.
        if !learned {
            self.portals.push(PortalEdge {
                source: EdgeSource::Learned,
                wcid: portal.wcid,
                name: portal.name.clone(),
                from: portal.position,
                to: None,
                verified: now,
                uses: 0,
                failures: 1,
                levels: (None, None),
                retired: Some(reason.to_owned()),
            });
        }
    }

    /// Take in published or seeded edges. An edge replaces one of the same source for the same
    /// portal; it never replaces a more trusted one, which stays beside it and wins at routing.
    pub fn merge(&mut self, edges: impl IntoIterator<Item = PortalEdge>) {
        for e in edges {
            if let Some(old) = self.find_mut(e.source, e.wcid, &e.from) {
                *old = e;
            } else {
                self.portals.push(e);
            }
        }
    }

    /// The edges a route may use: per portal the most trusted edge that knows a destination,
    /// unless that portal is retired by any of its edges, or does not admit `level`.
    #[must_use]
    pub fn routable(&self, level: Option<i32>) -> Vec<&PortalEdge> {
        let mut out: Vec<&PortalEdge> = Vec::new();
        for e in &self.portals {
            let same: Vec<&PortalEdge> = self
                .portals
                .iter()
                .filter(|o| o.same_portal(e.wcid, &e.from))
                .collect();
            if same.iter().any(|o| o.retired.is_some()) {
                continue;
            }
            let Some(best) = same
                .iter()
                .filter(|o| o.to.is_some())
                .max_by_key(|o| (o.source, o.verified))
            else {
                continue;
            };
            if !std::ptr::eq(*best, e) || !best.admits(level) {
                continue;
            }
            out.push(e);
        }
        out
    }

    /// `character`'s `recall` sent it to `arrival` at `now`.
    pub fn learn_recall(&mut self, character: &str, recall: Recall, arrival: &Position, now: u64) {
        if let Some(r) = self
            .recalls
            .iter_mut()
            .find(|r| r.character == character && r.recall == recall)
        {
            r.to = *arrival;
            r.verified = now;
            r.uses += 1;
            return;
        }
        self.recalls.push(RecallEdge {
            character: character.to_owned(),
            recall,
            to: *arrival,
            verified: now,
            uses: 1,
        });
    }

    /// Where `character`'s `recall` sent it last.
    #[must_use]
    pub fn recall_destination(&self, character: &str, recall: Recall) -> Option<Position> {
        self.recalls
            .iter()
            .find(|r| r.character == character && r.recall == recall)
            .map(|r| r.to)
    }

    /// The store as text:
    ///
    /// ```text
    /// dereth-navigation-store<TAB>1
    /// world<TAB><identity>
    /// portal<TAB>source<TAB>wcid<TAB>name<TAB>from<TAB>to<TAB>verified<TAB>uses<TAB>failures<TAB>min<TAB>max<TAB>retired
    /// recall<TAB>character<TAB>recall<TAB>to<TAB>verified<TAB>uses
    /// ```
    ///
    /// A position is `0xCELLID x y z`; an absent value is `-`; names and reasons have their tabs,
    /// line breaks and backslashes escaped.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "dereth-navigation-store\t{STORE_VERSION}\nworld\t{}\n",
            escape(&self.world)
        );
        for e in &self.portals {
            let _ = writeln!(
                out,
                "portal\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                e.source.word(),
                opt(e.wcid),
                escape(&e.name),
                write_position(&e.from),
                e.to.as_ref()
                    .map_or_else(|| String::from("-"), write_position),
                e.verified,
                e.uses,
                e.failures,
                opt(e.levels.0),
                opt(e.levels.1),
                e.retired
                    .as_deref()
                    .map_or_else(|| String::from("-"), escape),
            );
        }
        for r in &self.recalls {
            let _ = writeln!(
                out,
                "recall\t{}\t{}\t{}\t{}\t{}",
                escape(&r.character),
                r.recall.word(),
                write_position(&r.to),
                r.verified,
                r.uses
            );
        }
        out
    }

    /// Read a store written by [`Self::to_text`].
    ///
    /// # Errors
    /// [`StoreError`] for text that is not a store, a newer version, or a line that does not read.
    pub fn from_text(text: &str) -> Result<Self, StoreError> {
        let mut lines = text.lines().enumerate();
        let (_, first) = lines.next().ok_or(StoreError::NotAStore)?;
        let mut head = first.split('\t');
        if head.next() != Some("dereth-navigation-store") {
            return Err(StoreError::NotAStore);
        }
        let version: u32 = head
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or(StoreError::NotAStore)?;
        if version > STORE_VERSION {
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
                Some("world") => {
                    store.world = unescape(f.get(1).copied().unwrap_or(""));
                }
                Some("portal") if f.len() == 12 => {
                    let e = PortalEdge {
                        source: EdgeSource::parse(f[1]).ok_or_else(|| bad("source"))?,
                        wcid: read_opt(f[2]).map_err(|()| bad("wcid"))?,
                        name: unescape(f[3]),
                        from: read_position(f[4]).ok_or_else(|| bad("from"))?,
                        to: if f[5] == "-" {
                            None
                        } else {
                            Some(read_position(f[5]).ok_or_else(|| bad("to"))?)
                        },
                        verified: f[6].parse().map_err(|_| bad("verified"))?,
                        uses: f[7].parse().map_err(|_| bad("uses"))?,
                        failures: f[8].parse().map_err(|_| bad("failures"))?,
                        levels: (
                            read_opt(f[9]).map_err(|()| bad("min level"))?,
                            read_opt(f[10]).map_err(|()| bad("max level"))?,
                        ),
                        retired: (f[11] != "-").then(|| unescape(f[11])),
                    };
                    store.portals.push(e);
                }
                Some("recall") if f.len() == 6 => {
                    store.recalls.push(RecallEdge {
                        character: unescape(f[1]),
                        recall: Recall::parse(f[2]).ok_or_else(|| bad("recall"))?,
                        to: read_position(f[3]).ok_or_else(|| bad("to"))?,
                        verified: f[4].parse().map_err(|_| bad("verified"))?,
                        uses: f[5].parse().map_err(|_| bad("uses"))?,
                    });
                }
                _ => return Err(bad("unknown record")),
            }
        }
        Ok(store)
    }
}

/// What the frames after a portal or recall is used show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TeleportOutcome {
    /// Nothing yet.
    Waiting,
    /// The body is out of portal space, here, far from where it was used.
    Landed(Position),
    /// The wait ran out with the body where it was: the use was refused.
    Refused,
}

/// Watches the frames after a portal or recall is used, for where it landed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportWatch {
    /// Where the body stood when it used it.
    pub before: Position,
    /// When it was used, in seconds.
    pub since: f64,
    /// How long to wait before taking it for refused.
    pub wait: f64,
}

impl TeleportWatch {
    /// How far the body must have moved, in metres, for a teleport to have taken it.
    pub const MOVED: f32 = 15.0;

    /// The outcome given the body at `at` at `now`, and whether it is out of portal space
    /// (`idle`). The new position arrives before portal space ends; only both together are a
    /// landing. A trip under way when the wait runs out is given longer.
    #[must_use]
    pub fn update(&self, at: &Position, idle: bool, now: f64) -> TeleportOutcome {
        let waited = now - self.since;
        if idle && global_distance(at, &self.before) > Self::MOVED {
            TeleportOutcome::Landed(*at)
        } else if (idle && waited > self.wait) || waited > 4.0 * self.wait {
            TeleportOutcome::Refused
        } else {
            TeleportOutcome::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the portal store's learning and its text form).
    use super::*;

    fn at(cell: u32, x: f32, y: f32) -> Position {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 10.0), Quat::IDENTITY),
        )
    }

    fn sighting(x: f32) -> PortalSighting {
        PortalSighting {
            wcid: Some(1234),
            name: String::from("Portal to Somewhere"),
            position: at(0xA9B4_0011, x, 20.0),
        }
    }

    #[test]
    fn a_portal_used_is_learned_and_a_second_use_confirms_it() {
        let mut s = PortalStore::new("loopback");
        s.learn_use(&sighting(10.0), &at(0x0106_0104, 8.0, -60.0), 100);
        s.learn_use(&sighting(10.5), &at(0x0106_0104, 8.5, -60.0), 200);
        assert_eq!(
            s.portals().len(),
            1,
            "the same portal, seen half a metre apart"
        );
        let e = &s.portals()[0];
        assert_eq!((e.uses, e.failures, e.verified), (2, 0, 200));
        assert_eq!(s.routable(None).len(), 1);
    }

    #[test]
    fn a_portal_that_lands_elsewhere_is_corrected_and_a_refused_one_retired() {
        let mut s = PortalStore::new("loopback");
        s.learn_use(&sighting(10.0), &at(0x0106_0104, 8.0, -60.0), 100);
        s.learn_use(&sighting(10.0), &at(0x8602_0100, 150.0, -230.0), 200);
        let e = &s.portals()[0];
        assert_eq!(e.to.map(|t| t.cell), Some(CellId(0x8602_0100)));
        assert_eq!(e.failures, 1);
        s.learn_refusal(&sighting(10.0), "You are not powerful enough", 300);
        assert!(
            s.routable(None).is_empty(),
            "a refused portal is not routed through"
        );
        s.learn_use(&sighting(10.0), &at(0x8602_0100, 150.0, -230.0), 400);
        assert_eq!(s.routable(None).len(), 1, "a later use brings it back");
    }

    /// A frame of a recorded run: seconds since the use, the body, and whether it is out of
    /// portal space.
    type Frame3 = (f64, Position, bool);

    /// Feed recorded frames to a watch and learn from its outcome as the follower does.
    fn replay(
        store: &mut PortalStore,
        portal: &PortalSighting,
        frames: &[Frame3],
    ) -> TeleportOutcome {
        let watch = TeleportWatch {
            before: frames[0].1,
            since: 0.0,
            wait: 15.0,
        };
        for (t, at, idle) in frames {
            match watch.update(at, *idle, *t) {
                TeleportOutcome::Waiting => {}
                TeleportOutcome::Landed(p) => {
                    store.learn_use(portal, &p, 1);
                    return TeleportOutcome::Landed(p);
                }
                TeleportOutcome::Refused => {
                    store.learn_refusal(portal, "no teleport", 1);
                    return TeleportOutcome::Refused;
                }
            }
        }
        TeleportOutcome::Waiting
    }

    #[test]
    fn a_recorded_portal_trip_is_learned_where_it_landed_and_a_recorded_refusal_retires_the_portal()
    {
        // The frames of a live run through Holtburg's Drudge Hideout portal: standing at it,
        // then in portal space, where the new position arrives before portal space ends, then
        // out of it in the dungeon.
        let at_portal = Position::new(
            CellId(0xA9B3_0002),
            Frame::new(Vec3::new(5.54, 29.59, 94.005), Quat::IDENTITY),
        );
        let landed = Position::new(
            CellId(0x019E_0114),
            Frame::new(Vec3::new(10.0, -40.0, 0.005), Quat::IDENTITY),
        );
        let portal = PortalSighting {
            wcid: Some(2068),
            name: String::from("Drudge Hideout"),
            position: at_portal,
        };
        let trip = [
            (0.0, at_portal, true),
            (0.5, at_portal, true),
            (1.2, at_portal, false),
            (3.9, landed, false),
            (4.4, landed, true),
        ];
        let mut store = PortalStore::new("loopback");
        assert_eq!(
            replay(&mut store, &portal, &trip),
            TeleportOutcome::Landed(landed)
        );
        let e = &store.routable(None)[0];
        assert_eq!(
            (e.source, e.to, e.uses),
            (EdgeSource::Learned, Some(landed), 1)
        );
        // A use the server refuses: the body never leaves.
        let refused = [
            (0.0, at_portal, true),
            (8.0, at_portal, true),
            (15.5, at_portal, true),
        ];
        assert_eq!(
            replay(&mut store, &portal, &refused),
            TeleportOutcome::Refused
        );
        assert!(store.routable(None).is_empty());
        // A trip still in portal space when the wait runs out is not taken for a refusal.
        let slow = [(0.0, at_portal, true), (16.0, at_portal, false)];
        assert_eq!(replay(&mut store, &portal, &slow), TeleportOutcome::Waiting);
    }

    #[test]
    fn a_refused_hint_is_remembered_apart_from_the_hints_it_contradicts() {
        let mut s = PortalStore::new("loopback");
        s.merge([PortalEdge {
            source: EdgeSource::Hint,
            wcid: Some(1234),
            name: String::from("Portal to Somewhere"),
            from: sighting(10.0).position,
            to: Some(at(0x0106_0104, 0.0, 0.0)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        }]);
        assert_eq!(s.routable(None).len(), 1);
        s.learn_refusal(&sighting(10.0), "You are not powerful enough", 300);
        // What is kept for the world leaves the hints out; read back with the hints taken in
        // again, the portal stays out of routes.
        let mut kept =
            PortalStore::from_text(&s.without(EdgeSource::Hint).to_text()).expect("reads back");
        assert_eq!(kept.portals().len(), 1);
        assert_eq!(kept.portals()[0].source, EdgeSource::Learned);
        kept.merge(
            s.portals()
                .iter()
                .filter(|e| e.source == EdgeSource::Hint)
                .map(|e| PortalEdge {
                    retired: None,
                    ..e.clone()
                }),
        );
        assert!(kept.routable(None).is_empty());
    }

    #[test]
    fn learned_knowledge_wins_over_published_and_hints_and_levels_are_respected() {
        let mut s = PortalStore::new("loopback");
        let hint = PortalEdge {
            source: EdgeSource::Hint,
            wcid: Some(1234),
            name: String::from("Portal to Somewhere"),
            from: at(0xA9B4_0011, 10.0, 20.0),
            to: Some(at(0x0106_0104, 0.0, 0.0)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (Some(10), None),
            retired: None,
        };
        s.merge([hint.clone()]);
        assert!(
            s.routable(Some(5)).is_empty(),
            "level 5 is below the hint's minimum"
        );
        assert_eq!(s.routable(Some(20))[0].source, EdgeSource::Hint);
        s.merge([PortalEdge {
            source: EdgeSource::Published,
            levels: (None, None),
            ..hint.clone()
        }]);
        assert_eq!(s.routable(Some(5))[0].source, EdgeSource::Published);
        s.learn_use(&sighting(10.0), &at(0x0106_0104, 8.0, -60.0), 100);
        let best = s.routable(Some(5));
        assert_eq!(best.len(), 1);
        assert_eq!(best[0].source, EdgeSource::Learned);
        assert_eq!(s.portals().len(), 3, "the others are kept beside it");
    }

    #[test]
    fn the_store_reads_back_what_it_writes() {
        let mut s = PortalStore::new("loopback\tworld");
        s.learn_use(&sighting(10.0), &at(0x0106_0104, 8.0, -60.0), 100);
        s.learn_refusal(
            &PortalSighting {
                wcid: None,
                name: String::from("Odd\\name"),
                position: at(0xA9B3_0001, 1.0, 1.0),
            },
            "level\trange",
            5,
        );
        s.learn_recall(
            "+Navtwo",
            Recall::Spell(1635),
            &at(0xA9B4_0021, 50.0, 50.0),
            300,
        );
        let text = s.to_text();
        assert!(text.starts_with("dereth-navigation-store\t1\n"), "{text}");
        assert_eq!(PortalStore::from_text(&text), Ok(s));
        assert_eq!(
            PortalStore::from_text("dereth-navigation-store\t9\n"),
            Err(StoreError::Version(9))
        );
        assert_eq!(PortalStore::from_text("hello"), Err(StoreError::NotAStore));
    }
}
