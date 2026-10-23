//! A join link: a world's host, port and name in one link a host sends a friend, which opens the
//! launcher's Add world with the world filled in, for the friend to confirm.
//!
//! Two forms carry the same three things:
//!
//! - **`dereth://join?host=<host>&port=<port>&name=<name>`**, the launcher's own scheme, which the
//!   system hands the launcher (registered as [`windows_registry`] and [`linux_mimeapps`] say, and
//!   in the macOS bundle's `Info.plist`);
//! - **`https://dereth.network/join#host=<host>&port=<port>&name=<name>`**, a page any chat app
//!   shows as a link: it offers to open the first form, or to get Dereth. The details are after the
//!   `#`, which a browser never sends, so the site sees none of them.
//!
//! A link only fills the form: it never connects, logs in or carries an account or a password,
//! and anything else it holds is ignored. Its host and port are checked as a world added by hand
//! is ([`crate::state::custom_host`], [`crate::state::custom_port`]); its name is cut to
//! [`MAX_NAME`] characters, with no control characters.
//!
//! **Handing a link to the launcher that is already open.** The system starts a second launcher
//! with the link; that one writes the link to [`REQUEST_FILE`] in the launcher's data folder, and
//! the launcher already open takes it ([`take_request`]) and shows it. [`RUNNING_FILE`] there says
//! a launcher is open.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::state::{custom_host, custom_port, CustomWorldError};

/// The launcher's URL scheme.
pub const SCHEME: &str = "dereth";

/// The site's page for a join link.
pub const WEB_PAGE: &str = "https://dereth.network/join";

/// The most characters of a world's name a link carries.
pub const MAX_NAME: usize = 80;

/// The longest link read, in bytes.
pub const MAX_LINK: usize = 2048;

/// A link waiting for the launcher already open, in the launcher's data folder.
pub const REQUEST_FILE: &str = "join-request.txt";

/// Present while a launcher is open, in the launcher's data folder.
pub const RUNNING_FILE: &str = "launcher-running";

/// What a join link says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinLink {
    pub host: String,
    pub port: u16,
    /// The world's name; its host when the link names none.
    pub name: String,
}

/// Why a link is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// Not a join link at all.
    NotALink,
    /// A join link whose host or port is not one a world can be added with.
    World(CustomWorldError),
}

impl core::fmt::Display for JoinError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotALink => f.write_str("That is not a link to a world."),
            Self::World(e) => write!(f, "The link's world: {e}"),
        }
    }
}

impl std::error::Error for JoinError {}

/// `s` with `%XX` decoded and `+` read as a space; an escape that does not read is kept as it is.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let hex = |i: usize| b.get(i).and_then(|c| char::from(*c).to_digit(16));
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => match (hex(i + 1), hex(i + 2)) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `s` percent-encoded for a query or fragment value: letters, digits and `-._~` as they are,
/// every other byte as `%XX`.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The `key=value` pairs of a query or fragment, decoded.
fn pairs(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

/// The part of `link` after the scheme's `join`, or the site's page: its query or fragment.
fn details(link: &str) -> Option<&str> {
    let lower = link.to_ascii_lowercase();
    let after =
        |prefix: &str| -> Option<usize> { lower.starts_with(prefix).then_some(prefix.len()) };
    if let Some(at) = after("dereth://join").or_else(|| after("dereth:join")) {
        let rest = link[at..].strip_prefix('/').unwrap_or(&link[at..]);
        return rest.strip_prefix('?').or_else(|| rest.strip_prefix('#'));
    }
    for page in [
        "https://dereth.network/join",
        "https://www.dereth.network/join",
    ] {
        if let Some(at) = after(page) {
            let rest = link[at..].strip_prefix('/').unwrap_or(&link[at..]);
            let rest = rest.strip_prefix("index.html").unwrap_or(rest);
            return rest.strip_prefix('#');
        }
    }
    None
}

/// A world's name from a link: its control characters taken out, trimmed, and cut to
/// [`MAX_NAME`] characters; `host` when nothing is left.
fn clean_name(name: &str, host: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    let cut: String = kept.trim().chars().take(MAX_NAME).collect();
    let cut = cut.trim();
    if cut.is_empty() {
        host.to_owned()
    } else {
        cut.to_owned()
    }
}

/// Reads a join link, in either form.
///
/// # Errors
/// [`JoinError::NotALink`] for anything that is not one, or one over [`MAX_LINK`] bytes;
/// [`JoinError::World`] for a link whose host or port a world cannot be added with.
pub fn parse(link: &str) -> Result<JoinLink, JoinError> {
    let link = link.trim();
    if link.len() > MAX_LINK {
        return Err(JoinError::NotALink);
    }
    let details = details(link).ok_or(JoinError::NotALink)?;
    let pairs = pairs(details);
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    };
    let host = custom_host(get("host").unwrap_or("")).map_err(JoinError::World)?;
    let port = custom_port(get("port").unwrap_or("")).map_err(JoinError::World)?;
    Ok(JoinLink {
        host: host.to_owned(),
        port,
        name: clean_name(get("name").unwrap_or(""), host),
    })
}

