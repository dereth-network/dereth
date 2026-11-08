// The lamps: the scene's lights worked out per pixel, from lists of the lights that reach each
// piece of the view.
//
// - `cs_clusters` lists, for each piece of the view (a tile of the screen by a slice of depth),
//   the lamps whose reach meets it.
// - `fs_lamps` adds, at each pixel, the light of its piece's lamps: indoor lamps on rooms' walls
//   and on objects indoors, outdoor lamps on the ground and what the sun lights. Its alpha is 1
//   where the lamps take the place of retail's own light indoors; outdoors, below nought, it is how
//   much the pixel glows for standing right at a lamp, its flame or its glass.
//
// On a room's walls a static light's lamp is retail's burn worked out at the pixel. Elsewhere a
// static light's lamp falls off with the square of the distance, softened within half a metre,
// and ends smoothly at its reach; its strength, for each kind of surface, comes calibrated from
// the processor. A moving light's lamp is retail's light, its intensity over the distance, faded
// out over the last tenth of its reach. A static light's turn on walls and outdoors is wrapped, lighting to 120
// degrees as retail's burn does; a moving light's, and every light's on objects, is the plain
// cosine.
import bindless;
import retail;

// The pass's constants, in buffer `rows`, as float4 rows:
//   0..3   the view, as four columns
//   4..7   the projection's inverse, as four columns
//   8..11  the view and projection's inverse, as four columns
//   12     the eye, xyz
//   13     the world's size in pixels, xy; a tile's, zw
//   14     the first slice's depth, the last's, how many slices, and the softening distance
//   15     x 1 when the indoors is lit, y 1 when the outdoors is
//   16     (as words) the tiles across and down, the slices, and the lamps a piece lists at most
static const uint ROW_VIEW = 0;
static const uint ROW_INV_PROJ = 4;
static const uint ROW_INV_VIEW_PROJ = 8;
static const uint ROW_EYE = 12;
static const uint ROW_EXTENT = 13;
static const uint ROW_SLICES = 14;
static const uint ROW_LIT = 15;
static const uint ROW_GRID = 16;

// A lamp is three float4 rows in buffer `lamps`: where it is and its reach; its color and kind
// (1 a static light indoors, 2 a light outdoors, 3 a moving light indoors); its strength on
// walls, on objects, and outdoors, and its reach on objects. Outdoors the first two are how near a
// surface takes its glow, and how much.
static const float KIND_STATIC = 1.0;
static const float KIND_OUTDOOR = 2.0;
static const float KIND_MOVING = 3.0;

struct ClusterPush
{
    uint rows;
    uint lamps;
    uint count;
    // Each piece: its count, then its lamps, one word each.
    uint pieces;
};

struct ShadePush
{
    uint rows;
    uint lamps;
    uint pieces;
    // The G-buffer's normal and material, the depth, and the depth the outdoor surfaces are
    // placed by.
    uint normal;
    uint material;
    uint depth;
    uint outdoor_depth;
};

// The direction from the eye, in the eye's space, through the point `ndc` of the screen, scaled
// to depth 1.
float3 ray(uint rows, float2 ndc)
{
    float4 p = transform(rows, ROW_INV_PROJ, float4(ndc, 1.0, 1.0));
    float3 v = p.xyz / p.w;
    return abs(v.z) < 1e-9 ? float3(0.0, 0.0, 1.0) : v / v.z;
}

