//! The look: the remaster's one knob on retail's light.
//!
//! Retail's light is a set of inputs: the region's sun, ambient and fog at the hour, which the
//! scene hands over as a [`RetailLight`] every frame, exactly as Retail Look reads them. A [`Look`]
//! is a set of adjustments laid over those inputs before the remaster uses them, and nothing else
//! reads it: Retail Look never sees a look, and the remaster with the identity look draws what it
//! drew without one.
//!
//! **What a look can change**, each a number or a curve over the day ([`Curve`]):
//! - `sun`: the sun's strength, a factor on its brightness;
//! - `sky`: the sky's strength, a factor on both ambients;
//! - `tint`: the evening and night tint, 1 keeping retail's purple and 0 taking the ambient and
//!   fog colors to the gray of the same brightness;
//! - `fog_density`: how thick the fog is, a factor that brings its start and end nearer;
//! - `fog_color`: a color the fog takes in place of retail's;
//! - `night_floor`: the least the ambient's level may be;
//! - `exposure`: stops of exposure, read only by the high dynamic range pass;
//! - `foliage_hue` and `foliage_pull`: the hue an art pack's leaves are pulled toward, in degrees,
//!   and how far, 0 not at all and 1 all the way. Retail's own surfaces are never pulled. A look
//!   that names neither leaves the remaster's own pull ([`FOLIAGE`]), which brings a pack's trees
//!   into one family of greens.
//!
//! Each can be given again for one kind of day ([`DayKind`]: Sunny, Rainy, Clear or Cloudy), which
//! then wins over the look's own value on those days.
//!
//! **Where looks come from** ([`Layer`]), the later winning value by value: the remaster's built-in
//! look (the identity), an art pack's, a world pack's, and the player's `look.toml`. A layer that
//! says nothing about a value leaves it to the layers below. [`LookStack`] holds them and
//! [`LookStack::look`] gives the look of one moment.

mod file;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::scene::RetailLight;

pub use file::LookError;

/// What a player's look file is called, in the client's settings folder.
pub const PLAYER_FILE: &str = "look.toml";

/// A kind of day, by the name its day group starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DayKind {
    Sunny,
    Rainy,
    Clear,
    Cloudy,
}

impl DayKind {
    /// Every kind, in order.
    pub const ALL: [Self; 4] = [Self::Sunny, Self::Rainy, Self::Clear, Self::Cloudy];

    /// Its name, as a day group's name starts and a look file's table is called (lowercase).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sunny => "sunny",
            Self::Rainy => "rainy",
            Self::Clear => "clear",
            Self::Cloudy => "cloudy",
        }
    }

    /// The kind a day group's name starts with, whatever its case; `None` for any other name.
    #[must_use]
    pub fn of_group(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|k| name.starts_with(k.name()))
    }
}

/// A value over the day: one number, or points `(time of day, value)` joined by straight lines
/// and round midnight from the last to the first. The time of day runs from 0 to 1.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    Constant(f32),
    Points(Vec<(f32, f32)>),
}

impl Curve {
    /// The value at time of day `t`.
    #[must_use]
    pub fn at(&self, t: f32) -> f32 {
        match self {
            Self::Constant(v) => *v,
            Self::Points(p) => match p.as_slice() {
                [] => 1.0,
                [(_, v)] => *v,
                _ => {
                    let t = t.rem_euclid(1.0);
                    let n = p.len();
                    match p.iter().position(|&(pt, _)| pt > t) {
                        Some(i) if i > 0 => lerp(p[i - 1], p[i], t),
                        // Round midnight: from the last point to the first, a day later.
                        _ => {
                            let (last, first) = (p[n - 1], (p[0].0 + 1.0, p[0].1));
                            lerp(last, first, if t < last.0 { t + 1.0 } else { t })
                        }
                    }
                }
            },
        }
    }
}

/// The value at `t` on the line from `a` to `b`, each `(time, value)`.
fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> f32 {
    let span = b.0 - a.0;
    if span <= 0.0 {
        return a.1;
    }
    a.1 + (b.1 - a.1) * ((t - a.0) / span).clamp(0.0, 1.0)
}

