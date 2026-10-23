//! The separate server programs bundled with the editor.
use std::path::{Path, PathBuf};

/// Packaged builds select only the server shipped beside the editor.
pub const BUNDLED_SERVER: bool = option_env!("LYCEUM_BUNDLED_SERVER").is_some();

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

/// Empyrean's two programs beside the editor. Settings never select another installed server.
#[must_use]
pub fn find_empyrean(beside: Option<&Path>) -> Option<PathBuf> {
    beside.filter(|d| holds_empyrean(d)).map(Path::to_path_buf)
}

fn select_empyrean(
    beside: Option<&Path>,
    explicit: Option<&Path>,
    bundled: bool,
) -> Option<PathBuf> {
    if bundled {
        find_empyrean(beside)
    } else {
        explicit
            .map(Path::to_path_buf)
            .or_else(|| find_empyrean(beside))
    }
}

/// Select the bundled server, or the workspace build for development.
pub fn fill_empyrean(empyrean: &mut Option<PathBuf>) {
    *empyrean = select_empyrean(
        editor_folder().as_deref(),
        empyrean.as_deref(),
        BUNDLED_SERVER,
    );
}

/// Resolve the separate server and importer.
///
/// # Errors
/// The bundled programs are missing.
pub fn ready(empyrean: &mut Option<PathBuf>, text: &crate::text::Table) -> Result<(), String> {
    fill_empyrean(empyrean);
    if empyrean.is_none() {
        return Err(text.get("missing.empyrean").to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: bundled server selection)
    use super::*;
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lyceum-programs-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }
    #[test]
    fn a_release_uses_only_its_bundled_server_and_a_development_run_can_name_its_build() {
        let d = scratch("empyrean");
        let beside = d.join("editor");
        let other = d.join("another-build");
        for name in ["empyrean-server", "empyrean-import"] {
            touch(&other.join(program_file(name)));
        }
        assert_eq!(select_empyrean(Some(&beside), Some(&other), true), None);
        assert_eq!(
            select_empyrean(Some(&beside), Some(&other), false),
            Some(other)
        );
        touch(&beside.join(program_file("empyrean-server")));
        assert_eq!(
            find_empyrean(Some(&beside)),
            None,
            "the importer is required too"
        );
        touch(&beside.join(program_file("empyrean-import")));
        assert_eq!(
            select_empyrean(Some(&beside), Some(Path::new("ignored")), true),
            Some(beside.clone())
        );
        assert_eq!(select_empyrean(Some(&beside), None, false), Some(beside));
        let _ = std::fs::remove_dir_all(&d);
    }
}
