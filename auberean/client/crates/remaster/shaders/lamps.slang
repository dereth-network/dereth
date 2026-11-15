// The lamps' light: at each pixel, the light of the lamps of its piece of the view (the shared
// passes' lists), on the surface the depth's pass left there. A lamp is one of the frame's point
// lights, worked out per pixel as retail works it out at an object's vertices: its color, times
// how squarely it meets the surface, times retail's attenuation over the distance,
// 1 / (1 + 10 d^2 / R^2), ended smoothly at its range. Outdoors the lamps light what stands in the
// sun and, as night comes, the land, which retail lights by the sun alone; indoors the objects
// retail lights by them. Its alpha is unused.
import bindless;
import frame;

// The shared pass's constants, in buffer `rows`, as float4 rows (`frame_rows`).
static const uint ROW_VIEW = 0;
static const uint ROW_INV_VIEW_PROJ = 8;
static const uint ROW_EXTENT = 13;
static const uint ROW_SLICES = 14;
static const uint ROW_LIT = 15;
static const uint ROW_GRID = 16;

// A lamp is three float4 rows in buffer `lamps`: where it is and how far it reaches (`REACH`
// times its range); its color and kind (1 indoors, 2 outdoors); its strength on what stands in it
// and on the land, and its range.
static const float KIND_INDOOR = 1.0;
static const float KIND_OUTDOOR = 2.0;

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

// A lamp's light at distance `d` from a light of range `r` that reaches `reach`: retail's
// attenuation, which never ends, ended smoothly over the last tenth of the reach, where it is a
// hundredth of the light.
float attenuation(float d, float r, float reach)
{
    if (d >= reach)
    {
        return 0.0;
    }
    return saturate((reach - d) / (0.1 * reach)) / (1.0 + 10.0 * d * d / (r * r));
}

[shader("fragment")]
float4 fs_lamps(float4 position : SV_Position, uniform ShadePush push) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(textures[NonUniformResourceIndex(push.material)].Load(int3(p, 0)).x * 255.0 + 0.5);
    float4 lit = row(push.rows, ROW_LIT);
    bool indoor = cls == CLASS_INDOOR && lit.x > 0.5;
    bool outdoor = (cls == CLASS_GROUND || cls == CLASS_SUN) && lit.y > 0.5;
    if (!indoor && !outdoor)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float3 n = oct_decode(textures[NonUniformResourceIndex(push.normal)].Load(int3(p, 0)).xy);
    float4 e = row(push.rows, ROW_EXTENT);
    float2 ndc = float2(position.x / e.x * 2.0 - 1.0, 1.0 - position.y / e.y * 2.0);
    uint depth = outdoor ? push.outdoor_depth : push.depth;
    float dz = textures[NonUniformResourceIndex(depth)].Load(int3(p, 0)).x;
    float4 h = transform(push.rows, ROW_INV_VIEW_PROJ, float4(ndc, dz, 1.0));
    float3 world = h.xyz / h.w;
    float z = transform(push.rows, ROW_VIEW, float4(world, 1.0)).z;
    float4 sl = row(push.rows, ROW_SLICES);
    if (z > sl.y)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    uint4 grid = row_u(push.rows, ROW_GRID);
    float slice = floor(log(max(z, sl.x) / sl.x) / log(sl.y / sl.x) * sl.z);
    uint k = min(uint(max(slice, 0.0)), grid.z - 1);
    uint i = min(uint(position.x / e.z), grid.x - 1);
    uint j = min(uint(position.y / e.w), grid.y - 1);
    uint piece = (k * grid.y + j) * grid.x + i;
    uint base = piece * (grid.w + 1) * 4;
    ByteAddressBuffer list = buffers[NonUniformResourceIndex(push.pieces)];
    uint count = min(list.Load(base), grid.w);
    float3 light = float3(0.0, 0.0, 0.0);
    for (uint m = 0; m < count; m++)
    {
        uint l = list.Load(base + 4 + m * 4);
        float4 lp = row(push.lamps, l * 3);
        float4 lc = row(push.lamps, l * 3 + 1);
        float4 power = row(push.lamps, l * 3 + 2);
        if ((indoor && lc.w != KIND_INDOOR) || (outdoor && lc.w != KIND_OUTDOOR))
        {
            continue;
        }
        float3 to = lp.xyz - world;
        float dist = length(to);
        float mu = max(dot(n, to / max(dist, 1e-6)), 0.0);
        float strength = cls == CLASS_GROUND ? power.y : power.x;
        light += lc.rgb * strength * mu * attenuation(dist, power.z, lp.w);
    }
    return float4(light, 0.0);
}
