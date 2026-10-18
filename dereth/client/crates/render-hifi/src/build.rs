//! Pipelines built away from the frame.
//!
//! Building a pipeline compiles its shaders, which takes from a fraction of a millisecond to
//! seconds: on Direct3D 12, whose shader compiler is slow on long shaders, the presentation's
//! longest take seconds each. A frame that built them would stall that long, so they are built
//! ahead, on threads of the warm-up's ([`Warm`]), as soon as the effects that draw with them are
//! turned on. A pass keeps what is built for it in a [`Prebuilt`] slot, takes it from there when
//! it first draws, and sits out the frames before it is built.
//!
//! Every pipeline this crate makes is made through [`Build`], which counts the builds made on each
//! thread ([`built_on_this_thread`]), so a test can see that a frame builds none.
//!
//! On a target without threads (the browser) the builds run when asked, on the caller's thread.

use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Condvar;

thread_local! {
    /// The pipelines made on this thread so far.
    static BUILT: Cell<u64> = const { Cell::new(0) };
}

/// How many pipelines this crate has made on the calling thread so far.
#[must_use]
pub fn built_on_this_thread() -> u64 {
    BUILT.with(Cell::get)
}

fn count() {
    BUILT.with(|b| b.set(b.get() + 1));
}

/// A device's pipelines, made and counted for the thread that makes them.
pub trait Build {
    /// Make a render pipeline, counted for the calling thread.
    fn build_render_pipeline(
        &self,
        desc: &wgpu::RenderPipelineDescriptor<'_>,
    ) -> wgpu::RenderPipeline;

    /// Make a compute pipeline, counted for the calling thread.
    fn build_compute_pipeline(
        &self,
        desc: &wgpu::ComputePipelineDescriptor<'_>,
    ) -> wgpu::ComputePipeline;
}

impl Build for wgpu::Device {
    fn build_render_pipeline(
        &self,
        desc: &wgpu::RenderPipelineDescriptor<'_>,
    ) -> wgpu::RenderPipeline {
        count();
        self.create_render_pipeline(desc)
    }

    fn build_compute_pipeline(
        &self,
        desc: &wgpu::ComputePipelineDescriptor<'_>,
    ) -> wgpu::ComputePipeline {
        count();
        self.create_compute_pipeline(desc)
    }
}

/// One build the warm-up runs.
#[cfg(not(target_arch = "wasm32"))]
type Job = Box<dyn FnOnce() + Send>;

/// What the warm-up's threads share: the builds waiting, how many are not finished, and whether
/// the threads are to stop.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    unfinished: usize,
    /// How many builds the threads are running now.
    running: usize,
    /// Whether no build is to be started.
    held: bool,
    stop: bool,
}

/// The warm-up: builds run on threads of its own, a few at once, and whether they are done.
///
/// Dropping it starts no other build. With none under way it returns once its threads have
/// ended, every object they made released; with one under way (a shader that takes seconds to
/// compile) it returns at once, and what that build makes is released when it ends.
#[derive(Default)]
pub struct Warm {
    #[cfg(not(target_arch = "wasm32"))]
    shared: Arc<(Mutex<Queue>, Condvar)>,
    #[cfg(not(target_arch = "wasm32"))]
    threads: Vec<std::thread::JoinHandle<()>>,
    /// How many builds have been asked for.
    asked: usize,
}

impl std::fmt::Debug for Warm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Warm")
            .field("asked", &self.asked)
            .field("done", &self.done())
            .finish_non_exhaustive()
    }
}

/// How many builds run at once: half the processor's threads, from one to four, so the frame and
/// the landscape's streaming keep theirs.
#[cfg(not(target_arch = "wasm32"))]
fn width() -> usize {
    std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4))
}

impl Warm {
    /// A warm-up with nothing asked for, and no thread until the first build is.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `job` on one of the warm-up's threads.
    pub fn run(&mut self, job: impl FnOnce() + wgpu::WasmNotSend + 'static) {
        self.asked += 1;
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.threads.is_empty() {
                for i in 0..width() {
                    let shared = Arc::clone(&self.shared);
                    if let Ok(t) = std::thread::Builder::new()
                        .name(format!("high-fidelity warm-up {i}"))
                        .spawn(move || work(&shared))
                    {
                        self.threads.push(t);
                    }
                }
            }
            if self.threads.is_empty() {
                // No thread could be started: the build runs here.
                job();
                return;
            }
            let (lock, wake) = &*self.shared;
            if let Ok(mut q) = lock.lock() {
                q.unfinished += 1;
                q.jobs.push_back(Box::new(job));
            }
            wake.notify_one();
        }
        #[cfg(target_arch = "wasm32")]
        job();
    }

    /// How many builds have been asked for.
    #[must_use]
    pub fn asked(&self) -> usize {
        self.asked
    }

    /// How many builds asked for are not finished.
    #[must_use]
    pub fn unfinished(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.shared.0.lock().map_or(0, |q| q.unfinished)
        }
        #[cfg(target_arch = "wasm32")]
        0
    }

    /// Whether every build asked for is finished.
    #[must_use]
    pub fn done(&self) -> bool {
        self.unfinished() == 0
    }

    /// Keep the threads from starting another build (`held`), or let them go on. A held warm-up
    /// does not settle. On a target without threads the builds run when asked, held or not.
    pub fn hold(&self, held: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (lock, wake) = &*self.shared;
            if let Ok(mut q) = lock.lock() {
                q.held = held;
            }
            wake.notify_all();
        }
        #[cfg(target_arch = "wasm32")]
        let _ = held;
    }

    /// Wait up to `timeout` for every build asked for, and whether they are all finished.
    pub fn settle(&self, timeout: Duration) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let deadline = std::time::Instant::now() + timeout;
            let (lock, wake) = &*self.shared;
            let Ok(mut q) = lock.lock() else {
                return true;
            };
            while q.unfinished > 0 {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return false;
                }
                q = match wake.wait_timeout(q, left) {
                    Ok((q, _)) => q,
                    Err(_) => return true,
                };
            }
            true
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = timeout;
            true
        }
    }
}

