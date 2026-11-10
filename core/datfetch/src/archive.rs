//! The archives a data set comes in, zip and 7z: reading what one holds from its index, and
//! unpacking the members wanted as the archive's bytes are fed in.
//!
//! Nothing here reads a file. [`IndexReader`] says which byte range of the archive it wants next
//! and is fed it, until it has the archive's [`Index`]; [`Unpacker`] does the same for the
//! members' packed bytes and answers each feed with the unpacked bytes, member by member. Every
//! member's checksum is checked as it ends.
//!
//! Zip: stored or deflated members, ZIP64 included; no encryption. 7z: folders of one coder each
//! (copy, LZMA or LZMA2), with the header plain or packed by such a coder; no encryption and no
//! filter chains.

use serde::{Deserialize, Serialize};

/// How an archive is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZip,
}

impl Format {
    /// The file extension the format is known by.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::SevenZip => "7z",
        }
    }
}

/// Why an archive could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// Not an archive of the format expected, or one cut short.
    NotAnArchive(&'static str),
    /// A member packed in a way this reader does not unpack.
    Unsupported(String),
    /// A member's bytes are not what its checksum says.
    Corrupt(String),
    /// Fed bytes that are not the range asked for.
    WrongFeed,
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveError::NotAnArchive(why) => write!(f, "the archive could not be read: {why}"),
            ArchiveError::Unsupported(what) => write!(
                f,
                "the archive is packed in a way this cannot unpack: {what}"
            ),
            ArchiveError::Corrupt(name) => write!(f, "{name} in the archive is damaged"),
            ArchiveError::WrongFeed => f.write_str("the archive's bytes came out of order"),
        }
    }
}

impl std::error::Error for ArchiveError {}

fn short() -> ArchiveError {
    ArchiveError::NotAnArchive("it ends too soon")
}

/// How a run of packed bytes unpacks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Method {
    Stored,
    Deflate,
    Lzma {
        props: u8,
        dict: u32,
    },
    Lzma2 {
        dict: u32,
    },
    /// A zip member's method, or a 7z coder, that this reader does not unpack: an error only if
    /// one of its members is wanted.
    Other(String),
}

/// One run of packed bytes: a zip member, or a 7z folder holding one or more members in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stream {
    /// Where it starts in the archive: a zip member's local header, a 7z folder's packed bytes.
    start: u64,
    /// Whether a zip local header comes first, whose length is only known once it is read.
    local_header: bool,
    packed: u64,
    unpacked: u64,
    method: Method,
    /// Its members, by index into [`Index::members`], in the order they unpack.
    members: Vec<usize>,
}

/// One file in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its path in the archive, `/`-separated.
    pub name: String,
    pub size: u64,
    pub crc32: Option<u32>,
    stream: Option<usize>,
}

impl Member {
    /// The last part of its path.
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.name.rsplit(['/', '\\']).next().unwrap_or(&self.name)
    }
}

/// What an archive holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub members: Vec<Member>,
    streams: Vec<Stream>,
}

impl Index {
    /// The member whose file name (the last part of its path) is `name`, ignoring case.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<usize> {
        self.members
            .iter()
            .position(|m| m.file_name().eq_ignore_ascii_case(name))
    }
}

