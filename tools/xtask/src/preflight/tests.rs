//! Behaviour: none (tooling: the preflight and ship commands, over a fake setup).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::config::{Config, Kind, Os, Place};
use super::machine::{self, Machine, Service};
use super::plan::{self, Bump, Product};
use super::steps::{self, Ledger, Stage};
use super::{edit, run, setup, ship, Options, Repo};

/// A machine kept in memory: its files (by path, `/` between names), its services, the files on
/// the machines it reaches over ssh, and a record of everything done.
#[derive(Debug, Default)]
struct Fake {
    files: BTreeMap<String, String>,
    remote: BTreeMap<String, String>,
    services: BTreeMap<String, Service>,
    /// What a command answers, by the start of its line (`program args...`, `ssh host command`,
    /// with a PowerShell script as its text, or `service name state`).
    replies: Vec<(String, Result<String, String>)>,
    log: Vec<String>,
    /// Each command sent over ssh as it was sent, by host.
    sent: Vec<(String, String)>,
    /// Each PowerShell script sent over ssh, as its text, by host.
    scripts: Vec<(String, String)>,
    pid: u32,
    /// The repository's files, by path from its root: what `git worktree add` checks out.
    tree: BTreeMap<String, String>,
}

fn key(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

impl Fake {
    fn file(mut self, path: &str, text: &str) -> Self {
        self.files.insert(path.replace('\\', "/"), text.to_owned());
        self
    }

    fn tracked(mut self, path: &str, text: &str) -> Self {
        self.tree.insert(path.to_owned(), text.to_owned());
        self
    }

    fn there(mut self, at: &str, text: &str) -> Self {
        self.remote.insert(at.to_owned(), text.to_owned());
        self
    }

    /// `start`'s answer, which wins over any given before.
    fn reply(mut self, start: &str, answer: Result<&str, &str>) -> Self {
        self.replies.insert(
            0,
            (
                start.to_owned(),
                answer.map(str::to_owned).map_err(str::to_owned),
            ),
        );
        self
    }

    fn answer(&self, line: &str) -> Result<String, String> {
        self.replies
            .iter()
            .find(|(start, _)| line.starts_with(start.as_str()))
            .map_or(Ok(String::new()), |(_, a)| a.clone())
    }

    fn get(&self, path: &str) -> Option<&str> {
        self.files.get(&path.replace('\\', "/")).map(String::as_str)
    }

    fn at_remote(&self, at: &str) -> Option<&str> {
        self.remote.get(at).map(String::as_str)
    }

    /// The record, each path spelled with `/` whatever this system spells it with.
    fn record(&self) -> Vec<String> {
        self.log.iter().map(|l| l.replace('\\', "/")).collect()
    }

    /// The record's lines that start with `start`.
    fn done(&self, start: &str) -> Vec<String> {
        self.record()
            .into_iter()
            .filter(|l| l.starts_with(start))
            .collect()
    }

    /// Where in the record the first line starting with `start` is.
    fn at(&self, start: &str) -> usize {
        self.record()
            .iter()
            .position(|l| l.starts_with(start))
            .unwrap_or_else(|| panic!("never done: {start}\n{:#?}", self.record()))
    }

    /// Where in the record the last line starting with `start` is.
    fn last(&self, start: &str) -> usize {
        self.record()
            .iter()
            .rposition(|l| l.starts_with(start))
            .unwrap_or_else(|| panic!("never done: {start}\n{:#?}", self.record()))
    }

    /// One line of a shell script sent to `host`, done to the files there: the few commands the
    /// preflight's Linux scripts use, each path quoted.
    fn shell(&mut self, host: &str, line: &str) -> Result<(), String> {
        let test = regex::Regex::new(r"^if \[ -e '([^']*)' \]; then (.*?); (?:else (.*?); )?fi$")
            .expect("a pattern");
        if let Some(c) = test.captures(line) {
            let there = self.on(host, &c[1]);
            return match (there, c.get(3)) {
                (true, _) => self.shell(host, &c[2]),
                (false, Some(otherwise)) => self.shell(host, otherwise.as_str()),
                (false, None) => Ok(()),
            };
        }
        let words: Vec<&str> = line
            .split('\'')
            .enumerate()
            .flat_map(|(i, part)| {
                if i % 2 == 1 {
                    vec![part]
                } else {
                    part.split_whitespace().collect()
                }
            })
            .collect();
        let at = |p: &str| format!("{host}:{p}");
        match words.as_slice() {
            ["rm", "-rf" | "-f", p] => {
                let p = at(p);
                self.remote
                    .retain(|k, _| *k != p && !k.starts_with(&format!("{p}/")));
                Ok(())
            }
            ["mkdir", "-p", _] => Ok(()),
            ["cp", "-a", from, to] | ["cp", from, to] => {
                let (from, to) = (at(from), at(to));
                let copied: Vec<(String, String)> = self
                    .remote
                    .iter()
                    .filter_map(|(k, t)| {
                        if *k == from {
                            Some((to.clone(), t.clone()))
                        } else {
                            k.strip_prefix(&format!("{from}/"))
                                .map(|rest| (format!("{to}/{rest}"), t.clone()))
                        }
                    })
                    .collect();
                if copied.is_empty() {
                    return Err(format!("cp: cannot stat {from}"));
                }
                self.remote.extend(copied);
                Ok(())
            }
            other => panic!("a command the fake does not know: {other:?}"),
        }
    }

    /// Whether `path` is on `host`.
    fn on(&self, host: &str, path: &str) -> bool {
        let p = format!("{host}:{path}");
        self.remote
            .keys()
            .any(|k| *k == p || k.starts_with(&format!("{p}/")))
    }

    /// The PowerShell scripts sent to `host`, in order.
    fn scripts_on(&self, host: &str) -> Vec<&str> {
        self.scripts
            .iter()
            .filter(|(h, _)| h == host)
            .map(|(_, s)| s.as_str())
            .collect()
    }
}

impl Machine for Fake {
    fn run(
        &mut self,
        _dir: Option<&Path>,
        program: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<String, String> {
        let line = format!("{program} {}", args.join(" "));
        if program == "git" && args.len() >= 4 && args[0] == "worktree" && args[1] == "add" {
            let at = key(Path::new(&args[3]));
            let checked_out: Vec<(String, String)> = self
                .tree
                .iter()
                .map(|(p, t)| (format!("{at}/{p}"), t.clone()))
                .collect();
            self.files.extend(checked_out);
        }
        if program == "dotnet" {
            // The mod's build leaves its assembly, which says what ACE it was built against.
            let ace = args
                .iter()
                .find_map(|a| a.strip_prefix("-p:AceDirectory="))
                .map(|a| key(Path::new(a)))
                .unwrap_or_default();
            if let Some(i) = args.iter().position(|a| a == "-o") {
                let out = key(Path::new(&args[i + 1]));
                self.files.insert(
                    format!("{out}/Dereth.Network.dll"),
                    format!("mod built against {ace}"),
                );
            }
        }
        let env: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        self.log.push(if env.is_empty() {
            line.clone()
        } else {
            format!("{line} [{}]", env.join(" "))
        });
        self.answer(&line)
    }

    fn probe(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        let line = format!("{program} {}", args.join(" "));
        self.log.push(format!("probe {line}"));
        self.answer(&line)
    }

    fn ssh(&mut self, host: &str, command: &str) -> Result<String, String> {
        self.sent.push((host.to_owned(), command.to_owned()));
        let shown = match machine::script_of(command) {
            Some(script) => {
                self.scripts.push((host.to_owned(), script.clone()));
                script
            }
            None => command.to_owned(),
        };
        let line = format!("ssh {host} {shown}");
        self.log.push(line.clone());
        let answer = self.answer(&line)?;
        if let Some(script) = command.strip_prefix("set -e\n") {
            for l in script.lines() {
                self.shell(host, l)?;
            }
        }
        Ok(answer)
    }

    fn fetch(&mut self, host: &str, remote: &str, local: &Path) -> Result<(), String> {
        self.log
            .push(format!("fetch {host}:{remote} {}", key(local)));
        let from = format!("{host}:{remote}");
        let to = key(local);
        let copied: Vec<(String, String)> = self
            .remote
            .iter()
            .filter_map(|(p, t)| {
                if *p == from {
                    Some((to.clone(), t.clone()))
                } else {
                    p.strip_prefix(&format!("{from}/"))
                        .map(|rest| (format!("{to}/{rest}"), t.clone()))
                }
            })
            .collect();
        if copied.is_empty() {
            return Err(format!("no {remote} on {host}"));
        }
        self.files.extend(copied);
        Ok(())
    }

    fn send(&mut self, host: &str, local: &Path, remote: &str) -> Result<(), String> {
        let from = key(local);
        self.log.push(format!("send {from} {host}:{remote}"));
        let to = format!("{host}:{remote}");
        let copied: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(p, t)| {
                if *p == from {
                    Some((to.clone(), t.clone()))
                } else {
                    p.strip_prefix(&format!("{from}/"))
                        .map(|rest| (format!("{to}/{rest}"), t.clone()))
                }
            })
            .collect();
        if copied.is_empty() {
            return Err(format!("nothing to send at {from}"));
        }
        self.remote.extend(copied);
        Ok(())
    }

    fn spawn(
        &mut self,
        program: &str,
        args: &[String],
        _env: &[(String, String)],
        _log: &Path,
    ) -> Result<u32, String> {
        self.pid += 1;
        self.log.push(format!("spawn {program} {}", args.join(" ")));
        Ok(100 + self.pid)
    }

    fn kill(&mut self, pid: u32) -> Result<(), String> {
        self.log.push(format!("kill {pid}"));
        Ok(())
    }

    fn read(&mut self, path: &Path) -> Result<String, String> {
        self.files
            .get(&key(path))
            .cloned()
            .ok_or_else(|| format!("no file {}", key(path)))
    }

    fn write(&mut self, path: &Path, text: &str) -> Result<(), String> {
        self.log.push(format!("write {}", key(path)));
        self.files.insert(key(path), text.to_owned());
        Ok(())
    }

    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        let (from, to) = (key(from), key(to));
        self.log.push(format!("copy {from} {to}"));
        let copied: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(p, t)| {
                if *p == from {
                    Some((to.clone(), t.clone()))
                } else {
                    p.strip_prefix(&format!("{from}/"))
                        .map(|rest| (format!("{to}/{rest}"), t.clone()))
                }
            })
            .collect();
        if copied.is_empty() {
            return Err(format!("nothing at {from}"));
        }
        self.files.extend(copied);
        Ok(())
    }

    fn exists(&mut self, path: &Path) -> bool {
        let p = key(path);
        self.files
            .keys()
            .any(|k| *k == p || k.starts_with(&format!("{p}/")))
    }

    fn remove(&mut self, path: &Path) -> Result<(), String> {
        let p = key(path);
        self.files
            .retain(|k, _| *k != p && !k.starts_with(&format!("{p}/")));
        Ok(())
    }

    fn service(&mut self, name: &str, to: Service) -> Result<(), String> {
        self.log.push(format!("service {name} {to:?}"));
        if let Some((_, Err(e))) = self
            .replies
            .iter()
            .find(|(start, _)| *start == format!("service {name} {to:?}"))
        {
            return Err(e.clone());
        }
        self.services.insert(name.to_owned(), to);
        Ok(())
    }
}

const SETUP: &str = r#"
work_dir = 'D:\preflight'

[hub]
ssh = "owner@vps.example.org"
remote_db = "/srv/hub/hub.sqlite"
public_url = "https://dev-hub.example.org"
admins = ["1001"]
discord_client_id = "555"
discord_client_secret_file = 'D:\secrets\dev-client-secret.txt'
discord_bot_token_file = 'D:\secrets\dev-bot-token.txt'
tunnel = "dev-hub"

[[machine]]
name = "vm"
ssh = "owner@vm.example.org"
os = "linux"

[[world]]
name = "emp"
kind = "empyrean"
service = "EmpyreanDev"
dir = 'D:\worlds\emp'

[[world]]
name = "ace"
kind = "ace"
service = "AceDev"
dir = 'D:\ACE\Server'
depends_on = "MariaDB"
key_file = 'D:\secrets\ace.key'

[[world]]
name = "vm"
kind = "empyrean"
machine = "vm"
service = "empyrean"
dir = "/srv/emp"
optional = true
"#;

