//! The compiler Direct3D 12 builds the device's shaders with.
//!
//! On Direct3D 12, `wgpu` compiles a pipeline's shaders as the pipeline is made, with one of two
//! compilers. The client uses FXC, which is part of Windows, but it is slow on long shaders: the
//! high-fidelity presentation's longest take it many seconds each. DXC builds them several times
//! faster, and it is the one that lets the device trace rays, but it is a library of its own,
//! [`DXC_LIBRARY`], which the client does not carry. A player who puts Microsoft's (1.9 or newer)
//! beside the client gets it.
//!
//! So the device takes DXC when that library is beside the running program, loads, and makes a
//! compiler new enough to sign the shaders it builds itself (Direct3D 12 refuses a shader that is
//! not signed). Otherwise it takes FXC: a client without the library, or with a copy that is
//! damaged or too old, comes up as it always has. Only the copy beside the program is ever loaded:
//! one found elsewhere on the search path, an SDK's say, may be too old to sign its shaders.
//!
//! `wgpu` gives every thread that builds a pipeline on the device the same DXC compiler, and DXC's
//! compiler is not safe to call from two threads at once. So every pipeline is built through
//! [`BuildAlone`], and once a device of the process compiles with DXC, that builds one pipeline at
//! a time ([`BUILDS`]).
//!
//! In a test build the test-only `DERETH_TEST_DXC_DIR` names the folder DXC is taken from, when it
//! is set.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

/// The compiler a device on Direct3D 12 builds its shaders with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderCompiler {
    /// DXC, from the library beside the client.
    Dxc,
    /// FXC, which is part of Windows.
    Fxc,
}

impl ShaderCompiler {
    /// Its name, as the adapter's line prints it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dxc => "DXC",
            Self::Fxc => "FXC",
        }
    }
}

/// DXC's library, which the device looks for beside the client.
pub const DXC_LIBRARY: &str = "dxcompiler.dll";

/// The oldest DXC the device takes. DXC signs the shaders it builds itself from its release of
/// February 2025 on, and 1.9 is the first version number every such release carries; an older one
/// needs a second library to sign them, which the client does not carry.
pub const DXC_OLDEST: (u32, u32) = (1, 9);

/// Pipeline builds made one at a time once switched on, and at once before.
#[derive(Debug, Default)]
pub struct OneAtATime {
    on: AtomicBool,
    lock: Mutex<()>,
}

impl OneAtATime {
    /// Off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }

    /// Build one at a time from now on.
    pub fn switch_on(&self) {
        self.on.store(true, Ordering::SeqCst);
    }

    /// Whether builds are made one at a time.
    #[must_use]
    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    /// What one build is made under: the one lock once switched on, nothing before.
    pub fn hold(&self) -> Option<MutexGuard<'_, ()>> {
        self.is_on()
            .then(|| self.lock.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Whether a build could start now: always before it is switched on, and afterwards only when
    /// no build holds it.
    #[must_use]
    pub fn free(&self) -> bool {
        !self.is_on() || !matches!(self.lock.try_lock(), Err(TryLockError::WouldBlock))
    }
}

/// Every pipeline build of the process, one at a time once a device compiles with DXC.
pub static BUILDS: OneAtATime = OneAtATime::new();

/// A device's pipelines, built alone while [`BUILDS`] says so.
pub trait BuildAlone {
    /// Build a render pipeline, alone while [`BUILDS`] says so.
    fn render_pipeline_alone(
        &self,
        desc: &wgpu::RenderPipelineDescriptor<'_>,
    ) -> wgpu::RenderPipeline;

    /// Build a compute pipeline, alone while [`BUILDS`] says so.
    fn compute_pipeline_alone(
        &self,
        desc: &wgpu::ComputePipelineDescriptor<'_>,
    ) -> wgpu::ComputePipeline;
}

impl BuildAlone for wgpu::Device {
    fn render_pipeline_alone(
        &self,
        desc: &wgpu::RenderPipelineDescriptor<'_>,
    ) -> wgpu::RenderPipeline {
        let _alone = BUILDS.hold();
        self.create_render_pipeline(desc)
    }

