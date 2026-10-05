//! The landscape's two device paths on Direct3D 12: composing its per-cell surfaces with a compute
//! shader, and splatting its terrain layers in the pixel shader. The Vulkan backend has the same
//! pair (`vulkan/terrain_merge.rs`, `vulkan/terrain_splat.rs`); what each one is and why is
//! explained there, and only what differs here is explained here.
//!
//! **Composing.** The compute root signature is three root buffer descriptors -- the source pool,
//! the job, the output -- so the compositor touches no descriptor heap. The output's rows are
//! padded to the 256-byte pitch a buffer-to-texture copy requires, and the composite then takes
//! the upload path's own route: a copy into level 0 of a texture made exactly as an upload makes
//! it, and the same sublevel generator.
//!
//! **Splatting.** The splat root signature points eleven one-descriptor tables straight at the
//! layer textures' existing descriptors, so nothing is copied or cached; the samplers are the
//! filtering bank's, and the per-draw layer description is root constants. A different root
//! signature resets every root binding, so after each splat draw the legacy one is put back
//! together with the texture tables the last legacy bind installed.

use super::*;
use crate::device::terrain::{pack_job, JOB_WORDS, MAX_OVERLAYS};
use crate::device::{MergeSource, TerrainMergeJob, TerrainSplat};

/// The compositor. Pixels are BGRA8 bytes read as little-endian `uint`s.
///
/// `job` layout, in `uint`s: `[size, pitch, base_present, base_offset, base_w, base_h,
/// base_tiling, overlay_count]`, then per overlay `[alpha_offset, alpha_w, alpha_h, rotation,
/// tex_present, tex_offset, tex_w, tex_h, tiling]`. Offsets are in pool words; `pitch` is the
/// output's row length in words.
const MERGE_SHADER: &str = r"
StructuredBuffer<uint> pool : register(t0);
StructuredBuffer<uint> job : register(t1);
RWStructuredBuffer<uint> out_px : register(u0);

// The debug colour a missing texture fills with: bytes 00 FF 00 00.
static const uint MISSING = 0x0000FF00u;

uint fetch_tiled(uint off, uint w, uint h, uint tiling, uint size, uint x, uint y)
{
    uint t = max(tiling, 1u);
    uint sx = (x * t * w / size) % w;
    uint sy = (y * t * h / size) % h;
    return pool[off + sy * w + sx];
}

// The alpha map's walk for each rotation, as `rotated_offset`.
uint rotated(uint rot, uint w, uint h, uint col, uint row)
{
    int W = (int)w;
    int H = (int)h;
    int c = (int)col;
    int r = (int)row;
    int idx;
    if (rot == 0u) { idx = c + r * W; }
    else if (rot == 1u) { idx = (W - 1) + c * W - r; }
    else if (rot == 2u) { idx = (W * H - 1) - c - r * W; }
    else { idx = (H - 1) * W - c * W + r; }
    return (uint)idx;
}

// `integer_blend`: a == 255 keeps the destination, a == 0 takes the overlay; alpha untouched.
uint blend(uint d, uint s, uint a)
{
    if (a == 255u) {
        return d;
    }
    uint ap = a > 128u ? a + 1u : a;
    uint ia = 256u - ap;
    uint o = d & 0xFF000000u;
    [unroll] for (uint c = 0u; c < 3u; c++) {
        uint sh = c * 8u;
        uint sc = (s >> sh) & 0xFFu;
        uint dc = (d >> sh) & 0xFFu;
        o |= (((sc * ia + dc * ap) >> 8u) & 0xFFu) << sh;
    }
    return o;
}

