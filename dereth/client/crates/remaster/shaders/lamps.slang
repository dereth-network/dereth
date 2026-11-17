// The lamps: the scene's lights worked out per pixel, from lists of the lights that reach each
// piece of the view.
//
// - The shared passes' `cs_clusters` lists, for each piece of the view (a tile of the screen by a
//   slice of depth), the lamps whose reach meets it.
// - `fs_lamps` adds, at each pixel, the light of its piece's lamps: indoor lamps on rooms' walls
//   and on objects indoors, outdoor lamps on the ground and what the sun lights. Its alpha is 1
//   where the lamps take the place of retail's own light indoors; outdoors, below nought, it is how
//   much the pixel glows for standing right at a lamp, its flame or its glass.
//
// On a room's walls and outdoors a lamp lights a broad pool, full at the lamp and fading smoothly to
// its reach, its strength calibrated on the processor. On walls a share of its light falls off as
// retail's own does instead, softened near the light and ended smoothly, and the two together
// rise ever slower toward the lamp's color, with no edge anywhere. A moving light's lamp lights
// only the walls of the rooms its light reaches through the doorways.
// On objects indoors a static light's lamp falls off with the square of the distance, softened
// within half a metre, and ends smoothly at its reach, its strength calibrated; a moving light's
// lamp is retail's light, its intensity over the distance, faded out over the last tenth of its
// reach. A static light's turn on walls and every light's outdoors is wrapped, lighting to 120
// degrees as retail's burn does; a moving light's on walls, and every light's on objects, is the
// plain cosine.
import bindless;
import retail;

// The pass's constants, in buffer `rows`, as float4 rows:
//   0..3   the view, as four columns
//   4..7   the projection's inverse, as four columns
//   8..11  the view and projection's inverse, as four columns
//   12     the eye, xyz
//   13     the world's size in pixels, xy; a tile's, zw
//   14     the first slice's depth, the last's, how many slices, and the softening distance
//   15     x 1 when the indoors is lit, y 1 when the outdoors is
//   16     (as words) the tiles across and down, the slices, and the lamps a piece lists at most
//   17 + 2l, 18 + 2l   (as words) the rooms lamp `l` lights: the keys their walls carry in the
//          material target, two to a word, 0xFFFF where there is none; every word all ones where it
//          lights every room
static const uint ROW_VIEW = 0;
static const uint ROW_INV_PROJ = 4;
static const uint ROW_INV_VIEW_PROJ = 8;
static const uint ROW_EYE = 12;
static const uint ROW_EXTENT = 13;
static const uint ROW_SLICES = 14;
static const uint ROW_LIT = 15;
static const uint ROW_GRID = 16;
static const uint ROW_ROOMS = 17;

// A lamp is three float4 rows in buffer `lamps`: where it is and its reach; its color and kind
// (1 a static light indoors, 2 a light outdoors, 3 a moving light indoors); its strength on
// walls, on objects, and (indoors) the strength of its share on walls shaped as retail's own
// light or (outdoors) its strength, and its reach on objects. Outdoors the first two are how near
// a surface takes its glow, and how much.
static const float KIND_STATIC = 1.0;
static const float KIND_OUTDOOR = 2.0;
static const float KIND_MOVING = 3.0;

// How far retail burns a static light into a room's walls, and how far it lights with a moving
// one, against its falloff (a light's reach on objects).
static const float BURN_REACH = 1.3;
static const float MOVING_REACH = 1.5;

// Within how many metres a static light's and a moving light's own light on walls stops growing
// nearer (a static light's, and how near a wall turns from any light less).
static const float WALL_SOFTEN = 1.0;
static const float MOVING_SOFTEN = 3.0;

// Over how many metres behind a wall a static light's glow through it fades.
static const float BEHIND_FADE = 1.5;

struct ShadePush
{
    uint rows;
    uint lamps;
    uint pieces;
    // The G-buffer's normal and material, the depth, and the depth the outdoor surfaces are
    // placed by.
    uint normal;
    uint material;
    uint depth;
    uint outdoor_depth;
};

// How a surface at cosine `mu` to a lamp takes its light: wrapped, lit to 120 degrees and at two
// thirds facing it squarely, or the plain cosine.
float turn(float mu, bool wrapped)
{
    return wrapped ? max((mu + 0.5) * (2.0 / 3.0), 0.0) : max(mu, 0.0);
}

