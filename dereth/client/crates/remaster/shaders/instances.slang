// The blocks' parts chosen and listed on the device: each one's level of detail, its fade into the
// next, how much of it its impostor draws, whether it is seen, which of the sun's cascades it casts
// into, and the indexed indirect commands that draw it, every frame, in three passes.
//
// - `cs_choose`, one part a thread: its level and fade from its distance, as the processor chooses
//   a degrading part's (`render/lod.rs`): its group's hand-overs at its own scale, kept from levels
//   coarser than retail's own and passed to its impostor where it is a tree; the fade written into
//   its instance's row, as the G-buffer's fade reads it; whether it is seen (its block drawn, its
//   sphere in the view or the mirrored view); the cascades it reaches; whether its impostor's card
//   is drawn; and how many commands each list takes from it, summed over its workgroup.
// - `cs_scan`, one workgroup: each list's workgroup sums made into each workgroup's first place,
//   and each list's count written, the cards' as the instances of their indirect draw.
// - `cs_write`, one part a thread: its first place in each list (its workgroup's, plus the parts
//   before it in the workgroup), and its commands and its card written there.
//
// A list's commands are in the parts' order, frame after frame: no race decides a place. The
// G-buffer's two culled buckets take the parts' commands after the processor's own, from `base`;
// each cascade has a list of its own, its parts' first levels, drawn into its cache. Ground cover
// casts into the nearest cascade alone.
import bindless;
import retail;
import lift;