/// Nothing on this machine: a Windows machine with both kinds of world, in folders with spaces
/// and a quote in their names, and a Linux machine whose glibc is older.
const REMOTE: &str = r#"
work_dir = 'D:\preflight'

[hub]
ssh = "owner@vps.example.org"
remote_db = "/srv/hub/hub.sqlite"
public_url = "https://dev-hub.example.org"
admins = ["1001"]

[[machine]]
name = "world-box"
ssh = "owner@world-box"
os = "windows"

[[machine]]
name = "world-vm"
ssh = "owner@world-vm"
os = "linux"
target = "x86_64-unknown-linux-gnu.2.28"

[[world]]
name = "box-emp"
kind = "empyrean"
machine = "world-box"
service = "EmpyreanDev"
dir = "C:\\Worlds\\Bob's Empyrean Dev"

[[world]]
name = "box-ace"
kind = "ace"
machine = "world-box"
service = "ACEDev"
dir = 'D:\Games\ACE Server\Server'
depends_on = "MariaDB"
mariadb_dir = 'D:\Games\ACE Server\mariadb'
mariadb_ini = 'D:\Games\ACE Server\my.ini'
key_file = 'D:\keys\ace.key'

[[world]]
name = "infiltration"
kind = "empyrean"
machine = "world-vm"
service = "empyrean"
dir = "/srv/empyrean"
"#;

/// The local ACE world added to [`REMOTE`].
const LOCAL_ACE: &str = r#"
[[world]]
name = "ace"
kind = "ace"
service = "AceDev"
dir = 'D:\ACE\Server'
"#;

fn cfg() -> Config {
    Config::parse(SETUP).expect("the example parses")
}

fn remote_cfg() -> Config {
    Config::parse(REMOTE).expect("the remote example parses")
}

const EMP_TOML: &str = "[server]\nworld_name = \"Dev\"\n\n[federation]\nenabled = true\nhub_url = \"wss://hub.dereth.network\"\nslug = \"dev\"\n\n[database]\nshard_db_path = \"./shard.db\"\n";
const ACE_SETTINGS: &str = "{\n  \"Slug\": \"acedev\"\n}\n";

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The dev setup and the worktree's files as a run finds them.
fn fake() -> Fake {
    Fake::default()
        .tracked(
            "Cargo.toml",
            "[workspace]\nmembers = [\"core/primitives\", \"empyrean/server\", \"empyrean/crates/world\", \"network/hub\"]\n",
        )
        .tracked("empyrean/server/Cargo.toml", "[package]\nname = \"empyrean-server\"\nversion = \"0.3.1\"\n")
        .tracked("empyrean/crates/world/Cargo.toml", "[package]\nname = \"empyrean-world\"\nversion = \"0.3.1\"\n")
        .tracked("network/hub/Cargo.toml", "[package]\nname = \"dereth-network-hub\"\nversion = \"0.2.1\"\n")
        .tracked("network/ace-mod/Dereth.Network/Meta.json", "{\n  \"Version\": \"0.1.2\"\n}\n")
        .tracked(
            "network/ace-mod/Dereth.Network/Dereth.Network.csproj",
            "<Project>\n  <PropertyGroup>\n    <Version>0.1.2</Version>\n  </PropertyGroup>\n</Project>\n",
        )
        .reply(
            "rustup target list --installed",
            Ok("x86_64-pc-windows-msvc\nx86_64-unknown-linux-gnu\n"),
        )
        .reply("dotnet --list-sdks", Ok("8.0.414 [C:\\Program Files\\dotnet\\sdk]\n10.0.401 [C:\\Program Files\\dotnet\\sdk]\n"))
        .file(&format!("D:/preflight/target/release/{}", exe("dereth-network-hub")), "hub 0.2.2-rc.1")
        .file(&format!("D:/preflight/target/release/{}", exe("empyrean-server")), "empyrean 0.3.2-rc.1")
        .file(
            "D:/preflight/target/x86_64-unknown-linux-gnu/release/empyrean-server",
            "empyrean 0.3.2-rc.1 for linux",
        )
        .file("D:/worlds/emp/empyrean-server.exe", "empyrean 0.3.1")
        .file("D:/worlds/emp/empyrean.toml", EMP_TOML)
        .file("D:/worlds/emp/shard.db", "the shard")
        .file("D:/ACE/Server/ACE.Server.dll", "ACE.Server")
        .file("D:/ACE/Server/Mods/Dereth.Network/Dereth.Network.dll", "mod 0.1.2")
        .file("D:/ACE/Server/Mods/Dereth.Network/Settings.json", ACE_SETTINGS)
        .there(
            "owner@vm.example.org:/srv/emp/empyrean-server",
            "empyrean 0.3.1 for linux",
        )
        .there("owner@vm.example.org:/srv/emp/empyrean.toml", EMP_TOML)
}

/// [`fake`], with the other machines' files: each Windows world's configuration as its staging
/// script leaves it to be copied down, the ACE server's assemblies there, and the Linux world's
/// configuration.
fn remote_fake() -> Fake {
    fake()
        .there("owner@world-box:.dereth-preflight/box-emp/config", EMP_TOML)
        .there(
            "owner@world-box:.dereth-preflight/box-ace/config",
            ACE_SETTINGS,
        )
        .there(
            "owner@world-box:.dereth-preflight/box-ace/ace/ACE.Server.dll",
            "ACE.Server",
        )
        .there("owner@world-vm:/srv/empyrean/empyrean.toml", EMP_TOML)
        .there(
            "owner@world-vm:/srv/empyrean/empyrean-server",
            "empyrean 0.3.1 for linux",
        )
}

fn repo(products: &[Product]) -> Repo {
    Repo {
        branch: "release".to_owned(),
        commit: "c0ffee0000000000000000000000000000000001".to_owned(),
        dirty: Vec::new(),
        products: products.iter().copied().collect(),
        versions: [
            (Product::Hub, "0.2.1"),
            (Product::Ace, "0.1.2"),
            (Product::Empyrean, "0.3.1"),
            (Product::Dereth, "0.4.3"),
        ]
        .into_iter()
        .map(|(p, v)| (p, v.to_owned()))
        .collect(),
        bumps: [(
            Product::Hub,
            (Bump::Patch, "the registry's schema is unchanged".to_owned()),
        )]
        .into_iter()
        .collect(),
    }
}

/// A fake whose candidate hub answers its two counts in turn.
struct Counting {
    inner: Fake,
    counts: Vec<String>,
}

impl Machine for Counting {
    fn run(
        &mut self,
        dir: Option<&Path>,
        program: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<String, String> {
        let out = self.inner.run(dir, program, args, env)?;
        if args.last().is_some_and(|a| a == "table-counts") && !self.counts.is_empty() {
            return Ok(self.counts.remove(0));
        }
        if args.iter().any(|a| a == "defang-copy") {
            return Ok("dev.sqlite: 1 Discord mirror(s) taken out".to_owned());
        }
        Ok(out)
    }
    fn probe(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        self.inner.probe(program, args)
    }
    fn ssh(&mut self, host: &str, command: &str) -> Result<String, String> {
        self.inner.ssh(host, command)
    }
    fn fetch(&mut self, host: &str, remote: &str, local: &Path) -> Result<(), String> {
        self.inner.fetch(host, remote, local)
    }
    fn send(&mut self, host: &str, local: &Path, remote: &str) -> Result<(), String> {
        self.inner.send(host, local, remote)
    }
    fn spawn(
        &mut self,
        program: &str,
        args: &[String],
        env: &[(String, String)],
        log: &Path,
    ) -> Result<u32, String> {
        self.inner.spawn(program, args, env, log)
    }
    fn kill(&mut self, pid: u32) -> Result<(), String> {
        self.inner.kill(pid)
    }
    fn read(&mut self, path: &Path) -> Result<String, String> {
        self.inner.read(path)
    }
    fn write(&mut self, path: &Path, text: &str) -> Result<(), String> {
        self.inner.write(path, text)
    }
    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        self.inner.copy(from, to)
    }
    fn exists(&mut self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn remove(&mut self, path: &Path) -> Result<(), String> {
        self.inner.remove(path)
    }
    fn service(&mut self, name: &str, to: Service) -> Result<(), String> {
        self.inner.service(name, to)
    }
}

/// The fake with the real hub's backup there to copy down, and the candidate's two counts: before
/// it migrates the copy, and after.
fn counting(mut f: Fake) -> Counting {
    f.remote.insert(
        "owner@vps.example.org:/tmp/dereth-preflight-hub.sqlite".to_owned(),
        "the registry".to_owned(),
    );
    Counting {
        inner: f,
        counts: vec![
            "operators 2\nservers 3".to_owned(),
            "operators 2\nservers 3\nworld_rooms 0".to_owned(),
        ],
    }
}

fn preflight(m: &mut Counting, products: &[Product], with: &[&str]) -> Result<(), String> {
    run(
        m,
        &cfg(),
        Path::new("D:/repo"),
        &repo(products),
        &Options {
            with: with.iter().map(|s| (*s).to_owned()).collect(),
            dry_run: false,
        },
    )
}

/// A preflight of every product on `setup`.
fn preflight_on(m: &mut Counting, setup: &Config) -> Result<(), String> {
    run(
        m,
        setup,
        Path::new("D:/repo"),
        &repo(&[Product::Hub, Product::Ace, Product::Empyrean]),
        &Options::default(),
    )
}

fn ledger(m: &mut Counting) -> Ledger {
    Ledger::load(m, Path::new("D:\\preflight")).expect("a ledger")
}

/// What every script sent to a Windows machine starts with, after the line that says what it does.
const SETTINGS: &str =
    "$ErrorActionPreference = 'Stop'\n$ProgressPreference = 'SilentlyContinue'\n";

const STOP_EMPYREAN: &str = r#"$service = 'EmpyreanDev'
if (-not ((sc.exe query $service) -match 'STOPPED')) {
  sc.exe stop $service | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "sc stop $service failed ($LASTEXITCODE)" }
}
$until = (Get-Date).AddMinutes(1)
while (-not ((sc.exe query $service) -match 'STOPPED')) {
  if ((Get-Date) -gt $until) { throw "$service did not stop within a minute" }
  Start-Sleep -Milliseconds 500
}
"#;

const START_EMPYREAN: &str = r#"$service = 'EmpyreanDev'
if (-not ((sc.exe query $service) -match 'RUNNING')) {
  sc.exe start $service | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "sc start $service failed ($LASTEXITCODE)" }
}
$until = (Get-Date).AddMinutes(1)
while (-not ((sc.exe query $service) -match 'RUNNING')) {
  if ((Get-Date) -gt $until) { throw "$service did not start within a minute" }
  Start-Sleep -Milliseconds 500
}
"#;

/// The Empyrean world's folder on the Windows machine, as a PowerShell literal spells it.
const BOX_EMP: &str = r"C:\Worlds\Bob''s Empyrean Dev";
/// The ACE world's mod folder there.
const BOX_MOD: &str = r"D:\Games\ACE Server\Server\Mods\Dereth.Network";

/// The scripts the Empyrean world on the Windows machine is sent by a first preflight, in order.
fn box_emp_preflight_scripts() -> [String; 3] {
    let d = BOX_EMP;
    let dbs = format!(
        "@('{d}\\shard.db', '{d}\\shard.db-wal', '{d}\\shard.db-shm', '{d}\\auth.db', '{d}\\auth.db-wal', '{d}\\auth.db-shm')"
    );
    [
        format!(
            "# box-emp on world-box: stopped, and its configuration staged
{SETTINGS}{STOP_EMPYREAN}$stage = Join-Path $HOME '.dereth-preflight\\box-emp'
if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath '{d}\\empyrean.toml' -Destination (Join-Path $stage 'config')
"
        ),
        format!(
            "# box-emp on world-box: its files as they are, kept beside it
{SETTINGS}$backup = '{d}\\.preflight-backup'
if (Test-Path -LiteralPath $backup) {{ Remove-Item -LiteralPath $backup -Recurse -Force }}
New-Item -ItemType Directory -Force -Path (Join-Path $backup 'db') | Out-Null
Copy-Item -LiteralPath '{d}\\empyrean-server.exe' -Destination (Join-Path $backup 'replaced') -Recurse
Copy-Item -LiteralPath '{d}\\empyrean.toml' -Destination (Join-Path $backup 'config')
foreach ($file in {dbs}) {{
  if (Test-Path -LiteralPath $file) {{ Copy-Item -LiteralPath $file -Destination (Join-Path $backup 'db') }}
}}
"
        ),
        format!(
            "# box-emp on world-box: the build and its configuration put in place, and started
{SETTINGS}$stage = Join-Path $HOME '.dereth-preflight\\box-emp'
Copy-Item -LiteralPath (Join-Path $stage 'build') -Destination '{d}\\empyrean-server.exe' -Force
Copy-Item -LiteralPath (Join-Path $stage 'config') -Destination '{d}\\empyrean.toml' -Force
Remove-Item -LiteralPath $stage -Recurse -Force
{START_EMPYREAN}"
        ),
    ]
}