// ---- reading bytes -------------------------------------------------------------------------

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Cursor { b, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ArchiveError> {
        let end = self.at.checked_add(n).ok_or_else(short)?;
        let s = self.b.get(self.at..end).ok_or_else(short)?;
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, ArchiveError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ArchiveError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, ArchiveError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, ArchiveError> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    /// 7z's variable-length number: the first byte's leading ones count the bytes that follow.
    fn num(&mut self) -> Result<u64, ArchiveError> {
        let first = self.u8()?;
        let mut mask = 0x80u8;
        let mut value = 0u64;
        for i in 0..8 {
            if first & mask == 0 {
                let high = u64::from(first & mask.wrapping_sub(1));
                return Ok(value | (high << (8 * i)));
            }
            value |= u64::from(self.u8()?) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }
    fn count(&mut self) -> Result<usize, ArchiveError> {
        let n = self.num()?;
        // No count in a real header comes near this; a larger one is damage.
        usize::try_from(n)
            .ok()
            .filter(|&n| n <= 1 << 24)
            .ok_or(ArchiveError::NotAnArchive("a count is out of range"))
    }
}

fn usize_of(n: u64) -> Result<usize, ArchiveError> {
    usize::try_from(n).map_err(|_| ArchiveError::NotAnArchive("a length is out of range"))
}

// ---- the index -----------------------------------------------------------------------------

/// The most of a zip's end it reads first: its end record and the longest comment.
const ZIP_TAIL: u64 = 22 + 65_535 + 20;

#[derive(Debug)]
enum IndexState {
    ZipTail,
    ZipDirectory {
        entries: u64,
    },
    SevenStart,
    SevenHeader {
        crc: u32,
    },
    SevenPackedHeader {
        folder: Box<Folder7>,
        crc: Option<u32>,
    },
    Done,
}

/// Reads an archive's index from the byte ranges it asks for.
#[derive(Debug)]
pub struct IndexReader {
    size: u64,
    want: Option<(u64, u64)>,
    state: IndexState,
}

impl IndexReader {
    /// Read the index of an archive of `size` bytes packed as `format`.
    #[must_use]
    pub fn new(format: Format, size: u64) -> IndexReader {
        let (want, state) = match format {
            Format::Zip => {
                let len = size.min(ZIP_TAIL);
                (Some((size - len, len)), IndexState::ZipTail)
            }
            Format::SevenZip => (Some((0, 32.min(size))), IndexState::SevenStart),
        };
        IndexReader { size, want, state }
    }

    /// The range of the archive to feed next, `(offset, length)`; `None` once the index is read.
    #[must_use]
    pub fn wanted(&self) -> Option<(u64, u64)> {
        self.want
    }

    /// Feed the range [`IndexReader::wanted`] asked for. Answers the index once it is read.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Option<Index>, ArchiveError> {
        let (offset, len) = self.want.ok_or(ArchiveError::WrongFeed)?;
        if bytes.len() as u64 != len {
            return Err(short());
        }
        let state = std::mem::replace(&mut self.state, IndexState::Done);
        self.want = None;
        match state {
            IndexState::ZipTail => {
                let (dir_offset, dir_size, entries) = zip_end(bytes, offset)?;
                if dir_offset + dir_size > self.size {
                    return Err(short());
                }
                // The directory is usually in the tail already.
                if dir_offset >= offset {
                    let at = usize_of(dir_offset - offset)?;
                    let dir = bytes.get(at..at + usize_of(dir_size)?).ok_or_else(short)?;
                    return zip_directory(dir, entries).map(Some);
                }
                self.want = Some((dir_offset, dir_size));
                self.state = IndexState::ZipDirectory { entries };
                Ok(None)
            }
            IndexState::ZipDirectory { entries } => zip_directory(bytes, entries).map(Some),
            IndexState::SevenStart => {
                let mut c = Cursor::new(bytes);
                if c.take(6)? != b"7z\xBC\xAF\x27\x1C" {
                    return Err(ArchiveError::NotAnArchive("no 7z signature"));
                }
                c.take(2)?;
                let start_crc = c.u32()?;
                if crc32fast::hash(&bytes[12..32]) != start_crc {
                    return Err(ArchiveError::Corrupt("the archive's start header".into()));
                }
                let next = c.u64()?;
                let next_len = c.u64()?;
                let crc = c.u32()?;
                let at = 32u64.checked_add(next).ok_or_else(short)?;
                if next_len == 0 || at.checked_add(next_len).is_none_or(|e| e > self.size) {
                    return Err(short());
                }
                self.want = Some((at, next_len));
                self.state = IndexState::SevenHeader { crc };
                Ok(None)
            }
            IndexState::SevenHeader { crc } => {
                if crc32fast::hash(bytes) != crc {
                    return Err(ArchiveError::Corrupt("the archive's header".into()));
                }
                self.header(bytes)
            }
            IndexState::SevenPackedHeader { folder, crc } => {
                let header = unpack_whole(&folder, bytes)?;
                if crc.is_some_and(|c| crc32fast::hash(&header) != c) {
                    return Err(ArchiveError::Corrupt("the archive's header".into()));
                }
                self.header(&header)
            }
            IndexState::Done => Err(ArchiveError::WrongFeed),
        }
    }

    fn header(&mut self, bytes: &[u8]) -> Result<Option<Index>, ArchiveError> {
        let mut c = Cursor::new(bytes);
        match c.u8()? {
            0x01 => seven_header(&mut c).map(Some),
            0x17 => {
                let streams = streams_info(&mut c)?;
                let folder = streams
                    .folders
                    .into_iter()
                    .next()
                    .ok_or(ArchiveError::NotAnArchive("a packed header with no folder"))?;
                let start = 32u64.checked_add(streams.pack_pos).ok_or_else(short)?;
                let len = streams.pack_sizes.first().copied().ok_or_else(short)?;
                if start.checked_add(len).is_none_or(|e| e > self.size) {
                    return Err(short());
                }
                let crc = folder.crc;
                self.want = Some((start, len));
                self.state = IndexState::SevenPackedHeader {
                    folder: Box::new(folder),
                    crc,
                };
                Ok(None)
            }
            _ => Err(ArchiveError::NotAnArchive("not a 7z header")),
        }
    }
}

// ---- zip -----------------------------------------------------------------------------------

/// The central directory's `(offset, size, entries)`, from the archive's last bytes (`tail`,
/// starting at archive offset `tail_at`).
fn zip_end(tail: &[u8], tail_at: u64) -> Result<(u64, u64, u64), ArchiveError> {
    let pos = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or(ArchiveError::NotAnArchive("no zip end record"))?;
    let mut c = Cursor::new(&tail[pos + 4..]);
    c.take(4)?;
    let _on_disk = c.u16()?;
    let entries = c.u16()?;
    let size = c.u32()?;
    let offset = c.u32()?;
    if entries != 0xFFFF && size != 0xFFFF_FFFF && offset != 0xFFFF_FFFF {
        return Ok((u64::from(offset), u64::from(size), u64::from(entries)));
    }
    // ZIP64: a locator just before the end record points at the ZIP64 end record.
    let loc = pos
        .checked_sub(20)
        .ok_or(ArchiveError::NotAnArchive("no ZIP64 locator"))?;
    let mut c = Cursor::new(&tail[loc..]);
    if c.u32()? != 0x0706_4b50 {
        return Err(ArchiveError::NotAnArchive("no ZIP64 locator"));
    }
    c.u32()?;
    let rec = c.u64()?;
    let at = usize_of(rec.checked_sub(tail_at).ok_or(ArchiveError::Unsupported(
        "a ZIP64 end record far from the end".into(),
    ))?)?;
    let mut c = Cursor::new(tail.get(at..).ok_or_else(short)?);
    if c.u32()? != 0x0606_4b50 {
        return Err(ArchiveError::NotAnArchive("no ZIP64 end record"));
    }
    c.u64()?;
    c.take(4 + 8)?;
    let _on_disk = c.u64()?;
    let entries = c.u64()?;
    let size = c.u64()?;
    let offset = c.u64()?;
    Ok((offset, size, entries))
}

fn zip_directory(dir: &[u8], entries: u64) -> Result<Index, ArchiveError> {
    let mut c = Cursor::new(dir);
    let mut members = Vec::new();
    let mut streams = Vec::new();
    for _ in 0..entries {
        if c.u32()? != 0x0201_4b50 {
            return Err(ArchiveError::NotAnArchive("a damaged zip directory"));
        }
        c.take(4)?;
        let flags = c.u16()?;
        let method = c.u16()?;
        c.take(4)?;
        let crc = c.u32()?;
        let mut packed = u64::from(c.u32()?);
        let mut size = u64::from(c.u32()?);
        let name_len = usize::from(c.u16()?);
        let extra_len = usize::from(c.u16()?);
        let comment_len = usize::from(c.u16()?);
        c.take(8)?;
        let mut local = u64::from(c.u32()?);
        let name = String::from_utf8_lossy(c.take(name_len)?).into_owned();
        let mut extra = Cursor::new(c.take(extra_len)?);
        c.take(comment_len)?;
        // The ZIP64 extra field holds, in order, whichever of the three did not fit.
        while extra.at + 4 <= extra.b.len() {
            let id = extra.u16()?;
            let len = usize::from(extra.u16()?);
            let body = extra.take(len)?;
            if id == 0x0001 {
                let mut z = Cursor::new(body);
                if size == 0xFFFF_FFFF {
                    size = z.u64()?;
                }
                if packed == 0xFFFF_FFFF {
                    packed = z.u64()?;
                }
                if local == 0xFFFF_FFFF {
                    local = z.u64()?;
                }
            }
        }
        if name.ends_with('/') {
            continue;
        }
        let method = if flags & 1 != 0 {
            Method::Other("an encrypted member".into())
        } else {
            match method {
                0 => Method::Stored,
                8 => Method::Deflate,
                m => Method::Other(format!("zip method {m}")),
            }
        };
        let index = members.len();
        members.push(Member {
            name,
            size,
            crc32: Some(crc),
            stream: Some(streams.len()),
        });
        streams.push(Stream {
            start: local,
            local_header: true,
            packed,
            unpacked: size,
            method,
            members: vec![index],
        });
    }
    Ok(Index { members, streams })
}

// ---- 7z ------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Folder7 {
    method: Method,
    unpacked: u64,
    crc: Option<u32>,
}