// How a room's wall takes a lamp's light at cosine `mu` (worked out as though the lamp were
// `WALL_SOFTEN` further off, so a wall turns from a light less the nearer it stands), the lamp
// `behind` metres behind the wall's plane (nought in front of it): a static light's wrapped as its
// burn is in front, and from behind a third of it, fading with the depth, the same all over a flat
// wall, so a light set into a wall glows through it evenly, as retail spreading its burn from the
// wall's corners does, with no dark hole ringed by light; a moving light's the plain cosine.
float wall_turn(float mu, float behind, bool wrapped)
{
    if (!wrapped)
    {
        return max(mu, 0.0);
    }
    return mu >= 0.0 ? (mu + 0.5) * (2.0 / 3.0) : exp(-behind / BEHIND_FADE) / 3.0;
}

// A lamp's light at distance `d` within reach `reach`, before its strength and the surface's
// turn: a static light's the inverse square, ended smoothly; a moving light's the inverse of the
// distance, faded out over the last tenth.
float falloff(float d, float reach, float soften, bool moving)
{
    if (d >= reach)
    {
        return 0.0;
    }
    if (moving)
    {
        return saturate((reach - d) / (0.1 * reach)) / max(d, 1e-3);
    }
    float x = d / reach;
    float w = saturate(1.0 - x * x * x * x);
    return w * w / (d * d + soften * soften);
}

// How a lamp's light ends at its reach: 1 near it, 0 at `d == reach`, smoothly between.
float window(float d, float reach)
{
    if (d >= reach)
    {
        return 0.0;
    }
    float x = d / reach;
    float w = saturate(1.0 - x * x * x * x);
    return w * w;
}

// A lamp's share of its light on a room's wall shaped as retail's own light falls off, at
// distance `d` before its strength and the surface's turn, for a light of falloff `fall`: a
// static light's the inverse square softened within `WALL_SOFTEN`, a moving light's the inverse of
// the distance softened within `MOVING_SOFTEN`, each ended smoothly at retail's reach (a moving
// light's over less of it, as retail's light ends there at once).
float retail_on_walls(float d, float fall, bool moving)
{
    if (!moving)
    {
        return window(d, fall * BURN_REACH) / (d * d + WALL_SOFTEN * WALL_SOFTEN);
    }
    float x = min(d / (fall * MOVING_REACH), 1.0);
    float x2 = x * x;
    float w = 1.0 - x2 * x2 * x2 * x2;
    return w * w / sqrt(d * d + MOVING_SOFTEN * MOVING_SOFTEN);
}

// An outdoor lamp's pool at distance `d` within reach `reach`, before its strength and the
// surface's turn: full at the lamp, fading smoothly to nothing at the reach.
float pool(float d, float reach)
{
    if (d >= reach)
    {
        return 0.0;
    }
    float x = d / reach;
    float w = 1.0 - x * x;
    return w * w;
}

// A pixel of the G-buffer as the lamps see it.
struct Surface
{
    uint cls;
    // Whether the indoor lamps light it, or the outdoor ones.
    bool indoor;
    bool outdoor;
    float3 world;
    float3 n;
    // On a room's walls, the key the room carries.
    uint room;
    // Its view depth, and where it is on the screen.
    float z;
    float2 position;
};

