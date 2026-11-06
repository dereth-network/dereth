//! How property values answer, compare and take each other's contents, and how a collection finds,
//! adds and updates properties, over a property list made up for the tests.

use auberean_assets::common::{Color32, Waveform};
use auberean_assets::property::{
    format_g, NoNames, Property, PropertyCollection, PropertyDescriptor, PropertyGroup,
    PropertyList, PropertyValue, StringInfo, StringVariable, ValueKind, VariableValue,
};
use auberean_primitives::{DataId, Vec3};

const SPEED: u32 = 0x4100_0001;
const NAME: u32 = 0x4100_0002;
const FLAG: u32 = 0x4100_0003;
const COLOR: u32 = 0x4100_0004;
const ITEM_GROUP: u32 = 0x4000_000B;
const LOOK_GROUP: u32 = 0x4000_0017;

fn list() -> PropertyList {
    let d = |kind: ValueKind, group: u32, data: u32| PropertyDescriptor {
        value_kind: kind.number(),
        group,
        data,
        report: 0,
        required: false,
    };
    PropertyList::from_descriptors([
        (SPEED, d(ValueKind::Float, ITEM_GROUP, 0)),
        (NAME, d(ValueKind::String, ITEM_GROUP, 0)),
        (FLAG, d(ValueKind::Bool, LOOK_GROUP, 0x10)),
        (COLOR, d(ValueKind::Color, LOOK_GROUP, 0)),
    ])
}

/// Behaviour: data.property-value.answers-only-for-its-type
#[test]
fn a_value_answers_only_a_question_about_its_own_type() {
    let v = PropertyValue::Float(2.5);
    assert_eq!(v.inq_float(), Some(2.5));
    assert_eq!(v.inq_integer(), None);
    assert_eq!(v.inq_bool(), None);
    let mut s = PropertyValue::String("oak".into());
    assert!(!s.set(PropertyValue::Integer(3)));
    assert!(!s.set_from(&PropertyValue::Integer(3)));
    assert_eq!(s.inq_string(), Some("oak"));
    assert!(s.set_from(&PropertyValue::String("ash".into())));
    assert_eq!(s.inq_string(), Some("ash"));
}

/// Behaviour: data.property-value.new-values
#[test]
fn a_new_value_is_false_zero_white_empty_or_unset_by_its_type() {
    assert_eq!(
        PropertyValue::new(ValueKind::Bool),
        PropertyValue::Bool(false)
    );
    assert_eq!(
        PropertyValue::new(ValueKind::Color),
        PropertyValue::Color(Color32(0xFFFF_FFFF))
    );
    assert_eq!(
        PropertyValue::new(ValueKind::Package),
        PropertyValue::Package(u32::MAX)
    );
    let PropertyValue::Waveform(w) = PropertyValue::new(ValueKind::Waveform) else {
        panic!("a waveform");
    };
    assert_eq!((w.kind, w.amplitude, w.frequency), (1, 1.0, 1.0));
    let always = |_: u32| true;
    assert!(!PropertyValue::new(ValueKind::Package).has_valid_data(&always));
    assert!(!PropertyValue::new(ValueKind::String).has_valid_data(&always));
    assert!(!PropertyValue::new(ValueKind::DataFile).has_valid_data(&always));
    assert!(PropertyValue::new(ValueKind::Integer).has_valid_data(&always));
}

/// Behaviour: data.property-value.compare-within-a-tolerance
#[test]
fn vectors_and_waveforms_compare_within_a_tolerance_floats_exactly_and_strings_ignoring_case() {
    let v = |x: f32| PropertyValue::Vector(Vec3 { x, y: 0.0, z: 0.0 });
    assert!(v(1.0).same(&v(1.000_15)));
    assert!(!v(1.0).same(&v(1.000_3)));
    assert!(!PropertyValue::Float(1.0).same(&PropertyValue::Float(1.000_01)));
    let w = |base: f32| {
        PropertyValue::Waveform(Waveform {
            base,
            ..Waveform::defaults()
        })
    };
    assert!(w(0.5).same(&w(0.5001)));
    assert!(!w(0.5).same(&w(0.501)));
    assert!(!PropertyValue::Integer(1).same(&PropertyValue::Enum(1)));
    let s = |t: &str| PropertyValue::String(t.into());
    assert!(s("Oak").same(&s("oAK")), "strings ignore case");
    assert!(!s("oak").same(&s("ash")));
}

