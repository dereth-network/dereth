// Far trees as impostors: each tree's full mesh drawn once from VIEWS directions round it into a
// row of an atlas page (its own color and coverage, and its normals), then drawn far off as a
// card that stands upright and turns to the eye, showing the views on either side of the eye's
// direction through complementary halves of the fixed fade pattern.
//
// - `vs_bake`/`fs_bake` draw one view of a mesh, looked at level from the side, the mesh's sphere
//   filling the view.
// - `vs_draw`/`fs_draw` draw the cards into the G-buffer, lit as the sun lights what stands
//   outdoors.
import bindless;
import lod;
import retail;
import weather;

static const uint VIEWS = 16;
// A picture's side, pixels.
static const float TILE = 128.0;
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
    uint items;
    uint vertices;
    uint materials;
    uint wrap;
    uint clamp;
    // Which of the views.
    uint view;
    // The mesh's sphere, in its own space.
    float centre_x;
    float centre_y;
    float centre_z;
    float radius;
};

struct Baked
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float3 normal : TEXCOORD1;
    float4 color : TEXCOORD2;
    nointerpolation uint item : ITEM;
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
    uint4 a = row_u(push.items, item * 2);
    uint v = a.y + vertex;
    // A vertex's rows: its position and the normal's x; the normal's y and z and its texture
    // coordinates; its tangent; its color.
    float4 r0 = row(push.vertices, v * 4);
    float4 r1 = row(push.vertices, v * 4 + 1);
    float3 p = r0.xyz - float3(push.centre_x, push.centre_y, push.centre_z);
    float phi = TAU * float(push.view) / float(VIEWS);
    float r = max(push.radius, 1e-3);
    Baked o;
    o.position = float4(dot(p, across(phi)) / r, p.z / r, 0.5 - 0.5 * dot(p, toward(phi)) / r, 1.0);
    o.uv = r1.zw;
    o.normal = float3(r0.w, r1.x, r1.y);
    o.color = row(push.vertices, v * 4 + 3);
    o.item = item;
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
    uint4 a = row_u(push.items, in.item * 2);
    uint flags = row_u(push.items, in.item * 2 + 1).x;
    uint4 m = row_u(push.materials, a.w * 2);
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
    o.normal = float4(normalize(in.normal) * 0.5 + 0.5, 1.0);
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
    // The two views either side of the eye, the nearer shown on more of the pattern.
    uint v0 = uint(floor(in.view)) % VIEWS;
    uint v1 = (v0 + 1) % VIEWS;
    float between = in.view - floor(in.view);
    // A second threshold from the pattern turned a quarter, so the view chosen does not follow
    // the fade.
    float t2 = fade_threshold(int2(int(in.position.y), int(in.position.x)) + int2(3, 5));
    uint view = t2 < between ? v1 : v0;
    float2 tile = float2(1.0 / float(VIEWS), 1.0 / float(w.w));
    float2 corner = float2(float(view), float(w.z)) * tile;
    float2 uv = corner + in.at * tile;
    // The pictures have no smaller copies: four samples across the pixel's footprint stand in
    // for one, so a far card does not sparkle. The footprint is measured on the card, since
    // neighboring pixels may show different views, and the samples stay in this view's own picture.
    float2 dx = ddx(in.at) * tile * 0.25;
    float2 dy = ddy(in.at) * tile * 0.25;
    float2 inset = tile * (0.5 / TILE);
    float4 albedo = float4(0.0, 0.0, 0.0, 0.0);
    for (int k = 0; k < 4; k++)
    {
        float2 o = (k & 1) != 0 ? dx : -dx;
        o += (k & 2) != 0 ? dy : -dy;
        float4 c = textures[NonUniformResourceIndex(w.x)].SampleLevel(
            samplers[NonUniformResourceIndex(push.wrap)],
            clamp(uv + o, corner + inset, corner + tile - inset), 0.0);
        albedo += float4(c.rgb * c.a, c.a);
    }
    // The color of what the samples cover, and how much of the pixel they cover.
    albedo = float4(albedo.rgb / max(albedo.a, 1e-4), albedo.a * 0.25);
    if (albedo.a < 0.5)
    {
        discard;
    }
    float3 n = textures[NonUniformResourceIndex(w.y)].SampleLevel(
        samplers[NonUniformResourceIndex(push.wrap)],
        clamp(uv, corner + inset, corner + tile - inset), 0.0).xyz * 2.0 - 1.0;
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
    // A picture shows no place in its crown: a leaf taken as halfway in.
    uint kind = uint(r2.w + 0.5);
    o.material = material_bytes(CLASS_SUN, kind, kind == 1 ? 128 : 0);
    return o;
}
