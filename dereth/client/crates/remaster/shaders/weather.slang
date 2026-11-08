// The weather as the world's surfaces take it: how wet a surface out in the rain is, and how much
// of the flat ground is puddle; and what stands overhead, which keeps both off what stands under a
// roof. The G-buffer's shaders call it, and so do the rain's and the splashes'.
//
// - **Wet.** A surface near the viewer, out in the rain and lit by the sun (the ground and what
//   stands outdoors), is darkened and deepened in color as soaked material is, and its roughness
//   lowered; what faces the sky the most. Rooms and what stands in them never are.
// - **Puddles.** Flat ground gathers them where its terrain takes them (dirt and roads the most,
//   grass a few; rock, sand, snow and water none), from a mask fixed to the world, so a puddle
//   lies in the same place near and far and frame after frame.
// - **Shelter.** A depth of the resident statics seen from straight above, round the eye, says
//   where something stands overhead; its edge is softened by a patch whose middle wanders with
//   the world's noise.
module weather;

import bindless;
import retail;

// The weather's constants, in buffer `weather` as float4 rows:
//   0  x how wet what stands out in the rain is (0 dry, 1 soaked); y how much the ground
//      gathers puddles (0 none, 1 in full); z the weather's clock, seconds within its period; w
//      how much of what falls is snow
//   1  x, y where the frame's origin stands in the world, meters; z, w where the wet starts to
//      fade with distance and where it is gone, meters
//   2  the overhead map: x, y its centre (the frame's space), z how far it reaches, w its side,
//      texels
//   3  x the height its depth starts at, y the depth's span, z its index in the table, w 1 when
//      it holds anything
//   4  how far the rain (x) and the snow (y) have fallen, and how far the wind has carried both
//      east (z) and north (w), meters, each wrapped to the lattices' period
//   5  the rain's speed of fall (x) and the snow's (y), and the wind east (z) and north (w),
//      meters a second
//   6  x, y where the frame's origin stands in the world wrapped to the lattices' period; z that
//      period, meters; w the clock's period, seconds
//   7  where each of the rain's three lattices ends among the instances drawn (x, y, z); w how
//      much falls, 0 to 1
//   8  where each of the snow's ends, counted on from the rain's last (x, y, z); w radians a
//      pixel
public static const uint W_STATE = 0;
public static const uint W_WORLD = 1;
public static const uint W_COVER = 2;
public static const uint W_COVER_DEPTH = 3;
public static const uint W_FALL = 4;
public static const uint W_MOTION = 5;
public static const uint W_LATTICE = 6;
public static const uint W_RAIN = 7;
public static const uint W_SNOW = 8;

// No weather: the push word of a frame without it.
public static const uint NO_WEATHER = 0xFFFFFFFF;

// The steps the puddle's share is written in: the ground's material target's fourth byte, where
// its kind is not water.
public static const float PUDDLE_STEPS = 255.0;

// --- noise --------------------------------------------------------------------------------------

// A hash of lattice cell `c` with `salt`, 0 to 1.
float hash(int2 c, uint salt)
{
    uint h = uint(c.x) * 0x8DA6B343u ^ uint(c.y) * 0xD8163841u ^ salt * 0x9E3779B9u;
    h ^= h >> 13;
    h *= 0x85EBCA6Bu;
    h ^= h >> 16;
    return float(h & 0xFFFFFFu) / 16777216.0;
}

// Smooth noise of the world's plane at `p` (meters), `s` lattice cells a meter, 0 to 1.
public float noise(float2 p, float s, uint salt)
{
    float2 x = p * s;
    float2 c = floor(x);
    float2 f = x - c;
    float2 u = f * f * (3.0 - 2.0 * f);
    int2 i = int2(c);
    return lerp(lerp(hash(i, salt), hash(i + int2(1, 0), salt), u.x),
                lerp(hash(i + int2(0, 1), salt), hash(i + int2(1, 1), salt), u.x), u.y);
}

// Four octaves of it, from `s` cells a meter up.
float fbm(float2 p, float s, uint salt)
{
    float sum = 0.0;
    float a = 0.5;
    float k = s;
    for (uint i = 0; i < 4; i++)
    {
        sum += a * noise(p, k, salt + i * 101u);
        k *= 2.0;
        a *= 0.5;
    }
    return sum;
}

// --- where things are ---------------------------------------------------------------------------

// The world's place of `xy`, the frame's space.
public float2 world_xy(uint weather, float2 xy)
{
    return xy + row(weather, W_WORLD).xy;
}

// The frame's space place of a pixel at `position` (its window position and depth), through the
// frame's constants `frame`.
public float3 frame_point(uint frame, float4 position)
{
    float2 e = row(frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / e.x * 2.0 - 1.0, 1.0 - position.y / e.y * 2.0);
    float4 h = transform(frame, FRAME_INV_VIEW_PROJ, float4(ndc, position.z, 1.0));
    return h.xyz / h.w;
}

// --- what stands overhead -----------------------------------------------------------------------

