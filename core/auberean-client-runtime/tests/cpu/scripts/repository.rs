//! An object's repository: where its qualities are answered from, how they are set, taken off and
//! adjusted.

use auberean_assets::script::{ObjectDescription, Qualities as Record};
use auberean_client_model::{Item, Model};
use auberean_client_runtime::description;
use auberean_client_runtime::repository::{
    adjust, inq, remove, set, truncate, BOOL, FLOAT, INT, STRING, TIMESTAMP,
};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::ServerMessage;

const ITEM: u64 = 0x8130_0000_1000_0002;
/// The quantity (a number the description holds), and one it does not hold.
const QUANTITY: u32 = 0x106;
const OTHER: u32 = 720;

fn model() -> Model {
    let mut m = Model::new();
    m.apply(&Item {
        blob_id: 0,
        message: ServerMessage::CreateObject(CreateObject {
            id: ITEM,
            ..CreateObject::default()
        }),
    });
    m
}

/// A quality is answered by the object's description (for one it holds and has set), else by the
/// object's own table, else by its qualities record: the record's description, then its tables.
///
/// Behaviour: script.model.a-quality-is-answered-by-the-description-then-the-object-then-its-record
#[test]
fn a_quality_is_answered_by_the_description_then_the_object_then_its_record() {
    let mut m = model();
    let record = Record {
        description: Some(ObjectDescription {
            mask: 0x800,
            quantity: Some(9),
            ..ObjectDescription::default()
        }),
        ints: vec![(OTHER, 40), (QUANTITY, 30)],
        times: vec![(5, 2.5)],
        ..Record::default()
    };
    let o = m.object_mut(ITEM).expect("the item");
    assert_eq!(
        inq(&INT, Some(o), Some(&record), QUANTITY),
        Some(9),
        "the record's description"
    );
    assert_eq!(
        inq(&INT, Some(o), Some(&record), OTHER),
        Some(40),
        "the record's table"
    );
    o.qualities.ints.insert(QUANTITY, 20);
    o.qualities.ints.insert(OTHER, 21);
    assert_eq!(
        inq(&INT, Some(o), Some(&record), QUANTITY),
        Some(20),
        "the object's own"
    );
    assert_eq!(inq(&INT, Some(o), Some(&record), OTHER), Some(21));
    description::set_int(&mut o.weenie, QUANTITY, 3);
    assert_eq!(
        inq(&INT, Some(o), Some(&record), QUANTITY),
        Some(3),
        "its description"
    );
    assert_eq!(inq(&TIMESTAMP, Some(o), Some(&record), 5), Some(2.5));
    assert_eq!(inq(&TIMESTAMP, Some(o), None, 5), None, "no record");
}

/// Setting a quality the object's description holds sets it there and marks it set; any other is
/// set in the object's own table. The record is never written.
///
/// Behaviour: script.model.a-quality-set-goes-to-the-description-when-it-holds-it
#[test]
fn a_quality_set_goes_to_the_description_when_it_holds_it() {
    let mut m = model();
    let o = m.object_mut(ITEM).expect("the item");
    assert!(set(&INT, o, QUANTITY, 5));
    assert_eq!(description::int(&o.weenie, QUANTITY), Some(5));
    assert!(!o.qualities.ints.contains_key(&QUANTITY));
    assert!(set(&INT, o, OTHER, 6));
    assert_eq!(o.qualities.ints.get(&OTHER), Some(&6));
    // The selectable flag lives in the description's flag word; texts never do.
    assert!(set(&BOOL, o, 0x270, 1));
    assert_eq!(description::bool(&o.weenie, 0x270), Some(1));
    assert!(set(&STRING, o, 0x100, b"Rope".to_vec()));
    assert_eq!(inq(&STRING, Some(o), None, 0x100), Some(b"Rope".to_vec()));
}

/// Removing a quality takes it out of the object's description when the description has it set
/// (its mark cleared), else out of the object's own table; a record's value stays and answers
/// again.
///
/// Behaviour: script.model.a-quality-removed-leaves-the-record-answering
#[test]
fn a_quality_removed_leaves_the_record_answering() {
    let mut m = model();
    let record = Record {
        floats: vec![(0x12A, 2.0)],
        ..Record::default()
    };
    let o = m.object_mut(ITEM).expect("the item");
    assert!(set(&FLOAT, o, 0x12A, 1.5));
    assert_eq!(remove(&FLOAT, o, 0x12A), Some(1.5), "from the description");
    assert_eq!(description::float(&o.weenie, 0x12A), None);
    assert_eq!(inq(&FLOAT, Some(o), Some(&record), 0x12A), Some(2.0));
    o.qualities.floats.insert(3, 4.0);
    assert_eq!(remove(&FLOAT, o, 3), Some(4.0), "from its own");
    assert_eq!(remove(&FLOAT, o, 3), None);
    assert_eq!(remove(&FLOAT, o, 0x12A), None, "never from the record");
}

