//! Divergence: V373, V375
//! Discovery finds empyrean.toml in cwd then beside the exe, explicit config must exist, example
//! equals generated defaults, errors, log levels, Config.js converted once / startup fails with
//! converter message, removed env vars ignored, unknown keys warned, relative paths beside the
//! file.
//! Fixture: tests/fixtures/empyrean.toml.example and locally constructed edge cases.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use empyrean_common::config_manager::ConfigError;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::toml_config;
use empyrean_server::config_file::{self, ConfigSource, DiscoverError, Discovery};

const EMPYREAN_TOML_EXAMPLE: &str = include_str!("../../../empyrean.toml.example");
/// ACE's own `Config.js.example`, as ACE ships it.
const ACE_CONFIG_JS: &str =
    include_str!("../../../../crates/common/tests/fixtures/Config.js.example");

fn cwd() -> PathBuf {
    PathBuf::from("/work")
}

fn exe() -> PathBuf {
    PathBuf::from("/opt/empyrean")
}

fn discover(explicit: Option<&str>, files: &[PathBuf]) -> Result<Discovery, DiscoverError> {
    let files: BTreeSet<PathBuf> = files.iter().cloned().collect();
    config_file::discover(explicit, &cwd(), Some(&exe()), &|p: &Path| {
        files.contains(p)
    })
}

#[test]
fn discovery_finds_empyrean_toml_and_never_reads_config_js() {
    let toml_cwd = cwd().join("empyrean.toml");
    let toml_exe = exe().join("empyrean.toml");
    let js_cwd = cwd().join("Config.js");
    let js_exe = exe().join("Config.js");

    let cases: &[(&[PathBuf], ConfigSource, &[PathBuf])] = &[
        (&[], ConfigSource::Defaults, &[]),
        (
            std::slice::from_ref(&toml_exe),
            ConfigSource::File(toml_exe.clone()),
            &[],
        ),
        (
            &[toml_cwd.clone(), toml_exe.clone()],
            ConfigSource::File(toml_cwd.clone()),
            std::slice::from_ref(&toml_exe),
        ),
        // A Config.js beside an empyrean.toml is reported as not read.
        (
            &[js_cwd.clone(), toml_exe.clone()],
            ConfigSource::File(toml_exe.clone()),
            std::slice::from_ref(&js_cwd),
        ),
        (
            &[
                js_cwd.clone(),
                js_exe.clone(),
                toml_cwd.clone(),
                toml_exe.clone(),
            ],
            ConfigSource::File(toml_cwd.clone()),
            &[toml_exe.clone(), js_cwd.clone(), js_exe.clone()],
        ),
    ];
    for (files, source, ignored) in cases {
        let d = discover(None, files).expect("a configuration");
        assert_eq!(&d.source, source, "{files:?}");
        assert_eq!(d.ignored, *ignored, "{files:?}");
    }

    // Only a Config.js: an error naming it, not the defaults.
    assert_eq!(
        discover(None, std::slice::from_ref(&js_exe)),
        Err(DiscoverError::ConfigJs(js_exe.clone()))
    );
    assert_eq!(
        discover(None, &[js_cwd.clone(), js_exe.clone()]),
        Err(DiscoverError::ConfigJs(js_cwd.clone()))
    );
    let message = DiscoverError::ConfigJs(js_cwd.clone()).to_string();
    assert!(
        message.contains("--write-config --from")
            && message.contains("Config.js")
            && message.contains("empyrean.toml"),
        "{message}"
    );

    // Started from the executable's own directory: each file counts once.
    let files: BTreeSet<PathBuf> = [exe().join("empyrean.toml")].into();
    let d =
        config_file::discover(None, &exe(), Some(&exe()), &|p: &Path| files.contains(p)).unwrap();
    assert_eq!(
        d,
        Discovery {
            source: ConfigSource::File(exe().join("empyrean.toml")),
            ignored: vec![]
        }
    );
    // No executable directory known: the working directory only.
    let d = config_file::discover(None, &cwd(), None, &|_| false).unwrap();
    assert_eq!(d.source, ConfigSource::Defaults);
}

