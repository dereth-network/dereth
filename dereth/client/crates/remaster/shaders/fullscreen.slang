// A triangle that covers the whole target from three vertices and no vertex buffer. The passes
// drawn with it read their inputs by pixel (`SV_Position`), so the viewport's orientation does
// not matter to them.

struct Covering
{
    float4 position : SV_Position;
};

[shader("vertex")]
Covering vs_main(uint vertex : SV_VertexID)
{
    float2 corner = float2((vertex << 1) & 2, vertex & 2);
    Covering o;
    o.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
    return o;
}
