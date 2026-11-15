// Written by `cargo xtask shaders` from `shaders.toml`. Regenerate it; never edit it.

/// The entry points `shaders.toml` names, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entry {
    /// `vs_terrain` in `terrain.slang`, the vertex stage.
    TerrainVsTerrain,
    /// `fs_terrain_base` in `terrain.slang`, the fragment stage.
    TerrainFsTerrainBase,
    /// `fs_terrain_mask` in `terrain.slang`, the fragment stage.
    TerrainFsTerrainMask,
    /// `fs_terrain_far` in `terrain.slang`, the fragment stage.
    TerrainFsTerrainFar,
    /// `vs_veil` in `terrain.slang`, the vertex stage.
    TerrainVsVeil,
    /// `fs_veil` in `terrain.slang`, the fragment stage.
    TerrainFsVeil,
    /// `fs_sky_in` in `terrain.slang`, the fragment stage.
    TerrainFsSkyIn,
    /// `vs_mesh` in `mesh.slang`, the vertex stage.
    MeshVsMesh,
    /// `fs_mesh` in `mesh.slang`, the fragment stage.
    MeshFsMesh,
    /// `fs_ripple` in `mesh.slang`, the fragment stage.
    MeshFsRipple,
    /// `fs_water` in `mesh.slang`, the fragment stage.
    MeshFsWater,
    /// `fs_mesh_depth` in `mesh.slang`, the fragment stage.
    MeshFsMeshDepth,
    /// `fs_terrain_depth` in `terrain.slang`, the fragment stage.
    TerrainFsTerrainDepth,
    /// `vs_mesh_caster` in `mesh.slang`, the vertex stage.
    MeshVsMeshCaster,
    /// `fs_mesh_caster` in `mesh.slang`, the fragment stage.
    MeshFsMeshCaster,
    /// `fs_lamps` in `lamps.slang`, the fragment stage.
    LampsFsLamps,
}

impl Entry {
    /// Every entry point, in `shaders.toml`'s order.
    pub const ALL: [Self; 16] = [Self::TerrainVsTerrain, Self::TerrainFsTerrainBase, Self::TerrainFsTerrainMask, Self::TerrainFsTerrainFar, Self::TerrainVsVeil, Self::TerrainFsVeil, Self::TerrainFsSkyIn, Self::MeshVsMesh, Self::MeshFsMesh, Self::MeshFsRipple, Self::MeshFsWater, Self::MeshFsMeshDepth, Self::TerrainFsTerrainDepth, Self::MeshVsMeshCaster, Self::MeshFsMeshCaster, Self::LampsFsLamps];

