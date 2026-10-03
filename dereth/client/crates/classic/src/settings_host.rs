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
        s.landscape_detail = self.detail[0] && s.detail_available;
        s.environment_detail = self.detail[1] && s.detail_available;
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
/// The classic Client page's settings, carried onto the shared scene's render preferences.
///
/// The classic page has four texture sliders (landscape, clip-mapped, colour and indexed images),
/// each with four steps from full size to a sixteenth. The shared scene has two: landscape, and one
/// level for every other image. So the landscape slider sets the landscape level, the colour slider
/// sets the other one, and the last step is an eighth (the smallest the shared scene makes). The
/// detail-texture checkbox for buildings and environment sets the shared one; the shared scene draws
/// no landscape detail texture, so that checkbox changes nothing. Brightness sets the screen's gamma
/// (the classic page raised the ambient light and the viewer's own light instead) on the other
/// interface's scale, [`brightness_of_slider`], so the slider's middle leaves the picture as it is;
/// and the performance slider sets the degrade bias when automatic degrading is off.
pub fn render_preferences(s: &ClassicSettings, prefs: &mut RenderPreferences) {
    prefs.landscape_texture_detail = u32::from(s.texture_levels[0].min(3)) + 1;
    prefs.environment_texture_detail = if s.environment_very_high {
        0
    } else {
        u32::from(s.texture_levels[2].min(3)) + 1
    };
    prefs.environment_detail_textures = s.detail_available && s.environment_detail;
    prefs.screen_brightness = brightness_of_slider(s.brightness);
    prefs.automatic_degrades = s.auto_degrade;
    prefs.graphics_performance = 1.0 - 2.0 * normalized(s.performance);
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
            names::LANDSCAPE_TEXTURE_DETAIL,
            PrefValue::Int(i32::try_from(prefs.landscape_texture_detail).unwrap_or(1)),
        ),
        preference(
            names::ENVIRONMENT_TEXTURE_DETAIL,
            PrefValue::Int(i32::try_from(prefs.environment_texture_detail).unwrap_or(1)),
        ),
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
    if let Some(v) = int_of(names::LANDSCAPE_TEXTURE_DETAIL) {
        s.texture_levels[0] = level_of(v);
    }
    if let Some(v) = int_of(names::ENVIRONMENT_TEXTURE_DETAIL) {
        s.environment_very_high = v == 0;
        s.texture_levels[2] = level_of(v);
    }
    if let Some(v) = bool_of(names::BUILDING_DETAIL_TEXTURES) {
        s.environment_detail = v && s.detail_available;
    }
    if let Some(v) = float_of(names::SCREEN_BRIGHTNESS) {
        s.brightness = slider_of_brightness(v);
    }
    if let Some(v) = bool_of(names::AUTOMATIC_DEGRADES) {
        s.auto_degrade = v;
    }
    if let Some(v) = float_of(names::GRAPHICS_PERFORMANCE) {
        s.performance = normalized((1.0 - v) / 2.0);
    }
    if let Some(v) = float_of("Camera.Stiffness") {
        s.camera_stiffness = normalized(v / 0.714_285_73 - 0.4);
    }
    if let Some(v) = bool_of("Display.FullScreen") {
        s.full_screen = v;
    }
    s
}

