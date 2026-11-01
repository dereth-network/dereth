//! Visual descriptions, appearances and scenes.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.visual-desc.every-record-decodes
#[test]
fn every_visual_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::VISUAL_DESC]);
}

/// Behaviour: data.appearance.every-record-decodes
#[test]
fn every_appearance_table_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::APPEARANCE]);
}

/// Behaviour: data.scene.every-record-decodes
#[test]
fn every_scene_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SCENE]);
}

/// Every record a visual description, an appearance table or an effect table names, gathered as
/// the client gathers what a loaded record needs (parents, motion descriptions, behavior tables,
/// modes, setups, animation maps, appearance and effect tables, meshes, materials, modifiers,
/// pictures, sounds, particle systems), is in the data files, but one: appearance table
/// `0x200003A5` names material modifier `0x30000C87`, which no file holds.
///
/// Behaviour: data.visual-desc.every-record-a-look-names-is-there
#[test]
fn every_record_a_look_names_is_there() {
    use auberean_assets::database::GATHER_DEPENDENCIES;
    use auberean_assets::decode;
    use auberean_assets::fx::FxTable;
    use auberean_assets::visual::{Appearance, VisualDesc};
    use auberean_dat::QualifiedIds;

    for i in crate::common::installs() {
        let held = |id: u32| i.dats.iter().any(|(_, d)| d.entry(id).is_some());
        let (mut records, mut named) = (0u32, 0usize);
        let mut missing = Vec::new();
        for (name, dat) in &i.dats {
            if *name != auberean_dat::locate::DatName::Portal {
                continue;
            }
            for e in dat.entries() {
                let Ok(bytes) = dat.read_entry(e) else {
                    continue;
                };
                let mut q = QualifiedIds::new();
                match e.id >> 24 {
                    0x1F => decode::<VisualDesc>(e.id, &bytes, &i.ctx)
                        .map(|v| v.sub_ids(GATHER_DEPENDENCIES, &mut q)),
                    0x20 => decode::<Appearance>(e.id, &bytes, &i.ctx)
                        .map(|a| a.sub_ids(GATHER_DEPENDENCIES, &mut q)),
                    0x1E => decode::<FxTable>(e.id, &bytes, &i.ctx)
                        .map(|t| t.sub_ids(GATHER_DEPENDENCIES, &mut q)),
                    _ => continue,
                }
                .unwrap_or_else(|err| panic!("{}: {:08X}: {err}", i.set, e.id));
                records += 1;
                named += q.len();
                missing.extend(q.iter().filter(|n| !held(n.id)).map(|n| (e.id, n.id)));
            }
        }
        eprintln!(
            "{}: {records} descriptions and tables name {named} records",
            i.set
        );
        assert!(records > 8_000, "{}: {records} records", i.set);
        assert_eq!(missing, vec![(0x2000_03A5, 0x3000_0C87)], "{}", i.set);
    }
}
