//! Divergence: V375
//! A configured path: leading ~ is home, absolute kept, relative is relative to the config file's
//! folder (working dir without one), empty stays empty, search tries base then beside the
//! executable.
//! Fixture: locally constructed values and deterministic expected results.

use std::path::{Path, PathBuf};

use empyrean_common::config_paths::PathBase;

fn root() -> PathBuf {
    std::path::absolute("/").unwrap()
}

fn home() -> PathBuf {
    root().join("home").join("op")
}

fn cwd() -> PathBuf {
    root().join("work")
}

fn conf() -> PathBuf {
    root().join("etc").join("empyrean")
}

fn with_config() -> PathBase {
    PathBase::new(
        Some(&conf().join("empyrean.toml")),
        &cwd(),
        Some(home()),
        None,
    )
}

#[test]
fn a_leading_tilde_is_the_home_directory() {
    let b = with_config();
    assert_eq!(b.resolve("~"), home());
    assert_eq!(b.resolve("~/"), home());
    assert_eq!(b.resolve("~/x"), home().join("x"));
    assert_eq!(b.resolve("~\\x"), home().join("x"));
    assert_eq!(b.resolve("~/ac/dats"), home().join("ac").join("dats"));
    assert_eq!(b.resolve("  ~/x  "), home().join("x"), "trimmed");
}

#[test]
fn another_users_tilde_and_an_inner_tilde_are_literal() {
    let b = with_config();
    // `~user/x` is a relative path whose first folder is named `~user`.
    assert_eq!(b.resolve("~user/x"), conf().join("~user").join("x"));
    assert_eq!(b.resolve("~x"), conf().join("~x"));
    assert_eq!(b.resolve("a/~/x"), conf().join("a").join("~").join("x"));
}

#[test]
fn without_a_home_directory_a_tilde_path_is_kept_as_written() {
    let b = PathBase::new(Some(&conf().join("empyrean.toml")), &cwd(), None, None);
    assert_eq!(b.expand_home("~/x"), PathBuf::from("~/x"));
    // ...and, being relative, lands beside the configuration file.
    assert_eq!(b.resolve("~/x"), conf().join("~").join("x"));
}

#[test]
fn an_absolute_path_is_used_as_it_is() {
    let b = with_config();
    let abs = if cfg!(windows) {
        r"D:\data\shard.db"
    } else {
        "/data/shard.db"
    };
    assert_eq!(b.resolve(abs), PathBuf::from(abs));
}

#[test]
fn a_relative_path_is_relative_to_the_config_files_folder() {
    let b = with_config();
    assert!(b.from_config);
    assert_eq!(b.base, conf());
    assert_eq!(b.resolve("shard.db"), conf().join("shard.db"));
    assert_eq!(b.resolve("./shard.db"), conf().join("shard.db"));
    assert_eq!(
        b.resolve("data/auth.db"),
        conf().join("data").join("auth.db")
    );
    assert_ne!(b.resolve("shard.db"), cwd().join("shard.db"));
    // A relative configuration file is under the working directory; its folder is the base.
    let rel = PathBase::new(Some(Path::new("cfg/empyrean.toml")), &cwd(), None, None);
    assert_eq!(rel.resolve("shard.db"), cwd().join("cfg").join("shard.db"));
    let bare = PathBase::new(Some(Path::new("empyrean.toml")), &cwd(), None, None);
    assert_eq!(bare.resolve("shard.db"), cwd().join("shard.db"));
}

#[test]
fn without_a_config_file_a_relative_path_is_relative_to_the_working_directory() {
    let b = PathBase::new(None, &cwd(), Some(home()), None);
    assert!(!b.from_config);
    assert_eq!(b.resolve("./world.pack"), cwd().join("world.pack"));
    assert_eq!(b.resolve("~/w.pack"), home().join("w.pack"));
}

#[test]
fn an_empty_value_stays_empty() {
    assert_eq!(with_config().resolve(""), PathBuf::new());
    assert_eq!(with_config().resolve("   "), PathBuf::new());
}

#[test]
fn a_search_tries_the_base_then_beside_the_executable() {
    let exe = root().join("opt").join("empyrean");
    let b = PathBase::new(
        Some(&conf().join("empyrean.toml")),
        &cwd(),
        None,
        Some(exe.clone()),
    );
    let in_conf = conf().join("world.pack");
    let in_exe = exe.join("world.pack");
    assert_eq!(
        b.search("./world.pack", &|p| p == in_conf || p == in_exe),
        in_conf
    );
    assert_eq!(b.search("./world.pack", &|p| p == in_exe), in_exe);
    assert_eq!(
        b.search("./world.pack", &|p| p == cwd().join("world.pack")),
        in_conf,
        "the working directory is not searched"
    );
    let none = PathBase::new(None, &cwd(), None, None);
    assert_eq!(
        none.search("./world.pack", &|_| false),
        cwd().join("world.pack")
    );
}
