// Written by `cargo xtask shaders` from `shaders.toml`. Regenerate it; never edit it.

/// The entry points `shaders.toml` names, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entry {
    /// `vs_main` in `triangle.slang`, the vertex stage.
    TriangleVsMain,
    /// `fs_main` in `triangle.slang`, the fragment stage.
    TriangleFsMain,
    /// `vs_main` in `textures.slang`, the vertex stage.
    TexturesVsMain,
    /// `fs_main` in `textures.slang`, the fragment stage.
    TexturesFsMain,
    /// `vs_main` in `items.slang`, the vertex stage.
    ItemsVsMain,
    /// `fs_main` in `items.slang`, the fragment stage.
    ItemsFsMain,
    /// `cs_main` in `fill.slang`, the compute stage.
    FillCsMain,
    /// `cs_main` in `trace.slang`, the compute stage.
    TraceCsMain,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 8] = [Self::TriangleVsMain, Self::TriangleFsMain, Self::TexturesVsMain, Self::TexturesFsMain, Self::ItemsVsMain, Self::ItemsFsMain, Self::FillCsMain, Self::TraceCsMain];

    /// The source file that defines it.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::TriangleVsMain => "triangle.slang",
            Self::TriangleFsMain => "triangle.slang",
            Self::TexturesVsMain => "textures.slang",
            Self::TexturesFsMain => "textures.slang",
            Self::ItemsVsMain => "items.slang",
            Self::ItemsFsMain => "items.slang",
            Self::FillCsMain => "fill.slang",
            Self::TraceCsMain => "trace.slang",
        }
    }

    /// Its function, which is also its name in every blob.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TriangleVsMain => "vs_main",
            Self::TriangleFsMain => "fs_main",
            Self::TexturesVsMain => "vs_main",
            Self::TexturesFsMain => "fs_main",
            Self::ItemsVsMain => "vs_main",
            Self::ItemsFsMain => "fs_main",
            Self::FillCsMain => "cs_main",
            Self::TraceCsMain => "cs_main",
        }
    }

    /// The stage it runs in.
    #[must_use]
    pub const fn stage(self) -> Stage {
        match self {
            Self::TriangleVsMain => Stage::Vertex,
            Self::TriangleFsMain => Stage::Fragment,
            Self::TexturesVsMain => Stage::Vertex,
            Self::TexturesFsMain => Stage::Fragment,
            Self::ItemsVsMain => Stage::Vertex,
            Self::ItemsFsMain => Stage::Fragment,
            Self::FillCsMain => Stage::Compute,
            Self::TraceCsMain => Stage::Compute,
        }
    }

    /// Its SPIR-V blob, as the compiler wrote it.
    #[must_use]
    pub const fn spirv_bytes(self) -> &'static [u8] {
        match self {
            Self::TriangleVsMain => include_bytes!("spirv/triangle.vs_main.spv"),
            Self::TriangleFsMain => include_bytes!("spirv/triangle.fs_main.spv"),
            Self::TexturesVsMain => include_bytes!("spirv/textures.vs_main.spv"),
            Self::TexturesFsMain => include_bytes!("spirv/textures.fs_main.spv"),
            Self::ItemsVsMain => include_bytes!("spirv/items.vs_main.spv"),
            Self::ItemsFsMain => include_bytes!("spirv/items.fs_main.spv"),
            Self::FillCsMain => include_bytes!("spirv/fill.cs_main.spv"),
            Self::TraceCsMain => include_bytes!("spirv/trace.cs_main.spv"),
        }
    }

    /// Its DXIL blob, as the compiler wrote it.
    #[must_use]
    pub const fn dxil_bytes(self) -> &'static [u8] {
        match self {
            Self::TriangleVsMain => include_bytes!("dxil/triangle.vs_main.dxil"),
            Self::TriangleFsMain => include_bytes!("dxil/triangle.fs_main.dxil"),
            Self::TexturesVsMain => include_bytes!("dxil/textures.vs_main.dxil"),
            Self::TexturesFsMain => include_bytes!("dxil/textures.fs_main.dxil"),
            Self::ItemsVsMain => include_bytes!("dxil/items.vs_main.dxil"),
            Self::ItemsFsMain => include_bytes!("dxil/items.fs_main.dxil"),
            Self::FillCsMain => include_bytes!("dxil/fill.cs_main.dxil"),
            Self::TraceCsMain => include_bytes!("dxil/trace.cs_main.dxil"),
        }
    }
}