    fn compute_pipeline_alone(
        &self,
        desc: &wgpu::ComputePipelineDescriptor<'_>,
    ) -> wgpu::ComputePipeline {
        let _alone = BUILDS.hold();
        self.create_compute_pipeline(desc)
    }
}

/// The folder the device looks for [`DXC_LIBRARY`] in: the running program's own (in a test build,
/// the one `DERETH_TEST_DXC_DIR` names, when it is set).
#[must_use]
pub fn dxc_folder() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(dir) = std::env::var_os("DERETH_TEST_DXC_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// `wgpu`'s native instance as the environment asks for it (`WGPU_BACKEND`), with Direct3D 12's
/// compiler chosen: DXC from [`dxc_folder`] where it serves, FXC otherwise. The compiler is `None`
/// where the instance has no Direct3D 12.
#[must_use]
pub fn native_instance() -> (wgpu::InstanceDescriptor, Option<ShaderCompiler>) {
    let desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    #[cfg(windows)]
    if desc.backends.contains(wgpu::Backends::DX12) {
        let (desc, compiler) = with_dxc_in(desc, dxc_folder().as_deref());
        return (desc, Some(compiler));
    }
    (desc, None)
}

/// `desc` with Direct3D 12's compiler chosen with DXC looked for in `folder`: DXC when
/// [`DXC_LIBRARY`] is there, loads, makes a compiler and is [`DXC_OLDEST`] or newer; FXC when it
/// does not, or when `folder` is `None`.
#[cfg(windows)]
#[must_use]
pub fn with_dxc_in(
    mut desc: wgpu::InstanceDescriptor,
    folder: Option<&Path>,
) -> (wgpu::InstanceDescriptor, ShaderCompiler) {
    let found = folder
        .ok_or_else(|| "the client's folder is unknown".to_owned())
        .map(|f| f.join(DXC_LIBRARY))
        .and_then(|path| {
            let version = dxc_version(&path)?;
            if version < DXC_OLDEST {
                return Err(format!(
                    "{} is DXC {}.{}, older than {}.{}",
                    path.display(),
                    version.0,
                    version.1,
                    DXC_OLDEST.0,
                    DXC_OLDEST.1
                ));
            }
            let named = path
                .to_str()
                .ok_or_else(|| format!("{} is not a path DXC can be named by", path.display()))?
                .to_owned();
            Ok((named, version))
        });
    let compiler = match found {
        Ok((dxc_path, (major, minor))) => {
            tracing::info!("Direct3D 12 builds its shaders with DXC {major}.{minor} ({dxc_path})");
            desc.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc { dxc_path };
            ShaderCompiler::Dxc
        }
        Err(why) => {
            tracing::info!("Direct3D 12 builds its shaders with FXC: {why}");
            desc.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
            ShaderCompiler::Fxc
        }
    };
    (desc, compiler)
}

/// The version of the DXC library at `path`, once it has loaded and made a compiler, or why it
/// could not. The library is let go again before this returns: `wgpu` loads it for itself.
#[cfg(windows)]
fn dxc_version(path: &Path) -> Result<(u32, u32), String> {
    use windows::core::{Interface, GUID, HRESULT, HSTRING};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::Graphics::Direct3D::Dxc::{
        CLSID_DxcCompiler, IDxcCompiler3, IDxcVersionInfo,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        SetThreadErrorMode, SEM_FAILCRITICALERRORS, THREAD_ERROR_MODE,
    };
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        LOAD_LIBRARY_SEARCH_SYSTEM32,
    };

    type CreateInstance =
        unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT;

    if !path.is_file() {
        return Err(format!("no {}", path.display()));
    }
    let name = HSTRING::from(path.as_os_str());
    // A damaged library would otherwise make Windows stop the program with an error box of its
    // own; with critical errors failed for this thread, the load just fails and FXC is used.
    let mut before = THREAD_ERROR_MODE(0);
    // SAFETY: `before` outlives the call, which only writes the thread's previous mode into it.
    let quiet = unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, Some(&mut before)) }.is_ok();
    // SAFETY: `name` is a terminated wide string that outlives the call. The flags take the
    // library's own dependencies from its folder and System32 only, never the search path.
    let loaded = unsafe {
        LoadLibraryExW(
            &name,
            None,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if quiet {
        // SAFETY: as above; the thread's mode goes back to what it was.
        let _ = unsafe { SetThreadErrorMode(before, None) };
    }
    let module = loaded.map_err(|e| format!("{} does not load: {e}", path.display()))?;
    let version = (|| {
        // SAFETY: `module` is the library just loaded, held until after this closure returns.
        let create = unsafe { GetProcAddress(module, windows::core::s!("DxcCreateInstance")) }
            .ok_or_else(|| format!("{} is not DXC: it makes no compiler", path.display()))?;
        // SAFETY: DXC's `DxcCreateInstance` has exactly this signature.
        let create: CreateInstance = unsafe { std::mem::transmute(create) };
        let mut raw = std::ptr::null_mut();
        // SAFETY: both identifiers are valid for the call, and `raw` receives an owned reference
        // to the interface asked for when it succeeds.
        unsafe { create(&CLSID_DxcCompiler, &IDxcCompiler3::IID, &mut raw) }
            .ok()
            .map_err(|e| format!("{} makes no compiler: {e}", path.display()))?;
        if raw.is_null() {
            return Err(format!("{} makes no compiler", path.display()));
        }
        // SAFETY: `raw` is the owned `IDxcCompiler3` the call above returned; dropping the wrapper
        // releases it, which happens before the library is let go.
        let compiler = unsafe { IDxcCompiler3::from_raw(raw) };
        let info: IDxcVersionInfo = compiler
            .cast()
            .map_err(|e| format!("{} does not say its version: {e}", path.display()))?;
        let (mut major, mut minor) = (0, 0);
        // SAFETY: both out pointers are valid for the call.
        unsafe { info.GetVersion(&mut major, &mut minor) }
            .map_err(|e| format!("{} does not say its version: {e}", path.display()))?;
        Ok((major, minor))
    })();
    // SAFETY: every object made from the library was released inside the closure above.
    unsafe {
        let _ = FreeLibrary(module);
    }
    version
}

#[cfg(test)]
mod one_at_a_time {
    //! Behaviour: none (the lock every pipeline build takes once a device compiles with DXC).

    use super::OneAtATime;

    #[test]
    fn before_it_is_switched_on_a_build_holds_nothing() {
        let builds = OneAtATime::new();
        let first = builds.hold();
        assert!(first.is_none());
        assert!(builds.free(), "a second build waits for the first");
    }

    #[test]
    fn once_switched_on_no_build_starts_while_another_is_made() {
        let builds = OneAtATime::new();
        builds.switch_on();
        let first = builds.hold();
        assert!(first.is_some(), "the build holds nothing");
        assert!(
            !builds.free(),
            "a second build could start beside the first"
        );
        drop(first);
        assert!(builds.free(), "the build's end lets the next one start");
    }

    /// Every pipeline this crate makes is made through the builders that take the lock.
    #[test]
    fn every_pipeline_the_crate_makes_goes_through_the_one_at_a_time_builders() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut direct = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the source folder reads") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("compiler.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("the source reads");
                for (i, line) in text.lines().enumerate() {
                    if line.contains(".create_render_pipeline(")
                        || line.contains(".create_compute_pipeline(")
                    {
                        direct.push(format!("{}:{}", path.display(), i + 1));
                    }
                }
            }
        }
        assert!(
            direct.is_empty(),
            "pipelines made outside the one-at-a-time builders: {direct:?}"
        );
    }
}

