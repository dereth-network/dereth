//! Every word Lyceum shows, from a file the creator can edit: `lyceum-text.toml`, beside its
//! settings.
//!
//! The file is a TOML table of tables, one a panel (`[top]`, `[inspector]`, `[dungeon]` and so
//! on), each line one piece of text: `onto_ground = "Onto the ground"`. A text that names
//! something it is shown with says so in braces, `"{n} of {all}"`, and the braces are filled in.
//!
//! The editor is built with its own copy ([`BUILT_IN`]), which it writes out as the file the first
//! time it runs. After that the file is the creator's: a line it lacks takes the built-in text, a
//! file that does not read says which line, and the built-in text stands until it is put right;
//! and a change to the file is read again while the editor runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

/// The file's name, in the editor's settings folder.
pub const FILE_NAME: &str = "lyceum-text.toml";

/// The text the editor is built with: every piece of text, in English.
pub const BUILT_IN: &str = include_str!("../lyceum-text.toml");

/// How often the file is looked at for a change.
const LOOK_EVERY: Duration = Duration::from_millis(500);

/// One reading of the text: the creator's, over the built-in.
#[derive(Debug, Default)]
pub struct Table {
    built_in: Rc<HashMap<String, String>>,
    own: HashMap<String, String>,
}

impl Table {
    /// The text of `key` (`panel.name`): the file's, else the built-in, else the key itself.
    #[must_use]
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.own
            .get(key)
            .or_else(|| self.built_in.get(key))
            .map_or(key, String::as_str)
    }

    /// The text of `key` with each `{name}` in it filled in from `values`.
    #[must_use]
    pub fn fill(&self, key: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
        let mut s = self.get(key).to_owned();
        for (name, value) in values {
            s = s.replace(&format!("{{{name}}}"), &value.to_string());
        }
        s
    }

    /// The text of `key` when there is one, else `fallback`: for names that come with the data
    /// (a category, a field), which the file may rename.
    #[must_use]
    pub fn or<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.own
            .get(key)
            .or_else(|| self.built_in.get(key))
            .map_or(fallback, String::as_str)
    }

    /// Whether `key` has a text of its own, the file's or the built-in.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.own.contains_key(key) || self.built_in.contains_key(key)
    }
}

/// The editor's text and the file it is read from.
#[derive(Debug)]
pub struct Text {
    built_in: Rc<HashMap<String, String>>,
    table: Rc<Table>,
    file: Option<PathBuf>,
    /// The file as it stood when it was read last: its time and length.
    stamp: Option<(SystemTime, u64)>,
    looked: Option<Instant>,
    /// What was wrong with the file when it was read last, to show the creator.
    pub problem: Option<String>,
}

impl Default for Text {
    fn default() -> Self {
        Self::built_in()
    }
}

/// `text` read as the file is: every `[panel]`'s `name = "text"`, as `panel.name`.
///
/// # Errors
/// Where the text does not read as TOML: its line, and why.
pub fn read(text: &str) -> Result<HashMap<String, String>, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| {
        let line = e.span().map_or(0, |s| {
            text[..s.start.min(text.len())].matches('\n').count() + 1
        });
        format!("{FILE_NAME}, line {line}: {}", e.message())
    })?;
    let mut out = HashMap::new();
    flatten(&table, "", &mut out);
    Ok(out)
}

fn flatten(t: &toml::Table, prefix: &str, out: &mut HashMap<String, String>) {
    for (k, v) in t {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            toml::Value::Table(inner) => flatten(inner, &key, out),
            toml::Value::String(s) => {
                out.insert(key, s.clone());
            }
            other => {
                out.insert(key, other.to_string());
            }
        }
    }
}

impl Text {
    /// The built-in text alone, with no file: a scripted run's, or a test's.
    #[must_use]
    pub fn built_in() -> Self {
        let built_in = Rc::new(read(BUILT_IN).unwrap_or_default());
        Self {
            table: Rc::new(Table {
                built_in: Rc::clone(&built_in),
                own: HashMap::new(),
            }),
            built_in,
            file: None,
            stamp: None,
            looked: None,
            problem: None,
        }
    }

