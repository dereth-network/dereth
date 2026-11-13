// The far trees: every tree with far pictures, out to the reach, culled here each frame and drawn
// into the G-buffer as a card that stands upright and turns to the eye.
//
// Most trees' pictures are made on the device as the impostors' are, and drawn as the impostors
// draw theirs (`card.slang`): a card round the sphere pictured, the views either side of the eye
// through halves of a screen pattern, and the coverage kept as a share of a pattern, the patterns
// turning frame by frame while the frames are laid over each other. Near the blocks round the
// viewer a tree whose impostor's atlas row is made reads it as the impostor does; farther off, or
// without one, it reads its far picture's smaller copies. So a tree handed between the two draws
// alike, and so does a tree of a block that leaves the blocks round the viewer. Every tree keeps
// its own colors. An art pack's own pictures of a model are drawn as a quad across its axis, the
// view nearest the eye's bearing.
//
// A tree is kept while it lies within the reach, outside every block whose own objects stand in
// the world (those draw their trees themselves), and in the view. Past the distance every tree is
// drawn to, the trees thin out toward the reach: a tree's own number says how soon it goes, and it
// shrinks away over a short stretch rather than popping. The trees left standing stand wider,
// making up part of the coverage the thinning takes, and a tree too small for the screen to hold
// shrinks away too. A tree far off that stands on the skyline, with no land beyond it rising as
// high as its top, shrinks toward a small part of its size the farther off it stands: there it
// would stand against the sky as a spike out of all proportion to the hill under it.
import bindless;
import card;
import retail;
import weather;

static const float TAU = 6.2831853;
// A model whose pictures are not on the device yet.
static const uint NO_PICTURES = 0xFFFFFFFF;

// The constants' rows.
static const uint C_PLANES = 0;  // the view's six planes, each the inward normal and its offset
static const uint C_EYE = 6;     // xyz the eye, the frame's space; w how many trees
static const uint C_REACH = 7;   // xy the frame's origin in the world, meters; z the reach; w where the thinning starts
static const uint C_LOOK = 8;    // x how much of the thinned coverage is made up by standing wider; y how far a
                                 // tree's fade stretches, a share of the thinning; z pixels a meter at a meter's
                                 // distance; w the least height on screen a tree is drawn at, pixels
static const uint C_FAR = 9;     // x how far a tree is sunk, meters a meter of distance; y how many levels coarser
                                 // its pictures are read than its quad calls for; z the share of its size a tree
                                 // on the skyline keeps; w how far the blocks round the viewer reach, meters (0
                                 // unknown): a made tree is sunk only past them, and read softer only from twice
                                 // as far

// A tree, three rows: its foot and scale; its rotation; and (as words) its model, number and slot.
// A model, MODEL_ROWS rows:
// - the centre of its bounds and their radius;
// - the centre of the sphere its pictures are made round, in the space they are made in, and its
//   radius;
// - three rows of where that space stands in the model's own: its turn and scale, then offset;
// - its quad's half width, foot and top, and its pictures' cut;
// - its color;
// - (as words) its picture (a made model: its far picture's odd levels' color page), its normal
//   picture (the odd levels' normal page), its views (low half) and its views to a row (high half)
//   (a made model: its far picture's place in its pages), and its flags (bit 1: made on the
//   device);
// - (as words, a made model) its impostor's atlas row: the page's color and normal pictures (~0
//   while it has none), the row, and the rows a page holds;
// - (as words, a made model) its far picture's even levels' color and normal pages.
static const uint MODEL_ROWS = 10;
static const uint M_BOUNDS = 0;
static const uint M_SPHERE = 1;
static const uint M_FRAME = 2;
static const uint M_QUAD = 5;
static const uint M_COLOR = 6;
static const uint M_WORDS = 7;
static const uint M_ROW = 8;
static const uint M_EVEN = 9;
// A model made on the device.
static const uint MADE = 2;
// The most texels of an atlas row a pixel covers where a tree whose impostor has one reads it as
// the impostor does: farther, where the row's samples would spread too thin, it reads its far
// picture's smaller copies.
static const float ROW_TEXELS = 8.0;