[numthreads(8, 8, 1)]
void cs_merge(uint3 gid : SV_DispatchThreadID)
{
    uint size = job[0];
    if (gid.x >= size || gid.y >= size) {
        return;
    }
    uint x = gid.x;
    uint y = gid.y;
    uint px = MISSING;
    if (job[2] != 0u) {
        px = fetch_tiled(job[3], job[4], job[5], job[6], size, x, y);
    }
    uint n = job[7];
    for (uint k = 0u; k < n; k++) {
        uint b = 8u + k * 9u;
        uint aw = job[b + 1u];
        uint ah = job[b + 2u];
        uint ax = x * aw / size;
        uint ay = y * ah / size;
        uint a = pool[job[b] + rotated(job[b + 3u], aw, ah, ax, ay)] >> 24u;
        uint s = MISSING;
        if (job[b + 4u] != 0u) {
            s = fetch_tiled(job[b + 5u], job[b + 6u], job[b + 7u], job[b + 8u], size, x, y);
        }
        px = blend(px, s, a);
    }
    out_px[y * job[1] + x] = px;
}
";

/// The splat pixel shader, appended to the legacy source. Its registers start past the legacy
/// ones (`t0`/`t1`, `s0`/`s1`, `b0`/`b1`) so the two sets never collide in one compilation.
const SPLAT_TAIL: &str = r"
cbuffer Splat : register(b2)
{
    // x = a base texture is bound, y = its tiling, z = the overlay count.
    uint4 g_splat_base;
    // Per overlay: x = the alpha map's quarter turns, y = the tile's tiling, z = a tile is bound.
    uint4 g_splat_layers[5];
};

Texture2D s_base : register(t2);
Texture2D s_alpha0 : register(t3);
Texture2D s_alpha1 : register(t4);
Texture2D s_alpha2 : register(t5);
Texture2D s_alpha3 : register(t6);
Texture2D s_alpha4 : register(t7);
Texture2D s_tex0 : register(t8);
Texture2D s_tex1 : register(t9);
Texture2D s_tex2 : register(t10);
Texture2D s_tex3 : register(t11);
Texture2D s_tex4 : register(t12);
SamplerState s_wrap : register(s2);
SamplerState s_clamp : register(s3);

// The debug colour a missing texture fills with: bytes 00 FF 00 00, green.
static const float4 SPLAT_MISSING = float4(0.0, 1.0, 0.0, 0.0);

// Where the composite's alpha-map walk lands for a cell coordinate, per rotation.
float2 splat_rotate(float2 uv, uint rot)
{
    if (rot == 1u) { return float2(1.0 - uv.y, uv.x); }
    if (rot == 2u) { return float2(1.0 - uv.x, 1.0 - uv.y); }
    if (rot == 3u) { return float2(uv.y, 1.0 - uv.x); }
    return uv;
}

// One overlay: the alpha map's alpha keeps the colour beneath (1) or takes the tile (0).
float4 splat_layer(float4 below, Texture2D alpha, Texture2D tile, uint k, float2 uv)
{
    uint4 l = g_splat_layers[k];
    float a = alpha.Sample(s_clamp, splat_rotate(uv, l.x)).a;
    float4 s = SPLAT_MISSING;
    if (l.z != 0u) {
        s = tile.Sample(s_wrap, uv * (float)max(l.y, 1u));
    }
    return float4(lerp(s.rgb, below.rgb, a), below.a);
}

float4 ps_splat(VSOut i) : SV_TARGET
{
    // The cell's texture coordinates run 0..1 across it with the key's rotation already applied,
    // exactly as they address the composite.
    float2 uv = saturate(i.uv0);
    // Only the layers the cell has are sampled. Every branch tests a root constant, the same for
    // the whole draw, so the samples stay in uniform control flow.
    float4 c = SPLAT_MISSING;
    if (g_splat_base.x != 0u) {
        c = s_base.Sample(s_wrap, uv * (float)max(g_splat_base.y, 1u));
    }
    uint n = g_splat_base.z;
    if (n > 0u) { c = splat_layer(c, s_alpha0, s_tex0, 0u, uv); }
    if (n > 1u) { c = splat_layer(c, s_alpha1, s_tex1, 1u, uv); }
    if (n > 2u) { c = splat_layer(c, s_alpha2, s_tex2, 2u, uv); }
    if (n > 3u) { c = splat_layer(c, s_alpha3, s_tex3, 3u, uv); }
    if (n > 4u) { c = splat_layer(c, s_alpha4, s_tex4, 4u, uv); }
    // The composite is the stage-0 texture of a MODULATE draw; so is this.
    float4 lit = c * diffuse_arg(i);
    alpha_test(lit.a);
    return finish(lit, i.fog);
}
";

