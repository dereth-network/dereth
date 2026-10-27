//! `plugin.toml` as data, and the rules a manifest must meet.
//!
//! The host reads the TOML into a [`Value`] (a TOML document is a map) and [`Manifest::from_value`]
//! applies the rules. Unknown keys are refused, so a misspelled permission or capability cannot be
//! silently dropped; a repeated key is already an error of the TOML reader.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use crate::capability::HostCapability;
use crate::permission::Permission;
use crate::value::Value;
use crate::version::ApiVersion;

/// A plugin's id: reverse-DNS, lowercase, at least two labels (`example.xpmeter`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(String);

impl PluginId {
    /// Check and wrap an id. Labels are `a`–`z`, `0`–`9` and `-`, start with a letter, and are
    /// separated by single dots; the whole id is at most 128 bytes.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let ok_label = |l: &str| {
            !l.is_empty()
                && l.as_bytes()[0].is_ascii_lowercase()
                && l.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        };
        (s.len() <= 128 && s.split('.').count() >= 2 && s.split('.').all(ok_label))
            .then(|| Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the id is in the namespace kept for the host itself ([`RESERVED_NAMESPACE`]): the
    /// host's own windows, stores and folders are kept under such an id, and no plugin may take
    /// one.
    #[must_use]
    pub fn is_reserved(&self) -> bool {
        self.0.split('.').next() == Some(RESERVED_NAMESPACE)
    }
}

/// The first label of the ids kept for the host itself: no plugin's id starts `dereth.`.
pub const RESERVED_NAMESPACE: &str = "dereth";

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A SemVer 2 version: `MAJOR.MINOR.PATCH`, with an optional `-pre` and `+build` kept as text.
///
/// Versions order by SemVer's precedence ([`PluginVersion::precedence`]), with the build text as
/// the last tie-break so that only equal versions compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub rest: String,
}

impl PluginVersion {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let split = s.find(['-', '+']).unwrap_or(s.len());
        let (core, rest) = s.split_at(split);
        let mut parts = core.split('.');
        let num = |p: Option<&str>| -> Option<u64> {
            let p = p?;
            (!p.is_empty()
                && p.bytes().all(|c| c.is_ascii_digit())
                && (p == "0" || !p.starts_with('0')))
            .then(|| p.parse().ok())
            .flatten()
        };
        let v = Self {
            major: num(parts.next())?,
            minor: num(parts.next())?,
            patch: num(parts.next())?,
            rest: rest.to_owned(),
        };
        (parts.next().is_none() && (rest.is_empty() || rest.len() > 1)).then_some(v)
    }

    /// The pre-release text after the `-`, without the build; empty for a release.
    #[must_use]
    pub fn pre(&self) -> &str {
        let pre = self.rest.split('+').next().unwrap_or_default();
        pre.strip_prefix('-').unwrap_or_default()
    }

    /// The build text after the `+`; empty when there is none.
    #[must_use]
    pub fn build(&self) -> &str {
        self.rest.split_once('+').map_or("", |(_, b)| b)
    }

    /// SemVer 2's precedence: the numbers in order, then a pre-release below the release it leads
    /// to, pre-releases compared identifier by identifier (numbers numerically and below words,
    /// words by their ASCII text, and a shorter list below a longer one it begins). The build
    /// text plays no part: `1.0.0+a` and `1.0.0+b` have the same precedence.
    #[must_use]
    pub fn precedence(&self, other: &Self) -> Ordering {
        let numbers =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if numbers != Ordering::Equal {
            return numbers;
        }
        match (self.pre(), other.pre()) {
            ("", "") => Ordering::Equal,
            ("", _) => Ordering::Greater,
            (_, "") => Ordering::Less,
            (a, b) => {
                let (mut a, mut b) = (a.split('.'), b.split('.'));
                loop {
                    let (x, y) = match (a.next(), b.next()) {
                        (None, None) => return Ordering::Equal,
                        (None, Some(_)) => return Ordering::Less,
                        (Some(_), None) => return Ordering::Greater,
                        (Some(x), Some(y)) => (x, y),
                    };
                    let order = match (pre_number(x), pre_number(y)) {
                        // Numerically: more digits is larger, then digit by digit.
                        (Some(m), Some(n)) => m.len().cmp(&n.len()).then_with(|| m.cmp(n)),
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => x.cmp(y),
                    };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
            }
        }
    }

    /// Whether this version takes precedence over `other`: a later release.
    #[must_use]
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.precedence(other) == Ordering::Greater
    }
}

