// How a pixel's light is put together from its retail terms and the modern passes' masks, and
// the color a surface leaves in the scene's target.
//
// The resolve works out each term by retail's class rules and hands them here with the masks:
// the sun's visibility from the shadows, the ambient's occlusion, and the lamps' light. With each
// mask at its identity (fully lit, unoccluded, no lamps), the composition is retail's own light,
// exactly as the per-pixel resolve has it: saturated, and for the ground truncated to bytes.
//
// A frame is drawn in display values, as retail draws it, or linear (`FRAME_POST`): then the sun
// and the lamps light past 1 while retail's own lights stay clamped as retail clamps them (their
// fall-off was made for the clamp, and near a light it would sum to many times white), nothing is
// truncated, the colors are taken from display values to linear light after the fog and the
// screen ramp, and the post passes bring them back to the display.
module intent;

import retail;

// The row of the frame's constants that says how the scene's colors are drawn: x 1 linear and
// unclamped, 0 in display values; y 1 while an art pack's light is laid on what stands outdoors.
public static const uint FRAME_POST = 21;
// The kind a material of plants carries, as the G-buffer holds it.
public static const uint KIND_FOLIAGE = 1;

// Whether frame `frame` is drawn linear.
public bool linear_frame(uint frame)
{
    return row(frame, FRAME_POST).x > 0.5;
}

// Whether frame `frame` lays an art pack's light on what stands outdoors.
public bool pack_lit_frame(uint frame)
{
    return row(frame, FRAME_POST).x > 0.5 && row(frame, FRAME_POST).y > 0.5;
}

// Whether frame `frame` draws the air over the land in place of retail's fog on the opaque world.
public bool air_frame(uint frame)
{
    return row(frame, FRAME_POST).z > 0.5;
}

// A display value taken to linear light: the sRGB curve, continued above 1.
public float3 to_linear(float3 c)
{
    float3 low = c / 12.92;
    float3 high = pow((max(c, 0.0) + 0.055) / 1.055, 2.4);
    return float3(c.x <= 0.04045 ? low.x : high.x, c.y <= 0.04045 ? low.y : high.y,
                  c.z <= 0.04045 ? low.z : high.z);
}

// A pixel's light, term by term, by its class's rules.
public struct Terms
{
    // The light class.
    public uint cls;
    // The emissive: an object's own luminosity.
    public float3 emissive;
    // A room's burn of the static lights, baked at its vertices as retail bakes it; 0 for
    // everything else. The lamps replace it with the fixed-function lights' share.
    public float3 burn;
    // The ambient its class takes.
    public float3 ambient;
    // The sun's color (for objects, times its brightness), and how squarely the surface faces it.
    public float3 sun_color;
    public float sun_facing;
    // The two together, as retail works the sun's share out.
    public float3 sun;
    // The fixed-function lights' share: indoor objects' light sets, rooms' dynamic lights.
    public float3 lights;
};

// What the modern passes say of a pixel.
public struct Masks
{
    // How much of the sun reaches it: 1 in full sun, 0 in shadow.
    public float sun_visibility;
    // How much of the ambient reaches it: 1 unoccluded.
    public float ambient_visibility;
    // Light added by the lamps, and 1 where they replace the fixed-function lights' share (0
    // where retail's own lights stay).
    public float3 lamps;
    public float lamps_replace;
    // How much the pixel glows for standing right at a lamp, its flame or its glass.
    public float lamp_glow;
    // Bounced light, added: never below nought, so no pixel comes out darker for it. The ambient
    // occlusion takes it as it takes the ambient.
    public float3 gi;
    // Whether the frame is drawn linear: the sun and the lamps then light past 1, and nothing is
    // truncated.
    public bool linear;
    // Where an art pack's light is laid: what retail's sun and ambient are multiplied by.
    public bool pack_lit;
    public float3 pack_light;
};

// The masks of a pixel nothing shadows, occludes or lights besides retail's own light.
public Masks unmasked()
{
    Masks m;
    m.sun_visibility = 1.0;
    m.ambient_visibility = 1.0;
    m.lamps = float3(0.0, 0.0, 0.0);
    m.lamps_replace = 0.0;
    m.lamp_glow = 0.0;
    m.gi = float3(0.0, 0.0, 0.0);
    m.linear = false;
    m.pack_lit = false;
    m.pack_light = float3(0.0, 0.0, 0.0);
    return m;
}

