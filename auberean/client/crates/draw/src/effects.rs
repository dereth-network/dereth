//! Effects an object plays: an effect id looked up in the object's effect table, and the particle
//! systems of what it finds set going at the object.
//!
//! An effect table lists, for each effect id, any number of effects, each with variants by the
//! ground's terrain kind; a table may extend others, whose effects for the same id play too. An
//! effect can name sounds, particle systems, a camera effect, a light and more; its particle
//! systems are drawn here, and its sounds handed on to be played ([`FxSound`]).
//!
//! An object whose look carries effects (an effect table, or effect lists of its own) has an
//! [`ObjectEffects`]: its own table (its look's lists first, then the stored table the look names)
//! and the effects it plays from the moment it is made, its startup effects. A gateway's swirl, a
//! brazier's flames and a crystal's glow are startup effects. Each particle system they set going
//! stands at the object, raised by its effect's offset, and moves with it; the systems are kept
//! and drawn by [`ParticleSystems`], where the objects placed with the world share theirs.
//!
//! [`Effect`] plays one effect at a fixed place, for the viewer.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_assets::entity::Entity;
use auberean_assets::fx::{AppearanceKey, FxData, FxNode, Particles};
use auberean_primitives::random::Random;
use auberean_primitives::{DataId, Vec3};
use auberean_world::color::Color;
use auberean_world::inside::{BlockLights, Outside};
use auberean_world::look::{Atoms, Group, Keyed, Look, LookRecords};
use auberean_world::math::{add, length, sub, Mat4};
use auberean_world::place::{PlaceRecords, DRAWN_KINDS};
use auberean_world::records::WorldRecords;

use crate::particles::{approximate_lifetime, ParticleSet};
use crate::scene::ObjectBatch;
use crate::systems::{ParticleSystems, Repulsor, RepulsorRef, SystemRef};

/// Effect tables extending each other more deeply than this are taken to loop.
const MAX_DEPTH: usize = 16;

/// The bucket count of an object's own effect table.
const OWN_TABLE_BUCKETS: u32 = 32;

/// A slider an effect moves: to `value` over `time` seconds, on the object's children too when
/// `children`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxSlider {
    pub slider: u32,
    pub value: f32,
    pub time: f32,
    pub children: bool,
}

/// The sliders holding an object's color: red, green, blue, alpha.
const COLOR_SLIDERS: [u32; 4] = [0x4000_0016, 0x4000_0017, 0x4000_0018, 0x4000_001A];

/// The sliders `node` moves over its ramp time: its own to its intensity, and with a color each of
/// the object's color sliders to that color's part.
// D2-06881, D2-06880
#[must_use]
pub fn node_sliders(node: &FxNode) -> Vec<FxSlider> {
    let time = node.ramp_time.unwrap_or(0.0);
    let children = node.property_color == Some(true);
    let mut out = Vec::new();
    if let Some(slider) = node.slider.filter(|s| *s != 0) {
        out.push(FxSlider {
            slider,
            value: node.intensity.unwrap_or(1.0),
            time,
            children,
        });
    }
    if let Some((true, c)) = node.color {
        for (slider, value) in COLOR_SLIDERS.into_iter().zip([c.r, c.g, c.b, c.a]) {
            out.push(FxSlider {
                slider,
                value,
                time,
                children,
            });
        }
    }
    out
}

/// Every effect stored table `table` (and, after it, each table it extends, in order and depth
/// first) lists for effect id `effect`.
// D2-15746
#[must_use]
pub fn stored_fx_data(records: &WorldRecords, table: u32, effect: u32) -> Vec<FxData> {
    let mut out = Vec::new();
    collect(records, table, effect, 0, &mut out);
    out
}

fn collect(records: &WorldRecords, table: u32, effect: u32, depth: usize, out: &mut Vec<FxData>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(t) = records.fx_table(table) else {
        return;
    };
    if let Some((_, list)) = t.effects.iter().find(|(id, _)| *id == effect) {
        out.extend(list.iter().cloned());
    }
    for sub in &t.sub_tables {
        collect(records, sub.0, effect, depth + 1, out);
    }
}

/// The effect an entry plays on terrain kind `terrain`: its variant for that kind, else its
/// default.
// D2-15727
#[must_use]
pub fn node_for(data: &FxData, terrain: u32) -> &FxNode {
    data.by_terrain
        .iter()
        .find(|(kind, _)| *kind == terrain)
        .map_or(&data.default, |(_, n)| n)
}

/// Every effect `table` (and, after it, each table it extends, in order and depth first) lists
/// for effect id `effect`, each as its variant for terrain kind `terrain` (else its default).
#[must_use]
pub fn effect_nodes(records: &WorldRecords, table: u32, effect: u32, terrain: u32) -> Vec<FxNode> {
    stored_fx_data(records, table, effect)
        .iter()
        .map(|d| node_for(d, terrain).clone())
        .collect()
}

/// An object's effect table: the lists its look gives by effect id, and the stored table it
/// names. An id the object lists itself is never looked up in the stored table, even when its
/// list is empty.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectFxTable {
    own: Keyed<Vec<FxData>>,
    stored: DataId,
}

impl Default for ObjectFxTable {
    // D2-15740
    fn default() -> Self {
        Self {
            own: Keyed::stored(OWN_TABLE_BUCKETS, Vec::new()),
            stored: DataId::NONE,
        }
    }
}

impl ObjectFxTable {
    /// Forget the object's own lists.
    // D2-15736
    pub fn clear(&mut self) {
        self.own.entries.clear();
    }

    /// The stored table looked in for what the object does not list itself.
    // D2-15733
    pub fn set_stored(&mut self, table: DataId) {
        self.stored = table;
    }

    /// The stored table.
    #[must_use]
    pub fn stored(&self) -> DataId {
        self.stored
    }

    /// List `effects` for effect id `effect`, unless the object lists it already.
    // D2-15741
    pub fn add_entry(&mut self, effect: u32, effects: Vec<FxData>) {
        self.own.add(effect, effects);
    }

    /// The table a look's whole-object group gives: the stored table it names and its own lists.
    // D2-14063, D2-13795
    #[must_use]
    pub fn of_group(group: &Group) -> Self {
        let mut t = Self::default();
        t.clear();
        t.set_stored(group.fx_table);
        for (effect, list) in &group.fx_overrides {
            t.add_entry(*effect, list.clone());
        }
        t
    }

    /// As [`ObjectFxTable::of_group`], then the effects its appearance lists: each effect atom's
    /// effects for every id the atom lists (an id the group lists itself keeps its own). A
    /// brazier's blue flames are its appearance's in place of its table's orange ones.
    // D2-14063
    #[must_use]
    pub fn of_group_with(records: &dyn LookRecords, group: &Group) -> Self {
        let mut t = Self::of_group(group);
        let mut atoms = Atoms::default();
        group.augment_atoms(records, &mut atoms, false);
        for a in &atoms.effects {
            for id in &a.ids {
                t.add_entry(*id, a.effects.clone());
            }
        }
        t
    }

    /// Every effect listed for effect id `effect`.
    // D2-15737
    #[must_use]
    pub fn fx_data(&self, records: &WorldRecords, effect: u32) -> Vec<FxData> {
        match self.own.get(effect) {
            Some(list) => list.clone(),
            None if self.stored.is_none() => Vec::new(),
            None => stored_fx_data(records, self.stored.0, effect),
        }
    }
}

/// A decal an effect lays: what it is made from, and how far from its object's place it lies,
/// along the world's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxDecal {
    pub desc: crate::decal::DecalDesc,
    pub offset: Vec3,
}

/// The channels an effect's entry may be keyed to, and what its key there says.
pub mod channel {
    /// How much gore: fun, none, low or normal, 1 to 4.
    pub const GORE: u32 = 0x4000_0001;
    /// The color of blood: green or red, 1 or 2.
    pub const BLOOD: u32 = 0x4000_0007;
    /// Whether steps raise particles: 1 or 0.
    pub const FOOT_STEP_PARTICLES: u32 = 0x4000_0008;
}

/// What the player's preferences ask of the entries of an effect keyed to a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channels {
    pub gore: u32,
    pub blood: u32,
    pub foot_step_particles: bool,
}

impl Default for Channels {
    /// Normal gore, red blood, steps raising particles.
    fn default() -> Self {
        Self {
            gore: 4,
            blood: 2,
            foot_step_particles: true,
        }
    }
}

