//! Behaviour: none (a reading helper over decoded meshes: every vertex array yields its places)

use auberean_assets::decode;
use auberean_assets::mesh::Mesh;
use auberean_dat::locate::DatName;

use crate::common::installs;

#[test]
fn every_mesh_with_places_reads_one_place_a_vertex() {
    let mut meshes = 0;
    for i in installs() {
        let portal = &i
            .dats
            .iter()
            .find(|(n, _)| *n == DatName::Portal)
            .expect("portal")
            .1;
        for e in portal.entries_in(0x0600_0000, 0x06FF_FFFF) {
            let bytes = portal.read(e.id).expect("the record");
            let m: Mesh = decode(e.id, &bytes, &i.ctx).expect("decodes");
            if m.vertices.format & 0xE == 0 {
                continue;
            }
            let v = m
                .vertices
                .attributes()
                .unwrap_or_else(|| panic!("{:08X}: its vertices do not read", e.id));
            assert_eq!(v.len(), m.vertices.count as usize, "{:08X}", e.id);
            meshes += 1;
        }
    }
    assert!(meshes > 1000, "only {meshes} meshes");
}
