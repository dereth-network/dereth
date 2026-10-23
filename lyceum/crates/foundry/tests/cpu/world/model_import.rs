//! Behaviour: none (tooling: original triangle scenes compiled through the world's build)
//! Fixtures are generated glTF and GLB files. No model bytes come from the data files.

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup, Surface};
use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::compile;
use lyceum_foundry::lock::Lock;
use lyceum_foundry::symbol::Kind;
use serde_json::{json, Value};

use crate::common::{self, Fixture};

fn source(f: &Fixture, nodes: Value, collision: &str) -> Value {
    f.world();
    f.write(
        "sample/records/model.toml",
        &format!(
            "[[model]]\nname = \"arch\"\nfile = \"art/model.gltf\"\ncollision = \"{collision}\"\n"
        ),
    );
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[static]]\nname = \"arch\"\nmodel = \"arch\"\nat = [20.0, 30.0, 5.0]\n",
    );
    let vertices: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]];
    let bytes: Vec<u8> = vertices
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(f.path().join("sample/art/mesh.bin"), bytes).unwrap();
    let count = nodes.as_array().unwrap().len();
    let doc = json!({
        "asset":{"version":"2.0"},
        "scene":0,
        "scenes":[{"nodes":(0..count).collect::<Vec<_>>()}],
        "nodes":nodes,
        "buffers":[{"uri":"mesh.bin","byteLength":36}],
        "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[2,3,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1.0,0.5,0.0,1.0]}}]
    });
    write_doc(f, &doc);
    doc
}

fn write_doc(f: &Fixture, doc: &Value) {
    f.write("sample/art/model.gltf", &doc.to_string());
}

fn build(f: &Fixture) -> Lock {
    compile::build(&f.path().join("world.toml"), &f.dats(), None, false).unwrap();
    Lock::read(&f.path().join("world.lock")).unwrap()
}

fn record<T: Decode>(f: &Fixture, lock: &Lock, kind: Kind, name: &str) -> T {
    let id = lock.get(kind, name).unwrap();
    let bytes = common::read_through(&f.dats(), &f.out(), ModernDat::Portal, id).unwrap();
    T::decode_payload(DataId(id), &bytes).unwrap()
}

fn model(f: &Fixture, lock: &Lock, level: usize) -> GfxObj {
    record(
        f,
        lock,
        Kind::GfxObj,
        &format!("sample:arch-part-0-lod-{level}"),
    )
}

#[test]
fn a_scene_places_its_named_setup_with_meter_dimensions_and_upright_node_frames() {
    let f = Fixture::new("model-transform");
    source(
        &f,
        json!([{"name":"arch","mesh":0,"translation":[4.0,5.0,6.0],"scale":[2.0,1.0,1.0]}]),
        "mesh",
    );
    let lock = build(&f);
    let setup: Setup = record(&f, &lock, Kind::Setup, "sample:arch");
    assert_eq!(setup.parts.len(), 1);
    assert!(setup.has_physics_bsp);
    let mesh = model(&f, &lock, 0);
    let frame = setup.placement_frames[&0].frames[0];
    let positions: Vec<Vec3> = mesh
        .vertex_array
        .vertices
        .iter()
        .map(|v| dereth_primitives::frame::localtoglobal(&frame, v.position))
        .collect();
    let expected = [
        Vec3::new(4.0, -6.0, 5.0),
        Vec3::new(8.0, -6.0, 5.0),
        Vec3::new(4.0, -6.0, 8.0),
    ];
    for (actual, expected) in positions.iter().zip(expected) {
        assert!(
            (actual.x - expected.x).abs() < 1e-5
                && (actual.y - expected.y).abs() < 1e-5
                && (actual.z - expected.z).abs() < 1e-5,
            "{actual:?}"
        );
    }
    let bytes = common::read_through(&f.dats(), &f.out(), ModernDat::Cell, 0x0102_FFFE).unwrap();
    let statics =
        dereth_assets::world::LandblockInfo::decode_payload(DataId(0x0102_FFFE), &bytes).unwrap();
    assert_eq!(statics.objects[0].id, setup.id);
    let surface: Surface = record(&f, &lock, Kind::Surface, "sample:arch-material-0");
    assert_eq!(surface.color_value, Some(0xFFFF_BC00));
}

