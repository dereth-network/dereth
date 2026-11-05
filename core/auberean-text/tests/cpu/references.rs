//! String references written to text over tables made here.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_text::number::TableNumbers;
use auberean_text::{Code, Entry, Language, Source, StringInfo, Table, Value, Variable};

use crate::{english, wide};

const T1: u32 = 0x2500_0001;
const T2: u32 = 0x2500_0002;
const MISSING: u32 = 0x2500_0009;

fn entry(fragments: &[&str], variables: &[u32]) -> Entry {
    Entry {
        private: 0,
        fragments: fragments.iter().map(|f| Some(wide(f))).collect(),
        variables: variables.to_vec(),
        names: Vec::new(),
    }
}

struct Tables(HashMap<u32, Arc<Table>>, Arc<Language>);

impl Source for Tables {
    fn table(&mut self, id: u32) -> Option<Arc<Table>> {
        self.0.get(&id).cloned()
    }
    fn language(&mut self) -> Option<Arc<Language>> {
        Some(Arc::clone(&self.1))
    }
}

fn tables() -> Tables {
    let mut redirect = entry(&[], &[]);
    redirect.private = T2;
    let t1 = Table::new(
        TableNumbers::default(),
        [
            (1, entry(&["Hello, ", "!"], &[10])),
            (2, redirect),
            (3, entry(&["You have ", " {coin[1]|coins}."], &[5])),
            (4, entry(&["World"], &[])),
            (6, entry(&["Sword[mn]"], &[])),
        ],
    );
    let t2 = Table::new(TableNumbers::default(), [(2, entry(&["Private"], &[]))]);
    Tables(
        HashMap::from([(T1, Arc::new(t1)), (T2, Arc::new(t2))]),
        Arc::new(english()),
    )
}

fn with(mut s: StringInfo, id: u32, value: Value) -> StringInfo {
    s.variables.push(Variable { id, value });
    s
}

fn get(s: &mut StringInfo, src: &mut Tables) -> String {
    String::from_utf16_lossy(&s.get_string(src, false))
}

/// A reference is written from its table entry with its variables (numbers in the language's
/// format, nested references written in turn, taking this one's table when they name none and
/// keeping it), through an entry's private table when it names one, its flags cut off unless they
/// are kept, unescaped; a literal is its own text. A log line puts the text, escapes kept, in
/// parentheses.
///
/// Behaviour: text.references.write
#[test]
fn a_reference_is_written_from_its_entry_with_its_variables() {
    let mut src = tables();
    let mut hello = with(
        StringInfo::entry(T1, 1),
        10,
        Value::Text(Box::new(StringInfo::literal("Asheron"))),
    );
    assert_eq!(get(&mut hello, &mut src), "Hello, Asheron!");
    assert_eq!(
        String::from_utf16_lossy(&hello.log_string(&mut src, true)),
        "(Hello, Asheron!)"
    );
    let mut world = with(
        StringInfo::entry(T1, 1),
        10,
        Value::Text(Box::new(StringInfo::entry(0, 4))),
    );
    assert_eq!(get(&mut world, &mut src), "Hello, World!");
    match &world.variables[0].value {
        Value::Text(n) => assert_eq!(n.table, T1, "the nested reference keeps the table"),
        v => panic!("{v:?}"),
    }
    let coins = |n: u32| with(StringInfo::entry(T1, 3), 5, Value::UInt(n));
    assert_eq!(get(&mut coins(1), &mut src), "You have 1 coin.");
    assert_eq!(get(&mut coins(1000), &mut src), "You have 1,000 coins.");
    assert_eq!(get(&mut StringInfo::entry(T1, 2), &mut src), "Private");
    let mut sword = StringInfo::entry(T1, 6);
    assert_eq!(get(&mut sword, &mut src), "Sword");
    assert_eq!(
        String::from_utf16_lossy(&sword.get_string(&mut src, true)),
        "Sword[mn]"
    );
    assert_eq!(
        get(&mut StringInfo::literal("a \\{b\\}[x]"), &mut src),
        "a {b}"
    );
}

/// A reference that cannot be written is the error message naming its token and table: no token
/// or table, a missing table, a missing token, an entry variable it does not have; asked whether
/// it rendered, it says no, and its log line gives the reason's number.
///
/// Behaviour: text.references.errors
#[test]
fn a_reference_that_cannot_be_written_is_its_error_message() {
    let mut src = tables();
    let message =
        |t: &str, id: &str| format!("<string table error; token: 0x{id} tableDID: 0x{t}>");
    let mut missing = StringInfo::entry(MISSING, 1);
    assert_eq!(get(&mut missing, &mut src), message("25000009", "00000001"));
    assert!(!missing.inq(&mut src, false).0);
    assert_eq!(
        String::from_utf16_lossy(&missing.log_string(&mut src, true)),
        "<could not render string: table 0x25000009 token 0x00000001. Reason = 3>"
    );
    assert_eq!(
        get(&mut StringInfo::entry(0, 7), &mut src),
        message("00000000", "00000007")
    );
    assert_eq!(
        get(&mut StringInfo::entry(T1, 99), &mut src),
        message("25000001", "00000063")
    );
    assert_eq!(
        get(&mut StringInfo::entry(T1, 1), &mut src),
        message("25000001", "00000001"),
        "the entry's variable is missing"
    );
}

