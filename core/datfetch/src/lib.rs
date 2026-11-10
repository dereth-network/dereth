//! Getting a base data set from the community's archive for the player, with no network and no
//! files in it: the archive's public links, the transfer's decryption and checks, unpacking, and
//! the files' verification.
//!
//! **Depends on** no crate of this workspace: `serde` and `serde_json` for the list of sets,
//! `base64`, `aes` and `sha2` for the archive host's keys, encryption and checks, `crc32fast`,
//! `flate2` and `lzma-rust2` for unpacking zip and 7z archives. **Used by** the launcher's
//! knowledge (`dereth-launch`), for the sets the Library offers and the last check of one, and
//! the front ends that run the steps: the launcher's window, and the web client's front page
//! (`dereth-web-front`), which gets a set into the browser's own storage.
//!
//! **Must never** reach a network or a file itself: the caller does every request and every read
//! and write it asks for, so the same steps run in a browser and on a desktop.
//!
//! [`sources`] is the list of data sets on offer and where each archive is kept (built in, and
//! replaceable by a newer list, read the same way); [`mega`] is the archive host's public links, its API's request and
//! answer, and the file's decryption and MAC; [`archive`] reads zip and 7z archives from the
//! ranges it asks for; [`fetch`] drives getting one set through all of it, step by step.

pub mod archive;
pub mod fetch;
pub mod mega;
pub mod sources;

pub use fetch::{DownloadEnd, Failure, FailureKind, Fetch, Next, Progress, Stage, Step, Write};
pub use sources::{Catalogue, SetSource};

#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

#[cfg(test)]
mod tests;
