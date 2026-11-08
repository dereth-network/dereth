//! The one bindless descriptor table: every texture, sampler and storage buffer a shader reads,
//! each at an index the shader is handed.
//!
//! One descriptor set, bound at set 0 of every pipeline:
//!
//! | binding | holds | count |
//! |---|---|---|
//! | 0 | sampled images | [`crate::Caps::bindless_textures`] |
//! | 1 | samplers | [`SAMPLERS`] |
//! | 2 | storage buffers, read | [`crate::Caps::bindless_buffers`] |
//! | 3 | the same storage buffers, read and written | the same |
//! | 4 | top-level acceleration structures, only where the device traces rays | [`crate::rt::ACCELS`] |
//!
//! Every binding is partially bound and updated after binding, so an index is written while a
//! frame that does not use it is in flight, and an index no live resource holds is never read.

use ash::vk;

use crate::device::{vkr, Device};
use crate::{Buffer, BufferUsage, Error, Sampler, Texture};

/// How many samplers the table holds.
pub const SAMPLERS: u32 = 32;

/// A list of free indices below a capacity.
#[derive(Debug)]
pub(crate) struct Indices {
    next: u32,
    free: Vec<u32>,
    capacity: u32,
}

impl Indices {
    pub(crate) fn new(capacity: u32) -> Self {
        Self {
            next: 0,
            free: Vec::new(),
            capacity,
        }
    }

    pub(crate) fn take(&mut self) -> Option<u32> {
        if let Some(i) = self.free.pop() {
            return Some(i);
        }
        (self.next < self.capacity).then(|| {
            self.next += 1;
            self.next - 1
        })
    }

    pub(crate) fn release(&mut self, i: u32) {
        self.free.push(i);
    }
}

/// The table: its layout, pool and set, and the indices free in each binding.
#[derive(Debug)]
pub(crate) struct Bindless {
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
    pub textures: Indices,
    pub samplers: Indices,
    pub buffers: Indices,
}

impl Bindless {
    pub(crate) fn new(
        device: &ash::Device,
        textures: u32,
        buffers: u32,
        accels: Option<u32>,
    ) -> Result<Self, Error> {
        let all = vk::ShaderStageFlags::ALL;
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(textures)
                .stage_flags(all),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(SAMPLERS)
                .stage_flags(all),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(buffers)
                .stage_flags(all),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(buffers)
                .stage_flags(all),
        ];
        if let Some(n) = accels {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(4)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(n)
                    .stage_flags(all),
            );
        }
        let flag = vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING;
        let flags = vec![flag; bindings.len()];
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);
        let info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .bindings(&bindings)
            .push_next(&mut binding_flags);
        // SAFETY: live locals; destroyed with the table.
        let layout = vkr("vkCreateDescriptorSetLayout", unsafe {
            device.create_descriptor_set_layout(&info, None)
        })?;
        let mut sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: textures,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: SAMPLERS,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: buffers * 2,
            },
        ];
        if let Some(n) = accels {
            sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: n,
            });
        }
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: a live local; destroyed with the table.
        let pool = match unsafe { device.create_descriptor_pool(&pool_info, None) } {
            Ok(p) => p,
            Err(e) => {
                // SAFETY: unreferenced.
                unsafe { device.destroy_descriptor_set_layout(layout, None) };
                return Err(Error::Vulkan {
                    what: "vkCreateDescriptorPool",
                    result: e,
                });
            }
        };
        let layouts = [layout];
        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: the pool holds exactly this one set.
        let set = match unsafe { device.allocate_descriptor_sets(&alloc) } {
            Ok(s) => s[0],
            Err(e) => {
                // SAFETY: unreferenced.
                unsafe {
                    device.destroy_descriptor_pool(pool, None);
                    device.destroy_descriptor_set_layout(layout, None);
                }
                return Err(Error::Vulkan {
                    what: "vkAllocateDescriptorSets",
                    result: e,
                });
            }
        };
        Ok(Self {
            layout,
            pool,
            set,
            textures: Indices::new(textures),
            samplers: Indices::new(SAMPLERS),
            buffers: Indices::new(buffers),
        })
    }

    /// # Safety
    /// Nothing the device runs uses the table any more.
    pub(crate) unsafe fn destroy(&self, device: &ash::Device) {
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_descriptor_set_layout(self.layout, None);
    }
}

impl Device {
    /// The index shaders sample `t` at, given one on first asking. A texture made for sampling
    /// is ready to sample once its levels are uploaded; a target drawn into is sampled after a
    /// pass that says it samples it ([`crate::Use::Sampled`]).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture not made for sampling, [`Error::Full`] when the table
    /// is.
    pub fn bind_texture(&mut self, t: Texture) -> Result<u32, Error> {
        let e = self.texture(t)?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        let view = e.sample_view;
        if view == vk::ImageView::null() {
            return Err(Error::Invalid(
                "a texture made without the sampled use is bound".into(),
            ));
        }
        let index = self
            .bindless
            .textures
            .take()
            .ok_or(Error::Full("textures"))?;
        let info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.bindless.set)
            .dst_binding(0)
            .dst_array_element(index)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(&info);
        // SAFETY: the binding is update-after-bind and partially bound; no pending work reads
        // this index, which no live texture held.
        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
        if let Some(e) = self.textures.get_mut(t.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }

    /// The index shaders take `s` at, given one on first asking.
    ///
    /// # Errors
    /// [`Error::Full`] when the table's samplers are.
    pub fn bind_sampler(&mut self, s: Sampler) -> Result<u32, Error> {
        let e = self
            .samplers
            .get(s.0)
            .ok_or_else(|| Error::Invalid(format!("no sampler {}", s.0)))?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        let sampler = e.sampler;
        let index = self
            .bindless
            .samplers
            .take()
            .ok_or(Error::Full("samplers"))?;
        let info = [vk::DescriptorImageInfo {
            sampler,
            image_view: vk::ImageView::null(),
            image_layout: vk::ImageLayout::UNDEFINED,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.bindless.set)
            .dst_binding(1)
            .dst_array_element(index)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .image_info(&info);
        // SAFETY: as in `bind_texture`.
        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
        if let Some(e) = self.samplers.get_mut(s.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }

    /// The index shaders read and write `b` at as a storage buffer, given one on first asking.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer not made for storage, [`Error::Full`] when the table is.
    pub fn bind_buffer(&mut self, b: Buffer) -> Result<u32, Error> {
        let e = self.buffer(b)?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        if !e.usage.contains(BufferUsage::STORAGE) {
            return Err(Error::Invalid("the buffer is not a storage buffer".into()));
        }
        let buffer = e.buffer;
        let index = self.bindless.buffers.take().ok_or(Error::Full("buffers"))?;
        let info = [vk::DescriptorBufferInfo {
            buffer,
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let writes = [2, 3].map(|binding| {
            vk::WriteDescriptorSet::default()
                .dst_set(self.bindless.set)
                .dst_binding(binding)
                .dst_array_element(index)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&info)
        });
        // SAFETY: as in `bind_texture`.
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        if let Some(e) = self.buffers.get_mut(b.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }
}
