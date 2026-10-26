//! The route above the regions: walking, portals and the character's own teleports, searched
//! together before any walking leg is planned.
//!
//! The nodes are the start, the goal, every known portal's two ends and every recall's
//! destination. The edges are:
//!
//! * **walking**, between any two nodes in the same realm (the outdoors, or one part of a
//!   dungeon), costed by distance at the character's pace, a little longer than the straight line;
//! * **portals**, from where a portal stands to where it sends, costed by the trip through portal
//!   space, more for knowledge less trusted;
//! * **teleports from anywhere**: the character's recall spells and recall items whose destination
//!   it has learned, costed by the cast and the trip, and only when it can cast them now;
//! * **hand-backs**, from where a person stands to where they send the character once it hands
//!   back the token they gave it ([`HandBack`]): one way, like a portal, and only from their part
//!   of the world.
//!
//! The cheapest chain is the journey; each walking leg is planned in its region when the body
//! gets there.

use dereth_primitives::{CellId, ObjectId, Position};

use crate::path::global_distance;
use crate::portals::{PortalEdge, PortalStore, Recall};

/// Where a body can walk: the outdoors (with the buildings it opens onto) or a dungeon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Realm {
    /// The land and the insides of its buildings.
    Outdoors,
    /// The dungeon in this landblock, taken whole: its cells are not at hand to tell its parts
    /// apart.
    Dungeon(u16),
    /// One part of a dungeon: the cells its portals join, named by the lowest of them. A dungeon
    /// can be several parts no walk joins, linked only by portals inside it.
    Part(CellId),
}

/// One way a character can teleport from anywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Teleport {
    /// The recall: a spell, the spell the item casts, or a command.
    pub recall: Recall,
    /// Its name.
    pub name: String,
    /// The item that casts it, when it is an item rather than a spell the character knows or a
    /// command.
    pub item: Option<ObjectId>,
    /// Where it sends the character, as last learned.
    pub to: Position,
    /// Mana it costs to cast; zero for an item.
    pub mana: u32,
}

/// What a character can do right now, for choosing teleports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterState {
    /// The character's name, which keys its recalls in the store.
    pub name: String,
    /// Its level, when known.
    pub level: Option<i32>,
    /// Mana it has.
    pub mana: Option<u32>,
    /// The recall-type spells it knows: `(spell, name, mana cost)`.
    pub recall_spells: Vec<(u32, String, u32)>,
    /// The recall items it carries: `(item, spell it casts, name)`.
    pub recall_items: Vec<(ObjectId, u32, String)>,
    /// The recalls it has by command, with their names.
    pub recall_commands: Vec<(Recall, String)>,
    /// Whether it may cast now (not in a cooldown, not busy).
    pub can_cast: bool,
}

impl CharacterState {
    /// The teleports the character can use now: each known recall spell it has the mana for,
    /// each recall item and each recall by command, whose destination the store has learned. A recall it has never used has
    /// no known destination and is not offered.
    #[must_use]
    pub fn teleports(&self, store: &PortalStore) -> Vec<Teleport> {
        let mut out = Vec::new();
        if self.can_cast {
            for (spell, name, mana) in &self.recall_spells {
                if self.mana.is_some_and(|m| m < *mana) {
                    continue;
                }
                if let Some(to) = store.recall_destination(&self.name, Recall::Spell(*spell)) {
                    out.push(Teleport {
                        recall: Recall::Spell(*spell),
                        name: name.clone(),
                        item: None,
                        to,
                        mana: *mana,
                    });
                }
            }
        }
        for (item, spell, name) in &self.recall_items {
            if let Some(to) = store.recall_destination(&self.name, Recall::Spell(*spell)) {
                out.push(Teleport {
                    recall: Recall::Spell(*spell),
                    name: name.clone(),
                    item: Some(*item),
                    to,
                    mana: 0,
                });
            }
        }
        for (recall, name) in &self.recall_commands {
            if let Some(to) = store.recall_destination(&self.name, *recall) {
                out.push(Teleport {
                    recall: *recall,
                    name: name.clone(),
                    item: None,
                    to,
                    mana: 0,
                });
            }
        }
        out
    }
}

