//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every record of every held historical dat capture, from the October 1999 retail CD to
//! December 2017 and in both containers, decodes in the layouts of its own container with the
//! cursor on the record's end, and every id has a type; the only exceptions are the five records
//! one damaged capture holds broken.
//! Fixture: the historical dat captures (`DERETH_TEST_DAT_CAPTURES_DIR`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use dereth_assets::ui::PropertyTypes;
use dereth_assets::{exhaustive_decode_file, Decode, MasterProperty, VerifyReport};
use dereth_dat::DatFile;
use dereth_primitives::DataId;

/// Every capture file held, as `(capture, file, records)`.
const CAPTURES: &[(&str, &str, usize)] = &[
    ("1999-10-09", "cell.dat", 131560),
    ("1999-10-09", "portal.dat", 17794),
    ("1999-10-31", "portal.dat", 17836),
    ("2000-08-22", "cell.dat", 166888),
    ("2000-08-22", "portal.dat", 21307),
    ("2000-10-01", "cell.dat", 141323),
    ("2000-10-08", "portal.dat", 22318),
    ("2000-12-12", "portal.dat", 24803),
    ("2000-12-31", "cell.dat", 182569),
    ("2001-02-11", "cell.dat", 32693),
    ("2001-03-26", "cell.dat", 189882),
    ("2001-03-26", "portal.dat", 25647),
    ("2001-07-05", "cell.dat", 106057),
    ("2001-07-05", "portal.dat", 26522),
    ("2001-09-12", "cell.dat", 243292),
    ("2001-09-12", "portal.dat", 30397),
    ("2002-01-30", "cell.dat", 125799),
    ("2002-01-30", "portal.dat", 31937),
    ("2002-06-07", "cell.dat", 337699),
    ("2002-10-21", "portal.dat", 36894),
    ("2002-12-29", "cell.dat", 206968),
    ("2002-12-29", "portal.dat", 37789),
    ("2003-08-15", "cell.dat", 309710),
    ("2004-01-28", "cell.dat", 375769),
    ("2004-01-28", "portal.dat", 43874),
    ("2004-06-16", "cell.dat", 197831),
    ("2004-06-27", "portal.dat", 46632),
    ("2004-08-13", "cell.dat", 404950),
    ("2004-09-01", "cell.dat", 477248),
    ("2004-09-01", "portal.dat", 48555),
    ("2005-02", "cell.dat", 524954),
    ("2005-02", "portal.dat", 51001),
    ("2005-06-02", "client_cell_1.dat", 594866),
    ("2005-06-02", "client_highres.dat", 2245),
    ("2005-06-02", "client_portal.dat", 59589),
    ("2006-01-12", "client_cell_1.dat", 621290),
    ("2006-01-12", "client_portal.dat", 62611),
    ("2008-10-10", "client_cell_1.dat", 617013),
    ("2009-04-22", "client_cell_1.dat", 559867),
    ("2009-04-22", "client_local_english.dat", 101),
    ("2009-04-22", "client_portal.dat", 69118),
    ("2009-06-10", "client_cell_1.dat", 597124),
    ("2009-06-10", "client_local_english.dat", 101),
    ("2009-08-01", "client_portal.dat", 69452),
    ("2010-06-09", "client_cell_1.dat", 750528),
    ("2010-06-09", "client_local_english.dat", 105),
    ("2010-06-09", "client_portal.dat", 71697),
    ("2010-08-15", "client_cell_1.dat", 639342),
    ("2010-09-05", "client_local_english.dat", 105),
    ("2010-09-05", "client_portal.dat", 71984),
    ("2012-04-28", "client_portal.dat", 77264),
    ("2012-07-27", "client_cell_1.dat", 340053),
    ("2012-07-27", "client_local_english.dat", 106),
    ("2012-07-27", "client_portal.dat", 77569),
    ("2012-08-19", "client_portal.dat", 78069),
    ("2013-08-25", "client_cell_1.dat", 684865),
    ("2013-09-06", "client_cell_1.dat", 797873),
    ("2013-09-06", "client_local_english.dat", 118),
    ("2013-09-06", "client_portal.dat", 78961),
    ("2013-12-01", "client_portal.dat", 79404),
    ("2014-01-09", "client_cell_1.dat", 715330),
    ("2014-01-09", "client_local_english.dat", 118),
    ("2017-01-27", "client_cell_1.dat", 805299),
    ("2017-02-18", "client_portal.dat", 79694),
    ("2017-12-27", "client_portal.dat", 79694),
];