impl Channels {
    /// Whether an entry is keyed otherwise than the preferences ask: such an entry does not play.
    /// A channel no preference holds is not looked at.
    // D2-06908
    #[must_use]
    pub fn mismatch(&self, node: &FxNode) -> bool {
        let asked = [
            (channel::GORE, self.gore),
            (channel::BLOOD, self.blood),
            (
                channel::FOOT_STEP_PARTICLES,
                u32::from(self.foot_step_particles),
            ),
        ];
        node.channel_keys.as_ref().is_some_and(|keys| {
            asked
                .iter()
                .any(|(channel, value)| keys.iter().any(|(c, v)| c == channel && v != value))
        })
    }
}

/// One effect an object is playing: how strongly, whether it began with the object, the ids of
/// the particle systems it keeps going (those that run until stopped), and when it ends (the
/// client's time; none for never).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxState {
    pub desired: f32,
    pub current: f32,
    pub startup: bool,
    pub systems: Vec<u32>,
    pub end: Option<f64>,
    /// An entry's color lasts a time, so the object's color goes back when the effect ends.
    pub restore_color: bool,
    /// Sounds are kept going with it.
    pub keeps_sounds: bool,
}

/// Where an object stands as it plays an effect: in block `home`'s frame at `place` with `scale`,
/// at the client's time `now`.
#[derive(Clone, Copy, Debug)]
pub struct FxSpot {
    pub home: (i32, i32),
    pub place: Mat4,
    pub scale: Vec3,
    pub now: f64,
}

/// Where on an object a system set going on its own may stand, as the object stands now: a
/// holding location's own place, a bone of a kind (each in the object's own frame, none when the
/// object has none), and the height of the water in its cell (none without water).
pub trait Rig {
    fn holding_location(&self, kind: u32) -> Option<Mat4>;
    fn bone(&self, kind: u32) -> Option<Mat4>;
    fn water_height(&self) -> Option<f32>;
    /// The bone a system carried at a holding location of `kind` follows: the first part group's
    /// of the location's number, or its first when the object has no such location; none when
    /// that group has no such bone.
    fn holding_bone(&self, kind: u32) -> Option<Mat4>;
    /// Whether the object has a figure whose bones can be asked for. Without one, what an entry
    /// sets on a bone stands at the object.
    fn has_figure(&self) -> bool;
}

/// An object with no holding locations, no bones and no water.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRig;

impl Rig for NoRig {
    fn holding_location(&self, _kind: u32) -> Option<Mat4> {
        None
    }
    fn bone(&self, _kind: u32) -> Option<Mat4> {
        None
    }
    fn water_height(&self) -> Option<f32> {
        None
    }
    fn holding_bone(&self, _kind: u32) -> Option<Mat4> {
        None
    }
    fn has_figure(&self) -> bool {
        false
    }
}

/// An object with no animation of its own: its bones and holding locations as its look's body
/// first stands, for the systems its effects carry on them.
#[derive(Clone, Debug)]
pub struct RestRig {
    body: auberean_animation::body::Body,
}

impl RestRig {
    /// The rig of look `look`, when its body can be put together.
    #[must_use]
    pub fn of_look(records: &WorldRecords, look: &Look) -> Option<Self> {
        let anims = auberean_animation::records::CachedRecords::new(records);
        auberean_animation::body::Body::from_look(&anims, look).map(|body| Self { body })
    }
}

impl Rig for RestRig {
    fn holding_location(&self, kind: u32) -> Option<Mat4> {
        let anims = auberean_animation::records::CachedRecords::default();
        let (_, location, _) = self.body.holding_location(&anims, kind)?;
        let t = location.transform;
        Some(Mat4::from_parts(t.offset, t.rotation, t.scale))
    }
    fn bone(&self, kind: u32) -> Option<Mat4> {
        let anims = auberean_animation::records::CachedRecords::default();
        self.body.bone_type_matrix(&anims, kind)
    }
    fn water_height(&self) -> Option<f32> {
        None
    }
    fn holding_bone(&self, kind: u32) -> Option<Mat4> {
        let anims = auberean_animation::records::CachedRecords::default();
        self.body.carried_bone(&anims, kind)
    }
    fn has_figure(&self) -> bool {
        true
    }
}

/// What an object is as an effect asks: whether it is the player's own, and whether the game's
/// time of day is a night one.
#[derive(Clone, Copy, Debug, Default)]
pub struct FxWho {
    pub player: bool,
    pub night: bool,
}

/// An appearance an effect lays over its object, or takes off it: a value of an appearance table
/// for the whole object at a strength, or that value's key taken off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppearanceFx {
    Apply { table: u32, key: AppearanceKey },
    Remove { table: u32, key: u32 },
}

/// What part of an object's figure a carried system follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarriedOn {
    /// The object itself.
    Object,
    /// The first bone of a kind.
    Bone(u32),
    /// The bone a holding location of a kind is on.
    HoldingLocation(u32),
}

/// A particle system an object carries: where it stands on the object and the system.
#[derive(Clone, Debug)]
pub struct CarriedSystem {
    /// The id the object knows it by.
    pub id: u32,
    /// The system's record.
    pub desc: u32,
    /// Its place on what it stands on: the effect's offset, or the holding location's own.
    pub offset: Mat4,
    /// The part of the figure it follows as the figure moves.
    pub on: CarriedOn,
    /// Whether the object's turn is taken off it.
    pub remove_rotation: bool,
    pub system: SystemRef,
}

/// The effects an object plays and the particle systems they keep going.
#[derive(Clone, Debug)]
pub struct ObjectEffects {
    /// What its look gives its systems' scale.
    // D2-06005
    particle_scale: f32,
    table: ObjectFxTable,
    /// Whether its object is drawn this frame: only then are its systems queued, but for those
    /// their records force to draw.
    drawn: bool,
    /// The effects played from the moment the object is made, with their strengths, in the
    /// object's table order.
    startup: Vec<(u32, f32)>,
    states: BTreeMap<u32, FxState>,
    /// The terrain kind the object stands on: which variant of an effect it plays.
    pub terrain: u32,
    systems: Vec<CarriedSystem>,
    next_id: u32,
    /// Whether the object is one placed with the world, whose systems other objects may share.
    pub shared: bool,
    /// The repulsors its effects carry, and where the object last stood and when (its speed
    /// when it has no velocity of its own).
    repulsors: Vec<RepulsorRef>,
    last: Option<((i32, i32), Vec3, f64)>,
    /// The object's velocity this frame, when it moves by its physics.
    velocity: Option<Vec3>,
    /// The look's whole-object group, whose appearance's effects join the table as it starts.
    group: Group,
    /// The appearances its effects laid over the object or took off it, in order, not yet taken.
    appearance: Vec<AppearanceFx>,
    /// When its effects last had their time.
    used: Option<f64>,
    /// The lights its effects placed on it.
    lights: Vec<CarriedLight>,
    /// The streaks its effects made on it.
    streaks: Vec<crate::streak::Trail>,
    /// The streaks its effects asked turned on (true) or off since last taken, each with the
    /// holding location whose objects' streaks it means (0: every object it holds).
    streak_asks: Vec<(bool, u32)>,
    /// The sliders its effects moved since last taken, in order.
    sliders: Vec<FxSlider>,
    /// The decals its effects laid since last taken, in order.
    decals: Vec<FxDecal>,
    /// What the player's preferences last asked of its effects' entries.
    channels: Channels,
    /// The sounds its effects played or stopped, in order, not yet taken.
    sounds: Vec<FxSound>,
    /// The startup effects' entries that make an object, each with its effect, not yet taken.
    objects_to_make: Vec<(u32, FxNode)>,
}

/// A sound an object's effect played, or the sounds kept with an effect stopped. The object
/// makes the sound: it is heard from where the object is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxSound {
    /// Sound or music setting `sound` played at `volume` (the effect's strength as it starts),
    /// `kept` with effect `effect` to stop when the effect stops.
    Play {
        effect: u32,
        sound: u32,
        volume: f32,
        kept: bool,
    },
    /// The sounds kept with effect `effect` stop.
    Stop { effect: u32 },
    /// The sounds kept with effect `effect` go to `volume`.
    Volume { effect: u32, volume: f32 },
}

/// A light an effect placed on an object: the light its entity describes, its range's and
/// intensity's waveforms, its offset on the object, how long it lives (0: as long as the object)
/// and how long it fades, and when it was made.
#[derive(Clone, Debug, PartialEq)]
pub struct CarriedLight {
    pub light: auberean_world::inside::Light,
    pub range: auberean_assets::common::Waveform,
    pub intensity: auberean_assets::common::Waveform,
    pub offset: Vec3,
    pub lifespan: f32,
    pub fade: f32,
    pub created: f64,
}

/// A carried light as it stands this frame: a point light at its place, in its object's cell,
/// how much of it shows, and the light it is.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedLight {
    pub origin: Vec3,
    pub cell: u32,
    pub opacity: f32,
    pub light: CarriedLight,
}

