//! Acceleration structures on Direct3D 12, where the device has inline ray tracing: the same
//! calls as on Vulkan ([`crate::rt`]). A structure's instances are read from the staging memory or
//! from a buffer a shader wrote; the layout of an instance is the same on both backends.

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::recorder::{put, step};
use super::resource::{buffer_desc, default_heap};
use super::{hr, Device, Garbage, Retire, ACCELS_AT};
use crate::bindless::Indices;
use crate::resource::Slots;
use crate::rt::{instance_bytes, Blas, BlasGeometry, Tlas, TlasInstance, ACCELS, INSTANCE_BYTES};
use crate::{Buffer, Error, IndexType};

/// What the device keeps for ray tracing.
pub(crate) struct Rt {
    device: ID3D12Device5,
    accels: Slots<Entry>,
    /// The bottom-level structures waiting for the next recording's head.
    pending: Vec<u32>,
    indices: Indices,
}

/// A live acceleration structure and its memory.
struct Entry {
    resource: ID3D12Resource,
    bytes: u64,
    kind: Kind,
}

enum Kind {
    /// Its geometries, the scratch its build needs and whether it has been built.
    Bottom {
        geometries: Vec<BlasGeometry>,
        scratch: u64,
        built: bool,
    },
    /// How many instances it holds at most, its scratch, and its index in the table.
    Top {
        capacity: u32,
        scratch: ID3D12Resource,
        bindless: Option<u32>,
    },
}

impl Rt {
    /// A free table index goes back.
    pub(super) fn release_index(&mut self, i: u32) {
        self.indices.release(i);
    }
}

/// What ray tracing keeps on `device`, when it has inline ray tracing.
pub(super) fn support(device: &ID3D12Device) -> Option<Rt> {
    let mut o = D3D12_FEATURE_DATA_D3D12_OPTIONS5::default();
    // SAFETY: the struct is the feature's own, sized as the call says.
    let ok = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS5,
            std::ptr::from_mut(&mut o).cast(),
            u32::try_from(std::mem::size_of_val(&o)).unwrap_or(0),
        )
    };
    if ok.is_err() || o.RaytracingTier.0 < D3D12_RAYTRACING_TIER_1_1.0 {
        return None;
    }
    let device = device.cast::<ID3D12Device5>().ok()?;
    Some(Rt {
        device,
        accels: Slots::default(),
        pending: Vec::new(),
        indices: Indices::new(ACCELS),
    })
}

/// A committed buffer in the device's own memory, in `state`, that a build may write.
fn committed(
    device: &ID3D12Device5,
    size: u64,
    state: D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource, Error> {
    let props = default_heap();
    let desc = buffer_desc(size, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    let mut resource: Option<ID3D12Resource> = None;
    // SAFETY: live locals; the resource is owned by the caller.
    hr("CreateCommittedResource", unsafe {
        device.CreateCommittedResource(
            &props,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            state,
            None,
            &mut resource,
        )
    })?;
    resource.ok_or_else(|| Error::Allocation("the runtime made no acceleration structure".into()))
}

/// The geometry description of `g`, with its buffers at `vertices` and `indices`.
fn triangles(g: &BlasGeometry, vertices: u64, indices: u64) -> D3D12_RAYTRACING_GEOMETRY_DESC {
    D3D12_RAYTRACING_GEOMETRY_DESC {
        Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
        Flags: if g.opaque {
            D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE
        } else {
            D3D12_RAYTRACING_GEOMETRY_FLAG_NONE
        },
        Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
            Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                Transform3x4: 0,
                IndexFormat: match g.index_type {
                    IndexType::U16 => DXGI_FORMAT_R16_UINT,
                    IndexType::U32 => DXGI_FORMAT_R32_UINT,
                },
                VertexFormat: DXGI_FORMAT_R32G32B32_FLOAT,
                IndexCount: g.triangles * 3,
                VertexCount: g.vertex_count,
                IndexBuffer: indices + g.index_offset,
                VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
                    StartAddress: vertices + g.vertex_offset,
                    StrideInBytes: g.vertex_stride,
                },
            },
        },
    }
}

/// What a bottom-level build of `descs` is told.
fn bottom_inputs(
    descs: &[D3D12_RAYTRACING_GEOMETRY_DESC],
) -> D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
    D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
        Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL,
        Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
        NumDescs: u32::try_from(descs.len()).unwrap_or(u32::MAX),
        DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
        Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
            pGeometryDescs: descs.as_ptr(),
        },
    }
}

/// What a top-level build of `count` instances at `address` is told.
fn top_inputs(count: u32, address: u64) -> D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
    D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
        Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
        Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
        NumDescs: count,
        DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
        Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
            InstanceDescs: address,
        },
    }
}

