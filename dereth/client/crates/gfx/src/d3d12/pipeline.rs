//! The root signature every pipeline shares, the pipelines made from DXIL, and the command
//! signatures indirect draws are read with, on Direct3D 12.

use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::{formats, hr, Device, Garbage};
use crate::{Compare, ComputeDesc, Cull, Error, Factor, GraphicsDesc, Pipeline, PUSH_BYTES};

/// A live pipeline, and whether it computes.
pub(crate) struct PipelineEntry {
    pub pipeline: ID3D12PipelineState,
    pub compute: bool,
}

/// The command signatures indirect draws are read with: one root constant (the draw's item),
/// then the draw's arguments, the stride the layer's command layout has.
#[derive(Debug)]
pub(crate) struct Signatures {
    pub indexed: ID3D12CommandSignature,
    pub plain: ID3D12CommandSignature,
}

/// The bytes one indexed command takes, and one plain command.
pub(crate) const INDEXED_STRIDE: u32 = 24;
pub(crate) const PLAIN_STRIDE: u32 = 20;

impl Signatures {
    pub(crate) fn new(device: &ID3D12Device, root: &ID3D12RootSignature) -> Result<Self, Error> {
        let make = |draw: D3D12_INDIRECT_ARGUMENT_TYPE, stride: u32| {
            let args = [
                D3D12_INDIRECT_ARGUMENT_DESC {
                    Type: D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT,
                    Anonymous: D3D12_INDIRECT_ARGUMENT_DESC_0 {
                        Constant: D3D12_INDIRECT_ARGUMENT_DESC_0_1 {
                            RootParameterIndex: 1,
                            DestOffsetIn32BitValues: 0,
                            Num32BitValuesToSet: 1,
                        },
                    },
                },
                D3D12_INDIRECT_ARGUMENT_DESC {
                    Type: draw,
                    ..Default::default()
                },
            ];
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: stride,
                NumArgumentDescs: 2,
                pArgumentDescs: args.as_ptr(),
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            // SAFETY: live locals and a live root signature.
            hr("CreateCommandSignature", unsafe {
                device.CreateCommandSignature(&desc, root, &mut sig)
            })?;
            sig.ok_or_else(|| Error::Invalid("the runtime made no command signature".into()))
        };
        Ok(Self {
            indexed: make(D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED, INDEXED_STRIDE)?,
            plain: make(D3D12_INDIRECT_ARGUMENT_TYPE_DRAW, PLAIN_STRIDE)?,
        })
    }
}

/// The root signature every pipeline shares: the push block as root constants at `b0`, the
/// draw's item as one at `b1`, and the descriptor heaps indexed by the shaders themselves.
pub(crate) fn root_signature(device: &ID3D12Device) -> Result<ID3D12RootSignature, Error> {
    let params = [
        D3D12_ROOT_PARAMETER1 {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER1_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 0,
                    RegisterSpace: 0,
                    Num32BitValues: PUSH_BYTES / 4,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        },
        D3D12_ROOT_PARAMETER1 {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER1_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 1,
                    RegisterSpace: 0,
                    Num32BitValues: 1,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        },
    ];
    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: 2,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: D3D12_ROOT_SIGNATURE_FLAGS(
                    D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED.0
                        | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED.0,
                ),
            },
        },
    };
    let mut blob: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: live locals; the blobs are released when they go.
    let made = unsafe { D3D12SerializeVersionedRootSignature(&desc, &mut blob, Some(&mut errors)) };
    if let Err(e) = made {
        let detail = errors.map(|b| blob_text(&b)).unwrap_or_default();
        return Err(Error::D3d12 {
            what: "D3D12SerializeVersionedRootSignature",
            detail: format!("{e} {detail}"),
        });
    }
    let blob = blob.ok_or_else(|| Error::Invalid("no root signature was serialized".into()))?;
    // SAFETY: the blob's bytes live as long as the blob, for the call.
    let bytes = unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    };
    // SAFETY: a serialized root signature.
    hr("CreateRootSignature", unsafe {
        device.CreateRootSignature(0, bytes)
    })
}

