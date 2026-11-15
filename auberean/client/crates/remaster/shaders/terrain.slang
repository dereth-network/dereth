// The landscape: the near terrain's bottom layer and its blended layers, each with the clouds'
// shadows and the fog laid on in its own draw; and the far landscape. The landscape's vertices
// carry their lit color (worked out on the processor, as the retail client does), their ground's
// coordinates and their mask's, and the sun's share of the lit color.
//
// Retail lays the clouds' shadows (times its sun's shadow map) and then the fog over the near
// terrain as two more passes over its layers. Each layer here lays them on its own color instead:
// the layers blend by their alpha over an opaque bottom layer, so their weights sum to one, and a
// product and an affine fog come out the same whether they are laid on the blended layers or on
// each layer before it is blended. The remaster's own shadows replace the shadow map (white).
//
// In the rain, a near layer is wet where it stands open to the sky, and where its kind of ground
// gathers puddles (the weather's table, by the vertex's ground) flat ground holds them: the ground
// under a puddle soaked through and smoothed, giving back the overcast air the lower the view runs
// over it.
import bindless;
import light;
import weather;

// A landscape vertex's bytes: its position, its lit color (four floats), its ground's coordinates
// and its mask's, what of its lit color the sun gives, its normal, and its ground: a kind of
// terrain's index, a road or none told.
static const uint TERRAIN_VERTEX_BYTES = 72;

// The weather's row the table of how readily each ground gathers puddles starts at, the first
// after the shared passes' own: four grounds a row, 0 none to 1 a road's, a kind of terrain at its
// index and a road after the last kind.
static const uint W_GROUNDS = W_SNOW + 1;
static const uint ROAD_GROUND = 128;
static const uint NO_GROUND = 0xFFFFFFFF;

// Where a puddle starts to fade with distance and where it is gone, meters: past them the ground
// is only wet.
static const float2 PUDDLE_FADE = float2(15.0, 45.0);

struct Push
{
    // The frame's constants (the outdoors' light).
    uint frame;
    // The vertices.
    uint vertices;
    // The ground and its sampler; the mask (or the far landscape's clouds) and theirs.
    uint tex0;
    uint samp0;
    uint tex1;
    uint samp1;
    // How much of the sky reaches each pixel, the ambient occlusion's picture (~0: all of it).
    uint ao;
    // The landscape's light class, which the depth's pass writes into the material picture.
    uint light_class;
    // How much of the sun reaches each pixel, the sun's shadows' picture (~0: all of it).
    uint sun;
    // The clouds' picture and its sampler, for the near terrain's layers.
    uint clouds;
    uint samp_clouds;
    // 1 when the frame is drawn in linear light.
    uint linear;
    // The lamps' light at each pixel, added to the land's (~0: none).
    uint lamps;
    // The weather's constants, the rain's wet (~0: none).
    uint weather;
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
    // The clouds' picture's coordinates.
    float2 cloud_uv : TEXCOORD3;
    // What of the lit color the sun gives, and where the point stands in the frame's space.
    float3 sun : TEXCOORD4;
    float3 world : TEXCOORD5;
    // The ground's normal, smooth across its triangles, and how readily its ground gathers
    // puddles (0 without the weather).
    float3 normal : TEXCOORD6;
    float takes : TEXCOORD7;
};

// How readily ground `ground` (a landscape vertex's) gathers puddles by the weather's table, 0 to
// 1; 0 without the weather's constants.
float takes_puddles(uint weather, uint ground)
{
    if (weather == NO_WEATHER || ground > ROAD_GROUND)
    {
        return 0.0;
    }
    return row(weather, W_GROUNDS + ground / 4)[ground % 4];
}

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
    o.sun = asfloat(b.Load3(at + 44));
    o.normal = asfloat(b.Load3(at + 56));
    o.takes = takes_puddles(push.weather, b.Load(at + 68));
    o.world = position;
    o.fog = linear_fog(f, position);
    // The ground's place times the clouds' scale, slid by the running time.
    o.cloud_uv = position.xy * f.cloud.x + f.cloud.yz;
    return o;
}

