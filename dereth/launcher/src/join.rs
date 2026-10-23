//! Join links: `dereth://join?host=...&port=...&name=...` opens Worlds with Add world filled in,
//! for the person to confirm ([`dereth_launch::join`]).
//!
//! **How a link arrives.** On Windows and Linux the system starts the launcher with the link as
//! its argument. When a launcher is already open (its [`RUNNING_FILE`] is in the data folder), the
//! new one leaves the link in the data folder for it and ends; the open launcher looks for one
//! every second, takes it, and comes to the front. On macOS the system hands the open app the
//! link itself, through Tauri's deep-link plugin. Either way the link waits in [`Pending`] until
//! the page takes it ([`take_join`]).
//!
//! **Registering the scheme.** Only a release's launcher registers it, so that a build from a
//! checkout never takes the scheme over: on Windows at each start in the user's own registry
//! (`HKEY_CURRENT_USER\Software\Classes\dereth`, each value written only when it differs); on Linux
//! at each start in the launcher's desktop entry and the user's `mimeapps.list`; on macOS in the
//! bundle's `Info.plist`, which packaging writes from `plugins.deep-link` in `tauri.conf.json`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dereth_launch::join::{self, JoinLink, RegistryValue, REQUEST_FILE, RUNNING_FILE};
use tauri::{Manager, State};

/// The link that came in, waiting for the page.
pub type Pending = Arc<Mutex<Option<JoinLink>>>;

fn lock(p: &Pending) -> std::sync::MutexGuard<'_, Option<JoinLink>> {
    p.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Keeps `link`, when it reads, for the page to take; one that does not read is dropped with a
/// line on standard error.
pub fn offer(pending: &Pending, link: &str) {
    match join::parse(link) {
        Ok(l) => *lock(pending) = Some(l),
        Err(e) => eprintln!("dereth: a link was not read: {e}"),
    }
}

/// The page takes the link that came in, if any: it then opens Worlds with Add world filled in.
#[tauri::command]
pub fn take_join(pending: State<'_, Pending>) -> Option<JoinLink> {
    lock(&pending).take()
}

/// How long a second launcher waits for the open one to take its link before it opens itself.
const HAND_OFF_WAIT: Duration = Duration::from_secs(3);

/// Hands `link` to the launcher already open, whose data folder is `data`: true when it took it
/// (this launcher then ends), false when none is open or it did not take it in time (this one then
/// opens with the link itself, which it takes back).
pub fn hand_off(data: &Path, link: &str) -> bool {
    hand_off_within(data, link, HAND_OFF_WAIT)
}

