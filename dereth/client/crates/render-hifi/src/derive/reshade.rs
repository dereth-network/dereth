//! The re-shaded variant: the ordinary stages, with their colour written as linear light and two
//! outputs where the ordinary path has one.
//!
//! - The vertex stage hands on, beside what it always did, the world position, the world normal
//!   (for a format that has one), the vertex colour before lighting, and the lighting's diffuse
//!   and emissive terms before they are added and clamped, for passes that light per pixel.
//! - The stage chains, the alpha test, the detail grain, the selection blink, the landscape's
//!   layer blend and the normal transform are the ordinary text, computed on the colours as they
//!   are stored, exactly as the ordinary stage computes them.
//! - Every pixel stage ends in two outputs (see Outputs below): the ordinary colour, after fog and
//!   the brightness slider, decoded from sRGB to linear light, with the ordinary alpha for the
//!   ordinary blend; and the view-space normal with the material class.
//!
//! The stage chains are not computed in linear light: sRGB is not multiplicative, so a modulate
//! or a layer blend in linear light would come out up to a dozen levels away from the ordinary
//! picture. Shading as the ordinary stage does and decoding the result keeps the re-shaded world
//! the ordinary one, written through a plain linear-to-sRGB encode, except where the hardware
//! blends, which it does in linear light; a pass that lights per pixel takes the material from the
//! vertex outputs and decodes it itself.
//!
//! # Outputs
//!
//! Location 0 is the colour; location 1 is the normal, `xyz` the unit view-space normal (zero
//! where there is none) and `w` the [`MaterialClass`] as a small whole number. A lit draw whose
//! lights hold no sun (a thing standing in a building's or a dungeon's cell, lit by the cell's
//! lights) writes its class plus [`WITHOUT_SUN`].

use dereth_render::VertexFormat;

use super::{Anchor, Scope};
use crate::shared::fullscreen::COLOUR_WGSL;

/// What a [`MaterialClass::Lit`] draw adds to its class when none of its lights is the sun: it
/// was lit as a thing indoors is.
pub const WITHOUT_SUN: f32 = 0.25;

/// What kind of surface a re-shaded pixel is, as its normal target's `w` carries it.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialClass {
    /// Drawn with its colour as given: the sky, effects, the interface's world pieces, landscape
    /// drawn from its composite.
    Unlit = 0,
    /// Lit per vertex by the scene's lights.
    Lit = 1,
    /// Lit, with the vertex colour as the static light baked into an interior cell.
    Baked = 2,
    /// Landscape drawn by the splat.
    Terrain = 3,
    /// Lit, and cut out against its texture's alpha: leaves, grass and other thin growth.
    Foliage = 4,
}

impl MaterialClass {
    /// Every class.
    pub const ALL: [Self; 5] = [
        Self::Unlit,
        Self::Lit,
        Self::Baked,
        Self::Terrain,
        Self::Foliage,
    ];

    /// The value the normal target's `w` holds for this class.
    #[must_use]
    pub const fn encoded(self) -> f32 {
        self as u8 as f32
    }

    /// The class a normal target's `w` holds, if it is one.
    #[must_use]
    pub fn from_encoded(w: f32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.encoded() == w)
    }

    /// The constant's name in the derived shader text.
    #[must_use]
    pub const fn wgsl_name(self) -> &'static str {
        match self {
            Self::Unlit => "HIFI_CLASS_UNLIT",
            Self::Lit => "HIFI_CLASS_LIT",
            Self::Baked => "HIFI_CLASS_BAKED",
            Self::Terrain => "HIFI_CLASS_TERRAIN",
            Self::Foliage => "HIFI_CLASS_FOLIAGE",
        }
    }
}

/// The pixel stages of the ordinary source, by entry point.
pub const PIXEL_STAGES: [&str; 5] = [
    "ps_modulate",
    "ps_selectarg1",
    "ps_selectarg2",
    "ps_premodulate",
    "ps_blendcurrentalpha",
];

/// The splat's pixel stage.
pub const SPLAT_STAGE: &str = "ps_splat";

/// The ordinary pixel stage's output declaration, ending its header.
const PIXEL_OUTPUT: &str = ") -> @location(0) vec4<f32> {";
/// What the derived pixel stage declares instead.
const DERIVED_OUTPUT: &str = ") -> HifiOut {";

/// Whether `format` carries a vertex normal.
#[must_use]
pub const fn has_normal(format: VertexFormat) -> bool {
    matches!(
        format,
        VertexFormat::XyzNormalDiffuseTex1 | VertexFormat::XyzNormalDiffuseTex2
    )
}

/// Whether the ordinary vertex stage of `format` lights its vertices.
#[must_use]
pub const fn is_lit(format: VertexFormat) -> bool {
    has_normal(format) && !format.is_pre_transformed()
}

