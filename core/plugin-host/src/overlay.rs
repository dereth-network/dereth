//! Overlays: what a plugin marks in the world rather than in a window.
//!
//! A plugin adds **markers**: a marker on an object or a place in the world (`overlay.world`), a
//! blip on the radar (`overlay.radar`), points or a line on the map (`overlay.map`), and one
//! direction arrow toward an object or a place (`overlay.arrow`); and, in the world as well,
//! **shapes** (a line between two objects or places, a ring or a square on the ground, text
//! floating at a height) and the **nametags** layer (a label over every object of the kinds a
//! plugin names, within a range). Markers are data: the host keeps
//! them per plugin, resolves where each one is every frame ([`Overlays`]), and each client draws
//! them its own way (the 3-D client in the scene and on its radar and map; a host with no display
//! lists them). A plugin's markers go when it unloads, and a marker with `expires` (seconds) goes when its
//! time is up.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use dereth_client_runtime::frame_hook::Places;
use dereth_plugin_api::{CallError, HostCapability, Value};
use dereth_primitives::{ObjectId, Position};

use crate::places;

/// The most markers one plugin may keep.
pub const MAX_MARKERS: usize = 256;
/// The most points one map marker may hold.
pub const MAX_POINTS: usize = 4_096;

/// Where a marker is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    World,
    Radar,
    Map,
    Arrow,
    /// A line, a ring, a square or floating text in the 3-D view.
    Shape,
    /// The nametags layer; one per plugin. Resolved into one [`Kind::Nametag`] per object.
    Nametags,
    /// One object's nametag, as the layer resolved it.
    Nametag,
}

impl Kind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Radar => "radar",
            Self::Map => "map",
            Self::Arrow => "arrow",
            Self::Shape => "shape",
            Self::Nametags => "nametags",
            Self::Nametag => "nametag",
        }
    }

    /// A kind a plugin adds by name (a nametag is only ever resolved from the layer).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::World,
            Self::Radar,
            Self::Map,
            Self::Arrow,
            Self::Shape,
            Self::Nametags,
        ]
        .into_iter()
        .find(|k| k.name() == s)
    }

    /// The host capability that draws it.
    #[must_use]
    pub const fn capability(self) -> HostCapability {
        match self {
            Self::World | Self::Shape | Self::Nametags | Self::Nametag => {
                HostCapability::OverlayWorld
            }
            Self::Radar => HostCapability::OverlayRadar,
            Self::Map => HostCapability::OverlayMap,
            Self::Arrow => HostCapability::OverlayArrow,
        }
    }

    /// The colour a marker of this kind has when the plugin names none (`0xRRGGBB`).
    const fn default_color(self) -> u32 {
        match self {
            Self::World | Self::Arrow | Self::Shape => 0xFF_D7_00,
            Self::Radar => 0x00_FF_FF,
            Self::Map => 0xFF_40_40,
            Self::Nametags | Self::Nametag => 0xFF_FF_FF,
        }
    }
}

/// What a marker is attached to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Object(ObjectId),
    Place(Position),
}

/// What a shape is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// From its anchor to `to`.
    Line,
    /// A circle of `radius` metres about its anchor, flat at its height.
    Ring,
    /// A square `radius` metres from its anchor to each side, flat at its height.
    Square,
    /// Its label, floating at its height.
    Text,
}

impl ShapeKind {
    /// Every shape, as a plugin names it.
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::Line, "line"),
        (Self::Ring, "ring"),
        (Self::Square, "square"),
        (Self::Text, "text"),
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(s, _)| *s == self)
            .map_or("", |(_, n)| n)
    }
}

/// What the nametags layer labels: the kinds of object, how far, how many at most (the nearest),
/// and whether a label shows health.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Nametags {
    pub players: bool,
    pub monsters: bool,
    pub npcs: bool,
    pub portals: bool,
    pub within: f64,
    pub limit: usize,
    pub health: bool,
}

/// How far a nametags layer reaches when a plugin does not say, and the farthest it may.
pub const NAMETAGS_WITHIN: f64 = 35.0;
pub const NAMETAGS_WITHIN_MAX: f64 = 250.0;
/// How many tags a nametags layer shows when a plugin does not say, and the most it may.
pub const NAMETAGS_LIMIT: usize = 20;
pub const NAMETAGS_LIMIT_MAX: usize = 100;
/// Over how many metres at the far end of its range a nametag fades out (half the range, for a
/// range shorter than twice this).
pub const NAMETAGS_FADE: f64 = 10.0;