/// [`hand_off`], waiting at most `wait` for the open launcher to take the link.
fn hand_off_within(data: &Path, link: &str, wait: Duration) -> bool {
    if !data.join(RUNNING_FILE).is_file() || join::leave_request(data, link).is_err() {
        return false;
    }
    let started = Instant::now();
    while started.elapsed() < wait {
        if !join::request_waiting(data) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(data.join(REQUEST_FILE));
    false
}

/// Marks a launcher open in `data` until the marker is dropped.
#[derive(Debug)]
pub struct Running(PathBuf);

impl Running {
    #[must_use]
    pub fn mark(data: &Path) -> Self {
        let _ = std::fs::create_dir_all(data);
        let _ = std::fs::write(data.join(RUNNING_FILE), std::process::id().to_string());
        Self(data.join(RUNNING_FILE))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Every second, takes a link a second launcher left in `data` and brings the window to the front.
pub fn watch(app: tauri::AppHandle, data: PathBuf) {
    std::thread::spawn(move || loop {
        if let Some(taken) = join::take_request(&data) {
            match taken {
                Ok(link) => {
                    *lock(&app.state::<Pending>()) = Some(link);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                Err(e) => eprintln!("dereth: a link was not read: {e}"),
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    });
}

// ----- registering the scheme --------------------------------------------------------------------

#[cfg_attr(not(windows), allow(dead_code))]
/// The registry, as far as the scheme needs it: a value read and a value written, under
/// `HKEY_CURRENT_USER\Software\Classes`.
pub trait Registry {
    fn read(&self, key: &str, name: Option<&str>) -> Option<String>;
    fn write(&mut self, key: &str, name: Option<&str>, value: &str) -> Result<(), String>;
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Writes each of `values` whose text differs from what `registry` holds. Returns the values
/// written.
///
/// # Errors
/// A value could not be written.
pub fn write_changed(
    registry: &mut dyn Registry,
    values: &[RegistryValue],
) -> Result<Vec<RegistryValue>, String> {
    let mut written = Vec::new();
    for v in values {
        if registry.read(&v.key, v.name.as_deref()).as_deref() != Some(v.value.as_str()) {
            registry.write(&v.key, v.name.as_deref(), &v.value)?;
            written.push(v.clone());
        }
    }
    Ok(written)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The user's `mimeapps.list` (`current`, at `path`) with the launcher's desktop entry as the
/// scheme's default, when it is not already; the entry itself, which says it opens the scheme's
/// links, is [`crate::registration`]'s.
#[must_use]
pub fn linux_mimeapps_file(path: &Path, current: Option<&str>) -> Option<(PathBuf, String)> {
    let desktop_file = format!("{}.desktop", crate::registration::APP_ID);
    join::linux_mimeapps(current, &desktop_file).map(|list| (path.to_path_buf(), list))
}

/// Registers the scheme for this launcher, when it is a release's (`release`); a checkout's build
/// leaves the system as it is. A failure is reported and otherwise ignored: the launcher runs
/// without its links rather than not at all.
pub fn register(release: bool) {
    if !release {
        return;
    }
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut registry = windows::UserClasses;
        if let Err(e) = write_changed(&mut registry, &join::windows_registry(&exe)) {
            eprintln!("dereth: could not register dereth:// links: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let Some(path) = join::mimeapps_path(&|v| std::env::var_os(v)) else {
            return;
        };
        let current = std::fs::read_to_string(&path).ok();
        if let Some((path, text)) = linux_mimeapps_file(&path, current.as_deref()) {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, text));
            if let Err(e) = written {
                eprintln!("dereth: could not register dereth:// links: {e}");
            }
        }
    }
}

/// The user's own classes in the Windows registry.
#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn path(key: &str) -> Vec<u16> {
        wide(&format!("Software\\Classes\\{key}"))
    }

    /// `HKEY_CURRENT_USER\Software\Classes`.
    pub struct UserClasses;

    impl super::Registry for UserClasses {
        fn read(&self, key: &str, name: Option<&str>) -> Option<String> {
            let key = path(key);
            let name = name.map(wide);
            let name_ptr = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
            let mut buffer = vec![0u16; 2048];
            let mut bytes = u32::try_from(buffer.len() * 2).ok()?;
            // The buffer outlives the call, and its size in bytes is what the call is given.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name_ptr,
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            if status != ERROR_SUCCESS {
                return None;
            }
            let len = (bytes as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buffer[..len.min(buffer.len())]))
        }

        fn write(&mut self, key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
            let key = path(key);
            let name = name.map(wide);
            let name_ptr = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
            let data = wide(value);
            let size = u32::try_from(data.len() * 2).map_err(|e| e.to_string())?;
            let mut handle: HKEY = std::ptr::null_mut();
            // The key is created (or opened) under the user's own hive and closed on every path.
            unsafe {
                let status = RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut handle,
                    std::ptr::null_mut(),
                );
                if status != ERROR_SUCCESS {
                    return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
                }
                let status =
                    RegSetValueExW(handle, name_ptr, 0, REG_SZ, data.as_ptr().cast(), size);
                RegCloseKey(handle);
                if status != ERROR_SUCCESS {
                    return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A registry that is only memory.
    #[derive(Default)]
    struct FakeRegistry(HashMap<(String, Option<String>), String>);

    impl Registry for FakeRegistry {
        fn read(&self, key: &str, name: Option<&str>) -> Option<String> {
            self.0
                .get(&(key.to_owned(), name.map(str::to_owned)))
                .cloned()
        }
        fn write(&mut self, key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
            self.0
                .insert((key.to_owned(), name.map(str::to_owned)), value.to_owned());
            Ok(())
        }
    }

    #[test]
    fn windows_registration_writes_the_scheme_once_and_again_only_when_the_launcher_moved() {
        let mut registry = FakeRegistry::default();
        let here = join::windows_registry(Path::new(r"C:\Games\Dereth\dereth.exe"));
        assert_eq!(write_changed(&mut registry, &here).unwrap().len(), 4);
        assert_eq!(
            registry.read(r"dereth\shell\open\command", None).as_deref(),
            Some(r#""C:\Games\Dereth\dereth.exe" "%1""#)
        );
        assert!(
            write_changed(&mut registry, &here).unwrap().is_empty(),
            "the same again writes nothing"
        );
        let moved = join::windows_registry(Path::new(r"D:\Dereth\dereth.exe"));
        let written = write_changed(&mut registry, &moved).unwrap();
        assert_eq!(
            written.iter().map(|v| v.key.as_str()).collect::<Vec<_>>(),
            [r"dereth\DefaultIcon", r"dereth\shell\open\command"],
            "only what names the launcher's place"
        );
    }

    /// On Linux a release's desktop entry opens the scheme's links, and the user's defaults name it
    /// for the scheme once, every other line kept; a checkout's build writes the plain entry.
    #[test]
    fn linux_registration_writes_the_entry_that_opens_links_and_the_default_once() {
        let home = std::env::temp_dir().join("join-linux");
        let data = home.join(".local/share");
        let exec = Path::new("/apps/Dereth.AppImage");
        let files = dereth_launch::desktop::launcher_files_with(&data, exec, b"png", true);
        let entry = String::from_utf8(files[0].1.clone()).unwrap();
        assert!(
            entry.contains("\nExec=\"/apps/Dereth.AppImage\" %u\n"),
            "{entry}"
        );
        assert!(
            entry.contains("\nMimeType=x-scheme-handler/dereth;\n"),
            "{entry}"
        );
        let plain = dereth_launch::desktop::launcher_files(&data, exec, b"png");
        assert!(!String::from_utf8_lossy(&plain[0].1).contains("MimeType"));
        let mimeapps = home.join(".config/mimeapps.list");
        let theirs = "[Default Applications]\ntext/html=firefox.desktop\n";
        let (path, ours) = linux_mimeapps_file(&mimeapps, Some(theirs)).unwrap();
        assert_eq!(path, mimeapps);
        assert_eq!(
            ours,
            "[Default Applications]\ntext/html=firefox.desktop\nx-scheme-handler/dereth=network.dereth.dereth.desktop\n"
        );
        assert_eq!(
            linux_mimeapps_file(&mimeapps, Some(&ours)),
            None,
            "nothing again"
        );
    }

    #[test]
    fn a_link_for_an_open_launcher_is_handed_to_it_and_taken_once() {
        let data = std::env::temp_dir().join(format!("join-hand-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        assert!(
            !hand_off(&data, "dereth://join?host=a.example&port=9000"),
            "no launcher open: this one opens with the link"
        );
        assert!(
            !data.join(REQUEST_FILE).exists(),
            "and leaves nothing behind"
        );
        let running = Running::mark(&data);
        assert!(
            !hand_off_within(
                &data,
                "dereth://join?host=a.example&port=9000",
                Duration::ZERO
            ),
            "an open launcher that does not take it: this one opens with the link"
        );
        assert!(!data.join(REQUEST_FILE).exists(), "taken back");
        // The open launcher, in another thread, takes the link once it is there; the guard only
        // stops a hang.
        let taker = {
            let data = data.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                loop {
                    if let Some(taken) = join::take_request(&data) {
                        return taken;
                    }
                    assert!(started.elapsed() < Duration::from_secs(120), "nothing left");
                    std::thread::yield_now();
                }
            })
        };
        assert!(hand_off_within(
            &data,
            "dereth://join?host=a.example&port=9000&name=A",
            Duration::from_secs(120)
        ));
        assert_eq!(taker.join().unwrap().unwrap().name, "A");
        drop(running);
        assert!(
            !data.join(RUNNING_FILE).exists(),
            "the mark goes with the launcher"
        );
        let pending: Pending = Arc::default();
        offer(&pending, "dereth://join?host=b.example&port=9001&name=B");
        assert_eq!(lock(&pending).as_ref().map(|l| l.port), Some(9001));
        offer(&pending, "not a link");
        assert_eq!(
            lock(&pending).as_ref().map(|l| l.port),
            Some(9001),
            "a link that does not read changes nothing"
        );
        let _ = std::fs::remove_dir_all(&data);
    }
}
