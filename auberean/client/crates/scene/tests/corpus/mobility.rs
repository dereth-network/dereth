//! Whether the objects the 2017 sessions create move, by the qualities record each names.
//! Fixture: the end-of-retail install and the capture index.

use std::collections::BTreeSet;

use auberean_assets::script::Qualities as Record;
use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_scene::profile::{is_mobile, mobility_record};
use auberean_world::records::WorldRecords;

/// The messages read: objects created.
const CREATE: u32 = 0x10058;

/// Every object the 2017 sessions create names an entity template whose qualities record the data
/// holds, so none is still for want of its record; 342 of those 3,282 records say what they
/// describe does not move (the corpse's among them), and so 40,595 of the 160,633 creations are
/// still.
///
/// Behaviour: desktop.others.whether-a-recorded-object-moves-is-its-records-word
#[test]
fn whether_a_recorded_object_moves_is_its_records_word() {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
    let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    let r = WorldRecords::new(portal, cell, open(&|n| *n == DatName::HighRes), ctx);
    let index = auberean_testkit::data::require_capture_index();
    let (mut creates, mut missing, mut still) = (0u64, 0u64, 0u64);
    let (mut records, mut still_records) = (BTreeSet::new(), BTreeSet::new());
    auberean_testkit::captures::messages(&index, &[CREATE], &[2017], |m| {
        let Ok(ServerMessage::CreateObject(c)) = ServerMessage::decode(&m.raw) else {
            return;
        };
        creates += 1;
        let id = mobility_record(&c.weenie);
        records.insert(id);
        let record = r.get::<Record>(id);
        if record.is_none() {
            missing += 1;
        }
        if !is_mobile(&c.weenie, |_| record) {
            still += 1;
            still_records.insert(id);
        }
    })
    .expect("the index");
    eprintln!(
        "{creates} creations naming {} records ({missing} missing); {still} still, by {} records",
        records.len(),
        still_records.len()
    );
    assert_eq!((creates, records.len()), (160_633, 3_282));
    assert_eq!(missing, 0, "every record named is held");
    assert!(
        still_records.contains(&0x8100_0539),
        "the corpse's record says still"
    );
    assert_eq!((still, still_records.len()), (40_595, 342));
}
