// The G-buffer pass for meshes: statics, objects and rooms, each drawn as a draw item (an
// instance and one surface's triangles) named by its indirect command's first instance.
//
// The light is worked out per vertex, from the item's class: the sun alone for what stands
// outdoors, the item's own set of at most eight lights indoors, and for a room's walls the static
// lights burned in plus every dynamic light. With the light worked out per vertex
// (`FRAME_SCREEN.y` 0) the light target holds it, interpolated as retail interpolates it; with
// the light worked out per pixel (1) it holds what the resolve cannot know: the surface's own
// glow.
import bindless;
import intent;
import lift;
import lod;
import retail;
import weather;

struct Push
{
    uint frame;
    uint items;
    uint instances;
    uint vertices;
    uint materials;
    uint lights;
    uint sets;
    uint blocks;
    uint overrides;
    // The samplers for a texture that repeats and one that is held at its edges.
    uint wrap;
    uint clamp;
    // The light set every room's walls take: every dynamic light.
    uint room_set;
    // 1 to work the light out per vertex whatever the frame's mode: the forward pass.
    uint lit_always;
    // The detail textures, by their index in the table (~0 for none), and their tilings: the
    // buildings' and the rooms'.
    uint building_detail;
    float building_tiling;
    uint room_detail;
    float room_tiling;
    // For the outdoor surfaces drawn over the frame when the viewer stands inside, the material
    // target, whose interior pixels they leave alone (~0 for none).
    uint interior_mask;
    // The materials' maps: per entry the normal map, the roughness map (each ~0 for none) and the
    // normal's strength.
    uint maps;
    // The weather's constants (~0 for none): what stands out in the rain is drawn wet.
    uint weather;
};

// A vertex's rows: its position and normal, its texture coordinates, then its tangent and, in
// its fourth, the way its second texture coordinate runs from it, then its color.
static const uint VERTEX_ROWS = 4;

// The material flag of a surface drawn under its vertices' colors, and of an art pack's leaves.
static const uint VERTEX_COLORED = 512;
static const uint PACK_FOLIAGE = 1024;

// The item flags: the texture repeats; a building's shell, under the building detail texture; a
// room's walls, under the rooms' detail texture.
static const uint ITEM_TILED = 1;
static const uint ITEM_BUILDING = 2;
static const uint ITEM_ROOM = 4;

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct Varyings
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    // The light (rgb) and the fog factor (a).
    float4 light : TEXCOORD1;
    float3 normal : TEXCOORD2;
    // The vertex's alpha: the surface's, or the object's own translucency.
    float alpha : TEXCOORD3;
    // The tangent where the frame is, and in `w` the way the second texture coordinate runs.
    float4 tangent : TEXCOORD4;
    // The vertex's color, which a surface flagged for it is drawn under.
    float4 color : TEXCOORD5;
    nointerpolation uint item : ITEM;
};

// An item, two rows: its instance, its vertices' first index, its index count and its material;
// then its flags (bit 0 the texture repeats), its first index and its bucket. Its instance says
// the rest: its light class, its light set and its object's override (or ~0).
struct Item
{
    uint instance;
    uint vertex_base;
    uint material;
    uint flags;
    uint override_;
    uint set;
    uint class_;
};

Item item_at(uint i)
{
    uint4 a = row_u(push.items, i * 2);
    uint4 b = row_u(push.items, i * 2 + 1);
    uint4 info = row_u(push.instances, a.x * INSTANCE_ROWS + 5);
    Item it;
    it.instance = a.x;
    it.vertex_base = a.y;
    it.material = a.w;
    it.flags = b.x;
    it.class_ = info.y;
    it.set = info.z;
    it.override_ = info.w;
    return it;
}

