//! Effects: effect tables (`.fxtable`), effect scripts (`.fxscript`), camera effects (`.cfx`) and
//! particle systems (`.psdesc`), and the effect description they and the visual descriptions share.
//!
//! An effect ([`FxNode`]) is a 64-bit word saying which of some forty optional parts follow:
//! sounds, music, particle systems, a camera effect, an appearance to apply, a streak, a decal, a
//! light, a repulsor, a color, timing and attachment settings. An effect table maps an effect id to
//! the effects it plays, each with per-terrain variants.

use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader};
use auberean_primitives::{DataId, Rgba, Vec3};

use crate::common::{Color32, Waveform};
use crate::entity::Entity;
use crate::{Context, Decode};

/// Three waveforms, one per axis.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WaveformVec3 {
    pub x: Waveform,
    pub y: Waveform,
    pub z: Waveform,
}

impl WaveformVec3 {
    // D2-06941
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            x: Waveform::read(r)?,
            y: Waveform::read(r)?,
            z: Waveform::read(r)?,
        })
    }
}

/// A streak drawn behind a moving part.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Streak {
    pub color: Color32,
    pub start: Vec3,
    pub end: Vec3,
    pub material: DataId,
    pub lifespan: f32,
    pub trail_duration: f32,
    pub min_segment_time: f32,
    pub min_segment_distance: f32,
    pub texture_scale: f32,
}

impl Streak {
    // D2-15755
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            color: Color32::read(r)?,
            start: r.vec3()?,
            end: r.vec3()?,
            material: r.data_id()?,
            lifespan: r.f32()?,
            trail_duration: r.f32()?,
            min_segment_time: r.f32()?,
            min_segment_distance: r.f32()?,
            texture_scale: r.f32()?,
        })
    }
}

/// A decal laid on surfaces.
// D2-15756
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Decal {
    pub color: Color32,
    pub material: DataId,
    pub origin: Vec3,
    pub lifespan: f32,
    pub size: Waveform,
    pub rotation: Waveform,
    pub position_radius: Waveform,
}

impl Decal {
    /// The color comes first, then the material, origin and lifespan, then three waveforms.
    // D2-15758
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            color: Color32::read(r)?,
            material: r.data_id()?,
            origin: r.vec3()?,
            lifespan: r.f32()?,
            size: Waveform::read(r)?,
            rotation: Waveform::read(r)?,
            position_radius: Waveform::read(r)?,
        })
    }
}

/// A light the effect places: an entity describing it, how long it lives and how long it fades.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EffectLight {
    pub entity: Entity,
    pub lifespan: f32,
    pub fade_time: f32,
}

impl EffectLight {
    // D2-15761
    fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            entity: Entity::read(r, ctx)?,
            lifespan: r.f32()?,
            fade_time: r.f32()?,
        })
    }
}

/// A push (or pull) on what is near.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Repulsor {
    pub acceleration: f32,
    pub range: f32,
    pub lifetime: f32,
    pub attracts: bool,
    pub min_velocity: f32,
}

impl Repulsor {
    /// Acceleration, range, lifetime, the attract flag, then the minimum velocity.
    // D2-15763
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            acceleration: r.f32()?,
            range: r.f32()?,
            lifetime: r.f32()?,
            attracts: r.bool32()?,
            min_velocity: r.f32()?,
        })
    }
}

/// An appearance key applied by an effect, with its strength.
// D2-13962, D2-13966, D2-13963, D2-03201, D2-00015, D2-00019, D2-13965
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AppearanceKey {
    pub key: u32,
    pub scalar: f32,
}

impl AppearanceKey {
    /// Two applied keys are the same key whatever their strengths.
    // D2-13964
    #[must_use]
    pub fn same_key(&self, other: &Self) -> bool {
        self.key == other.key
    }

    // D2-14971
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            key: r.u32()?,
            scalar: r.f32()?,
        })
    }
}

