//! The clock a plugin's time in its VM is read from: what each entry takes counts against the
//! plugin's share of the frame and against all plugins' share together.

use web_time::{Duration, Instant};

pub use dereth_plugin_api::VmClock;

/// The machine's monotonic clock: an entry takes what it takes in real time. The host's clock
/// unless it is given another ([`crate::PluginHost::with_vm_clock`]).
#[derive(Debug)]
pub struct MachineClock {
    start: Instant,
}

impl Default for MachineClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl VmClock for MachineClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}
