//! Error-level log lines are captured separately for each test thread.
//! Fixture: a thread-local buffer around the supplied closure.

use std::cell::RefCell;
use std::sync::Once;

thread_local! {
    /// The lines captured on this thread, while a [`capture`] is running.
    static LINES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Error
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LINES.with_borrow_mut(|l| {
                if let Some(l) = l {
                    l.push(record.args().to_string());
                }
            });
        }
    }

    fn flush(&self) {}
}

/// Runs `f` and returns its result with the error lines it logged on this thread.
pub(crate) fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&Capture).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Error);
    });
    LINES.with_borrow_mut(|l| *l = Some(Vec::new()));
    let out = f();
    let lines = LINES.with_borrow_mut(Option::take).unwrap_or_default();
    (out, lines)
}
