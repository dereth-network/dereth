//! Behaviour: none (getting a data set from the community's archive: tooling around the client, run against a host made in the test).

use std::collections::BTreeMap;
use std::io::Write as _;

use sha2::{Digest, Sha256};

use crate::archive::Format;
use crate::mega::FileRef;
use crate::sources::{ArchiveSource, FileSource, Iterations, Mirror, SetKind, SetSource};
use crate::testing::{host, Hosted};
use crate::{DownloadEnd, FailureKind, Fetch, Next, Step};

/// Bytes that compress, but not to nothing.
fn content(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(7);
    (0..len)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            if i % 4 == 0 {
                x.to_le_bytes()[0]
            } else {
                (i / 32).to_le_bytes()[0]
            }
        })
        .collect()
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// The host as the test runs it: what it answers for each handle, and what goes wrong.
#[derive(Default)]
struct Host {
    files: Vec<Hosted>,
    /// Handles whose API answer is this error code.
    errors: BTreeMap<String, i64>,
    /// A download stops after this many bytes, this many times.
    cut_after: Option<(u64, u32)>,
    /// Every download answers this HTTP status.
    status: Option<u16>,
    /// The byte at this offset arrives flipped, in every download.
    flip: Option<u64>,
    /// The size the API reports, if not the file's.
    size: Option<u64>,
    /// Each download's handle and where it began.
    downloads: Vec<(String, u64)>,
    /// Where each range asked for began.
    ranges: Vec<u64>,
    /// The most GETs at once, and the most bytes the fetch held for later.
    most_at_once: usize,
    most_held: u64,
    /// The window the fetch was given, if not [`TEST_WINDOW`].
    window: Option<u64>,
    waits: Vec<u32>,
}

/// The caller's storage: the archives kept and the set's files.
#[derive(Default)]
struct Disk {
    archives: BTreeMap<String, Vec<u8>>,
    files: BTreeMap<String, Vec<u8>>,
}

fn handle_of(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    let c = &v[0];
    c["p"]
        .as_str()
        .or_else(|| c["n"].as_str())
        .unwrap()
        .to_owned()
}

/// The ranges and the window the tests' downloads use: the archives are a few of each long.
const TEST_RANGE: u64 = 100_000;
const TEST_WINDOW: u64 = 300_000;

fn fetch_of(set: SetSource, kept: &[u64]) -> Fetch {
    Fetch::new(set, kept).with_ranges(TEST_RANGE, TEST_WINDOW)
}

/// One GET in flight: its range, the bytes the host sends for it, and how many it has sent.
struct Get {
    id: usize,
    bytes: Vec<u8>,
    sent: usize,
}

