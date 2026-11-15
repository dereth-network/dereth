// Written by `cargo xtask shaders` from `shaders.toml`. Regenerate it; never edit it.

/// The entry points `shaders.toml` names, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entry {
    /// `vs_main` in `fullscreen.slang`, the vertex stage.
    FullscreenVsMain,
    /// `fs_gtao` in `ao.slang`, the fragment stage.
    AoFsGtao,
    /// `fs_gather` in `ao.slang`, the fragment stage.
    AoFsGather,
    /// `fs_upsample` in `ao.slang`, the fragment stage.
    AoFsUpsample,
    /// `fs_main` in `shadow_mask.slang`, the fragment stage.
    ShadowMaskFsMain,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 5] = [Self::FullscreenVsMain, Self::AoFsGtao, Self::AoFsGather, Self::AoFsUpsample, Self::ShadowMaskFsMain];

    /// The source file that defines it.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::FullscreenVsMain => "fullscreen.slang",
            Self::AoFsGtao => "ao.slang",
            Self::AoFsGather => "ao.slang",
            Self::AoFsUpsample => "ao.slang",
            Self::ShadowMaskFsMain => "shadow_mask.slang",
        }
    }

    /// Its function, which is also its name in every blob.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FullscreenVsMain => "vs_main",
            Self::AoFsGtao => "fs_gtao",
            Self::AoFsGather => "fs_gather",
            Self::AoFsUpsample => "fs_upsample",
            Self::ShadowMaskFsMain => "fs_main",
        }
    }

    /// The stage it runs in.
    #[must_use]
    pub const fn stage(self) -> Stage {
        match self {
            Self::FullscreenVsMain => Stage::Vertex,
            Self::AoFsGtao => Stage::Fragment,
            Self::AoFsGather => Stage::Fragment,
            Self::AoFsUpsample => Stage::Fragment,
            Self::ShadowMaskFsMain => Stage::Fragment,
        }
    }

    /// Its SPIR-V blob, as the compiler wrote it.
    #[must_use]
    pub const fn spirv_bytes(self) -> &'static [u8] {
        match self {
            Self::FullscreenVsMain => include_bytes!("spirv/fullscreen.vs_main.spv"),
            Self::AoFsGtao => include_bytes!("spirv/ao.fs_gtao.spv"),
            Self::AoFsGather => include_bytes!("spirv/ao.fs_gather.spv"),
            Self::AoFsUpsample => include_bytes!("spirv/ao.fs_upsample.spv"),
            Self::ShadowMaskFsMain => include_bytes!("spirv/shadow_mask.fs_main.spv"),
        }
    }

    /// Its DXIL blob, as the compiler wrote it.
    #[must_use]
    pub const fn dxil_bytes(self) -> &'static [u8] {
        match self {
            Self::FullscreenVsMain => include_bytes!("dxil/fullscreen.vs_main.dxil"),
            Self::AoFsGtao => include_bytes!("dxil/ao.fs_gtao.dxil"),
            Self::AoFsGather => include_bytes!("dxil/ao.fs_gather.dxil"),
            Self::AoFsUpsample => include_bytes!("dxil/ao.fs_upsample.dxil"),
            Self::ShadowMaskFsMain => include_bytes!("dxil/shadow_mask.fs_main.dxil"),
        }
    }
}
