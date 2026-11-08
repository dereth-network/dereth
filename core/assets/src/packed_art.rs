//! The contents of an art pack packed into one file: what the pack is, and every record it holds.
//!
//! **The file** is a container in the later layout (`docs/formats/01-dat-container.md`), its
//! blocks [`BLOCK`] bytes and its data subset [`SUBSET`]. Record 1 is its contents; every other
//! record, numbered from 2 in the contents' order, is one of the pack's files, byte for byte, or
//! the pictures made from them. Numbers are little-endian, and a string is a 32-bit length and
//! its UTF-8.
//!
//! **The contents**: `DAPK`, the layout ([`LAYOUT`]), the manifest format the pack was packed for
//! ([`crate::art_pack::FORMAT`]), the pack's name, and its hash: the hash of the folder it was
//! packed from, which names the pack in reports however it is opened. Then a count and, for each
//! record, its kind (0 a file, 1 pictures), its key, its id (0 for an empty file, which has no
//! record), its length and the SHA-256 of its bytes; then the SHA-256 of all that.
//!
//! The pictures themselves are made and read by whatever draws the pack. This module reads the
//! contents, and the manifest a packed file holds ([`read_manifest`]), for what has to know what a
//! packed file is without drawing it: a launcher checking a download, a tool listing packs.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_dat::overlay::record_hash;
use dereth_dat::{DatFile, DatStorageMut, DatWriter, MemoryFile};
use dereth_primitives::DataId;

use crate::art_pack::{FORMAT, MANIFEST_FILE};

/// The layout of packed pack this module writes and reads.
pub const LAYOUT: u32 = 1;

/// The container's block size: large, since a pack's records are mostly pictures of megabytes,
/// each read a block at a time.
pub const BLOCK: u32 = 0x4000;

/// The data subset a packed pack's header carries: `Pack`.
pub const SUBSET: u32 = u32::from_le_bytes(*b"Pack");

/// The contents' magic: `DAPK`.
const MAGIC: u32 = u32::from_le_bytes(*b"DAPK");

/// The contents' record.
pub const CONTENTS: DataId = DataId(1);

/// The first record after the contents.
pub const FIRST: u32 = 2;

/// What a record of a packed pack holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// One of the pack's files, byte for byte.
    File,
    /// Pictures made from the pack's files.
    Pictures,
}

/// One record a packed pack's contents name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// What it holds.
    pub kind: Kind,
    /// A file's path, or how its pictures are made.
    pub key: String,
    /// Its id in the container; 0 for an empty file, which has no record.
    pub id: DataId,
    /// Its length.
    pub size: u32,
    /// The SHA-256 of its bytes.
    pub sha256: [u8; 32],
}

/// A packed pack's contents: what the pack is and every record it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    /// The manifest format the pack was packed for.
    pub format: u32,
    /// The pack's name.
    pub name: String,
    /// The hash of the folder it was packed from.
    pub hash: String,
    /// Its records, in their ids' order.
    pub records: Vec<Record>,
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_text(out: &mut Vec<u8>, s: &str) {
    put(out, u32::try_from(s.len()).unwrap_or(u32::MAX));
    out.extend_from_slice(s.as_bytes());
}

/// Bytes read from the front.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (front, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(front)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn text(&mut self) -> Option<String> {
        let n = usize::try_from(self.u32()?).ok()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

