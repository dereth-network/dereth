// The ground's pictures: a ground cell's splat, retail's composite kept as its layers and
// blended at each pixel as retail blends them.
module ground;

import bindless;
import retail;

// The most overlays a splat blends.
public static const uint SPLAT_OVERLAYS = 5;
// The material kind of water.
public static const uint KIND_WATER = 7;
// The color a missing picture lays, as retail's: green.
public static const float4 SPLAT_MISSING = float4(0.0, 1.0, 0.0, 0.0);

// Where the composite's alpha map is read for a cell coordinate, at each quarter turn, and how a
// step across the screen turns with it.
public float2 turned(float2 uv, uint rot)
{
    switch (rot)
    {
    case 1: return float2(1.0 - uv.y, uv.x);
    case 2: return float2(1.0 - uv.x, 1.0 - uv.y);
    case 3: return float2(uv.y, 1.0 - uv.x);
    default: return uv;
    }
}

public float2 turned_step(float2 d, uint rot)
{
    switch (rot)
    {
    case 1: return float2(-d.y, d.x);
    case 2: return -d;
    case 3: return float2(d.y, -d.x);
    default: return d;
    }
}

public float4 sampled(uint texture, uint sampler, float2 uv, float2 dx, float2 dy)
{
    return textures[NonUniformResourceIndex(texture)].SampleGrad(
        samplers[NonUniformResourceIndex(sampler)], uv, dx, dy);
}

// The rows a splat takes: its base, an overlay each, then the maps of its base and of each
// overlay, then what each of those layers lays on steep ground, scree and then rock.
public static const uint SPLAT_LAYERS = 1 + SPLAT_OVERLAYS;
public static const uint SPLAT_STEEP = 2 * SPLAT_LAYERS;
// Then what a pack lays by the water: the grass grown over a muddy bank, the land laid in place of
// retail's water ground, and the bed well inside the water, each as a steep row is.
public static const uint SPLAT_BANK = 4 * SPLAT_LAYERS;
public static const uint SPLAT_ROWS = SPLAT_BANK + 3;
// The base row's flags: where it holds what a pack lays by the water.
public static const uint SPLAT_HAS_BANK = 64;

// The four candidate texture coordinates of a cell, and the one each corner takes at each quarter
// turn, as the retail grid lays them.
public static const float2 UVS[4] = { float2(0.0, 1.0), float2(1.0, 1.0), float2(1.0, 0.0), float2(0.0, 0.0) };
public static const uint SW[4] = { 0, 3, 2, 1 };
public static const uint SE[4] = { 1, 0, 3, 2 };
public static const uint NE[4] = { 2, 1, 0, 3 };
public static const uint NW[4] = { 3, 2, 1, 0 };

// What a splat lays at a pixel.
public struct SplatSample
{
    public float3 color;
    // The water's share of the blend.
    public float water;
    // How readily the blend gathers puddles, as its layers' terrain does: 0 never (rock, sand,
    // snow, water), 1 the most (a road).
    public float puddles;
    // The blended normal's lean, in the ground's own frame (x east, y north, z up), and the
    // roughness: (0, 0, 1) and 1 where no layer brings maps.
    public float3 bend;
    public float roughness;
    // Where a pack's materials are laid: how high the blend's surface stands, its share that is
    // mud or marsh, and its share that is the pack's.
    public float height;
    public float mud;
    public float pack;
};

// Where a pixel lies for what varies a pack's materials over the ground. The retail grid's pixels
// have none of it.
public struct PackPlace
{
    // Whether the pixel varies them at all, and whether retail's water ground is laid as land.
    public bool varies;
    public bool land;
    // World cells within the materials' whole period, north down, and their steps across the
    // screen.
    public float2 q;
    public float2 qdx;
    public float2 qdy;
    // Meters from the eye.
    public float dist;
};

public PackPlace no_pack_place()
{
    PackPlace o;
    o.varies = false;
    o.land = false;
    o.q = float2(0.0, 0.0);
    o.qdx = float2(0.0, 0.0);
    o.qdy = float2(0.0, 0.0);
    o.dist = 0.0;
    return o;
}