impl PlacedLight {
    /// The light as it stands at `now`, as the frame's lights take it: a point light in its
    /// object's cell, at its place in that cell's block (`home` the block whose frame its place is
    /// in, blocks `block_length` long); its range taken now and its intensity taken now times how
    /// much of it shows (rolling `dice` for any noise), its specular color times that too; its most
    /// range and most intensity (times how much shows) from its waveforms' bounds, for how far and
    /// how strongly it may light what is near it.
    // D2-06524, D2-13333, D2-13324, D2-13325, D2-13323
    #[must_use]
    pub fn standing(
        &self,
        home: (i32, i32),
        block_length: f32,
        now: f64,
        dice: &Random,
    ) -> auberean_world::inside::Light {
        let c = &self.light;
        let cell_block = (
            i32::try_from(self.cell >> 24).unwrap_or(0),
            i32::try_from((self.cell >> 16) & 0xFF).unwrap_or(0),
        );
        #[allow(clippy::cast_precision_loss)] // a few blocks apart
        let origin = Vec3::new(
            self.origin.x - (cell_block.0 - home.0) as f32 * block_length,
            self.origin.y - (cell_block.1 - home.1) as f32 * block_length,
            self.origin.z,
        );
        let o = self.opacity;
        let s = c.light.specular;
        auberean_world::inside::Light {
            cell: self.cell,
            origin,
            kind: auberean_world::inside::LightKind::Point,
            intensity: auberean_world::wave::value(&c.intensity, now, dice) * o,
            range: auberean_world::wave::value(&c.range, now, dice).max(0.0002),
            specular: Color::new(s.r * o, s.g * o, s.b * o, s.a),
            flicker: None,
            most_intensity: Some(c.intensity.upper_bound().unwrap_or(1.0) * o),
            most_range: c.range.upper_bound().unwrap_or(0.0),
            ..c.light.clone()
        }
    }
}

impl CarriedLight {
    /// How much of it shows `now` for an object at opacity `opacity`: all of it while it lives as
    /// long as its object, or fades in no time; else it fades in over its fade time (at most its
    /// lifespan) and out from the later of its fade-in's end and its fade time before its end,
    /// held to the object's opacity.
    // D2-06524
    #[must_use]
    pub fn opacity(&self, now: f64, opacity: f32) -> f32 {
        if self.lifespan == 0.0 || self.fade <= 0.0002 {
            return opacity;
        }
        let fade_in = self.fade.min(self.lifespan);
        let fade_out = (self.lifespan - self.fade).max(fade_in);
        if fade_in >= fade_out {
            return opacity;
        }
        #[allow(clippy::cast_possible_truncation)] // a few seconds
        let age = (now - self.created) as f32;
        let o = if (0.0..fade_in).contains(&age) {
            age / fade_in * opacity
        } else if (fade_out..self.lifespan).contains(&age) {
            (1.0 - (age - fade_out) / (self.lifespan - fade_out)) * opacity
        } else {
            opacity
        };
        o.clamp(0.0, opacity)
    }
}

impl ObjectEffects {
    /// The effects of an object of look `look`, or none when its look carries no effects (no
    /// effect table and no lists of its own). Their random choices roll the systems' dice.
    // D2-06548, D2-06922, D2-13797, D2-13796
    #[must_use]
    pub fn of_look(look: &Look) -> Option<Self> {
        if !look.global.has_fx() {
            return None;
        }
        Some(Self {
            particle_scale: look.particle_scale,
            table: ObjectFxTable::of_group(&look.global),
            drawn: true,
            startup: look.global.startup_fx.entries.clone(),
            states: BTreeMap::new(),
            terrain: 0,
            systems: Vec::new(),
            next_id: 1,
            shared: false,
            repulsors: Vec::new(),
            last: None,
            velocity: None,
            group: look.global.clone(),
            appearance: Vec::new(),
            used: None,
            lights: Vec::new(),
            streaks: Vec::new(),
            streak_asks: Vec::new(),
            sliders: Vec::new(),
            decals: Vec::new(),
            channels: Channels::default(),
            sounds: Vec::new(),
            objects_to_make: Vec::new(),
        })
    }

    /// The sliders its effects moved since last taken, in order.
    pub fn take_sliders(&mut self) -> Vec<FxSlider> {
        std::mem::take(&mut self.sliders)
    }

    /// The decals its effects laid since last taken, in order.
    pub fn take_decals(&mut self) -> Vec<FxDecal> {
        std::mem::take(&mut self.decals)
    }

    /// An entry's decal, while the decal effects preference is on: its size, its turn from the way
    /// the object faces and how far off it lies each read from its waveforms at this moment; off
    /// by the entry's origin and, with a distance, by a random way across the ground for a random
    /// part of that distance.
    // D2-06891
    fn decal_fx(
        &mut self,
        systems: &ParticleSystems,
        decal: &auberean_assets::fx::Decal,
        at: &Spot,
    ) {
        if !systems.decal_effects() {
            return;
        }
        let dice = systems.dice();
        let wave = |w| auberean_world::wave::value(w, at.now, &dice);
        let (size, turn, reach) = (
            wave(&decal.size),
            wave(&decal.rotation).to_radians(),
            wave(&decal.position_radius),
        );
        let mut offset = decal.origin;
        if reach > 0.0 {
            let way = Vec3::new(dice.roll_float(-1.0, 1.0), dice.roll_float(-1.0, 1.0), 0.0);
            let far = reach * dice.roll_float(0.0, 1.0);
            let long = length(way).max(f32::EPSILON);
            offset.x += way.x / long * far;
            offset.y += way.y / long * far;
        }
        let forward = sub(
            at.place.point(Vec3::new(0.0, 1.0, 0.0)),
            at.place.point(Vec3::ZERO),
        );
        let (s, c) = (
            dereth_primitives::num::math::sinf(turn),
            dereth_primitives::num::math::cosf(turn),
        );
        let way = Vec3::new(
            forward.x * c - forward.y * s,
            forward.x * s + forward.y * c,
            forward.z,
        );
        let long = length(way).max(f32::EPSILON);
        self.decals.push(FxDecal {
            desc: crate::decal::DecalDesc {
                material: decal.material.0,
                lifespan: decal.lifespan,
                size: Vec3::new(size, size, size),
                direction: Vec3::new(way.x / long, way.y / long, way.z / long),
                color: Color::from_argb(decal.color.0),
                draw_once: false,
            },
            offset,
        });
    }

    /// The sounds its effects played or stopped since last taken, in order.
    pub fn take_sounds(&mut self) -> Vec<FxSound> {
        std::mem::take(&mut self.sounds)
    }

    /// The startup effects' entries that make an object since last taken, each with its effect.
    pub fn take_objects_to_make(&mut self) -> Vec<(u32, FxNode)> {
        std::mem::take(&mut self.objects_to_make)
    }

    /// The appearances its effects laid over the object or took off it since last taken, in
    /// order.
    pub fn take_appearance(&mut self) -> Vec<AppearanceFx> {
        std::mem::take(&mut self.appearance)
    }

    /// The effects playing that did not begin with the object, each with its desired strength:
    /// what another set of the object's effects plays again when its look changes.
    // D2-06929
    #[must_use]
    pub fn current_fx(&self) -> Vec<(u32, f32)> {
        self.states
            .iter()
            .filter(|(_, s)| !s.startup)
            .map(|(k, s)| (*k, s.desired))
            .collect()
    }

    /// An appearance effect at `scalar`: its key's strength scaled by it; above 0 the value is
    /// laid over the object, else its key is taken off. A table of 0 does nothing.
    // D2-06892
    fn do_appearance_fx(&mut self, table: u32, mut key: AppearanceKey, scalar: f32) {
        key.scalar *= scalar;
        if table == 0 {
            return;
        }
        self.appearance.push(if key.scalar > 0.0 {
            AppearanceFx::Apply { table, key }
        } else {
            AppearanceFx::Remove {
                table,
                key: key.key,
            }
        });
    }

    /// The terrain kind its entries are played for.
    #[must_use]
    pub const fn terrain(&self) -> u32 {
        self.terrain
    }

    /// The object's table.
    #[must_use]
    pub fn table(&self) -> &ObjectFxTable {
        &self.table
    }

    /// The effects being played, by effect id.
    #[must_use]
    pub fn states(&self) -> &BTreeMap<u32, FxState> {
        &self.states
    }

    /// What its look gives its systems' scale.
    #[must_use]
    pub fn particle_scale(&self) -> f32 {
        self.particle_scale
    }

