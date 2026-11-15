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
    /// `cs_clusters` in `lamps.slang`, the compute stage.
    LampsCsClusters,
    /// `vs_halo` in `halo.slang`, the vertex stage.
    HaloVsHalo,
    /// `fs_halo` in `halo.slang`, the fragment stage.
    HaloFsHalo,
    /// `fs_transmittance` in `sky_tables.slang`, the fragment stage.
    SkyTablesFsTransmittance,
    /// `fs_multiple` in `sky_tables.slang`, the fragment stage.
    SkyTablesFsMultiple,
    /// `fs_sky_view` in `sky_tables.slang`, the fragment stage.
    SkyTablesFsSkyView,
    /// `fs_sky` in `sky.slang`, the fragment stage.
    SkyFsSky,
    /// `fs_veil` in `sky.slang`, the fragment stage.
    SkyFsVeil,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 18] = [Self::FullscreenVsMain, Self::AoFsGtao, Self::AoFsGather, Self::AoFsUpsample, Self::ShadowMaskFsMain, Self::PostFsTaa, Self::PostFsTaaCheck, Self::PostFsBloomDown, Self::PostFsBloomUp, Self::PostFsTonemap, Self::LampsCsClusters, Self::HaloVsHalo, Self::HaloFsHalo, Self::SkyTablesFsTransmittance, Self::SkyTablesFsMultiple, Self::SkyTablesFsSkyView, Self::SkyFsSky, Self::SkyFsVeil];

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
            Self::LampsCsClusters => "lamps.slang",
            Self::HaloVsHalo => "halo.slang",
            Self::HaloFsHalo => "halo.slang",
            Self::SkyTablesFsTransmittance => "sky_tables.slang",
            Self::SkyTablesFsMultiple => "sky_tables.slang",
            Self::SkyTablesFsSkyView => "sky_tables.slang",
            Self::SkyFsSky => "sky.slang",
            Self::SkyFsVeil => "sky.slang",
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
            Self::LampsCsClusters => "cs_clusters",
            Self::HaloVsHalo => "vs_halo",
            Self::HaloFsHalo => "fs_halo",
            Self::SkyTablesFsTransmittance => "fs_transmittance",
            Self::SkyTablesFsMultiple => "fs_multiple",
            Self::SkyTablesFsSkyView => "fs_sky_view",
            Self::SkyFsSky => "fs_sky",
            Self::SkyFsVeil => "fs_veil",
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
            Self::LampsCsClusters => Stage::Compute,
            Self::HaloVsHalo => Stage::Vertex,
            Self::HaloFsHalo => Stage::Fragment,
            Self::SkyTablesFsTransmittance => Stage::Fragment,
            Self::SkyTablesFsMultiple => Stage::Fragment,
            Self::SkyTablesFsSkyView => Stage::Fragment,
            Self::SkyFsSky => Stage::Fragment,
            Self::SkyFsVeil => Stage::Fragment,
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
            Self::LampsCsClusters => include_bytes!("spirv/lamps.cs_clusters.spv"),
            Self::HaloVsHalo => include_bytes!("spirv/halo.vs_halo.spv"),
            Self::HaloFsHalo => include_bytes!("spirv/halo.fs_halo.spv"),
            Self::SkyTablesFsTransmittance => include_bytes!("spirv/sky_tables.fs_transmittance.spv"),
            Self::SkyTablesFsMultiple => include_bytes!("spirv/sky_tables.fs_multiple.spv"),
            Self::SkyTablesFsSkyView => include_bytes!("spirv/sky_tables.fs_sky_view.spv"),
            Self::SkyFsSky => include_bytes!("spirv/sky.fs_sky.spv"),
            Self::SkyFsVeil => include_bytes!("spirv/sky.fs_veil.spv"),
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
            Self::LampsCsClusters => include_bytes!("dxil/lamps.cs_clusters.dxil"),
            Self::HaloVsHalo => include_bytes!("dxil/halo.vs_halo.dxil"),
            Self::HaloFsHalo => include_bytes!("dxil/halo.fs_halo.dxil"),
            Self::SkyTablesFsTransmittance => include_bytes!("dxil/sky_tables.fs_transmittance.dxil"),
            Self::SkyTablesFsMultiple => include_bytes!("dxil/sky_tables.fs_multiple.dxil"),
            Self::SkyTablesFsSkyView => include_bytes!("dxil/sky_tables.fs_sky_view.dxil"),
            Self::SkyFsSky => include_bytes!("dxil/sky.fs_sky.dxil"),
            Self::SkyFsVeil => include_bytes!("dxil/sky.fs_veil.dxil"),
        }
    }
}
