// The game's own light, as the retail device works it out: the frame's light, each draw's layer,
// the vertex program's pieces, the texture stages' arithmetic and the normal-mapped pixel programs.
// Everything is in display values, as the retail device blends them, with no gamma conversion.
//
// A frame's constants are the shared rows (`frame`) and, from row `LIGHT_ROWS`, the game's own
// (`Light`); a draw's are `DRAW_ROWS` rows of the frame's draw buffer (`Draw`). With the light's
// masks at their identity (every pixel fully sunlit, unoccluded, no lamps), every program here is
// the retail one.
module light;

import bindless;
__exported import frame;

// The first of the frame's own rows, after the shared ones.
public static const uint LIGHT_ROWS = 32;

// The frame's own rows, from `LIGHT_ROWS`:
//   0..3   the view and projection together, as four columns
//   4      the eye
//   5      the fog: start, end, 1 / (end - start), 1 when it is on
//   6      the fog's color
//   7      the direction toward the sun, a unit
//   8      the sun's diffuse color times its brightness
//   9      the ambient light: its color times its brightness
//   10     the direction the camera looks
//   11     the sun's specular color: its color, not scaled by its brightness
//   12     the alpha fog: start, end, 1 / (end - start)
//   13..16 the world to the sun's shadow map's picture coordinates, as four columns
//   17     the clouds' shadows: picture repeats per meter, the slide along u and v, 1 when drawn
//   18     x 1 when the near terrain's shadows and fog are laid on after its layers
//   19     the picture's width and height in pixels
public struct Light
{
    public float4x4 view_proj;
    public float4 eye;
    public float4 fog;
    public float4 fog_color;
    public float4 to_sun;
    public float4 sun_color;
    public float4 ambient;
    public float4 forward;
    public float4 sun_specular;
    public float4 alpha_fog;
    public float4x4 shadow_uv;
    public float4 cloud;
    public float4 terrain;
    public float4 viewport;
};

// The matrix whose columns are rows `i` to `i + 3` of buffer `b`.
float4x4 columns(uint b, uint i)
{
    return float4x4(row(b, i), row(b, i + 1), row(b, i + 2), row(b, i + 3));
}

// The frame's own light, from its constants in buffer `b`.
public Light light_of(uint b)
{
    uint r = LIGHT_ROWS;
    Light l;
    l.view_proj = columns(b, r);
    l.eye = row(b, r + 4);
    l.fog = row(b, r + 5);
    l.fog_color = row(b, r + 6);
    l.to_sun = row(b, r + 7);
    l.sun_color = row(b, r + 8);
    l.ambient = row(b, r + 9);
    l.forward = row(b, r + 10);
    l.sun_specular = row(b, r + 11);
    l.alpha_fog = row(b, r + 12);
    l.shadow_uv = columns(b, r + 13);
    l.cloud = row(b, r + 17);
    l.terrain = row(b, r + 18);
    l.viewport = row(b, r + 19);
    return l;
}

// `m` (columns) applied to `v`, as the shaders' matrices are read.
public float4 apply(float4x4 m, float4 v)
{
    return m[0] * v.x + m[1] * v.y + m[2] * v.z + m[3] * v.w;
}

// The rows one draw takes in the frame's draw buffer.
public static const uint DRAW_ROWS = 36;