const INITIAL_POOL_BYTES: u64 = 16 << 20;
/// The splat's layers: a base, five alpha maps and five tiles.
const MAX_LAYERS: usize = 5;
const SPLAT_IMAGES: u32 = 1 + 2 * MAX_LAYERS as u32;
const SPLAT_CONSTANTS: u32 = 4 * (1 + MAX_LAYERS as u32);
/// Root parameter indices of the splat signature: the two constant buffers, the eleven texture
/// tables, the two sampler tables, the root constants.
const SPLAT_FIRST_TEXTURE: u32 = 2;
const SPLAT_FIRST_SAMPLER: u32 = SPLAT_FIRST_TEXTURE + SPLAT_IMAGES;
const SPLAT_CONSTANTS_PARAM: u32 = SPLAT_FIRST_SAMPLER + 2;

/// The compositor's objects, built on first use.
#[derive(Debug)]
pub(super) struct TerrainMerge {
    root: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    /// The sources, in `NON_PIXEL_SHADER_RESOURCE` between uses.
    pool: ID3D12Resource,
    pool_size: u64,
    pool_used: u64,
    /// Each source's `(word offset, width, height)`, indexed by [`MergeSource`].
    sources: Vec<(u32, u32, u32)>,
    /// The job words; an upload-heap buffer the CPU writes and the shader reads.
    job: ID3D12Resource,
    /// The composite, in `UNORDERED_ACCESS` between uses.
    output: ID3D12Resource,
    output_size: u64,
}

/// The splat pipeline's objects, built on first use.
#[derive(Debug)]
pub(super) struct TerrainSplatState {
    root: ID3D12RootSignature,
    shader: Vec<u8>,
    psos: HashMap<PipelineKey, ID3D12PipelineState>,
    /// A 1 x 1 texture bound wherever a draw has nothing to bind.
    stand_in: TextureSlot,
}

impl Gpu {
    // ---- composing ------------------------------------------------------------------------------

    /// Always: every feature-level 11.0 device runs compute shaders.
    #[must_use]
    pub fn terrain_merge_supported(&self) -> bool {
        true
    }

