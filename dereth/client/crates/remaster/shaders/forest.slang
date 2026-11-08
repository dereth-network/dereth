// The far trees: every tree an art pack has far pictures for, out to the reach, culled here each
// frame and drawn into the G-buffer as a quad that stands upright and turns to the eye, showing the
// pack's picture of the view nearest the eye's bearing.
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
                                 // on the skyline keeps

// A tree, three rows: its foot and scale; its rotation; and (as words) its model, number and slot.
// A model, four rows: the centre of its bounds and their radius; its quad's half width, foot and
// top, and its pictures' cut; its color; and (as words) its picture, its normal picture, its views
// (low half) and its views to a row (high half), and its flags (bit 0: an art pack's leaves, pulled
// toward the look's foliage hue).

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
    float4 r0 = row(push.trees, i * 3);
    uint4 w = row_u(push.trees, i * 3 + 2);
    uint bits = buffers[NonUniformResourceIndex(push.excluded)].Load((w.z >> 5) * 4);
    if ((bits & (1u << (w.z & 31u))) != 0)
    {
        return;
    }
    float4 reach = row(push.constants, C_REACH);
    float3 foot = r0.xyz - float3(reach.xy, 0.0);
    float d = length(foot - eye.xyz);
    if (d > reach.z)
    {
        return;
    }
    uint4 words = row_u(push.models, w.x * 4 + 3);
    if (words.x == NO_PICTURES)
    {
        return;
    }
    float4 look = row(push.constants, C_LOOK);
    // The thinning: a tree goes once the share left standing falls below its own number, over a
    // short stretch.
    float left = saturate(1.0 - (d - reach.w) / max(reach.z - reach.w, 1.0));
    float own = float(w.y >> 8) / 16777216.0;
    float keep = left >= 1.0 ? 1.0 : saturate((left - own) / max(look.y, 1e-3) + 1.0);
    float4 bounds = row(push.models, w.x * 4);
    float radius = bounds.w * r0.w;
    // A tree smaller on the screen than the least height fades out.
    float pixels = 2.0 * radius * look.z / max(d, 1.0);
    keep *= smoothstep(look.w * 0.5, look.w, pixels);
    if (keep <= 0.0)
    {
        return;
    }
    // Its bounds, turned with it only about the upright: a tree is upright.
    float3 centre = foot + float3(0.0, 0.0, bounds.z * r0.w);
    for (uint p = 0; p < 6; p++)
    {
        float4 plane = row(push.constants, C_PLANES + p);
        if (dot(plane.xyz, centre) + plane.w < -radius)
        {
            return;
        }
    }
    float wider = min(1.0 + look.x * (1.0 / max(left, 0.05) - 1.0), 1.5);
    keep *= skyline(push.heights, eye.xyz, foot, row(push.models, w.x * 4 + 1).z * r0.w, reach.xy,
                    row(push.constants, C_FAR).z);
    uint slot;
    rw_buffers[NonUniformResourceIndex(push.args)].InterlockedAdd(push.count_at, 1, slot);
    RWByteAddressBuffer kept = rw_buffers[NonUniformResourceIndex(push.kept)];
    kept.Store(slot * 12, i);
    kept.Store(slot * 12 + 4, asuint(keep));
    kept.Store(slot * 12 + 8, asuint(wider));
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
    // The rows of its transform: its turn times its scale, and where its foot stands.
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
    float4 t = row(push.trees, entry.x * 3);
    float4 q = normalize(row(push.trees, entry.x * 3 + 1));
    uint model = row_u(push.trees, entry.x * 3 + 2).x;
    float4 quad = row(push.models, model * 4 + 1);
    uint4 words = row_u(push.models, model * 4 + 3);
    float4 reach = row(push.constants, C_REACH);
    float4 far = row(push.constants, C_FAR);
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    float3 foot = t.xyz - float3(reach.xy, 0.0);
    // Sunk a little, more the farther off it stands, where the far ground's mesh is coarser.
    foot.z -= far.x * length(foot - eye);
    // A tree going is drawn smaller, down to nothing, rather than dithered away: nothing of it
    // changes from one frame to the next while the eye stands still.
    float s = t.w * keep;
    float3 a0 = float3(1.0 - 2.0 * (q.y * q.y + q.z * q.z), 2.0 * (q.x * q.y - q.w * q.z), 2.0 * (q.x * q.z + q.w * q.y)) * s;
    float3 a1 = float3(2.0 * (q.x * q.y + q.w * q.z), 1.0 - 2.0 * (q.x * q.x + q.z * q.z), 2.0 * (q.y * q.z - q.w * q.x)) * s;
    float3 a2 = float3(2.0 * (q.x * q.z - q.w * q.y), 2.0 * (q.y * q.z + q.w * q.x), 1.0 - 2.0 * (q.x * q.x + q.y * q.y)) * s;
    // The eye in the model's own space, and its bearing round the model's axis.
    float3 d = eye - foot;
    float3 back = float3(dot(float3(a0.x, a1.x, a2.x), d), dot(float3(a0.y, a1.y, a2.y), d), dot(float3(a0.z, a1.z, a2.z), d));
    float bearing = atan2(back.y, back.x);
    // The quad's first axis goes across the eye's view, wider by the share the thinning asks; its
    // height stays up.
    float3 local = float3(-sin(bearing), cos(bearing), 0.0) * (c.x * quad.x * wider)
        + float3(0.0, 0.0, c.y > 0.0 ? quad.z : quad.y);
    float3 p = float3(dot(a0, local), dot(a1, local), dot(a2, local)) + foot;
    Far o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    o.uv = float2(0.5 + 0.5 * c.x, 0.5 - 0.5 * c.y);
    float views = max(float(words.z & 0xFFFF), 1.0);
    float b = bearing / TAU * views;
    o.bearing = b - floor(b / views) * views;
    o.model = model;
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

[shader("fragment")]
Targets fs_far(Far in, uniform DrawPush push)
{
    float4 quad = row(push.models, in.model * 4 + 1);
    float4 color = row(push.models, in.model * 4 + 2);
    uint4 words = row_u(push.models, in.model * 4 + 3);
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
    if ((words.w & 1) != 0)
    {
        base.rgb = pack_foliage(push.frame, base.rgb);
    }
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
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    float3 lit = saturate(row(push.frame, FRAME_OBJECT_AMBIENT).rgb + sun_light(push.frame, N));
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_SUN, N, 0.0, float4(base.rgb, 1.0));
    o.normal = oct_encode(N);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : lit, in.fog);
    // The kind: foliage.
    // A far tree shows no place in its crown: a leaf taken as halfway in.
    o.material = material_bytes(CLASS_SUN, 1, 128);
    o.depth = saturate(clip.z / clip.w);
    return o;
}
