//! Behaviour: none (the launcher's download of a data set from the community's archive: tooling
//! around the client, run against a host made in the test).

use std::sync::Mutex;

use dereth_datfetch::testing::{host, Hosted};
use sha2::{Digest, Sha256};

use super::*;

/// The archive's host as the test runs it: one file, its API answer and its bytes, with how many
/// bytes each download sends before it stops.
struct FakeHost {
    file: Hosted,
    cut_after: Mutex<Option<u64>>,
    gets: Mutex<Vec<String>>,
}

impl Net for FakeHost {
    fn post(&self, _url: &str, _body: &str, _hashcash: Option<&str>) -> Result<Answer, String> {
        Ok(Answer::Body(self.file.api_reply(&format!(
            "https://gfs.example/dl/{}",
            self.file.handle
        ))))
    }

    fn get(&self, url: &str, piece: &mut dyn FnMut(&mut [u8]) -> bool) -> Result<u16, String> {
        self.gets.lock().unwrap().push(url.to_owned());
        // `<url>/<from>-<to>`: the range still to come.
        let range = url.rsplit('/').next().unwrap();
        let (from, to) = range.split_once('-').unwrap();
        let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
        let mut end = to + 1;
        if let Some(cut) = self.cut_after.lock().unwrap().take() {
            end = end.min(from + usize::try_from(cut).unwrap());
        }
        for chunk in self.file.cipher[from..end].chunks(4093) {
            let mut bytes = chunk.to_vec();
            if !piece(&mut bytes) {
                break;
            }
        }
        Ok(200)
    }
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// A Launch pair, zipped and kept by the host, and the list that offers it.
fn launch_pair(dir: &Path) -> (FakeHost, Catalogue) {
    let made = dir.join("made");
    dereth_launch::testing::write_classic_set(&made, 105, 67);
    let portal = std::fs::read(made.join("portal.dat")).unwrap();
    let cell = std::fs::read(made.join("cell.dat")).unwrap();
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in [("Launch/portal.dat", &portal), ("Launch/cell.dat", &cell)] {
        w.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        w.write_all(bytes).unwrap();
    }
    // Something else in the archive, stored, so a download of it comes in many pieces.
    let mut x = 0x2545_f491_u32;
    let noise: Vec<u8> = (0..96 * 1024)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x.to_le_bytes()[0]
        })
        .collect();
    w.start_file(
        "Launch/readme.bin",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    w.write_all(&noise).unwrap();
    let archive = w.finish().unwrap().into_inner();
    let file = host("AbCdEfGh", "launch.zip", &archive, 3);
    let list = serde_json::json!({
        "version": 1,
        "guide": "https://www.accpp.net/manual-installation",
        "sets": [{
            "id": "launch",
            "label": "Launch",
            "kind": "classic",
            "page": "https://www.accpp.net/archive",
            "iterations": { "portal": 105, "cell": 67 },
            "archives": [{
                "format": "zip",
                "size": archive.len(),
                "mirrors": [{ "link": file.link() }],
                "files": [
                    { "name": "portal.dat", "size": portal.len(), "sha256": sha(&portal) },
                    { "name": "cell.dat", "size": cell.len(), "sha256": sha(&cell) }
                ]
            }]
        }]
    });
    let catalogue = Catalogue::parse(&list.to_string()).unwrap();
    (
        FakeHost {
            file,
            cut_after: Mutex::new(None),
            gets: Mutex::new(Vec::new()),
        },
        catalogue,
    )
}

#[test]
fn a_set_is_downloaded_unpacked_checked_and_its_folder_left_whole() {
    let dir = tempfile::tempdir().unwrap();
    let (net, catalogue) = launch_pair(dir.path());
    let (source, kind) = super::source(&catalogue, "launch").unwrap();
    let place = Place::in_library(&dir.path().join("library"), "launch");
    let seen = Mutex::new(Vec::new());
    let end = run(source, kind, &place, &net, &AtomicBool::new(false), &|p| {
        seen.lock().unwrap().push(p.stage)
    });
    assert_eq!(end, End::Done(None));
    assert!(!place.set.join(INCOMPLETE).exists(), "the folder is whole");
    let files = read_set(SetKind::Classic, &place.set);
    assert_eq!(datset::pair_era(&files).map(|e| e.name()), Some("launch"));
    assert!(
        std::fs::read_dir(&place.archives).unwrap().next().is_none(),
        "the archive goes once it is unpacked"
    );
    assert!(seen.lock().unwrap().contains(&Stage::Downloading));
}

/// A download stopped part way keeps what it downloaded, and the next try asks only for the rest.
#[test]
fn a_stopped_download_goes_on_from_where_it_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let (net, catalogue) = launch_pair(dir.path());
    let (source, kind) = super::source(&catalogue, "launch").unwrap();
    let place = Place::in_library(&dir.path().join("library"), "launch");
    let cancel = AtomicBool::new(false);
    *net.cut_after.lock().unwrap() = Some(1000);
    // The connection closes early: the download is tried again from byte 1000, and finishes.
    let end = run(source, kind, &place, &net, &cancel, &|_| {});
    assert_eq!(end, End::Done(None));
    let gets = net.gets.lock().unwrap().clone();
    assert_eq!(gets.len(), 2, "{gets:?}");
    assert!(gets[1].contains("/1000-"), "{gets:?}");
}