// A draw's constants: its place, its layer's colors, its switches, the texture stages' arithmetic,
// its pixel program, its coordinate sets and their transforms, its point lights, a frill's spot
// and the first light's own color.
public struct Draw
{
    public float4x4 model;
    // the layer's diffuse color times the tint
    public float4 diffuse;
    // the sun's diffuse color times the layer's diffuse color, each at most 1 (alpha: the layer's,
    // at most 1): the pixel programs' lit color
    public float4 lit_diffuse;
    // the layer's specular color times the tint
    public float4 specular;
    // the layer's ambient color
    public float4 layer_ambient;
    // specular power, alpha reference, the ripples' push (the water's reflection), unused
    public float4 params;
    // switches, the point lights' count, 1 inside, unused
    public uint4 stage;
    // the texture factor: the layer's diffuse color times the tint, as a stored color
    public float4 tfactor;
    // the first three stages' arithmetic: color op, color args (arg1 | arg2 << 8), alpha op, alpha
    // args; a stage past the layer's last is disabled (op 1)
    public uint4 ops[3];
    // the pixel program (0 the stage arithmetic, 1 the normal-mapped program), its options, unused
    public uint4 program;
    // the coordinate set each of the first three stages reads (2: the anisotropic coordinates; 3:
    // the eye's reflection about the surface)
    public uint4 uv_sets;
    // each of the first three stages' coordinate transform, two rows each
    public float4 uv_rows[6];
    // the point lights: each its place (w: 10 / range squared), its diffuse color and its specular
    public float4 pt_origin[4];
    public float4 pt_diffuse[4];
    public float4 pt_specular[4];
    // a frill's spot on the ground (w: 1), whose shadow its vertices take for their color; a
    // body's (w: 2), whose spot's texel it reads
    public float4 shade;
    // the first light's own diffuse color: the pixel programs' light color
    public float4 light;
};

// Draw `k`'s constants, from buffer `b`.
public Draw draw_of(uint b, uint k)
{
    uint r = k * DRAW_ROWS;
    Draw d;
    d.model = columns(b, r);
    d.diffuse = row(b, r + 4);
    d.lit_diffuse = row(b, r + 5);
    d.specular = row(b, r + 6);
    d.layer_ambient = row(b, r + 7);
    d.params = row(b, r + 8);
    d.stage = row_u(b, r + 9);
    d.tfactor = row(b, r + 10);
    for (uint i = 0; i < 3; i++)
    {
        d.ops[i] = row_u(b, r + 11 + i);
    }
    d.program = row_u(b, r + 14);
    d.uv_sets = row_u(b, r + 15);
    for (uint i = 0; i < 6; i++)
    {
        d.uv_rows[i] = row(b, r + 16 + i);
    }
    for (uint i = 0; i < 4; i++)
    {
        d.pt_origin[i] = row(b, r + 22 + i);
        d.pt_diffuse[i] = row(b, r + 26 + i);
        d.pt_specular[i] = row(b, r + 30 + i);
    }
    d.shade = row(b, r + 34);
    d.light = row(b, r + 35);
    return d;
}

// The layer's switches.
public static const uint DYNAMIC_LIGHTING = 0x1;
public static const uint PIXEL_LIGHTING = 0x2;
public static const uint SPECULAR = 0x4;
public static const uint PIXEL_SPECULAR = 0x8;
public static const uint MATERIAL_DIFFUSE = 0x10;
public static const uint VERTEX_DIFFUSE = 0x20;
public static const uint PIXEL_MULTIPLE_LIGHTS = 0x80;
public static const uint DEPTH_FOG = 0x100;
public static const uint DEPTH_ALPHA_FOG = 0x1000;
// A normal-mapped layer's color 1 carries the other lights alone, neither times the layer's
// diffuse nor with the ambient.
public static const uint UNMODULATED_COLOR1 = 0x2000;
// An interior cell's draws: the light is in the vertices' colors, no sun.
public static const uint LIT_IN_VERTICES = 0x40000000;

// The normal-mapped pixel program's options.
public static const uint DETAIL_ON_DIFFUSE = 0x1;
public static const uint DETAIL_ON_NORMAL = 0x2;
public static const uint ALPHA_FROM_DIFFUSE = 0x4;
public static const uint GLOSS = 0x8;
public static const uint HALF_VECTOR_SPECULAR = 0x10;
public static const uint ENVIRONMENT_ADD = 0x20;
public static const uint ENVIRONMENT_GLOSS = 0x40;
public static const uint ANISOTROPIC = 0x80;
public static const uint NORMAL_DETAIL_FIRST = 0x100;
public static const uint TINT_MASK = 0x200;
public static const uint COLOR1_HALF_VECTOR = 0x400;

