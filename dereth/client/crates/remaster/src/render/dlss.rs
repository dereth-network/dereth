//! NVIDIA DLSS Super Resolution, through the NGX library the graphics driver carries. It is loaded
//! only in a build with the `dlss` feature, on a Windows machine whose driver has the library, with
//! NVIDIA's `nvngx_dlss.dll` beside the executable, as the Windows release carries it.
//! Anywhere else [`Ngx::load`] finds nothing, and the upscaler's DLSS modes are drawn by FSR.
//!
//! The library is asked for by name and its calls are laid out here, as its public headers give
//! them; a parameter block is a C++ object, so its setters and getters are called through its
//! table of virtual functions.

#![allow(unsafe_code)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;

use ash::vk::{self, Handle};
use libloading::Library;

/// The DLL that must stand beside the executable.
pub const DLL: &str = "nvngx_dlss.dll";

/// The library the driver carries.
const LIBRARY: &str = "_nvngx.dll";

/// The version of the library's calls this module speaks.
const SDK_VERSION: i32 = 0x15;
/// The super resolution feature.
const FEATURE_SUPER_SAMPLING: i32 = 1;
/// The client's own identifier, for a project with no application id from NVIDIA. The library takes
/// anything shaped like a GUID.
const PROJECT_ID: &CStr = c"43df4e50-60f4-4b56-8caf-a0ce8c4bd839";
/// The engine type of a project on no engine the library knows.
const ENGINE_CUSTOM: i32 = 0;
/// How much the library says through the log callback: what it would log by default.
const LOGGING_ON: i32 = 1;

/// What is asked of the feature when it is made.
mod flags {
    pub const IS_HDR: i32 = 1 << 0;
    pub const MV_LOW_RES: i32 = 1 << 1;
    pub const AUTO_EXPOSURE: i32 = 1 << 6;
}

/// Places in a parameter block's table of virtual functions: the compiler lays each overloaded
/// name's functions out in reverse order of their declaration.
mod slot {
    pub const SET_POINTER: usize = 0;
    pub const SET_I32: usize = 3;
    pub const SET_U32: usize = 4;
    pub const SET_F32: usize = 6;
    pub const GET_I32: usize = 11;
}

fn succeeded(result: u32) -> bool {
    result & 0xFFF0_0000 != 0xBAD0_0000
}

/// A parameter block the library made.
#[repr(C)]
struct Parameter {
    table: *const *const c_void,
}

impl Parameter {
    /// # Safety
    /// `at` is a setter's place, taking a value of `V`.
    unsafe fn set<V>(this: *mut Self, at: usize, name: &CStr, value: V) {
        // SAFETY: the block is the library's own, and `at` a setter's place, as the caller says.
        unsafe {
            let f: unsafe extern "system" fn(*mut Self, *const c_char, V) =
                std::mem::transmute(*(*this).table.add(at));
            f(this, name.as_ptr(), value);
        }
    }

    fn set_i32(this: *mut Self, name: &CStr, value: i32) {
        // SAFETY: the place of the setter of a 32-bit integer.
        unsafe { Self::set(this, slot::SET_I32, name, value) }
    }

    fn set_u32(this: *mut Self, name: &CStr, value: u32) {
        // SAFETY: the place of the setter of an unsigned 32-bit integer.
        unsafe { Self::set(this, slot::SET_U32, name, value) }
    }

    fn set_f32(this: *mut Self, name: &CStr, value: f32) {
        // SAFETY: the place of the setter of a float.
        unsafe { Self::set(this, slot::SET_F32, name, value) }
    }

    fn set_pointer(this: *mut Self, name: &CStr, value: *mut c_void) {
        // SAFETY: the place of the setter of a pointer.
        unsafe { Self::set(this, slot::SET_POINTER, name, value) }
    }

    fn get_i32(this: *mut Self, name: &CStr) -> Option<i32> {
        let mut out = 0;
        // SAFETY: the block is the library's own; the place of the getter of a 32-bit integer.
        let result = unsafe {
            let f: unsafe extern "system" fn(*mut Self, *const c_char, *mut i32) -> u32 =
                std::mem::transmute(*(*this).table.add(slot::GET_I32));
            f(this, name.as_ptr(), &raw mut out)
        };
        succeeded(result).then_some(out)
    }
}

