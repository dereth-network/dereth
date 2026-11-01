//! A Yalain world the launcher runs, kept and run as an Empyrean world is ([`crate::hosts`]): a
//! folder of its own under `hosts/`, its own copy of the server, started and stopped by the same
//! supervisor ([`crate::host_runtime`]), with its console and log.
//!
//! What differs is what Yalain is told and how it says it is up. Its settings are the hosted
//! world's own ([`Settings`]), of which Yalain takes the name, the address it listens on, the login
//! port (it takes that port and the three after it: its world service is two above), whether it
//! makes an account the first time a name logs in, and the player's Asheron's Call 2 install. They
//! are kept as `yalain.json` in the world's folder, and given to Yalain on its command line
//! ([`args`]), with the world's database of accounts and characters beside them. The world
//! database it serves is the one its release pins, which `yalain world fetch` puts in the player's
//! cache before its first start ([`FETCHED`]). It is up once its log says where it listens.
//!
//! A world others may join listens on every address, and Yalain's login then refers each player
//! to the world service at the one address it is told (`--advertise`), which Yalain will not start
//! without. That is the address the launcher gives players for the world, the one in its join
//! link ([`advertised`]): this computer's address on the internet when its router has one of its
//! own, else its address on its network. The launcher writes it beside the settings at each start
//! ([`ADVERTISED`]), so a world run in the background is told it too.

use std::ffi::OsString;
use std::path::Path;

use dereth_launch::auberean as ac2;

use crate::host_config::Settings;

/// The world's settings, in its folder.
pub const CONFIG: &str = "yalain.json";

/// The world's database of accounts and characters, in its folder.
pub const DATABASE: &str = "yalain.sqlite";

/// Written once `yalain world fetch` has put the pinned world database in the cache.
pub const FETCHED: &str = "world-database.fetched";

/// The ports a world takes from its login port on.
pub const PORTS: u16 = 4;

/// The address players are referred to, in the world's folder, written at each start by the
/// launcher for a world others may join.
pub const ADVERTISED: &str = "advertised.txt";

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

