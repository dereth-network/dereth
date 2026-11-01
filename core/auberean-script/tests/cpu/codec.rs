//! The packed forms on hand-built values.

use auberean_script::args::{self, Arg};
use auberean_script::graph::{field, Body, Entry, Graph, PackedObject};
use auberean_script::heap::Heap;
use auberean_script::native_object::{NativeObject, BUCKET_SIZES};
use auberean_script::opcode::op;
use auberean_script::value::tag;

use crate::asm::{library, w, Pkg};

fn lib() -> auberean_script::Library {
    let packages = [
        Pkg {
            name: "Thing",
            id: 0x10,
            size: 2,
            parent: None,
            functions: vec![0],
            native: false,
        },
        Pkg {
            name: "AAHash",
            id: 1,
            size: 0,
            parent: None,
            functions: vec![],
            native: true,
        },
        Pkg {
            name: "RList",
            id: 0x65,
            size: 0,
            parent: None,
            functions: vec![],
            native: true,
        },
    ];
    library(&[w(op::END)], &packages, &[])
}

/// Behaviour: script.args.values-in-buffer-order
#[test]
fn an_argument_buffer_holds_tagged_values_in_order_and_refuses_an_unknown_tag() {
    let l = lib();
    let g = Graph {
        ids: vec![9],
        entries: vec![Entry {
            field: field::OBJECT | field::LOADED | field::ROOT,
            body: Body::Object(PackedObject {
                state: 0,
                package: 0x10,
                native_type: 0,
                native: None,
                slots: vec![1, 2],
                tags: vec![tag::INT, tag::INT],
            }),
        }],
    };
    let values = vec![
        Arg::Int(7),
        Arg::LongInt(1 << 40),
        Arg::Float(1.5f32.to_bits()),
        Arg::LongFloat(2.5f64.to_bits()),
        Arg::Object(g),
    ];
    let bytes = args::encode(&values).unwrap();
    assert_eq!(&bytes[..8], &[1, 0, 0, 0, 7, 0, 0, 0]);
    assert_eq!(args::decode(&bytes, &l).unwrap(), values);
    let mut bad = bytes.clone();
    bad[0] = 6;
    assert!(args::decode(&bad, &l).is_err());
    assert!(args::decode(&bytes[..bytes.len() - 1], &l).is_err());
}

/// Behaviour: script.graph.object-size-must-match
#[test]
fn a_script_object_whose_slot_count_is_not_its_packages_size_is_refused() {
    let l = lib();
    let g = Graph {
        ids: vec![0],
        entries: vec![Entry {
            field: field::OBJECT | field::LOADED,
            body: Body::Object(PackedObject {
                state: 0,
                package: 0x10,
                native_type: 0,
                native: None,
                slots: vec![1, 2, 3],
                tags: vec![tag::INT; 3],
            }),
        }],
    };
    let bytes = g.encode().unwrap();
    assert!(Graph::decode(&bytes, &l).is_err());
}

/// Behaviour: script.heap.import-rewrites-references
#[test]
fn a_graph_read_into_the_table_gets_new_ids_and_its_references_follow() {
    let l = lib();
    let list = NativeObject::new(&l, 0x65);
    let NativeObject::Sequence { item, .. } = list else {
        panic!("a list")
    };
    let g = Graph {
        ids: vec![40, 41, 42],
        entries: vec![
            Entry {
                field: field::OBJECT | field::LOADED | field::ROOT,
                body: Body::Object(PackedObject {
                    state: 0,
                    package: 0x10,
                    native_type: 0,
                    native: None,
                    slots: vec![41, 5],
                    tags: vec![tag::REFERENCE, tag::INT],
                }),
            },
            Entry {
                field: field::OBJECT | field::LOADED,
                body: Body::Object(PackedObject {
                    state: 0,
                    package: 0xFFFF,
                    native_type: 0x65,
                    native: Some(NativeObject::Sequence {
                        item,
                        items: vec![
                            auberean_script::native_object::Item::Ref(42),
                            auberean_script::native_object::Item::Ref(40),
                        ],
                    }),
                    slots: vec![],
                    tags: vec![],
                }),
            },
            Entry {
                field: field::OBJECT | field::SINGLETON,
                body: Body::Singleton(0x7000_0010),
            },
        ],
    };
    let mut heap = Heap::new();
    heap.add(field::OBJECT, auberean_script::heap::Target::Relative(0));
    let root = heap.import(&g, None).unwrap();
    let o = heap.object(root).unwrap();
    let list_id = o.slots[0];
    assert_ne!(list_id, 41);
    assert_eq!(o.slots[1], 5, "an integer slot is not a reference");
    let singleton = heap.singleton(0x7000_0010);
    let NativeObject::Sequence { items, .. } = heap.native(list_id).unwrap() else {
        panic!("a list")
    };
    assert_eq!(items[0].reference(), Some(singleton));
    assert_eq!(items[1].reference(), Some(root));
}

