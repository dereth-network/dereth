//! One Vulkan device that wgpu (the retail device, the interface and the window) and the remaster's
//! layer both draw on, and the remaster's pictures lent to wgpu.
//!
//! The device is made here, with every feature the layer asks for (`dereth_gfx::REQUIRED`) and
//! every one wgpu does, as the version-wide feature structures: the device's own features, all but
//! protected memory and sparse resources, are turned on. wgpu takes it as a device it did not make
//! (`device_from_raw`), and the layer adopts it ([`dereth_gfx::Device::adopt`]). Both submit to the
//! one queue from the thread that draws, so the queue's order orders their work: the remaster's
//! frame is submitted before the frame wgpu composes from it, and its last barrier leaves the
//! picture ready to sample, the state wgpu is told it holds.
//!
//! This is the one module of the crate that calls `unsafe` entry points: making the device from
//! its raw handles, handing them to wgpu and the layer, and lending wgpu a picture the layer made.
//! Each says why it holds.

#![allow(unsafe_code)]

use std::ffi::CStr;

use dereth_gfx::vk;
use dereth_gfx::{Device, DeviceDesc, Native, Texture};
use wgpu::hal::api::Vulkan;

/// A device wgpu and the layer share.
#[derive(Debug)]
pub struct Shared {
    /// The layer's hold on the device. Dropped before the rest, while the device lives.
    pub gfx: Device,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Why the shared device could not be made.
#[derive(Debug)]
pub struct SharedError(pub String);

impl std::fmt::Display for SharedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no device for the remaster: {}", self.0)
    }
}

impl std::error::Error for SharedError {}

fn fail(e: impl std::fmt::Display) -> SharedError {
    SharedError(e.to_string())
}

/// The features `features`, the version-wide structures' and the device's own, all that the
/// device has but protected memory and sparse resources, with every one the layer requires.
struct Wanted {
    core: vk::PhysicalDeviceFeatures,
    v11: vk::PhysicalDeviceVulkan11Features<'static>,
    v12: vk::PhysicalDeviceVulkan12Features<'static>,
    v13: vk::PhysicalDeviceVulkan13Features<'static>,
}

impl Wanted {
    /// What `pd` has, but protected memory and sparse resources; the names of what the layer
    /// requires that it lacks.
    fn of(instance: &ash::Instance, pd: vk::PhysicalDevice) -> (Self, Vec<&'static str>) {
        let mut v11 = vk::PhysicalDeviceVulkan11Features::default();
        let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
        let core = {
            let mut f2 = vk::PhysicalDeviceFeatures2::default()
                .push_next(&mut v11)
                .push_next(&mut v12)
                .push_next(&mut v13);
            // SAFETY: the chain is live locals for the call, on a live physical device.
            unsafe { instance.get_physical_device_features2(pd, &mut f2) };
            f2.features
        };
        let mut w = Self {
            core,
            v11,
            v12,
            v13,
        };
        w.v11.p_next = std::ptr::null_mut();
        w.v12.p_next = std::ptr::null_mut();
        w.v13.p_next = std::ptr::null_mut();
        w.v11.protected_memory = vk::FALSE;
        w.core.sparse_binding = vk::FALSE;
        w.core.sparse_residency_buffer = vk::FALSE;
        w.core.sparse_residency_image2_d = vk::FALSE;
        w.core.sparse_residency_image3_d = vk::FALSE;
        w.core.sparse_residency2_samples = vk::FALSE;
        w.core.sparse_residency4_samples = vk::FALSE;
        w.core.sparse_residency8_samples = vk::FALSE;
        w.core.sparse_residency16_samples = vk::FALSE;
        w.core.sparse_residency_aliased = vk::FALSE;
        let t = |b: vk::Bool32| b == vk::TRUE;
        let have = [
            t(w.core.multi_draw_indirect),
            t(w.core.draw_indirect_first_instance),
            t(w.v11.shader_draw_parameters),
            t(w.v12.descriptor_indexing),
            t(w.v12.runtime_descriptor_array),
            t(w.v12.descriptor_binding_partially_bound),
            t(w.v12.descriptor_binding_update_unused_while_pending),
            t(w.v12.descriptor_binding_sampled_image_update_after_bind),
            t(w.v12.descriptor_binding_storage_buffer_update_after_bind),
            t(w.v12.shader_sampled_image_array_non_uniform_indexing),
            t(w.v12.shader_storage_buffer_array_non_uniform_indexing),
            t(w.v12.scalar_block_layout),
            t(w.v12.timeline_semaphore),
            t(w.v13.dynamic_rendering),
            t(w.v13.synchronization2),
            t(w.v13.maintenance4),
            t(w.v13.shader_demote_to_helper_invocation),
        ];
        let lacking = dereth_gfx::REQUIRED[1..]
            .iter()
            .zip(have)
            .filter(|(_, h)| !h)
            .map(|(n, _)| *n)
            .collect();
        (w, lacking)
    }
}