/// The records the January 2014 cell capture holds broken: three of landblock `0x6757` whose
/// block chains end early, and one cell whose block holds other bytes. The same records are whole
/// in the captures on either side of it.
const DAMAGED: &[(&str, &str, u32)] = &[
    ("2014-01-09", "client_cell_1.dat", 0x5766_02B8),
    ("2014-01-09", "client_cell_1.dat", 0x6757_027A),
    ("2014-01-09", "client_cell_1.dat", 0x6757_FFFE),
    ("2014-01-09", "client_cell_1.dat", 0x6757_FFFF),
];

fn property_types(f: &DatFile) -> PropertyTypes {
    let mp = DataId(0x3900_0001);
    match f.read(mp) {
        Ok(b) => MasterProperty::decode_payload(mp, &b)
            .expect("the property map decodes")
            .property_types(),
        Err(_) => PropertyTypes::new(),
    }
}

fn census(path: &PathBuf, name: &str, portal: Option<&PathBuf>) -> VerifyReport {
    let f = DatFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    // A language file's interface records are typed by the property map of a portal of its time.
    let types = match portal {
        Some(p) => property_types(&DatFile::open(p).expect("the portal opens")),
        None => property_types(&f),
    };
    exhaustive_decode_file(&f, f.era(), name.contains("cell"), &types)
}

#[test]
fn every_record_of_every_held_capture_decodes() {
    let files = dereth_dat::testing::dat_captures_or_fail();
    let held: Vec<(&str, &str)> = files
        .iter()
        .map(|(c, n, _)| (c.as_str(), n.as_str()))
        .collect();
    let expected: Vec<(&str, &str)> = CAPTURES.iter().map(|(c, n, _)| (*c, *n)).collect();
    assert_eq!(
        held,
        expected,
        "the captures under {} are not the held set: every capture folder must be there",
        dereth_dat::testing::DAT_CAPTURES_DIR_VAR
    );
    // A language file is typed against the latest portal capture no later than it.
    let portal_for = |capture: &str| -> Option<PathBuf> {
        files
            .iter()
            .filter(|(c, n, _)| n == "client_portal.dat" && c.as_str() <= capture)
            .map(|(_, _, p)| p.clone())
            .next_back()
    };
    let reports: Vec<VerifyReport> = std::thread::scope(|s| {
        let chunks: Vec<_> = files.chunks(files.len().div_ceil(4)).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let portal_for = &portal_for;
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|(c, n, p)| {
                            let portal = n.contains("local").then(|| portal_for(c)).flatten();
                            census(p, n, portal.as_ref())
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a census thread panicked"))
            .collect()
    });
    let mut problems = BTreeMap::new();
    for ((capture, name, records), r) in CAPTURES.iter().zip(&reports) {
        let damaged: Vec<u32> = DAMAGED
            .iter()
            .filter(|(c, n, _)| c == capture && n == name)
            .map(|(_, _, id)| *id)
            .collect();
        let failed: Vec<u32> = r.failures.iter().map(|(id, _, _)| id.raw()).collect();
        if r.entries != *records
            || failed != damaged
            || !r.no_decoder.is_empty()
            || !r.untyped.is_empty()
            || !r.iteration_failures.is_empty()
            || r.decoded + r.iteration_lists + failed.len() != r.entries
        {
            problems.insert(
                format!("{capture}/{name}"),
                format!(
                    "{} of {} records decoded; failures {:?}; no decoder {:?}; untyped {:?}; \
                     iteration lists {:?}",
                    r.decoded,
                    r.entries,
                    r.failures.iter().take(5).collect::<Vec<_>>(),
                    r.no_decoder,
                    r.untyped.iter().take(5).collect::<Vec<_>>(),
                    r.iteration_failures,
                ),
            );
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
