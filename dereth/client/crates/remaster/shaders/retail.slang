// Retail's light, as the frame's light inputs give it: the class rules each kind of surface is
// lit by, the fixed-function lights, the static lights burned into a room's walls, and the fog.
// Everything here is in display values in [0, 1], as retail computes them per vertex.
module retail;

import bindless;

// The frame's constants, in storage buffer `frame` at offset 0, as float4 rows:
//   0..3   the view and projection together, as four columns
//   4..7   the view (into the eye), as four columns
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
// The hue an art pack's leaves are pulled toward, in turns (x), and how far (y, 0 not at all).
public static const uint FRAME_FOLIAGE = 23;

// `c` in hue (turns), saturation and value.
float3 hsv_of(float3 c)
{
    float hi = max(c.r, max(c.g, c.b));
    float lo = min(c.r, min(c.g, c.b));
    float span = hi - lo;
    float h = 0.0;
    if (span > 1e-6)
    {
        if (hi == c.r)
        {
            h = (c.g - c.b) / span;
        }
        else if (hi == c.g)
        {
            h = 2.0 + (c.b - c.r) / span;
        }
        else
        {
            h = 4.0 + (c.r - c.g) / span;
        }
    }
    return float3(frac(h / 6.0), hi > 0.0 ? span / hi : 0.0, hi);
}

// The color of hue (turns), saturation and value `hsv`.
float3 rgb_of(float3 hsv)
{
    float3 k = frac(hsv.x + float3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0;
    return hsv.z * lerp(float3(1.0, 1.0, 1.0), saturate(abs(k) - 1.0), hsv.y);
}

// An art pack's leaf color `c` pulled toward the frame's foliage hue: its hue taken that share of
// the way, by the shorter way round, its saturation and value as they were.
public float3 pack_foliage(uint frame, float3 c)
{
    float4 f = row(frame, FRAME_FOLIAGE);
    if (f.y <= 0.0)
    {
        return c;
    }
    float3 hsv = hsv_of(c);
    float off = frac(hsv.x - f.x + 0.5) - 0.5;
    hsv.x = frac(f.x + off * (1.0 - f.y));
    return rgb_of(hsv);
}

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

// The fixed-function lights of light set `set` (at most eight indices into the light buffer),
// at a point `p` with unit normal `n`: each point light's color times intensity times N.L over
// distance inside its range, each directional one's times N.L.
// A light is four float4 rows in buffer `lights`: position and range, diffuse and type
// (1 point, 2 spot, 3 directional), color, and intensity and falloff.
public float3 ff_lights(uint lights, uint sets, uint set, float3 p, float3 n)
{
    uint4 head = row_u(sets, set * 3);
    uint count = head.x;
    float3 sum = float3(0.0, 0.0, 0.0);
    for (uint k = 0; k < 8; k++)
    {
        if (k >= count)
        {
            break;
        }
        uint4 slots = row_u(sets, set * 3 + 1 + k / 4);
        uint l = slots[k % 4];
        float4 lp = row(lights, l * 4);
        float4 ld = row(lights, l * 4 + 1);
        if (ld.w == 3.0)
        {
            float3 L = -normalize(lp.xyz);
            sum += ld.rgb * max(dot(n, L), 0.0);
        }
        else
        {
            float3 d = lp.xyz - p;
            float dist = length(d);
            if (dist >= lp.w)
            {
                continue;
            }
            float3 L = d / max(dist, 1e-6);
            sum += ld.rgb * max(dot(n, L), 0.0) * (1.0 / dist);
        }
    }
    return sum;
}

// The static lights burned into a room's walls at a point `p` with normal `n`: every static
// light of the frame (the first `statics` of buffer `lights`), each channel capped at the light's
// color, the sum clamped and truncated to a byte.
public float3 burn(uint lights, uint statics, float3 p, float3 n)
{
    float3 c = float3(0.0, 0.0, 0.0);
    for (uint l = 0; l < statics; l++)
    {
        float4 ld = row(lights, l * 4 + 1);
        if (ld.w != 1.0)
        {
            continue;
        }
        float4 lp = row(lights, l * 4);
        float4 color = row(lights, l * 4 + 2);
        float4 b = row(lights, l * 4 + 3);
        float3 d = lp.xyz - p;
        float d2 = dot(d, d);
        float dist = sqrt(d2);
        float range = b.y * 1.3;
        if (dist >= range)
        {
            continue;
        }
        float n_dot = (dot(n, d) + dist * 0.5) * 0.6666667;
        if (n_dot <= 0.0)
        {
            continue;
        }
        float denom = d2 > 1.0 ? d2 * dist : dist;
        float s = (1.0 - dist / range) * b.x * (n_dot / denom);
        c += min(s * color.rgb, color.rgb);
    }
    return floor(saturate(c) * 255.0) / 255.0;
}

// The ground's light at a vertex with normal `n`: the sun times N.D plus the ground's ambient,
// each channel at most 1, truncated to a byte.
public float3 ground_light(uint frame, float3 n)
{
    float3 sun = row(frame, FRAME_SUN).xyz;
    float d = max(dot(n, sun), 0.0);
    float3 c = min(row(frame, FRAME_SUN_COLOR).rgb * d + row(frame, FRAME_GROUND_AMBIENT).rgb, 1.0);
    return floor(c * 255.0) / 255.0;
}

// The sun's light on an object at a unit normal `n`: its color times N.D, which carries its
// brightness.
public float3 sun_light(uint frame, float3 n)
{
    float3 sun = row(frame, FRAME_SUN).xyz;
    if (dot(sun, sun) < 1e-8)
    {
        return float3(0.0, 0.0, 0.0);
    }
    float b = length(sun);
    return row(frame, FRAME_SUN_COLOR).rgb * b * max(dot(n, sun / b), 0.0);
}

// The fog factor at eye distance `dist`: 1 is no fog, 0 fully fogged.
public float fog_factor(uint frame, float dist)
{
    float4 f = row(frame, FRAME_FOG);
    float k = saturate((f.y - dist) / max(f.y - f.x, 1e-6));
    return lerp(1.0, k, f.z);
}

// Retail's screen brightness ramp: `1 + 2v`.
public float gamma_scale(uint frame)
{
    return 1.0 + 2.0 * row(frame, FRAME_SCREEN).x;
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