// The cells a pack's coarse repeat comes round in, and how much coarser than the material's own
// it is; how much finer its near grain is; and where each comes in, meters from the eye.
public static const float PACK_PERIOD_CELLS = 64.0;
static const float COARSE_SPREAD = 3.3;
static const float GRAIN_REPEATS = 4.0;
static const float GRAIN_NEAR = 3.0;
static const float GRAIN_FAR = 22.0;
static const float COARSE_NEAR = 14.0;
static const float COARSE_FAR = 110.0;
// How close to the eye a material's height moves its edge, and where that has gone.
static const float HEIGHTS_NEAR = 12.0;
static const float HEIGHTS_FAR = 90.0;
// How much of a retail picture's broad patches is kept, a share of its mean either way, in
// display values; and the finest of it, a share of a repeat.
static const float BROAD_LEAST = 0.73;
static const float BROAD_MOST = 1.31;
static const float BROAD_FINEST = 1.0 / 64.0;

float brightness(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// A picture's mean: its last level.
float3 mean_of(uint texture, uint sampler, float2 uv)
{
    return textures[NonUniformResourceIndex(texture)].SampleLevel(
        samplers[NonUniformResourceIndex(sampler)], uv, 16.0).rgb;
}

// How much the retail picture `word` names (its index and one, its repeats across a cell from bit
// 24) varies about its mean at cell coordinate `uv`: each channel's share of its mean, its finest
// patches left out.
float3 broad_share(uint word, uint wrap, float2 uv, float2 dx, float2 dy)
{
    uint texture = (word & 0xFFFFFF) - 1;
    float k = float(word >> 24);
    float2 gx = dx * k;
    float2 gy = dy * k;
    float2 at = uv * k;
    float longest = max(max(length(gx), length(gy)), 1e-6);
    float wider = max(BROAD_FINEST / longest, 1.0);
    float3 here = sampled(texture, wrap, at, gx * wider, gy * wider).rgb;
    float3 mean = mean_of(texture, wrap, at);
    return clamp(here / max(mean, float3(0.01, 0.01, 0.01)), float3(BROAD_LEAST, BROAD_LEAST, BROAD_LEAST), float3(BROAD_MOST, BROAD_MOST, BROAD_MOST));
}

// How much of a layer of height `over` shows where its mask lays `t` of it over height `under`,
// `dist` meters from the eye: close by the mask's share moved toward the higher surface, most
// halfway, and sharpened, so the higher material (grass, a road's stones) stands up through the
// lower along their edge. Far off the mask's soft edge stands.
public float height_share(float under, float over, float t, float dist)
{
    if (t <= 0.0 || t >= 1.0)
    {
        return saturate(t);
    }
    float near = 1.0 - smoothstep(HEIGHTS_NEAR, HEIGHTS_FAR, dist);
    float moved = saturate(t + (over - under) * near * 4.0 * t * (1.0 - t));
    return lerp(moved, smoothstep(0.3, 0.7, moved), near);
}

// A pack's material laid at cell coordinate `uv`, `t` repeats to a cell (`m` its maps' row, as
// `layer_maps` takes it, with the retail picture it replaces in its last word): laid out in that
// picture's broad patches; far off a coarser repeat of it mixed in, so the repeats do not show;
// close by a finer repeat lending it its grain.
void pack_layer(uint tile, float t, uint4 m, uint wrap, uint rot, float2 uv, float2 dx, float2 dy, PackPlace place, out float4 c, out float3 bend, out float roughness)
{
    c = sampled(tile, wrap, uv * t, dx * t, dy * t);
    layer_maps(m, wrap, rot, uv * t, dx * t, dy * t, bend, roughness);
    if (!place.varies)
    {
        return;
    }
    if (m.w != 0)
    {
        c.rgb *= broad_share(m.w, wrap, uv, dx, dy);
    }
    float far = smoothstep(COARSE_NEAR, COARSE_FAR, place.dist) * 0.5;
    if (far > 0.001)
    {
        float coarse = max(round(t * PACK_PERIOD_CELLS / COARSE_SPREAD), 1.0) / PACK_PERIOD_CELLS;
        float4 k = sampled(tile, wrap, place.q * coarse + float2(0.37, 0.61), place.qdx * coarse, place.qdy * coarse);
        c.rgb = lerp(c.rgb, k.rgb, far);
        c.a = lerp(c.a, k.a, far);
    }
    float near = 1.0 - smoothstep(GRAIN_NEAR, GRAIN_FAR, place.dist);
    if (near > 0.001)
    {
        float fine = t * GRAIN_REPEATS;
        float2 at = uv * fine + float2(0.23, 0.79);
        float4 k = sampled(tile, wrap, at, dx * fine, dy * fine);
        float grain = clamp(brightness(k.rgb) / max(brightness(mean_of(tile, wrap, uv)), 0.01), 0.79, 1.2);
        c.rgb *= lerp(1.0, grain, near * 0.45);
        c.a = saturate(c.a + (k.a - 0.5) * near * 0.3);
        if (m.x != 0xFFFFFFFF)
        {
            float3 lean;
            float unused;
            layer_maps(m, wrap, rot, at, dx * fine, dy * fine, lean, unused);
            bend = normalize(bend + float3(lean.xy * near * 0.5, 0.0));
        }
    }
}

// One layer's maps at `uv`, its row `m` (the normal picture, the surface picture and the normal's
// strength): its normal turned from the picture's own frame (east along its first coordinate,
// north up its rows) into the ground's for a cell turned `rot` quarter turns; and its roughness.
void layer_maps(uint4 m, uint wrap, uint rot, float2 uv, float2 dx, float2 dy, out float3 bend, out float roughness)
{
    bend = float3(0.0, 0.0, 1.0);
    roughness = 1.0;
    if (m.x != 0xFFFFFFFF)
    {
        float2 t = (sampled(m.x, wrap, uv, dx, dy).rg * 2.0 - 1.0) * asfloat(m.z);
        float2 e = UVS[SE[rot]] - UVS[SW[rot]];
        float2 n = UVS[NW[rot]] - UVS[SW[rot]];
        float2 w = float2(e.x * t.x - e.y * t.y, n.x * t.x - n.y * t.y);
        bend = float3(w, sqrt(saturate(1.0 - dot(w, w))));
    }
    if (m.y != 0xFFFFFFFF)
    {
        // The surface picture in glTF's layout: occlusion, roughness, metalness.
        roughness = sampled(m.y, wrap, uv, dx, dy).g;
    }
}

// Splat `at`'s color at cell coordinate `uv` of a cell turned `rot` quarter turns, the
// composite's own: the base tile, then each overlay's tile laid where its alpha map lets it,
// `mix(tile, beneath, alpha)` as retail blends; with the water's share, how readily it gathers
// puddles, and the layers' normal and roughness maps where they bring them, laid the same way.
public SplatSample splat(uint splats, uint wrap, uint held, uint at, uint rot, float2 uv, float2 dx, float2 dy)
{
    return splat_at(splats, wrap, held, at, rot, uv, dx, dy, no_pack_place());
}

// The same, a pack's materials varied over the ground as `place` says and laid by their heights.
public SplatSample splat_at(uint splats, uint wrap, uint held, uint at, uint rot, float2 uv, float2 dx, float2 dy, PackPlace place)
{
    uint4 b = row_u(splats, at);
    bool mapped = (b.w & 2) != 0;
    uint4 land = row_u(splats, at + SPLAT_BANK + 1);
    bool as_land = place.land && land.x != 0xFFFFFFFF;
    SplatSample o;
    float4 c = SPLAT_MISSING;
    o.water = (b.w & 1) != 0 ? 1.0 : 0.0;
    o.puddles = float((b.w >> 2) & 3) / 3.0;
    o.bend = float3(0.0, 0.0, 1.0);
    o.roughness = 1.0;
    o.mud = float((b.w >> 5) & 1);
    o.pack = 0.0;
    uint tile = b.x;
    float t = float(max(b.y, 1u));
    uint4 maps = row_u(splats, at + 1 + SPLAT_OVERLAYS);
    if (as_land && (b.w & 1) != 0)
    {
        tile = land.x;
        t = float(land.y & 0xFF);
        maps = steep_maps(land, 0);
        o.mud = float((land.y >> 29) & 1);
    }
    if (tile != 0xFFFFFFFF)
    {
        if (mapped)
        {
            pack_layer(tile, t, maps, wrap, rot, uv, dx, dy, place, c, o.bend, o.roughness);
            o.pack = place.varies && maps.x != 0xFFFFFFFF ? 1.0 : 0.0;
        }
        else
        {
            c = sampled(tile, wrap, uv * t, dx * t, dy * t);
        }
    }
    for (uint k = 0; k < min(b.z, SPLAT_OVERLAYS); k++)
    {
        uint4 l = row_u(splats, at + 1 + k);
        if (l.x == 0xFFFFFFFF)
        {
            continue;
        }
        uint turn = l.y & 3;
        float a = sampled(l.x, held, turned(uv, turn), turned_step(dx, turn), turned_step(dy, turn)).a;
        float4 s = SPLAT_MISSING;
        float3 bend = float3(0.0, 0.0, 1.0);
        float roughness = 1.0;
        float mud = float((l.y >> 5) & 1);
        float pack = 0.0;
        tile = l.z;
        t = float(max(l.w, 1u));
        maps = row_u(splats, at + 2 + SPLAT_OVERLAYS + k);
        if (as_land && ((l.y >> 2) & 1) != 0)
        {
            tile = land.x;
            t = float(land.y & 0xFF);
            maps = steep_maps(land, 0);
            mud = float((land.y >> 29) & 1);
        }
        if (tile != 0xFFFFFFFF)
        {
            if (mapped)
            {
                pack_layer(tile, t, maps, wrap, rot, uv, dx, dy, place, s, bend, roughness);
                pack = place.varies && maps.x != 0xFFFFFFFF ? 1.0 : 0.0;
            }
            else
            {
                s = sampled(tile, wrap, uv * t, dx * t, dy * t);
            }
        }
        if (pack > 0.0 && o.pack > 0.0)
        {
            a = 1.0 - height_share(c.a, s.a, 1.0 - a, place.dist);
        }
        c.rgb = lerp(s.rgb, c.rgb, a);
        c.a = lerp(s.a, c.a, a);
        o.water = lerp(((l.y >> 2) & 1) != 0 ? 1.0 : 0.0, o.water, a);
        o.puddles = lerp(float((l.y >> 3) & 3) / 3.0, o.puddles, a);
        o.bend = lerp(bend, o.bend, a);
        o.roughness = lerp(roughness, o.roughness, a);
        o.mud = lerp(mud, o.mud, a);
        o.pack = lerp(pack, o.pack, a);
    }
    o.color = c.rgb;
    o.height = c.a;
    o.bend = normalize(o.bend);
    return o;
}

// A steep row's maps, as `layer_maps` takes them, with the retail picture `broad` names.
uint4 steep_maps(uint4 r, uint broad)
{
    return uint4(r.z, r.w, asuint(float((r.y >> 8) & 0xFFF) / 1000.0), broad);
}

// What a splat lays on steep ground at a pixel.
public struct SteepSample
{
    public float3 color;
    // How much of the pixel takes it: none where a layer is not the pack's, little where it is
    // snow; and how much of it is a ground's own material for its slopes.
    public float cover;
    public float own;
    public float3 bend;
    public float roughness;
    public float height;
};

// One layer's steep material, its row `r`: its color at `uv` and its maps, and how much of it the
// layer takes, snow taking `on_snow` of it; varied as `place` says, in the retail picture `broad`.
void steep_layer(uint4 r, uint wrap, uint rot, float2 uv, float2 dx, float2 dy, float on_snow, uint broad, PackPlace place, inout float3 color, out float cover, out float own, out float3 bend, out float roughness, out float height)
{
    cover = 0.0;
    own = 0.0;
    bend = float3(0.0, 0.0, 1.0);
    roughness = 1.0;
    height = 0.5;
    if (r.x == 0xFFFFFFFF)
    {
        return;
    }
    float4 c;
    pack_layer(r.x, float(r.y & 0xFF), steep_maps(r, broad), wrap, rot, uv, dx, dy, place, c, bend, roughness);
    color = c.rgb;
    height = c.a;
    cover = (r.y >> 31) != 0 ? on_snow : 1.0;
    own = float((r.y >> 30) & 1);
}

// What splat `at` lays on steep ground at cell coordinate `uv`, `mode` 0 its scree and 1 its rock:
// each layer's own, laid where retail's alpha maps lay that layer.
public SteepSample steep_splat(uint splats, uint wrap, uint held, uint at, uint rot, float2 uv, float2 dx, float2 dy, uint mode, float on_snow)
{
    return steep_at(splats, wrap, held, at, rot, uv, dx, dy, mode, on_snow, no_pack_place());
}

// The same, varied as `place` says.
public SteepSample steep_at(uint splats, uint wrap, uint held, uint at, uint rot, float2 uv, float2 dx, float2 dy, uint mode, float on_snow, PackPlace place)
{
    uint first = at + SPLAT_STEEP + mode * SPLAT_LAYERS;
    uint overlays = min(row_u(splats, at).z, SPLAT_OVERLAYS);
    uint maps = at + 1 + SPLAT_OVERLAYS;
    SteepSample o;
    o.color = float3(0.0, 0.0, 0.0);
    steep_layer(row_u(splats, first), wrap, rot, uv, dx, dy, on_snow, row_u(splats, maps).w, place, o.color, o.cover, o.own, o.bend, o.roughness, o.height);
    for (uint k = 0; k < overlays; k++)
    {
        uint4 l = row_u(splats, at + 1 + k);
        if (l.x == 0xFFFFFFFF)
        {
            continue;
        }
        uint turn = l.y & 3;
        float a = sampled(l.x, held, turned(uv, turn), turned_step(dx, turn), turned_step(dy, turn)).a;
        float3 color = o.color;
        float cover;
        float own;
        float3 bend;
        float roughness;
        float height;
        steep_layer(row_u(splats, first + 1 + k), wrap, rot, uv, dx, dy, on_snow, row_u(splats, maps + 1 + k).w, place, color, cover, own, bend, roughness, height);
        o.color = lerp(color, o.color, a);
        o.cover = lerp(cover, o.cover, a);
        o.own = lerp(own, o.own, a);
        o.bend = lerp(bend, o.bend, a);
        o.roughness = lerp(roughness, o.roughness, a);
        o.height = lerp(height, o.height, a);
    }
    o.bend = normalize(o.bend);
    return o;
}

// What splat `at` lays by the water, `which` its row among them: the grass grown over a muddy bank
// (0), or the bed (2).
public SteepSample bank_row(uint splats, uint wrap, uint at, uint rot, float2 uv, float2 dx, float2 dy, uint which, PackPlace place)
{
    SteepSample o;
    o.color = float3(0.0, 0.0, 0.0);
    steep_layer(row_u(splats, at + SPLAT_BANK + which), wrap, rot, uv, dx, dy, 1.0, 0, place, o.color, o.cover, o.own, o.bend, o.roughness, o.height);
    return o;
}

// `n` bent by `bend`, a lean in the ground's own frame (x east, y north, z up).
public float3 bent(float3 n, float3 bend)
{
    if (bend.x == 0.0 && bend.y == 0.0)
    {
        return n;
    }
    float3 east = normalize(float3(1.0, 0.0, 0.0) - n * n.x);
    float3 north = cross(n, east);
    return normalize(east * bend.x + north * bend.y + n * bend.z);
}
