//! The derived pipelines, built off the frame's thread and kept by key.
//!
//! A pipeline is asked for the first time a frame needs it. A worker thread derives its shader
//! module (once per module), builds the pipeline and hands it back; until it arrives, the draws
//! that need it keep their ordinary pipeline and the frame is drawn the ordinary way. Nothing in
//! a frame waits for a build.
//!
//! On a target without threads (the browser) the builds run when asked, on the caller's thread.

use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

use dereth_render::wgpu::sidecar::{build_derived_pipeline, ordinary_blend};
use dereth_render::PipelineKey;

use super::{derive, DerivedKey, ModuleKey, Variant};

/// What a pipeline is built against: the device and the layouts the recorded draws were bound
/// with.
#[derive(Debug, Clone)]
pub struct BuildContext {
    /// The device.
    pub device: wgpu::Device,
    /// The layout of every recorded pipeline but the landscape splat's.
    pub layout: wgpu::PipelineLayout,
    /// The landscape splat's layout, once the device has one.
    pub splat_layout: Option<wgpu::PipelineLayout>,
}

/// One finished build, handed back by the worker.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct Built {
    key: DerivedKey,
    result: Result<wgpu::RenderPipeline, String>,
}

/// The thread that builds.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct Worker {
    jobs: Option<mpsc::Sender<(DerivedKey, BuildContext)>>,
    done: mpsc::Receiver<Built>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Worker {
    fn drop(&mut self) {
        // The thread finishes the build it is on, if any, and nothing after it; every object it
        // made is released before the drop returns.
        self.stop.store(true, Ordering::Release);
        drop(self.jobs.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The derived pipelines, by key.
#[derive(Debug, Default)]
pub struct PipelineCache {
    ready: HashMap<DerivedKey, wgpu::RenderPipeline>,
    asked: HashSet<DerivedKey>,
    failed: HashMap<DerivedKey, String>,
    in_flight: usize,
    /// Builds asked for and not yet sent, in the order asked (the browser builds them here).
    queue: VecDeque<DerivedKey>,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<Worker>,
    #[cfg(target_arch = "wasm32")]
    modules: HashMap<ModuleKey, wgpu::ShaderModule>,
}

/// The keys the device builds when it builds its whole catalogue: every catalogue row in every
/// vertex format, with the base stage chain and fog, unlit.
#[must_use]
pub fn catalogue_keys() -> Vec<PipelineKey> {
    use dereth_render::VertexFormat;
    use dereth_render_cpu::pso::{Cull, StageOps, CATALOGUE};
    CATALOGUE
        .iter()
        .flat_map(|row| {
            VertexFormat::all()
                .into_iter()
                .map(move |vertex_format| PipelineKey {
                    vertex_format,
                    src_blend: row.src,
                    dst_blend: row.dst,
                    alpha_blend: row.alpha_blend,
                    alpha_test: row.alpha_test,
                    z_write: row.z_write,
                    z_func: row.z_func,
                    cull: Cull::Cw,
                    stage_ops: StageOps::BASE,
                    fog: true,
                    lighting: false,
                })
        })
        .collect()
}

/// The colour targets of a derived pipeline of `key`.
fn targets(key: &DerivedKey) -> Result<Vec<Option<wgpu::ColorTargetState>>, String> {
    if key.variant == Variant::Surface {
        return Ok(crate::passes::lighting::derive::targets());
    }
    key.targets(ordinary_blend(&key.key))
        .map(|t| t.into_iter().map(Some).collect())
        .ok_or_else(|| format!("no colour targets for {:?}", key.variant))
}

/// Run `f` inside a validation error scope on this thread, and its result or the device's
/// complaint.
fn scoped<T>(device: &wgpu::Device, f: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let out = f();
    match now(scope.pop()) {
        Some(Some(e)) => Err(e.to_string()),
        _ => Ok(out),
    }
}

/// A future's value if it is ready at once, as the native device's error scopes are.
fn now<F: std::future::Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

/// The module for `module`, derived and made on first use.
fn module_for<'m>(
    modules: &'m mut HashMap<ModuleKey, wgpu::ShaderModule>,
    device: &wgpu::Device,
    module: ModuleKey,
) -> Result<&'m wgpu::ShaderModule, String> {
    match modules.entry(module) {
        std::collections::hash_map::Entry::Occupied(slot) => Ok(slot.into_mut()),
        std::collections::hash_map::Entry::Vacant(slot) => {
            let text = derive(module).map_err(|e| e.to_string())?;
            let made = scoped(device, || {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("high-fidelity derived"),
                    source: wgpu::ShaderSource::Wgsl(text.into()),
                })
            })?;
            Ok(slot.insert(made))
        }
    }
}

