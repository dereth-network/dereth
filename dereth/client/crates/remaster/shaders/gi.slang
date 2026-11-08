// Bounced light: the light the world's surfaces send on to each other, added to each pixel's own
// light by the resolve (`compose`, `Masks::gi`), never taken from it.
//
// Two ways, as the device can:
// - **Probes, with ray queries.** A grid of probes round the eye outdoors, a probe every 4 m
//   across and 3 m up, kept where they stand in the world as the eye moves (each grid cell's probe
//   is held at its place modulo the grid's size, and a probe new to its place starts afresh); and
//   indoors a grid of its own for each room near the eye, a probe every 2 m, which only that
//   room's pixels read, so light does not leak between rooms. Each frame every probe casts its
//   rays (`gi_trace.slang`); where a ray meets a surface it takes the light that surface sends on
//   (its own color times the direct light on it, and a share of the bounced light already there),
//   and where it meets nothing it takes none (the sky's light is retail's ambient, already in the
//   pixel's own). Each probe keeps its light as spherical harmonics of the first two bands, and
//   the distances its rays ran in a small octahedral map, blended into what it kept.
// - **The screen, without them.** Rays from each pixel through the G-buffer's depth, taking the
//   direct light of the pixels they meet, blurred.
//
// Everything is in the units of retail's light, which the resolve multiplies a surface's color by.
module gi;

import bindless;
import retail;

// The pass's constants, in buffer `gi` as float4 rows (ints as their bits where said):
//   0  xyz the outdoor grid's first cell (world cells, ints), w its probes
//   1  xyz the grid's size in probes (ints), w the rays a probe casts
//   2  x the probes' spacing across, y up, z the hysteresis, w the strength
//   3  xy the frame's origin in world cells across (ints), z the frame's serial, w 1 when the
//      outdoor grid is drawn
//   4..6 the rays' turn this frame, three rows
//   7  x the share of bounced light a hit sends on, y the farthest a ray runs, z the rooms (int)
//   8 + 2k, 9 + 2k  room k: its first probe's place (frame space) and its spacing; its size in
//      probes (ints), and its first probe with, from bit 16, the serial its probes were laid
//      out for it with (int)
public static const uint GI_GRID = 0;
public static const uint GI_SIZE = 1;
public static const uint GI_SPACING = 2;
public static const uint GI_ORIGIN = 3;
public static const uint GI_TURN = 4;
public static const uint GI_BOUNCE = 7;
public static const uint GI_ROOMS = 8;
public static const uint GI_MAX_ROOMS = 16;

// A probe's rows in buffer `probes`: three of spherical harmonics (red, green, blue, each four
// coefficients), eight of distances (an octahedral map four texels a side: mean and mean square),
// and its state: the world cell it holds (ints) and the frames it has gathered.
public static const uint PROBE_ROWS = 12;
static const uint PROBE_MOMENTS = 3;
public static const uint PROBE_STATE = 11;
public static const uint MOMENTS_SIDE = 4;

static const float PI = 3.14159265;

// The octahedral place, -1..1 each way, of unit direction `d`.
public float2 oct_place(float3 d)
{
    float3 a = d / max(abs(d.x) + abs(d.y) + abs(d.z), 1e-6);
    if (a.z < 0.0)
    {
        float2 s = float2(a.x >= 0.0 ? 1.0 : -1.0, a.y >= 0.0 ? 1.0 : -1.0);
        return (1.0 - abs(a.yx)) * s;
    }
    return a.xy;
}

// The unit direction at octahedral place `e`, -1..1 each way.
public float3 oct_direction(float2 e)
{
    float3 v = float3(e, 1.0 - abs(e.x) - abs(e.y));
    if (v.z < 0.0)
    {
        float2 s = float2(v.x >= 0.0 ? 1.0 : -1.0, v.y >= 0.0 ? 1.0 : -1.0);
        v.xy = (1.0 - abs(v.yx)) * s;
    }
    return normalize(v);
}

// The distances' texel of direction `d`.
public uint moments_texel(float3 d)
{
    uint2 t = uint2(clamp((oct_place(d) * 0.5 + 0.5) * float(MOMENTS_SIDE), 0.0, float(MOMENTS_SIDE) - 0.5));
    return t.y * MOMENTS_SIDE + t.x;
}

