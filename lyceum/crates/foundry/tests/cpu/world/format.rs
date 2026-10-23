//! Behaviour: none (tooling: source version checks and explicit upgrades over temporary worlds)

use crate::common::Fixture;
use lyceum_foundry::{format, source, world::World};

#[test]
fn upgrading_old_sources_keeps_comments_content_and_assigned_ids() {
    let f = Fixture::new("format-upgrade");
    let original = "# my world\nkey='my-world'\npacks=['pack']\n[ids]\nwcid=120000\n";
    let world = f.write("world.toml", original);
    let pack = f.write(
        "pack/pack.toml",
        "# my pack\nname='pack'\nversion='0.1.0'\n",
    );
    let lock = f.write("world.lock", "assigned ids stay byte-for-byte\n");
    let before = World::read(&world).unwrap();
    assert_eq!(format::upgrade(&world, true).unwrap().len(), 2);
    assert!(!world.with_extension("toml.before-upgrade").exists());
    assert_eq!(std::fs::read_to_string(&world).unwrap(), original);
    assert_eq!(format::upgrade(&world, false).unwrap().len(), 2);
    assert_eq!(World::read(&world).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(&lock).unwrap(),
        "assigned ids stay byte-for-byte\n"
    );
    assert_eq!(
        std::fs::read_to_string(world.with_extension("toml.before-upgrade")).unwrap(),
        original
    );
    for path in [&world, &pack] {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("# my"));
        assert_eq!(format::check(path, &text).unwrap(), format::CURRENT);
    }
    assert!(format::upgrade(&world, false).unwrap().is_empty());
}

#[test]
fn newer_worlds_and_packs_are_refused_before_unknown_fields_are_interpreted() {
    let f = Fixture::new("format-newer");
    let world = f.write(
        "world.toml",
        "format_version=999\nfuture_field=true\nkey='world'\n",
    );
    assert!(World::read(&world)
        .unwrap_err()
        .to_string()
        .contains("newer Lyceum"));
    f.write(
        "pack/pack.toml",
        "format_version=999\nfuture_field=true\nname='pack'\nversion='1'\n",
    );
    assert!(source::read_pack(&f.path().join("pack"))
        .unwrap_err()
        .to_string()
        .contains("newer Lyceum"));
}

#[test]
fn an_invalid_pack_prevents_any_world_upgrade_or_backup() {
    let f = Fixture::new("format-invalid");
    let original = "key='world'\npacks=['pack']\n";
    let world = f.write("world.toml", original);
    f.write(
        "pack/pack.toml",
        "format_version=999\nname='pack'\nversion='1'\n",
    );
    assert!(format::upgrade(&world, false).is_err());
    assert_eq!(std::fs::read_to_string(&world).unwrap(), original);
    assert!(!world.with_extension("toml.before-upgrade").exists());
}

#[test]
fn an_existing_different_backup_is_never_overwritten() {
    let f = Fixture::new("format-backup");
    let world = f.write("world.toml", "key='world'\n");
    let backup = f.write("world.toml.before-upgrade", "older content");
    assert!(format::upgrade(&world, false).is_err());
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "older content");
    assert_eq!(std::fs::read_to_string(world).unwrap(), "key='world'\n");
}

#[test]
fn an_upgrade_resumes_after_only_one_source_was_replaced() {
    let f = Fixture::new("format-resume");
    let world = f.write("world.toml", "key='world'\npacks=['pack']\n");
    let original = "name='pack'\nversion='1'\n";
    f.write("pack/pack.toml.before-upgrade", original);
    f.write("pack/pack.toml", &format!("format_version=1\n{original}"));
    assert_eq!(format::upgrade(&world, false).unwrap(), vec![world.clone()]);
    assert!(format::upgrade(&world, false).unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(f.path().join("pack/pack.toml.before-upgrade")).unwrap(),
        original
    );
}
