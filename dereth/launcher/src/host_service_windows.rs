//! The Windows service entry point and its explicit elevated installation helper.

use crate::host_runtime;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

static SERVICE: OnceLock<(String, PathBuf)> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
static STOPPING: AtomicBool = AtomicBool::new(false);
static HANDLE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHECKPOINT: AtomicU32 = AtomicU32::new(1);
pub fn stopping_progress() {
    if !HANDLE.load(Ordering::SeqCst).is_null() {
        STOPPING.store(true, Ordering::SeqCst);
        report(SERVICE_STOP_PENDING, 0);
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn report(state: u32, exit: u32) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: exit,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: if state == SERVICE_STOP_PENDING {
            CHECKPOINT.fetch_add(1, Ordering::SeqCst)
        } else {
            0
        },
        dwWaitHint: if state == SERVICE_STOP_PENDING {
            300_000
        } else {
            0
        },
    };
    // The dispatcher owns the handle until service_main returns; status is valid for this call.
    unsafe {
        SetServiceStatus(HANDLE.load(Ordering::SeqCst), &status);
    }
}
unsafe extern "system" fn control(
    code: u32,
    _: u32,
    _: *mut core::ffi::c_void,
    _: *mut core::ffi::c_void,
) -> u32 {
    match code {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            STOP.store(true, Ordering::SeqCst);
            report(SERVICE_STOP_PENDING, 0);
            0
        }
        SERVICE_CONTROL_INTERROGATE => 0,
        _ => 120,
    }
}
unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let Some((name, dir)) = SERVICE.get() else {
        return;
    };
    let name = wide(name);
    // The null-terminated name and callback remain valid throughout registration.
    let handle =
        unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control), std::ptr::null()) };
    if handle.is_null() {
        return;
    }
    HANDLE.store(handle, Ordering::SeqCst);
    report(SERVICE_RUNNING, 0);
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        let progress = scope.spawn(|| {
            while !finished.load(Ordering::SeqCst) {
                if STOP.load(Ordering::SeqCst) || STOPPING.load(Ordering::SeqCst) {
                    report(SERVICE_STOP_PENDING, 0);
                }
                std::thread::park_timeout(std::time::Duration::from_secs(5));
            }
        });
        let result = host_runtime::run(dir, &STOP, None);
        finished.store(true, Ordering::SeqCst);
        progress.thread().unpark();
        let _ = progress.join();
        result
    });
    if let Err(error) = &result {
        let _ = host_runtime::write_json(
            &dir.join("status.json"),
            &host_runtime::Status {
                phase: "failed".into(),
                message: Some(error.clone()),
                ..Default::default()
            },
        );
    }
    report(SERVICE_STOPPED, if result.is_ok() { 0 } else { 1 });
}
pub fn dispatch(dir: PathBuf) -> Result<(), String> {
    let definition = host_runtime::read_definition(&dir)?;
    let name = super::name(&definition.id);
    SERVICE
        .set((name.clone(), dir))
        .map_err(|_| "The service dispatcher already started.")?;
    let mut name = wide(&name);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: std::ptr::null_mut(),
            lpServiceProc: None,
        },
    ];
    // The table and its name live until the blocking dispatcher returns.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
fn tool(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(name)
}
fn run(name: &str, args: &[&str]) -> Result<(), String> {
    let output = crate::components::command(&tool(name))
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{name}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in text.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '\"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            out.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        out.push(c);
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('\"');
    out
}
fn valid_owner_sid(text: &str) -> bool {
    let mut parts = text.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    if !parts
        .next()
        .is_some_and(|v| v.parse::<u64>().is_ok_and(|n| n < (1 << 48)))
    {
        return false;
    }
    let subauthorities: Vec<_> = parts.collect();
    !subauthorities.is_empty()
        && subauthorities.len() <= 15
        && subauthorities.iter().all(|v| {
            !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<u32>().is_ok()
        })
}
fn binary_path(dir: &Path) -> Result<String, String> {
    let definition = host_runtime::read_definition(dir)?;
    Ok(format!(
        "{} --host-service {}",
        quote(&definition.wrapper.to_string_lossy()),
        quote(&dir.to_string_lossy())
    ))
}
fn owns_service(dir: &Path) -> Result<bool, String> {
    let definition = host_runtime::read_definition(dir)?;
    let name = wide(&super::name(&definition.id));
    let expected = binary_path(dir)?;
    // Handles are closed on every path; the config buffer remains alive while its string is read.
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let service = OpenServiceW(manager, name.as_ptr(), SERVICE_QUERY_CONFIG);
        if service.is_null() {
            let error = std::io::Error::last_os_error();
            CloseServiceHandle(manager);
            if error.raw_os_error() == Some(1060) {
                return Ok(false);
            }
            return Err(error.to_string());
        }
        let mut needed = 0;
        QueryServiceConfigW(service, std::ptr::null_mut(), 0, &mut needed);
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let config = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        let success = QueryServiceConfigW(service, config, needed, &mut needed);
        let result = if success == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            let path = (*config).lpBinaryPathName;
            let mut len = 0;
            while *path.add(len) != 0 {
                len += 1;
            }
            let actual = String::from_utf16_lossy(std::slice::from_raw_parts(path, len));
            if actual == expected {
                Ok(true)
            } else {
                Err("A different service uses this name. It was left untouched.".into())
            }
        };
        CloseServiceHandle(service);
        CloseServiceHandle(manager);
        result
    }
}
pub fn start(dir: &Path) -> Result<(), String> {
    if !owns_service(dir)? {
        return Err("This world does not run in the background.".into());
    }
    let definition = host_runtime::read_definition(dir)?;
    run("sc.exe", &["start", &super::name(&definition.id)])
}

