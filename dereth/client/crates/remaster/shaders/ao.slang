// Ambient occlusion: how much of the sky each pixel's surroundings leave it, as ground-truth
// ambient occlusion finds it from the depth, at half the world's resolution; then
// gathered over the frames while the view holds still, and brought up to the full resolution
// along the depth's edges. 1 is open sky, 0 fully shut in. The resolve takes the ambient light by
// it, and nothing else.
//
// - `fs_gtao`: each half-resolution pixel's occlusion from this frame's samples: two slices of
//   the hemisphere about the view direction, each searched both ways for its highest horizon, the
//   arcs between the horizons integrated against the normal. Each frame turns the slices and
//   moves the steps by a different amount, from a sequence of a few frames.
// - `fs_gather`: this frame's occlusion laid over what the pixel had last frame, found where the
//   pixel was last frame; a pixel new to the view, or whose surface moved, starts again. Once a
//   pixel has seen every frame of the sequence with the view held still it keeps what it has, so
//   a still view settles to a still picture.
// - `fs_upsample`: each full-resolution pixel from the half-resolution pixels round it, each
//   weighed by how near it is and by how near its depth is to the pixel's.
import bindless;
import retail;

// This pass's own constants, in buffer `ao` as float4 rows:
//   0..3  the last frame's view and projection together, as four columns
//   4     x the radius searched (metres), y where the falloff begins (a share of the radius),
//         z the strength (a power on the occlusion), w the frame's place in the sequence
//   5     the half-resolution extent, xy; z 1 when the view held still since the last frame;
//         w 1 when rooms and what is in them take the occlusion
//   6     x the frames of the sequence, y the slices, z the steps each way, w 1 when nothing
//         has been gathered yet
static const uint AO_PREVIOUS = 0;
static const uint AO_SEARCH = 4;
static const uint AO_HALF = 5;
static const uint AO_SEQUENCE = 6;

static const float PI = 3.14159265;

struct Push
{
    uint frame;
    uint ao;
    uint depth;
    uint material;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
    // `fs_gather`: this frame's occlusion and the last frame's gathered. `fs_upsample`: the
    // gathered occlusion.
    uint raw;
    uint history;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

uint class_at(int2 p)
{
    return uint(at(push.material, p).x * 255.0 + 0.5);
}

// Whether a pixel of class `cls` takes the occlusion.
bool occluded(uint cls)
{
    if (cls == CLASS_NONE || cls == CLASS_SKY)
    {
        return false;
    }
    bool inside = cls == CLASS_INDOOR || cls == CLASS_CELL;
    return !inside || row(push.ao, AO_HALF).w > 0.5;
}

// The depth texture a pixel of class `cls` is placed by, as the resolve places it.
uint depth_of(uint cls)
{
    return (cls == CLASS_GROUND || cls == CLASS_SUN) ? push.outdoor_depth : push.depth;
}

// A full-resolution pixel position's place in the frame's space, at depth `d`.
float3 world_at(float2 pos, float d)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(pos.x / extent.x * 2.0 - 1.0, 1.0 - pos.y / extent.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return h.xyz / h.w;
}

// A point of the frame's space on the full-resolution target, in pixels.
float2 pixel_of(float3 world)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float4 c = transform(push.frame, FRAME_VIEW_PROJ, float4(world, 1.0));
    float2 ndc = c.xy / c.w;
    return float2((ndc.x + 1.0) * 0.5 * extent.x, (1.0 - ndc.y) * 0.5 * extent.y);
}

// The full-resolution pixel `q`'s place in the frame's space, by depth texture `dt`.
float3 world_of(uint dt, int2 q)
{
    return world_at(float2(q) + 0.5, at(dt, q).x);
}

// The normal of the surface at pixel `p` (at `world`), from the depth of its neighbours: each
// way, the neighbour nearer in depth; facing the view direction `v`.
float3 depth_normal(uint dt, int2 p, float3 world, float3 v)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    int2 last = int2(extent) - 1;
    float3 r = world_of(dt, min(p + int2(1, 0), last)) - world;
    float3 l = world - world_of(dt, max(p - int2(1, 0), int2(0, 0)));
    float3 d = world_of(dt, min(p + int2(0, 1), last)) - world;
    float3 u = world - world_of(dt, max(p - int2(0, 1), int2(0, 0)));
    float3 dx = dot(r, r) < dot(l, l) ? r : l;
    float3 dy = dot(d, d) < dot(u, u) ? d : u;
    float3 n = cross(dx, dy);
    float len = length(n);
    if (len < 1e-12)
    {
        return v;
    }
    n /= len;
    return dot(n, v) < 0.0 ? -n : n;
}

