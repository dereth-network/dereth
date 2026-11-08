// How much of the sun reaches each pixel the sun lights, found by tracing rays toward it through
// the scene's acceleration structures: the ray-traced stand-in for the cascades' look-up, writing
// the same target.
//
// Each pixel lit by the sun (the ground and what stands outdoors) casts a few rays toward the
// sun's disc, each turned by the pixel's own place in a fixed pattern, so a still view draws a
// still picture. A ray is moved off its surface toward the sun's side, more the more the surface
// turns from the sun. Alpha-tested surfaces are tested at the texel the ray meets, as the G-buffer
// tests them. The ground's own structure is retail's surface: a ray from the ground looks for it
// only past a meter, so the ground drawn over retail's never shadows itself. The shadows fade in
// as the sun rises off the horizon, as the cascades' do.
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
    // The rays a pixel casts, and the shadows' strength (a float's bits): 0 with the sun on the
    // horizon, 1 with it well up.
    uint rays;
    uint strength;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The sun's disc's angular radius, radians, a little wider than the sun's own.
static const float SUN_RADIUS = 0.005;
static const float TAU = 6.2831853;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

// A number from 0 to 1 that the pixel `p` takes in a fixed pattern spread evenly over every few
// neighbours.
float pattern(int2 p)
{
    return frac(52.9829189 * frac(0.06711056 * float(p.x) + 5.83715e-3 * float(p.y)));
}

// Whether anything the `mask` names stands along `ray` between `tmin` and `tmax`.
bool blocked(RayDesc ray, uint mask)
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
    return q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
}

[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    float strength = asfloat(push.strength);
    float3 to_sun = row(push.frame, FRAME_SUN).xyz;
    if ((cls != CLASS_GROUND && cls != CLASS_SUN) || strength <= 0.0 || dot(to_sun, to_sun) < 1e-12)
    {
        return float4(1.0, 1.0, 1.0, 1.0);
    }
    float d = at(push.outdoor_depth, p).x;
    if (d >= 1.0)
    {
        return float4(1.0, 1.0, 1.0, 1.0);
    }
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    float3 world = h.xyz / h.w;
    float3 n = oct_decode(at(push.normal, p).xy);
    float3 sun = normalize(to_sun);
    float facing = dot(n, sun);
    float3 side = facing < 0.0 ? -n : n;
    float dist = length(world - row(push.frame, FRAME_EYE).xyz);
    float3 origin = world + side * ((0.02 + 0.0005 * dist) * (1.0 + 2.0 * (1.0 - abs(facing))));
    // Two directions across the sun's disc.
    float3 across = normalize(cross(sun, abs(sun.z) < 0.99 ? float3(0.0, 0.0, 1.0) : float3(1.0, 0.0, 0.0)));
    float3 up = cross(across, sun);
    uint rays = max(push.rays, 1u);
    float turn = pattern(p) * TAU;
    float lit = 0.0;
    for (uint k = 0; k < rays; k++)
    {
        // A Vogel spiral over the disc, turned by the pixel's place in the pattern.
        float r = SUN_RADIUS * sqrt((float(k) + 0.5) / float(rays));
        float a = float(k) * 2.39996323 + turn;
        RayDesc ray;
        ray.Origin = origin;
        ray.Direction = normalize(sun + across * (cos(a) * r) + up * (sin(a) * r));
        ray.TMin = 0.0;
        ray.TMax = 4000.0;
        if (blocked(ray, 0xFF & ~RT_GROUND))
        {
            continue;
        }
        ray.TMin = cls == CLASS_GROUND ? 1.0 : 0.05;
        if (blocked(ray, RT_GROUND))
        {
            continue;
        }
        lit += 1.0;
    }
    float v = lerp(1.0, lit / float(rays), strength);
    return float4(v, v, v, 1.0);
}
