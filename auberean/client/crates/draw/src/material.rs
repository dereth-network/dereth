//! Materials resolved for drawing: a material instance's template with its own parameters and the
//! instance's modifiers written through, and the layers the device draws at the chosen detail.

use auberean_assets::common::Waveform;
use auberean_assets::material::{
    CoordinateAnimation, Material, MaterialInstance, MaterialLayer, MaterialModifier,
    MaterialParameter,
};
use auberean_primitives::random::Random;
use auberean_primitives::Rgba;
use auberean_world::color::Color;
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;

/// Direct3D 8 blend factors, comparison functions, cull modes, texture operations and arguments,
/// as the material records store them.
pub mod d3d {
    pub const BLEND_ZERO: u32 = 1;
    pub const BLEND_ONE: u32 = 2;
    pub const BLEND_SRC_ALPHA: u32 = 5;
    pub const BLEND_INV_SRC_ALPHA: u32 = 6;
    pub const BLEND_DEST_ALPHA: u32 = 7;
    pub const BLEND_INV_DEST_ALPHA: u32 = 8;
    pub const BLEND_DEST_COLOR: u32 = 9;
    pub const CMP_LESS_EQUAL: u32 = 4;
    pub const CMP_ALWAYS: u32 = 8;
    pub const CULL_NONE: u32 = 1;
    pub const CULL_CW: u32 = 2;
    pub const CULL_CCW: u32 = 3;
    pub const OP_DISABLE: u32 = 1;
    pub const OP_SELECT_ARG1: u32 = 2;
    pub const OP_SELECT_ARG2: u32 = 3;
    pub const OP_MODULATE: u32 = 4;
    pub const OP_MODULATE2X: u32 = 5;
    pub const OP_MODULATE4X: u32 = 6;
    pub const OP_ADD: u32 = 7;
    pub const OP_ADD_SIGNED: u32 = 8;
    pub const OP_ADD_SIGNED2X: u32 = 9;
    pub const ARG_DIFFUSE: u32 = 0;
    pub const ARG_CURRENT: u32 = 1;
    pub const ARG_TEXTURE: u32 = 2;
    pub const ADDRESS_CLAMP: u32 = 3;
}

/// The device flags a layer is kept for: the shader-capable hardware class, the high material
/// detail, and linear fog on.
pub const DEVICE_FLAGS: u32 = device_flags(2);

/// The device flags a layer is kept for at material detail `detail` (0 low, 1 medium, 2 high; any
/// higher is high): the shader-capable hardware class, that detail's flag (`0x10000`, `0x20000`,
/// `0x40000`), and linear fog on. Most materials hold one layer for each detail.
// D2-09885
#[must_use]
pub const fn device_flags(detail: u32) -> u32 {
    let detail = if detail > 2 { 2 } else { detail };
    0x0020_0000 | (0x0001_0000 << detail) | 0x0008_0000
}

/// The compiler switches of a layer, as the vertex program is built from them.
pub mod switch {
    pub const DYNAMIC_LIGHTING: u32 = 0x1;
    pub const PIXEL_LIGHTING: u32 = 0x2;
    pub const SPECULAR: u32 = 0x4;
    pub const PIXEL_SPECULAR: u32 = 0x8;
    pub const MATERIAL_DIFFUSE: u32 = 0x10;
    pub const VERTEX_DIFFUSE: u32 = 0x20;
    pub const MODULATE_VERTEX_DIFFUSE: u32 = 0x40;
    pub const PIXEL_MULTIPLE_LIGHTS: u32 = 0x80;
    pub const DEPTH_FOG: u32 = 0x100;
    pub const DIFFUSE_FOG_AMOUNT: u32 = 0x200;
    pub const IGNORE_DIRECTIONAL_LIGHTS: u32 = 0x400;
    pub const IGNORE_POINT_LIGHTS: u32 = 0x800;
}

/// One texture stage of a drawn layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// A picture list (`0x40...`) or picture (`0x41...`), or 0 for none.
    pub texture: u32,
    pub color_op: u32,
    pub color_arg1: u32,
    pub color_arg2: u32,
    pub alpha_op: u32,
    pub alpha_arg1: u32,
    pub alpha_arg2: u32,
    pub clamp_u: bool,
    pub clamp_v: bool,
    /// Which of the vertex's coordinate sets the stage reads.
    pub tex_coord: u32,
    /// The stage's coordinate transform, `(u, v) -> (a u + b v + c, d u + e v + f)` as
    /// `[a, b, c, d, e, f]`: the layer's coordinate animations at their start.
    pub uv_transform: [f32; 6],
}

/// The identity coordinate transform.
pub const UV_IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// What a layer's pixel program does beyond the common bump-lit program: the normal map in
/// stage 0 lit by the sun's direction in the surface's own frame, the light times the layer's
/// color plus the ambient, doubled against the diffuse map in stage 1, plus the specular.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PixelProgram {
    /// A detail map in stage 2 is added, signed, to the diffuse map.
    pub detail_on_diffuse: bool,
    /// A detail map in stage 2 is added, signed, to the normal map.
    pub detail_on_normal: bool,
    /// The diffuse map's alpha scales the layer's alpha.
    pub alpha_from_diffuse: bool,
    /// The diffuse map's alpha scales the specular (a gloss map).
    pub gloss: bool,
    /// The specular is worked out per pixel from the half vector between the light and the eye in
    /// the surface's own frame (handed on as a coordinate set) against the normal map:
    /// `sat(4 (sat(n·h)² - 0.75 a))` for the light's attenuation `a`, times the specular color.
    pub half_vector_specular: bool,
    /// An environment map in stage 2, looked up by the eye's reflection about the surface, is
    /// added to the diffuse map.
    pub environment_add: bool,
    /// An environment map in stage 2 is added to the diffuse map times the diffuse map's alpha
    /// (a gloss map for the reflection).
    pub environment_gloss: bool,
}

