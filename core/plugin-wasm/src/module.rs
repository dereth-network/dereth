//! A plugin's `.wasm` made ready to run: the core module taken out of its component, its imports
//! checked against the interface, and its code rewritten to count its work and cap its memory.
//!
//! **Work.** Every function entry and every loop iteration takes one unit of fuel from a global
//! the rewrite adds and exports as [`FUEL_EXPORT`]; when it goes below zero the module traps. An
//! instruction that does work in proportion to an operand takes fuel by that operand before it
//! runs: filling, copying or initialising memory a unit for every 64 bytes (and one), growing it
//! 1,024 units a page, and filling, copying, initialising or growing a table a unit an element.
//! The host fills it before each entry. The count is the same on every host, so a plugin stopped
//! for its work is stopped everywhere at the same point.
//!
//! **Memory.** Each memory's maximum is lowered to the limit, so growing past it fails as it would
//! at the top of memory.

use std::borrow::Cow;

use dereth_plugin_api::schema::{self, OPS};
use wasm_encoder::reencode::{self, Reencode, RoundtripReencoder};

use crate::abi::{import_signature, FlatTy};

/// The exported global the rewrite adds: the fuel left.
pub const FUEL_EXPORT: &str = "dereth_fuel";

/// The interface's import module name, without its version.
pub const HOST_MODULE: &str = "dereth:plugin/host";

/// A prepared module: its bytes, and each import's operation index in the schema.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    /// For each import in order: the index of its operation in [`schema::OPS`].
    pub imports: Vec<usize>,
}

/// The bytes of the core module a component holds (the one with the memory and the allocator),
/// or `wasm` itself when it is a core module.
///
/// # Errors
/// Not WebAssembly, or a component with no such module.
pub fn core_module(wasm: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if !wasm.starts_with(b"\0asm") {
        return Err("not a WebAssembly file".into());
    }
    if !wasmparser::Parser::is_component(wasm) {
        return Ok(Cow::Borrowed(wasm));
    }
    let mut modules = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        if let wasmparser::Payload::ModuleSection {
            unchecked_range, ..
        } = payload.map_err(|e| format!("the component does not read: {e}"))?
        {
            modules.push(&wasm[unchecked_range]);
        }
    }
    modules
        .into_iter()
        .find(|m| {
            exports_of(m).is_ok_and(|e| {
                e.iter().any(|n| n == "cabi_realloc") && e.iter().any(|n| n == "memory")
            })
        })
        .map(Cow::Borrowed)
        .ok_or_else(|| "the component holds no module with a memory and an allocator".into())
}

fn exports_of(module: &[u8]) -> Result<Vec<String>, wasmparser::BinaryReaderError> {
    let mut out = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(module) {
        if let wasmparser::Payload::ExportSection(s) = payload? {
            for e in s {
                out.push(e?.name.to_owned());
            }
        }
    }
    Ok(out)
}

fn flat_of(t: wasmparser::ValType) -> Option<FlatTy> {
    Some(match t {
        wasmparser::ValType::I32 => FlatTy::I32,
        wasmparser::ValType::I64 => FlatTy::I64,
        wasmparser::ValType::F32 => FlatTy::F32,
        wasmparser::ValType::F64 => FlatTy::F64,
        _ => return None,
    })
}

/// Whether an import module name is the interface at a version this host serves: the same major
/// and a minor no newer than the schema's.
fn serves(module: &str) -> bool {
    let Some(version) = module
        .strip_prefix(HOST_MODULE)
        .and_then(|v| v.strip_prefix('@'))
    else {
        return false;
    };
    let parse = |v: &str| -> Option<(u32, u32)> {
        let mut it = v.split('.');
        Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
    };
    match (parse(version), parse(schema::WIT_VERSION)) {
        (Some((maj, min)), Some((hmaj, hmin))) => maj == hmaj && min <= hmin,
        _ => false,
    }
}

