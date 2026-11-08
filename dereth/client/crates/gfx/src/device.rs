//! The device: made here or adopted, its tables, its clock and the destruction it defers.

use std::ffi::{c_char, c_void, CStr};

use ash::vk;
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};

use crate::bindless::Bindless;
use crate::pipeline::Pipelines;
use crate::query::Timers;
use crate::resource::{BufferEntry, SamplerEntry, Slots, Staging, TextureEntry};
use crate::upload::PendingCopy;
use crate::{Error, Stats};

/// What a device made here is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceDesc {
    /// Enable the Khronos validation layer when it is installed.
    pub debug: bool,
    /// Prefer a software rasterizer to the hardware.
    pub software: bool,
    /// How many textures the bindless table holds.
    pub bindless_textures: u32,
    /// How many storage buffers the bindless table holds.
    pub bindless_buffers: u32,
    /// The graphics API a device made here records with (an adopted device's is its own).
    pub backend: crate::Backend,
}

impl Default for DeviceDesc {
    fn default() -> Self {
        Self {
            debug: false,
            software: false,
            bindless_textures: 16_384,
            bindless_buffers: 4_096,
            backend: crate::Backend::Vulkan,
        }
    }
}

/// A device that already exists, to adopt: its loader, instance, physical device, device and
/// the queue the layer records for. The device must have been made with every feature
/// [`REQUIRED`] lists. The layer never destroys any of them.
#[derive(Clone)]
pub struct Native {
    /// The loader.
    pub entry: ash::Entry,
    /// The instance.
    pub instance: ash::Instance,
    /// The physical device.
    pub physical_device: vk::PhysicalDevice,
    /// The device.
    pub device: ash::Device,
    /// The queue.
    pub queue: vk::Queue,
    /// The queue's family.
    pub queue_family: u32,
    /// Whether the device was made with what ray queries need ([`crate::rt::EXTENSIONS`],
    /// `bufferDeviceAddress`, `accelerationStructure` and `rayQuery`): only then does the layer
    /// trace rays.
    pub ray_queries: bool,
}

impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Native")
            .field("queue_family", &self.queue_family)
            .finish_non_exhaustive()
    }
}

/// The features the layer draws with, by the Vulkan structure that holds each: what a device
/// made here enables, and what one adopted must have been made with.
pub const REQUIRED: &[&str] = &[
    "Vulkan 1.3",
    "VkPhysicalDeviceFeatures.multiDrawIndirect",
    "VkPhysicalDeviceFeatures.drawIndirectFirstInstance",
    "VkPhysicalDeviceVulkan11Features.shaderDrawParameters",
    "VkPhysicalDeviceVulkan12Features.descriptorIndexing",
    "VkPhysicalDeviceVulkan12Features.runtimeDescriptorArray",
    "VkPhysicalDeviceVulkan12Features.descriptorBindingPartiallyBound",
    "VkPhysicalDeviceVulkan12Features.descriptorBindingUpdateUnusedWhilePending",
    "VkPhysicalDeviceVulkan12Features.descriptorBindingSampledImageUpdateAfterBind",
    "VkPhysicalDeviceVulkan12Features.descriptorBindingStorageBufferUpdateAfterBind",
    "VkPhysicalDeviceVulkan12Features.shaderSampledImageArrayNonUniformIndexing",
    "VkPhysicalDeviceVulkan12Features.shaderStorageBufferArrayNonUniformIndexing",
    "VkPhysicalDeviceVulkan12Features.scalarBlockLayout",
    "VkPhysicalDeviceVulkan12Features.timelineSemaphore",
    "VkPhysicalDeviceVulkan13Features.dynamicRendering",
    "VkPhysicalDeviceVulkan13Features.synchronization2",
    "VkPhysicalDeviceVulkan13Features.maintenance4",
    "VkPhysicalDeviceVulkan13Features.shaderDemoteToHelperInvocation",
];

/// The feature structures a device is queried or made with: the core ones and the three
/// version structures.
#[derive(Default)]
pub(crate) struct Features {
    pub core: vk::PhysicalDeviceFeatures,
    pub v11: vk::PhysicalDeviceVulkan11Features<'static>,
    pub v12: vk::PhysicalDeviceVulkan12Features<'static>,
    pub v13: vk::PhysicalDeviceVulkan13Features<'static>,
}