fn set_logon(account: &str, enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::Security::Authentication::Identity::*;
    use windows_sys::Win32::Security::LookupAccountNameW;
    let account = wide(account);
    // The service SID is resolved after creation. Buffers are sized by the lookup and remain
    // alive until the policy operation completes; the policy handle is closed afterward.
    unsafe {
        let mut sid_len = 0;
        let mut domain_len = 0;
        let mut kind = 0;
        LookupAccountNameW(
            std::ptr::null(),
            account.as_ptr(),
            std::ptr::null_mut(),
            &mut sid_len,
            std::ptr::null_mut(),
            &mut domain_len,
            &mut kind,
        );
        if sid_len == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut sid = vec![0usize; (sid_len as usize).div_ceil(std::mem::size_of::<usize>())];
        let mut domain = vec![0u16; domain_len as usize];
        if LookupAccountNameW(
            std::ptr::null(),
            account.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let attributes = LSA_OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<LSA_OBJECT_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        let mut policy: LSA_HANDLE = 0;
        let status = LsaOpenPolicy(
            std::ptr::null(),
            &attributes,
            (POLICY_LOOKUP_NAMES | POLICY_CREATE_ACCOUNT) as u32,
            &mut policy,
        );
        if status != 0 {
            return Err(
                std::io::Error::from_raw_os_error(LsaNtStatusToWinError(status) as i32).to_string(),
            );
        }
        let mut right = wide("SeServiceLogonRight");
        let right = LSA_UNICODE_STRING {
            Length: ((right.len() - 1) * 2) as u16,
            MaximumLength: (right.len() * 2) as u16,
            Buffer: right.as_mut_ptr(),
        };
        let status = if enabled {
            LsaAddAccountRights(policy, sid.as_mut_ptr().cast(), &right, 1)
        } else {
            LsaRemoveAccountRights(policy, sid.as_mut_ptr().cast(), false, &right, 1)
        };
        LsaClose(policy);
        if status != 0 {
            return Err(
                std::io::Error::from_raw_os_error(LsaNtStatusToWinError(status) as i32).to_string(),
            );
        }
    }
    Ok(())
}

fn revoke_access(dir: &Path, account: &str) -> Result<(), String> {
    if let Ok(bytes) = std::fs::read(dir.join("service-dats.json")) {
        let paths: Vec<PathBuf> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        for path in paths {
            run(
                "icacls.exe",
                &[
                    path.to_str().ok_or("Invalid game data path")?,
                    "/remove:g",
                    account,
                    "/Q",
                ],
            )?;
        }
        std::fs::remove_file(dir.join("service-dats.json")).map_err(|e| e.to_string())?;
    }
    run(
        "icacls.exe",
        &[
            dir.to_str().ok_or("Invalid host path")?,
            "/remove:g",
            account,
            "/T",
            "/Q",
        ],
    )
}

fn reject_redirected_path(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    for part in path.ancestors() {
        let metadata = std::fs::symlink_metadata(part).map_err(|e| e.to_string())?;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(
                "Running in the background needs folders without links or junctions.".into(),
            );
        }
    }
    Ok(())
}
fn validate_service_tree(dir: &Path) -> Result<(), String> {
    reject_redirected_path(dir)?;
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        reject_redirected_path(&entry.path())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            validate_service_tree(&entry.path())?;
        }
    }
    Ok(())
}

