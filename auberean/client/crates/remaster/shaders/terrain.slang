// The landscape: the near terrain's bottom layer and its blended layers, the sun's shadow map and
// the clouds' shadows laid over them, then their fog; and the far landscape. The landscape's
// vertices carry their lit color (worked out on the processor, as the retail client does), their
// ground's coordinates and their mask's.
import bindless;
import light;

// A landscape vertex's bytes: its position, its lit color (four floats), its ground's coordinates
// and its mask's.
static const uint TERRAIN_VERTEX_BYTES = 44;

struct Push
{
    // The frame's constants (the outdoors' light).
    uint frame;
    // The vertices.
    uint vertices;
    // The ground (or the shadow map) and its sampler; the mask (or the clouds) and theirs.
    uint tex0;
    uint samp0;
    uint tex1;
    uint samp1;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct TerrainOut
{
    // The same position in every pass over the near terrain, so the passes laid over its layers
    // meet its depth exactly.
    precise float4 clip : SV_Position;
    float4 color : COLOR0;
    float2 uv : TEXCOORD0;
    float2 mask_uv : TEXCOORD1;
    float fog : TEXCOORD2;
    // The sun's shadow map's coordinates, and the clouds'.
    float2 shadow_uv : TEXCOORD3;
    float2 cloud_uv : TEXCOORD4;
};

[shader("vertex")]
TerrainOut vs_terrain(uint index : SV_VertexID)
{
    Light f = light_of(push.frame);
    ByteAddressBuffer b = buffers[NonUniformResourceIndex(push.vertices)];
    uint at = index * TERRAIN_VERTEX_BYTES;
    float3 position = asfloat(b.Load3(at));
    float4 color = asfloat(b.Load4(at + 12));
    TerrainOut o;
    o.clip = apply(f.view_proj, float4(position, 1.0));
    // White, times the vertex's lit color.
    o.color = color;
    o.uv = asfloat(b.Load2(at + 28));
    o.mask_uv = asfloat(b.Load2(at + 36));
    o.fog = linear_fog(f, position);
    o.shadow_uv = apply(f.shadow_uv, float4(position, 1.0)).xy;
    // The ground's place times the clouds' scale, slid by the running time.
    o.cloud_uv = position.xy * f.cloud.x + f.cloud.yz;
    return o;
}

// The near terrain's own fog, unless it is laid on after the shadows.
float3 terrain_fogged(Light f, float3 c, float fog)
{
    if (f.terrain.x > 0.5)
    {
        return c;
    }
    return fogged(f, c, fog);
}

// The bottom layer: the ground modulated twice by the lit color.
[shader("fragment")]
float4 fs_terrain_base(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    float3 c = saturate(2.0 * t.rgb * v.color.rgb);
    return float4(terrain_fogged(f, c, v.fog), 1.0);
}

// A blended layer: the same, with the mask's alpha.
[shader("fragment")]
float4 fs_terrain_mask(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    float4 m = sample(push.tex1, push.samp1, v.mask_uv);
    float3 c = saturate(2.0 * t.rgb * v.color.rgb);
    return float4(terrain_fogged(f, c, v.fog), m.a);
}

// The clouds' shadow at a point: the cloud picture where it lies, or white with no clouds.
float3 cloud_shade(Light f, float2 uv)
{
    float3 c = sample(push.tex1, push.samp1, uv).rgb;
    if (f.cloud.w < 0.5)
    {
        return float3(1.0, 1.0, 1.0);
    }
    return c;
}

// Over the near terrain's layers: the sun's shadow map (stage 0) times the clouds (stage 1),
// multiplying what the layers drew.
[shader("fragment")]
float4 fs_terrain_shade(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float3 s = sample(push.tex0, push.samp0, v.shadow_uv).rgb;
    return float4(s * cloud_shade(f, v.cloud_uv), 1.0);
}

// Then the fog over it: the fog color, as opaque as the point is fogged.
[shader("fragment")]
float4 fs_terrain_fog(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    return float4(f.fog_color.rgb, 1.0 - v.fog);
}

// The far landscape: the picture added to the lit color, signed and doubled; with the clouds'
// shadows, added signed, then doubled against the cloud picture.
[shader("fragment")]
float4 fs_terrain_far(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    float3 cloud = sample(push.tex1, push.samp1, v.cloud_uv).rgb;
    float3 c = saturate(2.0 * (t.rgb + v.color.rgb - 0.5));
    if (f.cloud.w > 0.5)
    {
        float3 signed_sum = saturate(t.rgb + v.color.rgb - 0.5);
        c = saturate(2.0 * signed_sum * cloud);
    }
    return float4(fogged(f, c, v.fog), 1.0);
}

// The sky's fog veil: a full-screen triangle of the fog color, as opaque as the color's alpha.
[shader("vertex")]
float4 vs_veil(uint i : SV_VertexID) : SV_Position
{
    float x = float((i << 1) & 2) * 2.0 - 1.0;
    float y = float(i & 2) * 2.0 - 1.0;
    return float4(x, y, 0.0, 1.0);
}

[shader("fragment")]
float4 fs_veil() : SV_Target0
{
    Light f = light_of(push.frame);
    return f.fog_color;
}
