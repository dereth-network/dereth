//! The interface's records once a client is here: string references written through the client's
//! data cache, and the text elements' string downloads started there for references its files
//! cannot write.

use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::runtime::cache::DataCache;
use auberean_client_sdk::runtime::strings::StringDownloads;
use auberean_client_sdk::text::StringInfo;
use auberean_interface::strings::TextAssets;
use auberean_ui::Assets;

use crate::screens::data;

/// A table no install holds.
const ABSENT: u32 = 0x2500_FFF0;

/// The client's string downloads over the install's portal and English files.
fn downloads() -> StringDownloads {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: DatName| {
        let (_, p) = found.iter().find(|(n, _)| *n == want).expect("the file");
        Arc::new(DatFile::open(p).expect("open it"))
    };
    let files = vec![
        open(DatName::Portal),
        open(DatName::Local("English".into())),
    ];
    let s = StringDownloads::default();
    s.set_cache(&DataCache::new(files, Arc::new(AtomicU32::new(0))));
    s
}

/// Before a client is here the interface's text is the install's and nothing is asked for; once
/// one is, a reference the files write is written through its data cache, as before, and asks for
/// nothing, while one naming a table the files lack starts a string download there, which waits.
///
/// Behaviour: desktop.text.references-the-files-lack-are-asked-for-through-the-clients-cache
#[test]
fn references_the_files_lack_are_asked_for_through_the_clients_cache() {
    let text = TextAssets::new(data().2.clone());
    let held = StringInfo::entry(0x2500_18A4, 0x4285_FF9A);
    let absent = StringInfo::entry(ABSENT, 7);
    let install = text.write(&held).expect("the install writes it");
    assert!(install.starts_with("The Lost Artifacts"), "{install:?}");
    assert_eq!(text.start_string_download(&absent), None, "no client yet");
    let strings = downloads();
    text.connect(&strings);
    assert_eq!(text.write(&held).as_deref(), Some(install.as_str()));
    assert_eq!(text.start_string_download(&held), None);
    let n = text.start_string_download(&absent).expect("asked for");
    assert!(text.string_download_waiting(n));
    text.release_string_download(n);
    assert!(strings.is_empty());
}
