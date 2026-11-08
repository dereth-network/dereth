// AMD FidelityFX Super Resolution 3.1's upscaler, ported from the FidelityFX SDK 1.1.4 (MIT; its
// notice is in `fsr3_LICENSE.txt` beside this file) to run on the remaster's own table: its
// passes are the SDK's, their arithmetic the SDK's own, in 32-bit floats. What differs is only
// where things are kept:
// - every picture the upscaler makes for itself is a storage buffer, a pixel a row-major
//   element (16-bit floats packed in 32-bit words where the SDK keeps half floats), and a
//   picture the SDK samples through a bilinear sampler is sampled here by the same four loads
//   weighed by hand; the frame's color stays a texture;
// - the shading change's pyramid is built a level at a time rather than by the single-pass
//   downsampler, each level the mean of four of the one before, as that downsampler makes them,
//   and only the levels read are made; the luma pyramid's only reader here is the farthest depth
//   at half size, as the exposure is found by a pass of this port's own;
// - the reactive mask is this port's own, from what the blended surfaces changed; there is no
//   composition mask, so the SDK's default for it, zero, is written
//   in place of the loads;
// - every size is the frame's own (the largest render and upscale sizes are the current ones,
//   and a new size starts the upscaler again).
//
// The passes, in the SDK's order (each a compute entry point in `fsr3_passes.slang`):
// prepare the inputs (dilated depth and motion, the depth reprojected into the last frame, the
// farthest depth and the luma), the farthest depth at half size, the shading change's pyramid and
// the shading change, the reactivity (disocclusion, shading change, accumulation and new locks),
// the luma instability, the accumulation (the upscaled color), and the sharpening (RCAS).
module fsr3;

import bindless;
import half;
import retail;

// --- the constants ------------------------------------------------------------------------
//
// In buffer `c`, float4 rows (ints as their bits, indices as uints):
//   0  the render size, the last frame's render size (ints)
//   1  the upscale size, the last frame's upscale size (ints)
//   2  the device-to-view depth factors
//   3  the jitter, the last frame's jitter (render pixels)
//   4  the motion vectors' scale, the downscale factor (render over upscale)
//   5  the motion vectors' jitter cancellation, unused, the jitter sequence's length
//   6  the change of pre-exposure, view space to meters, the frame's number since the upscaler
//      started, 1 where the colors are linear light and unclamped (the HDR path)
//   7  the velocity factor, the accumulation added a frame, the least accumulation on
//      disocclusion, the sharpness as RCAS takes it
//   8  unused
//   9  the frame's color texture; the input motion and depth; the dilated motion
//   10 the dilated depth; the reconstructed depth; the farthest depth, or the instability after
//      it; the farthest depth at half size
//   11 the luma this frame and last; the luma history written and read
//   12 the accumulation written and read; the reactivity; the shading change
//   13 the shading change's pyramid, levels 0, 1 and 2; the new locks
//   14 the upscaled color written and read; the sharpened output; what is shown
//   15 the opaque world's colors; the reactive mask; the exposure found
public struct Pictures
{
    public uint color;
    public uint input_motion;
    public uint input_depth;
    public uint dilated_motion;
    public uint dilated_depth;
    public uint reconstructed;
    public uint intermediate;
    public uint farthest_half;
    public uint luma_now;
    public uint luma_last;
    public uint history_write;
    public uint history_read;
    public uint accumulation_write;
    public uint accumulation_read;
    public uint reactive;
    // What was drawn before the blended surfaces, how much of each pixel they changed, and the
    // exposure found from the frame.
    public uint opaque;
    public uint input_reactive;
    public uint exposure;
    public uint shading_change;
    public uint level0;
    public uint level1;
    public uint level2;
    public uint new_locks;
    public uint upscaled_write;
    public uint upscaled_read;
    public uint output;
    public uint shown;
};

public struct Constants
{
    public int2 render_size;
    public int2 previous_render_size;
    public int2 upscale_size;
    public int2 previous_upscale_size;
    public float4 device_to_view;
    public float2 jitter;
    public float2 previous_jitter;
    public float2 motion_scale;
    public float2 downscale;
    public float2 motion_cancellation;
    public float jitter_length;
    public float delta_pre_exposure;
    public float view_to_meters;
    public float frame_index;
    public bool hdr;
    public float velocity_factor;
    public float accumulation_added;
    public float min_disocclusion;
    public float sharpness;
    public float exposure;
};

public static Constants g = {};
public static Pictures pictures = {};

int2 int2_of(float2 v)
{
    return int2(asint(v.x), asint(v.y));
}

// Read the constants every function reads, from buffer `c`. Each entry point does this first.
public void use_constants(uint c)
{
    float4 r0 = row(c, 0);
    float4 r1 = row(c, 1);
    float4 r3 = row(c, 3);
    float4 r4 = row(c, 4);
    float4 r5 = row(c, 5);
    float4 r6 = row(c, 6);
    float4 r7 = row(c, 7);
    g.render_size = int2_of(r0.xy);
    g.previous_render_size = int2_of(r0.zw);
    g.upscale_size = int2_of(r1.xy);
    g.previous_upscale_size = int2_of(r1.zw);
    g.device_to_view = row(c, 2);
    g.jitter = r3.xy;
    g.previous_jitter = r3.zw;
    g.motion_scale = r4.xy;
    g.downscale = r4.zw;
    g.motion_cancellation = r5.xy;
    g.jitter_length = r5.w;
    g.delta_pre_exposure = r6.x;
    g.view_to_meters = r6.y;
    g.frame_index = r6.z;
    g.hdr = r6.w > 0.5;
    g.velocity_factor = r7.x;
    g.accumulation_added = r7.y;
    g.min_disocclusion = r7.z;
    g.sharpness = r7.w;
    // The exposure the upscaler works its colors at, as the frame's own exposure pass found it
    // (1 until it has).
    uint4 u6 = row_u(c, 15);
    pictures.opaque = u6.x;
    pictures.input_reactive = u6.y;
    pictures.exposure = u6.z;
    float e = asfloat(buffers[NonUniformResourceIndex(u6.z)].Load(0));
    g.exposure = e == 0.0 ? 1.0 : e;
    uint4 u0 = row_u(c, 9);
    uint4 u1 = row_u(c, 10);
    uint4 u2 = row_u(c, 11);
    uint4 u3 = row_u(c, 12);
    uint4 u4 = row_u(c, 13);
    uint4 u5 = row_u(c, 14);
    pictures.color = u0.x;
    pictures.input_motion = u0.y;
    pictures.input_depth = u0.z;
    pictures.dilated_motion = u0.w;
    pictures.dilated_depth = u1.x;
    pictures.reconstructed = u1.y;
    pictures.intermediate = u1.z;
    pictures.farthest_half = u1.w;
    pictures.luma_now = u2.x;
    pictures.luma_last = u2.y;
    pictures.history_write = u2.z;
    pictures.history_read = u2.w;
    pictures.accumulation_write = u3.x;
    pictures.accumulation_read = u3.y;
    pictures.reactive = u3.z;
    pictures.shading_change = u3.w;
    pictures.level0 = u4.x;
    pictures.level1 = u4.y;
    pictures.level2 = u4.z;
    pictures.new_locks = u4.w;
    pictures.upscaled_write = u5.x;
    pictures.upscaled_read = u5.y;
    pictures.output = u5.z;
    pictures.shown = u5.w;
}

public int2 RenderSize() { return g.render_size; }
public int2 PreviousFrameRenderSize() { return g.previous_render_size; }
public int2 MaxRenderSize() { return g.render_size; }
public int2 UpscaleSize() { return g.upscale_size; }
public int2 PreviousFrameUpscaleSize() { return g.previous_upscale_size; }
public float4 DeviceToViewSpaceTransformFactors() { return g.device_to_view; }
public float2 Jitter() { return g.jitter; }
public float2 PreviousFrameJitter() { return g.previous_jitter; }
public float2 MotionVectorScale() { return g.motion_scale; }
public float2 DownscaleFactor() { return g.downscale; }
public float2 MotionVectorJitterCancellation() { return g.motion_cancellation; }
public float JitterSequenceLength() { return g.jitter_length; }
public float DeltaPreExposure() { return g.delta_pre_exposure; }
public float ViewSpaceToMetersFactor() { return g.view_to_meters; }
public float FrameIndex() { return g.frame_index; }
public float VelocityFactor() { return g.velocity_factor; }
public float AccumulationAddedPerFrame() { return g.accumulation_added; }
public float MinDisocclusionAccumulation() { return g.min_disocclusion; }
public float RcasSharpness() { return g.sharpness; }
public bool HdrInput() { return g.hdr; }
public float Exposure() { return g.exposure; }

