//! How a creature moves while nothing is fighting it: its movement type, whether it wanders and
//! how far and how often, its homesick and perception radii, whether it uses doors, and the route
//! it walks, if it has one.
//!
//! The six settings and the doors flag are template properties (`AI_MovementType`,
//! `AI_Wandering`, `AI_WanderingRange`, `AI_WanderingProb`, `AI_HomesickRadius`,
//! `AI_PerceptionRadius`, `AI_CanUseDoors`); a template that leaves one out takes the value the
//! game's own scripts read in its place ([`Movement::default`]). A camp may set any of them for
//! the creatures it makes, over their template's ([`MovementOverrides`]).
//!
//! No setting of the game describes a patrol. A **route** is this server's form for one: places
//! walked in turn, back and forth or round a loop, with a rest at each, at a walk or a run
//! ([`Route`]). A camp names a route and the waypoint its creatures start toward, and which way
//! they go, so several creatures can share one route.

use std::fmt::Write as _;

use crate::props::{creature_property as k, Prop, Props};

/// How a creature may move at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MovementType {
    /// Never moves, nor turns.
    Stationary,
    /// Turns where it stands, never moving from it.
    TurnOnly,
    /// Kept near its home: it wanders round it and goes back to it after a fight.
    #[default]
    ConstrainedToHome,
    /// Follows its master (a pet).
    ConstrainedToMaster,
    /// Not tied to its home: no distance from home ends a fight.
    FreeRange,
}

impl MovementType {
    /// The kinds, in the order of their values.
    pub const ALL: [Self; 5] = [
        Self::Stationary,
        Self::TurnOnly,
        Self::ConstrainedToHome,
        Self::ConstrainedToMaster,
        Self::FreeRange,
    ];

    /// The kind of a template's `AI_MovementType` value.
    #[must_use]
    pub const fn from_value(v: u32) -> Option<Self> {
        Some(match v {
            0x4000_0001 => Self::Stationary,
            0x4000_0002 => Self::TurnOnly,
            0x4000_0003 => Self::ConstrainedToHome,
            0x4000_0004 => Self::ConstrainedToMaster,
            0x4000_0005 => Self::FreeRange,
            _ => return None,
        })
    }

    /// Its value, as a template writes it.
    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Stationary => 0x4000_0001,
            Self::TurnOnly => 0x4000_0002,
            Self::ConstrainedToHome => 0x4000_0003,
            Self::ConstrainedToMaster => 0x4000_0004,
            Self::FreeRange => 0x4000_0005,
        }
    }

    /// Its word, as the world database writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Stationary => "stationary",
            Self::TurnOnly => "turn-only",
            Self::ConstrainedToHome => "constrained-to-home",
            Self::ConstrainedToMaster => "constrained-to-master",
            Self::FreeRange => "free-range",
        }
    }

    /// The kind a word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.word() == w)
    }
}

/// The values a template that leaves a setting out takes: the ones the game's scripts read in its
/// place.
pub mod default {
    use super::MovementType;
    pub const KIND: MovementType = MovementType::ConstrainedToHome;
    pub const WANDERS: bool = true;
    pub const WANDER_RANGE: f32 = 25.0;
    pub const WANDER_CHANCE: f32 = 0.1;
    pub const HOMESICK: f32 = 40.0;
    pub const PERCEPTION: f32 = 20.0;
    pub const USES_DOORS: bool = true;
}

/// The homesick radius under which a creature never wanders, in meters (2017: 0 of 40 creatures
/// of a radius of 4 m or 0 walked while at rest, over 7.3 hours; 17% to 44% of those of every larger
/// radius did).
pub const SETTLED_HOMESICK: f32 = 5.0;

/// A creature's movement settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Movement {
    pub kind: MovementType,
    /// Whether it wanders at rest.
    pub wanders: bool,
    /// How far from home a wander goes, in meters.
    pub wander_range: f32,
    /// The chance a wander sets out when one falls due.
    pub wander_chance: f32,
    /// The distance from home it keeps to, in meters.
    pub homesick: f32,
    /// How near a player must come to be noticed, in meters.
    pub perception: f32,
    /// Whether it opens doors.
    pub uses_doors: bool,
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            kind: default::KIND,
            wanders: default::WANDERS,
            wander_range: default::WANDER_RANGE,
            wander_chance: default::WANDER_CHANCE,
            homesick: default::HOMESICK,
            perception: default::PERCEPTION,
            uses_doors: default::USES_DOORS,
        }
    }
}

