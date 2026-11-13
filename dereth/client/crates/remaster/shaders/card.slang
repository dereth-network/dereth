// A tree's pictures from round it, as the impostors and the far forest both draw them: on an
// upright card turned to the eye, showing the views either side of the eye's direction through
// complementary halves of a screen pattern, its coverage kept as a share of another. While the
// smoothing or an upscaler lays frames over each other, the patterns turn each frame, so each
// pixel shows each view, and is kept, in the share of the frames it is due, and the frames laid
// over each other show the views blended and the coverage as it is. The smoothing's patterns turn
// a step of its sequence a frame, so a still view draws the same frames again each sequence;
// an upscaler's turn on without repeating, so it never takes them for a pattern on the surface.
//
// The pictures come in two forms. An atlas row holds a tree's views side by side at full size,
// each texel's color multiplied by its coverage, with no smaller copies. A far picture holds its
// smaller copies: each a row of the views at half the size of the one before, its colors and its
// normals multiplied by the coverage, laid out as [`chain_level`] says.
module card;

import bindless;
import lod;
import retail;

// The views round a tree, and a view's side in an atlas row, texels.
public static const uint CARD_VIEWS = 16;
public static const float CARD_TILE = 256.0;

// The levels a far picture holds: half an atlas row's size, a quarter, and so on to a texel. The
// odd levels lie in one page and the even in another, so each is made from the level before it.
public static const uint CHAIN_LEVELS = 8;
// The pages that hold far pictures, texels, and a far picture's height in each: the odd levels',
// then the even levels'.
public static const float2 CHAIN_ODD_PAGE = float2(2048.0, 1280.0);
public static const float2 CHAIN_EVEN_PAGE = float2(1024.0, 640.0);
public static const float CHAIN_ODD_HEIGHT = 160.0;
public static const float CHAIN_EVEN_HEIGHT = 80.0;

// Where level `level` (1 and on) of a far picture lies in its page's slot, texels: the first of
// the page's levels across the top, the rest side by side under it.
public float2 chain_level(uint level)
{
    if (level <= 2)
    {
        return float2(0.0, 0.0);
    }
    float x = 0.0;
    uint last = min(level, CHAIN_LEVELS);
    for (uint k = (level & 1) != 0 ? 3 : 4; k < last; k += 2)
    {
        x += float(CARD_VIEWS) * CARD_TILE / float(1u << k);
    }
    return float2(x, CARD_TILE / float((level & 1) != 0 ? 2 : 4));
}

// How far round frame `frame` turns the pattern that picks the views (x) and the one that cuts
// (y), 0 to 1, and whether frames are laid over each other at all (z, 1 or 0).
public float3 card_dither(uint frame)
{
    return row(frame, FRAME_DITHER).xyz;
}

// A screen pattern's threshold at pixel `p`, in (0, 1): the 8 by 8 ordered pattern's, moved on by
// `turn` while `turning`, so over the frames laid over each other each pixel's thresholds spread
// across the way and it is kept in the share its coverage gives it.
float card_threshold(int2 p, float turn, float turning)
{
    float t = fade_threshold(p);
    return turning > 0.5 ? frac(t + turn) : t;
}

// The view a card shows at pixel `position`, where the eye stands `view` views round the tree:
// of the two either side, each on the share of a screen pattern its nearness gives it, the
// pattern turned on as `dither` says. The pattern is turned a quarter from the fade's, so the
// view chosen does not follow the fade.
public uint card_view(float view, float4 position, float3 dither)
{
    uint v0 = uint(floor(view)) % CARD_VIEWS;
    uint v1 = (v0 + 1) % CARD_VIEWS;
    float between = view - floor(view);
    float t2 = card_threshold(int2(int(position.y), int(position.x)) + int2(3, 5), dither.x, dither.z);
    return t2 < between ? v1 : v0;
}

// The coverage below which a card's pixel at `position` is cut away, where a pixel covers
// `texels` texels of the full-size picture. A pixel is kept on the share of a screen pattern its
// coverage holds, so a crown keeps as much of the screen as its picture covers rather than
// thinning to its densest parts; the pattern turned from the others, so it follows neither the
// fade nor the views, and turned on as `dither` says. While no sequence of frames is laid over
// each other, a pixel that covers a texel or less is cut at half instead, along the picture's own
// edge.
public float card_cut(float texels, float4 position, float3 dither)
{
    float t3 = card_threshold(int2(int(position.x) + 7, int(position.y) + 2).yx, dither.y, dither.z);
    return lerp(0.5, t3, max(saturate(texels - 1.0), dither.z));
}

