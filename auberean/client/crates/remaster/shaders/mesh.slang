// The placed objects and the sky's objects: each draw's vertex program assembled from the retail
// vertex program's pieces, chosen by its layer's switches, then its pixel side, either the texture
// stages' arithmetic with the specular color added after it, or the layer's normal-mapped pixel
// program; the alpha test, then the fog.
//
// A draw's vertices are read from its mesh's buffer (`MESH_VERTEX_BYTES` a vertex), a moved
// body's places and facings from its pose's (`POSED_VERTEX_BYTES` a vertex); its constants from
// the frame's draw buffer.
import bindless;
import light;

// A mesh vertex's bytes: its position, normal, diffuse color (four floats), two coordinate sets,
// its surface frame's u and v directions, and its direction toward the light.
static const uint MESH_VERTEX_BYTES = 92;
// A posed vertex's bytes: where it stands and which way it faces.
static const uint POSED_VERTEX_BYTES = 24;

struct Push
{
    // The frame's constants (the light it is drawn under), and its draws.
    uint frame;
    uint draws;
    uint draw;
    // The mesh's vertices; the frame's posed vertices (~0 for none) and the first of this draw's.
    uint vertices;
    uint posed;
    uint posed_base;
    // The three stages' textures and samplers.
    uint tex0;
    uint tex1;
    uint tex2;
    uint samp0;
    uint samp1;
    uint samp2;
    // The picture of the frame's bodies' spots of shade (~0 for none).
    uint spots;
    // How the draw's alpha is taken for its blend: as the program leaves it (0); replaced by the
    // frame's alpha (1), for a draw that blends by the destination's alpha, which the frame holds
    // the same everywhere; or held at most to one less the frame's alpha (2).
    uint alpha_mode;
    // How much of the sky reaches each pixel, the ambient occlusion's picture (~0: all of it).
    uint ao;
    // The draw's light class, which the depth's pass writes into the material picture.
    uint light_class;
};

// The frame's alpha under every draw: 127 of 255.
static const float FRAME_ALPHA = 127.0 / 255.0;

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct MeshIn
{
    float3 position;
    float3 normal;
    float4 diffuse;
    float2 uv0;
    float2 uv1;
    float3 tangent_u;
    float3 tangent_v;
    float3 to_light;
};

float3 load3(uint b, uint at)
{
    return asfloat(buffers[NonUniformResourceIndex(b)].Load3(at));
}

float2 load2(uint b, uint at)
{
    return asfloat(buffers[NonUniformResourceIndex(b)].Load2(at));
}

float4 load4(uint b, uint at)
{
    return asfloat(buffers[NonUniformResourceIndex(b)].Load4(at));
}

// Vertex `i` of the draw's mesh, placed and turned as its pose has it when it has one.
MeshIn mesh_vertex(uint i)
{
    uint at = i * MESH_VERTEX_BYTES;
    MeshIn v;
    v.position = load3(push.vertices, at);
    v.normal = load3(push.vertices, at + 12);
    v.diffuse = load4(push.vertices, at + 24);
    v.uv0 = load2(push.vertices, at + 40);
    v.uv1 = load2(push.vertices, at + 48);
    v.tangent_u = load3(push.vertices, at + 56);
    v.tangent_v = load3(push.vertices, at + 68);
    v.to_light = load3(push.vertices, at + 80);
    if (push.posed != 0xFFFFFFFF)
    {
        uint p = (push.posed_base + i) * POSED_VERTEX_BYTES;
        v.position = load3(push.posed, p);
        v.normal = load3(push.posed, p + 12);
    }
    return v;
}

struct MeshOut
{
    float4 clip : SV_Position;
    float4 color0 : COLOR0;
    float4 color1 : COLOR1;
    float2 uv0 : TEXCOORD0;
    float2 uv1 : TEXCOORD1;
    float2 uv2 : TEXCOORD2;
    float fog : TEXCOORD3;
    // For a program lit per pixel against the half vector: that vector in the surface's own frame,
    // times the square root of the light's attenuation; and 0.75 times the attenuation.
    float4 half_vector : TEXCOORD4;
    // What the draw's light and specular are multiplied by for the shadow about its spot.
    float3 ground_tint : TEXCOORD5;
    // The ambient's share of color 0 and of color 1, which the ambient occlusion takes away.
    float3 ambient0 : TEXCOORD6;
    float3 ambient1 : TEXCOORD7;
};

