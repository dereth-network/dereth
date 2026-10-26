//! The preflight's setup file: where the dev hub runs and the real hub it copies is, the machines
//! the dev worlds run on, and the worlds. It is the owner's, kept beside the other Dereth settings
//! (`%APPDATA%\Dereth\preflight.toml` on Windows, `~/.config/dereth/preflight.toml` elsewhere, or
//! the file `DERETH_PREFLIGHT` names), never in the repository: it names machines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The environment variable that names the setup file in place of the usual one.
pub const CONFIG_ENV: &str = "DERETH_PREFLIGHT";

/// The setup file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Where the preflight keeps its builds, its copy of the registry, the worlds' backups and the
    /// record of what it changed.
    pub work_dir: PathBuf,
    /// The git remote `preflight` and `ship` push to and `ship` reads the preflighted branch
    /// from. The owner's mirror carries it to GitHub.
    #[serde(default = "origin")]
    pub remote: String,
    /// The public repository on GitHub, `<owner>/<name>`, whose CI `ship` waits for. Nothing here
    /// pushes to it: the mirror carries `remote` there.
    #[serde(default = "github_repo")]
    pub github_repo: String,
    /// The ACE server folder `ship`'s release of the mod checks against; by default the first ACE
    /// world's (for a world on another machine, the copy of its assemblies a preflight fetched).
    #[serde(default)]
    pub ace_dir: Option<PathBuf>,
    /// The public key the Windows world machines authorize for the preflight's ssh; by default the
    /// first of `id_ed25519.pub`, `id_ecdsa.pub` and `id_rsa.pub` in `~/.ssh`.
    #[serde(default)]
    pub ssh_key: Option<PathBuf>,
    pub hub: Hub,
    /// The machines dev worlds run on, other than this one.
    #[serde(default, rename = "machine")]
    pub machines: Vec<Host>,
    #[serde(default, rename = "world")]
    pub worlds: Vec<World>,
}

fn origin() -> String {
    "origin".to_owned()
}

fn github_repo() -> String {
    "dereth-network/dereth".to_owned()
}

/// The real hub the registry is copied from, and the dev hub run on the copy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hub {
    /// The real hub's machine, as `ssh` reaches it (`owner@vps.example.org`, or a `Host` of the
    /// ssh configuration).
    pub ssh: String,
    /// The hub program there.
    #[serde(default = "hub_program")]
    pub remote_binary: String,
    /// Its registry there.
    pub remote_db: String,
    /// Where the backup is written there before it is copied down (and removed).
    #[serde(default = "remote_tmp")]
    pub remote_tmp: String,
    /// Where the dev hub listens, on this machine.
    #[serde(default = "listen")]
    pub listen: String,
    /// The dev hub's own address, through the tunnel (`https://dev-hub.example.org`): the worlds
    /// link to it there, wherever they run, and Discord reaches it there.
    pub public_url: String,
    /// The dev site's origins, if any.
    #[serde(default)]
    pub site_origins: Vec<String>,
    /// The Discord ids of the dev hub's admins. Their persons' links are the only ones the copy
    /// keeps, so tells reach their Discord alone.
    #[serde(default)]
    pub admins: Vec<String>,
    /// The dev Discord application (never the real one): its client id, and the files that hold
    /// its client secret and its bot's token.
    #[serde(default)]
    pub discord_client_id: Option<String>,
    #[serde(default)]
    pub discord_client_secret_file: Option<PathBuf>,
    #[serde(default)]
    pub discord_bot_token_file: Option<PathBuf>,
    /// The Cloudflare tunnel that carries the dev hub's address here; none runs without it.
    #[serde(default)]
    pub tunnel: Option<String>,
    /// The `cloudflared` program.
    #[serde(default = "cloudflared")]
    pub cloudflared: String,
    /// More arguments for the dev hub's `serve` (`--trusted-proxy 127.0.0.1`, ...).
    #[serde(default)]
    pub serve_args: Vec<String>,
}

fn hub_program() -> String {
    "dereth-network-hub".to_owned()
}

fn remote_tmp() -> String {
    "/tmp/dereth-preflight-hub.sqlite".to_owned()
}

fn listen() -> String {
    "127.0.0.1:8650".to_owned()
}

fn cloudflared() -> String {
    "cloudflared".to_owned()
}

/// What a machine runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Windows,
    Linux,
}

/// A machine dev worlds run on, other than this one, reached over ssh with a key. Nothing is built
/// there: the builds are made here and copied over.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    /// Its name here, for a world's `machine` and its setup script's file.
    pub name: String,
    /// As `ssh` reaches it (`you@world-box`, or a `Host` of the ssh configuration).
    pub ssh: String,
    pub os: Os,
    /// For a Linux machine: the target Empyrean's server is built for here, with `cargo
    /// zigbuild`. By default `x86_64-unknown-linux-gnu`; a glibc floor is given as zigbuild spells
    /// it (`x86_64-unknown-linux-gnu.2.28`).
    #[serde(default)]
    pub target: Option<String>,
}