// Interleaved gradient noise: a number in [0, 1) that differs from each pixel to the next.
float noise(float2 p)
{
    return frac(52.9829189 * frac(dot(p, float2(0.06711056, 5.83715e-3))));
}

// The arc of a slice between the view direction and a horizon at angle `h`, against a normal at
// angle `n` whose cosine is `cos_n`, cosine weighted.
float arc(float h, float n, float cos_n)
{
    return 0.25 * (-cos(2.0 * h - n) + cos_n + 2.0 * h * sin(n));
}

[shader("fragment")]
float4 fs_gtao(float4 position : SV_Position) : SV_Target0
{
    int2 half_p = int2(position.xy);
    int2 p = half_p * 2;
    uint cls = class_at(p);
    if (!occluded(cls))
    {
        return float4(1.0, 0.0, 0.0, 0.0);
    }
    uint dt = depth_of(cls);
    float2 pos = float2(p) + 0.5;
    float3 world = world_at(pos, at(dt, p).x);
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float dist = length(eye - world);
    float3 v = (eye - world) / max(dist, 1e-6);
    // The surface's own normal, from the depth round the pixel: the side nearer in depth each
    // way, so an edge takes the surface it is on. The G-buffer's normals are smoothed across the
    // triangles while the depth is the triangles' own, and the two disagreeing would occlude a
    // flat slope by itself.
    float3 n = depth_normal(dt, p, world, v);

    float4 search = row(push.ao, AO_SEARCH);
    float4 sequence = row(push.ao, AO_SEQUENCE);
    float radius = search.x;
    float falloff = max(radius * (1.0 - search.y), 1e-4);
    float frame = search.w;
    uint slices = uint(sequence.y);
    uint steps = uint(sequence.z);
    float2 extent = row(push.frame, FRAME_EXTENT).xy;

    // How many pixels the radius spans here: a point the radius off along the view's up.
    float3 up = normalize(float3(row(push.frame, FRAME_VIEW).y, row(push.frame, FRAME_VIEW + 1).y,
        row(push.frame, FRAME_VIEW + 2).y));
    float span = length(pixel_of(world + up * radius) - pos);
    if (span < 1.0)
    {
        return float4(1.0, dist, 0.0, 0.0);
    }

    float turn = noise(float2(half_p) + frame * 5.588238);
    float shift = frac(noise(float2(half_p.yx) + frame * 7.239) + 0.5);
    float visibility = 0.0;
    for (uint s = 0; s < slices; s++)
    {
        float phi = (float(s) + turn) * PI / float(slices);
        float2 dir = float2(cos(phi), sin(phi));
        // The slice's direction in the frame's space: toward the point one pixel along `dir`.
        float3 along = world_at(pos + dir, at(dt, p).x) - world;
        float3 ortho = along - dot(along, v) * v;
        float ortho_len = length(ortho);
        if (ortho_len < 1e-8)
        {
            visibility += 1.0;
            continue;
        }
        ortho /= ortho_len;
        float3 axis = cross(ortho, v);
        float3 proj_n = n - axis * dot(n, axis);
        float proj_len = length(proj_n);
        if (proj_len < 1e-6)
        {
            visibility += 1.0;
            continue;
        }
        float cos_n = saturate(dot(proj_n, v) / proj_len);
        float n_angle = (dot(ortho, proj_n) >= 0.0 ? 1.0 : -1.0) * acos(cos_n);
        float low0 = cos(n_angle - 0.5 * PI);
        float low1 = cos(n_angle + 0.5 * PI);
        float h_cos0 = low1;
        float h_cos1 = low0;
        for (uint k = 0; k < steps; k++)
        {
            float t = (float(k) + shift) / float(steps);
            t = t * t;
            float2 offset = dir * max(t * span, float(k + 1));
            for (int side = 0; side < 2; side++)
            {
                float2 q = pos + (side == 0 ? offset : -offset);
                if (any(q < 0.0) || any(q >= extent))
                {
                    continue;
                }
                int2 qi = int2(q);
                float3 sample_world = world_at(float2(qi) + 0.5, at(dt, qi).x);
                float3 delta = sample_world - world;
                float len = length(delta);
                if (len < 1e-5)
                {
                    continue;
                }
                float h = dot(delta / len, v);
                float weight = saturate((radius - len) / falloff);
                if (side == 0)
                {
                    h_cos0 = max(h_cos0, lerp(low1, h, weight));
                }
                else
                {
                    h_cos1 = max(h_cos1, lerp(low0, h, weight));
                }
            }
        }
        // The horizons either side, kept within the hemisphere about the normal.
        float h1 = n_angle + min(acos(clamp(h_cos0, -1.0, 1.0)) - n_angle, 0.5 * PI);
        float h0 = n_angle + max(-acos(clamp(h_cos1, -1.0, 1.0)) - n_angle, -0.5 * PI);
        visibility += proj_len * (arc(h0, n_angle, cos_n) + arc(h1, n_angle, cos_n));
    }
    // Not clamped yet: one frame's few slices over-count some directions and under-count others,
    // and only their sum over the frames is the hemisphere's. Clamping each frame's would darken
    // open ground.
    return float4(visibility / float(slices), dist, 0.0, 0.0);
}

