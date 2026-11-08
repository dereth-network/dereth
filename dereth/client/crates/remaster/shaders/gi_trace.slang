// The probes' rays (see `gi.slang`), where the device traces rays: each probe's, one thread a
// ray, into the scene's top-level acceleration structure (`rt.slang`), alpha-tested hits tested
// as the G-buffer tests them. Where a ray meets a surface it takes the light the surface sends on:
// its own color (its texture, far off) times the direct light on it, by retail's class rules (the
// sun's on the ground and what it lights, by a ray toward the sun; a room's burned and dynamic
// lights; an indoor object's light set), and a share of the bounced light already there. A ray
// that meets the back of a one-sided surface takes nothing. Each ray's light and distance are
// written for the update.
import bindless;
import gi;
import lift;
import retail;
import rt;

struct TracePush
{
    uint frame;
    uint gi;
    uint probes;
    uint rays;
    uint count;
    uint accel;
    uint statics;
    uint objects;
    uint indices;
    uint vertices;
    uint materials;
    uint sampler;
    uint instances;
    uint ground_vertices;
    uint ground_cells;
    uint lights;
    uint sets;
    // The acceleration structures' entries, the statics' and the objects', and how many
    // statics: an instance's place in the top-level structure finds its entry.
    uint static_entries;
    uint object_entries;
    uint static_count;
};

