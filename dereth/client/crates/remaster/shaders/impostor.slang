// Far trees as impostors: each tree's full mesh drawn once from VIEWS directions round it into a
// row of an atlas page (its own color multiplied by its coverage, the coverage, and its normals
// with how deep in its crown each leaf lies), then drawn far off as a card that stands upright and
// turns to the eye, showing the views on either side of the eye's direction through
// complementary halves of a screen pattern.
//
// - `vs_bake`/`fs_bake` draw one view of a mesh, looked at level from the side, the mesh's sphere
//   filling the view, finer than the row keeps it.
// - `fs_gather` makes a row's texels from the views drawn finer: each the share of its square of
//   pixels the mesh covers, so a crown's needles keep their coverage instead of thinning to
//   blots with holes between.
// - `vs_draw`/`fs_draw` draw the cards into the G-buffer, lit as the sun lights what stands
//   outdoors.
// - `fs_chain` makes a far picture's smaller copies from a row of views, for the far forest.
import bindless;
import card;
import lod;
import retail;
import weather;

static const uint VIEWS = CARD_VIEWS;
static const float TAU = 6.2831853;

// A view's direction from the mesh's centre toward the eye, in the mesh's own space, at angle
// `phi` round its z axis.
float3 toward(float phi)
{
    return float3(cos(phi), sin(phi), 0.0);
}

// The direction across a view from left to right, at angle `phi`.
float3 across(float phi)
{
    return float3(-sin(phi), cos(phi), 0.0);
}

struct BakePush
{
    // The draw items, or ~0 where the draw's mesh is named below instead.
    uint items;
    uint vertices;
    uint materials;
    uint wrap;
    uint clamp;
    // Which of the views.
    uint view;
    // The sphere pictured, in its own space.
    float centre_x;
    float centre_y;
    float centre_z;
    float radius;
    // Where the drawn mesh stands in the space pictured: the rows of its transform, each its turn
    // and scale and then its offset.
    float f00;
    float f01;
    float f02;
    float f03;
    float f10;
    float f11;
    float f12;
    float f13;
    float f20;
    float f21;
    float f22;
    float f23;
    // Without draw items, the mesh's first vertex, its material and its flags.
    uint first_vertex;
    uint material;
    uint flags;
};

struct Baked
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float3 normal : TEXCOORD1;
    float4 color : TEXCOORD2;
    // How deep in its crown the vertex lies, 0 at its outside and 1 at its heart, as an art pack's
    // leaves carry it.
    float deep : TEXCOORD3;
    nointerpolation uint material : MATERIAL;
    nointerpolation uint flags : FLAGS;
};

[shader("vertex")]
Baked vs_bake(uint vertex : SV_VertexID,
#ifndef DERETH_DXIL
    uint item : SV_StartInstanceLocation,
#endif
    uniform BakePush push)
{
#ifdef DERETH_DXIL
    uint item = draw_item.value;
#endif
    uint first = push.first_vertex;
    uint material = push.material;
    uint flags = push.flags;
    if (push.items != 0xFFFFFFFF)
    {
        uint4 a = row_u(push.items, item * 2);
        first = a.y;
        material = a.w;
        flags = row_u(push.items, item * 2 + 1).x;
    }
    uint v = first + vertex;
    // A vertex's rows: its position and the normal's x; the normal's y and z and its texture
    // coordinates; its tangent; its color.
    float4 r0 = row(push.vertices, v * 4);
    float4 r1 = row(push.vertices, v * 4 + 1);
    float3 f0 = float3(push.f00, push.f01, push.f02);
    float3 f1 = float3(push.f10, push.f11, push.f12);
    float3 f2 = float3(push.f20, push.f21, push.f22);
    float3 placed = float3(dot(f0, r0.xyz) + push.f03, dot(f1, r0.xyz) + push.f13, dot(f2, r0.xyz) + push.f23);
    float3 n = float3(r0.w, r1.x, r1.y);
    float3 p = placed - float3(push.centre_x, push.centre_y, push.centre_z);
    float phi = TAU * float(push.view) / float(VIEWS);
    float r = max(push.radius, 1e-3);
    Baked o;
    o.position = float4(dot(p, across(phi)) / r, p.z / r, 0.5 - 0.5 * dot(p, toward(phi)) / r, 1.0);
    o.uv = r1.zw;
    o.normal = float3(dot(f0, n), dot(f1, n), dot(f2, n));
    o.color = row(push.vertices, v * 4 + 3);
    // An art pack's growing model carries how far the wind bends a vertex, whole millimeters, in
    // its tangent's last value's length, above how deep in the crown it lies.
    float wa = abs(row(push.vertices, v * 4 + 2).w);
    o.deep = clamp(wa - 1.0 - 2.0 * floor((wa - 1.0) * 0.5 + 1e-3), 0.0, 1.0);
    o.material = material;
    o.flags = flags;
    return o;
}