/// A download's connections, as a network runs them: each takes a range when it is free, and the
/// latest range sends first, so ranges arrive out of order. What the fetch hands back for writing
/// must always continue the archive kept.
fn connect(fetch: &mut Fetch, host: &mut Host, kept: &mut Vec<u8>, connections: usize, from: u64) {
    let mut gets: Vec<Option<Get>> = (0..connections).map(|_| None).collect();
    let mut finished = vec![false; connections];
    let mut first = true;
    loop {
        for c in 0..connections {
            if gets[c].is_some() || finished[c] {
                continue;
            }
            match fetch.next_range() {
                Next::Range {
                    id,
                    url,
                    from: start,
                    to,
                } => {
                    let handle = url
                        .split("/dl/")
                        .nth(1)
                        .unwrap()
                        .split('/')
                        .next()
                        .unwrap()
                        .to_owned();
                    assert!(url.ends_with(&format!("/{start}-{}", to - 1)), "{url}");
                    if first {
                        host.downloads.push((handle.clone(), from));
                        first = false;
                    }
                    host.ranges.push(start);
                    let f = host.files.iter().find(|f| f.handle == handle).unwrap();
                    let mut end = to;
                    if let Some((after, times)) = host.cut_after.as_mut() {
                        if *times > 0 {
                            *times -= 1;
                            end = end.min(start + *after);
                        }
                    }
                    let mut bytes = f.cipher
                        [usize::try_from(start).unwrap()..usize::try_from(end).unwrap()]
                        .to_vec();
                    if let Some(at) = host.flip {
                        if at >= start && at < end {
                            bytes[usize::try_from(at - start).unwrap()] ^= 1;
                        }
                    }
                    gets[c] = Some(Get { id, bytes, sent: 0 });
                }
                Next::Wait => {}
                Next::Finished => finished[c] = true,
            }
        }
        host.most_at_once = host.most_at_once.max(gets.iter().flatten().count());
        let Some(c) = (0..connections).rev().find(|&c| gets[c].is_some()) else {
            assert!(
                finished.iter().all(|f| *f),
                "a connection waits with none busy"
            );
            return;
        };
        let g = gets[c].as_mut().unwrap();
        let end = if let Some(s) = host.status {
            Some(DownloadEnd::Status(s))
        } else if g.sent == g.bytes.len() {
            Some(DownloadEnd::Ended)
        } else {
            // Pieces of odd sizes, as a network delivers them.
            let n = (g.bytes.len() - g.sent).min(16_381);
            let mut piece = g.bytes[g.sent..g.sent + n].to_vec();
            g.sent += n;
            match fetch.range_piece(g.id, &mut piece) {
                Some(writes) => {
                    for (at, bytes) in writes {
                        assert_eq!(at, kept.len() as u64, "written in order");
                        kept.extend_from_slice(&bytes);
                    }
                    let held = fetch.progress().done - kept.len() as u64;
                    let window = host.window.unwrap_or(TEST_WINDOW);
                    assert!(held < window + TEST_RANGE, "{held} bytes held");
                    host.most_held = host.most_held.max(held);
                    None
                }
                None => Some(DownloadEnd::Ended),
            }
        };
        if let Some(end) = end {
            fetch.range_ended(g.id, &end);
            gets[c] = None;
        }
    }
}

/// Run a fetch to its end against `host`, as a caller would.
fn run(fetch: &mut Fetch, host: &mut Host, disk: &mut Disk) -> Step {
    for _ in 0..10_000 {
        let step = fetch.step();
        match &step {
            Step::Api { body, after_ms, .. } => {
                host.waits.push(*after_ms);
                let handle = handle_of(body);
                let reply = if let Some(code) = host.errors.get(&handle) {
                    format!("[{code}]")
                } else {
                    let f = host
                        .files
                        .iter()
                        .find(|f| f.handle == handle)
                        .expect("a hosted file");
                    let size = host.size.unwrap_or(f.cipher.len() as u64);
                    format!(
                        r#"[{{"s":{size},"at":"{}","g":"https://gfs1.example/dl/{handle}"}}]"#,
                        f.attributes()
                    )
                };
                fetch.api_reply(Ok(&reply));
            }
            Step::Download {
                archive,
                from,
                connections,
                ..
            } => {
                let kept = disk.archives.entry(archive.clone()).or_default();
                kept.truncate(usize::try_from(*from).unwrap());
                connect(fetch, host, kept, *connections, *from);
            }
            Step::Read {
                archive,
                offset,
                len,
            } => {
                let a = &disk.archives[archive];
                let (o, l) = (
                    usize::try_from(*offset).unwrap(),
                    usize::try_from(*len).unwrap(),
                );
                for w in fetch.read(&a[o..o + l]) {
                    let f = disk.files.get_mut(&w.file).expect("a prepared file");
                    let at = usize::try_from(w.offset).unwrap();
                    if f.len() < at + w.bytes.len() {
                        f.resize(at + w.bytes.len(), 0);
                    }
                    f[at..at + w.bytes.len()].copy_from_slice(&w.bytes);
                }
            }
            Step::Prepare { files } => {
                for f in files {
                    disk.files.insert(f.clone(), Vec::new());
                }
                fetch.done();
            }
            Step::Remove { archive } => {
                disk.archives.remove(archive);
                fetch.done();
            }
            Step::Skip { archive, files, .. } => {
                disk.archives.remove(archive);
                for f in files {
                    disk.files.remove(f);
                }
                fetch.done();
            }
            Step::Done { .. } | Step::Failed(_) => return step,
        }
    }
    panic!("the fetch never ended");
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        w.start_file(*name, opts).unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn seven_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut w = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
    w.set_content_methods(vec![sevenz_rust2::EncoderMethod::LZMA2.into()]);
    w.push_archive_entry(
        sevenz_rust2::ArchiveEntry::new_file(name),
        Some(std::io::Cursor::new(bytes.to_vec())),
    )
    .unwrap();
    w.finish().unwrap().into_inner()
}