/// One effect. Every part is optional; [`FxNode::parts`] says which are stored.
// D2-15723
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxNode {
    /// Which parts are stored, a bit each.
    pub parts: u64,
    pub sounds: Option<Vec<DataId>>,
    pub music: Option<Vec<DataId>>,
    pub particles: Option<Vec<DataId>>,
    pub camera_fx: Option<DataId>,
    pub appearance: Option<DataId>,
    pub appearance_key: Option<AppearanceKey>,
    pub ramp_time: Option<f32>,
    pub intensity: Option<f32>,
    pub holding_location: Option<u32>,
    pub bone: Option<u32>,
    pub bone_offset: Option<Vec3>,
    /// Whether the streak is on, and the streak.
    pub streak: Option<(bool, Streak)>,
    pub start_streak: Option<bool>,
    pub stop_streak: Option<bool>,
    pub slider: Option<u32>,
    pub restore: Option<bool>,
    /// Whether the color is on, and the color.
    pub color: Option<(bool, Rgba)>,
    pub property_color: Option<bool>,
    pub visual_desc: Option<DataId>,
    pub lifetime: Option<f32>,
    pub color_lifetime: Option<f32>,
    pub behavior: Option<u32>,
    pub parent: Option<bool>,
    pub unparent: Option<bool>,
    pub retarget: Option<bool>,
    pub destroy_target: Option<bool>,
    pub hide_target: Option<bool>,
    pub target_fx: Option<u32>,
    pub day_time: Option<bool>,
    pub night_time: Option<bool>,
    pub channel_keys: Option<Vec<(u32, u32)>>,
    pub fire_and_forget: Option<bool>,
    pub water_plane: Option<bool>,
    pub conditional: Option<bool>,
    pub decal: Option<(bool, Decal)>,
    pub repulsor: Option<(bool, Repulsor)>,
    pub light: Option<(bool, EffectLight)>,
    pub player_only: Option<bool>,
    pub fx_script: Option<DataId>,
    pub remove_rotation: Option<bool>,
}

/// The parts word's bits this layout knows (bits 0 to 39).
const KNOWN_PARTS: u64 = (1 << 40) - 1;

impl FxNode {
    /// The 64-bit parts word, then each stored part in the stored order (which is not bit order).
    // D2-15724
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let parts = r.u64()?;
        r.ensure(parts & !KNOWN_PARTS == 0, || {
            format!("effect parts {parts:#x}")
        })?;
        let has = |bit: u32| parts & (1u64 << bit) != 0;
        let mut n = Self {
            parts,
            ..Self::default()
        };
        if has(0) {
            n.sounds = Some(r.list(4, Reader::data_id)?);
        }
        if has(39) {
            n.music = Some(r.list(4, Reader::data_id)?);
        }
        if has(1) {
            n.particles = Some(r.list(4, Reader::data_id)?);
        }
        if has(2) {
            n.camera_fx = Some(r.data_id()?);
        }
        if has(3) {
            n.appearance = Some(r.data_id()?);
        }
        if has(4) {
            n.appearance_key = Some(AppearanceKey::read(r)?);
        }
        if has(5) {
            n.ramp_time = Some(r.f32()?);
        }
        if has(6) {
            n.intensity = Some(r.f32()?);
        }
        if has(7) {
            n.holding_location = Some(r.u32()?);
        }
        if has(8) {
            n.bone = Some(r.u32()?);
        }
        if has(9) {
            n.bone_offset = Some(r.vec3()?);
        }
        if has(10) {
            n.streak = Some((r.bool32()?, Streak::read(r)?));
        }
        if has(11) {
            n.start_streak = Some(r.bool32()?);
        }
        if has(12) {
            n.stop_streak = Some(r.bool32()?);
        }
        if has(13) {
            n.slider = Some(r.u32()?);
        }
        if has(14) {
            n.restore = Some(r.bool32()?);
        }
        if has(15) {
            n.color = Some((r.bool32()?, r.rgba()?));
        }
        if has(16) {
            n.property_color = Some(r.bool32()?);
        }
        if has(17) {
            n.visual_desc = Some(r.data_id()?);
        }
        if has(18) {
            n.lifetime = Some(r.f32()?);
        }
        if has(37) {
            n.color_lifetime = Some(r.f32()?);
        }
        if has(19) {
            n.behavior = Some(r.u32()?);
        }
        if has(20) {
            n.parent = Some(r.bool32()?);
        }
        if has(21) {
            n.unparent = Some(r.bool32()?);
        }
        if has(22) {
            n.retarget = Some(r.bool32()?);
        }
        if has(23) {
            n.destroy_target = Some(r.bool32()?);
        }
        if has(35) {
            n.hide_target = Some(r.bool32()?);
        }
        if has(24) {
            n.target_fx = Some(r.u32()?);
        }
        if has(25) {
            n.day_time = Some(r.bool32()?);
        }
        if has(26) {
            n.night_time = Some(r.bool32()?);
        }
        if has(27) {
            n.channel_keys = Some(r.hash_table(8, |r| Ok((r.u32()?, r.u32()?)))?);
        }
        if has(28) {
            n.fire_and_forget = Some(r.bool32()?);
        }
        if has(29) {
            n.water_plane = Some(r.bool32()?);
        }
        if has(30) {
            n.conditional = Some(r.bool32()?);
        }
        if has(31) {
            n.decal = Some((r.bool32()?, Decal::read(r)?));
        }
        if has(33) {
            n.repulsor = Some((r.bool32()?, Repulsor::read(r)?));
        }
        if has(32) {
            n.light = Some((r.bool32()?, EffectLight::read(r, ctx)?));
        }
        if has(34) {
            n.player_only = Some(r.bool32()?);
        }
        if has(36) {
            n.fx_script = Some(r.data_id()?);
        }
        if has(38) {
            n.remove_rotation = Some(r.bool32()?);
        }
        Ok(n)
    }
}

