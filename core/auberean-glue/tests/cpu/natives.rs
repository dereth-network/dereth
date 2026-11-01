//! Natives called from script, on a hand-built library whose native packages carry the names the
//! containers are bound by.

// Hand-written programs and tables a few hundred entries long: every length and offset fits in
// 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native_object::{Field, NativeObject};
use auberean_script::opcode::op;
use auberean_script::value::{tag, FunctionId};
use auberean_script::{Program, Status, Vm};

use crate::asm::{i, library, small, w, Pkg, TestHost};

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