// The half vector between the eye and the light `to_light` at `world`, in the frame of the
// surface's normal map, a unit times the square root of the light's attenuation `att`; with 0.75
// times the attenuation.
float4 tex_half_vector(Light f, float3 world, float3 to_light, float att, float3 n, float3 tu, float3 tv)
{
    float3 e = f.eye.xyz - world;
    float el = length(e);
    if (el > 0.0)
    {
        e = e / el;
    }
    float3 h = e + to_light;
    float hl = length(h);
    if (hl > 0.0)
    {
        h = h / hl;
    }
    float3 t = float3(dot(tu, h), dot(tv, h), dot(n, h));
    float tl = length(t);
    if (tl > 0.0)
    {
        t = t / tl;
    }
    return float4(t * sqrt(att), 0.75 * att);
}

// The anisotropic coordinates at `world` on a surface facing `n`, the light `to_light` away: n.l,
// and n.h for the half vector between the light and the eye.
float2 anisotropic_uv(Light f, float3 world, float3 to_light, float3 n)
{
    float3 e = f.eye.xyz - world;
    float el = length(e);
    if (el > 0.0)
    {
        e = e / el;
    }
    float3 h = e + to_light;
    float hl = length(h);
    if (hl > 0.0)
    {
        h = h / hl;
    }
    return float2(dot(n, to_light), dot(n, h));
}

// The coordinates an environment map is looked up by at `world` on a surface facing `n`: the
// direction toward the eye reflected about the normal, along the camera's right and up, packed
// into 0..1.
float2 reflection_uv(Light f, float3 world, float3 n)
{
    float3 e = f.eye.xyz - world;
    float el = length(e);
    if (el > 0.0)
    {
        e = e / el;
    }
    float3 r = 2.0 * dot(n, e) * n - e;
    float rl = length(r);
    if (rl > 0.0)
    {
        r = r / rl;
    }
    float3 fw = f.forward.xyz;
    float3 right = cross(fw, float3(0.0, 0.0, 1.0));
    float l = length(right);
    if (l > 0.0)
    {
        right = right / l;
    }
    float3 up = cross(right, fw);
    return float2(dot(r, right), dot(r, up)) * 0.5 + 0.5;
}

// Stage `k`'s coordinates: its coordinate set through its transform.
float2 stage_uv(Draw d, uint k, MeshIn v)
{
    uint which = k == 0 ? d.uv_sets.x : (k == 1 ? d.uv_sets.y : d.uv_sets.z);
    float2 t = which != 0 ? v.uv1 : v.uv0;
    float4 a = d.uv_rows[2 * k];
    float4 b = d.uv_rows[2 * k + 1];
    return float2(dot(a.xyz, float3(t, 1.0)), dot(b.xyz, float3(t, 1.0)));
}

// A body's share of the sun: its spot's texel of the frame's spots (white with none).
float3 body_shade(int k)
{
    if (push.spots == 0xFFFFFFFF)
    {
        return float3(1.0, 1.0, 1.0);
    }
    return textures[NonUniformResourceIndex(push.spots)].Load(int3(k % 256, k / 256, 0)).rgb;
}

