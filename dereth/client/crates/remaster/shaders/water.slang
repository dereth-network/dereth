// The water, drawn over the resolved world: where the ground is water (the material's kind 7),
// and, with puddles asked for, where the ground gathers them: the share the weather lays in the
// ground's material on a rainy day, held down far off and at grazing angles.
//
// For each such pixel the surface's normal is the water's level bent by its ripples and by the rings
// round the bodies standing in it; the world on screen is found along the reflected ray, and laid
// over the pixel's own color by the Fresnel share; and the sun glints off it where the sun reaches.
//
// - **Ripples** are one small picture of wind ripples laid several times over the water, at
//   different sizes and turns, each drifting with the scene's clock alone, so a held clock draws a
//   still frame; they calm with distance, so far water lies smooth, and a ripple finer than a pixel
//   is left out, so it never shimmers. Puddles keep a few small waves of their own.
// - **Rings** spread from where each body stands now, stronger as it moves, so they show from the
//   first step and follow it.
// - **The reflection** marches the ray through the depth. Where it meets a surface, it takes that
//   surface's color. Where it leaves the picture running behind the land, it takes the land's
//   color, never the sky's, which would rim the land with sky; over the top it takes what the edge
//   shows round where it leaves, land and sky together, so the water does not stand in columns of
//   each; past a side it takes the sky's that way. Toward the
//   picture's sides, what it found fades into that sky, so the reflection has no hard edge there.
// - **The water's own color**, over a bed drawn under it, is retail's water under the light the bed
//   takes: what is seen through the water fades into it with the length of water the view runs
//   through, red first.
//
// Its own pictures are a copy of the resolved world (`copy`); it writes the scene's colors.
import bindless;
import intent;
import retail;

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
//      second)
static const uint WATER_STATE = 0;
static const uint WATER_DRAWN = 1;
// The drawn ground's constants' row whose x is 1 while the bed is lowered under the water.
static const uint D_BED = 6;
static const uint WATER_GLOBAL = 2;
static const uint WATER_LAYERS = 3;
static const uint LAYERS = 4;
static const uint WATER_WADERS = 11;
static const uint WADERS = 8;

// The water's kind, as the material target's second byte holds it.
static const uint KIND_WATER = 7;
// The step, meters, of how far under the water's surface the ground draws its bed: the material
// target's fourth byte on the ground (0 where the bed is not lowered).
static const float BED_STEP = 0.04;

static const float PI = 3.14159265;

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

// How the surface leans at `p` from the rings round the bodies standing in the water: each body's
// rings spread from where it stands now, a few metres out, their height as it moves.
float2 rings(float3 p, float t, float strength)
{
    float2 slope = float2(0.0, 0.0);
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
        if (r > 6.0 || r < 1e-3)
        {
            continue;
        }
        // A standing body barely stirs the water; a walking one rings it from where it stands.
        float a = strength * (0.15 + saturate(w.w / 3.0)) * exp(-r / 1.6);
        float k = 2.0 * PI / 0.55;
        slope += (d / r) * cos(r * k - t * 9.0) * a * 0.35;
    }
    return slope;
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

