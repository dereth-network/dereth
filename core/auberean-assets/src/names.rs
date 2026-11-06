//! Records' file names: the table that maps the name of a record's source file to the record's id
//! and back, per record type, and the client's own table of every record's file name.
//!
//! Two tables of names live in the client. The file-id table record (`0x0E000003`) is one: the
//! names of the music and script library files, by id, which the sound code asks when music names
//! a file ([`NameTable::from_file_ids`]). The other ([`FileNames`]) holds the names of every
//! record; the authoring tools filled it from a text file of names beside the program, which no
//! install of the relaunched game carries, so in the client it starts empty and stays empty: every
//! id it is asked for is 0, every name and path it is asked for is empty.

use std::collections::HashMap;

use auberean_dat::types::type_of_file_name;
use auberean_dat::{type_of_id, MasterMap, RecordType};

use crate::tables::FileIdTable;
use crate::Database;

/// What an entry of the names table is, and how [`NameTable::find_by_name`] makes one it lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// A name not yet given an id.
    NewName,
    /// Look for an entry only; make none.
    OnlyExisting,
    /// A folder not yet searched.
    UnsearchedFolder,
    /// A folder searched.
    SearchedFolder,
    /// A file with no id.
    NoId,
    /// A file with an id.
    WithId,
}

/// One name in the table.
// D2-14630, D2-14632
#[derive(Clone, Debug, PartialEq)]
pub struct NameEntry {
    /// The folder's path, for a folder (empty for a file).
    pub path: String,
    /// The name, as the table was given it.
    pub name: String,
    /// The record's id (0 until one is given).
    pub id: u32,
    /// The record's type (0 until an id is given).
    pub record_type: u32,
    pub kind: EntryKind,
    /// Whether the name's path is known for certain.
    pub authoritative: bool,
}

/// One record type in the table: its name, its folders, and its ids' names.
// D2-14652, D2-14664
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeEntry {
    /// Whether the type's folders were searched.
    pub searched: bool,
    /// Whether the type was registered.
    pub registered: bool,
    /// The type's folder under the game's folder, and under the engine's (empty unless
    /// registered with a folder that names a drive).
    pub game_root: String,
    pub engine_root: String,
    pub name: String,
    /// The greatest id given a name of this type.
    pub highest_id: u32,
    /// Each id's entry: the first name given the id keeps it.
    ids: HashMap<u32, usize>,
}

