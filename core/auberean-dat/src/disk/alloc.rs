//! The block allocator: records stored as chains of blocks, the free list, the file grown, and the
//! header and the transaction slot written.
//!
//! A block is `[next][payload]`: `next` is the offset of the chain's next block, 0 on its last.
//! A free block's `next` has bit 31 set. Blocks are taken from the free list's head and given back
//! at its tail.

use super::device::{Files, Handle, OpenMode};
use super::{DiskError, Result, Transaction, INFO_AT, MAGIC, TRANSACTION_AT};
use crate::stamp::DatIdStamp;

/// The first block.
pub const FIRST_BLOCK: u32 = 0x400;
/// A free block's mark.
pub const FREE: u32 = 0x8000_0000;
/// How much the file grows by when it runs short.
pub const GROWTH: u32 = 0x10_0000;

/// The header (`0x54` bytes at `300`), as the file holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreHeader {
    pub magic: u32,
    pub block_size: u32,
    pub file_size: u32,
    /// 1 portal and pictures, 2 cell, 3 local; `UNDEF` none.
    pub data_set: u32,
    /// A cell file's region, a local file's language.
    pub data_subset: u32,
    pub first_free: u32,
    pub final_free: u32,
    pub free_count: u32,
    pub root: u32,
    pub young_lru: u32,
    pub old_lru: u32,
    pub use_lru: u32,
    pub master_map_id: u32,
    pub eng_pack_vnum: i32,
    pub game_pack_vnum: i32,
    /// The word before the stamp, which nothing reads: a file keeps whatever it holds; one made
    /// here holds 0.
    pub stamp_word: u32,
    pub stamp: DatIdStamp,
}

impl StoreHeader {
    /// The header's size.
    pub const BYTES: usize = 0x54;
    /// The data set of no file.
    pub const UNDEF: u32 = 0xFFFF_FFFF;

    /// A header for no file yet: nothing allocated, no data versions, an empty stamp.
    // D2-17646
    #[must_use]
    pub fn new() -> Self {
        let mut h = Self {
            magic: MAGIC,
            block_size: 0,
            file_size: 0,
            data_set: Self::UNDEF,
            data_subset: 0,
            first_free: 0,
            final_free: 0,
            free_count: 0,
            root: 0,
            young_lru: 0,
            old_lru: 0,
            use_lru: 0,
            master_map_id: 0,
            eng_pack_vnum: -1,
            game_pack_vnum: -1,
            stamp_word: 0,
            stamp: DatIdStamp::default(),
        };
        h.clear_data();
        h
    }

    /// Back to [`StoreHeader::new`]'s values; the stamp's leading word is kept.
    // D2-17631
    pub fn clear_data(&mut self) {
        let word = self.stamp_word;
        *self = Self {
            magic: MAGIC,
            block_size: 0,
            file_size: 0,
            data_set: Self::UNDEF,
            data_subset: 0,
            first_free: 0,
            final_free: 0,
            free_count: 0,
            root: 0,
            young_lru: 0,
            old_lru: 0,
            use_lru: 0,
            master_map_id: 0,
            eng_pack_vnum: -1,
            game_pack_vnum: -1,
            stamp_word: word,
            stamp: DatIdStamp::default(),
        };
    }

    /// Copy another header's fields and stamp; the leading word of the stamp is this one's.
    // D2-17743
    pub fn assign(&mut self, other: &Self) {
        let word = self.stamp_word;
        *self = *other;
        self.stamp_word = word;
    }

    /// The data file's id in the data download: its set in the high word, its subset in the low.
    #[must_use]
    pub fn dat_file_id(&self) -> u64 {
        (u64::from(self.data_set) << 32) | u64::from(self.data_subset)
    }

    /// The header's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let mut b = [0; Self::BYTES];
        let words = [
            self.magic,
            self.block_size,
            self.file_size,
            self.data_set,
            self.data_subset,
            self.first_free,
            self.final_free,
            self.free_count,
            self.root,
            self.young_lru,
            self.old_lru,
            self.use_lru,
            self.master_map_id,
            self.eng_pack_vnum.cast_unsigned(),
            self.game_pack_vnum.cast_unsigned(),
            self.stamp_word,
        ];
        for (i, w) in words.iter().enumerate() {
            b[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
        b[0x40..0x50].copy_from_slice(&self.stamp.major);
        b[0x50..0x54].copy_from_slice(&self.stamp.minor.to_le_bytes());
        b
    }

    /// A header from its bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::BYTES]) -> Self {
        let w = |i: usize| u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        let mut major = [0; 16];
        major.copy_from_slice(&b[0x40..0x50]);
        Self {
            magic: w(0),
            block_size: w(1),
            file_size: w(2),
            data_set: w(3),
            data_subset: w(4),
            first_free: w(5),
            final_free: w(6),
            free_count: w(7),
            root: w(8),
            young_lru: w(9),
            old_lru: w(10),
            use_lru: w(11),
            master_map_id: w(12),
            eng_pack_vnum: w(13).cast_signed(),
            game_pack_vnum: w(14).cast_signed(),
            stamp_word: w(15),
            stamp: DatIdStamp {
                major,
                minor: w(20),
            },
        }
    }
}

