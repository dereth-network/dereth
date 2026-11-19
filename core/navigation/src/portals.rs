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
//! retires it, with the reason, until a later use succeeds. A refusal the server gave a reason for
//! (a quest the portal asks, a level, a player killer's standing) is the character's own, and kept
//! for that character alone, for an hour ([`REFUSAL_KEPT`]) or until it uses the portal: a quest
//! finished since may have opened it. Recalls are kept per character, and only
//! ever learned: where a recall sends a character is the server's business.
//!
//! The store is text, one record a line, so it diffs and survives being edited by hand; see
//! [`PortalStore::to_text`].

use std::fmt::Write as _;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
use dereth_world_places::requirement;
pub use dereth_world_places::Requirement;

use crate::path::global_distance;

/// How near two edges of one class stand to be one portal, metres.
const SAME_PORTAL: f32 = 3.0;

/// How long a portal's refusal of one character keeps it from that character's routes, seconds.
pub const REFUSAL_KEPT: u64 = 60 * 60;

/// Whether a portal of class `a` at `pa` and one of class `b` at `pb` are one portal: the same
/// class when both are known, within [`SAME_PORTAL`].
fn same_place(a: Option<u32>, pa: &Position, b: Option<u32>, pb: &Position) -> bool {
    a.zip(b).is_none_or(|(a, b)| a == b) && global_distance(pa, pb) < SAME_PORTAL
}
/// The store's format version, the first line's second field. Version 1 had no `requires`
/// field, version 2 no `shape` and version 3 no `refusal` records; all read still.
pub const STORE_VERSION: u32 = 4;

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

    pub(crate) fn parse(w: &str) -> Option<Self> {
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

/// What a portal's body is: its setup and its scale. The client's files turn it into how near a
/// body comes before the portal takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalShape {
    /// The setup it is drawn and collides as (a `0x02` data id).
    pub setup: u32,
    /// Its scale.
    pub scale: f32,
    /// Whether a body that touches it is taken: false for a portal that ignores collisions,
    /// which only a use takes anyone through.
    pub touches: bool,
}

impl PortalShape {
    /// The shape as the store writes it: `0x020001B3 1`, then ` untouched` for one a body is
    /// never taken by touching.
    #[must_use]
    pub fn write(self) -> String {
        format!(
            "0x{:08X} {}{}",
            self.setup,
            self.scale,
            if self.touches { "" } else { " untouched" }
        )
    }

    /// A shape as [`Self::write`] wrote it.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let mut words = text.split(' ');
        let setup = words.next()?;
        let setup = setup
            .strip_prefix("0x")
            .or_else(|| setup.strip_prefix("0X"))?;
        let scale: f32 = words.next()?.parse().ok()?;
        (scale.is_finite() && scale > 0.0).then_some(())?;
        let touches = match words.next() {
            None => true,
            Some("untouched") => false,
            Some(_) => return None,
        };
        words.next().is_none().then_some(())?;
        Some(Self {
            setup: u32::from_str_radix(setup, 16).ok()?,
            scale,
            touches,
        })
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
    /// What its body is, when the client saw it.
    pub shape: Option<PortalShape>,
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
    /// What else it asks, of the character who uses it or of the world for it to stand there.
    pub requires: Vec<Requirement>,
    /// What its body is, when known: how near a walk that is not taking it may come.
    pub shape: Option<PortalShape>,
}

/// A character's standing as a player killer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkStatus {
    /// Not a player killer.
    Npk,
    /// A player killer.
    Pk,
    /// A lite player killer.
    PkLite,
}

/// What is known of the character who would use a portal, and of the world it would be used in.
/// A portal asking what is not known (`None`) is taken to admit the character, as one asking
/// nothing would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traveller {
    pub level: Option<i32>,
    /// The quest flags the character holds.
    pub quests: Option<std::collections::BTreeSet<String>>,
    pub pk: Option<PkStatus>,
    /// Whether the character is an Olthoi.
    pub olthoi: bool,
    /// Whether the character is weakened by vitae.
    pub vitae: bool,
    /// Whether the character, or its account, is two weeks old.
    pub account_two_weeks: bool,
    /// The account's subscription, as portals rank it.
    pub subscription: Option<u32>,
    pub advocate: bool,
    /// Events known to run (`true`) or not, by name in any case; the others are as the portal's
    /// table says.
    pub events: std::collections::BTreeMap<String, bool>,
    /// Whether it is night in the world.
    pub night: Option<bool>,
}