// The first two bands of spherical harmonics of direction `d`.
public float4 sh_basis(float3 d)
{
    return float4(0.282095, 0.488603 * d.y, 0.488603 * d.z, 0.488603 * d.x);
}

int3 asint3(float3 v)
{
    return int3(asint(v.x), asint(v.y), asint(v.z));
}

// The outdoor grid: its first cell and its size.
public int3 grid_first(uint gi)
{
    return asint3(row(gi, GI_GRID).xyz);
}

public int3 grid_size(uint gi)
{
    return asint3(row(gi, GI_SIZE).xyz);
}

// The probe the outdoor grid holds world cell `c` at.
public uint grid_probe(uint gi, int3 c)
{
    int3 n = grid_size(gi);
    int3 m = ((c % n) + n) % n;
    return uint(m.x + n.x * (m.y + n.y * m.z));
}

// The world cell outdoor probe `i` stands for this frame: the one in the grid with its place.
public int3 grid_cell(uint gi, uint i)
{
    int3 n = grid_size(gi);
    int3 first = grid_first(gi);
    int3 m = int3(int(i) % n.x, (int(i) / n.x) % n.y, int(i) / (n.x * n.y));
    int3 fm = ((first % n) + n) % n;
    return first + ((m - fm + n) % n);
}

// Where world cell `c` stands, the frame's space.
public float3 cell_place(uint gi, int3 c)
{
    float4 s = row(gi, GI_SPACING);
    int2 o = int2(asint(row(gi, GI_ORIGIN).x), asint(row(gi, GI_ORIGIN).y));
    return float3(float(c.x - o.x) * s.x, float(c.y - o.y) * s.x, float(c.z) * s.y);
}

// What probe `p` holds, as light reaching a surface facing `n` (retail's units): its harmonics'
// irradiance there over pi, never below nought.
public float3 probe_light(uint probes, uint p, float3 n)
{
    float4 b = sh_basis(n);
    // The cosine lobe's bands: pi, then 2 pi / 3; over pi.
    float4 a = float4(1.0, 2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0) * b;
    float3 e = float3(dot(row(probes, p * PROBE_ROWS), a), dot(row(probes, p * PROBE_ROWS + 1), a),
                      dot(row(probes, p * PROBE_ROWS + 2), a));
    return max(e, 0.0);
}

// How much of probe `p`'s light reaches a point `dist` from it in direction `d` from it: its
// distances' Chebyshev bound, 1 where nothing stands between.
public float probe_sees(uint probes, uint p, float3 d, float dist)
{
    uint t = moments_texel(d);
    float4 r = row(probes, p * PROBE_ROWS + PROBE_MOMENTS + t / 2);
    float2 m = (t & 1) != 0 ? r.zw : r.xy;
    if (dist <= m.x)
    {
        return 1.0;
    }
    float var = max(m.y - m.x * m.x, 1e-4);
    float delta = dist - m.x;
    float c = var / (var + delta * delta);
    return max(c * c * c, 0.0);
}

// Whether probe `p` has gathered anything.
public bool probe_ready(uint probes, uint p)
{
    return row_u(probes, p * PROBE_ROWS + PROBE_STATE).w > 0;
}