/// The script that puts the Empyrean world on the Windows machine back.
fn box_emp_restore_script() -> String {
    let d = BOX_EMP;
    let dbs = format!(
        "@('{d}\\shard.db', '{d}\\shard.db-wal', '{d}\\shard.db-shm', '{d}\\auth.db', '{d}\\auth.db-wal', '{d}\\auth.db-shm')"
    );
    format!(
        "# box-emp on world-box: put back as it was before the preflight
{SETTINGS}{STOP_EMPYREAN}$backup = '{d}\\.preflight-backup'
Copy-Item -LiteralPath (Join-Path $backup 'replaced') -Destination '{d}\\empyrean-server.exe' -Recurse -Force
Copy-Item -LiteralPath (Join-Path $backup 'config') -Destination '{d}\\empyrean.toml' -Force
foreach ($file in {dbs}) {{
  $kept = Join-Path (Join-Path $backup 'db') (Split-Path -Leaf $file)
  if (Test-Path -LiteralPath $kept) {{ Copy-Item -LiteralPath $kept -Destination $file -Force }}
  elseif (Test-Path -LiteralPath $file) {{ Remove-Item -LiteralPath $file -Force }}
}}
{START_EMPYREAN}Remove-Item -LiteralPath $backup -Recurse -Force
"
    )
}

#[test]
fn the_setup_file_names_the_hub_and_the_worlds_and_refuses_what_it_cannot_use() {
    let c = cfg();
    assert_eq!(c.remote, "origin");
    assert_eq!(c.hub.listen, "127.0.0.1:8650");
    assert_eq!(c.dev_hub_link(), "wss://dev-hub.example.org");
    assert_eq!(c.worlds[1].kind, Kind::Ace);
    assert_eq!(
        c.release_ace_dir().as_deref(),
        Some(Path::new("D:\\ACE\\Server"))
    );
    assert_eq!(c.worlds[0].config_file(), "D:\\worlds\\emp\\empyrean.toml");
    assert_eq!(
        c.worlds[1].config_file(),
        "D:\\ACE\\Server\\Mods\\Dereth.Network\\Settings.json"
    );
    assert_eq!(c.worlds[2].replaced(), "/srv/emp/empyrean-server");
    let names = |with: &[String]| -> Vec<String> {
        c.worlds_for(with)
            .expect("worlds")
            .iter()
            .map(|w| w.name.clone())
            .collect()
    };
    assert_eq!(
        names(&[]),
        ["emp", "ace"],
        "an optional world only when asked"
    );
    assert_eq!(names(&["vm".to_owned()]), ["emp", "ace", "vm"]);
    assert!(c.worlds_for(&["nowhere".to_owned()]).is_err());

    for (bad, why) in [
        (format!("{SETUP}\nsurprise = 1\n"), "an unknown setting"),
        (
            SETUP.replace("discord_client_id = \"555\"\n", ""),
            "a secret with no client id",
        ),
        (
            SETUP.replace("https://dev-hub", "ftp://dev-hub"),
            "a public address that is not https",
        ),
        (
            SETUP.replace("name = \"ace\"", "name = \"emp\""),
            "two worlds of one name",
        ),
    ] {
        assert!(Config::parse(&bad).is_err(), "{why}");
    }

    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert_eq!(
        super::config::default_path(&env(&[("APPDATA", "C:\\Users\\o\\AppData\\Roaming")]), true),
        Some(
            Path::new("C:\\Users\\o\\AppData\\Roaming")
                .join("Dereth")
                .join("preflight.toml")
        )
    );
    assert_eq!(
        super::config::default_path(
            &env(&[("DERETH_PREFLIGHT", "E:\\p.toml"), ("APPDATA", "C:\\x")]),
            true
        ),
        Some(Path::new("E:\\p.toml").to_path_buf())
    );
    assert_eq!(
        super::config::default_path(&env(&[("HOME", "/home/o")]), false),
        Some(Path::new("/home/o/.config/dereth/preflight.toml").to_path_buf())
    );
}

#[test]
fn a_world_names_the_machine_it_runs_on_and_that_machine_says_what_it_runs() {
    let c = remote_cfg();
    let places: Vec<(&str, Option<(Os, &str)>)> = c
        .worlds
        .iter()
        .map(|w| {
            (
                w.name.as_str(),
                match w.place() {
                    Place::Here => None,
                    Place::Windows(h) | Place::Linux(h) => Some((h.os, h.ssh.as_str())),
                },
            )
        })
        .collect();
    assert_eq!(
        places,
        [
            ("box-emp", Some((Os::Windows, "owner@world-box"))),
            ("box-ace", Some((Os::Windows, "owner@world-box"))),
            ("infiltration", Some((Os::Linux, "owner@world-vm"))),
        ],
        "two worlds share a machine"
    );
    // Each machine spells its own paths.
    assert_eq!(
        c.worlds[0].config_file(),
        "C:\\Worlds\\Bob's Empyrean Dev\\empyrean.toml"
    );
    assert_eq!(
        c.worlds[0].replaced(),
        "C:\\Worlds\\Bob's Empyrean Dev\\empyrean-server.exe"
    );
    assert_eq!(
        c.worlds[1].config_file(),
        "D:\\Games\\ACE Server\\Server\\Mods\\Dereth.Network\\Settings.json"
    );
    assert_eq!(c.worlds[2].config_file(), "/srv/empyrean/empyrean.toml");
    assert_eq!(c.machines[0].account(), Some("owner"));
    assert_eq!(c.machines[1].target(), "x86_64-unknown-linux-gnu.2.28");
    assert_eq!(cfg().machines[0].target(), "x86_64-unknown-linux-gnu");
    assert_eq!(
        steps::rust_target("x86_64-unknown-linux-gnu.2.28"),
        "x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        steps::rust_target("aarch64-unknown-linux-gnu"),
        "aarch64-unknown-linux-gnu"
    );
    assert_eq!(
        c.release_ace_dir().as_deref(),
        Some(
            Path::new("D:\\preflight")
                .join("ace")
                .join("box-ace")
                .as_path()
        ),
        "with no ACE world here, the copy of the other's assemblies"
    );

    for (bad, why) in [
        (
            REMOTE.replace("machine = \"world-vm\"", "machine = \"nowhere\""),
            "no such machine",
        ),
        (
            REMOTE.replace(
                "os = \"windows\"",
                "os = \"windows\"\ntarget = \"x86_64-pc-windows-gnu\"",
            ),
            "a target for a Windows machine, whose builds are this one's",
        ),
        (
            REMOTE.replace(
                "kind = \"ace\"\nmachine = \"world-box\"",
                "kind = \"ace\"\nmachine = \"world-vm\"",
            ),
            "an ACE world on Linux",
        ),
        (
            REMOTE.replace("name = \"box-emp\"", "name = \"box emp\""),
            "a world's name with a space, which goes in paths",
        ),
        (
            REMOTE.replace("name = \"world-vm\"", "name = \"world-box\""),
            "two machines of one name",
        ),
        (
            REMOTE.replace("os = \"linux\"", "os = \"plan9\""),
            "an unknown system",
        ),
    ] {
        assert!(Config::parse(&bad).is_err(), "{why}");
    }

    // A setup file written before nothing was built on a world's machine says what to do now.
    let old = "work_dir = 'D:\\p'\n[hub]\nssh = \"h\"\nremote_db = \"/d\"\npublic_url = \"https://x\"\n\n[[world]]\nname = \"vm\"\nkind = \"empyrean\"\nservice = \"empyrean\"\ndir = \"/srv/emp\"\nssh = \"owner@vm\"\nbuild = \"cargo build\"\nbinary = \"/home/o/empyrean-server\"\n";
    let e = Config::parse(old).expect_err("refused");
    assert!(
        e.contains("vm: `build` and `binary` are gone")
            && e.contains("cargo zigbuild")
            && e.contains("Take them out"),
        "{e}"
    );
    assert!(e.contains("vm: a world names its machine"), "{e}");
}

#[test]
fn a_change_belongs_to_the_products_built_from_its_package_and_each_takes_its_release() {
    let packages: Vec<(String, String)> = [
        ("dereth-primitives", "core/primitives"),
        ("dereth-network-protocol", "network/protocol"),
        ("dereth-network-hub", "network/hub"),
        ("empyrean-world", "empyrean/crates/world"),
        ("empyrean-server", "empyrean/server"),
        ("dereth-client", "dereth/client"),
        ("dereth-launch", "dereth/launcher/crates/launch"),
    ]
    .iter()
    .map(|(n, d)| ((*n).to_owned(), (*d).to_owned()))
    .collect();
    let set = |names: &[&str]| {
        names
            .iter()
            .map(|n| (*n).to_owned())
            .collect::<BTreeSet<_>>()
    };
    let depends: BTreeMap<Product, BTreeSet<String>> = [
        (
            Product::Hub,
            set(&[
                "dereth-network-hub",
                "dereth-network-protocol",
                "dereth-primitives",
            ]),
        ),
        (
            Product::Empyrean,
            set(&[
                "empyrean-server",
                "empyrean-world",
                "dereth-network-protocol",
                "dereth-primitives",
            ]),
        ),
        (
            Product::Dereth,
            set(&["dereth-client", "dereth-primitives", "dereth-launch"]),
        ),
    ]
    .into_iter()
    .collect();
    let of = |paths: &[&str]| {
        let changed: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        plan::products_of(&changed, &packages, &depends)
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert_eq!(of(&["network/hub/src/core.rs"]), [Product::Hub]);
    assert_eq!(
        of(&["network/protocol/src/lib.rs"]),
        [Product::Hub, Product::Empyrean]
    );
    assert_eq!(
        of(&["network/ace-mod/Dereth.Network/Core/Wire.cs"]),
        [Product::Ace]
    );
    assert_eq!(
        of(&["core/primitives/src/lib.rs"]),
        [Product::Hub, Product::Empyrean, Product::Dereth]
    );
    assert_eq!(
        of(&[
            "dereth/launcher/ui/app.js",
            "dereth/launcher/crates/launch/src/pad.rs"
        ]),
        [Product::Dereth]
    );
    assert_eq!(
        of(&["docs/network/03-world-behaviour.md", "README.md"]),
        [] as [Product; 0]
    );
    assert_eq!(
        of(&["Cargo.lock"]),
        [Product::Hub, Product::Empyrean, Product::Dereth]
    );

    assert_eq!(
        plan::next_version("0.3.1", Bump::Patch).as_deref(),
        Ok("0.3.2")
    );
    assert_eq!(
        plan::next_version("0.3.1", Bump::Minor).as_deref(),
        Ok("0.4.0")
    );
    assert_eq!(
        plan::next_version("0.4.0-rc.2", Bump::Minor).as_deref(),
        Ok("0.4.0")
    );
    assert_eq!(
        plan::hub_schema("x\npub const SCHEMA_VERSION: i64 = 10;\n"),
        Some(10)
    );
    assert_eq!(plan::hub_bump(Some(10), Some(11)).0, Bump::Minor);
    assert_eq!(plan::hub_bump(Some(10), Some(10)).0, Bump::Patch);
    assert_eq!(
        plan::empyrean_bump(Some("it migrates the shard")).0,
        Bump::Minor
    );

    let mut stamped = BTreeMap::new();
    assert_eq!(
        plan::stamp(Product::Hub, "0.2.2", &mut stamped),
        "0.2.2-rc.1"
    );
    assert_eq!(
        plan::stamp(Product::Hub, "0.2.2", &mut stamped),
        "0.2.2-rc.2"
    );
    assert_eq!(
        plan::stamp(Product::Empyrean, "0.2.2", &mut stamped),
        "0.2.2-rc.1",
        "each product its own"
    );
}

#[test]
fn a_worlds_hub_setting_is_changed_alone_and_put_back_as_it_was() {
    let (t, old) = edit::empyrean_hub(EMP_TOML, "wss://dev-hub.example.org").expect("edited");
    assert_eq!(old.as_deref(), Some("wss://hub.dereth.network"));
    assert_eq!(
        t,
        EMP_TOML.replace("wss://hub.dereth.network", "wss://dev-hub.example.org")
    );
    assert_eq!(
        edit::empyrean_hub_back(&t, old.as_deref()).expect("back"),
        EMP_TOML
    );

    let unset = "[server]\nworld_name = \"Dev\"\n\n[federation]\nenabled = true\n\n[database]\n";
    let (t, old) = edit::empyrean_hub(unset, "wss://dev").expect("edited");
    assert_eq!(old, None);
    assert!(
        t.contains("[federation]\nenabled = true\nhub_url = \"wss://dev\"\n\n[database]"),
        "{t}"
    );
    assert_eq!(edit::empyrean_hub_back(&t, None).expect("back"), unset);

    let (t, _) = edit::empyrean_hub("[server]\n", "wss://dev").expect("edited");
    assert_eq!(t, "[server]\n\n[federation]\nhub_url = \"wss://dev\"\n");

    let (j, old) = edit::ace_hub(ACE_SETTINGS, "wss://dev-hub.example.org").expect("edited");
    assert_eq!(old, None);
    let v: serde_json::Value = serde_json::from_str(&j).expect("json");
    assert_eq!(v["HubUrl"], "wss://dev-hub.example.org");
    assert_eq!(v["Slug"], "acedev");
    assert!(
        v.get("AllowLocalWs").is_none(),
        "only for a hub on this machine"
    );
    let (j2, _) = edit::ace_hub(ACE_SETTINGS, "ws://127.0.0.1:8650").expect("edited");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&j2).expect("json")["AllowLocalWs"],
        true
    );
    let back: serde_json::Value =
        serde_json::from_str(&edit::ace_hub_back(&j, None).expect("back")).expect("json");
    assert_eq!(
        back,
        serde_json::from_str::<serde_json::Value>(ACE_SETTINGS).expect("json")
    );
    // An empty settings file sets nothing (a dry run reads the copy it did not make as one).
    let (j3, old) = edit::ace_hub("", "wss://dev").expect("edited");
    assert_eq!(old, None);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&j3).expect("json")["HubUrl"],
        "wss://dev"
    );
}