// --- pictures kept in buffers ---------------------------------------------------------------

// The element of pixel `p` of a picture `w` wide.
public uint element(int2 p, int w)
{
    return uint(p.y * w + p.x);
}

bool inside(int2 p, int2 size)
{
    return all(p >= int2(0, 0)) && all(p < size);
}

public float load1(uint b, int2 p, int2 size)
{
    int2 q = clamp(p, int2(0, 0), size - 1);
    return asfloat(buffers[NonUniformResourceIndex(b)].Load(element(q, size.x) * 4));
}

// A store past the picture's edge goes nowhere, as one into a texture does.
public void store1(uint b, int2 p, int2 size, float v)
{
    if (inside(p, size))
    {
        rw_buffers[NonUniformResourceIndex(b)].Store(element(p, size.x) * 4, asuint(v));
    }
}

public float2 load2(uint b, int2 p, int2 size)
{
    int2 q = clamp(p, int2(0, 0), size - 1);
    return unhalves(buffers[NonUniformResourceIndex(b)].Load(element(q, size.x) * 4));
}

public void store2(uint b, int2 p, int2 size, float2 v)
{
    if (inside(p, size))
    {
        rw_buffers[NonUniformResourceIndex(b)].Store(element(p, size.x) * 4, halves(v.x, v.y));
    }
}

public float4 load4(uint b, int2 p, int2 size)
{
    int2 q = clamp(p, int2(0, 0), size - 1);
    uint2 w = buffers[NonUniformResourceIndex(b)].Load2(element(q, size.x) * 8);
    return float4(unhalves(w.x), unhalves(w.y));
}

public void store4(uint b, int2 p, int2 size, float4 v)
{
    if (inside(p, size))
    {
        rw_buffers[NonUniformResourceIndex(b)].Store2(element(p, size.x) * 8,
            uint2(halves(v.x, v.y), halves(v.z, v.w)));
    }
}

// A picture sampled as a bilinear sampler clamped at its edges samples it, at `uv`.
public float sample1(uint b, float2 uv, int2 size)
{
    float2 p = uv * float2(size) - 0.5;
    int2 base = int2(floor(p));
    float2 f = p - floor(p);
    float a = lerp(load1(b, base, size), load1(b, base + int2(1, 0), size), f.x);
    float c = lerp(load1(b, base + int2(0, 1), size), load1(b, base + int2(1, 1), size), f.x);
    return lerp(a, c, f.y);
}

public float2 sample2(uint b, float2 uv, int2 size)
{
    float2 p = uv * float2(size) - 0.5;
    int2 base = int2(floor(p));
    float2 f = p - floor(p);
    float2 a = lerp(load2(b, base, size), load2(b, base + int2(1, 0), size), f.x);
    float2 c = lerp(load2(b, base + int2(0, 1), size), load2(b, base + int2(1, 1), size), f.x);
    return lerp(a, c, f.y);
}

public float4 sample4(uint b, float2 uv, int2 size)
{
    float2 p = uv * float2(size) - 0.5;
    int2 base = int2(floor(p));
    float2 f = p - floor(p);
    float4 a = lerp(load4(b, base, size), load4(b, base + int2(1, 0), size), f.x);
    float4 c = lerp(load4(b, base + int2(0, 1), size), load4(b, base + int2(1, 1), size), f.x);
    return lerp(a, c, f.y);
}

// --- the SDK's common arithmetic (`ffx_fsr3upscaler_common.h`) --------------------------

public static const float FSR3UPSCALER_FP16_MIN = 6.10e-05;
public static const float FSR3UPSCALER_FP16_MAX = 65504.0;
public static const float FSR3UPSCALER_EPSILON = FSR3UPSCALER_FP16_MIN;
public static const float FSR3UPSCALER_TONEMAP_EPSILON = FSR3UPSCALER_FP16_MIN;
public static const float FSR3UPSCALER_FP32_MAX = 3.402823466e+38;
public static const float FSR3UPSCALER_FP32_MIN = 1.175494351e-38;

public static const float fReconstructedDepthBilinearWeightThreshold = FSR3UPSCALER_EPSILON * 10.0;

public float ReconstructedDepthMvPxThreshold(float fNearestDepthInMeters)
{
    return lerp(0.25, 0.75, saturate(fNearestDepthInMeters / 100.0));
}

public static const float fUpsampleLanczosWeightScale = 1.0 / 16.0;
public static const float fAverageLanczosWeightPerFrame = 0.74 * fUpsampleLanczosWeightScale;
public static const float fAccumulationMaxOnMotion = 3.0 * fUpsampleLanczosWeightScale;

public static const int SHADING_CHANGE_SET_SIZE = 5;

public static const float fLockThreshold = 1.0;
public static const float fLockMax = 2.0;

public static const int REACTIVE = 0;
public static const int DISOCCLUSION = 1;
public static const int SHADING_CHANGE = 2;
public static const int ACCUMULAION = 3;

public struct AccumulationPassCommonParams
{
    public int2 iPxHrPos;
    public float2 fHrUv;
    public float2 fLrUvJittered;
    public float2 fLrUv_HwSampler;
    public float2 fMotionVector;
    public float2 fReprojectedHrUv;
    public float f4KVelocity;
    public float fDisocclusion;
    public float fReactiveMask;
    public float fShadingChange;
    public float fAccumulation;
    public float fLumaInstabilityFactor;
    public float fFarthestDepthInMeters;
    public bool bIsExistingSample;
    public bool bIsNewSample;
};

public float Get4KVelocity(float2 fMotionVector)
{
    return length(fMotionVector * float2(3840.0, 2160.0));
}

public struct RectificationBox
{
    public float3 boxCenter;
    public float3 boxVec;
    public float3 aabbMin;
    public float3 aabbMax;
    public float fBoxCenterWeight;
};

public struct AccumulationPassData
{
    public RectificationBox clippingBox;
    public float3 fUpsampledColor;
    public float fUpsampledWeight;
    public float3 fHistoryColor;
    public float fHistoryWeight;
    public float fLock;
    public float fLockContributionThisFrame;
};

public void RectificationBoxAddInitialSample(inout RectificationBox box, float3 colorSample, float fSampleWeight)
{
    box.aabbMin = colorSample;
    box.aabbMax = colorSample;
    float3 weightedSample = colorSample * fSampleWeight;
    box.boxCenter = weightedSample;
    box.boxVec = colorSample * weightedSample;
    box.fBoxCenterWeight = fSampleWeight;
}

public void RectificationBoxAddSample(bool bInitialSample, inout RectificationBox box, float3 colorSample, float fSampleWeight)
{
    if (bInitialSample)
    {
        RectificationBoxAddInitialSample(box, colorSample, fSampleWeight);
    }
    else
    {
        box.aabbMin = min(box.aabbMin, colorSample);
        box.aabbMax = max(box.aabbMax, colorSample);
        float3 weightedSample = colorSample * fSampleWeight;
        box.boxCenter += weightedSample;
        box.boxVec += colorSample * weightedSample;
        box.fBoxCenterWeight += fSampleWeight;
    }
}

public void RectificationBoxComputeVarianceBoxData(inout RectificationBox box)
{
    box.fBoxCenterWeight = abs(box.fBoxCenterWeight) > FSR3UPSCALER_FP32_MIN ? box.fBoxCenterWeight : 1.0;
    box.boxCenter /= box.fBoxCenterWeight;
    box.boxVec /= box.fBoxCenterWeight;
    float3 stdDev = sqrt(abs(box.boxVec - box.boxCenter * box.boxCenter));
    box.boxVec = stdDev;
}

public float MinDividedByMax(float v0, float v1, float fOnZeroReturnValue)
{
    float m = max(v0, v1);
    return m != 0.0 ? min(v0, v1) / m : fOnZeroReturnValue;
}

public float MinDividedByMax(float v0, float v1)
{
    float m = max(v0, v1);
    return m != 0.0 ? min(v0, v1) / m : 0.0;
}

public float3 YCoCgToRGB(float3 fYCoCg)
{
    return float3(fYCoCg.x + fYCoCg.y - fYCoCg.z, fYCoCg.x + fYCoCg.z, fYCoCg.x - fYCoCg.y - fYCoCg.z);
}

public float3 RGBToYCoCg(float3 fRgb)
{
    return float3(0.25 * fRgb.r + 0.5 * fRgb.g + 0.25 * fRgb.b, 0.5 * fRgb.r - 0.5 * fRgb.b,
                  -0.25 * fRgb.r + 0.5 * fRgb.g - 0.25 * fRgb.b);
}

public float RGBToLuma(float3 fLinearRgb)
{
    return dot(fLinearRgb, float3(0.2126, 0.7152, 0.0722));
}