// The clouds' shadow at a point: the cloud picture `t` where it lies, or white with no clouds.
float3 cloud_shade(Light f, uint t, uint s, float2 uv)
{
    float3 c = sample(t, s, uv).rgb;
    if (f.cloud.w < 0.5)
    {
        return float3(1.0, 1.0, 1.0);
    }
    return c;
}

// The ground picture's color at `uv` with its finest detail let go, as still water over it shows
// it: from a level of the picture eight times coarser than the steps `dx` and `dy` across the
// screen ask for.
float3 smoothed_ground(float2 uv, float2 dx, float2 dy)
{
    return textures[NonUniformResourceIndex(push.tex0)]
        .SampleGrad(samplers[NonUniformResourceIndex(push.samp0)], uv, dx * 8.0, dy * 8.0)
        .rgb;
}

// A near layer's color as the rain's puddles leave it: `wet` (its color `dry` as the rain leaves
// it, display values) where none lies, and where one does (by the weather's mask and how readily
// the ground takes them, `v.takes`; flat ground by its normal `n`) the ground under still water,
// soaked through and its finest detail smoothed (`smooth`, its color from a coarser level), giving
// back the overcast air (the fog's color) the lower the view runs over it, held down at grazing
// angles so it never turns to a sheet of light, and faded to the wet ground with distance.
float3 puddled(Light f, TerrainOut v, float3 n, float3 dry, float3 smooth, float3 wet)
{
    float share = puddle_share(push.weather, v.world, n, v.takes);
    if (share <= 0.0)
    {
        return wet;
    }
    float3 to_eye = f.eye.xyz - v.world;
    float dist = length(to_eye);
    float fade = 1.0 - smoothstep(PUDDLE_FADE.x, PUDDLE_FADE.y, dist);
    if (fade <= 0.0)
    {
        return wet;
    }
    // Still water faces straight up.
    float cosine = saturate(to_eye.z / max(dist, 1e-4));
    float edge = 1.0 - cosine;
    float fresnel = 0.02 + 0.98 * edge * edge * edge * edge * edge;
    float hold = lerp(0.75, 0.4, smoothstep(8.0, PUDDLE_FADE.y, dist))
                 * lerp(0.55, 1.0, smoothstep(0.03, 0.25, cosine));
    float3 bed = soaked(lerp(dry, smooth, 0.6), 1.0);
    float3 puddle = lerp(bed, f.fog_color.rgb * 0.8, saturate(fresnel * 1.2 + 0.12) * hold);
    return lerp(wet, puddle, share * fade);
}

// A near layer's color `c` as retail leaves it once its passes over the layers are drawn: where
// the frame lays the sun's shadow map and the clouds over the near terrain, times the clouds'
// shadow; then, in the rain, wet where it is open to the sky, and puddled where its ground gathers
// puddles (`smooth`: its color with the ground's finest detail let go); then fogged.
float3 near_fogged(Light f, TerrainOut v, float3 c, float3 smooth)
{
    if (f.terrain.x > 0.5)
    {
        float3 shade = cloud_shade(f, push.clouds, push.samp_clouds, v.cloud_uv);
        c = c * shade;
        smooth = smooth * shade;
    }
    if (push.weather != NO_WEATHER)
    {
        // The land never leans past upright: its triangle's slope, turned up.
        float3 n = normalize(cross(ddx(v.world), ddy(v.world)));
        float3 dry = c;
        c = rained(push.weather, f, push.light_class, v.world, n.z < 0.0 ? -n : n, c);
        if (v.takes > 0.0)
        {
            c = puddled(f, v, normalize(v.normal), dry, smooth, c);
        }
    }
    return fogged(f, c, v.fog);
}