    /// Make a decoded BGRA8 source image resident for [`Self::merge_terrain_texture`].
    ///
    /// # Errors
    /// A byte count that does not match the extent, or a device failure.
    pub fn upload_merge_source(
        &mut self,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> Result<MergeSource, RenderError> {
        let len = u64::from(width) * u64::from(height) * 4;
        if width == 0 || height == 0 || bgra.len() as u64 != len {
            return Err(RenderError::BadDimensions {
                width,
                height,
                reason: "a merge source is width x height BGRA8 texels",
            });
        }
        let mut tm = self.take_terrain_merge()?;
        let result = self.upload_merge_source_into(&mut tm, width, height, bgra);
        self.terrain_merge = Some(tm);
        result
    }

    /// Forget every resident source; their pool space is reused.
    pub fn reset_merge_sources(&mut self) {
        if let Some(tm) = self.terrain_merge.as_mut() {
            tm.pool_used = 0;
            tm.sources.clear();
        }
    }

    /// Compose one landscape surface on the device, as a texture with the levels an uploaded
    /// composite gets. One link, owned by the caller.
    ///
    /// # Errors
    /// A key that is not a world owner's, a job naming a source this device does not hold or more
    /// overlays than the shader takes, or a device failure.
    pub fn merge_terrain_texture(
        &mut self,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        if key.space() != crate::TextureSpace::World {
            return Err(RenderError::Unsupported(
                "runtime image texture mips require a world-owner key",
            ));
        }
        if let Some(slot) = self.texture_book.get(key) {
            return Ok(TextureSlot(slot));
        }
        if job.size == 0 {
            return Err(RenderError::BadDimensions {
                width: 0,
                height: 0,
                reason: "an empty terrain composite",
            });
        }
        if job.overlays.len() > MAX_OVERLAYS {
            return Err(RenderError::Unsupported(
                "too many overlays in one terrain composite",
            ));
        }
        let mut tm = self.take_terrain_merge()?;
        let result = self.merge_into_texture(&mut tm, key, job);
        self.terrain_merge = Some(tm);
        result
    }

    fn take_terrain_merge(&mut self) -> Result<TerrainMerge, RenderError> {
        match self.terrain_merge.take() {
            Some(tm) => Ok(tm),
            None => self.create_terrain_merge(),
        }
    }

    fn create_terrain_merge(&mut self) -> Result<TerrainMerge, RenderError> {
        let root_param = |t: D3D12_ROOT_PARAMETER_TYPE, register: u32| D3D12_ROOT_PARAMETER {
            ParameterType: t,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Descriptor: D3D12_ROOT_DESCRIPTOR {
                    ShaderRegister: register,
                    RegisterSpace: 0,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        };
        let params = [
            root_param(D3D12_ROOT_PARAMETER_TYPE_SRV, 0),
            root_param(D3D12_ROOT_PARAMETER_TYPE_SRV, 1),
            root_param(D3D12_ROOT_PARAMETER_TYPE_UAV, 0),
        ];
        let root = serialize_root_signature(&self.device, &params, D3D12_ROOT_SIGNATURE_FLAG_NONE)?;
        let code = compile_source(MERGE_SHADER.as_bytes(), &[], c"cs_merge", c"cs_5_0")?;
        let desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(root.clone())),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: code.as_ptr().cast(),
                BytecodeLength: code.len(),
            },
            ..Default::default()
        };
        // SAFETY: the descriptor and the bytecode outlive the call.
        let made = unsafe { self.device.CreateComputePipelineState(&desc) };
        let mut desc = desc;
        // SAFETY: the descriptor owns exactly one cloned COM reference; it is no longer read.
        unsafe { std::mem::ManuallyDrop::drop(&mut desc.pRootSignature) };
        let pipeline: ID3D12PipelineState = hr("CreateComputePipelineState(terrain merge)", made)?;
        let pool = self.create_buffer(
            INITIAL_POOL_BYTES,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        self.one_shot(|list| {
            // SAFETY: the list is open and the pool is fresh in COPY_DEST.
            unsafe {
                Self::transition_on(
                    list,
                    &pool,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                );
            }
            Ok(())
        })?;
        let job = Self::create_upload_buffer(&self.device, (JOB_WORDS * 4) as u64)?;
        let output_size = 512 * 512 * 4;
        let output = self.create_buffer(
            output_size,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        )?;
        Ok(TerrainMerge {
            root,
            pipeline,
            pool,
            pool_size: INITIAL_POOL_BYTES,
            pool_used: 0,
            sources: Vec::new(),
            job,
            output,
            output_size,
        })
    }

