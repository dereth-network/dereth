// Writes `value + i` into word `i` of read-write buffer `buffer`, for `count` words.
import table;

struct Push
{
    uint buffer;
    uint value;
    uint count;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    if (id.x < push.count)
    {
        rw_buffers[NonUniformResourceIndex(push.buffer)].Store(id.x * 4, push.value + id.x);
    }
}
