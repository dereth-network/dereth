//! The cell files: landblocks, their placed objects, their light and their interior cells.

use auberean_assets::landblock::{Landblock, LandblockInfo};
use auberean_assets::{decode, encode};
use auberean_dat::locate::DatName;
use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.landblock.every-record-decodes
#[test]
fn every_landblock_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LANDBLOCK]);
}

/// Behaviour: data.landblock-info.every-record-decodes
#[test]
fn every_landblock_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LANDBLOCK_INFO]);
}

/// **A landblock and its placed objects' record, read and written again, are their own bytes: what a
/// world changes in one, it changes and no more.**
///
/// Behaviour: data.landblock-info.every-record-packs-back-to-its-bytes
#[test]
fn every_landblock_and_landblock_info_packs_back_to_its_bytes() {
    let (mut lands, mut infos) = (0usize, 0usize);
    for install in crate::common::installs() {
        let cells = install
            .dats
            .iter()
            .filter(|(n, _)| matches!(n, DatName::Cell(_)));
        for (_, dat) in cells {
            for e in dat.entries() {
                if e.id & 0xFFFF == 0xFFFF {
                    let bytes = dat.read(e.id).unwrap();
                    let r: Landblock = decode(e.id, &bytes, &install.ctx).unwrap();
                    assert_eq!(encode(e.id, &r), bytes, "{:#x}", e.id);
                    lands += 1;
                } else if e.id & 0xFFFF == 0xFFFE {
                    let bytes = dat.read(e.id).unwrap();
                    let r: LandblockInfo = decode(e.id, &bytes, &install.ctx).unwrap();
                    assert_eq!(encode(e.id, &r), bytes, "{:#x}", e.id);
                    infos += 1;
                }
            }
        }
    }
    assert!(lands > 60_000 && infos > 3_000, "{lands} {infos}");
}

/// Behaviour: data.light-info.every-record-decodes
#[test]
fn every_light_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LIGHT_INFO]);
}

/// Behaviour: data.cell.every-record-decodes
#[test]
fn every_interior_cell_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::CELL]);
}

/// The landblocks under the light records the January 2017 clients asked their servers for: the
/// first 21 refused every time they were asked (5,618 asks in all), the last sent once (and so
/// among the records the end-of-retail set holds).
const ASKED_LIGHT: [u16; 22] = [
    0x396E, 0x3C6F, 0xCCAF, 0xD7AA, 0xEE74, 0xEEA8, 0xF076, 0xF174, 0xF176, 0xF49F, 0xF4A0, 0xF571,
    0xF7A1, 0xF785, 0xF786, 0xF787, 0xF789, 0xF78A, 0xF78B, 0xF78C, 0xFC92, 0x3F9E,
];
const REFUSED: usize = 21;

/// A landblock got with the gathering of the records it needs reads its info, and through the
/// info its light record: each end-of-retail landblock under a light record the 2017 clients
/// asked their servers for names an info that names it, so a get of all 22 landblocks asks the
/// server for exactly the 21 the servers refused (the files lack them; the one they sent is
/// held). A refusal makes its info's get a partial success, which leaves the landblock's a
/// success, and the batch is done.
///
/// Behaviour: data.requests.a-landblock-gathers-its-info-and-light-record
#[test]
fn a_landblocks_get_asks_for_the_light_record_its_info_names_when_the_files_lack_it() {
    use std::sync::Arc;

    use auberean_assets::requests::{Event, Listener, Outcome, Requests, Sources};
    use auberean_assets::Database;
    use auberean_dat::disk::StoreSet;
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::testing::require_target_install;
    use auberean_dat::{DatFile, QualifiedId};

    let dir = require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: DatName| -> Arc<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| *n == want).expect("the file");
        Arc::new(DatFile::open(p).expect("opens"))
    };
    let files = vec![open(DatName::Portal), open(DatName::Cell(1))];
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 0x5000_0000);
    let stores = Arc::new(StoreSet::over_files(&files, &clock));
    let db = Database::over_stores(files, stores.clone());
    let src = Sources::new(&db, &stores);
    let mut r = Requests::new();
    r.server = true;
    let to = Listener { to: 1, user: 0 };
    let block = |b: u16, low: u32, kind: RecordType| QualifiedId {
        id: (u32::from(b) << 16) | low,
        kind: kind.0,
    };
    let landblocks: Vec<QualifiedId> = ASKED_LIGHT
        .iter()
        .map(|&b| block(b, 0xFFFF, RecordType::LANDBLOCK))
        .collect();
    let batch = r.async_get_many(src, 0x4000_0009, &landblocks, Some(to));
    for t in 1..=8 {
        r.use_time(src, f64::from(t));
    }
    let asked: Vec<QualifiedId> = r
        .out
        .iter()
        .filter_map(|e| match e {
            Event::Ask(q) => Some(*q),
            _ => None,
        })
        .collect();
    let mut lights: Vec<QualifiedId> = ASKED_LIGHT[..REFUSED]
        .iter()
        .map(|&b| block(b, 0xFFFC, RecordType::LIGHT_INFO))
        .collect();
    lights.sort();
    let mut sorted = asked.clone();
    sorted.sort();
    assert_eq!(sorted, lights, "asked of the server: {asked:08X?}");
    for light in &lights {
        let info = QualifiedId {
            id: light.id | 0xFFFE,
            kind: RecordType::LANDBLOCK_INFO.0,
        };
        assert_eq!(
            r.get_state(info),
            Some((Outcome::Success, 1)),
            "{info:08X?} read, waiting on its light record"
        );
        r.on_refused(src, *light);
    }
    r.use_time(src, 9.0);
    for light in &lights {
        let info = QualifiedId {
            id: light.id | 0xFFFE,
            kind: RecordType::LANDBLOCK_INFO.0,
        };
        assert_eq!(
            r.get_state(info),
            Some((Outcome::Partial, 0)),
            "{info:08X?}"
        );
    }
    assert_eq!(
        r.out.last(),
        Some(&Event::BatchDone {
            to: to.to,
            batch,
            outcome: Outcome::Success,
            user: 0
        })
    );
}

/// Behaviour: data.light-info.lit-vertices-unpack-to-their-count-and-pack-back
#[test]
fn every_lit_list_unpacks_to_its_count_and_packs_back() {
    use auberean_assets::landblock::LightInfo;
    use auberean_assets::rle;
    use auberean_assets::verify::type_in;
    let mut lists = 0usize;
    for install in crate::common::installs() {
        for (_, dat) in &install.dats {
            for e in dat.entries() {
                if type_in(dat, e.id) != Some(RecordType::LIGHT_INFO) {
                    continue;
                }
                let bytes = dat.read_entry(e).expect("reads");
                let l: LightInfo =
                    auberean_assets::decode(e.id, &bytes, &install.ctx).expect("decodes");
                let all = l
                    .lights
                    .iter()
                    .flat_map(|x| &x.lit)
                    .chain(l.sunlight.iter().flatten().flatten());
                for c in all {
                    if c.count <= 0 {
                        continue;
                    }
                    let words: Vec<u16> = c
                        .packed
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|w| u16::from_le_bytes(*w))
                        .collect();
                    let unpacked = rle::decode(&words);
                    assert_eq!(
                        unpacked.len(),
                        usize::try_from(c.count).unwrap(),
                        "{:#x} cell {:#x}",
                        e.id,
                        c.cell
                    );
                    assert_eq!(rle::encode(&unpacked).as_ref(), Some(&words), "{:#x}", e.id);
                    lists += 1;
                }
            }
        }
    }
    assert!(lists > 10_000, "{lists} lists");
}
