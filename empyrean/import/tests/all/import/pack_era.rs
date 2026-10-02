//! Divergence: V388
//! `--era` records the era a pack is built for in its header; the pack keeps the dump's version
//! row (its lineage); an end-of-retail pack is the pack built before packs recorded an era; an
//! unknown era or `--era` outside a build is a usage error.
//! Fixture: the patch fixture's base dump, reshaped as a 16PY dump, and the built importer.

use std::path::{Path, PathBuf};
use std::process::Command;

use empyrean_common::era::EraId;
use empyrean_content::pack::Pack;
use empyrean_content::{PackContent, WorldDatabase};

fn importer(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The fixture's base dump with a 16PY `version` row: base v0.8.8, no patch version.
fn sixteen_py_dump(dir: &Path) -> PathBuf {
    let base = std::fs::read(crate::patch_fixtures().join("base.sql")).unwrap();
    let from: &[u8] = b"(1,'ACE-World-Test','v0.0.1','2021-11-01 00:00:00')";
    let to: &[u8] = b"(1,'v0.8.8',NULL,'2025-10-04 17:55:37')";
    let at = base
        .windows(from.len())
        .position(|w| w == from)
        .expect("the fixture's version row");
    let mut dump = base[..at].to_vec();
    dump.extend_from_slice(to);
    dump.extend_from_slice(&base[at + from.len()..]);
    let path = dir.join("ACE-World-16PY-db-test.sql");
    std::fs::write(&path, dump).unwrap();
    path
}

fn build(sql: &Path, out: &Path, era: Option<&str>) -> std::process::Output {
    let mut args = vec![sql.as_os_str(), out.as_os_str()];
    args.insert(0, "--sql".as_ref());
    args.insert(2, "--out".as_ref());
    if let Some(era) = era {
        args.push("--era".as_ref());
        args.push(era.as_ref());
    }
    importer(&args)
}

#[test]
fn a_pack_records_the_era_it_was_built_for_and_the_dumps_lineage() {
    let dir = scratch("import-pack-era");
    let sql = sixteen_py_dump(&dir);
    let infiltration = dir.join("infiltration.pack");
    let out = build(&sql, &infiltration, Some("infiltration"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("for era infiltration"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let content = PackContent::open(&infiltration).unwrap();
    assert_eq!(content.era(), EraId::Infiltration);
    let version = content.get_version().expect("the version row");
    assert_eq!(
        (
            version.base_version.as_deref(),
            version.patch_version.as_deref()
        ),
        (Some("v0.8.8"), None),
        "the lineage is the dump's own version row"
    );
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("infiltration.pack.report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["era"], "infiltration");

    // Without --era the pack is for the end of retail, and it differs from the Infiltration pack
    // only in the era field and the content hash that covers it.
    let eor = dir.join("eor.pack");
    assert!(build(&sql, &eor, None).status.success());
    assert_eq!(Pack::open(&eor).unwrap().era(), EraId::Eor);
    let (a, b) = (
        std::fs::read(&eor).unwrap(),
        std::fs::read(&infiltration).unwrap(),
    );
    assert_eq!(a.len(), b.len());
    let differing: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
    assert!(
        differing
            .iter()
            .all(|&i| (0x40..0x60).contains(&i) || (0x74..0x78).contains(&i)),
        "{differing:x?}"
    );
    assert_eq!(&a[0x74..0x78], &[0, 0, 0, 0], "end of retail is era 0");
    assert_eq!(&b[0x74..0x78], &[1, 0, 0, 0]);

    // `--era eor` is the default.
    let named = dir.join("named.pack");
    assert!(build(&sql, &named, Some("EOR")).status.success());
    assert_eq!(std::fs::read(&named).unwrap(), a);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unknown_era_or_an_era_outside_a_build_is_a_usage_error() {
    let dir = scratch("import-pack-era-usage");
    let sql = sixteen_py_dump(&dir);
    let out_path = dir.join("tod.pack");
    let out = build(&sql, &out_path, Some("tod"));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--era wants eor or infiltration"));
    assert!(!out_path.exists());

    let pack = dir.join("p.pack");
    assert!(build(&sql, &pack, None).status.success());
    let check = importer(&[
        "--check".as_ref(),
        pack.as_os_str(),
        pack.as_os_str(),
        "--era".as_ref(),
        "infiltration".as_ref(),
    ]);
    assert_eq!(check.status.code(), Some(2));
    let corrections = importer(&[
        "--corrections".as_ref(),
        pack.as_os_str(),
        "--era".as_ref(),
        "eor".as_ref(),
    ]);
    assert_eq!(corrections.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_pack_with_an_era_this_build_does_not_know_is_refused() {
    let dir = scratch("import-pack-era-unknown");
    let sql = sixteen_py_dump(&dir);
    let pack = dir.join("p.pack");
    assert!(build(&sql, &pack, None).status.success());
    let mut bytes = std::fs::read(&pack).unwrap();
    bytes[0x74] = 9;
    let err = Pack::from_bytes(bytes).unwrap_err();
    assert!(err.to_string().contains("era 9"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
