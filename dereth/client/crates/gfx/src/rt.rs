//! Acceleration structures for inline ray queries, where the device has them.
//!
//! A bottom-level structure ([`Blas`]) holds one mesh's triangles, one geometry a run of them;
//! a top-level structure ([`Tlas`]) holds the instances of bottom-level ones a frame traces
//! against, each with its transform, a 24-bit index of the user's and an 8-bit mask. Shaders
//! reach a top-level structure through the bindless table (binding 4, `accels[]`) at the index
//! [`Device::bind_tlas`] gives.
//!
//! **Only where the device can.** Everything here needs ray queries ([`crate::Caps::ray_queries`]):
//! a device made here turns them on when the adapter has them, an adopted one says whether its
//! owner did ([`crate::Native::ray_queries`]). Without them every call refuses with
//! [`Error::Unsupported`] and the table has no binding 4.
//!
//! **When builds happen.** A bottom-level structure is built at the head of the next recording,
//! after the staged copies, so its vertices and indices may be written in the same frame it is
//! made in; they need [`crate::BufferUsage::ACCELERATION_INPUT`]. A top-level structure is built
//! where [`Recorder::build_tlas`] is called, from instances given by the processor or written by
//! a shader into a buffer ([`Recorder::build_tlas_from`]). A build is followed by a barrier that
//! makes it visible to every later shader and build; it waits for every earlier use.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use gpu_allocator::MemoryLocation;

use crate::device::{Device, Garbage};
use crate::recorder::Recorder;
use crate::resource::Slots;
use crate::{Buffer, BufferUsage, Error, IndexType};

/// How many top-level structures the bindless table holds.
pub const ACCELS: u32 = 16;

/// The bytes of one instance as a top-level build reads it.
pub const INSTANCE_BYTES: u64 = 64;

/// A bottom-level acceleration structure the device holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Blas(pub(crate) u32);

/// A top-level acceleration structure the device holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tlas(pub(crate) u32);

/// One geometry of a bottom-level structure: a run of indexed triangles, positions three floats
/// at the head of each vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlasGeometry {
    /// The vertices' buffer, the byte its first vertex starts at, and the bytes from one vertex
    /// to the next.
    pub vertices: Buffer,
    pub vertex_offset: u64,
    pub vertex_stride: u64,
    /// How many vertices the indices reach, from the first.
    pub vertex_count: u32,
    /// The indices' buffer, the byte the first index starts at, and their width.
    pub indices: Buffer,
    pub index_offset: u64,
    pub index_type: IndexType,
    /// How many triangles: three indices each.
    pub triangles: u32,
    /// Whether every hit on it is taken as it is; otherwise a query is handed each candidate
    /// hit to accept or not (an alpha test).
    pub opaque: bool,
}

/// One instance of a top-level structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TlasInstance {
    /// The bottom-level structure it places.
    pub blas: Blas,
    /// Its transform, three rows of four: the first three columns turn and scale, the fourth
    /// moves.
    pub transform: [f32; 12],
    /// The user's index, 24 bits, which a hit reports.
    pub custom: u32,
    /// Its mask: a query sees it when the query's mask shares a bit with it.
    pub mask: u8,
}

/// What the device keeps for ray tracing.
pub(crate) struct Rt {
    pub loader: ash::khr::acceleration_structure::Device,
    /// The alignment a build's scratch memory must have.
    pub scratch_align: u64,
    pub accels: Slots<AccelEntry>,
    /// The bottom-level structures waiting for the next recording's head.
    pub pending: Vec<u32>,
    /// The table's free indices for top-level structures.
    pub indices: crate::bindless::Indices,
}

/// A live acceleration structure and its memory.
pub(crate) struct AccelEntry {
    pub accel: vk::AccelerationStructureKHR,
    pub buffer: vk::Buffer,
    pub allocation: Option<Allocation>,
    pub address: u64,
    pub bytes: u64,
    pub kind: AccelKind,
}

pub(crate) enum AccelKind {
    /// A bottom-level structure: its geometries, and the scratch its build needs.
    Bottom {
        geometries: Vec<BlasGeometry>,
        scratch: u64,
        built: bool,
    },
    /// A top-level structure: how many instances it holds at most, the buffer its instances are
    /// copied into, its scratch, and its index in the table.
    Top {
        capacity: u32,
        instances: (vk::Buffer, Option<Allocation>, u64),
        scratch: (vk::Buffer, Option<Allocation>, u64),
        bindless: Option<u32>,
    },
}