/// What one layer says, each value `None` where it leaves the value to the layers below.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Values {
    pub sun: Option<Curve>,
    pub sky: Option<Curve>,
    pub tint: Option<Curve>,
    pub fog_density: Option<Curve>,
    pub fog_color: Option<[u8; 3]>,
    pub night_floor: Option<Curve>,
    pub exposure: Option<Curve>,
    pub foliage_hue: Option<Curve>,
    pub foliage_pull: Option<Curve>,
}

impl Values {
    /// `self`, with each value `over` gives in its place.
    fn under(&mut self, over: &Self) {
        let take = |a: &mut Option<Curve>, b: &Option<Curve>| {
            if b.is_some() {
                a.clone_from(b);
            }
        };
        take(&mut self.sun, &over.sun);
        take(&mut self.sky, &over.sky);
        take(&mut self.tint, &over.tint);
        take(&mut self.fog_density, &over.fog_density);
        if over.fog_color.is_some() {
            self.fog_color = over.fog_color;
        }
        take(&mut self.night_floor, &over.night_floor);
        take(&mut self.exposure, &over.exposure);
        take(&mut self.foliage_hue, &over.foliage_hue);
        take(&mut self.foliage_pull, &over.foliage_pull);
    }
}

/// One layer's look: its values, and the values it gives for each kind of day.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LookToml {
    pub values: Values,
    pub days: BTreeMap<DayKind, Values>,
}

impl LookToml {
    /// A look file's text, read. See [`file`](self) for its form.
    ///
    /// # Errors
    /// [`LookError`] when it is not TOML, or names a value or a day it does not know, or gives a
    /// value outside what it can be.
    pub fn parse(text: &str) -> Result<Self, LookError> {
        file::parse(text)
    }
}

/// Where a look comes from, in the order they are laid on each other: a later layer's value wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// The remaster's own look: the identity.
    BuiltIn,
    /// The art pack drawn: a pack's pictures are tuned under a light, and its author can say so.
    ArtPack,
    /// The world's content pack: an older region has its own light.
    WorldPack,
    /// The player's `look.toml`.
    Player,
}

/// The looks of every layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LookStack {
    layers: BTreeMap<Layer, LookToml>,
}

impl LookStack {
    /// No layer but the built-in identity.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set one layer's look, or take it away. Returns whether anything changed.
    pub fn set(&mut self, layer: Layer, look: Option<LookToml>) -> bool {
        let before = self.layers.get(&layer).cloned();
        match look {
            Some(l) => {
                self.layers.insert(layer, l);
            }
            None => {
                self.layers.remove(&layer);
            }
        }
        before != self.layers.get(&layer).cloned()
    }

    /// One layer's look, when it has one.
    #[must_use]
    pub fn layer(&self, layer: Layer) -> Option<&LookToml> {
        self.layers.get(&layer)
    }

    /// Whether every layer leaves every value alone, so every moment's look is the identity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.values().all(|f| {
            f.values == Values::default() && f.days.values().all(|v| *v == Values::default())
        })
    }

    /// The look at time of day `t` on a day of kind `day`: the layers laid in order, each layer's
    /// own values and then its values for that kind of day.
    #[must_use]
    pub fn look(&self, day: Option<DayKind>, t: f32) -> Look {
        let mut v = Values::default();
        for f in self.layers.values() {
            v.under(&f.values);
            if let Some(d) = day.and_then(|d| f.days.get(&d)) {
                v.under(d);
            }
        }
        let at = |c: &Option<Curve>, default: f32| c.as_ref().map_or(default, |c| c.at(t));
        Look {
            sun: at(&v.sun, 1.0),
            sky: at(&v.sky, 1.0),
            tint: at(&v.tint, 1.0).clamp(0.0, 1.0),
            fog_density: at(&v.fog_density, 1.0),
            fog_color: v.fog_color,
            night_floor: at(&v.night_floor, 0.0),
            exposure: at(&v.exposure, 0.0),
            foliage_hue: v.foliage_hue.as_ref().map(|c| c.at(t)),
            foliage_pull: v.foliage_pull.as_ref().map(|c| c.at(t)),
        }
    }
}

