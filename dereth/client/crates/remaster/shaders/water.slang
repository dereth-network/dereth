// The water, drawn over the resolved world: where the ground is water (the material's kind 7),
// and, with puddles asked for, where the ground gathers them: the share the weather lays in the
// ground's material on a rainy day, held down far off and at grazing angles.
//
// For each such pixel the surface's normal is the water's level bent by its ripples and by the rings
// round the bodies standing in it; what it reflects is found along the reflected ray, and laid
// over the pixel's own color by the Fresnel share; and the sun glints off it where the sun reaches.
//
// - **Ripples** are one small picture of wind ripples laid several times over the water, at
//   different sizes and turns, each drifting with the scene's clock alone, so a held clock draws a
//   still frame; they calm with distance, so far water lies smooth, and a ripple finer than a pixel
//   is left out, so it never shimmers. Puddles keep a few small waves of their own.
// - **Rings**: the water swells round the legs of each body standing in it, and each sends out
//   rings as it goes, every so often walking and now and then standing, each a short train of
//   crests spreading from where it was sent and fading as it ages, so a walking body leaves a wake
//   behind it.
// - **The reflection**, where the device traces rays (`fs_traced`), is the scene itself: the ray
//   is traced through the scene's acceleration structures, and what it meets is lit as the resolve
//   lights it, whether it is on screen or not. Elsewhere (`fs_main`), on the water nearest the eye
//   it is the scene drawn mirrored in that water's level (`planar.rs`); on other water and on the
//   puddles it is found on screen: the ray
//   marched through the depth, a surface it meets close behind taken, but never the viewer's own
//   body or anything at the camera, and never the color of something it merely passes behind;
//   where it finds nothing it can trust, it takes what lies far off that way. Either way, what lies
//   far off is the land past the scene's reach, found along the world's heights, then the sky: the
//   sky's own tables that way, and the sky on screen where it shows that very way.
// - **The water's own color**, over a bed drawn under it, is retail's water under the light the bed
//   takes: what is seen through the water fades into it with the length of water the view runs
//   through, red first.
//
// Its own pictures are a copy of the resolved world (`copy`); it writes the scene's colors.
import atmosphere;
import bindless;
import intent;
import lift;
import retail;
import rt;

// The pass's constants, in buffer `water` as float4 rows:
//   0  x the scene's clock (seconds), y the waves' strength, z the rings' strength, w 1 where
//      puddles are drawn
//   1  x the bodies standing in the water, y 1 where the water is drawn, z 1 to check the
//      reflection: each water pixel shows what its reflection found, w how strongly the water
//      takes the light of what is seen through it over a lowered bed
//   2  xy where the frame's space's origin lies in the world, meters
//   3..10 the ripples' layers, two rows each: x meters a repeat, y how much it bends the surface,
//      zw the cosine and sine of its turn; then xy the share of a repeat its picture is moved
//   11..18 the bodies: where their feet are (the frame's space), and how fast they move (metres a
//      second); eight rows always, those past the bodies' count nought
//   19..21 (words) where the reflections are traced through the scene: the top-level structure,
//      the statics' and the objects' geometry tables, the scene's indices; its vertices, its
//      materials, the sampler its textures are read with, its instances; the ground's vertices
//      and cells, the statics' and the objects' entries (~0 where nothing is traced)
//   22 (words) x how many statics come first among the instances; y the sky's constants, z its
//      sky-view table and w the sampler it is read with (~0 without the sky's tables)
//   23 (words) x the world's vertex heights, y where the water lies about each vertex (how far
//      inside it, and how far over its level; two floats each) (~0 where the drawn ground is not
//      drawn)
//   24 the viewer's own body: where its feet are (the frame's space), w 1 where there is one
//   25, 26 (words) the sky all round the eye: its pictures east, west, north, south, up and down,
//      and the sampler they are read with (~0 where it is not drawn)
//   27 the scene mirrored in the water nearest the eye: (words) x its colors, y the sampler
//      they are read with (~0 where it is not drawn), z (a float's bits) the water's level it is
//      mirrored in
//   28..31 its view and projection together, as four columns
//   32 (words) x how many rings the bodies have sent out over the water
//   33.. the rings, two rows each: where the body stood (the frame's space) and how long ago,
//      seconds; then x how strongly
static const uint WATER_STATE = 0;
static const uint WATER_DRAWN = 1;
// The drawn ground's constants' row whose x is 1 while the bed is lowered under the water.
static const uint D_BED = 6;
static const uint WATER_GLOBAL = 2;
static const uint WATER_LAYERS = 3;
static const uint LAYERS = 4;
static const uint WATER_WADERS = 11;
static const uint WADERS = 8;
static const uint WATER_TRACE = 19;
static const uint WATER_SKY = 22;
static const uint WATER_LAND = 23;
static const uint WATER_VIEWER = 24;
static const uint WATER_PROBE = 25;
static const uint WATER_PLANAR = 27;
static const uint WATER_RING_COUNT = 32;
static const uint WATER_RINGS = 33;
static const uint RINGS = 32;
static const uint NONE = 0xFFFFFFFF;

// The water's kind, as the material target's second byte holds it.
static const uint KIND_WATER = 7;
// The step, meters, of how far under the water's surface the ground draws its bed: the material
// target's fourth byte on the ground (0 where the bed is not lowered).
static const float BED_STEP = 0.04;

// The share of the Fresnel reflection drawn: water seen at a low angle reflects nearly all of the
// sky, which would turn the water's own color to the sky's white; this keeps it showing through.
static const float REFLECTED = 0.55;
// What the water takes of what it reflects, a little more of the blue: the sky's warm gray would
// otherwise wash its color out.
static const float3 REFLECTED_TINT = float3(0.62, 0.86, 1.08);
// The steepest slope the ripples' picture holds, and how far from the eye they have calmed, meters.
static const float MOST_SLOPE = 0.6;
static const float RIPPLES_CALM = 120.0;
// How far the eye sees into the water, meters: what is seen through it fades into its own color
// over about this much of it.
static const float CLARITY = 1.6;
// The landscape's vertices: a cell's side, meters, and how many a side; the drawn ground's
// constants' rows of its terrain's colors, each type's, and the water's types.
static const float CELL = 24.0;
static const int SIDE = 2041;
static const uint D_COLORS = 104;
static const uint WATER_FIRST = 16;
static const uint WATER_LAST = 20;