impl Features {
    /// What `pd` has.
    fn query(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Self {
        let mut v11 = vk::PhysicalDeviceVulkan11Features::default();
        let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut f2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut v11)
            .push_next(&mut v12)
            .push_next(&mut v13);
        // SAFETY: the chain is live locals for the call.
        unsafe { instance.get_physical_device_features2(pd, &mut f2) };
        let core = f2.features;
        let mut out = Self {
            core,
            v11,
            v12,
            v13,
        };
        out.v11.p_next = std::ptr::null_mut();
        out.v12.p_next = std::ptr::null_mut();
        out.v13.p_next = std::ptr::null_mut();
        out
    }

    /// The names in [`REQUIRED`] this set lacks (the version is checked apart).
    fn missing(&self) -> Vec<&'static str> {
        let t = |b: vk::Bool32| b == vk::TRUE;
        let have = [
            t(self.core.multi_draw_indirect),
            t(self.core.draw_indirect_first_instance),
            t(self.v11.shader_draw_parameters),
            t(self.v12.descriptor_indexing),
            t(self.v12.runtime_descriptor_array),
            t(self.v12.descriptor_binding_partially_bound),
            t(self.v12.descriptor_binding_update_unused_while_pending),
            t(self.v12.descriptor_binding_sampled_image_update_after_bind),
            t(self.v12.descriptor_binding_storage_buffer_update_after_bind),
            t(self.v12.shader_sampled_image_array_non_uniform_indexing),
            t(self.v12.shader_storage_buffer_array_non_uniform_indexing),
            t(self.v12.scalar_block_layout),
            t(self.v12.timeline_semaphore),
            t(self.v13.dynamic_rendering),
            t(self.v13.synchronization2),
            t(self.v13.maintenance4),
            t(self.v13.shader_demote_to_helper_invocation),
        ];
        REQUIRED[1..]
            .iter()
            .zip(have)
            .filter(|(_, h)| !h)
            .map(|(n, _)| *n)
            .collect()
    }

    /// The set a device is made with: [`REQUIRED`], and anisotropic filtering and block
    /// compression where `have` has them.
    fn wanted(have: &Self) -> Self {
        let mut f = Self::default();
        f.core.multi_draw_indirect = vk::TRUE;
        f.core.draw_indirect_first_instance = vk::TRUE;
        f.core.sampler_anisotropy = have.core.sampler_anisotropy;
        f.core.texture_compression_bc = have.core.texture_compression_bc;
        f.v11.shader_draw_parameters = vk::TRUE;
        f.v12.descriptor_indexing = vk::TRUE;
        f.v12.runtime_descriptor_array = vk::TRUE;
        f.v12.descriptor_binding_partially_bound = vk::TRUE;
        f.v12.descriptor_binding_update_unused_while_pending = vk::TRUE;
        f.v12.descriptor_binding_sampled_image_update_after_bind = vk::TRUE;
        f.v12.descriptor_binding_storage_buffer_update_after_bind = vk::TRUE;
        f.v12.shader_sampled_image_array_non_uniform_indexing = vk::TRUE;
        f.v12.shader_storage_buffer_array_non_uniform_indexing = vk::TRUE;
        f.v12.draw_indirect_count = have.v12.draw_indirect_count;
        f.v12.scalar_block_layout = vk::TRUE;
        f.v12.timeline_semaphore = vk::TRUE;
        f.v13.dynamic_rendering = vk::TRUE;
        f.v13.synchronization2 = vk::TRUE;
        f.v13.maintenance4 = vk::TRUE;
        f.v13.shader_demote_to_helper_invocation = vk::TRUE;
        f
    }
}

