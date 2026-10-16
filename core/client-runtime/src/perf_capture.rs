//! The performance capture (`@perfcapture`, CD-report-commands): every frame's time for a few
//! seconds, where it went on the processor, step by step and in the presentation's own parts, and
//! each Better Lighting pass's time on the GPU where the device times them, saved to a file with
//! what `@report` says about the frame.
//!
//! Nothing is kept while no capture runs. While one does, a frame costs one row of text.
//!
//! The file is a few lines of `# name: value` saying what was recorded, then a table with a
//! header row and a row a frame:
//!
//! - `frame`: the frame's number in the capture;
//! - `frame_ms`: from the start of the frame before to the start of this one;
//! - the frame loop's steps, milliseconds each, in the order a frame runs them (`ui_queue` to
//!   `pace`, [`STEP_COLUMNS`](crate::perf_capture::STEP_COLUMNS));
//! - the presentation's parts: `walk` (the walk that records the frame's draws), `snapshot` (the
//!   copy handed to Better Lighting), `drawn_ground` (its upkeep) and `better_lighting` (Better
//!   Lighting's whole time on the processor);
//! - `gpu_total`, the sum of the passes' GPU times;
//! - `steps`, `prepare`, `encode` and `gpu_passes`: Better Lighting's steps, each pass's time
//!   preparing and recording, and each pass's GPU time, as `name=ms` a space between.
//!
//! The GPU's times are those the device has handed back by the frame, a few frames late.

use std::fmt::Write as _;

use crate::frame::{FrameStep, STEPS};
use crate::report::PresentCosts;

/// The columns of the frame loop's steps, in [`FrameStep::ORDER`].
pub const STEP_COLUMNS: [&str; STEPS] = [
    "ui_queue",
    "clock",
    "window_events",
    "network",
    "login_events",
    "packets",
    "asset_cache",
    "ui",
    "world",
    "prepare_device",
    "begin_frame",
    "draw_world",
    "present_frame",
    "pace",
];

/// The table's header row.
#[must_use]
pub fn header() -> String {
    let mut h = String::from("frame,frame_ms");
    for c in STEP_COLUMNS {
        h.push(',');
        h.push_str(c);
    }
    h.push_str(
        ",walk,snapshot,drawn_ground,better_lighting,gpu_total,steps,prepare,encode,gpu_passes",
    );
    h
}

/// A frame is a hitch when it takes more than this many times the median frame.
pub const HITCH: f32 = 2.0;

/// A capture that is recording.
#[derive(Debug, Clone)]
pub struct PerfCapture {
    /// How long to record, seconds of the frame loop's clock.
    seconds: f64,
    /// The frame loop's clock when the capture began: the start of the first whole frame after
    /// the command, `None` until then.
    started: Option<f64>,
    /// The chat window the command came from, which the answer goes to.
    pub window: u32,
    /// What `@report` said as the capture began.
    pub report: String,
    /// The table's rows so far.
    rows: String,
    /// Each frame's time, milliseconds.
    frame_ms: Vec<f32>,
}

/// What a finished capture found.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CaptureSummary {
    pub frames: usize,
    pub median_ms: f32,
    pub p95_ms: f32,
    pub worst_ms: f32,
    /// Frames longer than [`HITCH`] times the median.
    pub hitches: usize,
}

impl CaptureSummary {
    /// The summary as one plain sentence.
    #[must_use]
    pub fn sentence(&self) -> String {
        let hitches = if self.hitches == 1 {
            "1 hitch".to_owned()
        } else {
            format!("{} hitches", self.hitches)
        };
        format!(
            "{} frames: median {:.1} ms, 95% under {:.1} ms, worst {:.1} ms, {hitches}.",
            self.frames, self.median_ms, self.p95_ms, self.worst_ms
        )
    }
}

/// The value a share `p` of the way up `sorted`.
fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    // LINT-OK: an index into a list of at most a few hundred thousand frames.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

