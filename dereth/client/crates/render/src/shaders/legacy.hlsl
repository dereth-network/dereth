// The whole shader set for the legacy render path.
//
// Five specialised pixel shaders implement the texture-stage combinations used by this
// renderer. Vertex transforms use row vectors and column-major matrix storage.
//
// Compile-time permutations:
//   HAS_NORMAL      -- the vertex carries a D3DFVF_NORMAL (FVF 0x152 / 0x252)
//   HAS_TEXCOORD1   -- the vertex carries a second UV set (FVF 0x242 / 0x252)
//   PRETRANSFORMED  -- D3DFVF_XYZRHW, already in clip space (FVF 0x144)

cbuffer PerFrame : register(b0)
{
    float4x4 g_viewProj;
    float4x4 g_view;
    // x = fog start, y = fog end, z = fog enabled, w = unused. Fog is per-vertex linear.
    float4 g_fogParams;
    float4 g_fogColor;
    float4 g_ambient;
    // x = the clamped `Render.ScreenBrightness` (the device's gamma brightness); y, z, w
    // unused. See `gamma_scale` below.
    float4 g_screen;
};

cbuffer PerDraw : register(b1)
{
    float4x4 g_world;
    // The D3DTS_TEXTURE0 translation built from the mesh buffer's UV delta.
    float4 g_uvOffset;
    // D3DRS_TEXTUREFACTOR, where the UI's colour modulation and opacity both come from.
    float4 g_textureFactor;
    // x = alpha reference / 255 (100 or 200 out of 255), y = alpha test enabled,
    // z = fog enabled for this draw (the per-surface fog disable), w = unused.
    float4 g_drawParams;
    // The bound material's Emissive.rgb (x) and Diffuse.rgb (y); z = a material is
    // bound at all. The
    // fixed-function lit colour is Emissive + Diffuse * lights, and the vertex colour here is that
    // lit colour for the default material (Emissive 0, Diffuse 1), so the diffuse source becomes
    // saturate(colour * y + x). The selection blink uses (0.99, 1.0) /
    // (0.0, 0.35); with z = 0 the colour is untouched.
    float4 g_materialLighting;
    // The fixed-function vertex lighting the client turns on for every mesh subset
    // (its own disable flag is never set, so it is on for every mesh).
    // x = D3DRS_LIGHTING, y = how many of the eight slots the light-enable pass left enabled,
    // z = the material's Emissive scalar (the subset's own surface luminosity when > 0, else the
    // part clone's), w = 1 when the emissive colour source is the vertex -- the burned-in
    // env-cell branch, where the vertex colour *is* the cell's static light.
    float4 g_lightingParams;
    // The eight `D3DLIGHT9`s `SetLight(slot)` installed, as the client's light configuration
    // built them: xyz = Position for a point light (world space, with the client's y/z swap
    // folded in exactly as the client folds it) or Direction for a directional one;
    // w = Range (= falloff * 1.5) for a point light, unused for a directional.
    float4 g_lightPos[8];
    // rgb = Diffuse (= colour * intensity), w = D3DLIGHTTYPE:
    // 1 POINT, 2 SPOT, 3 DIRECTIONAL.
    float4 g_lightDiffuse[8];
    // Detail texture: x scales UV set 0 to generate UV set 1; y enables the detail texture.
    float4 g_detailParams;
    // The normal transform under a per-axis scale: xyz = 1 / s^2 per model axis, w = 1 to use
    // it. The inverse transpose of a rotation times a per-axis scale is the matrix itself applied
    // to n / s^2. w = 0 on every draw whose scale is uniform.
    float4 g_normalScale;
};

// The DIFFUSE colour argument of a stage, after the material's lighting. `arg2` is what
// the client's diffuse-source setting chose (the vertex colour, or the texture factor); the material term
// applies on top of either, as D3D lights the material before the stage chain sees the colour.
float4 material_diffuse(float4 arg2)
{
    float3 lit = saturate(arg2.rgb * g_materialLighting.y + g_materialLighting.x);
    return float4(lerp(arg2.rgb, lit, g_materialLighting.z), arg2.a);
}