impl Movement {
    /// A template's settings, each left out taking its default. A negative wandering range is
    /// none.
    #[must_use]
    pub fn from_props(p: &Props) -> Self {
        let d = Self::default();
        let flag = |name: &str, or: bool| match p.get(name) {
            Some(Prop::Bool(b)) => *b,
            _ => or,
        };
        Self {
            kind: p
                .word(k::MOVEMENT_TYPE)
                .and_then(MovementType::from_value)
                .unwrap_or(d.kind),
            wanders: flag(k::WANDERS, d.wanders),
            wander_range: p.float(k::WANDER_RANGE).unwrap_or(d.wander_range).max(0.0),
            wander_chance: p.float(k::WANDER_CHANCE).unwrap_or(d.wander_chance),
            homesick: p.float(k::HOMESICK).unwrap_or(d.homesick),
            perception: p.float(k::PERCEPTION).unwrap_or(d.perception),
            uses_doors: flag(k::USES_DOORS, d.uses_doors),
        }
    }

    /// These settings with a camp's over them.
    #[must_use]
    pub fn with(self, o: &MovementOverrides) -> Self {
        Self {
            kind: o.kind.unwrap_or(self.kind),
            wanders: o.wanders.unwrap_or(self.wanders),
            wander_range: o.wander_range.unwrap_or(self.wander_range).max(0.0),
            wander_chance: o.wander_chance.unwrap_or(self.wander_chance),
            homesick: o.homesick.unwrap_or(self.homesick),
            perception: o.perception.unwrap_or(self.perception),
            uses_doors: o.uses_doors.unwrap_or(self.uses_doors),
        }
    }

    /// It never wanders at rest: it never moves or only turns, it follows a master, it does not
    /// wander, or its homesick radius is under [`SETTLED_HOMESICK`] (2017: no creature of the 7
    /// templates saying they do not wander walked at rest, an hour or more of each watched; 342 of
    /// the 416 others did).
    #[must_use]
    pub fn settled(&self) -> bool {
        !self.wanders || self.homesick < SETTLED_HOMESICK || !self.walks()
    }

    /// Whether it may walk about at all, a route or a wander: neither fixed in place nor tied to
    /// a master.
    #[must_use]
    pub const fn walks(&self) -> bool {
        matches!(
            self.kind,
            MovementType::ConstrainedToHome | MovementType::FreeRange
        )
    }

    /// Whether it turns where it stands but never moves.
    #[must_use]
    pub const fn turns_only(&self) -> bool {
        matches!(self.kind, MovementType::TurnOnly)
    }
}

/// What a camp sets for its creatures over their template's settings.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementOverrides {
    pub kind: Option<MovementType>,
    pub wanders: Option<bool>,
    pub wander_range: Option<f32>,
    pub wander_chance: Option<f32>,
    pub homesick: Option<f32>,
    pub perception: Option<f32>,
    pub uses_doors: Option<bool>,
}

impl MovementOverrides {
    /// Whether it sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The overrides as the spawn table writes them: `name=value` pairs joined by `;`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = Vec::new();
        if let Some(m) = self.kind {
            out.push(format!("movement={}", m.word()));
        }
        let flag = |b: bool| u8::from(b);
        if let Some(v) = self.wanders {
            out.push(format!("wanders={}", flag(v)));
        }
        for (name, v) in [
            ("wander_range", self.wander_range),
            ("wander_chance", self.wander_chance),
            ("homesick", self.homesick),
            ("perception", self.perception),
        ] {
            if let Some(v) = v {
                out.push(format!("{name}={v}"));
            }
        }
        if let Some(v) = self.uses_doors {
            out.push(format!("uses_doors={}", flag(v)));
        }
        out.join(";")
    }

    /// Read the overrides' text.
    ///
    /// # Errors
    /// A pair that is not one of the settings, or a value that does not read.
    pub fn from_text(s: &str) -> Result<Self, String> {
        let mut o = Self::default();
        for pair in s.split(';').filter(|p| !p.is_empty()) {
            let (name, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("a setting is name=value, not {pair}"))?;
            let num = || v.parse::<f32>().map_err(|e| format!("{name}: {v}: {e}"));
            let flag = || match v {
                "1" => Ok(true),
                "0" => Ok(false),
                _ => Err(format!("{name}: {v} is not 0 or 1")),
            };
            match name {
                "movement" => {
                    o.kind = Some(
                        MovementType::from_word(v)
                            .ok_or_else(|| format!("movement: {v} is no movement type"))?,
                    );
                }
                "wanders" => o.wanders = Some(flag()?),
                "wander_range" => o.wander_range = Some(num()?),
                "wander_chance" => o.wander_chance = Some(num()?),
                "homesick" => o.homesick = Some(num()?),
                "perception" => o.perception = Some(num()?),
                "uses_doors" => o.uses_doors = Some(flag()?),
                other => return Err(format!("{other} is no movement setting")),
            }
        }
        Ok(o)
    }
}

