//! The remaster's seam on the Direct3D 12 device: the device's handles it adopts, and the world
//! hook, which draws the world into the frame where the world ends and the interface begins.
//!
//! **Off, it is not there.** With no hook installed, [`Gpu::world_end`] returns on its first test
//! and the frame is recorded as before. Direct3D 12 needs nothing asked of the device when it is
//! made, so a device is always ready to take a hook when its adapter can draw the remaster.
//!
//! **With a hook installed**, [`Gpu::world_end`] hands the hook the frame's command list, its
//! colour and depth targets and the world viewport, then puts back what the interface's drawing
//! relies on: the targets, the root signature and its tables, the descriptor heaps and the
//! viewport.
//!
//! **A hook that fails** is uninstalled and the failure kept ([`Gpu::world_hook_failed`]); that
//! frame's world is cleared to black, and the next frame is the ordinary one.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;

use super::{Gpu, BACK_BUFFER_FORMAT, DEPTH_FORMAT, FRAME_COUNT};
use crate::camera::Viewport;
use crate::vulkan::world_hook::{WorldHook, WorldHookRefused};

/// What the remaster's seam keeps on the device.
#[derive(Default)]
pub(super) struct RemasterSeam {
    /// The adapter can draw the remaster.
    pub granted: bool,
    /// The remaster may trace rays, where the adapter can.
    pub ray_queries: bool,
    hook: Option<Box<dyn WorldHook>>,
    failed: Option<String>,
    /// The world of the open frame has ended.
    ended: bool,
}

impl std::fmt::Debug for RemasterSeam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemasterSeam")
            .field("granted", &self.granted)
            .field("hooked", &self.hook.is_some())
            .finish_non_exhaustive()
    }
}

impl RemasterSeam {
    /// The seam on `device`: granted when the remaster is `allowed` on its adapter, and the adapter
    /// binds resources at tier 3 and runs shader model 6.6, which the remaster's shaders index
    /// every resource with.
    pub(super) fn new(device: &ID3D12Device, allowed: bool, ray_queries: bool) -> Self {
        Self {
            granted: allowed && remaster_capable(device),
            ray_queries,
            ..Self::default()
        }
    }
}

/// Whether `device` can draw the remaster.
fn remaster_capable(device: &ID3D12Device) -> bool {
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
    // SAFETY: a pure query into a live local of the size given.
    let tier = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS,
            std::ptr::from_mut(&mut options).cast(),
            std::mem::size_of_val(&options) as u32,
        )
    }
    .is_ok()
        && options.ResourceBindingTier.0 >= D3D12_RESOURCE_BINDING_TIER_3.0;
    let mut model = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL(0x66),
    };
    // SAFETY: as above.
    let shaders = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_SHADER_MODEL,
            std::ptr::from_mut(&mut model).cast(),
            std::mem::size_of_val(&model) as u32,
        )
    }
    .is_ok()
        && model.HighestShaderModel.0 >= 0x66;
    tier && shaders
}

/// The device's handles, for a renderer that adopts the device. Whatever is made from them
/// must be gone before the device is dropped.
#[derive(Clone)]
pub struct RemasterDevice {
    /// The device.
    pub device: ID3D12Device,
    /// The queue the device submits to; anything submitted to it is submitted from the thread
    /// that draws the frames.
    pub queue: ID3D12CommandQueue,
    /// The formats of the frame's colour and depth targets, which the hook draws into.
    pub color_format: DXGI_FORMAT,
    pub depth_format: DXGI_FORMAT,
    /// Whether the remaster may trace rays, where the adapter can.
    pub ray_queries: bool,
}

impl std::fmt::Debug for RemasterDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemasterDevice")
            .field("color_format", &self.color_format)
            .field("depth_format", &self.depth_format)
            .finish_non_exhaustive()
    }
}

/// One of the frame's targets, as the hook is handed it.
#[derive(Debug, Clone)]
pub struct FrameImage {
    /// The resource.
    pub resource: ID3D12Resource,
    /// Its format.
    pub format: DXGI_FORMAT,
    /// Its extent.
    pub width: u32,
    /// Its extent.
    pub height: u32,
    /// The state it is in when the hook is called.
    pub state: D3D12_RESOURCE_STATES,
    /// The state the hook leaves it in, failing or not.
    pub leave_in: D3D12_RESOURCE_STATES,
}

/// The frame, where its world ends.
pub struct WorldFrame<'a> {
    /// The frame's command list, open.
    pub list: &'a ID3D12GraphicsCommandList,
    /// The frame's colour target.
    pub color: FrameImage,
    /// The frame's depth target.
    pub depth: FrameImage,
    /// The world's rectangle on the frame.
    pub viewport: Viewport,
    /// The screen-brightness value the device applies to every draw.
    pub gamma: f32,
    /// The frame's stamp.
    pub stamp: u64,
    /// How many frames are in flight at most: every frame up to `stamp - in_flight` is done.
    pub in_flight: u64,
}