struct CullPush
{
    uint constants;
    uint trees;
    uint models;
    // One bit a slot of trees: set for a block gone out of reach, or whose own objects stand in
    // the world.
    uint excluded;
    uint kept;
    // The draw's command, and where in it its instance count lies.
    uint args;
    uint count_at;
    // The world's vertex heights (~0 before they are read), SIDE a side, CELL meters apart.
    uint heights;
};

// The world's vertices: how many a side, and how far apart.
static const int SIDE = 2041;
static const float CELL = 24.0;
// Past how far a tree on the skyline starts to shrink, and by how far it has shrunk all it does,
// meters.
static const float2 SKYLINE = float2(600.0, 1800.0);
// How far past a tree the land is looked at, meters, doubling each time.
static const float SKYLINE_LOOK = 120.0;
static const uint SKYLINE_LOOKS = 6;

// The world's height at world `p` (meters), between its four nearest vertices.
float world_height(uint heights, float2 p)
{
    float2 g = p / CELL;
    int2 c = int2(floor(g));
    float2 f = g - float2(c);
    float h[4];
    for (uint k = 0; k < 4; k++)
    {
        int2 at = clamp(c + int2(k & 1, k >> 1), int2(0, 0), int2(SIDE - 1, SIDE - 1));
        h[k] = asfloat(buffers[NonUniformResourceIndex(heights)].Load(uint(at.y * SIDE + at.x) * 4));
    }
    return lerp(lerp(h[0], h[1], f.x), lerp(h[2], h[3], f.x), f.y);
}

// How much of its size a tree whose foot is at `foot` and top `top` meters above it keeps for
// standing on the skyline seen from `eye` (the frame's space, its origin at world `origin`): all of
// it while some land beyond it rises as high as its top, or while it stands near; `least` from far
// enough off.
float skyline(uint heights, float3 eye, float3 foot, float top, float2 origin, float least)
{
    float2 flat = foot.xy - eye.xy;
    float h = length(flat);
    if (heights == 0xFFFFFFFF || least >= 1.0 || h <= SKYLINE.x)
    {
        return 1.0;
    }
    float2 ahead = flat / h;
    float rises = (foot.z + top - eye.z) / h;
    float s = SKYLINE_LOOK;
    for (uint k = 0; k < SKYLINE_LOOKS; k++)
    {
        float far = h + s;
        float ground = world_height(heights, eye.xy + ahead * far + origin);
        if ((ground - eye.z) / far >= rises)
        {
            return 1.0;
        }
        s *= 2.0;
    }
    return lerp(1.0, least, smoothstep(SKYLINE.x, SKYLINE.y, h));
}

