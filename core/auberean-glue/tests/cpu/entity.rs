//! An entity description's properties, asked for by id and type.

use auberean_assets::entity::Entity;
use auberean_assets::property::{
    Property, PropertyCollection, PropertyDescriptor, PropertyGroup, PropertyList, PropertyValue,
    StringInfo, StringVariable, VariableValue,
};
use auberean_glue::entity::{answer, Answer, Asked};
use auberean_primitives::{DataId, Vec3};
use auberean_script::native_object::{TextInfo, Variable, VariableValue as Script};

fn filed(group: u32) -> PropertyDescriptor {
    PropertyDescriptor {
        group,
        ..PropertyDescriptor::default()
    }
}

fn group(group: u32, properties: Vec<(u32, PropertyValue)>) -> PropertyGroup {
    PropertyGroup {
        group,
        properties: properties
            .into_iter()
            .map(|(id, value)| Property {
                id,
                group,
                value,
                ..Property::default()
            })
            .collect(),
        optional: Vec::new(),
    }
}

/// A property is found in the first group of the entity's own that has the group the property
/// list files it under, and nowhere else; a question for its type answers its value (a string
/// reference as a script's, its token its string), a question for another type nothing.
///
/// Behaviour: script.natives.entity-description-properties
#[test]
fn a_property_answers_from_its_listed_group_and_only_for_its_type() {
    let list = PropertyList::from_descriptors([
        (0x10, filed(5)),
        (0x11, filed(5)),
        (0x12, filed(0)),
        (0x13, filed(6)),
        (0x14, filed(6)),
        (0x15, filed(6)),
    ]);
    let name = StringInfo {
        token: 77,
        table: DataId(0x2300_0001),
        literal: None,
        variables: vec![StringVariable {
            key: 9,
            value: VariableValue::Int(-3),
        }],
    };
    let entity = Entity {
        properties: Some(PropertyCollection {
            groups: vec![
                group(5, vec![(0x10, PropertyValue::Integer(-7))]),
                group(5, vec![(0x11, PropertyValue::Integer(1))]),
                group(
                    6,
                    vec![
                        (0x12, PropertyValue::Integer(2)),
                        (0x13, PropertyValue::Bool(true)),
                        (
                            0x14,
                            PropertyValue::StringInfo {
                                info: name,
                                token_name: String::new(),
                            },
                        ),
                        (
                            0x15,
                            PropertyValue::Vector(Vec3 {
                                x: 1.0,
                                y: 2.0,
                                z: 3.0,
                            }),
                        ),
                    ],
                ),
            ],
        }),
        ..Entity::default()
    };
    let ask = |id: u32, asked: Asked| answer(&entity, &list, id, asked).unwrap();
    assert_eq!(
        ask(0x10, Asked::Integer),
        Some(Answer::Word((-7i32).cast_unsigned()))
    );
    assert_eq!(ask(0x10, Asked::Enum), None, "another type answers nothing");
    // Only in a second group of its id: not found.
    assert_eq!(ask(0x11, Asked::Integer), None);
    // Filed under no group: not found, wherever it is.
    assert_eq!(ask(0x12, Asked::Integer), None);
    // Not in the property list.
    assert_eq!(ask(0x99, Asked::Integer), None);
    assert_eq!(ask(0x13, Asked::Bool), Some(Answer::Word(1)));
    assert_eq!(
        ask(0x15, Asked::Vector),
        Some(Answer::Vector([
            1f32.to_bits(),
            2f32.to_bits(),
            3f32.to_bits()
        ]))
    );
    assert_eq!(
        ask(0x14, Asked::StringInfo),
        Some(Answer::Text(TextInfo {
            string_id: 77,
            table: 0x2300_0001,
            count: 1,
            variables: vec![Variable {
                id: 9,
                kind: 2,
                value: Script::Word((-3i32).cast_unsigned()),
            }],
            ..TextInfo::default()
        }))
    );
    // An entity without properties has none.
    assert_eq!(
        answer(&Entity::default(), &list, 0x10, Asked::Integer),
        Ok(None)
    );
}

/// Behaviour: script.glue.entity-names-and-link-requirements
#[test]
fn an_entity_is_found_by_name_and_a_links_requirements_are_five_words() {
    use auberean_script::native_object::NativeObject;
    use auberean_script::value::NULL;
    use auberean_script::wire::{In, Out};
    use auberean_script::Status;
    const ENTITY: u32 = 0x22;
    const LINK: u32 = 0x38;
    let mut b = crate::call::Bench::new();
    b.host
        .names
        .insert(("Drudge_Sneaker.ent".to_owned(), 0x4000_0049), 0x4700_1234);
    let name = b.text("Drudge_Sneaker.ent");
    assert_eq!(b.answer(ENTITY, 0x0F, NULL, &[name]), 0x4700_1234);
    // Another type's name of the same file is not an entity's.
    b.host.names.clear();
    b.host
        .names
        .insert(("Drudge_Sneaker.ent".to_owned(), 0x4000_0048), 0x4700_1234);
    assert_eq!(b.answer(ENTITY, 0x0F, NULL, &[name]), 0);
    let (_, slots) = b.call(ENTITY, 0x0E, NULL, &[0x4700_1234], 1);
    assert_eq!(slots[0].1, auberean_script::value::tag::REFERENCE);
    assert_eq!(
        String::from_utf16(&b.text_of(slots[0].0)).unwrap(),
        "Drudge_Sneaker.ent"
    );
    let (_, slots) = b.call(ENTITY, 0x0E, NULL, &[0x4700_9999], 1);
    assert!(b.text_of(slots[0].0).is_empty());

    // A new one is all zero; each setter sets its word and its getter reads it back.
    let l = b.new_object(LINK);
    for getter in [0x01, 0x03, 0x04, 0x05, 0x06] {
        assert_eq!(b.answer(LINK, getter, l, &[]), 0);
    }
    for (k, (setter, getter)) in (0u32..).zip([
        (0x07, 0x01),
        (0x09, 0x03),
        (0x0A, 0x04),
        (0x0B, 0x05),
        (0x0C, 0x06),
    ]) {
        let v = 0x1000_0000 + k * 0x0101 + 7;
        assert_eq!(b.call(LINK, setter, l, &[v], 0).0, Status::Success);
        assert_eq!(b.answer(LINK, getter, l, &[]), v);
    }
    // Packed: the five words in order; read back the same; a copy is the same words.
    let o = b.vm.heap.native(l).unwrap().clone();
    let mut w = Out::new();
    o.write(&mut w).unwrap();
    let want: Vec<u8> = (0..5u32)
        .flat_map(|k| (0x1000_0000 + k * 0x0101 + 7).to_le_bytes())
        .collect();
    assert_eq!(w.buf, want);
    let lib = auberean_script::library::Library::default();
    let back = NativeObject::read(&mut In::new(&w.buf), &lib, LINK).unwrap();
    assert_eq!(back, o);
    assert_eq!(o.copied(), o);
    // A null receiver crashes the call.
    assert_eq!(b.call(LINK, 0x01, NULL, &[], 1).0, Status::Crash);
    assert_eq!(b.call(LINK, 0x07, NULL, &[1], 0).0, Status::Crash);
}