#[test]
fn a_preflight_builds_the_candidates_runs_the_dev_hub_on_a_defanged_copy_and_installs_each_world() {
    let mut m = counting(fake());
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect("preflighted");
    let f = &m.inner;

    // The builds, in a worktree of the commit, stamped as candidates.
    assert!(
        f.at("git worktree add --detach D:/preflight/src c0ffee")
            < f.at("cargo build --release -p dereth-network-hub")
    );
    assert!(f
        .get("D:/preflight/src/network/hub/Cargo.toml")
        .is_some_and(|t| t.contains("version = \"0.2.2-rc.1\"")));
    assert!(f
        .get("D:/preflight/src/empyrean/crates/world/Cargo.toml")
        .is_some_and(|t| t.contains("0.3.2-rc.1")));
    assert!(f.done("cargo build --release -p empyrean-server").len() == 1);
    assert!(f.done("dotnet").is_empty(), "the mod did not change");
    assert!(
        f.last("git worktree remove --force D:/preflight/src")
            > f.at("cargo build --release -p empyrean-server")
    );

    // The dev hub: the real registry backed up there, copied down, counted, migrated and defanged,
    // and served with the dev application, behind the tunnel.
    let backup = f.at("ssh owner@vps.example.org rm -f /tmp/dereth-preflight-hub.sqlite && dereth-network-hub --db /srv/hub/hub.sqlite backup /tmp/dereth-preflight-hub.sqlite");
    let fetched = f.at("fetch owner@vps.example.org:/tmp/dereth-preflight-hub.sqlite");
    let defanged = f.at(&format!(
        "D:/preflight/builds/network-hub-0.2.2-rc.1/{} --db D:/preflight/hub/dev.sqlite defang-copy --this-is-a-copy --keep 1001",
        exe("dereth-network-hub")
    ));
    let served = f.at("spawn ");
    assert!(
        backup < fetched && fetched < defanged && defanged < served,
        "{:#?}",
        f.log
    );
    let serve = f.done("spawn ")[0].clone();
    for part in [
        "--public-url https://dev-hub.example.org",
        "serve --listen 127.0.0.1:8650",
        "--admin 1001",
        "--discord-client-id 555",
        "--discord-client-secret-file D:/secrets/dev-client-secret.txt",
        "--discord-bot-token-file D:/secrets/dev-bot-token.txt",
    ] {
        assert!(serve.contains(part), "{part} in {serve}");
    }
    assert_eq!(
        f.done("spawn cloudflared"),
        ["spawn cloudflared tunnel --url http://127.0.0.1:8650 run dev-hub"]
    );
    assert!(
        f.done("ssh owner@vps.example.org rm -f /tmp/dereth-preflight-hub.sqlite")
            .len()
            == 2,
        "and the backup there removed"
    );

    // Empyrean's world: stopped, backed up with its database, the candidate in place, pointed at
    // the dev hub, started.
    let stop = f.at("service EmpyreanDev Stopped");
    let kept = f.at("copy D:/worlds/emp/empyrean-server.exe D:/preflight/backups/emp/replaced");
    let start = f.at("service EmpyreanDev Running");
    assert!(stop < kept && kept < start);
    assert_eq!(
        f.get("D:/preflight/backups/emp/db/shard.db"),
        Some("the shard")
    );
    assert_eq!(
        f.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.2-rc.1")
    );
    assert_eq!(
        f.get("D:/worlds/emp/empyrean.toml"),
        Some(
            EMP_TOML
                .replace("wss://hub.dereth.network", "wss://dev-hub.example.org")
                .as_str()
        )
    );
    // ACE's world: its mod unchanged, pointed at the dev hub.
    assert_eq!(
        f.get("D:/ACE/Server/Mods/Dereth.Network/Dereth.Network.dll"),
        Some("mod 0.1.2")
    );
    assert!(f
        .get("D:/ACE/Server/Mods/Dereth.Network/Settings.json")
        .is_some_and(|t| t.contains("\"HubUrl\": \"wss://dev-hub.example.org\"")));
    assert_eq!(f.services.get("AceDev"), Some(&Service::Running));
    // The optional world on the Linux machine is not touched, and no tool for it asked after.
    assert!(f.done("ssh owner@vm.example.org").is_empty());
    assert!(f.done("probe").is_empty());

    let l = ledger(&mut m);
    assert_eq!(l.stage, Stage::Preflighted);
    assert_eq!(
        l.commit.as_deref(),
        Some("c0ffee0000000000000000000000000000000001")
    );
    let releases: Vec<(Product, &str, &str)> = l
        .releases
        .iter()
        .map(|r| (r.product, r.version.as_str(), r.candidate.as_str()))
        .collect();
    assert_eq!(
        releases,
        [
            (Product::Hub, "0.2.2", "0.2.2-rc.1"),
            (Product::Empyrean, "0.3.2", "0.3.2-rc.1")
        ]
    );
    assert_eq!(
        l.hub.as_ref().map(|h| (h.pid, h.tunnel)),
        Some((101, Some(102)))
    );
    assert!(
        l.counts
            .contains(&("world_rooms".to_owned(), None, Some(0))),
        "{:?}",
        l.counts
    );
    assert!(l.counts.contains(&("servers".to_owned(), Some(3), Some(3))));
    let worlds: Vec<(&str, Option<&str>, Option<&str>)> = l
        .worlds
        .iter()
        .map(|w| {
            (
                w.name.as_str(),
                w.original_hub.as_deref(),
                w.version.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        worlds,
        [
            ("emp", Some("wss://hub.dereth.network"), Some("0.3.2-rc.1")),
            ("ace", None, None)
        ]
    );
    let summary = steps::summary(&cfg(), &l);
    assert!(summary.contains("emp: runs 0.3.2-rc.1"), "{summary}");
    assert!(
        summary.contains("cargo xtask preflight rollback"),
        "{summary}"
    );
}

#[test]
fn a_second_preflight_stamps_the_next_candidate_and_keeps_the_first_backups_and_hub() {
    let mut m = counting(fake());
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect("preflighted");
    m.inner.files.insert(
        format!("D:/preflight/target/release/{}", exe("empyrean-server")),
        "empyrean 0.3.2-rc.2".to_owned(),
    );
    m.counts = vec![String::new(), String::new()];
    let backups_before = m
        .inner
        .done("copy D:/worlds/emp/empyrean-server.exe D:/preflight/backups")
        .len();
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect("again");
    let f = &m.inner;
    assert_eq!(
        f.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.2-rc.2")
    );
    assert_eq!(
        f.done("copy D:/worlds/emp/empyrean-server.exe D:/preflight/backups")
            .len(),
        backups_before,
        "the files as they were before any preflight stay the backup"
    );
    assert_eq!(
        f.get("D:/preflight/backups/emp/replaced"),
        Some("empyrean 0.3.1")
    );
    assert_eq!(
        f.done("kill"),
        ["kill 102", "kill 101"],
        "the first dev hub and its tunnel stopped"
    );
    let l = ledger(&mut m);
    assert_eq!(l.releases[1].candidate, "0.3.2-rc.2");
    assert_eq!(
        l.worlds[0].original_hub.as_deref(),
        Some("wss://hub.dereth.network"),
        "not the dev hub's"
    );
}

#[test]
fn a_rollback_puts_every_world_back_from_its_backup_and_stops_the_dev_hub() {
    let mut m = counting(fake());
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect("preflighted");
    m.inner
        .files
        .insert("D:/worlds/emp/shard.db".into(), "migrated".into());
    m.inner
        .files
        .insert("D:/worlds/emp/shard.db-wal".into(), "a log since".into());
    let mut l = ledger(&mut m);
    steps::rollback(&mut m, &cfg(), &mut l).expect("rolled back");
    let f = &m.inner;
    assert_eq!(
        f.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.1")
    );
    assert_eq!(f.get("D:/worlds/emp/empyrean.toml"), Some(EMP_TOML));
    assert_eq!(f.get("D:/worlds/emp/shard.db"), Some("the shard"));
    assert_eq!(f.get("D:/worlds/emp/shard.db-wal"), None);
    assert_eq!(
        f.get("D:/ACE/Server/Mods/Dereth.Network/Settings.json"),
        Some(ACE_SETTINGS)
    );
    assert!(f.last("service AceDev Running") > f.last("copy D:/preflight/backups/ace/config"));
    assert!(
        f.last("kill 102") > f.last("service EmpyreanDev Running"),
        "the worlds first, then the dev hub"
    );
    assert_eq!(l.stage, Stage::Idle);
    assert!(l.worlds.is_empty() && l.hub.is_none() && l.commit.is_none());
}

#[test]
fn a_run_cut_short_keeps_what_it_did_so_that_it_can_be_rolled_back() {
    let mut f = fake();
    f.replies.push((
        "service AceDev Stopped".to_owned(),
        Err("access denied".to_owned()),
    ));
    let mut m = counting(f);
    let e = preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect_err("stopped");
    assert!(
        e.contains("ace: access denied") && e.contains("rollback"),
        "{e}"
    );
    let l = ledger(&mut m);
    assert_eq!(l.stage, Stage::Preflighted);
    assert_eq!(
        l.worlds.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        ["emp"]
    );
    assert!(l.hub.is_some());

    // Cut short with the candidate in place: the world is recorded, so a rollback puts it back.
    let mut m = counting(fake().reply(
        "service EmpyreanDev Running",
        Err("the service did not start"),
    ));
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect_err("stopped");
    assert_eq!(
        m.inner.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.2-rc.1")
    );
    let mut l = ledger(&mut m);
    assert_eq!(
        l.worlds.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        ["emp"]
    );
    m.inner.replies.clear();
    steps::rollback(&mut m, &cfg(), &mut l).expect("rolled back");
    assert_eq!(
        m.inner.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.1")
    );
}

#[test]
fn a_preflight_runs_on_a_clean_branch_alone() {
    let mut m = counting(fake());
    let mut r = repo(&[Product::Hub]);
    r.branch = "HEAD".to_owned();
    let o = Options::default();
    assert!(
        run(&mut m, &cfg(), Path::new("D:/repo"), &r, &o).is_err_and(|e| e.contains("detached"))
    );
    r.branch = "release".to_owned();
    r.dirty = vec!["M network/hub/src/core.rs".to_owned()];
    assert!(
        run(&mut m, &cfg(), Path::new("D:/repo"), &r, &o).is_err_and(|e| e.contains("uncommitted"))
    );
    assert!(m.inner.log.is_empty(), "nothing done");
}

#[test]
fn ship_fast_forwards_main_to_exactly_the_preflighted_commit_and_releases_each_product() {
    let commit = "c0ffee0000000000000000000000000000000001";
    let shipping = |here: &str, there: &str, ancestor: bool| {
        let mut f = Fake::default()
            .reply("git rev-parse refs/heads/release", Ok(here))
            .reply("git rev-parse --verify --quiet origin/release", Ok(there))
            .reply("git rev-parse main", Ok("beef"));
        if !ancestor {
            f = f.reply("git merge-base --is-ancestor", Err("not an ancestor"));
        }
        let mut l = Ledger {
            stage: Stage::Preflighted,
            commit: Some(commit.to_owned()),
            branch: Some("release".to_owned()),
            ..Ledger::default()
        };
        let mut stamped = BTreeMap::new();
        l.releases = super::releases(&repo(&[Product::Hub, Product::Empyrean]), &mut stamped)
            .expect("releases");
        (f, l)
    };
    let env = [("DERETH_ACE_DIR".to_owned(), "D:\\ACE\\Server".to_owned())];

    let (mut f, mut l) = shipping("feed", commit, true);
    let e = ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, false)
        .expect_err("refused");
    assert!(e.starts_with("release has moved"), "{e}");
    assert!(f.done("git checkout").is_empty() && f.done("cargo").is_empty());

    let (mut f, mut l) = shipping(commit, "feed", true);
    let e = ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, false)
        .expect_err("refused");
    assert!(e.starts_with("origin/release has moved"), "{e}");
    assert!(f.done("git checkout").is_empty() && f.done("cargo").is_empty());

    // A branch never pushed is checked here alone.
    let (mut f, mut l) = shipping(commit, "", true);
    ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, true)
        .expect("not on the remote");

    let (mut f, mut l) = shipping(commit, commit, false);
    let e = ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, false)
        .expect_err("refused");
    assert!(e.contains("cannot be fast-forwarded"), "{e}");

    let (mut f, mut l) = shipping(commit, commit, true);
    ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, true).expect("a dry run");
    assert!(
        f.done("git checkout").is_empty() && f.done("cargo").is_empty(),
        "a dry run changes nothing"
    );
    assert_eq!(l.stage, Stage::Preflighted);

    let (mut f, mut l) = shipping(commit, commit, true);
    ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, false).expect("shipped");
    assert!(f.at("git fetch origin") < f.at("git checkout main"));
    assert!(f.at("git checkout main") < f.at(&format!("git merge --ff-only {commit}")));
    assert_eq!(
        f.done("cargo xtask release"),
        [
            "cargo xtask release network-hub 0.2.2 --push --remote origin [DERETH_ACE_DIR=D:/ACE/Server]",
            "cargo xtask release empyrean 0.3.2 --push --remote origin [DERETH_ACE_DIR=D:/ACE/Server]",
        ]
    );
    assert_eq!(l.stage, Stage::Shipped);
    assert!(
        ship::ship(&mut f, Path::new("D:/repo"), "origin", &env, &mut l, false)
            .is_err_and(|e| e.contains("shipped already"))
    );
    let mut idle = Ledger::default();
    assert!(ship::ship(
        &mut f,
        Path::new("D:/repo"),
        "origin",
        &env,
        &mut idle,
        false
    )
    .is_err_and(|e| e.contains("nothing is preflighted")));
}

