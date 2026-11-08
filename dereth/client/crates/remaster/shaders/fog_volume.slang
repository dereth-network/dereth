// The fog's volume, one column of slices a thread: down each direction from the eye, retail's
// range fog at each slice's distance, and with glow the sun the fog in front of it scatters toward
// the eye where the sun reaches it.
import bindless;
import fog;
import retail;

struct Push
{
    uint frame;
    // The fog's constants, and the volume it writes.
    uint fog;
    uint volume;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// What the air past the fog's end leaves of the fog's color.
static const float3 FAR_AIR = float3(0.62, 0.7, 0.85);

// The shadow cascades' rows, as the sun's visibility pass reads them.
static const uint CASCADE_FAR = 16;
static const uint CASCADE_MAPS = 18;
static const uint CASCADE_FADE = 19;

// How much of the sun reaches the point `p`, `ahead` metres along the view: one look into the
// first cascade that covers it.
float sunlit(uint cascades, float3 p, float ahead)
{
    float4 far = row(cascades, CASCADE_FAR);
    uint4 maps = row_u(cascades, CASCADE_MAPS);
    float4 fade = row(cascades, CASCADE_FADE);
    if (ahead >= fade.y)
    {
        return 1.0;
    }
    int size = int(fade.w);
    for (uint c = 0; c < 4; c++)
    {
        if (ahead > far[c])
        {
            continue;
        }
        float4 q = transform(cascades, c * 4, float4(p, 1.0));
        float2 uv = float2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
        if (any(uv < 0.0) || any(uv > 1.0) || q.z < 0.0 || q.z > 1.0)
        {
            continue;
        }
        int2 t = clamp(int2(uv * float(size)), int2(0, 0), int2(size - 1, size - 1));
        float d = textures[NonUniformResourceIndex(maps[c])].Load(int3(t, 0)).x;
        return q.z <= d + 1e-4 ? 1.0 : 0.0;
    }
    return 1.0;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    float4 grid = row(push.fog, FOG_GRID);
    float4 range = row(push.fog, FOG_RANGE);
    if (float(id.x) >= grid.x || float(id.y) >= grid.y)
    {
        return;
    }
    float2 ndc = float2((float(id.x) + 0.5) / grid.x * 2.0 - 1.0,
                        1.0 - (float(id.y) + 0.5) / grid.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, 0.5, 1.0));
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 dir = normalize(h.xyz / h.w - eye);
    float3 fog_color = row(push.frame, FRAME_FOG_COLOR).rgb;
    float4 sun = row(push.fog, FOG_SUN);
    float3 to_sun = normalize(row(push.frame, FRAME_SUN).xyz + float3(0.0, 0.0, 1e-6));
    float g = sun.w;
    float mu = dot(dir, to_sun);
    // The Henyey-Greenstein lobe, without its 1 / 4 pi: strongest looking toward the sun.
    float lobe = (1.0 - g * g) / pow(max(1.0 + g * g - 2.0 * g * mu, 1e-4), 1.5);
    uint cascades = asuint(grid.z);
    float shadows = grid.w;
    float3 glow = float3(0.0, 0.0, 0.0);
    float far_keep = row(push.fog, FOG_FAR).x;
    float fog_end = max(row(push.fog, FOG_FAR).y, 1.0);
    float before = lerp(far_keep, 1.0, fog_factor(push.frame, slice_distance(push.fog, 0.0)));
    int slices = int(range.z);
    float3 forward = transform(push.frame, FRAME_VIEW, float4(eye + dir, 1.0)).xyz
                     - transform(push.frame, FRAME_VIEW, float4(eye, 1.0)).xyz;
    for (int k = 0; k < slices; k++)
    {
        float d0 = slice_distance(push.fog, float(k));
        float d1 = slice_distance(push.fog, float(k + 1));
        float kept = lerp(far_keep, 1.0, fog_factor(push.frame, d1));
        // Past the fog's end the air between takes the far ground's color toward its own blue.
        float past = far_keep > 0.0 ? saturate((d1 - fog_end) / fog_end) : 0.0;
        float3 air = lerp(fog_color, fog_color * FAR_AIR, past);
        if (range.w > 0.0)
        {
            float3 mid = eye + dir * (0.5 * (d0 + d1));
            float lit = 1.0;
            if (cascades != 0xFFFFFFFF)
            {
                lit = lerp(1.0, sunlit(cascades, mid, 0.5 * (d0 + d1) * forward.z), shadows);
            }
            glow += sun.rgb * (max(before - kept, 0.0) * lobe * lit * range.w);
        }
        before = kept;
        float4 v = float4(air * (1.0 - kept) + glow, kept);
        uint at = ((uint(k) * uint(grid.y) + id.y) * uint(grid.x) + id.x) * 16;
        rw_buffers[NonUniformResourceIndex(push.volume)].Store4(at, asuint(v));
    }
}