/// How a route is walked when its last waypoint is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// Back the way it came, to the first, and so on.
    #[default]
    BackAndForth,
    /// On to the first again.
    Loop,
}

impl Shape {
    /// Its word, as the world database writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::BackAndForth => "back-and-forth",
            Self::Loop => "loop",
        }
    }

    /// The shape a word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        [Self::BackAndForth, Self::Loop]
            .into_iter()
            .find(|s| s.word() == w)
    }
}

/// How fast a route is walked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gait {
    #[default]
    Walk,
    Run,
}

impl Gait {
    /// Its word, as the world database writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Run => "run",
        }
    }

    /// The gait a word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        [Self::Walk, Self::Run].into_iter().find(|g| g.word() == w)
    }
}

/// One place of a route: a cell, a point in the cell's landblock, and how long a creature rests
/// there, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waypoint {
    pub cell: u32,
    pub at: [f32; 3],
    pub rest: f64,
}

/// A route creatures walk at rest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Route {
    /// Its key, as the world names it.
    pub key: String,
    pub shape: Shape,
    pub gait: Gait,
    /// How fast it is walked by a creature of scale 1, in meters a second, when not its gait's own
    /// pace.
    pub speed: Option<f32>,
    pub waypoints: Vec<Waypoint>,
}

impl Route {
    /// The waypoint after `at`, going the way `backward` says, and the way it goes on from there:
    /// a loop wraps round; a back-and-forth route turns at either end. A route of one waypoint
    /// stays there.
    #[must_use]
    pub fn next(&self, at: usize, backward: bool) -> (usize, bool) {
        let n = self.waypoints.len();
        if n < 2 {
            return (0, backward);
        }
        match (self.shape, backward) {
            (Shape::Loop, false) => ((at + 1) % n, false),
            (Shape::Loop, true) => ((at + n - 1) % n, true),
            (Shape::BackAndForth, false) if at + 1 >= n => (n - 2, true),
            (Shape::BackAndForth, false) => (at + 1, false),
            (Shape::BackAndForth, true) if at == 0 => (1, false),
            (Shape::BackAndForth, true) => (at - 1, true),
        }
    }
}

/// How a camp's creatures move: settings over their templates', and the route they walk, with the
/// waypoint they start toward and which way they go.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CampMoves {
    pub settings: MovementOverrides,
    pub route: Option<Route>,
    pub start: usize,
    pub backward: bool,
}

impl CampMoves {
    /// The route as the spawn table writes it: `key,shape,gait,speed,start,way` (the speed empty
    /// for the gait's own) then each waypoint as `cell:x:y:z:rest`, joined by `|`; empty for none.
    #[must_use]
    pub fn route_text(&self) -> String {
        let Some(r) = &self.route else {
            return String::new();
        };
        let mut out = format!(
            "{},{},{},{},{},{}",
            r.key,
            r.shape.word(),
            r.gait.word(),
            r.speed.map(|v| v.to_string()).unwrap_or_default(),
            self.start,
            if self.backward { "backward" } else { "forward" }
        );
        for w in &r.waypoints {
            let [x, y, z] = w.at;
            let _ = write!(out, "|{:08X}:{x}:{y}:{z}:{}", w.cell, w.rest);
        }
        out
    }

