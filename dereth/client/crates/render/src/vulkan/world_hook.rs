//! The remaster's seam on the Vulkan device: the features it asks for, the device's handles it
//! adopts, and the world hook, which draws the world into the frame where the world ends and
//! the interface begins.
//!
//! **Off, it is not there.** With [`crate::DeviceConfig::remaster`] off the device asks for
//! exactly what it asks for without the seam; with no hook installed, [`Gpu::world_end`] returns
//! on its first test and the frame is one render pass, as before.
//!
//! **With a hook installed** the frame's render pass keeps its depth when it ends, and
//! [`Gpu::world_end`] ends it, hands the hook the frame's command buffer, its colour and depth
//! images and the world viewport, then begins a second pass that loads both and draws the
//! interface over what the hook left. The two passes are compatible with the one every
//! pipeline was built for, so the interface draws exactly as it would have.
//!
//! **A hook that fails** is uninstalled and the failure kept ([`Gpu::world_hook_failed`]); that
//! frame's world is cleared to black, and the next frame is the ordinary one.

use std::any::Any;

use ash::vk;

use super::{vkr, Gpu, FRAME_COUNT};
use crate::camera::Viewport;
use crate::RenderError;

/// What the remaster's seam keeps on the device.
#[derive(Default)]
pub(super) struct RemasterSeam {
    /// The device was asked for the remaster's features.
    pub requested: bool,
    /// It has them all, and they are on.
    pub granted: bool,
    /// The queue's family.
    pub queue_family: u32,
    /// It was made with what ray queries need.
    pub ray_queries: bool,
    hook: Option<Box<dyn WorldHook>>,
    failed: Option<String>,
    /// The frame's pass that keeps its depth, and the pass that resumes the frame.
    passes: Option<(vk::RenderPass, vk::RenderPass)>,
    /// The open frame began with the pass that keeps its depth.
    kept: bool,
    /// The world of the open frame has ended.
    ended: bool,
}

impl RemasterSeam {
    pub(super) fn new(
        requested: bool,
        granted: bool,
        queue_family: u32,
        ray_queries: bool,
    ) -> Self {
        Self {
            requested,
            granted,
            queue_family,
            ray_queries,
            ..Self::default()
        }
    }
}

impl std::fmt::Debug for RemasterSeam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemasterSeam")
            .field("requested", &self.requested)
            .field("granted", &self.granted)
            .field("hooked", &self.hook.is_some())
            .finish_non_exhaustive()
    }
}

/// The device's handles, for a renderer that adopts the device. They live as long as the
/// device; anything made from them must be gone before it is dropped.
#[derive(Clone)]
pub struct RemasterDevice {
    /// The loader.
    pub entry: ash::Entry,
    /// The instance, made at Vulkan 1.3.
    pub instance: ash::Instance,
    /// The physical device.
    pub physical_device: vk::PhysicalDevice,
    /// The device, made with every feature the remaster draws with.
    pub device: ash::Device,
    /// The queue the device submits to; anything submitted to it is submitted from the thread
    /// that draws the frames.
    pub queue: vk::Queue,
    /// The queue's family.
    pub queue_family: u32,
    /// The formats of the frame's colour and depth images, which the hook draws into.
    pub color_format: vk::Format,
    pub depth_format: vk::Format,
    /// Whether the device was made with what ray queries need ([`RAY_QUERY_EXTENSIONS`],
    /// `bufferDeviceAddress`, `accelerationStructure` and `rayQuery`).
    pub ray_queries: bool,
}

impl std::fmt::Debug for RemasterDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemasterDevice")
            .field("queue_family", &self.queue_family)
            .finish_non_exhaustive()
    }
}

/// One of the frame's images, as the hook is handed it.
#[derive(Debug, Clone, Copy)]
pub struct FrameImage {
    /// The image.
    pub image: vk::Image,
    /// Its view of every aspect.
    pub view: vk::ImageView,
    /// Its format.
    pub format: vk::Format,
    /// Its extent.
    pub width: u32,
    /// Its extent.
    pub height: u32,
    /// The layout it is in when the hook is called.
    pub layout: vk::ImageLayout,
    /// The stages and accesses the frame's own work last touched it with.
    pub stages: vk::PipelineStageFlags2,
    /// As above.
    pub access: vk::AccessFlags2,
    /// The layout the hook leaves it in, its writes made visible to the interface's drawing.
    pub leave_in: vk::ImageLayout,
}