public float3 Tonemap(float3 fRgb)
{
    float m = max(max(0.0, fRgb.r), max(fRgb.g, fRgb.b)) + 1.0;
    return fRgb / float3(m, m, m);
}

public float3 InverseTonemap(float3 fRgb)
{
    float m = max(FSR3UPSCALER_TONEMAP_EPSILON, 1.0 - max(fRgb.r, max(fRgb.g, fRgb.b)));
    return fRgb / float3(m, m, m);
}

public bool IsUvInside(float2 fUv)
{
    return (fUv.x >= 0.0 && fUv.x <= 1.0) && (fUv.y >= 0.0 && fUv.y <= 1.0);
}

public int2 ClampLoad(int2 iPxSample, int2 iPxOffset, int2 iTextureSize)
{
    int2 result = iPxSample + iPxOffset;
    result.x = max(0, min(result.x, iTextureSize.x - 1));
    result.y = max(0, min(result.y, iTextureSize.y - 1));
    return result;
}

public float2 ClampUv(float2 fUv, int2 iTextureSize, int2 iResourceSize)
{
    float2 fSampleLocation = fUv * float2(iTextureSize);
    float2 fClampedLocation = max(float2(0.5, 0.5), min(fSampleLocation, float2(iTextureSize) - float2(0.5, 0.5)));
    return fClampedLocation / float2(iResourceSize);
}

public bool IsOnScreen(int2 pos, int2 size)
{
    return all(uint2(pos) < uint2(size));
}

public int2 ComputeHrPosFromLrPos(int2 iPxLrPos)
{
    float2 fSrcJitteredPos = float2(iPxLrPos) + 0.5 - Jitter();
    float2 fLrPosInHr = (fSrcJitteredPos / float2(RenderSize())) * float2(UpscaleSize());
    return int2(floor(fLrPosInHr));
}

public float GetViewSpaceDepth(float fDeviceDepth)
{
    float4 f = DeviceToViewSpaceTransformFactors();
    return f[1] / (fDeviceDepth - f[0]);
}

public float GetViewSpaceDepthInMeters(float fDeviceDepth)
{
    return GetViewSpaceDepth(fDeviceDepth) * ViewSpaceToMetersFactor();
}

public struct BilinearSamplingData
{
    public int2 iOffsets[4];
    public float fWeights[4];
    public int2 iBasePos;
};

public BilinearSamplingData GetBilinearSamplingData(float2 fUv, int2 iSize)
{
    BilinearSamplingData data;
    float2 fPxSample = (fUv * float2(iSize)) - float2(0.5, 0.5);
    data.iBasePos = int2(floor(fPxSample));
    float2 fPxFrac = frac(fPxSample);
    data.iOffsets[0] = int2(0, 0);
    data.iOffsets[1] = int2(1, 0);
    data.iOffsets[2] = int2(0, 1);
    data.iOffsets[3] = int2(1, 1);
    data.fWeights[0] = (1.0 - fPxFrac.x) * (1.0 - fPxFrac.y);
    data.fWeights[1] = fPxFrac.x * (1.0 - fPxFrac.y);
    data.fWeights[2] = (1.0 - fPxFrac.x) * fPxFrac.y;
    data.fWeights[3] = fPxFrac.x * fPxFrac.y;
    return data;
}

// --- the SDK's sampling (`ffx_fsr3upscaler_sample.h`, its 32-bit paths) -------------------

public float Lanczos2NoClamp(float x)
{
    const float PI = 3.141592653589793;
    return abs(x) < FSR3UPSCALER_EPSILON ? 1.0 : (sin(PI * x) / (PI * x)) * (sin(0.5 * PI * x) / (0.5 * PI * x));
}

public float Lanczos2(float x)
{
    x = min(abs(x), 2.0);
    return Lanczos2NoClamp(x);
}

public float Lanczos2ApproxSqNoClamp(float x2)
{
    float a = (2.0 / 5.0) * x2 - 1.0;
    float b = (1.0 / 4.0) * x2 - 1.0;
    return ((25.0 / 16.0) * a * a - (25.0 / 16.0 - 1.0)) * (b * b);
}

public float Lanczos2ApproxSq(float x2)
{
    x2 = min(x2, 4.0);
    return Lanczos2ApproxSqNoClamp(x2);
}

public float4 Lanczos2(float4 fColor0, float4 fColor1, float4 fColor2, float4 fColor3, float t)
{
    float fWeight0 = Lanczos2(-1.0 - t);
    float fWeight1 = Lanczos2(-0.0 - t);
    float fWeight2 = Lanczos2(+1.0 - t);
    float fWeight3 = Lanczos2(+2.0 - t);
    return (fWeight0 * fColor0 + fWeight1 * fColor1 + fWeight2 * fColor2 + fWeight3 * fColor3)
           / (fWeight0 + fWeight1 + fWeight2 + fWeight3);
}

// Clamp by offset direction, as the SDK's `ClampCoord`.
public int2 ClampCoord(int2 iPxSample, int2 iPxOffset, int2 iTextureSize)
{
    int2 result = iPxSample + iPxOffset;
    result.x = max(1, min(result.x, iTextureSize.x - 2));
    result.y = max(1, min(result.y, iTextureSize.y - 2));
    return result;
}

// The history sampled with the reference Lanczos2 filter over its 4x4 texels round `fUvSample`,
// deringed by the four nearest (the SDK's `HistorySample` with its reprojection's default
// filter). `history` is the last frame's upscaled color, `iTextureSize` its size.
public float4 HistorySample(uint history, float2 fUvSample, int2 iTextureSize)
{
    float2 fPxSample = (fUvSample * float2(iTextureSize)) - float2(0.5, 0.5);
    float2 fPxFrac = frac(fPxSample);
    fPxSample.x = max(0.0, min(float(iTextureSize.x - 1), fPxSample.x));
    fPxSample.y = max(0.0, min(float(iTextureSize.y - 1), fPxSample.y));
    int2 iPxSample = int2(floor(fPxSample));
    float4 s[4][4];
    for (int y = 0; y < 4; y++)
    {
        for (int x = 0; x < 4; x++)
        {
            s[y][x] = load4(history, ClampCoord(iPxSample, int2(x - 1, y - 1), iTextureSize), iTextureSize);
        }
    }
    float4 fColorX0 = Lanczos2(s[0][0], s[0][1], s[0][2], s[0][3], fPxFrac.x);
    float4 fColorX1 = Lanczos2(s[1][0], s[1][1], s[1][2], s[1][3], fPxFrac.x);
    float4 fColorX2 = Lanczos2(s[2][0], s[2][1], s[2][2], s[2][3], fPxFrac.x);
    float4 fColorX3 = Lanczos2(s[3][0], s[3][1], s[3][2], s[3][3], fPxFrac.x);
    float4 fColorXY = Lanczos2(fColorX0, fColorX1, fColorX2, fColorX3, fPxFrac.y);
    float4 lo = min(min(s[1][1], s[1][2]), min(s[2][1], s[2][2]));
    float4 hi = max(max(s[1][1], s[1][2]), max(s[2][1], s[2][2]));
    return clamp(fColorXY, lo, hi);
}

// The FidelityFX core's medium-precision reciprocal approximation.
public float ffxApproximateReciprocalMedium(float a)
{
    float b = asfloat(uint(0x7ef19fff) - asuint(a));
    return b * (-b * a + 2.0);
}

public float ffxMax3(float x, float y, float z)
{
    return max(x, max(y, z));
}

public float ffxMin3(float x, float y, float z)
{
    return min(x, min(y, z));
}

// --- what the passes read and write ----------------------------------------------------------

// The size a picture at half the render size has.
public int2 HalfSize()
{
    return max(RenderSize() / 2, int2(1, 1));
}

// The size of level `level` of the shading change's pyramid.
public int2 LevelSize(uint level)
{
    return max(HalfSize() >> level, int2(1, 1));
}

uint Level(uint level)
{
    return level == 0 ? pictures.level0 : (level == 1 ? pictures.level1 : pictures.level2);
}

// The frame's color at a pixel: never a number that would poison what is gathered.
float3 LoadInputColor(int2 p)
{
    float3 c = textures[NonUniformResourceIndex(pictures.color)].Load(int3(p, 0)).rgb;
    c.r = isnan(c.r) ? 0.0 : c.r;
    c.g = isnan(c.g) ? 0.0 : c.g;
    c.b = isnan(c.b) ? 0.0 : c.b;
    return clamp(c, float3(0.0, 0.0, 0.0), float3(FSR3UPSCALER_FP16_MAX, FSR3UPSCALER_FP16_MAX, FSR3UPSCALER_FP16_MAX));
}

