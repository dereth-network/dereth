// Ambient occlusion found by tracing rays through the scene's acceleration structures: the
// ray-traced stand-in for the screen-space occlusion, writing the same target. 1 is open sky, 0
// fully shut in; the resolve takes the ambient light by it.
//
// - `fs_rays`: each pixel casts a few rays over the hemisphere about its normal, spread as the
//   light from an open sky falls (more toward the normal), each turned by the pixel's own place in
//   a fixed four-by-four pattern, so a still view draws a still picture. A ray that meets
//   anything within the radius occludes, the less the farther past where the falloff begins.
//   Alpha-tested surfaces are tested at the texel the ray meets. The ground's own structure is
//   retail's surface, so the ground drawn over it never occludes itself.
// - `fs_filter`: each pixel's occlusion from the four by four pixels round it that lie on its own
//   surface (near in depth and facing the same way), which evens the pattern out.
import bindless;
import retail;
import rt;

struct Push
{
    uint frame;
    uint depth;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
    uint normal;
    uint material;
    // The top-level structure, and the geometry tables of the statics and of the objects.
    uint accel;
    uint statics;
    uint objects;
    // The scene's indices, vertices and materials, and the sampler alpha tests read with.
    uint indices;
    uint vertices;
    uint materials;
    uint sampler;
    // The rays a pixel casts; the radius searched (meters), where the falloff begins (a share of
    // the radius) and the strength (a power on the occlusion), each a float's bits; 1 when rooms
    // and what is in them take the occlusion.
    uint rays;
    uint radius;
    uint falloff;
    uint strength;
    uint indoors;
    // `fs_filter`: the occlusion `fs_rays` found.
    uint raw;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

static const float TAU = 6.2831853;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

uint class_at(int2 p)
{
    return uint(at(push.material, p).x * 255.0 + 0.5);
}

// Whether pixels of class `cls` take the occlusion: never the sky, and rooms and what is in them
// only when asked.
bool occluded(uint cls)
{
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        return false;
    }
    bool inside = cls == CLASS_INDOOR || cls == CLASS_CELL;
    return !inside || push.indoors != 0;
}

// The depth texture a pixel of class `cls` is placed by, as the resolve places it.
uint depth_of(uint cls)
{
    return (cls == CLASS_GROUND || cls == CLASS_SUN) ? push.outdoor_depth : push.depth;
}

float3 world_at(float2 pos, float d)
{
    float2 e = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(pos.x / e.x * 2.0 - 1.0, 1.0 - pos.y / e.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return h.xyz / h.w;
}

// The nearest hit of anything `mask` names along `ray`, its distance; `ray.TMax` for none.
float nearest(RayDesc ray, uint mask)
{
    RayQuery<RAY_FLAG_NONE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(push.accel)], RAY_FLAG_NONE, mask, ray);
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
    return q.CommittedStatus() == COMMITTED_TRIANGLE_HIT ? q.CommittedRayT() : ray.TMax;
}

[shader("fragment")]
float4 fs_rays(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = class_at(p);
    if (!occluded(cls))
    {
        return float4(1.0, 0.0, 0.0, 0.0);
    }
    float d = at(depth_of(cls), p).x;
    if (d >= 1.0)
    {
        return float4(1.0, 0.0, 0.0, 0.0);
    }
    float3 world = world_at(position.xy, d);
    float3 n = oct_decode(at(push.normal, p).xy);
    float radius = asfloat(push.radius);
    float start = asfloat(push.falloff) * radius;
    float dist = length(world - row(push.frame, FRAME_EYE).xyz);
    float3 origin = world + n * (0.02 + 0.0005 * dist);
    float3 t = normalize(cross(n, abs(n.z) < 0.99 ? float3(0.0, 0.0, 1.0) : float3(1.0, 0.0, 0.0)));
    float3 b = cross(n, t);
    // The ground never occludes itself: its own structure is retail's surface, not the one drawn.
    uint mask = cls == CLASS_GROUND ? (0xFF & ~RT_GROUND) : 0xFF;
    uint rays = max(push.rays, 1u);
    // The pixel's place in a four-by-four pattern, which turns its rays and moves them along it.
    uint cell = uint(p.x & 3) + 4 * uint(p.y & 3);
    float turn = float(cell) / 16.0;
    float shut = 0.0;
    for (uint k = 0; k < rays; k++)
    {
        // Spread as the light from an open sky falls: cosine-weighted, a Vogel spiral turned by
        // the pixel's place.
        float u = frac((float(k) + 0.5) / float(rays) + turn * 0.618034);
        float a = float(k) * 2.39996323 + turn * TAU;
        float s = sqrt(u);
        float3 dir = t * (cos(a) * s) + b * (sin(a) * s) + n * sqrt(max(1.0 - u, 0.0));
        RayDesc ray;
        ray.Origin = origin;
        ray.Direction = normalize(dir);
        ray.TMin = 0.0;
        ray.TMax = radius;
        float hit = nearest(ray, mask);
        if (hit < radius)
        {
            shut += 1.0 - smoothstep(start, radius, hit);
        }
    }
    float open = 1.0 - shut / float(rays);
    return float4(pow(saturate(open), asfloat(push.strength)), 0.0, 0.0, 0.0);
}

[shader("fragment")]
float4 fs_filter(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = class_at(p);
    if (!occluded(cls))
    {
        return float4(1.0, 0.0, 0.0, 0.0);
    }
    int2 size = int2(row(push.frame, FRAME_EXTENT).xy);
    float d = at(depth_of(cls), p).x;
    float3 world = world_at(position.xy, d);
    float z = length(world - row(push.frame, FRAME_EYE).xyz);
    float3 n = oct_decode(at(push.normal, p).xy);
    float sum = 0.0;
    float weight = 0.0;
    for (int dy = -1; dy <= 2; dy++)
    {
        for (int dx = -1; dx <= 2; dx++)
        {
            int2 q = clamp(p + int2(dx, dy), int2(0, 0), size - 1);
            uint c = class_at(q);
            if (!occluded(c))
            {
                continue;
            }
            float3 wq = world_at(float2(q) + 0.5, at(depth_of(c), q).x);
            float zq = length(wq - row(push.frame, FRAME_EYE).xyz);
            float3 nq = oct_decode(at(push.normal, q).xy);
            if (abs(zq - z) > 0.03 * z + 0.05 || dot(nq, n) < 0.8)
            {
                continue;
            }
            sum += at(push.raw, q).x;
            weight += 1.0;
        }
    }
    return float4(weight > 0.0 ? sum / weight : at(push.raw, p).x, 0.0, 0.0, 0.0);
}
