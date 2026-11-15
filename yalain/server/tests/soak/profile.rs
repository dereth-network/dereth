//! A run's profile and the host's baselines: each run's figures are kept by its host and its shape
//! (players, seconds, mix), and a later run of the same shape on the same host is compared with
//! them, so a regression shows. There are no absolute ceilings beyond the step's budget
//! ([`crate::invariants`]): hosts differ.

use std::collections::BTreeMap;
use std::path::Path;

use crate::metrics::{stage_means, Measured};

/// How much worse than its baseline a figure may be before it is a regression, in shares.
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub step: f64,
    pub memory: f64,
    pub messages: f64,
}

impl Default for Margins {
    fn default() -> Self {
        Self {
            step: 0.25,
            memory: 0.20,
            messages: 0.20,
        }
    }
}

/// The figures a run is compared by.
#[must_use]
pub fn profile(m: &Measured) -> serde_json::Value {
    serde_json::json!({
        "mean_ms": m.average(super::metrics::Sample::mean_ms),
        "p99_ms": m.average(super::metrics::Sample::percentile_ms),
        "worst_ms": m.highest(super::metrics::Sample::worst_ms),
        "rss_peak_mb": m.rss_peak,
        "messages_per_minute": m.messages_per_minute().0,
        "objects_in": m.objects_in,
        "stages_mean_ms": stage_means(m),
    })
}

/// The run's shape: what a baseline is kept under.
#[must_use]
pub fn key(m: &Measured) -> String {
    format!(
        "players={},seconds={:.0},mix={},seed={:#x}",
        m.players, m.seconds, m.mix, m.seed
    )
}

/// The host: its name and its processors.
#[must_use]
pub fn host() -> String {
    let name = std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".into());
    let cores = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    format!("{name}-{cores}")
}

/// Compare a run's profile with its host's baseline in `dir`, if one is kept for its shape; each
/// regression in words. With `record`, keep the run as the baseline after comparing.
#[must_use]
pub fn compare_and_record(m: &Measured, dir: &Path, margins: Margins, record: bool) -> Vec<String> {
    let path = dir.join(format!("{}.json", host()));
    let mut kept: BTreeMap<String, serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let now = profile(m);
    let mut worse = Vec::new();
    if let Some(base) = kept.get(&key(m)) {
        let f = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(0.0);
        for (k, margin) in [
            ("mean_ms", margins.step),
            ("p99_ms", margins.step),
            ("rss_peak_mb", margins.memory),
            ("messages_per_minute", margins.messages),
        ] {
            let (was, is) = (f(base, k), f(&now, k));
            if was > 0.0 && is > was * (1.0 + margin) {
                worse.push(format!(
                    "{k}: {is:.2} against the baseline's {was:.2} (more than {:.0}% worse)",
                    margin * 100.0
                ));
            }
        }
    }
    if record {
        kept.insert(key(m), now);
        std::fs::create_dir_all(dir).expect("the baselines' folder");
        std::fs::write(&path, serde_json::to_string_pretty(&kept).expect("JSON"))
            .expect("the baseline is written");
    }
    worse
}
