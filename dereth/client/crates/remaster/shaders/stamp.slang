// The openings' depth stamps. Where the viewer stands inside and the outdoors shows through the
// openings, the outdoor surfaces are drawn first, the depth is cleared, and each opening to the
// outdoors is drawn into the depth alone at its own depth, before the interior: the interior
// draws over the outdoors everywhere except where an opening nearer than it shows them.
import bindless;
import retail;

struct Push
{
    uint frame;
    // The openings' triangles' corners, a float4 each, in the frame's space.
    uint corners;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("vertex")]
float4 vs_main(uint v : SV_VertexID) : SV_Position
{
    float4 p = row(push.corners, v);
    return transform(push.frame, FRAME_VIEW_PROJ, float4(p.xyz, 1.0));
}
