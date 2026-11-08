// With an art pack, the clouds' shadows: how much of the sun comes through the clouds to the world
// under each texel of a map round the eye, from retail's own cloud picture laid over the world.
import atmosphere;
import bindless;
import retail;

struct Push
{
    uint atmosphere;
    // The map's constants, as float4 rows:
    //   0  retail's picture's coordinates: u = x + y east, v = z + w north
    //   1  xy how far the picture has drifted, zw where the frame's origin lies in the world
    //   2  x how much of the sky the clouds cover, y how thick they are, z how much of them
    //      retail's hours leave showing, w the texels a side of the map
    uint clouds;
    uint picture;
    uint sampler;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// How much light a meter of the cloud takes `h` of the way up the layer over the point `at` of the
// frame's space, read over a footprint `footprint` meters across: a column straight up hides about
// as much as retail's picture there does, its thin edges eaten away.
float extinction(float2 at, float h, float footprint)
{
    float4 lay = row(push.clouds, 0);
    float4 drift = row(push.clouds, 1);
    float4 look = row(push.clouds, 2);
    float2 w = at + drift.zw;
    float2 uv = float2(lay.x + lay.y * w.x, lay.z + lay.w * w.y) + drift.xy;
    float lod = max(log2(footprint * abs(lay.y) * 256.0), 0.0);
    float cover = textures[NonUniformResourceIndex(push.picture)]
                      .SampleLevel(samplers[NonUniformResourceIndex(push.sampler)], uv, lod)
                      .a
                  * look.x;
    float c = smoothstep(0.08, 1.0, cover);
    if (c < 0.01)
    {
        return 0.0;
    }
    float top = lerp(0.3, 1.0, sqrt(c));
    float shape = smoothstep(0.0, 0.08, h) * (1.0 - smoothstep(top * 0.45, top, h));
    if (shape < 0.01)
    {
        return 0.0;
    }
    float erosion = 0.45 * (1.0 - 0.5 * c);
    float s = saturate((shape - erosion) / max(1.0 - erosion, 0.05));
    float tau = -log(1.0 - 0.97 * c) * lerp(1.0, 2.0, c * c);
    return s * tau / max(look.y * top * 0.7, 1.0);
}

// Along the sun's line from the clouds' base up through their depth, in six steps.
[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    uint a = push.atmosphere;
    float4 map = row(a, ATM_CLOUD_MAP);
    float4 look = row(push.clouds, 2);
    float2 uv = position.xy / look.w;
    float2 xy = map.xy + (float2(uv.x, 1.0 - uv.y) * 2.0 - 1.0) * map.z;
    float3 sun = row(a, ATM_SUN).xyz;
    float rise = look.y / max(sun.z, 0.05);
    float footprint = max(2.0 * map.z / look.w, rise / 6.0 * 0.25);
    float tau = 0.0;
    for (int k = 0; k < 6; k++)
    {
        float h = (float(k) + 0.5) / 6.0;
        tau += extinction(xy + sun.xy * (h * rise), h, footprint) * rise / 6.0;
    }
    float t = exp(-tau * look.z);
    return float4(t, t, t, t);
}