impl Host {
    /// A Linux machine's target when the setup file gives none.
    pub const DEFAULT_TARGET: &str = "x86_64-unknown-linux-gnu";

    /// The target Empyrean's server is built for, for a Linux machine.
    pub fn target(&self) -> &str {
        self.target.as_deref().unwrap_or(Self::DEFAULT_TARGET)
    }

    /// The account its ssh logs in as, when the address names one (`you@world-box`).
    pub fn account(&self) -> Option<&str> {
        self.ssh.rsplit_once('@').map(|(user, _)| user)
    }
}

/// What a dev world runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Empyrean: `empyrean-server` and its `empyrean.toml` in the world's folder.
    Empyrean,
    /// ACE with the Dereth.Network mod: the mod's folder under the server's `Mods`.
    Ace,
}

/// A dev world.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    /// Its name here, for `--with` and the summary.
    pub name: String,
    pub kind: Kind,
    /// Its service: a Windows service (`sc` starts and stops it), or a systemd unit on a Linux
    /// machine.
    pub service: String,
    /// Its folder, as its machine spells it: Empyrean's (with `empyrean-server` and
    /// `empyrean.toml`), or ACE's `Server` folder (whose `Mods` holds the mod).
    pub dir: String,
    /// An ACE world's mods folder (ACE's `ModsDirectory`), when it is not `Mods` inside `dir`:
    /// the mod is its `Dereth.Network` folder.
    #[serde(default)]
    pub mods_dir: Option<String>,
    /// The machine it runs on, by its `[[machine]]` name; none: this one.
    #[serde(default)]
    pub machine: Option<String>,
    /// Only with `--with <name>`.
    #[serde(default)]
    pub optional: bool,
    /// For the one-time setup of an ACE world's service: the `dotnet` that runs `ACE.Server.dll`,
    /// the service it waits for (its MariaDB), and the file holding its registry key, which the
    /// service is given in its environment.
    #[serde(default)]
    pub dotnet: Option<String>,
    #[serde(default)]
    pub depends_on: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    /// For the one-time setup of the service an ACE world waits for, when MariaDB is not one yet:
    /// its folder (holding `bin\mariadbd.exe`) and the defaults file it runs with.
    #[serde(default)]
    pub mariadb_dir: Option<String>,
    #[serde(default)]
    pub mariadb_ini: Option<String>,
    /// The machine `machine` names, filled in when the setup file is read.
    #[serde(skip)]
    pub host: Option<Host>,
}

/// Where a world runs, as the preflight reaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    /// This machine: its service started and stopped here, its files copied here.
    Here,
    /// Another Windows machine: PowerShell there over ssh, files copied with scp.
    Windows(&'a Host),
    /// A Linux machine: systemd there over ssh, files copied with scp.
    Linux(&'a Host),
}

impl World {
    /// Where it runs.
    pub fn place(&self) -> Place<'_> {
        match &self.host {
            None => Place::Here,
            Some(h) if h.os == Os::Windows => Place::Windows(h),
            Some(h) => Place::Linux(h),
        }
    }

    /// Whether its machine spells paths with `/`.
    fn linux(&self) -> bool {
        matches!(self.place(), Place::Linux(_))
    }

    /// Its configuration file, where its hub address is.
    pub fn config_file(&self) -> String {
        match self.kind {
            Kind::Empyrean => join(&self.dir, "empyrean.toml", self.linux()),
            Kind::Ace => join(&self.mod_dir(), "Settings.json", self.linux()),
        }
    }

    /// The mod's folder of an ACE world.
    pub fn mod_dir(&self) -> String {
        let mods = self
            .mods_dir
            .clone()
            .unwrap_or_else(|| join(&self.dir, "Mods", self.linux()));
        join(&mods, "Dereth.Network", self.linux())
    }

    /// What a build replaces in it: the server, or the mod's folder.
    pub fn replaced(&self) -> String {
        match self.kind {
            Kind::Empyrean if self.linux() => join(&self.dir, "empyrean-server", true),
            Kind::Empyrean => join(&self.dir, "empyrean-server.exe", false),
            Kind::Ace => self.mod_dir(),
        }
    }
}

/// `dir` and `name` joined as the world's machine spells paths: with `/` on Linux, `\` on Windows.
fn join(dir: &str, name: &str, linux: bool) -> String {
    let sep = if linux { '/' } else { '\\' };
    format!("{}{sep}{name}", dir.trim_end_matches(['/', '\\']))
}

