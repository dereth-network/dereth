//! Divergence: V373
//! The importer's default --now is 2000-01-01;
//! SOURCE_DATE_EPOCH does not change them; --now does.
//! Fixture: temporary content files and the built importer.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A value that would move the clock to 2001-01-01 if it were read.
const EPOCH_2001: &str = "978307200";

fn fixtures() -> PathBuf {
    crate::patch_fixtures()
}

/// Runs `empyrean-import` over the patch fixtures (a JSON weenie, which is stamped with the
/// clock), with `now` as `--now` and `epoch` as `SOURCE_DATE_EPOCH`, and returns the pack.
fn import(dir: &Path, name: &str, now: Option<&str>, epoch: Option<&str>) -> Vec<u8> {
    let out_path = dir.join(name);
    let mut command = Command::new(env!("CARGO_BIN_EXE_empyrean-import"));
    command
        .arg("--sql")
        .arg(fixtures().join("base.sql"))
        .arg("--json")
        .arg(fixtures().join("json/weenies/00100 Test Drudge.json"))
        .arg("--out")
        .arg(&out_path)
        .arg("--report")
        .arg(dir.join(format!("{name}.json")));
    if let Some(now) = now {
        command.args(["--now", now]);
    }
    command.env_remove("SOURCE_DATE_EPOCH");
    if let Some(epoch) = epoch {
        command.env("SOURCE_DATE_EPOCH", epoch);
    }
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read(&out_path).unwrap()
}

#[test]
fn the_importer_ignores_source_date_epoch_and_honours_now() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("empyrean-content-clock");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let plain = import(&dir, "plain.pack", None, None);
    let with_epoch = import(&dir, "epoch.pack", None, Some(EPOCH_2001));
    let fixed = import(&dir, "fixed.pack", Some("2000-01-01 00:00:00"), None);
    let later = import(
        &dir,
        "later.pack",
        Some("2001-01-01 00:00:00"),
        Some(EPOCH_2001),
    );
    let later_plain = import(&dir, "later-plain.pack", Some("2001-01-01"), None);
    assert!(
        plain == with_epoch,
        "SOURCE_DATE_EPOCH must not change the pack"
    );
    assert!(plain == fixed, "the default --now is 2000-01-01 00:00:00");
    assert!(later != plain, "--now is honoured");
    assert!(later == later_plain, "--now alone sets the clock");
    let _ = std::fs::remove_dir_all(&dir);
}
