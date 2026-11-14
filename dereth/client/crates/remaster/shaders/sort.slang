// The cut-outs' edges and the blended surfaces sorted far to near on the device, as the processor
// sorts its own lists of them: each list's keys (how far off, as the bits of the squared distance
// turned over, then which item), the processor's first and then the device's parts', sorted
// farthest first and by item where two stand as far, and made into the indexed indirect commands
// the list is drawn by, in that order.
//
// A bitonic sort over each list's keys, padded to a power of two with keys that sort last:
// `cs_local` sorts each run of `RUN` keys in shared memory, read from the processor's keys and
// the device's; `cs_global` merges across runs, a pair of keys a thread; `cs_merge` finishes each
// merge within a run; `cs_emit` writes each key's command. Every pass works on both lists, a row
// of workgroups each, and does nothing past the keys a list holds. An item is listed once, so no
// two keys are equal: the order is the processor's, whatever the sort's own.
import bindless;

struct Push
{
    // The keys being sorted, `cap` of each list, two words each.
    uint sorted;
    // The device's parts' keys, `cap` of each list; and the processor's, the edges' then the
    // blended surfaces'.
    uint keys;
    uint cpu;
    // The processor's keys in each list.
    uint cpu_edges;
    uint cpu_forward;
    // Each list's count, the processor's keys with the device's.
    uint counts;
    uint cap;
    // The merge's run length, and how far apart the keys a pass compares stand.
    uint k;
    uint j;
    // The commands, `cap` to a list, and the draw items, two rows each.
    uint commands;
    uint items;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The keys a workgroup sorts in shared memory, two a thread.
static const uint RUN = 2048;
// The key that sorts after every other.
static const uint2 LAST = uint2(0xFFFFFFFF, 0xFFFFFFFF);

groupshared uint2 gs_keys[RUN];

// Whether key `a` sorts before key `b`.
bool before(uint2 a, uint2 b)
{
    return a.x < b.x || (a.x == b.x && a.y < b.y);
}

// How many keys list `list` holds.
uint held(uint list)
{
    return buffers[NonUniformResourceIndex(push.counts)].Load(list * 4);
}

// The keys list `list` is sorted over: what it holds, up to a power of two, at least a run.
uint span(uint list)
{
    uint n = held(list);
    uint p = RUN;
    while (p < n && p < push.cap)
    {
        p <<= 1;
    }
    return p;
}

// Key `i` of list `list`, which holds `count`: the processor's first, then the device's, then
// keys that sort last.
uint2 key_at(uint list, uint i, uint count)
{
    if (i >= count)
    {
        return LAST;
    }
    uint cpu = list == 0 ? push.cpu_edges : push.cpu_forward;
    if (i < cpu)
    {
        uint first = list == 0 ? 0 : push.cpu_edges;
        return buffers[NonUniformResourceIndex(push.cpu)].Load2((first + i) * 8);
    }
    return buffers[NonUniformResourceIndex(push.keys)].Load2((list * push.cap + i - cpu) * 8);
}

// The keys at `a` and `b` of the shared run put in order: rising where `up`.
void order_shared(uint a, uint b, bool up)
{
    uint2 x = gs_keys[a];
    uint2 y = gs_keys[b];
    if (before(y, x) == up)
    {
        gs_keys[a] = y;
        gs_keys[b] = x;
    }
}

// The first key of the pair thread `t` compares, the pairs `j` apart.
uint pair(uint t, uint j)
{
    return 2 * j * (t / j) + (t % j);
}

// Each run of `RUN` keys sorted, rising and falling by turns as the merges after it want them.
[shader("compute")]
[numthreads(1024, 1, 1)]
void cs_local(uint3 local : SV_GroupThreadID, uint3 group : SV_GroupID)
{
    uint list = group.y;
    uint base = group.x * RUN;
    if (base >= span(list))
    {
        return;
    }
    uint count = held(list);
    uint t = local.x;
    gs_keys[t] = key_at(list, base + t, count);
    gs_keys[t + RUN / 2] = key_at(list, base + t + RUN / 2, count);
    for (uint k = 2; k <= RUN; k <<= 1)
    {
        for (uint j = k >> 1; j > 0; j >>= 1)
        {
            GroupMemoryBarrierWithGroupSync();
            uint i = pair(t, j);
            order_shared(i, i + j, ((base + i) & k) == 0);
        }
    }
    GroupMemoryBarrierWithGroupSync();
    RWByteAddressBuffer sorted = rw_buffers[NonUniformResourceIndex(push.sorted)];
    uint at = list * push.cap + base;
    sorted.Store2((at + t) * 8, gs_keys[t]);
    sorted.Store2((at + t + RUN / 2) * 8, gs_keys[t + RUN / 2]);
}

// One step of merge `k` across runs: each pair `j` apart put in order.
[shader("compute")]
[numthreads(256, 1, 1)]
void cs_global(uint3 id : SV_DispatchThreadID, uint3 group : SV_GroupID)
{
    uint list = group.y;
    uint n = span(list);
    if (push.k > n || id.x >= n / 2)
    {
        return;
    }
    uint i = pair(id.x, push.j);
    RWByteAddressBuffer sorted = rw_buffers[NonUniformResourceIndex(push.sorted)];
    uint a = (list * push.cap + i) * 8;
    uint b = (list * push.cap + i + push.j) * 8;
    uint2 x = sorted.Load2(a);
    uint2 y = sorted.Load2(b);
    if (before(y, x) == ((i & push.k) == 0))
    {
        sorted.Store2(a, y);
        sorted.Store2(b, x);
    }
}

// The rest of merge `k` within each run: the pairs from half a run apart down to neighbours.
[shader("compute")]
[numthreads(1024, 1, 1)]
void cs_merge(uint3 local : SV_GroupThreadID, uint3 group : SV_GroupID)
{
    uint list = group.y;
    uint n = span(list);
    uint base = group.x * RUN;
    if (push.k > n || base >= n)
    {
        return;
    }
    uint t = local.x;
    RWByteAddressBuffer sorted = rw_buffers[NonUniformResourceIndex(push.sorted)];
    uint at = list * push.cap + base;
    gs_keys[t] = sorted.Load2((at + t) * 8);
    gs_keys[t + RUN / 2] = sorted.Load2((at + t + RUN / 2) * 8);
    for (uint j = RUN / 2; j > 0; j >>= 1)
    {
        GroupMemoryBarrierWithGroupSync();
        uint i = pair(t, j);
        order_shared(i, i + j, ((base + i) & push.k) == 0);
    }
    GroupMemoryBarrierWithGroupSync();
    sorted.Store2((at + t) * 8, gs_keys[t]);
    sorted.Store2((at + t + RUN / 2) * 8, gs_keys[t + RUN / 2]);
}

// Each sorted key's command: its item's indices, once, the item as its first instance.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_emit(uint3 id : SV_DispatchThreadID, uint3 group : SV_GroupID)
{
    uint list = group.y;
    if (id.x >= held(list))
    {
        return;
    }
    uint at = list * push.cap + id.x;
    uint item = buffers[NonUniformResourceIndex(push.sorted)].Load2(at * 8).y;
    uint4 a = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32);
    uint4 b = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32 + 16);
    RWByteAddressBuffer commands = rw_buffers[NonUniformResourceIndex(push.commands)];
#ifdef DERETH_DXIL
    // Direct3D 12's command: the item, set as the draw's root constant, then
    // `D3D12_DRAW_INDEXED_ARGUMENTS`.
    uint byte = at * 24;
    commands.Store(byte, item);
    byte += 4;
#else
    uint byte = at * 20;
#endif
    commands.Store(byte, a.z);
    commands.Store(byte + 4, 1);
    commands.Store(byte + 8, b.z);
    commands.Store(byte + 12, 0);
    commands.Store(byte + 16, item);
}