pub fn install_elevated(dir: &Path, remove: bool) -> Result<(), String> {
    validate_service_tree(dir)?;
    let definition = host_runtime::read_definition(dir)?;
    let name = super::name(&definition.id);
    let exists = owns_service(dir)?;
    if remove {
        if !exists {
            return Err("The service no longer exists.".into());
        }
        if host_runtime::active(dir) {
            return Err("Stop the world before turning off running in the background.".into());
        }
        let account = format!("NT SERVICE\\{name}");
        revoke_access(dir, &account)?;
        set_logon(&account, false)?;
        run("sc.exe", &["delete", &name])?;
        std::fs::remove_file(dir.join("service.json")).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if exists {
        return Err("This service already exists.".into());
    }
    let account = format!("NT SERVICE\\{name}");
    run(
        "sc.exe",
        &[
            "create",
            &name,
            "binPath=",
            &binary_path(dir)?,
            "start=",
            "auto",
            "obj=",
            &account,
            "DisplayName=",
            &format!("Dereth host {}", definition.id),
        ],
    )?;
    let installed = (|| {
        set_logon(&account, true)?;
        run(
            "icacls.exe",
            &[
                dir.to_str().ok_or("Invalid host path")?,
                "/grant",
                &format!("{account}:(OI)(CI)M"),
                "/T",
                "/Q",
            ],
        )?;
        let settings = crate::host_config::Settings::from_config(
            &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
        )?;
        let files = dereth_launch::datset::scan_dir(&settings.dats);
        if files.len() < 3 || files.iter().any(|f| f.error.is_some()) {
            return Err("The game data files could not be read.".into());
        }
        let mut permissions = vec![settings.dats.clone()];
        permissions.extend(files.into_iter().map(|f| settings.dats.join(f.file_name)));
        for path in &permissions {
            reject_redirected_path(path)?;
        }
        host_runtime::write_json(&dir.join("service-dats.json"), &permissions)?;
        for path in &permissions {
            run(
                "icacls.exe",
                &[
                    path.to_str().ok_or("Invalid game data path")?,
                    "/grant",
                    &format!("{account}:RX"),
                    "/Q",
                ],
            )?;
        }
        let owner =
            std::fs::read_to_string(dir.join("service-owner.txt")).map_err(|e| e.to_string())?;
        let sid = owner.trim();
        if !valid_owner_sid(sid) {
            return Err("The service owner identity is invalid.".into());
        }
        run(
            "sc.exe",
            &[
                "sdset",
                &name,
                &format!("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWRPWPDTLOCRRC;;;{sid})"),
            ],
        )?;
        host_runtime::write_json(&dir.join("service.json"), &name)?;
        start(dir)
    })();
    if let Err(error) = installed {
        let cleanup = revoke_access(dir, &account)
            .and_then(|()| set_logon(&account, false))
            .and_then(|()| run("sc.exe", &["delete", &name]));
        if let Err(cleanup_error) = cleanup {
            let _ = host_runtime::write_json(&dir.join("service.json"), &name);
            return Err(format!("{error} Service cleanup also failed: {cleanup_error}. Turn off running in the background to try the cleanup again."));
        }
        let _ = std::fs::remove_file(dir.join("service.json"));
        return Err(error);
    }
    Ok(())
}
pub fn elevated(dir: &Path, remove: bool) -> Result<(), String> {
    if !remove {
        let settings = crate::host_config::Settings::from_config(
            &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
        )?;
        let files = dereth_launch::datset::scan_dir(&settings.dats);
        if files.len() < 3 || files.iter().any(|f| f.error.is_some()) {
            return Err("The game data files could not be read.".into());
        }
        let output = crate::components::command(&tool("whoami.exe"))
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout);
        let sid = text
            .split('"')
            .find(|s| valid_owner_sid(s))
            .ok_or("Could not identify the service owner.")?;
        std::fs::write(dir.join("service-owner.txt"), sid).map_err(|e| e.to_string())?;
    }

    let exe = wide(
        &std::env::current_exe()
            .map_err(|e| e.to_string())?
            .to_string_lossy(),
    );
    let verb = wide("runas");
    let args = wide(&format!(
        "{} {}",
        if remove {
            "--remove-host-service"
        } else {
            "--install-host-service"
        },
        quote(&dir.to_string_lossy())
    ));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: 0,
        ..Default::default()
    };
    // The helper is started only by the service button. Windows owns the elevation prompt.
    unsafe {
        if ShellExecuteExW(&mut info) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if code != 0 {
            return Err(std::fs::read_to_string(dir.join("service-error.txt"))
                .unwrap_or_else(|_| "Service setup failed.".into()));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_arguments_quote_spaces_quotes_and_trailing_backslashes() {
        assert_eq!(quote("a b"), "\"a b\"");
        assert_eq!(quote("a\\"), "\"a\\\\\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
    }
}
