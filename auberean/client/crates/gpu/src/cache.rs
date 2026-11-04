//! The meshes' buffers kept on the device between frames: a batch whose vertices and indices are
//! the very ones an earlier frame drew (the same shared lists) draws from the buffers made then. A
//! buffer no frame has drawn for a few frames is let go.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_draw::mesh::MeshVertex;
use wgpu::util::DeviceExt;

use super::MeshGpuVertex;

/// How many frames a mesh's buffers are kept after the last frame that drew them.
const KEEP_FRAMES: u64 = 3;

struct Entry {
    /// The lists themselves, held so their addresses stay theirs while they key the buffers.
    _vertices: Arc<Vec<MeshVertex>>,
    _indices: Arc<Vec<u16>>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    used: u64,
}

/// The kept buffers, by the lists they were made from.
#[derive(Default)]
pub(crate) struct MeshCache {
    entries: HashMap<(usize, usize), Entry>,
    frame: u64,
    /// Lists asked for, and lists put on the device, since the last frame's end.
    asked: u32,
    misses: u32,
}

impl MeshCache {
    /// The vertex and index buffers of these lists, made the first time they are drawn.
    pub(crate) fn buffers(
        &mut self,
        device: &wgpu::Device,
        vertices: &Arc<Vec<MeshVertex>>,
        indices: &Arc<Vec<u16>>,
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        let key = (
            Arc::as_ptr(vertices) as usize,
            Arc::as_ptr(indices) as usize,
        );
        let frame = self.frame;
        self.asked += 1;
        let misses = &mut self.misses;
        let e = self.entries.entry(key).or_insert_with(|| {
            *misses += 1;
            let verts: Vec<MeshGpuVertex> = vertices.iter().map(MeshGpuVertex::of).collect();
            let mut idx: Vec<u16> = indices.to_vec();
            if idx.len() % 2 == 1 {
                idx.push(0);
            }
            Entry {
                _vertices: Arc::clone(vertices),
                _indices: Arc::clone(indices),
                vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                used: frame,
            }
        });
        e.used = frame;
        (e.vertex_buffer.clone(), e.index_buffer.clone())
    }

    /// A frame is drawn: what no recent frame drew is let go. How many lists it found on the
    /// device and how many it had to put there.
    pub(crate) fn end_frame(&mut self) -> (u32, u32) {
        let frame = self.frame;
        self.entries
            .retain(|_, e| frame.saturating_sub(e.used) < KEEP_FRAMES);
        self.frame += 1;
        let counts = (self.asked - self.misses, self.misses);
        self.asked = 0;
        self.misses = 0;
        counts
    }
}
