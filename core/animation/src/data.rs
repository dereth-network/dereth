//! The decoded inputs the animation layer consumes but does not own.
//!
//! These are plain owned structs: tests and fixtures build them by hand, and the application
//! builds them from the decoded dat records (`dereth_world_data::anim_convert`). Nothing in here
//! parses bytes, and this crate does not depend on the decoders.
//!
//! The small records whose decoded and runtime forms are the same ([`AnimData`], [`LightInfo`],
//! [`LocationEntry`], [`ScriptAndMod`], [`GfxObjInfo`]) and the [`Sphere`] and [`CylSphere`]
//! shapes are `dereth_primitives`', shared with the decoders and physics, and re-exported here. The rest
//! are runtime shapes of their own.
//!
//! Two shapes differ deliberately from the on-disk records:
//!
//! * [`MotionData`] carries `velocity`/`omega` as plain vectors that are zero when the file said
//!   "absent", because the runtime only ever multiplies them by a speed. The **presence mask is
//!   the third header byte and `bitfield` is the second**; the runtime meanings of
//!   `bitfield` — bit 0 "this motion clears all modifiers", bit 1 "only allowed from the style
//!   default" — belong to that second byte, not to the presence mask. See the note on
//!   the motion-data modifier-clear flag.
//! * [`AnimFrame`] holds its hooks as a `Vec` in **file order**, which is what the client's own
//!   hook list produces.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::{DataId, Frame, Vec3};

use crate::command::MotionCommand;
use crate::hooks::AnimHook;

pub use dereth_primitives::records::{
    AnimData, GfxObjInfo, LightInfo, LocationEntry, ScriptAndMod,
};
pub use dereth_primitives::shape::{CylSphere, Sphere};

// ---------------------------------------------------------------------------------------------
// Motion tables (dat 0x09, DB class 0x0E)
// ---------------------------------------------------------------------------------------------

/// `MotionData` — a list of animations plus a constant velocity and omega.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MotionData {
    pub anims: Vec<AnimData>,
    /// The **second** header byte. Both runtime meanings are live: the modifier-clear
    /// and default-substate restriction flags.
    pub bitfield: u8,
    /// Zero when the file carried no velocity.
    pub velocity: Vec3,
    /// Zero when the file carried no omega.
    pub omega: Vec3,
}

impl MotionData {
    /// `md->num_anims`.
    #[must_use]
    pub fn num_anims(&self) -> u32 {
        u32::try_from(self.anims.len()).unwrap_or(u32::MAX)
    }

    /// `bitfield & 1` — "starting this cycle clears all active modifiers".
    ///
    /// Motion decoding reads it at three sites.
    ///
    /// It is easy to read bit 0 as "has velocity (and clears modifiers)" and bit 1 as "has
    /// omega". It is not: the presence mask lives in the *third* header byte
    /// (`docs/formats/19-motion-table.md`), and the counts
    /// measured over all 62,210 shipped `MotionData` records differ — `bitfield & 1` on 1,626
    /// records against 441 with a velocity. The two bytes are independent and this is the second.
    #[must_use]
    pub const fn clears_modifiers(&self) -> bool {
        self.bitfield & 1 != 0
    }

    /// `bitfield & 2` — permits this motion only when the
    /// current substate is the style's default (or is already this motion).
    #[must_use]
    pub const fn restricted_to_default_substate(&self) -> bool {
        self.bitfield & 2 != 0
    }
}

/// Decoded dat type `0x09`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MotionTableData {
    pub default_style: MotionCommand,
    /// Keyed by the **raw** stance id (`0x8000003D`), which is the one table that is not packed.
    pub style_defaults: BTreeMap<u32, MotionCommand>,
    /// Keyed by the combined key `(style << 16) | (motion & 0xFFFFFF)`.
    pub cycles: BTreeMap<u32, MotionData>,
    /// Same key, plus a bare `motion & 0xFFFFFF` fallback the lookup applies.
    pub modifiers: BTreeMap<u32, MotionData>,
    /// Outer: the combined key of `(style, from)`. Inner: the **full** destination command.
    pub links: BTreeMap<u32, BTreeMap<u32, MotionData>>,
}

// ---------------------------------------------------------------------------------------------
// Animations (dat 0x03, DB class 0x08)
// ---------------------------------------------------------------------------------------------