    /// The particle systems the object carries.
    #[must_use]
    pub fn systems(&self) -> &[CarriedSystem] {
        &self.systems
    }

    /// Say whether its object is drawn this frame. One that isn't (the player's own body with the
    /// camera at its eyes, a body told not to be drawn, an object past its visible distance) has
    /// only the systems its records force to draw queued: the rest neither run nor show, and stay
    /// until it is drawn again.
    // D2-06658, D2-06524
    pub fn set_drawn(&mut self, drawn: bool) {
        self.drawn = drawn;
    }

    /// Play every startup effect, its systems kept by `systems`: the object was made at time
    /// `now`, standing at `place` in block `home`'s frame with `scale`.
    // D2-06529
    pub fn start(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        home: (i32, i32),
        place: &Mat4,
        scale: Vec3,
        now: f64,
    ) {
        self.start_on(systems, records, home, place, scale, now, &NoRig);
    }

    /// [`ObjectEffects::start`] on a figure: what its startup effects set on a bone or holding
    /// location stands there.
    // D2-06529
    #[allow(clippy::too_many_arguments)]
    pub fn start_on(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        home: (i32, i32),
        place: &Mat4,
        scale: Vec3,
        now: f64,
        rig: &dyn Rig,
    ) {
        let at = Spot {
            home,
            place: *place,
            scale,
            now,
        };
        // The table takes the effects the look's appearance lists as the object starts.
        self.table = ObjectFxTable::of_group_with(records, &self.group);
        for (effect, strength) in self.startup.clone() {
            self.startup_fx(systems, records, effect, strength, &at, false, rig);
        }
        self.object_end(systems, records, home, place, scale, now);
    }

    /// The rest of every startup effect, played as the object's making ends, and again each time
    /// its look is brought up to date: its streaks, its sounds (each entry's, kept to stop with the
    /// effect; taken with [`ObjectEffects::take_sounds`]), its lights and its repulsors (another each
    /// time). The objects they make are left for the host ([`ObjectEffects::take_objects_to_make`]),
    /// another each time too.
    // D2-06529
    pub fn object_end(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        home: (i32, i32),
        place: &Mat4,
        scale: Vec3,
        now: f64,
    ) {
        let at = Spot {
            home,
            place: *place,
            scale,
            now,
        };
        for (effect, strength) in self.startup.clone() {
            self.startup_fx(systems, records, effect, strength, &at, true, &NoRig);
        }
    }

    /// One startup effect at `strength`, played at `strength` from then on: as the object starts,
    /// each entry's particle systems set going and its appearance laid over the object; for the
    /// `rest`, each entry's repulsors.
    // D2-06933
    #[allow(clippy::too_many_arguments)]
    fn startup_fx(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        effect: u32,
        strength: f32,
        at: &Spot,
        rest: bool,
        rig: &dyn Rig,
    ) {
        let list = self.table.fx_data(records, effect);
        if list.is_empty() {
            return;
        }
        let state = self.states.entry(effect).or_default();
        state.desired = strength;
        state.current = strength;
        state.startup = true;
        let mut keeps_sounds = false;
        for data in &list {
            let node = node_for(data, self.terrain).clone();
            if rest {
                if let Some((true, s)) = &node.streak {
                    self.streak_fx(systems, s, at.now);
                }
                if let Some(sound) = sound_id(&node, &systems.dice()) {
                    self.sounds.push(FxSound::Play {
                        effect,
                        sound,
                        volume: strength,
                        kept: true,
                    });
                    keeps_sounds = true;
                }
                if let Some((true, d)) = &node.decal {
                    self.decal_fx(systems, d, at);
                }
                if let Some((true, r)) = node.repulsor {
                    self.repulse_fx(systems, &r, at);
                }
                if let Some((true, l)) = &node.light {
                    self.light_fx(systems, l, node.bone_offset, at.now);
                }
                if node.visual_desc.is_some_and(|v| !v.is_none()) {
                    self.objects_to_make.push((effect, node));
                }
                continue;
            }
            if node.particles.as_ref().is_some_and(|p| !p.is_empty()) {
                if let Some(id) = self.particle_fx(systems, records, &node, at, rig) {
                    if let Some(s) = self.states.get_mut(&effect) {
                        s.systems.push(id);
                    }
                }
            }
            if let Some(apr) = node.appearance.filter(|a| !a.is_none()) {
                let key = node.appearance_key.unwrap_or_default();
                self.do_appearance_fx(apr.0, key, strength);
            }
        }
        if let Some(s) = self.states.get_mut(&effect) {
            s.keeps_sounds = keeps_sounds;
        }
    }

    /// Carry a repulsor an effect names: its push, reach, lifetime, the speed it asks for and
    /// whether it pulls.
    // D2-06882, D2-06482, D2-06663
    fn repulse_fx(
        &mut self,
        systems: &ParticleSystems,
        r: &auberean_assets::fx::Repulsor,
        at: &Spot,
    ) {
        self.repulsors.push(systems.create_repulsor(Repulsor::new(
            r.acceleration,
            r.range,
            r.lifetime,
            r.min_velocity,
            r.attracts,
            at.now,
        )));
    }

    /// Make the streak an effect names on the object, made at `now`; none while the streaks
    /// preference is off.
    // D2-06883, D2-06481, D2-06664
    fn streak_fx(&mut self, systems: &ParticleSystems, s: &auberean_assets::fx::Streak, now: f64) {
        if !systems.streak_effects() {
            return;
        }
        self.streaks.push(crate::streak::Trail::new(
            crate::streak::StreakDesc::of(s),
            now,
        ));
    }

    /// The streaks its effects made on it.
    #[must_use]
    pub fn streaks(&self) -> &[crate::streak::Trail] {
        &self.streaks
    }

    /// The streaks its effects made on it, to place and run.
    pub fn streaks_mut(&mut self) -> &mut [crate::streak::Trail] {
        &mut self.streaks
    }

    /// Turn every streak on it on (`on`) or off.
    // D2-06475, D2-06476, D2-06644, D2-06645
    pub fn turn_streaks(&mut self, on: bool) {
        for s in &mut self.streaks {
            if on {
                s.activate();
            } else {
                s.deactivate();
            }
        }
    }

    /// The streaks its effects asked turned on or off since last taken, in order, each with the
    /// holding location it means.
    pub fn take_streak_asks(&mut self) -> Vec<(bool, u32)> {
        std::mem::take(&mut self.streak_asks)
    }

    /// Place the light an effect names on the object at the effect's offset (none: its origin),
    /// made at `now` from the light properties of its entity; none while the light source effects
    /// preference is off.
    // D2-06902, D2-06665
    fn light_fx(
        &mut self,
        systems: &ParticleSystems,
        l: &auberean_assets::fx::EffectLight,
        offset: Option<Vec3>,
        now: f64,
    ) {
        if !systems.light_sources() {
            return;
        }
        let props = l.entity.properties.clone().unwrap_or_default();
        let (light, range, intensity) =
            auberean_world::inside::Light::of_properties(0, Vec3::ZERO, &props);
        self.lights.push(CarriedLight {
            light,
            range,
            intensity,
            offset: offset.unwrap_or(Vec3::ZERO),
            lifespan: l.lifespan,
            fade: l.fade_time,
            created: now,
        });
    }

    /// The lights its effects placed on the object, as they stand `now` with the object at
    /// `place` (its frame, no scale) in cell `cell` at opacity `opacity`: each past its lifespan
    /// let go of; the rest point lights at their offset on the object, as much showing as they
    /// have faded.
    // D2-06524
    pub fn lights(&mut self, place: &Mat4, cell: u32, now: f64, opacity: f32) -> Vec<PlacedLight> {
        self.let_go_of_spent_lights(now);
        self.lights_now(place, cell, now, opacity)
    }

    /// The lights its effects placed on the object whose lifespan has run out by `now` are let go
    /// of.
    // D2-06524
    pub fn let_go_of_spent_lights(&mut self, now: f64) {
        self.lights
            .retain(|l| l.lifespan == 0.0 || f64::from(l.lifespan) + l.created > now);
    }

    /// The lights its effects placed on the object, as [`Self::lights`] gives them, leaving out
    /// those spent rather than letting go of them.
    #[must_use]
    pub fn lights_now(&self, place: &Mat4, cell: u32, now: f64, opacity: f32) -> Vec<PlacedLight> {
        self.lights
            .iter()
            .filter(|l| l.lifespan == 0.0 || f64::from(l.lifespan) + l.created > now)
            .map(|l| {
                let mut light = l.clone();
                light.light.kind = auberean_world::inside::LightKind::Point;
                PlacedLight {
                    origin: place.point(l.offset),
                    cell,
                    opacity: l.opacity(now, opacity),
                    light,
                }
            })
            .collect()
    }

