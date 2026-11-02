//! The client's preferences: each kind's value as the file keeps it, and the bank that keeps one of
//! each preference and tells the options page and the client's parts about them.

use std::collections::BTreeMap;

use auberean_ui_game::options::bank::{
    self, Bank, Notice, Party, PreferenceFile, KEY_NAMES, SECTION_NAMES,
};
use auberean_ui_game::options::preference::{
    Choice, Enumerations, Flag, NoEnumerations, Number, Pref, Text,
};

const CAMERA: u32 = 0x4000_0008;
const GRAPHICS: u32 = 0x4000_0004;
const KEY_MAP: u32 = 0x4000_0006;
const DISTANCE: u32 = 0x4000_002A;
const TILT: u32 = 0x4000_0054;
const AUTO_DISTANCE: u32 = 0x4000_0051;
const TEXTURE_DETAIL: u32 = 0x4000_004F;
const NORMAL: u32 = 0x4000_0030;
/// A made-up enumeration of detail levels, for a choice's names.
const LEVELS: u32 = 0x4000_0099;

/// The data's names, as far as these tests need them.
struct Names;

impl Enumerations for Names {
    fn value_of(&self, e: u32, name: &str) -> Option<u32> {
        let list = self.list(e);
        list.iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(v, _)| *v)
    }
    fn name_of(&self, e: u32, value: u32) -> Option<String> {
        self.list(e)
            .iter()
            .find(|(v, _)| *v == value)
            .map(|(_, n)| (*n).to_owned())
    }
}

impl Names {
    fn list(&self, e: u32) -> &'static [(u32, &'static str)] {
        match e {
            SECTION_NAMES => &[
                (CAMERA, "Camera"),
                (GRAPHICS, "Graphics"),
                (KEY_MAP, "KeyMap"),
            ],
            KEY_NAMES => &[
                (DISTANCE, "CameraDistance"),
                (TILT, "CameraTilt"),
                (AUTO_DISTANCE, "CameraAutoDistance"),
                (TEXTURE_DETAIL, "Detail"),
                (NORMAL, "Normal"),
            ],
            LEVELS => &[(0x51, "Low"), (0x52, "High"), (0x53, "VeryHigh")],
            _ => &[],
        }
    }
}

/// A file kept in memory, by section and key as written.
#[derive(Default)]
struct File(BTreeMap<(String, String), String>);

impl PreferenceFile for File {
    fn read(&self, section: &str, key: &str, default: &str, size: usize) -> String {
        let v = self
            .0
            .get(&(section.to_owned(), key.to_owned()))
            .map_or(default, String::as_str);
        v[..v.len().min(size - 1)].to_owned()
    }
    fn write(&mut self, section: &str, key: &str, value: &str) {
        self.0
            .insert((section.to_owned(), key.to_owned()), value.to_owned());
    }
}

fn detail() -> Choice {
    let mut c = Choice::new();
    c.map(LEVELS);
    c.assign_named_choice(1, (8, 101), 0x51);
    c.assign_named_choice(3, (8, 103), 0x52);
    c.assign_named_choice(4, (8, 104), 0x53);
    c.assign_choice(9, (8, 109));
    c.set_default(3);
    c
}

/// A number is written as the C library writes `%f`: its first seventeen significant digits,
/// zeros after them, six places.
///
/// Behaviour: ui.options.a-number-preference-is-written-as-percent-f
#[test]
fn a_number_preference_is_written_as_percent_f() {
    let mut n = Number::new();
    assert!(n.set_value_text("1e20"));
    assert_eq!(n.value_text(), "100000002004087730000.000000");
    assert!(n.set_value_text("0.0078125"));
    assert_eq!(n.value_text(), "0.007813", "a tie rounds up");
    assert!(n.set_value_text("-0.0000005"));
    assert_eq!(n.value_text(), "-0.000000");
}