float LoadInputDepth(int2 p)
{
    return load1(pictures.input_depth, p, RenderSize());
}

float2 LoadInputMotionVector(int2 p)
{
    return load2(pictures.input_motion, p, RenderSize()) * MotionVectorScale();
}

float2 LoadDilatedMotionVector(int2 p)
{
    return load2(pictures.dilated_motion, p, RenderSize());
}

float LoadDilatedDepth(int2 p)
{
    return load1(pictures.dilated_depth, p, RenderSize());
}

float LoadCurrentLuma(int2 p)
{
    return load1(pictures.luma_now, p, RenderSize());
}

float LoadPreviousLuma(int2 p)
{
    return load1(pictures.luma_last, p, PreviousFrameRenderSize());
}

// The reconstructed depth keeps the nearest of what lands on a pixel, as the device's bits: a
// depth is never negative, so the nearer is the smaller either way.
void StoreReconstructedDepth(int2 p, float depth)
{
    uint held;
    rw_buffers[NonUniformResourceIndex(pictures.reconstructed)].InterlockedMin(
        element(p, RenderSize().x) * 4, asuint(depth), held);
}

// --- preparing the inputs (`ffx_fsr3upscaler_prepare_inputs.h`) ------------------------------

void ReconstructPrevDepth(int2 iPxPos, float fDepth, float2 fMotionVector)
{
    float fNearestDepthInMeters = min(GetViewSpaceDepthInMeters(fDepth), FSR3UPSCALER_FP16_MAX);
    float fReconstructedDeptMvThreshold = ReconstructedDepthMvPxThreshold(fNearestDepthInMeters);

    // Discard small motion.
    fMotionVector *= float(Get4KVelocity(fMotionVector) > fReconstructedDeptMvThreshold);

    float2 fUv = (float2(iPxPos) + 0.5) / float2(RenderSize());
    float2 fReprojectedUv = fUv + fMotionVector;
    BilinearSamplingData bilinearInfo = GetBilinearSamplingData(fReprojectedUv, RenderSize());

    // This depth pushed to every pixel with a part of the place in the last frame.
    for (int iSampleIndex = 0; iSampleIndex < 4; iSampleIndex++)
    {
        int2 iOffset = bilinearInfo.iOffsets[iSampleIndex];
        float fWeight = bilinearInfo.fWeights[iSampleIndex];
        if (fWeight > fReconstructedDepthBilinearWeightThreshold)
        {
            int2 iStorePos = bilinearInfo.iBasePos + iOffset;
            if (IsOnScreen(iStorePos, RenderSize()))
            {
                StoreReconstructedDepth(iStorePos, fDepth);
            }
        }
    }
}

struct DepthExtents
{
    float fNearest;
    int2 fNearestCoord;
    float fFarthest;
};

DepthExtents FindDepthExtents(int2 iPxPos)
{
    DepthExtents extents;
    const int iSampleCount = 9;
    const int2 iSampleOffsets[9] = {
        int2(+0, +0),
        int2(+1, +0),
        int2(+0, +1),
        int2(+0, -1),
        int2(-1, +0),
        int2(-1, +1),
        int2(+1, +1),
        int2(-1, -1),
        int2(+1, -1),
    };

    extents.fNearestCoord = iPxPos;
    extents.fNearest = LoadInputDepth(iPxPos);
    extents.fFarthest = extents.fNearest;
    for (int iSampleIndex = 1; iSampleIndex < iSampleCount; ++iSampleIndex)
    {
        int2 iPos = iPxPos + iSampleOffsets[iSampleIndex];
        if (IsOnScreen(iPos, RenderSize()))
        {
            float fNdDepth = LoadInputDepth(iPos);
            if (fNdDepth < extents.fNearest)
            {
                extents.fFarthest = max(extents.fFarthest, fNdDepth);
                extents.fNearestCoord = iPos;
                extents.fNearest = fNdDepth;
            }
        }
    }
    return extents;
}

public void PrepareInputs(int2 iPxPos)
{
    DepthExtents depthExtents = FindDepthExtents(iPxPos);
    float2 fDilatedMotionVector = LoadInputMotionVector(depthExtents.fNearestCoord);

    ReconstructPrevDepth(iPxPos, depthExtents.fNearest, fDilatedMotionVector);

    store2(pictures.dilated_motion, iPxPos, RenderSize(), fDilatedMotionVector);
    store1(pictures.dilated_depth, iPxPos, RenderSize(), depthExtents.fNearest);

    float fFarthestDepthInMeters = min(GetViewSpaceDepthInMeters(depthExtents.fFarthest), FSR3UPSCALER_FP16_MAX);
    store1(pictures.intermediate, iPxPos, RenderSize(), fFarthestDepthInMeters);

    float3 fRgb = LoadInputColor(iPxPos);
    store1(pictures.luma_now, iPxPos, RenderSize(), RGBToLuma(fRgb));
}

// --- the farthest depth at half size (the luma pyramid's first level) ------------------------

public void FarthestDepthHalf(int2 p)
{
    int2 q = p * 2;
    int2 size = RenderSize();
    float v = load1(pictures.intermediate, q, size) + load1(pictures.intermediate, q + int2(1, 0), size)
              + load1(pictures.intermediate, q + int2(0, 1), size) + load1(pictures.intermediate, q + int2(1, 1), size);
    store1(pictures.farthest_half, p, HalfSize(), v * 0.25);
}

// --- the shading change's pyramid (`ffx_fsr3upscaler_shading_change_pyramid.h`) --------------

struct SampleSet
{
    float fSamples[5];
};

void CompareSwap(inout SampleSet fSet, int i, int j)
{
    float fTmp = min(fSet.fSamples[i], fSet.fSamples[j]);
    fSet.fSamples[j] = max(fSet.fSamples[i], fSet.fSamples[j]);
    fSet.fSamples[i] = fTmp;
}

void SortSet(inout SampleSet fSet)
{
    CompareSwap(fSet, 0, 3);
    CompareSwap(fSet, 1, 4);
    CompareSwap(fSet, 0, 2);
    CompareSwap(fSet, 1, 3);
    CompareSwap(fSet, 0, 1);
    CompareSwap(fSet, 2, 4);
    CompareSwap(fSet, 1, 2);
    CompareSwap(fSet, 3, 4);
    CompareSwap(fSet, 2, 3);
}

float ComputeMinimumDifference(SampleSet fSet0, SampleSet fSet1)
{
    float fMinDiff = FSR3UPSCALER_FP16_MAX - 1.0;
    int a = 0;
    int b = 0;

    SortSet(fSet0);
    SortSet(fSet1);

    float fMax = min(fSet0.fSamples[SHADING_CHANGE_SET_SIZE - 1], fSet1.fSamples[SHADING_CHANGE_SET_SIZE - 1]);
    if (fMax > FSR3UPSCALER_FP32_MIN)
    {
        for (int i = 0; i < SHADING_CHANGE_SET_SIZE && (fMinDiff < FSR3UPSCALER_FP16_MAX); i++)
        {
            float fDiff = fSet0.fSamples[a] - fSet1.fSamples[b];
            if (abs(fDiff) > FSR3UPSCALER_FP16_MIN)
            {
                float s = fDiff < 0.0 ? -1.0 : 1.0;
                fDiff = s * (1.0 - MinDividedByMax(fSet0.fSamples[a], fSet1.fSamples[b]));
                fMinDiff = (abs(fDiff) < abs(fMinDiff)) ? fDiff : fMinDiff;
                // The SDK's two steps, in its order: the second reads the first's `a`.
                a += int(fSet0.fSamples[a] < fSet1.fSamples[b]);
                a = min(a, SHADING_CHANGE_SET_SIZE - 1);
                b += int(fSet0.fSamples[a] >= fSet1.fSamples[b]);
                b = min(b, SHADING_CHANGE_SET_SIZE - 1);
            }
            else
            {
                fMinDiff = FSR3UPSCALER_FP16_MAX;
            }
        }
    }
    return fMinDiff * float(fMinDiff < (FSR3UPSCALER_FP16_MAX - 1.0));
}

void LumaSamples(out SampleSet fSet, bool last, int2 iBasePos)
{
    const int2 iSampleOffsets[5] = { int2(+0, +0), int2(-1, +0), int2(+1, +0), int2(+0, -1), int2(+0, +1) };
    for (int iSampleIndex = 0; iSampleIndex < SHADING_CHANGE_SET_SIZE; iSampleIndex++)
    {
        float v;
        if (last)
        {
            int2 iSamplePos = ClampLoad(iBasePos, iSampleOffsets[iSampleIndex], PreviousFrameRenderSize());
            v = LoadPreviousLuma(iSamplePos) * DeltaPreExposure() * Exposure();
        }
        else
        {
            int2 iSamplePos = ClampLoad(iBasePos, iSampleOffsets[iSampleIndex], RenderSize());
            v = LoadCurrentLuma(iSamplePos) * Exposure();
        }
        // The SDK raises it to fShadingChangeSamplePow here, which is 1.
        fSet.fSamples[iSampleIndex] = max(v, FSR3UPSCALER_EPSILON);
    }
}