#[test]
fn finish_installs_the_final_builds_points_each_world_back_and_stops_the_dev_hub() {
    let mut m = counting(fake());
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &[]).expect("preflighted");
    let mut l = ledger(&mut m);
    l.stage = Stage::Shipped;
    m.inner.files.insert(
        "D:/preflight/builds/empyrean-0.3.2/empyrean-server.exe".into(),
        "empyrean 0.3.2".into(),
    );
    let mut builds = steps::Builds::default();
    builds.empyrean.insert(
        None,
        steps::Build {
            version: "0.3.2".into(),
            path: Path::new("D:/preflight/builds/empyrean-0.3.2/empyrean-server.exe").to_path_buf(),
        },
    );
    steps::finish(&mut m, &cfg(), &builds, &mut l).expect("finished");
    let f = &m.inner;
    assert_eq!(
        f.get("D:/worlds/emp/empyrean-server.exe"),
        Some("empyrean 0.3.2")
    );
    assert_eq!(
        f.get("D:/worlds/emp/empyrean.toml"),
        Some(EMP_TOML),
        "its own hub again"
    );
    let ace: serde_json::Value = serde_json::from_str(
        f.get("D:/ACE/Server/Mods/Dereth.Network/Settings.json")
            .expect("settings"),
    )
    .expect("json");
    assert!(
        ace.get("HubUrl").is_none(),
        "it named none, the network's hub: {ace}"
    );
    assert_eq!(f.services.get("EmpyreanDev"), Some(&Service::Running));
    assert!(f.done("kill").len() == 2);
    assert_eq!(l.stage, Stage::Idle);
    assert!(l.worlds.is_empty());
}

#[test]
fn the_setup_script_here_grants_the_services_makes_the_ace_service_the_tunnel_and_the_secret_files()
{
    let scripts = setup::scripts(
        &cfg(),
        "C:\\Users\\o\\AppData\\Roaming\\Dereth\\preflight.toml",
        None,
    )
    .expect("no Windows machine needs a key");
    assert_eq!(
        scripts.iter().map(|s| s.file.as_str()).collect::<Vec<_>>(),
        ["setup.ps1", "setup-vm.sh"]
    );
    let text = &scripts[0].text;
    for want in [
        "Grant-StartStop 'EmpyreanDev'",
        "Grant-StartStop 'AceDev'",
        "nssm install 'AceDev' 'dotnet' 'ACE.Server.dll'",
        "nssm set 'AceDev' AppDirectory 'D:\\ACE\\Server'",
        "nssm set 'AceDev' DependOnService 'MariaDB'",
        "throw ('There is no service ' + 'MariaDB' + ' for ' + 'ace' + ' to wait for: name its mariadb_dir",
        "DERETH_NETWORK_KEY=",
        "'cloudflared' tunnel route dns 'dev-hub' 'dev-hub.example.org'",
        "if (-not (Test-Path $cert)) { throw \"No Cloudflare login at ${cert}: run 'cloudflared tunnel login'",
        "if ($LASTEXITCODE -ne 0) { throw \"cloudflared tunnel create failed ($LASTEXITCODE)\" }",
        "if ($LASTEXITCODE -ne 0) { throw \"cloudflared tunnel route dns failed ($LASTEXITCODE)\" }",
        "icacls 'D:\\secrets\\dev-bot-token.txt' /inheritance:r",
        "(A;;RPWPLCLORC;;;$sid)",
        "rustup target add 'x86_64-unknown-linux-gnu'",
        "icacls 'D:\\worlds\\emp' /grant \"${Account}:(OI)(CI)M\"",
        "icacls 'D:\\ACE\\Server\\Mods\\Dereth.Network\\Settings.json' /grant \"${Account}:M\"",
    ] {
        assert!(text.contains(want), "{want}\n{text}");
    }
    assert!(
        !text.contains("Grant-StartStop 'empyrean'"),
        "the Linux world's unit is not a Windows service"
    );
    assert!(
        text.contains("${Service}:"),
        "a variable before a colon is spelled as PowerShell reads it"
    );
    assert!(scripts[1]
        .text
        .contains("\"$systemctl stop empyrean, $systemctl start empyrean\""));
}

#[test]
fn the_setup_file_the_releasing_notes_show_is_one_the_preflight_reads() {
    let notes = include_str!("../../../../network/RELEASING.md");
    let start = notes.find("```toml\n").expect("an example") + "```toml\n".len();
    let example = &notes[start..start + notes[start..].find("```").expect("its end")];
    let c = Config::parse(example).expect("the example is a setup file");
    /// A world: its name, what it runs, its machine and what that runs, and whether it is optional.
    type Shown<'a> = (&'a str, Kind, Option<(&'a str, Os)>, bool);
    let worlds: Vec<Shown<'_>> = c
        .worlds
        .iter()
        .map(|w| {
            let on = match w.place() {
                Place::Here => None,
                Place::Windows(h) | Place::Linux(h) => Some((h.name.as_str(), h.os)),
            };
            (w.name.as_str(), w.kind, on, w.optional)
        })
        .collect();
    assert_eq!(
        worlds,
        [
            (
                "empyrean",
                Kind::Empyrean,
                Some(("world-box", Os::Windows)),
                false
            ),
            ("ace", Kind::Ace, Some(("world-box", Os::Windows)), false),
            (
                "infiltration",
                Kind::Empyrean,
                Some(("world-vm", Os::Linux)),
                true
            ),
        ]
    );
}

#[test]
fn a_world_on_a_linux_machine_takes_empyrean_built_here_with_zigbuild_copied_over() {
    let vm = "owner@vm.example.org";
    let at = |p: &str| format!("{vm}:{p}");
    let mut f = fake();
    f.remote.insert(at("/srv/emp/shard.db"), "the shard".into());
    // A world stopped after a crash: its log holds what the database has not taken in yet.
    f.remote
        .insert(at("/srv/emp/shard.db-wal"), "the shard's log".into());
    let mut m = counting(f);
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &["vm"]).expect("preflighted");
    let f = &m.inner;
    // Built here for the machine's target, and for this machine's world.
    assert_eq!(
        f.done("cargo zigbuild"),
        ["cargo zigbuild --release -p empyrean-server --target x86_64-unknown-linux-gnu [CARGO_TARGET_DIR=D:/preflight/target]"]
    );
    assert_eq!(f.done("cargo build --release -p empyrean-server").len(), 1);
    assert!(f.at("probe cargo-zigbuild --version") < f.at("git worktree add"));

    // The exact commands, a backup beside the world taking each database's log and shared memory
    // when they are there.
    let b = "/srv/emp/.preflight-backup";
    let dbs = [
        "shard.db",
        "shard.db-wal",
        "shard.db-shm",
        "auth.db",
        "auth.db-wal",
        "auth.db-shm",
    ];
    let backup = format!(
        "set -e\nrm -rf '{b}'\nmkdir -p '{b}/db'\ncp -a '/srv/emp/empyrean-server' '{b}/replaced'\ncp -a '/srv/emp/empyrean.toml' '{b}/config'\n{}",
        dbs.iter()
            .map(|d| format!("if [ -e '/srv/emp/{d}' ]; then cp -a '/srv/emp/{d}' '{b}/db/{d}'; fi\n"))
            .collect::<String>()
    );
    let place = "set -e\ncp '/srv/emp/empyrean-server.preflight-new' '/srv/emp/empyrean-server'\nrm -f '/srv/emp/empyrean-server.preflight-new'\n";
    let sent: Vec<&str> = f
        .sent
        .iter()
        .filter(|(h, _)| h == vm)
        .map(|(_, c)| c.as_str())
        .collect();
    assert_eq!(
        sent,
        [
            "sudo -n systemctl stop empyrean",
            backup.as_str(),
            place,
            "sudo -n systemctl start empyrean"
        ]
    );
    let kept = f.at(&format!("ssh {vm} set -e\nrm -rf '{b}'"));
    let copied = f.at(&format!(
        "send D:/preflight/builds/empyrean-0.3.2-rc.1-x86_64-unknown-linux-gnu/empyrean-server {vm}:/srv/emp/empyrean-server.preflight-new"
    ));
    let placed = f.at(&format!(
        "ssh {vm} set -e\ncp '/srv/emp/empyrean-server.preflight-new'"
    ));
    assert!(kept < copied && copied < placed);
    assert_eq!(
        f.at_remote(&at("/srv/emp/empyrean-server")),
        Some("empyrean 0.3.2-rc.1 for linux"),
        "the zigbuild's server, not this machine's"
    );
    assert_eq!(
        f.at_remote(&at("/srv/emp/empyrean-server.preflight-new")),
        None
    );
    assert_eq!(
        f.at_remote(&at(&format!("{b}/db/shard.db-wal"))),
        Some("the shard's log")
    );
    assert_eq!(
        f.at_remote(&at("/srv/emp/empyrean.toml")),
        Some(
            EMP_TOML
                .replace("wss://hub.dereth.network", "wss://dev-hub.example.org")
                .as_str()
        )
    );
    assert!(
        f.sent
            .iter()
            .all(|(_, c)| !c.contains("cargo") && !c.contains("git")),
        "nothing is built on a world's machine: {:#?}",
        f.sent
    );

    // The world runs a while: its database migrates, and files are made that were not there.
    for (p, t) in [
        ("/srv/emp/shard.db", "migrated"),
        ("/srv/emp/shard.db-wal", "the migration's log"),
        ("/srv/emp/shard.db-shm", "made since"),
        ("/srv/emp/auth.db", "made since"),
    ] {
        m.inner.remote.insert(at(p), t.into());
    }
    let mut l = ledger(&mut m);
    assert_eq!(
        l.worlds
            .iter()
            .find(|w| w.name == "vm")
            .map(|w| w.databases.clone()),
        Some(vec![
            "/srv/emp/shard.db".to_owned(),
            "/srv/emp/auth.db".to_owned()
        ])
    );
    let before = m.inner.sent.len();
    steps::rollback(&mut m, &cfg(), &mut l).expect("rolled back");
    let f = &m.inner;
    let restore = format!(
        "set -e\ncp -a '{b}/replaced' '/srv/emp/empyrean-server'\ncp -a '{b}/config' '/srv/emp/empyrean.toml'\n{}",
        dbs.iter()
            .map(|d| format!("if [ -e '{b}/db/{d}' ]; then cp -a '{b}/db/{d}' '/srv/emp/{d}'; else rm -f '/srv/emp/{d}'; fi\n"))
            .collect::<String>()
    );
    let removed = format!("set -e\nrm -rf '{b}'\n");
    assert_eq!(
        f.sent[before..]
            .iter()
            .map(|(_, c)| c.as_str())
            .collect::<Vec<_>>(),
        [
            "sudo -n systemctl stop empyrean",
            restore.as_str(),
            "sudo -n systemctl start empyrean",
            removed.as_str(),
        ],
        "the backup removed once the world runs"
    );
    // Everything as it was before the preflight, the log included; what was made since gone.
    for (p, t) in [
        ("/srv/emp/empyrean-server", Some("empyrean 0.3.1 for linux")),
        ("/srv/emp/empyrean.toml", Some(EMP_TOML)),
        ("/srv/emp/shard.db", Some("the shard")),
        ("/srv/emp/shard.db-wal", Some("the shard's log")),
        ("/srv/emp/shard.db-shm", None),
        ("/srv/emp/auth.db", None),
    ] {
        assert_eq!(f.at_remote(&at(p)), t, "{p}");
    }
    assert!(!f.on(vm, b), "the backup is gone");
    assert!(l.worlds.is_empty());
}

