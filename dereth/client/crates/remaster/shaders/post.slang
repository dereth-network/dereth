// The post passes, from the scene's linear colors to the display values the frame shows:
//
// - `fs_taa`: temporal anti-aliasing. Each frame draws the world through a projection moved by a
//   different part of a pixel, from a sequence of a few frames; this pass lays the frame over
//   what the pixel had, found where its surface was last frame (from the moving objects' own
//   last positions where they moved, from the depth and last frame's view elsewhere), held to
//   the colors round the pixel so nothing stale is kept. While the view holds still a pixel
//   gathers every frame of the sequence with equal weight; once it has, and the frame at its
//   place in the sequence draws the same as it did a sequence ago, the pixel keeps what it has, so
//   a still view settles to a still picture while what moves or changes in it does not.
// - `fs_taa_check`: to measure the motion, last frame's picture found where each pixel's surface
//   was, beside this frame's kept for the next.
// - `fs_bloom_down` and `fs_bloom_up`: the bloom, a chain of ever smaller pictures of the frame,
//   each blurred from the one before, then added back up the chain.
// - `fs_tonemap`: the bloom laid in, the exposure, a shoulder to display values, and the grade.
import bindless;
import retail;

// This pass's own constants, in buffer `post` as float4 rows:
//   0..3  the last frame's view and projection together, unmoved, in this frame's space
//   4     xy how far this frame's projection is moved, in clip space; z 1 when the view held still
//         since the last frame; w 1 when there is no last frame to gather from
//   5     x the frames of the sequence, y this frame's place in it
//   6     x the exposure's scale, y the bloom's share, z 1 when the colors are linear and tone
//         mapped, w the bloom's levels
//   7     the grade: x its saturation, y its contrast, z its vignette
static const uint POST_PREVIOUS = 0;
static const uint POST_STATE = 4;
static const uint POST_TAA = 5;
static const uint POST_TONE = 6;
static const uint POST_GRADE = 7;

struct Push
{
    uint frame;
    uint post;
    // The scene's colors, the depth, the outdoor surfaces' depth and the material target.
    uint scene;
    uint depth;
    uint outdoor_depth;
    uint material;
    // The moving objects' last places, in clip space halved, -1 where nothing moved (~0: none
    // moved).
    uint motion;
    // Last frame's history, and the frames' marks at each place in the sequence.
    uint history;
    uint marks;
    // The bloom passes and the tone map: the picture read, its size, and the sampler.
    uint source;
    uint sampler;
    float source_width;
    float source_height;
    // The tone map: the bloom (~0: none).
    uint bloom;
    // The bloom's first step down: 1 to weigh each box by its brightness.
    uint first;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

float4 sampled(uint t, float2 uv)
{
    return sample(t, push.sampler, uv);
}

uint class_at(int2 p)
{
    return uint(at(push.material, p).x * 255.0 + 0.5);
}

// The depth texture a pixel of class `cls` is placed by, as the resolve places it.
uint depth_of(uint cls)
{
    return (cls == CLASS_GROUND || cls == CLASS_SUN) ? push.outdoor_depth : push.depth;
}

// A pixel position's place in the frame's space, at depth `d`.
float3 world_at(float2 pos, float d)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(pos.x / extent.x * 2.0 - 1.0, 1.0 - pos.y / extent.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return h.xyz / h.w;
}

// A point in clip space's place on the target, in pixels.
float2 pixel_of_ndc(float2 ndc)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    return float2((ndc.x + 1.0) * 0.5 * extent.x, (1.0 - ndc.y) * 0.5 * extent.y);
}