    /// A default-heap buffer.
    fn create_buffer(
        &self,
        bytes: u64,
        flags: D3D12_RESOURCE_FLAGS,
        state: D3D12_RESOURCE_STATES,
    ) -> Result<ID3D12Resource, RenderError> {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: flags,
            ..Default::default()
        };
        let mut buffer: Option<ID3D12Resource> = None;
        // SAFETY: descriptors are live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(buffer)", unsafe {
            self.device.CreateCommittedResource(
                &default_heap(),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                state,
                None,
                &mut buffer,
            )
        })?;
        buffer.ok_or_else(|| RenderError::Device("no buffer".into()))
    }

    /// Record, submit and wait for one command list of its own, as a texture upload does.
    fn one_shot(
        &mut self,
        record: impl FnOnce(&ID3D12GraphicsCommandList) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        // SAFETY: a fresh allocator owned by this scope.
        let alloc: ID3D12CommandAllocator = hr("CreateCommandAllocator(terrain)", unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        })?;
        // SAFETY: `alloc` outlives the list, which is closed and waited for before this returns.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList(terrain)", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None)
        })?;
        record(&list)?;
        // SAFETY: recording is finished; the queue executes the closed list.
        unsafe {
            hr("Close", list.Close())?;
            let lists =
                [Some(list.cast::<ID3D12CommandList>().map_err(|e| {
                    RenderError::Device(format!("cast: {e}"))
                })?)];
            self.queue.ExecuteCommandLists(&lists);
        }
        self.wait_idle()
    }

    fn upload_merge_source_into(
        &mut self,
        tm: &mut TerrainMerge,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> Result<MergeSource, RenderError> {
        let len = bgra.len() as u64;
        let need = tm.pool_used + len;
        if need > tm.pool_size {
            // Grow: a new pool at least twice the size, the used part copied across.
            let size = need.next_power_of_two().max(tm.pool_size * 2);
            let bigger = self.create_buffer(
                size,
                D3D12_RESOURCE_FLAG_NONE,
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            let (old, used) = (tm.pool.clone(), tm.pool_used);
            self.one_shot(|list| {
                // SAFETY: the list is open; both buffers are live until after the wait.
                unsafe {
                    Self::transition_on(
                        list,
                        &old,
                        D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                        D3D12_RESOURCE_STATE_COPY_SOURCE,
                    );
                    if used > 0 {
                        list.CopyBufferRegion(&bigger, 0, &old, 0, used);
                    }
                    Self::transition_on(
                        list,
                        &bigger,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                    );
                }
                Ok(())
            })?;
            tm.pool = bigger;
            tm.pool_size = size;
        }
        let staging = Self::create_upload_buffer(&self.device, len)?;
        // SAFETY: the staging buffer is host-visible and `len` bytes long.
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let read = D3D12_RANGE { Begin: 0, End: 0 };
            hr(
                "Map(merge source)",
                staging.Map(0, Some(&read), Some(&mut ptr)),
            )?;
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), ptr.cast::<u8>(), bgra.len());
            staging.Unmap(0, None);
        }
        let (pool, offset) = (tm.pool.clone(), tm.pool_used);
        self.one_shot(|list| {
            // SAFETY: the list is open; both buffers are live until after the wait.
            unsafe {
                Self::transition_on(
                    list,
                    &pool,
                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                );
                list.CopyBufferRegion(&pool, offset, &staging, 0, len);
                Self::transition_on(
                    list,
                    &pool,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                );
            }
            Ok(())
        })?;
        let index = u32::try_from(tm.sources.len())
            .map_err(|_| RenderError::Unsupported("too many merge sources"))?;
        tm.sources.push(((offset / 4) as u32, width, height));
        tm.pool_used += len;
        Ok(MergeSource(index))
    }

    fn merge_into_texture(
        &mut self,
        tm: &mut TerrainMerge,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        let source = |s: MergeSource| {
            tm.sources
                .get(s.0 as usize)
                .copied()
                .ok_or(RenderError::Unsupported(
                    "a merge source this device does not hold",
                ))
        };
        let size = job.size;
        // A buffer-to-texture copy needs 256-byte rows; the shader writes at this pitch.
        let pitch_bytes = (u64::from(size) * 4)
            .div_ceil(u64::from(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT))
            * u64::from(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT);
        let words = pack_job(job, (pitch_bytes / 4) as u32, source)?;
        // SAFETY: the job buffer is host-visible and JOB_WORDS words long; the previous job was
        // waited for.
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let read = D3D12_RANGE { Begin: 0, End: 0 };
            hr("Map(merge job)", tm.job.Map(0, Some(&read), Some(&mut ptr)))?;
            std::ptr::copy_nonoverlapping(words.as_ptr(), ptr.cast::<u32>(), words.len());
            tm.job.Unmap(0, None);
        }
        let out_len = pitch_bytes * u64::from(size);
        if out_len > tm.output_size {
            tm.output = self.create_buffer(
                out_len,
                D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            )?;
            tm.output_size = out_len;
        }

        // The same level count, resource and sublevel generation an uploaded one-level BGRA8
        // composite gets.
        let shape = TextureData {
            width: size,
            height: size,
            format: TextureFormat::Bgra8,
            levels: vec![Vec::new()],
        };
        let levels = crate::mip::runtime_level_count(&shape, self.imgtex_autogen_supported);
        let levels = u16::try_from(levels)
            .map_err(|_| RenderError::Unsupported("too many texture levels"))?;
        let generate = levels > 1;
        if generate && self.mip_generator.is_none() {
            self.mip_generator = Some(mipgen::MipGenerator::new(&self.device)?);
        }
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(size),
            Height: size,
            DepthOrArraySize: 1,
            MipLevels: levels,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: if generate {
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
            } else {
                D3D12_RESOURCE_FLAG_NONE
            },
            ..Default::default()
        };
        let mut texture: Option<ID3D12Resource> = None;
        // SAFETY: descriptors are live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(terrain composite)", unsafe {
            self.device.CreateCommittedResource(
                &default_heap(),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut texture,
            )
        })?;
        let texture = texture.ok_or_else(|| RenderError::Device("no texture".into()))?;

        let (root, pipeline, pool, job_buf, output) = (
            tm.root.clone(),
            tm.pipeline.clone(),
            tm.pool.clone(),
            tm.job.clone(),
            tm.output.clone(),
        );
        let groups = size.div_ceil(8);
        let mut mip_views = None;
        // Out of `self` while the list records, so the recording borrows it rather than `self`.
        let generator = if generate {
            self.mip_generator.take()
        } else {
            None
        };
        let device = self.device.clone();
        let recorded = self.one_shot(|list| {
            // SAFETY: the list is open; every resource is live until after the wait, and each
            // barrier names the state the resource is in.
            unsafe {
                list.SetComputeRootSignature(&root);
                list.SetPipelineState(&pipeline);
                list.SetComputeRootShaderResourceView(0, pool.GetGPUVirtualAddress());
                list.SetComputeRootShaderResourceView(1, job_buf.GetGPUVirtualAddress());
                list.SetComputeRootUnorderedAccessView(2, output.GetGPUVirtualAddress());
                list.Dispatch(groups, groups, 1);
                Self::transition_on(
                    list,
                    &output,
                    D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                );
                let dst = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&texture),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        SubresourceIndex: 0,
                    },
                };
                let src = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&output),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                            Offset: 0,
                            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                Width: size,
                                Height: size,
                                Depth: 1,
                                RowPitch: pitch_bytes as u32,
                            },
                        },
                    },
                };
                list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
                Self::transition_on(
                    list,
                    &output,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                    D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                );
                if let Some(g) = generator.as_ref() {
                    mip_views = Some(g.record(&device, list, &texture, size, size, levels)?);
                } else {
                    Self::transition_on(
                        list,
                        &texture,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                    );
                }
            }
            Ok(())
        });
        if generator.is_some() {
            self.mip_generator = generator;
        }
        recorded?;
        drop(mip_views);
        self.register_texture(key, texture, DXGI_FORMAT_B8G8R8A8_UNORM, levels)
    }

    // ---- splatting ----------------------------------------------------------------------------

    /// Always: the splat root signature is well inside feature level 11.0's limits.
    #[must_use]
    pub fn terrain_splat_supported(&self) -> bool {
        true
    }

    /// Draw landscape triangles with `key`'s fixed-function state, blending `splat`'s layers in
    /// the pixel shader. Otherwise exactly [`Self::draw_dynamic`].
    ///
    /// # Errors
    /// More overlays than the shader takes, a texture slot that is not live, or a device failure.
    pub fn draw_terrain_splat(
        &mut self,
        key: &PipelineKey,
        splat: &TerrainSplat,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
    ) -> Result<(), RenderError> {
        let stride = key.vertex_format.stride();
        if stride == 0 || !vertices.len().is_multiple_of(stride as usize) {
            return Err(RenderError::Device(
                "vertex data is not a whole number of vertices".into(),
            ));
        }
        if splat.overlays.len() > MAX_LAYERS {
            return Err(RenderError::Unsupported(
                "too many overlays in one splat draw",
            ));
        }
        let mut state = match self.terrain_splat.take() {
            Some(s) => s,
            None => self.create_terrain_splat()?,
        };
        let pso = match state.psos.get(key) {
            Some(p) => Ok(p.clone()),
            None => self
                .build_pso_with(key, &state.shader, &state.root)
                .inspect(|p| {
                    state.psos.insert(*key, p.clone());
                }),
        };
        let (root, stand_in) = (state.root.clone(), state.stand_in);
        self.terrain_splat = Some(state);
        let pso = pso?;

        let mut slots = [stand_in; SPLAT_IMAGES as usize];
        if let Some(b) = splat.base {
            slots[0] = b;
        }
        let mut constants = [0u32; SPLAT_CONSTANTS as usize];
        constants[0] = u32::from(splat.base.is_some());
        constants[1] = splat.base_tiling;
        constants[2] = splat.overlays.len() as u32;
        for (k, o) in splat.overlays.iter().enumerate() {
            slots[1 + k] = o.alpha;
            if let Some(t) = o.tex {
                slots[1 + MAX_LAYERS + k] = t;
            }
            constants[4 + k * 4] = o.rotation;
            constants[4 + k * 4 + 1] = o.tiling;
            constants[4 + k * 4 + 2] = u32::from(o.tex.is_some());
        }
        if let Some(s) = slots.iter().find(|s| !self.textures.contains_key(&s.0)) {
            return Err(RenderError::Device(format!(
                "splat texture slot {} is not live",
                s.0
            )));
        }

        let count = vertices.len() as u32 / stride;
        self.draw_calls += 1;
        let mut draw = *per_draw;
        draw.uv_offset = [0.0, 0.0, per_draw.uv_offset[2], per_draw.uv_offset[3]];
        draw.texture_factor = unpack_argb(crate::DrawConstants::default().texture_factor);
        draw.draw_params = [
            f32::from(crate::DrawConstants::default().alpha_ref) / 255.0,
            if key.alpha_test { 1.0 } else { 0.0 },
            if key.fog { 1.0 } else { 0.0 },
            draw.draw_params[3],
        ];
        let mut frame = *per_frame;
        frame.screen[0] = self.gamma;
        let vb = self.upload_bytes(vertices)?;
        let frame_cb = self.upload_bytes(as_bytes(&frame))?;
        let draw_cb = self.upload_bytes(as_bytes(&draw))?;
        let wrap = self.sampler_descriptor_index(0);
        let clamp = self.sampler_descriptor_index(1);

        // SAFETY: the list is open inside the frame; the PSO, both root signatures and every heap
        // offset are live, and the upload addresses stay alive until this frame's fence retires.
        unsafe {
            self.list.SetGraphicsRootSignature(&root);
            self.list.SetPipelineState(&pso);
            self.list.SetGraphicsRootConstantBufferView(0, frame_cb);
            self.list.SetGraphicsRootConstantBufferView(1, draw_cb);
            let srv0 = self.srv_heap.GetGPUDescriptorHandleForHeapStart();
            for (i, s) in slots.iter().enumerate() {
                let mut h = srv0;
                h.ptr += u64::from(s.0) * u64::from(self.srv_size);
                self.list
                    .SetGraphicsRootDescriptorTable(SPLAT_FIRST_TEXTURE + i as u32, h);
            }
            let s0 = self.sampler_heap.GetGPUDescriptorHandleForHeapStart();
            for (i, d) in [wrap, clamp].iter().enumerate() {
                let mut h = s0;
                h.ptr += u64::from(*d) * u64::from(self.sampler_size);
                self.list
                    .SetGraphicsRootDescriptorTable(SPLAT_FIRST_SAMPLER + i as u32, h);
            }
            self.list.SetGraphicsRoot32BitConstants(
                SPLAT_CONSTANTS_PARAM,
                SPLAT_CONSTANTS,
                constants.as_ptr().cast(),
                0,
            );
            let view = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: vb,
                SizeInBytes: vertices.len() as u32,
                StrideInBytes: stride,
            };
            self.list.IASetVertexBuffers(0, Some(&[view]));
            self.list.DrawInstanced(count, 1, 0, 0);
            // Put the legacy signature back, and the texture tables its draws may be relying on.
            self.list.SetGraphicsRootSignature(&self.root_signature);
            for (i, t) in self.legacy_tables.get().iter().enumerate() {
                if let Some(ptr) = t {
                    self.list.SetGraphicsRootDescriptorTable(
                        2 + i as u32,
                        D3D12_GPU_DESCRIPTOR_HANDLE { ptr: *ptr },
                    );
                }
            }
        }
        Ok(())
    }

    /// Nothing is cached per binding here, so there is nothing to drop.
    pub fn reset_terrain_splat(&mut self) {}

    fn create_terrain_splat(&mut self) -> Result<TerrainSplatState, RenderError> {
        let range = |t: D3D12_DESCRIPTOR_RANGE_TYPE, register: u32| D3D12_DESCRIPTOR_RANGE {
            RangeType: t,
            NumDescriptors: 1,
            BaseShaderRegister: register,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        // Textures t2..t12, samplers s2 and s3: past the legacy registers, see `SPLAT_TAIL`.
        let ranges: Vec<D3D12_DESCRIPTOR_RANGE> = (0..SPLAT_IMAGES)
            .map(|i| range(D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 2 + i))
            .chain((0..2).map(|i| range(D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER, 2 + i)))
            .collect();
        let cbv = |register: u32| D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Descriptor: D3D12_ROOT_DESCRIPTOR {
                    ShaderRegister: register,
                    RegisterSpace: 0,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        };
        let mut params = vec![cbv(0), cbv(1)];
        for r in &ranges {
            params.push(D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::from_ref(r),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            });
        }
        params.push(D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 2,
                    RegisterSpace: 0,
                    Num32BitValues: SPLAT_CONSTANTS,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        });
        debug_assert_eq!(params.len() as u32, SPLAT_CONSTANTS_PARAM + 1);
        let root = serialize_root_signature(
            &self.device,
            &params,
            D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        )?;
        let source = format!("{SHADER_SOURCE}\n{SPLAT_TAIL}");
        let shader = compile_source(source.as_bytes(), &[], c"ps_splat", c"ps_5_0")?;
        let stand_in = self.upload_texture(&TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0, 0, 0, 0]],
        })?;
        Ok(TerrainSplatState {
            root,
            shader,
            psos: HashMap::new(),
            stand_in,
        })
    }
}

