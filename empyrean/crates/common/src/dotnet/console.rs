//! `System.Console`'s two writes, as the server's log.
//!
//! ACE writes some lines straight to the process console (`Console.WriteLine`, `Console.Write`)
//! instead of through its logger: quest and contract bookkeeping, loot-table parsing, dat dumps,
//! cast records. The port keeps each of those calls, one for one, as
//! [`console_write_line!`](crate::console_write_line!) or [`console_write!`](crate::console_write!),
//! and both route the line through the `log` facade with the target [`TARGET`] (`console`). So a
//! console line is timestamped, levelled and formatted like every other server line, and the
//! server's log level filters it (V383).
//!
//! The level is `info`. A write ACE made only under a `Debug` switch is written with the `debug:`
//! form, at `debug`, so a server logging at `info` does not show it even with the switch on.

use std::fmt;

/// The level a console write is recorded at (the macros name it through this path, so a crate that
/// calls them needs no `log` of its own).
pub use log::Level;

/// The `log` target every console write is recorded under.
pub const TARGET: &str = "console";

/// `Console.WriteLine(text)`: one log record of `args` at `level`, target [`TARGET`].
pub fn write_line(level: log::Level, args: fmt::Arguments<'_>) {
    log::log!(target: TARGET, level, "{args}");
}

/// `Console.Write(text)`: as [`write_line`], without the line ending the text already carries.
///
/// A log record is a line, so the text's own trailing line ending (ACE's multi-line debug dumps end
/// with one) is dropped rather than doubled; line breaks inside it are kept.
pub fn write(level: log::Level, args: fmt::Arguments<'_>) {
    let text = args.to_string();
    log::log!(target: TARGET, level, "{}", text.trim_end_matches(['\r', '\n']));
}

/// `Console.WriteLine(...)` in a port: `console_write_line!("{name} joined")`, or with no
/// arguments for ACE's blank line. `console_write_line!(debug: ...)` is a write ACE made only under
/// a `Debug` switch. See [`crate::dotnet::console`].
#[macro_export]
macro_rules! console_write_line {
    () => {
        $crate::dotnet::console::write_line($crate::dotnet::console::Level::Info, ::core::format_args!(""))
    };
    (debug: $($arg:tt)+) => {
        $crate::dotnet::console::write_line($crate::dotnet::console::Level::Debug, ::core::format_args!($($arg)+))
    };
    ($($arg:tt)+) => {
        $crate::dotnet::console::write_line($crate::dotnet::console::Level::Info, ::core::format_args!($($arg)+))
    };
}

/// `Console.Write(...)` in a port: `console_write!("{}", board)`. See [`crate::dotnet::console`].
#[macro_export]
macro_rules! console_write {
    (debug: $($arg:tt)+) => {
        $crate::dotnet::console::write($crate::dotnet::console::Level::Debug, ::core::format_args!($($arg)+))
    };
    ($($arg:tt)+) => {
        $crate::dotnet::console::write($crate::dotnet::console::Level::Info, ::core::format_args!($($arg)+))
    };
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    /// Every record the test logger saw: (target, level, message).
    static SEEN: Mutex<Vec<(String, log::Level, String)>> = Mutex::new(Vec::new());

    struct Capture;

    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, r: &log::Record<'_>) {
            if r.target() == super::TARGET {
                SEEN.lock().unwrap_or_else(|e| e.into_inner()).push((
                    r.target().to_owned(),
                    r.level(),
                    r.args().to_string(),
                ));
            }
        }
        fn flush(&self) {}
    }

    #[test]
    fn a_console_write_is_a_log_record_on_the_console_target() {
        // The one test in this crate that installs a logger; `log` allows one per process.
        let _ = log::set_logger(&Capture);
        log::set_max_level(log::LevelFilter::Trace);
        let who = "Tester";
        crate::console_write_line!("{who}.QuestManager.EraseAll");
        crate::console_write_line!(debug: "{who}.QuestManager.HasQuest(x): {}", false);
        crate::console_write_line!();
        crate::console_write!("board\n  a b\n");
        let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let want = vec![
            (
                "console".to_owned(),
                log::Level::Info,
                "Tester.QuestManager.EraseAll".to_owned(),
            ),
            (
                "console".to_owned(),
                log::Level::Debug,
                "Tester.QuestManager.HasQuest(x): false".to_owned(),
            ),
            ("console".to_owned(), log::Level::Info, String::new()),
            (
                "console".to_owned(),
                log::Level::Info,
                "board\n  a b".to_owned(),
            ),
        ];
        assert_eq!(seen, want);
    }
}