    /// Read a camp's movement settings and route from their text.
    ///
    /// # Errors
    /// A setting or a route that does not read.
    pub fn from_text(settings: &str, route: &str) -> Result<Self, String> {
        let mut m = Self {
            settings: MovementOverrides::from_text(settings)?,
            ..Self::default()
        };
        if route.is_empty() {
            return Ok(m);
        }
        let mut parts = route.split('|');
        let head: Vec<&str> = parts.next().unwrap_or_default().split(',').collect();
        let [key, shape, gait, speed, start, way] = head[..] else {
            return Err(format!(
                "a route is key,shape,gait,speed,start,way, not {}",
                head.join(",")
            ));
        };
        let num = |s: &str| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
        let mut waypoints = Vec::new();
        for w in parts {
            let p: Vec<&str> = w.split(':').collect();
            let [cell, x, y, z, rest] = p[..] else {
                return Err(format!("a waypoint is cell:x:y:z:rest, not {w}"));
            };
            waypoints.push(Waypoint {
                cell: u32::from_str_radix(cell, 16).map_err(|e| format!("{cell}: {e}"))?,
                at: [num(x)?, num(y)?, num(z)?],
                rest: rest.parse().map_err(|e| format!("{rest}: {e}"))?,
            });
        }
        m.route = Some(Route {
            key: key.to_string(),
            shape: Shape::from_word(shape).ok_or_else(|| format!("{shape} is no route shape"))?,
            gait: Gait::from_word(gait).ok_or_else(|| format!("{gait} is no gait"))?,
            speed: if speed.is_empty() {
                None
            } else {
                Some(num(speed)?)
            },
            waypoints,
        });
        m.start = start.parse().map_err(|e| format!("{start}: {e}"))?;
        m.backward = match way {
            "forward" => false,
            "backward" => true,
            other => return Err(format!("{other} is not forward or backward")),
        };
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the settings, their defaults and the route's order, on hand-built values)

    use super::*;

    fn route(shape: Shape, n: usize) -> Route {
        Route {
            key: "r".into(),
            shape,
            gait: Gait::Walk,
            speed: None,
            waypoints: (0..n)
                .map(|i| Waypoint {
                    cell: 1,
                    #[allow(clippy::cast_precision_loss)]
                    at: [i as f32, 0.0, 0.0],
                    rest: 0.0,
                })
                .collect(),
        }
    }

    #[test]
    fn a_back_and_forth_route_turns_at_either_end_and_a_loop_wraps_round() {
        let b = route(Shape::BackAndForth, 3);
        let mut walked = vec![0];
        let (mut at, mut back) = (0, false);
        for _ in 0..6 {
            (at, back) = b.next(at, back);
            walked.push(at);
        }
        assert_eq!(walked, [0, 1, 2, 1, 0, 1, 2]);
        let l = route(Shape::Loop, 4);
        let mut walked = vec![1];
        let (mut at, mut back) = (1, true);
        for _ in 0..5 {
            (at, back) = l.next(at, back);
            walked.push(at);
        }
        assert_eq!(walked, [1, 0, 3, 2, 1, 0]);
        assert_eq!(l.next(3, false), (0, false), "on round to the first");
        assert_eq!(route(Shape::Loop, 1).next(0, false), (0, false));
    }

    #[test]
    fn a_template_leaving_a_setting_out_takes_the_scripts_default_and_a_camp_overrides_it() {
        let mut p = Props::default();
        p.set(k::PERCEPTION, Prop::Float(12.0));
        p.set(k::MOVEMENT_TYPE, Prop::Word(0x4000_0005));
        p.set(k::WANDERS, Prop::Bool(false));
        let m = Movement::from_props(&p);
        assert_eq!(m.kind, MovementType::FreeRange);
        assert!(!m.wanders);
        assert!((m.perception - 12.0).abs() < 1e-6);
        assert!((m.wander_range - 25.0).abs() < 1e-6);
        assert!((m.wander_chance - 0.1).abs() < 1e-6);
        assert!((m.homesick - 40.0).abs() < 1e-6);
        assert!(m.uses_doors);
        assert_eq!(Movement::from_props(&Props::default()), Movement::default());
        let o = MovementOverrides {
            kind: Some(MovementType::Stationary),
            wander_range: Some(-3.0),
            ..MovementOverrides::default()
        };
        let m = m.with(&o);
        assert_eq!(m.kind, MovementType::Stationary);
        assert!(m.wander_range.abs() < 1e-6, "a negative range is none");
        assert!(m.settled() && !m.walks());
    }

    #[test]
    fn a_camps_moves_read_back_as_written() {
        let m = CampMoves {
            settings: MovementOverrides {
                kind: Some(MovementType::FreeRange),
                wanders: Some(false),
                perception: Some(12.5),
                uses_doors: Some(true),
                ..MovementOverrides::default()
            },
            route: Some(Route {
                key: "guard-1".into(),
                shape: Shape::Loop,
                gait: Gait::Run,
                speed: Some(3.65),
                waypoints: vec![
                    Waypoint {
                        cell: 0x048A_0101,
                        at: [60.0, 0.0, 12.5],
                        rest: 25.0,
                    },
                    Waypoint {
                        cell: 0x048A_0102,
                        at: [60.0, 80.0, 12.5],
                        rest: 0.0,
                    },
                ],
            }),
            start: 1,
            backward: true,
        };
        let back = CampMoves::from_text(&m.settings.to_text(), &m.route_text()).expect("reads");
        assert_eq!(back, m);
        assert_eq!(
            CampMoves::from_text("", "").expect("reads"),
            CampMoves::default()
        );
        assert!(MovementOverrides::from_text("movement=sideways").is_err());
    }
}