    /// The source file that defines it.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::TerrainVsTerrain => "terrain.slang",
            Self::TerrainFsTerrainBase => "terrain.slang",
            Self::TerrainFsTerrainMask => "terrain.slang",
            Self::TerrainFsTerrainFar => "terrain.slang",
            Self::TerrainVsVeil => "terrain.slang",
            Self::TerrainFsVeil => "terrain.slang",
            Self::TerrainFsSkyIn => "terrain.slang",
            Self::MeshVsMesh => "mesh.slang",
            Self::MeshFsMesh => "mesh.slang",
            Self::MeshFsRipple => "mesh.slang",
            Self::MeshFsWater => "mesh.slang",
            Self::MeshFsMeshDepth => "mesh.slang",
            Self::TerrainFsTerrainDepth => "terrain.slang",
            Self::MeshVsMeshCaster => "mesh.slang",
            Self::MeshFsMeshCaster => "mesh.slang",
            Self::LampsFsLamps => "lamps.slang",
        }
    }

    /// Its function, which is also its name in every blob.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TerrainVsTerrain => "vs_terrain",
            Self::TerrainFsTerrainBase => "fs_terrain_base",
            Self::TerrainFsTerrainMask => "fs_terrain_mask",
            Self::TerrainFsTerrainFar => "fs_terrain_far",
            Self::TerrainVsVeil => "vs_veil",
            Self::TerrainFsVeil => "fs_veil",
            Self::TerrainFsSkyIn => "fs_sky_in",
            Self::MeshVsMesh => "vs_mesh",
            Self::MeshFsMesh => "fs_mesh",
            Self::MeshFsRipple => "fs_ripple",
            Self::MeshFsWater => "fs_water",
            Self::MeshFsMeshDepth => "fs_mesh_depth",
            Self::TerrainFsTerrainDepth => "fs_terrain_depth",
            Self::MeshVsMeshCaster => "vs_mesh_caster",
            Self::MeshFsMeshCaster => "fs_mesh_caster",
            Self::LampsFsLamps => "fs_lamps",
        }
    }

    /// The stage it runs in.
    #[must_use]
    pub const fn stage(self) -> Stage {
        match self {
            Self::TerrainVsTerrain => Stage::Vertex,
            Self::TerrainFsTerrainBase => Stage::Fragment,
            Self::TerrainFsTerrainMask => Stage::Fragment,
            Self::TerrainFsTerrainFar => Stage::Fragment,
            Self::TerrainVsVeil => Stage::Vertex,
            Self::TerrainFsVeil => Stage::Fragment,
            Self::TerrainFsSkyIn => Stage::Fragment,
            Self::MeshVsMesh => Stage::Vertex,
            Self::MeshFsMesh => Stage::Fragment,
            Self::MeshFsRipple => Stage::Fragment,
            Self::MeshFsWater => Stage::Fragment,
            Self::MeshFsMeshDepth => Stage::Fragment,
            Self::TerrainFsTerrainDepth => Stage::Fragment,
            Self::MeshVsMeshCaster => Stage::Vertex,
            Self::MeshFsMeshCaster => Stage::Fragment,
            Self::LampsFsLamps => Stage::Fragment,
        }
    }

    /// Its SPIR-V blob, as the compiler wrote it.
    #[must_use]
    pub const fn spirv_bytes(self) -> &'static [u8] {
        match self {
            Self::TerrainVsTerrain => include_bytes!("spirv/terrain.vs_terrain.spv"),
            Self::TerrainFsTerrainBase => include_bytes!("spirv/terrain.fs_terrain_base.spv"),
            Self::TerrainFsTerrainMask => include_bytes!("spirv/terrain.fs_terrain_mask.spv"),
            Self::TerrainFsTerrainFar => include_bytes!("spirv/terrain.fs_terrain_far.spv"),
            Self::TerrainVsVeil => include_bytes!("spirv/terrain.vs_veil.spv"),
            Self::TerrainFsVeil => include_bytes!("spirv/terrain.fs_veil.spv"),
            Self::TerrainFsSkyIn => include_bytes!("spirv/terrain.fs_sky_in.spv"),
            Self::MeshVsMesh => include_bytes!("spirv/mesh.vs_mesh.spv"),
            Self::MeshFsMesh => include_bytes!("spirv/mesh.fs_mesh.spv"),
            Self::MeshFsRipple => include_bytes!("spirv/mesh.fs_ripple.spv"),
            Self::MeshFsWater => include_bytes!("spirv/mesh.fs_water.spv"),
            Self::MeshFsMeshDepth => include_bytes!("spirv/mesh.fs_mesh_depth.spv"),
            Self::TerrainFsTerrainDepth => include_bytes!("spirv/terrain.fs_terrain_depth.spv"),
            Self::MeshVsMeshCaster => include_bytes!("spirv/mesh.vs_mesh_caster.spv"),
            Self::MeshFsMeshCaster => include_bytes!("spirv/mesh.fs_mesh_caster.spv"),
            Self::LampsFsLamps => include_bytes!("spirv/lamps.fs_lamps.spv"),
        }
    }

    /// Its DXIL blob, as the compiler wrote it.
    #[must_use]
    pub const fn dxil_bytes(self) -> &'static [u8] {
        match self {
            Self::TerrainVsTerrain => include_bytes!("dxil/terrain.vs_terrain.dxil"),
            Self::TerrainFsTerrainBase => include_bytes!("dxil/terrain.fs_terrain_base.dxil"),
            Self::TerrainFsTerrainMask => include_bytes!("dxil/terrain.fs_terrain_mask.dxil"),
            Self::TerrainFsTerrainFar => include_bytes!("dxil/terrain.fs_terrain_far.dxil"),
            Self::TerrainVsVeil => include_bytes!("dxil/terrain.vs_veil.dxil"),
            Self::TerrainFsVeil => include_bytes!("dxil/terrain.fs_veil.dxil"),
            Self::TerrainFsSkyIn => include_bytes!("dxil/terrain.fs_sky_in.dxil"),
            Self::MeshVsMesh => include_bytes!("dxil/mesh.vs_mesh.dxil"),
            Self::MeshFsMesh => include_bytes!("dxil/mesh.fs_mesh.dxil"),
            Self::MeshFsRipple => include_bytes!("dxil/mesh.fs_ripple.dxil"),
            Self::MeshFsWater => include_bytes!("dxil/mesh.fs_water.dxil"),
            Self::MeshFsMeshDepth => include_bytes!("dxil/mesh.fs_mesh_depth.dxil"),
            Self::TerrainFsTerrainDepth => include_bytes!("dxil/terrain.fs_terrain_depth.dxil"),
            Self::MeshVsMeshCaster => include_bytes!("dxil/mesh.vs_mesh_caster.dxil"),
            Self::MeshFsMeshCaster => include_bytes!("dxil/mesh.fs_mesh_caster.dxil"),
            Self::LampsFsLamps => include_bytes!("dxil/lamps.fs_lamps.dxil"),
        }
    }
}