/// A set of two archives: a zip holding `a.dat` and `b.dat` (and a program left in it), on two
/// mirrors, and a 7z holding `c.dat`.
struct Fixture {
    set: SetSource,
    host: Host,
    plain: BTreeMap<String, Vec<u8>>,
}

fn fixture() -> Fixture {
    let a = content(1_500_000, 1);
    let b = content(40_000, 2);
    let c = content(700_000, 3);
    let zip = zip_of(&[
        ("data/a.dat", &a),
        ("prog.exe", &content(3_000, 4)),
        ("data/b.dat", &b),
    ]);
    let seven = seven_of("c.dat", &c);
    let first = host("ZipOne01", "set.zip", &zip, 1);
    let second = host("ZipTwo02", "set.zip", &zip, 2);
    let third = host("Seven003", "c.7z", &seven, 3);
    let file = |name: &str, bytes: &[u8]| FileSource {
        name: name.into(),
        size: bytes.len() as u64,
        sha256: Some(sha(bytes)),
    };
    let set = SetSource {
        id: "test-set".into(),
        label: "Test set".into(),
        kind: SetKind::Modern,
        page: "https://example.com/".into(),
        iterations: Iterations::default(),
        archives: vec![
            ArchiveSource {
                format: Format::Zip,
                size: zip.len() as u64,
                mirrors: vec![
                    Mirror {
                        link: first.link(),
                        key: None,
                    },
                    Mirror {
                        link: second.link(),
                        key: None,
                    },
                ],
                files: vec![file("a.dat", &a), file("b.dat", &b)],
                optional: false,
            },
            ArchiveSource {
                format: Format::SevenZip,
                size: seven.len() as u64,
                mirrors: vec![Mirror {
                    link: third.link(),
                    key: None,
                }],
                files: vec![file("c.dat", &c)],
                optional: false,
            },
        ],
    };
    let plain = [("a.dat", a), ("b.dat", b), ("c.dat", c)]
        .into_iter()
        .map(|(n, b)| (n.to_owned(), b))
        .collect();
    Fixture {
        set,
        host: Host {
            files: vec![first, second, third],
            ..Host::default()
        },
        plain,
    }
}

/// A set comes down archive by archive and ends as its files, each checked; the archives are
/// removed once unpacked, and the program in the zip is never written.
#[test]
fn a_set_ends_as_its_files() {
    let mut fx = fixture();
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let end = run(&mut fetch, &mut fx.host, &mut disk);
    assert_eq!(
        end,
        Step::Done {
            files: vec!["a.dat".into(), "b.dat".into(), "c.dat".into()],
            note: None,
        }
    );
    assert_eq!(disk.files, fx.plain);
    assert!(
        disk.archives.is_empty(),
        "archives removed: {:?}",
        disk.archives.keys()
    );
    assert_eq!(
        fx.host.downloads,
        [("ZipOne01".to_owned(), 0), ("Seven003".to_owned(), 0)]
    );
}

/// An archive partly kept from a try before is continued from where it stopped, not begun again.
#[test]
fn a_kept_archive_is_continued() {
    let mut fx = fixture();
    let mut disk = Disk::default();
    let half = fx.set.archives[0].size / 2;
    let mut kept = fx.host.files[0].cipher[..usize::try_from(half).unwrap()].to_vec();
    FileRef::parse(&fx.host.files[0].link(), None)
        .unwrap()
        .decryptor()
        .apply(0, &mut kept);
    disk.archives.insert("test-set-1.zip".into(), kept);
    let mut fetch = fetch_of(fx.set.clone(), &[half]);
    assert!(matches!(
        run(&mut fetch, &mut fx.host, &mut disk),
        Step::Done { .. }
    ));
    assert_eq!(fx.host.downloads[0], ("ZipOne01".to_owned(), half));
    assert_eq!(disk.files, fx.plain);
}

