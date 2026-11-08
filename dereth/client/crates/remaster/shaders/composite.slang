// The remaster's world composited into the frame: its colors and its depth written into the
// frame's own target within the world's rectangle, so the interface draws over it as it draws
// over the ordinary world. An upscaler leaves the colors at the display's size and the depth at the
// render size, which is read scaled.
import bindless;

struct Push
{
    uint color;
    uint depth;
    // The world's rectangle's top-left corner on the frame's target.
    int x;
    int y;
    // The depth's size over the colors', each way.
    float depth_x;
    float depth_y;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct Out
{
    float4 color : SV_Target0;
    float depth : SV_Depth;
};

[shader("fragment")]
Out fs_main(float4 position : SV_Position)
{
    int2 p = int2(position.xy) - int2(push.x, push.y);
    Out o;
    o.color = textures[NonUniformResourceIndex(push.color)].Load(int3(p, 0));
    int2 q = int2((float2(p) + 0.5) * float2(push.depth_x, push.depth_y));
    o.depth = textures[NonUniformResourceIndex(push.depth)].Load(int3(q, 0)).x;
    return o;
}
