//! One Vulkan device that wgpu (the retail device, the interface and the window) and the remaster's
//! layer both draw on, and the remaster's pictures lent to wgpu. On Windows the device may be a
//! Direct3D 12 one instead ([`open_d3d12`]): wgpu's own, which the layer adopts.
//!
//! The device is made here, with every feature the layer asks for (`dereth_gfx::REQUIRED`) and
//! every one wgpu does, as the version-wide feature structures: the device's own features, all but
//! protected memory and sparse resources, are turned on, and where the adapter has them the
//! extensions and features ray queries need, for the traced shadows and occlusion. wgpu takes it
//! as a device it did not make (`device_from_raw`), and the layer adopts it
//! ([`dereth_gfx::Device::adopt`]). Both submit to the one queue from the thread that draws, so
//! the queue's order orders their work: the remaster's frame is submitted before the frame wgpu
//! composes from it, and its last barrier leaves the picture ready to sample, the state wgpu is
//! told it holds.
//!
//! The Vulkan instance under them ([`instance`]) is wgpu's own, but on the Mac: there is no Vulkan
//! there, so the MoltenVK the program carries is loaded by its path and the instance made on it,
//! then handed to wgpu.
//!
//! This is the one module of the crate that calls `unsafe` entry points: making the instance and
//! the device from their raw handles, handing them to wgpu and the layer, and lending wgpu a
//! picture the layer made. Each says why it holds.

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

/// The Vulkan instance the shared device is made on, for wgpu: wgpu's own, with only its Vulkan
/// backend; on the Mac, one made on the MoltenVK the program carries.
///
/// # Errors
/// On the Mac, when no MoltenVK or Vulkan can be loaded, or the instance cannot be made.
pub fn instance() -> Result<wgpu::Instance, SharedError> {
    #[cfg(not(target_os = "macos"))]
    {
        Ok(wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
        }))
    }
    #[cfg(target_os = "macos")]
    {
        let entry = moltenvk()?;
        let flags = wgpu::InstanceFlags::from_build_config().with_env();
        let api = vk::API_VERSION_1_3;
        let extensions =
            wgpu::hal::vulkan::Instance::desired_extensions(&entry, api, flags).map_err(fail)?;
        let names: Vec<*const std::ffi::c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
        let app = vk::ApplicationInfo::default().api_version(api);
        // MoltenVK, a portability implementation, is listed only behind this flag.
        let create_flags = if extensions.contains(&ash::khr::portability_enumeration::NAME) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::empty()
        };
        let create = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&names)
            .flags(create_flags);
        // SAFETY: the instance is made from `entry`, with `extensions` (every one wgpu-hal asked
        // for, each one the entry has) and the version and flags it is handed to wgpu with below.
        let raw = unsafe { entry.create_instance(&create, None) }.map_err(fail)?;
        // SAFETY: `raw` was made from `entry` as above, `extensions` are the ones it was made
        // with, and wgpu-hal takes ownership of it (no drop callback).
        let hal = unsafe {
            wgpu::hal::vulkan::Instance::from_raw(
                entry,
                raw,
                api,
                0,
                None,
                extensions,
                flags,
                wgpu::MemoryBudgetThresholds::default(),
                false,
                None,
            )
        }
        .map_err(fail)?;
        // SAFETY: the instance is a Vulkan one, which this build of wgpu holds the backend for.
        Ok(unsafe { wgpu::Instance::from_hal::<Vulkan>(hal) })
    }
}

/// The MoltenVK library's file name, in a bundle and in an install alike.
#[cfg(target_os = "macos")]
const MOLTENVK_LIBRARY: &str = "libMoltenVK.dylib";

/// The Mac's Vulkan, which its `dlopen` has no path for: the MoltenVK the program carries (in the
/// application bundle's `Contents/Frameworks`, then beside the executable), then a Vulkan loader
/// where the plain `dlopen` finds one, then the SDK's and Homebrew's, then Homebrew's MoltenVK.
/// MoltenVK exports the Vulkan entry points itself, so it serves as the loader.
#[cfg(target_os = "macos")]
fn moltenvk() -> Result<ash::Entry, SharedError> {
    let mut bundled = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
    {
        if let Some(contents) = dir.parent() {
            bundled.push(contents.join("Frameworks").join(MOLTENVK_LIBRARY));
        }
        bundled.push(dir.join(MOLTENVK_LIBRARY));
    }
    let load = |p: &std::path::Path| {
        // SAFETY: the file is a Vulkan implementation's library, whose entry points ash loads.
        unsafe { ash::Entry::load_from(p) }.ok()
    };
    if let Some(e) = bundled.iter().find_map(|p| load(p)) {
        return Ok(e);
    }
    // SAFETY: as above, the system's Vulkan loader.
    if let Ok(e) = unsafe { ash::Entry::load() } {
        return Ok(e);
    }
    let mut installed = Vec::new();
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        installed.push(
            std::path::Path::new(&sdk)
                .join("lib")
                .join("libvulkan.dylib"),
        );
    }
    for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
        installed.push(std::path::Path::new(dir).join("libvulkan.dylib"));
    }
    for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
        installed.push(std::path::Path::new(dir).join(MOLTENVK_LIBRARY));
    }
    installed
        .iter()
        .find_map(|p| load(p))
        .ok_or_else(|| SharedError("no MoltenVK or Vulkan to make the remaster's device on".into()))
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
    // Ray queries, where the adapter has them, for the traced shadows and occlusion.
    let traces = dereth_gfx::rt::adapter_traces(&raw_instance, pd);
    if traces {
        wanted.v12.buffer_device_address = vk::TRUE;
        for e in dereth_gfx::rt::EXTENSIONS {
            if !extensions.contains(&e) {
                extensions.push(e);
            }
        }
    }
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
        .acceleration_structure(true)
        .descriptor_binding_acceleration_structure_update_after_bind(true);
    let mut query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
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
    if traces {
        features2 = features2.push_next(&mut accel).push_next(&mut query);
    }
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
        ray_queries: traces,
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