/// An image as the library takes it, read or read and written.
#[repr(C)]
#[derive(Clone, Copy)]
struct Resource {
    view: u64,
    image: u64,
    range: vk::ImageSubresourceRange,
    format: i32,
    width: u32,
    height: u32,
    /// The union's other member, a buffer's, is smaller; `kind` follows the union.
    kind: i32,
    read_write: bool,
}

/// The images a frame hands the library.
#[derive(Debug, Clone, Copy)]
pub struct Images {
    pub color: Image,
    pub output: Image,
    pub depth: Image,
    pub motion: Image,
}

/// An image the frame hands the library.
#[derive(Debug, Clone, Copy)]
pub struct Image {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub format: vk::Format,
    pub size: (u32, u32),
}

impl Image {
    fn resource(self, read_write: bool) -> Resource {
        Resource {
            view: self.view.as_raw(),
            image: self.image.as_raw(),
            range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            format: self.format.as_raw(),
            width: self.size.0,
            height: self.size.1,
            kind: 0,
            read_write,
        }
    }
}

type Init = unsafe extern "system" fn(
    *const c_char,
    i32,
    *const c_char,
    *const u16,
    u64,
    u64,
    u64,
    *const c_void,
    *const c_void,
    *const FeatureInfo,
    i32,
) -> u32;
type Capabilities = unsafe extern "system" fn(*mut *mut Parameter) -> u32;
type Allocate = unsafe extern "system" fn(*mut *mut Parameter) -> u32;
type Destroy = unsafe extern "system" fn(*mut Parameter) -> u32;
type Create = unsafe extern "system" fn(u64, u64, i32, *mut Parameter, *mut *mut c_void) -> u32;
type Release = unsafe extern "system" fn(*mut c_void) -> u32;
type Evaluate = unsafe extern "system" fn(u64, *mut c_void, *mut Parameter, *const c_void) -> u32;
type Required = unsafe extern "system" fn(
    *mut u32,
    *mut *const *const c_char,
    *mut u32,
    *mut *const *const c_char,
) -> u32;

/// The folders the library looks for the feature's DLL in, and where it says what it is doing.
#[repr(C)]
struct FeatureInfo {
    paths: *const *const u16,
    path_count: u32,
    internal: *mut c_void,
    log: Option<unsafe extern "system" fn(*const c_char, i32, i32)>,
    log_level: i32,
    only_the_callback: bool,
}

/// Whether the library was started in this process, so it is in it until the process ends.
static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ends the process now with `code` where the library was started in it, without the libraries'
/// own goodbyes; returns where it was not, so the process ends as any other. A release build,
/// its C runtime linked in, ends with an access violation in those goodbyes once the library has
/// started, after everything of the client's own is closed. Called once that is so.
pub fn end_process_if_started(code: u32) {
    if !RAN.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    type Current = unsafe extern "system" fn() -> isize;
    type Terminate = unsafe extern "system" fn(isize, u32) -> i32;
    // SAFETY: Windows' own calls, as its headers give them, on this process's own handle.
    unsafe {
        let Ok(kernel) = Library::new("kernel32.dll") else {
            return;
        };
        let (Ok(current), Ok(terminate)) = (
            kernel.get::<Current>(b"GetCurrentProcess\0"),
            kernel.get::<Terminate>(b"TerminateProcess\0"),
        ) else {
            return;
        };
        terminate(current(), code);
    }
}

/// Whether the library is being let go: from then on what it says is not passed on, since its own
/// threads may still speak as the process ends.
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the library says, in the client's log. It may call this from any thread.
unsafe extern "system" fn library_said(message: *const c_char, _level: i32, _feature: i32) {
    if QUIET.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    // SAFETY: a string the library keeps valid until this returns, as its headers say.
    let message = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    tracing::info!(target: "dereth::remaster", "NGX: {}", message.trim_end());
}