/// The geometry description of `g` with its buffers at `vertices` and `indices`.
fn triangles(
    g: &BlasGeometry,
    vertices: u64,
    indices: u64,
) -> vk::AccelerationStructureGeometryKHR<'static> {
    let data = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR {
            device_address: vertices + g.vertex_offset,
        })
        .vertex_stride(g.vertex_stride)
        .max_vertex(g.vertex_count.saturating_sub(1))
        .index_type(match g.index_type {
            IndexType::U16 => vk::IndexType::UINT16,
            IndexType::U32 => vk::IndexType::UINT32,
        })
        .index_data(vk::DeviceOrHostAddressConstKHR {
            device_address: indices + g.index_offset,
        });
    vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .geometry(vk::AccelerationStructureGeometryDataKHR { triangles: data })
        .flags(if g.opaque {
            vk::GeometryFlagsKHR::OPAQUE
        } else {
            vk::GeometryFlagsKHR::empty()
        })
}

/// The geometry description of `count` instances at `address`.
fn instances(address: u64) -> vk::AccelerationStructureGeometryKHR<'static> {
    vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                .array_of_pointers(false)
                .data(vk::DeviceOrHostAddressConstKHR {
                    device_address: address,
                }),
        })
}

/// The bytes of one instance as a build reads it: `blas` at `reference`.
#[must_use]
pub fn instance_bytes(i: &TlasInstance, reference: u64) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (k, f) in i.transform.iter().enumerate() {
        out[k * 4..k * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    let custom = (i.custom & 0x00FF_FFFF) | (u32::from(i.mask) << 24);
    out[48..52].copy_from_slice(&custom.to_le_bytes());
    // No shader binding table offset; neither face is culled.
    let flags = vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() << 24;
    out[52..56].copy_from_slice(&flags.to_le_bytes());
    out[56..64].copy_from_slice(&reference.to_le_bytes());
    out
}

impl Device {
    fn rt(&self) -> Result<&Rt, Error> {
        self.rt.as_ref().ok_or_else(|| {
            Error::Unsupported("this device cannot trace rays (no ray queries)".into())
        })
    }

    fn rt_mut(&mut self) -> Result<&mut Rt, Error> {
        self.rt.as_mut().ok_or_else(|| {
            Error::Unsupported("this device cannot trace rays (no ray queries)".into())
        })
    }

    /// A buffer's device address. It must have been made for acceleration inputs.
    pub(crate) fn buffer_address(&self, b: Buffer) -> Result<u64, Error> {
        let e = self.buffer(b)?;
        if !e.usage.contains(BufferUsage::ACCELERATION_INPUT) {
            return Err(Error::Invalid(
                "a buffer read by a build was not made for acceleration inputs".into(),
            ));
        }
        let info = vk::BufferDeviceAddressInfo::default().buffer(e.buffer);
        // SAFETY: the buffer is live and made with the device-address use.
        Ok(unsafe { self.device.get_buffer_device_address(&info) })
    }

    /// A raw buffer for an acceleration structure, its instances or its scratch, with its
    /// address; `extra` bytes more than `size`, for the scratch's alignment.
    fn accel_buffer(
        &mut self,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> Result<(vk::Buffer, Allocation, u64), Error> {
        let (buffer, allocation) = self.raw_buffer(
            name,
            size,
            usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::GpuOnly,
        )?;
        let info = vk::BufferDeviceAddressInfo::default().buffer(buffer);
        // SAFETY: the buffer is live and made with the device-address use.
        let address = unsafe { self.device.get_buffer_device_address(&info) };
        Ok((buffer, allocation, address))
    }

    /// An acceleration structure of `bytes` bytes and `ty`, in a buffer of its own.
    fn make_accel(
        &mut self,
        name: &str,
        bytes: u64,
        ty: vk::AccelerationStructureTypeKHR,
    ) -> Result<(vk::AccelerationStructureKHR, vk::Buffer, Allocation, u64), Error> {
        let (buffer, allocation, _) = self.accel_buffer(
            name,
            bytes,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
        )?;
        let info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(buffer)
            .offset(0)
            .size(bytes)
            .ty(ty);
        let rt = self.rt()?;
        // SAFETY: the buffer is live and large enough; the structure is destroyed with it.
        let made = unsafe { rt.loader.create_acceleration_structure(&info, None) };
        let accel = match made {
            Ok(a) => a,
            Err(e) => {
                self.free_raw(buffer, Some(allocation));
                return Err(Error::Vulkan {
                    what: "vkCreateAccelerationStructureKHR",
                    result: e,
                });
            }
        };
        let info =
            vk::AccelerationStructureDeviceAddressInfoKHR::default().acceleration_structure(accel);
        // SAFETY: the structure is live.
        let address = unsafe { rt.loader.get_acceleration_structure_device_address(&info) };
        Ok((accel, buffer, allocation, address))
    }

    pub(crate) fn free_raw(&mut self, buffer: vk::Buffer, allocation: Option<Allocation>) {
        self.release_allocation(allocation);
        // SAFETY: the work that used it is done, or it was never used.
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    /// Make a bottom-level structure of `geometries`, built at the head of the next recording.
    /// Their buffers must be made for acceleration inputs and live until then.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray queries; [`Error::Invalid`] for no geometry; any
    /// failure of the driver or the allocator.
    pub fn create_blas(&mut self, label: &str, geometries: &[BlasGeometry]) -> Result<Blas, Error> {
        self.rt()?;
        if geometries.is_empty() {
            return Err(Error::Invalid(
                "a bottom-level structure of no geometry".into(),
            ));
        }
        let descs: Vec<_> = geometries.iter().map(|g| triangles(g, 0, 0)).collect();
        let counts: Vec<u32> = geometries.iter().map(|g| g.triangles).collect();
        let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(&descs);
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        // SAFETY: a pure query; the geometry descriptions are live locals.
        unsafe {
            self.rt()?.loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &counts,
                &mut sizes,
            );
        }
        let (accel, buffer, allocation, address) = self.make_accel(
            label,
            sizes.acceleration_structure_size,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        )?;
        let bytes = sizes.acceleration_structure_size;
        let rt = self.rt_mut()?;
        let id = rt.accels.insert(AccelEntry {
            accel,
            buffer,
            allocation: Some(allocation),
            address,
            bytes,
            kind: AccelKind::Bottom {
                geometries: geometries.to_vec(),
                scratch: sizes.build_scratch_size,
                built: false,
            },
        });
        rt.pending.push(id);
        self.stats.acceleration_structures += 1;
        self.stats.acceleration_bytes += bytes;
        Ok(Blas(id))
    }

    /// Destroy a bottom-level structure once the work recorded so far is done with it.
    pub fn destroy_blas(&mut self, b: Blas) {
        self.destroy_accel(b.0);
    }

    /// Destroy a top-level structure once the work recorded so far is done with it.
    pub fn destroy_tlas(&mut self, t: Tlas) {
        self.destroy_accel(t.0);
    }

    fn destroy_accel(&mut self, id: u32) {
        let Some(rt) = self.rt.as_mut() else {
            return;
        };
        rt.pending.retain(|p| *p != id);
        let Some(e) = rt.accels.remove(id) else {
            return;
        };
        self.stats.acceleration_structures -= 1;
        self.stats.acceleration_bytes -= e.bytes;
        if let AccelKind::Top {
            bindless: Some(i), ..
        } = e.kind
        {
            self.defer(Garbage::AccelIndex(i));
        }
        self.defer(Garbage::Accel(e));
    }

    pub(crate) fn destroy_accel_now(&mut self, e: AccelEntry) {
        if let Some(rt) = self.rt.as_ref() {
            // SAFETY: the work that used it is done.
            unsafe { rt.loader.destroy_acceleration_structure(e.accel, None) };
        }
        self.free_raw(e.buffer, e.allocation);
        if let AccelKind::Top {
            instances, scratch, ..
        } = e.kind
        {
            self.free_raw(instances.0, instances.1);
            self.free_raw(scratch.0, scratch.1);
        }
    }

    /// Whether `b` has been built.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray queries; [`Error::Invalid`] for one the device does
    /// not hold.
    pub fn blas_built(&self, b: Blas) -> Result<bool, Error> {
        match self.rt()?.accels.get(b.0).map(|e| &e.kind) {
            Some(AccelKind::Bottom { built, .. }) => Ok(*built),
            _ => Err(Error::Invalid(format!("no bottom-level structure {}", b.0))),
        }
    }

    /// The address a top-level instance refers to `b` by, for instances a shader writes.
    ///
    /// # Errors
    /// As [`Self::blas_built`].
    pub fn blas_address(&self, b: Blas) -> Result<u64, Error> {
        match self.rt()?.accels.get(b.0) {
            Some(e) if matches!(e.kind, AccelKind::Bottom { .. }) => Ok(e.address),
            _ => Err(Error::Invalid(format!("no bottom-level structure {}", b.0))),
        }
    }

    /// Make a top-level structure that holds up to `capacity` instances; a build of more grows
    /// it, and gives it a new index in the table.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray queries; any failure of the driver or the allocator.
    pub fn create_tlas(&mut self, label: &str, capacity: u32) -> Result<Tlas, Error> {
        self.rt()?;
        let kind = self.top_parts(label, capacity.max(1))?;
        let (accel, buffer, allocation, address, bytes, kind) = kind;
        let rt = self.rt_mut()?;
        let id = rt.accels.insert(AccelEntry {
            accel,
            buffer,
            allocation: Some(allocation),
            address,
            bytes,
            kind,
        });
        self.stats.acceleration_structures += 1;
        self.stats.acceleration_bytes += bytes;
        Ok(Tlas(id))
    }

    /// The parts of a top-level structure for `capacity` instances.
    #[allow(clippy::type_complexity)]
    fn top_parts(
        &mut self,
        label: &str,
        capacity: u32,
    ) -> Result<
        (
            vk::AccelerationStructureKHR,
            vk::Buffer,
            Allocation,
            u64,
            u64,
            AccelKind,
        ),
        Error,
    > {
        let geometry = [instances(0)];
        let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(&geometry);
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        // SAFETY: a pure query on live locals.
        unsafe {
            self.rt()?.loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &[capacity],
                &mut sizes,
            );
        }
        let (accel, buffer, allocation, address) = self.make_accel(
            label,
            sizes.acceleration_structure_size,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        )?;
        let align = self.rt()?.scratch_align;
        let instances = self.accel_buffer(
            "top-level instances",
            u64::from(capacity) * INSTANCE_BYTES,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::TRANSFER_DST,
        );
        let instances = match instances {
            Ok((b, a, _)) => (b, Some(a), u64::from(capacity) * INSTANCE_BYTES),
            Err(e) => {
                self.free_accel_parts(accel, buffer, allocation);
                return Err(e);
            }
        };
        let scratch = self.accel_buffer(
            "top-level scratch",
            sizes.build_scratch_size + align,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        );
        let scratch = match scratch {
            Ok((b, a, address)) => (b, Some(a), address.next_multiple_of(align)),
            Err(e) => {
                self.free_raw(instances.0, instances.1);
                self.free_accel_parts(accel, buffer, allocation);
                return Err(e);
            }
        };
        Ok((
            accel,
            buffer,
            allocation,
            address,
            sizes.acceleration_structure_size,
            AccelKind::Top {
                capacity,
                instances,
                scratch,
                bindless: None,
            },
        ))
    }

    fn free_accel_parts(
        &mut self,
        accel: vk::AccelerationStructureKHR,
        buffer: vk::Buffer,
        allocation: Allocation,
    ) {
        if let Some(rt) = self.rt.as_ref() {
            // SAFETY: never used.
            unsafe { rt.loader.destroy_acceleration_structure(accel, None) };
        }
        self.free_raw(buffer, Some(allocation));
    }

    /// The index shaders find `t` at in the table's `accels[]`, given one on first asking. A
    /// build that grows `t` gives it a new one, so ask after each build.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray queries; [`Error::Invalid`] for one the device does
    /// not hold; [`Error::Full`] when the table's are all taken.
    pub fn bind_tlas(&mut self, t: Tlas) -> Result<u32, Error> {
        let set = self.bindless.set;
        let rt = self.rt_mut()?;
        let e = rt
            .accels
            .get(t.0)
            .ok_or_else(|| Error::Invalid(format!("no top-level structure {}", t.0)))?;
        let AccelKind::Top { bindless, .. } = e.kind else {
            return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
        };
        if let Some(i) = bindless {
            return Ok(i);
        }
        let accel = e.accel;
        let index = rt
            .indices
            .take()
            .ok_or(Error::Full("acceleration structures"))?;
        let accels = [accel];
        let mut write_accel = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accels);
        let mut write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(4)
            .dst_array_element(index)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .push_next(&mut write_accel);
        write.descriptor_count = 1;
        // SAFETY: the binding is update-after-bind and partially bound; no pending work reads
        // this index, which no live structure held.
        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
        if let Some(AccelEntry {
            kind: AccelKind::Top { bindless, .. },
            ..
        }) = self.rt.as_mut().and_then(|rt| rt.accels.get_mut(t.0))
        {
            *bindless = Some(index);
        }
        Ok(index)
    }

    /// Build every bottom-level structure waiting, into `cmd`, after the staged copies; those
    /// whose buffers are gone are left unbuilt.
    pub(crate) fn build_pending(&mut self, cmd: vk::CommandBuffer, at: crate::device::Retire) {
        let Some(rt) = self.rt.as_mut() else {
            return;
        };
        if rt.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut rt.pending);
        let align = rt.scratch_align;
        // Each build's geometries, with their buffers' addresses, and its scratch's place.
        let mut builds: Vec<(
            u32,
            Vec<vk::AccelerationStructureGeometryKHR<'static>>,
            Vec<vk::AccelerationStructureBuildRangeInfoKHR>,
            u64,
        )> = Vec::new();
        let mut scratch_total = 0u64;
        for id in pending {
            let Some(e) = self.rt.as_ref().and_then(|rt| rt.accels.get(id)) else {
                continue;
            };
            let AccelKind::Bottom {
                geometries,
                scratch,
                ..
            } = &e.kind
            else {
                continue;
            };
            let (geometries, scratch) = (geometries.clone(), *scratch);
            let mut descs = Vec::with_capacity(geometries.len());
            let mut ranges = Vec::with_capacity(geometries.len());
            let mut ok = true;
            for g in &geometries {
                match (
                    self.buffer_address(g.vertices),
                    self.buffer_address(g.indices),
                ) {
                    (Ok(v), Ok(i)) => {
                        descs.push(triangles(g, v, i));
                        ranges.push(vk::AccelerationStructureBuildRangeInfoKHR {
                            primitive_count: g.triangles,
                            primitive_offset: 0,
                            first_vertex: 0,
                            transform_offset: 0,
                        });
                    }
                    _ => ok = false,
                }
            }
            if !ok {
                tracing::warn!(target: "dereth::gfx", "a bottom-level structure's buffers are gone; it is not built");
                continue;
            }
            let offset = scratch_total.next_multiple_of(align);
            scratch_total = offset + scratch;
            builds.push((id, descs, ranges, offset));
        }
        if builds.is_empty() {
            return;
        }
        let scratch = match self.accel_buffer(
            "bottom-level scratch",
            scratch_total + align,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(target: "dereth::gfx", "no scratch for the bottom-level builds: {e}");
                return;
            }
        };
        let base = scratch.2.next_multiple_of(align);
        let infos: Vec<_> = builds
            .iter()
            .filter_map(|(id, descs, _, offset)| {
                let e = self.rt.as_ref()?.accels.get(*id)?;
                Some(
                    vk::AccelerationStructureBuildGeometryInfoKHR::default()
                        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                        .dst_acceleration_structure(e.accel)
                        .geometries(descs)
                        .scratch_data(vk::DeviceOrHostAddressKHR {
                            device_address: base + offset,
                        }),
                )
            })
            .collect();
        let ranges: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> =
            builds.iter().map(|b| b.2.as_slice()).collect();
        before_builds(&self.device, cmd);
        if let Some(rt) = self.rt.as_ref() {
            // SAFETY: every structure, buffer and address is live; the scratch ranges do not
            // overlap; recording outside any rendering.
            unsafe {
                rt.loader
                    .cmd_build_acceleration_structures(cmd, &infos, &ranges)
            };
        }
        after_builds(&self.device, cmd);
        for (id, ..) in &builds {
            if let Some(AccelEntry {
                kind: AccelKind::Bottom { built, .. },
                ..
            }) = self.rt.as_mut().and_then(|rt| rt.accels.get_mut(*id))
            {
                *built = true;
            }
        }
        self.garbage
            .push((at, Garbage::Raw(scratch.0, Some(scratch.1))));
    }
}

