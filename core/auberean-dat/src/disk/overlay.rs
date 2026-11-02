//! A data file written in memory over one only read: what is written is kept in pages beside the
//! file and read back from them; the file itself is never changed.
//!
//! The client writes downloaded records into its data files. A client whose files are only read
//! (an install shared with the original game, or a test's) keeps those writes for the session
//! instead, through this layer.
//!
//! **Read in chunks.** The disk controller reads a record one block at a time (a block is a
//! kilobyte in most files, a quarter of one in a cell file), and a directory node a few blocks at
//! a time. Each of those would be a call to the system. The file only read is read instead in aligned
//! chunks of [`CHUNK`] bytes, the last [`CHUNKS`] kept, so the blocks of a record, which mostly
//! follow one another in the file, come from one read. What is read is the same either way.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use dereth_dat::DatStorageMut;

use crate::container::DatFile;

const PAGE: usize = 0x1000;
const PAGE_U64: u64 = PAGE as u64;

/// The size of a chunk of the file read at once.
pub const CHUNK: usize = 0x8000;
const CHUNK_U64: u64 = CHUNK as u64;
/// How many chunks are kept (a megabyte in all).
pub const CHUNKS: usize = 32;

/// Where `at` falls: its page and the offset within it.
fn split(at: u64) -> (u64, usize) {
    (at / PAGE_U64, usize::try_from(at % PAGE_U64).unwrap_or(0))
}

/// The file's read error as an I/O error, a short read kept as one.
fn into_io(e: dereth_dat::DatError) -> io::Error {
    match e {
        dereth_dat::DatError::Io(e) => e,
        other => io::Error::other(other),
    }
}

/// A chunk of the file only read, and when it was last used.
#[derive(Debug, Clone)]
struct Chunk {
    at: u64,
    used: u64,
    bytes: Box<[u8]>,
}

/// A file only read, with the pages written over it.
#[derive(Debug, Clone)]
pub struct Overlay {
    base: Arc<DatFile>,
    pages: BTreeMap<u64, Box<[u8]>>,
    len: u64,
    /// The chunks of the file kept, the one used longest ago replaced first.
    chunks: Vec<Chunk>,
    /// The use counter.
    clock: u64,
}

impl Overlay {
    /// The file `base`, nothing written yet.
    #[must_use]
    pub fn new(base: Arc<DatFile>) -> Self {
        let len = u64::from(base.header().file_size);
        Self {
            base,
            pages: BTreeMap::new(),
            len,
            chunks: Vec::new(),
            clock: 0,
        }
    }

    /// Fill `buf` from the file only read at `at` (wholly within it), through the chunks kept.
    fn read_base(&mut self, at: u64, buf: &mut [u8]) -> io::Result<()> {
        let base_len = u64::from(self.base.header().file_size);
        let mut done = 0usize;
        while done < buf.len() {
            let here = at + done as u64;
            let start = here - here % CHUNK_U64;
            let within = usize::try_from(here - start).unwrap_or(0);
            let n = (CHUNK - within).min(buf.len() - done);
            let Some(i) = self.chunk(start, base_len) else {
                // A chunk that does not read whole (a file shorter than its header says) is read
                // as asked.
                self.base
                    .shared()
                    .read_raw(here, &mut buf[done..done + n])
                    .map_err(into_io)?;
                done += n;
                continue;
            };
            let c = &self.chunks[i];
            if c.bytes.len() < within + n {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            buf[done..done + n].copy_from_slice(&c.bytes[within..within + n]);
            done += n;
        }
        Ok(())
    }

    /// The kept chunk starting at `start`, read into the slot used longest ago when not kept;
    /// `None` when it does not read.
    fn chunk(&mut self, start: u64, base_len: u64) -> Option<usize> {
        self.clock += 1;
        if let Some(i) = self.chunks.iter().position(|c| c.at == start) {
            self.chunks[i].used = self.clock;
            return Some(i);
        }
        let n = usize::try_from(base_len.saturating_sub(start).min(CHUNK_U64)).unwrap_or(0);
        let mut bytes = vec![0u8; n].into_boxed_slice();
        self.base.shared().read_raw(start, &mut bytes).ok()?;
        let c = Chunk {
            at: start,
            used: self.clock,
            bytes,
        };
        if self.chunks.len() < CHUNKS {
            self.chunks.push(c);
            return Some(self.chunks.len() - 1);
        }
        let i = self
            .chunks
            .iter()
            .enumerate()
            .min_by_key(|(_, c)| c.used)
            .map_or(0, |(i, _)| i);
        self.chunks[i] = c;
        Some(i)
    }

    /// How many bytes have been written over the file, in whole pages.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.pages.len() as u64 * PAGE_U64
    }

    /// Page `p` as it stands: written, or the file's (zeros past its end).
    ///
    /// # Errors
    /// The file's part of the page does not read (it is never taken as zeros).
    fn page(&mut self, p: u64) -> io::Result<Box<[u8]>> {
        if let Some(b) = self.pages.get(&p) {
            return Ok(b.clone());
        }
        let mut b = vec![0u8; PAGE].into_boxed_slice();
        let at = p * PAGE_U64;
        let base_len = u64::from(self.base.header().file_size);
        if at < base_len {
            let n = usize::try_from((base_len - at).min(PAGE_U64)).unwrap_or(PAGE);
            self.read_base(at, &mut b[..n])?;
        }
        Ok(b)
    }
}

impl DatStorageMut for Overlay {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset + buf.len() as u64;
        if end > self.len {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = offset + done as u64;
            let (p, within) = split(at);
            let n = (PAGE - within).min(buf.len() - done);
            match self.pages.get(&p) {
                Some(b) => buf[done..done + n].copy_from_slice(&b[within..within + n]),
                None if at + n as u64 <= u64::from(self.base.header().file_size) => {
                    self.read_base(at, &mut buf[done..done + n])?;
                }
                None => {
                    let b = self.page(p)?;
                    buf[done..done + n].copy_from_slice(&b[within..within + n]);
                }
            }
            done += n;
        }
        Ok(())
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        let mut done = 0usize;
        while done < buf.len() {
            let at = offset + done as u64;
            let (p, within) = split(at);
            let n = (PAGE - within).min(buf.len() - done);
            if !self.pages.contains_key(&p) {
                let b = self.page(p)?;
                self.pages.insert(p, b);
            }
            if let Some(b) = self.pages.get_mut(&p) {
                b[within..within + n].copy_from_slice(&buf[done..done + n]);
            }
            done += n;
        }
        self.len = self.len.max(offset + buf.len() as u64);
        Ok(())
    }
}