/// How much memory and scratch a build told `inputs` needs.
fn sizes(
    device: &ID3D12Device5,
    inputs: &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS,
) -> (u64, u64) {
    let mut info = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();
    // SAFETY: a pure query on live locals.
    unsafe { device.GetRaytracingAccelerationStructurePrebuildInfo(inputs, &mut info) };
    (info.ResultDataMaxSizeInBytes, info.ScratchDataSizeInBytes)
}

fn no_rays() -> Error {
    Error::Unsupported("this device does not trace rays".into())
}

/// A barrier between a build and everything after it that reads or writes what it wrote.
fn uav_all() -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                pResource: std::mem::ManuallyDrop::new(None),
            }),
        },
    }
}

impl Device {
    fn rt(&self) -> Result<&Rt, Error> {
        self.rt.as_ref().ok_or_else(no_rays)
    }

    fn rt_mut(&mut self) -> Result<&mut Rt, Error> {
        self.rt.as_mut().ok_or_else(no_rays)
    }

    fn buffer_address(&self, b: Buffer) -> Result<u64, Error> {
        // SAFETY: a pure query on a live buffer.
        Ok(unsafe { self.buffer(b)?.resource.GetGPUVirtualAddress() })
    }

    /// Make a bottom-level structure of `geometries`, built at the head of the next recording.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; [`Error::Invalid`] for no geometry; any
    /// failure of the runtime.
    pub fn create_blas(
        &mut self,
        _label: &str,
        geometries: &[BlasGeometry],
    ) -> Result<Blas, Error> {
        self.rt()?;
        if geometries.is_empty() {
            return Err(Error::Invalid(
                "a bottom-level structure of no geometry".into(),
            ));
        }
        let descs: Vec<_> = geometries.iter().map(|g| triangles(g, 0, 0)).collect();
        let rt = self.rt_mut()?;
        let (bytes, scratch) = sizes(&rt.device, &bottom_inputs(&descs));
        let resource = committed(
            &rt.device,
            bytes,
            D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE,
        )?;
        let id = rt.accels.insert(Entry {
            resource,
            bytes,
            kind: Kind::Bottom {
                geometries: geometries.to_vec(),
                scratch,
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
        if let Kind::Top {
            scratch, bindless, ..
        } = e.kind
        {
            self.defer(Garbage::Scratch(scratch));
            if let Some(i) = bindless {
                self.defer(Garbage::AccelIndex(i));
            }
        }
        self.defer(Garbage::Scratch(e.resource));
    }

    /// Whether a bottom-level structure has been built.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; [`Error::Invalid`] for one it does not hold.
    pub fn blas_built(&self, b: Blas) -> Result<bool, Error> {
        match self.rt()?.accels.get(b.0).map(|e| &e.kind) {
            Some(Kind::Bottom { built, .. }) => Ok(*built),
            _ => Err(Error::Invalid(format!("no bottom-level structure {}", b.0))),
        }
    }

    /// A bottom-level structure's address, as an instance refers to it.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; [`Error::Invalid`] for one it does not hold.
    pub fn blas_address(&self, b: Blas) -> Result<u64, Error> {
        let e = self
            .rt()?
            .accels
            .get(b.0)
            .filter(|e| matches!(e.kind, Kind::Bottom { .. }))
            .ok_or_else(|| Error::Invalid(format!("no bottom-level structure {}", b.0)))?;
        // SAFETY: a pure query on a live resource.
        Ok(unsafe { e.resource.GetGPUVirtualAddress() })
    }

    /// The memory of a top-level structure for `capacity` instances.
    fn top_parts(&self, capacity: u32) -> Result<(ID3D12Resource, ID3D12Resource, u64), Error> {
        let rt = self.rt()?;
        let (bytes, scratch) = sizes(&rt.device, &top_inputs(capacity, 0));
        let resource = committed(
            &rt.device,
            bytes,
            D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE,
        )?;
        let scratch = committed(&rt.device, scratch, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)?;
        Ok((resource, scratch, bytes))
    }

    /// Make a top-level structure with room for `capacity` instances.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; any failure of the runtime.
    pub fn create_tlas(&mut self, _label: &str, capacity: u32) -> Result<Tlas, Error> {
        let capacity = capacity.max(1);
        let (resource, scratch, bytes) = self.top_parts(capacity)?;
        let id = self.rt_mut()?.accels.insert(Entry {
            resource,
            bytes,
            kind: Kind::Top {
                capacity,
                scratch,
                bindless: None,
            },
        });
        self.stats.acceleration_structures += 1;
        self.stats.acceleration_bytes += bytes;
        Ok(Tlas(id))
    }

    /// The index shaders find `t` at, given one on first asking. A build that grows `t` gives it
    /// a new one, so ask after each build.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; [`Error::Invalid`] for one it does not hold;
    /// [`Error::Full`] when the table's are all taken.
    pub fn bind_tlas(&mut self, t: Tlas) -> Result<u32, Error> {
        let rt = self.rt_mut()?;
        let e = rt
            .accels
            .get(t.0)
            .ok_or_else(|| Error::Invalid(format!("no top-level structure {}", t.0)))?;
        let Kind::Top { bindless, .. } = e.kind else {
            return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
        };
        if let Some(i) = bindless {
            return Ok(i);
        }
        // SAFETY: a pure query on a live resource.
        let location = unsafe { e.resource.GetGPUVirtualAddress() };
        let index = rt
            .indices
            .take()
            .ok_or(Error::Full("acceleration structures"))?;
        if let Some(Entry {
            kind: Kind::Top { bindless, .. },
            ..
        }) = rt.accels.get_mut(t.0)
        {
            *bindless = Some(index);
        }
        let view = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                RaytracingAccelerationStructure: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_SRV {
                    Location: location,
                },
            },
        };
        // SAFETY: a slot of the device's own heap no frame in flight reads, which no live
        // structure held; the view names the structure by its address, so no resource is passed.
        unsafe {
            self.device.CreateShaderResourceView(
                None,
                Some(&view),
                self.heaps.resource(ACCELS_AT + index),
            );
        }
        Ok(index)
    }

    /// Build every bottom-level structure waiting, into `list`, after the staged copies; those
    /// whose buffers are gone are left unbuilt.
    pub(super) fn build_pending(&mut self, list: &ID3D12GraphicsCommandList, at: Retire) {
        let Some(rt) = self.rt.as_mut() else {
            return;
        };
        if rt.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut rt.pending);
        let Ok(list4) = list.cast::<ID3D12GraphicsCommandList4>() else {
            return;
        };
        let mut builds = Vec::new();
        let mut scratch_total = 0u64;
        let align = u64::from(D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BYTE_ALIGNMENT);
        let mut barriers = Vec::new();
        for id in pending {
            let Some(e) = self.rt.as_ref().and_then(|rt| rt.accels.get(id)) else {
                continue;
            };
            let Kind::Bottom {
                geometries,
                scratch,
                ..
            } = &e.kind
            else {
                continue;
            };
            let (geometries, scratch) = (geometries.clone(), *scratch);
            // SAFETY: a pure query on a live resource.
            let dest = unsafe { e.resource.GetGPUVirtualAddress() };
            let mut descs = Vec::with_capacity(geometries.len());
            let mut ok = true;
            for g in &geometries {
                match (
                    self.buffer_address(g.vertices),
                    self.buffer_address(g.indices),
                ) {
                    (Ok(v), Ok(i)) => descs.push(triangles(g, v, i)),
                    _ => ok = false,
                }
                for b in [g.vertices, g.indices] {
                    if let Some(e) = self.buffers.get_mut(b.0) {
                        barriers.extend(step(
                            &e.resource,
                            &mut e.state,
                            &mut e.uav_written,
                            D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                            false,
                        ));
                    }
                }
            }
            if !ok {
                tracing::warn!(
                    target: "dereth::gfx",
                    "a bottom-level structure's buffers are gone; it is not built"
                );
                continue;
            }
            let offset = scratch_total.next_multiple_of(align);
            scratch_total = offset + scratch;
            builds.push((id, descs, dest, offset));
        }
        put(list, &barriers);
        if builds.is_empty() {
            return;
        }
        let Some(device5) = self.rt.as_ref().map(|rt| rt.device.clone()) else {
            return;
        };
        let scratch = match committed(
            &device5,
            scratch_total,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(target: "dereth::gfx", "no scratch for the bottom-level builds: {e}");
                return;
            }
        };
        // SAFETY: a pure query on a live resource.
        let base = unsafe { scratch.GetGPUVirtualAddress() };
        for (id, descs, dest, offset) in &builds {
            let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                DestAccelerationStructureData: *dest,
                Inputs: bottom_inputs(descs),
                SourceAccelerationStructureData: 0,
                ScratchAccelerationStructureData: base + offset,
            };
            // SAFETY: every structure, buffer and address is live; the scratch ranges do not
            // overlap; the list is open, outside any pass.
            unsafe { list4.BuildRaytracingAccelerationStructure(&desc, None) };
            if let Some(Entry {
                kind: Kind::Bottom { built, .. },
                ..
            }) = self.rt.as_mut().and_then(|rt| rt.accels.get_mut(*id))
            {
                *built = true;
            }
        }
        // What the builds wrote is read by the top-level builds and the shaders after them.
        put(list, &[uav_all()]);
        self.garbage.push((at, Garbage::Scratch(scratch)));
    }

    /// Grow `t` to hold `count` instances, when it holds fewer.
    fn grow_tlas(&mut self, t: Tlas, count: u32) -> Result<(), Error> {
        let capacity = match self.rt()?.accels.get(t.0).map(|e| &e.kind) {
            Some(Kind::Top { capacity, .. }) => *capacity,
            _ => return Err(Error::Invalid(format!("no top-level structure {}", t.0))),
        };
        if count <= capacity {
            return Ok(());
        }
        let capacity = count.next_power_of_two();
        let (resource, scratch, bytes) = self.top_parts(capacity)?;
        let rt = self.rt_mut()?;
        let Some(e) = rt.accels.get_mut(t.0) else {
            return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
        };
        let old = std::mem::replace(
            e,
            Entry {
                resource,
                bytes,
                kind: Kind::Top {
                    capacity,
                    scratch,
                    bindless: None,
                },
            },
        );
        self.stats.acceleration_bytes = self.stats.acceleration_bytes - old.bytes + bytes;
        if let Kind::Top {
            scratch, bindless, ..
        } = old.kind
        {
            self.defer(Garbage::Scratch(scratch));
            if let Some(i) = bindless {
                self.defer(Garbage::AccelIndex(i));
            }
        }
        self.defer(Garbage::Scratch(old.resource));
        Ok(())
    }
}

