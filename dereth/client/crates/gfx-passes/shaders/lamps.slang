// The lamps' lists: for each piece of the view (a tile of the screen by a slice of depth), the
// lamps whose reach meets it (`cs_clusters`). The game's own shading then adds, at each pixel, the
// light of its piece's lamps.
//
// A lamp is three float4 rows in buffer `lamps`: where it is and its reach, then two rows of the
// game's own.
import bindless;
import frame;

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

struct ClusterPush
{
    uint rows;
    uint lamps;
    uint count;
    // Each piece: its count, then its lamps, one word each.
    uint pieces;
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
