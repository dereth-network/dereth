//! A scripted travel for the plugin driver: journeys planned over a case's own portals and
//! carried out on the scripted body.
//!
//! The scripted world has no game data: a walking leg runs the body at
//! [`crate::plugins::RUN_SPEED`] along the walking route round the case's walls (below), corner by
//! corner, and straight to its end where there is none. A portal leg walks to the portal and,
//! a second later, lands the body at the portal's destination, or (a portal the case marks as
//! refusing) leaves it standing, which the travel learns as a refusal before planning again. A
//! recall the case lists lands the body at its destination two seconds after it is used. The
//! journey search is the client's own ([`dereth_navigation::plan_journey`]); what the travel
//! tells is what the client's travel tells.
//!
//! A walking route (`nav.path`) is planned round the case's walls: the shortest way from the
//! character to the target over the corners just off each wall's ends, each leg clear of every
//! wall; a closed door on a leg is a door the route opens. A target no way reaches has none.
//!
//! A ledge of the case is a wall to a walking route. A walking leg that only a drop off a ledge
//! carries on (a way round the walls alone crosses it) fails as the client's own does, naming the
//! drop: where to stand at the ledge's edge and where the body lands.

use std::collections::{BTreeSet, VecDeque};

use dereth_client_runtime::travel::{
    Learned, TravelAllow, TravelEvent, TravelLeg, TravelPlan, TravelRequest, TravelRoute,
    TravelStatus, TravelTarget, TripStats, TripView,
};
use dereth_navigation::{
    land_realm, plan_journey, EdgeSource, Journey, JourneyConfig, Leg, PortalEdge, PortalSighting,
    PortalStore, Realm, Recall, Teleport,
};
use dereth_primitives::Position;

use crate::plugins::{ScriptedPlaces, RUN_SPEED, WORLD_NAME};

/// A portal of a scripted case.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptedPortal {
    pub name: String,
    pub class: u32,
    /// Where it stands, and where it sends.
    pub at: Position,
    pub to: Position,
    /// How the store knows it.
    pub source: EdgeSource,
    /// The server refuses the character at it.
    pub refuses: bool,
}

/// A recall of a scripted case: by command, landing there.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptedRecall {
    pub recall: Recall,
    pub to: Position,
}

/// How far off a wall's ends a walking route turns, and how near a wall a leg may pass, metres.
const ROUND: f64 = 1.0;
const LEG_CLEAR: f64 = 0.3;

/// A straight stretch on the ground between two points, metres from the world's corner.
type Segment = ((f64, f64), (f64, f64));

/// A place's ground point in metres from the world's corner.
fn flat(p: &Position) -> (f64, f64) {
    let (x, y, _) = dereth_plugin_host::places::global(p);
    (x, y)
}

/// The outdoor place at the ground point `(x, y)` (metres from the world's corner), at height `z`.
fn unflat((x, y): (f64, f64), z: f64) -> Option<Position> {
    use dereth_plugin_host::places::{from_coords, ORIGIN, UNIT};
    from_coords(y / UNIT - ORIGIN, x / UNIT - ORIGIN, z)
}

/// The walking route from `from` to `goal` round the walls and ledges of `places`: the shortest
/// over the corners off each one's ends, each leg clear of every one.
fn walking_route(from: &Position, goal: &Position, places: &ScriptedPlaces) -> Option<TravelRoute> {
    let walls: Vec<Segment> = places
        .walls
        .iter()
        .map(|(a, b)| (flat(a), flat(b)))
        .chain(places.ledges.iter().map(|(a, b, _)| (flat(a), flat(b))))
        .collect();
    route_round(from, goal, &walls, places)
}

