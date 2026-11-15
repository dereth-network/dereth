//! The constants the world's shaders read, as a device uploads them: the frame's light and camera,
//! each draw's layer and place, and the vertices' layouts. Every device that draws the world in the
//! retail client's way takes them as they are.

use auberean_world::math::Mat4;
use bytemuck::{Pod, Zeroable};
use dereth_primitives::num::math;

use crate::material::{d3d, DrawLayer, Stage, UV_IDENTITY};
use crate::scene::PointLights;

/// The constants of the light a frame's draws are made under: the camera, the fog, the sun and the
/// ambient, the shadow map's and the clouds' coordinates, the picture's size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub fog: [f32; 4],
    pub fog_color: [f32; 4],
    pub to_sun: [f32; 4],
    pub sun_color: [f32; 4],
    pub ambient: [f32; 4],
    pub forward: [f32; 4],
    pub sun_specular: [f32; 4],
    pub alpha_fog: [f32; 4],
    pub shadow_uv: [[f32; 4]; 4],
    // the clouds' picture scale, its slide along u and v, 1 when they are drawn
    pub cloud: [f32; 4],
    // 1 when the near terrain's shadows and fog are laid on after its layers, unused x3
    pub terrain: [f32; 4],
    // the picture's width and height in pixels, unused x2
    pub viewport: [f32; 4],
}

impl FrameUniforms {
    /// The light and fog of `l`: the sun's direction and color, the ambient light, the fog's
    /// range and color, and how much the fog veils the sky.
    pub fn light(&mut self, l: &auberean_world::sky::DayLight) {
        let to_sun =
            auberean_world::math::normalize(auberean_world::math::scale(l.light_direction, -1.0));
        let ambient = l.ambient();
        let fog_on = l.fog_max > l.fog_min;
        let fog_span = if fog_on {
            1.0 / (l.fog_max - l.fog_min)
        } else {
            0.0
        };
        self.fog = [
            l.fog_min,
            l.fog_max,
            fog_span,
            if fog_on { 1.0 } else { 0.0 },
        ];
        self.fog_color = [l.fog_color.r, l.fog_color.g, l.fog_color.b, l.fog_sky];
        self.to_sun = [to_sun.x, to_sun.y, to_sun.z, 0.0];
        self.sun_color = [
            l.direct_color.r * l.direct_brightness,
            l.direct_color.g * l.direct_brightness,
            l.direct_color.b * l.direct_brightness,
            1.0,
        ];
        self.ambient = [ambient.r, ambient.g, ambient.b, 1.0];
        self.sun_specular = [l.direct_color.r, l.direct_color.g, l.direct_color.b, 1.0];
    }
}

/// One draw's constants.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DrawUniforms {
    pub model: [[f32; 4]; 4],
    pub diffuse: [f32; 4],
    pub lit_diffuse: [f32; 4],
    pub specular: [f32; 4],
    pub layer_ambient: [f32; 4],
    pub params: [f32; 4],
    pub stage: [u32; 4],
    pub tfactor: [f32; 4],
    pub ops: [[u32; 4]; 3],
    pub program: [u32; 4],
    pub uv_sets: [u32; 4],
    pub uv_rows: [[f32; 4]; 6],
    pub pt_origin: [[f32; 4]; 4],
    pub pt_diffuse: [[f32; 4]; 4],
    pub pt_specular: [[f32; 4]; 4],
    pub shade: [f32; 4],
    pub light: [f32; 4],
}

/// A mesh vertex as the device takes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshGpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub diffuse: [f32; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub tangent_u: [f32; 3],
    pub tangent_v: [f32; 3],
    pub to_light: [f32; 3],
}

impl MeshGpuVertex {
    /// The vertex as the device takes it: its diffuse color as four floats.
    #[must_use]
    pub fn of(v: &crate::mesh::MeshVertex) -> Self {
        let c = auberean_world::color::Color::from_argb(v.diffuse);
        Self {
            position: v.position,
            normal: v.normal,
            diffuse: [c.r, c.g, c.b, c.a],
            uv0: v.uv[0],
            uv1: v.uv[1],
            tangent_u: v.tangent_u,
            tangent_v: v.tangent_v,
            to_light: v.to_light,
        }
    }
}

