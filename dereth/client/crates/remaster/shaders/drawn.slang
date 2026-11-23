// The ground drawn as a grid of patches over retail's surface: finer near the eye and coarser
// away from it, each level twice the last's spacing, a vertex toward the edge of its level's ring
// sliding onto the next level's grid so the levels meet without cracks. Every quad is cut along
// the diagonal of the retail cell it lies in, so at the finest spacing retail's own planes are
// drawn exactly where nothing is laid over them.
//
// Laid over retail's height, when asked: the scene's own offset for the blocks round the viewer
// (so what stands on the ground and the ground agree), a fine relief and terraces leveled under
// each body, and far off the smooth surface through retail's vertices with an eroded relief, out
// to the world's edge. The water's bed is drawn where a body standing in the water stands, as
// deep under retail's ground as physics sinks it, and the land beside it never lies below the
// water's level at its edge.
//
// Each pixel takes the drawn surface's normal (finite differences of the drawn height), leaned
// to the smooth surface's where the ground is pinned to retail's; its picture is its cell's,
// composite or splat, where the block is held, and far off its corners' terrain's colors. The
// material target carries, on the ground, the water's share of the blend (third byte) and how far
// under the water's surface the bed is drawn, in 1 cm steps (fourth byte; where the ground is not
// water and the weather is drawn, how much of it is puddle).
import bindless;
import ground;
import retail;
import weather;
import world;

struct Push
{
    uint frame;
    // The drawn ground's constants (see the rows below).
    uint drawn;
    // The world's landscape vertices: their heights, their terrain words (two to a word), and
    // where the water lies about each (two floats each).
    uint heights;
    uint words;
    uint water;
    // The near offset: each slot a block, 192 by 192 samples of three floats, the offset, how
    // much of it the pins leave, and the part of it the rock bands make.
    uint near;
    // The patches: two rows each.
    uint patches;
    // The held blocks' ground cells, the table of the blocks round the frame's origin (each its
    // first cell and its cells a side), and the splats.
    uint cells;
    uint blocks;
    uint splats;
    uint wrap;
    uint clamp;
    // The first patch this draw covers.
    uint first;
    // The weather's constants (~0 for none): the wet, and the puddles' share.
    uint weather;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The constants' rows.
static const uint D_EYE = 0;      // xyz the eye, the frame's space
static const uint D_GLOBAL = 1;   // xy the frame's origin, world meters; z near slots a side; w drawn to, meters
static const uint D_K0 = 2;       // x the fine relief's height, y its size; z the level round's radius, w its fade
static const uint D_K1 = 3;       // x a terrace's height, y its cap, z the slope it starts at; w the rounding kept far
static const uint D_K2 = 4;       // x, y where the near gives way to the far; z 1 for the far ground; w its relief's height
static const uint D_K3 = 5;       // x the far relief's size; y, z the distances the rock bands fade in between; w how many bodies
static const uint D_WATER = 6;    // x 1 while the water is drawn over its bed, y how far in from its edge the bed slopes down to where bodies stand
static const uint D_ORIGIN = 7;   // (uint) xy the frame's origin block, z the blocks table's reach, w its side
static const uint D_BODIES = 8;   // 32 rows
static const uint D_SLOTS = 40;   // 64 rows of four blocks, `x * 256 + y`, or -1
static const uint D_COLORS = 104; // 32 rows: each terrain type's color far off
static const uint D_SLOPE = 136;  // the slopes, degrees, scree is laid from and over, then rock's
static const uint D_SLOPE2 = 137; // x how much of them is laid, y how much the slope is the smooth surface's, z how much shows through snow
static const uint D_COVER = 138;  // the forest cover: x how strongly it tints the far ground (nought for none); y, z where it comes in; w how much its crowns shade the ground past the sun's shadows
static const uint D_CANOPY = 139; // x how many meters its whole canopy raises the far ground (nought for none); y, z the distances it rises in over
static const uint D_COVER_PICTURES = 140; // (uint) x the cover's picture, y the canopy's
static const uint D_BANK = 141;    // x 1 while a pack's banks are laid by the water, y how much grass grows in over a muddy bank, z how much darker the wet ground along the water is
static const uint D_GRASS = 142;  // x 1 while the grass field is drawn, y how far its near clumps reach, z how far its far ones do
static const uint D_GRASS_TYPES = 143; // 32 rows: each terrain type's x share of grass, y height, z dryness, w how patchy
static const uint D_FLOWERS = 175; // x 1 while the flowers are drawn, y how far they reach
static const uint D_PALETTE = 176; // 8 rows: the flowers' colors
static const uint D_VARIANT_COLORS = 184; // 480 rows: each terrain type's variants' colors far off, 32 a variant from the first

// The color far off of the ground a terrain word names: its type's, or its variant's (bits 7-10).
float4 ground_color(uint word)
{
    uint t = (word >> 2) & 0x1F;
    uint v = (word >> 7) & 0xF;
    return v == 0 ? row(push.drawn, D_COLORS + t) : row(push.drawn, D_VARIANT_COLORS + (v - 1) * 32 + t);
}
static const uint BODIES = 32;

static const uint CELLS = 16;
// A land cell's side, meters: what the forest cover's picture (a texel a land cell corner,
// COVER_SIDE a side) and a pack's patterns are laid by.
static const float CELL = 24.0;
static const float BLOCK = 192.0;
static const int COVER_SIDE = 2041;
static const float WALKABLE_Z = 0.6641741;
// How far the ground is drawn below where it is, so that what stands on it wins the depth test.
static const float BELOW = 0.01;
// The step the bed's depth under the water is written in, meters: the material target's fourth
// byte on the ground.
static const float BED_STEP = 0.01;
// How far under the water's level the bed lies before the ground there is drawn as water.
static const float SURFACE_FROM = 0.02;

// --- the world's vertices ---------------------------------------------------------------------

// The meters between the world's neighboring vertices: a land cell's side on the default grid.
float vertex_spacing()
{
    return world_spacing(push.heights);
}

float vertex_height(int2 g)
{
    return world_height(push.heights, g);
}

uint vertex_word(int2 g)
{
    return world_word(push.heights, push.words, g);
}

// Whether the square whose south-west vertex is `c` has its shared edge run south-west to
// north-east.
bool sw_to_ne(int2 c)
{
    return world_cut(push.heights, push.words, c);
}

// Retail's height at world `p`, and its triangle's rise east and north.
float3 retail_at(float2 p)
{
    float cell = vertex_spacing();
    float2 q = p / cell;
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float h00 = vertex_height(c);
    float h10 = vertex_height(c + int2(1, 0));
    float h01 = vertex_height(c + int2(0, 1));
    float h11 = vertex_height(c + int2(1, 1));
    if (sw_to_ne(c))
    {
        if (f.x >= f.y)
        {
            return float3(h00 + f.x * (h10 - h00) + f.y * (h11 - h10), (h10 - h00) / cell, (h11 - h10) / cell);
        }
        return float3(h00 + f.y * (h01 - h00) + f.x * (h11 - h01), (h11 - h01) / cell, (h01 - h00) / cell);
    }
    if (f.x + f.y <= 1.0)
    {
        return float3(h00 + f.x * (h10 - h00) + f.y * (h01 - h00), (h10 - h00) / cell, (h01 - h00) / cell);
    }
    return float3(h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11), (h11 - h01) / cell, (h11 - h10) / cell);
}

// The rise east and north the ground cover goes by at world `p`, `triangle` being retail's
// triangle's there: on retail's grid, the triangle's; on a finer grid, whose triangles differ far
// more in slope from one to the next, each corner's rise (the central difference over its
// neighbors) blended across the cell, so the cover thins with the slope without showing single
// triangles. The terrain's `cover_gradient` is the same rule.
float2 cover_gradient(float2 p, float2 triangle)
{
    float cell = vertex_spacing();
    if (cell >= CELL)
    {
        return triangle;
    }
    float2 q = p / cell;
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float2 g[4];
    int2 at[4] = { c, c + int2(1, 0), c + int2(0, 1), c + int2(1, 1) };
    for (uint k = 0; k < 4; k++)
    {
        int2 v = at[k];
        g[k] = float2(vertex_height(v + int2(1, 0)) - vertex_height(v - int2(1, 0)),
                      vertex_height(v + int2(0, 1)) - vertex_height(v - int2(0, 1))) / (2.0 * cell);
    }
    return lerp(lerp(g[0], g[1], f.x), lerp(g[2], g[3], f.x), f.y);
}

float4 catmull(float t)
{
    float t2 = t * t;
    float t3 = t2 * t;
    return 0.5 * float4(-t3 + 2.0 * t2 - t, 3.0 * t3 - 5.0 * t2 + 2.0, -3.0 * t3 + 4.0 * t2 + t, t3 - t2);
}

// The smooth surface through retail's vertices at world `p`.
float smooth_height(float2 p)
{
    float2 q = p / vertex_spacing();
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float4 wx = catmull(f.x);
    float4 wy = catmull(f.y);
    float h = 0.0;
    for (int i = 0; i < 4; i++)
    {
        float column = 0.0;
        for (int j = 0; j < 4; j++)
        {
            column += wy[j] * vertex_height(c + int2(i - 1, j - 1));
        }
        h += wx[i] * column;
    }
    return h;
}

// The rises over a cell between which a vertex's slope is eased toward its gentler side's: the
// scene's own.
static const float EASE_FROM = 0.8391 * CELL;
static const float EASE_TO = 1.1325 * CELL;

