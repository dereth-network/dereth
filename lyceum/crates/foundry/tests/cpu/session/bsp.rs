//! The trees the pack builds for a new model or piece, asked what the client asks of them.

use dereth_assets::{BspTree, GfxObj, Polygon, SwVertex, VertexArray};
use dereth_physics::{Sphere, V3};
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::bsp;

/// The corners of a box 2 m on a side standing on the origin, numbered by their x, y and z bits.
fn box_corners() -> Vec<Vec3> {
    (0..8)
        .map(|i| {
            let bit = |b: u32| if i & (1 << b) == 0 { 0.0_f32 } else { 1.0 };
            Vec3::new(
                bit(0).mul_add(2.0, -1.0),
                bit(1).mul_add(2.0, -1.0),
                2.0 * bit(2),
            )
        })
        .collect()
}

/// The box's faces, each wound to face out: bottom, top, +x, -x, +y, -y.
const BOX_FACES: [[u16; 4]; 6] = [
    [0, 2, 3, 1],
    [4, 5, 7, 6],
    [1, 3, 7, 5],
    [0, 4, 6, 2],
    [2, 6, 7, 3],
    [0, 1, 5, 4],
];

fn mesh(corners: &[Vec3], faces: &[Vec<u16>]) -> (VertexArray, Vec<Polygon>) {
    let va = VertexArray {
        vertex_type: 1,
        vertices: corners
            .iter()
            .enumerate()
            .map(|(i, p)| SwVertex {
                id: i16::try_from(i).unwrap(),
                position: *p,
                normal: Vec3::ZERO,
                uvs: Vec::new(),
            })
            .collect(),
    };
    let polys = faces
        .iter()
        .enumerate()
        .map(|(i, f)| Polygon {
            poly_id: i16::try_from(i).unwrap(),
            num_pts: u8::try_from(f.len()).unwrap(),
            stippling: 0,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: f.clone(),
            pos_uv_indices: None,
            neg_uv_indices: None,
        })
        .collect();
    (va, polys)
}

fn solid_box(corners: &[Vec3]) -> (VertexArray, Vec<Polygon>) {
    mesh(
        corners,
        &BOX_FACES.iter().map(|f| f.to_vec()).collect::<Vec<_>>(),
    )
}

/// The box as a room: the same faces, facing in.
fn room(corners: &[Vec3], faces: &[Vec<u16>]) -> (VertexArray, Vec<Polygon>) {
    mesh(
        corners,
        &faces
            .iter()
            .map(|f| f.iter().rev().copied().collect())
            .collect::<Vec<_>>(),
    )
}

/// A tree as the client walks it.
fn walked(va: &VertexArray, polys: &[Polygon], tree: &BspTree) -> dereth_physics::BspTree {
    let g = GfxObj {
        id: DataId(0x0100_0000),
        flags: 1,
        surfaces: Vec::new(),
        vertex_array: va.clone(),
        physics_polygons: polys.to_vec(),
        physics_bsp: Some(tree.clone()),
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    dereth_world_data::env_cells::gfxobj_physics_bsp(&g).expect("a tree")
}

/// Points round the box from 30 cm to 2 m outside it, none inside.
fn around_the_box() -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in -6..=6 {
        for j in -6..=6 {
            for k in -4..=10 {
                #[allow(clippy::cast_precision_loss)]
                let p = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, k as f32 * 0.5);
                let gap = (p.x.abs() - 1.0)
                    .max(p.y.abs() - 1.0)
                    .max((p.z - 1.0).abs() - 1.0);
                if (0.3..=2.0).contains(&gap) {
                    out.push(p);
                }
            }
        }
    }
    out
}

#[test]
fn a_box_is_solid_inside_open_outside_and_a_sweep_into_a_face_hits_that_face() {
    let (va, polys) = solid_box(&box_corners());
    let tree = bsp::physics(&va, &polys, true);
    let t = walked(&va, &polys, &tree);
    assert!(t.interior_nodes_have_both_children());
    sweeps_miss_nothing(&t);
    assert!(t.sphere_intersects_solid(&Sphere::new(Vec3::new(0.2, -0.3, 1.1), 0.1), true));
    for p in around_the_box() {
        assert!(
            !t.sphere_intersects_solid(&Sphere::new(p, 0.1), true),
            "{p:?}"
        );
    }
    // Into the +x face, and along it.
    let s = Sphere::new(Vec3::new(1.3, 0.2, 1.0), 0.5);
    let (mut hit, mut contact) = (None, Vec3::ZERO);
    assert!(t.sphere_intersects_poly(&s, Vec3::new(-0.1, 0.0, 0.0), &mut hit, &mut contact));
    assert_eq!(hit, Some(2));
    // Standing on top.
    let on_top = Sphere::new(Vec3::new(0.1, 0.1, 2.4), 0.5);
    assert!(t.hits_walkable(&on_top, Vec3::new(0.0, 0.0, 1.0), 0.66));
}

