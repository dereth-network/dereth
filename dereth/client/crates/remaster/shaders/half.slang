// 16-bit floats held in 32-bit words, by the bits: the device is not asked for 16-bit types, so a
// shader can use neither those nor what the compiler makes of the conversions to them.
module half;

// The 16-bit float in the low bits of `h`, as a 32-bit float.
public float half_value(uint h)
{
    uint bits = h & 0x7FFF;
    float v;
    if (bits < 0x0400)
    {
        // Under the smallest normal number: a count of the smallest step, 2^-24.
        v = float(bits) * 5.9604644775390625e-8;
    }
    else if (bits < 0x7C00)
    {
        v = asfloat((bits << 13) + 0x38000000);
    }
    else
    {
        // Infinity or not a number, each as itself.
        v = asfloat((bits << 13) | 0x7F800000);
    }
    return (h & 0x8000) != 0 ? -v : v;
}

// `v` as a 16-bit float, rounded to nearest, in the low bits; past the largest it is infinity.
public uint half_bits(float v)
{
    uint u = asuint(v);
    uint sign = (u >> 16) & 0x8000;
    uint bits = u & 0x7FFFFFFF;
    uint h;
    if (bits > 0x7F800000)
    {
        h = 0x7E00;
    }
    else if (bits >= 0x47800000)
    {
        h = 0x7C00;
    }
    else if (bits >= 0x38800000)
    {
        // A round up past the mantissa carries into the exponent, as it should.
        h = (bits - 0x38000000 + 0x1000) >> 13;
    }
    else
    {
        h = uint(abs(v) * 16777216.0 + 0.5);
    }
    return h | sign;
}

// Two 16-bit floats in one word, the first in the low bits.
public uint halves(float a, float b)
{
    return half_bits(a) | (half_bits(b) << 16);
}

public float2 unhalves(uint w)
{
    return float2(half_value(w & 0xFFFF), half_value(w >> 16));
}