// The slope at the middle of three heights a cell apart, meters a cell: the smooth surface's,
// eased toward one that keeps to the gentler step where the steeper is steep. The scene's own
// arithmetic.
float eased_slope(float a, float b, float c)
{
    float d0 = b - a;
    float d1 = c - b;
    float mean = 0.5 * (d0 + d1);
    float kept = d0 * d1 <= 0.0 ? 0.0 : 2.0 * d0 * d1 / (d0 + d1);
    return mean + (kept - mean) * smoothstep(EASE_FROM, EASE_TO, max(abs(d0), abs(d1)));
}

// The cubic from `h0` to `h1` over a cell, leaving with slope `m0` and arriving with `m1`.
float hermite(float h0, float h1, float m0, float m1, float t)
{
    float t2 = t * t;
    float t3 = t2 * t;
    return h0 * (2.0 * t3 - 3.0 * t2 + 1.0) + h1 * (3.0 * t2 - 2.0 * t3) + m0 * (t3 - 2.0 * t2 + t) + m1 * (t3 - t2);
}

// The smooth surface eased where gentle ground meets a steep face, so it meets level ground
// level, at world `p`: the scene's own arithmetic.
float eased_height(float2 p)
{
    float2 q = p / vertex_spacing();
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float rows[4];
    for (int j = 0; j < 4; j++)
    {
        float h0 = vertex_height(c + int2(-1, j - 1));
        float h1 = vertex_height(c + int2(0, j - 1));
        float h2 = vertex_height(c + int2(1, j - 1));
        float h3 = vertex_height(c + int2(2, j - 1));
        rows[j] = hermite(h1, h2, eased_slope(h0, h1, h2), eased_slope(h1, h2, h3), f.x);
    }
    return hermite(rows[1], rows[2], eased_slope(rows[0], rows[1], rows[2]), eased_slope(rows[1], rows[2], rows[3]), f.y);
}

// A lattice point's hash, 0..1: the scene's own arithmetic.
float lattice_hash(int2 p)
{
    uint h0 = (asuint(p.x) * 0x8DA6B343u) ^ (asuint(p.y) * 0xD8163841u);
    uint h1 = (h0 ^ (h0 >> 13)) * 0x2C1B3C6Du;
    uint h2 = h1 ^ (h1 >> 15);
    return float(h2 & 0xFFFF) / 65535.0;
}

// Smooth noise over a lattice `cell` meters wide, 0..1: the scene's own arithmetic.
float value_noise(float2 p, float cell)
{
    float2 q = p / cell;
    int2 i = int2(floor(q));
    float2 f = q - floor(q);
    float2 s = f * f * (3.0 - 2.0 * f);
    float a = lattice_hash(i);
    float b = lattice_hash(i + int2(1, 0));
    float c = lattice_hash(i + int2(0, 1));
    float d = lattice_hash(i + int2(1, 1));
    float south = a + (b - a) * s.x;
    float north = c + (d - c) * s.x;
    return south + (north - south) * s.y;
}

// One octave of the far relief: ridged noise `cell` meters wide, moved `offset` meters.
float ridge_octave(float2 p, float offset, float cell)
{
    float n = value_noise(float2(p.x + offset, p.y - offset), cell);
    float ridge = 1.0 - abs(2.0 * n - 1.0);
    return ridge * ridge - 0.45;
}

// How much of a detail `cell` meters across shows in a pixel that covers `foot` meters of ground.
float resolved(float cell, float foot)
{
    return 1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot);
}

// The far ground's eroded relief at world `p` on ground rising `slope`, as a pixel covering
// `foot` meters sees it: three octaves of ridged noise, each half the last's size and weight.
float far_relief(float2 p, float scale, float amplitude, float slope, float foot)
{
    if (amplitude <= 0.0)
    {
        return 0.0;
    }
    float cell = max(scale, 1.0);
    float sum = ridge_octave(p, 0.0, cell) * resolved(cell, foot)
        + 0.5 * ridge_octave(p, 37.0, cell * 0.5) * resolved(cell * 0.5, foot)
        + 0.25 * ridge_octave(p, 74.0, cell * 0.25) * resolved(cell * 0.25, foot);
    return amplitude * sum / 1.75 * (0.3 + min(slope, 1.2));
}

// --- the water's shore ------------------------------------------------------------------------

static const float SHORE_CELL = 24.0;
static const float SHORE_NEAR = 40.0;
static const float SHORE_FALL = 0.25;
// How far from the water a bank is taken to reach, meters; and how far inside the water the bed's
// own ground starts to take over from the land beside it, and where it has.
static const float BANK_REACH = 40.0;
static const float BED_FROM = 6.0;
static const float BED_IN = 20.0;

float2 shore_texel(int2 g)
{
    return world_water(push.heights, push.water, g);
}

float shore_noise(float2 p, float cell)
{
    return value_noise(p, cell);
}

// How much the water's edge is moved at world `p`, out into the land where positive.
float shore_wander(float2 p, float foot)
{
    return (shore_noise(p, 9.0) - 0.5) * 6.0 * resolved(9.0, foot)
        + (shore_noise(p + float2(31.7, 12.9), 3.5) - 0.5) * 2.0 * resolved(3.5, foot);
}

// How far inside the water world `p` lies, meters, negative on land.
float shore_distance(float2 p, float foot)
{
    float2 q = p / vertex_spacing();
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float a = shore_texel(c).x;
    float b = shore_texel(c + int2(1, 0)).x;
    float d = shore_texel(c + int2(0, 1)).x;
    float e = shore_texel(c + int2(1, 1)).x;
    float nearest = max(max(a, b), max(d, e));
    if (nearest < -SHORE_NEAR)
    {
        return lerp(lerp(a, b, f.x), lerp(d, e, f.x), f.y);
    }
    float4 wx = catmull(f.x);
    float4 wy = catmull(f.y);
    float s = 0.0;
    for (int i = 0; i < 4; i++)
    {
        float column = 0.0;
        for (int j = 0; j < 4; j++)
        {
            column += wy[j] * shore_texel(c + int2(i - 1, j - 1)).x;
        }
        s += wx[i] * column;
    }
    return s + shore_wander(p, foot);
}

// How far physics stands a body under retail's ground at a point standing on it, meters, by its
// square's water corners: in a square of water corners alone, its deepest; in one with some, by
// whether the corner nearest is water; in one with none, nought.
static const float SINK_DEEP = 0.9;
static const float SINK_WET = 0.45;
static const float SINK_DRY = 0.1;
// How far either side of where two of those depths meet the bed eases from one to the other.
static const float SINK_EASE = 0.75;

// Whether the world's vertices lie on a grid finer than a land cell.
bool finer_grid()
{
    return world_steps(push.heights) > 1;
}

// How far physics stands a body under retail's ground in quarter `q` of a square (two a square
// each way), from `wet`, whether each of the 3 by 3 vertices from vertex `from` is water: by the
// square's own corners. On a grid finer than a land cell physics stands a body in a square all
// of water as at a shore where the vertex nearest it is wading water.
float quarter_sink(int2 q, int2 from, bool wet[3][3])
{
    int2 square = int2(floor(float2(q) * 0.5));
    int2 c = square - from;
    int2 near = c + (q - square * 2);
    bool a = wet[c.x][c.y];
    bool b = wet[c.x + 1][c.y];
    bool d = wet[c.x][c.y + 1];
    bool e = wet[c.x + 1][c.y + 1];
    if (a && b && d && e)
    {
        if (finer_grid())
        {
            uint t = (vertex_word(from + near) >> 2) & 0x1F;
            return t == 18 || t == 19 ? SINK_WET : SINK_DEEP;
        }
        return SINK_DEEP;
    }
    if (!(a || b || d || e))
    {
        return 0.0;
    }
    return wet[near.x][near.y] ? SINK_WET : SINK_DRY;
}

// How far physics stands a body at world `p` under retail's ground, its quarter's depth eased
// over `SINK_EASE` either side of where quarters meet.
float shore_sink(float2 p)
{
    float half_cell = 0.5 * vertex_spacing();
    float2 b = round(p / half_cell);
    float2 t = float2(smoothstep(-SINK_EASE, SINK_EASE, p.x - b.x * half_cell),
                      smoothstep(-SINK_EASE, SINK_EASE, p.y - b.y * half_cell));
    int2 q = int2(b);
    int2 from = int2(floor(float2(q - 1) * 0.5));
    bool wet[3][3];
    for (int i = 0; i < 3; i++)
    {
        for (int j = 0; j < 3; j++)
        {
            wet[i][j] = shore_texel(from + int2(i, j)).x > 0.0;
        }
    }
    return lerp(lerp(quarter_sink(q - int2(1, 1), from, wet), quarter_sink(q - int2(0, 1), from, wet), t.x),
                lerp(quarter_sink(q - int2(1, 0), from, wet), quarter_sink(q, from, wet), t.x), t.y);
}

// How far under retail's ground the bed is drawn at world `p`, `inside` meters inside the water:
// where a body standing there stands, sloping up to nought over `slope` meters to
// the water's edge.
float bed_depth(float2 p, float inside, float slope)
{
    if (inside <= 0.0)
    {
        return 0.0;
    }
    return shore_sink(p) * smoothstep(0.0, max(slope, 1e-3), inside);
}

float shore_floor(float level, float inside)
{
    return level + min(inside, 0.0) * SHORE_FALL;
}

