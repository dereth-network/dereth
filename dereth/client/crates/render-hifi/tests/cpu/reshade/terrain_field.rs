//! The landscape field agrees with the landscape: its diagonal is the landscape's own, in the
//! shader as on the CPU, and its height at any point of a full-detail block is the height of the
//! drawn triangle there.

use dereth_render_hifi::reshade;
use dereth_render_hifi::shared::noise::pcg;
use dereth_render_hifi::shared::terrain_field::{
    cell_plane_height, f16_bits, split_diagonal, GroundClass, TerrainField, BLOCK_METRES,
};
use dereth_render_hifi::snapshot::HifiTerrainBlock;
use dereth_terrain::land::mesh::{adj_planes, cell_triangles, plane_set_height, sw_to_ne_cut};
use dereth_terrain::land::tests_support::flat_mesh;
use dereth_terrain::scenery::find_terrain_poly;
use glam::{Vec2, Vec3};

use crate::common::wgsl_eval::{Evaluator, Value};

/// A height for global landscape vertex `(x, y)`, the same whichever block asks: a rolling
/// 60 to 100 m, rising or falling up to 40 m between neighbouring vertices, as steep as the
/// landscape's hillsides get.
fn height_at(x: i32, y: i32) -> f32 {
    let h = pcg(x.cast_unsigned().wrapping_mul(7919) ^ pcg(y.cast_unsigned()));
    #[allow(clippy::cast_precision_loss)]
    let h = 60.0 + (h % 4_000) as f32 / 100.0;
    h
}

/// A pseudo-random fraction in 0..1 from `seed`.
fn unit(seed: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let f = (pcg(seed) >> 8) as f32 / (1u32 << 24) as f32;
    f
}

/// The full-detail block at `(bx, by)`: its mesh as the landscape builds it (vertices, the
/// landscape's own cuts, the drawn triangles and their planes) and its snapshot, placed in
/// render space with the block at `first` at `origin`.
fn block(
    bx: u8,
    by: u8,
    first: (u8, u8),
    origin: Vec2,
) -> (dereth_terrain::land::mesh::LandblockMesh, HifiTerrainBlock) {
    let mut mesh = flat_mesh(0.0);
    let (gx, gy) = (i32::from(bx) * 8, i32::from(by) * 8);
    for a in 0..9usize {
        for b in 0..9usize {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let z = height_at(gx + a as i32, gy + b as i32);
            mesh.vertices[a * 9 + b].z = z;
        }
    }
    for i in 0..8usize {
        for j in 0..8usize {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let cut = sw_to_ne_cut(gx + i as i32, gy + j as i32);
            mesh.sw_to_ne_cut[i * 8 + j] = cut;
            let tris = cell_triangles(9, i, j, cut);
            let n = i * 8 + j;
            mesh.polygons[2 * n].v = tris[0];
            mesh.polygons[2 * n + 1].v = tris[1];
        }
    }
    adj_planes(&mut mesh);
    let shift = Vec2::new(
        f32::from(bx - first.0) * BLOCK_METRES,
        f32::from(by - first.1) * BLOCK_METRES,
    );
    let mut words = [0u16; 81];
    for (k, w) in words.iter_mut().enumerate() {
        // Grassland, with a road through every ninth vertex.
        *w = (1 << 2) | u16::from(k % 9 == 4);
    }
    let snapshot = HifiTerrainBlock {
        block: (u16::from(bx) << 8) | u16::from(by),
        ring: 0,
        origin: (origin + shift).extend(0.0),
        words,
        side: 9,
        heights: mesh.vertices.iter().map(|v| v.z).collect(),
        cuts: mesh.sw_to_ne_cut.clone(),
        generation: 1,
    };
    (mesh, snapshot)
}

