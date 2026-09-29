// The whole shader set for the legacy render path, in WGSL.
//
// The client contains **no programmable shaders at all**: the only shader call in the client is
// a `SetVertexShader(NULL)` in the device's state setup, and there is no
// `CreatePixelShader` anywhere [verified against retail]. These shaders
// exist only because Vulkan
// has no fixed-function pipeline; each one reproduces one fixed-function configuration the client
// actually issues, and nothing else.
//
// This is a line-for-line port of the D3D12 build's `legacy.hlsl` (in the git history before the
// Vulkan port), keeping its names, its arithmetic and -- deliberately -- its **row-vector**
// matrix form: `v * M` here is HLSL's `mul(v, M)` over the same column-major sixteen floats, so
// `dereth_render::vulkan::hlsl_matrix` and every caller of it are unchanged.
//
// The texture-stage op chain becomes a handful of specialised pixel shaders; the client only uses
// the combinations
// tabulated in 03 (MODULATE, SELECTARG1/2, PREMODULATE, BLENDCURRENTALPHA, DISABLE), so **five**
// pixel shaders cover the legacy path.
//
// Compile-time permutations of the vertex shader are text substitutions made by
// `vulkan/shaders.rs` (WGSL has no preprocessor), one per FVF code:
//   HAS_NORMAL      -- the vertex carries a D3DFVF_NORMAL (FVF 0x152 / 0x252)
//   HAS_TEXCOORD1   -- the vertex carries a second UV set (FVF 0x242 / 0x252)
//   PRETRANSFORMED  -- D3DFVF_XYZRHW, already in clip space (FVF 0x144)
//
// Clip space: the renderer installs its viewports with a **negative height** (Vulkan 1.1
// `VK_KHR_maintenance1`), which makes framebuffer space identical to Direct3D's -- +y up in clip
// space, y down on the screen, z in [0, 1] -- so nothing in here flips anything.

struct PerFrame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    // x = fog start, y = fog end, z = fog enabled, w = unused. Fog is per-vertex linear:
    // The client's default device state sets the fog *table* mode to D3DFOG_NONE and the *vertex* mode to
    // D3DFOG_LINEAR (02-render-device-abstraction.md).
    fog_params: vec4<f32>,
    fog_color: vec4<f32>,
    ambient: vec4<f32>,
    // Unit P3.5. x = the clamped `Render.ScreenBrightness` (the device's gamma brightness); y, z, w
    // unused. See `gamma_scale` below.
    screen: vec4<f32>,
};