/// A pre-release identifier that is a number, without its leading zeros; `None` for a word.
fn pre_number(s: &str) -> Option<&str> {
    (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())).then(|| s.trim_start_matches('0'))
}

impl PartialOrd for PluginVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PluginVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.precedence(other)
            .then_with(|| self.build().cmp(other.build()))
    }
}

impl fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}{}",
            self.major, self.minor, self.patch, self.rest
        )
    }
}

/// A part of the player's session a plugin can be active in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Context {
    /// A character is in the world (portal space included).
    World,
    /// Before the world: the login and character screens, entering the world, after a
    /// disconnection.
    Pregame,
    /// The character creation screens.
    Chargen,
}

impl Context {
    pub const ALL: [Self; 3] = [Self::World, Self::Pregame, Self::Chargen];

    /// The context's name, as manifests and events spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Pregame => "pregame",
            Self::Chargen => "chargen",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == s)
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A checked manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub id: PluginId,
    pub name: String,
    pub version: PluginVersion,
    /// The lowest API version the plugin needs.
    pub api: ApiVersion,
    /// The Lua file loaded first, a path inside the package.
    pub entry: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub homepage: String,
    /// Requested permissions, each with the reason the player is shown.
    pub permissions: BTreeMap<Permission, String>,
    /// Host capabilities the plugin cannot run without.
    pub required: Vec<HostCapability>,
    /// Host capabilities the plugin checks for at run time.
    pub optional: Vec<HostCapability>,
    /// Hosts (by `host.name`) the plugin does not run in.
    pub exclude_hosts: Vec<String>,
    /// The domains its HTTPS requests may reach (`[net] domains`): each a host name, which also
    /// admits its subdomains.
    pub net_domains: Vec<String>,
    /// The parts of the session the plugin is active in (`contexts`); `world` when the manifest
    /// names none.
    pub contexts: Vec<Context>,
    /// The ids of plugins this one takes the place of (`replaces`): a plugin renamed to a new id
    /// keeps what the player had under the old one (see [`Manifest::replaces`]).
    pub replaces: Vec<PluginId>,
    /// The package is a script written for UtilityBelt's scripting (`ubscript = true`, or a folder
    /// holding `index.lua` and no manifest): its entry runs with UtilityBelt's script API as its
    /// globals, and `index.lua` is its entry when it names none.
    pub ubscript: bool,
}

/// The entry file of a script written for UtilityBelt's scripting.
pub const SCRIPT_ENTRY: &str = "index.lua";

/// The first label of the ids given to scripts that come with no manifest.
pub const SCRIPT_NAMESPACE: &str = "ubscript";

