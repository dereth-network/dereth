//! A Yalain world the launcher runs, kept and run as an Empyrean world is ([`crate::hosts`]): a
//! folder of its own under `hosts/`, its own copy of the server, started and stopped by the same
//! supervisor ([`crate::host_runtime`]), with its console and log.
//!
//! Its settings are kept as Empyrean's are: in a configuration file in the world's folder,
//! `yalain.toml` ([`CONFIG`]), which takes the form of `empyrean.toml` (its sections and key
//! names), and Yalain is started with `--config` naming it and nothing else ([`args`]). The page's
//! settings are the hosted world's own ([`Settings`]), read from the file ([`from_config`]) and
//! saved into it over what it holds ([`update_config`]), so a setting changed by hand is read back
//! and one the page has no field for is kept. Yalain takes the name, the install, where it listens
//! (the login port and the three after it: its world service is two above), its player limits,
//! whether it makes an account the first time a name logs in, its welcome message, its shutdown
//! warning, its log level, its status endpoint (on this computer, where the launcher counts its
//! players), its WebSocket endpoint and the Dereth Network's settings, with the world's database of accounts and
//! characters beside them. The world database it serves is the one its release pins, which `yalain
//! world fetch` puts in the player's cache before its first start ([`FETCHED`]). It is up once its
//! log says where it listens.
//!
//! The supervisor holds Yalain's console, and Yalain is told to stop, saving everyone in the
//! world, when that console's input ends (`stop_at_console_end`): a supervisor that goes away
//! without stopping it (a crash, a kill) stops its world with it rather than leaving it running
//! unwatched.
//!
//! A world others may join listens on every address, and Yalain's login then refers each player
//! to the world service at the one address it is told (`advertised_address`), which Yalain will not
//! start without. That is the address the launcher gives players for the world, the one in its
//! join link ([`advertised`]): this computer's address on the internet when its router has one of
//! its own, else its address on its network. The launcher writes it into the file at each start
//! ([`keep_advertised`]), so a world run in the background is told it too.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use dereth_launch::auberean as ac2;
use toml_edit::DocumentMut;

use crate::host_config::{escaped, put, set, Settings};

/// The world's settings, in its folder.
pub const CONFIG: &str = "yalain.toml";

/// The world's database of accounts and characters, in its folder.
pub const DATABASE: &str = "yalain.sqlite";

/// Written once `yalain world fetch` has put the pinned world database in the cache.
pub const FETCHED: &str = "world-database.fetched";

/// The folder of a world that keeps its world database releases in its own folder rather than in
/// the player's cache: a world run as a Windows service, whose account cannot read the player's
/// cache ([`keep_own_world_database`]). Each release is a folder in it named by its tag.
pub const OWN_DATABASE: &str = "world-database";

/// The ports a world takes from its login port on.
pub const PORTS: u16 = 4;

/// Whether a world listening on `bind` lets players on other computers in.
#[must_use]
pub fn others_allowed(bind: &str) -> bool {
    bind.parse::<std::net::IpAddr>()
        .map_or(true, |ip| !ip.is_loopback())
}

/// The address players are referred to for a world listening on `bind`, from what the launcher
/// knows of this computer: `None` for a world on this computer alone (Yalain refers players to
/// its own address); for one others may join, the address on the internet (`public`) unless it is
/// shared among many homes, else the address on its network (`lan`): the address of the world's
/// join link.
///
/// # Errors
/// In words for a person, when the launcher knows neither address.
pub fn advertised(
    bind: &str,
    lan: Option<std::net::IpAddr>,
    public: Option<std::net::IpAddr>,
    shared_address: bool,
) -> Result<Option<std::net::Ipv4Addr>, String> {
    use std::net::IpAddr;
    if !others_allowed(bind) {
        return Ok(None);
    }
    let v4 = |ip: Option<IpAddr>| match ip {
        Some(IpAddr::V4(v4)) => Some(v4),
        _ => None,
    };
    let outside = if shared_address { None } else { v4(public) };
    outside.or(v4(lan)).map(Some).ok_or_else(|| {
        "The launcher doesn't know this computer's address yet, which players are sent to. Try \
         again in a moment, or untick Allow other players in the world's Settings."
            .to_owned()
    })
}