#[derive(Debug, Default)]
struct StreamsInfo {
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<Folder7>,
    /// Per folder, how many members it unpacks to.
    per_folder: Vec<usize>,
    /// Per member stream (in order across folders), its size and checksum.
    sizes: Vec<u64>,
    crcs: Vec<Option<u32>>,
}

fn bits(c: &mut Cursor<'_>, n: usize) -> Result<Vec<bool>, ArchiveError> {
    let mut out = Vec::with_capacity(n);
    let mut byte = 0u8;
    for i in 0..n {
        if i % 8 == 0 {
            byte = c.u8()?;
        }
        out.push(byte & (0x80 >> (i % 8)) != 0);
    }
    Ok(out)
}

fn digests(c: &mut Cursor<'_>, n: usize) -> Result<Vec<Option<u32>>, ArchiveError> {
    let all = c.u8()?;
    let defined = if all == 0 { bits(c, n)? } else { vec![true; n] };
    defined
        .into_iter()
        .map(|d| if d { c.u32().map(Some) } else { Ok(None) })
        .collect()
}

fn method_of(id: &[u8], props: &[u8]) -> Method {
    match id {
        [0x00] => Method::Stored,
        [0x21] => match props.first() {
            Some(&d) if d <= 40 => Method::Lzma2 {
                dict: if d == 40 {
                    u32::MAX
                } else {
                    (2 | u32::from(d & 1)) << (d / 2 + 11)
                },
            },
            _ => Method::Other("LZMA2 with no dictionary size".into()),
        },
        [0x03, 0x01, 0x01] if props.len() >= 5 => Method::Lzma {
            props: props[0],
            dict: u32::from_le_bytes([props[1], props[2], props[3], props[4]]),
        },
        other => Method::Other(format!(
            "7z coder {}",
            other.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )),
    }
}