/// The text a file's first 0x100 bytes hold when the whole header block is written.
pub const VERSION_TEXT: &[u8] = b"\nFile Header Structure Default Constructor v1.3\n";
/// The header block's size: the text, the transaction slot and the header.
pub const HEADER_BLOCK_BYTES: usize = 0x180;

/// The whole header block as the client builds one to write with a file: the version text from
/// the start (the rest of its 0x100 bytes zero but the last, `0x1A`), a clear transaction slot,
/// and `info`.
// D2-17745
#[must_use]
pub fn header_block(info: &StoreHeader) -> [u8; HEADER_BLOCK_BYTES] {
    let mut b = [0u8; HEADER_BLOCK_BYTES];
    b[..VERSION_TEXT.len()].copy_from_slice(VERSION_TEXT);
    b[0xFF] = 0x1A;
    b[INFO_AT as usize..].copy_from_slice(&info.to_bytes());
    b
}

impl Default for StoreHeader {
    fn default() -> Self {
        Self::new()
    }
}

/// The block allocator of one file: its header and its handle.
#[derive(Debug, Default)]
pub struct Blocks {
    pub info: StoreHeader,
    pub dev: Handle,
    /// Whether the file may grow (else, short of room, the oldest records go).
    pub expandable: bool,
}

impl Blocks {
    /// No file.
    // D2-17708, D2-17709
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Close the file.
    ///
    /// # Errors
    /// As [`Handle::close`].
    // D2-17710
    pub fn close_data_file(&mut self) -> Result<()> {
        self.dev.close()
    }

    /// Write the header.
    ///
    /// # Errors
    /// The handle's.
    // D2-17711
    pub fn save_file_info(&mut self) -> Result<()> {
        let b = self.info.to_bytes();
        self.dev.sync_write(&b, INFO_AT)
    }

    /// Store `data` in a chain from `start`, which must be the free list's head: the chain is the
    /// first blocks of the free list, in its order. The header (the head moved past the chain, the
    /// free count less the chain's length) is written before the chain's last block, so the last
    /// block's end-of-chain link reaches the file after it. A last block is written only as far
    /// as its payload; the rest of the block keeps what it held.
    ///
    /// # Errors
    /// A read or write that fails.
    // D2-17712
    pub fn store_data(&mut self, data: &[u8], start: u32) -> Result<()> {
        let bs = self.info.block_size;
        let per = bs.wrapping_sub(4);
        let mut remaining = data.len();
        let mut pos = 0;
        let mut current = start;
        let mut used = 1u32;
        let first = self.dev.read_word(start);
        let mut next = first.unwrap_or(0) & !FREE;
        let mut r = first.map(|_| ());
        while remaining != 0 && r.is_ok() {
            let mut after = next;
            let (link, take) = if per as usize >= remaining {
                self.info.free_count = self.info.free_count.wrapping_sub(used);
                self.info.first_free = next;
                let _ = self.save_file_info();
                (0, remaining)
            } else {
                after = self.dev.read_word(next).unwrap_or(next) & !FREE;
                used += 1;
                (next, per as usize)
            };
            let mut block = Vec::with_capacity(4 + take);
            block.extend_from_slice(&link.to_le_bytes());
            block.extend_from_slice(&data[pos..pos + take]);
            r = self.dev.sync_write(&block, current);
            if r.is_ok() {
                remaining -= take;
                pos += take;
                current = link;
            }
            next = after;
        }
        r
    }

    /// Give back a chain stored from `start` that was never entered in a node: each of its links
    /// marked free and its last block linked to the free list's head, and the head moved back to
    /// `start`, the count up by the chain's length.
    ///
    /// # Errors
    /// A read or write that fails.
    // D2-17713
    pub fn store_data_rollback(&mut self, start: u32) -> Result<()> {
        let mut blocks: u32 = 1;
        let mut r = self.dev.read_word(start);
        let mut link = r.unwrap_or(0);
        if r.is_ok() && link & FREE == 0 {
            let mut at = start;
            loop {
                r?;
                if link == 0 {
                    link = self.info.first_free;
                    blocks = blocks.wrapping_sub(1);
                }
                link |= FREE;
                let _ = self.dev.write_word(link, at);
                at = link & !FREE;
                blocks = blocks.wrapping_add(1);
                r = self.dev.read_word(at);
                link = r.unwrap_or(link);
                if link & FREE != 0 {
                    break;
                }
            }
        }
        r?;
        if start != self.info.first_free {
            self.info.first_free = start;
            self.info.free_count = self.info.free_count.wrapping_add(blocks);
            return self.save_file_info();
        }
        Ok(())
    }

