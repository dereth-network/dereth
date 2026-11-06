//! The options profile's chat setters, each called as the machine calls a native: its frame built
//! by hand (the result slot, the arguments last first, the receiver), then the native by id.

use std::sync::Arc;

use auberean_glue::chat_options::{self as chat, kind};
use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::opcode::op;
use auberean_script::options::GameplayOptions;
use auberean_script::value::{tag, NativeId};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

const PROFILE: u32 = 0x9B;
const SET_FILTER: u32 = 0x21;
const SET_FONT_SIZE: u32 = 0x3C;
const SET_FONT_COLOR: u32 = 0x3D;
const SET_DEVOTED: u32 = 0x40;
const SET_POPUP: u32 = 0x52;

struct Bench {
    vm: Vm,
    host: TestHost,
    prog: Arc<Program>,
}

impl Bench {
    fn new() -> Self {
        let lib = library(&[w(op::END)], &[], &[]);
        let table = auberean_glue::natives(&lib);
        let prog = Arc::new(Program::link(lib, table).expect("links"));
        Self {
            vm: Vm::new(Arc::clone(&prog)),
            host: TestHost::default(),
            prog,
        }
    }

    /// A new profile.
    fn profile(&mut self) -> u32 {
        self.vm.heap.add_native(
            native_type::GAMEPLAY_OPTIONS_PROFILE,
            NativeObject::Options(Box::default()),
        )
    }

    /// Call a profile native with two words (`fp[-2]`, `fp[-3]`); the status and the result slot.
    fn call(&mut self, number: u32, receiver: u32, a: u32, b: u32) -> (Status, u32) {
        self.call_in(PROFILE, number, receiver, a, b)
    }

    /// Call a native of a package with two words (`fp[-2]`, `fp[-3]`).
    fn call_in(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        a: u32,
        b: u32,
    ) -> (Status, u32) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(0, tag::INT).unwrap();
        cx.push(b, tag::MIXED).unwrap();
        cx.push(a, tag::MIXED).unwrap();
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(package, number).0;
        let Some(Binding::Ported(f)) = self.prog.table.get(id) else {
            panic!("native {id:#x} is not ported");
        };
        let status = f(&mut Call {
            vm: &mut self.vm,
            cx: &mut cx,
            host: &mut self.host,
            native: id,
        })
        .expect("the native runs");
        (status, cx.slot(0).unwrap().0)
    }

    /// Ask a profile native for its answer (`fp[-2]`).
    fn get(&mut self, number: u32, receiver: u32) -> u32 {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(0, tag::MIXED).unwrap();
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(PROFILE, number).0;
        let Some(Binding::Ported(f)) = self.prog.table.get(id) else {
            panic!("native {id:#x} is not ported");
        };
        f(&mut Call {
            vm: &mut self.vm,
            cx: &mut cx,
            host: &mut self.host,
            native: id,
        })
        .expect("the native runs");
        cx.slot(0).unwrap().0
    }

    fn read(&self, r: u32) -> GameplayOptions {
        match self.vm.heap.native(r).unwrap() {
            NativeObject::Options(p) => (**p).clone(),
            _ => panic!("a profile"),
        }
    }
}