[shader("vertex")]
Varyings vs_main(uint vertex : SV_VertexID
#ifndef DERETH_DXIL
    , uint item : SV_StartInstanceLocation
#endif
)
{
#ifdef DERETH_DXIL
    uint item = draw_item.value;
#endif
    Item it = item_at(item);
    uint v = it.vertex_base + vertex;
    float4 r0 = row(push.vertices, v * VERTEX_ROWS);
    float4 r1 = row(push.vertices, v * VERTEX_ROWS + 1);
    float4 r2 = row(push.vertices, v * VERTEX_ROWS + 2);
    float3 position = r0.xyz;
    float3 normal = float3(r0.w, r1.x, r1.y);
    float2 uv = r1.zw;

    // The instance: its transform's four columns, its per-axis scale's inverse square, and the
    // block it stands in (~0 for one placed in the frame's space directly).
    uint at = it.instance * INSTANCE_ROWS;
    float4 p = transform(push.instances, at, float4(position, 1.0));
    float4 scale = row(push.instances, at + 4);
    uint4 info = row_u(push.instances, at + 5);
    if (info.x != 0xFFFFFFFF)
    {
        p.xyz += row(push.blocks, info.x).xyz;
    }
    // Drawn standing on the ground as it is drawn.
    float3 origin = instance_origin(push.instances, push.blocks, it.instance);
    p.z += lifted(push.frame, push.instances, it.instance, origin);
    float3 nrm = scale.w > 0.5 ? normal * scale.xyz : normal;
    float3 N = normalize((row(push.instances, at) * nrm.x + row(push.instances, at + 1) * nrm.y
                          + row(push.instances, at + 2) * nrm.z).xyz);
    // A tangent runs along the surface, so it turns with the transform as it is.
    float3 T = (row(push.instances, at) * r2.x + row(push.instances, at + 1) * r2.y
                + row(push.instances, at + 2) * r2.z).xyz;

    uint4 m = row_u(push.materials, it.material * 2);
    float4 mf = row(push.materials, it.material * 2 + 1);
    float luminosity = mf.x;
    float4 o = it.override_ != 0xFFFFFFFF ? row(push.overrides, it.override_) : float4(0.0, 0.0, 0.0, 0.0);
    float bound = o.w;

    float3 sum = float3(0.0, 0.0, 0.0);
    float3 emissive;
    if (it.class_ == CLASS_CELL)
    {
        emissive = burn(push.lights, uint(row(push.frame, FRAME_SCREEN).w), p.xyz, N);
        sum = ff_lights(push.lights, push.sets, push.room_set, p.xyz, N);
    }
    else
    {
        float e = luminosity > 0.0 ? luminosity : o.x;
        emissive = float3(e, e, e);
        if (it.class_ == CLASS_INDOOR)
        {
            sum = ff_lights(push.lights, push.sets, it.set, p.xyz, N);
        }
        else if (it.class_ == CLASS_SUN || it.class_ == CLASS_SKY)
        {
            sum = sun_light(push.frame, N);
        }
    }
    float md = bound > 0.5 ? o.y : 1.0;
    float3 ambient = row(push.frame, FRAME_OBJECT_AMBIENT).rgb;
    float3 lit = saturate(emissive + ambient + md * sum);

    float4 eye = transform(push.frame, FRAME_VIEW, float4(p.xyz, 1.0));
    // The sky is fogged only while retail fogs the whole of it (its set carries that); every
    // other surface as its material says.
    bool fogged = it.class_ == CLASS_SKY ? (it.set & 1) != 0 : (m.y & 8) != 0;
    float fog = fogged ? fog_factor(push.frame, length(eye.xyz)) : 1.0;

    Varyings out;
    out.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p.xyz, 1.0));
    out.uv = uv + row(push.instances, at + 6).xy;
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5 && push.lit_always == 0;
    out.light = float4(per_pixel ? emissive : lit, fog);
    out.normal = N;
    // A bound material's translucency replaces the surface's own alpha only when it is not the
    // default: a material that changes only the light leaves a translucent surface's alpha be.
    out.alpha = bound > 0.5 && o.z != 0.0 ? 1.0 - o.z : float(m.w) / 255.0;
    out.tangent = float4(T, r2.w);
    out.color = row(push.vertices, v * VERTEX_ROWS + 3);
    out.item = item;
    return out;
}