impl std::fmt::Debug for WorldFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldFrame")
            .field("color", &self.color)
            .field("depth", &self.depth)
            .field("viewport", &self.viewport)
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl Gpu {
    /// Direct3D 12 asks nothing of the device for the remaster when it is made, so every device
    /// was asked.
    #[must_use]
    pub fn remaster_requested(&self) -> bool {
        true
    }

    /// Whether the adapter can draw the remaster: only then does the device take a world hook.
    #[must_use]
    pub fn remaster_supported(&self) -> bool {
        self.remaster.granted
    }

    /// The device's handles, for the remaster to adopt; `None` when the adapter cannot draw it.
    #[must_use]
    pub fn remaster_device(&self) -> Option<RemasterDevice> {
        self.remaster.granted.then(|| RemasterDevice {
            device: self.device.clone(),
            queue: self.queue.clone(),
            color_format: BACK_BUFFER_FORMAT,
            depth_format: DEPTH_FORMAT,
            ray_queries: self.remaster.ray_queries,
        })
    }

    /// Install `hook`, which draws the world from the next frame on. It is dropped before the
    /// device.
    ///
    /// # Errors
    /// [`WorldHookRefused`] on an adapter that cannot draw the remaster, or a device with a hook
    /// already.
    pub fn install_world_hook(&mut self, hook: Box<dyn WorldHook>) -> Result<(), WorldHookRefused> {
        if !self.remaster.granted {
            return Err(WorldHookRefused::Unsupported);
        }
        if self.remaster.hook.is_some() {
            return Err(WorldHookRefused::AlreadyInstalled);
        }
        self.remaster.failed = None;
        // A hook installed inside a frame waits for the next one.
        self.remaster.ended = self.frame_open;
        self.remaster.hook = Some(hook);
        Ok(())
    }

    /// Uninstall the hook and hand it back; the device draws its ordinary frame again. It must
    /// be dropped before the device.
    pub fn take_world_hook(&mut self) -> Option<Box<dyn WorldHook>> {
        self.remaster.hook.take()
    }

    /// The installed hook.
    pub fn world_hook_mut(&mut self) -> Option<&mut dyn WorldHook> {
        self.remaster.hook.as_deref_mut()
    }

    /// Why the last hook was uninstalled, if one failed.
    #[must_use]
    pub fn world_hook_failed(&self) -> Option<String> {
        self.remaster.failed.clone()
    }

    /// A frame has begun: its world has not ended.
    pub(super) fn world_begins(&mut self) {
        self.remaster.ended = false;
    }

    /// The world of the open frame has been drawn: with a hook installed, let it draw the world
    /// into the frame's targets, then put back what the interface's drawing relies on. With
    /// none, nothing.
    pub fn world_end(&mut self, viewport: Viewport) {
        if self.remaster.hook.is_none() || !self.frame_open || self.remaster.ended {
            return;
        }
        self.remaster.ended = true;
        let (rtv, back) = self.current_rtv();
        let (w, h) = (self.config.width, self.config.height);
        let frame = WorldFrame {
            list: &self.list,
            color: FrameImage {
                resource: back,
                format: BACK_BUFFER_FORMAT,
                width: w,
                height: h,
                state: D3D12_RESOURCE_STATE_RENDER_TARGET,
                leave_in: D3D12_RESOURCE_STATE_RENDER_TARGET,
            },
            depth: FrameImage {
                resource: self.depth.clone(),
                format: DEPTH_FORMAT,
                width: w,
                height: h,
                state: D3D12_RESOURCE_STATE_DEPTH_WRITE,
                leave_in: D3D12_RESOURCE_STATE_DEPTH_WRITE,
            },
            viewport,
            gamma: self.gamma,
            stamp: self.frame_stamp,
            in_flight: FRAME_COUNT as u64,
        };
        let recorded = match self.remaster.hook.as_mut() {
            Some(hook) => hook.record_d3d12(&frame),
            None => Ok(()),
        };
        drop(frame);
        if let Err(e) = &recorded {
            tracing::warn!(target: "dereth::render", "the world hook failed and is removed: {e}");
            self.remaster.failed = Some(e.0.clone());
            self.remaster.hook = None;
        }
        // SAFETY: the list is open; every handle and resource is the device's own and alive, and
        // the targets are in the states the hook leaves them in.
        unsafe {
            // SAFETY: a pure query.
            let dsv = self.dsv_heap.GetCPUDescriptorHandleForHeapStart();
            self.list
                .OMSetRenderTargets(1, Some(&rtv), false, Some(&dsv));
            self.list.SetDescriptorHeaps(&[
                Some(self.srv_heap.clone()),
                Some(self.sampler_heap.clone()),
            ]);
            self.list.SetGraphicsRootSignature(&self.root_signature);
            for (i, t) in self.legacy_tables.get().iter().enumerate() {
                if let Some(ptr) = t {
                    self.list.SetGraphicsRootDescriptorTable(
                        2 + i as u32,
                        D3D12_GPU_DESCRIPTOR_HANDLE { ptr: *ptr },
                    );
                }
            }
            self.list
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }
        if recorded.is_err() {
            self.clear_world(viewport, rtv);
        }
        // The viewport the frame had, which the hook's own drawing replaced.
        self.set_viewport(viewport);
    }

    /// The world's rectangle cleared to black, its depth to the far plane.
    fn clear_world(&self, v: Viewport, rtv: D3D12_CPU_DESCRIPTOR_HANDLE) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        let rect = RECT {
            left: v.x as i32,
            top: v.y as i32,
            right: (v.x + v.width) as i32,
            bottom: (v.y + v.height) as i32,
        };
        // SAFETY: the list is open, the targets are in their target states, and the rectangle is
        // inside them.
        unsafe {
            let dsv = self.dsv_heap.GetCPUDescriptorHandleForHeapStart();
            self.list
                .ClearRenderTargetView(rtv, &[0.0, 0.0, 0.0, 1.0], Some(&[rect]));
            self.list
                .ClearDepthStencilView(dsv, D3D12_CLEAR_FLAG_DEPTH, 1.0, 0, Some(&[rect]));
        }
    }

    /// Drop the hook. Called by the device's own drop, once the queue is idle.
    pub(super) fn destroy_world_hook(&mut self) {
        drop(self.remaster.hook.take());
    }
}