/// Check a core module against the interface and rewrite it (see the module documentation);
/// `max_pages` is the memory limit in 64 KiB pages.
///
/// # Errors
/// It does not validate, imports anything but the interface's functions with their signatures,
/// lacks an export the interface needs, or asks for more memory than the limit at its start.
pub fn prepare(module: &[u8], max_pages: u64) -> Result<Prepared, String> {
    wasmparser::Validator::new()
        .validate_all(module)
        .map_err(|e| format!("not a valid module: {e}"))?;
    let mut types: Vec<(Vec<FlatTy>, Vec<FlatTy>)> = Vec::new();
    let mut param_counts: Vec<u32> = Vec::new();
    let mut function_types: Vec<u32> = Vec::new();
    let mut imports = Vec::new();
    let mut defined_globals = 0u32;
    let mut exports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(module) {
        match payload.map_err(|e| e.to_string())? {
            wasmparser::Payload::TypeSection(s) => {
                for t in s.into_iter_err_on_gc_types() {
                    let t = t.map_err(|e| e.to_string())?;
                    param_counts.push(u32::try_from(t.params().len()).unwrap_or(u32::MAX));
                    let conv = |v: &[wasmparser::ValType]| {
                        v.iter().map(|t| flat_of(*t)).collect::<Option<Vec<_>>>()
                    };
                    types.push((
                        conv(t.params()).unwrap_or_default(),
                        conv(t.results()).unwrap_or_default(),
                    ));
                }
            }
            wasmparser::Payload::ImportSection(s) => {
                for i in s.into_imports() {
                    let i = i.map_err(|e| e.to_string())?;
                    match i.ty {
                        wasmparser::TypeRef::Func(t) | wasmparser::TypeRef::FuncExact(t) => {
                            let op = serves(i.module)
                                .then(|| OPS.iter().position(|o| o.wit_name() == i.name))
                                .flatten()
                                .ok_or_else(|| {
                                    format!(
                                        "it imports {}/{}, which the host does not offer",
                                        i.module, i.name
                                    )
                                })?;
                            let want = import_signature(&OPS[op]);
                            if types.get(t as usize) != Some(&want) {
                                return Err(format!(
                                    "it imports {} with the wrong signature",
                                    i.name
                                ));
                            }
                            imports.push(op);
                        }
                        _ => {
                            return Err(format!(
                                "it imports {}/{}, which the host does not offer",
                                i.module, i.name
                            ))
                        }
                    }
                }
            }
            wasmparser::Payload::GlobalSection(s) => defined_globals += s.count(),
            wasmparser::Payload::FunctionSection(s) => {
                for t in s {
                    function_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            wasmparser::Payload::MemorySection(s) => {
                for m in s {
                    let m = m.map_err(|e| e.to_string())?;
                    if m.memory64 {
                        return Err("it has a 64-bit memory, which the host does not run".into());
                    }
                    if m.initial > max_pages {
                        return Err("it needs more memory at its start than the limit".into());
                    }
                }
            }
            wasmparser::Payload::TableSection(s) => {
                for t in s {
                    if t.map_err(|e| e.to_string())?.ty.table64 {
                        return Err("it has a 64-bit table, which the host does not run".into());
                    }
                }
            }
            wasmparser::Payload::ExportSection(s) => {
                for e in s {
                    exports.push(e.map_err(|e| e.to_string())?.name.to_owned());
                }
            }
            _ => {}
        }
    }
    for need in ["memory", "cabi_realloc", "init", "deliver"] {
        if !exports.iter().any(|e| e == need) {
            return Err(format!("it exports no `{need}`"));
        }
    }
    let mut rewrite = Rewrite {
        fuel: defined_globals,
        max_pages,
        global_added: false,
        params: function_types
            .iter()
            .map(|t| param_counts.get(*t as usize).copied().unwrap_or(0))
            .collect(),
        body: 0,
    };
    let mut out = wasm_encoder::Module::new();
    rewrite
        .parse_core_module(&mut out, wasmparser::Parser::new(0), module)
        .map_err(|e| format!("the module could not be rewritten: {e}"))?;
    Ok(Prepared {
        bytes: out.finish(),
        imports,
    })
}

/// The rewrite: a fuel global, its checks, its export, and the memory cap.
struct Rewrite {
    /// The fuel global's index.
    fuel: u32,
    max_pages: u64,
    global_added: bool,
    /// Each defined function's number of parameters, in the order of the code section.
    params: Vec<u32>,
    /// The function body being rewritten, from 0.
    body: usize,
}

/// How an instruction's last operand (an `i32` count) is charged: shifted right by `shift` (a
/// unit for every `1 << shift`), or left by `-shift`, and one more.
fn bulk_charge(op: &wasmparser::Operator<'_>) -> Option<i64> {
    use wasmparser::Operator as O;
    Some(match op {
        O::MemoryFill { .. } | O::MemoryCopy { .. } | O::MemoryInit { .. } => 6,
        O::MemoryGrow { .. } => -10,
        O::TableFill { .. } | O::TableCopy { .. } | O::TableInit { .. } | O::TableGrow { .. } => 0,
        _ => return None,
    })
}

impl Rewrite {
    fn charge(&self, f: &mut wasm_encoder::Function) {
        use wasm_encoder::Instruction as I;
        f.instruction(&I::GlobalGet(self.fuel));
        f.instruction(&I::I64Const(1));
        f.instruction(&I::I64Sub);
        f.instruction(&I::GlobalSet(self.fuel));
        self.trap_when_out(f);
    }

    fn trap_when_out(&self, f: &mut wasm_encoder::Function) {
        use wasm_encoder::Instruction as I;
        f.instruction(&I::GlobalGet(self.fuel));
        f.instruction(&I::I64Const(0));
        f.instruction(&I::I64LtS);
        f.instruction(&I::If(wasm_encoder::BlockType::Empty));
        f.instruction(&I::Unreachable);
        f.instruction(&I::End);
    }

    /// Charge the count on top of the stack, keeping it there: it is kept in the scratch local
    /// `count` meanwhile.
    fn charge_count(&self, f: &mut wasm_encoder::Function, count: u32, shift: i64) {
        use wasm_encoder::Instruction as I;
        f.instruction(&I::LocalTee(count));
        f.instruction(&I::GlobalGet(self.fuel));
        f.instruction(&I::LocalGet(count));
        f.instruction(&I::I64ExtendI32U);
        if shift > 0 {
            f.instruction(&I::I64Const((1 << shift) - 1));
            f.instruction(&I::I64Add);
            f.instruction(&I::I64Const(shift));
            f.instruction(&I::I64ShrU);
        } else if shift < 0 {
            f.instruction(&I::I64Const(-shift));
            f.instruction(&I::I64Shl);
        }
        f.instruction(&I::I64Const(1));
        f.instruction(&I::I64Add);
        f.instruction(&I::I64Sub);
        f.instruction(&I::GlobalSet(self.fuel));
        self.trap_when_out(f);
    }

    fn fuel_global() -> (wasm_encoder::GlobalType, wasm_encoder::ConstExpr) {
        (
            wasm_encoder::GlobalType {
                val_type: wasm_encoder::ValType::I64,
                mutable: true,
                shared: false,
            },
            wasm_encoder::ConstExpr::i64_const(0),
        )
    }
}

impl Reencode for Rewrite {
    type Error = std::convert::Infallible;

    fn memory_type(
        &mut self,
        ty: wasmparser::MemoryType,
    ) -> Result<wasm_encoder::MemoryType, reencode::Error<Self::Error>> {
        let mut t = RoundtripReencoder.memory_type(ty)?;
        t.maximum = Some(t.maximum.map_or(self.max_pages, |m| m.min(self.max_pages)));
        Ok(t)
    }

    fn parse_global_section(
        &mut self,
        globals: &mut wasm_encoder::GlobalSection,
        section: wasmparser::GlobalSectionReader<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        reencode::utils::parse_global_section(self, globals, section)?;
        let (ty, init) = Self::fuel_global();
        globals.global(ty, &init);
        self.global_added = true;
        Ok(())
    }

    fn intersperse_section_hook(
        &mut self,
        module: &mut wasm_encoder::Module,
        _after: Option<wasm_encoder::SectionId>,
        before: Option<wasm_encoder::SectionId>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        // A module with no globals of its own gets a global section just before its exports.
        if !self.global_added && before == Some(wasm_encoder::SectionId::Export) {
            let mut g = wasm_encoder::GlobalSection::new();
            let (ty, init) = Self::fuel_global();
            g.global(ty, &init);
            module.section(&g);
            self.global_added = true;
        }
        Ok(())
    }

    fn parse_export_section(
        &mut self,
        exports: &mut wasm_encoder::ExportSection,
        section: wasmparser::ExportSectionReader<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        reencode::utils::parse_export_section(self, exports, section)?;
        exports.export(FUEL_EXPORT, wasm_encoder::ExportKind::Global, self.fuel);
        Ok(())
    }

    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        // The function's own locals, then one `i32` scratch local for the counts charged.
        let mut locals = Vec::new();
        let mut declared = 0u32;
        for l in func.get_locals_reader()? {
            let (n, ty) = l?;
            declared = declared.saturating_add(n);
            locals.push((n, self.val_type(ty)?));
        }
        locals.push((1, wasm_encoder::ValType::I32));
        let count = self
            .params
            .get(self.body)
            .copied()
            .unwrap_or(0)
            .saturating_add(declared);
        self.body += 1;
        let mut f = wasm_encoder::Function::new(locals);
        self.charge(&mut f);
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            let op = reader.read()?;
            let is_loop = matches!(op, wasmparser::Operator::Loop { .. });
            if let Some(shift) = bulk_charge(&op) {
                self.charge_count(&mut f, count, shift);
            }
            f.instruction(&self.instruction(op)?);
            if is_loop {
                self.charge(&mut f);
            }
        }
        code.function(&f);
        Ok(())
    }

    fn parse_custom_section(
        &mut self,
        _module: &mut wasm_encoder::Module,
        _section: wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        // Names, producers and the component's type description are not needed to run it.
        Ok(())
    }
}