#[test]
fn missing_build_tools_stop_a_run_before_anything_is_built_or_stopped() {
    let refused = |f: Fake| {
        let mut m = counting(f);
        let e =
            preflight(&mut m, &[Product::Hub, Product::Empyrean], &["vm"]).expect_err("refused");
        let f = m.inner;
        assert!(
            f.record().iter().all(|l| l.starts_with("probe ")),
            "only asked: {:#?}",
            f.record()
        );
        e
    };
    let e = refused(fake().reply(
        "cargo-zigbuild --version",
        Err("error: no such command: `zigbuild`"),
    ));
    assert!(
        e.contains("cargo zigbuild: `cargo install --locked cargo-zigbuild`")
            && e.contains("Nothing was built or stopped"),
        "{e}"
    );
    let e = refused(
        fake()
            .reply("zig version", Err("not found"))
            .reply("python3 -m ziglang version", Err("No module named ziglang")),
    );
    assert!(e.contains("zig, which cargo zigbuild runs"), "{e}");
    let e = refused(fake().reply(
        "rustup target list --installed",
        Ok("x86_64-pc-windows-msvc\n"),
    ));
    assert!(
        e.contains("`rustup target add x86_64-unknown-linux-gnu`"),
        "{e}"
    );

    // Zig from Python's package is zig enough.
    let mut m = counting(
        fake()
            .reply("zig version", Err("not found"))
            .there("owner@vm.example.org:/srv/emp/empyrean.toml", EMP_TOML),
    );
    preflight(&mut m, &[Product::Hub, Product::Empyrean], &["vm"]).expect("preflighted");
    // With Empyrean unchanged, nothing is built for Linux, and nothing asked.
    let mut m = counting(
        fake()
            .reply("cargo-zigbuild --version", Err("no zigbuild"))
            .there("owner@vm.example.org:/srv/emp/empyrean.toml", EMP_TOML),
    );
    preflight(&mut m, &[Product::Hub], &["vm"]).expect("preflighted");
    assert!(m.inner.done("probe").is_empty());
}

#[test]
fn the_ace_mod_needs_the_dotnet_10_sdk_here_asked_before_anything_is_built_or_stopped() {
    let mut m = counting(fake().reply(
        "dotnet --list-sdks",
        Ok("8.0.414 [C:\\Program Files\\dotnet\\sdk]\n"),
    ));
    let e = preflight(&mut m, &[Product::Ace], &[]).expect_err("refused");
    assert!(
        e.contains("the .NET 10 SDK, which builds the ACE mod")
            && e.contains("Nothing was built or stopped"),
        "{e}"
    );
    assert!(
        m.inner.record().iter().all(|l| l.starts_with("probe ")),
        "only asked: {:#?}",
        m.inner.record()
    );
    // Without the mod in the run, the SDK is not asked about.
    let mut m = counting(fake().reply("dotnet --list-sdks", Ok("")));
    preflight(&mut m, &[Product::Hub], &[]).expect("preflighted");
    assert!(m.inner.done("probe dotnet").is_empty());
}

#[test]
fn an_ace_worlds_mods_folder_may_sit_beside_its_server() {
    let beside = Config::parse(&REMOTE.replace(
        "dir = 'D:\\Games\\ACE Server\\Server'\n",
        "dir = 'D:\\Games\\ACE Server\\Server'\nmods_dir = 'D:\\Games\\ACE Server\\Mods'\n",
    ))
    .expect("parses");
    let w = beside
        .worlds
        .iter()
        .find(|w| w.name == "box-ace")
        .expect("the ACE world");
    assert_eq!(w.mod_dir(), "D:\\Games\\ACE Server\\Mods\\Dereth.Network");
    assert_eq!(
        w.config_file(),
        "D:\\Games\\ACE Server\\Mods\\Dereth.Network\\Settings.json"
    );
    let usual = remote_cfg();
    let w = usual
        .worlds
        .iter()
        .find(|w| w.name == "box-ace")
        .expect("the ACE world");
    assert_eq!(
        w.mod_dir(),
        "D:\\Games\\ACE Server\\Server\\Mods\\Dereth.Network"
    );
}

#[test]
fn a_worlds_file_is_named_by_either_slash_whatever_this_machine_uses() {
    assert_eq!(steps::last_part("D:\\worlds\\emp\\shard.db"), "shard.db");
    assert_eq!(
        steps::last_part("D:\\worlds\\emp/db/auth.db-wal"),
        "auth.db-wal"
    );
    assert_eq!(steps::last_part("/srv/emp/shard.db-shm"), "shard.db-shm");
    assert_eq!(steps::last_part("shard.db"), "shard.db");
}

#[test]
fn the_mod_is_not_built_against_a_folder_with_no_ace_server_in_it() {
    let mut f = fake();
    f.remove(Path::new("D:/ACE/Server/ACE.Server.dll"))
        .expect("removed");
    let mut m = counting(f);
    let e = preflight(&mut m, &[Product::Ace], &[]).expect_err("refused");
    assert!(
        e.contains("ace: there is no ACE.Server.dll in D:")
            && e.contains("the world's `dir` is the folder that holds its server's ACE.Server.dll"),
        "{e}"
    );
    assert!(m.inner.done("dotnet build").is_empty());
}

#[test]
fn a_failed_build_says_what_the_build_said_and_keeps_its_worktree() {
    let msbuild = "  Determining projects to restore...\n\
                   D:\\preflight\\src\\x.csproj : error NETSDK1045: The current .NET SDK does not support targeting .NET 10.0.\n\
                   D:\\preflight\\src\\x.csproj : error NETSDK1045: The current .NET SDK does not support targeting .NET 10.0.\n\
                   Build FAILED.\n";
    let text = machine::failure_text("", msbuild);
    assert_eq!(
        text,
        "D:\\preflight\\src\\x.csproj : error NETSDK1045: The current .NET SDK does not support targeting .NET 10.0."
    );
    assert_eq!(
        machine::failure_text("", "one\ntwo\n"),
        "one\ntwo",
        "with no error line, the end of the output"
    );
    assert_eq!(machine::failure_text("fatal: no", ""), "fatal: no");

    let mut m = counting(fake().reply("dotnet build", Err("dotnet build failed: error CS0246")));
    let e = preflight(&mut m, &[Product::Ace], &[]).expect_err("the mod's build fails");
    assert!(
        e.contains("error CS0246") && e.contains("is kept at D:"),
        "{e}"
    );
    assert_eq!(
        m.inner.done("git worktree remove").len(),
        1,
        "only a left-behind worktree is removed, before the build"
    );
}

#[test]
fn the_command_a_windows_machine_is_sent_is_powershell_given_the_script_encoded() {
    // As PowerShell itself encodes it: UTF-16, in base64.
    assert_eq!(
        super::machine::powershell("Get-Item -LiteralPath 'C:\\A B'"),
        "powershell -NoProfile -NonInteractive -EncodedCommand RwBlAHQALQBJAHQAZQBtACAALQBMAGkAdABlAHIAYQBsAFAAYQB0AGgAIAAnAEMAOgBcAEEAIABCACcA"
    );
    let script = "# x\r\nCopy-Item -LiteralPath 'C:\\Bob''s Wörld\\a b' | Out-Null; \"$HOME\" & echo %PATH%\n";
    assert_eq!(
        super::machine::script_of(&super::machine::powershell(script)).as_deref(),
        Some(script)
    );
    assert_eq!(super::machine::script_of("sudo -n systemctl stop x"), None);
    // PowerShell given an encoded command reports its errors as CLIXML; they read as lines.
    let clixml = "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><Obj S=\"progress\" RefId=\"0\"><MS><PR N=\"Record\"><AV>Preparing modules for first use.</AV></PR></MS></Obj><S S=\"Error\">Copy-Item : Cannot find path 'C:\\a &amp; b' because it does not exist._x000D__x000A_</S><S S=\"Error\">At line:3 char:1_x000D__x000A_</S></Objs>";
    assert_eq!(
        super::machine::plain_errors(clixml),
        "Copy-Item : Cannot find path 'C:\\a & b' because it does not exist.\nAt line:3 char:1"
    );
    assert_eq!(
        super::machine::plain_errors(" sudo: a password is required\n"),
        "sudo: a password is required"
    );

    let (m, r) = remote_preflight(remote_fake());
    r.expect("preflighted");
    let first = m
        .inner
        .sent
        .iter()
        .find(|(h, _)| h == "owner@world-box")
        .map(|(_, c)| c.clone())
        .expect("sent");
    assert_eq!(
        first,
        super::machine::powershell(m.inner.scripts_on("owner@world-box")[0]),
        "each script is sent as it is encoded"
    );
}

/// A first preflight of every product on [`REMOTE`].
fn remote_preflight(f: Fake) -> (Counting, Result<(), String>) {
    let mut m = counting(f);
    let r = preflight_on(&mut m, &remote_cfg());
    (m, r)
}

