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