// The water's level at world `p`, across retail's triangles.
float level_at(float2 p)
{
    float2 q = p / vertex_spacing();
    int2 c = int2(floor(q));
    float2 f = clamp(q - float2(c), 0.0, 1.0);
    float h00 = vertex_height(c) - shore_texel(c).y;
    float h10 = vertex_height(c + int2(1, 0)) - shore_texel(c + int2(1, 0)).y;
    float h01 = vertex_height(c + int2(0, 1)) - shore_texel(c + int2(0, 1)).y;
    float h11 = vertex_height(c + int2(1, 1)) - shore_texel(c + int2(1, 1)).y;
    if (sw_to_ne(c))
    {
        if (f.x >= f.y)
        {
            return h00 + f.x * (h10 - h00) + f.y * (h11 - h10);
        }
        return h00 + f.y * (h01 - h00) + f.x * (h11 - h01);
    }
    if (f.x + f.y <= 1.0)
    {
        return h00 + f.x * (h10 - h00) + f.y * (h01 - h00);
    }
    return h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11);
}

// How far over the water's level ground on a grid finer than a land cell may stand and keep the
// water its pictures lay, meters; and over how much more that water fades out.
static const float ABOVE_FROM = 0.02;
static const float ABOVE_FADE = 0.1;

// How much of the water its pictures lay at world `p` is drawn over ground standing at `ground`
// there. On retail's grid, all of it. On a finer grid, where a square's water corner lies at the
// water's level and its land corner up the bank, none on the ground standing over the water's
// level, so a bank's slope is never drawn as water.
float water_kept(float2 p, float ground)
{
    if (!finer_grid())
    {
        return 1.0;
    }
    return 1.0 - smoothstep(ABOVE_FROM, ABOVE_FROM + ABOVE_FADE, ground - level_at(p));
}

// --- the offset -------------------------------------------------------------------------------

// The near offset's sample at world meter `g`: the offset, the pins' weight, the rock bands'
// part of the offset, and whether a slot holds the block; nought and whole where none does.
float4 near_sample(int2 g)
{
    float4 gl = row(push.drawn, D_GLOBAL);
    int n = int(gl.z);
    if (n <= 0)
    {
        return float4(0.0, 1.0, 0.0, 0.0);
    }
    int2 b = int2(floor(float2(g) / BLOCK));
    int2 slot = ((b % n) + n) % n;
    int k = slot.y * n + slot.x;
    int held = asint(row_u(push.drawn, D_SLOTS + uint(k) / 4)[k % 4]);
    if (held != b.x * 256 + b.y)
    {
        return float4(0.0, 1.0, 0.0, 0.0);
    }
    int2 local = g - b * 192;
    uint at = (uint(k) * 192 * 192 + uint(local.y) * 192 + uint(local.x)) * 12;
    float3 v = asfloat(buffers[NonUniformResourceIndex(push.near)].Load3(at));
    return float4(v, 1.0);
}

// The near offset, the pins' weight and the rock bands' part of the offset at world `p`, between
// the samples; w whether any sample round it is held.
float4 near_at(float2 p)
{
    int2 i = int2(floor(p));
    float2 f = p - floor(p);
    float4 a = near_sample(i);
    float4 b = near_sample(i + int2(1, 0));
    float4 c = near_sample(i + int2(0, 1));
    float4 d = near_sample(i + int2(1, 1));
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// How much of the rock bands is drawn `dist` meters from the eye: the scene's own arithmetic.
float bands_kept(float dist)
{
    float4 k3 = row(push.drawn, D_K3);
    return smoothstep(k3.y, max(k3.z, k3.y + 1e-3), dist);
}

// How far `dist` meters from the eye has given way to the far ground: 0 near, 1 far.
float morph_far(float dist)
{
    float4 k2 = row(push.drawn, D_K2);
    if (k2.z < 0.5)
    {
        return 0.0;
    }
    return smoothstep(k2.x, max(k2.y, k2.x + 1e-3), dist);
}

// How much of the fine relief and the terraces is left at world `p`: none under a body.
float unleveled(float2 p)
{
    float4 k0 = row(push.drawn, D_K0);
    if (k0.z <= 0.0)
    {
        return 1.0;
    }
    float2 global = row(push.drawn, D_GLOBAL).xy;
    float keep = 1.0;
    uint count = min(uint(row(push.drawn, D_K3).w), BODIES);
    for (uint k = 0; k < count; k++)
    {
        float2 b = row(push.drawn, D_BODIES + k).xy + global;
        float d = length(p - b);
        keep = min(keep, smoothstep(k0.z, k0.z + max(k0.w, 1e-3), d));
    }
    return keep;
}

// The fine relief and the terraces at world `p` on retail ground `r` (height, rise east and
// north), `pinned` how much the pins leave, `far` how far it has given way to the far ground.
float fine(float2 p, float3 r, float pinned, float far, float foot)
{
    float room = pinned * (1.0 - far);
    if (room <= 0.0)
    {
        return 0.0;
    }
    float4 k0 = row(push.drawn, D_K0);
    float4 k1 = row(push.drawn, D_K1);
    float h = 0.0;
    float micro = k0.x;
    if (micro > 0.0)
    {
        float s = max(k0.y, 0.05);
        float n = (value_noise(p, s) - 0.5) * 0.65 * resolved(s, foot)
            + (value_noise(p + float2(11.3, 7.1), s * 0.45) - 0.5) * 0.35 * resolved(s * 0.45, foot);
        h += n * 2.0 * micro;
    }
    float band = k1.x;
    if (band > 0.0)
    {
        float slope = length(r.yz);
        float nz = rsqrt(1.0 + slope * slope);
        float start = tan(radians(k1.z));
        float steep = smoothstep(start, tan(radians(k1.z + 5.0)), slope) * (nz >= WALKABLE_Z ? 1.0 : 0.0);
        if (steep > 0.0)
        {
            float q = r.x / band;
            float f = q - floor(q);
            float riser = 0.35;
            float off = (smoothstep(1.0 - riser, 1.0, f) - f + (1.0 - riser) * 0.5) * band;
            h += clamp(off, -k1.y, k1.y) * steep * resolved(band, foot);
        }
    }
    return h * room * unleveled(p);
}

// The forest cover's canopy at world `p`: how high it raises the ground, meters, between the
// vertices, as far as it has risen in `dist` meters from the eye.
float canopy(float2 p, float dist)
{
    float4 k = row(push.drawn, D_CANOPY);
    if (k.x <= 0.0)
    {
        return 0.0;
    }
    float rise = smoothstep(k.y, max(k.z, k.y + 1.0), dist);
    if (rise <= 0.0)
    {
        return 0.0;
    }
    uint picture = row_u(push.drawn, D_COVER_PICTURES).y;
    float2 q = p / CELL;
    int2 c = int2(floor(q));
    float2 f = q - floor(q);
    float h[4];
    int2 offs[4] = { int2(0, 0), int2(1, 0), int2(0, 1), int2(1, 1) };
    for (int i = 0; i < 4; i++)
    {
        int2 at = clamp(c + offs[i], int2(0, 0), int2(COVER_SIDE - 1, COVER_SIDE - 1));
        h[i] = textures[NonUniformResourceIndex(picture)].Load(int3(at, 0)).r;
    }
    return lerp(lerp(h[0], h[1], f.x), lerp(h[2], h[3], f.x), f.y) * k.x * rise;
}

// The drawn ground's height at world `p`, `dist` meters from the eye: x the height, y how much
// of the offset is laid there (the pins' weight where a block's offset is held, nought where
// none is), z retail's height, w how far it has given way to the far ground. `fine_share` is how
// much of the fine relief is laid over it; `foot` the ground a pixel covers there.
float4 drawn_at(float2 p, float dist, float fine_share, float foot)
{
    float3 r = retail_at(p);
    float far = morph_far(dist);
    float4 near = near_at(p);
    float offset = near.x - near.z * (1.0 - bands_kept(dist));
    float4 water = row(push.drawn, D_WATER);
    float4 k1 = row(push.drawn, D_K1);
    float4 k2 = row(push.drawn, D_K2);
    float4 k3 = row(push.drawn, D_K3);
    float inside = -1000.0;
    if (water.x > 0.5)
    {
        inside = shore_distance(p, foot);
    }
    if (far > 0.0)
    {
        float rounded = smooth_height(p);
        float dry = 1.0 - smoothstep(-SHORE_CELL, 0.0, inside);
        float far_offset = (k1.w * (rounded - r.x) + far_relief(p, k3.x, k2.w, length(r.yz), foot)) * dry;
        offset = lerp(offset, far_offset, far);
    }
    float h = r.x + offset - bed_depth(p, inside, water.y);
    // The forest cover's canopy far off, kept out of the water and its shore.
    h += canopy(p, dist) * (1.0 - smoothstep(-SHORE_CELL, 0.0, inside));
    if (fine_share > 0.0)
    {
        h += fine(p, r, near.y, far, foot) * fine_share;
    }
    if (inside <= 0.0 && inside > -SHORE_NEAR)
    {
        h = max(h, shore_floor(level_at(p), inside));
    }
    return float4(h, near.y * near.w, r.x, far);
}

// --- the mesh ---------------------------------------------------------------------------------

float slid(float2 p, float2 morph, float2 eye)
{
    float d = length(p - eye);
    return clamp((d - morph.x) / max(morph.y - morph.x, 1e-3), 0.0, 1.0);
}

float2 slide(float2 p, float2 grid, float spacing, float k)
{
    float2 odd = frac(grid * 0.5) * 2.0;
    return p - odd * spacing * k;
}

float fine_share_of(float level, float k)
{
    return level < 0.5 ? 1.0 - k : 0.0;
}

// A patch vertex's world position, its level's grid slid onto the next level's toward the ring's
// edge; its distance from the eye; and how far it has slid.
float4 patch_vertex(uint vi, uint patch, out float level)
{
    float4 at = row(push.patches, patch * 2);
    float4 morph = row(push.patches, patch * 2 + 1);
    level = at.w;
    uint quad = vi / 6;
    uint corner = vi % 6;
    uint qx = quad % CELLS;
    uint qy = quad / CELLS;
    float spacing = at.z;
    float2 center = at.xy + (float2(float(qx), float(qy)) + 0.5) * spacing;
    bool cut = sw_to_ne(int2(floor(center / vertex_spacing())));
    uint2 ne[6] = { uint2(0, 0), uint2(1, 0), uint2(1, 1), uint2(0, 0), uint2(1, 1), uint2(0, 1) };
    uint2 nw[6] = { uint2(0, 0), uint2(1, 0), uint2(0, 1), uint2(1, 0), uint2(1, 1), uint2(0, 1) };
    uint2 c = cut ? ne[corner] : nw[corner];
    float2 grid = float2(float(qx + c.x), float(qy + c.y));
    float2 p0 = at.xy + grid * spacing;
    float3 eye = row(push.drawn, D_EYE).xyz + float3(row(push.drawn, D_GLOBAL).xy, 0.0);
    float k = slid(p0, morph.xy, eye.xy);
    float2 p = slide(p0, grid, spacing, k);
    float dist = length(float3(p - eye.xy, retail_at(p).x - eye.z));
    return float4(p, dist, k);
}

struct Varyings
{
    float4 position : SV_Position;
    // xy world meters, z the drawn height, w the distance from the eye.
    float4 world : TEXCOORD0;
    float fine_share : TEXCOORD1;
};

[shader("vertex")]
Varyings vs_ground(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    float level;
    float4 v = patch_vertex(vertex, push.first + instance, level);
    float fine_share = fine_share_of(level, v.w);
    float h = drawn_at(v.xy, v.z, fine_share, 0.0).x;
    float2 global = row(push.drawn, D_GLOBAL).xy;
    float3 at = float3(v.xy - global, h - BELOW);
    Varyings o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(at, 1.0));
    o.world = float4(v.xy, h, v.z);
    o.fine_share = fine_share;
    return o;
}