#[test]
fn an_empyrean_world_on_another_windows_machine_is_stopped_backed_up_given_the_build_made_here_and_started(
) {
    let (mut m, r) = remote_preflight(remote_fake());
    r.expect("preflighted");
    let f = &m.inner;
    let scripts: Vec<&str> = f
        .scripts_on("owner@world-box")
        .into_iter()
        .filter(|s| s.starts_with("# box-emp "))
        .collect();
    assert_eq!(scripts, box_emp_preflight_scripts());
    // Its configuration down, the build and the edited configuration up, between the scripts.
    let staged = f.at("ssh owner@world-box # box-emp on world-box: stopped");
    let fetched = f.at("fetch owner@world-box:.dereth-preflight/box-emp/config D:/preflight/remote/box-emp/empyrean.toml");
    let kept = f.at("ssh owner@world-box # box-emp on world-box: its files as they are");
    let build = f.at(&format!(
        "send D:/preflight/builds/empyrean-0.3.2-rc.1/{} owner@world-box:.dereth-preflight/box-emp/build",
        exe("empyrean-server")
    ));
    let config = f.at("send D:/preflight/remote/box-emp/empyrean.toml owner@world-box:.dereth-preflight/box-emp/config");
    let placed = f.at(
        "ssh owner@world-box # box-emp on world-box: the build and its configuration put in place",
    );
    assert!(
        staged < fetched && fetched < kept && kept < build && build < config && config < placed
    );
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-emp/build"),
        Some("empyrean 0.3.2-rc.1"),
        "this machine's build"
    );
    // Pointed at the dev hub's public address, which the tunnel carries, not where it listens.
    let config = f
        .at_remote("owner@world-box:.dereth-preflight/box-emp/config")
        .expect("sent");
    assert_eq!(
        config,
        EMP_TOML.replace("wss://hub.dereth.network", "wss://dev-hub.example.org")
    );
    assert!(!config.contains("127.0.0.1"));
    assert!(f.done("service").is_empty(), "no service here");
    assert!(f.done("cargo zigbuild").len() == 1);

    let l = ledger(&mut m);
    let w = &l.worlds[0];
    assert_eq!(
        (
            w.name.as_str(),
            w.original_hub.as_deref(),
            w.version.as_deref(),
            w.backup.as_str()
        ),
        (
            "box-emp",
            Some("wss://hub.dereth.network"),
            Some("0.3.2-rc.1"),
            "C:\\Worlds\\Bob's Empyrean Dev\\.preflight-backup"
        )
    );
    assert_eq!(
        w.databases,
        [
            "C:\\Worlds\\Bob's Empyrean Dev\\shard.db",
            "C:\\Worlds\\Bob's Empyrean Dev\\auth.db"
        ]
    );
    let summary = steps::summary(&remote_cfg(), &l);
    assert!(
        summary.contains("box-emp: runs 0.3.2-rc.1")
            && summary.contains(".preflight-backup on world-box"),
        "{summary}"
    );

    // A second preflight keeps the first backup.
    m.counts = vec![String::new(), String::new()];
    preflight_on(&mut m, &remote_cfg()).expect("again");
    assert_eq!(
        m.inner
            .scripts_on("owner@world-box")
            .iter()
            .filter(|s| s.starts_with("# box-emp on world-box: its files as they are"))
            .count(),
        1
    );
}

#[test]
fn each_ace_world_takes_the_mod_built_against_its_own_ace_server() {
    let setup = Config::parse(&format!("{REMOTE}{LOCAL_ACE}")).expect("parses");
    let mut m = counting(remote_fake());
    preflight_on(&mut m, &setup).expect("preflighted");
    let f = &m.inner;
    // The other machine's server's assemblies, copied here first.
    assert_eq!(
        f.scripts_on("owner@world-box")[0],
        format!(
            "# box-ace on world-box: its ACE server's assemblies staged, for the mod's build
{SETTINGS}if (-not (Test-Path -LiteralPath (Join-Path 'D:\\Games\\ACE Server\\Server' 'ACE.Server.dll'))) {{ throw ('There is no ACE.Server.dll in ' + 'D:\\Games\\ACE Server\\Server' + ': the world''s dir is the folder that holds it.') }}
$stage = Join-Path $HOME '.dereth-preflight\\box-ace\\ace'
if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Get-ChildItem -LiteralPath 'D:\\Games\\ACE Server\\Server' -Filter '*.dll' -File | Copy-Item -Destination $stage
"
        )
    );
    let fetched =
        f.at("fetch owner@world-box:.dereth-preflight/box-ace/ace D:/preflight/ace/box-ace");
    assert_eq!(
        f.done("dotnet build"),
        [
            "dotnet build D:/preflight/src/network/ace-mod/Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=D:/ACE/Server -o D:/preflight/builds/network-ace-0.1.3-rc.1-ace/Dereth.Network",
            "dotnet build D:/preflight/src/network/ace-mod/Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=D:/preflight/ace/box-ace -o D:/preflight/builds/network-ace-0.1.3-rc.1-box-ace/Dereth.Network",
        ]
    );
    assert!(fetched < f.at("dotnet build"));
    // Each world runs the one built against its own server.
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-ace/build/Dereth.Network.dll"),
        Some("mod built against D:/preflight/ace/box-ace")
    );
    assert_eq!(
        f.get("D:/ACE/Server/Mods/Dereth.Network/Dereth.Network.dll"),
        Some("mod built against D:/ACE/Server")
    );
    // The mod's build copied over its folder there, keeping its settings; then started.
    let place = f
        .scripts_on("owner@world-box")
        .into_iter()
        .find(|s| s.starts_with("# box-ace on world-box: the build"))
        .expect("placed")
        .to_owned();
    let start = START_EMPYREAN.replace("'EmpyreanDev'", "'ACEDev'");
    assert_eq!(
        place,
        format!(
            "# box-ace on world-box: the build and its configuration put in place, and started
{SETTINGS}$stage = Join-Path $HOME '.dereth-preflight\\box-ace'
New-Item -ItemType Directory -Force -Path '{BOX_MOD}' | Out-Null
Get-ChildItem -LiteralPath (Join-Path $stage 'build') | Copy-Item -Destination '{BOX_MOD}' -Recurse -Force
Copy-Item -LiteralPath (Join-Path $stage 'config') -Destination '{BOX_MOD}\\Settings.json' -Force
Remove-Item -LiteralPath $stage -Recurse -Force
{start}"
        )
    );
    let settings: serde_json::Value = serde_json::from_str(
        f.at_remote("owner@world-box:.dereth-preflight/box-ace/config")
            .expect("sent"),
    )
    .expect("json");
    assert_eq!(settings["HubUrl"], "wss://dev-hub.example.org");
    assert_eq!(settings["Slug"], "acedev");
    // Its backup holds the mod's folder and settings, and no database.
    let kept = f
        .scripts_on("owner@world-box")
        .into_iter()
        .find(|s| s.starts_with("# box-ace on world-box: its files"))
        .expect("kept")
        .to_owned();
    assert!(
        kept.contains(&format!(
            "Copy-Item -LiteralPath '{BOX_MOD}' -Destination (Join-Path $backup 'replaced') -Recurse"
        )) && !kept.contains("foreach"),
        "{kept}"
    );
}

#[test]
fn a_setup_with_nothing_here_drives_a_windows_machine_and_a_linux_one_and_builds_nothing_there() {
    let (mut m, r) = remote_preflight(remote_fake());
    r.expect("preflighted");
    let f = &m.inner;
    assert!(f.done("service").is_empty(), "nothing here is touched");
    assert_eq!(
        f.done("cargo zigbuild"),
        ["cargo zigbuild --release -p empyrean-server --target x86_64-unknown-linux-gnu.2.28 [CARGO_TARGET_DIR=D:/preflight/target]"],
        "the glibc floor given as zigbuild spells it"
    );
    assert!(f
        .done("copy D:/preflight/target/x86_64-unknown-linux-gnu/release/empyrean-server D:/preflight/builds/empyrean-0.3.2-rc.1-x86_64-unknown-linux-gnu.2.28/empyrean-server")
        .len()
        == 1);
    assert_eq!(
        f.at_remote("owner@world-vm:/srv/empyrean/empyrean-server"),
        Some("empyrean 0.3.2-rc.1 for linux")
    );
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-emp/build"),
        Some("empyrean 0.3.2-rc.1")
    );
    for (host, command) in &f.sent {
        let shown = super::machine::readable(command);
        for never in ["cargo", "dotnet", "git ", "zig"] {
            assert!(
                !shown.contains(never),
                "nothing is built on {host}: {shown}"
            );
        }
    }
    // The Linux world over systemd, the Windows ones over PowerShell.
    assert_eq!(
        f.done("ssh owner@world-vm sudo"),
        [
            "ssh owner@world-vm sudo -n systemctl stop empyrean",
            "ssh owner@world-vm sudo -n systemctl start empyrean"
        ]
    );
    assert_eq!(f.scripts_on("owner@world-vm"), [] as [&str; 0]);
    let labels: Vec<&str> = f
        .scripts_on("owner@world-box")
        .into_iter()
        .map(|s| s.lines().next().unwrap_or_default())
        .collect();
    assert_eq!(
        labels,
        [
            "# box-ace on world-box: its ACE server's assemblies staged, for the mod's build",
            "# box-emp on world-box: stopped, and its configuration staged",
            "# box-emp on world-box: its files as they are, kept beside it",
            "# box-emp on world-box: the build and its configuration put in place, and started",
            "# box-ace on world-box: stopped, and its configuration staged",
            "# box-ace on world-box: its files as they are, kept beside it",
            "# box-ace on world-box: the build and its configuration put in place, and started",
        ]
    );
    // The dev hub stays here, reached through the tunnel's address.
    assert_eq!(f.done("spawn ").len(), 1, "the dev hub, here");
    for at in [
        "owner@world-box:.dereth-preflight/box-emp/config",
        "owner@world-vm:/srv/empyrean/empyrean.toml",
    ] {
        let text = f.at_remote(at).expect("sent");
        assert!(
            text.contains("hub_url = \"wss://dev-hub.example.org\"") && !text.contains("127.0.0.1"),
            "{at}: {text}"
        );
    }

    // Rolled back, the last first: the Linux world, then each Windows one from its backup there.
    let mut l = ledger(&mut m);
    let scripts_before = m.inner.scripts.len();
    steps::rollback(&mut m, &remote_cfg(), &mut l).expect("rolled back");
    let f = &m.inner;
    let restores: Vec<String> = f.scripts[scripts_before..]
        .iter()
        .map(|(_, s)| s.clone())
        .collect();
    let mod_restore = format!(
        "# box-ace on world-box: put back as it was before the preflight
{SETTINGS}{stop}$backup = 'D:\\Games\\ACE Server\\Server\\.preflight-backup'
if (Test-Path -LiteralPath '{BOX_MOD}') {{ Remove-Item -LiteralPath '{BOX_MOD}' -Recurse -Force }}
Copy-Item -LiteralPath (Join-Path $backup 'replaced') -Destination '{BOX_MOD}' -Recurse -Force
Copy-Item -LiteralPath (Join-Path $backup 'config') -Destination '{BOX_MOD}\\Settings.json' -Force
{start}Remove-Item -LiteralPath $backup -Recurse -Force
",
        stop = STOP_EMPYREAN.replace("'EmpyreanDev'", "'ACEDev'"),
        start = START_EMPYREAN.replace("'EmpyreanDev'", "'ACEDev'"),
    );
    assert_eq!(restores, [mod_restore, box_emp_restore_script()]);
    assert!(
        f.at("ssh owner@world-vm set -e\ncp -a '/srv/empyrean/.preflight-backup/replaced'")
            < f.last("ssh owner@world-box # box-ace on world-box: put back")
    );
    assert_eq!(l.stage, Stage::Idle);
    assert!(l.worlds.is_empty());
}

