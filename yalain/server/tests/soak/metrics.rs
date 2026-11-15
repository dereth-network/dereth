//! What a soak run measures: samples of the world's step times by stage (its step timer, as the
//! status endpoint gives it), its objects, the server's memory and processor time, and what the
//! players did and were told; written out as a report and as JSON.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::bot::{Counts, Kind};

/// The stages in the step timer's order.
pub const STAGES: [&str; 9] = [
    "network",
    "timers",
    "camps",
    "creatures",
    "step",
    "fights",
    "saves",
    "other",
    "transport",
];

/// One sample: the status endpoint's step figures over the last minute's steps, the world's objects
/// and players, and the server's memory and share of a processor since the sample before.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Seconds since sampling began.
    pub at: f64,
    pub step: serde_json::Value,
    pub objects: u64,
    pub players: u64,
    pub rss_mb: Option<f64>,
    pub cpu: Option<f64>,
}

impl Sample {
    pub fn mean_ms(&self) -> f64 {
        ms(&self.step["recent"]["mean_ms"])
    }
    pub fn percentile_ms(&self) -> f64 {
        ms(&self.step["recent"]["p99_ms"])
    }
    pub fn worst_ms(&self) -> f64 {
        ms(&self.step["recent"]["worst_ms"])
    }
    pub fn stage_mean_ms(&self, stage: &str) -> f64 {
        ms(&self.step["recent"]["stages"][stage]["mean_ms"])
    }
    pub fn stage_worst_ms(&self, stage: &str) -> f64 {
        ms(&self.step["recent"]["stages"][stage]["worst_ms"])
    }
}

/// Milliseconds from a figure.
pub fn ms(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// What one run measured.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    pub players: usize,
    pub seconds: f64,
    pub mix: String,
    pub seed: u64,
    pub creatures: u32,
    pub places: usize,
    pub entered_after: f64,
    /// The step timer's totals at the start and end of sampling.
    pub first: serde_json::Value,
    pub last: serde_json::Value,
    pub samples: Vec<Sample>,
    /// The world's objects before any player came, once they had all entered, and after they had
    /// all left and the tail had run.
    pub objects_before: u64,
    pub objects_in: u64,
    pub objects_after: u64,
    pub rss_before: Option<f64>,
    pub rss_peak: Option<f64>,
    pub players_counts: Vec<Counts>,
    pub in_world_at_end: usize,
    pub warned: usize,
    pub panicked: bool,
}

impl Measured {
    /// Steps and overruns while sampled.
    pub fn steps(&self) -> (u64, u64) {
        let d = |k: &str| self.last[k].as_u64().unwrap_or(0) - self.first[k].as_u64().unwrap_or(0);
        (d("steps"), d("overruns"))
    }

    /// The mean over the samples of a figure.
    pub fn average(&self, f: impl Fn(&Sample) -> f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let n = self.samples.len() as f64;
        self.samples.iter().map(f).sum::<f64>() / n
    }

    /// The highest over the samples of a figure.
    pub fn highest(&self, f: impl Fn(&Sample) -> f64) -> f64 {
        self.samples.iter().map(f).fold(0.0, f64::max)
    }