/// Behaviour: script.graph.hash-new-entry-heads-its-chain
#[test]
fn a_hash_table_packs_bucket_by_bucket_each_chain_newest_first() {
    let l = lib();
    let mut h = match NativeObject::new(&l, 1) {
        NativeObject::Hash(h) => h,
        _ => panic!("a hash"),
    };
    // A new table has 47 buckets.
    assert_eq!(h.buckets.len(), BUCKET_SIZES[2] as usize);
    use auberean_script::native_object::Item::Word;
    for k in [50u32, 3, 97] {
        let b = Word(k).bucket(h.buckets.len());
        h.buckets[b].insert(0, (Word(k), Word(k * 10)));
    }
    let mut w = auberean_script::wire::Out::new();
    NativeObject::Hash(h).write(&mut w).unwrap();
    let words: Vec<u32> = w
        .buf
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    // 50, 3 and 97 all land in bucket 3 of 47; each was added at the head of the chain.
    assert_eq!(words, vec![3 | 2 << 24, 97, 970, 3, 30, 50, 500]);
}

fn word_var(id: u32, v: u32) -> auberean_script::native_object::Variable {
    auberean_script::native_object::Variable {
        id,
        kind: 2,
        value: auberean_script::native_object::VariableValue::Word(v),
    }
}

/// Behaviour: script.graph.quest-list-reads-over-one-record
#[test]
fn a_quest_list_read_into_the_table_keeps_what_earlier_records_left_in_their_strings() {
    use auberean_script::native_object::{QuestInfo, TextInfo};
    use auberean_script::wire::{In, Out};
    let text = |string_id, vars: Vec<_>| TextInfo {
        string_id,
        table: 7,
        #[allow(clippy::cast_possible_truncation)]
        count: vars.len() as u16,
        variables: vars,
        ..TextInfo::default()
    };
    let first = QuestInfo {
        id: 1,
        name: TextInfo {
            has_literal: 1,
            literal: Some("Ab".encode_utf16().collect()),
            ..text(3, Vec::new())
        },
        journal: text(5, vec![word_var(40, 9)]),
        bestowed: 2f64.to_bits(),
        ..QuestInfo::default()
    };
    let second = QuestInfo {
        id: 2,
        journal: text(6, vec![word_var(41, 3), word_var(40, 8)]),
        ..QuestInfo::default()
    };
    let third = QuestInfo {
        id: 3,
        name: text(4, Vec::new()),
        ..QuestInfo::default()
    };
    let packed = NativeObject::QuestList(vec![first, second, third.clone()]);
    // The packed form: the count, then each record; it reads back to the same list.
    let mut w = Out::new();
    packed.write(&mut w).unwrap();
    let mut r = In::new(&w.buf);
    assert_eq!(NativeObject::read(&mut r, &lib(), 0xA1).unwrap(), packed);
    assert!(r.at_end());

    // Read into the table, each record is read over the one before: a string reference keeps
    // the earlier records' variables (one with the same id replaced) and, unused, their literal.
    let NativeObject::QuestList(held) = packed.unpacked() else {
        panic!("a quest list")
    };
    let ids = |t: &TextInfo| t.variables.iter().map(|v| v.id).collect::<Vec<_>>();
    assert_eq!(ids(&held[1].journal), [40, 41]);
    assert_eq!(held[1].journal.variables[0], word_var(40, 8));
    assert_eq!(
        (held[2].journal.string_id, ids(&held[2].journal)),
        (0, vec![40, 41])
    );
    assert_eq!(held[2].name.has_literal, 0);
    assert!(held[2].name.literal.is_some());
    // Packed again, the third carries those variables and no literal.
    let mut want = third;
    want.journal = TextInfo {
        string_id: 0,
        table: 0,
        count: 2,
        variables: vec![word_var(40, 8), word_var(41, 3)],
        ..TextInfo::default()
    };
    assert_eq!(held[2].pack(), want.pack());
    assert_eq!(held[2].id, 3);
}

/// Behaviour: script.graph.behavior-params-pack-what-differs
#[test]
fn behavior_parameters_pack_only_what_differs_from_a_new_ones() {
    use auberean_script::native_object::BehaviorParams;
    use auberean_script::wire::In;
    // A new one packs its bits alone: none.
    assert_eq!(BehaviorParams::default().pack(), 0u32.to_le_bytes());
    let mut p = BehaviorParams {
        behavior: 0x1000_0041,
        target: 0x8130_0000_1000_0002,
        move_to_cancels: 1,
        fade_children: 1,
        // Within 0.0002 of none: not packed.
        impulse: [0.0001f32.to_bits(), 0, 0],
        ..BehaviorParams::default()
    };
    assert_eq!(p.bits(), 0x1 | 0x2000 | 0x20 | 0x100);
    let words: Vec<u32> = p
        .pack()
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert_eq!(
        words,
        [0x2121, 0x1000_0041, 0x1000_0002, 0x8130_0000, 1, 1],
        "the behavior, the target (low word first), then the switches in their order"
    );
    // Read over a new one, it comes back (but the impulse, which was not packed).
    let mut back = BehaviorParams::default();
    back.read_over(&mut In::new(&p.pack())).unwrap();
    p.impulse = [0; 3];
    assert_eq!(back, p);
    // An event not sent on completion differs from a new one's and is packed.
    let quiet = BehaviorParams {
        send_event: 0,
        ..BehaviorParams::default()
    };
    assert_eq!(quiet.bits(), 0x200);
}
