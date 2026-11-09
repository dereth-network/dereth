// The grass field's blades, drawn into the G-buffer from the clumps `drawn.slang`'s `cs_grass`
// placed: each clump a few blades from one root, each blade a strip bent along a curve from its
// root to its tip, leaning its own way and swayed by the wind, rounded across its width so it
// shades softly. A clump is one of a few kinds of grass: a fine meadow grass, a broad-bladed one,
// a tall one with straw-colored tips and a short dark one. A blade is dark at its root and lighter
// and yellower at its tip; its color is the ground's own color there, varied clump by clump, so
// the field and the ground under it agree. The blades are geometry, with no picture and nothing
// cut out. Flowers and pebbles are drawn the same way, below.
//
// `vs_blade_motion` and `fs_blade_motion` draw the blades again where the G-buffer holds them,
// writing where each was last frame, as the wind then bent it, so the anti-aliasing follows them.
import bindless;
import retail;
import weather;

struct Push
{
    uint frame;
    // The field's constants and its clumps.
    uint constants;
    uint clumps;
    // The first clump this draw reads, the blades a clump, the segments a blade, and the blade's
    // width at its root, meters (as bits).
    uint base;
    uint blades;
    uint segments;
    uint width;
    // The weather's constants (~0 for none): grass out in the rain is wet as the ground is.
    uint weather;
    // For the motion: the post passes' constants (rows 0..3 the last frame's view and projection,
    // unmoved, in this frame's space), the depth and the outdoor depth.
    uint post;
    uint depth;
    uint outdoor_depth;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

static const uint G_WIND = 9; // xy the wind, z the field's clock, seconds; w the last frame's clock
static const uint CLUMP_BYTES = 32;
static const float TAU = 6.2831853;

uint blade_hash(uint x)
{
    x ^= x >> 16;
    x *= 0x7FEB352Du;
    x ^= x >> 15;
    x *= 0x846CA68Bu;
    x ^= x >> 16;
    return x;
}

float blade_unit(uint h)
{
    return float(h & 0xFFFFFFu) / 16777216.0;
}

// A clump's kind of grass, from its seed: 0 fine meadow grass, 1 broad blades, 2 tall with straw
// tips, 3 short and dark. Its width, height and lean factors, and how dry its tips are.
uint kind_of(uint seed)
{
    float u = blade_unit(blade_hash(seed ^ 0x2545F491u));
    return u < 0.5 ? 0u : (u < 0.7 ? 1u : (u < 0.85 ? 2u : 3u));
}

float4 kind_shape(uint kind)
{
    static const float4 SHAPES[4] = {
        float4(1.0, 1.0, 1.0, 0.0),
        float4(1.8, 0.78, 0.7, 0.0),
        float4(0.7, 1.4, 0.8, 0.75),
        float4(1.1, 0.6, 1.2, 0.0),
    };
    return SHAPES[kind & 3];
}

// One corner of a blade: where it stands, its rounded normal, how far up the blade it is.
struct Corner
{
    float3 p;
    float3 n;
    float t;
};

// Corner `vertex` of clump `index` with the wind of the moment `clock`.
Corner blade_corner(uint vertex, uint index, float clock)
{
    ByteAddressBuffer clumps = buffers[NonUniformResourceIndex(push.clumps)];
    uint at = index * CLUMP_BYTES;
    float4 a = asfloat(clumps.Load4(at));
    uint4 b = clumps.Load4(at + 16);
    uint segments = max(push.segments, 1u);
    uint quads = (segments - 1) * 6;
    uint per_blade = quads + 3;
    uint blade = vertex / per_blade;
    uint v = vertex % per_blade;
    uint level;
    float side;
    if (v < quads)
    {
        static const uint2 QUAD[6] = { uint2(0, 0), uint2(0, 1), uint2(1, 0), uint2(1, 0), uint2(0, 1), uint2(1, 1) };
        uint2 q = QUAD[v % 6];
        level = v / 6 + q.x;
        side = q.y != 0 ? 1.0 : -1.0;
    }
    else
    {
        uint c = v - quads;
        level = c == 2 ? segments : segments - 1;
        side = c == 0 ? -1.0 : (c == 1 ? 1.0 : 0.0);
    }
    float t = float(level) / float(segments);

    float4 shape = kind_shape(kind_of(b.x));
    uint h = blade_hash(b.x + blade * 0x9E3779B9u);
    uint h1 = blade_hash(h);
    uint h2 = blade_hash(h1);
    uint h3 = blade_hash(h2);
    float wider = asfloat(b.z);
    // Where the blade grows from the clump's root, which way it faces and leans, how tall.
    float spread = 0.14 * wider * sqrt(blade_unit(h));
    float around = TAU * blade_unit(h1);
    float3 root = a.xyz + float3(cos(around), sin(around), 0.0) * spread;
    float facing = TAU * blade_unit(h2);
    float3 fwd = float3(cos(facing), sin(facing), 0.0);
    float3 across = float3(-fwd.y, fwd.x, 0.0);
    float tall = a.w * shape.y * lerp(0.6, 1.15, blade_unit(h3));
    float lean = lerp(0.15, 0.7, blade_unit(blade_hash(h3))) * shape.z;
    // The wind: a steady push along it, and gusts that roll across the field.
    float4 wind = row(push.constants, G_WIND);
    float strength = length(wind.xy);
    float2 along = strength > 1e-4 ? wind.xy / strength : float2(1.0, 0.0);
    float gust = 0.5 + 0.5 * sin(dot(root.xy, along) * 0.45 - clock * 2.1 + blade_unit(h) * 0.8)
        * (0.7 + 0.3 * sin(dot(root.xy, float2(-along.y, along.x)) * 0.23 + clock * 0.7));
    float flutter = sin(clock * 3.3 + blade_unit(h1) * TAU) * 0.08;
    float2 push_xy = wind.xy * tall * (0.22 + 0.4 * gust) + across.xy * flutter * tall;
    float2 reach = fwd.xy * tall * sin(lean) + push_xy;
    float reach_len = length(reach);
    if (reach_len > 0.8 * tall)
    {
        reach *= 0.8 * tall / reach_len;
        reach_len = 0.8 * tall;
    }
    float3 tip = root + float3(reach, sqrt(max(tall * tall - reach_len * reach_len, 0.04 * tall * tall)));
    float3 bend = root + float3(reach * 0.15, tall * 0.6);
    float s = 1.0 - t;
    float3 p = s * s * root + 2.0 * s * t * bend + t * t * tip;
    float3 tangent = normalize(2.0 * s * (bend - root) + 2.0 * t * (tip - bend) + float3(0.0, 0.0, 1e-4));
    float width = asfloat(push.width) * shape.x * wider * (1.0 - pow(t, 1.3));
    p += across * side * width * 0.5;
    // Rounded across the blade: its face turned toward the eye, and leaned out at its edges.
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 face = normalize(cross(tangent, across));
    if (dot(face, eye - p) < 0.0)
    {
        face = -face;
    }
    float3 n = normalize(face + across * side * 0.5);
    Corner o;
    o.p = p;
    o.n = normalize(lerp(n, float3(0.0, 0.0, 1.0), 0.35));
    o.t = t;
    return o;
}

struct Blade
{
    float4 position : SV_Position;
    float3 normal : NORMAL;
    // How far up the blade, 0 at the root to 1 at the tip.
    float up : TEXCOORD0;
    // The clump's color (rgb) and dryness (a), and its own shade.
    nointerpolation float4 color : COLOR0;
    nointerpolation float shade : TEXCOORD1;
    float fog : TEXCOORD2;
};

[shader("vertex")]
Blade vs_blade(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    uint index = push.base + instance;
    float4 wind = row(push.constants, G_WIND);
    Corner c = blade_corner(vertex, index, wind.z);
    uint4 b = buffers[NonUniformResourceIndex(push.clumps)].Load4(index * CLUMP_BYTES + 16);
    uint kind = kind_of(b.x);
    float4 shape = kind_shape(kind);
    Blade o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(c.p, 1.0));
    o.normal = c.n;
    o.up = c.t;
    float4 color = float4(float(b.y & 0xFF), float((b.y >> 8) & 0xFF), float((b.y >> 16) & 0xFF), float(b.y >> 24)) / 255.0;
    // Some clumps have gone to straw at their tips; the tall kind most of all.
    float straw = blade_unit(blade_hash(b.x ^ 0x68E31DA4u)) < 0.3 ? 0.85 : 0.0;
    color.a = saturate(max(color.a, max(shape.w, straw)));
    // Each clump a little warmer or cooler than the next.
    color.rgb *= lerp(float3(1.08, 1.0, 0.82), float3(0.9, 1.02, 1.06), blade_unit(blade_hash(b.x ^ 0x3C6EF372u)));
    o.color = color;
    o.shade = lerp(0.86, 1.12, blade_unit(blade_hash(b.x ^ 0xA5A5A5A5u))) * (kind == 3 ? 0.8 : 1.0);
    float4 view = transform(push.frame, FRAME_VIEW, float4(c.p, 1.0));
    o.fog = fog_factor(push.frame, length(view.xyz));
    return o;
}

