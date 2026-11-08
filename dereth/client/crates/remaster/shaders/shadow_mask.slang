// How much of the sun reaches each pixel the sun lights: its place looked up in the cascade that
// covers it, compared against the casters' depth there with a soft filter, blended into the next
// cascade near a cascade's end, and faded out toward the shadows' reach.
import bindless;
import retail;

struct Push
{
    uint frame;
    uint depth;
    uint outdoor_depth;
    uint normal;
    uint material;
    // The cascades: four matrices, then the far end of each, each texel's size in metres, each
    // map's index in the table, (fade start, fade end, strength, the maps' size), and each one's
    // depth per metre.
    uint cascades;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

static const uint CASCADES = 4;
static const uint ROW_FAR = 16;
static const uint ROW_TEXEL = 17;
static const uint ROW_MAPS = 18;
static const uint ROW_FADE = 19;
static const uint ROW_DEPTH = 20;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

// The map's depth at texel `p`, held at its edges.
float map_depth(uint map, int2 p, int size)
{
    return at(map, clamp(p, int2(0, 0), int2(size - 1, size - 1))).x;
}

// The share of the sun at `uv` in cascade `c` for a receiver at depth `z`: each of the nine
// texels around it compared, bilinearly weighted, over a 3 x 3 tent.
float filtered(uint c, float2 uv, float z)
{
    uint map = row_u(push.cascades, ROW_MAPS)[c];
    int size = int(row(push.cascades, ROW_FADE).w);
    float2 t = uv * float(size) - 0.5;
    int2 base = int2(floor(t));
    float2 f = t - floor(t);
    float lit = 0.0;
    float weight = 0.0;
    for (int dy = -1; dy <= 2; dy++)
    {
        for (int dx = -1; dx <= 2; dx++)
        {
            // The tent's weight of this texel: a 3-texel box convolved with the bilinear one.
            float wx = dx == -1 ? 1.0 - f.x : (dx == 2 ? f.x : 1.0);
            float wy = dy == -1 ? 1.0 - f.y : (dy == 2 ? f.y : 1.0);
            float w = wx * wy;
            float d = map_depth(map, base + int2(dx, dy), size);
            lit += w * (z <= d ? 1.0 : 0.0);
            weight += w;
        }
    }
    return lit / weight;
}

// The sun's share at `world` with normal `n` in cascade `c`, or -1 where the cascade doesn't
// cover it. `facing` is N.L toward the sun. The point is moved off its surface toward the sun's
// side by a texel or so, more the more the surface turns from the sun, and compared with a bias
// that grows with the surface's slope to the sun, so a surface doesn't shadow itself.
float in_cascade(uint c, float3 world, float3 n, float facing)
{
    float texel = row(push.cascades, ROW_TEXEL)[c];
    float grazing = 1.0 - abs(facing);
    float3 side = facing < 0.0 ? -n : n;
    float3 p = world + side * texel * (1.0 + 2.0 * grazing);
    float4 q = transform(push.cascades, c * 4, float4(p, 1.0));
    float2 uv = float2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    if (any(uv < 0.0) || any(uv > 1.0) || q.z < 0.0 || q.z > 1.0)
    {
        return -1.0;
    }
    float slope = sqrt(saturate(1.0 - facing * facing)) / max(abs(facing), 0.1);
    float bias = row(push.cascades, ROW_DEPTH)[c] * texel * (1.0 + min(slope, 8.0));
    return filtered(c, uv, q.z - bias);
}

[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    if (cls != CLASS_GROUND && cls != CLASS_SUN)
    {
        return float4(1.0, 1.0, 1.0, 1.0);
    }
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    float d = at(push.outdoor_depth, p).x;
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    float3 world = h.xyz / h.w;
    float3 n = oct_decode(at(push.normal, p).xy);
    float facing = dot(n, normalize(row(push.frame, FRAME_SUN).xyz));
    float ahead = transform(push.frame, FRAME_VIEW, float4(world, 1.0)).z;
    float4 fade = row(push.cascades, ROW_FADE);
    if (ahead >= fade.y)
    {
        return float4(1.0, 1.0, 1.0, 1.0);
    }
    float4 far = row(push.cascades, ROW_FAR);
    float lit = 1.0;
    for (uint c = 0; c < CASCADES; c++)
    {
        if (ahead > far[c])
        {
            continue;
        }
        float v = in_cascade(c, world, n, facing);
        if (v < 0.0)
        {
            continue;
        }
        lit = v;
        // Near the cascade's end, blend into the next one, so its edge doesn't show.
        float start = c == 0 ? 0.0 : far[c - 1];
        float blend = saturate((ahead - lerp(start, far[c], 0.8)) / max(far[c] - lerp(start, far[c], 0.8), 1e-3));
        if (blend > 0.0 && c + 1 < CASCADES)
        {
            float next = in_cascade(c + 1, world, n, facing);
            if (next >= 0.0)
            {
                lit = lerp(lit, next, blend);
            }
        }
        break;
    }
    float out_of_reach = saturate((ahead - fade.x) / max(fade.y - fade.x, 1e-3));
    lit = lerp(lit, 1.0, out_of_reach);
    lit = lerp(1.0, lit, fade.z);
    return float4(lit, lit, lit, 1.0);
}