/// The pet panel's and fellows on the map's options read their bit the other way: a cleared
/// profile has them on, and turning one off sets its bit. Another option reads its bit straight.
///
/// Behaviour: script.natives.two-options-are-on-with-their-bit-clear
#[test]
fn two_options_are_on_with_their_bit_clear() {
    const GET_SHOW_FELLOWS_ON_MAP: u32 = 0x49;
    const SET_SHOW_FELLOWS_ON_MAP: u32 = 0x48;
    const GET_SHOW_PET_UI: u32 = 0x4B;
    const SET_SHOW_PET_UI: u32 = 0x4A;
    const GET_SHOW_SHORTCUT_NUMBERS: u32 = 0x10;
    const SET_SHOW_SHORTCUT_NUMBERS: u32 = 0x0F;
    let mut b = Bench::new();
    let p = b.profile();
    for (get, set, bit) in [
        (GET_SHOW_FELLOWS_ON_MAP, SET_SHOW_FELLOWS_ON_MAP, 0x8000),
        (GET_SHOW_PET_UI, SET_SHOW_PET_UI, 0x1_0000),
    ] {
        assert_eq!(b.get(get, p), 1, "on with the bit clear");
        b.call(set, p, 0, 0);
        assert_eq!(b.read(p).bits & bit, bit, "off sets it");
        assert_eq!(b.get(get, p), 0);
        b.call(set, p, 1, 0);
        assert_eq!(b.read(p).bits & bit, 0, "on clears it");
    }
    assert_eq!(b.get(GET_SHOW_SHORTCUT_NUMBERS, p), 0);
    b.call(SET_SHOW_SHORTCUT_NUMBERS, p, 1, 0);
    assert_eq!(b.get(GET_SHOW_SHORTCUT_NUMBERS, p), 1);
    assert_eq!(b.read(p).bits & 1, 1);
}

/// A chat window's filter and devoted channel, a kind's font size, color and popup flag are set
/// in the profile, answering 1; a table a new profile does not hold yet is first made with a new
/// player's values, so the rest keep them; the popup flag keeps the value's low byte.
///
/// Behaviour: script.natives.the-chat-settings-are-set-over-a-new-players
#[test]
fn the_chat_settings_are_set_over_a_new_players() {
    let mut b = Bench::new();
    let p = b.profile();
    assert_eq!(
        b.call(SET_DEVOTED, p, 2, kind::TRADE_CHANNEL),
        (Status::Success, 1)
    );
    assert_eq!(
        b.call(SET_FILTER, p, 2, kind::GENERAL_CHANNEL | kind::SAY)
            .1,
        1
    );
    assert_eq!(b.call(SET_FONT_SIZE, p, kind::SAY, 2).1, 1);
    assert_eq!(b.call(SET_FONT_COLOR, p, kind::SAY, 7).1, 1);
    assert_eq!(b.call(SET_POPUP, p, kind::TELL, 0x101).1, 1);
    let o = b.read(p);
    assert_eq!(chat::devoted_channel(&o, 2), kind::TRADE_CHANNEL);
    assert_eq!(
        chat::devoted_channel(&o, 3),
        kind::FELLOWSHIP,
        "the others kept"
    );
    assert_eq!(chat::filter(&o, 2), kind::GENERAL_CHANNEL | kind::SAY);
    assert_eq!(chat::filter(&o, 4), kind::ALLEGIANCE);
    assert_eq!(chat::font_size(&o, kind::SAY), 2);
    assert_eq!(chat::font_size(&o, kind::TELL), chat::SMALL_FONT);
    assert_eq!(chat::font_color(&o, kind::SAY), 7);
    assert_eq!(chat::font_color(&o, kind::BROADCAST), 9);
    assert!(chat::popup(&o, kind::TELL));
    assert!(!chat::popup(&o, kind::SAY));
    assert!(
        chat::popup(&o, kind::FELLOWSHIP),
        "the popup table made with a new player's flags first"
    );
    let devoted = o.devoted_windows.as_ref().expect("made");
    assert_eq!(
        devoted.header, 0x0200_0004,
        "four entries in a new table's buckets"
    );
    assert_eq!(
        o.font_colors.as_ref().map(|t| t.entries.len()),
        Some(chat::chat_defaults::FONT_COLORS.len())
    );
    // Set again: changed in place.
    b.call(SET_DEVOTED, p, 2, kind::SAY);
    let o = b.read(p);
    assert_eq!(chat::devoted_channel(&o, 2), kind::SAY);
    assert_eq!(o.devoted_windows.map(|t| t.entries.len()), Some(4));
}

