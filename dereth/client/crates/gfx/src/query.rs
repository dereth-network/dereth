//! Timestamps: marks written by the device as it reaches them, read once the work is done.
//! What they measure is reported, never asserted.

use ash::vk;

use crate::device::vkr;
use crate::Error;

/// The marks one recording can write.
pub const MARKS: u32 = 64;

/// One span the device measured: from the mark named `label` to the next mark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// The mark the span starts at.
    pub label: &'static str,
    /// Its length in the device's ticks.
    pub ticks: u64,
    /// Its length in milliseconds.
    pub ms: f64,
}

/// One recording's marks: the query pool they are written into, and their names.
#[derive(Debug)]
pub(crate) struct Slot {
    pub pool: vk::QueryPool,
    pub labels: Vec<&'static str>,
}

/// The query pools, and the spans of the last recording read.
#[derive(Debug, Default)]
pub(crate) struct Timers {
    free: Vec<vk::QueryPool>,
    all: Vec<vk::QueryPool>,
    pub last: Vec<Timing>,
}

impl Timers {
    /// A pool for one recording's marks.
    pub(crate) fn take(&mut self, device: &ash::Device) -> Result<vk::QueryPool, Error> {
        if let Some(p) = self.free.pop() {
            return Ok(p);
        }
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(MARKS);
        // SAFETY: a live local; destroyed with the timers.
        let pool = vkr("vkCreateQueryPool", unsafe {
            device.create_query_pool(&info, None)
        })?;
        self.all.push(pool);
        Ok(pool)
    }

    /// The recording `slot` was written by is done: read its marks into the last spans, and
    /// give its pool back.
    pub(crate) fn retired(&mut self, device: &ash::Device, slot: Slot, period_ns: f32) {
        let n = slot.labels.len();
        if n > 0 {
            let mut ticks = vec![0u64; n];
            // SAFETY: the work that wrote the queries is done; `ticks` holds one per mark.
            let read = unsafe {
                device.get_query_pool_results(
                    slot.pool,
                    0,
                    &mut ticks,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
            };
            if read.is_ok() {
                self.last = slot
                    .labels
                    .iter()
                    .zip(ticks.windows(2))
                    .map(|(label, w)| {
                        let t = w[1].saturating_sub(w[0]);
                        #[allow(clippy::cast_precision_loss)] // a span of ticks
                        let ms = t as f64 * f64::from(period_ns) / 1.0e6;
                        Timing {
                            label,
                            ticks: t,
                            ms,
                        }
                    })
                    .collect();
            }
        }
        self.free.push(slot.pool);
    }

    /// # Safety
    /// Nothing the device runs writes the pools any more.
    pub(crate) unsafe fn destroy(&self, device: &ash::Device) {
        for p in &self.all {
            device.destroy_query_pool(*p, None);
        }
    }
}