/// A way on that a person gives: the character takes a token from them (they give it when used)
/// and hands it back, and they send it on, one way. The Training Academy's exit is one: its exit
/// person gives a token and, handed it back, sends the character to its town for good.
#[derive(Debug, Clone, PartialEq)]
pub struct HandBack {
    /// The person's name.
    pub name: String,
    /// The person's object, when the client knows it.
    pub object: Option<ObjectId>,
    /// Where they stand.
    pub at: Position,
    /// The name of the token they give and take back.
    pub token: String,
    /// Where they send the character.
    pub to: Position,
}

/// One leg of a journey.
#[derive(Debug, Clone, PartialEq)]
pub enum Leg {
    /// Walk to here.
    Walk {
        /// Where.
        to: Position,
    },
    /// Use this portal, standing at its `from`, and arrive at its `to`.
    Portal {
        /// The portal.
        edge: PortalEdge,
    },
    /// Cast this teleport (or use its item) from wherever the body is.
    Teleport {
        /// The teleport.
        teleport: Teleport,
    },
    /// Take the token from this person, standing at their `at`, hand it back, and arrive at their
    /// `to`.
    HandBack {
        /// The person.
        hand_back: HandBack,
    },
}

/// The chain of legs from start to goal, and what it is expected to take.
#[derive(Debug, Clone, PartialEq)]
pub struct Journey {
    /// The legs, in order.
    pub legs: Vec<Leg>,
    /// Expected seconds.
    pub seconds: f32,
}

/// How a journey's legs are costed.
#[derive(Debug, Clone, PartialEq)]
pub struct JourneyConfig {
    /// The character's pace, metres a second.
    pub speed: f32,
    /// How much longer a walk is than the straight line.
    pub detour: f32,
    /// Seconds a portal's trip takes, portal space included.
    pub portal_seconds: f32,
    /// Seconds a cast takes before the trip.
    pub cast_seconds: f32,
    /// Extra seconds for a portal known only from a hint or the server's table, scaled by how
    /// untrusted it is: an untried portal is taken only when it is clearly better.
    pub doubt_seconds: f32,
    /// Seconds a hand-back takes: the question to the player, the token taken and given back,
    /// and the trip. Dear, as there is no coming back by it.
    pub hand_back_seconds: f32,
}

impl Default for JourneyConfig {
    fn default() -> Self {
        Self {
            speed: 5.0,
            detour: 1.3,
            portal_seconds: 8.0,
            cast_seconds: 4.0,
            doubt_seconds: 30.0,
            hand_back_seconds: 60.0,
        }
    }
}

/// What a node of the search is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Node {
    Start,
    Goal,
    PortalEntry(usize),
    PortalExit(usize),
    TeleportExit(usize),
}

/// The side of a cell of the grid walk targets are kept in, in metres: a landblock.
const GRID: f32 = 192.0;

/// A realm with no more walk targets than this is searched whole from each node.
const SCAN_ALL: usize = 64;

/// The places a walk in one realm can end at, by grid cell.
#[derive(Debug, Default)]
struct Targets {
    all: Vec<usize>,
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
    min: (i32, i32),
    max: (i32, i32),
}

impl Targets {
    fn add(&mut self, node: usize, cell: (i32, i32)) {
        if self.all.is_empty() {
            (self.min, self.max) = (cell, cell);
        }
        self.min = (self.min.0.min(cell.0), self.min.1.min(cell.1));
        self.max = (self.max.0.max(cell.0), self.max.1.max(cell.1));
        self.all.push(node);
        self.cells.entry(cell).or_default().push(node);
    }
}

/// A node waiting in the search, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Open(f32, usize);

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Open {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.total_cmp(&self.0).then_with(|| o.1.cmp(&self.1))
    }
}