impl MeshGpuVertex {
    /// A vertex already on the screen (-1 to 1 across and up, and its depth), with its picture's
    /// coordinates and its color.
    #[must_use]
    pub fn screen(position: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            normal: [0.0, 0.0, 1.0],
            diffuse: color,
            uv0: uv,
            uv1: uv,
            tangent_u: [0.0; 3],
            tangent_v: [0.0; 3],
            to_light: [0.0; 3],
        }
    }
}

/// A landscape vertex as the device takes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TerrainGpuVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub mask_uv: [f32; 2],
}

/// A matrix as the device takes it: its rows as stored.
#[must_use]
pub fn mat(m: &Mat4) -> [[f32; 4]; 4] {
    m.0
}

/// A right-handed view and projection for a camera at `eye` looking along `forward`, z up, with
/// depth from 0 (near) to 1 (far): row-vector convention, as the world's transforms. In
/// perspective `fov_y` degrees high; straight on (`orthographic`), that many metres high and as
/// wide as the picture's shape makes it.
// D2-10574
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn view_proj(
    eye: [f32; 3],
    forward: [f32; 3],
    roll: f32,
    fov_y: f32,
    orthographic: Option<f32>,
    aspect: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    view_matrix(eye, forward, roll).then(&projection_matrix(fov_y, orthographic, aspect, near, far))
}

/// [`view_proj`]'s view alone: the world to the camera's space (x right, y up, z backward).
#[must_use]
pub fn view_matrix(eye: [f32; 3], forward: [f32; 3], roll: f32) -> Mat4 {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let f = norm(forward);
    let (r, u) = crate::scene::view_axes_of(f, roll);
    Mat4([
        [r[0], u[0], -f[0], 0.0],
        [r[1], u[1], -f[1], 0.0],
        [r[2], u[2], -f[2], 0.0],
        [-dot(r, eye), -dot(u, eye), dot(f, eye), 1.0],
    ])
}

/// [`view_proj`]'s projection alone.
#[must_use]
pub fn projection_matrix(
    fov_y: f32,
    orthographic: Option<f32>,
    aspect: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    if let Some(height) = orthographic {
        let d = 1.0 / (far - near);
        return Mat4([
            [2.0 / (height * aspect), 0.0, 0.0, 0.0],
            [0.0, 2.0 / height, 0.0, 0.0],
            [0.0, 0.0, -d, 0.0],
            [0.0, 0.0, -near * d, 1.0],
        ]);
    }
    let h = 1.0 / math::tanf(fov_y.to_radians() * 0.5);
    let w = h / aspect;
    let q = far / (near - far);
    Mat4([
        [w, 0.0, 0.0, 0.0],
        [0.0, h, 0.0, 0.0],
        [0.0, 0.0, q, -1.0],
        [0.0, 0.0, q * near, 0.0],
    ])
}

/// A layer's first three stages, each or none.
#[must_use]
pub fn bound_stages(layer: &DrawLayer) -> [Option<Stage>; 3] {
    [0, 1, 2].map(|k| layer.stages.get(k).copied())
}