/// The fields the derived vertex stage adds to its output.
fn added_fields(format: VertexFormat) -> String {
    let mut fields = String::from(
        "struct VsOut {
    // The world position, for the surface normal where the format has none and for passes that
    // light per pixel; zero for a pre-transformed draw.
    @location(4) world_p: vec3<f32>,
    // The vertex colour as it arrived, before any lighting.
    @location(5) unlit: vec4<f32>,
    // The lighting's diffuse term and emissive colour, before they are added and clamped; zero
    // where the vertex is not lit.
    @location(6) lit_diffuse: vec3<f32>,
    @location(7) lit_emissive: vec3<f32>,
    // How much of the directional light the vertex took, 0..1 (the N.L its lighting used, weighted
    // by each light's brightness); negative where the vertex is not lit by one.
    @location(9) sun_share: f32,
",
    );
    if has_normal(format) {
        fields.push_str(
            "    // The vertex normal in world space, under the draw's scale as the lighting takes it.
    @location(8) world_n: vec3<f32>,
",
        );
    }
    fields
}

/// What the vertex stage writes before it returns.
fn world_outputs(format: VertexFormat) -> String {
    let mut out = String::new();
    if !format.is_pre_transformed() {
        out.push_str("    o.world_p = world.xyz;\n");
        if has_normal(format) {
            out.push_str(
                "    let hifi_nrm = select(i.normal, i.normal * g_draw.normal_scale.xyz, g_draw.normal_scale.w > 0.5);
    o.world_n = (vec4<f32>(hifi_nrm, 0.0) * g_draw.world).xyz;
",
            );
        }
    }
    out.push_str("    return o;\n");
    out
}

/// The anchors of the re-shaded variant for `format`, with the splat's when `splat`.
#[must_use]
pub fn anchors(format: VertexFormat, splat: bool) -> Vec<Anchor> {
    let mut a = vec![
        Anchor {
            name: "vertex-outputs",
            scope: Scope::Source,
            find: "struct VsOut {\n",
            with: added_fields(format),
        },
        Anchor {
            name: "unlit-colour",
            scope: Scope::Function("vs_main"),
            find: "    o.color = color;\n",
            with: "    o.color = color;\n    o.unlit = color;\n    o.sun_share = -1.0;\n"
                .to_owned(),
        },
    ];
    if is_lit(format) {
        a.push(Anchor {
            name: "lighting-terms",
            scope: Scope::Function("vs_main"),
            find: "        let lit = saturate(Me + Ma * g_frame.ambient.rgb + Md * sum);\n",
            with: "        let lit = saturate(Me + Ma * g_frame.ambient.rgb + Md * sum);
        o.lit_diffuse = Md * sum;
        o.lit_emissive = Me;
        var hifi_sun = 0.0;
        var hifi_sun_full = 0.0;
        for (var k: i32 = 0; k < 8; k = k + 1) {
            if (k < n && g_draw.light_diffuse[k].w == 3.0) {
                let w = dot(g_draw.light_diffuse[k].rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
                hifi_sun_full = hifi_sun_full + w;
                hifi_sun = hifi_sun + w * max(dot(N, -normalize(g_draw.light_pos[k].xyz)), 0.0);
            }
        }
        if (hifi_sun_full > 1e-4) {
            o.sun_share = hifi_sun / hifi_sun_full;
        }
"
            .to_owned(),
        });
    }
    a.push(Anchor {
        name: "world-outputs",
        scope: Scope::Function("vs_main"),
        find: "    return o;\n",
        with: world_outputs(format),
    });
    for stage in PIXEL_STAGES {
        a.push(Anchor {
            name: "pixel-output",
            scope: Scope::Function(stage),
            find: PIXEL_OUTPUT,
            with: DERIVED_OUTPUT.to_owned(),
        });
        a.push(Anchor {
            name: "pixel-return",
            scope: Scope::Function(stage),
            find: "return finish(c, i.fog);",
            with: "return hifi_out(c, i, hifi_class());".to_owned(),
        });
    }
    if splat {
        a.extend([
            Anchor {
                name: "pixel-output",
                scope: Scope::Function(SPLAT_STAGE),
                find: PIXEL_OUTPUT,
                with: DERIVED_OUTPUT.to_owned(),
            },
            Anchor {
                name: "pixel-return",
                scope: Scope::Function(SPLAT_STAGE),
                find: "return finish(lit, i.fog);",
                with: format!(
                    "return hifi_out(lit, i, {});",
                    MaterialClass::Terrain.wgsl_name()
                ),
            },
        ]);
    }
    a
}

/// The view-space normal: the vertex normal where the format has one and it is not degenerate,
/// otherwise the face's, turned towards the eye.
fn view_normal(format: VertexFormat) -> &'static str {
    if has_normal(format) {
        "fn hifi_view_normal(i: VsOut) -> vec3<f32> {
    let face = hifi_face_normal(i);
    let n = (vec4<f32>(i.world_n, 0.0) * g_frame.view).xyz;
    let l = length(n);
    return select(face, n / max(l, 1e-20), l > 1e-6);
}
"
    } else {
        "fn hifi_view_normal(i: VsOut) -> vec3<f32> {
    return hifi_face_normal(i);
}
"
    }
}

