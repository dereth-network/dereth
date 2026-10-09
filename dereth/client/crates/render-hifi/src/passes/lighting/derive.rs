//! The surface variant: the re-shaded stages, run so that each draw writes what its surface is
//! made of instead of how it was lit.
//!
//! It starts from the re-shaded text (so the vertex stage already hands on the world position,
//! the world normal and the vertex colour before lighting) and turns every pixel stage into a
//! helper the new pixel stage calls three times, each time with the same texture samples, alpha
//! test and stage chain:
//!
//! - as recorded: the ordinary colour, kept so what blended over the surface later in the frame
//!   can be carried over onto the new light;
//! - with the vertex colour replaced by the material's diffuse colour (the landscape's by white,
//!   since its vertex colour is its baked light): the surface's albedo;
//! - with the vertex colour replaced by the material's emissive colour plus the draw's own point
//!   lights, evaluated per pixel: the light the surface gives off or takes from lights the scene
//!   chose for it.
//!
//! Every pixel stage ends in four outputs (see [`TARGETS`]).

use dereth_render::VertexFormat;

use crate::derive::reshade::{has_normal, PIXEL_STAGES, SPLAT_STAGE};
use crate::derive::{Anchor, Scope};

/// What a surface pixel is, as the albedo target's alpha carries it (`kind / 8`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    /// Nothing opaque was drawn here.
    None = 0,
    /// An object lit by the sun.
    Lit = 1,
    /// An object lit only by the lights near it, indoors.
    Indoor = 2,
    /// An indoor cell, its static light baked into its vertices.
    Baked = 3,
    /// The landscape.
    Terrain = 4,
    /// Drawn with its colour as given: kept as drawn.
    Unlit = 5,
    /// Alpha-tested leaves and other cut-out surfaces lit by the sun.
    Foliage = 6,
}

impl SurfaceKind {
    /// Every kind.
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Lit,
        Self::Indoor,
        Self::Baked,
        Self::Terrain,
        Self::Unlit,
        Self::Foliage,
    ];

    /// The constant's name in the derived shader text.
    #[must_use]
    pub const fn wgsl_name(self) -> &'static str {
        match self {
            Self::None => "HIFI_KIND_NONE",
            Self::Lit => "HIFI_KIND_LIT",
            Self::Indoor => "HIFI_KIND_INDOOR",
            Self::Baked => "HIFI_KIND_BAKED",
            Self::Terrain => "HIFI_KIND_TERRAIN",
            Self::Unlit => "HIFI_KIND_UNLIT",
            Self::Foliage => "HIFI_KIND_FOLIAGE",
        }
    }

    /// The kinds as WGSL constants, for every shader that reads or writes them.
    #[must_use]
    pub fn wgsl() -> String {
        Self::ALL
            .iter()
            .map(|k| format!("const {}: f32 = {:.1};\n", k.wgsl_name(), *k as u8 as f32))
            .collect()
    }
}

/// The four targets every surface pipeline writes, in location order.
///
/// - 0, the albedo, linear light stored as sRGB; alpha the [`SurfaceKind`] over 8.
/// - 1, the radiance the surface gives off or takes from its own lights, linear; alpha the
///   brightness of the sun the draw was lit by (zero when none).
/// - 2, the ordinary colour, linear, after fog.
/// - 3, the view-space normal, each part from -1..1 to 0..1.
pub const TARGETS: [wgpu::TextureFormat; 4] = [
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Rgb10a2Unorm,
];

/// The colour targets of a surface pipeline: nothing blends, every channel is written.
#[must_use]
pub fn targets() -> Vec<Option<wgpu::ColorTargetState>> {
    TARGETS
        .iter()
        .map(|format| {
            Some(wgpu::ColorTargetState {
                format: *format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect()
}

/// The helper a pixel stage's text becomes.
fn helper(stage: &str) -> String {
    format!("hifi_stage_{stage}")
}

/// The anchors of the surface variant, applied to the re-shaded text of `splat`'s source: each
/// pixel stage stops being an entry point and becomes a helper.
#[must_use]
pub fn anchors(splat: bool) -> Vec<Anchor> {
    let mut stages: Vec<&'static str> = PIXEL_STAGES.to_vec();
    if splat {
        stages.push(SPLAT_STAGE);
    }
    stages
        .into_iter()
        .map(|stage| Anchor {
            name: "pixel-stage-helper",
            scope: Scope::Source,
            find: match stage {
                "ps_modulate" => "@fragment\nfn ps_modulate(i: VsOut) -> HifiOut {",
                "ps_selectarg1" => "@fragment\nfn ps_selectarg1(i: VsOut) -> HifiOut {",
                "ps_selectarg2" => "@fragment\nfn ps_selectarg2(i: VsOut) -> HifiOut {",
                "ps_premodulate" => "@fragment\nfn ps_premodulate(i: VsOut) -> HifiOut {",
                "ps_blendcurrentalpha" => {
                    "@fragment\nfn ps_blendcurrentalpha(i: VsOut) -> HifiOut {"
                }
                _ => "@fragment\nfn ps_splat(i: VsOut) -> HifiOut {",
            },
            with: format!("fn {}(i: VsOut) -> HifiOut {{", helper(stage)),
        })
        .collect()
}

/// The kind a draw of `format`'s non-splat pipelines writes when its fixed-function lighting is
/// off: the landscape's formats are the landscape, anything else is drawn as given.
fn unlit_kind(format: VertexFormat) -> &'static str {
    if has_normal(format) || format.is_pre_transformed() {
        SurfaceKind::Unlit.wgsl_name()
    } else {
        SurfaceKind::Terrain.wgsl_name()
    }
}

/// The draw's own point lights at the pixel, as the vertex stage sums them, and its emissive
/// colour, for a format with a normal.
fn radiance_fn(format: VertexFormat) -> &'static str {
    if has_normal(format) {
        "fn hifi_radiance_in(i: VsOut) -> VsOut {
    var j = i;
    j.fog = 1.0;
    if (g_draw.lighting_params.x < 0.5) {
        j.color = vec4<f32>(0.0, 0.0, 0.0, i.color.a);
        return j;
    }
    let bound = g_draw.material_lighting.z;
    let md = mix(i.unlit.rgb, vec3<f32>(g_draw.material_lighting.y), bound);
    let me = mix(vec3<f32>(g_draw.lighting_params.z), i.unlit.rgb, g_draw.lighting_params.w);
    let ln = length(i.world_n);
    let n = select(vec3<f32>(0.0, 1.0, 0.0), i.world_n / max(ln, 1e-20), ln > 1e-6);
    var sum = vec3<f32>(0.0);
    let count = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < 8; k = k + 1) {
        if (k < count) {
            let lp = g_draw.light_pos[k];
            let ld = g_draw.light_diffuse[k];
            if (ld.w != 3.0) {
                let d = lp.xyz - i.world_p;
                let dist = length(d);
                if (dist < lp.w) {
                    sum = sum + ld.rgb * max(dot(n, d / max(dist, 1e-6)), 0.0) / max(dist, 1e-6);
                }
            }
        }
    }
    j.color = vec4<f32>(me + md * sum, i.color.a);
    return j;
}
"
    } else {
        "fn hifi_radiance_in(i: VsOut) -> VsOut {
    var j = i;
    j.fog = 1.0;
    j.color = vec4<f32>(0.0, 0.0, 0.0, i.color.a);
    return j;
}
"
    }
}