#[test]
fn an_explicit_config_is_used_alone_and_must_exist() {
    let all = [
        cwd().join("empyrean.toml"),
        cwd().join("Config.js"),
        exe().join("mine.toml"),
        PathBuf::from("/etc/empyrean/server.conf"),
        PathBuf::from("/etc/empyrean/ace.JS"),
    ];
    // A bare name: the working directory, then beside the executable (ACE's ConfigManager rule).
    assert_eq!(
        discover(Some("mine.toml"), &all).unwrap().source,
        ConfigSource::File(exe().join("mine.toml"))
    );
    // Any extension but .js is read as TOML.
    let d = discover(Some("/etc/empyrean/server.conf"), &all).unwrap();
    assert_eq!(
        d.source,
        ConfigSource::File(PathBuf::from("/etc/empyrean/server.conf"))
    );
    assert!(d.ignored.is_empty());
    // A .js file is ACE's Config.js: the converter's error.
    assert_eq!(
        discover(Some("Config.js"), &all),
        Err(DiscoverError::ConfigJs(cwd().join("Config.js")))
    );
    assert_eq!(
        discover(Some("/etc/empyrean/ace.JS"), &all),
        Err(DiscoverError::ConfigJs(PathBuf::from(
            "/etc/empyrean/ace.JS"
        )))
    );
    // A missing explicit file is an error naming where it was looked for.
    assert_eq!(
        discover(Some("/etc/empyrean/none.toml"), &all),
        Err(DiscoverError::Missing(PathBuf::from(
            "/etc/empyrean/none.toml"
        )))
    );
    assert_eq!(
        discover(Some("none.toml"), &all),
        Err(DiscoverError::Missing(exe().join("none.toml")))
    );
}

#[test]
fn empyrean_toml_example_is_the_generated_defaults() {
    // Generator drift fails here: regenerate with `empyrean-server --write-config empyrean.toml.example`
    // in an empty directory (empyrean/README.md).
    let expected = toml_config::to_toml_string(&MasterConfiguration::default());
    assert!(
        !EMPYREAN_TOML_EXAMPLE.contains('\r'),
        "empyrean.toml.example has LF line endings"
    );
    assert!(
        EMPYREAN_TOML_EXAMPLE == expected,
        "empyrean.toml.example is stale: regenerate it"
    );
    let parsed = config_file::parse(EMPYREAN_TOML_EXAMPLE).unwrap();
    assert!(parsed.unknown_keys.is_empty() && parsed.ignored.is_empty());
    assert_eq!(parsed.config, MasterConfiguration::default());
    // Its header says what the configuration surface is.
    for words in [
        "only configuration",
        "no environment variables",
        "Config.js is not read",
        "--write-config --from",
    ] {
        assert!(EMPYREAN_TOML_EXAMPLE.contains(words), "{words}");
    }
}

#[test]
fn parse_and_load_errors() {
    assert!(matches!(
        config_file::parse("[server"),
        Err(ConfigError::Toml(_))
    ));
    assert!(matches!(
        config_file::load(Path::new("/definitely/not/here/empyrean.toml")),
        Err(ConfigError::Io(_))
    ));
}

#[test]
fn log_level_names_the_five_levels() {
    use log::LevelFilter as L;
    for (text, level) in [
        ("error", L::Error),
        ("warn", L::Warn),
        ("info", L::Info),
        (" Debug ", L::Debug),
        ("TRACE", L::Trace),
    ] {
        assert_eq!(config_file::log_level(text), Some(level), "{text}");
    }
    for bad in ["", "warning", "off", "verbose", "3"] {
        assert_eq!(config_file::log_level(bad), None, "{bad}");
    }
    assert_eq!(MasterConfiguration::default().server.log_level, "info");
}