Surface surface_at(ShadePush push, int2 p, float2 position)
{
    Surface s;
    uint4 mat = uint4(textures[NonUniformResourceIndex(push.material)].Load(int3(p, 0)) * 255.0 + 0.5);
    s.cls = mat.x;
    s.room = mat.z | (mat.w << 8);
    s.position = position;
    float4 lit = row(push.rows, ROW_LIT);
    s.indoor = (s.cls == CLASS_CELL || s.cls == CLASS_INDOOR) && lit.x > 0.5;
    s.outdoor = (s.cls == CLASS_GROUND || s.cls == CLASS_SUN) && lit.y > 0.5;
    s.n = float3(0.0, 0.0, 1.0);
    s.world = float3(0.0, 0.0, 0.0);
    s.z = 0.0;
    if (!s.indoor && !s.outdoor)
    {
        return s;
    }
    s.n = oct_decode(textures[NonUniformResourceIndex(push.normal)].Load(int3(p, 0)).xy);
    float4 e = row(push.rows, ROW_EXTENT);
    float2 ndc = float2(position.x / e.x * 2.0 - 1.0, 1.0 - position.y / e.y * 2.0);
    uint depth = s.outdoor ? push.outdoor_depth : push.depth;
    float d = textures[NonUniformResourceIndex(depth)].Load(int3(p, 0)).x;
    float4 h = transform(push.rows, ROW_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    s.world = h.xyz / h.w;
    s.z = transform(push.rows, ROW_VIEW, float4(s.world, 1.0)).z;
    return s;
}

// Whether lamp `l` lights the room of key `room`: one of those it lists, or any where it lists
// none.
bool lights_room(uint rows, uint l, uint room)
{
    uint4 a = row_u(rows, ROW_ROOMS + l * 2);
    if (all(a == 0xFFFFFFFF))
    {
        return true;
    }
    uint4 b = row_u(rows, ROW_ROOMS + l * 2 + 1);
    uint words[8] = { a.x, a.y, a.z, a.w, b.x, b.y, b.z, b.w };
    for (uint k = 0; k < 8; k++)
    {
        if ((words[k] & 0xFFFF) == room || (words[k] >> 16) == room)
        {
            return true;
        }
    }
    return false;
}

// The light of a surface's lamps, and how much it glows.
float3 lamps_on(ShadePush push, Surface s, out float glow)
{
    float3 out = float3(0.0, 0.0, 0.0);
    glow = 0.0;
    float4 sl = row(push.rows, ROW_SLICES);
    if (s.z > sl.y)
    {
        return out;
    }
    float4 e = row(push.rows, ROW_EXTENT);
    uint4 grid = row_u(push.rows, ROW_GRID);
    float slice = floor(log(max(s.z, sl.x) / sl.x) / log(sl.y / sl.x) * sl.z);
    uint k = min(uint(max(slice, 0.0)), grid.z - 1);
    uint i = min(uint(s.position.x / e.z), grid.x - 1);
    uint j = min(uint(s.position.y / e.w), grid.y - 1);
    uint piece = (k * grid.y + j) * grid.x + i;
    uint base = piece * (grid.w + 1) * 4;
    ByteAddressBuffer list = buffers[NonUniformResourceIndex(push.pieces)];
    uint count = min(list.Load(base), grid.w);
    for (uint m = 0; m < count; m++)
    {
        uint l = list.Load(base + 4 + m * 4);
        float4 lp = row(push.lamps, l * 3);
        float4 lc = row(push.lamps, l * 3 + 1);
        float4 power = row(push.lamps, l * 3 + 2);
        float strength = 0.0;
        float reach = lp.w;
        bool wrapped = false;
        bool pooled = false;
        if (s.indoor && (lc.w == KIND_STATIC || lc.w == KIND_MOVING))
        {
            if (s.cls == CLASS_CELL)
            {
                // A room's walls take only the lamps whose light reaches them through the
                // doorways, none through a wall.
                if (!lights_room(push.rows, l, s.room))
                {
                    continue;
                }
                strength = power.x;
                wrapped = lc.w == KIND_STATIC;
                pooled = true;
            }
            else
            {
                strength = power.y;
                reach = power.w;
            }
        }
        else if (s.outdoor && lc.w == KIND_OUTDOOR)
        {
            strength = power.z;
            wrapped = true;
            pooled = true;
        }
        if (strength <= 0.0)
        {
            continue;
        }
        float3 to = lp.xyz - s.world;
        float dist = length(to);
        if (s.outdoor && dist < power.x)
        {
            float g = 1.0 - dist / power.x;
            glow = max(glow, power.y * g * g * (3.0 - 2.0 * g));
        }
        float mu = dot(s.n, to / max(dist, 1e-6));
        if (pooled)
        {
            if (s.indoor)
            {
                float facing = dot(s.n, to);
                float t = wall_turn(facing / sqrt(dist * dist + WALL_SOFTEN * WALL_SOFTEN),
                                    max(-facing, 0.0), wrapped);
                float light = t * (strength * pool(dist, reach)
                                   + power.z * retail_on_walls(dist, power.w, lc.w == KIND_MOVING));
                out += lc.rgb * lamp_shoulder(light);
            }
            else
            {
                out += lc.rgb * min(strength * turn(mu, wrapped) * pool(dist, reach), 1.0);
            }
        }
        else
        {
            out += lc.rgb * strength * turn(mu, wrapped)
                   * falloff(dist, reach, sl.w, lc.w == KIND_MOVING);
        }
    }
    return out;
}

[shader("fragment")]
float4 fs_lamps(float4 position : SV_Position, uniform ShadePush push) : SV_Target0
{
    Surface s = surface_at(push, int2(position.xy), position.xy);
    if (!s.indoor && !s.outdoor)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float glow;
    float3 light = lamps_on(push, s, glow);
    return float4(light, s.indoor ? 1.0 : -glow);
}
