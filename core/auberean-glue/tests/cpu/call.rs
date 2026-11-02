//! A native called as the machine calls one, its frame built by hand: the result slots, the
//! arguments last first, the receiver.
//! Behaviour: none (test support)

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

/// A machine with every native of the glue, and a host with nothing in it.
pub struct Bench {
    pub vm: Vm,
    pub host: TestHost,
    prog: Arc<Program>,
}

impl Bench {
    pub fn new() -> Self {
        let lib = library(&[w(op::END)], &[], &[]);
        let table = auberean_glue::natives(&lib);
        let prog = Arc::new(Program::link(lib, table).expect("links"));
        Self {
            vm: Vm::new(Arc::clone(&prog)),
            host: TestHost::default(),
            prog,
        }
    }

    /// A machine over a library of the given native packages (name, id, function names numbered
    /// from 1), with every native of the glue.
    pub fn with_packages(packages: &[(&'static str, u32, &[&str])]) -> Self {
        let pkgs: Vec<crate::asm::Pkg> = packages
            .iter()
            .map(|(name, id, functions)| crate::asm::Pkg {
                name,
                id: *id,
                size: 0,
                parent: None,
                functions: vec![0; functions.len() + 1],
                native: true,
            })
            .collect();
        let mut lib = library(&[w(op::END)], &pkgs, &[]);
        for (p, (_, _, names)) in lib.packages.iter_mut().zip(packages) {
            for (k, n) in names.iter().enumerate() {
                p.functions[k + 1].name = (*n).to_owned();
            }
        }
        let table = auberean_glue::natives(&lib);
        let prog = Arc::new(Program::link(lib, table).expect("links"));
        Self {
            vm: Vm::new(Arc::clone(&prog)),
            host: TestHost::default(),
            prog,
        }
    }

    /// A new native object of a type, as the native factory makes one.
    pub fn new_object(&mut self, ty: u32) -> u32 {
        let o = NativeObject::new(&self.prog.library, ty);
        self.vm.heap.add_native(ty, o)
    }

    /// A new native object.
    pub fn object(&mut self, ty: u32, o: NativeObject) -> u32 {
        self.vm.heap.add_native(ty, o)
    }

    /// A new string.
    pub fn text(&mut self, s: &str) -> u32 {
        self.object(
            native_type::WP_STRING,
            NativeObject::Text(s.encode_utf16().collect()),
        )
    }

    /// The units of a string object.
    pub fn text_of(&self, id: u32) -> Vec<u16> {
        match self.vm.heap.native(id).expect("an object") {
            NativeObject::Text(t) => t.clone(),
            o => panic!("{id:#x} is not a string: {o:?}"),
        }
    }

    /// Call a native: `results` result slots, the arguments (`args[0]` at `fp[-2]`), the
    /// receiver. Returns the status and the result slots, nearest first.
    pub fn call(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        args: &[u32],
        results: usize,
    ) -> (Status, Vec<(u32, u8)>) {
        let mut cx = Context::new(usize::MAX, 0);
        for _ in 0..results {
            cx.push(0, tag::INT).unwrap();
        }
        for v in args.iter().rev() {
            cx.push(*v, tag::MIXED).unwrap();
        }
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
        let slots = (0..results).rev().map(|k| cx.slot(k).unwrap()).collect();
        (status, slots)
    }

    /// Call a native whose one argument is a reference to a local slot (slot 0 of the stack)
    /// holding `held`: `results` result slots above the local. Returns the status and the result
    /// slots, nearest first.
    pub fn call_with_out(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        held: u32,
        results: usize,
    ) -> (Status, Vec<(u32, u8)>) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(held, tag::REFERENCE).unwrap();
        for _ in 0..results {
            cx.push(0, tag::INT).unwrap();
        }
        let local = self.vm.heap.add_relative(NULL, 0);
        cx.push(local, tag::REFERENCE).unwrap();
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
        let slots = (1..=results).rev().map(|k| cx.slot(k).unwrap()).collect();
        (status, slots)
    }

    /// Call a native whose arguments include references to a local slot (slot 0 of the stack)
    /// holding `held`: `args[0]` at `fp[-2]`, `None` the reference to the local. `results` result
    /// slots above the local. Returns the status and the result slots, nearest first.
    pub fn call_with_local(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        held: u32,
        args: &[Option<u32>],
        results: usize,
    ) -> (Status, Vec<(u32, u8)>) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(held, tag::REFERENCE).unwrap();
        for _ in 0..results {
            cx.push(0, tag::INT).unwrap();
        }
        for a in args.iter().rev() {
            match a {
                Some(v) => cx.push(*v, tag::MIXED).unwrap(),
                None => {
                    let local = self.vm.heap.add_relative(NULL, 0);
                    cx.push(local, tag::REFERENCE).unwrap();
                }
            }
        }
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
        let slots = (1..=results).rev().map(|k| cx.slot(k).unwrap()).collect();
        (status, slots)
    }

    /// The first result word of a native that succeeds.
    pub fn answer(&mut self, package: u32, number: u32, receiver: u32, args: &[u32]) -> u32 {
        let (status, slots) = self.call(package, number, receiver, args, 1);
        assert_eq!(status, Status::Success, "native {package:#x}:{number:#x}");
        slots[0].0
    }
}