/// A scratch directory under the target directory.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs the binary in `dir` with none of the variables it used to read, and no console input.
fn run_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_empyrean-server"));
    command.args(args).current_dir(dir).stdin(Stdio::null());
    for var in [
        "DERETH_TEST_DAT_DIR",
        "EMPYREAN_WORLD_PACK",
        "EMPYREAN_SHARD_DB",
        "EMPYREAN_AUTH_DB",
        "EMPYREAN_LOG",
        "EMPYREAN_STATUS",
        "EMPYREAN_NONINTERACTIVE_CONSOLE",
        "ACE_NONINTERACTIVE_CONSOLE",
    ] {
        command.env_remove(var);
    }
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().expect("empyrean-server runs")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The converter end to end: ACE's own `Config.js.example` becomes an `empyrean.toml` that loads
/// cleanly as the configuration the Config.js held, with every dropped key logged and listed in
/// the file's header. It never overwrites.
#[test]
fn write_config_from_converts_aces_config_js_once() {
    let dir = scratch("config-convert");
    std::fs::write(dir.join("Config.js"), ACE_CONFIG_JS).unwrap();

    let out = run_in(
        &dir,
        &["--write-config", "--from", "Config.js", "--out", "out.toml"],
        &[],
    );
    let log = stderr(&out);
    assert!(out.status.success(), "{log}");
    let written = std::fs::read_to_string(dir.join("out.toml")).unwrap();
    let converted = toml_config::from_config_js_str(ACE_CONFIG_JS).unwrap();
    let loaded = config_file::parse(&written).expect("the converted file loads");
    assert!(
        loaded.unknown_keys.is_empty() && loaded.ignored.is_empty(),
        "{loaded:?}"
    );
    assert_eq!(loaded.config, converted.config);
    assert_eq!(loaded.config.server.world_name, "ACEmulator");
    assert_eq!(loaded.config.server.dat_files_directory, "c:\\ACE\\Dats\\");
    // Every dropped key is logged and listed in the header.
    assert!(!converted.dropped.is_empty());
    for d in &converted.dropped {
        assert!(
            log.contains(&format!("`{}` dropped", d.key)),
            "{} not logged:\n{log}",
            d.key
        );
        assert!(
            written.contains(&format!("#   {}: ", d.key)),
            "{} not listed",
            d.key
        );
    }
    assert!(written.starts_with(toml_config::HEADER));
    assert!(written.contains("# Converted from Config.js. Dropped (Empyrean has no such setting):"));
    // Its paths are copied as written, and the header says they now resolve beside the new file.
    assert!(
        written.contains("A relative path in Config.js was relative to ACE's working"),
        "{written}"
    );
    assert!(written.contains("relative to the folder this file is in"));

    // It never overwrites; the positional path works as --out does.
    let out = run_in(
        &dir,
        &["--write-config", "out.toml", "--from", "Config.js"],
        &[],
    );
    assert!(!out.status.success());
    assert_eq!(
        std::fs::read_to_string(dir.join("out.toml")).unwrap(),
        written
    );
    let out = run_in(&dir, &["--write-config", "--from", "Config.js"], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(dir.join("empyrean.toml")).unwrap(),
        written,
        "default: empyrean.toml"
    );

    // --from and --out need --write-config.
    let out = run_in(&dir, &["--from", "Config.js"], &[]);
    assert!(!out.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A server that finds only a Config.js stops and names the converter; `--write-config` without
/// `--from` refuses it the same way.
#[test]
fn startup_with_only_a_config_js_fails_with_the_converter_message() {
    let dir = scratch("config-js-only");
    std::fs::write(
        dir.join("Config.js"),
        "{ \"Server\": { \"WorldName\": \"Old\" } }",
    )
    .unwrap();
    for args in [
        &["--run-for", "1"][..],
        &["--write-config"][..],
        &["--config", "Config.js", "--run-for", "1"][..],
    ] {
        let out = run_in(&dir, args, &[]);
        let log = stderr(&out);
        assert!(!out.status.success(), "{args:?}: {log}");
        assert!(
            log.contains("empyrean-server --write-config --from"),
            "{args:?}: {log}"
        );
        assert!(!log.contains("Listening on"), "{log}");
    }
    assert!(!dir.join("empyrean.toml").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The server reads no environment variable: the variables that used to override the dat folder,
/// the log level, the world pack, the databases, the status endpoint and the console are ignored,
/// and the configuration's keys are used. An older file's removed keys and its
/// `[mysql]` section are warned about and the server goes on.
#[test]
fn the_server_ignores_the_removed_environment_variables() {
    let dir = scratch("config-env-ignored");
    let client = dereth_dat::testing::dat_dir();
    let real_dats = dereth_dat::testing::have_dats();
    let dats = if real_dats {
        client
    } else {
        dir.join("no-dats")
    };
    let toml = format!(
        "[server]\nlog_level = \"info\"\ndat_files_directory = '{}'\nworld_pack_path = \"config.pack\"\n\
         [server.threading]\nworld_thread_count_multiplier = 0.5\n\
         [mysql]\nshard_db_path = \"old-shard.db\"\n\
         [database]\nshard_db_path = \"config-shard.db\"\nauth_db_path = \"config-auth.db\"\n",
        dats.display()
    );
    std::fs::write(dir.join("empyrean.toml"), toml).unwrap();
    let env_pack = dir.join("env.pack");
    std::fs::write(&env_pack, b"not a pack either").unwrap();
    // A folder that is never opened: the tests' dat variable does not name the server's dat folder.
    let env_dats = dir.join("env-dats-never-read");
    let env: &[(&str, &str)] = &[
        ("DERETH_TEST_DAT_DIR", &env_dats.to_string_lossy()),
        ("EMPYREAN_LOG", "error"),
        ("EMPYREAN_WORLD_PACK", &env_pack.to_string_lossy()),
        ("EMPYREAN_SHARD_DB", "env-shard.db"),
        ("EMPYREAN_AUTH_DB", "env-auth.db"),
        ("EMPYREAN_STATUS", "127.0.0.1:1"),
        ("EMPYREAN_NONINTERACTIVE_CONSOLE", "true"),
        ("ACE_NONINTERACTIVE_CONSOLE", "true"),
    ];
    let out = run_in(&dir, &["--run-for", "1"], env);
    let log = stderr(&out);
    // No usable dats or no world.pack: start-up stops either way, having read the configuration.
    assert!(!out.status.success(), "{log}");
    assert!(
        log.contains(" INFO "),
        "EMPYREAN_LOG=error must not silence info lines:\n{log}"
    );
    assert!(
        log.contains("`server.threading`") && log.contains("is not read"),
        "{log}"
    );
    assert!(
        log.contains("`mysql.shard_db_path`") && log.contains("[database]"),
        "{log}"
    );
    assert!(
        !log.contains("env.pack") && !log.contains("Status endpoint"),
        "{log}"
    );
    assert!(
        !log.contains("env-dats-never-read"),
        "DERETH_TEST_DAT_DIR must not name the dat folder:
{log}"
    );
    if real_dats {
        // The configuration's dat folder opened: start-up got as far as the world pack.
        assert!(
            log.contains("No usable world database at") && log.contains("config.pack"),
            "{log}"
        );
    } else {
        assert!(
            log.contains("DatManager initialization failed") && log.contains("no-dats"),
            "{log}"
        );
        eprintln!(
            "(no retail dats at {}: the world-pack half of this check did not run)",
            dats.display()
        );
    }
    for f in ["env-shard.db", "env-auth.db"] {
        assert!(!dir.join(f).exists(), "{f} was created");
    }

    // An unknown log level is a warning, and the server logs at info.
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "[server]\nlog_level = \"loud\"\ndat_files_directory = '{}'\n",
            dir.join("no-dats").display()
        ),
    )
    .unwrap();
    let out = run_in(&dir, &["--run-for", "1"], &[]);
    let log = stderr(&out);
    assert!(
        log.contains("server.log_level = \"loud\"") && log.contains("logging at info"),
        "{log}"
    );
    assert!(log.contains(" INFO "), "{log}");
    // A known one is used: at error, no info line follows the configuration's.
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "[server]\nlog_level = \"error\"\ndat_files_directory = '{}'\n",
            dir.join("no-dats").display()
        ),
    )
    .unwrap();
    let out = run_in(&dir, &["--run-for", "1"], &[]);
    let log = stderr(&out);
    assert!(log.contains("DatManager initialization failed"), "{log}");
    assert!(!log.contains("Initializing ModManager"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A key the configuration does not have is a warning naming it, never an error: start-up goes on
/// past the configuration.
#[test]
fn an_unknown_config_key_is_a_warning_and_start_up_goes_on() {
    let dir = scratch("config-unknown-key");
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "colour = \"blue\"\n[server]\nlog_level = \"info\"\nwrold_name = \"typo\"\ndat_files_directory = '{}'\n",
            dir.join("no-dats").display()
        ),
    )
    .unwrap();
    let out = run_in(&dir, &["--run-for", "1"], &[]);
    let log = stderr(&out);
    for key in ["`colour`", "`server.wrold_name`"] {
        assert!(
            log.lines()
                .any(|l| l.contains(" WARN ") && l.contains("unknown key") && l.contains(key)),
            "{key}: {log}"
        );
    }
    assert!(!log.contains("An exception occured while loading"), "{log}");
    // The configuration was accepted: start-up reached the dat folder it names.
    assert!(log.contains("DatManager initialization failed"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Relative paths resolve beside the config file not the working directory.
#[test]
fn relative_paths_resolve_beside_the_config_file_not_the_working_directory() {
    let conf = scratch("paths-config-folder");
    let cwd = scratch("paths-working-directory");
    std::fs::write(
        conf.join("empyrean.toml"),
        "[server]\nlog_level = \"info\"\ndat_files_directory = \"no-dats\"\n\
         [offline]\nprune_deleted_characters_from_friend_lists = true\n",
    )
    .unwrap();
    let config = conf.join("empyrean.toml");
    let out = run_in(
        &cwd,
        &["--config", &config.to_string_lossy(), "--run-for", "1"],
        &[],
    );
    let log = stderr(&out);
    assert!(!out.status.success(), "no dats: start-up stops\n{log}");
    assert!(
        conf.join("shard.db").is_file(),
        "the shard is created beside the configuration file:\n{log}"
    );
    assert!(
        !cwd.join("shard.db").exists(),
        "not in the working directory:\n{log}"
    );
    let no_dats = conf.join("no-dats").display().to_string();
    assert!(
        log.contains("DatManager initialization failed") && log.contains(&no_dats),
        "{log}"
    );
    // The resolved paths are logged at info.
    let shard = conf.join("shard.db").display().to_string();
    assert!(
        log.contains("Paths: shard database: ") && log.contains(&shard),
        "{log}"
    );
    assert!(log.contains("Paths: authentication database: "), "{log}");
    assert!(log.contains("the configuration file's folder"), "{log}");
    assert_eq!(
        std::fs::read_dir(&cwd).unwrap().count(),
        0,
        "nothing is written to the working directory"
    );
    let _ = std::fs::remove_dir_all(&conf);
    let _ = std::fs::remove_dir_all(&cwd);
}
