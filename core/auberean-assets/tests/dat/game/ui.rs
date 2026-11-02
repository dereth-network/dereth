//! Layouts, user-interface scenes, fonts, key maps and action maps.

use auberean_assets::ui::{Font, UiLayout};
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

/// Every font of every checked set carries the map of its characters once read, which finds
/// each of its glyphs by its code point (the last of one held twice); a character none holds is
/// drawn as the font's question mark.
///
/// Behaviour: data.font.a-character-finds-its-glyph-or-the-question-marks
#[test]
fn every_font_finds_its_characters_through_its_map() {
    let mut fonts = 0;
    for install in crate::common::installs() {
        for (name, dat) in &install.dats {
            // fonts are the portal file's and the language files'
            if !matches!(
                name,
                auberean_dat::locate::DatName::Portal | auberean_dat::locate::DatName::Local(_)
            ) {
                continue;
            }
            for e in dat.entries_in(0x4200_0000, 0x42FF_FFFF) {
                let bytes = dat.read_entry(e).expect("read the record");
                let f = auberean_assets::decode::<Font>(e.id, &bytes, &install.ctx)
                    .expect("the font reads");
                let m = f
                    .character_map
                    .as_ref()
                    .unwrap_or_else(|| panic!("{:08X}: a map", e.id));
                for g in &f.glyphs {
                    let last = f.glyphs.iter().rposition(|h| h.code_point == g.code_point);
                    assert_eq!(
                        m.place(g.code_point),
                        last,
                        "{:08X} {:04X}",
                        e.id,
                        g.code_point
                    );
                }
                let question = f.glyphs.iter().rposition(|g| g.code_point == 0x3F);
                assert_eq!(
                    f.char_desc(0xFFFF).map(std::ptr::from_ref),
                    question.map(|q| std::ptr::from_ref(&f.glyphs[q])),
                    "{:08X}",
                    e.id
                );
                fonts += 1;
            }
        }
    }
    assert!(fonts > 10, "{fonts} fonts");
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