/// The text the surface variant appends after the re-shaded prelude: its outputs, the inputs
/// of the three runs, and the pixel stages that make them.
#[must_use]
pub fn prelude(format: VertexFormat, splat: bool) -> String {
    let mut stages: Vec<(&str, &str)> = PIXEL_STAGES
        .iter()
        .map(|s| {
            (
                *s,
                // The texture alone, whatever the light: drawn as given.
                if *s == "ps_selectarg1" {
                    "HIFI_KIND_UNLIT"
                } else {
                    "hifi_kind()"
                },
            )
        })
        .collect();
    if splat {
        stages.push((SPLAT_STAGE, SurfaceKind::Terrain.wgsl_name()));
    }
    let entries: String = stages
        .iter()
        .map(|(stage, kind)| {
            let h = helper(stage);
            format!(
                "
@fragment
fn {stage}(i: VsOut) -> HifiSurface {{
    return hifi_surface({h}(i), {h}(hifi_albedo_in(i)), {h}(hifi_radiance_in(i)), {kind});
}}
"
            )
        })
        .collect();
    format!(
        "
// ---- the surface variant ------------------------------------------------------------------

struct HifiSurface {{
    // Linear albedo (stored as sRGB); alpha the surface kind over 8.
    @location(0) albedo: vec4<f32>,
    // Emissive light and the draw's own lights, linear; alpha the brightness of its sun.
    @location(1) radiance: vec4<f32>,
    // The ordinary colour, linear, after fog.
    @location(2) ordinary: vec4<f32>,
    // The view-space normal, each part from -1..1 to 0..1.
    @location(3) normal: vec4<f32>,
}};

{kinds}
// The brightness of the directional light among the draw's lights, zero when it has none.
fn hifi_sun_strength() -> f32 {{
    if (g_draw.lighting_params.x < 0.5) {{
        return 0.0;
    }}
    var s = 0.0;
    let count = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < 8; k = k + 1) {{
        if (k < count && g_draw.light_diffuse[k].w == 3.0) {{
            s = max(s, luminance(g_draw.light_diffuse[k].rgb));
        }}
    }}
    return s;
}}

fn hifi_kind() -> f32 {{
    if (g_draw.lighting_params.x > 0.5) {{
        if (g_draw.lighting_params.w > 0.5) {{
            return HIFI_KIND_BAKED;
        }}
        if (hifi_sun_strength() <= 0.0) {{
            return HIFI_KIND_INDOOR;
        }}
        return select(HIFI_KIND_LIT, HIFI_KIND_FOLIAGE, g_draw.draw_params.y > 0.5);
    }}
    return {unlit_kind};
}}

// The run that finds the albedo: the material's diffuse colour where the vertex is lit, white
// where it is not (the landscape's vertex colour is its baked light), and no fog.
fn hifi_albedo_in(i: VsOut) -> VsOut {{
    var j = i;
    j.fog = 1.0;
    if (g_draw.lighting_params.x > 0.5) {{
        let bound = g_draw.material_lighting.z;
        j.color = vec4<f32>(mix(i.unlit.rgb, vec3<f32>(g_draw.material_lighting.y), bound), i.color.a);
    }} else {{
        j.color = vec4<f32>(1.0, 1.0, 1.0, i.color.a);
    }}
    return j;
}}

{radiance}
fn hifi_surface(ordinary: HifiOut, albedo: HifiOut, radiance: HifiOut, kind: f32) -> HifiSurface {{
    var o: HifiSurface;
    o.albedo = vec4<f32>(clamp(albedo.colour.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), kind / 8.0);
    o.radiance = vec4<f32>(radiance.colour.rgb, hifi_sun_strength());
    o.ordinary = vec4<f32>(ordinary.colour.rgb, 1.0);
    o.normal = vec4<f32>(ordinary.normal.xyz * 0.5 + vec3<f32>(0.5), 1.0);
    return o;
}}
{entries}",
        kinds = SurfaceKind::wgsl(),
        unlit_kind = unlit_kind(format),
        radiance = radiance_fn(format),
    )
}
