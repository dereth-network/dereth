//! Layouts, user-interface scenes, fonts, key maps and action maps.

use auberean_assets::ui::UiLayout;
use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.layout.every-record-decodes
#[test]
fn every_layout_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::UI_LAYOUT]);
}

/// Every layout of every language file, read and written back, gives its record's bytes, and
/// the size it says it takes is that many bytes.
///
/// Behaviour: data.layout.every-record-packs-back-to-its-bytes
#[test]
fn every_layout_packs_back_to_its_bytes() {
    let mut n = 0;
    for install in crate::common::installs() {
        for (name, dat) in &install.dats {
            if !matches!(name, auberean_dat::locate::DatName::Local(_)) {
                continue;
            }
            for e in dat.entries_in(0x2200_0000, 0x22FF_FFFF) {
                let bytes = dat.read_entry(e).expect("read the record");
                let layout = auberean_assets::decode::<UiLayout>(e.id, &bytes, &install.ctx)
                    .expect("the layout reads");
                assert_eq!(layout.pack(e.id), bytes, "{:08X} packs back", e.id);
                assert_eq!(layout.pack_size(), bytes.len(), "{:08X} size", e.id);
                n += 1;
            }
        }
    }
    assert!(n >= 14, "{n} layouts");
}

/// Behaviour: data.ui-scene.every-record-decodes
#[test]
fn every_ui_scene_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::UI_SCENE]);
}

/// Behaviour: data.font.every-record-decodes
#[test]
fn every_font_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::FONT, RecordType::FONT_LOCAL]);
}

/// Behaviour: data.key-map.every-record-decodes
#[test]
fn every_key_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::KEY_MAP]);
}

/// Behaviour: data.action-map.every-record-decodes
#[test]
fn every_action_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ACTION_MAP]);
}