/// Starts the library on a device, looking for the feature's DLL beside the executable, and gives
/// its result.
///
/// # Safety
/// `lib` is the driver's library, and the handles are live and of one device.
unsafe fn start(
    lib: &Library,
    data: &std::path::Path,
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
) -> Option<u32> {
    let wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect()
    };
    // What the library is pointed at lives as long as the process: it may keep the pointers past
    // the call, and use them as late as the process's end.
    let here = std::env::current_exe().ok()?;
    let here: &'static [u16] = Vec::leak(wide(here.parent()?));
    let data: &'static [u16] = Vec::leak(wide(data));
    let version: &'static CStr = Box::leak(
        CString::new(env!("CARGO_PKG_VERSION"))
            .ok()?
            .into_boxed_c_str(),
    );
    let paths: &'static [*const u16] = Vec::leak(vec![here.as_ptr()]);
    let info: &'static FeatureInfo = Box::leak(Box::new(FeatureInfo {
        paths: paths.as_ptr(),
        path_count: 1,
        internal: std::ptr::null_mut(),
        log: Some(library_said),
        log_level: LOGGING_ON,
        only_the_callback: false,
    }));
    // SAFETY: the call as the library's headers give it, as the caller says of its handles; what
    // it is pointed at lives as long as the process.
    unsafe {
        let init: Init = *lib.get(b"NVSDK_NGX_VULKAN_Init_ProjectID\0").ok()?;
        Some(init(
            PROJECT_ID.as_ptr(),
            ENGINE_CUSTOM,
            version.as_ptr(),
            data.as_ptr(),
            instance.as_raw(),
            physical.as_raw(),
            device.as_raw(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::from_ref(info),
            SDK_VERSION,
        ))
    }
}

/// What the library's capability block says of DLSS, in words for the log.
fn capability_report(caps: *mut Parameter) -> String {
    let read = |name: &CStr| Parameter::get_i32(caps, name);
    let result = read(c"SuperSampling.FeatureInitResult").map(|r| format!("{:#X}", r as u32));
    format!(
        "available {:?}, feature init result {result:?}, needs an updated driver {:?}, minimum driver {:?}.{:?}",
        read(c"SuperSampling.Available"),
        read(c"SuperSampling.NeedsUpdatedDriver"),
        read(c"SuperSampling.MinDriverVersionMajor"),
        read(c"SuperSampling.MinDriverVersionMinor"),
    )
}

/// A text value of the machine's registry: the first, of a list.
fn registry_text(key: &str, value: &str) -> Option<String> {
    // SAFETY: the system's own library.
    let advapi = (unsafe { Library::new("advapi32.dll") }).ok()?;
    type Get = unsafe extern "system" fn(
        isize,
        *const u16,
        *const u16,
        u32,
        *mut u32,
        *mut u16,
        *mut u32,
    ) -> i32;
    const LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
    const TEXT_OR_LIST: u32 = 0x2 | 0x20;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, value) = (wide(key), wide(value));
    let mut text = vec![0u16; 1024];
    let mut bytes = 2048u32;
    // SAFETY: the system's own call, its text bounded by the size given.
    let result = unsafe {
        let get: libloading::Symbol<Get> = advapi.get(b"RegGetValueW\0").ok()?;
        get(
            LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            TEXT_OR_LIST,
            std::ptr::null_mut(),
            text.as_mut_ptr(),
            &raw mut bytes,
        )
    };
    if result != 0 {
        return None;
    }
    let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..end]))
}

/// The first of `folders` that holds the driver's library.
fn folder_with_library(folders: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    folders.into_iter().find(|f| f.join(LIBRARY).is_file())
}

/// The folder the driver keeps the library in: where the driver's own install records it, or, when
/// that names a driver since replaced, where an NVIDIA display adapter's driver is.
fn driver_folder() -> Option<PathBuf> {
    const DISPLAY_ADAPTERS: &str =
        "SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let recorded = registry_text("SOFTWARE\\NVIDIA Corporation\\Global\\NGXCore", "FullPath")
        .map(PathBuf::from);
    let adapters = (0..16).filter_map(|n| {
        let key = format!("{DISPLAY_ADAPTERS}\\{n:04}");
        registry_text(&key, "DriverDesc").filter(|d| d.contains("NVIDIA"))?;
        let driver = PathBuf::from(registry_text(&key, "OpenGLDriverName")?);
        driver.parent().map(PathBuf::from)
    });
    folder_with_library(recorded.into_iter().chain(adapters))
}

/// Whether the DLL stands beside the executable.
#[must_use]
pub fn dll_beside_client() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(DLL).is_file()))
        .unwrap_or(false)
}

/// The driver's library, when the DLL stands beside the executable and the driver has it.
fn library() -> Option<Library> {
    if !dll_beside_client() {
        return None;
    }
    let Some(folder) = driver_folder() else {
        tracing::info!(target: "dereth::remaster", "DLSS: the driver's library was not found");
        return None;
    };
    // SAFETY: the driver's own library, whose start-up does nothing the client depends on.
    unsafe { Library::new(folder.join(LIBRARY)) }
        .map_err(|e| tracing::info!(target: "dereth::remaster", "DLSS: the driver's library did not load: {e}"))
        .ok()
}