/// What the device found it can do.
#[derive(Debug, Clone, PartialEq)]
pub struct Caps {
    /// The adapter's name.
    pub name: String,
    /// Whether it is a hardware adapter rather than a software rasterizer.
    pub hardware: bool,
    /// Whether the queue writes timestamps.
    pub timestamps: bool,
    /// Nanoseconds per timestamp tick.
    pub timestamp_period: f32,
    /// How many textures the bindless table holds.
    pub bindless_textures: u32,
    /// How many storage buffers it holds.
    pub bindless_buffers: u32,
    /// Whether BC1 to BC3 are sampled by the device.
    pub block_compression: bool,
    /// The largest anisotropy a sampler may ask for.
    pub max_anisotropy: f32,
    /// Whether [`Format::Rg11b10Ufloat`](crate::Format::Rg11b10Ufloat) is drawn into, blended
    /// and sampled with filtering.
    pub packed_float_targets: bool,
    /// Whether shaders trace rays through acceleration structures ([`crate::rt`]).
    pub ray_queries: bool,
    /// Whether an indirect draw takes its count from a buffer. Without it, a counted draw draws
    /// all `max` commands, and the commands past the count must draw nothing.
    pub indirect_count: bool,
}

/// When something the device holds may go: the timeline value a submission of its own signals,
/// and the frame stamp of a frame recorded into an adopted command buffer. Either is 0 or -1
/// when no such work used it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Retire {
    pub value: u64,
    pub frame: i64,
}

/// What waits to be destroyed or reused until the work that used it is done.
pub(crate) enum Garbage {
    Buffer(BufferEntry),
    Texture(TextureEntry),
    Sampler(vk::Sampler),
    Pipeline(vk::Pipeline),
    Command(vk::CommandBuffer),
    Staging(crate::resource::Arena),
    Timer(crate::query::Slot),
    /// A bindless index to hand out again.
    TextureIndex(u32),
    BufferIndex(u32),
    SamplerIndex(u32),
    /// An acceleration structure, and a top-level one's index in the table.
    Accel(crate::rt::AccelEntry),
    AccelIndex(u32),
    /// A buffer the layer made for itself, such as a build's scratch.
    Raw(vk::Buffer, Option<gpu_allocator::vulkan::Allocation>),
}

/// An adopted device's frame clock: the stamp of the frame being recorded, and how many frames
/// its owner keeps in flight, so every frame up to `stamp - in_flight` has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    /// The frame being recorded.
    pub stamp: u64,
    /// How many frames are in flight at most.
    pub in_flight: u64,
}

/// A device: its handles, tables, clock and counters.
pub struct Device {
    pub(crate) entry: ash::Entry,
    pub(crate) instance: ash::Instance,
    pub(crate) physical: vk::PhysicalDevice,
    pub(crate) device: ash::Device,
    pub(crate) queue: vk::Queue,
    pub(crate) family: u32,
    /// Whether the instance and device are the layer's own, to destroy with it.
    owned: bool,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    pub(crate) allocator: Option<Allocator>,
    pub(crate) caps: Caps,
    pub(crate) pool: vk::CommandPool,
    pub(crate) free_commands: Vec<vk::CommandBuffer>,
    pub(crate) timeline: vk::Semaphore,
    pub(crate) next_value: u64,
    /// The newest adopted frame known finished, and the newest recorded.
    pub(crate) finished_frame: i64,
    pub(crate) last_frame: i64,
    pub(crate) buffers: Slots<BufferEntry>,
    pub(crate) textures: Slots<TextureEntry>,
    pub(crate) samplers: Slots<SamplerEntry>,
    pub(crate) pipelines: Pipelines,
    pub(crate) bindless: Bindless,
    pub(crate) pending: Vec<PendingCopy>,
    pub(crate) staging: Staging,
    pub(crate) garbage: Vec<(Retire, Garbage)>,
    pub(crate) timers: Timers,
    pub(crate) stats: Stats,
    /// What ray tracing keeps, where the device traces rays.
    pub(crate) rt: Option<crate::rt::Rt>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("caps", &self.caps)
            .field("owned", &self.owned)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

pub(crate) fn vkr<T>(what: &'static str, r: Result<T, vk::Result>) -> Result<T, Error> {
    r.map_err(|result| Error::Vulkan { what, result })
}

const VALIDATION: &CStr = c"VK_LAYER_KHRONOS_validation";

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() {
        return vk::FALSE;
    }
    // SAFETY: the loader hands a live struct whose message is NUL-terminated for the call.
    let text = unsafe {
        let d = &*data;
        if d.p_message.is_null() {
            String::new()
        } else {
            CStr::from_ptr(d.p_message).to_string_lossy().into_owned()
        }
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!(target: "dereth::gfx", "validation: {text}");
        if let Ok(mut m) = VALIDATION_ERRORS.lock() {
            m.push(text);
        }
    } else {
        tracing::warn!(target: "dereth::gfx", "validation: {text}");
    }
    vk::FALSE
}

