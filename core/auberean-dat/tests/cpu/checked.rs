//! Reads that say what went wrong: a read the storage ends early reported as a short read and never
//! decoded, and a record that fails to decode checked against its file and read once more.

// The files built here are a few kilobytes: every length and offset fits in 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use auberean_dat::checked::{short_read, CheckedError, Exact, ReadAt, Retried, Watch};
use auberean_dat::DatFile;

use crate::container::build;

/// The record the tests read: 200 bytes, chained across four blocks.
const ID: u32 = 0x0600_0002;

fn record() -> Vec<u8> {
    (0u8..200).collect()
}

fn file() -> Vec<u8> {
    build(&[(0x0600_0001, vec![7; 10]), (ID, record())], 2)
}

/// A file in memory, served up to `end` (all of it until it is moved), `piece` bytes a call at
/// most, with the byte at `flip` inverted the first time a read covers it.
#[derive(Debug)]
struct Source {
    bytes: Vec<u8>,
    end: AtomicU64,
    piece: usize,
    flip: Option<u64>,
    flipped: AtomicBool,
}

impl Source {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            end: AtomicU64::new(bytes.len() as u64),
            bytes,
            piece: usize::MAX,
            flip: None,
            flipped: AtomicBool::new(false),
        }
    }
}

impl ReadAt for Source {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let end = self.end.load(Ordering::SeqCst).min(self.bytes.len() as u64);
        if offset >= end {
            return Ok(0);
        }
        let n = buf.len().min(self.piece).min((end - offset) as usize);
        let at = offset as usize;
        buf[..n].copy_from_slice(&self.bytes[at..at + n]);
        if let Some(f) = self.flip {
            if (offset..offset + n as u64).contains(&f)
                && !self.flipped.swap(true, Ordering::SeqCst)
            {
                buf[(f - offset) as usize] ^= 0xFF;
            }
        }
        Ok(n)
    }
}

/// A watch the test turns: changed once `changed` is set.
#[derive(Debug, Default)]
struct Turned {
    changed: AtomicBool,
    asked: AtomicUsize,
}

impl Watch for Turned {
    fn changed(&self) -> io::Result<Option<String>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .changed
            .load(Ordering::SeqCst)
            .then(|| "it grew".to_owned()))
    }
}

fn open(source: &Arc<Source>, watch: &Arc<Turned>) -> DatFile {
    DatFile::from_storage("records.dat".into(), Box::new(Exact(Arc::clone(source))))
        .unwrap()
        .watched(Arc::clone(watch) as Arc<dyn Watch>)
}

/// A decoder that takes only the record's own bytes, counting its calls.
fn exact(calls: &AtomicUsize) -> impl Fn(&[u8]) -> Result<Vec<u8>, String> + '_ {
    move |b: &[u8]| {
        calls.fetch_add(1, Ordering::SeqCst);
        if b == record() {
            Ok(b.to_vec())
        } else {
            Err("not the record".to_owned())
        }
    }
}

/// **A read the file ends before its buffer is full is reported as a short read, saying where
/// and how much came back, and nothing it filled is decoded.** The file is cut inside the
/// record's chain after the directory was read.
///
/// Behaviour: data.read.short-reads-are-reported
#[test]
fn a_read_the_file_ends_early_is_a_short_read_and_is_never_decoded() {
    let source = Arc::new(Source::new(file()));
    let watch = Arc::new(Turned::default());
    let dat = open(&source, &watch);
    let first = u64::from(dat.entry(ID).unwrap().offset);
    // Halfway into the record's second block.
    source.end.store(first + 0x40 + 0x20, Ordering::SeqCst);

    let e = dat.read(ID).unwrap_err();
    let short = short_read(&e).expect("a short read");
    assert_eq!(short.offset, first + 0x40, "the second block's read");
    assert_eq!((short.got, short.wanted), (0x20, 0x40));
    assert!(e.to_string().contains("a short read at"), "{e}");

    let calls = AtomicUsize::new(0);
    let failed = dat.read_checked(ID, exact(&calls)).unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 0, "nothing decoded");
    assert!(failed.to_string().contains("a short read at"), "{failed}");
    assert!(
        matches!(
            failed,
            CheckedError::Failed {
                same_bytes: false,
                ..
            }
        ),
        "{failed:?}"
    );
}