impl TypeEntry {
    /// How many ids of the type have a name.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether no id of the type has a name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// The table of names: every entry by its name (letter case ignored), and by type, each type's
/// entries by id.
// D2-14666
#[derive(Clone, Debug, Default)]
pub struct NameTable {
    entries: Vec<NameEntry>,
    by_name: HashMap<String, usize>,
    types: HashMap<u32, TypeEntry>,
}

/// A file's name without its folders: what follows the last `\`, `/` or `:` (the whole name when
/// it has none).
// D2-17796
#[must_use]
pub fn strip_folders(name: &str) -> &str {
    name.rfind(['\\', '/', ':'])
        .map_or(name, |i| &name[i + 1..])
}

/// A folder's name as the table keeps it: letters lowered, each `/` turned into `\` and a `//`
/// dropped, one trailing `\` taken off; and whether it names a drive (an odd number of `:`).
// D2-14637
#[must_use]
pub fn reformat_folder(dir: &str) -> (String, bool) {
    let chars: Vec<char> = dir.chars().collect();
    let mut out = String::with_capacity(dir.len());
    let mut drive = false;
    for (i, &c) in chars.iter().enumerate() {
        if c == ':' {
            drive = !drive;
        }
        if c == '/' {
            if chars.get(i + 1) == Some(&'/') {
                continue;
            }
            out.push('\\');
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    if out.ends_with('\\') {
        out.pop();
    }
    (out, drive)
}

impl NameTable {
    /// An empty table.
    // D2-14665
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The table the file-id table record holds, read as the client reads it: the table cleared,
    /// then each type's entry made and named, and each of its names given its id
    /// ([`NameTable::assign_id`]: a name whose extension is not the type's gets none).
    #[must_use]
    pub fn from_file_ids(record: &FileIdTable) -> Self {
        let mut t = Self::new();
        t.clear();
        for ty in &record.types {
            t.type_entry(ty.record_type).name.clone_from(&ty.name);
            for e in &ty.entries {
                t.assign_id(&e.name, e.id.0, ty.record_type);
            }
        }
        t
    }

    /// Forget every name and every type.
    // D2-14660
    pub fn clear(&mut self) {
        self.entries.clear();
        self.by_name.clear();
        self.types.clear();
    }

    /// A type's entry, made (not registered, unnamed) when the table has none.
    // D2-14669
    pub fn type_entry(&mut self, ty: u32) -> &mut TypeEntry {
        self.types.entry(ty).or_default()
    }

    /// A type's entry, if the table has one.
    #[must_use]
    pub fn type_entry_if_any(&self, ty: u32) -> Option<&TypeEntry> {
        self.types.get(&ty)
    }

    /// An entry by its name, letter case ignored. One the table lacks is made unless `kind` is
    /// [`EntryKind::OnlyExisting`]: a folder's entry has the name as its path too; the new entry
    /// has no id, the kind asked for, and is not authoritative.
    // D2-14670
    pub fn find_by_name(&mut self, name: &str, kind: EntryKind) -> Option<usize> {
        let key = name.to_ascii_lowercase();
        if let Some(&i) = self.by_name.get(&key) {
            return Some(i);
        }
        if kind == EntryKind::OnlyExisting {
            return None;
        }
        let folder = matches!(
            kind,
            EntryKind::SearchedFolder | EntryKind::UnsearchedFolder
        );
        self.entries.push(NameEntry {
            path: if folder {
                name.to_owned()
            } else {
                String::new()
            },
            name: name.to_owned(),
            id: 0,
            record_type: 0,
            kind,
            authoritative: false,
        });
        let i = self.entries.len() - 1;
        self.by_name.insert(key, i);
        Some(i)
    }

    /// An entry by its id and type (the type's entry made when the table has none).
    // D2-14650
    pub fn find_by_id(&mut self, id: u32, ty: u32) -> Option<usize> {
        self.type_entry(ty).ids.get(&id).copied()
    }

    /// The entry at an index [`find_by_name`](Self::find_by_name) or
    /// [`find_by_id`](Self::find_by_id) gave.
    #[must_use]
    pub fn entry(&self, i: usize) -> &NameEntry {
        &self.entries[i]
    }

    /// Give a file's name an id of a type. Nothing is given, and the answer is `false`, unless the
    /// name's extension is the type's. The name is kept without its folders and in lower case; a
    /// name that already has an id keeps it (`true`). Otherwise the name takes the id, and the
    /// type's entry lists the name under the id unless another name has it already (then the
    /// answer is `false`, the name keeping the id all the same).
    // D2-14667
    pub fn assign_id(&mut self, name: &str, id: u32, ty: u32) -> bool {
        if type_of_file_name(name).map_or(0, |t| t.0) != ty {
            return false;
        }
        let file = strip_folders(name).to_ascii_lowercase();
        let Some(i) = self.find_by_name(&file, EntryKind::NoId) else {
            return false;
        };
        if self.entries[i].id != 0 {
            return true;
        }
        let e = &mut self.entries[i];
        e.id = id;
        e.record_type = ty;
        e.kind = EntryKind::WithId;
        let t = self.type_entry(ty);
        if t.ids.contains_key(&id) {
            return false;
        }
        t.ids.insert(id, i);
        if t.highest_id <= id {
            t.highest_id = id;
        }
        true
    }

    /// The id of a file's name (folders and letter case ignored), or 0 for a name the table does
    /// not hold. The type is not consulted: a name holds one id.
    // D2-14642
    #[must_use]
    pub fn id_by_name(&self, name: &str, _ty: u32) -> u32 {
        let key = strip_folders(name).to_ascii_lowercase();
        self.by_name.get(&key).map_or(0, |&i| self.entries[i].id)
    }

    /// The file's name of an id of a type (the type's entry made when the table has none).
    // D2-14633
    pub fn name_by_id(&mut self, id: u32, ty: u32) -> Option<String> {
        self.find_by_id(id, ty)
            .map(|i| self.entries[i].name.clone())
    }

    /// Register a type under a name, with its folders under the game's and the engine's folders:
    /// `false` for a type registered already. A folder that names a drive is kept, as an entry of
    /// the table, and the type counts as searched only when both such folders are; other folders
    /// are not kept.
    // D2-14639
    pub fn register_type(&mut self, ty: u32, name: &str, engine_dir: &str, game_dir: &str) -> bool {
        let t = self.type_entry(ty);
        if t.registered {
            return false;
        }
        name.clone_into(&mut t.name);
        t.registered = true;
        t.searched = false;
        for (dir, game) in [(game_dir, true), (engine_dir, false)] {
            let (folder, drive) = reformat_folder(dir);
            if !drive {
                continue;
            }
            let Some(i) = self.find_by_name(&folder, EntryKind::UnsearchedFolder) else {
                continue;
            };
            let searched = self.entries[i].kind == EntryKind::SearchedFolder;
            let path = self.entries[i].path.clone();
            let t = self.type_entry(ty);
            if game {
                t.searched = searched;
                t.game_root = path;
            } else {
                t.searched &= searched;
                t.engine_root = path;
            }
        }
        true
    }

    /// Whether a type was registered.
    // D2-14648
    #[must_use]
    pub fn is_type_registered(&self, ty: u32) -> bool {
        self.types.get(&ty).is_some_and(|t| t.registered)
    }

    /// How many names the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table holds no name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A wide text as a narrow one: a character a unit, one past the narrow range as `?`.
fn narrowed(s: &[u16]) -> String {
    s.iter()
        .map(|&c| u8::try_from(c).map_or('?', char::from))
        .collect()
}

/// A narrow text as a wide one.
fn widened(s: &str) -> Vec<u16> {
    s.chars()
        .map(|c| u16::try_from(u32::from(c)).unwrap_or(0x3F))
        .collect()
}

/// The characters a name is trimmed of at both ends.
const WHITESPACE: &[char] = &[' ', '\n', '\r', '\t', '\x0C'];

/// The enumeration value and mapping that find the enum map of the record types' names.
const TYPE_NAMES: (u32, u32) = (0x4000_0001, 0x4000_0001);

/// The client's table of every record's file name, with the switches the client sets on it.
///
/// The client starts it when its data cache starts (folders off, ids not to be made), with the
/// text file of names beside the program when there is one. No install of the relaunched game
/// has that file, and this table is the client's without it: started empty, it stays empty, so
/// every id asked of it is 0 and every name and path empty. Searching folders for source files and
/// making ids for new names, which the client never turns on, are the authoring tools' and are
/// not here: with either on, a name not held is still answered 0 and a path empty.
// D2-14675, D2-14676, D2-14704, D2-14705
#[derive(Clone, Debug)]
pub struct FileNames {
    table: NameTable,
    started: u32,
    uses_folders: bool,
    ids_may_be_made: bool,
    engine_only: bool,
    read_only: bool,
    new_name_file: bool,
    starting: bool,
    game_pack_stamp: i32,
    /// The places names are kept: the text file's, added at each first start.
    stores: usize,
}

impl Default for FileNames {
    fn default() -> Self {
        Self::new()
    }
}

impl FileNames {
    /// The table before the client starts it: folders off, ids allowed to be made.
    #[must_use]
    pub fn new() -> Self {
        Self {
            table: NameTable::new(),
            started: 0,
            uses_folders: false,
            ids_may_be_made: true,
            engine_only: false,
            read_only: false,
            new_name_file: false,
            starting: false,
            game_pack_stamp: 0,
            stores: 0,
        }
    }

    /// Whether the table is started.
    // D2-14598
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.started != 0
    }

    /// Whether names are looked for in the folders (the client turns this off).
    // D2-14599, D2-14677
    pub fn set_uses_folders(&mut self, on: bool) {
        self.uses_folders = on;
    }

    /// Whether a name not held may be given a new id (the client turns this off).
    // D2-14600
    pub fn set_ids_may_be_made(&mut self, on: bool) {
        self.ids_may_be_made = on;
    }

    /// Start the table, with no text file of names: `flags` bit 0 is "the engine's only", bit 2
    /// "only read"; the game data's pack stamp is kept. A table started already counts one more
    /// start and answers `true`. With folders on, the start fails unless both folders are set.
    // D2-14621, D2-14620, D2-00014
    pub fn init(&mut self, flags: u32, game_pack_stamp: i32, master: &MasterMap) -> bool {
        self.started = self.started.wrapping_add(1);
        if self.started != 1 {
            return true;
        }
        self.table.clear();
        if self.uses_folders
            && (master.game_folder().is_empty() || master.engine_folder().is_empty())
        {
            return false;
        }
        self.engine_only = flags & 1 != 0;
        self.read_only = flags >> 2 & 1 != 0;
        self.starting = true;
        self.game_pack_stamp = game_pack_stamp;
        // The text file's place of names is added, and told whom to tell what it reads.
        self.stores += 1;
        // No file of names: a new one, which is only read.
        self.new_name_file = true;
        self.read_only = true;
        true
    }

    /// Check the names file's types against the game's: a new file has none to check (`true`).
    // D2-14605
    pub fn verify_types(&mut self) -> bool {
        self.starting = false;
        true
    }

    /// Stop the table once: the last stop forgets every name.
    // D2-14604, D2-14697
    pub fn cleanup(&mut self) {
        self.started = self.started.wrapping_sub(1);
        if self.started == 0 {
            self.table.clear();
        }
    }

    /// Register a record type with the table once: named for the enum maps' type names,
    /// `ENUM_MAPPER` and `DID_MAPPER` for those two types, `UNKNOWN` without a name (the names are
    /// looked up only once the data cache is started: `db`); with its folders when its records
    /// are files of their own.
    // D2-14607, D2-14694
    pub fn register_type(&mut self, ty: u32, db: Option<&Database>, master: &MasterMap) {
        if self.table.is_type_registered(ty) {
            return;
        }
        let name = match RecordType(ty) {
            RecordType::ENUM_MAP => "ENUM_MAPPER".to_owned(),
            RecordType::ID_MAP => "DID_MAPPER".to_owned(),
            _ => db
                .and_then(|db| {
                    let map = db.data_id_by_enum(TYPE_NAMES.0, TYPE_NAMES.1);
                    db.enum_map_name(map, ty)
                })
                .unwrap_or_else(|| "UNKNOWN".to_owned()),
        };
        let (engine, game) = if RecordType(ty).has_file() {
            (
                master.engine_data_dir(RecordType(ty)),
                master.game_data_dir(RecordType(ty)),
            )
        } else {
            (String::new(), String::new())
        };
        self.table.register_type(ty, &name, &engine, &game);
    }

    /// The id of a named file of a type: 0 for an empty name, type 0, a table not started, or a
    /// name (trimmed) whose extension is not the type's; else the table's (the type registered
    /// first).
    // D2-14615
    pub fn id(&mut self, name: &str, ty: u32, db: Option<&Database>, master: &MasterMap) -> u32 {
        if name.is_empty() || ty == 0 || self.started == 0 {
            return 0;
        }
        let name = name.trim_matches(WHITESPACE);
        self.register_type(ty, db, master);
        if type_of_file_name(name).map_or(0, |t| t.0) != ty {
            return 0;
        }
        self.table.id_by_name(name, ty)
    }

    /// [`FileNames::id`] of a wide name.
    pub fn id_wide(
        &mut self,
        name: &[u16],
        ty: u32,
        db: Option<&Database>,
        master: &MasterMap,
    ) -> u32 {
        self.id(&narrowed(name), ty, db, master)
    }

    /// The id of a named file, typed by its extension: 0 for a table not started or a name with no
    /// type.
    // D2-14617
    pub fn id_by_extension(
        &mut self,
        name: &str,
        db: Option<&Database>,
        master: &MasterMap,
    ) -> u32 {
        if self.started == 0 {
            return 0;
        }
        match type_of_file_name(name.trim_matches(WHITESPACE)) {
            Some(t) => self.id(name, t.0, db, master),
            None => 0,
        }
    }

    /// [`FileNames::id_by_extension`] of a wide name.
    pub fn id_by_extension_wide(
        &mut self,
        name: &[u16],
        db: Option<&Database>,
        master: &MasterMap,
    ) -> u32 {
        self.id_by_extension(&narrowed(name), db, master)
    }

    /// The file's name of an id of a type: empty for id 0, type 0, a table not started, or an id
    /// the table does not name (the type registered first).
    // D2-14610
    pub fn file_name(
        &mut self,
        id: u32,
        ty: u32,
        db: Option<&Database>,
        master: &MasterMap,
    ) -> String {
        if id == 0 || ty == 0 || self.started == 0 {
            return String::new();
        }
        self.register_type(ty, db, master);
        self.table.name_by_id(id, ty).unwrap_or_default()
    }

    /// The file's name of an id, typed by the id.
    // D2-14611
    pub fn file_name_of(&mut self, id: u32, db: Option<&Database>, master: &MasterMap) -> String {
        if id == 0 {
            return String::new();
        }
        self.file_name(id, type_of_id(id).map_or(0, |t| t.0), db, master)
    }

    /// [`FileNames::file_name_of`] as a wide text.
    pub fn file_name_of_wide(
        &mut self,
        id: u32,
        db: Option<&Database>,
        master: &MasterMap,
    ) -> Vec<u16> {
        widened(&self.file_name_of(id, db, master))
    }

    /// The full path of an id's source file: empty, the folders being off.
    // D2-14608
    #[must_use]
    pub fn path(&self, _id: u32, _ty: u32) -> String {
        String::new()
    }

    /// The full path of an id's source file, typed by the id: empty, the folders being off.
    // D2-14609
    #[must_use]
    pub fn path_of(&self, id: u32) -> String {
        self.path(id, type_of_id(id).map_or(0, |t| t.0))
    }

    /// The full path of a named source file: empty, the folders being off.
    // D2-14613
    #[must_use]
    pub fn path_by_name(&self, _name: &str, _ty: u32) -> String {
        String::new()
    }

    /// The full path of a named source file, typed by its extension: empty, the folders being off.
    // D2-14619
    #[must_use]
    pub fn path_by_extension(&self, name: &str) -> String {
        self.path_by_name(name, type_of_file_name(name).map_or(0, |t| t.0))
    }

    /// The table of names.
    #[must_use]
    pub fn table(&self) -> &NameTable {
        &self.table
    }

    /// Whether the table was started as the engine's only.
    #[must_use]
    pub fn is_engine_only(&self) -> bool {
        self.engine_only
    }

    /// Whether the table is only read.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the table's file of names is a new one (no file was found).
    #[must_use]
    pub fn is_new_name_file(&self) -> bool {
        self.new_name_file
    }

    /// Whether the table was started and its types not yet checked.
    #[must_use]
    pub fn is_starting(&self) -> bool {
        self.starting
    }

    /// The game data's pack stamp the table was started with.
    #[must_use]
    pub fn game_pack_stamp(&self) -> i32 {
        self.game_pack_stamp
    }

    /// How many places of names the table was given.
    #[must_use]
    pub fn name_stores(&self) -> usize {
        self.stores
    }

    /// Whether names are looked for in the folders.
    #[must_use]
    pub fn uses_folders(&self) -> bool {
        self.uses_folders
    }

    /// Whether a name not held may be given a new id.
    #[must_use]
    pub fn ids_may_be_made(&self) -> bool {
        self.ids_may_be_made
    }
}