/// Behaviour: hifi.terrain-field.the-diagonal-is-the-landscapes-own
/// The field's diagonal, on the CPU and as the shader computes it, is the landscape's own for
/// cells all over the map and at the edges of the coordinate range.
#[test]
fn the_field_diagonal_is_the_landscape_diagonal_in_the_shader_and_on_the_cpu() {
    let shader = Evaluator::new(&reshade::shaders()[1]);
    let mut cells: Vec<(i32, i32)> = (0..2048)
        .step_by(37)
        .flat_map(|x| (0..2048).step_by(41).map(move |y| (x, y)))
        .collect();
    cells.extend([(0, 0), (2039, 2039), (-1, -1), (i32::MAX, 3), (5, i32::MIN)]);
    let mut cuts = [0usize; 2];
    for (x, y) in cells {
        let want = sw_to_ne_cut(x, y);
        assert_eq!(split_diagonal(x, y), want, "cpu ({x}, {y})");
        let got = shader
            .call(
                "split_diagonal",
                &[Value::Vector(vec![Value::I32(x), Value::I32(y)])],
            )
            .bool();
        assert_eq!(got, want, "shader ({x}, {y})");
        cuts[usize::from(want)] += 1;
    }
    assert!(
        cuts[0] > 100 && cuts[1] > 100,
        "both diagonals were met: {cuts:?}"
    );
}

/// Behaviour: hifi.terrain-field.the-plane-height-is-the-drawn-landscapes
/// At thousands of points over a window of four full-detail blocks, the field's height and the
/// shader's cell height are the height of the drawn triangle the point lies in, to 0.1 mm.
#[test]
fn the_field_height_is_the_drawn_triangle_height_to_a_tenth_of_a_millimetre() {
    let shader = Evaluator::new(&reshade::shaders()[1]);
    let first = (0x20u8, 0x30u8);
    // Render space keeps the viewer's block near the origin.
    let origin = Vec2::new(-150.0, -96.0);
    let blocks: Vec<_> = [(0x20, 0x30), (0x21, 0x30), (0x20, 0x31), (0x21, 0x31)]
        .into_iter()
        .map(|(x, y)| block(x, y, first, origin))
        .collect();
    let snapshots: Vec<HifiTerrainBlock> = blocks.iter().map(|(_, s)| s.clone()).collect();
    let field = TerrainField::build(&snapshots).expect("a field");
    assert_eq!(field.size, (17, 17));
    assert_eq!(field.origin, origin);
    let mut checked = 0;
    let mut worst = 0.0f32;
    for (k, (mesh, snap)) in blocks.iter().enumerate() {
        for s in 0..1500u32 {
            let seed = u32::try_from(k).unwrap_or(0) * 10_000 + s * 2;
            let p = Vec2::new(unit(seed), unit(seed + 1)) * BLOCK_METRES;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, j) = ((p.x / 24.0) as usize, (p.y / 24.0) as usize);
            let (i, j) = (i.min(7), j.min(7));
            let cell = u16::try_from(i * 8 + j + 1).unwrap_or(0);
            let Some(plane) = find_terrain_poly(mesh, cell, p.x, p.y) else {
                continue;
            };
            let want = plane_set_height(plane, p.x, p.y).expect("the ground is not vertical");
            let at = snap.origin.truncate() + p;
            let got = field.plane_height(at.x, at.y);
            worst = worst.max((got - want).abs());
            assert!(
                (got - want).abs() <= 1e-4,
                "cpu at {p:?} of block {k}: {got} against {want}"
            );
            // The shader's cell height from the same corners and diagonal.
            let v = |a: usize, b: usize| mesh.vertices[a * 9 + b].z;
            #[allow(clippy::cast_precision_loss)]
            let f = (p - Vec2::new(i as f32, j as f32) * 24.0) / 24.0;
            let corners = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
            let cut = mesh.sw_to_ne_cut[i * 8 + j];
            let shaded = shader
                .call(
                    "cell_plane_height",
                    &[
                        Value::Vector(corners.iter().map(|h| Value::F32(*h)).collect()),
                        Value::Vector(vec![Value::F32(f.x), Value::F32(f.y)]),
                        Value::Bool(cut),
                    ],
                )
                .f32();
            assert!(
                (shaded - want).abs() <= 1e-4,
                "shader at {p:?}: {shaded} against {want}"
            );
            assert_eq!(
                cell_plane_height(corners, f, cut).to_bits(),
                shaded.to_bits()
            );
            checked += 1;
        }
    }
    assert!(checked > 5000, "{checked} points checked");
    eprintln!("{checked} points, worst {worst} m");
}