/// Each kind reads its value from the file's text and writes it back: a yes-or-no only as `1` or
/// `0` (its default's text sets the value now whatever it says); a number only when the whole text
/// is one, written with six places; a choice by a whole number, else by its name in its
/// enumeration (a name no choice has gives the default; a name the enumeration lacks, nothing),
/// written by that name when it has one, else as a number; a text as it is. A new number is a
/// half, over 0 to 1.
///
/// Behaviour: ui.options.each-preference-reads-and-writes-its-value-as-the-file-keeps-it
#[test]
fn each_preference_reads_and_writes_its_value_as_the_file_keeps_it() {
    let mut f = Flag::new();
    assert!(f.set_value_text("1") && f.get());
    assert!(!f.set_value_text("true") && f.get(), "only 1 or 0");
    assert_eq!(f.value_text(), "1");
    assert!(!f.set_default_text("yes"));
    assert!(!f.get(), "the value now set to the default anyway");
    assert_eq!(f.default_text(), "0");

    let mut n = Number::new();
    assert_eq!(
        (n.get(), n.default_value(), n.range()),
        (0.5, 0.5, (0.0, 1.0))
    );
    assert!(n.set_value_text("4.5") && n.get() == 4.5);
    assert!(!n.set_value_text("6m"));
    assert!(n.set_value_text(" 0.25"), "leading spaces read");
    assert_eq!(n.value_text(), "0.250000");
    assert!(n.set_default_text("1e1"));
    assert_eq!((n.get(), n.default_text().as_str()), (10.0, "10.000000"));

    let mut c = detail();
    assert!(c.set_value_text("veryhigh", &Names));
    assert_eq!(c.get(), 4);
    assert_eq!(c.value_text(&Names), "VeryHigh");
    assert!(
        c.set_value_text("0x9", &Names),
        "a number as the C library reads it"
    );
    assert_eq!(c.value_text(&Names), "9", "no name: a number");
    assert!(
        !c.set_value_text("Medium", &Names),
        "a name the enumeration lacks"
    );
    let mut unnamed = detail();
    unnamed.clear_choices();
    unnamed.assign_named_choice(1, (8, 101), 0x51);
    unnamed.assign_named_choice(3, (8, 103), 0x52);
    assert_eq!(
        unnamed.find_value(0x53),
        3,
        "a name no choice has: the default"
    );
    assert!(unnamed.set_value_text("Low", &Names) && unnamed.get() == 1);
    assert!(unnamed.set_value_text("VeryHigh", &Names) && unnamed.get() == 3);
    assert_eq!(c.examine_named_choice(1), Some(((8, 101), 0x51)));
    assert_eq!(c.examine_named_choice(9), None);
    assert_eq!(c.value_text(&NoEnumerations), "9");

    let mut t = Text::new();
    t.set_default("Main.keymap");
    t.apply();
    assert!(!t.changed());
    t.set_value("MAIN.KEYMAP");
    assert!(!t.changed(), "letter case ignored");
    t.set_value("Other.keymap");
    assert!(t.changed());
    t.recall();
    assert_eq!(t.get(), "Main.keymap", "the value applied");
}

/// Two preferences compare by their five names, their kind and, of the same kind, their values (a
/// number's range too, a choice's enumeration too); a preference copies only another of its own
/// kind, and a copy is new, its value not yet applied.
///
/// Behaviour: ui.options.a-preference-compares-and-copies-by-kind
#[test]
fn a_preference_compares_and_copies_by_kind() {
    let mut a = Number::new();
    a.description.section = CAMERA;
    a.description.key = DISTANCE;
    a.set_range(-0.2, 10.0);
    a.set_default(4.5);
    let mut b = a;
    assert_eq!(Pref::Number(a).compare(&Pref::Number(b)), 0);
    b.set_value(6.0);
    b.set_range(0.0, 10.0);
    b.description.help = 7;
    assert_eq!(Pref::Number(a).compare(&Pref::Number(b)), 3);
    let flag = Pref::Flag(Flag::new());
    assert_eq!(Pref::Number(a).compare(&flag), 3, "section, key and kind");
    let mut p = Pref::Number(a);
    assert!(!p.copy_from(&flag));
    assert!(p.copy_from(&Pref::Number(b)));
    assert_eq!(p, Pref::Number(b.duplicate()));
    let mut applied = Flag::new();
    applied.set_value(true);
    applied.apply();
    assert!(!applied.changed());
    assert!(
        applied.duplicate().changed(),
        "a copy's applied value is a new one's"
    );
}

