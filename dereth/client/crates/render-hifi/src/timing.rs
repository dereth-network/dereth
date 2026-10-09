//! What each pass cost: GPU milliseconds from timestamp queries where the device has them, and
//! the CPU time of the frame's preparation.
//!
//! The timestamps of a frame are read back while later frames are drawn: the device is never
//! waited on for them, so a frame whose timings are still on their way simply takes none.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// One pass's cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassTime {
    /// The pass's name.
    pub name: &'static str,
    /// Its GPU time, milliseconds.
    pub gpu_ms: f32,
}

/// One frame's costs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timing {
    /// The CPU time spent preparing the frame, milliseconds.
    pub cpu_ms: f32,
    /// Each pass's GPU time, in pass order; empty without timestamp queries.
    pub passes: Vec<PassTime>,
}

impl Timing {
    /// The GPU time of every pass together, milliseconds.
    #[must_use]
    pub fn gpu_ms_total(&self) -> f32 {
        self.passes.iter().map(|p| p.gpu_ms).sum()
    }
}

/// How many passes one frame can time.
pub const MAX_TIMED: u32 = 32;

/// The read-back's state: free, on its way back, or back and readable.
const FREE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;

/// The timestamp pool: two timestamps per timed pass, resolved into a buffer and read back
/// without waiting.
#[derive(Debug)]
pub struct GpuTimer {
    pool: Option<Pool>,
    /// The passes timed in the frame being recorded, in order.
    names: Vec<&'static str>,
    /// The passes of the frame whose timestamps are on their way back.
    in_flight: Vec<&'static str>,
    /// Whether this frame's timestamps are being recorded.
    armed: bool,
    /// The span opened by [`GpuTimer::span_begin`] and not closed yet: its index.
    open_span: Option<u32>,
    last: Timing,
}

#[derive(Debug)]
struct Pool {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
    period_ns: f32,
}

