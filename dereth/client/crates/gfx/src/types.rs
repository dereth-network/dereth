//! The plain data the layer is described with: formats, usages, locations, the descriptions of
//! resources and pipelines, and the handles it gives back. Nothing here names the device.

use ash::vk;

/// A pixel format.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Four 8-bit channels, unsigned normalized.
    Rgba8Unorm,
    /// Four 8-bit channels in blue, green, red, alpha order, unsigned normalized.
    Bgra8Unorm,
    /// Four 8-bit unsigned integers.
    Rgba8Uint,
    /// Two 16-bit channels, signed normalized.
    Rg16Snorm,
    /// Four 16-bit floats.
    Rgba16Float,
    /// Three unsigned floats packed in 32 bits: red and green of 11 bits, blue of 10. No alpha.
    Rg11b10Ufloat,
    /// One 8-bit channel, unsigned normalized.
    R8Unorm,
    /// One 16-bit float.
    R16Float,
    /// One 32-bit float.
    R32Float,
    /// One 32-bit unsigned integer.
    R32Uint,
    /// A 32-bit float depth.
    D32Float,
    /// A 24-bit depth and an 8-bit stencil.
    D24UnormS8,
    /// A 32-bit float depth and an 8-bit stencil.
    D32FloatS8,
    /// BC1 (DXT1) blocks.
    Bc1,
    /// BC2 (DXT3) blocks.
    Bc2,
    /// BC3 (DXT5) blocks.
    Bc3,
    /// BC5 blocks: two channels, red and green.
    Bc5,
    /// BC7 blocks: four channels.
    Bc7,
}

impl Format {
    /// The Vulkan format.
    #[must_use]
    pub const fn vk(self) -> vk::Format {
        match self {
            Self::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
            Self::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
            Self::Rgba8Uint => vk::Format::R8G8B8A8_UINT,
            Self::Rg16Snorm => vk::Format::R16G16_SNORM,
            Self::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
            Self::Rg11b10Ufloat => vk::Format::B10G11R11_UFLOAT_PACK32,
            Self::R8Unorm => vk::Format::R8_UNORM,
            Self::R16Float => vk::Format::R16_SFLOAT,
            Self::R32Float => vk::Format::R32_SFLOAT,
            Self::R32Uint => vk::Format::R32_UINT,
            Self::D32Float => vk::Format::D32_SFLOAT,
            Self::D24UnormS8 => vk::Format::D24_UNORM_S8_UINT,
            Self::D32FloatS8 => vk::Format::D32_SFLOAT_S8_UINT,
            Self::Bc1 => vk::Format::BC1_RGBA_UNORM_BLOCK,
            Self::Bc2 => vk::Format::BC2_UNORM_BLOCK,
            Self::Bc3 => vk::Format::BC3_UNORM_BLOCK,
            Self::Bc5 => vk::Format::BC5_UNORM_BLOCK,
            Self::Bc7 => vk::Format::BC7_UNORM_BLOCK,
        }
    }

    /// The format a Vulkan format is, when it is one of these.
    #[must_use]
    pub fn from_vk(f: vk::Format) -> Option<Self> {
        [
            Self::Rgba8Unorm,
            Self::Bgra8Unorm,
            Self::Rgba8Uint,
            Self::Rg16Snorm,
            Self::Rgba16Float,
            Self::Rg11b10Ufloat,
            Self::R8Unorm,
            Self::R16Float,
            Self::R32Float,
            Self::R32Uint,
            Self::D32Float,
            Self::D24UnormS8,
            Self::D32FloatS8,
            Self::Bc1,
            Self::Bc2,
            Self::Bc3,
            Self::Bc5,
            Self::Bc7,
        ]
        .into_iter()
        .find(|x| x.vk() == f)
    }

    /// Whether it holds depth.
    #[must_use]
    pub const fn is_depth(self) -> bool {
        matches!(self, Self::D32Float | Self::D24UnormS8 | Self::D32FloatS8)
    }