/// Keep the address players are referred to in the world's folder `dir` for its next start, or
/// take it away for a world on this computer alone.
///
/// # Errors
/// When the file cannot be written.
pub fn keep_advertised(dir: &Path, address: Option<std::net::Ipv4Addr>) -> Result<(), String> {
    let file = dir.join(ADVERTISED);
    match address {
        Some(a) => std::fs::write(file, a.to_string()).map_err(|e| e.to_string()),
        None => match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

/// The settings kept in the world's folder `dir`.
///
/// # Errors
/// In words for a person, when there are none or they do not read.
pub fn read(dir: &Path) -> Result<Settings, String> {
    let bytes = std::fs::read(dir.join(CONFIG)).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("The world's settings do not read: {e}"))
}

/// Keep `settings` in the world's folder `dir`.
///
/// # Errors
/// When they cannot be written.
pub fn write(dir: &Path, settings: &Settings) -> Result<(), String> {
    crate::host_runtime::write_json(&dir.join(CONFIG), settings)
}

/// What is wrong with settings for a Yalain world, in words for a person.
///
/// # Errors
/// A name, address or port Yalain cannot take, or an install that is not ready.
pub fn validate(settings: &Settings) -> Result<(), String> {
    if settings.name.trim().is_empty()
        || settings.name.len() > 128
        || settings.name.chars().any(char::is_control)
    {
        return Err("Give the world a name of up to 128 characters.".into());
    }
    settings
        .bind
        .parse::<std::net::IpAddr>()
        .map_err(|_| "Enter an IP address to listen on.")?;
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

/// Yalain's command line for the world in `dir` with `settings`: the install, the world's own
/// database, where it listens, its name, and who may log in.
#[must_use]
pub fn args(dir: &Path, settings: &Settings) -> Vec<OsString> {
    let mut out: Vec<OsString> = vec![
        "--dats".into(),
        settings.dats.clone().into(),
        "--data".into(),
        dir.join(DATABASE).into(),
        "--bind".into(),
        settings.bind.clone().into(),
        "--login-port".into(),
        settings.port.to_string().into(),
        "--world-port".into(),
        (settings.port + 2).to_string().into(),
        "--world-name".into(),
        settings.name.clone().into(),
    ];
    if !settings.accounts {
        out.push("--listed-accounts".into());
    }
    let advertised = std::fs::read_to_string(dir.join(ADVERTISED)).unwrap_or_default();
    if others_allowed(&settings.bind) && !advertised.trim().is_empty() {
        out.extend(["--advertise".into(), advertised.trim().into()]);
    }
    out
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

/// The world database's fetch, run by Yalain `program` in `dir`: `yalain world fetch`, which puts
/// the release its build pins in the player's cache (once for every world).
///
/// # Errors
/// What Yalain said when it could not.
pub fn fetch(program: &Path, dir: &Path) -> Result<(), String> {
    let out = crate::components::command(program)
        .args(ac2::WORLD_FETCH_ARGS)
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
    std::fs::write(dir.join(FETCHED), b"").map_err(|e| e.to_string())
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
    //! Behaviour: none (the launcher's Yalain worlds: their settings, and Yalain's command line).
    use super::*;

    fn settings(dats: &Path) -> Settings {
        Settings {
            name: "Dawnsong".into(),
            dats: dats.to_path_buf(),
            bind: "0.0.0.0".into(),
            port: 9200,
            accounts: true,
            ..Settings::default()
        }
    }

    #[test]
    fn yalain_is_told_the_install_its_database_where_it_listens_its_name_and_who_may_log_in() {
        let dir = Path::new("/hosts/host-1");
        let mut s = settings(Path::new("/ac2"));
        let shown = |args: Vec<OsString>| {
            args.iter()
                .map(|a| a.to_string_lossy().replace('\\', "/"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shown(args(dir, &s)),
            [
                "--dats",
                "/ac2",
                "--data",
                "/hosts/host-1/yalain.sqlite",
                "--bind",
                "0.0.0.0",
                "--login-port",
                "9200",
                "--world-port",
                "9202",
                "--world-name",
                "Dawnsong"
            ]
        );
        s.accounts = false;
        assert_eq!(shown(args(dir, &s)).last().unwrap(), "--listed-accounts");
        assert!(is_up(
            "listening on 0.0.0.0:9200/9201 (login) and 0.0.0.0:9202/9203 (world)"
        ));
        assert_eq!(local_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(local_host("192.168.1.9"), "192.168.1.9");
    }

    /// A world others may join refers its players to the address in its join link: the internet
    /// address when the router has one of its own, else the one on the network; a world on this
    /// computer alone is told none; and an address the launcher does not know stops the start.
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

        // The address kept for the next start goes on Yalain's command line.
        let d = tempfile::tempdir().unwrap();
        let mut s = settings(Path::new("/ac2"));
        keep_advertised(d.path(), ip("203.0.113.7")).unwrap();
        let tail: Vec<String> = args(d.path(), &s)
            .iter()
            .rev()
            .take(2)
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(tail, ["203.0.113.7", "--advertise"]);
        // A world on this computer alone is told none, whatever was kept.
        s.bind = "127.0.0.1".into();
        assert!(!args(d.path(), &s).iter().any(|a| a == "--advertise"));
        keep_advertised(d.path(), None).unwrap();
        keep_advertised(d.path(), None).unwrap();
        assert!(!d.path().join(ADVERTISED).exists());
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
    }

    #[test]
    fn the_settings_are_kept_in_the_world_s_folder() {
        let d = tempfile::tempdir().unwrap();
        let s = settings(Path::new("/ac2"));
        write(d.path(), &s).unwrap();
        let back = read(d.path()).unwrap();
        assert_eq!((back.name, back.port, back.dats), (s.name, s.port, s.dats));
        assert!(!prepared(d.path()));
    }
}