[shader("vertex")]
MeshOut vs_mesh(uint index : SV_VertexID)
{
    Light f = light_of(push.frame);
    Draw d = draw_of(push.draws, push.draw);
    MeshIn v = mesh_vertex(index);
    MeshOut o;
    uint sw = d.stage.x;
    // A frill's vertices take the shadow's color about its spot in place of their own (none in the
    // data reads it, so the frills are drawn as stored); a body's light takes its spot's shade.
    float4 vd = v.diffuse;
    float3 ground_tint = float3(1.0, 1.0, 1.0);
    if (d.shade.w > 1.5)
    {
        ground_tint = body_shade(int(d.shade.x));
    }
    o.ground_tint = ground_tint;
    float4 world4 = apply(d.model, float4(v.position, 1.0));
    float3 world = world4.xyz;
    o.clip = apply(f.view_proj, world4);
    o.half_vector = float4(0.0, 0.0, 0.0, 0.0);
    float4 c0 = float4(1.0, 1.0, 1.0, 1.0);
    float4 c1 = float4(0.0, 0.0, 0.0, 0.0);
    float3 mat_diffuse = d.diffuse.rgb * ground_tint;
    float3 amb = layer_ambient(f, d);
    o.ambient0 = float3(0.0, 0.0, 0.0);
    o.ambient1 = float3(0.0, 0.0, 0.0);
    if ((sw & LIT_IN_VERTICES) != 0 && d.program.x == 1)
    {
        // Lit in the vertices, normal-mapped: color 0 the vertex's direction toward the light in
        // the surface's own frame (its own lean plus the kept lights'), a unit, not packed; color 1
        // the vertex's light plus the kept lights' plus the ambient.
        float3 n = transform_normal(v.normal, d.model);
        KeptLight kept = kept_lights(d, world, n, transform_normal(v.tangent_u, d.model),
                                     transform_normal(v.tangent_v, d.model));
        float3 lean = v.to_light + kept.lean;
        float ll = length(lean);
        lean = ll > 0.0 ? lean / ll : float3(0.0, 0.0, 1.0);
        c0 = float4(lean, 1.0);
        c1 = apply_ambient(f, d, float4(vd.rgb + kept.color, 1.0));
        o.ambient1 = amb;
    }
    else if ((sw & LIT_IN_VERTICES) != 0 && (sw & DYNAMIC_LIGHTING) != 0)
    {
        // Lit in the vertices: the vertex's light plus the kept lights' plus the ambient.
        float3 n = transform_normal(v.normal, d.model);
        KeptLight kept = kept_lights(d, world, n, n, n);
        c0 = apply_ambient(f, d, float4(vd.rgb + kept.color, 1.0));
        o.ambient0 = amb;
    }
    else if (d.program.x == 1 && (d.stage.z == 1 || d.stage.y > 0))
    {
        // Normal-mapped with a point light, or inside with none: the first point light lights the
        // normal map; each other's diffuse share goes to color 1 in the color of the light before
        // it, and the sun's outside in its own; times the layer's diffuse, plus the ambient; every
        // light's specular intensity times its specular red to color 1's alpha.
        float3 n = transform_normal(v.normal, d.model);
        float3 tu = transform_normal(v.tangent_u, d.model);
        float3 tv = transform_normal(v.tangent_v, d.model);
        uint count = min(d.stage.y, 4u);
        float3 other = float3(0.0, 0.0, 0.0);
        float spec = 0.0;
        c0 = float4(0.5, 0.5, 0.5, 1.0);
        o.half_vector = tex_half_vector(f, world, float3(0.0, 0.0, 0.0), 1.0, n, tu, tv);
        for (uint k = 0; k < count; k++)
        {
            float3 r = pt_light(f, d, k, world, n, d.params.x);
            spec += r.y * r.z * d.pt_specular[k].r;
            if (k == 0)
            {
                float3 to = d.pt_origin[0].xyz - world;
                float3 t = float3(dot(tu, to), dot(tv, to), dot(n, to));
                float len = length(t);
                if (len > 0.0)
                {
                    t = t / len;
                }
                c0 = float4(packed_light(t, v.normal, v.tangent_u, v.tangent_v), r.z);
                o.half_vector = tex_half_vector(f, world, to, r.z, n, tu, tv);
            }
            else
            {
                other += r.x * d.pt_diffuse[k - 1].rgb;
            }
        }
        if (d.stage.z == 0)
        {
            float2 s = dir_light_diffuse_specular(f, n, d.params.x);
            other += s.x * f.sun_color.rgb;
            spec += s.y * f.sun_specular.r;
        }
        float3 c11 = (sw & MATERIAL_DIFFUSE) != 0 ? mat_diffuse : float3(1.0, 1.0, 1.0);
        if ((sw & UNMODULATED_COLOR1) != 0)
        {
            c1 = float4(other, spec);
        }
        else
        {
            c1 = apply_ambient(f, d, float4(other * c11, spec));
            o.ambient1 = amb;
        }
    }
    else if (d.stage.z == 1 && (sw & DYNAMIC_LIGHTING) != 0)
    {
        // Inside, lit: black plus each point light's share, times the layer's diffuse, plus the
        // ambient; the specular from each light, times the layer's specular color.
        float3 n = transform_normal(v.normal, d.model);
        uint count = min(d.stage.y, 4u);
        c0 = float4(0.0, 0.0, 0.0, 1.0);
        for (uint k = 0; k < count; k++)
        {
            float3 r = pt_light(f, d, k, world, n, d.params.x);
            c0.rgb += r.x * d.pt_diffuse[k].rgb;
            if ((sw & SPECULAR) != 0)
            {
                c1 = float4(c1.rgb + r.y * r.z * d.pt_specular[k].rgb, 0.0);
            }
        }
        c1 = float4(c1.rgb * (d.specular.rgb * ground_tint), 0.0);
        if ((sw & MATERIAL_DIFFUSE) != 0)
        {
            c0 = c0 * float4(ground_tint, 1.0) * d.diffuse;
        }
        c0 = apply_ambient(f, d, c0);
        o.ambient0 = amb;
        if ((sw & VERTEX_DIFFUSE) != 0)
        {
            c0 = c0 * vd;
            o.ambient0 = amb * vd.rgb;
        }
    }
    else if (d.program.x == 1)
    {
        // Normal-mapped by the sun alone: color 0 its direction in the surface's own frame, packed
        // into 0..1; color 1 the ambient, its alpha the sun's specular intensity times its
        // specular red.
        float3 n = transform_normal(v.normal, d.model);
        float3 tu = transform_normal(v.tangent_u, d.model);
        float3 tv = transform_normal(v.tangent_v, d.model);
        float3 l = tex_light_direction(f, n, tu, tv);
        c0 = float4(packed_light(l, v.normal, v.tangent_u, v.tangent_v), 1.0);
        o.half_vector = tex_half_vector(f, world, f.to_sun.xyz, 1.0, n, tu, tv);
        float2 s = dir_light_diffuse_specular(f, n, d.params.x);
        if ((sw & UNMODULATED_COLOR1) != 0)
        {
            c1 = float4(0.0, 0.0, 0.0, s.y * f.sun_specular.r);
        }
        else
        {
            c1 = apply_ambient(f, d, float4(0.0, 0.0, 0.0, s.y * f.sun_specular.r));
            o.ambient1 = amb;
        }
    }
    else if ((sw & DYNAMIC_LIGHTING) != 0)
    {
        // Lit: black, plus the sun's share and each point light's beside it, times the layer's
        // diffuse, plus the ambient; the specular from the sun and each of them, times the layer's
        // specular color.
        c0 = float4(0.0, 0.0, 0.0, 1.0);
        float3 n = transform_normal(v.normal, d.model);
        float2 s = dir_light_diffuse_specular(f, n, d.params.x);
        c0.rgb += s.x * f.sun_color.rgb;
        float3 spec = s.y * f.sun_specular.rgb;
        uint count = min(d.stage.y, 4u);
        for (uint k = 0; k < count; k++)
        {
            float3 r = pt_light(f, d, k, world, n, d.params.x);
            c0.rgb += r.x * d.pt_diffuse[k].rgb;
            spec += r.y * r.z * d.pt_specular[k].rgb;
        }
        if ((sw & SPECULAR) != 0)
        {
            c1 = float4(spec * (d.specular.rgb * ground_tint), 0.0);
        }
        if ((sw & MATERIAL_DIFFUSE) != 0)
        {
            c0 = c0 * float4(ground_tint, 1.0) * d.diffuse;
        }
        c0 = apply_ambient(f, d, c0);
        o.ambient0 = amb;
        if ((sw & VERTEX_DIFFUSE) != 0)
        {
            c0 = c0 * vd;
            o.ambient0 = amb * vd.rgb;
        }
    }
    else
    {
        // Unlit: white, times the layer's diffuse, times the vertex's color.
        if ((sw & MATERIAL_DIFFUSE) != 0)
        {
            c0 = c0 * float4(ground_tint, 1.0) * d.diffuse;
        }
        if ((sw & VERTEX_DIFFUSE) != 0)
        {
            c0 = c0 * vd;
        }
    }
    // Normal-mapped by one light, its specular worked out per pixel: color 1 is black, its alpha
    // three quarters of the light's attenuation.
    uint one_light = PIXEL_LIGHTING | PIXEL_SPECULAR | PIXEL_MULTIPLE_LIGHTS;
    if (d.program.x == 1 && (sw & LIT_IN_VERTICES) == 0 && (sw & one_light) == (PIXEL_LIGHTING | PIXEL_SPECULAR))
    {
        c1 = float4(0.0, 0.0, 0.0, 0.75 * c0.a);
        o.ambient1 = float3(0.0, 0.0, 0.0);
    }
    // The alpha fog: the color's alpha fades out between the alpha fog's distances.
    if ((sw & DEPTH_ALPHA_FOG) != 0 && d.program.x == 0)
    {
        float dist = distance(world, f.eye.xyz);
        c0.a = c0.a * clamp((f.alpha_fog.y - dist) * f.alpha_fog.z, 0.0, 1.0);
    }
    // The device clamps both colors to [0, 1] before the texture stages; a direction toward the
    // light is held to 0..1 where each pixel reads it, not at the vertices.
    if ((sw & LIT_IN_VERTICES) != 0 && d.program.x == 1)
    {
        o.color0 = c0;
    }
    else
    {
        o.color0 = saturate(c0);
    }
    o.color1 = saturate(c1);
    o.uv0 = stage_uv(d, 0, v);
    o.uv1 = stage_uv(d, 1, v);
    o.uv2 = stage_uv(d, 2, v);
    // A stage reading the anisotropic coordinates: the sun outside, the first point light inside.
    if (d.uv_sets.x == 2 || d.uv_sets.y == 2 || d.uv_sets.z == 2)
    {
        float3 n = transform_normal(v.normal, d.model);
        float3 to = f.to_sun.xyz;
        if (d.stage.z == 1)
        {
            to = d.stage.y > 0 ? d.pt_origin[0].xyz - world : float3(0.0, 0.0, 0.0);
        }
        float2 a = anisotropic_uv(f, world, to, n);
        if (d.uv_sets.x == 2)
        {
            o.uv0 = a;
        }
        if (d.uv_sets.y == 2)
        {
            o.uv1 = a;
        }
        if (d.uv_sets.z == 2)
        {
            o.uv2 = a;
        }
    }
    // A stage reading the eye's reflection: an environment map.
    if (d.uv_sets.x == 3 || d.uv_sets.y == 3 || d.uv_sets.z == 3)
    {
        float2 r = reflection_uv(f, world, transform_normal(v.normal, d.model));
        if (d.uv_sets.x == 3)
        {
            o.uv0 = r;
        }
        if (d.uv_sets.y == 3)
        {
            o.uv1 = r;
        }
        if (d.uv_sets.z == 3)
        {
            o.uv2 = r;
        }
    }
    o.fog = (sw & DEPTH_FOG) != 0 ? linear_fog(f, world) : 1.0;
    return o;
}