#[test]
fn a_run_cut_short_partway_through_a_world_on_another_machine_is_rolled_back_from_its_backup() {
    // The build is in place there, but the world does not start.
    let (mut m, r) = remote_preflight(remote_fake().reply(
        "ssh owner@world-box # box-emp on world-box: the build and its configuration put in place",
        Err("EmpyreanDev did not start within a minute"),
    ));
    let e = r.expect_err("stopped");
    assert!(
        e.contains("box-emp: EmpyreanDev did not start") && e.contains("rollback"),
        "{e}"
    );
    let mut l = ledger(&mut m);
    assert_eq!(
        l.worlds
            .iter()
            .map(|w| (w.name.as_str(), w.backup.as_str()))
            .collect::<Vec<_>>(),
        [(
            "box-emp",
            "C:\\Worlds\\Bob's Empyrean Dev\\.preflight-backup"
        )],
        "recorded once its backup was whole"
    );
    assert!(
        m.inner
            .scripts_on("owner@world-box")
            .iter()
            .all(|s| !s.starts_with("# box-ace on world-box: stopped")),
        "the next world untouched"
    );
    steps::rollback(&mut m, &remote_cfg(), &mut l).expect("rolled back");
    assert_eq!(
        m.inner.scripts_on("owner@world-box").last().copied(),
        Some(box_emp_restore_script().as_str())
    );
    assert!(l.worlds.is_empty() && l.stage == Stage::Idle);

    // Stopped before its backup was whole: nothing to put back, so it is not recorded.
    let (mut m, r) = remote_preflight(remote_fake().reply(
        "ssh owner@world-box # box-emp on world-box: stopped",
        Err("access denied"),
    ));
    r.expect_err("stopped");
    let mut l = ledger(&mut m);
    assert!(l.worlds.is_empty());
    let before = m.inner.scripts.len();
    steps::rollback(&mut m, &remote_cfg(), &mut l).expect("rolled back");
    assert_eq!(m.inner.scripts.len(), before, "nothing sent");
}

#[test]
fn finish_copies_the_final_builds_to_the_other_machines_and_points_each_world_back() {
    let (mut m, r) = remote_preflight(remote_fake());
    r.expect("preflighted");
    let mut l = ledger(&mut m);
    l.stage = Stage::Shipped;
    let made = [
        (
            format!(
                "D:/preflight/builds/empyrean-0.3.2/{}",
                exe("empyrean-server")
            ),
            "empyrean 0.3.2",
        ),
        (
            "D:/preflight/builds/empyrean-0.3.2-x86_64-unknown-linux-gnu.2.28/empyrean-server"
                .to_owned(),
            "empyrean 0.3.2 for linux",
        ),
        (
            "D:/preflight/builds/network-ace-0.1.3-box-ace/Dereth.Network/Dereth.Network.dll"
                .to_owned(),
            "mod 0.1.3",
        ),
    ];
    for (path, text) in &made {
        m.inner.files.insert(path.clone(), (*text).to_owned());
    }
    let build = |version: &str, path: &str| steps::Build {
        version: version.to_owned(),
        path: Path::new(path).to_path_buf(),
    };
    let mut builds = steps::Builds::default();
    builds.empyrean.insert(None, build("0.3.2", &made[0].0));
    builds.empyrean.insert(
        Some("x86_64-unknown-linux-gnu.2.28".to_owned()),
        build("0.3.2", &made[1].0),
    );
    builds.mods.insert(
        "box-ace".to_owned(),
        build(
            "0.1.3",
            "D:/preflight/builds/network-ace-0.1.3-box-ace/Dereth.Network",
        ),
    );
    let scripts_before = m.inner.scripts.len();
    let sent_before = m.inner.sent.len();
    steps::finish(&mut m, &remote_cfg(), &builds, &mut l).expect("finished");
    let f = &m.inner;
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-emp/build"),
        Some("empyrean 0.3.2")
    );
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-emp/config"),
        Some(EMP_TOML),
        "its own hub again"
    );
    assert_eq!(
        f.at_remote("owner@world-box:.dereth-preflight/box-ace/build/Dereth.Network.dll"),
        Some("mod 0.1.3")
    );
    let settings: serde_json::Value = serde_json::from_str(
        f.at_remote("owner@world-box:.dereth-preflight/box-ace/config")
            .expect("sent"),
    )
    .expect("json");
    assert!(settings.get("HubUrl").is_none(), "{settings}");
    assert_eq!(
        f.at_remote("owner@world-vm:/srv/empyrean/empyrean-server"),
        Some("empyrean 0.3.2 for linux")
    );
    assert_eq!(
        f.at_remote("owner@world-vm:/srv/empyrean/empyrean.toml"),
        Some(EMP_TOML)
    );
    let labels: Vec<&str> = f.scripts[scripts_before..]
        .iter()
        .map(|(_, s)| s.lines().next().unwrap_or_default())
        .collect();
    assert_eq!(
        labels,
        [
            "# box-emp on world-box: stopped, and its configuration staged",
            "# box-emp on world-box: the build and its configuration put in place, and started",
            "# box-ace on world-box: stopped, and its configuration staged",
            "# box-ace on world-box: the build and its configuration put in place, and started",
        ],
        "no backup taken again"
    );
    assert!(f.sent[sent_before..]
        .iter()
        .all(|(_, c)| !super::machine::readable(c).contains("cargo")));
    assert_eq!(l.stage, Stage::Idle);
    assert!(l.worlds.is_empty());
}

#[test]
fn the_setup_writes_a_script_for_each_machine() {
    let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample owner@preflight";
    let scripts = setup::scripts(&remote_cfg(), "D:\\p.toml", Some(key)).expect("scripts");
    let files: Vec<&str> = scripts.iter().map(|s| s.file.as_str()).collect();
    assert_eq!(
        files,
        ["setup.ps1", "setup-world-box.ps1", "setup-world-vm.sh"]
    );
    let text = |f: &str| {
        scripts
            .iter()
            .find(|s| s.file == f)
            .map(|s| s.text.clone())
            .expect("a script")
    };

    // This machine: the tools that build for the Linux machine, and no world's service.
    let here = text("setup.ps1");
    for want in [
        "winget install --exact --id zig.zig",
        "cargo install --locked cargo-zigbuild",
        "rustup target add 'x86_64-unknown-linux-gnu'",
        "winget install --exact --id Microsoft.DotNet.SDK.10",
        "# world-box: run setup-world-box.ps1 there, once.",
        "# world-vm: run setup-world-vm.sh there, once.",
    ] {
        assert!(here.contains(want), "{want}\n{here}");
    }
    assert!(!here.contains("Grant-StartStop '"), "{here}");

    // The Windows machine: its ssh server, this machine's key, and its worlds' services.
    let windows = text("setup-world-box.ps1");
    for want in [
        "param([string]$Account = 'owner')",
        "Add-WindowsCapability -Online -Name $capability.Name",
        "Set-Service -Name sshd -StartupType Automatic",
        "New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP'",
        &format!("$key = '{key}'"),
        "ssh\\administrators_authorized_keys",
        ".ssh\\authorized_keys",
        "icacls $keys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F'",
        "Grant-StartStop 'EmpyreanDev'",
        "Grant-StartStop 'ACEDev'",
        "nssm install 'ACEDev' 'dotnet' 'ACE.Server.dll'",
        "nssm set 'ACEDev' AppDirectory 'D:\\Games\\ACE Server\\Server'",
        "nssm set 'ACEDev' DependOnService 'MariaDB'",
        "$mariadbd = Join-Path 'D:\\Games\\ACE Server\\mariadb' 'bin\\mariadbd.exe'",
        "throw 'MariaDB is running in a window: stop it, then run this again.'",
        "nssm install 'MariaDB' $mariadbd",
        "Set-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\MariaDB\\Parameters' -Name AppParameters -Value '--defaults-file=\"D:\\Games\\ACE Server\\my.ini\" --console'",
        "$dbHome = Split-Path 'D:\\Games\\ACE Server\\my.ini' -Parent",
        "nssm set 'MariaDB' AppDirectory $dbHome",
        "nssm set 'MariaDB' AppStdout (Join-Path $dbHome 'mariadb.log')",
        "nssm set 'MariaDB' AppStderr (Join-Path $dbHome 'mariadb.log')",
        "nssm set 'MariaDB' AppStopMethodConsole 60000",
        "Start-Service -Name 'MariaDB'",
        "(Get-Content 'D:\\keys\\ace.key' -Raw)",
        "(A;;RPWPLCLORC;;;$sid)",
    ] {
        assert!(windows.contains(want), "{want}\n{windows}");
    }
    assert!(!windows.contains("systemctl") && !windows.contains("cloudflared"));
    // The account may change each world's folder; an ACE world's mod folder and settings too.
    let check = "if ($LASTEXITCODE -ne 0) { throw \"icacls failed ($LASTEXITCODE)\" }";
    for block in [
        format!(
            "Grant-StartStop 'EmpyreanDev'\nicacls '{BOX_EMP}' /grant \"${{Account}}:(OI)(CI)M\"\n{check}\n"
        ),
        format!(
            "Grant-StartStop 'ACEDev'\nicacls 'D:\\Games\\ACE Server\\Server' /grant \"${{Account}}:(OI)(CI)M\"\n{check}\nicacls '{BOX_MOD}' /grant \"${{Account}}:(OI)(CI)M\"\n{check}\nicacls '{BOX_MOD}\\Settings.json' /grant \"${{Account}}:M\"\n{check}\n"
        ),
    ] {
        assert!(windows.contains(&block), "{block}\n{windows}");
    }

    // The Linux machine: the sudoers line, checked before it is put in place, and the account's
    // write access to the world's folder, and to what the world writes there later.
    let setfacl = "command -v setfacl >/dev/null || { echo \"setfacl is missing: install the acl package, then run this again\" >&2; exit 1; }\n";
    let folder = "setfacl -R -m \"u:$account:rwX\" '/srv/empyrean'\nsetfacl -R -d -m \"u:$account:rwX\" '/srv/empyrean'\n";
    let linux = text("setup-world-vm.sh");
    assert!(linux.contains(setfacl) && linux.contains(folder), "{linux}");
    assert!(
        linux.find(setfacl) < linux.find("printf "),
        "setfacl is there before anything is changed"
    );
    for want in [
        "account=\"${1:-owner}\"",
        "printf '%s ALL=(root) NOPASSWD: %s\\n' \"$account\" \"$systemctl stop empyrean, $systemctl start empyrean\" > \"$rule.new\"",
        "visudo -cf \"$rule.new\"",
        "mv \"$rule.new\" \"$rule\"",
    ] {
        assert!(linux.contains(want), "{want}\n{linux}");
    }
    assert!(linux.find("visudo") < linux.find("mv "));

    // The key must be a public key, and there must be one for a Windows machine.
    assert!(setup::scripts(&remote_cfg(), "D:\\p.toml", None).is_err());
    assert_eq!(setup::public_key(&format!("{key}\n")).as_deref(), Ok(key));
    for private in [
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----\n",
        "-----BEGIN OPENSSH PRIVATE KEY----- b3BlbnNzaC1rZXktdjEAAAAA -----END OPENSSH PRIVATE KEY-----",
    ] {
        assert_eq!(
            setup::public_key(private),
            Err("this is a private key; give its .pub file".to_owned())
        );
    }
    assert!(setup::public_key("hello").is_err());
    assert!(
        setup::public_key(&format!("{key}\n{key}")).is_err(),
        "one key"
    );
}

#[test]
fn a_dry_run_shows_the_steps_on_another_windows_machine_and_changes_nothing() {
    // Only a Windows machine: a Linux one would have the dry run ask this machine for zigbuild.
    let setup = Config::parse(
        &REMOTE
            .replace("D:\\preflight", "Z:\\no-such-preflight\\work")
            .split("[[world]]\nname = \"infiltration\"")
            .next()
            .expect("the setup")
            .replace(
                "[[machine]]\nname = \"world-vm\"\nssh = \"owner@world-vm\"\nos = \"linux\"\ntarget = \"x86_64-unknown-linux-gnu.2.28\"\n",
                "",
            ),
    )
    .expect("parses");
    assert!(setup
        .worlds
        .iter()
        .all(|w| matches!(w.place(), Place::Windows(_))));
    run(
        &mut super::machine::DryRun,
        &setup,
        Path::new("Z:/no-such-repo"),
        &repo(&[Product::Hub, Product::Ace, Product::Empyrean]),
        &Options {
            with: Vec::new(),
            dry_run: true,
        },
    )
    .expect("a dry run");
}