/// Why a manifest was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The document is not a table.
    NotATable,
    /// A key this version of the spec does not define.
    UnknownKey(String),
    /// A required key is absent.
    Missing(&'static str),
    /// A key holds the wrong kind of value.
    WrongType {
        key: String,
        expected: &'static str,
    },
    BadId(String),
    /// The id is in the namespace kept for the host ([`PluginId::is_reserved`]).
    ReservedId(String),
    BadVersion(String),
    BadApi(String),
    UnknownPermission(String),
    UnknownCapability(String),
    /// A context that is not `world`, `pregame` or `chargen`, or an empty list of them.
    BadContext(String),
    /// The entry file is not a relative path inside the package.
    BadEntry(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotATable => f.write_str("the manifest is not a table"),
            Self::UnknownKey(k) => write!(f, "unknown key {k:?}"),
            Self::Missing(k) => write!(f, "missing required key {k:?}"),
            Self::WrongType { key, expected } => write!(f, "{key:?} must be {expected}"),
            Self::BadId(s) => write!(
                f,
                "{s:?} is not a plugin id (lowercase reverse-DNS, e.g. example.xpmeter)"
            ),
            Self::ReservedId(s) => write!(
                f,
                "{s:?} is in the {RESERVED_NAMESPACE}. namespace, which is kept for the client itself"
            ),
            Self::BadVersion(s) => write!(f, "{s:?} is not a SemVer version"),
            Self::BadApi(s) => write!(f, "{s:?} is not an API version (MAJOR.MINOR)"),
            Self::UnknownPermission(s) => write!(f, "unknown permission {s:?}"),
            Self::UnknownCapability(s) => write!(f, "unknown host capability {s:?}"),
            Self::BadContext(s) => write!(
                f,
                "contexts: {s} (each one of \"world\", \"pregame\", \"chargen\"; at least one)"
            ),
            Self::BadEntry(s) => write!(f, "entry {s:?} is not a relative path inside the package"),
        }
    }
}

impl std::error::Error for ManifestError {}

const TOP_KEYS: &[&str] = &[
    "id",
    "name",
    "version",
    "api",
    "entry",
    "description",
    "authors",
    "license",
    "homepage",
    "permissions",
    "capabilities",
    "hosts",
    "net",
    "contexts",
    "replaces",
    "ubscript",
];

fn string(doc: &Value, key: &'static str, required: bool) -> Result<Option<String>, ManifestError> {
    match doc.get(key) {
        Value::Nil if required => Err(ManifestError::Missing(key)),
        Value::Nil => Ok(None),
        Value::Str(s) => Ok(Some(s.clone())),
        _ => Err(ManifestError::WrongType {
            key: key.to_owned(),
            expected: "a string",
        }),
    }
}

fn strings(v: &Value, key: &str) -> Result<Vec<String>, ManifestError> {
    match v {
        Value::Nil => Ok(Vec::new()),
        Value::List(l) => l
            .iter()
            .map(|e| {
                e.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| ManifestError::WrongType {
                        key: key.to_owned(),
                        expected: "a list of strings",
                    })
            })
            .collect(),
        _ => Err(ManifestError::WrongType {
            key: key.to_owned(),
            expected: "a list of strings",
        }),
    }
}

fn only_keys(v: &Value, key: &str, allowed: &[&str]) -> Result<(), ManifestError> {
    match v {
        Value::Nil => Ok(()),
        Value::Map(m) => m
            .keys()
            .find(|k| !allowed.contains(&k.as_str()))
            .map_or(Ok(()), |k| {
                Err(ManifestError::UnknownKey(format!("{key}.{k}")))
            }),
        _ => Err(ManifestError::WrongType {
            key: key.to_owned(),
            expected: "a table",
        }),
    }
}

/// The port an HTTPS request goes to when its URL and the manifest name none.
pub const HTTPS_PORT: u16 = 443;

/// A `[net] domains` entry's host and port: `example.org` is port 443, `example.org:8443` names
/// its own. `None` when it is not a host name (dot-separated labels of letters, digits and
/// hyphens, at least two, none of them a number, so that no address is written as one) or the
/// port is not one.
#[must_use]
pub fn net_domain(d: &str) -> Option<(&str, u16)> {
    let (host, port) = match d.split_once(':') {
        Some((h, p)) => {
            let port = (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse::<u16>().ok())
                .flatten()
                .filter(|p| *p != 0)?;
            (h, port)
        }
        None => (d, HTTPS_PORT),
    };
    let labels: Vec<&str> = host.split('.').collect();
    let number = |l: &str| {
        l.bytes().all(|b| b.is_ascii_digit())
            || l.strip_prefix("0x")
                .or_else(|| l.strip_prefix("0X"))
                .is_some_and(|x| x.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    (labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !number(l)
        }))
    .then_some((host, port))
}

