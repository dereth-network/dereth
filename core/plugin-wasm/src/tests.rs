//! The engine against a module built here instruction by instruction: a guest that prints, starts
//! a timer, echoes an event's data, loops for ever or grows its memory without end. The engine
//! reads a test clock, never the machine's: an outcome does not depend on how fast or loaded the
//! machine is.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use dereth_plugin_api::engine::{
    CallError, HostCalls, Limits, ScriptEngine, ScriptErrorKind, VmClock,
};
use dereth_plugin_api::event::Event;
use dereth_plugin_api::Value;
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction as I, MemArg,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};

use crate::interp::Interpreter;
use crate::WasmEngine;

/// A clock that moves on only as far as the host says it took.
#[derive(Debug, Default)]
struct TestClock(Cell<Duration>);

impl VmClock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

#[derive(Default)]
struct Recorder {
    calls: Vec<(String, Vec<Value>)>,
    console: Vec<String>,
    /// The engine's clock.
    clock: Rc<TestClock>,
}

impl HostCalls for Recorder {
    fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        self.calls.push((op.to_owned(), args.to_vec()));
        match op {
            "task.timer" => Ok(Value::Int(9)),
            "chat.print" if args.first().and_then(Value::as_str) == Some("deny") => {
                Err(CallError::Failed("chat.print is not available here".into()))
            }
            // A host that takes a millisecond over each of these.
            "chat.print" if args.first().and_then(Value::as_str) == Some("slow") => {
                let clock = &self.clock.0;
                clock.set(clock.get() + Duration::from_millis(1));
                Ok(Value::Nil)
            }
            _ => Ok(Value::Nil),
        }
    }

    fn console(&mut self, line: &str) {
        self.console.push(line.to_owned());
    }

    fn module_source(&mut self, _: &str) -> Option<String> {
        None
    }
}

const HOST: &str = "dereth:plugin/host@0.4.0";

fn mem(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

/// The guest. `import` names its print import (a wrong one tests refusal).
fn guest(print_import: &str) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32; 6], []); // 0 chat-print
    types
        .ty()
        .function([ValType::F64, ValType::I32, ValType::I32], []); // 1 task-timer
    types.ty().function([ValType::I32; 4], [ValType::I32]); // 2 cabi_realloc, 4 deliver
    types.ty().function([], [ValType::I32]); // 3 init
    let mut imports = ImportSection::new();
    imports.import(HOST, print_import, EntityType::Function(0));
    imports.import(HOST, "task-timer", EntityType::Function(1));
    let mut funcs = FunctionSection::new();
    funcs.function(2); // 2: cabi_realloc
    funcs.function(3); // 3: init
    funcs.function(2); // 4: deliver
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(4096),
    );
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("cabi_realloc", ExportKind::Func, 2);
    exports.export("init", ExportKind::Func, 3);
    exports.export("deliver", ExportKind::Func, 4);

    let mut code = CodeSection::new();
    // cabi_realloc: a bump allocator, eight-byte aligned.
    let mut f = Function::new([(1, ValType::I32)]);
    for i in [
        I::GlobalGet(0),
        I::I32Const(7),
        I::I32Add,
        I::I32Const(-8),
        I::I32And,
        I::LocalTee(4),
        I::LocalGet(3),
        I::I32Add,
        I::GlobalSet(0),
        I::LocalGet(4),
        I::End,
    ] {
        f.instruction(&i);
    }
    code.function(&f);
    // init: print the greeting (no style), start a repeating 250 ms timer, answer ok.
    let mut f = Function::new([]);
    for i in [
        I::I32Const(16),
        I::I32Const(15),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(1024),
        I::Call(0),
        I::F64Const(250.0.into()),
        I::I32Const(1),
        I::I32Const(1040),
        I::Call(1),
        I::I32Const(1100),
        I::End,
    ] {
        f.instruction(&i);
    }
    code.function(&f);
    // deliver: by the event name's first letter, loop for ever (`l`), grow memory until it
    // cannot and then fail as an allocator would (`g`), print "deny" and answer the error it
    // got (`d`), fill its first page a thousand times (`f`), print "slow" ten times (`s`), or
    // answer the data back (anything else).
    let mut f = Function::new([(1, ValType::I32)]);
    for i in [
        I::LocalGet(0),
        I::I32Load8U(mem(0)),
        I::I32Const(i32::from(b'f')),
        I::I32Eq,
        I::If(wasm_encoder::BlockType::Empty),
        I::I32Const(1000),
        I::LocalSet(4),
        I::Loop(wasm_encoder::BlockType::Empty),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(65_536),
        I::MemoryFill(0),
        I::LocalGet(4),
        I::I32Const(1),
        I::I32Sub,
        I::LocalTee(4),
        I::BrIf(0),
        I::End,
        I::End,
        I::LocalGet(0),
        I::I32Load8U(mem(0)),
        I::I32Const(i32::from(b's')),
        I::I32Eq,
        I::If(wasm_encoder::BlockType::Empty),
        I::I32Const(10),
        I::LocalSet(4),
        I::Loop(wasm_encoder::BlockType::Empty),
        I::I32Const(64),
        I::I32Const(4),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(1024),
        I::Call(0),
        I::LocalGet(4),
        I::I32Const(1),
        I::I32Sub,
        I::LocalTee(4),
        I::BrIf(0),
        I::End,
        I::End,
        I::LocalGet(0),
        I::I32Load8U(mem(0)),
        I::I32Const(i32::from(b'l')),
        I::I32Eq,
        I::If(wasm_encoder::BlockType::Empty),
        I::Loop(wasm_encoder::BlockType::Empty),
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(0),
        I::I32Load8U(mem(0)),
        I::I32Const(i32::from(b'g')),
        I::I32Eq,
        I::If(wasm_encoder::BlockType::Empty),
        I::Loop(wasm_encoder::BlockType::Empty),
        I::I32Const(1),
        I::MemoryGrow(0),
        I::I32Const(-1),
        I::I32Ne,
        I::BrIf(0),
        I::End,
        I::Unreachable,
        I::End,
        I::LocalGet(0),
        I::I32Load8U(mem(0)),
        I::I32Const(i32::from(b'd')),
        I::I32Eq,
        I::If(wasm_encoder::BlockType::Empty),
        I::I32Const(48),
        I::I32Const(4),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(1024),
        I::Call(0),
        I::I32Const(1024),
        I::Return,
        I::End,
        I::I32Const(1200),
        I::I32Const(0),
        I::I32Store8(mem(0)),
        I::I32Const(1200),
        I::LocalGet(2),
        I::I32Store(mem(4)),
        I::I32Const(1200),
        I::LocalGet(3),
        I::I32Store(mem(8)),
        I::I32Const(1200),
        I::End,
    ] {
        f.instruction(&i);
    }
    code.function(&f);
    let mut data = DataSection::new();
    data.active(
        0,
        &ConstExpr::i32_const(16),
        b"hello from wasm".iter().copied(),
    );
    data.active(0, &ConstExpr::i32_const(48), b"deny".iter().copied());
    data.active(0, &ConstExpr::i32_const(64), b"slow".iter().copied());

    let mut m = Module::new();
    m.section(&types)
        .section(&imports)
        .section(&funcs)
        .section(&memory)
        .section(&globals)
        .section(&exports)
        .section(&code)
        .section(&data);
    m.finish()
}