float4 tex(uint t, uint s, float2 uv)
{
    return sample(t, s, uv);
}

// The device's texture stages, in order: each stage's color and alpha from its own arguments, the
// result held within 0..1 and passed on as the next stage's current color; the first disabled
// stage ends the chain.
float4 texture_stages(Draw dr, MeshOut v)
{
    float4 t[3] = { tex(push.tex0, push.samp0, v.uv0), tex(push.tex1, push.samp1, v.uv1),
                    tex(push.tex2, push.samp2, v.uv2) };
    float4 d = v.color0;
    float4 s = v.color1;
    float4 cur = d;
    for (uint k = 0; k < 3; k++)
    {
        uint4 o = dr.ops[k];
        if (o.x == 1)
        {
            break;
        }
        float4 c = stage_op(dr, o.x, stage_arg(dr, v.ground_tint, o.y & 0xFF, t[k], cur, d, s),
                            stage_arg(dr, v.ground_tint, (o.y >> 8) & 0xFF, t[k], cur, d, s), t[k], cur, d);
        uint aop = o.z;
        uint aargs = o.w;
        if (aop == 1)
        {
            // Alpha may not be disabled while color runs: it passes the current alpha on.
            aop = 2;
            aargs = 1;
        }
        float4 a = stage_op(dr, aop, stage_arg(dr, v.ground_tint, aargs & 0xFF, t[k], cur, d, s),
                            stage_arg(dr, v.ground_tint, (aargs >> 8) & 0xFF, t[k], cur, d, s), t[k], cur, d);
        cur = saturate(float4(c.rgb, a.a));
    }
    return cur;
}