/// Every earlier use of what a build writes or reads (the structures, their scratch, their
/// inputs) is done before it.
fn before_builds(device: &ash::Device, cmd: vk::CommandBuffer) {
    let before = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_WRITE | vk::AccessFlags2::MEMORY_READ)
        .dst_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
        .dst_access_mask(
            vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR
                | vk::AccessFlags2::SHADER_READ,
        )];
    let dep = vk::DependencyInfo::default().memory_barriers(&before);
    // SAFETY: recording outside any rendering.
    unsafe { device.cmd_pipeline_barrier2(cmd, &dep) };
}

/// What a build wrote is visible to every later build and shader.
fn after_builds(device: &ash::Device, cmd: vk::CommandBuffer) {
    let after = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
        .src_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(
            vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags2::SHADER_READ,
        )];
    let dep = vk::DependencyInfo::default().memory_barriers(&after);
    // SAFETY: recording outside any rendering.
    unsafe { device.cmd_pipeline_barrier2(cmd, &dep) };
}

impl Recorder<'_> {
    /// Build `t` from `instances`, now: their bytes are staged and copied into its own buffer
    /// first. Every structure an instance places must have been built, or be built at this
    /// recording's head.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray queries; [`Error::Invalid`] for a structure the device
    /// does not hold; any failure to stage or to grow.
    pub fn build_tlas(&mut self, t: Tlas, instances: &[TlasInstance]) -> Result<(), Error> {
        let mut bytes = Vec::with_capacity(instances.len() * INSTANCE_BYTES as usize);
        {
            let rt = self.device().rt()?;
            for i in instances {
                let e = rt
                    .accels
                    .get(i.blas.0)
                    .filter(|e| matches!(e.kind, AccelKind::Bottom { .. }))
                    .ok_or_else(|| {
                        Error::Invalid(format!("no bottom-level structure {}", i.blas.0))
                    })?;
                bytes.extend_from_slice(&instance_bytes(i, e.address));
            }
        }
        #[allow(clippy::cast_possible_truncation)] // far fewer than 2^32 instances
        let count = instances.len() as u32;
        self.grow_tlas(t, count)?;
        let (dst, _) = self.tlas_instances(t)?;
        if !bytes.is_empty() {
            let (src, src_offset) = self.device().stage(&bytes)?;
            let cmd = self.raw();
            let region = vk::BufferCopy {
                src_offset,
                dst_offset: 0,
                size: bytes.len() as u64,
            };
            // The copy waits for the last build's read of the instances.
            before_builds_copy(&self.device().device, cmd);
            // SAFETY: both buffers are live; the range is inside each.
            unsafe {
                self.device()
                    .device
                    .cmd_copy_buffer(cmd, src, dst, &[region])
            };
        }
        self.record_tlas_build(t, count)
    }

    /// Build `t` from `count` instances a shader wrote into `buffer` from byte `offset`, made for
    /// acceleration inputs; each 64 bytes as [`instance_bytes`] lays them out, with
    /// [`Device::blas_address`] as the reference. The writes must be visible to the build.
    ///
    /// # Errors
    /// As [`Self::build_tlas`]; [`Error::Invalid`] for a range past the buffer's end.
    pub fn build_tlas_from(
        &mut self,
        t: Tlas,
        buffer: Buffer,
        offset: u64,
        count: u32,
    ) -> Result<(), Error> {
        let size = self.device().buffer_size(buffer)?;
        if offset + u64::from(count) * INSTANCE_BYTES > size {
            return Err(Error::Invalid("instances past the buffer's end".into()));
        }
        let address = self.device().buffer_address(buffer)? + offset;
        self.grow_tlas(t, count)?;
        self.record_build_at(t, count, address)
    }

    /// Grow `t` to hold `count` instances, when it holds fewer.
    fn grow_tlas(&mut self, t: Tlas, count: u32) -> Result<(), Error> {
        let capacity = match self.device().rt()?.accels.get(t.0).map(|e| &e.kind) {
            Some(AccelKind::Top { capacity, .. }) => *capacity,
            _ => return Err(Error::Invalid(format!("no top-level structure {}", t.0))),
        };
        if count <= capacity {
            return Ok(());
        }
        let dev = self.device();
        let parts = dev.top_parts("top-level structure", count.next_power_of_two())?;
        let (accel, buffer, allocation, address, bytes, kind) = parts;
        let rt = dev.rt_mut()?;
        let Some(e) = rt.accels.get_mut(t.0) else {
            return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
        };
        let old = std::mem::replace(
            e,
            AccelEntry {
                accel,
                buffer,
                allocation: Some(allocation),
                address,
                bytes,
                kind,
            },
        );
        dev.stats.acceleration_bytes = dev.stats.acceleration_bytes - old.bytes + bytes;
        if let AccelKind::Top {
            bindless: Some(i), ..
        } = old.kind
        {
            dev.defer(Garbage::AccelIndex(i));
        }
        dev.defer(Garbage::Accel(old));
        Ok(())
    }

    fn tlas_instances(&mut self, t: Tlas) -> Result<(vk::Buffer, u64), Error> {
        match self.device().rt()?.accels.get(t.0).map(|e| &e.kind) {
            Some(AccelKind::Top { instances, .. }) => Ok((instances.0, instances.2)),
            _ => Err(Error::Invalid(format!("no top-level structure {}", t.0))),
        }
    }

    fn record_tlas_build(&mut self, t: Tlas, count: u32) -> Result<(), Error> {
        let (buffer, _) = self.tlas_instances(t)?;
        let info = vk::BufferDeviceAddressInfo::default().buffer(buffer);
        // SAFETY: the buffer is live and made with the device-address use.
        let address = unsafe { self.device().device.get_buffer_device_address(&info) };
        self.record_build_at(t, count, address)
    }

    fn record_build_at(&mut self, t: Tlas, count: u32, address: u64) -> Result<(), Error> {
        let cmd = self.raw();
        let dev = self.device();
        let rt = dev.rt()?;
        let e = rt
            .accels
            .get(t.0)
            .ok_or_else(|| Error::Invalid(format!("no top-level structure {}", t.0)))?;
        let AccelKind::Top { scratch, .. } = &e.kind else {
            return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
        };
        let geometry = [instances(address)];
        let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(e.accel)
            .geometries(&geometry)
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch.2,
            });
        let range = [vk::AccelerationStructureBuildRangeInfoKHR {
            primitive_count: count,
            primitive_offset: 0,
            first_vertex: 0,
            transform_offset: 0,
        }];
        before_builds(&dev.device, cmd);
        // SAFETY: the structure, its scratch and its instances are live; recording outside any
        // rendering.
        unsafe {
            rt.loader
                .cmd_build_acceleration_structures(cmd, &[info], &[&range]);
        }
        after_builds(&dev.device, cmd);
        Ok(())
    }
}