// The drawn surface's normal at world `p`, where it stands at `h`, by differences `e` meters on.
float3 drawn_normal(float2 p, float dist, float e, float h, float fine_share, float foot)
{
    float hx = drawn_at(p + float2(e, 0.0), dist, fine_share, foot).x - h;
    float hy = drawn_at(p + float2(0.0, e), dist, fine_share, foot).x - h;
    return normalize(float3(-hx / e, -hy / e, 1.0));
}

// The smooth surface's normal at world `p`, by differences `e` meters on.
float3 smooth_normal(float2 p, float e)
{
    float h = smooth_height(p);
    float hx = smooth_height(p + float2(e, 0.0)) - h;
    float hy = smooth_height(p + float2(0.0, e)) - h;
    return normalize(float3(-hx / e, -hy / e, 1.0));
}

// The eased smooth surface's normal at world `p`, by differences `e` meters on.
float3 eased_normal(float2 p, float e)
{
    float h = eased_height(p);
    float hx = eased_height(p + float2(e, 0.0)) - h;
    float hy = eased_height(p + float2(0.0, e)) - h;
    return normalize(float3(-hx / e, -hy / e, 1.0));
}


// How far a pack's bank blends from the cell beside it into a cell, meters: where the cell beside
// lays the bed in place of retail's water ground and this one lays the land beside the water, the
// bed comes in over this much of this cell, so their side is no line between the two.
static const float BED_BESIDE = 6.0;

// Whether the held cell at world `p` lays a pack's bed in place of retail's water ground.
bool lays_bed(float2 p)
{
    uint4 o = row_u(push.drawn, D_ORIGIN);
    int2 b = int2(floor(p / BLOCK));
    int2 at = b - int2(o.xy) + int(o.z);
    if (any(at < 0) || any(at >= int(o.w)))
    {
        return false;
    }
    uint2 entry = row_u(push.blocks, uint(at.y) * o.w + uint(at.x)).xy;
    if (entry.x == 0xFFFFFFFF)
    {
        return false;
    }
    uint side = max(entry.y, 1u);
    float2 local = (p - float2(b) * BLOCK) / (BLOCK / float(side));
    uint2 ij = uint2(clamp(floor(local), 0.0, float(side - 1)));
    uint w = row_u(push.cells, entry.x + ij.x * side + ij.y).w;
    return w != 0xFFFFFFFF && (row_u(push.splats, w * SPLAT_ROWS).w & SPLAT_BANK_IS_BED) != 0;
}

// How much of the bed a cell `size` meters a side that lays the land beside the water takes at
// `f` across it (0 to 1 each way), from the cells about it that lay the bed: all of it at their
// side or corner, none `BED_BESIDE` meters in from it.
float bed_beside(float2 p, float2 f, float size)
{
    float2 lo = f * size;
    float2 hi = (1.0 - f) * size;
    if (min(min(lo.x, lo.y), min(hi.x, hi.y)) >= BED_BESIDE)
    {
        return 0.0;
    }
    float2 corner = p - lo;
    float most = 0.0;
    for (int i = -1; i <= 1; i++)
    {
        for (int j = -1; j <= 1; j++)
        {
            if (i == 0 && j == 0)
            {
                continue;
            }
            float dx = i < 0 ? lo.x : (i > 0 ? hi.x : 0.0);
            float dy = j < 0 ? lo.y : (j > 0 ? hi.y : 0.0);
            float d = length(float2(dx, dy));
            if (d >= BED_BESIDE)
            {
                continue;
            }
            if (lays_bed(corner + (float2(i, j) + 0.5) * size))
            {
                most = max(most, 1.0 - smoothstep(0.0, BED_BESIDE, d));
            }
        }
    }
    return most;
}

