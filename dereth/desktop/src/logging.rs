//! The client's log: the one subscriber for every crate's `tracing` events (and, bridged, the
//! `log` records of the dependencies that use that facade instead).
//!
//! Configured by [`Config`] alone -- `--log <filter>` or `[Log] Level=`, `--log-file` or
//! `[Log] File=`, and `--log-spans` -- and never by the environment. The default is `info`. The
//! live diagnostic traces are the `dereth::trace::net`, `::camera`, `::raise` and `::notice`
//! targets at `debug`: `--log info,dereth::trace=debug` shows all four.
//!
//! * **stderr**, always: one compact line per event, `LEVEL target: message`, with no timestamp,
//!   so that two runs of the same deterministic scene write the same bytes (the headless capture's
//!   three-run comparison reads this stream).
//! * **`<binary>.log`** beside the preferences file, with `--log-file`: the same lines with a
//!   UTC timestamp, appended, so a run started from the launcher with no console still leaves a
//!   record. Several clients may share it, so each run starts with a line naming its process.

use dereth_client_runtime::config::Config;

use crate::Product;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Install the log for `cfg`. Called once, before anything that logs.
pub fn install<P: Product>(cfg: &Config) {
    let file_name = format!("{}.log", P::BINARY_NAME);
    let (filter, refused) = match cfg.log_filter.as_deref().map(str::parse::<Targets>) {
        None => (Targets::new().with_default(LevelFilter::INFO), None),
        Some(Ok(t)) => (t, None),
        Some(Err(e)) => (Targets::new().with_default(LevelFilter::INFO), Some(e)),
    };
    let spans = if cfg.log_spans {
        FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };
    let stderr = tracing_subscriber::fmt::layer()
        .compact()
        .without_time()
        .with_span_events(spans.clone())
        .with_writer(std::io::stderr);
    let path = cfg
        .preferences_file
        .parent()
        .map_or_else(|| file_name.clone().into(), |d| d.join(&file_name));
    let (file, file_error) = if cfg.log_file {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(f) => (
                Some(
                    tracing_subscriber::fmt::layer()
                        .compact()
                        .with_span_events(spans)
                        .with_writer(std::sync::Mutex::new(f)),
                ),
                None,
            ),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    let has_file = file.is_some();
    // `try_init` also installs the bridge that turns `log` records into events. It fails only
    // if a subscriber is already installed, which in this binary nothing else does.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .try_init();
    if let Some(e) = refused {
        tracing::warn!(
            "the log filter {:?} is not one this client reads ({e}); logging at info",
            cfg.log_filter.as_deref().unwrap_or_default()
        );
    }
    if let Some(e) = file_error {
        tracing::warn!("the log file {} would not open: {e}", path.display());
    }
    if has_file {
        tracing::info!(
            "log file {}, process {}",
            path.display(),
            std::process::id()
        );
    }
}
