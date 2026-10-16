//! The retained geometry of each resident block: its landscape triangles and static batches, every
//! detail level of their objects, held on the device once per block contents and replaced when the
//! block is rebuilt; the levels drawn are chosen by the block's objects' current levels.

use std::collections::HashMap;

use crate::snapshot::HifiLevelRun;

/// Which block contents are held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticFeed {
    generations: HashMap<u16, u32>,
}

impl StaticFeed {
    /// Whether `block` at `generation` still has to be sent.
    #[must_use]
    pub fn needs(&self, block: u16, generation: u32) -> bool {
        self.generations.get(&block) != Some(&generation)
    }

    /// Record that `block` at `generation` is held.
    pub fn hold(&mut self, block: u16, generation: u32) {
        self.generations.insert(block, generation);
    }

    /// Forget `block`, as when it leaves the resident window.
    pub fn drop_block(&mut self, block: u16) {
        self.generations.remove(&block);
    }
}

/// Where a pass's own copy of a block's retained geometry lies by detail level: runs of the copy's
/// vertices, each drawn at every distance or as one level of one object, so the vertices to draw
/// at the block's levels are listed without the copy being made again when a level changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LevelRuns {
    runs: Vec<HifiLevelRun>,
    vertices: u32,
}

impl LevelRuns {
    /// Append `count` vertices drawn at every distance.
    pub fn every(&mut self, count: u32) {
        if count > 0 {
            self.runs.push(HifiLevelRun {
                object: HifiLevelRun::EVERY,
                level: 0,
                start: self.vertices,
                end: self.vertices + count,
                placed: true,
            });
        }
        self.vertices += count;
    }

    /// Append a batch's first `count` vertices, which `runs` lay out: all of them drawn at every
    /// distance when it has none.
    pub fn batch(&mut self, runs: &[HifiLevelRun], count: u32) {
        if runs.is_empty() {
            self.every(count);
            return;
        }
        let base = self.vertices;
        for r in runs {
            let (start, end) = (r.start.min(count), r.end.min(count));
            if start < end {
                self.runs.push(HifiLevelRun {
                    start: base + start,
                    end: base + end,
                    ..*r
                });
            }
        }
        self.vertices += count;
    }

    /// How many vertices the copy holds.
    #[must_use]
    pub fn vertices(&self) -> u32 {
        self.vertices
    }

    /// The copy's vertices drawn while its block's objects stand at `levels`, in order: the list
    /// [`DeviceCopy::select`] hands the device.
    #[must_use]
    pub fn indices(&self, levels: &[u32]) -> Vec<u32> {
        let (bytes, _) = self.index_bytes(&self.choice(levels));
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect()
    }

    /// Which of the copy's runs are drawn while its block's objects stand at `levels`: the same
    /// whatever the levels of objects the copy does not hold.
    #[must_use]
    pub fn choice(&self, levels: &[u32]) -> Choice {
        let mut bits = vec![0u64; self.runs.len().div_ceil(64)];
        for (k, r) in self.runs.iter().enumerate() {
            if r.drawn(levels) {
                bits[k / 64] |= 1 << (k % 64);
            }
        }
        Choice(bits)
    }

    /// The indices of the vertices of the runs `choice` draws, in order, as the device reads
    /// them, and how many there are.
    fn index_bytes(&self, choice: &Choice) -> (Vec<u8>, u32) {
        let chosen = || {
            self.runs.iter().enumerate().filter_map(|(k, r)| {
                choice
                    .0
                    .get(k / 64)
                    .is_some_and(|w| w & (1 << (k % 64)) != 0)
                    .then_some(r)
            })
        };
        let count: u32 = chosen().map(|r| r.end - r.start).sum();
        let mut out = Vec::with_capacity(count as usize * 4);
        for r in chosen() {
            for i in r.start..r.end {
                out.extend_from_slice(&i.to_le_bytes());
            }
        }
        (out, count)
    }

    /// The copy's vertices of every object at its finest level, and of what is drawn at every
    /// distance, in order: the geometry whatever the levels, for what is not drawn again as they
    /// change.
    #[must_use]
    pub fn finest(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for r in self
            .runs
            .iter()
            .filter(|r| r.placed && (r.object == HifiLevelRun::EVERY || r.level == 0))
        {
            out.extend(r.start..r.end);
        }
        out
    }
}

/// Which runs of a copy are drawn, one bit a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice(Vec<u64>);

/// A pass's copy of a block's retained geometry on the device: its vertices, laid out by its
/// [`LevelRuns`], and the list of those drawn at the block's current detail levels, made again
/// when a level changes what it draws while the vertices stay where they are.
#[derive(Debug)]
pub struct DeviceCopy {
    /// The vertices.
    pub buffer: wgpu::Buffer,
    /// How the vertices lie by detail level.
    pub runs: LevelRuns,
    /// The indices of the vertices drawn, and how many there are; `None` while none is.
    pub drawn: Option<(wgpu::Buffer, u32)>,
    /// The runs the list was made for; `None` before it is first made.
    choice: Option<Choice>,
}

impl DeviceCopy {
    /// `bytes` on the device as a vertex buffer, laid out by `runs`, nothing drawn until
    /// [`DeviceCopy::select`]; `None` for no bytes.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        bytes: &[u8],
        runs: LevelRuns,
        usage: wgpu::BufferUsages,
    ) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: usage | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytes);
        Some(Self {
            buffer,
            runs,
            drawn: None,
            choice: None,
        })
    }

    /// List the vertices drawn while the block's objects stand at `levels`: whether the list was
    /// made again, which it is not when the levels draw the same runs of this copy as before.
    pub fn select(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, levels: &[u32]) -> bool {
        let choice = self.runs.choice(levels);
        if self.choice.as_ref() == Some(&choice) {
            return false;
        }
        let (bytes, count) = self.runs.index_bytes(&choice);
        self.choice = Some(choice);
        if bytes.is_empty() {
            self.drawn = None;
            return true;
        }
        let fits = self
            .drawn
            .as_ref()
            .is_some_and(|(b, _)| b.size() >= bytes.len() as u64);
        if !fits {
            self.drawn = Some((
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("retained geometry drawn"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                0,
            ));
        }
        if let Some((buffer, drawn)) = self.drawn.as_mut() {
            queue.write_buffer(buffer, 0, &bytes);
            *drawn = count;
        }
        true
    }

    /// Draw the listed vertices from index `from` on, the copy bound as vertex buffer 0.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, from: u32) {
        let Some((indices, count)) = self.drawn.as_ref() else {
            return;
        };
        if from >= *count {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(from..*count, 0, 0..1);
    }
}
