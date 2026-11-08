// Written by `cargo xtask shaders` from `shaders.toml`. Regenerate it; never edit it.

/// The entry points `shaders.toml` names, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entry {
    /// `fs_air` in `air.slang`, the fragment stage.
    AirFsAir,
    /// `fs_main` in `cloud_shadow.slang`, the fragment stage.
    CloudShadowFsMain,
    /// `vs_main` in `stamp.slang`, the vertex stage.
    StampVsMain,
    /// `vs_main` in `fullscreen.slang`, the vertex stage.
    FullscreenVsMain,
    /// `fs_main` in `copy.slang`, the fragment stage.
    CopyFsMain,
    /// `vs_main` in `gbuffer.slang`, the vertex stage.
    GbufferVsMain,
    /// `fs_main` in `gbuffer.slang`, the fragment stage.
    GbufferFsMain,
    /// `fs_forward` in `gbuffer.slang`, the fragment stage.
    GbufferFsForward,
    /// `fs_edge` in `gbuffer.slang`, the fragment stage.
    GbufferFsEdge,
    /// `vs_main` in `terrain.slang`, the vertex stage.
    TerrainVsMain,
    /// `fs_main` in `terrain.slang`, the fragment stage.
    TerrainFsMain,
    /// `fs_vertex` in `resolve.slang`, the fragment stage.
    ResolveFsVertex,
    /// `fs_pixel` in `resolve.slang`, the fragment stage.
    ResolveFsPixel,
    /// `fs_main` in `composite.slang`, the fragment stage.
    CompositeFsMain,
    /// `cs_main` in `cull.slang`, the compute stage.
    CullCsMain,
    /// `fs_caster` in `gbuffer.slang`, the fragment stage.
    GbufferFsCaster,
    /// `fs_main` in `shadow_mask.slang`, the fragment stage.
    ShadowMaskFsMain,
    /// `fs_gtao` in `ao.slang`, the fragment stage.
    AoFsGtao,
    /// `fs_gather` in `ao.slang`, the fragment stage.
    AoFsGather,
    /// `fs_upsample` in `ao.slang`, the fragment stage.
    AoFsUpsample,
    /// `cs_clusters` in `lamps.slang`, the compute stage.
    LampsCsClusters,
    /// `fs_lamps` in `lamps.slang`, the fragment stage.
    LampsFsLamps,
    /// `vs_halo` in `halo.slang`, the vertex stage.
    HaloVsHalo,
    /// `fs_halo` in `halo.slang`, the fragment stage.
    HaloFsHalo,
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
    /// `vs_main` in `motion.slang`, the vertex stage.
    MotionVsMain,
    /// `fs_main` in `motion.slang`, the fragment stage.
    MotionFsMain,
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
    /// `cs_main` in `fog_volume.slang`, the compute stage.
    FogVolumeCsMain,
    /// `vs_ground` in `drawn.slang`, the vertex stage.
    DrawnVsGround,
    /// `fs_ground` in `drawn.slang`, the fragment stage.
    DrawnFsGround,
    /// `vs_caster` in `drawn.slang`, the vertex stage.
    DrawnVsCaster,
    /// `cs_probe` in `drawn.slang`, the compute stage.
    DrawnCsProbe,
    /// `cs_grass` in `drawn.slang`, the compute stage.
    DrawnCsGrass,
    /// `vs_blade` in `grass.slang`, the vertex stage.
    GrassVsBlade,
    /// `fs_blade` in `grass.slang`, the fragment stage.
    GrassFsBlade,
    /// `vs_blade_motion` in `grass.slang`, the vertex stage.
    GrassVsBladeMotion,
    /// `fs_blade_motion` in `grass.slang`, the fragment stage.
    GrassFsBladeMotion,
    /// `cs_scatter` in `drawn.slang`, the compute stage.
    DrawnCsScatter,
    /// `vs_scatter` in `grass.slang`, the vertex stage.
    GrassVsScatter,
    /// `fs_scatter` in `grass.slang`, the fragment stage.
    GrassFsScatter,
    /// `fs_main` in `water.slang`, the fragment stage.
    WaterFsMain,
    /// `fs_traced` in `water.slang`, the fragment stage.
    WaterFsTraced,
    /// `cs_bases` in `lift.slang`, the compute stage.
    LiftCsBases,
    /// `vs_bake` in `impostor.slang`, the vertex stage.
    ImpostorVsBake,
    /// `fs_bake` in `impostor.slang`, the fragment stage.
    ImpostorFsBake,
    /// `vs_draw` in `impostor.slang`, the vertex stage.
    ImpostorVsDraw,
    /// `fs_draw` in `impostor.slang`, the fragment stage.
    ImpostorFsDraw,
    /// `cs_cull` in `forest.slang`, the compute stage.
    ForestCsCull,
    /// `vs_far` in `forest.slang`, the vertex stage.
    ForestVsFar,
    /// `fs_far` in `forest.slang`, the fragment stage.
    ForestFsFar,
    /// `fs_splash` in `rain.slang`, the fragment stage.
    RainFsSplash,
    /// `vs_fall` in `rain.slang`, the vertex stage.
    RainVsFall,
    /// `fs_fall` in `rain.slang`, the fragment stage.
    RainFsFall,
    /// `cs_instances` in `rt.slang`, the compute stage.
    RtCsInstances,
    /// `cs_check` in `rt_check.slang`, the compute stage.
    RtCheckCsCheck,
    /// `cs_update` in `gi.slang`, the compute stage.
    GiCsUpdate,
    /// `fs_probes` in `gi.slang`, the fragment stage.
    GiFsProbes,
    /// `fs_screen` in `gi.slang`, the fragment stage.
    GiFsScreen,
    /// `fs_blur` in `gi.slang`, the fragment stage.
    GiFsBlur,
    /// `cs_trace` in `gi_trace.slang`, the compute stage.
    GiTraceCsTrace,
    /// `fs_main` in `rt_shadow.slang`, the fragment stage.
    RtShadowFsMain,
    /// `fs_rays` in `rt_ao.slang`, the fragment stage.
    RtAoFsRays,
    /// `fs_filter` in `rt_ao.slang`, the fragment stage.
    RtAoFsFilter,
    /// `cs_main` in `upscale_inputs.slang`, the compute stage.
    UpscaleInputsCsMain,
    /// `cs_clear_reconstructed` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsClearReconstructed,
    /// `cs_prepare_inputs` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsPrepareInputs,
    /// `cs_farthest_half` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsFarthestHalf,
    /// `cs_shading_level_zero` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsShadingLevelZero,
    /// `cs_shading_level` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsShadingLevel,
    /// `cs_shading_change` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsShadingChange,
    /// `cs_reactivity` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsReactivity,
    /// `cs_luma_instability` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsLumaInstability,
    /// `cs_accumulate` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsAccumulate,
    /// `cs_rcas` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsRcas,
    /// `fs_show` in `fsr3_passes.slang`, the fragment stage.
    Fsr3PassesFsShow,
    /// `cs_reactive_input` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsReactiveInput,
    /// `cs_exposure` in `fsr3_passes.slang`, the compute stage.
    Fsr3PassesCsExposure,
    /// `fs_inputs_as_pictures` in `fsr3_passes.slang`, the fragment stage.
    Fsr3PassesFsInputsAsPictures,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 87] = [Self::AirFsAir, Self::CloudShadowFsMain, Self::StampVsMain, Self::FullscreenVsMain, Self::CopyFsMain, Self::GbufferVsMain, Self::GbufferFsMain, Self::GbufferFsForward, Self::GbufferFsEdge, Self::TerrainVsMain, Self::TerrainFsMain, Self::ResolveFsVertex, Self::ResolveFsPixel, Self::CompositeFsMain, Self::CullCsMain, Self::GbufferFsCaster, Self::ShadowMaskFsMain, Self::AoFsGtao, Self::AoFsGather, Self::AoFsUpsample, Self::LampsCsClusters, Self::LampsFsLamps, Self::HaloVsHalo, Self::HaloFsHalo, Self::PostFsTaa, Self::PostFsTaaCheck, Self::PostFsBloomDown, Self::PostFsBloomUp, Self::PostFsTonemap, Self::MotionVsMain, Self::MotionFsMain, Self::SkyTablesFsTransmittance, Self::SkyTablesFsMultiple, Self::SkyTablesFsSkyView, Self::SkyFsSky, Self::SkyFsVeil, Self::FogVolumeCsMain, Self::DrawnVsGround, Self::DrawnFsGround, Self::DrawnVsCaster, Self::DrawnCsProbe, Self::DrawnCsGrass, Self::GrassVsBlade, Self::GrassFsBlade, Self::GrassVsBladeMotion, Self::GrassFsBladeMotion, Self::DrawnCsScatter, Self::GrassVsScatter, Self::GrassFsScatter, Self::WaterFsMain, Self::WaterFsTraced, Self::LiftCsBases, Self::ImpostorVsBake, Self::ImpostorFsBake, Self::ImpostorVsDraw, Self::ImpostorFsDraw, Self::ForestCsCull, Self::ForestVsFar, Self::ForestFsFar, Self::RainFsSplash, Self::RainVsFall, Self::RainFsFall, Self::RtCsInstances, Self::RtCheckCsCheck, Self::GiCsUpdate, Self::GiFsProbes, Self::GiFsScreen, Self::GiFsBlur, Self::GiTraceCsTrace, Self::RtShadowFsMain, Self::RtAoFsRays, Self::RtAoFsFilter, Self::UpscaleInputsCsMain, Self::Fsr3PassesCsClearReconstructed, Self::Fsr3PassesCsPrepareInputs, Self::Fsr3PassesCsFarthestHalf, Self::Fsr3PassesCsShadingLevelZero, Self::Fsr3PassesCsShadingLevel, Self::Fsr3PassesCsShadingChange, Self::Fsr3PassesCsReactivity, Self::Fsr3PassesCsLumaInstability, Self::Fsr3PassesCsAccumulate, Self::Fsr3PassesCsRcas, Self::Fsr3PassesFsShow, Self::Fsr3PassesCsReactiveInput, Self::Fsr3PassesCsExposure, Self::Fsr3PassesFsInputsAsPictures];

    /// The source file that defines it.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::AirFsAir => "air.slang",
            Self::CloudShadowFsMain => "cloud_shadow.slang",
            Self::StampVsMain => "stamp.slang",
            Self::FullscreenVsMain => "fullscreen.slang",
            Self::CopyFsMain => "copy.slang",
            Self::GbufferVsMain => "gbuffer.slang",
            Self::GbufferFsMain => "gbuffer.slang",
            Self::GbufferFsForward => "gbuffer.slang",
            Self::GbufferFsEdge => "gbuffer.slang",
            Self::TerrainVsMain => "terrain.slang",
            Self::TerrainFsMain => "terrain.slang",
            Self::ResolveFsVertex => "resolve.slang",
            Self::ResolveFsPixel => "resolve.slang",
            Self::CompositeFsMain => "composite.slang",
            Self::CullCsMain => "cull.slang",
            Self::GbufferFsCaster => "gbuffer.slang",
            Self::ShadowMaskFsMain => "shadow_mask.slang",
            Self::AoFsGtao => "ao.slang",
            Self::AoFsGather => "ao.slang",
            Self::AoFsUpsample => "ao.slang",
            Self::LampsCsClusters => "lamps.slang",
            Self::LampsFsLamps => "lamps.slang",
            Self::HaloVsHalo => "halo.slang",
            Self::HaloFsHalo => "halo.slang",
            Self::PostFsTaa => "post.slang",
            Self::PostFsTaaCheck => "post.slang",
            Self::PostFsBloomDown => "post.slang",
            Self::PostFsBloomUp => "post.slang",
            Self::PostFsTonemap => "post.slang",
            Self::MotionVsMain => "motion.slang",
            Self::MotionFsMain => "motion.slang",
            Self::SkyTablesFsTransmittance => "sky_tables.slang",
            Self::SkyTablesFsMultiple => "sky_tables.slang",
            Self::SkyTablesFsSkyView => "sky_tables.slang",
            Self::SkyFsSky => "sky.slang",
            Self::SkyFsVeil => "sky.slang",
            Self::FogVolumeCsMain => "fog_volume.slang",
            Self::DrawnVsGround => "drawn.slang",
            Self::DrawnFsGround => "drawn.slang",
            Self::DrawnVsCaster => "drawn.slang",
            Self::DrawnCsProbe => "drawn.slang",
            Self::DrawnCsGrass => "drawn.slang",
            Self::GrassVsBlade => "grass.slang",
            Self::GrassFsBlade => "grass.slang",
            Self::GrassVsBladeMotion => "grass.slang",
            Self::GrassFsBladeMotion => "grass.slang",
            Self::DrawnCsScatter => "drawn.slang",
            Self::GrassVsScatter => "grass.slang",
            Self::GrassFsScatter => "grass.slang",
            Self::WaterFsMain => "water.slang",
            Self::WaterFsTraced => "water.slang",
            Self::LiftCsBases => "lift.slang",
            Self::ImpostorVsBake => "impostor.slang",
            Self::ImpostorFsBake => "impostor.slang",
            Self::ImpostorVsDraw => "impostor.slang",
            Self::ImpostorFsDraw => "impostor.slang",
            Self::ForestCsCull => "forest.slang",
            Self::ForestVsFar => "forest.slang",
            Self::ForestFsFar => "forest.slang",
            Self::RainFsSplash => "rain.slang",
            Self::RainVsFall => "rain.slang",
            Self::RainFsFall => "rain.slang",
            Self::RtCsInstances => "rt.slang",
            Self::RtCheckCsCheck => "rt_check.slang",
            Self::GiCsUpdate => "gi.slang",
            Self::GiFsProbes => "gi.slang",
            Self::GiFsScreen => "gi.slang",
            Self::GiFsBlur => "gi.slang",
            Self::GiTraceCsTrace => "gi_trace.slang",
            Self::RtShadowFsMain => "rt_shadow.slang",
            Self::RtAoFsRays => "rt_ao.slang",
            Self::RtAoFsFilter => "rt_ao.slang",
            Self::UpscaleInputsCsMain => "upscale_inputs.slang",
            Self::Fsr3PassesCsClearReconstructed => "fsr3_passes.slang",
            Self::Fsr3PassesCsPrepareInputs => "fsr3_passes.slang",
            Self::Fsr3PassesCsFarthestHalf => "fsr3_passes.slang",
            Self::Fsr3PassesCsShadingLevelZero => "fsr3_passes.slang",
            Self::Fsr3PassesCsShadingLevel => "fsr3_passes.slang",
            Self::Fsr3PassesCsShadingChange => "fsr3_passes.slang",
            Self::Fsr3PassesCsReactivity => "fsr3_passes.slang",
            Self::Fsr3PassesCsLumaInstability => "fsr3_passes.slang",
            Self::Fsr3PassesCsAccumulate => "fsr3_passes.slang",
            Self::Fsr3PassesCsRcas => "fsr3_passes.slang",
            Self::Fsr3PassesFsShow => "fsr3_passes.slang",
            Self::Fsr3PassesCsReactiveInput => "fsr3_passes.slang",
            Self::Fsr3PassesCsExposure => "fsr3_passes.slang",
            Self::Fsr3PassesFsInputsAsPictures => "fsr3_passes.slang",
        }
    }

    /// Its function, which is also its name in every blob.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AirFsAir => "fs_air",
            Self::CloudShadowFsMain => "fs_main",
            Self::StampVsMain => "vs_main",
            Self::FullscreenVsMain => "vs_main",
            Self::CopyFsMain => "fs_main",
            Self::GbufferVsMain => "vs_main",
            Self::GbufferFsMain => "fs_main",
            Self::GbufferFsForward => "fs_forward",
            Self::GbufferFsEdge => "fs_edge",
            Self::TerrainVsMain => "vs_main",
            Self::TerrainFsMain => "fs_main",
            Self::ResolveFsVertex => "fs_vertex",
            Self::ResolveFsPixel => "fs_pixel",
            Self::CompositeFsMain => "fs_main",
            Self::CullCsMain => "cs_main",
            Self::GbufferFsCaster => "fs_caster",
            Self::ShadowMaskFsMain => "fs_main",
            Self::AoFsGtao => "fs_gtao",
            Self::AoFsGather => "fs_gather",
            Self::AoFsUpsample => "fs_upsample",
            Self::LampsCsClusters => "cs_clusters",
            Self::LampsFsLamps => "fs_lamps",
            Self::HaloVsHalo => "vs_halo",
            Self::HaloFsHalo => "fs_halo",
            Self::PostFsTaa => "fs_taa",
            Self::PostFsTaaCheck => "fs_taa_check",
            Self::PostFsBloomDown => "fs_bloom_down",
            Self::PostFsBloomUp => "fs_bloom_up",
            Self::PostFsTonemap => "fs_tonemap",
            Self::MotionVsMain => "vs_main",
            Self::MotionFsMain => "fs_main",
            Self::SkyTablesFsTransmittance => "fs_transmittance",
            Self::SkyTablesFsMultiple => "fs_multiple",
            Self::SkyTablesFsSkyView => "fs_sky_view",
            Self::SkyFsSky => "fs_sky",
            Self::SkyFsVeil => "fs_veil",
            Self::FogVolumeCsMain => "cs_main",
            Self::DrawnVsGround => "vs_ground",
            Self::DrawnFsGround => "fs_ground",
            Self::DrawnVsCaster => "vs_caster",
            Self::DrawnCsProbe => "cs_probe",
            Self::DrawnCsGrass => "cs_grass",
            Self::GrassVsBlade => "vs_blade",
            Self::GrassFsBlade => "fs_blade",
            Self::GrassVsBladeMotion => "vs_blade_motion",
            Self::GrassFsBladeMotion => "fs_blade_motion",
            Self::DrawnCsScatter => "cs_scatter",
            Self::GrassVsScatter => "vs_scatter",
            Self::GrassFsScatter => "fs_scatter",
            Self::WaterFsMain => "fs_main",
            Self::WaterFsTraced => "fs_traced",
            Self::LiftCsBases => "cs_bases",
            Self::ImpostorVsBake => "vs_bake",
            Self::ImpostorFsBake => "fs_bake",
            Self::ImpostorVsDraw => "vs_draw",
            Self::ImpostorFsDraw => "fs_draw",
            Self::ForestCsCull => "cs_cull",
            Self::ForestVsFar => "vs_far",
            Self::ForestFsFar => "fs_far",
            Self::RainFsSplash => "fs_splash",
            Self::RainVsFall => "vs_fall",
            Self::RainFsFall => "fs_fall",
            Self::RtCsInstances => "cs_instances",
            Self::RtCheckCsCheck => "cs_check",
            Self::GiCsUpdate => "cs_update",
            Self::GiFsProbes => "fs_probes",
            Self::GiFsScreen => "fs_screen",
            Self::GiFsBlur => "fs_blur",
            Self::GiTraceCsTrace => "cs_trace",
            Self::RtShadowFsMain => "fs_main",
            Self::RtAoFsRays => "fs_rays",
            Self::RtAoFsFilter => "fs_filter",
            Self::UpscaleInputsCsMain => "cs_main",
            Self::Fsr3PassesCsClearReconstructed => "cs_clear_reconstructed",
            Self::Fsr3PassesCsPrepareInputs => "cs_prepare_inputs",
            Self::Fsr3PassesCsFarthestHalf => "cs_farthest_half",
            Self::Fsr3PassesCsShadingLevelZero => "cs_shading_level_zero",
            Self::Fsr3PassesCsShadingLevel => "cs_shading_level",
            Self::Fsr3PassesCsShadingChange => "cs_shading_change",
            Self::Fsr3PassesCsReactivity => "cs_reactivity",
            Self::Fsr3PassesCsLumaInstability => "cs_luma_instability",
            Self::Fsr3PassesCsAccumulate => "cs_accumulate",
            Self::Fsr3PassesCsRcas => "cs_rcas",
            Self::Fsr3PassesFsShow => "fs_show",
            Self::Fsr3PassesCsReactiveInput => "cs_reactive_input",
            Self::Fsr3PassesCsExposure => "cs_exposure",
            Self::Fsr3PassesFsInputsAsPictures => "fs_inputs_as_pictures",
        }
    }

    /// The stage it runs in.
    #[must_use]
    pub const fn stage(self) -> Stage {
        match self {
            Self::AirFsAir => Stage::Fragment,
            Self::CloudShadowFsMain => Stage::Fragment,
            Self::StampVsMain => Stage::Vertex,
            Self::FullscreenVsMain => Stage::Vertex,
            Self::CopyFsMain => Stage::Fragment,
            Self::GbufferVsMain => Stage::Vertex,
            Self::GbufferFsMain => Stage::Fragment,
            Self::GbufferFsForward => Stage::Fragment,
            Self::GbufferFsEdge => Stage::Fragment,
            Self::TerrainVsMain => Stage::Vertex,
            Self::TerrainFsMain => Stage::Fragment,
            Self::ResolveFsVertex => Stage::Fragment,
            Self::ResolveFsPixel => Stage::Fragment,
            Self::CompositeFsMain => Stage::Fragment,
            Self::CullCsMain => Stage::Compute,
            Self::GbufferFsCaster => Stage::Fragment,
            Self::ShadowMaskFsMain => Stage::Fragment,
            Self::AoFsGtao => Stage::Fragment,
            Self::AoFsGather => Stage::Fragment,
            Self::AoFsUpsample => Stage::Fragment,
            Self::LampsCsClusters => Stage::Compute,
            Self::LampsFsLamps => Stage::Fragment,
            Self::HaloVsHalo => Stage::Vertex,
            Self::HaloFsHalo => Stage::Fragment,
            Self::PostFsTaa => Stage::Fragment,
            Self::PostFsTaaCheck => Stage::Fragment,
            Self::PostFsBloomDown => Stage::Fragment,
            Self::PostFsBloomUp => Stage::Fragment,
            Self::PostFsTonemap => Stage::Fragment,
            Self::MotionVsMain => Stage::Vertex,
            Self::MotionFsMain => Stage::Fragment,
            Self::SkyTablesFsTransmittance => Stage::Fragment,
            Self::SkyTablesFsMultiple => Stage::Fragment,
            Self::SkyTablesFsSkyView => Stage::Fragment,
            Self::SkyFsSky => Stage::Fragment,
            Self::SkyFsVeil => Stage::Fragment,
            Self::FogVolumeCsMain => Stage::Compute,
            Self::DrawnVsGround => Stage::Vertex,
            Self::DrawnFsGround => Stage::Fragment,
            Self::DrawnVsCaster => Stage::Vertex,
            Self::DrawnCsProbe => Stage::Compute,
            Self::DrawnCsGrass => Stage::Compute,
            Self::GrassVsBlade => Stage::Vertex,
            Self::GrassFsBlade => Stage::Fragment,
            Self::GrassVsBladeMotion => Stage::Vertex,
            Self::GrassFsBladeMotion => Stage::Fragment,
            Self::DrawnCsScatter => Stage::Compute,
            Self::GrassVsScatter => Stage::Vertex,
            Self::GrassFsScatter => Stage::Fragment,
            Self::WaterFsMain => Stage::Fragment,
            Self::WaterFsTraced => Stage::Fragment,
            Self::LiftCsBases => Stage::Compute,
            Self::ImpostorVsBake => Stage::Vertex,
            Self::ImpostorFsBake => Stage::Fragment,
            Self::ImpostorVsDraw => Stage::Vertex,
            Self::ImpostorFsDraw => Stage::Fragment,
            Self::ForestCsCull => Stage::Compute,
            Self::ForestVsFar => Stage::Vertex,
            Self::ForestFsFar => Stage::Fragment,
            Self::RainFsSplash => Stage::Fragment,
            Self::RainVsFall => Stage::Vertex,
            Self::RainFsFall => Stage::Fragment,
            Self::RtCsInstances => Stage::Compute,
            Self::RtCheckCsCheck => Stage::Compute,
            Self::GiCsUpdate => Stage::Compute,
            Self::GiFsProbes => Stage::Fragment,
            Self::GiFsScreen => Stage::Fragment,
            Self::GiFsBlur => Stage::Fragment,
            Self::GiTraceCsTrace => Stage::Compute,
            Self::RtShadowFsMain => Stage::Fragment,
            Self::RtAoFsRays => Stage::Fragment,
            Self::RtAoFsFilter => Stage::Fragment,
            Self::UpscaleInputsCsMain => Stage::Compute,
            Self::Fsr3PassesCsClearReconstructed => Stage::Compute,
            Self::Fsr3PassesCsPrepareInputs => Stage::Compute,
            Self::Fsr3PassesCsFarthestHalf => Stage::Compute,
            Self::Fsr3PassesCsShadingLevelZero => Stage::Compute,
            Self::Fsr3PassesCsShadingLevel => Stage::Compute,
            Self::Fsr3PassesCsShadingChange => Stage::Compute,
            Self::Fsr3PassesCsReactivity => Stage::Compute,
            Self::Fsr3PassesCsLumaInstability => Stage::Compute,
            Self::Fsr3PassesCsAccumulate => Stage::Compute,
            Self::Fsr3PassesCsRcas => Stage::Compute,
            Self::Fsr3PassesFsShow => Stage::Fragment,
            Self::Fsr3PassesCsReactiveInput => Stage::Compute,
            Self::Fsr3PassesCsExposure => Stage::Compute,
            Self::Fsr3PassesFsInputsAsPictures => Stage::Fragment,
        }
    }

    /// Its SPIR-V blob, as the compiler wrote it.
    #[must_use]
    pub const fn spirv_bytes(self) -> &'static [u8] {
        match self {
            Self::AirFsAir => include_bytes!("spirv/air.fs_air.spv"),
            Self::CloudShadowFsMain => include_bytes!("spirv/cloud_shadow.fs_main.spv"),
            Self::StampVsMain => include_bytes!("spirv/stamp.vs_main.spv"),
            Self::FullscreenVsMain => include_bytes!("spirv/fullscreen.vs_main.spv"),
            Self::CopyFsMain => include_bytes!("spirv/copy.fs_main.spv"),
            Self::GbufferVsMain => include_bytes!("spirv/gbuffer.vs_main.spv"),
            Self::GbufferFsMain => include_bytes!("spirv/gbuffer.fs_main.spv"),
            Self::GbufferFsForward => include_bytes!("spirv/gbuffer.fs_forward.spv"),
            Self::GbufferFsEdge => include_bytes!("spirv/gbuffer.fs_edge.spv"),
            Self::TerrainVsMain => include_bytes!("spirv/terrain.vs_main.spv"),
            Self::TerrainFsMain => include_bytes!("spirv/terrain.fs_main.spv"),
            Self::ResolveFsVertex => include_bytes!("spirv/resolve.fs_vertex.spv"),
            Self::ResolveFsPixel => include_bytes!("spirv/resolve.fs_pixel.spv"),
            Self::CompositeFsMain => include_bytes!("spirv/composite.fs_main.spv"),
            Self::CullCsMain => include_bytes!("spirv/cull.cs_main.spv"),
            Self::GbufferFsCaster => include_bytes!("spirv/gbuffer.fs_caster.spv"),
            Self::ShadowMaskFsMain => include_bytes!("spirv/shadow_mask.fs_main.spv"),
            Self::AoFsGtao => include_bytes!("spirv/ao.fs_gtao.spv"),
            Self::AoFsGather => include_bytes!("spirv/ao.fs_gather.spv"),
            Self::AoFsUpsample => include_bytes!("spirv/ao.fs_upsample.spv"),
            Self::LampsCsClusters => include_bytes!("spirv/lamps.cs_clusters.spv"),
            Self::LampsFsLamps => include_bytes!("spirv/lamps.fs_lamps.spv"),
            Self::HaloVsHalo => include_bytes!("spirv/halo.vs_halo.spv"),
            Self::HaloFsHalo => include_bytes!("spirv/halo.fs_halo.spv"),
            Self::PostFsTaa => include_bytes!("spirv/post.fs_taa.spv"),
            Self::PostFsTaaCheck => include_bytes!("spirv/post.fs_taa_check.spv"),
            Self::PostFsBloomDown => include_bytes!("spirv/post.fs_bloom_down.spv"),
            Self::PostFsBloomUp => include_bytes!("spirv/post.fs_bloom_up.spv"),
            Self::PostFsTonemap => include_bytes!("spirv/post.fs_tonemap.spv"),
            Self::MotionVsMain => include_bytes!("spirv/motion.vs_main.spv"),
            Self::MotionFsMain => include_bytes!("spirv/motion.fs_main.spv"),
            Self::SkyTablesFsTransmittance => include_bytes!("spirv/sky_tables.fs_transmittance.spv"),
            Self::SkyTablesFsMultiple => include_bytes!("spirv/sky_tables.fs_multiple.spv"),
            Self::SkyTablesFsSkyView => include_bytes!("spirv/sky_tables.fs_sky_view.spv"),
            Self::SkyFsSky => include_bytes!("spirv/sky.fs_sky.spv"),
            Self::SkyFsVeil => include_bytes!("spirv/sky.fs_veil.spv"),
            Self::FogVolumeCsMain => include_bytes!("spirv/fog_volume.cs_main.spv"),
            Self::DrawnVsGround => include_bytes!("spirv/drawn.vs_ground.spv"),
            Self::DrawnFsGround => include_bytes!("spirv/drawn.fs_ground.spv"),
            Self::DrawnVsCaster => include_bytes!("spirv/drawn.vs_caster.spv"),
            Self::DrawnCsProbe => include_bytes!("spirv/drawn.cs_probe.spv"),
            Self::DrawnCsGrass => include_bytes!("spirv/drawn.cs_grass.spv"),
            Self::GrassVsBlade => include_bytes!("spirv/grass.vs_blade.spv"),
            Self::GrassFsBlade => include_bytes!("spirv/grass.fs_blade.spv"),
            Self::GrassVsBladeMotion => include_bytes!("spirv/grass.vs_blade_motion.spv"),
            Self::GrassFsBladeMotion => include_bytes!("spirv/grass.fs_blade_motion.spv"),
            Self::DrawnCsScatter => include_bytes!("spirv/drawn.cs_scatter.spv"),
            Self::GrassVsScatter => include_bytes!("spirv/grass.vs_scatter.spv"),
            Self::GrassFsScatter => include_bytes!("spirv/grass.fs_scatter.spv"),
            Self::WaterFsMain => include_bytes!("spirv/water.fs_main.spv"),
            Self::WaterFsTraced => include_bytes!("spirv/water.fs_traced.spv"),
            Self::LiftCsBases => include_bytes!("spirv/lift.cs_bases.spv"),
            Self::ImpostorVsBake => include_bytes!("spirv/impostor.vs_bake.spv"),
            Self::ImpostorFsBake => include_bytes!("spirv/impostor.fs_bake.spv"),
            Self::ImpostorVsDraw => include_bytes!("spirv/impostor.vs_draw.spv"),
            Self::ImpostorFsDraw => include_bytes!("spirv/impostor.fs_draw.spv"),
            Self::ForestCsCull => include_bytes!("spirv/forest.cs_cull.spv"),
            Self::ForestVsFar => include_bytes!("spirv/forest.vs_far.spv"),
            Self::ForestFsFar => include_bytes!("spirv/forest.fs_far.spv"),
            Self::RainFsSplash => include_bytes!("spirv/rain.fs_splash.spv"),
            Self::RainVsFall => include_bytes!("spirv/rain.vs_fall.spv"),
            Self::RainFsFall => include_bytes!("spirv/rain.fs_fall.spv"),
            Self::RtCsInstances => include_bytes!("spirv/rt.cs_instances.spv"),
            Self::RtCheckCsCheck => include_bytes!("spirv/rt_check.cs_check.spv"),
            Self::GiCsUpdate => include_bytes!("spirv/gi.cs_update.spv"),
            Self::GiFsProbes => include_bytes!("spirv/gi.fs_probes.spv"),
            Self::GiFsScreen => include_bytes!("spirv/gi.fs_screen.spv"),
            Self::GiFsBlur => include_bytes!("spirv/gi.fs_blur.spv"),
            Self::GiTraceCsTrace => include_bytes!("spirv/gi_trace.cs_trace.spv"),
            Self::RtShadowFsMain => include_bytes!("spirv/rt_shadow.fs_main.spv"),
            Self::RtAoFsRays => include_bytes!("spirv/rt_ao.fs_rays.spv"),
            Self::RtAoFsFilter => include_bytes!("spirv/rt_ao.fs_filter.spv"),
            Self::UpscaleInputsCsMain => include_bytes!("spirv/upscale_inputs.cs_main.spv"),
            Self::Fsr3PassesCsClearReconstructed => include_bytes!("spirv/fsr3_passes.cs_clear_reconstructed.spv"),
            Self::Fsr3PassesCsPrepareInputs => include_bytes!("spirv/fsr3_passes.cs_prepare_inputs.spv"),
            Self::Fsr3PassesCsFarthestHalf => include_bytes!("spirv/fsr3_passes.cs_farthest_half.spv"),
            Self::Fsr3PassesCsShadingLevelZero => include_bytes!("spirv/fsr3_passes.cs_shading_level_zero.spv"),
            Self::Fsr3PassesCsShadingLevel => include_bytes!("spirv/fsr3_passes.cs_shading_level.spv"),
            Self::Fsr3PassesCsShadingChange => include_bytes!("spirv/fsr3_passes.cs_shading_change.spv"),
            Self::Fsr3PassesCsReactivity => include_bytes!("spirv/fsr3_passes.cs_reactivity.spv"),
            Self::Fsr3PassesCsLumaInstability => include_bytes!("spirv/fsr3_passes.cs_luma_instability.spv"),
            Self::Fsr3PassesCsAccumulate => include_bytes!("spirv/fsr3_passes.cs_accumulate.spv"),
            Self::Fsr3PassesCsRcas => include_bytes!("spirv/fsr3_passes.cs_rcas.spv"),
            Self::Fsr3PassesFsShow => include_bytes!("spirv/fsr3_passes.fs_show.spv"),
            Self::Fsr3PassesCsReactiveInput => include_bytes!("spirv/fsr3_passes.cs_reactive_input.spv"),
            Self::Fsr3PassesCsExposure => include_bytes!("spirv/fsr3_passes.cs_exposure.spv"),
            Self::Fsr3PassesFsInputsAsPictures => include_bytes!("spirv/fsr3_passes.fs_inputs_as_pictures.spv"),
        }
    }

    /// Its DXIL blob, as the compiler wrote it.
    #[must_use]
    pub const fn dxil_bytes(self) -> &'static [u8] {
        match self {
            Self::AirFsAir => include_bytes!("dxil/air.fs_air.dxil"),
            Self::CloudShadowFsMain => include_bytes!("dxil/cloud_shadow.fs_main.dxil"),
            Self::StampVsMain => include_bytes!("dxil/stamp.vs_main.dxil"),
            Self::FullscreenVsMain => include_bytes!("dxil/fullscreen.vs_main.dxil"),
            Self::CopyFsMain => include_bytes!("dxil/copy.fs_main.dxil"),
            Self::GbufferVsMain => include_bytes!("dxil/gbuffer.vs_main.dxil"),
            Self::GbufferFsMain => include_bytes!("dxil/gbuffer.fs_main.dxil"),
            Self::GbufferFsForward => include_bytes!("dxil/gbuffer.fs_forward.dxil"),
            Self::GbufferFsEdge => include_bytes!("dxil/gbuffer.fs_edge.dxil"),
            Self::TerrainVsMain => include_bytes!("dxil/terrain.vs_main.dxil"),
            Self::TerrainFsMain => include_bytes!("dxil/terrain.fs_main.dxil"),
            Self::ResolveFsVertex => include_bytes!("dxil/resolve.fs_vertex.dxil"),
            Self::ResolveFsPixel => include_bytes!("dxil/resolve.fs_pixel.dxil"),
            Self::CompositeFsMain => include_bytes!("dxil/composite.fs_main.dxil"),
            Self::CullCsMain => include_bytes!("dxil/cull.cs_main.dxil"),
            Self::GbufferFsCaster => include_bytes!("dxil/gbuffer.fs_caster.dxil"),
            Self::ShadowMaskFsMain => include_bytes!("dxil/shadow_mask.fs_main.dxil"),
            Self::AoFsGtao => include_bytes!("dxil/ao.fs_gtao.dxil"),
            Self::AoFsGather => include_bytes!("dxil/ao.fs_gather.dxil"),
            Self::AoFsUpsample => include_bytes!("dxil/ao.fs_upsample.dxil"),
            Self::LampsCsClusters => include_bytes!("dxil/lamps.cs_clusters.dxil"),
            Self::LampsFsLamps => include_bytes!("dxil/lamps.fs_lamps.dxil"),
            Self::HaloVsHalo => include_bytes!("dxil/halo.vs_halo.dxil"),
            Self::HaloFsHalo => include_bytes!("dxil/halo.fs_halo.dxil"),
            Self::PostFsTaa => include_bytes!("dxil/post.fs_taa.dxil"),
            Self::PostFsTaaCheck => include_bytes!("dxil/post.fs_taa_check.dxil"),
            Self::PostFsBloomDown => include_bytes!("dxil/post.fs_bloom_down.dxil"),
            Self::PostFsBloomUp => include_bytes!("dxil/post.fs_bloom_up.dxil"),
            Self::PostFsTonemap => include_bytes!("dxil/post.fs_tonemap.dxil"),
            Self::MotionVsMain => include_bytes!("dxil/motion.vs_main.dxil"),
            Self::MotionFsMain => include_bytes!("dxil/motion.fs_main.dxil"),
            Self::SkyTablesFsTransmittance => include_bytes!("dxil/sky_tables.fs_transmittance.dxil"),
            Self::SkyTablesFsMultiple => include_bytes!("dxil/sky_tables.fs_multiple.dxil"),
            Self::SkyTablesFsSkyView => include_bytes!("dxil/sky_tables.fs_sky_view.dxil"),
            Self::SkyFsSky => include_bytes!("dxil/sky.fs_sky.dxil"),
            Self::SkyFsVeil => include_bytes!("dxil/sky.fs_veil.dxil"),
            Self::FogVolumeCsMain => include_bytes!("dxil/fog_volume.cs_main.dxil"),
            Self::DrawnVsGround => include_bytes!("dxil/drawn.vs_ground.dxil"),
            Self::DrawnFsGround => include_bytes!("dxil/drawn.fs_ground.dxil"),
            Self::DrawnVsCaster => include_bytes!("dxil/drawn.vs_caster.dxil"),
            Self::DrawnCsProbe => include_bytes!("dxil/drawn.cs_probe.dxil"),
            Self::DrawnCsGrass => include_bytes!("dxil/drawn.cs_grass.dxil"),
            Self::GrassVsBlade => include_bytes!("dxil/grass.vs_blade.dxil"),
            Self::GrassFsBlade => include_bytes!("dxil/grass.fs_blade.dxil"),
            Self::GrassVsBladeMotion => include_bytes!("dxil/grass.vs_blade_motion.dxil"),
            Self::GrassFsBladeMotion => include_bytes!("dxil/grass.fs_blade_motion.dxil"),
            Self::DrawnCsScatter => include_bytes!("dxil/drawn.cs_scatter.dxil"),
            Self::GrassVsScatter => include_bytes!("dxil/grass.vs_scatter.dxil"),
            Self::GrassFsScatter => include_bytes!("dxil/grass.fs_scatter.dxil"),
            Self::WaterFsMain => include_bytes!("dxil/water.fs_main.dxil"),
            Self::WaterFsTraced => include_bytes!("dxil/water.fs_traced.dxil"),
            Self::LiftCsBases => include_bytes!("dxil/lift.cs_bases.dxil"),
            Self::ImpostorVsBake => include_bytes!("dxil/impostor.vs_bake.dxil"),
            Self::ImpostorFsBake => include_bytes!("dxil/impostor.fs_bake.dxil"),
            Self::ImpostorVsDraw => include_bytes!("dxil/impostor.vs_draw.dxil"),
            Self::ImpostorFsDraw => include_bytes!("dxil/impostor.fs_draw.dxil"),
            Self::ForestCsCull => include_bytes!("dxil/forest.cs_cull.dxil"),
            Self::ForestVsFar => include_bytes!("dxil/forest.vs_far.dxil"),
            Self::ForestFsFar => include_bytes!("dxil/forest.fs_far.dxil"),
            Self::RainFsSplash => include_bytes!("dxil/rain.fs_splash.dxil"),
            Self::RainVsFall => include_bytes!("dxil/rain.vs_fall.dxil"),
            Self::RainFsFall => include_bytes!("dxil/rain.fs_fall.dxil"),
            Self::RtCsInstances => include_bytes!("dxil/rt.cs_instances.dxil"),
            Self::RtCheckCsCheck => include_bytes!("dxil/rt_check.cs_check.dxil"),
            Self::GiCsUpdate => include_bytes!("dxil/gi.cs_update.dxil"),
            Self::GiFsProbes => include_bytes!("dxil/gi.fs_probes.dxil"),
            Self::GiFsScreen => include_bytes!("dxil/gi.fs_screen.dxil"),
            Self::GiFsBlur => include_bytes!("dxil/gi.fs_blur.dxil"),
            Self::GiTraceCsTrace => include_bytes!("dxil/gi_trace.cs_trace.dxil"),
            Self::RtShadowFsMain => include_bytes!("dxil/rt_shadow.fs_main.dxil"),
            Self::RtAoFsRays => include_bytes!("dxil/rt_ao.fs_rays.dxil"),
            Self::RtAoFsFilter => include_bytes!("dxil/rt_ao.fs_filter.dxil"),
            Self::UpscaleInputsCsMain => include_bytes!("dxil/upscale_inputs.cs_main.dxil"),
            Self::Fsr3PassesCsClearReconstructed => include_bytes!("dxil/fsr3_passes.cs_clear_reconstructed.dxil"),
            Self::Fsr3PassesCsPrepareInputs => include_bytes!("dxil/fsr3_passes.cs_prepare_inputs.dxil"),
            Self::Fsr3PassesCsFarthestHalf => include_bytes!("dxil/fsr3_passes.cs_farthest_half.dxil"),
            Self::Fsr3PassesCsShadingLevelZero => include_bytes!("dxil/fsr3_passes.cs_shading_level_zero.dxil"),
            Self::Fsr3PassesCsShadingLevel => include_bytes!("dxil/fsr3_passes.cs_shading_level.dxil"),
            Self::Fsr3PassesCsShadingChange => include_bytes!("dxil/fsr3_passes.cs_shading_change.dxil"),
            Self::Fsr3PassesCsReactivity => include_bytes!("dxil/fsr3_passes.cs_reactivity.dxil"),
            Self::Fsr3PassesCsLumaInstability => include_bytes!("dxil/fsr3_passes.cs_luma_instability.dxil"),
            Self::Fsr3PassesCsAccumulate => include_bytes!("dxil/fsr3_passes.cs_accumulate.dxil"),
            Self::Fsr3PassesCsRcas => include_bytes!("dxil/fsr3_passes.cs_rcas.dxil"),
            Self::Fsr3PassesFsShow => include_bytes!("dxil/fsr3_passes.fs_show.dxil"),
            Self::Fsr3PassesCsReactiveInput => include_bytes!("dxil/fsr3_passes.cs_reactive_input.dxil"),
            Self::Fsr3PassesCsExposure => include_bytes!("dxil/fsr3_passes.cs_exposure.dxil"),
            Self::Fsr3PassesFsInputsAsPictures => include_bytes!("dxil/fsr3_passes.fs_inputs_as_pictures.dxil"),
        }
    }
}
