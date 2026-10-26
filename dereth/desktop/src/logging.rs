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
//!
//! Both are written off the frame: a line is handed to a writer thread, so a slow disk or console
//! does not stall the frame that logged it (a plugin's console line, written in its entry, once
//! held a frame 59 ms). The order of the lines is kept, and the thread is drained when the run
//! ends.

use dereth_client_runtime::config::Config;

use crate::Product;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Where a finished line goes, or the run's end.
enum Line {
    Stderr(Vec<u8>),
    File(Vec<u8>),
    /// The run is over: every line sent before this is written, and the thread ends. The
    /// installed subscriber keeps its way to the thread for as long as the process lives, so the
    /// thread is told to end rather than waiting for every way to it to close.
    End,
}

/// The log's writer thread, and the way to it. Dropping it drains the thread and ends it: the
/// product holds it to the end of the run.
pub struct Guard {
    tx: std::sync::Mutex<Option<std::sync::mpsc::Sender<Line>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard").finish_non_exhaustive()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut tx) = self.tx.lock() {
            if let Some(tx) = tx.take() {
                let _ = tx.send(Line::End);
            }
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A `tracing` writer that collects one event's bytes and hands them to the writer thread.
struct OffFrame {
    tx: std::sync::mpsc::Sender<Line>,
    file: bool,
}

/// One event's bytes on their way to the writer thread, sent when the event is finished.
struct Buffered {
    tx: std::sync::mpsc::Sender<Line>,
    file: bool,
    bytes: Vec<u8>,
}

impl std::io::Write for Buffered {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Buffered {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.bytes);
        if !bytes.is_empty() {
            let _ = self.tx.send(if self.file {
                Line::File(bytes)
            } else {
                Line::Stderr(bytes)
            });
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for OffFrame {
    type Writer = Buffered;
    fn make_writer(&'a self) -> Buffered {
        Buffered {
            tx: self.tx.clone(),
            file: self.file,
            bytes: Vec::new(),
        }
    }
}

/// Install the log for `cfg`. Called once, before anything that logs; the answer is held to the
/// end of the run.
#[must_use]
pub fn install<P: Product>(cfg: &Config) -> Guard {
    let (tx, rx) = std::sync::mpsc::channel::<Line>();
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
        .with_writer(OffFrame {
            tx: tx.clone(),
            file: false,
        });
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
                Some((
                    tracing_subscriber::fmt::layer()
                        .compact()
                        .with_span_events(spans)
                        .with_writer(OffFrame {
                            tx: tx.clone(),
                            file: true,
                        }),
                    f,
                )),
                None,
            ),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    let has_file = file.is_some();
    let (file, mut sink) = match file {
        Some((layer, f)) => (Some(layer), Some(f)),
        None => (None, None),
    };
    let thread = writer(rx, sink.take());
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
    Guard {
        tx: std::sync::Mutex::new(Some(tx)),
        thread,
    }
}

/// The writer thread: each line to stderr or the file, in the order sent, until the run's end.
fn writer(
    rx: std::sync::mpsc::Receiver<Line>,
    mut sink: Option<std::fs::File>,
) -> Option<std::thread::JoinHandle<()>> {
    use std::io::Write as _;
    std::thread::Builder::new()
        .name("log".into())
        .spawn(move || {
            let mut err = std::io::stderr();
            for line in rx {
                match line {
                    Line::Stderr(b) => {
                        let _ = err.write_all(&b);
                    }
                    Line::File(b) => {
                        if let Some(f) = sink.as_mut() {
                            let _ = f.write_all(&b);
                        }
                    }
                    Line::End => break,
                }
            }
            let _ = err.flush();
            if let Some(f) = sink.as_mut() {
                let _ = f.flush();
            }
        })
        .ok()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own log writer: the run's end is not held up by it)
    use super::{writer, Guard, Line};

    /// Dropping the run's guard ends the writer thread at once, though the installed subscriber
    /// still holds a way to it (as it does for as long as the process lives), and every line sent
    /// before the end is written.
    #[test]
    fn the_runs_end_ends_the_log_writer_while_the_subscriber_still_holds_a_way_to_it() {
        let path = std::env::temp_dir().join(format!("dereth-log-end-{}.log", std::process::id()));
        let file = std::fs::File::create(&path).expect("a scratch file");
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = writer(rx, Some(file));
        let subscriber = tx.clone();
        tx.send(Line::File(b"the last line\n".to_vec()))
            .expect("the writer is there");
        let guard = Guard {
            tx: std::sync::Mutex::new(Some(tx)),
            thread,
        };
        let (done, ended) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(guard);
            let _ = done.send(());
        });
        let returned = ended
            .recv_timeout(std::time::Duration::from_secs(10))
            .is_ok();
        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        drop(subscriber);
        assert!(
            returned,
            "the guard's drop returned: the writer thread ended"
        );
        assert_eq!(
            written, "the last line\n",
            "the lines sent before the end are written"
        );
    }
}