// [`unmasked`] for a pixel of frame `frame`, linear when the frame is.
public Masks unmasked(uint frame)
{
    Masks m = unmasked();
    m.linear = linear_frame(frame);
    return m;
}

// Whether the masks are at their identity: nothing shadowed, occluded or added.
public bool is_unmasked(Masks m)
{
    return m.sun_visibility == 1.0 && m.ambient_visibility == 1.0 && m.lamps_replace == 0.0
           && m.lamp_glow == 0.0 && all(m.lamps == 0.0) && all(m.gi == 0.0) && !m.linear;
}

// An art pack's light, over retail's sun and ambient: worked out in linear light against what
// retail's would be at the same facing, and laid on as a share of it. The sun is as strong as brings
// a surface square to it to retail's brightness, the open sky counted in. A surface turned away from
// the sun takes the open sky, where retail's falls straight to its ambient. What retail lights more
// than the ground, as it lights what stands on it, keeps that.
// How much brighter what stands on the ground comes out than the ground, in linear light.
static const float PACK_OBJECT = 1.5;
// The most the sun is strengthened past retail's, and the most the light is taken past retail's at
// a pixel, where a face turned from the sun takes the open sky.
static const float PACK_MOST = 4.0;
static const float PACK_MOST_RATIO = 16.0;
// The sun's light the open sky holds, as a share of the beam, and its color; the share the ground
// far below holds, and its color; and what of the beam fills a surface it misses.
static const float PACK_SKY_SHARE = 0.6;
static const float3 PACK_SKY_TINT = float3(0.62, 0.80, 1.0);
static const float PACK_GROUND_SHARE = 0.16;
static const float3 PACK_GROUND_TINT = float3(0.85, 1.0, 0.62);
static const float PACK_SHADE_FILL = 0.18;
// What of the sun a leaf keeps in its crown's shade, on the crown's outside, and how much of that
// deep inside; and how much of the sky a leaf deep inside still sees.
static const float LEAF_SCATTER = 0.4;
static const float LEAF_SCATTER_DEEP = 0.5;
static const float LEAF_INNER_SKY = 0.15;
// How much a crown's shaded leaves are darkened against its lit ones.
static const float LEAF_DEPTH = 0.6;
// How much of the sun's light a leaf takes square on.
static const float LEAF_SUN = 0.8;
// The most a leaf is taken past retail's brightness by day.
static const float LEAF_MOST = 1.6;