/// The world's configuration file's text in its folder `dir` (empty when it has none yet).
fn config_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(CONFIG)).unwrap_or_default()
}

/// Keep `text` as the world's configuration file in `dir`, in place of the one before, whole or
/// not at all.
///
/// # Errors
/// When it cannot be written.
pub fn keep_config(dir: &Path, text: &str) -> Result<(), String> {
    let next = dir.join(format!("{CONFIG}.new"));
    let written = std::fs::File::create(&next).and_then(|mut f| {
        use std::io::Write as _;
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&next, dir.join(CONFIG)))
        .map_err(|e| e.to_string())
}

/// Keep the address players are referred to in the world's configuration file in `dir` for its
/// next start, or take it away for a world on this computer alone.
///
/// # Errors
/// When the file does not read or cannot be written.
pub fn keep_advertised(dir: &Path, address: Option<std::net::Ipv4Addr>) -> Result<(), String> {
    let text = config_text(dir);
    let mut doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    match address {
        Some(a) => put(
            &mut doc,
            &["server", "network"],
            "advertised_address",
            a.to_string(),
            "",
        )?,
        None => {
            if let Some(network) = doc
                .get_mut("server")
                .and_then(|s| s.get_mut("network"))
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                network.remove("advertised_address");
            }
        }
    }
    let next = doc.to_string();
    if next == text {
        return Ok(());
    }
    keep_config(dir, &next)
}

/// Whether a configuration file's `text` lets the launcher run the world: Yalain's own updater must
/// be off, as Empyrean's must ([`crate::host_config::validate_managed`]).
///
/// # Errors
/// In words for a person.
pub fn check_updates_off(text: &str) -> Result<(), String> {
    let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let update = doc
        .get("server")
        .and_then(|s| s.get("update"))
        .and_then(toml_edit::Item::as_str)
        .unwrap_or("off");
    if update == "off" {
        Ok(())
    } else {
        Err("The launcher updates this server itself: server.update must be 'off'.".into())
    }
}

/// The settings a configuration file's `text` holds, as the page shows them: Empyrean's keys, with
/// the welcome message from `[server] message_of_the_day`.
///
/// # Errors
/// In words for a person, when the text does not read.
pub fn from_config(text: &str) -> Result<Settings, String> {
    let mut s = Settings::from_config(text)?;
    let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    s.welcome = doc
        .get("server")
        .and_then(|t| t.get("message_of_the_day"))
        .and_then(toml_edit::Item::as_str)
        .unwrap_or("")
        .to_owned();
    Ok(s)
}

/// The settings kept in the world's folder `dir`.
///
/// # Errors
/// In words for a person, when there are none or they do not read.
pub fn read(dir: &Path) -> Result<Settings, String> {
    let text = std::fs::read_to_string(dir.join(CONFIG)).map_err(|e| e.to_string())?;
    from_config(&text)
}

