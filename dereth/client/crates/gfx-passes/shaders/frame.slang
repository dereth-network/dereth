// The frame's vocabulary, shared by every game's remaster and the passes they share: the rows of
// the frame's constants, the light classes the material target carries, and how a buffer's rows
// are read. A game's own light model imports it and adds its own rows and rules.
module frame;

import bindless;

// The frame's constants, in storage buffer `frame` at offset 0, as float4 rows:
//   0..3   the view and projection together, as four columns
//   4..7   the view (into the eye), as four columns
//   8      the eye, xyz
//   9      the sun: its direction scaled by its brightness, xyz (not normalized)
//   10     the sun's color, rgb
//   11     the ground's ambient: its color times its level, rgb
//   12     the objects' ambient, as the device is given it, rgb
//   13     the fog: start, end, on (0 or 1), unused
//   14     the fog's color, rgb
//   15     x the screen brightness, y the mode (0 the light per vertex, 1 per pixel),
//          z the light count, w the static light count
//   16..19 the inverse of the view and projection, as four columns
//   20     the target's extent in pixels, xy
public static const uint FRAME_VIEW_PROJ = 0;
public static const uint FRAME_VIEW = 4;
public static const uint FRAME_EYE = 8;
public static const uint FRAME_SUN = 9;
public static const uint FRAME_SUN_COLOR = 10;
public static const uint FRAME_GROUND_AMBIENT = 11;
public static const uint FRAME_OBJECT_AMBIENT = 12;
public static const uint FRAME_FOG = 13;
public static const uint FRAME_FOG_COLOR = 14;
public static const uint FRAME_SCREEN = 15;
public static const uint FRAME_INV_VIEW_PROJ = 16;
public static const uint FRAME_EXTENT = 20;
// Where the near ground gives way to the far, meters from the eye (x, y), and z 1 while a far
// ground is drawn: what stands on the ground is lifted onto it mixed the same way.
public static const uint FRAME_LIFT = 22;
// How far round the screen patterns a tree's pictures are cut by are turned this frame, while the
// smoothing or an upscaler lays frames over each other: x the pattern that picks a card's view, y
// the one that cuts its coverage, 0 to 1; z 1 while they turn at all, 0 while they hold still.
public static const uint FRAME_DITHER = 23;
// The wind an art pack's trees sway in: xy its push east and north, meters a second; z the
// weather's clock within its period, seconds.
public static const uint FRAME_WIND = 24;

// The light classes, which the material target's first channel carries.
public static const uint CLASS_NONE = 0;
public static const uint CLASS_GROUND = 1;
public static const uint CLASS_SUN = 2;
public static const uint CLASS_INDOOR = 3;
public static const uint CLASS_CELL = 4;
public static const uint CLASS_SKY = 5;

// One float4 of buffer `b`, the `i`th.
public float4 row(uint b, uint i)
{
    return asfloat(buffers[NonUniformResourceIndex(b)].Load4(i * 16));
}

// One uint4 of buffer `b`, the `i`th.
public uint4 row_u(uint b, uint i)
{
    return buffers[NonUniformResourceIndex(b)].Load4(i * 16);
}

// The column-major matrix at row `i` of buffer `b`, applied to `v`.
public float4 transform(uint b, uint i, float4 v)
{
    return row(b, i) * v.x + row(b, i + 1) * v.y + row(b, i + 2) * v.z + row(b, i + 3) * v.w;
}
