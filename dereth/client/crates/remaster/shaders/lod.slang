// A part fading from one level of detail into the next: both levels are drawn, each through its
// own half of a fixed screen pattern, the share of the next level's half growing with the fade.
// The pattern is fixed on the screen, so a still view draws the same pixels frame after frame.
module lod;

import retail;

// The 8 by 8 ordered pattern's threshold at pixel `p`, in (0, 1): each of its 64 values once.
public float fade_threshold(int2 p)
{
    static const uint BAYER[64] = {
        0, 32, 8, 40, 2, 34, 10, 42,
        48, 16, 56, 24, 50, 18, 58, 26,
        12, 44, 4, 36, 14, 46, 6, 38,
        60, 28, 52, 20, 62, 30, 54, 22,
        3, 35, 11, 43, 1, 33, 9, 41,
        51, 19, 59, 27, 49, 17, 57, 25,
        15, 47, 7, 39, 13, 45, 5, 37,
        63, 31, 55, 23, 61, 29, 53, 21,
    };
    uint i = uint(p.y & 7) * 8 + uint(p.x & 7);
    return (float(BAYER[i]) + 0.5) / 64.0;
}

// The level an item draws, from its flags (bits 8 to 15).
public uint item_level(uint flags)
{
    return (flags >> 8) & 0xFF;
}

// Whether draw item `item` (of buffer `items`, two rows each: its instance first, its flags
// second) is left out at pixel `position` while its part fades, as row 6 of its instance (of
// buffer `instances`, eight rows each) says.
public bool faded(uint items, uint instances, uint item, float2 position)
{
    uint instance = row_u(items, item * 2).x;
    uint flags = row_u(items, item * 2 + 1).x;
    return faded_out(row(instances, instance * 8 + 6), item_level(flags), position);
}

// Whether an item of level `level` is left out at pixel `position` while its part fades as
// `fade_row` says: its z the fade (0 not fading), its w the level faded from plus 256 times the
// level faded into. The level faded from keeps the pixels whose threshold is at or above the
// fade, the level faded into the rest.
public bool faded_out(float4 fade_row, uint level, float2 position)
{
    float fade = fade_row.z;
    if (fade <= 0.0)
    {
        return false;
    }
    uint both = uint(fade_row.w + 0.5);
    uint from = both & 0xFF;
    uint into = both >> 8;
    float t = fade_threshold(int2(position));
    if (level == from)
    {
        return t < fade;
    }
    if (level == into)
    {
        return t >= fade;
    }
    return false;
}
