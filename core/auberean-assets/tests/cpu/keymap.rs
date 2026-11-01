//! A key map's file: how the client reads one, the bindings it keeps, and the updates for an older
//! file.

use auberean_assets::keymap::{input_by_name, input_name, VersionUpdate, NORMAL_UPDATES};
use auberean_assets::ui::KeyMap;
use auberean_dat::tree;

/// Action names as an enumeration would answer them.
fn action_of(name: &str) -> Option<u32> {
    match name.to_ascii_uppercase().as_str() {
        "USE_KEY" => Some(0x4100_000D),
        "REPLY_KEY" => Some(0x4100_0040),
        "JUMP_KEY" => Some(0x4000_0007),
        _ => None,
    }
}

fn read(text: &[u8], updates: &[VersionUpdate]) -> Option<(KeyMap, u32, bool)> {
    let mut root = tree::parse_bytes(b"Main.keymap", text)?;
    let r = KeyMap::from_tree(&mut root, &action_of, updates)?;
    Some((r.map, r.version, r.updated))
}

fn bindings(m: &KeyMap) -> Vec<(&str, &str)> {
    m.bindings
        .iter()
        .map(|b| (b.name.as_str(), input_name(b.input).unwrap_or("?")))
        .collect()
}

/// A key map's file is an optional `VERSION`, the name, an 11-part identity of whole numbers (the
/// last ten kept to 16, 16 and 8 bits), then bindings of exactly two names: an action the input
/// actions enumeration knows and an input of the client's names (exact); any other node is
/// skipped, and of two bindings of one input the first is kept.
///
/// Behaviour: data.key-map.file-reads-as-the-client-reads-it
#[test]
fn a_key_map_file_keeps_known_bindings_the_first_of_an_input_winning() {
    let text = b"VERSION [ 1 ]\r\n\"\" [ mine ]\r\n\"\" [ 0x10203040 0x15060 7 8 9 10 11 12 13 14 0x1FF ]\r\n\
        \"\" [ use_key DIKEYBOARD_F ]\r\n\"\" [ JUMP_KEY DIKEYBOARD_F ]\r\n\"\" [ JUMP_KEY DIKEYBOARD_SPACE ]\r\n\
        \"\" [ NO_SUCH_KEY DIKEYBOARD_Q ]\r\n\"\" [ USE_KEY dikeyboard_e ]\r\n\"\" [ USE_KEY DIKEYBOARD_E extra ]\r\n";
    let (m, version, updated) = read(text, &[]).expect("reads");
    assert_eq!((m.name.as_str(), version, updated), ("mine", 1, false));
    assert_eq!(
        m.guid,
        [0x40, 0x30, 0x20, 0x10, 0x60, 0x50, 7, 0, 8, 9, 10, 11, 12, 13, 14, 0xFF]
    );
    assert_eq!(
        bindings(&m),
        [
            ("use_key", "DIKEYBOARD_F"),
            ("JUMP_KEY", "DIKEYBOARD_SPACE")
        ]
    );
    assert_eq!(m.bindings[0].action, 0x4100_000D);
    // Too few nodes, an identity of other than eleven numbers, a version not a number: no map.
    assert!(read(b"\"\" [ only ]\n", &[]).is_none());
    assert!(read(b"\"\" [ m ]\n\"\" [ 1 2 3 ]\n", &[]).is_none());
    assert!(read(b"\"\" [ m ]\n\"\" [ 1 2 3 4 5 6 7 8 9 10 x ]\n", &[]).is_none());
    assert!(read(
        b"VERSION [ one ]\n\"\" [ m ]\n\"\" [ 1 2 3 4 5 6 7 8 9 10 11 ]\n",
        &[]
    )
    .is_none());
    assert_eq!(input_by_name(b"DIKEYBOARD_ESCAPE"), Some(0x8100_0401));
    assert_eq!(input_by_name(b"dikeyboard_escape"), None);
}

/// The plain map is read with one update: a file older than version 1 gains R as the reply key
/// when R is free, and then the client writes the file again; a file of version 1, or one that
/// binds R, gains nothing. A file with a preprocessor header reads its version from the header.
///
/// Behaviour: data.key-map.an-older-plain-map-gains-reply-on-r
#[test]
fn an_older_plain_map_gains_reply_on_r_when_r_is_free() {
    let identity = "\"\" [ 0 0 0 0 0 0 0 0 0 0 0 ]\n";
    let old = format!("\"\" [ m ]\n{identity}\"\" [ USE_KEY DIKEYBOARD_F ]\n");
    let (m, version, updated) = read(old.as_bytes(), &NORMAL_UPDATES).expect("reads");
    assert_eq!((version, updated), (0, true));
    assert_eq!(
        bindings(&m),
        [("USE_KEY", "DIKEYBOARD_F"), ("REPLY_KEY", "DIKEYBOARD_R")]
    );
    assert_eq!(m.bindings[1].action, 0x4100_0040);
    let bound = format!("\"\" [ m ]\n{identity}\"\" [ USE_KEY DIKEYBOARD_R ]\n");
    assert!(!read(bound.as_bytes(), &NORMAL_UPDATES).expect("reads").2);
    let current = format!("VERSION [ 1 ]\n\"\" [ m ]\n{identity}");
    assert!(!read(current.as_bytes(), &NORMAL_UPDATES).expect("reads").2);
    let headed = format!("PREPROC_HEADER [ 7 ]\nVERSION [ 0 ]\n\"\" [ m ]\n{identity}");
    let (m, version, updated) = read(headed.as_bytes(), &NORMAL_UPDATES).expect("reads");
    assert_eq!((version, updated, m.name.as_str()), (7, false, "m"));
}

/// A key map is written as the client saves one: `VERSION [ 1 ]`, the name, the identity's 11
/// parts in decimal, then a binding a line, each line after a blank one.
///
/// Behaviour: data.key-map.written-as-the-client-saves-one
#[test]
fn a_key_map_is_written_as_the_client_saves_one() {
    let mut m = KeyMap {
        name: "mydefault".into(),
        guid: [
            0x71, 0xD7, 0x9E, 0x00, 0xE4, 0xEC, 0x18, 0x00, 0xF0, 0xEC, 0x18, 0, 0x50, 0xF2, 0x3F,
            0x22,
        ],
        bindings: Vec::new(),
    };
    m.add_action(0x4100_0001, 0x8100_042D, "EXAMINATION_PANEL_KEY");
    let mut t = m.to_tree();
    assert_eq!(
        String::from_utf8(tree::to_file_bytes(&mut t)).unwrap(),
        "\r\nVERSION [ 1 ]\r\n\r\n\"\" [ mydefault ]\r\n\r\n\"\" [ 10409841 60644 24 240 236 24 0 80 242 63 34 ]\r\n\
         \r\n\"\" [ EXAMINATION_PANEL_KEY DIKEYBOARD_X ]\r\n"
    );
    let mut copy = KeyMap::default();
    m.copy_into(&mut copy);
    assert_eq!(copy, m);
    m.copy_into(&mut copy);
    assert_eq!(copy.bindings.len(), 2, "a copy adds to the bindings there");
}
