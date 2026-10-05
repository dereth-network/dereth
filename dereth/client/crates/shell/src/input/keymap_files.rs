//! Keymap files, named schemes and the separately stored interface maps.

use super::InputShell;
use dereth_input::{ActionId, InputMapId};
use std::path::PathBuf;

/// The keyboard UI's three non-error keymap-save outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKeymapAs {
    Saved,
    NeedsOverwrite,
    ReadOnly,
}

/// Keymap initialization step 2: resolve where the `.keymap` file lives.
///
/// ```text
/// dir  = the user-preferences settings directory
/// name = Input.KeymapFile, or the running executable's file name with the extension -> "keymap"
/// path = (dir joined with name, true)
/// ```
///
/// `keymap_file` is the `Input.KeymapFile` preference, whose
/// help text is *"The filename of the keymap file to use"*. `None` or empty takes the default,
/// which is [`DEFAULT_KEYMAP_FILE`].
///
/// **The default is a constant, and retail's rule was not.** Retail's keymap initialization takes
/// the *running executable's* file name and swaps the extension for `keymap`, which is the only
/// reason retail's file is called `acclient.keymap`. A rule that reads the binary's name means renaming or copying the binary silently orphans the
/// player's bindings -- a `dereth-client-debug.exe` or a second copy under any other name comes up
/// with no keymap and writes a new one beside the old. The file is the player's, not the
/// executable's, so it gets a fixed name.
///
/// Returns `None` when `preferences_file` is empty, which is an empty keymap-file path and the one
/// state in which keymap cleanup writes nothing.
#[must_use]
pub fn keymap_path_for(
    preferences_file: &std::path::Path,
    keymap_file: Option<&str>,
) -> Option<PathBuf> {
    if preferences_file.as_os_str().is_empty() {
        return None;
    }
    let dir = preferences_file
        .parent()
        .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
    let name = match keymap_file.filter(|s| !s.is_empty()) {
        Some(n) => PathBuf::from(n),
        None => PathBuf::from(DEFAULT_KEYMAP_FILE),
    };
    Some(dir.join(name))
}

/// The `.keymap` file a run with no `Input.KeymapFile` preference reads and writes, in the
/// settings directory. Retail's equivalent is `acclient.keymap`; see [`keymap_path_for`] for why
/// this is a constant and that one is not.
pub const DEFAULT_KEYMAP_FILE: &str = "dereth-modern.keymap";

/// The modern (retail) interface's slug: its key maps are `<name>-modern.keymap`.
pub const MODERN_SLUG: &str = "modern";

/// The classic interface's slug: its key maps are `<name>-classic.keymap`.
pub const CLASSIC_SLUG: &str = "classic";

/// The file of the key map `name` of the interface whose slug is `slug`: `<name>-<slug>.keymap`.
/// A name typed with the slug or the extension already on is taken without them.
#[must_use]
pub fn scheme_file(name: &str, slug: &str) -> String {
    let name = name.trim();
    let name = strip_suffix_ignore_case(name, ".keymap").unwrap_or(name);
    let tail = format!("-{slug}");
    let name = strip_suffix_ignore_case(name, &tail).unwrap_or(name);
    format!("{name}-{slug}.keymap")
}

/// The name of the key map in `file` when it is one of the interface whose slug is `slug`.
#[must_use]
pub fn scheme_name(file: &str, slug: &str) -> Option<String> {
    let tail = format!("-{slug}.keymap");
    strip_suffix_ignore_case(file, &tail)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

fn strip_suffix_ignore_case<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let at = s.len().checked_sub(suffix.len())?;
    (s.is_char_boundary(at) && s[at..].eq_ignore_ascii_case(suffix)).then(|| &s[..at])
}

/// The classic interface's key map file, beside the retail one.
pub const CLASSIC_KEYMAP_FILE: &str = "dereth-classic.keymap";

/// The classic interface's key map: its file read over its default scheme, defaults included,
/// as the retail key map is kept.
#[derive(Debug, Clone)]
pub struct ClassicKeymap {
    /// The map as it is now.
    pub map: dereth_input::MasterInputMap,
    /// The classic default scheme it is read over.
    pub defaults: dereth_input::MasterInputMap,
    /// Its file, `None` when the client keeps no files.
    pub path: Option<PathBuf>,
}