/// The validation layer's errors since they were last taken, for the tests.
static VALIDATION_ERRORS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Keep an error the driver's validation reported, for [`take_validation_errors`].
#[cfg_attr(not(windows), allow(dead_code))] // Direct3D 12's debug layer reports through it
pub(crate) fn validation_error(text: String) {
    tracing::error!(target: "dereth::gfx", "validation: {text}");
    if let Ok(mut m) = VALIDATION_ERRORS.lock() {
        m.push(text);
    }
}

/// The validation layer's errors since they were last taken: empty when it reported none or is
/// not installed.
#[must_use]
pub fn take_validation_errors() -> Vec<String> {
    VALIDATION_ERRORS
        .lock()
        .map(|mut m| std::mem::take(&mut *m))
        .unwrap_or_default()
}

impl Device {
    /// Make a device of the layer's own, with no window: the hardware adapter unless
    /// `desc.software` asks for a software rasterizer, the best one with every feature
    /// [`REQUIRED`] lists and a queue that draws and computes.
    ///
    /// # Errors
    /// [`Error::Unsupported`] when there is no Vulkan 1.3 loader or no such adapter, or any
    /// failure of the driver.
    pub fn create(desc: &DeviceDesc) -> Result<Self, Error> {
        // SAFETY: loading the Vulkan loader library has no preconditions; the entry owns it.
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| Error::Unsupported(format!("no Vulkan loader: {e}")))?;
        // SAFETY: a pure loader query.
        let version = unsafe { entry.try_enumerate_instance_version() }
            .ok()
            .flatten()
            .unwrap_or(vk::API_VERSION_1_0);
        if version < vk::API_VERSION_1_3 {
            return Err(Error::Unsupported(
                "the Vulkan loader is older than 1.3".into(),
            ));
        }
        // SAFETY: pure loader queries.
        let layers = unsafe { entry.enumerate_instance_layer_properties() }.unwrap_or_default();
        // SAFETY: as above.
        let extensions =
            unsafe { entry.enumerate_instance_extension_properties(None) }.unwrap_or_default();
        let has_layer = layers
            .iter()
            .any(|l| l.layer_name_as_c_str().is_ok_and(|n| n == VALIDATION));
        let has_utils = extensions.iter().any(|e| {
            e.extension_name_as_c_str()
                .is_ok_and(|n| n == ash::ext::debug_utils::NAME)
        });
        let debug = desc.debug && has_layer && has_utils;
        let mut layer_names: Vec<*const c_char> = Vec::new();
        let mut ext_names: Vec<*const c_char> = Vec::new();
        if debug {
            layer_names.push(VALIDATION.as_ptr());
            ext_names.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        // A portability implementation (MoltenVK, over Metal) is listed only behind this flag.
        let mut flags = vk::InstanceCreateFlags::empty();
        if extensions.iter().any(|e| {
            e.extension_name_as_c_str()
                .is_ok_and(|n| n == ash::khr::portability_enumeration::NAME)
        }) {
            ext_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }
        let app = vk::ApplicationInfo::default()
            .application_name(c"dereth-remaster")
            .engine_name(c"dereth-gfx")
            .api_version(vk::API_VERSION_1_3);
        let info = vk::InstanceCreateInfo::default()
            .flags(flags)
            .application_info(&app)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&ext_names);
        // SAFETY: every pointer in `info` is to a live local or a static name.
        let instance = vkr("vkCreateInstance", unsafe {
            entry.create_instance(&info, None)
        })?;
        let messenger = if debug {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL,
                )
                .pfn_user_callback(Some(debug_callback));
            // SAFETY: the callback has the required signature and touches only statics.
            unsafe { loader.create_debug_utils_messenger(&info, None) }
                .ok()
                .map(|m| (loader, m))
        } else {
            None
        };
        match Self::create_on(&entry, &instance, desc) {
            Ok((physical, device, queue, family, ray_queries)) => {
                let native = Native {
                    entry: entry.clone(),
                    instance: instance.clone(),
                    physical_device: physical,
                    device,
                    queue,
                    queue_family: family,
                    ray_queries,
                };
                let mut d = Self::build(native, desc, true)?;
                d.debug = messenger;
                Ok(d)
            }
            Err(e) => {
                // SAFETY: nothing made from the instance outlives it but the messenger, which
                // goes first.
                unsafe {
                    if let Some((l, m)) = messenger {
                        l.destroy_debug_utils_messenger(m, None);
                    }
                    instance.destroy_instance(None);
                }
                Err(e)
            }
        }
    }

    /// The physical device, device, queue and family a device made here uses, and whether it
    /// was made with ray queries (where the adapter has them).
    fn create_on(
        entry: &ash::Entry,
        instance: &ash::Instance,
        desc: &DeviceDesc,
    ) -> Result<(vk::PhysicalDevice, ash::Device, vk::Queue, u32, bool), Error> {
        let _ = entry;
        // SAFETY: a pure query.
        let devices = vkr("vkEnumeratePhysicalDevices", unsafe {
            instance.enumerate_physical_devices()
        })?;
        let mut best: Option<(i32, vk::PhysicalDevice, u32)> = None;
        let mut refused = Vec::new();
        for pd in devices {
            // SAFETY: pure queries on a live physical device.
            let (props, families) = unsafe {
                (
                    instance.get_physical_device_properties(pd),
                    instance.get_physical_device_queue_family_properties(pd),
                )
            };
            let name = props
                .device_name_as_c_str()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if props.api_version < vk::API_VERSION_1_3 {
                refused.push(format!("{name}: Vulkan older than 1.3"));
                continue;
            }
            let missing = Features::query(instance, pd).missing();
            if !missing.is_empty() {
                refused.push(format!("{name}: lacks {}", missing.join(", ")));
                continue;
            }
            let Some(family) = families.iter().position(|f| {
                f.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
            }) else {
                continue;
            };
            let cpu = props.device_type == vk::PhysicalDeviceType::CPU;
            let rank = match (desc.software, props.device_type) {
                (true, _) if cpu => 10,
                (_, vk::PhysicalDeviceType::DISCRETE_GPU) => 4,
                (_, vk::PhysicalDeviceType::INTEGRATED_GPU) => 3,
                (_, vk::PhysicalDeviceType::VIRTUAL_GPU) => 2,
                (_, vk::PhysicalDeviceType::CPU) => 1,
                _ => 0,
            };
            #[allow(clippy::cast_possible_truncation)] // a queue family index
            let family = family as u32;
            if best.is_none_or(|b| rank > b.0) {
                best = Some((rank, pd, family));
            }
        }
        let Some((_, pd, family)) = best else {
            return Err(Error::Unsupported(format!(
                "no Vulkan 1.3 adapter has what the layer draws with ({})",
                refused.join("; ")
            )));
        };
        let have = Features::query(instance, pd);
        let mut want = Features::wanted(&have);
        let priorities = [1.0f32];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        // Ray queries, where the adapter has them.
        let traces = crate::rt::adapter_traces(instance, pd);
        let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
            .acceleration_structure(true)
            .descriptor_binding_acceleration_structure_update_after_bind(true);
        let mut query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
        let mut extensions: Vec<*const c_char> = if traces {
            want.v12.buffer_device_address = vk::TRUE;
            crate::rt::EXTENSIONS.iter().map(|n| n.as_ptr()).collect()
        } else {
            Vec::new()
        };
        let mut f2 = vk::PhysicalDeviceFeatures2::default()
            .features(want.core)
            .push_next(&mut want.v11)
            .push_next(&mut want.v12)
            .push_next(&mut want.v13);
        // SAFETY: a pure query.
        let portability = unsafe { instance.enumerate_device_extension_properties(pd) }
            .unwrap_or_default()
            .iter()
            .any(|e| {
                e.extension_name_as_c_str()
                    .is_ok_and(|n| n == ash::khr::portability_subset::NAME)
            });
        if portability {
            extensions.push(ash::khr::portability_subset::NAME.as_ptr());
        }
        if traces {
            f2 = f2.push_next(&mut accel).push_next(&mut query);
        }
        let info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&extensions)
            .push_next(&mut f2);
        // SAFETY: every pointer in `info` is to a live local.
        let device = vkr("vkCreateDevice", unsafe {
            instance.create_device(pd, &info, None)
        })?;
        // SAFETY: the family was chosen from the device's own list.
        let queue = unsafe { device.get_device_queue(family, 0) };
        Ok((pd, device, queue, family, traces))
    }

    /// Adopt a device that already exists. The layer makes its own allocator, command pool,
    /// timeline, descriptor table and pipeline cache on it, and destroys only those.
    ///
    /// # Safety
    /// The handles must be live, the device made with every feature [`REQUIRED`] lists, and they
    /// must outlive the layer. Everything the layer submits goes to `native.queue`, from the
    /// thread that drives the layer, which must be the one its owner submits from.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub unsafe fn adopt(native: Native, desc: &DeviceDesc) -> Result<Self, Error> {
        Self::build(native, desc, false)
    }

    /// The rest of a device, made or adopted alike.
    fn build(native: Native, desc: &DeviceDesc, owned: bool) -> Result<Self, Error> {
        let Native {
            entry,
            instance,
            physical_device: physical,
            device,
            queue,
            queue_family: family,
            ray_queries,
        } = native;
        // SAFETY: pure queries on a live physical device.
        let (props, families, features) = unsafe {
            (
                instance.get_physical_device_properties(physical),
                instance.get_physical_device_queue_family_properties(physical),
                instance.get_physical_device_features(physical),
            )
        };
        let mut v12_props = vk::PhysicalDeviceDescriptorIndexingProperties::default();
        let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut v12_props);
        // SAFETY: the chain is live locals for the call.
        unsafe { instance.get_physical_device_properties2(physical, &mut props2) };
        let limits = props.limits;
        let max_images = v12_props
            .max_per_stage_descriptor_update_after_bind_sampled_images
            .min(v12_props.max_descriptor_set_update_after_bind_sampled_images);
        let max_buffers = v12_props
            .max_per_stage_descriptor_update_after_bind_storage_buffers
            .min(v12_props.max_descriptor_set_update_after_bind_storage_buffers)
            / 2;
        // SAFETY: a pure query on a live physical device.
        let packed = unsafe {
            instance.get_physical_device_format_properties(
                physical,
                vk::Format::B10G11R11_UFLOAT_PACK32,
            )
        };
        let packed_float_targets = packed.optimal_tiling_features.contains(
            vk::FormatFeatureFlags::COLOR_ATTACHMENT
                | vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND
                | vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        );
        let timestamps = limits.timestamp_compute_and_graphics == vk::TRUE
            || families
                .get(family as usize)
                .is_some_and(|f| f.timestamp_valid_bits > 0);
        // A device made here, or by the ordinary renderer, enables it wherever it is supported.
        let indirect_count =
            Features::query(&instance, physical).v12.draw_indirect_count == vk::TRUE;
        let caps = Caps {
            name: props
                .device_name_as_c_str()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            hardware: props.device_type != vk::PhysicalDeviceType::CPU,
            timestamps,
            timestamp_period: limits.timestamp_period,
            bindless_textures: desc.bindless_textures.min(max_images),
            bindless_buffers: desc.bindless_buffers.min(max_buffers),
            block_compression: features.texture_compression_bc == vk::TRUE,
            max_anisotropy: if features.sampler_anisotropy == vk::TRUE {
                limits.max_sampler_anisotropy
            } else {
                1.0
            },
            packed_float_targets,
            ray_queries,
            indirect_count,
        };
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: gpu_allocator::AllocatorDebugSettings::default(),
            // Acceleration structures and their inputs are reached by their addresses.
            buffer_device_address: ray_queries,
            allocation_sizes: gpu_allocator::AllocationSizes::default(),
        })
        .map_err(|e| Error::Allocation(e.to_string()))?;
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY: a live local; destroyed in `Drop`.
        let pool = vkr("vkCreateCommandPool", unsafe {
            device.create_command_pool(&pool_info, None)
        })?;
        let mut timeline_type = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let sem_info = vk::SemaphoreCreateInfo::default().push_next(&mut timeline_type);
        // SAFETY: a live local; destroyed in `Drop`.
        let timeline = vkr("vkCreateSemaphore", unsafe {
            device.create_semaphore(&sem_info, None)
        })?;
        let bindless = Bindless::new(
            &device,
            caps.bindless_textures,
            caps.bindless_buffers,
            ray_queries.then_some(crate::rt::ACCELS),
        )?;
        let rt = ray_queries.then(|| crate::rt::Rt {
            loader: ash::khr::acceleration_structure::Device::new(&instance, &device),
            scratch_align: crate::rt::scratch_alignment(&instance, physical),
            accels: Slots::default(),
            pending: Vec::new(),
            indices: crate::bindless::Indices::new(crate::rt::ACCELS),
        });
        let pipelines = Pipelines::new(&device, bindless.layout)?;
        Ok(Self {
            entry,
            instance,
            physical,
            device,
            queue,
            family,
            owned,
            debug: None,
            allocator: Some(allocator),
            caps,
            pool,
            free_commands: Vec::new(),
            timeline,
            next_value: 1,
            finished_frame: -1,
            last_frame: -1,
            buffers: Slots::default(),
            textures: Slots::default(),
            samplers: Slots::default(),
            pipelines,
            bindless,
            pending: Vec::new(),
            staging: Staging::default(),
            garbage: Vec::new(),
            timers: Timers::default(),
            stats: Stats::default(),
            rt,
        })
    }

    /// What the device found it can do.
    #[must_use]
    pub fn caps(&self) -> &Caps {
        &self.caps
    }

    /// What the device has counted since it was made.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Whether the Khronos validation layer is checking the device's calls.
    #[must_use]
    pub fn validating(&self) -> bool {
        self.debug.is_some()
    }

    /// Whether the instance and device are the layer's own rather than adopted.
    #[must_use]
    pub fn owned(&self) -> bool {
        self.owned
    }

    /// The family of the queue the layer submits to.
    #[must_use]
    pub fn queue_family(&self) -> u32 {
        self.family
    }

    /// The raw device, for what the layer does not wrap.
    #[must_use]
    pub fn raw(&self) -> &ash::Device {
        &self.device
    }

    /// The instance and physical device, for what the layer does not wrap.
    #[must_use]
    pub fn raw_instance(&self) -> (vk::Instance, vk::PhysicalDevice) {
        (self.instance.handle(), self.physical)
    }

    /// The loader the device came from.
    #[must_use]
    pub fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    /// When work recorded now will have finished: the next submission of its own, and the frame
    /// last recorded into an adopted command buffer.
    pub(crate) fn now(&self) -> Retire {
        Retire {
            value: self.next_value.saturating_sub(1),
            frame: self.last_frame,
        }
    }

    /// The highest timeline value the device has passed.
    pub(crate) fn completed(&self) -> u64 {
        // SAFETY: a pure query on a live semaphore.
        unsafe { self.device.get_semaphore_counter_value(self.timeline) }.unwrap_or(0)
    }

    /// Hold `g` until the work recorded so far is done.
    pub(crate) fn defer(&mut self, g: Garbage) {
        let at = self.now();
        self.garbage.push((at, g));
    }

    /// Destroy or reuse what the finished work was holding.
    pub(crate) fn collect(&mut self) {
        if self.garbage.is_empty() {
            return;
        }
        let completed = self.completed();
        let finished = self.finished_frame;
        let (done, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.garbage)
            .into_iter()
            .partition(|(r, _)| r.value <= completed && r.frame <= finished);
        self.garbage = keep;
        for (_, g) in done {
            self.free(g);
        }
    }

    /// Destroy or reuse `g` now.
    fn free(&mut self, g: Garbage) {
        match g {
            Garbage::Buffer(b) => self.destroy_buffer_now(b),
            Garbage::Texture(t) => self.destroy_texture_now(t),
            // SAFETY: the work that used them is done.
            Garbage::Sampler(s) => unsafe { self.device.destroy_sampler(s, None) },
            // SAFETY: as above.
            Garbage::Pipeline(p) => unsafe { self.device.destroy_pipeline(p, None) },
            Garbage::Command(c) => self.free_commands.push(c),
            Garbage::Staging(a) => self.staging.free.push(a),
            Garbage::Timer(slot) => {
                self.timers
                    .retired(&self.device, slot, self.caps.timestamp_period)
            }
            Garbage::TextureIndex(i) => self.bindless.textures.release(i),
            Garbage::BufferIndex(i) => self.bindless.buffers.release(i),
            Garbage::SamplerIndex(i) => self.bindless.samplers.release(i),
            Garbage::Accel(e) => self.destroy_accel_now(e),
            Garbage::AccelIndex(i) => {
                if let Some(rt) = self.rt.as_mut() {
                    rt.indices.release(i);
                }
            }
            Garbage::Raw(b, a) => self.free_raw(b, a),
        }
    }

    /// A ticket for the device's own timeline.
    pub(crate) fn signal_next(&mut self) -> u64 {
        let v = self.next_value;
        self.next_value += 1;
        v
    }

    /// Block until the device has finished the submission `ticket` names, then let go of what
    /// finished work was holding.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn wait(&mut self, ticket: Ticket) -> Result<(), Error> {
        let sems = [self.timeline];
        let values = [ticket.0];
        let info = vk::SemaphoreWaitInfo::default()
            .semaphores(&sems)
            .values(&values);
        // SAFETY: live locals on a live semaphore.
        vkr("vkWaitSemaphores", unsafe {
            self.device.wait_semaphores(&info, u64::MAX)
        })?;
        self.collect();
        Ok(())
    }

    /// Block until the device's queue is idle: every submission of the layer's and of the owner
    /// of an adopted device. Then let go of everything held.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn wait_idle(&mut self) -> Result<(), Error> {
        // SAFETY: the queue is live; the caller drives the layer from the queue's one thread.
        vkr("vkQueueWaitIdle", unsafe {
            self.device.queue_wait_idle(self.queue)
        })?;
        self.finished_frame = self.last_frame;
        self.collect();
        Ok(())
    }
}

