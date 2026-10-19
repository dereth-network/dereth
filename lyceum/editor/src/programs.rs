//! Where the editor finds the programs it runs: Empyrean's, for its own world, and Dereth's
//! client, for Play. Each is looked up on its own, so that one can go without the other.
//!
//! Each is taken from the command line when it names one (`--empyrean`, `--client`), else found in
//! this order:
//!
//! 1. **beside the editor**, as a build of the workspace has them;
//! 2. **the setting**, `programs.toml` in the editor's settings folder ([`Setting`]);
//! 3. for the client, **the Dereth install** ([`installed_clients`]).
//!
//! Empyrean's release is a folder its operator unpacks anywhere, which nothing records, so it is
//! found beside the editor or by the setting. When one is still missing, the editor says which, in
//! one line naming the setting's file ([`missing`]), and writes that file with its lines ready to
//! fill in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The setting's file, in the editor's settings folder.
pub const FILE_NAME: &str = "programs.toml";

/// What the setting's file holds when the editor first writes it.
const TEMPLATE: &str = "\
# Where Lyceum finds the programs it runs when they are not beside it.
# Take the # off a line and give the path.

# Dereth's client, which Play starts:
# client = \"/path/to/dereth-client\"

# The folder Empyrean's programs (empyrean-server and empyrean-import) are in, which runs the
# editor's own world:
# empyrean = \"/path/to/empyrean\"
";

/// The setting: other copies of the programs, named by the person using the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setting {
    /// Dereth's client program.
    pub client: Option<PathBuf>,
    /// The folder Empyrean's programs are in.
    pub empyrean: Option<PathBuf>,
}

impl Setting {
    /// The setting `text` holds: its `client` and `empyrean` lines; an empty or missing line, or
    /// text that does not read, names nothing.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let table: toml::Table = text.parse().unwrap_or_default();
        let path = |key: &str| {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
        };
        Self {
            client: path("client"),
            empyrean: path("empyrean"),
        }
    }

    /// The setting in `file`; none when there is no file.
    #[must_use]
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .map(|t| Self::read(&t))
            .unwrap_or_default()
    }
}

/// The setting's file in the editor's settings folder.
#[must_use]
pub fn setting_file() -> Option<PathBuf> {
    dereth_desktop::folders::default_settings_dir(
        <crate::Lyceum as dereth_desktop::Product>::SETTINGS_DIR_NAME,
    )
    .map(|d| d.join(FILE_NAME))
}

/// A program's file name on this system.
fn program_file(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The folder the editor's program is in.
fn editor_folder() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// Whether `dir` holds Empyrean's two programs.
fn holds_empyrean(dir: &Path) -> bool {
    ["empyrean-server", "empyrean-import"]
        .iter()
        .all(|p| dir.join(program_file(p)).is_file())
}

/// Empyrean's folder: beside the editor (`beside`, the editor's folder), else the setting's.
#[must_use]
pub fn find_empyrean(beside: Option<&Path>, setting: &Setting) -> Option<PathBuf> {
    beside
        .filter(|d| holds_empyrean(d))
        .map(Path::to_path_buf)
        .or_else(|| setting.empyrean.clone().filter(|d| holds_empyrean(d)))
}

/// Dereth's client: beside the editor (`beside`, the editor's folder), else the setting's, else
/// the first of `installed` that is there.
#[must_use]
pub fn find_client(
    beside: Option<&Path>,
    setting: &Setting,
    installed: &[PathBuf],
) -> Option<PathBuf> {
    beside
        .map(|d| d.join(program_file("dereth-client")))
        .filter(|p| p.is_file())
        .or_else(|| setting.client.clone().filter(|p| p.is_file()))
        .or_else(|| installed.iter().find(|p| p.is_file()).cloned())
}

/// The Dereth launcher's application id, which names its desktop entry on Linux.
const DERETH_APP_ID: &str = "network.dereth.dereth";

/// Where the Dereth install keeps its client, as Dereth's own release and launcher leave a record
/// of it on each system, `var` reading the environment:
///
/// - **macOS:** `Dereth.app`, in the Applications folder its release is moved to (the system's,
///   then the user's), holds the client in `Contents/MacOS`.
/// - **Linux:** the launcher writes its desktop entry on every start, naming what starts it. A
///   launcher run from its folder has the client beside it; an AppImage carries its client inside
///   it, where no other program can start it.
/// - **Windows:** the release is a folder unpacked anywhere, and nothing records where.
#[must_use]
pub fn installed_clients(var: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = var("HOME").map(PathBuf::from);
        let mut apps = vec![PathBuf::from("/Applications")];
        apps.extend(home.map(|h| h.join("Applications")));
        return apps
            .into_iter()
            .map(|a| a.join("Dereth.app/Contents/MacOS/dereth-client"))
            .collect();
    }
    if cfg!(target_os = "linux") {
        let data_home = var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|d| d.is_absolute())
            .or_else(|| {
                var("HOME")
                    .map(PathBuf::from)
                    .map(|h| h.join(".local/share"))
            });
        let entry = data_home.map(|d| {
            d.join("applications")
                .join(format!("{DERETH_APP_ID}.desktop"))
        });
        return entry
            .and_then(|e| std::fs::read_to_string(e).ok())
            .and_then(|text| launcher_folder(&text))
            .map(|dir| vec![dir.join("dereth-client")])
            .unwrap_or_default();
    }
    Vec::new()
}

