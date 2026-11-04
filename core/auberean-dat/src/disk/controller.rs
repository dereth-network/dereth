//! A data file as the data cache uses it: opened (or made), records looked up, loaded, saved and
//! deleted, and the data versions it holds.

use super::alloc::{Blocks, StoreHeader, GROWTH};
use super::device::Files;
use super::intset::VersionSet;
use super::tree::Directory;
use crate::container::ITERATION_LIST;
use crate::{Reader, Writer};

/// The file may grow when it runs short.
pub const OPEN_EXPANDABLE: u32 = 2;
/// The file is only read.
pub const OPEN_READ_ONLY: u32 = 4;
/// A new file is made.
pub const OPEN_CREATE: u32 = 8;
/// A file that does not open, or holds other data versions than asked, is made anew.
pub const OPEN_CREATE_IF_BAD: u32 = 0x10;

/// What a file is opened as: its name, and what a new one is made with.
// D2-05121
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenSpec {
    pub file: String,
    pub path: String,
    /// For a new file; set to an opened file's own.
    pub data_set: u32,
    pub data_subset: u32,
    pub open_flags: u32,
    pub file_size: u32,
    pub block_size: u32,
    /// The engine's data version the program wants (negative: any).
    pub eng_pack_vnum: i32,
    /// The game's data version the program wants (negative, or `eng_only`: any).
    pub game_pack_vnum: i32,
    pub eng_only: bool,
}

impl OpenSpec {
    /// A spec from its parts.
    // D2-05118
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file: &str,
        path: &str,
        data_set: u32,
        data_subset: u32,
        open_flags: u32,
        file_size: u32,
        block_size: u32,
        eng_pack_vnum: i32,
        game_pack_vnum: i32,
        eng_only: bool,
    ) -> Self {
        Self {
            file: file.into(),
            path: path.into(),
            data_set,
            data_subset,
            open_flags,
            file_size,
            block_size,
            eng_pack_vnum,
            game_pack_vnum,
            eng_only,
        }
    }
}

/// What a record's entry says ([`Store::is_member`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectInfo {
    pub id: u32,
    pub offset: u32,
    pub size: u32,
}

/// One data file, open or not.
pub struct Store {
    pub initialized: bool,
    pub read_only: bool,
    pub expandable: bool,
    pub data_set: u32,
    pub data_subset: u32,
    pub blocks: Blocks,
    pub tree: Directory,
    pub filename: String,
    /// The time a record stored now is dated with, in seconds since 1970.
    clock: Box<dyn FnMut() -> u32 + Send>,
}

impl core::fmt::Debug for Store {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Store")
            .field("initialized", &self.initialized)
            .field("read_only", &self.read_only)
            .field("filename", &self.filename)
            .field("header", &self.blocks.info)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// No file, dating what it stores by `clock`.
    // D2-17651
    #[must_use]
    pub fn new(clock: Box<dyn FnMut() -> u32 + Send>) -> Self {
        Self {
            initialized: false,
            read_only: false,
            expandable: false,
            data_set: 0,
            data_subset: 0,
            blocks: Blocks::new(),
            tree: Directory::new(),
            filename: String::new(),
            clock,
        }
    }

