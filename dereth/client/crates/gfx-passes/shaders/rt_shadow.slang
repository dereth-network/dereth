// How much of the sun reaches each pixel the sun lights, found by tracing rays toward it through
// the scene's acceleration structures: the ray-traced stand-in for the cascades' look-up, writing
// the same target.
//
// Each pixel lit by the sun (the ground and what stands outdoors) casts a few rays toward the
// sun's disc, each turned by the pixel's own place in a fixed pattern, so a still view draws a
// still picture. A ray is moved off its surface toward the sun's side, more the more the surface
// turns from the sun. Alpha-tested surfaces are tested at the texel the ray meets, as the G-buffer
// tests them. The ground's own structure is the surface the game lights its ground by: a ray
// from the ground looks for it only past a meter, so ground drawn over that surface never shadows
// itself. Past the near ground, a ray meets that surface only from above, so neither far ground
// drawn coarser than it nor a far point's barely known depth shades the far land itself. The
// shadows fade in as the sun rises off the horizon, as the cascades' do.
//
// The scene's structures are the game's own, and so is the module that reads them: `rt`, found
// with the game's shaders, names the masks a ray asks for (`RT_GROUND` the ground's structure,
// `RT_COVER` ground cover) and `hit_kept`, whether a query keeps a candidate hit on a surface it
// is handed (an alpha-tested or a swaying one), read from the game's tables and pools below.
import bindless;
import frame;
import rt;

struct Push
{
    uint frame;
    uint depth;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
    uint normal;
    uint material;
    // The top-level structure, and the game's two tables of its instances' rows.
    uint accel;
    uint statics;
    uint objects;
    // The game's four pools its rows name (its indices, vertices and materials, and the sampler
    // alpha tests read with, say).
    uint indices;
    uint vertices;
    uint materials;
    uint sampler;
    // The rays a pixel casts, and the shadows' strength (a float's bits): 0 with the sun on the
    // horizon, 1 with it well up.
    uint rays;
    uint strength;
    // How far along the view ground cover casts (a float's bits): the nearest cascade's reach,
    // fading out over its last fifth as the cascades' look-up blends it into the next, which has
    // none.
    uint cover_reach;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The sun's disc's angular radius, radians, a little wider than the sun's own.
static const float SUN_RADIUS = 0.005;
static const float TAU = 6.2831853;
// Where the near ground gives way to the far, meters from the eye, where the frame draws no far
// ground of its own: the drawn ground's own defaults.
static const float2 NEAR_GROUND = float2(300.0, 500.0);

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
        float t;
        if (q.CandidateType() == CANDIDATE_NON_OPAQUE_TRIANGLE
            && hit_kept(push.frame, push.statics, push.objects, push.indices, push.vertices,
                        push.materials, push.sampler, q.CandidateInstanceID(),
                        q.CandidateGeometryIndex(), q.CandidatePrimitiveIndex(),
                        q.CandidateTriangleBarycentrics(), ray.Origin, ray.Direction, ray.TMin,
                        ray.TMax, q.CandidateObjectToWorld3x4(), t))
        {
            q.CommitNonOpaqueTriangleHit();
        }
    }
    return q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
}

// Whether the ground's structure stands along `ray` between its ends, met on its upper face. A
// ray that starts under it leaves through its underside, which hides nothing.
bool ground_above(RayDesc ray)
{
    RayQuery<RAY_FLAG_FORCE_NON_OPAQUE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(push.accel)], RAY_FLAG_FORCE_NON_OPAQUE,
                     RT_GROUND, ray);
    while (q.Proceed())
    {
        // The ground's triangles turn their front faces up.
        if (q.CandidateTriangleFrontFace())
        {
            q.CommitNonOpaqueTriangleHit();
            q.Abort();
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
    float ahead = transform(push.frame, FRAME_VIEW, float4(world, 1.0)).z;
    float reach = asfloat(push.cover_reach);
    float cover = 1.0 - saturate((ahead - 0.8 * reach) / max(0.2 * reach, 1e-3));
    // Past the near ground (by the pixel's pattern across the stretch where it gives way to the
    // far) a ray meets the ground's structure only from above: ground drawn there may be coarser
    // than it, lying under it on every crest, and a far point's depth is known only to meters,
    // so a far point may stand under the structure, and leaving through its underside would
    // shade it, differently from frame to frame. Nearer, ground drawn under the structure (a shore
    // lowered under the water) is shaded by its rim, as the drawn ground shades it.
    float4 lift = row(push.frame, FRAME_LIFT);
    float2 band = lift.z > 0.5 ? lift.xy : NEAR_GROUND;
    float far = smoothstep(band.x, max(band.y, band.x + 1e-3), dist);
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
        // Each ray takes the cover in as the share of it left here says, by the pixel's pattern.
        bool with_cover = frac(pattern(p) + (float(k) + 0.5) / float(rays)) < cover;
        uint mask = 0xFF & ~RT_GROUND & (with_cover ? 0xFF : ~RT_COVER);
        if (blocked(ray, mask))
        {
            continue;
        }
        ray.TMin = cls == CLASS_GROUND ? 1.0 : 0.05;
        bool from_above = frac(pattern(p) * 7.0 + (float(k) + 0.5) / float(rays)) < far;
        if (from_above ? ground_above(ray) : blocked(ray, RT_GROUND))
        {
            continue;
        }
        lit += 1.0;
    }
    float v = lerp(1.0, lit / float(rays), strength);
    return float4(v, v, v, 1.0);
}