/// The remaster's own pull on an art pack's leaves, where no look names one: toward this hue, in
/// degrees, this far. A pack's trees each take their retail model's own hue, from red-brown to
/// blue-green; pulled most of the way, they read as one family of greens with a little of each
/// left.
pub const FOLIAGE: (f32, f32) = (100.0, 0.8);

/// The look of one moment: plain numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub sun: f32,
    pub sky: f32,
    pub tint: f32,
    pub fog_density: f32,
    pub fog_color: Option<[u8; 3]>,
    pub night_floor: f32,
    pub exposure: f32,
    /// The hue an art pack's leaves are pulled toward, degrees, and how far, where a look names
    /// them; `None` leaves the remaster's own ([`FOLIAGE`]).
    pub foliage_hue: Option<f32>,
    pub foliage_pull: Option<f32>,
}

impl Default for Look {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Rec. 601's weights: a color's brightness, as the gray it is taken to keeps it.
const LUMA: [f32; 3] = [0.299, 0.587, 0.114];

/// `c` taken toward the gray of its own brightness: all the way at `t` 0, not at all at 1.
fn tint_f(c: [f32; 3], t: f32) -> [f32; 3] {
    let y = c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2];
    c.map(|v| y + (v - y) * t)
}

/// [`tint_f`] on a byte color, rounded to the nearest byte.
fn tint_bytes(c: [u8; 3], t: f32) -> [u8; 3] {
    tint_f(c.map(f32::from), t).map(|v| {
        // A float in 0..=255 after the clamp, rounded: the cast cannot wrap.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = v.round().clamp(0.0, 255.0) as u8;
        b
    })
}

impl Look {
    /// The look that changes nothing.
    pub const IDENTITY: Self = Self {
        sun: 1.0,
        sky: 1.0,
        tint: 1.0,
        fog_density: 1.0,
        fog_color: None,
        night_floor: 0.0,
        exposure: 0.0,
        foliage_hue: None,
        foliage_pull: None,
    };

    /// The hue an art pack's leaves are pulled toward, in turns (0 to 1), and how far (0 to 1):
    /// as the look names them, else the remaster's own ([`FOLIAGE`]).
    #[must_use]
    pub fn foliage(&self) -> [f32; 2] {
        let hue = self.foliage_hue.unwrap_or(FOLIAGE.0);
        let pull = self.foliage_pull.unwrap_or(FOLIAGE.1);
        [(hue / 360.0).rem_euclid(1.0), pull.clamp(0.0, 1.0)]
    }

    /// Whether it changes nothing retail's light inputs are made of. The exposure is not one of
    /// them: only the high dynamic range pass reads it.
    #[must_use]
    pub fn leaves_light_alone(&self) -> bool {
        Self {
            exposure: 0.0,
            foliage_hue: None,
            foliage_pull: None,
            ..*self
        } == Self::IDENTITY
    }

    /// Retail's light inputs with this look laid over them. With a look that leaves the light
    /// alone they come back as they are, bit for bit.
    #[must_use]
    pub fn apply(&self, light: &RetailLight) -> RetailLight {
        let mut l = light.clone();
        if self.leaves_light_alone() {
            return l;
        }
        // The sun's direction is scaled by its brightness, so its length is its strength.
        l.sun = light.sun * self.sun.max(0.0);
        let sky = self.sky.max(0.0);
        let t = self.tint.clamp(0.0, 1.0);
        let floor = self.night_floor.max(0.0);
        if t < 1.0 {
            l.ambient_color = tint_bytes(light.ambient_color, t);
        }
        l.ambient_level = (light.ambient_level * sky).max(floor);
        let objects = tint_f(light.object_ambient, t);
        let a = l.ambient_color.map(|b| f32::from(b) / 255.0);
        for (k, o) in l.object_ambient.iter_mut().enumerate() {
            *o = (objects[k] * sky).max(floor * a[k]);
        }
        if let Some(f) = l.fog.as_mut() {
            let d = self.fog_density.max(1e-3);
            f.start /= d;
            f.end /= d;
            if let Some(c) = self.fog_color {
                f.color = c;
            }
            if t < 1.0 {
                f.color = tint_bytes(f.color, t);
            }
        }
        l
    }
}
