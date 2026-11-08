// The G-buffer pass for the ground: every drawn block's grid, one instance a block, its cells
// two triangles each, cut as the block cuts them, each cell's picture turned as retail turns it.
// A cell's picture is its composite, or its splat: the composite's layers blended at each pixel
// as retail blends them, the tiles repeating across the cells and the alpha maps turned with the
// cell. The ground is lit per vertex by the sun and its own ambient, truncated to bytes as retail
// bakes it, and fogged per vertex.
import bindless;
import ground;
import retail;
import weather;

struct Push
{
    uint frame;
    // Each vertex: its block-local position, then its normal, as two float4 rows.
    uint vertices;
    // Each cell: its composite picture's index, its quarter turns, its cut (bit 0), kind (bits 1
    // to 3) and how readily it gathers puddles (bits 4 and 5, 0 to 3), and its splat's place in
    // `splats` (~0 for none).
    uint cells;
    // Each drawn block: its first vertex, its first cell, its cells a side, its block slot.
    uint drawn;
    uint blocks;
    uint clamp;
    // The first entry of `drawn` this draw covers: blocks of one side are drawn together.
    uint first;
    // The splats: per splat its base row, then one row an overlay (see `splat`).
    uint splats;
    uint wrap;
    // The weather's constants (~0 for none): the wet, and the puddles' share.
    uint weather;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

struct Varyings
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float4 light : TEXCOORD1;
    float3 normal : TEXCOORD2;
    nointerpolation uint texture : TEXTURE;
    nointerpolation uint splat : SPLAT;
    nointerpolation uint rot : ROT;
    nointerpolation uint kind : KIND;
    nointerpolation uint puddles : PUDDLES;
};



// How far the ground is drawn below where it is, so that what stands on it wins the depth test.
static const float BELOW = 0.01;

[shader("vertex")]
Varyings vs_main(uint vertex : SV_VertexID, uint instance : SV_InstanceID)
{
    uint4 b = row_u(push.drawn, push.first + instance);
    uint side = b.z;
    uint cell = vertex / 6;
    uint corner = vertex % 6;
    uint i = cell / side;
    uint j = cell % side;
    uint4 c = row_u(push.cells, b.y + cell);
    bool cut = (c.z & 1) != 0;
    uint rot = c.y & 3;
    uint svc = side + 1;
    uint sw = i * svc + j;
    uint se = (i + 1) * svc + j;
    uint ne = (i + 1) * svc + j + 1;
    uint nw = i * svc + j + 1;
    // The two triangles of the cell, and their corners' texture coordinates, in retail's order.
    uint v;
    uint u;
    uint k = corner % 3;
    if (cut)
    {
        uint tri0[3] = { sw, se, ne };
        uint tri1[3] = { sw, ne, nw };
        uint uv0[3] = { SW[rot], SE[rot], NE[rot] };
        uint uv1[3] = { SW[rot], NE[rot], NW[rot] };
        v = corner < 3 ? tri0[k] : tri1[k];
        u = corner < 3 ? uv0[k] : uv1[k];
    }
    else
    {
        uint tri0[3] = { sw, se, nw };
        uint tri1[3] = { ne, nw, se };
        uint uv0[3] = { SW[rot], SE[rot], NW[rot] };
        uint uv1[3] = { NE[rot], NW[rot], SE[rot] };
        v = corner < 3 ? tri0[k] : tri1[k];
        u = corner < 3 ? uv0[k] : uv1[k];
    }
    float3 position = row(push.vertices, (b.x + v) * 2).xyz;
    float3 normal = row(push.vertices, (b.x + v) * 2 + 1).xyz;
    float3 p = position - float3(0.0, 0.0, BELOW) + row(push.blocks, b.w).xyz;
    float4 eye = transform(push.frame, FRAME_VIEW, float4(p, 1.0));
    float fog = fog_factor(push.frame, length(eye.xyz));
    bool per_pixel = row(push.frame, FRAME_SCREEN).y > 0.5;

    Varyings o;
    o.position = transform(push.frame, FRAME_VIEW_PROJ, float4(p, 1.0));
    o.uv = UVS[u];
    o.light = float4(per_pixel ? float3(0.0, 0.0, 0.0) : ground_light(push.frame, normal), fog);
    o.normal = normal;
    o.texture = c.x;
    o.splat = c.w;
    o.rot = rot;
    o.kind = (c.z >> 1) & 7;
    o.puddles = (c.z >> 4) & 3;
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
Targets fs_main(Varyings in)
{
    // The steps across the screen are taken before any branch, so every splat sample has them.
    float2 dx = ddx(in.uv);
    float2 dy = ddy(in.uv);
    float4 t = in.texture != 0xFFFFFFFF ? sample(in.texture, push.clamp, in.uv) : float4(1.0, 1.0, 1.0, 1.0);
    // The water's share of the ground at the pixel: from the splat's layers, or the cell's own
    // kind under a composite.
    float water = in.kind == KIND_WATER ? 1.0 : 0.0;
    // How readily it gathers puddles: from the splat's layers, or the cell's own.
    float takes = float(in.puddles) / 3.0;
    float3 n = normalize(in.normal);
    float3 flat = n;
    float roughness = 1.0;
    if (in.splat != 0xFFFFFFFF)
    {
        SplatSample s = splat(push.splats, push.wrap, push.clamp, in.splat * SPLAT_ROWS, in.rot, clamp(in.uv, 0.0, 1.0), dx, dy);
        t = float4(s.color, 1.0);
        water = s.water;
        takes = s.puddles;
        n = bent(n, s.bend);
        roughness = s.roughness;
    }
    Targets o;
    o.albedo = wetted(push.weather, push.frame, in.position, CLASS_GROUND, n, water, float4(t.rgb, roughness));
    o.normal = oct_encode(n);
    o.light = in.light;
    // The kind is water where water is most of the blend, and the share is kept beside it; where
    // it is not water, how much of it is puddle.
    uint kind = water > 0.5 ? KIND_WATER : (in.kind == KIND_WATER ? 0 : in.kind);
    uint puddle = 0;
    if (kind != KIND_WATER && push.weather != NO_WEATHER)
    {
        float3 at = frame_point(push.frame, in.position);
        puddle = puddle_byte(puddle_share(push.weather, at, flat, takes * (1.0 - water)));
    }
    o.material = material_bytes(CLASS_GROUND, kind, uint(water * 255.0 + 0.5) | (puddle << 8));
    return o;
}
