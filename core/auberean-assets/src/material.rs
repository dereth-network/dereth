//! Materials: how a surface is drawn (`.mat`, `0x2B000000`+), modifications of a material
//! (`.mm`, `0x30000000`+) and the instances meshes name (`.mi`, `0x31000000`+).
//!
//! A material is a set of named parameters (a modifier's parameters) and up to a few drawing
//! layers; each layer has its blend, depth, cull and alpha-test state, its lighting switches and
//! colors, up to four texture stages, texture-coordinate animations, and optionally its own
//! compiled vertex and pixel shaders. A modifier is only the named parameters; an instance names a
//! material, its kind, and the modifiers applied to it.

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::{DataId, Rgba};

use crate::common::Waveform;
use crate::{Context, Decode};

/// One field of a material parameter: four words saying which part of the value feeds what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ParameterField {
    pub words: [u32; 4],
}

/// A named material parameter: its name, its data type and its data, and its fields.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MaterialParameter {
    /// The parameter's name, as a name id.
    pub name: u32,
    pub data_type: u32,
    pub data: Vec<u8>,
    pub fields: Vec<ParameterField>,
}

impl MaterialParameter {
    /// The name id, the data type, a byte count (never zero) and that many bytes, then a counted
    /// list of four-word fields.
    // D2-10167
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let name = r.u32()?;
        let data_type = r.u32()?;
        let n = r.count(1)?;
        r.ensure(n != 0, || "a parameter with no data".into())?;
        let data = r.bytes(n)?.to_vec();
        let fields = r.list(16, |r| {
            Ok(ParameterField {
                words: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            })
        })?;
        Ok(Self {
            name,
            data_type,
            data,
            fields,
        })
    }
}

/// Named parameters that modify a material (`.mm`); also the parameters a material carries.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MaterialModifier {
    /// The modifier's own id (a material's embedded parameters carry an id word of their own).
    pub id: u32,
    pub parameters: Vec<MaterialParameter>,
}

impl MaterialModifier {
    /// The id, then a counted list of parameters.
    // D2-10146
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let parameters = r.list(16, MaterialParameter::read)?;
        Ok(Self { id, parameters })
    }
}

impl Decode for MaterialModifier {
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let m = Self::read(r)?;
        if m.id != id {
            return Err(auberean_dat::DecodeError {
                at: 0,
                what: format!("id {:08X}, filed as {id:08X}", m.id),
            });
        }
        Ok(m)
    }
}

/// One texture stage of a layer: the texture, how its color and alpha combine, how it is
/// addressed and filtered, and which texture coordinates it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextureStage {
    pub texture: DataId,
    pub color_op: u32,
    pub color_arg1: u32,
    pub color_arg2: u32,
    pub alpha_op: u32,
    pub alpha_arg1: u32,
    pub alpha_arg2: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub min_filter: u32,
    pub mag_filter: u32,
    pub tex_coord: u32,
    pub projected: bool,
    /// A texture the renderer supplies (0 for none).
    pub special: u32,
}

impl TextureStage {
    // D2-10085
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            texture: r.data_id()?,
            color_op: r.u32()?,
            color_arg1: r.u32()?,
            color_arg2: r.u32()?,
            alpha_op: r.u32()?,
            alpha_arg1: r.u32()?,
            alpha_arg2: r.u32()?,
            address_u: r.u32()?,
            address_v: r.u32()?,
            min_filter: r.u32()?,
            mag_filter: r.u32()?,
            tex_coord: r.u32()?,
            projected: r.bool32()?,
            special: r.u32()?,
        })
    }
}

/// How a layer animates one stage's texture coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum CoordinateAnimation {
    /// Kind 4: slide by a waveform in u and in v.
    Translate { u: Waveform, v: Waveform },
    /// Kind 5: turn by a waveform.
    Rotate(Waveform),
    /// Kind 6: stretch by a waveform in u and in v.
    Scale { u: Waveform, v: Waveform },
    /// Kind 7: a fixed 4 x 4 matrix, row by row.
    Transform([f32; 16]),
}

/// A texture-coordinate animation and the stage it applies to.
#[derive(Clone, Debug, PartialEq)]
pub struct StageAnimation {
    pub stage: u32,
    pub animation: CoordinateAnimation,
}

impl StageAnimation {
    /// The kind word, the stage index, then the kind's values.
    // D2-10071, D2-10073, D2-10076, D2-10069
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        let stage = r.u32()?;
        let animation = match kind {
            4 => CoordinateAnimation::Translate {
                u: Waveform::read(r)?,
                v: Waveform::read(r)?,
            },
            5 => CoordinateAnimation::Rotate(Waveform::read(r)?),
            6 => CoordinateAnimation::Scale {
                u: Waveform::read(r)?,
                v: Waveform::read(r)?,
            },
            7 => {
                let mut m = [0f32; 16];
                for x in &mut m {
                    *x = r.f32()?;
                }
                CoordinateAnimation::Transform(m)
            }
            _ => return r.fail(format!("texture-coordinate animation of kind {kind}")),
        };
        Ok(Self { stage, animation })
    }
}

