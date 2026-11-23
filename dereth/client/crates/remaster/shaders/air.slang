// With an art pack, the air between the eye and the land, in place of the fog: what the world drew
// is dimmed by the air in front of it and lit by the light that air scatters toward the eye. Two
// layers act on every pixel of land. The atmosphere's own air tints far land toward the sky's color
// at the horizon in that direction. The low haze fades the land into its own light, the light the
// veil over the horizon is drawn in, and lies thickest over low ground. Near where the land ends it
// fades a little further, so the end of the world is softened the same way wherever the eye stands.
// A low fog may lie over the ground, thinning with height over each place's own ground and thicker
// over low ground, lit by the sky as the haze is and by the sun where the sun reaches it.
import atmosphere;
import bindless;
import intent;
import lod;
import retail;
import world;

struct Push
{
    uint frame;
    uint atmosphere;
    uint sky_view;
    uint sampler;
    // The world as it was drawn, its depth and its material.
    uint color;
    uint depth;
    uint material;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// How far into the air a point `dist` metres away lies: nothing short of where the haze begins, then
// easing in, so the air takes hold gradually and the near field is barely touched.
float into_air(float dist)
{
    float x = max(dist - row(push.atmosphere, ATM_HAZE_AT).x, 0.0);
    float soft = row(push.atmosphere, ATM_HAZE_LOOK).w;
    return x < soft ? x * x / (2.0 * soft) : x - 0.5 * soft;
}

// What of a point `dist` metres away and `rise` metres higher comes through the haze, each channel:
// its depth grows faster than the distance, so the near and middle ground stay crisp, and it leans
// toward the air's own blue, so far land keeps its red longest.
float3 through_haze(float dist, float rise)
{
    uint a = push.atmosphere;
    float x = rise / row(a, ATM_HAZE_AT).y;
    float density = abs(x) > 1e-3 ? (1.0 - exp(-x)) / x : 1.0;
    float4 shape = row(a, ATM_HAZE_SHAPE);
    density = clamp(density, shape.x, shape.y);
    float4 tint = row(a, ATM_HAZE_TINT);
    float4 air = row(a, ATM_RAYLEIGH);
    float3 lean = lerp(float3(1.0, 1.0, 1.0), air.rgb / air.g, tint.x);
    float3 weights = lean / luma(lean);
    float depth = row(a, ATM_HAZE).w * tint.z * pow(into_air(dist) / tint.z, tint.y);
    return exp(-depth * density * weights);
}

// The landscape's height at `xy`, the frame's space, between its vertices.
float ground_at(float2 xy)
{
    float4 g = row(push.atmosphere, ATM_LOW_FOG_GROUND);
    uint heights = asuint(g.z);
    float2 q = (xy + g.xy) / world_spacing(heights);
    int2 c = int2(floor(q));
    float2 f = q - floor(q);
    float h[4];
    int2 offs[4] = { int2(0, 0), int2(1, 0), int2(0, 1), int2(1, 1) };
    for (int k = 0; k < 4; k++)
    {
        h[k] = world_height(heights, c + offs[k]);
    }
    return lerp(lerp(h[0], h[1], f.x), lerp(h[2], h[3], f.x), f.y);
}

// How far along the line from the eye the low fog lies, metres: past it the air's own haze takes
// over.
static const float LOW_FOG_REACH = 1500.0;

// How far into the low fog the eye sees a point `dist` metres away, along a line that stands at
// height `z` over its ground `gz` where the fog along it ends (the point itself, when it is nearer
// than that), from the eye at `eye_z` over its ground `ge`: its extinction integrated along the
// line, the fog thinning by e every `height` metres over the ground under it, and thicker the lower
// that ground lies toward the lowest round the eye. Nothing within the first 25 metres from the eye,
// and nothing past `LOW_FOG_REACH`.
float low_fog_depth(float4 fog, float dist, float eye_z, float ge, float z, float gz)
{
    float he = clamp((eye_z - ge) / fog.y, -3.0, 40.0);
    float hp = clamp((z - gz) / fog.y, -3.0, 40.0);
    float dz = hp - he;
    float shape = abs(dz) > 1e-3 ? (exp(-he) - exp(-hp)) / dz : exp(-he);
    float low = exp(-max(0.5 * (ge + gz) - fog.z, 0.0) / (fog.y * 6.0));
    return fog.x * clamp(dist - 25.0, 0.0, LOW_FOG_REACH) * shape * low;
}

// The shadow cascades' rows, as the sun's visibility pass reads them.
static const uint CASCADE_FAR = 16;
static const uint CASCADE_MAPS = 18;
static const uint CASCADE_FADE = 19;

// How many looks into the sun's shadows the low fog takes along the line to a point.
static const uint SHAFT_LOOKS = 12;

// How far along its step each of the low fog's looks into the shadows lies at pixel `p`, 0 to 1:
// the fades' screen pattern, turned each frame as the trees' pictures' patterns are while frames
// are laid over each other, so that between them the frames look all along each step.
float shaft_jitter(int2 p)
{
    float4 turns = row(push.frame, FRAME_DITHER);
    float t = fade_threshold(p);
    return turns.z > 0.5 ? frac(t + turns.x) : t;
}

// How much of the light the low fog along the line from the eye toward `dir` sends toward the eye
// comes from fog the sun reaches: 1 where nothing shadows it. The line runs `len` metres to where
// it is measured, `forward` metres along the view for each metre along it, from `he` to `hp` of
// the fog's heights over its ground as `low_fog_depth` has it, through fog of extinction `density`
// per metre on the ground; `kept` is what all of the fog along it lets through.
// The shadows are looked into at even steps out to where they reach, each look `jitter` of the
// way along its step, so a shadow's edge spreads over a step instead of standing in the fog as a
// sharp copy of the shadow. Each look counts for what its step of fog sends toward the eye through
// the fog in front of it. The fog past the shadows' reach counts as sunlit, as the land there is,
// and the shadows fade out toward their reach and in as the sun rises, as the land's do.
float shafts(uint cascades, float3 eye, float3 dir, float forward, float len, float he, float hp,
             float density, float kept, float jitter)
{
    float4 far = row(cascades, CASCADE_FAR);
    uint4 maps = row_u(cascades, CASCADE_MAPS);
    float4 fade = row(cascades, CASCADE_FADE);
    int size = int(fade.w);
    float reach = min(len, fade.y / max(forward, 1e-3));
    float step = reach / float(SHAFT_LOOKS);
    // Each cascade's place for the eye, and how far it moves for each metre along the line.
    float4 from[4];
    float4 along[4];
    for (uint c = 0; c < 4; c++)
    {
        from[c] = transform(cascades, c * 4, float4(eye, 1.0));
        along[c] = transform(cascades, c * 4, float4(dir, 0.0));
    }
    float depth = 0.0;
    float sunlit = 0.0;
    float sent = 0.0;
    for (uint k = 0; k < SHAFT_LOOKS; k++)
    {
        float t = (float(k) + jitter) * step;
        float h = clamp(lerp(he, hp, saturate(t / max(len, 1e-3))), -3.0, 40.0);
        float next = depth + (t > 25.0 ? density * exp(-h) * step : 0.0);
        float w = exp(-depth) - exp(-next);
        depth = next;
        float ahead = t * forward;
        float lit = 1.0;
        for (uint c = 0; c < 4; c++)
        {
            if (ahead > far[c])
            {
                continue;
            }
            float4 q = from[c] + along[c] * t;
            float2 uv = float2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
            if (any(uv < 0.0) || any(uv > 1.0) || q.z < 0.0 || q.z > 1.0)
            {
                continue;
            }
            int2 cell = clamp(int2(uv * float(size)), int2(0, 0), int2(size - 1, size - 1));
            float d = textures[NonUniformResourceIndex(maps[c])].Load(int3(cell, 0)).x;
            lit = q.z <= d + 1e-4 ? 1.0 : 0.0;
            break;
        }
        lit = lerp(lit, 1.0, saturate((ahead - fade.x) / max(fade.y - fade.x, 1e-3)));
        sunlit += w * lit;
        sent += w;
    }
    float beyond = max(exp(-depth) - kept, 0.0);
    float share = sent + beyond > 1e-6 ? (sunlit + beyond) / (sent + beyond) : 1.0;
    return lerp(1.0, share, fade.z);
}

// 1 well inside where the land ends, falling toward it to leave a share, by how far along the way
// from the eye to its end the point lies across the ground.
float land_fade(float3 p)
{
    uint a = push.atmosphere;
    float4 w = row(a, ATM_LAND);
    if (w.z <= w.x || w.w <= w.y)
    {
        return 1.0;
    }
    float2 e = row(a, ATM_EYE).xy;
    float2 across = p.xy - e;
    float len = length(across);
    if (len < 1.0)
    {
        return 1.0;
    }
    float2 dir = across / len;
    float tx = dir.x > 1e-5 ? (w.z - e.x) / dir.x : (dir.x < -1e-5 ? (w.x - e.x) / dir.x : 1e9);
    float ty = dir.y > 1e-5 ? (w.w - e.y) / dir.y : (dir.y < -1e-5 ? (w.y - e.y) / dir.y : 1e9);
    float end = max(min(tx, ty), 1.0);
    return 1.0 - row(a, ATM_HAZE_LOOK).y * smoothstep(row(a, ATM_AIR).w, 1.0, len / end);
}

[shader("fragment")]
float4 fs_air(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    float4 c = textures[NonUniformResourceIndex(push.color)].Load(int3(p, 0));
    uint cls = uint(textures[NonUniformResourceIndex(push.material)].Load(int3(p, 0)).x * 255.0 + 0.5);
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        // The sky, which the veil has already covered.
        return c;
    }
    uint a = push.atmosphere;
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    float d = textures[NonUniformResourceIndex(push.depth)].Load(int3(p, 0)).x;
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    float3 world = h.xyz / h.w;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float dist = length(world - eye);
    float3 dir = view_ray(a, ndc);
    // The atmosphere's air, at the eye's height, drawn thicker than the real air, from where the
    // haze begins.
    float4 thick = row(a, ATM_AIR);
    float3 through = lerp(float3(1.0, 1.0, 1.0),
                          exp(-medium(a, row(a, ATM_EYE).w).extinction * (into_air(dist) * 0.001 * thick.y)),
                          thick.x);
    float3 aerial = c.rgb * through
                    + horizon_sky(a, push.sky_view, push.sampler, dir) * (float3(1.0, 1.0, 1.0) - through);
    float3 haze = haze_light(a, push.sky_view, push.sampler, dir,
                             1.0 - exp(-dist / row(a, ATM_HAZE_SHAPE).w));
    float3 kept = through_haze(dist, world.z - eye.z);
    float3 seen = aerial * kept + haze * (float3(1.0, 1.0, 1.0) - kept);
    float4 low = row(a, ATM_LOW_FOG);
    if (low.w > 0.5)
    {
        // The low fog: the haze's own light toward the eye, all of its glow toward the sun built up,
        // a little brighter for the sun lighting it from above; dimmer for the share of it the
        // sun's shadows keep from the sun.
        // The line is measured where the fog along it ends. A point farther than that says nothing
        // of the fog, and how high it stands over its own ground is not known there: the depth
        // places a far point only to within tens of metres, differently as the view moves by a
        // hair, and the fog thins by e every few metres of that height.
        float3 end = dist > LOW_FOG_REACH ? eye + dir * LOW_FOG_REACH : world;
        float ge = ground_at(eye.xy);
        float gz = ground_at(end.xy);
        float through_fog = exp(-low_fog_depth(low, dist, eye.z, ge, end.z, gz));
        uint cascades = asuint(row(a, ATM_LOW_FOG_GROUND).w);
        float lit = 1.0;
        if (cascades != 0xFFFFFFFF && through_fog < 0.999)
        {
            float forward = transform(push.frame, FRAME_VIEW, float4(eye + dir, 1.0)).z
                            - transform(push.frame, FRAME_VIEW, float4(eye, 1.0)).z;
            float density = low.x * exp(-max(0.5 * (ge + gz) - low.z, 0.0) / (low.y * 6.0));
            lit = shafts(cascades, eye, dir, forward, min(dist, LOW_FOG_REACH),
                         (eye.z - ge) / low.y, (end.z - gz) / low.y, density, through_fog,
                         shaft_jitter(p));
        }
        float3 fog_light = haze_light(a, push.sky_view, push.sampler, dir, 1.0) * 1.15 * lerp(0.55, 1.0, lit);
        seen = seen * through_fog + fog_light * (1.0 - through_fog);
    }
    return float4(lerp(max(haze, 0.0), max(seen, 0.0), land_fade(world)), c.a);
}
