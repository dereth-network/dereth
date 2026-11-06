//! The packed forms on hand-built values.

use auberean_script::args::{self, Arg};
use auberean_script::graph::{field, Body, Entry, Graph, PackedObject};
use auberean_script::heap::Heap;
use auberean_script::native_object::{native_type, NativeObject, BUCKET_SIZES};
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
            name: "LAHashSet",
            id: 0x1AC8,
            size: 0,
            parent: None,
            functions: vec![],
            native: true,
        },
        Pkg {
            name: "LLMultiHash",
            id: 0x3D,
            size: 0,
            parent: None,
            functions: vec![],
            native: true,
        },
        Pkg {
            name: "AAHashIter",
            id: 2,
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
        Pkg {
            name: "LLMultiHashIter",
            id: 0x3E,
            size: 0,
            parent: None,
            functions: vec![],
            native: true,
        },
        Pkg {
            name: "NAHash",
            id: 0x4C,
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

/// Behaviour: script.graph.a-native-copy-reverses-each-hash-chain
#[test]
fn a_native_copy_of_a_hash_reverses_each_chain_and_keeps_everything_else_in_order() {
    use auberean_script::native_object::Item::{Long, Word};
    let l = lib();
    // New tables: a hash and a set of 47 buckets, a multi-hash of 193, an empty list, an iterator
    // over nothing.
    let NativeObject::Hash(mut h) = NativeObject::new(&l, 1) else {
        panic!("a hash")
    };
    let NativeObject::HashSet(mut s) = NativeObject::new(&l, 0x1AC8) else {
        panic!("a set")
    };
    let NativeObject::MultiHash(mut m) = NativeObject::new(&l, 0x3D) else {
        panic!("a multi-hash")
    };
    assert_eq!(
        (h.buckets.len(), s.buckets.len(), m.buckets.len()),
        (47, 47, 193)
    );
    assert_eq!(
        NativeObject::new(&l, 2),
        NativeObject::Iterator {
            container: auberean_script::value::NULL,
            position: 0
        }
    );
    // 50, 3 and 97 share bucket 3 of 47, 10 is alone in bucket 10.
    for k in [50u32, 3, 97, 10] {
        let b = Word(k).bucket(47);
        h.buckets[b].insert(0, (Word(k), Word(k * 10)));
        s.buckets[b].insert(0, Long(u64::from(k)));
        m.insert(Long(u64::from(k)), Long(u64::from(k) + 1));
    }
    let chain = |o: &NativeObject, b: usize| -> Vec<u32> {
        match o {
            NativeObject::Hash(h) => h.buckets[b]
                .iter()
                .map(|(k, _)| match k {
                    Word(k) => *k,
                    _ => 0,
                })
                .collect(),
            NativeObject::HashSet(s) => s.buckets[b]
                .iter()
                .map(|k| match k {
                    #[allow(clippy::cast_possible_truncation)]
                    Long(k) => *k as u32,
                    _ => 0,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let (h, s) = (NativeObject::Hash(h), NativeObject::HashSet(s));
    assert_eq!(chain(&h, 3), vec![97, 3, 50]);
    let (hc, sc) = (h.copied(), s.copied());
    assert_eq!(chain(&hc, 3), vec![50, 3, 97], "each chain reversed");
    assert_eq!(chain(&sc, 3), vec![50, 3, 97]);
    assert_eq!(chain(&hc, 10), vec![10]);
    let NativeObject::Hash(copy) = &hc else {
        panic!("a hash")
    };
    assert_eq!((copy.size_index, copy.buckets.len()), (2, 47));
    // A multi-hash, a list and an iterator are copied as they are.
    let m = NativeObject::MultiHash(m);
    assert_eq!(m.copied(), m);
    let list = NativeObject::Sequence {
        item: auberean_script::library::Elem::Word,
        items: vec![Word(3), Word(1), Word(2)],
    };
    assert_eq!(list.copied(), list);
    let at = NativeObject::Iterator {
        container: 7,
        position: 2,
    };
    assert_eq!(at.copied(), at);
}

/// The table packed from `root` and read back in under new ids: the new root.
fn repacked(heap: &mut Heap, root: u32, l: &auberean_script::Library) -> (Vec<u8>, u32) {
    let bytes = heap.export(root).unwrap().encode().unwrap();
    let g = Graph::decode(&bytes, l).unwrap();
    (bytes, heap.import(&g, None).unwrap())
}

/// Behaviour: script.graph.an-iterator-packs-its-container-and-its-steps
#[test]
fn an_iterator_packs_its_container_and_its_steps_and_stands_there_again_when_read_in() {
    use auberean_script::native_object::{Item::Long, Item::Word, ITERATOR_END};
    let l = lib();
    let mut heap = Heap::new();
    // A hash of three entries in buckets 1, 3 and 10, walked in that order; an iterator at its
    // second entry and one past its last.
    let NativeObject::Hash(mut h) = NativeObject::new(&l, 1) else {
        panic!("a hash")
    };
    for k in [10u32, 3, 1] {
        h.buckets[k as usize].push((Word(k), Word(k * 100)));
    }
    let hash = heap.add_native(1, NativeObject::Hash(h));
    for (place, steps) in [(1, 1), (ITERATOR_END, 3)] {
        let at = heap.add_native(
            2,
            NativeObject::Iterator {
                container: hash,
                position: place,
            },
        );
        let (bytes, back) = repacked(&mut heap, at, &l);
        let words: Vec<u32> = bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert!(
            words.windows(2).any(|w| w == [hash, steps]),
            "the container, then {steps} steps: {words:x?}"
        );
        let NativeObject::Iterator {
            container,
            position,
        } = heap.native(back).unwrap().clone()
        else {
            panic!("an iterator")
        };
        assert_ne!(container, hash, "the container read in anew");
        assert!(matches!(heap.native(container), Ok(NativeObject::Hash(_))));
        assert_eq!(position, place);
    }
    // A multi-hash's iterator names its entry; it packs the entry's place in walking order and
    // names the same entry of the copy read in.
    let NativeObject::MultiHash(mut m) = NativeObject::new(&l, 0x3D) else {
        panic!("a multi-hash")
    };
    m.insert(Long(5), Long(50));
    let second = m.insert(Long(2), Long(20));
    m.insert(Long(7), Long(70));
    let walked: Vec<u64> = m
        .entries()
        .map(|n| match n.key {
            Long(k) => k,
            _ => 0,
        })
        .collect();
    let steps = walked.iter().position(|k| *k == 2).unwrap();
    let multi = heap.add_native(0x3D, NativeObject::MultiHash(m));
    let at = heap.add_native(
        0x3E,
        NativeObject::Iterator {
            container: multi,
            position: second,
        },
    );
    let (_, back) = repacked(&mut heap, at, &l);
    let NativeObject::Iterator {
        container,
        position,
    } = heap.native(back).unwrap().clone()
    else {
        panic!("an iterator")
    };
    let NativeObject::MultiHash(copy) = heap.native(container).unwrap() else {
        panic!("a multi-hash")
    };
    assert_eq!(copy.node(position).map(|n| n.key), Some(Long(2)));
    assert_eq!(
        copy.entries().position(|n| n.id == position),
        Some(steps),
        "{walked:?}"
    );
}

/// Behaviour: script.graph.a-hashed-key-read-in-is-hashed-through-what-it-names
#[test]
fn a_hashed_reference_key_read_into_the_table_follows_its_object_and_stops_using_its_filed_hash() {
    use auberean_script::native_object::Item;
    let l = lib();
    let NativeObject::Hash(mut h) = NativeObject::new(&l, 0x4C) else {
        panic!("a hash")
    };
    // Key: the string packed as 41, filed under hash 5, its filed hash in use.
    h.buckets[5].push((
        Item::HashedRef {
            id: 41,
            hash: 5,
            use_hash: 1,
        },
        Item::Word(9),
    ));
    let g = Graph {
        ids: vec![40, 41],
        entries: vec![
            Entry {
                field: field::OBJECT | field::LOADED | field::ROOT,
                body: Body::Object(PackedObject {
                    state: 0,
                    package: 0xFFFF,
                    native_type: 0x4C,
                    native: Some(NativeObject::Hash(h)),
                    slots: vec![],
                    tags: vec![],
                }),
            },
            Entry {
                field: field::OBJECT | field::LOADED,
                body: Body::Object(PackedObject {
                    state: 0,
                    package: 0xFFFF,
                    native_type: 0x76,
                    native: Some(NativeObject::Text("ab".encode_utf16().collect())),
                    slots: vec![],
                    tags: vec![],
                }),
            },
        ],
    };
    let g = Graph::decode(&g.encode().unwrap(), &l).unwrap();
    let mut heap = Heap::new();
    let root = heap.import(&g, None).unwrap();
    let NativeObject::Hash(h) = heap.native(root).unwrap() else {
        panic!("a hash")
    };
    let (key, _) = h.entries().next().copied().unwrap();
    let Item::HashedRef { id, hash, use_hash } = key else {
        panic!("a hashed reference")
    };
    assert!(matches!(heap.native(id), Ok(NativeObject::Text(_))));
    assert_eq!((hash, use_hash), (5, 0), "filed hash kept, no longer used");
    assert_eq!(h.buckets[5].len(), 1, "it stays in its bucket");
}

/// Behaviour: script.library.taken-only-from-the-machines-version
#[test]
fn a_library_is_taken_only_when_it_names_the_machine_version_the_client_runs() {
    use auberean_assets::script::Section;
    let with = |entries: &[(&str, &str)]| {
        let mut lib = crate::asm::sections(&[w(op::END)], &[], &[]);
        lib.sections.push(Section::Versions(
            entries
                .iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect(),
        ));
        auberean_script::Library::from_sections(lib)
    };
    assert!(with(&[("$wcc", "13"), ("GlueGenEngine", "anything")]).is_ok());
    assert!(
        with(&[("other", "12")]).is_ok(),
        "only the machine's entry is checked"
    );
    let refused = with(&[("$wcc", "12")]).unwrap_err();
    assert!(refused.0.contains("version is 12"), "{refused:?}");
}

/// **A new icon description has no layers: it packs as a count of zero**, so an object holding one
/// can be copied and sent.
///
/// Behaviour: script.natives.a-new-icon-packs-a-count-of-nothing
#[test]
fn a_new_icon_description_packs_a_count_of_nothing() {
    let l = lib();
    let icon = NativeObject::new(&l, native_type::ICON_DESC);
    let mut w = auberean_script::wire::Out::new();
    icon.write(&mut w).expect("a new icon description packs");
    assert_eq!(w.buf, vec![0; 4]);
}
