// The scene's acceleration structures, as shaders read them, where the device traces rays.
//
// The top-level structure holds every resident static at its finest level, every room's walls,
// every block's ground and every object, in the G-buffer's own space (block offsets and the drawn
// ground's lifts applied). Each instance's mask says what it is:
//   1 a static with no alpha-tested surface, 2 a static with one, 4 the ground, 8 an object,
//   16 the viewer's own body.
// Its index of the user's names its first row in the geometry tables: one row per geometry of
// its bottom-level structure, in order, for that instance, `uint4(first index, vertex base,
// material row, flags)` with the flags 1 alpha-tested, 2 two-sided and 8 swayed by the wind; a
// block's ground has one
// row, `uint4(0, vertex base, first cell, 4 | cells a side << 8)`, its triangles two a cell in the
// cells' order. Statics' and the ground's rows are in one table, the objects' in another, which
// bit 23 of the index picks.
//
// Every geometry is opaque but the alpha-tested and the swaying ones, whose candidate hits a query
// is handed to test; no instance culls either face.
module rt;

import bindless;
import lift;
import retail;

// The masks.
public static const uint RT_STATIC = 1;
public static const uint RT_STATIC_ALPHA = 2;
public static const uint RT_GROUND = 4;
public static const uint RT_OBJECT = 8;
public static const uint RT_BODY = 16;
// An art pack's ground cover.
public static const uint RT_COVER = 32;

// A geometry row's flags.
public static const uint RT_ALPHA_TESTED = 1;
public static const uint RT_TWO_SIDED = 2;
public static const uint RT_IS_GROUND = 4;
public static const uint RT_SWAYS = 8;

// The bit of an instance's index that names the objects' table.
static const uint OBJECT_ROWS = 0x800000;

// Geometry `geometry`'s row of the instance whose index of the user's is `custom`: from `statics`
// or `objects`, the two tables.
public uint4 geometry_row(uint statics, uint objects, uint custom, uint geometry)
{
    if ((custom & OBJECT_ROWS) != 0)
    {
        return row_u(objects, (custom & (OBJECT_ROWS - 1)) + geometry);
    }
    return row_u(statics, custom + geometry);
}

