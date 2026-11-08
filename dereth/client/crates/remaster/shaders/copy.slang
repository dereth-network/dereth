// Copies one bound texture to the target, pixel for pixel, drawn with the covering triangle.
import bindless;

struct Push
{
    // The texture's index in the table.
    uint source;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    return textures[NonUniformResourceIndex(push.source)].Load(int3(int2(position.xy), 0));
}