/// A connection that closes early is asked again, after a pause, from where it stopped.
#[test]
fn a_cut_download_is_continued_after_a_pause() {
    let mut fx = fixture();
    // The first three ranges asked for, and the first of the next try, each stop 40,000 bytes in.
    fx.host.cut_after = Some((40_000, 4));
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    assert!(matches!(
        run(&mut fetch, &mut fx.host, &mut disk),
        Step::Done { .. }
    ));
    let froms: Vec<u64> = fx.host.downloads.iter().map(|d| d.1).collect();
    assert_eq!(&froms[..3], [0, 40_000, 80_000]);
    assert!(
        fx.host.waits[1] > 0 && fx.host.waits[2] > fx.host.waits[1],
        "{:?}",
        fx.host.waits
    );
    assert_eq!(disk.files, fx.plain);
}

/// A dead link is passed over for the next mirror.
#[test]
fn a_dead_link_falls_back_to_the_next_mirror() {
    let mut fx = fixture();
    fx.host.errors.insert("ZipOne01".into(), -9);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    assert!(matches!(
        run(&mut fetch, &mut fx.host, &mut disk),
        Step::Done { .. }
    ));
    assert_eq!(fx.host.downloads[0].0, "ZipTwo02");
}

/// When every link is dead the fetch says so, as a dead link.
#[test]
fn every_link_dead_is_gone() {
    let mut fx = fixture();
    fx.host.errors.insert("ZipOne01".into(), -9);
    fx.host.errors.insert("ZipTwo02".into(), -16);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::Gone);
    assert!(f.message.contains("Test set"), "{}", f.message);
}

/// A link that answers a file of another size has changed, and is not used.
#[test]
fn a_changed_file_is_not_used() {
    let mut fx = fixture();
    fx.host.size = Some(12_345);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::Gone);
    assert!(fx.host.downloads.is_empty());
}

/// The host's download limit stops the fetch at once, as such.
#[test]
fn the_download_limit_stops_it() {
    let mut fx = fixture();
    fx.host.status = Some(509);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::OverQuota);
    let mut fx = fixture();
    fx.host.errors.insert("ZipOne01".into(), -17);
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::OverQuota);
}

/// A byte damaged in transit fails the MAC: the archive is downloaded once more, and damaged
/// again, the fetch says so.
#[test]
fn a_damaged_download_is_caught_by_its_mac() {
    let mut fx = fixture();
    fx.host.flip = Some(1_000);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::Damaged);
    assert_eq!(fx.host.downloads.len(), 2);
    assert!(
        disk.files.is_empty(),
        "nothing unpacked from a damaged archive"
    );
}

/// A file that unpacks cleanly but is not the one listed (by its hash) is refused.
#[test]
fn a_file_not_listed_is_refused() {
    let mut fx = fixture();
    fx.set.archives[0].files[1].sha256 = Some("0".repeat(64));
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::Damaged);
    assert!(f.message.contains("b.dat"), "{}", f.message);
}

/// A list naming a file the archive does not hold is refused before anything is written.
#[test]
fn a_file_missing_from_the_archive_is_refused() {
    let mut fx = fixture();
    fx.set.archives[1].files[0].name = "d.dat".into();
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert!(f.message.contains("d.dat"), "{}", f.message);
    assert!(!disk.files.contains_key("d.dat"));
}

/// An optional archive that cannot be had is given up, its files with it: the set ends without
/// them, and says so in a line.
#[test]
fn an_optional_archive_that_cannot_be_had_is_left_out() {
    for dead in [true, false] {
        let mut fx = fixture();
        fx.set.archives[1].optional = true;
        if dead {
            fx.host.errors.insert("Seven003".into(), -9);
        } else {
            // Damaged past saving: the file is not the one listed.
            fx.set.archives[1].files[0].sha256 = Some("0".repeat(64));
        }
        let mut disk = Disk::default();
        let mut fetch = fetch_of(fx.set.clone(), &[]);
        let end = run(&mut fetch, &mut fx.host, &mut disk);
        assert_eq!(
            end,
            Step::Done {
                files: vec!["a.dat".into(), "b.dat".into()],
                note: Some("c.dat couldn't be downloaded; the set works without it.".into()),
            },
            "dead link: {dead}"
        );
        assert!(!disk.files.contains_key("c.dat"), "nothing of it is left");
        assert!(disk.archives.is_empty());
    }
    // An optional archive had is kept like any other.
    let mut fx = fixture();
    fx.set.archives[1].optional = true;
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Done { note, .. } = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("done")
    };
    assert_eq!(note, None);
    assert_eq!(disk.files, fx.plain);
}