/// Spheres round and through the box, each moving a little some way.
fn sweeps() -> Vec<(Sphere, Vec3)> {
    let mut out = Vec::new();
    let mut seed = 0x2545_F491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        #[allow(clippy::cast_precision_loss)]
        let r = (seed >> 8) as f32 / (1u32 << 24) as f32;
        r
    };
    for _ in 0..4000 {
        let c = Vec3::new(4.0 * next() - 2.0, 4.0 * next() - 2.0, 4.0 * next() - 1.0);
        let m = Vec3::new(next() - 0.5, next() - 0.5, next() - 0.5);
        out.push((Sphere::new(c, 0.05 + 0.6 * next()), m));
    }
    out
}

/// Every sweep that some polygon stops, the tree stops, naming a polygon it hits.
fn sweeps_miss_nothing(t: &dereth_physics::BspTree) {
    for (s, m) in sweeps() {
        let any = t.polygons.iter().any(|p| {
            let mut c = Vec3::ZERO;
            p.pos_hits_sphere_parts(&s, m, &mut c).1
        });
        let (mut hit, mut contact) = (None, Vec3::ZERO);
        assert_eq!(
            t.sphere_intersects_poly(&s, m, &mut hit, &mut contact),
            any,
            "{s:?} {m:?}"
        );
        if any {
            let mut c = Vec3::ZERO;
            assert!(
                t.polygons[hit.unwrap() as usize]
                    .pos_hits_sphere_parts(&s, m, &mut c)
                    .1
            );
        }
    }
}

#[test]
fn a_lumpy_box_whose_faces_are_none_quite_flat_is_solid_only_inside_and_misses_no_sweep() {
    // Every corner moved up to 4 mm, so no face is flat and each reaches a little past its
    // neighbors' planes.
    let mut corners = box_corners();
    let lumps = [
        [3, -2, 4],
        [-4, 1, -3],
        [2, 4, -1],
        [-1, -3, 2],
        [4, 2, -4],
        [-3, 4, 1],
        [1, -4, 3],
        [-2, 3, -2],
    ];
    for (c, lump) in corners.iter_mut().zip(lumps) {
        let mm = |k: i32| f32::from(i8::try_from(k).unwrap()) * 0.001;
        *c = c.add(Vec3::new(mm(lump[0]), mm(lump[1]), mm(lump[2])));
    }
    let (va, polys) = solid_box(&corners);
    let t = walked(&va, &polys, &bsp::physics(&va, &polys, true));
    assert!(t.sphere_intersects_solid(&Sphere::new(Vec3::new(0.0, 0.0, 1.0), 0.1), true));
    for p in around_the_box() {
        assert!(
            !t.sphere_intersects_solid(&Sphere::new(p, 0.1), true),
            "{p:?}"
        );
    }
    sweeps_miss_nothing(&t);
}

#[test]
fn a_box_with_a_bent_face_is_still_open_everywhere_outside_it() {
    // One top corner raised 4 mm: the top and the two sides meeting there are not quite flat, and
    // each reaches a little past the others' planes.
    let mut corners = box_corners();
    corners[7].z += 0.004;
    let (va, polys) = solid_box(&corners);
    let t = walked(&va, &polys, &bsp::physics(&va, &polys, true));
    assert!(t.sphere_intersects_solid(&Sphere::new(Vec3::new(0.0, 0.0, 1.0), 0.1), true));
    for p in around_the_box() {
        assert!(
            !t.sphere_intersects_solid(&Sphere::new(p, 0.1), true),
            "{p:?}"
        );
    }
    // Every face still found by a sweep near its bent corner.
    for (face, from, toward) in [
        (1, Vec3::new(0.9, 0.9, 2.3), Vec3::new(0.0, 0.0, -0.1)),
        (2, Vec3::new(1.3, 0.9, 1.9), Vec3::new(-0.1, 0.0, 0.0)),
        (4, Vec3::new(0.9, 1.3, 1.9), Vec3::new(0.0, -0.1, 0.0)),
    ] {
        let (mut hit, mut contact) = (None, Vec3::ZERO);
        assert!(t.sphere_intersects_poly(&Sphere::new(from, 0.35), toward, &mut hit, &mut contact));
        assert_eq!(hit, Some(face), "{from:?}");
    }
}