// The bounced light reaching a surface at `x` (the frame's space) facing `n`, from the eight
// probes of a grid round it: `first` its first probe, `origin` where its first cell stands, `size`
// and `spacing` its size and spacing, `wrap` whether it holds world cells modulo its size (the
// outdoor grid) or is laid from `origin` (a room's). Each probe weighs by its place, by whether
// the surface faces it, and by whether it sees the point; nought where none is ready.
public float3 grid_light(uint gi, uint probes, float3 x, float3 n, float3 v, bool wrap, uint first,
                         float3 origin, int3 size, float2 spacing)
{
    float3 s = float3(spacing.x, spacing.x, spacing.y);
    // Off the surface a little, along its normal and toward the eye, so it does not see itself.
    float3 q = x + (n * 0.2 + v * 0.1) * min(s.x, s.z);
    float3 g = (q - origin) / s;
    int3 base = int3(floor(g));
    float3 f = g - float3(base);
    float3 sum = float3(0.0, 0.0, 0.0);
    float weights = 0.0;
    for (uint k = 0; k < 8; k++)
    {
        int3 o = int3(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        int3 c = base + o;
        uint p;
        if (wrap)
        {
            int3 first_cell = grid_first(gi);
            int3 rel = c + int3(asint(row(gi, GI_ORIGIN).x), asint(row(gi, GI_ORIGIN).y), 0);
            int3 inside = rel - first_cell;
            if (any(inside < 0) || any(inside >= size))
            {
                continue;
            }
            p = grid_probe(gi, rel);
            int4 held = asint(row(probes, p * PROBE_ROWS + PROBE_STATE));
            if (any(held.xyz != rel) || held.w == 0)
            {
                continue;
            }
        }
        else
        {
            if (any(c < 0) || any(c >= size))
            {
                continue;
            }
            p = first + uint(c.x + size.x * (c.y + size.y * c.z));
            if (!probe_ready(probes, p))
            {
                continue;
            }
        }
        float3 at = origin + float3(c) * s;
        float3 tri = lerp(1.0 - f, f, float3(o));
        float w = tri.x * tri.y * tri.z;
        float3 to = at - x;
        float dist = length(to);
        float3 dir = dist > 1e-4 ? to / dist : n;
        // A probe behind the surface counts little.
        float facing = (dot(dir, n) + 1.0) * 0.5;
        w *= facing * facing + 0.2;
        w *= probe_sees(probes, p, -dir, length(at - q));
        w = max(w, 0.0);
        sum += probe_light(probes, p, n) * w;
        weights += w;
    }
    return weights > 1e-5 ? sum / weights : float3(0.0, 0.0, 0.0);
}

// The outdoor grid's bounced light at `x` facing `n`, seen from direction `v` (toward the eye).
public float3 outdoor_light(uint gi, uint probes, float3 x, float3 n, float3 v)
{
    if (row(gi, GI_ORIGIN).w < 0.5)
    {
        return float3(0.0, 0.0, 0.0);
    }
    int3 size = grid_size(gi);
    float2 spacing = row(gi, GI_SPACING).xy;
    // The grid's probes stand at their cells' corners: cell (0,0,0) of the frame's space at its
    // origin.
    return grid_light(gi, probes, x, n, v, true, 0, float3(0.0, 0.0, 0.0), size, spacing);
}

// The room whose grid holds `x`, by its row; ~0 for none.
public uint room_at(uint gi, float3 x)
{
    uint rooms = asuint(row(gi, GI_BOUNCE).z);
    for (uint k = 0; k < min(rooms, GI_MAX_ROOMS); k++)
    {
        float4 a = row(gi, GI_ROOMS + 2 * k);
        int4 b = asint(row(gi, GI_ROOMS + 2 * k + 1));
        float3 hi = a.xyz + float3(b.xyz - 1) * a.w;
        if (all(x >= a.xyz - 0.5) && all(x <= hi + 0.5))
        {
            return k;
        }
    }
    return 0xFFFFFFFF;
}

// The bounced light of room grid `k` at `x` facing `n`.
public float3 room_light(uint gi, uint probes, uint k, float3 x, float3 n, float3 v)
{
    float4 a = row(gi, GI_ROOMS + 2 * k);
    int4 b = asint(row(gi, GI_ROOMS + 2 * k + 1));
    return grid_light(gi, probes, x, n, v, false, uint(b.w) & 0xFFFF, a.xyz, b.xyz, float2(a.w, a.w));
}

// The bounced light at `x` facing `n` for a pixel of class `cls`: the outdoors from the outdoor
// grid, a room's from its own.
public float3 bounced(uint gi, uint probes, uint cls, float3 x, float3 n, float3 v)
{
    if (cls == CLASS_GROUND || cls == CLASS_SUN)
    {
        return outdoor_light(gi, probes, x, n, v);
    }
    if (cls == CLASS_CELL || cls == CLASS_INDOOR)
    {
        uint k = room_at(gi, x);
        if (k != 0xFFFFFFFF)
        {
            return room_light(gi, probes, k, x, n, v);
        }
    }
    return float3(0.0, 0.0, 0.0);
}

// The world cell, and the place, of probe `i`: an outdoor one from the grid, a room's from its
// room. `room` is ~0 for an outdoor probe.
public void probe_place(uint gi, uint i, out int3 cell, out float3 place, out uint room)
{
    uint outdoor = asuint(row(gi, GI_GRID).w);
    if (i < outdoor)
    {
        cell = grid_cell(gi, i);
        place = cell_place(gi, cell);
        room = 0xFFFFFFFF;
        return;
    }
    uint rooms = asuint(row(gi, GI_BOUNCE).z);
    for (uint k = 0; k < min(rooms, GI_MAX_ROOMS); k++)
    {
        float4 a = row(gi, GI_ROOMS + 2 * k);
        int4 b = asint(row(gi, GI_ROOMS + 2 * k + 1));
        uint first = uint(b.w) & 0xFFFF;
        uint count = uint(b.x * b.y * b.z);
        if (i >= first && i < first + count)
        {
            uint j = i - first;
            int3 c = int3(int(j) % b.x, (int(j) / b.x) % b.y, int(j) / (b.x * b.y));
            // A room's probe holds its cell and the serial its room's probes were laid out with,
            // so a probe laid out again starts afresh.
            cell = c + int3((b.w >> 16) * 64, 0, 0);
            place = a.xyz + float3(c) * a.w;
            room = k;
            return;
        }
    }
    cell = int3(0, 0, 0);
    place = float3(0.0, 0.0, 0.0);
    room = 0xFFFFFFFF;
}

// The direction of ray `k` of `count`, a spherical Fibonacci spiral turned by the frame's turn.
public float3 ray_direction(uint gi, uint k, uint count)
{
    float n = float(count);
    float golden = 2.39996323;
    float z = 1.0 - (2.0 * float(k) + 1.0) / n;
    float r = sqrt(max(1.0 - z * z, 0.0));
    float phi = float(k) * golden;
    float3 d = float3(cos(phi) * r, sin(phi) * r, z);
    return normalize(float3(dot(row(gi, GI_TURN).xyz, d), dot(row(gi, GI_TURN + 1).xyz, d),
                            dot(row(gi, GI_TURN + 2).xyz, d)));
}

// --- the probes' update ---------------------------------------------------------------------

struct UpdatePush
{
    uint gi;
    uint probes;
    // Each ray's light (rgb) and distance (a), the probes' rays one after another.
    uint rays;
    uint count;
};

groupshared float4 shared_rays[64];

// Each probe's rays gathered into its harmonics and its distances, blended into what it kept
// (afresh for a probe new to its place); one group a probe, one thread a ray.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_update(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID, uniform UpdatePush push)
{
    uint p = group.x;
    if (p >= push.count)
    {
        return;
    }
    uint rays = min(asuint(row(push.gi, GI_SIZE).w), 64u);
    uint k = thread.x;
    float4 hit = k < rays ? row(push.rays, p * 64 + k) : float4(0.0, 0.0, 0.0, 0.0);
    shared_rays[k] = hit;
    GroupMemoryBarrierWithGroupSync();
    if (k != 0)
    {
        return;
    }
    int3 cell;
    float3 place;
    uint room;
    probe_place(push.gi, p, cell, place, room);
    int4 held = asint(row(push.probes, p * PROBE_ROWS + PROBE_STATE));
    bool fresh = any(held.xyz != cell) || held.w == 0;
    float hysteresis = fresh ? 0.0 : row(push.gi, GI_SPACING).z;
    // The harmonics: each ray's light times its direction's basis, over the rays, times the
    // sphere's whole.
    float4 sr = float4(0.0, 0.0, 0.0, 0.0);
    float4 sg = sr;
    float4 sb = sr;
    float2 moments[16];
    float wsum[16];
    for (uint t = 0; t < 16; t++)
    {
        moments[t] = float2(0.0, 0.0);
        wsum[t] = 0.0;
    }
    float far = row(push.gi, GI_BOUNCE).y;
    for (uint r = 0; r < rays; r++)
    {
        float3 d = ray_direction(push.gi, r, rays);
        float4 b = sh_basis(d);
        float4 h = shared_rays[r];
        sr += h.r * b;
        sg += h.g * b;
        sb += h.b * b;
        float dist = min(h.a, far);
        // Each texel takes the rays near its own direction, the nearer the more.
        for (uint t = 0; t < 16; t++)
        {
            float2 e = (float2(t % MOMENTS_SIDE, t / MOMENTS_SIDE) + 0.5) / float(MOMENTS_SIDE) * 2.0 - 1.0;
            float w = pow(max(dot(oct_direction(e), d), 0.0), 8.0);
            moments[t] += float2(dist, dist * dist) * w;
            wsum[t] += w;
        }
    }
    float scale = 4.0 * PI / float(max(rays, 1u));
    RWByteAddressBuffer o = rw_buffers[NonUniformResourceIndex(push.probes)];
    uint base = p * PROBE_ROWS * 16;
    float4 old[3] = { row(push.probes, p * PROBE_ROWS), row(push.probes, p * PROBE_ROWS + 1),
                      row(push.probes, p * PROBE_ROWS + 2) };
    float4 now[3] = { sr * scale, sg * scale, sb * scale };
    for (uint c = 0; c < 3; c++)
    {
        o.Store4(base + c * 16, asuint(lerp(now[c], old[c], hysteresis)));
    }
    for (uint t = 0; t < 16; t += 2)
    {
        float4 kept = row(push.probes, p * PROBE_ROWS + PROBE_MOMENTS + t / 2);
        float2 a = wsum[t] > 1e-5 ? moments[t] / wsum[t] : float2(far, far * far);
        float2 b = wsum[t + 1] > 1e-5 ? moments[t + 1] / wsum[t + 1] : float2(far, far * far);
        o.Store4(base + (PROBE_MOMENTS + t / 2) * 16, asuint(lerp(float4(a, b), kept, hysteresis)));
    }
    uint frames = fresh ? 1u : uint(held.w) + 1u;
    o.Store4(base + PROBE_STATE * 16, uint4(asuint(cell.x), asuint(cell.y), asuint(cell.z), frames));
}

// --- the pixels' bounced light --------------------------------------------------------------

struct ApplyPush
{
    uint frame;
    uint gi;
    uint probes;
    uint normal;
    uint material;
    uint depth;
    uint outdoor_depth;
};

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

// The pixel's place in the frame's space, from depth `d`.
float3 place_of(uint frame, float2 position, float d)
{
    float2 extent = row(frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    float4 h = transform(frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return h.xyz / h.w;
}

// Each pixel's bounced light from the probes, in the G-buffer's own pixels.
[shader("fragment")]
float4 fs_probes(float4 position : SV_Position, uniform ApplyPush push) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        return float4(0.0, 0.0, 0.0, 1.0);
    }
    bool outdoors = cls == CLASS_GROUND || cls == CLASS_SUN;
    float d = at(outdoors ? push.outdoor_depth : push.depth, p).x;
    float3 x = place_of(push.frame, position.xy, d);
    float3 n = oct_decode(at(push.normal, p).xy);
    float3 v = normalize(row(push.frame, FRAME_EYE).xyz - x);
    float strength = row(push.gi, GI_SPACING).w;
    return float4(bounced(push.gi, push.probes, cls, x, n, v) * strength, 1.0);
}

struct ScreenPush
{
    uint frame;
    uint gi;
    uint albedo;
    uint normal;
    uint light;
    uint material;
    uint depth;
    uint outdoor_depth;
    uint sun_visibility;
    uint rays;
    float reach;
};

// The direct light pixel `q` of the G-buffer holds: the sun's on the ground and on what it lights
// (by the shadows, where they are drawn), a room's burned light, an indoor object's own.
float3 direct_at(ScreenPush push, int2 q)
{
    uint cls = uint(at(push.material, q).x * 255.0 + 0.5);
    float3 n = oct_decode(at(push.normal, q).xy);
    if (cls == CLASS_GROUND || cls == CLASS_SUN)
    {
        float3 sun = row(push.frame, FRAME_SUN).xyz;
        float vis = push.sun_visibility != 0xFFFFFFFF ? at(push.sun_visibility, q).x : 1.0;
        return row(push.frame, FRAME_SUN_COLOR).rgb * max(dot(n, sun), 0.0) * vis;
    }
    if (cls == CLASS_CELL || cls == CLASS_INDOOR)
    {
        return at(push.light, q).rgb;
    }
    return float3(0.0, 0.0, 0.0);
}

// A hash of a pixel, 0 to 1, the same every frame.
float hash2(int2 p, uint k)
{
    uint h = uint(p.x) * 0x8DA6B343u ^ uint(p.y) * 0xD8163841u ^ k * 0xCB1AB31Fu;
    h ^= h >> 13;
    h *= 0x85EBCA6Bu;
    h ^= h >> 16;
    return float(h & 0xFFFFFFu) / 16777216.0;
}

// Each pixel's bounced light from the screen: rays round its normal through the depth, each
// taking the direct light of the pixel it meets times that pixel's own color.
[shader("fragment")]
float4 fs_screen(float4 position : SV_Position, uniform ScreenPush push) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        return float4(0.0, 0.0, 0.0, 1.0);
    }
    bool outdoors = cls == CLASS_GROUND || cls == CLASS_SUN;
    float d = at(outdoors ? push.outdoor_depth : push.depth, p).x;
    float3 x = place_of(push.frame, position.xy, d);
    float3 n = oct_decode(at(push.normal, p).xy);
    float3 t = normalize(abs(n.z) < 0.9 ? cross(n, float3(0.0, 0.0, 1.0)) : cross(n, float3(1.0, 0.0, 0.0)));
    float3 b = cross(n, t);
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 sum = float3(0.0, 0.0, 0.0);
    uint count = max(push.rays, 1u);
    float turn = hash2(p, 0) * 6.2831853;
    for (uint k = 0; k < count; k++)
    {
        // Cosine-weighted directions round the normal.
        float u1 = (float(k) + hash2(p, k + 1)) / float(count);
        float u2 = frac(float(k) * 0.618034 + turn / 6.2831853);
        float r = sqrt(u1);
        float phi = u2 * 6.2831853;
        float3 dir = normalize(t * (r * cos(phi)) + b * (r * sin(phi)) + n * sqrt(max(1.0 - u1, 0.0)));
        // March it through the depth, its steps growing.
        float s = push.reach / 16.0;
        float travelled = 0.0;
        for (uint i = 0; i < 12; i++)
        {
            travelled += s;
            s *= 1.15;
            float3 qw = x + n * 0.05 + dir * travelled;
            float4 c = transform(push.frame, FRAME_VIEW_PROJ, float4(qw, 1.0));
            if (c.w <= 0.0)
            {
                break;
            }
            float2 ndc = c.xy / c.w;
            int2 q = int2(float2((ndc.x * 0.5 + 0.5) * extent.x, (0.5 - ndc.y * 0.5) * extent.y));
            if (any(q < 0) || any(q >= int2(extent)))
            {
                break;
            }
            uint qc = uint(at(push.material, q).x * 255.0 + 0.5);
            if (qc == CLASS_NONE || qc == CLASS_SKY)
            {
                continue;
            }
            bool qo = qc == CLASS_GROUND || qc == CLASS_SUN;
            float qd = at(qo ? push.outdoor_depth : push.depth, q).x;
            float3 sw = place_of(push.frame, float2(q) + 0.5, qd);
            float ray = length(qw - eye);
            float surface = length(sw - eye);
            if (ray > surface && ray - surface < 0.5 + surface * 0.02)
            {
                // A surface facing back along the ray sends its light on.
                float3 qn = oct_decode(at(push.normal, q).xy);
                float facing = saturate(-dot(qn, dir));
                sum += at(push.albedo, q).rgb * direct_at(push, q) * facing;
                break;
            }
        }
    }
    float strength = row(push.gi, GI_SPACING).w;
    return float4(max(sum / float(count), 0.0) * strength, 1.0);
}