/// **A read the source returns in pieces is completed, not cut short.**
///
/// Behaviour: data.read.short-reads-are-reported
#[test]
fn a_read_the_source_returns_a_few_bytes_at_a_time_is_read_whole() {
    let mut s = Source::new(file());
    s.piece = 7;
    let dat = open(&Arc::new(s), &Arc::new(Turned::default()));
    assert_eq!(dat.read(ID).unwrap(), record());
}

/// **A record that fails over a file that changed while it was open is reported as the file
/// changing, and not read again.**
///
/// Behaviour: data.read.failed-records-are-checked-and-read-again
#[test]
fn a_record_that_fails_over_a_changed_file_is_reported_as_the_file_changing() {
    let source = Arc::new(Source::new(file()));
    let watch = Arc::new(Turned::default());
    let dat = open(&source, &watch);
    watch.changed.store(true, Ordering::SeqCst);
    let calls = AtomicUsize::new(0);
    let e = dat
        .read_checked(ID, |b: &[u8]| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(format!("count {} exceeds", b.len()))
        })
        .unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "not read again");
    assert_eq!(
        e,
        CheckedError::Changed {
            file: "records.dat".into(),
            failure: "count 200 exceeds".into(),
            how: "it grew".into(),
        }
    );
    assert_eq!(
        e.to_string(),
        "records.dat changed while it was read (it grew): count 200 exceeds"
    );
    // Negative control: a record that decodes never asks.
    let asked = watch.asked.load(Ordering::SeqCst);
    let ok = dat.read_checked(ID, exact(&calls)).unwrap();
    assert_eq!((ok.value, ok.retried), (record(), None));
    assert_eq!(watch.asked.load(Ordering::SeqCst), asked);
}

/// **A record whose first read comes back wrong over a file that did not change is read again,
/// and the second read's record is the answer, with the first failure and the bytes having
/// differed.**
///
/// Behaviour: data.read.failed-records-are-checked-and-read-again
#[test]
fn a_record_read_wrong_once_over_an_unchanged_file_is_read_again() {
    let bytes = file();
    let probe = DatFile::from_bytes(bytes.clone()).unwrap();
    let at = u64::from(probe.entry(ID).unwrap().offset) + 4 + 9;
    let mut s = Source::new(bytes);
    s.flip = Some(at);
    let watch = Arc::new(Turned::default());
    let dat = open(&Arc::new(s), &watch);
    let calls = AtomicUsize::new(0);
    let got = dat.read_checked(ID, exact(&calls)).unwrap();
    assert_eq!(got.value, record());
    assert_eq!(
        got.retried,
        Some(Retried {
            first: "not the record".into(),
            same_bytes: false,
        })
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        watch.asked.load(Ordering::SeqCst),
        1,
        "asked once, at the failure"
    );
}

/// **A decode that fails once over the same bytes is read again and says the bytes were the
/// same**: the fault was not in the file's bytes.
///
/// Behaviour: data.read.failed-records-are-checked-and-read-again
#[test]
fn a_decode_that_fails_once_over_the_same_bytes_says_they_were_the_same() {
    let dat = open(&Arc::new(Source::new(file())), &Arc::new(Turned::default()));
    let calls = AtomicUsize::new(0);
    let got = dat
        .read_checked(ID, |b: &[u8]| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Err("count 1743650835 x 20 exceeds 1136344 left")
            } else {
                Ok(b.len())
            }
        })
        .unwrap();
    assert_eq!(got.value, 200);
    assert!(
        got.retried.as_ref().is_some_and(|r| r.same_bytes),
        "{got:?}"
    );
    assert!(
        got.retried
            .unwrap()
            .to_string()
            .contains("the same bytes both times"),
        "the retry says so"
    );
}