/// Where a walking leg from `from` to `goal` goes on only by a drop: the way round the walls
/// alone crosses a ledge. Answers the failure naming the first such drop, as the client's travel
/// names it.
fn drop_on_the_way(from: &Position, goal: &Position, places: &ScriptedPlaces) -> Option<String> {
    let walls: Vec<Segment> = places
        .walls
        .iter()
        .map(|(a, b)| (flat(a), flat(b)))
        .collect();
    let route = route_round(from, goal, &walls, places)?;
    let place = |p: &Position| {
        let o = p.frame.origin;
        format!("{:08X} ({:.1}, {:.1}, {:.1})", p.cell.0, o.x, o.y, o.z)
    };
    route.points.windows(2).find_map(|w| {
        let (a, b) = (flat(&w[0]), flat(&w[1]));
        places.ledges.iter().find_map(|(c, d, height)| {
            let t = crossing(a, b, flat(c), flat(d))?;
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dereth_primitives::num::math::hypot(dx, dy).max(f64::EPSILON);
            let at = |metres: f64| {
                (
                    a.0 + dx * t + dx / len * metres,
                    a.1 + dy * t + dy / len * metres,
                )
            };
            let stand = unflat(at(-0.5), *height)?;
            let land = unflat(at(1.0), 0.0)?;
            Some(format!(
                "only_by_drop: a drop of {height:.1} m, off the ledge at {} to {}",
                place(&stand),
                place(&land)
            ))
        })
    })
}

