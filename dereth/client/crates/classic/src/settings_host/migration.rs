//! Migration of the classic interface's earlier files into the shared preference store.
#[cfg(test)]
use super::digit;
use super::{quantize, resolution_value, shared_values, store, ClassicSettings, PrefValue};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct Stored {
    pub(super) version: u32,
    pub(super) sound: u32,
    pub(super) graphics: u32,
    pub(super) textures: u32,
    pub(super) detail: [bool; 2],
    pub(super) resolution: [u32; 2],
}
impl Stored {
    #[cfg(test)]
    pub(super) fn from_settings(s: &ClassicSettings) -> Self {
        let (w, h) = s
            .resolutions
            .get(s.resolution)
            .copied()
            .unwrap_or((800, 600));
        Self {
            version: 1,
            sound: u32::from(s.effects)
                | (u32::from(s.ambient) << 1)
                | (u32::from(s.interface) << 2)
                | (u32::from(!s.auto_degrade) << 3)
                | (u32::from(!s.stereo) << 4)
                | (digit(s.effects_volume, 10.0) << 8)
                | (digit(s.ambient_volume, 10.0) << 12),
            graphics: digit(s.brightness, 100.0)
                | (digit(s.performance, 10.0) << 8)
                | (digit(s.camera_stiffness, 10.0) << 12),
            textures: s
                .texture_levels
                .iter()
                .enumerate()
                .fold(0, |v, (i, n)| v | (u32::from((*n).min(3)) << (4 * i))),
            detail: [s.landscape_detail, s.environment_detail],
            resolution: [w, h],
        }
    }
    pub(super) fn decode(&self, capabilities: &ClassicSettings) -> Result<ClassicSettings, String> {
        if self.version != 1
            || self.sound & !0xffff != 0
            || self.graphics & !0xffff != 0
            || self.graphics & 255 > 100
            || (self.graphics >> 8) & 15 > 10
            || (self.graphics >> 12) & 15 > 10
            || (self.sound >> 8) & 15 > 10
            || (self.sound >> 12) & 15 > 10
            || self.textures & !0xffff != 0
            || (0..4).any(|i| (self.textures >> (4 * i)) & 15 > 3)
        {
            return Err("Invalid classic settings state".into());
        }
        let mut s = capabilities.clone();
        s.stereo = self.sound & 0xf0 != 0x10;
        s.effects = self.sound & 1 != 0;
        s.ambient = self.sound & 2 != 0;
        s.interface = self.sound & 4 != 0;
        s.auto_degrade = self.sound & 8 == 0;
        s.effects_volume = ((self.sound >> 8) & 15) as f32 / 10.0;
        s.ambient_volume = ((self.sound >> 12) & 15) as f32 / 10.0;
        s.brightness = (self.graphics & 255) as f32 / 100.0;
        s.performance = ((self.graphics >> 8) & 15) as f32 / 10.0;
        s.camera_stiffness = ((self.graphics >> 12) & 15) as f32 / 10.0;
        s.texture_levels = std::array::from_fn(|i| ((self.textures >> (4 * i)) & 15) as u8);
        s.landscape_detail = self.detail[0];
        s.environment_detail = self.detail[1];
        if let Some(i) = s
            .resolutions
            .iter()
            .position(|r| *r == (self.resolution[0], self.resolution[1]))
        {
            s.resolution = i;
        }
        Ok(s)
    }
}
/// The texture sizes a `settings.json` kept as the page's old steps (0 full size .. 3 the
/// smallest), as the shared preferences they stand for: landscape and every other image, stored
/// value step + 1 (High .. Very Low).
fn texture_values(s: &ClassicSettings) -> Vec<(&'static str, PrefValue)> {
    use dereth_client_runtime::render_prefs as names;
    let value = |step: u8| PrefValue::Int(i32::from(step.min(3)) + 1);
    vec![
        (names::LANDSCAPE_TEXTURE_DETAIL, value(s.texture_levels[0])),
        (
            names::ENVIRONMENT_TEXTURE_DETAIL,
            value(s.texture_levels[2]),
        ),
    ]
}