/// Kinds of text added to a chat window's filter are or'ed into it (a window with no filter gets
/// them), and taken out are cleared from it; taking out of a window with no filter leaves it
/// without one, and a profile that holds no filters answers from a new one's; each answers 1.
///
/// Behaviour: script.natives.a-chat-windows-filter-gains-and-loses-kinds
#[test]
fn a_chat_windows_filter_gains_and_loses_kinds_of_text() {
    const ADD_FILTER: u32 = 0x1F;
    const REMOVE_FILTER: u32 = 0x20;
    let mut b = Bench::new();
    let p = b.profile();
    // Taken out of a window a new profile filters: the filters made, that one changed.
    assert_eq!(
        b.call(REMOVE_FILTER, p, 4, kind::ALLEGIANCE),
        (Status::Success, 1)
    );
    let o = b.read(p);
    assert_eq!(chat::filter(&o, 4), 0);
    assert_eq!(
        chat::filter(&o, 3),
        chat::filter(&GameplayOptions::default(), 3)
    );
    // Taken out of a window with no filter: still none.
    let fresh = b.profile();
    assert_eq!(b.call(REMOVE_FILTER, fresh, 9, kind::SAY).1, 1);
    let o = b.read(fresh);
    assert!(o.filters.is_none(), "nothing made");
    // Added: or'ed in; a window with none gets the kinds.
    assert_eq!(b.call(ADD_FILTER, p, 4, kind::SAY).1, 1);
    assert_eq!(b.call(ADD_FILTER, p, 4, kind::TELL).1, 1);
    assert_eq!(b.call(ADD_FILTER, p, 9, kind::TRADE_CHANNEL).1, 1);
    let o = b.read(p);
    assert_eq!(chat::filter(&o, 4), kind::SAY | kind::TELL);
    assert_eq!(chat::filter(&o, 9), kind::TRADE_CHANNEL);
    b.call(REMOVE_FILTER, p, 4, kind::SAY | kind::GENERAL_CHANNEL);
    assert_eq!(chat::filter(&b.read(p), 4), kind::TELL);
}

/// A profile's shortcuts set from a list become a copy of each shortcut the list names, in order
/// (later changes to the script's shortcuts leave them); clearing a table of the windows' saved
/// places takes every window out and keeps its size.
///
/// Behaviour: script.natives.shortcuts-set-from-a-list-and-saved-places-cleared
#[test]
fn a_profiles_shortcuts_are_set_from_a_list_and_saved_places_clear() {
    use auberean_script::library::Elem;
    use auberean_script::native_object::Item;
    use auberean_script::options::Shortcut;
    let mut b = Bench::new();
    let p = b.profile();
    let add = |b: &mut Bench, s: Shortcut| {
        b.vm.heap
            .add_native(native_type::SHORTCUT_INFO, NativeObject::Shortcut(s))
    };
    let first = Shortcut {
        kind: 0x4000_0001,
        data_enum: 7,
        ..Shortcut::default()
    };
    let second = Shortcut {
        kind: 0x4000_0003,
        text: "/wave".encode_utf16().collect(),
        ..Shortcut::default()
    };
    let (a, c) = (add(&mut b, first.clone()), add(&mut b, second.clone()));
    let list = b.vm.heap.add_native(
        0x65,
        NativeObject::Sequence {
            item: Elem::Ref,
            items: vec![Item::Ref(a), Item::Ref(c)],
        },
    );
    assert_eq!(b.call(0x0A, p, list, 0).0, Status::Success);
    if let NativeObject::Shortcut(s) = b.vm.heap.native_mut(a).unwrap() {
        s.data_enum = 99;
    }
    assert_eq!(b.read(p).shortcuts, vec![first, second]);
    assert_eq!(
        b.call(0x0A, auberean_script::value::NULL, list, 0).0,
        Status::Crash
    );

    // A table of 47 buckets (size place 2) holding one window with one element.
    let words: [u32; 9] = [2 << 24 | 1, 0x10, 2 << 24 | 1, 0x20, 1, 2, 3, 4, 1];
    let t = b.vm.heap.add_native(
        native_type::UI_SAVE_LOCATIONS,
        NativeObject::Packed {
            ty: native_type::UI_SAVE_LOCATIONS,
            bytes: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        },
    );
    assert_eq!(b.call_in(0x6F, 0x01, t, 0, 0).0, Status::Success);
    let NativeObject::Packed { bytes, .. } = b.vm.heap.native(t).unwrap() else {
        panic!("a packed table")
    };
    assert_eq!(
        bytes,
        &(2u32 << 24).to_le_bytes().to_vec(),
        "empty, its size kept"
    );
}

