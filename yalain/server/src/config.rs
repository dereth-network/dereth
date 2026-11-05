//! The configuration file, `yalain.toml`: the server's options kept in a file, in the form of
//! Empyrean's `empyrean.toml` (its sections, its key names, their meanings and units), so a host
//! who keeps worlds of both games keeps one kind of file.
//!
//! Each key stands for a command-line option ([`KEYS`]): `[server] world_name` for `--world-name`,
//! `[server.network] port` for `--login-port`, `[federation] enabled` for `--federation`, and the
//! rest. Where Yalain has an option Empyrean lacks (the world service's port, the address players
//! are sent to, the friends service, the world database's folders, the options for testing), its
//! key sits in the section Empyrean would put it in, named as Empyrean names its keys. Empyrean's
//! keys Yalain has no setting for are warned about and left out ([`EMPYREAN_ONLY`]), as is any key
//! the server does not know. A relative path is read from the file's folder. The command line is
//! read after the file, so an option given on both wins on the command line; one given many times
//! takes the file's values and the command line's.
//!
//! The file read is `--config <file>` (which must exist), else `yalain.toml` in the working
//! folder, else beside the executable, else none: the command line alone.

use std::path::{Path, PathBuf};

/// The file's name, looked for in the working folder and beside the executable.
pub const FILE: &str = "yalain.toml";

