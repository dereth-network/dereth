//! Every placed object of the September 2013 world, resolved to meshes.

use auberean_assets::mesh::Mesh;
use auberean_world::place::{entity_parts, DRAWN_KINDS};

use crate::common::need_world;

/// Every entity a landblock info places as a drawn object resolves, through its visual
/// descriptions or entity records and their setups, to at least one mesh, and every mesh it names
/// decodes.
///
/// Behaviour: world.objects.every-placed-object-resolves-to-meshes
#[test]
fn every_placed_object_resolves_to_meshes() {
    let w = need_world!();
    let (mut infos, mut drawn, mut parts) = (0usize, 0usize, 0usize);
    let mut unresolved = Vec::new();
    let mut bad_meshes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (x, y) in w.records.landblock_coordinates() {
        let Some(info) = w.records.landblock_info(x, y) else {
            continue;
        };
        infos += 1;
        for e in &info.entities.entities {
            if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) {
                continue;
            }
            drawn += 1;
            let p = entity_parts(&w.records, e);
            if p.is_empty() && unresolved.len() < 20 {
                unresolved.push(((x, y), e.object));
            }
            parts += p.len();
            for part in p {
                if seen.insert(part.mesh.0) && w.records.get::<Mesh>(part.mesh.0).is_none() {
                    bad_meshes.push(part.mesh.0);
                }
            }
        }
    }
    eprintln!(
        "{infos} landblock infos, {drawn} drawn entities, {parts} parts, {} distinct meshes",
        seen.len()
    );
    assert!(infos > 3_000, "the 2013 world has 3,036 landblock infos");
    assert!(
        unresolved.is_empty(),
        "entities with no mesh (block, object): {unresolved:?}"
    );
    assert!(
        bad_meshes.is_empty(),
        "meshes that do not decode: {bad_meshes:x?}"
    );
}

/// Block 0x3AA8's trees whose setups weigh their canopy to bones (`0x04000381`, `0x0400037F`) place
/// their trunk (a mesh that only collides: it has no drawn levels) at the tree's own frame, not on the
/// bone their resting pose puts it on (7.8 m and 2.4 m up): every part of such a tree stands where the
/// tree stands. Whether a mesh is weighted agrees with its decoded vertex format.
///
/// Behaviour: world.place.a-weighted-tree-stands-its-trunk-at-its-root
#[test]
fn a_weighted_tree_stands_its_trunk_at_its_root() {
    use auberean_world::place::PlaceRecords;
    let w = need_world!();
    let info = w.records.landblock_info(0x3A, 0xA8).expect("block 0x3AA8");
    let mut trees = 0usize;
    for e in &info.entities.entities {
        if !matches!(e.object, Some(0x4700_03BC | 0x4700_03BA)) {
            continue;
        }
        let at = e.offset.as_ref().expect("a placed tree").frame.origin;
        let parts = entity_parts(&w.records, e);
        assert_eq!(parts.len(), 2, "a canopy and a trunk");
        let trunk = w.records.get::<Mesh>(parts[1].mesh.0).expect("the trunk");
        assert!(trunk.levels.is_empty(), "the trunk is not drawn");
        let canopy = w.records.get::<Mesh>(parts[0].mesh.0).expect("the canopy");
        assert!(canopy.has_matrix_weights() && !trunk.has_matrix_weights());
        for p in &parts {
            let o = p.transform.0[3];
            let off =
                ((o[0] - at.x).powi(2) + (o[1] - at.y).powi(2) + (o[2] - at.z).powi(2)).sqrt();
            assert!(
                off < 1e-3,
                "{:08X} of {:08X?} drawn {off} m from the tree",
                p.mesh.0,
                e.object
            );
            let mesh = w.records.get::<Mesh>(p.mesh.0).expect("the mesh");
            assert_eq!(
                w.records.mesh_weighted(p.mesh),
                mesh.has_matrix_weights(),
                "{:08X}",
                p.mesh.0
            );
        }
        trees += 1;
    }
    assert!(trees >= 6, "block 0x3AA8 places such trees: {trees}");
}