/// How far along `a`–`b` it crosses `c`–`d`, as a fraction of it, or None.
fn crossing(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> Option<f64> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() <= f64::EPSILON {
        return None;
    }
    let (qx, qy) = (c.0 - a.0, c.1 - a.1);
    let t = (qx * s.1 - qy * s.0) / den;
    let u = (qx * r.1 - qy * r.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The shortest walking route from `from` to `goal` round `walls`: over the corners off each
/// one's ends, each leg clear of every one.
fn route_round(
    from: &Position,
    goal: &Position,
    walls: &[Segment],
    places: &ScriptedPlaces,
) -> Option<TravelRoute> {
    use crate::plugins::{point_segment_distance, segment_distance, DOOR_RADIUS};
    use dereth_primitives::num::math::hypot;
    let clear = |a: (f64, f64), b: (f64, f64)| {
        walls
            .iter()
            .all(|(c, d)| segment_distance(a, b, *c, *d) >= LEG_CLEAR)
    };
    let mut nodes = vec![flat(from), flat(goal)];
    for (a, b) in walls {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = hypot(dx, dy).max(f64::EPSILON);
        let (ux, uy) = (dx / len, dy / len);
        for (end, out) in [(*a, -1.0), (*b, 1.0)] {
            for side in [-1.0, 1.0] {
                let p = (
                    end.0 + (ux * out - uy * side) * ROUND,
                    end.1 + (uy * out + ux * side) * ROUND,
                );
                if walls
                    .iter()
                    .all(|(c, d)| point_segment_distance(p, *c, *d) >= LEG_CLEAR)
                {
                    nodes.push(p);
                }
            }
        }
    }
    // The shortest way from node 0 to node 1, over the legs clear of the walls.
    let n = nodes.len();
    let mut best = vec![f64::INFINITY; n];
    let mut came = vec![usize::MAX; n];
    let mut done = vec![false; n];
    best[0] = 0.0;
    while let Some(i) = (0..n)
        .filter(|i| !done[*i] && best[*i].is_finite())
        .min_by(|a, b| best[*a].total_cmp(&best[*b]))
    {
        done[i] = true;
        if i == 1 {
            break;
        }
        for j in 0..n {
            if done[j] || !clear(nodes[i], nodes[j]) {
                continue;
            }
            let d = best[i] + hypot(nodes[j].0 - nodes[i].0, nodes[j].1 - nodes[i].1);
            if d < best[j] {
                best[j] = d;
                came[j] = i;
            }
        }
    }
    if !best[1].is_finite() {
        return None;
    }
    let mut chain = vec![1];
    while let Some(&last) = chain.last() {
        if last == 0 {
            break;
        }
        chain.push(came[last]);
    }
    chain.reverse();
    let z = f64::from(goal.frame.origin.z);
    let mut points = vec![*from];
    for &i in &chain[1..] {
        points.push(if i == 1 { *goal } else { unflat(nodes[i], z)? });
    }
    // A closed door on a leg is opened from the corner the leg starts at.
    let mut doors = Vec::new();
    for (i, leg) in points.windows(2).enumerate() {
        let (a, b) = (flat(&leg[0]), flat(&leg[1]));
        for (id, d) in &places.closed_doors {
            if point_segment_distance(flat(d), a, b) < DOOR_RADIUS + LEG_CLEAR {
                doors.push((dereth_primitives::ObjectId(*id), i));
            }
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(TravelRoute {
        length: best[1] as f32,
        points,
        doors,
        complete: true,
    })
}

/// Seconds a portal takes to land or be refused, and a recall to land.
const PORTAL_SECS: f64 = 1.0;
const RECALL_SECS: f64 = 2.0;
/// A walk ends this close to its end, and a portal is used from this close.
const REACH: f64 = 0.5;

#[derive(Debug, Clone)]
enum Phase {
    /// Running to `to`.
    Walk { to: Position },
    /// A teleport lands (`Some`) or is refused (`None`) at `at`.
    Teleport {
        at: f64,
        to: Option<Position>,
        from: Option<PortalSighting>,
        recall: Option<Recall>,
    },
}

#[derive(Debug)]
struct Trip {
    id: u64,
    goal: Position,
    allow: TravelAllow,
    journey: Journey,
    leg: usize,
    phase: Option<Phase>,
    stats: TripStats,
    started: f64,
}

/// The scripted travel.
#[derive(Debug)]
pub struct ScriptedTravel {
    pub store: PortalStore,
    refusing: BTreeSet<u32>,
    recalls: Vec<ScriptedRecall>,
    requests: VecDeque<TravelRequest>,
    trip: Option<Trip>,
    /// What it told this frame, and the journey in progress, for the next frame's hook.
    pub status: TravelStatus,
    /// What it did, as lines.
    pub moves: Vec<String>,
}

impl Default for ScriptedTravel {
    fn default() -> Self {
        Self {
            store: PortalStore::new(WORLD_NAME),
            refusing: BTreeSet::new(),
            recalls: Vec::new(),
            requests: VecDeque::new(),
            trip: None,
            status: TravelStatus::default(),
            moves: Vec::new(),
        }
    }
}

/// Outdoors on land cells; each landblock's interior cells a dungeon of their own.
fn realm(p: &Position) -> Option<Realm> {
    land_realm(p.cell).or_else(|| u16::try_from(p.cell.0 >> 16).ok().map(Realm::Dungeon))
}

/// The distance between two places, in metres: across the map outdoors, within the cell's space
/// in a dungeon.
fn distance(a: &Position, b: &Position) -> f64 {
    f64::from(dereth_navigation::global_distance(a, b))
}

/// `from` moved `step` metres toward `to`, in `to`'s cell.
fn toward(from: &Position, to: &Position, step: f64) -> Position {
    let d = distance(from, to);
    if d <= step || d <= f64::EPSILON {
        return *to;
    }
    let (a, b) = (
        dereth_navigation::global(from),
        dereth_navigation::global(to),
    );
    #[allow(clippy::cast_possible_truncation)]
    let k = (step / d) as f32;
    let mut out = *to;
    out.frame.origin.x -= (b.x - a.x) * (1.0 - k);
    out.frame.origin.y -= (b.y - a.y) * (1.0 - k);
    out.frame.origin.z -= (b.z - a.z) * (1.0 - k);
    out
}

impl ScriptedTravel {
    /// The case's portals and recalls.
    pub fn set_world(&mut self, portals: &[ScriptedPortal], recalls: Vec<ScriptedRecall>) {
        self.store = PortalStore::new(WORLD_NAME);
        self.store.merge(portals.iter().map(|p| PortalEdge {
            source: p.source,
            wcid: Some(p.class),
            name: p.name.clone(),
            from: p.at,
            to: Some(p.to),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        }));
        self.refusing = portals
            .iter()
            .filter(|p| p.refuses)
            .map(|p| p.class)
            .collect();
        self.recalls = recalls;
    }

    /// Ask something of the travel, taken up at the next step.
    pub fn request(&mut self, r: TravelRequest) {
        self.requests.push_back(r);
    }

    fn teleports(&self, allow: TravelAllow, character: &str) -> Vec<Teleport> {
        let _ = character;
        if !allow.commands {
            return Vec::new();
        }
        self.recalls
            .iter()
            .map(|r| Teleport {
                recall: r.recall,
                name: r.recall.word(),
                item: None,
                to: r.to,
                mana: 0,
            })
            .collect()
    }

    fn plan(&self, from: &Position, goal: &Position, allow: TravelAllow) -> Option<Journey> {
        let portals = if allow.portals {
            self.store.routable(None)
        } else {
            Vec::new()
        };
        plan_journey(
            from,
            goal,
            &portals,
            &self.teleports(allow, ""),
            &realm,
            &JourneyConfig::default(),
        )
    }

    fn view(j: &Journey) -> TravelPlan {
        TravelPlan {
            legs: j
                .legs
                .iter()
                .map(|l| match l {
                    Leg::Walk { to } => TravelLeg::Walk {
                        to: *to,
                        path: Vec::new(),
                    },
                    Leg::Portal { edge } => TravelLeg::Portal {
                        name: edge.name.clone(),
                        wcid: edge.wcid,
                        from: edge.from,
                        to: edge.to,
                        source: edge.source,
                    },
                    Leg::Teleport { teleport } => TravelLeg::Recall {
                        name: teleport.name.clone(),
                        recall: teleport.recall,
                        item: teleport.item,
                        to: teleport.to,
                    },
                    Leg::HandBack { hand_back } => TravelLeg::HandBack {
                        name: hand_back.name.clone(),
                        token: hand_back.token.clone(),
                        at: hand_back.at,
                        to: hand_back.to,
                    },
                })
                .collect(),
            seconds: j.seconds,
        }
    }

    fn end(
        &mut self,
        events: &mut Vec<TravelEvent>,
        now: f64,
        at: Option<&Position>,
        failed: Option<&str>,
    ) {
        let Some(mut t) = self.trip.take() else {
            return;
        };
        t.stats.game_seconds = now - t.started;
        t.stats.wall_seconds = t.stats.game_seconds;
        match failed {
            None => {
                let d = at.map_or(0.0, |p| distance(p, &t.goal));
                #[allow(clippy::cast_possible_truncation)]
                events.push(TravelEvent::Arrived {
                    id: t.id,
                    distance: d as f32,
                    stats: t.stats,
                });
                self.moves.push(String::from("travel arrived"));
            }
            Some(reason) => {
                events.push(TravelEvent::Failed {
                    id: t.id,
                    reason: reason.to_owned(),
                    stats: t.stats,
                });
                self.moves.push(format!("travel failed {reason}"));
            }
        }
    }

    /// Plan the trip again from `from` and start its first leg.
    fn replan(
        &mut self,
        events: &mut Vec<TravelEvent>,
        now: f64,
        from: &Position,
        reason: Option<String>,
    ) {
        let Some(t) = self.trip.as_ref() else { return };
        let (id, goal, allow) = (t.id, t.goal, t.allow);
        let Some(j) = self.plan(from, &goal, allow) else {
            self.end(events, now, Some(from), Some("no_path"));
            return;
        };
        events.push(TravelEvent::Planned {
            id,
            plan: Self::view(&j),
            reason,
        });
        if let Some(t) = self.trip.as_mut() {
            t.stats.journeys += 1;
            t.journey = j;
            t.leg = 0;
            t.phase = None;
        }
        self.start_leg(events, now, from);
    }

    fn start_leg(&mut self, events: &mut Vec<TravelEvent>, now: f64, here: &Position) {
        let Some(t) = self.trip.as_mut() else { return };
        let Some(leg) = t.journey.legs.get(t.leg).cloned() else {
            self.end(events, now, Some(here), None);
            return;
        };
        events.push(TravelEvent::Leg {
            id: t.id,
            index: t.leg,
            leg: Self::view(&Journey {
                legs: vec![leg.clone()],
                seconds: 0.0,
            })
            .legs
            .remove(0),
        });
        t.phase = Some(match leg {
            Leg::Walk { to } => Phase::Walk { to },
            Leg::Portal { edge } => Phase::Walk { to: edge.from },
            // A scripted case offers no hand-backs: its journeys never plan one.
            Leg::HandBack { hand_back } => Phase::Walk { to: hand_back.at },
            Leg::Teleport { teleport } => {
                self.moves
                    .push(format!("travel recall {}", teleport.recall.word()));
                Phase::Teleport {
                    at: now + RECALL_SECS,
                    to: Some(teleport.to),
                    from: None,
                    recall: Some(teleport.recall),
                }
            }
        });
    }

    /// One frame of `dt` seconds at `now`: take up requests, carry the journey on, and keep what
    /// it told for the next frame's hook.
    pub fn step(&mut self, now: f64, dt: f64, places: &mut ScriptedPlaces, player_moved: bool) {
        let mut events = Vec::new();
        while let Some(r) = self.requests.pop_front() {
            let here = places.player;
            match r {
                TravelRequest::Cancel { id, reason } => {
                    if self.trip.as_ref().is_some_and(|t| t.id == id) {
                        self.end(&mut events, now, here.as_ref(), Some(&reason));
                    }
                }
                TravelRequest::Plan { id, target, allow } => {
                    let plan = match (here, self.target(target, places)) {
                        (Some(from), Some(goal)) => self
                            .plan(&from, &goal, allow)
                            .map(|j| Self::view(&j))
                            .ok_or_else(|| String::from("no_path")),
                        (None, _) => Err(String::from("no_position")),
                        (_, None) => Err(String::from("no_target")),
                    };
                    events.push(TravelEvent::Answer { id, plan });
                }
                TravelRequest::Go { id, target, allow } => {
                    if self.trip.is_some() {
                        self.end(&mut events, now, here.as_ref(), Some("replaced"));
                    }
                    let (Some(from), Some(goal)) = (here, self.target(target, places)) else {
                        events.push(TravelEvent::Failed {
                            id,
                            reason: String::from("no_target"),
                            stats: TripStats::default(),
                        });
                        continue;
                    };
                    self.trip = Some(Trip {
                        id,
                        goal,
                        allow,
                        journey: Journey {
                            legs: Vec::new(),
                            seconds: 0.0,
                        },
                        leg: 0,
                        phase: None,
                        stats: TripStats::default(),
                        started: now,
                    });
                    self.replan(&mut events, now, &from, None);
                }
                TravelRequest::Route { id, target } => {
                    let route = match (here, self.target(target, places)) {
                        (Some(from), Some(goal)) => walking_route(&from, &goal, places)
                            .ok_or_else(|| String::from("no_route")),
                        (None, _) => Err(String::from("no_position")),
                        (_, None) => Err(String::from("no_target")),
                    };
                    events.push(TravelEvent::Route { id, route });
                }
                TravelRequest::LearnRecalls { id } => events.push(TravelEvent::Failed {
                    id,
                    reason: String::from("no_recalls"),
                    stats: TripStats::default(),
                }),
            }
        }
        if player_moved && self.trip.is_some() {
            self.end(
                &mut events,
                now,
                places.player.as_ref(),
                Some("interrupted"),
            );
        }
        if let (Some(p), Some(phase)) = (
            places.player,
            self.trip.as_ref().and_then(|t| t.phase.clone()),
        ) {
            self.carry_on(&mut events, now, dt, places, p, phase);
        }
        self.status = TravelStatus {
            events,
            trip: self.trip.as_ref().map(|t| TripView {
                id: t.id,
                legs: Self::view(&t.journey).legs,
                leg: t.leg,
                target: match &t.phase {
                    Some(Phase::Walk { to }) => Some(*to),
                    Some(Phase::Teleport { to, .. }) => *to,
                    None => None,
                },
                route: t
                    .journey
                    .legs
                    .iter()
                    .skip(t.leg)
                    .filter_map(|l| match l {
                        Leg::Walk { to } => Some(*to),
                        Leg::Portal { edge } => edge.to,
                        Leg::Teleport { teleport } => Some(teleport.to),
                        Leg::HandBack { hand_back } => Some(hand_back.to),
                    })
                    .collect(),
                goal: t.goal,
            }),
        };
    }

    fn target(&self, target: TravelTarget, places: &ScriptedPlaces) -> Option<Position> {
        match target {
            TravelTarget::Place(p) => Some(p),
            TravelTarget::Object(o) => places.objects.get(&o.0).copied(),
            TravelTarget::Cell(_) => None,
        }
    }

    fn carry_on(
        &mut self,
        events: &mut Vec<TravelEvent>,
        now: f64,
        dt: f64,
        places: &mut ScriptedPlaces,
        p: Position,
        phase: Phase,
    ) {
        match phase {
            Phase::Walk { to } => {
                // Round the case's walls, as the client's own route goes: on to its next corner.
                let route = walking_route(&p, &to, places);
                if route.is_none() {
                    if let Some(why) = drop_on_the_way(&p, &to, places) {
                        self.end(events, now, Some(&p), Some(&why));
                        return;
                    }
                }
                let corner = route.and_then(|r| r.points.get(1).copied()).unwrap_or(to);
                let next = toward(&p, &corner, RUN_SPEED * dt);
                if let Some(t) = self.trip.as_mut() {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        t.stats.walked += distance(&p, &next) as f32;
                    }
                }
                places.player = Some(next);
                if distance(&next, &to) > REACH {
                    return;
                }
                let leg = self
                    .trip
                    .as_ref()
                    .and_then(|t| t.journey.legs.get(t.leg).cloned());
                match leg {
                    Some(Leg::Portal { edge }) => {
                        let refused = edge.wcid.is_some_and(|w| self.refusing.contains(&w));
                        self.moves.push(format!("travel portal {}", edge.name));
                        if let Some(t) = self.trip.as_mut() {
                            t.phase = Some(Phase::Teleport {
                                at: now + PORTAL_SECS,
                                to: (!refused).then_some(edge.to).flatten(),
                                from: Some(PortalSighting {
                                    wcid: edge.wcid,
                                    name: edge.name.clone(),
                                    position: edge.from,
                                }),
                                recall: None,
                            });
                        }
                    }
                    _ => {
                        if let Some(t) = self.trip.as_mut() {
                            t.leg += 1;
                        }
                        self.start_leg(events, now, &next);
                    }
                }
            }
            Phase::Teleport {
                at,
                to,
                from,
                recall,
            } => {
                if now < at {
                    return;
                }
                let id = self.trip.as_ref().map_or(0, |t| t.id);
                match (to, from) {
                    (Some(to), from) => {
                        places.player = Some(to);
                        match (from, recall) {
                            (Some(s), _) => {
                                self.store.learn_use(&s, &to, 1);
                                events.push(TravelEvent::Learned {
                                    id,
                                    what: Learned::Portal { name: s.name, to },
                                });
                                if let Some(t) = self.trip.as_mut() {
                                    t.stats.portal_uses += 1;
                                }
                            }
                            (None, Some(r)) => {
                                self.store.learn_recall("", r, &to, 1);
                                events.push(TravelEvent::Learned {
                                    id,
                                    what: Learned::Recall { recall: r, to },
                                });
                                if let Some(t) = self.trip.as_mut() {
                                    t.stats.recalls += 1;
                                }
                            }
                            (None, None) => {}
                        }
                        if let Some(t) = self.trip.as_mut() {
                            t.leg += 1;
                            t.phase = None;
                        }
                        self.start_leg(events, now, &to);
                    }
                    (None, Some(s)) => {
                        let reason = String::from("no teleport after the use");
                        self.store.learn_refusal(&s, &reason, 1);
                        events.push(TravelEvent::Learned {
                            id,
                            what: Learned::Refusal {
                                name: s.name.clone(),
                                reason,
                            },
                        });
                        if let Some(t) = self.trip.as_mut() {
                            t.stats.refusals += 1;
                            t.stats.replans += 1;
                        }
                        self.replan(events, now, &p, Some(format!("portal {} refused", s.name)));
                    }
                    (None, None) => {}
                }
            }
        }
    }
}
