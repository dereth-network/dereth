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
//! A room's surface takes the material's dampness: a little darker, brighter at a low angle for the
//! light on it, and its color's alpha, past 1, tells the light per pixel to add the room's lights'
//! highlight.
//!
//! Every picture is read where the view meets the material's height ([`LEVELS`] says how deep), so
//! near a viewer its crevices sink into the surface and its proud parts hide what is behind them.
//! The depth stays the surface's, so what is set into the surface is not seen through its relief.
//! From that point the height is walked toward the light, in a room the lights it is burned from,
//! or else the sun or a slanted light, so its proud parts shade what lies beside them.
//!
//! A fine picture many materials share, the grit, lies over the material at the size it has on the
//! surface, adding a bend, a speckle of its color and some roughness finer than the retail picture.
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
pub const OCCLUSION: f32 = 0.8;

/// The least and the greatest class a sunlit surface writes for its roughness: the light per
/// pixel's reading of an art pack's dielectric.
pub const ROUGHNESS_CODES: (f32, f32) = (1.02, 1.18);

/// The re-shaded read of the surface's picture, which this variant replaces.
const RESHADED_SAMPLE: &str = "textureSampleBias(g_tex0, g_samp0, i.uv0, g_frame.screen.y)";

/// The read of the picture where the view meets the material's height.
const MATERIAL_SAMPLE: &str = "hifi_mat_sample(g_tex0, i)";

/// What each depth mapping level draws: how far below the surface a material's height reaches, in
/// meters (none, no parallax and no self-shadow); how many of the steps it is searched in are
/// taken; how strongly the material's normal bends; and how strongly its grit shows. A material
/// with a base color of its own is drawn at the first with depth mapping off, its normal whole.
const LEVELS: [[f32; 4]; 4] = [
    [0.0, 1.0, 1.0, 0.0],
    [0.0, 1.0, 0.7, 0.6],
    [0.06, 0.5, 0.85, 0.8],
    [0.12, 1.0, 1.0, 1.0],
];

/// The constants the material variant reads for depth mapping level `level`; past the last, the
/// last.
#[must_use]
pub fn level_constants(level: u8) -> [f32; 4] {
    LEVELS[usize::from(level).min(LEVELS.len() - 1)]
}

/// How far a grit picture's speckle moves a surface's color, for its whole range.
pub const GRIT_SPECKLE: f32 = 0.6;

/// How many of the rooms' lights the material variant can see at once, the nearest the eye.
pub const ROOM_LIGHTS: usize = 32;

/// How squarely a surface must face the eye (the cosine) for its height to show at all.
pub const FLAT_AT: f32 = 0.1;

/// How squarely a surface must face the eye for its height to show whole.
pub const FULL_AT: f32 = 0.45;

/// How dark a material's height shades what it stands in the light of.
pub const SELF_SHADOW: f32 = 0.8;

/// How strongly a room's surface is shaded by its relief.
pub const ROOM_SHAPE: f32 = 1.5;

/// How much brighter a damp surface is drawn for what it mirrors, before its Fresnel term.
pub const DAMP_SHEEN: f32 = 1.6;

/// How far up a surface may face (its normal's height) and still be damp: walls, not floors.
pub const DAMP_WALL: f32 = 0.7;

/// How much of its dampness a floor that may be wet shows.
pub const WET_FLOOR: f32 = 0.3;

/// How much of its color a damp surface keeps.
pub const DAMP_DARK: f32 = 0.82;

