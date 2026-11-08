// One triangle in a solid color. Its corners are in clip space: the first two in `corners01`, the
// third in `corner2`. (No arrays in a push block: Direct3D 12 lays an array's elements 16 bytes
// apart, Vulkan side by side.)
struct Push
{
    float4 color;
    float4 corners01;
    float2 corner2;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("vertex")]
float4 vs_main(uint vertex : SV_VertexID) : SV_Position
{
    float2 c = vertex == 0 ? push.corners01.xy : (vertex == 1 ? push.corners01.zw : push.corner2);
    return float4(c, 0.0, 1.0);
}

[shader("fragment")]
float4 fs_main() : SV_Target0
{
    return push.color;
}