impl PixelProgram {
    /// The program a layer's pixel-program text describes, read from its instructions.
    // D2-09824
    #[must_use]
    pub fn from_text(text: &[u8]) -> Self {
        let mut p = Self::default();
        for line in String::from_utf8_lossy(text).lines() {
            let code: String = line
                .split(';')
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            match code.trim_start_matches('+') {
                "addt1.rgb,t1,t2_bias" => p.detail_on_diffuse = true,
                "addt0.rgb,t0,t2_bias" => p.detail_on_normal = true,
                "mulr0.a,c4.a,t1.a" => p.alpha_from_diffuse = true,
                "madr0.rgb,r1,t1.a,r0" => p.gloss = true,
                "mad_x4_satr1.rgb,r1,r1,-v1.a" => p.half_vector_specular = true,
                "addt1.rgb,t1,t2" => p.environment_add = true,
                "madt1.rgb,t2,t1.a,t1" => p.environment_gloss = true,
                _ => {}
            }
        }
        p
    }
}

/// One layer as the device draws it: one pass.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawLayer {
    pub stages: Vec<Stage>,
    pub source_blend: u32,
    pub dest_blend: u32,
    pub depth_test: u32,
    pub depth_write: bool,
    pub cull: u32,
    pub alpha_test: bool,
    /// The alpha a texel must exceed, 0 to 1.
    pub alpha_ref: f32,
    /// The layer itself asks for the alpha test, not just for its blending by alpha.
    pub tests_alpha: bool,
    /// The vertex program's switches ([`switch`]).
    pub switches: u32,
    pub ambient: Color,
    pub diffuse: Color,
    pub specular: Color,
    pub specular_power: f32,
    /// The layer's own pixel program, if it has one.
    pub pixel_program: Option<PixelProgram>,
    /// The light counts, (directional, point), the layer has vertex programs for
    /// ([`layer_light_variants`]).
    pub light_variants: Vec<(u32, u32)>,
    /// What moves the layer as the client's running time goes on, when anything does: its
    /// pictures' coordinates, its specular power, its alpha reference.
    pub motion: Option<std::sync::Arc<LayerMotion>>,
}

/// How a layer tests alpha and marks depth when drawn with a tint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Faded {
    /// Whether the draw marks how near it is.
    pub depth_write: bool,
    /// The alpha a texel must exceed, when it is tested at all.
    pub alpha_ref: Option<f32>,
}

impl DrawLayer {
    /// How the layer draws at `alpha` (its tint's), an object past 40 m being `far`: a layer that
    /// tests or blends by alpha, when the draw is not wholly opaque and either far off or (a layer
    /// only blending) under 0.8, no longer marks depth and passes every texel with some alpha, so
    /// what is behind it shows through. A layer neither testing nor blending by alpha draws as it
    /// is, whatever the alpha.
    // D2-09861
    #[must_use]
    pub fn faded(&self, alpha: f32, far: bool) -> Faded {
        let a = self.diffuse.a * alpha;
        if self.alpha_test && a < 1.0 && (far || (!self.tests_alpha && a < 0.8)) {
            Faded {
                depth_write: false,
                alpha_ref: Some(0.0),
            }
        } else {
            Faded {
                depth_write: self.depth_write,
                alpha_ref: self.alpha_test.then_some(self.alpha_ref),
            }
        }
    }
}

/// What moves a layer as time goes on: its stored layer's coordinate animations, specular power
/// and alpha reference, with the switches and color that say whether specular light is on.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerMotion {
    layer: MaterialLayer,
}

impl LayerMotion {
    /// What moves `l`, when anything does: a coordinate animation, its specular power or its
    /// alpha reference that is not steady.
    #[must_use]
    pub fn of(l: &MaterialLayer) -> Option<Self> {
        let moves = |w: &Waveform| w.kind != 1 || w.base_rate != 0.0;
        let animated = l.animations.iter().any(|a| match &a.animation {
            CoordinateAnimation::Translate { u, v } | CoordinateAnimation::Scale { u, v } => {
                moves(u) || moves(v)
            }
            CoordinateAnimation::Rotate(r) => moves(r),
            CoordinateAnimation::Transform(_) => false,
        });
        let alpha = l.alpha_test != 0 && moves(&l.alpha_test_ref);
        (animated || moves(&l.specular_power) || alpha).then(|| Self { layer: l.clone() })
    }
}