/// The extensions of Vulkan's instance and device the library needs: nothing where it will not be
/// loaded. A device made without them draws its DLSS modes with FSR.
#[must_use]
pub fn extensions() -> (&'static [&'static CStr], &'static [&'static CStr]) {
    static ASKED: std::sync::OnceLock<(Vec<&'static CStr>, Vec<&'static CStr>)> =
        std::sync::OnceLock::new();
    let (i, d) = ASKED.get_or_init(asked_extensions);
    (i, d)
}

fn asked_extensions() -> (Vec<&'static CStr>, Vec<&'static CStr>) {
    let Some(lib) = library() else {
        return (Vec::new(), Vec::new());
    };
    let (mut ni, mut nd) = (0u32, 0u32);
    let (mut i, mut d): (*const *const c_char, *const *const c_char) =
        (std::ptr::null(), std::ptr::null());
    // SAFETY: the library's call, which hands back lists of its own strings.
    let result = unsafe {
        let Ok(f) = lib.get::<Required>(b"NVSDK_NGX_VULKAN_RequiredExtensions\0") else {
            return (Vec::new(), Vec::new());
        };
        f(&raw mut ni, &raw mut i, &raw mut nd, &raw mut d)
    };
    if !succeeded(result) {
        return (Vec::new(), Vec::new());
    }
    let list = |n: u32, p: *const *const c_char| -> Vec<&'static CStr> {
        (0..n as usize)
            .map(|k| {
                // SAFETY: `n` strings, as the library said. They are kept for the whole run.
                let name = unsafe { CStr::from_ptr(*p.add(k)) };
                &*Box::leak(CString::from(name).into_boxed_c_str())
            })
            .collect()
    };
    let asked = (list(ni, i), list(nd, d));
    tracing::info!(target: "dereth::remaster", "DLSS asks for instance extensions {:?} and device extensions {:?}", asked.0, asked.1);
    asked
}

/// What was started on the device in use, so a remaster made again on it keeps what it has.
struct Started<T> {
    device: Option<u64>,
    held: Option<std::sync::Arc<T>>,
}

impl<T> Started<T> {
    /// What runs on `device`, started only if nothing has been tried on it since it came. A start
    /// that failed is not tried again.
    fn on(&mut self, device: u64, start: impl FnOnce() -> Option<T>) -> Option<std::sync::Arc<T>> {
        if self.device != Some(device) {
            self.device = Some(device);
            self.held = start().map(std::sync::Arc::new);
        }
        self.held.clone()
    }

    /// What ran on `device`, which is closing. A later device starts anew.
    fn closing(&mut self, device: u64) -> Option<std::sync::Arc<T>> {
        if self.device != Some(device) {
            return None;
        }
        self.device = None;
        self.held.take()
    }
}

/// The one start per device, for the whole process: the remaster comes and goes on a device.
static STARTED: std::sync::Mutex<Started<Ngx>> = std::sync::Mutex::new(Started {
    device: None,
    held: None,
});

/// A device is closing, idle: what runs on it stops while it is still alive.
fn device_closing(device: u64) {
    let closing = STARTED.lock().ok().and_then(|mut s| s.closing(device));
    if let Some(ngx) = closing {
        ngx.shut_down();
    }
}

/// The driver's library, started on the frame's device.
///
/// The library's shutdown call is never made: it ends the process with an access violation, as a
/// start that found DLSS unavailable showed on driver 621.14 and a release build's exit shows on
/// later drivers. A ready start lets its feature and parameters go when its device closes
/// ([`Ngx::shut_down`]). The library itself is never unloaded once started; the system takes it
/// back at exit.
pub struct Ngx {
    _lib: std::mem::ManuallyDrop<Library>,
    device: u64,
    destroy: Destroy,
    create: Create,
    release: Release,
    evaluate: Evaluate,
    params: *mut Parameter,
    /// The feature at the sizes last drawn.
    feature: std::sync::Mutex<Option<Feature>>,
    /// Whether a frame has been drawn yet, to say so once.
    drawn: std::sync::atomic::AtomicBool,
    /// Whether it has shut down, after which nothing more is drawn.
    closed: std::sync::atomic::AtomicBool,
}