/// Build the pipeline of `key` against `cx`, deriving its module into `modules` if need be.
fn build(
    modules: &mut HashMap<ModuleKey, wgpu::ShaderModule>,
    cx: &BuildContext,
    key: DerivedKey,
) -> Result<wgpu::RenderPipeline, String> {
    let layout = if key.splat {
        cx.splat_layout
            .as_ref()
            .ok_or_else(|| "no landscape splat layout yet".to_owned())?
    } else {
        &cx.layout
    };
    let targets = targets(&key)?;
    let module = module_for(modules, &cx.device, key.module())?.clone();
    scoped(&cx.device, || {
        build_derived_pipeline(
            &cx.device,
            layout,
            &module,
            key.fragment_entry(),
            &key.key,
            &targets,
        )
    })
}

impl PipelineCache {
    /// An empty cache, with no worker until the first build is asked for.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The pipeline of `key`, once built.
    #[must_use]
    pub fn get(&self, key: &DerivedKey) -> Option<&wgpu::RenderPipeline> {
        self.ready.get(key)
    }

    /// Whether `key` is built.
    #[must_use]
    pub fn is_ready(&self, key: &DerivedKey) -> bool {
        self.ready.contains_key(key)
    }

    /// Whether `key` was asked for and could not be built.
    #[must_use]
    pub fn has_failed(&self, key: &DerivedKey) -> bool {
        self.failed.contains_key(key)
    }

    /// How many pipelines are built.
    #[must_use]
    pub fn ready_count(&self) -> usize {
        self.ready.len()
    }

    /// How many builds are asked for and not finished.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.in_flight + self.queue.len()
    }

    /// The first build failures, for a report.
    #[must_use]
    pub fn failures(&self) -> Vec<(DerivedKey, String)> {
        self.failed.iter().map(|(k, e)| (*k, e.clone())).collect()
    }

    /// Ask for `key` to be built, unless it was asked for already. A splat key waits until the
    /// device has a splat layout.
    pub fn ask(&mut self, key: DerivedKey) {
        debug_assert!(
            matches!(key.variant, Variant::Reshade | Variant::Surface),
            "only re-shading and surfaces are derived"
        );
        if self.asked.insert(key) {
            self.queue.push_back(key);
        }
    }

    /// Hand the asked-for builds to the worker, and take back what it finished. Never waits.
    pub fn pump(&mut self, cx: &BuildContext) {
        self.collect();
        self.send(cx);
    }

    /// Wait up to `timeout` for every build asked for, and whether none is left.
    pub fn settle(&mut self, timeout: Duration) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let deadline = std::time::Instant::now() + timeout;
            while self.in_flight > 0 {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                let Some(w) = self.worker.as_ref() else {
                    break;
                };
                match w.done.recv_timeout(left) {
                    Ok(b) => self.finish(b),
                    Err(_) => break,
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = timeout;
        self.waiting() == 0
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish(&mut self, b: Built) {
        self.in_flight = self.in_flight.saturating_sub(1);
        match b.result {
            Ok(p) => {
                self.ready.insert(b.key, p);
            }
            Err(e) => {
                self.failed.insert(b.key, e);
            }
        }
    }

    fn collect(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        while let Some(b) = self.worker.as_ref().and_then(|w| w.done.try_recv().ok()) {
            self.finish(b);
        }
    }

    /// Whether `key` can be built against `cx` now.
    fn buildable(key: &DerivedKey, cx: &BuildContext) -> bool {
        !key.splat || cx.splat_layout.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send(&mut self, cx: &BuildContext) {
        if self.queue.is_empty() {
            return;
        }
        if self.worker.is_none() {
            self.worker = Some(spawn());
        }
        let mut later = VecDeque::new();
        while let Some(key) = self.queue.pop_front() {
            if !Self::buildable(&key, cx) {
                later.push_back(key);
                continue;
            }
            let sent = self
                .worker
                .as_ref()
                .and_then(|w| w.jobs.as_ref())
                .is_some_and(|j| j.send((key, cx.clone())).is_ok());
            if sent {
                self.in_flight += 1;
            } else {
                self.failed
                    .insert(key, "the pipeline worker stopped".into());
            }
        }
        self.queue = later;
    }

    #[cfg(target_arch = "wasm32")]
    fn send(&mut self, cx: &BuildContext) {
        let mut later = VecDeque::new();
        while let Some(key) = self.queue.pop_front() {
            if !Self::buildable(&key, cx) {
                later.push_back(key);
                continue;
            }
            match build(&mut self.modules, cx, key) {
                Ok(p) => {
                    self.ready.insert(key, p);
                }
                Err(e) => {
                    self.failed.insert(key, e);
                }
            }
        }
        self.queue = later;
    }
}

/// Start the worker thread.
#[cfg(not(target_arch = "wasm32"))]
fn spawn() -> Worker {
    let (jobs, inbox) = mpsc::channel::<(DerivedKey, BuildContext)>();
    let (outbox, done) = mpsc::channel::<Built>();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("high-fidelity pipelines".into())
        .spawn(move || {
            let mut modules: HashMap<ModuleKey, wgpu::ShaderModule> = HashMap::new();
            while let Ok((key, cx)) = inbox.recv() {
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                let result = build(&mut modules, &cx, key);
                if outbox.send(Built { key, result }).is_err() {
                    break;
                }
            }
        })
        .ok();
    Worker {
        jobs: Some(jobs),
        done,
        stop,
        thread,
    }
}
