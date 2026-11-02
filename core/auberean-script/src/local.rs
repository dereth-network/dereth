//! The machine's own clock and files: the wall clock, and files kept in the client's own folder.
//!
//! The client writes a player's chat log and exports their options beside itself, by a name the
//! player gives; it reads the wall clock for the log's timestamps. A host that has a disk and a
//! clock gives them through [`LocalSystem`]; one that has none keeps the defaults, which refuse,
//! and the natives then answer as the client does when the call fails.
//!
//! A file is named by a bare name only ([`bare_name`]): no folder, no drive, nothing that climbs
//! out of the folder the host keeps them in.

/// The local date and time, as the system gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 0 (Sunday) to 6.
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

/// The clock and the files of the machine the client runs on. Every method has a default that
/// refuses, so a host gives only what it has.
pub trait LocalSystem {
    /// The full path of the folder the files are kept in, as a player is shown it; none when the
    /// host keeps no folder a player can find.
    fn folder(&self) -> Option<String> {
        None
    }

    /// The local date and time; none when the host has no clock.
    fn local_time(&self) -> Option<LocalTime> {
        None
    }

    /// Seconds since 1970 (the wall clock); none when the host has no clock.
    fn real_time(&self) -> Option<i64> {
        None
    }

    /// Add bytes to the end of a file, making it when there is none; whether they were written.
    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        let _ = (file, bytes);
        false
    }

    /// Write a file anew; whether it was written.
    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        let _ = (file, bytes);
        false
    }

    /// A file's bytes; none when there is no such file or the host keeps no files.
    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        let _ = file;
        None
    }
}

/// Whether a file name is a bare name, which stays in the folder the host keeps files in: not
/// empty, not `.` or `..`, and holding no folder separator (`/` or `\`), no drive (`:`) and no
/// zero.
#[must_use]
pub fn bare_name(file: &str) -> bool {
    !file.is_empty() && file != "." && file != ".." && !file.contains(['/', '\\', ':', '\0'])
}
