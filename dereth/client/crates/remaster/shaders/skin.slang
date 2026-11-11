// The skinned bodies posed on the device: each body's vertices, kept in its bind pose, moved by its
// joints this frame and written over its mesh's vertices in the vertex pool, where every pass that
// draws its anchor part reads them. Before a vertex is written, where it was drawn last frame is
// kept beside the mesh, for the motion pass.
//
// A body's vertex, in its bind pose, is four rows as the pool holds vertices: the position and the
// normal's first number; the normal's other two and the texture coordinates; the tangent and its
// handedness; the color. Its influences are two rows: its four joints (two to a word), then their
// four weights. Each joint this frame is four rows, its matrix's four columns.
import bindless;
import retail;

struct Push
{
    // The body's vertices.
    uint count;
    // Its bind-pose vertices and its influences.
    uint bind;
    uint influences;
    // This frame's joints: the buffer, and the row they start at.
    uint frame;
    uint joints;
    // The vertex pool, read and written, and where the body's vertices start in it; where its last
    // frame's are kept, one more than the first (0: not kept).
    uint pool;
    uint first;
    uint last;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

void store(uint row, float4 v)
{
    rw_buffers[NonUniformResourceIndex(push.pool)].Store4(row * 16, asuint(v));
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    uint v = id.x;
    if (v >= push.count)
    {
        return;
    }
    float4 r0 = row(push.bind, v * 4);
    float4 r1 = row(push.bind, v * 4 + 1);
    float4 tangent = row(push.bind, v * 4 + 2);
    float4 color = row(push.bind, v * 4 + 3);
    float3 position = r0.xyz;
    float3 normal = float3(r0.w, r1.x, r1.y);
    uint4 joints = row_u(push.influences, v * 2);
    float4 weights = row(push.influences, v * 2 + 1);

    uint j[4] = { joints.x & 0xffff, joints.x >> 16, joints.y & 0xffff, joints.y >> 16 };
    float3 c0 = 0, c1 = 0, c2 = 0, c3 = 0;
    for (uint s = 0; s < 4; s++)
    {
        if (weights[s] <= 0.0)
        {
            continue;
        }
        uint at = push.joints + j[s] * 4;
        c0 += weights[s] * row(push.frame, at).xyz;
        c1 += weights[s] * row(push.frame, at + 1).xyz;
        c2 += weights[s] * row(push.frame, at + 2).xyz;
        c3 += weights[s] * row(push.frame, at + 3).xyz;
    }
    float3 p = c0 * position.x + c1 * position.y + c2 * position.z + c3;
    // The normal by the blend's inverse transpose (its cofactors, the scale left to the
    // normalization); the tangent by the blend itself.
    float3 n = normalize(cross(c1, c2) * normal.x + cross(c2, c0) * normal.y + cross(c0, c1) * normal.z);
    float3 t = normalize(c0 * tangent.x + c1 * tangent.y + c2 * tangent.z);

    uint dst = push.first + v;
    if (push.last != 0)
    {
        // Where it was drawn last frame, kept before it is written over.
        uint was = push.last - 1 + v;
        for (uint r = 0; r < 4; r++)
        {
            store(was * 4 + r, asfloat(rw_buffers[NonUniformResourceIndex(push.pool)].Load4((dst * 4 + r) * 16)));
        }
    }
    store(dst * 4, float4(p, n.x));
    store(dst * 4 + 1, float4(n.yz, r1.zw));
    store(dst * 4 + 2, float4(t, tangent.w));
    store(dst * 4 + 3, color);
}