    /// Whether it holds stencil too.
    #[must_use]
    pub const fn has_stencil(self) -> bool {
        matches!(self, Self::D24UnormS8 | Self::D32FloatS8)
    }

    /// Whether it is made of 4x4 compressed blocks.
    #[must_use]
    pub const fn is_block(self) -> bool {
        matches!(
            self,
            Self::Bc1 | Self::Bc2 | Self::Bc3 | Self::Bc5 | Self::Bc7
        )
    }

    /// The aspects an image of this format has.
    #[must_use]
    pub const fn aspect(self) -> vk::ImageAspectFlags {
        if self.has_stencil() {
            vk::ImageAspectFlags::from_raw(
                vk::ImageAspectFlags::DEPTH.as_raw() | vk::ImageAspectFlags::STENCIL.as_raw(),
            )
        } else if self.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        }
    }

    /// The bytes one level of `width` x `height` takes, tightly packed.
    #[must_use]
    pub const fn level_bytes(self, width: u32, height: u32) -> u64 {
        let (w, h) = (width as u64, height as u64);
        match self {
            Self::Bc1 => w.div_ceil(4) * h.div_ceil(4) * 8,
            Self::Bc2 | Self::Bc3 | Self::Bc5 | Self::Bc7 => w.div_ceil(4) * h.div_ceil(4) * 16,
            Self::Rgba16Float => w * h * 8,
            Self::D32FloatS8 => w * h * 8,
            Self::R8Unorm => w * h,
            Self::R16Float => w * h * 2,
            _ => w * h * 4,
        }
    }
}

/// Bit sets, each a `u32`, with the operators a set needs.
macro_rules! flags {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $flag:ident = $bit:expr,)* }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub u32);
        impl $name {
            $($(#[$fm])* pub const $flag: Self = Self(1 << $bit);)*
            /// Whether every bit of `other` is set here.
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }
        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
    };
}

flags!(
    /// What a buffer is used for.
    BufferUsage {
        /// Read by index buffer binding.
        INDEX = 0,
        /// Read as indirect draw or dispatch arguments, or as a draw count.
        INDIRECT = 1,
        /// Read or written by shaders through the bindless table.
        STORAGE = 2,
        /// Read by copies.
        TRANSFER_SRC = 3,
        /// Written by copies and fills.
        TRANSFER_DST = 4,
        /// Read as a vertex buffer.
        VERTEX = 5,
        /// Read by acceleration structure builds ([`crate::rt`]); nothing where the device does
        /// not trace rays.
        ACCELERATION_INPUT = 6,
    }
);

flags!(
    /// What a texture is used for.
    TextureUsage {
        /// Sampled by shaders through the bindless table.
        SAMPLED = 0,
        /// Drawn into as a colour target.
        COLOR_TARGET = 1,
        /// Drawn into as a depth target.
        DEPTH_TARGET = 2,
        /// Read by copies.
        TRANSFER_SRC = 3,
        /// Written by copies.
        TRANSFER_DST = 4,
        /// Written by what the layer does not wrap, as a storage image.
        STORAGE = 5,
    }
);

/// Where a buffer's memory is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Location {
    /// Device-local; written through staged copies.
    Gpu,
    /// Host-visible, written by the processor and read by the device.
    Upload,
    /// Host-visible, written by the device and read back by the processor.
    Readback,
}

/// A buffer to make.
#[derive(Debug, Clone, Copy)]
pub struct BufferDesc<'a> {
    /// Its name, for the driver's tools.
    pub label: &'a str,
    /// Its size in bytes.
    pub size: u64,
    /// What it is used for.
    pub usage: BufferUsage,
    /// Where its memory is.
    pub location: Location,
}

