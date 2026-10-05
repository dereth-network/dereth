//! The classic settings pages carried onto the host. UI values are normalized.
//!
//! **One store.** Every row of the Client page is a shared preference, the same one the
//! modern interface's Client Options page edits: the page opens on the store's values and Apply
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
mod migration;
pub use migration::migrate_settings_file;
pub(crate) use migration::Migration;
#[cfg(test)]
use migration::Stored;
#[cfg(test)]
use std::path::Path;

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
/// The live page uses the same finite, truncated steps as a saved settings value.
fn quantize(settings: &ClassicSettings, capabilities: &ClassicSettings) -> ClassicSettings {
    let size = settings
        .resolutions
        .get(settings.resolution)
        .copied()
        .unwrap_or((800, 600));
    quantize_at_size(settings, capabilities, size)
}

fn quantize_at_size(
    settings: &ClassicSettings,
    capabilities: &ClassicSettings,
    size: (u32, u32),
) -> ClassicSettings {
    let mut out = capabilities.clone();
    out.stereo = settings.stereo;
    out.effects = settings.effects;
    out.ambient = settings.ambient;
    out.interface = settings.interface;
    out.auto_degrade = settings.auto_degrade;
    out.effects_volume = digit(settings.effects_volume, 10.0) as f32 / 10.0;
    out.ambient_volume = digit(settings.ambient_volume, 10.0) as f32 / 10.0;
    out.brightness = digit(settings.brightness, 100.0) as f32 / 100.0;
    out.performance = digit(settings.performance, 10.0) as f32 / 10.0;
    out.camera_stiffness = digit(settings.camera_stiffness, 10.0) as f32 / 10.0;
    out.texture_levels = settings.texture_levels.map(|n| n.min(3));
    out.landscape_detail = settings.landscape_detail;
    out.environment_detail = settings.environment_detail;
    if let Some(i) = out.resolutions.iter().position(|r| *r == size) {
        out.resolution = i;
    }
    out
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
        out.push(preference(
            dereth_client_contract::options::names::CAMERA_STIFFNESS,
            PrefValue::Float(value),
        ));
    }
    out
}
/// The sound page's values as the game's sound preferences. The three "Disabled" preferences
/// are enables despite their names: true means the sound plays.
fn sound_requests(s: &ClassicSettings) -> Vec<UiRequest> {
    vec![
        preference(
            dereth_client_contract::options::names::SOUND_DISABLED,
            PrefValue::Bool(s.effects),
        ),
        preference(
            dereth_client_contract::options::names::AMBIENT_SOUND_DISABLED,
            PrefValue::Bool(s.ambient),
        ),
        preference(
            dereth_client_contract::options::names::INTERFACE_SOUND_DISABLED,
            PrefValue::Bool(s.interface),
        ),
        preference(
            dereth_client_contract::options::names::SOUND_FEATURES,
            PrefValue::Int(i32::from(!s.stereo)),
        ),
        preference(
            dereth_client_contract::options::names::SOUND_VOLUME,
            PrefValue::Float(normalized(s.effects_volume)),
        ),
        preference(
            dereth_client_contract::options::names::AMBIENT_SOUND_VOLUME,
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
        out.push((
            dereth_client_contract::options::names::DISPLAY_RESOLUTION,
            resolution_value((*w, *h)),
        ));
    }
    out.push((
        dereth_client_contract::options::names::DISPLAY_FULL_SCREEN,
        PrefValue::Bool(s.full_screen),
    ));
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
    if let Some(v) = bool_of(dereth_client_contract::options::names::SOUND_DISABLED) {
        s.effects = v;
    }
    if let Some(v) = bool_of(dereth_client_contract::options::names::AMBIENT_SOUND_DISABLED) {
        s.ambient = v;
    }
    if let Some(v) = bool_of(dereth_client_contract::options::names::INTERFACE_SOUND_DISABLED) {
        s.interface = v;
    }
    if let Some(v) = int_of(dereth_client_contract::options::names::SOUND_FEATURES) {
        s.stereo = v == 0;
    }
    if let Some(v) = float_of(dereth_client_contract::options::names::SOUND_VOLUME) {
        s.effects_volume = normalized(v);
    }
    if let Some(v) = float_of(dereth_client_contract::options::names::AMBIENT_SOUND_VOLUME) {
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
    if let Some(v) = float_of(dereth_client_contract::options::names::CAMERA_STIFFNESS) {
        s.camera_stiffness = normalized(v / 0.714_285_73 - 0.4);
    }
    if let Some(v) = bool_of(dereth_client_contract::options::names::DISPLAY_FULL_SCREEN) {
        s.full_screen = v;
    }
    s
}

fn resolution_value(size: (u32, u32)) -> PrefValue {
    PrefValue::Int(((size.0 << 16) | size.1) as i32)
}

fn resolution_request(size: (u32, u32)) -> UiRequest {
    preference(
        dereth_client_contract::options::names::DISPLAY_RESOLUTION,
        resolution_value(size),
    )
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
    /// Apply quantizes the page to its tenths/hundredths before sending the values.
    /// Any returned display request must enter the host's normal UiRequest dispatcher.
    pub fn apply<S: Shell>(
        &mut self,
        cx: &mut Cx<'_, S>,
        settings: ClassicSettings,
        save: bool,
    ) -> Result<Vec<UiRequest>, String> {
        let effective = quantize(&settings, &self.current);
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
                dereth_client_contract::options::names::DISPLAY_FULL_SCREEN,
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
        self.saved = quantize_at_size(&self.current, &self.current, size);
        write_shared(&self.saved);
        let _ = store::set_value(
            dereth_client_contract::options::names::DISPLAY_RESOLUTION,
            resolution_value(size),
        );
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
                        dereth_client_contract::options::names::DISPLAY_RESOLUTION,
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
    /// Behaviour: presentation.settings.both-interfaces-edit-one-store
    #[test]
    fn quantization_keeps_capabilities_and_matches_the_saved_numeric_steps() {
        let mut capabilities = settings();
        capabilities.sound_available = false;
        capabilities.resolution = 1;
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.0,
            -0.0,
            0.579,
            1.0,
            2.0,
        ] {
            let mut input = settings();
            input.effects_volume = value;
            input.ambient_volume = value;
            input.brightness = value;
            input.performance = value;
            input.camera_stiffness = value;
            input.texture_levels = [0, 2, 4, u8::MAX];
            input.resolutions = vec![(1234, 789)];
            let actual = quantize(&input, &capabilities);
            let decoded = Stored::from_settings(&input).decode(&capabilities).unwrap();
            assert_eq!(actual, decoded);
            assert!(!actual.sound_available);
            assert_eq!(actual.resolution, 1);
            assert_eq!(actual.texture_levels, [0, 2, 3, 3]);
            if !value.is_finite() {
                assert_eq!(actual.effects_volume, 0.0);
                assert_eq!(actual.brightness, 0.0);
                assert_eq!(actual.camera_stiffness, 0.0);
            }
        }
    }

    /// Behaviour: presentation.settings.both-interfaces-edit-one-store
    #[test]
    fn migration_applies_bits_before_defaults_and_json_after_them_then_retires_only_empty_folders()
    {
        store::init();
        let mut scratch = dereth_dat::testing::ScratchDir::new("classic-migration-phases").unwrap();
        let folder = scratch.path().join("classic");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("classic-options"), b" 10\n").unwrap();
        let mut old = settings();
        old.effects_volume = 0.3;
        std::fs::write(
            folder.join("settings.json"),
            serde_json::to_vec(&Stored::from_settings(&old)).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.join("kept"), b"unrelated contents").unwrap();
        let migration = Migration::new(&folder);
        migration.before_defaults();
        assert_eq!(
            store::inq_value("UI.Classic.InvertMouseLook"),
            Some(PrefValue::Bool(true))
        );
        assert_eq!(
            store::inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(1.0))
        );
        assert!(!folder.join("classic-options").exists());
        assert!(folder.join("settings.json").exists());
        migration.after_defaults(&settings());
        assert_eq!(
            store::inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(0.3))
        );
        assert!(!folder.join("settings.json").exists());
        assert_eq!(
            std::fs::read(folder.join("kept")).unwrap(),
            b"unrelated contents"
        );
        std::fs::remove_file(folder.join("kept")).unwrap();
        migration.after_defaults(&settings());
        assert!(!folder.exists());
        scratch.cleanup().unwrap();
    }

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
        let d = quantize(&s, &s);
        assert_eq!(d, p.decode(&s).unwrap());
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
        let input = s.clone();
        s.resolutions.reverse();
        assert_eq!(p.decode(&s).unwrap().resolution, 0);
        assert_eq!(quantize(&input, &s).resolution, 0);
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
        let mut scratch = dereth_dat::testing::ScratchDir::new("classic-settings").unwrap();
        let dir = scratch.path();
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
        scratch.cleanup().unwrap();
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
    /// Behaviour: presentation.settings.both-interfaces-edit-one-store
    #[test]
    fn hosted_settings_migration_keeps_unreadable_or_invalid_files_and_reports_delete_failure() {
        use dereth_client_runtime::platform::files::{self, FileHost};
        use std::{cell::RefCell, io};
        #[derive(Default)]
        struct Memory {
            bytes: Option<Vec<u8>>,
            read_error: bool,
            delete_error: bool,
            deletes: usize,
        }
        thread_local! { static MEMORY: RefCell<Memory> = RefCell::new(Memory::default()); }
        const HOST: FileHost = FileHost {
            is_file: |_| MEMORY.with(|m| m.borrow().bytes.is_some()),
            read: |_| {
                MEMORY.with(|m| {
                    let m = m.borrow();
                    if m.read_error {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    m.bytes
                        .clone()
                        .ok_or_else(|| io::ErrorKind::NotFound.into())
                })
            },
            remove_file: |_| {
                MEMORY.with(|m| {
                    let mut m = m.borrow_mut();
                    m.deletes += 1;
                    if m.delete_error {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    m.bytes = None;
                    Ok(())
                })
            },
            ..files::DISK
        };
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                files::install(files::DISK);
            }
        }
        files::install(HOST);
        let _restore = Restore;
        store::init();
        let path = Path::new("/virtual/settings.json");
        assert_eq!(migrate_settings_file(path, &settings()).unwrap(), 0);
        MEMORY.with(|m| {
            m.borrow_mut().bytes = Some(b"not json".to_vec());
        });
        assert!(migrate_settings_file(path, &settings()).is_err());
        MEMORY.with(|m| assert_eq!(m.borrow().deletes, 0));
        let mut s = settings();
        s.effects_volume = 0.3;
        MEMORY.with(|m| {
            let mut m = m.borrow_mut();
            m.bytes = Some(serde_json::to_vec(&Stored::from_settings(&s)).unwrap());
            m.read_error = true;
        });
        assert!(migrate_settings_file(path, &settings()).is_err());
        MEMORY.with(|m| {
            let mut m = m.borrow_mut();
            assert_eq!(m.deletes, 0);
            m.read_error = false;
            m.delete_error = true;
        });
        assert!(migrate_settings_file(path, &settings()).is_err());
        assert_eq!(
            store::inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(0.3))
        );
        MEMORY.with(|m| {
            let mut m = m.borrow_mut();
            assert!(m.bytes.is_some());
            assert_eq!(m.deletes, 1);
            m.delete_error = false;
        });
        assert!(migrate_settings_file(path, &settings()).unwrap() > 0);
        assert_eq!(migrate_settings_file(path, &settings()).unwrap(), 0);
        MEMORY.with(|m| {
            assert!(m.borrow().bytes.is_none());
            assert_eq!(m.borrow().deletes, 2);
        });
    }
}