impl JoinLink {
    fn details(&self) -> String {
        format!(
            "host={}&port={}&name={}",
            encode(&self.host),
            self.port,
            encode(&self.name)
        )
    }

    /// The launcher's own link: `dereth://join?host=...&port=...&name=...`.
    #[must_use]
    pub fn app_link(&self) -> String {
        format!("{SCHEME}://join?{}", self.details())
    }

    /// The site's page for it: `https://dereth.network/join#host=...&port=...&name=...`.
    #[must_use]
    pub fn web_link(&self) -> String {
        format!("{WEB_PAGE}#{}", self.details())
    }
}

/// The link among a program's arguments, as the system passes it: the first that is a join link
/// of the launcher's scheme.
#[must_use]
pub fn from_args(args: &[String]) -> Option<&str> {
    args.iter()
        .map(String::as_str)
        .find(|a| a.to_ascii_lowercase().starts_with("dereth:"))
}

// ----- handing a link to the launcher that is already open -----------------------------------------

/// Leaves `link` for the launcher already open, in its data folder `dir`, whole or not at all.
///
/// # Errors
/// The file could not be written.
pub fn leave_request(dir: &Path, link: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{REQUEST_FILE}.tmp"));
    std::fs::write(&tmp, link.as_bytes())?;
    std::fs::rename(&tmp, dir.join(REQUEST_FILE))
}

/// Whether a link left in `dir` is still waiting.
#[must_use]
pub fn request_waiting(dir: &Path) -> bool {
    dir.join(REQUEST_FILE).is_file()
}

/// The link left in `dir`, taken (the file is removed whether it reads or not); `None` when there
/// is none.
#[must_use]
pub fn take_request(dir: &Path) -> Option<Result<JoinLink, JoinError>> {
    let path = dir.join(REQUEST_FILE);
    let text = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&text[..text.len().min(MAX_LINK + 1)]).into_owned();
    Some(parse(&text))
}

// ----- registering the scheme -----------------------------------------------------------------------

/// One value the launcher's scheme writes in the Windows registry, under
/// `HKEY_CURRENT_USER\Software\Classes`: its key there, its name (`None` for the key's default
/// value), and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryValue {
    pub key: String,
    pub name: Option<String>,
    pub value: String,
}

/// What registers the scheme on Windows for the launcher at `exe`: `dereth` as a URL protocol,
/// with the launcher's icon, opened by the launcher with the link as its one argument.
#[must_use]
pub fn windows_registry(exe: &Path) -> Vec<RegistryValue> {
    let exe = exe.display();
    let value = |key: &str, name: Option<&str>, value: String| RegistryValue {
        key: key.to_owned(),
        name: name.map(str::to_owned),
        value,
    };
    vec![
        value(SCHEME, None, "URL:Dereth".to_owned()),
        value(SCHEME, Some("URL Protocol"), String::new()),
        value(
            &format!("{SCHEME}\\DefaultIcon"),
            None,
            format!("\"{exe}\",0"),
        ),
        value(
            &format!("{SCHEME}\\shell\\open\\command"),
            None,
            format!("\"{exe}\" \"%1\""),
        ),
    ]
}

/// The MIME type a Linux desktop opens the scheme's links by.
pub const LINUX_MIME: &str = "x-scheme-handler/dereth";

/// Where a Linux desktop keeps the user's default applications: `$XDG_CONFIG_HOME/mimeapps.list`
/// when that is an absolute path, else `~/.config/mimeapps.list`.
#[must_use]
pub fn mimeapps_path(var: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("XDG_CONFIG_HOME").map(PathBuf::from) {
        if dir.is_absolute() {
            return Some(dir.join("mimeapps.list"));
        }
    }
    let home = var("HOME").map(PathBuf::from).filter(|h| h.is_absolute())?;
    Some(home.join(".config/mimeapps.list"))
}