// The normal-mapped pixel program: the normal map (stage 0) lit by the light's packed direction,
// times the layer's lit diffuse color, plus color 1; doubled against the diffuse map (stage 1);
// plus the specular intensity times the layer's specular color. A detail map (stage 2) is added,
// signed, to the diffuse map or to the normal map; an environment map there is added to the
// diffuse map, or added times its alpha. Some programs read their stages and colors otherwise.
float4 bump_program(Light f, Draw dr, MeshOut v, float ao)
{
    uint opts = dr.program.y;
    float4 nm = tex(push.tex0, push.samp0, v.uv0);
    float4 dm = tex(push.tex1, push.samp1, v.uv1);
    float4 detail = tex(push.tex2, push.samp2, v.uv2);
    if ((opts & NORMAL_DETAIL_FIRST) != 0)
    {
        nm = float4(nm.rgb + dm.rgb - 0.5, nm.a);
        dm = detail;
    }
    if ((opts & DETAIL_ON_DIFFUSE) != 0)
    {
        dm = float4(dm.rgb + detail.rgb - 0.5, dm.a);
    }
    if ((opts & ENVIRONMENT_ADD) != 0)
    {
        dm = float4(dm.rgb + detail.rgb, dm.a);
    }
    if ((opts & ENVIRONMENT_GLOSS) != 0)
    {
        dm = float4(dm.rgb + detail.rgb * dm.a, dm.a);
    }
    if ((opts & DETAIL_ON_NORMAL) != 0)
    {
        nm = float4(nm.rgb + detail.rgb - 0.5, nm.a);
    }
    float3 n = nm.rgb * 2.0 - 1.0;
    float3 l = v.color0.rgb * 2.0 - 1.0;
    float facing = saturate(dot(n, l));
    float dd = facing * v.color0.a;
    float3 ambient = layer_ambient(f, dr) * ao;
    float3 g = v.ground_tint;
    float3 rgb = (dd * (dr.lit_diffuse.rgb * g) + v.color1.rgb) * 2.0 * dm.rgb;
    if ((opts & TINT_MASK) != 0)
    {
        // The layer's color where the diffuse map's alpha is full, white where it is clear.
        float3 tint = lerp(float3(1.0, 1.0, 1.0), dr.diffuse.rgb, dm.a);
        rgb = ((dd * (dr.light.rgb * g) + v.color1.rgb) * tint + ambient) * 2.0 * dm.rgb;
    }
    if ((opts & COLOR1_HALF_VECTOR) != 0)
    {
        rgb = (facing * (dr.lit_diffuse.rgb * g) + ambient) * 2.0 * dm.rgb;
    }
    if ((opts & ANISOTROPIC) != 0)
    {
        rgb = rgb * 2.0 * detail.rgb;
    }
    float a = dr.diffuse.a;
    if ((opts & ALPHA_FROM_DIFFUSE) != 0)
    {
        a = a * dm.a;
    }
    float3 spec = v.color1.a * (dr.specular.rgb * g);
    if ((opts & HALF_VECTOR_SPECULAR) != 0)
    {
        // The half vector comes as a coordinate set: packed into 0..1, held there, unpacked; its
        // facing on the normal map squared, less 0.75 of the attenuation, four times.
        float3 packed = saturate((v.half_vector.xyz + 1.0) * 0.5);
        float3 h = packed * 2.0 - 1.0;
        float x = saturate(dot(n, h));
        spec = saturate(4.0 * (x * x - v.half_vector.w)) * (dr.specular.rgb * g);
    }
    if ((opts & COLOR1_HALF_VECTOR) != 0)
    {
        // Toward color 1's direction, packed: its facing on the normal map squared, less 0.75,
        // four times.
        float3 h = v.color1.rgb * 2.0 - 1.0;
        float x = saturate(dot(n, h));
        spec = saturate(4.0 * (x * x - 0.75)) * (dr.specular.rgb * g);
    }
    if ((opts & GLOSS) != 0)
    {
        spec = spec * dm.a;
    }
    return float4(rgb + spec, a);
}