/// How strongly a nametag `distance` metres away is drawn in a layer reaching `within`: whole
/// until the last [`NAMETAGS_FADE`] metres of the range, then fading evenly to nothing at its end.
#[must_use]
pub fn nametag_strength(distance: f64, within: f64) -> f64 {
    let fade = NAMETAGS_FADE.min(within / 2.0);
    if fade <= 0.0 {
        return 1.0;
    }
    ((within - distance) / fade).clamp(0.0, 1.0)
}

/// One marker, as a plugin declared it.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub kind: Kind,
    /// What it marks (a world marker, a blip, the arrow); `None` for a map marker of points.
    pub anchor: Option<Anchor>,
    /// A map marker's points, in order.
    pub points: Vec<Position>,
    /// Whether a map marker's points are joined into a line.
    pub line: bool,
    pub label: String,
    /// `0xRRGGBB`.
    pub color: u32,
    /// An icon from the game's data, by id.
    pub icon: Option<u32>,
    /// When it goes, on the frame clock.
    pub until: Option<f64>,
    /// A shape's kind.
    pub shape: Option<ShapeKind>,
    /// A line's other end.
    pub to: Option<Anchor>,
    /// A ring's radius, or a square's half side, in metres.
    pub radius: f64,
    /// How high above its anchor a shape is drawn, in metres.
    pub height: f64,
    /// A shape's line width in pixels, 1 to 6.
    pub width: i32,
    /// The nametags layer's settings.
    pub nametags: Option<Nametags>,
    /// Drawn over the world even where something stands in front of it.
    pub on_top: bool,
}

fn bad(msg: impl Into<String>) -> CallError {
    CallError::BadArgs(msg.into())
}

/// A colour a plugin named: `"#RRGGBB"` or a number `0xRRGGBB`.
fn color(v: &Value, kind: Kind) -> Result<u32, CallError> {
    match v {
        Value::Nil => Ok(kind.default_color()),
        Value::Str(s) => s
            .strip_prefix('#')
            .filter(|h| h.len() == 6)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| bad(format!("{s:?} is not a colour (\"#RRGGBB\")"))),
        other => other
            .as_i64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= 0xFF_FF_FF)
            .ok_or_else(|| bad("a colour is \"#RRGGBB\" or a number 0xRRGGBB")),
    }
}

