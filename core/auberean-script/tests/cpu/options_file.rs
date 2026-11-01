//! The player's options exported to their text file and imported back.

use auberean_script::options::GameplayOptions;
use auberean_script::options_file::{NOT_READ, NO_FILE_NAME, NO_OPTIONS};

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// An export writes the options as a text tree (`"GAMEPLAY OPTIONS"`: the aliases, the damage
/// text's flags and range, the other sections bare), and an import of that file gives a new
/// profile holding the same aliases and damage text options and a new profile's everything else.
///
/// Behaviour: script.options.an-exported-file-imports-back
#[test]
fn an_exported_options_file_imports_back_into_a_new_profile() {
    let mut o = GameplayOptions::default();
    o.set_alias(&w("loc"), &w("/loc"));
    o.set_alias(&w("hi"), &w("say hello there"));
    o.set_bits(true, 4);
    o.damage_text_range = 30.5f32.to_bits();
    o.shortcut_height = 3;
    let bytes = o.export("my options.txt").expect("written");
    assert_eq!(
        String::from_utf8(bytes.clone()).expect("text"),
        concat!(
            "\r\n\"GAMEPLAY OPTIONS\"\r\n[\r\n",
            "  Aliases [ loc [ /loc ] hi [ \"say hello there\" ] ]\r\n",
            "  Shortcuts\r\n",
            "  DamageText [ All [ true ] Other [ false ] Range [ 30.5 ] ]\r\n",
            "  UISaveLocations\r\n  FilterSettings\r\n  AutoTarget\r\n]\r\n",
        )
    );
    let back = GameplayOptions::import("my options.txt", &bytes).expect("read");
    assert_eq!(back.aliases.entries, o.aliases.entries);
    assert_eq!(back.bits & 0xC, 4);
    assert_eq!(f32::from_bits(back.damage_text_range), 30.5);
    assert_eq!(back.shortcut_height, 1, "a new profile's");
    assert_eq!(o.export(""), Err(NO_FILE_NAME));
}

/// An import fails on an empty file name, a file that does not parse, one holding nothing, one
/// whose node is not the options', one with a section it does not know, an alias with nothing
/// under it, or a range that is not a number; `Other` turns the damage text for others on and a
/// confirmation section is read as nothing.
///
/// Behaviour: script.options.an-import-refuses-what-it-does-not-know
#[test]
fn an_import_refuses_what_it_does_not_know() {
    let imp = |t: &str| GameplayOptions::import("f", t.as_bytes());
    assert_eq!(imp("x").err(), Some(NOT_READ), "not the options' node");
    assert_eq!(GameplayOptions::import("", b"x").err(), Some(NO_FILE_NAME));
    assert_eq!(imp("x ]").err(), Some(NOT_READ), "does not parse");
    assert_eq!(imp("  \r\n").err(), Some(NO_OPTIONS));
    assert_eq!(
        imp("\"GAMEPLAY OPTIONS\" [ Mystery ]").err(),
        Some(NOT_READ)
    );
    assert_eq!(
        imp("\"GAMEPLAY OPTIONS\" [ Aliases [ lonely ] ]").err(),
        Some(NOT_READ)
    );
    assert_eq!(
        imp("\"GAMEPLAY OPTIONS\" [ DamageText [ Range [ far ] ] ]").err(),
        Some(NOT_READ)
    );
    let o = imp("\"gameplay options\" [ DamageText [ Other [ TRUE ] ] ConfirmationSettings ]")
        .expect("read");
    assert_eq!(o.bits & 8, 8);
    assert!(imp("\"GAMEPLAY OPTIONS\"").is_ok(), "nothing in it reads");
}