impl DrawLayer {
    /// The layer as it is drawn `t` seconds into the client's run, rolling `rng` for any noise:
    /// as the device sets each draw up, its specular power, then its alpha reference, then each
    /// stage's coordinate transform from the animations on it, in their order. A layer nothing
    /// moves is left as it is.
    // D2-09861
    pub fn move_on(&mut self, t: f64, rng: &Random) {
        let Some(m) = self.motion.clone() else {
            return;
        };
        let l = &m.layer;
        let power = auberean_world::wave::value(&l.specular_power, t, rng);
        let on = (power != 0.0 && l.switches.specular_highlights) || l.switches.pixel_specular;
        self.specular = if on {
            color(l.specular)
        } else {
            Color::new(0.0, 0.0, 0.0, 0.0)
        };
        self.specular_power = if on { power } else { 0.0 };
        if l.alpha_test != 0 {
            self.alpha_ref =
                alpha_reference(auberean_world::wave::value(&l.alpha_test_ref, t, rng));
        }
        for (k, s) in (0u32..).zip(self.stages.iter_mut()) {
            s.uv_transform = uv_transform(l, k, |w| auberean_world::wave::value(w, t, rng));
        }
    }
}

/// The alpha a texel must exceed for an alpha reference `v`: as the device takes it, a whole
/// step of 255 (cut toward zero), not held to one.
fn alpha_reference(v: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let steps = dereth_primitives::num::to_i32(v * 255.0) as f32;
    steps / 255.0
}

impl DrawLayer {
    /// Whether the pass blends with what is behind it.
    #[must_use]
    pub fn blends(&self) -> bool {
        !(self.source_blend == d3d::BLEND_ONE && self.dest_blend == d3d::BLEND_ZERO)
    }

    /// The stage that carries the surface's own picture: the second stage of a layer with a
    /// pixel program (whose first stage is the normal map), else the first stage with a picture.
    #[must_use]
    pub fn diffuse_stage(&self) -> Option<&Stage> {
        if self.pixel_program.is_some() && self.stages.len() > 1 && self.stages[1].texture != 0 {
            return self.stages.get(1);
        }
        self.stages.iter().find(|s| s.texture != 0)
    }
}

fn color(c: Rgba) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

/// A waveform's value at its start: a steady one is its base; a moving one adds its phase.
// D2-13338, D2-13337
#[must_use]
pub fn waveform_value(w: &Waveform) -> f32 {
    let phase = w.phase * std::f32::consts::PI;
    match w.kind {
        // speed: the base plus the phase in half turns (plus the frequency times the time)
        2 => w.base + phase,
        // sine and bounce: the base plus the amplitude times the sine of the phase
        4 => w.base + w.amplitude * math::sinf(phase),
        6 => w.base + w.amplitude * math::sinf(phase).abs(),
        _ => w.base,
    }
}

/// A waveform from a parameter's bytes: the shape word, then ten floats.
fn waveform(data: &[u8]) -> Option<Waveform> {
    Some(Waveform {
        kind: word(data, 0)?,
        base: float(data, 1)?,
        base_rate: float(data, 2)?,
        amplitude: float(data, 3)?,
        amplitude_rate: float(data, 4)?,
        phase: float(data, 5)?,
        phase_rate: float(data, 6)?,
        frequency: float(data, 7)?,
        frequency_rate: float(data, 8)?,
        roughness: float(data, 9)?,
        roughness_rate: float(data, 10)?,
    })
}

/// The coordinate transform of `stage`: each of the layer's animations on that stage applied in
/// list order, each at its start (translate, turn by degrees, scale; a full matrix as stored).
// D2-10056, D2-10061, D2-10064, D2-10068
#[must_use]
pub fn stage_uv_transform(l: &MaterialLayer, stage: u32) -> [f32; 6] {
    uv_transform(l, stage, waveform_value)
}

fn uv_transform(
    l: &MaterialLayer,
    stage: u32,
    mut value: impl FnMut(&Waveform) -> f32,
) -> [f32; 6] {
    // u' = m[0] u + m[1] v + m[2]; v' = m[3] u + m[4] v + m[5]
    let then = |m: [f32; 6], n: [f32; 6]| {
        [
            n[0] * m[0] + n[1] * m[3],
            n[0] * m[1] + n[1] * m[4],
            n[0] * m[2] + n[1] * m[5] + n[2],
            n[3] * m[0] + n[4] * m[3],
            n[3] * m[1] + n[4] * m[4],
            n[3] * m[2] + n[4] * m[5] + n[5],
        ]
    };
    let mut m = UV_IDENTITY;
    for a in l.animations.iter().filter(|a| a.stage == stage) {
        let n = match &a.animation {
            // the client takes v first
            CoordinateAnimation::Translate { u, v } => {
                let tv = value(v);
                [1.0, 0.0, value(u), 0.0, 1.0, tv]
            }
            CoordinateAnimation::Scale { u, v } => {
                let sv = value(v);
                [value(u), 0.0, 0.0, 0.0, sv, 0.0]
            }
            CoordinateAnimation::Rotate(r) => {
                let (s, c) = math::sin_cosf(value(r).to_radians());
                // a row vector times a turn about z: u' = u c - v s, v' = u s + v c
                [c, -s, 0.0, s, c, 0.0]
            }
            CoordinateAnimation::Transform(t) => [t[0], t[4], t[12], t[1], t[5], t[13]],
        };
        m = then(m, n);
    }
    m
}

