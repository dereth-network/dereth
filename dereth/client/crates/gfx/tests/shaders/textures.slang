// A grid of `columns` squares a row, filling clip space row by row; the square of instance `i`
// is filled with texture `first + i`, sampled at its center with sampler `sampler`.
import table;

struct Push
{
    uint first;
    uint sampler;
    uint columns;
    uint rows;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// Two triangles, corners 0, 1, 2 and 2, 1, 3 of the unit square, corner `c` at
// `(c & 1, c >> 1)`.
static const uint CORNERS[6] = { 0, 1, 2, 2, 1, 3 };

struct Square
{
    float4 position : SV_Position;
    nointerpolation uint texture : TEXTURE;
};

[shader("vertex")]
Square vs_main(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    uint corner = CORNERS[vertex];
    float2 unit = float2(corner & 1, corner >> 1);
    float2 cell = float2(instance % push.columns, instance / push.columns);
    float2 size = float2(2.0 / push.columns, 2.0 / push.rows);
    Square o;
    o.position = float4(-1.0 + (cell + unit) * size, 0.0, 1.0);
    o.texture = push.first + instance;
    return o;
}

[shader("fragment")]
float4 fs_main(Square square) : SV_Target0
{
    return textures[NonUniformResourceIndex(square.texture)].Sample(
        samplers[NonUniformResourceIndex(push.sampler)], float2(0.5, 0.5));
}
