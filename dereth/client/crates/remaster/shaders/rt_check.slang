// A check of the scene's acceleration structures against the G-buffer: rays cast from the eye
// through a grid of the frame's pixels, every `step` a side, into top-level structure `accel` with
// mask `mask`, alpha-tested hits tested as the G-buffer tests them. Each ray writes its hit's
// distance from the eye (-1 for a miss) and its instance's index of the user's (~0 for a miss).
import bindless;
import retail;
import rt;

struct CheckPush
{
    uint frame;
    uint accel;
    uint statics;
    uint objects;
    uint indices;
    uint vertices;
    uint materials;
    uint sampler;
    uint out;
    uint width;
    uint height;
    uint step;
    uint mask;
};

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_check(uint3 id : SV_DispatchThreadID, uniform CheckPush push)
{
    uint cols = (push.width + push.step - 1) / push.step;
    uint rows = (push.height + push.step - 1) / push.step;
    if (id.x >= cols || id.y >= rows)
    {
        return;
    }
    float2 px = float2(id.xy * push.step) + 0.5;
    float2 ndc = float2(px.x / float(push.width) * 2.0 - 1.0, 1.0 - px.y / float(push.height) * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, 1.0, 1.0));
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    RayDesc ray;
    ray.Origin = eye;
    ray.Direction = normalize(h.xyz / h.w - eye);
    ray.TMin = 0.0;
    ray.TMax = 100000.0;
    RayQuery<RAY_FLAG_NONE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(push.accel)], RAY_FLAG_NONE, push.mask, ray);
    while (q.Proceed())
    {
        if (q.CandidateType() == CANDIDATE_NON_OPAQUE_TRIANGLE
            && alpha_kept(push.statics, push.objects, push.indices, push.vertices, push.materials,
                          push.sampler, q.CandidateInstanceID(), q.CandidateGeometryIndex(),
                          q.CandidatePrimitiveIndex(), q.CandidateTriangleBarycentrics()))
        {
            q.CommitNonOpaqueTriangleHit();
        }
    }
    bool hit = q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    float t = hit ? q.CommittedRayT() : -1.0;
    uint custom = hit ? q.CommittedInstanceID() : 0xFFFFFFFF;
    rw_buffers[NonUniformResourceIndex(push.out)].Store2((id.y * cols + id.x) * 8, uint2(asuint(t), custom));
}