Texture2D g_tex0 : register(t0);
Texture2D g_tex1 : register(t1);
SamplerState g_samp0 : register(s0);
SamplerState g_samp1 : register(s1);

struct VSIn
{
#if PRETRANSFORMED
    float4 pos : POSITION;
#else
    float3 pos : POSITION;
#endif
#if HAS_NORMAL
    float3 normal : NORMAL;
#endif
    float4 color : COLOR;
    float2 uv0 : TEXCOORD0;
#if HAS_TEXCOORD1
    float2 uv1 : TEXCOORD1;
#endif
};

struct VSOut
{
    float4 pos : SV_POSITION;
    float4 color : COLOR;
    float2 uv0 : TEXCOORD0;
    float2 uv1 : TEXCOORD1;
    // Computed in the VS and interpolated, which is what D3DFOG_LINEAR in the vertex pipeline does.
    float fog : TEXCOORD2;
};

VSOut vs_main(VSIn i)
{
    VSOut o;
#if PRETRANSFORMED
    // D3DFVF_XYZRHW. The client uses this for exactly one draw, the portal depth stamp
    // the portal depth stamp, and the vertex it builds there holds
    // **screen** coordinates, not clip ones: `x = viewportX + xw/w`, `y = viewportY + yw/w`,
    // `z` = the device depth (the polygon's own, or the constant 0.999999 when bit 0 of the debug
    // mask is set), `w` = `1/w` (the reciprocal homogeneous w, which is what "RHW" names).
    //
    // Convert screen coordinates to clip space without an extra half-pixel offset.
    // g_uvOffset.zw carries the viewport extent on this path.
    float2 vp = max(g_uvOffset.zw, float2(1.0, 1.0));
    o.pos = float4(i.pos.x * 2.0 / vp.x - 1.0, 1.0 - i.pos.y * 2.0 / vp.y, i.pos.z, 1.0);
    // RHW is 1/w and w is the view-space distance, so this recovers the fog distance. Every shipped
    // caller of this path draws with fog disabled, so it is never read.
    float viewZ = 1.0 / max(i.pos.w, 1e-6);
#else
    float4 world = mul(float4(i.pos, 1.0), g_world);
    o.pos = mul(world, g_viewProj);
    // Range fog uses radial eye distance with linear attenuation.
    float viewZ = length(mul(world, g_view).xyz);
#endif
    o.color = i.color;
#if HAS_NORMAL && !PRETRANSFORMED
    // The Direct3D 9 fixed-function vertex lighting, per vertex (Gouraud:
    // D3DRS_SHADEMODE is never changed from the default state block's GOURAUD), with the
    // states the client leaves set: D3DRS_NORMALIZENORMALS = 1, D3DRS_SPECULARENABLE = 0, every
    // light's Ambient and Specular zero, Attenuation0/1/2 = (0, 1, 0), and the material sources
    // the client chooses when it binds one: the vertex colour for Diffuse and Ambient when no
    // material is bound, the material for both when one is. The D3D9 vertex pipeline's
    // diffuse output is
    //
    //     saturate(Me + Ma * D3DRS_AMBIENT + sum_k Md * Ld_k * max(N . L_k, 0) * atten_k)
    //
    // with atten_k = 1 / (a0 + a1 * d + a2 * d^2) inside Range and 0 beyond it, and
    // L_k = -Direction for a directional light. A material clone starts with Diffuse and
    // Ambient at (1, 1, 1, 1) and the diffuse setter scales the former,
    // so a bound material is Md = diffuse, Ma = 1; an unbound one takes both from the vertex
    // colour, which `build_meshes` writes white.
    if (g_lightingParams.x > 0.5)
    {
        // D3D transforms the normal by the inverse transpose of the world matrix before it
        // normalises it.
        float3 nrm = g_normalScale.w > 0.5 ? i.normal * g_normalScale.xyz : i.normal;
        float3 N = normalize(mul(float4(nrm, 0.0), g_world).xyz);
        float3 P = world.xyz;
        float bound = g_materialLighting.z;
        float3 Md = lerp(i.color.rgb, g_materialLighting.yyy, bound);
        float3 Ma = lerp(i.color.rgb, float3(1.0, 1.0, 1.0), bound);
        float3 Me = lerp(g_lightingParams.zzz, i.color.rgb, g_lightingParams.w);
        float3 sum = float3(0.0, 0.0, 0.0);
        int n = (int)g_lightingParams.y;
        [unroll]
        for (int k = 0; k < 8; ++k)
        {
            if (k < n)
            {
                float4 lp = g_lightPos[k];
                float4 ld = g_lightDiffuse[k];
                float3 L;
                float atten;
                if (ld.w == 3.0)
                {
                    // D3DLIGHT_DIRECTIONAL: the vector the light travels along, so the vertex
                    // sees it from the opposite side.
                    L = -normalize(lp.xyz);
                    atten = 1.0;
                }
                else
                {
                    float3 d = lp.xyz - P;
                    float dist = length(d);
                    if (dist >= lp.w)
                    {
                        continue;
                    }
                    L = d / max(dist, 1e-6);
                    // Attenuation0 + Attenuation1 * d + Attenuation2 * d^2 with (0, 1, 0).
                    atten = 1.0 / (0.0 + 1.0 * dist + 0.0 * dist * dist);
                }
                sum += ld.rgb * max(dot(N, L), 0.0) * atten;
            }
        }
        float3 lit = saturate(Me + Ma * g_ambient.rgb + Md * sum);
        // Select vertex alpha or the bound material alpha carried in the texture factor.
        o.color = float4(lit, lerp(i.color.a, g_textureFactor.a, g_drawParams.w));
    }
#endif
    o.uv0 = i.uv0 + g_uvOffset.xy;
#if HAS_TEXCOORD1
    o.uv1 = i.uv1;
#else
    // A vertex with no second set gets the one the client's detail tiling would have
    // written into it: UV set 1 is UV set 0 scaled by the tiling factor, for every vertex of the
    // buffer, so it is a constant multiply rather than 8 more bytes per vertex. `g_detailParams.x`
    // is 0 on every draw that has no detail texture, and `lerp` keeps those at the old `i.uv0`.
    o.uv1 = i.uv0 * lerp(1.0, g_detailParams.x, g_detailParams.y);
#endif
    // D3DRS_FOGSTART / D3DRS_FOGEND, linear, saturated. 1 = no fog, 0 = fully fogged.
    float f = saturate((g_fogParams.y - viewZ) / max(g_fogParams.y - g_fogParams.x, 1e-6));
    o.fog = lerp(1.0, f, g_fogParams.z * g_drawParams.z);
    return o;
}