/// The text the re-shaded variant appends: its output, the colour conversions and the helpers
/// the anchors call.
#[must_use]
pub fn prelude(format: VertexFormat) -> String {
    let classes: String = MaterialClass::ALL
        .iter()
        .map(|c| format!("const {}: f32 = {:.1};\n", c.wgsl_name(), c.encoded()))
        .collect();
    format!(
        "
// ---- the re-shaded output ----------------------------------------------------------------

struct HifiOut {{
    // Linear light, unclamped; the alpha is the ordinary stage's, for the ordinary blend.
    @location(0) colour: vec4<f32>,
    // xyz = the unit view-space normal, or zero where there is none; w = the material class.
    @location(1) normal: vec4<f32>,
}};

{classes}
{COLOUR_WGSL}
fn hifi_linear_rgb(c: vec3<f32>) -> vec3<f32> {{
    return srgb_to_linear(max(c, vec3<f32>(0.0)));
}}

// Whether the sun is among the lights this draw was lit with.
fn hifi_sun_among_lights() -> bool {{
    let n = i32(g_draw.lighting_params.y);
    for (var k: i32 = 0; k < 8; k = k + 1) {{
        if (k < n && g_draw.light_diffuse[k].w > 2.5) {{
            return true;
        }}
    }}
    return false;
}}

fn hifi_class() -> f32 {{
    let cut_out = g_draw.draw_params.y > 0.5;
    if (g_draw.lighting_params.x > 0.5) {{
        if (g_draw.lighting_params.w > 0.5) {{
            return {baked};
        }}
        if (!cut_out && !hifi_sun_among_lights()) {{
            return {lit} + {without_sun};
        }}
        return select({lit}, {foliage}, cut_out);
    }}
    return select({unlit}, {foliage}, cut_out);
}}

// The face's normal in view space from the screen-space change of its position, turned towards
// the eye; zero where the position does not change (a pre-transformed draw).
fn hifi_face_normal(i: VsOut) -> vec3<f32> {{
    let p = (vec4<f32>(i.world_p, 1.0) * g_frame.view).xyz;
    let n = cross(dpdx(p), dpdy(p));
    let l = length(n);
    let u = select(vec3<f32>(0.0), n / max(l, 1e-20), l > 1e-20);
    return select(u, -u, dot(u, p) > 0.0);
}}

{view_normal}
// A leaf card's normal as the light should see it: the plant is a volume, not a stack of flat
// cards. Mostly the direction out from a point on the plant's own axis, half as high as the leaf
// stands over the plant's root (so the crown faces the sky and the flanks face out), with a
// quarter of the card's own facing, turned toward the eye, so each card keeps a little shape.
fn hifi_canopy_normal(i: VsOut, card: vec3<f32>) -> vec3<f32> {{
    let root = (vec4<f32>(0.0, 0.0, 0.0, 1.0) * g_draw.world).xyz;
    let r = i.world_p - root;
    let out = vec3<f32>(r.x, r.y, r.z * 0.5);
    let l = length(out);
    if (l < 1e-3 || dot(card, card) < 0.25) {{
        return card;
    }}
    let radial = normalize((vec4<f32>(out / l, 0.0) * g_frame.view).xyz);
    let p = (vec4<f32>(i.world_p, 1.0) * g_frame.view).xyz;
    let toward = select(card, -card, dot(card, p) > 0.0);
    return normalize(radial * 0.75 + toward * 0.25);
}}

// The foliage class, with how much of the directional light the surface was drawn with carried
// in its fraction (4.1 none to 4.4 all; exactly 4 where that is not known), so a pass that
// lights it again can take out exactly the light it was drawn with.
fn hifi_foliage_code(i: VsOut) -> f32 {{
    if (i.sun_share < 0.0) {{
        return {foliage};
    }}
    return {foliage} + 0.1 + 0.3 * clamp(i.sun_share, 0.0, 1.0);
}}

fn hifi_out(c: vec4<f32>, i: VsOut, material: f32) -> HifiOut {{
    var o: HifiOut;
    // Fog and brightness as the ordinary stage applies them, then the result as linear light,
    // not clamped: the ordinary stage's clamp is the resolve's.
    let shown = mix(g_frame.fog_color.rgb, c.rgb, i.fog) * gamma_scale();
    o.colour = vec4<f32>(hifi_linear_rgb(shown), c.a);
    let n = hifi_view_normal(i);
    if (material == {foliage}) {{
        o.normal = vec4<f32>(hifi_canopy_normal(i, n), hifi_foliage_code(i));
    }} else {{
        o.normal = vec4<f32>(n, material);
    }}
    return o;
}}
",
        lit = MaterialClass::Lit.wgsl_name(),
        baked = MaterialClass::Baked.wgsl_name(),
        unlit = MaterialClass::Unlit.wgsl_name(),
        foliage = MaterialClass::Foliage.wgsl_name(),
        without_sun = WITHOUT_SUN,
        view_normal = view_normal(format),
    )
}