/// Stopped by the player, a download ends at once, its folder is removed and its archive kept.
#[test]
fn a_cancelled_download_leaves_no_set_and_keeps_its_archive() {
    let dir = tempfile::tempdir().unwrap();
    let (net, catalogue) = launch_pair(dir.path());
    let (source, kind) = super::source(&catalogue, "launch").unwrap();
    let place = Place::in_library(&dir.path().join("library"), "launch");
    let cancel = AtomicBool::new(false);
    let end = run(source, kind, &place, &net, &cancel, &|p| {
        if p.stage == Stage::Downloading && p.done > 0 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert_eq!(end, End::Cancelled);
    assert!(!place.set.exists(), "no half a set");
    let kept = std::fs::metadata(place.archives.join(&source.archive_names()[0]))
        .unwrap()
        .len();
    assert!(kept > 0, "what was downloaded is kept");
}

/// Files that are not what the list says are refused, and leave nothing for the Library.
#[test]
fn a_set_whose_files_are_not_the_ones_listed_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (net, catalogue) = launch_pair(dir.path());
    let text =
        serde_json::to_string(&catalogue)
            .unwrap()
            .replacen("\"portal\":105", "\"portal\":102", 1);
    let catalogue = Catalogue::parse(&text).unwrap();
    let (source, kind) = super::source(&catalogue, "launch").unwrap();
    let place = Place::in_library(&dir.path().join("library"), "launch");
    let end = run(source, kind, &place, &net, &AtomicBool::new(false), &|_| {});
    assert!(
        matches!(&end, End::Failed(why) if why.contains("not 102")),
        "{end:?}"
    );
    assert!(!place.set.exists());
}

/// Through the launcher: the set downloaded joins its Library as one the launcher made, and the
/// snapshot says it is done.
#[test]
fn a_downloaded_set_joins_the_library_as_one_the_launcher_made() {
    let dir = tempfile::tempdir().unwrap();
    let (net, catalogue) = launch_pair(dir.path());
    let b = Backend::new(
        dereth_launch::folders::Folders::single(dir.path().join("state")),
        Vec::new(),
        None,
        Box::new(dereth_launch::vault::MemoryVault::default()),
        "memory",
        None,
    );
    let shared: Shared = Arc::new(Mutex::new(b));
    let worker = Backend::start_get_dats(&shared, Arc::new(net), &catalogue, "launch").unwrap();
    worker.join().unwrap();
    let b = crate::backend::lock(&shared);
    let snap = b.snapshot();
    assert_eq!(
        snap.downloads.status.as_ref().map(|s| s.stage),
        Some("done")
    );
    let set = snap
        .state
        .dat_sets
        .iter()
        .find(|s| s.kind == SetKind::Classic)
        .expect("the set is in the Library");
    assert!(set.created_by_launcher);
    assert_eq!(set.pair_era().map(|e| e.name()), Some("launch"));
}

use crate::backend::{Backend, Shared};
use std::sync::Arc;

/// One real download of a set the built-in list offers, from the community's archive, into a
/// folder of the runner's: `DERETH_TEST_GET_DATS` names the set, `DERETH_TEST_GET_DATS_DIR` the
/// folder. Never run by default: it reaches the network and writes hundreds of megabytes.
#[test]
#[ignore = "downloads from the community's archive: DERETH_TEST_GET_DATS and DERETH_TEST_GET_DATS_DIR"]
fn a_real_set_downloads_from_the_community_archive() {
    let id = std::env::var("DERETH_TEST_GET_DATS").expect("DERETH_TEST_GET_DATS");
    let dir =
        PathBuf::from(std::env::var("DERETH_TEST_GET_DATS_DIR").expect("DERETH_TEST_GET_DATS_DIR"));
    let catalogue = Catalogue::built_in();
    let (source, kind) = super::source(&catalogue, &id).expect("a set on offer");
    let place = Place::in_library(&dir, &id);
    let last = Mutex::new(Stage::Asking);
    let began = std::time::Instant::now();
    let end = run(
        source,
        kind,
        &place,
        &Https::new(),
        &AtomicBool::new(false),
        &|p| {
            let mut l = last.lock().unwrap();
            if *l != p.stage {
                eprintln!(
                    "{id}: {} (archive {} of {}) at {:.1} s",
                    p.stage.name(),
                    p.archive,
                    p.archives,
                    began.elapsed().as_secs_f64()
                );
                *l = p.stage;
            }
        },
    );
    assert_eq!(end, End::Done(None));
    for f in read_set(kind, &place.set) {
        eprintln!(
            "{id}: {} {} bytes, iteration {:?}",
            f.file_name, f.size, f.iterations
        );
    }
}
