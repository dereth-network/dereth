//! Windows services the launcher did not set up: whether the person may start and stop one,
//! starting and stopping it, and letting the person do so with one administrator's permission.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS,
    SERVICE_START, SERVICE_STOP,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn tool(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(name)
}

fn run(name: &str, args: &[&str]) -> Result<String, String> {
    let output = crate::components::command(&tool(name))
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    let out = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(out)
    } else {
        Err(format!(
            "{name}: {} {}",
            out.trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Whether this account may start, stop and query the service `name`.
pub fn may_control(name: &str) -> bool {
    if !super::valid_service_name(name) {
        return false;
    }
    let name = wide(name);
    // Both handles are closed on every path; nothing is changed.
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return false;
        }
        let service = OpenServiceW(
            manager,
            name.as_ptr(),
            SERVICE_START | SERVICE_STOP | SERVICE_QUERY_STATUS,
        );
        let allowed = !service.is_null();
        if allowed {
            CloseServiceHandle(service);
        } else {
            let _ = GetLastError();
        }
        CloseServiceHandle(manager);
        allowed
    }
}

pub fn start(name: &str) -> Result<(), String> {
    if !super::valid_service_name(name) {
        return Err("That is not a service's name.".into());
    }
    run("sc.exe", &["start", name]).map(|_| ())
}

pub fn stop(name: &str) -> Result<(), String> {
    if !super::valid_service_name(name) {
        return Err("That is not a service's name.".into());
    }
    run("sc.exe", &["stop", name]).map(|_| ())
}

/// This account's security identifier.
fn own_sid() -> Result<String, String> {
    let text = run("whoami.exe", &["/user", "/fo", "csv", "/nh"])?;
    text.split('"')
        .find(|s| super::valid_sid(s))
        .map(str::to_owned)
        .ok_or_else(|| "Could not tell who you are.".into())
}

/// Asks an administrator, with Windows' own prompt, to let this account start and stop the service
/// `name`: the launcher runs itself elevated with `--grant-service <name> <account>`.
pub fn grant(name: &str) -> Result<(), String> {
    if !super::valid_service_name(name) {
        return Err("That is not a service's name.".into());
    }
    let sid = own_sid()?;
    let exe = wide(
        &std::env::current_exe()
            .map_err(|e| e.to_string())?
            .to_string_lossy(),
    );
    let verb = wide("runas");
    let args = wide(&format!("--grant-service \"{name}\" {sid}"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: 0,
        ..Default::default()
    };
    // Started only by the page's button; Windows owns the prompt. The process handle is waited on
    // and closed.
    unsafe {
        if ShellExecuteExW(&mut info) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if code != 0 {
            return Err("The service's permissions were not changed.".into());
        }
    }
    Ok(())
}

/// The elevated half of [`grant`]: adds an entry for `sid` that may start, stop and query the
/// service `name` to its permissions, keeping every other entry.
///
/// # Errors
/// The name or the account does not read, or the service's permissions could not be read or set.
pub fn grant_elevated(name: &str, sid: &str) -> Result<(), String> {
    if !super::valid_service_name(name) || !super::valid_sid(sid) {
        return Err("The service or the account does not read.".into());
    }
    let current = run("sc.exe", &["sdshow", name])?;
    let current = current.trim();
    if !current.contains("D:") {
        return Err("The service's permissions do not read.".into());
    }
    match super::grant_sddl(current, sid) {
        None => Ok(()),
        Some(next) => run("sc.exe", &["sdset", name, &next]).map(|_| ()),
    }
}