/// Carry a `settings.json` the classic interface kept in its own folder into the shared store,
/// once: each setting the player had moved from the page's own first values (`defaults`) is
/// written to its shared preference, and the file is removed. A setting left at the page's first
/// value is not written, so it does not override what the shared store holds. Returns how many
/// settings were carried, or why the file could not be read (it is then left in place).
///
/// # Errors
/// The file is there and cannot be read or is not the page's.
pub fn migrate_settings_file(path: &Path, defaults: &ClassicSettings) -> Result<usize, String> {
    if !dereth_client_runtime::platform::files::is_file(path) {
        return Ok(0);
    }
    let bytes = dereth_client_runtime::platform::files::read(path).map_err(|e| e.to_string())?;
    let stored = serde_json::from_slice::<Stored>(&bytes).map_err(|e| e.to_string())?;
    let mut file = stored.decode(defaults)?;
    // The file named a size, not a place in this machine's list of sizes.
    file.full_screen = defaults.full_screen;
    let mut carried = 0;
    // The page's first values as the file would have held them (tenths, hundredths).
    let first = quantize(defaults, defaults);
    let mut before = shared_values(&first);
    before.extend(texture_values(&first));
    let mut after = shared_values(&file);
    after.extend(texture_values(&file));
    if !defaults
        .resolutions
        .contains(&(stored.resolution[0], stored.resolution[1]))
    {
        after.retain(|(n, _)| *n != dereth_client_contract::options::names::DISPLAY_RESOLUTION);
        after.push((
            dereth_client_contract::options::names::DISPLAY_RESOLUTION,
            resolution_value((stored.resolution[0], stored.resolution[1])),
        ));
    }
    for (name, v) in after {
        if before.iter().any(|(n, d)| *n == name && *d == v) {
            continue;
        }
        if store::set_value(name, v) {
            carried += 1;
        }
    }
    dereth_client_runtime::platform::files::remove_file(path).map_err(|e| e.to_string())?;
    Ok(carried)
}

/// Startup has two phases: option bits before binding, then JSON after capabilities/defaults.
#[derive(Debug)]
pub(crate) struct Migration<'a> {
    folder: &'a Path,
}
impl<'a> Migration<'a> {
    pub(crate) fn new(folder: &'a Path) -> Self {
        Self { folder }
    }
    pub(crate) fn before_defaults(&self) {
        if let Some(bits) = dereth_client_runtime::platform::files::read_to_string(
            &self.folder.join("classic-options"),
        )
        .ok()
        .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
        {
            crate::keyboard_runtime::set_classic_bits(bits);
            let _ = dereth_client_runtime::platform::files::remove_file(
                &self.folder.join("classic-options"),
            );
            tracing::info!(target: "dereth_classic_ui::runtime", "the classic interface's own options moved into the profile");
        }
    }
    pub(crate) fn after_defaults(&self, defaults: &ClassicSettings) {
        match migrate_settings_file(&self.folder.join("settings.json"), defaults) {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!(target: "dereth_classic_ui::runtime", "{n} classic sound and graphics setting(s) moved into the profile")
            }
            Err(e) => {
                tracing::warn!(target: "dereth_classic_ui::runtime", "the classic interface's old settings file: {e}")
            }
        }
        retire_folder(self.folder);
    }
}

/// The classic interface's old settings folder, once everything in it has moved into the shared
/// store and key map: removed when nothing is left in it, and left with what is otherwise.
fn retire_folder(folder: &std::path::Path) {
    match dereth_client_runtime::platform::files::remove_empty_dir(folder) {
        Ok(true) => {
            tracing::info!(target: "dereth_classic_ui::runtime", "the classic interface's old settings folder is retired")
        }
        Ok(false) => tracing::info!(target: "dereth_classic_ui::runtime",
            "the classic interface's old settings folder {} still holds files this client no longer reads",
            folder.display()
        ),
        Err(_) => {}
    }
}
