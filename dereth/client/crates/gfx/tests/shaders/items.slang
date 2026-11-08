// One square per draw. The draw item is the command's first instance (on Direct3D 12 the draw's
// root constant); item `i` is eight floats
// in buffer `items`: the square's corners in clip space (x0, y0, x1, y1) and its color.
import table;

struct Push
{
    uint items;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// Two triangles, corners 0, 1, 2 and 2, 1, 3 of the unit square, corner `c` at
// `(c & 1, c >> 1)`.
static const uint CORNERS[6] = { 0, 1, 2, 2, 1, 3 };

struct Square
{
    float4 position : SV_Position;
    nointerpolation float4 color : COLOR;
};

[shader("vertex")]
Square vs_main(uint vertex : SV_VertexID
#ifndef DERETH_DXIL
    , uint item : SV_StartInstanceLocation
#endif
)
{
#ifdef DERETH_DXIL
    uint item = draw_item.value;
#endif
    ByteAddressBuffer items = buffers[NonUniformResourceIndex(push.items)];
    float4 rect = asfloat(items.Load4(item * 32));
    uint corner = CORNERS[vertex];
    float2 unit = float2(corner & 1, corner >> 1);
    Square o;
    o.position = float4(lerp(rect.xy, rect.zw, unit), 0.0, 1.0);
    o.color = asfloat(items.Load4(item * 32 + 16));
    return o;
}

[shader("fragment")]
float4 fs_main(Square square) : SV_Target0
{
    return square.color;
}