/// `AnimFrame` — one frame's part placements plus its hook list, in file order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimFrame {
    /// One placement frame per part.
    pub frames: Vec<Frame>,
    pub hooks: Vec<AnimHook>,
}

/// Decoded dat type `0x03`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimationData {
    pub num_frames: u32,
    pub num_parts: u32,
    /// The per-frame **object** motion delta ("root motion"), or `None`.
    pub pos_frames: Option<Vec<Frame>>,
    pub part_frames: Vec<AnimFrame>,
    /// Hook presence (`flags & 0x2`). Written on unpack, never read by the client.
    /// Kept because a rebuild may legitimately use it to skip the hook walk.
    pub has_hooks: bool,
}

impl AnimationData {
    /// Bounds-checked against `num_frames`.
    #[must_use]
    pub fn pos_frame(&self, i: i32) -> Option<&Frame> {
        let n = usize::try_from(i).ok()?;
        if u32::try_from(n).ok()? >= self.num_frames {
            return None;
        }
        self.pos_frames.as_ref()?.get(n)
    }

    /// The per-part frame of one sequence node.
    #[must_use]
    pub fn part_frame(&self, i: i32) -> Option<&AnimFrame> {
        let n = usize::try_from(i).ok()?;
        if u32::try_from(n).ok()? >= self.num_frames {
            return None;
        }
        self.part_frames.get(n)
    }
}

// ---------------------------------------------------------------------------------------------
// Setups (dat 0x02, DB class 7) — the animation half
// ---------------------------------------------------------------------------------------------

/// The animation half of a decoded setup. The collision half belongs to physics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetupData {
    /// One gfxobj id per part.
    pub parts: Vec<DataId>,
    /// Keyed by placement id. The placement-frame setter looks up the requested id and falls back
    /// to key **0**.
    pub placement_frames: BTreeMap<u32, AnimFrame>,
    /// Unpacked, never read at runtime. Stored, used by nothing — parts are
    /// placed from the animation frame alone.
    pub parent_index: Option<Vec<u32>>,
    /// Per-part default scale, applied when the setup's parts are built.
    pub default_scale: Option<Vec<Vec3>>,
    pub allow_free_heading: bool,
    pub has_physics_bsp: bool,
    pub height: f32,
    pub radius: f32,
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub sorting_sphere: Sphere,
    pub selection_sphere: Sphere,
    pub spheres: Vec<Sphere>,
    pub cylspheres: Vec<CylSphere>,
    pub lights: BTreeMap<u32, LightInfo>,
    pub holding_locations: BTreeMap<u32, LocationEntry>,
    pub connection_points: BTreeMap<u32, LocationEntry>,
    pub default_animation: Option<DataId>,
    pub default_script: Option<DataId>,
    pub default_motion_table: Option<DataId>,
    pub default_sound_table: Option<DataId>,
    pub default_phs_table: Option<DataId>,
}

// ---------------------------------------------------------------------------------------------
// Physics scripts (dat 0x33 / 0x34)
// ---------------------------------------------------------------------------------------------

/// One physics-script step: a double-precision start time and its animation hook.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptStep {
    pub start_time: f64,
    pub hook: AnimHook,
}

/// Decoded dat type `0x33`, **after** the client's `qsort` (see [`crate::script`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhysicsScriptData {
    pub steps: Vec<ScriptStep>,
    /// Script conversion sets `length = script_data[num-1]->start_time` after sorting.
    pub length: f64,
}

/// Decoded dat type `0x34`. The inner vectors are in **file order**, which is the evaluation
/// order: the lookup returns the first entry whose threshold
/// is **greater than or equal to** the intensity (see [`crate::script::get_script`]
/// for the comparison that settles it) and the client never sorts them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhysicsScriptTableData {
    pub scripts: BTreeMap<u32, Vec<ScriptAndMod>>,
}

// ---------------------------------------------------------------------------------------------
// Degrade info (dat 0x11, DB class 0x1A)
// ---------------------------------------------------------------------------------------------

/// `GfxObjDegradeInfo`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DegradeInfo {
    pub degrades: Vec<GfxObjInfo>,
}

// ---------------------------------------------------------------------------------------------
// Particle emitter info (dat 0x32, DB class 0x2A)
// ---------------------------------------------------------------------------------------------