float ComputeDiff(float2 fUv, float2 fMotionVector)
{
    float2 fUvJittered = fUv + Jitter() / float2(RenderSize());
    SampleSet fCurrentSamples;
    LumaSamples(fCurrentSamples, false, int2(floor(fUvJittered * float2(RenderSize()))));

    float2 fLastJittered = fUv + PreviousFrameJitter() / float2(PreviousFrameRenderSize());
    float2 fReprojectedUv = fLastJittered + fMotionVector;
    if (!IsUvInside(fReprojectedUv))
    {
        return 0.0;
    }
    SampleSet fLastSamples;
    LumaSamples(fLastSamples, true, int2(floor(fReprojectedUv * float2(PreviousFrameRenderSize()))));
    return ComputeMinimumDifference(fCurrentSamples, fLastSamples);
}

float2 ShadingChangeSource(int2 p)
{
    int2 q = ClampLoad(p, int2(0, 0), RenderSize());
    float2 fDilatedMotionVector = LoadDilatedMotionVector(q);
    float2 fUv = (float2(q) + 0.5) / float2(RenderSize());
    float fScaledAndSignedLumaDiff = ComputeDiff(fUv, fDilatedMotionVector);
    float s = fScaledAndSignedLumaDiff > 0.0 ? 1.0 : (fScaledAndSignedLumaDiff < 0.0 ? -1.0 : 0.0);
    return float2(fScaledAndSignedLumaDiff, s);
}

// The pyramid's first level: at half the render size, the mean of four of the signed differences.
public void ShadingChangeLevelZero(int2 p)
{
    int2 q = p * 2;
    float2 v = ShadingChangeSource(q) + ShadingChangeSource(q + int2(1, 0)) + ShadingChangeSource(q + int2(0, 1))
               + ShadingChangeSource(q + int2(1, 1));
    store2(pictures.level0, p, LevelSize(0), v * 0.25);
}

// A later level: the mean of four of the level before.
public void ShadingChangeLevel(int2 p, uint level)
{
    int2 q = p * 2;
    int2 from_size = LevelSize(level - 1);
    uint from = Level(level - 1);
    float2 v = load2(from, q, from_size) + load2(from, q + int2(1, 0), from_size) + load2(from, q + int2(0, 1), from_size)
               + load2(from, q + int2(1, 1), from_size);
    store2(Level(level), p, LevelSize(level), v * 0.25);
}

// --- the shading change (`ffx_fsr3upscaler_shading_change.h`) --------------------------------

public void ShadingChange(int2 iPxPos)
{
    int2 size = HalfSize();
    float2 fUv = (float2(iPxPos) + 0.5) / float2(size);
    float2 fUvJittered = fUv + Jitter() / float2(RenderSize());
    float2 fMipUv = ClampUv(fUvJittered, size, LevelSize(0));

    float fShadingChange = 0.0;
    for (uint iMipLevel = 0; iMipLevel < 3; iMipLevel++)
    {
        float2 fSample = sample2(Level(iMipLevel), fMipUv, LevelSize(iMipLevel));
        float v = abs(fSample.x * fSample.y);
        if (v > 0.0)
        {
            fShadingChange = max(fShadingChange, v);
        }
    }
    store1(pictures.shading_change, iPxPos, size, saturate(fShadingChange));
}

// --- the reactivity (`ffx_fsr3upscaler_prepare_reactivity.h`) --------------------------------

float ComputeDisocclusions(float2 fUv, float2 fMotionVector, float fCurrentDepthViewSpace)
{
    float fNearestDepthInMeters = min(fCurrentDepthViewSpace * ViewSpaceToMetersFactor(), FSR3UPSCALER_FP16_MAX);
    float fReconstructedDeptMvThreshold = ReconstructedDepthMvPxThreshold(fNearestDepthInMeters);

    fMotionVector *= float(Get4KVelocity(fMotionVector) > fReconstructedDeptMvThreshold);

    float2 fReprojectedUv = fUv + fMotionVector;
    BilinearSamplingData bilinearInfo = GetBilinearSamplingData(fReprojectedUv, RenderSize());

    float fDisocclusion = 0.0;
    float fWeightSum = 0.0;
    bool bPotentialDisocclusion = true;

    for (int iSampleIndex = 0; iSampleIndex < 4 && bPotentialDisocclusion; iSampleIndex++)
    {
        int2 iOffset = bilinearInfo.iOffsets[iSampleIndex];
        int2 iSamplePos = ClampLoad(bilinearInfo.iBasePos, iOffset, RenderSize());
        if (IsOnScreen(iSamplePos, RenderSize()))
        {
            float fWeight = bilinearInfo.fWeights[iSampleIndex];
            if (fWeight > fReconstructedDepthBilinearWeightThreshold)
            {
                float fPrevNearestDepthViewSpace = GetViewSpaceDepth(load1(pictures.reconstructed, iSamplePos, RenderSize()));
                float fDepthDifference = fCurrentDepthViewSpace - fPrevNearestDepthViewSpace;

                bPotentialDisocclusion = bPotentialDisocclusion && (fDepthDifference > FSR3UPSCALER_FP32_MIN);

                if (bPotentialDisocclusion)
                {
                    float fHalfViewportWidth = length(float2(RenderSize()) * 0.5);
                    float fDepthThreshold = max(fCurrentDepthViewSpace, fPrevNearestDepthViewSpace);

                    const float Ksep = 1.37e-05;
                    float fRequiredDepthSeparation = Ksep * fHalfViewportWidth * fDepthThreshold;

                    fDisocclusion += saturate(fRequiredDepthSeparation / fDepthDifference) * fWeight;
                    fWeightSum += fWeight;
                }
            }
        }
    }

    return (bPotentialDisocclusion && fWeightSum > 0.0) ? saturate(1.0 - fDisocclusion / fWeightSum) : 0.0;
}

float ComputeMotionDivergence(float2 fUv, float2 fMotionVector, float fCurrentDepthSample)
{
    float f4KVelocity = Get4KVelocity(fMotionVector);
    if (f4KVelocity <= 0.0)
    {
        // The SDK's sum is 0 by its velocity factor here, after a division by zero.
        return 0.0;
    }
    int2 iPxReprojectedPos = int2((fUv + fMotionVector) * float2(RenderSize()));
    float fReprojectedDepth = LoadDilatedDepth(iPxReprojectedPos);
    float2 fReprojectedMotionVector = LoadDilatedMotionVector(iPxReprojectedPos);

    float fReprojectedVelocity = Get4KVelocity(fReprojectedMotionVector);

    float fNucleusDepthInMeters = GetViewSpaceDepthInMeters(fReprojectedDepth);
    float fCurrentDepthInMeters = GetViewSpaceDepthInMeters(fCurrentDepthSample);

    float fDistanceFactor = MinDividedByMax(fNucleusDepthInMeters, fCurrentDepthInMeters);
    float fVelocityFactor = saturate(f4KVelocity / 10.0);
    return (1.0 - saturate(fReprojectedVelocity / f4KVelocity)) * fDistanceFactor * fVelocityFactor;
}

