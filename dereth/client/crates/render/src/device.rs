//! The backend-neutral device surface: the plain data both backends speak, and the one type the
//! rest of the workspace holds a device through.
//!
//! `vulkan::Gpu` (ash + gpu-allocator + naga, WGSL) is the default on every platform;
//! `d3d12::Gpu` is an option on Windows. Neither is named by
//! anything outside this module: `dereth-client` and both GPU test tiers hold a [`Gpu`], which is a
//! sealed enum over whichever backends this build compiled in.
//!
//! # Why an enum rather than a trait
//!
//! Either would work. A trait is not the cheaper shape here:
//!
//! * `Gpu::new` is a constructor returning `Self`, which no object-safe trait can express;
//! * [`Gpu::with_preview_sharp`] is generic in the closure's return type, so it is not
//!   object-safe either;
//! * the device carries five counters that were `pub` **fields** (`adapter_kind`, `adapter_name`,
//!   `frame_stamp`, `portal_stamps`, `draw_calls`), which a trait cannot have at all;
//! * `dereth-client`'s world, preview, sky and particle modules pass `&mut Gpu` through some thirty
//!   signatures and the draw path is the hot loop, which `&mut dyn` would make virtual.
//!
//! The enum keeps every one of those static, keeps the backend types themselves untouched, and
//! costs one `match` per call. Each backend module stays `pub` for its own device tests.
//!
//! # The FPU
//!
//! Neither backend changes the FPU control word; see each module's `assert_fpu_untouched`.

// The items only the device enum names are gated with it: a build with no backend compiles this
// module for its plain data alone.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
use crate::descriptor::{DescriptorStats, Released, TextureKey, TextureTableStats};
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
use crate::pso::PipelineKey;
use crate::pso::PixelShader;
use crate::RenderError;
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
use dereth_primitives::TextureData;

pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// The frame ring depth. The client's swap chain is `BackBufferCount = 1` with
/// `D3DSWAPEFFECT_DISCARD` when it sets up presentation; an explicit-synchronisation API needs
/// the CPU to run ahead of the GPU explicitly, so this ring has three frames. The observable
/// behaviour to preserve is that each frame's dynamic data is contiguous and the buffer only
/// grows.
pub const FRAME_COUNT: usize = 3;

/// How many texture stages `bind_texture` counts binds for. The same eight in both backends.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub(crate) const SAMPLER_COUNT: u32 = crate::sampler::SAMPLER_COUNT;

/// The two handles a window contributes: enough for `ash-window` to make a
/// surface on any of the three platforms, and enough for `CreateSwapChainForHwnd` on Windows.
/// The type is [`dereth_client_contract::window::WindowHandles`], because
/// `dereth_client_runtime::platform::window::WindowHost` returns one; it is re-exported here
/// because every use of it is here.
pub use dereth_client_contract::window::WindowHandles;

/// Which adapter the device was created on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    /// A real GPU.
    Hardware,
    /// A CPU rasteriser (lavapipe, SwiftShader; **WARP** on the D3D12 backend). Deterministic,
    /// which is what makes the headless capture backend reproducible across runs and
    /// machines.
    Software,
}

/// The device configuration and presentation settings.
#[derive(Debug, Clone, Copy)]
pub struct DeviceConfig {
    pub width: u32,
    pub height: u32,
    /// Whether to create a stencil buffer — the client always asks for one.
    pub use_stencil_buffer: bool,
    /// Prefer a CPU-type device — a Vulkan `CPU` physical device, or D3D12's WARP. The capture
    /// backend does this so its output does not depend on the machine's GPU. Falls back to
    /// whatever there is; see [`Gpu::adapter_kind`].
    pub force_software: bool,
    /// Enable the Khronos validation layer / the D3D12 debug layer, when it is installed.
    pub debug: bool,
    /// The texture descriptor budget, in **descriptors** (two per texture, as the D3D12 heap
    /// counted them). `None` takes each backend's own `SRV_HEAP_SIZE`.
    ///
    /// This lets a test measure a scene's *working set* rather than only
    /// finding out that it did not fit.
    pub srv_descriptors: Option<u32>,
    /// Apply the texture-filtering preference's level-of-detail bias in the pixel shader rather
    /// than in the sampler, even on a device whose samplers can carry one. A Vulkan device whose
    /// samplers cannot always does; this lets a device that can run that path too, so the two can
    /// be compared. Only the Vulkan device reads it: the D3D12 device's samplers always carry the
    /// bias and the `wgpu` device's never do.
    pub force_shader_lod_bias: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        // Device initialization starts at 800x600, and the pre-game screens are forced to
        // exactly that.
        Self {
            width: 800,
            height: 600,
            use_stencil_buffer: true,
            force_software: false,
            debug: false,
            srv_descriptors: None,
            force_shader_lod_bias: false,
        }
    }
}