// How much of the surface at `world` shows through the frame's fog, by distance.
public float linear_fog(Light f, float3 world)
{
    if (f.fog.w < 0.5)
    {
        return 1.0;
    }
    float d = distance(world, f.eye.xyz);
    return clamp((f.fog.y - d) * f.fog.z, 0.0, 1.0);
}

// `c` fogged by `fog`, the share of it that shows.
public float3 fogged(Light f, float3 c, float fog)
{
    return lerp(f.fog_color.rgb, c, fog);
}

// The normal `n` into the world's frame by `model`, a unit (straight up when it has no length).
public float3 transform_normal(float3 n, float4x4 model)
{
    float3 w = model[0].xyz * n.x + model[1].xyz * n.y + model[2].xyz * n.z;
    float l = length(w);
    if (l > 0.0)
    {
        return w / l;
    }
    return float3(0.0, 0.0, 1.0);
}

// lit(N.L, N.H, power): max(N.L, 0), and N.H^power where N.L and N.H are both above 0, the power
// held within the vertex program's limit of 128.
public float2 dir_light_diffuse_specular(Light f, float3 n, float power)
{
    float nl = dot(n, f.to_sun.xyz);
    float3 h = normalize(f.to_sun.xyz - f.forward.xyz);
    float nh = dot(n, h);
    float spec = 0.0;
    if (nl > 0.0 && nh > 0.0)
    {
        spec = pow(nh, clamp(power, -127.9961, 127.9961));
    }
    return float2(max(nl, 0.0), spec);
}

// The sun's direction in the frame of the surface's normal map, a unit.
public float3 tex_light_direction(Light f, float3 n, float3 tu, float3 tv)
{
    float3 l = f.to_sun.xyz;
    float3 t = float3(dot(tu, l), dot(tv, l), dot(n, l));
    float len = length(t);
    if (len > 0.0)
    {
        return t / len;
    }
    return float3(0.0, 0.0, 1.0);
}

// Whether every part of `v` is a number: its exponent's bits are not all set. Read from the bits,
// so no compiler's float rules can fold it away.
public bool finite3(float3 v)
{
    uint3 e = asuint(v) & 0x7f800000u;
    return all(e != 0x7f800000u);
}

// A direction toward the light in the surface's own frame, packed into 0..1 as color 0 carries
// it; white where the stored surface frame is not a number, as the retail device reads each such
// part as 1.
public float3 packed_light(float3 l, float3 normal, float3 tangent_u, float3 tangent_v)
{
    if (!(finite3(normal) && finite3(tangent_u) && finite3(tangent_v)))
    {
        return float3(1.0, 1.0, 1.0);
    }
    return (l + 1.0) * 0.5;
}

// Point light `k`'s share at `world` on a surface facing `n`: how squarely its light meets the
// surface times its attenuation 1 / (1 + 10 d^2 / R^2); the specular intensity with the half vector
// toward the viewer, times the attenuation; and the attenuation.
public float3 pt_light(Light f, Draw d, uint k, float3 world, float3 n, float power)
{
    float4 o = d.pt_origin[k];
    float3 to = o.xyz - world;
    float d2 = dot(to, to);
    float att = 1.0 / (1.0 + o.w * d2);
    float3 l = to * rsqrt(max(d2, 1e-12));
    float nl = dot(n, l);
    float nh = dot(n, -f.forward.xyz);
    float spec = 0.0;
    if (nl > 0.0 && nh > 0.0)
    {
        spec = pow(nh, clamp(power, -127.9961, 127.9961)) * att;
    }
    return float3(max(nl, 0.0) * att, spec, att);
}

// The kept point lights of a surface lit in its vertices: their color at `world` on a surface
// facing `n` (along `tu` and `tv`), and their lean in the surface's own frame.
public struct KeptLight
{
    public float3 color;
    public float3 lean;
};