// The normal-mapped program of a surface lit in its vertices: the diffuse map (with its detail),
// doubled, times the vertex's light and the normal map's facing of the light, plus the diffuse map
// times the ambient. The direction toward the light comes as a unit, each part held to 0..1 and
// then unpacked as a packed one would be.
float4 vertex_lit_program(Light f, Draw dr, MeshOut v, float ao)
{
    uint opts = dr.program.y;
    float4 nm = tex(push.tex0, push.samp0, v.uv0);
    float4 dm = tex(push.tex1, push.samp1, v.uv1);
    float4 detail = tex(push.tex2, push.samp2, v.uv2);
    if ((opts & DETAIL_ON_DIFFUSE) != 0)
    {
        dm = float4(dm.rgb + detail.rgb - 0.5, dm.a);
    }
    if ((opts & DETAIL_ON_NORMAL) != 0)
    {
        nm = float4(nm.rgb + detail.rgb - 0.5, nm.a);
    }
    float3 n = nm.rgb * 2.0 - 1.0;
    float3 l = saturate(v.color0.rgb) * 2.0 - 1.0;
    float dd = saturate(dot(n, l));
    float3 ambient = layer_ambient(f, dr) * ao;
    float3 rgb = 2.0 * dm.rgb * v.color1.rgb * dd + dm.rgb * ambient;
    float a = dr.diffuse.a;
    if ((opts & ALPHA_FROM_DIFFUSE) != 0)
    {
        a = a * dm.a;
    }
    return float4(rgb, a);
}

