// The entry points of the FSR 3.1 upscaler (`fsr3.slang`, which credits it), a compute pass each,
// all reading their constants through one push word, and the pass that fills the display's texture
// from the upscaled picture. A pass covers its picture in groups of eight by eight, so the last
// groups can stand partly past its edge.
import bindless;
import fsr3;

struct Push
{
    // The upscaler's constants.
    uint constants;
    // The shading change's pyramid level that a pass makes.
    uint level;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

bool past(int2 p, int2 size)
{
    return any(p >= size);
}

// The reconstructed depth starts each frame at the far plane, which is nearer than nothing.
[shader("compute")]
[numthreads(8, 8, 1)]
void cs_clear_reconstructed(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, RenderSize()))
    {
        store1(pictures.reconstructed, p, RenderSize(), 1.0);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_prepare_inputs(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, RenderSize()))
    {
        PrepareInputs(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_farthest_half(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, HalfSize()))
    {
        FarthestDepthHalf(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_shading_level_zero(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, LevelSize(0)))
    {
        ShadingChangeLevelZero(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_shading_level(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, LevelSize(push.level)))
    {
        ShadingChangeLevel(p, push.level);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_shading_change(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, HalfSize()))
    {
        ShadingChange(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_reactivity(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, RenderSize()))
    {
        PrepareReactivity(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_luma_instability(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, RenderSize()))
    {
        LumaInstability(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_accumulate(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, UpscaleSize()))
    {
        Accumulate(p);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_rcas(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (!past(p, UpscaleSize()))
    {
        Rcas(p);
    }
}

// How much of each pixel was drawn over the opaque world, by what it changed there: the rain, the
// snow, smoke, glass. Such a surface has no motion of its own to follow, so the upscaler takes
// little of its history there, or it would leave trails. Colors are compared brought under 1, so
// a change in the dark counts as one in the light.
[shader("compute")]
[numthreads(8, 8, 1)]
void cs_reactive_input(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 p = int2(id.xy);
    if (past(p, RenderSize()))
    {
        return;
    }
    float3 now = textures[NonUniformResourceIndex(pictures.color)].Load(int3(p, 0)).rgb;
    float3 before = textures[NonUniformResourceIndex(pictures.opaque)].Load(int3(p, 0)).rgb;
    float3 a = now / (1.0 + max(now.r, max(now.g, now.b)));
    float3 b = before / (1.0 + max(before.r, max(before.g, before.b)));
    float3 d = abs(a - b);
    float changed = max(d.r, max(d.g, d.b));
    store1(pictures.input_reactive, p, RenderSize(), smoothstep(0.003, 0.05, changed) * 0.9);
}

// The exposure the upscaler works the frame's colors at: from the mean of the logarithm of their
// brightness over a sparse grid of the picture, eased from frame to frame. Its first word is the
// exposure, its second what it was worked out from.
[shader("compute")]
[numthreads(1, 1, 1)]
void cs_exposure(uint3 id : SV_DispatchThreadID)
{
    use_constants(push.constants);
    int2 size = RenderSize();
    float sum = 0.0;
    static const int ACROSS = 32;
    static const int DOWN = 18;
    for (int y = 0; y < DOWN; y++)
    {
        for (int x = 0; x < ACROSS; x++)
        {
            int2 p = int2((float2(x, y) + 0.5) / float2(ACROSS, DOWN) * float2(size));
            float3 c = textures[NonUniformResourceIndex(pictures.color)].Load(int3(p, 0)).rgb;
            sum += log(max(dot(c, float3(0.2126, 0.7152, 0.0722)), 1e-4));
        }
    }
    float mean = sum / float(ACROSS * DOWN);
    RWByteAddressBuffer out = rw_buffers[NonUniformResourceIndex(pictures.exposure)];
    float was = asfloat(out.Load(4));
    // A first frame finds nothing kept, a mean of zero exactly being no real picture's.
    float eased = was == 0.0 ? mean : lerp(was, mean, 0.05);
    // The camera's own rule: the saturation-based speed at ISO 100.
    out.Store(0, asuint(clamp(0.104 / exp(eased), 0.05, 40.0)));
    out.Store(4, asuint(eased));
}

struct InputPictures
{
    float depth : SV_Target0;
    float4 motion : SV_Target1;
};

// The depth and the motion, in pixels, as pictures, for an upscaler that takes them so.
[shader("fragment")]
InputPictures fs_inputs_as_pictures(float4 position : SV_Position)
{
    use_constants(push.constants);
    int2 p = int2(position.xy);
    int2 size = RenderSize();
    uint packed = buffers[NonUniformResourceIndex(pictures.input_motion)].Load((p.y * size.x + p.x) * 4);
    float2 motion = float2(f16tof32(packed & 0xFFFF), f16tof32(packed >> 16));
    InputPictures o;
    o.depth = load1(pictures.input_depth, p, size);
    o.motion = float4(motion * float2(size), 0.0, 0.0);
    return o;
}

// The upscaled picture copied into the display's texture, drawn with the covering triangle.
[shader("fragment")]
float4 fs_show(float4 position : SV_Position) : SV_Target0
{
    use_constants(push.constants);
    return float4(Shown(int2(position.xy)), 1.0);
}
