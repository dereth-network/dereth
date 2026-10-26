//! Depth probes on Direct3D 12: the depth the frame left at a few pixels, read back without
//! waiting.
//!
//! A depth-stencil resource is copied only whole, so the probed pixels are not copied out of it.
//! At the end of the frame's list a small compute pass loads the depth at each probed pixel
//! through a view of the depth plane's depth, and writes the bits of the depth it read into one
//! word of a small buffer; those words, four bytes a probe, are copied into the frame's own
//! read-back buffer, which is read once the frame's fence has passed. A pixel outside the frame
//! reads its nearest edge pixel, as on the other devices.
//!
//! The pass's view of the plane lives in a shader-visible heap of its own, one descriptor, bound
//! for the pass alone: it is the last thing the frame's list records, so nothing after it needs
//! the frame's own heap back.

use super::*;
use crate::device::{DepthProbes, MAX_DEPTH_PROBES};

/// Each thread loads one probe's depth: its pixel's texel of the plane, as the bits of the depth
/// the view reads.
const SHADER: &str = r"
Texture2D<float> plane : register(t0);
StructuredBuffer<uint2> pixels : register(t1);
RWStructuredBuffer<uint> depths : register(u0);
cbuffer Probes : register(b0)
{
    uint count;
};

[numthreads(64, 1, 1)]
void cs_probe(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= count) {
        return;
    }
    depths[id.x] = asuint(plane.Load(int3(pixels[id.x], 0)));
}
";

/// The threads in a group, as the shader declares them.
const GROUP: u32 = 64;

/// The words the pass writes and a frame reads back, at most: four bytes a probe.
pub(super) const PROBE_BYTES: u64 = (MAX_DEPTH_PROBES * 4) as u64;

/// The probe pass's objects, made on first use.
#[derive(Debug)]
pub(super) struct ProbePass {
    root: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    /// A shader-visible heap of one descriptor: the view of the depth plane's depth, made again
    /// whenever the plane is.
    heap: ID3D12DescriptorHeap,
    /// The words the pass writes, in `UNORDERED_ACCESS` between frames.
    output: ID3D12Resource,
}

impl ProbePass {
    fn new(gpu: &Gpu) -> Result<Self, RenderError> {
        let range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        let descriptor = |t: D3D12_ROOT_PARAMETER_TYPE, register: u32| D3D12_ROOT_PARAMETER {
            ParameterType: t,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Descriptor: D3D12_ROOT_DESCRIPTOR {
                    ShaderRegister: register,
                    RegisterSpace: 0,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        };
        // The plane's view, the pixels, the depths, and the count.
        let params = [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::from_ref(&range),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            descriptor(D3D12_ROOT_PARAMETER_TYPE_SRV, 1),
            descriptor(D3D12_ROOT_PARAMETER_TYPE_UAV, 0),
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                        Num32BitValues: 1,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        ];
        let root = super::terrain::serialize_root_signature(
            &gpu.device,
            &params,
            D3D12_ROOT_SIGNATURE_FLAG_NONE,
        )?;
        let code = compile_source(SHADER.as_bytes(), &[], c"cs_probe", c"cs_5_0")?;
        let desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(root.clone())),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: code.as_ptr().cast(),
                BytecodeLength: code.len(),
            },
            ..Default::default()
        };
        // SAFETY: the descriptor and the bytecode outlive the call.
        let made = unsafe { gpu.device.CreateComputePipelineState(&desc) };
        let mut desc = desc;
        // SAFETY: the descriptor owns exactly one cloned COM reference; it is no longer read.
        unsafe { std::mem::ManuallyDrop::drop(&mut desc.pRootSignature) };
        let pipeline: ID3D12PipelineState = hr("CreateComputePipelineState(depth probes)", made)?;
        let heap = Gpu::heap(&gpu.device, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 1, true)?;
        let output = buffer(
            &gpu.device,
            &default_heap(),
            PROBE_BYTES,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        )?;
        Ok(Self {
            root,
            pipeline,
            heap,
            output,
        })
    }

    /// Make the pass's view of `depth`, the depth plane, in its heap. Only while no frame in
    /// flight reads the view: when the pass is made, and when the plane is made again behind a
    /// wait for the device.
    pub(super) fn view(&self, device: &ID3D12Device, depth: &ID3D12Resource) {
        let view = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };
        // SAFETY: the heap holds one descriptor, written here, and `depth` is a live typeless
        // depth-stencil plane the view's format reads.
        unsafe {
            device.CreateShaderResourceView(
                depth,
                Some(&view),
                self.heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }
    }
}

