// The skinned bodies posed on the device: each body's vertices, kept in its bind pose, moved by its
// joints this frame and written over its mesh's vertices in the vertex pool, where every pass that
// draws its anchor part reads them. Before a vertex is written, where it was drawn last frame is
// kept beside the mesh, for the motion pass.
//
// A body's vertex, in its bind pose, is four rows as the pool holds vertices: the position and the
// normal's first number; the normal's other two and the texture coordinates; the tangent and its
// handedness; the color. Its influences are two rows: its four joints (two to a word) and its share
// of dual-quaternion blending; then its four weights. Each joint this frame is six rows: its
// matrix's four columns, then its turn and its dual part as quaternions (x, y, z, w).
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

float4 quat_mul(float4 a, float4 b)
{
    return float4(a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz), a.w * b.w - dot(a.xyz, b.xyz));
}

float3 quat_turn(float4 q, float3 v)
{
    float3 t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

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
    float share = asfloat(joints.z);

    uint j[4] = { joints.x & 0xffff, joints.x >> 16, joints.y & 0xffff, joints.y >> 16 };
    float3 c0 = 0, c1 = 0, c2 = 0, c3 = 0;
    for (uint s = 0; s < 4; s++)
    {
        if (weights[s] <= 0.0)
        {
            continue;
        }
        uint at = push.joints + j[s] * 6;
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

    if (share > 0.0)
    {
        // The joints' dual quaternions blended, each turned to the first's side, made unit; and
        // their scales blended, since a dual quaternion carries none: the vertex is scaled by it
        // before it is turned and moved, as the matrices scale it.
        float4 first = row(push.frame, push.joints + j[0] * 6 + 4);
        float4 q = 0, d = 0;
        float size = 0, total = 0;
        for (uint s = 0; s < 4; s++)
        {
            if (weights[s] <= 0.0)
            {
                continue;
            }
            uint at = push.joints + j[s] * 6;
            float4 qj = row(push.frame, at + 4);
            float w = dot(qj, first) < 0.0 ? -weights[s] : weights[s];
            q += w * qj;
            d += w * row(push.frame, at + 5);
            float3 x = row(push.frame, at).xyz;
            float3 y = row(push.frame, at + 1).xyz;
            float3 z = row(push.frame, at + 2).xyz;
            size += weights[s] * pow(abs(dot(x, cross(y, z))), 1.0 / 3.0);
            total += weights[s];
        }
        float len = max(length(q), 1e-12);
        q /= len;
        d /= len;
        size = total > 0.0 ? size / total : 1.0;
        float4 conj = float4(-q.xyz, q.w);
        float3 dp = quat_turn(q, position * size) + 2.0 * quat_mul(d, conj).xyz;
        p = lerp(p, dp, share);
        n = normalize(lerp(n, quat_turn(q, normal), share));
        t = normalize(lerp(t, quat_turn(q, tangent.xyz), share));
    }

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