fn streams_info(c: &mut Cursor<'_>) -> Result<StreamsInfo, ArchiveError> {
    let mut s = StreamsInfo::default();
    loop {
        match c.num()? {
            0x00 => break,
            0x06 => {
                s.pack_pos = c.num()?;
                let n = c.count()?;
                loop {
                    match c.num()? {
                        0x00 => break,
                        0x09 => s.pack_sizes = (0..n).map(|_| c.num()).collect::<Result<_, _>>()?,
                        0x0A => {
                            digests(c, n)?;
                        }
                        _ => return Err(ArchiveError::NotAnArchive("a damaged 7z pack record")),
                    }
                }
            }
            0x07 => {
                if c.num()? != 0x0B {
                    return Err(ArchiveError::NotAnArchive("a damaged 7z unpack record"));
                }
                let n = c.count()?;
                if c.u8()? != 0 {
                    return Err(ArchiveError::Unsupported("folders kept elsewhere".into()));
                }
                let mut outs = Vec::with_capacity(n);
                for _ in 0..n {
                    let coders = c.count()?;
                    let mut method = None;
                    let (mut ins, mut out_streams) = (0u64, 0u64);
                    for _ in 0..coders {
                        let flag = c.u8()?;
                        let id = c.take(usize::from(flag & 0x0F))?.to_vec();
                        let (i, o) = if flag & 0x10 != 0 {
                            (c.num()?, c.num()?)
                        } else {
                            (1, 1)
                        };
                        ins += i;
                        out_streams += o;
                        let props = if flag & 0x20 != 0 {
                            let len = c.count()?;
                            c.take(len)?.to_vec()
                        } else {
                            Vec::new()
                        };
                        method = Some(method_of(&id, &props));
                    }
                    for _ in 1..out_streams {
                        c.num()?;
                        c.num()?;
                    }
                    let packed_streams = ins.saturating_sub(out_streams.saturating_sub(1));
                    if packed_streams > 1 {
                        for _ in 0..packed_streams {
                            c.num()?;
                        }
                    }
                    let method = if coders == 1 {
                        method.unwrap_or(Method::Other("no coder".into()))
                    } else {
                        Method::Other(format!("a chain of {coders} coders"))
                    };
                    outs.push((method, out_streams));
                }
                if c.num()? != 0x0C {
                    return Err(ArchiveError::NotAnArchive("a damaged 7z unpack record"));
                }
                for (method, out_streams) in outs {
                    let mut sizes = Vec::new();
                    for _ in 0..out_streams {
                        sizes.push(c.num()?);
                    }
                    // A chain's last output is its unpacked size; one coder has just one.
                    let unpacked = sizes.last().copied().unwrap_or(0);
                    s.folders.push(Folder7 {
                        method,
                        unpacked,
                        crc: None,
                    });
                }
                loop {
                    match c.num()? {
                        0x00 => break,
                        0x0A => {
                            let d = digests(c, s.folders.len())?;
                            for (f, crc) in s.folders.iter_mut().zip(d) {
                                f.crc = crc;
                            }
                        }
                        _ => return Err(ArchiveError::NotAnArchive("a damaged 7z unpack record")),
                    }
                }
            }
            0x08 => {
                s.per_folder = vec![1; s.folders.len()];
                let mut kind = c.num()?;
                if kind == 0x0D {
                    for p in &mut s.per_folder {
                        *p = c.count()?;
                    }
                    kind = c.num()?;
                }
                for (f, &n) in s.folders.iter().zip(&s.per_folder) {
                    if n == 0 {
                        continue;
                    }
                    let mut sum = 0u64;
                    if kind == 0x09 {
                        for _ in 1..n {
                            let size = c.num()?;
                            sum += size;
                            s.sizes.push(size);
                        }
                    }
                    s.sizes.push(f.unpacked.checked_sub(sum).ok_or_else(short)?);
                }
                if kind == 0x09 {
                    kind = c.num()?;
                }
                // Members of a folder that unpacks to one member take the folder's checksum.
                let missing: usize = s
                    .folders
                    .iter()
                    .zip(&s.per_folder)
                    .map(|(f, &n)| if n == 1 && f.crc.is_some() { 0 } else { n })
                    .sum();
                let mut extra = Vec::new();
                while kind != 0x00 {
                    if kind == 0x0A {
                        extra = digests(c, missing)?;
                    } else {
                        return Err(ArchiveError::NotAnArchive("a damaged 7z substreams record"));
                    }
                    kind = c.num()?;
                }
                let mut extra = extra.into_iter();
                for (f, &n) in s.folders.iter().zip(&s.per_folder) {
                    if n == 1 && f.crc.is_some() {
                        s.crcs.push(f.crc);
                    } else {
                        for _ in 0..n {
                            s.crcs.push(extra.next().flatten());
                        }
                    }
                }
            }
            _ => return Err(ArchiveError::NotAnArchive("a damaged 7z streams record")),
        }
    }
    if s.per_folder.is_empty() {
        s.per_folder = vec![1; s.folders.len()];
        s.sizes = s.folders.iter().map(|f| f.unpacked).collect();
        s.crcs = s.folders.iter().map(|f| f.crc).collect();
    }
    if s.pack_sizes.len() < s.folders.len() {
        return Err(ArchiveError::Unsupported(
            "folders sharing packed streams".into(),
        ));
    }
    Ok(s)
}