impl DeviceConfig {
    /// Whether the device this config asks for should prefer a software rasteriser, after the
    /// test-only adapter override.
    ///
    /// In a test build (the `test-support` feature) every device prefers a real GPU, whatever the
    /// caller asked for: a software rasteriser is several times slower and saturates the CPU when
    /// test processes run side by side. `DERETH_TEST_GPU=software` makes every device prefer the CPU
    /// rasteriser instead, for a run that wants one. A machine with no GPU still gets a device, since
    /// each backend's ladder falls back to its software rasteriser when no hardware adapter exists.
    /// Outside a test build [`Self::force_software`] is followed as the caller set it. Both backends read the adapter preference
    /// through this, so one variable moves every device a test tier opens, including the ones a
    /// headless application makes for itself.
    ///
    /// On Vulkan "software" is a preference and not a guarantee: the ladder falls back to a real GPU
    /// when the machine has no CPU-type physical device. `Gpu::adapter_kind` says which it got.
    #[must_use]
    pub fn prefers_software(&self) -> bool {
        match test_adapter_override() {
            Some(AdapterKind::Hardware) => false,
            Some(AdapterKind::Software) => true,
            None => !cfg!(any(test, feature = "test-support")) && self.force_software,
        }
    }
}

/// The adapter the test-only `DERETH_TEST_GPU=hardware|software` names, in a test build; `None`
/// when it is unset or unrecognised (a test build then prefers hardware), and always `None` outside
/// a test build.
#[must_use]
pub fn test_adapter_override() -> Option<AdapterKind> {
    #[cfg(any(test, feature = "test-support"))]
    {
        match std::env::var("DERETH_TEST_GPU")
            .ok()?
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "hardware" | "hw" => Some(AdapterKind::Hardware),
            "software" | "sw" => Some(AdapterKind::Software),
            _ => None,
        }
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        None
    }
}

/// A texture's slot in the descriptor budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlot(pub u32);

/// A decoded terrain source image (a terrain tile or an alpha map) resident on the device for
/// [`Gpu::merge_terrain_texture`]. Valid until [`Gpu::reset_merge_sources`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MergeSource(pub u32);

/// One overlay pass of a landscape composite: `tex` tiled `tiling` times, blended through the
/// alpha map `alpha` walked at `rotation` (0 to 3, quarter turns). `tex` is `None` to blend the
/// missing-texture debug colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainMergeOverlay {
    pub alpha: MergeSource,
    pub rotation: u32,
    pub tex: Option<MergeSource>,
    pub tiling: u32,
}

/// A landscape surface composite for the device to build: a `size x size` base tiled from `base`
/// (`None` fills the debug colour), then each overlay in order. The arithmetic is the CPU
/// compositor's, so the result is bit-identical to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainMergeJob {
    pub size: u32,
    pub base: Option<MergeSource>,
    pub base_tiling: u32,
    pub overlays: Vec<TerrainMergeOverlay>,
}

/// One layer of a splatted landscape cell: an ordinary texture (a terrain tile) tiled `tiling`
/// times across the cell, blended through the alpha map texture `alpha` turned `rotation`
/// quarter turns. `tex` is `None` for the missing-texture debug colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainSplatOverlay {
    pub alpha: TextureSlot,
    pub rotation: u32,
    pub tex: Option<TextureSlot>,
    pub tiling: u32,
}

/// A landscape cell drawn by blending its terrain layers in the pixel shader, rather than by
/// sampling a composite made beforehand. Same layers as a [`TerrainMergeJob`], but each is
/// filtered and mipmapped on its own before the blend, so it is close to the composite rather
/// than identical to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainSplat {
    pub base: Option<TextureSlot>,
    pub base_tiling: u32,
    pub overlays: Vec<TerrainSplatOverlay>,
}

/// A snapshot of descriptor occupancy, in slots (one texture each).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DescriptorUsage {
    /// Slots handed out and not yet released.
    pub live: u32,
    /// Slots ever taken from fresh space: `live + free + pending`.
    pub frontier: u32,
    /// The largest `live` ever reached.
    pub high_water: u32,
    /// Slots reusable right now.
    pub free: u32,
    /// Slots released but still behind the fence.
    pub pending: u32,
    /// Slots released inside the open frame, still counted in `live` because that frame's command
    /// buffer may have bound them and has not been submitted. They join `pending` at `end_frame`.
    pub deferred: u32,
    /// The budget.
    pub capacity: u32,
}

