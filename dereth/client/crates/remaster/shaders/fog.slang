// The fog as a volume over the view: for each of a grid of directions across the screen and each
// of a run of distances along it, how much of what lies there comes through the fog
// (transmittance) and how much light the fog in front of it adds (in-scatter), in display values.
//
// Retail's fog is a range: nothing before its start, all fog at its end, a straight ramp between,
// in the fog's colour. That is what the volume holds with no glow, so the world is fogged as
// retail fogs it. With glow, the fog in each stretch of the way also scatters the sun toward the
// eye, the more so the nearer the eye looks toward the sun, and only where the sun reaches that
// stretch (by the shadow maps): shafts of sunlight through the trees and past the buildings.
module fog;

import bindless;
import retail;

// The fog's constants, as float4 rows of buffer `f`:
//   0  x the nearest distance, y the farthest, z the slices, w the glow's strength
//   1  rgb the sun's colour as the fog scatters it, w the forward scattering's asymmetry
//   2  xy the grid's columns and rows; z the shadow's cascades' buffer (~0 for no shadows);
//      w how strong the shadows are
//   3  x how much of what lies at and past the fog's end comes through it, y the fog's end
public static const uint FOG_RANGE = 0;
public static const uint FOG_SUN = 1;
public static const uint FOG_GRID = 2;
public static const uint FOG_FAR = 3;

// The distance of slice `k` (0 the eye's nearest, `slices` the farthest).
public float slice_distance(uint f, float k)
{
    float4 r = row(f, FOG_RANGE);
    return r.x * pow(r.y / r.x, k / r.z);
}

// The slice coordinate of distance `d`.
public float slice_of(uint f, float d)
{
    float4 r = row(f, FOG_RANGE);
    return log(max(d, r.x) / r.x) / log(r.y / r.x) * r.z;
}

// What the fog adds to and keeps of a surface `dist` metres away along the screen point `uv` (0
// to 1): rgb the light added, a the share of the surface's own light that comes through.
public float4 fog_at(uint f, uint volume, float2 uv, float dist)
{
    float4 grid = row(f, FOG_GRID);
    float4 range = row(f, FOG_RANGE);
    float3 p = float3(uv * grid.xy - 0.5, slice_of(f, dist) - 1.0);
    float3 lo = floor(p);
    float3 t = p - lo;
    int3 size = int3(int(grid.x), int(grid.y), int(range.z));
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int k = 0; k < 8; k++)
    {
        int3 o = int3(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        int3 c = clamp(int3(lo) + o, int3(0, 0, -1), size - 1);
        float w = (o.x == 1 ? t.x : 1.0 - t.x) * (o.y == 1 ? t.y : 1.0 - t.y)
                  * (o.z == 1 ? t.z : 1.0 - t.z);
        // Before the first slice, nothing of the fog.
        float4 v = c.z < 0 ? float4(0.0, 0.0, 0.0, 1.0)
                           : asfloat(buffers[NonUniformResourceIndex(volume)].Load4(
                                 ((c.z * size.y + c.y) * size.x + c.x) * 16));
        acc += v * w;
    }
    return acc;
}
