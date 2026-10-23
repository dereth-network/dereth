//! Starting and stopping a world that a service manager runs, when the launcher did not set it up:
//! a Windows service, a systemd service, or one launchd runs.
//!
//! - **A Windows service** is started and stopped by whoever its permissions let. **Let the
//!   launcher start and stop this service** asks once for an administrator's permission and adds
//!   the person's own account to the service's permissions, with start, stop and status only, as
//!   the launcher gives the person for a world it sets to run in the background. From then on the
//!   launcher starts and stops it as the person, with no prompt.
//! - **A systemd user service** is the person's own: started and stopped with
//!   `systemctl --user`.
//! - **A system service** (systemd's, or launchd's) needs an administrator each time: the page
//!   shows the command to run.
//!
//! What is asked of the system is behind [`Services`], so every rule here is tested over a
//! stand-in, and no test ever touches a real service.

use serde::Serialize;

use crate::local::Manager;

/// What the launcher can do about a service-managed world, as the page shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Control {
    /// The launcher may start and stop it itself.
    pub can_control: bool,
    /// The launcher can be let control it with one administrator's permission (Windows).
    pub can_grant: bool,
    /// The commands to run, when the launcher may not: to start it, and to stop it.
    pub start_command: Option<String>,
    pub stop_command: Option<String>,
}

/// The system's services, as far as the launcher uses them.
pub trait Services: Send + Sync {
    /// Whether the person may start and stop the Windows service `name`.
    fn may_control(&self, name: &str) -> bool;
    /// Starts it, or a systemd user service.
    fn start(&self, manager: &Manager) -> Result<(), String>;
    /// Stops it, or a systemd user service, through the service manager.
    fn stop(&self, manager: &Manager) -> Result<(), String>;
    /// Asks an administrator to let the person start and stop the Windows service `name`.
    fn grant(&self, name: &str) -> Result<(), String>;
}