/// The frame, where its world ends.
pub struct WorldFrame<'a> {
    /// The device.
    pub device: &'a ash::Device,
    /// The frame's command buffer, recording, outside any render pass.
    pub cmd: vk::CommandBuffer,
    /// The frame's colour image.
    pub color: FrameImage,
    /// The frame's depth image.
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

/// A hook's failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct WorldHookError(pub String);

/// Why a hook was not installed.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorldHookRefused {
    /// The device is neither the Vulkan one nor the Direct3D 12 one.
    #[error("the remaster needs the Vulkan or Direct3D 12 renderer")]
    NotVulkan,
    /// The device was made without the remaster's request.
    #[error("the device was made without the remaster's features")]
    NotRequested,
    /// The device lacks what the remaster draws with.
    #[error("this graphics card cannot draw the remaster")]
    Unsupported,
    /// A hook is installed already.
    #[error("a world hook is installed already")]
    AlreadyInstalled,
}

/// What is told when a device closes, by the device's handle.
static CLOSING: std::sync::Mutex<Vec<fn(u64)>> = std::sync::Mutex::new(Vec::new());

/// Tell `f` when a device closes: idle, with its hook and everything on it still alive. For what
/// outlives any one hook, and must stop before the device goes.
pub fn when_a_device_closes(f: fn(u64)) {
    if let Ok(mut told) = CLOSING.lock() {
        if !told.contains(&f) {
            told.push(f);
        }
    }
}

/// `device`, idle, is closing.
pub(crate) fn device_closing(device: u64) {
    let told = CLOSING.lock().map(|t| t.clone()).unwrap_or_default();
    for f in told {
        f(device);
    }
}

/// What draws the world into the frame where the world ends.
pub trait WorldHook: Any {
    /// Record into `frame.cmd` the world, into `frame`'s images within its viewport, and leave
    /// each image in its `leave_in` layout with its writes visible to colour output and the
    /// depth tests.
    ///
    /// # Errors
    /// A failure uninstalls the hook.
    fn record(&mut self, frame: &WorldFrame<'_>) -> Result<(), WorldHookError>;

    /// The same on the Direct3D 12 device: record into `frame.list` the world, into `frame`'s
    /// targets within its viewport, and leave each target in its `leave_in` state, failing or
    /// not. A hook that draws only on Vulkan keeps this, which refuses.
    ///
    /// # Errors
    /// A failure uninstalls the hook.
    #[cfg(all(windows, feature = "d3d12"))]
    fn record_d3d12(
        &mut self,
        frame: &crate::d3d12::world_hook::WorldFrame<'_>,
    ) -> Result<(), WorldHookError> {
        let _ = frame;
        Err(WorldHookError("this hook draws on Vulkan only".to_owned()))
    }

    /// For the owner that installed it, to reach its own type.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// The device extensions ray queries need, which the device asks for with the remaster's
/// features when it has them all.
pub(super) const RAY_QUERY_EXTENSIONS: [&std::ffi::CStr; 3] = [
    ash::khr::acceleration_structure::NAME,
    ash::khr::ray_query::NAME,
    ash::khr::deferred_host_operations::NAME,
];

/// Whether `pd` has everything ray queries need: their extensions, and the device-address,
/// acceleration-structure (updated after binding) and ray-query features.
pub(super) fn ray_query_support(instance: &ash::Instance, pd: vk::PhysicalDevice) -> bool {
    // SAFETY: a pure query on a live physical device.
    let Ok(available) = (unsafe { instance.enumerate_device_extension_properties(pd) }) else {
        return false;
    };
    let has = |name: &std::ffi::CStr| {
        available
            .iter()
            .any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == name))
    };
    if !RAY_QUERY_EXTENSIONS.iter().all(|n| has(n)) {
        return false;
    }
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
    let mut query = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut f2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut accel)
        .push_next(&mut query)
        .push_next(&mut v12);
    // SAFETY: the chain is live locals for the call.
    unsafe { instance.get_physical_device_features2(pd, &mut f2) };
    accel.acceleration_structure == vk::TRUE
        && accel.descriptor_binding_acceleration_structure_update_after_bind == vk::TRUE
        && query.ray_query == vk::TRUE
        && v12.buffer_device_address == vk::TRUE
}

