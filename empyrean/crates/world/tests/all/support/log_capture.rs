//! Log lines are captured separately for each test thread.
//! Fixture: a thread-local buffer around the supplied closure.

use std::cell::RefCell;
use std::sync::Once;

thread_local! {
    /// The lines captured on this thread, with their levels, while a capture is running.
    static LINES: RefCell<Option<Vec<(log::Level, String)>>> = const { RefCell::new(None) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LINES.with_borrow_mut(|l| {
                if let Some(l) = l {
                    l.push((record.level(), record.args().to_string()));
                }
            });
        }
    }

    fn flush(&self) {}
}

/// Runs `f` and returns its result with the error lines it logged on this thread.
pub(crate) fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    capture_at(log::Level::Error, f)
}

/// Runs `f` and returns its result with the lines it logged on this thread at `level` or more
/// severe (`info` at the most verbose).
pub(crate) fn capture_at<T>(level: log::Level, f: impl FnOnce() -> T) -> (T, Vec<String>) {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&Capture).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Info);
    });
    LINES.with_borrow_mut(|l| *l = Some(Vec::new()));
    let out = f();
    let lines = LINES.with_borrow_mut(Option::take).unwrap_or_default();
    let lines = lines
        .into_iter()
        .filter(|(l, _)| *l <= level)
        .map(|(_, text)| text)
        .collect();
    (out, lines)
}