struct BakeTargets
{
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
};

[shader("fragment")]
BakeTargets fs_bake(Baked in, uniform BakePush push)
{
    uint flags = in.flags;
    uint4 m = row_u(push.materials, in.material * 2);
    float4 t = float4(1.0, 1.0, 1.0, 1.0);
    if ((m.y & 64) != 0)
    {
        t = sample(m.x, (flags & 1) != 0 ? push.wrap : push.clamp, in.uv);
    }
    // A surface drawn under its vertices' colors.
    if ((m.y & 512) != 0)
    {
        t *= in.color;
    }
    // An art pack's leaves, which the card pulls toward the look's foliage hue as it is drawn, and
    // whose depth in their crown the card's light reads.
    bool leaf = (m.y & 1024) != 0;
    float alpha = t.a * float(m.w) / 255.0;
    // Kept where the surface keeps its texel: at or above its alpha reference where it has one,
    // at or above half where it is otherwise see-through.
    float reference = (m.y & 1) != 0 ? float(m.z) / 255.0 : 0.5;
    if (alpha + 1e-6 < reference)
    {
        discard;
    }
    BakeTargets o;
    o.albedo = float4(t.rgb, 1.0);
    o.normal = float4(normalize(in.normal) * 0.5 + 0.5, leaf ? in.deep : 0.5);
    return o;
}

struct DrawPush
{
    uint frame;
    // The cards, five rows each: the centre (frame's space) and the radius; the mesh's x axis and
    // the fade into the card; its y axis and the material's kind; its z axis; and (as words) the
    // atlas page's color and normal pictures, the row in the page and the rows in a page.
    uint cards;
    uint wrap;
    // The weather's constants (~0 for none): a card out in the rain is wet as its mesh is.
    uint weather;
};

struct Card
{
    float4 position : SV_Position;
    // Where on the card, 0 to 1 across and down.
    float2 at : TEXCOORD0;
    // The eye's angle round the mesh's z axis, in views.
    nointerpolation float view : TEXCOORD1;
    nointerpolation uint card : CARD;
    float fog : TEXCOORD2;
};

[shader("vertex")]
Card vs_draw(uint vertex : SV_VertexID, uint card : SV_InstanceID, uniform DrawPush push)
{
    // Two triangles: corners (-1,-1) (1,-1) (1,1), (-1,-1) (1,1) (-1,1).
    static const float2 CORNERS[6] = {
        float2(-1.0, -1.0), float2(1.0, -1.0), float2(1.0, 1.0),
        float2(-1.0, -1.0), float2(1.0, 1.0), float2(-1.0, 1.0),
    };
    float2 c = CORNERS[vertex % 6];
    float4 r0 = row(push.cards, card * 5);
    float3 ax = row(push.cards, card * 5 + 1).xyz;
    float3 ay = row(push.cards, card * 5 + 2).xyz;
    float3 az = row(push.cards, card * 5 + 3).xyz;
    float3 centre = r0.xyz;
    float radius = r0.w;
    float3 eye = row(push.frame, FRAME_EYE).xyz;
    // The eye in the mesh's own space, and its angle round the mesh's z axis.
    float3 e = eye - centre;
    float3 local = float3(dot(e, normalize(ax)), dot(e, normalize(ay)), dot(e, normalize(az)));
    float phi = atan2(local.y, local.x);
    float3 side = across(phi);
    float3 right = normalize(ax) * side.x + normalize(ay) * side.y;
    float3 up = normalize(az);
    float3 p = centre + (right * c.x + up * c.y) * radius;
    Card o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    o.at = float2(0.5 + 0.5 * c.x, 0.5 - 0.5 * c.y);
    float views = phi / TAU * float(VIEWS);
    o.view = views < 0.0 ? views + float(VIEWS) : views;
    o.card = card;
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
};

