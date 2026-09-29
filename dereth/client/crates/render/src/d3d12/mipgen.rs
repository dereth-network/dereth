//! D3D12 implementation of the video-memory AUTOGEN path, not D3DX system-memory BOX.
//! The source requests flags 4, which translates to D3DUSAGE_AUTOGENMIPMAP. D3D9's default
//! autogen filter is LINEAR
//! (Microsoft, IDirect3DBaseTexture9::SetAutoGenFilterType). No CPU rounding or BC encoder is
//! inferred here. Each destination texel samples the preceding level at its normalized centre;
//! the device performs linear filtering and UNORM conversion. Cross-driver bytes are not promised.

use super::*;

#[derive(Debug)]
pub(super) struct MipGenerator {
    root: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
}

/// Per-upload views cannot be rewritten or dropped until the upload fence has retired.
pub(super) struct MipViews {
    _srv: ID3D12DescriptorHeap,
    _rtv: ID3D12DescriptorHeap,
}

const SOURCE: &[u8] = br"
Texture2D<float4> source : register(t0);
SamplerState linear_filter : register(s0);
struct V { float4 position : SV_Position; float2 uv : TEXCOORD; };
V vs(uint id : SV_VertexID) {
    V v;
    v.uv = float2((id << 1) & 2, id & 2);
    v.position = float4(v.uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return v;
}
float4 ps(V v) : SV_Target { return source.SampleLevel(linear_filter, v.uv, 0); }
";

/// D3D12 has no AUTOGEN bit. This backend requires the actual BGRA8 format to support the
/// operations used to implement it. Failure keeps the original provided-level resource.
pub(super) fn supported(device: &ID3D12Device) -> bool {
    let mut support = D3D12_FEATURE_DATA_FORMAT_SUPPORT {
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        ..Default::default()
    };
    // SAFETY: support points to a correctly sized writable descriptor for this feature.
    let queried = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_FORMAT_SUPPORT,
            std::ptr::addr_of_mut!(support).cast(),
            std::mem::size_of_val(&support) as u32,
        )
    }
    .is_ok();
    let required = D3D12_FORMAT_SUPPORT1_TEXTURE2D
        | D3D12_FORMAT_SUPPORT1_MIP
        | D3D12_FORMAT_SUPPORT1_SHADER_SAMPLE
        | D3D12_FORMAT_SUPPORT1_RENDER_TARGET;
    queried && support.Support1.contains(required)
}

