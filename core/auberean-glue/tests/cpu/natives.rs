//! Natives called from script, on a hand-built library whose native packages carry the names the
//! containers are bound by.

// Hand-written programs and tables a few hundred entries long: every length and offset fits in
// 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native_object::{Field, NativeObject};
use auberean_script::opcode::op;
use auberean_script::value::{tag, FunctionId, NULL};
use auberean_script::{Program, Status, Vm};

use auberean_glue::chargen::{CreationChoices, CREATION_CHOICES};

use crate::asm::{i, library, small, w, Pkg, TestHost};
use crate::call::Bench;

const THING: u32 = 0x10;
const HASH: u32 = 1;
const HASH_ITER: u32 = 2;
const MULTI: u32 = 3;
const MULTI_ITER: u32 = 4;
const FELLOW: u32 = 0x93;
const FELLOWSHIP: u32 = 0x94;
const CRAFT_SKILL: u32 = 0x1AD4;
const SHOP: u32 = 0x1AD6;
const SHOP_SALE: u32 = 0x1AD7;
const LIST: u32 = 0x65;
const BUILD: u32 = 0x74;
const TEXT: u32 = 0x76;
const TEXT_INFO: u32 = 0x6A;
const TOKENIZE: u32 = 0x6D;
const DAMAGE_TEXT: u32 = 0x97;
const STRING_UTILS: u32 = 0x1ACE;
const VECTOR: u32 = 0x70;
const POSITION: u32 = 0x5A;

/// A package of native functions with the given names, numbered from 1.
fn natives(name: &'static str, id: u32, functions: &[&str]) -> (Pkg, Vec<String>) {
    let mut f = vec![0];
    f.extend(std::iter::repeat_n(0, functions.len()));
    (
        Pkg {
            name,
            id,
            size: 0,
            parent: None,
            functions: f,
            native: true,
        },
        functions.iter().map(ToString::to_string).collect(),
    )
}