/// A 2D texture to make.
#[derive(Debug, Clone, Copy)]
pub struct TextureDesc<'a> {
    /// Its name, for the driver's tools.
    pub label: &'a str,
    /// Its width, pixels.
    pub width: u32,
    /// Its height, pixels.
    pub height: u32,
    /// How many mip levels it has, at least one.
    pub mips: u32,
    /// Its format.
    pub format: Format,
    /// What it is used for.
    pub usage: TextureUsage,
}

/// How a sampler filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    /// The nearest texel.
    Nearest,
    /// A linear blend.
    Linear,
}

/// What a sampler does past a texture's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    /// The texture repeats.
    Repeat,
    /// The edge texel is held.
    Clamp,
}

/// A sampler to make.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerDesc {
    /// The filter when the texture is magnified or minified.
    pub filter: Filter,
    /// The filter between mip levels.
    pub mip: Filter,
    /// Past the edge, across.
    pub address: Address,
    /// Past the edge, down.
    pub address_v: Address,
    /// The largest anisotropy, 1 for none.
    pub anisotropy: u32,
    /// The bias added to the mip level.
    pub lod_bias: f32,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            filter: Filter::Linear,
            mip: Filter::Linear,
            address: Address::Repeat,
            address_v: Address::Repeat,
            anisotropy: 1,
            lod_bias: 0.0,
        }
    }
}

/// A buffer the device holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Buffer(pub(crate) u32);

/// A texture the device holds, or an image it was lent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Texture(pub(crate) u32);

/// A sampler the device holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sampler(pub(crate) u32);

/// A pipeline the device holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pipeline(pub(crate) u32);

/// One shader stage: the entry point compiled for each backend, and its name.
#[derive(Debug, Clone, Copy)]
pub struct Shader<'a> {
    /// The SPIR-V module, for Vulkan.
    pub spirv: &'a [u32],
    /// The signed DXIL container, for Direct3D 12 (empty when the entry has none).
    pub dxil: &'a [u8],
    /// The entry point.
    pub entry: &'a str,
}

/// Which graphics API a device records with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Backend {
    /// Vulkan 1.3.
    #[default]
    Vulkan,
    /// Direct3D 12, shader model 6.6 (Windows).
    D3d12,
}

impl Backend {
    /// How many 32-bit words one indexed indirect draw command takes in this backend's layout:
    /// Vulkan's five (`VkDrawIndexedIndirectCommand`); on Direct3D 12 the draw's item, set as a
    /// root constant, then D3D12's five.
    #[must_use]
    pub const fn indexed_command_words(self) -> u32 {
        match self {
            Self::Vulkan => 5,
            Self::D3d12 => 6,
        }
    }

    /// One indexed indirect draw command in this backend's layout: `indices` indices from
    /// `first_index`, `instances` times, the draw's item `item` (its first instance on Vulkan).
    #[must_use]
    pub fn indexed_command(
        self,
        indices: u32,
        instances: u32,
        first_index: u32,
        vertex_offset: i32,
        item: u32,
    ) -> Vec<u32> {
        let offset = u32::from_ne_bytes(vertex_offset.to_ne_bytes());
        match self {
            Self::Vulkan => vec![indices, instances, first_index, offset, item],
            Self::D3d12 => vec![item, indices, instances, first_index, offset, item],
        }
    }

    /// How many 32-bit words one (non-indexed) indirect draw command takes.
    #[must_use]
    pub const fn command_words(self) -> u32 {
        match self {
            Self::Vulkan => 4,
            Self::D3d12 => 5,
        }
    }

    /// One indirect draw command in this backend's layout: `vertices` vertices from
    /// `first_vertex`, `instances` times, the draw's item `item`.
    #[must_use]
    pub fn command(self, vertices: u32, instances: u32, first_vertex: u32, item: u32) -> Vec<u32> {
        match self {
            Self::Vulkan => vec![vertices, instances, first_vertex, item],
            Self::D3d12 => vec![item, vertices, instances, first_vertex, item],
        }
    }
}