/// `/exportoptions` writes the profile to a file in the client's folder and `/importoptions` reads
/// one back into the profile, each answering 0; a host that writes nothing answers that the file
/// was not written (50002), one that holds no such file that it did not read (50000), an empty
/// name 50003; a failed import leaves the profile as it was.
///
/// Behaviour: script.natives.options-export-and-import-through-the-hosts-files
#[test]
fn the_options_export_to_a_file_and_import_back_and_answer_the_clients_errors() {
    const EXPORT: u32 = 0x2B;
    const IMPORT: u32 = 0x2C;
    let mut b = crate::call::Bench::new();
    let p = b.object(
        native_type::GAMEPLAY_OPTIONS_PROFILE,
        NativeObject::Options(Box::default()),
    );
    if let NativeObject::Options(o) = b.vm.heap.native_mut(p).unwrap() {
        o.set_alias(
            &";wm".encode_utf16().collect::<Vec<_>>(),
            &"Well met".encode_utf16().collect::<Vec<_>>(),
        );
    }
    let (name, empty) = (b.text("mine.txt"), b.text(""));
    let answer = |b: &mut crate::call::Bench, n: u32, r: u32, f: u32| {
        let (status, slots) = b.call(PROFILE, n, r, &[f], 1);
        assert_eq!(status, Status::Success);
        slots[0].0
    };
    // No folder: not written, not read.
    assert_eq!(answer(&mut b, EXPORT, p, name), 50_002);
    assert_eq!(answer(&mut b, IMPORT, p, name), 50_000);
    assert_eq!(answer(&mut b, EXPORT, p, empty), 50_003);
    b.host.folder = Some(std::collections::BTreeMap::new());
    assert_eq!(answer(&mut b, EXPORT, p, name), 0);
    let written = b.host.folder.as_ref().unwrap()["mine.txt"].clone();
    let text = String::from_utf8_lossy(&written);
    assert!(
        text.contains("\"GAMEPLAY OPTIONS\"") && text.contains(";wm [ \"Well met\" ]"),
        "{text}"
    );
    let fresh = b.object(
        native_type::GAMEPLAY_OPTIONS_PROFILE,
        NativeObject::Options(Box::default()),
    );
    assert_eq!(answer(&mut b, IMPORT, fresh, name), 0);
    let read = |b: &crate::call::Bench, r: u32| match b.vm.heap.native(r).unwrap() {
        NativeObject::Options(o) => (**o).clone(),
        _ => panic!("a profile"),
    };
    assert_eq!(read(&b, fresh), read(&b, p));
    // A file that does not read leaves the profile alone.
    b.host
        .folder
        .as_mut()
        .unwrap()
        .insert("bad.txt".into(), b"[ [".to_vec());
    let bad = b.text("bad.txt");
    let other = b.object(
        native_type::GAMEPLAY_OPTIONS_PROFILE,
        NativeObject::Options(Box::default()),
    );
    assert_eq!(answer(&mut b, IMPORT, other, bad), 50_000);
    assert_eq!(read(&b, other), GameplayOptions::default());
    assert_eq!(answer(&mut b, IMPORT, other, empty), 50_003);
    // A profile that is not there stops the call.
    assert_eq!(
        b.call(PROFILE, EXPORT, auberean_script::value::NULL, &[name], 1)
            .0,
        Status::Crash
    );
}
