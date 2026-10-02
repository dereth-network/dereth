//! The landscape: mesh construction, texturing, LOD, lighting, water and ordering.

pub mod lighting;
pub mod merge;
pub mod mesh;
pub mod order;
pub mod palshift;
pub mod stitch;
pub mod water;

#[cfg(any(test, feature = "test-support"))]
pub mod tests_support {
    //! Synthetic meshes the unit tests share, here and in the crates that draw this landscape.
    //! Nothing here is an oracle: every test that asserts fidelity names its behavioral evidence
    //! and uses these only to hold the data.

    use super::mesh::{cell_triangles, make_plane, Direction, LandPolygon, LandblockMesh};
    use super::water::WaterType;
    use dereth_primitives::{LandblockId, Vec3};

    /// A full-detail block of flat ground at height `z`, SW-to-NE cut everywhere.
    pub fn flat_mesh(z: f32) -> LandblockMesh {
        let svc = 9usize;
        let mut vertices = Vec::new();
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                vertices.push(Vec3::new(i as f32 * 24.0, j as f32 * 24.0, z));
            }
        }
        let mut polygons = Vec::new();
        for i in 0..8usize {
            for j in 0..8usize {
                for tri in cell_triangles(svc, i, j, true) {
                    polygons.push(LandPolygon {
                        v: tri,
                        plane: make_plane(
                            vertices[tri[0] as usize],
                            vertices[tri[1] as usize],
                            vertices[tri[2] as usize],
                        ),
                        surface: 0,
                        uv_indices: [0, 1, 2],
                        stippling: 0,
                    });
                }
            }
        }
        LandblockMesh {
            id: LandblockId(0),
            side_cell_count: 8,
            side_vertex_count: 9,
            trans_dir: Direction::InViewerBlock,
            vertices,
            sw_to_ne_cut: vec![true; 64],
            polygons,
            cell_keys: Vec::new(),
            cell_water: Vec::new(),
            water_type: WaterType::NotWater,
            colours: Vec::new(),
            max_zval: z + 200.0,
            min_zval: z - 1.0,
        }
    }
}
