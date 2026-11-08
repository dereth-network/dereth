// What an upscaler takes from the frame, written into its buffers: each pixel's depth, and its
// motion in the picture's own units, where its surface was last frame less where it is now, both
// with the view unmoved by the frame's jitter.
//
// A pixel is placed by the depth the resolve places it by, the outdoor surfaces' depth or the
// frame's by its class. Its surface was last frame where the view of last frame had it, or, on the
// parts that moved, where the motion pass found them.
import bindless;
import half;
import retail;

struct Push
{
    uint frame;
    // The post passes' constants: rows 0..3 the last frame's view and projection, unmoved, in this
    // frame's space; row 4 xy how far this frame's projection is moved, in clip space, z 1 when the
    // view held still since the last frame.
    uint post;
    uint depth;
    uint outdoor_depth;
    uint material;
    // The moving parts' last places, in clip space halved, -1 where nothing moved (~0: none).
    uint motion;
    // The motion, two 16-bit floats a pixel, and the depth, a 32-bit float a pixel.
    uint out_motion;
    uint out_depth;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

float2 uv_of(float2 ndc)
{
    return float2((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    int2 p = int2(id.xy);
    int2 size = int2(extent);
    if (any(p >= size))
    {
        return;
    }
    float4 state = row(push.post, 4);
    uint cls = uint(at(push.material, p).x * 255.0 + 0.5);
    float d = at((cls == CLASS_GROUND || cls == CLASS_SUN) ? push.outdoor_depth : push.depth, p).x;

    float2 pos = float2(p) + 0.5;
    float2 ndc = float2(pos.x / extent.x * 2.0 - 1.0, 1.0 - pos.y / extent.y * 2.0);
    // Where the pixel's surface stands with the view unmoved.
    float2 now = uv_of(ndc - state.xy);

    float2 moved = push.motion != 0xFFFFFFFF ? at(push.motion, p).xy : float2(-1.0, -1.0);
    bool moving = moved.x > -0.99995 || moved.y > -0.99995;
    float2 motion = float2(0.0, 0.0);
    if (moving)
    {
        motion = uv_of(moved * 2.0) - now;
    }
    else if (state.z < 0.5)
    {
        float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
        float4 c = transform(push.post, 0, float4(h.xyz / h.w, 1.0));
        // A point behind last frame's eye was on no picture: it reprojects off this one.
        motion = c.w > 0.0 ? uv_of(c.xy / c.w) - now : float2(4.0, 4.0);
    }

    uint at_pixel = uint(p.y * size.x + p.x) * 4;
    rw_buffers[NonUniformResourceIndex(push.out_motion)].Store(at_pixel, halves(motion.x, motion.y));
    rw_buffers[NonUniformResourceIndex(push.out_depth)].Store(at_pixel, asuint(d));
}
