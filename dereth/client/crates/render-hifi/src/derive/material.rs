//! The material variant: the re-shaded stages of a retail surface drawn with an art pack's
//! material in place of its picture.
//!
//! The material's base color is bound where the surface's picture was, so every stage computes
//! with it exactly as with the retail picture: the vertex color, the alpha test, the detail
//! grain, fog and brightness are the surface's own. A fifth group holds the rest of the material,
//! sampled as the surface samples its picture (its sampler, its coordinates, its bias): its
//! normal (x along the picture's width, y up it) and its roughness and occlusion.
//!
//! Each pixel stage then ends in [`MATERIAL_OUT`] in place of the re-shaded output:
//! - on a lit surface that is neither cut out nor unlit, the surface's normal is bent by the
//!   material's, in the frame the picture lies in on the surface (found from how the position and
//!   the coordinates change across the pixel), and written to the normal target in its place, so
//!   the light per pixel and the rooms' own lights take the bend;
//! - the vertex light is worked out again for the bent normal, as the vertex stage works it out,
//!   and the color is drawn with it in place of the light the vertices held, so the bend shows
//!   under the surface's own lights and the light per pixel divides out the light it drew with;
//! - the occlusion darkens the color a little;
//! - a surface lit by the sun carries the material's roughness in its class (1.02 to 1.18), so
//!   the light per pixel gives it the sun's reflection.
//!
//! A cut-out or unlit surface takes the material's color alone.

use dereth_render::VertexFormat;

use super::reshade::{has_normal, PIXEL_STAGES};
use super::{Anchor, Scope};

/// The re-shaded output each pixel stage ends in, which this variant replaces.
const RESHADED_OUT: &str = "return hifi_out(c, i, hifi_class());";

/// The material's output, in its place.
pub const MATERIAL_OUT: &str = "return hifi_material_out(c, i, hifi_class());";

/// The group the material's own pictures are bound at.
pub const MATERIAL_GROUP: u32 = 4;

/// How much of the material's occlusion darkens it.
pub const OCCLUSION: f32 = 0.5;

/// The least and the greatest class a sunlit surface writes for its roughness: the light per
/// pixel's reading of an art pack's dielectric.
pub const ROUGHNESS_CODES: (f32, f32) = (1.02, 1.18);

/// The anchors the material variant applies to the re-shaded text.
#[must_use]
pub fn anchors() -> Vec<Anchor> {
    PIXEL_STAGES
        .iter()
        .map(|stage| Anchor {
            name: "material-return",
            scope: Scope::Function(stage),
            find: RESHADED_OUT,
            with: MATERIAL_OUT.to_owned(),
        })
        .collect()
}

