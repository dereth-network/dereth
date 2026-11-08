// The atmosphere every sky shader shares: its constants, the scattering medium, the planet's
// geometry, the lookup tables' parametrisations, and how the sky's light is exposed for a picture
// in display values.
//
// The sky model is a planet of radius `ground.w` kilometres under an atmosphere reaching
// `misc.z`, holding Rayleigh scatterers (air), Mie scatterers (haze) and an absorbing ozone
// layer. Distances in the sky model are kilometres; distances in the world are metres. The
// frame's space has z up.
module atmosphere;

import bindless;
import retail;
import intent;

// The constants, as float4 rows of buffer `a`:
//   0..3  normalised device coordinates to a direction in the frame's space (the inverse of the
//         projection times the view's rotation alone), as four columns
//   4     xyz the eye; w its height over the planet, kilometres
//   5     xy the target's extent in pixels
//   6     xyz the unit direction toward the sun; w how overcast the day is, 0 clear to 1 grey
//   7     rgb the sun's illuminance as the sky is exposed for; w how far into the low sun of dawn
//         or dusk the day is, 0 to 1
//   8     rgb Rayleigh scattering per kilometre at the ground; w -1 / its scale height, km
//   9     x Mie scattering, y Mie extinction per kilometre at the ground, z -1 / its scale
//         height, w the Mie phase's asymmetry
//   10    rgb ozone absorption per kilometre at the layer's peak
//   11    rgb the ground's albedo; w the planet's radius, km
//   12    rgb the fog's color, linear
//   13    x how much of the authored sky dome the physical sky covers, y 1 when the frame's
//         colors are drawn linear (0 in display values), z the atmosphere's outer radius, km
//   14    xy the sky-view table's size, w its raymarch's steps
//   15    x the horizon's veil's band (the sine of an elevation), y how much of the day it is (0
//         night to 1 day), z how bright the veil is against the fog
public static const uint ATM_RAY = 0;
public static const uint ATM_EYE = 4;
public static const uint ATM_EXTENT = 5;
public static const uint ATM_SUN = 6;
public static const uint ATM_SUN_LIGHT = 7;
public static const uint ATM_RAYLEIGH = 8;
public static const uint ATM_MIE = 9;
public static const uint ATM_OZONE = 10;
public static const uint ATM_GROUND = 11;
public static const uint ATM_FOG = 12;
public static const uint ATM_MISC = 13;
public static const uint ATM_TABLES = 14;
public static const uint ATM_VEIL = 15;

public static const float PI = 3.14159265358979;

// The sky's color `c`, in display values, as the frame draws its colors: dithered in display
// values, or taken to linear light as the rest of a linear frame is.
public float3 drawn_as(uint a, float3 c, float2 pixel)
{
    if (row(a, ATM_MISC).y > 0.5)
    {
        return to_linear(saturate(c));
    }
    return saturate(c + dither(pixel));
}

public float planet_radius(uint a)
{
    return row(a, ATM_GROUND).w;
}

public float top_radius(uint a)
{
    return row(a, ATM_MISC).z;
}

// The nearest non-negative distance along `d` (unit) from `o` to a sphere of radius `r` about the
// planet's centre, or -1 when the ray misses it.
public float ray_sphere(float3 o, float3 d, float r)
{
    float b = dot(o, d);
    float c = dot(o, o) - r * r;
    float disc = b * b - c;
    if (disc < 0.0)
    {
        return -1.0;
    }
    float s = sqrt(disc);
    float near = -b - s;
    float far = -b + s;
    if (near >= 0.0)
    {
        return near;
    }
    if (far >= 0.0)
    {
        return far;
    }
    return -1.0;
}

public struct Medium
{
    public float3 scattering;
    public float3 extinction;
    public float3 rayleigh;
    public float mie;
};

// The scattering medium at height `h` kilometres over the ground.
public Medium medium(uint a, float h)
{
    float hh = max(h, 0.0);
    float4 ray = row(a, ATM_RAYLEIGH);
    float4 mie = row(a, ATM_MIE);
    float rayleigh_density = exp(hh * ray.w);
    float mie_density = exp(hh * mie.z);
    float ozone_density = max(0.0, 1.0 - abs(hh - 25.0) / 15.0);
    Medium m;
    m.rayleigh = ray.rgb * rayleigh_density;
    m.mie = mie.x * mie_density;
    m.scattering = m.rayleigh + float3(m.mie, m.mie, m.mie);
    m.extinction = m.rayleigh + float3(1.0, 1.0, 1.0) * (mie.y * mie_density)
                   + row(a, ATM_OZONE).rgb * ozone_density;
    return m;
}