/// An effect and its variants per terrain.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxData {
    pub default: FxNode,
    /// Per terrain kind, the effect played there instead.
    pub by_terrain: Vec<(u32, FxNode)>,
}

impl FxData {
    // D2-15717, D2-00135
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let default = FxNode::read(r, ctx)?;
        let by_terrain = r.hash_table(12, |r| Ok((r.u32()?, FxNode::read(r, ctx)?)))?;
        Ok(Self {
            default,
            by_terrain,
        })
    }

    /// A counted list of effects.
    // D2-13856
    pub fn read_list(r: &mut Reader<'_>, ctx: &Context) -> Result<Vec<Self>> {
        r.list(12, |r| Self::read(r, ctx))
    }
}

/// An effect table (`.fxtable`): per effect id, the effects it plays, and the tables it extends.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxTable {
    pub effects: Vec<(u32, Vec<FxData>)>,
    pub sub_tables: Vec<DataId>,
}

impl Decode for FxTable {
    /// The id, a hash table from effect id to a counted list of effects, then a counted list of
    /// table ids.
    // D2-15744, D2-14055
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let effects = r.hash_table(8, |r| Ok((r.u32()?, FxData::read_list(r, ctx)?)))?;
        let sub_tables = r.list(4, Reader::data_id)?;
        Ok(Self {
            effects,
            sub_tables,
        })
    }
}

/// A timed trigger of an effect script.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Trigger {
    pub name: u32,
    pub time: f32,
    pub stop: bool,
    pub probability: f32,
    pub intensity: f32,
    pub intensity_variance: f32,
    pub play: bool,
}

impl Trigger {
    /// Name, time, the stop flag, probability, intensity, its variance, the play flag.
    // D2-15222, D2-05147
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r.u32()?,
            time: r.f32()?,
            stop: r.bool32()?,
            probability: r.f32()?,
            intensity: r.f32()?,
            intensity_variance: r.f32()?,
            play: r.bool32()?,
        })
    }
}

/// A timed push of an effect script: a jump, or an impulse vector.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ImpulseTrigger {
    pub jump: bool,
    /// The impulse, stored only when this is not a jump.
    pub impulse: Option<Vec3>,
    pub number: u32,
    pub time: f32,
}

impl ImpulseTrigger {
    /// The jump flag, the impulse when not a jump, the impulse number, the time.
    // D2-15150, D2-05150
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let jump = r.bool32()?;
        let impulse = if jump { None } else { Some(r.vec3()?) };
        Ok(Self {
            jump,
            impulse,
            number: r.u32()?,
            time: r.f32()?,
        })
    }
}

/// An effect script (`.fxscript`): effect triggers and impulse triggers over a period.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxScript {
    pub triggers: Vec<Trigger>,
    pub impulses: Vec<ImpulseTrigger>,
    pub period: f32,
}

// D2-05120, D2-05133, D2-05134, D2-15125
impl Decode for FxScript {
    /// The id, a counted list of triggers, a counted list of impulse triggers, the period.
    // D2-15124
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let triggers = r.list(28, Trigger::read)?;
        let impulses = r.list(12, ImpulseTrigger::read)?;
        let period = r.f32()?;
        Ok(Self {
            triggers,
            impulses,
            period,
        })
    }
}

