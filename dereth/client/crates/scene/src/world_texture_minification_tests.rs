// real DAT pixels through the production world surface resolver and draw consumer.
// Geometry/camera are deliberately synthetic and stationary. This is not a matched retail
// screenshot, and GPU autogen rounding is not claimed identical across drivers.

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store().expect("required pristine retail DATs (DERETH_TEST_DAT_DIR)")
}

fn warp() -> Gpu {
    Gpu::new(
        None,
        &dereth_render::device::DeviceConfig {
            width: 16,
            height: 16,
            ..Default::default()
        },
    )
    .expect("required headless WARP device")
}

fn uncompressed_surface(store: &RetailDatStore) -> DataId {
    let textures = TextureStore::new(store);
    store
        .ids_of(DbType::Surface)
        .into_iter()
        .find(|id| {
            let Some(s) = read_surface(store, *id) else { return false };
            s.surface_type == dereth_render::surface::surface_type::BASE1_IMAGE
                && textures.texture_data(*id).is_ok_and(|t| {
                    t.format == dereth_primitives::TextureFormat::Bgra8
                        && t.width >= 128
                        && t.width == t.height
                        && t.width.is_power_of_two()
                        && t.levels.len() == 1
                })
        })
        .expect("retail opaque, uncompressed world texture at least 128x128")
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_dat_world_upload_has_runtime_sublevels() {
    let store = store();
    let textures = TextureStore::new(&store);
    let id = uncompressed_surface(&store);
    let data = textures.texture_data(id).expect("real source pixels");
    let mut gpu = warp();
    let mut cache = BakeCache::default();
    let key = GroupKey {
        surface: Some(id),
        two_sided: true,
        tiled: false,
        appearance: cache.appearance_for(&store, &textures, Some(id), None),
    };
    let resolved = cache.resolve(&store, &textures, &mut gpu, key).expect("production resolve");
    let slot = resolved.texture.expect("textured production surface");
    eprintln!(
        "DAT {id:?}, {}x{}, {:?}, runtime {:?}",
        data.width,
        data.height,
        data.format,
        gpu.texture_mip_levels(slot)
    );
    assert_eq!(data.levels.len(), 1, "system-memory source remains one level");
    assert_eq!(
        gpu.texture_mip_levels(slot),
        Some(u16::try_from(data.width.max(data.height).ilog2() + 1).expect("a mip count")),
        "an eligible one-level uncompressed decoded texture gets runtime autogen sublevels"
    );
    assert!(gpu.imgtex_autogen_supported(), "WARP must support the tested resource format");
    assert_eq!(
        gpu.capture_texture_level(slot, 0).expect("level-zero GPU copy").bgra,
        data.levels[0],
        "autogen must never replace level zero"
    );
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn compressed_mips_real_dat_world_upload_has_system_sublevels() {
    compressed_world_upload(dereth_primitives::TextureFormat::Bc1);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn compressed_bc2_mips_real_dat_world_upload_has_system_sublevels() {
    compressed_world_upload(dereth_primitives::TextureFormat::Bc2);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn compressed_bc3_mips_real_dat_world_upload_has_system_sublevels() {
    compressed_world_upload(dereth_primitives::TextureFormat::Bc3);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn compressed_bc3_mips_dat_source_station() {
    let store = store();
    let textures = TextureStore::new(&store);
    let samples: Vec<_> = store
        .ids_of(DbType::Surface)
        .into_iter()
        .filter_map(|id| {
            let surface = read_surface(&store, id)?;
            let (image_id, image, _) = textures.resolve(id).ok()?;
            (image.format == dereth_render::PixelFormatId::Dxt5.raw()
                && surface.surface_type & 6 != 0
                && image.width >= 128
                && image.height >= 128)
                .then_some((id, image_id, surface.surface_type, image.width, image.height))
        })
        .take(5)
        .collect();
    eprintln!("first installed DXT5 world stations: {samples:?}");
    assert_eq!(samples[0], (DataId(0x0800_0000), DataId(0x0600_3789), 4, 512, 512));
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn premultiplied_mips_dat_census() {
    let store = store();
    assert!(store.grant_highres().expect("client_highres.dat opens"), "client_highres.dat is present");
    let textures = TextureStore::new(&store);
    let ids = store.ids_of(DbType::RenderSurface);
    let mut formats = std::collections::BTreeMap::new();
    let mut failed = Vec::new();
    let mut premultiplied = Vec::new();
    for id in &ids {
        match textures.resolve(*id) {
            Ok((_, image, _)) => {
                *formats.entry(image.format).or_insert(0usize) += 1;
                if matches!(image.format, 0x3254_5844 | 0x3454_5844) {
                    premultiplied.push((*id, image.width, image.height));
                }
            }
            Err(_) => failed.push(*id),
        }
    }
    let surfaces = store.ids_of(DbType::Surface);
    let mut unresolved_links = 0usize;
    let mut unresolved_kinds = std::collections::BTreeMap::new();
    let mut source_links = Vec::new();
    for id in &surfaces {
        match textures.resolve(*id) {
            Ok((image_id, image, _)) if matches!(image.format, 0x3254_5844 | 0x3454_5844) => {
                source_links.push((*id, image_id))
            }
            Err(_) => {
                unresolved_links += 1;
                let kind = match read_surface(&store, *id) {
                    Some(s) if s.surface_type & 6 == 0 => "non-textured surface",
                    Some(_) => "textured surface unresolved",
                    None => "surface header unresolved",
                };
                *unresolved_kinds.entry(kind).or_insert(0usize) += 1;
            }
            _ => {}
        }
    }
    eprintln!(
        "installed RenderSurface denominator{}, formats{formats:x?}, unresolved{failed:?}; DXT2/4{premultiplied:?}; surface record denominator{}, unresolved{unresolved_links} {unresolved_kinds:?}, DXT2/4 links{source_links:?}",
        ids.len(),
        surfaces.len()
    );
    assert_eq!(formats.values().sum::<usize>() + failed.len(), ids.len());
    assert_eq!(ids.len(), 22_978, "installed source census identity");
    assert_eq!(surfaces.len(), 6_152);
    assert_eq!(unresolved_links, 153);
    assert_eq!(
        unresolved_kinds,
        [("non-textured surface", 153)].into_iter().collect(),
        "a broken textured link must not satisfy the absence evidence"
    );
    assert!(failed.is_empty(), "absence claim requires all RenderSurface headers resolved");
    assert!(
        premultiplied.is_empty(),
        "new DXT2/4 DAT sources require a real-DAT acceptance station"
    );
    assert!(source_links.is_empty());
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn compressed_bc2_mips_dat_source_ownership() {
    let store = store();
    let textures = TextureStore::new(&store);
    let mut counts = std::collections::BTreeMap::new();
    let mut samples = Vec::new();
    for id in store.ids_of(DbType::Surface) {
        let Some(surface) = read_surface(&store, id) else { continue };
        let Ok((image_id, image, _)) = textures.resolve(id) else { continue };
        if image.format == dereth_render::PixelFormatId::Dxt3.raw() {
            *counts.entry((surface.surface_type, image.width, image.height)).or_insert(0usize) += 1;
            if samples.len() < 8 {
                samples.push((id, image_id, surface.surface_type));
            }
        }
    }
    eprintln!("actual DXT3 surface record links {counts:?}; first {samples:?}");
    assert_eq!(counts, [((4, 256, 512), 1), ((4, 512, 512), 4)].into_iter().collect());
    assert_eq!(samples[0], (DataId(0x0800_1521), DataId(0x0600_6992), 4));
    assert_eq!(
        textures.resolve(DataId(0x0600_6992)).unwrap().1.format,
        dereth_render::PixelFormatId::Dxt3.raw()
    );
}

fn compressed_world_upload(format: dereth_primitives::TextureFormat) {
    let store = store();
    let textures = TextureStore::new(&store);
    let (id, data) = store
        .ids_of(DbType::Surface)
        .into_iter()
        .find_map(|id| {
            let surface = read_surface(&store, id)?;
            if surface.surface_type & 6 == 0 {
                return None;
            }
            let data = textures.texture_data(id).ok()?;
            (data.format == format
                && data.width >= 128
                && data.height >= 128
                && data.levels.len() == 1)
                .then_some((id, data))
        })
        .expect("installed textured compressed world surface");
    assert_eq!(
        id,
        match format {
            dereth_primitives::TextureFormat::Bc1 => DataId(0x0800_0002),
            dereth_primitives::TextureFormat::Bc2 => DataId(0x0800_1521),
            dereth_primitives::TextureFormat::Bc3 => DataId(0x0800_0000),
            _ => unreachable!("this station only covers non-premultiplied system codecs"),
        },
        "the shared source search must retain each explicit DAT fixture"
    );
    let mut gpu = warp();
    let mut cache = BakeCache::default();
    let mut meshes = build_meshes(&store, &mut cache, &textures, &mut gpu, &quad(id), None, None)
        .expect("normal production world bake");
    let slot = meshes[0].texture.expect("normal production decoded texture upload");
    eprintln!(
        "compressed DAT {id:?}: {}x{} {:?}, actual resource {:?}",
        data.width,
        data.height,
        data.format,
        gpu.texture_mip_levels(slot)
    );
    assert_eq!(
        gpu.texture_mip_levels(slot),
        Some(4),
        ": compressed system chain includes four levels"
    );
    let expected = dereth_render::mip::compressed_system_chain(&data).unwrap().unwrap();
    assert_eq!(expected.levels[0], data.levels[0], "no level-zero recompression");
    for (i, bytes) in expected.levels.iter().enumerate() {
        let actual =
            gpu.capture_texture_level_data(slot, u16::try_from(i).expect("a mip level")).expect("actual resident BC block bytes");
        assert_eq!(actual.format, data.format);
        assert_eq!((actual.width, actual.height), (data.width >> i, data.height >> i));
        assert_eq!(
            actual.levels[0], *bytes,
            "actual subresource{i} exactly matches the system chain"
        );
    }
    let sampled = draw(&mut gpu, &meshes[0]);
    assert_eq!(sampled, draw(&mut gpu, &meshes[0]), "stationary BC world draw");
    let tail = dereth_primitives::TextureData {
        width: data.width >> 3,
        height: data.height >> 3,
        format: data.format,
        levels: vec![expected.levels[3].clone()],
    };
    let tail_slot = gpu.upload_texture(&tail).unwrap();
    meshes[0].texture = Some(tail_slot);
    assert_eq!(sampled, draw(&mut gpu, &meshes[0]), "real world consumer samples capped final mip");
    let base = gpu.upload_texture(&data).unwrap();
    meshes[0].texture = Some(base);
    let old = draw(&mut gpu, &meshes[0]);
    let changed = rgb(&sampled).iter().zip(rgb(&old)).filter(|(a, b)| **a != *b).count();
    assert!(changed > 0, "real DAT must distinguish generated BC mips from old one-level upload");
    eprintln!("{format:?} world: {changed}/256 pixels differ from old one-level resource");
    gpu.release_texture(tail_slot);
    gpu.release_texture(base);
    assert!(cache.release_group_texture(&mut gpu, slot));
    assert!(gpu.capture_texture_level_data(slot, 0).is_err(), "last owner releases all BC levels");
}

fn quad(surface: DataId) -> Vec<dereth_client_runtime::models::SurfaceGroup> {
    // Z-up geometry becomes a clip-space full-screen quad via the normal world-matrix swizzle.
    vec![dereth_client_runtime::models::SurfaceGroup {
        surface: Some(surface),
        two_sided: true,
        tiled: false,
        normals: Vec::new(),
        vertices: [
            (-1.0, 1.0, 0.0, 0.0),
            (-1.0, -1.0, 0.0, 1.0),
            (1.0, -1.0, 1.0, 1.0),
            (1.0, -1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0, 0.0),
            (-1.0, 1.0, 0.0, 0.0),
        ]
        .into_iter()
        .map(|(x, z, u, v)| (Vec3::new(x, 0.5, z), u, v))
        .collect(),
    }]
}

fn draw(gpu: &mut Gpu, mesh: &PartMesh) -> Vec<u8> {
    let frame = PerFrameConstants {
        view_proj: hlsl_matrix(glam::Mat4::IDENTITY),
        view: hlsl_matrix(glam::Mat4::IDENTITY),
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..Default::default()
    };
    let part = dereth_animation::parts::PhysicsPart::new(DataId(0));
    gpu.begin_frame().expect("begin minified world draw");
    submit_part_mesh(gpu, &frame, &part, &Frame::default(), mesh, true, None, false)
        .expect("actual production part submission");
    gpu.end_frame().expect("end minified world draw");
    gpu.capture().expect("actual sampled frame").bgra
}

fn rgb(bytes: &[u8]) -> Vec<[u8; 3]> {
    bytes.as_chunks::<4>().0.iter().map(|c| [c[0], c[1], c[2]]).collect()
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_dat_world_consumer_samples_generated_mip_not_level_zero() {
    let store = store();
    let textures = TextureStore::new(&store);
    let id = uncompressed_surface(&store);
    let data = textures.texture_data(id).expect("real DAT pixels");
    let mut gpu = warp();
    let mut cache = BakeCache::default();
    let mut meshes = build_meshes(&store, &mut cache, &textures, &mut gpu, &quad(id), None, None)
        .expect("normal world bake");
    assert_eq!(meshes.len(), 1);
    let mesh = &mut meshes[0];
    let world_slot = mesh.texture.expect("resolved texture slot");
    let mip_level = u16::try_from((data.width / 16).ilog2()).expect("a mip level");
    let mip =
        gpu.capture_texture_level(world_slot, mip_level).expect("resident minified subresource");
    assert_eq!((mip.width, mip.height), (16, 16));
    let sampled = draw(&mut gpu, mesh);
    assert_eq!(
        rgb(&sampled),
        rgb(&mip.bgra),
        "actual submit_part_mesh implicit LOD must sample the generated 16x16 mip"
    );
    assert_eq!(sampled, draw(&mut gpu, mesh), "stationary draw has zero noise floor");

    // Deliberately use the supported provided-level upload as a negative control, with exactly
    // the same source bytes, shader, mesh, sampler and camera. It models the pre-fix resource.
    let raw_slot = gpu.upload_texture(&data).expect("provided-level control");
    assert_eq!(gpu.texture_mip_levels(raw_slot), Some(1));
    mesh.texture = Some(raw_slot);
    let raw = draw(&mut gpu, mesh);
    let changed = rgb(&sampled).iter().zip(rgb(&raw)).filter(|(a, b)| **a != *b).count();
    assert!(changed > 0, "real DAT station must distinguish minification from level-zero sampling");
    eprintln!(
        "{id:?}: {}x{} ->16x16, mip{mip_level}, {changed}/256 RGB pixels differ from no-mip control",
        data.width, data.height
    );
    gpu.release_texture(raw_slot);
    assert!(cache.release_group_texture(&mut gpu, world_slot));
    assert_eq!(gpu.texture_mip_levels(world_slot), None, "last owner releases the whole resource");
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn texture_samplers_real_dat_world_draw_observes_preference_without_reupload() {
    let store = store();
    let textures = TextureStore::new(&store);
    let id = uncompressed_surface(&store);
    let mut gpu = warp();
    let mut cache = BakeCache::default();
    let mut groups = quad(id);
    for (_, u, v) in &mut groups[0].vertices {
        *u *= 1.5;
        *v *= 1.5;
    }
    let meshes =
        build_meshes(&store, &mut cache, &textures, &mut gpu, &groups, None, None).unwrap();
    let mesh = &meshes[0];
    let slot = mesh.texture.unwrap();
    let levels = gpu.texture_mip_levels(slot);
    let stats = gpu.texture_table_stats();
    let mut samples = Vec::new();
    for preference in [0, 1, 2, 3] {
        gpu.set_texture_filtering(preference);
        let pixels = draw(&mut gpu, mesh);
        assert_eq!(pixels, draw(&mut gpu, mesh), "stationary preference{preference}");
        samples.push(pixels);
        assert_eq!(gpu.texture_mip_levels(slot), levels);
        assert_eq!(
            gpu.texture_table_stats(),
            stats,
            "sampling preference never reuploads/recaches"
        );
    }
    assert_ne!(samples[0], samples[1], "real DAT fractional minification: Bilinear vs Trilinear");
    assert_ne!(samples[1], samples[2], "real DAT world Sharp bias");
    eprintln!(
        "world sampler DAT {id:?}: Bilinear/Trilinear {} pixels, Trilinear/Sharp {} pixels",
        rgb(&samples[0]).iter().zip(rgb(&samples[1])).filter(|(a, b)| **a != *b).count(),
        rgb(&samples[1]).iter().zip(rgb(&samples[2])).filter(|(a, b)| **a != *b).count()
    );
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_dat_generated_terrain_uses_runtime_mips_without_changing_merge_or_ownership() {
    use dereth_world_render::land::merge::{MergeKey, fill_temp_tex_buffer};
    let store = store();
    let textures = TextureStore::new(&store);
    let region = load_region(&store).expect("retail region");
    let tm = region.land_surf.tex_merge.as_ref().expect("retail terrain compositor description");
    let requested = std::cell::Cell::new(0);
    let missing = std::cell::Cell::new(0);
    let src = |id| {
        requested.set(requested.get() + 1);
        let image = textures.bgra8(id).ok();
        if image.is_none() {
            missing.set(missing.get() + 1);
        }
        image
    };
    let key = MergeKey::new([1; 4], [0; 4], 1); // synthetic terrain-key station, real DAT inputs.
    let mut merge = TerrainMergeCache::new();
    let expected = fill_temp_tex_buffer(tm, key, merge.shift, &src);
    assert!(requested.get() > 0);
    assert_eq!(missing.get(), 0, "no missing source may turn this into a debug-colour test");
    let mut gpu = warp();
    let a = {
        let mut uploader = TextureUploader { gpu: &mut gpu, error: None };
        let a = merge.get_or_build(tm, key, &mut uploader, &src);
        assert!(uploader.error.is_none());
        assert_eq!(merge.get_or_build(tm, key, &mut uploader, &src), a);
        a
    };
    let slot = TextureSlot(a.0);
    assert_eq!(
        u32::from(gpu.texture_mip_levels(slot).expect("resident merge")),
        expected.width.max(expected.height).ilog2() + 1
    );
    let base = gpu.capture_texture_level(slot, 0).expect("terrain base");
    assert_eq!((base.width, base.height), (expected.width, expected.height));
    assert_eq!(
        base.bgra,
        expected.into_bytes(),
        "runtime generation leaves the compositor untouched"
    );
    assert_eq!(merge.surfaces_built, 1);
    assert_eq!(merge.remove_surface(key), None, "one cell still owns the generated texture");
    assert!(gpu.texture_mip_levels(slot).is_some());
    assert_eq!(merge.remove_surface(key), Some(a));
    gpu.release_texture(slot);
    assert_eq!(gpu.texture_mip_levels(slot), None);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_dat_palette_and_clipmap_pixels_survive_runtime_generation_and_cache_release() {
    let store = store();
    let textures = TextureStore::new(&store);
    let id = store
        .ids_of(DbType::Surface)
        .into_iter()
        .find(|id| {
            let Some(s) = read_surface(&store, *id) else { return false };
            if s.surface_type & dereth_render::surface::surface_type::BASE1_CLIPMAP == 0
                || !textures.is_palettised(*id).is_ok_and(|p| p)
            {
                return false;
            }
            let (Ok(clipped), Ok(plain)) = (
                textures.texture_data_clipped(*id, true),
                textures.texture_data_clipped(*id, false),
            ) else {
                return false;
            };
            clipped.width > 1 && clipped.height > 1 && clipped.levels[0] != plain.levels[0]
        })
        .expect("real indexed clip-map with observable transparent indices");
    let clipped = textures.texture_data_clipped(id, true).expect("clipped DAT pixels");
    let (palette_id, shifted, recipe) = store
        .ids_of(DbType::Palette)
        .into_iter()
        .find_map(|p| {
            let recipe = PaletteComposition {
                base: p,
                ranges: vec![],
                from_look: vec![],
            };
            let (palette, failures) = recipe.build(&textures)?;
            assert_eq!(failures, 0);
            let data = textures.texture_data_shifted(id, true, Some(&palette)).ok()?;
            (data.levels[0] != clipped.levels[0]).then_some((p, data, recipe))
        })
        .expect("real alternate palette gives observable shifted pixels");
    let mut gpu = warp();
    let mut cache = BakeCache::default();
    let key = GroupKey {
        surface: Some(id),
        two_sided: true,
        tiled: false,
        appearance: SurfaceAppearance { texture: None, palette: None },
    };
    let base =
        cache.resolve(&store, &textures, &mut gpu, key.clone()).expect("normal clip-map resolve");
    let a = base.texture.expect("base texture");
    let shifted_key =
        GroupKey { appearance: SurfaceAppearance { texture: None, palette: Some(recipe) }, ..key };
    let changed =
        cache.resolve(&store, &textures, &mut gpu, shifted_key.clone()).expect("shifted resolve");
    let b = changed.texture.expect("shifted texture");
    assert_ne!(a, b, "modified palette cannot replace the base cached texture");
    assert!(changed.texture_key.is_uncached());
    assert_eq!(gpu.capture_texture_level(a, 0).expect("base level").bgra, clipped.levels[0]);
    assert_eq!(gpu.capture_texture_level(b, 0).expect("shifted level").bgra, shifted.levels[0]);
    let n = u32::from(gpu.texture_mip_levels(b).expect("shifted resource"));
    assert_eq!(n, shifted.width.max(shifted.height).ilog2() + 1);
    let second =
        cache.resolve(&store, &textures, &mut gpu, shifted_key).expect("same recipe owner");
    assert_eq!(second.texture, Some(b));
    assert!(!cache.release_group_texture(&mut gpu, b));
    assert_eq!(gpu.capture_texture_level(b, 0).expect("surviving owner").bgra, shifted.levels[0]);
    assert!(cache.release_group_texture(&mut gpu, b));
    assert_eq!(gpu.texture_mip_levels(b), None);
    assert_eq!(gpu.capture_texture_level(a, 0).expect("independent base").bgra, clipped.levels[0]);
    assert!(cache.release_group_texture(&mut gpu, a));
    eprintln!(
        "clip-map {id:?}, alternate palette {palette_id:?}, {n} runtime levels; both level-zero copies unchanged"
    );
}