/// One warm-up thread: run the builds as they come, until told to stop.
#[cfg(not(target_arch = "wasm32"))]
fn work(shared: &(Mutex<Queue>, Condvar)) {
    let (lock, wake) = shared;
    loop {
        let job = {
            let Ok(mut q) = lock.lock() else {
                return;
            };
            loop {
                if q.stop {
                    return;
                }
                if !q.held {
                    if let Some(job) = q.jobs.pop_front() {
                        q.running += 1;
                        break job;
                    }
                }
                q = match wake.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        // A build that panics (a pipeline the device refuses) ends that build alone.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
        if let Ok(mut q) = lock.lock() {
            q.unfinished = q.unfinished.saturating_sub(1);
            q.running = q.running.saturating_sub(1);
        }
        // A thread waiting in `settle` is woken as well as the idle ones.
        wake.notify_all();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Warm {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        let running = match lock.lock() {
            Ok(mut q) => {
                q.stop = true;
                q.unfinished = q.unfinished.saturating_sub(q.jobs.len());
                q.jobs.clear();
                q.running
            }
            Err(_) => 0,
        };
        wake.notify_all();
        // A build under way is left to end on its own rather than hold up whoever lets the
        // warm-up go: the frame that turns the presentation off.
        if running == 0 {
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
        }
    }
}

/// What a pass draws with, built ahead by the warm-up, until the pass takes it.
#[derive(Debug)]
pub struct Prebuilt<T> {
    slot: Arc<Mutex<Slot<T>>>,
    /// What it was last asked for: a key the pass chooses (an art pack's serial, say).
    asked: Option<u64>,
}

/// Where a build leaves what it built.
#[derive(Debug)]
enum Slot<T> {
    /// Not built yet.
    Empty,
    Built(T),
    /// Built and taken by the pass.
    Taken,
    /// The build panicked: the pass makes it itself, as it would without the warm-up, and meets
    /// the same failure there.
    Failed,
}

impl<T> Default for Prebuilt<T> {
    fn default() -> Self {
        Self {
            slot: Arc::new(Mutex::new(Slot::Empty)),
            asked: None,
        }
    }
}

impl<T: wgpu::WasmNotSend + 'static> Prebuilt<T> {
    /// Have `make` build it on the warm-up's threads, unless it was asked for already.
    pub fn ask(&mut self, warm: &mut Warm, make: impl FnOnce() -> T + wgpu::WasmNotSend + 'static) {
        self.ask_for(0, warm, make);
    }

    /// Have `make` build it for `key` on the warm-up's threads, unless it was last asked for that
    /// key. What was built for another key is let go.
    pub fn ask_for(
        &mut self,
        key: u64,
        warm: &mut Warm,
        make: impl FnOnce() -> T + wgpu::WasmNotSend + 'static,
    ) {
        if self.asked == Some(key) {
            return;
        }
        self.asked = Some(key);
        self.slot = Arc::new(Mutex::new(Slot::Empty));
        let slot = Arc::clone(&self.slot);
        warm.run(move || {
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(make));
            if let Ok(mut s) = slot.lock() {
                *s = made.map_or(Slot::Failed, Slot::Built);
            }
        });
    }

    /// Whether it has been asked for.
    #[must_use]
    pub fn is_asked(&self) -> bool {
        self.asked.is_some()
    }

    /// Whether it has been asked for and its build has not ended yet: the pass waits for it.
    #[must_use]
    pub fn waits(&self) -> bool {
        self.is_asked() && self.slot.lock().is_ok_and(|s| matches!(*s, Slot::Empty))
    }

    /// It, once built and not yet taken.
    pub fn take(&mut self) -> Option<T> {
        let mut s = self.slot.lock().ok()?;
        match std::mem::replace(&mut *s, Slot::Taken) {
            Slot::Built(made) => Some(made),
            other => {
                *s = other;
                None
            }
        }
    }
}