// The surface's own color at `uv`: its texture, or white where it has none, under its detail
// texture where it takes one: the detail's color plus its inverse alpha, faded out as the
// vertex's alpha does.
float4 texel(Item it, uint4 m, float2 uv, float alpha)
{
    float4 t = float4(1.0, 1.0, 1.0, 1.0);
    if ((m.y & 64) != 0)
    {
        uint s = (it.flags & ITEM_TILED) != 0 ? push.wrap : push.clamp;
        t = sample(m.x, s, uv);
    }
    uint detail = 0xFFFFFFFF;
    float tiling = 1.0;
    if ((it.flags & ITEM_BUILDING) != 0)
    {
        detail = push.building_detail;
        tiling = push.building_tiling;
    }
    else if ((it.flags & ITEM_ROOM) != 0)
    {
        detail = push.room_detail;
        tiling = push.room_tiling;
    }
    if (detail != 0xFFFFFFFF)
    {
        float4 d = sample(detail, push.wrap, uv * tiling);
        t.rgb *= lerp(float3(1.0, 1.0, 1.0), d.rgb + (1.0 - d.a), alpha);
    }
    return t;
}

// Leave the pixel alone where the interior is drawn, for an outdoor surface drawn when the
// viewer stands inside: the interior covers the outdoors there.
void masked(float4 position)
{
    if (push.interior_mask == 0xFFFFFFFF)
    {
        return;
    }
    float4 m = textures[NonUniformResourceIndex(push.interior_mask)].Load(int3(int2(position.xy), 0));
    uint cls = uint(m.x * 255.0 + 0.5);
    if (cls == CLASS_INDOOR || cls == CLASS_CELL)
    {
        discard;
    }
}

// Whether a blended cut-out keeps the pixel in the G-buffer: only where it is fully opaque; the
// edge pass blends the rest over what is behind, as retail blends it.
bool partial(uint4 m, float a)
{
    return (m.y & 1) != 0 && (m.y & 32) != 0 && a < 254.5 / 255.0;
}

struct Targets
{
    float4 albedo : SV_Target0;
    float2 normal : SV_Target1;
    float4 light : SV_Target2;
    float4 material : SV_Target3;
};

[shader("fragment")]
Targets fs_main(Varyings in)
{
    if (faded(push.items, push.instances, in.item, in.position.xy)) discard;
    Item it = item_at(in.item);
    uint4 m = row_u(push.materials, it.material * 2);
    float4 mf = row(push.materials, it.material * 2 + 1);
    float4 t = texel(it, m, in.uv, in.alpha);
    if ((m.y & VERTEX_COLORED) != 0)
    {
        t *= in.color;
    }
    if ((m.y & PACK_FOLIAGE) != 0)
    {
        t.rgb = pack_foliage(push.frame, t.rgb);
    }
    float a = t.a * in.alpha;
    // The alpha test, where the surface has one: kept at or above the reference.
    if ((m.y & 1) != 0 && a - float(m.z) / 255.0 + 1e-6 < 0.0)
    {
        discard;
    }
    if (partial(m, a))
    {
        discard;
    }
    // The normal, bent by the material's normal map where it has one, and its roughness (1,
    // the most, for retail's surfaces, which have no maps).
    float3 n = normalize(in.normal);
    float roughness = 1.0;
    uint maps = asuint(mf.w);
    if (maps != 0xFFFFFFFF)
    {
        uint4 mp = row_u(push.maps, maps);
        uint s = (it.flags & ITEM_TILED) != 0 ? push.wrap : push.clamp;
        if (mp.x != 0xFFFFFFFF)
        {
            float2 xy = (sample(mp.x, s, in.uv).rg * 2.0 - 1.0) * asfloat(mp.z);
            float3 bent = float3(xy, sqrt(saturate(1.0 - dot(xy, xy))));
            float3 t3 = in.tangent.xyz - n * dot(n, in.tangent.xyz);
            if (dot(t3, t3) > 1e-12)
            {
                t3 = normalize(t3);
                float3 b3 = cross(n, t3) * (in.tangent.w < 0.0 ? -1.0 : 1.0);
                n = normalize(t3 * bent.x + b3 * bent.y + n * bent.z);
            }
        }
        // The roughness's factor, times the map's green where there is a map.
        roughness = asfloat(mp.w);
        if (mp.y != 0xFFFFFFFF)
        {
            roughness *= sample(mp.y, s, in.uv).g;
        }
    }
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, it.class_, n, 0.0, float4(t.rgb, roughness));
    o.normal = oct_encode(n);
    o.light = in.light;
    // A leaf's set holds no lights: it carries how deep in its crown it lies, which an art pack's
    // leaves bring in the length of their tangent's last value.
    bool leaf = it.class_ == CLASS_SUN && uint(mf.z) == KIND_FOLIAGE;
    uint deep = uint(saturate(abs(in.tangent.w) - 1.0) * 255.0 + 0.5);
    o.material = material_bytes(it.class_, uint(mf.z), leaf ? deep : it.set);
    return o;
}