impl super::Recorder<'_> {
    /// Build `t` from `instances`, now: their bytes are read from the staging memory. Every
    /// structure an instance places must have been built, or be built at this recording's head.
    ///
    /// # Errors
    /// [`Error::Unsupported`] without ray tracing; [`Error::Invalid`] for a structure the device
    /// does not hold; any failure to stage or to grow.
    pub fn build_tlas(&mut self, t: Tlas, instances: &[TlasInstance]) -> Result<(), Error> {
        let mut bytes = Vec::with_capacity(instances.len() * INSTANCE_BYTES as usize);
        for i in instances {
            bytes.extend_from_slice(&instance_bytes(i, self.device().blas_address(i.blas)?));
        }
        let count = u32::try_from(instances.len()).unwrap_or(u32::MAX);
        self.device().grow_tlas(t, count)?;
        let address = if bytes.is_empty() {
            0
        } else {
            let (src, offset) = self.device().stage(&bytes, 16)?;
            // SAFETY: a pure query on a live resource.
            unsafe { src.GetGPUVirtualAddress() + offset }
        };
        self.record_top_build(t, count, address)
    }

    /// Build `t` from `count` instances a shader wrote into `buffer` from byte `offset`, each laid
    /// out as [`instance_bytes`] lays them out, with [`Device::blas_address`] as the reference.
    /// The writes must be visible to the build.
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
        self.device().grow_tlas(t, count)?;
        let address = self.device().buffer_address(buffer)? + offset;
        let mut barriers = Vec::new();
        if let Some(e) = self.device().buffers.get_mut(buffer.0) {
            barriers.extend(step(
                &e.resource,
                &mut e.state,
                &mut e.uav_written,
                D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                false,
            ));
        }
        put(self.list(), &barriers);
        self.record_top_build(t, count, address)
    }

    fn record_top_build(&mut self, t: Tlas, count: u32, address: u64) -> Result<(), Error> {
        let (dest, scratch) = {
            let e = self
                .device()
                .rt()?
                .accels
                .get(t.0)
                .ok_or_else(|| Error::Invalid(format!("no top-level structure {}", t.0)))?;
            let Kind::Top { scratch, .. } = &e.kind else {
                return Err(Error::Invalid(format!("no top-level structure {}", t.0)));
            };
            // SAFETY: pure queries on live resources.
            unsafe {
                (
                    e.resource.GetGPUVirtualAddress(),
                    scratch.GetGPUVirtualAddress(),
                )
            }
        };
        let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            DestAccelerationStructureData: dest,
            Inputs: top_inputs(count, address),
            SourceAccelerationStructureData: 0,
            ScratchAccelerationStructureData: scratch,
        };
        let list = self.list().clone();
        let list4 = hr(
            "the list's ray tracing",
            list.cast::<ID3D12GraphicsCommandList4>(),
        )?;
        // The last build's use of the structure and the scratch is done before this one, and
        // what this one writes is read by the shaders after it.
        put(&list, &[uav_all()]);
        // SAFETY: every structure and address is live; the list is open, outside any pass.
        unsafe { list4.BuildRaytracingAccelerationStructure(&desc, None) };
        put(&list, &[uav_all()]);
        Ok(())
    }
}
