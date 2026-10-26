//! The runtime on the desktop, headless and wherever no WebAssembly engine of the platform's is
//! used: `wasmi`, an interpreter in Rust (so it also runs where compiling code at run time is not
//! allowed).

use crate::abi::{Flat, Guest};
use crate::{host_import, module::FUEL_EXPORT, Backend, Instance};

/// `wasmi` as a [`Backend`].
#[derive(Debug, Default)]
pub struct Interpreter {
    engine: wasmi::Engine,
}

fn to_flat(v: &wasmi::Val) -> Flat {
    match v {
        wasmi::Val::I32(x) => Flat::I32(*x),
        wasmi::Val::I64(x) => Flat::I64(*x),
        wasmi::Val::F32(x) => Flat::F32(f32::from(*x)),
        wasmi::Val::F64(x) => Flat::F64(f64::from(*x)),
        // The interface has no other value types; a module using them is refused before it runs.
        _ => Flat::I32(0),
    }
}

fn to_val(v: Flat) -> wasmi::Val {
    match v {
        Flat::I32(x) => wasmi::Val::I32(x),
        Flat::I64(x) => wasmi::Val::I64(x),
        Flat::F32(x) => wasmi::Val::F32(x.into()),
        Flat::F64(x) => wasmi::Val::F64(x.into()),
    }
}

fn read(mem: &[u8], at: u32, len: u32) -> Result<Vec<u8>, String> {
    let (at, len) = (at as usize, len as usize);
    mem.get(at..at.saturating_add(len))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "a read outside the module's memory".to_owned())
}

fn write(mem: &mut [u8], at: u32, bytes: &[u8]) -> Result<(), String> {
    let at = at as usize;
    mem.get_mut(at..at.saturating_add(bytes.len()))
        .ok_or_else(|| "a write outside the module's memory".to_owned())?
        .copy_from_slice(bytes);
    Ok(())
}

fn realloc(
    ctx: impl wasmi::AsContextMut,
    f: wasmi::Func,
    align: u32,
    size: u32,
) -> Result<u32, String> {
    let mut out = [wasmi::Val::I32(0)];
    let args = [0, 0, align, size].map(|v| wasmi::Val::I32(v.cast_signed()));
    f.call(ctx, &args, &mut out).map_err(|e| e.to_string())?;
    match out[0] {
        wasmi::Val::I32(p) => Ok(p.cast_unsigned()),
        _ => Err("the allocator answered no pointer".into()),
    }
}

/// The module as an import sees it: through the calling context.
struct Calling<'a, 'b>(&'a mut wasmi::Caller<'b, ()>);

impl Calling<'_, '_> {
    fn memory(&self) -> Result<wasmi::Memory, String> {
        self.0
            .get_export("memory")
            .and_then(wasmi::Extern::into_memory)
            .ok_or_else(|| "no memory".into())
    }
}

impl Guest for Calling<'_, '_> {
    fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String> {
        let m = self.memory()?;
        read(m.data(&*self.0), at, len)
    }

    fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String> {
        let m = self.memory()?;
        write(m.data_mut(&mut *self.0), at, bytes)
    }

    fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String> {
        let f = self
            .0
            .get_export("cabi_realloc")
            .and_then(wasmi::Extern::into_func)
            .ok_or("no allocator")?;
        realloc(&mut *self.0, f, align, size)
    }

    fn spend(&mut self, fuel: i64) -> Result<(), String> {
        let g = self
            .0
            .get_export(FUEL_EXPORT)
            .and_then(wasmi::Extern::into_global)
            .ok_or("no fuel")?;
        let left = match g.get(&*self.0) {
            wasmi::Val::I64(v) => v,
            _ => 0,
        }
        .saturating_sub(fuel);
        g.set(&mut *self.0, wasmi::Val::I64(left))
            .map_err(|e| e.to_string())?;
        if left < 0 {
            return Err("the plugin ran past its time budget".into());
        }
        Ok(())
    }
}

impl Backend for Interpreter {
    fn name(&self) -> &'static str {
        "wasmi"
    }

    fn instantiate(&self, module: &[u8], imports: &[usize]) -> Result<Box<dyn Instance>, String> {
        let m = wasmi::Module::new(&self.engine, module).map_err(|e| e.to_string())?;
        let mut store = wasmi::Store::new(&self.engine, ());
        let mut linker = wasmi::Linker::<()>::new(&self.engine);
        for (i, imp) in m.imports().enumerate() {
            let op = *imports.get(i).ok_or("an import the host did not check")?;
            let ty = imp
                .ty()
                .func()
                .cloned()
                .ok_or("an import that is not a function")?;
            linker
                .func_new(
                    imp.module(),
                    imp.name(),
                    ty,
                    move |mut caller, params, _results| {
                        let args: Vec<Flat> = params.iter().map(to_flat).collect();
                        host_import(op, &mut Calling(&mut caller), &args)
                            .map_err(wasmi::Error::new)?;
                        Ok(())
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        let instance = linker
            .instantiate_and_start(&mut store, &m)
            .map_err(|e| e.to_string())?;
        let memory = instance.get_memory(&store, "memory").ok_or("no memory")?;
        let alloc = instance
            .get_func(&store, "cabi_realloc")
            .ok_or("no allocator")?;
        let fuel = instance.get_global(&store, FUEL_EXPORT).ok_or("no fuel")?;
        Ok(Box::new(Running {
            store,
            instance,
            memory,
            alloc,
            fuel,
        }))
    }
}

/// A module running in the interpreter.
struct Running {
    store: wasmi::Store<()>,
    instance: wasmi::Instance,
    memory: wasmi::Memory,
    alloc: wasmi::Func,
    fuel: wasmi::Global,
}

impl Guest for Running {
    fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String> {
        read(self.memory.data(&self.store), at, len)
    }

    fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String> {
        write(self.memory.data_mut(&mut self.store), at, bytes)
    }

    fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String> {
        realloc(&mut self.store, self.alloc, align, size)
    }
}

impl Instance for Running {
    fn call(&mut self, name: &str, args: &[Flat]) -> Result<Vec<Flat>, String> {
        let f = self
            .instance
            .get_func(&self.store, name)
            .ok_or_else(|| format!("no export {name}"))?;
        let n = f.ty(&self.store).results().len();
        let mut out = vec![wasmi::Val::I32(0); n];
        let args: Vec<wasmi::Val> = args.iter().copied().map(to_val).collect();
        f.call(&mut self.store, &args, &mut out)
            .map_err(|e| e.to_string())?;
        Ok(out.iter().map(to_flat).collect())
    }

    fn has_export(&self, name: &str) -> bool {
        self.instance.get_export(&self.store, name).is_some()
    }

    fn set_fuel(&mut self, fuel: i64) {
        let _ = self.fuel.set(&mut self.store, wasmi::Val::I64(fuel));
    }

    fn fuel(&self) -> i64 {
        match self.fuel.get(&self.store) {
            wasmi::Val::I64(v) => v,
            _ => 0,
        }
    }

    fn memory_bytes(&self) -> usize {
        self.memory.data_size(&self.store)
    }
}