/// The configuration file's `original` text with `settings` saved over it, keeping every key the
/// page has no field for and every comment: Empyrean's keys where Yalain has the setting, the
/// welcome message as `message_of_the_day`, the status endpoint on this computer, the world's own
/// database, and the stop when the supervisor's console closes.
///
/// # Errors
/// The settings are not ones a world takes, or the text does not read.
pub fn update_config(settings: &Settings, original: &str) -> Result<String, String> {
    settings.validate_shared()?;
    settings.websocket.validate()?;
    let mut doc = original.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    for table in ["server", "database"] {
        if doc.get(table).is_none() {
            doc[table] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if !doc[table].is_table() {
            return Err(format!("{table} must be a table."));
        }
    }
    for table in ["network", "accounts"] {
        if doc["server"].get(table).is_none() {
            doc["server"][table] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if !doc["server"][table].is_table() {
            return Err(format!("server.{table} must be a table."));
        }
    }
    let server = &mut doc["server"];
    set(&mut server["world_name"], &settings.name);
    set(
        &mut server["dat_files_directory"],
        settings.dats.to_string_lossy().as_ref(),
    );
    set(
        &mut server["status_address"],
        std::net::SocketAddr::new(settings.status_host, settings.status_port).to_string(),
    );
    set(
        &mut server["message_of_the_day"],
        escaped(&settings.welcome)?,
    );
    set(&mut server["shutdown_interval"], settings.shutdown_warning);
    set(&mut server["log_level"], &settings.log_level);
    set(&mut server["stop_at_console_end"], true);
    // The launcher updates its worlds itself, so Yalain's own updater never races it.
    set(&mut server["update"], "off");
    let network = &mut server["network"];
    set(&mut network["host"], &settings.bind);
    set(&mut network["port"], i64::from(settings.port));
    set(
        &mut network["maximum_allowed_sessions"],
        settings.max_players,
    );
    set(
        &mut network["maximum_allowed_sessions_per_ip_address"],
        settings.max_per_address,
    );
    set(
        &mut server["accounts"]["allow_auto_account_creation"],
        settings.accounts,
    );
    if doc["database"].get("shard_db_path").is_none() {
        doc["database"]["shard_db_path"] = toml_edit::value(DATABASE);
    }
    crate::host_config::put_websocket(&mut doc, &settings.websocket)?;
    let fed = &settings.federation;
    put(&mut doc, &["federation"], "enabled", fed.enabled, false)?;
    put(&mut doc, &["federation"], "slug", fed.slug.as_str(), "")?;
    Ok(doc.to_string())
}

/// Keep `settings` in the world's folder `dir`, saved over its configuration file, its status
/// endpoint kept where the launcher put it.
///
/// # Errors
/// When they cannot be written.
pub fn write(dir: &Path, settings: &Settings) -> Result<(), String> {
    let original = config_text(dir);
    let mut next = settings.clone();
    if let Ok(before) = from_config(&original) {
        if !original.is_empty() {
            next.status_port = before.status_port;
            next.status_host = before.status_host;
        }
    }
    keep_config(dir, &update_config(&next, &original)?)
}

/// What is wrong with settings for a Yalain world, in words for a person.
///
/// # Errors
/// A name, address, port or limit Yalain cannot take, or an install that is not ready.
pub fn validate(settings: &Settings) -> Result<(), String> {
    settings.validate_shared()?;
    settings.websocket.validate()?;
    if settings.port == 0 || settings.port > u16::MAX - (PORTS - 1) {
        return Err(
            "Choose a port from 1 to 65532: the world takes it and the three after it.".into(),
        );
    }
    if !settings.dats.is_absolute() {
        return Err("Choose your Asheron's Call 2 folder.".into());
    }
    let check = ac2::check_install(&settings.dats);
    if !check.ok {
        return Err(check.summary);
    }
    Ok(())
}

/// Yalain's command line for the world in `dir`: its configuration file, as Empyrean's world is
/// given its own.
#[must_use]
pub fn args(dir: &Path) -> Vec<OsString> {
    vec!["--config".into(), dir.join(CONFIG).into()]
}

/// Whether a line of Yalain's log says it is ready for logins.
#[must_use]
pub fn is_up(line: &str) -> bool {
    ac2::is_ready(line)
}

/// Whether the world in `dir` has its world database: fetched once, before its first start.
#[must_use]
pub fn prepared(dir: &Path) -> bool {
    dir.join(FETCHED).is_file()
}

/// Whether the world in `dir` keeps its world database releases in its own folder
/// ([`OWN_DATABASE`]).
#[must_use]
pub fn keeps_own_world_database(dir: &Path) -> bool {
    dir.join(OWN_DATABASE).is_dir()
}

/// Fetch a release of the world database with Yalain `program` for the world in `dir`: the one its
/// build pins, or `version`. Into the player's cache, or the world's own folder when it keeps its
/// own ([`OWN_DATABASE`]). The release's tag, when Yalain said it.
///
/// # Errors
/// What Yalain said when it could not.
pub fn fetch_release(
    program: &Path,
    dir: &Path,
    version: Option<&str>,
) -> Result<Option<String>, String> {
    let mut command = crate::components::command(program);
    command.args(ac2::WORLD_FETCH_ARGS);
    if let Some(version) = version {
        command.args(["--version", version]);
    }
    if keeps_own_world_database(dir) {
        command.arg("--dir").arg(dir.join(OWN_DATABASE));
    }
    let out = command
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Could not run Yalain: {e}"))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "Could not get the world database. {}",
            said.lines().last().unwrap_or("").trim()
        ));
    }
    Ok(fetched_release(&String::from_utf8_lossy(&out.stdout)))
}