#[cfg(all(test, windows))]
mod tests {
    //! Behaviour: none (choosing the compiler from a folder; the gpu tier tests the device).

    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dereth-render-dxc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    fn chosen(folder: Option<&Path>) -> (ShaderCompiler, wgpu::Dx12Compiler) {
        let (desc, compiler) = with_dxc_in(
            wgpu::InstanceDescriptor::new_without_display_handle(),
            folder,
        );
        (compiler, desc.backend_options.dx12.shader_compiler)
    }

    #[test]
    fn with_no_folder_direct3d_12_builds_with_fxc() {
        let (compiler, told) = chosen(None);
        assert_eq!(compiler, ShaderCompiler::Fxc);
        assert!(matches!(told, wgpu::Dx12Compiler::Fxc), "{told:?}");
    }

    #[test]
    fn a_folder_without_the_library_gives_fxc() {
        let dir = temp_folder("missing");
        let (compiler, told) = chosen(Some(&dir));
        assert_eq!(compiler, ShaderCompiler::Fxc);
        assert!(matches!(told, wgpu::Dx12Compiler::Fxc), "{told:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_file_that_is_not_a_library_gives_fxc() {
        let dir = temp_folder("garbage");
        std::fs::write(dir.join(DXC_LIBRARY), b"not a library at all").expect("written");
        let (compiler, told) = chosen(Some(&dir));
        assert_eq!(compiler, ShaderCompiler::Fxc);
        assert!(matches!(told, wgpu::Dx12Compiler::Fxc), "{told:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_that_makes_no_compiler_gives_fxc() {
        // A real Windows library under DXC's name: it loads, but has nothing to make a compiler.
        let system = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let dir = temp_folder("other");
        std::fs::copy(
            system.join("System32").join("version.dll"),
            dir.join(DXC_LIBRARY),
        )
        .expect("a copy of a Windows library");
        let (compiler, told) = chosen(Some(&dir));
        assert_eq!(compiler, ShaderCompiler::Fxc);
        assert!(matches!(told, wgpu::Dx12Compiler::Fxc), "{told:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