// SAFETY: the library's handles are only ever used under the renderer's own recording, one frame
// at a time; the feature is behind its lock.
unsafe impl Send for Ngx {}
// SAFETY: as for `Send`.
unsafe impl Sync for Ngx {}

impl std::fmt::Debug for Ngx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ngx")
    }
}

impl Ngx {
    /// The library on a device, started if nothing has been tried on that device yet. `None` where
    /// DLSS will not draw; the remaster then draws its DLSS modes with FSR.
    #[must_use]
    pub fn on_device(
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
        device: vk::Device,
        data: &std::path::Path,
    ) -> Option<std::sync::Arc<Self>> {
        STARTED.lock().ok()?.on(device.as_raw(), || {
            let ngx = Self::load(instance, physical, device, data)?;
            dereth_render::vulkan::world_hook::when_a_device_closes(device_closing);
            Some(ngx)
        })
    }

    /// Releases the feature and the parameter block, on an idle device still alive.
    fn shut_down(&self) {
        use std::sync::atomic::Ordering;
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        tracing::info!(target: "dereth::remaster", "DLSS: its device is closing");
        // Each step said before it is taken, so a log cut short says which one ended the process.
        // SAFETY: the handles are this library's, once each: `closed` stops any more drawing.
        unsafe {
            if let Some(f) = self.feature.lock().ok().and_then(|mut f| f.take()) {
                tracing::info!(target: "dereth::remaster", "DLSS: releasing the feature");
                let result = (self.release)(f.handle);
                tracing::info!(target: "dereth::remaster", "DLSS: the feature released: {result:#X}");
            }
            tracing::info!(target: "dereth::remaster", "DLSS: destroying the parameter block");
            (self.destroy)(self.params);
            // The library's own shutdown is not called: on a release build it ends the process
            // with an access violation (driver 32.0.16.1088, DLSS 310.9.16), as a start that found
            // DLSS unavailable does on driver 621.14. The device closes only as the process ends,
            // which takes the library back.
            QUIET.store(true, std::sync::atomic::Ordering::SeqCst);
            tracing::info!(target: "dereth::remaster", "DLSS: the feature and its parameters are let go");
        }
    }

    /// The library on a device, when it stands ready and says DLSS is available on it; `None`
    /// otherwise, and the reason as a line in the log. `data` is a folder it may write its log to.
    #[must_use]
    fn load(
        instance: vk::Instance,
        physical: vk::PhysicalDevice,
        device: vk::Device,
        data: &std::path::Path,
    ) -> Option<Self> {
        let lib = library()?;
        // SAFETY: the calls are as the library's headers give them; a name it lacks ends the load.
        unsafe {
            let capabilities: Capabilities = *lib
                .get(b"NVSDK_NGX_VULKAN_GetCapabilityParameters\0")
                .ok()?;
            let allocate: Allocate = *lib.get(b"NVSDK_NGX_VULKAN_AllocateParameters\0").ok()?;
            let destroy: Destroy = *lib.get(b"NVSDK_NGX_VULKAN_DestroyParameters\0").ok()?;
            let create: Create = *lib.get(b"NVSDK_NGX_VULKAN_CreateFeature1\0").ok()?;
            let release: Release = *lib.get(b"NVSDK_NGX_VULKAN_ReleaseFeature\0").ok()?;
            let evaluate: Evaluate = *lib.get(b"NVSDK_NGX_VULKAN_EvaluateFeature\0").ok()?;
            let result = start(&lib, data, instance, physical, device)?;
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            // From here it stays for the process's life, on every path: see [`Ngx`].
            let lib = std::mem::ManuallyDrop::new(lib);
            if !succeeded(result) {
                tracing::info!(target: "dereth::remaster", "DLSS: the library did not start: {result:#X}");
                return None;
            }
            let mut caps = std::ptr::null_mut();
            let found = capabilities(&raw mut caps);
            let available = succeeded(found)
                && !caps.is_null()
                && Parameter::get_i32(caps, c"SuperSampling.Available") == Some(1);
            if !caps.is_null() {
                let said = capability_report(caps);
                tracing::info!(target: "dereth::remaster", "DLSS capabilities: {said}");
                destroy(caps);
            }
            if !available {
                tracing::info!(target: "dereth::remaster", "DLSS is not available on this device ({found:#X})");
                return None;
            }
            let mut params = std::ptr::null_mut();
            if !succeeded(allocate(&raw mut params)) || params.is_null() {
                tracing::info!(target: "dereth::remaster", "DLSS: no parameter block");
                return None;
            }
            tracing::info!(target: "dereth::remaster", "DLSS is ready on this device");
            Some(Self {
                _lib: lib,
                device: device.as_raw(),
                destroy,
                create,
                release,
                evaluate,
                params,
                feature: std::sync::Mutex::new(None),
                drawn: std::sync::atomic::AtomicBool::new(false),
                closed: std::sync::atomic::AtomicBool::new(false),
            })
        }
    }