    /// Read `out.len()` bytes of the chain from `start`. Fails on a link to a free block, a chain
    /// that ends early, or one that goes on past the data.
    // D2-17714
    pub fn load_data(&mut self, out: &mut [u8], start: u32) -> bool {
        let bs = self.info.block_size as usize;
        let mut per = bs.wrapping_sub(4);
        let mut remaining = out.len();
        let mut pos = 0;
        let mut current = start;
        let mut ok = true;
        while remaining != 0 {
            if current == 0 {
                break;
            }
            if !ok {
                return false;
            }
            if remaining < per {
                per = remaining;
            }
            let mut block = vec![0u8; 4 + per];
            match self.dev.sync_read(&mut block, current) {
                Ok(()) => {
                    out[pos..pos + per].copy_from_slice(&block[4..]);
                    current = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
                    remaining -= per;
                    pos += per;
                    if current & FREE != 0 {
                        current &= !FREE;
                        ok = false;
                    }
                }
                Err(_) => ok = false,
            }
        }
        if !ok {
            return false;
        }
        current == 0 && remaining == 0
    }

    /// Free the chain from `start`: every link marked free, the old tail linked to it, the tail
    /// moved to its last block and the count set to `count` plus its length. A link already
    /// free fails unless `allow_free`.
    ///
    /// # Errors
    /// [`DiskError::Corrupt`] for a block already free; a read or write that fails.
    // D2-17715
    pub fn delete_blocks(
        &mut self,
        start: u32,
        final_free: u32,
        count: u32,
        allow_free: bool,
    ) -> Result<()> {
        let mut freed: u32 = 0;
        let mut last = start;
        let mut link = start;
        if start != 0 {
            let mut r: Result<()> = Ok(());
            loop {
                last = link;
                r?;
                r = match self.dev.read_word(link) {
                    Ok(next) => {
                        link = next;
                        if !allow_free && next & FREE != 0 {
                            Err(DiskError::Corrupt)
                        } else {
                            let w = self.dev.write_word(next | FREE, last);
                            link = next & !FREE;
                            freed = freed.wrapping_add(1);
                            w
                        }
                    }
                    Err(e) => Err(e),
                };
                if link == 0 {
                    break;
                }
            }
            r?;
        }
        self.dev.write_word(start | FREE, final_free)?;
        self.info.final_free = last;
        self.info.free_count = freed.wrapping_add(count);
        self.save_file_info()
    }

    /// Grow the file by `bytes` (whole blocks): the new blocks zeroed and chained free, added at
    /// the free list's tail. The header is written whether or not the blocks were.
    ///
    /// # Errors
    /// The header's write.
    // D2-17716
    pub fn expand_file(&mut self, bytes: u32) -> Result<()> {
        let bs = self.info.block_size;
        let n = bytes.cast_signed() / bs.cast_signed();
        self.info.free_count = self.info.free_count.wrapping_add(n.cast_unsigned());
        let mut block = vec![0u8; bs as usize];
        let mut end = self.info.file_size;
        let mut last = end;
        let mut wrote = true;
        for _ in 0..n.max(0) {
            last = end;
            end = last.wrapping_add(bs);
            block[..4].copy_from_slice(&(end | FREE).to_le_bytes());
            if self.dev.sync_write(&block, last).is_err() {
                wrote = false;
                break;
            }
        }
        if wrote {
            let _ = self.dev.write_word(FREE, last);
            let tail = self.info.final_free;
            let link = self.info.file_size | FREE;
            self.info.file_size = end;
            self.info.final_free = last;
            let _ = self.dev.write_word(link, tail);
        }
        self.save_file_info()
    }

    /// Write a change about to be made into the transaction slot.
    ///
    /// # Errors
    /// The handle's.
    // D2-17717
    pub fn save_transaction(&mut self, t: &Transaction) -> Result<()> {
        self.dev.sync_write(&t.to_bytes(), TRANSACTION_AT)
    }

    /// Clear the transaction slot.
    ///
    /// # Errors
    /// The handle's.
    // D2-17718
    pub fn clear_transaction(&mut self) -> Result<()> {
        self.dev
            .sync_write(&[0; Transaction::BYTES], TRANSACTION_AT)
    }