// The ground's picture at world `p` and the water's share of it: its cell's, where its block is
// held; far off, its corners' terrain's colors. `dx`, `dy` are `p`'s steps across the screen;
// `takes` how readily it gathers puddles, 0 to 1.
float4 picture(float2 p, float2 dx, float2 dy, float3 steep, float dist, float foot, out uint kind, out float3 bend, out float roughness, out float takes)
{
    kind = 0;
    bend = float3(0.0, 0.0, 1.0);
    roughness = 1.0;
    takes = 0.0;
    uint4 o = row_u(push.drawn, D_ORIGIN);
    int2 b = int2(floor(p / BLOCK));
    int2 at = b - int2(o.xy) + int(o.z);
    if (all(at >= 0) && all(at < int(o.w)))
    {
        uint2 entry = row_u(push.blocks, uint(at.y) * o.w + uint(at.x)).xy;
        if (entry.x != 0xFFFFFFFF)
        {
            uint side = max(entry.y, 1u);
            float size = BLOCK / float(side);
            float2 local = (p - float2(b) * BLOCK) / size;
            uint2 ij = uint2(clamp(floor(local), 0.0, float(side - 1)));
            float2 f = local - float2(ij);
            uint4 c = row_u(push.cells, entry.x + ij.x * side + ij.y);
            uint rot = c.y & 3;
            float2 a = UVS[SW[rot]];
            float2 e = UVS[SE[rot]] - a;
            float2 n = UVS[NW[rot]] - a;
            float2 uv = a + f.x * e + f.y * n;
            float2 udx = (e * dx.x + n * dx.y) / size;
            float2 udy = (e * dy.x + n * dy.y) / size;
            uint cell_kind = (c.z >> 1) & 7;
            if (c.w != 0xFFFFFFFF)
            {
                uint at_splat = c.w * SPLAT_ROWS;
                uint flags = row_u(push.splats, at_splat).w;
                float4 waters = row(push.drawn, D_BANK);
                bool banks = (flags & SPLAT_HAS_BANK) != 0 && waters.x > 0.5;
                float2 inside = clamp(uv, 0.0, 1.0);
                PackPlace place = no_pack_place();
                // On a grid finer than a square a cell, the square's place in its cell, in the
                // cell's own coordinates turned as the square's are.
                float of = float(world_steps(push.heights));
                uint2 k = ij % uint(of);
                place.cell_of = of;
                place.cell_at = float(k.x) * e + float(k.y) * n + (of - 1.0) * a;
                if ((flags & SPLAT_HAS_BANK) != 0)
                {
                    float2 w = fmod(fmod(p / CELL, PACK_PERIOD_CELLS) + PACK_PERIOD_CELLS, PACK_PERIOD_CELLS);
                    place.varies = true;
                    place.land = banks;
                    place.q = float2(w.x, -w.y);
                    place.qdx = float2(dx.x, -dx.y) / CELL;
                    place.qdy = float2(dy.x, -dy.y) / CELL;
                    place.dist = dist;
                }
                SplatSample s = splat_at(push.splats, push.wrap, push.clamp, at_splat, rot, inside, udx, udy, place);
                kind = s.water > 0.5 ? KIND_WATER : (cell_kind == KIND_WATER ? 0 : cell_kind);
                bend = s.bend;
                roughness = s.roughness;
                takes = s.puddles;
                float height = s.height;
                float from_water = -1000.0;
                if (banks)
                {
                    from_water = shore_distance(p, foot);
                    // Well inside the water, the bed's own ground in place of the land beside it;
                    // and by the side of a cell that lays the bed there, coming in over a few
                    // meters, so their side is no line between the two.
                    float into = smoothstep(BED_FROM, BED_IN, from_water);
                    if ((flags & SPLAT_BANK_IS_BED) == 0)
                    {
                        into = max(into, bed_beside(p, f, size));
                    }
                    into *= s.water;
                    if (into > 0.001)
                    {
                        SteepSample t = bank_row(push.splats, push.wrap, at_splat, rot, inside, udx, udy, 2, place);
                        s.color = lerp(s.color, t.color, into);
                        bend = lerp(bend, t.bend, into);
                        roughness = lerp(roughness, t.roughness, into);
                        height = lerp(height, t.height, into);
                    }
                    // A muddy bank: grass grows in over the mud in patches, more the farther from
                    // the water's edge, so the shore reads as wet mud at the water giving way to a
                    // grassy bank, not as a flat ring of mud. Patches finer than a pixel are left
                    // out.
                    if (waters.y > 0.0 && s.mud > 0.01 && from_water < 0.0 && from_water > -BANK_REACH)
                    {
                        float away = saturate((-1.0 - from_water) / 9.0);
                        away = away * away * (3.0 - 2.0 * away);
                        float n = lerp(0.5, value_noise(p, 16.0), resolved(16.0, foot)) * 0.6
                            + lerp(0.5, value_noise(p + float2(7.0, 3.0), 4.0), resolved(4.0, foot)) * 0.4;
                        float lo = 0.72 - 0.45 * away;
                        float share = waters.y * s.mud * away * smoothstep(lo, lo + 0.16, n);
                        if (share > 0.002)
                        {
                            SteepSample t = bank_row(push.splats, push.wrap, at_splat, rot, inside, udx, udy, 0, place);
                            share = height_share(height, t.height, share, dist);
                            s.color = lerp(s.color, t.color, share);
                            bend = lerp(bend, t.bend, share);
                            roughness = lerp(roughness, t.roughness, share);
                            height = lerp(height, t.height, share);
                        }
                    }
                }
                if ((flags & 16) != 0)
                {
                    float4 k = row(push.drawn, D_SLOPE2);
                    if (steep.x > 0.002)
                    {
                        SteepSample t = steep_at(push.splats, push.wrap, push.clamp, at_splat, rot, inside, udx, udy, 0, k.z, place);
                        float share = steep.x * k.x * t.cover;
                        if (place.varies)
                        {
                            share = height_share(height, t.height, share, dist);
                        }
                        s.color = lerp(s.color, t.color, share);
                        bend = lerp(bend, t.bend, share);
                        roughness = lerp(roughness, t.roughness, share);
                        height = lerp(height, t.height, share);
                    }
                    if (steep.y > 0.002 || steep.z > 0.002)
                    {
                        // A ground's own steep material covers all of its unwalkable ground, the
                        // bands' treads as well as their risers; the generic rock keeps to the
                        // risers.
                        SteepSample t = steep_at(push.splats, push.wrap, push.clamp, at_splat, rot, inside, udx, udy, 1, k.z, place);
                        float share = max(steep.y, steep.z * t.own) * k.x * t.cover;
                        if (place.varies)
                        {
                            share = height_share(height, t.height, share, dist);
                        }
                        s.color = lerp(s.color, t.color, share);
                        bend = lerp(bend, t.bend, share);
                        roughness = lerp(roughness, t.roughness, share);
                    }
                    bend = normalize(bend);
                }
                if (s.pack > 0.0)
                {
                    // Gentle variation of the pack's color, in patches a few meters across and
                    // over tens and hundreds of meters, as the ground's own pictures vary from
                    // place to place; each left out as it grows finer than the pixel.
                    float patches = (value_noise(p, 4.0) - 0.5) * 0.08 * resolved(4.0, foot)
                        + (value_noise(p + float2(17.0, 5.0), 6.0) - 0.5) * 0.06 * resolved(6.0, foot);
                    float broad = (value_noise(p, 32.0) - 0.5) * 0.14 * resolved(32.0, foot)
                        + (value_noise(p + float2(500.0, 500.0), 128.0) - 0.5) * 0.12 * resolved(128.0, foot);
                    // A share of the light; the ground's colors are display values.
                    s.color *= lerp(1.0, pow(1.0 + patches + broad, 1.0 / 2.2), s.pack);
                }
                if (banks)
                {
                    // Wet ground: retail's water laid above the water's edge, and a strip of any
                    // ground along it, darker and smoother, drying away from the water.
                    float damp = smoothstep(-3.0, 0.0, from_water) * 0.6;
                    float wet = saturate(max(s.water * smoothstep(-SHORE_CELL, -2.0, from_water), damp));
                    s.color *= pow(max(1.0 - waters.z * wet, 0.0), 1.0 / 2.2);
                    roughness = lerp(roughness, 0.55, wet * 0.6);
                }
                return float4(s.color, s.water);
            }
            float water = cell_kind == KIND_WATER ? 1.0 : 0.0;
            kind = cell_kind;
            takes = float((c.z >> 4) & 3) / 3.0;
            if (c.x != 0xFFFFFFFF)
            {
                return float4(sampled(c.x, push.clamp, uv, udx, udy).rgb, water);
            }
            return float4(1.0, 1.0, 1.0, water);
        }
    }
    // Far off: the corners' terrain's colors across the cell.
    float2 q = p / vertex_spacing();
    int2 g = int2(floor(q));
    float2 f = clamp(q - float2(g), 0.0, 1.0);
    float4 corners[4];
    int2 offs[4] = { int2(0, 0), int2(1, 0), int2(0, 1), int2(1, 1) };
    float weights[4];
    for (int k = 0; k < 4; k++)
    {
        uint w = vertex_word(g + offs[k]);
        uint t = (w >> 2) & 0x1F;
        float4 col = ground_color(w);
        corners[k] = float4(col.rgb, (t >= 16 && t <= 20) ? 1.0 : 0.0);
        weights[k] = puddle_weight(w);
    }
    float4 s = lerp(lerp(corners[0], corners[1], f.x), lerp(corners[2], corners[3], f.x), f.y);
    takes = lerp(lerp(weights[0], weights[1], f.x), lerp(weights[2], weights[3], f.x), f.y);
    kind = s.a > 0.5 ? KIND_WATER : 0;
    return s;
}

struct Targets
{
    float4 albedo : SV_Target0;
    float2 normal : SV_Target1;
    float4 light : SV_Target2;
    float4 material : SV_Target3;
};

// Where the sun's shadows end, meters from the eye: past them the forest cover's crowns shade the
// ground under them.
static const float SHADOWS_END = 560.0;

// The ground's color `albedo` at world `p`, `dist` meters from the eye, `steep` how far the line
// to the eye rises in a meter, under the forest cover: shaded by its crowns past the sun's
// shadows, and laid with their color as the far trees thin out and past them. Seen low across the
// land, the crowns hide more of the ground between them than they cover from above.
float3 forest(float3 albedo, float2 p, float dist, float steep)
{
    float4 k = row(push.drawn, D_COVER);
    if (k.x <= 0.0)
    {
        return albedo;
    }
    float2 at = (p / CELL + float2(0.5, 0.5)) / float(COVER_SIDE);
    float4 c = textures[NonUniformResourceIndex(row_u(push.drawn, D_COVER_PICTURES).x)].SampleLevel(
        samplers[NonUniformResourceIndex(push.clamp)], at, 0.0);
    if (c.a <= 0.0)
    {
        return albedo;
    }
    float low = 1.0 - saturate(steep);
    float seen = 1.0 - pow(1.0 - c.a, 1.0 + 2.0 * low);
    float shade = c.a * k.w * smoothstep(SHADOWS_END - 60.0, SHADOWS_END + 60.0, dist);
    float laid = seen * k.x * smoothstep(k.y, k.z, dist);
    // The ground's colors are display values: the shade is a share of the light.
    return lerp(albedo * pow(1.0 - shade, 1.0 / 2.2), c.rgb, saturate(laid));
}

// The ground `c` at world `p`, `dist` meters from the eye, as the flowers' colors lie on it where
// they are no longer drawn: from halfway out to the flowers' `reach` and past it, specks of each
// cluster's colors where the flowers grow, or, finer than a pixel `foot` meters across, their mean.
float3 flower_specks(float3 c, float2 p, float dist, float foot, float reach)
{
    float fade = smoothstep(reach * 0.5, reach, dist);
    if (fade <= 0.0)
    {
        return c;
    }
    float3 unused;
    float4 grows = grass_ground(p, unused);
    float cluster = value_noise(p, 9.0) * 0.7 + value_noise(p + float2(41.0, 13.0), 3.0) * 0.3;
    float share = grows.x * smoothstep(0.5, 0.64, cluster) * (1.0 - smoothstep(0.4, 0.6, length(cover_gradient(p, retail_at(p).yz))));
    if (share <= 0.0)
    {
        return c;
    }
    uint scene = vertex_word(int2(round(p / vertex_spacing()))) >> 11;
    uint k = grass_hash(uint(floor(p.x / 9.0)) * 7919u ^ uint(floor(p.y / 9.0)) * 104729u);
    float3 a = row(push.drawn, D_PALETTE + (scene + (k & 3)) % 8).rgb;
    float3 b = row(push.drawn, D_PALETTE + (scene + (k & 3) + 2) % 8).rgb;
    float speck = smoothstep(0.78, 0.9, value_noise(p, 0.5));
    float3 pick = value_noise(p + float2(5.0, 7.0), 0.5) > 0.5 ? a : b;
    float seen = resolved(0.5, foot);
    float3 specks = lerp(c, pick, speck * 0.85);
    float3 mean = lerp(c, 0.5 * (a + b), 0.16);
    return lerp(c, lerp(mean, specks, seen), share * fade);
}

