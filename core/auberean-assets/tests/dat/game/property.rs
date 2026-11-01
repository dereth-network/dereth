//! The property list, the records that are only properties, and how the game finds a record's
//! properties.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::entity::Entity;
use auberean_assets::landblock::{Cell, LandblockInfo, LightInfo};
use auberean_assets::property::{PhysicsMaterial, PropertyCollection, PropertyList, PropertyValue};
use auberean_assets::verify::type_in;
use auberean_assets::{decode, Decode};
use auberean_dat::{type_of_id, RecordType};

use crate::common::{assert_every_record_decodes, installs, Install};

/// Behaviour: data.property-list.every-record-decodes
#[test]
fn the_property_list_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::PROPERTY_LIST]);
}

/// Behaviour: data.physics-material.every-record-decodes
#[test]
fn every_physics_material_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::PHYSICS_MATERIAL]);
}

/// Every record of a type, decoded, with its id.
fn records<T: Decode>(i: &Install, ty: RecordType) -> Vec<(u32, T)> {
    let mut v = Vec::new();
    for (_, dat) in &i.dats {
        for e in dat.entries() {
            if type_in(dat, e.id) != Some(ty) {
                continue;
            }
            let bytes = dat.read_entry(e).expect("read the record");
            v.push((e.id, decode(e.id, &bytes, &i.ctx).expect("decode")));
        }
    }
    v
}

/// Every property collection a record of the checked set holds: entities', landblock infos'
/// (theirs and their entities'), lights', cells' (theirs and their entities') and physics
/// materials'.
fn collections(i: &Install) -> Vec<(u32, PropertyCollection)> {
    let mut out = Vec::new();
    let entity = |id: u32, e: &Entity, out: &mut Vec<(u32, PropertyCollection)>| {
        if let Some(p) = &e.properties {
            out.push((id, p.clone()));
        }
    };
    for (id, e) in records::<Entity>(i, RecordType::ENTITY) {
        entity(id, &e, &mut out);
    }
    for (id, l) in records::<LandblockInfo>(i, RecordType::LANDBLOCK_INFO) {
        out.push((id, l.properties.clone()));
        for e in &l.entities.entities {
            entity(id, e, &mut out);
        }
    }
    for (id, l) in records::<LightInfo>(i, RecordType::LIGHT_INFO) {
        for s in l.lights {
            out.push((id, s.properties));
        }
    }
    for (id, c) in records::<Cell>(i, RecordType::CELL) {
        for e in c.entities.iter().flat_map(|g| g.entities.iter()) {
            entity(id, e, &mut out);
        }
        if let Some(p) = c.properties {
            out.push((id, p));
        }
    }
    for (id, m) in records::<PhysicsMaterial>(i, RecordType::PHYSICS_MATERIAL) {
        out.push((id, m.properties));
    }
    out
}

fn list(i: &Install) -> &PropertyList {
    i.ctx.properties().expect("the property list")
}

/// Behaviour: data.property.found-through-its-listed-group
#[test]
fn a_stored_property_is_found_only_in_the_group_its_list_names() {
    for i in installs() {
        let list = list(i);
        let (mut n, mut hidden) = (0, 0);
        for (id, c) in &collections(i) {
            for g in &c.groups {
                for p in &g.properties {
                    n += 1;
                    assert_eq!(Some(p.group), list.group(p.id));
                    if list.group(p.id) == Some(g.group) {
                        assert_eq!(c.get(list, p.id), Some(p), "{id:08X} {:#x}", p.id);
                    } else {
                        // Stored under another group than the list's: the game's lookup, which
                        // goes through the listed group, does not see it.
                        hidden += 1;
                        let seen = c.get(list, p.id).is_some_and(|f| std::ptr::eq(f, p));
                        assert!(!seen, "{id:08X} {:#x}", p.id);
                    }
                }
            }
        }
        assert!(n > 100_000, "{}: only {n} properties", i.set);
        assert!(
            hidden * 100 < n,
            "{}: {hidden} of {n} out of their group",
            i.set
        );
    }
}

/// Behaviour: data.property.string-reference-token-then-table
#[test]
fn a_stored_string_reference_names_its_token_and_then_its_table() {
    for i in installs() {
        let (mut n, mut tables) = (0, 0);
        for (_, c) in collections(i) {
            for p in c.iter() {
                if let PropertyValue::StringInfo { info, .. } = &p.value {
                    n += 1;
                    if info.literal.is_none()
                        && type_of_id(info.table.0) == Some(RecordType::STRING_TABLE)
                    {
                        tables += 1;
                    }
                }
            }
        }
        assert!(n > 10_000, "{}: {n} references", i.set);
        assert_eq!(
            tables, n,
            "{}: references whose second word is a string table",
            i.set
        );
    }
}

/// Behaviour: data.entity.derived-holds-its-parents-properties
#[test]
fn a_derived_entity_holds_its_parents_properties_beneath_its_own() {
    for i in installs() {
        let list = list(i);
        let all: HashMap<u32, Arc<Entity>> = records::<Entity>(i, RecordType::ENTITY)
            .into_iter()
            .map(|(id, e)| (id, Arc::new(e)))
            .collect();
        let lookup = |id: u32| all.get(&id).cloned();
        let mut derived = 0;
        for e in all.values() {
            let Some(parent) = e.object.and_then(|o| all.get(&o)) else {
                continue;
            };
            derived += 1;
            let total = e.total_properties(&lookup);
            let parent_total = parent.total_properties(&lookup);
            let own = |id: u32| e.properties.as_ref().and_then(|c| c.get(list, id));
            for p in e.properties.iter().flat_map(PropertyCollection::iter) {
                if own(p.id) == Some(p) {
                    let got = total.get(list, p.id).expect("its own property");
                    assert!(got.value.same(&p.value), "{:08X?} {:#x}", e.id, p.id);
                }
            }
            for id in parent_total.iter().map(|p| p.id) {
                if let (None, Some(p)) = (own(id), parent_total.get(list, id)) {
                    let got = total.get(list, id).expect("an inherited property");
                    assert!(got.value.same(&p.value), "{:08X?} {id:#x}", e.id);
                }
            }
            assert_eq!(e.visual_desc(&lookup), parent.visual_desc(&lookup));
        }
        assert!(derived > 1_000, "{}: {derived} derived entities", i.set);
    }
}