#[test]
fn rebuilding_an_import_keeps_its_ids_and_encoded_records() {
    let f = Fixture::new("model-stable");
    source(&f, json!([{"name":"arch","mesh":0}]), "mesh");
    let first = build(&f);
    let before = model(&f, &first, 0).encode();
    let second = build(&f);
    assert_eq!(first, second);
    assert_eq!(before, model(&f, &second, 0).encode());
}

#[test]
fn collision_can_use_the_mesh_a_cylinder_a_sphere_or_nothing() {
    for choice in ["mesh", "cylinder", "sphere", "none"] {
        let f = Fixture::new(&format!("model-collision-{choice}"));
        source(&f, json!([{"name":"arch","mesh":0}]), choice);
        let lock = build(&f);
        let setup: Setup = record(&f, &lock, Kind::Setup, "sample:arch");
        let mesh = model(&f, &lock, 0);
        assert_eq!(mesh.physics_bsp.is_some(), choice == "mesh");
        assert_eq!(setup.has_physics_bsp, choice == "mesh");
        assert_eq!(setup.cylspheres.len(), usize::from(choice == "cylinder"));
        assert_eq!(setup.spheres.len(), usize::from(choice == "sphere"));
        if let Some(cylinder) = setup.cylspheres.first() {
            assert!((cylinder.radius - 1.0).abs() < 1e-5);
            assert!((cylinder.height - 3.0).abs() < 1e-5);
        }
        if let Some(sphere) = setup.spheres.first() {
            assert!((sphere.radius - 3.25_f32.sqrt()).abs() < 1e-5);
        }
    }
}

#[test]
fn lower_detail_nodes_become_bands_and_only_the_base_collides() {
    let f = Fixture::new("model-detail");
    source(
        &f,
        json!([{"name":"arch","mesh":0},{"name":"arch_lod1","mesh":0}]),
        "mesh",
    );
    let lock = build(&f);
    let setup: Setup = record(&f, &lock, Kind::Setup, "sample:arch");
    assert_eq!(setup.parts.len(), 1);
    let base = model(&f, &lock, 0);
    let lower = model(&f, &lock, 1);
    assert!(base.physics_bsp.is_some());
    assert!(lower.physics_bsp.is_none());
    let bands: GfxObjDegradeInfo = record(&f, &lock, Kind::Degrade, "sample:arch-part-0");
    assert_eq!(base.did_degrade, Some(bands.id));
    assert_eq!(
        bands
            .degrades
            .iter()
            .map(|d| d.gfxobj_id)
            .collect::<Vec<_>>(),
        [base.id, lower.id, DataId(0)]
    );
    assert_eq!(bands.degrades[0].ideal_dist, 40.0);
    assert_eq!(bands.degrades[1].ideal_dist, 80.0);
    assert_eq!(bands.degrades[2].max_dist, f32::MAX);
}

#[test]
fn a_named_collision_mesh_is_not_drawn_and_keeps_its_own_position() {
    let f = Fixture::new("model-named-collision");
    source(
        &f,
        json!([{"name":"arch","mesh":0},{"name":"arch_collision","mesh":0,"translation":[0.0,0.0,2.0]}]),
        "mesh",
    );
    let lock = build(&f);
    let mesh = model(&f, &lock, 0);
    let setup: Setup = record(&f, &lock, Kind::Setup, "sample:arch");
    assert_eq!(setup.parts.len(), 1);
    assert_eq!(mesh.polygons.len(), 1);
    assert_eq!(mesh.physics_polygons.len(), 1);
    let frame = setup.placement_frames[&0].frames[0];
    let p = &mesh.physics_polygons[0];
    for id in &p.vertex_ids {
        let point = dereth_primitives::frame::localtoglobal(
            &frame,
            mesh.vertex_array.vertices[usize::from(*id)].position,
        );
        assert!((point.y + 2.0).abs() < 1e-5);
    }
}

