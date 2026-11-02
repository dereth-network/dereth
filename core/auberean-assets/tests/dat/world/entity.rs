//! Standalone entities.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.entity.every-record-decodes
#[test]
fn every_entity_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ENTITY]);
}

/// Every standalone entity's stored flags are the ones its fields call for (an id, kind, object
/// or version not 0, an instance, an offset off the identity, a scale off 1, a property group, a
/// name), and an entity is written with those, whatever flags it was read with.
///
/// Behaviour: data.entity.written-with-the-flags-its-fields-call-for
#[test]
fn every_entitys_flags_are_the_ones_its_fields_call_for() {
    use auberean_assets::entity::Entity;
    use auberean_assets::verify::type_in;
    use auberean_assets::{decode, encode};

    let mut n = 0;
    let mut differ = Vec::new();
    for i in crate::common::installs() {
        for (_, dat) in &i.dats {
            for e in dat.entries() {
                if type_in(dat, e.id) != Some(RecordType::ENTITY) {
                    continue;
                }
                let bytes = dat.read_entry(e).expect("read the record");
                let rec: Entity = decode(e.id, &bytes, &i.ctx).expect("an entity");
                n += 1;
                if rec.pack_flags() != rec.flags {
                    differ.push((e.id, rec.flags, rec.pack_flags()));
                }
            }
        }
    }
    assert!(n > 0, "no entity");
    assert!(
        differ.is_empty(),
        "{} of {n} differ: {:08X?}",
        differ.len(),
        &differ[..differ.len().min(8)]
    );

    let claims_all = Entity {
        flags: 0x1FF,
        object: Some(0x1F00_0001),
        version: Some(0),
        scale: Some(auberean_primitives::Vec3::new(1.0, 1.0001, 1.0)),
        name: Some(String::new()),
        ..Entity::default()
    };
    let bytes = encode(0x4700_0001, &claims_all);
    assert_eq!(&bytes[..8], &[0x08, 0, 0, 0, 0x01, 0, 0, 0x1F]);
    assert_eq!(bytes.len(), 8, "only the object follows the flags");
}