/// Test builds: whether the next devices made take the card as lacking the remaster's features,
/// as an older card does ([`lack_remaster_features_for_tests`]).
#[cfg(feature = "test-support")]
static LACKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Test builds: the devices made from now on take the card as lacking the remaster's features
/// (`true`), or as it is (`false`), so a test can make on any card the device an older one makes.
#[cfg(feature = "test-support")]
pub fn lack_remaster_features_for_tests(lacking: bool) {
    LACKING.store(lacking, std::sync::atomic::Ordering::SeqCst);
}

/// The features the remaster draws with that this device asks for when they are all there,
/// filled into `core`, `v11`, `v12` and `v13`; whether they were.
pub(super) fn remaster_features(
    instance: &ash::Instance,
    pd: vk::PhysicalDevice,
    core: &mut vk::PhysicalDeviceFeatures,
    v11: &mut vk::PhysicalDeviceVulkan11Features<'_>,
    v12: &mut vk::PhysicalDeviceVulkan12Features<'_>,
    v13: &mut vk::PhysicalDeviceVulkan13Features<'_>,
) -> bool {
    #[cfg(feature = "test-support")]
    if LACKING.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    // SAFETY: a pure query on a live physical device.
    let api = unsafe { instance.get_physical_device_properties(pd) }.api_version;
    if api < vk::API_VERSION_1_3 {
        return false;
    }
    let mut h11 = vk::PhysicalDeviceVulkan11Features::default();
    let mut h12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut h13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut f2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut h11)
        .push_next(&mut h12)
        .push_next(&mut h13);
    // SAFETY: the chain is live locals for the call.
    unsafe { instance.get_physical_device_features2(pd, &mut f2) };
    let hc = f2.features;
    let t = |b: vk::Bool32| b == vk::TRUE;
    let all = [
        t(hc.multi_draw_indirect),
        t(hc.draw_indirect_first_instance),
        t(h11.shader_draw_parameters),
        t(h12.descriptor_indexing),
        t(h12.runtime_descriptor_array),
        t(h12.descriptor_binding_partially_bound),
        t(h12.descriptor_binding_update_unused_while_pending),
        t(h12.descriptor_binding_sampled_image_update_after_bind),
        t(h12.descriptor_binding_storage_buffer_update_after_bind),
        t(h12.shader_sampled_image_array_non_uniform_indexing),
        t(h12.shader_storage_buffer_array_non_uniform_indexing),
        t(h12.scalar_block_layout),
        t(h13.dynamic_rendering),
        t(h13.synchronization2),
        t(h13.maintenance4),
        t(h13.shader_demote_to_helper_invocation),
    ];
    if !all.into_iter().all(|b| b) {
        return false;
    }
    core.multi_draw_indirect = vk::TRUE;
    core.draw_indirect_first_instance = vk::TRUE;
    v11.shader_draw_parameters = vk::TRUE;
    v12.descriptor_indexing = vk::TRUE;
    v12.runtime_descriptor_array = vk::TRUE;
    v12.descriptor_binding_partially_bound = vk::TRUE;
    v12.descriptor_binding_update_unused_while_pending = vk::TRUE;
    v12.descriptor_binding_sampled_image_update_after_bind = vk::TRUE;
    v12.descriptor_binding_storage_buffer_update_after_bind = vk::TRUE;
    v12.shader_sampled_image_array_non_uniform_indexing = vk::TRUE;
    v12.shader_storage_buffer_array_non_uniform_indexing = vk::TRUE;
    v12.draw_indirect_count = h12.draw_indirect_count;
    v12.scalar_block_layout = vk::TRUE;
    v13.dynamic_rendering = vk::TRUE;
    v13.synchronization2 = vk::TRUE;
    v13.maintenance4 = vk::TRUE;
    v13.shader_demote_to_helper_invocation = vk::TRUE;
    // Half floats and short integers where the device has them: the upscaler's pictures for an
    // outside scaler are written with them.
    core.shader_int16 = hc.shader_int16;
    v12.shader_float16 = h12.shader_float16;
    true
}