impl Marker {
    /// Read a marker of `kind` from a plugin's table.
    ///
    /// # Errors
    /// A field of the wrong kind, a marker with nothing to mark, or too many points.
    pub fn from_value(kind: Kind, v: &Value, now: f64) -> Result<Self, CallError> {
        for k in v.as_map().into_iter().flatten().map(|(k, _)| k) {
            let known: &[&str] = match kind {
                Kind::Map => &["points", "line", "label", "color", "expires"],
                Kind::Shape => &[
                    "shape",
                    "object",
                    "position",
                    "to_object",
                    "to_position",
                    "radius",
                    "height",
                    "width",
                    "label",
                    "color",
                    "expires",
                    "on_top",
                ],
                Kind::Nametags => &[
                    "players", "monsters", "npcs", "portals", "within", "limit", "health", "color",
                    "on_top",
                ],
                Kind::World => &[
                    "object", "position", "label", "color", "icon", "expires", "on_top",
                ],
                _ => &["object", "position", "label", "color", "icon", "expires"],
            };
            if !known.contains(&k.as_str()) {
                return Err(bad(format!("a {} marker has no field {k:?}", kind.name())));
            }
        }
        let anchor = match (v.get("object"), v.get("position")) {
            (Value::Nil, Value::Nil) => None,
            (o, Value::Nil) => Some(Anchor::Object(
                o.as_i64()
                    .and_then(|i| u32::try_from(i).ok())
                    .map(ObjectId)
                    .ok_or_else(|| bad("a marker's object must be an object id"))?,
            )),
            (Value::Nil, p) => Some(Anchor::Place(places::from_value(p)?)),
            _ => return Err(bad("a marker marks an object or a position, not both")),
        };
        let points = match v.get("points") {
            Value::Nil => Vec::new(),
            p => {
                let list = p
                    .as_list()
                    .ok_or_else(|| bad("a map marker's points must be a list of places"))?;
                if list.len() > MAX_POINTS {
                    return Err(bad(format!(
                        "a map marker holds at most {MAX_POINTS} points"
                    )));
                }
                list.iter()
                    .map(places::from_value)
                    .collect::<Result<_, _>>()?
            }
        };
        let to = match (v.get("to_object"), v.get("to_position")) {
            (Value::Nil, Value::Nil) => None,
            (o, Value::Nil) => Some(Anchor::Object(
                o.as_i64()
                    .and_then(|i| u32::try_from(i).ok())
                    .map(ObjectId)
                    .ok_or_else(|| bad("a line's to_object must be an object id"))?,
            )),
            (Value::Nil, p) => Some(Anchor::Place(places::from_value(p)?)),
            _ => return Err(bad("a line ends at an object or a position, not both")),
        };
        let shape = match (kind, v.get("shape").as_str()) {
            (Kind::Shape, Some(s)) => Some(
                ShapeKind::ALL
                    .iter()
                    .find(|(_, n)| *n == s)
                    .map(|(k, _)| *k)
                    .ok_or_else(|| bad("a shape is line, ring, square or text"))?,
            ),
            (Kind::Shape, None) => return Err(bad("a shape names its shape")),
            _ => None,
        };
        let metres = |k: &str, d: f64| match v.get(k) {
            Value::Nil => Ok(d),
            x => x
                .as_f64()
                .filter(|m| m.is_finite() && (0.0..=500.0).contains(m))
                .ok_or_else(|| bad(format!("{k} is a number of metres from 0 to 500"))),
        };
        let radius = metres("radius", 1.0)?;
        let height = metres(
            "height",
            if shape == Some(ShapeKind::Text) {
                2.0
            } else {
                0.1
            },
        )?;
        if shape == Some(ShapeKind::Line) && to.is_none() {
            return Err(bad("a line needs to_object or to_position"));
        }
        let nametags = (kind == Kind::Nametags).then(|| Nametags {
            players: v.get("players").as_bool().unwrap_or(false),
            monsters: v.get("monsters").as_bool().unwrap_or(false),
            npcs: v.get("npcs").as_bool().unwrap_or(false),
            portals: v.get("portals").as_bool().unwrap_or(false),
            within: v
                .get("within")
                .as_f64()
                .unwrap_or(NAMETAGS_WITHIN)
                .clamp(1.0, NAMETAGS_WITHIN_MAX),
            limit: v
                .get("limit")
                .as_i64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(NAMETAGS_LIMIT)
                .clamp(1, NAMETAGS_LIMIT_MAX),
            health: v.get("health").as_bool().unwrap_or(false),
        });
        match kind {
            Kind::Map if points.is_empty() => return Err(bad("a map marker needs points")),
            Kind::World | Kind::Radar | Kind::Arrow | Kind::Shape if anchor.is_none() => {
                return Err(bad(format!(
                    "a {} marker needs an object or a position",
                    kind.name()
                )))
            }
            _ => {}
        }
        let until = match v.get("expires") {
            Value::Nil => None,
            u => Some(
                now + u
                    .as_f64()
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .ok_or_else(|| bad("expires is a number of seconds"))?,
            ),
        };
        Ok(Self {
            kind,
            anchor,
            points,
            line: v.get("line").as_bool().unwrap_or(false),
            label: v.get("label").as_str().unwrap_or_default().to_owned(),
            color: color(v.get("color"), kind)?,
            icon: v.get("icon").as_i64().and_then(|i| u32::try_from(i).ok()),
            until,
            shape,
            to,
            radius,
            height,
            width: v
                .get("width")
                .as_i64()
                .and_then(|w| i32::try_from(w).ok())
                .unwrap_or(2)
                .clamp(1, 6),
            nametags,
            on_top: match v.get("on_top") {
                Value::Nil => false,
                b => b.as_bool().ok_or_else(|| bad("on_top is true or false"))?,
            },
        })
    }
}

/// The points around a ring, and the corners of a square, in order and closed.
const RING_POINTS: usize = 32;

