//! Where the draws' vertices and indices live on the device. A list seen for the first time is
//! taken for one frame's: it is copied into buffers kept between frames, written once before the
//! frame is submitted, and nothing is made for it. A list seen again in a later frame stays: it
//! gets buffers of its own, kept for as long as frames go on drawing it. So what is the same from
//! frame to frame (the world's meshes, the land) is uploaded once, and what changes every frame (a
//! moving body's vertices, the particles) costs a copy, never a buffer.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_draw::mesh::{MeshVertex, PosedVertex};
use auberean_draw::scene::TerrainVertex;
use bytemuck::Pod;

use super::{MeshGpuVertex, TerrainGpuVertex};

/// How many frames a list's own buffers are kept after the last frame that drew them, so that
/// what is out of sight a moment is not uploaded again as it comes back.
const KEEP_FRAMES: u64 = 240;

/// The size of each buffer one frame's lists are written into. A list larger than this has a
/// buffer of its size.
const CHUNK: usize = 4 << 20;

/// How much of bodies' lists the arena being filled holds before the other takes over.
const KEPT_BYTES: usize = 32 << 20;

/// A list's place on the device.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Slice {
    buffer: wgpu::Buffer,
    offset: u64,
    size: u64,
}

impl Slice {
    pub(crate) fn get(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(self.offset..self.offset + self.size)
    }
}

struct Chunk {
    buffer: wgpu::Buffer,
    data: Vec<u8>,
    used: usize,
    /// How much of what it holds the device has been given.
    written: usize,
}

/// One frame's lists, written into buffers kept between frames. A browser's WebGL 2 keeps a
/// buffer of indices apart from any other, so vertices and indices have an arena each.
pub(crate) struct Arena {
    usage: wgpu::BufferUsages,
    chunks: Vec<Chunk>,
    /// The chunk being filled.
    at: usize,
}

impl Arena {
    fn new(usage: wgpu::BufferUsages) -> Self {
        Self {
            usage: usage | wgpu::BufferUsages::COPY_DST,
            chunks: Vec::new(),
            at: 0,
        }
    }

    /// A frame starts: every chunk is free again. What earlier frames wrote has already been
    /// handed to the device in submission order, so overwriting it here is safe.
    fn begin(&mut self) {
        self.at = 0;
        for c in &mut self.chunks {
            c.used = 0;
            c.written = 0;
        }
    }

    /// Room for `bytes` with the rest of the frame's lists, holding them once the frame is
    /// written. Whatever it holds starts at four bytes' alignment, which any vertex or index asks
    /// for at most.
    pub(crate) fn put(&mut self, device: &wgpu::Device, bytes: &[u8]) -> Slice {
        let need = bytes.len().div_ceil(4) * 4;
        while self.at < self.chunks.len() {
            let c = &self.chunks[self.at];
            if c.used + need <= c.data.len() {
                break;
            }
            self.at += 1;
        }
        if self.at == self.chunks.len() {
            let size = need.max(CHUNK);
            self.chunks.push(Chunk {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("one frame's lists"),
                    size: size as u64,
                    usage: self.usage,
                    mapped_at_creation: false,
                }),
                data: vec![0; size],
                used: 0,
                written: 0,
            });
        }
        let c = &mut self.chunks[self.at];
        let at = c.used;
        c.data[at..at + bytes.len()].copy_from_slice(bytes);
        c.used = at + need;
        Slice {
            buffer: c.buffer.clone(),
            offset: at as u64,
            size: bytes.len() as u64,
        }
    }

    /// Write what it took since the last write to the device; the bytes written.
    fn write(&mut self, queue: &wgpu::Queue) -> u64 {
        let mut bytes = 0;
        for c in self.chunks.iter_mut().filter(|c| c.used > c.written) {
            queue.write_buffer(&c.buffer, c.written as u64, &c.data[c.written..c.used]);
            bytes += (c.used - c.written) as u64;
            c.written = c.used;
        }
        bytes
    }

    /// The bytes it holds.
    fn holds(&self) -> usize {
        self.chunks.iter().map(|c| c.used).sum()
    }
}

/// Where a list seen in recent frames is.
enum Place {
    /// In this frame's arena, for the draw it was first seen in.
    Passing(Slice, u64),
    /// In a buffer of its own.
    Own(Slice),
    /// In the kept arena of a generation.
    Kept(Slice, u64),
}

struct Entry<T> {
    /// The list itself, held so its address stays its own while it keys the entry.
    _list: Arc<Vec<T>>,
    place: Place,
    used: u64,
}

