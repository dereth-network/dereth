//! Meshes, cell meshes and their physics parts.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.mesh.every-record-decodes
#[test]
fn every_mesh_decodes_exactly_with_its_polygons_and_trees() {
    assert_every_record_decodes(&[RecordType::MESH]);
}

/// Behaviour: data.cell-mesh.every-record-decodes
#[test]
fn every_cell_mesh_decodes_exactly_with_its_tree_and_portals() {
    assert_every_record_decodes(&[RecordType::CELL_MESH]);
}
