// Exercise the production building-to-mesh path and its subset guard.
// Real DAT materials and placements, with explicitly synthetic stationary draw geometry/camera.

fn retail_store() -> RetailDatStore {
    dereth_dat::testing::open_store().expect("required installed retail DATs")
}

fn visible_translucent_solid(store: &RetailDatStore) -> DataId {
    let id = DataId(0x0800_006b);
    let source = read_surface(store, id).expect("required visible translucent-solid DAT material");
    assert_eq!(source.surface_type, 0x11);
    assert_eq!(source.translucency, 0.5);
    assert_eq!(source.color_value, Some(0xffff_ffff));
    id
}

fn gpu() -> Gpu {
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

fn frame() -> PerFrameConstants {
    PerFrameConstants {
        view_proj: hlsl_matrix(glam::Mat4::IDENTITY),
        view: hlsl_matrix(glam::Mat4::IDENTITY),
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..Default::default()
    }
}

fn quad(surface: DataId) -> dereth_client_runtime::models::SurfaceGroup {
    dereth_client_runtime::models::SurfaceGroup {
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
    }
}

// Inject only synthetic geometry into the normal decode memo. add_object_kind, append_mesh,
// finish, the actual DAT SetSurface/texture cache and all submission code remain production.
fn batches(
    store: &RetailDatStore,
    cache: &mut BakeCache,
    gpu: &mut Gpu,
    surface: DataId,
) -> Vec<StaticBatch> {
    let id = DataId(0x0100_0000);
    cache.parts.insert(id, vec![dereth_client_runtime::models::ModelPart { gfxobj: id, frame: Frame::default(), scale: Vec3::new(1.0, 1.0, 1.0) }]);
    cache.geometry.insert(id, vec![quad(surface)]);
    let mut baker = ObjectBaker::new(store, cache, false, false, false);
    baker.add_object(id, &Frame::default(), 1.0);
    baker.add_object_kind(id, &Frame::default(), 1.0, true);
    let (opaque, blended, placements) = baker.finish(gpu).unwrap();
    assert!(placements.is_empty());
    opaque.into_iter().chain(blended).collect()
}

fn draw(gpu: &mut Gpu, batches: &[&StaticBatch]) -> Vec<u8> {
    gpu.begin_frame().unwrap();
    for batch in batches {
        submit_static_batch(gpu, &frame(), &world_constants(&Frame::default()), batch, None, None)
            .unwrap();
    }
    gpu.end_frame().unwrap();
    gpu.capture().unwrap().bgra
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_building_and_ordinary_geometry_keep_separate_batches_with_identical_material_keys() {
    let store = retail_store();
    let translucent = visible_translucent_solid(&store);
    eprintln!("visible translucent solid {translucent:?}: {:?}", read_surface(&store, translucent));
    assert_eq!(
        read_surface(&store, DataId(0x0800_00dd)).unwrap().translucency,
        1.0,
        "academy portal material was unsuitable as a visible ordinary control"
    );
    let (bx, by) = block_xy(DEFAULT_LANDBLOCK);
    let lbi = read_lbi(&store, bx, by).unwrap();
    let building = lbi.buildings.first().expect("Holtburg building placement");
    let parts = resolve_parts(&store, building.id);
    assert!(!parts.is_empty());
    let mut cache = BakeCache::default();
    let mut baker = ObjectBaker::new(&store, &mut cache, false, false, false);
    baker.add_object_kind(building.id, &building.frame, 1.0, true);
    let building_count = baker.order.len();
    assert!(building_count > 0);
    baker.add_object(building.id, &building.frame, 1.0);
    assert_eq!(baker.order.len(), building_count * 2);
    let mut solids = 0;
    for key in &baker.order[..building_count] {
        assert!(key.building_pass);
        let ordinary = BatchKey { material: key.material.clone(), building_pass: false, instance: key.instance.map(|i| i + 1) };
        assert_eq!(baker.groups[key], baker.groups[&ordinary], "geometry/chunks are not filtered");
        if key
            .material
            .surface
            .and_then(|id| read_surface(&store, id))
            .is_some_and(|surface| surface.surface_type & 6 == 0)
        {
            solids += 1;
        }
    }
    eprintln!(
        "real Holtburg building {:08x}: {} parts, {} material groups, {} solid groups",
        building.id.0,
        parts.len(),
        building_count,
        solids
    );
    assert!(solids > 0, "real building must exercise nontextured guard");
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn actual_baker_draw_skips_building_solid_but_preserves_ordinary_textured_and_cache_lifetime() {
    let store = retail_store();
    let mut gpu = gpu();
    for surface in [DataId(0x0800_0139), visible_translucent_solid(&store), DataId(0x0800_0005)] {
        let source = read_surface(&store, surface).unwrap();
        let mut cache = BakeCache::default();
        let batches = batches(&store, &mut cache, &mut gpu, surface);
        assert_eq!(batches.len(), 2, "same-material ordinary/building are distinct draw owners");
        let ordinary = &batches[0];
        let shell = &batches[1];
        assert!(!ordinary.building_pass);
        assert!(shell.building_pass);
        assert_eq!(ordinary.surface_type, source.surface_type);
        assert_eq!(shell.surface_type, source.surface_type);
        assert_eq!(ordinary.vertices, shell.vertices);
        assert_eq!(ordinary.chunks, shell.chunks);
        assert_eq!(drawn_vertices(shell).len(), 6 * OBJECT_VERTEX_STRIDE,"picking/stats retain the triangles");
        assert_eq!(cache.surfaces.len(), 1, "draw owner is NOT a material cache key");
        let slot = ordinary.texture.expect("even solid colour has a texture");
        assert_eq!(shell.texture, Some(slot));
        assert_eq!(cache.links[&slot.0], 2);
        let clear = draw(&mut gpu, &[]);
        let object_pixels = draw(&mut gpu, &[ordinary]);
        assert!(object_pixels != clear, "ordinary positive control must draw {surface:?}");
        let before_shell = gpu.draw_calls();
        let shell_pixels = draw(&mut gpu, &[shell]);
        if source.surface_type & 6 == 0 {
            assert_eq!(gpu.draw_calls(), before_shell, "skipped subset is not merely painted black");
            assert!(
                shell_pixels == clear,
                "building shell must skip nontextured subset {surface:?}; first BGRA {:?} vs {:?}",
                &shell_pixels[..4],
                &clear[..4]
            );
        } else {
            assert_eq!(gpu.draw_calls(), before_shell + 1);
            assert_eq!(shell_pixels, object_pixels, "textured building subset remains visible");
        }
        assert_eq!(
            draw(&mut gpu, &[shell, ordinary]),
            object_pixels,
            "skipped shell does not alter following ordinary draw state"
        );
        assert!(!cache.release_group_texture(&mut gpu, slot));
        assert_eq!(cache.links[&slot.0], 1);
        assert_eq!(draw(&mut gpu, &[ordinary]), object_pixels, "shared texture survives one owner");
        assert!(cache.release_group_texture(&mut gpu, slot));
        assert!(cache.surfaces.is_empty());
        assert!(gpu.texture_mip_levels(slot).is_none());
    }
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_land_bake_tags_shells_and_alpha_flush_excludes_them_without_hiding_cell_statics() {
    let store = retail_store();
    let mut gpu = gpu();
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            character: false,
            particles: false,
            part_degrades: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(scene.draw.blocks.len(), 1);
    let key = *scene.draw.blocks.keys().next().unwrap();
    let block = &scene.draw.blocks[&key];
    assert!(block.buildings > 0);
    assert!(
        block
            .opaque
            .iter()
            .chain(&block.blended)
            .any(|b| b.building_pass && b.surface_type & 6 == 0),
        "real LandContext::bake building call must carry shell ownership"
    );
    assert!(
        block.opaque.iter().chain(&block.blended).any(|b| !b.building_pass),
        "ordinary placements must retain their own draw owner"
    );
    let resident_textures = gpu.live_textures();
    let translucent = visible_translucent_solid(&store);
    let mut pair = batches(&store, &mut scene.draw.land.bake, &mut gpu, translucent);
    assert_eq!(pair.len(), 2);
    assert!(
        pair.iter().all(|b| is_alpha_list_member(&b.key)),
        "actual DAT translucent-solid material"
    );
    let ordinary = pair.remove(0);
    let shell = pair.remove(0);
    assert!(
        ordinary.vertices.as_chunks::<OBJECT_VERTEX_STRIDE>().0.iter().all(|v| v[VERTEX_ALPHA_BYTE] == 127),
        "actual SetSurface alpha must reach the ordinary overlap control"
    );
    let clear = draw(&mut gpu, &[]);
    let expected = draw(&mut gpu, &[&ordinary]);
    assert!(expected != clear);
    // A private stationary batch replaces only the draw list, not any asset/physics geometry.
    let block = scene.draw.blocks.get_mut(&key).unwrap();
    let origin = block.origin;
    block.origin = (0.0, 0.0);
    let saved = std::mem::replace(&mut block.blended, vec![shell]);
    assert_eq!(scene.alpha_list_batches(), 0, "hidden shell never enters alpha queue");
    let mut flushed = std::collections::HashSet::new();
    let before_flush = gpu.draw_calls();
    gpu.begin_frame().unwrap();
    scene.draw.flush_alpha_list(&mut gpu, &frame(), &[key], &mut flushed, AlphaFlush::Frame).unwrap();
    gpu.end_frame().unwrap();
    assert!(gpu.capture().unwrap().bgra == clear);
    assert!(flushed.is_empty(), "no eligible batch means no flush marker");
    assert_eq!(gpu.draw_calls(), before_flush);
    scene.draw.blocks.get_mut(&key).unwrap().blended.push(ordinary);
    assert_eq!(scene.alpha_list_batches(), 1);
    gpu.begin_frame().unwrap();
    scene.draw.flush_alpha_list(&mut gpu, &frame(), &[key], &mut flushed, AlphaFlush::Frame).unwrap();
    gpu.end_frame().unwrap();
    assert!(
        gpu.capture().unwrap().bgra == expected,
        "eligible ordinary batch flushed in original order"
    );
    assert_eq!(flushed, std::collections::HashSet::from([key]));
    assert_eq!(gpu.draw_calls(), before_flush + 1, "eligible batch is submitted once");
    // The same production helper also backs indoor object drawing. These are NOT env-cell faces.
    gpu.begin_frame().unwrap();
    scene.draw.draw_cell_statics(&mut gpu, &frame(), &scene.draw.blocks[&key].blended, (0.0, 0.0), None).unwrap();
    gpu.end_frame().unwrap();
    assert!(gpu.capture().unwrap().bgra == expected, "cell static ordinary solid remains visible");
    assert_eq!(gpu.draw_calls(), before_flush + 2);

    // Genuine overlap/order oracle. Both surfaces are the actual DAT alpha0.5 white material;
    // only the intervening quad's RGB vertex tint is synthetic red, preserving authored alpha.
    // This replaces the old clipped_outdoor_pass fixed camera's stale pixel-difference premise.
    let mut intervening = batches(&store, &mut scene.draw.land.bake, &mut gpu, translucent);
    for vertex in intervening[0].vertices.as_chunks_mut::<OBJECT_VERTEX_STRIDE>().0 {
        vertex[OBJECT_DIFFUSE_OFFSET] = 0;
        vertex[OBJECT_DIFFUSE_OFFSET + 1] = 0;
        vertex[OBJECT_DIFFUSE_OFFSET + 2] = 255;
    }
    let expected_early = draw(&mut gpu, &[&scene.draw.blocks[&key].blended[1], &intervening[0]]);
    let expected_deferred = draw(&mut gpu, &[&intervening[0], &scene.draw.blocks[&key].blended[1]]);
    assert!(expected_early != expected_deferred, "ordinary alpha overlap must discriminate order");
    for early in [true, false] {
        let mut flushed = std::collections::HashSet::new();
        let before = gpu.draw_calls();
        gpu.begin_frame().unwrap();
        if early {
            scene.draw.flush_alpha_list(&mut gpu, &frame(), &[key], &mut flushed, AlphaFlush::Frame).unwrap();
        }
        submit_static_batch(
            &mut gpu,
            &frame(),
            &world_constants(&Frame::default()),
            &intervening[0],
            None,
            None,
        )
        .unwrap();
        // The alpha flush is the only
        // path a baked blended batch reaches the device by, so the two arms are the same flush
        // issued on either side of the intervening quad.
        if !early {
            scene.draw.flush_alpha_list(&mut gpu, &frame(), &[key], &mut flushed, AlphaFlush::Frame).unwrap();
        }
        gpu.end_frame().unwrap();
        let pixels = gpu.capture().unwrap().bgra;
        assert_eq!(
            gpu.draw_calls(),
            before + 2,
            "ordinary alpha once; hidden shell never; early={early}"
        );
        let expected_order = if early { &expected_early } else { &expected_deferred };
        assert!(
            pixels == *expected_order,
            "the flush must preserve source order, and where it is issued must decide the overlap"
        );
        eprintln!(
            "explicit ordinary-alpha overlap early={early}: BGRA {:?}, exactly2 draws",
            &pixels[..4]
        );
    }
    for b in intervening {
        scene.draw.land.bake.release_group_texture(&mut gpu, b.texture.unwrap());
    }
    let block = scene.draw.blocks.get_mut(&key).unwrap();
    block.origin = origin;
    let injected = std::mem::replace(&mut block.blended, saved);
    for batch in injected {
        assert_eq!(drawn_vertices(&batch).len(), 6 * OBJECT_VERTEX_STRIDE);
        scene.draw.land.bake.release_group_texture(&mut gpu, batch.texture.unwrap());
    }
    assert_eq!(
        gpu.live_textures(),
        resident_textures,
        "injected owners return to real resident baseline"
    );
    scene.release_textures(&mut gpu);
    assert_eq!(gpu.texture_table_stats().unknown_releases, 0);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn real_degrade_levels_keep_owner_and_active_chunks_without_filtering_geometry() {
    let store = retail_store();
    let mut cache = BakeCache::default();
    let (id, info) = store
        .ids_of(DbType::GfxObj)
        .into_iter()
        .find_map(|id| {
            let info = cache.degrade_record(&store, id)?;
            (info.degrades.iter().filter(|e| e.gfxobj_id.0 != 0).count() >= 2
                && cache.draws_at_near_band(&store, id))
            .then_some((id, info))
        })
        .expect("required actual multi-level DAT object");
    let mut baker = ObjectBaker::new(&store, &mut cache, true, true, false);
    baker.add_object_kind(id, &Frame::default(), 1.0, true);
    baker.add_object(id, &Frame::default(), 1.0);
    assert_eq!(baker.placements.len(), 2);
    let mut gpu = gpu();
    let (opaque, blended, mut placements) = baker.finish(&mut gpu).unwrap();
    let mut batches: Vec<_> = opaque.into_iter().chain(blended).collect();
    assert!(!batches.is_empty());
    assert!(batches.iter().all(|b| !b.chunks.is_empty()));
    for b in &batches {
        assert!(
            b.chunks.iter().all(|c| c.placement == u32::from(!b.building_pass)),
            "all levels must retain their owner's placement index"
        );
    }
    let whole: Vec<_> = batches.iter().map(|b| b.vertices.clone()).collect();
    let mut nonempty_levels = 0;
    for (level, entry) in info.degrades.iter().enumerate() {
        let level = u32::try_from(level).unwrap();
        for placement in &mut placements {
            placement.level = level;
        }
        assemble_batches(&mut batches, &placements);
        let mut building_bytes = Vec::new();
        let mut ordinary_bytes = Vec::new();
        for b in &batches {
            let expected: Vec<_> = b
                .chunks
                .iter()
                .filter(|c| c.level == level)
                .flat_map(|c| b.vertices[c.start as usize..c.end as usize].iter().copied())
                .collect();
            assert_eq!(
                drawn_vertices(b),
                expected,
                "active level bytes are not visibility-filtered"
            );
            let out = if b.building_pass { &mut building_bytes } else { &mut ordinary_bytes };
            out.extend_from_slice(drawn_vertices(b));
        }
        assert_eq!(building_bytes, ordinary_bytes, "same real mesh at every owner/level");
        if entry.gfxobj_id.0 == 0 {
            assert!(building_bytes.is_empty());
        } else if !building_bytes.is_empty() {
            nonempty_levels += 1;
        }
        assert_eq!(batches.iter().map(|b| b.vertices.clone()).collect::<Vec<_>>(), whole);
    }
    assert!(nonempty_levels >= 2);
    eprintln!(
        "real degrade object {id:?}: {} levels, {nonempty_levels} nonempty",
        info.degrades.len()
    );
    for b in batches {
        if let Some(slot) = b.texture {
            cache.release_group_texture(&mut gpu, slot);
        }
    }
    assert!(cache.links.is_empty());
    assert_eq!(gpu.live_textures(), 0);
}
