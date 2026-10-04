//! The classic settings pages carried onto the host. UI values are normalized.
//!
//! **One store.** Every row of the Client page is a shared preference, the same one the
//! retail interface's Client Options page edits: the page opens on the store's values and Apply
//! writes them back, so the profile (`UserPreferences.ini`) holds both interfaces' settings and
//! either page shows what the other set. The page keeps its own steps (tenths for the volumes,
//! hundredths for the brightness) by packing a value into the classic interface's words before
//! it is written, as the early client saved it.
//!
//! The classic interface once kept these in a file of its own, `classic/settings.json`;
//! [`migrate_settings_file`] carries a file left from then into the store, once.
use crate::{panels::ClassicSettings, runtime::Cx};
use dereth_client_contract::options::store;
use dereth_client_contract::{PrefValue, UiRequest};
use dereth_client_runtime::render_prefs::RenderPreferences;
use dereth_client_runtime::{present::Presentation, shell::Shell};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Stored {
    version: u32,
    sound: u32,
    graphics: u32,
    textures: u32,
    detail: [bool; 2],
    resolution: [u32; 2],
}
fn normalized(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn digit(v: f32, scale: f32) -> u32 {
    u32::try_from(dereth_primitives::num::to_i32(
        (normalized(v) * scale).trunc(),
    ))
    .unwrap_or(0)
}
impl Stored {
    fn from_settings(s: &ClassicSettings) -> Self {
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
    fn decode(&self, capabilities: &ClassicSettings) -> Result<ClassicSettings, String> {
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
/// The classic Client page's hosted settings, carried onto the shared scene's render preferences.
///
/// The Environment Detail Textures box sets the shared preference as it is. Brightness sets the
/// screen's gamma (the classic page raised the ambient light and the viewer's own light instead)
/// on the other interface's scale, [`brightness_of_slider`], so the slider's middle leaves the
/// picture as it is; and the Adaptive Degrade Bias slider sets the degrade bias when automatic
/// degrading is off, speed at its left and detail at its right, the way the other interface's
/// slider runs. The texture sizes, the landscape detail textures and the degrade distance are not
/// hosted: the page edits them as the preferences they are.
pub fn render_preferences(s: &ClassicSettings, prefs: &mut RenderPreferences) {
    prefs.environment_detail_textures = s.environment_detail;
    prefs.screen_brightness = brightness_of_slider(s.brightness);
    prefs.automatic_degrades = s.auto_degrade;
    prefs.graphics_performance = 2.0 * normalized(s.performance) - 1.0;
}
/// The screen brightness for the classic Brightness slider's position (0 to 1): the other
/// interface's brightness slider runs from -1 to 1 with the unchanged picture at 0, and the classic
/// slider covers the same range, its middle at 0.
#[must_use]
pub fn brightness_of_slider(position: f32) -> f32 {
    2.0 * normalized(position) - 1.0
}
/// The classic Brightness slider's position for a screen brightness, the inverse of
/// [`brightness_of_slider`]: where the slider starts when no classic settings are saved.
#[must_use]
pub fn slider_of_brightness(brightness: f32) -> f32 {
    if brightness.is_finite() {
        ((brightness + 1.0) / 2.0).clamp(0.0, 1.0)
    } else {
        0.5
    }
}
fn preference(name: &'static str, value: PrefValue) -> UiRequest {
    UiRequest::SetPreference(name, value)
}
/// The graphics page's values as the shared render preferences, the way [`render_preferences`]
/// carries them, and the camera's stiffness when one is set.
fn render_requests(s: &ClassicSettings, camera: Option<f32>) -> Vec<UiRequest> {
    use dereth_client_runtime::render_prefs as names;
    let mut prefs = RenderPreferences::default();
    render_preferences(s, &mut prefs);
    let mut out = vec![
        preference(
            names::BUILDING_DETAIL_TEXTURES,
            PrefValue::Bool(prefs.environment_detail_textures),
        ),
        preference(
            names::SCREEN_BRIGHTNESS,
            PrefValue::Float(prefs.screen_brightness),
        ),
        preference(
            names::AUTOMATIC_DEGRADES,
            PrefValue::Bool(prefs.automatic_degrades),
        ),
        preference(
            names::GRAPHICS_PERFORMANCE,
            PrefValue::Float(prefs.graphics_performance),
        ),
    ];
    if let Some(value) = camera {
        out.push(preference("Camera.Stiffness", PrefValue::Float(value)));
    }
    out
}
/// The sound page's values as the game's sound preferences. The three "Disabled" preferences
/// are enables despite their names: true means the sound plays.
fn sound_requests(s: &ClassicSettings) -> Vec<UiRequest> {
    vec![
        preference("Sound.SoundDisabled", PrefValue::Bool(s.effects)),
        preference("Sound.AmbientSoundDisabled", PrefValue::Bool(s.ambient)),
        preference("Sound.InterfaceSoundDisabled", PrefValue::Bool(s.interface)),
        preference("Sound.SoundFeatures", PrefValue::Int(i32::from(!s.stereo))),
        preference(
            "Sound.SoundVolume",
            PrefValue::Float(normalized(s.effects_volume)),
        ),
        preference(
            "Sound.AmbientSoundVolume",
            PrefValue::Float(normalized(s.ambient_volume)),
        ),
    ]
}
/// The page's values as the shared preferences they are, the Display ones included. The camera's
/// stiffness is left out at zero, where the page leaves the live stiffness alone.
fn shared_values(s: &ClassicSettings) -> Vec<(&'static str, PrefValue)> {
    let mut out: Vec<(&'static str, PrefValue)> = sound_requests(s)
        .into_iter()
        .chain(render_requests(
            s,
            (s.camera_stiffness > 0.0).then(|| camera_stiffness(s.camera_stiffness)),
        ))
        .filter_map(|r| match r {
            UiRequest::SetPreference(name, v) => Some((name, v)),
            _ => None,
        })
        .collect();
    if let Some((w, h)) = s.resolutions.get(s.resolution) {
        out.push(("Display.Resolution", resolution_value((*w, *h))));
    }
    out.push(("Display.FullScreen", PrefValue::Bool(s.full_screen)));
    out
}

/// Write the page's values into the shared store.
fn write_shared(s: &ClassicSettings) {
    for (name, v) in shared_values(s) {
        let _ = store::set_value(name, v);
    }
}

/// The page's values read out of the shared store, over `capabilities` (what this machine can
/// do, and the window's size, which the window already took from the store when it opened).
/// A preference the store does not hold keeps the capability's value.
#[must_use]
pub fn from_shared(capabilities: &ClassicSettings) -> ClassicSettings {
    let mut s = capabilities.clone();
    let bool_of = |name: &str| match store::inq_value(name) {
        Some(PrefValue::Bool(b)) => Some(b),
        _ => None,
    };
    let float_of = |name: &str| match store::inq_value(name) {
        Some(PrefValue::Float(f)) => Some(f),
        _ => None,
    };
    let int_of = |name: &str| match store::inq_value(name) {
        Some(PrefValue::Int(i)) => Some(i),
        _ => None,
    };
    if let Some(v) = bool_of("Sound.SoundDisabled") {
        s.effects = v;
    }
    if let Some(v) = bool_of("Sound.AmbientSoundDisabled") {
        s.ambient = v;
    }
    if let Some(v) = bool_of("Sound.InterfaceSoundDisabled") {
        s.interface = v;
    }
    if let Some(v) = int_of("Sound.SoundFeatures") {
        s.stereo = v == 0;
    }
    if let Some(v) = float_of("Sound.SoundVolume") {
        s.effects_volume = normalized(v);
    }
    if let Some(v) = float_of("Sound.AmbientSoundVolume") {
        s.ambient_volume = normalized(v);
    }
    use dereth_client_runtime::render_prefs as names;
    if let Some(v) = bool_of(names::BUILDING_DETAIL_TEXTURES) {
        s.environment_detail = v;
    }
    if let Some(v) = float_of(names::SCREEN_BRIGHTNESS) {
        s.brightness = slider_of_brightness(v);
    }
    if let Some(v) = bool_of(names::AUTOMATIC_DEGRADES) {
        s.auto_degrade = v;
    }
    if let Some(v) = float_of(names::GRAPHICS_PERFORMANCE) {
        s.performance = normalized((v + 1.0) / 2.0);
    }
    if let Some(v) = float_of("Camera.Stiffness") {
        s.camera_stiffness = normalized(v / 0.714_285_73 - 0.4);
    }
    if let Some(v) = bool_of("Display.FullScreen") {
        s.full_screen = v;
    }
    s
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
    if !path.is_file() {
        return Ok(0);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let stored = serde_json::from_slice::<Stored>(&bytes).map_err(|e| e.to_string())?;
    let mut file = stored.decode(defaults)?;
    // The file named a size, not a place in this machine's list of sizes.
    file.full_screen = defaults.full_screen;
    let mut carried = 0;
    // The page's first values as the file would have held them (tenths, hundredths).
    let first = Stored::from_settings(defaults).decode(defaults)?;
    let mut before = shared_values(&first);
    before.extend(texture_values(&first));
    let mut after = shared_values(&file);
    after.extend(texture_values(&file));
    if !defaults
        .resolutions
        .contains(&(stored.resolution[0], stored.resolution[1]))
    {
        after.retain(|(n, _)| *n != "Display.Resolution");
        after.push((
            "Display.Resolution",
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
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(carried)
}

fn resolution_value(size: (u32, u32)) -> PrefValue {
    PrefValue::Int(((size.0 << 16) | size.1) as i32)
}

fn resolution_request(size: (u32, u32)) -> UiRequest {
    preference("Display.Resolution", resolution_value(size))
}
/// The capability fields are supplied by the actual endpoint/renderer/display, never by the file.
#[derive(Debug)]
pub struct SettingsHost {
    current: ClassicSettings,
    saved: ClassicSettings,
    camera_value: Option<f32>,
    /// The render and camera preferences last sent, so an unchanged frame sends none.
    sent: Vec<UiRequest>,
}
impl SettingsHost {
    /// The page over the shared store's values ([`from_shared`]).
    pub fn load(capabilities: ClassicSettings) -> Result<Self, String> {
        let current = from_shared(&capabilities);
        let camera_value =
            (current.camera_stiffness > 0.0).then(|| camera_stiffness(current.camera_stiffness));
        Ok(Self {
            saved: current.clone(),
            current,
            camera_value,
            sent: Vec::new(),
        })
    }
    pub fn snapshot(&self) -> ClassicSettings {
        self.current.clone()
    }
    /// Open the page again on the shared store's values: the other interface's page may have
    /// changed them while this one was put away. A size choice under way is left as it is.
    pub fn reload(&mut self) {
        let current = from_shared(&self.current);
        self.saved = current.clone();
        self.current = current;
        if self.current.camera_stiffness > 0.0 {
            self.camera_value = Some(camera_stiffness(self.current.camera_stiffness));
        }
    }
    /// Call once after start_shell; sound is then available and loaded preferences can take effect.
    pub fn initialize<S: Shell>(&mut self, cx: &mut Cx<'_, S>) -> Result<Vec<UiRequest>, String> {
        self.apply_values(cx, self.current.clone(), true)
    }
    /// Apply uses integer packing before decoding, reproducing the tenths/hundredths truncation.
    /// Any returned display request must enter the host's normal UiRequest dispatcher.
    pub fn apply<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        settings: ClassicSettings,
        save: bool,
    ) -> Result<Vec<UiRequest>, String> {
        let packed = Stored::from_settings(&settings);
        let effective = packed.decode(&self.current)?;
        // A rejected native resize can leave the requested configuration ahead of
        // the live surface; rollback and retry must start from what is displayed.
        let previous = cx.present().size();
        let target = effective
            .resolutions
            .get(effective.resolution)
            .copied()
            .unwrap_or(previous);
        let full_screen = settings.full_screen;
        let mut left = self.apply_values(cx, effective, false)?;
        self.current.full_screen = full_screen;
        if full_screen_changed(
            full_screen,
            cx.config().display.full_screen,
            cx.full_screen(),
        ) {
            left.push(preference(
                "Display.FullScreen",
                PrefValue::Bool(full_screen),
            ));
        }
        if save {
            self.persist_resolution(cx.resolution_previous().unwrap_or(previous))?;
        }
        if full_screen {
            // Full screen covers the monitor whatever size is chosen: the size is kept for the
            // window, to take effect when the game next runs in one.
            if target != previous {
                left.push(resolution_request(target));
                self.select_resolution(target);
                if save {
                    self.persist_resolution(target)?;
                }
            }
        } else if target != previous {
            left.push(UiRequest::Resolution(
                dereth_client_contract::resolution::ResolutionAction::Begin {
                    size: target,
                    policy: dereth_client_contract::resolution::ResolutionPolicy::Classic,
                    persist: save,
                },
            ));
        }
        Ok(left)
    }
    fn select_resolution(&mut self, size: (u32, u32)) {
        if let Some(i) = self.current.resolutions.iter().position(|r| *r == size) {
            self.current.resolution = i;
        }
    }
    /// Reconcile only screen size after the shared transaction finishes.
    pub fn resolution_readback(&mut self, size: (u32, u32), save: bool) {
        self.select_resolution(size);
        if save {
            if let Some(i) = self.saved.resolutions.iter().position(|r| *r == size) {
                self.saved.resolution = i;
            }
        }
    }
    /// Commit the page's values to the shared store, with `size` as the window's size.
    fn persist_resolution(&mut self, size: (u32, u32)) -> Result<(), String> {
        let mut stored = Stored::from_settings(&self.current);
        stored.resolution = [size.0, size.1];
        self.saved = stored.decode(&self.current)?;
        write_shared(&self.saved);
        let _ = store::set_value("Display.Resolution", resolution_value(size));
        Ok(())
    }
    /// Only the three immediately applied sliders reach this edge.
    pub fn preview<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        draft: &ClassicSettings,
    ) -> Result<(), String> {
        self.current = preview_values(&self.current, draft);
        self.camera_value = Some(camera_stiffness(draft.camera_stiffness));
        self.sync(cx)
    }
    pub fn defaults<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        draft: ClassicSettings,
    ) -> Result<Vec<UiRequest>, String> {
        self.preview(cx, &draft)?;
        Ok(vec![])
    }
    pub fn reset<S: Shell>(&mut self, cx: &mut Cx<'_, S>) -> Result<Vec<UiRequest>, String> {
        self.current = preview_values(&self.current, &self.saved);
        // A zero saved camera digit deliberately leaves the live stiffness unchanged.
        if self.saved.camera_stiffness > 0.0 {
            self.camera_value = Some(camera_stiffness(self.saved.camera_stiffness));
        }
        self.sync(cx)?;
        Ok(vec![])
    }
    fn apply_values<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        settings: ClassicSettings,
        display: bool,
    ) -> Result<Vec<UiRequest>, String> {
        self.current = settings;
        if self.current.camera_stiffness > 0.0 {
            self.camera_value = Some(camera_stiffness(self.current.camera_stiffness));
        }
        self.current.sound_available = cx.audio_mut().is_some_and(|a| a.has_device());
        // The sound preferences go the shared way, as the caller sends what this returns.
        let mut left = if self.current.sound_available {
            sound_requests(&self.current)
        } else {
            vec![]
        };
        if display {
            if let Some((w, h)) = self.current.resolutions.get(self.current.resolution) {
                if (*w, *h) != (cx.config().width, cx.config().height) {
                    left.push(preference(
                        "Display.Resolution",
                        PrefValue::Int(((*w << 16) | *h) as i32),
                    ));
                }
            }
        }
        self.sync(cx)?;
        Ok(left)
    }
    /// Called before every frame: the render and camera preferences follow the page, sent the
    /// shared way when they change.
    pub fn sync<S: Shell>(&mut self, cx: &mut Cx<'_, S>) -> Result<(), String> {
        let requests = render_requests(&self.current, self.camera_value);
        if requests != self.sent {
            self.sent.clone_from(&requests);
            cx.queue(Vec::new(), requests);
        }
        Ok(())
    }
}
fn preview_values(current: &ClassicSettings, draft: &ClassicSettings) -> ClassicSettings {
    let mut live = current.clone();
    live.brightness = normalized(draft.brightness);
    live.performance = normalized(draft.performance);
    live.auto_degrade = draft.auto_degrade;
    live.camera_stiffness = normalized(draft.camera_stiffness);
    live
}
fn camera_stiffness(v: f32) -> f32 {
    (normalized(v) + 0.4) * 0.71428573
}

/// Whether Apply writes the Full Screen box: when it differs from the saved preference, which
/// the next start follows, or from the screen now. Alt+Enter changes only the screen now, so a box
/// that matches the screen can still differ from what is saved.
fn full_screen_changed(chosen: bool, saved: bool, shown: bool) -> bool {
    chosen != saved || chosen != shown
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn full_screen_off_is_saved_even_after_alt_enter_left_full_screen() {
        // Saved on, Alt+Enter left full screen, the box is cleared: the saved preference changes.
        assert!(full_screen_changed(false, true, false));
        // Nothing to write when all three agree.
        assert!(!full_screen_changed(false, false, false));
        assert!(!full_screen_changed(true, true, true));
        // Saved off, Alt+Enter went full screen, the box is cleared: the screen follows the box.
        assert!(full_screen_changed(false, false, true));
    }
    fn settings() -> ClassicSettings {
        ClassicSettings {
            sound_available: true,
            detail_available: true,
            hardware_acceleration: true,
            resolutions: vec![(800, 600), (1024, 768)],
            effects: true,
            ambient: true,
            interface: true,
            stereo: true,
            auto_degrade: true,
            effects_volume: 0.66,
            ambient_volume: 0.66,
            brightness: 0.5,
            performance: 0.5,
            camera_stiffness: 0.23,
            ..ClassicSettings::default()
        }
    }
    #[test]
    fn apply_packs_tenths_and_hundredths_without_rounding() {
        let mut s = settings();
        s.brightness = 0.579;
        s.texture_levels = [0, 1, 2, 3];
        let p = Stored::from_settings(&s);
        assert_eq!(p.sound, 0x6607);
        assert_eq!(p.graphics, 0x2539);
        assert_eq!(p.textures, 0x3210);
        let d = p.decode(&s).unwrap();
        assert_eq!(d.effects_volume, 0.6);
        assert_eq!(d.camera_stiffness, 0.2);
        assert_eq!(d.brightness, 0.57);
    }
    #[test]
    fn the_brightness_slider_s_middle_leaves_the_world_as_bright_as_the_other_interface_shows_it() {
        let mut s = settings();
        let mut prefs = RenderPreferences::default();
        let unchanged = prefs.screen_brightness;
        render_preferences(&s, &mut prefs);
        assert_eq!(prefs.screen_brightness, unchanged);
        s.brightness = 0.0;
        render_preferences(&s, &mut prefs);
        assert_eq!(prefs.screen_brightness, -1.0);
        s.brightness = 0.75;
        render_preferences(&s, &mut prefs);
        assert_eq!(prefs.screen_brightness, 0.5);
        for b in [-1.0, -0.2, 0.0, 0.5, 1.0] {
            assert!((brightness_of_slider(slider_of_brightness(b)) - b).abs() < 1e-6);
        }
        assert_eq!(slider_of_brightness(f32::NAN), 0.5);
    }
    #[test]
    fn endpoint_settings_do_not_survive_as_stale_capabilities() {
        let old = Stored::from_settings(&settings());
        let mut now = settings();
        now.sound_available = false;
        now.detail_available = false;
        let d = old.decode(&now).unwrap();
        assert!(!d.sound_available);
        assert!(!d.detail_available);
    }
    #[test]
    fn the_classic_graphics_settings_land_on_the_shared_render_preferences() {
        let mut s = settings();
        s.brightness = 1.5;
        s.performance = 1.0;
        s.auto_degrade = false;
        s.environment_detail = true;
        let mut prefs = RenderPreferences::default();
        let sizes = (
            prefs.landscape_texture_detail,
            prefs.environment_texture_detail,
        );
        render_preferences(&s, &mut prefs);
        assert!(prefs.environment_detail_textures);
        assert_eq!(prefs.screen_brightness, 1.0);
        assert!(!prefs.automatic_degrades);
        assert_eq!(prefs.graphics_performance, 1.0);
        // The texture sizes are the page's own rows, not the host's: it leaves them alone.
        assert_eq!(
            (
                prefs.landscape_texture_detail,
                prefs.environment_texture_detail
            ),
            sizes
        );
    }
    /// Behaviour: options.client-page.the-classic-page-sends-the-environment-detail-textures-as-set
    #[test]
    fn the_environment_detail_textures_box_reaches_the_scene_as_stored_and_the_sizes_are_not_sent()
    {
        store::init();
        assert!(store::set_value(
            "Render.BuildingDetailTextures",
            PrefValue::Bool(true)
        ));
        let mut capabilities = settings();
        capabilities.detail_available = false;
        let s = from_shared(&capabilities);
        assert!(s.environment_detail, "the page opens on the stored value");
        let sent = render_requests(&s, None);
        assert!(sent.contains(&UiRequest::SetPreference(
            "Render.BuildingDetailTextures",
            PrefValue::Bool(true)
        )));
        assert!(
            !sent.iter().any(|r| matches!(r, UiRequest::SetPreference(n, _)
                if *n == "Render.LandscapeTextureDetail" || *n == "Render.EnvironmentTextureDetail")),
            "the host never sends the texture sizes: the page's own rows do"
        );
    }
    #[test]
    fn a_ticked_sound_box_plays_that_sound() {
        let mut s = settings();
        s.effects = true;
        s.ambient = false;
        s.interface = true;
        let value = |wanted: &str| {
            sound_requests(&s).into_iter().find_map(|r| match r {
                UiRequest::SetPreference(name, PrefValue::Bool(v)) if name == wanted => Some(v),
                _ => None,
            })
        };
        // The game's "Disabled" preferences hold the enable: true plays the sound.
        assert_eq!(value("Sound.SoundDisabled"), Some(true));
        assert_eq!(value("Sound.AmbientSoundDisabled"), Some(false));
        assert_eq!(value("Sound.InterfaceSoundDisabled"), Some(true));
    }
    #[test]
    fn saved_resolution_is_matched_by_dimensions_after_mode_reordering() {
        let mut s = settings();
        s.resolution = 1;
        let p = Stored::from_settings(&s);
        s.resolutions.reverse();
        assert_eq!(p.decode(&s).unwrap().resolution, 0);
    }
    #[test]
    fn invalid_texture_nibble_and_version_are_rejected() {
        let mut p = Stored::from_settings(&settings());
        p.textures = 4;
        assert!(p.decode(&settings()).is_err());
        p.textures = 0;
        p.version = 2;
        assert!(p.decode(&settings()).is_err());
    }
    #[test]
    fn default_live_camera_value_is_point_four_five() {
        assert!((camera_stiffness(0.23) - 0.45).abs() < 0.000001);
    }
    #[test]
    fn preview_does_not_apply_uncommitted_sound_texture_or_resolution_choices() {
        let live = settings();
        let mut draft = live.clone();
        draft.effects = false;
        draft.effects_volume = 0.1;
        draft.texture_levels = [3; 4];
        draft.resolution = 1;
        draft.brightness = 0.9;
        draft.performance = 0.8;
        draft.camera_stiffness = 0.7;
        let actual = preview_values(&live, &draft);
        assert!(actual.effects);
        assert_eq!(actual.effects_volume, live.effects_volume);
        assert_eq!(actual.texture_levels, live.texture_levels);
        assert_eq!(actual.resolution, live.resolution);
        assert_eq!(actual.brightness, 0.9);
        assert_eq!(actual.performance, 0.8);
        assert_eq!(actual.camera_stiffness, 0.7);
    }
    /// Behaviour: presentation.settings.both-interfaces-edit-one-store
    #[test]
    fn the_page_opens_on_the_shared_store_and_commits_to_it() {
        store::init();
        // What the retail page set, in the shared store.
        assert!(store::set_value("Sound.SoundVolume", PrefValue::Float(0.4)));
        assert!(store::set_value(
            "Render.ScreenBrightness",
            PrefValue::Float(0.0)
        ));
        assert!(store::set_value("Camera.Stiffness", PrefValue::Float(0.45)));
        let mut capabilities = settings();
        capabilities.sound_available = false;
        let mut host = SettingsHost::load(capabilities).unwrap();
        let s = host.snapshot();
        assert_eq!(s.effects_volume, 0.4);
        assert_eq!(
            s.brightness, 0.5,
            "the slider's middle: the world's brightness as the other page left it"
        );
        assert!((s.camera_stiffness - 0.23).abs() < 0.001);
        assert!(
            !s.sound_available,
            "capabilities are the machine's, not the store's"
        );
        // What this page commits, the retail page reads.
        host.current.ambient_volume = 0.7;
        host.current.performance = 1.0;
        host.persist_resolution((800, 600)).unwrap();
        assert_eq!(
            store::inq_value("Sound.AmbientSoundVolume"),
            Some(PrefValue::Float(0.7))
        );
        assert_eq!(
            store::inq_value("Render.GraphicsPerformance"),
            Some(PrefValue::Float(1.0))
        );
    }
    /// Behaviour: presentation.settings.both-interfaces-edit-one-store
    #[test]
    fn a_settings_file_left_by_the_classic_interface_is_carried_into_the_store_once() {
        store::init();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join("dereth-classic-settings-tests")
            .join(format!("{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = settings();
        s.effects_volume = 0.3;
        std::fs::write(
            &path,
            serde_json::to_vec(&Stored::from_settings(&s)).unwrap(),
        )
        .unwrap();
        assert!(migrate_settings_file(&path, &settings()).unwrap() >= 1);
        assert_eq!(
            store::inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(0.3)),
            "the setting the player moved"
        );
        assert_eq!(
            store::inq_value("Render.ScreenBrightness"),
            Some(PrefValue::Float(0.0)),
            "a setting left at the page's own first value does not override the store"
        );
        assert!(!path.exists(), "the file is gone");
        assert_eq!(migrate_settings_file(&path, &settings()).unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// Behaviour: presentation.resolution.shared-transaction
    #[test]
    fn completed_size_readback_preserves_unsaved_snapshot_and_other_values() {
        store::init();
        let mut host = SettingsHost::load(settings()).unwrap();
        host.current.resolution = 0;
        host.saved.resolution = 0;
        host.current.brightness = 0.8;
        let saved_brightness = host.saved.brightness;
        host.resolution_readback((1024, 768), false);
        assert_eq!(host.current.resolution, 1);
        assert_eq!(host.saved.resolution, 0);
        assert_eq!(host.current.brightness, 0.8);
        assert_eq!(host.saved.brightness, saved_brightness);
        host.resolution_readback((1024, 768), true);
        assert_eq!(host.saved.resolution, 1);
        assert_eq!(host.saved.brightness, saved_brightness);
    }
}
