//! The Asheron's Call 2 data-file container, its type table and the record reader every decoder
//! reads through.
//!
//! **Depends on** `auberean-primitives` and the shared container, `dereth-dat`, with
//! `dereth-primitives` for its ids. **Used by** the decoders (`auberean-assets`) and everything
//! that reads the data files.
//!
//! A data file is the shared container (`dereth-dat`) in this game's layout ([`container`]): a
//! header, fixed-size blocks chained into files, and a B-tree directory of 16-byte entries keyed
//! by record id. A record carries no type; its type is derived from its id ([`types`]). Every
//! record is read through a [`Reader`], which is bounds-checked and says where a read failed, and
//! whose [`Reader::finish`] is the exact-consumption check every decoder ends with.
//!
//! **No I/O by default.** A container reads its bytes through the [`DatStorage`] the host gives
//! it: the bytes in memory, or with the `fs` feature a file on disk ([`DatFile::open`]). Where the
//! retail files are is decided in [`locate`] (`fs`) and, for tests, in `testing` (the
//! `test-support` feature) and nowhere else.
//!
//! A read is checked ([`checked`]): a read the storage ends early is reported as a short read, and
//! a record that fails to decode is checked against the file it came from and read once more
//! ([`DatFile::read_checked`]).
//!
//! The data files are also written, as the client writes the records the server sends while the
//! game runs: [`disk`] is the client's disk controller (the block allocator, the directory's
//! splits and merges, a write recorded first so a crash is put right on the next opening).
//!
//! A world's records lie over the files in a folder of their own ([`overlay`]), so what a player
//! installed is never written.
//!
//! The client's own text files (the key maps beside the program, the gameplay options a player
//! exports) are bracketed text trees: [`tree`] reads and writes them.
//!
//! **Must never** interpret what a record means: it yields ids, bytes and readers. Decoding a
//! record is `auberean-assets`' job.

pub mod checked;
pub mod container;
pub mod disk;
pub mod error;
pub mod overlay;
pub mod qualified;
pub mod reader;
pub mod stamp;
pub mod tree;
pub mod types;
pub mod writer;

#[cfg(feature = "fs")]
pub mod fs;
#[cfg(feature = "fs")]
pub mod locate;
#[cfg(feature = "test-support")]
pub mod testing;

pub use checked::{Checked, CheckedError, Retried, ShortRead};
pub use container::{DatFile, Entry, Header, Structure};
/// The iteration list's encoding, the shared container's: [`iteration::decode_signed`].
pub use dereth_dat::iteration;
pub use dereth_dat::{DatError, DatStorage};
pub use error::DecodeError;
pub use qualified::{QualifiedId, QualifiedIds};
pub use reader::Reader;
pub use stamp::DatIdStamp;
pub use types::{cell_type, type_of_id, DatKind, MasterMap, RecordType, TypeInfo};
pub use writer::Writer;