float ComputeThinFeatureConfidence(int2 iPxPos)
{
    /*
     1 2 3
     4 0 5
     6 7 8
    */
    const int iNucleusIndex = 0;
    const int iSampleCount = 9;
    const int2 iSampleOffsets[9] = {
        int2(+0, +0),
        int2(-1, -1),
        int2(+0, -1),
        int2(+1, -1),
        int2(-1, +0),
        int2(+1, +0),
        int2(-1, +1),
        int2(+0, +1),
        int2(+1, +1),
    };

    float fSamples[9];
    float fLumaMin = FSR3UPSCALER_FP32_MAX;
    float fLumaMax = FSR3UPSCALER_FP32_MIN;
    for (int iSampleIndex = 0; iSampleIndex < iSampleCount; ++iSampleIndex)
    {
        int2 iPxSamplePos = ClampLoad(iPxPos, iSampleOffsets[iSampleIndex], RenderSize());
        fSamples[iSampleIndex] = LoadCurrentLuma(iPxSamplePos) * Exposure();
        fLumaMin = min(fLumaMin, fSamples[iSampleIndex]);
        fLumaMax = max(fLumaMax, fSamples[iSampleIndex]);
    }

    const float fThreshold = 0.9;
    float fDissimilarLumaMin = FSR3UPSCALER_FP32_MAX;
    float fDissimilarLumaMax = 0.0;

    uint uPatternMask = 1u; // the nucleus is similar to itself
    const uint uRejectionMasks[4] = {
        (1u << 1) | (1u << 2) | (1u << 4) | 1u, // upper left
        (1u << 2) | (1u << 3) | (1u << 5) | 1u, // upper right
        (1u << 4) | (1u << 6) | (1u << 7) | 1u, // lower left
        (1u << 5) | (1u << 7) | (1u << 8) | 1u, // lower right
    };

    float fRange = fLumaMax - fLumaMin;
    for (int iSampleIndex = 1; iSampleIndex < iSampleCount; ++iSampleIndex)
    {
        // With no range at all the SDK's sum is 0 over 0, which is not under the threshold.
        bool similar = fRange > 0.0 && abs(fSamples[iSampleIndex] - fSamples[iNucleusIndex]) / fRange < fThreshold;
        if (similar)
        {
            uPatternMask |= 1u << uint(iSampleIndex);
        }
        else
        {
            fDissimilarLumaMin = min(fDissimilarLumaMin, fSamples[iSampleIndex]);
            fDissimilarLumaMax = max(fDissimilarLumaMax, fSamples[iSampleIndex]);
        }
    }

    bool bIsRidge = fSamples[iNucleusIndex] > fDissimilarLumaMax || fSamples[iNucleusIndex] < fDissimilarLumaMin;
    if (!bIsRidge)
    {
        return 0.0;
    }
    for (int i = 0; i < 4; i++)
    {
        if ((uPatternMask & uRejectionMasks[i]) == uRejectionMasks[i])
        {
            return 0.0;
        }
    }
    return 1.0 - fLumaMin / fLumaMax;
}

float UpdateAccumulation(int2 iPxPos, float2 fUv, float2 fMotionVector, float fDisocclusion, float fShadingChange)
{
    float2 fReprojectedUv = fUv + fMotionVector;
    float fAccumulation = 0.0;
    if (IsUvInside(fReprojectedUv))
    {
        float2 fReprojectedUv_HW = ClampUv(fReprojectedUv, PreviousFrameRenderSize(), MaxRenderSize());
        fAccumulation = saturate(sample1(pictures.accumulation_read, fReprojectedUv_HW, PreviousFrameRenderSize()));
    }
    float fAccumulationAddedPerFrame = AccumulationAddedPerFrame();

    // A frame whose shading changed starts over: with no further change its accumulation is 0
    // on it, and a third more on each frame after.
    fAccumulation = lerp(fAccumulation, 0.0, fShadingChange);

    // A frame that uncovered this pixel starts a third below that.
    float fMinDisocclusionAccumulation = MinDisocclusionAccumulation();
    fAccumulation = lerp(fAccumulation, min(fMinDisocclusionAccumulation, fAccumulation), fDisocclusion);

    fAccumulation *= float(round(fAccumulation * 100.0) > 1.0);

    float fAccumulatedFramesToStore = saturate(fAccumulation + fAccumulationAddedPerFrame);
    store1(pictures.accumulation_write, iPxPos, RenderSize(), fAccumulatedFramesToStore);

    return fAccumulation;
}

float ComputeShadingChange(float2 fUv)
{
    // The jitter put back here is taken away again when the accumulation samples it.
    float2 fShadingChangeUv = ClampUv(fUv - Jitter() / float2(RenderSize()), HalfSize(), HalfSize());
    return saturate(sample1(pictures.shading_change, fShadingChangeUv, HalfSize()));
}

public void PrepareReactivity(int2 iPxPos)
{
    float2 fUv = (float2(iPxPos) + 0.5) / float2(RenderSize());
    float2 fMotionVector = LoadDilatedMotionVector(iPxPos);

    float fDilatedDepth = LoadDilatedDepth(iPxPos);

    float fDisocclusion = ComputeDisocclusions(fUv, fMotionVector, GetViewSpaceDepth(fDilatedDepth));
    // The composition mask's dilation below is of the SDK's default mask, zero.
    float fShadingChange = ComputeShadingChange(fUv);

    // The reactive mask, dilated over the pixels about this one.
    float fInputReactive = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            fInputReactive = max(fInputReactive, load1(pictures.input_reactive, iPxPos + int2(x, y), RenderSize()));
        }
    }
    float fFinalReactiveness = max(ComputeMotionDivergence(fUv, fMotionVector, fDilatedDepth), fInputReactive);

    float fAccumulation = UpdateAccumulation(iPxPos, fUv, fMotionVector, fDisocclusion, fShadingChange);

    // The SDK keeps these in unsigned normalized bytes, so a negative accumulation is kept as 0.
    float4 fOutput = saturate(float4(fFinalReactiveness, fDisocclusion, fShadingChange, fAccumulation));
    store4(pictures.reactive, iPxPos, RenderSize(), fOutput);

    float fLockStrength = ComputeThinFeatureConfidence(iPxPos);
    if (fLockStrength > (1.0 / 100.0))
    {
        store1(pictures.new_locks, ComputeHrPosFromLrPos(iPxPos), UpscaleSize(), fLockStrength);
    }
}

// --- the luma instability (`ffx_fsr3upscaler_luma_instability.h`) ----------------------------

public void LumaInstability(int2 iPxPos)
{
    float4 fLumaHistory = float4(0.0, 0.0, 0.0, 0.0);
    float fLumaInstabilityFactor = 0.0;

    float2 fDilatedMotionVector = LoadDilatedMotionVector(iPxPos);
    float2 fUv = (float2(iPxPos) + 0.5) / float2(RenderSize());
    float2 fUvCurrFrameJittered = fUv + Jitter() / float2(RenderSize());
    float2 fUvPrevFrameJittered = fUv + PreviousFrameJitter() / float2(PreviousFrameRenderSize());
    float2 fReprojectedUv = fUvPrevFrameJittered + fDilatedMotionVector;

    if (IsUvInside(fReprojectedUv))
    {
        float2 fUvReactive_HW = ClampUv(fUvCurrFrameJittered, RenderSize(), MaxRenderSize());
        float4 fDilatedReactiveMasks = sample4(pictures.reactive, fUvReactive_HW, RenderSize());
        float fReactiveMask = saturate(fDilatedReactiveMasks[REACTIVE]);
        float fDisocclusion = saturate(fDilatedReactiveMasks[DISOCCLUSION]);
        float fShadingChange = saturate(fDilatedReactiveMasks[SHADING_CHANGE]);
        float fAccumulation = saturate(fDilatedReactiveMasks[ACCUMULAION]);

        if (fAccumulation > 0.9)
        {
            float2 fUv_HW = ClampUv(fUvCurrFrameJittered, RenderSize(), MaxRenderSize());
            float fCurrentFrameLuma = sample1(pictures.luma_now, fUv_HW, RenderSize()) * Exposure();

            float2 fReprojectedUv_HW = ClampUv(fReprojectedUv, PreviousFrameRenderSize(), MaxRenderSize());
            fLumaHistory = sample4(pictures.history_read, fReprojectedUv_HW, PreviousFrameRenderSize())
                           * DeltaPreExposure() * Exposure();

            float fLumaInstability = 0.0;
            float fDiffs0 = fCurrentFrameLuma - fLumaHistory[0];
            float fSimilarity0 = MinDividedByMax(fCurrentFrameLuma, fLumaHistory[0], 1.0);
            float fMaxSimilarity = fSimilarity0;
            if (fSimilarity0 < 1.0)
            {
                for (int i = 1; i <= 3; i++)
                {
                    float fDiffs1 = fCurrentFrameLuma - fLumaHistory[i];
                    float fSimilarity1 = MinDividedByMax(fCurrentFrameLuma, fLumaHistory[i]);
                    if (sign(fDiffs0) == sign(fDiffs1))
                    {
                        fMaxSimilarity = max(fMaxSimilarity, fSimilarity1);
                    }
                }
                fLumaInstability = float(fMaxSimilarity > fSimilarity0);
            }

            // The history shifted along.
            fLumaHistory = float4(fCurrentFrameLuma, fLumaHistory[0], fLumaHistory[1], fLumaHistory[2]);
            fLumaHistory /= Exposure();

            fLumaInstabilityFactor = fLumaInstability * float(fLumaHistory[3] != 0.0);

            float fVelocityWeight = 1.0 - saturate(Get4KVelocity(fDilatedMotionVector) / 20.0);
            fLumaInstabilityFactor *= fVelocityWeight * (1.0 - fDisocclusion) * (1.0 - fReactiveMask) * (1.0 - fShadingChange);
        }
    }

    store4(pictures.history_write, iPxPos, RenderSize(), fLumaHistory);
    store1(pictures.intermediate, iPxPos, RenderSize(), fLumaInstabilityFactor);
}