    /// The feature at the sizes given, made in the command buffer `cmd`.
    fn make_feature(
        &self,
        cmd: vk::CommandBuffer,
        render: (u32, u32),
        display: (u32, u32),
        hdr: bool,
    ) -> Option<Feature> {
        let p = self.params;
        Parameter::set_u32(p, c"CreationNodeMask", 1);
        Parameter::set_u32(p, c"VisibilityNodeMask", 1);
        Parameter::set_u32(p, c"Width", render.0);
        Parameter::set_u32(p, c"Height", render.1);
        Parameter::set_u32(p, c"OutWidth", display.0);
        Parameter::set_u32(p, c"OutHeight", display.1);
        Parameter::set_i32(p, c"PerfQualityValue", quality(render.0, display.0));
        Parameter::set_i32(
            p,
            c"DLSS.Feature.Create.Flags",
            flags::MV_LOW_RES | flags::AUTO_EXPOSURE | if hdr { flags::IS_HDR } else { 0 },
        );
        Parameter::set_i32(p, c"DLSS.Enable.Output.Subrects", 0);
        let mut handle = std::ptr::null_mut();
        // SAFETY: as the library's headers give the call; the command buffer is recording.
        let result = unsafe {
            (self.create)(
                self.device,
                cmd.as_raw(),
                FEATURE_SUPER_SAMPLING,
                p,
                &raw mut handle,
            )
        };
        if succeeded(result) && !handle.is_null() {
            let (from, to) = (render, display);
            tracing::info!(target: "dereth::remaster", "DLSS: the feature was made, {from:?} to {to:?}");
            Some(Feature {
                handle,
                render,
                display,
                hdr,
            })
        } else {
            tracing::info!(target: "dereth::remaster", "DLSS: the feature was not made: {result:#X}");
            None
        }
    }

    /// One frame of the feature, made first for these sizes if need be, in `cmd`, whose images
    /// stand as the library wants them: the inputs read by shaders, the output in the general
    /// layout.
    #[allow(clippy::too_many_arguments)] // the frame's inputs, each its own
    pub fn draw(
        &self,
        cmd: vk::CommandBuffer,
        render: (u32, u32),
        display: (u32, u32),
        hdr: bool,
        images: Images,
        jitter: [f32; 2],
        reset: bool,
    ) {
        use std::sync::atomic::Ordering;
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut held) = self.feature.lock() else {
            return;
        };
        if held
            .as_ref()
            .is_some_and(|f| (f.render, f.display, f.hdr) != (render, display, hdr))
        {
            if let Some(f) = held.take() {
                // SAFETY: a handle this library gave.
                unsafe {
                    (self.release)(f.handle);
                }
            }
        }
        if held.is_none() {
            *held = self.make_feature(cmd, render, display, hdr);
        }
        let Some(feature) = held.as_ref() else {
            return;
        };
        let p = self.params;
        let mut c = images.color.resource(false);
        let mut o = images.output.resource(true);
        let mut d = images.depth.resource(false);
        let mut m = images.motion.resource(false);
        Parameter::set_pointer(p, c"Color", (&raw mut c).cast());
        Parameter::set_pointer(p, c"Output", (&raw mut o).cast());
        Parameter::set_pointer(p, c"Depth", (&raw mut d).cast());
        Parameter::set_pointer(p, c"MotionVectors", (&raw mut m).cast());
        Parameter::set_f32(p, c"Jitter.Offset.X", jitter[0]);
        Parameter::set_f32(p, c"Jitter.Offset.Y", jitter[1]);
        Parameter::set_i32(p, c"Reset", i32::from(reset));
        Parameter::set_f32(p, c"MV.Scale.X", 1.0);
        Parameter::set_f32(p, c"MV.Scale.Y", 1.0);
        Parameter::set_f32(p, c"Sharpness", 0.0);
        Parameter::set_u32(p, c"DLSS.Render.Subrect.Dimensions.Width", render.0);
        Parameter::set_u32(p, c"DLSS.Render.Subrect.Dimensions.Height", render.1);
        Parameter::set_f32(p, c"DLSS.Pre.Exposure", 1.0);
        Parameter::set_f32(p, c"DLSS.Exposure.Scale", 1.0);
        // SAFETY: as the library's headers give the call; the resources live through the call,
        // which reads them as it records.
        let result = unsafe { (self.evaluate)(cmd.as_raw(), feature.handle, p, std::ptr::null()) };
        if !succeeded(result) {
            tracing::info!(target: "dereth::remaster", "DLSS did not draw: {result:#X}");
        } else if !self.drawn.swap(true, Ordering::SeqCst) {
            let (from, to) = (render, display);
            tracing::info!(target: "dereth::remaster", "DLSS drew its first frame, {from:?} to {to:?}");
        }
    }
}