struct Targets
{
    float4 albedo : SV_Target0;
    float2 normal : SV_Target1;
    float4 light : SV_Target2;
    float4 material : SV_Target3;
};

[shader("fragment")]
Targets fs_blade(Blade in)
{
    float3 n = normalize(in.normal);
    // The ground's color there, a little greener and fresher for the living blade.
    float3 base = in.color.rgb * in.shade * float3(0.95, 1.08, 0.8);
    // Darker at the root, where the field shades itself; lighter and yellower toward the tip, the
    // more so the drier the clump.
    float3 dry_tip = base * float3(1.32, 1.2, 0.7);
    float3 tip = lerp(base * float3(1.1, 1.14, 0.92), dry_tip, in.color.a * smoothstep(0.45, 1.0, in.up));
    float rise = smoothstep(0.0, 1.0, in.up);
    float3 albedo = lerp(base * 0.6, tip * 1.08, rise);
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_GROUND, n, 0.0, float4(saturate(albedo), 0.9));
    o.normal = oct_encode(n);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : ground_light(push.frame, n), in.fog);
    o.material = material_bytes(CLASS_GROUND, 0, 0);
    return o;
}

struct Moved
{
    float4 position : SV_Position;
    // Where the corner was last frame, in clip space.
    float4 last : TEXCOORD0;
};

