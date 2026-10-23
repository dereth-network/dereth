//! The scene's worker threads, started through here so that a host can tell how many of them are
//! running: the mip chains, the landblock bakes, the ground's pictures and vertices, and the far
//! trees, of every scene in the process.

use std::sync::atomic::{AtomicUsize, Ordering};

static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// How many of the scenes' worker threads are running in this process: started, and not yet
/// returned from their work.
#[must_use]
pub fn running() -> usize {
    RUNNING.load(Ordering::SeqCst)
}

/// Start a worker thread named `name` doing `work`, counted from before it starts until it
/// returns (or unwinds).
///
/// # Errors
/// The thread would not start.
pub(crate) fn spawn<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<T>> {
    RUNNING.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _counted = Counted;
            work()
        });
    if spawned.is_err() {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
    spawned
}

/// One running worker, uncounted as its thread leaves its work.
struct Counted;

impl Drop for Counted {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the scene's worker threads, counted)
    use super::*;

    #[test]
    fn a_worker_is_counted_while_it_works() {
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let handle = spawn("counted", move || {
            let _ = wait.recv();
        })
        .unwrap();
        // Other tests' workers may come and go meanwhile; this one is among those counted.
        assert!(running() >= 1);
        go.send(()).unwrap();
        handle.join().unwrap();
    }
}