// Whether a candidate hit on an alpha-tested geometry is kept: the texel its triangle shows
// there against its material's alpha reference, as the G-buffer tests it. `custom` and `geometry`
// name the geometry's row, `primitive` its triangle and `bary` the hit's place in it; `indices`,
// `vertices` and `materials` are the scene's pools, and `sampler` the one the texture is read
// with. A geometry that is not alpha-tested keeps every hit.
public bool alpha_kept(uint statics, uint objects, uint indices, uint vertices, uint materials,
                       uint sampler, uint custom, uint geometry, uint primitive, float2 bary)
{
    uint4 g = geometry_row(statics, objects, custom, geometry);
    if ((g.w & RT_ALPHA_TESTED) == 0)
    {
        return true;
    }
    ByteAddressBuffer ib = buffers[NonUniformResourceIndex(indices)];
    uint first = g.x + primitive * 3;
    float2 uv = float2(0.0, 0.0);
    float w[3] = { 1.0 - bary.x - bary.y, bary.x, bary.y };
    for (uint k = 0; k < 3; k++)
    {
        uint v = g.y + ib.Load((first + k) * 4);
        uv += row(vertices, v * 4 + 1).zw * w[k];
    }
    uint4 m = row_u(materials, g.z * 2);
    float a = 1.0;
    if ((m.y & 64) != 0)
    {
        a = textures[NonUniformResourceIndex(m.x)]
                .SampleLevel(samplers[NonUniformResourceIndex(sampler)], uv, 0.0)
                .a;
    }
    return a - float(m.z) / 255.0 + 1e-6 >= 0.0;
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

// Whether a query keeps a candidate hit, and where. The structures hold a swaying model as it
// stands still, so a candidate on one is tested again against its triangle as the G-buffer drew
// it this frame, swayed by the frame's wind (`frame`'s constants): a ray leaving a swayed surface
// then never meets that surface's still self, and a hit is where the drawn model is. Such a hit's
// distance is `t`; any other leaves `t` negative, the candidate's own. An alpha-tested surface is
// then tested as `alpha_kept` tests it. The ray runs from `o` along `d`, kept between `tmin` and
// `tmax`; `object_to_world` is the candidate's instance's transform, `bary` its place in the
// triangle.
public bool hit_kept(uint frame, uint statics, uint objects, uint indices, uint vertices,
                     uint materials, uint sampler, uint custom, uint geometry, uint primitive,
                     float2 bary, float3 o, float3 d, float tmin, float tmax,
                     float3x4 object_to_world, out float t)
{
    t = -1.0;
    uint4 g = geometry_row(statics, objects, custom, geometry);
    float4 wind = row(frame, FRAME_WIND);
    if ((g.w & RT_SWAYS) != 0 && dot(wind.xy, wind.xy) >= 1e-8)
    {
        ByteAddressBuffer ib = buffers[NonUniformResourceIndex(indices)];
        uint first = g.x + primitive * 3;
        bool leaf = (row_u(materials, g.z * 2).y & PACK_FOLIAGE) != 0;
        float scale = length(float3(object_to_world[0].x, object_to_world[1].x, object_to_world[2].x));
        float3 origin = float3(object_to_world[0].w, object_to_world[1].w, object_to_world[2].w);
        float3 p[3];
        for (uint k = 0; k < 3; k++)
        {
            uint v = g.y + ib.Load((first + k) * 4);
            float3 w = mul(object_to_world, float4(row(vertices, v * 4).xyz, 1.0));
            // How far the wind bends the vertex, whole millimeters, above twice its tangent's
            // last value's own part, as the G-buffer takes it out.
            float bent = floor((abs(row(vertices, v * 4 + 2).w) - 1.0) * 0.5 + 1e-3);
            if (bent > 0.5)
            {
                w += swayed(wind, bent * 0.001 * scale, origin, w, leaf);
            }
            p[k] = w;
        }
        float2 drawn;
        if (!meets(o, d, tmin, tmax, p[0], p[1], p[2], t, drawn))
        {
            t = -1.0;
            return false;
        }
        bary = drawn;
        if (!alpha_kept(statics, objects, indices, vertices, materials, sampler, custom, geometry,
                        primitive, bary))
        {
            t = -1.0;
            return false;
        }
        return true;
    }
    return alpha_kept(statics, objects, indices, vertices, materials, sampler, custom, geometry,
                      primitive, bary);
}

// What the instances' pass reads and writes.
struct InstancesPush
{
    uint frame;
    uint instances;
    uint blocks;
    // The entries: two rows each, `uint4(instance or block slot, index of the user's, mask |
    // kind << 8, 0)` and `uint4(the bottom-level structure's address, low and high, 0, 0)`, the
    // kind 0 a placed instance, 1 a block's ground.
    uint entries;
    uint count;
    // Where the records go, read and written, and the first record this pass writes.
    uint out;
    uint first;
};

// Each entry's record as a top-level build reads it: its transform, three rows of four, its
// index and mask, its flags (neither face culled), and its bottom-level structure's address.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_instances(uint3 id : SV_DispatchThreadID, uniform InstancesPush push)
{
    if (id.x >= push.count)
    {
        return;
    }
    uint4 e = row_u(push.entries, id.x * 2);
    uint4 a = row_u(push.entries, id.x * 2 + 1);
    float4 c0 = float4(1.0, 0.0, 0.0, 0.0);
    float4 c1 = float4(0.0, 1.0, 0.0, 0.0);
    float4 c2 = float4(0.0, 0.0, 1.0, 0.0);
    float3 t;
    if ((e.z >> 8) == 1)
    {
        // A block's ground: its vertices block-local, moved by the block's offset.
        t = row(push.blocks, e.x).xyz;
    }
    else
    {
        uint at = e.x * INSTANCE_ROWS;
        c0 = row(push.instances, at);
        c1 = row(push.instances, at + 1);
        c2 = row(push.instances, at + 2);
        t = row(push.instances, at + 3).xyz;
        uint slot = row_u(push.instances, at + 5).x;
        if (slot != 0xFFFFFFFF)
        {
            t += row(push.blocks, slot).xyz;
        }
        // Standing on the ground as it is drawn.
        t.z += lifted(push.frame, push.instances, e.x, instance_origin(push.instances, push.blocks, e.x));
    }
    uint base = (push.first + id.x) * 64;
    RWByteAddressBuffer o = rw_buffers[NonUniformResourceIndex(push.out)];
    o.Store4(base, asuint(float4(c0.x, c1.x, c2.x, t.x)));
    o.Store4(base + 16, asuint(float4(c0.y, c1.y, c2.y, t.y)));
    o.Store4(base + 32, asuint(float4(c0.z, c1.z, c2.z, t.z)));
    // Neither face culled: the instance flag 1, in the high byte.
    o.Store4(base + 48, uint4((e.y & 0xFFFFFF) | ((e.z & 0xFF) << 24), 1u << 24, a.x, a.y));
}