// The depth slice `k` starts at.
float slice_depth(uint rows, uint k)
{
    float4 s = row(rows, ROW_SLICES);
    return s.x * pow(s.y / s.x, float(k) / s.z);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_clusters(uint3 id : SV_DispatchThreadID, uniform ClusterPush push)
{
    uint4 grid = row_u(push.rows, ROW_GRID);
    uint n = id.x;
    if (n >= grid.x * grid.y * grid.z)
    {
        return;
    }
    uint i = n % grid.x;
    uint j = (n / grid.x) % grid.y;
    uint k = n / (grid.x * grid.y);
    float4 e = row(push.rows, ROW_EXTENT);
    float2 extent = e.xy;
    float2 tile = e.zw;
    float z0 = slice_depth(push.rows, k);
    float z1 = slice_depth(push.rows, k + 1);
    float3 lo = float3(1e30, 1e30, 1e30);
    float3 hi = float3(-1e30, -1e30, -1e30);
    for (uint c = 0; c < 4; c++)
    {
        float2 px = float2(float(i + (c & 1)), float(j + (c >> 1))) * tile;
        px = min(px, extent);
        float2 ndc = float2(px.x / max(extent.x, 1.0) * 2.0 - 1.0, 1.0 - px.y / max(extent.y, 1.0) * 2.0);
        float3 r = ray(push.rows, ndc);
        lo = min(lo, min(r * z0, r * z1));
        hi = max(hi, max(r * z0, r * z1));
    }
    RWByteAddressBuffer out = rw_buffers[NonUniformResourceIndex(push.pieces)];
    uint base = n * (grid.w + 1) * 4;
    uint listed = 0;
    for (uint l = 0; l < push.count; l++)
    {
        if (listed >= grid.w)
        {
            break;
        }
        float4 lp = row(push.lamps, l * 3);
        float3 c = transform(push.rows, ROW_VIEW, float4(lp.xyz, 1.0)).xyz;
        float3 nearest = clamp(c, lo, hi);
        float3 d = nearest - c;
        if (dot(d, d) <= lp.w * lp.w)
        {
            out.Store(base + 4 + listed * 4, l);
            listed++;
        }
    }
    out.Store(base, listed);
}

// How a surface at cosine `mu` to a lamp takes its light: wrapped, lit to 120 degrees and at two
// thirds facing it squarely, or the plain cosine.
float turn(float mu, bool wrapped)
{
    return wrapped ? max((mu + 0.5) * (2.0 / 3.0), 0.0) : max(mu, 0.0);
}

// A static light's share of retail's burn at distance `d` and cosine `mu`, worked out at the
// pixel: within `range`, flat within a metre and the inverse square beyond, at most 1.
float burn_share(float d, float mu, float intensity, float range)
{
    if (d >= range)
    {
        return 0.0;
    }
    float near = d * d > 1.0 ? 1.0 / (d * d) : 1.0;
    return min((1.0 - d / range) * intensity * turn(mu, true) * near, 1.0);
}

// A lamp's light at distance `d` within reach `reach`, before its strength and the surface's
// turn: a static light's the inverse square, ended smoothly; a moving light's the inverse of the
// distance, faded out over the last tenth.
float falloff(float d, float reach, float soften, bool moving)
{
    if (d >= reach)
    {
        return 0.0;
    }
    if (moving)
    {
        return saturate((reach - d) / (0.1 * reach)) / max(d, 1e-3);
    }
    float x = d / reach;
    float w = saturate(1.0 - x * x * x * x);
    return w * w / (d * d + soften * soften);
}

// A pixel of the G-buffer as the lamps see it.
struct Surface
{
    uint cls;
    // Whether the indoor lamps light it, or the outdoor ones.
    bool indoor;
    bool outdoor;
    float3 world;
    float3 n;
    // Its view depth, and where it is on the screen.
    float z;
    float2 position;
};

Surface surface_at(ShadePush push, int2 p, float2 position)
{
    Surface s;
    uint4 mat = uint4(textures[NonUniformResourceIndex(push.material)].Load(int3(p, 0)) * 255.0 + 0.5);
    s.cls = mat.x;
    s.position = position;
    float4 lit = row(push.rows, ROW_LIT);
    s.indoor = (s.cls == CLASS_CELL || s.cls == CLASS_INDOOR) && lit.x > 0.5;
    s.outdoor = (s.cls == CLASS_GROUND || s.cls == CLASS_SUN) && lit.y > 0.5;
    s.n = float3(0.0, 0.0, 1.0);
    s.world = float3(0.0, 0.0, 0.0);
    s.z = 0.0;
    if (!s.indoor && !s.outdoor)
    {
        return s;
    }
    s.n = oct_decode(textures[NonUniformResourceIndex(push.normal)].Load(int3(p, 0)).xy);
    float4 e = row(push.rows, ROW_EXTENT);
    float2 ndc = float2(position.x / e.x * 2.0 - 1.0, 1.0 - position.y / e.y * 2.0);
    uint depth = s.outdoor ? push.outdoor_depth : push.depth;
    float d = textures[NonUniformResourceIndex(depth)].Load(int3(p, 0)).x;
    float4 h = transform(push.rows, ROW_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    s.world = h.xyz / h.w;
    s.z = transform(push.rows, ROW_VIEW, float4(s.world, 1.0)).z;
    return s;
}

// The light of a surface's lamps, and how much it glows.
float3 lamps_on(ShadePush push, Surface s, out float glow)
{
    float3 out = float3(0.0, 0.0, 0.0);
    glow = 0.0;
    float4 sl = row(push.rows, ROW_SLICES);
    if (s.z > sl.y)
    {
        return out;
    }
    float4 e = row(push.rows, ROW_EXTENT);
    uint4 grid = row_u(push.rows, ROW_GRID);
    float slice = floor(log(max(s.z, sl.x) / sl.x) / log(sl.y / sl.x) * sl.z);
    uint k = min(uint(max(slice, 0.0)), grid.z - 1);
    uint i = min(uint(s.position.x / e.z), grid.x - 1);
    uint j = min(uint(s.position.y / e.w), grid.y - 1);
    uint piece = (k * grid.y + j) * grid.x + i;
    uint base = piece * (grid.w + 1) * 4;
    ByteAddressBuffer list = buffers[NonUniformResourceIndex(push.pieces)];
    uint count = min(list.Load(base), grid.w);
    for (uint m = 0; m < count; m++)
    {
        uint l = list.Load(base + 4 + m * 4);
        float4 lp = row(push.lamps, l * 3);
        float4 lc = row(push.lamps, l * 3 + 1);
        float4 power = row(push.lamps, l * 3 + 2);
        float strength = 0.0;
        float reach = lp.w;
        bool wrapped = false;
        bool burned = false;
        if (s.indoor && (lc.w == KIND_STATIC || lc.w == KIND_MOVING))
        {
            if (s.cls == CLASS_CELL)
            {
                strength = power.x;
                wrapped = lc.w == KIND_STATIC;
                burned = wrapped;
            }
            else
            {
                strength = power.y;
                reach = power.w;
            }
        }
        else if (s.outdoor && lc.w == KIND_OUTDOOR)
        {
            strength = power.z;
            wrapped = true;
        }
        if (strength <= 0.0)
        {
            continue;
        }
        float3 to = lp.xyz - s.world;
        float dist = length(to);
        if (s.outdoor && dist < power.x)
        {
            float g = 1.0 - dist / power.x;
            glow = max(glow, power.y * g * g * (3.0 - 2.0 * g));
        }
        float mu = dot(s.n, to / max(dist, 1e-6));
        out += burned ? lc.rgb * burn_share(dist, mu, strength, reach)
                      : lc.rgb * strength * turn(mu, wrapped)
                            * falloff(dist, reach, sl.w, lc.w == KIND_MOVING);
    }
    return out;
}

[shader("fragment")]
float4 fs_lamps(float4 position : SV_Position, uniform ShadePush push) : SV_Target0
{
    Surface s = surface_at(push, int2(position.xy), position.xy);
    if (!s.indoor && !s.outdoor)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float glow;
    float3 light = lamps_on(push, s, glow);
    return float4(light, s.indoor ? 1.0 : -glow);
}
