//! Every string table entry of the end-of-retail English data, rendered, and English's number
//! rules.

use std::sync::Arc;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_text::meta::{render_string, RenderCode};
use auberean_text::number::{int_to_string, TableNumbers};
use auberean_text::{DatSource, Source};

fn source() -> (DatSource, Arc<DatFile>) {
    let dir = require_target_install();
    let found = install_dats(&dir).expect("the install lists");
    let open = |want: &DatName| {
        let (_, p) = found
            .iter()
            .find(|(n, _)| n == want)
            .expect("the data file is there");
        Arc::new(DatFile::open(p).expect("the data file opens"))
    };
    let portal = open(&DatName::Portal);
    let english = open(&DatName::Local("English".into()));
    (DatSource::new(vec![portal, Arc::clone(&english)]), english)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Every entry of every English string table renders with plain values (one, two, nothing) in
/// every variable. English (the string state the table mapping names) writes numbers in base 10,
/// at least five digits when zeros lead, none leading, in groups of three with a comma and a
/// point, negative after a minus; its hexadecimal numerals repeat the zero after the nine, so ten
/// is written as a zero; only one takes the singular.
///
/// Behaviour: text.data.every-entry-renders
#[test]
fn every_english_entry_renders_and_english_writes_numbers_as_its_rules_say() {
    let (mut src, english) = source();
    let lang = src.language().expect("the language's rules");
    assert_eq!(
        (
            lang.base,
            lang.decimal_digits,
            lang.leading_zero,
            lang.grouping_size
        ),
        (10, 5, 0, 3)
    );
    assert_eq!(lang.numerals, wide("01234567890ABCDEF"));
    assert_eq!(
        (
            &lang.decimal_separator,
            &lang.grouping_separator,
            &lang.negative_format
        ),
        (&wide("."), &wide(","), &wide("-%s"))
    );
    assert_eq!(lang.singular, [false, true, false, false, false]);
    let t = TableNumbers::default();
    let int = |v: i32, base: i16| {
        int_to_string(t, Some(&lang), v, 1, base).map(|u| String::from_utf16_lossy(&u))
    };
    assert_eq!(int(-1_234_567, -1).as_deref(), Some("-1,234,567"));
    assert_eq!(
        int(26, 16).as_deref(),
        Some("0x10"),
        "ten is the second zero"
    );
    assert_eq!(int(255, 16).as_deref(), Some("0xEE"));

    let ids: Vec<u32> = english
        .entries_in(0x2500_0000, 0x26FF_FFFF)
        .map(|e| e.id)
        .collect();
    let mut entries = 0;
    let mut failed = Vec::new();
    for id in &ids {
        let t = src.table(*id).expect("the table decodes");
        for (token, e) in t.entries() {
            entries += 1;
            for v in ["1", "2", ""] {
                let values: Vec<Vec<u16>> = e.variables.iter().map(|_| wide(v)).collect();
                let mut out = Vec::new();
                let mut outside = Vec::new();
                let err = render_string(
                    &e.fragments,
                    &values,
                    &e.names,
                    &mut out,
                    &mut outside,
                    1,
                    Some(&lang),
                );
                if err.code != RenderCode::Success {
                    failed.push((*id, token, v, err));
                }
            }
        }
    }
    assert_eq!((ids.len(), entries), (11_488, 52_270));
    assert!(
        failed.is_empty(),
        "{} failed: {:?}",
        failed.len(),
        &failed[..failed.len().min(5)]
    );
}
