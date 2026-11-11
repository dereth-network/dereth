// An art pack's placed models chosen and listed on the device: each one's level of detail, its
// fade into the next, whether it is seen, which of the sun's cascades it casts into, and the
// indexed indirect commands that draw it, every frame, in three passes.
//
// - `cs_choose`, one model a thread: its level and fade from its distance, as the processor
//   chooses a degrading part's (`render/lod.rs`); the fade written into its instance's row, as the
//   G-buffer's fade reads it; whether it is seen (its block drawn, its sphere in the view or the
//   mirrored view); the cascades it reaches; and how many commands each list takes from it, summed
//   over its workgroup.
// - `cs_scan`, one workgroup: each list's workgroup sums made into each workgroup's first place,
//   and each list's count written.
// - `cs_write`, one model a thread: its first place in each list (its workgroup's, plus the models
//   before it in the workgroup), and its commands written there.
//
// A list's commands are in the models' order, frame after frame: no race decides a place. The
// G-buffer's two culled buckets take the models' commands after the processor's own, from
// `base`; each cascade has a list of its own, its models' first levels, drawn into its cache.
// Ground cover casts into the nearest cascade alone.
import bindless;
import retail;

struct Push
{
    // The models: four rows each, `count` of them.
    uint table;
    uint count;
    // Each group's levels' errors and hand-over distances: five rows each.
    uint groups;
    // The frame's constants, below.
    uint constants;
    // Each block's offset from the frame's origin, a row a slot; and a bit a slot for the blocks
    // the walk drew.
    uint blocks;
    uint drawn;
    // The instances, eight rows each, and the draw items, two rows each.
    uint instances;
    uint items;
    // The G-buffer's commands, `capacity` to a bucket, and the buckets' counts.
    uint commands;
    uint counts;
    uint capacity;
    // Each model's command counts, a row each, and each workgroup's sums, two rows each.
    uint tallies;
    uint sums;
    // Each model's choice, a row each, for the checks.
    uint states;
    // The cascades' commands, `caster_capacity` to a cascade, and their counts.
    uint casters;
    uint caster_counts;
    uint caster_capacity;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The constants' rows: the eye and the pixels a meter covers at a meter; the error bar, the fade's
// band, whether levels are chosen, and the flags; the processor's commands in each bucket, the
// cascades drawn again and the groups' count; the view's six planes, the mirrored view's six; the
// sun's view; each cascade's box across the sun and along it.
static const uint C_EYE = 0;
static const uint C_CHOICE = 1;
static const uint C_LISTS = 2;
static const uint C_VIEW = 3;
static const uint C_MIRROR = 9;
static const uint C_SUN = 15;
static const uint C_CASCADES = 19;

// The flags: the mirrored view is drawn; the casters are those in the view alone.
static const uint F_MIRROR = 1;
static const uint F_IN_VIEW = 2;

// A model's flags: it is ground cover; it is held.
static const uint M_COVER = 1;
static const uint M_LIVE = 2;

// No group: the model draws its first level alone.
static const uint NO_GROUP = 0xFFFFFFFF;

// The lists: the two culled buckets, then the four cascades.
static const uint LISTS = 6;
static const uint CASCADES = 4;
static const uint MAX_LEVELS = 8;

// One model's choice.
struct Choice
{
    uint level;
    uint next;
    float fade;
    bool seen;
    // A bit a cascade it casts into.
    uint casts;
};

// How many items model `m`'s level `l` draws, and the first of them.
uint2 level_items(uint4 head, uint4 counts, uint l)
{
    uint first = head.w;
    for (uint k = 0; k < l; k++)
    {
        first += ((k < 4 ? counts.x : counts.y) >> ((k & 3) * 8)) & 0xFF;
    }
    uint n = ((l < 4 ? counts.x : counts.y) >> ((l & 3) * 8)) & 0xFF;
    return uint2(first, n);
}

// Whether a sphere stands inside six planes starting at constants row `at`.
bool inside(uint at, float3 c, float r)
{
    for (uint k = 0; k < 6; k++)
    {
        float4 p = row(push.constants, at + k);
        if (dot(p.xyz, c) + p.w < -r)
        {
            return false;
        }
    }
    return true;
}

// Whether a sphere reaches into cascade `c`'s box.
bool reaches(uint c, float3 centre, float r)
{
    float4 q = transform(push.constants, C_SUN, float4(centre, 1.0));
    float4 across = row(push.constants, C_CASCADES + c * 2);
    float4 along = row(push.constants, C_CASCADES + c * 2 + 1);
    float a = -q.z;
    return q.x + r >= across.x && q.x - r <= across.y && q.y + r >= across.z && q.y - r <= across.w
        && a + r >= along.x && a - r <= along.y;
}

// The level a model at distance `d` draws, from its group's errors and distances at scale
// `scale`: the processor's `hand_overs` and `pick`.
Choice pick(uint group, uint levels, float scale, float d)
{
    Choice out;
    out.level = 0;
    out.next = 0;
    out.fade = 0.0;
    out.seen = false;
    out.casts = 0;
    float4 head = row(push.constants, C_EYE);
    float4 choice = row(push.constants, C_CHOICE);
    if (group == NO_GROUP || choice.z < 0.5)
    {
        return out;
    }
    uint4 g = row_u(push.groups, group * 5);
    if (g.y == 0)
    {
        return out;
    }
    float errors[MAX_LEVELS];
    float switches[MAX_LEVELS];
    for (uint k = 0; k < MAX_LEVELS; k++)
    {
        errors[k] = row(push.groups, group * 5 + 1 + k / 4)[k & 3];
        switches[k] = row(push.groups, group * 5 + 3 + k / 4)[k & 3];
    }
    float k_px = head.w;
    float error_px = choice.x;
    float band = max(choice.y, 0.0);
    uint n = min(levels, MAX_LEVELS);
    uint hands = n > 0 ? n - 1 : 0;
    float h[MAX_LEVELS];
    for (uint i = 0; i < MAX_LEVELS; i++)
    {
        h[i] = 0.0;
    }
    for (uint i = 0; i < hands; i++)
    {
        float next_error = i + 1 < n ? errors[i + 1] : 0.0;
        float strays = next_error * scale * k_px / max(error_px, 1e-3);
        float after = i > 0 ? h[i - 1] * (1.0 + band) : 0.0;
        h[i] = max(max(switches[i], strays), after);
    }
    uint level = 0;
    uint at = 0;
    while (at < hands)
    {
        float hv = h[at];
        if (d < hv)
        {
            break;
        }
        uint j = at;
        while (j + 1 < hands && h[j + 1] <= hv)
        {
            j += 1;
        }
        float end = min(hv * (1.0 + band), j + 1 < hands ? h[j + 1] : 3.402823466e38);
        if (d < end && end > hv)
        {
            out.level = at;
            out.next = j + 1;
            out.fade = saturate((d - hv) / (end - hv));
            return out;
        }
        level = j + 1;
        at = j + 1;
    }
    out.level = level;
    out.next = level;
    return out;
}

// Model `m`'s choice this frame, and how many commands each list takes from it.
Choice choose(uint m, out uint n[LISTS])
{
    for (uint l = 0; l < LISTS; l++)
    {
        n[l] = 0;
    }
    Choice c;
    c.level = 0;
    c.next = 0;
    c.fade = 0.0;
    c.seen = false;
    c.casts = 0;
    uint4 head = row_u(push.table, m * 4);
    uint4 counts = row_u(push.table, m * 4 + 1);
    if ((counts.z & M_LIVE) == 0)
    {
        return c;
    }
    float4 centre = row(push.table, m * 4 + 2);
    float4 sphere = row(push.table, m * 4 + 3);
    float3 offset = row(push.blocks, head.y).xyz;
    float3 eye = row(push.constants, C_EYE).xyz;
    c = pick(head.z, counts.w, centre.w, length(centre.xyz + offset - eye));
    // The processor fades a part only into a farther level.
    bool fading = c.fade > 0.0 && c.next > c.level;
    if (!fading)
    {
        c.fade = 0.0;
        c.next = c.level;
    }
    // Its fade, as the G-buffer's fade reads it: how far, and both levels in one number.
    uint row6 = (head.x * 8 + 6) * 16 + 8;
    RWByteAddressBuffer instances = rw_buffers[NonUniformResourceIndex(push.instances)];
    instances.Store2(row6, uint2(asuint(c.fade), asuint(float(c.level | (c.next << 8)))));

    float3 at = sphere.xyz + offset;
    uint flags = asuint(row(push.constants, C_CHOICE).w);
    bool drawn = ((buffers[NonUniformResourceIndex(push.drawn)].Load((head.y >> 5) * 4) >> (head.y & 31)) & 1) != 0;
    bool in_view = inside(C_VIEW, at, sphere.w);
    bool in_mirror = (flags & F_MIRROR) != 0 && inside(C_MIRROR, at, sphere.w);
    c.seen = drawn && (in_view || in_mirror);
    if (c.seen)
    {
        for (uint k = 0; k < (fading ? 2u : 1u); k++)
        {
            uint2 r = level_items(head, counts, k == 0 ? c.level : c.next);
            for (uint i = 0; i < r.y; i++)
            {
                uint bucket = buffers[NonUniformResourceIndex(push.items)].Load((r.x + i) * 32 + 28);
                if (bucket < 2)
                {
                    n[bucket] += 1;
                }
            }
        }
    }
    uint redraw = row_u(push.constants, C_LISTS).z;
    uint first = level_items(head, counts, 0).y;
    for (uint k = 0; k < CASCADES; k++)
    {
        bool casts = ((redraw >> k) & 1) != 0
            && ((counts.z & M_COVER) == 0 || k == 0)
            && ((flags & F_IN_VIEW) == 0 || in_view)
            && reaches(k, at, sphere.w);
        if (casts)
        {
            c.casts |= 1u << k;
            n[2 + k] = first;
        }
    }
    return c;
}

groupshared uint gs_sums[LISTS];

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_choose(uint3 id : SV_DispatchThreadID, uint3 local : SV_GroupThreadID, uint3 group : SV_GroupID)
{
    if (local.x < LISTS)
    {
        gs_sums[local.x] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    uint n[LISTS];
    for (uint l = 0; l < LISTS; l++)
    {
        n[l] = 0;
    }
    if (id.x < push.count)
    {
        Choice c = choose(id.x, n);
        RWByteAddressBuffer states = rw_buffers[NonUniformResourceIndex(push.states)];
        uint flags = (c.seen ? 1u : 0u) | (c.fade > 0.0 ? 2u : 0u) | (c.casts << 2);
        states.Store4(id.x * 16, uint4(c.level | (c.next << 8) | (flags << 16), asuint(c.fade), 0, 0));
        RWByteAddressBuffer tallies = rw_buffers[NonUniformResourceIndex(push.tallies)];
        tallies.Store4(id.x * 16, uint4(n[0] | (n[1] << 16), n[2] | (n[3] << 16), n[4] | (n[5] << 16), 0));
        uint unused;
        for (uint l = 0; l < LISTS; l++)
        {
            if (n[l] > 0)
            {
                InterlockedAdd(gs_sums[l], n[l], unused);
            }
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (local.x < LISTS)
    {
        rw_buffers[NonUniformResourceIndex(push.sums)].Store((group.x * 8 + local.x) * 4, gs_sums[local.x]);
    }
}

[shader("compute")]
[numthreads(8, 1, 1)]
void cs_scan(uint3 id : SV_DispatchThreadID)
{
    uint l = id.x;
    if (l >= LISTS)
    {
        return;
    }
    uint groups = (push.count + 63) / 64;
    RWByteAddressBuffer sums = rw_buffers[NonUniformResourceIndex(push.sums)];
    uint running = 0;
    for (uint g = 0; g < groups; g++)
    {
        uint v = sums.Load((g * 8 + l) * 4);
        sums.Store((g * 8 + l) * 4, running);
        running += v;
    }
    if (l < 2)
    {
        uint base = row_u(push.constants, C_LISTS)[l];
        rw_buffers[NonUniformResourceIndex(push.counts)].Store(l * 4, min(base + running, push.capacity));
    }
    else
    {
        rw_buffers[NonUniformResourceIndex(push.caster_counts)].Store((l - 2) * 4, min(running, push.caster_capacity));
    }
}

// Write one indexed indirect command at place `slot` of the list from command `first`: item
// `item`'s indices, once, the item as its first instance.
void command(uint buffer, uint at, uint item)
{
    uint4 a = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32);
    uint4 b = buffers[NonUniformResourceIndex(push.items)].Load4(item * 32 + 16);
    RWByteAddressBuffer commands = rw_buffers[NonUniformResourceIndex(buffer)];
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

groupshared uint gs_tallies[64 * LISTS];

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_write(uint3 id : SV_DispatchThreadID, uint3 local : SV_GroupThreadID, uint3 group : SV_GroupID)
{
    uint n[LISTS];
    for (uint l = 0; l < LISTS; l++)
    {
        n[l] = 0;
    }
    if (id.x < push.count)
    {
        uint4 t = buffers[NonUniformResourceIndex(push.tallies)].Load4(id.x * 16);
        n[0] = t.x & 0xFFFF;
        n[1] = t.x >> 16;
        n[2] = t.y & 0xFFFF;
        n[3] = t.y >> 16;
        n[4] = t.z & 0xFFFF;
        n[5] = t.z >> 16;
    }
    for (uint l = 0; l < LISTS; l++)
    {
        gs_tallies[local.x * LISTS + l] = n[l];
    }
    GroupMemoryBarrierWithGroupSync();
    if (id.x >= push.count)
    {
        return;
    }
    // Its first place in each list: its workgroup's, then the models before it in the workgroup.
    uint place[LISTS];
    for (uint l = 0; l < LISTS; l++)
    {
        uint before = buffers[NonUniformResourceIndex(push.sums)].Load((group.x * 8 + l) * 4);
        for (uint j = 0; j < local.x; j++)
        {
            before += gs_tallies[j * LISTS + l];
        }
        place[l] = before;
    }
    uint4 state = buffers[NonUniformResourceIndex(push.states)].Load4(id.x * 16);
    uint flags = state.x >> 16;
    uint4 head = row_u(push.table, id.x * 4);
    uint4 counts = row_u(push.table, id.x * 4 + 1);
    uint4 lists = row_u(push.constants, C_LISTS);
    if ((flags & 1) != 0)
    {
        uint level = state.x & 0xFF;
        uint next = (state.x >> 8) & 0xFF;
        bool fading = (flags & 2) != 0;
        for (uint k = 0; k < (fading ? 2u : 1u); k++)
        {
            uint2 r = level_items(head, counts, k == 0 ? level : next);
            for (uint i = 0; i < r.y; i++)
            {
                uint item = r.x + i;
                uint bucket = buffers[NonUniformResourceIndex(push.items)].Load(item * 32 + 28);
                if (bucket >= 2)
                {
                    continue;
                }
                uint slot = lists[bucket] + place[bucket];
                place[bucket] += 1;
                if (slot < push.capacity)
                {
                    command(push.commands, bucket * push.capacity + slot, item);
                }
            }
        }
    }
    uint2 first = level_items(head, counts, 0);
    for (uint c = 0; c < CASCADES; c++)
    {
        if (((flags >> (2 + c)) & 1) == 0)
        {
            continue;
        }
        for (uint i = 0; i < first.y; i++)
        {
            uint slot = place[2 + c] + i;
            if (slot < push.caster_capacity)
            {
                command(push.casters, c * push.caster_capacity + slot, first.x + i);
            }
        }
    }
}
