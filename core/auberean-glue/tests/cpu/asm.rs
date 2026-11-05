//! A small assembler and library builder for hand-written test programs.
//! Behaviour: none (test support, as the script machine's own tests have it)

// Hand-written programs and tables a few hundred entries long: every length and offset fits in
// 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_assets::script::{
    DispatchTables, Export, ExportFunction, Literal, ScriptLibrary, Section,
};
use auberean_script::graph::SavedState;
use auberean_script::library::{Library, NO_CODE};
use auberean_script::native::{Host, NativeTable};
use auberean_script::opcode::op;
use auberean_script::Program;

/// An instruction word with no immediate.
pub fn w(o: u8) -> u32 {
    u32::from(o)
}

/// An instruction word with a 16-bit immediate.
pub fn i(o: u8, v: i16) -> u32 {
    #[allow(clippy::cast_sign_loss)]
    let v = v as u16;
    u32::from(o) | u32::from(v) << 16
}

/// A push of a small value with a tag.
pub fn small(tag: u8, v: i16) -> u32 {
    #[allow(clippy::cast_sign_loss)]
    let v = v as u16;
    u32::from(op::PUSH_SMALL) | u32::from(tag) << 8 | u32::from(v) << 16
}

/// One script package: its id, slots, and functions as code offsets in words (function 0 first;
/// `u32::MAX` leaves the slot empty).
pub struct Pkg {
    pub name: &'static str,
    pub id: u32,
    pub size: u32,
    pub parent: Option<usize>,
    pub functions: Vec<u32>,
    pub native: bool,
}

/// Build a library from code words and packages.
pub fn library(code: &[u32], packages: &[Pkg], literals: &[&str]) -> Library {
    let max_id = packages.iter().map(|p| p.id).max().unwrap_or(0) as usize;
    let mut function_slots = vec![Vec::new(); max_id + 1];
    let mut package_sizes = vec![(0, 0); max_id + 1];
    let mut export_index = vec![NO_CODE; max_id + 1];
    let mut slot_tables = Vec::new();
    let mut exports = Vec::new();
    for (e, p) in packages.iter().enumerate() {
        let n = p.functions.len() as u32;
        function_slots[p.id as usize] = (0..n).collect();
        package_sizes[p.id as usize] = (p.size, 0);
        export_index[p.id as usize] = e as u32;
        let offsets: Vec<u32> = p
            .functions
            .iter()
            .map(|f| {
                if p.native || *f == u32::MAX {
                    NO_CODE
                } else {
                    f * 4
                }
            })
            .collect();
        slot_tables.push(offsets.clone());
        exports.push(Export {
            name: p.name.to_string(),
            base_name: String::new(),
            flags: if p.native { 6 } else { 0 },
            size: p.size,
            checksum: 0,
            package_id: p.id,
            parent: p.parent.map_or(-1, |x| x as i32),
            checkpoints: Vec::new(),
            functions: (0..n)
                .map(|k| ExportFunction {
                    name: format!("f{k}"),
                    id: (p.id << 16) | k,
                    offset: offsets[k as usize],
                    size: 0,
                    flags: 0,
                    dependencies: Vec::new(),
                })
                .collect(),
        });
    }
    let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    let lib = ScriptLibrary {
        version: 15,
        sections: vec![
            Section::Code(bytes),
            Section::Literals(
                literals
                    .iter()
                    .map(|t| Literal {
                        text: (*t).to_string(),
                        uses: Vec::new(),
                    })
                    .collect(),
            ),
            Section::Exports(exports),
            Section::Dispatch(Box::new(DispatchTables {
                function_slots,
                slot_tables,
                package_sizes,
                export_index,
                names: packages
                    .iter()
                    .map(|p| (p.name.to_string(), p.id))
                    .collect(),
            })),
        ],
    };
    Library::from_sections(lib).expect("a library")
}

/// Link a library.
pub fn program(lib: Library, natives: NativeTable) -> Arc<Program> {
    Arc::new(Program::link(lib, natives).expect("links"))
}

/// A host with no saved states, collecting what is sent and the lines printed, drawing from a
/// generator of its own.
#[derive(Default)]
pub struct TestHost {
    pub sent: Vec<(u32, Vec<u8>)>,
    pub dice: auberean_glue::numbers::Random,
    pub printed: Vec<String>,
    /// The client's folder, when the host has one: its files by name.
    pub folder: Option<std::collections::BTreeMap<String, Vec<u8>>>,
    /// The wall clock, when the host has one.
    pub real_time: Option<i64>,
    /// The table of file names: (name, record type) to id.
    pub names: std::collections::BTreeMap<(String, u32), u32>,
    /// The enumeration mappings: (value, mapping) to id.
    pub enum_ids: std::collections::BTreeMap<(u32, u32), u32>,
    /// The records' caches, when the host has them.
    pub database: Option<auberean_assets::Database>,
}

impl Host for TestHost {
    fn saved_state(&mut self, _: u32) -> Option<Arc<SavedState>> {
        None
    }

    fn database(&mut self) -> Option<&auberean_assets::Database> {
        self.database.as_ref()
    }

    fn send(
        &mut self,
        function: u32,
        args: &[u8],
        _encrypted: bool,
    ) -> Result<(), auberean_script::Stop> {
        self.sent.push((function, args.to_vec()));
        Ok(())
    }

    fn random(&mut self) -> Result<f64, auberean_script::Stop> {
        Ok(self.dice.draw())
    }

    fn print(&mut self, line: &str) {
        self.printed.push(line.to_owned());
    }

    fn real_time(&self) -> Option<i64> {
        self.real_time
    }

    fn data_id_by_name(&mut self, name: &str, record_type: u32) -> u32 {
        self.names
            .get(&(name.to_owned(), record_type))
            .copied()
            .unwrap_or(0)
    }

    fn data_id_by_enum(&mut self, value: u32, mapping: u32) -> u32 {
        self.enum_ids.get(&(value, mapping)).copied().unwrap_or(0)
    }

    fn file_name_by_id(&mut self, id: u32) -> Vec<u16> {
        self.names
            .iter()
            .find(|(_, v)| **v == id)
            .map(|((n, _), _)| n.encode_utf16().collect())
            .unwrap_or_default()
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.folder
            .as_mut()
            .map(|f| f.insert(file.to_owned(), bytes.to_vec()))
            .is_some()
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        self.folder.as_ref()?.get(file).cloned()
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