// A color's brightness, Rec. 709's weights.
float luma(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// The colors the frames are laid over each other in: each brought under 1 by its brightness, so
// one bright pixel does not outweigh the rest; and back.
float3 compress(float3 c)
{
    return c / (1.0 + luma(c));
}

float3 expand(float3 c)
{
    return c / max(1.0 - luma(c), 1e-4);
}

float3 to_ycocg(float3 c)
{
    return float3(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b,
                  -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}

float3 from_ycocg(float3 c)
{
    return float3(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

// Texture `t` (`size` pixels) at `pos` (pixels), by Catmull-Rom's curve through the four by four
// texels round it, from nine bilinear samples: sharper than one bilinear sample, so what is laid
// over itself frame after frame does not blur.
float3 catmull_rom(uint t, float2 pos, float2 size)
{
    float2 p1 = floor(pos - 0.5) + 0.5;
    float2 f = pos - p1;
    float2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    float2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    float2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    float2 w3 = f * f * (-0.5 + 0.5 * f);
    float2 w12 = w1 + w2;
    float2 p0 = (p1 - 1.0) / size;
    float2 p3 = (p1 + 2.0) / size;
    float2 p12 = (p1 + w2 / w12) / size;
    float3 c = float3(0.0, 0.0, 0.0);
    c += sampled(t, float2(p0.x, p0.y)).rgb * w0.x * w0.y;
    c += sampled(t, float2(p12.x, p0.y)).rgb * w12.x * w0.y;
    c += sampled(t, float2(p3.x, p0.y)).rgb * w3.x * w0.y;
    c += sampled(t, float2(p0.x, p12.y)).rgb * w0.x * w12.y;
    c += sampled(t, float2(p12.x, p12.y)).rgb * w12.x * w12.y;
    c += sampled(t, float2(p3.x, p12.y)).rgb * w3.x * w12.y;
    c += sampled(t, float2(p0.x, p3.y)).rgb * w0.x * w3.y;
    c += sampled(t, float2(p12.x, p3.y)).rgb * w12.x * w3.y;
    c += sampled(t, float2(p3.x, p3.y)).rgb * w3.x * w3.y;
    return max(c, 0.0);
}

// A mark of a color, 1 to 15, from its bits: two frames that draw a pixel the same give it the
// same mark, and two that draw it differently different ones but one time in fifteen (when the
// pixel keeps last frame's color a frame longer). 0 is no mark.
uint mark(float3 c)
{
    uint3 u = asuint(c);
    uint h = u.x * 0x9E3779B1u ^ u.y * 0x85EBCA77u ^ u.z * 0xC2B2AE3Du;
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12;
    h *= 0x297A2D39u;
    h ^= h >> 15;
    return 1 + h % 15;
}

// Where pixel `p`'s surface was last frame, on last frame's picture in pixels, and whether it was
// on it; and whether it is a moving object's.
struct Found
{
    float2 last;
    bool found;
    bool moving;
};

Found found_at(int2 p)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float4 state = row(push.post, POST_STATE);
    uint cls = class_at(p);
    float2 motion = push.motion != 0xFFFFFFFF ? at(push.motion, p).xy : float2(-1.0, -1.0);
    Found f;
    f.moving = motion.x > -0.99995 || motion.y > -0.99995;
    float2 last_ndc;
    bool behind = false;
    if (f.moving)
    {
        last_ndc = motion * 2.0;
    }
    else
    {
        float3 world = world_at(float2(p) + 0.5, at(depth_of(cls), p).x);
        float4 c = transform(push.post, POST_PREVIOUS, float4(world, 1.0));
        behind = c.w <= 0.0;
        last_ndc = c.xy / c.w;
    }
    // Last frame's picture is of the view unmoved, and so is this pixel's surface's place in it:
    // where the pixel's center was this frame, unmoved, its surface was there last frame.
    f.last = pixel_of_ndc(last_ndc + state.xy);
    f.found = state.w < 0.5 && !behind && all(f.last >= 0.0) && all(f.last < extent);
    return f;
}

struct Taa
{
    // The pixel's history: its color and the frames gathered in it, which is also what is shown.
    float4 history : SV_Target0;
    // The marks of the frames at each place in the sequence, four bits each, two to a channel.
    float4 marks : SV_Target1;
};

[shader("fragment")]
Taa fs_taa(float4 position : SV_Position)
{
    int2 p = int2(position.xy);
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float4 state = row(push.post, POST_STATE);
    float4 seq = row(push.post, POST_TAA);
    float frames = seq.x;
    uint place = uint(seq.y + 0.5);
    bool still = state.z > 0.5;
    bool fresh = state.w > 0.5;
    float3 now = at(push.scene, p).rgb;
    Found f = found_at(p);

    // The marks: this frame's at its place in the sequence; the others kept while the view holds
    // still, and forgotten when it moves.
    uint marks[8];
    float4 held = at(push.marks, p) * 255.0 + 0.5;
    for (uint k = 0; k < 4; k++)
    {
        uint v = still && !fresh ? uint(held[k]) : 0;
        marks[k * 2] = v & 15;
        marks[k * 2 + 1] = v >> 4;
    }
    uint had_mark = marks[place];
    uint now_mark = mark(now);
    marks[place] = now_mark;

    Taa o;
    o.marks = float4(marks[0] | (marks[1] << 4), marks[2] | (marks[3] << 4),
                     marks[4] | (marks[5] << 4), marks[6] | (marks[7] << 4)) / 255.0;
    if (!f.found)
    {
        o.history = float4(now, 1.0);
        return o;
    }
    float gathered = at(push.history, int2(floor(f.last))).a;
    if (still && !f.moving && gathered >= frames && had_mark == now_mark)
    {
        // Every frame of the sequence is in, and this one draws the pixel as the one a sequence
        // ago did: keep what it has.
        o.history = at(push.history, p);
        return o;
    }
    // The colors round the pixel this frame: their mean and spread, in which last frame's color
    // is held.
    float3 m1 = float3(0.0, 0.0, 0.0);
    float3 m2 = float3(0.0, 0.0, 0.0);
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            int2 q = clamp(p + int2(x, y), int2(0, 0), int2(extent) - 1);
            float3 c = to_ycocg(compress(at(push.scene, q).rgb));
            m1 += c;
            m2 += c * c;
        }
    }
    m1 /= 9.0;
    float3 sigma = sqrt(max(m2 / 9.0 - m1 * m1, 0.0));
    float3 lo = m1 - sigma * 1.25;
    float3 hi = m1 + sigma * 1.25;
    float3 had = to_ycocg(compress(catmull_rom(push.history, f.last, extent)));
    // Held: moved toward the mean until it is inside the spread.
    float3 d = had - m1;
    float3 reach = (hi - lo) * 0.5 + 1e-5;
    float3 out_by = abs(d) / reach;
    float most = max(out_by.x, max(out_by.y, out_by.z));
    if (most > 1.0)
    {
        had = m1 + d / most;
    }
    float count = min(gathered + 1.0, frames);
    float3 c = lerp(had, to_ycocg(compress(now)), 1.0 / count);
    o.history = float4(expand(from_ycocg(c)), count);
    return o;
}

struct Check
{
    // This frame's colors, for the next frame to find.
    float4 history : SV_Target0;
    // Last frame's picture found where each pixel's surface was; its alpha 1 where it was found.
    float4 shown : SV_Target1;
};

[shader("fragment")]
Check fs_taa_check(float4 position : SV_Position)
{
    int2 p = int2(position.xy);
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float3 now = at(push.scene, p).rgb;
    Found f = found_at(p);
    Check o;
    o.history = float4(now, 1.0);
    o.shown = f.found ? float4(catmull_rom(push.history, f.last, extent), 1.0) : float4(now, 0.0);
    return o;
}

// The bloom's first step and each after: the picture read, a quarter the pixels, from thirteen
// samples round each (a box of four in the middle and four overlapping boxes round it, weighed as
// one); the first weighs each box by its brightness so one bright pixel does not flicker.
[shader("fragment")]
float4 fs_bloom_down(float4 position : SV_Position) : SV_Target0
{
    float2 texel = 1.0 / float2(push.source_width, push.source_height);
    float2 uv = position.xy * 2.0 * texel;
    float3 a = sampled(push.source, uv + texel * float2(-2.0, -2.0)).rgb;
    float3 b = sampled(push.source, uv + texel * float2(0.0, -2.0)).rgb;
    float3 c = sampled(push.source, uv + texel * float2(2.0, -2.0)).rgb;
    float3 d = sampled(push.source, uv + texel * float2(-2.0, 0.0)).rgb;
    float3 e = sampled(push.source, uv).rgb;
    float3 f = sampled(push.source, uv + texel * float2(2.0, 0.0)).rgb;
    float3 g = sampled(push.source, uv + texel * float2(-2.0, 2.0)).rgb;
    float3 h = sampled(push.source, uv + texel * float2(0.0, 2.0)).rgb;
    float3 i = sampled(push.source, uv + texel * float2(2.0, 2.0)).rgb;
    float3 j = sampled(push.source, uv + texel * float2(-1.0, -1.0)).rgb;
    float3 k = sampled(push.source, uv + texel * float2(1.0, -1.0)).rgb;
    float3 l = sampled(push.source, uv + texel * float2(-1.0, 1.0)).rgb;
    float3 m = sampled(push.source, uv + texel * float2(1.0, 1.0)).rgb;
    float3 boxes[5] = { (j + k + l + m) * 0.25, (a + b + d + e) * 0.25, (b + c + e + f) * 0.25,
                        (d + e + g + h) * 0.25, (e + f + h + i) * 0.25 };
    float weights[5] = { 0.5, 0.125, 0.125, 0.125, 0.125 };
    bool first = push.first != 0;
    float3 sum = float3(0.0, 0.0, 0.0);
    float total = 0.0;
    for (int n = 0; n < 5; n++)
    {
        float w = weights[n] * (first ? 1.0 / (1.0 + luma(boxes[n])) : 1.0);
        sum += boxes[n] * w;
        total += w;
    }
    return float4(sum / total, 1.0);
}

// The bloom back up the chain: the smaller picture, blurred by a tent three texels wide, added to
// the one a step larger.
[shader("fragment")]
float4 fs_bloom_up(float4 position : SV_Position) : SV_Target0
{
    float2 texel = 1.0 / float2(push.source_width, push.source_height);
    float2 uv = position.xy * 0.5 * texel;
    float3 c = sampled(push.source, uv).rgb * 4.0;
    c += (sampled(push.source, uv + texel * float2(-1.0, 0.0)).rgb
          + sampled(push.source, uv + texel * float2(1.0, 0.0)).rgb
          + sampled(push.source, uv + texel * float2(0.0, -1.0)).rgb
          + sampled(push.source, uv + texel * float2(0.0, 1.0)).rgb) * 2.0;
    c += sampled(push.source, uv + texel * float2(-1.0, -1.0)).rgb
         + sampled(push.source, uv + texel * float2(1.0, -1.0)).rgb
         + sampled(push.source, uv + texel * float2(-1.0, 1.0)).rgb
         + sampled(push.source, uv + texel * float2(1.0, 1.0)).rgb;
    return float4(c / 16.0, 1.0);
}

// Linear light as display values: as it is below the shoulder, where the game's own colors lie, so
// the frame shown is the frame drawn; above it, what goes past white rolls off into white, each
// channel apart, so a bright surface warms and then whitens rather than clipping flat.
float3 shown_light(float3 c)
{
    static const float SHOULDER = 0.6;
    float3 over = max(c - SHOULDER, 0.0) / (1.0 - SHOULDER);
    float3 l = min(c, SHOULDER) + (1.0 - SHOULDER) * (1.0 - exp(-over));
    float3 low = l * 12.92;
    float3 high = 1.055 * pow(max(l, 1e-6), 1.0 / 2.4) - 0.055;
    return float3(l.x <= 0.0031308 ? low.x : high.x, l.y <= 0.0031308 ? low.y : high.y,
                  l.z <= 0.0031308 ? low.z : high.z);
}

// The frame shown: the bloom laid in, the exposure and the shoulder when the colors are linear;
// as they are when they are display values.
[shader("fragment")]
float4 fs_tonemap(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    float4 tone = row(push.post, POST_TONE);
    float3 c = at(push.source, p).rgb;
    if (tone.z > 0.5)
    {
        float2 extent = float2(push.source_width, push.source_height);
        float2 uv = (float2(p) + 0.5) / extent;
        if (push.bloom != 0xFFFFFFFF)
        {
            float3 b = sampled(push.bloom, uv).rgb / tone.w;
            c = lerp(c, b, tone.y);
        }
        float4 grade = row(push.post, POST_GRADE);
        float2 v = uv - 0.5;
        c = shown_light(c * tone.x * (1.0 - grade.z * dot(v, v)));
        // A little saturation and contrast about middle gray, in display values, the contrast
        // easing out toward black so the shade keeps its color.
        c = lerp(float3(luma(c)), c, grade.x);
        c = lerp(c, (c - 0.5) * grade.y + 0.5, smoothstep(0.0, 0.25, c));
    }
    return float4(saturate(c), 1.0);
}