/// Link a program whose `THING` function 1 is `body`, with natives named as the library would
/// name them.
fn program(body: Vec<u32>) -> Arc<Program> {
    let mut code = vec![w(op::END), i(op::ENTER, 0), w(op::LEAVE)];
    let f1 = code.len() as u32;
    code.extend(body);
    let (hash, hash_names) = natives(
        "AAHash",
        HASH,
        &["add", "set", "erase", "find", "num_elements", "get_begin"],
    );
    let (iter, iter_names) = natives("AAHashIter", HASH_ITER, &["next", "key", "data", "notdone"]);
    let (multi, multi_names) = natives(
        "AAMultiHash",
        MULTI,
        &[
            "add",
            "count",
            "erase",
            "find",
            "find_last",
            "size",
            "remove",
        ],
    );
    let (multi_iter, multi_iter_names) = natives(
        "AAMultiHashIter",
        MULTI_ITER,
        &["next", "key", "data", "equals"],
    );
    let (fellow, fellow_names) = natives("FellowAdaptor", FELLOW, &["SetName"]);
    let (fellowship, fellowship_names) = natives("FellowshipAdaptor", FELLOWSHIP, &["add"]);
    let (craft, craft_names) = natives("CraftSkillAdaptor", CRAFT_SKILL, &["GetDID", "SetDID"]);
    let (shop, shop_names) = natives(
        "UIShop",
        SHOP,
        &[
            "SetID",
            "SetName",
            "SetDesc",
            "SetSorter",
            "SetPortrait",
            "AddSale",
        ],
    );
    let (sale, sale_names) = natives(
        "UIShopSale",
        SHOP_SALE,
        &[
            "SetItemIID",
            "SetItemDID",
            "SetItemIconDesc",
            "SetItemIconID",
            "SetItemDesc",
            "SetCostDesc",
            "SetCostIcon",
            "SetCostQuantity",
            "SetStockQuantity",
            "SetOrdinal",
        ],
    );
    let (list, list_names) = natives("RList", LIST, &["push_back", "pop_front", "num_elements"]);
    let (build, build_names) = natives("Build", BUILD, &["a", "b", "c", "d", "e", "f", "g", "h"]);
    let (text, text_names) = natives("Text", TEXT, &["a", "b", "c", "d", "e", "f"]);
    let names: Vec<String> = (1..=0x26).map(|k| format!("f{k}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (text_info, text_info_names) = natives("StringInfo", TEXT_INFO, &refs);
    let (tokenize, tokenize_names) = natives("Tokenize", TOKENIZE, &["Tokenize"]);
    let (damage, damage_names) = natives(
        "DamageTextBlob",
        DAMAGE_TEXT,
        &[
            "SetDisplayString",
            "GetDisplayString",
            "SetFontColor",
            "GetFontColor",
            "SetDuration",
            "GetDuration",
            "SetScale",
            "GetScale",
        ],
    );
    let (utils, utils_names) = natives("StringUtils", STRING_UTILS, &["a"]);
    let (vector, vector_names) = natives(
        "Vector",
        VECTOR,
        &[
            "Cross",
            "Dot",
            "Magnitude",
            "MagnitudeSquared",
            "Normalize",
            "Set",
            "Add",
            "Subtract",
            "Scale",
            "Copy",
            "Equals",
            "x",
            "y",
            "z",
            "GetString",
        ],
    );
    let names: Vec<String> = (1..=0x2B).map(|k| format!("p{k}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (position, position_names) = natives("Position", POSITION, &refs);
    let packages = vec![
        Pkg {
            name: "Thing",
            id: THING,
            size: 1,
            parent: None,
            functions: vec![1, f1],
            native: false,
        },
        hash,
        iter,
        multi,
        multi_iter,
        fellow,
        fellowship,
        craft,
        shop,
        sale,
        list,
        build,
        text,
        text_info,
        tokenize,
        damage,
        utils,
        vector,
        position,
    ];
    let mut lib = library(
        &code,
        &packages,
        &[
            "count: ",
            "a[b] and {c}",
            "AND",
            "b",
            "Wave, smile;bow",
            "bow",
        ],
    );
    // Name the native functions as the library would, function k + 1 for name k.
    for (p, names) in lib.packages.iter_mut().zip([
        vec![],
        hash_names,
        iter_names,
        multi_names,
        multi_iter_names,
        fellow_names,
        fellowship_names,
        craft_names,
        shop_names,
        sale_names,
        list_names,
        build_names,
        text_names,
        text_info_names,
        tokenize_names,
        damage_names,
        utils_names,
        vector_names,
        position_names,
    ]) {
        for (k, n) in names.into_iter().enumerate() {
            p.functions[k + 1].name = n;
        }
    }
    let table = auberean_glue::natives(&lib);
    Arc::new(Program::link(lib, table).expect("links"))
}

fn run(prog: &Arc<Program>) -> (Vm, u32) {
    let mut vm = Vm::new(Arc::clone(prog));
    vm.budget = 100_000;
    let mut host = TestHost::default();
    let this = vm.new_object(&mut host, THING, prog).unwrap();
    let mut cx = Context::new(usize::MAX, 0);
    cx.push(0, tag::INT).unwrap();
    cx.push(this, tag::REFERENCE).unwrap();
    let r = vm.call(
        &mut host,
        &mut cx,
        THING,
        FunctionId::new(THING, 1).direct(),
    );
    assert_eq!(r, Ok(Status::Terminated), "{r:?}");
    (vm, cx.data[0])
}

fn call_native(package: u32, number: u32) -> [u32; 2] {
    [w(op::CALL_NATIVE), (package << 16) | number]
}

/// A hash table asked to optimize its size keeps a bucket count within a factor of two of its
/// entries and answers no; otherwise it moves to the smallest count that holds them.
///
/// Behaviour: script.glue.a-hash-table-optimizes-only-a-size-that-does-not-suit
#[test]
fn a_hash_table_optimizes_only_a_size_that_does_not_suit() {
    const FNS: &[&str] = &["add", "optimize_size"];
    let mut b = Bench::with_packages(&[("AAHash", HASH, FNS)]);
    let table = b.new_object(HASH);
    let shape = |b: &Bench| match b.vm.heap.native(table) {
        Ok(NativeObject::Hash(h)) => (h.len(), h.buckets.len()),
        _ => panic!("a hash table"),
    };
    assert_eq!(shape(&b), (0, 47));
    for key in 0..11 {
        b.answer(HASH, 1, table, &[key, 1]);
    }
    assert_eq!(
        b.answer(HASH, 2, table, &[]),
        1,
        "47 buckets for 11 entries"
    );
    assert_eq!(shape(&b), (11, 11));
    b.answer(HASH, 1, table, &[11, 1]);
    assert_eq!(
        b.answer(HASH, 2, table, &[]),
        0,
        "11 buckets for 12 entries"
    );
    assert_eq!(shape(&b), (12, 11));
}

/// Behaviour: script.glue.hash-table-natives
#[test]
fn a_hash_table_adds_finds_and_walks_its_entries_in_bucket_order() {
    // local0 = new hash (47 buckets); add (1, 4), (3, 2), (50, 3) (3 and 50 in bucket 3); set
    // (3, 7); find 50 into local1; walk with an iterator and sum key * 10 + value into local2.
    let mut b = vec![
        i(op::ENTER, 4),
        w(op::NEW_NATIVE),
        HASH,
        small(tag::INT, 0),
        w(op::STORE),
        w(op::POP),
    ];
    for (k, v, f) in [(1, 4, 1), (3, 2, 1), (50, 3, 1), (3, 7, 2)] {
        b.extend([
            small(tag::INT, 0),
            small(tag::INT, v),
            small(tag::INT, k),
            i(op::LOAD, 0),
        ]);
        b.extend(call_native(HASH, f));
        b.extend([i(op::POP_MANY, 3), w(op::POP)]);
    }
    b.extend([
        small(tag::INT, 0),
        i(op::SLOT_REF, 1),
        small(tag::INT, 50),
        i(op::LOAD, 0),
    ]);
    b.extend(call_native(HASH, 4));
    b.extend([i(op::POP_MANY, 3), w(op::POP)]);
    // local3 = iterator; while notdone: local2 = local2 * 1000 + key * 10 + data; next.
    b.extend([
        small(tag::INT, 0),
        small(tag::INT, 2),
        w(op::STORE),
        w(op::POP),
    ]);
    b.extend([small(tag::INT, 0), i(op::LOAD, 0)]);
    b.extend(call_native(HASH, 6));
    b.extend([w(op::POP), small(tag::INT, 3), w(op::STORE), w(op::POP)]);
    let top = b.len();
    b.extend([small(tag::INT, 0), i(op::LOAD, 3)]);
    b.extend(call_native(HASH_ITER, 4));
    b.extend([w(op::POP)]);
    let test = b.len();
    b.push(0); // patched: if.zero to the end
    b.extend([i(op::LOAD, 2), small(tag::INT, 1000), w(op::MUL)]);
    b.extend([small(tag::INT, 0), i(op::LOAD, 3)]);
    b.extend(call_native(HASH_ITER, 2));
    b.extend([w(op::POP), small(tag::INT, 10), w(op::MUL), w(op::ADD)]);
    b.extend([small(tag::INT, 0), i(op::LOAD, 3)]);
    b.extend(call_native(HASH_ITER, 3));
    b.extend([
        w(op::POP),
        w(op::ADD),
        small(tag::INT, 2),
        w(op::STORE),
        w(op::POP),
    ]);
    b.extend([i(op::LOAD, 3)]);
    b.extend(call_native(HASH_ITER, 1));
    b.extend([w(op::POP)]);
    let back = b.len();
    b.push(0); // patched: jump to the top
    let end = b.len();
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    {
        b[test] = i(op::IF_ZERO, ((end - test - 1) * 4) as i16);
        b[back] = i(op::JUMP, -(((back + 1 - top) * 4) as i16));
    }
    // Return local2 * 10 + local1 (the found value).
    b.extend([i(op::LOAD, 2), i(op::PUSH, 0)]);
    b.pop();
    b.extend([
        w(op::PUSH) | u32::from(tag::INT) << 8,
        10,
        w(op::MUL),
        i(op::LOAD, 1),
        w(op::ADD),
    ]);
    b.extend([small(tag::INT, -2), w(op::STORE), w(op::LEAVE)]);
    let prog = program(b);
    let (vm, v) = run(&prog);
    // Bucket 1, then bucket 3's chain newest first: 1 (4), 50 (3), 3 (7, set in place).
    assert_eq!(v, ((14 * 1000 + 503) * 1000 + 37) * 10 + 3);
    let hashes = (0..vm.heap.capacity() as u32)
        .filter(|id| matches!(vm.heap.native(*id), Ok(NativeObject::Hash(_))))
        .count();
    assert_eq!(hashes, 1);
}

/// Behaviour: script.glue.string-building
#[test]
fn numbers_join_strings_as_the_c_library_writes_them() {
    // build.b("count: ", -42, 0) then build.d(_, 1.5, 1): "1.500000count: -42".
    let mut b = vec![i(op::ENTER, 1)];
    b.extend([
        small(tag::INT, 0),
        small(tag::INT, 0),
        small(tag::INT, -42),
        w(op::NEW_STRING),
        0,
        w(op::LOAD_THIS),
    ]);
    b.extend(call_native(BUILD, 2));
    b.extend([
        i(op::POP_MANY, 4),
        small(tag::INT, 0),
        w(op::STORE),
        w(op::POP),
    ]);
    b.extend([
        small(tag::INT, 0),
        small(tag::INT, 1),
        w(op::PUSH) | u32::from(tag::FLOAT) << 8,
        1.5f32.to_bits(),
        i(op::LOAD, 0),
        w(op::LOAD_THIS),
    ]);
    b.extend(call_native(BUILD, 4));
    b.extend([
        i(op::POP_MANY, 4),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ]);
    let prog = program(b);
    let (vm, v) = run(&prog);
    let NativeObject::Text(t) = vm.heap.native(v).unwrap() else {
        panic!("a string")
    };
    assert_eq!(String::from_utf16(t).unwrap(), "1.500000count: -42");
}

/// Push a native call's result slot, `args` (pushed last first by the caller), the receiver
/// from local `this`; call; pop the arguments and receiver, leaving the result.
fn native_call(b: &mut Vec<u32>, this: i16, args: &[u32], package: u32, number: u32) {
    b.push(small(tag::INT, 0));
    b.extend(args.iter().rev());
    b.push(i(op::LOAD, this));
    b.extend(call_native(package, number));
    b.push(i(op::POP_MANY, args.len() as i16 + 1));
}

/// A native call with no result slot (an add).
fn native_do(b: &mut Vec<u32>, this: i16, args: &[u32], package: u32, number: u32) {
    b.extend(args.iter().rev());
    b.push(i(op::LOAD, this));
    b.extend(call_native(package, number));
    b.push(i(op::POP_MANY, args.len() as i16 + 1));
}

/// Store the top of the stack into local `k`.
fn store(b: &mut Vec<u32>, k: i16) {
    b.extend([small(tag::INT, k), w(op::STORE), w(op::POP)]);
}

/// `acc = acc * scale + top`, `acc` in local `k`.
fn accumulate(b: &mut Vec<u32>, k: i16, scale: i16) {
    b.extend([
        i(op::LOAD, k),
        small(tag::INT, scale),
        w(op::MUL),
        w(op::ADD),
    ]);
    store(b, k);
}

fn new_multi_hash(b: &mut Vec<u32>, pairs: &[(i16, i16)]) {
    b.extend([w(op::NEW_NATIVE), MULTI]);
    store(b, 0);
    for (k, v) in pairs {
        native_do(b, 0, &[small(tag::INT, *k), small(tag::INT, *v)], MULTI, 1);
    }
}

fn finish(b: &mut Vec<u32>, k: i16) {
    b.extend([
        i(op::LOAD, k),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ]);
}

/// Behaviour: script.glue.multi-hash-natives
#[test]
fn a_multi_hash_keeps_a_keys_entries_together_after_its_first() {
    // Key 1 gets 10, 20, 30 (they walk 10, 30, 20); key 2 gets 40; key 194 shares key 1's bucket
    // of 193. Walk key 1 from its first entry to where its entries end, then count and size.
    let mut b = vec![i(op::ENTER, 4)];
    new_multi_hash(&mut b, &[(1, 10), (1, 20), (1, 30), (2, 40), (194, 50)]);
    b.extend([small(tag::INT, 0)]);
    store(&mut b, 3);
    native_call(&mut b, 0, &[small(tag::INT, 1)], MULTI, 4);
    store(&mut b, 1);
    native_call(&mut b, 0, &[small(tag::INT, 1)], MULTI, 5);
    store(&mut b, 2);
    for _ in 0..3 {
        native_call(&mut b, 1, &[], MULTI_ITER, 3);
        accumulate(&mut b, 3, 100);
        native_do(&mut b, 1, &[], MULTI_ITER, 1);
    }
    native_call(&mut b, 1, &[i(op::LOAD, 2)], MULTI_ITER, 4);
    accumulate(&mut b, 3, 10);
    native_call(&mut b, 0, &[small(tag::INT, 1)], MULTI, 2);
    accumulate(&mut b, 3, 10);
    native_call(&mut b, 0, &[], MULTI, 6);
    accumulate(&mut b, 3, 10);
    finish(&mut b, 3);
    let (vm, v) = run(&program(b));
    // 10, 30, 20; at the end (1); 3 entries of key 1; 5 in all.
    assert_eq!(v, 103_020_135);
    let tables = (0..vm.heap.capacity() as u32)
        .filter(|id| matches!(vm.heap.native(*id), Ok(NativeObject::MultiHash(_))))
        .count();
    assert_eq!(tables, 1);

    // Erase the entry an iterator stands at (key 1's first, 10): key 1 then starts at 20; key 194
    // heads the shared chain and ends where key 1 begins; removing key 1 removes 1 entry.
    let mut b = vec![i(op::ENTER, 4)];
    new_multi_hash(&mut b, &[(1, 10), (1, 20), (194, 50)]);
    b.extend([small(tag::INT, 0)]);
    store(&mut b, 3);
    native_call(&mut b, 0, &[small(tag::INT, 1)], MULTI, 4);
    store(&mut b, 1);
    native_do(&mut b, 0, &[i(op::LOAD, 1)], MULTI, 3);
    for (key, number) in [(1, 4), (194, 4), (194, 5)] {
        native_call(&mut b, 0, &[small(tag::INT, key)], MULTI, number);
        store(&mut b, 1);
        native_call(&mut b, 1, &[], MULTI_ITER, 3);
        accumulate(&mut b, 3, 100);
    }
    native_call(&mut b, 0, &[small(tag::INT, 1)], MULTI, 7);
    accumulate(&mut b, 3, 10);
    native_call(&mut b, 0, &[], MULTI, 6);
    accumulate(&mut b, 3, 10);
    finish(&mut b, 3);
    let (_, v) = run(&program(b));
    assert_eq!(v, 20_502_011);
}

/// Behaviour: script.glue.adaptor-fields
#[test]
fn an_adaptor_keeps_what_its_setters_give_and_a_fellowship_adds_a_fellow_once() {
    // A craft skill line's id set to 77 reads back; a fellow added twice under id 5 is added once.
    let mut b = vec![i(op::ENTER, 4)];
    b.extend([w(op::NEW_NATIVE), CRAFT_SKILL]);
    store(&mut b, 0);
    native_do(&mut b, 0, &[small(tag::INT, 77)], CRAFT_SKILL, 2);
    native_call(&mut b, 0, &[], CRAFT_SKILL, 1);
    store(&mut b, 3);
    b.extend([w(op::NEW_NATIVE), FELLOWSHIP]);
    store(&mut b, 1);
    b.extend([w(op::NEW_NATIVE), FELLOW]);
    store(&mut b, 2);
    for _ in 0..2 {
        native_call(
            &mut b,
            1,
            &[small(tag::INT, 0), small(tag::INT, 5), i(op::LOAD, 2)],
            FELLOWSHIP,
            1,
        );
        accumulate(&mut b, 3, 10);
    }
    finish(&mut b, 3);
    let (_, v) = run(&program(b));
    assert_eq!(v, 7710);
}

/// A literal a script gives a string reference is kept with its meta language marks escaped, so
/// the reference writes it back as it was given; a string's token folds its units seven bits
/// apart.
///
/// Behaviour: script.glue.string-references-write
#[test]
fn a_literal_string_reference_writes_back_as_given_and_a_string_has_its_token() {
    // A new string reference, given the literal "a[b] and {c}", asked for its text.
    let mut b = vec![i(op::ENTER, 2)];
    b.extend([w(op::NEW_NATIVE), TEXT_INFO]);
    store(&mut b, 0);
    b.extend([w(op::NEW_STRING), 1]);
    store(&mut b, 1);
    native_do(&mut b, 0, &[i(op::LOAD, 1)], TEXT_INFO, 0x12);
    native_call(&mut b, 0, &[], TEXT_INFO, 0x10);
    store(&mut b, 1);
    finish(&mut b, 1);
    let (vm, v) = run(&program(b));
    let NativeObject::Text(t) = vm.heap.native(v).unwrap() else {
        panic!("a string")
    };
    assert_eq!(String::from_utf16(t).unwrap(), "a[b] and {c}");

    let mut b = vec![i(op::ENTER, 1)];
    b.extend([w(op::NEW_STRING), 1]);
    store(&mut b, 0);
    native_call(&mut b, 0, &[i(op::LOAD, 0)], TOKENIZE, 1);
    store(&mut b, 0);
    finish(&mut b, 0);
    let (_, v) = run(&program(b));
    let units: Vec<u16> = "a[b] and {c}".encode_utf16().collect();
    assert_eq!(v, auberean_glue::text::token_of(&units));
    assert_eq!(
        auberean_glue::text::token_of(&"abc".encode_utf16().collect::<Vec<_>>()),
        0xC38B_1800
    );
}

/// Behaviour: script.glue.shop-sales-in-ordinal-order
#[test]
fn a_shop_keeps_its_sales_in_ordinal_order_ties_in_the_order_added() {
    // Sales with ordinals 5, 2, 5 and none (0), stock quantities 1 to 4, added in that order.
    let mut b = vec![i(op::ENTER, 2)];
    b.extend([w(op::NEW_NATIVE), SHOP]);
    store(&mut b, 0);
    for (ordinal, stock) in [(Some(5), 1), (Some(2), 2), (Some(5), 3), (None, 4)] {
        b.extend([w(op::NEW_NATIVE), SHOP_SALE]);
        store(&mut b, 1);
        if let Some(o) = ordinal {
            native_do(&mut b, 1, &[small(tag::INT, o)], SHOP_SALE, 0x0A);
        }
        native_do(&mut b, 1, &[small(tag::INT, stock)], SHOP_SALE, 0x09);
        native_do(&mut b, 0, &[i(op::LOAD, 1)], SHOP, 6);
    }
    finish(&mut b, 0);
    let (vm, v) = run(&program(b));
    let NativeObject::Adaptor { fields, .. } = vm.heap.native(v).unwrap() else {
        panic!("an adaptor")
    };
    let Some(Field::Many(sales)) = fields.get("Sale") else {
        panic!("a list of sales")
    };
    let stocks: Vec<u32> = sales
        .iter()
        .map(|s| match s {
            Field::Value(a) => match &**a {
                NativeObject::Adaptor { fields, .. } => match fields.get("StockQuantity") {
                    Some(Field::Word(w)) => *w,
                    _ => 0,
                },
                _ => 0,
            },
            _ => 0,
        })
        .collect();
    assert_eq!(stocks, [4, 2, 1, 3]);
}

/// A new floating damage text shows for -1 (the default); its duration and scale are stored as the
/// floating-point unit leaves them, a signalling not-a-number made quiet.
///
/// Behaviour: script.glue.damage-text-natives
#[test]
fn a_new_damage_text_lasts_minus_one_and_keeps_its_floats_as_the_fpu_leaves_them() {
    use auberean_script::native_object::DamageText;
    // local0 = new text; local1 = its duration; its scale = local1; its duration = a signalling
    // not-a-number.
    let mut b = vec![i(op::ENTER, 2), w(op::NEW_NATIVE), DAMAGE_TEXT];
    store(&mut b, 0);
    native_call(&mut b, 0, &[], DAMAGE_TEXT, 6);
    store(&mut b, 1);
    native_do(&mut b, 0, &[i(op::LOAD, 1)], DAMAGE_TEXT, 7);
    b.extend([
        w(op::PUSH) | u32::from(tag::FLOAT) << 8,
        0x7F80_0001,
        i(op::LOAD, 0),
    ]);
    b.extend(call_native(DAMAGE_TEXT, 5));
    b.push(i(op::POP_MANY, 2));
    finish(&mut b, 0);
    let (vm, v) = run(&program(b));
    let NativeObject::DamageText(d) = vm.heap.native(v).unwrap() else {
        panic!("a damage text")
    };
    let expected = DamageText {
        scale: (-1.0f32).to_bits(),
        duration: 0x7FC0_0001,
        ..DamageText::default()
    };
    assert_eq!(**d, expected);
}

/// A word is one of a text's words when a piece of the text split at spaces, commas and semicolons
/// is the word, ASCII letters compared without their case; part of a piece is not.
///
/// Behaviour: script.glue.a-word-is-found-among-a-texts-words
#[test]
fn a_word_is_one_of_a_texts_words_split_at_spaces_commas_and_semicolons() {
    // locals 1-5: "a[b] and {c}", "AND", "b", "Wave, smile;bow", "bow"; local0 = three answers.
    let mut b = vec![i(op::ENTER, 6)];
    for (k, lit) in [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)] {
        b.extend([w(op::NEW_STRING), lit]);
        store(&mut b, k);
    }
    b.extend([small(tag::INT, 0)]);
    store(&mut b, 0);
    for (word, text) in [(2, 1), (3, 1), (5, 4)] {
        native_call(
            &mut b,
            1,
            &[i(op::LOAD, word), i(op::LOAD, text)],
            STRING_UTILS,
            1,
        );
        accumulate(&mut b, 0, 10);
    }
    finish(&mut b, 0);
    let (_, v) = run(&program(b));
    assert_eq!(v, 101);
}

/// Vector arithmetic at double precision, each result kept as a float: the cross product a new
/// vector; the dot product (its last term restoring what a float sum would lose); the length;
/// adding and taking away in place; a copy; equal when every part is within 0.0002; the squared
/// length answering nothing.
///
/// Behaviour: script.glue.vector-arithmetic
#[test]
fn vector_arithmetic_works_at_double_precision_and_keeps_floats() {
    let float = |v: f32| [w(op::PUSH) | u32::from(tag::FLOAT) << 8, v.to_bits()];
    // local k = a new vector (x, y, z).
    let set = |b: &mut Vec<u32>, k: i16, v: [f32; 3]| {
        b.extend([w(op::NEW_NATIVE), VECTOR]);
        store(b, k);
        for x in v.iter().rev() {
            b.extend(float(*x));
        }
        b.push(i(op::LOAD, k));
        b.extend(call_native(VECTOR, 6));
        b.push(i(op::POP_MANY, 4));
    };
    let run_with = |f: &dyn Fn(&mut Vec<u32>)| {
        let mut b = vec![i(op::ENTER, 4)];
        set(&mut b, 0, [1.5, -2.0, 0.25]);
        set(&mut b, 1, [3.0, 0.5, -1.0]);
        f(&mut b);
        let (vm, v) = run(&program(b));
        (vm, v)
    };
    let vector = |vm: &Vm, r: u32| match vm.heap.native(r).unwrap() {
        NativeObject::Vector(p) => p.map(f32::from_bits),
        _ => panic!("a vector"),
    };
    // The cross product, a new vector.
    let (vm, r) = run_with(&|b| {
        native_call(b, 0, &[i(op::LOAD, 1)], VECTOR, 1);
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(vector(&vm, r), [1.875, 2.25, 6.75]);
    // The dot product, and one only double precision gets right.
    let (_, d) = run_with(&|b| {
        native_call(b, 0, &[i(op::LOAD, 1)], VECTOR, 2);
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(f32::from_bits(d), 3.25);
    let (_, d) = run_with(&|b| {
        set(b, 2, [1.0e8, 1.0, -1.0e8]);
        set(b, 3, [1.0, 1.0, 1.0]);
        native_call(b, 2, &[i(op::LOAD, 3)], VECTOR, 2);
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(f32::from_bits(d), 1.0);
    // The length.
    let (_, l) = run_with(&|b| {
        native_call(b, 1, &[], VECTOR, 3);
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(f32::from_bits(l), (10.25f64.sqrt()) as f32);
    // Added, then taken away twice, in place.
    let (vm, r) = run_with(&|b| {
        native_do(b, 0, &[i(op::LOAD, 1)], VECTOR, 7);
        native_do(b, 0, &[i(op::LOAD, 1)], VECTOR, 8);
        native_do(b, 0, &[i(op::LOAD, 1)], VECTOR, 8);
        finish(b, 0);
    });
    assert_eq!(vector(&vm, r), [-1.5, -2.5, 1.25]);
    // A copy, then equal to what it copied; equal within 0.0002, not at it.
    let (_, e) = run_with(&|b| {
        native_do(b, 0, &[i(op::LOAD, 1)], VECTOR, 0x0A);
        native_call(b, 0, &[i(op::LOAD, 1)], VECTOR, 0x0B);
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(e, 1);
    for (dz, want) in [(0.000_19, 1), (0.000_21, 0)] {
        let (_, e) = run_with(&|b| {
            set(b, 2, [3.0, 0.5, -1.0 + dz]);
            native_call(b, 1, &[i(op::LOAD, 2)], VECTOR, 0x0B);
            store(b, 3);
            finish(b, 3);
        });
        assert_eq!(e, want, "{dz}");
    }
    // The squared length writes nothing below the receiver.
    let (_, n) = run_with(&|b| {
        b.push(small(tag::INT, 7));
        b.push(i(op::LOAD, 1));
        b.extend(call_native(VECTOR, 4));
        b.push(i(op::POP_MANY, 1));
        store(b, 2);
        finish(b, 2);
    });
    assert_eq!(n, 7);
}

/// A vector set from three floats keeps them as the floating-point unit leaves them (a signalling
/// not-a-number made quiet), and each part reads back as stored.
///
/// Behaviour: script.glue.vector-set-and-parts
#[test]
fn a_vector_set_from_three_floats_reads_back_each_part() {
    let set = |b: &mut Vec<u32>| {
        b.extend([w(op::NEW_NATIVE), VECTOR]);
        store(b, 0);
        let float = |v: f32| [w(op::PUSH) | u32::from(tag::FLOAT) << 8, v.to_bits()];
        // Pushed z, y, x: x lands nearest the receiver.
        b.extend(float(-2.0));
        b.extend([w(op::PUSH) | u32::from(tag::FLOAT) << 8, 0x7F80_0001]);
        b.extend(float(1.5));
        b.push(i(op::LOAD, 0));
        b.extend(call_native(VECTOR, 6));
        b.push(i(op::POP_MANY, 4));
    };
    let mut b = vec![i(op::ENTER, 2)];
    set(&mut b);
    finish(&mut b, 0);
    let (vm, v) = run(&program(b));
    let NativeObject::Vector(p) = vm.heap.native(v).unwrap() else {
        panic!("a vector")
    };
    assert_eq!(*p, [1.5f32.to_bits(), 0x7FC0_0001, (-2.0f32).to_bits()]);
    for (k, want) in [(0x0C, 1.5f32.to_bits()), (0x0E, (-2.0f32).to_bits())] {
        let mut b = vec![i(op::ENTER, 2)];
        set(&mut b);
        native_call(&mut b, 0, &[], VECTOR, k);
        store(&mut b, 1);
        finish(&mut b, 1);
        let (_, got) = run(&program(b));
        assert_eq!(got, want);
    }
}

/// A new position and a new vector into locals `k` and `k + 1`: the position at `cell`, its
/// origin set to `origin` through the vector (local 5 is scratch).
fn new_position(b: &mut Vec<u32>, k: i16, cell: u32, origin: [f32; 3]) {
    let float = |v: f32| [w(op::PUSH) | u32::from(tag::FLOAT) << 8, v.to_bits()];
    b.extend([w(op::NEW_NATIVE), POSITION]);
    store(b, k);
    b.extend([w(op::NEW_NATIVE), VECTOR]);
    store(b, k + 1);
    for x in origin.iter().rev() {
        b.extend(float(*x));
    }
    b.push(i(op::LOAD, k + 1));
    b.extend(call_native(VECTOR, 6));
    b.push(i(op::POP_MANY, 4));
    // Words pushed whole go through a local: the call helpers push one-word instructions.
    b.extend([w(op::PUSH) | u32::from(tag::INT) << 8, cell]);
    store(b, 5);
    native_do(b, k, &[i(op::LOAD, 5)], POSITION, 4);
    native_do(b, k, &[i(op::LOAD, k + 1)], POSITION, 8);
}

/// A position's parts are read and set whole (its cell, its origin, its height, a copy of
/// another's); two are in one landblock when their cells' top halves agree (two of cell 0 are, one
/// of cell 0 and another are not); its landblock's own cell; whether it is in the world and
/// outside; equal when their cells are and every part of their frames is within 0.0002; the
/// distances between two in landblocks a row apart, kept as floats; offsets added in the
/// landblock's frame and in the position's own (turned by its rotation); the client's position of
/// nothing; and the position written out with six decimals. Before the region is known no cell is
/// valid and none has map coordinates.
///
/// Behaviour: script.glue.a-positions-parts-distances-and-text
#[test]
fn a_positions_parts_are_read_and_set_and_its_distances_kept_as_floats() {
    let float = |v: f32| [w(op::PUSH) | u32::from(tag::FLOAT) << 8, v.to_bits()];
    // Two positions: one in landblock 0x0102 and one in the landblock north of it.
    let two = |b: &mut Vec<u32>| {
        new_position(b, 0, 0x0102_0001, [10.0, 20.0, 0.5]);
        new_position(b, 2, 0x0103_0009, [13.0, 4.0, -1.5]);
    };
    let ask = |f: &dyn Fn(&mut Vec<u32>)| {
        let mut b = vec![i(op::ENTER, 6)];
        two(&mut b);
        f(&mut b);
        store(&mut b, 4);
        finish(&mut b, 4);
        run(&program(b))
    };
    let word = |f: &dyn Fn(&mut Vec<u32>)| ask(f).1;
    let as_float = |f: &dyn Fn(&mut Vec<u32>)| f32::from_bits(word(f));
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 3)), 0x0102_0001);
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 6)), 0x0102_FFFF);
    assert_eq!(
        word(&|b| native_call(b, 0, &[], POSITION, 0x1E)),
        0.5f32.to_bits()
    );
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 0x16)), 1);
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 9)), 0);
    assert_eq!(
        word(&|b| native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 5)),
        0
    );
    // The offset to the second: across 3, up 160 - 16, down 2.
    let offset = (3.0f64, 144.0f64, -2.0f64);
    let squared = offset.2 * offset.2 + offset.1 * offset.1 + offset.0 * offset.0;
    assert_eq!(
        as_float(&|b| native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 0x0B)),
        squared as f32
    );
    let across = offset.0 * offset.0 + offset.1 * offset.1;
    assert_eq!(
        as_float(&|b| native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 0x23)),
        across as f32
    );
    assert_eq!(
        as_float(&|b| native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 0x22)),
        across.sqrt() as f32
    );
    let vector = |vm: &Vm, r: u32| match vm.heap.native(r).unwrap() {
        NativeObject::Vector(p) => p.map(f32::from_bits),
        _ => panic!("a vector"),
    };
    let (vm, r) = ask(&|b| native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 1));
    assert_eq!(vector(&vm, r), [3.0, 144.0, -2.0]);
    let (vm, r) = ask(&|b| {
        b.extend([w(op::PUSH) | u32::from(tag::INT) << 8, 0x0302_0001]);
        store(b, 5);
        native_call(b, 0, &[i(op::LOAD, 5)], POSITION, 7);
    });
    assert_eq!(vector(&vm, r), [320.0, 0.0, 0.0]);
    let (vm, r) = ask(&|b| native_call(b, 2, &[], POSITION, 2));
    assert_eq!(vector(&vm, r), [13.0, 4.0, -1.5]);
    // A copy: equal, and in one landblock; its height moved a little, still equal, then not.
    let copy = |b: &mut Vec<u32>| native_do(b, 2, &[i(op::LOAD, 0)], POSITION, 0x1C);
    assert_eq!(
        word(&|b| {
            copy(b);
            native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 0x17);
        }),
        1
    );
    assert_eq!(
        word(&|b| {
            copy(b);
            native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 5);
        }),
        1
    );
    for (height, want) in [(0.500_19, 1), (0.500_21, 0)] {
        assert_eq!(
            word(&|b| {
                copy(b);
                b.extend(float(height));
                store(b, 5);
                native_do(b, 2, &[i(op::LOAD, 5)], POSITION, 0x1D);
                native_call(b, 0, &[i(op::LOAD, 2)], POSITION, 0x17);
            }),
            want,
            "{height}"
        );
    }
    // Offsets added: in the landblock's frame, then in the position's own (no turn: the same).
    let (vm, r) = ask(&|b| {
        native_do(b, 0, &[i(op::LOAD, 3)], POSITION, 0x1A);
        native_do(b, 0, &[i(op::LOAD, 3)], POSITION, 0x19);
        native_call(b, 0, &[], POSITION, 2);
    });
    assert_eq!(vector(&vm, r), [36.0, 28.0, -2.5]);
    // The position of nothing: cell 0, not in the world.
    assert_eq!(
        word(&|b| {
            native_call(b, 0, &[], POSITION, 0x18);
            store(b, 5);
            native_call(b, 5, &[], POSITION, 9);
        }),
        1
    );
    let text = |vm: &Vm, r: u32| match vm.heap.native(r).unwrap() {
        NativeObject::Text(t) => String::from_utf16_lossy(t),
        _ => panic!("a string"),
    };
    let (vm, r) = ask(&|b| native_call(b, 0, &[], POSITION, 0x10));
    assert_eq!(
        text(&vm, r),
        "0x01020001 [10.000000 20.000000 0.500000] 1.000000 0.000000 0.000000 0.000000"
    );
    // No region: no cell is valid, none has map coordinates or land coordinates.
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 0x21)), 0);
    let (vm, r) = ask(&|b| native_call(b, 0, &[], POSITION, 0x13));
    assert_eq!(text(&vm, r), "<error>");
    let (vm, r) = ask(&|b| native_call(b, 0, &[], POSITION, 0x27));
    assert_eq!(text(&vm, r), "<error>");
    assert_eq!(word(&|b| native_call(b, 0, &[], POSITION, 0x29)), u32::MAX);
}