/// A camera effect (`.cfx`): a 64-bit word saying which settings follow, then the settings.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CameraFx {
    pub parts: u64,
    pub shake: Option<bool>,
    pub look_at: Option<bool>,
    pub free_camera: Option<bool>,
    pub parent_camera: Option<bool>,
    pub target_camera: Option<bool>,
    pub restore_camera: Option<bool>,
    pub intensity: Option<f32>,
    pub period: Option<f32>,
    pub ramp_up: Option<f32>,
    pub ramp_down: Option<f32>,
    pub blur: Option<bool>,
    pub field_of_view: Option<bool>,
    /// Whether the screen flashes, and the flash color.
    pub screen_flash: Option<(bool, Color32)>,
    pub player_only: Option<bool>,
    pub camera_distance: Option<bool>,
}

impl Decode for CameraFx {
    /// The id, the parts word, then each stored setting in the stored order.
    // D2-10730
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let parts = r.u64()?;
        r.ensure(parts & !0x7FFF == 0, || {
            format!("camera effect parts {parts:#x}")
        })?;
        let has = |bit: u64| parts & bit != 0;
        let mut c = Self {
            parts,
            ..Self::default()
        };
        if has(0x1) {
            c.shake = Some(r.bool32()?);
        }
        if has(0x2) {
            c.look_at = Some(r.bool32()?);
        }
        if has(0x4) {
            c.free_camera = Some(r.bool32()?);
        }
        if has(0x8) {
            c.parent_camera = Some(r.bool32()?);
        }
        if has(0x10) {
            c.target_camera = Some(r.bool32()?);
        }
        if has(0x20) {
            c.restore_camera = Some(r.bool32()?);
        }
        if has(0x40) {
            c.intensity = Some(r.f32()?);
        }
        if has(0x80) {
            c.period = Some(r.f32()?);
        }
        if has(0x100) {
            c.ramp_up = Some(r.f32()?);
        }
        if has(0x2000) {
            c.ramp_down = Some(r.f32()?);
        }
        if has(0x200) {
            c.blur = Some(r.bool32()?);
        }
        if has(0x1000) {
            c.field_of_view = Some(r.bool32()?);
        }
        if has(0x400) {
            c.screen_flash = Some((r.bool32()?, Color32::read(r)?));
        }
        if has(0x800) {
            c.player_only = Some(r.bool32()?);
        }
        if has(0x4000) {
            c.camera_distance = Some(r.bool32()?);
        }
        Ok(c)
    }
}

/// A key of a particle emitter's life: at a time, the particle's scale, color, mass and shape,
/// and a path of points.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EmitterKeyframe {
    pub time: f32,
    pub key_flags: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub color: Rgba,
    pub mass: f32,
    pub particle_kind: u32,
    pub keyframe_flags: u32,
    pub position: WaveformVec3,
    pub points: Vec<Vec3>,
}

impl EmitterKeyframe {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let k = Self {
            time: r.f32()?,
            key_flags: r.u32()?,
            scale_x: r.f32()?,
            scale_y: r.f32()?,
            color: r.rgba()?,
            mass: r.f32()?,
            particle_kind: r.u32()?,
            keyframe_flags: r.u32()?,
            position: WaveformVec3::read(r)?,
            points: r.list(12, Reader::vec3)?,
        };
        r.align()?;
        Ok(k)
    }
}

/// One emitter of a particle system.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Emitter {
    pub max_particles: u32,
    pub origin: WaveformVec3,
    pub shape: u32,
    pub particle_shape: u32,
    pub scale: Waveform,
    pub particle_scale: Waveform,
    pub exploding: bool,
    pub inclusive_shape: bool,
    pub active: bool,
    pub birth_rate: f32,
    pub lifespan: Waveform,
    pub velocity: Waveform,
    pub streak: Waveform,
    pub rotation: WaveformVec3,
    pub world_rotation: WaveformVec3,
    pub rotate_velocity: WaveformVec3,
    pub direction: WaveformVec3,
    pub min_spread: Waveform,
    pub max_spread: Waveform,
    pub emission_limit: u32,
    pub blast_count: u32,
    pub start_time: f32,
    pub time_limit: f32,
    pub emission_distance: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub particle_snap: Waveform,
    pub constrain_axes: [bool; 3],
    pub keyframes: Vec<EmitterKeyframe>,
}