struct PerDraw {
    world: mat4x4<f32>,
    // The D3DTS_TEXTURE0 translation built from the mesh buffer's UV delta. Open question #87.
    uv_offset: vec4<f32>,
    // D3DRS_TEXTUREFACTOR, where the UI's colour modulation and opacity both come from.
    texture_factor: vec4<f32>,
    // x = alpha reference / 255 (100 or 200 out of 255 -- contract 11.2), y = alpha test enabled,
    // z = fog enabled for this draw (the per-surface fog disable), w = use the texture factor instead
    // of the vertex colour.
    draw_params: vec4<f32>,
    // Unit P1.35. The bound material's Emissive.rgb (x) and Diffuse.rgb (y); z = a material is
    // bound at all. The
    // fixed-function lit colour is Emissive + Diffuse * lights, and the vertex colour here is that
    // lit colour for the default material (Emissive 0, Diffuse 1), so the diffuse source becomes
    // saturate(colour * y + x). This is what the selection blink's SetLighting(0.99, 1.0) /
    // (0.0, 0.35) is made of; with z = 0 the colour is untouched.
    material_lighting: vec4<f32>,
    // Unit P3.2 -- the fixed-function vertex lighting the client turns on for every mesh subset
    // (its own disable flag is never set, so it is on for every mesh).
    // x = D3DRS_LIGHTING, y = how many of the eight slots the light-enable pass left enabled,
    // z = the material's Emissive scalar (the subset's own surface luminosity when > 0, else the
    // part clone's), w = 1 when the emissive colour source is the vertex -- the burned-in
    // env-cell branch, where the vertex colour *is* the cell's static light.
    lighting_params: vec4<f32>,
    // The eight `D3DLIGHT9`s `SetLight(slot)` installed, as the client's light configuration
    // built them: xyz = Position for a point light (world space, with the client's y/z swap
    // folded in exactly as the client folds it) or Direction for a directional one;
    // w = Range (= falloff * 1.5) for a point light, unused for a directional.
    light_pos: array<vec4<f32>, 8>,
    // rgb = Diffuse (= colour * intensity), w = D3DLIGHTTYPE:
    // 1 POINT, 2 SPOT, 3 DIRECTIONAL.
    light_diffuse: array<vec4<f32>, 8>,
    // Unit P3.9 -- the detail-texture pass. x = the client's current detail tiling, the
    // factor it scales UV set 0 by to make UV set 1
    // (`v[0x24] = factor * v[0x1C]`, `v[0x28] = factor * v[0x20]`, for the whole vertex buffer);
    // y = 1 when stage 1 holds a detail texture. Zero on every other draw.
    detail_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g_frame: PerFrame;
@group(0) @binding(1) var<uniform> g_draw: PerDraw;
@group(1) @binding(0) var g_tex0: texture_2d<f32>;
@group(2) @binding(0) var g_tex1: texture_2d<f32>;
@group(3) @binding(0) var g_samp0: sampler;
@group(3) @binding(1) var g_samp1: sampler;

// The DIFFUSE colour argument of a stage, after the material's lighting. `arg2` is what
// the client's diffuse-source setting chose (the vertex colour, or the texture factor); the material term
// applies on top of either, as D3D lights the material before the stage chain sees the colour.
fn material_diffuse(arg2: vec4<f32>) -> vec4<f32> {
    let lit = saturate(arg2.rgb * g_draw.material_lighting.y + g_draw.material_lighting.x);
    return vec4<f32>(mix(arg2.rgb, lit, g_draw.material_lighting.z), arg2.a);
}

struct VsIn {
    @location(0) pos: {{POS_TYPE}},
{{NORMAL_IN}}
    // D3DCOLOR: B, G, R, A in memory. Read as `B8G8R8A8_UNORM` where the device allows that as a
    // vertex format, and as `R8G8B8A8_UNORM` plus the `.bgra` swizzle below where it does not.
    @location(2) color: vec4<f32>,
    @location(3) uv0: vec2<f32>,
{{UV1_IN}}
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv0: vec2<f32>,
    @location(2) uv1: vec2<f32>,
    // Computed in the VS and interpolated, which is what D3DFOG_LINEAR in the vertex pipeline does.
    @location(3) fog: f32,
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    let color = i.color{{COLOR_SWIZZLE}};
{{POSITION}}
    o.color = color;
{{LIGHTING}}
    o.uv0 = i.uv0 + g_draw.uv_offset.xy;
{{UV1}}
    // D3DRS_FOGSTART / D3DRS_FOGEND, linear, saturated. 1 = no fog, 0 = fully fogged.
    let f = saturate((g_frame.fog_params.y - view_z)
        / max(g_frame.fog_params.y - g_frame.fog_params.x, 1e-6));
    o.fog = mix(1.0, f, g_frame.fog_params.z * g_draw.draw_params.z);
    return o;
}

// ---- the vertex-shader permutation blocks `vulkan/shaders.rs` substitutes ----------------------
//
// `POSITION`, world-space (every format but 0x144):
//
//     let world = vec4<f32>(i.pos, 1.0) * g_draw.world;
//     o.pos = world * g_frame.view_proj;
//     // **Unit P4.6a.** The client's default state block sets `D3DRS_RANGEFOGENABLE` to 1 and
//     // the vertex fog mode to `D3DFOG_LINEAR`, so the fog coordinate is the **radial** eye
//     // distance and not the view-space z.
//     let view_z = length((world * g_frame.view).xyz);
//
// `POSITION`, PRETRANSFORMED (0x144):
//
//     // D3DFVF_XYZRHW. The client uses this for exactly one draw, the portal depth stamp
//     // the portal depth stamp, and the vertex it builds there holds
//     // **screen** coordinates, not clip ones: `x = viewportX + xw/w`, `y = viewportY + yw/w`,
//     // `z` = the device depth (the polygon's own, or the constant 0.999999 when bit 0 of the debug
//     // mask is set), `w` = `1/w` (the reciprocal homogeneous w, which is what "RHW" names).
//     //
//     // D3D9's driver did the screen -> clip step; a programmable pipeline has no fixed-function
//     // stage that does, so it is here. `07-engine-specifics-d3d9.md` section 2: the
//     // pre-transformed paths "add the viewport origin to the screen coordinate but **do not**
//     // apply an extra half-pixel", so this is the plain viewport inverse and nothing else.
//     // g_uvOffset.zw carries the viewport extent, which is free on this path because a
//     // pre-transformed draw has no D3DTS_TEXTURE0 translation.
//     let vp = max(g_draw.uv_offset.zw, vec2<f32>(1.0, 1.0));
//     o.pos = vec4<f32>(i.pos.x * 2.0 / vp.x - 1.0, 1.0 - i.pos.y * 2.0 / vp.y, i.pos.z, 1.0);
//     // RHW is 1/w and w is the view-space distance, so this recovers the fog distance. Every
//     // shipped caller of this path draws with fog disabled, so it is never read.
//     let view_z = 1.0 / max(i.pos.w, 1e-6);
//
// `LIGHTING`, HAS_NORMAL && !PRETRANSFORMED -- unit P3.2, the Direct3D 9 fixed-function vertex
// lighting, per vertex (Gouraud: D3DRS_SHADEMODE is never changed from the default state
// block's GOURAUD), with the states the client leaves set: D3DRS_NORMALIZENORMALS = 1,
// D3DRS_SPECULARENABLE = 0, every light's Ambient and Specular zero, Attenuation0/1/2 =
// (0, 1, 0), and the
// material sources the client chooses when it binds one: the vertex colour for Diffuse and
// Ambient when no material is bound, the material for both when one is. The D3D9 vertex pipeline's
// diffuse output is
//
//     saturate(Me + Ma * D3DRS_AMBIENT + sum_k Md * Ld_k * max(N . L_k, 0) * atten_k)
//
// with atten_k = 1 / (a0 + a1 * d + a2 * d^2) inside Range and 0 beyond it, and L_k = -Direction
// for a directional light. A material clone starts with Diffuse and Ambient
// at (1, 1, 1, 1) and the diffuse setter scales the former, so a bound material is
// Md = diffuse, Ma = 1; an unbound one takes both from the vertex colour, which `build_meshes`
// writes white. The block itself is in `vulkan/shaders.rs`.
//
// `UV1`, HAS_TEXCOORD1:      o.uv1 = i.uv1;
// `UV1`, otherwise (P3.9):   o.uv1 = i.uv0 * mix(1.0, g_draw.detail_params.x, g_draw.detail_params.y);
//   A vertex with no second set gets the one the client's detail tiling would have written into
//   it: UV set 1 is UV set 0 scaled by the tiling factor, for every vertex of the buffer, so it is
//   a constant multiply rather than 8 more bytes per vertex. `detail_params.x` is 0 on every draw
//   that has no detail texture, and `mix` keeps those at the old `i.uv0`.

// D3DRS_ALPHATESTENABLE / ALPHAFUNC / ALPHAREF have no programmable-pipeline equivalent and must
// become a pixel-shader discard. The compare is GREATEREQUAL for a surface (GREATER for
// RenderMaterial layers -- the two material systems genuinely differ), so the surviving condition
// is `a >= ref` and the discarded one is `a < ref`.
fn alpha_test(a: f32) {
    if (g_draw.draw_params.y > 0.5) {
        if (a - g_draw.draw_params.x + 1e-6 < 0.0) {
            discard;
        }
    }
}

// Unit P3.5 -- the client's gamma ramp, which the swap chain cannot install.
//
// The client builds `ramp[i] = clamp(255*i + 510*i*v, 0, 65535)` for a clamped
// `v = Render.ScreenBrightness` in `[-0.2, 1.0]` and hands it to `IDirect3DDevice9::SetGammaRamp`,
// so the display hardware applies it to the whole finished frame. The ramp is **linear in i**:
// relative to the `v = 0` ramp it is exactly a multiply by `1 + 2v`, saturating where the entry
// hits 0xFFFF. Applying that factor per pixel here is equivalent for the composite, because every
// blend the client issues is linear in both source and destination and the frame clear is black.
//
// Two declared departures, both stated rather than hidden:
//   * the ramp's own absolute slope is 255*i out of 65535, i.e. 255/257 of a true identity. That
//     is the 16-bit D3DGAMMARAMP encoding, not a brightness the player chose; reproducing it would
//     darken every frame by 0.8% against a client with no ramp installed. The factor here is the
//     ratio against the v = 0 ramp, which is what the slider means.
//   * the saturation point moves: the hardware clamps the final pixel once, this clamps each
//     source and each destination. Both are `saturate` of a linear ramp, and they agree except on
//     a pixel that is already over-bright before the blend.
fn gamma_scale() -> f32 {
    return 1.0 + 2.0 * g_frame.screen.x;
}

fn finish(c: vec4<f32>, fog: f32) -> vec4<f32> {
    // Fog is applied to colour only; the alpha channel is not fogged, and the render target's
    // alpha is masked off by COLORWRITEENABLE = 7 in any case.
    let fogged = mix(g_frame.fog_color.rgb, c.rgb, fog);
    // The gamma ramp is applied after everything else, because the hardware applies it to the
    // finished frame.
    return vec4<f32>(saturate(fogged * gamma_scale()), c.a);
}

// Unit P3.2. With D3DRS_LIGHTING on, the vertex stage already produced the lit colour --
// Emissive + Ambient * D3DRS_AMBIENT + Diffuse * (the lights) -- with the material's own
// channels folded in, so the stage chain takes the interpolated vertex colour as it is. Without
// it the pre-P3.2 path stands: the vertex colour or the texture factor, then the material term.
fn diffuse_arg(i: VsOut) -> vec4<f32> {
    if (g_draw.lighting_params.x > 0.5) {
        return i.color;
    }
    return material_diffuse(mix(i.color, g_draw.texture_factor, g_draw.draw_params.w));
}

// ---------------------------------------------------------------------------------------------
// The five stage-op chains.
// ---------------------------------------------------------------------------------------------

// Stage 0: COLOROP MODULATE(TEXTURE, DIFFUSE), ALPHAOP MODULATE(TEXTURE, DIFFUSE); stage 1 DISABLE.
// The base surface pass, and the UI quad with ARG2 = TFACTOR.
@fragment
fn ps_modulate(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(g_tex0, g_samp0, i.uv0);
    let arg2 = diffuse_arg(i);
    let c = t * arg2;
    alpha_test(c.a);
    return finish(c, i.fog);
}

// COLOROP SELECTARG1: the texture alone.
@fragment
fn ps_selectarg1(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(g_tex0, g_samp0, i.uv0);
    let c = vec4<f32>(t.rgb, t.a * i.color.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// COLOROP SELECTARG2(TEXTURE, DIFFUSE), ALPHAOP MODULATE(TEXTURE, DIFFUSE) --
// the texture-based font's own setup: "the colour comes entirely from the vertex" and
// "alpha = glyph coverage x vertex alpha".
@fragment
fn ps_selectarg2(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(g_tex0, g_samp0, i.uv0);
    let arg2 = diffuse_arg(i);
    let c = vec4<f32>(arg2.rgb, t.a * arg2.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// Stage 0 of the single-pass detail chain in isolation: COLOROP MODULATE(TEXTURE, DIFFUSE),
// ALPHAOP PREMODULATE(DIFFUSE, DIFFUSE). PREMODULATE hands the *next* stage the un-modulated
// alpha, so on its own the visible result is the modulated colour with the vertex alpha.
@fragment
fn ps_premodulate(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(g_tex0, g_samp0, i.uv0);
    let c = vec4<f32>((t * i.color).rgb, i.color.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}

// The two-stage detail chain the client's surface binding sets up with
// `bDetailInStage1`:
//   stage 0 COLOR MODULATE(TEXTURE, DIFFUSE), ALPHA PREMODULATE(DIFFUSE, DIFFUSE)
//   stage 1 COLOR BLENDCURRENTALPHA(TEXTURE, CURRENT), ALPHA MODULATE(TEXTURE, CURRENT)
//   stage 2 DISABLE, sampler 1 WRAP/WRAP LINEAR/LINEAR/LINEAR
//
// **Unit P3.9 corrected this function's combine, and the correction is a declared reading.**
// D3DTOP_BLENDCURRENTALPHA on its own is `arg1 * alpha(current) + arg2 * (1 - alpha(current))`,
// which with arg1 = the detail texture and alpha = 1 (every opaque surface: `SetSurface`'s
// `curr_alpha` is 0xFF for a non-TRANSLUCENT one) would *replace* the lit base texture with the
// raw detail texture. Measured, the shipped detail texture `SurfaceTexture 0x05001787` is
// 256x256 with mean RGB 42/255 and mean alpha 34/255 -- a dark, low-alpha grain sheet. Replacing
// a wall with it, or modulating a wall by it, takes the whole building to 16% brightness; the
// neighbour suite `p3_lighting` measured exactly that (an object band at 34.0 -> 15.9).
//
// **The unambiguous half of the machine settles it.** When the device cannot do two stages,
// the mesh subset draw issues the same content as a **separate** pass with the detail surface
// in stage 0, and that path's arithmetic is device blend
// state rather than a fixed-function op whose semantics live in a driver:
// a blend of the current detail source and destination blends with `ADD`, using
// (9 `DESTCOLOR`, 6 `INVSRCALPHA`) for the only two contents retail ever enables -- buildings
// and environment cells. That is
//
//     result = detail.rgb * dst + dst * (1 - detail.a)
//            = dst * (detail.rgb + 1 - detail.a)
//
// -- a **grain overlay**: neutral wherever the texel's colour equals its alpha, darkening where it
// is darker, brightening where it is lighter. On the shipped sheet that is `dst * 1.03` on
// average, which is what a "detail texture" is for. The single-pass fork exists to produce the
// same picture in one pass, so that is the picture here, weighted by the current alpha exactly as
// BLENDCURRENTALPHA weights: full effect at alpha 1, none at alpha 0 (which is what would make
// the client's 10..50 m distance fade work if the landscape arm were ever
// revived -- it is dead in the shipped client, see `dereth_world_render::detail`).
//
// **One declared departure, and it is the alpha channel.** Stage 1's ALPHAOP is
// `MODULATE(TEXTURE, CURRENT)`, i.e. `detail.a * current.a`. Applied literally that
// puts every detail-textured pixel's alpha at ~0.13, and `SetSurface`'s alpha test (rows 7 and 8,
// `a >= ref` with ref 100/255) then clips every clip-mapped building subset out of the frame.
// Retail visibly does not do that, so the alpha here stays the base pass's. Named rather than
// hidden; a successor with a way to observe retail's alpha test under this pass should check it.
@fragment
fn ps_blendcurrentalpha(i: VsOut) -> @location(0) vec4<f32> {
    let t0 = textureSample(g_tex0, g_samp0, i.uv0);
    let t1 = textureSample(g_tex1, g_samp1, i.uv1);
    let arg2 = diffuse_arg(i);
    let current = t0 * arg2;
    // D3DTA_DIFFUSE's alpha, which stage 0's PREMODULATE hands on unchanged.
    let current_alpha = arg2.a;
    // `detail.rgb + 1 - detail.a`, faded to 1 as the current alpha goes to 0.
    let grain = mix(vec3<f32>(1.0), t1.rgb + vec3<f32>(1.0 - t1.a), current_alpha);
    let c = vec4<f32>(current.rgb * grain, current.a);
    alpha_test(c.a);
    return finish(c, i.fog);
}