/// Behaviour: data.property-value.as-text
#[test]
fn a_value_writes_as_the_game_writes_it_for_output() {
    let d = PropertyDescriptor::default();
    let t = |v: PropertyValue| v.as_text(&d, &NoNames);
    assert_eq!(t(PropertyValue::Bool(true)).as_deref(), Some("True"));
    assert_eq!(t(PropertyValue::Float(2.0)).as_deref(), Some("2"));
    assert_eq!(
        t(PropertyValue::Float(1234.5)).as_deref(),
        Some("1.23e+003")
    );
    assert_eq!(
        t(PropertyValue::Float(0.000_012_5)).as_deref(),
        Some("1.25e-005")
    );
    assert_eq!(
        t(PropertyValue::LongInteger(0x1_0000_0005)).as_deref(),
        Some("5")
    );
    assert_eq!(
        t(PropertyValue::DataFile(DataId(0))).as_deref(),
        Some("<file unset>")
    );
    assert_eq!(
        t(PropertyValue::Package(u32::MAX)).as_deref(),
        Some("<PackageID unset>")
    );
    assert_eq!(
        t(PropertyValue::new(ValueKind::StringInfo)).as_deref(),
        Some("<string table unset>:<token unset>")
    );
    assert_eq!(
        t(PropertyValue::Color(Color32(0x80FF_0000))).as_deref(),
        Some("1.000 0.000 0.000 0.502")
    );
    assert_eq!(format_g(0.0001, 3), "0.0001");
    assert_eq!(format_g(100.0, 3), "100");
}

/// Behaviour: data.property-collection.find-add-update
#[test]
fn a_collection_finds_adds_and_updates_through_the_listed_group() {
    let list = list();
    let mut c = PropertyCollection::default();
    let mut speed = Property::named(&list, SPEED).expect("listed");
    assert_eq!(speed.value, PropertyValue::Float(0.0));
    speed.value.set(PropertyValue::Float(4.0));
    assert!(c.add_property(&list, speed.clone()));
    assert!(!c.add_property(&list, speed.clone()));
    assert_eq!(c.groups.len(), 1);
    assert_eq!(c.groups[0].group, ITEM_GROUP);
    let mut flag = Property::named(&list, FLAG).expect("listed");
    flag.value.set(PropertyValue::Bool(true));
    assert!(c.add_property(&list, flag));
    assert_eq!(c.groups.len(), 2);
    assert_eq!(
        c.get(&list, FLAG).and_then(|p| p.value.inq_bool()),
        Some(true)
    );
    assert_eq!(list.bit(FLAG), Some(0x10));
    assert_eq!(list.bit(SPEED), None);
    // An update takes the other's value when it is of the stored type.
    let mut faster = speed.clone();
    faster.value = PropertyValue::Float(6.0);
    assert!(c.update_property(&list, &faster));
    assert_eq!(
        c.get(&list, SPEED).and_then(|p| p.value.inq_float()),
        Some(6.0)
    );
    // A property stored under another group than the listed one is not found.
    let mut stray = PropertyCollection::default();
    let mut g = PropertyGroup::new(LOOK_GROUP);
    g.properties.push(speed);
    stray.groups.push(g);
    assert!(stray.get(&list, SPEED).is_none());
    assert!(c.inq_by_name(&list, "x").is_none());
}

/// Behaviour: data.property-collection.optional-properties
#[test]
fn optional_properties_come_from_the_list_and_become_required_only_into_an_empty_group() {
    let list = list();
    let mut c = PropertyCollection::default();
    assert!(list.add_optional_properties(LOOK_GROUP, &mut c));
    let g = c.group(LOOK_GROUP).expect("the group");
    assert_eq!(g.optional.len(), 2);
    assert!(g.properties.is_empty());
    assert!(c.get(&list, FLAG).is_none());
    assert!(c.get_optional(&list, COLOR).is_some());
    assert!(c.set_optional_to_required());
    assert!(c.get(&list, COLOR).is_some());
    // A group that already stores properties keeps its optional ones apart.
    assert!(list.add_optional_properties(LOOK_GROUP, &mut c));
    assert!(!c.set_optional_to_required());
}

/// Behaviour: data.string-reference.same
#[test]
fn two_references_are_the_same_by_token_table_and_variables_by_key() {
    let var = |key: u32, v: i32| StringVariable {
        key,
        value: VariableValue::Int(v),
    };
    let a = StringInfo {
        token: 7,
        table: DataId(0x2500_0001),
        literal: None,
        variables: vec![var(1, 5), var(2, 6)],
    };
    let mut b = a.clone();
    b.variables.reverse();
    assert!(a.same(&b));
    // A later variable with a key replaces an earlier one.
    let mut c = a.clone();
    c.variables.insert(0, var(2, 99));
    assert!(a.same(&c));
    let mut d = a.clone();
    d.variables[0] = var(1, 4);
    assert!(!a.same(&d));
    let lit = |s: &str| StringInfo {
        literal: Some(s.into()),
        ..a.clone()
    };
    assert!(lit("x").same(&lit("x")));
    assert!(!lit("x").same(&lit("y")));
    assert!(!lit("x").same(&a));
    assert_eq!(a.pack_size(), {
        let mut w = auberean_dat::Writer::new();
        a.write(&mut w);
        w.finish().len()
    });
}
