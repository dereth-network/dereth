// The rain and the snow falling, and the rain's splashes, drawn over the scene's colors after the
// world and its forward pass.
//
// - **Splashes.** Short-lived crowns where the drops land on ground facing up near the viewer,
//   out in the rain: a pass over the screen adds them.
// - **Rain and snow.** Every drop and flake is a particle of a lattice fixed in the world. A
//   lattice fills a box round the viewer: each particle has its own place in the world, and the
//   box chooses only which repeat of the lattice is drawn, so a particle holds still in the world,
//   but for its fall and the wind, however the viewer walks or turns. Only far off, at the box's
//   side, does one leave and the next repeat come in at the other side, and it fades out before
//   it does. Three lattices for each, each wider and sparser than the last, fill the near air, the
//   middle distance and the far. A drop is a streak as long as a camera's shutter would blur it;
//   a flake a soft round mark that sways as it falls. Nothing falls under a roof, and anything
//   drawn nearer hides it.
//
// Every motion repeats over the clock's period a whole number of times, so its wrap is never
// seen.
import bindless;
import intent;
import retail;
import weather;

struct Push
{
    uint frame;
    uint weather;
    uint depth;
    // The outdoor surfaces' depth where the viewer stands inside; the depth otherwise.
    uint outdoor_depth;
    uint material;
    uint normal;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

static const float PI = 3.14159265;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

float2 extent()
{
    return row(push.frame, FRAME_EXTENT).xy;
}

uint class_at(int2 p)
{
    return uint(at(push.material, p).x * 255.0 + 0.5);
}

// The depth a pixel of class `cls` is placed by: the room's where a room is drawn, and the
// outdoors' everywhere else, through the openings too.
float depth_at(int2 p)
{
    uint cls = class_at(p);
    uint t = (cls == CLASS_INDOOR || cls == CLASS_CELL) ? push.depth : push.outdoor_depth;
    return at(t, p).x;
}

float2 ndc_of(float2 px)
{
    float2 e = extent();
    return float2(px.x / e.x * 2.0 - 1.0, 1.0 - px.y / e.y * 2.0);
}

float2 px_of(float2 ndc)
{
    float2 e = extent();
    return float2((ndc.x * 0.5 + 0.5) * e.x, (0.5 - ndc.y * 0.5) * e.y);
}

float3 world_at(float2 px, float d)
{
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc_of(px), d, 1.0));
    return h.xyz / h.w;
}

// A color in the frame's own terms: display values, or linear light for the post passes.
float3 scene_terms(float3 c)
{
    return linear_frame(push.frame) ? to_linear(c) : saturate(c);
}

// The light the overcast air carries, in display values: the fog's color, a little of the
// ground's ambient in it.
float3 air()
{
    float g = gamma_scale(push.frame);
    float3 fog = row(push.frame, FRAME_FOG_COLOR).rgb * g;
    float3 ambient = row(push.frame, FRAME_GROUND_AMBIENT).rgb * g;
    return lerp(fog, ambient, 0.25);
}

float luminance(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// --- time ---------------------------------------------------------------------------------------

// A rate, cycles a second, nudged to a whole number of cycles over the clock's period.
float per_period(float rate)
{
    float p = row(push.weather, W_LATTICE).w;
    return max(round(rate * p), 1.0) / p;
}

// An angular speed, radians a second, nudged the same way.
float turns_per_period(float w)
{
    float p = row(push.weather, W_LATTICE).w;
    return 2.0 * PI * max(round(w * p / (2.0 * PI)), 1.0) / p;
}

// --- splashes -----------------------------------------------------------------------------------

// A hash of lattice cell `c` with `salt`: two numbers, 0 to 1.
float2 rand2(int2 c, uint salt)
{
    return float2(noise(float2(c) + 0.5, 1.0, salt), noise(float2(c) + 0.5, 1.0, salt + 7u));
}

// Splashes where drops land on ground facing up at the world's `q`, with a pixel `footprint`
// meters across: how bright, 0 to 1.
float splashes(float2 q, float t, float footprint)
{
    float s = 0.0;
    float size = 0.2;
    float2 c = q / size;
    int2 base = int2(floor(c));
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            int2 cell = base + int2(i, j);
            float2 r = rand2(cell, 23u);
            float2 centre = (float2(cell) + 0.15 + r * 0.7) * size;
            float rate = per_period(1.7 + r.y);
            float phase = frac(t * rate + r.x * 7.0);
            float life = 0.16;
            if (phase < life)
            {
                float a = phase / life;
                float d = length(q - centre);
                // A crown that widens and fades, and the drop's bright landing point.
                float radius = 0.006 + a * 0.035;
                float width = max(0.004, footprint * 0.7);
                float x = (d - radius) / width;
                float ring = exp(-x * x) * (1.0 - a);
                float y = d / max(0.006, footprint);
                float landing = exp(-y * y) * (1.0 - a) * (1.0 - a);
                // Drops smaller than a pixel give their light to the pixel as a whole.
                float cover = saturate(0.03 / max(footprint, 0.001));
                s += (ring * 0.5 + landing * 0.6) * cover;
            }
        }
    }
    return saturate(s);
}