/// `EmitterType` is tested as **bit flags**, not as an enum, by particle emission.
pub const BIRTHRATE_PER_SEC: i32 = 1;
pub const BIRTHRATE_PER_METER: i32 = 2;

/// `ParticleType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ParticleType {
    Unknown = 0,
    Still = 1,
    LocalVelocity = 2,
    ParabolicLvga = 3,
    ParabolicLvgagr = 4,
    Swarm = 5,
    Explode = 6,
    Implode = 7,
    ParabolicLvla = 8,
    ParabolicLvlalr = 9,
    ParabolicGvga = 10,
    ParabolicGvgagr = 11,
    GlobalVelocity = 12,
}

impl ParticleType {
    #[must_use]
    pub const fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Still,
            2 => Self::LocalVelocity,
            3 => Self::ParabolicLvga,
            4 => Self::ParabolicLvgagr,
            5 => Self::Swarm,
            6 => Self::Explode,
            7 => Self::Implode,
            8 => Self::ParabolicLvla,
            9 => Self::ParabolicLvlalr,
            10 => Self::ParabolicGvga,
            11 => Self::ParabolicGvgagr,
            12 => Self::GlobalVelocity,
            _ => Self::Unknown,
        }
    }
}

/// Decoded dat type `0x32`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleEmitterInfo {
    pub emitter_type: i32,
    pub particle_type: ParticleType,
    pub gfxobj_id: DataId,
    pub hw_gfxobj_id: DataId,
    pub birthrate: f64,
    pub max_particles: i32,
    pub initial_particles: i32,
    pub total_particles: i32,
    pub total_seconds: f64,
    pub lifespan: f64,
    pub lifespan_rand: f64,
    pub offset_dir: Vec3,
    pub min_offset: f32,
    pub max_offset: f32,
    pub a: Vec3,
    pub min_a: f32,
    pub max_a: f32,
    pub b: Vec3,
    pub min_b: f32,
    pub max_b: f32,
    pub c: Vec3,
    pub min_c: f32,
    pub max_c: f32,
    pub start_scale: f32,
    pub final_scale: f32,
    pub scale_rand: f32,
    pub start_trans: f32,
    pub final_trans: f32,
    pub trans_rand: f32,
    pub is_parent_local: i32,
    /// `radius = max(max_offset, max_a * lifespan)`,
    /// centre at the origin. Recomputed here, not read from the file.
    pub sorting_sphere: Sphere,
}

impl Default for ParticleEmitterInfo {
    /// Constructor : everything zero except the six range maxima and the two scales,
    /// which are 1.0.
    fn default() -> Self {
        Self {
            emitter_type: 0,
            particle_type: ParticleType::Unknown,
            gfxobj_id: DataId(0),
            hw_gfxobj_id: DataId(0),
            birthrate: 0.0,
            max_particles: 0,
            initial_particles: 0,
            total_particles: 0,
            total_seconds: 0.0,
            lifespan: 0.0,
            lifespan_rand: 0.0,
            offset_dir: Vec3::ZERO,
            min_offset: 0.0,
            max_offset: 0.0,
            a: Vec3::ZERO,
            min_a: 1.0,
            max_a: 1.0,
            b: Vec3::ZERO,
            min_b: 1.0,
            max_b: 1.0,
            c: Vec3::ZERO,
            min_c: 1.0,
            max_c: 1.0,
            start_scale: 1.0,
            final_scale: 1.0,
            scale_rand: 0.0,
            start_trans: 0.0,
            final_trans: 0.0,
            trans_rand: 0.0,
            is_parent_local: 0,
            sorting_sphere: Sphere {
                center: Vec3::ZERO,
                radius: 0.0,
            },
        }
    }
}

impl ParticleEmitterInfo {
    /// The emitter's end-of-init step.
    ///
    /// The radius uses only the `a` (velocity) term, so a fast parabolic emitter can outrun its own
    /// sorting sphere — visible as pop-in on the original client too.
    pub fn init_end(&mut self) {
        #[allow(clippy::cast_possible_truncation)]
        let reach = (f64::from(self.max_a) * self.lifespan) as f32;
        self.sorting_sphere = Sphere {
            center: Vec3::ZERO,
            radius: self.max_offset.max(reach),
        };
    }
}