/// A multi-map keeps its entries in key order, each key's in the order they came; `find` is a
/// key's first entry, `find_last` the place after its last (where it would be when absent), the
/// bounds are the first not less and the first greater; an iterator stays on its entry while
/// other entries come and go, steps back from past the last to the last, and two iterators are
/// equal at the same place of one map; erasing at an iterator, removing a key and clearing take
/// entries out; a long-keyed map orders its keys as unsigned numbers.
///
/// Behaviour: script.glue.multi-map-natives
#[test]
fn a_multi_map_keeps_its_entries_in_key_order_and_its_iterators_on_their_entries() {
    const MAP: u32 = 0x1AC3;
    const ITER: u32 = 0x1AC4;
    const LONG_MAP: u32 = 0x1ACC;
    const LONG_ITER: u32 = 0x1ACD;
    const MAP_FNS: &[&str] = &[
        "insert",
        "clear",
        "empty",
        "count",
        "erase",
        "remove",
        "find",
        "find_last",
        "get_begin",
        "get_end",
        "lower_bound",
        "upper_bound",
    ];
    const ITER_FNS: &[&str] = &["prev", "next", "key", "data", "equals"];
    let (insert, clear, empty, count, erase, remove, find, find_last, begin, end, lower, upper) =
        (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
    let (prev, next, key, data, equals) = (1, 2, 3, 4, 5);
    let mut b = crate::call::Bench::with_packages(&[
        ("AAMultiMap", MAP, MAP_FNS),
        ("AAMultiMapIter", ITER, ITER_FNS),
        ("LLMultiMap", LONG_MAP, MAP_FNS),
        ("LLMultiMapIter", LONG_ITER, ITER_FNS),
    ]);
    let m = b.new_object(MAP);
    assert_eq!(b.answer(MAP, empty, m, &[]), 1);
    for (k, v) in [(5, 50), (2, 20), (5, 51), (9, 90), (2, 21), (5, 52)] {
        assert_eq!(b.call(MAP, insert, m, &[k, v], 0).0, Status::Success);
    }
    assert_eq!(b.answer(MAP, empty, m, &[]), 0);
    assert_eq!(b.answer(MAP, count, m, &[5]), 3);
    // Walk from the first entry to past the last.
    let walk = |b: &mut crate::call::Bench, it: u32| -> Vec<u32> {
        let stop = b.answer(MAP, end, m, &[]);
        let mut out = Vec::new();
        while b.answer(ITER, equals, it, &[stop]) == 0 {
            out.push(b.answer(ITER, key, it, &[]) * 100 + b.answer(ITER, data, it, &[]));
            b.call(ITER, next, it, &[], 0);
        }
        out
    };
    let it = b.answer(MAP, begin, m, &[]);
    assert_eq!(walk(&mut b, it), [220, 221, 550, 551, 552, 990]);
    let at = |b: &mut crate::call::Bench, f: u32, k: u32| -> Option<u32> {
        let it = b.answer(MAP, f, m, &[k]);
        let stop = b.answer(MAP, end, m, &[]);
        (b.answer(ITER, equals, it, &[stop]) == 0).then(|| b.answer(ITER, data, it, &[]))
    };
    assert_eq!(at(&mut b, find, 5), Some(50));
    assert_eq!(at(&mut b, find, 7), None);
    assert_eq!(at(&mut b, find_last, 5), Some(90), "after the last 5");
    assert_eq!(at(&mut b, find_last, 7), Some(90), "where 7 would be");
    assert_eq!(at(&mut b, find_last, 9), None);
    assert_eq!(at(&mut b, lower, 3), Some(50));
    assert_eq!(at(&mut b, upper, 2), Some(50));
    assert_eq!(at(&mut b, upper, 9), None);
    // An iterator on 51 stays there while others come and go.
    let on = b.answer(MAP, find, m, &[5]);
    b.call(ITER, next, on, &[], 0);
    assert_eq!(b.answer(ITER, data, on, &[]), 51);
    b.call(MAP, insert, m, &[5, 53], 0);
    b.call(MAP, insert, m, &[1, 10], 0);
    let first = b.answer(MAP, find, m, &[5]);
    assert_eq!(b.call(MAP, erase, m, &[first], 0).0, Status::Success);
    assert_eq!(b.answer(ITER, data, on, &[]), 51);
    b.call(ITER, prev, on, &[], 0);
    assert_eq!(b.answer(ITER, data, on, &[]), 21);
    // From past the last, back to the last.
    let back = b.answer(MAP, end, m, &[]);
    b.call(ITER, prev, back, &[], 0);
    assert_eq!(b.answer(ITER, data, back, &[]), 90);
    // Two iterators past the last of different maps differ.
    let other = b.new_object(MAP);
    let (e1, e2) = (b.answer(MAP, end, m, &[]), b.answer(MAP, end, other, &[]));
    assert_eq!(b.answer(ITER, equals, e1, &[e2]), 0);
    let it = b.answer(MAP, begin, m, &[]);
    assert_eq!(walk(&mut b, it), [110, 220, 221, 551, 552, 553, 990]);
    b.call(MAP, remove, m, &[5], 0);
    assert_eq!(b.answer(MAP, count, m, &[5]), 0);
    let it = b.answer(MAP, begin, m, &[]);
    assert_eq!(walk(&mut b, it), [110, 220, 221, 990]);
    b.call(MAP, clear, m, &[], 0);
    assert_eq!(b.answer(MAP, empty, m, &[]), 1);
    // A 64-bit key: high word nearest, then the low word; keys order as unsigned numbers.
    let l = b.new_object(LONG_MAP);
    for (hi, lo, v) in [(1u32, 0u32, 7u32), (0, 0xFFFF_FFFF, 6), (0, 3, 5)] {
        b.call(LONG_MAP, insert, l, &[hi, lo, 0, v], 0);
    }
    let it = b.answer(LONG_MAP, begin, l, &[]);
    let mut values = Vec::new();
    for _ in 0..3 {
        let (_, slots) = b.call(LONG_ITER, data, it, &[], 2);
        values.push(slots[1].0);
        b.call(LONG_ITER, next, it, &[], 0);
    }
    assert_eq!(values, [5, 6, 7]);
}

/// Behaviour: script.glue.file-names-and-tracks-by-name
#[test]
fn a_file_name_finds_its_id_by_its_extension_and_a_keyframe_takes_its_tracks_by_name() {
    use auberean_script::native_object::native_type;
    const FILES: u32 = 0x27;
    const KEY: u32 = 0x9F;
    const IMAGE: u32 = 0x4000_003C;
    const SOUND: u32 = 0x4000_002D;
    let mut b = crate::call::Bench::new();
    b.host
        .names
        .insert(("Story_Black01.jpg".to_owned(), IMAGE), 0x0600_1234);
    b.host
        .names
        .insert(("  SQ_Intro.mif ".to_owned(), SOUND), 0x0A00_0042);
    // The type is read from the name trimmed; the table is asked with the name as given.
    let jpg = b.text("Story_Black01.jpg");
    assert_eq!(b.answer(FILES, 0x02, NULL, &[jpg]), 0x0600_1234);
    let mif = b.text("  SQ_Intro.mif ");
    assert_eq!(b.answer(FILES, 0x02, NULL, &[mif]), 0x0A00_0042);
    // A name with no known extension answers 0 without asking the table.
    b.host.names.insert(("Story_Black01".to_owned(), 0), 7);
    let bare = b.text("Story_Black01");
    assert_eq!(b.answer(FILES, 0x02, NULL, &[bare]), 0);
    // By name and an explicit type: the table's answer for that pair.
    let (_, slots) = b.call(FILES, 0x03, NULL, &[jpg, IMAGE], 1);
    assert_eq!(slots[0].0, 0x0600_1234);
    let (_, slots) = b.call(FILES, 0x03, NULL, &[jpg, SOUND], 1);
    assert_eq!(slots[0].0, 0);
    // An id by an enumeration value (`fp[-2]`) in a mapping (`fp[-3]`).
    b.host.enum_ids.insert((5, 0x4000_0004), 0x2300_0005);
    let (_, slots) = b.call(FILES, 0x01, NULL, &[5, 0x4000_0004], 1);
    assert_eq!(slots[0].0, 0x2300_0005);
    // A file's name by its id, as a new string.
    let (status, slots) = b.call(FILES, 0x04, NULL, &[0x0600_1234], 1);
    assert_eq!(status, Status::Success);
    assert_eq!(slots[0].1, tag::REFERENCE);
    assert_eq!(
        String::from_utf16(&b.text_of(slots[0].0)).unwrap(),
        "Story_Black01.jpg"
    );
    // A keyframe's tracks by name: the id, and the track marked defined.
    let key = b.new_object(native_type::KEYFRAME);
    assert_eq!(b.call(KEY, 0x09, key, &[jpg], 0).0, Status::Success);
    b.call(KEY, 0x0F, key, &[mif], 0);
    let unknown = b.text("Story_Missing.jpg");
    b.call(KEY, 0x0C, key, &[unknown], 0);
    match b.vm.heap.native(key).unwrap() {
        NativeObject::Keyframe(k) => {
            assert_eq!((k.video, k.voice, k.audio), (0x0600_1234, 0x0A00_0042, 0));
            // A name the table does not hold still defines the track, as empty.
            assert_eq!(k.tracks, 0x1 | 0x2 | 0x4);
        }
        o => panic!("not a keyframe: {o:?}"),
    }
    // A null keyframe crashes the call.
    assert_eq!(b.call(KEY, 0x09, NULL, &[jpg], 0).0, Status::Crash);
    // A host with no table (an install of the relaunched game) answers 0 and an empty name.
    let mut empty = crate::call::Bench::new();
    let jpg = empty.text("Story_Black01.jpg");
    assert_eq!(empty.answer(FILES, 0x02, NULL, &[jpg]), 0);
    let (_, slots) = empty.call(FILES, 0x04, NULL, &[0x0600_1234], 1);
    assert!(empty.text_of(slots[0].0).is_empty());
}

/// The ids a set of record ids holds, in its order.
fn record_ids(b: &crate::call::Bench, set: u32) -> Vec<u32> {
    match b.vm.heap.native(set).expect("an object") {
        NativeObject::Engine(e) => e
            .get::<auberean_glue::data_ids::DataIds>()
            .expect("a set of record ids")
            .0
            .iter()
            .map(|q| q.id)
            .collect(),
        o => panic!("{set:#x} is not a set of record ids: {o:?}"),
    }
}

/// **The records a saved state needs are gathered into a set of ids**: each id once, in id order,
/// never id 0, each add answering whether it added; a string reference adds its table and each
/// nested reference's (a literal and a number add nothing); a scene adds each keyframe's video,
/// audio and voice records and its subtitle's table, the tracks left out when the type asked
/// about is the one they are never fetched for; a race and sex's look adds its body, its
/// appearance table's records, its pile's look and its music. A null receiver crashes each call.
/// Fixture: each native called as the machine calls one, the set held in a local slot.
/// Behaviour: script.glue.a-saved-states-records-are-gathered
#[test]
fn the_records_a_saved_state_needs_are_gathered_once_each_in_id_order() {
    use auberean_script::native_object::{TextInfo, Variable, VariableValue};
    use auberean_script::story::{Keyframe, SceneInfo};

    const SET: u32 = 0x5E;
    let mut b = crate::call::Bench::new();
    let set = b.object(SET, NativeObject::Opaque { ty: SET });
    assert_eq!(b.answer(SET, 1, set, &[0x2500_0010]), 1);
    assert_eq!(b.answer(SET, 1, set, &[0x0600_0001]), 1);
    assert_eq!(b.answer(SET, 1, set, &[0x2500_0010]), 0, "held already");
    assert_eq!(b.answer(SET, 1, set, &[0]), 0, "id 0");
    assert_eq!(record_ids(&b, set), [0x0600_0001, 0x2500_0010]);
    assert_eq!(b.call(SET, 1, NULL, &[5], 1).0, Status::Crash);

    // A string reference: its table and its nested references' tables.
    let table = |t: u32| TextInfo {
        table: t,
        ..TextInfo::default()
    };
    let literal = TextInfo {
        has_literal: 1,
        literal: Some("x".encode_utf16().collect()),
        ..table(0x2500_0030)
    };
    let nested = |id: u32, t: TextInfo| Variable {
        id,
        kind: 9,
        value: VariableValue::Nested(Box::new(t)),
    };
    let info = TextInfo {
        variables: vec![
            nested(1, table(0x2500_0020)),
            nested(2, literal),
            Variable {
                id: 3,
                kind: 6,
                value: VariableValue::Word(0x2500_0040),
            },
        ],
        ..table(0x2500_0011)
    };
    let set = b.object(SET, NativeObject::Opaque { ty: SET });
    let r = b.object(0x6A, NativeObject::TextInfo(info));
    let (status, _) = b.call_with_local(0x6A, 0x1C, r, set, &[Some(0), None], 0);
    assert_eq!(status, Status::Success);
    assert_eq!(record_ids(&b, set), [0x2500_0011, 0x2500_0020]);
    let (status, _) = b.call_with_local(0x6A, 0x1C, NULL, set, &[Some(0), None], 0);
    assert_eq!(status, Status::Crash);

    // A scene: each keyframe's tracks and subtitle.
    let key = |n: u32| Keyframe {
        video: 0x0A00_0000 + n,
        audio: 0x0A00_0100 + n,
        voice: 0,
        subtitle: table(0x2500_0100 + n),
        ..Keyframe::default()
    };
    let scene = SceneInfo {
        keyframes: vec![key(1), key(2)],
        ..SceneInfo::default()
    };
    let r = b.object(0xA3, NativeObject::Scene(Box::new(scene)));
    let set = b.object(SET, NativeObject::Opaque { ty: SET });
    let (status, _) = b.call_with_local(0xA3, 0x02, r, set, &[Some(0x4000_0009), None], 0);
    assert_eq!(status, Status::Success);
    assert_eq!(
        record_ids(&b, set),
        [
            0x0A00_0001,
            0x0A00_0002,
            0x0A00_0101,
            0x0A00_0102,
            0x2500_0101,
            0x2500_0102
        ]
    );
    let set = b.object(SET, NativeObject::Opaque { ty: SET });
    let (status, _) = b.call_with_local(0xA3, 0x02, r, set, &[Some(0x4000_0008), None], 0);
    assert_eq!(status, Status::Success);
    assert_eq!(record_ids(&b, set), [0x2500_0101, 0x2500_0102], "no tracks");

    // A race and sex's look: body, pile, music, then a table of two entries.
    let words: [u32; 8] = [
        0x0200_0001,
        0x0200_0002,
        0x0300_0003,
        0x0200_0002,
        7,
        0x0700_0007,
        9,
        0x0700_0009,
    ];
    let bytes = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let r = b.object(0xA2, NativeObject::Packed { ty: 0xA2, bytes });
    let set = b.object(SET, NativeObject::Opaque { ty: SET });
    let (status, _) = b.call_with_local(0xA2, 0x08, r, set, &[Some(0), None], 0);
    assert_eq!(status, Status::Success);
    assert_eq!(
        record_ids(&b, set),
        [
            0x0200_0001,
            0x0200_0002,
            0x0300_0003,
            0x0700_0007,
            0x0700_0009
        ]
    );
}

/// Behaviour: script.glue.an-arrays-new-places-hold-nothing
#[test]
fn an_array_of_references_made_longer_holds_no_reference_in_its_new_places() {
    use auberean_script::native_object::Item;
    let mut b = crate::call::Bench::with_packages(&[("RArray", 0x61, &["set_num_elements"])]);
    let r = b.new_object(0x61);
    assert_eq!(b.answer(0x61, 1, r, &[3]), 1);
    let Ok(NativeObject::Sequence { items, .. }) = b.vm.heap.native(r) else {
        panic!("an array")
    };
    assert_eq!(items, &vec![Item::Ref(NULL); 3]);
    assert_eq!(b.answer(0x61, 1, r, &[1]), 1);
    let Ok(NativeObject::Sequence { items, .. }) = b.vm.heap.native(r) else {
        panic!("an array")
    };
    assert_eq!(items.len(), 1, "made shorter");
}

const MACHINE_PACKAGE: u32 = 0x72;
const TYPE_INFO_PACKAGE: u32 = 0x6E;
const THING_PACKAGE: u32 = 0x80;

fn listing(b: &Bench, id: u32) -> String {
    String::from_utf16(&b.text_of(id)).expect("text")
}

/// The table's text gives how many processes it holds and its room, 128 until it is full; the
/// contexts' text counts each process's suspended ones; a process asked after is there or not, its
/// text empty.
///
/// Behaviour: script.glue.the-machine-counts-its-processes
#[test]
fn the_machine_counts_its_processes_and_contexts() {
    let mut b = Bench::new();
    let out = b.text("before");
    let (status, slots) = b.call_with_local(MACHINE_PACKAGE, 8, NULL, out, &[None], 1);
    assert_eq!((status, slots[0].0), (Status::Success, 1));
    assert_eq!(listing(&b, out), "process table:\n  total: 0, size: 128\n");
    for _ in 0..129 {
        b.vm.create_process(NULL);
    }
    b.call_with_local(MACHINE_PACKAGE, 8, NULL, out, &[None], 1);
    assert_eq!(
        listing(&b, out),
        "process table:\n  total: 129, size: 256\n"
    );
    b.call_with_local(MACHINE_PACKAGE, 9, NULL, out, &[Some(0), None], 1);
    assert_eq!(listing(&b, out), "contexts:\n  total: 0\n");
    b.vm.running_process = 3;
    let (_, slots) = b.call_with_local(MACHINE_PACKAGE, 9, NULL, out, &[Some(0), None], 1);
    assert_eq!(listing(&b, out), "contexts:\n  total: 1\n");
    assert_eq!(slots[0].0, 1);
    let (_, slots) = b.call_with_local(MACHINE_PACKAGE, 0xA, NULL, out, &[Some(5), None], 1);
    assert_eq!((slots[0].0, listing(&b, out).as_str()), (1, ""));
    let (_, slots) = b.call_with_local(MACHINE_PACKAGE, 0xA, NULL, out, &[Some(500), None], 1);
    assert_eq!(slots[0].0, 0);
    let (_, slots) = b.call_with_local(MACHINE_PACKAGE, 0xA, NULL, out, &[Some(NULL), None], 1);
    assert_eq!(slots[0].0, 0, "the id that is no process");
}

/// The performance listing is a heading and a blank line; the listings of references are empty and
/// answer true; a null string stops the call.
///
/// Behaviour: script.glue.the-machines-other-listings-say-little
#[test]
fn the_machines_other_listings_say_little() {
    let mut b = Bench::new();
    let out = b.text("before");
    let (status, slots) = b.call_with_local(MACHINE_PACKAGE, 0xC, NULL, out, &[None], 1);
    assert_eq!((status, slots[0].0), (Status::Success, 1));
    assert_eq!(listing(&b, out), "perf:\n\n");
    for (number, args) in [
        (0xB, vec![None]),
        (0x16, vec![None, Some(1)]),
        (0x17, vec![None, Some(1)]),
        (0x17, vec![None, Some(NULL)]),
    ] {
        let out = b.text("before");
        let (status, slots) = b.call_with_local(MACHINE_PACKAGE, number, NULL, out, &args, 1);
        assert_eq!((status, slots[0].0), (Status::Success, 1), "{number:#x}");
        assert_eq!(listing(&b, out), "", "{number:#x}");
    }
    let (status, _) = b.call_with_local(MACHINE_PACKAGE, 0xC, NULL, NULL, &[None], 1);
    assert_eq!(status, Status::Crash);
}

/// A package's name comes as a new string, empty for no package; a new instance comes of a package
/// by name, and a name no package has stops the call with the null reference; a validated new type
/// always answers -1.
///
/// Behaviour: script.glue.a-package-by-name-makes-an-instance
#[test]
fn a_package_by_name_makes_an_instance() {
    let mut b = Bench::with_packages(&[("Thing", THING_PACKAGE, &[])]);
    let (_, slots) = b.call(TYPE_INFO_PACKAGE, 1, NULL, &[THING_PACKAGE], 1);
    assert_eq!(slots[0].1, tag::REFERENCE);
    assert_eq!(listing(&b, slots[0].0), "Thing");
    let (_, slots) = b.call(TYPE_INFO_PACKAGE, 1, NULL, &[0x7777], 1);
    assert_eq!(listing(&b, slots[0].0), "");
    let name = b.text("Thing");
    let (status, slots) = b.call(TYPE_INFO_PACKAGE, 2, NULL, &[name], 1);
    assert_eq!(status, Status::Success);
    let mut host = TestHost::default();
    let prog = std::sync::Arc::clone(b.vm.program());
    assert_eq!(
        b.vm.package_of(&mut host, slots[0].0, &prog),
        Ok(THING_PACKAGE),
        "an instance of the package"
    );
    let name = b.text("No Such Thing");
    let (status, slots) = b.call(TYPE_INFO_PACKAGE, 2, NULL, &[name], 1);
    assert_eq!((status, slots[0].0), (Status::Crash, NULL));
    let name = b.text("Thing");
    let (status, slots) = b.call(TYPE_INFO_PACKAGE, 3, NULL, &[name, 0], 1);
    assert_eq!((status, slots[0].0), (Status::Success, u32::MAX));
}

/// The named places' table: its natives called as the machine calls one, over a table put in the
/// host's records.
mod places {
    use auberean_assets::common::Position;
    use auberean_assets::tables::Places;
    use auberean_assets::Database;
    use auberean_primitives::{CellId, Frame, Quat, Vec3};
    use auberean_script::native_object::NativeObject;
    use auberean_script::value::tag;
    use auberean_script::Status;

    use crate::call::Bench;

    const PLACES_TABLE: u32 = 0x58;
    const POSITION: u32 = 0x5A;
    const RECORD: u32 = 0x0E00_0004;

    fn table() -> Places {
        Places {
            places: vec![
                (
                    "the keep".to_owned(),
                    Position {
                        cell: CellId(0x5D3A_0101),
                        frame: Frame {
                            origin: Vec3::new(1.0, 2.0, 3.0),
                            rotation: Quat {
                                w: 1.0,
                                x: 0.0,
                                y: 0.0,
                                z: 0.0,
                            },
                        },
                    },
                ),
                ("Upper".to_owned(), Position::default()),
            ],
        }
    }

    fn bench_with_table() -> (Bench, Database) {
        let db = Database::new(Vec::new());
        let held = db.put(RECORD, table()).expect("not loaded yet");
        drop(held);
        let mut b = Bench::new();
        b.host.enum_ids.insert((0x4000_0054, 0x4000_0002), RECORD);
        (b, db)
    }

    const KEEP_TEXT: &str =
        "0x5D3A0101 [1.000000 2.000000 3.000000] 1.000000 0.000000 0.000000 0.000000";

    /// **A named place is found by its name in lower case, whatever case it is asked in**, and copied
    /// into the position an out reference holds; a name in the table with a capital is never found
    /// (only the name asked is put in lower case); not found leaves the position as it was. Written
    /// out, it is the position's own text, in the string an out reference holds, which stays as it was
    /// when the place is not found.
    ///
    /// Behaviour: script.glue.named-places-are-found-in-lower-case
    #[test]
    fn a_named_place_is_found_by_its_name_in_lower_case() {
        let (mut b, db) = bench_with_table();
        b.host.database = Some(db);
        let name = b.text("The KEEP");
        let at = b.object(
            POSITION,
            NativeObject::Position {
                cell: 7,
                frame: [0; 7],
            },
        );
        let (status, r) = b.call_with_local(PLACES_TABLE, 1, 0, at, &[Some(name), None], 1);
        assert_eq!(status, Status::Success);
        assert_eq!(r[0], (1, tag::MIXED));
        let NativeObject::Position { cell, frame } = b.vm.heap.native(at).unwrap() else {
            panic!("a position")
        };
        assert_eq!(*cell, 0x5D3A_0101);
        assert_eq!(
            frame.map(f32::from_bits),
            [1.0, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0]
        );

        for gone in ["upper", "Upper", "nowhere"] {
            let name = b.text(gone);
            let at = b.object(
                POSITION,
                NativeObject::Position {
                    cell: 7,
                    frame: [0; 7],
                },
            );
            let (_, r) = b.call_with_local(PLACES_TABLE, 1, 0, at, &[Some(name), None], 1);
            assert_eq!(r[0], (0, tag::MIXED), "{gone}");
            let NativeObject::Position { cell, .. } = b.vm.heap.native(at).unwrap() else {
                panic!("a position")
            };
            assert_eq!(*cell, 7, "{gone} leaves the position");
        }

        let name = b.text("the keep");
        let out = b.text("before");
        let (_, r) = b.call_with_local(PLACES_TABLE, 2, 0, out, &[Some(name), None], 1);
        assert_eq!(r[0], (1, tag::MIXED));
        assert_eq!(String::from_utf16_lossy(&b.text_of(out)), KEEP_TEXT);
        let name = b.text("nowhere");
        let out = b.text("before");
        let (_, r) = b.call_with_local(PLACES_TABLE, 2, 0, out, &[Some(name), None], 1);
        assert_eq!(r[0], (0, tag::MIXED));
        assert_eq!(String::from_utf16_lossy(&b.text_of(out)), "before");
    }

    /// **The named places' dump is a line for each place, or says it could not get the list**: the
    /// name, a colon and the position's own text, with a stop; the install holding no table says so.
    ///
    /// Behaviour: script.glue.named-places-dump
    #[test]
    fn the_named_places_dump_lists_each_place_or_says_it_could_not_get_the_list() {
        let (mut b, db) = bench_with_table();
        let (_, none) = b.call(PLACES_TABLE, 3, 0, &[], 1);
        assert_eq!(
            String::from_utf16_lossy(&b.text_of(none[0].0)),
            "Couldn't get places list.\n"
        );
        b.host.database = Some(db);
        let (_, r) = b.call(PLACES_TABLE, 3, 0, &[], 1);
        assert_eq!(
            String::from_utf16_lossy(&b.text_of(r[0].0)),
            format!(
                "the keep: {KEEP_TEXT}.\nUpper: 0x00000000 [0.000000 0.000000 0.000000] 1.000000 0.000000 0.000000 0.000000.\n"
            )
        );
    }
}

// A creation's natives, by number.
const GET_RACE: u32 = 0x03;
const SET_RACE: u32 = 0x04;
const GET_SEX: u32 = 0x05;
const SET_SEX: u32 = 0x06;
const SET_CLASS: u32 = 0x07;
const GET_CLASS: u32 = 0x08;
const SET_IMPLEMENT: u32 = 0x09;
const SET_VALUE: u32 = 0x0B;
const GET_VALUE: u32 = 0x0C;
const SET_START_AREA: u32 = 0x0E;

fn result_of(b: &Bench, r: u32) -> CreationChoices {
    CreationChoices::of(b.vm.heap.native(r).expect("the object"))
        .expect("a creation's choices")
        .clone()
}

/// Race, sex and class read back what was set; setting the class or the implement answers 1, and
/// setting the race, the sex or the starting area answers nothing, leaving the slot alone.
///
/// Behaviour: script.glue.a-creations-choices-are-set-and-read
#[test]
fn a_creations_fields_read_back_what_was_set() {
    let mut b = Bench::new();
    let r = b.object(CREATION_CHOICES, CreationChoices::default().native());
    for (set, get, v) in [(SET_RACE, GET_RACE, 4), (SET_SEX, GET_SEX, 0x2000)] {
        let (status, slots) = b.call(CREATION_CHOICES, set, r, &[v], 1);
        assert_eq!(status, Status::Success);
        assert_eq!(slots[0].0, 0, "a setter that answers nothing");
        assert_eq!(b.answer(CREATION_CHOICES, get, r, &[]), v);
    }
    assert_eq!(b.answer(CREATION_CHOICES, SET_CLASS, r, &[7]), 1);
    assert_eq!(b.answer(CREATION_CHOICES, GET_CLASS, r, &[]), 7);
    assert_eq!(b.answer(CREATION_CHOICES, SET_IMPLEMENT, r, &[9]), 1);
    let (status, slots) = b.call(CREATION_CHOICES, SET_START_AREA, r, &[3], 1);
    assert_eq!((status, slots[0].0), (Status::Success, 0));
    let got = result_of(&b, r);
    assert_eq!(
        (got.race, got.sex, got.class, got.implement, got.start_area),
        (4, 0x2000, 7, 9, 3)
    );
}

/// A physique type with no value reads as -1; setting one answers 1, whether or not the type had a
/// value, and the last value set is the one read.
///
/// Behaviour: script.glue.a-creations-choices-are-set-and-read
#[test]
fn a_physique_type_with_no_value_reads_as_minus_one() {
    let mut b = Bench::new();
    let r = b.object(CREATION_CHOICES, CreationChoices::default().native());
    let half = 0.5f32.to_bits();
    let quarter = 0.25f32.to_bits();
    assert_eq!(
        b.answer(CREATION_CHOICES, GET_VALUE, r, &[4]),
        (-1.0f32).to_bits()
    );
    assert_eq!(b.answer(CREATION_CHOICES, SET_VALUE, r, &[4, half]), 1);
    assert_eq!(b.answer(CREATION_CHOICES, SET_VALUE, r, &[4, quarter]), 1);
    assert_eq!(b.answer(CREATION_CHOICES, GET_VALUE, r, &[4]), quarter);
    assert_eq!(
        b.answer(CREATION_CHOICES, GET_VALUE, r, &[8]),
        (-1.0f32).to_bits()
    );
}

/// Through the null reference, each crashes the script.
///
/// Behaviour: script.glue.a-creations-choices-are-set-and-read
#[test]
fn a_creation_through_the_null_reference_crashes() {
    let mut b = Bench::new();
    for n in [
        GET_RACE,
        SET_RACE,
        GET_SEX,
        SET_SEX,
        SET_CLASS,
        GET_CLASS,
        SET_IMPLEMENT,
        SET_VALUE,
        GET_VALUE,
        SET_START_AREA,
    ] {
        let (status, _) = b.call(CREATION_CHOICES, n, NULL, &[1, 2], 1);
        assert_eq!(status, Status::Crash, "native {n:#x}");
    }
}

/// Data files with no word filter leave the string alone and answer 0.
///
/// Behaviour: script.glue.a-name-is-left-alone-with-no-word-filter
#[test]
fn a_name_is_left_alone_with_no_word_filter() {
    let mut b = Bench::new();
    let name = b.text("  jOHN ");
    let held = b.text("before");
    let (status, slots) = b.call_with_local(0x6B, 3, NULL, held, &[Some(1), Some(name), None], 1);
    assert_eq!((status, slots[0].0), (Status::Success, 0));
    assert_eq!(
        b.text_of(held),
        "before".encode_utf16().collect::<Vec<u16>>()
    );
}