struct Push
{
    // The parts: four rows each, `count` of them.
    uint table;
    uint count;
    // Each group's hand-overs: eight rows each.
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
    // Each part's command counts, a row each, and each workgroup's sums, two rows each.
    uint tallies;
    uint sums;
    // Each part's choice, a row each, for the checks.
    uint states;
    // The cascades' commands, `caster_capacity` to a cascade, and their counts.
    uint casters;
    uint caster_counts;
    uint caster_capacity;
    // The records the parts degrade by, two rows each, and retail's level of each, a word each.
    uint records;
    uint retail;
    // The trees' pictures in the atlas, two rows each.
    uint trees;
    // The cards, five rows each, `card_capacity` of them; the part each stands for, a word each;
    // and the cards' indirect draw.
    uint cards;
    uint card_parts;
    uint card_capacity;
    uint card_draw;
    // The frame's own constants, as the G-buffer reads them: the eye and how far a lift reaches.
    uint frame;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

// The constants' rows: the eye and the pixels a meter covers at a meter; the error bar, the fade's
// band, whether levels are chosen, and the flags; the processor's commands in each bucket, the
// cascades drawn again and the groups' count; how far out a tree is wholly its impostor; the
// view's six planes, the mirrored view's six; the sun's view; each cascade's box across the sun
// and along it.
static const uint C_EYE = 0;
static const uint C_CHOICE = 1;
static const uint C_LISTS = 2;
static const uint C_CAP = 3;
static const uint C_VIEW = 4;
static const uint C_MIRROR = 10;
static const uint C_SUN = 16;
static const uint C_CASCADES = 20;

// The flags: the mirrored view is drawn; the casters are those in the view alone.
static const uint F_MIRROR = 1;
static const uint F_IN_VIEW = 2;

// A part's flags: it is ground cover; it is held.
static const uint M_COVER = 1;
static const uint M_LIVE = 2;

// A group's flags: its errors are measured; its own hand-over distances grow with the part's
// scale; its last level is a tree's impostor.
static const uint G_MEASURED = 1;
static const uint G_SCALED = 2;
static const uint G_IMPOSTOR = 4;

// No record, group or tree.
static const uint NONE = 0xFFFFFFFF;

// The lists: the two culled buckets, the four cascades, then the cards.
static const uint LISTS = 7;
static const uint CASCADES = 4;
static const uint CARDS = 6;
static const uint MAX_LEVELS = 8;
static const uint MAX_HANDS = 8;

// A group's rows: its hand-overs' count, flags, impostor level and tree; its levels' errors,
// twelve; its hand-overs' own distances, eight.
static const uint GROUP_ROWS = 8;

// One part's choice.
struct Choice
{
    uint level;
    uint next;
    float fade;
    // How much of it its impostor draws: 0 none, 1 all of it; and its tree's pictures' row.
    float share;
    uint tree;
    bool seen;
    // Its block is drawn.
    bool drawn;
    // A bit a cascade it casts into.
    uint casts;
};

Choice nothing()
{
    Choice c;
    c.level = 0;
    c.next = 0;
    c.fade = 0.0;
    c.share = 0.0;
    c.tree = NONE;
    c.seen = false;
    c.drawn = false;
    c.casts = 0;
    return c;
}

// How many items a part's level `l` draws, and the first of them: none past its last level.
uint2 level_items(uint4 head, uint4 counts, uint l)
{
    if (l >= min(counts.w, MAX_LEVELS))
    {
        return uint2(head.w, 0);
    }
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

// The processor's arithmetic, rounded as it rounds it: each sum and product rounded on its own,
// never fused, and each quotient and square root the nearest float to the true one. A part's
// distance, hand-overs and fade, and a card the device writes, are then the processor's to the
// bit, so a part on the edge of a hand-over takes the level the processor would give it.

// `a` split into two halves whose product with another's halves is exact.
void halves(float a, out float hi, out float lo)
{
    precise float c = 4097.0 * a;
    precise float big = c - a;
    precise float h = c - big;
    precise float l = a - h;
    hi = h;
    lo = l;
}

// `x - q * y`, to well within the last place of the product.
float residue(float x, float q, float y)
{
    precise float p = q * y;
    float qh, ql, yh, yl;
    halves(q, qh, ql);
    halves(y, yh, yl);
    precise float e = ((qh * yh - p) + qh * yl + ql * yh) + ql * yl;
    precise float r = (x - p) - e;
    return r;
}

// The float nearest `x / y`: the device's quotient, or a neighbour of it nearer.
float quotient(float x, float y)
{
    float q = x / y;
    float best = q;
    float least = abs(residue(x, q, y));
    for (int k = 1; k <= 2; k++)
    {
        for (int s = -1; s <= 1; s += 2)
        {
            float c = asfloat(asint(q) + s * k);
            float r = abs(residue(x, c, y));
            if (r < least)
            {
                best = c;
                least = r;
            }
        }
    }
    return best;
}

// The float nearest the square root of `x`, for `x` positive.
float root(float x)
{
    float q = sqrt(x);
    float best = q;
    float least = abs(residue(x, q, q));
    for (int k = 1; k <= 2; k++)
    {
        for (int s = -1; s <= 1; s += 2)
        {
            float c = asfloat(asint(q) + s * k);
            float r = abs(residue(x, c, c));
            if (r < least)
            {
                best = c;
                least = r;
            }
        }
    }
    return best;
}

// What a part of group `group` at scale `scale` and distance `d` draws by its own hand-overs alone:
// the processor's `hand_overs`, `pull_in` and `pick`.
Choice pick(uint group, float scale, float d)
{
    Choice out = nothing();
    float4 head = row(push.constants, C_EYE);
    float4 choice = row(push.constants, C_CHOICE);
    uint4 g = row_u(push.groups, group * GROUP_ROWS);
    uint hands = min(g.x, MAX_HANDS);
    float k_px = head.w;
    float error_px = max(choice.x, 1e-3);
    float band = max(choice.y, 0.0);
    precise float grow = 1.0 + band;
    float h[MAX_HANDS];
    for (uint i = 0; i < MAX_HANDS; i++)
    {
        h[i] = 0.0;
    }
    for (uint i = 0; i < hands; i++)
    {
        float next_error = row(push.groups, group * GROUP_ROWS + 1 + (i + 1) / 4)[(i + 1) & 3];
        precise float own = row(push.groups, group * GROUP_ROWS + 4 + i / 4)[i & 3];
        if ((g.y & G_SCALED) != 0)
        {
            own = own * scale;
        }
        precise float scaled = next_error * scale;
        precise float seen = scaled * k_px;
        float strays = quotient(seen, error_px);
        precise float after = i > 0 ? h[i - 1] * grow : 0.0;
        h[i] = max(max(own, strays), after);
    }
    // A tree is wholly its impostor before it could stand where the far forest draws it: a
    // hand-over past the cap comes at it, and a level that would leave no room to fade before it
    // is passed over.
    float cap = row(push.constants, C_CAP).x;
    if ((g.y & G_IMPOSTOR) != 0 && hands > 0 && cap < 3.402823466e38)
    {
        float last = min(h[hands - 1], cap);
        h[hands - 1] = last;
        float room = quotient(last, grow);
        for (int i = int(hands) - 2; i >= 0; i--)
        {
            if (h[i] <= room)
            {
                break;
            }
            h[i] = last;
        }
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
        precise float far = hv * grow;
        float end = min(far, j + 1 < hands ? h[j + 1] : 3.402823466e38);
        if (d < end && end > hv)
        {
            precise float into = d - hv;
            precise float span = end - hv;
            out.level = at;
            out.next = j + 1;
            out.fade = clamp(quotient(into, span), 0.0, 1.0);
            return out;
        }
        level = j + 1;
        at = j + 1;
    }
    out.level = level;
    out.next = level;
    return out;
}

// What record `record` has its part draw, its block standing at `offset`: the processor's
// `BlockLod::part` for a part standing outdoors.
Choice degrade(uint record, float3 offset)
{
    Choice c = nothing();
    if (record == NONE)
    {
        return c;
    }
    uint walk = buffers[NonUniformResourceIndex(push.retail)].Load(record * 4);
    uint retail = walk & 0xFF;
    bool walked = (walk & 0x100) != 0;
    float4 r0 = row(push.records, record * 2);
    uint group = row_u(push.records, record * 2 + 1).x;
    // Levels are chosen while the settings choose them, and past the blocks retail stands objects
    // on round the viewer's (the ring round it) whatever they say: retail draws nothing there.
    bool past_retail = max(abs(offset.x), abs(offset.y)) > 1.5 * 192.0;
    bool on = row(push.constants, C_CHOICE).z > 0.5 || past_retail;
    uint4 g = group == NONE ? uint4(0, 0, NONE, NONE) : row_u(push.groups, group * GROUP_ROWS);
    if (!on || group == NONE || (g.y & G_MEASURED) == 0)
    {
        // Retail's level, as the walk chose it.
        c.level = retail;
        c.next = retail;
        return c;
    }
    // Its distance, as the processor measures it: the record's centre moved by its block's offset,
    // less the eye.
    float3 eye = row(push.constants, C_EYE).xyz;
    precise float3 at = r0.xyz + offset;
    precise float3 v = at - eye;
    precise float dd = (v.x * v.x + v.y * v.y) + v.z * v.z;
    Choice p = pick(group, r0.w, root(dd));
    uint impostor = g.z;
    // Retail's level keeps the part from coarser levels; the impostor may stand in once retail
    // draws the last level before it.
    uint most = retail;
    if (!walked)
    {
        most = 0xFF;
    }
    else if (impostor != NONE && retail + 1 >= impostor)
    {
        most = max(retail, impostor);
    }
    c.level = min(p.level, most);
    bool fading = p.fade > 0.0 && p.next > p.level && p.next <= most && c.level == p.level;
    c.next = fading ? p.next : c.level;
    c.fade = fading ? p.fade : 0.0;
    if (impostor != NONE)
    {
        c.share = c.level == impostor ? 1.0 : (fading && c.next == impostor ? c.fade : 0.0);
        c.tree = g.w;
    }
    return c;
}

// Part `m`'s choice this frame, and how many commands each list takes from it.
Choice choose(uint m, out uint n[LISTS])
{
    for (uint l = 0; l < LISTS; l++)
    {
        n[l] = 0;
    }
    uint4 head = row_u(push.table, m * 4);
    uint4 counts = row_u(push.table, m * 4 + 1);
    if ((counts.z & M_LIVE) == 0)
    {
        return nothing();
    }
    float4 sphere = row(push.table, m * 4 + 2);
    float3 offset = row(push.blocks, head.y).xyz;
    Choice c = degrade(head.z, offset);
    bool fading = c.fade > 0.0;
    // Its fade, as the G-buffer's fade reads it: how far, and both levels in one number.
    uint row6 = (head.x * INSTANCE_ROWS + 6) * 16 + 8;
    RWByteAddressBuffer instances = rw_buffers[NonUniformResourceIndex(push.instances)];
    instances.Store2(row6, uint2(asuint(c.fade), asuint(float(c.level | (c.next << 8)))));

    float3 at = sphere.xyz + offset;
    uint flags = asuint(row(push.constants, C_CHOICE).w);
    c.drawn = ((buffers[NonUniformResourceIndex(push.drawn)].Load((head.y >> 5) * 4) >> (head.y & 31)) & 1) != 0;
    bool in_view = inside(C_VIEW, at, sphere.w);
    bool in_mirror = (flags & F_MIRROR) != 0 && inside(C_MIRROR, at, sphere.w);
    c.seen = c.drawn && (in_view || in_mirror);
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
    // Its impostor's card, in the view, where its tree's pictures have a row: the mirrored view
    // draws no cards.
    if (c.share > 0.0 && c.tree != NONE && c.drawn && in_view)
    {
        n[CARDS] = 1;
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
        uint flags = (c.seen ? 1u : 0u) | (c.fade > 0.0 ? 2u : 0u) | (c.casts << 2)
            | (c.drawn ? 64u : 0u) | (n[CARDS] != 0 ? 128u : 0u);
        states.Store4(id.x * 16, uint4(c.level | (c.next << 8) | (flags << 16), asuint(c.fade), asuint(c.share), c.tree));
        RWByteAddressBuffer tallies = rw_buffers[NonUniformResourceIndex(push.tallies)];
        tallies.Store4(id.x * 16, uint4(n[0] | (n[1] << 16), n[2] | (n[3] << 16), n[4] | (n[5] << 16), n[6]));
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
    else if (l < CARDS)
    {
        rw_buffers[NonUniformResourceIndex(push.caster_counts)].Store((l - 2) * 4, min(running, push.caster_capacity));
    }
    else
    {
        // The cards' draw: six corners, a card an instance.
        RWByteAddressBuffer draw = rw_buffers[NonUniformResourceIndex(push.card_draw)];
        uint cards = min(running, push.card_capacity);
#ifdef DERETH_DXIL
        // Direct3D 12's command: the draw's item, set as its root constant, then
        // `D3D12_DRAW_ARGUMENTS`.
        draw.Store(0, 0);
        draw.Store4(4, uint4(6, cards, 0, 0));
#else
        draw.Store4(0, uint4(6, cards, 0, 0));
#endif
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

// How far instance `instance`, standing at `origin` (the frame's space), is lifted onto the ground:
// its lifts on the near ground and the far mixed by its distance from the eye, as the processor
// works a card's lift out (`render/light.rs`, `lifted`).
float card_lift(uint instance, float3 origin)
{
    float4 l = row(push.instances, instance * INSTANCE_ROWS + 7);
    if (l.x == 0.0 && l.y == 0.0)
    {
        return 0.0;
    }
    float4 m = row(push.frame, FRAME_LIFT);
    precise float t = 0.0;
    if (m.z > 0.5)
    {
        float3 eye = row(push.frame, FRAME_EYE).xyz;
        precise float dx = origin.x - eye.x;
        precise float dy = origin.y - eye.y;
        precise float dz = origin.z - eye.z;
        precise float dd = (dx * dx + dy * dy) + dz * dz;
        float d = root(dd);
        precise float floor_end = m.x + 1e-3;
        float end = max(m.y, floor_end);
        precise float over = d - m.x;
        precise float span = end - m.x;
        float x = clamp(quotient(over, span), 0.0, 1.0);
        precise float s = (x * x) * (3.0 - 2.0 * x);
        t = s;
    }
    precise float lift = l.x + (l.y - l.x) * t;
    return lift;
}

// Write part `m`'s impostor card at place `at`, drawing `share` of the tree whose pictures are at
// row `tree` of the trees' table: its pictures' sphere as its transform places it (worked out on
// the processor as its block came, a row of the part's), moved to the frame's space and lifted
// onto the ground as the tree is; its axes; and where its pictures are in the atlas.
void card(uint m, uint at, uint4 head, float share, uint tree)
{
    uint4 words = row_u(push.trees, tree * 2);
    float4 sphere = row(push.table, m * 4 + 3);
    uint i = head.x * INSTANCE_ROWS;
    float3 ax = row(push.instances, i).xyz;
    float3 ay = row(push.instances, i + 1).xyz;
    float3 az = row(push.instances, i + 2).xyz;
    float3 offset = row(push.blocks, head.y).xyz;
    precise float3 base = row(push.instances, i + 3).xyz + offset;
    float lift = card_lift(head.x, base);
    precise float3 placed = sphere.xyz + offset;
    precise float3 centre = placed + float3(0.0, 0.0, lift);
    RWByteAddressBuffer cards = rw_buffers[NonUniformResourceIndex(push.cards)];
    uint byte = at * 80;
    cards.Store4(byte, asuint(float4(centre, sphere.w)));
    cards.Store4(byte + 16, asuint(float4(ax, share)));
    // The kind: foliage.
    cards.Store4(byte + 32, asuint(float4(ay, 1.0)));
    cards.Store4(byte + 48, asuint(float4(az, 0.0)));
    cards.Store4(byte + 64, words);
    rw_buffers[NonUniformResourceIndex(push.card_parts)].Store(at * 4, m);
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
        n[6] = t.w;
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
    // Its first place in each list: its workgroup's, then the parts before it in the workgroup.
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
    if ((flags & 128) != 0 && place[CARDS] < push.card_capacity)
    {
        card(id.x, place[CARDS], head, asfloat(state.z), state.w);
    }
}