    /// The object's velocity this frame (metres a second): its repulsors push while its length
    /// is fast enough. Taken by the next [`ObjectEffects::think`].
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = Some(velocity);
    }

    /// How many repulsors the object carries.
    #[must_use]
    pub fn repulsor_count(&self) -> usize {
        self.repulsors.len()
    }

    /// Set going an effect's particle system on the object: on the bone its entry names, raised by
    /// its offset (none when the figure has no such bone); else at the holding location it names,
    /// on that location's bone; else at the object, raised by its offset. The id the object knows
    /// it by when it runs until stopped.
    // D2-06901, D2-06484, D2-06525
    fn particle_fx(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        node: &FxNode,
        at: &Spot,
        rig: &dyn Rig,
    ) -> Option<u32> {
        let desc = particle_id(node, &systems.dice())?;
        let lifted = || {
            let o = node.bone_offset.unwrap_or(Vec3::ZERO);
            if o.x.is_nan() || o.y.is_nan() || o.z.is_nan() {
                Mat4::IDENTITY
            } else {
                Mat4::translation(o)
            }
        };
        let (offset, on) = match (
            node.bone.filter(|k| *k != 0),
            node.holding_location.filter(|k| *k != 0),
        ) {
            (Some(kind), _) if rig.has_figure() => {
                rig.bone(kind)?;
                (lifted(), CarriedOn::Bone(kind))
            }
            (None, Some(kind)) if rig.has_figure() => (
                rig.holding_location(kind).unwrap_or(Mat4::IDENTITY),
                CarriedOn::HoldingLocation(kind),
            ),
            _ => (lifted(), CarriedOn::Object),
        };
        let remove_rotation = node.remove_rotation.unwrap_or(false);
        let id = self.create_system(systems, records, desc, offset, on, remove_rotation, at, rig);
        id.filter(|_| particle_is_looping(records, desc))
    }

    /// A system of record `desc` set going on the object at `offset` on what it stands `on`: one
    /// already going that may be shared, else a new one, which other objects may share when this
    /// one is placed with the world.
    // D2-06483, D2-06662
    #[allow(clippy::too_many_arguments)]
    fn create_system(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        desc: u32,
        offset: Mat4,
        on: CarriedOn,
        remove_rotation: bool,
        at: &Spot,
        rig: &dyn Rig,
    ) -> Option<u32> {
        let place = carried_place(&offset, on, remove_rotation, &at.place, rig);
        let system = systems.create(
            records,
            desc,
            true,
            at.home,
            place,
            at.scale,
            self.particle_scale,
            at.now,
        )?;
        systems.set_can_be_shared(&system, self.shared);
        let id = self.next_id;
        self.next_id += 1;
        self.systems.push(CarriedSystem {
            id,
            desc,
            offset,
            on,
            remove_rotation,
            system,
        });
        Some(id)
    }

    /// Plays effect `effect` at `intensity`, its particle systems kept by `systems`, the object
    /// standing at `at`: for one not playing, every entry the object's table lists for it (in its
    /// variant for the terrain the object stands on) that is for this object (the player's own
    /// only, by day or by night) is applied: its strength (none while it ramps in), its appearance
    /// at that strength, its sound (one of its list, at that strength, kept to stop with the effect
    /// when it loops and the entry is not fired and forgotten; taken with
    /// [`ObjectEffects::take_sounds`]), its effect script (handed back for the figure), its
    /// particles (one fired
    /// and forgotten set going on its own where `rig` puts it, else one carried, kept with the
    /// effect when it runs until stopped); and when it ends, by the last entry applied. With none
    /// applied it does not play. One playing already takes the new strength (its entries for this
    /// object that do not ramp at once) and lays its appearances over the object again at the
    /// strength it then has; nothing restarts; a startup effect is left alone. Whether it played,
    /// and the effect scripts to play. Its streaks are made, and the streaks it turns on or off asked
    /// for, its decals laid and its repulsors carried.
    // D2-06934, D2-06932, D2-06887
    #[allow(clippy::too_many_arguments)]
    pub fn do_fx(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        effect: u32,
        intensity: f32,
        at: &FxSpot,
        who: FxWho,
        rig: &dyn Rig,
    ) -> (bool, Vec<u32>) {
        let list = self.table.fx_data(records, effect);
        if list.is_empty() {
            return (false, Vec::new());
        }
        let channels = systems.fx_channels();
        self.channels = channels;
        let applies = |n: &FxNode| {
            !channels.mismatch(n)
                && (n.player_only != Some(true) || who.player)
                && (n.day_time.unwrap_or(true) || who.night)
                && (n.night_time.unwrap_or(true) || !who.night)
        };
        let terrain = self.terrain;
        if let Some(state) = self.states.get_mut(&effect) {
            if state.startup {
                return (false, Vec::new());
            }
            state.desired = intensity;
            // Playing already: every entry for this object (whatever the time of day) that does
            // not ramp takes the strength at once, and an appearance takes the strength it has.
            let mut current = state.current;
            let mut appearance = Vec::new();
            for data in &list {
                let n = node_for(data, terrain);
                if channels.mismatch(n) || (n.player_only == Some(true) && !who.player) {
                    continue;
                }
                if n.ramp_time.unwrap_or(0.0) < 0.0002 {
                    current = intensity;
                }
                if let Some(apr) = n.appearance.filter(|a| !a.is_none()) {
                    appearance.push((apr.0, n.appearance_key.unwrap_or_default(), current));
                }
            }
            state.current = current;
            if state.keeps_sounds {
                self.sounds.push(FxSound::Volume {
                    effect,
                    volume: current,
                });
            }
            for (apr, key, scalar) in appearance {
                self.do_appearance_fx(apr, key, scalar);
            }
            return (true, Vec::new());
        }
        let spot = Spot {
            home: at.home,
            place: at.place,
            scale: at.scale,
            now: at.now,
        };
        let mut state = FxState {
            desired: intensity,
            ..FxState::default()
        };
        let mut applied = false;
        let mut scripts = Vec::new();
        for data in &list {
            let node = node_for(data, terrain).clone();
            if !applies(&node) {
                continue;
            }
            applied = true;
            state.current = if node.ramp_time.unwrap_or(0.0) >= 0.0002 {
                0.0
            } else {
                intensity
            };
            if let Some(sound) = sound_id(&node, &systems.dice()) {
                let kept = sound_is_looping(records, sound) && node.fire_and_forget != Some(true);
                state.keeps_sounds |= kept;
                self.sounds.push(FxSound::Play {
                    effect,
                    sound,
                    volume: state.current,
                    kept,
                });
            }
            if let Some(apr) = node.appearance.filter(|a| !a.is_none()) {
                let key = node.appearance_key.unwrap_or_default();
                self.do_appearance_fx(apr.0, key, state.current);
            }
            let script = node.fx_script.filter(|d| !d.is_none()).map(|d| d.0);
            scripts.extend(script);
            self.sliders.extend(node_sliders(&node));
            if node.color.is_some_and(|(on, _)| on) && node.color_lifetime.unwrap_or(0.0) != 0.0 {
                state.restore_color = true;
            }
            if node.particles.as_ref().is_some_and(|p| !p.is_empty()) {
                if node.fire_and_forget == Some(true) {
                    self.fire_particle_fx(systems, records, &node, &spot, rig);
                } else if let Some(id) = self.particle_fx(systems, records, &node, &spot, rig) {
                    state.systems.push(id);
                }
            }
            if let Some((true, s)) = &node.streak {
                self.streak_fx(systems, s, at.now);
            }
            if let Some((true, d)) = &node.decal {
                self.decal_fx(systems, d, &spot);
            }
            if let Some((true, r)) = node.repulsor {
                self.repulse_fx(systems, &r, &spot);
            }
            if let Some((true, l)) = &node.light {
                self.light_fx(systems, l, node.bone_offset, at.now);
            }
            // An entry turns the streaks of what the object holds at its holding location on
            // (while the streaks preference is on) or off.
            // D2-06477, D2-06478
            let location = node.holding_location.unwrap_or(0);
            if node.start_streak == Some(true) && systems.streak_effects() {
                self.streak_asks.push((true, location));
            }
            if node.stop_streak == Some(true) {
                self.streak_asks.push((false, location));
            }
            state.end = calc_duration(records, &node, &systems.dice(), script, state.restore_color)
                .map(|d| at.now + d);
        }
        if !applied {
            return (false, scripts);
        }
        self.states.insert(effect, state);
        (true, scripts)
    }

    /// A fired and forgotten particle system set going on its own where the object stands: at
    /// the effect's holding location (that location's own place on the object, whatever bone it
    /// is on; the object's frame when it has none), else at the effect's bone (at the effect's
    /// offset when the object has no such bone), else on the water of the object's cell, else at
    /// the effect's offset. It is the world's, and runs until it is spent.
    // D2-06895
    fn fire_particle_fx(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        node: &FxNode,
        at: &Spot,
        rig: &dyn Rig,
    ) {
        let Some(desc) = particle_id(node, &systems.dice()) else {
            return;
        };
        let offset = || {
            node.bone_offset
                .filter(|o| !(o.x.is_nan() || o.y.is_nan() || o.z.is_nan()))
                .map_or(Mat4::IDENTITY, Mat4::translation)
        };
        let mut place = at.place;
        let on = if let Some(kind) = node.holding_location.filter(|k| *k != 0) {
            rig.holding_location(kind).unwrap_or(Mat4::IDENTITY)
        } else if let Some(kind) = node.bone.filter(|k| *k != 0) {
            rig.bone(kind).unwrap_or_else(offset)
        } else if node.water_plane == Some(true) {
            if let Some(h) = rig.water_height() {
                place.0[3][2] = h;
            }
            Mat4::IDENTITY
        } else {
            offset()
        };
        let place = on.then(&place);
        let _ = systems.create_autonomous(
            records,
            desc,
            at.home,
            place,
            at.scale,
            self.particle_scale,
            at.now,
        );
    }

    /// Stops effect `effect`: the particle systems it keeps going are let go of, and it no longer
    /// plays; a startup effect only when `force`d. Asked (not `force`d), the appearances its
    /// entries lay over the object are taken off it. An entry that says so puts the object's color
    /// back to white in a tenth of a second, whether the effect was playing or not.
    // D2-06920, D2-06588, D2-06646
    pub fn stop_fx(&mut self, records: &WorldRecords, effect: u32, force: bool) -> bool {
        let Some(state) = self.states.get(&effect) else {
            self.restore_color(records, effect);
            self.stop_streaks(records, effect);
            return false;
        };
        if state.startup && !force {
            return false;
        }
        let gone = state.systems.clone();
        self.restore_color(records, effect);
        self.stop_streaks(records, effect);
        self.systems.retain(|c| !gone.contains(&c.id));
        if !force {
            for data in self.table.fx_data(records, effect) {
                let n = node_for(&data, self.terrain);
                if let Some(apr) = n.appearance.filter(|a| !a.is_none()) {
                    let key = n.appearance_key.unwrap_or_default().key;
                    self.appearance
                        .push(AppearanceFx::Remove { table: apr.0, key });
                }
            }
        }
        self.states.remove(&effect);
        self.sounds.push(FxSound::Stop { effect });
        true
    }

    /// The streaks the effect's entries turn on are turned off, as it is stopped.
    fn stop_streaks(&mut self, records: &WorldRecords, effect: u32) {
        for data in self.table.fx_data(records, effect) {
            let n = node_for(&data, self.terrain);
            if n.start_streak == Some(true) {
                self.streak_asks
                    .push((false, n.holding_location.unwrap_or(0)));
            }
        }
    }

    /// The object's color back to white, once for each of the effect's entries that says so.
    fn restore_color(&mut self, records: &WorldRecords, effect: u32) {
        for data in self.table.fx_data(records, effect) {
            if node_for(&data, self.terrain).restore == Some(true) {
                self.color_back();
            }
        }
    }

    /// The object's color back to white in a tenth of a second.
    fn color_back(&mut self) {
        self.sliders
            .extend(COLOR_SLIDERS.into_iter().map(|slider| FxSlider {
                slider,
                value: 1.0,
                time: 0.1,
                children: false,
            }));
    }

    /// The object is going: every effect it plays stops, its startup effects too, with the systems
    /// they keep going, and its sounds with them.
    // D2-06923, D2-06643
    pub fn destroy(&mut self) {
        for effect in std::mem::take(&mut self.states).into_keys() {
            self.sounds.push(FxSound::Stop { effect });
        }
        self.systems.clear();
    }

    /// The effects have their time up to `now` (the client's time; nothing passes the first
    /// time): each whose strength is not yet the one asked moves toward it, at each of its entries
    /// in turn (one that ramps by the time passed over its ramp's time, held at the strength
    /// asked; one that does not, to it at once), each entry's appearance laid over the object at
    /// the strength it has then (or taken off at none); then those whose end has come are over: their
    /// systems go, what they lay over the object and the sounds they keep going stay, and the
    /// color goes back only when an entry's color lasts a time.
    // D2-06924
    pub fn use_time(&mut self, records: &WorldRecords, now: f64) {
        #[allow(clippy::cast_possible_truncation)] // a frame's span
        let dt = self.used.map_or(0.0, |t| (now - t) as f32);
        self.used = Some(now);
        let mut appearance = Vec::new();
        for (effect, state) in &mut self.states {
            if state.current == state.desired {
                continue;
            }
            for data in self.table.fx_data(records, *effect) {
                let n = node_for(&data, self.terrain);
                if self.channels.mismatch(n) {
                    continue;
                }
                let ramp = n.ramp_time.unwrap_or(0.0);
                if ramp >= 0.0002 {
                    let step = dt / ramp;
                    state.current = if state.desired <= state.current {
                        (state.current - step).max(state.desired)
                    } else {
                        (state.current + step).min(state.desired)
                    };
                } else {
                    state.current = state.desired;
                }
                if let Some(apr) = n.appearance.filter(|a| !a.is_none()) {
                    appearance.push((apr.0, n.appearance_key.unwrap_or_default(), state.current));
                }
            }
            if state.keeps_sounds {
                self.sounds.push(FxSound::Volume {
                    effect: *effect,
                    volume: state.current,
                });
            }
        }
        for (apr, key, scalar) in appearance {
            self.do_appearance_fx(apr, key, scalar);
        }
        let ended: Vec<u32> = self
            .states
            .iter()
            .filter(|(_, s)| s.end.is_some_and(|e| e <= now))
            .map(|(k, _)| *k)
            .collect();
        for effect in ended {
            let Some(state) = self.states.remove(&effect) else {
                continue;
            };
            self.systems.retain(|c| !state.systems.contains(&c.id));
            if state.restore_color {
                self.color_back();
            }
        }
    }

    /// Queue every system to be drawn with the object (now at `place` in block `home`'s frame with
    /// `scale`), lit with `light`, run to `now`; let go of the ones that are done. Each repulsor it
    /// carries stands where the object does, at the object's speed (its velocity's length, when
    /// one was set this frame, else how fast it moved since the last frame); one that has
    /// outlived its lifetime goes.
    // D2-06649
    pub fn think(
        &mut self,
        systems: &ParticleSystems,
        home: (i32, i32),
        place: &Mat4,
        scale: Vec3,
        light: Color,
        now: f64,
    ) {
        self.think_on(systems, home, place, scale, light, now, &NoRig);
    }

    /// [`ObjectEffects::think`] on a figure: each system on a bone stands where that bone is now.
    // D2-06649
    #[allow(clippy::too_many_arguments)]
    pub fn think_on(
        &mut self,
        systems: &ParticleSystems,
        home: (i32, i32),
        place: &Mat4,
        scale: Vec3,
        light: Color,
        now: f64,
        rig: &dyn Rig,
    ) {
        let drawn = self.drawn;
        for c in &self.systems {
            if !drawn && !systems.force_draw(&c.system) {
                continue;
            }
            let at = carried_place(&c.offset, c.on, c.remove_rotation, place, rig);
            systems.queue(&c.system, home, at, scale, self.particle_scale, light, now);
        }
        self.systems.retain(|c| {
            !(drawn || systems.force_draw(&c.system)) || !systems.ready_to_die(&c.system, now)
        });
        let origin = place.point(Vec3::ZERO);
        let speed = match (self.velocity.take(), self.last) {
            (Some(v), _) => length(v),
            (None, Some((h, o, t))) if h == home && now > t => {
                #[allow(clippy::cast_possible_truncation)] // a frame's span
                let dt = (now - t) as f32;
                length(sub(origin, o)) / dt
            }
            _ => 0.0,
        };
        self.last = Some((home, origin, now));
        self.repulsors
            .retain(|r| !systems.repulsor_ready_to_die(r, now));
        for r in &self.repulsors {
            systems.place_repulsor(r, home, origin, speed);
        }
    }
}

