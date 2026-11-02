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