// The forward pass's color for translucent and additive surfaces: the surface's own color times
// its light, fogged unless it adds, with the screen ramp, blended over what is drawn.
[shader("fragment")]
float4 fs_forward(Varyings in) : SV_Target0
{
    if (faded(push.items, push.instances, in.item, in.position.xy)) discard;
    masked(in.position);
    Item it = item_at(in.item);
    uint4 m = row_u(push.materials, it.material * 2);
    float4 t = texel(it, m, in.uv, in.alpha);
    float a = t.a * in.alpha;
    if ((m.y & 1) != 0 && a - float(m.z) / 255.0 + 1e-6 < 0.0)
    {
        discard;
    }
    float3 c = t.rgb * in.light.rgb;
    float4 o = blended(push.frame, c, in.light.a, a);
    // An additive surface adds its color whole, or weighted by its alpha or by one less it, as
    // retail's blend for it says.
    if ((m.y & 128) != 0)
    {
        o.rgb *= a;
    }
    else if ((m.y & 256) != 0)
    {
        o.rgb *= 1.0 - a;
    }
    return o;
}

// The edge pass for blended cut-outs: where their texels are partly transparent, blended over
// what is drawn behind them, as retail blends a cut-out; their opaque texels are in the G-buffer.
[shader("fragment")]
float4 fs_edge(Varyings in) : SV_Target0
{
    if (faded(push.items, push.instances, in.item, in.position.xy)) discard;
    masked(in.position);
    Item it = item_at(in.item);
    uint4 m = row_u(push.materials, it.material * 2);
    float4 t = texel(it, m, in.uv, in.alpha);
    float a = t.a * in.alpha;
    if (a - float(m.z) / 255.0 + 1e-6 < 0.0 || !partial(m, a))
    {
        discard;
    }
    float3 c = t.rgb * in.light.rgb;
    return blended(push.frame, c, air_frame(push.frame) ? 1.0 : in.light.a, a);
}

// A caster in the sun's shadow maps: only its depth, where its texels are solid enough. A
// cut-out casts where it stands at or above its alpha reference, as it is drawn.
[shader("fragment")]
void fs_caster(Varyings in)
{
    Item it = item_at(in.item);
    uint4 m = row_u(push.materials, it.material * 2);
    if ((m.y & 1) == 0)
    {
        return;
    }
    float4 t = texel(it, m, in.uv, in.alpha);
    if (t.a * in.alpha - float(m.z) / 255.0 + 1e-6 < 0.0)
    {
        discard;
    }
}