/// A device wgpu and the remaster's layer share, on `instance`'s hardware Vulkan adapter (one that
/// can present to `surface` when one is given), with `features` and `limits` for wgpu: the ones
/// the retail device asks for, so it draws on it as it draws on its own.
///
/// # Errors
/// When there is no hardware Vulkan 1.3 adapter with every feature the layer needs, or the device
/// cannot be made.
pub fn open(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    features: wgpu::Features,
    limits: &wgpu::Limits,
) -> Result<Shared, SharedError> {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        ..Default::default()
    }))
    .map_err(fail)?;
    let info = adapter.get_info();
    if info.backend != wgpu::Backend::Vulkan || info.device_type == wgpu::DeviceType::Cpu {
        return Err(fail(format!(
            "{} is not a hardware Vulkan adapter",
            info.name
        )));
    }
    // SAFETY: the adapter is wgpu's Vulkan adapter (checked above); the reference is used only
    // while the adapter lives, within this function.
    let hal = unsafe { adapter.as_hal::<Vulkan>() }.ok_or_else(|| fail("no Vulkan adapter"))?;
    let shared = hal.shared_instance();
    let entry = shared.entry().clone();
    let raw_instance = shared.raw_instance().clone();
    let pd = hal.raw_physical_device();
    // SAFETY: a pure query on a live physical device.
    let props = unsafe { raw_instance.get_physical_device_properties(pd) };
    if props.api_version < vk::API_VERSION_1_3 {
        return Err(fail(format!("{} has no Vulkan 1.3", info.name)));
    }
    let (mut wanted, lacking) = Wanted::of(&raw_instance, pd);
    if !lacking.is_empty() {
        return Err(fail(format!("{} lacks {}", info.name, lacking.join(", "))));
    }
    let mut extensions: Vec<&'static CStr> = hal.required_device_extensions(features);
    if !extensions.contains(&ash::khr::swapchain::NAME) {
        extensions.push(ash::khr::swapchain::NAME);
    }
    let names: Vec<*const std::ffi::c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
    // wgpu draws on the first queue of the first family; the layer records for the same queue.
    let family = 0;
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priorities)];
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .features(wanted.core)
        .push_next(&mut wanted.v11)
        .push_next(&mut wanted.v12)
        .push_next(&mut wanted.v13);
    let create = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&names)
        .push_next(&mut features2);
    // SAFETY: the create info and everything it points to are live locals; the physical device
    // is wgpu's adapter's, its features and extensions are ones it reports.
    let raw_device = unsafe { raw_instance.create_device(pd, &create, None) }.map_err(fail)?;
    // SAFETY: the device was made from this adapter, on family 0, with `extensions` (which hold
    // every one `required_device_extensions` named) and every feature the adapter has (a superset
    // of `physical_device_features`); wgpu takes ownership of it and destroys it last.
    let open = unsafe {
        hal.device_from_raw(
            raw_device.clone(),
            None,
            &extensions,
            features,
            limits,
            &wgpu::MemoryHints::Performance,
            family,
            0,
        )
    }
    .map_err(fail)?;
    drop(hal);
    // SAFETY: `open` was made from this adapter's Vulkan backend just above.
    let (device, queue) = unsafe {
        adapter.create_device_from_hal::<Vulkan>(
            open,
            &wgpu::DeviceDescriptor {
                label: Some("shared with the remaster"),
                required_features: features,
                required_limits: limits.clone(),
                ..Default::default()
            },
        )
    }
    .map_err(fail)?;
    // SAFETY: wgpu's Vulkan device, as made above; the reference is used only to read its queue.
    let raw_queue = unsafe { device.as_hal::<Vulkan>() }
        .map(|d| d.raw_queue())
        .ok_or_else(|| fail("no Vulkan device"))?;
    let native = Native {
        entry,
        instance: raw_instance,
        physical_device: pd,
        device: raw_device,
        queue: raw_queue,
        queue_family: family,
        ray_queries: false,
    };
    // SAFETY: the handles are the device made above with every feature the layer requires; the
    // layer is dropped before wgpu's device (`Shared` drops it first, and a holder of the layer
    // keeps a clone of wgpu's device), and it is driven from the thread wgpu submits from.
    let gfx = unsafe { Device::adopt(native, &DeviceDesc::default()) }.map_err(fail)?;
    Ok(Shared {
        gfx,
        adapter,
        device,
        queue,
    })
}

