// The remaster's acceleration structures, as the shared traced passes read them (`rt_shadow` and
// `rt_ao`), where the device traces rays.
//
// The top-level structure holds the frame's casters of the sun's shadows and the cells it draws,
// each draw an instance of its mesh's triangles, in the frame's own space. Every instance's mask is
// `RT_OBJECT`: the land is not in it. Its index of the user's names its two rows in the instances'
// table, `uint4(vertices, indices, the draw's constants' place, flags)` and `uint4(texture,
// sampler, sway, 0)`: the mesh's vertices and indices by their index in the table, the flags 1
// alpha-tested and 8 swayed by the wind, the picture and sampler of the draw's first stage, and
// how far the wind bends it at each meter over its origin (a float's bits).
//
// Every geometry is opaque but the alpha-tested and the swaying ones, whose candidate hits a query
// is handed to test; no instance culls either face.
module rt;

import bindless;
import frame;
import light;

// The masks the traced passes ask for. Nothing here is the ground, or ground cover.
public static const uint RT_GROUND = 4;
public static const uint RT_OBJECT = 8;
public static const uint RT_COVER = 32;

// An instance's flags.
public static const uint RT_ALPHA_TESTED = 1;
public static const uint RT_SWAYS = 8;

// A mesh vertex's bytes, its position first and its two coordinate sets at 40 and 48.
static const uint VERTEX_BYTES = 92;

float3 position_of(uint vertices, uint v)
{
    return asfloat(buffers[NonUniformResourceIndex(vertices)].Load3(v * VERTEX_BYTES));
}

// Index `i` of a list of 16-bit indices.
uint index_of(uint indices, uint i)
{
    uint w = buffers[NonUniformResourceIndex(indices)].Load((i * 2) & ~3u);
    return (i & 1) != 0 ? (w >> 16) : (w & 0xFFFF);
}

// Whether the ray from `o` along `d` meets the triangle `a`, `b`, `c` (either face) past `tmin` and
// before `tmax`: where along it (`t`) and where in the triangle (`bary`, of `b` and `c`).
bool meets(float3 o, float3 d, float tmin, float tmax, float3 a, float3 b, float3 c, out float t,
           out float2 bary)
{
    t = 0.0;
    bary = float2(0.0, 0.0);
    float3 e1 = b - a;
    float3 e2 = c - a;
    float3 p = cross(d, e2);
    float det = dot(e1, p);
    if (abs(det) < 1e-12)
    {
        return false;
    }
    float inv = 1.0 / det;
    float3 s = o - a;
    float u = dot(s, p) * inv;
    float3 q = cross(s, e1);
    float v = dot(d, q) * inv;
    if (u < 0.0 || v < 0.0 || u + v > 1.0)
    {
        return false;
    }
    t = dot(e2, q) * inv;
    bary = float2(u, v);
    return t > tmin && t < tmax;
}

// Whether a candidate hit on an alpha-tested draw is kept, as the sun's casters are drawn into
// the cascades' maps: where the first stage's alpha operation on its picture's alpha is at least
// a half, at the place `bary` of triangle `primitive`.
bool alpha_kept(uint4 a, uint4 b, uint draws, uint primitive, float2 bary)
{
    Draw dr = draw_of(draws, a.z);
    float w[3] = { 1.0 - bary.x - bary.y, bary.x, bary.y };
    float2 uv = float2(0.0, 0.0);
    bool second = dr.uv_sets.x != 0;
    for (uint k = 0; k < 3; k++)
    {
        uint v = index_of(a.y, primitive * 3 + k);
        uint at = v * VERTEX_BYTES + (second ? 48 : 40);
        uv += asfloat(buffers[NonUniformResourceIndex(a.x)].Load2(at)) * w[k];
    }
    // The first stage's coordinates, through its transform.
    float3 h = float3(uv, 1.0);
    float2 st = float2(dot(dr.uv_rows[0].xyz, h), dot(dr.uv_rows[1].xyz, h));
    float4 t = textures[NonUniformResourceIndex(b.x)].SampleLevel(
        samplers[NonUniformResourceIndex(b.y)], st, 0.0);
    uint op = dr.ops[0].z;
    if (op == 1)
    {
        op = 3;
    }
    float4 one = float4(1.0, 1.0, 1.0, 1.0);
    return saturate(stage_op(dr, op, float4(t.a, t.a, t.a, t.a), one, t, one, one).a) >= 0.5;
}

// Whether a query keeps a candidate hit, and where, as the traced passes ask: `rows` is the
// instances' table and `draws` the frame's draws' constants (the passes' first two tables); the
// four pools are not read. The structures hold a swaying draw as it stands still, so a candidate
// on one is tested again against its triangle as the frame draws it, bent by the frame's wind
// (`frame`'s constants): a ray leaving a bent leaf then never meets that leaf's still self. Such a
// hit's distance is `t`; any other leaves `t` negative, the candidate's own. An alpha-tested draw
// is then tested at its texel.
public bool hit_kept(uint frame, uint rows, uint draws, uint pool0, uint pool1, uint pool2,
                     uint pool3, uint custom, uint geometry, uint primitive, float2 bary, float3 o,
                     float3 d, float tmin, float tmax, float3x4 object_to_world, out float t)
{
    t = -1.0;
    uint4 a = row_u(rows, custom * 2);
    uint4 b = row_u(rows, custom * 2 + 1);
    float4 wind = row(frame, FRAME_WIND);
    if ((a.w & RT_SWAYS) != 0 && dot(wind.xy, wind.xy) >= 1e-8)
    {
        float3 origin = float3(object_to_world[0].w, object_to_world[1].w, object_to_world[2].w);
        float sway = asfloat(b.z);
        float3 p[3];
        for (uint k = 0; k < 3; k++)
        {
            uint v = index_of(a.y, primitive * 3 + k);
            float3 w = mul(object_to_world, float4(position_of(a.x, v), 1.0));
            p[k] = w + swayed(wind, sway * max(w.z - origin.z, 0.0), origin, w, true);
        }
        float2 drawn;
        if (!meets(o, d, tmin, tmax, p[0], p[1], p[2], t, drawn))
        {
            t = -1.0;
            return false;
        }
        if ((a.w & RT_ALPHA_TESTED) != 0 && !alpha_kept(a, b, draws, primitive, drawn))
        {
            t = -1.0;
            return false;
        }
        return true;
    }
    return (a.w & RT_ALPHA_TESTED) == 0 || alpha_kept(a, b, draws, primitive, bary);
}
