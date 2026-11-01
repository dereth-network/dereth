//! The script library loads into the machine's form and links.

use auberean_script::library::NO_CODE;
use auberean_script::native::{Binding, Call, NativeTable, Status, Stop};
use auberean_script::opcode::{self, op};
use auberean_script::value::NativeId;
use auberean_script::Program;

use crate::common::need_target;

fn refuse(c: &mut Call<'_>) -> Result<Status, Stop> {
    Err(c.fail("not under test"))
}

/// Behaviour: script.library.loads-and-links
#[test]
fn the_library_links_every_native_call_and_turns_direct_calls_into_code_offsets() {
    let (_, lib) = need_target!();
    assert_eq!(lib.code.len() * 4, 9_930_184);
    assert_eq!(lib.packages.len(), 9_955);
    assert_eq!(lib.literals.len(), 26_353);
    let mut t = NativeTable::new();
    for p in lib.packages.iter().filter(|p| p.is_native()) {
        for f in &p.functions {
            t.bind(NativeId(f.id.0), Binding::Ported(refuse));
        }
    }
    let prog = Program::link(lib.clone(), t).expect("the library links");
    assert_eq!(prog.natives.len(), 1_026, "distinct natives the code calls");
    let mut at = 0;
    let (mut native_calls, mut direct, mut direct_empty) = (0, 0, 0);
    while at < prog.code.len() {
        let w = prog.code[at];
        match opcode::low_byte(w) {
            op::CALL_NATIVE => {
                native_calls += 1;
                assert!((prog.code[at + 1] as usize) < prog.natives.len());
            }
            op::CALL_AT => {
                direct += 1;
                let o = prog.code[at + 1];
                if o == NO_CODE {
                    direct_empty += 1;
                } else {
                    assert_eq!(o % 4, 0);
                    assert!((o as usize) < lib.code.len() * 4);
                }
            }
            op::CALL => assert_eq!(
                prog.code[at + 1] & 0x8000_0000,
                0,
                "a direct call left unlinked"
            ),
            _ => {}
        }
        at += opcode::length(&prog.code, at).unwrap();
    }
    assert_eq!(at, prog.code.len());
    assert_eq!(native_calls, 40_528);
    assert_eq!(direct, 95_048);
    eprintln!("direct calls to an empty slot: {direct_empty}");
}

/// Behaviour: script.library.unbound-native-fails-the-load
#[test]
fn a_native_call_the_table_does_not_know_fails_the_load() {
    let (_, lib) = need_target!();
    let err = Program::link(lib.clone(), NativeTable::new()).unwrap_err();
    assert!(
        matches!(err, auberean_script::link::LinkError::Unbound { .. }),
        "{err}"
    );
}