struct Push
{
    uint frame;
    uint water;
    // The resolved world's colors, copied.
    uint copy;
    uint depth;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
    uint material;
    uint normal;
    // How much of the sun reaches each pixel (~0: all of it everywhere).
    uint sun_visibility;
    // The ground's own colors.
    uint albedo;
    // The ripples' picture (~0 while only the puddles are drawn) and the sampler it repeats under.
    uint ripples;
    uint wrap;
    // The drawn ground's constants and the landscape's terrain words (~0 where it is not drawn).
    uint drawn;
    uint words;
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

uint kind_at(int2 p)
{
    return uint(at(push.material, p).y * 255.0 + 0.5);
}

// The depth texture a pixel of class `cls` is placed by, as the resolve places it.
uint depth_of(uint cls)
{
    return (cls == CLASS_GROUND || cls == CLASS_SUN) ? push.outdoor_depth : push.depth;
}

float2 extent()
{
    return row(push.frame, FRAME_EXTENT).xy;
}

// A pixel position's place in the frame's space, at depth `d`.
float3 world_at(float2 pos, float d)
{
    float2 e = extent();
    float2 ndc = float2(pos.x / e.x * 2.0 - 1.0, 1.0 - pos.y / e.y * 2.0);
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    return h.xyz / h.w;
}

// How the puddles' surface leans at `xy` (the slope of its height each way), from their small
// waves: four trains of them, each its own length, way and speed; their height falls off with the
// distance `dist` from the eye.
float2 waves(float2 xy, float t, float dist, float strength)
{
    float calm = saturate(1.0 - dist / 90.0);
    float a = strength * calm * calm;
    if (a <= 0.0)
    {
        return float2(0.0, 0.0);
    }
    float2 slope = float2(0.0, 0.0);
    static const float4 trains[4] = { float4(0.8, 0.6, 2.1, 0.9), float4(-0.5, 0.86, 3.4, 1.3),
                                      float4(0.2, -0.98, 1.3, 0.7),
                                      float4(-0.9, -0.42, 5.2, 1.7) };
    for (int i = 0; i < 4; i++)
    {
        float2 dir = trains[i].xy;
        float wavelength = trains[i].z;
        float k = 2.0 * PI / wavelength;
        float phase = dot(dir, xy) * k - t * trains[i].w * k * 0.35;
        // Each train's height is a hundredth of its length, so the slope is the same for each.
        slope += dir * cos(phase) * 0.01 * wavelength * k;
    }
    return slope * a;
}

static uint lattice_hash(int2 p)
{
    uint h0 = (asuint(p.x) * 0x8DA6B343u) ^ (asuint(p.y) * 0xD8163841u);
    uint h1 = (h0 ^ (h0 >> 13)) * 0x2C1B3C6Du;
    return h1 ^ (h1 >> 15);
}

// Smooth noise over a lattice `cell` meters wide, 0..1.
float value_noise(float2 p, float cell)
{
    float2 q = p / cell;
    int2 i = int2(floor(q));
    float2 f = q - floor(q);
    float2 s = f * f * (3.0 - 2.0 * f);
    float4 h = float4(float(lattice_hash(i) & 0xFFFF), float(lattice_hash(i + int2(1, 0)) & 0xFFFF),
                      float(lattice_hash(i + int2(0, 1)) & 0xFFFF),
                      float(lattice_hash(i + int2(1, 1)) & 0xFFFF))
              / 65535.0;
    return lerp(lerp(h.x, h.y, s.x), lerp(h.z, h.w, s.x), s.y);
}

// How much of a layer of ripples `repeat` meters a repeat shows in a pixel that covers `foot`
// meters of water: all of it while the pixel is a twenty-fourth of a repeat or less, none once it
// is a sixth. A ripple finer than the pixel is left out, not sampled, so the far water lies calm
// and holds still under the anti-aliasing's jitter.
float ripple_resolved(float repeat, float foot)
{
    return 1.0 - smoothstep(repeat / 24.0, repeat / 6.0, foot);
}

// How the water leans at `p` (the frame's space) from its ripples, `dist` meters from the eye, as
// a pixel covering `dx` and `dy` of the water sees them, `foot` meters of it at the most.
float2 ripples(float2 p, float dist, float2 dx, float2 dy, float foot, float strength)
{
    float calm = 1.0 - smoothstep(RIPPLES_CALM * 0.15, RIPPLES_CALM, dist);
    if (calm <= 0.0 || strength <= 0.0 || push.ripples == 0xFFFFFFFF)
    {
        return float2(0.0, 0.0);
    }
    // Patches of rougher and glassier water, tens of meters across.
    float gust = 0.55 + 0.9 * value_noise(p + row(push.water, WATER_GLOBAL).xy + float2(5.0, 11.0), 41.0);
    float2 s = float2(0.0, 0.0);
    for (uint k = 0; k < LAYERS; k++)
    {
        float4 l = row(push.water, WATER_LAYERS + k * 2);
        float2 moved = row(push.water, WATER_LAYERS + k * 2 + 1).xy;
        float c = l.z;
        float n = l.w;
        // The layer's own east and north, the picture's rows running down to the south.
        float2 uv = float2(c * p.x + n * p.y, n * p.x - c * p.y) / l.x + moved;
        float2 gx = float2(c * dx.x + n * dx.y, n * dx.x - c * dx.y) / l.x;
        float2 gy = float2(c * dy.x + n * dy.y, n * dy.x - c * dy.y) / l.x;
        float2 t = textures[NonUniformResourceIndex(push.ripples)]
                       .SampleGrad(samplers[NonUniformResourceIndex(push.wrap)], uv, gx, gy)
                       .rg
                   * 2.0
                   - 1.0;
        // Back from the layer's turn.
        s += float2(c * t.x - n * t.y, n * t.x + c * t.y) * MOST_SLOPE * l.y * ripple_resolved(l.x, foot);
    }
    return s * 0.35 * strength * calm * gust;
}

// Where the line of sight through the pixel position `pos` meets the level of `s`, east and north
// in the frame's space: the water a neighboring pixel sees.
float2 on_level(float2 pos, float3 s)
{
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 r = world_at(pos, 0.99) - eye;
    if (r.z > -1e-5)
    {
        return s.xy + normalize(r.xy + float2(1e-6, 0.0)) * 1.0e4;
    }
    return eye.xy + r.xy * ((s.z - eye.z) / r.z);
}

// The drawn ground's terrain word at landscape vertex `g`.
uint vertex_word(int2 g)
{
    int2 c = clamp(g, int2(0, 0), int2(SIDE - 1, SIDE - 1));
    uint k = uint(c.y * SIDE + c.x);
    uint pair = buffers[NonUniformResourceIndex(push.words)].Load((k / 2) * 4);
    return (k & 1) != 0 ? pair >> 16 : pair & 0xFFFF;
}

// Retail's water hue at world `p`: the water types of the cell's corners, blended across the cell,
// the land corners leaving their share to the water ones.
float3 hue_at(float2 p)
{
    float2 q = p / CELL;
    int2 g = int2(floor(q));
    float2 f = saturate(q - float2(g));
    float3 sum = float3(0.0, 0.0, 0.0);
    float n = 0.0;
    for (int k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        uint t = (vertex_word(g + o) >> 2) & 0x1F;
        if (t >= WATER_FIRST && t <= WATER_LAST)
        {
            float w = lerp(1.0 - f.x, f.x, float(o.x)) * lerp(1.0 - f.y, f.y, float(o.y)) + 1e-4;
            sum += row(push.drawn, D_COLORS + t).rgb * w;
            n += w;
        }
    }
    return n > 0.0 ? sum / n : row(push.drawn, D_COLORS + WATER_FIRST).rgb;
}

// How fast a ring spreads over the water, metres a second; how long it lasts, seconds for most
// of it; and the length of its crests, metres.
static const float RING_SPEED = 0.8;
static const float RING_LIFE = 1.1;
static const float RING_CREST = 0.3;

// How the surface leans at `p` from the bodies standing in the water: the water heaped a little
// round where each stands, more as it moves; and the rings each sends out as it goes, each a short
// train of crests spreading from where the body stood when it sent it, wider and fainter as it
// ages, so a body walking draws a wake of rings behind it and a body standing still now and then
// a faint one.
float2 rings(float3 p, float t, float strength)
{
    float2 slope = float2(0.0, 0.0);
    if (strength <= 0.0)
    {
        return slope;
    }
    uint n = min(uint(row(push.water, WATER_DRAWN).x + 0.5), WADERS);
    for (uint i = 0; i < n; i++)
    {
        float4 w = row(push.water, WATER_WADERS + i);
        if (abs(p.z - w.z) > 1.5)
        {
            continue;
        }
        float2 d = p.xy - w.xy;
        float r = length(d);
        if (r > 2.0 || r < 1e-3)
        {
            continue;
        }
        // A swell round the legs: highest at them, gone a metre or so out.
        float a = 0.012 + 0.03 * saturate(w.w / 2.0);
        float x = r - 0.25;
        slope += (d / r) * (-2.0 * x / 0.16) * exp(-x * x / 0.16) * a;
    }
    uint count = min(row_u(push.water, WATER_RING_COUNT).x, RINGS);
    float k = 2.0 * PI / RING_CREST;
    for (uint j = 0; j < count; j++)
    {
        float4 o = row(push.water, WATER_RINGS + j * 2);
        float4 e = row(push.water, WATER_RINGS + j * 2 + 1);
        if (abs(p.z - o.z) > 1.5)
        {
            continue;
        }
        float age = o.w;
        float2 d = p.xy - o.xy;
        float r = length(d);
        float front = 0.25 + RING_SPEED * age;
        float width = 0.18 + 0.12 * age;
        float x = r - front;
        if (abs(x) > 2.5 * width || r < 1e-3)
        {
            continue;
        }
        // A couple of centimetres high, fainter as it ages and as it spreads over a wider ring.
        float a = e.x * 0.02 * exp(-age / RING_LIFE) * saturate(age * 12.0 + 0.25) * rsqrt(max(front, 0.3));
        float envelope = exp(-x * x / (width * width));
        float rise = a * envelope * (k * cos(k * x) - 2.0 * x / (width * width) * sin(k * x));
        slope += (d / r) * rise;
    }
    return slope * strength;
}

// The sky that shows above pixel `px`, the first pixel of sky up its column, a few steps at most;
// false where the column shows none.
bool sky_above(int2 px, out float3 color)
{
    int step = max(int(extent().y) / 12, 1);
    for (int k = 0; k < 12 && px.y >= 0; k++)
    {
        uint cls = class_at(px);
        if ((cls == CLASS_NONE || cls == CLASS_SKY) && kind_at(px) != KIND_WATER)
        {
            color = at(push.copy, px).rgb;
            return true;
        }
        px.y -= step;
    }
    color = float3(0.0, 0.0, 0.0);
    return false;
}

// The fog's color, which retail's sky meets at the horizon, as the frame draws its colors.
float3 fog_light()
{
    float3 fog = row(push.frame, FRAME_FOG_COLOR).rgb * gamma_scale(push.frame);
    return linear_frame(push.frame) ? to_linear(fog) : saturate(fog);
}

// Where `q` (the frame's space) lands on the target, in pixels; `w` its clip space depth's w.
float2 screen_of(float3 q, out float w)
{
    float2 e = extent();
    float4 c = transform(push.frame, FRAME_VIEW_PROJ, float4(q, 1.0));
    w = c.w;
    float2 ndc = c.xy / c.w;
    return float2((ndc.x + 1.0) * 0.5 * e.x, (1.0 - ndc.y) * 0.5 * e.y);
}

// The sky the picture shows the way `dir` goes from `from`: `wide`, the picture's sky along its
// width weighted toward that way, which varies slowly however the land cuts the picture's edge
// (the fog's light where the picture shows no sky); and, where the way lands well inside the
// picture on sky, that sky itself (`exact`), with how much of it to keep.
void screen_sky(float3 from, float3 dir, out float3 wide, out float keep, out float3 exact)
{
    wide = fog_light();
    keep = 0.0;
    exact = wide;
    float w;
    float2 pos = screen_of(from + dir * 1.0e5, w);
    if (w <= 0.0)
    {
        return;
    }
    float2 e = extent();
    float2 inside = clamp(pos, float2(0.0, 0.0), e - 1.0);
    float3 sum = float3(0.0, 0.0, 0.0);
    float weight = 0.0;
    for (int k = 0; k < 7; k++)
    {
        float x = (float(k) + 0.5) / 7.0 * e.x;
        float off = (x - inside.x) / (0.3 * e.x);
        float3 c;
        if (sky_above(int2(int(x), int(inside.y)), c))
        {
            float g = exp(-off * off);
            sum += c * g;
            weight += g;
        }
    }
    if (weight > 1.0e-4)
    {
        wide = sum / weight;
    }
    float margin = min(min(pos.x, e.x - pos.x) / e.x, min(pos.y, e.y - pos.y) / e.y);
    float k = smoothstep(0.0, 0.12, margin);
    int2 px = int2(inside);
    uint cls = class_at(px);
    if (k > 0.0 && (cls == CLASS_NONE || cls == CLASS_SKY) && kind_at(px) != KIND_WATER)
    {
        keep = k;
        exact = at(push.copy, px).rgb;
    }
}

// A color in display values as the frame draws its colors.
float3 as_drawn(float3 c)
{
    return linear_frame(push.frame) ? to_linear(saturate(c)) : saturate(c);
}

// The sky's own light toward `dir` from its tables, with the veil over the horizon, as the sky
// pass draws them; false without the sky's tables.
bool tables_toward(float3 dir, out float3 color, out float share)
{
    uint4 s = row_u(push.water, WATER_SKY);
    color = float3(0.0, 0.0, 0.0);
    share = 0.0;
    if (s.y == NONE)
    {
        return false;
    }
    uint a = s.y;
    float r = eye_radius(a);
    float bottom = planet_radius(a);
    float horizon_cos = -sqrt(max(r * r - bottom * bottom, 0.0)) / r;
    float2 uv = sky_view_uv(a, r, max(dir.z, horizon_cos + 0.002), light_view_cos(a, dir));
    float3 sky = exposed_sky(a, textures[NonUniformResourceIndex(s.z)]
                                    .SampleLevel(samplers[NonUniformResourceIndex(s.w)], uv, 0.0)
                                    .rgb);
    // The veil over the horizon, as `sky.slang` lays it.
    float4 v = row(a, ATM_VEIL);
    float up = max(dir.z, 0.0);
    float band = max(v.x, 1e-4);
    float day_veil = exp(-up / band);
    float x = clamp((dir.z + 2.0 * band) / (5.0 * band), 0.0, 1.0);
    float night_veil = (1.0 - x) * (1.0 - x) * (1.0 - x);
    float veil = lerp(night_veil, day_veil, v.y);
    float2 huv = sky_view_uv(a, r, 0.01, light_view_cos(a, dir));
    float3 horizon = exposed_sky(a, textures[NonUniformResourceIndex(s.z)]
                                        .SampleLevel(samplers[NonUniformResourceIndex(s.w)], huv, 0.0)
                                        .rgb);
    float3 light = lerp(row(a, ATM_FOG).rgb, horizon, v.y * 0.9) * v.z;
    if (air_drawn(a))
    {
        float toward_sun = pow(0.5 + 0.5 * light_view_cos(a, dir), 4.0);
        veil = max(veil, row(a, ATM_SUN_LIGHT).w * toward_sun * 0.75 * exp(-up / (band * 5.0)));
        light = haze_light(a, s.z, s.w, dir, 1.0) * v.z;
    }
    color = as_drawn(lerp(encode(sky), encode(light), saturate(veil)));
    share = row(a, ATM_MISC).x;
    return true;
}

// The sky the way `dir` goes, as the sky all round the eye shows it; false where it is not drawn.
bool probe_toward(float3 dir, out float3 color)
{
    uint4 a = row_u(push.water, WATER_PROBE);
    uint4 b = row_u(push.water, WATER_PROBE + 1);
    color = float3(0.0, 0.0, 0.0);
    if (a.x == NONE)
    {
        return false;
    }
    // The way whose picture holds `dir`, and that picture's up, as `sky_probe.rs` lays them.
    float3 m = abs(dir);
    uint face;
    float3 f;
    float3 up = float3(0.0, 0.0, 1.0);
    if (m.x >= m.y && m.x >= m.z)
    {
        face = dir.x > 0.0 ? a.x : a.y;
        f = float3(dir.x > 0.0 ? 1.0 : -1.0, 0.0, 0.0);
    }
    else if (m.y >= m.z)
    {
        face = dir.y > 0.0 ? a.z : a.w;
        f = float3(0.0, dir.y > 0.0 ? 1.0 : -1.0, 0.0);
    }
    else
    {
        face = dir.z > 0.0 ? b.x : b.y;
        f = float3(0.0, 0.0, dir.z > 0.0 ? 1.0 : -1.0);
        up = float3(0.0, 1.0, 0.0);
    }
    float3 across = normalize(cross(up, f));
    float3 above = cross(f, across);
    float z = max(dot(dir, f), 1e-4);
    float2 uv = float2(0.5 + 0.5 * dot(dir, across) / z, 0.5 - 0.5 * dot(dir, above) / z);
    color = textures[NonUniformResourceIndex(face)]
                .SampleLevel(samplers[NonUniformResourceIndex(b.z)], uv, 0.0)
                .rgb;
    return true;
}

// The sky the way `dir` goes from `from`: the sky all round the eye, where it is drawn; otherwise
// the sky's own tables that way over the sky the picture shows along its width, as the sky pass
// lays the one over the other, and the sky on screen itself where it shows that very way.
float3 sky_toward(float3 from, float3 dir)
{
    float3 drawn;
    if (probe_toward(dir, drawn))
    {
        return drawn;
    }
    float3 wide;
    float keep;
    float3 exact;
    screen_sky(from, dir, wide, keep, exact);
    float3 tables;
    float share;
    float3 base = wide;
    if (tables_toward(dir, tables, share))
    {
        base = lerp(wide, tables, share);
    }
    return lerp(base, exact, keep);
}

// The light on a surface of class `cls` with normal `n` (outdoors), `sun` how much of the sun
// reaches it, `leaf` whether it is a plant's leaves: the resolve's own light.
float3 lit(uint cls, float3 n, float sun_reach, bool leaf, float3 to_eye)
{
    Terms t;
    t.cls = cls;
    t.emissive = float3(0.0, 0.0, 0.0);
    t.burn = float3(0.0, 0.0, 0.0);
    t.lights = float3(0.0, 0.0, 0.0);
    t.ambient = row(push.frame, FRAME_OBJECT_AMBIENT).rgb;
    t.sun_color = float3(0.0, 0.0, 0.0);
    t.sun_facing = 0.0;
    t.sun = float3(0.0, 0.0, 0.0);
    float3 sun = row(push.frame, FRAME_SUN).xyz;
    if (cls == CLASS_GROUND)
    {
        t.ambient = row(push.frame, FRAME_GROUND_AMBIENT).rgb;
        t.sun_color = row(push.frame, FRAME_SUN_COLOR).rgb;
        t.sun_facing = max(dot(n, sun), 0.0);
    }
    else if (dot(sun, sun) >= 1e-8)
    {
        float b = length(sun);
        t.sun_color = row(push.frame, FRAME_SUN_COLOR).rgb * b;
        t.sun_facing = max(dot(n, sun / b), 0.0);
        t.sun = sun_light(push.frame, n) * sun_reach;
    }
    Masks m = unmasked(push.frame);
    m.sun_visibility = sun_reach;
    if (pack_lit_frame(push.frame))
    {
        m.pack_lit = true;
        m.pack_light = pack_light(push.frame, n, sun_reach, 1.0, cls == CLASS_SUN, leaf, 0.5, to_eye);
    }
    return compose(t, m);
}

// The world's height at landscape vertex `g`.
float vertex_height(uint heights, int2 g)
{
    int2 c = clamp(g, int2(0, 0), int2(SIDE - 1, SIDE - 1));
    return asfloat(buffers[NonUniformResourceIndex(heights)].Load(uint(c.y * SIDE + c.x) * 4));
}

// The world's height at world `p`, between its vertices.
float land_height(uint heights, float2 p)
{
    float2 q = p / CELL;
    int2 g = int2(floor(q));
    float2 f = q - float2(g);
    float a = vertex_height(heights, g);
    float b = vertex_height(heights, g + int2(1, 0));
    float c = vertex_height(heights, g + int2(0, 1));
    float d = vertex_height(heights, g + int2(1, 1));
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// The land's color far off at world `p`, display values: each terrain type's color, blended
// across the cell.
float3 land_color(float2 p)
{
    if (push.drawn == NONE || push.words == NONE)
    {
        return float3(0.3, 0.3, 0.25);
    }
    float2 q = p / CELL;
    int2 g = int2(floor(q));
    float2 f = saturate(q - float2(g));
    float3 sum = float3(0.0, 0.0, 0.0);
    for (int k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        uint t = (vertex_word(g + o) >> 2) & 0x1F;
        float w = lerp(1.0 - f.x, f.x, float(o.x)) * lerp(1.0 - f.y, f.y, float(o.y));
        sum += row(push.drawn, D_COLORS + t).rgb * w;
    }
    return sum;
}

// How far off the land's color has given way to the air's, meters for most of it.
static const float AIR_REACH = 3500.0;

// The land far off the way `dir` goes from `from`, found along the world's heights from `t0`
// meters on, lit by the sun and given to the air with its distance; false where the way climbs
// over all of it.
bool land_toward(float3 from, float3 dir, float t0, out float3 color)
{
    color = float3(0.0, 0.0, 0.0);
    uint heights = row_u(push.water, WATER_LAND).x;
    if (heights == NONE || dir.z > 0.5)
    {
        return false;
    }
    float2 g = row(push.water, WATER_GLOBAL).xy;
    float t = t0;
    float before = t0;
    bool met = false;
    for (int i = 0; i < 72; i++)
    {
        float3 q = from + dir * t;
        if (q.z > 3000.0)
        {
            break;
        }
        if (q.z < land_height(heights, q.xy + g))
        {
            met = true;
            break;
        }
        before = t;
        t = t * 1.1 + 4.0;
        if (t > 24000.0)
        {
            break;
        }
    }
    if (!met)
    {
        return false;
    }
    float lo = before;
    float hi = t;
    for (int k = 0; k < 6; k++)
    {
        float mid = (lo + hi) * 0.5;
        float3 q = from + dir * mid;
        if (q.z < land_height(heights, q.xy + g))
        {
            hi = mid;
        }
        else
        {
            lo = mid;
        }
    }
    float3 x = from + dir * hi;
    float2 w = x.xy + g;
    float e = max(CELL * 0.5, hi * 0.01);
    float3 n = normalize(float3(land_height(heights, w - float2(e, 0.0)) - land_height(heights, w + float2(e, 0.0)),
                                land_height(heights, w - float2(0.0, e)) - land_height(heights, w + float2(0.0, e)),
                                2.0 * e));
    float3 own = resolved(push.frame, land_color(w), lit(CLASS_GROUND, n, 1.0, false, -dir), 1.0).rgb;
    float air = 1.0 - exp(-hi / AIR_REACH);
    float3 sky = sky_toward(from, normalize(float3(dir.xy, 0.01)));
    color = lerp(own, sky, air);
    return true;
}

// Retail's height at world `p`, across its triangles: the water's level where it is water.
float level_at(uint heights, float2 p)
{
    float2 q = p / CELL;
    int2 c = int2(floor(q));
    float2 f = saturate(q - float2(c));
    float h00 = vertex_height(heights, c);
    float h10 = vertex_height(heights, c + int2(1, 0));
    float h01 = vertex_height(heights, c + int2(0, 1));
    float h11 = vertex_height(heights, c + int2(1, 1));
    uint x = asuint(c.x);
    uint y = asuint(c.y);
    uint v = (x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu;
    if ((v >> 31) != 0)
    {
        return f.x >= f.y ? h00 + f.x * (h10 - h00) + f.y * (h11 - h10)
                          : h00 + f.y * (h01 - h00) + f.x * (h11 - h01);
    }
    return f.x + f.y <= 1.0 ? h00 + f.x * (h10 - h00) + f.y * (h01 - h00)
                            : h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11);
}

// How far over the water's level the landscape vertex `g` stands: nought in the water.
float over_level(uint shore, int2 g)
{
    int2 c = clamp(g, int2(0, 0), int2(SIDE - 1, SIDE - 1));
    return asfloat(buffers[NonUniformResourceIndex(shore)].Load(uint(c.y * SIDE + c.x) * 8 + 4));
}

// How far inside the water world `p` lies, meters, negative on land, between the vertices; far
// outside it where the ground does not say where the water lies.
float inside_water(float2 p)
{
    uint shore = row_u(push.water, WATER_LAND).y;
    if (shore == NONE)
    {
        return -1000.0;
    }
    float2 q = p / CELL;
    int2 g = int2(floor(q));
    float2 f = saturate(q - float2(g));
    float s[4];
    for (int k = 0; k < 4; k++)
    {
        int2 c = clamp(g + int2(k & 1, k >> 1), int2(0, 0), int2(SIDE - 1, SIDE - 1));
        s[k] = asfloat(buffers[NonUniformResourceIndex(shore)].Load(uint(c.y * SIDE + c.x) * 8));
    }
    return lerp(lerp(s[0], s[1], f.x), lerp(s[2], s[3], f.x), f.y);
}

// The water's level at world `p`, across retail's triangles: level everywhere the water reaches,
// its banks' vertices taken down to the water beside them, as the ground draws its bed under it.
// Retail's height there where the ground does not say where the water lies.
float water_level(uint heights, float2 p)
{
    uint shore = row_u(push.water, WATER_LAND).y;
    if (shore == NONE)
    {
        return level_at(heights, p);
    }
    float2 q = p / CELL;
    int2 c = int2(floor(q));
    float2 f = saturate(q - float2(c));
    float h00 = vertex_height(heights, c) - over_level(shore, c);
    float h10 = vertex_height(heights, c + int2(1, 0)) - over_level(shore, c + int2(1, 0));
    float h01 = vertex_height(heights, c + int2(0, 1)) - over_level(shore, c + int2(0, 1));
    float h11 = vertex_height(heights, c + int2(1, 1)) - over_level(shore, c + int2(1, 1));
    uint x = asuint(c.x);
    uint y = asuint(c.y);
    uint v = (x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu;
    if ((v >> 31) != 0)
    {
        return f.x >= f.y ? h00 + f.x * (h10 - h00) + f.y * (h11 - h10)
                          : h00 + f.y * (h01 - h00) + f.x * (h11 - h01);
    }
    return f.x + f.y <= 1.0 ? h00 + f.x * (h10 - h00) + f.y * (h01 - h00)
                            : h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11);
}

// Where the line of sight to the bed at `bed`, drawn about `under` meters under the water, meets
// the water's surface: on the water's own level, which runs smooth where the bed's depth is
// written in steps. False where the eye is not above it.
bool on_surface(float3 bed, float under, out float3 surface)
{
    surface = bed;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float level = bed.z + under;
    uint heights = row_u(push.water, WATER_LAND).x;
    float2 g = row(push.water, WATER_GLOBAL).xy;
    for (int k = 0; k < 3; k++)
    {
        if (eye.z <= level || level <= bed.z)
        {
            return false;
        }
        surface = eye + (bed - eye) * ((eye.z - level) / (eye.z - bed.z));
        if (heights == NONE)
        {
            return true;
        }
        level = water_level(heights, surface.xy + g);
    }
    if (eye.z <= level || level <= bed.z)
    {
        return false;
    }
    surface = eye + (bed - eye) * ((eye.z - level) / (eye.z - bed.z));
    return true;
}

// How much of world `p` is water: its cell's water corners, blended across the cell.
float water_share(float2 p)
{
    float2 q = p / CELL;
    int2 g = int2(floor(q));
    float2 f = saturate(q - float2(g));
    float share = 0.0;
    for (int k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        uint t = (vertex_word(g + o) >> 2) & 0x1F;
        if (t >= WATER_FIRST && t <= WATER_LAST)
        {
            share += lerp(1.0 - f.x, f.x, float(o.x)) * lerp(1.0 - f.y, f.y, float(o.y));
        }
    }
    return share;
}

// Whether the picture draws water on the ground under `surface`, a point on the water's level: 1
// where the ground it shows there is water, 0 where it is land, and -1 where it does not show that
// ground. The ground is drawn water where retail's blend says so, which the corners' share only
// approaches along a shore. Where something stands in front of that ground (the body the line of
// sight met, most often), the first ground the picture shows below it in its column stands for it,
// so long as it lies near.
int drawn_water_under(float3 surface)
{
    uint heights = row_u(push.water, WATER_LAND).x;
    float2 g = row(push.water, WATER_GLOBAL).xy;
    float3 ground = float3(surface.xy, min(land_height(heights, surface.xy + g), surface.z));
    float w;
    float2 at_px = screen_of(ground, w);
    float2 e = extent();
    if (w <= 0.0 || any(at_px < 0.0) || at_px.x >= e.x || at_px.y >= e.y)
    {
        return -1;
    }
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float near = max(1.5, 0.05 * distance(eye, ground));
    int2 q = int2(at_px);
    int step = max(int(e.y) / 270, 1);
    for (int k = 0; k < 48; k++)
    {
        if (q.y >= int(e.y))
        {
            return -1;
        }
        uint cls = class_at(q);
        if (cls == CLASS_NONE || cls == CLASS_SKY)
        {
            return -1;
        }
        if (cls == CLASS_GROUND)
        {
            // The ground shown must be the ground under the surface, not some ground past it or
            // short of it.
            float3 shown = world_at(float2(q) + 0.5, at(push.outdoor_depth, q).x);
            if (distance(shown.xy, ground.xy) > near)
            {
                return -1;
            }
            return kind_at(q) == KIND_WATER ? 1 : 0;
        }
        q.y += step;
    }
    return -1;
}

// Where the line of sight to `world`, a point of something standing in the water, crosses the
// water's surface above it: false where it does not stand below the water, or the eye is under
// the water, or the water is not drawn there: the ground the picture shows under the crossing is
// land, or, where the picture does not show it, the corners there are not water.
bool under_water(float3 world, out float3 surface)
{
    surface = world;
    uint heights = row_u(push.water, WATER_LAND).x;
    if (heights == NONE || push.words == NONE)
    {
        return false;
    }
    float2 g = row(push.water, WATER_GLOBAL).xy;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float level = water_level(heights, world.xy + g);
    for (int k = 0; k < 2; k++)
    {
        if (world.z >= level - 0.005 || eye.z <= level)
        {
            return false;
        }
        surface = eye + (world - eye) * ((eye.z - level) / (eye.z - world.z));
        level = water_level(heights, surface.xy + g);
    }
    if (world.z >= level - 0.005 || eye.z <= level)
    {
        return false;
    }
    surface = eye + (world - eye) * ((eye.z - level) / (eye.z - world.z));
    int drawn = drawn_water_under(surface);
    return drawn < 0 ? water_share(surface.xy + g) > 0.5 : drawn > 0;
}

// What lies far off the way `dir` goes from `from`, past `t0` meters: the land, then the sky.
float3 far_off(float3 from, float3 dir, float t0)
{
    float3 land;
    if (land_toward(from, dir, t0, land))
    {
        return land;
    }
    return sky_toward(from, dir);
}

// Whether `q` (the frame's space) is part of the viewer's own body, or stands at the camera:
// what the camera sees of either is never what a reflection shows.
bool viewers(float3 q)
{
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    if (length(q - eye) < 1.5)
    {
        return true;
    }
    float4 body = row(push.water, WATER_VIEWER);
    if (body.w < 0.5)
    {
        return false;
    }
    float3 d = q - body.xyz;
    return length(d.xy) < 0.9 && d.z > -0.3 && d.z < 2.4;
}

// What a reflected ray found: a surface it met, the land, or the sky.
static const uint FOUND_SURFACE = 0;
static const uint FOUND_LAND = 1;
static const uint FOUND_SKY = 2;

struct Found
{
    float3 color;
    uint what;
};

Found found(float3 color, uint what)
{
    Found f;
    f.color = color;
    f.what = what;
    return f;
}

// What lies far off, as a reflection finds it.
Found found_far(float3 from, float3 dir, float t0)
{
    float3 land;
    if (land_toward(from, dir, t0, land))
    {
        return found(land, FOUND_LAND);
    }
    return found(sky_toward(from, dir), FOUND_SKY);
}

// Whether `q` (the frame's space) stands behind what the pixel it lands on shows.
bool is_behind(float3 q)
{
    float w;
    float2 pos = screen_of(q, w);
    if (w <= 0.0 || any(pos < 0.0) || any(pos >= extent()))
    {
        return false;
    }
    int2 px = int2(pos);
    uint cls = class_at(px);
    float d = at(depth_of(cls), px).x;
    if (cls == CLASS_NONE || d >= 1.0)
    {
        return false;
    }
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    return length(q - eye) > length(world_at(float2(px) + 0.5, d) - eye);
}

// How much of what a reflected ray found on screen at `pos` is kept: none at the picture's sides
// and top, where what lies past the picture takes over.
float kept_on_screen(float2 pos)
{
    float2 u = pos / extent();
    return smoothstep(0.0, 0.15, min(u.x, 1.0 - u.x)) * smoothstep(0.0, 0.1, u.y);
}

// The world on screen along the ray from `from` the way `dir` goes:
// - a surface it meets close behind, found between its last two steps by halving, but never the
//   viewer's own body or what stands at the camera, faded toward the picture's sides and top;
// - where it leaves the picture or goes past its reach running behind the land, the land's
//   color, never the sky's, which would rim the land with sky;
// - elsewhere, what lies far off that way: behind something that is not the land, what the
//   picture shows of it says nothing of what the ray meets.
// The pixels round where it starts are its own surface, and are passed over.
Found traced(float3 from, float3 dir, int2 start)
{
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float2 e = extent();
    bool behind = false;
    bool behind_land = false;
    // What the picture shows that the ray found, how much of it is kept over what lies far off,
    // and what it is.
    float3 shown = float3(0.0, 0.0, 0.0);
    float keep = 0.0;
    uint what = FOUND_SKY;
    float t = 0.25;
    for (int i = 0; i < 80; i++)
    {
        float3 q = from + dir * t;
        t *= 1.12;
        float w;
        float2 pos = screen_of(q, w);
        if (w <= 0.0 || any(pos < 0.0) || any(pos >= e))
        {
            break;
        }
        int2 px = int2(pos);
        if (all(abs(px - start) <= 1))
        {
            continue;
        }
        uint cls = class_at(px);
        float d = at(depth_of(cls), px).x;
        if (cls == CLASS_NONE || d >= 1.0)
        {
            // Nothing stands there: the sky, which the ray passes in front of.
            behind = false;
            behind_land = false;
            continue;
        }
        float ray = length(q - eye);
        float surface = length(world_at(float2(px) + 0.5, d) - eye);
        if (ray <= surface)
        {
            behind = false;
            behind_land = false;
            continue;
        }
        // Behind what the pixel shows, having just been in front of something: where it crossed,
        // between this step and the last, by halving. Close behind the surface there, it met it;
        // far behind, it passed behind the edge of something nearer.
        if (!behind)
        {
            float lo = t / (1.12 * 1.12);
            float hi = t / 1.12;
            for (int k = 0; k < 6; k++)
            {
                float mid = (lo + hi) * 0.5;
                if (is_behind(from + dir * mid))
                {
                    hi = mid;
                }
                else
                {
                    lo = mid;
                }
            }
            float3 crossed = from + dir * hi;
            float hw;
            float2 met = screen_of(crossed, hw);
            int2 met_px = clamp(int2(met), int2(0, 0), int2(e) - 1);
            uint met_cls = class_at(met_px);
            float met_d = at(depth_of(met_cls), met_px).x;
            float3 met_at = world_at(float2(met_px) + 0.5, met_d);
            float met_surface = length(met_at - eye);
            if (met_cls != CLASS_NONE && met_d < 1.0 && kind_at(met_px) != KIND_WATER
                && length(crossed - eye) - met_surface < 0.25 + met_surface * 0.03
                && !viewers(met_at))
            {
                shown = at(push.copy, met_px).rgb;
                keep = kept_on_screen(met);
                what = FOUND_SURFACE;
                behind = false;
                behind_land = false;
                break;
            }
        }
        behind = true;
        behind_land = cls == CLASS_GROUND && kind_at(px) != KIND_WATER;
        shown = at(push.copy, px).rgb;
        keep = kept_on_screen(pos);
    }
    if (what != FOUND_SURFACE)
    {
        if (behind && behind_land)
        {
            what = FOUND_LAND;
        }
        else
        {
            keep = 0.0;
        }
    }
    if (keep >= 1.0)
    {
        return found(shown, what);
    }
    Found far = found_far(from, dir, 2.0);
    return found(lerp(far.color, shown, keep), keep > 0.5 ? what : far.what);
}

// How far a traced reflection reaches through the scene, meters; past it, what lies far off.
static const float TRACE_REACH = 2500.0;

// Whether anything stands between `x` and the sun, as the traced shadows find it.
bool sun_blocked(uint accel, float3 x, float3 n, float3 sun)
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
    return q.CommittedStatus() != COMMITTED_NOTHING;
}

// What a ray traced through the scene met: how far along, and which triangle of what.
struct Hit
{
    float t;
    uint instance;
    uint geometry;
    uint index;
    uint prim;
    float2 bary;
    float3x4 to_world;
    bool front;
};

// The first surface along `ray` that its pictures keep; false where it meets none.
bool first_hit(uint accel, uint statics, uint objects, uint indices, uint vertices, uint materials,
               uint sampler, RayDesc ray, out Hit h)
{
    RayQuery<RAY_FLAG_NONE> q;
    q.TraceRayInline(accels[NonUniformResourceIndex(accel)], RAY_FLAG_NONE, 0xFF, ray);
    while (q.Proceed())
    {
        if (q.CandidateType() == CANDIDATE_NON_OPAQUE_TRIANGLE
            && alpha_kept(statics, objects, indices, vertices, materials, sampler,
                          q.CandidateInstanceID(), q.CandidateGeometryIndex(),
                          q.CandidatePrimitiveIndex(), q.CandidateTriangleBarycentrics()))
        {
            q.CommitNonOpaqueTriangleHit();
        }
    }
    h.t = 0.0;
    h.instance = 0;
    h.geometry = 0;
    h.index = 0;
    h.prim = 0;
    h.bary = float2(0.0, 0.0);
    h.to_world = float3x4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    h.front = true;
    if (q.CommittedStatus() != COMMITTED_TRIANGLE_HIT)
    {
        return false;
    }
    h.t = q.CommittedRayT();
    h.instance = q.CommittedInstanceID();
    h.geometry = q.CommittedGeometryIndex();
    h.index = q.CommittedInstanceIndex();
    h.prim = q.CommittedPrimitiveIndex();
    h.bary = q.CommittedTriangleBarycentrics();
    h.to_world = q.CommittedObjectToWorld3x4();
    h.front = q.CommittedTriangleFrontFace();
    return true;
}

// Whether world `x` (the frame's space), a point of retail's ground, lies under the water as the
// ground is drawn: inside the water, or at or under its level where there is water. The ground
// the acceleration structures hold there is the water's own level, or a bank the drawn ground
// lowers under it, which a reflection off the water passes over.
bool on_the_water(float3 x)
{
    uint heights = row_u(push.water, WATER_LAND).x;
    if (heights == NONE || push.words == NONE)
    {
        return false;
    }
    float2 p = x.xy + row(push.water, WATER_GLOBAL).xy;
    return inside_water(p) > 0.0 || (water_share(p) > 0.0 && x.z <= water_level(heights, p) + 0.05);
}

// The scene along the ray from `from` the way `dir` goes, traced through its acceleration
// structures: what it meets, its own color lit as the resolve lights it and fogged over the way
// from the eye; what lies far off where it meets nothing. The water's own level, which the
// structures hold as ground, is passed over. `dist` is how far `from` lies from the eye.
Found traced_scene(float3 from, float3 dir, float dist)
{
    uint4 a = row_u(push.water, WATER_TRACE);
    uint4 b = row_u(push.water, WATER_TRACE + 1);
    uint4 c = row_u(push.water, WATER_TRACE + 2);
    uint statics_first = row_u(push.water, WATER_SKY).x;
    uint accel = a.x;
    uint statics = a.y;
    uint objects = a.z;
    uint indices = a.w;
    uint vertices = b.x;
    uint materials = b.y;
    uint sampler = b.z;
    uint instances = b.w;
    uint ground_vertices = c.x;
    uint ground_cells = c.y;
    RayDesc ray;
    ray.Origin = from + float3(0.0, 0.0, 0.02);
    ray.Direction = dir;
    ray.TMin = 0.05;
    ray.TMax = TRACE_REACH;
    Hit h;
    uint4 g = uint4(0, 0, 0, 0);
    bool met = false;
    for (int k = 0; k < 3 && !met; k++)
    {
        if (!first_hit(accel, statics, objects, indices, vertices, materials, sampler, ray, h))
        {
            return found_far(from, dir, 20.0);
        }
        g = geometry_row(statics, objects, h.instance, h.geometry);
        met = (g.w & RT_IS_GROUND) == 0 || !on_the_water(ray.Origin + dir * h.t);
        ray.TMin = h.t + 0.05;
    }
    if (!met)
    {
        return found_far(from, dir, 20.0);
    }
    float t = h.t;
    uint prim = h.prim;
    float2 bary = h.bary;
    float w[3] = { 1.0 - bary.x - bary.y, bary.x, bary.y };
    float3x4 to_world = h.to_world;
    float3 x = ray.Origin + dir * t;
    // How sharply the hit's pictures are read: the farther the way from the eye, the coarser.
    float lod = clamp(log2(max((dist + t) * 0.15, 1.0)), 0.0, 6.0);
    float3 n;
    float3 albedo;
    uint cls;
    bool leaf = false;
    bool front = h.front;
    if ((g.w & RT_IS_GROUND) != 0)
    {
        // The ground: the cell's corners' normals, and its picture across the cell, or each
        // terrain type's color where it has none.
        uint side = g.w >> 8;
        uint cell = prim / 2;
        uint i = cell / side;
        uint j = cell % side;
        uint4 cw = row_u(ground_cells, g.z + cell);
        bool cut = (cw.z & 1) != 0;
        uint svc = side + 1;
        uint sw = i * svc + j;
        uint se = (i + 1) * svc + j;
        uint ne = (i + 1) * svc + j + 1;
        uint nw = i * svc + j + 1;
        uint3 tri = (prim & 1) == 0 ? (cut ? uint3(sw, se, ne) : uint3(sw, se, nw))
                                    : (cut ? uint3(sw, ne, nw) : uint3(ne, nw, se));
        n = float3(0.0, 0.0, 0.0);
        float3 local = float3(0.0, 0.0, 0.0);
        for (uint v = 0; v < 3; v++)
        {
            local += row(ground_vertices, (g.y + tri[v]) * 2).xyz * w[v];
            n += row(ground_vertices, (g.y + tri[v]) * 2 + 1).xyz * w[v];
        }
        n = normalize(n);
        albedo = land_color(x.xy + row(push.water, WATER_GLOBAL).xy);
        if (cw.x != NONE)
        {
            // The cell's picture, turned as the cell turns it.
            float2 f = saturate((local.xy - float2(float(i), float(j)) * CELL) / CELL);
            uint rot = cw.y & 3;
            static const float2 UVS[4] = { float2(0.0, 1.0), float2(1.0, 1.0), float2(1.0, 0.0),
                                           float2(0.0, 0.0) };
            float2 uv_sw = UVS[(0 + 4 - rot) & 3];
            float2 uv_se = UVS[(1 + 4 - rot) & 3];
            float2 uv_ne = UVS[(2 + 4 - rot) & 3];
            float2 uv_nw = UVS[(3 + 4 - rot) & 3];
            float2 uv = lerp(lerp(uv_sw, uv_se, f.x), lerp(uv_nw, uv_ne, f.x), f.y);
            albedo = textures[NonUniformResourceIndex(cw.x)]
                         .SampleLevel(samplers[NonUniformResourceIndex(sampler)], uv, lod)
                         .rgb;
        }
        cls = CLASS_GROUND;
        front = true;
    }
    else
    {
        // A mesh: its corners' normals turned into the world, and its texture.
        ByteAddressBuffer ib = buffers[NonUniformResourceIndex(indices)];
        float3 nl = float3(0.0, 0.0, 0.0);
        float2 uv = float2(0.0, 0.0);
        float3 tint = float3(0.0, 0.0, 0.0);
        for (uint v = 0; v < 3; v++)
        {
            uint vi = g.y + ib.Load((g.x + prim * 3 + v) * 4);
            float4 r0 = row(vertices, vi * 4);
            float4 r1 = row(vertices, vi * 4 + 1);
            nl += float3(r0.w, r1.x, r1.y) * w[v];
            uv += r1.zw * w[v];
            tint += row(vertices, vi * 4 + 3).rgb * w[v];
        }
        n = normalize(mul(to_world, float4(nl, 0.0)).xyz);
        if (!front)
        {
            n = -n;
        }
        uint4 m = row_u(materials, g.z * 2);
        albedo = float3(1.0, 1.0, 1.0);
        if ((m.y & 64) != 0)
        {
            albedo = textures[NonUniformResourceIndex(m.x)]
                         .SampleLevel(samplers[NonUniformResourceIndex(sampler)], uv, lod)
                         .rgb;
        }
        // A surface drawn under its vertices' colors.
        if ((m.y & 512) != 0)
        {
            albedo *= tint;
        }
        uint index = h.index;
        uint4 entry = index < statics_first
                          ? row_u(c.z, index * 2)
                          : row_u(c.w, (index - statics_first) * 2);
        uint4 info = row_u(instances, entry.x * INSTANCE_ROWS + 5);
        cls = info.y;
        leaf = uint(row(materials, g.z * 2 + 1).z) == KIND_FOLIAGE;
    }
    // Rooms and what stands in them take the ambient alone: water outdoors seldom sees them.
    if (cls != CLASS_GROUND && cls != CLASS_SUN)
    {
        cls = CLASS_SUN;
    }
    float3 sun = row(push.frame, FRAME_SUN).xyz;
    float reach = 1.0;
    if (dot(sun, sun) > 1e-8 && dot(n, sun) > 0.0 && sun_blocked(accel, x, n, sun))
    {
        reach = 0.0;
    }
    float fog = air_frame(push.frame) ? 1.0 : fog_factor(push.frame, dist + t);
    float3 color = resolved(push.frame, albedo, lit(cls, n, reach, leaf, -dir), fog).rgb;
    return found(color, cls == CLASS_GROUND ? FOUND_LAND : FOUND_SURFACE);
}

// How a reflection is found: on screen, or through the scene.
interface IReflection
{
    Found seen(float3 from, float3 dir, int2 start, float dist);
};

// What the scene mirrored in the water shows the way `dir` goes from `from`, a point of the
// water's surface; false where it is not drawn, `from` does not lie at the level it is mirrored
// in, or the way leaves its picture.
bool mirrored(float3 from, float3 dir, out float3 color)
{
    color = float3(0.0, 0.0, 0.0);
    uint4 w = row_u(push.water, WATER_PLANAR);
    if (w.x == NONE || abs(from.z - asfloat(w.z)) > 0.3)
    {
        return false;
    }
    // The mirrored view sees the reflected ray's mirror image, which runs on down from `from`:
    // where it stands some way along, so a surface bent by its ripples looks a little aside.
    float3 q = from + float3(dir.xy, -dir.z) * 10.0;
    float4 c = transform(push.water, WATER_PLANAR + 1, float4(q, 1.0));
    if (c.w <= 0.0)
    {
        return false;
    }
    float2 uv = float2(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
    if (any(uv < 0.0) || any(uv > 1.0))
    {
        return false;
    }
    color = textures[NonUniformResourceIndex(w.x)]
                .SampleLevel(samplers[NonUniformResourceIndex(w.y)], uv, 0.0)
                .rgb;
    return true;
}

struct OnScreen : IReflection
{
    Found seen(float3 from, float3 dir, int2 start, float dist)
    {
        float3 color;
        if (mirrored(from, dir, color))
        {
            return found(color, FOUND_SURFACE);
        }
        return traced(from, dir, start);
    }
};

struct ThroughScene : IReflection
{
    Found seen(float3 from, float3 dir, int2 start, float dist)
    {
        return traced_scene(from, dir, dist);
    }
};

float4 shade<R : IReflection>(float4 position, R reflection)
{
    int2 p = int2(position.xy);
    float4 m = at(push.material, p);
    uint cls = uint(m.x * 255.0 + 0.5);
    uint kind = uint(m.y * 255.0 + 0.5);
    float4 state = row(push.water, WATER_STATE);
    bool drawn = row(push.water, WATER_DRAWN).y > 0.5;
    bool water = drawn && kind == KIND_WATER;
    float d = at(depth_of(cls), p).x;
    float3 world = world_at(float2(p) + 0.5, d);
    // Something standing in the water, below its surface: the water is drawn over it there, the
    // surface where the line of sight crosses the water's level and what stands under it seen
    // through the water as the bed is.
    float3 crossing = world;
    bool wading = drawn && !water && cls != CLASS_GROUND && cls != CLASS_NONE && d < 1.0
                  && under_water(world, crossing);
    if (!water && !wading && !(state.w > 0.5 && cls == CLASS_GROUND))
    {
        discard;
    }
    // How much of the pixel is water, where the ground says (its third byte); all of it where it
    // does not.
    float share = water && m.z > 0.0 ? m.z : 1.0;
    if (wading)
    {
        water = true;
        share = 1.0;
    }
    else if (!water)
    {
        // A puddle where the ground gathers one: its share, which the weather lays in the
        // ground's material (its fourth byte, where the ground is not water) on flat dry ground,
        // fixed in the world.
        share = m.w;
        if (share <= 0.0)
        {
            discard;
        }
    }
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    // Where the bed is drawn under the water, the surface is where the view's ray meets the
    // water's level above it: what stands in the water meets its reflection there.
    float under = water ? float(uint(m.w * 255.0 + 0.5)) * BED_STEP : 0.0;
    float3 bed = world;
    if (wading)
    {
        under = crossing.z - world.z;
        world = crossing;
    }
    else if (under > 0.0)
    {
        float3 surface;
        if (on_surface(bed, under, surface))
        {
            under = surface.z - bed.z;
            world = surface;
        }
    }
    float dist = length(eye - world);
    float3 v = (eye - world) / max(dist, 1e-4);
    float t = state.x;
    float2 slope;
    float2 ringed = float2(0.0, 0.0);
    float3 n;
    if (water)
    {
        // How far the water moves from this pixel to the next, across and down.
        float2 pos = float2(p) + 0.5;
        float2 dx = on_level(pos + float2(1.0, 0.0), world) - world.xy;
        float2 dy = on_level(pos + float2(0.0, 1.0), world) - world.xy;
        float foot = max(length(dx), length(dy));
        float cap = dist * 0.01 + 0.01;
        dx *= min(1.0, cap / max(length(dx), 1e-6));
        dy *= min(1.0, cap / max(length(dy), 1e-6));
        float2 rippled = ripples(world.xy, dist, dx, dy, foot, state.y);
        ringed = rings(world, t, state.z);
        slope = rippled + ringed;
        // Reflected off a surface bent half as much as the ripples bend the light through it, so
        // the reflections waver rather than break up; the rings' crests whole, so they stand out.
        n = normalize(float3(-(rippled * 0.5 + ringed), 1.0));
    }
    else
    {
        slope = waves(world.xy, t, dist, state.y * 0.15);
        n = normalize(float3(-slope, 1.0));
    }
    float3 r = reflect(-v, n);
    r.z = max(r.z, 0.02);
    r = normalize(r);
    Found seen = reflection.seen(world, r, p, dist);
    if (row(push.water, WATER_DRAWN).z > 0.5)
    {
        // Checking the reflection: what it found, red a surface met, green the land, blue the
        // sky.
        return float4(float(seen.what == FOUND_SURFACE), float(seen.what == FOUND_LAND),
                      float(seen.what == FOUND_SKY), 1.0);
    }
    float cosine = saturate(dot(n, v));
    float fresnel = 0.02 + 0.98 * pow(1.0 - cosine, 5.0);
    // The water's own color, bent by its slope as the surface bends what is seen through it.
    int2 bent = clamp(p + int2(round(slope * extent().y * 0.04)), int2(0, 0), int2(extent()) - 1);
    int2 through_at = water && !wading && kind_at(bent) == KIND_WATER ? bent : p;
    float3 own = at(push.copy, through_at).rgb;
    float absorption = row(push.water, WATER_DRAWN).w;
    float reflects = 1.0;
    if (water && (under > 0.0 || (push.drawn != NONE && row(push.drawn, D_BED).x > 0.5)))
    {
        // Only as the water deepens from its edge, so a film of water over the bank shows the wet
        // ground rather than a pale rim of sky.
        reflects = smoothstep(0.0, 0.3, under);
    }
    if (water && under > 0.0 && absorption > 0.0 && push.drawn != NONE)
    {
        // The water's own color: retail's water under the light the bed takes, as the resolve has
        // lit the bed against its own color. Far off, in the fog, what the fog left of the bed.
        float3 hue = hue_at(world.xy + row(push.water, WATER_GLOBAL).xy);
        float3 albedo = at(push.albedo, through_at).rgb;
        if (linear_frame(push.frame))
        {
            hue = to_linear(hue);
            albedo = to_linear(albedo);
        }
        float3 light = clamp((own + 0.003) / (albedo + 0.003), 0.0, 6.0);
        float3 body = air_frame(push.frame) ? hue * light : lerp(own, hue * light, fog_factor(push.frame, dist));
        // Light reaching the bed and back up, faded by the water it passes through: red first.
        float through = length(bed - world);
        float3 keep = exp(-float3(1.3, 1.0, 0.82) / CLARITY * absorption * (through + through * v.z));
        own = own * keep + body * (1.0 - keep);
    }
    float3 c;
    if (water)
    {
        c = lerp(own, seen.color * REFLECTED_TINT, saturate(fresnel * REFLECTED * share * reflects));
        // A ring's steep faces catch what the water reflects at a low angle, seen from above as
        // well: the rings show as light lines on the water.
        float sheen = saturate(length(ringed) * 1.5) * 0.3 * reflects;
        c = lerp(c, seen.color * REFLECTED_TINT, sheen);
    }
    else
    {
        // A puddle: held down far off and at grazing angles, so it never turns to a white sheet,
        // and faded out with distance.
        float hold = lerp(0.75, 0.4, smoothstep(8.0, 45.0, dist))
                     * lerp(0.55, 1.0, smoothstep(0.03, 0.25, cosine));
        float fade = 1.0 - smoothstep(15.0, 45.0, dist);
        c = lerp(own, seen.color * 0.8, saturate(fresnel * 1.2 + 0.12) * hold * share * fade);
    }
    // The sun's glint off the waves, where the sun reaches.
    float3 sun = row(push.frame, FRAME_SUN).xyz;
    if (water && dot(sun, sun) > 1e-8)
    {
        float3 l = normalize(sun);
        float reach = push.sun_visibility != NONE ? at(push.sun_visibility, p).x : 1.0;
        float glint = pow(saturate(dot(r, l)), 400.0) * 4.0 * reach * reflects;
        float3 color = row(push.frame, FRAME_SUN_COLOR).rgb * length(sun);
        if (linear_frame(push.frame))
        {
            color = to_linear(color);
        }
        c += color * glint;
    }
    return float4(c, 1.0);
}

// The water with its reflection found on screen.
[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    OnScreen s;
    return shade(position, s);
}

// The water with its reflection traced through the scene, where the device traces rays.
[shader("fragment")]
float4 fs_traced(float4 position : SV_Position) : SV_Target0
{
    ThroughScene s;
    return shade(position, s);
}