// --- the accumulation (`ffx_fsr3upscaler_accumulate.h`, `_upsample.h`, `_reproject.h`) -------

float GetUpsampleLanczosWeight(float2 fSrcSampleOffset, float fKernelWeight)
{
    float2 fSrcSampleOffsetBiased = fSrcSampleOffset * fKernelWeight;
    return Lanczos2ApproxSq(dot(fSrcSampleOffsetBiased, fSrcSampleOffsetBiased));
}

float3 LoadPreparedColor(int2 iSamplePos)
{
    return RGBToYCoCg(LoadInputColor(iSamplePos) * Exposure());
}

void ComputeUpsampledColorAndWeight(AccumulationPassCommonParams params, inout AccumulationPassData data)
{
    // A sliced Lanczos filter of two lobes; the other slices are gathered over the frames.
    float2 fDstOutputPos = float2(params.iPxHrPos) + 0.5;
    float2 fSrcOutputPos = fDstOutputPos * DownscaleFactor();
    int2 iSrcInputPos = int2(floor(fSrcOutputPos));
    float2 fSrcUnjitteredPos = (float2(iSrcInputPos) + 0.5) - Jitter();
    float2 fBaseSampleOffset = fSrcUnjitteredPos - fSrcOutputPos;

    int2 offsetTL;
    offsetTL.x = (fSrcUnjitteredPos.x > fSrcOutputPos.x) ? -2 : -1;
    offsetTL.y = (fSrcUnjitteredPos.y > fSrcOutputPos.y) ? -2 : -1;

    // The rows and columns flipped where the kernel leans the other way, so the first three of
    // each make the clipping box.
    bool bFlipRow = fSrcUnjitteredPos.y > fSrcOutputPos.y;
    bool bFlipCol = fSrcUnjitteredPos.x > fSrcOutputPos.x;
    float2 fOffsetTL = float2(offsetTL);

    bool bIsInitialSample = params.fAccumulation == 0.0;

    float3 fSamples[9];
    int iSampleIndex = 0;
    for (int r = 0; r < 3; r++)
    {
        for (int col = 0; col < 3; col++)
        {
            int2 iSampleColRow = int2(bFlipCol ? (3 - col) : col, bFlipRow ? (3 - r) : r);
            int2 iSrcSamplePos = iSrcInputPos + offsetTL + iSampleColRow;
            int2 iSampleCoord = ClampLoad(iSrcSamplePos, int2(0, 0), RenderSize());
            fSamples[iSampleIndex] = LoadPreparedColor(iSampleCoord);
            ++iSampleIndex;
        }
    }

    if (HdrInput() && bIsInitialSample)
    {
        for (iSampleIndex = 0; iSampleIndex < 9; ++iSampleIndex)
        {
            fSamples[iSampleIndex] = RGBToYCoCg(Tonemap(YCoCgToRGB(fSamples[iSampleIndex])));
        }
    }

    // How much of each sample this frame takes.
    float fKernelBiasMax = min(1.99, 1.0 + (1.0 / DownscaleFactor().x - 1.0));
    float fKernelBiasMin = max(1.0, (1.0 + fKernelBiasMax) * 0.3);
    float fKernelBiasWeight = min(1.0 - params.fDisocclusion * 0.5, min(1.0 - params.fShadingChange, saturate(data.fHistoryWeight * 5.0)));
    float fKernelBias = lerp(fKernelBiasMin, fKernelBiasMax, fKernelBiasWeight);

    iSampleIndex = 0;
    for (int r = 0; r < 3; r++)
    {
        for (int col = 0; col < 3; col++)
        {
            int2 sampleColRow = int2(bFlipCol ? (3 - col) : col, bFlipRow ? (3 - r) : r);
            float2 fOffset = fOffsetTL + float2(sampleColRow);
            float2 fSrcSampleOffset = fBaseSampleOffset + fOffset;

            int2 iSrcSamplePos = iSrcInputPos + offsetTL + sampleColRow;
            float fOnScreenFactor = float(IsOnScreen(iSrcSamplePos, RenderSize()));

            if (!bIsInitialSample)
            {
                float fSampleWeight = fOnScreenFactor * GetUpsampleLanczosWeight(fSrcSampleOffset, fKernelBias);
                data.fUpsampledColor += fSamples[iSampleIndex] * fSampleWeight;
                data.fUpsampledWeight += fSampleWeight;
            }

            // The rectification box.
            const float fRectificationCurveBias = -2.3;
            float fSrcSampleOffsetSq = dot(fSrcSampleOffset, fSrcSampleOffset);
            float fBoxSampleWeight = exp(fRectificationCurveBias * fSrcSampleOffsetSq) * fOnScreenFactor;
            bool bInitialSample = (r == 0) && (col == 0);
            RectificationBoxAddSample(bInitialSample, data.clippingBox, fSamples[iSampleIndex], fBoxSampleWeight);
            ++iSampleIndex;
        }
    }

    RectificationBoxComputeVarianceBoxData(data.clippingBox);

    data.fUpsampledWeight *= float(data.fUpsampledWeight > FSR3UPSCALER_EPSILON);

    if (data.fUpsampledWeight > FSR3UPSCALER_EPSILON)
    {
        // Normalized for deringing, which compares colors.
        data.fUpsampledColor = data.fUpsampledColor / data.fUpsampledWeight;
        data.fUpsampledWeight *= fAverageLanczosWeightPerFrame;
        data.fUpsampledColor = clamp(data.fUpsampledColor, data.clippingBox.aabbMin, data.clippingBox.aabbMax);
    }

    // A first sample uses the tone-mapped mean.
    if (bIsInitialSample)
    {
        data.fUpsampledColor = HdrInput() ? RGBToYCoCg(InverseTonemap(YCoCgToRGB(data.clippingBox.boxCenter)))
                                          : data.clippingBox.boxCenter;
        data.fUpsampledWeight = 1.0;
        data.fHistoryWeight = 0.0;
    }
}

void AccumulateHistory(inout AccumulationPassData data)
{
    // Invalid values are avoided when the accumulation and the upsampled weight are 0.
    data.fHistoryWeight *= float(data.fHistoryWeight > FSR3UPSCALER_FP16_MIN);
    data.fHistoryWeight = max(FSR3UPSCALER_EPSILON, data.fHistoryWeight + data.fUpsampledWeight);

    if (HdrInput())
    {
        // Through RGB, so the tone map does not desaturate.
        data.fUpsampledColor = RGBToYCoCg(Tonemap(YCoCgToRGB(data.fUpsampledColor)));
        data.fHistoryColor = RGBToYCoCg(Tonemap(YCoCgToRGB(data.fHistoryColor)));
    }

    float fAlpha = saturate(data.fUpsampledWeight / data.fHistoryWeight);
    data.fHistoryColor = lerp(data.fHistoryColor, data.fUpsampledColor, fAlpha);
    data.fHistoryColor = YCoCgToRGB(data.fHistoryColor);

    if (HdrInput())
    {
        data.fHistoryColor = InverseTonemap(data.fHistoryColor);
    }
}

void RectifyHistory(AccumulationPassCommonParams params, inout AccumulationPassData data)
{
    float f4kVelocityFactor = saturate(params.f4KVelocity / 20.0);
    float fDistanceFactor = saturate(0.75 - params.fFarthestDepthInMeters / 20.0);
    float fAccumulationFactor = 1.0 - params.fAccumulation;
    float fReactiveFactor = sqrt(max(params.fReactiveMask, 0.0));
    float fShadingChangeFactor = params.fShadingChange;
    float fBoxScaleT = max(f4kVelocityFactor, max(fDistanceFactor, max(fAccumulationFactor, max(fReactiveFactor, fShadingChangeFactor))));

    float fBoxScale = lerp(3.0, 1.0, fBoxScaleT);
    float3 fScaledBoxVec = data.clippingBox.boxVec * float3(1.7, 1.0, 1.0) * fBoxScale;

    float3 fClampedScaledBoxVec = max(fScaledBoxVec, float3(1.193e-7, 1.193e-7, 1.193e-7));
    float3 fTransformedHistoryColor = (data.fHistoryColor - data.clippingBox.boxCenter) / fClampedScaledBoxVec;

    if (length(fTransformedHistoryColor) > 1.0)
    {
        float3 fClampedHistoryColor = normalize(fTransformedHistoryColor);
        float3 fFinalClampedHistoryColor = (fClampedHistoryColor * fScaledBoxVec) + data.clippingBox.boxCenter;

        // How much of the history stays is what locks and instability hold, by the accumulation,
        // so a color that may not be valid is not protected.
        float fHistoryContribution = max(params.fLumaInstabilityFactor, data.fLockContributionThisFrame) * params.fAccumulation * (1.0 - params.fDisocclusion);
        data.fHistoryColor = lerp(fFinalClampedHistoryColor, data.fHistoryColor, saturate(fHistoryContribution));
    }
}