/// A layer's switches as the vertex program's flag word.
// D2-10115
#[must_use]
pub fn switches(l: &MaterialLayer) -> u32 {
    let s = &l.switches;
    let mut f = 0;
    for (on, bit) in [
        (s.dynamic_lighting, switch::DYNAMIC_LIGHTING),
        (s.pixel_lighting, switch::PIXEL_LIGHTING),
        (s.specular_highlights, switch::SPECULAR),
        (s.pixel_specular, switch::PIXEL_SPECULAR),
        (s.material_diffuse, switch::MATERIAL_DIFFUSE),
        (s.vertex_diffuse, switch::VERTEX_DIFFUSE),
        (s.modulate_vertex_diffuse, switch::MODULATE_VERTEX_DIFFUSE),
        (s.pixel_multiple_lights, switch::PIXEL_MULTIPLE_LIGHTS),
        (s.depth_fog, switch::DEPTH_FOG),
        (s.diffuse_fog_amount, switch::DIFFUSE_FOG_AMOUNT),
        (
            s.ignore_directional_lights,
            switch::IGNORE_DIRECTIONAL_LIGHTS,
        ),
        (s.ignore_point_lights, switch::IGNORE_POINT_LIGHTS),
        (s.depth_alpha_fog, 0x1000),
    ] {
        if on {
            f |= bit;
        }
    }
    if l.options & 1 != 0 {
        f |= 0x2000;
    }
    f
}

/// The light counts, (directional, point), a layer with switches `sw` has vertex programs for: a
/// dynamically lit layer, or a pixel-lit one lighting with several lights, one for each count up to
/// one directional and four point lights (none of a kind it ignores), never neither; a layer lit
/// per pixel by one light, one directional and one point program (unless it ignores that kind);
/// any other, one program with no light.
// D2-10135
#[must_use]
pub fn light_variants(sw: u32) -> Vec<(u32, u32)> {
    let dirs = u32::from(sw & switch::IGNORE_DIRECTIONAL_LIGHTS == 0);
    let points = if sw & switch::IGNORE_POINT_LIGHTS == 0 {
        4
    } else {
        0
    };
    let several = sw & switch::DYNAMIC_LIGHTING != 0
        || (sw & switch::PIXEL_LIGHTING != 0 && sw & switch::PIXEL_MULTIPLE_LIGHTS != 0);
    if several {
        let mut out = Vec::new();
        for d in 0..=dirs {
            for p in 0..=points {
                if d + p != 0 {
                    out.push((d, p));
                }
            }
        }
        out
    } else if sw & switch::PIXEL_LIGHTING != 0 {
        let mut out = Vec::new();
        if dirs != 0 {
            out.push((1, 0));
        }
        if points != 0 {
            out.push((0, 1));
        }
        out
    } else {
        vec![(0, 0)]
    }
}

/// The most instructions a vertex program may have as its pieces are written. The device takes
/// programs of at most 128 instructions, and the linker that joins the pieces saves some ten of
/// them; every program the shipped materials make with more than this does not fit, and every one
/// with this many or fewer does.
pub const PROGRAM_PIECES_LIMIT: u32 = 138;

/// The parts of a vertex program a later piece may need, each added once, with the parts it needs
/// first.
#[derive(Default)]
struct Built {
    length: u32,
    view_position: bool,
    to_eye: bool,
    model_normal: bool,
    view_normal: bool,
    reflection: bool,
    model_s: bool,
    view_s: bool,
    model_t: bool,
    view_t: bool,
}

#[derive(Clone, Copy)]
enum Part {
    ViewPosition,
    ToEye,
    ModelNormal,
    ViewNormal,
    Reflection,
    ModelS,
    ViewS,
    ModelT,
    ViewT,
}

impl Built {
    fn add(&mut self, n: u32) {
        self.length += n;
    }

    fn need(&mut self, p: Part) {
        let (have, before, n): (bool, &[Part], u32) = match p {
            Part::ViewPosition => (self.view_position, &[], 4),
            Part::ToEye => (self.to_eye, &[Part::ViewPosition], 3),
            Part::ModelNormal => (self.model_normal, &[], 1),
            Part::ViewNormal => (self.view_normal, &[Part::ModelNormal], 6),
            Part::Reflection => (self.reflection, &[Part::ToEye, Part::ViewNormal], 6),
            Part::ModelS => (self.model_s, &[], 1),
            Part::ViewS => (self.view_s, &[Part::ModelS], 6),
            Part::ModelT => (self.model_t, &[], 1),
            Part::ViewT => (self.view_t, &[Part::ModelT], 6),
        };
        if have {
            return;
        }
        for &b in before {
            self.need(b);
        }
        self.add(n);
        *match p {
            Part::ViewPosition => &mut self.view_position,
            Part::ToEye => &mut self.to_eye,
            Part::ModelNormal => &mut self.model_normal,
            Part::ViewNormal => &mut self.view_normal,
            Part::Reflection => &mut self.reflection,
            Part::ModelS => &mut self.model_s,
            Part::ViewS => &mut self.view_s,
            Part::ModelT => &mut self.model_t,
            Part::ViewT => &mut self.view_t,
        } = true;
    }
}

