//! An object's qualities written out for an admin: the levels, the order, each kind's format, a
//! name the data does not hold, the description, and the natives' frame.

use auberean_assets::script::{ObjectDescription, Qualities as Record};
use auberean_client_runtime::description;
use auberean_client_runtime::quality_dump::{
    bools_text, data_ids_text, descriptions_text, floats_text, inq_description, instance_ids_text,
    ints_text, long_ints_text, own_description_text, positions_text, string_infos_text,
    strings_text, timestamps_text, Levels,
};
use auberean_primitives::DataId;
use auberean_protocol::text::{NarrowString, StringInfo};
use auberean_protocol::types::Placement;
use auberean_script::native_object::NativeObject;
use auberean_script::value::tag;

use crate::common::{Bench, ABSENT, ABSENT_REF, ITEM, ITEM_REF};

fn wide(s: &[u16]) -> String {
    String::from_utf16(s).unwrap()
}

/// Every value named by its number.
fn named(e: u32, v: u32) -> Option<String> {
    Some(format!("{e:X}-{v}"))
}

/// A level's own qualities are its table in hash order: a key's bucket is the key modulo 47.
///
/// Behaviour: script.model.a-dump-lists-each-level-of-a-kind-in-hash-order
#[test]
fn a_dump_lists_the_object_then_its_record_in_hash_order() {
    let mut b = Bench::new();
    let o = b.host.model.object_mut(ITEM).unwrap();
    o.qualities.ints.insert(1, -1);
    o.qualities.ints.insert(48 + 2, 7);
    o.qualities.ints.insert(47, 8);
    let record = Record {
        ints: vec![(9, 90), (3, 30)],
        ..Record::default()
    };
    let o = b.host.model.object(ITEM);
    let levels = Levels {
        object: o,
        record: Some(&record),
        recurse: true,
    };
    let text = ints_text(levels, &mut named);
    assert_eq!(
        wide(&text),
        "3 int stats in LOCAL qualities level 0\n\
         4000001F-47(47): 8\n\
         4000001F-1(1): 4294967295\n\
         4000001F-50(50): 7\n\
         \n\
         2 int stats in BASE qualities level 1\n\
         4000001F-9(9): 90\n\
         4000001F-3(3): 30\n\
         \n"
    );
}

/// Asked not to go on to the record, the object's own level is listed once and the call returns.
///
/// Behaviour: script.model.a-dump-of-the-objects-own-level-answers-once
#[test]
fn a_dump_of_the_objects_own_level_answers_once() {
    let mut b = Bench::new();
    b.host
        .model
        .object_mut(ITEM)
        .unwrap()
        .qualities
        .ints
        .insert(1, 2);
    let record = Record {
        ints: vec![(9, 90)],
        ..Record::default()
    };
    let levels = Levels {
        object: b.host.model.object(ITEM),
        record: Some(&record),
        recurse: false,
    };
    assert_eq!(
        wide(&ints_text(levels, &mut named)),
        "1 int stats in LOCAL qualities level 0\n4000001F-1(1): 2\n\n"
    );
}

/// A value the enumeration does not name is listed under the last name found.
///
/// Behaviour: script.model.a-dump-lists-a-value-the-data-does-not-name-under-the-last-name
#[test]
fn a_value_the_data_does_not_name_is_listed_under_the_last_name() {
    let mut b = Bench::new();
    let o = b.host.model.object_mut(ITEM).unwrap();
    o.qualities.bools.insert(1, 1);
    o.qualities.bools.insert(2, 0);
    o.qualities.bools.insert(3, 1);
    let levels = Levels {
        object: b.host.model.object(ITEM),
        record: None,
        recurse: true,
    };
    let mut names = |_: u32, v: u32| (v == 2).then(|| "Two".to_owned());
    assert_eq!(
        wide(&bools_text(levels, &mut names)),
        "3 bool stats in LOCAL qualities level 0\n\
         (1): true\n\
         Two(2): false\n\
         Two(3): true\n\
         \n"
    );
}

/// Each kind writes its values as the client's format does.
///
/// Behaviour: script.model.a-dump-writes-each-kind-in-its-format
#[test]
fn each_kind_writes_its_values_in_its_format() {
    let mut b = Bench::new();
    let o = b.host.model.object_mut(ITEM).unwrap();
    let q = &mut o.qualities;
    q.data_ids.insert(1, 0x0600_0ABC);
    q.floats.insert(1, 1.005);
    q.instance_ids.insert(1, 0x8130_0000_1000_0002);
    q.long_ints.insert(1, 0x1_0000_0005);
    q.strings.insert(1, NarrowString(b"Caf\xE9".to_vec()));
    q.timestamps.insert(1, 2.5);
    q.positions.insert(
        1,
        Placement {
            cell: 0x0298_0109,
            frame: Default::default(),
        },
    );
    let levels = Levels {
        object: b.host.model.object(ITEM),
        record: None,
        recurse: false,
    };
    let mut none = |_: u32, _: u32| None;
    let row = |text: Vec<u16>| wide(&text).lines().nth(1).unwrap().to_owned();
    assert_eq!(row(data_ids_text(levels, &mut none)), "(1): 0x06000ABC");
    assert_eq!(row(floats_text(levels, &mut none)), "(1): 1.00");
    assert_eq!(
        row(instance_ids_text(levels, &mut none)),
        "(1): 0x8130000010000002"
    );
    assert_eq!(
        row(long_ints_text(levels, &mut none)),
        "(1): 5",
        "the low half"
    );
    assert_eq!(row(strings_text(levels, &mut none)), "(1): Café");
    assert_eq!(row(timestamps_text(levels, &mut none)), "(1): 2.50");
    assert!(row(positions_text(levels, &mut none)).starts_with("(1): 0x02980109 [0.000000 "));
}