/// Lay a matrix out the way `legacy.hlsl` and `legacy.wgsl` read it.
///
/// The shaders multiply `vector * matrix` -- HLSL's `mul(vector, matrix)`, kept as a row-vector
/// product in the WGSL port -- and a `mat4x4` is column-major in memory, so the shader reads
/// `M[i][j]` from `data[j * 4 + i]` and computes `result[j] = sum_i v[i] * M[i][j]`, the row-vector
/// product the client's own convention uses.
///
/// `camera::view_from_frame` already returns the D3D matrix *transposed*, because glam composes
/// `M * v` down columns where D3D composes `v * M` across rows. Feeding that straight to
/// `to_cols_array` therefore hands the shader the transpose, and it computes `M * v` instead --
/// which puts `w` negative for every vertex in front of the camera and renders a black frame. The
/// only earlier consumer was the UI, whose matrices are identity and whose vertices are
/// pre-transformed, so nothing catches it until a world is drawn. The name is the D3D12
/// build's, kept because forty call sites and a test in `dereth-client` name it.
#[must_use]
pub fn hlsl_matrix(m: glam::Mat4) -> [f32; 16] {
    m.transpose().to_cols_array()
}

/// The `PerFrame` constant buffer the shaders declare at `b0` / set 0, binding 0.
///
/// The transforms, `D3DRS_AMBIENT`, and the fog parameters become shader constants; the vertex
/// shader computes the fog factor and interpolates it.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PerFrameConstants {
    pub view_proj: [f32; 16],
    pub view: [f32; 16],
    /// x = `D3DRS_FOGSTART`, y = `D3DRS_FOGEND`, z = `D3DRS_FOGENABLE`, w = unused.
    pub fog_params: [f32; 4],
    pub fog_color: [f32; 4],
    /// `D3DRS_AMBIENT`, unpacked.
    pub ambient: [f32; 4],
    /// x holds the clamped screen-brightness value. y, z, w are unused. See
    /// `Gpu::draw_dynamic`, which overwrites this
    /// field from the device's own live value so that it is device state here as it is there.
    pub screen: [f32; 4],
}

impl PerFrameConstants {
    /// Build the block from a [`crate::ViewParams`], applying the client's own field-of-view
    /// projection convention.
    #[must_use]
    pub fn from_view(v: &crate::ViewParams) -> Self {
        let proj = crate::camera::projection(v);
        Self {
            view_proj: hlsl_matrix(proj * v.view),
            view: hlsl_matrix(v.view),
            fog_params: [
                v.fog.near,
                v.fog.far,
                if v.fog.enabled { 1.0 } else { 0.0 },
                0.0,
            ],
            fog_color: unpack_argb(v.fog.color),
            ambient: unpack_argb(v.lights.ambient),
            // Device state, not view state: `Gpu::draw_dynamic` overwrites it from the live
            // gamma brightness value. Zero here is the ramp's identity.
            screen: [0.0; 4],
        }
    }
}

/// The `PerDraw` constant buffer the shaders declare at `b1` / set 0, binding 1.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PerDrawConstants {
    pub world: [f32; 16],
    /// The `D3DTS_TEXTURE0` translation built from the mesh buffer's UV delta.
    pub uv_offset: [f32; 4],
    /// `D3DRS_TEXTUREFACTOR`.
    pub texture_factor: [f32; 4],
    /// x = alpha reference / 255, y = alpha test enabled, z = fog enabled for this draw,
    /// w = use the texture factor instead of the vertex colour.
    pub draw_params: [f32; 4],
    /// The bound material's `Emissive.rgb` (x) and `Diffuse.rgb` (y), and
    /// whether a material is bound at all (z). The client hands the part's material clone to the
    /// fixed-function pipeline, whose lit colour is `Emissive + Diffuse * lights`; the vertex colour
    /// this renderer carries is that lit colour for the default material, so the shader applies
    /// `rgb * y + x` when z is set and leaves the colour alone otherwise. All-zero (the `Default`)
    /// is "no material bound".
    pub material_lighting: [f32; 4],
    /// `x` = `D3DRS_LIGHTING`, which the client turns on for every mesh
    /// subset; `y` = how many of the eight `D3DLIGHT9` slots the light-enable pass left enabled
    /// for this draw; `z` = the material's Emissive scalar (the mesh subset draw's
    /// surface luminosity when positive, else the part clone's simple luminosity), `w` =
    /// 1 when the emissive colour source is the vertex -- the burned-in env-cell branch.
    /// All-zero (the `Default`) is "lighting off", which is every path other than the mesh
    /// subset draw: the UI, text, terrain (fixed-function lighting off), the sky and the particles.
    pub lighting_params: [f32; 4],
    /// The eight `D3DLIGHT9`s, `Position` (point) or `Direction` (directional) in `xyz` with the
    /// client's y/z swap folded in as the light-configuration step folds it, and `Range`
    /// (`falloff * 1.5`) in `w`. Slots at and past `lighting_params.y` are ignored.
    pub light_pos: [[f32; 4]; 8],
    /// The same eight lights' `Diffuse` (`colour * intensity`) in `rgb` and `D3DLIGHTTYPE` in
    /// `w`: 1 point, 2 spot, 3 directional.
    pub light_diffuse: [[f32; 4]; 8],
    /// **The detail-texture pass.** `x` = the current detail tiling, the factor
    /// the client multiplies the mesh's first UV set by to produce its second
    /// (`u1 = factor * u0`, `v1 = factor * v0`);
    /// `y` = 1 when stage 1 holds a detail texture, i.e. when the mesh subset draw
    /// decided to use detail. `y == 0` (the `Default`) is every draw without detail, and
    /// the pixel shader then never samples `t1`.
    pub detail_params: [f32; 4],
    /// **The normal transform under a per-axis scale.** Fixed-function lighting takes a normal
    /// through the inverse transpose of the world matrix. For a world matrix that is a rotation
    /// times a per-axis scale `s`, that is the world matrix itself applied to `n / s²`, so `xyz`
    /// holds `1 / s²` per model axis and `w` = 1 says to use it. `w` = 0 (the `Default`) takes
    /// the normal through the world matrix as it is, which is exact for a uniform scale.
    pub normal_scale: [f32; 4],
}