impl Contents {
    /// The contents' record.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, MAGIC);
        put(&mut out, LAYOUT);
        put(&mut out, self.format);
        put_text(&mut out, &self.name);
        put_text(&mut out, &self.hash);
        put(
            &mut out,
            u32::try_from(self.records.len()).unwrap_or(u32::MAX),
        );
        for r in &self.records {
            out.push(match r.kind {
                Kind::File => 0,
                Kind::Pictures => 1,
            });
            put_text(&mut out, &r.key);
            put(&mut out, r.id.raw());
            put(&mut out, r.size);
            out.extend_from_slice(&r.sha256);
        }
        let sum = record_hash(&out);
        out.extend_from_slice(&sum);
        out
    }

    /// The contents' record, read: the reason, as a sentence, when it is not a packed pack's, or
    /// one of another layout or manifest format, or damaged.
    ///
    /// # Errors
    /// As above.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut b = Bytes(bytes);
        if b.u32() != Some(MAGIC) {
            return Err("not a packed art pack".into());
        }
        match b.u32() {
            Some(LAYOUT) => {}
            Some(n) => {
                return Err(format!(
                    "packed in layout {n}, and this client reads layout {LAYOUT}; pack it again"
                ))
            }
            None => return Err("its contents are damaged; pack it again".into()),
        }
        let damaged = || "its contents are damaged; pack it again".to_owned();
        let (body, sum) = bytes.split_at(bytes.len().checked_sub(32).ok_or_else(damaged)?);
        if record_hash(body)[..] != sum[..] {
            return Err(damaged());
        }
        let mut b = Bytes(body.get(8..).ok_or_else(damaged)?);
        let format = b.u32().ok_or_else(damaged)?;
        if i64::from(format) != FORMAT {
            return Err(format!(
                "packed from manifest format {format}, and this client reads format {FORMAT}; \
                 pack it again"
            ));
        }
        let read = |b: &mut Bytes<'_>| -> Option<Self> {
            let name = b.text()?;
            let hash = b.text()?;
            let count = b.u32()?;
            let mut records = Vec::new();
            for _ in 0..count {
                let kind = match b.take(1)?[0] {
                    0 => Kind::File,
                    1 => Kind::Pictures,
                    _ => return None,
                };
                let key = b.text()?;
                let id = DataId(b.u32()?);
                let size = b.u32()?;
                let sha256 = b.take(32)?.try_into().ok()?;
                records.push(Record {
                    kind,
                    key,
                    id,
                    size,
                    sha256,
                });
            }
            b.0.is_empty().then_some(Self {
                format,
                name,
                hash,
                records,
            })
        };
        read(&mut b).ok_or_else(damaged)
    }
}

/// What a packed file holds, read without its pictures: its contents and its manifest's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packed {
    pub contents: Contents,
    pub manifest: String,
}

/// The contents and the manifest of the packed pack in `file`.
///
/// # Errors
/// The file is not a packed pack, was packed in another layout or for another manifest format,
/// or its contents or its manifest are damaged or missing, as a sentence.
pub fn read_manifest_of(file: &DatFile) -> Result<Packed, String> {
    if file.header().data_subset != SUBSET {
        return Err("not a packed art pack".into());
    }
    let contents = match file.read(CONTENTS) {
        Ok(bytes) => Contents::decode(&bytes)?,
        Err(dereth_dat::DatError::NotFound(_)) => return Err("not a packed art pack".into()),
        Err(e) => return Err(format!("its contents will not read ({e}); pack it again")),
    };
    let record = contents
        .records
        .iter()
        .find(|r| r.kind == Kind::File && r.key == MANIFEST_FILE)
        .ok_or("it holds no manifest; pack it again")?;
    let bytes = file
        .read(record.id)
        .map_err(|e| format!("its manifest will not read ({e}); pack it again"))?;
    if record_hash(&bytes)[..] != record.sha256[..] {
        return Err("its manifest is damaged; pack it again".into());
    }
    let manifest =
        String::from_utf8(bytes).map_err(|_| "its manifest is not text; pack it again")?;
    Ok(Packed { contents, manifest })
}

/// [`read_manifest_of`] the packed pack in the file at `path`.
///
/// # Errors
/// The file will not open, or as [`read_manifest_of`].
pub fn read_manifest(path: &Path) -> Result<Packed, String> {
    let file = DatFile::open(path).map_err(|e| match e {
        dereth_dat::DatError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
            "no such file".to_owned()
        }
        e => format!("not a packed art pack ({e})"),
    })?;
    read_manifest_of(&file)
}

/// The largest packed pack: a container's blocks lie below 2 GiB, where the high bit of a link
/// marks a free block.
pub const LARGEST: u64 = 0x8000_0000;

/// The blocks the writer keeps free beyond what the record it writes needs.
pub const RESERVE: u64 = 0x34;