[shader("fragment")]
Targets fs_draw(Card in, uniform DrawPush push)
{
    float t = fade_threshold(int2(in.position.xy));
    float4 r1 = row(push.cards, in.card * 5 + 1);
    float4 r2 = row(push.cards, in.card * 5 + 2);
    uint4 w = row_u(push.cards, in.card * 5 + 4);
    // The card keeps its share of the fade from the mesh it takes over from.
    if (t >= r1.w)
    {
        discard;
    }
    float3 dither = card_dither(push.frame);
    uint view = card_view(in.view, in.position, dither);
    float2 uv;
    float2 corner;
    float2 tile;
    float4 albedo = card_row_color(w.x, push.wrap, view, w.z, w.w, in.at, uv, corner, tile);
    if (albedo.a < card_cut(card_texels(in.at), in.position, dither) || albedo.a <= 0.0)
    {
        discard;
    }
    float2 inset = tile * (0.5 / CARD_TILE);
    float4 baked = textures[NonUniformResourceIndex(w.y)].SampleLevel(
        samplers[NonUniformResourceIndex(push.wrap)],
        clamp(uv, corner + inset, corner + tile - inset), 0.0);
    float3 n = baked.xyz * 2.0 - 1.0;
    float3 ax = normalize(r1.xyz);
    float3 ay = normalize(r2.xyz);
    float3 az = normalize(row(push.cards, in.card * 5 + 3).xyz);
    float3 N = normalize(ax * n.x + ay * n.y + az * n.z);
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;
    float3 lit = saturate(row(push.frame, FRAME_OBJECT_AMBIENT).rgb + sun_light(push.frame, N));
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_SUN, N, 0.0, float4(albedo.rgb, 1.0));
    o.normal = oct_encode(N);
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : lit, in.fog);
    // A leaf as deep in its crown as the picture shows it.
    uint kind = uint(r2.w + 0.5);
    uint deep = kind == 1 ? uint(saturate(baked.w) * 255.0 + 0.5) : 0;
    o.material = material_bytes(CLASS_SUN, kind, deep | SET_PICTURED);
    return o;
}

struct GatherPush
{
    // The views drawn finer: their color and coverage, and their normals and depth in the crown.
    uint color;
    uint normal;
    // The pixels each way a texel is gathered from.
    uint fine;
    // The corner in the target of the texels made.
    uint x;
    uint y;
};

struct ChainTargets
{
    float4 color : SV_Target0;
    float4 normal : SV_Target1;
};

// One texel of a row of views, from the square of `fine` by `fine` pixels drawn for it: its color
// multiplied by the share of them the mesh covers, that share, and the normal and depth in the
// crown of what it covers (none, where it covers nothing).
[shader("fragment")]
ChainTargets fs_gather(float4 position : SV_Position, uniform GatherPush push)
{
    int fine = int(push.fine);
    int2 from = (int2(position.xy) - int2(int(push.x), int(push.y))) * fine;
    float4 c = float4(0.0, 0.0, 0.0, 0.0);
    float4 m = float4(0.0, 0.0, 0.0, 0.0);
    for (int j = 0; j < fine; j++)
    {
        for (int i = 0; i < fine; i++)
        {
            int3 s = int3(from + int2(i, j), 0);
            float4 a = textures[NonUniformResourceIndex(push.color)].Load(s);
            float4 b = textures[NonUniformResourceIndex(push.normal)].Load(s);
            c += a;
            m += float4(b.xyz * 2.0 - 1.0, b.w) * a.a;
        }
    }
    ChainTargets o;
    o.color = c / float(fine * fine);
    o.normal = c.a > 0.0 && dot(m.xyz, m.xyz) > 0.0
        ? float4(normalize(m.xyz) * 0.5 + 0.5, m.w / c.a)
        : float4(0.5, 0.5, 1.0, 0.0);
    return o;
}

struct ChainPush
{
    // The level before the one made: its color and coverage, and its normals and leaf mark; and
    // its corner, texels. The first level is made from a row of views at full size, whose colors
    // the coverage multiplies and whose normals it does not.
    uint color;
    uint normal;
    uint from_x;
    uint from_y;
    uint first;
    // The corner in the target of the level made, texels.
    uint x;
    uint y;
};

// One texel of a far picture's level: the mean of the four texels of the level before that it
// covers, the color, the normal and the leaf mark each multiplied by the coverage, so a smaller
// copy bleeds nothing of what is cut away into a crown's edge.
[shader("fragment")]
ChainTargets fs_chain(float4 position : SV_Position, uniform ChainPush push)
{
    int2 at = int2(position.xy) - int2(int(push.x), int(push.y));
    int2 from = int2(int(push.from_x), int(push.from_y)) + at * 2;
    float4 c = float4(0.0, 0.0, 0.0, 0.0);
    float4 m = float4(0.0, 0.0, 0.0, 0.0);
    for (int k = 0; k < 4; k++)
    {
        int3 s = int3(from + int2(k & 1, k >> 1), 0);
        float4 a = textures[NonUniformResourceIndex(push.color)].Load(s);
        float4 b = textures[NonUniformResourceIndex(push.normal)].Load(s);
        c += a;
        m += push.first != 0 ? b * a.a : b;
    }
    ChainTargets o;
    o.color = c * 0.25;
    o.normal = m * 0.25;
    return o;
}
