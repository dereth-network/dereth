//! The script library, saved script states, qualities and quality filters.

use auberean_assets::script::QualityFilter;
use auberean_assets::verify::type_in;
use auberean_assets::{decode, encode};
use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.script-library.every-record-decodes
#[test]
fn the_script_library_decodes_exactly_section_by_section() {
    assert_every_record_decodes(&[RecordType::SCRIPT_LIBRARY]);
}

/// Behaviour: data.script-state.every-record-decodes
#[test]
fn every_saved_script_state_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SCRIPT_STATE]);
}

/// Behaviour: data.qualities.every-record-decodes
#[test]
fn every_qualities_record_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::QUALITIES]);
}

/// Behaviour: data.quality-filter.every-record-decodes
#[test]
fn every_quality_filter_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::QUALITY_FILTER]);
}

/// Behaviour: data.quality-filter.packs-back-to-its-own-bytes
#[test]
fn every_quality_filter_packs_back_to_its_own_bytes() {
    let mut seen = 0;
    for install in crate::common::installs() {
        for (_, dat) in &install.dats {
            for e in dat.entries() {
                if type_in(dat, e.id) != Some(RecordType::QUALITY_FILTER) {
                    continue;
                }
                let bytes = dat.read_entry(e).expect("reads");
                let f: QualityFilter = decode(e.id, &bytes, &install.ctx).expect("decodes");
                assert_eq!(encode(e.id, &f), bytes, "{:#x}", e.id);
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "the data holds quality filters");
}
