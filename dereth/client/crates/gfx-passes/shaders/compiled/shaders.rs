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
    /// `fs_taa` in `post.slang`, the fragment stage.
    PostFsTaa,
    /// `fs_taa_check` in `post.slang`, the fragment stage.
    PostFsTaaCheck,
    /// `fs_bloom_down` in `post.slang`, the fragment stage.
    PostFsBloomDown,
    /// `fs_bloom_up` in `post.slang`, the fragment stage.
    PostFsBloomUp,
    /// `fs_tonemap` in `post.slang`, the fragment stage.
    PostFsTonemap,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 10] = [Self::FullscreenVsMain, Self::AoFsGtao, Self::AoFsGather, Self::AoFsUpsample, Self::ShadowMaskFsMain, Self::PostFsTaa, Self::PostFsTaaCheck, Self::PostFsBloomDown, Self::PostFsBloomUp, Self::PostFsTonemap];

    /// The source file that defines it.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::FullscreenVsMain => "fullscreen.slang",
            Self::AoFsGtao => "ao.slang",
            Self::AoFsGather => "ao.slang",
            Self::AoFsUpsample => "ao.slang",
            Self::ShadowMaskFsMain => "shadow_mask.slang",
            Self::PostFsTaa => "post.slang",
            Self::PostFsTaaCheck => "post.slang",
            Self::PostFsBloomDown => "post.slang",
            Self::PostFsBloomUp => "post.slang",
            Self::PostFsTonemap => "post.slang",
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
            Self::PostFsTaa => "fs_taa",
            Self::PostFsTaaCheck => "fs_taa_check",
            Self::PostFsBloomDown => "fs_bloom_down",
            Self::PostFsBloomUp => "fs_bloom_up",
            Self::PostFsTonemap => "fs_tonemap",
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
            Self::PostFsTaa => Stage::Fragment,
            Self::PostFsTaaCheck => Stage::Fragment,
            Self::PostFsBloomDown => Stage::Fragment,
            Self::PostFsBloomUp => Stage::Fragment,
            Self::PostFsTonemap => Stage::Fragment,
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
            Self::PostFsTaa => include_bytes!("spirv/post.fs_taa.spv"),
            Self::PostFsTaaCheck => include_bytes!("spirv/post.fs_taa_check.spv"),
            Self::PostFsBloomDown => include_bytes!("spirv/post.fs_bloom_down.spv"),
            Self::PostFsBloomUp => include_bytes!("spirv/post.fs_bloom_up.spv"),
            Self::PostFsTonemap => include_bytes!("spirv/post.fs_tonemap.spv"),
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
            Self::PostFsTaa => include_bytes!("dxil/post.fs_taa.dxil"),
            Self::PostFsTaaCheck => include_bytes!("dxil/post.fs_taa_check.dxil"),
            Self::PostFsBloomDown => include_bytes!("dxil/post.fs_bloom_down.dxil"),
            Self::PostFsBloomUp => include_bytes!("dxil/post.fs_bloom_up.dxil"),
            Self::PostFsTonemap => include_bytes!("dxil/post.fs_tonemap.dxil"),
        }
    }
}
