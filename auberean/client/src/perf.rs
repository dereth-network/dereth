//! `--dev-perf`: each frame's times and device work, one line of tab-separated columns a frame.
//! A developer's measure: nothing the player sees depends on it.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use auberean_gpu::Work;

/// The columns, in order.
const HEADER: &str = "at_s\tframe_ms\tcost_ms\tshell_ms\tsetup\tshadow\tprepare\tspots\treflection\tworld\tfinish\tsubmit\tcompose\tacquire\tcompose_submit\tshow\treclaim\tdraws\tbuffers\tbuffer_bytes\twritten_bytes\ttextures\ttexture_bytes\tbind_groups\tmesh_hits\tmesh_misses\tframes\tscene_ms\tclient_ms\tbuild_ms\thear_ms\tui_update_ms\tui_after_ms\tsound_ms\tui_draw_ms\tview_ms";

/// The file, and when the last frame began.
pub struct PerfLog {
    out: BufWriter<File>,
    start: Instant,
    last: Option<Instant>,
}

impl PerfLog {
    /// Start a log at `path`, replacing what was there.
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "{HEADER}")?;
        Ok(Self {
            out,
            start: Instant::now(),
            last: None,
        })
    }

    /// A frame began at `began`, spent `shell` in the shell (`costs` of it in each of the shell's
    /// steps, in seconds), and asked `work` of the device.
    pub fn frame(&mut self, began: Instant, shell: Duration, costs: [f64; 9], work: Work) {
        let frame = self
            .last
            .map_or(0.0, |l| began.duration_since(l).as_secs_f64() * 1000.0);
        self.last = Some(began);
        let m = work.ms;
        let line = writeln!(
            self.out,
            "{:.3}\t{frame:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            began.duration_since(self.start).as_secs_f64(),
            began.elapsed().as_secs_f64() * 1000.0,
            shell.as_secs_f64() * 1000.0,
            m.setup,
            m.shadow,
            m.prepare,
            m.spots,
            m.reflection,
            m.world,
            m.finish,
            m.submit,
            m.compose,
            m.acquire,
            m.compose_submit,
            m.show,
            m.reclaim,
            work.draws,
            work.buffers,
            work.buffer_bytes,
            work.written_bytes,
            work.textures,
            work.texture_bytes,
            work.bind_groups,
            work.mesh_hits,
            work.mesh_misses,
            work.frames,
            costs.map(|c| format!("{:.2}", c * 1000.0)).join("\t"),
        );
        if line.is_ok() {
            // A frame's line is on the disk by the next: a run is stopped by ending its process.
            let _ = self.out.flush();
        }
    }
}