/// The lists of one kind, by their addresses.
struct Lists<T> {
    entries: HashMap<usize, Entry<T>>,
}

impl<T> Default for Lists<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

/// What a frame's lists have cost so far.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) hits: u32,
    pub(crate) misses: u32,
    pub(crate) buffers: u32,
    pub(crate) buffer_bytes: u64,
    pub(crate) written: u64,
}

/// The moment a list is asked for in: which frame, and which draw of it.
#[derive(Clone, Copy)]
struct Now {
    frame: u64,
    draw: u64,
}

/// A buffer of its own holding `bytes`. A device that has been lost makes no buffer to write them
/// into, and the frame goes on without them until the device is made again.
fn buffer_holding(device: &wgpu::Device, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        // Whole words, and never nothing, as a buffer mapped when it is made must be.
        size: (bytes.len() as u64)
            .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
            .max(wgpu::COPY_BUFFER_ALIGNMENT),
        usage,
        mapped_at_creation: true,
    });
    if let Ok(mut held) = buffer.get_mapped_range_mut(..) {
        held.slice(..bytes.len()).copy_from_slice(bytes);
    }
    buffer.unmap();
    buffer
}

impl<T> Lists<T> {
    /// Where `list` is on the device. `gpu` is what the device reads of it.
    fn slice<B: Pod>(
        &mut self,
        (device, arena, usage): (&wgpu::Device, &mut Arena, wgpu::BufferUsages),
        now: Now,
        counts: &mut Counts,
        list: &Arc<Vec<T>>,
        gpu: impl Fn(&[T]) -> Vec<B>,
    ) -> Slice {
        let key = Arc::as_ptr(list) as usize;
        let Some(e) = self.entries.get_mut(&key) else {
            counts.misses += 1;
            let slice = arena.put(device, bytemuck::cast_slice(&gpu(list)));
            self.entries.insert(
                key,
                Entry {
                    _list: Arc::clone(list),
                    place: Place::Passing(slice.clone(), now.draw),
                    used: now.frame,
                },
            );
            return slice;
        };
        e.used = now.frame;
        match &e.place {
            Place::Own(slice) => {
                counts.hits += 1;
                slice.clone()
            }
            Place::Passing(slice, seen) if *seen == now.draw => {
                counts.hits += 1;
                slice.clone()
            }
            Place::Passing(..) | Place::Kept(..) => {
                // Seen in an earlier draw: it stays.
                counts.misses += 1;
                let data = gpu(list);
                let bytes: &[u8] = bytemuck::cast_slice(&data);
                counts.buffers += 1;
                counts.buffer_bytes += bytes.len() as u64;
                let slice = Slice {
                    buffer: buffer_holding(device, bytes, usage),
                    offset: 0,
                    size: bytes.len() as u64,
                };
                e.place = Place::Own(slice.clone());
                slice
            }
        }
    }

    /// Where `list` is on the device when it lasts a few frames: written once into the kept arena,
    /// and found there for as long as its generation stands.
    fn kept(
        &mut self,
        (device, arena, generation): (&wgpu::Device, &mut Arena, u64),
        now: Now,
        counts: &mut Counts,
        list: &Arc<Vec<T>>,
    ) -> Slice
    where
        T: Pod,
    {
        let key = Arc::as_ptr(list) as usize;
        if let Some(Entry {
            place: Place::Kept(slice, of),
            used,
            ..
        }) = self.entries.get_mut(&key)
        {
            if *of + 1 >= generation {
                *used = now.frame;
                counts.hits += 1;
                return slice.clone();
            }
        }
        counts.misses += 1;
        let slice = arena.put(device, bytemuck::cast_slice(list.as_slice()));
        self.entries.insert(
            key,
            Entry {
                _list: Arc::clone(list),
                place: Place::Kept(slice.clone(), generation),
                used: now.frame,
            },
        );
        slice
    }

    /// A frame is over: lists its frames have not gone on drawing are let go.
    fn end_frame(&mut self, frame: u64) {
        self.entries.retain(|_, e| match e.place {
            Place::Passing(..) | Place::Kept(..) => e.used == frame,
            Place::Own(..) => frame.saturating_sub(e.used) < KEEP_FRAMES,
        });
    }
}