// What retail's light is multiplied by, as a display value, at a surface of normal `n`,
// `sun_visibility` and `ambient_visibility` of the sun and the sky reaching it, `deep` how deep in
// its crown a leaf lies, `to_eye` the way to the eye. A leaf takes the sky from all round and the
// sun softly on both faces, some of it in the shade of its own crown, more where it sees more of
// the sky; it takes the light's brightness and little of its color, within a limit by day; and it
// lets the sun through when looked at toward it.
public float3 pack_light(uint frame, float3 n, float sun_visibility, float ambient_visibility, bool object, bool leaf, float deep, float3 to_eye)
{
    static const float3 WEIGHTS = float3(0.2126, 0.7152, 0.0722);
    float3 toward = row(frame, FRAME_SUN).xyz;
    float brightness = length(toward);
    float3 facing_sun = brightness > 1e-4 ? toward / brightness : float3(0.0, 0.0, 1.0);
    float3 sun_display = row(frame, FRAME_SUN_COLOR).rgb * brightness;
    float3 sun = pow(max(sun_display, 0.0), 2.2);
    float3 sky_display = row(frame, FRAME_GROUND_AMBIENT).rgb;
    float3 amb = pow(max(sky_display, 0.0), 2.2);
    float raw = dot(n, facing_sun);
    // The open sky holds a share of the sun's light, blue, and more the higher the sun; the ground
    // far below it a little, green. That sky is what lights a face the sun misses, so shallow folds
    // in the ground do not read as dark streaks.
    float elevation = saturate(facing_sun.z);
    float open_share = PACK_SKY_SHARE * (0.25 + 0.75 * elevation);
    float3 open_sky = sun * open_share * PACK_SKY_TINT;
    float3 open_ground = sun * elevation * PACK_GROUND_SHARE * PACK_GROUND_TINT;
    float strength = clamp((dot(pow(max(sky_display + sun_display, 0.0), 2.2), WEIGHTS) - dot(amb, WEIGHTS))
                               / max(dot(sun + open_sky, WEIGHTS), 1e-4),
                           0.0, PACK_MOST);
    float3 sky = amb * float3(0.89, 1.0, 1.21) + open_sky * strength;
    float3 below = amb * 0.45 + sun * float3(0.10, 0.08, 0.05) + open_ground * strength;
    // Retail's own light here, which the result multiplies: what stands on the ground has its own
    // ambient.
    float3 own_ambient = object ? row(frame, FRAME_OBJECT_AMBIENT).rgb : sky_display;
    float3 retail = max(pow(saturate(own_ambient + sun_display * max(raw, 0.0)), 2.2), 1e-4);
    float3 ratio;
    if (leaf)
    {
        float open = saturate(1.0 - deep);
        float kept = LEAF_SCATTER * lerp(LEAF_SCATTER_DEEP, 1.0, open);
        float shade = kept + (1.0 - kept) * sun_visibility;
        float wrap = 0.3 + 0.7 * saturate((raw + 0.5) / 1.5);
        // The leaves round a leaf hide the open sky's share of the sun, so its sky is the ambient.
        float3 leaf_below = amb * 0.45 + sun * float3(0.10, 0.08, 0.05);
        float3 hemi = lerp(leaf_below, amb * float3(0.89, 1.0, 1.21), 0.5 + 0.3 * n.z) * lerp(LEAF_INNER_SKY, 1.0, open) * ambient_visibility;
        float leaf_strength = clamp((dot(pow(max(sky_display + sun_display, 0.0), 2.2), WEIGHTS) - dot(amb, WEIGHTS))
                                        / max(dot(sun, WEIGHTS), 1e-4),
                                    0.0, PACK_MOST);
        float3 l = hemi + sun * leaf_strength * LEAF_SUN * wrap * shade;
        // A crown keeps its depth: the less light a leaf has against the fullest, the less of it
        // shows, so what lies in shade reads dark among what the sun finds.
        float fullest = dot(pow(max(sky_display + sun_display, 0.0), 2.2), WEIGHTS);
        l *= pow(saturate(dot(l, WEIGHTS) / max(fullest, 1e-4)), LEAF_DEPTH);
        float day = smoothstep(0.0, 0.35, facing_sun.z) * (brightness > 1e-4 ? 1.0 : 0.0);
        float share = min(dot(l, WEIGHTS) / max(dot(retail, WEIGHTS), 1e-4), lerp(1.0, LEAF_MOST, day));
        ratio = share * clamp(l / retail / max(share, 1e-4), 0.8, 1.25);
        // Through the leaf: a soft glow and a bright forward lobe, on the crown's thin outside.
        float forward = saturate(dot(-normalize(to_eye), facing_sun));
        ratio += sun * leaf_strength * (forward * forward * 0.45 + pow(forward, 5.0) * 1.6) * shade * open * open * day / retail;
    }
    else
    {
        // Nothing of the sun past the facing's edge: a body's far side is all in its own shadow, so
        // any there would end in a hard line at the edge of that shadow, down the body.
        float facing = saturate(raw);
        float up = n.z * 0.5 + 0.5;
        float fill = PACK_SHADE_FILL * (0.5 + 0.5 * up) * (1.0 - facing * sun_visibility);
        ratio = (lerp(below, sky, up) * ambient_visibility + sun * strength * (facing * sun_visibility + fill)) / retail;
        ratio *= object ? PACK_OBJECT : 1.0;
    }
    return pow(clamp(ratio, 0.0, PACK_MOST_RATIO), 1.0 / 2.2);
}

// How much of the bounced light is added to a room and what stands in it.
static const float INDOOR_BOUNCE = 0.3;

// How much a lamp's own flame or glass is brightened at its most, over the light it takes; and how
// near white the lamp's color goes there, as a flame's heart is.
static const float LAMP_GLOW = 2.5;
static const float LAMP_WHITE = 0.75;

// The lamps' light at a pixel that glows `glow`.
float3 whitened(float3 lamps, float glow)
{
    return lerp(lamps, float3(max(lamps.r, max(lamps.g, lamps.b))), glow * LAMP_WHITE);
}

// The lamps' light as it goes into the linear frame: faint light as it is, strong light held to
// under twice what retail's clamped light reaches, so that a wall by a torch glows and keeps its
// color rather than going white.
float3 lamps_held(float3 lamps)
{
    return lamps / (1.0 + 0.5 * max(lamps.r, max(lamps.g, lamps.b)));
}

