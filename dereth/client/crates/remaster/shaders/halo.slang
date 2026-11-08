// The soft halos round the lamps the eye can see: a bright core and a faint wide glow, added over
// the scene's colors. Each is a card facing the eye, as wide as the glow can be seen. A pixel whose
// surface stands in front of the lamp takes none, so a wall or a post between the eye and the lamp
// cuts its halo cleanly.
import bindless;
import retail;

struct Push
{
    uint frame;
    // The view's right and up, then for each halo its place and width, and its color.
    uint rows;
    uint depth;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The core's width and the wide glow's, as shares of the lamp's own, the wide glow's share of the
// light, and how much of the lamp's color it shows.
static const float CORE = 0.14;
static const float WIDE = 0.7;
static const float WIDE_SHARE = 0.1;
static const float GAIN = 0.9;
// How far out a card reaches, in the wide glow's widths: nothing can be seen past it.
static const float CARD = 3.0;

struct Varyings
{
    float4 position : SV_Position;
    // The point on the card, and the lamp's place and width.
    float3 point : TEXCOORD0;
    float4 lamp : TEXCOORD1;
    float3 color : TEXCOORD2;
};

[shader("vertex")]
Varyings vs_halo(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    static const float2 CORNERS[6] = {
        float2(-1.0, -1.0), float2(1.0, -1.0), float2(1.0, 1.0),
        float2(-1.0, -1.0), float2(1.0, 1.0), float2(-1.0, 1.0)
    };
    float4 lamp = row(push.rows, 2 + instance * 2);
    float2 c = CORNERS[vertex] * CARD * WIDE * lamp.w;
    float3 point = lamp.xyz + row(push.rows, 0).xyz * c.x + row(push.rows, 1).xyz * c.y;
    Varyings o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(point, 1.0));
    o.point = point;
    o.lamp = lamp;
    o.color = row(push.rows, 3 + instance * 2).rgb;
    return o;
}

// How far from the eye the surface of depth `d` stands at the pixel.
float surface_at(float2 position, float d)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return length(h.xyz / h.w - row(push.frame, FRAME_EYE).xyz);
}

[shader("fragment")]
float4 fs_halo(Varyings in) : SV_Target0
{
    int3 p = int3(int2(in.position.xy), 0);
    float near = surface_at(in.position.xy, textures[NonUniformResourceIndex(push.depth)].Load(p).x);
    float out_near = surface_at(in.position.xy, textures[NonUniformResourceIndex(push.outdoor_depth)].Load(p).x);
    float size = in.lamp.w;
    float dist = length(in.lamp.xyz - row(push.frame, FRAME_EYE).xyz);
    // A surface in front of the lamp hides its halo, with room for the lamp's own glass.
    if (min(near, out_near) < dist - 0.35 * size - 0.15)
    {
        discard;
    }
    float off = length(in.point - in.lamp.xyz);
    float a = off / (CORE * size);
    float b = off / (WIDE * size);
    float g = exp(-a * a) + WIDE_SHARE * exp(-b * b);
    return float4(in.color * g * GAIN, 0.0);
}