/// Serialize and create a root signature.
fn serialize_root_signature(
    device: &ID3D12Device,
    params: &[D3D12_ROOT_PARAMETER],
    flags: D3D12_ROOT_SIGNATURE_FLAGS,
) -> Result<ID3D12RootSignature, RenderError> {
    let desc = D3D12_ROOT_SIGNATURE_DESC {
        NumParameters: params.len() as u32,
        pParameters: params.as_ptr(),
        NumStaticSamplers: 0,
        pStaticSamplers: std::ptr::null(),
        Flags: flags,
    };
    let mut blob: Option<ID3DBlob> = None;
    let mut error: Option<ID3DBlob> = None;
    // SAFETY: `desc` and what it points at outlive the call; the out-parameters are read after.
    let r = unsafe {
        D3D12SerializeRootSignature(
            &desc,
            D3D_ROOT_SIGNATURE_VERSION_1,
            &mut blob,
            Some(&mut error),
        )
    };
    if let Err(e) = r {
        let msg = error.map_or_else(String::new, |b| blob_text(&b));
        return Err(RenderError::Device(format!(
            "D3D12SerializeRootSignature: {e} {msg}"
        )));
    }
    let blob = blob.ok_or_else(|| RenderError::Device("no root signature blob".into()))?;
    // SAFETY: the blob's buffer is valid until the blob is dropped, which is after this call.
    let bytes = unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    };
    // SAFETY: `bytes` is a live slice for the duration of the call.
    hr("CreateRootSignature", unsafe {
        device.CreateRootSignature(0, bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both shaders compile with the device's compiler, without a device.
    #[test]
    fn the_compositor_and_the_splat_shader_compile() {
        compile_source(MERGE_SHADER.as_bytes(), &[], c"cs_merge", c"cs_5_0")
            .expect("the compositor compiles");
        let source = format!("{SHADER_SOURCE}\n{SPLAT_TAIL}");
        compile_source(source.as_bytes(), &[], c"ps_splat", c"ps_5_0")
            .expect("the splat shader compiles");
    }
}