/// A submission of the device's own, to wait for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticket(pub(crate) u64);

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the queue is live; nothing else submits during the drop.
        let _ = unsafe { self.device.queue_wait_idle(self.queue) };
        self.finished_frame = i64::MAX;
        self.next_value = self.next_value.max(1);
        // Everything held for finished work, then everything alive.
        let garbage = std::mem::take(&mut self.garbage);
        for (_, g) in garbage {
            self.free(g);
        }
        for b in self.buffers.drain() {
            self.destroy_buffer_now(b);
        }
        for t in self.textures.drain() {
            self.destroy_texture_now(t);
        }
        for s in self.samplers.drain() {
            // SAFETY: the queue is idle.
            unsafe { self.device.destroy_sampler(s.sampler, None) };
        }
        let accels = self
            .rt
            .as_mut()
            .map(|rt| rt.accels.drain())
            .unwrap_or_default();
        for a in accels {
            self.destroy_accel_now(a);
        }
        let staging = std::mem::take(&mut self.staging);
        for a in staging.into_arenas() {
            self.destroy_arena(a);
        }
        // SAFETY: the queue is idle and nothing made here is used again.
        unsafe {
            self.timers.destroy(&self.device);
            self.pipelines.destroy(&self.device);
            self.bindless.destroy(&self.device);
            self.device.destroy_semaphore(self.timeline, None);
            self.device.destroy_command_pool(self.pool, None);
        }
        drop(self.allocator.take());
        if self.owned {
            // SAFETY: everything made from the device and the instance is gone.
            unsafe {
                self.device.destroy_device(None);
                if let Some((l, m)) = self.debug.take() {
                    l.destroy_debug_utils_messenger(m, None);
                }
                self.instance.destroy_instance(None);
            }
        }
    }
}