/// The world database's fetch, run by Yalain `program` in `dir`: `yalain world fetch`, which puts
/// the release its build pins in the player's cache (once for every world), or in the world's own
/// folder for a world that keeps its own, which then serves it from there unless it plays a
/// published Athenaeum world.
///
/// # Errors
/// What Yalain said when it could not.
pub fn fetch(program: &Path, dir: &Path) -> Result<(), String> {
    let release = fetch_release(program, dir, None)?;
    if let (true, Some(tag)) = (keeps_own_world_database(dir), &release) {
        if crate::host_publication::selected(dir)
            .ok()
            .flatten()
            .is_none()
        {
            serve_own(dir, "world_database_path", tag)?;
        }
    }
    // The release it fetched, for the world's page; nothing when its output does not say.
    std::fs::write(dir.join(FETCHED), release.unwrap_or_default()).map_err(|e| e.to_string())
}

/// Name the release `tag` in the world's own folder as the world's `[server] key` in its
/// configuration, relative to the file: `world_database_path` (the database it serves) or
/// `world_base_path` (the base a published world lies over).
///
/// # Errors
/// A configuration that does not read or cannot be written.
pub fn serve_own(dir: &Path, key: &str, tag: &str) -> Result<(), String> {
    let text = config_text(dir);
    let mut doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    if doc.get("server").is_none() {
        doc["server"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if !doc["server"].is_table() {
        return Err("server must be a table.".into());
    }
    doc["server"][key] = toml_edit::value(format!("{OWN_DATABASE}/{tag}"));
    keep_config(dir, &doc.to_string())
}

/// Keep the world in `dir`'s world database in its own folder from now on, run by Yalain
/// `program`: what a Windows service needs, whose account cannot read the player's cache. The
/// release its Yalain pins is fetched there and served from there; a world that plays a published
/// Athenaeum world has the release that world lies over fetched there too, as its base.
///
/// # Errors
/// A fetch that failed, in Yalain's words.
pub fn keep_own_world_database(program: &Path, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir.join(OWN_DATABASE)).map_err(|e| e.to_string())?;
    fetch(program, dir)?;
    if let Some((_, publication)) = crate::host_publication::selected(dir).ok().flatten() {
        let tag = fetch_release(program, dir, publication.base.as_deref())?
            .ok_or("Yalain did not say which world database it fetched.")?;
        serve_own(dir, "world_base_path", &tag)?;
    }
    Ok(())
}

/// What the Yalain world in `dir`, configured by `text` and run by Yalain `program`, plays: the
/// world database its Yalain pins (`era`, as Empyrean's world plays its era's own), a published
/// Athenaeum world (`athenaeum`), or another database its configuration names (`own`), with when
/// that world was published and the release of the world database under it, when those are known:
/// the release a published world names, else the one its program pins ([`program_pin`]), else the
/// one its first fetch found.
#[must_use]
pub fn plays(text: &str, dir: &Path, program: Option<&Path>) -> crate::host_config::Plays {
    let doc = text.parse::<DocumentMut>().unwrap_or_default();
    // A release in the world's own folder is the pin, kept there rather than in the cache.
    let pack = doc
        .get("server")
        .and_then(|s| s.get("world_database_path"))
        .and_then(toml_edit::Item::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "pinned")
        .filter(|p| {
            !p.replace('\\', "/")
                .starts_with(&format!("{OWN_DATABASE}/"))
        })
        .map(str::to_owned);
    let publication = crate::host_publication::selected(dir).ok().flatten();
    let published = publication
        .as_ref()
        .and_then(|(folder, _)| std::fs::metadata(folder.join("publication.json")).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let version = publication
        .as_ref()
        .and_then(|(_, p)| p.base.clone())
        .or_else(|| program.and_then(program_pin))
        .or_else(|| pinned_release(dir));
    crate::host_config::Plays {
        database: if publication.is_some() {
            "athenaeum"
        } else if pack.is_some() {
            "own"
        } else {
            "era"
        },
        pack: pack.unwrap_or_default(),
        published,
        base: version.map(|version| crate::host_config::WorldBase {
            era: "eor",
            version,
        }),
        ..crate::host_config::Plays::default()
    }
}

/// The release of the world database Yalain `program` pins, as its release facts say
/// (`yalain --release-facts`' `world_database`): asked once for each build of it, by when it was
/// written. `None` when it cannot say.
#[must_use]
pub fn program_pin(program: &Path) -> Option<String> {
    type Said = HashMap<PathBuf, (Option<SystemTime>, Option<String>)>;
    static SAID: OnceLock<Mutex<Said>> = OnceLock::new();
    let written = std::fs::metadata(program).and_then(|m| m.modified()).ok();
    let said = SAID.get_or_init(Mutex::default);
    if let Some((when, pin)) = said
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(program)
    {
        if *when == written {
            return pin.clone();
        }
    }
    let pin = crate::components::command(program)
        .arg("--release-facts")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| pin_from(&String::from_utf8_lossy(&o.stdout)));
    said.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(program.to_path_buf(), (written, pin.clone()));
    pin
}

/// The world database release in what `yalain --release-facts` prints, one line of JSON.
fn pin_from(said: &str) -> Option<String> {
    let facts: serde_json::Value = serde_json::from_str(said.trim()).ok()?;
    let tag = facts.get("world_database")?.as_str()?;
    release_tag(tag).then(|| tag.to_owned())
}

/// The release of the world database the world in `dir`'s Yalain pins, as its fetch found it,
/// when it said.
#[must_use]
pub fn pinned_release(dir: &Path) -> Option<String> {
    let said = std::fs::read_to_string(dir.join(FETCHED)).ok()?;
    let tag = said.trim();
    release_tag(tag).then(|| tag.to_owned())
}

/// Whether `tag` is a release's tag, `v` and three numbers: `v0.21.0`.
fn release_tag(tag: &str) -> bool {
    tag.strip_prefix('v').is_some_and(|v| {
        let parts: Vec<&str> = v.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// The release a fetch's output names: its last line ends with the release's folder in the cache
/// (`v0.21.0 is already cached: <cache>/v0.21.0`, `cached 412 files in <cache>/v0.21.0`).
#[must_use]
pub fn fetched_release(out: &str) -> Option<String> {
    let last = out.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let tag = last.rsplit(['/', '\\']).next()?;
    release_tag(tag).then(|| tag.to_owned())
}

/// The address a player on this computer logs in at, for a world listening on `bind`.
#[must_use]
pub fn local_host(bind: &str) -> String {
    if bind == "0.0.0.0" || bind == "::" {
        "127.0.0.1".into()
    } else {
        bind.to_owned()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's Yalain worlds: their settings in yalain.toml, and Yalain's command line).
    use super::*;

    /// A Yalain that says its release facts, pinning the world database `pin`.
    fn yalain_saying(dir: &Path, pin: &str) -> PathBuf {
        let facts = format!(r#"{{"version":"0.1.2","world_database":"{pin}"}}"#);
        if cfg!(windows) {
            let p = dir.join("yalain.cmd");
            std::fs::write(&p, format!("@echo {facts}\r\n")).unwrap();
            p
        } else {
            let p = dir.join("yalain");
            std::fs::write(&p, format!("#!/bin/sh\necho '{facts}'\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        }
    }

    /// A Yalain world's base is the world database its own Yalain pins, as that program says, not
    /// the release its first fetch found: after an update to a Yalain pinning a newer release, the
    /// newer one. With no program that says, the fetch's.
    #[test]
    fn a_worlds_base_is_the_release_its_own_yalain_pins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FETCHED), "v0.23.0").unwrap();
        let program = yalain_saying(dir.path(), "v0.23.1");
        assert_eq!(program_pin(&program).as_deref(), Some("v0.23.1"));
        let base = |program: &Path| {
            plays("[server]\n", dir.path(), Some(program))
                .base
                .map(|b| b.version)
        };
        assert_eq!(base(&program).as_deref(), Some("v0.23.1"));
        let gone = dir.path().join("gone").join("yalain");
        assert_eq!(base(&gone).as_deref(), Some("v0.23.0"));
        assert_eq!(pin_from(r#"{"world_database":"pinned"}"#), None);
        assert_eq!(pin_from("yalain 0.1.2"), None);
    }

    /// The release a fetch found is read from what it said, whether it fetched it or had it
    /// already, on either system's paths; anything else is no release.
    #[test]
    fn the_release_a_fetch_found_is_read_from_what_it_said() {
        let cached = "fetching https://example.org/yalain-world-v0.21.0.zip\nSHA-256 ab matches\n\
                      cached 412 files in C:\\Users\\you\\AppData\\Local\\yalain\\world\\v0.21.0\n";
        assert_eq!(fetched_release(cached).as_deref(), Some("v0.21.0"));
        let had = "v0.21.0 is already cached: /home/you/.cache/yalain/world/v0.21.0\n";
        assert_eq!(fetched_release(had).as_deref(), Some("v0.21.0"));
        assert_eq!(fetched_release("cached 3 files in /tmp/checkout"), None);
        assert_eq!(fetched_release(""), None);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FETCHED), "v0.21.0").unwrap();
        assert_eq!(pinned_release(dir.path()).as_deref(), Some("v0.21.0"));
        std::fs::write(dir.path().join(FETCHED), "").unwrap();
        assert_eq!(pinned_release(dir.path()), None, "a fetch that did not say");
    }

    fn settings(dats: &Path) -> Settings {
        Settings {
            name: "Dawnsong".into(),
            dats: dats.to_path_buf(),
            bind: "0.0.0.0".into(),
            port: 9200,
            status_port: 9104,
            accounts: true,
            ..Settings::default()
        }
    }

    /// The keys of a TOML text, as `section.key`.
    fn keys(text: &str) -> Vec<String> {
        fn walk(item: &toml_edit::Item, at: &str, out: &mut Vec<String>) {
            if let Some(t) = item.as_table_like() {
                for (k, v) in t.iter() {
                    let path = if at.is_empty() {
                        k.to_owned()
                    } else {
                        format!("{at}.{k}")
                    };
                    if v.is_table_like() {
                        walk(v, &path, out);
                    } else {
                        out.push(path);
                    }
                }
            }
        }
        let doc = text.parse::<DocumentMut>().unwrap();
        let mut out = Vec::new();
        walk(doc.as_item(), "", &mut out);
        out
    }

    /// Yalain is started as Empyrean's world is: with its configuration file and nothing else;
    /// and it is up once its log's tag says it listens.
    #[test]
    fn yalain_is_started_with_its_configuration_file_alone() {
        let dir = Path::new("/hosts/host-1");
        let shown: Vec<String> = args(dir)
            .iter()
            .map(|a| a.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(shown, ["--config", "/hosts/host-1/yalain.toml"]);
        assert!(is_up(
            "2026-11-01T10:00:00Z  INFO yalain: listening on 0.0.0.0:9200/9201 (login) and \
             0.0.0.0:9202/9203 (world, referred as 203.0.113.7) [listening]"
        ));
        assert_eq!(local_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(local_host("192.168.1.9"), "192.168.1.9");
    }

    /// A world others may join refers its players to the address in its join link: the internet
    /// address when the router has one of its own, else the one on the network; a world on this
    /// computer alone is told none; and an address the launcher does not know stops the start.
    /// The address is kept in the world's configuration file, and taken out of it again.
    #[test]
    fn a_world_others_may_join_is_told_the_address_players_reach_it_at() {
        let lan = Some("192.168.1.20".parse().unwrap());
        let public = Some("203.0.113.7".parse().unwrap());
        let ip = |s: &str| Some(s.parse::<std::net::Ipv4Addr>().unwrap());
        assert_eq!(advertised("127.0.0.1", lan, public, false), Ok(None));
        assert_eq!(
            advertised("0.0.0.0", lan, public, false),
            Ok(ip("203.0.113.7"))
        );
        assert_eq!(
            advertised("0.0.0.0", lan, public, true),
            Ok(ip("192.168.1.20"))
        );
        assert_eq!(
            advertised("0.0.0.0", lan, None, false),
            Ok(ip("192.168.1.20"))
        );
        assert!(advertised("0.0.0.0", None, None, false)
            .unwrap_err()
            .contains("untick Allow other players"));
        // An IPv6 address is not one Yalain refers to.
        let v6 = Some("2001:db8::1".parse().unwrap());
        assert!(advertised("0.0.0.0", v6, v6, false).is_err());

        let d = tempfile::tempdir().unwrap();
        write(d.path(), &settings(Path::new("/ac2"))).unwrap();
        keep_advertised(d.path(), ip("203.0.113.7")).unwrap();
        let text = config_text(d.path());
        let doc = text.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["server"]["network"]["advertised_address"].as_str(),
            Some("203.0.113.7"),
            "{text}"
        );
        // A world on this computer alone is told none.
        keep_advertised(d.path(), None).unwrap();
        keep_advertised(d.path(), None).unwrap();
        assert!(!config_text(d.path()).contains("advertised_address"));
    }

    #[test]
    fn a_name_an_address_four_ports_and_a_ready_install_are_needed() {
        let d = tempfile::tempdir().unwrap();
        let s = settings(d.path());
        // An empty folder is no install.
        assert!(validate(&s).unwrap_err().contains("portal.dat"));
        let mut bad = s.clone();
        bad.port = 65533;
        assert!(validate(&bad).unwrap_err().contains("three after it"));
        bad = s.clone();
        bad.name = " ".into();
        assert!(validate(&bad).is_err());
        bad = s.clone();
        bad.bind = "everywhere".into();
        assert!(validate(&bad).is_err());
        bad = s.clone();
        bad.max_players = 0;
        assert!(validate(&bad).is_err(), "the limits Empyrean's world has");
    }

    /// The page's WebSocket fields are `[server.websocket]` in `yalain.toml`, Empyrean's keys, read
    /// back as they were saved, with a key the page has no field for kept; the same settings are
    /// refused as for an Empyrean world.
    #[test]
    fn the_websocket_settings_are_kept_as_empyrean_keeps_them_and_refused_alike() {
        use crate::host_config::WebSocket;
        let d = tempfile::tempdir().unwrap();
        let mut s = settings(Path::new("/ac2"));
        write(d.path(), &s).unwrap();
        assert!(
            !config_text(d.path()).contains("websocket"),
            "a new world's file stays short"
        );
        s.websocket = WebSocket {
            enabled: true,
            listen: "127.0.0.1:9180".into(),
            behind_tls_proxy: true,
            allowed_origins: vec!["https://play.example.org".into()],
            public_url: "wss://play.example.org/ws".into(),
            ..WebSocket::default()
        };
        let edited = format!(
            "{}\n[server.websocket]\n# by hand\ntrusted_proxies = [\"127.0.0.1\"]\n",
            config_text(d.path())
        );
        keep_config(d.path(), &edited).unwrap();
        write(d.path(), &s).unwrap();
        let text = config_text(d.path());
        assert_eq!(read(d.path()).unwrap().websocket, s.websocket, "{text}");
        assert!(
            text.contains("[server.websocket]") && text.contains("listen = \"127.0.0.1:9180\""),
            "{text}"
        );
        assert!(
            text.contains("# by hand\ntrusted_proxies = [\"127.0.0.1\"]"),
            "{text}"
        );
        let refused = |change: fn(&mut WebSocket)| {
            let mut bad = s.clone();
            change(&mut bad.websocket);
            update_config(&bad, "").unwrap_err()
        };
        let plain = refused(|w| {
            w.listen = "0.0.0.0:9443".into();
            w.behind_tls_proxy = false;
        });
        assert!(plain.contains("secure connection"), "{plain}");
        let scheme = refused(|w| w.public_url = "https://play.example.org/ws".into());
        assert!(scheme.contains("wss://"), "{scheme}");
    }

    /// A world the launcher hosts keeps Yalain's own updater off: the settings are saved with it
    /// off, whatever the file said, and a file turned on by hand is refused at the world's start,
    /// in the words Empyrean's world is refused in.
    #[test]
    fn a_hosted_worlds_own_updater_stays_off() {
        let s = settings(Path::new("/ac2"));
        let saved = update_config(&s, "").unwrap();
        assert_eq!(check_updates_off(&saved), Ok(()));
        let by_hand = saved.replace("update = \"off\"", "update = \"patch\"");
        assert_eq!(
            check_updates_off(&by_hand).unwrap_err(),
            "The launcher updates this server itself: server.update must be 'off'."
        );
        assert_eq!(
            check_updates_off(&update_config(&s, &by_hand).unwrap()),
            Ok(())
        );
        assert_eq!(check_updates_off(""), Ok(()), "off by default");
    }

    /// The settings are kept in `yalain.toml` in the form of `empyrean.toml`, in Yalain's keys
    /// alone (none of Empyrean's it has no setting for); a setting changed by hand is read back,
    /// and a key or comment the page has no field for is kept when the page saves over it.
    #[test]
    fn the_settings_are_kept_in_yalain_toml_as_empyrean_keeps_its_own_and_hand_edits_hold() {
        let d = tempfile::tempdir().unwrap();
        let mut s = settings(Path::new("/ac2"));
        s.welcome = "Welcome.\nBe kind.".into();
        s.max_players = 40;
        s.federation.slug = "dawnsong".into();
        write(d.path(), &s).unwrap();
        let text = config_text(d.path());
        let back = read(d.path()).unwrap();
        assert_eq!(
            (
                &back.name,
                back.port,
                &back.dats,
                &back.welcome,
                back.max_players,
                back.status_port,
                &back.federation.slug
            ),
            (
                &s.name,
                s.port,
                &s.dats,
                &s.welcome,
                40,
                9104,
                &"dawnsong".to_owned()
            ),
            "{text}"
        );
        // Only keys Yalain's configuration file has (yalain.toml.example lists them).
        let known = [
            "server.world_name",
            "server.dat_files_directory",
            "server.status_address",
            "server.message_of_the_day",
            "server.shutdown_interval",
            "server.log_level",
            "server.stop_at_console_end",
            "server.update",
            "server.network.host",
            "server.network.port",
            "server.network.maximum_allowed_sessions",
            "server.network.maximum_allowed_sessions_per_ip_address",
            "server.accounts.allow_auto_account_creation",
            "database.shard_db_path",
            "federation.slug",
        ];
        for k in keys(&text) {
            assert!(
                known.contains(&k.as_str()),
                "{k}: not one of Yalain's\n{text}"
            );
        }
        let doc = text.parse::<DocumentMut>().unwrap();
        assert_eq!(doc["server"]["stop_at_console_end"].as_bool(), Some(true));
        assert_eq!(doc["server"]["update"].as_str(), Some("off"));
        assert_eq!(doc["database"]["shard_db_path"].as_str(), Some(DATABASE));
        assert_eq!(
            doc["server"]["status_address"].as_str(),
            Some("127.0.0.1:9104")
        );
        assert!(!prepared(d.path()));

        // Changed by hand: read back; a key and a comment of its own kept by the next save.
        let edited = text.replace("\"Dawnsong\"", "\"Dawnsong Edited\"").replace(
            "[server]\n",
            "[server]\n# kept by hand\nplayer_save_interval = 60\n",
        );
        keep_config(d.path(), &edited).unwrap();
        let mut back = read(d.path()).unwrap();
        assert_eq!(back.name, "Dawnsong Edited");
        back.max_players = 50;
        write(d.path(), &back).unwrap();
        let after = config_text(d.path());
        assert!(after.contains("# kept by hand"), "{after}");
        assert!(after.contains("player_save_interval = 60"), "{after}");
        assert!(after.contains("maximum_allowed_sessions = 50"), "{after}");
        assert_eq!(read(d.path()).unwrap().status_port, 9104, "kept");
    }
}