/// A texture-detail preference (1 full size .. 4 an eighth) as the page's step (0 .. 3).
fn level_of(v: i32) -> u8 {
    u8::try_from((v - 1).clamp(0, 3)).unwrap_or(0)
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
    let before = shared_values(&Stored::from_settings(defaults).decode(defaults)?);
    let mut after = shared_values(&file);
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

/// A resolution change first offers a test; the test's acceptance dialog times out after 15
/// seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResolutionStage {
    OfferTest,
    Applying { test: bool },
    Testing { deadline: f64 },
    Reverting,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolutionChange {
    pub previous: (u32, u32),
    pub target: (u32, u32),
    pub stage: ResolutionStage,
    persist: bool,
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
    pending_resolution: Option<ResolutionChange>,
    /// When the window was asked for the size now awaited (see [`Self::resolution_report_overdue`]).
    awaiting_since: Option<f64>,
    /// The render and camera preferences last sent, so an unchanged frame sends none.
    sent: Vec<UiRequest>,
}
/// How long a requested window size may go unanswered before the size the window has is taken as
/// the answer.
const RESIZE_REPORT_SECONDS: f64 = 2.0;
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
            pending_resolution: None,
            awaiting_since: None,
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
        let mut effective = packed.decode(&self.current)?;
        // Very High is not one of the packed steps.
        effective.environment_very_high = settings.environment_very_high;
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
            self.persist_resolution(previous)?;
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
        } else {
            self.stage_resolution(previous, target, save);
        }
        Ok(left)
    }
    /// A size choice already under way is left to finish; the other applied settings still apply.
    fn stage_resolution(&mut self, previous: (u32, u32), target: (u32, u32), persist: bool) {
        if self.pending_resolution.is_some() {
            return;
        }
        self.pending_resolution = (previous != target).then_some(ResolutionChange {
            previous,
            target,
            stage: ResolutionStage::OfferTest,
            persist,
        });
    }
    pub fn pending_resolution(&self) -> Option<ResolutionChange> {
        self.pending_resolution
    }
    fn select_resolution(&mut self, size: (u32, u32)) {
        if let Some(i) = self.current.resolutions.iter().position(|r| *r == size) {
            self.current.resolution = i;
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
    /// First dialog: Yes tests; No applies permanently. Second: Yes keeps; No reverts.
    pub fn answer_resolution(&mut self, yes: bool) -> Result<Vec<UiRequest>, String> {
        let Some(mut pending) = self.pending_resolution else {
            return Ok(vec![]);
        };
        match pending.stage {
            ResolutionStage::OfferTest => {
                pending.stage = ResolutionStage::Applying { test: yes };
                self.pending_resolution = Some(pending);
                Ok(vec![resolution_request(pending.target)])
            }
            ResolutionStage::Testing { .. } if yes => {
                if pending.persist {
                    self.persist_resolution(pending.target)?;
                }
                self.select_resolution(pending.target);
                self.pending_resolution = None;
                Ok(vec![])
            }
            ResolutionStage::Testing { .. } => {
                pending.stage = ResolutionStage::Reverting;
                self.pending_resolution = Some(pending);
                Ok(vec![resolution_request(pending.previous)])
            }
            _ => Ok(vec![]),
        }
    }
    /// Called only after the native resize reports its actual result, never on queueing.
    pub fn resolution_applied(&mut self, success: bool, now: f64) -> Result<(), String> {
        let Some(mut pending) = self.pending_resolution else {
            return Ok(());
        };
        match pending.stage {
            ResolutionStage::Applying { test } => {
                if !success {
                    self.select_resolution(pending.previous);
                    self.pending_resolution = None;
                } else {
                    self.select_resolution(pending.target);
                    if test {
                        pending.stage = ResolutionStage::Testing {
                            deadline: now + 15.0,
                        };
                        self.pending_resolution = Some(pending);
                    } else {
                        if pending.persist {
                            self.persist_resolution(pending.target)?;
                        }
                        self.pending_resolution = None;
                    }
                }
            }
            ResolutionStage::Reverting => {
                if !success {
                    self.select_resolution(pending.target);
                    self.pending_resolution = None;
                    return Err("Unable to restore the previous screen size!".into());
                }
                self.select_resolution(pending.previous);
                self.pending_resolution = None;
            }
            _ => {}
        }
        Ok(())
    }
    /// The size the window was asked for and has not yet reported.
    pub fn awaited_size(&self) -> Option<(u32, u32)> {
        match self.pending_resolution?.stage {
            ResolutionStage::Applying { .. } => self.pending_resolution.map(|p| p.target),
            ResolutionStage::Reverting => self.pending_resolution.map(|p| p.previous),
            _ => None,
        }
    }
    /// Whether a requested size has gone unanswered too long. A window that is full screen, or a
    /// size the display refuses outright, never reports a resize; the choice must still end.
    pub fn resolution_report_overdue(&mut self, now: f64) -> bool {
        if self.awaited_size().is_none() {
            self.awaiting_since = None;
            return false;
        }
        let since = *self.awaiting_since.get_or_insert(now);
        now - since >= RESIZE_REPORT_SECONDS
    }
    pub fn resolution_expired(&self, now: f64) -> bool {
        matches!(self.pending_resolution.map(|p| p.stage),
            Some(ResolutionStage::Testing { deadline }) if now >= deadline)
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
        s.texture_levels = [3, 2, 1, 0];
        s.brightness = 1.5;
        s.performance = 1.0;
        s.auto_degrade = false;
        s.detail_available = true;
        s.environment_detail = true;
        let mut prefs = RenderPreferences::default();
        render_preferences(&s, &mut prefs);
        assert_eq!(prefs.landscape_texture_detail, 4);
        assert_eq!(prefs.environment_texture_detail, 2);
        assert!(prefs.environment_detail_textures);
        assert_eq!(prefs.screen_brightness, 1.0);
        assert!(!prefs.automatic_degrades);
        assert_eq!(prefs.graphics_performance, -1.0);
        s.detail_available = false;
        render_preferences(&s, &mut prefs);
        assert!(!prefs.environment_detail_textures);
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
        assert!(store::set_value(
            "Render.LandscapeTextureDetail",
            PrefValue::Int(3)
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
        assert_eq!(s.texture_levels[0], 2);
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
            Some(PrefValue::Float(-1.0))
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
    fn resolution_host() -> SettingsHost {
        store::init();
        let mut host = SettingsHost::load(settings()).unwrap();
        host.current.brightness = 0.8;
        host.current.resolution = 1;
        host.persist_resolution((800, 600)).unwrap();
        host.stage_resolution((800, 600), (1024, 768), true);
        host
    }
    fn saved_size(_host: &SettingsHost) -> (u32, u32) {
        let Some(PrefValue::Float(b)) = store::inq_value("Render.ScreenBrightness") else {
            panic!("a brightness");
        };
        assert!(
            (b - 0.6).abs() < 1e-6,
            "other applied settings remain committed"
        );
        let Some(PrefValue::Int(v)) = store::inq_value("Display.Resolution") else {
            panic!("a saved size");
        };
        #[allow(clippy::cast_sign_loss)]
        let v = v as u32;
        (v >> 16, v & 0xFFFF)
    }
    #[test]
    fn tested_resolution_is_saved_only_after_explicit_acceptance() {
        let mut host = resolution_host();
        assert_eq!(saved_size(&host), (800, 600));
        assert_eq!(
            host.pending_resolution().unwrap().stage,
            ResolutionStage::OfferTest
        );
        assert_eq!(host.answer_resolution(true).unwrap().len(), 1);
        assert_eq!(saved_size(&host), (800, 600));
        host.resolution_applied(true, 20.0).unwrap();
        assert!(!host.resolution_expired(34.999));
        assert!(host.resolution_expired(35.0));
        assert_eq!(saved_size(&host), (800, 600));
        assert!(host.answer_resolution(true).unwrap().is_empty());
        assert_eq!(saved_size(&host), (1024, 768));
        assert!(host.pending_resolution().is_none());
    }
    #[test]
    fn failed_or_rejected_resolution_keeps_previous_size_and_other_applied_settings() {
        for reject_after_apply in [false, true] {
            let mut host = resolution_host();
            host.answer_resolution(true).unwrap();
            host.resolution_applied(reject_after_apply, 20.0).unwrap();
            if reject_after_apply {
                let requests = host.answer_resolution(false).unwrap();
                assert!(
                    matches!(requests.as_slice(), [UiRequest::SetPreference(name, PrefValue::Int(n))]
                    if *name == "Display.Resolution" && *n == ((800 << 16) | 600))
                );
                assert_eq!(
                    host.pending_resolution().unwrap().stage,
                    ResolutionStage::Reverting
                );
                host.resolution_applied(true, 21.0).unwrap();
            }
            assert!(host.pending_resolution().is_none());
            assert_eq!(host.snapshot().resolution, 0);
            assert_eq!(saved_size(&host), (800, 600));
        }
    }
    #[test]
    fn applying_again_while_a_size_choice_is_pending_keeps_that_choice() {
        let mut host = resolution_host();
        host.answer_resolution(true).unwrap();
        host.stage_resolution((1024, 768), (800, 600), true);
        assert_eq!(
            host.pending_resolution().unwrap().stage,
            ResolutionStage::Applying { test: true }
        );
        assert_eq!(host.pending_resolution().unwrap().target, (1024, 768));
    }
    #[test]
    fn an_unanswered_resize_is_overdue_after_two_seconds() {
        let mut host = resolution_host();
        assert!(!host.resolution_report_overdue(10.0), "nothing asked yet");
        host.answer_resolution(false).unwrap();
        assert_eq!(host.awaited_size(), Some((1024, 768)));
        assert!(!host.resolution_report_overdue(10.0));
        assert!(!host.resolution_report_overdue(11.9));
        assert!(host.resolution_report_overdue(12.0));
        host.resolution_applied(false, 12.0).unwrap();
        assert!(!host.resolution_report_overdue(13.0));
        assert_eq!(host.awaited_size(), None);
    }
    #[test]
    fn declining_the_test_saves_only_after_successful_resize() {
        for success in [false, true] {
            let mut host = resolution_host();
            assert_eq!(host.answer_resolution(false).unwrap().len(), 1);
            assert_eq!(
                host.pending_resolution().unwrap().stage,
                ResolutionStage::Applying { test: false }
            );
            assert_eq!(saved_size(&host), (800, 600));
            host.resolution_applied(success, 20.0).unwrap();
            assert_eq!(
                saved_size(&host),
                if success { (1024, 768) } else { (800, 600) }
            );
            assert!(host.pending_resolution().is_none());
        }
    }
}