impl MipGenerator {
    pub(super) fn new(device: &ID3D12Device) -> Result<Self, RenderError> {
        let range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: 0,
        };
        let param = D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                    NumDescriptorRanges: 1,
                    pDescriptorRanges: std::ptr::addr_of!(range),
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        };
        let sampler = D3D12_STATIC_SAMPLER_DESC {
            Filter: D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            AddressU: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressV: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D12_COMPARISON_FUNC_ALWAYS,
            BorderColor: D3D12_STATIC_BORDER_COLOR_TRANSPARENT_BLACK,
            MinLOD: 0.0,
            MaxLOD: 0.0,
            ShaderRegister: 0,
            RegisterSpace: 0,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        };
        let desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: 1,
            pParameters: std::ptr::addr_of!(param),
            NumStaticSamplers: 1,
            pStaticSamplers: std::ptr::addr_of!(sampler),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_NONE,
        };
        let mut blob = None;
        let mut errors = None;
        // SAFETY: all descriptor pointers refer to live locals and the output blobs are owned.
        hr("SerializeRootSignature(mips)", unsafe {
            D3D12SerializeRootSignature(
                &desc,
                D3D_ROOT_SIGNATURE_VERSION_1,
                &mut blob,
                Some(&mut errors),
            )
        })?;
        let blob = blob.ok_or_else(|| RenderError::Device("no mip root signature".into()))?;
        // SAFETY: blob owns these bytes throughout CreateRootSignature, which copies them.
        let root: ID3D12RootSignature = unsafe {
            let bytes = std::slice::from_raw_parts(
                blob.GetBufferPointer().cast::<u8>(),
                blob.GetBufferSize(),
            );
            hr(
                "CreateRootSignature(mips)",
                device.CreateRootSignature(0, bytes),
            )?
        };
        let vs = compile_source(SOURCE, &[], c"vs", c"vs_5_0")?;
        let ps = compile_source(SOURCE, &[], c"ps", c"ps_5_0")?;
        let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(root.clone())),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vs.as_ptr().cast(),
                BytecodeLength: vs.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: ps.as_ptr().cast(),
                BytecodeLength: ps.len(),
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: false.into(),
                DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ZERO,
                DepthFunc: D3D12_COMPARISON_FUNC_ALWAYS,
                StencilEnable: false.into(),
                FrontFace: DISABLED_STENCIL_FACE,
                BackFace: DISABLED_STENCIL_FACE,
                ..Default::default()
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        desc.RTVFormats[0] = DXGI_FORMAT_B8G8R8A8_UNORM;
        desc.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            SrcBlend: D3D12_BLEND_ONE,
            DestBlend: D3D12_BLEND_ZERO,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_ZERO,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        };
        // SAFETY: descriptor and shader bytecode outlive the call; release the descriptor's
        // explicitly cloned COM reference on both success and failure.
        let result = unsafe { device.CreateGraphicsPipelineState(&desc) };
        // SAFETY: the descriptor owns exactly one cloned COM reference; it is no longer read.
        unsafe {
            std::mem::ManuallyDrop::drop(&mut desc.pRootSignature);
        }
        Ok(Self {
            root,
            pipeline: hr("CreateGraphicsPipelineState(mips)", result)?,
        })
    }

    /// Record without touching the frame command list or its descriptor heaps. The resource
    /// starts entirely COPY_DEST, with level zero copied; all levels end PIXEL_SHADER_RESOURCE.
    pub(super) fn record(
        &self,
        device: &ID3D12Device,
        list: &ID3D12GraphicsCommandList,
        texture: &ID3D12Resource,
        width: u32,
        height: u32,
        levels: u16,
    ) -> Result<MipViews, RenderError> {
        let count = u32::from(levels - 1);
        let srv = Gpu::heap(device, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, count, true)?;
        let rtv = Gpu::heap(device, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, count, false)?;
        // SAFETY: fresh heaps have count descriptors. Every view references one valid mip;
        // barriers track individual subresources, so no mip is read and written simultaneously.
        // The caller keeps MipViews, texture and generator alive through the upload fence.
        unsafe {
            let srv_stride =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
            let rtv_stride =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
            list.SetGraphicsRootSignature(&self.root);
            list.SetPipelineState(&self.pipeline);
            list.SetDescriptorHeaps(&[Some(srv.clone())]);
            list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            transition_level(
                list,
                texture,
                0,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            );
            let (mut w, mut h) = (width, height);
            for level in 1..u32::from(levels) {
                let offset = (level - 1) as usize;
                let mut cpu_srv = srv.GetCPUDescriptorHandleForHeapStart();
                cpu_srv.ptr += offset * srv_stride as usize;
                let source = D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_SRV {
                            MostDetailedMip: level - 1,
                            MipLevels: 1,
                            PlaneSlice: 0,
                            ResourceMinLODClamp: 0.0,
                        },
                    },
                };
                device.CreateShaderResourceView(texture, Some(&source), cpu_srv);
                let mut dst = rtv.GetCPUDescriptorHandleForHeapStart();
                dst.ptr += offset * rtv_stride as usize;
                let dest = D3D12_RENDER_TARGET_VIEW_DESC {
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D12_RENDER_TARGET_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_RTV {
                            MipSlice: level,
                            PlaneSlice: 0,
                        },
                    },
                };
                device.CreateRenderTargetView(texture, Some(&dest), dst);
                transition_level(
                    list,
                    texture,
                    level,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                );
                w = crate::mip::half(w);
                h = crate::mip::half(h);
                list.RSSetViewports(&[D3D12_VIEWPORT {
                    Width: w as f32,
                    Height: h as f32,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                    ..Default::default()
                }]);
                list.RSSetScissorRects(&[RECT {
                    left: 0,
                    top: 0,
                    right: w as i32,
                    bottom: h as i32,
                }]);
                list.OMSetRenderTargets(1, Some(&dst), false, None);
                let mut source_gpu = srv.GetGPUDescriptorHandleForHeapStart();
                source_gpu.ptr += u64::from(level - 1) * u64::from(srv_stride);
                list.SetGraphicsRootDescriptorTable(0, source_gpu);
                list.DrawInstanced(3, 1, 0, 0);
                transition_level(
                    list,
                    texture,
                    level,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                );
            }
        }
        Ok(MipViews {
            _srv: srv,
            _rtv: rtv,
        })
    }
}

/// # Safety
/// List must be open; the named level must be in `from` and remain alive through execution.
unsafe fn transition_level(
    list: &ID3D12GraphicsCommandList,
    resource: &ID3D12Resource,
    level: u32,
    from: D3D12_RESOURCE_STATES,
    to: D3D12_RESOURCE_STATES,
) {
    let barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: std::mem::transmute_copy(resource),
                Subresource: level,
                StateBefore: from,
                StateAfter: to,
            }),
        },
    };
    list.ResourceBarrier(&[barrier]);
}

