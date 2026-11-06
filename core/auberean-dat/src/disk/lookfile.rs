//! Where the client looks for a data file: the folder it was given, then (when asked to look
//! further) the working folder, the folder the `PROJECT_DAT` variable names, and the folder a
//! registry value names, each with a slash added when it has none.

/// What the host answers when a file is looked for.
pub trait LookHost {
    /// Whether a file of this name (a folder and a file name, run together) opens for reading.
    fn exists(&self, name: &str) -> bool;
    /// The working folder.
    fn working_folder(&self) -> Option<String>;
    /// An environment variable's value.
    fn variable(&self, name: &str) -> Option<String>;
    /// A string value under the machine's registry key `key`.
    fn registry(&self, key: &str, value: &str) -> Option<String>;
}

/// The variable naming the data folder.
pub const PROJECT_DAT: &str = "PROJECT_DAT";

/// A search, and the folder it found the file in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSearch {
    found: String,
    /// The registry key and value naming a folder to look in last (none when either is empty).
    pub key: String,
    pub value: String,
}

/// `folder` with a slash at its end.
fn slashed(mut folder: String) -> String {
    if !folder.ends_with(['\\', '/']) {
        folder.push('/');
    }
    folder
}

impl FileSearch {
    /// A search that looks in the registry folder `key`/`value` last.
    #[must_use]
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            found: String::new(),
            key: key.into(),
            value: value.into(),
        }
    }

    /// The folder the file was found in.
    #[must_use]
    pub fn get_found_path(&self) -> String {
        self.found.clone()
    }

    /// Whether `file` is in `path` (the folder already found when `path` is empty). The folder and
    /// the name are run together as they are; a folder given while one is already found is not
    /// looked in. The folder is kept as the one found.
    pub fn is_file_here(&mut self, host: &dyn LookHost, file: &str, path: &str) -> bool {
        if file.is_empty() {
            return false;
        }
        let folder = if path.is_empty() {
            if self.found.is_empty() {
                return false;
            }
            self.found.clone()
        } else {
            if !self.found.is_empty() {
                return false;
            }
            path.to_string()
        };
        if !host.exists(&(folder.clone() + file)) {
            return false;
        }
        if self.found.is_empty() {
            self.found = path.to_string();
        }
        true
    }

    /// Look for `file`: in `path`, then, when `further`, in the working folder, the folder
    /// `PROJECT_DAT` names, and the registry's folder. Whether it was found (see
    /// [`FileSearch::get_found_path`]).
    pub fn look_for_file(
        &mut self,
        host: &dyn LookHost,
        file: &str,
        path: &str,
        further: bool,
    ) -> bool {
        if file.is_empty() {
            return false;
        }
        self.found.clear();
        if !path.is_empty() && self.is_file_here(host, file, path) {
            return true;
        }
        if !further {
            return false;
        }
        if let Some(cwd) = host.working_folder() {
            if self.is_file_here(host, file, &slashed(cwd)) {
                return true;
            }
        }
        if let Some(dir) = host.variable(PROJECT_DAT) {
            if self.is_file_here(host, file, &slashed(dir)) {
                return true;
            }
        }
        if !self.key.is_empty() && !self.value.is_empty() {
            if let Some(dir) = host.registry(&self.key, &self.value) {
                return self.is_file_here(host, file, &slashed(dir));
            }
        }
        false
    }
}
