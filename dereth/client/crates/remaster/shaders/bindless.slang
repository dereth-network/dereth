// The table every pipeline reads its resources through: descriptor set 0 of the one pipeline
// layout they all share on Vulkan, the descriptor heaps themselves on Direct3D 12. A draw names
// what it reads by index, in its push constants.
module bindless;

#ifdef DERETH_DXIL

// Direct3D 12, shader model 6.6: the same names, read straight from the descriptor heaps. The layer
// lays the resource heap out as every texture from 0, then every storage buffer as a read view and
// a write view side by side from `DXIL_BUFFERS`; the samplers are the sampler heap's.
static const uint DXIL_BUFFERS = 65536;
static const uint DXIL_ACCELS = 65536 * 3;

public struct TextureTable
{
    public __subscript(uint i) -> Texture2D
    {
        get { return ResourceDescriptorHeap[i]; }
    }
};

public struct SamplerTable
{
    public __subscript(uint i) -> SamplerState
    {
        get { return SamplerDescriptorHeap[i]; }
    }
};

public struct BufferTable
{
    public __subscript(uint i) -> ByteAddressBuffer
    {
        get { return ResourceDescriptorHeap[DXIL_BUFFERS + i * 2]; }
    }
};

public struct RwBufferTable
{
    public __subscript(uint i) -> RWByteAddressBuffer
    {
        get { return ResourceDescriptorHeap[DXIL_BUFFERS + i * 2 + 1]; }
    }
};

public struct AccelTable
{
    public __subscript(uint i) -> RaytracingAccelerationStructure
    {
        get { return ResourceDescriptorHeap[DXIL_ACCELS + i]; }
    }
};

// Every sampled texture, at the index the layer gave it when it was bound.
public static TextureTable textures;

// The samplers.
public static SamplerTable samplers;

// Every storage buffer, read only.
public static BufferTable buffers;

// Every storage buffer, read and written.
public static RwBufferTable rw_buffers;

// Every top-level acceleration structure, where the device traces rays: only shaders that query
// them name them, and only those are made into pipelines there.
public static AccelTable accels;

// The draw's item: on Direct3D 12 the second root constant, set for each draw, where Vulkan has the
// draw's first instance. A vertex shader that reads it reads `draw_item.value` here and
// `SV_StartInstanceLocation` on Vulkan.
public struct DrawItem
{
    public uint value;
};
public ConstantBuffer<DrawItem> draw_item : register(b1);

#else

// Every sampled texture, at the index the layer gave it when it was bound.
[[vk::binding(0, 0)]]
public Texture2D textures[];

// The samplers.
[[vk::binding(1, 0)]]
public SamplerState samplers[];

// Every storage buffer, read only.
[[vk::binding(2, 0)]]
public ByteAddressBuffer buffers[];

// Every storage buffer, read and written.
[[vk::binding(3, 0)]]
public RWByteAddressBuffer rw_buffers[];

// Every top-level acceleration structure, where the device traces rays: only shaders that query
// them name them, and only those are made into pipelines there.
[[vk::binding(4, 0)]]
public RaytracingAccelerationStructure accels[];

#endif

// Texture `texture` sampled with sampler `sampler` at `uv`. Either index may differ between the
// pixels of one draw.
public float4 sample(uint texture, uint sampler, float2 uv)
{
    return textures[NonUniformResourceIndex(texture)].Sample(
        samplers[NonUniformResourceIndex(sampler)], uv);
}
