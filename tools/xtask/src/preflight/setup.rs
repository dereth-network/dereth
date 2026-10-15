//! The one-time setup, as one script per machine, written from the setup file for the owner to
//! read and run once on that machine as an administrator (or root):
//! - **this machine** (`setup.ps1`): the account's right to start and stop each world's service
//!   here and to change its folder (so the preflight never needs to be an administrator), an ACE
//!   world's service with its database as its dependency, the tools that build Empyrean for the
//!   Linux machines, the tunnel and its address, and the dev Discord application's secret files,
//!   readable by the account alone;
//! - **each Windows world machine** (`setup-<machine>.ps1`): its OpenSSH server, this machine's
//!   key authorized for the account the preflight logs in as, and that account's right to start
//!   and stop its worlds' services and to change their folders, an ACE world's made as here;
//! - **each Linux world machine** (`setup-<machine>.sh`): the sudoers line that lets that account
//!   stop and start its worlds' units, and nothing more, and its write access to their folders.
//!
//! The repository holds no script; these are written for the machines the setup file names.

use super::config::{Config, Host, Kind, Os, Place, World};
use super::linux::sh;
use super::steps::rust_target;
use super::windows::ps;

/// A setup script: its file, where and how it is run, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub file: String,
    pub run: String,
    pub text: String,
}

/// The host of an address: `https://dev-hub.example.org/` is `dev-hub.example.org`.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', ':']).next().unwrap_or(rest)
}

/// The public key `text` (an ssh `.pub` file), checked to be one public key and nothing else.
pub fn public_key(text: &str) -> Result<String, String> {
    let key = text.trim();
    if key.contains("PRIVATE KEY") {
        return Err("this is a private key; give its .pub file".to_owned());
    }
    let kind = key.split_whitespace().next().unwrap_or_default();
    if key.lines().count() != 1
        || !(kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.starts_with("sk-"))
    {
        return Err("not an ssh public key (one line, `ssh-ed25519 AAAA...`)".to_owned());
    }
    Ok(key.to_owned())
}

/// Lines of a script, gathered.
#[derive(Default)]
struct Text(String);

impl Text {
    fn line(&mut self, l: &str) {
        self.0.push_str(l);
        self.0.push('\n');
    }
}