    /// Open or make the file `spec` names; whether it is ready. A file opened is checked against
    /// the data versions asked for, read, and any change its transaction slot holds finished; one
    /// that fails there (or is asked for with [`OPEN_CREATE`]) is made anew when the spec says so:
    /// an empty tree and an empty list of data versions. `spec` is given an opened file's set and
    /// subset.
    // D2-17654
    pub fn init_file(&mut self, files: &mut dyn Files, spec: &mut OpenSpec) -> bool {
        if self.initialized {
            return false;
        }
        let mut r = Ok(());
        self.read_only = spec.open_flags & OPEN_READ_ONLY != 0;
        self.expandable = spec.open_flags & OPEN_EXPANDABLE != 0;
        let mut opened = false;
        if spec.open_flags & OPEN_CREATE == 0 {
            let t = self.blocks.open_data_file(
                files,
                &spec.file,
                &spec.path,
                self.read_only,
                self.expandable,
            );
            self.filename.clone_from(&spec.file);
            self.blocks.info.use_lru = 0;
            let recreate = spec.open_flags & OPEN_CREATE_IF_BAD != 0;
            match t {
                Ok(t) => {
                    let info = self.blocks.info;
                    spec.data_set = info.data_set;
                    spec.data_subset = info.data_subset;
                    if (spec.eng_pack_vnum < 0 || spec.eng_pack_vnum == info.eng_pack_vnum)
                        && (spec.game_pack_vnum < 0
                            || spec.eng_only
                            || spec.game_pack_vnum == info.game_pack_vnum)
                    {
                        r = self.tree.load_tree(&mut self.blocks);
                        if r.is_ok() && t.kind != 0 {
                            r = self.tree.recover_transaction(&mut self.blocks, &t);
                        }
                        opened = r.is_ok();
                    } else {
                        r = Err(super::DiskError::Incompatible);
                    }
                }
                Err(e) => r = Err(e),
            }
            if !opened && recreate {
                spec.open_flags |= OPEN_CREATE;
            }
        }
        if !opened && spec.open_flags & OPEN_CREATE != 0 {
            let mut info = self.blocks.info;
            info.clear_data();
            info.block_size = spec.block_size;
            info.file_size = spec.file_size;
            info.data_set = spec.data_set;
            info.data_subset = spec.data_subset;
            info.use_lru = 0;
            info.eng_pack_vnum = spec.eng_pack_vnum;
            info.game_pack_vnum = spec.game_pack_vnum;
            r = self
                .blocks
                .create_data_file(info, files, &spec.file, &spec.path, self.expandable);
            if r.is_ok() {
                r = self.tree.create_tree(&mut self.blocks);
            }
            if r.is_ok() {
                r = self.blocks.save_file_info();
            }
            if r.is_ok() {
                let mut empty = VersionSet::new();
                self.save_iteration_list(&mut empty);
            }
        }
        if r.is_err() {
            let _ = self.blocks.close_data_file();
        }
        self.data_subset = self.blocks.info.data_subset;
        self.data_set = self.blocks.info.data_set;
        self.initialized = r.is_ok();
        self.initialized
    }

    /// Close the file, written out first unless it was only read; whether it closed.
    // D2-17637
    pub fn close(&mut self) -> bool {
        if !self.initialized {
            return false;
        }
        if !self.read_only {
            let _ = self.tree.save_data_to_file(&mut self.blocks);
        }
        let r = self.blocks.close_data_file();
        self.tree.destroy_tree();
        self.initialized = false;
        r.is_ok()
    }

    /// Make room for `size` bytes: while the free blocks are not more than the record needs and
    /// 51 more, grow the file a megabyte (or, in a file that may not grow, remove the record used
    /// longest ago). A file that may not grow and has nothing to remove stays short; the client
    /// would wait forever there, and this answers no instead.
    // D2-17638
    pub fn check_room(&mut self, size: u32) -> bool {
        let per = self.blocks.info.block_size.cast_signed().wrapping_sub(4);
        if per <= 0 {
            return false;
        }
        while self.blocks.info.free_count.cast_signed() <= size.cast_signed() / per + 0x33 {
            if self.expandable {
                let before = self.blocks.info.free_count;
                let _ = self.blocks.expand_file(GROWTH);
                if self.blocks.info.free_count == before {
                    return false;
                }
            } else if !self.tree.try_delete_oldest(&mut self.blocks) {
                return false;
            }
        }
        true
    }

    /// When record `id` was stored; 0 when it is not here.
    // D2-17639
    pub fn get_datestamp(&mut self, id: u32) -> u32 {
        self.entry(id).map_or(0, |e| e.date)
    }

    fn entry(&mut self, id: u32) -> Option<super::node::DirEntry> {
        let (at, i) = self.tree.lookup(&mut self.blocks, id)?;
        Some(self.tree.kept(at).node.entry(i))
    }

