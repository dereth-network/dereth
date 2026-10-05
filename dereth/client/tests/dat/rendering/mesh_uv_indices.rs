//! The mesh builder's absent-index-array branch, for cell and object meshes alike.
//! A missing corner-index array selects vertex UV index zero. Copying that vertex then uses its UV
//! when present and substitutes zero for an invalid index.

use dereth_assets::common::{BspTree, Polygon, SwVertex, VertexArray};
use dereth_assets::geometry::CellStruct;
use dereth_assets::{Decode, Environment, GfxObj};
use dereth_dat::DbType;
use dereth_primitives::{DataId, Vec3};
use {dereth_client_runtime::models::triangulate, dereth_client_runtime::models::triangulate_cell};

/// Behaviour: rendering.mesh.a-polygon-without-uv-indices-uses-each-vertexs-first-uv
#[test]
fn absent_cell_uv_indices_select_vertex_uv_zero() {
    let cs = cell_fixture();
    assert_both_mesh_consumers(&cs, &[(0.25, 0.75), (1.25, 0.75), (2.25, 0.75)]);
}

#[test]
fn explicit_uv_indices_remain_per_corner_and_missing_vertex_uvs_are_zero() {
    let mut cs = cell_fixture();
    cs.polygons[0].pos_uv_indices = Some(vec![1, 0, 1]);
    assert_both_mesh_consumers(&cs, &[(0.5, 0.125), (1.25, 0.75), (0.5, 0.125)]);
    cs.vertex_array.vertices[1].uvs.clear();
    assert_both_mesh_consumers(&cs, &[(0.5, 0.125), (0.0, 0.0), (0.5, 0.125)]);
    cs.polygons[0].pos_uv_indices = None;
    assert_both_mesh_consumers(&cs, &[(0.25, 0.75), (0.0, 0.0), (2.25, 0.75)]);
}

#[test]
fn invalid_and_signed_negative_indices_do_not_fall_back_to_uv_zero() {
    let mut cs = cell_fixture();
    // 0x80 must remain negative even if an unusually large UV array has an entry128.
    cs.vertex_array.vertices[1].uvs.resize(129, (0.625, 0.875));
    cs.polygons[0].pos_uv_indices = Some(vec![2, 128, 255]);
    assert_both_mesh_consumers(&cs, &[(0.0, 0.0); 3]);
    // A malformed/truncated non-null array retains the existing safe zero fallback.
    cs.polygons[0].pos_uv_indices = Some(vec![1]);
    assert_both_mesh_consumers(&cs, &[(0.5, 0.125), (0.0, 0.0), (0.0, 0.0)]);
}

fn cell_fixture() -> CellStruct {
    CellStruct {
        cellstruct_id: 0,
        vertex_array: VertexArray {
            vertex_type: 1,
            vertices: (0..3)
                .map(|i| SwVertex {
                    id: i,
                    position: Vec3::new(f32::from(i), 0.0, 0.0),
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uvs: vec![(0.25 + f32::from(i), 0.75), (0.5, 0.125)],
                })
                .collect(),
        },
        polygons: vec![Polygon {
            poly_id: 0,
            num_pts: 3,
            stippling: 4,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: vec![0, 1, 2],
            pos_uv_indices: None,
            neg_uv_indices: None,
        }],
        portals: vec![],
        cell_bsp: BspTree::default(),
        physics_polygons: vec![],
        physics_bsp: BspTree::default(),
        drawing_bsp: None,
    }
}

fn assert_both_mesh_consumers(cs: &CellStruct, expected: &[(f32, f32)]) {
    let surface = DataId(0x0800_0001);
    let object = GfxObj {
        id: DataId(0x0100_0001),
        flags: 2,
        surfaces: vec![surface],
        vertex_array: cs.vertex_array.clone(),
        physics_polygons: vec![],
        physics_bsp: None,
        sort_center: Vec3::ZERO,
        polygons: cs.polygons.clone(),
        drawing_bsp: None,
        did_degrade: None,
    };
    for groups in [triangulate_cell(cs, &[surface]), triangulate(&object)] {
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].surface, Some(surface));
        assert!(!groups[0].two_sided);
        assert!(!groups[0].tiled);
        let got: Vec<_> = groups[0].vertices.iter().map(|v| (v.1, v.2)).collect();
        assert_eq!(got, expected);
        let positions: Vec<_> = groups[0].vertices.iter().map(|v| v.0).collect();
        let original: Vec<_> = cs
            .vertex_array
            .vertices
            .iter()
            .map(|v| v.position)
            .collect();
        assert_eq!(positions, original);
    }
}

// The installed DATs must actually exercise the absent-array branch. Compare its production
// result with an explicit zero-index array, which mesh construction treats identically. Neither the
// DAT bytes nor their polygon/vertex/surface membership are changed on disk.
#[test]
fn retail_absent_uv_arrays_match_explicit_zero_indices() {
    let store = dereth_dat::testing::open_store().expect("retail DATs required");
    let mut objects = 0;
    let mut cells = 0;
    for id in store.ids_of(DbType::GfxObj) {
        let bytes = store.read_typed(DbType::GfxObj, id).unwrap();
        let obj = GfxObj::decode_payload(id, &bytes).unwrap();
        if !has_nonzero_implicit_uv(&obj.polygons, &obj.vertex_array) {
            continue;
        }
        objects += 1;
        let mut explicit = obj.clone();
        explicit_zero_indices(&mut explicit.polygons);
        let actual = triangulate(&obj);
        let expected = triangulate(&explicit);
        assert_eq!(actual.len(), expected.len(), "{id} group count");
        for (a, e) in actual.iter().zip(&expected) {
            assert_eq!(a.vertices, e.vertices, "{id} absent UV array");
        }
    }
    for id in store.ids_of(DbType::Environment) {
        let bytes = store.read_typed(DbType::Environment, id).unwrap();
        let env = Environment::decode_payload(id, &bytes).unwrap();
        for cs in env.cells {
            if !has_nonzero_implicit_uv(&cs.polygons, &cs.vertex_array) {
                continue;
            }
            cells += 1;
            let mut explicit = cs.clone();
            explicit_zero_indices(&mut explicit.polygons);
            let actual = triangulate_cell(&cs, &[]);
            let expected = triangulate_cell(&explicit, &[]);
            assert_eq!(actual.len(), expected.len(), "{id} group count");
            for (a, e) in actual.iter().zip(&expected) {
                assert_eq!(
                    a.vertices, e.vertices,
                    "{id} cell {} absent UV array",
                    cs.cellstruct_id
                );
            }
        }
    }
    eprintln!("nonzero implicit UV0: {objects} GfxObjs, {cells} CellStructs");
    assert!(
        objects + cells > 0,
        "DAT oracle must exercise nonzero implicit UV0"
    );
}

fn has_nonzero_implicit_uv(polygons: &[Polygon], va: &VertexArray) -> bool {
    polygons.iter().any(|p| {
        p.num_pts >= 3
            && p.pos_uv_indices.is_none()
            && p.vertex_ids.iter().any(|&id| {
                va.vertices[id as usize]
                    .uvs
                    .first()
                    .is_some_and(|&uv| uv != (0.0, 0.0))
            })
    })
}

fn explicit_zero_indices(polygons: &mut [Polygon]) {
    for p in polygons {
        if p.pos_uv_indices.is_none() {
            p.pos_uv_indices = Some(vec![0; usize::from(p.num_pts)]);
        }
    }
}