impl Emitter {
    /// Every setting in the stored order, then a counted list of keyframes.
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            max_particles: r.u32()?,
            origin: WaveformVec3::read(r)?,
            shape: r.u32()?,
            particle_shape: r.u32()?,
            scale: Waveform::read(r)?,
            particle_scale: Waveform::read(r)?,
            exploding: r.bool32()?,
            inclusive_shape: r.bool32()?,
            active: r.bool32()?,
            birth_rate: r.f32()?,
            lifespan: Waveform::read(r)?,
            velocity: Waveform::read(r)?,
            streak: Waveform::read(r)?,
            rotation: WaveformVec3::read(r)?,
            world_rotation: WaveformVec3::read(r)?,
            rotate_velocity: WaveformVec3::read(r)?,
            direction: WaveformVec3::read(r)?,
            min_spread: Waveform::read(r)?,
            max_spread: Waveform::read(r)?,
            emission_limit: r.u32()?,
            blast_count: r.u32()?,
            start_time: r.f32()?,
            time_limit: r.f32()?,
            emission_distance: r.f32()?,
            fade_in: r.f32()?,
            fade_out: r.f32()?,
            particle_snap: Waveform::read(r)?,
            constrain_axes: [r.bool32()?, r.bool32()?, r.bool32()?],
            keyframes: r.list(180, EmitterKeyframe::read)?,
        })
    }
}

/// A particle system (`.psdesc`): limits and timing, its emitters, and the material it draws with.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Particles {
    pub max_particles: u32,
    pub scale_type: u32,
    pub physics_time_step: f32,
    pub fast_forward_time: f32,
    pub physics_duration: f32,
    pub start_fade_distance: f32,
    pub stop_fade_distance: f32,
    pub world_space: bool,
    pub force_draw: bool,
    pub emitters: Vec<Emitter>,
    pub material: DataId,
}

impl Decode for Particles {
    /// The id, the system's settings, a counted list of emitters, the material.
    // D2-06968
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            max_particles: r.u32()?,
            scale_type: r.u32()?,
            physics_time_step: r.f32()?,
            fast_forward_time: r.f32()?,
            physics_duration: r.f32()?,
            start_fade_distance: r.f32()?,
            stop_fade_distance: r.f32()?,
            world_space: r.bool32()?,
            force_draw: r.bool32()?,
            emitters: r.list(1000, Emitter::read)?,
            material: r.data_id()?,
        })
    }
}

// ------------------------------------------------------------------ the records an effect names

/// The gathering that leaves out the records effects name.
pub const GATHER_WITHOUT_EFFECTS: u32 = 0x4000_0009;

impl Streak {
    /// The material it draws with.
    // D2-15753
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.material.0);
    }
}

impl Decal {
    /// The material it draws with.
    // D2-12473
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.material.0);
    }
}

impl FxNode {
    /// Its sounds, music and particle systems, its camera effect, appearance table, visual
    /// description and effect script, and the material of the streak and of the decal it carries.
    // D2-15716
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for list in [&self.sounds, &self.music, &self.particles] {
            for id in list.iter().flatten() {
                out.add(id.0);
            }
        }
        for id in [
            self.camera_fx,
            self.appearance,
            self.visual_desc,
            self.fx_script,
        ]
        .into_iter()
        .flatten()
        {
            out.add(id.0);
        }
        if let Some((true, s)) = &self.streak {
            s.sub_ids(out);
        }
        if let Some((true, d)) = &self.decal {
            d.sub_ids(out);
        }
    }
}

impl FxData {
    /// The default effect's records, then each terrain's.
    // D2-15725
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        self.default.sub_ids(out);
        for (_, n) in &self.by_terrain {
            n.sub_ids(out);
        }
    }
}

/// The records a table of effects by id names: none for [`GATHER_WITHOUT_EFFECTS`].
// D2-15726
pub fn fx_map_sub_ids(map: &[(u32, Vec<FxData>)], gathering: u32, out: &mut QualifiedIds) {
    if gathering == GATHER_WITHOUT_EFFECTS {
        return;
    }
    for (_, list) in map {
        for d in list {
            d.sub_ids(out);
        }
    }
}

impl FxTable {
    /// Its effects' records, then the tables it extends.
    // D2-15742
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        fx_map_sub_ids(&self.effects, gathering, out);
        for t in &self.sub_tables {
            out.add(t.0);
        }
    }
}