    /// World messages per player per minute in the world, the mean and the most.
    pub fn messages_per_minute(&self) -> (f64, f64) {
        let rates: Vec<f64> = self
            .players_counts
            .iter()
            .map(|c| {
                let t: f64 = c.time_in.values().sum();
                #[allow(clippy::cast_precision_loss)]
                if t > 0.0 {
                    c.world_messages as f64 * 60.0 / t
                } else {
                    0.0
                }
            })
            .collect();
        if rates.is_empty() {
            return (0.0, 0.0);
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = rates.iter().sum::<f64>() / rates.len() as f64;
        (mean, rates.iter().copied().fold(0.0, f64::max))
    }

    /// The players' counts added up.
    pub fn totals(&self) -> Counts {
        let mut t = Counts::default();
        for c in &self.players_counts {
            t.world_messages += c.world_messages;
            for (k, v) in &c.by_type {
                *t.by_type.entry(*k).or_default() += v;
            }
            for (k, v) in &c.time_in {
                *t.time_in.entry(*k).or_default() += v;
            }
            t.reports += c.reports;
            t.told_back += c.told_back;
            t.said += c.said;
            t.attacks += c.attacks;
            t.kills_seen += c.kills_seen;
            t.corpses_opened += c.corpses_opened;
            t.takes += c.takes;
            t.portals_used += c.portals_used;
            t.no_portal += c.no_portal;
            t.trips += c.trips;
        }
        t
    }

    /// The report, in Markdown.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let (steps, overruns) = self.steps();
        let _ = writeln!(
            out,
            "## {} players ({} places, {} creatures kept by the camps), mix {}, seed {:#x}",
            self.players, self.places, self.creatures, self.mix, self.seed
        );
        let _ = writeln!(
            out,
            "entered after {:.1} s; sampled {:.0} s; steps {steps}, overruns {overruns}, warnings in \
             the log {}; first step {} ms",
            self.entered_after, self.seconds, self.warned, self.last["first_ms"]
        );
        let mb = |m: Option<f64>| m.map_or("?".into(), |v| format!("{v:.0} MB"));
        let (mean_msgs, most_msgs) = self.messages_per_minute();
        let _ = writeln!(
            out,
            "objects: before {}, all in {}, after {}; memory before {}, peak {}; world messages per \
             player per minute: mean {mean_msgs:.0}, most {most_msgs:.0}",
            self.objects_before,
            self.objects_in,
            self.objects_after,
            mb(self.rss_before),
            mb(self.rss_peak)
        );
        let t = self.totals();
        let time: Vec<String> = Kind::ALL
            .iter()
            .map(|k| {
                format!(
                    "{} {:.0} s",
                    k.name(),
                    t.time_in.get(k).copied().unwrap_or(0.0)
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "players did: {}; reports {}, told back {}, said {}, attacks {}, targets gone {}, corpses \
             opened {}, take-alls {}, portals used {} (none near {}), trips {}",
            time.join(", "),
            t.reports,
            t.told_back,
            t.said,
            t.attacks,
            t.kills_seen,
            t.corpses_opened,
            t.takes,
            t.portals_used,
            t.no_portal,
            t.trips
        );
        let mut kinds: Vec<(u32, u64)> = t.by_type.iter().map(|(k, v)| (*k, *v)).collect();
        kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
        let top: Vec<String> = kinds
            .iter()
            .take(8)
            .map(|(k, v)| format!("{k:#x} {v}"))
            .collect();
        let _ = writeln!(out, "most told message types: {}", top.join(", "));
        let _ = writeln!(
            out,
            "\n| at s | players | objects | mean ms | p99 ms | worst ms | rss MB | cpu | {} |",
            STAGES.join(" | ")
        );
        let _ = writeln!(out, "|{}", "---|".repeat(8 + STAGES.len()));
        for s in &self.samples {
            let stages: Vec<String> = STAGES
                .iter()
                .map(|st| format!("{:.2}/{:.1}", s.stage_mean_ms(st), s.stage_worst_ms(st)))
                .collect();
            let _ = writeln!(
                out,
                "| {:.0} | {} | {} | {:.2} | {:.2} | {:.1} | {} | {} | {} |",
                s.at,
                s.players,
                s.objects,
                s.mean_ms(),
                s.percentile_ms(),
                s.worst_ms(),
                s.rss_mb.map_or("?".into(), |v| format!("{v:.0}")),
                s.cpu.map_or("?".into(), |c| format!("{:.0}%", c * 100.0)),
                stages.join(" | ")
            );
        }
        let _ = writeln!(
            out,
            "\n(stages: mean/worst ms over the last minute's steps)\n"
        );
        out
    }

    /// The run as JSON.
    pub fn json(&self) -> serde_json::Value {
        let t = self.totals();
        serde_json::json!({
            "players": self.players,
            "seconds": self.seconds,
            "mix": self.mix,
            "seed": self.seed,
            "first": self.first,
            "last": self.last,
            "objects": { "before": self.objects_before, "in": self.objects_in, "after": self.objects_after },
            "rss_mb": { "before": self.rss_before, "peak": self.rss_peak },
            "messages_per_minute": self.messages_per_minute(),
            "did": {
                "reports": t.reports, "told_back": t.told_back, "said": t.said,
                "attacks": t.attacks, "targets_gone": t.kills_seen, "corpses_opened": t.corpses_opened,
                "takes": t.takes, "portals_used": t.portals_used, "no_portal": t.no_portal, "trips": t.trips,
                "seconds": t.time_in.iter().map(|(k, v)| (k.name().to_owned(), serde_json::json!(v))).collect::<serde_json::Map<_, _>>(),
            },
            "by_type": t.by_type.iter().map(|(k, v)| (format!("{k:#x}"), serde_json::json!(v))).collect::<serde_json::Map<_, _>>(),
            "samples": self.samples.iter().map(|s| serde_json::json!({
                "at": s.at, "step": s.step, "objects": s.objects, "players": s.players,
                "rss_mb": s.rss_mb, "cpu": s.cpu,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The stages' mean over a run's samples, by name.
pub fn stage_means(m: &Measured) -> BTreeMap<&'static str, f64> {
    STAGES
        .iter()
        .map(|st| (*st, m.average(|s| s.stage_mean_ms(st))))
        .collect()
}
