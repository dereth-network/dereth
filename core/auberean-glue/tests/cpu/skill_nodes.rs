//! Skill panel nodes as scripts fill them, each native called as the machine calls one: its frame
//! built by hand (the result slot, the arguments last first, the receiver), then the native by id.

use std::sync::Arc;

use auberean_glue::skill_nodes::SkillNode;
use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, Item, NativeObject, TextInfo};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

const NODE: u32 = 0xAE;

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

    /// A new node, as the native factory makes one.
    fn node(&mut self) -> u32 {
        let new = NativeObject::new(&self.prog.library, native_type::SKILL_UI_NODE);
        self.vm.heap.add_native(native_type::SKILL_UI_NODE, new)
    }

    /// Call a node native: two result slots (the nearer one answered), the arguments (`args[0]` at `fp[-2]`), the
    /// receiver. Returns the status and the slot below the arguments.
    fn call(&mut self, number: u32, receiver: u32, args: &[(u32, u8)]) -> (Status, u32) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(0, tag::INT).unwrap();
        cx.push(0, tag::INT).unwrap();
        for (v, t) in args.iter().rev() {
            cx.push(*v, *t).unwrap();
        }
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(NODE, number).0;
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
        (status, cx.slot(1).unwrap().0)
    }

    fn read(&self, r: u32) -> SkillNode {
        SkillNode::of(self.vm.heap.native(r).unwrap()).expect("a node")
    }
}

/// A new node is empty with its place at -1; each part reads back as set; a name reads back as a
/// new copy; the place reads into two out slots; a parent or dependency listed already is not
/// added again; the lists read back as new lists of words; a node is set from another whole and
/// cleared; it packs and reads back the same.
///
/// Behaviour: script.natives.a-skill-panel-node-keeps-what-it-is-given
#[test]
fn a_skill_panel_node_keeps_what_it_is_given() {
    let mut b = Bench::new();
    let n = b.node();
    assert_eq!(b.read(n), SkillNode::default());
    assert_eq!(b.read(n).x, u32::MAX);
    let int = tag::INT;
    assert_eq!(b.call(0x02, n, &[(0x4000_0010, int)]).0, Status::Success);
    b.call(0x04, n, &[(2, int)]);
    b.call(0x08, n, &[(0x0600_1234, int)]);
    b.call(0x0C, n, &[(40, int), (80, int)]);
    b.call(0x13, n, &[(1, int)]);
    b.call(0x19, n, &[(1, int)]);
    let name = b.vm.heap.add_native(
        native_type::STRING_INFO,
        NativeObject::TextInfo(TextInfo {
            string_id: 9,
            table: 0x2300_0001,
            ..TextInfo::default()
        }),
    );
    b.call(0x06, n, &[(name, tag::REFERENCE)]);
    for p in [3, 4, 3] {
        b.call(0x0F, n, &[(p, int)]);
    }
    assert_eq!(b.call(0x11, n, &[(7, int)]).1, 1, "added");
    assert_eq!(b.call(0x11, n, &[(7, int)]).1, 0, "listed already");
    assert_eq!(b.call(0x01, n, &[]).1, 0x4000_0010);
    assert_eq!(b.call(0x03, n, &[]).1, 2);
    assert_eq!(b.call(0x07, n, &[]).1, 0x0600_1234);
    assert_eq!((b.call(0x0D, n, &[]).1, b.call(0x0E, n, &[]).1), (40, 80));
    assert_eq!(b.call(0x14, n, &[]).1, 1);
    assert_eq!(b.call(0x16, n, &[]).1, 0);
    assert_eq!(b.call(0x1A, n, &[]).1, 1);
    let got = b.call(0x05, n, &[]).1;
    assert_ne!(got, name, "a new string reference");
    assert!(
        matches!(b.vm.heap.native(got).unwrap(), NativeObject::TextInfo(t) if t.string_id == 9)
    );
    let parents = b.call(0x10, n, &[]).1;
    let NativeObject::Sequence { items, .. } = b.vm.heap.native(parents).unwrap() else {
        panic!("a list")
    };
    assert_eq!(items, &vec![Item::Word(3), Item::Word(4)]);
    let node = b.read(n);
    assert_eq!(node.dependencies, vec![7]);
    assert_eq!(
        SkillNode::read(&mut auberean_script::wire::In::new(&node.pack())).unwrap(),
        node,
        "packs back"
    );
    // Set whole from it, then cleared.
    let other = b.node();
    b.call(0x18, other, &[(n, tag::REFERENCE)]);
    assert_eq!(b.read(other), node);
    b.call(0x17, n, &[]);
    assert_eq!(b.read(n), SkillNode::default());
    assert_eq!(b.call(0x01, NULL, &[]).0, Status::Crash);
}