/// Ranges come in over several connections at once and out of order; what is written is always
/// the archive's next bytes, at most the window is held, and the whole passes its MAC.
#[test]
fn ranges_out_of_order_are_written_in_order_and_pass_the_mac() {
    let mut fx = fixture();
    let mut disk = Disk::default();
    let window = TEST_WINDOW + 3 * TEST_RANGE;
    fx.host.window = Some(window);
    let mut fetch = Fetch::new(fx.set.clone(), &[]).with_ranges(TEST_RANGE, window);
    assert!(matches!(
        run(&mut fetch, &mut fx.host, &mut disk),
        Step::Done { .. }
    ));
    assert_eq!(disk.files, fx.plain);
    assert_eq!(fx.host.most_at_once, crate::fetch::CONNECTIONS);
    assert!(fx.host.most_held > TEST_RANGE, "{}", fx.host.most_held);
    let sizes: u64 = fx
        .set
        .archives
        .iter()
        .map(|a| a.size.div_ceil(TEST_RANGE))
        .sum();
    assert_eq!(
        fx.host.ranges.len() as u64,
        sizes,
        "each range asked for once"
    );
}

/// A byte damaged in a range that arrived ahead of the ones before it fails the MAC too.
#[test]
fn a_damaged_byte_in_a_later_range_is_caught_by_the_mac() {
    let mut fx = fixture();
    fx.host.flip = Some(2 * TEST_RANGE + 5);
    let mut disk = Disk::default();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    let Step::Failed(f) = run(&mut fetch, &mut fx.host, &mut disk) else {
        panic!("it failed")
    };
    assert_eq!(f.kind, FailureKind::Damaged);
    assert_eq!(fx.host.downloads.len(), 2);
}

/// Progress names the stage and counts its bytes, those held for later among them.
#[test]
fn progress_counts_the_download() {
    let fx = fixture();
    let mut fetch = fetch_of(fx.set.clone(), &[]);
    assert_eq!(fetch.progress().stage, crate::Stage::Asking);
    let Step::Api { .. } = fetch.step() else {
        panic!("asks first")
    };
    let f = &fx.host.files[0];
    fetch.api_reply(Ok(&format!(
        r#"[{{"s":{},"at":"{}","g":"https://gfs1.example/dl/x"}}]"#,
        f.cipher.len(),
        f.attributes()
    )));
    let Step::Download { connections, .. } = fetch.step() else {
        panic!("then downloads")
    };
    assert_eq!(connections, crate::fetch::CONNECTIONS);
    let Next::Range { id: first, url, .. } = fetch.next_range() else {
        panic!("a range")
    };
    assert_eq!(
        url,
        format!("https://gfs1.example/dl/x/0-{}", TEST_RANGE - 1)
    );
    let Next::Range { id: second, .. } = fetch.next_range() else {
        panic!("a second range")
    };
    let r = usize::try_from(TEST_RANGE).unwrap();
    let mut later = f.cipher[r..r + 500].to_vec();
    assert_eq!(fetch.range_piece(second, &mut later), Some(vec![]), "held");
    let mut piece = f.cipher[..1000].to_vec();
    let writes = fetch.range_piece(first, &mut piece).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!((writes[0].0, writes[0].1.len()), (0, 1000));
    let p = fetch.progress();
    assert_eq!(
        (p.stage, p.done, p.total, p.archive, p.archives),
        (crate::Stage::Downloading, 1500, f.cipher.len() as u64, 1, 2)
    );
}