    /// Record `id`'s bytes; none when it is not here or does not load.
    // D2-17641
    pub fn load_data(&mut self, id: u32) -> Option<Vec<u8>> {
        let e = self.entry(id)?;
        let mut out = vec![0u8; e.size as usize];
        if !self.blocks.load_data(&mut out, e.offset) {
            return None;
        }
        if self.blocks.info.use_lru != 0 && !self.read_only {
            self.tree.restamp_entry(id);
        }
        Some(out)
    }

    /// Whether record `id` is here, and its entry.
    // D2-17642
    pub fn is_member(&mut self, id: u32) -> Option<ObjectInfo> {
        self.entry(id).map(|e| ObjectInfo {
            id: e.id,
            offset: e.offset,
            size: e.size,
        })
    }

    /// Remove record `id`; whether it went (never from a file only read).
    // D2-17643
    pub fn delete_data(&mut self, id: u32) -> bool {
        if self.read_only || self.tree.lookup(&mut self.blocks, id).is_none() {
            return false;
        }
        self.tree.remove(&mut self.blocks, id)
    }

    /// Store `data` as record `id`, replacing one already there; whether it was stored. Id 0,
    /// empty data and a file only read store nothing.
    // D2-17647
    pub fn save_data(&mut self, id: u32, data: &[u8]) -> bool {
        if self.read_only || id == 0 || data.is_empty() {
            return false;
        }
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if !self.check_room(size) {
            return false;
        }
        let now = (self.clock)();
        match self.tree.lookup(&mut self.blocks, id) {
            Some((at, _)) => self
                .tree
                .update_data_trans(&mut self.blocks, at, id, data, now)
                .is_ok(),
            None => self
                .tree
                .descend_to_add(&mut self.blocks, id, data, now)
                .is_ok(),
        }
    }

    /// Remove every record whose id, masked by `mask`, is `id` masked: those found between the
    /// masked id and the masked id with every bit outside the mask set, each removed in the order
    /// the tree lists them. Whether any went; a mask of 0 removes nothing and answers yes.
    // D2-17648
    pub fn delete_data_by_mask(&mut self, id: u32, mask: u32) -> bool {
        if self.read_only {
            return false;
        }
        if mask == 0 {
            return true;
        }
        let lo = id & mask;
        let mut list = Vec::new();
        self.tree
            .get_id_list_range(&mut self.blocks, &mut list, lo, !mask | lo);
        let mut any = false;
        for found in list {
            if found & mask == lo {
                self.delete_data(found);
                any = true;
            }
        }
        any
    }

    /// The data versions the file holds (its iteration list record); none when the record is not
    /// here or does not unpack.
    // D2-17649
    pub fn load_iteration_list(&mut self) -> Option<VersionSet> {
        let raw = self.load_data(ITERATION_LIST)?;
        let mut set = VersionSet::new();
        set.unpack(&mut Reader::new(&raw)).then_some(set)
    }

    /// Store the data versions the file holds; whether it was stored.
    // D2-17650
    pub fn save_iteration_list(&mut self, set: &mut VersionSet) -> bool {
        let mut w = Writer::new();
        set.pack(&mut w);
        self.save_data(ITERATION_LIST, &w.finish())
    }

    /// The free space, in bytes.
    // D2-05124
    #[must_use]
    pub fn howmuch_free_space(&self) -> u32 {
        self.blocks
            .info
            .free_count
            .wrapping_mul(self.blocks.info.block_size)
    }

    /// The name the file was opened by.
    // D2-05127
    #[must_use]
    pub fn get_filename(&self) -> &str {
        &self.filename
    }

    /// The file's id in the data download.
    // D2-05123
    #[must_use]
    pub fn get_dat_file_id(&self) -> u64 {
        (u64::from(self.data_set) << 32) | u64::from(self.data_subset)
    }

    /// The header as it stands.
    #[must_use]
    pub fn header(&self) -> &StoreHeader {
        &self.blocks.info
    }
}

impl Drop for Store {
    /// A store let go is closed (and written out) as [`Store::close`] does.
    // D2-17652, D2-17653
    fn drop(&mut self) {
        self.close();
    }
}