struct BlurPush
{
    uint frame;
    uint source;
    uint normal;
    uint depth;
    uint material;
    // 1 across, 0 up.
    uint across;
};

// The bounced light blurred along one axis, by pixels of the same surface: their normals and
// depths alike.
[shader("fragment")]
float4 fs_blur(float4 position : SV_Position, uniform BlurPush push) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    float4 own = at(push.source, p);
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        return own;
    }
    float3 n = oct_decode(at(push.normal, p).xy);
    float d = at(push.depth, p).x;
    int2 axis = push.across != 0 ? int2(1, 0) : int2(0, 1);
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float3 sum = float3(0.0, 0.0, 0.0);
    float weights = 0.0;
    for (int k = -6; k <= 6; k++)
    {
        int2 q = clamp(p + axis * k, int2(0, 0), int2(extent) - 1);
        float3 qn = oct_decode(at(push.normal, q).xy);
        float qd = at(push.depth, q).x;
        float w = exp(-float(k * k) / 18.0) * pow(saturate(dot(n, qn)), 8.0)
                  * (abs(qd - d) < 0.002 * (1.0 - d) * 400.0 + 1e-4 ? 1.0 : 0.0);
        sum += at(push.source, q).rgb * w;
        weights += w;
    }
    return float4(weights > 1e-5 ? sum / weights : own.rgb, 1.0);
}