// How much of its size tree `i` keeps this frame: nought where it is not drawn.
float cull_keep(CullPush push, uint i)
{
    float4 eye = row(push.constants, C_EYE);
    float4 r0 = row(push.trees, i * 3);
    uint4 w = row_u(push.trees, i * 3 + 2);
    uint bits = buffers[NonUniformResourceIndex(push.excluded)].Load((w.z >> 5) * 4);
    if ((bits & (1u << (w.z & 31u))) != 0)
    {
        return 0.0;
    }
    float4 reach = row(push.constants, C_REACH);
    float3 foot = r0.xyz - float3(reach.xy, 0.0);
    float d = length(foot - eye.xyz);
    if (d > reach.z)
    {
        return 0.0;
    }
    uint4 words = row_u(push.models, w.x * MODEL_ROWS + M_WORDS);
    if (words.x == NO_PICTURES)
    {
        return 0.0;
    }
    float4 look = row(push.constants, C_LOOK);
    // The thinning: a tree goes once the share left standing falls below its own number, over a
    // short stretch.
    float left = saturate(1.0 - (d - reach.w) / max(reach.z - reach.w, 1.0));
    float own = float(w.y >> 8) / 16777216.0;
    float keep = left >= 1.0 ? 1.0 : saturate((left - own) / max(look.y, 1e-3) + 1.0);
    float4 bounds = row(push.models, w.x * MODEL_ROWS + M_BOUNDS);
    float radius = bounds.w * r0.w;
    // A tree smaller on the screen than the least height fades out.
    float pixels = 2.0 * radius * look.z / max(d, 1.0);
    keep *= smoothstep(look.w * 0.5, look.w, pixels);
    if (keep <= 0.0)
    {
        return 0.0;
    }
    // Its bounds, turned with it only about the upright: a tree is upright.
    float3 centre = foot + float3(0.0, 0.0, bounds.z * r0.w);
    for (uint p = 0; p < 6; p++)
    {
        float4 plane = row(push.constants, C_PLANES + p);
        if (dot(plane.xyz, centre) + plane.w < -radius)
        {
            return 0.0;
        }
    }
    return keep * skyline(push.heights, eye.xyz, foot, row(push.models, w.x * MODEL_ROWS + M_QUAD).z * r0.w,
                          reach.xy, row(push.constants, C_FAR).z);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_cull(uint3 id : SV_DispatchThreadID, uniform CullPush push)
{
    uint i = id.x;
    float4 eye = row(push.constants, C_EYE);
    if (i >= uint(eye.w))
    {
        return;
    }
    float keep = cull_keep(push, i);
    float4 r = row(push.trees, i * 3);
    float wider = 1.0;
    // Each tree keeps its own place in the list, whether it is drawn or not (nought: not), so the
    // trees are drawn in the same order every frame.
    RWByteAddressBuffer kept = rw_buffers[NonUniformResourceIndex(push.kept)];
    if (keep > 0.0)
    {
        float4 reach = row(push.constants, C_REACH);
        float d = length(r.xyz - float3(reach.xy, 0.0) - row(push.constants, C_EYE).xyz);
        float left = saturate(1.0 - (d - reach.w) / max(reach.z - reach.w, 1.0));
        wider = min(1.0 + row(push.constants, C_LOOK).x * (1.0 / max(left, 0.05) - 1.0), 1.5);
    }
    kept.Store(i * 12, i);
    kept.Store(i * 12 + 4, asuint(keep));
    kept.Store(i * 12 + 8, asuint(wider));
}

struct DrawPush
{
    uint frame;
    uint constants;
    uint trees;
    uint kept;
    uint models;
    uint clamp;
    // The weather's constants (~0 for none): a tree out in the rain is wet as its mesh is.
    uint weather;
};

struct Far
{
    float4 position : SV_Position;
    // Where on the quad, 0 to 1 across and down.
    float2 uv : TEXCOORD0;
    // The eye's bearing round the model's axis, in views.
    nointerpolation float bearing : TEXCOORD1;
    nointerpolation uint model : MODEL;
    // How many levels coarser its pictures are read.
    nointerpolation float coarser : TEXCOORD2;
    // The rows of its transform: its turn times its scale, and where its foot stands. A made
    // model's: the axes of the space its pictures are made in.
    nointerpolation float4 r0 : TEXCOORD3;
    nointerpolation float4 r1 : TEXCOORD4;
    nointerpolation float4 r2 : TEXCOORD5;
    float fog : TEXCOORD6;
};

[shader("vertex")]
Far vs_far(uint vertex : SV_VertexID, uint instance : SV_InstanceID, uniform DrawPush push)
{
    // Two triangles: corners (-1,-1) (1,-1) (1,1), (-1,-1) (1,1) (-1,1).
    static const float2 CORNERS[6] = {
        float2(-1.0, -1.0), float2(1.0, -1.0), float2(1.0, 1.0),
        float2(-1.0, -1.0), float2(1.0, 1.0), float2(-1.0, 1.0),
    };
    float2 c = CORNERS[vertex % 6];
    uint3 entry = buffers[NonUniformResourceIndex(push.kept)].Load3(instance * 12);
    float keep = asfloat(entry.y);
    float wider = asfloat(entry.z);
    if (keep <= 0.0)
    {
        // A tree the culling left out: its quad falls to one point, and nothing is drawn.
        Far none = (Far)0;
        none.position = float4(0.0, 0.0, -1.0, 1.0);
        return none;
    }
    float4 t = row(push.trees, entry.x * 3);
    float4 q = normalize(row(push.trees, entry.x * 3 + 1));
    uint model = row_u(push.trees, entry.x * 3 + 2).x;
    uint4 words = row_u(push.models, model * MODEL_ROWS + M_WORDS);
    float4 reach = row(push.constants, C_REACH);
    float4 far = row(push.constants, C_FAR);
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 foot = t.xyz - float3(reach.xy, 0.0);
    // A tree going is drawn smaller, down to nothing, rather than dithered away: nothing of it
    // changes from one frame to the next while the eye stands still.
    float s = t.w * keep;
    float3 a0 = float3(1.0 - 2.0 * (q.y * q.y + q.z * q.z), 2.0 * (q.x * q.y - q.w * q.z), 2.0 * (q.x * q.z + q.w * q.y)) * s;
    float3 a1 = float3(2.0 * (q.x * q.y + q.w * q.z), 1.0 - 2.0 * (q.x * q.x + q.z * q.z), 2.0 * (q.y * q.z - q.w * q.x)) * s;
    float3 a2 = float3(2.0 * (q.x * q.z - q.w * q.y), 2.0 * (q.y * q.z + q.w * q.x), 1.0 - 2.0 * (q.x * q.x + q.y * q.y)) * s;
    Far o;
    o.model = model;
    if ((words.w & MADE) != 0)
    {
        // Sunk a little past the blocks round the viewer, more the farther off it stands, where
        // the far ground's mesh is coarser; and read softer from twice as far as they reach.
        float d = length(foot - eye);
        float edge = max(far.w, 0.0);
        foot.z -= far.x * max(d - edge, 0.0);
        o.coarser = far.y * saturate((d - 2.0 * edge) / max(2.0 * edge, 1.0));
        // Where the pictures' space stands: the tree's turn and scale times the model's.
        float4 f0 = row(push.models, model * MODEL_ROWS + M_FRAME);
        float4 f1 = row(push.models, model * MODEL_ROWS + M_FRAME + 1);
        float4 f2 = row(push.models, model * MODEL_ROWS + M_FRAME + 2);
        float3 m0 = a0.x * f0.xyz + a0.y * f1.xyz + a0.z * f2.xyz;
        float3 m1 = a1.x * f0.xyz + a1.y * f1.xyz + a1.z * f2.xyz;
        float3 m2 = a2.x * f0.xyz + a2.y * f1.xyz + a2.z * f2.xyz;
        float3 offset = float3(f0.w, f1.w, f2.w);
        float3 ax = float3(m0.x, m1.x, m2.x);
        float3 ay = float3(m0.y, m1.y, m2.y);
        float3 az = float3(m0.z, m1.z, m2.z);
        float4 sphere = row(push.models, model * MODEL_ROWS + M_SPHERE);
        float3 centre = foot + float3(dot(a0, offset), dot(a1, offset), dot(a2, offset))
            + float3(dot(m0, sphere.xyz), dot(m1, sphere.xyz), dot(m2, sphere.xyz));
        float radius = sphere.w * max(length(ax), max(length(ay), length(az)));
        // The card as an impostor stands its own: round the sphere, turned to the eye about the
        // pictures' up, wider by the share the thinning asks.
        float3 e = eye - centre;
        float3 local = float3(dot(e, normalize(ax)), dot(e, normalize(ay)), dot(e, normalize(az)));
        float phi = atan2(local.y, local.x);
        float3 right = normalize(ax) * -sin(phi) + normalize(ay) * cos(phi);
        float3 up = normalize(az);
        float3 p = centre + (right * c.x * wider + up * c.y) * radius;
        o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
        o.uv = float2(0.5 + 0.5 * c.x, 0.5 - 0.5 * c.y);
        float views = phi / TAU * float(CARD_VIEWS);
        o.bearing = views < 0.0 ? views + float(CARD_VIEWS) : views;
        o.r0 = float4(ax, 0.0);
        o.r1 = float4(ay, 0.0);
        o.r2 = float4(az, 0.0);
        float4 v = transform(push.frame, FRAME_VIEW, float4(p, 1.0));
        o.fog = fog_factor(push.frame, length(v.xyz));
        return o;
    }
    float4 quad = row(push.models, model * MODEL_ROWS + M_QUAD);
    // Sunk a little, more the farther off it stands, where the far ground's mesh is coarser.
    foot.z -= far.x * length(foot - eye);
    // The eye in the model's own space, and its bearing round the model's axis.
    float3 d = eye - foot;
    float3 back = float3(dot(float3(a0.x, a1.x, a2.x), d), dot(float3(a0.y, a1.y, a2.y), d), dot(float3(a0.z, a1.z, a2.z), d));
    float bearing = atan2(back.y, back.x);
    // The quad's first axis goes across the eye's view, wider by the share the thinning asks; its
    // height stays up.
    float3 local = float3(-sin(bearing), cos(bearing), 0.0) * (c.x * quad.x * wider)
        + float3(0.0, 0.0, c.y > 0.0 ? quad.z : quad.y);
    float3 p = float3(dot(a0, local), dot(a1, local), dot(a2, local)) + foot;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    o.uv = float2(0.5 + 0.5 * c.x, 0.5 - 0.5 * c.y);
    float views = max(float(words.z & 0xFFFF), 1.0);
    float b = bearing / TAU * views;
    o.bearing = b - floor(b / views) * views;
    o.coarser = far.y;
    o.r0 = float4(a0, foot.x);
    o.r1 = float4(a1, foot.y);
    o.r2 = float4(a2, foot.z);
    float4 v = transform(push.frame, FRAME_VIEW, float4(p, 1.0));
    o.fog = fog_factor(push.frame, length(v.xyz));
    return o;
}

struct Targets
{
    float4 albedo : SV_Target0;
    float2 normal : SV_Target1;
    float4 light : SV_Target2;
    float4 material : SV_Target3;
    float depth : SV_Depth;
};

// What a far tree writes of `albedo` and the normal `N` at `depth`, lit as the sun lights what
// stands outdoors, a leaf `deep` in its crown (0 its outside, 1 its heart).
Targets far_targets(Far in, DrawPush push, float3 albedo, float3 N, float depth, float deep)
{
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    float3 lit = saturate(row(push.frame, FRAME_OBJECT_AMBIENT).rgb + sun_light(push.frame, N));
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_SUN, N, 0.0, float4(albedo, 1.0));
    o.normal = oct_encode(N);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : lit, in.fog);
    // The kind: foliage.
    o.material = material_bytes(CLASS_SUN, 1, uint(saturate(deep) * 255.0 + 0.5));
    o.depth = depth;
    return o;
}

