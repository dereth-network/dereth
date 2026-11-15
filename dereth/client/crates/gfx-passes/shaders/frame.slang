// The frame's vocabulary, shared by every game's remaster and the passes they share: the rows of
// the frame's constants, the light classes the material target carries, and how a buffer's rows
// are read. A game's own light model imports it and adds its own rows and rules.
module frame;

import bindless;

// The frame's constants, in storage buffer `frame` at offset 0, as float4 rows:
//   0..3   the view and projection together, as four columns
//   4..7   the view (into the eye, z the distance ahead of it), as four columns
//   8      the eye, xyz
//   9      the sun: its direction scaled by its brightness, xyz (not normalized)
//   10     the sun's color, rgb
//   11     the ground's ambient: its color times its level, rgb
//   12     the objects' ambient, as the device is given it, rgb
//   13     the fog: start, end, on (0 or 1), unused
//   14     the fog's color, rgb
//   15     x the screen brightness, y the mode (0 the light per vertex, 1 per pixel),
//          z the light count, w the static light count
//   16..19 the inverse of the view and projection, as four columns
//   20     the target's extent in pixels, xy
public static const uint FRAME_VIEW_PROJ = 0;
public static const uint FRAME_VIEW = 4;
public static const uint FRAME_EYE = 8;
public static const uint FRAME_SUN = 9;
public static const uint FRAME_SUN_COLOR = 10;
public static const uint FRAME_GROUND_AMBIENT = 11;
public static const uint FRAME_OBJECT_AMBIENT = 12;
public static const uint FRAME_FOG = 13;
public static const uint FRAME_FOG_COLOR = 14;
public static const uint FRAME_SCREEN = 15;
public static const uint FRAME_INV_VIEW_PROJ = 16;
public static const uint FRAME_EXTENT = 20;
// Where the near ground gives way to the far, meters from the eye (x, y), and z 1 while a far
// ground is drawn: what stands on the ground is lifted onto it mixed the same way.
public static const uint FRAME_LIFT = 22;
// How far round the screen patterns a tree's pictures are cut by are turned this frame, while the
// smoothing or an upscaler lays frames over each other: x the pattern that picks a card's view, y
// the one that cuts its coverage, 0 to 1; z 1 while they turn at all, 0 while they hold still.
public static const uint FRAME_DITHER = 23;
// The wind what grows sways in (`swayed`): xy its push east and north, meters a second; z the
// sway's clock within its period (1200 s), seconds.
public static const uint FRAME_WIND = 24;

// The light classes, which the material target's first channel carries.
public static const uint CLASS_NONE = 0;
public static const uint CLASS_GROUND = 1;
public static const uint CLASS_SUN = 2;
public static const uint CLASS_INDOOR = 3;
public static const uint CLASS_CELL = 4;
public static const uint CLASS_SKY = 5;

// One float4 of buffer `b`, the `i`th.
public float4 row(uint b, uint i)
{
    return asfloat(buffers[NonUniformResourceIndex(b)].Load4(i * 16));
}

// One uint4 of buffer `b`, the `i`th.
public uint4 row_u(uint b, uint i)
{
    return buffers[NonUniformResourceIndex(b)].Load4(i * 16);
}

// The column-major matrix at row `i` of buffer `b`, applied to `v`.
public float4 transform(uint b, uint i, float4 v)
{
    return row(b, i) * v.x + row(b, i + 1) * v.y + row(b, i + 2) * v.z + row(b, i + 3) * v.w;
}

// The material target's bytes: the light class, the material's kind, and the light set's index
// over two bytes.
public float4 material_bytes(uint cls, uint kind, uint set)
{
    return float4(float(cls), float(kind), float(set & 0xFF), float((set >> 8) & 0xFF)) / 255.0;
}

// A unit normal folded into two signed values (octahedral), and back.
public float2 oct_encode(float3 n)
{
    n /= abs(n.x) + abs(n.y) + abs(n.z);
    float2 e = n.xy;
    if (n.z < 0.0)
    {
        e = (1.0 - abs(n.yx)) * float2(n.x >= 0.0 ? 1.0 : -1.0, n.y >= 0.0 ? 1.0 : -1.0);
    }
    return e;
}

public float3 oct_decode(float2 e)
{
    float3 n = float3(e.xy, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0)
    {
        n.xy = (1.0 - abs(n.yx)) * float2(n.x >= 0.0 ? 1.0 : -1.0, n.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(n);
}

// A display value taken to linear light: the sRGB curve, continued above 1.
public float3 to_linear(float3 c)
{
    float3 low = c / 12.92;
    float3 high = pow((max(c, 0.0) + 0.055) / 1.055, 2.4);
    return float3(c.x <= 0.04045 ? low.x : high.x, c.y <= 0.04045 ? low.y : high.y,
                  c.z <= 0.04045 ? low.z : high.z);
}

public static const float SWAY_TAU = 6.2831853;
// The sway's motions, each a whole number of cycles in the weather's clock's period (1200 s), so
// the clock's wrap moves nothing: the slow swing (about 5 s), the gusts that roll across the land
// (about 20 s), a sideways rock (about 4 s) and the leaves' flutter (about 1 s).
public static const float SWAY_SWING = SWAY_TAU * 229.0 / 1200.0;
public static const float SWAY_GUST = SWAY_TAU * 61.0 / 1200.0;
public static const float SWAY_ROCK = SWAY_TAU * 311.0 / 1200.0;
public static const float SWAY_FLUTTER = SWAY_TAU * 1259.0 / 1200.0;

// How far the wind moves the vertex at `pos` of a growing model standing at `origin` (both the
// frame's space), which bends `bend` meters at a wind of a meter a second, in the frame's wind `w`
// (its row of the frame's constants); a cut-out leaf (`leaf`) flutters as well. The model leans
// with the wind, further in a gust, and swings about that lean; trees apart swing apart. Only
// `origin`'s xy is read.
public float3 swayed(float4 w, float bend, float3 origin, float3 pos, bool leaf)
{
    float strength = length(w.xy);
    if (strength < 1e-4)
    {
        return float3(0.0, 0.0, 0.0);
    }
    float2 along = w.xy / strength;
    float2 across = float2(-along.y, along.x);
    float t = w.z;
    float phase = dot(origin.xy, float2(0.173, 0.131));
    float gust = 0.5 + 0.5 * sin(t * SWAY_GUST - dot(origin.xy, float2(0.021, 0.017)));
    float swing = sin(t * SWAY_SWING + phase);
    float rock = sin(t * SWAY_ROCK + phase * 1.7);
    float reach = bend * strength;
    float3 d = float3(along * reach * (0.4 + 0.35 * gust + 0.25 * swing) + across * reach * 0.12 * rock, 0.0);
    if (leaf)
    {
        float f = sin(t * SWAY_FLUTTER + dot(pos, float3(1.7, 1.3, 2.1)));
        d += float3(across * 0.05, 0.03) * strength * min(reach * 4.0, 1.0) * f;
    }
    return d;
}
