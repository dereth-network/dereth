//! Compiled plugins in the browser: their modules run in the browser's own WebAssembly, the
//! runtime the page already has, instead of an interpreter.
//!
//! The plugin host runs in the page's worker, where a module of any size may be compiled at once.
//! Each of a module's imports is a JavaScript function that hands its arguments to the host's
//! import dispatch and throws what it answers as an error, so a failed call traps the module as it
//! would in the interpreter.

use std::cell::RefCell;
use std::rc::Rc;

use dereth_client_sdk::plugin_api::schema::OPS;
use dereth_client_sdk::plugin_wasm::abi::{import_signature, FlatTy};
use dereth_client_sdk::plugin_wasm::module::FUEL_EXPORT;
use dereth_client_sdk::plugin_wasm::{host_import, Backend, Flat, Guest, Instance};
use js_sys::{Array, BigInt, Function, Object, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

/// The browser's WebAssembly as a plugin runtime.
#[derive(Debug, Default, Clone, Copy)]
pub struct BrowserWasm;

fn text(e: &JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

/// What an import needs of the running module: its memory and its allocator, known once the
/// module is instantiated.
#[derive(Clone)]
struct Handles {
    memory: WebAssembly::Memory,
    alloc: Function,
}

impl Handles {
    fn bytes(&self) -> Uint8Array {
        Uint8Array::new(&self.memory.buffer())
    }
}

impl Guest for Handles {
    fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String> {
        let mem = self.bytes();
        let end = at.checked_add(len).filter(|e| *e <= mem.length());
        end.map(|e| mem.subarray(at, e).to_vec())
            .ok_or_else(|| "a read outside the module's memory".into())
    }

    fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String> {
        let mem = self.bytes();
        let len = u32::try_from(bytes.len()).map_err(|_| "too long")?;
        let end = at
            .checked_add(len)
            .filter(|e| *e <= mem.length())
            .ok_or("a write outside the module's memory")?;
        mem.subarray(at, end).copy_from(bytes);
        Ok(())
    }

    fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String> {
        let args = Array::of4(
            &JsValue::from(0),
            &JsValue::from(0),
            &JsValue::from(align),
            &JsValue::from(size),
        );
        let p = self
            .alloc
            .apply(&JsValue::NULL, &args)
            .map_err(|e| text(&e))?;
        p.as_f64()
            // A 32-bit pointer, as the module answered it (an i32, so possibly negative).
            .map(|v| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let p = (v as i64 & 0xffff_ffff) as u32;
                p
            })
            .ok_or_else(|| "the allocator answered no pointer".into())
    }
}

fn to_js(v: Flat) -> JsValue {
    match v {
        Flat::I32(x) => JsValue::from(x),
        Flat::I64(x) => BigInt::from(x).into(),
        Flat::F32(x) => JsValue::from(x),
        Flat::F64(x) => JsValue::from(x),
    }
}

fn from_js(v: &JsValue, ty: FlatTy) -> Result<Flat, String> {
    Ok(match ty {
        FlatTy::I64 => Flat::I64(i64::try_from(v.clone()).map_err(|_| "expected a BigInt")?),
        // Integers below 64 bits come as numbers.
        #[allow(clippy::cast_possible_truncation)]
        FlatTy::I32 => Flat::I32(v.as_f64().ok_or("expected a number")? as i64 as i32), // LINT-OK: an i32 the module passed, which JavaScript holds as a whole number
        #[allow(clippy::cast_possible_truncation)]
        FlatTy::F32 => Flat::F32(v.as_f64().ok_or("expected a number")? as f32),
        FlatTy::F64 => Flat::F64(v.as_f64().ok_or("expected a number")?),
    })
}

/// Wrap a closure taking the call's arguments as an array into a function that throws what the
/// closure answers when that is an error.
fn thrower(f: &Closure<dyn FnMut(Array) -> JsValue>) -> Result<Function, String> {
    let make = Function::new_with_args(
        "f",
        "return function(...a) { const r = f(a); if (r instanceof Error) { throw r; } return r; };",
    );
    make.call1(&JsValue::NULL, f.as_ref())
        .map_err(|e| text(&e))?
        .dyn_into::<Function>()
        .map_err(|_| "not a function".into())
}

impl Backend for BrowserWasm {
    fn name(&self) -> &'static str {
        "browser"
    }

    fn instantiate(&self, module: &[u8], imports: &[usize]) -> Result<Box<dyn Instance>, String> {
        let bytes = Uint8Array::from(module);
        let m = WebAssembly::Module::new(&bytes.into()).map_err(|e| text(&e))?;
        let cell: Rc<RefCell<Option<Handles>>> = Rc::new(RefCell::new(None));
        let object = Object::new();
        let mut closures = Vec::new();
        for (i, desc) in WebAssembly::Module::imports(&m).iter().enumerate() {
            let op = *imports.get(i).ok_or("an import the host did not check")?;
            let module_name = Reflect::get(&desc, &"module".into()).map_err(|e| text(&e))?;
            let name = Reflect::get(&desc, &"name".into()).map_err(|e| text(&e))?;
            let params = import_signature(&OPS[op]).0;
            let cell = cell.clone();
            let f = Closure::<dyn FnMut(Array) -> JsValue>::new(move |args: Array| -> JsValue {
                let run = || -> Result<(), String> {
                    let flats = params
                        .iter()
                        .enumerate()
                        .map(|(k, ty)| from_js(&args.get(u32::try_from(k).unwrap_or(0)), *ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut guest = cell
                        .borrow()
                        .clone()
                        .ok_or("an import before the module was ready")?;
                    host_import(op, &mut guest, &flats).map(|_| ())
                };
                match run() {
                    Ok(()) => JsValue::UNDEFINED,
                    Err(e) => js_sys::Error::new(&e).into(),
                }
            });
            let group = match Reflect::get(&object, &module_name) {
                Ok(g) if g.is_object() => g,
                _ => {
                    let g: JsValue = Object::new().into();
                    Reflect::set(&object, &module_name, &g).map_err(|e| text(&e))?;
                    g
                }
            };
            Reflect::set(&group, &name, &JsValue::from(thrower(&f)?)).map_err(|e| text(&e))?;
            closures.push(f);
        }
        let instance = WebAssembly::Instance::new(&m, &object).map_err(|e| text(&e))?;
        let exports = instance.exports();
        let get = |n: &str| Reflect::get(&exports, &n.into()).map_err(|e| text(&e));
        let memory = get("memory")?
            .dyn_into::<WebAssembly::Memory>()
            .map_err(|_| "no memory")?;
        let alloc = get("cabi_realloc")?
            .dyn_into::<Function>()
            .map_err(|_| "no allocator")?;
        let fuel = get(FUEL_EXPORT)?
            .dyn_into::<WebAssembly::Global>()
            .map_err(|_| "no fuel")?;
        let handles = Handles { memory, alloc };
        *cell.borrow_mut() = Some(handles.clone());
        Ok(Box::new(Running {
            exports: exports.into(),
            handles,
            fuel,
            _closures: closures,
        }))
    }
}

/// A module running in the browser.
struct Running {
    exports: JsValue,
    handles: Handles,
    fuel: WebAssembly::Global,
    /// The import functions, kept alive as long as the module is.
    _closures: Vec<Closure<dyn FnMut(Array) -> JsValue>>,
}

impl Guest for Running {
    fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String> {
        self.handles.read(at, len)
    }

    fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String> {
        self.handles.write(at, bytes)
    }

    fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String> {
        self.handles.alloc(align, size)
    }
}

impl Instance for Running {
    fn call(&mut self, name: &str, args: &[Flat]) -> Result<Vec<Flat>, String> {
        let f = Reflect::get(&self.exports, &name.into())
            .map_err(|e| text(&e))?
            .dyn_into::<Function>()
            .map_err(|_| format!("no export {name}"))?;
        let list = Array::new();
        for a in args {
            list.push(&to_js(*a));
        }
        let r = f.apply(&JsValue::NULL, &list).map_err(|e| text(&e))?;
        if r.is_undefined() {
            return Ok(Vec::new());
        }
        if r.is_bigint() {
            return Ok(vec![from_js(&r, FlatTy::I64)?]);
        }
        Ok(vec![from_js(&r, FlatTy::I32)?])
    }

    fn has_export(&self, name: &str) -> bool {
        Reflect::get(&self.exports, &name.into()).is_ok_and(|v| !v.is_undefined())
    }

    fn set_fuel(&mut self, fuel: i64) {
        self.fuel.set_value(&BigInt::from(fuel).into());
    }

    fn fuel(&self) -> i64 {
        i64::try_from(self.fuel.value()).unwrap_or(0)
    }

    fn memory_bytes(&self) -> usize {
        self.handles
            .memory
            .buffer()
            .unchecked_into::<js_sys::ArrayBuffer>()
            .byte_length() as usize
    }
}

/// The page's plugins in this build, checked: each package in `bundle` (the page's plugin bundle,
/// zips included) by origin, id and entry; and each compiled one loaded in the browser's
/// WebAssembly against a host that answers plainly and notes each call, then handed an experience
/// gain and its own command. One line per step.
#[must_use]
pub fn check(bundle: &[u8]) -> Vec<String> {
    use dereth_client_sdk::plugin_api::engine::{CallError, HostCalls, Limits, ScriptEngine};
    use dereth_client_sdk::plugin_api::event::Event;
    use dereth_client_sdk::plugin_api::Value;
    use dereth_client_sdk::plugin_wasm::WasmEngine;

    struct Noting(Vec<String>);
    impl HostCalls for Noting {
        fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
            let args: Vec<String> = args.iter().map(Value::to_json).collect();
            self.0.push(format!("call {op} {}", args.join(" ")));
            Ok(match op {
                "task.timer" => Value::Int(1),
                "host.has" => Value::Bool(true),
                "time.clock" => Value::Num(60.0),
                _ => Value::Nil,
            })
        }
        fn console(&mut self, line: &str) {
            self.0.push(format!("console {line}"));
        }
        fn module_source(&mut self, _: &str) -> Option<String> {
            None
        }
    }

    let mut lines = Vec::new();
    for p in crate::plugins::packages(bundle) {
        let Ok(m) = p.manifest() else {
            lines.push(format!("package {} does not read", p.origin));
            continue;
        };
        lines.push(format!("package {} {} {}", p.origin, m.id, m.entry));
        if !m.entry.ends_with(".wasm") {
            continue;
        }
        let engine = WasmEngine::new(Rc::new(BrowserWasm));
        let mut host = Noting(Vec::new());
        let outcome = engine.instantiate(Limits::default()).and_then(|mut vm| {
            let bytes = p.files.get(&m.entry).cloned().unwrap_or_default();
            vm.load_bytes(&mut host, &m.entry, &bytes)?;
            vm.deliver(
                &mut host,
                &Event::new("character.xp", Value::map().with("gained", 250)),
            )?;
            vm.deliver(
                &mut host,
                &Event::new("command", Value::map().with("name", "xp").with("args", "")),
            )?;
            Ok(vm.memory_used())
        });
        lines.append(&mut host.0);
        match outcome {
            Ok(bytes) => lines.push(format!("ran {} in {bytes} bytes ok", m.id)),
            Err(e) => lines.push(format!("{} failed: {e}", m.id)),
        }
    }
    lines
}
