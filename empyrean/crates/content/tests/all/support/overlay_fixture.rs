//! Shared synthetic pack, overlay and patch-file fixtures.

pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::sync::Arc;

pub(crate) use empyrean_content::import::patch::{Input, InputKind};
pub(crate) use empyrean_content::import::{build, default_now, import, Build};
pub(crate) use empyrean_content::overlay::{BaseInputs, ContentOverlay, OverlayError};
pub(crate) use empyrean_content::pack::{Pack, TableId};
pub(crate) use empyrean_content::PackContent;

pub(crate) fn fixtures() -> PathBuf {
    crate::patch_fixtures()
}

pub(crate) fn base_sql() -> PathBuf {
    fixtures().join("base.sql")
}

pub(crate) fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("serv-content-8b2-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub(crate) fn base_pack() -> Pack {
    Pack::from_bytes(
        import(std::fs::read(base_sql()).unwrap().as_slice())
            .unwrap()
            .0,
    )
    .unwrap()
}

pub(crate) fn base_inputs() -> BaseInputs {
    BaseInputs {
        sql: base_sql(),
        patches: Vec::new(),
    }
}

/// A pack database over the fixture with an overlay at `dir/overlay.sqlite` in front of it.
pub(crate) fn open(dir: &Path) -> (PackContent, Arc<ContentOverlay>) {
    let db = PackContent::new(base_pack());
    let o = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        base_inputs(),
        db.base().pack().header(),
        default_now(),
    )
    .unwrap()
    .shared();
    db.base().attach_overlay(o.clone()).unwrap();
    (db, o)
}

pub(crate) fn fixture_sql(rel: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(rel)).unwrap()
}

#[allow(dead_code)] // shared with a test binary that does not read ints
pub(crate) fn ints(db: &PackContent, wcid: u32) -> Vec<(u16, i32)> {
    db.get_weenie(wcid)
        .unwrap()
        .weenie_properties_int
        .iter()
        .map(|r| (r.r#type, r.value))
        .collect()
}