/// The frame's colour and depth attachments, loaded or cleared, kept, ending in `final`.
fn pass(
    device: &ash::Device,
    color: vk::Format,
    depth: vk::Format,
    resume: bool,
) -> Result<vk::RenderPass, RenderError> {
    let (load, color_in, depth_in) = if resume {
        (
            vk::AttachmentLoadOp::LOAD,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        )
    } else {
        (
            vk::AttachmentLoadOp::CLEAR,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::UNDEFINED,
        )
    };
    let attachments = [
        vk::AttachmentDescription::default()
            .format(color)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(load)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(color_in)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
        vk::AttachmentDescription::default()
            .format(depth)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(load)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(load)
            .stencil_store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(depth_in)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
    ];
    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let subpass = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_ref)
        .depth_stencil_attachment(&depth_ref)];
    // Exactly the frame pass's dependencies: two passes are compatible only when they differ in
    // nothing but their load and store operations and their layouts. What the resumed pass loads
    // is made visible to it by the hook's own barriers.
    let dependencies = Gpu::frame_dependencies();
    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpass)
        .dependencies(&dependencies);
    // SAFETY: live locals; destroyed with the device.
    vkr("vkCreateRenderPass(world hook)", unsafe {
        device.create_render_pass(&info, None)
    })
}

impl Gpu {
    /// Whether the device was asked for the remaster's features.
    #[must_use]
    pub fn remaster_requested(&self) -> bool {
        self.remaster.requested
    }

    /// Whether it has them and they are on: only then does it take a world hook.
    #[must_use]
    pub fn remaster_supported(&self) -> bool {
        self.remaster.granted
    }

    /// The device's handles, for the remaster to adopt; `None` unless it has the remaster's
    /// features on.
    #[must_use]
    pub fn remaster_device(&self) -> Option<RemasterDevice> {
        self.remaster.granted.then(|| RemasterDevice {
            entry: self.entry.clone(),
            instance: self.instance.clone(),
            physical_device: self.physical_device,
            device: self.device.clone(),
            queue: self.queue,
            queue_family: self.remaster.queue_family,
            color_format: super::BACK_BUFFER_FORMAT,
            depth_format: self.depth_format,
            ray_queries: self.remaster.ray_queries,
        })
    }

