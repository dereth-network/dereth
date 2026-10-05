//! Region records and terrain style selection.

use super::*;

/// Hand back every texture a surface cache holds, one release per slot it owns, and empty it.
/// Returns how many were released.
pub(super) fn release_bake_cache(cache: &mut BakeCache, gpu: &mut Gpu) -> u32 {
    let mut n = 0;
    // ORDER-OK: every slot is released exactly once and no release can affect another.
    for slot in cache.by_slot.keys() {
        gpu.release_texture(TextureSlot(*slot));
        n += 1;
    }
    cache.surfaces.clear();
    cache.links.clear();
    cache.by_slot.clear();
    cache.key_clip_map.clear();
    cache.textures_uploaded = 0;
    n
}

/// Read and decode one region record from `files`.
pub(super) fn read_region(
    files: &RetailDatStore,
    id: DataId,
) -> Result<dereth_assets::Region, WorldError> {
    let bytes = files
        .read_typed(DbType::Region, id)
        .map_err(|e| WorldError::Region(id, e.to_string()))?;
    match dereth_assets::decode_any_in(files.era_of(id), DbType::Region, id, &bytes)
        .map_err(|e| WorldError::Region(id, e.to_string()))?
    {
        dereth_assets::DecodedAsset::Region(r) => Ok(r),
        other => Err(WorldError::Region(id, format!("decoded as {other:?}"))),
    }
}

/// The world's own region: the world files' hardware region where they have one, as the
/// clients of the world's time loaded it when they drew with 3D hardware, and otherwise the
/// region at `0x13000000`. Its ground and sky are what [`SceneConfig::render`]'s `ground` and
/// `sky` of `None` draw, and everything else the region decides (scenery, sound, the calendar)
/// is read from it whatever they choose.
///
/// # Errors
/// [`WorldError::Region`] when the record will not read or decode.
pub fn world_region(store: &RetailDatStore) -> Result<dereth_assets::Region, WorldError> {
    if store.portal().contains(HARDWARE_REGION) {
        read_region(store, HARDWARE_REGION)
    } else {
        load_region(store)
    }
}

impl From<WorldError> for StyleError {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}

/// The region `style` names, from the files of its era: an older world's own files or the
/// presentation portal beside a later world for the two older styles, a later world's own
/// files or the later files beside an older world for the modern one.
///
/// # Errors
/// [`StyleError::Missing`] when those files are not present or do not hold the region;
/// [`StyleError::World`] when it will not decode.
pub fn style_region(store: &RetailDatStore, style: RegionStyle) -> Result<StyleSource, StyleError> {
    let needs = style.required_files();
    let (files, own) = match needs {
        RequiredFiles::Legacy => (
            store.legacy_files(),
            store.era() == dereth_dat::ContainerEra::PreTod,
        ),
        RequiredFiles::Modern => (
            store.modern_files(),
            store.era() == dereth_dat::ContainerEra::Tod,
        ),
    };
    let files = files.ok_or(StyleError::Missing(needs))?;
    let id = match style {
        RegionStyle::LegacyHardware => HARDWARE_REGION,
        RegionStyle::LegacySoftware | RegionStyle::Modern => DERETH_REGION,
    };
    if !files.portal().contains(id) {
        return Err(StyleError::Missing(needs));
    }
    let region = read_region(&files, id)?;
    Ok(StyleSource {
        region,
        files: (!own).then_some(files),
    })
}

/// The ground `style` draws over the world's `region` (`None`: the world's own). The style's
/// land surface must be its technique: palette shifting for the older software region, texture
/// merging for the other two.
///
/// The cells' terrain words index the same terrain types in every region from the first to
/// the last (the same names and map colours at the same indices, later regions adding to the
/// end), and the road bits mean the same, so any land surface reads any world's cells as they
/// are. A type the land surface has no picture for is filled from its neighbours
/// ([`dereth_world_render::land::fill`]).
///
/// # Errors
/// As [`style_region`].
pub(super) fn ground_for(
    store: &RetailDatStore,
    region: &dereth_assets::Region,
    style: Option<RegionStyle>,
) -> Result<GroundChoice, StyleError> {
    let Some(style) = style else {
        return Ok(GroundChoice {
            ground: None,
            files: None,
            drawn: dereth_terrain::land::fill::drawn_terrain_types(
                &region.land_surf,
                region.terrain_types.len(),
            ),
        });
    };
    let source = style_region(store, style)?;
    let surf = source.region.land_surf;
    let technique = match style {
        RegionStyle::LegacySoftware => surf.pal_shift.is_some(),
        RegionStyle::LegacyHardware | RegionStyle::Modern => surf.tex_merge.is_some(),
    };
    if !technique {
        return Err(StyleError::Missing(style.required_files()));
    }
    // The types the style's own region names, which its land surface draws with pictures of
    // their own.
    let drawn =
        dereth_terrain::land::fill::drawn_terrain_types(&surf, source.region.terrain_types.len());
    if source.files.is_none() && surf == region.land_surf {
        return Ok(GroundChoice {
            ground: None,
            files: None,
            drawn,
        });
    }
    let mut ground = region.clone();
    ground.land_surf = surf;
    Ok(GroundChoice {
        ground: Some(ground),
        files: source.files,
        drawn,
    })
}

/// The sky `style` draws: the style's region, whose sky, light and fog are read, and the
/// files to read its objects from (`None`: the world's). Both `None` for the world's own sky.
///
/// # Errors
/// As [`style_region`].
pub(super) fn sky_for(
    store: &RetailDatStore,
    region: &dereth_assets::Region,
    style: Option<RegionStyle>,
) -> Result<(Option<dereth_assets::Region>, Option<RetailDatStore>), StyleError> {
    let Some(style) = style else {
        return Ok((None, None));
    };
    let source = style_region(store, style)?;
    if source.region.sky_info.is_none() {
        return Err(StyleError::Missing(style.required_files()));
    }
    if source.files.is_none() && source.region.sky_info == region.sky_info {
        return Ok((None, None));
    }
    Ok((Some(source.region), source.files))
}

/// The land surface a region draws its ground with: texture merging, or (the software region
/// of an older dat set) palette shifting, which composes each cell on the CPU and has no splat
/// form. The record decides; the dat set's era does not.
pub(super) fn land_surface(
    region: &dereth_assets::Region,
) -> Result<
    (
        dereth_assets::region::TexMerge,
        Option<dereth_assets::region::PalShift>,
    ),
    WorldError,
> {
    let surf = &region.land_surf;
    let pal_shift = surf.pal_shift.clone();
    let tex_merge = match (&surf.tex_merge, &pal_shift) {
        (Some(tm), _) => tm.clone(),
        (None, Some(_)) => dereth_assets::region::TexMerge {
            base_tex_size: 0,
            corner_terrain_maps: Vec::new(),
            side_terrain_maps: Vec::new(),
            road_maps: Vec::new(),
            terrain_desc: Vec::new(),
        },
        (None, None) => return Err(WorldError::MissingTerrainTexture),
    };
    Ok((tex_merge, pal_shift))
}
