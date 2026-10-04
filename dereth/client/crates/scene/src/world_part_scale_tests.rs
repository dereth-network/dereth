// A setup's own per-part scale reaches the baked landscape objects: the flowering shrub
// `0x020007A3` that scenery generation grows by the water west of the Holtburg dungeon entrance
// (block 0xA8B5, land cell 0x3E) is baked through the production object bake and its vertices
// measured against the setup record and the part meshes read independently from the retail dats.

/// The shrub: two parts, each a full-size mesh the setup shrinks per axis.
const SHRUB: DataId = DataId(0x0200_07A3);

/// Every baked object vertex position, from the bake's group buffers.
fn baked_positions(baker: &ObjectBaker<'_>) -> Vec<Vec3> {
    let mut out = Vec::new();
    for key in &baker.order {
        let (buf, _) = &baker.groups[key];
        for v in buf.as_chunks::<OBJECT_VERTEX_STRIDE>().0 {
            let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
            out.push(Vec3::new(f(0), f(4), f(8)));
        }
    }
    out
}

fn bounds(points: impl IntoIterator<Item = Vec3>) -> (Vec3, Vec3) {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for p in points {
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo, hi)
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 && (a.z - b.z).abs() < 1e-3
}

/// Behaviour: rendering.scenery.a-setups-part-scale-sizes-its-baked-parts
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_ranged_scenery_shrub_is_drawn_at_its_scale_times_each_parts_own_scale() {
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let region = load_region(&store).expect("retail region");
    let (bx, by) = (0xA8, 0xB5);
    let lb =
        dereth_client_runtime::world_build::read_landblock(&store, bx, by).expect("block 0xA8B5");
    let table = dereth_world_render::land::mesh::height_table(&region);
    let mesh = dereth_world_render::land::mesh::generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_world_render::land::mesh::Direction::InViewerBlock,
    );
    let content =
        dereth_client_runtime::world_build::land_content(&store, &region, &lb, &mesh, bx, by, true);
    // The scale step's pick for this placement, from the scene record's 0.5..2.5 range.
    let shrub = content
        .placed
        .iter()
        .find(|p| p.gfxobj == SHRUB && p.cell == CellId(0xA8B5_003E))
        .expect("scenery grows the shrub in land cell 0xA8B5003E");
    assert!(
        (shrub.scale - 2.225).abs() < 1e-3,
        "the shrub's scenery scale is {}",
        shrub.scale
    );
    let s = shrub.scale;

    // The setup and its part meshes, read straight from the dats.
    let bytes = store
        .read_typed(DbType::Setup, SHRUB)
        .expect("the shrub's setup");
    let setup = dereth_assets::Setup::decode_payload_in(store.era_of(SHRUB), SHRUB, &bytes)
        .expect("the setup decodes");
    let part_scales = setup
        .default_scale
        .clone()
        .expect("the shrub's setup scales its parts");
    assert_eq!(part_scales.len(), setup.parts.len());
    assert!(
        part_scales.iter().all(|d| d.x < 1.0 && d.z < 1.0),
        "every part is shrunk: {part_scales:?}"
    );
    let frames = setup
        .placement_frames
        .get(&0x65)
        .or_else(|| setup.placement_frames.get(&0))
        .expect("a placement")
        .frames
        .clone();
    let expected = bounds(setup.parts.iter().enumerate().flat_map(|(i, &gfx)| {
        let d = part_scales[i];
        let part = dereth_world_render::objects::parts::combine_scaled(
            &Frame::default(),
            &frames[i],
            Vec3::new(s, s, s),
        );
        dereth_client_runtime::models::build_gfxobj(&store, gfx)
            .into_iter()
            .flat_map(|g| g.vertices.into_iter().map(|(p, _, _)| p))
            .map(move |p| {
                dereth_world_render::math::localtoglobal(
                    &part,
                    Vec3::new(p.x * d.x * s, p.y * d.y * s, p.z * d.z * s),
                )
            })
            .collect::<Vec<_>>()
    }));

    let mut cache = BakeCache::default();
    let mut baker = ObjectBaker::new(&store, &mut cache, false, false, false);
    baker.add_object(SHRUB, &Frame::default(), s);
    let drawn = bounds(baked_positions(&baker));
    assert!(
        close(drawn.0, expected.0) && close(drawn.1, expected.1),
        "drawn {drawn:?}, the setup's part scales give {expected:?}"
    );
    // The shrub stands well under its unshrunk mesh's height at this scale.
    let full = bounds(
        dereth_client_runtime::models::build_gfxobj(&store, setup.parts[0])
            .into_iter()
            .flat_map(|g| g.vertices.into_iter().map(|(p, _, _)| p)),
    );
    assert!(
        drawn.1.z - drawn.0.z < 0.5 * (full.1.z - full.0.z) * s,
        "drawn {} m tall; its first part's mesh at full size would be {} m",
        drawn.1.z - drawn.0.z,
        (full.1.z - full.0.z) * s
    );
}