impl Gpu {
    /// Read an actual BGRA8 texture subresource for diagnostics, without rebuilding its pixels
    /// from source data. Uses its own command list and restores shader-read state. Does not add a
    /// cache link or expose/rewrite any persistent descriptor.
    ///
    /// # Errors
    /// An absent slot, non-BGRA resource, invalid level, or device/readback failure.
    pub fn capture_texture_level(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<CapturedImage, RenderError> {
        let texture = self
            .textures
            .get(&slot.0)
            .ok_or(RenderError::Unsupported("texture slot is not live"))?;
        // SAFETY: the map owns this live resource; preserve the BGRA-only API contract.
        if unsafe { texture.GetDesc() }.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(RenderError::Unsupported(
                "readback requires an existing BGRA8 mip",
            ));
        }
        let mut data = self.capture_texture_level_data(slot, level)?;
        Ok(CapturedImage {
            width: data.width,
            height: data.height,
            bgra: data.levels.remove(0),
        })
    }

    /// Copy one actual resident mip, preserving compressed block bytes when applicable.
    /// Uses the device's subresource footprint; no decode/re-encode or policy recomputation.
    /// # Errors
    /// An absent slot, unsupported format, invalid level, or device/readback failure.
    pub fn capture_texture_level_data(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<TextureData, RenderError> {
        let texture = self
            .textures
            .get(&slot.0)
            .cloned()
            .ok_or(RenderError::Unsupported("texture slot is not live"))?;
        // SAFETY: the local clone owns a live resource.
        let desc = unsafe { texture.GetDesc() };
        let format = match desc.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => TextureFormat::Bgra8,
            DXGI_FORMAT_BC1_UNORM => TextureFormat::Bc1,
            DXGI_FORMAT_BC2_UNORM => TextureFormat::Bc2,
            DXGI_FORMAT_BC3_UNORM => TextureFormat::Bc3,
            _ => {
                return Err(RenderError::Unsupported(
                    "unsupported texture readback format",
                ))
            }
        };
        if level >= desc.MipLevels {
            return Err(RenderError::Unsupported(
                "readback requires an existing mip",
            ));
        }
        let w = (desc.Width as u32 >> level).max(1);
        let h = (desc.Height >> level).max(1);
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut rows = 0u32;
        let mut row = 0u64;
        let mut total = 0u64;
        // SAFETY: desc is the actual live resource descriptor, level is in range, outputs live.
        unsafe {
            self.device.GetCopyableFootprints(
                &desc,
                u32::from(level),
                1,
                0,
                Some(&mut footprint),
                Some(&mut rows),
                Some(&mut row),
                Some(&mut total),
            );
        }
        let (row, total) = (row as usize, total as usize);
        let pitch = footprint.Footprint.RowPitch as usize;
        let buffer_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: total as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut readback = None;
        // SAFETY: descriptors and out pointer live through the call; buffer read only after fence.
        hr("CreateCommittedResource(texture readback)", unsafe {
            self.device.CreateCommittedResource(
                &heap_props(D3D12_HEAP_TYPE_READBACK),
                D3D12_HEAP_FLAG_NONE,
                &buffer_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback,
            )
        })?;
        let readback: ID3D12Resource =
            readback.ok_or_else(|| RenderError::Device("no texture readback buffer".into()))?;
        // SAFETY: fresh allocator/list; all resources stay owned until the queue fence retires.
        let alloc: ID3D12CommandAllocator =
            hr("CreateCommandAllocator(texture readback)", unsafe {
                self.device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            })?;
        // SAFETY: alloc stays live through this list's submission and fence completion.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList(texture readback)", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None)
        })?;
        // SAFETY: the texture's upload completes before publication; all resident subresources
        // are shader-readable. Transition only the requested mip, then restore it before return.
        unsafe {
            transition_level(
                &list,
                &texture,
                u32::from(level),
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            );
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&readback),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: footprint,
                },
            };
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&texture),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: u32::from(level),
                },
            };
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            transition_level(
                &list,
                &texture,
                u32::from(level),
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            );
            hr("Close(texture readback)", list.Close())?;
            self.queue
                .ExecuteCommandLists(&[Some(hr("cast(texture readback)", list.cast())?)]);
        }
        self.wait_idle()?;
        let mut bytes_out = vec![0; row * rows as usize];
        // SAFETY: the GPU has completed this buffer. Copy only initialized row bytes and unmap.
        unsafe {
            let mut ptr = std::ptr::null_mut();
            hr(
                "Map(texture readback)",
                readback.Map(0, None, Some(&mut ptr)),
            )?;
            let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), total);
            for y in 0..rows as usize {
                bytes_out[y * row..(y + 1) * row]
                    .copy_from_slice(&bytes[y * pitch..y * pitch + row]);
            }
            readback.Unmap(0, None);
        }
        Ok(TextureData {
            width: w,
            height: h,
            format,
            levels: vec![bytes_out],
        })
    }
}