// The height of the highest static surface over `xy` (the frame's space), or far below anything
// where nothing stands there or the map does not reach.
public float shelter_height(uint weather, float2 xy)
{
    float4 c = row(weather, W_COVER);
    float4 s = row(weather, W_COVER_DEPTH);
    if (s.w < 0.5)
    {
        return -1.0e9;
    }
    float2 q = (xy - c.xy) / c.z;
    if (abs(q.x) >= 1.0 || abs(q.y) >= 1.0)
    {
        return -1.0e9;
    }
    int side = int(c.w);
    float2 uv = float2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    int2 t = clamp(int2(uv * c.w), int2(0, 0), int2(side - 1, side - 1));
    float d = textures[NonUniformResourceIndex(asuint(s.z))].Load(int3(t, 0)).x;
    if (d >= 1.0)
    {
        return -1.0e9;
    }
    return s.x - d * s.y;
}

// Whether something stands more than `margin` meters over `p`: 1 sheltered, 0 open.
public float sheltered(uint weather, float3 p, float margin)
{
    return shelter_height(weather, p.xy) > p.z + margin ? 1.0 : 0.0;
}

// The share of the ground round `p` that something shelters, over a patch half a meter across
// whose middle wanders with the world's noise, so the dry ground under a roof's edge ends in a
// soft, uneven line rather than in the map's texels.
public float shelter_share(uint weather, float3 p, float margin)
{
    float2 wp = world_xy(weather, p.xy);
    float2 wander = float2(noise(wp, 1.3, 71u), noise(wp, 1.3, 73u)) - 0.5;
    float2 centre = p.xy + wander * 0.7;
    float n = 0.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 o = float2(float(i), float(j)) * 0.25;
            n += sheltered(weather, float3(centre + o, p.z), margin);
        }
    }
    return smoothstep(0.0, 1.0, n / 9.0);
}

// --- what the surfaces take ---------------------------------------------------------------------

// How much of the ground at `p` (the frame's space; `n` its normal before any picture bends it)
// is puddle, 0 to 1, where its layers take puddles by `weight` (0 none, 1 a road's): flat ground
// only, open to the sky, from a mask fixed to the world. 0 without the weather's constants, or
// where the ground gathers no puddles.
public float puddle_share(uint weather, float3 p, float3 n, float weight)
{
    if (weather == NO_WEATHER || weight <= 0.0)
    {
        return 0.0;
    }
    float on = row(weather, W_STATE).y;
    float level = smoothstep(0.955, 0.99, n.z);
    if (on <= 0.0 || level <= 0.0)
    {
        return 0.0;
    }
    float2 wp = world_xy(weather, p.xy);
    float m = fbm(wp, 0.16, 1u) * 0.8 + fbm(wp + float2(3.1, 7.7), 0.7, 2u) * 0.2;
    // Roads gather the most, grass the fewest.
    float lip = lerp(0.82, 0.44, saturate(weight));
    float share = smoothstep(lip, lip + 0.07, m) * level * smoothstep(0.0, 0.25, weight) * on;
    if (share <= 0.0)
    {
        return 0.0;
    }
    return share * (1.0 - shelter_share(weather, p, 0.4));
}

// How readily ground of landscape vertex word `word` gathers puddles, 0 to 1: a road the most,
// dirt and marsh less, grass a little, and rock, sand, snow, ice and water never. The same table
// as the splats' layers carry.
public float puddle_weight(uint word)
{
    if ((word & 3) != 0)
    {
        return 1.0;
    }
    uint t = (word >> 2) & 0x1F;
    if (t == 4 || t == 5 || t == 7 || t == 8)
    {
        return 2.0 / 3.0;
    }
    if (t == 1 || t == 3 || t == 9)
    {
        return 1.0 / 3.0;
    }
    return 0.0;
}

// The fourth byte of the ground's material at a pixel that is not water: its puddle share.
public uint puddle_byte(float share)
{
    return uint(saturate(share) * PUDDLE_STEPS + 0.5);
}

// `albedo` (its color, and its roughness in `a`) as a surface of light class `cls` at
// `position` (its window position and depth, through the frame's constants `frame`) facing `n`
// takes the rain: darkened and deepened in color as soaked material is and its roughness lowered,
// the more the more it faces the sky, fading with distance, and not at all under a roof, indoors
// or where it is `water` (0 to 1) already. Unchanged without the weather's constants.
public float4 wetted(uint weather, uint frame, float4 position, uint cls, float3 n, float water, float4 albedo)
{
    if (weather == NO_WEATHER || (cls != CLASS_GROUND && cls != CLASS_SUN))
    {
        return albedo;
    }
    float soak = row(weather, W_STATE).x;
    if (soak <= 0.0 || water >= 1.0)
    {
        return albedo;
    }
    float3 p = frame_point(frame, position);
    float4 w = row(weather, W_WORLD);
    float dist = length(p - row(frame, FRAME_EYE).xyz);
    float reach = 1.0 - smoothstep(w.z, w.w, dist);
    if (reach <= 0.0)
    {
        return albedo;
    }
    float up = smoothstep(0.3, 0.85, n.z);
    float open = 1.0 - shelter_share(weather, p, 0.4);
    float wet = lerp(0.45, 1.0, up) * reach * open * soak * (1.0 - water);
    if (wet <= 0.0)
    {
        return albedo;
    }
    float l = dot(albedo.rgb, float3(0.2126, 0.7152, 0.0722));
    float3 c = max(lerp(float3(l, l, l), albedo.rgb, 1.0 + 0.3 * wet), 0.0);
    c *= lerp(1.0, 0.58, wet);
    float roughness = lerp(albedo.a, min(albedo.a, 0.35), wet * up);
    return float4(c, roughness);
}