// How much of the sky reaches the pixel at `position`: the ambient occlusion's picture there, or
// all of it with none.
float ambient_visibility(float4 position)
{
    if (push.ao == 0xFFFFFFFF)
    {
        return 1.0;
    }
    return textures[NonUniformResourceIndex(push.ao)].Load(int3(int2(position.xy), 0)).x;
}

// `v` with the ambient's share of its colors taken by the occlusion `ao`: at 1 (open sky) the
// colors are the retail ones.
MeshOut occluded(MeshOut v, float ao)
{
    MeshOut o = v;
    float shut = 1.0 - ao;
    o.color0 = float4(max(v.color0.rgb - shut * v.ambient0, 0.0), v.color0.a);
    o.color1 = float4(max(v.color1.rgb - shut * v.ambient1, 0.0), v.color1.a);
    return o;
}

// The draw's color and alpha before the alpha test and the fog, its ambient taken by `ao`.
float4 surface(Light f, Draw d, MeshOut v, float ao)
{
    MeshOut w = occluded(v, ao);
    if (d.program.x == 1 && (d.stage.x & LIT_IN_VERTICES) != 0)
    {
        return vertex_lit_program(f, d, w, ao);
    }
    if (d.program.x == 1)
    {
        return bump_program(f, d, w, ao);
    }
    float4 s = texture_stages(d, w);
    return float4(s.rgb + w.color1.rgb, s.a);
}

// The depth's pass: what the draw covers, cut where its alpha fails the test as the frame's draw
// does, its light class into the material picture.
[shader("fragment")]
float4 fs_mesh_depth(MeshOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    Draw d = draw_of(push.draws, push.draw);
    float4 c = surface(f, d, v, 1.0);
    float a = d.program.x == 1 ? saturate(c.a) : c.a;
    if (a <= d.params.y)
    {
        discard;
    }
    return float4(float(push.light_class) / 255.0, 0.0, 0.0, 0.0);
}

[shader("fragment")]
float4 fs_mesh(MeshOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    Draw d = draw_of(push.draws, push.draw);
    float4 c = surface(f, d, v, ambient_visibility(v.clip));
    float a = d.program.x == 1 ? saturate(c.a) : c.a;
    if (a <= d.params.y)
    {
        discard;
    }
    if (push.alpha_mode == 1)
    {
        a = FRAME_ALPHA;
    }
    else if (push.alpha_mode == 2)
    {
        a = min(saturate(a), 1.0 - FRAME_ALPHA);
    }
    return float4(fogged(f, saturate(c.rgb), v.fog), a);
}

// The ripples' picture on a water surface: the bump picture's push where it lies (the picture keeps
// only what is above zero, as a picture that holds no value below zero does).
[shader("fragment")]
float4 fs_ripple(MeshOut v) : SV_Target0
{
    return float4(tex(push.tex0, push.samp0, v.uv0).rg, 0.0, 1.0);
}

// The reflection (stage 0, the mirrored scene) where this point lies on the screen, turned over top
// to bottom as the mirrored camera sees it, its place pushed by the ripples' picture (stage 1,
// where this point lies on the screen) by the draw's push (0.04 of the picture per unit of offset,
// 0 with no ripples); laid over by the frame's alpha.
[shader("fragment")]
float4 fs_water(MeshOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    Draw d = draw_of(push.draws, push.draw);
    float2 screen = v.clip.xy / f.viewport.xy;
    float2 ripple = tex(push.tex1, push.samp1, screen).rg;
    float2 at = float2(screen.x, 1.0 - screen.y) + d.params.z * ripple;
    float3 c = tex(push.tex0, push.samp0, at).rgb;
    return float4(c, FRAME_ALPHA);
}