/// Whether `path` is a relative path that stays inside the package: `/`-separated, no empty,
/// `.` or `..` component, no drive or backslash.
#[must_use]
pub fn is_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

impl Manifest {
    /// Apply the manifest rules to a parsed `plugin.toml`.
    ///
    /// # Errors
    /// The first rule the document breaks.
    pub fn from_value(doc: &Value) -> Result<Self, ManifestError> {
        let map = doc.as_map().ok_or(ManifestError::NotATable)?;
        if let Some(k) = map.keys().find(|k| !TOP_KEYS.contains(&k.as_str())) {
            return Err(ManifestError::UnknownKey(k.clone()));
        }
        let id_text = string(doc, "id", true)?.unwrap_or_default();
        let id = PluginId::parse(&id_text).ok_or(ManifestError::BadId(id_text))?;
        if id.is_reserved() {
            return Err(ManifestError::ReservedId(id.to_string()));
        }
        let name = string(doc, "name", true)?.unwrap_or_default();
        let version_text = string(doc, "version", true)?.unwrap_or_default();
        let version =
            PluginVersion::parse(&version_text).ok_or(ManifestError::BadVersion(version_text))?;
        let api_text = string(doc, "api", true)?.unwrap_or_default();
        let api = ApiVersion::parse(&api_text).ok_or(ManifestError::BadApi(api_text))?;
        let ubscript = match doc.get("ubscript") {
            Value::Nil => false,
            Value::Bool(b) => *b,
            _ => {
                return Err(ManifestError::WrongType {
                    key: "ubscript".into(),
                    expected: "true or false",
                })
            }
        };
        let default_entry = if ubscript { SCRIPT_ENTRY } else { "main.lua" };
        let entry = string(doc, "entry", false)?.unwrap_or_else(|| default_entry.to_owned());
        if !is_package_path(&entry) {
            return Err(ManifestError::BadEntry(entry));
        }
        let mut permissions = BTreeMap::new();
        let perms = doc.get("permissions");
        match perms {
            Value::Nil => {}
            Value::Map(m) => {
                for (k, reason) in m {
                    let p = Permission::parse(k)
                        .ok_or_else(|| ManifestError::UnknownPermission(k.clone()))?;
                    let reason = reason.as_str().ok_or_else(|| ManifestError::WrongType {
                        key: format!("permissions.{k}"),
                        expected: "a string (the reason the player is shown)",
                    })?;
                    permissions.insert(p, reason.to_owned());
                }
            }
            _ => {
                return Err(ManifestError::WrongType {
                    key: "permissions".into(),
                    expected: "a table",
                })
            }
        }
        let caps = doc.get("capabilities");
        only_keys(caps, "capabilities", &["required", "optional"])?;
        let cap_list = |key: &str| -> Result<Vec<HostCapability>, ManifestError> {
            strings(caps.get(key), &format!("capabilities.{key}"))?
                .into_iter()
                .map(|s| HostCapability::parse(&s).ok_or(ManifestError::UnknownCapability(s)))
                .collect()
        };
        let required = cap_list("required")?;
        let optional = cap_list("optional")?;
        let hosts = doc.get("hosts");
        only_keys(hosts, "hosts", &["exclude"])?;
        let net = doc.get("net");
        only_keys(net, "net", &["domains"])?;
        let net_domains = strings(net.get("domains"), "net.domains")?
            .into_iter()
            .map(|d| d.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if let Some(bad) = net_domains.iter().find(|d| net_domain(d).is_none()) {
            return Err(ManifestError::WrongType {
                key: format!("net.domains ({bad:?})"),
                expected: "a list of host names, each with its port when that is not 443 \
                           (example.org, example.org:8443), and no addresses",
            });
        }
        let contexts = match doc.get("contexts") {
            Value::Nil => vec![Context::World],
            v => {
                let mut out = Vec::new();
                for s in strings(v, "contexts")? {
                    let c = Context::parse(&s)
                        .ok_or_else(|| ManifestError::BadContext(format!("unknown {s:?}")))?;
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
                if out.is_empty() {
                    return Err(ManifestError::BadContext("the list is empty".into()));
                }
                out
            }
        };
        let mut replaces = Vec::new();
        for s in strings(doc.get("replaces"), "replaces")? {
            let old = PluginId::parse(&s).ok_or_else(|| ManifestError::BadId(s.clone()))?;
            if old == id || old.is_reserved() {
                return Err(ManifestError::BadId(s));
            }
            if !replaces.contains(&old) {
                replaces.push(old);
            }
        }
        Ok(Self {
            id,
            name,
            version,
            api,
            entry,
            contexts,
            replaces,
            description: string(doc, "description", false)?.unwrap_or_default(),
            authors: strings(doc.get("authors"), "authors")?,
            license: string(doc, "license", false)?.unwrap_or_default(),
            homepage: string(doc, "homepage", false)?.unwrap_or_default(),
            permissions,
            required,
            optional,
            exclude_hosts: strings(hosts.get("exclude"), "hosts.exclude")?,
            net_domains,
            ubscript,
        })
    }

    /// The manifest of a script written for UtilityBelt's scripting that came as a folder with no
    /// manifest of its own: named for its folder, its id `ubscript.<the folder's name>` (lowercase,
    /// each run of other characters a `-`), its entry `index.lua`, active in the world, needing no
    /// capability and naming no permission.
    #[must_use]
    pub fn for_script_folder(folder: &str) -> Self {
        let mut label = String::new();
        for c in folder.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || (c.is_ascii_digit() && !label.is_empty()) {
                label.push(c);
            } else if !label.is_empty() && !label.ends_with('-') {
                label.push('-');
            }
        }
        while label.ends_with('-') {
            label.pop();
        }
        if label.is_empty() {
            label.push_str("script");
        }
        label.truncate(100);
        let id = PluginId::parse(&format!("{SCRIPT_NAMESPACE}.{label}"))
            .unwrap_or_else(|| PluginId(format!("{SCRIPT_NAMESPACE}.script")));
        let name = if folder.trim().is_empty() {
            "Script".to_owned()
        } else {
            folder.trim().to_owned()
        };
        Self {
            id,
            name,
            version: PluginVersion {
                major: 0,
                minor: 0,
                patch: 0,
                rest: String::new(),
            },
            api: ApiVersion::CURRENT,
            entry: SCRIPT_ENTRY.to_owned(),
            description: String::new(),
            authors: Vec::new(),
            license: String::new(),
            homepage: String::new(),
            permissions: BTreeMap::new(),
            required: Vec::new(),
            optional: Vec::new(),
            exclude_hosts: Vec::new(),
            net_domains: Vec::new(),
            contexts: vec![Context::World],
            replaces: Vec::new(),
            ubscript: true,
        }
    }

    /// Whether an HTTPS request to `host` at `port` is one the manifest's `[net] domains` admits:
    /// the host is a listed domain or one of its subdomains, at the port listed with it (443
    /// when it names none).
    #[must_use]
    pub fn admits(&self, host: &str, port: u16) -> bool {
        let host = host.to_ascii_lowercase();
        self.net_domains
            .iter()
            .filter_map(|d| net_domain(d))
            .any(|(d, p)| p == port && (host == d || host.ends_with(&format!(".{d}"))))
    }
}

#[cfg(test)]
mod tests {
    use super::{is_package_path, Manifest, ManifestError, PluginId, PluginVersion};
    use crate::permission::Permission;
    use crate::value::Value;