#[cfg(feature = "test-support")]
impl Gpu {
    /// Test builds: the last frame's depth, one value in [0, 1] a pixel, row by row.
    ///
    /// # Errors
    /// Any failure from the runtime.
    pub fn capture_depth(&mut self) -> Result<Vec<f32>, crate::RenderError> {
        use windows::core::Interface;
        use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};

        use super::hr;
        let (w, h) = (self.config.width, self.config.height);
        // SAFETY: a pure query on a live resource.
        let desc = unsafe { self.depth.GetDesc() };
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut total = 0u64;
        // SAFETY: a pure query into live locals; the depth plane is subresource 0.
        unsafe {
            self.device.GetCopyableFootprints(
                &desc,
                0,
                1,
                0,
                Some(&mut footprint),
                None,
                None,
                Some(&mut total),
            );
        }
        let buffer = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: total,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut readback: Option<ID3D12Resource> = None;
        // SAFETY: live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(depth readback)", unsafe {
            self.device.CreateCommittedResource(
                &super::heap_props(D3D12_HEAP_TYPE_READBACK),
                D3D12_HEAP_FLAG_NONE,
                &buffer,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback,
            )
        })?;
        let readback =
            readback.ok_or_else(|| crate::RenderError::Device("no readback buffer".into()))?;
        // SAFETY: a fresh allocator and list of this call's own, closed and executed before the
        // wait below, and dropped after it.
        let alloc: ID3D12CommandAllocator = hr("CreateCommandAllocator(depth readback)", unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        })?;
        // SAFETY: as above.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList(depth readback)", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None)
        })?;
        // SAFETY: the list is open; the depth target is in its write state between frames and
        // goes back to it; the copy is queued behind the frames that drew it, on the same queue.
        unsafe {
            Self::transition_on(
                &list,
                &self.depth,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
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
                pResource: std::mem::transmute_copy(&self.depth),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            Self::transition_on(
                &list,
                &self.depth,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
            );
            hr("Close(depth readback)", list.Close())?;
            let lists =
                [Some(list.cast::<ID3D12CommandList>().map_err(|e| {
                    crate::RenderError::Device(format!("cast: {e}"))
                })?)];
            self.queue.ExecuteCommandLists(&lists);
        }
        self.wait_idle()?;
        drop(list);
        drop(alloc);
        let pitch = footprint.Footprint.RowPitch as usize;
        let mut depth = Vec::with_capacity(w as usize * h as usize);
        // SAFETY: the buffer holds `total` bytes the queue has finished writing; mapped and
        // unmapped inside this block.
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            hr("Map", readback.Map(0, None, Some(&mut ptr)))?;
            let src = std::slice::from_raw_parts(ptr.cast::<u8>(), total as usize);
            for y in 0..h as usize {
                let row = &src[y * pitch..y * pitch + w as usize * 4];
                depth.extend(row.as_chunks::<4>().0.iter().map(|b| {
                    let v = u32::from_le_bytes(*b) & 0x00FF_FFFF;
                    #[allow(clippy::cast_precision_loss)] // a 24-bit value
                    let d = v as f32 / 16_777_215.0;
                    d
                }));
            }
            readback.Unmap(0, None);
        }
        Ok(depth)
    }
}