// ---------------------------------------------------------------------------------------------
// The asset seam
// ---------------------------------------------------------------------------------------------

/// Everything this track needs to resolve a `DataId` to a decoded asset.
///
/// Loads may be deferred, so the `Option` is a real state and not an error: a missing animation is
/// silently dropped ( deletes the node), which is the
/// client's own behaviour.
pub trait AnimAssets: Send + Sync {
    fn motion_table(&self, id: DataId) -> Option<Arc<MotionTableData>>;
    fn animation(&self, id: DataId) -> Option<Arc<AnimationData>>;
    fn setup(&self, id: DataId) -> Option<Arc<SetupData>>;
    fn script(&self, id: DataId) -> Option<Arc<PhysicsScriptData>>;
    fn script_table(&self, id: DataId) -> Option<Arc<PhysicsScriptTableData>>;
    fn emitter_info(&self, id: DataId) -> Option<Arc<ParticleEmitterInfo>>;
    fn degrade_info(&self, id: DataId) -> Option<Arc<DegradeInfo>>;

    /// Fetch graphics-object type 6 by id — does the dat hold this object, and which
    /// `GfxObjDegradeInfo` does it name?
    ///
    /// This crate draws nothing, so it wants no geometry; it needs the *two* facts that part
    /// loading uses — whether the object exists at all, and
    /// its `did_degrade` — because both decide whether a part swap succeeds and which LOD table
    /// the part carries afterwards.
    ///
    /// **Defaulted to [`GfxObjLookup::Unknown`] deliberately, and that is a third state rather
    /// than a permissive default.** An `AnimAssets` with no graphics-object index cannot answer, and
    /// "cannot answer" must not read as "the dat does not hold it" (which would fail every swap)
    /// or as "the dat holds it" (which would be a guess dressed as a measurement).
    /// [`PhysicsPart::set_part`](crate::parts::PhysicsPart::set_part) distinguishes all three.
    fn gfxobj(&self, _id: DataId) -> GfxObjLookup {
        GfxObjLookup::Unknown
    }
}

/// The answer to [`AnimAssets::gfxobj`] — three states, never two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GfxObjLookup {
    /// The graphics-object lookup returned an object. `did_degrade` is the `GfxObjDegradeInfo` id it names, or
    /// `None` when the graphics object's flags bit 3 is clear.
    Present { did_degrade: Option<DataId> },
    /// The graphics-object lookup returned NULL, so part initialization returns 0 at its first test
    /// without touching the part.
    Absent,
    /// This asset source carries no graphics-object index and was not able to look. Not a negative.
    Unknown,
}

/// An `AnimAssets` that knows nothing. Used by tests that drive the player with animations they
/// have built by hand, and by objects whose dat is not loaded yet.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoAssets;

impl AnimAssets for NoAssets {
    fn motion_table(&self, _: DataId) -> Option<Arc<MotionTableData>> {
        None
    }
    fn animation(&self, _: DataId) -> Option<Arc<AnimationData>> {
        None
    }
    fn setup(&self, _: DataId) -> Option<Arc<SetupData>> {
        None
    }
    fn script(&self, _: DataId) -> Option<Arc<PhysicsScriptData>> {
        None
    }
    fn script_table(&self, _: DataId) -> Option<Arc<PhysicsScriptTableData>> {
        None
    }
    fn emitter_info(&self, _: DataId) -> Option<Arc<ParticleEmitterInfo>> {
        None
    }
    fn degrade_info(&self, _: DataId) -> Option<Arc<DegradeInfo>> {
        None
    }
    /// Not `Absent`: an `AnimAssets` that knows nothing has not looked, and a part swap driven by
    /// a hand-built fixture must not start failing because the fixture has no dat behind it.
    fn gfxobj(&self, _: DataId) -> GfxObjLookup {
        GfxObjLookup::Unknown
    }
}