    fn minimal() -> Value {
        Value::map()
            .with("id", "example.xpmeter")
            .with("name", "XP Meter")
            .with("version", "1.2.0")
            .with("api", "0.1")
    }

    #[test]
    fn an_id_in_the_clients_own_namespace_is_refused() {
        for id in ["dereth.plugins", "dereth.tank", "dereth.a.b"] {
            assert_eq!(
                Manifest::from_value(&minimal().with("id", id)),
                Err(ManifestError::ReservedId(id.to_owned())),
                "{id}"
            );
        }
        for id in ["derethx.tank", "network.dereth.tank", "example.dereth"] {
            assert!(
                Manifest::from_value(&minimal().with("id", id)).is_ok(),
                "{id}"
            );
        }
    }

    #[test]
    fn a_minimal_manifest_takes_its_defaults() {
        let m = Manifest::from_value(&minimal()).expect("valid");
        assert_eq!(m.entry, "main.lua");
        assert!(m.permissions.is_empty() && m.required.is_empty());
    }

    #[test]
    fn a_plugin_runs_in_the_world_unless_it_names_its_contexts() {
        use super::Context;
        let m = Manifest::from_value(&minimal()).expect("valid");
        assert_eq!(m.contexts, [Context::World]);
        let doc = minimal().with(
            "contexts",
            vec![Value::from("pregame"), Value::from("chargen")],
        );
        let m = Manifest::from_value(&doc).expect("valid");
        assert_eq!(m.contexts, [Context::Pregame, Context::Chargen]);
        for bad in [
            Value::List(Vec::new()),
            Value::List(vec![Value::from("lobby")]),
            Value::from("world"),
        ] {
            assert!(
                Manifest::from_value(&minimal().with("contexts", bad.clone())).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn requested_permissions_keep_their_reasons() {
        let doc = minimal().with(
            "permissions",
            Value::map().with("chat.filter", "Hides the lines you pick."),
        );
        let m = Manifest::from_value(&doc).expect("valid");
        assert_eq!(
            m.permissions
                .get(&Permission::ChatFilter)
                .map(String::as_str),
            Some("Hides the lines you pick.")
        );
    }

    #[test]
    fn the_net_domains_admit_their_hosts_and_subdomains_only() {
        let doc = minimal().with(
            "net",
            Value::map().with("domains", vec![Value::from("Example.org")]),
        );
        let m = Manifest::from_value(&doc).expect("valid");
        assert!(m.admits("example.org", 443) && m.admits("maps.EXAMPLE.org", 443));
        assert!(!m.admits("badexample.org", 443) && !m.admits("example.com", 443));
        assert!(
            !m.admits("example.org", 8443),
            "443 only, unless a port is listed"
        );
        let doc = minimal().with(
            "net",
            Value::map().with("domains", vec![Value::from("example.org:8443")]),
        );
        let m = Manifest::from_value(&doc).expect("valid");
        assert!(m.admits("example.org", 8443) && !m.admits("example.org", 443));
        for bad in [
            "localhost",
            "exa mple.org",
            "http://example.org",
            ".org",
            "127.0.0.1",
            "10.0.0.1",
            "[::1]",
            "::1",
            "0x7f.1",
            "127.1",
            "123.example.org",
            "example.org:0",
            "example.org:99999",
            "example.org:",
        ] {
            let doc = minimal().with("net", Value::map().with("domains", vec![Value::from(bad)]));
            assert!(Manifest::from_value(&doc).is_err(), "{bad}");
        }
        let doc = minimal().with("net", Value::map().with("hosts", Value::List(Vec::new())));
        assert!(Manifest::from_value(&doc).is_err());
    }

    #[test]
    fn unknown_keys_permissions_and_capabilities_are_refused() {
        let doc = minimal().with("permisions", Value::map());
        assert_eq!(
            Manifest::from_value(&doc),
            Err(ManifestError::UnknownKey("permisions".into()))
        );
        let doc = minimal().with("permissions", Value::map().with("chat.sned", "x"));
        assert_eq!(
            Manifest::from_value(&doc),
            Err(ManifestError::UnknownPermission("chat.sned".into()))
        );
        let doc = minimal().with(
            "capabilities",
            Value::map().with("required", vec![Value::from("ui.windw")]),
        );
        assert_eq!(
            Manifest::from_value(&doc),
            Err(ManifestError::UnknownCapability("ui.windw".into()))
        );
        let doc = minimal().with(
            "capabilities",
            Value::map().with("needed", Value::List(vec![])),
        );
        assert_eq!(
            Manifest::from_value(&doc),
            Err(ManifestError::UnknownKey("capabilities.needed".into()))
        );
    }

    #[test]
    fn ids_versions_and_entries_are_checked() {
        assert!(PluginId::parse("example.xpmeter").is_some());
        assert!(PluginId::parse("a.b-c.d2").is_some());
        for bad in ["xpmeter", "Example.x", "a..b", ".a.b", "a.1b", "a.b_c"] {
            assert!(PluginId::parse(bad).is_none(), "{bad:?}");
        }
        assert!(PluginVersion::parse("1.2.0").is_some());
        assert!(PluginVersion::parse("1.2.0-beta.1+sha").is_some());
        for bad in ["1.2", "01.2.0", "1.2.0.4", "1.2.x", "1.2.0-"] {
            assert!(PluginVersion::parse(bad).is_none(), "{bad:?}");
        }
        for bad in [
            "../x.lua",
            "/abs.lua",
            "a/./b.lua",
            "c:\\x.lua",
            "a//b.lua",
            "",
        ] {
            assert!(!is_package_path(bad), "{bad:?}");
        }
        assert!(is_package_path("lib/util.lua"));
    }

    #[test]
    fn versions_order_by_semver_precedence() {
        let v = |s: &str| PluginVersion::parse(s).expect(s);
        // SemVer 2's own example order, and numbers compared as numbers.
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.2.0",
            "1.10.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            let (lower, higher) = (v(pair[0]), v(pair[1]));
            assert!(higher.is_newer_than(&lower), "{higher} > {lower}");
            assert!(!lower.is_newer_than(&higher), "{lower} < {higher}");
            assert!(lower < higher, "{lower} < {higher}");
        }
        // The build plays no part in precedence, but only equal versions are equal.
        assert!(!v("1.0.0+b").is_newer_than(&v("1.0.0+a")));
        assert_eq!(
            v("1.0.0+a").precedence(&v("1.0.0+b")),
            std::cmp::Ordering::Equal
        );
        assert_ne!(v("1.0.0+a").cmp(&v("1.0.0+b")), std::cmp::Ordering::Equal);
        assert_eq!(
            (v("1.2.3-rc.1+x").pre(), v("1.2.3-rc.1+x").build()),
            ("rc.1", "x")
        );
        assert_eq!((v("1.2.3+x-y").pre(), v("1.2.3+x-y").build()), ("", "x-y"));
    }

    #[test]
    fn a_utilitybelt_script_runs_index_lua_unless_it_names_its_entry() {
        let m = Manifest::from_value(&minimal().with("ubscript", true)).expect("valid");
        assert!(m.ubscript);
        assert_eq!(m.entry, "index.lua");
        let m = Manifest::from_value(&minimal().with("ubscript", true).with("entry", "a.lua"))
            .expect("valid");
        assert_eq!(m.entry, "a.lua");
        assert!(!Manifest::from_value(&minimal()).expect("valid").ubscript);
        assert!(Manifest::from_value(&minimal().with("ubscript", "yes")).is_err());
    }

    #[test]
    fn a_script_folder_is_named_for_its_folder() {
        let m = Manifest::for_script_folder("RarePointer");
        assert_eq!(m.id.as_str(), "ubscript.rarepointer");
        assert_eq!(
            (m.name.as_str(), m.entry.as_str()),
            ("RarePointer", "index.lua")
        );
        assert!(m.ubscript && m.permissions.is_empty());
        assert_eq!(
            Manifest::for_script_folder("2 Quest_Helper!").id.as_str(),
            "ubscript.quest-helper"
        );
        assert_eq!(
            Manifest::for_script_folder("123").id.as_str(),
            "ubscript.script"
        );
    }

    #[test]
    fn missing_required_keys_are_named() {
        let doc = Value::map().with("id", "example.x");
        assert_eq!(
            Manifest::from_value(&doc),
            Err(ManifestError::Missing("name"))
        );
    }
}
