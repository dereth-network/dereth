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
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(0, tag::INT).unwrap();
        cx.push(b, tag::MIXED).unwrap();
        cx.push(a, tag::MIXED).unwrap();
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(PROFILE, number).0;
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

    fn read(&self, r: u32) -> GameplayOptions {
        match self.vm.heap.native(r).unwrap() {
            NativeObject::Options(p) => (**p).clone(),
            _ => panic!("a profile"),
        }
    }
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
