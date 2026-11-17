// Retail's light, as the frame's light inputs give it: the class rules each kind of surface is
// lit by, the fixed-function lights, the static lights burned into a room's walls, and the fog.
// Everything here is in display values in [0, 1], as retail computes them per vertex.
module retail;

import bindless;
// The frame's rows, the light classes and the rows of a buffer, which every importer of this
// module reads as its own.
__exported import frame;

// The material flag of a surface drawn under its vertices' colors, and of an art pack's leaves.
public static const uint VERTEX_COLORED = 512;
public static const uint PACK_FOLIAGE = 1024;

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

// Up to which brightness, against the light's color, a surface takes the lamps' light as it is.
public static const float LAMP_KNEE = 0.8;

// The brightness a surface takes from light `x`: `x` up to `LAMP_KNEE`, then rising ever slower
// toward 1 with no corner, so no ring shows where the light reaches the most a surface can take.
public float lamp_shoulder(float x)
{
    if (x <= LAMP_KNEE)
    {
        return x;
    }
    return LAMP_KNEE + (1.0 - LAMP_KNEE) * (1.0 - exp(-(x - LAMP_KNEE) / (1.0 - LAMP_KNEE)));
}

public float3 lamp_shoulder(float3 x)
{
    return float3(lamp_shoulder(x.r), lamp_shoulder(x.g), lamp_shoulder(x.b));
}
