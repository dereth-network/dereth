//! The Metal objects behind a device that runs over Metal, for what only Metal offers. They are
//! there when the device was made with `VK_EXT_metal_objects`.

use std::ffi::c_void;

use ash::vk;

use crate::device::Ticket;
use crate::front::Dev;
use crate::{Device, Error, Texture};

/// The device's Metal objects, each a borrowed `id`.
#[derive(Debug, Clone, Copy)]
pub struct MetalObjects {
    /// The `MTLDevice`.
    pub device: *mut c_void,
    /// The queue's `MTLCommandQueue`.
    pub queue: *mut c_void,
    /// The device's timeline's `MTLSharedEvent`: a [`Ticket`]'s value is signaled on it.
    pub timeline: *mut c_void,
}

impl Ticket {
    /// The value the device's timeline reaches when this ticket's work is done.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }
}

impl crate::device::Device {
    /// Ask for what `info` chains; `false` where the device was made without the extension.
    fn export_metal(&self, info: &mut vk::ExportMetalObjectsInfoEXT<'_>) -> bool {
        // SAFETY: a pure query on the live device.
        let there = unsafe {
            self.instance
                .get_device_proc_addr(self.device.handle(), c"vkExportMetalObjectsEXT".as_ptr())
        };
        if there.is_none() {
            return false;
        }
        let ext = ash::ext::metal_objects::Device::new(&self.instance, &self.device);
        // SAFETY: `info` and what it chains are live locals of the caller, naming the device's own
        // live objects.
        unsafe { (ext.fp().export_metal_objects_ext)(ext.device(), info) };
        true
    }
}

impl Device {
    /// The device's Metal objects, or `None` where it has none to give.
    #[must_use]
    pub fn metal_objects(&self) -> Option<MetalObjects> {
        let Dev::Vk(d) = &self.b;
        let mut device = vk::ExportMetalDeviceInfoEXT::default();
        let mut queue = vk::ExportMetalCommandQueueInfoEXT::default().queue(d.queue);
        let mut timeline = vk::ExportMetalSharedEventInfoEXT::default().semaphore(d.timeline);
        let mut info = vk::ExportMetalObjectsInfoEXT::default()
            .push_next(&mut device)
            .push_next(&mut queue)
            .push_next(&mut timeline);
        if !d.export_metal(&mut info) {
            return None;
        }
        let objects = MetalObjects {
            device: device.mtl_device.cast(),
            queue: queue.mtl_command_queue.cast(),
            timeline: timeline.mtl_shared_event.cast(),
        };
        (!objects.device.is_null() && !objects.queue.is_null() && !objects.timeline.is_null())
            .then_some(objects)
    }

    /// The `MTLTexture` behind `t`, a borrowed `id`, null where the device has none to give.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold.
    pub fn metal_texture(&self, t: Texture) -> Result<*mut c_void, Error> {
        let Dev::Vk(d) = &self.b;
        let image = d.texture(t)?.image;
        let mut texture = vk::ExportMetalTextureInfoEXT::default()
            .image(image)
            .plane(vk::ImageAspectFlags::PLANE_0);
        let mut info = vk::ExportMetalObjectsInfoEXT::default().push_next(&mut texture);
        if !d.export_metal(&mut info) {
            return Ok(std::ptr::null_mut());
        }
        Ok(texture.mtl_texture.cast())
    }
}