[shader("fragment")]
float4 fs_gather(float4 position : SV_Position) : SV_Target0
{
    int2 half_p = int2(position.xy);
    float4 now = at(push.raw, half_p);
    int2 p = half_p * 2;
    uint cls = class_at(p);
    if (!occluded(cls))
    {
        return float4(1.0, now.y, 0.0, 0.0);
    }
    float4 half_row = row(push.ao, AO_HALF);
    float4 sequence = row(push.ao, AO_SEQUENCE);
    float frames = sequence.x;
    if (sequence.w > 0.5)
    {
        return float4(now.x, now.y, 1.0, 0.0);
    }
    bool still = half_row.z > 0.5;
    // Where this pixel's surface was last frame, in half-resolution pixels.
    float3 world = world_at(float2(p) + 0.5, at(depth_of(cls), p).x);
    float4 c = transform(push.ao, AO_PREVIOUS, float4(world, 1.0));
    float2 ndc = c.xy / c.w;
    float2 half_extent = half_row.xy;
    float2 last = float2((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5) * half_extent;
    int2 li = still ? half_p : int2(floor(last));
    if (c.w <= 0.0 || any(li < 0) || any(float2(li) >= half_extent))
    {
        return float4(now.x, now.y, 1.0, 0.0);
    }
    float4 had = at(push.history, li);
    // Last frame's surface there is this one only when it stood at the same distance.
    float eye_dist = now.y;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float moved = abs(had.y - (still ? eye_dist : length(eye - world)));
    if (had.z < 1.0 || moved > max(eye_dist, 1.0) * 0.02)
    {
        return float4(now.x, now.y, 1.0, 0.0);
    }
    if (still && had.z >= frames)
    {
        // Every frame of the sequence is in: keep what it has.
        return float4(had.x, now.y, had.z, 0.0);
    }
    float count = min(had.z + 1.0, frames);
    return float4(lerp(had.x, now.x, 1.0 / count), now.y, count, 0.0);
}

[shader("fragment")]
float4 fs_upsample(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = class_at(p);
    if (!occluded(cls))
    {
        return float4(1.0, 0.0, 0.0, 0.0);
    }
    float3 world = world_at(float2(p) + 0.5, at(depth_of(cls), p).x);
    float dist = length(row(push.frame, FRAME_EYE).xyz - world);
    float2 half_extent = row(push.ao, AO_HALF).xy;
    // The four by four half-resolution pixels round this one, each weighed by how near it is in
    // place (a tent two half-resolution pixels wide each way, which also smooths what is left of
    // the samples' turning) and by how near its distance is to this pixel's.
    float2 h = (float2(p) + 0.5) * 0.5 - 0.5;
    int2 base = int2(floor(h)) - 1;
    float sum = 0.0;
    float weight = 0.0;
    for (int j = 0; j < 4; j++)
    {
        for (int i = 0; i < 4; i++)
        {
            int2 q = clamp(base + int2(i, j), int2(0, 0), int2(half_extent) - 1);
            float4 g = at(push.history, q);
            float2 off = abs(float2(base + int2(i, j)) - h);
            float place = max(2.0 - off.x, 0.0) * max(2.0 - off.y, 0.0);
            float near = 1.0 / (1e-3 + abs(g.y - dist) / max(dist, 1.0));
            float w = place * near + 1e-6;
            sum += g.x * w;
            weight += w;
        }
    }
    return float4(pow(saturate(sum / weight), row(push.ao, AO_SEARCH).z), 0.0, 0.0, 0.0);
}
