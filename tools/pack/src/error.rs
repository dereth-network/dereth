//! What a build, a bake or an import could not do.

use std::fmt;
use std::path::{Path, PathBuf};

/// A place in a source file: its path and line (0 when the whole file is meant).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    pub file: PathBuf,
    pub line: usize,
}

impl Loc {
    #[must_use]
    pub fn new(file: &Path, line: usize) -> Self {
        Self {
            file: file.to_path_buf(),
            line,
        }
    }

    /// The line of byte `offset` in `text`, counted from 1.
    #[must_use]
    pub fn at(file: &Path, text: &str, offset: usize) -> Self {
        let line = text
            .as_bytes()
            .iter()
            .take(offset)
            .filter(|b| **b == b'\n')
            .count()
            + 1;
        Self::new(file, line)
    }
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.file.display())
        } else {
            write!(f, "{}:{}", self.file.display(), self.line)
        }
    }
}

/// What went wrong.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A source that does not read, or says something the format does not allow.
    Source { at: Loc, why: String },
    /// Two sources that set the same thing differently.
    Conflict {
        first: Loc,
        second: Loc,
        what: String,
    },
    /// A file that could not be read or written.
    Io { path: PathBuf, why: String },
    /// The data files: one that will not open or read, or an overlay that will not write.
    Dat(String),
    /// Records that failed the checks, one line each.
    Invalid(Vec<String>),
    /// A request the tool refuses, with the reason.
    Refused(String),
}

impl Error {
    /// A source error at `at`.
    pub fn source(at: &Loc, why: impl Into<String>) -> Self {
        Self::Source {
            at: at.clone(),
            why: why.into(),
        }
    }

    /// An I/O error on `path`.
    pub fn io(path: &Path, e: impl fmt::Display) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            why: e.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source { at, why } => write!(f, "{at}: {why}"),
            Self::Conflict {
                first,
                second,
                what,
            } => {
                write!(f, "{second}: {what}, which {first} sets otherwise")
            }
            Self::Io { path, why } => write!(f, "{}: {why}", path.display()),
            Self::Dat(why) => write!(f, "{why}"),
            Self::Invalid(problems) => {
                write!(f, "{} record(s) failed the checks:", problems.len())?;
                for p in problems {
                    write!(f, "\n  {p}")?;
                }
                Ok(())
            }
            Self::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<dereth_dat::DatError> for Error {
    fn from(e: dereth_dat::DatError) -> Self {
        Self::Dat(e.to_string())
    }
}

impl From<dereth_dat::overlay::OverlayError> for Error {
    fn from(e: dereth_dat::overlay::OverlayError) -> Self {
        Self::Dat(e.to_string())
    }
}