// How many texels of the full-size picture a pixel covers, for a card position `at` (0 to 1
// across and down).
public float card_texels(float2 at)
{
    return max(length(ddx(at)), length(ddy(at))) * CARD_TILE;
}

// The color of an atlas row's view `view` at card position `at`, and how much of the pixel it
// covers. The row holds each texel's color multiplied by its coverage, and no smaller copies:
// samples spread over the pixel's footprint stand in for one, two by two, or four by four where
// it covers more than a few texels, so a far card does not sparkle. The footprint is measured on
// the card, since neighboring pixels may show different views, and the samples stay in this
// view's own picture. `uv`, `corner` and `tile` are where in the page the view's picture is read.
public float4 card_row_color(uint picture, uint wrap, uint view, uint row, uint rows, float2 at,
                             out float2 uv, out float2 corner, out float2 tile)
{
    tile = float2(1.0 / float(CARD_VIEWS), 1.0 / float(rows));
    corner = float2(float(view), float(row)) * tile;
    uv = corner + at * tile;
    float2 fx = ddx(at);
    float2 fy = ddy(at);
    float texels = max(length(fx), length(fy)) * CARD_TILE;
    int taps = texels > 3.0 ? 4 : 2;
    float2 dx = fx * tile;
    float2 dy = fy * tile;
    float2 inset = tile * (0.5 / CARD_TILE);
    float4 albedo = float4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < taps; i++)
    {
        for (int j = 0; j < taps; j++)
        {
            float2 o = dx * ((float(i) + 0.5) / float(taps) - 0.5) + dy * ((float(j) + 0.5) / float(taps) - 0.5);
            float4 c = textures[NonUniformResourceIndex(picture)].SampleLevel(
                samplers[NonUniformResourceIndex(wrap)],
                clamp(uv + o, corner + inset, corner + tile - inset), 0.0);
            albedo += c;
        }
    }
    // The color of what the samples cover, and how much of the pixel they cover.
    return float4(albedo.rgb / max(albedo.a, 1e-4), albedo.a / float(taps * taps));
}

// One level of a far picture at view `view`, card position `at`, onto `c` and `n` by `weight`: its
// color and normal, each still multiplied by the coverage, read bilinearly within the view's own
// picture. `pages` are the odd and even levels' pages' color and normal pictures, and `slot` the
// picture's place in them.
void chain_read(uint4 pages, uint clamp_sampler, uint slot, uint level, uint view, float2 at,
                inout float4 c, inout float4 n, float weight)
{
    bool odd = (level & 1) != 0;
    float side = CARD_TILE / float(1u << level);
    float top = float(slot) * (odd ? CHAIN_ODD_HEIGHT : CHAIN_EVEN_HEIGHT);
    float2 corner = chain_level(level) + float2(float(view) * side, top);
    float2 p = corner + clamp(at * side, 0.5, side - 0.5);
    float2 uv = p / (odd ? CHAIN_ODD_PAGE : CHAIN_EVEN_PAGE);
    SamplerState s = samplers[NonUniformResourceIndex(clamp_sampler)];
    uint color = odd ? pages.x : pages.z;
    uint normal = odd ? pages.y : pages.w;
    c += textures[NonUniformResourceIndex(color)].SampleLevel(s, uv, 0.0) * weight;
    n += textures[NonUniformResourceIndex(normal)].SampleLevel(s, uv, 0.0) * weight;
}

// A far picture's view `view` at card position `at`, read at the level `detail` calls for (1 its
// first, fractions between two levels blended): its color multiplied by the coverage, and its
// normal and leaf mark, each multiplied by the coverage.
public float4 chain_color(uint4 pages, uint clamp_sampler, uint slot, uint view, float2 at,
                          float detail, out float4 n)
{
    // A footprint that is no number (a card seen edge on) reads the first level.
    float l = detail == detail ? clamp(detail, 1.0, float(CHAIN_LEVELS)) : 1.0;
    uint l0 = min(uint(floor(l)), CHAIN_LEVELS);
    uint l1 = min(l0 + 1, CHAIN_LEVELS);
    float f = l - float(l0);
    float4 c = float4(0.0, 0.0, 0.0, 0.0);
    n = float4(0.0, 0.0, 0.0, 0.0);
    chain_read(pages, clamp_sampler, slot, l0, view, at, c, n, 1.0 - f);
    if (f > 0.0)
    {
        chain_read(pages, clamp_sampler, slot, l1, view, at, c, n, f);
    }
    return c;
}