#[test]
fn a_binary_gltf_reads_the_same_geometry_as_its_json_and_buffer() {
    let f = Fixture::new("model-glb");
    let mut doc = source(&f, json!([{"name":"arch","mesh":0}]), "mesh");
    let lock = build(&f);
    let before = model(&f, &lock, 0);
    doc["buffers"][0].as_object_mut().unwrap().remove("uri");
    let mut json = doc.to_string().into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let bin = std::fs::read(f.path().join("sample/art/mesh.bin")).unwrap();
    let mut bytes = b"glTF".to_vec();
    for word in [
        2,
        u32::try_from(28 + json.len() + bin.len()).unwrap(),
        u32::try_from(json.len()).unwrap(),
        0x4E4F_534A,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend(json);
    bytes.extend_from_slice(&u32::try_from(bin.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    bytes.extend(bin);
    std::fs::write(f.path().join("sample/art/model.glb"), bytes).unwrap();
    f.write(
        "sample/records/model.toml",
        "[[model]]\nname = \"arch\"\nfile = \"art/model.glb\"\n",
    );
    let lock = build(&f);
    assert_eq!(before, model(&f, &lock, 0));
}

#[test]
fn malformed_meshes_and_incomplete_detail_chains_are_refused_before_build_output() {
    for case in [
        "short-buffer",
        "cycle",
        "missing-base",
        "missing-level",
        "bad-mode",
        "nonfinite",
    ] {
        let f = Fixture::new(&format!("model-refuse-{case}"));
        let mut doc = source(&f, json!([{"name":"arch","mesh":0}]), "mesh");
        match case {
            "short-buffer" => {
                std::fs::write(f.path().join("sample/art/mesh.bin"), [0; 4]).unwrap();
            }
            "cycle" => doc["nodes"][0]["children"] = json!([0]),
            "missing-base" => doc["nodes"][0]["name"] = json!("arch_lod1"),
            "missing-level" => {
                doc["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name":"arch_lod2","mesh":0}));
                doc["scenes"][0]["nodes"] = json!([0, 1]);
            }
            "bad-mode" => doc["meshes"][0]["primitives"][0]["mode"] = json!(1),
            _ => {
                let mut data = std::fs::read(f.path().join("sample/art/mesh.bin")).unwrap();
                data[..4].copy_from_slice(&f32::NAN.to_le_bytes());
                std::fs::write(f.path().join("sample/art/mesh.bin"), data).unwrap();
            }
        }
        write_doc(&f, &doc);
        let error =
            compile::build(&f.path().join("world.toml"), &f.dats(), None, false).expect_err(case);
        assert!(error.to_string().contains("model.toml"), "{error}");
        assert!(!f.path().join("world.lock").exists());
        assert!(!f.out().join("overlay").exists());
    }
}

#[test]
fn png_base_color_is_tinted_in_linear_space_and_obeys_the_alpha_mode() {
    for (mode, alpha) in [("OPAQUE", 255), ("BLEND", 64)] {
        let f = Fixture::new(&format!("model-picture-{mode}"));
        let mut doc = source(&f, json!([{"name":"arch","mesh":0}]), "none");
        let png = common::png(1, 1, &[255, 255, 255, 128]);
        std::fs::write(f.path().join("sample/art/color.png"), png).unwrap();
        let mut bytes = std::fs::read(f.path().join("sample/art/mesh.bin")).unwrap();
        bytes.extend(
            [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0]
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        std::fs::write(f.path().join("sample/art/mesh.bin"), bytes).unwrap();
        doc["buffers"][0]["byteLength"] = json!(60);
        doc["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":36,"byteLength":24}));
        doc["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
        doc["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
        doc["images"] = json!([{"uri":"color.png"}]);
        doc["textures"] = json!([{"source":0}]);
        doc["materials"][0]["alphaMode"] = json!(mode);
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
            json!([1.0, 0.5, 0.0, 0.5]);
        write_doc(&f, &doc);
        let lock = build(&f);
        let surface: Surface = record(&f, &lock, Kind::Surface, "sample:arch-material-0");
        assert_eq!(surface.surface_type, 2);
        let id = lock
            .get(Kind::RenderSurface, "sample:arch-material-0")
            .unwrap();
        let bytes = common::read_through(&f.dats(), &f.out(), ModernDat::Portal, id).unwrap();
        let image = dereth_assets::RenderSurface::decode_payload(DataId(id), &bytes).unwrap();
        assert_eq!((image.width, image.height, image.format), (1, 1, 21));
        assert_eq!(&bytes[image.data_offset..], &[0, 188, 255, alpha]);
        let mesh = model(&f, &lock, 0);
        assert_eq!(mesh.vertex_array.vertices[1].uvs, [(1.0, 0.0)]);
    }
}

#[test]
fn a_model_can_replace_a_setup_without_allocating_another_setup_id() {
    let f = Fixture::new("model-replace");
    source(&f, json!([{"name":"arch","mesh":0}]), "sphere");
    f.write("sample/records/model.toml", "[[model]]\nname = \"arch\"\nreplace = 0x02000001\nfile = \"art/model.gltf\"\ncollision = \"sphere\"\n");
    f.write("sample/edits/landblocks/0102.toml", "");
    let lock = build(&f);
    assert!(lock.get(Kind::Setup, "sample:arch").is_none());
    let bytes =
        common::read_through(&f.dats(), &f.out(), ModernDat::Portal, common::SETUP).unwrap();
    let setup = Setup::decode_payload(DataId(common::SETUP), &bytes).unwrap();
    assert_eq!(setup.spheres.len(), 1);
    assert_eq!(setup.parts, [model(&f, &lock, 0).id]);
}

#[test]
fn nested_mirrored_nodes_keep_world_positions_and_outward_winding() {
    use dereth_primitives::frame::{l2g, localtoglobal, localtoglobalvec, V3};
    let f = Fixture::new("model-mirrored-hierarchy");
    let mut doc = source(
        &f,
        json!([
            {"translation":[10.0,20.0,30.0],"scale":[-2.0,3.0,4.0],"children":[1]},
            {"name":"arch","mesh":0,"translation":[1.0,2.0,3.0]}
        ]),
        "mesh",
    );
    doc["scenes"][0]["nodes"] = json!([0]);
    write_doc(&f, &doc);
    let lock = build(&f);
    let setup: Setup = record(&f, &lock, Kind::Setup, "sample:arch");
    let mesh = model(&f, &lock, 0);
    let frame = setup.placement_frames[&0].frames[0];
    let points: Vec<_> = mesh
        .vertex_array
        .vertices
        .iter()
        .map(|v| localtoglobal(&frame, v.position))
        .collect();
    for (point, expected) in points.iter().zip([
        Vec3::new(8.0, -42.0, 26.0),
        Vec3::new(4.0, -42.0, 26.0),
        Vec3::new(8.0, -42.0, 35.0),
    ]) {
        assert!(point.sub(expected).mag2() < 1e-8, "{point:?}");
    }
    let ids = &mesh.polygons[0].vertex_ids;
    let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| points[usize::from(i)]);
    let outward = b.sub(a).cross(c.sub(a)).normalize();
    let normal = localtoglobalvec(l2g(frame.rotation), mesh.vertex_array.vertices[0].normal);
    assert!(outward.dot(normal) > 0.99999);
}

#[test]
fn fit_reads_imported_drawing_and_collision_in_their_placement_frames() {
    use lyceum_foundry::{dats::DatSet, fit};
    let f = Fixture::new("model-fit-loading");
    source(
        &f,
        json!([
            {"name":"arch","mesh":0,"translation":[4.0,5.0,6.0]},
            {"name":"arch_collision","mesh":0,"translation":[4.0,5.0,8.0]}
        ]),
        "mesh",
    );
    let lock = build(&f);
    let set = DatSet::open(&f.dats())
        .unwrap()
        .with_overlay(&f.out().join("overlay"), None)
        .unwrap();
    let store =
        dereth_dat::RetailDatStore::open_with(set.portal.clone(), set.cell, set.portal, None);
    let id = DataId(lock.get(Kind::Setup, "sample:arch").unwrap());
    let loaded = fit::load(&store, id).unwrap();
    assert_eq!(loaded.drawn.len(), 1);
    assert_eq!(loaded.physics.parts.len(), 1);
    assert!(loaded.physics.caches_physics_bsp());
    let p = loaded.drawn[0][0][0];
    assert!((p.x - 4.0).abs() < 1e-5 && (p.y + 6.0).abs() < 1e-5 && (p.z - 5.0).abs() < 1e-5);
    let report = fit::measure(&loaded, 8).unwrap();
    assert!((report.largest_gap.unwrap() - 2.0).abs() < 1e-5);
    assert_eq!(report.within_five_cm, 0);
    assert_eq!(report.volume, None);
}