    /// The text of file `path`, which is written with the built-in text first when it is not
    /// there.
    #[must_use]
    pub fn open(path: &Path) -> Self {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(path, BUILT_IN) {
                tracing::warn!("{}: {e}", path.display());
            }
        }
        let mut t = Self {
            file: Some(path.to_path_buf()),
            ..Self::built_in()
        };
        t.read_file();
        t
    }

    /// This reading of the text, to look pieces up in.
    #[must_use]
    pub fn table(&self) -> Rc<Table> {
        Rc::clone(&self.table)
    }

    /// The file the text is read from, when there is one.
    #[must_use]
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
        let m = std::fs::metadata(path).ok()?;
        Some((m.modified().ok()?, m.len()))
    }

    fn read_file(&mut self) {
        let Some(path) = self.file.clone() else {
            return;
        };
        self.stamp = Self::stamp_of(&path);
        let own = match std::fs::read_to_string(&path) {
            Ok(text) => read(&text),
            // A file that has gone: the built-in text, said once.
            Err(e) => Err(format!("{}: {e}", path.display())),
        };
        let built_in = Rc::clone(&self.built_in);
        match own {
            Ok(own) => {
                self.problem = None;
                self.table = Rc::new(Table { built_in, own });
            }
            Err(e) => {
                tracing::warn!("{e}");
                self.problem = Some(e);
                self.table = Rc::new(Table {
                    built_in,
                    own: HashMap::new(),
                });
            }
        }
    }

    /// Read the file again if it has changed since it was read last, looking no more than twice
    /// a second; whether it was read again.
    pub fn refresh(&mut self) -> bool {
        let now = Instant::now();
        if self
            .looked
            .is_some_and(|t| now.duration_since(t) < LOOK_EVERY)
        {
            return false;
        }
        self.looked = Some(now);
        self.refresh_now()
    }

    /// Read the file again now if it has changed; whether it was read again.
    pub fn refresh_now(&mut self) -> bool {
        let Some(path) = self.file.as_deref() else {
            return false;
        };
        if Self::stamp_of(path) == self.stamp {
            return false;
        }
        self.read_file();
        true
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's text file)
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lyceum-text-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_built_in_text_reads_and_every_piece_has_a_panel() {
        let t = read(BUILT_IN).unwrap();
        assert!(t.len() > 100, "{}", t.len());
        assert!(t.keys().all(|k| k.contains('.')), "a piece outside a panel");
    }

    #[test]
    fn every_piece_of_text_the_editor_names_is_in_the_built_in_file() {
        let built_in = read(BUILT_IN).unwrap();
        let panels: std::collections::BTreeSet<&str> = built_in
            .keys()
            .filter_map(|k| k.split_once('.'))
            .map(|(p, _)| p)
            .collect();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        for e in std::fs::read_dir(&src).unwrap() {
            let path = e.unwrap().path();
            let code = std::fs::read_to_string(&path).unwrap();
            let code = code.split("#[cfg(test)]").next().unwrap_or_default();
            for piece in code.split('"').skip(1).step_by(2) {
                let Some((panel, name)) = piece.split_once('.') else {
                    continue;
                };
                let a_key = name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && !name.is_empty()
                    && name != "rs";
                if a_key && panels.contains(panel) && !built_in.contains_key(piece) {
                    missing.push(format!("{}: {piece}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "{missing:#?}");
    }

    #[test]
    fn the_file_is_written_on_first_run_and_its_own_text_is_shown() {
        let dir = scratch("first");
        let path = dir.join(FILE_NAME);
        let t = Text::open(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), BUILT_IN);
        assert_eq!(t.table().get("top.save"), "Save");
        std::fs::write(&path, "[top]\nsave = \"Keep\"\n").unwrap();
        let t = Text::open(&path);
        assert_eq!(t.table().get("top.save"), "Keep");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_piece_the_file_lacks_takes_the_built_in_text() {
        let dir = scratch("lacks");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "[top]\nsave = \"Keep\"\n").unwrap();
        let t = Text::open(&path).table();
        assert_eq!(t.get("top.undo"), "Undo");
        assert_eq!(t.get("no.such_piece"), "no.such_piece");
        assert_eq!(t.or("categories.Nothing", "Nothing"), "Nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_does_not_read_says_which_line_and_the_built_in_text_stands() {
        let dir = scratch("bad");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "[top]\nsave = \"Keep\"\nundo = Undo it\n").unwrap();
        let t = Text::open(&path);
        let problem = t.problem.clone().unwrap();
        assert!(problem.contains("line 3"), "{problem}");
        assert_eq!(t.table().get("top.save"), "Save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_to_the_file_is_read_again_and_a_text_is_filled_in() {
        let dir = scratch("again");
        let path = dir.join(FILE_NAME);
        let mut t = Text::open(&path);
        assert!(!t.refresh_now(), "nothing changed");
        std::fs::write(
            &path,
            "[checks]\ncount = \"{faults} wrong, {warnings} to look at\"\n",
        )
        .unwrap();
        assert!(t.refresh_now());
        let table = t.table();
        assert_eq!(
            table.fill("checks.count", &[("faults", &2), ("warnings", &5)]),
            "2 wrong, 5 to look at"
        );
        // Put wrong, then right again.
        std::fs::write(&path, "[checks\n").unwrap();
        assert!(t.refresh_now());
        assert!(t.problem.as_deref().is_some_and(|p| p.contains("line 1")));
        std::fs::write(&path, "[top]\nsave = \"Keep it\"\n").unwrap();
        assert!(t.refresh_now());
        assert_eq!(
            (t.problem.as_deref(), t.table().get("top.save")),
            (None, "Keep it")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