public float rayleigh_phase(float mu)
{
    return 3.0 / (16.0 * PI) * (1.0 + mu * mu);
}

// The Cornette-Shanks form of the Mie phase function.
public float mie_phase(float mu, float g)
{
    float g2 = g * g;
    float k = 3.0 / (8.0 * PI) * (1.0 - g2) / (2.0 + g2);
    return k * (1.0 + mu * mu) / pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5);
}

// The transmittance table maps (height, cosine of the zenith angle) so that both the ground and
// the horizon get resolution.
public float2 transmittance_uv(uint a, float r, float mu)
{
    float bottom = planet_radius(a);
    float top = top_radius(a);
    float h = sqrt(max(top * top - bottom * bottom, 0.0));
    float rho = sqrt(max(r * r - bottom * bottom, 0.0));
    float disc = r * r * (mu * mu - 1.0) + top * top;
    float d = max(0.0, -r * mu + sqrt(max(disc, 0.0)));
    float d_min = top - r;
    float d_max = rho + h;
    return float2((d - d_min) / max(d_max - d_min, 1e-6), rho / h);
}

public float2 transmittance_from_uv(uint a, float2 uv)
{
    float bottom = planet_radius(a);
    float top = top_radius(a);
    float h = sqrt(max(top * top - bottom * bottom, 0.0));
    float rho = h * uv.y;
    float r = sqrt(rho * rho + bottom * bottom);
    float d_min = top - r;
    float d_max = rho + h;
    float d = d_min + uv.x * (d_max - d_min);
    float mu = 1.0;
    if (d > 0.0)
    {
        mu = (h * h - rho * rho - d * d) / (2.0 * r * d);
    }
    return float2(r, clamp(mu, -1.0, 1.0));
}

// The sky-view table maps (cosine of the view's zenith angle, the angle to the sun about the
// vertical) with most of its rows near the horizon and most of its columns near the sun.
public float2 sky_view_uv(uint a, float r, float view_zenith_cos, float light_view_cos)
{
    float bottom = planet_radius(a);
    float v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    float beta = acos(clamp(v_horizon / r, -1.0, 1.0));
    float zenith_horizon = PI - beta;
    float view_zenith = acos(clamp(view_zenith_cos, -1.0, 1.0));
    float v;
    if (view_zenith < zenith_horizon)
    {
        float c = 1.0 - view_zenith / zenith_horizon;
        c = 1.0 - sqrt(max(c, 0.0));
        v = c * 0.5;
    }
    else
    {
        float c = (view_zenith - zenith_horizon) / max(beta, 1e-6);
        v = sqrt(max(c, 0.0)) * 0.5 + 0.5;
    }
    float u = sqrt(max(-light_view_cos * 0.5 + 0.5, 0.0));
    return float2(u, v);
}

// The inverse of `sky_view_uv`: (cosine of the view's zenith angle, cosine of the angle to the
// sun about the vertical).
public float2 sky_view_from_uv(uint a, float r, float2 uv)
{
    float bottom = planet_radius(a);
    float v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    float beta = acos(clamp(v_horizon / r, -1.0, 1.0));
    float zenith_horizon = PI - beta;
    float view_zenith;
    if (uv.y < 0.5)
    {
        float c = 1.0 - 2.0 * uv.y;
        view_zenith = zenith_horizon * (1.0 - c * c);
    }
    else
    {
        float c = uv.y * 2.0 - 1.0;
        view_zenith = zenith_horizon + beta * c * c;
    }
    float c = uv.x * uv.x;
    return float2(cos(view_zenith), -(c * 2.0 - 1.0));
}

// The eye's distance from the planet's centre, kept a little over the ground.
public float eye_radius(uint a)
{
    return planet_radius(a)
           + clamp(row(a, ATM_EYE).w, 0.002, top_radius(a) - planet_radius(a) - 1.0);
}

// The cosine of `dir`'s angle to the sun about the vertical (+z).
public float light_view_cos(uint a, float3 dir)
{
    float2 p = dir.xy;
    float2 q = row(a, ATM_SUN).xy;
    float lp = length(p);
    float lq = length(q);
    if (lp < 1e-5 || lq < 1e-5)
    {
        return 1.0;
    }
    return clamp(dot(p, q) / (lp * lq), -1.0, 1.0);
}