/// The device wgpu and the layer share on Direct3D 12: wgpu's own, made on its Direct3D 12 adapter
/// with `features` and `limits` (and, for a window, `surface`), adopted by the layer. The layer asks
/// the device to enable nothing: what it needs (shader model 6.6, the third resource binding tier)
/// the adapter has or hasn't, and adopting fails where it hasn't.
///
/// # Errors
/// When there is no such adapter, wgpu refuses the device, or the layer can't draw on it.
#[cfg(windows)]
pub fn open_d3d12(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    features: wgpu::Features,
    limits: &wgpu::Limits,
) -> Result<Shared, SharedError> {
    use wgpu::hal::api::Dx12;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: surface,
        ..Default::default()
    }))
    .map_err(fail)?;
    if adapter.get_info().backend != wgpu::Backend::Dx12 {
        return Err(fail("not a Direct3D 12 adapter"));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("shared with the remaster"),
        required_features: features,
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .map_err(fail)?;
    // SAFETY: wgpu's Direct3D 12 device, as made above; the reference is used only to clone its
    // device and its direct queue, which wgpu submits to.
    let (raw_device, raw_queue) = unsafe { device.as_hal::<Dx12>() }
        .map(|d| (d.raw_device().clone(), d.raw_queue().clone()))
        .ok_or_else(|| fail("no Direct3D 12 device"))?;
    // The layer traces rays where the device has inline ray queries; it asks nothing enabled.
    let native = dereth_gfx::d3d12::D3d12Native {
        device: raw_device,
        queue: raw_queue,
        ray_queries: true,
    };
    // SAFETY: the device and queue are wgpu's, live while wgpu's device is; the layer is dropped
    // before wgpu's device (`Shared` drops it first, and a holder of the layer keeps a clone of
    // wgpu's device), and it is driven from the thread wgpu submits from.
    let gfx = unsafe { Device::adopt_d3d12(native, &DeviceDesc::default()) }.map_err(fail)?;
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
    #[cfg(windows)]
    if gfx.backend() == dereth_gfx::Backend::D3d12 {
        return lend_d3d12(gfx, device, t, (width, height));
    }
    let raw = gfx
        .raw_texture(t)
        .map_err(fail)?
        .ok_or_else(|| fail("the picture is not a Vulkan image"))?;
    let desc = lent_desc(width, height);
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

/// [`lend`] on Direct3D 12: the layer's resource wrapped for wgpu, in the state both call ready to
/// sample (read by pixel and other shaders alike).
#[cfg(windows)]
fn lend_d3d12(
    gfx: &Device,
    device: &wgpu::Device,
    t: Texture,
    (width, height): (u32, u32),
) -> Result<wgpu::Texture, SharedError> {
    use wgpu::hal::api::Dx12;
    let resource = gfx
        .raw_d3d12_texture(t)
        .map_err(fail)?
        .ok_or_else(|| fail("the picture is not a Direct3D 12 resource"))?;
    let desc = lent_desc(width, height);
    // SAFETY: the resource is the layer's live color target, `width` x `height` of one level and
    // one sample, of the format `desc` names.
    let hal_texture = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            desc.format,
            wgpu::TextureDimension::D2,
            desc.size,
            1,
            1,
        )
    };
    // SAFETY: the resource was drawn before wgpu first reads it, and each frame leaves it ready to
    // sample, the state wgpu is told; the layer outlives the texture.
    Ok(unsafe {
        device.create_texture_from_hal::<Dx12>(hal_texture, &desc, wgpu::TextureUses::RESOURCE)
    })
}

/// How the lent picture is described to wgpu.
fn lent_desc(width: u32, height: u32) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
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
    }
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
        let mut remaster = crate::Remaster::on(gfx).map_err(fail)?;
        remaster.look();
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

    /// The remaster, to set how it draws.
    pub fn remaster_mut(&mut self) -> &mut crate::Remaster {
        &mut self.remaster
    }
}