[shader("fragment")]
Targets fs_ground(Varyings in)
{
    float2 p = in.world.xy;
    float dist = in.world.w;
    // The steps across the screen, taken before any branch.
    float2 dx = ddx(p);
    float2 dy = ddy(p);
    float foot = max(length(dx), length(dy));
    if (dist > row(push.drawn, D_GLOBAL).w)
    {
        discard;
    }
    float4 parts = drawn_at(p, dist, in.fine_share, foot);
    float e = max(clamp(dist * 0.002, 0.1, 6.0), foot);
    float3 n = drawn_normal(p, dist, e, parts.x, in.fine_share, foot);
    float3 drawn_n = n;
    // Where no offset is laid the ground is retail's, shaded with the smooth surface's normal as
    // the retail grid's vertex normals shade it, not with its triangles' own.
    float drawn_share = lerp(parts.y, 1.0, parts.w);
    if (drawn_share < 0.999)
    {
        n = normalize(lerp(smooth_normal(p, max(e, 0.5)), n, drawn_share));
    }
    uint kind;
    float3 bend;
    float roughness;
    float takes;
    // How steep the ground is for what is laid by its slope: by the drawn ground's own shape, its
    // rounding, pins and rock bands but not the fine relief, over at least a meter so its
    // samples do not show; where no offset is laid, by the smooth surface it is rounded to, eased
    // where it meets a steep face. On walkable ground that surface's slope is taken as much as
    // asked. x the scree's share, y the rock's, z how far from walkable it is.
    float3 steep = float3(0.0, 0.0, 0.0);
    float4 sloped = row(push.drawn, D_SLOPE2);
    if (sloped.x > 0.0)
    {
        float4 from = row(push.drawn, D_SLOPE);
        float es = max(e, 1.0);
        float smooth_up = eased_normal(p, max(e, 0.5)).z;
        // Without the fine relief and over a meter or more, the shading's own normal.
        float3 shape = drawn_n;
        if (in.fine_share > 0.0 || e < 1.0)
        {
            float shaped = in.fine_share > 0.0 ? drawn_at(p, dist, 0.0, foot).x : parts.x;
            shape = drawn_normal(p, dist, es, shaped, 0.0, foot);
        }
        float level = lerp(smooth_up, shape.z, drawn_share);
        steep.z = 1.0 - smoothstep(WALKABLE_Z - 0.04, WALKABLE_Z + 0.04, level);
        float up = lerp(level, smooth_up, sloped.y * (1.0 - steep.z));
        float angle = degrees(acos(clamp(up, -1.0, 1.0)));
        steep.x = smoothstep(from.x, from.x + max(from.y, 0.1), angle);
        steep.y = smoothstep(from.z, from.z + max(from.w, 0.1), angle);
    }
    float4 t = picture(p, dx, dy, steep, dist, foot, kind, bend, roughness, takes);
    n = bent(n, bend);
    // The ground's light as the grid's vertices take it, here at every pixel; and the fog.
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float2 global = row(push.drawn, D_GLOBAL).xy;
    float3 at = float3(p - global, parts.x);
    float fog = fog_factor(push.frame, length(at - eye));
    float4 field = row(push.drawn, D_GRASS);
    if (field.x > 0.5 && t.a < 0.5)
    {
        // Under the grass field the ground between the blades is shaded by them; toward its reach
        // and past it, the ground takes the blades' own mean color where grass grows, so the field
        // never ends in a line.
        float3 unused;
        float4 g = grass_ground(p, unused);
        float bare = value_noise(p, 6.0) * 0.65 + value_noise(p + float2(17.0, 5.0), 2.2) * 0.35;
        float lo = lerp(0.15, 0.36, g.w);
        // None on ground steep enough to be rock.
        float share = g.x * lerp(0.4, 1.0, smoothstep(lo, lo + 0.12, bare)) * (1.0 - smoothstep(0.62, 0.7, length(retail_at(p).yz)))
            * smoothstep(0.8, 0.84, smooth_normal(p, 2.0).z);
        float under = share * (1.0 - smoothstep(field.z * 0.8, field.z, dist));
        float past = share * smoothstep(field.y * 0.5, field.z, dist);
        t.rgb *= lerp(1.0, 0.8, under);
        t.rgb = lerp(t.rgb, t.rgb * float3(0.86, 0.97, 0.72), past);
    }
    float4 flowers = row(push.drawn, D_FLOWERS);
    if (flowers.x > 0.5 && field.x > 0.5 && t.a < 0.5)
    {
        t.rgb = flower_specks(t.rgb, p, dist, foot, flowers.y);
    }
    t.rgb = forest(t.rgb, p, dist, abs(eye.z - at.z) / max(length(eye - at), 1e-3));
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    // How far below the water's surface the bed is drawn here, in steps of 1 cm: the water stands
    // that far over the ground's own pixel.
    float4 water = row(push.drawn, D_WATER);
    float bed = water.x > 0.5 ? bed_depth(p, shore_distance(p, foot), water.y) : 0.0;
    // Counted from the depth this pixel writes, the mesh's, which lies a little off the drawn
    // height between its vertices where the bed eases or slopes, so the surface over it stays at
    // the water's level.
    float depth = bed > 0.0 ? max(parts.x + bed - (in.world.z - BELOW), 0.0) : 0.0;
    uint below = uint(clamp(depth / BED_STEP + 0.5, 0.0, 255.0));
    // Where the bed is lowered under the water's level the ground is water, whatever retail's
    // picture lays there, so the water's surface runs on over the land inside its edge rather
    // than ending where the picture turns to land.
    if (bed > SURFACE_FROM && level_at(p) - parts.x > SURFACE_FROM)
    {
        kind = KIND_WATER;
    }
    // Ground standing over the water's level keeps less of its water, or none (all of it on
    // retail's grid). A water share of nought means all of the pixel is water.
    if (kind == KIND_WATER)
    {
        float kept = water_kept(p, parts.x);
        if (kept < 1.0)
        {
            t.a = (t.a > 0.0 ? t.a : 1.0) * kept;
            if (t.a * 255.0 < 0.5)
            {
                kind = 0;
                t.a = 0.0;
            }
        }
    }
    // Ground left dry, where its bed still stands over the water's level, has no water over it.
    if (kind != KIND_WATER)
    {
        below = 0;
    }
    // Where the ground is not water and the weather is drawn, the fourth byte is how much of it is
    // puddle instead: on ground the smooth surface holds flat, judged over half a meter however
    // far off it is drawn, and over the stretch the pixel covers where that is longer, so a far
    // pixel drawn across the foot of a slope gathers none from the flat half meter at its middle.
    if (kind != KIND_WATER && push.weather != NO_WEATHER)
    {
        float3 level = smooth_normal(p, 0.5);
        float3 wide = smooth_normal(p, max(e, 0.5));
        if (wide.z < level.z)
        {
            level = wide;
        }
        below = puddle_byte(puddle_share(push.weather, at, level, takes * (1.0 - t.a)));
    }
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_GROUND, n, t.a, float4(t.rgb, roughness));
    o.normal = oct_encode(n);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : ground_light(push.frame, n), fog);
    o.material = material_bytes(CLASS_GROUND, kind, uint(t.a * 255.0 + 0.5) | (below << 8));
    return o;
}

// The ground as the sun sees it, for its shadow cascades: without the fine relief. `frame` is the
// cascade's own constants, its view and projection the sun's.
[shader("vertex")]
float4 vs_caster(uint vertex : SV_VertexID, uint instance : SV_InstanceID) : SV_Position
{
    float level;
    float4 v = patch_vertex(vertex, push.first + instance, level);
    float h = drawn_at(v.xy, v.z, 0.0, 0.0).x;
    float2 global = row(push.drawn, D_GLOBAL).xy;
    return transform(push.frame, FRAME_VIEW_PROJ, float4(v.xy - global, h, 1.0));
}

// The drawn height at points: each point's world x and y and its distance from the eye in, its
// height out (no fine relief), for the checks that the ground is drawn where the scene stands
// what is on it; or, asked for, how much water is drawn over it. `patches` holds the points,
// `cells` takes the heights.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_probe(uint3 id : SV_DispatchThreadID)
{
    uint count = buffers[NonUniformResourceIndex(push.patches)].Load(0);
    if (id.x >= count)
    {
        return;
    }
    float4 q = asfloat(buffers[NonUniformResourceIndex(push.patches)].Load4(16 + id.x * 16));
    float h = drawn_at(q.xy, q.z, 0.0, 0.0).x;
    // Asked for in a point's fourth float: in place of the height, how much of the water its
    // pictures lay is drawn over the ground there.
    if (q.w > 0.5)
    {
        h = water_kept(q.xy, h);
    }
    rw_buffers[NonUniformResourceIndex(push.cells)].Store(id.x * 4, asuint(h));
}
// --- the grass field --------------------------------------------------------------------------
//
// Clumps of grass placed over the drawn ground round the eye, each frame, from the terrain grid
// alone: candidate points on a fixed hash of the world, square tiles of it, each kept as often as
// the ground there grows grass. How much grows comes from the terrain types of the cell's four
// corners, less on slopes, none on the road, in the water or under what stands overhead, and
// little in bare patches that wander with the world's noise. Near the eye the clumps are dense
// and fine; farther off fewer, wider ones stand in for them, and both shrink away rather than pop.
// The draw of the clumps is `grass.slang`.
//
// The push's fields carry, for this entry alone: `patches` the field's constants, `cells` the
// clumps written, `blocks` the draws' commands, `first` the bytes of one command.