/// The realms one can get to from `from` over `links` (one realm to another), and to which any
/// realm in `anywhere` is reachable from everywhere.
fn reach(
    from: Realm,
    links: &[(Realm, Realm)],
    anywhere: &[Realm],
    forward: bool,
) -> std::collections::HashSet<Realm> {
    let mut seen = std::collections::HashSet::from([from]);
    let mut queue = vec![from];
    if forward {
        for r in anywhere {
            if seen.insert(*r) {
                queue.push(*r);
            }
        }
    }
    while let Some(r) = queue.pop() {
        for &(a, b) in links {
            let next = if forward {
                (a == r).then_some(b)
            } else {
                (b == r).then_some(a)
            };
            if let Some(n) = next {
                if seen.insert(n) {
                    queue.push(n);
                }
            }
        }
    }
    seen
}

/// The cheapest journey from `from` to `to`, over walking, the `portals` given and the
/// character's `teleports`; `realm` says where a position can be walked to from. `None` when no
/// chain reaches the goal.
///
/// Portals that cannot be on any chain from the start's realm to the goal's are left out before
/// the search: one into a dungeon with no way on toward the goal is never worth walking to. The
/// search keeps the places a walk can end at (portals' entrances and the goal) in a grid per
/// realm, and from each node walks only to those near enough to still beat the goal's cost.
#[must_use]
pub fn plan_journey(
    from: &Position,
    to: &Position,
    portals: &[&PortalEdge],
    teleports: &[Teleport],
    realm: &dyn Fn(&Position) -> Option<Realm>,
    config: &JourneyConfig,
) -> Option<Journey> {
    plan_journey_with(from, to, portals, teleports, &[], realm, config)
}

/// A way from one place to another that the search takes as a portal is taken: a portal, or a
/// person's hand-back.
#[derive(Debug, Clone, Copy)]
enum Link<'a> {
    Portal(&'a PortalEdge),
    HandBack(&'a HandBack),
}

impl Link<'_> {
    fn from(&self) -> Position {
        match self {
            Self::Portal(p) => p.from,
            Self::HandBack(h) => h.at,
        }
    }

    fn to(&self) -> Option<Position> {
        match self {
            Self::Portal(p) => p.to,
            Self::HandBack(h) => Some(h.to),
        }
    }

    /// Seconds from standing at it to arriving.
    fn seconds(&self, config: &JourneyConfig) -> f32 {
        match self {
            Self::Portal(p) => {
                config.portal_seconds + (1.0 - p.source.confidence()) * config.doubt_seconds
            }
            Self::HandBack(_) => config.hand_back_seconds,
        }
    }

    fn leg(&self) -> Leg {
        match self {
            Self::Portal(p) => Leg::Portal { edge: (*p).clone() },
            Self::HandBack(h) => Leg::HandBack {
                hand_back: (*h).clone(),
            },
        }
    }
}