    /// Make a new file: `info` (block size, file size and the rest) becomes its header, every
    /// block from the first to the file's size is chained free, then the header is written and the
    /// transaction slot cleared.
    ///
    /// # Errors
    /// The file does not open, or a write fails.
    // D2-17719
    pub fn create_data_file(
        &mut self,
        info: StoreHeader,
        files: &mut dyn Files,
        file: &str,
        path: &str,
        expandable: bool,
    ) -> Result<()> {
        self.info = info;
        self.expandable = expandable;
        self.dev.open_file(files, file, path, OpenMode::Create)?;
        self.format()
    }

    /// The new file's free list and header ([`Blocks::create_data_file`] once the file is open).
    ///
    /// # Errors
    /// A write that fails.
    // D2-17719
    pub fn format(&mut self) -> Result<()> {
        let bs = self.info.block_size;
        let n = self.info.file_size.wrapping_sub(FIRST_BLOCK).cast_signed() / bs.cast_signed();
        self.info.free_count = n.cast_unsigned();
        let mut block = vec![0u8; bs as usize];
        self.info.first_free = FIRST_BLOCK;
        let mut end = FIRST_BLOCK;
        let mut last = FIRST_BLOCK;
        for _ in 0..n.max(0) {
            last = end;
            end = last.wrapping_add(bs);
            block[..4].copy_from_slice(&(end | FREE).to_le_bytes());
            self.dev.sync_write(&block, last)?;
        }
        self.info.final_free = last;
        let r = self.dev.write_word(FREE, last);
        self.info.file_size = end;
        r?;
        self.save_file_info()?;
        self.clear_transaction()
    }

    /// Open a file that is there: its header read (a wrong magic word fails and closes it), and
    /// the transaction slot.
    ///
    /// # Errors
    /// The file does not open or read, or is not a data file.
    // D2-17720
    pub fn open_data_file(
        &mut self,
        files: &mut dyn Files,
        file: &str,
        path: &str,
        read_only: bool,
        expandable: bool,
    ) -> Result<Transaction> {
        self.expandable = expandable;
        self.dev
            .open_file(files, file, path, OpenMode::Existing { read_only })?;
        self.read_open()
    }

    /// The header and the transaction slot of the file just opened ([`Blocks::open_data_file`]).
    ///
    /// # Errors
    /// A read that fails, or a wrong magic word (the file is then closed).
    // D2-17720
    pub fn read_open(&mut self) -> Result<Transaction> {
        let mut h = [0u8; StoreHeader::BYTES];
        let r = self.dev.sync_read(&mut h, INFO_AT);
        self.info = StoreHeader::from_bytes(&h);
        if self.info.magic != MAGIC {
            let _ = self.dev.close();
            return Err(DiskError::BadMagic);
        }
        r?;
        let mut t = [0u8; Transaction::BYTES];
        self.dev.sync_read(&mut t, TRANSACTION_AT)?;
        Ok(Transaction::from_bytes(&t))
    }

    /// Rewrite the chain from `start` with `data` in place: its blocks reused in order, more taken
    /// from the free list's head when it is too short (the header then changed in memory only),
    /// the rest of it freed when it is too long.
    ///
    /// # Errors
    /// A write that fails.
    // D2-17721
    pub fn update(&mut self, data: &[u8], start: u32) -> Result<()> {
        let bs = self.info.block_size;
        let mut per = bs.wrapping_sub(4) as usize;
        let mut wlen = bs as usize;
        let mut remaining = data.len();
        let mut pos = 0;
        let mut current = start;
        let mut next: u32 = 0;
        let mut from_free = false;
        let mut r: Result<()> = Ok(());
        while remaining != 0 && r.is_ok() {
            if per < remaining {
                if !from_free {
                    next = self.dev.read_word(current).unwrap_or(next);
                }
                if from_free || next == 0 {
                    from_free = true;
                    let head = self.info.first_free;
                    let link = self.dev.read_word(head).unwrap_or(0) & !FREE;
                    next = head;
                    self.info.first_free = link;
                    self.info.free_count = self.info.free_count.wrapping_sub(1);
                }
            } else {
                if !from_free {
                    next = self.dev.read_word(current).unwrap_or(next);
                    if next != 0 {
                        let (tail, count) = (self.info.final_free, self.info.free_count);
                        let _ = self.delete_blocks(next, tail, count, false);
                    }
                }
                wlen = remaining + 4;
                next = 0;
                per = remaining;
            }
            let mut block = Vec::with_capacity(wlen);
            block.extend_from_slice(&next.to_le_bytes());
            block.extend_from_slice(&data[pos..pos + wlen - 4]);
            r = self.dev.sync_write(&block, current);
            if r.is_ok() {
                remaining -= per;
                pos += per;
                current = next;
            }
        }
        r
    }
}