/// The feature at one pair of sizes.
struct Feature {
    handle: *mut c_void,
    render: (u32, u32),
    display: (u32, u32),
    hdr: bool,
}

/// The quality the library is told for a render width of a display's width. It takes every size
/// between; this only names the nearest of its modes.
fn quality(render: u32, display: u32) -> i32 {
    const MAX_PERF: i32 = 0;
    const BALANCED: i32 = 1;
    const MAX_QUALITY: i32 = 2;
    const ULTRA_PERFORMANCE: i32 = 3;
    const DLAA: i32 = 5;
    #[allow(clippy::cast_precision_loss)] // targets' widths
    let share = render as f32 / display.max(1) as f32;
    if share >= 0.99 {
        DLAA
    } else if share >= 0.62 {
        MAX_QUALITY
    } else if share >= 0.55 {
        BALANCED
    } else if share >= 0.45 {
        MAX_PERF
    } else {
        ULTRA_PERFORMANCE
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A failure is the library's mark on the high bits, and anything else succeeded.
    #[test]
    fn the_librarys_results_read_as_its_headers_say() {
        assert!(succeeded(1));
        assert!(!succeeded(0xBAD0_0001));
        assert!(!succeeded(0xBAD0_0000));
    }

    /// The modes named for the render scale's choices.
    #[test]
    fn the_quality_named_is_the_nearest_to_the_share_drawn() {
        assert_eq!(quality(1000, 1000), 5);
        assert_eq!(quality(670, 1000), 2);
        assert_eq!(quality(500, 1000), 0);
        assert_eq!(quality(340, 1000), 3);
    }

    /// One start per device, whether it worked or not, until that device closes; a remaster made
    /// again on the device takes what was started.
    #[test]
    fn a_device_is_started_on_once_until_it_closes() {
        let mut started = Started {
            device: None,
            held: None,
        };
        let mut starts = 0;
        let mut start = |worked: bool| {
            starts += 1;
            worked.then_some(starts)
        };
        let first = started.on(1, || start(true)).expect("it started");
        let again = started.on(1, || start(true)).expect("what it started");
        assert!(std::sync::Arc::ptr_eq(&first, &again));
        assert!(started.closing(2).is_none(), "another device's closing");
        assert!(started.closing(1).is_some(), "what ran on it stops");
        assert!(started.closing(1).is_none(), "only once");
        assert_eq!(started.on(1, || start(false)), None);
        assert_eq!(
            started.on(1, || start(true)),
            None,
            "a failure isn't tried again"
        );
        assert!(started.closing(1).is_none(), "nothing ran to stop");
        assert_eq!(started.on(2, || start(true)).as_deref(), Some(&3));
    }

    /// A folder a replaced driver left behind is passed over for the one that has the library.
    #[test]
    fn a_driver_folder_left_behind_is_passed_over_for_the_one_with_the_library() {
        let root = std::env::temp_dir().join(format!("dereth-dlss-{}", std::process::id()));
        let (gone, empty, current) = (root.join("gone"), root.join("empty"), root.join("current"));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join(LIBRARY), b"library").unwrap();
        let found = folder_with_library([gone.clone(), empty.clone(), current.clone()]);
        let none = folder_with_library([gone, empty]);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, Some(current));
        assert_eq!(none, None);
    }

    /// The driver's library starts on a device, and a parameter block reads back what is set in it
    /// through the places this module names, whether or not the DLL is there to draw.
    #[test]
    #[ignore = "needs an NVIDIA driver and the gpu to itself; run with --ignored"]
    fn the_drivers_library_starts_and_a_parameter_block_reads_back_what_is_set() {
        let folder = driver_folder().expect("the driver's folder");
        // SAFETY: the installed driver's own library.
        let lib = unsafe { Library::new(folder.join(LIBRARY)) }.expect("the library");
        let gfx = dereth_gfx::Device::create(&dereth_gfx::DeviceDesc::default()).expect("a device");
        let (instance, physical) = gfx.raw_instance().expect("Vulkan");
        let device = gfx.raw().expect("Vulkan").handle();
        // SAFETY: as the library's headers give the calls.
        unsafe {
            let allocate: Allocate = *lib
                .get(b"NVSDK_NGX_VULKAN_AllocateParameters\0")
                .expect("allocate");
            let destroy: Destroy = *lib
                .get(b"NVSDK_NGX_VULKAN_DestroyParameters\0")
                .expect("destroy");
            let capabilities: Capabilities = *lib
                .get(b"NVSDK_NGX_VULKAN_GetCapabilityParameters\0")
                .expect("capabilities");
            let result = start(
                &lib,
                &std::env::temp_dir(),
                instance,
                physical,
                vk::Device::from_raw(device.as_raw()),
            )
            .expect("the start call");
            eprintln!("started: {result:#X}");
            assert!(succeeded(result), "{result:#X}");
            let mut p = std::ptr::null_mut();
            let allocated = allocate(&raw mut p);
            assert!(succeeded(allocated) && !p.is_null(), "{allocated:#X}");
            Parameter::set_i32(p, c"Dereth.Test", 7);
            assert_eq!(Parameter::get_i32(p, c"Dereth.Test"), Some(7));
            Parameter::set_i32(p, c"Dereth.Test", -3);
            assert_eq!(Parameter::get_i32(p, c"Dereth.Test"), Some(-3));
            assert_eq!(Parameter::get_i32(p, c"Dereth.Absent"), None);
            // Each of the other setters this module calls, read back through its own getter's
            // place, the next along the table from the setter's as the getters run in step.
            let read = |at: usize, name: &CStr, out: *mut c_void| -> bool {
                let f: unsafe extern "system" fn(
                    *mut Parameter,
                    *const c_char,
                    *mut c_void,
                ) -> u32 = std::mem::transmute(*(*p).table.add(at));
                succeeded(f(p, name.as_ptr(), out))
            };
            Parameter::set_f32(p, c"Dereth.Float", 1.5);
            let mut f = 0.0f32;
            assert!(read(14, c"Dereth.Float", (&raw mut f).cast()));
            assert!((f - 1.5).abs() < 1e-6, "{f}");
            Parameter::set_u32(p, c"Dereth.Unsigned", 9);
            let mut u = 0u32;
            assert!(read(12, c"Dereth.Unsigned", (&raw mut u).cast()));
            assert_eq!(u, 9);
            let mut held = 5u8;
            Parameter::set_pointer(p, c"Dereth.Pointer", (&raw mut held).cast());
            let mut back: *mut c_void = std::ptr::null_mut();
            assert!(read(8, c"Dereth.Pointer", (&raw mut back).cast()));
            assert_eq!(back, (&raw mut held).cast());
            let mut caps = std::ptr::null_mut();
            let found = capabilities(&raw mut caps);
            eprintln!(
                "capabilities {found:#X}: available {:?}, needs a newer driver {:?}",
                (!caps.is_null()).then(|| Parameter::get_i32(caps, c"SuperSampling.Available")),
                (!caps.is_null())
                    .then(|| Parameter::get_i32(caps, c"SuperSampling.NeedsUpdatedDriver")),
            );
            if !caps.is_null() {
                eprintln!("{}", capability_report(caps));
                destroy(caps);
            }
            destroy(p);
        }
        // As the client leaves a start that found DLSS unavailable: never shut down or unloaded.
        std::mem::forget(lib);
    }

    /// A build with the feature, with no DLL beside it, has nothing to load and asks for no
    /// extension.
    #[test]
    fn without_the_dll_nothing_is_loaded_and_no_extension_is_asked_for() {
        if !dll_beside_client() {
            assert!(library().is_none());
            let empty: &[&CStr] = &[];
            assert_eq!(extensions(), (empty, empty));
        }
    }
}
