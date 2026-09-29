//! Divergence: V3
//! Building over an existing world.pack keeps the old pack beside it as a timestamped backup, and
//! only the newest three; a build that fails leaves the old pack in place and makes no backup.
//! Fixture: temporary content files and the built importer.

use std::path::{Path, PathBuf};
use std::process::Command;

fn build(sql: &Path, out: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
        .arg("--sql")
        .arg(sql)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap()
}

fn backups_of(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("world.pack.backup-"))
        .collect();
    v.sort();
    v
}

#[test]
fn rebuilding_a_pack_keeps_the_old_one_as_a_backup() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("import-pack-backups");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sql = crate::patch_fixtures().join("base.sql");
    let out = dir.join("world.pack");

    // A first build replaces nothing.
    let first = build(&sql, &out);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(backups_of(&dir).is_empty());

    // Over an older pack (here, bytes standing for one): the old bytes are kept.
    std::fs::write(&out, b"the previous release's pack").unwrap();
    let second = build(&sql, &out);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let kept = backups_of(&dir);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].ends_with('Z'), "{kept:?}");
    assert_eq!(
        std::fs::read(dir.join(&kept[0])).unwrap(),
        b"the previous release's pack"
    );
    assert_ne!(std::fs::read(&out).unwrap(), b"the previous release's pack");
    assert!(String::from_utf8_lossy(&second.stdout).contains("kept the previous pack as"));

    // Three older backups and one more build: the newest three remain.
    for stamp in ["20200101T000000Z", "20210101T000000Z", "20220101T000000Z"] {
        std::fs::write(dir.join(format!("world.pack.backup-{stamp}")), b"old").unwrap();
    }
    let third = build(&sql, &out);
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    let kept = backups_of(&dir);
    assert_eq!(kept.len(), 3, "{kept:?}");
    assert_eq!(kept[0], "world.pack.backup-20220101T000000Z", "{kept:?}");

    // A build that fails writes nothing: no backup, the pack as it was.
    let before = std::fs::read(&out).unwrap();
    let missing = build(&dir.join("no-such-dump.sql"), &out);
    assert!(!missing.status.success());
    assert_eq!(backups_of(&dir), kept);
    assert_eq!(std::fs::read(&out).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}