fn blob_text(b: &ID3DBlob) -> String {
    // SAFETY: the blob's bytes live as long as the blob.
    let bytes =
        unsafe { std::slice::from_raw_parts(b.GetBufferPointer().cast::<u8>(), b.GetBufferSize()) };
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .to_owned()
}

/// The shader bytecode of a DXIL blob, which must not be empty.
fn bytecode(dxil: &[u8], entry: &str) -> Result<D3D12_SHADER_BYTECODE, Error> {
    if dxil.is_empty() {
        return Err(Error::Unsupported(format!(
            "the entry point {entry} has no DXIL"
        )));
    }
    Ok(D3D12_SHADER_BYTECODE {
        pShaderBytecode: dxil.as_ptr().cast(),
        BytecodeLength: dxil.len(),
    })
}

fn blend(f: Factor, alpha: bool) -> D3D12_BLEND {
    match f {
        Factor::Zero => D3D12_BLEND_ZERO,
        Factor::One => D3D12_BLEND_ONE,
        Factor::SrcAlpha => D3D12_BLEND_SRC_ALPHA,
        Factor::OneMinusSrcAlpha => D3D12_BLEND_INV_SRC_ALPHA,
        // In the alpha channel a colour factor is its alpha, as Vulkan reads it.
        Factor::SrcColor if alpha => D3D12_BLEND_SRC_ALPHA,
        Factor::SrcColor => D3D12_BLEND_SRC_COLOR,
        Factor::DstColor if alpha => D3D12_BLEND_DEST_ALPHA,
        Factor::DstColor => D3D12_BLEND_DEST_COLOR,
    }
}

fn compare(c: Compare) -> D3D12_COMPARISON_FUNC {
    match c {
        Compare::Always => D3D12_COMPARISON_FUNC_ALWAYS,
        Compare::Less => D3D12_COMPARISON_FUNC_LESS,
        Compare::LessEqual => D3D12_COMPARISON_FUNC_LESS_EQUAL,
        Compare::Equal => D3D12_COMPARISON_FUNC_EQUAL,
        Compare::GreaterEqual => D3D12_COMPARISON_FUNC_GREATER_EQUAL,
    }
}

impl Device {
    fn insert_pipeline(&mut self, entry: PipelineEntry) -> Pipeline {
        self.stats.pipelines_built += 1;
        let id = u32::try_from(self.pipelines.len()).unwrap_or(u32::MAX);
        self.pipelines.push(Some(entry));
        Pipeline(id)
    }