/// Where an object stands as it sets a system going: in block `home`'s frame, at `place` with
/// `scale`, at the client's running time `now`.
struct Spot {
    home: (i32, i32),
    place: Mat4,
    scale: Vec3,
    now: f64,
}

/// Where a carried system stands: its offset, then the bone it follows as the figure stands now
/// (with no scale), then the object's place. An effect that asks for its bone's turn to be taken
/// off keeps only where that puts it on the object.
// D2-06649
fn carried_place(
    offset: &Mat4,
    on: CarriedOn,
    remove_rotation: bool,
    place: &Mat4,
    rig: &dyn Rig,
) -> Mat4 {
    let bone = match on {
        CarriedOn::Object => None,
        CarriedOn::Bone(kind) => rig.bone(kind),
        CarriedOn::HoldingLocation(kind) => rig.holding_bone(kind),
    };
    let on_object = bone.map_or(*offset, |b| offset.then(&b));
    let on_object = if remove_rotation {
        Mat4::translation(on_object.point(Vec3::ZERO))
    } else {
        on_object
    };
    on_object.then(place)
}

/// How long an object an effect makes or retargets lasts when its entry doesn't say, seconds.
const DEFAULT_LIFETIME: f32 = 15.0;

/// How long an entry of an effect plays, seconds; none for until it is stopped: a looping particle
/// system's never ends, else none at all; an object it makes or a retarget, its lifetime if not 0
/// (15 s unless the entry says, never if negative); an effect on a target, never; an appearance, a tenth of a second when its scalar
/// lessens it, else never; an effect script, the script's length. (A color it restores is not
/// played here.)
// D2-06903
#[must_use]
pub fn calc_duration(
    records: &WorldRecords,
    node: &FxNode,
    dice: &Random,
    script: Option<u32>,
    restore_color: bool,
) -> Option<f64> {
    let sound = sound_id(node, dice).is_some_and(|s| sound_is_looping(records, s));
    let particles = particle_id(node, dice).is_some_and(|p| particle_is_looping(records, p));
    let mut d = if sound || particles { None } else { Some(0.0) };
    if node.visual_desc.is_some_and(|v| !v.is_none()) || node.retarget == Some(true) {
        let life = f64::from(node.lifetime.unwrap_or(DEFAULT_LIFETIME));
        if life != 0.0 {
            d = (life >= 0.0).then_some(life);
        }
    }
    if node.target_fx.is_some_and(|t| t != 0) {
        d = None;
    }
    if node.appearance.is_some_and(|a| !a.is_none()) {
        d = node
            .appearance_key
            .is_some_and(|k| k.scalar < 0.0)
            .then_some(0.1);
    }
    if let Some(s) = script {
        if let Some(fx) = records.get::<auberean_assets::fx::FxScript>(s) {
            d = Some(f64::from(fx.period));
        }
    }
    // A color that lasts holds the effect for as long.
    if restore_color {
        if let Some(life) = node.color_lifetime.filter(|l| *l > 0.0) {
            d = Some(f64::from(life));
        }
    }
    d
}