impl Default for Traveller {
    fn default() -> Self {
        Self::at_level(None)
    }
}

impl Traveller {
    /// A character of `level` (when known) of whom nothing else is known: no Olthoi, no vitae,
    /// an account two weeks old and no Advocate.
    #[must_use]
    pub const fn at_level(level: Option<i32>) -> Self {
        Self {
            level,
            quests: None,
            pk: None,
            olthoi: false,
            vitae: false,
            account_two_weeks: true,
            subscription: None,
            advocate: false,
            events: std::collections::BTreeMap::new(),
            night: None,
        }
    }

    /// Whether the event `name` runs, when a portal's table says it does (`on`) unless switched
    /// off.
    #[must_use]
    pub fn runs(&self, name: &str, on: bool) -> bool {
        self.events
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map_or(on, |(_, &v)| v)
    }
}

/// Why a portal would not take a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A use was refused, or landed elsewhere, and none has succeeded since: the reason given.
    Retired(String),
    /// The character is below its lowest level.
    BelowLevel(i32),
    /// The character is above its highest level.
    AboveLevel(i32),
    /// It asks what the character, or the world, does not have.
    Requires(Requirement),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retired(why) => write!(f, "refused before: {why}"),
            Self::BelowLevel(min) => write!(f, "level {min} at least"),
            Self::AboveLevel(max) => write!(f, "level {max} at most"),
            Self::Requires(r) => write!(f, "requires {r}"),
        }
    }
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
        class && global_distance(&self.from, at) < SAME_PORTAL
    }

    /// Whether a character of `level` may use it, as far as is known.
    #[must_use]
    pub fn admits(&self, level: Option<i32>) -> bool {
        let Some(l) = level else { return true };
        self.levels.0.is_none_or(|min| l >= min) && self.levels.1.is_none_or(|max| l <= max)
    }

    /// Why it would not take `traveller`, as far as is known; `None` when it would.
    #[must_use]
    pub fn refusal(&self, traveller: &Traveller) -> Option<Refusal> {
        if let Some(why) = &self.retired {
            return Some(Refusal::Retired(why.clone()));
        }
        if let Some(l) = traveller.level {
            if let Some(min) = self.levels.0.filter(|&min| l < min) {
                return Some(Refusal::BelowLevel(min));
            }
            if let Some(max) = self.levels.1.filter(|&max| l > max) {
                return Some(Refusal::AboveLevel(max));
            }
        }
        let t = traveller;
        self.requires
            .iter()
            .find(|r| match r {
                Requirement::Closed | Requirement::Temporary => true,
                Requirement::Quest(q) => t
                    .quests
                    .as_ref()
                    .is_some_and(|held| !held.iter().any(|h| h.eq_ignore_ascii_case(q))),
                Requirement::NoPk => t.pk == Some(PkStatus::Pk),
                Requirement::NoPkLite => t.pk == Some(PkStatus::PkLite),
                Requirement::NoNpk => t.pk == Some(PkStatus::Npk),
                Requirement::OlthoiOnly => !t.olthoi,
                Requirement::NoOlthoi => t.olthoi,
                Requirement::NoVitae => t.vitae,
                Requirement::AccountAge => !t.account_two_weeks,
                Requirement::Account(n) => t.subscription.is_some_and(|s| s < *n),
                Requirement::Advocate => !t.advocate,
                Requirement::Event { name, on } => !t.runs(name, *on),
                Requirement::Night => t.night == Some(false),
                Requirement::Day => t.night == Some(true),
                Requirement::Other(_) => false,
            })
            .map(|r| Refusal::Requires(r.clone()))
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
#[derive(Debug, Clone, Default)]
pub struct PortalStore {
    /// The world this knowledge is about: the server's own identity, never mixed with another's.
    pub world: String,
    portals: Vec<PortalEdge>,
    recalls: Vec<RecallEdge>,
    /// The portals that refused one character, with the server's reason.
    refused: Vec<CharacterRefusal>,
    /// Counts the changes, so a reader can tell when to read again.
    revision: u64,
}

/// A portal that refused one character, saying why: kept for that character until it uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterRefusal {
    /// The character's name.
    pub character: String,
    /// The portal's weenie class, when known.
    pub wcid: Option<u32>,
    /// Where it stands.
    pub at: Position,
    /// What the server said.
    pub reason: String,
    /// When, in seconds since 1970.
    pub when: u64,
}

