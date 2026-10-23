//! Native background services for hosts created by the launcher.

use crate::host_runtime::{self, Definition};
use std::path::{Path, PathBuf};
#[cfg(windows)]
#[path = "host_service_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::stopping_progress;
#[cfg(windows)]
pub use windows::{dispatch, elevated, install_elevated};

pub fn name(id: &str) -> String {
    format!("dereth-{id}")
}
fn systemd_quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn systemd(definition: &Definition, dir: &Path) -> String {
    format!("[Unit]\nDescription=Dereth world host\nStartLimitIntervalSec=60\nStartLimitBurst=3\n\n[Service]\nType=simple\nExecStart={} --run-host {}\nWorkingDirectory={}\nKillSignal=SIGTERM\nKillMode=mixed\nTimeoutStopSec=infinity\nRestart=on-failure\nRestartSec=10\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(&definition.wrapper.to_string_lossy()), systemd_quote(&dir.to_string_lossy()), systemd_quote(&dir.to_string_lossy()))
}
pub fn launchd(definition: &Definition, dir: &Path) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>--run-host</string><string>{}</string></array><key>WorkingDirectory</key><string>{}</string><key>RunAtLoad</key><true/><key>ExitTimeOut</key><integer>0</integer></dict></plist>\n",
        xml(&name(&definition.id)), xml(&definition.wrapper.to_string_lossy()), xml(&dir.to_string_lossy()), xml(&dir.to_string_lossy()))
}
fn execute(program: &str, args: &[&str]) -> Result<(), String> {
    let output = crate::components::command(Path::new(program))
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program}: {}",
            crate::host_config::redact(&String::from_utf8_lossy(&output.stderr))
        ))
    }
}
fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "No home folder is available.".into())
}
fn gui_domain() -> Result<String, String> {
    let output = crate::components::command(Path::new("id"))
        .arg("-u")
        .output()
        .map_err(|e| e.to_string())?;
    let uid = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let uid = uid.trim();
    if !output.status.success() || uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Could not identify the login session.".into());
    }
    Ok(format!("gui/{uid}"))
}
pub fn installed(dir: &Path) -> bool {
    dir.join("service.json").is_file()
}
fn owned_unit(dir: &Path, definition: &Definition, linux: bool) -> Result<PathBuf, String> {
    let path: PathBuf = serde_json::from_slice(
        &std::fs::read(dir.join("service.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let expected = if linux {
        systemd(definition, dir)
    } else {
        launchd(definition, dir)
    };
    if std::fs::read_to_string(&path).map_err(|e| e.to_string())? != expected {
        return Err("The service definition has changed. It was left untouched.".into());
    }
    Ok(path)
}
pub fn install(dir: &Path) -> Result<(), String> {
    let definition = host_runtime::read_definition(dir)?;
    if host_runtime::active(dir) {
        return Err("Stop the world before setting it to run in the background.".into());
    }
    let _ = std::fs::remove_file(dir.join("stop.request"));
    if cfg!(target_os = "linux") {
        execute("systemctl", &["--user", "show-environment"])?;
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or(home()?.join(".config"));
        let folder = base.join("systemd/user");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let unit = format!("{}.service", name(&definition.id));
        let path = folder.join(&unit);
        if path.exists() && !installed(dir) {
            return Err("A service with this name already exists.".into());
        }
        if installed(dir) {
            owned_unit(dir, &definition, true)?;
        }
        std::fs::write(&path, systemd(&definition, dir)).map_err(|e| e.to_string())?;
        host_runtime::write_json(&dir.join("service.json"), &path)?;
        execute("systemctl", &["--user", "daemon-reload"])?;
        execute("systemctl", &["--user", "enable", "--now", &unit])?;
    } else if cfg!(target_os = "macos") {
        let folder = home()?.join("Library/LaunchAgents");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!("{}.plist", name(&definition.id)));
        if path.exists() && !installed(dir) {
            return Err("A service with this name already exists.".into());
        }
        if installed(dir) {
            owned_unit(dir, &definition, false)?;
        }
        std::fs::write(&path, launchd(&definition, dir)).map_err(|e| e.to_string())?;
        host_runtime::write_json(&dir.join("service.json"), &path)?;
        execute(
            "launchctl",
            &[
                "bootstrap",
                &gui_domain()?,
                path.to_str().ok_or("Invalid service path")?,
            ],
        )?;
    } else {
        #[cfg(windows)]
        {
            elevated(dir, false)?;
        }
        #[cfg(not(windows))]
        {
            return Err("Background services are unavailable on this system.".into());
        }
    }
    Ok(())
}
pub fn start(dir: &Path) -> Result<(), String> {
    let definition = host_runtime::read_definition(dir)?;
    let _ = std::fs::remove_file(dir.join("stop.request"));
    if cfg!(target_os = "linux") {
        owned_unit(dir, &definition, true)?;
        execute(
            "systemctl",
            &[
                "--user",
                "start",
                &format!("{}.service", name(&definition.id)),
            ],
        )
    } else if cfg!(target_os = "macos") {
        owned_unit(dir, &definition, false)?;
        execute(
            "launchctl",
            &[
                "kickstart",
                &format!("{}/{}", gui_domain()?, name(&definition.id)),
            ],
        )
    } else {
        #[cfg(windows)]
        {
            windows::start(dir)
        }
        #[cfg(not(windows))]
        {
            Err("Background services are unavailable on this system.".into())
        }
    }
}
pub fn remove(dir: &Path) -> Result<(), String> {
    if host_runtime::active(dir) {
        return Err("Stop the world before turning off running in the background.".into());
    }
    let definition = host_runtime::read_definition(dir)?;
    if cfg!(target_os = "linux") {
        let path = owned_unit(dir, &definition, true)?;
        execute(
            "systemctl",
            &[
                "--user",
                "disable",
                &format!("{}.service", name(&definition.id)),
            ],
        )?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        execute("systemctl", &["--user", "daemon-reload"])?;
    } else if cfg!(target_os = "macos") {
        let path = owned_unit(dir, &definition, false)?;
        execute(
            "launchctl",
            &[
                "bootout",
                &format!("{}/{}", gui_domain()?, name(&definition.id)),
            ],
        )?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    } else {
        #[cfg(windows)]
        {
            elevated(dir, true)?;
        }
        #[cfg(not(windows))]
        {
            return Err("Background services are unavailable on this system.".into());
        }
    }
    let _ = std::fs::remove_file(dir.join("service.json"));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_service_files_are_refused_without_modifying_them() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let definition = Definition {
            id: "host-test".into(),
            version: "1.0.0".into(),
            program: dir.join("server"),
            wrapper: dir.join("wrapper"),
            checkout: false,
        };
        let path = dir.join("host.service");
        host_runtime::write_json(&dir.join("service.json"), &path).unwrap();
        std::fs::write(&path, systemd(&definition, dir)).unwrap();
        assert_eq!(owned_unit(dir, &definition, true).unwrap(), path);
        std::fs::write(&path, "another service").unwrap();
        assert!(owned_unit(dir, &definition, true).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "another service");
    }
    #[test]
    fn linux_services_quote_paths_and_preserve_a_graceful_shutdown() {
        let d = Definition {
            id: "host-123".into(),
            version: "1.0.0".into(),
            program: "/tmp/server".into(),
            wrapper: "/home/a b/100%/host".into(),
            checkout: false,
        };
        let unit = systemd(&d, Path::new("/home/a b/world"));
        assert!(unit.contains("\"/home/a b/100%%/host\" --run-host \"/home/a b/world\""));
        assert!(unit.contains("KillMode=mixed"));
        assert!(unit.contains("TimeoutStopSec=infinity"));
        assert!(unit.contains("WantedBy=default.target"));
    }
    #[test]
    fn macos_services_escape_xml_and_keep_arguments_separate() {
        let d = Definition {
            id: "host-123".into(),
            version: "1.0.0".into(),
            program: "/tmp/server".into(),
            wrapper: "/Users/A & B/host".into(),
            checkout: false,
        };
        let plist = launchd(&d, Path::new("/Users/A & B/world"));
        assert!(plist.contains("/Users/A &amp; B/host</string><string>--run-host</string>"));
        assert!(plist.contains("<key>Label</key><string>dereth-host-123</string>"));
        assert!(plist.contains("<key>ExitTimeOut</key><integer>0</integer>"));
    }
}
