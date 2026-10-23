//! Behaviour: none (tooling: original indoor meshes, portals, kit placement and collision)

use dereth_assets::{Decode, Environment};
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::source::{Link, Rotation};
use serde_json::json;

use crate::common::{Fixture, HOME};

fn source(f: &Fixture, reverse: bool, portal_name: &str) {
    f.world();
    f.write(
        "sample/records/room.toml",
        "[[piece]]\nname = \"hall\"\nfile = \"art/hall.gltf\"\n",
    );
    // Inward faces of an eight-meter room, four meters tall, in the world's Z-up axes.
    let corners: [[f32; 3]; 8] = [
        [-4., -4., 0.],
        [4., -4., 0.],
        [-4., 4., 0.],
        [4., 4., 0.],
        [-4., -4., 4.],
        [4., -4., 4.],
        [-4., 4., 4.],
        [4., 4., 4.],
    ];
    let faces = [
        [1, 3, 2, 0],
        [6, 7, 5, 4],
        [5, 7, 3, 1],
        [2, 6, 4, 0],
        [3, 7, 6, 2],
        [4, 5, 1, 0],
    ];
    let groups = [vec![0, 1, 4, 5], vec![2], vec![3]];
    let mut bytes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    for group in groups {
        let offset = bytes.len();
        for face in group {
            for mut tri in [[0, 1, 2], [0, 2, 3]] {
                if reverse {
                    tri.swap(1, 2);
                }
                for k in tri {
                    let [x, y, z] = corners[faces[face][k]];
                    for v in [x, z, -y] {
                        bytes.extend(v.to_le_bytes());
                    }
                }
            }
        }
        let count = (bytes.len() - offset) / 12;
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":count*12}));
        let i = accessors.len();
        accessors.push(json!({"bufferView":i,"componentType":5126,"count":count,
            "type":"VEC3","min":[-4.,0.,-4.],"max":[4.,4.,4.]}));
        meshes.push(json!({"primitives":[{"attributes":{"POSITION":i}}]}));
    }
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(f.path().join("sample/art/hall.bin"), &bytes).unwrap();
    let doc = json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0,1,2]}],
        "nodes":[{"name":"walls","mesh":0},{"name":portal_name,"mesh":1},{"name":"portal_1","mesh":2}],
        "meshes":meshes,"buffers":[{"uri":"hall.bin","byteLength":bytes.len()}],
        "bufferViews":views,"accessors":accessors});
    f.write("sample/art/hall.gltf", &doc.to_string());
}