// The splashes over the scene's colors, added: on ground and what stands outdoors facing up,
// within sixteen meters, out in the rain.
[shader("fragment")]
float4 fs_splash(float4 position : SV_Position) : SV_Target0
{
    int2 px = int2(position.xy);
    uint cls = class_at(px);
    float4 state = row(push.weather, W_STATE);
    float rain = row(push.weather, W_RAIN).w * (1.0 - state.w);
    if ((cls != CLASS_GROUND && cls != CLASS_SUN) || rain <= 0.0)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float d = at(push.outdoor_depth, px).x;
    if (d >= 1.0)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float3 p = world_at(position.xy, d);
    float dist = length(p - row(push.frame, FRAME_EYE).xyz);
    float3 n = oct_decode(at(push.normal, px).xy);
    float up = smoothstep(0.3, 0.85, n.z);
    bool water = uint(at(push.material, px).y * 255.0 + 0.5) == 7;
    if (dist >= 16.0 || up <= 0.3 || water)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float open = 1.0 - shelter_share(push.weather, p, 0.4);
    if (open <= 0.5)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float footprint = dist * row(push.weather, W_SNOW).w;
    float s = splashes(world_xy(push.weather, p.xy), state.z, footprint) * up
        * (1.0 - smoothstep(7.0, 16.0, dist)) * open * rain;
    return float4(scene_terms(air()) * s * 0.6, 0.0);
}

// --- the falling rain and snow ------------------------------------------------------------------

// Seconds a drop or a flake is blurred over, as a camera's shutter would.
static const float SHUTTER = 0.033;
// The widest a streak is drawn, radians either side of its line, and the longest, radians.
static const float STREAK_WIDEST = 0.0012;
static const float STREAK_LONGEST = 0.1;
// The widest a flake is drawn, radians from its middle.
static const float FLAKE_WIDEST = 0.006;

// The rain's lattices, nearest first: x the half-width of the box round the eye, meters; y its
// half-height; z the drop's radius as drawn, meters; w how strongly a streak shows.
float4 rain_tier(uint k)
{
    if (k == 0u)
    {
        return float4(6.0, 6.0, 0.002, 0.6);
    }
    if (k == 1u)
    {
        return float4(18.0, 12.0, 0.0025, 0.5);
    }
    return float4(48.0, 24.0, 0.003, 0.4);
}

// The snow's: x, y as the rain's; z the least radius of a flake, meters, the largest twice it;
// w how strongly a flake shows.
float4 snow_tier(uint k)
{
    if (k == 0u)
    {
        return float4(5.0, 5.0, 0.01, 1.0);
    }
    if (k == 1u)
    {
        return float4(15.0, 10.0, 0.012, 0.95);
    }
    return float4(40.0, 20.0, 0.014, 0.85);
}