impl PerDrawConstants {
    /// An identity-world block, which is what the UI and text paths use: they set the world,
    /// view and projection matrices to the identity rather than to a camera.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            world: hlsl_matrix(glam::Mat4::IDENTITY),
            ..Self::default()
        }
    }
}

/// Unpack a packed ARGB word to a float4, as the fixed-function pipeline did.
pub(crate) fn unpack_argb(c: u32) -> [f32; 4] {
    [
        ((c >> 16) & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        (c & 0xFF) as f32 / 255.0,
        ((c >> 24) & 0xFF) as f32 / 255.0,
    ]
}

/// View a `#[repr(C)]` plain-old-data struct as bytes for the upload ring.
#[allow(dead_code)]
pub(crate) fn as_bytes<T: Copy>(v: &T) -> &[u8] {
    // SAFETY: `T` is a Copy, repr(C) struct of floats with no padding invariants and no interior
    // pointers, and the slice borrows `v` for exactly its own lifetime.
    unsafe {
        std::slice::from_raw_parts(std::ptr::from_ref(v).cast::<u8>(), std::mem::size_of::<T>())
    }
}

/// The fragment/pixel entry point's name as a NUL-terminated byte string. Both shader sets use
/// these names.
#[allow(dead_code)]
pub(crate) const fn shader_entry_c(s: PixelShader) -> &'static [u8] {
    match s {
        PixelShader::Modulate => b"ps_modulate\0",
        PixelShader::SelectArg1 => b"ps_selectarg1\0",
        PixelShader::SelectArg2 => b"ps_selectarg2\0",
        PixelShader::PreModulate => b"ps_premodulate\0",
        PixelShader::BlendCurrentAlpha => b"ps_blendcurrentalpha\0",
    }
}

/// A back-buffer read-back. The golden-image harness consumes this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, B, G, R, A per pixel — the back buffer's own order.
    pub bgra: Vec<u8>,
}

impl CapturedImage {
    /// The same pixels as RGBA8, which is what a capture is compared as.
    #[must_use]
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = self.bgra.clone();
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            // COLORWRITEENABLE = 7 means the back buffer's alpha was never written, so it carries
            // whatever the clear left. Force it opaque rather than exporting an undefined channel.
            px[3] = 0xFF;
        }
        out
    }
}

// =================================================================================================
// Backend selection
// =================================================================================================

/// Which device implementation a [`Gpu`] is.
///
/// The *selection* is [`dereth_client_contract::RendererChoice`], which is the
/// same two names with no opinion about what this build compiled; [`Backend::parse`] is that
/// type's parser plus a conversion, and [`Backend::compiled_in`] reads this crate's own
/// cargo features, which is why this type lives here rather than in the contract crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// `dereth_render::vulkan` — the default on every platform.
    Vulkan,
    /// `dereth_render::d3d12` — Windows only, and only when this build compiled the `d3d12`
    /// feature.
    D3d12,
    /// `dereth_render::wgpu` — the platform's own API through `wgpu`, and the browser's.
    Wgpu,
}

