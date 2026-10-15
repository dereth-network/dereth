//! What the preflight runs on another Windows machine for one of its worlds: PowerShell scripts,
//! each one step, sent over ssh as [`super::machine::powershell`] encodes them. Every path is a
//! PowerShell literal, so a folder with spaces or quotes in its name is no different. Files cross
//! with scp through a staging folder in the ssh account's home (`.dereth-preflight\<world>`),
//! whose path scp sees relative to that home and so with no characters to quote; the scripts move
//! them from there.

use super::config::{Kind, World};
use super::machine::Service;

/// `s` as a PowerShell string literal.
pub fn ps(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Where `name` is staged for `w`, as scp spells it from the ssh account's home.
pub fn staged(w: &World, name: &str) -> String {
    format!(".dereth-preflight/{}/{name}", w.name)
}

/// Where `w`'s files as they were are kept, beside it.
pub fn backup_dir(w: &World) -> String {
    format!("{}\\.preflight-backup", w.dir.trim_end_matches(['/', '\\']))
}

/// A script's first lines: what it does, and its settings.
fn start(w: &World, what: &str) -> String {
    let machine = w.host.as_ref().map_or("this machine", |h| h.name.as_str());
    format!(
        "# {} on {machine}: {what}\n$ErrorActionPreference = 'Stop'\n$ProgressPreference = 'SilentlyContinue'\n",
        w.name
    )
}

/// The staging folder of `w`, as a variable.
fn stage(w: &World) -> String {
    format!(
        "$stage = Join-Path $HOME {}\n",
        ps(&format!(".dereth-preflight\\{}", w.name))
    )
}

/// `w`'s service brought to `to`, waiting up to a minute for it to get there.
fn service(w: &World, to: Service) -> String {
    let (verb, state) = match to {
        Service::Stopped => ("stop", "STOPPED"),
        Service::Running => ("start", "RUNNING"),
    };
    format!(
        "$service = {name}
if (-not ((sc.exe query $service) -match '{state}')) {{
  sc.exe {verb} $service | Out-Null
  if ($LASTEXITCODE -ne 0) {{ throw \"sc {verb} $service failed ($LASTEXITCODE)\" }}
}}
$until = (Get-Date).AddMinutes(1)
while (-not ((sc.exe query $service) -match '{state}')) {{
  if ((Get-Date) -gt $until) {{ throw \"$service did not {verb} within a minute\" }}
  Start-Sleep -Milliseconds 500
}}
",
        name = ps(&w.service)
    )
}

/// The database files of `w` that go with each of `dbs`: each, and its write-ahead log and
/// shared memory, as a PowerShell array.
fn database_files(dbs: &[String]) -> String {
    let files: Vec<String> = dbs
        .iter()
        .flat_map(|db| ["", "-wal", "-shm"].map(|suffix| ps(&format!("{db}{suffix}"))))
        .collect();
    format!("@({})", files.join(", "))
}

/// `w` stopped, and its configuration staged to be copied down.
pub fn stage_config(w: &World) -> String {
    format!(
        "{}{}{}if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath {} -Destination (Join-Path $stage 'config')
",
        start(w, "stopped, and its configuration staged"),
        service(w, Service::Stopped),
        stage(w),
        ps(&w.config_file())
    )
}

/// `w`'s files as they are kept beside it: what a build replaces, its configuration, and the
/// database files `dbs` (with their logs) that are there.
pub fn backup(w: &World, dbs: &[String]) -> String {
    let mut s = start(w, "its files as they are, kept beside it");
    s.push_str(&format!(
        "$backup = {}
if (Test-Path -LiteralPath $backup) {{ Remove-Item -LiteralPath $backup -Recurse -Force }}
New-Item -ItemType Directory -Force -Path (Join-Path $backup 'db') | Out-Null
Copy-Item -LiteralPath {} -Destination (Join-Path $backup 'replaced') -Recurse
Copy-Item -LiteralPath {} -Destination (Join-Path $backup 'config')
",
        ps(&backup_dir(w)),
        ps(&w.replaced()),
        ps(&w.config_file())
    ));
    if !dbs.is_empty() {
        s.push_str(&format!(
            "foreach ($file in {}) {{
  if (Test-Path -LiteralPath $file) {{ Copy-Item -LiteralPath $file -Destination (Join-Path $backup 'db') }}
}}
",
            database_files(dbs)
        ));
    }
    s
}

/// The staged build (when there is one) and configuration put in place, the staging folder
/// removed, and `w` started. A mod's build is copied over its folder, which keeps what the
/// build does not carry (its settings).
pub fn place(w: &World, build: bool) -> String {
    let mut s = start(
        w,
        "the build and its configuration put in place, and started",
    );
    s.push_str(&stage(w));
    if build {
        match w.kind {
            Kind::Empyrean => s.push_str(&format!(
                "Copy-Item -LiteralPath (Join-Path $stage 'build') -Destination {} -Force\n",
                ps(&w.replaced())
            )),
            Kind::Ace => {
                let to = ps(&w.replaced());
                s.push_str(&format!(
                    "New-Item -ItemType Directory -Force -Path {to} | Out-Null
Get-ChildItem -LiteralPath (Join-Path $stage 'build') | Copy-Item -Destination {to} -Recurse -Force
"
                ));
            }
        }
    }
    s.push_str(&format!(
        "Copy-Item -LiteralPath (Join-Path $stage 'config') -Destination {} -Force
Remove-Item -LiteralPath $stage -Recurse -Force
",
        ps(&w.config_file())
    ));
    s.push_str(&service(w, Service::Running));
    s
}

/// `w` put back from its backup as it was before the preflight, and started; the backup removed
/// once it runs. A mod's folder is put back whole, nothing the build added left in it; a database
/// file the backup lacks (a log made since) goes.
pub fn restore(w: &World, dbs: &[String]) -> String {
    let mut s = start(w, "put back as it was before the preflight");
    s.push_str(&service(w, Service::Stopped));
    let replaced = ps(&w.replaced());
    s.push_str(&format!("$backup = {}\n", ps(&backup_dir(w))));
    if w.kind == Kind::Ace {
        s.push_str(&format!(
            "if (Test-Path -LiteralPath {replaced}) {{ Remove-Item -LiteralPath {replaced} -Recurse -Force }}\n"
        ));
    }
    s.push_str(&format!(
        "Copy-Item -LiteralPath (Join-Path $backup 'replaced') -Destination {replaced} -Recurse -Force
Copy-Item -LiteralPath (Join-Path $backup 'config') -Destination {} -Force
",
        ps(&w.config_file())
    ));
    if !dbs.is_empty() {
        s.push_str(&format!(
            "foreach ($file in {}) {{
  $kept = Join-Path (Join-Path $backup 'db') (Split-Path -Leaf $file)
  if (Test-Path -LiteralPath $kept) {{ Copy-Item -LiteralPath $kept -Destination $file -Force }}
  elseif (Test-Path -LiteralPath $file) {{ Remove-Item -LiteralPath $file -Force }}
}}
",
            database_files(dbs)
        ));
    }
    s.push_str(&service(w, Service::Running));
    s.push_str("Remove-Item -LiteralPath $backup -Recurse -Force\n");
    s
}

/// The assemblies of `w`'s ACE server staged to be copied down, for its mod's build.
pub fn assemblies(w: &World) -> String {
    format!(
        "{}if (-not (Test-Path -LiteralPath (Join-Path {dir} 'ACE.Server.dll'))) {{ throw ('There is no ACE.Server.dll in ' + {dir} + ': the world''s dir is the folder that holds it.') }}
$stage = Join-Path $HOME {}
if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Get-ChildItem -LiteralPath {dir} -Filter '*.dll' -File | Copy-Item -Destination $stage
",
        start(w, "its ACE server's assemblies staged, for the mod's build"),
        ps(&format!(".dereth-preflight\\{}\\ace", w.name)),
        dir = ps(&w.dir)
    )
}