fn number(default: f32) -> Pref {
    let mut n = Number::new();
    n.set_range(-0.2, 10.0);
    n.set_default(default);
    Pref::Number(n)
}

/// A preference blessed is copied into the bank and read from the file by its section's and key's
/// names (its default when the file lacks it), as applied; the owner's preference then holds the
/// bank's value. Blessed again, the bank's takes the new description and values without the file,
/// and its handler and monitor are told of the change. Every preference is written back by its
/// names, a choice by its name.
///
/// Behaviour: ui.options.a-blessed-preference-is-read-from-the-file-once-and-written-back
#[test]
fn a_blessed_preference_is_read_from_the_file_once_and_written_back() {
    let mut file = File::default();
    file.write("Camera", "CameraDistance", "6.000000");
    file.write("Graphics", "Detail", "High");
    let mut bank = Bank::new();
    let mut out = Vec::new();
    let mut p = number(4.5);
    assert!(bank.bless(&mut p, CAMERA, DISTANCE, &file, &Names, &mut out));
    let Pref::Number(n) = &p else { unreachable!() };
    assert_eq!(n.get(), 6.0, "the owner holds the bank's value");
    let Some(Pref::Number(kept)) = bank.find(DISTANCE) else {
        panic!("kept")
    };
    assert_eq!(kept.get(), 6.0);
    assert!(!kept.changed(), "read as applied");
    let mut tilt = number(0.25);
    bank.bless(&mut tilt, CAMERA, TILT, &file, &Names, &mut out);
    assert_eq!(
        bank.find(TILT).map(|t| t.value_text(&Names)).as_deref(),
        Some("0.250000")
    );
    let mut d = Pref::Choice(detail());
    bank.bless(&mut d, GRAPHICS, TEXTURE_DETAIL, &file, &Names, &mut out);
    assert_eq!(
        bank.find(TEXTURE_DETAIL)
            .map(|t| t.value_text(&Names))
            .as_deref(),
        Some("High")
    );
    assert!(out.is_empty(), "no one to tell");

    assert!(bank.link(DISTANCE, 1));
    assert!(bank.attach(CAMERA, 9, &mut out));
    out.clear();
    file.write("Camera", "CameraDistance", "2.000000");
    let mut again = number(3.0);
    bank.bless(&mut again, CAMERA, DISTANCE, &file, &Names, &mut out);
    assert_eq!(
        bank.find(DISTANCE).map(|t| t.value_text(&Names)).as_deref(),
        Some("3.000000"),
        "the new one's values, not the file's"
    );
    assert!(matches!(
        &out[..],
        [
            Notice::Change {
                party: Party::Handler(1),
                ..
            },
            Notice::Change {
                party: Party::Monitor(9),
                ..
            }
        ]
    ));
    let mut kept = File::default();
    assert!(bank.save(&mut kept, &Names));
    assert_eq!(kept.0.len(), 3);
    assert_eq!(
        kept.0[&("Camera".into(), "CameraDistance".into())],
        "3.000000"
    );
    assert_eq!(kept.0[&("Graphics".into(), "Detail".into())], "High");
    assert!(
        !bank::save(&Pref::Text(Text::new()), &mut kept, &Names),
        "no section or key"
    );
}