impl Backend {
    /// The name the command line and the preferences file both spell.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::D3d12 => "d3d12",
            Self::Wgpu => "wgpu",
        }
    }

    /// Parse one of those spellings, case-insensitively. `None` for anything else.
    ///
    /// The spellings are [`dereth_client_contract::RendererChoice::parse`]'s, so that the
    /// client's `--renderer` switch and this function cannot drift apart.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        dereth_client_contract::RendererChoice::parse(s).map(Self::from)
    }

    /// Whether this build can actually create this backend.
    #[must_use]
    pub const fn compiled_in(self) -> bool {
        match self {
            Self::Vulkan => cfg!(feature = "vulkan"),
            Self::D3d12 => cfg!(all(windows, feature = "d3d12")),
            Self::Wgpu => cfg!(feature = "wgpu"),
        }
    }

    /// The backends this build compiled in, in preference order. The default is first.
    #[must_use]
    pub fn compiled() -> Vec<Self> {
        [Self::Vulkan, Self::D3d12, Self::Wgpu]
            .into_iter()
            .filter(|b| b.compiled_in())
            .collect()
    }

    /// The default — Vulkan everywhere — falling back to whatever *is* compiled in when
    /// a build left Vulkan out.
    #[must_use]
    pub fn default_backend() -> Option<Self> {
        Self::compiled().first().copied()
    }

    /// In a test build (the `test-support` feature), the backend the test-only
    /// `DERETH_TEST_RENDERER=vulkan|d3d12|wgpu` names, when this build has it; otherwise `None`.
    ///
    /// An unset, empty or unparseable value is `None`, and so is a value naming a backend this
    /// build did not compile; the caller then takes the default.
    #[must_use]
    pub fn test_selection() -> Option<Self> {
        #[cfg(any(test, feature = "test-support"))]
        {
            let raw = std::env::var("DERETH_TEST_RENDERER").ok()?;
            Self::parse(&raw).filter(|b| b.compiled_in())
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            None
        }
    }

    /// The backend a device made without a named one comes up on: [`Self::test_selection`] in a
    /// test build, else the default.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] when this build compiled no device at all.
    pub fn resolve_default() -> Result<Self, RenderError> {
        Self::test_selection()
            .or_else(Self::default_backend)
            .ok_or(RenderError::Unsupported(
                "this build compiled no graphics backend",
            ))
    }
}

/// The selection becomes the backend by name; whether this build can create it is
/// [`Backend::compiled_in`]'s question, asked at the one site that makes a device.
impl From<dereth_client_contract::RendererChoice> for Backend {
    fn from(c: dereth_client_contract::RendererChoice) -> Self {
        match c {
            dereth_client_contract::RendererChoice::Vulkan => Self::Vulkan,
            dereth_client_contract::RendererChoice::D3d12 => Self::D3d12,
            dereth_client_contract::RendererChoice::Wgpu => Self::Wgpu,
        }
    }
}

// =================================================================================================
// The device
// =================================================================================================

/// The device. One variant per backend this build compiled in; the surface below is the whole of
/// what `dereth-client` and both GPU test tiers ask of a device.
///
/// Each arm forwards to the backend type unchanged: no method here adds, reorders or reinterprets
/// anything a backend does.
///
/// A build with neither backend has no device at all, and this type is then absent rather than
/// uninhabited -- which is what keeps `dereth-client`'s no-device build free of dead arms.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[allow(clippy::large_enum_variant)] // one device per process, never moved in bulk
#[derive(Debug)]
#[non_exhaustive]
pub enum Gpu {
    /// The Vulkan device (the default).
    #[cfg(feature = "vulkan")]
    Vulkan(crate::vulkan::Gpu),
    /// The Direct3D 12 device (Windows only).
    #[cfg(all(windows, feature = "d3d12"))]
    D3d12(crate::d3d12::Gpu),
    /// The `wgpu` device.
    #[cfg(feature = "wgpu")]
    Wgpu(crate::wgpu::Gpu),
}