/// `list` as `name=ms`, a space between.
fn pairs(list: &[(&'static str, f32)]) -> String {
    let mut out = String::new();
    for (name, ms) in list {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = write!(out, "{}={ms:.3}", name.replace(' ', "_"));
    }
    out
}

impl PerfCapture {
    /// A capture of `seconds`, answering in `window`, with what `@report` says. It starts with
    /// the next frame ([`Self::frame`]).
    #[must_use]
    pub fn new(seconds: u32, window: u32, report: String) -> Self {
        Self {
            seconds: f64::from(seconds),
            started: None,
            window,
            report,
            rows: String::new(),
            frame_ms: Vec::new(),
        }
    }

    /// A frame is starting, and the frame loop's clock read `now` in the frame before it: the
    /// first time, the capture starts; after that, the frame before is recorded with `record`.
    /// Whether the capture has recorded its seconds.
    pub fn frame(&mut self, now: f64, record: impl FnOnce(&mut Self)) -> bool {
        let Some(started) = self.started else {
            self.started = Some(now);
            return false;
        };
        record(self);
        // A microsecond's grace, so seconds made of whole steps of a stepped clock end on the
        // step that completes them.
        now - started >= self.seconds - 1e-6
    }

    /// How many frames it has recorded.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frame_ms.len()
    }

    /// Record one frame: `interval` seconds since the frame before began, `steps` seconds in
    /// each of its steps, and what the presentation's parts took.
    pub fn record(&mut self, interval: f32, steps: &[f32; STEPS], costs: Option<&PresentCosts>) {
        let ms = interval * 1000.0;
        let _ = write!(self.rows, "{},{ms:.3}", self.frame_ms.len());
        for step in FrameStep::ORDER {
            let _ = write!(self.rows, ",{:.3}", steps[step.index()] * 1000.0);
        }
        match costs {
            Some(c) => {
                let gpu: f32 = c.gpu.iter().map(|(_, ms)| ms).sum();
                let _ = write!(
                    self.rows,
                    ",{:.3},{:.3},{:.3},{:.3},{gpu:.3},\"{}\",\"{}\",\"{}\",\"{}\"",
                    c.walk,
                    c.snapshot,
                    c.drawn_ground,
                    c.better_lighting,
                    pairs(&c.steps),
                    pairs(&c.prepare),
                    pairs(&c.encode),
                    pairs(&c.gpu),
                );
            }
            None => self.rows.push_str(",,,,,,\"\",\"\",\"\",\"\""),
        }
        self.rows.push('\n');
        self.frame_ms.push(ms);
    }

    /// What the frames recorded so far found.
    #[must_use]
    pub fn summary(&self) -> CaptureSummary {
        let mut sorted = self.frame_ms.clone();
        sorted.sort_by(f32::total_cmp);
        let median = percentile(&sorted, 0.5);
        CaptureSummary {
            frames: sorted.len(),
            median_ms: median,
            p95_ms: percentile(&sorted, 0.95),
            worst_ms: sorted.last().copied().unwrap_or(0.0),
            hitches: sorted.iter().filter(|ms| **ms > HITCH * median).count(),
        }
    }

    /// The file: what was recorded, then the table. `context` is more `name: value` lines for
    /// the head (the GPU, its driver, the present mode).
    #[must_use]
    pub fn file_text(&self, context: &[(&str, String)]) -> String {
        let s = self.summary();
        let mut out = String::from("# Dereth performance capture\n");
        let _ = writeln!(out, "# report: {}", self.report);
        for (name, value) in context {
            let _ = writeln!(out, "# {name}: {}", value.replace(['\r', '\n'], " "));
        }
        let _ = writeln!(out, "# seconds: {}", self.seconds);
        let _ = writeln!(out, "# frames: {}", s.frames);
        let _ = writeln!(
            out,
            "# frame ms: median {:.3}, p95 {:.3}, worst {:.3}, hitches {}",
            s.median_ms, s.p95_ms, s.worst_ms, s.hitches
        );
        out.push_str(&header());
        out.push('\n');
        out.push_str(&self.rows);
        out
    }
}

/// The file's name for a capture finished at `unix_secs` in a zone `utc_offset` seconds east:
/// `perf-capture-YYYYMMDD-HHMMSS.csv`, local time.
#[must_use]
pub fn file_name(unix_secs: i64, utc_offset: i32) -> String {
    let t = unix_secs + i64::from(utc_offset);
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // The civil date of a day count from 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "perf-capture-{year:04}{month:02}{day:02}-{:02}{:02}{:02}.csv",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the capture file's own arithmetic and form; the command that writes it is
    //! asserted through the whole client).
    use super::*;

    #[test]
    fn the_summary_counts_frames_and_names_the_median_the_tail_and_the_hitches() {
        let mut c = PerfCapture::new(1, 1, "dereth report 1".into());
        let steps = [0.001; STEPS];
        for ms in [10.0_f32, 10.0, 11.0, 12.0, 40.0] {
            c.record(ms / 1000.0, &steps, None);
        }
        let s = c.summary();
        assert_eq!(s.frames, 5);
        assert!((s.median_ms - 11.0).abs() < 1e-3, "{s:?}");
        assert!((s.worst_ms - 40.0).abs() < 1e-3);
        assert_eq!(s.hitches, 1);
        assert_eq!(
            s.sentence(),
            "5 frames: median 11.0 ms, 95% under 40.0 ms, worst 40.0 ms, 1 hitch."
        );
    }

    #[test]
    fn every_row_has_a_value_or_an_empty_field_for_every_column() {
        let mut c = PerfCapture::new(1, 1, "dereth report 1".into());
        let steps = [0.002; STEPS];
        c.record(0.016, &steps, None);
        c.record(
            0.017,
            &steps,
            Some(&PresentCosts {
                walk: 1.0,
                gpu: vec![("sky", 0.25), ("lighting", 1.5)],
                steps: vec![("prepare", 0.5)],
                ..PresentCosts::default()
            }),
        );
        let text = c.file_text(&[("gpu", "a card".into())]);
        let table: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        let columns = header().split(',').count();
        assert_eq!(table[0], header());
        assert_eq!(table.len(), 3);
        for row in &table[1..] {
            // The pass lists are quoted and hold no commas.
            assert_eq!(row.split(',').count(), columns, "{row}");
        }
        assert!(
            table[2].contains("\"sky=0.250 lighting=1.500\""),
            "{}",
            table[2]
        );
        assert!(
            table[2].contains(",1.750,"),
            "the GPU's total: {}",
            table[2]
        );
        assert!(text.contains("# gpu: a card\n"));
    }

    #[test]
    fn a_capture_starts_with_the_frame_after_the_command_and_ends_on_its_last_whole_step() {
        let step = 1.0 / 30.0;
        let mut c = PerfCapture::new(1, 1, String::new());
        let mut now = 5.0;
        let mut frames = 0;
        assert!(
            !c.frame(now, |_| frames += 1),
            "the first frame only starts it"
        );
        let mut done = false;
        while !done {
            now += step;
            done = c.frame(now, |c| {
                c.record(0.016, &[0.0; STEPS], None);
                frames += 1;
            });
        }
        assert_eq!(frames, 30, "a second of thirtieths");
        assert_eq!(c.frames(), 30);
    }

    #[test]
    fn the_file_is_named_for_the_local_time_it_was_saved() {
        // 2026-10-16 14:22:33 UTC, two hours east.
        assert_eq!(
            file_name(1_792_160_553, 7200),
            "perf-capture-20261016-162233.csv"
        );
        assert_eq!(file_name(0, 0), "perf-capture-19700101-000000.csv");
    }
}