/// The packed file holding `contents`' records, each record's bytes from `bytes_of`: the records
/// are numbered from [`FIRST`] in their order (an empty one gets 0, and no record), and each gets
/// its bytes' SHA-256. Answers the file's bytes and how many records it holds, the contents among
/// them.
///
/// # Errors
/// It would be larger than [`LARGEST`], or a record will not write.
pub fn write<'a>(
    mut contents: Contents,
    bytes_of: &mut dyn FnMut(&Record) -> Cow<'a, [u8]>,
) -> Result<(Vec<u8>, usize), String> {
    let mut next = FIRST;
    for r in &mut contents.records {
        r.id = if r.size == 0 {
            DataId(0)
        } else {
            next += 1;
            DataId(next - 1)
        };
    }
    // The container exactly as large as the records need: a chain each, the contents', the
    // directory's nodes as it grows, and the room the writer keeps in hand.
    let per = u64::from(BLOCK - 4);
    let blocks = |len: usize| (len as u64).div_ceil(per);
    let records = (next - FIRST) as usize + 1;
    let nodes = blocks(dereth_dat::btree::NODE_SIZE) * (1 + 2 * (records as u64).div_ceil(30));
    let total = contents
        .records
        .iter()
        .map(|r| blocks(r.size as usize))
        .sum::<u64>()
        + blocks(contents.encode().len())
        + nodes
        + RESERVE
        + 2;
    let size = 0x400 + u64::from(BLOCK) * total;
    let too_big = |size: u64| {
        format!(
            "the pack takes {} MB packed, and one file holds {} MB",
            size >> 20,
            LARGEST >> 20
        )
    };
    if size > LARGEST {
        return Err(too_big(size));
    }
    let size = u32::try_from(size).map_err(|_| too_big(size))?;
    let storage = MemoryFile::default();
    // Its length set once, before the writer fills it.
    storage
        .clone()
        .write_all_at(u64::from(size) - 1, &[0])
        .map_err(|e| e.to_string())?;
    let fail = |e: dereth_dat::DatError| e.to_string();
    let mut w = DatWriter::create_in(
        PathBuf::from(&contents.name),
        Box::new(storage.clone()),
        BLOCK,
        dereth_dat::container::PORTAL_DATFILE,
        SUBSET,
        size,
    )
    .map_err(fail)?;
    for i in 0..contents.records.len() {
        if contents.records[i].size == 0 {
            continue;
        }
        let bytes = bytes_of(&contents.records[i]);
        let r = &mut contents.records[i];
        r.sha256 = record_hash(&bytes);
        w.save(r.id, &bytes, 1, 0, 0)
            .map_err(|e| format!("{}: {e}", r.key))?;
    }
    w.save(CONTENTS, &contents.encode(), 1, 0, 0)
        .map_err(fail)?;
    // Grown past what was reckoned, it may not reach past what one file holds either.
    if u64::from(w.header().file_size) > LARGEST {
        return Err(too_big(u64::from(w.header().file_size)));
    }
    drop(w);
    Ok((storage.bytes(), records))
}

/// A packed file holding `files` (by path, the manifest among them) and no pictures, named
/// `name` with hash `hash`: what reads a packed pack's manifest and files reads it as any other.
///
/// # Errors
/// As [`write`](fn@write), or a file larger than a record holds.
pub fn pack_files(
    name: &str,
    hash: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let mut contents = Contents {
        format: u32::try_from(FORMAT).unwrap_or(u32::MAX),
        name: name.to_owned(),
        hash: hash.to_owned(),
        records: Vec::with_capacity(files.len()),
    };
    for (key, bytes) in files {
        contents.records.push(Record {
            kind: Kind::File,
            key: key.clone(),
            id: DataId(0),
            size: u32::try_from(bytes.len())
                .map_err(|_| format!("{key} is larger than one record holds"))?,
            sha256: [0; 32],
        });
    }
    write(contents, &mut |r| Cow::Borrowed(&files[&r.key][..])).map(|(bytes, _)| bytes)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (art packs packed into one file)

    use super::*;

    #[test]
    fn a_packed_files_manifest_reads_back_with_its_name_and_hash() {
        let manifest = "[pack]\nname = \"Houses\"\nformat = 1\nid = \"com.example.houses\"\n";
        let files = BTreeMap::from([
            (MANIFEST_FILE.to_owned(), manifest.as_bytes().to_vec()),
            ("empty.txt".to_owned(), Vec::new()),
            ("house.gltf".to_owned(), b"{}".to_vec()),
        ]);
        let bytes = pack_files("Houses", "c0ffee", &files).expect("packs");
        let dir = std::env::temp_dir().join(format!("dereth-packed-art-a-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("houses.dat");
        std::fs::write(&path, bytes).unwrap();
        let read = read_manifest(&path).expect("reads");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read.manifest, manifest);
        assert_eq!(read.contents.name, "Houses");
        assert_eq!(read.contents.hash, "c0ffee");
        assert_eq!(read.contents.records.len(), 3);
    }

    #[test]
    fn a_file_that_is_not_a_packed_pack_is_refused() {
        let dir = std::env::temp_dir().join(format!("dereth-packed-art-b-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.dat");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert!(read_manifest(&path).is_err());
        assert!(read_manifest(&dir.join("missing.dat"))
            .unwrap_err()
            .contains("no such file"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
