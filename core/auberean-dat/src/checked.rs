//! Reads that say what went wrong: a short read named as one, and a record that fails to decode
//! checked against the file it came from and read once more.
//!
//! - **A short read** ([`ShortRead`]): a read the storage ends before its buffer is full. [`Exact`]
//!   completes a read the source returns in pieces and, when the source ends first, fails naming
//!   where and how much of it came back; nothing a short read filled is decoded.
//! - **A failed decode** ([`DatFile::read_checked`]): when a record fails to read or to decode,
//!   the file's [`Watch`] is asked whether the file changed since it was opened. A file that changed
//!   is reported as that ([`CheckedError::Changed`]). One that did not is read and decoded once
//!   more: a second success is returned with the first failure ([`Retried`]), a second failure is
//!   reported with both and whether the two reads gave the same bytes ([`CheckedError::Failed`]).
//!
//! Nothing here does I/O itself: the source ([`ReadAt`]) and the watch are the host's, a file's
//! with the `fs` feature ([`DatFile::open`]).

use std::fmt;
use std::io;
use std::sync::Arc;

use dereth_dat::{DatError, DatStorage};

use crate::container::DatFile;

/// A source of bytes by position: as many bytes as it has from `offset`, up to `buf`'s length.
pub trait ReadAt: fmt::Debug + Send + Sync {
    /// Read into `buf` from `offset`; how many bytes were read, 0 at the end of the source.
    ///
    /// # Errors
    /// Whatever the source reports.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

impl<R: ReadAt + ?Sized> ReadAt for Arc<R> {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read_at(offset, buf)
    }
}

/// A read the source ended before its buffer was full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortRead {
    /// Where the read started.
    pub offset: u64,
    /// How many bytes it asked for.
    pub wanted: usize,
    /// How many came back before the source ended.
    pub got: usize,
}

impl fmt::Display for ShortRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a short read at {:#x}: {} of {} bytes",
            self.offset, self.got, self.wanted
        )
    }
}

impl std::error::Error for ShortRead {}

/// The short read behind a read error, when it was one.
#[must_use]
pub fn short_read(e: &DatError) -> Option<&ShortRead> {
    match e {
        DatError::Io(io) => io.get_ref()?.downcast_ref::<ShortRead>(),
        _ => None,
    }
}

/// A source read as storage: each read filled whole, a short one reported as [`ShortRead`].
#[derive(Debug)]
pub struct Exact<R>(pub R);

impl<R: ReadAt> DatStorage for Exact<R> {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let wanted = buf.len();
        let mut got = 0;
        while got < wanted {
            match self.0.read_at(offset + got as u64, &mut buf[got..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        ShortRead {
                            offset,
                            wanted,
                            got,
                        },
                    ))
                }
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Whether the file under a container changed since it was opened.
pub trait Watch: fmt::Debug + Send + Sync {
    /// How it changed, or `None` when it did not.
    ///
    /// # Errors
    /// The file no longer answers.
    fn changed(&self) -> io::Result<Option<String>>;
}

/// A record read by [`DatFile::read_checked`] the second time, after the first failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retried {
    /// How the first read or decode failed.
    pub first: String,
    /// Whether the two reads gave the same bytes (false when the first read failed).
    pub same_bytes: bool,
}

impl fmt::Display for Retried {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = if self.same_bytes {
            "the same bytes both times"
        } else {
            "different bytes the second time"
        };
        write!(
            f,
            "read again after it failed ({}); {bytes}, over a file that did not change",
            self.first
        )
    }
}

/// A record read by [`DatFile::read_checked`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked<T> {
    pub value: T,
    /// Set when the first read or decode failed and the second succeeded.
    pub retried: Option<Retried>,
}

/// Why [`DatFile::read_checked`] gave no record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedError {
    /// The file changed while it was open: what the read or decode found, and how the file
    /// changed.
    Changed {
        file: String,
        failure: String,
        how: String,
    },
    /// The record failed twice over a file that did not change.
    Failed {
        first: String,
        second: String,
        /// Whether the two reads gave the same bytes (false when either read failed).
        same_bytes: bool,
    },
}

impl fmt::Display for CheckedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed { file, failure, how } => {
                write!(f, "{file} changed while it was read ({how}): {failure}")
            }
            Self::Failed {
                first,
                second,
                same_bytes,
            } => {
                let bytes = if *same_bytes {
                    "the same bytes"
                } else {
                    "different bytes"
                };
                write!(f, "{second} (read twice, {bytes}")?;
                if first != second {
                    write!(f, "; the first time: {first}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl std::error::Error for CheckedError {}

impl DatFile {
    /// Read the record `id` and decode it with `decode`, checking a failure against the file.
    ///
    /// When the read or the decode fails, the file's watch ([`DatFile::watched`]) is asked whether
    /// the file changed; a container with no watch is taken as unchanged. A changed file is
    /// [`CheckedError::Changed`]. Over an unchanged file the record is read and decoded once more:
    /// a success then carries the first failure in [`Checked::retried`]; a second failure, the
    /// file still unchanged, is [`CheckedError::Failed`].
    ///
    /// # Errors
    /// As above.
    pub fn read_checked<T, E: fmt::Display>(
        &self,
        id: u32,
        decode: impl Fn(&[u8]) -> Result<T, E>,
    ) -> Result<Checked<T>, CheckedError> {
        let attempt = || -> (Option<Vec<u8>>, Result<T, String>) {
            match self.read(id) {
                Err(e) => (None, Err(e.to_string())),
                Ok(bytes) => {
                    let value = decode(&bytes).map_err(|e| e.to_string());
                    (Some(bytes), value)
                }
            }
        };
        let (bytes, first) = match attempt() {
            (_, Ok(value)) => {
                return Ok(Checked {
                    value,
                    retried: None,
                })
            }
            (bytes, Err(first)) => (bytes, first),
        };
        self.unchanged(&first)?;
        let (again, second) = attempt();
        let same_bytes = bytes.is_some() && bytes == again;
        match second {
            Ok(value) => Ok(Checked {
                value,
                retried: Some(Retried { first, same_bytes }),
            }),
            Err(second) => {
                self.unchanged(&second)?;
                Err(CheckedError::Failed {
                    first,
                    second,
                    same_bytes,
                })
            }
        }
    }

    /// `Ok` when the file did not change since it was opened (or has no watch), else what
    /// [`DatFile::read_checked`] reports for `failure`.
    fn unchanged(&self, failure: &str) -> Result<(), CheckedError> {
        let Some(watch) = self.watch() else {
            return Ok(());
        };
        let how = match watch.changed() {
            Ok(None) => return Ok(()),
            Ok(Some(how)) => how,
            Err(e) => format!("it no longer answers: {e}"),
        };
        Err(CheckedError::Changed {
            file: self.shared().path().display().to_string(),
            failure: failure.to_owned(),
            how,
        })
    }
}