/// The sound an effect plays: its only one, else one of its list at random.
// D2-15719
fn sound_id(node: &FxNode, rng: &Random) -> Option<u32> {
    let list = node.sounds.as_deref().unwrap_or(&[]);
    match list.len() {
        0 => None,
        1 => Some(list[0].0).filter(|&s| s != 0),
        n => {
            let k = rng.roll_int(0, i32::try_from(n - 1).unwrap_or(0));
            usize::try_from(k)
                .ok()
                .and_then(|k| list.get(k))
                .map(|d| d.0)
                .filter(|&s| s != 0)
        }
    }
}

/// Whether a sound or music setting loops (its loop bit), so that an effect keeps it to stop
/// with the effect.
// D2-06896
fn sound_is_looping(records: &WorldRecords, sound: u32) -> bool {
    match sound >> 24 {
        0x2A => records
            .get::<auberean_assets::sound::SoundInfo>(sound)
            .is_some_and(|s| s.flags & 1 != 0),
        0x2C => records
            .get::<auberean_assets::sound::MusicInfo>(sound)
            .is_some_and(|m| m.flags & 1 != 0),
        _ => false,
    }
}

/// The system an effect sets going: its only one, else one of its list at random.
// D2-15720
fn particle_id(node: &FxNode, rng: &Random) -> Option<u32> {
    let list = node.particles.as_deref().unwrap_or(&[]);
    match list.len() {
        0 => None,
        1 => Some(list[0].0),
        n => {
            let k = rng.roll_int(0, i32::try_from(n - 1).unwrap_or(0));
            usize::try_from(k)
                .ok()
                .and_then(|k| list.get(k))
                .map(|d| d.0)
        }
    }
}

/// Whether system `desc` runs until it is stopped.
// D2-06897
#[must_use]
pub fn particle_is_looping(records: &WorldRecords, desc: u32) -> bool {
    records
        .get::<Particles>(desc)
        .is_some_and(|p| approximate_lifetime(&p) < 0.0)
}

/// An object placed with the world whose look plays startup effects.
#[derive(Clone, Debug)]
pub struct PlacedLook {
    /// Which object: its block, its cell (0 for the block's own objects) and its place in the
    /// list.
    pub key: u64,
    /// The block whose frame `place` is in.
    pub block: (i32, i32),
    pub look: Arc<Look>,
    /// Where it stands in its block's frame: its origin and turn.
    pub place: Mat4,
    pub scale: Vec3,
}

impl PlacedLook {
    /// The key of entity `index` of cell `cell` (0: the block's own list) of block `block`.
    #[must_use]
    pub fn key(block: (i32, i32), cell: u16, index: usize) -> u64 {
        let byte = |v: i32| u64::try_from(v & 0xFF).unwrap_or(0);
        let index = u64::try_from(index).unwrap_or(0) & 0xFFFF;
        (((byte(block.0) << 8) | byte(block.1)) << 32) | (u64::from(cell) << 16) | index
    }
}

/// The look object `object` (a visual description, or an entity made from one) shows when it
/// plays startup effects, with the scale of the entities it is made from and of the look.
#[must_use]
pub fn object_effect_look(records: &WorldRecords, object: u32) -> Option<(Arc<Look>, Vec3)> {
    let mut id = object;
    let mut scale = Vec3::new(1.0, 1.0, 1.0);
    for _ in 0..8 {
        match id >> 24 {
            0x47 => {
                let inner = records.entity(DataId(id))?;
                if let Some(s) = inner.scale {
                    scale = Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z);
                }
                id = inner.object?;
            }
            0x1F => {
                let look = Look::of_record(records, DataId(id)).ok()?;
                if !look.global.has_fx() || look.global.startup_fx.is_empty() {
                    return None;
                }
                let s = look.scale;
                return Some((
                    Arc::new(look),
                    Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z),
                ));
            }
            _ => return None,
        }
    }
    None
}

/// Where an entity placed with the world stands (its origin and turn, no scale) and its own
/// scale, when it is one that is drawn.
#[must_use]
pub fn entity_place(e: &Entity) -> Option<(u32, Mat4, Vec3)> {
    if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) {
        return None;
    }
    let object = e.object?;
    let place = e.offset.as_ref().map_or(Mat4::IDENTITY, |f| {
        Mat4::from_parts(f.frame.origin, f.frame.rotation, Vec3::new(1.0, 1.0, 1.0))
    });
    Some((object, place, e.scale.unwrap_or(Vec3::new(1.0, 1.0, 1.0))))
}

/// The effects of the objects an interface scene places: begun when the scene is first shown, run
/// on the scene's own clock with systems of its own, lit by the scene's lights.
#[derive(Debug, Default)]
pub struct ViewEffects {
    systems: ParticleSystems,
    effects: PlacedEffects,
    placed: Vec<PlacedLook>,
}

impl ViewEffects {
    /// The effects of the objects among `entities` that play any.
    #[must_use]
    pub fn of_scene(records: &WorldRecords, entities: &[Entity], dice: &Random) -> Self {
        let systems = ParticleSystems::default();
        systems.set_dice(dice);
        let placed = entities
            .iter()
            .enumerate()
            .filter_map(|(index, e)| {
                let (object, place, own) = entity_place(e)?;
                let (look, scale) = object_effect_look(records, object)?;
                Some(PlacedLook {
                    key: PlacedLook::key(VIEW_BLOCK, 0, index),
                    block: VIEW_BLOCK,
                    look,
                    place,
                    scale: Vec3::new(own.x * scale.x, own.y * scale.y, own.z * scale.z),
                })
            })
            .collect();
        Self {
            systems,
            effects: PlacedEffects::default(),
            placed,
        }
    }