// A direction in the frame's space from normalised device coordinates.
public float3 view_ray(uint a, float2 ndc)
{
    float4 p = transform(a, ATM_RAY, float4(ndc, 0.5, 1.0));
    return normalize(p.xyz / p.w);
}

// A smooth roll-off for light brighter than the picture can hold: the identity while every
// channel is under `k`, then the brightest channel approaches 1 and the others follow it in
// proportion, going toward white the brighter the light is. In the low sun it goes less toward
// white, so the glow of dawn and dusk stays orange.
public float3 shoulder(uint a, float3 x)
{
    float k = 0.8;
    float peak = max(x.r, max(x.g, x.b));
    if (peak <= k)
    {
        return x;
    }
    float rolled = k + (1.0 - k) * (1.0 - exp(-(peak - k) / (1.0 - k)));
    float3 kept = x * (rolled / peak);
    float white = clamp((peak - k) / 1.2, 0.0, 1.0) * 0.75 * (1.0 - 0.6 * row(a, ATM_SUN_LIGHT).w);
    return lerp(kept, float3(rolled, rolled, rolled), white);
}

public float luma(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// The sky's radiance, exposed for the picture: given a little of the fog's cast, greyed toward
// that cast as the day is overcast, and rolled off where it is brighter than the picture holds.
public float3 exposed_sky(uint a, float3 raw)
{
    float4 sun = row(a, ATM_SUN_LIGHT);
    float3 lit = raw * sun.rgb;
    float3 fog = row(a, ATM_FOG).rgb;
    float3 tone = clamp(fog / max(luma(fog), 1e-4), float3(0.6, 0.6, 0.6), float3(1.4, 1.4, 1.4));
    float3 grey = luma(lit) * tone;
    return shoulder(a, lerp(lit, grey * 1.05, max(row(a, ATM_SUN).w, 0.06)));
}

// Linear light to sRGB display values.
public float3 encode(float3 c)
{
    float3 x = max(c, float3(0.0, 0.0, 0.0));
    float3 lo = x * 12.92;
    float3 hi = 1.055 * pow(x, float3(1.0 / 2.4, 1.0 / 2.4, 1.0 / 2.4)) - 0.055;
    return float3(x.r <= 0.0031308 ? lo.r : hi.r, x.g <= 0.0031308 ? lo.g : hi.g,
                  x.b <= 0.0031308 ? lo.b : hi.b);
}

// Triangular noise of one step of an eight-bit channel at `pixel`, the same every frame, so
// gradients don't band and a still view stays still.
public float dither(float2 pixel)
{
    uint2 seed = uint2(pixel);
    uint h = seed.x * 1664525u + seed.y * 1013904223u + 374761393u;
    h = (h ^ (h >> 15u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    h = h ^ (h >> 16u);
    float p = float(h & 0xffffu) / 65535.0;
    float q = float(h >> 16u) / 65535.0;
    return (p + q - 1.0) / 255.0;
}

// With an art pack, the air between the eye and the land and the clouds' shadows on it, as more
// rows of the same buffer; all zero otherwise:
//   16    rgb the low haze's color, linear; w its extinction per metre at the eye's height
//   17    x where the haze begins, y how far it thins with height, w how much brighter it glows
//         looking at the sun
//   18    the rectangle the land ends at: x and y its lowest corner, z and w its highest
//   19    x how much of the day it is, y how much thicker than the real air the air is drawn, z 1
//         while the air is drawn, w where along the way to the land's end it starts to fade
//   20    x and y the least and the most the haze's density differs from its density at the eye's
//         height, w over how many metres its glow builds up
//   21    x how much of the haze's color is the sky's, y how much of the land at its very end the
//         haze takes, w over how many metres past its start the air eases in
//   22    x how far the haze leans toward blue, y how steeply it thickens with distance, z the
//         length of air over which it reaches its depth
//   23    the clouds' shadow map: xy its center, z how far it reaches each way, w the clouds' base
//   24    x how much of the sun the clouds' shadows take (0 for none), y the map's picture, z its texels
public static const uint ATM_HAZE = 16;
public static const uint ATM_HAZE_AT = 17;
public static const uint ATM_LAND = 18;
public static const uint ATM_AIR = 19;
public static const uint ATM_HAZE_SHAPE = 20;
public static const uint ATM_HAZE_LOOK = 21;
public static const uint ATM_HAZE_TINT = 22;
public static const uint ATM_CLOUD_MAP = 23;
public static const uint ATM_CLOUD_SHADE = 24;

// Whether the air is drawn over the land by these constants.
public bool air_drawn(uint a)
{
    return row(a, ATM_AIR).z > 0.5;
}

// The exposed sky just over the horizon toward `dir`, or a little higher when it looks up: what the
// air in front of the land scatters toward the eye. `sky_view` is the table, read with `sampler`.
public float3 horizon_sky(uint a, uint sky_view, uint sampler, float3 dir)
{
    float2 uv = sky_view_uv(a, eye_radius(a), max(dir.z, 0.01), light_view_cos(a, dir));
    return exposed_sky(a, textures[NonUniformResourceIndex(sky_view)]
                              .SampleLevel(samplers[NonUniformResourceIndex(sampler)], uv, 0.0)
                              .rgb);
}

// The low haze's own light toward `dir`: by day mostly the sky's own color at the horizon in that
// direction, so the distance is lit by the sky drawn over it; the rest the fog's, warmer and brighter
// toward the sun. `reach` is how much of the glow the air in front of the point has built up.
public float3 haze_light(uint a, uint sky_view, uint sampler, float3 dir, float reach)
{
    float g = 0.6;
    float mu = dot(dir, row(a, ATM_SUN).xyz);
    float lobe = pow((1.0 + g * g - 2.0 * g) / max(1.0 + g * g - 2.0 * g * mu, 1e-4), 1.5);
    float4 sun = row(a, ATM_SUN_LIGHT);
    float3 warm = sun.rgb / max(max(sun.r, max(sun.g, sun.b)), 1e-4);
    float3 authored = row(a, ATM_HAZE).rgb * lerp(float3(1.0, 1.0, 1.0), warm, lobe)
                      * (1.0 + row(a, ATM_HAZE_AT).w * lobe * reach);
    float toward_sun = pow(0.5 + 0.5 * light_view_cos(a, dir), 3.0);
    float share = row(a, ATM_HAZE_LOOK).x * lerp(1.0, toward_sun, sun.w);
    return shoulder(a, lerp(authored, horizon_sky(a, sky_view, sampler, dir), share));
}

// The share of the sun's light the clouds let through to the point `world`: their shadow map read
// where the sun's line from the point meets their base, between its four nearest texels, fading to
// none toward the map's edge.
public float cloud_light(uint a, float3 world)
{
    float4 shade = row(a, ATM_CLOUD_SHADE);
    float3 sun = row(a, ATM_SUN).xyz;
    if (shade.x <= 0.0 || sun.z <= 0.01)
    {
        return 1.0;
    }
    float4 map = row(a, ATM_CLOUD_MAP);
    float2 off = (world.xy + sun.xy * ((map.w - world.z) / sun.z) - map.xy) / map.z;
    float edge = 1.0 - smoothstep(0.75, 0.95, max(abs(off.x), abs(off.y)));
    if (edge <= 0.0)
    {
        return 1.0;
    }
    uint picture = asuint(shade.y);
    float2 texel = (float2(off.x, -off.y) * 0.5 + 0.5) * shade.z - 0.5;
    float2 lo = floor(texel);
    float2 f = texel - lo;
    int2 top = int2(int(shade.z) - 1, int(shade.z) - 1);
    float a0 = textures[NonUniformResourceIndex(picture)].Load(int3(clamp(int2(lo), int2(0, 0), top), 0)).r;
    float a1 = textures[NonUniformResourceIndex(picture)].Load(int3(clamp(int2(lo) + int2(1, 0), int2(0, 0), top), 0)).r;
    float a2 = textures[NonUniformResourceIndex(picture)].Load(int3(clamp(int2(lo) + int2(0, 1), int2(0, 0), top), 0)).r;
    float a3 = textures[NonUniformResourceIndex(picture)].Load(int3(clamp(int2(lo) + int2(1, 1), int2(0, 0), top), 0)).r;
    return 1.0 - shade.x * edge * (1.0 - lerp(lerp(a0, a1, f.x), lerp(a2, a3, f.x), f.y));
}