// D3DRS_ALPHATESTENABLE / ALPHAFUNC / ALPHAREF have no D3D12 equivalent and must become a
// pixel-shader clip(). The compare is GREATEREQUAL for a surface (GREATER for material
// layers -- the two material systems genuinely differ), so the surviving condition is
// `a >= ref` and the discarded one is `a < ref`.
void alpha_test(float a)
{
    if (g_drawParams.y > 0.5)
    {
        clip(a - g_drawParams.x + 1e-6);
    }
}

// Brightness scales RGB by 1 + 2v, relative to the neutral slider value.
// This omits the 255/257 slope of a 16-bit identity ramp and clamps each shader output
// before blending rather than clamping the final composited frame.
float gamma_scale()
{
    return 1.0 + 2.0 * g_screen.x;
}

float4 finish(float4 c, float fog)
{
    // Fog is applied to colour only; the alpha channel is not fogged, and the render target's
    // alpha is masked off by COLORWRITEENABLE = 7 in any case.
    c.rgb = lerp(g_fogColor.rgb, c.rgb, fog);
    // Apply brightness after fog while preserving alpha.
    c.rgb = saturate(c.rgb * gamma_scale());
    return c;
}

// With D3DRS_LIGHTING on, the vertex stage already produced the lit colour --
// Emissive + Ambient * D3DRS_AMBIENT + Diffuse * (the lights) -- with the material's own
// channels folded in, so the stage chain takes the interpolated vertex colour as it is. Without
// it the unlit path stands: the vertex colour or the texture factor, then the material term.
float4 diffuse_arg(VSOut i)
{
    if (g_lightingParams.x > 0.5)
    {
        return i.color;
    }
    return material_diffuse(lerp(i.color, g_textureFactor, g_drawParams.w));
}