/// Two stores are equal when they know the same: how many times each was changed is not part of
/// what they know.
impl PartialEq for PortalStore {
    fn eq(&self, other: &Self) -> bool {
        self.world == other.world
            && self.portals == other.portals
            && self.recalls == other.recalls
            && self.refused == other.refused
    }
}

/// A position as one field: `0xCELLID x y z`.
pub(crate) fn write_position(p: &Position) -> String {
    let o = p.frame.origin;
    format!("0x{:08X} {:.3} {:.3} {:.3}", p.cell.0, o.x, o.y, o.z)
}

pub(crate) fn read_position(s: &str) -> Option<Position> {
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
pub(crate) fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

pub(crate) fn unescape(s: &str) -> String {
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

pub(crate) fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| String::from("-"), |v| v.to_string())
}

pub(crate) fn read_opt<T: std::str::FromStr>(s: &str) -> Result<Option<T>, ()> {
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
            refused: Vec::new(),
            revision: 0,
        }
    }

    /// How many times the store has changed: a reader keeps what it made of it while this stays.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
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
            refused: self.refused.clone(),
            revision: 0,
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

    /// The character sees `portal`: what it sees of its body is known of every edge for it.
    pub fn note_shape(&mut self, portal: &PortalSighting) {
        let Some(shape) = portal.shape else { return };
        for e in &mut self.portals {
            if e.shape != Some(shape) && e.same_portal(portal.wcid, &portal.position) {
                e.shape = Some(shape);
                self.revision += 1;
            }
        }
    }

    /// The character used `portal` at `now` and arrived at `arrival`: the learned edge is
    /// confirmed, or corrected to where it really sends, or made.
    pub fn learn_use(&mut self, portal: &PortalSighting, arrival: &Position, now: u64) {
        self.revision += 1;
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
            e.shape = portal.shape.or(e.shape);
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
            requires: Vec::new(),
            shape: portal.shape,
        });
    }

    /// `character` used `portal` at `now` and the server refused it, saying why (`reason`): the
    /// portal is kept from that character's routes, with the reason, until it uses it. Other
    /// characters may still be routed through it.
    pub fn learn_refusal_for(
        &mut self,
        character: &str,
        portal: &PortalSighting,
        reason: &str,
        now: u64,
    ) {
        self.revision += 1;
        self.refused.retain(|r| {
            !(r.character.eq_ignore_ascii_case(character)
                && same_place(r.wcid, &r.at, portal.wcid, &portal.position))
        });
        self.refused.push(CharacterRefusal {
            character: character.to_owned(),
            wcid: portal.wcid,
            at: portal.position,
            reason: reason.to_owned(),
            when: now,
        });
    }

    /// `character` used `portal`: what it refused that character before is forgotten.
    pub fn forget_refusal_for(&mut self, character: &str, portal: &PortalSighting) {
        let before = self.refused.len();
        self.refused.retain(|r| {
            !(r.character.eq_ignore_ascii_case(character)
                && same_place(r.wcid, &r.at, portal.wcid, &portal.position))
        });
        if self.refused.len() != before {
            self.revision += 1;
        }
    }

    /// What the portal of `edge` said when it refused `character` within [`REFUSAL_KEPT`] of
    /// `now`, if it did.
    #[must_use]
    pub fn refused_for(&self, character: &str, edge: &PortalEdge, now: u64) -> Option<&str> {
        self.refused
            .iter()
            .find(|r| {
                r.character.eq_ignore_ascii_case(character)
                    && edge.same_portal(r.wcid, &r.at)
                    && now.saturating_sub(r.when) < REFUSAL_KEPT
            })
            .map(|r| r.reason.as_str())
    }

    /// The character used `portal` at `now` and it refused (`reason`): every edge for that
    /// portal is retired until a use succeeds, so routes stop counting on it.
    pub fn learn_refusal(&mut self, portal: &PortalSighting, reason: &str, now: u64) {
        self.revision += 1;
        let mut learned = false;
        for e in &mut self.portals {
            if e.same_portal(portal.wcid, &portal.position) {
                e.failures += 1;
                e.retired = Some(reason.to_owned());
                e.verified = now;
                e.shape = portal.shape.or(e.shape);
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
                requires: Vec::new(),
                shape: portal.shape,
            });
        }
    }

    /// Take in published or seeded edges. An edge replaces one of the same source for the same
    /// portal; it never replaces a more trusted one, which stays beside it and wins at routing.
    pub fn merge(&mut self, edges: impl IntoIterator<Item = PortalEdge>) {
        self.revision += 1;
        for e in edges {
            if let Some(old) = self.find_mut(e.source, e.wcid, &e.from) {
                *old = e;
            } else {
                self.portals.push(e);
            }
        }
    }

    /// The edges a route may use for a character of `level` of whom nothing else is known
    /// ([`Self::routable_for`] with [`Traveller::at_level`]).
    #[must_use]
    pub fn routable(&self, level: Option<i32>) -> Vec<&PortalEdge> {
        self.routable_for(&Traveller::at_level(level))
    }

    /// The edges a route may use for `traveller`: per portal the most trusted edge that knows a
    /// destination, unless that portal is retired by any of its edges, or that edge would refuse
    /// the traveller ([`PortalEdge::refusal`]).
    #[must_use]
    pub fn routable_for(&self, traveller: &Traveller) -> Vec<&PortalEdge> {
        self.routable_where(|e| e.refusal(traveller).is_none())
    }

    /// The edges a route may use for anyone at all: per portal the most trusted edge that knows a
    /// destination, unless that portal is retired by any of its edges. What a traveller is refused
    /// on such a route is what keeps them from it ([`crate::reach_for`]).
    #[must_use]
    pub fn routable_for_anyone(&self) -> Vec<&PortalEdge> {
        self.routable_where(|_| true)
    }

    fn routable_where(&self, admits: impl Fn(&PortalEdge) -> bool) -> Vec<&PortalEdge> {
        // Edges by where they stand, a square of the distance that makes two the same portal at a
        // time: an edge's same portal is in its square or one beside it.
        let square = |e: &PortalEdge| {
            let g = crate::path::global(&e.from);
            #[allow(clippy::cast_possible_truncation)]
            (
                (g.x / SAME_PORTAL).floor() as i64,
                (g.y / SAME_PORTAL).floor() as i64,
            )
        };
        let mut squares: std::collections::HashMap<(i64, i64), Vec<usize>> =
            std::collections::HashMap::new();
        for (i, e) in self.portals.iter().enumerate() {
            squares.entry(square(e)).or_default().push(i);
        }
        let mut out: Vec<&PortalEdge> = Vec::new();
        for e in &self.portals {
            let (sx, sy) = square(e);
            let mut near: Vec<usize> = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(v) = squares.get(&(sx + dx, sy + dy)) {
                        near.extend(v);
                    }
                }
            }
            // In the store's order, as the search over every edge took them.
            near.sort_unstable();
            let same: Vec<&PortalEdge> = near
                .into_iter()
                .map(|i| &self.portals[i])
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
            if !std::ptr::eq(*best, e) || !admits(best) {
                continue;
            }
            out.push(e);
        }
        out
    }

    /// `character`'s `recall` sent it to `arrival` at `now`.
    pub fn learn_recall(&mut self, character: &str, recall: Recall, arrival: &Position, now: u64) {
        self.revision += 1;
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
    /// dereth-navigation-store<TAB>4
    /// world<TAB><identity>
    /// portal<TAB>source<TAB>wcid<TAB>name<TAB>from<TAB>to<TAB>verified<TAB>uses<TAB>failures<TAB>min<TAB>max<TAB>retired<TAB>requires<TAB>shape
    /// recall<TAB>character<TAB>recall<TAB>to<TAB>verified<TAB>uses
    /// refusal<TAB>character<TAB>wcid<TAB>at<TAB>when<TAB>reason
    /// ```
    ///
    /// A position is `0xCELLID x y z`; an absent value is `-`; names and reasons have their tabs,
    /// line breaks and backslashes escaped. `requires` is what else the portal asks, its terms
    /// separated by `;` ([`dereth_world_places::requirement`]), `-` for nothing. `shape` is its
    /// setup and scale ([`PortalShape::write`]), `-` when unknown. A version 1 store's `portal`
    /// lines have no `requires`, and a version 2 store's no `shape`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "dereth-navigation-store\t{STORE_VERSION}\nworld\t{}\n",
            escape(&self.world)
        );
        for e in &self.portals {
            let _ = writeln!(
                out,
                "portal\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
                escape(&requirement::write_field(&e.requires)),
                e.shape
                    .map_or_else(|| String::from("-"), PortalShape::write),
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
        for r in &self.refused {
            let _ = writeln!(
                out,
                "refusal\t{}\t{}\t{}\t{}\t{}",
                escape(&r.character),
                opt(r.wcid),
                write_position(&r.at),
                r.when,
                escape(&r.reason)
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
                Some("portal")
                    if f.len() == 12
                        || (version >= 2 && f.len() == 13)
                        || (version >= 3 && f.len() == 14) =>
                {
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
                        requires: f
                            .get(12)
                            .map(|r| requirement::read_field(&unescape(r)))
                            .unwrap_or_default(),
                        shape: match f.get(13).copied() {
                            None | Some("-") => None,
                            Some(s) => Some(PortalShape::read(s).ok_or_else(|| bad("shape"))?),
                        },
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
                Some("refusal") if version >= 4 && f.len() == 6 => {
                    store.refused.push(CharacterRefusal {
                        character: unescape(f[1]),
                        wcid: read_opt(f[2]).map_err(|()| bad("wcid"))?,
                        at: read_position(f[3]).ok_or_else(|| bad("at"))?,
                        when: f[4].parse().map_err(|_| bad("when"))?,
                        reason: unescape(f[5]),
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
    /// Where it sends, when the body is known to be set down there already: a landing there
    /// counts however near it is to where the body was.
    pub to: Option<Position>,
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
        let there = self.to.is_some_and(|to| {
            global_distance(at, &to) < 1.0 && global_distance(at, &self.before) > 1.0
        });
        if idle && (there || global_distance(at, &self.before) > Self::MOVED) {
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
            shape: None,
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
            to: None,
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
            shape: None,
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
            requires: Vec::new(),
            shape: None,
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
            requires: Vec::new(),
            shape: None,
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

    fn published(x: f32, levels: (Option<i32>, Option<i32>), requires: &str) -> PortalEdge {
        PortalEdge {
            source: EdgeSource::Published,
            wcid: Some(1234),
            name: String::from("Portal to Somewhere"),
            from: at(0xA9B4_0011, x, 20.0),
            to: Some(at(0x0106_0104, 0.0, 0.0)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels,
            retired: None,
            requires: requirement::read_field(requires),
            shape: None,
        }
    }

    #[test]
    fn a_portal_turns_away_a_character_it_asks_more_of_and_says_why() {
        let arena = published(10.0, (Some(40), Some(80)), "quest ArenaFlag;no-pk;no-vitae");
        let pk = Traveller {
            pk: Some(PkStatus::Pk),
            ..Traveller::at_level(Some(60))
        };
        assert_eq!(
            arena.refusal(&Traveller::at_level(Some(30))),
            Some(Refusal::BelowLevel(40))
        );
        assert_eq!(
            arena.refusal(&Traveller::at_level(Some(90))),
            Some(Refusal::AboveLevel(80))
        );
        assert_eq!(
            arena.refusal(&pk),
            Some(Refusal::Requires(Requirement::NoPk))
        );
        // A character whose flags are not known is taken to hold them; one whose flags are
        // known is not.
        assert_eq!(arena.refusal(&Traveller::at_level(Some(60))), None);
        let flagless = Traveller {
            quests: Some(std::collections::BTreeSet::new()),
            ..Traveller::at_level(Some(60))
        };
        assert_eq!(
            arena.refusal(&flagless).map(|r| r.to_string()),
            Some(String::from("requires quest ArenaFlag"))
        );
        let flagged = Traveller {
            quests: Some(["arenaflag".to_owned()].into()),
            vitae: true,
            ..flagless
        };
        assert_eq!(
            arena.refusal(&flagged),
            Some(Refusal::Requires(Requirement::NoVitae))
        );

        // What the world must be for a portal to stand there: an event off unless switched on,
        // the night, and a portal that stands only a while, which no route counts on.
        let festival = published(10.0, (None, None), "event Snowreap off;night");
        assert_eq!(
            festival.refusal(&Traveller::default()),
            Some(Refusal::Requires(Requirement::Event {
                name: "Snowreap".into(),
                on: false
            }))
        );
        let mut snowreap = Traveller::default();
        snowreap.events.insert("snowreap".into(), true);
        assert_eq!(festival.refusal(&snowreap), None);
        snowreap.night = Some(false);
        assert_eq!(
            festival.refusal(&snowreap),
            Some(Refusal::Requires(Requirement::Night))
        );
        for never in ["closed", "temporary"] {
            let p = published(10.0, (None, None), never);
            assert!(p.refusal(&Traveller::default()).is_some(), "{never}");
        }

        // Routes leave out what would turn the character away.
        let mut s = PortalStore::new("loopback");
        s.merge([arena, published(30.0, (None, None), "closed")]);
        assert_eq!(
            s.routable(Some(60)).len(),
            1,
            "the closed portal is never routed"
        );
        assert!(s.routable_for(&pk).is_empty());
    }

    #[test]
    fn a_version_1_store_reads_and_a_portals_requirements_read_back() {
        let v1 = "dereth-navigation-store\t1\nworld\tW\n\
portal\tpublished\t1000\tRanged\t0xA9B40021 84.000 7.500 94.000\t0x01AC0118 60.000 -50.250 0.500\t0\t0\t0\t10\t50\t-\n";
        let s = PortalStore::from_text(v1).expect("a version 1 store reads");
        assert_eq!(s.portals()[0].levels, (Some(10), Some(50)));
        assert!(s.portals()[0].requires.is_empty());
        let v2 = format!("{}\t-\n", v1.trim_end().replacen("\t1\n", "\t2\n", 1));
        assert_eq!(
            PortalStore::from_text(&v2).expect("a version 2 store reads"),
            s
        );
        // A line of version 2's width in a version 1 store is no line of that version.
        let wide = format!("{}\t-\n", v1.trim_end());
        assert!(PortalStore::from_text(&wide).is_err());
        let mut s = PortalStore::new("W");
        s.merge([published(
            10.0,
            (None, None),
            "quest Gharu'n\\; Temple;event Fair on;tide high",
        )]);
        let back = PortalStore::from_text(&s.to_text()).expect("reads back");
        assert_eq!(back, s);
        assert_eq!(
            back.portals()[0].requires[0],
            Requirement::Quest("Gharu'n; Temple".into())
        );
        assert_eq!(
            back.portals()[0].requires[2],
            Requirement::Other("tide high".into())
        );
    }

    #[test]
    fn a_portals_shape_reads_back_and_a_version_2_store_has_none() {
        let common = PortalShape {
            setup: 0x0200_01B3,
            scale: 1.0,
            touches: true,
        };
        assert_eq!(common.write(), "0x020001B3 1");
        let apart = PortalShape {
            setup: 0x0200_0EF6,
            scale: 0.8,
            touches: false,
        };
        assert_eq!(apart.write(), "0x02000EF6 0.8 untouched");
        for shape in [common, apart] {
            assert_eq!(PortalShape::read(&shape.write()), Some(shape));
        }
        for bad in [
            "",
            "0x020001B3",
            "020001B3 1",
            "0x020001B3 0",
            "0x020001B3 1 odd",
            "0x020001B3 1 untouched x",
        ] {
            assert_eq!(PortalShape::read(bad), None, "{bad}");
        }
        // A seen shape is kept by every edge of the portal, and written with it.
        let mut store = PortalStore::new("W");
        store.merge([published(10.0, (None, None), "")]);
        let mut seen = sighting(10.0);
        seen.shape = Some(common);
        store.note_shape(&seen);
        assert_eq!(store.portals()[0].shape, Some(common));
        let text = store.to_text();
        assert!(text.contains("\t0x020001B3 1\n"), "{text}");
        assert_eq!(PortalStore::from_text(&text), Ok(store.clone()));
        // A version 2 store's lines have no shape; a shape field in one is no line of that
        // version.
        let v2 = text
            .replacen("\t4\n", "\t2\n", 1)
            .replace("\t0x020001B3 1\n", "\n");
        let read = PortalStore::from_text(&v2).expect("a version 2 store reads");
        assert_eq!(read.portals()[0].shape, None);
        assert!(PortalStore::from_text(&text.replacen("\t4\n", "\t2\n", 1)).is_err());
        let odd = text.replace("\t0x020001B3 1\n", "\tround\n");
        assert!(
            PortalStore::from_text(&odd).is_err(),
            "a shape that does not read"
        );
    }

    #[test]
    fn a_portal_that_refused_one_character_saying_why_is_kept_from_that_character_alone() {
        let mut s = PortalStore::new("W");
        let edge = published(10.0, (None, None), "");
        s.merge([edge.clone()]);
        let why = "You must complete a quest to interact with that portal.";
        s.learn_refusal_for("+Climber", &sighting(10.5), why, 50);
        assert_eq!(s.refused_for("+climber", &s.portals()[0], 60), Some(why));
        assert_eq!(s.refused_for("+Other", &s.portals()[0], 60), None);
        assert_eq!(
            s.refused_for("+Climber", &s.portals()[0], 50 + REFUSAL_KEPT),
            None,
            "tried again after an hour"
        );
        assert_eq!(
            s.routable_for(&Traveller::default()).len(),
            1,
            "still routed for anyone the store has no word for"
        );
        // Read back, then forgotten once the character has used it.
        let text = s.to_text();
        assert!(text.contains("\nrefusal\t+Climber\t1234\t"), "{text}");
        let mut read = PortalStore::from_text(&text).expect("reads");
        assert_eq!(read, s);
        read.forget_refusal_for("+CLIMBER", &sighting(10.0));
        assert_eq!(read.refused_for("+Climber", &read.portals()[0], 60), None);
        // A refusal line is no line of a version 3 store.
        assert!(PortalStore::from_text(&text.replacen("\t4\n", "\t3\n", 1)).is_err());
        let v3 = PortalStore::new("W")
            .to_text()
            .replacen("\t4\n", "\t3\n", 1);
        assert!(
            PortalStore::from_text(&v3).is_ok(),
            "a version 3 store reads"
        );
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
                shape: None,
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
        assert!(text.starts_with("dereth-navigation-store\t4\n"), "{text}");
        assert_eq!(PortalStore::from_text(&text), Ok(s));
        assert_eq!(
            PortalStore::from_text("dereth-navigation-store\t9\n"),
            Err(StoreError::Version(9))
        );
        assert_eq!(PortalStore::from_text("hello"), Err(StoreError::NotAStore));
    }
}