/// `current` (the user's `mimeapps.list`, or `None` when there is none) with the launcher's entry
/// as the scheme's default application, every other line kept; `None` when it already is.
#[must_use]
pub fn linux_mimeapps(current: Option<&str>, desktop_file: &str) -> Option<String> {
    let wanted = format!("{LINUX_MIME}={desktop_file}");
    let text = current.unwrap_or("");
    let mut out: Vec<String> = Vec::new();
    let mut in_defaults = false;
    let mut found_section = false;
    let mut set = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_defaults && !set {
                out.push(wanted.clone());
                set = true;
            }
            in_defaults = trimmed == "[Default Applications]";
            found_section |= in_defaults;
        } else if in_defaults
            && trimmed
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == LINUX_MIME)
        {
            if trimmed == wanted && !set {
                set = true;
                out.push(line.to_owned());
            } else if !set {
                out.push(wanted.clone());
                set = true;
            }
            continue;
        }
        out.push(line.to_owned());
    }
    if in_defaults && !set {
        out.push(wanted.clone());
        set = true;
    }
    if !found_section {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[Default Applications]".to_owned());
        out.push(wanted);
        set = true;
    }
    debug_assert!(set);
    let mut new = out.join("\n");
    new.push('\n');
    (new != text).then_some(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(host: &str, port: u16, name: &str) -> JoinLink {
        JoinLink {
            host: host.into(),
            port,
            name: name.into(),
        }
    }

    #[test]
    fn a_join_link_reads_its_host_port_and_name_in_either_form() {
        let want = link("play.example.org", 9000, "Coldeve & Friends");
        for text in [
            "dereth://join?host=play.example.org&port=9000&name=Coldeve%20%26%20Friends",
            "dereth://join/?host=play.example.org&port=9000&name=Coldeve+%26+Friends",
            "DERETH://JOIN?HOST=play.example.org&Port=9000&name=Coldeve%20%26%20Friends",
            "https://dereth.network/join#host=play.example.org&port=9000&name=Coldeve%20%26%20Friends",
            "https://dereth.network/join/#host=play.example.org&port=9000&name=Coldeve%20%26%20Friends",
            "  dereth://join?name=Coldeve%20%26%20Friends&port=9000&host=play.example.org  ",
        ] {
            assert_eq!(parse(text), Ok(want.clone()), "{text}");
        }
    }

    #[test]
    fn a_link_round_trips_through_both_forms() {
        let l = link("192.0.2.10", 9050, "Ünïcode, \"quotes\" & #hash");
        assert_eq!(parse(&l.app_link()), Ok(l.clone()));
        assert_eq!(parse(&l.web_link()), Ok(l.clone()));
        // Escaped as the site's join page writes the same link.
        assert_eq!(
            link("192.0.2.10", 9050, "Coldeve, \"quotes\" & #hash").app_link(),
            "dereth://join?host=192.0.2.10&port=9050&name=Coldeve%2C%20%22quotes%22%20%26%20%23hash"
        );
        assert!(l
            .web_link()
            .starts_with("https://dereth.network/join#host="));
        let web = l.web_link();
        assert_eq!(web.matches('#').count(), 1, "{web}");
        assert!(!web.contains(' ') && !web.contains('"'), "{web}");
    }

    #[test]
    fn a_links_host_and_port_are_checked_as_an_added_worlds_are() {
        assert_eq!(
            parse("dereth://join?port=9000"),
            Err(JoinError::World(CustomWorldError::NoHost))
        );
        for host in ["a%20b", "a:b", "a%2Fb", "%5B2001:db8::1%5D"] {
            assert_eq!(
                parse(&format!("dereth://join?host={host}&port=9000")),
                Err(JoinError::World(CustomWorldError::BadHost)),
                "{host}"
            );
        }
        for port in ["0", "65536", "x", ""] {
            assert_eq!(
                parse(&format!("dereth://join?host=a.example&port={port}")),
                Err(JoinError::World(CustomWorldError::BadPort)),
                "{port}"
            );
        }
    }

    #[test]
    fn a_link_never_carries_an_account_or_password_and_its_name_is_cleaned() {
        let read = parse(
            "dereth://join?host=a.example&port=9000&name=%0D%0AHi%07%20there&account=bob&password=hunter2",
        )
        .unwrap();
        assert_eq!(read, link("a.example", 9000, "Hi there"));
        let long = "x".repeat(200);
        assert_eq!(
            parse(&format!(
                "dereth://join?host=a.example&port=9000&name={long}"
            ))
            .unwrap()
            .name
            .chars()
            .count(),
            MAX_NAME
        );
        assert_eq!(
            parse("dereth://join?host=a.example&port=9000")
                .unwrap()
                .name,
            "a.example",
            "no name: the host"
        );
    }

    #[test]
    fn anything_but_a_join_link_is_not_one() {
        for text in [
            "",
            "https://example.org/join#host=a&port=1",
            "dereth://play?host=a&port=9000",
            "https://dereth.network/join?host=a&port=9000",
            "javascript:alert(1)",
        ] {
            assert_eq!(parse(text), Err(JoinError::NotALink), "{text}");
        }
        assert_eq!(
            parse(&format!(
                "dereth://join?host=a&port=1&name={}",
                "n".repeat(MAX_LINK)
            )),
            Err(JoinError::NotALink),
            "too long"
        );
    }

    #[test]
    fn the_link_is_found_among_the_programs_arguments() {
        let args = vec![
            "--flag".to_owned(),
            "dereth://join?host=a&port=9000".to_owned(),
        ];
        assert_eq!(from_args(&args), Some("dereth://join?host=a&port=9000"));
        assert_eq!(from_args(&["--downloads".to_owned()]), None);
    }

    #[test]
    fn a_link_left_for_the_open_launcher_is_taken_once() {
        let dir = std::env::temp_dir().join(format!("dereth-join-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(take_request(&dir).is_none());
        leave_request(&dir, "dereth://join?host=a.example&port=9000&name=A").unwrap();
        assert!(request_waiting(&dir));
        assert_eq!(take_request(&dir), Some(Ok(link("a.example", 9000, "A"))));
        assert!(
            !request_waiting(&dir) && take_request(&dir).is_none(),
            "taken once"
        );
        leave_request(&dir, "not a link").unwrap();
        assert_eq!(take_request(&dir), Some(Err(JoinError::NotALink)));
        assert!(
            !request_waiting(&dir),
            "a link that does not read is taken too"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_registers_the_scheme_to_open_the_launcher_with_the_link() {
        let values = windows_registry(Path::new(r"C:\Games\Dereth\dereth.exe"));
        let at = |key: &str, name: Option<&str>| {
            values
                .iter()
                .find(|v| v.key == key && v.name.as_deref() == name)
                .map(|v| v.value.as_str())
        };
        assert_eq!(at("dereth", None), Some("URL:Dereth"));
        assert_eq!(at("dereth", Some("URL Protocol")), Some(""));
        assert_eq!(
            at(r"dereth\DefaultIcon", None),
            Some(r#""C:\Games\Dereth\dereth.exe",0"#)
        );
        assert_eq!(
            at(r"dereth\shell\open\command", None),
            Some(r#""C:\Games\Dereth\dereth.exe" "%1""#)
        );
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn linux_makes_the_launcher_the_schemes_default_keeping_every_other_line() {
        let desktop = "network.dereth.dereth.desktop";
        assert_eq!(
            linux_mimeapps(None, desktop).as_deref(),
            Some("[Default Applications]\nx-scheme-handler/dereth=network.dereth.dereth.desktop\n")
        );
        let theirs = "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/html=firefox.desktop\n";
        let ours = linux_mimeapps(Some(theirs), desktop).unwrap();
        assert_eq!(
            ours,
            "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/html=firefox.desktop\nx-scheme-handler/dereth=network.dereth.dereth.desktop\n"
        );
        assert_eq!(linux_mimeapps(Some(&ours), desktop), None, "already ours");
        let elsewhere = "[Default Applications]\nx-scheme-handler/dereth=old.desktop\n[Removed Associations]\na/b=c.desktop\n";
        assert_eq!(
            linux_mimeapps(Some(elsewhere), desktop).as_deref(),
            Some("[Default Applications]\nx-scheme-handler/dereth=network.dereth.dereth.desktop\n[Removed Associations]\na/b=c.desktop\n")
        );
        let no_defaults = "[Added Associations]\ntext/plain=gedit.desktop;\n";
        assert_eq!(
            linux_mimeapps(Some(no_defaults), desktop).as_deref(),
            Some("[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\nx-scheme-handler/dereth=network.dereth.dereth.desktop\n")
        );
        // A home folder that is absolute on the system the test runs on.
        let home = std::env::temp_dir().join("home-a");
        let config = std::env::temp_dir().join("config-a");
        let var = |name: &str| match name {
            "HOME" => Some(home.clone().into_os_string()),
            _ => None,
        };
        assert_eq!(
            mimeapps_path(&var),
            Some(home.join(".config/mimeapps.list"))
        );
        let var = |name: &str| match name {
            "HOME" => Some(home.clone().into_os_string()),
            "XDG_CONFIG_HOME" => Some(config.clone().into_os_string()),
            _ => None,
        };
        assert_eq!(mimeapps_path(&var), Some(config.join("mimeapps.list")));
    }
}