/// The lighting and fog switches of a layer, in stored order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LayerSwitches {
    pub particle_lighting: bool,
    pub dynamic_lighting: bool,
    pub pixel_lighting: bool,
    pub ignore_directional_lights: bool,
    pub ignore_point_lights: bool,
    pub specular_highlights: bool,
    pub pixel_specular: bool,
    pub material_diffuse: bool,
    pub vertex_diffuse: bool,
    pub modulate_vertex_diffuse: bool,
    pub pixel_multiple_lights: bool,
    pub depth_fog: bool,
    pub depth_alpha_fog: bool,
    pub diffuse_fog_amount: bool,
}

/// One drawing layer of a material.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MaterialLayer {
    /// Render flags that must be set, and that must be clear, for the layer to draw.
    pub flags_set: u32,
    pub flags_clear: u32,
    pub stages: Vec<TextureStage>,
    pub animations: Vec<StageAnimation>,
    pub source_blend: u32,
    pub dest_blend: u32,
    pub depth_test: u32,
    pub depth_write: u32,
    pub cull: u32,
    pub alpha_test: u32,
    pub alpha_test_ref: Waveform,
    pub switches: LayerSwitches,
    pub ambient: Rgba,
    pub diffuse: Rgba,
    pub specular: Rgba,
    pub specular_power: Waveform,
    pub options: u64,
    pub reserved: [u32; 2],
    pub vertex_format: u32,
    /// A compiled vertex shader the layer carries, if any.
    pub vertex_shader: Vec<u8>,
    /// A compiled pixel shader the layer carries, if any.
    pub pixel_shader: Vec<u8>,
}

impl MaterialLayer {
    /// The two flag words; a counted list of texture stages (at most four); a counted list of
    /// texture-coordinate animations; the blend, depth, cull and alpha-test state and the alpha
    /// reference; the lighting switches; ambient, diffuse and specular colors and the specular
    /// power; a 64-bit options word and two reserved words; the vertex format; then the vertex
    /// shader and the pixel shader, each a byte count and that many bytes.
    // D2-10129
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let flags_set = r.u32()?;
        let flags_clear = r.u32()?;
        let stages = r.list(56, TextureStage::read)?;
        r.ensure(stages.len() < 5, || {
            format!("{} texture stages", stages.len())
        })?;
        let animations = r.list(52, StageAnimation::read)?;
        let source_blend = r.u32()?;
        let dest_blend = r.u32()?;
        let depth_test = r.u32()?;
        let depth_write = r.u32()?;
        let cull = r.u32()?;
        let alpha_test = r.u32()?;
        let alpha_test_ref = Waveform::read(r)?;
        let switches = LayerSwitches {
            particle_lighting: r.bool32()?,
            dynamic_lighting: r.bool32()?,
            pixel_lighting: r.bool32()?,
            ignore_directional_lights: r.bool32()?,
            ignore_point_lights: r.bool32()?,
            specular_highlights: r.bool32()?,
            pixel_specular: r.bool32()?,
            material_diffuse: r.bool32()?,
            vertex_diffuse: r.bool32()?,
            modulate_vertex_diffuse: r.bool32()?,
            pixel_multiple_lights: r.bool32()?,
            depth_fog: r.bool32()?,
            depth_alpha_fog: r.bool32()?,
            diffuse_fog_amount: r.bool32()?,
        };
        let ambient = r.rgba()?;
        let diffuse = r.rgba()?;
        let specular = r.rgba()?;
        let specular_power = Waveform::read(r)?;
        let options = r.u64()?;
        let reserved = [r.u32()?, r.u32()?];
        let vertex_format = r.u32()?;
        let n = r.count(1)?;
        let vertex_shader = r.bytes(n)?.to_vec();
        let n = r.count(1)?;
        let pixel_shader = r.bytes(n)?.to_vec();
        Ok(Self {
            flags_set,
            flags_clear,
            stages,
            animations,
            source_blend,
            dest_blend,
            depth_test,
            depth_write,
            cull,
            alpha_test,
            alpha_test_ref,
            switches,
            ambient,
            diffuse,
            specular,
            specular_power,
            options,
            reserved,
            vertex_format,
            vertex_shader,
            pixel_shader,
        })
    }
}

/// A material (`.mat`): its parameters, its layers and how it sorts.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Material {
    pub parameters: MaterialModifier,
    pub layers: Vec<MaterialLayer>,
    pub sort_mode: u32,
}

impl Decode for Material {
    /// The id, the parameters (with their own id word), a counted list of layers, the sort mode.
    // D2-10180
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let parameters = MaterialModifier::read(r)?;
        let layers = r.list(8, MaterialLayer::read)?;
        let sort_mode = r.u32()?;
        Ok(Self {
            parameters,
            layers,
            sort_mode,
        })
    }
}

/// A material instance (`.mi`): the material it draws with, its kind, and the modifiers applied.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MaterialInstance {
    pub material: DataId,
    pub kind: u32,
    pub modifiers: Vec<DataId>,
}

impl Decode for MaterialInstance {
    /// The id, the material id, the kind, then a counted list of modifier ids.
    // D2-10105
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let material = r.data_id()?;
        let kind = r.u32()?;
        let modifiers = r.list(4, Reader::data_id)?;
        Ok(Self {
            material,
            kind,
            modifiers,
        })
    }
}
