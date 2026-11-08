// Where an instance stands, and how far it is drawn above where it stands on the ground drawn
// over retail's: the G-buffer's own arithmetic, and a probe of it for the checks.
import bindless;
import retail;

// An instance's rows: its transform's four columns, its per-axis scale's inverse square, its
// block slot, class, light set and override, how far its texture has scrolled, and how far it is
// drawn above where it stands on the near ground and the far.
public static const uint INSTANCE_ROWS = 8;

// Where instance `instance` stands, the frame's space: its transform's origin, moved by its
// block's offset when it stands in one.
public float3 instance_origin(uint instances, uint blocks, uint instance)
{
    uint at = instance * INSTANCE_ROWS;
    float3 origin = row(instances, at + 3).xyz;
    uint slot = row_u(instances, at + 5).x;
    if (slot != 0xFFFFFFFF)
    {
        origin += row(blocks, slot).xyz;
    }
    return origin;
}

// How far instance `instance`, standing at `origin` (the frame's space), is drawn above where it
// stands: its lift on the near ground and on the far, mixed by its distance from the eye as the
// ground mixes them.
public float lifted(uint frame, uint instances, uint instance, float3 origin)
{
    float4 l = row(instances, instance * INSTANCE_ROWS + 7);
    if (l.x == 0.0 && l.y == 0.0)
    {
        return 0.0;
    }
    float4 m = row(frame, FRAME_LIFT);
    float far = 0.0;
    if (m.z > 0.5)
    {
        float d = length(origin - row(frame, FRAME_EYE).xyz);
        far = smoothstep(m.x, max(m.y, m.x + 1e-3), d);
    }
    return lerp(l.x, l.y, far);
}

struct ProbePush
{
    uint frame;
    uint instances;
    uint blocks;
    // The instances asked about: their count, then one a word.
    uint list;
    // For each, where it stands and how far it is lifted.
    uint out;
};

// Where each listed instance stands and how far it is drawn above it, as the G-buffer places it.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_bases(uint3 id : SV_DispatchThreadID, uniform ProbePush push)
{
    uint count = buffers[NonUniformResourceIndex(push.list)].Load(0);
    if (id.x >= count)
    {
        return;
    }
    uint instance = buffers[NonUniformResourceIndex(push.list)].Load(16 + id.x * 4);
    float3 o = instance_origin(push.instances, push.blocks, instance);
    float l = lifted(push.frame, push.instances, instance, o);
    rw_buffers[NonUniformResourceIndex(push.out)].Store4(id.x * 16, asuint(float4(o, l)));
}