uint pcg(uint v)
{
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Three numbers from 0 to 1 for particle `id` of the lattice `salt` names.
float3 hash3(uint id, uint salt)
{
    uint a = pcg(id ^ (salt * 2654435769u));
    uint b = pcg(a + 1759714724u);
    uint c = pcg(b + 3043654221u);
    return float3(float(a >> 8u), float(b >> 8u), float(c >> 8u)) / 16777216.0;
}

// Where particle `id` of the lattice `salt` names stands, in the world wrapped to the lattices'
// period: the repeat of its place that falls in the box `size` meters across centred on `eye`,
// the eye in the same terms. `fall` is how far the lattice has fallen and `drift` how far the
// wind has carried it. The eye chooses only the repeat, never the place.
float3 lattice_place(uint id, uint salt, float3 size, float3 eye, float fall, float2 drift)
{
    float3 own = hash3(id, salt) * size + float3(drift, -fall);
    float3 corner = eye - size * 0.5;
    float3 rel = own - corner;
    return corner + rel - size * floor(rel / size);
}

// How a drop's streak (`flake` false) or a flake `radius` meters across at `dist` meters from the
// eye is drawn, with `pixel` radians to a pixel: x its radius on screen, pixels, no less than a
// little over half a pixel and no more than the widest a streak or a flake is drawn; y how much of
// its light it keeps: less as it is drawn wider than it is (by the ratio for a streak's width, its
// square for a flake's area), so one too small to see is faint rather than bright; and none within
// half a meter of the eye, fading in by a meter and a half, so nothing passes the camera as a blur.
float2 drawn_size(float radius, float dist, float pixel, bool flake)
{
    float widest = flake ? FLAKE_WIDEST : STREAK_WIDEST;
    float seen = radius / (max(dist, 0.01) * pixel);
    float drawn = clamp(seen, 0.6, max(widest / pixel, 0.6));
    float ratio = min(seen / drawn, 1.0);
    float keep = flake ? ratio * ratio : ratio;
    return float2(drawn, keep * smoothstep(0.5, 1.5, dist));
}

// The tail of a streak from `head` toward `tail` on screen, pixels, with `pixel` radians to a
// pixel: where it is, or nearer the head where the streak would be longer than the longest a
// streak is drawn, as it would be just in front of the eye.
float2 streak_tail(float2 head, float2 tail, float pixel)
{
    float longest = STREAK_LONGEST / pixel;
    float run = length(tail - head);
    if (run > longest)
    {
        return head + (tail - head) * (longest / run);
    }
    return tail;
}

// The light a drop (`flake` false) or a flake carries, in display values: the overcast air's,
// a little brighter than it. A flake is white.
float3 fall_light(bool flake)
{
    float3 a = air();
    if (flake)
    {
        float w = min(luminance(a) * 1.5 + 0.08, 1.0);
        return float3(w, w, w);
    }
    return max(a * 1.15, float3(0.02, 0.02, 0.02));
}

struct Falling
{
    float4 position : SV_Position;
    // The particle's two ends on screen, pixels: where it is, and where it was a shutter's time
    // ago.
    nointerpolation float2 head : HEAD;
    nointerpolation float2 tail : TAIL;
    // x its radius on screen, pixels; y how strongly it shows; z how far it is from the eye,
    // meters; w 1 for a flake.
    nointerpolation float4 shape : SHAPE;
    // Its light, in the frame's terms; w how much of what is behind it it hides.
    nointerpolation float4 light : LIGHT;
};

// The quad's corners, two triangles: bit 0 the side across the particle's line, bit 1 its end.
static const uint CORNERS[6] = { 0, 1, 2, 2, 1, 3 };

// One vertex (`vertex`, 0 to 5) of the quad particle `i` is drawn on. The instances are the rain's
// three lattices and then the snow's; a particle that does not show this frame (its form not
// falling, under a roof, behind the eye, faded out) is drawn as nothing.
[shader("vertex")]
Falling vs_fall(uint vertex : SV_VertexID, uint i : SV_InstanceID)
{
    uint v = CORNERS[vertex % 6];
    Falling o;
    o.position = float4(0.0, 0.0, -2.0, 1.0);
    o.head = float2(0.0, 0.0);
    o.tail = float2(0.0, 0.0);
    o.shape = float4(0.0, 0.0, 0.0, 0.0);
    o.light = float4(0.0, 0.0, 0.0, 0.0);
    float4 state = row(push.weather, W_STATE);
    float4 rain_counts = row(push.weather, W_RAIN);
    float4 snow_counts = row(push.weather, W_SNOW);
    float snowfall = state.w;
    float amount = rain_counts.w;
    float at = float(i);
    bool flake = at >= rain_counts.z;
    float4 counts = flake ? snow_counts : rain_counts;
    float strength = flake ? amount * snowfall : amount * (1.0 - snowfall);
    if (at >= counts.z || strength <= 0.0)
    {
        return o;
    }
    uint k = at < counts.x ? 0u : (at < counts.y ? 1u : 2u);
    float4 tier = flake ? snow_tier(k) : rain_tier(k);
    float3 size = float3(2.0 * tier.x, 2.0 * tier.x, 2.0 * tier.y);
    float3 eye3 = row(push.frame, FRAME_EYE).xyz;
    float4 lattice = row(push.weather, W_LATTICE);
    float4 fall = row(push.weather, W_FALL);
    float4 motion = row(push.weather, W_MOTION);
    float3 eye = float3(eye3.xy + lattice.xy, eye3.z);
    uint salt = (flake ? 167u : 81u) + k * 4099u;
    float3 place = lattice_place(i, salt, size, eye, flake ? fall.y : fall.x, fall.zw);
    // How far out in its box it stands, 0 at the eye and 1 at a side: it fades before it leaves.
    float3 out_by = abs(place - eye) / (size * 0.5);
    float inside = 1.0 - smoothstep(0.7, 0.97, max(max(out_by.x, out_by.y), out_by.z));
    float3 p = float3(place.xy - lattice.xy, place.z);
    float3 vel = float3(motion.zw, -motion.x);
    float3 r = hash3(i, salt + 91u);
    if (flake)
    {
        // A flake sways as it falls, on a slow round of its own about where the fall takes it.
        float t = state.z;
        float w1 = turns_per_period(0.9 + r.x * 0.9);
        float w2 = turns_per_period(0.7 + r.y * 0.8);
        float sway = 0.06 + r.z * 0.1;
        float a1 = t * w1 + r.y * 6.2831853;
        float a2 = t * w2 + r.x * 6.2831853;
        p += float3(sin(a1), cos(a2), 0.0) * sway;
        vel = float3(motion.zw + float2(cos(a1) * w1, -sin(a2) * w2) * sway, -motion.y);
    }
    if (sheltered(push.weather, p, 0.1) > 0.5)
    {
        return o;
    }
    float dist = length(p - eye3);
    float pixel = snow_counts.w;
    float4 ch = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    // A flake is blurred over a quarter of the time a drop is: it falls slowly, and is drawn as
    // a flake rather than a streak.
    float shutter = flake ? SHUTTER * 0.25 : SHUTTER;
    float4 ct = transform(push.frame, FRAME_VIEW_PROJ, float4(p - vel * shutter, 1.0));
    if (ch.w < 0.1)
    {
        return o;
    }
    if (ct.w < 0.1)
    {
        ct = ch;
    }
    float2 head = px_of(ch.xy / ch.w);
    float2 tail = streak_tail(head, px_of(ct.xy / ct.w), pixel);
    float radius = flake ? tier.z * (0.6 + 1.2 * r.x) : tier.z;
    float2 drawn = drawn_size(radius, dist, pixel, flake);
    // Far off, the world's own fog takes most of it.
    float4 fog = row(push.frame, FRAME_FOG);
    float fogged = fog.z > 0.5 ? 1.0 - 0.75 * smoothstep(fog.x, fog.y, dist) : 1.0;
    float alpha = drawn.y * tier.w * strength * inside * fogged;
    if (alpha < 0.002)
    {
        return o;
    }
    // A quad over the particle's line, a pixel past its edge all round for the soft edge.
    float2 along = tail - head;
    float len = length(along);
    float2 dir = len > 0.001 ? along / max(len, 0.0001) : float2(0.0, 1.0);
    float2 across = float2(-dir.y, dir.x);
    float grow = drawn.x + 1.0;
    float side = (v & 1u) == 1u ? 1.0 : -1.0;
    float2 end = (v & 2u) == 2u ? tail + dir * grow : head - dir * grow;
    o.position = float4(ndc_of(end + across * grow * side), 0.5, 1.0);
    o.head = head;
    o.tail = tail;
    o.shape = float4(drawn.x, alpha, dist, flake ? 1.0 : 0.0);
    o.light = float4(scene_terms(fall_light(flake)), flake ? 1.0 : 0.2);
    return o;
}

[shader("fragment")]
float4 fs_fall(Falling in) : SV_Target0
{
    float2 px = in.position.xy;
    float2 ab = in.tail - in.head;
    float l2 = dot(ab, ab);
    float h = 0.0;
    if (l2 > 0.000001)
    {
        h = clamp(dot(px - in.head, ab) / l2, 0.0, 1.0);
    }
    float d = length(px - (in.head + ab * h));
    float radius = in.shape.x;
    float cover;
    if (in.shape.w > 0.5)
    {
        // A flake: soft and round, its light spread along its blur.
        cover = (1.0 - smoothstep(radius * 0.55, radius + 0.5, d)) / (1.0 + sqrt(l2) / (2.0 * radius + 1.0));
    }
    else
    {
        // A streak: brightest at its head, fading toward its tail.
        cover = (1.0 - smoothstep(radius - 0.5, radius + 0.5, d)) * (1.0 - 0.6 * h);
    }
    // Hidden by whatever is drawn nearer than it, softly over a few tens of centimeters, so its
    // edge never crawls along a surface.
    float sd = depth_at(int2(px));
    float scene = 1.0e5;
    if (sd < 1.0)
    {
        scene = length(world_at(px, sd) - row(push.frame, FRAME_EYE).xyz);
    }
    float a = cover * in.shape.y * smoothstep(0.0, 0.3, scene - in.shape.z);
    if (a < 0.002)
    {
        discard;
    }
    return float4(in.light.rgb * a, a * in.light.w);
}