/// A name the preflight puts in paths and file names: letters, digits, `-`, `_` and `.`, not
/// starting with a dot.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The usual place of the setup file, with `get` reading the environment.
pub fn default_path(get: &dyn Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    if let Some(p) = get(CONFIG_ENV).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if windows {
        return get("APPDATA").map(|a| Path::new(&a).join("Dereth").join("preflight.toml"));
    }
    get("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| get("HOME").map(|h| Path::new(&h).join(".config")))
        .map(|c| c.join("dereth").join("preflight.toml"))
}

/// What an older setup file says that this one does not, and what to write instead.
fn stale(text: &str) -> Vec<String> {
    let Ok(raw) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for w in raw
        .get("world")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
    {
        let name = w
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or("a world");
        if w.contains_key("build") || w.contains_key("binary") {
            out.push(format!(
                "{name}: `build` and `binary` are gone, since nothing is built on a world's \
                 machine: Empyrean is built here with cargo zigbuild for its Linux machine's \
                 `target` and copied over. Take them out"
            ));
        }
        if w.contains_key("ssh") {
            out.push(format!(
                "{name}: a world names its machine, `machine = \"<name>\"`, and a [[machine]] \
                 gives that machine's `name`, `ssh` and `os` (\"windows\" or \"linux\")"
            ));
        }
    }
    out
}

impl Config {
    /// The setup file's text, read and checked.
    pub fn parse(text: &str) -> Result<Self, String> {
        let old = stale(text);
        if !old.is_empty() {
            return Err(format!("the setup file:\n  {}", old.join("\n  ")));
        }
        let mut c: Self = toml::from_str(text).map_err(|e| format!("the setup file: {e}"))?;
        c.check()?;
        Ok(c)
    }

    /// Every reason the setup cannot be used, together; each world given its machine.
    fn check(&mut self) -> Result<(), String> {
        let mut problems = Vec::new();
        let mut hosts = BTreeSet::new();
        for h in &self.machines {
            if !plain(&h.name) {
                problems.push(format!(
                    "the machine `{}`: a name of letters, digits, `-`, `_` and `.`",
                    h.name
                ));
            }
            if !hosts.insert(h.name.as_str()) {
                problems.push(format!("two machines are named {}", h.name));
            }
            if h.os == Os::Windows && h.target.is_some() {
                problems.push(format!(
                    "{} runs Windows: its builds are this machine's own, so it takes no `target`",
                    h.name
                ));
            }
        }
        let mut names = BTreeSet::new();
        for w in &mut self.worlds {
            if !plain(&w.name) {
                problems.push(format!(
                    "the world `{}`: a name of letters, digits, `-`, `_` and `.`",
                    w.name
                ));
            }
            if !names.insert(w.name.clone()) {
                problems.push(format!("two worlds are named {}", w.name));
            }
            w.host = None;
            if let Some(m) = &w.machine {
                match self.machines.iter().find(|h| &h.name == m) {
                    Some(h) => w.host = Some(h.clone()),
                    None => problems.push(format!(
                        "{} is on the machine {m}, which no [[machine]] names",
                        w.name
                    )),
                }
            }
            if w.kind == Kind::Ace && matches!(w.place(), Place::Linux(_)) {
                problems.push(format!(
                    "{} is an ACE world on a Linux machine, which the preflight does not install",
                    w.name
                ));
            }
        }
        if self.hub.discord_client_id.is_some() != self.hub.discord_client_secret_file.is_some() {
            problems.push(
                "the dev Discord application needs both `discord_client_id` and \
                 `discord_client_secret_file`"
                    .to_owned(),
            );
        }
        if !super::ci::repo_name(&self.github_repo) {
            problems.push(format!(
                "`github_repo` is {}: a GitHub repository's name, `<owner>/<name>`",
                self.github_repo
            ));
        }
        if !self.hub.public_url.starts_with("https://")
            && !self.hub.public_url.starts_with("http://127.0.0.1")
        {
            problems.push(format!(
                "the dev hub's `public_url` is {}: an https address the tunnel carries",
                self.hub.public_url
            ));
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(format!("the setup file:\n  {}", problems.join("\n  ")))
        }
    }

    /// The worlds a run takes: every world that is not optional, and the optional ones `with`
    /// names. Every name in `with` must be a world's.
    pub fn worlds_for(&self, with: &[String]) -> Result<Vec<&World>, String> {
        for n in with {
            if !self.worlds.iter().any(|w| &w.name == n) {
                return Err(format!(
                    "--with {n}: no world of that name in the setup file"
                ));
            }
        }
        Ok(self
            .worlds
            .iter()
            .filter(|w| !w.optional || with.contains(&w.name))
            .collect())
    }

    /// Where the copy of the assemblies of `w`, an ACE world on another machine, is kept here: its
    /// mod is built against them.
    pub fn fetched_ace(&self, w: &World) -> PathBuf {
        self.work_dir.join("ace").join(&w.name)
    }

    /// The ACE server folder `ship`'s release of the mod checks against: `ace_dir`, or the first
    /// ACE world's on this machine, or the copy of the first other one's.
    pub fn release_ace_dir(&self) -> Option<PathBuf> {
        if let Some(d) = &self.ace_dir {
            return Some(d.clone());
        }
        let ace = || self.worlds.iter().filter(|w| w.kind == Kind::Ace);
        ace()
            .find(|w| w.place() == Place::Here)
            .map(|w| PathBuf::from(&w.dir))
            .or_else(|| ace().next().map(|w| self.fetched_ace(w)))
    }

    /// The address the worlds link to the dev hub at: its public address, as a link.
    pub fn dev_hub_link(&self) -> String {
        let url = self.hub.public_url.trim_end_matches('/');
        if let Some(rest) = url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            url.to_owned()
        }
    }
}
