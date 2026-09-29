// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/ConfigManager.cs
//! `ConfigManager`: the process-wide [`MasterConfiguration`].
//!
//! DIVERGE: the server reads its configuration from `empyrean.toml` ([`crate::toml_config`]);
//! `Config.js` is read only by the one-time converter (`empyrean-server --write-config --from
//! <Config.js>`), through [`ConfigManager::deserialize`].
//!
//! The JSON options are ACE's `SerializerOptions` (comments skipped, trailing commas allowed,
//! numbers readable from strings, indented output); see [`crate::json`].
//!
//! DIVERGE: ACE writes its load errors to the console and rethrows; here they are logged with the
//! `log` crate (ACE's log4net setup is not ported) and returned as [`ConfigError`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::json;
use crate::master_configuration::MasterConfiguration;

/// Why a configuration could not be loaded.
#[derive(Debug)]
pub enum ConfigError {
    /// The file does not exist (`"missing configuration file"` in ACE).
    Missing(PathBuf),
    /// The file could not be read.
    Io(std::io::Error),
    /// The JSON did not match `MasterConfiguration`.
    Json(serde_json::Error),
    /// Not ACE: `empyrean.toml` was not TOML, or did not match `MasterConfiguration`
    /// ([`crate::toml_config`]).
    Toml(toml::de::Error),
    /// Not ACE: the configuration found is ACE's `Config.js`, which the server does not read; it
    /// is converted once with `empyrean-server --write-config --from <Config.js>`.
    ConfigJs(PathBuf),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(_) => f.write_str("missing configuration file"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::Toml(e) => write!(f, "{e}"),
            Self::ConfigJs(path) => write!(
                f,
                "{} is ACE's Config.js, which is not read: convert it once with `empyrean-server --write-config --from {}`",
                path.display(),
                path.display()
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Missing(_) | Self::ConfigJs(_) => None,
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Toml(e) => Some(e),
        }
    }
}

static CONFIG: RwLock<Option<Arc<MasterConfiguration>>> = RwLock::new(None);

thread_local! {
    /// Not ACE: a configuration that replaces [`CONFIG`] on this thread only. A test world
    /// runs on its test's thread, so a test that needs a non-default configuration installs one
    /// here ([`ConfigManager::override_for_thread`]) instead of changing the process-wide one that
    /// every parallel test reads.
    static OVERRIDE: std::cell::RefCell<Option<Arc<MasterConfiguration>>> = const { std::cell::RefCell::new(None) };
}

/// Not ACE: restores the thread's previous configuration override when dropped (see
/// [`ConfigManager::override_for_thread`]).
#[derive(Debug)]
#[must_use = "the override ends when the guard is dropped"]
pub struct ConfigOverride {
    previous: Option<Arc<MasterConfiguration>>,
}

impl Drop for ConfigOverride {
    fn drop(&mut self) {
        let previous = self.previous.take();
        OVERRIDE.with(|o| *o.borrow_mut() = previous);
    }
}

/// `ConfigManager` (a static class in ACE).
#[derive(Debug)]
pub struct ConfigManager;

impl ConfigManager {
    // ACE: ConfigManager.Config
    /// `ConfigManager.Config`.
    ///
    /// # Panics
    /// Before any `initialize` (ACE's `Config` is `null` then, and every read throws).
    #[must_use]
    pub fn config() -> Arc<MasterConfiguration> {
        if let Some(config) = OVERRIDE.with(|o| o.borrow().clone()) {
            return config;
        }
        CONFIG
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect(
                "NullReferenceException: ConfigManager.Config read before ConfigManager.Initialize",
            )
    }

    /// Not ACE: [`config`](Self::config), or `None` before any `initialize` instead of a panic.
    #[must_use]
    pub fn try_config() -> Option<Arc<MasterConfiguration>> {
        if let Some(config) = OVERRIDE.with(|o| o.borrow().clone()) {
            return Some(config);
        }
        CONFIG
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    // ACE: ConfigManager.Initialize(MasterConfiguration)
    /// Initialises from a preloaded configuration.
    pub fn initialize(configuration: MasterConfiguration) {
        *CONFIG
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(configuration));
    }