// ---------------------------------------------------------------------------------------------
// The five stage-op chains.
// ---------------------------------------------------------------------------------------------

// Stage 0: COLOROP MODULATE(TEXTURE, DIFFUSE), ALPHAOP MODULATE(TEXTURE, DIFFUSE); stage 1 DISABLE.
// The base surface pass, and the UI quad with ARG2 = TFACTOR.
float4 ps_modulate(VSOut i) : SV_TARGET
{
    float4 t = g_tex0.Sample(g_samp0, i.uv0);
    float4 arg2 = diffuse_arg(i);
    float4 c = t * arg2;
    alpha_test(c.a);
    return finish(c, i.fog);
}

// COLOROP SELECTARG1: the texture alone.
float4 ps_selectarg1(VSOut i) : SV_TARGET
{
    float4 t = g_tex0.Sample(g_samp0, i.uv0);
    float4 c = float4(t.rgb, t.a * i.color.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// COLOROP SELECTARG2(TEXTURE, DIFFUSE), ALPHAOP MODULATE(TEXTURE, DIFFUSE) --
// the texture-based font's own setup: "the colour comes entirely from the vertex" and
// "alpha = glyph coverage x vertex alpha".
float4 ps_selectarg2(VSOut i) : SV_TARGET
{
    float4 t = g_tex0.Sample(g_samp0, i.uv0);
    float4 arg2 = diffuse_arg(i);
    float4 c = float4(arg2.rgb, t.a * arg2.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// Stage 0 of the single-pass detail chain in isolation: COLOROP MODULATE(TEXTURE, DIFFUSE),
// ALPHAOP PREMODULATE(DIFFUSE, DIFFUSE). PREMODULATE hands the *next* stage the un-modulated
// alpha, so on its own the visible result is the modulated colour with the vertex alpha.
float4 ps_premodulate(VSOut i) : SV_TARGET
{
    float4 t = g_tex0.Sample(g_samp0, i.uv0);
    float4 c = float4((t * i.color).rgb, i.color.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// Apply detail as a grain overlay: base.rgb * (detail.rgb + 1 - detail.a).
// The diffuse alpha weights the overlay from no effect at zero to full effect at one.
// Preserve the base alpha for alpha testing instead of multiplying it by detail alpha.
float4 ps_blendcurrentalpha(VSOut i) : SV_TARGET
{
    float4 t0 = g_tex0.Sample(g_samp0, i.uv0);
    float4 t1 = g_tex1.Sample(g_samp1, i.uv1);
    float4 arg2 = diffuse_arg(i);
    float4 current = t0 * arg2;
    // D3DTA_DIFFUSE's alpha, which stage 0's PREMODULATE hands on unchanged.
    float currentAlpha = arg2.a;
    // `detail.rgb + 1 - detail.a`, faded to 1 as the current alpha goes to 0.
    float3 grain = lerp(1.0.xxx, t1.rgb + (1.0 - t1.a).xxx, currentAlpha);
    float4 c = float4(current.rgb * grain, current.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}