/// A string reference that can show is written for a log; one that cannot says so.
///
/// Behaviour: script.model.a-dump-writes-a-string-reference-for-a-log
#[test]
fn a_string_reference_is_written_for_a_log_or_says_it_cannot_show() {
    let mut b = Bench::new();
    let o = b.host.model.object_mut(ITEM).unwrap();
    o.qualities
        .string_infos
        .insert(1, StringInfo::literal("Asheron"));
    o.qualities.string_infos.insert(
        2,
        StringInfo {
            string_id: 7,
            table_id: 0x2300_0001,
            ..StringInfo::default()
        },
    );
    let (host_model, text) = (&b.host.model, &mut b.host.text);
    let levels = Levels {
        object: host_model.object(ITEM),
        record: None,
        recurse: false,
    };
    let out = string_infos_text(levels, text, &mut |_, _| None);
    assert_eq!(
        wide(&out),
        "2 stringinfo stats in LOCAL qualities level 0\n\
         (1): (Asheron)\n\
         (2): Unable to display StringInfo\n\
         \n"
    );
}

/// A description lists what it has set, in the client's order; a record with none says so.
///
/// Behaviour: script.model.a-dump-lists-a-descriptions-contents-in-order
#[test]
fn a_description_lists_what_it_has_set_in_the_clients_order() {
    let mut b = Bench::new();
    let o = b.host.model.object_mut(ITEM).unwrap();
    description::set_int(&mut o.weenie, 3, 0x1F);
    description::set_int(&mut o.weenie, 0x106, -1);
    description::set_bool(&mut o.weenie, 0x117, 1);
    description::set_float(&mut o.weenie, 0x12A, 1.5);
    description::set_instance_id(&mut o.weenie, 0x102, 5);
    let (model, text) = (&b.host.model, &mut b.host.text);
    let none = Record::default();
    let levels = Levels {
        object: model.object(ITEM),
        record: Some(&none),
        recurse: true,
    };
    let out = descriptions_text(levels, text, &mut named);
    assert_eq!(
        wide(&out),
        "WeenieDesc contents in LOCAL qualities level 0\n\
         Weenie description contains:\n\
         40000024-258(258): 0x0000000000000005\n\
         40000021-298(298): 1.500000\n\
         4000001F-262(262): 4294967295\n\
         4000001F-3(3): 0000001F\n\
         40000020-279(279): true\n\
         \n\
         WeenieDesc contents in BASE qualities level 1\n\
         No WeenieDesc\n"
    );
    let with = Record {
        description: Some(ObjectDescription {
            mask: 0x10,
            icon: Some(DataId(0x0600_0001)),
            ..ObjectDescription::default()
        }),
        ..Record::default()
    };
    let (model, text) = (&b.host.model, &mut b.host.text);
    let levels = Levels {
        object: model.object(ITEM),
        record: Some(&with),
        recurse: true,
    };
    assert!(wide(&descriptions_text(levels, text, &mut named))
        .ends_with("level 1\nWeenie description contains:\n40000023-257(257): 06000001\n\n"));
    let (model, text) = (&b.host.model, &mut b.host.text);
    assert_eq!(
        own_description_text(model.object(ABSENT), text, &mut named),
        b"No Weenie Desc."
    );
    assert_eq!(
        inq_description(model.object(ITEM)).unwrap().numbers[0],
        model.object(ITEM).unwrap().weenie.numbers[0]
    );
}

/// The natives take whether to go on to the record, and answer a new string; a null receiver
/// crashes the call, and an object the client does not hold has its own line.
///
/// Behaviour: script.model.a-dump-native-answers-a-new-string
#[test]
fn the_natives_answer_a_new_string_and_a_missing_object_says_so() {
    let mut b = Bench::new();
    let (status, slots) = b.call(0x81, 0x5A, ITEM_REF, &[(1, tag::MIXED)], 1);
    assert_eq!(status, auberean_script::Status::Success);
    let (r, t) = slots[0];
    assert_eq!(t, tag::REFERENCE);
    let NativeObject::Text(text) = b.vm.heap.native(r).unwrap() else {
        panic!("a string");
    };
    assert_eq!(wide(text), "0 int stats in LOCAL qualities level 0\n\n");
    let (status, slots) = b.call(0x81, 0x5A, ABSENT_REF, &[(1, tag::MIXED)], 1);
    assert_eq!(status, auberean_script::Status::Success);
    let NativeObject::Text(text) = b.vm.heap.native(slots[0].0).unwrap() else {
        panic!("a string");
    };
    assert_eq!(
        wide(text),
        "WREPOSITORY::DumIntsToString: ERROR: missing qualities!!"
    );
    let (status, _) = b.call(
        0x81,
        0x5A,
        auberean_script::value::NULL,
        &[(1, tag::MIXED)],
        1,
    );
    assert_eq!(status, auberean_script::Status::Crash);
}