/// A committed buffer of `bytes` on `heap`, in `state`.
fn buffer(
    device: &ID3D12Device,
    heap: &D3D12_HEAP_PROPERTIES,
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
    let mut made: Option<ID3D12Resource> = None;
    // SAFETY: descriptors are live locals; the out-parameter is read only on success.
    hr("CreateCommittedResource(depth probes)", unsafe {
        device.CreateCommittedResource(heap, D3D12_HEAP_FLAG_NONE, &desc, state, None, &mut made)
    })?;
    made.ok_or_else(|| RenderError::Device("no depth probe buffer".into()))
}

impl Gpu {
    /// At the end of the frame's list: load the depth at each of `pixels` (each held to the
    /// frame, so a pixel outside it reads its nearest edge pixel) into the pass's words, and copy
    /// those words, four bytes a probe, into this frame's read-back buffer.
    pub(super) fn record_depth_probes(
        &mut self,
        tag: u64,
        pixels: &[(u32, u32)],
    ) -> Result<(), RenderError> {
        let n = pixels.len().min(MAX_DEPTH_PROBES);
        if n == 0 {
            return Ok(());
        }
        if self.probe_pass.is_none() {
            let pass = ProbePass::new(self)?;
            pass.view(&self.device, &self.depth);
            self.probe_pass = Some(pass);
        }
        let (w, h) = (self.config.width, self.config.height);
        let mut at = Vec::with_capacity(n * 8);
        for &(x, y) in &pixels[..n] {
            at.extend_from_slice(&x.min(w.saturating_sub(1)).to_le_bytes());
            at.extend_from_slice(&y.min(h.saturating_sub(1)).to_le_bytes());
        }
        let at = self.upload_bytes(&at)?;
        let index = self.frame_index;
        let readback = match &self.frames[index].probe_readback {
            Some(b) => b.clone(),
            None => {
                let b = buffer(
                    &self.device,
                    &heap_props(D3D12_HEAP_TYPE_READBACK),
                    PROBE_BYTES,
                    D3D12_RESOURCE_FLAG_NONE,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                )?;
                self.frames[index].probe_readback = Some(b.clone());
                b
            }
        };
        let count = n as u32;
        let pass = self.probe_pass.as_ref().expect("the pass was made above");
        // SAFETY: the frame's list is open; the depth plane is in DEPTH_WRITE, as the frame left
        // it, and is put back there; the pass's words are in UNORDERED_ACCESS between frames and
        // are put back there; the pixels' upload stays alive until this frame's fence passes.
        unsafe {
            self.transition(
                &self.depth,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
            );
            self.list.SetComputeRootSignature(&pass.root);
            self.list.SetPipelineState(&pass.pipeline);
            self.list.SetDescriptorHeaps(&[Some(pass.heap.clone())]);
            self.list
                .SetComputeRootDescriptorTable(0, pass.heap.GetGPUDescriptorHandleForHeapStart());
            self.list.SetComputeRootShaderResourceView(1, at);
            self.list
                .SetComputeRootUnorderedAccessView(2, pass.output.GetGPUVirtualAddress());
            self.list.SetComputeRoot32BitConstant(3, count, 0);
            self.list.Dispatch(count.div_ceil(GROUP), 1, 1);
            self.transition(
                &self.depth,
                D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
            );
            self.transition(
                &pass.output,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            );
            self.list
                .CopyBufferRegion(&readback, 0, &pass.output, 0, u64::from(count) * 4);
            self.transition(
                &pass.output,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            );
        }
        self.frames[index].probe = Some((tag, n));
        Ok(())
    }

    /// Read the `n` depths a finished frame's pass left in its slot's read-back buffer.
    pub(super) fn read_depth_probes(&mut self, tag: u64, n: usize) {
        let Some(readback) = &self.frames[self.frame_index].probe_readback else {
            return;
        };
        let bytes = n * 4;
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let read = D3D12_RANGE {
            Begin: 0,
            End: bytes,
        };
        // SAFETY: the fence wait in `begin_frame` proved the copy finished; the buffer holds at
        // least `bytes`, and the mapping is used only inside this block and unmapped before it
        // ends.
        let depths = unsafe {
            if readback.Map(0, Some(&read), Some(&mut ptr)).is_err() || ptr.is_null() {
                return;
            }
            let words = std::slice::from_raw_parts(ptr.cast::<u8>(), bytes);
            let depths = words
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_bits(u32::from_le_bytes(*b)))
                .collect();
            let written = D3D12_RANGE { Begin: 0, End: 0 };
            readback.Unmap(0, Some(&written));
            depths
        };
        self.probe_result = Some(DepthProbes { tag, depths });
    }
}

#[cfg(test)]
mod tests {
    /// The pass's shader compiles with the device's compiler, without a device.
    #[test]
    fn the_depth_probe_shader_compiles() {
        super::compile_source(super::SHADER.as_bytes(), &[], c"cs_probe", c"cs_5_0")
            .expect("the depth probe shader compiles");
    }
}
