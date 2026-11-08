// With an art pack, the air between the eye and the land, in place of the fog: what the world drew
// is dimmed by the air in front of it and lit by the light that air scatters toward the eye. Two
// layers act on every pixel of land. The atmosphere's own air tints far land toward the sky's color
// at the horizon in that direction. The low haze fades the land into its own light, the light the
// veil over the horizon is drawn in, and lies thickest over low ground. Near where the land ends it
// fades a little further, so the end of the world is softened the same way wherever the eye stands.
// A low fog may lie over the lowest ground round the eye, thinning with height, lit by the sun and the
// sky as the haze is.
import atmosphere;
import bindless;
import intent;
import retail;

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

// How far into the low fog the eye sees a point `dist` metres away at height `z`, from the eye at
// `eye_z`: its extinction integrated along the line, the fog thinning by e every `height` metres
// over `base`. Nothing within the first 25 metres from the eye.
float low_fog_depth(float4 fog, float dist, float eye_z, float z)
{
    float he = clamp((eye_z - fog.z) / fog.y, -3.0, 40.0);
    float hp = clamp((z - fog.z) / fog.y, -3.0, 40.0);
    float dz = hp - he;
    float shape = abs(dz) > 1e-3 ? (exp(-he) - exp(-hp)) / dz : exp(-he);
    return fog.x * max(dist - 25.0, 0.0) * shape;
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
        // a little brighter for the sun lighting it from above.
        float through_fog = exp(-low_fog_depth(low, dist, eye.z, world.z));
        float3 fog_light = haze_light(a, push.sky_view, push.sampler, dir, 1.0) * 1.15;
        seen = seen * through_fog + fog_light * (1.0 - through_fog);
    }
    return float4(lerp(max(haze, 0.0), max(seen, 0.0), land_fade(world)), c.a);
}