void UpdateLockStatus(AccumulationPassCommonParams params, inout AccumulationPassData data)
{
    data.fLock *= float(!params.bIsNewSample);

    float fLifetimeDecreaseFactor = max(saturate(params.fShadingChange), max(params.fReactiveMask, params.fDisocclusion));
    data.fLock = max(0.0, data.fLock - fLifetimeDecreaseFactor * fLockMax);

    // This frame's part of the lock.
    data.fLockContributionThisFrame = saturate(saturate(data.fLock - fLockThreshold) * (fLockMax - fLockThreshold));

    float fNewLockIntensity = load1(pictures.new_locks, params.iPxHrPos, UpscaleSize()) * (1.0 - params.fReactiveMask);
    data.fLock = max(0.0, min(data.fLock + fNewLockIntensity, fLockMax));

    // For the next frame.
    float fLifetimeDecrease = (0.1 / JitterSequenceLength()) * (1.0 - fLifetimeDecreaseFactor);
    data.fLock = max(0.0, data.fLock - fLifetimeDecrease);

    // The next frame expects similar motion: a lock going off the screen ends, rather than being
    // held at its border.
    float2 fEstimatedUvNextFrame = params.fHrUv - params.fMotionVector;
    data.fLock *= float(IsUvInside(fEstimatedUvNextFrame));
}

void ComputeBaseAccumulationWeight(AccumulationPassCommonParams params, inout AccumulationPassData data)
{
    float fBaseAccumulation = params.fAccumulation;
    fBaseAccumulation = min(fBaseAccumulation, lerp(fBaseAccumulation, 0.15, saturate(max(0.0, (params.f4KVelocity * VelocityFactor()) / 0.5))));
    data.fHistoryWeight = fBaseAccumulation;
}

void ReprojectHistoryColor(AccumulationPassCommonParams params, inout AccumulationPassData data)
{
    float4 fReprojectedHistory = HistorySample(pictures.upscaled_read, params.fReprojectedHrUv, PreviousFrameUpscaleSize());
    data.fHistoryColor = fReprojectedHistory.rgb * DeltaPreExposure() * Exposure();
    data.fHistoryColor = RGBToYCoCg(data.fHistoryColor);
    data.fLock = fReprojectedHistory.w;
}

public void Accumulate(int2 iPxHrPos)
{
    AccumulationPassCommonParams params;
    AccumulationPassData data;

    params.iPxHrPos = iPxHrPos;
    float2 fHrUv = (float2(iPxHrPos) + 0.5) / float2(UpscaleSize());
    params.fHrUv = fHrUv;
    params.fLrUvJittered = fHrUv + Jitter() / float2(RenderSize());
    params.fLrUv_HwSampler = ClampUv(params.fLrUvJittered, RenderSize(), MaxRenderSize());

    params.fMotionVector = LoadDilatedMotionVector(int2(fHrUv * float2(RenderSize())));
    params.f4KVelocity = Get4KVelocity(params.fMotionVector);

    params.fReprojectedHrUv = params.fHrUv + params.fMotionVector;
    params.bIsExistingSample = IsUvInside(params.fReprojectedHrUv);

    float2 fLumaInstabilityUv_HW = ClampUv(fHrUv, RenderSize(), MaxRenderSize());
    params.fLumaInstabilityFactor = sample1(pictures.intermediate, fLumaInstabilityUv_HW, RenderSize());

    float2 fFarthestDepthUv = ClampUv(params.fLrUvJittered, HalfSize(), HalfSize());
    params.fFarthestDepthInMeters = sample1(pictures.farthest_half, fFarthestDepthUv, HalfSize());
    params.bIsNewSample = !params.bIsExistingSample || FrameIndex() == 0.0;

    float4 fDilatedReactiveMasks = sample4(pictures.reactive, params.fLrUv_HwSampler, RenderSize());
    params.fReactiveMask = saturate(fDilatedReactiveMasks[REACTIVE]);
    params.fDisocclusion = saturate(fDilatedReactiveMasks[DISOCCLUSION]);
    params.fShadingChange = saturate(fDilatedReactiveMasks[SHADING_CHANGE]);
    params.fAccumulation = saturate(fDilatedReactiveMasks[ACCUMULAION]);
    params.fAccumulation *= float(round(params.fAccumulation * 100.0) > 1.0);

    data.fUpsampledColor = float3(0.0, 0.0, 0.0);
    data.fHistoryColor = float3(0.0, 0.0, 0.0);
    data.fHistoryWeight = 1.0;
    data.fUpsampledWeight = 0.0;
    data.fLock = 0.0;
    data.fLockContributionThisFrame = 0.0;

    if (params.bIsExistingSample && !params.bIsNewSample)
    {
        ReprojectHistoryColor(params, data);
    }

    UpdateLockStatus(params, data);

    ComputeBaseAccumulationWeight(params, data);

    ComputeUpsampledColorAndWeight(params, data);

    RectifyHistory(params, data);

    AccumulateHistory(data);

    data.fHistoryColor /= Exposure();

    data.fHistoryColor = max(data.fHistoryColor, float3(0.0, 0.0, 0.0));

    // A number that is not one must not stay in the history.
    if (any(isnan(data.fHistoryColor)))
    {
        data.fHistoryColor = float3(0.0, 0.0, 0.0);
    }

    store4(pictures.upscaled_write, iPxHrPos, UpscaleSize(), float4(data.fHistoryColor, data.fLock));
    store1(pictures.new_locks, iPxHrPos, UpscaleSize(), 0.0);
}

// --- the sharpening (`ffx_fsr3upscaler_rcas.h`, RCAS in `ffx_fsr1.h`, with its denoise) ------

static const float FSR_RCAS_LIMIT = 0.25 - (1.0 / 16.0);

float3 RcasLoad(int2 p)
{
    return load4(pictures.upscaled_write, p, UpscaleSize()).rgb * Exposure();
}

public void Rcas(int2 sp)
{
    //    b
    //  d e f
    //    h
    float3 b = RcasLoad(sp + int2(0, -1));
    float3 d = RcasLoad(sp + int2(-1, 0));
    float3 e = RcasLoad(sp);
    float3 f = RcasLoad(sp + int2(1, 0));
    float3 h = RcasLoad(sp + int2(0, 1));
    // Luma times 2.
    float bL = b.b * 0.5 + (b.r * 0.5 + b.g);
    float dL = d.b * 0.5 + (d.r * 0.5 + d.g);
    float eL = e.b * 0.5 + (e.r * 0.5 + e.g);
    float fL = f.b * 0.5 + (f.r * 0.5 + f.g);
    float hL = h.b * 0.5 + (h.r * 0.5 + h.g);
    // Noise detection.
    float nz = 0.25 * bL + 0.25 * dL + 0.25 * fL + 0.25 * hL - eL;
    nz = saturate(abs(nz) * ffxApproximateReciprocalMedium(ffxMax3(ffxMax3(bL, dL, eL), fL, hL) - ffxMin3(ffxMin3(bL, dL, eL), fL, hL)));
    nz = -0.5 * nz + 1.0;
    // The ring's least and most.
    float3 mn4 = min(min(min(b, d), f), h);
    float3 mx4 = max(max(max(b, d), f), h);
    // Limiters: these need high precision reciprocals.
    float2 peakC = float2(1.0, -4.0);
    float3 hitMin = mn4 / (4.0 * mx4);
    float3 hitMax = (peakC.x - mx4) / (4.0 * mn4 + peakC.y);
    float3 lobeRGB = max(-hitMin, hitMax);
    float lobe = max(-FSR_RCAS_LIMIT, min(ffxMax3(lobeRGB.r, lobeRGB.g, lobeRGB.b), 0.0)) * RcasSharpness();
    lobe *= nz;
    // The resolve needs the medium precision reciprocal, to avoid visible changes of tone.
    float rcpL = ffxApproximateReciprocalMedium(4.0 * lobe + 1.0);
    float3 c = (lobe * b + lobe * d + lobe * h + lobe * f + e) * rcpL;
    c /= Exposure();
    store4(pictures.output, sp, UpscaleSize(), float4(c, 1.0));
}

// What the display's texture is filled from, at a pixel of the upscale size: the sharpened output,
// or the upscaled color where nothing sharpens it.
public float3 Shown(int2 p)
{
    return load4(pictures.shown, p, UpscaleSize()).rgb;
}
