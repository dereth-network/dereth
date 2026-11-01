//! The checked set of data files (the end of retail), opened once per test binary, and the
//! exact-decode assertion every layout test makes.

use std::sync::OnceLock;

use auberean_assets::verify::{verify_dat, Tally};
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, CHECKED};
use auberean_dat::{DatFile, RecordType};

/// One checked set: its data files and its decoding context.
pub struct Install {
    pub set: &'static str,
    pub dats: Vec<(DatName, DatFile)>,
    pub ctx: Context,
}

/// Every checked set; a test fails when one is not found.
pub fn installs() -> &'static [Install] {
    static ALL: OnceLock<Vec<Install>> = OnceLock::new();
    ALL.get_or_init(|| {
        CHECKED
            .iter()
            .map(|v| {
                let dir = require_install(v);
                let dats: Vec<(DatName, DatFile)> = install_dats(&dir)
                    .expect("list the install")
                    .into_iter()
                    .map(|(n, p)| {
                        let d =
                            DatFile::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                        (n, d)
                    })
                    .collect();
                let portal = &dats
                    .iter()
                    .find(|(n, _)| *n == DatName::Portal)
                    .expect("a portal file")
                    .1;
                let ctx = Context::from_portal(portal).expect("the property list");
                Install { set: v, dats, ctx }
            })
            .collect()
    })
}

/// One set's tally for some types.
pub fn tally(install: &Install, types: &[RecordType]) -> Tally {
    let mut t = Tally::default();
    for (_, dat) in &install.dats {
        for ty in types {
            if let Some(x) = verify_dat(dat, &install.ctx, Some(*ty)).get(ty) {
                t.merge(x);
            }
        }
    }
    t
}

/// Every record of the types, in every checked set, decodes with every byte consumed, and at
/// least one such record exists.
pub fn assert_every_record_decodes(types: &[RecordType]) {
    let all = installs();
    let mut seen = 0;
    for i in all {
        let t = tally(i, types);
        seen += t.total;
        assert!(
            t.all_exact(),
            "{}: {types:?}: {}/{} exact; {:?}",
            i.set,
            t.exact,
            t.total,
            t.failures
        );
    }
    assert!(seen > 0, "no record of {types:?} in any checked set");
}