    /// Not ACE (test isolation): makes [`config`](Self::config) answer `configuration` on the
    /// calling thread until the returned guard is dropped; other threads keep reading the
    /// process-wide configuration. Overrides nest.
    pub fn override_for_thread(configuration: MasterConfiguration) -> ConfigOverride {
        let previous = OVERRIDE.with(|o| o.borrow_mut().replace(Arc::new(configuration)));
        ConfigOverride { previous }
    }

    // ACE: ConfigManager.Initialize(string)
    /// Initialises from a `Config.js` file (ACE's default argument is `"Config.js"`). A bare file
    /// name is looked for in the current directory, then beside the executable.
    ///
    /// # Errors
    /// When the file is missing, unreadable or not a valid configuration.
    pub fn initialize_from_path(path: &str) -> Result<(), ConfigError> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let path_to_use = Self::resolve_path(path, &cwd, exe_dir.as_deref(), &|p| p.exists());

        let result = (|| {
            if !path_to_use.exists() {
                // DIVERGE: names Empyrean where ACE's names ACE (brand).
                log::error!(
                    "Configuration file is missing.  Please copy the file Config.js.example to Config.js and edit it to match your needs before running Empyrean."
                );
                return Err(ConfigError::Missing(path_to_use.clone()));
            }

            let file_text = std::fs::read_to_string(&path_to_use).map_err(ConfigError::Io)?;

            Self::deserialize(&file_text)
        })();

        match result {
            Ok(config) => {
                Self::initialize(config);
                Ok(())
            }
            Err(exception) => {
                log::error!("An exception occured while loading the configuration file!");
                log::error!("Exception: {exception}");
                Err(exception)
            }
        }
    }

    /// The path-resolution half of `Initialize(string)`, with the file system passed in: a path
    /// with a directory is used as is; a bare name is tried in `current_dir`, then in `exe_dir`.
    #[must_use]
    pub fn resolve_path(
        path: &str,
        current_dir: &Path,
        exe_dir: Option<&Path>,
        exists: &dyn Fn(&Path) -> bool,
    ) -> PathBuf {
        let p = Path::new(path);
        let directory_name = p.parent().filter(|d| !d.as_os_str().is_empty());
        let file_name = p
            .file_name()
            .map_or_else(|| "Config.js".into(), std::ffi::OsStr::to_os_string);

        match directory_name {
            // If no directory was specified, try both the current directory and the startup directory
            None => {
                let mut path_to_use = current_dir.join(&file_name);
                if !exists(&path_to_use) {
                    // File not found in Environment.CurrentDirectory
                    // Lets try the ExecutingAssembly Location
                    if let Some(dir) = exe_dir {
                        path_to_use = dir.join(&file_name);
                    }
                }
                path_to_use
            }
            Some(_) => p.to_path_buf(),
        }
    }

    /// `JsonSerializer.Deserialize<MasterConfiguration>(text, SerializerOptions)`: a `Config.js`
    /// text. Keys the configuration does not have (ACE's removed settings among them) are ignored.
    ///
    /// # Errors
    /// When the text is not a valid configuration.
    pub fn deserialize(text: &str) -> Result<MasterConfiguration, ConfigError> {
        serde_json::from_value(Self::parse_json(text)?).map_err(ConfigError::Json)
    }

    /// Not ACE: a `Config.js` text as a JSON value (a UTF-8 byte order mark, comments and trailing
    /// commas dropped), for the converter to walk before it deserialises.
    ///
    /// # Errors
    /// When the text is not JSON.
    pub fn parse_json(text: &str) -> Result<serde_json::Value, ConfigError> {
        // File.ReadAllText drops a UTF-8 byte order mark.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let clean = json::strip_comments_and_trailing_commas(text);
        serde_json::from_str(&clean).map_err(ConfigError::Json)
    }

    // ACE: ConfigManager.SerializerOptions
    /// `JsonSerializer.Serialize(config, SerializerOptions)`: indented JSON with ACE's key names
    /// in declaration order.
    ///
    /// # Panics
    /// Never in practice: the configuration types always serialise.
    #[must_use]
    pub fn serialize(config: &MasterConfiguration) -> String {
        serde_json::to_string_pretty(config).expect("MasterConfiguration serialises")
    }
}