/// The lists on the device, and the two arenas of one frame's.
pub(crate) struct MeshCache {
    vertices: Lists<MeshVertex>,
    posed: Lists<PosedVertex>,
    indices: Lists<u16>,
    land: Lists<TerrainVertex>,
    land_indices: Lists<u32>,
    /// One draw's lists.
    pub(crate) passing_vertices: Arena,
    pub(crate) passing_indices: Arena,
    /// Bodies' lists, which a pose keeps for several frames. Each is filled in its turn and starts
    /// again only after the other has been filled since, so what a frame draws is never written
    /// over: a list stands for one generation after the one it was written in.
    kept: [Arena; 2],
    /// Which of them is being filled, and how many times they have changed places.
    kept_now: usize,
    generation: u64,
    frame: u64,
    draw: u64,
    counts: Counts,
}

impl Default for MeshCache {
    fn default() -> Self {
        Self {
            vertices: Lists::default(),
            posed: Lists::default(),
            indices: Lists::default(),
            land: Lists::default(),
            land_indices: Lists::default(),
            passing_vertices: Arena::new(wgpu::BufferUsages::VERTEX),
            passing_indices: Arena::new(wgpu::BufferUsages::INDEX),
            kept: [
                Arena::new(wgpu::BufferUsages::VERTEX),
                Arena::new(wgpu::BufferUsages::VERTEX),
            ],
            kept_now: 0,
            generation: 0,
            frame: 0,
            draw: 0,
            counts: Counts::default(),
        }
    }
}

impl MeshCache {
    /// A frame of the world starts being put together: the arenas are free again.
    pub(crate) fn begin_draw(&mut self) {
        self.passing_vertices.begin();
        self.passing_indices.begin();
        self.draw += 1;
    }

    fn now(&self) -> Now {
        Now {
            frame: self.frame,
            draw: self.draw,
        }
    }

    /// The vertex and index buffers of these lists.
    pub(crate) fn buffers(
        &mut self,
        device: &wgpu::Device,
        vertices: &Arc<Vec<MeshVertex>>,
        indices: &Arc<Vec<u16>>,
    ) -> (Slice, Slice) {
        let now = self.now();
        (
            self.vertices.slice(
                (
                    device,
                    &mut self.passing_vertices,
                    wgpu::BufferUsages::VERTEX,
                ),
                now,
                &mut self.counts,
                vertices,
                |l| l.iter().map(MeshGpuVertex::of).collect(),
            ),
            self.indices.slice(
                (device, &mut self.passing_indices, wgpu::BufferUsages::INDEX),
                now,
                &mut self.counts,
                indices,
                <[u16]>::to_vec,
            ),
        )
    }

    /// The buffer of a moved body's places and facings.
    pub(crate) fn posed_buffer(
        &mut self,
        device: &wgpu::Device,
        list: &Arc<Vec<PosedVertex>>,
    ) -> Slice {
        let now = self.now();
        self.posed.kept(
            (device, &mut self.kept[self.kept_now], self.generation),
            now,
            &mut self.counts,
            list,
        )
    }

    /// The vertex and index buffers of a piece of the land's lists.
    pub(crate) fn land_buffers(
        &mut self,
        device: &wgpu::Device,
        vertices: &Arc<Vec<TerrainVertex>>,
        indices: &Arc<Vec<u32>>,
    ) -> (Slice, Slice) {
        let now = self.now();
        (
            self.land.slice(
                (
                    device,
                    &mut self.passing_vertices,
                    wgpu::BufferUsages::VERTEX,
                ),
                now,
                &mut self.counts,
                vertices,
                |l| {
                    l.iter()
                        .map(|v| TerrainGpuVertex {
                            position: v.position,
                            color: v.color,
                            uv: v.uv,
                            mask_uv: v.mask_uv,
                        })
                        .collect()
                },
            ),
            self.land_indices.slice(
                (device, &mut self.passing_indices, wgpu::BufferUsages::INDEX),
                now,
                &mut self.counts,
                indices,
                <[u32]>::to_vec,
            ),
        )
    }

    /// Write this draw's lists to the device, before it is submitted.
    pub(crate) fn write(&mut self, queue: &wgpu::Queue) {
        self.counts.written += self.passing_vertices.write(queue)
            + self.passing_indices.write(queue)
            + self.kept[self.kept_now].write(queue);
    }

    /// What the lists have cost since the last call.
    pub(crate) fn take_counts(&mut self) -> Counts {
        std::mem::take(&mut self.counts)
    }

    /// A frame is shown: what its frames have stopped drawing is let go.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.vertices.end_frame(frame);
        self.posed.end_frame(frame);
        self.indices.end_frame(frame);
        self.land.end_frame(frame);
        self.land_indices.end_frame(frame);
        self.frame += 1;
        if self.kept[self.kept_now].holds() >= KEPT_BYTES {
            self.kept_now = 1 - self.kept_now;
            self.kept[self.kept_now].begin();
            self.generation += 1;
        }
    }
}