#[test]
fn a_rooms_cell_is_the_box_its_walls_make() {
    let faces: Vec<Vec<u16>> = BOX_FACES.iter().map(|f| f.to_vec()).collect();
    let (va, polys) = room(&box_corners(), &faces);
    let tree = bsp::cell(&va, &polys);
    assert_eq!(tree.nodes.len(), 6);
    let t = walked(&va, &polys, &tree);
    assert!(t.point_inside_cell_bsp(Vec3::new(0.9, -0.9, 1.9)));
    assert!(!t.point_inside_cell_bsp(Vec3::new(1.1, 0.0, 1.0)));
    assert!(!t.point_inside_cell_bsp(Vec3::new(0.0, 0.0, -0.1)));
}

#[test]
fn a_wall_of_two_polygons_not_quite_in_line_is_one_face_of_the_cell() {
    // The +x wall in two halves meeting 1.4 mm outside the line of the wall's edges.
    let mut corners = box_corners();
    corners.push(Vec3::new(1.0014, -1.0, 1.0));
    corners.push(Vec3::new(1.0014, 1.0, 1.0));
    let mut faces: Vec<Vec<u16>> = BOX_FACES.iter().map(|f| f.to_vec()).collect();
    faces[2] = vec![1, 3, 9, 8];
    faces.push(vec![8, 9, 7, 5]);
    let (va, polys) = room(&corners, &faces);
    let tree = bsp::cell(&va, &polys);
    assert_eq!(tree.nodes.len(), 6);
    let t = walked(&va, &polys, &tree);
    assert!(t.point_inside_cell_bsp(Vec3::new(0.999, 0.0, 1.0)));
    assert!(!t.point_inside_cell_bsp(Vec3::new(1.01, 0.0, 1.0)));
}

#[test]
fn an_l_shaped_rooms_cell_is_the_smallest_convex_shape_round_it() {
    // A room 4 m by 4 m with the +x+y quarter taken out.
    let at = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let floor = [
        at(-2.0, -2.0, 0.0),
        at(2.0, -2.0, 0.0),
        at(2.0, 0.0, 0.0),
        at(0.0, 0.0, 0.0),
        at(0.0, 2.0, 0.0),
        at(-2.0, 2.0, 0.0),
    ];
    let mut corners: Vec<Vec3> = floor.to_vec();
    corners.extend(floor.iter().map(|p| p.add(Vec3::new(0.0, 0.0, 3.0))));
    // Facing in: the floor up, the ceiling down, each wall toward the middle.
    let mut faces: Vec<Vec<u16>> = vec![vec![0, 1, 2, 3, 4, 5], vec![11, 10, 9, 8, 7, 6]];
    for k in 0..6u16 {
        let n = (k + 1) % 6;
        faces.push(vec![k, k + 6, n + 6, n]);
    }
    let (va, polys) = mesh(&corners, &faces);
    let t = walked(&va, &polys, &bsp::cell(&va, &polys));
    assert!(t.point_inside_cell_bsp(at(-1.0, -1.0, 1.0)));
    // In the quarter taken out, but inside the shape round the room, as the client's cells are.
    assert!(t.point_inside_cell_bsp(at(0.5, 0.5, 1.0)));
    assert!(!t.point_inside_cell_bsp(at(1.5, 1.5, 1.0)));
    assert!(!t.point_inside_cell_bsp(at(-1.0, -1.0, 3.1)));
}

#[test]
fn a_drawing_tree_lists_every_polygon_once_inside_its_root_sphere() {
    let (va, polys) = solid_box(&box_corners());
    let tree = bsp::drawing(&va, &polys);
    let mut listed = vec![0; polys.len()];
    for n in &tree.nodes {
        for p in &n.in_polys {
            listed[usize::try_from(*p).unwrap()] += 1;
        }
    }
    assert_eq!(listed, vec![1; 6]);
    let root = tree.nodes[0].sphere.expect("a root sphere");
    for c in box_corners() {
        assert!(c.sub(root.center).dot(c.sub(root.center)).sqrt() <= root.radius);
    }
}
