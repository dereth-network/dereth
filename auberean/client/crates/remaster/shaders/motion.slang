// What moves under the view: each draw of a body that moved since the last frame, and each draw
// the wind sways, drawn again as the depth's pass drew it, writing where its surface was last frame
// (in the last frame's clip space, unmoved, halved), so the temporal anti-aliasing finds its last
// color where it moved from rather than where the view alone would put it. A swaying draw stood
// last frame where the last frame's wind bent it. A pixel where the depth's pass found another
// surface, nearer or (through a cut-out leaf) farther, is left as it is.
import bindless;
import frame;
import light;

// A mesh vertex's bytes, and a posed vertex's (`shaders/mesh.slang`).
static const uint MESH_VERTEX_BYTES = 92;
static const uint POSED_VERTEX_BYTES = 24;

struct Push
{
    // The frame's constants (its view and projection as the depth's pass drew), and its draws.
    uint frame;
    uint draws;
    // The draw's constants this frame, and as it stood last frame.
    uint draw;
    uint last_draw;
    // The mesh's vertices; the frame's posed vertices (~0 for none), the first of this draw's
    // pose this frame, and the first of its pose last frame.
    uint vertices;
    uint posed;
    uint posed_base;
    uint last_posed_base;
    // A buffer whose first four rows are the last frame's view and projection, unmoved, and
    // whose fifth is the last frame's wind, as the frame's own wind row.
    uint last;
    // The depth's pass's depth.
    uint depth;
    // How far the wind bends the draw at each meter over its own origin, as a float's bits (~0:
    // it holds still).
    uint sway;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct MotionOut
{
    float4 clip : SV_Position;
    // Where the vertex was last frame, in the last frame's clip space.
    float4 last : TEXCOORD0;
};

float3 position_at(uint i, uint base)
{
    if (push.posed != 0xFFFFFFFF)
    {
        return asfloat(buffers[NonUniformResourceIndex(push.posed)].Load3((base + i) * POSED_VERTEX_BYTES));
    }
    return asfloat(buffers[NonUniformResourceIndex(push.vertices)].Load3(i * MESH_VERTEX_BYTES));
}

[shader("vertex")]
MotionOut vs_motion(uint index : SV_VertexID)
{
    Light f = light_of(push.frame);
    Draw d = draw_of(push.draws, push.draw);
    Draw was = draw_of(push.draws, push.last_draw);
    MotionOut o;
    // The same arithmetic as the frame's own draw of it, so the depth is the same.
    float4 world4 = apply(d.model, float4(position_at(index, push.posed_base), 1.0));
    float4 last_world = apply(was.model, float4(position_at(index, push.last_posed_base), 1.0));
    if (push.sway != 0xFFFFFFFF)
    {
        float3 origin = apply(d.model, float4(0.0, 0.0, 0.0, 1.0)).xyz;
        float bend = asfloat(push.sway) * max(world4.z - origin.z, 0.0);
        world4.xyz += swayed(row(push.frame, FRAME_WIND), bend, origin, world4.xyz, true);
        float3 was_origin = apply(was.model, float4(0.0, 0.0, 0.0, 1.0)).xyz;
        float was_bend = asfloat(push.sway) * max(last_world.z - was_origin.z, 0.0);
        last_world.xyz += swayed(row(push.last, 4), was_bend, was_origin, last_world.xyz, true);
    }
    o.clip = apply(f.view_proj, world4);
    o.last = transform(push.last, 0, float4(last_world.xyz, 1.0));
    return o;
}

[shader("fragment")]
float4 fs_motion(MotionOut v) : SV_Target0
{
    float held = textures[NonUniformResourceIndex(push.depth)].Load(int3(int2(v.clip.xy), 0)).x;
    // The depth's pass drew another surface here: something nearer covers this one, or the
    // draw's own texel was cut out and what stands behind shows.
    if (abs(held - v.clip.z) > max(abs(v.clip.z) * 1e-4, 1e-6))
    {
        discard;
    }
    float2 last = v.last.w > 0.0 ? v.last.xy / v.last.w : float2(-2.0, -2.0);
    return float4(clamp(last * 0.5, -0.99, 0.99), 0.0, 0.0);
}