/// How many instructions a layer's vertex program has as its pieces are written, the program for
/// switches `sw`, stage coordinate sources `sources` and texture matrices `matrices` (as
/// [`vertex_inputs`] gives them) lit by `dirs` directional and `points` point lights; none when no
/// program is made (a layer lit per pixel with no light). The pieces are chosen as the client
/// chooses them: the position, each stage's coordinates, the light in the surface's own frame for
/// a normal map, each light's share, the fog, the colors out.
// D2-10130
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn program_length(
    sw: u32,
    sources: [u32; 4],
    matrices: u32,
    dirs: u32,
    points: u32,
) -> Option<u32> {
    const NONE: u32 = u32::MAX;
    let mode = if sw & 0x21 == 0x21 {
        3
    } else if sw & switch::PIXEL_LIGHTING != 0 {
        if sw & switch::PIXEL_MULTIPLE_LIGHTS != 0 {
            4
        } else {
            2
        }
    } else if sw & switch::DYNAMIC_LIGHTING != 0 {
        1
    } else {
        0
    };
    let spec = sw & switch::SPECULAR != 0;
    let pspec = sw & switch::PIXEL_SPECULAR != 0;
    let matdif = sw & switch::MATERIAL_DIFFUSE != 0;
    let vtxdif = sw & switch::VERTEX_DIFFUSE != 0;
    let modvtx = sw & switch::MODULATE_VERTEX_DIFFUSE != 0;
    let fog = sw & switch::DEPTH_FOG != 0;
    let diffog = sw & switch::DIFFUSE_FOG_AMOUNT != 0 && fog;
    let afog = sw & 0x1000 != 0;
    let premodulated = sw & 0x2000 == 0;
    let mut b = Built::default();
    // the position, through to the clip space, out
    b.add(6);
    if sources.contains(&0x200) {
        b.need(Part::ViewPosition);
        b.need(Part::ToEye);
        b.need(Part::ViewNormal);
        if mode == 3 {
            b.add(7);
        } else if points != 0 {
            b.add(9);
        } else if dirs != 0 {
            b.add(7);
        }
    }
    let (mut bump_point, mut bump_dir, mut bump_point_att, mut bump_dir_att) =
        (false, false, false, false);
    if sources.contains(&0x600) {
        if points != 0 {
            b.need(Part::ViewPosition);
            b.add(1);
            bump_point = true;
        } else {
            b.add(1);
            bump_dir = true;
        }
        b.need(Part::ToEye);
        b.add(4);
        b.add(14);
        (b.model_s, b.view_s, b.model_t, b.view_t) = (true, true, true, true);
        b.need(Part::ViewNormal);
        if points != 0 {
            if !bump_point {
                b.need(Part::ViewPosition);
                b.add(1);
                bump_point = true;
            }
            b.add(6);
            bump_point_att = true;
        } else {
            if !bump_dir {
                b.add(1);
                bump_dir = true;
            }
            b.add(1);
            bump_dir_att = true;
        }
        b.add(9);
    }
    let mut to_light_stage = None;
    for (k, &src) in (0u32..).zip(&sources) {
        if src == NONE {
            break;
        }
        match src {
            0x100 => {
                b.need(Part::Reflection);
                b.add(1);
            }
            0x200 | 0x500 => b.add(1),
            0x300 | 0x600 => b.add(2),
            0x400 => {
                b.need(Part::ViewPosition);
                b.add(2);
            }
            _ => b.add(1),
        }
        if src == 0x500 {
            to_light_stage = Some(k);
        }
        if matrices & (1 << k) != 0 {
            b.add(4);
        }
        if src != 0x500 {
            b.add(1);
        }
    }
    if mode == 2 || mode == 4 {
        if dirs == 0 && points == 0 {
            return None;
        }
        b.need(Part::ViewS);
        b.need(Part::ViewT);
        b.need(Part::ViewNormal);
        if points != 0 {
            if !bump_point {
                b.need(Part::ViewPosition);
                b.add(1);
                bump_point = true;
            }
        } else if !bump_dir {
            b.add(1);
            bump_dir = true;
        }
        b.add(9);
        if points != 0 {
            if !bump_point_att {
                if !bump_point {
                    b.need(Part::ViewPosition);
                    b.add(1);
                }
                b.add(6);
            }
        } else if !bump_dir_att {
            if !bump_dir {
                b.add(1);
            }
            b.add(1);
        }
        b.add(1);
        if pspec {
            b.add(2);
        } else if !spec {
            b.add(3 + u32::from(matdif));
        }
    }
    let mut vertex_block = false;
    match mode {
        3 => {
            b.need(Part::ViewS);
            b.need(Part::ViewT);
            b.need(Part::ViewNormal);
            b.add(4);
            vertex_block = true;
        }
        1 => {
            b.add(1);
            vertex_block = true;
        }
        4 => {
            if !pspec {
                b.add(1);
            }
            if !b.view_normal {
                if !b.model_normal {
                    b.add(1);
                }
                b.add(6);
            }
            b.need(Part::ViewPosition);
            let mut first = true;
            for _ in 0..points {
                b.add(13 + if first { 0 } else { 2 } + if pspec { 0 } else { 2 });
                first = false;
            }
            for _ in 0..dirs {
                b.add(6 + if first { 0 } else { 1 } + if pspec { 0 } else { 1 });
                first = false;
            }
            b.add(if premodulated { 3 } else { 1 });
        }
        2 if spec && !pspec => vertex_block = true,
        _ => {}
    }
    if vertex_block {
        if spec {
            b.add(1);
        }
        b.need(Part::ViewNormal);
        b.need(Part::ViewPosition);
        for _ in 0..dirs {
            b.add(if spec { 4 } else { 2 } + 2);
            if mode != 2 {
                b.add(1);
            }
            if spec {
                b.add(1);
            }
        }
        for _ in 0..points {
            if mode == 3 && to_light_stage.is_some() {
                b.need(Part::ViewS);
                b.need(Part::ViewT);
                b.need(Part::ViewNormal);
                b.add(12);
            }
            b.add(if spec { 11 } else { 9 } + 2);
            if mode != 2 {
                b.add(2);
            }
            if spec {
                b.add(2);
            }
        }
        if mode == 1 {
            b.add(1 + u32::from(matdif) + u32::from(vtxdif));
        } else if mode == 3 && to_light_stage.is_some() {
            b.add(1);
        }
        if afog {
            b.need(Part::ToEye);
            b.add(4);
        }
        if spec || pspec {
            b.add(2);
        }
    }
    if fog {
        b.need(Part::ToEye);
        b.add(3);
        if !diffog {
            b.add(1);
        }
    }
    if mode == 0 {
        b.add(1 + u32::from(diffog) + u32::from(matdif) + u32::from(vtxdif));
        if afog {
            b.need(Part::ToEye);
            b.add(4);
        }
    } else if mode == 3 && modvtx {
        b.add(1);
    }
    b.add(1);
    if to_light_stage.is_some() {
        b.add(6);
    }
    Some(b.length)
}