fn seven_header(c: &mut Cursor<'_>) -> Result<Index, ArchiveError> {
    let mut streams = StreamsInfo::default();
    let mut names = Vec::new();
    let mut empty_stream = Vec::new();
    let mut files = 0usize;
    loop {
        match c.num()? {
            0x00 => break,
            0x02 => {
                // Archive properties: (type, size, bytes) until a zero type.
                loop {
                    if c.num()? == 0 {
                        break;
                    }
                    let len = c.count()?;
                    c.take(len)?;
                }
            }
            0x03 => {
                streams_info(c)?;
            }
            0x04 => streams = streams_info(c)?,
            0x05 => {
                files = c.count()?;
                loop {
                    let kind = c.num()?;
                    if kind == 0 {
                        break;
                    }
                    let len = c.count()?;
                    let body = c.take(len)?;
                    let mut b = Cursor::new(body);
                    match kind {
                        0x0E => empty_stream = bits(&mut b, files)?,
                        0x11 => {
                            if b.u8()? != 0 {
                                return Err(ArchiveError::Unsupported(
                                    "names kept elsewhere".into(),
                                ));
                            }
                            let units: Vec<u16> = body[1..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|p| u16::from_le_bytes(*p))
                                .collect();
                            names = units
                                .split(|&u| u == 0)
                                .take(files)
                                .map(String::from_utf16_lossy)
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
            _ => return Err(ArchiveError::NotAnArchive("a damaged 7z header")),
        }
    }
    let mut index = Index {
        members: Vec::new(),
        streams: Vec::new(),
    };
    let mut pack_at = 32u64.checked_add(streams.pack_pos).ok_or_else(short)?;
    for (i, f) in streams.folders.iter().enumerate() {
        let packed = streams.pack_sizes[i];
        index.streams.push(Stream {
            start: pack_at,
            local_header: false,
            packed,
            unpacked: f.unpacked,
            method: f.method.clone(),
            members: Vec::new(),
        });
        pack_at = pack_at.checked_add(packed).ok_or_else(short)?;
    }
    // Members with a stream take the streams in order, folder by folder.
    let mut folder = 0usize;
    let mut left_in_folder = streams.per_folder.first().copied().unwrap_or(0);
    let mut stream_index = 0usize;
    for i in 0..files {
        let name = names.get(i).cloned().unwrap_or_default();
        if empty_stream.get(i).copied().unwrap_or(false) {
            index.members.push(Member {
                name,
                size: 0,
                crc32: None,
                stream: None,
            });
            continue;
        }
        while left_in_folder == 0 {
            folder += 1;
            left_in_folder = *streams
                .per_folder
                .get(folder)
                .ok_or(ArchiveError::NotAnArchive("more files than streams"))?;
        }
        let size = *streams.sizes.get(stream_index).ok_or_else(short)?;
        let crc32 = streams.crcs.get(stream_index).copied().flatten();
        let m = index.members.len();
        index.members.push(Member {
            name,
            size,
            crc32,
            stream: Some(folder),
        });
        index.streams[folder].members.push(m);
        stream_index += 1;
        left_in_folder -= 1;
    }
    Ok(index)
}

/// Unpack all of one small folder held in memory (a packed header).
fn unpack_whole(folder: &Folder7, packed: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    let mut d = Decoder::new(&folder.method, folder.unpacked)
        .map_err(|_| ArchiveError::Unsupported("a packed header".into()))?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut input = packed;
    loop {
        let (used, made, ended) = d.run(input, &mut buf, true)?;
        input = &input[used..];
        out.extend_from_slice(&buf[..made]);
        if out.len() as u64 > folder.unpacked {
            return Err(ArchiveError::Corrupt("the archive's header".into()));
        }
        if ended || (used == 0 && made == 0) {
            break;
        }
    }
    if out.len() as u64 != folder.unpacked {
        return Err(ArchiveError::Corrupt("the archive's header".into()));
    }
    Ok(out)
}

// ---- the decoders --------------------------------------------------------------------------

enum Decoder {
    Stored { left: u64 },
    Deflate(Box<flate2::Decompress>),
    Lzma(Box<lzma_rust2::LzmaStream>),
    Lzma2(Box<lzma_rust2::Lzma2Stream>),
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Decoder::Stored { .. } => "Stored",
            Decoder::Deflate(_) => "Deflate",
            Decoder::Lzma(_) => "Lzma",
            Decoder::Lzma2(_) => "Lzma2",
        })
    }
}

impl Decoder {
    fn new(method: &Method, unpacked: u64) -> Result<Decoder, ArchiveError> {
        Ok(match method {
            Method::Stored => Decoder::Stored { left: unpacked },
            Method::Deflate => Decoder::Deflate(Box::new(flate2::Decompress::new(false))),
            Method::Lzma { props, dict } => Decoder::Lzma(Box::new(
                lzma_rust2::LzmaStream::new_with_props(unpacked, *props, *dict, None)
                    .map_err(|e| ArchiveError::Unsupported(format!("LZMA: {e}")))?,
            )),
            Method::Lzma2 { dict } => Decoder::Lzma2(Box::new(lzma_rust2::Lzma2Stream::new(*dict))),
            Method::Other(what) => return Err(ArchiveError::Unsupported(what.clone())),
        })
    }

    /// Unpack from `input` into `output`: `(bytes used, bytes made, whether the stream ended)`.
    fn run(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        finish: bool,
    ) -> Result<(usize, usize, bool), ArchiveError> {
        let bad = |e: String| ArchiveError::Corrupt(format!("a packed stream ({e})"));
        match self {
            Decoder::Stored { left } => {
                let n = input
                    .len()
                    .min(output.len())
                    .min(usize::try_from(*left).unwrap_or(usize::MAX));
                output[..n].copy_from_slice(&input[..n]);
                *left -= n as u64;
                Ok((n, n, *left == 0))
            }
            Decoder::Deflate(d) => {
                let (i0, o0) = (d.total_in(), d.total_out());
                let status = d
                    .decompress(input, output, flate2::FlushDecompress::None)
                    .map_err(|e| bad(e.to_string()))?;
                let used = usize_of(d.total_in() - i0)?;
                let made = usize_of(d.total_out() - o0)?;
                Ok((used, made, status == flate2::Status::StreamEnd))
            }
            Decoder::Lzma(d) => {
                let action = if finish {
                    lzma_rust2::Action::Finish
                } else {
                    lzma_rust2::Action::Run
                };
                let r = d
                    .process(input, output, action)
                    .map_err(|e| bad(e.to_string()))?;
                Ok((
                    r.bytes_consumed,
                    r.bytes_produced,
                    r.status == lzma_rust2::Status::StreamEnd,
                ))
            }
            Decoder::Lzma2(d) => {
                let action = if finish {
                    lzma_rust2::Action::Finish
                } else {
                    lzma_rust2::Action::Run
                };
                let r = d
                    .process(input, output, action)
                    .map_err(|e| bad(e.to_string()))?;
                Ok((
                    r.bytes_consumed,
                    r.bytes_produced,
                    r.status == lzma_rust2::Status::StreamEnd,
                ))
            }
        }
    }
}

// ---- unpacking -----------------------------------------------------------------------------

/// Some unpacked bytes of one member: `bytes` go at `offset` of member `member`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub member: usize,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// The stream being unpacked.
#[derive(Debug)]
struct Active {
    stream: usize,
    /// The archive offset to feed next.
    pos: u64,
    /// Where its packed bytes end; unknown until a zip local header is read.
    end: Option<u64>,
    decoder: Decoder,
    /// How far into the stream's unpacked bytes it is.
    out: u64,
    /// The member being unpacked, by place in the stream's list, and how far into it.
    member: usize,
    in_member: u64,
    crc: crc32fast::Hasher,
    ended: bool,
}

/// The size of the reads [`Unpacker::next_read`] asks for.
pub const READ_SIZE: u64 = 4 * 1024 * 1024;

/// Unpacks the wanted members of an archive from its packed bytes, fed as it asks for them.
#[derive(Debug)]
pub struct Unpacker {
    index: Index,
    wanted: Vec<bool>,
    /// The streams still to unpack, in the order they lie in the archive.
    todo: std::collections::VecDeque<usize>,
    active: Option<Active>,
    /// The unpacked bytes of all wanted streams, and how many are done: for progress.
    total: u64,
    done: u64,
}

impl Unpacker {
    /// Unpack the members `wanted` (indices into the index's members) of the archive `index`
    /// describes. Fails at once if a wanted member is packed in a way this cannot unpack.
    pub fn new(index: Index, wanted: &[usize]) -> Result<Unpacker, ArchiveError> {
        let mut w = vec![false; index.members.len()];
        let mut streams = Vec::new();
        for &m in wanted {
            let member = index.members.get(m).ok_or(ArchiveError::WrongFeed)?;
            w[m] = true;
            if let Some(s) = member.stream {
                if let Method::Other(what) = &index.streams[s].method {
                    return Err(ArchiveError::Unsupported(format!(
                        "{}: {what}",
                        member.name
                    )));
                }
                if !streams.contains(&s) {
                    streams.push(s);
                }
            }
        }
        streams.sort_by_key(|&s| index.streams[s].start);
        let total = streams.iter().map(|&s| index.streams[s].unpacked).sum();
        Ok(Unpacker {
            index,
            wanted: w,
            todo: streams.into(),
            active: None,
            total,
            done: 0,
        })
    }

    /// The archive's index.
    #[must_use]
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// `(unpacked so far, to unpack in all)`, in bytes.
    #[must_use]
    pub fn progress(&self) -> (u64, u64) {
        (self.done, self.total)
    }

    fn start_next(&mut self) -> Result<(), ArchiveError> {
        while self.active.is_none() {
            let Some(s) = self.todo.pop_front() else {
                return Ok(());
            };
            let stream = &self.index.streams[s];
            let decoder = Decoder::new(&stream.method, stream.unpacked)?;
            self.active = Some(Active {
                stream: s,
                pos: stream.start,
                end: (!stream.local_header).then_some(stream.start + stream.packed),
                decoder,
                out: 0,
                member: 0,
                in_member: 0,
                crc: crc32fast::Hasher::new(),
                ended: false,
            });
        }
        Ok(())
    }

    /// The range of the archive to feed next, `(offset, length)`; `None` when every wanted member
    /// is unpacked.
    pub fn next_read(&mut self) -> Result<Option<(u64, u64)>, ArchiveError> {
        self.start_next()?;
        Ok(self.active.as_ref().map(|a| match a.end {
            None => (a.pos, 30),
            Some(end) => (a.pos, (end - a.pos).min(READ_SIZE)),
        }))
    }

    /// Feed the range [`Unpacker::next_read`] asked for: answers the wanted members' bytes it
    /// unpacked to.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Piece>, ArchiveError> {
        let Some((at, len)) = self.next_read()? else {
            return Err(ArchiveError::WrongFeed);
        };
        if bytes.len() as u64 != len {
            return Err(short());
        }
        let Some(a) = self.active.as_mut() else {
            return Err(ArchiveError::WrongFeed);
        };
        debug_assert_eq!(a.pos, at);
        if a.end.is_none() {
            // A zip member's local header: its name and extra field come before the data.
            let mut c = Cursor::new(bytes);
            if c.u32()? != 0x0403_4b50 {
                return Err(ArchiveError::NotAnArchive("a damaged zip member header"));
            }
            c.take(22)?;
            let skip = u64::from(c.u16()?) + u64::from(c.u16()?);
            let start = a.pos + 30 + skip;
            a.pos = start;
            a.end = Some(start + self.index.streams[a.stream].packed);
            return Ok(Vec::new());
        }
        a.pos += len;
        let last = Some(a.pos) == a.end;
        let mut pieces = Vec::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut input = bytes;
        loop {
            let a = self.active.as_mut().ok_or(ArchiveError::WrongFeed)?;
            if a.ended {
                break;
            }
            let (used, made, ended) = a.decoder.run(input, &mut buf, last)?;
            input = &input[used..];
            a.ended = ended;
            if made > 0 {
                self.take_out(&buf[..made], &mut pieces)?;
            }
            if made == 0 && (used == 0 || input.is_empty()) && !ended {
                break;
            }
        }
        let a = self.active.as_ref().ok_or(ArchiveError::WrongFeed)?;
        if last || a.ended {
            self.finish_stream()?;
        }
        Ok(pieces)
    }

    /// Hand out unpacked bytes to the members they belong to, checking each as it ends.
    fn take_out(&mut self, mut data: &[u8], pieces: &mut Vec<Piece>) -> Result<(), ArchiveError> {
        self.done += data.len() as u64;
        while !data.is_empty() {
            let a = self.active.as_mut().ok_or(ArchiveError::WrongFeed)?;
            let stream = &self.index.streams[a.stream];
            let Some(&m) = stream.members.get(a.member) else {
                return Err(ArchiveError::Corrupt(
                    "a packed stream (it runs long)".into(),
                ));
            };
            let member = &self.index.members[m];
            let left = member.size - a.in_member;
            let take = usize::try_from(left).unwrap_or(usize::MAX).min(data.len());
            let part = &data[..take];
            a.crc.update(part);
            if self.wanted[m] {
                match pieces.last_mut() {
                    Some(p)
                        if p.member == m
                            && p.offset + p.bytes.len() as u64 == a.in_member
                            && p.bytes.len() < 4 << 20 =>
                    {
                        p.bytes.extend_from_slice(part);
                    }
                    _ => pieces.push(Piece {
                        member: m,
                        offset: a.in_member,
                        bytes: part.to_vec(),
                    }),
                }
            }
            a.in_member += take as u64;
            a.out += take as u64;
            data = &data[take..];
            if a.in_member == member.size {
                let crc = std::mem::take(&mut a.crc).finalize();
                if member.crc32.is_some_and(|c| c != crc) {
                    return Err(ArchiveError::Corrupt(member.name.clone()));
                }
                a.member += 1;
                a.in_member = 0;
                // Members of no length end where they start.
                self.skip_empty();
            }
        }
        Ok(())
    }

    fn skip_empty(&mut self) {
        if let Some(a) = self.active.as_mut() {
            let stream = &self.index.streams[a.stream];
            while let Some(&m) = stream.members.get(a.member) {
                if self.index.members[m].size != 0 {
                    break;
                }
                a.member += 1;
            }
        }
    }

    fn finish_stream(&mut self) -> Result<(), ArchiveError> {
        self.skip_empty();
        let a = self.active.take().ok_or(ArchiveError::WrongFeed)?;
        let stream = &self.index.streams[a.stream];
        if a.out != stream.unpacked || a.member < stream.members.len() {
            let name = stream
                .members
                .get(a.member.min(stream.members.len().saturating_sub(1)))
                .map_or_else(
                    || "a packed stream".to_owned(),
                    |&m| self.index.members[m].name.clone(),
                );
            return Err(ArchiveError::Corrupt(name));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (reading zip and 7z archives, as their formats define them).
    use super::*;
    use std::io::Write as _;

    /// Feed an index reader from `archive` until it has the index.
    pub(crate) fn read_index(format: Format, archive: &[u8]) -> Result<Index, ArchiveError> {
        let mut r = IndexReader::new(format, archive.len() as u64);
        loop {
            let (at, len) = r.wanted().expect("a range wanted");
            let (at, len) = (usize::try_from(at).unwrap(), usize::try_from(len).unwrap());
            if let Some(index) = r.feed(&archive[at..at + len])? {
                return Ok(index);
            }
        }
    }

    /// Unpack `names` from `archive` through the unpacker: each name's bytes.
    pub(crate) fn unpack(
        format: Format,
        archive: &[u8],
        names: &[&str],
    ) -> Result<Vec<Vec<u8>>, ArchiveError> {
        let index = read_index(format, archive)?;
        let wanted: Vec<usize> = names
            .iter()
            .map(|n| index.find(n).expect("member"))
            .collect();
        let mut out: Vec<Vec<u8>> = wanted
            .iter()
            .map(|&m| vec![0; usize::try_from(index.members[m].size).unwrap()])
            .collect();
        let mut u = Unpacker::new(index, &wanted)?;
        while let Some((at, len)) = u.next_read()? {
            let (at, len) = (usize::try_from(at).unwrap(), usize::try_from(len).unwrap());
            for p in u.feed(&archive[at..at + len])? {
                let slot = wanted
                    .iter()
                    .position(|&w| w == p.member)
                    .expect("a wanted member");
                let o = usize::try_from(p.offset).unwrap();
                out[slot][o..o + p.bytes.len()].copy_from_slice(&p.bytes);
            }
        }
        Ok(out)
    }

    /// Bytes that compress, but not to nothing.
    fn content(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        (0..len)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                if i % 3 == 0 {
                    x.to_le_bytes()[0]
                } else {
                    (i / 64).to_le_bytes()[0]
                }
            })
            .collect()
    }

    fn zip_of(files: &[(&str, &[u8], bool)], zip64: bool) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes, deflate) in files {
            let method = if *deflate {
                zip::CompressionMethod::Deflated
            } else {
                zip::CompressionMethod::Stored
            };
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(method)
                .large_file(zip64);
            w.start_file(*name, opts).unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// A zip's wanted members unpack to their bytes, stored and deflated alike, and an unwanted
    /// one is passed over.
    #[test]
    fn unpacks_wanted_zip_members() {
        let a = content(3_000_000, 1);
        let b = content(70_000, 2);
        let c = content(5_000, 3);
        let zip = zip_of(
            &[
                ("data/a.dat", &a, true),
                ("skip.exe", &c, true),
                ("b.dat", &b, false),
            ],
            false,
        );
        let index = read_index(Format::Zip, &zip).unwrap();
        assert_eq!(index.members.len(), 3);
        assert_eq!(index.find("A.DAT"), Some(0));
        let out = unpack(Format::Zip, &zip, &["a.dat", "b.dat"]).unwrap();
        assert_eq!(out[0], a);
        assert_eq!(out[1], b);
    }

    /// A ZIP64 archive's directory is found through its ZIP64 end record.
    #[test]
    fn reads_a_zip64_directory() {
        let a = content(100_000, 4);
        let zip = zip_of(&[("a.dat", &a, true)], true);
        let out = unpack(Format::Zip, &zip, &["a.dat"]).unwrap();
        assert_eq!(out[0], a);
    }

    /// A zip whose directory lies before the last 64 KiB is read with a second range.
    #[test]
    fn reads_a_zip_directory_outside_the_tail() {
        let names: Vec<String> = (0..2_000)
            .map(|i| format!("folder/a-rather-long-file-name-{i:05}.txt"))
            .collect();
        let files: Vec<(&str, &[u8], bool)> = names
            .iter()
            .map(|n| (n.as_str(), &b"x"[..], false))
            .collect();
        let zip = zip_of(&files, false);
        let index = read_index(Format::Zip, &zip).unwrap();
        assert_eq!(index.members.len(), 2_000);
    }

    /// A damaged member is caught by its checksum.
    #[test]
    fn a_damaged_zip_member_fails_its_checksum() {
        let a = content(50_000, 5);
        let mut zip = zip_of(&[("a.dat", &a, false)], false);
        zip[30 + 5 + 100] ^= 0x40;
        assert_eq!(
            unpack(Format::Zip, &zip, &["a.dat"]),
            Err(ArchiveError::Corrupt("a.dat".into()))
        );
    }

    fn seven_of(
        files: &[(&str, &[u8])],
        method: sevenz_rust2::EncoderMethod,
        solid: bool,
    ) -> Vec<u8> {
        let mut w = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
        w.set_content_methods(vec![method.into()]);
        let entries: Vec<_> = files
            .iter()
            .map(|(name, bytes)| {
                (
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    std::io::Cursor::new(bytes.to_vec()),
                )
            })
            .collect();
        if solid {
            let (e, r): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
            w.push_archive_entries(e, r.into_iter().map(Into::into).collect())
                .unwrap();
        } else {
            for (e, r) in entries {
                w.push_archive_entry(e, Some(r)).unwrap();
            }
        }
        w.finish().unwrap().into_inner()
    }

    /// Whether a 7z's header is packed (rather than plain).
    fn header_packed(archive: &[u8]) -> bool {
        let next = u64::from_le_bytes(archive[12..20].try_into().unwrap());
        archive[usize::try_from(32 + next).unwrap()] == 0x17
    }

    /// A 7z of LZMA2, solid (one folder, members in turn) and not, with its header plain (few
    /// files) or packed (many), unpacks the wanted members.
    #[test]
    fn unpacks_seven_zip_members() {
        let a = content(2_500_000, 6);
        let b = content(80_000, 7);
        let c = content(9_000, 8);
        let names: Vec<String> = (0..60)
            .map(|i| format!("padding/a-long-name-to-make-the-header-compress-{i:03}.txt"))
            .collect();
        for solid in [true, false] {
            for many in [false, true] {
                let mut files: Vec<(&str, &[u8])> =
                    vec![("a.dat", &a), ("c.exe", &c), ("b.dat", &b)];
                if many {
                    files.extend(names.iter().map(|n| (n.as_str(), &b"pad"[..])));
                }
                let archive = seven_of(&files, sevenz_rust2::EncoderMethod::LZMA2, solid);
                assert_eq!(header_packed(&archive), many, "solid {solid}");
                let out = unpack(Format::SevenZip, &archive, &["b.dat", "a.dat"]).unwrap();
                assert_eq!(out[0], b, "solid {solid}, many {many}");
                assert_eq!(out[1], a, "solid {solid}, many {many}");
            }
        }
    }

    /// LZMA (not LZMA2) folders unpack too.
    #[test]
    fn unpacks_seven_zip_lzma() {
        let a = content(300_000, 9);
        let archive = seven_of(&[("a.dat", &a)], sevenz_rust2::EncoderMethod::LZMA, false);
        assert_eq!(
            unpack(Format::SevenZip, &archive, &["a.dat"]).unwrap()[0],
            a
        );
    }

    /// A damaged 7z member is caught: by the decoder or by its checksum.
    #[test]
    fn a_damaged_seven_zip_member_is_caught() {
        let a = content(200_000, 10);
        let mut archive = seven_of(&[("a.dat", &a)], sevenz_rust2::EncoderMethod::LZMA2, false);
        archive[32 + 5_000] ^= 0x10;
        assert!(matches!(
            unpack(Format::SevenZip, &archive, &["a.dat"]),
            Err(ArchiveError::Corrupt(_))
        ));
    }

    /// Something that is not an archive is said to be one of neither format.
    #[test]
    fn refuses_what_is_not_an_archive() {
        let junk = vec![7u8; 5_000];
        assert!(matches!(
            read_index(Format::Zip, &junk),
            Err(ArchiveError::NotAnArchive(_))
        ));
        assert!(matches!(
            read_index(Format::SevenZip, &junk),
            Err(ArchiveError::NotAnArchive(_))
        ));
    }

    /// The 7z variable-length number reads each of its widths.
    #[test]
    fn reads_seven_zip_numbers() {
        for (bytes, want) in [
            (&[0x05][..], 5u64),
            (&[0x80, 0xFF][..], 0xFF),
            (&[0xA1, 0x34][..], 0x2134),
            (&[0xC1, 0x34, 0x12][..], 0x01_1234),
            (&[0xFF, 1, 2, 3, 4, 5, 6, 7, 8][..], 0x0807_0605_0403_0201),
        ] {
            assert_eq!(Cursor::new(bytes).num().unwrap(), want, "{bytes:?}");
        }
    }
}