/// The folder of the launcher a desktop entry's `Exec` line starts, when it starts the launcher
/// itself rather than an AppImage.
#[must_use]
pub fn launcher_folder(entry: &str) -> Option<PathBuf> {
    let exec = entry.lines().find_map(|l| l.strip_prefix("Exec="))?.trim();
    // The launcher writes the path quoted, its `"`, `` ` `` and `$` escaped, its backslashes
    // doubled twice and its percent signs doubled.
    let inner = exec.strip_prefix('"')?.strip_suffix('"')?;
    let path = inner
        .replace("\\\\\\\\", "\u{0}")
        .replace("\\\\", "")
        .replace('\u{0}', "\\")
        .replace("%%", "%");
    let path = PathBuf::from(path);
    let appimage = path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("appimage"));
    if appimage {
        return None;
    }
    path.parent().map(Path::to_path_buf)
}

/// The setting as its file has it now.
fn current_setting() -> Setting {
    setting_file()
        .map(|f| Setting::load(&f))
        .unwrap_or_default()
}

/// Fill in Empyrean's folder when the command line named none, as [`find_empyrean`] finds it.
pub fn fill_empyrean(empyrean: &mut Option<PathBuf>) {
    if empyrean.is_none() {
        *empyrean = find_empyrean(editor_folder().as_deref(), &current_setting());
    }
}

/// Fill in Dereth's client when the command line named none, as [`find_client`] finds it.
pub fn fill_client(client: &mut Option<PathBuf>) {
    if client.is_none() {
        let installed = installed_clients(&|v| std::env::var_os(v));
        *client = find_client(editor_folder().as_deref(), &current_setting(), &installed);
    }
}

/// Before the editor's world starts, or Play (`wants_client`): fill in what the command line did
/// not name, and when something is still missing, the editor's one line about it, in `text`.
///
/// # Errors
/// What is missing, and where to set it.
pub fn ready(
    client: &mut Option<PathBuf>,
    empyrean: &mut Option<PathBuf>,
    text: &crate::text::Table,
    wants_client: bool,
) -> Result<(), String> {
    fill_empyrean(empyrean);
    if wants_client {
        fill_client(client);
    }
    match missing(
        text,
        setting_file().as_deref(),
        client.as_deref(),
        empyrean.as_deref(),
        wants_client,
    ) {
        Some(said) => Err(said),
        None => Ok(()),
    }
}