    /// What the effects draw at `now`, seen from `eye` with right, forward and up: their lit
    /// particles take `light` and, when the scene has lights, those kept where each stands.
    pub fn batches(
        &mut self,
        records: &WorldRecords,
        now: f64,
        light: Color,
        lights: Option<(&BlockLights, &Outside)>,
        eye: Vec3,
        axes: (Vec3, Vec3, Vec3),
    ) -> Vec<ObjectBatch> {
        self.effects.sync(&self.systems, records, &self.placed, now);
        self.effects.draw(&self.systems, now, light);
        if let Some((lights, outside)) = lights {
            let mut order = lights.scene();
            self.systems.light_queued(now, |_, at| {
                let kept = order.keep(lights, at, 0.0, outside);
                Some(lights.particle_lights(&kept))
            });
        }
        self.systems.batches(VIEW_BLOCK, 1.0, now, eye, axes)
    }
}

/// The block an interface scene's objects are taken to stand in: it has no other.
const VIEW_BLOCK: (i32, i32) = (0, 0);

/// One placed object's effects, kept from frame to frame.
#[derive(Clone, Debug)]
struct Placed {
    block: (i32, i32),
    place: Mat4,
    scale: Vec3,
    effects: ObjectEffects,
    rig: Option<RestRig>,
    /// The distances its setup is seen and degraded at, one for each model detail.
    detail: Option<[auberean_assets::setup::DetailDistances; 3]>,
}

/// The distances a look's setup is seen and degraded at: its root part group's.
fn seen_by(
    records: &WorldRecords,
    look: &Look,
) -> Option<[auberean_assets::setup::DetailDistances; 3]> {
    let group = look
        .root
        .and_then(|key| look.groups.entries.iter().find(|(k, _)| *k == key))
        .map_or(&look.global, |(_, g)| g);
    let setup = records.loaded_setup(group.setup)?;
    Some(setup.get().detail)
}

/// Where the camera stands, for telling which placed objects are drawn: their systems are queued
/// only for those.
#[derive(Clone, Copy, Debug)]
pub struct PlacedSight {
    /// The camera, in block `home`'s frame.
    pub eye: Vec3,
    pub home: (i32, i32),
    pub block_length: f32,
    /// What the scenery draw distance scales visible distances by.
    pub scale: f32,
    /// The model detail preference.
    pub model_detail: usize,
}

/// The effects of the objects placed with the world around the viewer: an object's begin when it
/// first comes into the world drawn and go when it leaves it.
#[derive(Clone, Debug, Default)]
pub struct PlacedEffects {
    objects: BTreeMap<u64, Placed>,
}

impl PlacedEffects {
    /// How many objects play effects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether none does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Follow the objects now drawn, `placed`: those new to it begin their startup effects at
    /// `now`, their systems kept by `systems` and shared with the other objects placed with the
    /// world; those no longer drawn are dropped, letting go of their systems.
    /// The objects their startup effects make are not made: an object placed with the world has no
    /// body for them to hang from.
    pub fn sync(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        placed: &[PlacedLook],
        now: f64,
    ) {
        let keep: std::collections::BTreeSet<u64> = placed.iter().map(|p| p.key).collect();
        self.objects.retain(|k, _| keep.contains(k));
        for p in placed {
            if self.objects.contains_key(&p.key) {
                continue;
            }
            let Some(mut effects) = ObjectEffects::of_look(&p.look) else {
                continue;
            };
            effects.shared = true;
            let rig = RestRig::of_look(records, &p.look);
            match &rig {
                Some(rig) => {
                    effects.start_on(systems, records, p.block, &p.place, p.scale, now, rig);
                }
                None => effects.start(systems, records, p.block, &p.place, p.scale, now),
            }
            self.objects.insert(
                p.key,
                Placed {
                    block: p.block,
                    place: p.place,
                    scale: p.scale,
                    effects,
                    rig,
                    detail: seen_by(records, &p.look),
                },
            );
        }
    }

    /// The systems the object `key` carries.
    #[must_use]
    pub fn systems_of(&self, key: u64) -> Option<&[CarriedSystem]> {
        self.objects.get(&key).map(|p| p.effects.systems())
    }

    /// Say where the camera stands: an object past its visible distance from it isn't drawn, so
    /// its systems aren't queued but for those their records force to draw.
    // D2-06658
    pub fn see_from(&mut self, sight: &PlacedSight) {
        for p in self.objects.values_mut() {
            let Some(detail) = &p.detail else {
                continue;
            };
            let d = &detail[sight.model_detail.min(2)];
            #[allow(clippy::cast_precision_loss)] // a few blocks
            let shift = Vec3::new(
                (p.block.0 - sight.home.0) as f32 * sight.block_length,
                (p.block.1 - sight.home.1) as f32 * sight.block_length,
                0.0,
            );
            let at = p.place.point(Vec3::ZERO);
            let at = Vec3::new(at.x + shift.x, at.y + shift.y, at.z);
            let far = length(sub(at, sight.eye));
            p.effects
                .set_drawn(far <= (sight.scale * d.visible).max(20.0));
        }
    }

    /// The lights every object's effects place, as they stand `now`, each in the frame of block
    /// `home`, in the object's cell (its block's first land cell for one of the block's own).
    pub fn lights(&mut self, home: (i32, i32), block_length: f32, now: f64) -> Vec<PlacedLight> {
        let mut out = Vec::new();
        for (key, p) in &mut self.objects {
            #[allow(clippy::cast_precision_loss)] // a few blocks
            let shift = Mat4::translation(Vec3::new(
                (p.block.0 - home.0) as f32 * block_length,
                (p.block.1 - home.1) as f32 * block_length,
                0.0,
            ));
            #[allow(clippy::cast_possible_truncation)] // the key's cell word
            let low = ((key >> 16) & 0xFFFF) as u32;
            let cell = u32::try_from(p.block.0 & 0xFF).unwrap_or(0) << 24
                | u32::try_from(p.block.1 & 0xFF).unwrap_or(0) << 16
                | low.max(1);
            out.extend(p.effects.lights(&p.place.then(&shift), cell, now, 1.0));
        }
        out
    }

    /// Queue every object's systems with `systems` to be drawn this frame, run to `now` and lit
    /// with `light`.
    pub fn draw(&mut self, systems: &ParticleSystems, now: f64, light: Color) {
        for p in self.objects.values_mut() {
            match &p.rig {
                Some(rig) => {
                    p.effects
                        .think_on(systems, p.block, &p.place, p.scale, light, now, rig);
                }
                None => p
                    .effects
                    .think(systems, p.block, &p.place, p.scale, light, now),
            }
        }
    }
}

/// An effect playing at a place: its particle systems, each at the place raised by its effect's
/// offset.
#[derive(Debug, Clone, Default)]
pub struct Effect {
    pub systems: Vec<ParticleSet>,
}

impl Effect {
    /// Effect `effect` of table `table` begun at time `now` at `place` (a frame of the scene, the
    /// object's origin and turn) for an object of `scale`, its systems rolling `dice`.
    #[must_use]
    pub fn start(
        records: &WorldRecords,
        table: u32,
        effect: u32,
        place: Mat4,
        scale: Vec3,
        now: f64,
        dice: &Random,
    ) -> Self {
        let mut systems = Vec::new();
        for node in effect_nodes(records, table, effect, 0) {
            let offset = node.bone_offset.unwrap_or(Vec3::ZERO);
            let mut at = place;
            let o = add(place.point(Vec3::ZERO), place.direction(offset));
            at.0[3] = [o.x, o.y, o.z, 1.0];
            for id in node.particles.iter().flatten() {
                if let Some(s) = ParticleSet::load(records, id.0, at, scale, now, dice) {
                    systems.push(s);
                }
            }
        }
        Self { systems }
    }

    /// One particle system begun at `place`, as an effect of its own, rolling `dice`.
    #[must_use]
    pub fn particles(
        records: &WorldRecords,
        id: u32,
        place: Mat4,
        scale: Vec3,
        now: f64,
        dice: &Random,
    ) -> Self {
        Self {
            systems: ParticleSet::load(records, id, place, scale, now, dice)
                .into_iter()
                .collect(),
        }
    }

    /// Light every system's lit particles with `light`.
    pub fn set_light(&mut self, light: Color) {
        for s in &mut self.systems {
            s.set_light(light);
        }
    }

    /// Run every system to `until`, a frame every `frame` seconds.
    pub fn run(&mut self, until: f64, frame: f64) {
        for s in &mut self.systems {
            s.run(until, frame);
        }
    }

    /// What every system draws at `now` from the camera at `eye` with right, forward and up.
    #[must_use]
    pub fn batches(
        &self,
        now: f64,
        eye: Vec3,
        right: Vec3,
        forward: Vec3,
        up: Vec3,
    ) -> Vec<ObjectBatch> {
        self.systems
            .iter()
            .flat_map(|s| s.batches(now, eye, right, forward, up))
            .collect()
    }
}
