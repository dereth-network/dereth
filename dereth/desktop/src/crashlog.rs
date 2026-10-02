//! A crash report on disk, because an intermittent exit otherwise leaves no reason behind.
//!
//! A launcher that starts the client with `Start-Process` and does not redirect its streams
//! sends everything `main` prints -- including the `Error: ...` line on the failure path --
//! to a console that dies with the process. An exit with code **1** would then leave no
//! record of why.
//!
//! So every run appends to a file, and a kept file has **three** states rather than two:
//!
//! | the log says | what happened |
//! |---|---|
//! | `START` then `EXIT code=N` | `main` returned an error; the client itself decided to stop |
//! | `START` then `PANIC` with a backtrace | a Rust panic unwound out of `main` (exit code **101**) |
//! | `START` and nothing else | the process was **killed**; it never reached the end of `main` |
//!
//! The third state is the point. On a machine where several clients run at once, an external
//! process kill and a self-inflicted
//! failure are **indistinguishable from the exit code alone** -- both are 1 -- and only a record
//! written by the process itself can tell them apart.
//!
//! **Only a run that went wrong keeps its log.** A run that reaches the end of `main` with code 0,
//! having recorded no panic on any thread, removes its own file; a non-zero exit, a panic, an
//! unhandled exception or a kill leaves it. Each start also prunes the folder to the newest
//! [`crate::folders::CRASH_LOGS_KEPT`] logs, and the command line is written with the login's
//! values hidden (`dereth_client_runtime::config::argv_for_log`), because a kept log may be shared.

use crate::Product;

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// A panic was recorded, on any thread: the log is kept even if `main` then ends cleanly.
static PANICKED: AtomicBool = AtomicBool::new(false);

/// `<binary>-<pid>.log` in the product's settings directory's `crash-logs` folder
/// ([`crate::folders::crash_log_dir`]): beside the player's other files, and per-pid so that
/// concurrent clients cannot interleave into one file. `None` when the environment names no home,
/// and then no log is kept.
#[must_use]
pub fn path<P: Product>() -> Option<PathBuf> {
    crate::folders::crash_log_dir(P::SETTINGS_DIR_NAME)
        .map(|dir| dir.join(format!("{}-{}.log", P::BINARY_NAME, std::process::id())))
}

/// Seconds since the Unix epoch, so a record can be lined up against a harness transcript.
fn stamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Append one record. This never fails the run and never panics: a diagnostic that can kill
/// the process it is diagnosing is worse than no diagnostic at all.
pub fn append<P: Product>(kind: &str, body: &str) {
    let Some(p) = path::<P>() else {
        return;
    };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let _ = writeln!(f, "[{kind}] unix={:.3} pid={}", stamp(), std::process::id());
        let _ = writeln!(f, "{body}");
        let _ = f.flush();
    }
}

/// Record the start of the run and install the panic hook. Call this first thing in `main`.
pub fn install<P: Product>() {
    let argv: Vec<String> = std::env::args().collect();
    // The login (account, password, ticket) is not written: the log is kept and may be shared.
    let logged = dereth_client_runtime::config::argv_for_log(&argv);
    // The file names itself, so a reader who has the log does not need the launch transcript.
    // **This is where the path goes, and it is deliberately not on stderr.** The path embeds
    // the pid, and printing it unconditionally would break `headless_capture`'s three-run
    // stderr comparison: the pid is the one thing that cannot repeat, and stderr is the
    // channel a determinism test reads.
    append::<P>(
        "START",
        &format!(
            "log = {}\nargv = {logged:?}",
            path::<P>().unwrap_or_default().display()
        ),
    );
    // After this run's own record, so it is the newest and always survives.
    if let Some(dir) = crate::folders::crash_log_dir(P::SETTINGS_DIR_NAME) {
        crate::folders::prune_crash_logs(&dir, P::BINARY_NAME, crate::folders::CRASH_LOGS_KEPT);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `force_capture` ignores `RUST_BACKTRACE`. The moment an intermittent fault finally
        // fires is not the moment to discover the variable was unset.
        let backtrace = std::backtrace::Backtrace::force_capture();
        PANICKED.store(true, Ordering::SeqCst);
        append::<P>(
            "PANIC",
            &format!(
                "{info}
{backtrace}"
            ),
        );
        previous(info);
    }));
    // The crash log's calibration hook, and the reason it lives in production rather than in a
    // test: a zero from an instrument is worth nothing
    // until that instrument has produced a known-positive, and a panic hook is by definition
    // never exercised by a run that goes well. The hidden `--crash-test` switch fires one on
    // demand, so anyone can prove in one command that the hook still records a backtrace
    // before reporting that no crash occurred.
    if dereth_client_runtime::config::crash_test_in_argv(argv.get(1..).unwrap_or_default()) {
        panic!("--crash-test -- a deliberate panic, to prove the hook writes a backtrace");
    }
}

/// The last record of any run that reaches the end of `main`. A clean end -- code 0 and no panic
/// recorded -- removes the log instead: there is nothing in it worth keeping.
pub fn finished<P: Product>(code: i32, detail: &str) {
    if code == 0 && !PANICKED.load(Ordering::SeqCst) {
        if let Some(p) = path::<P>() {
            let _ = std::fs::remove_file(p);
        }
        return;
    }
    append::<P>("EXIT", &format!("code={code} {detail}"));
}