/// The anchors the material variant applies to the re-shaded text.
#[must_use]
pub fn anchors() -> Vec<Anchor> {
    PIXEL_STAGES
        .iter()
        .flat_map(|stage| {
            [
                Anchor {
                    name: "material-return",
                    scope: Scope::Function(stage),
                    find: RESHADED_OUT,
                    with: MATERIAL_OUT.to_owned(),
                },
                Anchor {
                    name: "material-sample",
                    scope: Scope::Function(stage),
                    find: RESHADED_SAMPLE,
                    with: MATERIAL_SAMPLE.to_owned(),
                },
            ]
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
// x how deep the height reaches, meters; y the share of the search's steps; z the normal's strength.
@group({group}) @binding(2) var<uniform> g_mat_level: vec4<f32>;
@group({group}) @binding(3) var g_mat_grit: texture_2d<f32>;
@group({group}) @binding(4) var g_mat_grit_sampler: sampler;
struct HifiMatTile {{
    // xy how many times the grit repeats across the picture, z 1 where there is grit, w 1 where it
    // is wet on floors too.
    at: vec4<f32>,
    // 1 where a film of slime lies on it.
    slime: f32,
    // 1 where it is ice.
    ice: f32,
}};
@group({group}) @binding(5) var<uniform> g_mat_tile: HifiMatTile;

struct HifiMatRooms {{
    // x how many lights, y 1 while surfaces show their damp and slime.
    head: vec4<f32>,
    // xyz where it is, w how far it reaches.
    place: array<vec4<f32>, {room_lights}>,
    // x how bright it is.
    bright: array<vec4<f32>, {room_lights}>,
}};
@group({group}) @binding(6) var<uniform> g_mat_rooms: HifiMatRooms;

// Where the eye is.
fn hifi_mat_eye() -> vec3<f32> {{
    // The view is held so that a point's view coordinate k is dot(p, v[k].xyz) + v[k].w.
    let v = g_frame.view;
    return -(v[0].xyz * v[0].w + v[1].xyz * v[1].w + v[2].xyz * v[2].w);
}}

var<private> hifi_mat_found: bool = false;
var<private> hifi_mat_at: vec2<f32> = vec2<f32>(0.0);
var<private> hifi_mat_per_u: vec3<f32> = vec3<f32>(0.0);
var<private> hifi_mat_per_v: vec3<f32> = vec3<f32>(0.0);
var<private> hifi_mat_fade: f32 = 0.0;
var<private> hifi_mat_level: f32 = 0.0;
var<private> hifi_mat_hit: f32 = 0.0;

// Where the view from the eye to the pixel meets the material's height, in the picture's
// coordinates: the coordinates themselves past the distance the depth shows at.
fn hifi_mat_uv(i: VsOut) -> vec2<f32> {{
    if (hifi_mat_found) {{
        return hifi_mat_at;
    }}
    hifi_mat_found = true;
    hifi_mat_at = i.uv0;
    let n = {own};
    let dp1 = dpdx(i.world_p);
    let dp2 = dpdy(i.world_p);
    let duv1 = dpdx(i.uv0);
    let duv2 = dpdy(i.uv0);
    let perp2 = cross(dp2, n);
    let perp1 = cross(n, dp1);
    let det = dot(dp1, perp2);
    let to_eye = hifi_mat_eye() - i.world_p;
    let dist = length(to_eye);
    let to = to_eye / max(dist, 1e-3);
    let facing = dot(to, n);
    // Seen nearly along it, a surface's shift grows without bound and tears: it flattens instead.
    let fade = (1.0 - smoothstep(10.0, 30.0, dist)) * smoothstep({flat_at:.2}, {full_at:.2}, facing);
    if (abs(det) < 1e-14 || fade <= 0.0 || g_mat_level.x <= 0.0) {{
        return hifi_mat_at;
    }}
    // The picture's coordinates change this much per meter along the surface.
    let per_u = (perp2 * duv1.x + perp1 * duv2.x) / det;
    let per_v = (perp2 * duv1.y + perp1 * duv2.y) / det;
    let along = -(to - n * facing) / max(facing, {full_at:.2}) * g_mat_level.x * fade;
    let span = vec2<f32>(dot(along, per_u), dot(along, per_v));
    let side = vec2<f32>(textureDimensions(g_mat_surface));
    let level = max(0.0, 0.5 * log2(max(dot(duv1 * side, duv1 * side), dot(duv2 * side, duv2 * side))));
    hifi_mat_per_u = per_u;
    hifi_mat_per_v = per_v;
    hifi_mat_fade = fade;
    hifi_mat_level = level;
    let steps = (8.0 + 24.0 * (1.0 - facing)) * g_mat_level.y;
    var was_depth = 0.0;
    var was_under = 1.0;
    var depth = 0.0;
    for (var k = 0; k < 32; k = k + 1) {{
        let under = 1.0 - textureSampleLevel(g_mat_surface, g_samp0, i.uv0 + span * depth, level).b;
        if (depth >= under || depth >= 1.0) {{
            let a = was_under - was_depth;
            let b = depth - under;
            let f = a / max(a + b, 1e-4);
            hifi_mat_hit = mix(was_depth, depth, clamp(f, 0.0, 1.0));
            hifi_mat_at = i.uv0 + span * hifi_mat_hit;
            break;
        }}
        was_depth = depth;
        was_under = under;
        depth = depth + 1.0 / steps;
    }}
    return hifi_mat_at;
}}

// `picture` where the view meets the material's height, at the level the surface shows it at.
fn hifi_mat_sample(picture: texture_2d<f32>, i: VsOut) -> vec4<f32> {{
    let bias = pow(2.0, g_frame.screen.y);
    return textureSampleGrad(picture, g_samp0, hifi_mat_uv(i), dpdx(i.uv0) * bias, dpdy(i.uv0) * bias);
}}

// The lights of the rooms on a surface at `p` facing `n`, added as directions weighed by how much
// of each reaches it, so a surface between two lights turns smoothly from one to the other.
fn hifi_mat_room_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {{
    var sum = vec3<f32>(0.0);
    let count = min(i32(g_mat_rooms.head.x), {room_lights});
    for (var k: i32 = 0; k < count; k = k + 1) {{
        let d = g_mat_rooms.place[k].xyz - p;
        let dist = length(d);
        let x = dist / max(g_mat_rooms.place[k].w, 1e-3);
        if (x < 1.0 && dist > 1e-3) {{
            let w = 1.0 - x * x;
            let l = d / dist;
            sum = sum + l * g_mat_rooms.bright[k].x * w * w * max(dot(l, n), 0.0);
        }}
    }}
    return sum;
}}

// Where the light on a surface at `p` facing `n` comes from: in a room, the lights it is burned
// from; the sun among the draw's lights; or else a light slanted across all three axes, so that it
// rakes floors and walls alike.
fn hifi_mat_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {{
    if (g_draw.lighting_params.w > 0.5) {{
        let room = hifi_mat_room_light(p, n);
        if (dot(room, room) > 1e-8) {{
            return normalize(room);
        }}
    }}
    let count = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < 8; k = k + 1) {{
        if (k < count && g_draw.light_diffuse[k].w == 3.0) {{
            return -normalize(g_draw.light_pos[k].xyz);
        }}
    }}
    return normalize(vec3<f32>(0.6, 0.45, 0.65) + n * 0.3);
}}

// How much of the light on a surface at `p` facing `n` reaches the point the view met: 1 all of it, less
// where the height stands between that point and the light.
fn hifi_mat_self_shadow(p: vec3<f32>, n: vec3<f32>) -> f32 {{
    if (hifi_mat_fade <= 0.0 || hifi_mat_hit <= 0.0) {{
        return 1.0;
    }}
    let l = hifi_mat_light(p, n);
    let facing = dot(l, n);
    if (facing < 0.05) {{
        return 1.0;
    }}
    let along = (l - n * facing) / max(facing, 0.33) * g_mat_level.x * hifi_mat_fade;
    let span = vec2<f32>(dot(along, hifi_mat_per_u), dot(along, hifi_mat_per_v));
    var shade = 0.0;
    for (var k = 1; k <= 10; k = k + 1) {{
        let t = f32(k) / 10.0;
        let ray = hifi_mat_hit * (1.0 - t);
        let at = hifi_mat_at + span * hifi_mat_hit * t;
        let under = 1.0 - textureSampleLevel(g_mat_surface, g_samp0, at, hifi_mat_level).b;
        shade = max(shade, (ray - under) * 7.0 * (1.0 - t * 0.5));
    }}
    return 1.0 - {self_shadow:.2} * clamp(shade, 0.0, 1.0) * hifi_mat_fade;
}}

// The material's bend at a texel of its normal picture, `strength` times as far: x along the picture's width, y up it,
// z out of it.
fn hifi_mat_bend(n: vec4<f32>, strength: f32) -> vec3<f32> {{
    let xy = (n.xy * 2.0 - vec2<f32>(1.0)) * strength;
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

// How much brighter a damp surface at `i` facing `n` is drawn for what it mirrors: more at a low
// angle, and in proportion to the light on it, so that what is dark stays dark.
fn hifi_mat_damp(i: VsOut, n: vec3<f32>) -> f32 {{
    let to_eye = normalize(hifi_mat_eye() - i.world_p);
    return (0.1 + 0.9 * pow(1.0 - max(dot(n, to_eye), 0.0), 4.0)) * {damp_sheen:.2};
}}

// How a room's surface is shaded by its relief. The light burned into its vertices knows nothing of
// the relief, so it is shaded as if lit slantwise and from the eye, relative to the surface's own
// facing: what is dark stays dark, and the relief shows wherever there is light on it.
fn hifi_mat_shape(i: VsOut, own: vec3<f32>, bent: vec3<f32>) -> vec3<f32> {{
    let l = normalize(normalize(hifi_mat_eye() - i.world_p) + vec3<f32>(0.5, 0.4, 0.5));
    return vec3<f32>(clamp(1.0 + {shape:.2} * (dot(bent, l) - dot(own, l)), 0.5, 1.45));
}}

// The class a surface of class `kind` writes with the material's roughness `rough`: a surface lit by
// the sun carries it, every other its own class.
fn hifi_mat_class(kind: f32, rough: f32) -> f32 {{
    return select(kind, {lo:.2} + ({hi:.2} - {lo:.2}) * clamp(rough, 0.0, 1.0), kind == {lit});
}}

// The grit where the view meets the material's height, at the level the surface shows it at.
fn hifi_mat_grit_sample(i: VsOut) -> vec4<f32> {{
    let bias = pow(2.0, g_frame.screen.y);
    let tile = g_mat_tile.at.xy;
    return textureSampleGrad(g_mat_grit, g_mat_grit_sampler, hifi_mat_uv(i) * tile, dpdx(i.uv0) * tile * bias, dpdy(i.uv0) * tile * bias);
}}

// The material's bend `m` with the grit's added, `strength` times as far.
fn hifi_mat_with_grit(m: vec3<f32>, grit: vec4<f32>, strength: f32) -> vec3<f32> {{
    let xy = m.xy + (grit.xy * 2.0 - vec2<f32>(1.0)) * strength;
    let l = max(length(xy), 1.0);
    let held = xy / l * 0.98;
    return vec3<f32>(held, sqrt(max(1.0 - dot(held, held), 0.0)));
}}

// How much of a surface facing `n`'s dampness shows: on walls, and on floors a little if it says so;
// a ceiling's would light up as a pale sheet.
fn hifi_mat_wet_here(n: vec3<f32>) -> f32 {{
    let wall = 1.0 - smoothstep({wall_most:.2} * 0.6, {wall_most:.2}, abs(n.z));
    return max(wall, g_mat_tile.at.w * {wet_floor:.2} * step(0.0, n.z));
}}

// Glacier ice: opaque and deep blue, paler where light passes through it to the eye. That is where
// the view, bent as it enters, runs toward the sun, so each crystal plane of `n` catches its own
// patch. The surface mirrors the sky at a glancing angle and the sun in a glint.
fn hifi_mat_ice(i: VsOut, n: vec3<f32>, body: vec3<f32>) -> vec4<f32> {{
    let v = normalize(hifi_mat_eye() - i.world_p);
    let facing = saturate(dot(n, v));
    let r = reflect(-v, n);
    var sun = vec3<f32>(0.0, 0.0, 1.0);
    var colour = vec3<f32>(1.0);
    let count = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < min(count, 8); k = k + 1) {{
        if (g_draw.light_diffuse[k].w == 3.0) {{
            sun = -normalize(g_draw.light_pos[k].xyz);
            colour = g_draw.light_diffuse[k].rgb;
            break;
        }}
    }}
    let bent = refract(-v, n, 1.0 / 1.31);
    let through = pow(saturate(0.5 + 0.5 * dot(bent, sun)), 5.0);
    let tone = dot(body, vec3<f32>(0.3, 0.59, 0.11));
    let deep = vec3<f32>(0.01, 0.15, 0.36);
    let lit = vec3<f32>(0.05, 0.52, 0.76);
    let inside = mix(deep, lit, saturate(1.6 * through + 0.5 * (tone - 0.45)));
    let up = mix(g_frame.fog_color.rgb, vec3<f32>(0.32, 0.58, 1.0), smoothstep(0.0, 0.6, r.z));
    let sky = mix(vec3<f32>(0.78, 0.8, 0.84), up, smoothstep(-0.25, 0.05, r.z));
    let mirror = 0.03 + 0.42 * pow(1.0 - facing, 4.0);
    let d = max(dot(r, sun), 0.0);
    let glint = colour * (3.5 * pow(d, 300.0) + 0.25 * pow(d, 40.0));
    return vec4<f32>(mix(inside, sky, mirror) + glint, 1.0);
}}

fn hifi_material_out(c: vec4<f32>, i: VsOut, material: f32) -> HifiOut {{
    // Every picture and every change across the pixel is read before anything branches.
    let surface = hifi_mat_sample(g_mat_surface, i);
    let bend = hifi_mat_bend(hifi_mat_sample(g_mat_normal, i), g_mat_level.z);
    let own = {own};
    let grit = hifi_mat_grit_sample(i);
    let strength = g_mat_level.w * g_mat_tile.at.z;
    let bent = hifi_mat_bent(own, i.world_p, i.uv0, hifi_mat_with_grit(bend, grit, strength));
    let lit = g_draw.lighting_params.x > 0.5;
    let solid = {solid};
    var shaded = c;
    if (solid) {{
        let speck = 1.0 + (grit.b * 2.0 - 1.0) * {speckle:.2} * strength;
        var k = vec3<f32>(mix(1.0, surface.g, {occlusion:.2}) * hifi_mat_self_shadow(i.world_p, own) * speck);
        if (lit) {{
            k = k * hifi_mat_relight(i, own, bent);
        }}
        if (lit && g_draw.lighting_params.w > 0.5) {{
            k = k * hifi_mat_shape(i, own, bent);
        }}
        shaded = vec4<f32>(c.rgb * k, c.a);
        let damp = surface.a * hifi_mat_wet_here(own) * g_mat_rooms.head.y;
        if (lit && g_draw.lighting_params.w > 0.5 && damp > 0.0) {{
            let wet = mix(1.0, {damp_dark:.2} + hifi_mat_damp(i, bent), damp);
            shaded = vec4<f32>(shaded.rgb * wet, c.a);
        }}
    }}
    let ice = solid && g_mat_tile.ice > 0.5;
    if (ice) {{
        shaded = hifi_mat_ice(i, bent, c.rgb);
    }}
    var o = hifi_out(shaded, i, material);
    if (solid && !ice && lit && g_draw.lighting_params.w > 0.5) {{
        // Its bumps and edges catch the light; a flat face would only wash pale.
        o.colour.a = 1.0 + surface.a * g_mat_rooms.head.y * hifi_mat_wet_here(own) * mix(0.15, 1.0, smoothstep(0.03, 0.3, length(bend.xy)));
        // Past 2, how much slime: it lies on ceilings too, and the room's light pass shapes it.
        if (g_mat_tile.slime > 0.5 && g_mat_rooms.head.y > 0.5 && surface.a > 0.0) {{
            o.colour.a = 2.02 + surface.a;
        }}
    }}
    if (solid && dot(o.normal.xyz, o.normal.xyz) > 0.25) {{
        let view_n = normalize((vec4<f32>(bent, 0.0) * g_frame.view).xyz);
        let rough = surface.r + (grit.a * 2.0 - 1.0) * strength;
        o.normal = vec4<f32>(view_n, hifi_mat_class(o.normal.w, rough));
    }}
    return o;
}}
",
        group = MATERIAL_GROUP,
        room_lights = ROOM_LIGHTS,
        speckle = GRIT_SPECKLE,
        wall_most = DAMP_WALL,
        wet_floor = WET_FLOOR,
        flat_at = FLAT_AT,
        full_at = FULL_AT,
        shape = ROOM_SHAPE,
        self_shadow = SELF_SHADOW,
        damp_sheen = DAMP_SHEEN,
        damp_dark = DAMP_DARK,
        lo = ROUGHNESS_CODES.0,
        hi = ROUGHNESS_CODES.1,
        lit = super::reshade::MaterialClass::Lit.wgsl_name(),
        occlusion = OCCLUSION,
    )
}
