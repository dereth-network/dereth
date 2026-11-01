//! The scripts' debug lines, each native called as the machine calls one (its arguments last
//! first, the receiver) on a host that keeps what is printed.

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

const DEBUG: u32 = 0x1C;

/// What debug native `number` printed for the words `args` (`fp[-2]` first, so a 64-bit value's
/// high word first); a string argument is given as its text and handed over as a new string.
fn printed(number: u32, args: &[u32], text: Option<&str>) -> Vec<String> {
    let lib = library(&[w(op::END)], &[], &[]);
    let table = auberean_glue::natives(&lib);
    let prog = Arc::new(Program::link(lib, table).expect("links"));
    let mut vm = Vm::new(Arc::clone(&prog));
    let mut host = TestHost::default();
    let id = NativeId::new(DEBUG, number).0;
    let Some(Binding::Ported(f)) = prog.table.get(id) else {
        panic!("native {id:#x} is not ported");
    };
    let mut cx = Context::new(usize::MAX, 0);
    let mut words: Vec<(u32, u8)> = args.iter().map(|a| (*a, tag::MIXED)).collect();
    if let Some(t) = text {
        let s = vm.heap.add_native(
            native_type::WP_STRING,
            NativeObject::Text(t.encode_utf16().collect()),
        );
        words.push((s, tag::REFERENCE));
    }
    for (v, t) in words.iter().rev() {
        cx.push(*v, *t).unwrap();
    }
    cx.push(0, tag::REFERENCE).unwrap();
    cx.fp = cx.sp;
    let status = f(&mut Call {
        vm: &mut vm,
        cx: &mut cx,
        host: &mut host,
        native: id,
    })
    .expect("the native runs");
    assert_eq!(status, Status::Success);
    host.printed
}

/// A debug native prints one line after `interp:  `: an integer in hexadecimal and signed, a
/// 64-bit one likewise (its hexadecimal in capitals), a float to three places and a 64-bit float
/// to six (rounded as the C library rounds the exact value, a negative keeping its sign), a
/// string as it is.
///
/// Behaviour: script.glue.debug-lines-are-printed-for-the-console
#[test]
fn debug_lines_are_printed_for_the_console() {
    assert_eq!(
        printed(1, &[0xFFFF_FFFE], None),
        ["interp:  0xfffffffe (-2)"]
    );
    let (lo, hi) = (0x89AB_CDEF, 0x0123_4567);
    assert_eq!(
        printed(2, &[hi, lo], None),
        ["interp:  0x0123456789ABCDEF (81985529216486895)"]
    );
    assert_eq!(printed(3, &[2.0625f32.to_bits()], None), ["interp:  2.063"]);
    let d = (-0.000_000_5f64).to_bits();
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (d as u32, (d >> 32) as u32);
    assert_eq!(printed(4, &[hi, lo], None), ["interp:  -0.000000"]);
    assert_eq!(printed(5, &[], Some("hello")), ["interp:  hello"]);
}
