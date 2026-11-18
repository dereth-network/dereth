// The openings' depth stamps. Where the viewer stands inside and the outdoors shows through the
// openings, the outdoor surfaces are drawn first, the depth is cleared, and each opening to the
// outdoors is drawn into the depth alone at its own depth, before the interior: the interior
// draws over the outdoors everywhere except where an opening nearer than it shows them.
//
// Where the viewer stands outdoors and looks into the rooms through their openings, the outdoors
// is drawn first, and each opening clears, in a copy of the depth, what the outdoors drew behind
// it (the ground a building stands on, inside its walls), and keeps what stands in front of it.
// The rooms are drawn into that copy, and the frame's depth then takes the rooms' where they drew
// and keeps the outdoors' where they did not: what is seen beyond the rooms through a second
// opening keeps its own depth.
import bindless;
import retail;

struct Push
{
    uint frame;
    // The openings' triangles' corners, a float4 each, in the frame's space.
    uint corners;
    // The depth read, by its index in the table: the outdoors' (`fs_behind`), or the copy the
    // rooms were drawn into (`fs_rooms`).
    uint depth;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("vertex")]
float4 vs_main(uint v : SV_VertexID) : SV_Position
{
    float4 p = row(push.corners, v);
    return transform(push.frame, FRAME_VIEW_PROJ, float4(p.xyz, 1.0));
}

// Inside an opening seen from outdoors: the far plane where the outdoors drew behind the opening,
// the outdoors' own depth where it drew in front of it. Drawn keeping the farther of the two, so
// an opening behind another cannot put back what the nearer one cleared.
[shader("fragment")]
float fs_behind(float4 position : SV_Position) : SV_Depth
{
    float drawn = textures[NonUniformResourceIndex(push.depth)].Load(int3(int2(position.xy), 0)).x;
    return drawn > position.z ? 1.0 : drawn;
}

// Over the whole frame, drawn with the covering triangle: the rooms' depth where anything was
// drawn into the copy, and the frame's own left where nothing was.
[shader("fragment")]
float fs_rooms(float4 position : SV_Position) : SV_Depth
{
    float drawn = textures[NonUniformResourceIndex(push.depth)].Load(int3(int2(position.xy), 0)).x;
    if (drawn >= 1.0)
    {
        discard;
    }
    return drawn;
}