    /// Make a graphics pipeline from DXIL. Viewport and scissor are set while drawing; vertices
    /// are pulled by the vertex shader.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for an entry point without DXIL; any failure of the runtime.
    pub fn create_graphics_pipeline(&mut self, desc: &GraphicsDesc<'_>) -> Result<Pipeline, Error> {
        let vs = bytecode(desc.vertex.dxil, desc.vertex.entry)?;
        let ps = match desc.fragment {
            Some(f) => bytecode(f.dxil, f.entry)?,
            None => D3D12_SHADER_BYTECODE::default(),
        };
        let mut targets = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
        let mut rtv_formats = [DXGI_FORMAT_UNKNOWN; 8];
        for (k, c) in desc.color.iter().enumerate().take(8) {
            let mut mask = D3D12_COLOR_WRITE_ENABLE_RED.0
                | D3D12_COLOR_WRITE_ENABLE_GREEN.0
                | D3D12_COLOR_WRITE_ENABLE_BLUE.0;
            if c.write_alpha {
                mask |= D3D12_COLOR_WRITE_ENABLE_ALPHA.0;
            }
            targets[k] = match c.blend {
                None => D3D12_RENDER_TARGET_BLEND_DESC {
                    RenderTargetWriteMask: u8::try_from(mask).unwrap_or(0x0F),
                    ..Default::default()
                },
                Some(b) => D3D12_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    LogicOpEnable: false.into(),
                    SrcBlend: blend(b.src, false),
                    DestBlend: blend(b.dst, false),
                    BlendOp: D3D12_BLEND_OP_ADD,
                    SrcBlendAlpha: blend(b.src, true),
                    DestBlendAlpha: blend(b.dst, true),
                    BlendOpAlpha: D3D12_BLEND_OP_ADD,
                    LogicOp: D3D12_LOGIC_OP_NOOP,
                    RenderTargetWriteMask: u8::try_from(mask).unwrap_or(0x0F),
                },
            };
            rtv_formats[k] = formats(c.format).1;
        }
        let depth = desc
            .depth
            .map_or_else(D3D12_DEPTH_STENCIL_DESC::default, |d| {
                D3D12_DEPTH_STENCIL_DESC {
                    DepthEnable: d.compare.is_some().into(),
                    DepthWriteMask: if d.write {
                        D3D12_DEPTH_WRITE_MASK_ALL
                    } else {
                        D3D12_DEPTH_WRITE_MASK_ZERO
                    },
                    DepthFunc: d.compare.map_or(D3D12_COMPARISON_FUNC_ALWAYS, compare),
                    ..Default::default()
                }
            });
        let pso = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // SAFETY: a non-owning copy of the device's root signature, which outlives the call.
            pRootSignature: unsafe { std::mem::transmute_copy(&self.root) },
            VS: vs,
            PS: ps,
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: true.into(),
                RenderTarget: targets,
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: match desc.cull {
                    Cull::None => D3D12_CULL_MODE_NONE,
                    Cull::Front => D3D12_CULL_MODE_FRONT,
                    Cull::Back => D3D12_CULL_MODE_BACK,
                },
                // Front faces are wound clockwise, as on Vulkan's negative-height viewport.
                FrontCounterClockwise: false.into(),
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            DepthStencilState: depth,
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: u32::try_from(desc.color.len().min(8)).unwrap_or(0),
            RTVFormats: rtv_formats,
            DSVFormat: desc
                .depth
                .map_or(DXGI_FORMAT_UNKNOWN, |d| formats(d.format).1),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        // SAFETY: `pso` and the bytecode it points at live for the call.
        let made: Result<ID3D12PipelineState, Error> = hr("CreateGraphicsPipelineState", unsafe {
            self.device.CreateGraphicsPipelineState(&pso)
        });
        let pipeline = made.map_err(|e| Error::D3d12 {
            what: "CreateGraphicsPipelineState",
            detail: format!("{}: {e}", desc.label),
        })?;
        Ok(self.insert_pipeline(PipelineEntry {
            pipeline,
            compute: false,
        }))
    }

    /// Make a compute pipeline from DXIL.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for an entry point without DXIL; any failure of the runtime.
    pub fn create_compute_pipeline(&mut self, desc: &ComputeDesc<'_>) -> Result<Pipeline, Error> {
        let cs = bytecode(desc.shader.dxil, desc.shader.entry)?;
        let pso = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            // SAFETY: a non-owning copy of the device's root signature, which outlives the call.
            pRootSignature: unsafe { std::mem::transmute_copy(&self.root) },
            CS: cs,
            ..Default::default()
        };
        // SAFETY: `pso` and the bytecode it points at live for the call.
        let made: Result<ID3D12PipelineState, Error> = hr("CreateComputePipelineState", unsafe {
            self.device.CreateComputePipelineState(&pso)
        });
        let pipeline = made.map_err(|e| Error::D3d12 {
            what: "CreateComputePipelineState",
            detail: format!("{}: {e}", desc.label),
        })?;
        Ok(self.insert_pipeline(PipelineEntry {
            pipeline,
            compute: true,
        }))
    }

    /// Destroy a pipeline once the work recorded so far is done with it.
    pub fn destroy_pipeline(&mut self, p: Pipeline) {
        if let Some(e) = self.pipelines.get_mut(p.0 as usize).and_then(Option::take) {
            self.defer(Garbage::Pipeline(e.pipeline));
        }
    }
}
