//! Per-tick timings read from the server's own monitors, after each world iteration.
//!
//! The server's own monitors, plus its flag-gated message-handler timing (which the soak turns
//! on; the probe drains it). Each iteration, the probe reads the last event of every
//! `ServerPerformanceMonitor` stage and cumulative section (ACE's `TimedEventHistory.LastEvent`, on
//! the machine clock the soak installs) and of every loaded landblock's `Monitor5m`, and adds it to
//! a histogram when the history registered a new event. The **server tick** is the sum of the six
//! stages `WorldManager.UpdateWorld` times (player manager, inbound queue, world actions, delays,
//! `UpdateGameWorld`, session work): the world's own iteration, without the bots' side.
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::collections::BTreeMap;

use empyrean_common::performance::timed_event_history::TimedEventHistory;
use empyrean_world::managers::landblock_manager;
use empyrean_world::managers::server_performance_monitor::{
    self as perf, CumulativeEventHistoryType, MonitorType,
};
use empyrean_world::World;

use super::metrics::Histogram;
use super::profile::Timed;

/// The stages `WorldManager.UpdateWorld` times directly, one after another (the others nest inside
/// `UpdateGameWorld` or `DoSessionWork`).
pub const TICK_STAGES: [MonitorType; 6] = [
    MonitorType::PlayerManagerTick,
    MonitorType::NetworkManagerInboundClientMessageQueueRun,
    MonitorType::ActionQueueRunActions,
    MonitorType::DelayManagerRunActions,
    MonitorType::UpdateGameWorld,
    MonitorType::NetworkManagerDoSessionWork,
];

/// A history's identity between two reads: a new event changes the count (or, after a clear, the
/// total).
fn mark(h: &TimedEventHistory) -> (i64, u64) {
    (h.total_events, h.total_seconds.to_bits())
}

#[derive(Debug, Default)]
struct Series {
    last: (i64, u64),
    hist: Histogram,
}

impl Series {
    /// Adds the history's last event if it is new; returns it in ms.
    fn read(&mut self, h: &TimedEventHistory) -> Option<f64> {
        let m = mark(h);
        if m == self.last {
            return None;
        }
        self.last = m;
        if h.total_events == 0 {
            return None;
        }
        let ms = h.last_event * 1000.0;
        self.hist.add(ms);
        Some(ms)
    }
}

/// The per-tick reader.
#[derive(Debug, Default)]
pub struct Probe {
    stages: Vec<(&'static str, Series)>,
    sections: Vec<(&'static str, Series)>,
    landblocks: BTreeMap<u32, Series>,
    /// Whether any landblock monitor ever read above zero (they read the tick's frozen clock).
    pub landblock_timed: bool,
    /// Each message handler's invocations, by "kind name".
    handlers: BTreeMap<String, Histogram>,
    /// Whether the server's handler timing was on.
    pub handler_timed: bool,
}

impl Probe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stages: MonitorType::ALL
                .iter()
                .map(|m| (m.name(), Series::default()))
                .collect(),
            sections: CumulativeEventHistoryType::ALL
                .iter()
                .map(|c| (c.name(), Series::default()))
                .collect(),
            landblocks: BTreeMap::new(),
            landblock_timed: false,
            handlers: BTreeMap::new(),
            handler_timed: false,
        }
    }

    /// Reads the monitors after one iteration; returns the server tick in ms (0 when no stage
    /// registered, e.g. before the monitor starts).
    pub fn after_tick(&mut self, w: &mut World) -> f64 {
        if let Some(times) = w.sessions.inbound.handler_timing.as_mut() {
            self.handler_timed = true;
            for t in times.drain(..) {
                self.handlers
                    .entry(format!("{} {}", t.kind, t.name))
                    .or_default()
                    .add(t.seconds * 1000.0);
            }
        }
        let w = &*w;
        let mut tick = 0.0;
        for (m, (_, s)) in MonitorType::ALL.iter().zip(self.stages.iter_mut()) {
            if let Some(ms) = s.read(perf::get_event_history_24h(w, *m)) {
                if TICK_STAGES.contains(m) {
                    tick += ms;
                }
            }
        }
        for (c, (_, s)) in CumulativeEventHistoryType::ALL
            .iter()
            .zip(self.sections.iter_mut())
        {
            s.read(perf::get_cumulative_event_history_24h(w, *c));
        }
        for id in landblock_manager::get_loaded_landblocks(w) {
            if let Some(l) = w.landblock_manager.landblocks.get(id) {
                let s = self.landblocks.entry(id.raw() >> 16).or_default();
                if s.read(&l.monitor_5m.event_history)
                    .is_some_and(|ms| ms > 0.0)
                {
                    self.landblock_timed = true;
                }
            }
        }
        tick
    }

    /// Every monitor stage and cumulative section, timed over the run.
    #[must_use]
    pub fn systems(&self) -> Vec<Timed> {
        self.stages
            .iter()
            .chain(self.sections.iter())
            .map(|(n, s)| Timed::from_histogram(n, &s.hist))
            .collect()
    }

    /// Every message handler invoked, timed over the run (empty when the timing was off).
    #[must_use]
    pub fn handlers(&self) -> Vec<Timed> {
        self.handlers
            .iter()
            .map(|(n, h)| Timed::from_histogram(n, h))
            .collect()
    }

    /// Every landblock seen, timed over the run (all zero while the landblock monitors read the
    /// tick's frozen clock).
    #[must_use]
    pub fn landblocks(&self) -> Vec<Timed> {
        self.landblocks
            .iter()
            .map(|(id, s)| Timed::from_histogram(&format!("{id:04X}"), &s.hist))
            .collect()
    }
}