// A tree whose pictures are made on the device, drawn as an impostor draws its card, at the
// card's own depth.
Targets fs_made(Far in, DrawPush push, uint4 words)
{
    float3 dither = card_dither(push.frame);
    uint view = card_view(in.bearing, in.position, dither);
    float texels = card_texels(in.uv);
    uint4 atlas = row_u(push.models, in.model * MODEL_ROWS + M_ROW);
    float3 albedo;
    float deep;
    float3 n;
    if (atlas.x != NO_PICTURES && texels <= ROW_TEXELS && in.coarser <= 0.0)
    {
        // Near enough that its impostor's atlas row is read, as the impostor reads it.
        float2 uv;
        float2 corner;
        float2 tile;
        float4 c = card_row_color(atlas.x, push.clamp, view, atlas.z, atlas.w, in.uv, uv, corner, tile);
        if (c.a < card_cut(texels, in.position, dither) || c.a <= 0.0)
        {
            discard;
        }
        float2 inset = tile * (0.5 / CARD_TILE);
        float4 baked = textures[NonUniformResourceIndex(atlas.y)].SampleLevel(
            samplers[NonUniformResourceIndex(push.clamp)],
            clamp(uv, corner + inset, corner + tile - inset), 0.0);
        n = baked.xyz * 2.0 - 1.0;
        deep = baked.w;
        albedo = c.rgb;
    }
    else
    {
        // Its far picture's smaller copies, at the level the pixel's footprint calls for.
        float4 nd;
        uint4 even = row_u(push.models, in.model * MODEL_ROWS + M_EVEN);
        float4 c = chain_color(uint4(words.xy, even.xy), push.clamp, words.z, view, in.uv,
                               log2(max(texels, 1.0)) + in.coarser, nd);
        if (c.a < card_cut(texels, in.position, dither) || c.a <= 0.0)
        {
            discard;
        }
        albedo = c.rgb / c.a;
        n = (nd.xyz / c.a) * 2.0 - 1.0;
        deep = nd.w / c.a;
    }
    float3 N = normalize(normalize(in.r0.xyz) * n.x + normalize(in.r1.xyz) * n.y + normalize(in.r2.xyz) * n.z);
    return far_targets(in, push, albedo, N, in.position.z, deep);
}