/// The text the material variant appends to the re-shaded text for `format`.
#[must_use]
pub fn prelude(format: VertexFormat) -> String {
    // A format with no normal is never lit: it takes the material's color alone.
    let (own, solid) = if has_normal(format) {
        (
            "normalize(i.world_n)",
            format!(
                "material != {} && material != {} && dot(i.world_n, i.world_n) > 1e-12",
                super::reshade::MaterialClass::Foliage.wgsl_name(),
                super::reshade::MaterialClass::Unlit.wgsl_name(),
            ),
        )
    } else {
        ("vec3<f32>(0.0, 0.0, 1.0)", "false".to_owned())
    };
    format!(
        "
// ---- the art pack's material ------------------------------------------------------------

@group({group}) @binding(0) var g_mat_normal: texture_2d<f32>;
@group({group}) @binding(1) var g_mat_surface: texture_2d<f32>;

// The material's bend at a texel of its normal picture: x along the picture's width, y up it,
// z out of it.
fn hifi_mat_bend(n: vec4<f32>) -> vec3<f32> {{
    let xy = n.xy * 2.0 - vec2<f32>(1.0);
    return vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}}

// The normal `n` (a unit vector) bent by `m`, in the frame the picture lies in on the surface
// where it is at `p` with coordinates `uv`: found from how the two change across the pixel. Where
// they do not tell, `n` as it is.
fn hifi_mat_bent(n: vec3<f32>, p: vec3<f32>, uv: vec2<f32>, m: vec3<f32>) -> vec3<f32> {{
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    // Along the picture's width, and down it (its coordinates run down the picture).
    let t = dp2perp * duv1.x + dp1perp * duv2.x;
    let b = dp2perp * duv1.y + dp1perp * duv2.y;
    let scale = max(dot(t, t), dot(b, b));
    if (scale < 1e-30) {{
        return n;
    }}
    let k = inverseSqrt(scale);
    let bent = t * (k * m.x) - b * (k * m.y) + n * m.z;
    let l = length(bent);
    return select(n, bent / max(l, 1e-20), l > 1e-6);
}}

// The light the vertex stage gives a surface at `i` facing `n` (world space): the emissive, the
// ambient, and each of the draw's lights, as it lights a vertex.
fn hifi_mat_vertex_light(i: VsOut, n: vec3<f32>) -> vec3<f32> {{
    let color = i.unlit;
    let bound = g_draw.material_lighting.z;
    let Md = mix(color.rgb, vec3<f32>(g_draw.material_lighting.y), bound);
    let Ma = mix(color.rgb, vec3<f32>(1.0), bound);
    let Me = mix(vec3<f32>(g_draw.lighting_params.z), color.rgb, g_draw.lighting_params.w);
    var sum = vec3<f32>(0.0);
    let count = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < 8; k = k + 1) {{
        if (k < count) {{
            let lp = g_draw.light_pos[k];
            let ld = g_draw.light_diffuse[k];
            var l = vec3<f32>(0.0);
            var atten = 1.0;
            if (ld.w == 3.0) {{
                l = -normalize(lp.xyz);
            }} else {{
                let d = lp.xyz - i.world_p;
                let dist = length(d);
                if (dist >= lp.w) {{
                    continue;
                }}
                l = d / max(dist, 1e-6);
                atten = 1.0 / max(dist, 1e-6);
            }}
            sum = sum + ld.rgb * max(dot(n, l), 0.0) * atten;
        }}
    }}
    return saturate(Me + Ma * g_frame.ambient.rgb + Md * sum);
}}

// How much brighter a lit surface is drawn for its bent normal `bent` than for its own `own`:
// the vertex light worked out for each, within a half and twice.
fn hifi_mat_relight(i: VsOut, own: vec3<f32>, bent: vec3<f32>) -> vec3<f32> {{
    let with_own = hifi_mat_vertex_light(i, own);
    let with_bend = hifi_mat_vertex_light(i, bent);
    return clamp(with_bend / max(with_own, vec3<f32>(1e-3)), vec3<f32>(0.5), vec3<f32>(2.0));
}}

// The class a surface of class `kind` writes with the material's roughness `rough`: a surface lit by
// the sun carries it, every other its own class.
fn hifi_mat_class(kind: f32, rough: f32) -> f32 {{
    return select(kind, {lo:.2} + ({hi:.2} - {lo:.2}) * clamp(rough, 0.0, 1.0), kind == {lit});
}}

fn hifi_material_out(c: vec4<f32>, i: VsOut, material: f32) -> HifiOut {{
    // Every picture and every change across the pixel is read before anything branches.
    let surface = textureSampleBias(g_mat_surface, g_samp0, i.uv0, g_frame.screen.y);
    let bend = hifi_mat_bend(textureSampleBias(g_mat_normal, g_samp0, i.uv0, g_frame.screen.y));
    let own = {own};
    let bent = hifi_mat_bent(own, i.world_p, i.uv0, bend);
    let lit = g_draw.lighting_params.x > 0.5;
    let solid = {solid};
    var shaded = c;
    if (solid) {{
        var k = vec3<f32>(mix(1.0, surface.g, {occlusion:.2}));
        if (lit) {{
            k = k * hifi_mat_relight(i, own, bent);
        }}
        shaded = vec4<f32>(c.rgb * k, c.a);
    }}
    var o = hifi_out(shaded, i, material);
    if (solid && dot(o.normal.xyz, o.normal.xyz) > 0.25) {{
        let view_n = normalize((vec4<f32>(bent, 0.0) * g_frame.view).xyz);
        o.normal = vec4<f32>(view_n, hifi_mat_class(o.normal.w, surface.r));
    }}
    return o;
}}
",
        group = MATERIAL_GROUP,
        lo = ROUGHNESS_CODES.0,
        hi = ROUGHNESS_CODES.1,
        lit = super::reshade::MaterialClass::Lit.wgsl_name(),
        occlusion = OCCLUSION,
    )
}