// The light the pixel takes. With the masks at their identity it is retail's own sum, written as
// retail's is, so the arithmetic is the same to the last bit.
public float3 compose(Terms t, Masks m)
{
    if (is_unmasked(m))
    {
        if (t.cls == CLASS_GROUND)
        {
            float3 g = min(t.sun_color * t.sun_facing + t.ambient, 1.0);
            return floor(g * 255.0) / 255.0;
        }
        if (t.cls == CLASS_CELL)
        {
            return saturate(t.burn + t.ambient + t.lights);
        }
        if (t.cls == CLASS_INDOOR)
        {
            return saturate(t.emissive + t.ambient + t.lights);
        }
        return saturate(t.emissive + t.ambient + t.sun);
    }
    float facing = t.sun_facing * m.sun_visibility;
    float3 gi = max(m.gi, 0.0) * m.ambient_visibility;
    // A room's own light was made with what it bounces already in it: only a little more is added.
    if (t.cls == CLASS_CELL || t.cls == CLASS_INDOOR)
    {
        gi *= INDOOR_BOUNCE;
    }
    float glow = 1.0 + m.lamp_glow * LAMP_GLOW;
    if (t.cls == CLASS_GROUND)
    {
        float3 c = (m.pack_lit ? (t.sun_color * t.sun_facing + t.ambient) * m.pack_light
                               : t.sun_color * facing + t.ambient * m.ambient_visibility)
                   + gi;
        if (m.linear)
        {
            return (c + whitened(lamps_held(m.lamps), m.lamp_glow)) * glow;
        }
        c = min((c + m.lamps) * glow, 1.0);
        return floor(c * 255.0) / 255.0;
    }
    if (m.pack_lit)
    {
        return ((saturate(t.emissive + t.ambient) + t.sun_color * t.sun_facing) * m.pack_light
                + whitened(lamps_held(m.lamps), m.lamp_glow) + gi)
               * glow;
    }
    if (m.linear)
    {
        return (saturate(t.emissive + t.ambient * m.ambient_visibility
                         + (t.burn + t.lights) * (1.0 - m.lamps_replace))
                + t.sun_color * facing + whitened(lamps_held(m.lamps), m.lamp_glow) + gi)
               * glow;
    }
    return saturate((t.emissive + t.ambient * m.ambient_visibility + t.sun_color * facing
                     + (t.burn + t.lights) * (1.0 - m.lamps_replace) + m.lamps + gi)
                    * glow);
}

// The color the resolve leaves for a surface of own color `albedo` under light `light`, fogged by
// `fog` (1 no fog, 0 fully fogged), with the screen ramp: in display values, clamped, as retail
// draws it; or linear and unclamped.
public float4 resolved(uint frame, float3 albedo, float3 light, float fog)
{
    if (!linear_frame(frame))
    {
        float3 c = saturate(albedo * light);
        float3 fogged = lerp(row(frame, FRAME_FOG_COLOR).rgb, c, fog);
        return float4(saturate(fogged * gamma_scale(frame)), 1.0);
    }
    float3 c = albedo * light;
    float3 fogged = lerp(row(frame, FRAME_FOG_COLOR).rgb, c, fog);
    return float4(to_linear(max(fogged * gamma_scale(frame), 0.0)), 1.0);
}

// [`resolved`], fogged instead from the fog's volume: `f.a` is what the fog in front of the
// surface keeps of it and `f.rgb` the light the fog adds.
public float4 resolved_through(uint frame, float3 albedo, float3 light, float4 f)
{
    if (!linear_frame(frame))
    {
        float3 c = saturate(albedo * light);
        return float4(saturate((c * f.a + f.rgb) * gamma_scale(frame)), 1.0);
    }
    float3 c = albedo * light;
    return float4(to_linear(max((c * f.a + f.rgb) * gamma_scale(frame), 0.0)), 1.0);
}

// The color a blended surface (translucent, additive, a cut-out's edge, the sky) leaves: its own
// color times its light `c`, fogged by `fog`, with the screen ramp, at alpha `alpha`; in display
// values or linear as [`resolved`].
public float4 blended(uint frame, float3 c, float fog, float alpha)
{
    if (!linear_frame(frame))
    {
        float3 fogged = lerp(row(frame, FRAME_FOG_COLOR).rgb, c, fog);
        return float4(saturate(fogged * gamma_scale(frame)), alpha);
    }
    float3 fogged = lerp(row(frame, FRAME_FOG_COLOR).rgb, c, fog);
    return float4(to_linear(max(fogged * gamma_scale(frame), 0.0)), alpha);
}