[shader("fragment")]
Targets fs_far(Far in, uniform DrawPush push)
{
    float4 quad = row(push.models, in.model * MODEL_ROWS + M_QUAD);
    float4 color = row(push.models, in.model * MODEL_ROWS + M_COLOR);
    uint4 words = row_u(push.models, in.model * MODEL_ROWS + M_WORDS);
    if ((words.w & MADE) != 0)
    {
        return fs_made(in, push, words);
    }
    // The view nearest the eye's bearing, whole: a blend of two by the frame's dither would change
    // from one frame to the next, and their leaves do not lie alike.
    float views = max(float(words.z & 0xFFFF), 1.0);
    float columns = max(float(words.z >> 16), 1.0);
    float rows = ceil(views / columns);
    float k = round(in.bearing);
    k -= floor(k / views) * views;
    float2 tile = float2(k - floor(k / columns) * columns, floor(k / columns));
    float2 grid = float2(columns, rows);
    float2 at = (tile + saturate(in.uv)) / grid;
    // The pictures are read at the level of detail the quad itself calls for, whichever view, or
    // coarser as asked. Their coverage is cut as it is, with no dither: its smaller levels hold
    // the share of each texel the model covers.
    float bias = pow(2.0, in.coarser);
    float2 gx = ddx(in.uv) / grid * bias;
    float2 gy = ddy(in.uv) / grid * bias;
    SamplerState wrap = samplers[NonUniformResourceIndex(push.clamp)];
    float4 base = textures[NonUniformResourceIndex(words.x)].SampleGrad(wrap, at, gx, gy) * color;
    if (base.a < quad.w)
    {
        discard;
    }
    float4 nd = textures[NonUniformResourceIndex(words.y)].SampleGrad(wrap, at, gx, gy);
    float3 normal = nd.xyz * 2.0 - 1.0;
    // Where the surface stands in the model's own space: across the view's picture, up its height,
    // and along the view by its depth.
    float theta = k / views * TAU;
    float3 across = float3(-sin(theta), cos(theta), 0.0);
    float3 inward = float3(-cos(theta), -sin(theta), 0.0);
    float3 local = across * ((in.uv.x * 2.0 - 1.0) * quad.x)
        + float3(0.0, 0.0, lerp(quad.z, quad.y, in.uv.y))
        + inward * ((nd.w - 0.5) * 2.0 * quad.x);
    float3 N = normalize(float3(dot(in.r0.xyz, normal), dot(in.r1.xyz, normal), dot(in.r2.xyz, normal)));
    // The depth of the surface itself, not the quad's, so it meets the ground where the model would.
    float4 q = float4(local, 1.0);
    float3 world = float3(dot(in.r0, q), dot(in.r1, q), dot(in.r2, q));
    float4 clip = transform(push.frame, FRAME_VIEW_PROJ, float4(world, 1.0));
    // A far tree of an art pack's own pictures shows no place in its crown: a leaf taken as
    // halfway in.
    return far_targets(in, push, base.rgb, N, saturate(clip.z / clip.w), 0.5);
}
