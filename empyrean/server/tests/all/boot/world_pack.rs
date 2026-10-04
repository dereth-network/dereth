//! Vectors: local synthetic packs and configured boot-path cases in this module
//! Divergence: V388
//! World.pack path from config else default; a pack on disk is the world DB; start-up corrections
//! line; missing/corrupt pack boots empty; another release's pack is refused with its rebuild
//! command; default found beside the exe; overlay opened in front and resolves beside config.
//! Fixture: isolated configuration paths and synthetic server state.

use std::path::{Path, PathBuf};

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_content::models::world::Quest;
use empyrean_content::MemContent;
use empyrean_server::world_pack::{self, WorldPackStatus};

fn temp_path(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("empyrean-server-world-pack-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join(name)
}

fn quest(id: u32, name: &str) -> Quest {
    Quest {
        id,
        name: name.to_owned(),
        max_solves: 1,
        ..Quest::default()
    }
}

fn write_pack(path: &Path) -> Vec<u8> {
    let mem = MemContent::new()
        .quest(quest(1, "F16QuestA"))
        .quest(quest(2, "F16QuestB"));
    let (bytes, _) = mem.content().to_pack([7; 16]).expect("pack");
    std::fs::write(path, &bytes).expect("write pack");
    bytes
}

#[test]
fn the_pack_path_comes_from_the_config_then_the_default() {
    let key = "C:/config/world.pack";
    assert_eq!(
        world_pack::world_pack_path(key),
        PathBuf::from("C:/config/world.pack")
    );
    assert_eq!(
        world_pack::world_pack_path("  "),
        PathBuf::from("./world.pack")
    );
    assert_eq!(
        world_pack::world_pack_path(""),
        PathBuf::from("./world.pack")
    );
    assert_eq!(world_pack::DEFAULT_WORLD_PACK_PATH, "./world.pack");
}

#[test]
fn world_pack_path_is_a_field_of_the_server_section() {
    let config = empyrean_common::toml_config::from_toml_str(
        "[server]\nworld_pack_path = 'D:\\data\\world.pack'\n",
    )
    .expect("valid")
    .config;
    assert_eq!(config.server.world_pack_path, r"D:\data\world.pack");
    let parsed = empyrean_common::toml_config::from_toml_str(
        "world_pack_path = \"top-level is not read\"\n",
    )
    .expect("valid");
    assert_eq!(parsed.config.server.world_pack_path, "./world.pack");
    assert_eq!(parsed.unknown_keys, ["world_pack_path"]);
    assert_eq!(
        MasterConfiguration::default().server.world_pack_path,
        "./world.pack"
    );
}

#[test]
fn a_pack_on_disk_is_installed_as_the_world_database() {
    let path = temp_path("good.pack");
    let bytes = write_pack(&path);
    let (content, status) = world_pack::open_world_database(&path);
    let WorldPackStatus::Loaded {
        records,
        content_hash,
    } = status
    else {
        panic!("loaded: {status:?}")
    };
    assert_eq!(records, 2, "two quest records");
    let hash = empyrean_content::pack::PackHeader::read(&bytes)
        .expect("header")
        .content_hash;
    assert_eq!(content_hash, empyrean_content::pack::hex(&hash));
    assert_eq!(content.get_cached_quest("F16QuestB").map(|q| q.id), Some(2));
    // With the corrections digest, the content hash names the world served.
    assert_eq!(
        content.content_hash().as_deref(),
        Some(content_hash.as_str())
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_start_up_corrections_line_counts_what_applies() {
    use empyrean_content::corrections::report::CorrectionsReport;
    use empyrean_content::corrections::{digest, SPELL_CORRECTIONS, WEENIE_CORRECTIONS};
    use empyrean_content::models::world::Weenie;
    use empyrean_entity::enums::{PropertyDataId, PropertyInt, WeenieType};

    let mem = MemContent::new()
        .weenie(
            Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
                .with_int(PropertyInt::PhysicsState, 0x0012_0034),
        )
        .weenie(
            Weenie::new(33845, "acidbomb", WeenieType::ProjectileSpell)
                .with_int(PropertyInt::PhysicsState, 0x408),
        )
        .weenie(
            Weenie::new(3768, "flamingclub", WeenieType::MeleeWeapon)
                .with_did(PropertyDataId::PhysicsScript, 83),
        );
    let line = world_pack::corrections_summary(&CorrectionsReport::of(mem.db().base()));
    let entries = WEENIE_CORRECTIONS.len() + SPELL_CORRECTIONS.len();
    assert_eq!(
        line,
        format!(
            "corrections {}: 1 of {entries} entries apply (1 stale, {} absent); the rules change 1 default scripts and 0 emote motions",
            digest(),
            entries - 2
        )
    );

    // Content built for another era: no entry applies, none is stale or absent; the rule still
    // applies.
    let line = world_pack::corrections_summary(&CorrectionsReport::of(
        mem.era(empyrean_common::era::EraId::Infiltration)
            .db()
            .base(),
    ));
    assert_eq!(
        line,
        format!(
            "corrections {}: 0 of {entries} entries apply (0 stale, 0 absent); the rules change 1 default scripts and 0 emote motions; the pack is for era infiltration, and {entries} entries are for other eras",
            digest(),
        )
    );
}

#[test]
fn a_missing_pack_boots_with_empty_content() {
    let path = temp_path("does-not-exist.pack");
    let (content, status) = world_pack::open_world_database(&path);
    assert_eq!(status, WorldPackStatus::Missing);
    assert_eq!(content.get_cached_quest("F16QuestA"), None);
}

#[test]
fn a_corrupt_pack_boots_with_empty_content() {
    let path = temp_path("corrupt.pack");
    let mut bytes = write_pack(&path);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&path, &bytes).expect("rewrite");
    let (content, status) = world_pack::open_world_database(&path);
    assert!(matches!(status, WorldPackStatus::Invalid(_)), "{status:?}");
    assert_eq!(content.get_cached_quest("F16QuestA"), None);

    std::fs::write(&path, b"not a pack").expect("rewrite");
    let (_, status) = world_pack::open_world_database(&path);
    assert!(matches!(status, WorldPackStatus::Invalid(_)), "{status:?}");
    let _ = std::fs::remove_file(&path);
}

/// A pack another release wrote (a format or schema version this build does not read) gives empty
/// content, and the start-up message names the exact command that rebuilds it: from the cached dump
/// when `empyrean-import fetch` left one, else fetching first. Nothing replaces the file.
#[test]
fn a_pack_from_another_release_is_refused_with_the_command_that_rebuilds_it() {
    for (offset, what) in [(8, "format"), (12, "schema")] {
        let path = temp_path(&format!("other-release-{what}.pack"));
        let mut bytes = write_pack(&path);
        bytes[offset] = bytes[offset].wrapping_add(1);
        std::fs::write(&path, &bytes).expect("rewrite");

        let (content, status) = world_pack::open_world_database(&path);
        assert_eq!(content.get_cached_quest("F16QuestA"), None);
        let WorldPackStatus::WrongVersion { reason, rebuild } = &status else {
            panic!("{what}: {status:?}");
        };
        assert!(reason.contains(&format!("pack {what} version")), "{reason}");
        assert!(
            rebuild.starts_with("empyrean-import ")
                && rebuild.ends_with(&format!("--out \"{}\"", path.display())),
            "{rebuild}"
        );
        let message = world_pack::unusable_pack_message(&path, &status).unwrap();
        assert!(
            message.contains("another Empyrean release")
                && message.contains(rebuild.as_str())
                && message.contains(".backup-<UTC timestamp>")
                && message.contains("abort startup"),
            "{message}"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "the old pack is left as it was"
        );
        let _ = std::fs::remove_file(&path);
    }

    let pack = Path::new("/srv/empyrean/world.pack");
    assert_eq!(
        world_pack::rebuild_command(
            pack,
            Some(Path::new("/cache/ACE-World-Database-v0.9.295.sql"))
        ),
        format!(
            "empyrean-import --sql \"{}\" --out \"{}\"",
            Path::new("/cache/ACE-World-Database-v0.9.295.sql").display(),
            pack.display()
        )
    );
    assert_eq!(
        world_pack::rebuild_command(pack, None),
        format!("empyrean-import fetch --pack --out \"{}\"", pack.display())
    );
    // A missing or damaged pack keeps the build-one message.
    let missing = world_pack::unusable_pack_message(pack, &WorldPackStatus::Missing).unwrap();
    assert!(
        missing.contains("empyrean-import fetch --pack"),
        "{missing}"
    );
    assert_eq!(
        world_pack::unusable_pack_message(
            pack,
            &WorldPackStatus::Loaded {
                records: 1,
                content_hash: String::new()
            }
        ),
        None
    );
}

/// The default pack is also found beside the executable a written one is not.
#[test]
fn the_default_pack_is_also_found_beside_the_executable_a_written_one_is_not() {
    let root = std::path::absolute("/").unwrap();
    let (cwd, conf, exe) = (
        root.join("work"),
        root.join("etc").join("empyrean"),
        root.join("opt").join("empyrean"),
    );
    let file = conf.join("empyrean.toml");
    let base = PathBase::new(Some(&file), &cwd, None, Some(exe.clone()));
    let find = |key: &str, present: &[PathBuf]| {
        world_pack::resolve_world_pack_path(key, &base, &|p: &Path| present.iter().any(|q| q == p))
    };
    let (beside_conf, beside_exe) = (conf.join("world.pack"), exe.join("world.pack"));
    for key in ["", " ", "./world.pack"] {
        assert_eq!(
            find(key, &[beside_conf.clone(), beside_exe.clone()]),
            beside_conf,
            "{key:?}"
        );
        assert_eq!(
            find(key, &[beside_exe.clone(), cwd.join("world.pack")]),
            beside_exe,
            "{key:?}"
        );
        assert_eq!(
            find(key, &[]),
            beside_conf,
            "{key:?}: the error names the config folder"
        );
    }
    // A written relative path: the config folder only.
    assert_eq!(
        find("packs/w.pack", &[exe.join("packs").join("w.pack")]),
        conf.join("packs").join("w.pack")
    );
    assert_eq!(
        find("world.pack", std::slice::from_ref(&beside_exe)),
        beside_conf,
        "not the default spelling: no search"
    );
    // Absolute: as written.
    let abs = std::env::temp_dir().join("elsewhere.pack");
    assert_eq!(find(&abs.to_string_lossy(), &[]), abs);
    // Without a configuration file: the working directory, then beside the executable.
    let base = PathBase::new(None, &cwd, None, Some(exe.clone()));
    let found = world_pack::resolve_world_pack_path("", &base, &|p: &Path| p == beside_exe);
    assert_eq!(found, beside_exe);
    let found =
        world_pack::resolve_world_pack_path("", &base, &|p: &Path| p == cwd.join("world.pack"));
    assert_eq!(found, cwd.join("world.pack"));
}

/// The configured overlay is opened in front of the pack.
#[test]
fn the_configured_overlay_is_opened_in_front_of_the_pack() {
    use empyrean_content::import::patch::InputKind;
    let mut config = MasterConfiguration::default();
    let cwd = std::env::temp_dir();
    let paths = PathBase::new(None, &cwd, None, None);
    assert!(
        world_pack::configured_overlay(&config, &paths).is_none(),
        "no overlay by default"
    );
    // Absolute on this host.
    let o = std::env::temp_dir().join("o");
    let at = |name: &str| o.join(name).to_string_lossy().into_owned();
    config.server.world_overlay_path = at("overlay.sqlite");
    config.server.world_base_sql = at("base.sql");
    config.server.world_base_patches = vec![at("patches"), format!("json:{}", at("json"))];
    let (file, base) = world_pack::configured_overlay(&config, &paths).unwrap();
    assert_eq!(file, o.join("overlay.sqlite"));
    assert_eq!(base.sql, o.join("base.sql"));
    assert_eq!(
        base.patches
            .iter()
            .map(|i| (i.kind, i.path.clone()))
            .collect::<Vec<_>>(),
        [
            (InputKind::Sql, o.join("patches")),
            (InputKind::Json, o.join("json")),
        ]
    );

    let pack = temp_path("overlay-world.pack");
    write_pack(&pack);
    let overlay = temp_path("overlay-8b2.sqlite");
    let _ = std::fs::remove_file(&overlay);
    let (content, status, opened) =
        world_pack::open_world_database_with_overlay(&pack, Some((overlay.clone(), base.clone())));
    assert!(matches!(status, WorldPackStatus::Loaded { .. }));
    assert_eq!(opened, Some(Ok(0)));
    assert_eq!(content.overlay().unwrap().path(), overlay);
    assert_eq!(
        content.get_cached_quest("F16QuestA").unwrap().id,
        1,
        "reads fall through to the pack"
    );

    // An overlay made over another pack is refused, and the server would not start.
    let other = temp_path("overlay-other.pack");
    let mem = MemContent::new().quest(quest(3, "Other"));
    std::fs::write(&other, mem.content().to_pack([8; 16]).unwrap().0).unwrap();
    let (_, _, opened) =
        world_pack::open_world_database_with_overlay(&other, Some((overlay, base)));
    assert!(
        matches!(opened, Some(Err(ref e)) if e.contains("was made over")),
        "{opened:?}"
    );
}

/// The overlay and its base inputs resolve against the configuration file's folder, `json:`
/// entries included, not against the working directory.
#[test]
fn a_relative_overlay_resolves_beside_the_config() {
    let root = std::path::absolute("/").unwrap();
    let conf = root.join("etc").join("empyrean");
    let file = conf.join("empyrean.toml");
    let base = PathBase::new(
        Some(&file),
        &root.join("work"),
        Some(root.join("home")),
        None,
    );
    let mut config = MasterConfiguration::default();
    config.server.world_overlay_path = "overlay.sqlite".to_owned();
    config.server.world_base_sql = "./sql/base.sql".to_owned();
    config.server.world_base_patches = vec![
        "patches".to_owned(),
        "json:content/json".to_owned(),
        "json:~/json".to_owned(),
    ];
    let (overlay, inputs) = world_pack::configured_overlay(&config, &base).unwrap();
    assert_eq!(overlay, conf.join("overlay.sqlite"));
    assert_eq!(inputs.sql, conf.join("sql").join("base.sql"));
    let paths: Vec<PathBuf> = inputs.patches.iter().map(|i| i.path.clone()).collect();
    assert_eq!(
        paths,
        [
            conf.join("patches"),
            conf.join("content").join("json"),
            root.join("home").join("json")
        ]
    );
    // An unset base dump stays unset.
    config.server.world_base_sql = String::new();
    assert_eq!(
        world_pack::configured_overlay(&config, &base)
            .unwrap()
            .1
            .sql,
        PathBuf::new()
    );
}
