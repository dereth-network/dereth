//! The one thread a test process does its device work on.
//!
//! A test harness runs each test on a thread of its own and ends that thread when the test is
//! done. A thread that has used the graphics driver runs the driver's own clean-up as it ends,
//! inside the system's loader lock, and a driver may wait there for threads of its own while
//! another thread opens a device and loads a library, which needs that same lock: the process
//! stops for good. So a test that makes a device does all its work through [`run`], on one
//! thread kept for the whole process. No thread that touched the driver ends while another opens
//! a device, and the test threads never touch it.
//!
//! One job runs at a time. A job that holds the thread for longer than [`HANG`] ends the process
//! with the test's name, so a hang fails the run with a name rather than stopping it.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// How long one test may hold the device thread before the process is ended, naming it: far
/// longer than any device test takes, even on a loaded machine with a software adapter.
pub const HANG: Duration = Duration::from_secs(600);

/// [`HANG`], or the seconds `DERETH_TEST_DEVICE_HANG` names: the guard's own test sets it short.
fn hang() -> Duration {
    std::env::var("DERETH_TEST_DEVICE_HANG")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(HANG, Duration::from_secs)
}

type Job = Box<dyn FnOnce() + Send>;

/// The test whose job is on the device thread, and since when.
static RUNNING: Mutex<Option<(String, Instant)>> = Mutex::new(None);

std::thread_local! {
    /// Whether this thread is the device thread: a job that asks for it again runs in place.
    static ON_IT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The device thread, started on first use with the watchdog that ends a hung process.
fn jobs() -> &'static Mutex<Sender<Job>> {
    static JOBS: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    JOBS.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        std::thread::Builder::new()
            .name("device tests".to_owned())
            .spawn(move || {
                ON_IT.with(|c| c.set(true));
                for job in rx {
                    job();
                }
            })
            .expect("the device thread starts");
        std::thread::Builder::new()
            .name("device tests' watchdog".to_owned())
            .spawn(|| {
                let hang = hang();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    let held = RUNNING
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .as_ref()
                        .filter(|(_, since)| since.elapsed() > hang)
                        .map(|(name, since)| (name.clone(), since.elapsed()));
                    if let Some((name, for_how_long)) = held {
                        eprintln!(
                            "{name} has held the device thread for {:.0} s: the process is ended, \
                         as a hang in the driver would never end it",
                            for_how_long.as_secs_f64()
                        );
                        // Not an ordinary exit, which takes the loader lock a hang may be holding.
                        std::process::abort();
                    }
                }
            })
            .expect("the watchdog starts");
        Mutex::new(tx)
    })
}

/// `f`'s result, run on the device thread once the jobs before it are done. A panic in `f`
/// is the caller's panic.
///
/// # Panics
/// When `f` does.
pub fn run<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    if ON_IT.with(std::cell::Cell::get) {
        return f();
    }
    let name = std::thread::current()
        .name()
        .unwrap_or("a device test")
        .to_owned();
    let (done, answer) = channel();
    let job: Job = Box::new(move || {
        *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = Some((name, Instant::now()));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let _ = done.send(r);
    });
    jobs()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(job)
        .expect("the device thread takes jobs");
    match answer.recv().expect("the device thread answers") {
        Ok(r) => r,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how this crate's own device tests are run).

    use super::*;

    #[test]
    fn every_job_runs_on_one_thread_and_its_answer_comes_back() {
        let a = run(|| std::thread::current().id());
        let b = std::thread::spawn(|| run(|| std::thread::current().id()))
            .join()
            .expect("a second test");
        assert_eq!(a, b, "two tests' jobs ran on one thread");
        assert_ne!(a, std::thread::current().id(), "not the test's own");
        assert_eq!(run(|| 2 + 2), 4);
    }

    #[test]
    fn a_job_that_asks_for_the_thread_again_runs_in_place() {
        let (outer, inner) = run(|| {
            (
                std::thread::current().id(),
                run(|| std::thread::current().id()),
            )
        });
        assert_eq!(outer, inner);
    }

    /// A job that never ends ends its process, which says which test held the thread; the run
    /// fails then, rather than waiting for it. (The child process is this test binary, running
    /// this one test with the guard set to a second.)
    #[test]
    fn a_job_that_never_ends_ends_the_process_naming_its_test() {
        const NAME: &str =
            "device_thread::tests::a_job_that_never_ends_ends_the_process_naming_its_test";
        if std::env::var_os("DERETH_TEST_DEVICE_HANG").is_some() {
            run(|| loop {
                std::thread::sleep(Duration::from_secs(1));
            });
        }
        let mut child = std::process::Command::new(std::env::current_exe().expect("this binary"))
            .args(["--exact", NAME, "--nocapture"])
            .env("DERETH_TEST_DEVICE_HANG", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        // Only a guard against this test hanging: the child ends a second or two in.
        let started = Instant::now();
        while child.try_wait().expect("the child is there").is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "the guard never ended the child"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let out = child.wait_with_output().expect("the child's output");
        assert!(!out.status.success(), "a hung run fails");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains(&format!("{NAME} has held the device thread")),
            "{said}"
        );
    }

    #[test]
    fn a_panic_in_the_job_is_the_callers() {
        let caught = std::panic::catch_unwind(|| run(|| panic!("the job's own")));
        let message = caught
            .expect_err("the caller panics")
            .downcast::<&str>()
            .map(|s| *s)
            .unwrap_or_default();
        assert_eq!(message, "the job's own");
        assert_eq!(run(|| 1), 1, "and the thread goes on");
    }
}