/// The light counts, (directional, point), a layer has vertex programs for: those
/// [`light_variants`] makes for its switches whose programs fit the device (see
/// [`PROGRAM_PIECES_LIMIT`]); a program that does not fit is not made, and a draw with that many
/// lights takes the nearest program that was.
// D2-10135, D2-10130
#[must_use]
pub fn layer_light_variants(l: &MaterialLayer) -> Vec<(u32, u32)> {
    let sw = switches(l);
    let (sources, matrices) = vertex_inputs(l);
    light_variants(sw)
        .into_iter()
        .filter(|&(d, p)| {
            program_length(sw, sources, matrices, d, p).is_some_and(|n| n <= PROGRAM_PIECES_LIMIT)
        })
        .collect()
}

/// Which of a layer's programs (`variants`, as [`light_variants`] makes them) draws on a device
/// with `dirs` directional and `points` point lights: the one made for those counts, else, when
/// there are fewer point lights than directional ones, the one with the most directional lights
/// up to `dirs` (whatever its point lights), otherwise the one with the most point lights up to
/// `points` (whatever its directional ones), nearer than `dirs + points`; the first of equals;
/// failing all, the first program (none for a layer with none).
// D2-10119
#[must_use]
pub fn variant_for(variants: &[(u32, u32)], dirs: u32, points: u32) -> Option<usize> {
    if let Some(k) = variants.iter().position(|&v| v == (dirs, points)) {
        return Some(k);
    }
    let by_dirs = points < dirs;
    let want = if by_dirs { dirs } else { points };
    let of = |v: &(u32, u32)| if by_dirs { v.0 } else { v.1 };
    let mut best = (!variants.is_empty()).then_some(0);
    let mut gap = dirs + points;
    for (k, v) in variants.iter().enumerate() {
        let have = of(v);
        if have <= want && want - have < gap {
            best = Some(k);
            gap = want - have;
        }
    }
    best
}

fn word(data: &[u8], k: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(k * 4..k * 4 + 4)?.try_into().ok()?,
    ))
}

fn float(data: &[u8], k: usize) -> Option<f32> {
    word(data, k).map(f32::from_bits)
}