/// The engine on `host`'s clock.
fn engine(host: &Recorder) -> WasmEngine {
    WasmEngine::new(Rc::new(Interpreter::default())).with_clock(host.clock.clone())
}

const LIMITS: Limits = Limits {
    memory_bytes: 4 * 65_536,
    call_budget_ms: 2.0,
    profile: false,
};

#[test]
fn a_module_runs_its_init_and_reaches_the_host_with_typed_values() {
    let mut host = Recorder::default();
    let e = engine(&host);
    assert!(e.handles("plugin.wasm") && !e.handles("main.lua"));
    let mut vm = e.instantiate(LIMITS).unwrap();
    vm.load_bytes(&mut host, "plugin.wasm", &guest("chat-print"))
        .unwrap();
    assert_eq!(
        host.calls,
        [
            (
                "chat.print".to_owned(),
                vec![Value::Str("hello from wasm".into()), Value::Nil]
            ),
            (
                "task.timer".to_owned(),
                vec![Value::Num(250.0), Value::Bool(true)]
            ),
        ]
    );
    let data = Value::map().with("a", 1).with("b", "two");
    let back = vm
        .deliver(&mut host, &Event::new("echo", data.clone()))
        .unwrap();
    assert_eq!(back, data);
}

#[test]
fn a_host_error_reaches_the_module_as_its_error_and_a_runaway_or_greedy_one_is_stopped() {
    let mut host = Recorder::default();
    let mut vm = engine(&host).instantiate(LIMITS).unwrap();
    vm.load_bytes(&mut host, "plugin.wasm", &guest("chat-print"))
        .unwrap();
    let e = vm
        .deliver(&mut host, &Event::new("deny", Value::Nil))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Runtime);
    assert_eq!(e.message, "chat.print is not available here");
    let e = vm
        .deliver(&mut host, &Event::new("loop", Value::Nil))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Budget, "{e}");
    let e = vm
        .deliver(&mut host, &Event::new("grow", Value::Nil))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Memory, "{e}");
    assert_eq!(vm.memory_used(), 4 * 65_536);
}

#[test]
fn bulk_memory_work_and_the_hosts_time_answering_are_charged_to_the_budget() {
    let mut host = Recorder::default();
    let mut vm = engine(&host).instantiate(LIMITS).unwrap();
    vm.load_bytes(&mut host, "plugin.wasm", &guest("chat-print"))
        .unwrap();
    // Ten calls the host takes a millisecond over each: stopped once the host's time passes the
    // budget (2 ms), at the third at the latest.
    let e = vm
        .deliver(&mut host, &Event::new("slow", Value::Nil))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Budget, "{e}");
    let slow = host
        .calls
        .iter()
        .filter(|(_, a)| a.first().and_then(Value::as_str) == Some("slow"))
        .count();
    assert!((2..=3).contains(&slow), "{slow} slow calls ran");
    // A thousand fills of a page: a few thousand instructions, and far more work than a 2 ms
    // budget holds.
    let e = vm
        .deliver(&mut host, &Event::new("fill", Value::Nil))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Budget, "{e}");
    // The module still answers.
    let data = Value::map().with("a", 1);
    assert_eq!(
        vm.deliver(&mut host, &Event::new("echo", data.clone()))
            .unwrap(),
        data
    );
}

#[test]
fn a_module_importing_what_the_host_does_not_offer_is_refused() {
    let mut host = Recorder::default();
    let mut vm = engine(&host).instantiate(LIMITS).unwrap();
    let e = vm
        .load_bytes(&mut host, "plugin.wasm", &guest("launch-missiles"))
        .unwrap_err();
    assert_eq!(e.kind, ScriptErrorKind::Syntax);
    assert!(e.message.contains("does not offer"), "{}", e.message);
    assert!(host.calls.is_empty());
}