// What lies far off the way `dir` goes from `from`, a way that climbs: the sky the picture shows
// over that way, its clouds and all, well inside the picture; out toward and past its edges, the
// picture's sky along its width, weighted toward that way, which varies slowly however the land
// cuts the picture's edge, so that no ray takes a streak or a wedge of what stands there. `otherwise`
// where the picture shows no sky there.
float3 sky_toward(float3 from, float3 dir, float3 otherwise)
{
    float w;
    float2 pos = screen_of(from + dir * 1.0e5, w);
    if (w <= 0.0)
    {
        return otherwise;
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
    float3 wide = weight > 1.0e-4 ? sum / weight : otherwise;
    float margin = min(min(pos.x, e.x - pos.x) / e.x, min(pos.y, e.y - pos.y) / e.y);
    float keep = smoothstep(0.0, 0.12, margin);
    float3 exact;
    if (keep > 0.0 && sky_above(int2(inside), exact))
    {
        return lerp(wide, exact, keep);
    }
    return wide;
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

// What a reflected ray found at `pos` on the target, faded toward the picture's sides into the sky
// the way `dir` goes from `from`: the world runs off the picture there, and the sky is what a ray
// going past a side takes.
float3 faded_at_side(float3 from, float3 dir, float3 color, float2 pos)
{
    float u = pos.x / extent().x;
    float keep = smoothstep(0.0, 0.2, min(u, 1.0 - u));
    return keep >= 1.0 ? color : lerp(sky_toward(from, dir, color), color, keep);
}

// What a reflected ray found: a surface it met, the land it ran behind or that the picture's edge
// shows where it left, or the sky.
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

// The world on screen along the ray from `from` the way `dir` goes:
// - the surface it meets, found between its last two steps by halving;
// - where it leaves the picture (or goes past its reach) running behind the land, the land's
//   color, never the sky's, which would rim the land with sky;
// - where it leaves the picture in front of everything, what the picture's edge shows there: the
//   land beyond the picture where the top edge is land, the sky where it is sky or water, or the
//   ray leaves past a side;
// - where it goes past its reach in front of everything, the sky that way.
// The pixels round where it starts are its own surface, and are passed over.
Found traced(float3 from, float3 dir, int2 start)
{
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float2 e = extent();
    bool behind = false;
    float3 occluder = float3(0.0, 0.0, 0.0);
    float2 occluder_at = float2(0.0, 0.0);
    float t = 0.25;
    for (int i = 0; i < 80; i++)
    {
        float3 q = from + dir * t;
        t *= 1.12;
        float w;
        float2 pos = screen_of(q, w);
        if (w <= 0.0 || any(pos < 0.0) || any(pos >= e))
        {
            if (behind)
            {
                break;
            }
            // Where it leaves the picture, between this step and the last, by halving.
            float lo = t / (1.12 * 1.12);
            float hi = t / 1.12;
            for (int k = 0; k < 6; k++)
            {
                float mid = (lo + hi) * 0.5;
                float mw;
                float2 mp = screen_of(from + dir * mid, mw);
                if (mw > 0.0 && all(mp >= 0.0) && all(mp < e))
                {
                    lo = mid;
                }
                else
                {
                    hi = mid;
                }
            }
            float lw;
            int2 edge = clamp(int2(screen_of(from + dir * lo, lw)), int2(0, 0), int2(e) - 1);
            // A ray going up off the water never reflects water: what lies past that edge of the
            // picture is what stands beyond it, and all that is known of that is the sky's way.
            // Past a side, the land the edge shows says nothing of what stands past it, and would
            // lay a wedge of its color on the water; over the top, the land rises on past it.
            uint where_ = class_at(edge);
            bool side = edge.x <= 0 || edge.x >= int(e.x) - 1;
            if (side)
            {
                if (kind_at(edge) == KIND_WATER || where_ == CLASS_NONE)
                {
                    return found(sky_toward(from, dir, fog_light()),
                                 where_ == CLASS_NONE ? FOUND_SKY : FOUND_LAND);
                }
                return found(sky_toward(from, dir, at(push.copy, edge).rgb), FOUND_LAND);
            }
            // Over the top: what the edge shows round where the ray leaves, land or sky, taken
            // together, not the one pixel's, so where the edge runs from land to sky the water
            // shades from one to the other rather than standing in columns of each.
            float3 sky = sky_toward(from, dir, fog_light());
            float3 sum = float3(0.0, 0.0, 0.0);
            float open = 0.0;
            for (int s = -6; s <= 6; s++)
            {
                int2 q = int2(clamp(edge.x + s * 12, 0, int(e.x) - 1), edge.y);
                uint c = class_at(q);
                bool shows_land = c != CLASS_NONE && kind_at(q) != KIND_WATER;
                sum += shows_land ? at(push.copy, q).rgb : sky;
                open += c == CLASS_NONE ? 1.0 : 0.0;
            }
            float3 beyond = sum / 13.0;
            return found(faded_at_side(from, dir, beyond, float2(edge)),
                         open >= 6.5 ? FOUND_SKY : FOUND_LAND);
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
            continue;
        }
        float ray = length(q - eye);
        float surface = length(world_at(float2(px) + 0.5, d) - eye);
        if (ray <= surface)
        {
            behind = false;
            continue;
        }
        // Behind what the pixel shows, having just been in front of something: where it crossed,
        // between this step and the last, by halving. Close behind the surface there, it met it;
        // far behind, it passed behind the edge of something nearer. Telling them apart only after
        // the halving keeps the steps themselves from showing as bands in the reflection.
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
            float met_surface = length(world_at(float2(met_px) + 0.5, met_d) - eye);
            if (met_cls != CLASS_NONE && met_d < 1.0
                && length(crossed - eye) - met_surface < 0.25 + met_surface * 0.03)
            {
                return found(faded_at_side(from, dir, at(push.copy, met_px).rgb, met), FOUND_SURFACE);
            }
        }
        behind = true;
        occluder = at(push.copy, px).rgb;
        occluder_at = pos;
    }
    return behind ? found(faded_at_side(from, dir, occluder, occluder_at), FOUND_LAND)
                  : found(sky_toward(from, dir, fog_light()), FOUND_SKY);
}

[shader("fragment")]
float4 fs_main(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    float4 m = at(push.material, p);
    uint cls = uint(m.x * 255.0 + 0.5);
    uint kind = uint(m.y * 255.0 + 0.5);
    float4 state = row(push.water, WATER_STATE);
    bool water = row(push.water, WATER_DRAWN).y > 0.5 && kind == KIND_WATER;
    if (!water && !(state.w > 0.5 && cls == CLASS_GROUND))
    {
        discard;
    }
    float d = at(depth_of(cls), p).x;
    float3 world = world_at(float2(p) + 0.5, d);
    // How much of the pixel is water, where the ground says (its third byte); all of it where it
    // does not.
    float share = water && m.z > 0.0 ? m.z : 1.0;
    if (!water)
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
    if (under > 0.0 && eye.z > world.z + under)
    {
        world = eye + (world - eye) * ((eye.z - world.z - under) / (eye.z - world.z));
    }
    float dist = length(eye - world);
    float3 v = (eye - world) / max(dist, 1e-4);
    float t = state.x;
    float2 slope;
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
        slope = ripples(world.xy, dist, dx, dy, foot, state.y) + rings(world, t, state.z);
        // Reflected off a surface bent half as much as the ripples bend the light through it, so
        // the reflections waver rather than break up.
        n = normalize(float3(-slope * 0.5, 1.0));
    }
    else
    {
        slope = waves(world.xy, t, dist, state.y * 0.15);
        n = normalize(float3(-slope, 1.0));
    }
    float3 r = reflect(-v, n);
    r.z = max(r.z, 0.02);
    r = normalize(r);
    Found seen = traced(world, r, p);
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
    int2 through_at = water && kind_at(bent) == KIND_WATER ? bent : p;
    float3 own = at(push.copy, through_at).rgb;
    float absorption = row(push.water, WATER_DRAWN).w;
    float reflects = 1.0;
    if (water && (under > 0.0 || (push.drawn != 0xFFFFFFFF && row(push.drawn, D_BED).x > 0.5)))
    {
        // Only as the water deepens from its edge, so a film of water over the bank shows the wet
        // ground rather than a pale rim of sky.
        reflects = smoothstep(0.0, 0.3, under);
    }
    if (water && under > 0.0 && absorption > 0.0 && push.drawn != 0xFFFFFFFF)
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
        float reach = push.sun_visibility != 0xFFFFFFFF ? at(push.sun_visibility, p).x : 1.0;
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