/// A reference is valid when it is a literal, or its table holds its token; deeply valid when
/// every nested reference is valid too (a nested reference naming no table is not). One that
/// writes without an error is on disk.
///
/// Behaviour: text.references.valid
#[test]
fn a_reference_is_valid_when_its_table_holds_its_token() {
    let mut src = tables();
    assert!(StringInfo::literal("x").is_valid(&mut src, true));
    assert!(StringInfo::entry(T1, 4).is_valid(&mut src, false));
    assert!(!StringInfo::entry(T1, 99).is_valid(&mut src, false));
    assert!(!StringInfo::entry(MISSING, 1).is_valid(&mut src, false));
    assert!(!StringInfo::entry(0, 4).is_valid(&mut src, false));
    let nested = with(
        StringInfo::entry(T1, 1),
        10,
        Value::Text(Box::new(StringInfo::entry(0, 4))),
    );
    assert!(nested.is_valid(&mut src, false));
    assert!(
        !nested.is_ready(&mut src),
        "the nested reference names no table"
    );
    let mut hello = StringInfo::entry(T1, 1);
    assert!(!hello.is_on_disk(&mut src), "its variable is missing");
    hello.add_variable_string_id(10, 4);
    assert!(hello.is_ready(&mut src));
    assert!(hello.is_on_disk(&mut src));
    assert_eq!(get(&mut hello, &mut src), "Hello, World!");
}

/// A text variable is a nested literal holding the text escaped; narrow text is read in the
/// English Windows code page. A later variable of an id replaces the earlier.
///
/// Behaviour: text.references.variables
#[test]
fn a_text_variable_is_an_escaped_literal_and_replaces_one_of_its_id() {
    let mut src = tables();
    let mut hello = StringInfo::entry(T1, 1);
    hello.add_variable_string(10, &wide("[you]"));
    assert_eq!(get(&mut hello, &mut src), "Hello, [you]!");
    hello.add_variable_narrow(10, b"caf\xE9 \x80");
    assert_eq!(get(&mut hello, &mut src), "Hello, caf\u{e9} \u{20ac}!");
    assert_eq!(hello.variables.len(), 1);
    let mut copy = hello.clone();
    assert_eq!(get(&mut copy, &mut src), "Hello, caf\u{e9} \u{20ac}!");
    copy.variables.clear();
    assert!(copy.variables.is_empty());
}

/// A literal's value is its text unescaped, its flags cut off unless kept; a player's name
/// literal takes the player's flags.
///
/// Behaviour: text.references.literal-value
#[test]
fn a_literals_value_is_its_text_unescaped() {
    let lit = StringInfo::literal("a \\{b\\} [x]");
    assert_eq!(String::from_utf16_lossy(&lit.literal_value(false)), "a {b}");
    assert_eq!(
        String::from_utf16_lossy(&lit.literal_value(true)),
        "a {b} [x]"
    );
    assert!(StringInfo::entry(T1, 4).literal_value(false).is_empty());
    let mut name = StringInfo::literal(" Ana [x]");
    name.apply_metas_to_player_name(false);
    assert_eq!(
        name.literal
            .as_deref()
            .map(String::from_utf16_lossy)
            .as_deref(),
        Some("Ana [Fv]")
    );
}

/// A reference's bucket among `n` is its literal text's hash, else its table's and its token's
/// remainders added (modulo `n`).
///
/// Behaviour: text.references.bucket
#[test]
fn a_references_bucket_is_its_literals_hash_or_its_ids_remainders() {
    assert_eq!(StringInfo::entry(10, 7).bucket(4), (10 % 4 + 7 % 4) % 4);
    let lit = StringInfo::literal("Hello");
    // H, e, l, l, o: each added to the hash times 16, the top four bits folded back in.
    let mut h: u32 = 0;
    for c in "Hello".bytes() {
        h = h * 16 + u32::from(c);
        if h & 0xF000_0000 != 0 {
            h = ((h & 0xF000_0000) >> 24 ^ h) & 0x0FFF_FFFF;
        }
    }
    assert_eq!(lit.bucket(u32::MAX), h);
    assert_eq!(lit.bucket(1000), h % 1000);
}

/// The private tables a reference sends its tokens on to, for the table asked about: its own
/// entry's and its nested references'. An entry's text without rendering is its strings joined.
///
/// Behaviour: text.references.private-tables
#[test]
fn a_reference_names_the_private_tables_its_entries_use() {
    let mut src = tables();
    let r = with(
        StringInfo::entry(T1, 1),
        10,
        Value::Text(Box::new(StringInfo::entry(T1, 2))),
    );
    let mut out = auberean_dat::QualifiedIds::new();
    r.sub_private_ids(&mut src, T1, &mut out);
    assert_eq!(out.iter().map(|q| q.id).collect::<Vec<_>>(), [T2]);
    let mut none = auberean_dat::QualifiedIds::new();
    r.sub_private_ids(&mut src, T2, &mut none);
    assert!(none.is_empty());
    let t1 = src.table(T1).expect("table");
    let mut raw = |token| {
        let (code, text) = auberean_text::info::raw_string(&t1, token, &mut src);
        (code, String::from_utf16_lossy(&text))
    };
    assert_eq!(raw(1), (Code::Success, "Hello, !".to_owned()));
    assert_eq!(raw(2), (Code::Success, "Private".to_owned()));
    assert_eq!(raw(99).0, Code::MissingString);
}
