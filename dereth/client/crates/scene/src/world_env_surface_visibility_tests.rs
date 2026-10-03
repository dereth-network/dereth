// Exercise the actual environment-surface draw consumers with synthetic stationary geometry and
// unchanged installed surfaces. This does not reproduce any particular camera or cell.

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

fn draw(gpu: &mut Gpu, cell: &EnvCellDraw, env: bool) -> Vec<u8> {
    let frame = PerFrameConstants {
        view_proj: hlsl_matrix(glam::Mat4::IDENTITY),
        view: hlsl_matrix(glam::Mat4::IDENTITY),
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..Default::default()
    };
    gpu.begin_frame().unwrap();
    if env {
        SceneDraw::draw_env_cell(gpu, &frame, cell, (0.0, 0.0), None, None).unwrap();
    } else {
        let part = dereth_animation::parts::PhysicsPart::new(DataId(0));
        for mesh in &cell.meshes {
            submit_part_mesh(gpu, &frame, &part, &Frame::default(), mesh, true, None, false).unwrap();
        }
    }
    gpu.end_frame().unwrap();
    gpu.capture().unwrap().bgra
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn actual_cell_draw_skips_solid_subset_but_ordinary_object_still_draws_it() {
    check(DataId(0x0800_0139), true);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats: --features retail-dats")]
fn actual_cell_draw_keeps_textured_subset() {
    check(DataId(0x0800_0005), false);
}

fn check(surface: DataId, untextured: bool) {
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let textures = TextureStore::new(&store);
    let source = read_surface(&store, surface).expect("retail surface");
    assert_eq!(source.surface_type & 6 == 0, untextured);
    if untextured {
        assert_eq!(source.color_value, Some(0xffc8_c8c8));
    }
    let mut gpu = Gpu::new(
        None,
        &dereth_render::device::DeviceConfig {
            width: 16,
            height: 16,
            ..Default::default()
        },
    )
    .expect("required headless WARP device");
    let mut cache = BakeCache::default();
    let meshes =
        build_meshes(&store, &mut cache, &textures, &mut gpu, &[quad(surface)], None, None)
            .unwrap();
    assert_eq!(meshes.len(), 1, "draw guard must not erase baked geometry");
    assert_eq!(meshes[0].surface_type, source.surface_type);
    assert!(meshes[0].texture.is_some(), "even the solid surface has a bound colour texel");
    let bytes = meshes[0].vertices.clone();
    let cell = EnvCellDraw {
        id: CellId(0x7f03_0100),
        frame: Frame::default(),
        portals: vec![],
        meshes,
        from_look: false,
        statics: vec![],
        statics_blended: vec![],
        degrade: vec![],
        burned_count: None,
    };
    gpu.begin_frame().unwrap();
    gpu.end_frame().unwrap();
    let clear = gpu.capture().unwrap().bgra;
    let ordinary = draw(&mut gpu, &cell, false);
    assert_ne!(ordinary, clear, "the ordinary-object control must actually draw");
    let interior = draw(&mut gpu, &cell, true);
    if untextured {
        assert_eq!(interior, clear, "retail's isEnvCell guard must skip this solid-colour subset");
    } else {
        assert_eq!(interior, ordinary, "the guard must preserve textured cell geometry");
    }
    assert_eq!(cell.meshes[0].vertices, bytes, "draw eligibility never mutates geometry");
    assert_eq!(draw(&mut gpu, &cell, false), ordinary, "later ordinary draw remains visible");
}