static const uint G_PLANES = 0; // the view's six planes, each the inward normal and its offset
static const uint G_EYE = 6;    // xyz the eye, the frame's space; w how far the near clumps reach
static const uint G_FIELD = 7;  // xy the frame's origin, world meters; z how far the far clumps reach; w how
                                // wide the near clumps hand over to the far
static const uint G_TILES = 8;  // (uint) x tiles a side, y and z the first tile's world index (signed), w
                                // the near clumps' room
static const uint G_LOOK = 10;  // x near clumps a square meter, y far ones, z a height factor, w the far
                                // clumps' room
static const uint G_TYPES = 11; // 32 rows kept for the layout; what grows is read from the drawn
                                // constants' D_GRASS_TYPES
static const float GRASS_TILE = 8.0;
static const uint CLUMP_BYTES = 32;

uint grass_hash(uint x)
{
    x ^= x >> 16;
    x *= 0x7FEB352Du;
    x ^= x >> 15;
    x *= 0x846CA68Bu;
    x ^= x >> 16;
    return x;
}

float grass_unit(uint h)
{
    return float(h & 0xFFFFFFu) / 16777216.0;
}

// How far inside a road point (`dx`, `dy`) of a square `cell` meters a side lies, meters, negative
// outside: the square's four corners' road bits (south-west, north-west, south-east, north-east)
// shape it as the game's own on-road test does, a road half-width of `W` meters.
float road_inside(bool a, bool b, bool c, bool d, float dx, float dy, float cell, float W)
{
    float F = cell - W;
    static const float R = 0.70710678;
    uint n = uint(a) + uint(b) + uint(c) + uint(d);
    if (n == 0)
    {
        return -100.0;
    }
    if (n == 4)
    {
        return 100.0;
    }
    if (n == 1)
    {
        if (a)
        {
            return (W - dx - dy) * R;
        }
        if (b)
        {
            return (dy - dx - F) * R;
        }
        if (c)
        {
            return (dx - dy - F) * R;
        }
        return (dx + dy - cell - F) * R;
    }
    if (n == 2)
    {
        if (a && b)
        {
            return W - dx;
        }
        if (a && c)
        {
            return W - dy;
        }
        if (c && d)
        {
            return dx - F;
        }
        if (b && d)
        {
            return dy - F;
        }
        if (a && d)
        {
            return (W - abs(dx - dy)) * R;
        }
        return (W - abs(dx + dy - cell)) * R;
    }
    if (!d)
    {
        return max(W - dx, W - dy);
    }
    if (!c)
    {
        return max(W - dx, dy - F);
    }
    if (!b)
    {
        return max(dx - F, W - dy);
    }
    return max(dx - F, dy - F);
}

