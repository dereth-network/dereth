//! Every way reading a `world.pack` or importing the SQL dump can fail.
//!
//! Not ACE-derived: the pack container is our own format (carried from the v1 server).

use crate::pack::TableId;

/// Reading a pack.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackError {
    #[error("magic {0:#018x}, expected {expected:#018x}", expected = crate::pack::format::MAGIC)]
    BadMagic(u64),

    #[error("pack format version {found}, this build reads {expected}")]
    FormatVersion { expected: u32, found: u32 },

    #[error("pack schema version {found}, this build reads {expected}")]
    SchemaVersion { expected: u32, found: u32 },

    #[error("content hash mismatch: header says {expected}, file hashes to {found}")]
    HashMismatch { expected: String, found: String },

    #[error("index is not sorted at entry {0}")]
    IndexUnsorted(usize),

    #[error("record for table {table:?} key {key:#x} runs past the blob region")]
    BlobOutOfRange { table: TableId, key: u64 },

    #[error("decoder stopped {0} bytes before the end of the record")]
    Shortfall(usize),

    #[error("decoder read {0} bytes past the end of the record")]
    Overrun(usize),

    /// The file is shorter than the offsets in its own header claim; caught at open rather than as
    /// a fault on first touch.
    #[error("file is {len} bytes but the header describes {need} (region: {region})")]
    Truncated {
        region: &'static str,
        len: u64,
        need: u64,
    },

    #[error("header field {field} is {found}, expected {expected}")]
    HeaderField {
        field: &'static str,
        expected: u64,
        found: u64,
    },

    #[error("header flags {0:#010x} set a bit this build does not define")]
    UnknownFlags(u32),

    #[error("the pack was built for era {0}, which this build does not know")]
    UnknownEra(u32),

    #[error("{region} at {off:#x} is not 8-byte aligned")]
    Misaligned { region: &'static str, off: u64 },

    #[error("table directory is not sorted by table id at entry {0}")]
    DirectoryUnsorted(usize),

    #[error("table {table:?} spans index entries {first}..{last} but the index holds {count}")]
    TableSpan {
        table: TableId,
        first: u64,
        last: u64,
        count: u64,
    },

    #[error("string of {0} bytes is not valid UTF-8")]
    BadUtf8(usize),

    #[error("a length field says {0} which does not fit this platform's usize")]
    LengthOverflow(u64),

    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl PackError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

/// Importing the SQL dump and writing a pack.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("SQL syntax at line {line}: {what}")]
    Sql { line: u64, what: String },

    #[error("table `{table}` row {row}: column `{column}` is missing")]
    MissingColumn {
        table: String,
        row: u64,
        column: &'static str,
    },

    #[error("table `{table}` row {row}: column `{column}` is NULL")]
    UnexpectedNull {
        table: String,
        row: u64,
        column: &'static str,
    },

    #[error("table `{table}` row {row}: column `{column}` value {value:?} is not {expected}")]
    BadValue {
        table: String,
        row: u64,
        column: &'static str,
        value: String,
        expected: &'static str,
    },

    #[error("duplicate key {key:#x} in pack table {table}")]
    DuplicateKey { table: u16, key: u64 },

    #[error("record of {len} bytes for table {table} key {key:#x} is too large")]
    RecordTooLarge { table: u16, key: u64, len: usize },

    #[error("the dump has no CREATE TABLE for `{0}`, which the world database requires")]
    MissingTable(String),

    #[error("table `{table}` has no column `{column}`")]
    SchemaColumn { table: String, column: String },

    /// The dump imported, but it left the pack empty or left data unread: each line says what.
    #[error("the import is refused:\n  {}", .0.join("\n  "))]
    Refused(Vec<String>),

    #[error(transparent)]
    Pack(#[from] PackError),
}

impl ImportError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}
