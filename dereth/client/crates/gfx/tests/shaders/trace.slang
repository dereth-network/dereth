// Rays cast straight down through a grid of `width` by `height` points over the square from -2 to
// 2, from 10 up, into top-level structure `accel` with mask `mask`. Word `i` of read-write buffer
// `out` is 0 for a miss, else the hit's instance index of the user's plus one, its geometry's
// index times 4096, and its distance in hundredths times 65536. A candidate hit on a geometry that
// is not opaque is taken.
import table;

struct Push
{
    uint accel;
    uint out;
    uint width;
    uint height;
    uint mask;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= push.width || id.y >= push.height)
    {
        return;
    }
    float2 xy = (float2(id.xy) + 0.5) / float2(push.width, push.height) * 4.0 - 2.0;
    RayDesc ray;
    ray.Origin = float3(xy, 10.0);
    ray.Direction = float3(0.0, 0.0, -1.0);
    ray.TMin = 0.0;
    ray.TMax = 100.0;
    RayQuery<RAY_FLAG_NONE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(push.accel)], RAY_FLAG_NONE, push.mask, ray);
    while (q.Proceed())
    {
        if (q.CandidateType() == CANDIDATE_NON_OPAQUE_TRIANGLE)
        {
            q.CommitNonOpaqueTriangleHit();
        }
    }
    uint v = 0;
    if (q.CommittedStatus() == COMMITTED_TRIANGLE_HIT)
    {
        v = (q.CommittedInstanceID() + 1) | (q.CommittedGeometryIndex() << 12)
            | (uint(q.CommittedRayT() * 100.0 + 0.5) << 16);
    }
    rw_buffers[NonUniformResourceIndex(push.out)].Store((id.y * push.width + id.x) * 4, v);
}