/// A copy into a top-level structure's instances waits for every earlier read of them.
fn before_builds_copy(device: &ash::Device, cmd: vk::CommandBuffer) {
    let before = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)];
    let dep = vk::DependencyInfo::default().memory_barriers(&before);
    // SAFETY: recording outside any rendering.
    unsafe { device.cmd_pipeline_barrier2(cmd, &dep) };
}

/// Whether the adapter has everything ray queries need, by its extensions and features.
pub(crate) fn adapter_traces(instance: &ash::Instance, pd: vk::PhysicalDevice) -> bool {
    // SAFETY: a pure query on a live physical device.
    let Ok(available) = (unsafe { instance.enumerate_device_extension_properties(pd) }) else {
        return false;
    };
    let has = |name: &std::ffi::CStr| {
        available
            .iter()
            .any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == name))
    };
    if !EXTENSIONS.iter().all(|n| has(n)) {
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

/// The device extensions ray queries need.
pub const EXTENSIONS: [&std::ffi::CStr; 3] = [
    ash::khr::acceleration_structure::NAME,
    ash::khr::ray_query::NAME,
    ash::khr::deferred_host_operations::NAME,
];

/// The alignment `pd` asks of a build's scratch memory.
pub(crate) fn scratch_alignment(instance: &ash::Instance, pd: vk::PhysicalDevice) -> u64 {
    let mut props = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
    let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut props);
    // SAFETY: the chain is live locals for the call.
    unsafe { instance.get_physical_device_properties2(pd, &mut p2) };
    u64::from(
        props
            .min_acceleration_structure_scratch_offset_alignment
            .max(1),
    )
}