/// What the launcher can do about a world `manager` runs.
#[must_use]
pub fn control(manager: &Manager, services: &dyn Services) -> Control {
    match manager {
        Manager::WindowsService { name } => {
            let can = services.may_control(name);
            Control {
                can_control: can,
                can_grant: !can,
                start_command: (!can).then(|| format!("sc start {name}")),
                stop_command: (!can).then(|| format!("sc stop {name}")),
            }
        }
        Manager::Systemd { user: true, .. } => Control {
            can_control: true,
            ..Control::default()
        },
        Manager::Systemd { unit, user: false } => Control {
            start_command: Some(format!("sudo systemctl start {unit}")),
            stop_command: Some(format!("sudo systemctl stop {unit}")),
            ..Control::default()
        },
        Manager::Launchd => Control {
            start_command: Some("sudo launchctl kickstart system/<its label>".into()),
            stop_command: Some("sudo launchctl kill TERM system/<its label>".into()),
            ..Control::default()
        },
        Manager::Elsewhere | Manager::Launcher => Control::default(),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Whether `name` can be a Windows service's name as the launcher passes it on: letters, digits,
/// and `-`, `_`, `.` and spaces, at most 256 characters.
#[must_use]
pub fn valid_service_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Whether `text` reads as a Windows security identifier: `S`, its revision `1`, an authority
/// and one to fifteen sub-authorities, all numbers, joined by hyphens.
#[must_use]
pub fn valid_sid(text: &str) -> bool {
    let mut parts = text.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    (2..=16).contains(&rest.len())
        && rest
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 15 && p.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg_attr(not(windows), allow(dead_code))]
/// The rights the person is given on a service: query its status, start it, stop it, and read
/// its permissions.
pub const GRANTED: &str = "LCRPWPRC";

#[cfg_attr(not(windows), allow(dead_code))]
/// A service's permissions (`sc sdshow`'s SDDL) with an entry for `sid` that may start, stop and
/// query it; `None` when an entry already gives it start and stop. The entry goes at the end of
/// the access list, before any audit list; everything else is kept as it was.
#[must_use]
pub fn grant_sddl(current: &str, sid: &str) -> Option<String> {
    let current = current.trim();
    let dacl_start = current.find("D:")?;
    let dacl_end = current[dacl_start..]
        .find("S:")
        .map_or(current.len(), |i| dacl_start + i);
    let dacl = &current[dacl_start..dacl_end];
    let already = dacl
        .split('(')
        .skip(1)
        .filter_map(|ace| ace.strip_suffix(')').or(Some(ace)))
        .map(|ace| ace.trim_end_matches(')').split(';').collect::<Vec<_>>())
        .any(|parts| {
            parts.len() >= 6
                && parts[0] == "A"
                && parts[5].eq_ignore_ascii_case(sid)
                && parts[2].contains("RP")
                && parts[2].contains("WP")
        });
    if already {
        return None;
    }
    Some(format!(
        "{}(A;;{GRANTED};;;{sid}){}",
        &current[..dacl_end],
        &current[dacl_end..]
    ))
}

/// The system's own services.
#[derive(Debug, Clone, Copy, Default)]
pub struct System;

#[cfg(windows)]
#[path = "services_windows.rs"]
mod windows;

#[cfg(windows)]
pub use windows::grant_elevated;

impl Services for System {
    fn may_control(&self, name: &str) -> bool {
        #[cfg(windows)]
        {
            windows::may_control(name)
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            false
        }
    }

    fn start(&self, manager: &Manager) -> Result<(), String> {
        match manager {
            #[cfg(windows)]
            Manager::WindowsService { name } => windows::start(name),
            Manager::Systemd { unit, user: true } => systemctl_user("start", unit),
            _ => Err("The launcher may not start this world.".into()),
        }
    }

    fn stop(&self, manager: &Manager) -> Result<(), String> {
        match manager {
            #[cfg(windows)]
            Manager::WindowsService { name } => windows::stop(name),
            Manager::Systemd { unit, user: true } => systemctl_user("stop", unit),
            _ => Err("The launcher may not stop this world.".into()),
        }
    }

    fn grant(&self, name: &str) -> Result<(), String> {
        #[cfg(windows)]
        {
            windows::grant(name)
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err("Only a Windows service is set up this way.".into())
        }
    }
}

fn systemctl_user(verb: &str, unit: &str) -> Result<(), String> {
    let output = crate::components::command(std::path::Path::new("systemctl"))
        .args(["--user", verb, unit])
        .output()
        .map_err(|e| format!("systemctl: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl: {}",
            crate::host_config::redact(String::from_utf8_lossy(&output.stderr).trim())
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    /// Services that are only memory: which Windows services the person may control, and what was
    /// asked.
    #[derive(Default)]
    pub struct FakeServices {
        pub allowed: Mutex<HashSet<String>>,
        pub asked: Mutex<Vec<String>>,
    }

    impl Services for FakeServices {
        fn may_control(&self, name: &str) -> bool {
            self.allowed.lock().unwrap().contains(name)
        }
        fn start(&self, manager: &Manager) -> Result<(), String> {
            self.asked
                .lock()
                .unwrap()
                .push(format!("start {manager:?}"));
            Ok(())
        }
        fn stop(&self, manager: &Manager) -> Result<(), String> {
            self.asked.lock().unwrap().push(format!("stop {manager:?}"));
            Ok(())
        }
        fn grant(&self, name: &str) -> Result<(), String> {
            self.asked.lock().unwrap().push(format!("grant {name}"));
            self.allowed.lock().unwrap().insert(name.to_owned());
            Ok(())
        }
    }

    #[test]
    fn a_windows_service_is_the_launchers_to_control_once_the_person_is_let() {
        let fake = FakeServices::default();
        let service = Manager::WindowsService {
            name: "Empyrean".into(),
        };
        let before = control(&service, &fake);
        assert!(!before.can_control && before.can_grant);
        assert_eq!(before.start_command.as_deref(), Some("sc start Empyrean"));
        fake.grant("Empyrean").unwrap();
        let after = control(&service, &fake);
        assert!(after.can_control && !after.can_grant);
        assert_eq!(after.start_command, None);
    }

    #[test]
    fn a_user_service_is_controlled_and_a_system_one_shows_its_commands() {
        let fake = FakeServices::default();
        let user = control(
            &Manager::Systemd {
                unit: "empyrean.service".into(),
                user: true,
            },
            &fake,
        );
        assert!(user.can_control && !user.can_grant);
        let system = control(
            &Manager::Systemd {
                unit: "empyrean.service".into(),
                user: false,
            },
            &fake,
        );
        assert!(!system.can_control && !system.can_grant);
        assert_eq!(
            system.stop_command.as_deref(),
            Some("sudo systemctl stop empyrean.service")
        );
        assert_eq!(control(&Manager::Elsewhere, &fake), Control::default());
    }

    #[test]
    fn the_grant_adds_start_and_stop_for_the_person_and_keeps_the_rest() {
        // A person's identifier, written from its parts.
        let sid = ["S", "1", "5", "21", "1", "2", "3", "1001"].join("-");
        let current = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)S:(AU;FA;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;WD)";
        let granted = grant_sddl(current, &sid).unwrap();
        assert_eq!(
            granted,
            format!("D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;LCRPWPRC;;;{sid})S:(AU;FA;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;WD)")
        );
        assert_eq!(grant_sddl(&granted, &sid), None, "once is enough");
        assert_eq!(
            grant_sddl("D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)", &sid),
            Some(format!(
                "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;LCRPWPRC;;;{sid})"
            )),
            "no audit list"
        );
        assert_eq!(grant_sddl("O:SY", &sid), None, "no access list to add to");
        assert!(valid_sid(&sid));
        assert!(!valid_sid(&sid.replacen('1', "2", 1)) && !valid_sid("S") && !valid_sid("x"));
        assert!(valid_service_name("Empyrean World 1"));
        assert!(!valid_service_name("a&b") && !valid_service_name(""));
    }
}