/// A section's monitor is told its preferences when it attaches and when one is put in or
/// dropped; a key's handler is one only. Applying a section hands each handler its changed
/// preferences there, now applied; submitting hands them over without applying them; recalling
/// puts back every preference of the section and hands each handler its own; synchronizing takes
/// them as applied, no one told; applying one hands it to its handler alone.
///
/// Behaviour: ui.options.the-bank-tells-the-page-and-the-handlers-what-they-hold
#[test]
fn the_bank_tells_the_page_and_the_handlers_what_they_hold() {
    let file = File::default();
    let mut bank = Bank::new();
    let mut out = Vec::new();
    for (key, d) in [(DISTANCE, 4.5), (TILT, 0.25)] {
        bank.bless(&mut number(d), CAMERA, key, &file, &Names, &mut out);
    }
    let mut auto = Pref::Flag(Flag::new());
    bank.bless(&mut auto, CAMERA, AUTO_DISTANCE, &file, &Names, &mut out);
    assert!(bank.link(DISTANCE, 1) && bank.link(TILT, 1) && bank.link(AUTO_DISTANCE, 2));
    assert!(!bank.link(TILT, 2), "one handler a key");
    assert!(bank.attach(CAMERA, 9, &mut out));
    assert!(!bank.attach(CAMERA, 8, &mut out), "one monitor a section");
    let [Notice::Reload {
        monitor: 9,
        section: CAMERA,
        prefs,
    }] = &out[..]
    else {
        panic!("{out:?}")
    };
    let keys: Vec<u32> = prefs.iter().map(|p| p.description().key).collect();
    assert_eq!(keys, [DISTANCE, TILT, AUTO_DISTANCE], "in the order put in");
    out.clear();

    let mut tried = number(4.5);
    tried.description_mut().section = CAMERA;
    tried.description_mut().key = TILT;
    let Pref::Number(t) = &mut tried else {
        unreachable!()
    };
    t.set_value(0.75);
    assert!(bank.take_value(&tried));
    bank.change(&tried, &mut out);
    assert_eq!(out.len(), 2);
    out.clear();
    bank.submit(CAMERA, &mut out);
    assert!(matches!(&out[..], [Notice::Handle { handler: 1, prefs }] if prefs.len() == 1));
    assert!(
        bank.find(TILT).is_some_and(Pref::changed),
        "submitted, not applied"
    );
    out.clear();
    bank.apply(CAMERA, &mut out);
    assert!(matches!(&out[..], [Notice::Handle { handler: 1, prefs }] if prefs.len() == 1));
    assert!(!bank.find(TILT).is_some_and(Pref::changed));
    out.clear();
    bank.apply(CAMERA, &mut out);
    assert!(out.is_empty(), "nothing changed");

    let mut on = Flag::new();
    on.description.section = CAMERA;
    on.description.key = AUTO_DISTANCE;
    on.set_value(true);
    bank.take_value(&Pref::Flag(on));
    bank.take_value(&{
        let mut x = tried.clone();
        if let Pref::Number(n) = &mut x {
            n.set_value(0.1);
        }
        x
    });
    bank.recall(CAMERA, &mut out);
    let handled: Vec<(u32, usize)> = out
        .iter()
        .map(|n| match n {
            Notice::Handle { handler, prefs } => (*handler, prefs.len()),
            _ => (0, 0),
        })
        .collect();
    assert_eq!(handled, [(1, 2), (2, 1)], "every preference of the section");
    assert_eq!(
        bank.find(TILT).map(|t| t.value_text(&Names)).as_deref(),
        Some("0.750000"),
        "back to the value applied"
    );
    assert_eq!(
        bank.find(AUTO_DISTANCE)
            .map(|t| t.value_text(&Names))
            .as_deref(),
        Some("0")
    );
    out.clear();

    bank.take_value(&Pref::Flag(on));
    bank.synchronize(CAMERA);
    assert!(!bank.find(AUTO_DISTANCE).is_some_and(Pref::changed));
    bank.take_value(&Pref::Flag(Flag::new()));
    bank.apply_one(&Pref::Flag(on), &mut out);
    assert!(matches!(&out[..], [Notice::Handle { handler: 2, prefs }] if prefs.len() == 1));
    out.clear();

    assert!(bank.destroy(TILT, &mut out));
    assert!(matches!(&out[..], [Notice::Reload { prefs, .. }] if prefs.len() == 2));
    assert!(bank.find(TILT).is_none());
    bank.detach(CAMERA);
    out.clear();
    bank.reload(CAMERA, &mut out);
    assert!(out.is_empty(), "no monitor");
}
