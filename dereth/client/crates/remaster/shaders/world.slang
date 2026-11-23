// The world's landscape vertices on the device, on the world's land grid: the heights buffer opens
// with the grid they lie on (the vertices along a side, the meters between neighbors, a road's
// half width and the vertex steps along a land cell side), then holds each vertex's height, row by row from the south. The terrain words
// buffer holds a word a vertex in the same order: the terrain word in its low half, and above it
// whether the square that vertex is the south-west corner of is cut from that corner to its
// north-east one. The water buffer holds two 16-bit floats a vertex in the same order, in one word,
// the first in its low half. Every reader of the world's vertices takes the grid from here.
module world;

import bindless;
import half;

// The bytes before the heights.
static const uint HEADER = 16;

// The world's vertices along a side.
public int world_side(uint heights)
{
    return int(buffers[NonUniformResourceIndex(heights)].Load(0));
}

// The meters between neighboring vertices.
public float world_spacing(uint heights)
{
    return asfloat(buffers[NonUniformResourceIndex(heights)].Load(4));
}

// A road's half width, meters.
public float world_road_width(uint heights)
{
    return asfloat(buffers[NonUniformResourceIndex(heights)].Load(8));
}

// The vertex steps along a land cell side: 1 on the default grid.
public uint world_steps(uint heights)
{
    return max(buffers[NonUniformResourceIndex(heights)].Load(12), 1u);
}

// Vertex `g` clamped to the world, and its place in the buffers.
public uint world_vertex(uint heights, int2 g)
{
    int side = world_side(heights);
    int2 c = clamp(g, int2(0, 0), int2(side - 1, side - 1));
    return uint(c.y * side + c.x);
}

// The height of vertex `g`, clamped to the world.
public float world_height(uint heights, int2 g)
{
    return asfloat(buffers[NonUniformResourceIndex(heights)].Load(HEADER + world_vertex(heights, g) * 4));
}

// The terrain word of vertex `g`, clamped to the world.
public uint world_word(uint heights, uint words, int2 g)
{
    return buffers[NonUniformResourceIndex(words)].Load(world_vertex(heights, g) * 4) & 0xFFFF;
}

// Whether the square whose south-west vertex is `g` is cut from that corner to its north-east one.
public bool world_cut(uint heights, uint words, int2 g)
{
    return (buffers[NonUniformResourceIndex(words)].Load(world_vertex(heights, g) * 4) & 0x10000) != 0;
}

// The two water floats of vertex `g`, clamped to the world.
public float2 world_water(uint heights, uint water, int2 g)
{
    return unhalves(buffers[NonUniformResourceIndex(water)].Load(world_vertex(heights, g) * 4));
}
