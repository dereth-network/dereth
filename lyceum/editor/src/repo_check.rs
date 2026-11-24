//! The world repository's own check, as the server's tools make it (`empyrean-import world check
//! --json`): run after each save, off the frame loop, and before the editor's world starts, which
//! it refuses on an error. What it finds is listed in the Log tab, each going to its item in the
//! Rows tab when clicked.
//!
//! The check reads the repository as saved: an unsaved change is checked once it is saved.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::{EditorApp, UiAction};

/// One thing the repository's check found: its layer, how bad, its item and where it is written
/// when known, and what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub layer: String,
    pub severity: String,
    pub item: Option<String>,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub text: String,
}

impl Found {
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == "error"
    }
}

/// What a check found, and how many errors it counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub found: Vec<Found>,
    pub errors: usize,
}

/// The check's JSON lines read: each thing found, and the closing line's count of errors.
///
/// # Errors
/// A line that isn't one of its.
pub fn parse(out: &str) -> Result<Checked, String> {
    let mut c = Checked::default();
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("{line}: {e}"))?;
        if let Some(n) = v.get("errors").and_then(serde_json::Value::as_u64) {
            c.errors = usize::try_from(n).unwrap_or(usize::MAX);
            continue;
        }
        let s = |k: &str| v.get(k).and_then(serde_json::Value::as_str);
        c.found.push(Found {
            layer: s("layer").unwrap_or_default().to_owned(),
            severity: s("severity").unwrap_or_default().to_owned(),
            item: s("item").map(str::to_owned),
            file: s("file").map(PathBuf::from),
            line: v
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| usize::try_from(n).ok()),
            text: s("text").unwrap_or_default().to_owned(),
        });
    }
    Ok(c)
}

/// The check of the repository whose world file is `world`, by the import tool `import`.
///
/// # Errors
/// A tool that doesn't run, or answers what isn't the check's.
pub fn run(import: &Path, world: &Path) -> Result<Checked, String> {
    let mut cmd = std::process::Command::new(import);
    cmd.args(["world", "check", "--json", "--world"])
        .arg(world)
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("{}: {e}", import.display()))?;
    let checked = parse(&String::from_utf8_lossy(&out.stdout))?;
    if !out.status.success() && checked.errors == 0 {
        return Err(format!(
            "{}: {}",
            import.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(checked)
}

/// A check going on off the frame loop.
#[derive(Debug)]
pub struct Checking {
    rx: mpsc::Receiver<Result<Checked, String>>,
}

impl EditorApp {
    /// The import tool and the repository's world file, when the world is a repository and the
    /// tool is there.
    fn repo_check_parts(&self) -> Option<(PathBuf, PathBuf)> {
        if !self.repo_world {
            return None;
        }
        let import = crate::live::program(self.live_args.empyrean.as_deref(), "empyrean-import")?;
        Some((import, self.session.world().path.clone()))
    }

    /// Check the repository off the frame loop, as it is saved: after a save.
    pub(super) fn start_repo_check(&mut self) {
        let Some((import, world)) = self.repo_check_parts() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(run(&import, &world));
        });
        self.repo_checking = Some(Checking { rx });
    }

    /// A check's answer taken, when it has come: its findings listed.
    pub(super) fn poll_repo_check(&mut self) {
        let Some(r) = self
            .repo_checking
            .as_ref()
            .and_then(|c| c.rx.try_recv().ok())
        else {
            return;
        };
        self.repo_checking = None;
        self.took_repo_check(r);
    }

    fn took_repo_check(&mut self, r: Result<Checked, String>) {
        match r {
            Ok(c) => {
                self.panels.message = self.say_with(
                    "messages.repository_checked",
                    &[("errors", &c.errors), ("n", &c.found.len())],
                );
                self.repo_found = Some(c);
            }
            Err(e) => self.panels.message = e,
        }
    }

    /// The repository checked now, waited for, before its world starts: refused on an error, its
    /// findings in the Log.
    ///
    /// # Errors
    /// The check's errors, or a check that didn't run.
    pub(super) fn repo_check_now(&mut self) -> Result<(), String> {
        let Some((import, world)) = self.repo_check_parts() else {
            return Ok(());
        };
        let c = run(&import, &world)?;
        let errors = c.errors;
        self.took_repo_check(Ok(c));
        if errors > 0 {
            return Err(self.say_with("messages.repository_errors", &[("n", &errors)]));
        }
        Ok(())
    }

    /// The Log tab's list of what the repository's check found, each going to its item in the
    /// Rows tab when clicked.
    pub(super) fn repo_findings(&mut self, ui: &mut egui::Ui, _out: &mut Vec<UiAction>) {
        let Some(c) = &self.repo_found else {
            return;
        };
        let t = self.text.table();
        let head = t.fill(
            "rows.check_head",
            &[("errors", &c.errors), ("n", &c.found.len())],
        );
        let mut go = None;
        egui::CollapsingHeader::new(head)
            .id_salt("repository-check")
            .default_open(c.errors > 0)
            .show(ui, |ui| {
                for f in &c.found {
                    let color = if f.is_error() {
                        super::panels::mark::FAULT
                    } else {
                        super::panels::mark::WARN
                    };
                    let r = ui.selectable_label(
                        false,
                        egui::RichText::new(format!("{}: {}", f.layer, f.text)).color(color),
                    );
                    let r = match (&f.file, f.line) {
                        (Some(file), Some(line)) => {
                            r.on_hover_text(format!("{}:{line}", file.display()))
                        }
                        _ => r,
                    };
                    if r.clicked() {
                        go = f.item.clone();
                    }
                }
            });
        if let Some(item) = go {
            item.clone_into(&mut self.rows.filter);
            self.rows.selected = Some(item);
            self.open_rows();
        }
        ui.separator();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world repository's check read by the world editor)
    use super::*;

    /// The check's lines are read into what it found, each with its layer, item and place when it
    /// has them, and the closing line's count of errors.
    #[test]
    fn the_repositorys_check_is_read_from_its_json_lines() {
        let out = r#"{"layer":"base","severity":"warning","item":"weenie 11","file":"base/weenie/11.toml","line":3,"text":"weenie 11 int.Value: changes nothing below"}
{"layer":"2027-01-fixes","severity":"error","item":"spawn 0x7A9B4002","text":"spawn 0x7A9B4002: class 4000 is in no weenie"}
{"layers":2,"quiet":0,"errors":1}
"#;
        let c = parse(out).unwrap();
        assert_eq!(c.errors, 1);
        assert_eq!(c.found.len(), 2);
        assert_eq!(c.found[0].item.as_deref(), Some("weenie 11"));
        assert_eq!(c.found[0].line, Some(3));
        assert!(!c.found[0].is_error() && c.found[1].is_error());
        assert_eq!(c.found[1].file, None);
        assert!(parse("not json").is_err());
    }
}
