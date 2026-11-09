//! Backend-independent WGSL permutations for the legacy vertex and pixel stages.

use crate::vertex::VertexFormat;

/// The shader template before vertex-layout substitution.
pub const SOURCE: &str = include_str!("shaders/legacy.wgsl");

/// The world-space `{{POSITION}}` block.
const POSITION_WORLD: &str = "    let world = vec4<f32>(i.pos, 1.0) * g_draw.world;
    o.pos = world * g_frame.view_proj;
    let view_z = length((world * g_frame.view).xyz);
";

/// The `PRETRANSFORMED` `{{POSITION}}` block. `world` is declared so the block interface is the
/// same shape either way; nothing reads it on this path.
const POSITION_PRETRANSFORMED: &str = "    let vp = max(g_draw.uv_offset.zw, vec2<f32>(1.0, 1.0));
    o.pos = vec4<f32>(i.pos.x * 2.0 / vp.x - 1.0, 1.0 - i.pos.y * 2.0 / vp.y, i.pos.z, 1.0);
    let view_z = 1.0 / max(i.pos.w, 1e-6);
";

/// The `HAS_NORMAL && !PRETRANSFORMED` `{{LIGHTING}}` block. The derivation is in the
/// shader source's comment block.
const LIGHTING: &str = "    if (g_draw.lighting_params.x > 0.5) {
        let nrm = select(i.normal, i.normal * g_draw.normal_scale.xyz, g_draw.normal_scale.w > 0.5);
        let N = normalize((vec4<f32>(nrm, 0.0) * g_draw.world).xyz);
        let P = world.xyz;
        let bound = g_draw.material_lighting.z;
        let Md = mix(color.rgb, vec3<f32>(g_draw.material_lighting.y), bound);
        let Ma = mix(color.rgb, vec3<f32>(1.0), bound);
        let Me = mix(vec3<f32>(g_draw.lighting_params.z), color.rgb, g_draw.lighting_params.w);
        var sum = vec3<f32>(0.0);
        let n = i32(g_draw.lighting_params.y);
        for (var k: i32 = 0; k < 8; k = k + 1) {
            if (k < n) {
                let lp = g_draw.light_pos[k];
                let ld = g_draw.light_diffuse[k];
                var L = vec3<f32>(0.0);
                var atten = 1.0;
                if (ld.w == 3.0) {
                    // D3DLIGHT_DIRECTIONAL: the vector the light travels along, so the vertex
                    // sees it from the opposite side.
                    L = -normalize(lp.xyz);
                    atten = 1.0;
                } else {
                    let d = lp.xyz - P;
                    let dist = length(d);
                    if (dist >= lp.w) {
                        continue;
                    }
                    L = d / max(dist, 1e-6);
                    // Attenuation0 + Attenuation1 * d + Attenuation2 * d^2 with (0, 1, 0).
                    atten = 1.0 / (0.0 + 1.0 * dist + 0.0 * dist * dist);
                }
                sum = sum + ld.rgb * max(dot(N, L), 0.0) * atten;
            }
        }
        let lit = saturate(Me + Ma * g_frame.ambient.rgb + Md * sum);
        // The alpha is the diffuse source's: the vertex's (the surface setup's current alpha) or,
        // with a material bound, the clone's `1 - translucency`, which is carried in the
        // texture factor's alpha byte and selects with draw_params.w.
        o.color = vec4<f32>(lit, mix(color.a, g_draw.texture_factor.a, g_draw.draw_params.w));
    }
";

/// The vertex-shader source for one FVF code.
///
/// `bgra_vertex_colour` says whether the device reads the `D3DCOLOR` diffuse as
/// `B8G8R8A8_UNORM` directly; when it cannot, the attribute is `R8G8B8A8_UNORM` and the shader
/// swizzles the channels back.
#[must_use]
pub fn vertex_source(format: VertexFormat, bgra_vertex_colour: bool) -> String {
    let has_normal = matches!(
        format,
        VertexFormat::XyzNormalDiffuseTex1 | VertexFormat::XyzNormalDiffuseTex2
    );
    let two_uv = format.tex_coord_sets() == 2;
    let pre = format.is_pre_transformed();
    SOURCE
        .replace("{{POS_TYPE}}", if pre { "vec4<f32>" } else { "vec3<f32>" })
        .replace(
            "{{NORMAL_IN}}",
            if has_normal {
                "    @location(1) normal: vec3<f32>,"
            } else {
                ""
            },
        )
        .replace(
            "{{UV1_IN}}",
            if two_uv {
                "    @location(4) uv1: vec2<f32>,"
            } else {
                ""
            },
        )
        .replace(
            "{{COLOR_SWIZZLE}}",
            if bgra_vertex_colour { "" } else { ".bgra" },
        )
        .replace(
            "{{POSITION}}",
            if pre {
                POSITION_PRETRANSFORMED
            } else {
                POSITION_WORLD
            },
        )
        .replace(
            "{{LIGHTING}}",
            if has_normal && !pre { LIGHTING } else { "" },
        )
        .replace(
            "{{UV1}}",
            if two_uv {
                "    o.uv1 = i.uv1;"
            } else {
                "    o.uv1 = i.uv0 * mix(1.0, g_draw.detail_params.x, g_draw.detail_params.y);"
            },
        )
}

/// The pixel-shader source: the same file with the vertex permutation fixed to the simplest one,
/// because WGSL wants every marker resolved even for the entry points that do not use them.
#[must_use]
pub fn fragment_source() -> String {
    vertex_source(VertexFormat::XyzDiffuseTex1, true)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (each supported shader permutation resolves every template marker).
    use super::*;

    /// The markers are all consumed, for every permutation; a marker left in the text is a
    /// parse error at device creation, which is later than this.
    #[test]
    fn no_marker_survives_substitution() {
        for f in VertexFormat::all() {
            for bgra in [true, false] {
                let s = vertex_source(f, bgra);
                assert!(!s.contains("{{"), "{f:?}: {s}");
            }
        }
    }
}
