// The cull: each visible draw item becomes an indexed indirect draw command in its bucket's
// list, counted, so the G-buffer pass draws every bucket with one indirect-count draw. In the
// mode that matches retail, the visible items are exactly the ones the walk drew. The visible
// items from `split` on are the interior's, drawn after the openings' stamps, into buckets of
// their own. Each item comes with its place in its bucket's list, in the walk's order, so a
// bucket's commands are in the same order every frame.
import bindless;

struct Push
{
    // The visible items: each its index and its place in its bucket's list; and how many there
    // are.
    uint visible;
    uint count;
    // Each item: two uint4 rows; the second's last word is its bucket.
    uint items;
    // The commands, `capacity` to a bucket, and the buckets' counts.
    uint commands;
    uint counts;
    uint capacity;
    // The first of the interior's visible items.
    uint split;
};

// The buckets a group of items takes: drawn from front faces, and from both.
static const uint GROUP_BUCKETS = 2;

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= push.count)
    {
        return;
    }
    uint2 v = buffers[NonUniformResourceIndex(push.visible)].Load2(id.x * 8);
    uint item = v.x;
    uint4 a = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32);
    uint4 b = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32 + 16);
    uint bucket = b.w + (id.x >= push.split ? GROUP_BUCKETS : 0);
    uint slot = v.y;
    uint counted;
    rw_buffers[NonUniformResourceIndex(push.counts)].InterlockedAdd(bucket * 4, 1, counted);
    if (slot >= push.capacity)
    {
        return;
    }
    RWByteAddressBuffer commands = rw_buffers[NonUniformResourceIndex(push.commands)];
#ifdef DERETH_DXIL
    // Direct3D 12's command: the item, set as the draw's root constant, then
    // `D3D12_DRAW_INDEXED_ARGUMENTS` as below.
    uint at = (bucket * push.capacity + slot) * 24;
    commands.Store(at, item);
    at += 4;
#else
    uint at = (bucket * push.capacity + slot) * 20;
#endif
    // `DrawIndexedIndirectCommand`: the index count, one instance, the first index, no vertex
    // offset (the item carries its vertices' first index), and the item as the first instance.
    commands.Store(at, a.z);
    commands.Store(at + 4, 1);
    commands.Store(at + 8, b.z);
    commands.Store(at + 12, 0);
    commands.Store(at + 16, item);
}