/// What a run of the editor's world, or Play (`wants_client`), needs and does not have, in one
/// line of the editor's `text` naming the setting's `file`: `None` when it has both. The file is
/// written, ready to fill in, when it is not there.
#[must_use]
pub fn missing(
    text: &crate::text::Table,
    file: Option<&Path>,
    client: Option<&Path>,
    empyrean: Option<&Path>,
    wants_client: bool,
) -> Option<String> {
    let key = match (wants_client && client.is_none(), empyrean.is_none()) {
        (false, false) => return None,
        (true, false) => "missing.client",
        (false, true) => "missing.empyrean",
        (true, true) => "missing.both",
    };
    if let Some(f) = file.filter(|f| !f.exists()) {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(f, TEMPLATE) {
            tracing::warn!("{}: {e}", f.display());
        }
    }
    let shown = file.map_or_else(|| FILE_NAME.to_owned(), |f| f.display().to_string());
    Some(text.fill(key, &[("file", &shown)]))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: where the world editor finds the programs it runs)
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lyceum-programs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }

    #[test]
    fn the_setting_names_each_program_and_an_empty_line_names_nothing() {
        let s = Setting::read("client = \"C:/Dereth/dereth-client.exe\"\nempyrean = \" \"\n");
        assert_eq!(s.client, Some(PathBuf::from("C:/Dereth/dereth-client.exe")));
        assert_eq!(s.empyrean, None);
        assert_eq!(Setting::read(TEMPLATE), Setting::default());
        assert_eq!(Setting::read("not toml ["), Setting::default());
    }

    #[test]
    fn the_client_beside_the_editor_wins_then_the_setting_then_the_install() {
        let d = scratch("client");
        let beside = d.join("editor");
        let set = d.join("set").join(program_file("dereth-client"));
        let installed = d.join("installed").join(program_file("dereth-client"));
        let setting = Setting {
            client: Some(set.clone()),
            empyrean: None,
        };
        let all = [installed.clone()];
        let find = || find_client(Some(&beside), &setting, &all);
        assert_eq!(find(), None, "none of them there");
        touch(&installed);
        assert_eq!(find(), Some(installed.clone()));
        touch(&set);
        assert_eq!(find(), Some(set.clone()), "the setting over the install");
        touch(&beside.join(program_file("dereth-client")));
        assert_eq!(
            find(),
            Some(beside.join(program_file("dereth-client"))),
            "beside the editor over both"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn empyrean_beside_the_editor_wins_then_the_setting_and_needs_both_programs() {
        let d = scratch("empyrean");
        let beside = d.join("editor");
        let set = d.join("server");
        let setting = Setting {
            client: None,
            empyrean: Some(set.clone()),
        };
        assert_eq!(find_empyrean(Some(&beside), &setting), None);
        touch(&set.join(program_file("empyrean-server")));
        assert_eq!(
            find_empyrean(Some(&beside), &setting),
            None,
            "the importer too"
        );
        touch(&set.join(program_file("empyrean-import")));
        assert_eq!(find_empyrean(Some(&beside), &setting), Some(set.clone()));
        touch(&beside.join(program_file("empyrean-server")));
        touch(&beside.join(program_file("empyrean-import")));
        assert_eq!(find_empyrean(Some(&beside), &setting), Some(beside.clone()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_dereth_install_is_found_where_its_release_and_launcher_record_it() {
        let env = |v: &str| match v {
            "HOME" => Some(OsString::from("/home/p")),
            _ => None,
        };
        let found = installed_clients(&env);
        if cfg!(target_os = "macos") {
            assert_eq!(
                found,
                [
                    PathBuf::from("/Applications/Dereth.app/Contents/MacOS/dereth-client"),
                    PathBuf::from("/home/p/Applications/Dereth.app/Contents/MacOS/dereth-client"),
                ]
            );
        } else if cfg!(windows) {
            assert!(found.is_empty(), "nothing records a Windows install");
        }
        // The launcher's own desktop entry: a launcher run from its folder has the client beside
        // it; an AppImage keeps its client inside.
        let entry = |exec: &str| format!("[Desktop Entry]\nType=Application\nExec={exec}\n");
        assert_eq!(
            launcher_folder(&entry("\"/opt/Dereth 1/dereth\"")),
            Some(PathBuf::from("/opt/Dereth 1"))
        );
        assert_eq!(
            launcher_folder(&entry("\"/opt/100%%/dereth\"")),
            Some(PathBuf::from("/opt/100%"))
        );
        assert_eq!(
            launcher_folder(&entry(
                "\"/home/p/Apps/Dereth-0.4.4-linux-x86_64.AppImage\""
            )),
            None
        );
        assert_eq!(launcher_folder("[Desktop Entry]\n"), None);
    }

    #[test]
    fn what_is_missing_is_said_in_one_line_naming_the_settings_file() {
        let d = scratch("missing");
        let file = d.join(FILE_NAME);
        let t = crate::text::Text::built_in();
        let t = t.table();
        let here = Path::new("here");
        let missing = |c, e, w| missing(&t, Some(&file), c, e, w);
        assert_eq!(missing(Some(here), Some(here), true), None);
        assert_eq!(
            missing(None, Some(here), false),
            None,
            "Live needs no client"
        );
        assert!(!file.exists(), "nothing written while nothing is missing");
        for (client, empyrean, key) in [
            (None, Some(here), "missing.client"),
            (Some(here), None, "missing.empyrean"),
            (None, None, "missing.both"),
        ] {
            let said = missing(client, empyrean, true).expect("missing");
            assert!(!said.contains('\n'), "{said}");
            assert!(said.contains(FILE_NAME), "{said}");
            assert!(t.has(key), "{key}");
            assert!(!said.contains("{file}"), "{said}");
        }
        assert!(missing(None, Some(here), true)
            .unwrap()
            .contains("Dereth's client"));
        assert!(missing(Some(here), None, true)
            .unwrap()
            .contains("Empyrean"));
        // The file it names is there to fill in, and reads as naming nothing until it is.
        let written = std::fs::read_to_string(&file).expect("the setting's file");
        assert_eq!(Setting::read(&written), Setting::default());
        std::fs::write(&file, "client = \"/x/dereth-client\"\n").unwrap();
        let _ = missing(None, None, true);
        assert_eq!(
            Setting::load(&file).client,
            Some(PathBuf::from("/x/dereth-client")),
            "a file there is never written over"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
