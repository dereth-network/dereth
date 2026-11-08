// The moving objects' motion: each part that moved since the last frame drawn again, where the
// G-buffer holds it, writing where its surface was last frame (in clip space, halved), so the
// temporal anti-aliasing finds its last color where it moved from rather than where the view
// alone would put it.
//
// A part is drawn by its own draw items, each command naming its row in buffer `moving`: the
// item, then the part's last transform, in the frame's space, as four columns.
import bindless;
import retail;

struct Push
{
    uint frame;
    // The post passes' constants: their rows 0..3 are the last frame's view and projection,
    // unmoved, in this frame's space.
    uint post;
    uint items;
    uint instances;
    uint vertices;
    uint moving;
    // The rows an instance takes in buffer `instances`; its transform is the first four.
    uint instance_rows;
    // The depth, and the outdoor surfaces' depth: where the part is drawn in neither, something
    // else covers it.
    uint depth;
    uint outdoor_depth;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct Varyings
{
    float4 position : SV_Position;
    // Where the vertex was last frame, in clip space.
    float4 last : TEXCOORD0;
};

[shader("vertex")]
Varyings vs_main(uint vertex : SV_VertexID
#ifndef DERETH_DXIL
    , uint command : SV_StartInstanceLocation
#endif
)
{
#ifdef DERETH_DXIL
    uint command = draw_item.value;
#endif
    uint item = row_u(push.moving, command * 5).x;
    uint4 a = row_u(push.items, item * 2);
    // A mesh vertex is three rows; its position leads the first.
    float3 position = row(push.vertices, (a.y + vertex) * 4).xyz;
    // The same arithmetic as the G-buffer's, so the depth is the same.
    float4 p = transform(push.instances, a.x * push.instance_rows, float4(position, 1.0));
    float4 was = transform(push.moving, command * 5 + 1, float4(position, 1.0));
    Varyings o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p.xyz, 1.0));
    o.last = transform(push.post, 0, float4(was.xyz, 1.0));
    return o;
}

// Whether depth `z` is the depth `held` holds, to a thousandth of the distance.
bool same_depth(float z, float held)
{
    return abs(z - held) <= (1.0 - held) * 1e-3 + 1e-7;
}

[shader("fragment")]
float4 fs_main(Varyings in) : SV_Target0
{
    int2 p = int2(in.position.xy);
    float z = in.position.z;
    float d = textures[NonUniformResourceIndex(push.depth)].Load(int3(p, 0)).x;
    float od = textures[NonUniformResourceIndex(push.outdoor_depth)].Load(int3(p, 0)).x;
    if (!same_depth(z, d) && !same_depth(z, od))
    {
        discard;
    }
    // Halved, so a place up to a screen's width off it fits the target's range; never -1, which
    // is where nothing moved.
    return float4(clamp(in.last.xy / in.last.w * 0.5, -0.9999, 1.0), 0.0, 0.0);
}