// A near layer's lit ground at the pixel, `t` its ground picture's color there: the ground
// modulated twice by the lit color, and the same with the picture's finest detail let go where a
// puddle may lie.
float3 near_layer(Light f, TerrainOut v, float3 t)
{
    // The steps across the screen are taken before the branch.
    float2 dx = ddx(v.uv);
    float2 dy = ddy(v.uv);
    float3 l = lit(f, v);
    float3 c = saturate(2.0 * t * l);
    float3 smooth = c;
    if (v.takes > 0.0)
    {
        smooth = saturate(2.0 * smoothed_ground(v.uv, dx, dy) * l);
    }
    return near_fogged(f, v, c, smooth);
}

// Picture `t`'s first channel at the pixel `position`, or 1 with none (~0).
float mask_at(uint t, float4 position)
{
    if (t == 0xFFFFFFFF)
    {
        return 1.0;
    }
    return textures[NonUniformResourceIndex(t)].Load(int3(int2(position.xy), 0)).x;
}

// The vertex's lit color with the ambient's share taken by the ambient occlusion at the pixel and
// the sun's by the sun's shadows, and the lamps' light added: in open sky and full sun, with no
// lamps, the retail color. The sun's share is the vertex's own; the ambient's is the rest.
float3 lit(Light f, TerrainOut v)
{
    float3 lamps = lamp_at(push.lamps, v.clip);
    if (push.ao == 0xFFFFFFFF && push.sun == 0xFFFFFFFF)
    {
        return v.color.rgb + lamps;
    }
    float ao = mask_at(push.ao, v.clip);
    float sun = mask_at(push.sun, v.clip);
    float3 sunlit = clamp(v.sun, 0.0, v.color.rgb);
    float3 ambient = v.color.rgb - sunlit;
    return max(v.color.rgb - (1.0 - ao) * ambient - (1.0 - sun) * sunlit, 0.0) + lamps;
}

// The depth's pass: the near terrain's bottom layer, its light class into the material picture,
// and the slope of its triangle, turned to the eye, into the normal picture.
[shader("fragment")]
DepthOut fs_terrain_depth(TerrainOut v)
{
    Light f = light_of(push.frame);
    return depth_out(push.light_class, cross(ddx(v.world), ddy(v.world)), f.eye.xyz - v.world);
}

// The bottom layer: the ground modulated twice by the lit color.
[shader("fragment")]
float4 fs_terrain_base(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    return float4(shown(push.linear, near_layer(f, v, t.rgb)), 1.0);
}

// A blended layer: the same, with the mask's alpha.
[shader("fragment")]
float4 fs_terrain_mask(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    float4 m = sample(push.tex1, push.samp1, v.mask_uv);
    return float4(shown(push.linear, near_layer(f, v, t.rgb)), m.a);
}

// The far landscape: the picture added to the lit color, signed and doubled; with the clouds'
// shadows, added signed, then doubled against the cloud picture.
[shader("fragment")]
float4 fs_terrain_far(TerrainOut v) : SV_Target0
{
    Light f = light_of(push.frame);
    float4 t = sample(push.tex0, push.samp0, v.uv);
    float3 cloud = sample(push.tex1, push.samp1, v.cloud_uv).rgb;
    float3 color = lit(f, v);
    float3 c = saturate(2.0 * (t.rgb + color - 0.5));
    if (f.cloud.w > 0.5)
    {
        float3 signed_sum = saturate(t.rgb + color - 0.5);
        c = saturate(2.0 * signed_sum * cloud);
    }
    return float4(shown(push.linear, fogged(f, c, v.fog)), 1.0);
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

// A frame drawn in linear light starts from the sky drawn as retail draws it, its objects and its
// veil blended in display values into a picture of its own (`tex0`): each pixel of it, in linear
// light. The veil is a blend of the fog's color over whatever the sky's objects drew, which in
// linear light would weigh the bright fog far more than retail's blend does.
[shader("fragment")]
float4 fs_sky_in(float4 position : SV_Position) : SV_Target0
{
    float3 c = textures[NonUniformResourceIndex(push.tex0)].Load(int3(int2(position.xy), 0)).rgb;
    return float4(to_linear(c), 1.0);
}