/// [`plan_journey`], with the `hand_backs` the character can use besides: each is taken from
/// where its person stands, as a portal is.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn plan_journey_with(
    from: &Position,
    to: &Position,
    portals: &[&PortalEdge],
    teleports: &[Teleport],
    hand_backs: &[HandBack],
    realm: &dyn Fn(&Position) -> Option<Realm>,
    config: &JourneyConfig,
) -> Option<Journey> {
    let (start_realm, goal_realm) = (realm(from)?, realm(to)?);
    let portals: Vec<Link> = portals
        .iter()
        .map(|p| Link::Portal(p))
        .chain(hand_backs.iter().map(Link::HandBack))
        .collect();
    // Realms one portal (or hand-back) leads between, and where each teleport lands.
    let ends: Vec<Option<(Realm, Realm)>> = portals
        .iter()
        .map(|p| Some((realm(&p.from())?, realm(&p.to()?)?)))
        .collect();
    let landings: Vec<Option<Realm>> = teleports.iter().map(|t| realm(&t.to)).collect();
    let links: Vec<(Realm, Realm)> = ends.iter().flatten().copied().collect();
    let anywhere: Vec<Realm> = landings.iter().flatten().copied().collect();
    let ahead = reach(start_realm, &links, &anywhere, true);
    if !ahead.contains(&goal_realm) {
        return None;
    }
    let behind = reach(goal_realm, &links, &[], false);
    let mut nodes = vec![
        (Node::Start, *from, start_realm),
        (Node::Goal, *to, goal_realm),
    ];
    for (i, (p, e)) in portals.iter().zip(&ends).enumerate() {
        let (Some((a, b)), Some(dest)) = (e, p.to()) else {
            continue;
        };
        if ahead.contains(a) && behind.contains(b) {
            nodes.push((Node::PortalEntry(i), p.from(), *a));
            nodes.push((Node::PortalExit(i), dest, *b));
        }
    }
    for (i, (t, r)) in teleports.iter().zip(&landings).enumerate() {
        if let Some(r) = r.filter(|r| behind.contains(r)) {
            nodes.push((Node::TeleportExit(i), t.to, r));
        }
    }
    let n = nodes.len();
    // Where each portal's exit node is, by portal.
    let mut exit_of = std::collections::HashMap::new();
    for (k, (node, _, _)) in nodes.iter().enumerate() {
        if let Node::PortalExit(i) = node {
            exit_of.insert(*i, k);
        }
    }
    let teleport_nodes: Vec<usize> = (0..n)
        .filter(|&k| matches!(nodes[k].0, Node::TeleportExit(_)))
        .collect();
    // The walk targets of each realm, in a grid of landblocks.
    let cell = |p: &Position| {
        let g = crate::path::global(p);
        (
            dereth_primitives::num::floor_to_i32(g.x / GRID),
            dereth_primitives::num::floor_to_i32(g.y / GRID),
        )
    };
    let mut grids: std::collections::HashMap<Realm, Targets> = std::collections::HashMap::new();
    for (k, (node, p, r)) in nodes.iter().enumerate() {
        if matches!(node, Node::PortalEntry(_)) {
            grids.entry(*r).or_default().add(k, cell(p));
        }
    }
    let per_metre = config.detour / config.speed;
    let mut cost = vec![f32::INFINITY; n];
    let mut came: Vec<Option<(usize, Leg)>> = vec![None; n];
    let mut done = vec![false; n];
    let mut open = std::collections::BinaryHeap::from([Open(0.0, 0)]);
    cost[0] = 0.0;
    while let Some(Open(c, u)) = open.pop() {
        if done[u] || c > cost[u] {
            continue;
        }
        done[u] = true;
        if u == 1 || c >= cost[1] {
            break;
        }
        let mut relax = |v: usize,
                         step: f32,
                         leg: Leg,
                         open: &mut std::collections::BinaryHeap<Open>,
                         cost: &mut [f32]| {
            if !done[v] && c + step < cost[v] {
                cost[v] = c + step;
                came[v] = Some((u, leg));
                open.push(Open(cost[v], v));
            }
        };
        let (kind, here, r) = nodes[u];
        // A portal (or a hand-back) from its entrance.
        if let Node::PortalEntry(i) = kind {
            if let Some(&v) = exit_of.get(&i) {
                let e = portals[i];
                relax(v, e.seconds(config), e.leg(), &mut open, &mut cost);
            }
        }
        // A teleport from anywhere.
        for &v in &teleport_nodes {
            if let Node::TeleportExit(i) = nodes[v].0 {
                relax(
                    v,
                    config.cast_seconds + config.portal_seconds,
                    Leg::Teleport {
                        teleport: teleports[i].clone(),
                    },
                    &mut open,
                    &mut cost,
                );
            }
        }
        // A walk: to the goal, and to the portals near enough to beat it.
        if r == goal_realm {
            let d = global_distance(&here, to);
            relax(
                1,
                d * per_metre,
                Leg::Walk { to: *to },
                &mut open,
                &mut cost,
            );
        }
        let Some(grid) = grids.get(&r) else { continue };
        let mut walk =
            |v: usize, open: &mut std::collections::BinaryHeap<Open>, cost: &mut [f32]| {
                if v != u {
                    let d = global_distance(&here, &nodes[v].1);
                    relax(v, d * per_metre, Leg::Walk { to: nodes[v].1 }, open, cost);
                }
            };
        // Few targets, or nothing yet to beat: every one. Otherwise ring by ring outward, until a
        // ring's nearest place is too far to beat the goal's cost.
        if grid.all.len() <= SCAN_ALL || !cost[1].is_finite() {
            for &v in &grid.all {
                walk(v, &mut open, &mut cost);
            }
            continue;
        }
        let (cx, cy) = cell(&here);
        let rings = (cx - grid.min.0)
            .abs()
            .max((cx - grid.max.0).abs())
            .max((cy - grid.min.1).abs())
            .max((cy - grid.max.1).abs());
        for ring in 0..=rings {
            #[allow(clippy::cast_precision_loss)]
            let nearest = (ring - 1).max(0) as f32 * GRID;
            if c + nearest * per_metre >= cost[1] {
                break;
            }
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    if dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    for &v in grid.cells.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                        walk(v, &mut open, &mut cost);
                    }
                }
            }
        }
    }
    if !cost[1].is_finite() {
        return None;
    }
    let mut legs = Vec::new();
    let mut at = 1;
    while let Some((prev, leg)) = came[at].clone() {
        legs.push(leg);
        at = prev;
    }
    legs.reverse();
    Some(Journey {
        legs,
        seconds: cost[1],
    })
}

