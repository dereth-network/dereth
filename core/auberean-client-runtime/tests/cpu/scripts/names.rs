//! An object's name and plural name as its repository holds them.

use auberean_client_runtime::repository::{name, plural_name, set, STRING_INFO};
use auberean_script::native_object::TextInfo;

use auberean_script::Host as _;

use crate::common::{Bench, ITEM};

/// The scripts ask the client's table of file names, which its data cache starts and no file of
/// names fills: an id is 0, a name empty, and the type asked about is registered.
///
/// Behaviour: data.file-names.the-scripts-ask-the-clients-table
#[test]
fn the_scripts_ask_the_clients_table_of_file_names() {
    const WAVE: u32 = 0x0A;
    let mut b = Bench::new();
    assert!(b.host.file_names.is_active());
    assert!(!b.host.file_names.uses_folders());
    assert!(!b.host.file_names.ids_may_be_made());
    assert!(b.host.file_names.is_read_only());
    assert!(!b.host.file_names.table().is_type_registered(WAVE));
    assert_eq!(b.host.data_id_by_name("thunder.wav", WAVE), 0);
    assert!(b.host.file_names.table().is_type_registered(WAVE));
    assert_eq!(b.host.file_name_by_id(0x0A00_0001), Vec::<u16>::new());
}

/// An object's name and plural name are its name qualities; without them, a literal that says
/// it has none.
///
/// Behaviour: script.model.an-objects-name-is-its-name-quality
#[test]
fn an_objects_name_is_its_name_quality_or_a_literal_saying_none() {
    let mut b = Bench::new();
    let lit = |t: &TextInfo| String::from_utf16(t.literal.as_deref().unwrap_or(&[])).unwrap();
    let n = name(&mut b.host, ITEM);
    assert_eq!((n.has_literal, lit(&n).as_str()), (1, "I have no name!"));
    let p = plural_name(&mut b.host, ITEM);
    assert_eq!(lit(&p), "I have no plural name!");
    let held = TextInfo {
        string_id: 7,
        table: 0x2300_0001,
        ..TextInfo::default()
    };
    let o = b.host.model.object_mut(ITEM).unwrap();
    assert!(set(&STRING_INFO, o, 0x100, held.clone()));
    assert_eq!(name(&mut b.host, ITEM), held);
    assert_eq!(
        lit(&plural_name(&mut b.host, ITEM)),
        "I have no plural name!"
    );
}

/// An object moves unless its mobile flag says not; it is a player or a creature when its kind
/// holds every bit of that kind; a detection reports it when the report's mask meets its two
/// detection words.
///
/// Behaviour: script.model.an-objects-kind-mobility-and-detection-words
#[test]
fn an_objects_kind_mobility_and_detection_words_answer_as_held() {
    use auberean_client_runtime::repository::{
        detection_type_match, is_creature, is_mobile, is_player, phys_obj_id, BOOL, DATA_ID, INT,
    };
    let mut b = Bench::new();
    assert_eq!(phys_obj_id(&mut b.host, ITEM), 0);
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&DATA_ID, o, 1, 0x0200_0123);
    assert_eq!(phys_obj_id(&mut b.host, ITEM), 0x0200_0123);
    assert!(is_mobile(&mut b.host, ITEM), "none held: it moves");
    assert!(!is_creature(&mut b.host, ITEM));
    assert!(!is_player(&mut b.host, ITEM));
    assert!(!detection_type_match(&mut b.host, ITEM, u64::MAX));
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&BOOL, o, 8, 0);
    set(&INT, o, 0x14, 0x4E);
    set(&INT, o, 10, 0x4);
    set(&INT, o, 11, 0x2);
    assert!(!is_mobile(&mut b.host, ITEM));
    assert!(!is_creature(&mut b.host, ITEM), "one creature bit short");
    assert!(!is_player(&mut b.host, ITEM));
    assert!(detection_type_match(&mut b.host, ITEM, 0x4));
    assert!(detection_type_match(&mut b.host, ITEM, 0x2 << 32));
    assert!(!detection_type_match(&mut b.host, ITEM, 0x2 | 0x4 << 32));
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&INT, o, 0x14, 0xFF);
    assert!(is_creature(&mut b.host, ITEM));
    assert!(is_player(&mut b.host, ITEM));
    // The kinds the recorded sessions' objects have: a player's, a creature's, an item's.
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&INT, o, 0x14, 0x1F);
    assert!(is_player(&mut b.host, ITEM));
    assert!(!is_creature(&mut b.host, ITEM), "a player's kind");
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&INT, o, 0x14, 0x1_004F);
    assert!(is_creature(&mut b.host, ITEM));
    assert!(!is_player(&mut b.host, ITEM), "a creature's kind");
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&INT, o, 0x14, 0x7_0081);
    assert!(!is_creature(&mut b.host, ITEM), "an item's kind");
    assert!(!is_player(&mut b.host, ITEM), "an item's kind");
}

/// An object described again with a body: not drawn when its no-draw flag is set; drawn again
/// when neither that nor its kept-hidden flag is; left as it was when only kept hidden.
///
/// Behaviour: script.model.an-object-described-again-is-drawn-by-its-flags
#[test]
fn an_object_described_again_is_drawn_or_not_by_its_flags() {
    use auberean_client_runtime::body::described_again;
    use auberean_client_runtime::repository::BOOL;
    let mut b = Bench::new();
    b.host.model.object_mut(ITEM).unwrap().parent = None;
    b.host.model.object_mut(ITEM).unwrap().in_world = true;
    let drawn = |b: &Bench| !b.host.bodies.get(&ITEM).is_some_and(|s| s.no_draw);
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&BOOL, o, 10, 1);
    described_again(&mut b.host, ITEM);
    assert!(!drawn(&b), "no-draw set");
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&BOOL, o, 10, 0);
    set(&BOOL, o, 0x117, 1);
    described_again(&mut b.host, ITEM);
    assert!(!drawn(&b), "kept hidden: left as it was");
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&BOOL, o, 0x117, 0);
    described_again(&mut b.host, ITEM);
    assert!(drawn(&b), "neither: drawn again");
    // No body: nothing.
    b.host.model.object_mut(ITEM).unwrap().in_world = false;
    let o = b.host.model.object_mut(ITEM).unwrap();
    set(&BOOL, o, 10, 1);
    described_again(&mut b.host, ITEM);
    assert!(drawn(&b));
}