// What grows at world `p` by the terrain grid alone: x the share of grass, y its height, z its
// dryness, w how patchy; and the ground's own color there.
float4 grass_ground(float2 p, out float3 color)
{
    float cell = vertex_spacing();
    float2 q = p / cell;
    int2 c = int2(floor(q));
    float2 f = q - float2(c);
    uint words[4] = { vertex_word(c), vertex_word(c + int2(1, 0)), vertex_word(c + int2(0, 1)), vertex_word(c + int2(1, 1)) };
    float weights[4] = { (1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y };
    float4 sum = float4(0.0, 0.0, 0.0, 0.0);
    float grows = 0.0;
    color = float3(0.0, 0.0, 0.0);
    for (uint k = 0; k < 4; k++)
    {
        uint t = (words[k] >> 2) & 31;
        float4 kind = row(push.drawn, D_GRASS_TYPES + t);
        float w = weights[k];
        sum.x += w * kind.x;
        sum.yzw += w * kind.x * kind.yzw;
        grows += w * kind.x;
        color += w * ground_color(words[k]).rgb;
    }
    sum.yzw /= max(grows, 1e-4);
    // No grass on the road, the edge wandering a little.
    float inside = road_inside((words[0] & 3) != 0, (words[2] & 3) != 0, (words[1] & 3) != 0,
                               (words[3] & 3) != 0, f.x * cell, f.y * cell, cell, world_road_width(push.heights))
        + (value_noise(p, 1.7) - 0.5) * 0.9;
    sum.x *= smoothstep(0.3, -1.2, inside);
    return sum;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_grass(uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID)
{
    uint g = push.patches;
    uint4 tiles = row_u(g, G_TILES);
    int2 tile = int2(asint(tiles.y) + int(group.x), asint(tiles.z) + int(group.y));
    float4 field = row(g, G_FIELD);
    float4 eye4 = row(g, G_EYE);
    float3 eye = eye4.xyz;
    float near_reach = eye4.w;
    float far_reach = field.z;
    float hand = max(field.w, 0.5);
    float2 origin = float2(tile) * GRASS_TILE;
    float2 centre = origin + 0.5 * GRASS_TILE - field.xy;
    float tile_dist = length(centre - eye.xy);
    if (tile_dist - GRASS_TILE > far_reach)
    {
        return;
    }
    float3 c3 = float3(centre, retail_at(origin + 0.5 * GRASS_TILE).x);
    for (uint pl = 0; pl < 6; pl++)
    {
        float4 plane = row(g, G_PLANES + pl);
        if (dot(plane.xyz, c3) + plane.w < -GRASS_TILE)
        {
            return;
        }
    }
    float4 look = row(g, G_LOOK);
    bool near_tile = tile_dist - GRASS_TILE < near_reach;
    uint near_count = near_tile ? uint(look.x * GRASS_TILE * GRASS_TILE + 0.5) : 0;
    uint far_count = tile_dist + GRASS_TILE > near_reach - hand ? uint(look.y * GRASS_TILE * GRASS_TILE + 0.5) : 0;
    uint tile_seed = grass_hash(uint(tile.x) * 73856093u ^ uint(tile.y) * 19349663u);
    float4 water = row(push.drawn, D_WATER);
    for (uint k = local.x; k < near_count + far_count; k += 64)
    {
        bool far = k >= near_count;
        uint h = grass_hash(tile_seed ^ ((k + (far ? 0x5bd1e995u : 0u)) * 0x9E3779B9u));
        uint h2 = grass_hash(h);
        uint h3 = grass_hash(h2);
        float2 p = origin + float2(grass_unit(h), grass_unit(h2)) * GRASS_TILE;
        float2 pf = p - field.xy;
        float d = length(float3(pf - eye.xy, retail_at(p).x - eye.z));
        // The near clumps thin a little with distance and shrink away into the far ones; the far
        // ones grow in where the near go, and shrink away toward their reach.
        float scale;
        float keep;
        if (!far)
        {
            scale = 1.0 - smoothstep(near_reach - hand, near_reach, d);
            keep = lerp(1.0, 0.35, smoothstep(10.0, near_reach, d));
        }
        else
        {
            scale = smoothstep(near_reach - hand, near_reach, d) * (1.0 - smoothstep(far_reach * 0.8, far_reach, d));
            keep = lerp(0.9, 0.2, smoothstep(near_reach, far_reach, d));
        }
        if (scale <= 0.02)
        {
            continue;
        }
        float3 ground_color;
        float4 grows = grass_ground(p, ground_color);
        float share = grows.x;
        if (share <= 0.0)
        {
            continue;
        }
        // Bare patches, more of them where the ground grows patchily.
        float n = value_noise(p, 6.0) * 0.65 + value_noise(p + float2(17.0, 5.0), 2.2) * 0.35;
        float lo = lerp(0.15, 0.36, grows.w);
        share *= smoothstep(lo, lo + 0.12, n);
        // Less on slopes, none on what is steep enough to be scree.
        float3 r = retail_at(p);
        float slope = length(cover_gradient(p, r.yz));
        share *= 1.0 - smoothstep(0.45, 0.67, slope);
        // A clump comes in by growing from nothing as the share kept passes its own number, never
        // whole at once.
        float grow = saturate((share * keep - grass_unit(h3)) / 0.12);
        if (grow <= 0.0)
        {
            continue;
        }
        // None in the water or at its very edge.
        if (water.x > 0.5)
        {
            float inside = shore_distance(p, 0.1);
            if (inside > -1.0 - grass_unit(h3 >> 3) * 1.5)
            {
                continue;
            }
        }
        float dist = length(float3(pf - eye.xy, r.x - eye.z));
        float ground = drawn_at(p, dist, 1.0, 0.0).x;
        // None under what stands overhead.
        if (push.weather != NO_WEATHER && sheltered(push.weather, float3(pf, ground), 0.25) > 0.5)
        {
            continue;
        }
        // Taller in some patches than others, a few meters across.
        float tall = grows.y * look.z * scale * grow * lerp(0.78, 1.18, grass_unit(grass_hash(h3)))
            * lerp(0.62, 1.28, value_noise(p + float2(29.0, 61.0), 3.5));
        // The field's color changes slowly over the land, a little drier here, a little lusher
        // there.
        float macro = value_noise(p, 23.0);
        float dry = saturate(grows.z + (macro - 0.5) * 0.5);
        float3 color = saturate(ground_color * lerp(0.9, 1.08, value_noise(p + float2(3.0, 9.0), 11.0)));
        uint slot;
        uint at = far ? 1 : 0;
        uint stride = push.first;
        rw_buffers[NonUniformResourceIndex(push.blocks)].InterlockedAdd(stride * at + stride - 12, 1, slot);
        uint room = far ? asuint(look.w) : tiles.w;
        if (slot >= room)
        {
            // No room: the count is taken back, so the draw never reads past the room.
            uint unused;
            rw_buffers[NonUniformResourceIndex(push.blocks)].InterlockedAdd(stride * at + stride - 12, 0xFFFFFFFFu, unused);
            continue;
        }
        uint index = (far ? tiles.w : 0) + slot;
        uint packed = uint(color.r * 255.0 + 0.5) | (uint(color.g * 255.0 + 0.5) << 8)
            | (uint(color.b * 255.0 + 0.5) << 16) | (uint(dry * 255.0 + 0.5) << 24);
        // Survivors of the thinning stand a little wider.
        float wider = min(1.0 / sqrt(max(keep, 0.2)), 2.0);
        RWByteAddressBuffer out_clumps = rw_buffers[NonUniformResourceIndex(push.cells)];
        out_clumps.Store4(index * CLUMP_BYTES, asuint(float4(pf, ground - 0.03, tall)));
        out_clumps.Store4(index * CLUMP_BYTES + 16, uint4(h3, packed, asuint(wider), asuint(scale)));
    }
}

// --- flowers and small scatter ----------------------------------------------------------------
//
// Over the same tiles near the eye, a sparser set of candidates: flowers on grassland, in
// clusters a few meters across that wander with the world's noise, each cluster's few colors
// chosen from its region's palette; and pebbles on bare ground, scree and along the road's edges.
// The push's `first` is the bytes of one draw command, as for the grass; the scatter's command is
// the third. Its constants: G_SCATTER (12 + 32 = 43) x candidates a square meter, y how far they
// reach, z the room for them, w how big; G_PEBBLES (44..75) each terrain type's x share of
// pebbles, y share of its flowers kept and z 1 where its pebbles lie at all (0 where a pack keeps
// them off its ground, flat and steep); G_PALETTE (76..83) the flowers' colors; G_ROADSIDE (84) x
// 1 where pebbles lie along the road's edges.

static const uint G_SCATTER = 43;
static const uint G_PEBBLES = 44;
static const uint G_PALETTE = 76;
static const uint G_ROADSIDE = 84;
static const uint PALETTE = 8;

uint pack_color(float3 c, uint flags)
{
    float3 s = saturate(c);
    return uint(s.r * 255.0 + 0.5) | (uint(s.g * 255.0 + 0.5) << 8) | (uint(s.b * 255.0 + 0.5) << 16) | (flags << 24);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_scatter(uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID)
{
    uint g = push.patches;
    uint4 tiles = row_u(g, G_TILES);
    float4 sc = row(g, G_SCATTER);
    int2 tile = int2(asint(tiles.y) + int(group.x), asint(tiles.z) + int(group.y));
    float4 field = row(g, G_FIELD);
    float3 eye = row(g, G_EYE).xyz;
    float reach = sc.y;
    float2 origin = float2(tile) * GRASS_TILE;
    float2 centre = origin + 0.5 * GRASS_TILE - field.xy;
    float tile_dist = length(centre - eye.xy);
    if (tile_dist - GRASS_TILE > reach)
    {
        return;
    }
    float3 c3 = float3(centre, retail_at(origin + 0.5 * GRASS_TILE).x);
    for (uint pl = 0; pl < 6; pl++)
    {
        float4 plane = row(g, G_PLANES + pl);
        if (dot(plane.xyz, c3) + plane.w < -GRASS_TILE)
        {
            return;
        }
    }
    uint count = uint(sc.x * GRASS_TILE * GRASS_TILE + 0.5);
    uint tile_seed = grass_hash(uint(tile.x) * 83492791u ^ uint(tile.y) * 2971215073u);
    float4 water = row(push.drawn, D_WATER);
    for (uint k = local.x; k < count; k += 64)
    {
        uint h = grass_hash(tile_seed ^ (k * 0x9E3779B9u));
        uint h2 = grass_hash(h);
        uint h3 = grass_hash(h2);
        float2 p = origin + float2(grass_unit(h), grass_unit(h2)) * GRASS_TILE;
        float2 pf = p - field.xy;
        float d = length(pf - eye.xy);
        // Thinning out and shrinking over most of the reach, each going by growing smaller rather
        // than at once, so no edge shows while walking; past the reach the ground carries their
        // color.
        float thin = 1.0 - smoothstep(reach * 0.25, reach, d);
        float scale = (1.0 - smoothstep(reach * 0.45, reach, d))
            * saturate((thin - grass_unit(grass_hash(h ^ 0x1B873593u))) / 0.2);
        if (scale <= 0.02)
        {
            continue;
        }
        float3 ground_color;
        float4 grows = grass_ground(p, ground_color);
        float3 r = retail_at(p);
        float slope = length(cover_gradient(p, r.yz));
        // Flowers: where grass grows, in clusters.
        float cluster = value_noise(p, 9.0) * 0.7 + value_noise(p + float2(41.0, 13.0), 3.0) * 0.3;
        float flowers = grows.x * smoothstep(0.5, 0.64, cluster) * (1.0 - smoothstep(0.4, 0.6, slope));
        // Pebbles: on bare ground by its type, and along the road's edge.
        float cell = vertex_spacing();
        float2 q = p / cell;
        int2 cc = int2(floor(q));
        float2 f = q - float2(cc);
        uint words[4] = { vertex_word(cc), vertex_word(cc + int2(1, 0)), vertex_word(cc + int2(0, 1)), vertex_word(cc + int2(1, 1)) };
        float weights[4] = { (1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y };
        float pebbles = 0.0;
        float kept = 0.0;
        float lie = 0.0;
        uint scene = 0;
        float most = -1.0;
        for (uint j = 0; j < 4; j++)
        {
            float4 ground_row = row(g, G_PEBBLES + ((words[j] >> 2) & 31));
            pebbles += weights[j] * ground_row.x * ground_row.z;
            kept += weights[j] * ground_row.y;
            lie += weights[j] * ground_row.z;
            if (weights[j] > most)
            {
                most = weights[j];
                scene = words[j] >> 11;
            }
        }
        float inside = road_inside((words[0] & 3) != 0, (words[2] & 3) != 0, (words[1] & 3) != 0,
                                   (words[3] & 3) != 0, f.x * cell, f.y * cell, cell, world_road_width(push.heights));
        pebbles = max(pebbles, row(g, G_ROADSIDE).x * 0.5 * (smoothstep(-3.0, -1.2, inside) - smoothstep(0.0, 1.5, inside)));
        pebbles += lie * 0.4 * smoothstep(0.5, 0.8, slope);
        flowers *= kept;
        float u = grass_unit(h3);
        bool flower = u < flowers * 0.85;
        bool pebble = !flower && u < flowers * 0.85 + pebbles * 0.3;
        if (!flower && !pebble)
        {
            continue;
        }
        if (water.x > 0.5 && shore_distance(p, 0.1) > -0.8)
        {
            continue;
        }
        float dist = length(float3(pf - eye.xy, r.x - eye.z));
        float ground = drawn_at(p, dist, 1.0, 0.0).x;
        if (push.weather != NO_WEATHER && sheltered(push.weather, float3(pf, ground), 0.25) > 0.5)
        {
            continue;
        }
        float3 color;
        float size;
        if (flower)
        {
            // A cluster's colors: two or three of its region's palette.
            uint c = grass_hash(uint(floor(p.x / 9.0)) * 7919u ^ uint(floor(p.y / 9.0)) * 104729u);
            uint pick = (scene + (c & 3) + (grass_hash(h3) % 3 == 0 ? 2u : 0u)) % PALETTE;
            color = row(g, G_PALETTE + pick).rgb * lerp(0.9, 1.08, grass_unit(grass_hash(h)));
            // A little under the grass round them, their heads among its tips.
            size = grows.y * lerp(0.75, 0.95, grass_unit(c >> 7));
        }
        else
        {
            float gray = dot(ground_color, float3(0.299, 0.587, 0.114));
            color = lerp(float3(gray, gray, gray), ground_color, 0.4) * lerp(0.75, 1.3, grass_unit(grass_hash(h2)));
            size = lerp(0.04, 0.13, pow(grass_unit(grass_hash(h3 ^ 0x51ED27u)), 2.0));
        }
        uint slot;
        uint stride = push.first;
        rw_buffers[NonUniformResourceIndex(push.blocks)].InterlockedAdd(stride * 2 + stride - 12, 1, slot);
        if (slot >= asuint(sc.z))
        {
            uint unused;
            rw_buffers[NonUniformResourceIndex(push.blocks)].InterlockedAdd(stride * 2 + stride - 12, 0xFFFFFFFFu, unused);
            continue;
        }
        uint index = tiles.w + asuint(row(g, G_LOOK).w) + slot;
        RWByteAddressBuffer out_clumps = rw_buffers[NonUniformResourceIndex(push.cells)];
        out_clumps.Store4(index * CLUMP_BYTES, asuint(float4(pf, ground - 0.01, size * sc.w * scale)));
        out_clumps.Store4(index * CLUMP_BYTES + 16, uint4(h3, pack_color(color, flower ? 1u : 0u), asuint(1.0), asuint(scale)));
    }
}