[shader("vertex")]
Moved vs_blade_motion(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    uint index = push.base + instance;
    float4 wind = row(push.constants, G_WIND);
    Corner now = blade_corner(vertex, index, wind.z);
    Corner was = blade_corner(vertex, index, wind.w);
    Moved o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(now.p, 1.0));
    o.last = transform(push.post, 0, float4(was.p, 1.0));
    return o;
}

// Whether depth `z` is the depth `held` holds, to a thousandth of the distance.
bool same_depth(float z, float held)
{
    return abs(z - held) <= (1.0 - held) * 1e-3 + 1e-7;
}

[shader("fragment")]
float4 fs_blade_motion(Moved in) : SV_Target0
{
    int2 p = int2(in.position.xy);
    float d = textures[NonUniformResourceIndex(push.depth)].Load(int3(p, 0)).x;
    float od = textures[NonUniformResourceIndex(push.outdoor_depth)].Load(int3(p, 0)).x;
    if (!same_depth(in.position.z, d) && !same_depth(in.position.z, od))
    {
        discard;
    }
    // Halved, as the moving objects' are; never -1, which is where nothing moved.
    return float4(clamp(in.last.xy / in.last.w * 0.5, -0.9999, 1.0), 0.0, 0.0);
}

// --- flowers and pebbles ------------------------------------------------------------------------
//
// Each scattered thing is 24 corners. A flower: a stem, two leaves at its foot and five petals
// round its head, the head facing up and a little to one side. A pebble: a squat eight-faced
// stone, half sunk.

struct Scatter
{
    float4 position : SV_Position;
    float3 normal : NORMAL;
    // The surface's color, and how far up a stem or leaf it is (petals and stones: 1).
    float3 color : COLOR0;
    float up : TEXCOORD0;
    float fog : TEXCOORD1;
};

static const float3 STEM = float3(0.22, 0.34, 0.11);

