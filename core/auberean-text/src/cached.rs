//! The client's strings as its interfaces reach them: string references written through the
//! client's data cache (a string table the server sent read as the files' own are), and
//! downloads of the string tables the cache lacks, which a text element waits on before it writes
//! its text. The client runtime's string downloads are one; an interface holds them as a
//! [`CachedStrings`], so it names nothing of the runtime.

use std::sync::Arc;

use crate::StringInfo;

/// The client's strings, written through its data cache, and downloads of the string tables it
/// lacks.
pub trait CachedStrings: Send + Sync {
    /// Whether it reads through a data cache that has files (it writes nothing otherwise).
    fn reads(&self) -> bool;

    /// A reference's text as a text element shows it: written, or the client's error message when
    /// it cannot be; `None` without a cache that has files.
    fn write(&self, info: &StringInfo) -> Option<Vec<u16>>;

    /// A reference written only when it is valid (it and every nested reference name an entry
    /// their tables hold); `None` otherwise, or without a cache that has files.
    fn write_valid(&self, info: &StringInfo) -> Option<Vec<u16>>;

    /// A reference written when it renders; `None` when it does not, or without a cache that has
    /// files.
    fn write_rendered(&self, info: &StringInfo) -> Option<Vec<u16>>;

    /// Start a download for a reference a text element was given: its number while it waits,
    /// `None` when every table it reads is at hand and it is written at once.
    fn start(&self, info: &StringInfo) -> Option<u32>;

    /// Whether a download still waits. One that does not is forgotten (the element writes it).
    fn waiting(&self, download: u32) -> bool;

    /// A download dropped by its element: what it asked for is let go, and it is told nothing.
    fn release(&self, download: u32);

    /// The same strings, to keep: what it holds is shared, not copied.
    fn share(&self) -> Arc<dyn CachedStrings>;
}
