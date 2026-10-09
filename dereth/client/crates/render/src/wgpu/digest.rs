//! Test builds: one number for what a frame recorded, so two builds, or one build under two
//! settings, can be shown to have recorded exactly the same frame without reading a pixel.
//!
//! The digest covers every recorded step in order: each viewport and depth-clear rectangle;
//! each bind, by the texture slot, sampler pair or landscape-layer set it binds; each draw, by
//! its pipeline's key, its vertex range and count and its two constant offsets; and then every
//! vertex and constant byte the frame recorded. It is a 64-bit FNV-1a hash, the same on every
//! machine and in every run.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::{Cmd, Gpu};
use crate::PipelineKey;

/// FNV-1a, 64 bits.
struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }
}

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
}

/// What a recorded bind binds, as far as the device can tell.
#[derive(Hash)]
enum Bound {
    /// The one-texel white texture.
    White,
    /// A resident texture, by slot.
    Texture(u32),
    /// A sampler pair, by descriptor index.
    Samplers(u32, u32),
    /// A landscape-layer set, by the slots, samplers and constants it was made from.
    Layers(u64),
    /// Something no table names any more: a texture released since it was bound.
    Unknown,
}

impl Gpu {
    /// Start or stop digesting each frame's recording.
    pub fn set_frame_digest(&mut self, on: bool) {
        self.digest_frames = on;
        if !on {
            self.last_digest = None;
        }
    }

    /// The digest of the last ended frame's recording, while digesting is on.
    #[must_use]
    pub fn last_frame_digest(&self) -> Option<u64> {
        self.last_digest
    }

    /// How many render passes this device has encoded.
    #[must_use]
    pub fn passes_encoded(&self) -> u64 {
        self.passes_encoded
    }

    /// The objects this device holds by its own count, after waiting for its work so that what
    /// was dropped is gone.
    pub fn device_objects(&mut self) -> crate::device::DeviceObjects {
        let _ = self.wait_idle();
        let c = self.device.get_internal_counters().hal;
        crate::device::DeviceObjects {
            buffers: c.buffers.read(),
            buffer_memory: c.buffer_memory.read(),
            texture_memory: c.texture_memory.read(),
            memory_allocations: c.memory_allocations.read(),
            texture_views: c.texture_views.read(),
            bind_groups: c.bind_groups.read(),
            bind_group_layouts: c.bind_group_layouts.read(),
            render_pipelines: c.render_pipelines.read(),
            compute_pipelines: c.compute_pipelines.read(),
            pipeline_layouts: c.pipeline_layouts.read(),
            samplers: c.samplers.read(),
            shader_modules: c.shader_modules.read(),
            query_sets: c.query_sets.read(),
            acceleration_structure_memory: c.acceleration_structure_memory.read(),
        }
    }

    /// The digest of `commands` and the frame's two arenas.
    // Bind groups are keyed by identity, which their interior state never changes.
    #[allow(clippy::mutable_key_type)]
    pub(super) fn frame_digest(&self, commands: &[Cmd]) -> u64 {
        let mut keys: HashMap<usize, (PipelineKey, bool)> = self
            .pipeline_index
            .iter()
            .map(|(k, i)| (*i, (*k, false)))
            .collect();
        let mut layers: HashMap<&wgpu::BindGroup, u64> = HashMap::new();
        if let Some(s) = &self.terrain_splat {
            keys.extend(s.pipelines.iter().map(|(k, i)| (*i, (*k, true))));
            for (key, group) in &s.groups {
                let mut h = Fnv::new();
                key.hash(&mut h);
                layers.insert(group, h.finish());
            }
        }
        let slots: HashMap<&wgpu::BindGroup, u32> =
            self.textures.iter().map(|(s, t)| (&t.bind, *s)).collect();
        let pairs: HashMap<wgpu::BindGroup, (u32, u32)> = self
            .sampler_pairs
            .borrow()
            .iter()
            .map(|(k, g)| (g.clone(), *k))
            .collect();
        let mut h = Fnv::new();
        for cmd in commands {
            match cmd {
                Cmd::Viewport(v) => {
                    0u8.hash(&mut h);
                    (v.x, v.y, v.width, v.height).hash(&mut h);
                }
                Cmd::ClearDepth(v) => {
                    1u8.hash(&mut h);
                    (v.x, v.y, v.width, v.height).hash(&mut h);
                }
                Cmd::Bind { group, bind } => {
                    2u8.hash(&mut h);
                    group.hash(&mut h);
                    let bound = if *bind == self.white {
                        Bound::White
                    } else if let Some(slot) = slots.get(bind) {
                        Bound::Texture(*slot)
                    } else if let Some((a, b)) = pairs.get(bind) {
                        Bound::Samplers(*a, *b)
                    } else if let Some(l) = layers.get(bind) {
                        Bound::Layers(*l)
                    } else {
                        Bound::Unknown
                    };
                    bound.hash(&mut h);
                }
                Cmd::Draw {
                    pipeline,
                    vertices,
                    count,
                    frame,
                    draw,
                } => {
                    3u8.hash(&mut h);
                    pipeline.hash(&mut h);
                    keys.get(pipeline).hash(&mut h);
                    (vertices.start, vertices.end, count, frame, draw).hash(&mut h);
                }
            }
        }
        h.write(&self.vertex_arena);
        h.write(&self.uniform_arena);
        h.finish()
    }
}