impl GpuTimer {
    /// A pool on `device`, when it has timestamp queries; one that times nothing otherwise.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let pool = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| {
                let bytes = u64::from(MAX_TIMED) * 2 * 8;
                Pool {
                    set: device.create_query_set(&wgpu::QuerySetDescriptor {
                        label: Some("high-fidelity timestamps"),
                        ty: wgpu::QueryType::Timestamp,
                        count: MAX_TIMED * 2,
                    }),
                    resolve: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("high-fidelity timestamps"),
                        size: bytes,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    }),
                    readback: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("high-fidelity timestamps"),
                        size: bytes,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    state: Arc::new(AtomicU8::new(FREE)),
                    period_ns: queue.get_timestamp_period(),
                }
            });
        Self {
            pool,
            names: Vec::new(),
            in_flight: Vec::new(),
            armed: false,
            open_span: None,
            last: Timing::default(),
        }
    }

    /// A timer that times nothing.
    #[must_use]
    pub fn none() -> Self {
        Self {
            pool: None,
            names: Vec::new(),
            in_flight: Vec::new(),
            armed: false,
            open_span: None,
            last: Timing::default(),
        }
    }

    /// Whether the device can time passes at all.
    #[must_use]
    pub fn available(&self) -> bool {
        self.pool.is_some()
    }

    /// Start a frame: collect the timings that have come back, and time this frame's passes if
    /// the read-back is free.
    pub fn begin_frame(&mut self, device: &wgpu::Device) {
        self.names.clear();
        self.armed = false;
        self.open_span = None;
        let Some(pool) = &self.pool else {
            return;
        };
        // A look at the device without waiting, so a finished read-back reports in.
        #[cfg(not(target_arch = "wasm32"))]
        let _ = device.poll(wgpu::PollType::Poll);
        #[cfg(target_arch = "wasm32")]
        let _ = device;
        match pool.state.load(Ordering::Acquire) {
            MAPPED => {
                if let Ok(data) = pool.readback.slice(..).get_mapped_range() {
                    let ticks: Vec<u64> = data
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|b| u64::from_le_bytes(*b))
                        .collect();
                    self.last.passes = self
                        .in_flight
                        .iter()
                        .enumerate()
                        .map(|(i, name)| {
                            let (a, b) = (ticks[i * 2], ticks[i * 2 + 1]);
                            #[allow(clippy::cast_precision_loss)] // tick counts
                            let ms = b.saturating_sub(a) as f32 * pool.period_ns / 1.0e6;
                            PassTime { name, gpu_ms: ms }
                        })
                        .collect();
                }
                pool.readback.unmap();
                pool.state.store(FREE, Ordering::Release);
                self.armed = true;
            }
            FREE => self.armed = true,
            _ => {}
        }
    }

    /// The timestamp writes for one compute pass named `name`, when this frame is timed.
    pub fn compute_writes(
        &mut self,
        name: &'static str,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let pool = self.pool.as_ref()?;
        let index = u32::try_from(self.names.len()).ok()?;
        if !self.armed || index >= MAX_TIMED {
            return None;
        }
        self.names.push(name);
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &pool.set,
            beginning_of_pass_write_index: Some(index * 2),
            end_of_pass_write_index: Some(index * 2 + 1),
        })
    }

    /// The timestamp writes for one pass named `name`, when this frame is timed.
    pub fn writes(&mut self, name: &'static str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let pool = self.pool.as_ref()?;
        let index = u32::try_from(self.names.len()).ok()?;
        if !self.armed || index >= MAX_TIMED {
            return None;
        }
        self.names.push(name);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &pool.set,
            beginning_of_pass_write_index: Some(index * 2),
            end_of_pass_write_index: Some(index * 2 + 1),
        })
    }

    /// The writes that open a span named `name` over several render passes, for the first of
    /// them: the span's time is from the start of that pass to the end of the pass handed
    /// [`GpuTimer::span_end`]'s writes.
    pub fn span_begin(
        &mut self,
        name: &'static str,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let pool = self.pool.as_ref()?;
        let index = u32::try_from(self.names.len()).ok()?;
        if !self.armed || index >= MAX_TIMED {
            return None;
        }
        self.names.push(name);
        self.open_span = Some(index);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &pool.set,
            beginning_of_pass_write_index: Some(index * 2),
            end_of_pass_write_index: None,
        })
    }

    /// The writes that close the open span, for the last of its render passes; `None` when no
    /// span is open.
    pub fn span_end(&mut self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let pool = self.pool.as_ref()?;
        let index = self.open_span.take()?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &pool.set,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(index * 2 + 1),
        })
    }

    /// The writes for one render pass of a span of `count` passes, the `i`th: the first opens
    /// the span named `name`, the last closes it, and a span of one pass is timed whole.
    pub fn span_writes(
        &mut self,
        name: &'static str,
        i: usize,
        count: usize,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        match (i, count) {
            (_, 1) => self.writes(name),
            (0, _) => self.span_begin(name),
            (i, n) if i + 1 == n => self.span_end(),
            _ => None,
        }
    }

    /// Resolve this frame's timestamps into the read-back, at the end of its encoding.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(pool) = &self.pool else {
            return;
        };
        if !self.armed || self.names.is_empty() {
            self.armed = false;
            return;
        }
        let count = u32::try_from(self.names.len()).unwrap_or(MAX_TIMED) * 2;
        encoder.resolve_query_set(&pool.set, 0..count, &pool.resolve, 0);
        encoder.copy_buffer_to_buffer(
            &pool.resolve,
            0,
            &pool.readback,
            0,
            Some(u64::from(count) * 8),
        );
    }

    /// The frame has been handed to the queue: start reading its timestamps back.
    pub fn submitted(&mut self) {
        let Some(pool) = &self.pool else {
            return;
        };
        if !self.armed || self.names.is_empty() {
            return;
        }
        self.armed = false;
        self.in_flight = std::mem::take(&mut self.names);
        pool.state.store(MAPPING, Ordering::Release);
        let state = Arc::clone(&pool.state);
        pool.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                state.store(if r.is_ok() { MAPPED } else { FREE }, Ordering::Release);
            });
    }

    /// The latest timings that have come back.
    #[must_use]
    pub fn last(&self) -> &Timing {
        &self.last
    }
}