/// A blend factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Factor {
    /// 0.
    Zero,
    /// 1.
    One,
    /// The source's alpha.
    SrcAlpha,
    /// One minus the source's alpha.
    OneMinusSrcAlpha,
    /// The source's colour.
    SrcColor,
    /// The destination's colour.
    DstColor,
}

impl Factor {
    pub(crate) const fn vk(self) -> vk::BlendFactor {
        match self {
            Self::Zero => vk::BlendFactor::ZERO,
            Self::One => vk::BlendFactor::ONE,
            Self::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
            Self::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            Self::SrcColor => vk::BlendFactor::SRC_COLOR,
            Self::DstColor => vk::BlendFactor::DST_COLOR,
        }
    }
}

/// A colour target's blend: `src * Fs + dst * Fd`, the same factors for alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Blend {
    /// The source factor.
    pub src: Factor,
    /// The destination factor.
    pub dst: Factor,
}

/// One colour target of a pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorTarget {
    /// Its format.
    pub format: Format,
    /// Its blend, or `None` to write the shader's output as it is.
    pub blend: Option<Blend>,
    /// Whether alpha is written as well as red, green and blue.
    pub write_alpha: bool,
}

/// A depth comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compare {
    /// Always passes.
    Always,
    /// Passes when nearer.
    Less,
    /// Passes when nearer or equal.
    LessEqual,
    /// Passes when equal.
    Equal,
    /// Passes when farther or equal.
    GreaterEqual,
}

impl Compare {
    pub(crate) const fn vk(self) -> vk::CompareOp {
        match self {
            Self::Always => vk::CompareOp::ALWAYS,
            Self::Less => vk::CompareOp::LESS,
            Self::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
            Self::Equal => vk::CompareOp::EQUAL,
            Self::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        }
    }
}

/// A pipeline's depth target and test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DepthState {
    /// The depth target's format.
    pub format: Format,
    /// The test, `None` for no test.
    pub compare: Option<Compare>,
    /// Whether depth is written.
    pub write: bool,
}

/// Which faces are culled. Front faces are wound clockwise in the framebuffer space the
/// negative-height viewport gives, as the ordinary renderer has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cull {
    /// None.
    None,
    /// Front faces.
    Front,
    /// Back faces.
    Back,
}

/// A graphics pipeline to make. Vertices are pulled by the shaders, so it has no vertex input.
#[derive(Debug, Clone, Copy)]
pub struct GraphicsDesc<'a> {
    /// Its name, for the driver's tools.
    pub label: &'a str,
    /// The vertex stage.
    pub vertex: Shader<'a>,
    /// The fragment stage, if any.
    pub fragment: Option<Shader<'a>>,
    /// The colour targets.
    pub color: &'a [ColorTarget],
    /// The depth target and test.
    pub depth: Option<DepthState>,
    /// Face culling.
    pub cull: Cull,
}

/// A compute pipeline to make.
#[derive(Debug, Clone, Copy)]
pub struct ComputeDesc<'a> {
    /// Its name, for the driver's tools.
    pub label: &'a str,
    /// The compute stage.
    pub shader: Shader<'a>,
}

/// A rectangle of a target, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// An index's width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    /// 16 bits.
    U16,
    /// 32 bits.
    U32,
}

/// What the device counts, since it was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// Pipelines built.
    pub pipelines_built: u64,
    /// Bytes written to the device: into mapped buffers directly, or staged and copied.
    pub uploaded_bytes: u64,
    /// Buffers, textures and samplers alive.
    pub buffers: u32,
    /// Textures alive, lent images not counted.
    pub textures: u32,
    /// Samplers alive.
    pub samplers: u32,
    /// Recordings submitted or finished.
    pub recordings: u64,
    /// Acceleration structures alive, and their bytes.
    pub acceleration_structures: u32,
    pub acceleration_bytes: u64,
}