/// Behaviour: hifi.terrain-field.the-field-is-seamless-across-blocks
/// Where blocks meet, the field holds one height per landscape vertex, a full-detail block's
/// over a coarser neighbour's; a coarser block's vertices lie on the triangles it draws; and the
/// classes and normals come from the terrain words and the heights.
#[test]
fn the_field_holds_one_height_per_vertex_and_the_finer_block_wins_an_edge() {
    let first = (0x40u8, 0x40u8);
    let origin = Vec2::ZERO;
    let (_, fine) = block(0x40, 0x40, first, origin);
    // A ring-one neighbour to the east: 4 cells a side, its own corner heights.
    let side = 5usize;
    let mut heights = Vec::new();
    for a in 0..side {
        for b in 0..side {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            heights.push(height_at(0x41 * 8 + 2 * a as i32, 0x40 * 8 + 2 * b as i32) + 5.0);
        }
    }
    let coarse = HifiTerrainBlock {
        block: 0x4140,
        ring: 1,
        origin: Vec3::new(BLOCK_METRES, 0.0, 0.0),
        words: [(18 << 2); 81],
        side: 5,
        heights: heights.clone(),
        cuts: vec![false; 16],
        generation: 3,
    };
    for order in [
        [fine.clone(), coarse.clone()],
        [coarse.clone(), fine.clone()],
    ] {
        let field = TerrainField::build(&order).expect("a field");
        assert_eq!(field.size, (17, 9));
        let at = |x: usize, y: usize| field.heights[y * 17 + x];
        for n in 0..9 {
            assert_eq!(at(8, n), fine.heights[8 * 9 + n], "the shared edge at {n}");
            assert_eq!(
                field.classes[n * 17 + 8],
                GroundClass::of_word(fine.words[8 * 9 + n])
            );
            assert_eq!(field.classes[n * 17 + 12], GroundClass::Water);
        }
        // The coarse block's own vertices, away from the shared edge, are its corners.
        assert_eq!(at(16, 8), heights[4 * side + 4]);
        assert_eq!(at(10, 0), heights[side]);
        // Between them, the coarse triangle (cut south-east to north-west).
        let mid = cell_plane_height(
            [
                heights[side],
                heights[2 * side],
                heights[2 * side + 1],
                heights[side + 1],
            ],
            Vec2::new(0.5, 0.5),
            false,
        );
        assert!((at(11, 1) - mid).abs() < 1e-4);
        let n = field.normals[4 * 17 + 4];
        assert!(n[0] * n[0] + n[1] * n[1] < 1.0);
    }
    assert_eq!(
        TerrainField::signature(&[fine.clone(), coarse.clone()]),
        TerrainField::signature(&[coarse.clone(), fine.clone()]),
        "the signature does not depend on the order"
    );
    let mut moved = fine.clone();
    moved.origin.x += BLOCK_METRES;
    assert_ne!(
        TerrainField::signature(&[fine, coarse.clone()]),
        TerrainField::signature(&[moved, coarse]),
        "the window moving changes the signature"
    );
}

/// Behaviour: hifi.terrain-field.the-field-is-seamless-across-blocks
/// The classes follow the terrain types: roads first, the five water types as water, and the
/// half-precision normals are what a device reads back.
#[test]
fn the_ground_classes_follow_the_terrain_types_and_the_normals_encode_as_half_floats() {
    for t in 16..=20u8 {
        assert_eq!(GroundClass::of_type(t), GroundClass::Water, "type {t}");
    }
    for t in [1u8, 3, 9] {
        assert_eq!(GroundClass::of_type(t), GroundClass::Grass);
    }
    assert_eq!(GroundClass::of_type(15), GroundClass::Snow);
    assert_eq!(GroundClass::of_word((17 << 2) | 1), GroundClass::Road);
    assert_eq!(GroundClass::of_word(17 << 2), GroundClass::Water);
    for (v, bits) in [
        (0.0f32, 0x0000u16),
        (1.0, 0x3C00),
        (-2.0, 0xC000),
        (0.5, 0x3800),
        (65504.0, 0x7BFF),
        (1e6, 0x7C00),
        (0.333_333_34, 0x3555),
        (6.0e-8, 0x0001),
    ] {
        assert_eq!(f16_bits(v), bits, "{v}");
    }
}
