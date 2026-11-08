//! The enumeration natives over the install's data: each answers what the data files hold, in the
//! shape the client gives it.

use std::any::Any;
use std::sync::Arc;

use auberean_assets::Database;
use auberean_script::context::Context;
use auberean_script::graph::SavedState;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Host, Status, Vm};

use crate::world::target;

/// A host holding the install's records.
struct Records(Database);

impl Host for Records {
    fn saved_state(&mut self, _: u32) -> Option<Arc<SavedState>> {
        None
    }

    fn database(&mut self) -> Option<&Database> {
        Some(&self.0)
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

const ENUM_MAPPER: u32 = 0x23;

/// Call enumeration native `number` with `args` (`fp[-2]` first) and `results` result slots:
/// the result slots, nearest first.
fn call(vm: &mut Vm, host: &mut Records, number: u32, args: &[u32], results: usize) -> Vec<u32> {
    let id = NativeId::new(ENUM_MAPPER, number).0;
    let Some(Binding::Ported(f)) = vm.program().table.get(id) else {
        panic!("native {id:#x} is not ported");
    };
    let mut cx = Context::new(usize::MAX, 0);
    for _ in 0..results {
        cx.push(0, tag::INT).unwrap();
    }
    for a in args.iter().rev() {
        cx.push(*a, tag::MIXED).unwrap();
    }
    cx.push(NULL, tag::REFERENCE).unwrap();
    cx.fp = cx.sp;
    let status = f(&mut Call {
        vm,
        cx: &mut cx,
        host,
        native: id,
    })
    .expect("the native runs");
    assert_eq!(status, Status::Success);
    (0..results).rev().map(|k| cx.slot(k).unwrap().0).collect()
}

fn text(vm: &Vm, id: u32) -> String {
    match vm.heap.native(id).expect("an object") {
        NativeObject::Text(t) => String::from_utf16_lossy(t),
        o => panic!("not a string: {o:?}"),
    }
}

/// **An enumeration's names, its count and its lists come from its record**: a value's name as a
/// new string (the plain name and the name for logs alike, empty for a value it lacks), how many
/// values it has, every value as a new list of words and every name as a new list of new strings,
/// in the enumeration's order; and a name looked up in a record named directly.
///
/// Behaviour: script.glue.enumeration-names-and-lists
#[test]
fn an_enumerations_names_count_and_lists_come_from_its_record() {
    let (dat, prog) = target().expect("the September 2013 install opens");
    let mut host = Records(Database::new(vec![Arc::clone(dat)]));
    let mut vm = Vm::new(Arc::clone(prog));
    let mut checked = 0;
    for e in 0x4000_0000..0x4000_0400 {
        let values = host.0.enumeration_values(e);
        if values.is_empty() {
            continue;
        }
        let names = host.0.enumeration_names(e);
        assert_eq!(
            call(&mut vm, &mut host, 0x06, &[e], 1)[0] as usize,
            host.0.enumeration_len(e)
        );
        let list = call(&mut vm, &mut host, 0x07, &[e], 1)[0];
        let NativeObject::Sequence { items, .. } = vm.heap.native(list).unwrap().clone() else {
            panic!("a list of values");
        };
        assert_eq!(
            items,
            values.iter().map(|v| Item::Word(*v)).collect::<Vec<_>>()
        );
        let list = call(&mut vm, &mut host, 0x08, &[e], 1)[0];
        let NativeObject::Sequence { items, .. } = vm.heap.native(list).unwrap().clone() else {
            panic!("a list of names");
        };
        let got: Vec<String> = items
            .iter()
            .map(|i| match i {
                Item::Ref(r) => text(&vm, *r),
                i => panic!("not a reference: {i:?}"),
            })
            .collect();
        assert_eq!(got, names, "enumeration {e:#x}");
        for v in values.iter().take(8) {
            let want = host.0.enumeration_name(e, *v).unwrap_or_default();
            for number in [0x02, 0x04] {
                let s = call(&mut vm, &mut host, number, &[e, *v], 1)[0];
                assert_eq!(text(&vm, s), want, "enumeration {e:#x} value {v:#x}");
            }
        }
        let s = call(&mut vm, &mut host, 0x02, &[e, 0xFFFF_FFF0], 1)[0];
        assert_eq!(text(&vm, s), "", "a value it lacks");
        checked += 1;
    }
    assert!(checked >= 10, "{checked} enumerations");
}

/// Call the enumeration native `number` that looks a name up (`fp[-2]` the enumeration, `fp[-3]`
/// the name as a new string, `fp[-4]` a reference to a local): the local's value and whether it
/// was found.
fn look_up(vm: &mut Vm, host: &mut Records, number: u32, map: u32, name: &str) -> (u32, u32) {
    use auberean_script::native_object::native_type;
    let id = NativeId::new(ENUM_MAPPER, number).0;
    let Some(Binding::Ported(f)) = vm.program().table.get(id) else {
        panic!("native {id:#x} is not ported");
    };
    let name = vm.heap.add_native(
        native_type::WP_STRING,
        NativeObject::Text(name.encode_utf16().collect()),
    );
    let local = vm.heap.add_relative(NULL, 0);
    let mut cx = Context::new(usize::MAX, 0);
    cx.push(0, tag::INT).unwrap();
    cx.push(0, tag::INT).unwrap();
    cx.push(local, tag::REFERENCE).unwrap();
    cx.push(name, tag::REFERENCE).unwrap();
    cx.push(map, tag::MIXED).unwrap();
    cx.push(NULL, tag::REFERENCE).unwrap();
    cx.fp = cx.sp;
    let status = f(&mut Call {
        vm,
        cx: &mut cx,
        host,
        native: id,
    })
    .expect("the native runs");
    assert_eq!(status, Status::Success);
    (cx.slot(0).unwrap().0, cx.slot(1).unwrap().0)
}

/// **An admin's words the data files lack are the client's own**: no install holds the
/// enumeration of the ethereality overrides or the log type an admin is answered under, so the
/// client supplies them: `super`, `normal` and `admin` (letter case ignored) are found with the
/// values the servers read, and the log type `Admin` is found and named; a word of neither is not.
///
/// Behaviour: script.glue.admin-words-the-data-lacks-are-the-clients-own
#[test]
fn the_admin_words_the_data_lacks_are_the_clients_own() {
    const ETHEREALITY: u32 = 0x4100_0026;
    const LOG_TYPE: u32 = 0x4000_002C;
    let (dat, prog) = target().expect("the September 2013 install opens");
    let mut host = Records(Database::new(vec![Arc::clone(dat)]));
    let mut vm = Vm::new(Arc::clone(prog));
    for (map, word) in [(ETHEREALITY, "Super"), (LOG_TYPE, "Admin")] {
        assert!(host.0.enumeration_values(map).is_empty(), "{map:#x}");
        assert_eq!(host.0.enumeration_value(map, word), None, "{map:#x}");
    }
    for (word, want) in [
        ("super", 0x4000_0001),
        ("Normal", 0x4000_0002),
        ("ADMIN", 0x4000_0003),
    ] {
        assert_eq!(
            look_up(&mut vm, &mut host, 0x03, ETHEREALITY, word),
            (want, 1),
            "{word}"
        );
    }
    assert_eq!(look_up(&mut vm, &mut host, 0x03, ETHEREALITY, "ghost").1, 0);
    assert_eq!(look_up(&mut vm, &mut host, 0x03, LOG_TYPE, "admin"), (1, 1));
    for number in [0x02, 0x04] {
        let s = call(&mut vm, &mut host, number, &[LOG_TYPE, 1], 1)[0];
        assert_eq!(text(&vm, s), "Admin");
    }
}