impl ClassicKeymap {
    /// Write the map to its file.
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save(&self) -> Result<(), dereth_input::InputError> {
        if let Some(path) = &self.path {
            dereth_client_runtime::platform::files::write(path, self.map.to_keymap_text())?;
        }
        Ok(())
    }
}

/// The input manager's full merged keymap, written where the host keeps the client's files.
///
/// # Errors
/// [`dereth_input::InputError::Io`] if the file cannot be written.
fn write_keymap(
    map: &dereth_input::MasterInputMap,
    path: &std::path::Path,
) -> Result<(), dereth_input::InputError> {
    dereth_client_runtime::platform::files::write(path, map.to_keymap_text())?;
    Ok(())
}

/// Persist `Input.KeymapFile` with `WritePrivateProfileString`-style merge semantics.
pub fn save_keymap_preference(
    preferences_file: &std::path::Path,
    keymap_file: &str,
) -> Result<(), std::io::Error> {
    if preferences_file.as_os_str().is_empty() {
        return Ok(());
    }
    let mut ini = match dereth_client_runtime::platform::files::read_to_string(preferences_file) {
        Ok(text) => dereth_client_contract::persist::preferences::UserPreferences::parse(&text)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            dereth_client_contract::persist::preferences::UserPreferences {
                crlf: true,
                ..Default::default()
            }
        }
        Err(error) => return Err(error),
    };
    ini.write_profile_string("Input", "KeymapFile", keymap_file);
    dereth_client_runtime::platform::files::write(preferences_file, ini.to_text())
}

impl InputShell {
    /// The configured keymap filename resolved to a path, or `None` when the client would not save.
    #[must_use]
    pub fn keymap_path(&self) -> Option<&std::path::Path> {
        self.keymap_path.as_deref()
    }

    /// The keymap basename shown by the keyboard options panel.
    #[must_use]
    pub fn keymap_file_name(&self) -> Option<String> {
        self.keymap_path
            .as_deref()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
    }

    /// The keyboard page's label, without changing the file used for persistence.
    #[must_use]
    pub fn keymap_display_name(&self) -> Option<String> {
        let file = self
            .keymap_file_name()
            .unwrap_or_else(|| DEFAULT_KEYMAP_FILE.into());
        if file.eq_ignore_ascii_case(DEFAULT_KEYMAP_FILE)
            || file.eq_ignore_ascii_case("acclient.keymap")
        {
            Some("Default".into())
        } else {
            scheme_name(&file, MODERN_SLUG).or(Some(file))
        }
    }

    /// The names of one interface's saved key maps (`<name>-<slug>.keymap`) in the folder,
    /// sorted.
    pub fn scheme_names(&self, slug: &str) -> Result<Vec<String>, dereth_input::InputError> {
        Ok(self
            .keymap_files()?
            .iter()
            .filter_map(|f| scheme_name(f, slug))
            .collect())
    }

    /// The name of the modern interface's key map in use, when its file is one of the scheme's.
    #[must_use]
    pub fn modern_scheme_in_use(&self) -> Option<String> {
        self.keymap_file_name()
            .and_then(|f| scheme_name(&f, MODERN_SLUG))
    }

