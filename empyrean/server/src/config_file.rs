//! Not ACE: which configuration file the server reads (TOML only).
//!
//! The configuration is `empyrean.toml`. Without `--config`, the server looks for it in the working
//! directory, then beside the executable, and runs on the defaults when there is none.
//! `--config <path>` names the file instead; a bare name is looked for in the working directory,
//! then beside the executable, as ACE's `ConfigManager` does.
//!
//! ACE's `Config.js` is not read. When the server would otherwise run on the defaults and finds a
//! `Config.js`, or `--config` names a `.js` file, it stops with a message naming the one-time
//! converter (`empyrean-server --write-config --from <Config.js>`). No environment variable
//! configures the server; the dat folder is `server.dat_files_directory` only
//! ([`crate::dat_directory`]).

use std::path::{Path, PathBuf};

use empyrean_common::config_manager::{ConfigError, ConfigManager};
use empyrean_common::toml_config::{self, Parsed};

/// The configuration file's name.
pub const NATIVE_FILE_NAME: &str = "empyrean.toml";

/// ACE's configuration file's name (read only by the converter).
pub const ACE_FILE_NAME: &str = "Config.js";

/// Where the configuration comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// An `empyrean.toml`.
    File(PathBuf),
    /// No file: the defaults.
    Defaults,
}

/// The result of [`discover`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// The configuration to load.
    pub source: ConfigSource,
    /// Other configuration files found that are not read (an `empyrean.toml` beside the executable
    /// shadowed by one in the working directory, or a `Config.js`).
    pub ignored: Vec<PathBuf>,
}

/// Why no configuration could be chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverError {
    /// `--config` names a file that does not exist: the path looked at.
    Missing(PathBuf),
    /// The configuration found is ACE's `Config.js`, which must be converted first.
    ConfigJs(PathBuf),
}

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => {
                write!(f, "Configuration file {} does not exist.", path.display())
            }
            Self::ConfigJs(path) => f.write_str(&converter_message(path)),
        }
    }
}

/// The message for a `Config.js` found where the configuration was expected.
#[must_use]
pub fn converter_message(path: &Path) -> String {
    format!(
        "{} is ACE's {ACE_FILE_NAME}, which Empyrean does not read: its configuration is {NATIVE_FILE_NAME}. \
         Convert it once with `empyrean-server --write-config --from {} --out {NATIVE_FILE_NAME}`, check the result, and start the server again.",
        path.display(),
        path.display()
    )
}

/// Whether `path` names a `Config.js`-style file (a `.js` extension, any case).
fn is_config_js(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("js"))
}

/// Finds the configuration. `explicit` is `--config`'s path; `exists` stands for the file system.
///
/// # Errors
/// When `explicit` names a file that does not exist or a `.js` file, or when no `empyrean.toml`
/// exists but a `Config.js` does.
pub fn discover(
    explicit: Option<&str>,
    current_dir: &Path,
    exe_dir: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Discovery, DiscoverError> {
    if let Some(path) = explicit {
        let path = ConfigManager::resolve_path(path, current_dir, exe_dir, exists);
        if !exists(&path) {
            return Err(DiscoverError::Missing(path));
        }
        if is_config_js(&path) {
            return Err(DiscoverError::ConfigJs(path));
        }
        return Ok(Discovery {
            source: ConfigSource::File(path),
            ignored: Vec::new(),
        });
    }

    let dirs: Vec<&Path> = std::iter::once(current_dir).chain(exe_dir).collect();
    let found = |name: &str| {
        let mut paths: Vec<PathBuf> = Vec::new();
        for dir in &dirs {
            let path = dir.join(name);
            // The working directory may be the executable's directory: count it once.
            if exists(&path) && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    };
    let mut tomls = found(NATIVE_FILE_NAME).into_iter();
    let config_js = found(ACE_FILE_NAME);
    match tomls.next() {
        Some(path) => Ok(Discovery {
            source: ConfigSource::File(path),
            ignored: tomls.chain(config_js).collect(),
        }),
        None => match config_js.into_iter().next() {
            Some(js) => Err(DiscoverError::ConfigJs(js)),
            None => Ok(Discovery {
                source: ConfigSource::Defaults,
                ignored: Vec::new(),
            }),
        },
    }
}

/// Parses an `empyrean.toml` text.
///
/// # Errors
/// When the text is not a valid configuration.
pub fn parse(text: &str) -> Result<Parsed, ConfigError> {
    toml_config::from_toml_str(text)
}

/// Reads and parses an `empyrean.toml`.
///
/// # Errors
/// When the file cannot be read or is not a valid configuration.
pub fn load(path: &Path) -> Result<Parsed, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
    parse(&text)
}

/// The log level `server.log_level` names: `error`, `warn`, `info`, `debug` or `trace` (any case,
/// trimmed). `None` for anything else (the caller warns and logs at `info`).
#[must_use]
pub fn log_level(value: &str) -> Option<log::LevelFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "error" => Some(log::LevelFilter::Error),
        "warn" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}