[shader("vertex")]
Scatter vs_scatter(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    ByteAddressBuffer clumps = buffers[NonUniformResourceIndex(push.clumps)];
    uint at = (push.base + instance) * CLUMP_BYTES;
    float4 a = asfloat(clumps.Load4(at));
    uint4 b = clumps.Load4(at + 16);
    bool flower = (b.y >> 24) != 0;
    float3 color = float3(float(b.y & 0xFF), float((b.y >> 8) & 0xFF), float((b.y >> 16) & 0xFF)) / 255.0;
    uint h = blade_hash(b.x);
    uint h1 = blade_hash(h);
    float turn = TAU * blade_unit(h);
    float3 fwd = float3(cos(turn), sin(turn), 0.0);
    float3 across = float3(-fwd.y, fwd.x, 0.0);
    float3 up3 = float3(0.0, 0.0, 1.0);
    float3 p;
    float3 n;
    float3 c = color;
    float rise = 1.0;
    uint tri = vertex / 3;
    uint corner = vertex % 3;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    if (flower)
    {
        float tall = a.w;
        // The wind sways the head a little.
        float4 wind = row(push.constants, G_WIND);
        float sway = 0.5 + 0.5 * sin(dot(a.xy, float2(0.31, 0.27)) - wind.z * 2.4 + blade_unit(h1) * TAU);
        float3 head = a.xyz + float3(wind.xy * tall * 0.25 * sway + fwd.xy * tall * 0.15, tall);
        float petal = lerp(0.04, 0.065, blade_unit(h1)) * (1.0 + length(head - eye) / 60.0);
        if (tri == 0)
        {
            // The stem: a thin triangle from the ground to the head.
            p = corner == 2 ? head : a.xyz + across * (corner == 0 ? -0.006 : 0.006);
            n = normalize(fwd + up3 * 0.3);
            c = STEM;
            rise = corner == 2 ? 1.0 : 0.0;
        }
        else if (tri < 3)
        {
            // A leaf at the foot, leaning out.
            float3 out_dir = tri == 1 ? across : -across;
            float3 tip = a.xyz + out_dir * tall * 0.35 + up3 * tall * 0.3;
            p = corner == 0 ? a.xyz + fwd * 0.012 : (corner == 1 ? a.xyz - fwd * 0.012 : tip);
            n = normalize(up3 + out_dir * 0.4);
            c = STEM * 1.1;
            rise = corner == 2 ? 0.8 : 0.0;
        }
        else
        {
            // A petal round the head: the disc tilted a little toward its side.
            uint k = tri - 3;
            float3 face = normalize(up3 + fwd * 0.35);
            float3 e1 = normalize(cross(face, across));
            float3 e2 = cross(face, e1);
            float a0 = TAU * (float(k) - 0.35) / 5.0;
            float a1 = TAU * (float(k) + 0.35) / 5.0;
            float3 rim0 = head + (e1 * cos(a0) + e2 * sin(a0)) * petal;
            float3 rim1 = head + (e1 * cos(a1) + e2 * sin(a1)) * petal;
            p = corner == 0 ? head + face * 0.004 : (corner == 1 ? rim0 : rim1);
            n = face;
            c = corner == 0 ? color * 0.75 : color;
        }
    }
    else
    {
        // A squat stone: eight faces round its middle, half sunk.
        float size = a.w;
        float3 axes[3] = { fwd * size, across * size * lerp(0.7, 1.0, blade_unit(h1)), up3 * size * 0.55 };
        uint k = tri;
        float3 sx = (k & 1) != 0 ? axes[0] : -axes[0];
        float3 sy = (k & 2) != 0 ? axes[1] : -axes[1];
        float3 sz = (k & 4) != 0 ? axes[2] : -axes[2];
        float3 centre = a.xyz + up3 * size * 0.15;
        p = centre + (corner == 0 ? sx : (corner == 1 ? sy : sz));
        n = normalize(normalize(sx) + normalize(sy) + normalize(sz));
        if (dot(n, up3) < -0.2)
        {
            n = up3;
        }
        c = color;
    }
    Scatter o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    o.normal = dot(n, eye - p) < 0.0 && flower ? -n : n;
    o.color = c;
    o.up = rise;
    float4 view = transform(push.frame, FRAME_VIEW, float4(p, 1.0));
    o.fog = fog_factor(push.frame, length(view.xyz));
    return o;
}

[shader("fragment")]
Targets fs_scatter(Scatter in)
{
    float3 n = normalize(in.normal);
    float3 albedo = in.color * lerp(0.6, 1.0, in.up);
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_GROUND, n, 0.0, float4(saturate(albedo), 0.8));
    o.normal = oct_encode(n);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : ground_light(push.frame, n), in.fog);
    o.material = material_bytes(CLASS_GROUND, 0, 0);
    return o;
}