/// Forward a method to whichever backend this device is. Only compiled when there is one.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
macro_rules! dispatch {
    ($self:expr, $g:ident => $body:expr) => {{
        match $self {
            #[cfg(feature = "vulkan")]
            Gpu::Vulkan($g) => $body,
            #[cfg(all(windows, feature = "d3d12"))]
            Gpu::D3d12($g) => $body,
            #[cfg(feature = "wgpu")]
            Gpu::Wgpu($g) => $body,
        }
    }};
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
impl Gpu {
    /// Create a device on [`Backend::resolve_default`]: the default backend (in a test build, the
    /// one `DERETH_TEST_RENDERER` names, when it does).
    ///
    /// `window` is `None` for the headless path, which renders to an offscreen texture; that is
    /// what makes the capture tests runnable without a display.
    ///
    /// # Errors
    /// [`RenderError`] when the selected backend cannot create a device, or when this build
    /// compiled none.
    pub fn new(window: Option<WindowHandles>, cfg: &DeviceConfig) -> Result<Self, RenderError> {
        Self::new_on(Backend::resolve_default()?, window, cfg)
    }

    /// Create the device on a named backend. The caller has already chosen; a backend this build
    /// does not have is an error here rather than a silent substitution, so the caller logs its
    /// own fallback line.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] when `backend` is not in this build, else the backend's own
    /// creation error.
    #[allow(unreachable_patterns)]
    pub fn new_on(
        backend: Backend,
        window: Option<WindowHandles>,
        cfg: &DeviceConfig,
    ) -> Result<Self, RenderError> {
        match backend {
            #[cfg(feature = "vulkan")]
            Backend::Vulkan => Ok(Self::Vulkan(crate::vulkan::Gpu::new(window, cfg)?)),
            #[cfg(all(windows, feature = "d3d12"))]
            Backend::D3d12 => Ok(Self::D3d12(crate::d3d12::Gpu::new(window, cfg)?)),
            #[cfg(feature = "wgpu")]
            Backend::Wgpu => Ok(Self::Wgpu(crate::wgpu::Gpu::new(window, cfg)?)),
            Backend::Vulkan => Err(RenderError::Unsupported("this build has no Vulkan backend")),
            Backend::D3d12 => Err(RenderError::Unsupported(
                "this build has no Direct3D 12 backend",
            )),
            Backend::Wgpu => Err(RenderError::Unsupported("this build has no wgpu backend")),
        }
    }

    /// Which backend this device is. Spelled out rather than dispatched, because the macro cannot
    /// name the variant it matched.
    #[must_use]
    pub fn backend(&self) -> Backend {
        match self {
            #[cfg(feature = "vulkan")]
            Gpu::Vulkan(_) => Backend::Vulkan,
            #[cfg(all(windows, feature = "d3d12"))]
            Gpu::D3d12(_) => Backend::D3d12,
            #[cfg(feature = "wgpu")]
            Gpu::Wgpu(_) => Backend::Wgpu,
        }
    }

    // --- the adapter -------------------------------------------------------------------------

    /// Hardware or software. **The pixel-convention instruments read this**: the half-pixel POINT
    /// case is asserted on software devices only, and D3D12's WARP is one.
    #[must_use]
    pub fn adapter_kind(&self) -> AdapterKind {
        dispatch!(self, g => g.adapter_kind)
    }

    /// The adapter's name, for the startup line.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        dispatch!(self, g => g.adapter_name.as_str())
    }

    /// The frame stamp, bumped once per presented frame.
    #[must_use]
    pub fn frame_stamp(&self) -> u64 {
        dispatch!(self, g => g.frame_stamp)
    }

    /// How many portal depth stamps this device has issued.
    #[must_use]
    pub fn portal_stamps(&self) -> u64 {
        dispatch!(self, g => g.portal_stamps)
    }

    /// How many `draw_dynamic` batches this device has issued.
    #[must_use]
    pub fn draw_calls(&self) -> u64 {
        dispatch!(self, g => g.draw_calls)
    }

    // --- the frame bracket -------------------------------------------------------------------

    /// # Errors
    /// The backend's own.
    pub fn begin_frame(&mut self) -> Result<(), RenderError> {
        dispatch!(self, g => g.begin_frame())
    }

    /// # Errors
    /// The backend's own.
    pub fn end_frame(&mut self) -> Result<(), RenderError> {
        dispatch!(self, g => g.end_frame())
    }

    /// # Errors
    /// The backend's own.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        dispatch!(self, g => g.wait_idle())
    }

    /// Whether a frame's command list/buffer is open.
    #[must_use]
    pub fn frame_open(&self) -> bool {
        dispatch!(self, g => g.frame_open())
    }

    // --- the back buffer ---------------------------------------------------------------------

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        dispatch!(self, g => g.size())
    }

    /// A presentation change, and the device reset it drives.
    ///
    /// # Errors
    /// The backend's own.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        dispatch!(self, g => g.resize(width, height))
    }

    pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        dispatch!(self, g => g.set_presentation_sync(full_screen, sync_to_refresh));
    }

    #[must_use]
    pub fn present_sync_interval(&self) -> u32 {
        dispatch!(self, g => g.present_sync_interval())
    }

    #[must_use]
    pub fn last_present_sync_interval(&self) -> Option<u32> {
        dispatch!(self, g => g.last_present_sync_interval())
    }

    pub fn set_gamma(&mut self, brightness: f32) {
        dispatch!(self, g => g.set_gamma(brightness));
    }

    #[must_use]
    pub fn gamma(&self) -> f32 {
        dispatch!(self, g => g.gamma())
    }

    /// A back-buffer read-back.
    ///
    /// # Errors
    /// The backend's own.
    pub fn capture(&mut self) -> Result<CapturedImage, RenderError> {
        dispatch!(self, g => g.capture())
    }

    // --- drawing -----------------------------------------------------------------------------

    /// # Errors
    /// The backend's own.
    pub fn draw_dynamic(
        &mut self,
        key: &PipelineKey,
        constants: &crate::DrawConstants,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
    ) -> Result<(), RenderError> {
        dispatch!(self, g => g.draw_dynamic(key, constants, per_frame, per_draw, vertices))
    }

    /// The portal depth stamp.
    ///
    /// # Errors
    /// The backend's own.
    pub fn draw_portal_poly(
        &mut self,
        per_frame: &PerFrameConstants,
        clip: &[[f32; 4]],
        mask: u8,
    ) -> Result<(), RenderError> {
        dispatch!(self, g => g.draw_portal_poly(per_frame, clip, mask))
    }

    /// # Errors
    /// The backend's own.
    pub fn upload_bytes(&mut self, data: &[u8]) -> Result<u64, RenderError> {
        dispatch!(self, g => g.upload_bytes(data))
    }

    /// The high-water mark of the per-frame upload arena.
    #[must_use]
    pub fn upload_high_water(&self) -> u64 {
        dispatch!(self, g => g.upload_high_water())
    }

    /// Build every pipeline in the catalogue up front.
    ///
    /// # Errors
    /// The backend's own.
    pub fn build_whole_catalogue(&mut self) -> Result<usize, RenderError> {
        dispatch!(self, g => g.build_whole_catalogue())
    }

    pub fn set_viewport(&self, v: crate::camera::Viewport) {
        dispatch!(self, g => g.set_viewport(v));
    }

    pub fn reset_viewport(&self) {
        dispatch!(self, g => g.reset_viewport());
    }

    pub fn clear_depth(&self, v: crate::camera::Viewport) {
        dispatch!(self, g => g.clear_depth(v));
    }

    // --- textures ----------------------------------------------------------------------------

    /// # Errors
    /// The backend's own.
    pub fn upload_texture(&mut self, t: &TextureData) -> Result<TextureSlot, RenderError> {
        dispatch!(self, g => g.upload_texture(t))
    }

    /// # Errors
    /// The backend's own.
    pub fn upload_texture_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        dispatch!(self, g => g.upload_texture_keyed(key, t))
    }

    /// # Errors
    /// The backend's own.
    pub fn upload_imgtex_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        dispatch!(self, g => g.upload_imgtex_keyed(key, t))
    }

    #[must_use]
    pub fn imgtex_autogen_supported(&self) -> bool {
        dispatch!(self, g => g.imgtex_autogen_supported())
    }

    /// Whether an upload under `key` would be a cache hit, without taking a link.
    #[must_use]
    pub fn has_texture_key(&self, key: TextureKey) -> bool {
        dispatch!(self, g => g.has_texture_key(key))
    }

    // --- landscape surfaces on the device ----------------------------------------------------

    /// Whether [`Self::merge_terrain_texture`] is available on this device.
    #[must_use]
    pub fn terrain_merge_supported(&self) -> bool {
        dispatch!(self, g => g.terrain_merge_supported())
    }

    /// Make a decoded BGRA8 terrain source resident for [`Self::merge_terrain_texture`].
    ///
    /// # Errors
    /// The backend's own, or unsupported.
    pub fn upload_merge_source(
        &mut self,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> Result<MergeSource, RenderError> {
        dispatch!(self, g => g.upload_merge_source(width, height, bgra))
    }

    /// Forget every resident merge source.
    pub fn reset_merge_sources(&mut self) {
        dispatch!(self, g => g.reset_merge_sources());
    }

    /// Build a landscape composite on the device, as a texture with the same levels an uploaded
    /// CPU composite gets. One link, owned by the caller.
    ///
    /// # Errors
    /// The backend's own, or unsupported.
    pub fn merge_terrain_texture(
        &mut self,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        dispatch!(self, g => g.merge_terrain_texture(key, job))
    }

    /// Whether [`Self::draw_terrain_splat`] is available on this device.
    #[must_use]
    pub fn terrain_splat_supported(&self) -> bool {
        dispatch!(self, g => g.terrain_splat_supported())
    }

    /// Draw landscape triangles with `key`'s fixed-function state, blending `splat`'s layers in
    /// the pixel shader in place of the bound composite. Otherwise as [`Self::draw_dynamic`].
    ///
    /// # Errors
    /// The backend's own, or unsupported.
    pub fn draw_terrain_splat(
        &mut self,
        key: &PipelineKey,
        splat: &TerrainSplat,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
    ) -> Result<(), RenderError> {
        dispatch!(self, g => g.draw_terrain_splat(key, splat, per_frame, per_draw, vertices))
    }

    /// Drop every cached splat binding. Call before releasing textures a splat named.
    pub fn reset_terrain_splat(&mut self) {
        dispatch!(self, g => g.reset_terrain_splat());
    }

    /// Take another link on a texture.
    pub fn retain_texture(&mut self, slot: TextureSlot) -> Option<u32> {
        dispatch!(self, g => g.retain_texture(slot))
    }

    /// Drop one link on a texture, freeing it at zero.
    pub fn release_texture(&mut self, slot: TextureSlot) -> Released {
        dispatch!(self, g => g.release_texture(slot))
    }

    #[must_use]
    pub fn texture_keys(&self) -> Vec<TextureKey> {
        dispatch!(self, g => g.texture_keys())
    }

    #[must_use]
    pub fn texture_table_stats(&self) -> TextureTableStats {
        dispatch!(self, g => g.texture_table_stats())
    }

    #[must_use]
    pub fn texture_mip_levels(&self, slot: TextureSlot) -> Option<u16> {
        dispatch!(self, g => g.texture_mip_levels(slot))
    }

    #[must_use]
    pub fn live_textures(&self) -> usize {
        dispatch!(self, g => g.live_textures())
    }

    #[must_use]
    pub fn descriptor_stats(&self) -> DescriptorStats {
        dispatch!(self, g => g.descriptor_stats())
    }

    #[must_use]
    pub fn descriptor_usage(&self) -> DescriptorUsage {
        dispatch!(self, g => g.descriptor_usage())
    }

    /// Read one resident BGRA8 subresource back.
    ///
    /// # Errors
    /// The backend's own.
    pub fn capture_texture_level(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<CapturedImage, RenderError> {
        dispatch!(self, g => g.capture_texture_level(slot, level))
    }

    /// Read one resident subresource back, keeping compressed block bytes.
    ///
    /// # Errors
    /// The backend's own.
    pub fn capture_texture_level_data(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<TextureData, RenderError> {
        dispatch!(self, g => g.capture_texture_level_data(slot, level))
    }

    // --- binding and samplers ------------------------------------------------------------------

    pub fn bind_texture(&self, slot: TextureSlot, sampler: u32) {
        dispatch!(self, g => g.bind_texture(slot, sampler));
    }

    pub fn bind_stage1_texture(&self, slot: TextureSlot) {
        dispatch!(self, g => g.bind_stage1_texture(slot));
    }

    #[must_use]
    pub fn stage1_binds(&self) -> u64 {
        dispatch!(self, g => g.stage1_binds())
    }

    pub fn clear_stage1_binds(&self) {
        dispatch!(self, g => g.clear_stage1_binds());
    }

    #[must_use]
    pub fn sampler_binds(&self) -> [u64; SAMPLER_COUNT as usize] {
        dispatch!(self, g => g.sampler_binds())
    }

    pub fn clear_sampler_binds(&self) {
        dispatch!(self, g => g.clear_sampler_binds());
    }

    /// The live UInt32 `Render.TextureFiltering`.
    pub fn set_texture_filtering(&mut self, preference: u32) {
        dispatch!(self, g => g.set_texture_filtering(preference));
    }

    #[must_use]
    pub fn texture_filtering(&self) -> u32 {
        dispatch!(self, g => g.texture_filtering())
    }

    /// Sharp-preview sampler bracket around a draw.
    ///
    /// This is the backend's own `with_preview_sharp`, re-expressed on the enum because a closure
    /// typed `FnOnce(&mut Self)` cannot be handed a backend reference. The two statements it
    /// brackets the draw with are the backend's, unchanged.
    pub fn with_preview_sharp<T>(&mut self, enabled: bool, draw: impl FnOnce(&mut Self) -> T) -> T {
        let changed = dispatch!(self, g => g.begin_preview_sharp(enabled));
        let result = draw(self);
        if changed {
            dispatch!(self, g => g.end_preview_sharp());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend is named and parsed by the same three spellings.
    #[test]
    fn a_backend_is_named_and_parsed_by_the_same_three_spellings() {
        assert_eq!(Backend::parse("vulkan"), Some(Backend::Vulkan));
        assert_eq!(Backend::parse(" D3D12 "), Some(Backend::D3d12));
        assert_eq!(Backend::parse("dx12"), Some(Backend::D3d12));
        assert_eq!(Backend::parse("vk"), Some(Backend::Vulkan));
        assert_eq!(Backend::parse("opengl"), None);
        assert_eq!(Backend::parse(""), None);
        for b in [Backend::Vulkan, Backend::D3d12] {
            assert_eq!(
                Backend::parse(b.name()),
                Some(b),
                "{b:?} round trips through its name"
            );
        }
    }

    /// Vulkan is the default wherever it is compiled in.
    #[test]
    fn vulkan_is_the_default_wherever_it_is_compiled_in() {
        let compiled = Backend::compiled();
        assert_eq!(compiled.first().copied(), Backend::default_backend());
        if Backend::Vulkan.compiled_in() {
            assert_eq!(Backend::default_backend(), Some(Backend::Vulkan));
        }
        for b in [Backend::Vulkan, Backend::D3d12] {
            assert_eq!(compiled.contains(&b), b.compiled_in(), "{b:?}");
        }
        assert_eq!(
            Backend::D3d12.compiled_in(),
            cfg!(all(windows, feature = "d3d12"))
        );
    }
}