/// The PowerShell that grants the account `$sid` the right to start, stop and ask after a
/// service.
const GRANT: &str = "\
# The account may start, stop and ask after a service: one entry added to its access list.
function Grant-StartStop([string]$Service) {
  $sddl = ((sc.exe sdshow $Service) | Where-Object { $_ -match 'D:' }).Trim()
  $entry = \"(A;;RPWPLCLORC;;;$sid)\"
  if ($sddl.Contains($entry)) { Write-Host \"${Service}: $Account may already start and stop it\"; return }
  $at = $sddl.IndexOf('(', $sddl.IndexOf('D:'))
  sc.exe sdset $Service $sddl.Insert($at, $entry) | Out-Null
  Write-Host \"${Service}: $Account may start and stop it\"
}";

/// The lines that let the account change `path` (a folder, and everything in it, or a file).
fn grant_modify(t: &mut Text, path: &str, folder: bool) {
    let right = if folder { "(OI)(CI)M" } else { "M" };
    t.line(&format!(
        "icacls {} /grant \"${{Account}}:{right}\"",
        ps(path)
    ));
    t.line("if ($LASTEXITCODE -ne 0) { throw \"icacls failed ($LASTEXITCODE)\" }");
}

/// A Windows world's lines: an ACE world's service made (when it is not there yet), the account's
/// right to start and stop it, and to change its folder (and an ACE world's mod and settings,
/// wherever they are).
fn windows_world(t: &mut Text, w: &World) {
    t.line("");
    t.line(&format!("# {}", w.name));
    if w.kind == Kind::Ace {
        if let Some(db) = &w.depends_on {
            mariadb_service(t, w, db);
        }
        t.line(&format!(
            "if (-not (Get-Service -Name {} -ErrorAction SilentlyContinue)) {{",
            ps(&w.service)
        ));
        t.line(&format!(
            "  nssm install {} {} 'ACE.Server.dll'",
            ps(&w.service),
            ps(w.dotnet.as_deref().unwrap_or("dotnet"))
        ));
        t.line(&format!(
            "  nssm set {} AppDirectory {}",
            ps(&w.service),
            ps(&w.dir)
        ));
        if let Some(db) = &w.depends_on {
            t.line(&format!(
                "  nssm set {} DependOnService {}",
                ps(&w.service),
                ps(db)
            ));
        }
        if let Some(key) = &w.key_file {
            t.line(&format!(
                "  nssm set {} AppEnvironmentExtra (\"DERETH_NETWORK_KEY=\" + (Get-Content {} -Raw).Trim())",
                ps(&w.service),
                ps(key)
            ));
        }
        t.line("}");
    }
    t.line(&format!("Grant-StartStop {}", ps(&w.service)));
    grant_modify(t, &w.dir, true);
    if w.kind == Kind::Ace {
        grant_modify(t, &w.mod_dir(), true);
        grant_modify(t, &w.config_file(), false);
    }
}

/// The service an ACE world waits for (`db`), made with nssm from its MariaDB folder when it is
/// not there yet: `mariadbd --defaults-file=<ini> --console`, run from the ini's folder, as the
/// owner runs it by hand, given a minute to shut down cleanly when stopped. Without the folder, or
/// with MariaDB running in a window, the script stops and says what to do.
fn mariadb_service(t: &mut Text, w: &World, db: &str) {
    t.line(&format!(
        "if (-not (Get-Service -Name {} -ErrorAction SilentlyContinue)) {{",
        ps(db)
    ));
    let Some(dir) = &w.mariadb_dir else {
        t.line(&format!(
            "  throw ('There is no service ' + {} + ' for ' + {} + ' to wait for: name its mariadb_dir and mariadb_ini in the setup file, or make the service yourself.')",
            ps(db),
            ps(&w.name)
        ));
        t.line("}");
        return;
    };
    t.line(&format!(
        "  $mariadbd = Join-Path {} 'bin\\mariadbd.exe'",
        ps(dir)
    ));
    t.line("  if (-not (Test-Path $mariadbd)) { throw \"There is no mariadbd.exe at $mariadbd\" }");
    t.line("  if (Get-Process -Name mariadbd, mysqld -ErrorAction SilentlyContinue) { throw 'MariaDB is running in a window: stop it, then run this again.' }");
    t.line(&format!("  nssm install {} $mariadbd", ps(db)));
    t.line("  if ($LASTEXITCODE -ne 0) { throw \"nssm install failed ($LASTEXITCODE)\" }");
    // The parameters go straight into the service's registry values, where nssm reads them: a
    // quoted path inside an argument does not survive Windows PowerShell's native quoting.
    let (params, home) = match &w.mariadb_ini {
        Some(ini) => (
            format!("--defaults-file=\"{ini}\" --console"),
            format!("Split-Path {} -Parent", ps(ini)),
        ),
        None => ("--console".to_owned(), ps(dir)),
    };
    t.line(&format!(
        "  Set-ItemProperty -Path {} -Name AppParameters -Value {}",
        ps(&format!(
            "HKLM:\\SYSTEM\\CurrentControlSet\\Services\\{db}\\Parameters"
        )),
        ps(&params)
    ));
    t.line(&format!("  $dbHome = {home}"));
    t.line(&format!("  nssm set {} AppDirectory $dbHome", ps(db)));
    // `--console` is what makes MariaDB shut down cleanly on the Ctrl+C nssm stops it with; its
    // log then goes to the console, which nssm keeps in a file.
    t.line(&format!(
        "  nssm set {} AppStdout (Join-Path $dbHome 'mariadb.log')",
        ps(db)
    ));
    t.line(&format!(
        "  nssm set {} AppStderr (Join-Path $dbHome 'mariadb.log')",
        ps(db)
    ));
    t.line(&format!("  nssm set {} AppStopMethodConsole 60000", ps(db)));
    t.line(&format!("  nssm set {} Start SERVICE_AUTO_START", ps(db)));
    t.line(&format!("  Start-Service -Name {}", ps(db)));
    t.line("}");
}

/// The scripts, for the setup file at `from`, with `key` the public key the Windows world machines
/// authorize (needed when there are any).
pub fn scripts(cfg: &Config, from: &str, key: Option<&str>) -> Result<Vec<Script>, String> {
    let mut out = vec![Script {
        file: "setup.ps1".to_owned(),
        run: "run it once here, in PowerShell as an administrator".to_owned(),
        text: here(cfg, from),
    }];
    for h in &cfg.machines {
        let worlds: Vec<&World> = cfg
            .worlds
            .iter()
            .filter(|w| w.machine.as_deref() == Some(h.name.as_str()))
            .collect();
        out.push(match h.os {
            Os::Windows => {
                let key = key.ok_or_else(|| {
                    format!("{} runs Windows: its setup authorizes this machine's public key, and there is none", h.name)
                })?;
                Script {
                    file: format!("setup-{}.ps1", h.name),
                    run: format!(
                        "run it once on {}, in PowerShell as an administrator",
                        h.name
                    ),
                    text: windows_machine(h, &worlds, from, key),
                }
            }
            Os::Linux => Script {
                file: format!("setup-{}.sh", h.name),
                run: format!("run it once on {}, as root (`sudo sh setup-{}.sh`)", h.name, h.name),
                text: linux_machine(h, &worlds, from),
            },
        });
    }
    Ok(out)
}

/// The script for this machine.
fn here(cfg: &Config, from: &str) -> String {
    let mut t = Text::default();
    t.line(&format!(
        "# The Dereth preflight's one-time setup of this machine, written by `cargo xtask preflight setup` from {from}."
    ));
    t.line("# Read it, then run it once in PowerShell as an administrator. It does nothing the preflight");
    t.line("# does not need, and running it again changes nothing more.");
    t.line("param([string]$Account = \"$env:USERDOMAIN\\$env:USERNAME\")");
    t.line("$ErrorActionPreference = 'Stop'");
    t.line("$sid = (New-Object System.Security.Principal.NTAccount($Account)).Translate([System.Security.Principal.SecurityIdentifier]).Value");
    if cfg.hub.tunnel.is_some() {
        t.line("");
        t.line("# The tunnel needs Cloudflare's login for this account: stop before changing anything without it.");
        t.line("$cert = if ($env:TUNNEL_ORIGIN_CERT) { $env:TUNNEL_ORIGIN_CERT } else { Join-Path $env:USERPROFILE '.cloudflared\\cert.pem' }");
        t.line("if (-not (Test-Path $cert)) { throw \"No Cloudflare login at ${cert}: run 'cloudflared tunnel login' as this account, pick the zone, then run this script again.\" }");
    }
    let local: Vec<&World> = cfg
        .worlds
        .iter()
        .filter(|w| w.place() == Place::Here)
        .collect();
    if !local.is_empty() {
        t.line("");
        t.line(GRANT);
        for w in local {
            windows_world(&mut t, w);
        }
    }
    let mut targets: Vec<&str> = cfg
        .worlds
        .iter()
        .filter(|w| w.kind == Kind::Empyrean)
        .filter_map(|w| match w.place() {
            Place::Linux(h) => Some(rust_target(h.target())),
            _ => None,
        })
        .collect();
    targets.sort_unstable();
    targets.dedup();
    if !targets.is_empty() {
        t.line("");
        t.line("# Empyrean for the Linux machines is built here with cargo zigbuild, which runs zig and");
        t.line("# needs each machine's target.");
        t.line("if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {");
        t.line("  winget install --exact --id zig.zig --accept-source-agreements --accept-package-agreements");
        t.line("  if ($LASTEXITCODE -ne 0) { throw 'installing zig failed' }");
        t.line("}");
        t.line("if (-not (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue)) {");
        t.line("  cargo install --locked cargo-zigbuild");
        t.line("  if ($LASTEXITCODE -ne 0) { throw 'installing cargo-zigbuild failed' }");
        t.line("}");
        for target in targets {
            t.line(&format!("rustup target add {}", ps(target)));
            t.line(&format!(
                "if ($LASTEXITCODE -ne 0) {{ throw 'adding the target {target} failed' }}"
            ));
        }
    }
    if cfg.worlds.iter().any(|w| w.kind == Kind::Ace) {
        t.line("");
        t.line("# The ACE mod is built here, for .NET 10.");
        t.line("if (-not ((Get-Command dotnet -ErrorAction SilentlyContinue) -and ((dotnet --list-sdks) -match '^10\\.'))) {");
        t.line("  winget install --exact --id Microsoft.DotNet.SDK.10 --accept-source-agreements --accept-package-agreements");
        t.line("  if ($LASTEXITCODE -ne 0) { throw 'installing the .NET 10 SDK failed' }");
        t.line("}");
    }
    if let Some(tunnel) = &cfg.hub.tunnel {
        let host = host_of(&cfg.hub.public_url);
        t.line("");
        t.line(&format!(
            "# The tunnel {tunnel}, carrying {host} to the dev hub. `cloudflared tunnel login` first, once."
        ));
        let cf = ps(&cfg.hub.cloudflared);
        t.line(&format!("$tunnels = & {cf} tunnel list"));
        t.line(
            "if ($LASTEXITCODE -ne 0) { throw \"cloudflared tunnel list failed ($LASTEXITCODE)\" }",
        );
        t.line(&format!("if (-not ($tunnels -match ' {tunnel} ')) {{"));
        t.line(&format!("  & {cf} tunnel create {}", ps(tunnel)));
        t.line("  if ($LASTEXITCODE -ne 0) { throw \"cloudflared tunnel create failed ($LASTEXITCODE)\" }");
        t.line("}");
        t.line(&format!(
            "& {cf} tunnel route dns {} {}",
            ps(tunnel),
            ps(host)
        ));
        t.line("if ($LASTEXITCODE -ne 0) { throw \"cloudflared tunnel route dns failed ($LASTEXITCODE)\" }");
    }
    let secrets: Vec<String> = [
        &cfg.hub.discord_client_secret_file,
        &cfg.hub.discord_bot_token_file,
    ]
    .into_iter()
    .flatten()
    .map(|p| p.display().to_string())
    .collect();
    if !secrets.is_empty() {
        t.line("");
        t.line("# The dev Discord application's secrets (never the real one's): files the account alone");
        t.line("# reads. Paste each secret into its file.");
        for f in &secrets {
            t.line(&format!(
                "if (-not (Test-Path {f})) {{ New-Item -ItemType File -Force {f} | Out-Null }}",
                f = ps(f)
            ));
            t.line(&format!(
                "icacls {} /inheritance:r /grant:r \"${{Account}}:(R,W)\" | Out-Null",
                ps(f)
            ));
        }
    }
    for h in &cfg.machines {
        let file = match h.os {
            Os::Windows => format!("setup-{}.ps1", h.name),
            Os::Linux => format!("setup-{}.sh", h.name),
        };
        t.line("");
        t.line(&format!("# {}: run {file} there, once.", h.name));
    }
    t.0
}

/// The script for `h`, a Windows machine running `worlds`, authorizing `key`.
fn windows_machine(h: &Host, worlds: &[&World], from: &str, key: &str) -> String {
    let mut t = Text::default();
    t.line(&format!(
        "# The Dereth preflight's one-time setup of {}, written by `cargo xtask preflight setup` from {from}.",
        h.name
    ));
    t.line(&format!(
        "# Read it, then run it once on {} in PowerShell as an administrator. -Account is the account the",
        h.name
    ));
    t.line("# preflight's ssh logs in as. It does nothing the preflight does not need, and running it again");
    t.line("# changes nothing more.");
    match h.account() {
        Some(a) => t.line(&format!("param([string]$Account = {})", ps(a))),
        None => t.line("param([string]$Account = \"$env:USERDOMAIN\\$env:USERNAME\")"),
    }
    t.line("$ErrorActionPreference = 'Stop'");
    t.line("$sid = (New-Object System.Security.Principal.NTAccount($Account)).Translate([System.Security.Principal.SecurityIdentifier]).Value");
    t.line("");
    t.line("# The OpenSSH server, started now and with Windows, and open to the network.");
    t.line("if (-not (Get-Service -Name sshd -ErrorAction SilentlyContinue)) {");
    t.line("  $capability = Get-WindowsCapability -Online | Where-Object { $_.Name -like 'OpenSSH.Server*' } | Select-Object -First 1");
    t.line("  Add-WindowsCapability -Online -Name $capability.Name | Out-Null");
    t.line("}");
    t.line("Set-Service -Name sshd -StartupType Automatic");
    t.line("Start-Service -Name sshd");
    t.line("if (-not (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {");
    t.line("  New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 | Out-Null");
    t.line("}");
    t.line("");
    t.line("# The preflight machine's key, which logs in as the account: an administrator's key is read from");
    t.line("# the administrators' key file, anyone else's from their own.");
    t.line(&format!("$key = {}", ps(key)));
    t.line("$admins = (New-Object System.Security.Principal.SecurityIdentifier([System.Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid, $null)).Translate([System.Security.Principal.NTAccount]).Value.Split('\\')[-1]");
    t.line("$isAdmin = @(([ADSI](\"WinNT://./$admins,group\")).psbase.Invoke('Members') | ForEach-Object {");
    t.line("  (New-Object System.Security.Principal.SecurityIdentifier([byte[]]$_.GetType().InvokeMember('objectSid', 'GetProperty', $null, $_, $null), 0)).Value");
    t.line("}) -contains $sid");
    t.line("if ($isAdmin) {");
    t.line("  $keys = Join-Path $env:ProgramData 'ssh\\administrators_authorized_keys'");
    t.line("} else {");
    t.line("  $userDir = (Get-CimInstance Win32_UserProfile | Where-Object { $_.SID -eq $sid }).LocalPath");
    t.line("  if (-not $userDir) { throw \"$Account has no profile here yet: log in as it once, then run this again\" }");
    t.line("  $keys = Join-Path $userDir '.ssh\\authorized_keys'");
    t.line("}");
    t.line("New-Item -ItemType Directory -Force -Path (Split-Path $keys) | Out-Null");
    t.line("$lines = @(if (Test-Path -LiteralPath $keys) { Get-Content -LiteralPath $keys })");
    t.line("if ($lines -notcontains $key) { Set-Content -LiteralPath $keys -Value ($lines + $key) -Encoding ascii }");
    t.line("if ($isAdmin) { icacls $keys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F' | Out-Null }");
    t.line("Write-Host \"the preflight machine's key logs in as $Account\"");
    if !worlds.is_empty() {
        t.line("");
        t.line(GRANT);
        for w in worlds {
            windows_world(&mut t, w);
        }
    }
    t.0
}

/// The script for `h`, a Linux machine running `worlds`.
fn linux_machine(h: &Host, worlds: &[&World], from: &str) -> String {
    let mut t = Text::default();
    t.line("#!/bin/sh");
    t.line(&format!(
        "# The Dereth preflight's one-time setup of {}, written by `cargo xtask preflight setup` from {from}.",
        h.name
    ));
    t.line(&format!(
        "# Read it, then run it once on {} as root: `sudo sh setup-{}.sh [account]`, the account the",
        h.name, h.name
    ));
    t.line("# preflight's ssh logs in as. That account may then stop and start the worlds' units without a");
    t.line("# password, and nothing more.");
    t.line("set -eu");
    if worlds.is_empty() {
        t.line(&format!(
            "echo \"no world runs on {}: nothing to do\"",
            h.name
        ));
        return t.0;
    }
    t.line(&format!(
        "account=\"${{1:-{}}}\"",
        h.account()
            .map_or("${SUDO_USER:-}".to_owned(), str::to_owned)
    ));
    t.line(
        "[ -n \"$account\" ] || { echo \"name the account: sudo sh $0 <account>\" >&2; exit 1; }",
    );
    t.line("command -v setfacl >/dev/null || { echo \"setfacl is missing: install the acl package, then run this again\" >&2; exit 1; }");
    t.line("systemctl=\"$(command -v systemctl)\"");
    let commands: Vec<String> = worlds
        .iter()
        .flat_map(|w| {
            [
                format!("$systemctl stop {}", w.service),
                format!("$systemctl start {}", w.service),
            ]
        })
        .collect();
    t.line("rule=/etc/sudoers.d/dereth-preflight");
    t.line(&format!(
        "printf '%s ALL=(root) NOPASSWD: %s\\n' \"$account\" \"{}\" > \"$rule.new\"",
        commands.join(", ")
    ));
    t.line("visudo -cf \"$rule.new\"");
    t.line("chmod 0440 \"$rule.new\"");
    t.line("mv \"$rule.new\" \"$rule\"");
    // The account may change each world's folder, and what the world writes there later.
    for w in worlds {
        t.line(&format!("setfacl -R -m \"u:$account:rwX\" {}", sh(&w.dir)));
        t.line(&format!(
            "setfacl -R -d -m \"u:$account:rwX\" {}",
            sh(&w.dir)
        ));
    }
    t.line(&format!(
        "echo \"$account may stop and start, and change the folders of: {}\"",
        worlds
            .iter()
            .map(|w| w.service.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    ));
    t.0
}
