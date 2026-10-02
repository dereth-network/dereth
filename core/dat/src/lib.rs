//! The retail data-file container and the serialisation primitives every decoder reads through.
//!
//! **Depends on** `dereth-primitives`. **Used by** the decoders (`dereth-assets`), the shared world
//! adapters (`dereth-world-data`), the client runtime (`dereth-client-runtime`), the client
//! (`dereth-client`), the SDK (`dereth-client-sdk`), the test kit (`dereth-testkit`), the
//! server's dat layer (`empyrean-dat`) and binary (`empyrean-server`) and `xtask`; every crate's
//! tests find the retail files through it.
//!
//! Where the retail files are is decided here and nowhere else: [`locate`] searches the
//! directories a caller offers for `client_portal.dat` and names each file in the one it finds,
//! and `testing` (the `test-support` feature, for tests only) is the tests' single lookup. The
//! crate reads no environment variable outside that feature.
//!
//! **Must never** interpret what it reads: it yields bytes, ids and cursors, and decoding an object
//! is `dereth-assets`' job.
//!
//! Three things here are easy to get wrong:
//!
//! 1. **The archive is not word-aligned.** Alignment is opt-in per archive and data-file reads run
//!    with it off, so every read is unaligned and the only padding is what a decoder asks for with
//!    [`Cursor::align_ptr`].
//! 2. **The file-type routing is a range table, not `id >> 24`.** See [`divine::divine_type`].
//! 3. **The three hash-table headers are different.** See [`archive::intrusive_hash_table_header`],
//!    [`archive::intrusive_hash_list_header`] and [`packobj::packable_hash_table_header`].
//!
//! Writing is [`write`](mod@write). The container's half of the data-patch path is here too: [`inflate`], the
//! zlib decompressor a compressed data message needs, and [`container::DatFile::reload`] /
//! [`store::RetailDatStore::reload`], which re-walk a directory a writer has changed underneath an
//! open reader. The patch protocol itself is `dereth_client_runtime::ddd`.
//!
//! **Specified in** `docs/formats/01-dat-container.md` (the container and the lookup across the
//! four files), `docs/formats/02-file-ids-and-types.md` (file ids and their types) and
//! `docs/formats/03-serialisation-primitives.md` (the primitives every record is built from).

#![doc(html_no_source)]

pub mod archive;
pub mod btree;
pub mod container;
pub mod cursor;
pub mod divine;
pub mod error;
pub mod inflate;
pub mod iteration;
pub mod locate;
pub mod packobj;
pub mod store;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
pub mod write;

pub use archive::{HashHeader, VersionRow};
pub use btree::{BtEntry, BtNode};
pub use container::{ContainerEra, DatFile, DatStorage, DiskFileInfo, StructureReport};
pub use cursor::Cursor;
pub use divine::{
    classify_cell_id, dat_for_type, divine_type, divine_type_in, DatKind, DbType, ITERATION_LIST,
};
pub use error::DatError;
pub use inflate::{inflate_raw, inflate_zlib};
pub use locate::{
    holds_pre_tod_dats, holds_retail_dats, locate_retail_dats, protect_install, DatDir,
    DatsNotFound, PreTodDat, RetailDat,
};
pub use store::RetailDatStore;
pub use write::{DatWriter, Fault, SaveOutcome};