// Whether anything stands between `x` and the sun.
bool sunlit(uint accel, float3 x, float3 n, float3 sun)
{
    RayDesc ray;
    ray.Origin = x + n * 0.05;
    ray.Direction = normalize(sun);
    ray.TMin = 0.0;
    ray.TMax = 2000.0;
    RayQuery<RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_FORCE_OPAQUE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(accel)],
                     RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_FORCE_OPAQUE,
                     0xFF & ~RT_BODY, ray);
    q.Proceed();
    return q.CommittedStatus() == COMMITTED_NOTHING;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_trace(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID, uniform TracePush push)
{
    uint p = group.x;
    uint k = thread.x;
    uint rays = min(asuint(row(push.gi, GI_SIZE).w), 64u);
    if (p >= push.count || k >= rays)
    {
        return;
    }
    int3 cell;
    float3 place;
    uint room;
    probe_place(push.gi, p, cell, place, room);
    float3 dir = ray_direction(push.gi, k, rays);
    float far = row(push.gi, GI_BOUNCE).y;
    RayDesc ray;
    ray.Origin = place;
    ray.Direction = dir;
    ray.TMin = 0.02;
    ray.TMax = far;
    RayQuery<RAY_FLAG_NONE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(push.accel)], RAY_FLAG_NONE, 0xFF & ~RT_BODY, ray);
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
    RWByteAddressBuffer o = rw_buffers[NonUniformResourceIndex(push.rays)];
    uint at = (p * 64 + k) * 16;
    if (q.CommittedStatus() != COMMITTED_TRIANGLE_HIT)
    {
        o.Store4(at, asuint(float4(0.0, 0.0, 0.0, far)));
        return;
    }
    float t = q.CommittedRayT();
    uint4 g = geometry_row(push.statics, push.objects, q.CommittedInstanceID(),
                           q.CommittedGeometryIndex());
    uint prim = q.CommittedPrimitiveIndex();
    float2 bary = q.CommittedTriangleBarycentrics();
    float w[3] = { 1.0 - bary.x - bary.y, bary.x, bary.y };
    float3x4 to_world = q.CommittedObjectToWorld3x4();
    float3 x = place + dir * t;
    float3 n;
    float3 albedo;
    uint cls;
    uint set = 0;
    bool front = q.CommittedTriangleFrontFace();
    if ((g.w & RT_IS_GROUND) != 0)
    {
        // The ground: the cell's corners' normals, and its picture's color across the cell.
        uint side = g.w >> 8;
        uint c = prim / 2;
        uint i = c / side;
        uint j = c % side;
        uint4 cw = row_u(push.ground_cells, g.z + c);
        bool cut = (cw.z & 1) != 0;
        uint svc = side + 1;
        uint sw = i * svc + j;
        uint se = (i + 1) * svc + j;
        uint ne = (i + 1) * svc + j + 1;
        uint nw = i * svc + j + 1;
        uint3 tri = (prim & 1) == 0 ? (cut ? uint3(sw, se, ne) : uint3(sw, se, nw))
                                    : (cut ? uint3(sw, ne, nw) : uint3(ne, nw, se));
        n = float3(0.0, 0.0, 0.0);
        for (uint v = 0; v < 3; v++)
        {
            n += row(push.ground_vertices, (g.y + tri[v]) * 2 + 1).xyz * w[v];
        }
        n = normalize(n);
        albedo = float3(0.3, 0.3, 0.25);
        if (cw.x != 0xFFFFFFFF)
        {
            albedo = textures[NonUniformResourceIndex(cw.x)]
                         .SampleLevel(samplers[NonUniformResourceIndex(push.sampler)], float2(0.5, 0.5), 6.0)
                         .rgb;
        }
        cls = CLASS_GROUND;
        front = true;
    }
    else
    {
        // A mesh: its corners' normals turned into the world, and its texture far off.
        ByteAddressBuffer ib = buffers[NonUniformResourceIndex(push.indices)];
        float3 nl = float3(0.0, 0.0, 0.0);
        float2 uv = float2(0.0, 0.0);
        float3 tint = float3(0.0, 0.0, 0.0);
        for (uint v = 0; v < 3; v++)
        {
            uint vi = g.y + ib.Load((g.x + prim * 3 + v) * 4);
            float4 r0 = row(push.vertices, vi * 4);
            float4 r1 = row(push.vertices, vi * 4 + 1);
            nl += float3(r0.w, r1.x, r1.y) * w[v];
            uv += r1.zw * w[v];
            tint += row(push.vertices, vi * 4 + 3).rgb * w[v];
        }
        n = normalize(mul(to_world, float4(nl, 0.0)).xyz);
        if (!front)
        {
            n = -n;
        }
        uint4 m = row_u(push.materials, g.z * 2);
        albedo = float3(1.0, 1.0, 1.0);
        if ((m.y & 64) != 0)
        {
            albedo = textures[NonUniformResourceIndex(m.x)]
                         .SampleLevel(samplers[NonUniformResourceIndex(push.sampler)], uv, 3.0)
                         .rgb;
        }
        // A surface drawn under its vertices' colors.
        if ((m.y & 512) != 0)
        {
            albedo *= tint;
        }
        // The instance's class and light set, from its entry.
        uint index = q.CommittedInstanceIndex();
        uint4 e = index < push.static_count
                      ? row_u(push.static_entries, index * 2)
                      : row_u(push.object_entries, (index - push.static_count) * 2);
        uint4 info = row_u(push.instances, e.x * INSTANCE_ROWS + 5);
        cls = info.y;
        set = info.z;
    }
    if (!front && (g.w & RT_TWO_SIDED) == 0)
    {
        // The back of a one-sided surface: inside something.
        o.Store4(at, asuint(float4(0.0, 0.0, 0.0, t)));
        return;
    }
    float3 direct = float3(0.0, 0.0, 0.0);
    if (cls == CLASS_GROUND || cls == CLASS_SUN)
    {
        float3 sun = row(push.frame, FRAME_SUN).xyz;
        float facing = dot(n, sun);
        if (facing > 0.0 && sunlit(push.accel, x, n, sun))
        {
            direct = row(push.frame, FRAME_SUN_COLOR).rgb * facing;
        }
    }
    else if (cls == CLASS_CELL)
    {
        direct = burn(push.lights, uint(row(push.frame, FRAME_SCREEN).w), x, n)
                 + ff_lights(push.lights, push.sets, 0, x, n);
    }
    else if (cls == CLASS_INDOOR)
    {
        direct = ff_lights(push.lights, push.sets, set, x, n);
    }
    float share = row(push.gi, GI_BOUNCE).x;
    float3 again = bounced(push.gi, push.probes, cls, x, n, -dir) * share;
    float3 light = albedo * (direct + again);
    o.Store4(at, asuint(float4(max(light, 0.0), t)));
}