/// One marker, resolved for drawing this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayView {
    pub plugin: String,
    pub id: i64,
    pub kind: Kind,
    pub label: String,
    pub color: u32,
    pub icon: Option<u32>,
    /// The object it marks, if it marks one.
    pub object: Option<ObjectId>,
    /// Where it is this frame (`None` when its object is out of sight).
    pub at: Option<Position>,
    pub points: Vec<Position>,
    pub line: bool,
    /// A shape's kind.
    pub shape: Option<ShapeKind>,
    /// A shape's outline in the world this frame, in order (a line's two ends, a ring's points
    /// closed); empty for text and when what it is anchored to is out of sight.
    pub path: Vec<Position>,
    /// A shape's line width in pixels.
    pub width: i32,
    /// Drawn over the world even where something stands in front of it.
    pub on_top: bool,
    /// A nametag's layer's range, in metres, over the end of which it fades.
    pub reach: Option<f64>,
}

/// `p` moved by `dx`, `dy`, `dz` metres in its own cell's frame.
fn moved(p: &Position, dx: f64, dy: f64, dz: f64) -> Position {
    let mut q = *p;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: metres to a frame's f32 coordinates.
    {
        q.frame.origin.x += dx as f32;
        q.frame.origin.y += dy as f32;
        q.frame.origin.z += dz as f32;
    }
    q
}

