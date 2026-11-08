//! The examination profile natives, each called as the machine calls a native: its frame built by
//! hand (the result slot, the arguments last first, the receiver), then the native by id.

use std::sync::Arc;

use auberean_glue::examination::{kind, Profile};
use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

const PROFILE: u32 = 0x99;

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

    /// A new profile, as the native factory makes one.
    fn profile(&mut self) -> u32 {
        let new = NativeObject::new(&self.prog.library, native_type::EXAMINATION_PROFILE);
        self.vm
            .heap
            .add_native(native_type::EXAMINATION_PROFILE, new)
    }

    /// An examination request's packed form, as a native object.
    fn request(&mut self, words: &[u32]) -> u32 {
        let bytes = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.vm.heap.add_native(
            native_type::EXAMINATION_REQUEST,
            NativeObject::Packed {
                ty: native_type::EXAMINATION_REQUEST,
                bytes,
            },
        )
    }

    /// Call a profile native: the result slot, the arguments (`args[0]` at `fp[-2]`), the
    /// receiver. Returns the status and the slot below the arguments.
    fn call(&mut self, number: u32, receiver: u32, args: &[(u32, u8)]) -> (Status, (u32, u8)) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(0, tag::INT).unwrap();
        for (v, t) in args.iter().rev() {
            cx.push(*v, *t).unwrap();
        }
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
        (status, cx.slot(0).unwrap())
    }

    fn read(&self, r: u32) -> Profile {
        let NativeObject::Packed { bytes, .. } = self.vm.heap.native(r).unwrap() else {
            panic!("a packed profile")
        };
        Profile::unpack(bytes).expect("reads")
    }
}

/// Lines stay in order of their tab words, whatever order they are added in; among equal tab words,
/// in the order they came.
///
/// Behaviour: script.natives.examination-profile-keeps-its-lines-in-order-of-their-tabs
#[test]
fn a_profile_keeps_its_lines_in_order_of_their_tabs() {
    let mut b = Bench::new();
    let p = b.profile();
    let q = b.request(&[0x4000_0002, 0, 7, 0, 0x3FF]);
    b.call(0x03, p, &[(q, tag::REFERENCE)]);
    let int = tag::INT;
    for (value, tabs) in [(1, 0x100), (2, 0x02), (3, 0x02), (4, 0x08)] {
        b.call(0x08, p, &[(value, int), (0xAB, int), (tabs, int)]);
    }
    let order: Vec<u64> = b.read(p).nodes.iter().map(|n| n.value).collect();
    assert_eq!(order, vec![2, 3, 4, 1]);
}