/// The realm of a cell id, when the cell's kind alone says: a land cell is outdoors. An
/// interior cell needs the cells' portals to tell a building from a dungeon.
#[must_use]
pub const fn land_realm(cell: CellId) -> Option<Realm> {
    if cell.0 & 0xFFFF < 0x0100 {
        Some(Realm::Outdoors)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the journey search over a made-up world).
    use super::*;
    use crate::portals::{EdgeSource, PortalSighting};
    use dereth_primitives::{Frame, Quat, Vec3};

    fn at(cell: u32, x: f32, y: f32) -> Position {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        )
    }

    /// Land cells are outdoors; interior cells of block 0x0106 are its dungeon; the rest are
    /// outdoors (buildings).
    fn realm(p: &Position) -> Option<Realm> {
        land_realm(p.cell).or(Some(if p.cell.0 >> 16 == 0x0106 {
            Realm::Dungeon(0x0106)
        } else {
            Realm::Outdoors
        }))
    }

    fn portal_to_dungeon(source: EdgeSource) -> PortalEdge {
        PortalEdge {
            source,
            wcid: Some(1),
            name: String::from("Portal"),
            from: at(0xA9B4_0011, 50.0, 50.0),
            to: Some(at(0x0106_0104, 8.0, -60.0)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        }
    }

    /// The cheapest journey's cost by the plain search: every node walks to every other node of
    /// its realm, nothing left out.
    fn exhaustive(
        from: &Position,
        to: &Position,
        portals: &[&PortalEdge],
        teleports: &[Teleport],
        realm: &dyn Fn(&Position) -> Option<Realm>,
        config: &JourneyConfig,
    ) -> Option<f32> {
        let mut nodes = vec![(*from, None, None), (*to, None, None)];
        for (i, p) in portals.iter().enumerate() {
            nodes.push((p.from, Some(i), None));
            nodes.push((p.to?, None, Some(i)));
        }
        for t in teleports {
            nodes.push((t.to, None, None));
        }
        let first_teleport = 2 + 2 * portals.len();
        let n = nodes.len();
        let mut cost = vec![f32::INFINITY; n];
        let mut done = vec![false; n];
        cost[0] = 0.0;
        while let Some(u) = (0..n)
            .filter(|&i| !done[i] && cost[i].is_finite())
            .min_by(|&a, &b| cost[a].total_cmp(&cost[b]))
        {
            done[u] = true;
            for v in 0..n {
                let mut step = f32::INFINITY;
                if realm(&nodes[u].0).is_some() && realm(&nodes[u].0) == realm(&nodes[v].0) {
                    step = global_distance(&nodes[u].0, &nodes[v].0) * config.detour / config.speed;
                }
                if let (Some(i), Some(j)) = (nodes[u].1, nodes[v].2) {
                    if i == j {
                        let doubt = (1.0 - portals[i].source.confidence()) * config.doubt_seconds;
                        step = step.min(config.portal_seconds + doubt);
                    }
                }
                if v >= first_teleport {
                    step = step.min(config.cast_seconds + config.portal_seconds);
                }
                cost[v] = cost[v].min(cost[u] + step);
            }
        }
        cost[1].is_finite().then_some(cost[1])
    }

    #[test]
    fn the_pruned_search_finds_journeys_exactly_as_cheap_as_searching_everything() {
        // Random worlds: portals between the land and twenty dungeons, some ways back out, two
        // recalls, a hundred journeys each between random places.
        let mut seed = 0x9E37_79B9_u32;
        let mut next = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed % n
        };
        let dungeon_realm = |p: &Position| {
            land_realm(p.cell).or_else(|| u16::try_from(p.cell.0 >> 16).ok().map(Realm::Dungeon))
        };
        #[allow(clippy::cast_precision_loss)]
        let place = |next: &mut dyn FnMut(u32) -> u32| {
            if next(3) == 0 {
                at(
                    ((0x0100 + next(20)) << 16) | 0x0100,
                    next(150) as f32,
                    next(150) as f32,
                )
            } else {
                at(
                    ((0xA0 + next(24)) << 24) | ((0xA0 + next(24)) << 16) | 0x0011,
                    next(190) as f32,
                    next(190) as f32,
                )
            }
        };
        let config = JourneyConfig::default();
        let mut compared = 0;
        let mut found = 0;
        for world in 0..20 {
            let edges: Vec<PortalEdge> = (0..120)
                .map(|i| PortalEdge {
                    source: [EdgeSource::Hint, EdgeSource::Learned][usize::from(i % 3 == 0)],
                    wcid: Some(i),
                    name: format!("Portal {i}"),
                    from: place(&mut next),
                    to: Some(place(&mut next)),
                    verified: 0,
                    uses: 0,
                    failures: 0,
                    levels: (None, None),
                    retired: None,
                })
                .collect();
            let portals: Vec<&PortalEdge> = edges.iter().collect();
            let teleports: Vec<Teleport> = (0..2)
                .map(|i| Teleport {
                    recall: Recall::Spell(i),
                    name: String::from("Recall"),
                    item: None,
                    to: place(&mut next),
                    mana: 0,
                })
                .filter(|_| world % 2 == 0)
                .collect();
            for _ in 0..100 {
                let (a, b) = (place(&mut next), place(&mut next));
                let fast = plan_journey(&a, &b, &portals, &teleports, &dungeon_realm, &config);
                let slow = exhaustive(&a, &b, &portals, &teleports, &dungeon_realm, &config);
                match (fast, slow) {
                    (Some(j), Some(c)) => {
                        assert!((j.seconds - c).abs() < 1e-2, "{} against {c}", j.seconds);
                        found += 1;
                    }
                    (None, None) => {}
                    (f, s) => panic!("world {world}: {f:?} against {s:?}"),
                }
                compared += 1;
            }
        }
        assert_eq!(compared, 2000);
        assert!(found > 500, "only {found} journeys found");
    }

    #[test]
    fn a_dungeon_is_reached_by_walking_to_its_portal_and_using_it() {
        let p = portal_to_dungeon(EdgeSource::Learned);
        let j = plan_journey(
            &at(0xA9B4_0001, 10.0, 10.0),
            &at(0x0106_0120, 30.0, -40.0),
            &[&p],
            &[],
            &realm,
            &JourneyConfig::default(),
        )
        .expect("through the portal");
        assert!(matches!(j.legs[0], Leg::Walk { .. }));
        assert!(matches!(j.legs[1], Leg::Portal { .. }));
        assert!(matches!(j.legs[2], Leg::Walk { .. }));
        assert_eq!(j.legs.len(), 3);
        let none = plan_journey(
            &at(0xA9B4_0001, 10.0, 10.0),
            &at(0x0106_0120, 30.0, -40.0),
            &[],
            &[],
            &realm,
            &JourneyConfig::default(),
        );
        assert!(
            none.is_none(),
            "without the portal the dungeon is not walked to"
        );
    }

    #[test]
    fn a_recall_is_taken_when_it_is_cheaper_than_the_walk_and_only_when_it_can_be_cast() {
        let mut store = PortalStore::new("w");
        store.learn_recall("me", Recall::Spell(1635), &at(0xA9B4_0021, 96.0, 96.0), 1);
        let mut me = CharacterState {
            name: String::from("me"),
            level: Some(1),
            mana: Some(100),
            recall_spells: vec![(1635, String::from("Lifestone Recall"), 50)],
            recall_items: Vec::new(),
            recall_commands: vec![(Recall::Lifestone, String::from("lifestone"))],
            can_cast: true,
        };
        // Ten landblocks away from home: recalling beats a long walk.
        let far = at(0xB3B4_0001, 10.0, 10.0);
        let home = at(0xA9B4_0021, 100.0, 96.0);
        let t = me.teleports(&store);
        let j = plan_journey(&far, &home, &[], &t, &realm, &JourneyConfig::default())
            .expect("a journey");
        assert!(matches!(j.legs[0], Leg::Teleport { .. }), "{:?}", j.legs);
        // Next door: walking wins.
        let near = at(0xA9B4_0011, 60.0, 96.0);
        let j = plan_journey(&near, &home, &[], &t, &realm, &JourneyConfig::default())
            .expect("a journey");
        assert!(j.legs.iter().all(|l| matches!(l, Leg::Walk { .. })));
        // Without the mana, or unable to cast, or never having recalled, there is no recall.
        me.mana = Some(10);
        assert!(me.teleports(&store).is_empty());
        me.mana = Some(100);
        me.can_cast = false;
        assert!(me.teleports(&store).is_empty());
        me.can_cast = true;
        assert!(me.teleports(&PortalStore::new("w")).is_empty());
        // An item carries its spell regardless of mana.
        me.mana = Some(0);
        me.recall_items = vec![(ObjectId(77), 1635, String::from("Lifestone Recall Gem"))];
        assert_eq!(me.teleports(&store)[0].item, Some(ObjectId(77)));
        // A recall by command needs neither mana nor casting, only a destination learned.
        me.recall_items.clear();
        me.can_cast = false;
        assert!(me.teleports(&store).is_empty());
        store.learn_recall("me", Recall::Lifestone, &at(0xA9B4_0021, 96.0, 96.0), 2);
        assert_eq!(me.teleports(&store)[0].recall, Recall::Lifestone);
    }

    #[test]
    fn an_untried_portal_is_taken_only_when_clearly_better_and_a_retired_one_never() {
        let start = at(0xA9B4_0011, 40.0, 50.0);
        let goal = at(0xAAB4_0011, 50.0, 50.0);
        // A hinted portal standing beside the start and sending to the goal, versus a 192 m walk.
        let mut hint = portal_to_dungeon(EdgeSource::Hint);
        hint.to = Some(goal);
        let j = plan_journey(
            &start,
            &goal,
            &[&hint],
            &[],
            &realm,
            &JourneyConfig::default(),
        )
        .expect("a journey");
        assert!(
            j.legs.iter().any(|l| matches!(l, Leg::Portal { .. })),
            "{:?}",
            j.legs
        );
        // The same portal for a short walk: not worth the doubt.
        let near_goal = at(0xA9B4_0011, 90.0, 50.0);
        hint.to = Some(near_goal);
        let j = plan_journey(
            &start,
            &near_goal,
            &[&hint],
            &[],
            &realm,
            &JourneyConfig::default(),
        )
        .expect("a journey");
        assert!(j.legs.iter().all(|l| matches!(l, Leg::Walk { .. })));
        // Retired by a refusal: the store stops offering it.
        let mut store = PortalStore::new("w");
        store.merge([hint.clone()]);
        assert_eq!(store.routable(None).len(), 1);
        store.learn_refusal(
            &PortalSighting {
                wcid: hint.wcid,
                name: hint.name.clone(),
                position: hint.from,
            },
            "refused",
            9,
        );
        assert!(store.routable(None).is_empty());
    }

    /// Two parts of one dungeon (`0x8602`), told apart by their cells as the Training Academy's
    /// are: cells below `0x01AD` the second part, the rest the first.
    fn academy_realm(p: &Position) -> Option<Realm> {
        land_realm(p.cell).or(Some(if p.cell.0 >> 16 == 0x8602 {
            Realm::Part(CellId(if p.cell.0 & 0xFFFF < 0x01AD {
                0x8602_0100
            } else {
                0x8602_01AD
            }))
        } else {
            Realm::Outdoors
        }))
    }

    fn jonathan() -> HandBack {
        HandBack {
            name: String::from("Jonathan"),
            object: Some(ObjectId(0x8000_0116)),
            at: at(0x8602_01B0, 22.1, -19.1),
            token: String::from("Academy Exit Token"),
            to: at(0xA9B4_0019, 84.0, 7.1),
        }
    }

    #[test]
    fn a_journey_out_of_a_part_no_portal_leaves_takes_the_hand_back_from_there_only() {
        let start = at(0x8602_01AD, 12.3, -28.5);
        let town = at(0xA9B4_0021, 96.0, 96.0);
        let exit = PortalEdge {
            source: EdgeSource::Published,
            wcid: Some(29338),
            name: String::from("Exit to Holtburg"),
            from: at(0x8602_0169, 158.6, -149.5),
            to: Some(at(0xA9B4_0019, 84.0, 7.1)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        };
        let config = JourneyConfig::default();
        // The exit portal stands in the other part: no journey without the hand-back.
        assert!(plan_journey(&start, &town, &[&exit], &[], &academy_realm, &config).is_none());
        let j = plan_journey_with(
            &start,
            &town,
            &[&exit],
            &[],
            &[jonathan()],
            &academy_realm,
            &config,
        )
        .expect("out by the hand-back");
        assert!(
            matches!(
                j.legs.as_slice(),
                [Leg::Walk { to }, Leg::HandBack { hand_back }, Leg::Walk { .. }]
                    if *to == jonathan().at && hand_back.name == "Jonathan"
            ),
            "{:?}",
            j.legs
        );
        // From the exit portal's part the hand-back is not to be had: the portal it is.
        let far_part = at(0x8602_0134, 120.0, -160.0);
        let j = plan_journey_with(
            &far_part,
            &town,
            &[&exit],
            &[],
            &[jonathan()],
            &academy_realm,
            &config,
        )
        .expect("out by the portal");
        assert!(
            j.legs.iter().any(|l| matches!(l, Leg::Portal { .. }))
                && !j.legs.iter().any(|l| matches!(l, Leg::HandBack { .. })),
            "{:?}",
            j.legs
        );
    }

    #[test]
    fn a_portal_out_of_the_same_part_is_taken_before_the_hand_back() {
        let start = at(0x8602_01AD, 12.3, -28.5);
        let town = at(0xA9B4_0021, 96.0, 96.0);
        let mut out = portal_to_dungeon(EdgeSource::Learned);
        out.from = at(0x8602_01B1, 21.9, -30.6);
        out.to = Some(at(0xA9B4_0019, 84.0, 7.1));
        let j = plan_journey_with(
            &start,
            &town,
            &[&out],
            &[],
            &[jonathan()],
            &academy_realm,
            &JourneyConfig::default(),
        )
        .expect("a journey");
        assert!(
            !j.legs.iter().any(|l| matches!(l, Leg::HandBack { .. })),
            "{:?}",
            j.legs
        );
    }
}