/// A shape's outline: a line's two ends, a ring's points or a square's corners, closed.
fn outline(m: &Marker, at: &Position, to: Option<&Position>) -> Vec<Position> {
    let base = moved(at, 0.0, 0.0, m.height);
    match m.shape {
        Some(ShapeKind::Line) => to
            .map(|t| vec![base, moved(t, 0.0, 0.0, m.height)])
            .unwrap_or_default(),
        Some(ShapeKind::Ring) => (0..=RING_POINTS)
            .map(|i| {
                let a = f64::from(u32::try_from(i).unwrap_or(0)) / 32.0 * std::f64::consts::TAU;
                moved(
                    &base,
                    m.radius * dereth_primitives::num::math::cos(a),
                    m.radius * dereth_primitives::num::math::sin(a),
                    0.0,
                )
            })
            .collect(),
        Some(ShapeKind::Square) => {
            let r = m.radius;
            [(-r, -r), (r, -r), (r, r), (-r, r), (-r, -r)]
                .iter()
                .map(|(dx, dy)| moved(&base, *dx, *dy, 0.0))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Every plugin's markers as they stand this frame, with the player's position (an arrow points
/// from it; a renderer turns that into a direction on screen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlays {
    pub player: Option<Position>,
    pub markers: Vec<OverlayView>,
}

impl Overlays {
    /// Resolve every plugin's markers against this frame's places.
    #[must_use]
    pub fn resolve<'a>(
        places: &dyn Places,
        player_id: Option<ObjectId>,
        markers: impl Iterator<Item = (&'a str, &'a BTreeMap<i64, Marker>)>,
    ) -> Self {
        let mut out = Self {
            player: places.player(),
            markers: Vec::new(),
        };
        for (plugin, ms) in markers {
            for (&id, m) in ms {
                if m.kind == Kind::Nametags {
                    continue;
                }
                let place = |a: Option<Anchor>| match a {
                    Some(Anchor::Object(o)) if Some(o) == player_id => places.player(),
                    Some(Anchor::Object(o)) => places.object(o),
                    Some(Anchor::Place(p)) => Some(p),
                    None => None,
                };
                let object = match m.anchor {
                    Some(Anchor::Object(o)) => Some(o),
                    _ => None,
                };
                let at = place(m.anchor);
                let path = match (m.kind, &at) {
                    (Kind::Shape, Some(a)) => outline(m, a, place(m.to).as_ref()),
                    _ => Vec::new(),
                };
                let at = if m.shape == Some(ShapeKind::Text) {
                    at.map(|a| moved(&a, 0.0, 0.0, m.height))
                } else {
                    at
                };
                out.markers.push(OverlayView {
                    plugin: plugin.to_owned(),
                    id,
                    kind: m.kind,
                    label: m.label.clone(),
                    color: m.color,
                    icon: m.icon,
                    object,
                    at,
                    points: m.points.clone(),
                    line: m.line,
                    shape: m.shape,
                    path,
                    width: m.width,
                    on_top: m.on_top,
                    reach: None,
                });
            }
        }
        out
    }

    /// The arrow's direction relative to the player's heading, in degrees (0 straight ahead,
    /// clockwise), and its distance in metres.
    #[must_use]
    pub fn arrow_of(&self, view: &OverlayView) -> Option<(f64, f64)> {
        let (me, at) = (self.player.as_ref()?, view.at.as_ref()?);
        let rel = places::turn(places::heading(me), places::bearing(me, at));
        Some((rel, places::distance(me, at)))
    }

    /// The markers as text, one a line: what a host with no display shows, and what a test
    /// compares.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for v in &self.markers {
            let _ = write!(out, "{} {} #{}", v.kind.name(), v.plugin, v.id);
            if !v.label.is_empty() {
                let _ = write!(out, " label={:?}", v.label);
            }
            let _ = write!(out, " color=#{:06X}", v.color);
            if let Some(o) = v.object {
                let _ = write!(out, " object=0x{:08X}", o.0);
            }
            let at = |p: &Position| match places::coords(p) {
                Some((ns, ew)) => format!("{ns:.2}N {ew:.2}E"),
                None => format!(
                    "0x{:08X} {:.1} {:.1} {:.1}",
                    p.cell.0, p.frame.origin.x, p.frame.origin.y, p.frame.origin.z
                ),
            };
            match &v.at {
                Some(p) => {
                    let _ = write!(out, " at={}", at(p));
                }
                None if v.kind != Kind::Map => out.push_str(" at=unseen"),
                None => {}
            }
            if v.kind == Kind::Arrow {
                if let Some((rel, d)) = self.arrow_of(v) {
                    let _ = write!(out, " turn={rel:.0} distance={d:.1}");
                }
            }
            if let Some(s) = v.shape {
                let _ = write!(out, " shape={} path={}", s.name(), v.path.len());
                if let (Some(a), Some(b)) = (v.path.first(), v.path.get(1)) {
                    let _ = write!(out, " span={:.1}", places::distance(a, b));
                }
            }
            if v.on_top {
                out.push_str(" on_top");
            }
            if !v.points.is_empty() {
                let pts: Vec<String> = v.points.iter().map(at).collect();
                let _ = write!(
                    out,
                    " {}=[{}]",
                    if v.line { "line" } else { "points" },
                    pts.join(", ")
                );
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, Marker};
    use dereth_plugin_api::Value;

    #[test]
    fn a_marker_needs_something_to_mark_and_takes_only_its_own_fields() {
        let place = Value::map().with("ns", 42.2).with("ew", 33.8);
        let m = Marker::from_value(
            Kind::World,
            &Value::map()
                .with("position", place.clone())
                .with("label", "Here")
                .with("color", "#00FF00")
                .with("expires", 5.0),
            10.0,
        )
        .expect("valid");
        assert_eq!((m.color, m.until), (0x00FF00, Some(15.0)));
        assert!(Marker::from_value(Kind::World, &Value::map(), 0.0).is_err());
        assert!(Marker::from_value(Kind::Map, &Value::map().with("object", 5_i64), 0.0).is_err());
        assert!(Marker::from_value(
            Kind::Radar,
            &Value::map().with("object", 5_i64).with("position", place),
            0.0
        )
        .is_err());
        assert!(
            Marker::from_value(Kind::Arrow, &Value::map().with("colour", "#FFFFFF"), 0.0).is_err()
        );
    }

    #[test]
    fn a_nametag_fades_over_the_last_ten_metres_of_its_range() {
        let s = super::nametag_strength;
        assert!(
            (s(10.0, 35.0) - 1.0).abs() < 1e-9,
            "whole well inside the range"
        );
        assert!(
            (s(25.0, 35.0) - 1.0).abs() < 1e-9,
            "whole until the last ten metres"
        );
        assert!(
            (s(30.0, 35.0) - 0.5).abs() < 1e-9,
            "half way through the fade"
        );
        assert!(s(35.0, 35.0).abs() < 1e-9, "gone at the end of the range");
        assert!(
            (s(9.0, 12.0) - 0.5).abs() < 1e-9,
            "a short range fades over its last half"
        );
    }
}