/// **A record that fails twice over a file that did not change is reported with both failures.**
///
/// Behaviour: data.read.failed-records-are-checked-and-read-again
#[test]
fn a_record_that_fails_twice_over_an_unchanged_file_reports_both_failures() {
    let watch = Arc::new(Turned::default());
    let dat = open(&Arc::new(Source::new(file())), &watch);
    let calls = AtomicUsize::new(0);
    let e = dat
        .read_checked(ID, |_: &[u8]| {
            Err::<(), _>(format!("try {}", calls.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap_err();
    assert_eq!(
        e,
        CheckedError::Failed {
            first: "try 0".into(),
            second: "try 1".into(),
            same_bytes: true,
        }
    );
    assert_eq!(
        e.to_string(),
        "try 1 (read twice, the same bytes; the first time: try 0)"
    );
    assert_eq!(
        watch.asked.load(Ordering::SeqCst),
        2,
        "before and after the second read"
    );
}

/// The file tests, over a file on disk (the `fs` feature).
#[cfg(feature = "fs")]
mod on_disk {
    use super::*;
    use std::path::PathBuf;

    /// A folder of the test's own, removed when dropped.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "auberean-dat-checked-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// **A data file on disk cut short while open: the record's read is a short read, and the
    /// failure is reported as the file changing, with its size then and now.** A file left alone
    /// reports a failing decode as failing twice, not as a change.
    ///
    /// Behaviour: data.read.failed-records-are-checked-and-read-again
    #[test]
    fn a_data_file_cut_short_while_open_is_reported_as_changed() {
        let dir = Folder::new("cut");
        let path = dir.0.join("records.dat");
        let bytes = file();
        std::fs::write(&path, &bytes).unwrap();
        let dat = DatFile::open(&path).unwrap();
        let calls = AtomicUsize::new(0);

        // Negative control: the file unchanged.
        let e = dat
            .read_checked(ID, |_: &[u8]| Err::<(), _>("bad"))
            .unwrap_err();
        assert!(matches!(e, CheckedError::Failed { .. }), "{e:?}");
        assert_eq!(dat.read_checked(ID, exact(&calls)).unwrap().value, record());

        let first = u64::from(dat.entry(ID).unwrap().offset);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(first + 0x10)
            .unwrap();
        assert!(short_read(&dat.read(ID).unwrap_err()).is_some());
        let e = dat.read_checked(ID, exact(&calls)).unwrap_err();
        let CheckedError::Changed { failure, how, .. } = &e else {
            panic!("{e:?}");
        };
        assert!(failure.contains("a short read at"), "{failure}");
        assert!(
            how.starts_with(&format!("{} bytes modified at ", bytes.len()))
                && how.contains(&format!("now {} bytes", first + 0x10)),
            "{how}"
        );
        assert!(e.to_string().contains("changed while it was read"), "{e}");
    }
}

/// **A write in memory over a part of the file that no longer reads fails with the short read,
/// rather than taking the file's part of the page as zeros.**
///
/// Behaviour: data.read.short-reads-are-reported
#[test]
fn a_write_over_a_part_of_the_file_that_does_not_read_fails_and_takes_no_zeros() {
    use auberean_dat::disk::Overlay;
    use dereth_dat::DatStorageMut;

    let bytes = file();
    let source = Arc::new(Source::new(bytes.clone()));
    let dat = open(&source, &Arc::new(Turned::default()));
    let first = u64::from(dat.entry(ID).unwrap().offset);
    let mut layer = Overlay::new(Arc::new(dat));
    source.end.store(first, Ordering::SeqCst);
    let e = layer.write_all_at(first + 8, &[1, 2, 3, 4]).unwrap_err();
    assert!(
        e.get_ref()
            .and_then(|i| i.downcast_ref::<auberean_dat::ShortRead>())
            .is_some(),
        "{e}"
    );
    // The file whole again, the same write takes the file's bytes around it.
    source.end.store(bytes.len() as u64, Ordering::SeqCst);
    layer.write_all_at(first + 8, &[1, 2, 3, 4]).unwrap();
    let mut back = [0u8; 12];
    layer.read_exact_at(first + 4, &mut back).unwrap();
    assert_eq!(back, [0, 1, 2, 3, 1, 2, 3, 4, 8, 9, 10, 11]);
}