    /// Install `hook`, which draws the world from the next frame on. It is dropped before the
    /// device.
    ///
    /// # Errors
    /// [`WorldHookRefused`] on a device made without the remaster's request, one without its
    /// features, or one with a hook already.
    pub fn install_world_hook(&mut self, hook: Box<dyn WorldHook>) -> Result<(), WorldHookRefused> {
        if !self.remaster.requested {
            return Err(WorldHookRefused::NotRequested);
        }
        if !self.remaster.granted {
            return Err(WorldHookRefused::Unsupported);
        }
        if self.remaster.hook.is_some() {
            return Err(WorldHookRefused::AlreadyInstalled);
        }
        if self.remaster.passes.is_none() {
            let made = pass(
                &self.device,
                super::BACK_BUFFER_FORMAT,
                self.depth_format,
                false,
            )
            .and_then(|kept| {
                pass(
                    &self.device,
                    super::BACK_BUFFER_FORMAT,
                    self.depth_format,
                    true,
                )
                .map(|resume| (kept, resume))
                .inspect_err(|_| {
                    // SAFETY: unreferenced.
                    unsafe { self.device.destroy_render_pass(kept, None) };
                })
            });
            match made {
                Ok(p) => self.remaster.passes = Some(p),
                Err(e) => {
                    tracing::warn!(target: "dereth::render", "the world hook's passes: {e}");
                    return Err(WorldHookRefused::Unsupported);
                }
            }
        }
        self.remaster.failed = None;
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

    /// The render pass a frame begins with: the ordinary one, or, with a hook installed, the
    /// same pass keeping its depth for the pass that resumes the frame.
    pub(super) fn frame_pass(&mut self) -> vk::RenderPass {
        self.remaster.ended = false;
        match (&self.remaster.hook, self.remaster.passes) {
            (Some(_), Some((kept, _))) => {
                self.remaster.kept = true;
                kept
            }
            _ => {
                self.remaster.kept = false;
                self.render_pass
            }
        }
    }

    /// The world of the open frame has been drawn: with a hook installed, end the frame's pass,
    /// let the hook draw the world into the frame's images, and resume the frame for the
    /// interface. With none, nothing.
    pub fn world_end(&mut self, viewport: Viewport) {
        let (Some(hook), Some((_, resume))) = (self.remaster.hook.as_mut(), self.remaster.passes)
        else {
            return;
        };
        // A hook installed after the frame began waits for the next one, whose pass keeps its
        // depth for it.
        if !self.frame_open || !self.remaster.kept || self.remaster.ended {
            return;
        }
        self.remaster.ended = true;
        let cmd = self.frames[self.frame_index].cmd;
        // SAFETY: the frame's pass is open (`begin_frame` began it with a hook installed).
        unsafe { self.device.cmd_end_render_pass(cmd) };
        let (w, h) = (self.config.width, self.config.height);
        let frame = WorldFrame {
            device: &self.device,
            cmd,
            color: FrameImage {
                image: self.target.color.image,
                view: self.target.color.view,
                format: super::BACK_BUFFER_FORMAT,
                width: w,
                height: h,
                layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                // The frame pass's end moves it to its final layout in the transfer stage.
                stages: vk::PipelineStageFlags2::from_raw(
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT.as_raw()
                        | vk::PipelineStageFlags2::ALL_TRANSFER.as_raw(),
                ),
                access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                leave_in: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            },
            depth: FrameImage {
                image: self.target.depth.image,
                view: self.target.depth.view,
                format: self.depth_format,
                width: w,
                height: h,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                stages: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                access: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                leave_in: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            },
            viewport,
            gamma: self.gamma,
            stamp: self.frame_stamp,
            in_flight: FRAME_COUNT as u64,
        };
        let recorded = hook.record(&frame);
        if let Err(e) = &recorded {
            tracing::warn!(target: "dereth::render", "the world hook failed and is removed: {e}");
            self.remaster.failed = Some(e.0.clone());
            self.remaster.hook = None;
            // What the hook left is unknown: start both images afresh in the layouts the
            // resumed pass takes them in.
            let barrier = |image, aspect, layout| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(layout)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
            };
            let barriers = [
                barrier(
                    self.target.color.image,
                    vk::ImageAspectFlags::COLOR,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                barrier(
                    self.target.depth.image,
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                ),
            ];
            // SAFETY: the buffer is recording outside any pass; both images are live.
            unsafe {
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
        }
        let info = vk::RenderPassBeginInfo::default()
            .render_pass(resume)
            .framebuffer(self.target.framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: vk::Extent2D {
                    width: w,
                    height: h,
                },
            });
        // SAFETY: the buffer is recording outside any pass; the framebuffer is compatible with
        // the resumed pass, which differs from the frame's only in its load operations and
        // layouts.
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &info, vk::SubpassContents::INLINE);
        }
        if recorded.is_err() {
            self.clear_world(viewport);
        }
        // The viewport the frame had, which a new pass does not keep: put back as it was, not
        // set again.
        self.install_viewport(crate::camera::clamp_viewport(
            viewport,
            self.config.width,
            self.config.height,
        ));
    }

    /// The world's rectangle cleared to black, its depth to the far plane.
    fn clear_world(&self, v: Viewport) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        let cmd = self.frames[self.frame_index].cmd;
        let attachments = [
            vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                color_attachment: 0,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0; 4] },
                },
            },
            vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                color_attachment: 0,
                clear_value: vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            },
        ];
        #[allow(clippy::cast_possible_wrap)] // a target's extent
        let rect = vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D {
                    x: v.x as i32,
                    y: v.y as i32,
                },
                extent: vk::Extent2D {
                    width: v.width,
                    height: v.height,
                },
            },
            base_array_layer: 0,
            layer_count: 1,
        };
        // SAFETY: the buffer is recording inside the resumed pass.
        unsafe {
            self.device
                .cmd_clear_attachments(cmd, &attachments, &[rect]);
        }
    }

    /// Drop the hook, then the passes made for it. Called by the device's own drop, before the
    /// device goes.
    pub(super) fn destroy_world_hook(&mut self) {
        drop(self.remaster.hook.take());
        if let Some((kept, resume)) = self.remaster.passes.take() {
            // SAFETY: the device is idle and nothing records with them any more.
            unsafe {
                self.device.destroy_render_pass(kept, None);
                self.device.destroy_render_pass(resume, None);
            }
        }
    }
}