/// How a key's value becomes options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Takes {
    /// A string or a number, given as the option's value; an empty string gives nothing.
    Value(&'static str),
    /// A path, read from the file's folder when relative; an empty string gives nothing.
    Path(&'static str),
    /// `true` gives the option, which takes no value.
    Flag(&'static str),
    /// `false` gives the option, which takes no value (a setting on by default turned off).
    Unless(&'static str),
    /// A string given as the option's value even when it is empty, which a setting's checks read.
    Text(&'static str),
    /// A whole number, or a string holding one, given as the option's value.
    Whole(&'static str),
    /// A list, the option given once for each item.
    Many(&'static str),
    /// A list of paths.
    ManyPaths(&'static str),
    /// `[server] dat_files_directory`: a path; empty means the file's folder.
    Install,
    /// `[server] log_level`: `debug` or `trace` log at the debug level (`--verbose`).
    LogLevel,
    /// `[server.network] maximum_allowed_sessions_per_ip_address`: -1 is no limit.
    PerAddress,
    /// `[server] world_database_path`: a folder, or empty (or `pinned`) for the release this
    /// build pins.
    WorldDatabase,
    /// `[federation]`'s keys, given together ([`hub`]).
    Hub,
}

/// Every key a file may give, by its section and name, and what it gives.
const KEYS: &[(&str, &str, Takes)] = &[
    ("server", "world_name", Takes::Value("--world-name")),
    ("server", "update", Takes::Value("--update")),
    (
        "server",
        "update_check_hours",
        Takes::Value("--update-check-hours"),
    ),
    (
        "server",
        "update_warning_seconds",
        Takes::Value("--update-warning-seconds"),
    ),
    ("server", "update_source", Takes::Value("--update-source")),
    ("server", "dat_files_directory", Takes::Install),
    ("server", "message_of_the_day", Takes::Value("--motd")),
    ("server", "world_database_path", Takes::WorldDatabase),
    ("server", "world_base_path", Takes::Path("--world-base")),
    (
        "server",
        "shutdown_interval",
        Takes::Value("--shutdown-interval"),
    ),
    (
        "server",
        "player_save_interval",
        Takes::Value("--save-interval"),
    ),
    ("server", "log_level", Takes::LogLevel),
    ("server", "status_address", Takes::Value("--status")),
    (
        "server",
        "stop_at_console_end",
        Takes::Flag("--stop-at-console-end"),
    ),
    ("server.network", "host", Takes::Value("--bind")),
    ("server.network", "port", Takes::Value("--login-port")),
    ("server.network", "world_port", Takes::Value("--world-port")),
    (
        "server.network",
        "advertised_address",
        Takes::Value("--advertise"),
    ),
    (
        "server.network",
        "world_referral",
        Takes::Unless("--no-referral"),
    ),
    (
        "server.network",
        "friends_service",
        Takes::Unless("--no-friends"),
    ),
    (
        "server.network",
        "maximum_allowed_sessions",
        Takes::Value("--max-sessions"),
    ),
    (
        "server.network",
        "maximum_allowed_sessions_per_ip_address",
        Takes::PerAddress,
    ),
    (
        "server.network",
        "allow_unlimited_sessions_from_ip_addresses",
        Takes::Many("--unlimited-sessions-from"),
    ),
    (
        "server.network",
        "download_bytes_per_second",
        Takes::Whole("--download-rate"),
    ),
    ("server.websocket", "enabled", Takes::Flag("--websocket")),
    (
        "server.websocket",
        "listen",
        Takes::Text("--websocket-listen"),
    ),
    (
        "server.websocket",
        "tls_certificate",
        Takes::Path("--websocket-tls-certificate"),
    ),
    (
        "server.websocket",
        "tls_private_key",
        Takes::Path("--websocket-tls-private-key"),
    ),
    (
        "server.websocket",
        "behind_tls_proxy",
        Takes::Flag("--websocket-behind-tls-proxy"),
    ),
    (
        "server.websocket",
        "trusted_proxies",
        Takes::Many("--websocket-trusted-proxy"),
    ),
    (
        "server.websocket",
        "allowed_origins",
        Takes::Many("--websocket-allowed-origin"),
    ),
    (
        "server.websocket",
        "maximum_connections_per_ip_address",
        Takes::Whole("--websocket-max-per-address"),
    ),
    (
        "server.websocket",
        "idle_timeout",
        Takes::Whole("--websocket-idle-timeout"),
    ),
    (
        "server.websocket",
        "public_url",
        Takes::Text("--websocket-public-url"),
    ),
    (
        "server.accounts",
        "allow_auto_account_creation",
        Takes::Unless("--listed-accounts"),
    ),
    (
        "server.accounts",
        "password_hash_work_factor",
        Takes::Value("--secret-cost"),
    ),
    ("server.accounts", "accounts", Takes::Many("--add-account")),
    ("server.accounts", "admins", Takes::Many("--admin")),
    (
        "server.accounts",
        "serve_admin_library",
        Takes::Flag("--serve-admin-library"),
    ),
    (
        "server.accounts",
        "admin_commands",
        Takes::Many("--admin-command"),
    ),
    (
        "server.accounts",
        "banned_names_path",
        Takes::Path("--banned-names"),
    ),
    ("status", "udp_ping", Takes::Unless("--no-status-ping")),
    (
        "status",
        "hellos_per_minute",
        Takes::Value("--status-ping-hellos"),
    ),
    (
        "status",
        "asks_per_minute",
        Takes::Value("--status-ping-asks"),
    ),
    (
        "status",
        "replies_per_second",
        Takes::Value("--status-ping-replies"),
    ),
    ("database", "shard_db_path", Takes::Path("--data")),
    ("database", "in_memory", Takes::Flag("--memory")),
    ("federation", "enabled", Takes::Hub),
    ("federation", "listed", Takes::Hub),
    ("federation", "hub_url", Takes::Hub),
    ("federation", "slug", Takes::Hub),
    ("federation", "key_file", Takes::Hub),
    ("federation", "global_room", Takes::Hub),
    (
        "federation",
        "min_character_age_seconds",
        Takes::Value("--min-character-age"),
    ),
    ("testing", "add_characters", Takes::Many("--add-character")),
    ("testing", "place_characters", Takes::Many("--place")),
    ("testing", "character_money", Takes::Many("--money")),
    (
        "testing",
        "make_characters",
        Takes::ManyPaths("--make-character"),
    ),
    ("testing", "game_time", Takes::Value("--game-time")),
    ("testing", "hold_clock", Takes::Flag("--hold-clock")),
    ("testing", "echo_positions", Takes::Unless("--no-echo")),
    ("testing", "run_seconds", Takes::Value("--seconds")),
];

/// Empyrean's keys and sections Yalain has no setting for: a file copied from an Empyrean world
/// reads, each of these warned about and left out.
pub const EMPYREAN_ONLY: &[&str] = &[
    "server.character_screen_message",
    "server.world_pack_path",
    "server.world_overlay_path",
    "server.world_base_sql",
    "server.world_base_patches",
    "server.server_performance_monitor_auto_start",
    "server.shard_player_biota_cache_time",
    "server.shard_non_player_biota_cache_time",
    "server.landblock_preloading",
    "server.status_public_url",
    "server.publish_portals",
    "server.interactive_console",
    "server.admin_channel",
    "server.source_url",
    "server.preloaded_landblocks",
    "server.network.default_session_timeout",
    "server.accounts.override_character_permissions",
    "server.accounts.default_access_level",
    "server.accounts.force_work_factor_migration",
    "server.accounts.allow_character_import",
    "database.auth_db_path",
    "offline",
    "ddd",
    "dat_overlay",
    "federation.store_path",
    "era",
];

/// The sections [`KEYS`] holds keys of.
const SECTIONS: &[&str] = &[
    "server",
    "server.network",
    "server.websocket",
    "server.accounts",
    "status",
    "database",
    "federation",
    "testing",
];

/// The hub's address when `[federation] hub_url` is not given, as Empyrean's.
pub const HUB: &str = "wss://hub.dereth.network";

/// The file to read: the one `--config` names (taken out of `args`), else [`FILE`] in `cwd`, else
/// beside the executable (`exe_dir`), else none.
///
/// # Errors
/// `--config` has no value, or names a file that does not exist.
pub fn locate(
    args: &mut Vec<String>,
    cwd: &Path,
    exe_dir: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    if let Some(i) = args.iter().position(|a| a == "--config") {
        if i + 1 >= args.len() {
            return Err("--config needs a value".into());
        }
        let file = PathBuf::from(args.remove(i + 1));
        args.remove(i);
        if !file.is_file() {
            return Err(format!(
                "--config {}: the configuration file does not exist",
                file.display()
            ));
        }
        return Ok(Some(file));
    }
    Ok([Some(cwd), exe_dir]
        .into_iter()
        .flatten()
        .map(|d| d.join(FILE))
        .find(|f| f.is_file()))
}

/// What a file gives, as it reads.
struct Reading<'a> {
    folder: &'a Path,
    args: Vec<String>,
    warnings: Vec<String>,
}

/// One value as the command line gives it.
fn text_of(key: &str, v: &toml::Value) -> Result<String, String> {
    match v {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(n) => Ok(n.to_string()),
        toml::Value::Float(x) => Ok(x.to_string()),
        _ => Err(format!("{key}: a string or a number")),
    }
}

/// A path as the command line gives it: from the file's folder when relative.
fn path_of(folder: &Path, text: &str) -> String {
    let p = Path::new(text);
    if p.is_relative() {
        folder.join(p).to_string_lossy().into_owned()
    } else {
        text.to_owned()
    }
}

fn flag(key: &str, v: &toml::Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("{key}: true or false"))
}

fn items<'v>(key: &str, v: &'v toml::Value) -> Result<&'v Vec<toml::Value>, String> {
    v.as_array().ok_or_else(|| format!("{key}: a list"))
}

impl Reading<'_> {
    fn give(&mut self, option: &str, value: Option<String>) {
        self.args.push(option.to_owned());
        self.args.extend(value);
    }

    /// The options one key gives.
    fn key(&mut self, key: &str, takes: Takes, v: &toml::Value) -> Result<(), String> {
        match takes {
            Takes::Value(option) => {
                let text = text_of(key, v)?;
                if !text.is_empty() {
                    self.give(option, Some(text));
                }
            }
            Takes::Text(option) => {
                let text = text_of(key, v)?;
                self.give(option, Some(text));
            }
            Takes::Whole(option) => {
                let text = match v {
                    toml::Value::Integer(n) => n.to_string(),
                    toml::Value::String(s) => s.clone(),
                    _ => return Err(format!("{key}: a whole number")),
                };
                self.give(option, Some(text));
            }
            Takes::Path(option) => {
                let text = text_of(key, v)?;
                if !text.is_empty() {
                    let path = path_of(self.folder, &text);
                    self.give(option, Some(path));
                }
            }
            Takes::Flag(option) => {
                if flag(key, v)? {
                    self.give(option, None);
                }
            }
            Takes::Unless(option) => {
                if !flag(key, v)? {
                    self.give(option, None);
                }
            }
            Takes::Many(option) => {
                for item in items(key, v)? {
                    let text = text_of(key, item)?;
                    self.give(option, Some(text));
                }
            }
            Takes::ManyPaths(option) => {
                for item in items(key, v)? {
                    let path = path_of(self.folder, &text_of(key, item)?);
                    self.give(option, Some(path));
                }
            }
            Takes::Install => {
                let text = text_of(key, v)?;
                let path = if text.is_empty() {
                    self.folder.to_string_lossy().into_owned()
                } else {
                    path_of(self.folder, &text)
                };
                self.give("--dats", Some(path));
            }
            Takes::LogLevel => match text_of(key, v)?.as_str() {
                "debug" | "trace" => self.give("--verbose", None),
                "info" | "warn" | "error" => {}
                other => self.warnings.push(format!(
                    "{key}: {other:?} is none of error, warn, info, debug or trace; logging at info"
                )),
            },
            Takes::PerAddress => {
                let n = v
                    .as_integer()
                    .ok_or_else(|| format!("{key}: a whole number, -1 for no limit"))?;
                let text = if n < 0 {
                    "unlimited".to_owned()
                } else {
                    n.to_string()
                };
                self.give("--max-sessions-per-address", Some(text));
            }
            Takes::WorldDatabase => {
                let text = text_of(key, v)?;
                if !text.is_empty() && text != "pinned" {
                    let path = path_of(self.folder, &text);
                    self.give("--world-db", Some(path));
                }
            }
            // Read whole, with the section ([`hub`]).
            Takes::Hub => {}
        }
        Ok(())
    }
}

/// The hub's address as the server's `--hub` takes it: an `https://` hub for a `wss://` one, and
/// `http://` for `ws://` (a hub on this machine).
fn hub_address(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        url.to_owned()
    }
}

/// `[federation]`'s options: the world listed at the hub (`listed`), or linked to it as well
/// (`enabled`), with the hub's address, the world's slug and its key file; nothing when neither.
fn hub(r: &mut Reading<'_>, section: &toml::Table) -> Result<(), String> {
    let on = |key: &str| -> Result<bool, String> {
        section
            .get(key)
            .map_or(Ok(false), |v| flag(&format!("federation.{key}"), v))
    };
    let (enabled, listed) = (on("enabled")?, on("listed")?);
    if !enabled && !listed {
        return Ok(());
    }
    let text = |key: &str, default: &str| -> Result<String, String> {
        section.get(key).map_or(Ok(default.to_owned()), |v| {
            text_of(&format!("federation.{key}"), v)
        })
    };
    let url = text("hub_url", HUB)?;
    r.give("--hub", Some(hub_address(&url)));
    let slug = text("slug", "")?;
    r.give("--hub-slug", Some(slug));
    let key_file = text("key_file", "network.key")?;
    let path = path_of(r.folder, &key_file);
    r.give("--hub-key-file", Some(path));
    if enabled {
        r.give("--federation", None);
        // Empty is a value here: it carries the Global room in no room.
        if section.contains_key("global_room") {
            let room = text("global_room", "")?;
            r.give("--global-room", Some(room));
        }
    }
    Ok(())
}

/// The command-line arguments a file's text gives, read as from `folder`, with a warning for each
/// key it does not know and each of Empyrean's it has no setting for.
///
/// # Errors
/// The text is not TOML, or a key's value is not one its option takes.
pub fn arguments(text: &str, folder: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let table: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
    let mut r = Reading {
        folder,
        args: Vec::new(),
        warnings: Vec::new(),
    };
    read_table(&mut r, &table, "")?;
    // The world service's port, unless the file names it: two above the login's.
    let network = table
        .get("server")
        .and_then(|s| s.get("network"))
        .and_then(toml::Value::as_table);
    if let Some(n) = network.filter(|n| !n.contains_key("world_port")) {
        if let Some(port) = n.get("port").and_then(toml::Value::as_integer) {
            r.give("--world-port", Some((port + 2).to_string()));
        }
    }
    if let Some(f) = table.get("federation").and_then(toml::Value::as_table) {
        hub(&mut r, f)?;
    }
    Ok((r.args, r.warnings))
}

/// The keys of one section (`at`, "" for the file's top), and the sections inside it.
fn read_table(r: &mut Reading<'_>, table: &toml::Table, at: &str) -> Result<(), String> {
    for (name, value) in table {
        let key = if at.is_empty() {
            name.clone()
        } else {
            format!("{at}.{name}")
        };
        if EMPYREAN_ONLY.contains(&key.as_str()) {
            r.warnings.push(format!(
                "{key}: Empyrean's setting, which Yalain does not have; ignored"
            ));
            continue;
        }
        if SECTIONS.contains(&key.as_str()) {
            let section = value
                .as_table()
                .ok_or_else(|| format!("{key}: a section ([{key}])"))?;
            read_table(r, section, &key)?;
            continue;
        }
        let Some(&(_, _, takes)) = KEYS.iter().find(|(s, k, _)| *s == at && *k == name) else {
            r.warnings
                .push(format!("{key}: not a setting of the server; ignored"));
            continue;
        };
        r.key(&key, takes, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The file's keys, in `empyrean.toml`'s sections, read as their command-line options with
    /// their values: a setting on by default given when false, a list as the option given once
    /// per item, a relative path from the file's folder, -1 sessions an address as no limit, the
    /// world port two above the login port unless named; the command line read after the file
    /// wins; an unknown key, and Empyrean's that Yalain has no setting for, are warned about and
    /// left out.
    /// Behaviour: server.operator.config-file-sets-every-option
    #[test]
    fn a_config_file_in_empyreans_form_sets_the_options_and_the_command_line_wins() {
        let folder = Path::new("/srv/yalain");
        let text = r#"
            [server]
            world_name = "Dawnsong"
            dat_files_directory = "ac2"
            message_of_the_day = "Welcome.\nBe kind."
            world_database_path = "pinned"
            log_level = "debug"
            interactive_console = true
            colour = "red"

            [server.network]
            host = "0.0.0.0"
            port = 9100
            maximum_allowed_sessions = 40
            maximum_allowed_sessions_per_ip_address = -1
            friends_service = true
            world_referral = false

            [server.accounts]
            allow_auto_account_creation = false
            accounts = ["a:one", "b:two"]
            admins = ["a"]
            serve_admin_library = true
            admin_commands = ["ban:moderators"]
            banned_names_path = "/etc/yalain/banned.txt"

            [database]
            shard_db_path = "worlds/dawnsong.sqlite"
            in_memory = true

            [era]
            profile = "eor"
        "#;
        let (args, mut warnings) = arguments(text, folder).expect("reads");
        // Their order is the table's, which a build's features can change.
        warnings.sort();
        assert_eq!(
            warnings,
            [
                "era: Empyrean's setting, which Yalain does not have; ignored",
                "server.colour: not a setting of the server; ignored",
                "server.interactive_console: Empyrean's setting, which Yalain does not have; \
                 ignored",
            ]
        );
        let dats = folder.join("ac2").to_string_lossy().into_owned();
        let data = folder
            .join("worlds/dawnsong.sqlite")
            .to_string_lossy()
            .into_owned();
        for pair in [
            ["--dats", dats.as_str()],
            ["--world-name", "Dawnsong"],
            ["--bind", "0.0.0.0"],
            ["--login-port", "9100"],
            ["--world-port", "9102"],
            ["--max-sessions", "40"],
            ["--max-sessions-per-address", "unlimited"],
            ["--motd", "Welcome.\nBe kind."],
            ["--add-account", "a:one"],
            ["--add-account", "b:two"],
            ["--admin", "a"],
            ["--admin-command", "ban:moderators"],
            ["--banned-names", "/etc/yalain/banned.txt"],
            ["--data", data.as_str()],
        ] {
            assert!(args.windows(2).any(|w| w == pair), "{pair:?} in {args:?}");
        }
        for given in [
            "--verbose",
            "--memory",
            "--listed-accounts",
            "--no-referral",
        ] {
            assert!(args.contains(&given.to_owned()), "{given} in {args:?}");
        }
        for not in ["--no-friends", "--world-db", "--hub", "--federation"] {
            assert!(!args.contains(&not.to_owned()), "{not} not in {args:?}");
        }

        let mut all = args.clone();
        all.extend(strings(&[
            "--world-name",
            "Other",
            "--add-account",
            "c:three",
        ]));
        let o = crate::parse_from(all).expect("parses");
        assert_eq!(o.world_name, "Other", "the command line wins");
        assert_eq!((o.login_port, o.world_port), (9100, 9102));
        assert_eq!(o.max_sessions, 40);
        assert_eq!(o.motd, "Welcome.\nBe kind.");
        assert!(o.memory && o.listed && !o.referral);
        assert!(o.serve_admin_library);
        assert_eq!(o.add_accounts.len(), 3, "the file's and the command line's");
        assert_eq!(o.dats.as_deref(), Some(folder.join("ac2").as_path()));

        // A world port named is the one taken; an empty install is the file's folder.
        let (args, _) = arguments(
            "[server]\ndat_files_directory = \"\"\n[server.network]\nport = 9100\nworld_port = 9200\n",
            folder,
        )
        .expect("reads");
        assert!(args.windows(2).any(|w| w == ["--world-port", "9200"]));
        assert!(!args.windows(2).any(|w| w == ["--world-port", "9102"]));
        let here = folder.to_string_lossy().into_owned();
        assert!(args.windows(2).any(|w| w == ["--dats", here.as_str()]));

        assert!(
            arguments("[database]\nin_memory = 1", folder).is_err(),
            "a flag takes true or false"
        );
        assert!(arguments("server = 3", folder).is_err(), "a section");
        assert!(arguments("[server", folder).is_err(), "not TOML");
        // Every option a key gives is one the command line takes.
        for (_, _, takes) in KEYS {
            let a = match takes {
                Takes::Value(o)
                | Takes::Text(o)
                | Takes::Whole(o)
                | Takes::Path(o)
                | Takes::Many(o)
                | Takes::ManyPaths(o) => {
                    vec![(*o).to_owned(), "1".into()]
                }
                Takes::Flag(o) | Takes::Unless(o) => vec![(*o).to_owned()],
                _ => continue,
            };
            if let Err(e) = crate::parse_from(a) {
                assert!(!e.starts_with("unknown option"), "{takes:?}: {e}");
            }
        }
    }

    /// `[status]` is Empyrean's: the status ping on by default, `udp_ping = false` turning it off,
    /// and its limits by Empyrean's names (four hellos and four asks a minute from one address,
    /// fifty replies a second unless set).
    /// Behaviour: server.operator.status-ping-on-the-login-port
    #[test]
    fn the_status_section_turns_the_ping_off_and_sets_its_limits() {
        let folder = Path::new("/srv/yalain");
        let o = crate::parse_from(Vec::<String>::new()).expect("parses");
        assert!(o.status_ping, "on by default");
        assert_eq!(
            o.status_ping_limits,
            yalain_net::status_ping::Limits::default()
        );
        let (args, warnings) = arguments(
            "[status]\nudp_ping = false\nhellos_per_minute = 6\nasks_per_minute = 7\n\
             replies_per_second = 20\n",
            folder,
        )
        .expect("reads");
        assert!(warnings.is_empty(), "{warnings:?}");
        let o = crate::parse_from(args).expect("parses");
        let ping = crate::status_ping(&o);
        assert!(!ping.on, "the ping as the server runs it");
        let l = ping.limits;
        assert_eq!(
            (l.hellos_per_minute, l.asks_per_minute, l.replies_per_second),
            (6, 7, 20)
        );
        let (args, _) = arguments("[status]\nudp_ping = true\n", folder).expect("reads");
        assert!(crate::status_ping(&crate::parse_from(args).expect("parses")).on);
    }

    /// `[federation]` lists the world at the hub (`listed`) or links it too (`enabled`), each with
    /// the hub's address (a `wss://` hub's posts go to its `https://` address, a `ws://` one's on
    /// this machine to `http://`), the slug and the key file from the file's folder; neither
    /// gives no hub at all.
    /// Behaviour: server.operator.config-file-sets-every-option
    #[test]
    fn the_federation_section_lists_or_links_the_world_at_its_hub() {
        let folder = Path::new("/srv/yalain");
        let key = folder.join("network.key").to_string_lossy().into_owned();
        let (args, _) = arguments(
            "[federation]\nenabled = true\nslug = \"dawnsong\"\n",
            folder,
        )
        .expect("reads");
        for pair in [
            ["--hub", "https://hub.dereth.network"],
            ["--hub-slug", "dawnsong"],
            ["--hub-key-file", key.as_str()],
        ] {
            assert!(args.windows(2).any(|w| w == pair), "{pair:?} in {args:?}");
        }
        assert!(args.contains(&"--federation".to_owned()));
        let (args, _) = arguments(
            "[federation]\nlisted = true\nhub_url = \"ws://127.0.0.1:8090\"\nslug = \"dawnsong\"\n",
            folder,
        )
        .expect("reads");
        assert!(args
            .windows(2)
            .any(|w| w == ["--hub", "http://127.0.0.1:8090"]));
        assert!(!args.contains(&"--federation".to_owned()), "listed alone");
        let (args, _) = arguments(
            "[federation]\nslug = \"dawnsong\"\nhub_url = \"wss://hub.example\"\n",
            folder,
        )
        .expect("reads");
        assert!(args.is_empty(), "neither listed nor linked: {args:?}");
        // The server takes them: a key file beside the configuration file.
        let dir = std::env::temp_dir().join(format!("yalain-config-hub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        std::fs::write(
            dir.join("network.key"),
            format!("drth_{}\n", "a".repeat(43)),
        )
        .expect("the key");
        let given = arguments("[federation]\nenabled = true\nslug = \"dawnsong\"\n", &dir)
            .expect("reads")
            .0;
        let o = crate::parse_from(given).expect("parses");
        assert!(o.federation && o.hub.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `[server.websocket]` takes each of Empyrean's keys by its name, into the settings Empyrean's
    /// own read: a key both lists share, off unless enabled, Empyrean's defaults when left out, a
    /// relative certificate from the file's folder, and no warning.
    /// Behaviour: server.operator.websocket-settings-read-as-empyreans
    #[test]
    fn the_websocket_section_takes_empyreans_keys_into_empyreans_settings() {
        use dereth_server_websocket::settings::{WebSocketSettings, KEYS};
        let shared: Vec<&str> = KEYS.to_vec();
        let ours: Vec<&str> = super::KEYS
            .iter()
            .filter(|(section, ..)| *section == "server.websocket")
            .map(|(_, key, _)| *key)
            .collect();
        assert_eq!(ours, shared, "the keys, in the shared order");
        assert!(!EMPYREAN_ONLY.contains(&"server.websocket"));

        let folder = Path::new("/srv/yalain");
        let o = crate::parse_from(Vec::<String>::new()).expect("parses");
        assert_eq!(o.websocket, WebSocketSettings::default(), "the defaults");
        let (args, warnings) = arguments("[server.websocket]\n", folder).expect("reads");
        assert!(
            args.is_empty() && warnings.is_empty(),
            "{args:?} {warnings:?}"
        );

        let text = r#"
            [server.websocket]
            enabled = true
            listen = "10.0.0.5:9180"
            tls_certificate = "cert.pem"
            tls_private_key = "/etc/yalain/key.pem"
            behind_tls_proxy = true
            trusted_proxies = ["10.0.0.1", "10.0.0.2"]
            allowed_origins = ["https://play.example.org"]
            maximum_connections_per_ip_address = -1
            idle_timeout = "90"
            public_url = "wss://play.example.org/ws"
        "#;
        let (args, warnings) = arguments(text, folder).expect("reads");
        assert!(warnings.is_empty(), "{warnings:?}");
        let o = crate::parse_from(args).expect("parses");
        assert_eq!(
            o.websocket,
            WebSocketSettings {
                enabled: true,
                listen: "10.0.0.5:9180".to_owned(),
                tls_certificate: folder.join("cert.pem").to_string_lossy().into_owned(),
                tls_private_key: "/etc/yalain/key.pem".to_owned(),
                behind_tls_proxy: true,
                trusted_proxies: vec!["10.0.0.1".to_owned(), "10.0.0.2".to_owned()],
                allowed_origins: vec!["https://play.example.org".to_owned()],
                maximum_connections_per_ip_address: -1,
                idle_timeout: 90,
                public_url: "wss://play.example.org/ws".to_owned(),
            }
        );
        assert!(
            arguments("[server.websocket]\nidle_timeout = 1.5\n", folder).is_err(),
            "a whole number"
        );
        assert!(
            arguments("[server.websocket]\nenabled = 1\n", folder).is_err(),
            "a flag takes true or false"
        );
    }

    /// The server stops at its start, in Empyrean's words, for each setting Empyrean's refuses,
    /// while the endpoint is on: a plain listener off loopback, half a certificate, an address that
    /// is none, and a proxy that is no address. Off, none is read.
    /// Behaviour: server.operator.websocket-settings-refused-as-empyreans
    #[test]
    fn a_websocket_setting_empyrean_refuses_stops_the_server_at_its_start() {
        let folder = Path::new("/srv/yalain");
        let started = |keys: &str| {
            let text = format!("[server.websocket]\n{keys}\n");
            crate::parse_from(arguments(&text, folder).expect("reads").0)
        };
        let refusal = |keys: &str| started(&format!("enabled = true\n{keys}")).err();
        let plain = refusal("listen = \"0.0.0.0:9443\"").expect("refused");
        assert!(
            plain.contains("plain ws://") && plain.contains("0.0.0.0:9443"),
            "{plain}"
        );
        assert!(
            refusal("").is_some(),
            "the default listen address is not loopback"
        );
        let half = refusal("listen = \"0.0.0.0:9443\"\ntls_certificate = \"cert.pem\"");
        assert!(half.expect("refused").contains("set together"));
        let half = refusal("listen = \"127.0.0.1:9180\"\ntls_private_key = \"key.pem\"");
        assert!(half.expect("refused").contains("set together"));
        let listen = refusal("listen = \"localhost:9443\"").expect("refused");
        assert!(listen.contains("is not an ip:port"), "{listen}");
        let empty = refusal("listen = \"\"").expect("refused");
        assert!(empty.contains("is not an ip:port"), "{empty}");
        let proxy = refusal("listen = \"127.0.0.1:9180\"\ntrusted_proxies = [\"nginx\"]");
        assert!(proxy.expect("refused").contains("not an IP address"));

        for allowed in [
            "listen = \"127.0.0.1:9180\"",
            "listen = \"[::1]:9180\"",
            "listen = \"0.0.0.0:9443\"\nbehind_tls_proxy = true",
            "listen = \"0.0.0.0:9443\"\ntls_certificate = \"c\"\ntls_private_key = \"k\"",
        ] {
            let got = started(&format!("enabled = true\n{allowed}"));
            assert!(got.is_ok(), "{allowed}: {:?}", got.err());
        }
        assert!(
            started("listen = \"0.0.0.0:9443\"").is_ok(),
            "off, a listener is not checked"
        );
    }

    /// `federation.min_character_age_seconds` is Empyrean's key, and reaches the server as its
    /// option.
    /// Behaviour: server.network.too-new-sends-nothing
    #[test]
    fn empyreans_least_age_key_reaches_the_server() {
        let (args, warnings) = arguments(
            "[federation]\nmin_character_age_seconds = 600\n",
            Path::new("."),
        )
        .expect("reads");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(args.windows(2).any(|w| w == ["--min-character-age", "600"]));
    }

    /// `federation.global_room` is Empyrean's key, in Empyrean's form: it reaches the server as
    /// named, empty included, left out the world carries the Global room in General, and anything
    /// but the four rooms and empty stops the server. A world only listed reads none of it.
    /// Behaviour: server.operator.global-room-setting
    #[test]
    fn the_global_room_setting_takes_the_four_rooms_or_empty_and_refuses_anything_else() {
        let dir = std::env::temp_dir().join(format!("yalain-config-room-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        std::fs::write(
            dir.join("network.key"),
            format!("drth_{}\n", "a".repeat(43)),
        )
        .expect("the key");
        let linked = |room: &str| {
            format!("[federation]\nenabled = true\nslug = \"dawnsong\"\nglobal_room = \"{room}\"\n")
        };
        let settings = |text: &str| -> Result<yalain_federation::Settings, String> {
            let (args, warnings) = arguments(text, &dir)?;
            assert_eq!(warnings, Vec::<String>::new());
            let o = crate::parse_from(args)?;
            crate::network::settings(
                o.hub.as_ref().expect("a hub"),
                None,
                o.global_room.as_deref(),
            )
        };

        let unset = settings("[federation]\nenabled = true\nslug = \"dawnsong\"\n").expect("reads");
        assert_eq!(unset.global_room, Some("general"), "the default, and on");
        assert!(!unset.global_room_named);
        for room in ["general", "trade", "lfg", "roleplay"] {
            let s = settings(&linked(room)).expect(room);
            assert_eq!(s.global_room, Some(room));
            assert!(s.global_room_named);
        }
        let upper = settings(&linked(" Roleplay ")).expect("any case");
        assert_eq!(upper.global_room, Some("roleplay"));
        let empty = settings(&linked("")).expect("empty");
        assert_eq!(empty.global_room, None, "no room");
        assert!(empty.global_room_named);

        for bad in ["off", "pvp", "global", "society"] {
            let why = settings(&linked(bad)).expect_err(bad);
            assert!(why.starts_with("federation.global_room"), "{why}");
        }

        let (args, _) = arguments(
            "[federation]\nlisted = true\nslug = \"dawnsong\"\nglobal_room = \"pvp\"\n",
            &dir,
        )
        .expect("reads");
        assert!(!args.contains(&"--global-room".to_owned()), "{args:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The example file shipped beside the server reads with no warning, every key in it
    /// uncommented, and names every key a file takes.
    /// Behaviour: server.operator.config-file-sets-every-option
    #[test]
    fn the_example_config_file_names_every_key_and_reads_whole() {
        let example = include_str!("../yalain.toml.example");
        let all: String = example
            .lines()
            .map(|l| {
                // A key left as a comment (`# key = value`), not a sentence.
                l.strip_prefix("# ")
                    .filter(|r| {
                        r.split_once(" = ").is_some_and(|(k, _)| {
                            !k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                        })
                    })
                    .unwrap_or(l)
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let (args, warnings) = arguments(&all, Path::new("/srv")).expect("reads");
        assert!(warnings.is_empty(), "{warnings:?}");
        for (section, key, _) in KEYS {
            let head = format!("[{section}]");
            let body = all
                .split_once(&head)
                .map(|(_, rest)| rest.split("\n[").next().unwrap_or(""))
                .unwrap_or_else(|| panic!("{head} is in the example"));
            assert!(
                body.lines().any(|l| l.starts_with(&format!("{key} = "))),
                "{section}.{key} is in the example"
            );
        }
        assert!(args.contains(&"--max-sessions".to_owned()));
    }

    /// `[server.network] download_bytes_per_second` sets the rate of large downloads, retail's when
    /// it is not given; a rate no download does well at stops the server at its start.
    /// Behaviour: server.net.large-message-rate-setting
    #[test]
    fn the_download_rate_setting_takes_a_rate_and_refuses_one_out_of_bounds() {
        let folder = Path::new("/srv");
        let rate = |n: u64| {
            let text = format!("[server.network]\ndownload_bytes_per_second = {n}\n");
            crate::parse_from(arguments(&text, folder).expect("reads").0).map(|o| o.download_rate)
        };
        assert_eq!(rate(4096), Ok(4096));
        assert_eq!(rate(262_144), Ok(262_144));
        assert_eq!(
            crate::parse_from(Vec::<String>::new())
                .expect("parses")
                .download_rate,
            65_536
        );
        for refused in [0, 4095, 262_145] {
            let e = rate(refused).expect_err("refused");
            assert!(e.contains("--download-rate"), "{e}");
        }
        assert!(rate(1 << 40).is_err(), "more than a rate can be");
    }

    /// `--config` names the file and is taken out of the arguments; without it, `yalain.toml` in
    /// the working folder is read, else the one beside the executable, else none; a named file
    /// that is not there stops the server.
    /// Behaviour: server.operator.config-file-sets-every-option
    #[test]
    fn the_config_file_is_the_one_named_else_the_working_folders_else_the_executables() {
        let base = std::env::temp_dir().join(format!("yalain-config-{}", std::process::id()));
        let (cwd, exe) = (base.join("cwd"), base.join("exe"));
        std::fs::create_dir_all(&cwd).expect("a folder");
        std::fs::create_dir_all(&exe).expect("a folder");
        let none = locate(&mut Vec::new(), &cwd, Some(&exe)).expect("looks");
        assert_eq!(none, None);
        std::fs::write(exe.join(FILE), "").expect("written");
        let beside = locate(&mut Vec::new(), &cwd, Some(&exe)).expect("looks");
        assert_eq!(beside, Some(exe.join(FILE)));
        std::fs::write(cwd.join(FILE), "").expect("written");
        let here = locate(&mut Vec::new(), &cwd, Some(&exe)).expect("looks");
        assert_eq!(here, Some(cwd.join(FILE)));
        let named = base.join("other.toml");
        std::fs::write(&named, "").expect("written");
        let mut args = strings(&["--verbose", "--config", named.to_str().expect("utf-8")]);
        assert_eq!(
            locate(&mut args, &cwd, Some(&exe)).expect("looks"),
            Some(named.clone())
        );
        assert_eq!(args, ["--verbose"]);
        let mut missing = strings(&["--config", "no-such.toml"]);
        assert!(locate(&mut missing, &cwd, Some(&exe)).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
