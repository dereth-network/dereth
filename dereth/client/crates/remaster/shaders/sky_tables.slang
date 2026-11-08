// The sky's lookup tables, each drawn as a covering triangle: the transmittance from any height
// toward any zenith angle, the light scattered more than once, and the sky as seen from the eye
// in every direction, for unit sun illuminance.
import atmosphere;
import bindless;
import retail;

struct Push
{
    uint atmosphere;
    // The transmittance and multiple-scattering tables, by their index in the table, and the
    // sampler they are read with.
    uint transmittance;
    uint multiple;
    uint sampler;
    // The table drawn's size in texels.
    float width;
    float height;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

static const float TRANSMITTANCE_STEPS = 40.0;

float2 uv_of(float4 position)
{
    return position.xy / float2(push.width, push.height);
}

float3 lookup(uint t, float2 uv)
{
    return textures[NonUniformResourceIndex(t)]
        .SampleLevel(samplers[NonUniformResourceIndex(push.sampler)], uv, 0.0)
        .rgb;
}

float3 transmittance_to_top(float r, float mu)
{
    return lookup(push.transmittance, transmittance_uv(push.atmosphere, r, mu));
}

// Sunlight reaching a point at radius `r` whose sun is at zenith cosine `mu_sun`: none where the
// planet is in the way.
float3 sunlight(float r, float mu_sun)
{
    float3 o = float3(0.0, 0.0, r);
    float3 s = float3(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    if (ray_sphere(o, s, planet_radius(push.atmosphere)) > 0.0)
    {
        return float3(0.0, 0.0, 0.0);
    }
    return transmittance_to_top(r, mu_sun);
}

float3 multiple_scattering(float r, float mu_sun)
{
    uint a = push.atmosphere;
    float u = clamp(mu_sun * 0.5 + 0.5, 0.0, 1.0);
    float v = clamp((r - planet_radius(a)) / (top_radius(a) - planet_radius(a)), 0.0, 1.0);
    return lookup(push.multiple, float2(u, v));
}

[shader("fragment")]
float4 fs_transmittance(float4 position : SV_Position) : SV_Target0
{
    uint a = push.atmosphere;
    float2 rm = transmittance_from_uv(a, uv_of(position));
    float r = rm.x;
    float mu = rm.y;
    float3 o = float3(0.0, 0.0, r);
    float3 d = float3(sqrt(max(1.0 - mu * mu, 0.0)), 0.0, mu);
    float t_max = max(ray_sphere(o, d, top_radius(a)), 0.0);
    float dt = t_max / TRANSMITTANCE_STEPS;
    float3 depth = float3(0.0, 0.0, 0.0);
    for (int s = 0; s < int(TRANSMITTANCE_STEPS); s++)
    {
        float3 p = o + d * ((float(s) + 0.5) * dt);
        depth += medium(a, length(p) - planet_radius(a)).extinction * dt;
    }
    return float4(exp(-depth), 1.0);
}

// The second order of scattering from every direction over a sphere, and the fraction that
// scatters again, give the whole series of higher orders in closed form.
[shader("fragment")]
float4 fs_multiple(float4 position : SV_Position) : SV_Target0
{
    uint a = push.atmosphere;
    float2 uv = uv_of(position);
    float mu_sun = uv.x * 2.0 - 1.0;
    float r = planet_radius(a) + clamp(uv.y, 0.0, 1.0) * (top_radius(a) - planet_radius(a));
    float3 o = float3(0.0, 0.0, max(r, planet_radius(a) + 0.001));
    float3 sun = float3(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    float isotropic = 1.0 / (4.0 * PI);
    const int n = 8;
    float3 second = float3(0.0, 0.0, 0.0);
    float3 again = float3(0.0, 0.0, 0.0);
    for (int i = 0; i < n; i++)
    {
        for (int j = 0; j < n; j++)
        {
            float z = 1.0 - 2.0 * (float(i) + 0.5) / float(n);
            float phi = 2.0 * PI * (float(j) + 0.5) / float(n);
            float sz = sqrt(max(1.0 - z * z, 0.0));
            float3 d = float3(sz * cos(phi), sz * sin(phi), z);
            float t_ground = ray_sphere(o, d, planet_radius(a));
            float t_max = max(ray_sphere(o, d, top_radius(a)), 0.0);
            if (t_ground > 0.0)
            {
                t_max = t_ground;
            }
            const int steps = 20;
            float dt = t_max / float(steps);
            float3 transmittance = float3(1.0, 1.0, 1.0);
            float3 light = float3(0.0, 0.0, 0.0);
            float3 fraction = float3(0.0, 0.0, 0.0);
            for (int s = 0; s < steps; s++)
            {
                float3 p = o + d * ((float(s) + 0.5) * dt);
                float pr = length(p);
                Medium m = medium(a, pr - planet_radius(a));
                float3 step_t = exp(-m.extinction * dt);
                float mu_p = dot(p / pr, sun);
                float3 scattered = m.scattering * sunlight(pr, mu_p) * isotropic;
                float3 ext = max(m.extinction, float3(1e-7, 1e-7, 1e-7));
                light += transmittance * (scattered - scattered * step_t) / ext;
                fraction += transmittance * (m.scattering - m.scattering * step_t) / ext;
                transmittance *= step_t;
            }
            if (t_ground > 0.0)
            {
                float3 p = o + d * t_ground;
                float3 up = normalize(p);
                float mu_g = dot(up, sun);
                light += transmittance * sunlight(planet_radius(a), mu_g) * max(mu_g, 0.0)
                         * row(a, ATM_GROUND).rgb / PI;
            }
            second += light;
            again += fraction;
        }
    }
    float count = float(n * n);
    second /= count;
    again /= count;
    return float4(second / max(float3(1.0, 1.0, 1.0) - again, float3(1e-4, 1e-4, 1e-4)), 1.0);
}

// The sky from the eye: single scattering of the sun by air and haze, with the higher orders from
// the table above, for unit sun illuminance.
[shader("fragment")]
float4 fs_sky_view(float4 position : SV_Position) : SV_Target0
{
    uint a = push.atmosphere;
    float r = eye_radius(a);
    float2 angles = sky_view_from_uv(a, r, uv_of(position));
    float cos_zenith = angles.x;
    float cos_light = angles.y;
    float sin_zenith = sqrt(max(1.0 - cos_zenith * cos_zenith, 0.0));
    float sin_light = sqrt(max(1.0 - cos_light * cos_light, 0.0));
    float3 d = float3(sin_zenith * cos_light, sin_zenith * sin_light, cos_zenith);
    float sun_z = clamp(row(a, ATM_SUN).z, -1.0, 1.0);
    float3 sun = float3(sqrt(max(1.0 - sun_z * sun_z, 0.0)), 0.0, sun_z);
    float3 o = float3(0.0, 0.0, r);
    float t_ground = ray_sphere(o, d, planet_radius(a));
    float t_max = max(ray_sphere(o, d, top_radius(a)), 0.0);
    if (t_ground > 0.0)
    {
        t_max = t_ground;
    }
    t_max = min(t_max, 4000.0);
    float mu = dot(d, sun);
    float phase_r = rayleigh_phase(mu);
    float phase_m = mie_phase(mu, row(a, ATM_MIE).w);
    int steps = int(row(a, ATM_TABLES).w);
    float3 transmittance = float3(1.0, 1.0, 1.0);
    float3 light = float3(0.0, 0.0, 0.0);
    for (int s = 0; s < steps; s++)
    {
        // Steps grow with distance: short near the eye, long through the thin upper air.
        float f0 = float(s) / float(steps);
        float f1 = float(s + 1) / float(steps);
        float t0 = t_max * f0 * f0;
        float t1 = t_max * f1 * f1;
        float dt = t1 - t0;
        float3 p = o + d * (t0 + dt * 0.3);
        float pr = length(p);
        Medium m = medium(a, pr - planet_radius(a));
        float3 step_t = exp(-m.extinction * dt);
        float mu_p = dot(p / pr, sun);
        float3 single = (m.rayleigh * phase_r + float3(m.mie, m.mie, m.mie) * phase_m)
                        * sunlight(pr, mu_p);
        float3 scattered = single + multiple_scattering(pr, mu_p) * m.scattering;
        float3 ext = max(m.extinction, float3(1e-7, 1e-7, 1e-7));
        light += transmittance * (scattered - scattered * step_t) / ext;
        transmittance *= step_t;
    }
    return float4(light, 1.0);
}