public KeptLight kept_lights(Draw d, float3 world, float3 n, float3 tu, float3 tv)
{
    KeptLight o;
    o.color = float3(0.0, 0.0, 0.0);
    o.lean = float3(0.0, 0.0, 0.0);
    uint count = min(d.stage.y, 4u);
    for (uint k = 0; k < count; k++)
    {
        float4 p = d.pt_origin[k];
        float3 to = p.xyz - world;
        float d2 = dot(to, to);
        float att = 1.0 / (1.0 + p.w * d2);
        float3 l = to * rsqrt(max(d2, 1e-12));
        o.color += d.pt_diffuse[k].rgb * max(dot(n, l), 0.0) * att;
        float3 seen = float3(dot(tu, to), dot(tv, to), dot(n, to));
        float range2 = 10.0 / max(p.w, 1e-12);
        float fall = max(1.0 - dot(seen, seen) / range2, 0.0);
        float sl = length(seen);
        if (sl > 0.0)
        {
            o.lean += seen / sl * fall;
        }
    }
    return o;
}

// The ambient light the layer takes: the frame's ambient times the layer's, at most 1.
public float3 layer_ambient(Light f, Draw d)
{
    return min(f.ambient.rgb * d.layer_ambient.rgb, 1.0);
}

// `c` with the layer's ambient added.
public float4 apply_ambient(Light f, Draw d, float4 c)
{
    return float4(c.rgb + layer_ambient(f, d), c.a);
}

// A stage argument: the vertex's diffuse (0) or specular (4) color, the previous stage's result (1),
// the stage's texture (2), the texture factor (3); 0x10 takes its complement, 0x20 copies its
// alpha into every channel.
public float4 stage_arg(Draw dr, float3 ground_tint, uint a, float4 t, float4 cur, float4 d, float4 s)
{
    float4 x = cur;
    switch (a & 0xF)
    {
    case 0:
        x = d;
        break;
    case 2:
        x = t;
        break;
    case 3:
        x = float4(dr.tfactor.rgb * ground_tint, dr.tfactor.a);
        break;
    case 4:
        x = s;
        break;
    default:
        x = cur;
        break;
    }
    if ((a & 0x20) != 0)
    {
        x = float4(x.a, x.a, x.a, x.a);
    }
    if ((a & 0x10) != 0)
    {
        x = 1.0 - x;
    }
    return x;
}

// A stage operation on its two arguments; the blending ones also read the diffuse, texture, factor
// or current alpha.
public float4 stage_op(Draw dr, uint op, float4 a1, float4 a2, float4 t, float4 cur, float4 d)
{
    switch (op)
    {
    case 2:
        return a1;
    case 3:
        return a2;
    case 4:
        return a1 * a2;
    case 5:
        return 2.0 * a1 * a2;
    case 6:
        return 4.0 * a1 * a2;
    case 7:
        return a1 + a2;
    case 8:
        return a1 + a2 - 0.5;
    case 9:
        return 2.0 * (a1 + a2 - 0.5);
    case 10:
        return a1 - a2;
    case 11:
        return a1 + a2 - a1 * a2;
    case 12:
        return lerp(a2, a1, d.a);
    case 13:
        return lerp(a2, a1, t.a);
    case 14:
        return lerp(a2, a1, dr.tfactor.a);
    case 15:
        return a1 + a2 * (1.0 - t.a);
    case 16:
        return lerp(a2, a1, cur.a);
    case 18:
        return float4(a1.rgb + a1.a * a2.rgb, a1.a);
    case 19:
        return float4(a1.rgb * a2.rgb + a1.a, a1.a);
    case 20:
        return float4((1.0 - a1.a) * a2.rgb + a1.rgb, a1.a);
    case 21:
        return float4((1.0 - a1.rgb) * a2.rgb + a1.a, a1.a);
    case 24:
    {
        float x = 4.0 * dot(a1.rgb - 0.5, a2.rgb - 0.5);
        return float4(x, x, x, x);
    }
    default:
        return a1 * a2;
    }
}