/// Behaviour: script.natives.examination-profile-keeps-the-asked-tabs-lines
#[test]
fn a_profile_keeps_the_lines_of_the_tabs_asked_and_notes_every_tab() {
    let mut b = Bench::new();
    let p = b.profile();
    assert_eq!(b.read(p), Profile::new(), "a new profile is empty");
    // A request about an object asking for the effects tab (0x10).
    let q = b.request(&[0x4000_0002, 0, 7, 0, 0x10]);
    let int = tag::INT;
    assert_eq!(b.call(0x03, p, &[(q, tag::REFERENCE)]).0, Status::Success);
    // An integer on the details tab (dropped), one on the effects tab, one on the first tab.
    b.call(0x08, p, &[(5, int), (0xAB, int), (0x08, int)]);
    b.call(0x08, p, &[(6, int), (0xAB, int), (0x10, int)]);
    b.call(0x08, p, &[(7, int), (0xAB, int), (0x01, int)]);
    // A break, two indents, a picture, a 64-bit number (low word first) and a time left.
    b.call(0x13, p, &[(0x10, int)]);
    b.call(0x16, p, &[(2, int), (0x10, int)]);
    b.call(0x11, p, &[(0x0600_0001, int), (0x10, int)]);
    b.call(0x15, p, &[(1, int), (2, int), (0xCD, int), (0x10, int)]);
    let t = 90.5f64.to_bits();
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (t as u32, (t >> 32) as u32);
    b.call(0x06, p, &[(hi, int), (lo, int), (0xEF, int), (0x10, int)]);
    let s = b.vm.heap.add_native(
        native_type::STRING_INFO,
        NativeObject::TextInfo(TextInfo {
            string_id: 9,
            table: 0x2300_0001,
            ..TextInfo::default()
        }),
    );
    b.call(0x09, p, &[(s, tag::REFERENCE), (0xAB, int), (0x10, int)]);
    let got = b.read(p);
    assert_eq!(got.tabs, 0x08 | 0x10 | 0x01, "every tab is noted");
    let kinds: Vec<(u32, u64)> = got.nodes.iter().map(|n| (n.kind, n.value)).collect();
    assert_eq!(
        kinds,
        vec![
            (kind::INT, 7),
            (kind::INT, 6),
            (kind::BREAK, 0),
            (kind::INDENT, 2),
            (kind::IMAGE, 0x0600_0001),
            (kind::LONG, 1 << 32 | 2),
            (kind::COUNTDOWN, 0),
            (kind::STRING, 0),
        ]
    );
    assert_eq!(got.nodes[6].time.to_bits(), t);
    assert_eq!(got.nodes[7].text.string_id, 9);
    assert_eq!(Profile::unpack(&got.pack()).unwrap(), got, "packs back");
    // The same profile built again is equal; one line more is not.
    let p2 = b.profile();
    b.call(0x03, p2, &[(q, tag::REFERENCE)]);
    let NativeObject::Packed { bytes, .. } = b.vm.heap.native(p).unwrap().clone() else {
        unreachable!()
    };
    *b.vm.heap.native_mut(p2).unwrap() = NativeObject::Packed {
        ty: native_type::EXAMINATION_PROFILE,
        bytes,
    };
    assert_eq!(b.call(0x01, p, &[(p2, tag::REFERENCE)]).1, (1, tag::MIXED));
    assert_eq!(b.call(0x02, p, &[(p2, tag::REFERENCE)]).1, (0, tag::MIXED));
    b.call(0x12, p2, &[(0x10, int)]);
    assert_eq!(b.call(0x01, p, &[(p2, tag::REFERENCE)]).1, (0, tag::MIXED));
    assert_eq!(
        b.call(0x08, NULL, &[(1, int), (2, int), (3, int)]).0,
        Status::Crash
    );
}

/// A line answers the value of its own kind and nothing else's: a number, a 64-bit number, a
/// float, a yes or no, a picture, a time left (none is -1); a line of no kind is drawn with the
/// examination's text template; a copy of a profile keeps every line and its tabs.
///
/// Behaviour: script.natives.examination-line-answers-its-own-kind
#[test]
fn a_line_answers_only_the_value_of_its_own_kind() {
    use auberean_glue::examination::{default_appearance, Node};
    let fresh = Node::default();
    assert_eq!((fresh.kind, fresh.tabs, fresh.value), (0, 0, 0));
    assert_eq!(fresh.appearance, default_appearance());
    assert_ne!(fresh.appearance, 0);
    assert_eq!(fresh.countdown(), -1.0);
    assert!(fresh.icon().is_empty());
    assert_eq!(Node::new(kind::IMAGE, 1).appearance, default_appearance());
    let with = |k: u32, value: u64| Node {
        value,
        ..Node::new(k, 1)
    };
    let long = with(kind::LONG, 5 << 32 | 9);
    assert_eq!(long.long_int(), 5 << 32 | 9);
    assert_eq!(long.int(), 0);
    let float = with(kind::FLOAT, u64::from(2.5f32.to_bits()));
    assert_eq!(float.float(), 2.5);
    assert_eq!(float.long_int(), 0);
    let yes = with(kind::BOOL, 1);
    assert_eq!(yes.bool(), 1);
    assert_eq!(yes.image(), 0);
    assert_eq!(yes.float(), 0.0);
    let picture = with(kind::IMAGE, 0x0600_0001);
    assert_eq!(picture.image(), 0x0600_0001);
    assert_eq!(picture.bool(), 0);
    let mut time = Node::new(kind::COUNTDOWN, 1);
    time.time = 12.5;
    assert_eq!(time.countdown(), 12.5);
    assert_eq!(picture.countdown(), -1.0);
    let mut p = Profile::new();
    p.add(long);
    p.add(time);
    let copy = p.clone();
    p.nodes.clear();
    assert_eq!(copy.nodes.len(), 2);
    assert_eq!(copy.tabs, 1);
}
