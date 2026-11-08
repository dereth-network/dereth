// The physical sky, drawn into the world over the authored sky dome and under the clouds, the
// sun, the moons and the stars, which are drawn over it as they always are; and the haze's veil
// over the horizon, drawn over all of them before the land. Both are written in display values,
// blended by their own alpha: how much of the dome the physical sky covers, and how thick the
// veil is at each elevation.
import atmosphere;
import bindless;
import retail;

struct Push
{
    uint atmosphere;
    // The sky-view table, by its index in the table, and the sampler it is read with.
    uint sky_view;
    uint sampler;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

float3 sky_view(float2 uv)
{
    return textures[NonUniformResourceIndex(push.sky_view)]
        .SampleLevel(samplers[NonUniformResourceIndex(push.sampler)], uv, 0.0)
        .rgb;
}

float3 direction(float4 position)
{
    uint a = push.atmosphere;
    float2 extent = row(a, ATM_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    return view_ray(a, ndc);
}

// The exposed physical sky toward `dir`. Below the horizon the horizon's own colour is kept: the
// veil and the land cover it.
float3 physical_sky(float3 dir)
{
    uint a = push.atmosphere;
    float r = eye_radius(a);
    float bottom = planet_radius(a);
    float horizon_cos = -sqrt(max(r * r - bottom * bottom, 0.0)) / r;
    float cos_zenith = max(dir.z, horizon_cos + 0.002);
    float2 uv = sky_view_uv(a, r, cos_zenith, light_view_cos(a, dir));
    return exposed_sky(a, sky_view(uv));
}

[shader("fragment")]
float4 fs_sky(float4 position : SV_Position) : SV_Target0
{
    float3 c = encode(physical_sky(direction(position)));
    return float4(drawn_as(push.atmosphere, c, position.xy), row(push.atmosphere, ATM_MISC).x);
}

// The haze over the horizon: the sky, its clouds and its lights are veiled toward the fog's light
// the nearer they are to the horizon, by day fully at and below it. At night the veil is darker
// and narrower, so the hills keep their line against the sky.
[shader("fragment")]
float4 fs_veil(float4 position : SV_Position) : SV_Target0
{
    uint a = push.atmosphere;
    float3 dir = direction(position);
    float4 v = row(a, ATM_VEIL);
    float up = max(dir.z, 0.0);
    float band = max(v.x, 1e-4);
    float day_veil = exp(-up / band);
    float x = clamp((dir.z + 2.0 * band) / (5.0 * band), 0.0, 1.0);
    float night_veil = (1.0 - x) * (1.0 - x) * (1.0 - x);
    float veil = lerp(night_veil, day_veil, v.y);
    // The veil's light: the sky's own colour just over the horizon toward `dir`'s azimuth by day,
    // and the fog's colour by night.
    float r = eye_radius(a);
    float2 uv = sky_view_uv(a, r, 0.01, light_view_cos(a, dir));
    float3 horizon = exposed_sky(a, sky_view(uv));
    float3 light = lerp(row(a, ATM_FOG).rgb, horizon, v.y * 0.9) * v.z;
    if (air_drawn(a))
    {
        // The veil in the haze's own light, so the land fades into the color behind it; in the low
        // sun the glow climbs higher toward the sun.
        float toward_sun = pow(0.5 + 0.5 * light_view_cos(a, dir), 4.0);
        veil = max(veil, row(a, ATM_SUN_LIGHT).w * toward_sun * 0.75 * exp(-up / (band * 5.0)));
        light = haze_light(a, push.sky_view, push.sampler, dir, 1.0) * v.z;
    }
    return float4(drawn_as(a, encode(light), position.xy), veil);
}