/// A draw's constants: its layer drawn at `model` in `tint`, the sun's color `sun`, lit by
/// `points`.
#[must_use]
pub fn draw_uniforms(
    layer: &DrawLayer,
    model: &Mat4,
    tint: auberean_world::color::Color,
    sun: [f32; 4],
    points: &PointLights,
) -> DrawUniforms {
    let c =
        |c: auberean_world::color::Color| [c.r * tint.r, c.g * tint.g, c.b * tint.b, c.a * tint.a];
    let bound = bound_stages(layer);
    let mut uv_sets = [0u32; 4];
    let mut uv_rows = [[0.0f32; 4]; 6];
    for (k, s) in bound.iter().enumerate() {
        let m = s.map_or(UV_IDENTITY, |s| s.uv_transform);
        // A coordinate set (the vertex has two), or coordinates the program works out: the
        // anisotropic ones (source 0x200) or the eye's reflection about the surface (0x100).
        uv_sets[k] = s.map_or(0, |s| match s.tex_coord {
            0x100 => 3,
            0x200 => 2,
            t => t.min(1),
        });
        uv_rows[2 * k] = [m[0], m[1], m[2], 0.0];
        uv_rows[2 * k + 1] = [m[3], m[4], m[5], 0.0];
    }
    let pixel = layer.pixel_program.map_or([0; 4], |p| {
        let mut opts = 0;
        for (on, bit) in [
            (p.detail_on_diffuse, 0x1),
            (p.detail_on_normal, 0x2),
            (p.alpha_from_diffuse, 0x4),
            (p.gloss, 0x8),
            (p.half_vector_specular, 0x10),
            (p.environment_add, 0x20),
            (p.environment_gloss, 0x40),
            (p.anisotropic, 0x80),
            (p.normal_detail_first, 0x100),
            (p.tint_mask, 0x200),
            (p.color1_half_vector, 0x400),
        ] {
            if on {
                opts |= bit;
            }
        }
        [1, opts, 0, 0]
    });
    // The pixel programs' lit color: the first light's diffuse (the first point light's, else
    // the sun's, black inside with neither) times the layer's diffuse (and tint), each channel
    // at most 1, alpha the layer's at most 1.
    let sun = if points.count > 0 {
        let f = points.lights[0].diffuse;
        [f.r, f.g, f.b, 1.0]
    } else if points.inside {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        sun
    };
    let d = c(layer.diffuse);
    let lit_diffuse = [
        (sun[0] * d[0]).min(1.0),
        (sun[1] * d[1]).min(1.0),
        (sun[2] * d[2]).min(1.0),
        d[3].min(1.0),
    ];
    // The texture stages' arithmetic, for a layer without a pixel program: each of the first
    // three stages as stored, the rest disabled; a layer with no stages modulates its vertex
    // color twice by white.
    let mut ops = [[d3d::OP_DISABLE, 0, d3d::OP_DISABLE, 0]; 3];
    if layer.stages.is_empty() {
        ops[0] = [
            d3d::OP_MODULATE2X,
            d3d::ARG_TEXTURE | (d3d::ARG_DIFFUSE << 8),
            d3d::OP_SELECT_ARG1,
            d3d::ARG_DIFFUSE,
        ];
    }
    for (o, s) in ops.iter_mut().zip(&layer.stages) {
        *o = [
            s.color_op,
            s.color_arg1 | (s.color_arg2 << 8),
            s.alpha_op,
            s.alpha_arg1 | (s.alpha_arg2 << 8),
        ];
    }
    // The texture factor is the layer's diffuse color (and tint) as a stored color.
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
    let tfactor = [q(d[0]), q(d[1]), q(d[2]), q(d[3])];
    DrawUniforms {
        model: mat(model),
        diffuse: d,
        lit_diffuse,
        specular: c(layer.specular),
        layer_ambient: [
            layer.ambient.r,
            layer.ambient.g,
            layer.ambient.b,
            layer.ambient.a,
        ],
        params: [
            layer.specular_power,
            if layer.alpha_test {
                layer.alpha_ref
            } else {
                -1.0
            },
            0.0,
            0.0,
        ],
        stage: [
            layer.switches,
            u32::try_from(points.count).unwrap_or(0),
            u32::from(points.inside),
            0,
        ],
        tfactor,
        ops,
        program: pixel,
        uv_sets,
        uv_rows,
        pt_origin: points.lights.map(|l| {
            let r = l.range.max(0.0002);
            [l.origin.x, l.origin.y, l.origin.z, 10.0 / (r * r)]
        }),
        pt_diffuse: points
            .lights
            .map(|l| [l.diffuse.r, l.diffuse.g, l.diffuse.b, 1.0]),
        pt_specular: points
            .lights
            .map(|l| [l.specular.r, l.specular.g, l.specular.b, 1.0]),
        shade: [0.0; 4],
        light: sun,
    }
}