/// The layer's picture `t` (a color target the remaster draws, `width` x `height`, of the format
/// [`crate::render`] draws in) as a wgpu texture on `device`, which is the device the layer
/// shares: wgpu is told it is ready to sample, as the remaster leaves it after each frame, and
/// never destroys it.
///
/// # Errors
/// When the layer holds no such picture.
pub fn lend(
    gfx: &Device,
    device: &wgpu::Device,
    t: Texture,
    (width, height): (u32, u32),
) -> Result<wgpu::Texture, SharedError> {
    let raw = gfx
        .raw_texture(t)
        .map_err(fail)?
        .ok_or_else(|| fail("the picture is not a Vulkan image"))?;
    let desc = wgpu::TextureDescriptor {
        label: Some("the remaster's world"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: desc.label,
        size: desc.size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: desc.format,
        usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };
    // SAFETY: wgpu's device is the Vulkan device the layer adopted; the reference lives only for
    // the call.
    let hal_texture = unsafe {
        let d = device
            .as_hal::<Vulkan>()
            .ok_or_else(|| fail("no Vulkan device"))?;
        d.texture_from_raw(
            raw.0,
            &hal_desc,
            Some(Box::new(|| {})),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    // SAFETY: the image is the layer's live color target, made with the size, format and usage
    // `desc` names (sampled, a copy's source); it was drawn before wgpu first reads it, and each
    // frame leaves it ready to sample, the state wgpu is told; the layer outlives the texture.
    Ok(unsafe {
        device.create_texture_from_hal::<Vulkan>(hal_texture, &desc, wgpu::TextureUses::RESOURCE)
    })
}

/// The remaster drawing the world's picture on a device it shares with wgpu: each frame's picture
/// lent to wgpu, the same texture while its size holds.
#[derive(Debug)]
pub struct Painter {
    /// The picture last lent, its layer texture and size. Dropped first.
    lent: Option<(Texture, (u32, u32), wgpu::Texture)>,
    remaster: crate::Remaster,
    /// wgpu's device, kept alive while the layer's hold on it lasts.
    device: wgpu::Device,
}

impl Painter {
    /// The remaster on `shared`'s device; wgpu's device and queue go on to the retail device and
    /// the interface.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn new(
        shared: Shared,
    ) -> Result<(Self, wgpu::Adapter, wgpu::Device, wgpu::Queue), SharedError> {
        let Shared {
            gfx,
            adapter,
            device,
            queue,
        } = shared;
        let remaster = crate::Remaster::on(gfx).map_err(fail)?;
        Ok((
            Self {
                lent: None,
                remaster,
                device: device.clone(),
            },
            adapter,
            device,
            queue,
        ))
    }

    /// `frame` drawn at `width` x `height`, its picture as wgpu's texture, ready to sample once the
    /// work wgpu submits after this is run.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn paint(
        &mut self,
        frame: &auberean_draw::scene::Frame,
        (width, height): (u32, u32),
        load: &mut dyn FnMut(u32) -> Option<auberean_draw::picture::Image>,
    ) -> Result<wgpu::Texture, SharedError> {
        let t = self
            .remaster
            .draw(frame, width, height, load)
            .map_err(fail)?;
        match &self.lent {
            Some((was, size, tex)) if *was == t && *size == (width, height) => Ok(tex.clone()),
            _ => {
                let tex = lend(self.remaster.device(), &self.device, t, (width, height))?;
                self.lent = Some((t, (width, height), tex.clone()));
                Ok(tex)
            }
        }
    }

    /// The remaster.
    #[must_use]
    pub fn remaster(&self) -> &crate::Remaster {
        &self.remaster
    }
}