/// A `BTreeMap`-backed `AnimAssets` for tests and for a preloaded object.
#[derive(Debug, Default)]
pub struct MapAssets {
    pub motion_tables: BTreeMap<u32, Arc<MotionTableData>>,
    pub animations: BTreeMap<u32, Arc<AnimationData>>,
    pub setups: BTreeMap<u32, Arc<SetupData>>,
    pub scripts: BTreeMap<u32, Arc<PhysicsScriptData>>,
    pub script_tables: BTreeMap<u32, Arc<PhysicsScriptTableData>>,
    pub emitters: BTreeMap<u32, Arc<ParticleEmitterInfo>>,
    pub degrades: BTreeMap<u32, Arc<DegradeInfo>>,
    /// The graphics-object index, or `None` when this fixture has not been given one. `None` answers
    /// [`GfxObjLookup::Unknown`] for every id; `Some(map)` is **authoritative**, so an id the map
    /// does not hold answers [`GfxObjLookup::Absent`] and a part swap naming it fails.
    pub gfxobjs: Option<BTreeMap<u32, Option<DataId>>>,
}

impl AnimAssets for MapAssets {
    fn motion_table(&self, id: DataId) -> Option<Arc<MotionTableData>> {
        self.motion_tables.get(&id.0).cloned()
    }
    fn animation(&self, id: DataId) -> Option<Arc<AnimationData>> {
        self.animations.get(&id.0).cloned()
    }
    fn setup(&self, id: DataId) -> Option<Arc<SetupData>> {
        self.setups.get(&id.0).cloned()
    }
    fn script(&self, id: DataId) -> Option<Arc<PhysicsScriptData>> {
        self.scripts.get(&id.0).cloned()
    }
    fn script_table(&self, id: DataId) -> Option<Arc<PhysicsScriptTableData>> {
        self.script_tables.get(&id.0).cloned()
    }
    fn emitter_info(&self, id: DataId) -> Option<Arc<ParticleEmitterInfo>> {
        self.emitters.get(&id.0).cloned()
    }
    fn degrade_info(&self, id: DataId) -> Option<Arc<DegradeInfo>> {
        self.degrades.get(&id.0).cloned()
    }
    fn gfxobj(&self, id: DataId) -> GfxObjLookup {
        match &self.gfxobjs {
            None => GfxObjLookup::Unknown,
            Some(map) => match map.get(&id.0) {
                Some(did) => GfxObjLookup::Present { did_degrade: *did },
                None => GfxObjLookup::Absent,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ORACLE: `docs/formats/19-motion-table.md`, the `bitfield` table, whose counts
    /// were measured over all 62,210 shipped `MotionData` records.
    #[test]
    fn the_bitfield_carries_both_runtime_meanings() {
        let md = MotionData {
            bitfield: 1,
            ..MotionData::default()
        };
        assert!(md.clears_modifiers());
        assert!(!md.restricted_to_default_substate());
        let md = MotionData {
            bitfield: 2,
            ..MotionData::default()
        };
        assert!(!md.clears_modifiers());
        assert!(md.restricted_to_default_substate());
        let md = MotionData {
            bitfield: 3,
            ..MotionData::default()
        };
        assert!(md.clears_modifiers() && md.restricted_to_default_substate());
    }

    /// `InitEnd`'s sorting radius is `max(max_offset, max_a * lifespan)`.
    #[test]
    fn emitter_init_end_sets_the_sorting_radius() {
        let mut i = ParticleEmitterInfo {
            max_offset: 0.5,
            max_a: 3.0,
            lifespan: 2.0,
            ..ParticleEmitterInfo::default()
        };
        i.init_end();
        assert_eq!(i.sorting_sphere.radius, 6.0);
        let mut i = ParticleEmitterInfo {
            max_offset: 9.0,
            max_a: 3.0,
            lifespan: 2.0,
            ..ParticleEmitterInfo::default()
        };
        i.init_end();
        assert_eq!(i.sorting_sphere.radius, 9.0);
    }

    /// The bounds checks in `get_pos_frame`/`get_part_frame` are against `num_frames`, and a
    /// negative index returns `None` rather than wrapping.
    #[test]
    fn frame_accessors_are_bounds_checked() {
        let a = AnimationData {
            num_frames: 2,
            num_parts: 1,
            pos_frames: Some(vec![Frame::default(), Frame::default()]),
            part_frames: vec![AnimFrame::default(), AnimFrame::default()],
            has_hooks: false,
        };
        assert!(a.pos_frame(0).is_some());
        assert!(a.pos_frame(1).is_some());
        assert!(a.pos_frame(2).is_none());
        assert!(a.pos_frame(-1).is_none());
        assert!(a.part_frame(2).is_none());
    }
}