    /// Every `*.keymap` in the folder, by basename.
    pub fn keymap_files(&self) -> Result<Vec<String>, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(Vec::new());
        };
        let mut names = Vec::new();
        for path in dereth_client_runtime::platform::files::list(dir)? {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("keymap"))
            {
                if let Some(name) = path.file_name() {
                    names.push(name.to_string_lossy().into_owned());
                }
            }
        }
        names.sort_by_key(|name| name.to_ascii_lowercase());
        Ok(names)
    }

    /// Initialize the keymap from the selected file when the load dialog closes.
    /// A malformed user file deliberately becomes the two shipped defaults: keymap
    /// initialization clears the partial user map after adding it fails and then adds
    /// the game keymap and the base keymap, in that order.
    pub fn load_keymap_file(&mut self, name: &str) -> Result<bool, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let Some(file_name) = std::path::Path::new(name).file_name() else {
            return Ok(false);
        };
        if file_name != std::path::Path::new(name) {
            return Ok(false);
        }
        let path = dir.join(file_name);
        let text = dereth_client_runtime::platform::files::read_to_string(&path)?;
        let Some(maps) = self.manager.shipped_maps.take() else {
            return Err(dereth_input::InputError::KeymapFile(
                "the shipped keymaps are unavailable".to_owned(),
            ));
        };
        let merged = dereth_input::InputManager::load_over_defaults(
            Some(&text),
            &[&maps.0, &maps.1],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = merged;
        self.manager.shipped_maps = Some(maps);
        self.keymap_path = Some(path);
        Ok(true)
    }

    /// The two shipped key maps, the game's and the engine's, in the order they are read.
    fn shipped_maps(&self) -> Vec<&dereth_input::MasterInputMap> {
        self.manager
            .shipped_maps
            .as_ref()
            .map(|m| vec![&m.0, &m.1])
            .unwrap_or_default()
    }

    /// The classic interface's default scheme, as a key map over the same keyboard as the retail
    /// one.
    #[must_use]
    pub fn classic_defaults(&self) -> dereth_input::MasterInputMap {
        dereth_classic_ui::default_keys::default_map(
            self.modern_map(),
            &self.manager.action_map,
            &self.shipped_maps(),
        )
    }

    /// The classic interface's key map, read the first time it is wanted: its file over its
    /// default scheme, the default scheme alone when there is no file yet.
    pub fn classic_keymap(&mut self) -> &mut ClassicKeymap {
        if self.classic.is_none() {
            let defaults = self.classic_defaults();
            let path = self
                .keymap_path
                .as_deref()
                .and_then(std::path::Path::parent)
                .map(|dir| dir.join(CLASSIC_KEYMAP_FILE));
            let file = path
                .as_ref()
                .and_then(|p| dereth_client_runtime::platform::files::read_to_string(p).ok());
            let map = dereth_input::InputManager::load_over_defaults(
                file.as_deref(),
                &[&defaults],
                Some(&self.manager.action_map),
            );
            self.classic = Some(ClassicKeymap {
                map,
                defaults,
                path,
            });
        }
        if self.classic_active {
            self.classic.as_mut().expect("active map").map = self.manager.keymap.clone();
        }
        self.classic.as_mut().expect("just made")
    }

    /// The classic key map as the classic interface reads it.
    pub fn classic_keys(&mut self) -> dereth_classic_ui::keystore::ClassicKeys {
        use dereth_classic_ui::keystore::{ClassicBinding, ClassicKeys};
        let active = scheme_name(CLASSIC_KEYMAP_FILE, CLASSIC_SLUG).unwrap_or_default();
        let files = self
            .scheme_names(CLASSIC_SLUG)
            .unwrap_or_default()
            .into_iter()
            .filter(|n| !n.eq_ignore_ascii_case(&active))
            .collect();
        let mut maps: Vec<u32> = dereth_input::presentation::ROWS
            .iter()
            .map(|r| r.map.0)
            .collect();
        maps.sort_unstable();
        maps.dedup();
        let conflicts = maps
            .iter()
            .map(|m| {
                (
                    *m,
                    self.manager
                        .action_map
                        .conflicting_input_maps(InputMapId(*m))
                        .iter()
                        .map(|x| x.0)
                        .collect(),
                )
            })
            .collect();
        let holds = dereth_input::presentation::ROWS
            .iter()
            .filter(|r| {
                self.manager
                    .action_map
                    .toggle_type(r.input_map(), r.action())
                    .is_hold()
            })
            .map(|r| (r.map.0, r.action().0))
            .collect();
        let classic = self.classic_keymap();
        // The keys of the rows the key pages list.
        let bindings = dereth_input::scheme::keyboard_bindings(&classic.map)
            .filter(|(map, _, action)| dereth_input::presentation::find(*map, *action).is_some())
            .filter_map(|(map, qc, action)| {
                Some(ClassicBinding {
                    scan: qc.control.offset(),
                    modifiers: dereth_classic_ui::keystore::modifiers_of_meta(qc.meta_mode)?,
                    action: action.0,
                    map: map.0,
                })
            })
            .collect();
        ClassicKeys {
            bindings,
            files,
            active,
            conflicts,
            holds,
        }
    }

    /// The keyboard key `scan` held with `modifiers`, as the shipped maps bind keyboard keys.
    fn classic_key(&self, scan: u16, modifiers: u8) -> Option<dereth_input::ControlChord> {
        dereth_input::scheme::keyboard_key(
            &self.manager.keymap,
            scan,
            dereth_classic_ui::keystore::meta_of_modifiers(modifiers),
        )
    }

    /// The classic scheme `scheme` as a whole key map: the default scheme, or one of this
    /// interface's saved key maps.
    fn scheme(
        &mut self,
        scheme: &dereth_classic_ui::keystore::Scheme,
    ) -> Option<dereth_input::MasterInputMap> {
        use dereth_classic_ui::keystore::Scheme;
        match scheme {
            Scheme::Default => Some(self.classic_defaults()),
            Scheme::File(name) => {
                let dir = self.keymap_path.as_deref()?.parent()?;
                let file = dir.join(scheme_file(name, CLASSIC_SLUG));
                let text = dereth_client_runtime::platform::files::read_to_string(&file).ok()?;
                dereth_input::MasterInputMap::from_keymap_text(&text).ok()
            }
        }
    }

    /// Carry out one of the classic key page's requests on the classic key map, and write it.
    pub fn classic_request(&mut self, request: dereth_classic_ui::keystore::KeyStoreRequest) {
        use dereth_classic_ui::keystore::KeyStoreRequest as R;
        let result = match request {
            R::Bind {
                scan,
                modifiers,
                action,
                map,
                replaced,
            } => {
                let key = self.classic_key(scan, modifiers);
                let old = replaced.and_then(|(s, m)| self.classic_key(s, m));
                let action_map = self.manager.action_map.clone();
                let classic = self.classic_keymap();
                if let Some(key) = key {
                    dereth_input::scheme::bind(
                        &mut classic.map,
                        &action_map,
                        InputMapId(map),
                        key,
                        ActionId(action),
                        old,
                    );
                }
                classic.save()
            }
            R::Clear {
                scan,
                modifiers,
                action,
                map,
            } => {
                let key = self.classic_key(scan, modifiers);
                let classic = self.classic_keymap();
                if let Some(key) = key {
                    dereth_input::scheme::clear(
                        &mut classic.map,
                        InputMapId(map),
                        key,
                        ActionId(action),
                    );
                }
                classic.save()
            }
            R::SaveAs { name, overwrite } => self.save_classic_as(&name, overwrite),
            R::Load(scheme) => match self.scheme(&scheme) {
                Some(scheme) => {
                    let action_map = self.manager.action_map.clone();
                    let classic = self.classic_keymap();
                    let file = dereth_input::scheme::exactly(&scheme, &[&classic.defaults]);
                    classic.map = dereth_input::scheme::over_defaults(
                        Some(&file),
                        &[&classic.defaults],
                        Some(&action_map),
                    );
                    classic.save()
                }
                None => Err(dereth_input::InputError::KeymapFile(format!(
                    "the key scheme {scheme:?} could not be read"
                ))),
            },
            R::Delete(name) => {
                let file = scheme_file(&name, CLASSIC_SLUG);
                if file.eq_ignore_ascii_case(CLASSIC_KEYMAP_FILE) {
                    Ok(())
                } else {
                    self.delete_keymap_file(&file).map(|_| ())
                }
            }
        };
        if self.classic_active {
            if let Some(classic) = &self.classic {
                self.manager.keymap = classic.map.clone();
            }
        }
        if let Err(e) = result {
            tracing::warn!(target: "dereth_client_shell::input", "the classic key map: {e}");
        }
    }

    /// Write the classic key map as `name`, the file `<name>-classic.keymap` beside the others,
    /// replacing one of that name only when `overwrite`. The classic key map in use is not
    /// written this way.
    fn save_classic_as(
        &mut self,
        name: &str,
        overwrite: bool,
    ) -> Result<(), dereth_input::InputError> {
        let file = scheme_file(name, CLASSIC_SLUG);
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
        else {
            return Ok(());
        };
        if file.eq_ignore_ascii_case(CLASSIC_KEYMAP_FILE)
            || name.trim().is_empty()
            || std::path::Path::new(&file).file_name() != Some(file.as_ref())
        {
            return Ok(());
        }
        let path = dir.join(file);
        if dereth_client_runtime::platform::files::exists(&path) && !overwrite {
            return Ok(());
        }
        let text = self.classic_keymap().map.to_keymap_text();
        dereth_client_runtime::platform::files::write(&path, text)?;
        Ok(())
    }

    /// Back to the shipped defaults: the player's own keys are dropped.
    pub fn restore_shipped_keys(&mut self) -> bool {
        let Some((game, base)) = self.manager.shipped_maps.as_deref() else {
            return false;
        };
        let restored = dereth_input::InputManager::load_over_defaults(
            None,
            &[game, base],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = restored;
        true
    }

    /// Delete the key map file `name` (a basename, `.keymap` added when absent) beside the one in
    /// use. The one in use is not deleted.
    pub fn delete_keymap_file(&mut self, name: &str) -> Result<bool, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let mut file = std::path::PathBuf::from(name.trim());
        if file.file_name() != Some(file.as_os_str()) {
            return Ok(false);
        }
        if file
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("keymap"))
        {
            file = std::path::PathBuf::from(format!("{}.keymap", name.trim()));
        }
        let path = dir.join(file);
        if self.keymap_path.as_deref() == Some(path.as_path()) {
            return Ok(false);
        }
        dereth_client_runtime::platform::files::remove_file(&path)?;
        Ok(true)
    }

    /// `SaveKeymap` for a new name from the Save Keymap dialog: the modern interface's key map
    /// `name`, the file `<name>-modern.keymap`.
    pub fn save_keymap_as(
        &mut self,
        name: &str,
        overwrite: bool,
    ) -> Result<Option<SaveKeymapAs>, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(None);
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let file_name = std::path::PathBuf::from(scheme_file(name, MODERN_SLUG));
        if file_name.file_name().is_none() || file_name.file_name() != Some(file_name.as_os_str()) {
            return Ok(None);
        }
        let path = dir.join(file_name);
        if dereth_client_runtime::platform::files::exists(&path) {
            if dereth_client_runtime::platform::files::read_only(&path)? {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            if !overwrite {
                return Ok(Some(SaveKeymapAs::NeedsOverwrite));
            }
        }
        if let Err(error) = write_keymap(self.modern_map(), &path) {
            if matches!(&error, dereth_input::InputError::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied)
            {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            return Err(error);
        }
        self.keymap_path = Some(path);
        Ok(Some(SaveKeymapAs::Saved))
    }

    /// Save the keymap during cleanup when its filename is nonempty and it was loaded.
    /// The destination joins the settings directory and that filename.
    ///
    /// Saving does not require the preference `Input.KeymapFile` to be set. Initialization assigns
    /// the keymap-file path either from the preference or, when the preference is empty, from
    /// the running executable's file name with the extension changed to `keymap` — so
    /// the path is **never** empty after successful keymap initialization, and a retail client
    /// therefore writes `<prefs dir>\acclient.keymap` on every clean exit. That is why no shipped
    /// install carries a `.keymap` and every played one does.
    ///
    /// What is written is the **full merged map**, defaults included, not just the overrides
    /// — [`dereth_input::InputManager::save_keymap`] is the writer and this is its caller.
    ///
    /// Returns `Ok(false)` when there is no path, which is the client's own skip.
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save_keymap(&self) -> Result<bool, dereth_input::InputError> {
        let Some(path) = self.keymap_path.as_ref() else {
            return Ok(false);
        };
        write_keymap(self.modern_map(), path)?;
        Ok(true)
    }
}