/// Write a parameter's `data` through `fields` into the layers.
// D2-10160, D2-10155, D2-10156, D2-10157, D2-10159, D2-10089
fn apply(layers: &mut [MaterialLayer], fields: &MaterialParameter, data: &[u8]) {
    for f in &fields.fields {
        let [field, _data_type, layer, stage] = f.words;
        let Some(l) = usize::try_from(layer).ok().and_then(|i| layers.get_mut(i)) else {
            continue;
        };
        let st = usize::try_from(stage).ok();
        let rgba = || -> Option<Rgba> {
            if data.len() >= 16 {
                Some(Rgba {
                    r: float(data, 0)?,
                    g: float(data, 1)?,
                    b: float(data, 2)?,
                    a: float(data, 3)?,
                })
            } else {
                let c = Color::from_argb(word(data, 0)?);
                Some(Rgba {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                    a: c.a,
                })
            }
        };
        match field {
            0 => l.ambient = rgba().unwrap_or(l.ambient),
            1 => l.diffuse = rgba().unwrap_or(l.diffuse),
            2 => l.specular = rgba().unwrap_or(l.specular),
            3 => {
                if let Some(w) = waveform(data) {
                    l.specular_power = w;
                }
            }
            5 => {
                l.cull = if word(data, 0).unwrap_or(0) != 0 {
                    d3d::CULL_NONE
                } else {
                    d3d::CULL_CW
                }
            }
            6 => {
                l.depth_test = if word(data, 0).unwrap_or(1) == 0 {
                    d3d::CMP_ALWAYS
                } else {
                    d3d::CMP_LESS_EQUAL
                }
            }
            7 => l.depth_write = u32::from(word(data, 0).unwrap_or(l.depth_write) != 0),
            8 => l.alpha_test = u32::from(word(data, 0).unwrap_or(l.alpha_test) != 0),
            9 => {
                if let Some(w) = waveform(data) {
                    l.alpha_test_ref = w;
                }
            }
            10 => {
                if let (Some(s), Some(id)) = (st.and_then(|s| l.stages.get_mut(s)), word(data, 0)) {
                    s.texture = auberean_primitives::DataId(id);
                }
            }
            11 | 12 => {
                if let (Some(s), Some(v)) = (st.and_then(|s| l.stages.get_mut(s)), word(data, 0)) {
                    let mode = if v != 0 { 1 } else { d3d::ADDRESS_CLAMP };
                    if field == 11 {
                        s.address_u = mode;
                    } else {
                        s.address_v = mode;
                    }
                }
            }
            // The layer's coordinate animations, by their place in its list: three fields set an
            // animation's first waveform (a slide's u, a turn, a stretch's u) and two its second
            // (a slide's or a stretch's v), whatever kind of animation is in that place. A fixed
            // matrix has no waveforms; the original writes over it (no stored material does).
            // Fields 13 to 32 set the original's diffuse and normal modifiers, which no stored
            // layer holds.
            33 | 35 | 36 => {
                let a = st.and_then(|k| l.animations.get_mut(k));
                if let (Some(a), Some(w)) = (a, waveform(data)) {
                    match &mut a.animation {
                        CoordinateAnimation::Translate { u, .. }
                        | CoordinateAnimation::Scale { u, .. }
                        | CoordinateAnimation::Rotate(u) => *u = w,
                        CoordinateAnimation::Transform(_) => {}
                    }
                }
            }
            34 | 37 => {
                let a = st.and_then(|k| l.animations.get_mut(k));
                if let (Some(a), Some(w)) = (a, waveform(data)) {
                    match &mut a.animation {
                        CoordinateAnimation::Translate { v, .. }
                        | CoordinateAnimation::Scale { v, .. } => *v = w,
                        CoordinateAnimation::Rotate(_) | CoordinateAnimation::Transform(_) => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Write a modifier's parameters into a material, each through the material's own parameter of
/// the same name.
pub fn apply_modifier(m: &mut Material, modifier: &MaterialModifier) {
    let template = m.parameters.clone();
    for p in &modifier.parameters {
        if let Some(t) = template.parameters.iter().find(|t| t.name == p.name) {
            apply(&mut m.layers, t, &p.data);
        }
    }
}

/// The kind a material instance has when a mesh names a material directly.
pub const DEFAULT_MATERIAL_KIND: u32 = 2;

/// The kind of the material a mesh names: its instance's kind, or the default for a material
/// named directly.
// D2-10102
#[must_use]
pub fn material_kind(records: &WorldRecords, instance: u32) -> u32 {
    if instance >> 24 == 0x31 {
        records
            .get::<MaterialInstance>(instance)
            .map_or(DEFAULT_MATERIAL_KIND, |mi| mi.kind)
    } else {
        DEFAULT_MATERIAL_KIND
    }
}

/// A material made ready: its own parameters, then the instance's modifiers in order.
// D2-10100
#[must_use]
pub fn instance_material(records: &WorldRecords, instance: u32) -> Option<Material> {
    instance_material_with(records, instance, None, &[])
}

/// A material made ready as an appearance changes it: its template replaced by `template` when
/// given, then its own parameters, the instance's modifiers in order, and `extra` after them.
// D2-10100, D2-10096, D2-10106
#[must_use]
pub fn instance_material_with(
    records: &WorldRecords,
    instance: u32,
    template: Option<u32>,
    extra: &[u32],
) -> Option<Material> {
    let (own_template, mut modifiers) = if instance >> 24 == 0x31 {
        let mi: MaterialInstance = records.get(instance)?;
        (
            mi.material.0,
            mi.modifiers.iter().map(|m| m.0).collect::<Vec<_>>(),
        )
    } else {
        (instance, Vec::new())
    };
    modifiers.extend_from_slice(extra);
    let mut m: Material = records.get(template.unwrap_or(own_template))?;
    let own = m.parameters.clone();
    for p in &own.parameters {
        apply(&mut m.layers, p, &p.data);
    }
    for id in modifiers {
        if let Some(modifier) = records.get::<MaterialModifier>(id) {
            apply_modifier(&mut m, &modifier);
        }
    }
    Some(m)
}

/// The hardware class of the device the client draws as: the one with vertex and pixel programs.
pub const HARDWARE_CLASS: u32 = 2;

/// The layer flags that tie a layer to a hardware class: must have (or must not have) class 1, 2 or
/// 3.
const CLASS_FLAGS: [(u32, u32); 3] = [(0x10_0000, 1), (0x20_0000, 2), (0x40_0000, 3)];

/// The material as a device of hardware class `class` keeps it: the layers made for another class
/// (by their must-set and must-clear flags) and the layers with no stages removed, and with them
/// the parameters' fields that wrote to them; the other fields renumbered to the layers that
/// remain.
// D2-10173
#[must_use]
pub fn sub_objects(m: &Material, class: u32) -> Material {
    let fits = |l: &MaterialLayer| {
        CLASS_FLAGS.iter().all(|&(bit, c)| {
            !(l.flags_set & bit != 0 && class != c) && !(l.flags_clear & bit != 0 && class == c)
        }) && !l.stages.is_empty()
    };
    let mut out = m.clone();
    let mut removed = 0u32;
    let mut layers = Vec::new();
    for (k, l) in (0u32..).zip(&m.layers) {
        if fits(l) {
            layers.push(l.clone());
            continue;
        }
        let at = k - removed;
        for p in &mut out.parameters.parameters {
            p.fields.retain(|f| f.words[2] != at);
            for f in &mut p.fields {
                if f.words[2] > at {
                    f.words[2] -= 1;
                }
            }
        }
        removed += 1;
    }
    out.layers = layers;
    out
}

/// What a layer's vertex program is built from besides its switches: each of the first four
/// stages' coordinate source (`u32::MAX` for none), set when the stage has a texture, a texture the
/// renderer supplies, or a source; and the stages whose coordinates an animation moves (a texture
/// matrix each, at most eight), as a mask.
// D2-10130
#[must_use]
pub fn vertex_inputs(l: &MaterialLayer) -> ([u32; 4], u32) {
    let mut sources = [u32::MAX; 4];
    for (k, st) in l.stages.iter().take(4).enumerate() {
        if st.texture.0 != 0 || st.special != 0 || st.tex_coord != u32::MAX {
            sources[k] = st.tex_coord;
        }
    }
    let mut matrices = 0u32;
    let mut count = 0;
    for (k, &src) in (0u32..).zip(&sources) {
        if src != u32::MAX && count < 8 && l.animations.iter().any(|a| a.stage == k) {
            matrices |= 1 << k;
            count += 1;
        }
    }
    (sources, matrices)
}

/// The layers the device draws for flags `device`: of the layers its hardware class keeps, those
/// whose must-set flags it has and whose must-clear flags it lacks (the material detail's
/// `0x10000`, `0x20000`, `0x40000` for low, medium and high, and `0x80000` for fog on), each one
/// pass.
// D2-09885
#[must_use]
pub fn draw_layers(m: &Material, device: u32) -> Vec<DrawLayer> {
    let m = &sub_objects(m, HARDWARE_CLASS);
    let keep =
        |l: &&MaterialLayer| l.flags_set & device == l.flags_set && l.flags_clear & device == 0;
    let mut kept: Vec<&MaterialLayer> = m.layers.iter().filter(keep).collect();
    if kept.is_empty() {
        kept = m.layers.iter().take(1).collect();
    }
    kept.into_iter().map(draw_layer).collect()
}

/// One stored layer as the device draws it.
#[must_use]
pub fn draw_layer(l: &MaterialLayer) -> DrawLayer {
    let stages = l
        .stages
        .iter()
        .zip(0u32..)
        .map(|(s, k)| Stage {
            texture: s.texture.0,
            color_op: s.color_op,
            color_arg1: s.color_arg1,
            color_arg2: s.color_arg2,
            alpha_op: s.alpha_op,
            alpha_arg1: s.alpha_arg1,
            alpha_arg2: s.alpha_arg2,
            clamp_u: s.address_u == d3d::ADDRESS_CLAMP,
            clamp_v: s.address_v == d3d::ADDRESS_CLAMP,
            tex_coord: s.tex_coord,
            uv_transform: stage_uv_transform(l, k),
        })
        .collect();
    // Specular light is on only with a non-zero power and highlights switched on, or
    // with specular in the pixel program; otherwise its color and power are zero.
    let power = waveform_value(&l.specular_power);
    let specular_on = (power != 0.0 && l.switches.specular_highlights) || l.switches.pixel_specular;
    DrawLayer {
        stages,
        source_blend: l.source_blend,
        dest_blend: l.dest_blend,
        depth_test: l.depth_test,
        depth_write: l.depth_write != 0,
        cull: l.cull,
        alpha_test: l.alpha_test != 0 || l.source_blend == d3d::BLEND_SRC_ALPHA,
        alpha_ref: if l.alpha_test != 0 {
            alpha_reference(waveform_value(&l.alpha_test_ref))
        } else {
            0.0
        },
        tests_alpha: l.alpha_test != 0,
        switches: switches(l),
        ambient: color(l.ambient),
        diffuse: color(l.diffuse),
        specular: if specular_on {
            color(l.specular)
        } else {
            Color::new(0.0, 0.0, 0.0, 0.0)
        },
        specular_power: if specular_on { power } else { 0.0 },
        pixel_program: (!l.pixel_shader.is_empty())
            .then(|| PixelProgram::from_text(&l.pixel_shader)),
        light_variants: layer_light_variants(l),
        motion: LayerMotion::of(l).map(std::sync::Arc::new),
    }
}

/// The layer an object draws its shadow with: its material's first stored layer, whatever the
/// device, whose first texture's alpha shapes the shadow.
// D2-05492
#[must_use]
pub fn shadow_layer(m: &Material) -> Option<DrawLayer> {
    m.layers.first().map(draw_layer)
}

/// The picture a terrain ground material shows: its first layer's first texture.
// D2-10884
#[must_use]
pub fn ground_picture(records: &WorldRecords, material: u32) -> Option<u32> {
    let m: Material = records.get(material)?;
    let t = m.layers.first()?.stages.first()?.texture.0;
    (t != 0).then_some(t)
}