#[cfg(feature = "test-support")]
impl Gpu {
    /// Test builds: the last frame's depth, one value in [0, 1] a pixel, row by row. Only a frame
    /// drawn with a world hook installed keeps its depth to the end; another frame's is
    /// undefined.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn capture_depth(&mut self) -> Result<Vec<f32>, RenderError> {
        let (w, h) = (self.config.width, self.config.height);
        let total = u64::from(w) * u64::from(h) * 4;
        let readback = self.create_buffer(
            total,
            vk::BufferUsageFlags::TRANSFER_DST,
            gpu_allocator::MemoryLocation::GpuToCpu,
            "depth capture",
        )?;
        let buffer = readback.buffer;
        let image = self.target.depth.image;
        let result = self.one_shot("depth capture", |device, cmd| {
            let range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            let barrier = |old, new, src, dst| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(src)
                    .dst_access_mask(dst)
                    .old_layout(old)
                    .new_layout(new)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(range)
            };
            // SAFETY: the buffer is recording; the depth image is in the frame pass's final layout
            // and goes back to it.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        vk::AccessFlags::TRANSFER_READ,
                    )],
                );
                let region = vk::BufferImageCopy::default()
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::DEPTH,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_extent(vk::Extent3D {
                        width: w,
                        height: h,
                        depth: 1,
                    });
                device.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer,
                    &[region],
                );
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                        vk::AccessFlags::TRANSFER_READ,
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )],
                );
            }
            Ok(())
        });
        let bytes = result.and_then(|()| readback.read(0, total as usize));
        self.destroy_buffer(readback);
        let unorm24 = self.depth_format == vk::Format::D24_UNORM_S8_UINT;
        bytes.map(|b| {
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|w| {
                    let v = u32::from_le_bytes(*w);
                    if unorm24 {
                        #[allow(clippy::cast_precision_loss)] // a 24-bit value
                        let d = (v & 0x00FF_FFFF) as f32 / 16_777_215.0;
                        d
                    } else {
                        f32::from_bits(v)
                    }
                })
                .collect()
        })
    }
}

/// Test builds: a hook that draws nothing. It hands the frame's images straight back in the
/// layouts the interface resumes in, with the frame's own writes visible to it, so a frame
/// drawn through it is the frame drawn without it, split in two passes.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct Untouched {
    /// How many frames it was handed.
    pub handed: u64,
}

#[cfg(feature = "test-support")]
impl WorldHook for Untouched {
    fn record(&mut self, frame: &WorldFrame<'_>) -> Result<(), WorldHookError> {
        self.handed += 1;
        let barrier = |img: &FrameImage, aspect, dst_access| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
                .dst_access_mask(dst_access)
                .old_layout(img.layout)
                .new_layout(img.leave_in)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(img.image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: aspect,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let barriers = [
            barrier(
                &frame.color,
                vk::ImageAspectFlags::COLOR,
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            ),
            barrier(
                &frame.depth,
                vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            ),
        ];
        // SAFETY: the frame's buffer is recording outside any pass; both images are live and in
        // the layouts the frame says.
        unsafe {
            frame.device.cmd_pipeline_barrier(
                frame.cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::TRANSFER
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
        Ok(())
    }

    /// On Direct3D 12 the targets are already in the states the interface draws them in.
    #[cfg(all(windows, feature = "d3d12"))]
    fn record_d3d12(
        &mut self,
        _frame: &crate::d3d12::world_hook::WorldFrame<'_>,
    ) -> Result<(), WorldHookError> {
        self.handed += 1;
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