/// Adjusting a number adds to the object's own value; with none of its own, the record's value
/// plus the change becomes its own (the change alone when the record has none); without a record,
/// nothing changes.
///
/// Behaviour: script.model.an-adjusted-quality-adds-to-its-own-value-or-its-records
#[test]
fn an_adjusted_quality_adds_to_its_own_value_or_its_records() {
    assert_eq!(adjust(Some(5), Some(100), true, 3, None), Some((8, 5)));
    assert_eq!(adjust(None, Some(100), true, 3, None), Some((103, 100)));
    assert_eq!(adjust(None, None, true, 3, None), Some((3, 0)));
    assert_eq!(adjust::<i32>(None, None, false, 3, None), None);
    assert_eq!(
        adjust(Some(1.5f32), None, false, 0.25, None),
        Some((1.75, 1.5))
    );
    // Held between its ends; the change alone is not held.
    assert_eq!(
        adjust(Some(5), None, true, 10, Some((0, 12))),
        Some((12, 5))
    );
    assert_eq!(
        adjust(None, Some(5), true, -10, Some((0, 12))),
        Some((0, 5))
    );
    assert_eq!(adjust(None, None, true, 40, Some((0, 12))), Some((40, 0)));
}

/// An adjustment that overflowed (the sum moved against its change) takes the end it ran past.
///
/// Behaviour: script.model.an-overflowing-adjustment-takes-the-end-it-ran-past
#[test]
fn an_overflowing_adjustment_takes_the_end_it_ran_past() {
    let mut v = i32::MAX.wrapping_add(1);
    truncate(1, i32::MAX, -10, i32::MAX - 5, &mut v);
    assert_eq!(v, i32::MAX - 5, "past the top: the greatest");
    let mut v = i32::MIN.wrapping_sub(1);
    truncate(-1, i32::MIN, -10, 10, &mut v);
    assert_eq!(v, -10, "past the bottom: the least");
    let mut v = 3i64;
    truncate(1, 2, 5, 9, &mut v);
    assert_eq!(v, 5, "held to the least");
}

/// A data id set from a file's name: the client's table of file names holds no name on an install
/// of the relaunched game, so the name finds no record and nothing is set (answers 0).
///
/// Behaviour: script.model.a-data-id-loaded-by-a-files-name-finds-nothing-on-the-relaunched-game
#[test]
fn a_data_id_loaded_by_a_files_name_sets_nothing_when_the_table_has_no_names() {
    use crate::common::{Bench, ITEM, ITEM_REF};
    use auberean_script::native_object::NativeObject;
    use auberean_script::value::tag;
    const REPOSITORY: u32 = 0x81;
    const KEY: u32 = 0x5A;
    let mut b = Bench::new();
    let name = b.vm.heap.add_native(
        0x76,
        NativeObject::Text("Story_Black01.jpg".encode_utf16().collect()),
    );
    let args = [(KEY, tag::MIXED), (name, tag::REFERENCE)];
    assert_eq!(b.answer(REPOSITORY, 0x54, ITEM_REF, &args), 0);
    assert!(
        !b.host
            .model
            .object(ITEM)
            .unwrap()
            .qualities
            .data_ids
            .contains_key(&KEY),
        "nothing is set"
    );
    // The same quality set by id is set: the name, not the quality, was refused.
    assert_eq!(
        b.answer(
            REPOSITORY,
            0x2E,
            ITEM_REF,
            &[(KEY, tag::MIXED), (7, tag::MIXED)]
        ),
        1
    );
}

/// The record an object's qualities derive from is its entity template's when its description
/// names one; with no template, its package's (`0x8000_0000` and the package's low half; no
/// package: `0x8000_FFFF`); a description naming something else as its entity names none.
///
/// Behaviour: script.model.an-object-without-a-template-derives-from-its-packages-qualities
#[test]
fn an_object_without_a_template_derives_from_its_packages_qualities() {
    use auberean_client_runtime::host::qualities_record;
    use auberean_protocol::types::WeenieDesc;
    let made = |mask: u32, package_id: u32, entity_did: u32| {
        let mut m = Model::new();
        m.apply(&Item {
            blob_id: 0,
            message: ServerMessage::CreateObject(CreateObject {
                id: ITEM,
                weenie: WeenieDesc {
                    mask,
                    package_id,
                    entity_did,
                    ..WeenieDesc::default()
                },
                ..CreateObject::default()
            }),
        });
        qualities_record(m.object(ITEM).expect("made"))
    };
    let both = WeenieDesc::PACKAGE | WeenieDesc::ENTITY;
    assert_eq!(made(both, 0x37F, 0x4700_0539), 0x8100_0539);
    assert_eq!(made(WeenieDesc::PACKAGE, 0x0012_037F, 0), 0x8000_037F);
    assert_eq!(made(0, u32::MAX, 0), 0x8000_FFFF, "neither is named");
    assert_eq!(
        made(both, 0x37F, 0x2000_0001),
        0,
        "an entity that is no template"
    );
}