#[test]
fn an_original_piece_is_in_the_kit_before_placement_and_its_portals_are_not_walls() {
    let f = Fixture::new("piece-kit");
    source(&f, false, "portal_0");
    let s = Session::open(&f.path().join("world.toml"), &f.dats(), None).unwrap();
    let pieces = s.new_pieces();
    assert_eq!(pieces.len(), 1);
    let p = &pieces[0];
    assert_eq!(p.size, [8., 8., 4.]);
    assert_eq!(p.portals, 2);
    assert!(p.landblocks.is_empty());
    assert!(!p.surfaces.is_empty());
    let env = s.environment(p.environment).unwrap();
    let bytes = env.encode();
    assert_eq!(
        Environment::decode_payload(p.environment, &bytes).unwrap(),
        env
    );
    let cs = &env.cells[0];
    assert_eq!(cs.physics_polygons.len(), 8);
    assert_eq!(cs.polygons.len(), 10);
    assert_eq!(cs.portals, [8, 9]);
    let wall_id = p.surfaces[usize::from(cs.polygons[0].pos_surface)];
    let bytes = &s.compiled().records[&dereth_dat::ModernDat::Portal][&wall_id.raw()].bytes;
    let wall = dereth_assets::Surface::decode_payload(wall_id, bytes).unwrap();
    assert_eq!(
        wall.surface_type, 2,
        "indoor walls require a texture, including solid glTF colors"
    );
    assert!(wall.orig_texture_id.is_some());
    for index in &cs.portals {
        let poly = &cs.polygons[usize::from(*index)];
        let id = p.surfaces[usize::from(poly.pos_surface)];
        let bytes = &s.compiled().records[&dereth_dat::ModernDat::Portal][&id.raw()].bytes;
        let surface = dereth_assets::Surface::decode_payload(id, bytes).unwrap();
        assert_eq!(surface.surface_type, 0x11);
        assert_eq!(surface.translucency, 1.0);
    }
    let pairs: Vec<_> = cs
        .drawing_bsp
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .flat_map(|n| n.in_portals.iter().copied())
        .collect();
    assert!(pairs.contains(&(8, 0)) && pairs.contains(&(9, 1)));
    let model = dereth_assets::GfxObj {
        id: DataId(0x0100_0000),
        flags: 1,
        surfaces: Vec::new(),
        vertex_array: cs.vertex_array.clone(),
        physics_polygons: cs.physics_polygons.clone(),
        physics_bsp: Some(cs.physics_bsp.clone()),
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    let physics = dereth_world_data::env_cells::gfxobj_physics_bsp(&model).unwrap();
    let sphere = dereth_physics::Sphere::new(Vec3::new(3.9, 0., 1.), 0.48);
    let (mut hit, mut contact) = (None, Vec3::ZERO);
    assert!(!physics.sphere_intersects_poly(
        &sphere,
        Vec3::new(0.12, 0., 0.),
        &mut hit,
        &mut contact
    ));
    let wall = dereth_physics::Sphere::new(Vec3::new(0., 3.9, 1.), 0.48);
    assert!(physics.sphere_intersects_poly(&wall, Vec3::new(0., 0.12, 0.), &mut hit, &mut contact));
    let floor = dereth_physics::Sphere::new(Vec3::new(1., 0., 0.4), 0.48);
    assert!(physics.hits_walkable(&floor, Vec3::new(0., 0., 1.), 0.66));
}

#[test]
fn two_original_pieces_snap_join_and_walk_over_their_own_floors() {
    let f = Fixture::new("piece-join");
    source(&f, false, "portal_0");
    let mut s = Session::open(&f.path().join("world.toml"), &f.dats(), None).unwrap();
    let p = s.new_pieces().remove(0);
    let env = s.environment(p.environment).unwrap();
    let first = Frame::new(Vec3::new(40., 40., -40.), Quat::IDENTITY);
    s.apply(&Op::PlaceCell {
        landblock: HOME,
        environment: p.environment,
        piece: 0,
        surfaces: p.surfaces.clone(),
        at: [40., 40., -40.],
        rotation: Rotation::Heading(0.),
        links: vec![Link::Shut, Link::Shut],
    })
    .unwrap();
    let target = lyceum_foundry::pieces::opening(&env, 0, 0, &first).unwrap();
    let (frame, _) = lyceum_foundry::pieces::snap(&env, 0, 1, &target, 0., 90.).unwrap();
    let q = frame.rotation;
    s.apply(&Op::PlaceCell {
        landblock: HOME,
        environment: p.environment,
        piece: 0,
        surfaces: p.surfaces,
        at: [frame.origin.x, frame.origin.y, frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
        links: vec![
            Link::Shut,
            Link::Cell {
                cell: "sample:cell-1".into(),
                portal: 0,
            },
        ],
    })
    .unwrap();
    let cells = s.cells(HOME);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[&0x100].visible_cells, [0x101]);
    for (id, x) in [(0x100, 40.), (0x101, 48.)] {
        assert_eq!(s.cell_at(HOME, Vec3::new(x, 40., -39.)), Some((id, -40.)));
    }
    let walked = |step: Vec3| {
        use dereth_physics::{
            PhysicsWorld, ScriptedMotion, SetupGeometry, Sphere, StaticLandSource,
        };
        use dereth_primitives::{CellId, LocalTime, ObjectId, Position};
        use std::sync::Arc;
        let mut land = StaticLandSource::linear();
        for cell in cells.values() {
            let decoded = dereth_world_data::env_cells::DecodedCell {
                id: CellId(cell.id.raw()),
                cell: cell.clone(),
                structure: env.cells[0].clone(),
            };
            land.add_cell(dereth_world_data::env_cells::physics_geometry(&decoded));
        }
        let mut world = PhysicsWorld::new(Arc::new(land));
        let body = Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0., 0., 0.48), 0.48)],
            radius: 0.48,
            height: 0.96,
            step_up_height: 0.4,
            step_down_height: 0.4,
            ..SetupGeometry::default()
        });
        let mover = world.create(ObjectId(1), body, true);
        let cell = CellId((u32::from(HOME) << 16) | 0x100);
        world.enter_cell(mover, cell);
        let object = world.get_mut(mover).unwrap();
        object.position = Position::new(cell, first);
        object.set_motion(Box::new(ScriptedMotion::new(
            vec![Frame::new(step, Quat::IDENTITY); 81],
            true,
        )));
        object.transient_state.set_active_bit(true);
        object.calc_acceleration();
        object.update_time = 0.;
        world.calc_cross_cells(mover, false);
        for i in 1..=80 {
            world.use_time(LocalTime(f64::from(i) / 30.), false);
        }
        world.get(mover).unwrap().position
    };
    let across = walked(Vec3::new(0.12, 0., 0.));
    assert_eq!(across.cell.0 & 0xFFFF, 0x101, "{across:?}");
    assert!(
        across.frame.origin.x > 48. && (across.frame.origin.z + 40.).abs() < 0.01,
        "{across:?}"
    );
    let stopped = walked(Vec3::new(0., 0.12, 0.));
    assert_eq!(stopped.cell.0 & 0xFFFF, 0x100);
    assert!(
        (43.4..43.6).contains(&stopped.frame.origin.y),
        "{stopped:?}"
    );
    let findings = s.walk(HOME);
    assert!(
        findings
            .iter()
            .all(|f| !f.text.contains("falls") && !f.text.contains("reaches")),
        "{findings:?}"
    );
    assert!(s.environment(DataId(0x0D00_FFFE)).is_none());
}

#[test]
fn outward_faces_and_missing_portal_numbers_are_refused() {
    let f = Fixture::new("piece-invalid");
    source(&f, true, "portal_0");
    let error = Session::open(&f.path().join("world.toml"), &f.dats(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("face inward"), "{error}");
    source(&f, false, "portal_2");
    let error = Session::open(&f.path().join("world.toml"), &f.dats(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("without gaps"), "{error}");
}

#[test]
fn a_missing_wall_triangle_is_refused_instead_of_leaving_an_unmarked_hole() {
    let f = Fixture::new("piece-hole");
    source(&f, false, "portal_0");
    let path = f.path().join("sample/art/hall.gltf");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc["accessors"][0]["count"] = json!(21);
    std::fs::write(path, doc.to_string()).unwrap();
    let error = Session::open(&f.path().join("world.toml"), &f.dats(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("closed cell"), "{error}");
}
